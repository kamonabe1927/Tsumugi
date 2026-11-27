import contextlib
import gc
import io
import json
import os
import socket
import sys
import unittest
import urllib.error
import warnings
from unittest import mock

sys.path.insert(0, os.path.dirname(__file__))

import jev_category as jc  # noqa: E402

ISSUE = {"number": 544, "title": "fuel の上限", "body": "x" * 5000}
GOOD = {k: {"noul": 0.5} for k in jc.CATEGORIES} | {"budget": {"noul": 0.91}}


def fetch_ok(repo, number):
    return ISSUE


def record(lines):
    head = "<!-- jev-category "
    assert lines[1].startswith(head) and lines[1].endswith(" -->")
    return json.loads(lines[1][len(head):-4])


class FakeResponse:
    def __init__(self, payload):
        self.payload = payload

    def read(self):
        return self.payload

    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False


class FormatTest(unittest.TestCase):
    def test_success_lines(self):
        lines = jc.format_lines(544, jc.parse_answers(GOOD), 4.23, None)
        self.assertEqual(len(lines), 2)
        self.assertTrue(lines[0].startswith(f"Jev のカテゴリ（参考・{jc.MODEL}・{jc.QUESTIONS_VERSION}）: budget 0.91 ／ authority 0.50"))
        self.assertTrue(lines[0].endswith("loop 0.50"))
        r = record(lines)
        self.assertEqual((r["model"], r["questions"], r["issue"], r["seconds"], r["error"]),
                         ("jev-1.14.0", "category-v1", 544, 4.2, None))
        self.assertEqual(r["values"]["budget"], 0.91)

    def test_failure_lines(self):
        lines = jc.format_lines(544, None, None, "HTTP 403")
        self.assertEqual(lines[0], "Jev のカテゴリ（参考）: 取得できず（HTTP 403）")
        self.assertEqual(record(lines)["error"], "HTTP 403")
        self.assertIsNone(record(lines)["values"])

    def test_state_has_only_title_and_body_head(self):
        s = jc.make_state(ISSUE | {"comments": 3, "labels": ["x"]})
        self.assertEqual(set(s["issue"]), {"タイトル", "本文"})
        self.assertEqual(len(s["issue"]["本文"]), jc.BODY_LIMIT)
        self.assertEqual(jc.make_state({"title": None, "body": None})["issue"]["本文"], "")

    def test_questions_are_seven_noul(self):
        self.assertEqual(jc.CATEGORIES, ["budget", "authority", "parity", "embedding", "semantics", "build", "loop"])
        self.assertTrue(all(q["type"] == "noul" for q in jc.QUESTIONS.values()))
        text = json.dumps(jc.QUESTIONS, ensure_ascii=False)
        self.assertNotIn("#356", text)  # 「当たる例」は渡さない

    def test_parse_answers_rejects_bad_shapes(self):
        for bad in (None, [], {}, GOOD | {"loop": {"noul": 1.5}}, GOOD | {"loop": {"noul": "0.3"}},
                    GOOD | {"loop": {"noul": True}}, GOOD | {"loop": 0.3}):
            with self.assertRaises(ValueError):
                jc.parse_answers(bad)


class ClassifyTest(unittest.TestCase):
    """どの失敗でも、例外を外に出さずに「取得できず」の行になる。"""

    def run_one(self, caller, fetcher=fetch_ok):
        blocks = jc.classify([544], "r", caller=caller, fetcher=fetcher)
        self.assertEqual(len(blocks), 1)
        return blocks[0]

    def test_success_one_call_per_issue(self):
        calls = []

        def caller(state, questions, timeout):
            calls.append((state, timeout))
            return GOOD
        lines = self.run_one(caller)
        self.assertEqual(len(calls), 1)
        self.assertLessEqual(calls[0][1], jc.TIMEOUT)
        self.assertIsNone(record(lines)["error"])

    def test_http_error(self):
        def caller(state, questions, timeout):
            raise RuntimeError("HTTP 403")
        self.assertEqual(self.run_one(caller)[0], "Jev のカテゴリ（参考）: 取得できず（HTTP 403）")

    def test_bad_shape(self):
        lines = self.run_one(lambda s, q, timeout: {"answers": "?"})
        self.assertEqual(lines[0], "Jev のカテゴリ（参考）: 取得できず（答えの形が違う）")

    def test_unexpected_exception(self):
        def caller(state, questions, timeout):
            raise ZeroDivisionError()
        self.assertIn("取得できず（ZeroDivisionError）", self.run_one(caller)[0])

    def test_fetch_failure(self):
        def fetcher(repo, n):
            raise RuntimeError("issue を読めず: CalledProcessError")
        self.assertIn("取得できず（issue を読めず", self.run_one(lambda *a, **k: GOOD, fetcher)[0])

    def test_budget(self):
        ticks = iter([0, 1, 2, 3, 5, jc.BUDGET + 1, jc.BUDGET + 2])
        blocks = jc.classify([1, 2], "r", caller=lambda *a, **k: GOOD, fetcher=fetch_ok, clock=lambda: next(ticks))
        self.assertIsNone(record(blocks[0])["error"])
        self.assertEqual(blocks[1][0], "Jev のカテゴリ（参考）: 取得できず（時間の上限）")

    def test_call_timeout_is_capped_by_remaining_budget(self):
        seen = []
        ticks = iter([0, jc.BUDGET - 10, jc.BUDGET - 10, jc.BUDGET - 9, jc.BUDGET - 8])
        jc.classify([1], "r", caller=lambda s, q, timeout: seen.append(timeout) or GOOD, fetcher=fetch_ok,
                    clock=lambda: next(ticks))
        self.assertEqual(seen, [10])


class CallTest(unittest.TestCase):
    """urllib の失敗を理由つきの RuntimeError にする（ネットワークには出ない）。"""

    def call_with(self, effect):
        with mock.patch("urllib.request.urlopen", side_effect=effect):
            return jc.call({}, jc.QUESTIONS, timeout=1)

    def test_http_error(self):
        err = urllib.error.HTTPError(jc.ENDPOINT, 403, "Forbidden", {}, None)
        with self.assertRaisesRegex(RuntimeError, "HTTP 403"):
            self.call_with(err)

    def test_timeout(self):
        for err in (socket.timeout("timed out"), TimeoutError(), urllib.error.URLError(socket.timeout())):
            with self.assertRaisesRegex(RuntimeError, "時間切れ"):
                self.call_with(err)

    def test_unreachable(self):
        with self.assertRaisesRegex(RuntimeError, "接続できず"):
            self.call_with(urllib.error.URLError(ConnectionRefusedError()))

    def test_http_error_is_closed(self):
        err = urllib.error.HTTPError(jc.ENDPOINT, 500, "Error", {}, io.BytesIO(b"body"))
        with self.assertRaisesRegex(RuntimeError, "HTTP 500"):
            self.call_with(err)
        self.assertTrue(err.fp is None or err.fp.closed)
        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")
            del err
            gc.collect()
        self.assertEqual([x for x in w if issubclass(x.category, ResourceWarning)], [])

    def test_bad_json(self):
        with self.assertRaisesRegex(RuntimeError, "答えの形が違う"):
            self.call_with(lambda *a, **k: FakeResponse(b"<html>"))

    def test_ok_and_key_header(self):
        seen = {}

        def urlopen(req, timeout):
            seen["auth"] = req.get_header("Authorization")
            seen["timeout"] = timeout
            seen["body"] = json.loads(req.data)
            return FakeResponse(json.dumps({"answers": GOOD}).encode())
        with mock.patch.dict(os.environ, {"TYPESAFE_API_KEY": "k"}), mock.patch("urllib.request.urlopen", urlopen):
            self.assertEqual(jc.call({"issue": {}}, jc.QUESTIONS, timeout=7), GOOD)
        self.assertEqual((seen["auth"], seen["timeout"]), ("Bearer k", 7))
        self.assertEqual((seen["body"]["model"], sorted(seen["body"]["questions"])), (jc.MODEL, sorted(jc.CATEGORIES)))

    def test_no_key_no_header(self):
        seen = {}

        def urlopen(req, timeout):
            seen["auth"] = req.get_header("Authorization")
            raise urllib.error.HTTPError(jc.ENDPOINT, 401, "Unauthorized", {}, None)
        env = {k: v for k, v in os.environ.items() if k != "TYPESAFE_API_KEY"}
        with mock.patch.dict(os.environ, env, clear=True), mock.patch("urllib.request.urlopen", urlopen):
            with self.assertRaisesRegex(RuntimeError, "HTTP 401"):
                jc.call({}, jc.QUESTIONS)
        self.assertIsNone(seen["auth"])


class MainTest(unittest.TestCase):
    """終了コードは何が起きても 0 で、取得できずの行が出る。"""

    def run_main(self, argv):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(io.StringIO()):
            code = jc.main(argv)
        return code, buf.getvalue()

    def test_no_key_and_unreachable(self):
        # 環境に依らない: キーを消し、urlopen は 403、gh は呼ばれたら失敗する（fetch_issue を差し替えるので呼ばれない）
        env = {k: v for k, v in os.environ.items() if k != "TYPESAFE_API_KEY"}
        err = urllib.error.HTTPError(jc.ENDPOINT, 403, "Forbidden", {}, None)
        fetched = []

        def fetcher(repo, n):
            fetched.append(n)
            return ISSUE
        with mock.patch.dict(os.environ, env, clear=True), mock.patch("urllib.request.urlopen", side_effect=err), \
                mock.patch("subprocess.run", side_effect=AssertionError("gh を呼んだ")), \
                mock.patch.object(jc, "fetch_issue", fetcher):
            code, out = self.run_main(["--number", "543", "--number", "544"])
        self.assertEqual(code, 0)
        self.assertEqual(fetched, [543, 544])  # main は差し替えた fetch_issue を使う（既定を定義時に束縛しない）
        self.assertEqual(out.count("取得できず（HTTP 403）"), 2)
        self.assertIn("=== #543\n", out)
        self.assertIn('"issue": 544', out)

    def test_gh_missing(self):
        with mock.patch("subprocess.run", side_effect=FileNotFoundError()), \
                mock.patch("urllib.request.urlopen", side_effect=AssertionError("ネットワークに出た")):
            code, out = self.run_main(["--number", "1"])
        self.assertEqual(code, 0)
        self.assertIn("取得できず（issue を読めず: FileNotFoundError）", out)

    def test_bad_args(self):
        code, out = self.run_main([])
        self.assertEqual(code, 0)
        self.assertIn("取得できず（引数を読めない）", out)

    def test_classify_crash(self):
        with mock.patch.object(jc, "classify", side_effect=MemoryError()):
            code, out = self.run_main(["--number", "9"])
        self.assertEqual(code, 0)
        self.assertIn("取得できず（MemoryError）", out)


if __name__ == "__main__":
    unittest.main()

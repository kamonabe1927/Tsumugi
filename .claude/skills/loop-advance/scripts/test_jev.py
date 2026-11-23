import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

import jev as j  # noqa: E402

LOOP = {"id": j.LOOP_ID, "login": "kamonabe1927-stack"}
OTHER = {"id": 1, "login": "someone"}

CAND = """🤖 Claude Code 判断の候補 D-12
前提: spec §3 と実装がずれている。
Q1. どちらを正にするか
A（推奨）: spec を正にする — 目標を保つ 根拠: 原則 5
B: 実装を正にする — 変更が小さい 根拠: 原則 8
マニフェストに反しうる選択肢: B（原則 5）
Q2. 置き場所
A: §3 に書く — 近い
B（推奨）: §4 に書く — 揃う
マニフェストに反しうる選択肢: なし"""


def c(user, body, at):
    return {"user": user, "body": body, "created_at": at}


class ParseTest(unittest.TestCase):
    def test_split_and_options(self):
        cands = j.split_candidates(CAND)
        self.assertEqual(len(cands), 1)
        self.assertEqual(cands[0]["target"], "D-12")
        self.assertIn("spec §3", cands[0]["pre"])
        self.assertEqual(sorted(cands[0]["qs"]), ["1", "2"])
        self.assertNotIn("マニフェストに反しうる", cands[0]["qs"]["1"])
        qtext, opts = j.options(cands[0]["qs"]["1"])
        self.assertTrue(qtext.startswith("Q1."))
        self.assertEqual([(L, rec) for L, rec, _ in opts], [("A", True), ("B", False)])

    def test_parse_picks(self):
        self.assertEqual(j.parse_picks("Q1=A, Q2=b"), {"1": "A", "2": "b"})
        self.assertEqual(j.parse_picks("なし"), {})

    def test_pick_candidate_only_loop_account_latest(self):
        old = CAND.replace("D-12", "D-11")
        comments = [
            c(OTHER, CAND.replace("D-12", "D-99"), "2026-11-23T03:00:00Z"),  # 別人のものは読まない
            c(LOOP, CAND, "2026-11-23T02:00:00Z"),
            c(LOOP, old, "2026-11-23T01:00:00Z"),
        ]
        self.assertEqual(j.pick_candidate(comments, None)["target"], "D-12")
        self.assertEqual(j.pick_candidate(comments, "D-11")["target"], "D-11")
        self.assertIsNone(j.pick_candidate([c(OTHER, CAND, "x")], None))


class EvaluateTest(unittest.TestCase):
    def fake(self, risky):
        def caller(state, questions):
            if "grounded" in questions:
                return {"grounded": {"noul": 0.5}, "kind": {"choice": "doc"}}
            return {"option_risky": {"noul": risky[state["この選択肢"][0]]}}
        return caller

    def test_values_and_warn(self):
        cand = j.split_candidates(CAND)[0]
        rows = j.evaluate(cand, {"1": "A"}, "m", caller=self.fake({"A": 0.1, "B": 0.7}))
        self.assertEqual(rows[0]["options"], {"A": 0.1, "B": 0.7})
        self.assertTrue(rows[0]["warn"])  # 裏付け 0.5 < 0.6
        lines = j.format_lines(rows)
        self.assertTrue(lines[1].startswith("⚠️ Q1 種類: 文書"))
        self.assertIn("A（採用） 0.10", lines[1])
        self.assertTrue(lines[-1].startswith("<!-- jev "))

    def test_failure_does_not_raise(self):
        def broken(state, questions):
            raise RuntimeError("HTTP 403")
        cand = j.split_candidates(CAND)[0]
        rows = j.evaluate(cand, {"1": "A", "3": "B"}, "m", caller=broken)
        self.assertEqual([r["error"] for r in rows], ["HTTP 403", "候補に Q が無い"])
        self.assertIn("Q1 取得できず（HTTP 403）", j.format_lines(rows))

    def test_budget(self):
        ticks = iter([0, j.BUDGET + 1, j.BUDGET + 2])
        cand = j.split_candidates(CAND)[0]
        rows = j.evaluate(cand, {"1": "A"}, "m", caller=self.fake({"A": 0, "B": 0}), clock=lambda: next(ticks))
        self.assertEqual(rows[0]["error"], "時間の上限")

    def test_whole_failure_line(self):
        self.assertEqual(j.format_lines([], "判断の候補が見つからない"),
                         [f"Jev（参考・{j.MODEL}・{j.QUESTIONS_VERSION}）: 取得できず（判断の候補が見つからない）"])

    def test_is_warn(self):
        self.assertFalse(j.is_warn(0.8, {"A": 0.49}, "A"))
        self.assertTrue(j.is_warn(0.8, {"A": 0.5}, "A"))
        self.assertFalse(j.is_warn(0.8, {"A": 0.1, "B": 0.9}, "A"))  # 採らなかった案の値では付けない


if __name__ == "__main__":
    unittest.main()

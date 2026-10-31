import os
import sys
import unittest
from datetime import datetime, timezone

sys.path.insert(0, os.path.dirname(__file__))

import state as s  # noqa: E402

OWNER = {"id": s.OWNER_ID, "login": "kamonabe1927"}
LOOP = {"id": s.LOOP_ID, "login": "kamonabe1927-stack"}
IMPOSTOR = {"id": 1, "login": "kamonabe"}
NOW = datetime(2026, 11, 1, 12, 0, tzinfo=timezone.utc)  # JST 21:00

REVIEW_OK = "🤖 Claude Code review（シニア）abc1234\n本証:\n結論: 指摘なし"
REVIEW_NG = "🤖 Claude Code review（シニア）abc1234\n結論: 指摘あり"
GATE_OK = "🤖 Claude Code manifesto-gate abc1234\n| x |\n判定: 適合"
GATE_NG = "🤖 Claude Code manifesto-gate abc1234\n判定: 疑いあり"
GATE_SUB = "🤖 Claude Code manifesto-gate abc1234（代替: Opus）\n判定: 適合"


def c(user, body, at):
    return {"user": user, "body": body, "created_at": at}


def pr(n=1, labels=("loop:impl",), head="2026-11-01T06:00:00Z", comments=()):
    return {"number": n, "title": "t", "labels": [{"name": x} for x in labels], "head": {"sha": "abc"},
            "head_committed_at": head, "comments": list(comments), "reviews": [], "review_comments": []}


def ok_comments():
    return [c(LOOP, REVIEW_OK, "2026-11-01T07:00:00Z"), c(LOOP, GATE_OK, "2026-11-01T07:10:00Z")]


class MergeCandidate(unittest.TestCase):
    def one(self, p, merged=()):
        return s.judge([p], [], list(merged), NOW)["open_loop_prs"][0]

    def test_all_conditions(self):
        self.assertTrue(self.one(pr(comments=ok_comments()))["merge_candidate"])

    def test_waits_three_hours(self):
        self.assertFalse(self.one(pr(head="2026-11-01T10:00:00Z", comments=[
            c(LOOP, REVIEW_OK, "2026-11-01T10:30:00Z"), c(LOOP, GATE_OK, "2026-11-01T10:40:00Z")]))["merge_candidate"])

    def test_revert_skips_wait_and_limit(self):
        merged = [{"merged_at": "2026-11-01T0%d:00:00Z" % i, "labels": [{"name": "loop:impl"}]} for i in range(4)]
        p = pr(labels=("loop:revert",), head="2026-11-01T11:00:00Z", comments=[
            c(LOOP, REVIEW_OK, "2026-11-01T11:10:00Z"), c(LOOP, GATE_OK, "2026-11-01T11:20:00Z")])
        self.assertTrue(self.one(p, merged)["merge_candidate"])

    def test_daily_limit(self):
        merged = [{"merged_at": "2026-11-01T0%d:00:00Z" % i, "labels": [{"name": "loop:impl"}]} for i in range(4)]
        self.assertFalse(self.one(pr(comments=ok_comments()), merged)["merge_candidate"])

    def test_review_before_head_does_not_count(self):
        p = pr(head="2026-11-01T07:30:00Z", comments=ok_comments())
        out = self.one(p)
        self.assertIsNone(out["review"])
        self.assertFalse(out["merge_candidate"])

    def test_findings_and_gate_ng(self):
        self.assertEqual(self.one(pr(comments=[c(LOOP, REVIEW_NG, "2026-11-01T07:00:00Z")]))["review"], "findings")
        out = self.one(pr(comments=[c(LOOP, REVIEW_OK, "2026-11-01T07:00:00Z"), c(LOOP, GATE_NG, "2026-11-01T07:10:00Z")]))
        self.assertEqual(out["gate"], "ng")
        self.assertFalse(out["merge_candidate"])

    def test_gate_substituted_mark(self):
        out = self.one(pr(comments=[c(LOOP, REVIEW_OK, "2026-11-01T07:00:00Z"), c(LOOP, GATE_SUB, "2026-11-01T07:10:00Z")]))
        self.assertTrue(out["gate_substituted"])
        self.assertTrue(out["merge_candidate"])

    def test_hold_and_release(self):
        held = ok_comments() + [c(OWNER, "保留: まって", "2026-11-01T06:30:00Z")]
        self.assertFalse(self.one(pr(comments=held))["merge_candidate"])
        released = held + [c(OWNER, "保留解除: OK", "2026-11-01T06:40:00Z")]
        self.assertFalse(self.one(pr(comments=released))["hold"])

    def test_owner_after_head_blocks(self):
        self.assertFalse(self.one(pr(comments=ok_comments() + [c(OWNER, "質問です", "2026-11-01T08:00:00Z")]))["merge_candidate"])

    def test_impostor_and_agent_prefix_are_not_owner(self):
        out = self.one(pr(comments=ok_comments() + [c(IMPOSTOR, "保留: x", "2026-11-01T08:00:00Z"),
                                                    c(OWNER, "🤖 Claude Code: 保留: x", "2026-11-01T08:01:00Z")]))
        self.assertFalse(out["hold"])
        self.assertTrue(out["merge_candidate"])

    def test_block_labels(self):
        self.assertFalse(self.one(pr(labels=("loop:impl", "loop:running"), comments=ok_comments()))["merge_candidate"])


class Issues(unittest.TestCase):
    def test_halt_and_triage(self):
        issues = [
            {"number": 1, "user": OWNER, "body": "止めて", "labels": [{"name": "loop:halt"}]},
            {"number": 2, "user": OWNER, "body": "要望", "labels": []},
            {"number": 3, "user": OWNER, "body": "済み", "labels": [{"name": "loop:triaged"}]},
            {"number": 4, "user": IMPOSTOR, "body": "x", "labels": []},
            {"number": 5, "user": OWNER, "body": "改訂して", "labels": [{"name": "manifesto-change"}]},
        ]
        out = s.judge([], issues, [], NOW)
        self.assertEqual(out["halt"], [1])
        self.assertEqual([i["number"] for i in out["issues_to_triage"]], [2, 5])
        self.assertTrue(out["issues_to_triage"][1]["manifesto_change"])

    def test_owner_comment_on_triaged_issue(self):
        iss = {"number": 3, "user": OWNER, "body": "x", "labels": [{"name": "loop:triaged"}],
               "comments": [c(LOOP, "🤖 Claude Code 仕分け", "2026-11-01T01:00:00Z"),
                            c(OWNER, "やっぱりこうして", "2026-11-01T02:00:00Z")]}
        self.assertEqual(s.judge([], [iss], [], NOW)["owner_writes_to_handle"], [{"where": "issue #3", "count": 1}])


class MergedToday(unittest.TestCase):
    def test_jst_boundary(self):
        merged = [
            {"merged_at": "2026-10-31T14:59:00Z", "labels": [{"name": "loop:impl"}]},  # JST 10/31 23:59
            {"merged_at": "2026-10-31T15:00:00Z", "labels": [{"name": "loop:impl"}]},  # JST 11/1 0:00
            {"merged_at": "2026-11-01T01:00:00Z", "labels": [{"name": "loop:revert"}]},
            {"merged_at": "2026-11-01T01:00:00Z", "labels": []},
        ]
        self.assertEqual(s.merged_today(merged, NOW), 1)


if __name__ == "__main__":
    unittest.main()

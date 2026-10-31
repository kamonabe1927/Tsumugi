import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

import frozen_guard as g  # noqa: E402

OWNER = {"id": g.OWNER_ID, "login": "kamonabe1927"}
LOOP = {"id": 348768222, "login": "kamonabe1927-stack"}
IMPOSTOR = {"id": 1, "login": "kamonabe"}


def labeled(name, actor, at="2026-11-01T10:00:00Z"):
    return {"event": "labeled", "label": {"name": name}, "actor": actor, "created_at": at}


class ParseFrozen(unittest.TestCase):
    def test_reads_repo_file(self):
        here = os.path.dirname(__file__)
        with open(os.path.join(here, "..", "frozen-paths.txt"), encoding="utf-8") as fp:
            rules = g.parse_frozen(fp.read())
        self.assertIn(("manifesto", "docs/manifesto.md"), rules)
        self.assertIn(("guard", ".github/**"), rules)
        self.assertIn(("guard", "docs/loop-guard.md"), rules)

    def test_rejects_bad_lines(self):
        with self.assertRaises(ValueError):
            g.parse_frozen("guard\n")
        with self.assertRaises(ValueError):
            g.parse_frozen("other x\n")
        with self.assertRaises(ValueError):
            g.parse_frozen("# only comments\n")


class Classify(unittest.TestCase):
    rules = [("guard", ".github/**"), ("guard", "CLAUDE.md"), ("manifesto", "docs/manifesto.md")]

    def test_prefix_and_exact(self):
        got = g.classify(
            [".github/workflows/ci.yml", ".github", ".githubx/a", "CLAUDE.md", "docs/CLAUDE.md",
             "docs/manifesto.md", "src/lib.rs"],
            self.rules,
        )
        self.assertEqual(got, {
            ".github/workflows/ci.yml": "guard",
            ".github": "guard",
            "CLAUDE.md": "guard",
            "docs/manifesto.md": "manifesto",
        })

    def test_wildcard(self):
        rules = [("guard", ".claude/settings*")]
        self.assertEqual(g.classify([".claude/settings.json", ".claude/settings.local.json", ".claude/x/settings.json"], rules),
                         {".claude/settings.json": "guard", ".claude/settings.local.json": "guard"})

    def test_rename_counts_both_sides(self):
        paths = g.changed_paths([{"filename": "docs/x.md", "previous_filename": "docs/manifesto.md"}])
        self.assertEqual(g.classify(paths, self.rules), {"docs/manifesto.md": "manifesto"})


class LabeledByOwner(unittest.TestCase):
    def test_owner_last(self):
        ev = [labeled("frozen-ok", LOOP), labeled("frozen-ok", OWNER)]
        self.assertTrue(g.labeled_by_owner("frozen-ok", ["frozen-ok"], ev))

    def test_loop_last(self):
        ev = [labeled("frozen-ok", OWNER), labeled("frozen-ok", LOOP)]
        self.assertFalse(g.labeled_by_owner("frozen-ok", ["frozen-ok"], ev))

    def test_impostor_login(self):
        self.assertFalse(g.labeled_by_owner("frozen-ok", ["frozen-ok"], [labeled("frozen-ok", IMPOSTOR)]))

    def test_label_before_push_is_stale(self):
        ev = [labeled("frozen-ok", OWNER, "2026-11-01T09:00:00Z")]
        self.assertFalse(g.labeled_by_owner("frozen-ok", ["frozen-ok"], ev, "2026-11-01T09:30:00Z"))
        self.assertTrue(g.labeled_by_owner("frozen-ok", ["frozen-ok"], ev, "2026-11-01T08:30:00Z"))

    def test_head_pushed_at(self):
        suites = [{"created_at": "2026-11-01T09:05:00Z"}, {"created_at": "2026-11-01T09:00:00Z"}]
        self.assertEqual(g.head_pushed_at(suites), "2026-11-01T09:00:00Z")
        self.assertIsNone(g.head_pushed_at([]))

    def test_label_not_present(self):
        self.assertFalse(g.labeled_by_owner("frozen-ok", [], [labeled("frozen-ok", OWNER)]))


class Decide(unittest.TestCase):
    def test_no_frozen(self):
        self.assertTrue(g.decide({}, False, [])[0])

    def test_guard_needs_frozen_ok(self):
        self.assertFalse(g.decide({"CLAUDE.md": "guard"}, False, [5])[0])
        self.assertTrue(g.decide({"CLAUDE.md": "guard"}, True, [])[0])

    def test_manifesto_via_issue(self):
        self.assertTrue(g.decide({"docs/manifesto.md": "manifesto"}, False, [7])[0])
        self.assertFalse(g.decide({"docs/manifesto.md": "manifesto"}, False, [])[0])

    def test_mixed_needs_frozen_ok(self):
        frozen = {"docs/manifesto.md": "manifesto", "CLAUDE.md": "guard"}
        self.assertFalse(g.decide(frozen, False, [7])[0])

    def test_links(self):
        self.assertEqual(g.linked_issue_numbers("Refs #12\ncloses #3, Fixes #12 #99, resolved #4"), [3, 4, 12])
        self.assertEqual(g.linked_issue_numbers("Refs #12"), [])
        self.assertEqual(g.linked_issue_numbers(None), [])


if __name__ == "__main__":
    unittest.main()

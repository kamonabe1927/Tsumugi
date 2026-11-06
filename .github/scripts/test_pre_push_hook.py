import json
import os
import subprocess
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

import pre_push_hook as h  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))


def bash(command):
    return {"tool_name": "Bash", "tool_input": {"command": command}}


def decide(command, branch="feature", paths=("src/lib.rs",), gate=None):
    calls = []

    def runner():
        calls.append(1)
        return gate

    reason = h.decide(bash(command), current_branch=branch,
                      paths=None if paths is None else list(paths), gate_runner=runner)
    return reason, len(calls)


class FindPushes(unittest.TestCase):
    def test_plain(self):
        self.assertEqual(h.find_pushes("git push -u origin feat"), [["-u", "origin", "feat"]])

    def test_chained_and_options(self):
        cmd = "cd /x && FOO=1 git -C /y -c a=b push origin HEAD; echo ok"
        self.assertEqual(h.find_pushes(cmd), [["origin", "HEAD"]])

    def test_not_push(self):
        self.assertEqual(h.find_pushes("git status && echo 'git push'"), [])
        self.assertEqual(h.find_pushes("git log --grep push"), [])


class ProtectedBranch(unittest.TestCase):
    def assertBlocked(self, command, branch="feature"):
        reason, gates = decide(command, branch=branch)
        self.assertIsNotNone(reason, command)
        self.assertIn("main", reason)
        self.assertEqual(gates, 0)

    def test_explicit_main(self):
        self.assertBlocked("git push origin main")
        self.assertBlocked("git push origin HEAD:main")
        self.assertBlocked("git push -f origin +feat:refs/heads/main")
        self.assertBlocked("git push origin --delete main")
        self.assertBlocked("git push --mirror origin")
        self.assertBlocked("git push --all origin")

    def test_implicit_main(self):
        self.assertBlocked("git push", branch="main")
        self.assertBlocked("git push -u origin HEAD", branch="main")

    def test_feature_branch_passes(self):
        reason, _ = decide("git push -u origin claude/x")
        self.assertIsNone(reason)
        reason, _ = decide("git push origin mainline")
        self.assertIsNone(reason)


class Gates(unittest.TestCase):
    def test_gate_failure_blocks(self):
        reason, gates = decide("git push -u origin feat", gate="`cargo fmt --check` が失敗した")
        self.assertEqual(gates, 1)
        self.assertIn("cargo fmt --check", reason)

    def test_docs_only_skips_gates(self):
        reason, gates = decide("git push -u origin feat", paths=("docs/a.md",))
        self.assertIsNone(reason)
        self.assertEqual(gates, 0)

    def test_unknown_diff_runs_gates(self):
        _, gates = decide("git push -u origin feat", paths=None)
        self.assertEqual(gates, 1)

    def test_cargo_files_count_as_rust(self):
        self.assertTrue(h.touches_rust(["Cargo.lock"]))
        self.assertTrue(h.touches_rust(["tests/a.rs"]))
        self.assertFalse(h.touches_rust(["docs/x.md", ".github/scripts/a.py"]))

    def test_other_tools_ignored(self):
        payload = {"tool_name": "Edit", "tool_input": {"file_path": "a"}}
        self.assertIsNone(h.decide(payload, current_branch="main", paths=None,
                                   gate_runner=lambda: "boom"))


class Wiring(unittest.TestCase):
    def test_settings_registers_hook(self):
        with open(os.path.join(ROOT, ".claude", "settings.json"), encoding="utf-8") as fp:
            settings = json.load(fp)
        entries = settings["hooks"]["PreToolUse"]
        commands = [hk["command"] for e in entries if e.get("matcher") == "Bash"
                    for hk in e["hooks"]]
        self.assertTrue(any(".github/scripts/pre_push_hook.py" in c for c in commands))

    def test_script_ignores_non_push(self):
        r = subprocess.run([sys.executable, os.path.join(HERE, "pre_push_hook.py")],
                           input=json.dumps(bash("ls")), capture_output=True, text=True)
        self.assertEqual(r.returncode, 0)

    def test_script_blocks_main_with_exit_2(self):
        r = subprocess.run([sys.executable, os.path.join(HERE, "pre_push_hook.py")],
                           input=json.dumps(bash("git push origin main")),
                           capture_output=True, text=True, cwd=ROOT)
        self.assertEqual(r.returncode, 2)
        self.assertIn("main", r.stderr)


if __name__ == "__main__":
    unittest.main()

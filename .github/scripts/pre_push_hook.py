"""Claude Code の PreToolUse フック: `git push` の直前に守りを掛ける。

`.claude/settings.json` から Bash ツールの呼び出しごとに呼ばれる。標準入力に
フックの JSON（`tool_name`・`tool_input.command`・`cwd`）が来る。

- コマンドに `git push` が無ければ何もしない（exit 0）。
- main へ向かう push は止める（CLAUDE.md「main へ直接 push しない」）。
- それ以外の push では、Rust の差分があれば CI と同じ `cargo fmt --check` と
  `cargo clippy --all-targets --all-features -- -D warnings` を先に走らせ、
  落ちたら push を止める（loop-runbook §14.2 の 1・2）。

止めるときは exit 2 で理由を標準エラーに書く。Claude Code はこれを Claude に返す。
最終的な守りは CI と frozen-guard であり、このフックは「その場で止める」ための前段である。
"""

import json
import os
import shlex
import subprocess
import sys

PROTECTED = "main"
SEPARATORS = {";", "&&", "||", "|", "&", "(", ")", "\n"}
# git のグローバルオプションのうち、次の 1 語を値に取るもの
GIT_OPTS_WITH_VALUE = {"-C", "-c", "--git-dir", "--work-tree", "--namespace"}
# push のオプションのうち、次の 1 語を値に取るもの
PUSH_OPTS_WITH_VALUE = {"--repo", "--receive-pack", "--exec", "-o", "--push-option"}
# Rust のゲートに関わる差分かどうか
RUST_SUFFIXES = (".rs",)
RUST_FILES = ("Cargo.toml", "Cargo.lock", "rust-toolchain", "rust-toolchain.toml",
              "clippy.toml", ".clippy.toml", "rustfmt.toml", ".rustfmt.toml")

GATES = [
    ["cargo", "fmt", "--check"],
    ["cargo", "clippy", "--all-targets", "--all-features", "--", "-D", "warnings"],
]


def split_commands(command):
    """シェルのコマンド列を、区切り記号ごとの語の並びに分ける。"""
    lexer = shlex.shlex(command, posix=True, punctuation_chars=";&|()")
    lexer.whitespace_split = True
    lexer.commenters = ""
    try:
        tokens = list(lexer)
    except ValueError:
        # 引用符が閉じていないなど。安全側に倒して空白で分ける
        tokens = command.split()
    parts, cur = [], []
    for tok in tokens:
        if tok in SEPARATORS:
            if cur:
                parts.append(cur)
            cur = []
        else:
            cur.append(tok)
    if cur:
        parts.append(cur)
    return parts


def push_args(words):
    """`git [opts] push ARGS...` なら ARGS を返す。push でなければ None。"""
    # 先頭の環境変数代入（FOO=bar git push）を飛ばす
    i = 0
    while i < len(words) and "=" in words[i] and not words[i].startswith("-"):
        i += 1
    if i >= len(words) or os.path.basename(words[i]) != "git":
        return None
    i += 1
    while i < len(words) and words[i].startswith("-"):
        if words[i] in GIT_OPTS_WITH_VALUE:
            i += 1
        i += 1
    if i < len(words) and words[i] == "push":
        return words[i + 1:]
    return None


def find_pushes(command):
    return [a for a in (push_args(w) for w in split_commands(command)) if a is not None]


def refspec_targets_protected(spec):
    spec = spec.lstrip("+")
    dst = spec.split(":", 1)[1] if ":" in spec else spec
    return dst in (PROTECTED, "refs/heads/" + PROTECTED)


def is_protected_push(args, current_branch):
    """push の引数が main を書き換えうるなら理由を返す。そうでなければ None。"""
    positional = []
    i = 0
    while i < len(args):
        a = args[i]
        if a in ("--mirror", "--all", "--branches"):
            return f"`git push {a}` は main も含めて push する"
        if a in ("--delete", "-d"):
            pass
        elif a in PUSH_OPTS_WITH_VALUE:
            i += 1
        elif a.startswith("-"):
            pass
        else:
            positional.append(a)
        i += 1
    refspecs = positional[1:]  # 先頭はリモート名
    for spec in refspecs:
        if refspec_targets_protected(spec):
            return f"refspec `{spec}` は main を書き換える"
        if spec == "HEAD" and current_branch == PROTECTED:
            return "いまのブランチが main なので `HEAD` は main への push になる"
    if not refspecs and current_branch == PROTECTED:
        return "いまのブランチが main なので、refspec を省いた push は main への push になる"
    return None


def git(cwd, *args):
    r = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None


def touches_rust(paths):
    for p in paths:
        name = p.rsplit("/", 1)[-1]
        if p.endswith(RUST_SUFFIXES) or name in RUST_FILES:
            return True
    return False


def changed_paths(cwd):
    """origin/main からの差分（コミット済み）。取れなければ None（ゲートを走らせる）。"""
    out = git(cwd, "diff", "--name-only", "origin/main...HEAD")
    if out is None:
        return None
    return [line for line in out.splitlines() if line]


def run_gates(cwd):
    for cmd in GATES:
        try:
            r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
        except FileNotFoundError:
            return f"`{cmd[0]}` が見つからない"
        if r.returncode != 0:
            out = (r.stdout + r.stderr).strip()
            tail = "\n".join(out.splitlines()[-40:])
            return f"`{' '.join(cmd)}` が失敗した:\n{tail}"
    return None


def decide(payload, *, current_branch, paths, gate_runner):
    """止める理由を返す。通すなら None。テストから副作用なしで呼べるようにしてある。"""
    if payload.get("tool_name") != "Bash":
        return None
    command = (payload.get("tool_input") or {}).get("command") or ""
    if "push" not in command:
        return None
    pushes = find_pushes(command)
    if not pushes:
        return None
    for args in pushes:
        reason = is_protected_push(args, current_branch)
        if reason:
            return (f"main への push は止めています（{reason}）。"
                    "ブランチを切って PR を出してください（CLAUDE.md「守ること」）。")
    if paths is not None and not touches_rust(paths):
        return None
    failure = gate_runner()
    if failure:
        return (f"push の前のゲートが落ちたので push を止めました。直してから push し直してください"
                f"（loop-runbook §14.2）。\n{failure}")
    return None


def main():
    try:
        payload = json.load(sys.stdin)
    except ValueError:
        return 0
    cwd = os.environ.get("CLAUDE_PROJECT_DIR") or payload.get("cwd") or os.getcwd()
    command = (payload.get("tool_input") or {}).get("command") or ""
    if payload.get("tool_name") != "Bash" or not find_pushes(command):
        return 0
    reason = decide(
        payload,
        current_branch=git(cwd, "rev-parse", "--abbrev-ref", "HEAD"),
        paths=changed_paths(cwd),
        gate_runner=lambda: run_gates(cwd),
    )
    if reason:
        print(reason, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())

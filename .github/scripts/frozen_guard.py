#!/usr/bin/env python3
"""凍結ガード: PR が凍結ファイルを変えていれば、kamonabe の許可があるかを確かめる。

規則の正本は docs/loop-guard.md §3。凍結するファイルの一覧は .github/frozen-paths.txt。
.github/workflows/frozen-guard.yml から pull_request_target で呼ばれ、main 側のこのスクリプトが動く。

環境変数:
    GITHUB_TOKEN  REPO  PR_NUMBER  HEAD_SHA  EVENT_ACTION
    FROZEN_FILE（省略時 .github/frozen-paths.txt）

結果は PR の最新 commit に commit status（context: Frozen guard (status)）として付け、
許可が無ければ終了コード 1 で終わる。
"""

from __future__ import annotations

import fnmatch
import json
import os
import re
import sys
import urllib.error
import urllib.request

OWNER_ID = 19951033  # kamonabe（login ではなく user id で見分ける。loop-guard §2）
STATUS_CONTEXT = "Frozen guard (status)"
FROZEN_OK = "frozen-ok"
MANIFESTO_CHANGE = "manifesto-change"
CLASSES = ("guard", "manifesto")
# マニフェストの改訂は 1 回限りの許可にするため、issue を閉じる参照（Closes など）だけを数える（loop-guard §3.1）
LINK_RE = re.compile(r"\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s+#(\d+)", re.I)


# ---------------------------------------------------------------- 判定（純粋関数）


def parse_frozen(text: str) -> list[tuple[str, str]]:
    """frozen-paths.txt を (区分, パターン) の一覧にする。書式の誤りは例外にする（甘く読まない）。"""
    rules = []
    for n, raw in enumerate(text.splitlines(), 1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) != 2 or parts[0] not in CLASSES:
            raise ValueError(f"frozen-paths.txt {n} 行目の書式が不正: {raw!r}")
        rules.append((parts[0], parts[1]))
    if not rules:
        raise ValueError("frozen-paths.txt に規則がありません")
    return rules


def matches(path: str, pattern: str) -> bool:
    if pattern.endswith("/**"):
        prefix = pattern[:-3]
        return path == prefix or path.startswith(prefix + "/")
    if any(ch in pattern for ch in "*?["):
        return fnmatch.fnmatchcase(path, pattern) and "/" not in path[len(pattern.split("*")[0]):]
    return path == pattern


def classify(paths: list[str], rules: list[tuple[str, str]]) -> dict[str, str]:
    """変更されたパスのうち凍結ファイルを {パス: 区分} にする。両方に当たれば guard を取る。"""
    out: dict[str, str] = {}
    for p in paths:
        hit = [c for c, pat in rules if matches(p, pat)]
        if "guard" in hit:
            out[p] = "guard"
        elif hit:
            out[p] = "manifesto"
    return out


def changed_paths(files: list[dict]) -> list[str]:
    """PR の files API の結果から、変更・追加・削除・改名の前後のパスをすべて取る。"""
    out = []
    for f in files:
        out.append(f["filename"])
        if f.get("previous_filename"):
            out.append(f["previous_filename"])
    return out


def labeled_by_owner(label: str, current_labels: list[str], events: list[dict], not_before: str | None = None) -> bool:
    """ラベルが今付いていて、最後にそのラベルを付けたのが kamonabe かどうか。

    not_before を渡したときは、そのラベルが not_before 以降に付けられたものだけを有効にする
    （PR の最新 commit が push される前に付けた許可を、新しい commit に引き継がないため）。
    """
    if label not in current_labels:
        return False
    last = None
    for e in events:
        if e.get("event") == "labeled" and (e.get("label") or {}).get("name") == label:
            last = e
    if not last or (last.get("actor") or {}).get("id") != OWNER_ID:
        return False
    if not_before is not None:
        return (last.get("created_at") or "") >= not_before
    return True


def head_pushed_at(check_suites: list[dict]) -> str | None:
    """最新 commit が push された時刻の代わりに、その commit の check suite の最も早い作成時刻を使う。

    commit の日付はエージェントが自由に決められるが、check suite の作成時刻は GitHub が付ける。
    """
    times = sorted(cs["created_at"] for cs in check_suites if cs.get("created_at"))
    return times[0] if times else None


def linked_issue_numbers(body: str | None) -> list[int]:
    return sorted({int(m) for m in LINK_RE.findall(body or "")})


def decide(
    frozen: dict[str, str],
    pr_frozen_ok: bool,
    manifesto_issues_ok: list[int],
) -> tuple[bool, str]:
    """(許可するか, 説明) を返す。"""
    if not frozen:
        return True, "凍結ファイルの変更なし"
    if pr_frozen_ok:
        return True, "kamonabe の frozen-ok あり"
    guard = sorted(p for p, c in frozen.items() if c == "guard")
    if guard:
        return False, "guard 区分の凍結ファイルを変えている（kamonabe の frozen-ok が要る）: " + ", ".join(guard)
    if manifesto_issues_ok:
        return True, "manifesto-change の issue を参照: " + ", ".join(f"#{n}" for n in manifesto_issues_ok)
    files = ", ".join(sorted(frozen))
    return False, "マニフェストを変えている（kamonabe の manifesto-change の issue の参照か frozen-ok が要る）: " + files


# ---------------------------------------------------------------- GitHub API


class GitHub:
    def __init__(self, token: str, repo: str):
        self.token = token
        self.base = f"https://api.github.com/repos/{repo}"

    def call(self, method: str, path: str, body: dict | None = None):
        req = urllib.request.Request(
            self.base + path,
            method=method,
            data=json.dumps(body).encode() if body is not None else None,
            headers={
                "Authorization": f"Bearer {self.token}",
                "Accept": "application/vnd.github+json",
                "X-GitHub-Api-Version": "2022-11-28",
            },
        )
        with urllib.request.urlopen(req, timeout=30) as r:
            data = r.read()
        return json.loads(data) if data else None

    def pages(self, path: str, limit: int = 30) -> list:
        out = []
        sep = "&" if "?" in path else "?"
        for page in range(1, limit + 1):
            got = self.call("GET", f"{path}{sep}per_page=100&page={page}")
            out.extend(got)
            if len(got) < 100:
                break
        return out


def main() -> int:
    env = os.environ
    gh = GitHub(env["GITHUB_TOKEN"], env["REPO"])
    pr_no = int(env["PR_NUMBER"])
    head = env["HEAD_SHA"]
    action = env.get("EVENT_ACTION", "")

    def status(ok: bool, msg: str) -> None:
        gh.call("POST", f"/statuses/{head}", {
            "state": "success" if ok else "failure",
            "context": STATUS_CONTEXT,
            "description": msg[:139],
        })

    try:
        with open(env.get("FROZEN_FILE", ".github/frozen-paths.txt"), encoding="utf-8") as fp:
            rules = parse_frozen(fp.read())
        pr = gh.call("GET", f"/pulls/{pr_no}")
        labels = [lb["name"] for lb in pr.get("labels", [])]

        # 新しい commit が足されたら、許可は付け直す（loop-guard §3.2）
        if action == "synchronize" and FROZEN_OK in labels:
            try:
                gh.call("DELETE", f"/issues/{pr_no}/labels/{FROZEN_OK}")
            except urllib.error.HTTPError as e:
                if e.code != 404:
                    raise
            labels.remove(FROZEN_OK)

        files = gh.pages(f"/pulls/{pr_no}/files")
        if len(files) != pr.get("changed_files"):
            # files API は 3000 件までしか返さない。全部を見られないときは通さない
            raise RuntimeError(f"変更ファイルを全部読めません（{len(files)} / {pr.get('changed_files')} 件）")
        frozen = classify(changed_paths(files), rules)
        pr_ok = False
        issues_ok: list[int] = []
        if frozen:
            suites = gh.call("GET", f"/commits/{head}/check-suites?per_page=100").get("check_suites", [])
            pushed = head_pushed_at(suites)
            if pushed is None:
                raise RuntimeError("最新 commit の push の時刻（check suite）が見つかりません")
            pr_ok = labeled_by_owner(FROZEN_OK, labels, gh.pages(f"/issues/{pr_no}/events"), pushed)
            if not pr_ok:
                for n in linked_issue_numbers(pr.get("body")):
                    try:
                        issue = gh.call("GET", f"/issues/{n}")
                    except urllib.error.HTTPError:
                        continue
                    if issue.get("pull_request") or issue.get("state") != "open":
                        continue
                    if (issue.get("user") or {}).get("id") != OWNER_ID:
                        continue
                    ilabels = [lb["name"] for lb in issue.get("labels", [])]
                    if labeled_by_owner(MANIFESTO_CHANGE, ilabels, gh.pages(f"/issues/{n}/events")):
                        issues_ok.append(n)
        ok, msg = decide(frozen, pr_ok, issues_ok)
    except Exception as e:  # noqa: BLE001 判定できないときは通さない（fail-closed）
        ok, msg = False, f"判定できませんでした: {type(e).__name__}: {e}"

    print(("OK: " if ok else "NG: ") + msg)
    try:
        status(ok, msg)
    except Exception as e:  # noqa: BLE001
        print(f"commit status を付けられませんでした: {e}", file=sys.stderr)
        return 1
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())

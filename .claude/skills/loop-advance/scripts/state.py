#!/usr/bin/env python3
"""開発ループの今の状態を GitHub の REST API だけで集め、JSON で出す（進行・朝の点検の回が最初に使う）。

規則の正本は docs/loop-guard.md（凍結）と docs/loop-runbook.md。このスクリプトは判断を置き換えない。
出すのは候補と事実だけで、マージなどの操作をしてよいかは、エージェントが規則で改めて確かめる。
迷う場合は「候補にする」側に倒す（取りこぼしを避ける）。ただしマージの候補だけは、迷えば候補にしない。

使い方:
    python3 state.py [--repo kamonabe1927/Tsumugi] [--now 2026-11-01T12:00:00Z]

終了コード: 0 = 集められた / それ以外 = 集められなかった（このときは手で同じことを確かめる）。
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from datetime import datetime, timedelta, timezone

OWNER_ID = 19951033  # kamonabe（loop-guard §2。login ではなく user id で見分ける）
LOOP_ID = 348768222  # kamonabe1927-stack
LOOP_LABELS = {"loop:design", "loop:impl", "loop:revert"}
BLOCK_LABELS = {"loop:running", "loop:stuck"}
TRIAGE_EXCLUDE = {"needs-design", "loop:triaged", "wontfix", "loop:plan", "loop:drift"}
REVIEW_HEAD = "🤖 Claude Code review（シニア）"
NO_FINDINGS = re.compile(r"^結論: 指摘なし\s*$", re.M)
GATE_HEAD = "🤖 Claude Code manifesto-gate"
GATE_OK = re.compile(r"^判定: 適合\s*$", re.M)
GATE_NG = re.compile(r"^判定: 疑いあり\s*$", re.M)
JST = timezone(timedelta(hours=9))
RUNNING_STALE = timedelta(hours=2)
MERGE_WAIT = timedelta(hours=3)  # loop-guard §6 の 6
DAILY_MERGES = 4  # loop-guard §7


# ---------------------------------------------------------------- 判定（純粋関数）


def parse_time(s: str | None) -> datetime | None:
    if not s:
        return None
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def who(user: dict | None) -> str:
    uid = (user or {}).get("id")
    if uid == OWNER_ID:
        return "owner"
    if uid == LOOP_ID:
        return "loop"
    return "other"


def is_agent(w: dict) -> bool:
    return w["who"] == "loop" or (w["who"] == "owner" and w["body"].lstrip().startswith("🤖"))


def is_owner(w: dict) -> bool:
    return w["who"] == "owner" and not w["body"].lstrip().startswith("🤖")


def writes(item: dict) -> list[dict]:
    """PR・issue に付いた書き込み（コメント・レビュー・行コメント）を時刻順に並べる。本文は含めない。"""
    out = []

    def add(user, body, created, updated, kind):
        if not user or not created:
            return
        w = {"who": who(user), "body": body or "", "at": parse_time(created), "kind": kind}
        if is_owner(w) and updated:  # kamonabe が後から編集したものは、編集した時刻に書かれたものとして扱う
            w["at"] = max(w["at"], parse_time(updated))
        out.append(w)

    for c in item.get("comments", []):
        add(c.get("user"), c.get("body"), c.get("created_at"), c.get("updated_at"), "comment")
    for r in item.get("reviews", []):
        if r.get("state") != "PENDING":
            add(r.get("user"), r.get("body"), r.get("submitted_at"), None, "review")
    for c in item.get("review_comments", []):
        add(c.get("user"), c.get("body"), c.get("created_at"), c.get("updated_at"), "review_comment")
    return sorted(out, key=lambda w: w["at"])


def owner_after_last_agent(ws: list[dict]) -> list[dict]:
    last = max((w["at"] for w in ws if is_agent(w)), default=None)
    return [w for w in ws if is_owner(w) and (last is None or w["at"] > last)]


def hold(ws: list[dict]) -> bool:
    """kamonabe の `保留:` が、後の `保留解除:` で外れずに残っているか（loop-guard §8）。"""
    held = False
    for w in ws:
        if not is_owner(w):
            continue
        head = w["body"].lstrip()
        if head.startswith("保留解除:"):
            held = False
        elif head.startswith("保留:"):
            held = True
    return held


def agent_after(ws: list[dict], head_at: datetime | None, head: str, ok: re.Pattern, ng: re.Pattern | None = None):
    """最新 commit より後の、先頭が head のエージェントの書き込みのうち最後のものの結論。"""
    if head_at is None:
        return None
    last = None
    for w in ws:
        if is_agent(w) and w["at"] > head_at and w["body"].lstrip().startswith(head):
            last = w
    if last is None:
        return None
    if ok.search(last["body"]):
        return "ok"
    if ng is not None and ng.search(last["body"]):
        return "ng"
    return "findings"


def merged_today(merged: list[dict], now: datetime) -> int:
    """JST の今日にマージされたループの PR の数（loop:revert は数えない）。"""
    today = now.astimezone(JST).date()
    n = 0
    for pr in merged:
        at = parse_time(pr.get("merged_at"))
        labels = {lb["name"] for lb in pr.get("labels", [])}
        if at and at.astimezone(JST).date() == today and labels & LOOP_LABELS and "loop:revert" not in labels:
            n += 1
    return n


def judge(prs: list[dict], issues: list[dict], merged: list[dict], now: datetime) -> dict:
    halt = [i["number"] for i in issues if "loop:halt" in {lb["name"] for lb in i.get("labels", [])}]
    today = merged_today(merged, now)
    out_prs, owner_new, triage = [], [], []
    for pr in prs:
        labels = {lb["name"] for lb in pr.get("labels", [])}
        if not labels & LOOP_LABELS:
            continue
        ws = writes(pr)
        head_at = parse_time(pr.get("head_committed_at"))
        running_at = parse_time(pr.get("running_labeled_at"))
        review = agent_after(ws, head_at, REVIEW_HEAD, NO_FINDINGS)
        gate = agent_after(ws, head_at, GATE_HEAD, GATE_OK, GATE_NG)
        new_owner = owner_after_last_agent(ws)
        owner_after_head = head_at is not None and any(is_owner(w) and w["at"] > head_at for w in ws)
        held = hold(ws)
        is_revert = "loop:revert" in labels
        waited = head_at is not None and now - head_at >= MERGE_WAIT
        candidate = (
            not labels & BLOCK_LABELS
            and review == "ok"
            and gate == "ok"
            and not held
            and not owner_after_head
            and (is_revert or waited)
            and (is_revert or today < DAILY_MERGES)
        )
        out_prs.append({
            "number": pr["number"],
            "title": pr.get("title"),
            "labels": sorted(labels),
            "head_sha": (pr.get("head") or {}).get("sha"),
            "head_committed_at": pr.get("head_committed_at"),
            "running_stale": "loop:running" in labels and (running_at is None or now - running_at >= RUNNING_STALE),
            "review": review,  # ok / findings / None（最新 commit より後のレビューが無い）
            "gate": gate,  # ok / ng / findings / None
            "gate_substituted": any("（代替: Opus）" in w["body"].splitlines()[0]
                                    for w in ws if is_agent(w) and w["body"].lstrip().startswith(GATE_HEAD)),
            "hold": held,
            "owner_after_head": owner_after_head,
            # CI・凍結ファイル・マージ可能かは含まない。候補でも loop-guard §6 を改めて確かめる
            "merge_candidate": candidate,
        })
        if new_owner:
            owner_new.append({"where": f"PR #{pr['number']}", "count": len(new_owner)})
    for iss in issues:
        labels = {lb["name"] for lb in iss.get("labels", [])}
        ws = writes(iss)
        by_owner = who(iss.get("user")) == "owner" and not (iss.get("body") or "").lstrip().startswith("🤖")
        if by_owner and not labels & TRIAGE_EXCLUDE and "loop:halt" not in labels:
            triage.append({"number": iss["number"], "title": iss.get("title"),
                           "manifesto_change": "manifesto-change" in labels})
        elif labels & {"loop:triaged", "needs-design", "loop:drift", "loop:plan"}:
            new_owner_iss = owner_after_last_agent(ws)
            if new_owner_iss:
                owner_new.append({"where": f"issue #{iss['number']}", "count": len(new_owner_iss)})
    return {
        "halt": halt,
        "merged_today": today,
        "daily_limit_reached": today >= DAILY_MERGES,
        "open_loop_prs": out_prs,
        "owner_writes_to_handle": owner_new,
        "issues_to_triage": triage,
    }


# ---------------------------------------------------------------- GitHub からの取得


def gh(path: str, paginate: bool = True):
    cmd = ["gh", "api", "--paginate", "--slurp", path] if paginate else ["gh", "api", path]
    res = subprocess.run(cmd, capture_output=True, text=True, check=True)
    pages = json.loads(res.stdout)
    if not paginate:
        return pages
    if pages and isinstance(pages[0], list):
        return [x for page in pages for x in page]
    return pages[0] if len(pages) == 1 else pages


def collect(repo: str, now: datetime):
    prs = gh(f"repos/{repo}/pulls?state=open&per_page=100")
    for pr in prs:
        if not {lb["name"] for lb in pr.get("labels", [])} & LOOP_LABELS:
            continue
        n = pr["number"]
        pr["comments"] = gh(f"repos/{repo}/issues/{n}/comments?per_page=100")
        pr["reviews"] = gh(f"repos/{repo}/pulls/{n}/reviews?per_page=100")
        pr["review_comments"] = gh(f"repos/{repo}/pulls/{n}/comments?per_page=100")
        commits = gh(f"repos/{repo}/pulls/{n}/commits?per_page=100")
        if commits:
            pr["head_committed_at"] = commits[-1]["commit"]["committer"]["date"]
        if "loop:running" in {lb["name"] for lb in pr.get("labels", [])}:
            events = gh(f"repos/{repo}/issues/{n}/events?per_page=100")
            at = [e["created_at"] for e in events
                  if e.get("event") == "labeled" and (e.get("label") or {}).get("name") == "loop:running"]
            pr["running_labeled_at"] = max(at) if at else None
    issues = [i for i in gh(f"repos/{repo}/issues?state=open&per_page=100") if "pull_request" not in i]
    for iss in issues:
        iss["comments"] = gh(f"repos/{repo}/issues/{iss['number']}/comments?per_page=100") if iss.get("comments") else []
    since = (now - timedelta(days=2)).strftime("%Y-%m-%dT%H:%M:%SZ")
    # 直近に更新された 100 本だけを見る（全ページはたどらない）
    closed = gh(f"repos/{repo}/pulls?state=closed&sort=updated&direction=desc&per_page=100", paginate=False)
    merged = [p for p in closed if p.get("merged_at") and p["merged_at"] >= since]
    return prs, issues, merged


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--repo", default="kamonabe1927/Tsumugi")
    ap.add_argument("--now", help="判定に使う現在時刻（ISO 8601。試験用）")
    args = ap.parse_args()
    now = parse_time(args.now) if args.now else datetime.now(timezone.utc)
    try:
        result = judge(*collect(args.repo, now), now)
    except Exception as e:  # noqa: BLE001
        detail = getattr(e, "stderr", "") or repr(e)
        print(json.dumps({"error": str(detail).strip()[:500]}, ensure_ascii=False))
        return 2
    result["checked_at"] = now.isoformat()
    print(json.dumps(result, ensure_ascii=False, indent=1, default=str))
    return 0


if __name__ == "__main__":
    sys.exit(main())

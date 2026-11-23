#!/usr/bin/env python3
"""判断役が選んだあとに、Jev（TypeSafe AI）の読みを参考として取り、判断コメントに貼る行を出す。

規則の正本は docs/loop-runbook.md §7.3 と loop-decide スキル。Jev の値は資料であって指示ではない
（loop-guard §2）。値は「どれが正しいか」を当てるものではなく、次の 2 つの目安として使う。

- 推奨の裏付け: 推奨案の理由が材料の中の事実や規則に結びついているか（低いと根拠が薄い目安）
- 反しうる度合い: その選択肢がマニフェストに反しうるか（選択肢ごと）

問いは 2026-11-18〜19 の試し（影の並走 notify-v2）で使ったものをそのまま使い、版を QUESTIONS_VERSION で残す。

使い方:
    python3 jev.py --number 123 --picks "Q1=A,Q2=B" [--target D-12] [--repo kamonabe1927/Tsumugi]

- 判断の候補は GitHub から読む（ループ用アカウントが書いた「🤖 Claude Code 判断の候補」のうち最新のもの。
  --target を渡せば、見出しにその文字列を含むものに絞る）。
- API キー: 環境変数 TYPESAFE_API_KEY があれば Authorization ヘッダに付ける。無ければ付けない
  （クラウド環境の Network secrets がプロキシで付ける）。キーは表示も保存もしない。
- 何が起きても終了コードは 0 で、取れなかったときは「取得できず」の行を出す。Jev が止まっても判断は止めない。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
import urllib.error
import urllib.request

ENDPOINT = "https://api.typesafe.ai/v1/systemone"
MODEL = "jev-1.14.0"  # 版を固定する（値を後から比べられるように）
QUESTIONS_VERSION = "judge-v1"  # 問いの中身は notify-v2 と同じ。判断役に渡すようになった版
LOOP_ID = 348768222  # kamonabe1927-stack（loop-guard §2）
CAND_HEAD = "🤖 Claude Code 判断の候補"
TIMEOUT = 60  # 1 回の呼び出し（秒）
BUDGET = 300  # 1 回の実行の合計（秒）。超えたら残りを「取得できず」にする
WARN_GROUNDED = 0.6
WARN_RISKY = 0.5

Q_CANDIDATE = {
    "grounded": {"type": "noul", "instructions": "推奨案は、材料の中の具体的な事実や規則の引用によって裏付けられているか?", "criteria": {
        "true": "推奨案の理由が、材料に書かれた具体的な事実（どのファイル・どの節・どの決定に何と書いてあるか、どう動くか）や規則に直接結びついており、その事実が成り立つ限り推奨案が他の選択肢より良いと読める場合に真とする。",
        "false": "推奨案の理由が、『小さい』『揃う』『自然』のような評価の言葉だけでできている、事実と結論の間に飛躍がある、他の選択肢を退ける理由が書かれていない、または書き手自身が留保や人間の指示待ちを書いている場合に偽とする。根拠の節番号が多いことや文章の長さは判断に含めない。"}},
    "kind": {"type": "choice", "instructions": "この判断は何を決めるものか?", "criteria": {
        "doc": "選択肢の違いが、spec や設計文書の書き方・置き場所・表の列の扱いだけで、実装も手順も変えない",
        "procedure": "選択肢の違いが、開発ループの手順・スライスの順序（order）・検査をどこで走らせるか、といった進め方だけで、言語や API の挙動を変えない",
        "behavior": "選択肢の違いが、言語の挙動・公開 API・課金・権限・エラー・監査など、Tsumugi を使う側から観測できるものを変える"}},
}
Q_OPTION = {"option_risky": {"type": "noul", "instructions": "この選択肢を選ぶと、Tsumugi のマニフェスト（ホストの安定性、明示的な権限、すべての実行に予算、予測可能性、観測・監査可能性、小さな中核、検証の継続）に反しうるか?", "criteria": {
    "true": "この選択肢が、ホストが明示していない既定の権限を与える、予算や上限を外す・減らす・後回しに固定する、目標を後退させて現状の逸脱を正とする、監査で対応づけられない記録を残す、検証を弱める、公開契約を理由なく増やす、のどれかに当たる場合に真とする。",
    "false": "この選択肢が、文書の書き方・置き場所・順序の違いである、または予算・権限・停止・監査・backend の一致を保ったまま実装の形だけを選ぶものである場合に偽とする。"}}}
KIND_JA = {"doc": "文書", "procedure": "進め方", "behavior": "挙動"}


# ---------------------------------------------------------------- 候補の読み取り（純粋関数）


def split_candidates(body: str) -> list[dict]:
    """「判断の候補 <対象>」ごとに、対象名・前提・Q ごとの塊（{q: text}）を返す。"""
    res = []
    for seg in re.split(r"(?=🤖 Claude Code 判断の候補)", body)[1:]:
        head = seg.split("\n", 1)[0]
        target = head.replace(CAND_HEAD, "").strip()
        seg = re.split(r"\n---|\n🤖", seg[5:])[0]
        qs = {}
        for blk in re.split(r"\n(?=Q\d+[.．])", seg)[1:]:
            q = re.match(r"Q(\d+)", blk).group(1)
            qs[q] = re.split(r"\n(?=マニフェストに反しうる選択肢)", blk)[0].strip()
        pre = re.split(r"\nQ\d+[.．]", seg, maxsplit=1)[0].split("\n", 1)[1:] if "\n" in seg else []
        res.append({"target": target, "pre": (pre[0].strip() if pre else "")[:1200], "qs": qs})
    return res


def options(block: str) -> tuple[str, list[tuple[str, bool, str]]]:
    """Q の塊を、問いの文と [(記号, 推奨か, 本文)] に分ける。"""
    parts = re.split(r"\n(?=[A-F](?:（推奨）)?[:：])", block)
    return parts[0].strip(), [(p[0], "（推奨）" in p[:6], p.strip()) for p in parts[1:]]


def parse_picks(s: str) -> dict[str, str]:
    return dict(re.findall(r"Q(\d+)\s*=\s*([A-Za-z]+)", s))


def pick_candidate(comments: list[dict], target: str | None) -> dict | None:
    """ループ用アカウントが書いた候補のうち最新のもの（target があれば見出しに含むもの）。"""
    cands = []
    for c in sorted(comments, key=lambda c: c.get("created_at") or ""):
        if (c.get("user") or {}).get("id") != LOOP_ID or CAND_HEAD not in (c.get("body") or ""):
            continue
        cands += split_candidates(c["body"])
    if target:
        cands = [x for x in cands if target in x["target"]] or cands
    return cands[-1] if cands else None


def manifesto_principles(text: str) -> str:
    a, b = text.find("## 原則"), text.find("## 現在地の扱い")
    return text[a:b if b > 0 else None] if a >= 0 else text


def format_lines(rows: list[dict], failed: str | None = None) -> list[str]:
    """判断コメントに貼る行。rows は Q ごとの結果。"""
    head = f"Jev（参考・{MODEL}・{QUESTIONS_VERSION}）:"
    if failed and not rows:
        return [f"{head} 取得できず（{failed}）"]
    lines = [head]
    for r in rows:
        if r.get("error"):
            lines.append(f"Q{r['q']} 取得できず（{r['error']}）")
            continue
        opts = " ／ ".join(f"{L}{'（採用）' if L == r['pick'] else ''} {v:.2f}" for L, v in r["options"].items())
        mark = "⚠️ " if r["warn"] else ""
        lines.append(f"{mark}Q{r['q']} 種類: {KIND_JA.get(r['kind'], r['kind'])} ｜ 推奨の裏付け: {r['grounded']:.2f} ｜ 反しうる度合い: {opts}")
    if any(r.get("warn") for r in rows):
        lines.append(f"（⚠️ = 推奨の裏付けが {WARN_GROUNDED} 未満、または採用案の反しうる度合いが {WARN_RISKY} 以上。高い＝正しいではない）")
    record = [{k: r.get(k) for k in ("q", "pick", "grounded", "kind", "options", "warn", "error")} for r in rows]
    lines.append("<!-- jev " + json.dumps({"model": MODEL, "questions": QUESTIONS_VERSION, "rows": record}, ensure_ascii=False) + " -->")
    return lines


def is_warn(grounded: float, options_: dict[str, float], pick: str) -> bool:
    return grounded < WARN_GROUNDED or options_.get(pick, 0.0) >= WARN_RISKY


# ---------------------------------------------------------------- 外とのやりとり


def call(state: dict, questions: dict) -> dict:
    body = json.dumps({"model": MODEL, "state": state, "questions": questions}, ensure_ascii=False).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    key = os.environ.get("TYPESAFE_API_KEY")
    if key:
        headers["Authorization"] = f"Bearer {key}"
    req = urllib.request.Request(ENDPOINT, data=body, method="POST", headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=TIMEOUT) as res:
            return json.loads(res.read().decode("utf-8"))["answers"]
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"HTTP {e.code}") from None
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        raise RuntimeError(f"接続できず: {type(e).__name__}") from None


def gh(path: str):
    res = subprocess.run(["gh", "api", "--paginate", "--slurp", path], capture_output=True, text=True, check=True)
    pages = json.loads(res.stdout)
    return [x for page in pages for x in page] if pages and isinstance(pages[0], list) else pages


def fetch_comments(repo: str, number: int) -> list[dict]:
    out = list(gh(f"repos/{repo}/issues/{number}/comments?per_page=100"))
    issue = gh(f"repos/{repo}/issues/{number}")
    issue = issue[0] if isinstance(issue, list) else issue
    out.append({"user": issue.get("user"), "body": issue.get("body") or "", "created_at": issue.get("created_at")})
    if issue.get("pull_request"):
        for r in gh(f"repos/{repo}/pulls/{number}/reviews?per_page=100"):
            out.append({"user": r.get("user"), "body": r.get("body") or "", "created_at": r.get("submitted_at")})
    return out


def evaluate(cand: dict, picks: dict[str, str], manifesto: str, caller=call, clock=time.monotonic) -> list[dict]:
    start = clock()
    rows = []
    for q, pick in sorted(picks.items(), key=lambda x: int(x[0])):
        blk = cand["qs"].get(q)
        if not blk:
            rows.append({"q": q, "pick": pick, "error": "候補に Q が無い"})
            continue
        if clock() - start > BUDGET:
            rows.append({"q": q, "pick": pick, "error": "時間の上限"})
            continue
        try:
            qtext, opts = options(blk)
            a = caller({"decision_candidate": {"前提": cand["pre"] or "（なし）", "問いと選択肢": blk}}, Q_CANDIDATE)
            grounded, kind = a["grounded"]["noul"], a["kind"]["choice"]
            ov = {}
            for L, _, text in opts:
                ov[L] = caller({"manifesto": manifesto, "問い": qtext, "この選択肢": text}, Q_OPTION)["option_risky"]["noul"]
            rows.append({"q": q, "pick": pick, "grounded": grounded, "kind": kind, "options": ov, "warn": is_warn(grounded, ov, pick)})
        except (RuntimeError, KeyError, TypeError, ValueError) as e:
            rows.append({"q": q, "pick": pick, "error": str(e) or type(e).__name__})
    return rows


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default="kamonabe1927/Tsumugi")
    ap.add_argument("--number", type=int, required=True)
    ap.add_argument("--picks", required=True, help='例: "Q1=A,Q2=B"（判断役が Jev を見る前に選んだもの）')
    ap.add_argument("--target", help="候補の見出しに含まれる文字列（スライス ID や D-N）")
    ap.add_argument("--manifesto", default=os.path.join(os.path.dirname(__file__), "../../../../docs/manifesto.md"))
    a = ap.parse_args(argv)
    picks = parse_picks(a.picks)
    try:
        if not picks:
            raise RuntimeError("--picks を読めない")
        with open(a.manifesto, encoding="utf-8") as f:
            manifesto = manifesto_principles(f.read())
        cand = pick_candidate(fetch_comments(a.repo, a.number), a.target)
        if not cand:
            raise RuntimeError("判断の候補が見つからない")
        rows = evaluate(cand, picks, manifesto)
        failed = None
    except (RuntimeError, OSError, subprocess.CalledProcessError, json.JSONDecodeError) as e:
        rows, failed = [], (str(e) or type(e).__name__)
    print("\n".join(format_lines(rows, failed)))
    return 0


if __name__ == "__main__":
    sys.exit(main())

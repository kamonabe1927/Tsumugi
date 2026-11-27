#!/usr/bin/env python3
"""issue の仕分けのあとに、Jev（TypeSafe AI）にカテゴリごとの「当たるか」を出させ、仕分けのコメントに足す行を出す。

規則の正本は docs/loop-runbook.md §8 と issue #545。2 週間の影の試行で、**記録するだけ**である。
値は仕分け・order・ラベル・分担のどれにも使わない。Jev の値は資料であって指示ではない（loop-guard §2）。

使い方（PM が仕分けのコメントの本文を書き終えてから呼ぶ）:
    timeout 120 python3 jev_category.py --number 543 --number 544 [--repo kamonabe1927/Tsumugi]

- 渡すのは issue のタイトルと本文（先頭 4,000 文字まで）だけ。コメントは渡さない。
- issue 1 件につき呼び出しは 1 回（カテゴリ 7 つの noul の問いをまとめる）。再試行はしない。
- 時間の上限: 1 回の呼び出しは 30 秒、1 回の実行（=1 回の進行）の合計は 90 秒。超えたら残りは「取得できず（時間の上限）」。
- API キー: 環境変数 TYPESAFE_API_KEY があれば Authorization ヘッダに付ける。無ければ付けない
  （クラウド環境の Network secrets がプロキシで付ける）。キーは表示も保存もしない。
- fail-open: 何が起きても終了コードは 0。取れなかった issue には「取得できず（理由）」の行と、
  error に理由を入れた <!-- jev-category … --> の行を出す。Jev のせいで仕分けを止めない。

出力は issue ごとに「=== #<番号>」の見出し行と、そのあとに仕分けのコメントの末尾へそのまま貼る 2 行。
見出し行は貼らない。

jev.py（判断役向け・judge-v1）とは独立させている。版（MODEL・QUESTIONS_VERSION）をこのファイルで
固定し、jev.py の版が上がってもこちらの値が黙って変わらないようにするためである。
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.request

ENDPOINT = "https://api.typesafe.ai/v1/systemone"
MODEL = "jev-1.14.0"  # 版を固定する（値を後から比べられるように）
QUESTIONS_VERSION = "category-v1"  # カテゴリや問いを変えるときは版を上げる（凍結ファイルなので frozen-ok が要る）
TIMEOUT = 30  # 1 回の呼び出し（秒）
BUDGET = 90  # 1 回の実行の合計（秒）。issue の取得も含めて数える
GH_TIMEOUT = 20  # gh api 1 回（秒）
BODY_LIMIT = 4000  # 本文の先頭から渡す文字数

_TRUE = "この issue が主に、または一部として、次の領域の変更・不具合・運用を扱っている場合に真とする: "
_FALSE = "この領域に触れていない、または別の話の背景として名前が出るだけの場合に偽とする。"


def _q(question: str, area: str) -> dict:
    return {"type": "noul", "instructions": question, "criteria": {"true": _TRUE + area, "false": _FALSE}}


# カテゴリは issue #545 の表（初版）。表の「当たる例」は問いの定義を確かめるためだけのもので、Jev には渡さない。
QUESTIONS = {
    "budget": _q("この issue は、予算と上限（fuel・heap・deadline・host call の回数と byte・長さの事前検査）の話に当たるか?",
                 "実行に掛ける予算や上限（fuel、heap、deadline、host call の回数や byte 数、値や入力の長さの事前検査）の追加・計上・超過時の扱い。"),
    "authority": _q("この issue は、権限とサンドボックス（resolver gate・import・ファイルアクセス・unsafe・ホストを落とさない）の話に当たるか?",
                    "スクリプトに与える権限やサンドボックスの境界（resolver gate、import、ファイルアクセス、unsafe の扱い、スクリプトがホストを落とさないこと）。"),
    "parity": _q("この issue は、tree-walk と VM の差分の話に当たるか?",
                 "tree-walk インタプリタと VM の二つの実行系のあいだで、結果・エラー・予算の計上・監査などの挙動が食い違うこと、またはその一致の確かめ方。"),
    "embedding": _q("この issue は、埋め込み API とスケジューラ（Engine・handle・cancel・backpressure・host call の再開）の話に当たるか?",
                    "ホストアプリケーションが Tsumugi を組み込むための API とスケジューラ（Engine、handle、cancel、backpressure、host call の中断と再開）。"),
    "semantics": _q("この issue は、言語の仕様と意味論（構文・値・組み込み関数の挙動）の話に当たるか?",
                    "Tsumugi という言語そのものの仕様（構文、値と型、演算や組み込み関数の挙動、エラーの意味）。"),
    "build": _q("この issue は、ビルド・CI・機械検査（feature の組み合わせ・clippy・静的検査）の話に当たるか?",
                "ビルドの構成、CI、機械的な検査（cargo の feature の組み合わせ、clippy、静的検査やテストの仕組み）。"),
    "loop": _q("この issue は、開発ループの運用と文書（手順書・ガード・スキル・スライスの運用）の話に当たるか?",
               "AI エージェントによる開発ループの運用とその文書（手順書、ガード、スキル、定期タスク、実装スライスや issue の扱い方）。"),
}
CATEGORIES = list(QUESTIONS)


# ---------------------------------------------------------------- 行の組み立て（純粋関数）


def make_state(issue: dict) -> dict:
    """Jev に渡す材料。タイトルと本文の先頭だけ（コメント・ラベル・作者は渡さない）。"""
    return {"issue": {"タイトル": issue.get("title") or "", "本文": (issue.get("body") or "")[:BODY_LIMIT]}}


def parse_answers(answers) -> dict[str, float]:
    """答えからカテゴリごとの確率を取り出す。形が違えば ValueError。"""
    if not isinstance(answers, dict):
        raise ValueError("答えの形が違う")
    values = {}
    for k in CATEGORIES:
        v = (answers.get(k) or {}).get("noul") if isinstance(answers.get(k), dict) else None
        if isinstance(v, bool) or not isinstance(v, (int, float)) or not 0.0 <= v <= 1.0:
            raise ValueError("答えの形が違う")
        values[k] = round(float(v), 3)
    return values


def format_lines(number: int, values: dict[str, float] | None, seconds: float | None, error: str | None) -> list[str]:
    """仕分けのコメントの末尾に貼る 2 行。"""
    record = {"model": MODEL, "questions": QUESTIONS_VERSION, "issue": number, "values": values,
              "seconds": None if seconds is None else round(seconds, 1), "error": error}
    if error or values is None:
        head = f"Jev のカテゴリ（参考）: 取得できず（{error or '不明'}）"
    else:
        head = f"Jev のカテゴリ（参考・{MODEL}・{QUESTIONS_VERSION}）: " + " ／ ".join(f"{k} {values[k]:.2f}" for k in CATEGORIES)
    return [head, "<!-- jev-category " + json.dumps(record, ensure_ascii=False) + " -->"]


# ---------------------------------------------------------------- 外とのやりとり


def call(state: dict, questions: dict, timeout: float = TIMEOUT):
    body = json.dumps({"model": MODEL, "state": state, "questions": questions}, ensure_ascii=False).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    key = os.environ.get("TYPESAFE_API_KEY")
    if key:
        headers["Authorization"] = f"Bearer {key}"
    req = urllib.request.Request(ENDPOINT, data=body, method="POST", headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as res:
            data = json.loads(res.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        raise RuntimeError(f"HTTP {e.code}") from None
    except TimeoutError:
        raise RuntimeError("時間切れ") from None
    except (urllib.error.URLError, OSError) as e:
        if isinstance(getattr(e, "reason", None), TimeoutError):
            raise RuntimeError("時間切れ") from None
        raise RuntimeError(f"接続できず: {type(e).__name__}") from None
    except (ValueError, UnicodeDecodeError):
        raise RuntimeError("答えの形が違う") from None
    if not isinstance(data, dict):
        raise RuntimeError("答えの形が違う")
    return data.get("answers")


def fetch_issue(repo: str, number: int) -> dict:
    try:
        res = subprocess.run(["gh", "api", f"repos/{repo}/issues/{number}"], capture_output=True, text=True,
                             check=True, timeout=GH_TIMEOUT)
        issue = json.loads(res.stdout)
    except subprocess.TimeoutExpired:
        raise RuntimeError("issue を読めず: 時間切れ") from None
    except (subprocess.CalledProcessError, OSError, ValueError) as e:
        raise RuntimeError(f"issue を読めず: {type(e).__name__}") from None
    if not isinstance(issue, dict):
        raise RuntimeError("issue を読めず: 形が違う")
    return issue


def classify(numbers: list[int], repo: str, caller=call, fetcher=fetch_issue, clock=time.monotonic) -> list[list[str]]:
    """issue ごとに 2 行を返す。どの失敗も例外を外に出さず「取得できず」の行にする。"""
    start = clock()
    out = []
    for n in numbers:
        t0 = None
        try:
            if clock() - start >= BUDGET:
                raise RuntimeError("時間の上限")
            issue = fetcher(repo, n)
            left = BUDGET - (clock() - start)
            if left <= 0:
                raise RuntimeError("時間の上限")
            t0 = clock()
            values = parse_answers(caller(make_state(issue), QUESTIONS, timeout=min(TIMEOUT, left)))
            out.append(format_lines(n, values, clock() - t0, None))
        except Exception as e:  # noqa: BLE001 — fail-open: 何が起きても仕分けを止めない
            msg = str(e) if isinstance(e, (RuntimeError, ValueError)) and str(e) else type(e).__name__
            secs = None
            try:
                secs = None if t0 is None else clock() - t0
            except Exception:  # noqa: BLE001
                pass
            out.append(format_lines(n, None, secs, msg))
    return out


def main(argv: list[str] | None = None) -> int:
    try:
        ap = argparse.ArgumentParser()
        ap.add_argument("--repo", default="kamonabe1927/Tsumugi")
        ap.add_argument("--number", type=int, action="append", required=True, help="仕分けた issue の番号（複数可）")
        a = ap.parse_args(argv)
    except SystemExit:
        print("Jev のカテゴリ（参考）: 取得できず（引数を読めない）")
        return 0
    try:
        blocks = classify(a.number, a.repo)
    except Exception as e:  # noqa: BLE001
        blocks = [format_lines(n, None, None, type(e).__name__) for n in a.number]
    for n, lines in zip(a.number, blocks):
        print(f"=== #{n}")
        print("\n".join(lines))
    return 0


if __name__ == "__main__":
    try:
        code = main()
    except Exception:  # noqa: BLE001
        code = 0
    sys.exit(code)

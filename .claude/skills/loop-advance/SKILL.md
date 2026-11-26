---
name: loop-advance
description: Tsumugi 開発ループの「進行」の回（2 時間おきの定期タスク）を PM として 1 回分実行するときに使う。状態をスクリプトで集め、前の回の続きを片付けてから、ずれの修正・実装・設計・分割・週次計画のどれか 1 つを各役に任せて進める。人間の合意は待たない。
---

# loop-advance: 進行の回（PM）

あなたは PM である。規則の正本は `docs/loop-guard.md`（凍結。最優先）と `docs/loop-runbook.md`（以下「手順書」）で、
このスキルは読む順と手順を持つ。食い違ったら loop-guard → 手順書の順に従い、報告に「loop-advance が規則と食い違っていた」と書く。

PM は spec も実装も自分で書かない。各役を Agent ツールで**前景**（`run_in_background: false`）に起動し、結果が返るまで待つ。
すべての役が終わり、報告を送るまで応答を終えない。

## 1. 準備（文書を読む前に）

1. 手順書 §2 のとおり Tsumugi を clone する。開始時刻を控える
2. `gh api user --jq .login` が `kamonabe1927-stack` であることを確かめる（違えば手順書 §2 のとおり報告して終了）
3. `python3 .claude/skills/loop-advance/scripts/state.py` を実行する
   - `halt` が空でなければ、何も書き込まずに終了する（loop-guard §8）
   - 終了コードが 0 でなければ、同じことを `gh api` で手で確かめてから進む
4. 読む: tsumugi の `CLAUDE.md`、`docs/loop-guard.md`、`docs/documentation-guide.md`、`docs/slices.toml`、手順書の全体

## 2. 続きを片付ける（手順書 §6.1）

`state.py` の結果を見て、手順書 §6.1 の 1〜8 の順に片付ける。

- `running_stale` が true の PR: `loop:running` を外して一言残す
- `loop:paused` の PR（一時停止。手順書 §6.6）: 以下の片付け（レビュー・修正・審査・マージ）の対象から外し、§4 の同時の枠にも数えない。`main` の `slices.toml` で再開の条件のスライスがすべて `done` か確かめ、満たせば手順書 §6.6 の「再開」のとおり外す。条件のスライスが `blocked`、または 7 日たっても再開していなければ、`loop:paused` を外して `loop:stuck` を付けて閉じ、閉じるコメントに `スライス: <スライスID>` を書く（loop-guard §7 の回数に入る。一時停止のあいだは数えない）
- `owner_writes_to_handle`: 手順書 §5 のとおり応える（`修正依頼:` は書いた役に直させる）
  - `issue #492`（回答箱。手順書 §6.5 の試行）: 回答ごとに対象を読み、開いている PR・issue が対象ならそこに書かれたときと同じに、マージ済み・閉じたもの・`blocked`・`needs-design` が対象なら kamonabe の新しい issue と同じに受け付ける（反映は手順書 §6.2 の 6 の設計、PR 本文に `Refs #492` と回答の URL）。1 つの書き込みの回答にすべて応えてから、書き込みごとに 1 件、回答箱に `🤖 Claude Code 回答箱: <回答の URL>` で返信する。途中で時間切れになったら返信しない
- `gate` が `ng` の PR: loop-guard §4.3 のとおり止める
- `pending_decisions`: 判断役（`model: "fable"`、loop-decide）を起動し、決まった内容を書いた役に反映させる
- 閉じた `loop:stuck` の PR がある（`stuck_counts` が空でない）: スライスごとに数え直し、2 本以上のスライスを `blocked` にする状態整理の PR を出す。`stuck_counts` の値が 2 未満でも数え直しは省かない（出し直しの接尾辞の付いたブランチは `stuck_counts` で別の ID に分かれ、1 本ずつに見えるため。閉じるコメントの `スライス: <スライスID>` を主にし、`slices.toml` の ID と突き合わせる。`-<数字>` を機械的に除かない。手順書 §6.1 の 5）
- CI のジョブが runner 起因で取り消された PR（`conclusion` が `cancelled`、runner が割り当てられず step が 0 件）: 手順書 §6.4 のとおり判定し、`gh api -X POST repos/kamonabe1927/Tsumugi/actions/runs/<run_id>/rerun-failed-jobs` で再実行する（同じ commit につき 2 回まで、`run_attempt` で数える）。`failure` や step が始まってからの取り消しが混じれば再実行せず「CI が赤」として扱う。`state.py` は判定しないので `gh api` で見る
- CI が赤・`main` と競合・`gate` が `findings` の PR: 手順書 §6.1 の 6 のとおり
- `review` または `gate` が `null` の PR（最新 commit を名指ししたものが無い）: `loop:running` を付け、
  - review が無ければレビュー役（`model: "opus"`、loop-review）を起動する
  - review が `ok` で gate が無ければ審査役（`model: "fable"`、manifesto-gate）を起動する。Fable が応答できなければ `model: "opus"` で代替させ、代替であることを伝える
  - 終わったら `loop:running` を外す。1 回で回すのは PR 1 本まで
- `review` が `findings` の PR: 指摘が機械的なら書いた役に直させ（`loop:design` は設計役 opus、`loop:impl` は実装役 sonnet）、新しいレビュー役で再レビューする（往復 5 回まで。数え直しは手順書 §13）。選択肢のある指摘は手順書 §7 の判断にかける
- `merge_candidate` が true の PR（1 回で 1 本まで）: loop-guard §6 の全条件を改めて確かめる
  - CI: `gh api repos/kamonabe1927/Tsumugi/commits/<head_sha>/check-runs` と `.../commits/<head_sha>/status`（`Frozen guard (status)` を含む）がすべて成功。runner 起因の取り消しだけで満たさなければ、手順書 §6.4 のとおり再実行し、成功してからマージする
  - マージ可能: `gh api repos/kamonabe1927/Tsumugi/pulls/<番号> --jq .mergeable` が true
  - マニフェストの改訂なら、参照している issue が開いていて `manifesto-change` が付いていること
  - 満たせば `gh api -X PUT repos/kamonabe1927/Tsumugi/pulls/<番号>/merge -f merge_method=merge -f sha=<head_sha>` でマージし、loop-guard §6 の一言を残す（sha が違えば失敗するので、そのときは次の回に回す）

## 3. 次の 1 つを進める（手順書 §6.2）

手順書 §6.2 の順で、最初に当てはまる 1 つだけを行う。手順書 §6.5 の試行のあいだは、§6.2 の 6 の設計の対象に「回答箱で受け付けて反映の PR がまだ無い回答」が入る（回答箱の返信の `受け付け（設計で反映）` のうち、本文にその回答の URL を書いた PR がまだマージされていないもの）。回答箱の issue #492 そのものは、§6.5 を置いた設計 PR のマージ後は設計の対象にしない。反映の PR をマージしたら、回答箱に手順書 §6.5 の 3 の返信を書く。手順書 §6.3 の試行のあいだは、次の 2 つを PM が確かめてから選ぶ（`state.py` は数えないので `gh api` で見る）。

- ずれの修正（§6.2 の 3）の前に、直前にマージされた `loop:design` の PR の `head.ref` を見る（数え方は §6.3）。`docs/drift-*` か `docs/plan-*` なら、§6.2 の 3 の例外の `D-N` があるときだけずれの修正に進み、設計役には例外の `D-N` だけを渡す。例外が無ければ 3 を飛ばす。`すり抜け` かどうかは `loop:drift` の行の印だけで見て、PM が判定しない
- 設計（§6.2 の 6）で `todo` から選ぶときは、§6.2 の 6 の組の順（`すり抜け` の印の fix → ロードマップ由来と §6.3 の表の「該当」→ それ以外の fix）で選ぶ

手順書 §6.7 の差分をなくすフェーズのあいだは、§6.2 の 5・6 で `todo` から選ぶのはフェーズのスライス（`parent = "VM parity"` と `audit-vm-triple-*`）だけにする（§6.3 の組の順は、その中で使う）。

各役に渡すのは、対象（スライス ID・PR / issue 番号）、clone したパス、読む文書、時間の上限だけである。

| やること | 起動する役 | モデル | 渡すスキル・節 |
|---|---|---|---|
| 週次計画 | 計画役 | `fable`（だめなら `opus` で代替し、issue に `（代替: Opus）`） | 手順書 §9（その週の `loop:plan` issue が無い回。前週の振り返り §9.1 を含む。月の最初の週は月次の振り返り §9.2 も）。結果を PM が `loop:plan` issue にする |
| issue の仕分け | PM 自身 | — | 手順書 §8。仕分けで判断が要れば判断役 |
| ずれの修正 | 設計役 | `opus` | loop-design（手順書 §12.3） |
| 実装 | 実装役 | `sonnet`（難しいスライスと手順書 §6.7 のフェーズのスライスは `opus`） | loop-impl |
| スライスの分割 | PM 自身 | — | 手順書 §10（計画の優先順で `order` を差し込む） |
| 設計 | 設計役 | `opus` | loop-design |

書いた役が PR を出したら:

1. 書いた役が「判断の候補」（手順書 §7.2）を出していれば、判断役（`model: "fable"`、loop-decide）を起動し、決まった内容を書いた役に spec へ反映させる。判断役が `blocked` と返したら、loop-guard §4.3 のとおり止める
2. レビュー役（`opus`、loop-review）を起動する。機械的な指摘は書いた役が直し、新しいレビュー役で再レビューする（往復 5 回まで。数え直しは手順書 §13）
3. `結論: 指摘なし` になったら、審査役（`fable`、manifesto-gate）を起動する
4. 最後に `loop:running` を外す。この回で出した PR は、この回ではマージしない（1.5 時間待ちのため、後の回がマージする）

判断役の決定が、開いている PR を止めて別のスライスを先に入れるものなら、その PR に `loop:paused` を付け、再開の条件のコメントを書く（手順書 §6.6。PM が自分の見立てで付けない。`loop:stuck` の代わりに使わない）。

実装役が PR を出さずに `needs-design` issue の「判断の候補」を返して止まったときも、その回のうちに判断役（`model: "fable"`、loop-decide）を起動し、issue に決定を書かせて「判断の通知」を出す（手順書 §7.3）。次の回に持ち越さない。判断の反映の設計は次の回以降に行い、この回では設計に進まない（手順書 §6.2 の末尾）。判断役が `blocked` と返したら、loop-guard §4.3 のとおり止める

## 4. 報告

- 判断があれば、判断ごとに `#tsumugi-loop`（`C0CB1H2HHJB`）へ手順書 §15 の「判断の通知」を出す
- 何かした回は `#tsumugi-dev`（`C0CBC97JPRR`）に手順書 §15 の「進行の報告」を 1 件出す。何もしなかった回は投稿しない
- 上限（手順書 §14.3）に達したら、その時点で止め、状況を PR（無ければ issue）に残して報告する

## スクリプトを直すとき

- `state.py` は loop-guard §6・§7・§8 と手順書 §4・§5・§8 を機械化した補助である。正本は文書の側で、食い違ったら文書に合わせて直す
- 試験: `python3 -m unittest discover -s .claude/skills/loop-advance/scripts`（CI の `Loop scripts (python)` でも走る）

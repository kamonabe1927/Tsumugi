---
name: loop-morning
description: Tsumugi 開発ループの「朝の点検」（毎朝 1 回の定期タスク）を実行するときに使う。前日のマージをマニフェストに照らして読み直す事後点検、文書と実装の整合点検、日報をまとめ、見逃しがあれば取り消しの PR を出し、Slack に朝のまとめを 1 件出す。
---

# loop-morning: 朝の点検

規則の正本は `docs/loop-guard.md`（凍結。最優先）と `docs/loop-runbook.md`（以下「手順書」）の §11・§12・§15。
「前日」は JST の 0:00〜24:00 である。時間の上限は 45 分（手順書 §14.3）。

この回は**見つけて記録する回**である。直してよいのは、事後点検で見逃しが見つかったときの取り消しの PR（loop-guard §4.4）だけで、
それ以外のずれは `loop:drift` issue に記録し、直すのは進行の回に任せる。

## 1. 準備

1. 手順書 §2 のとおり clone し、アカウントを確かめ、`python3 .claude/skills/loop-advance/scripts/state.py` を実行する。`halt` が空でなければ何もせずに終了する
2. 読む: tsumugi の `CLAUDE.md`、`docs/loop-guard.md`、`docs/manifesto.md`、`docs/documentation-guide.md`、手順書 §11・§12・§15

## 2. 事後点検（手順書 §11）

1. 前日にマージされた PR を集める: `gh api "repos/kamonabe1927/Tsumugi/pulls?state=closed&sort=updated&direction=desc&per_page=100"` から `merged_at` が前日のもの
2. PR ごとに、事後点検役を Agent ツールで起動する（`model: "opus"`、前景）。渡すのは PR 番号・マージ commit・clone したパス・「`.claude/skills/manifesto-gate/SKILL.md` の §1（事後点検の読み方）と §2 の表で、マージされた差分を判定する。審査コメントは先に読まない。PR にはコメントを書かず、表と疑いだけを返す」という依頼だけ。
   審査が代替（`（代替: Opus）`）だった PR は、全差分を必ず読ませる。判断が代替だった PR・issue は、判断の結論が手順書 §7.3 と loop-decide の比べ方に合っているかも読ませる。数が多ければ代替のものを先にし、45 分に収まらない分は次の朝に回して要確認に書く
3. あわせて PM（このセッション）が、マージの根拠が PR に残っているかを確かめる（loop-guard §6: レビュー・審査のコメント、3 時間待ち、1 日の上限、`保留:`、凍結ファイルの許可）。PR を経ずに main に入った commit が無いかも見る（見つけたら取り消さず、要確認に書く）
4. 見逃しが見つかったら、取り消しの PR を出す
   - ブランチ `revert/<PR番号>` で `git revert -m 1 <merge commit>` と `slices.toml` の変更を 1 commit にまとめ（loop-guard §4.4 の形）、`loop:revert` と `loop:running` を付ける
   - 取り消す差分が凍結ファイルを含むときは、PR を出したうえで要確認に書く（kamonabe の `frozen-ok` が要る）
   - 本文に、事後点検の表・どの原則にどう反するか・`Refs #<元の PR>` を書く
   - レビュー役（`opus`、loop-review）と審査役（`fable`、manifesto-gate）を起動し、終わったら `loop:running` を外す。マージは進行の回が行う

## 3. 整合点検（手順書 §12）

手順書 §12.1 の 1〜4 を点検し、§12.2 のとおり `loop:drift` issue に記録する。輪番の精読は、`loop:drift` issue の最後の点検コメントにある「次は <spec>」から読む（無ければ最後に閉じた `loop:drift` issue、それも無ければファイル名順の先頭）。
判断が要るずれは、手順書 §7.2 の形で選択肢を issue に書く（決めるのは進行の回の判断役）。

## 4. 日報とまとめ

前日の動きを集める。

- 進行の回の `#tsumugi-dev` 報告（何もしなかった回は投稿しないので、件数は Claude_Code_Remote の定期タスクの実行記録 `list_triggers` が使えればそれで確かめる。読めなければ「確認できず」）
- 開いているループの PR と、それぞれ何を待っているか（`state.py` の結果）
- `blocked` にしたスライスと原則（前日の `slices.toml` の変更）、`#tsumugi-loop` の判断の通知の件数
- 代替した審査・判断。同じ種類で 3 回続いていれば要確認

`#tsumugi-dev`（`C0CBC97JPRR`）に手順書 §15 の「朝のまとめ」を 1 件だけ出す。何も無い日も出す。

# Tsumugi 実行予算・協調実行仕様

最終更新: 2026-11-20

設計ステータス: **到達目標の確定仕様（§1〜§13）＋ 実装の現在地（下記の逸脱一覧）**

本文書の §1〜§13 は **到達目標の確定仕様**であり、現在の実装状態ではない。現在の実装が目標仕様からどう
逸脱しているか・何が未実装かは、下の **逸脱一覧** と **未実装範囲** の 2 表が唯一の正本である（状態の正本は
この 2 表に置き、§14 のスライス小節は再掲しない）。実装スライスの進捗の正本は [`slices.toml`](slices.toml)。

2 表は現在の逸脱と未実装範囲だけを持つ。done にする実装 PR は、この 2 表に新しい逸脱・未実装範囲を追加し、
解消した行を削除する（どのスライスで解消したかはスライス小節と git 履歴に任せる。[ドキュメント運用ガイド](documentation-guide.md)
「逸脱一覧の持ち方」）。2 表は tree-expr-continuation-e1 の実装 PR で新設した。それ以前に完了したスライスの
逸脱・未実装範囲は、事実を確かめられたものから順に載せる。

逸脱一覧の `ExecutionRequest`（§3）の行は loop:drift #288 の D-15、handle API（§9.1）の 4 行は D-16 を受けて載せた。
どちらも本文（目標の §3・§9.1）を実装に合わせて後退させず、実装との差を逸脱として載せる案を選んだ（D-15 は #288 の
Q1=A、D-16 は Q2=A。いずれも返信なしのため推奨案で確定した、[開発ループ手順書](loop-runbook.md) §7.5）。理由: spec が
実装の正本であり、現在の逸脱は事実を確かめて逸脱一覧に置く（ドキュメント運用ガイド「逸脱一覧の持ち方」。#263 の D-8、
#282 の D-12・D-13 と同じ扱い）。§3 の `execution_id` を必須にする目標は、host 供給の実行 ID を全 execution の監査
envelope に置く [決定性・実行時監査仕様](determinism-and-audit.md) §7.1 が前提にしており、§3 を実装の `new(budget, clock)`
に合わせると弱まる。deadline を効かせる clock の持ち方は §3 が「注入した `MonotonicClock`」として別に定めており、目標側を
変えずに残せる。§9.1 の handle API と facade（`src/embedding.rs`）の統合は [組み込みAPI仕様](embedding-api.md) の E10
の範囲で、目標を先に後退させると統合の受け皿が消え、budget・capability を持たない handle が §3・§4.2・§12 と矛盾する。
handle 経路を §3 の `ExecutionRequest` と組み込みAPI仕様 §8 の `ExecutionOutcome` へ寄せる `fix` スライスは、この判断では
足していない（#288 の Q2 の選択肢 C を選ばなかった。公開 API の breaking 変更で、E10 との順序を決める必要があるため）。

## 逸脱一覧（目標仕様 §1〜§13 に対する現在の逸脱）

| 逸脱内容 | 対象の節(§) | 発生スライス |
|---|---|---|
| tree evaluator の `sort`（revision 0.21 以降）の比較の段（`sort_by` と比較ごとの `compare_budgeted`、`src/builtin_core.rs` の `bulk_step`）は 1 段で実行し、途中で yield しない。slice の使用量は台帳の commit 済み fuel の差分で数える（fuel-bulk-chunk-yield の Q1 = A）ので builtin の per-element fuel も slice に入るが、比較の段 1 回分だけは slice を超えうる（§4.2 の超過例外。`max_collection_elements` で有界）。中断可能なソートへの書き換えは比較回数・比較順と fuel 値が変わるため行っていない（fuel-bulk-chunk-yield の Q3 = A） | §4.2 | Slice 3 PR-d |
| Dict の heap の delta 課金（`Value::retrack_dict`、`src/value.rs`）は、挿入・上書きのたびに全 key の byte 長を合計し直す。VM の Dict リテラルの構築（`OpCode::DictInsert`）と、index 代入による Dict の構築（tree・VM）は entry 数 × 挿入の回数の時間になり、fuel（文・反復・呼び出し）に比例しない。重複した key を含むリテラルをループで評価したときの時間と fuel の比も、深度の読み直しを無くした後（fix-vm-dict-literal-depth-settle）もこのために source の大きさに比例して開く。fix スライス fix-dict-retrack-key-bytes（todo）で直し、issue #374 の「時間が fuel に比例する」受入基準もそこに置く（判断役が選択（PR #447）、Q1=A） | §5.1 | REV-015 Slice 2（heap accounting） |
| `ExecutionRequest`（`src/embedding.rs` の `ExecutionRequest`、crate root では `EmbeddingRequest`）の field は private で、目標に無い `clock: Arc<dyn MonotonicClock>`（`budget.deadline` を効かせる clock）を持つ。`execution_id` は `Option<ExecutionId>` で、`with_execution_id` で設定し、監査 sink を設定した engine で実行するときだけ必須（未設定なら script work 前に fail-closed、sink 無しなら無視）。`arguments` は `Arc<[String]>` でなく `Vec<String>`。constructor は目標の `new(execution_id, capabilities, budget)` でなく `new(budget, clock) -> Result<Self, ConfigError>` で、capability は `with_capabilities`（既定は `CapabilitySet::empty`）、引数は `with_arguments(Vec<String>)`（目標は `arguments(impl Into<Arc<[String]>>)`）で設定する。`cancellation` builder は目標どおり | §3 | REV-015 最終形移行 Slice 1・Slice 2、Phase 6 A-1（`execution_id`） |
| scheduler（`src/engine.rs`、crate root の `Engine`）の handle 経路の `ExecutionRequest`（`src/engine.rs` の `ExecutionRequest`、crate root では `ExecutionRequest`）は field を持たない骨格（`new()` / `Default` だけ）で、§3 の `ExecutionRequest` と別型である。budget は context 内の評価器が持つ `BudgetConfig` を使い、capability・cancellation token・実行 ID を request で渡せない | §3/§9.1 | Slice 3 PR-a |
| `Engine::create_execution`（`src/engine.rs`）に `link_options: LinkOptions` 引数が無い（`LinkOptions` 型も無い）。`Engine::start` は `&LinkedScript` でなく `&CompiledScript`（`src/engine.rs` の `CompiledScript`）を取り、Link を初回 poll で行う。`create_execution` との違いは開始 phase（Created / Linked）だけである | §9.1 | Slice 3 PR-a |
| `PollSlice.max_fuel`（`src/engine.rs`）は `NonZeroU64`（16 以上）でなく `u64` で、0 や 16 未満を型で排除しない（field は `pub` で、構築時の検査を通らない）。既定 10,000 は目標どおり | §9.1 | Slice 3 PR-a |
| `PollResult::Terminal` の `outcome` は [組み込みAPI仕様](embedding-api.md) §8 の `ExecutionOutcome` でなく、`src/engine.rs` の別の `ExecutionOutcome`（crate root では `ExecutionOutcome`）で、variant は `Completed` / `RuntimeError` / `LinkError` / `Cancelled` / `DeadlineExceeded` の 5 つだけである（予算超過は `RuntimeError` に含まれ、`Denied` / `Exited` / `HostError` / `BudgetExceeded` などが無い。variant は `usage` を持たず、最終 usage は `PollResult::Terminal.usage` で観測する）。scheduler の `Engine` が監査 sink を持たない点は [決定性・実行時監査仕様](determinism-and-audit.md) の逸脱一覧に B-1 としてある | §9.1 | Slice 3 PR-a |
| scheduler の handle 経路の link 失敗は、alpha の `ExecutionOutcome` に `Denied`・`HostError` などの変種が無いため、cancel は `Cancelled`、deadline は `DeadlineExceeded`、それ以外は `LinkError { error }`（REPL の入力は `RuntimeError`）の `TsumugiError` に写る。resolver 未 grant の import は error_type `capability`、import 先のparse失敗は §3.4 のテンプレートに parse の cause を保持した `import`、link 時の予算超過は budget 系の error（判断役が選択（PR #480、Q1=A）。semantic-decisions §3.4）。解決の失敗は `モジュールを読み込めません: {code}`、循環は `import.cycle`（`{chain}`）、深度は `import のネストが深すぎます (上限: {limit})`（判断役が選択（PR #480、Q2=A））。構造化した `LinkError` を返す形は E10 で解消する | §1.1・§9.1 | fix-handle-import-from-link |
| admission queue から active へ昇格した handle の waker を鳴らさない。§9.1 は admission 取得で状態を ready にした後で waker を呼ぶとするが、active slot を解放する `release_active`（`src/scheduler.rs`）は queue 先頭の entry を active へ付け替えたあと、その entry の waker を placeholder（`QueuedEntry::waker: Option<()>`。登録する経路が無く常に `None`）として退避するだけで鳴らさない（コメント「placeholder: FEAT-002 が waker.wake() を行う」）。昇格は host が次に poll したとき（`poll` の step 3）に観測する。loop:drift #429 D-115。fix スライスは無い（未設計） | §9.1 | — |
| `CancellationToken::register_waker`（`src/budget.rs`）は waker を保存するだけで、保存の後に `is_cancelled()` を確かめ直さない。`ExecutionHandle::set_waker`（`src/engine.rs`）はこれを呼ぶだけなので、`set_waker` より先に cancel された handle には wake が来ない。§9.1 は登録を「waker保存→ready flag再確認→readyなら即wake」の順とする（ticket 側の `HostCallTicket::register_waker`、`src/host_pending.rs` は確かめ直している）。コードを読んで確かめた（実行での再現はしていない。再現の確認は fix の設計で行う）。fix スライス fix-cancel-token-waker-recheck（todo、未設計）で直す（判断役が選択、PR #451 の Q1=A）。loop:drift #429 D-139 | §8/§9.1 | Slice 5 |
| 同期の `Engine::execute`・`execute_repl_submission`（`src/engine.rs`、crate root の `Engine`）は、同じ `Engine` に他の nonterminal な handle があっても `StartError::ConcurrentRunRequiresPolling` を返さずに開始する。terminal まで poll する `drive_to_outcome` は `Yielded` で `continue` するだけなので、run-turn queue の先頭でない handle は `SchedulerPreempted` の `Yielded` を受け続け、呼び出しが戻らない（例: 同じ `Engine` で handle h1 を `max_fuel: 100` で 1 回 poll して `Yielded` にしたあと、別の context で `execute` を呼ぶ）。variant は `src/scheduler.rs` に定義済みだが検出は配線しておらず、その doc の「同期入口は構造的にこの状況を起こさない」は事実と異なる。fix スライス fix-sync-run-concurrent-handle で直す（`src/scheduler.rs` の variant の doc の誤りも同じスライスで直す。loop:drift #373 D-69） | §9.2 | Slice 3 PR-a（Slice 5 の run-turn FIFO で顕在化） |
| scheduler（`src/engine.rs`、crate root の `Engine`）の handle 経路の cancellation token は execution ごとに作られず、`ExecutionContext` の評価器が持つ 1 つの token（`BudgetLedger` の `cancellation`。`ExecutionContext::new` で作り、`create_execution`・`start`・`execute`・`execute_repl_submission` は据え直さない）を context の寿命の間使い回す。`ExecutionHandle::cancellation_token` はこの token の clone を返し、非 terminal の handle の `Drop` はこれを `cancel()` するので、poll していない `Created` の handle を drop しただけで、その context の以後の execution はすべて最初の checkpoint で `Cancelled` になる（同期の `execute` は `Err(実行がキャンセルされました)`）。前の execution の token の clone を host が保持していて後から `cancel()` した場合も、同じ context の次の execution が止まる。token に登録した waker も次の execution へ持ち越す。fix スライス fix-handle-execution-cancel-token で直す（issue #363） | §8/§9.1/§9.2/§10 | Slice 4 |
| scheduler（`src/engine.rs`、crate root の `ExecutionContext`）の `ExecutionContext` は、§13 の目標に無い公開メソッド `reset_step_budget()` を持つ。中身は `Evaluator::reset_step_budget` → `budget.reset_fuel()` で、同じ ledger の fuel だけを戻す（heap・I/O などの使用量は戻さない）。src・tests に呼び出し元は無い。facade の `embedding::ExecutionContext` には無い。fix スライス fix-scheduler-context-reset-step-budget で直す（loop:drift #373 D-71） | §13 | Slice 1（legacy adapter） |
| budget の使用量を execution ごとに新しく数えない経路がある。§13 は使用量を execution ごとに新しい `BudgetUsage` で数えるとし、§2 の total budget は execution 全体の上限だが、scheduler（`src/engine.rs` の `drive_one_slice` → `Evaluator::begin_execution`、`src/eval.rs`）は context の台帳を execution ごとに作り直さず、`ExecutionHandle::usage`・`PollResult` の usage（`finish_terminal`）と上限の判定が同じ context の前の execution の使用量を含む（同じ context で同じ script を 3 回走らせると fuel が 100・200・300 になる）。facade の `Engine::run`・`run_audited` は台帳を作り直さず、使用量を 0 から数え直して live heap を持ち越す（`rearm_budget_for_repl` → `BudgetLedger::begin_repl_submission`、`src/budget.rs`）。scheduler の差は fix スライス fix-execution-budget-usage-fresh（todo、未設計）で直す。挙動の直し方（execution ごとの台帳の作り直し）はその設計で決める（判断役が選択、PR #451 の Q1=A）。loop:drift #429 D-136 | §2/§9.1/§13 | Slice 3 PR-a（scheduler） |
| `for` の Dict・Str の反復列を作る `Value::new_iter_str_list`（`src/value.rs`）は List body の backing（`Vec::with_capacity(要素数)`）を、要素数の検査（`check_collection_elements`）のあと List body（24 + 32 × 要素数）の課金より前に確保する（List body は要素をすべて作ったあとで課金する）。確保は `max_collection_elements` で抑えられる | §5.2 | fix-for-iter-snapshot-charge |
| `builtin_core::dispatch` の builtin のうち新しい String body を作るもの（`upper` / `lower` / `trim` / `split` / `slice`（Str）/ `reverse`（Str）/ `format_time` / `keys` / `type`）と文字列の連結 `+` は、結果を作ってから長さを検査する（§5.3 の「確保の前に確かめる」に対する逸脱）。builtin は `dispatch` が結果を返したあと tree（`src/builtin.rs` の `apply_pure_core_builtin`）・VM（`src/vm.rs` の `exec_builtin`）の `track_result` → `charge_string` で、`+` は tree（`src/eval.rs` の `eval_binop` の結果）・VM（`src/vm.rs` の `binary_add` の結果）の `track_result` で検査する。ただし結果の長さは引数の数に依らず入力の定数倍で抑えられ、上限ちょうどの入力でも結果は `max_single_string_bytes` の約 6.5 倍（`format_time`）までにしかならない。コードで確かめた抑え: `trim`・`slice`（Str）は入力以下（`slice` は途中で文字数 × 4 byte の `Vec<char>` を作る）、`reverse`（Str）は入力と同じ、`split` の各要素は入力以下で要素数は先に `max_collection_elements` で検査する、`upper` / `lower` は Unicode の大小文字の対応で 1 文字が多くとも 3 文字に広がる入力の定数倍、`format_time` は書式の指定子ごとに固定長（`%Y` は多くとも 13 文字）の値へ置き換える書式の定数倍、`keys` の各要素は既存の key と同じ長さ（要素数は先に検査する）、`type` は固定の型名、`+` は両辺の合計で上限の 2 倍以下。再現（main 397a23f の debug ビルド、既定の budget、`ulimit -v 4000000`、tree-walk 版・VM 版とも同じ）: 上限ちょうど（8 MiB）の入力で、`upper`（2 byte の `ΐ`）は 25165824 バイト、`lower`（2 byte の `İ`）は 12582912 バイト、`format_time(-9000000000000000000, "%Y" の繰り返し)` は 54525952 バイト、`s + s` は 16777216 バイトで「文字列の長さが上限を超えました」（exit 1）、`split(s, "")` は要素数の上限で exit 1、`trim` / `slice` / `reverse` は成功し、どれも abort しない。`keys`（4 MiB の key 2 つ）と `type` も成功する。fix スライスは無い（PR #386 の Q2=A。issue #399 で `path_join` を外した） | §5.2/§5.3/§7 | REV-015 Slice 2（string accounting） |
| cooperative host function が返す `HostCallTicket<Value>` は `Send + Sync` でない。`HostCallTicket<T>`（`src/host_pending.rs`）は `Arc<TicketShared<T>>` を持ち `T: Send` のときだけ `Send + Sync` で、`Value`（`src/value.rs`）は `Rc` を含み `!Send` なので、別 thread からの complete はできない。同じ thread（adapter・clock・waker の callback など）か ticket を作った側が complete した結果だけを待てる。loop:drift #420 D-114 | §6.2 | Slice 5 |
| 登録 host function（同期・cooperative）の呼び出しは host-call 回数と request/response の byte を課金しない。§6.1 は任意の host call で capability 判定の前に `host_calls += 1` とするが、`charge_host_call_request`（`src/budget.rs`）を呼ぶのは filesystem builtin（`src/builtin.rs`・`src/vm.rs`）だけで、同期の `apply_host_call` と cooperative の `start_cooperative_call`（`src/eval.rs`）は呼ばない。`HostCost` の fuel・結果上限が使われない差は [Capability Model仕様](capability-model.md) の逸脱一覧（§11.2）にある。loop:drift #420 D-108。fix スライス fix-host-function-call-charge（todo）で直す（issue #364 の設計で filesystem builtin の fix と分けた） | §6.1 | C8 / Slice 5 |
| `env`・`now`・`exit` の呼び出しは host call として回数と byte を課金しない。§2 は clock・env・process の呼び出しを host call とし、§6.1 は任意の host call で capability 判定の前に `host_calls += 1` とするが、tree（`src/builtin.rs` の `eval_builtin` の Context の arm）と VM（`src/vm.rs` の `exec_builtin` の `env`・`now` などの arm）は Environment・Clock・ProcessExit の authority を引くだけで `charge_host_call_request` を呼ばない（`host_calls` を数えるのは、filesystem builtin の課金の 2 箇所の `charge_host_call_request` と、host counter を co-charge する stdio の `charge_input`・`charge_output`（`src/budget.rs`）だけ）。issue #364 の設計で事実を確かめた。fix スライスは無い（未設計） | §2/§6.1 | REV-015 Slice 2（I-O accounting）・C3・C7 |

## 未実装範囲（目標仕様 §1〜§13 のうち未着手・延期中）

| 未実装範囲 | 対象の節(§) | 備考 |
|---|---|---|
| closure の capture の連鎖（`f = fn() g() end` で前の関数値を capture し続ける）の破棄による host stack 枯渇の防止。関数値の深度は 0 と数える（[次期意味論・実装決定](semantic-decisions.md) §21.3）ため、値のネスト深度上限では防げず、20 万段の連鎖の破棄で tree-walk 版・VM 版とも abort する（exit 134） | [次期意味論・実装決定](semantic-decisions.md) §21 | fix-closure-capture-chain-stack |
| 関数本体・callback 本体の中での cooperative host-call の Pending yield と audit drain queue 上限の yield。どちらもトップレベル（driver 上の呼び出しの外）の文/反復境界だけで判定し、関数本体の中で Pending を返す adapter は `may_yield = false` の規約違反（catch 不能 `InternalControl`）になる | §4.2/§6.2 | §14「tree-expr-continuation 実装スライス」の逸脱（audit drain queue は [決定性・実行時監査仕様](determinism-and-audit.md) の逸脱一覧にも B-2 として記載） |
| callback なし bulk builtin の chunk 境界での slice yield の残り（fuel-bulk-chunk-yield で `join` / `contains`（List）/ `sort` の走査・描画キーは tree の poll 経路で 256 要素・16 KiB の chunk 境界を yield 可能点にした）。(1) per-element の fuel を課金しない bulk builtin（`range` / `keys` / `values` / `reverse` / `slice` / `split` / 文字列の繰り返し・連結 / 大きな collection のコピー等）は slice fuel で区切りが生じない。(2) スカラー要素（`Str` / `Int` / `Float` / `Bool` / `Null`）の List への `join` / `contains` と revision 0.20 以下の `sort` の描画キーは、要素の描画・等価判定が fuel を課金しないので chunk 境界があっても slice fuel では yield せず、1 回分を §4.2 の超過（collection / string 上限で有界）として扱う（Q4 = A）。(3) 1 つの値の描画（`to_str`、`join` の 1 要素）と `replace` / `path_join` は段階実行にしていない。chunk 前の確認は従来どおり各 charge の前の deadline / cancel だけ | §4.1/§4.2 | (1)(2) は §4.1 の per-element・UTF-8 の課金が入る F-2・`fuel-bulk-charge` の後（その課金が入れば (2) は chunk 境界が追加の仕組みなしで効く）、(3) は未設計 |
| VM の式レベル continuation（VM の slice yield と continuation parity） | §9.3 | Slice 6（VM が experimental の間） |
| script / host の明示 yield（`YieldReason::ExplicitYield`）。型はあるが trigger を配線していない | §9 | — |
| §4.1 課金表のうち UTF-8 payload（`ceil(bytes / 64)`）・AST node（1 / node）・`map` / `filter` / serialize の per-element の fuel。主要 charge point（statement / expression / operation / invocation 5 / loop iteration 2 / host call attempt 10）は F-2c で版 `2` として入ったが、これらは課金していない（per-element は比較・表示・`sort` だけ） | §4.1 | fuel-bulk-charge |
| reservation オブジェクト（§7 の `reserve` が返し `commit(actual)` / `refund()` で settle する型）と、未 settle の drop を release build でも `InternalFailure(UnsettledReservation)` にする規則 7。現状の `Budget::reserve_all` は `Result<(), ControlStop>` を返し、`commit(resource, reserved, actual)`・`refund(resource, reserved)` に量を渡す形で、未 settle は検出しない。`actual > reserved` は規則 3 の拒否でなく `debug_assert` と `min` による丸めで扱う | §7 | budget-reservation-settle |
| deadline の timer による waker の通知。§9.1 は deadline timer が状態を ready にした後で waker を呼び、host の busy-poll を不要とするが、deadline は poll の checkpoint（`src/engine.rs` の `checkpoint_terminal`）でしか観測せず、deadline に waker を結ぶ timer が無い（`src/engine.rs`・`src/scheduler.rs`・`src/host_pending.rs`・`src/budget.rs`）。`HostCallPending`・`AdmissionQueued` で待つ handle は、host が poll しない限り deadline の到達に気づけない。admission の昇格の wake（逸脱一覧の D-115 の行）とは別。loop:drift #429 D-138 | §9.1 | 未設計 |

## 1. 位置づけ

本文書は、[Tsumugi Manifesto](manifesto.md)と[ロードマップ](roadmap.md)のうち、マニフェスト実現ロードマップ Phase 3「包括的な実行予算」とPhase 4「協調実行と負荷制御」の実装仕様を定める。既存のstep上限、collection上限、call・AST・import深度上限を土台として再利用する。各スライスの状態は[`slices.toml`](slices.toml)、目標に対する現在の逸脱と未実装範囲は冒頭の逸脱一覧と未実装範囲の 2 表を見る。

本文書は次の既存仕様と一体で実装する。

- [組み込みAPI仕様](embedding-api.md): Phase 1/2のEngine、compile/link、source identity、terminal channelの先行契約
- [Capability Model仕様](capability-model.md): Phase 2のfilesystem、env、clock、stdio、process、host functionのdeny-by-default権限境界
- [決定性・実行時監査仕様](determinism-and-audit.md): Phase 5/6の規範backend、注入host、audit event、record/replay
- [次期意味論・実装決定](semantic-decisions.md): 次期言語挙動、CLI、canonical error、catch可否

本文書のbudgetはsecurity sandboxではない。敵対的scriptを扱う場合は、別process、container、cgroup、OSのCPU・memory・time制限を併用する。

### 1.1 Phase 1/2先行実装との統合規則

Phase 1/2は本書の最終公開型の内部subsetとして実装する。先行実装専用の公開budget/state/poll型を作らず、本文書がPhase 3/4の最終契約である。

- `ExecutionRequest.budget`は最初から本書の有限`BudgetConfig`とし、独立したoptional deadline fieldや無制限sentinelを設けない。Phase 1/2でmeter未接続部分を内部bootstrapとして段階実装しても、その型をpublic stable APIにしない。
- `ExecutionState`は本書の`Created` / `Linked` / `Ready` / `Running` / `Yielded(YieldReason)` / `Paused(PausedState)` / `Terminal`だけを公開する。`Suspended`等の先行enumを作らない。
- `ExecutionOutcome`はterminal payloadの正本として維持し、`ExecutionState::Terminal`から同じoutcomeを参照する。stateとoutcomeでterminal理由を二重定義しない。poll中のlink失敗を表す`ExecutionOutcome::LinkError { error: LinkError, usage: BudgetUsage }`を使う。resolver denial/host error、link中budget/deadline/cancelはそれぞれ`Denied` / `HostError` / `BudgetExceeded` / `DeadlineExceeded` / `Cancelled`へmapし、module parse/compile、cycle、depth、symbol/arity mismatchだけを`LinkError`にする。
- Phase 1の`Engine::start(&LinkedScript, ...)`はLinkedから開始する互換入口として残せる。source/import予算を含む入口は`Engine::create_execution(&CompiledScript, ...)`でCreatedから開始し、linkを同じhandleで進める。
- Phase 1のcontext/handleに対する`!Send + !Sync`保証を維持する。engine内部scheduler stateとwake handleだけを`Send + Sync`にし、公開`ExecutionHandle`を別threadへ移動しない。
- AUD-024は本文書第10節の全language-state transactionを最初から最終契約とする。prefix commitや「terminal failureでもrollbackしない」という先行公開契約を設けない。

これらは同一実装系列で切り替え、同じbuildへ旧型と最終型、旧状態機械と最終状態機械を併存させない。

## 2. 用語と不変条件

| 用語 | 定義 |
|---|---|
| execution | 1個のroot scriptを、1個の`ExecutionContext`と不変の設定でterminalまで進める単位 |
| total budget | execution全体で消費できる資源上限。yield・pause・host call待ちを跨いで補充しない |
| slice | 1回の`poll`で実行を許可する量。公平性のための量子であり、total budgetではない |
| fuel | backend非依存の論理実行量。実時間やVM opcode数そのものではない |
| heap | executionから到達可能な論理allocationのlive byte数。OS allocatorの実測値ではない |
| reservation | 副作用・allocation前に上限内の枠を確保した状態 |
| terminal | `Completed`、`Exited`、`Denied`、`LinkError`、`RuntimeError`、`HostError`、`BudgetExceeded`、`DeadlineExceeded`、`Cancelled`、`AuditFailure`、`InternalFailure`、`RecordFailure`、`ReplayMismatch`のいずれか。terminal後は再開できない |
| yield | 非terminalの協調停止。continuationを保持し、再度`poll`できる |
| pause | hostの明示要求による非terminal停止。`resume`までschedulerへ戻さない |
| host call | capability境界を越えるclock、env、stdio、filesystem、process、登録host function等の呼出し |

次を全実装の不変条件とする。

1. すべてのexecutionは有限の`BudgetConfig`を持つ。無制限値を表すsentinelは提供しない。
2. total budgetのusageはyield・pause・resumeを跨いで単調に維持する。live heapだけは解放により減少できる。
3. budget超過、deadline、cancel、audit失敗はscriptの`try` / `catch`から捕捉できない。
4. terminal遷移は1回だけであり、terminal後の`poll`、`resume`、context変更は`HandleError::Terminal`を返す。
5. backend内部の実装量ではなく、本文書の論理charge pointを課金する。treeとVMで同じscript・同じ入力・同じbudgetなら同じcharge列になる。
6. capability、budget上限、deadline、注入host、redaction policy、backend、root source、linked importはexecution作成後に変更しない。

## 3. 公開データ型

以下は実装時にそのままRustの公開型へ落とす擬似定義である。field名と単位は公開契約とする。

```rust
pub struct BudgetConfig {
    // 論理accounting契約（§5.1の論理サイズ表の版）。§3.1のとおり1だけを受理する。
    pub heap_accounting_revision: u32,
    // §4.1課金表の版（§3.1）。
    pub fuel_accounting_revision: u32,

    // 論理fuel unit
    pub total_fuel: u64,

    // logical byte / count
    pub max_live_heap_bytes: u64,
    pub max_string_allocations: u64,
    pub max_string_bytes: u64,
    pub max_single_string_bytes: u64,
    pub max_source_count: u32,
    pub max_source_bytes: u64,
    pub max_single_source_bytes: u64,
    pub max_import_count: u32,
    pub max_import_bytes: u64,
    pub max_collection_elements: u64,

    // call count / payload byte
    pub max_input_calls: u64,
    pub max_input_bytes: u64,
    pub max_output_calls: u64,
    pub max_output_bytes: u64,
    pub max_host_calls: u64,
    pub max_host_request_bytes: u64,
    pub max_host_response_bytes: u64,
    pub max_host_call_bytes: u64,

    // 注入MonotonicClockと同じclock domainの絶対時刻。単位はns。
    pub deadline: MonotonicInstant,
}

pub struct BudgetCounters {
    pub fuel: u64,
    pub string_allocations: u64,
    pub string_bytes: u64,
    pub source_count: u32,
    pub source_bytes: u64,
    pub import_count: u32,
    pub import_bytes: u64,
    pub input_calls: u64,
    pub input_bytes: u64,
    pub output_calls: u64,
    pub output_bytes: u64,
    pub host_calls: u64,
    pub host_request_bytes: u64,
    pub host_response_bytes: u64,
    pub host_call_bytes: u64,
}

pub struct BudgetPeaks {
    pub single_string_bytes: u64,
    pub single_source_bytes: u64,
    pub collection_elements: u64,
}

pub struct BudgetUsage {
    pub committed: BudgetCounters,
    pub reserved: BudgetCounters,
    pub live_heap_bytes: u64,
    pub reserved_heap_bytes: u64,
    pub peak_heap_bytes: u64,
    pub peaks: BudgetPeaks,
}

pub struct BudgetExceeded {
    pub resource: BudgetResource,
    pub limit: u64,
    pub used: u64,
    pub reserved: u64,
    pub requested: u64,
    pub unit: BudgetUnit,
    pub phase: ExecutionPhase,
}

pub enum BudgetUnit {
    Fuel,
    Bytes,
    Count,
    Elements,
}

pub enum BudgetResource {
    Fuel,
    HeapBytes,
    SingleStringBytes,
    StringAllocations,
    StringBytes,
    SingleSourceBytes,
    SourceCount,
    SourceBytes,
    ImportCount,
    ImportBytes,
    CollectionElements,
    InputCalls,
    InputBytes,
    OutputCalls,
    OutputBytes,
    HostCalls,
    HostRequestBytes,
    HostResponseBytes,
    HostCallBytes,
}

pub enum ExecutionPhase {
    Compile,
    Link,
    Run,
    HostCall,
    Commit,
}

#[derive(Clone)]
pub struct ExecutionRequest {
    pub execution_id: ExecutionId,
    pub capabilities: CapabilitySet,
    pub arguments: Arc<[String]>,
    pub budget: BudgetConfig,
    pub cancellation: CancellationToken,
}

impl ExecutionRequest {
    pub fn new(
        execution_id: ExecutionId,
        capabilities: CapabilitySet,
        budget: BudgetConfig,
    ) -> Result<Self, ConfigError>;
    pub fn arguments(self, values: impl Into<Arc<[String]>>) -> Self;
    pub fn cancellation(self, value: CancellationToken) -> Self;
}
```

`ExecutionRequest`は必ず有限の`BudgetConfig`を所有する。deadlineは`BudgetConfig.deadline`だけに存在し、requestに独立したoptional fieldを持たせない。constructorはclock domain、accounting revision、deadlineが作成時点より後であることを検証する。

resourceを3種類に分類する。

- **cumulative**: Fuel、StringAllocations/Bytes、SourceCount/Bytes、ImportCount/Bytes、Input/Output/Host各count/bytes。成功時に`committed`へ加算し、通常は減らさない。
- **live**: HeapBytes。reserve/commit/releaseと`live_heap_bytes` / `peak_heap_bytes`で管理する。
- **per-item**: SingleStringBytes、SingleSourceBytes、CollectionElements。各1 objectのcandidate size/cardinalityだけを上限と比較し、成功時は累積加算せず`BudgetPeaks`を`max(old, candidate)`で更新する。文字列・sourceは同時に対応するcumulative bytesへ、collection backingはheapへ課金する。

`max_collection_elements`は**1個のListまたはDictの要素数**上限であり、execution内で生成した全要素数の累積ではない。List/Dict literal、push、新規Dict key、map/filter等の結果を変更前のcandidate cardinalityで検査する。Stringはcollection扱いせずsingle/string bytesで制御する。

per-item超過時の`BudgetExceeded`は`used = 0`、`reserved = 0`、`requested = candidate size/cardinality`、`limit = per-item limit`とする。成功したper-item検査は`BudgetCharged`を発行せず、Terminal/Yieldedの`BudgetUsage.peaks`で観測する。したがって監査delta総和との一致要件はcumulativeとlive resourceに適用し、per-itemはpeak一致を検査する。

`MonotonicInstant`は注入した`MonotonicClock`だけが生成するopaqueな`u64` nanosecond tickである。異なるclock instanceから作られたdeadlineを渡した場合、execution作成を`ConfigError::ForeignClock`で拒否する。`now >= deadline`をdeadline到達とする。pause、admission queue、host call待ちの時間もdeadlineに含める。

### 3.1 既定値

`BudgetConfig::standard(clock)`は`clock.now() + 30 s`をdeadlineとし、次を設定する。addition overflowまたはclock errorならconfig生成を失敗させる。

| field | 既定値 |
|---|---:|
| `heap_accounting_revision` | 1 |
| `fuel_accounting_revision` | 2 |
| `total_fuel` | 20,000,000 fuel |
| `max_live_heap_bytes` | 64 MiB |
| `max_string_allocations` | 1,000,000 |
| `max_string_bytes` | 64 MiB |
| `max_single_string_bytes` | 8 MiB |
| `max_source_count` | 1,025（root 1 + import 1,024） |
| `max_source_bytes` | 16 MiB |
| `max_single_source_bytes` | 2 MiB |
| `max_import_count` | 1,024 |
| `max_import_bytes` | 16 MiB |
| `max_collection_elements` | 1,000,000 |
| `max_input_calls` | 10,000 |
| `max_input_bytes` | 8 MiB |
| `max_output_calls` | 10,000 |
| `max_output_bytes` | 8 MiB |
| `max_host_calls` | 10,000 |
| `max_host_request_bytes` | 8 MiB |
| `max_host_response_bytes` | 16 MiB |
| `max_host_call_bytes` | 24 MiB |

`MiB`は1,048,576 byteである。`heap_accounting_revision != 1`は`ConfigError::UnsupportedHeapAccountingRevision(u32)`でexecution作成前に拒否する。すべての上限は0を許し、該当操作を最初から禁止できる。deadlineだけは作成時点より後でなければならない。

`fuel_accounting_revision`は§4.1の課金表（charge pointと課金値）の版である。`heap_accounting_revision`が§5.1の論理サイズ表の版であるのと対称で、`BudgetConfig`が`u32`で保持し、`BudgetConfig::standard`は既定を現行版にする。課金表を変える変更（charge pointの追加・課金値の変更）はこの値を1つ上げ、同時に受理する版を新版だけへ更新する（`heap_accounting_revision`と同じく、受理しない版はexecution作成前に`ConfigError`で拒否する）。会計版の不正は、どのfield（heapかfuelか）が不正かを呼び出し側が区別できる形のconfig errorで返す。heapは`ConfigError::UnsupportedHeapAccountingRevision(u32)`、fuelは`ConfigError::UnsupportedFuelAccountingRevision(u32)`で、値は指定された版である（§14「F-2 の sub-slice 分割」の判断Q3=A。判断役が選択（PR #470））。現在の差は冒頭の未実装範囲を見る。旧版の課金を再現する互換経路は持たない（alphaにつき後方互換無保証、旧版指定は拒否されるのであって旧課金で動くのではない）。fuelの会計版をこの専用fieldに分離するのは、`language_revision`（§17.8の全順序化などscriptから観測できる言語挙動の版）と会計の版を混ぜないためである。したがってfuel課金の変更で`language_revision`はbumpしない。初期版は`1`（F-1までの課金）。現在版は`2`で、§4.1の主要charge point（statement・expression・operation・invocation・loop iteration・host call attempt）の課金である（§14「F-2c 実装スライス」）。既定`total_fuel`は版`2`の課金で代表script集合を測り、それが通る最小の切りの良い値にした。§4.1の「List / Dictの深度の読み直し」は版`1`に含める（そう決めた理由は§14「fix-value-depth-settle-quadratic 実装スライス」の決定記録を見る）。

## 4. Fuel課金

### 4.1 課金表

| charge point | charge | 備考 |
|---|---:|---|
| statementへ入る | 1 | 到達しないstatementは課金しない |
| expressionへ入る | 1 | literal、変数参照、call式を含む |
| unary / binary / compare / index / assignmentの論理operation | 1 | operandのexpression chargeとは別 |
| language-level function invocation | 5 | user function、lambda、core builtin、callbackで共通 |
| loop iteration開始 | 2 | condition・iterable・bodyのchargeとは別 |
| host call attempt | 10 | 許可・拒否・host errorのいずれでも課金する |
| collection elementを走査・copy・生成 | 1 / element | `map`、`filter`、`sort`、比較、serialize等 |
| List / Dictの深度の読み直し | 1 / 読み直した子 | index代入・`pop`で最大深度の子がいなくなり、残った子（Listの要素・Dictの値）の保存済み深度を全部読み直すときだけ（[次期意味論・実装決定](semantic-decisions.md) §21.3）。値を変える前（COWの複製の前）に課金する。同じ深度の子が残る取り除きと、値を作るときの深度の要約・検査は0 |
| UTF-8 payloadをscan・copy・encode/decode | `ceil(bytes / 64)` | 0 byteは0。string、source、I/O、host payloadに適用 |
| AST node生成またはlinked node複製 | 1 / node | compile・import link中 |
| VM `Charge` opcode dispatch | 0 | opcode payloadに上記論理chargeをencodeする |
| その他のVM内部opcode dispatch | 0 | lowering差をscriptのfuel差にしない |

VM compilerはstatement、expression、operation、function、loop、bulk workの境界へ`Charge` opcodeを挿入する。tree evaluatorは同じ共有`FuelSchedule` APIを呼ぶ。compiler optimizationでcharge pointを削除・併合してはならない。VM固有のstack操作やjump数は課金しない。

`ceil(bytes / 64)`は`bytes == 0 ? 0 : 1 + (bytes - 1) / 64`でoverflowなしに計算する。collection/string builtinは長い処理を最大256 elementsまたは16 KiBごとの小chunkへ分け、chunk前にfuelとcancel/deadlineを確認する。

### 4.2 total fuelとslice fuel

`total_fuel`はterminalまでの上限であり、`poll`ごとの`PollSlice::max_fuel`は公平性の量子である。

- 次の論理chargeがtotal残量を超える場合は`BudgetExceeded(Fuel)`へterminal遷移する。
- total残量はあるがslice残量を超える場合はchargeせず`Yielded(SliceFuelExhausted)`を返す。
- 1個の固定chargeを分割しない。`PollSlice::max_fuel`は16以上とし、bulk workは前述のchunkへ分割する。
- slice終了でtotal fuelは補充しない。slice内で実際にcommitしたfuelだけをtotal usageへ加える。

#### yield 可能点に挟まれた区間の slice 超過（§14 Slice 3 PR-e）

tree evaluator は yield 可能点でだけ slice 境界を確認する。関数呼び出しと反復は明示 frame 上で
段階実行し、その文/反復境界は必ず yield 可能点にする（§14 Slice 3 PR-e）。したがって yield 可能点に
挟まれて途切れずに実行される区間は、**呼び出しも反復も含まない区間**に限られる。この区間の中で
slice 残量を使い切った場合は、区間の途中では yield せず、**区間を抜けた最初の yield 可能点で
`Yielded(SliceFuelExhausted)` を返す**（それまでは slice を超過して実行してよい）。これは
「charge せずに yield する」原則の、呼び出しも反復も含まない有界区間に限った例外である。

- **超過量の上限**: この区間 1 つで超過しうる fuel は、次の 2 つの和で上限が決まる。
  1. **区間が含む AST node 数**（呼び出しも反復も含まない部分式の大きさ。`MAX_AST_DEPTH` と
     source/collection 上限で有界）。
  2. **区間内の callback を持たない bulk builtin 1 回分**（`sort` / `join` / 文字列の繰り返し /
     大きな collection のコピー等）の fuel。入力サイズが `max_collection_elements`（既定 1,000,000）・
     `max_single_string_bytes` 等の **collection / string 上限で有界**なので、1 回分の fuel も有限値に
     なる。ただし **fuel 量そのものは入力要素数 n を超えうる**（`sort` は要素数ぶんの走査課金 n に
     加えて比較ごとの per-element 課金で O(n log n)、`join` 等の表示系も per-node 課金）。既定 slice
     （例 10,000）の数十〜数百倍になりうるが、config の collection / string 上限が入力サイズを抑える
     ことで最悪の fuel 量が有限に決まる。
  呼び出し・反復はこの区間に入らないため、再帰やループで超過量が無制限に積み上がることはない。
- **bulk builtin の chunk 境界での yield**: §4.1 は bulk work を 256 element / 16 KiB の chunk に分けると
  定める。chunk 境界で slice yield しない bulk builtin は、1 回分を上記 2 の有界な超過として扱う。
  chunk 境界での slice yield の実装状態は冒頭の未実装範囲（fuel-bulk-chunk-yield の行）を見る。
- **根拠（manifesto 原則 1・4）**: slice yield の目的は「ホストへ制御が戻るまでの最悪時間を有界に
  保つ」ことである。呼び出し・反復を必ず yield 可能点にすれば、最悪時間は「1 区間の有界な超過
  （AST 分 + bulk builtin 1 回分）+ slice」で抑えられる。区間の内部まで細かく中断できることは要件
  ではないため、Rust 再帰を一部残すハイブリッド実装（§14 Slice 3 PR-e の 1b）を許す。

## 5. Heap accounting

### 5.1 論理サイズ

heap quotaはallocator、platform、Rust compilerに依存しない論理サイズを使う。以下の定数を`HEAP_ACCOUNTING_REVISION = 1`として固定する。

| allocation | logical bytes |
|---|---:|
| `Value` slot / captured cell | 32 |
| UTF-8 `String` body | 24 + byte length |
| `List` body | 24 + 32 × element count |
| `Dict` body | 24 + 64 × entry count + 各keyのUTF-8 byte length |
| tree function instance | 64 + 16 × captured cell reference count |
| VM function instance | 48 + 16 × upvalue reference count |
| AST program root | 64 |
| AST node | 64 + nodeが所有するidentifier/string literalのbyte length |
| bytecode chunk | 64 + 16 × opcode count + 32 × constant slot count |
| imported module record | 96 + normalized module IDのUTF-8 byte length |
| continuation frame | 96 + frameが所有するlocal slot 32 × count |
| exception handler / loop handler | 32 |
| rollback journal entry | 48 + 保持する旧valueの到達payload |

continuation frameの課金対象は、スコープを作るframe（96 + local slot 32 × count）だけである。Call frame・ループ制御frame・式frameは、同期経路のRust再帰が持つ状態をheapへ移したものなので課金しない（0）。

`Value::Str`はValue slotとString body、`Value::List`はValue slotとList body、そこから初めて到達する子payloadを課金する。Dict keyはentryの64 bytesにString header 24 bytesを含むため、追加するのはkey payload byteだけである。

### 5.2 `Rc`共有と二重計上回避

すべてのheap-owned objectへ単調増加する`AllocationId(u64)`を付け、executionごとに`AllocationLedger`を持つ。同じ`ExecutionContext`の次のexecutionは、前のexecutionの台帳を据え直して使ってよい（使用量と上限は新しいexecutionのものにし、contextに残る値のlive heapだけを引き継ぐ。下の末尾の段落）。

- 同一execution内で同じ`AllocationId`へ複数の`Rc`参照があっても1回だけ課金する。
- `Rc::clone`、closure capture、変数代入で既存objectを共有するだけなら追加課金しない。
- copy-on-write、string連結、collection拡張で新objectを作る場合は新しい`AllocationId`を発行し、allocation前に全logical bytesをreserveする。
- objectへの最後のexecution内参照が消えた時点でlive heapをreleaseする。cycle等により到達可能性が残るobjectはreleaseしない。
- 物理artifactを複数executionが共有しても、各executionは自分のquotaへ1回ずつ論理サイズを課金する。一方のexecutionのbudgetを他方が肩代わりしない。
- `AllocationId`の発行overflowは`InternalFailure(AllocationIdExhausted)`であり、0へwrapしない。
- `for`の反復列もこの規則に従う。Listの反復は開始時のbackingを`Rc`で共有して保持し（[次期意味論・実装決定](semantic-decisions.md) §11.2）、要素を複製せず追加課金しない。Dictのkey・Strの文字から反復列を作る場合は新objectなので、作るList bodyと各要素のString body（§5.1）をallocation前に課金し、各要素を§5.3の`string_allocations`・`string_bytes`にも数える（builtinの結果の`String`と同じ）。反復列はloopの終わり（`break`・`return`・errorによる脱出を含む）で最後の参照が消えてreleaseする。tree-walk版とVM版で課金の有無・量・順序とrelease寿命を揃える（issue #358）。

`ExecutionContext`に以前から残る変数・closure・collectionは、新executionを`Linked`へ進めるときにbaseline live heapとして課金する。baselineとLinkのheap課金の合計が上限を超える場合はscriptを1文も実行せず`BudgetExceeded(HeapBytes)`となる。同じobjectを二重に数えない。baselineの求め方は次のどちらかとする。

- 台帳をexecution間で持ち越す経路: 台帳を作り直さず、前のexecutionの使用量（committed・reserved・peaks）を0に戻し、live上限を新しいexecutionの`max_live_heap_bytes`に据え直し、peakをその時点のliveに戻す。前のexecutionのartifact（AST・chunk）の分は先にreleaseする。contextに残る値は同じ台帳に1回だけ課金されたまま残り、そのliveがbaselineになる。値を手放せば、最後の参照が消えた時点で同じ台帳からreleaseされる（§10のjournalが旧valueを保持する間はcommit / rollbackまでliveに残る）。
- 台帳を作り直す経路: 到達graphを反復worklistで走査し、baseline live heapとして新しい台帳へ課金する。再帰走査は使わず、同じ`AllocationId`をvisited setで除外する。残った値が手放されたときに新しい台帳からreleaseされるよう、値の台帳の参照も新しい台帳へ付け替える。

facadeの`Engine::run`とREPLは持ち越しの経路を使う（判断役が選択（PR #496 の Q1=A・PR #499 の Q1=A・Q2=A）。§14「fix-link-context-baseline-charge 実装スライス」の決定記録）。

### 5.3 string、source、import

- `string_allocations`と`string_bytes`は成功した新規String bodyの累積値で、解放しても減らさない。substringが既存bodyを共有する実装なら新規allocationとして数えず、新bodyをcopyすれば数える。
- `max_single_string_bytes`はUTF-8 payload 1個の長さに適用する。headerは含めない。
- builtinが新しいString bodyを作るとき（`join`・`replace`・`path_join`、値の描画など）は、bodyを確保する前に結果の長さが`max_single_string_bytes`を超えないことを確かめる。長さを先に求めるか、上限付きで逐次に書き出して超えた時点で止める。上限を超える結果を作り切ってから検査してはならない（§5.2、§7）。逐次に組み立てるbuiltinは途中の長さで判定し、後の入力で結果が縮む場合（`path_join`の後ろの引数がabsolute componentでpathを置き換える場合など）でも、途中の長さが上限を超えた時点で止める。
- `source_count`はrootを1として、読み込んだimport sourceごとに1増やす。同一normalized module IDのcache hitは増やさない。
- `source_bytes`はrootとimportの生UTF-8 byte長の合計である。BOM・改行を正規化せず、hash対象と同じbyte列を数える。
- `import_count`はrootを含まず、初めて解決したnormalized module IDごとに1増やす。
- `import_bytes`はimport sourceの生byte長である。したがってimport sourceは`source_bytes`と`import_bytes`の両方へ意図的に課金する。
- imported module record、ASTまたはbytecode、module IDはheapにも課金する。
- 既存の`MAX_AST_DEPTH`、`MAX_IMPORT_DEPTH`、`MAX_USER_CALL_DEPTH`は構造的上限として残し、byte/count budgetとは独立に先に検査する。

#### closure が retain する code の課金（A-1、REV-015 Slice 6）

`charge_link` が課金する artifact token（AST / bytecode chunk）は **REPL 入力（artifact）の
寿命**に結びつく。しかし closure を定義した入力が終わっても、その closure が生き残る限り、
closure が握る code（tree=`FnDef.body` の `Rc<[Stmt]>`、VM=`VmFn.chunk` の prototype
`Rc<Chunk>`）はメモリ上に残る。**A-1 モデル**では、両 engine とも「reachable な code は課金し
続ける」: retained closure の code を、artifact の release 後も reachable な限り課金し続ける。**課金の有無と
release 寿命は両 engine で一致する**が、課金額は engine ごとの artifact 単位（tree=`ast_node`、
VM=`bytecode_chunk`）のままで、§5.3 の「AST または bytecode」どおり数値は engine 固有であって
よい（engine 共通の単一基準へ寄せる＝ byte 単位で `live_heap` / `peak_heap` を
一致させることは**しない**）。これにより resource 消費の backend 差（＝「片方だけ落ちる」
非対称）を解消し、[決定性・監査仕様](determinism-and-audit.md) §3 の決定性要件（terminal 時
`BudgetUsage` の backend 一致＝課金の有無・寿命が揃うこと）および manifesto 原則 5 を満たす。

A-1 の設計原則は 1 つに尽きる: **retained-code token は closure instance ではなく、課金対象の
distinct code object（`Rc` の backing 同一性）に紐づける**。関数 header の token は function
instance 固有（instance ごとに別物で、複数 instance 間で共有されない）だが、retained code
は複数の closure instance が同一 `Rc` を共有しうる（下記の生成経路を参照）ため、header と同じ
instance 単位では N 個の instance が同一 code を N 回課金してしまう。したがって retained-code
token は **code object 単位で 1 つ**とし、instance はそれを `Rc::clone`（無課金）で共有する。
この一点で、以下に挙げる instance 間共有・ネスト closure・dead branch のいずれの穴も塞がる。

- **課金単位は distinct code object（§5.2「共有は 1 回課金」の token 層での具体化）**: closure が
  retain する code の §5.1 論理サイズは、その code object（tree=`FnDef.body` の `Rc<[Stmt]>`、
  VM=prototype `Rc<Chunk>`）に対して **Link/adopt 時（tree=`charge_link` の Link、
  VM=`charge_chunk_tree` の adopt＝`ExecutionPhase::Compile`）に 1 本の token で 1 回だけ**
  課金する（課金の timing と token 保持者の移譲は下記「ownership transfer」で定義）。
  同じ `Rc` を共有する
  2 個目以降の closure instance は、既存トークンへの `Rc::clone` だけを持ち、追加課金しない。
  - **実現機構**: retained-code token を
    **code object（distinct `Rc<[Stmt]>` / `Rc<Chunk>`）ごとに 1 本の `Rc<HeapToken>`
    （`HeapToken = Tracked<()>`）として立て、closure instance はそれを `Rc::clone` で共有する**
    方式を採る。この `Rc<HeapToken>` は Link/adopt 時に課金して立て（下記「ownership transfer」）、
    生成前は artifact 側が code object との対応で保持し、生成時に closure instance
    （tree=`Value::Fn`、VM=`Value::VmFn`）へ `Rc::clone` 共有する。同じ code object を共有する
    2 個目以降の instance も同じ `Rc<HeapToken>` を `Rc::clone` するだけで追加課金しない。
    - **token と code object の対応付け（正準方式・tree / VM 対称の side-table）**:
      生成時に「いま instantiate しようとしている body / prototype」から「その事前課金済み
      token」を引くために、**両 engine とも `Rc::as_ptr` を key にした side-table で引く**。
      - **VM**: `Vm` が `HashMap<*const Chunk, Weak<HeapToken>>` を 1 本持つ。key は
        `Rc::as_ptr(&prototype.chunk)`（`FunctionPrototype.chunk` は `Rc<Chunk>`）。adopt 時の `charge_chunk_tree_into` 走査で各 distinct prototype
        chunk を課金し、立てた strong `Rc<HeapToken>` は `chunk_tokens` へ（artifact 保持）、
        同じ token の `Weak` を side-table へ登録する。`MakeClosure` は
        手元の `prototype.chunk`（および下記の transitive な子孫 prototype chunk）を
        `Rc::as_ptr` で side-table から引き、`Weak::upgrade` して得た strong を `Rc::clone` で
        `VmFn` の retain-token 集合へ載せる。`FunctionPrototype` / `Chunk` へのフィールド追加は
        不要（`Chunk` を可変にせず、`PartialEq`/`Clone` 派生も変えない）。
      - **tree**: `Evaluator` が `HashMap<*const [Stmt], Weak<HeapToken>>` を 1 本持つ。key は
        `Rc::as_ptr(&body)`（`FnDef.body` / `Lambda.body` は `Block = Rc<[Stmt]>`）。Link 時の per-body 課金で立てた strong `Rc<HeapToken>` は
        `ast_token` 相当の artifact 保持側へ、`Weak` を side-table へ登録する。closure 生成点
        （`Stmt::FnDef` / `Expr::Lambda` の評価）は、生成する closure の body
        （および下記 transitive な内側 body）を `Rc::as_ptr` で引き、`Weak::upgrade` して得た
        strong を `Rc::clone` で `Value::Fn` の retain-token 集合へ載せる。
    - **side-table は `Weak` の index であり token 寿命を一切縛らない（跨入力の鍵）**:
      side-table が持つのは `Weak<HeapToken>` だけなので、side-table 自身は token を生かさない。
      token の寿命は常に strong 参照（artifact 保持側の `chunk_tokens`/`ast_token` 相当、または
      生成済み closure instance が握る `Rc::clone`）の参照カウントだけが決める。これにより
      side-table は **`Vm` / `Evaluator` の寿命を通じて持ち続けてよい**（入力ごとに破棄しない）。
      REPL 入力をまたいで生成される closure（下記不変量）でも `Weak::upgrade` が成立する限り
      token を引けるためである。upgrade が失敗する `Weak`（token が既に release 済み）は
      dead entry であり、次回同 key 挿入時に上書きするか、入力入れ替え時に掃除してよい
      （掃除は性能最適化であって budget 不変量には影響しない）。
    - **跨入力生成の不変量（両 engine 共通）**: closure 生成（tree=`FnDef`/`Lambda`
      評価、VM=`MakeClosure`）は、その code object を adopt/Link した入力と**必ずしも同一
      入力ではない**。top-level closure は定義入力内で生成されるが、ネストした内側 closure は
      外側 closure が後続入力で call されて初めて生成されうる（例: 入力 N で `fn outer() { fn
      inner()... }` を定義し、入力 N+1 で `outer()` を呼ぶと `inner` の生成は入力 N+1）。
      この跨ぎを安全にするため、本設計は **(1) side-table を `Weak` の長命 index にする**（直前
      項）ことと、**(2) closure 生成時にその closure が reachable に握る code object subtree
      全体（自分の code object + transitive な子孫 code object）の token を一括で `Rc::clone`
      して instance へ移譲する**こと（下記「生成時はトークンを移譲する」で定義）で担保する。
      (2) により、外側 closure が生き残れば内側 prototype/body の token も外側 instance の
      strong 参照で生き続け、後続入力の内側生成時に `Weak::upgrade` が成立する。raw-pointer
      key の跨入力 ABA（drop 済みアドレスの再利用）は、upgrade 可否が token の生死で判定される
      ため実害を生まない（dead な `Weak` は upgrade 失敗で弾かれ、生きている token だけが引ける）。
    - **token 保持構造自体は無課金**: 両 engine の side-table（`HashMap<_, Weak<HeapToken>>`）・
      artifact 保持側の token 集合・closure instance 側の retain-token 集合は、いずれも
      **budget の内部簿記であってそれ自体は課金しない**（artifact 保持側の
      `chunk_tokens: Vec<Rc<HeapToken>>` / `ast_token` を課金対象にしないのと同じ扱い）。
      これらは課金対象の code object 本体ではなく token への参照を保持するだけの bookkeeping で
      あるため、`live_heap` の「課金の有無と寿命が backend 間で一致」不変量には影響しない。
  - **tree**: `FnDef.body`（`ast_heap_size` と同じ `ast_node` 単位＝64 + 所有 identifier /
    string literal の byte 長）を本体 AST の論理サイズとして、その body `Rc<[Stmt]>` に紐づく
    token で 1 回だけ課金する。
  - **VM**: closure が握る prototype chunk を `charge_chunk_tree` と同じ `bytecode_chunk`
    （64 + 16×opcode + 32×constant）で課金する。**その prototype が transitive に抱える
    子・孫 prototype も含めて**、各 distinct chunk に紐づく token で 1 回ずつ課金する
    （§5.2 の共有は 1 回課金）。
- **ネスト closure の transitive 規則（tree / VM 対称）**: `FnDef.body` / `Lambda.body` には
  さらにネストした `FnDef` / `Lambda` を書け、`ast_heap_size` は body へ再帰展開する。したがって
  tree 側も VM の prototype 走査と対称に、**各 distinct body AST slice はちょうど 1 つの token が
  課金する**ものとして扱う。
  - **tree**: 外側 closure の token が `FnDef.body` を課金する際、body 内にネストした
    `FnDef` / `Lambda` の body（＝内側の distinct `Rc<[Stmt]>`）は、**その内側 closure の token
    が課金する責務**とし、外側 token の走査対象から除外する。どの body がどの token に属するかは
    code object（`Rc<[Stmt]>` の backing 同一性）で判定する。これにより外側と内側が同じ内側
    body を二重課金しない。tree の各 `FnDef` / `Lambda` ノードの body は原則 distinct な `Rc`
    だが、仮にパーサ/linker が同一 `Rc<[Stmt]>` を複数ノードへ共有させても、backing 同一性
    （`Rc::as_ptr`）で 1 token に畳まれるため §5.2「共有は 1 回課金」と整合する。
  - **VM**: 同様に、prototype が抱える子孫 prototype は、それ自体が別の `MakeClosure` の対象なら
    その内側 closure の token が課金する責務とし、外側の走査対象から除外する。
- **二重課金回避境界（最重要）と ownership transfer の発火条件**: artifact token（入力単位）と
  retained-code token（code object 寿命）が **同じ code object を同時に課金してはならない**。
  ただし責務移譲の発火点は「linked program の走査（closure 生成**前**）」ではなく
  「**closure が実際に生成された時点**」である。この時間差を取り違えると、評価されない
  `FnDef` / `Lambda`（dead branch の定義、束縛も返却もされず捨てられる lambda）で body が
  どちらの token にも載らない uncharged 状態になり、§5.2「reachable な code は課金」と矛盾する。
  token は 1 個の `Tracked<()>` が生成時 `bytes` を課金し `Drop` でちょうどその `bytes`
  を release する（partial release / 途中での再配賦 API は持たない）。この性質を前提に、
  lump token からの差し引きではなく **Link 時の per-code-object 分割 + 生成時のトークン移譲**で
  境界を実現する。
  - **Link/adopt 時に artifact 課金を「非 closure 部」と「code object ごと」に分割して積む**:
    Link/adopt（tree=`charge_link` の Link、VM=`charge_chunk_tree` の adopt）で artifact を課金
    する際、(a) closure body / prototype に該当しない top-level code は artifact token
    で課金し、(b) 各 closure body / prototype（distinct `Rc<[Stmt]>` / `Rc<Chunk>`）は **それぞれ
    独立した `Rc<HeapToken>` を 1 本ずつ立てて課金する**。この per-code-object token の strong
    参照は生成前は **artifact 保持側（tree=`ast_token` 相当、VM=`chunk_tokens`）が保持し**、
    その `Weak` を side-table（上記「token と code object の対応付け」）へ index として登録して、
    closure が生成されるまでは artifact の寿命で生かす。
    - **全ネスト深度を eager に列挙すること（tree / VM 対称・最重要）**: この分割走査は
      **linked program 全体をネスト深度に関わらず走り、どんな深さにある distinct body / prototype
      に対しても 1 本ずつ token を立てなければならない**。top-level の `FnDef` / `Lambda` だけを
      列挙して body 内にネストした closure を取りこぼすと、その内側 body が uncharged になり本
      設計が塞ごうとしている穴が再発する。
      - **VM**: adopt 時の走査（`charge_chunk_tree_into`）は `chunk.prototypes` を **transitive に
        再帰**し、到達されない深いネスト prototype も含め全 distinct chunk を 1 回ずつ課金する。各 prototype を課金した
        strong token を `chunk_tokens` へ積み、その `Weak` を side-table
        （key=`Rc::as_ptr(&prototype.chunk)`）へ登録するだけで、列挙漏れは起きない。prototype は
        compile 時に distinct に作られるため backing 同一性での重複判定が素直に成立する。
      - **tree**: 走査は **body（`Rc<[Stmt]>`）を単位に小計を区切る**: `FnDef` / `Lambda` に
        当たったら body を worklist へ平坦展開せず、その body を 1 つの課金単位として小計を
        計算し、body 内にさらにネストした `FnDef` / `Lambda` の body は再帰的に別単位へ切り出す。
        外側 token が内側 body を二重課金しないよう、外側小計からは内側 body slice ぶんを除外
        する（上記「ネスト closure の transitive 規則」の tree 項と同じ境界）。任意のネスト深度の
        distinct body を列挙し、各 slice に 1 本ずつ token を立てて side-table
        （key=`Rc::as_ptr(&body)`）へ `Weak` を登録する。top-level だけを列挙する実装は禁止。
    これにより dead branch の body やネストした未到達 lambda も Link/adopt 時点で必ずどこかの
    token に載り、uncharged 状態を作らない（§5.2「reachable な code は課金」を満たす）。
  - **生成時はトークンを「移譲」する（再課金も差し引きもしない・subtree 一括）**: closure が
    実際に生成された瞬間（tree=`FnDef` / `Lambda` 評価、VM=`MakeClosure`）に、その closure が
    reachable に握る code object subtree 全体 — **自分の code object（tree=`FnDef.body`、
    VM=`VmFn.chunk`）に加えて、そこから transitive に辿れる子孫 code object（内側 body /
    子孫 prototype chunk）すべて** — の per-code-object token を、side-table から `Rc::as_ptr`
    で引き `Weak::upgrade` して得た strong を `Rc::clone` し、closure instance（tree=`Value::Fn`、
    VM=`Value::VmFn`）の retain-token 集合へ載せる。追加課金は発生しない（既に Link/adopt 時に
    1 回課金済み）。同じ code object を共有する 2 個目以降の instance も同じ `Rc<HeapToken>` を
    `Rc::clone` するだけ。subtree 一括で移譲するのは、内側 closure が後続入力で初めて生成される
    場合でも（上記「跨入力生成の不変量」）、外側 instance が内側 token の strong を握り続けて
    side-table の `Weak::upgrade` を成立させ続けるためである。こうして「生成時に責務が
    artifact 保持から closure instance 保持へ移る」。lump token からの byte 差し引きは行わない
    ため、`Tracked` の partial release 不在と矛盾しない。
  - **移譲は `Rc::clone`（共有）であり token の move-out ではない（課金 phase と生成 phase の
    時間差を跨ぐ鍵）**: per-code-object token は Link/adopt 時（VM は `ExecutionPhase::Compile`）に立って artifact 保持側（VM=`chunk_tokens`、tree=`ast_token` 相当）へ
    strong が入り、side-table には `Weak` だけが入る。生成（VM=`MakeClosure`、tree=`FnDef` /
    `Lambda` 評価）はずっと後に、場合によっては**複数入力をまたいで**走る。この時間差を安全に
    跨ぐため、移譲は **保持側から token を取り出して move するのではなく、side-table 経由で
    `Weak::upgrade` した strong を `Rc::clone` して共有を増やす**操作と定義する。生成時点で
    closure instance が独立した強参照（`Rc` clone）を握る。
  - **artifact release との接続（両 engine とも自分の clone を無条件 release、生存は instance の
    独立 `Rc` が担保）**: REPL が次入力で artifact を入れ替えるとき、VM は
    **`chunk_tokens.clear()` で自分が持つ strong を無条件に（移譲済みの prototype 分も含めて）
    drop する**。tree も対称に、`ast_token = None` と artifact 保持側の
    per-body token 集合を無条件に drop する。「移譲されたものだけ残す」フィルタリングは両 engine
    とも不要かつ行わない。移譲済み code object の課金が生き残るのは、生きている closure instance
    が握る**独立した `Rc` clone** が last-ref ではないため `Tracked::Drop`
    （last-ref でのみ release）が発火しないから、という一点のみで担保
    される。side-table はこの release に関与しない（`Weak` のみ保持し token 寿命を縛らない。
    上記「side-table は `Weak` の index」）。生成済み closure 側に `Rc::clone` の強参照が残る
    ため、その token は last-ref になるまで release されず、closure が生きている限り課金が残る。
    これが A-1 の核心である。
    - **帰結（取り違え防止）**: 移譲を「保持側コレクションからの move-out」とモデル化しては
      ならない。move-out にすると、入力 N で生成した closure がまだ生きているのに入力 N+1 の
      無条件 clear が「残った未移譲分だけ」を落とそうとして実装が複雑化し、かつ clone 共有の
      意味論から外れる。正準モデルは「side-table 経由の `Weak::upgrade` → `Rc::clone` 共有 +
      保持側の無条件 clear + last-ref release」であり、未移譲 only のフィルタは不要。dead branch
      等「一度も移譲されなかった」token も、この無条件 clear で artifact 保持側の strong が落ち、
      他に強参照がないので last-ref として正しく release される（side-table の `Weak` は
      dangling になるだけで token を生かさない）。
  - 要約すると、各 distinct code object は常に **ちょうど 1 つの `Rc<HeapToken>`** に課金され、
    その token の保持者が「artifact 側（生成前・未生成）」から「closure instance 側（生成後）」へ
    移るだけで、課金額は Link 時の 1 回から変化しない。§5.2 の `AllocationId` visited-set と同じ
    「同じ object は 1 回だけ課金」の不変量を token 層でも守る。
- **release 寿命（retained-code token を retain する最後の `Rc` が drop された時点で落とす）**:
  retained-code token は、その code object を retain する **最後の `Rc<HeapToken>` が drop された
  時点**で `Tracked` の `Drop`（last strong ref の drop 時に発火）が
  live heap を release する。生成後は token の保持者が closure instance（および
  同じ code object を共有する他 instance）だけになるため、これは「その code object を retain
  する最後の closure instance が drop した瞬間」と一致する。closure instance が複数共有されて
  いても 1 個の drop では release せず、最後の 1 個まで retained code を live heap に残す
  （§5.2「到達可能性が残る object は release しない」）。生成されずに artifact 側へ残った token は
  artifact release 時（REPL 入れ替え）に drop されて release される。これは関数 header の
  per-drop 追跡（`Value::Fn` / `Value::VmFn` の `header: Rc<Tracked<()>>` を生成時課金・
  drop で release）と**同じ per-drop パターン**だが、header が instance 固有トークン
  なのに対し retained-code は **code object 単位の共有トークン**である点が異なる。header token
  （instance の固定 overhead）と retained-code token（本体 AST / prototype chunk の論理サイズ）
  は別トークンとして並置する。
- **§5.3「AST または bytecode」原則との整合**: tree は AST を、VM は bytecode chunk を課金する
  という engine ごとの別 artifact の建付けは A-1 でも維持する（engine 共通の
  単一基準へ寄せない）。したがって retained closure の課金額も engine ごとに（tree=`ast_node` /
  VM=`bytecode_chunk`）異なる数値になってよく、A-1 が backend 間で揃えるのは **課金の有無と
  release 寿命**であって `live_heap` / `peak_heap` の byte 単位の数値一致ではない。
  [決定性・監査仕様](determinism-and-audit.md) §15.5 differential は retained closure を含めて
  charge trace を突合するが、その突合対象は
  「reachable な限り課金され続けること・release 寿命が一致すること」および code artifact に
  既に認められている engine 固有サイズ差であり、code artifact の byte 単位数値一致ではない。

## 6. Input、output、host callの課金

### 6.1 共通規則

byte数はhost境界で実際に受け渡すUTF-8またはbinary payloadの長さであり、Rust objectのcapacityやtransport headerは含めない。serialize形式は[Capability仕様](capability-model.md)で操作ごとに固定する。

| 操作 | count | bytes |
|---|---|---|
| `input` request | dispatch開始時に`input_calls += 1` | 受け取ったpayloadを`input_bytes`へcommit |
| output request | dispatch開始時に`output_calls += 1` | hostへ渡すpayloadを`output_bytes`へcommit |
| 任意host call | capability判定前に`host_calls += 1` | requestを`host_request_bytes`、responseを`host_response_bytes`、双方を`host_call_bytes`へcommit |

stdioもhost callであるため、input/output固有counterに加えてhost call counterとrequest/response byteへ課金する。capability拒否でもcountとrequest bytesは課金し、response bytesは0とする。

request payloadを作るためにscriptの値を文字列へ描画する場合（`write_file` / `append_file`の第2引数が`Str`でないときなど）、その描画は`print`・`to_str`と同じ予算付きの描画（複合ノードごとのfuelと`max_single_string_bytes`の確認、§4.1・§5.3）で行い、request bytesはその描画結果の長さとする。描画はrequest bytesの課金の前、つまりcapability判定の前に行うので、描画がfuelまたはper-item byte上限を超えた場合は、countとrequest bytesを課金せず（requestを外部へ渡しておらず、§7の規則4の「operation未開始」にあたる）、capability判定・adapter呼び出し・外部効果のどれも起こさずに`ControlStop`で止まる（§7の「外部効果またはallocationの前」）。描画は1回だけ行い、同じ文字列をrequest bytesの算出・adapterへ渡すpayload・監査のrequest bodyで共有する。予算を見ない描画（`Value`の`Display`）でrequest payloadを作ってはならない。

単一callのrequest/response最大値は、host function descriptorの上限とexecution残量の最小値で決まる。providerへ`max_response_bytes`を渡し、providerは超過payloadを作成・読み込み切る前に内部signal `HostAdapterLimit::ResponseBytes`で停止する。dispatcherはこれをpublic `HostError`へ変換せず、`BudgetExceeded { resource: HostResponseBytes, ... }`へ変換する。descriptor上限とexecution累積上限を同時に超える場合は小さい上限を`limit`とし、同値ならexecution累積上限をprimaryとする。先に全bodyを無制限に読み込んでから拒否してはならない。

### 6.2 blocking host call契約

[Capability仕様](capability-model.md)のPhase 2同期adapterは、descriptorが`may_block = false`であり、その場で有限時間に完了する場合だけ`Ready`へwrapする。Phase 4では、同期traitの意味を変えず、blocking可能なadapter向けに次のcooperative extension traitを追加する。Phase 2の「v1にasync callbackを入れない」という制約はPhase 4まで維持され、`Pending`対応後も既存同期callbackを暗黙に別threadへ移さない。

`poll`を呼ぶthreadで、時間上限が証明できないI/O、lock待ち、DNS、network、process待ちを行ってはならない。cooperative host adapterは次のいずれかを返す。

```rust
pub enum HostCallPoll<T> {
    Ready(Result<T, AdapterError>),
    Pending(HostCallTicket<T>),
}

pub trait CooperativeAdapter<Request, Response>: Send + Sync + 'static {
    fn start(
        &self,
        context: &mut CapabilityCallContext<'_>,
        request: Request,
    ) -> HostCallPoll<Response>;
}

pub trait Wake: Send + Sync { fn wake(&self); }
#[derive(Clone)]
pub struct ExecutionWaker(Arc<dyn Wake>);
impl ExecutionWaker {
    pub fn new(wake: Arc<dyn Wake>) -> Self;
    pub fn wake(&self);
}

pub struct HostCallTicket<T> { /* private, Send + Sync */ }

impl<T> HostCallTicket<T> {
    pub fn id(&self) -> u64;
    pub fn register_waker(&self, waker: &ExecutionWaker);
    pub fn try_take(&self) -> Option<Result<T, AdapterError>>;
    pub fn cancel(&self);
}
```

`HostCallTicket<T>`が`T`を問わず`Send + Sync`であることを目標として変えない（判断役が選択（issue #420）、D-114 Q1=A）。理由: 本節は時間上限が証明できないI/Oを`poll`のthreadで行わず、adapter executorがresultをticketへ格納してwakeし、作成thread上の次回`poll`が`try_take`すると定める。結果を別threadからticketへ返せることがこの形の前提で、`Send + Sync`はその受け皿である。`T: Send`のときだけに緩めると、blockingな処理をpollのthreadの外へ出してhostへ制御を返す目標（[マニフェスト](manifesto.md)原則4・原則1）が本文から消える（本書冒頭の#288 Q1=A・Q2=A、[Capability Model仕様](capability-model.md)冒頭のissue #263 Q2=Aと同じく、本文を実装に合わせて後退させない）。この決定は、目標をどう実装するか（`Value`をthread間で渡す形、completerの型）を決めない。

各Phase 2同期traitのrequest/response型に対して、Phase 4でだけ対応する`CooperativeAdapter<Request, Response>`実装を登録できる。同期実装は常に`Ready`相当で完了し`Pending`を返すAPIを持たない。cooperative実装は同じcapability判定、budget reservation、deadline、cancellation、audit correlationを使用し、Phase 2 traitを別threadへ暗黙offloadして擬似的に`Pending`へ変換してはならない。

`Pending`ではexecutionを`Yielded(HostCallPending { call_id })`にし、ticketへexecutionの`ExecutionWaker`を1個登録する。adapter executorはresultをticketへ一度だけ格納してwakeし、作成thread上の次回`poll`が`try_take`する。wakeはcontinuationを別threadで実行しない。ticket dropまたはcancelはadapterへ取消要求を送り、遅着resultは破棄する。executorはEngine本体と別の有限thread/concurrency/queue上限を持ち、deadline、`CancellationToken`、request/response残量を受け取る。Engineのscheduler lock、context lock、audit sink lockを保持したままadapterを呼ばない。

providerはcancelを協調的に処理する。基盤APIがcancel不能なら、adapterはexecution deadline以下の有限timeoutを必ず設定し、cancel後の結果と副作用をscriptへ返さない。cancel時は監査上のhost callを`Cancelled`または`Detached`で閉じてからexecutionをterminalにし、遅着callbackは破棄する。

## 7. reserve / commit / refund

複数resourceを使う操作は、外部効果またはallocationの前に1個のatomic reservationとして処理する。

```rust
pub enum ControlStop {
    Cancelled,
    DeadlineExceeded { deadline: MonotonicInstant, observed: MonotonicInstant },
    BudgetExceeded(BudgetExceeded),
}

let reservation = budget.reserve(BudgetRequest { ... })?; // Result<_, ControlStop>
let actual = perform_bounded_operation(reservation.limits())?;
reservation.commit(actual)?; // actual <= reserved
// operation未開始なら reservation.refund()
```

1. `reserve`はcancelとdeadlineを先に確認し、該当時は独立した`ControlStop`を返す。その後`used + reserved + requested`を全resourceでchecked additionする。
2. 1つでも超える場合は何もreserveせず、固定優先順位の`BudgetExceeded`を返す。
3. `commit(actual)`は`actual <= reserved`だけを許し、reservedからactualを引いてcommittedへ足し、差分をrefundする。
4. operation未開始、allocation失敗、capability判定前の内部失敗では全額refundする。
5. 外部へrequestを渡した後、count、request bytes、fuelはrefundしない。responseが不明なままadapterをdetachした場合はresponse予約を全額commitする。
6. heap objectのdropによるlive byte減少は`release`であり、累積string/source/I/O counterのrefundではない。
7. reservation objectを未settleでdropした場合、debug buildだけでpanicするのではなく、productionでも`InternalFailure(UnsettledReservation)`へ遷移する。外部効果開始前なら自動refund、開始後なら全額commitする。

### 7.1 overflow

全加算・乗算・byte長変換は`checked_*`を使う。演算overflowは「実質無制限」とせず、そのresourceの`BudgetExceeded`として扱い、`requested = u64::MAX`を記録する。`usize`から`u64`へ変換できないplatformでは同様に超過とする。usage、sequence、AllocationIdをsaturating/wrapping更新してはならない。

### 7.2 複数超過の優先順位

同一atomic reservationで複数上限を超える場合は、次の先頭1件をprimary `BudgetExceeded`とする。deadlineはbudget resourceではなく独立した`ExecutionOutcome::DeadlineExceeded`であり、この一覧へ含めない。監査には同時に超えたresource一覧を補助fieldとして記録できるが、outcomeは1件だけである。

1. `Fuel`
2. `HeapBytes`
3. `SingleStringBytes`
4. `StringAllocations`
5. `StringBytes`
6. `SingleSourceBytes`
7. `SourceCount`
8. `SourceBytes`
9. `ImportCount`
10. `ImportBytes`
11. `CollectionElements`
12. `InputCalls`
13. `InputBytes`
14. `OutputCalls`
15. `OutputBytes`
16. `HostCalls`
17. `HostRequestBytes`
18. `HostResponseBytes`
19. `HostCallBytes`

同じcheckpointでcancel、deadline、budget超過が同時に観測された場合は`ExecutionOutcome::Cancelled`、`ExecutionOutcome::DeadlineExceeded`、`ExecutionOutcome::BudgetExceeded`の順にする。既にterminal遷移が完了していれば、そのterminal結果を変更しない。

## 8. CancellationToken

```rust
#[derive(Clone)]
pub struct CancellationToken { /* Arc<AtomicBool> + wake registration */ }

impl CancellationToken {
    pub fn cancel(&self) -> bool;      // falseなら既にcancel済み
    pub fn is_cancelled(&self) -> bool;
}
```

cancelはidempotentかつthread-safeで、最初の`false -> true`をlinearization pointとする。tokenはReady/Yielded/Paused/host call待ちのhandleをwakeする。

確認点は、各fuel charge前、bulk chunk間、host call開始前後、yield/resume、queue admission、commit直前である。Running中にcancelされた場合は次の確認点まで現在のbounded atomic operationを終えられる。host callの外部効果が既に始まっている場合、その効果のrollbackは保証しない。

cancelと正常完了が競合した場合、terminal stateのcompare-and-setに先に成功した側を採用する。正常結果がterminalへcommitした後のcancelは結果を変えない。cancelが先に観測され`Cancelled`へ遷移した後の値・host responseは破棄する。

## 9. ExecutionHandleと状態機械

### 9.1 公開型

```rust
pub struct ExecutionHandle<'engine, 'script, 'context> {
    /* Engine/Script共有借用、ExecutionContext排他借用、!Send + !Sync */
}

pub enum ExecutionState {
    Created,
    Linked,
    Ready,
    Running,
    Yielded(YieldReason),
    Paused(PausedState),
    Terminal,
}

pub struct PausedState {
    pub reason: PauseReason,
    pub resume_to: ResumeState,
}

pub enum ResumeState {
    Created,
    Linked,
    Ready,
    Yielded(YieldReason),
}

pub enum AdmissionPhase { Created, Linked }

pub enum YieldReason {
    AdmissionQueued { resume_to: AdmissionPhase },
    SliceFuelExhausted,
    ExplicitYield,
    HostCallPending { call_id: u64 },
    AuditBackpressure,
    SchedulerPreempted,
}

pub enum PauseReason {
    HostRequested,
}

pub struct PollSlice {
    pub max_fuel: NonZeroU64, // 16以上。既定10,000
}

pub enum PollResult {
    Yielded { reason: YieldReason, usage: BudgetUsage },
    Paused { state: PausedState, usage: BudgetUsage },
    Terminal { outcome: ExecutionOutcome, usage: BudgetUsage },
}

pub enum HandleError {
    WrongThread,
    InvalidState { operation: &'static str, state: ExecutionState },
    Terminal,
}

impl Engine {
    pub fn create_execution<'e, 's, 'c>(
        &'e self,
        script: &'s CompiledScript,
        context: &'c mut ExecutionContext,
        request: ExecutionRequest,
        link_options: LinkOptions,
    ) -> Result<ExecutionHandle<'e, 's, 'c>, StartError>;

    pub fn start<'e, 's, 'c>(
        &'e self,
        script: &'s LinkedScript,
        context: &'c mut ExecutionContext,
        request: ExecutionRequest,
    ) -> Result<ExecutionHandle<'e, 's, 'c>, StartError>;
}

impl ExecutionHandle<'_, '_, '_> {
    pub fn state(&self) -> ExecutionState;
    pub fn usage(&self) -> BudgetUsage;
    pub fn poll(&mut self, slice: PollSlice) -> Result<PollResult, HandleError>;
    pub fn pause(&mut self) -> Result<(), HandleError>;
    pub fn resume(&mut self) -> Result<(), HandleError>;
    pub fn set_waker(&mut self, waker: Option<ExecutionWaker>) -> Result<(), HandleError>;
    pub fn cancellation_token(&self) -> CancellationToken;
    pub fn outcome(&self) -> Option<&ExecutionOutcome>;
}
```

`create_execution`と`start`はhandle作成前に、active slotが空いていればactiveを、なければadmission queue slotを1個だけatomicに予約する。両方が満杯なら`StartError::Backpressure`を返し、handleを作らずcontextを変更しない。`create_execution`はphase Created、`start`はphase Linkedから始まる。queue slotを得たhandleの公開stateは`Yielded(AdmissionQueued { resume_to: Created | Linked })`で、linkを含むworkを一切行わない。FIFO先頭でactiveが空くとqueue slotをactive slotへatomic変換し、`resume_to`のCreated/Linkedへ戻してwakerで通知する。link/terminal/drop時は所有するslotを必ず1個だけ解放する。

public `ExecutionHandle`は`!Send + !Sync`で、作成したthread上からだけ操作する。`poll`だけがReady/YieldedをRunningへ進め、1 slice以内に戻る。`pause`はCreated/Linked/Ready/Yieldedでだけ成功し、`resume`はPausedでだけ成功する。Running中はmutable borrow中なので別操作できず、terminalでは`HandleError::Terminal`を返す。

`set_waker(Some(w))`は作成threadから呼び、以前のwakerを置換する。admission取得、host/audit ticket完了、cancel、deadline timerは状態をreadyにした後でwakerを呼ぶ。wakeはedge-triggered hintで複数通知を1回へcoalesceでき、waker callbackはhandleへ再入せずhost event loopへ通知するだけとする。lost wakeを避けるため、登録は「waker保存→ready flag再確認→readyなら即wake」の順に行う。`set_waker(None)`で解除でき、waker自体は`Send + Sync`で別threadから呼べる。hostはwake後に作成threadで`poll`する。busy-pollは不要である。

別threadから許可する操作は`CancellationToken::cancel()`とwaker invocationだけであり、continuationやcontextへのmutable referenceをhostへ公開しない。

Engineはworker threadを生成しない。`Engine`内の`AdmissionController`とrun-turn queueだけを`Send + Sync`とし、continuationは各handleの作成threadに留める。Ready execution IDをengine-wide FIFOへ置き、queue先頭のhandleだけがsemantic workを1 slice進められる。先頭でないhandleの`poll`はworkを行わず`Yielded(SchedulerPreempted)`を返す。hostはwakeされたhandleをpollする責任を持ち、先頭handleをpollしなければ全体の進行は停止するが、上限超過や順序飛越は起きない。

非terminal handleのdropはcancelをlinearizeし、pending host callを`Detached`で閉じ、language-stateをrollbackし、logical `Terminal(Cancelled)`をappendする。sinkがPendingならjournalをEngineのbounded orphan-audit queueへmoveしてからcontext borrowを解放する。orphan queue満杯時はdrop前に予約済みTerminal slotを使い、以降の新規executionを`AuditUnavailable`でfail-closedにする。Dropはblocking I/Oやpanicを行わない。

### 9.2 状態遷移

```text
create_execution --active slot予約---------------------------> Created
start --active slot予約--------------------------------------> Linked
create_execution/start --queue slot予約----------------------> Yielded(AdmissionQueued { resume_to })
Yielded(AdmissionQueued { resume_to }) --slot変換-------------> resume_to
Created --pollでlink成功--> Linked --pollでrun-turn登録-------> Ready --poll--> Running
Created/Linked --失敗----------------------------------------> Terminal
Running --slice/host/audit待ち--> Yielded --再queue----------> Ready
Created/Linked/Ready/Yielded --pause-------------------------> Paused
Paused --resume----------------------------------------------> resume_to
Created/Linked/Ready/Running/Yielded/Paused --cancel等-------> Terminal
```

- `Created`: rootの`CompiledScript`、context、config、tokenを所有し、active slotを取得済み。script文は未実行。
- `Linked`: import graphを実行前解決し、source/import/compiled heapとcontext baselineを課金済みで、active slotを取得済み。
- `Ready`: link済みでrun-turn queueにいる。
- `Running`: 作成threadで`poll`しているcallerだけがcontinuationを進めている。
- `Yielded`: continuationを保持し、理由の解消後にrun-turn queue末尾へ戻る。`AdmissionQueued`だけはactiveではなくqueue slotを所有し、link workをまだ始めない。
- `Paused`: hostが明示resumeするまでrun-turn queueへ戻らない。`PausedState.resume_to`に直前のCreated/Linked/Ready/Yieldedを保持し、その状態へだけ戻る。pause前にactive slotを取得済みならactiveを、未取得ならqueue slotを消費し続け、deadlineも進む。
- `Terminal`: active/queue slot、pending reservation、continuationを解放する。context commit/rollback後の状態と`ExecutionOutcome`だけをcallerへ公開する。

`poll`はrun-turn queue先頭のReadyまたは再開可能なYieldedだけをRunningへ遷移させる。同期`Engine::run`はEngineに他のnonterminal handleがない場合だけ同じhandleをterminalまでpollし、存在する場合はscriptを開始せず`StartError::ConcurrentRunRequiresPolling`を返す。複数executionは各作成threadが公開`poll`をdriveする。

### 9.3 continuationへ保存するもの

再帰するRust call stackをcontinuationとして使ってはならない。少なくとも次をheap上の明示状態として保存する。

- AST cursorまたはprogram counter、value stack、保留中operand
- call frame、lexical scope、local/global slot、captured cell/upvalue
- loop frame、`return` / `break` / `continue`の保留control flow
- `try` handler stack、unwind位置、catch対象
- linked module table、import実行位置、loaded/loading marker
- `ExecutionContext`へのtransaction journalとbaseline generation
- `BudgetUsage`、reservation、AllocationLedger、slice残量
- cancellation/pause要求、poll index
- pending host callのcall ID、ticket、予約量、結果受渡し位置
- audit sequence、未flush event、redaction context

Paused/Yielded中にこれらを外部serializeしない。process終了を跨ぐ永続化、別Engineへの移送、version間resumeは非対応である。

## 10. Contextのcommit / rollback（AUD-024）

AUD-024の方針を次で確定する。

- `Completed`とscript要求の`Exited`だけが、execution中のscript-visible stateを`ExecutionContext`へcommitする。最終`AuditEvent::Terminal`は`context_committed = true`とする。
- catch済みruntime error（script操作中のcanonical `capability` / `host` errorを含む）は通常の制御フローであり、その後executionがCompleted/Exitedならcatchより前後の変更をcommit対象として維持する。
- `Denied`、未捕捉`RuntimeError`、`HostError`、`LinkError`、budget/deadline、cancel、audit/record/replay失敗、internal failureは、execution開始時点まで**全language-stateをrollback**し、最終`AuditEvent::Terminal`を`context_committed = false`とする。`Denied` / `HostError` terminalはscript handlerがまだ存在しないlink/control-plane失敗に使い、script実行中のcatch可能errorと混同しない。
- rollback対象はbindingの追加・削除・代入、List/Dict mutation、captured cell/upvalue、function定義、loaded module markerである。
- root parse/link失敗はscriptを実行していないためcontextを変更しない。失敗moduleはloaded扱いにしない。
- stdout、filesystem、network、database、process、host function等、既に境界外でcommitした効果はrollbackしない。監査のterminal eventへ`host_effects_may_remain = true`を記録する。
- yieldとpauseはtransaction境界ではなく、journalを保持する。resume後も同じexecution transactionを続ける。

実装はmutation journalまたはcopy-on-write generationを使い、context全体のdeep cloneを禁止する。journal自身と旧valueの保持はheap budgetへ課金する。rollback処理用に追加fuelを要求せず、terminal処理のhost control-plane workとして扱うが、work量はjournal entry数で有限でなければならない。

## 11. pause中の不変性

Paused中に許可する操作は`state`、`usage`のread、`resume`、`cancel`、handle dropだけである。次は変更不可とする。

- `BudgetConfig`とdeadline
- capability grant/policy
- host provider、clock、input、output、module resolver
- root source、linked import、language revision、backend
- `ExecutionContext`内の値
- audit sink、redaction、record/replay mode

budget追加、deadline延長、capability追加を行うAPIは提供しない。別設定で続けるにはterminal後に新executionを作る必要があり、terminal continuationは再利用できない。

## 12. Engine-wide負荷制御

```rust
pub struct EngineLimits {
    pub max_active_executions: NonZeroUsize, // 既定: available_parallelism、上限64
    pub max_queued_executions: usize,        // 既定: 256
    pub default_slice_fuel: NonZeroU64,      // 既定: 10,000
}
```

`available_parallelism`取得失敗時は1とし、`max_active_executions`の既定値は`min(available_parallelism, 64)`である。activeにはactive slotを予約したCreated、Linked、Ready、Running、通常Yielded、Paused、host call/audit待ちを含む。`Yielded(AdmissionQueued { resume_to })`だけはqueue slotへ数える。Pausedや外部I/O待ちをslot外へ出して無制限handleを抱えない。

handle作成時にactive slotが空いていればCreated/Linkedへ進め、空いていなければFIFO admission queueへ入れる。active slot解放時は最古のAdmissionQueued handleのqueue slotをactiveへatomic変換する。queue満杯なら待たずに次を返し、handleを作らない。

```rust
StartError::Backpressure {
    active: usize,
    queued: usize,
    limit: usize,
}
```

queue中もdeadlineとcancelを監視する。slot解放時は最古の`Yielded(AdmissionQueued { resume_to })`だけを対応するCreated/Linkedへ進める。link失敗、cancel、drop時もqueue/active slotを1回だけ解放する。priority APIは提供しない（判断役が選択（PR #379）、Q1=A。理由: priorityは§12.1の規則3と、本節の最古から進めるadmissionの順序を崩す入口で、提供しないと言い切るほうが[マニフェスト](manifesto.md)原則1・8に沿う。priorityが無くてもhostは`PollSlice.max_fuel`（§9.1）・`EngineLimits`・budgetで待ち時間と資源使用量の釣り合いを選べるので、原則4を塞がない。採らなかった案B〔未実装範囲へ移す〕は、priorityの型・順序の規則が目標に無いまま公開APIの追加を先取りする。priorityが要るときは要望のissueから設計PRで本節に目標を書く）。

### 12.1 fairness

run-turn queueはEngine全体でFIFO round-robinとし、continuation自体ではなくexecution IDとwakerだけを保持する。

1. Readyになったexecution IDをqueue末尾へ置く。
2. queue先頭のhandleによる1回の`poll`で最大1 sliceだけ実行する。
3. sliceでyieldしたexecutionを、同時点でReadyなexecutionより前へ挿入しない。
4. host call/audit完了でwakeしたexecutionも末尾へ置く。
5. 同じexecutionが連続してsemantic workを行えるのは、turn取得時点で他にReadyがない場合だけとする。

公平性は実行回数ではなくslice機会に対して保証する。Engineはhost threadをspawnせず、hostがqueue先頭handleをpollしない場合の進捗は保証しない。host adapterとaudit sinkは別のbounded queueを持ち、その詰まりをcaller threadへblocking伝播させずyield/backpressureへ変換する。

## 13. 既存環境変数からの移行

現行の`TSUMUGI_MAX_STEPS`と`TSUMUGI_MAX_COLLECTION_SIZE`はprocess-globalな互換入口として1 release cycleだけ残し、その後削除する。

| 旧設定 | 新設定 | 移行規則 |
|---|---|---|
| `TSUMUGI_MAX_STEPS` | `BudgetConfig.total_fuel` | embedding側の明示値が最優先。CLIで明示値がない場合だけ起動時に1回読み、同値へ設定してdeprecation warningを出す |
| `TSUMUGI_MAX_COLLECTION_SIZE` | `BudgetConfig.max_collection_elements` | 同上。`OnceLock`を使わず、executionごとのconfigへcopyする |

Slice 2で追加した上限のlegacy key（`TSUMUGI_MAX_SINGLE_STRING_BYTES` / `TSUMUGI_MAX_STRING_ALLOCATIONS` / `TSUMUGI_MAX_STRING_BYTES` / `TSUMUGI_MAX_SINGLE_SOURCE_BYTES` / `TSUMUGI_MAX_SOURCE_COUNT` / `TSUMUGI_MAX_SOURCE_BYTES` / `TSUMUGI_MAX_IMPORT_COUNT` / `TSUMUGI_MAX_IMPORT_BYTES` / `TSUMUGI_MAX_LIVE_HEAP_BYTES` / `TSUMUGI_MAX_INPUT_CALLS` / `TSUMUGI_MAX_INPUT_BYTES` / `TSUMUGI_MAX_OUTPUT_CALLS` / `TSUMUGI_MAX_OUTPUT_BYTES` / `TSUMUGI_MAX_HOST_CALLS` / `TSUMUGI_MAX_HOST_REQUEST_BYTES` / `TSUMUGI_MAX_HOST_RESPONSE_BYTES` / `TSUMUGI_MAX_HOST_CALL_BYTES`）も、上表と同じ移行規則で扱う（2026-10-20決定）。各keyは同名の`BudgetConfig`上限fieldへ写し、数値として解釈できない値は無視して既定を残す。

新fuelはstatement・expression等も課金するため、同じ数値でも旧step limitより早く到達し得る。互換mappingは安全側であり、旧実行量の完全再現を保証しない。

legacy envを読むのはCLI adapterだけで、libraryはprocess envを読まない（上書きはenv非依存の入口でCLI adapterから渡す）。CLI adapterは起動時に1回だけenvを読み、tree/VMのfile・stdin・REPLの全経路へ同じ上書きを適用する。budgetの使用量はexecutionごとに新しい`BudgetUsage`で数え、同じcontextの使用量を手で戻す公開APIは持たない。REPLの各入力は1 executionとし、execution間ではcontextだけを引き継ぐ。

`MAX_AST_DEPTH`、`MAX_IMPORT_DEPTH`、`MAX_USER_CALL_DEPTH`は環境変数化せず、language revisionに紐づく構造上限として維持する。将来configurableにする場合もbudgetと同じ不変configへ置き、paused中変更は許さない。

## 14. 実装slice

各スライス小節はそのスライスを実装した時点の記録であり、書き換えない。現在の逸脱・未実装範囲は冒頭の逸脱一覧を見ること。

各sliceは独立PRとし、前sliceの受入基準を満たしてから次へ進む。

### Slice 1: budget型とlegacy adapter

実装状況: ✅ 実装済み（`src/budget.rs`）。詳細は末尾の注記を参照。

- `BudgetConfig`、`BudgetUsage`、`BudgetExceeded`、checked arithmetic、固定優先順位を実装
- CLIだけに旧環境変数adapterを置き、libraryは明示configを要求
- fake monotonic clockを導入
- まだ同期実行だが、既存step/collection検査を共有BudgetLedger経由へ移す

> **Slice 1 実装注記（2026-09-11）**
>
> `src/budget.rs` に第3節の公開型（`BudgetConfig` / `BudgetCounters` / `BudgetPeaks` /
> `BudgetUsage` / `BudgetExceeded` / `BudgetUnit` / `BudgetResource` / `ExecutionPhase`）、
> `ControlStop`、`MonotonicClock` / `MonotonicInstant` / `FakeClock`、`CancellationToken`、
> `BudgetLedger` を実装した。`reserve_all` / `commit` / `refund` は第7節の checked
> arithmetic と第7.2節の固定優先順位（`BudgetResource::priority`）に従う。overflow は
> `requested = u64::MAX` の `BudgetExceeded` へ写像し wrap しない。`BudgetConfig::standard`
> は第3.1節の既定値を、`from_legacy_env` / `for_legacy` は `TSUMUGI_MAX_STEPS` /
> `TSUMUGI_MAX_COLLECTION_SIZE` を fuel / collection 上限へ写す legacy 入口を提供する
> （2026-10-20: `from_legacy_env` は削除し、env の読み込みは CLI adapter へ移した。§13 実装注記）。
>
> tree evaluator（`src/eval.rs`）と VM（`src/vm.rs`）の step 検査を `charge_fuel(1, Run)`
> へ、collection 検査を `check_collection_elements` / 共有 handler へ渡す
> `max_collection_elements` へ一本化した。`builtin_core` の process-global な `OnceLock`
> collection 上限は廃止し、上限値は各 execution の ledger config を単一の正本とする。
> `ControlStop` は Slice 1 では既存の `step_limit` / `collection_limit` エラーへ写像し、
> 観測挙動を変えない（既存 golden・回帰テストは不変で通過）。
>
> Slice 1 の範囲外（後続 slice）: fuel 全 charge point の展開・heap/string/source/I-O
> accounting は Slice 2。ledger charge 経路での deadline / cancel checkpoint は Slice 4
> （現状 `check_deadline` は no-op、cancel は `charge` 前確認のみ）。共有 builtin の
> per-item collection peak 反映（`note_collection_elements`）は未接続。library の
> 明示 config 要求は `Evaluator::with_budget` を用意済みだが、公開 `Engine` API への
> `ExecutionRequest` 配線は Slice 3。

### Slice 2: source・string・heap・I/O accounting

- string accounting（✅ 実装済み）: per-item `SingleStringBytes` と cumulative
  `StringAllocations`/`StringBytes` を `BudgetLedger::charge_string` /
  `charge_result_strings` で課金する。§7.2 の固定優先順位（`SingleStringBytes` <
  `StringAllocations` < `StringBytes`）に従い、cancel を charge 前に確認する。共有
  builtin handler（`builtin_core::dispatch`）が新規生成する String body（scalar /
  List / Dict key を含む）へ、tree（`builtin.rs`）と VM（`vm.rs`）の dispatch 呼び
  出し側から共通で配線する。`control_stop_to_error` を `budget` へ集約し両 engine の
  error 写像を一本化。legacy env `TSUMUGI_MAX_SINGLE_STRING_BYTES` /
  `TSUMUGI_MAX_STRING_ALLOCATIONS` / `TSUMUGI_MAX_STRING_BYTES` を追加。既定上限では
  観測挙動を変えない。この cumulative 会計は §5.3 のとおり解放しても減らさず、String body の
  live heap（`HeapBytes`）per-drop 追跡（PR-b、下記）とは独立した別会計である。string
  リテラル・`+` 連結・f-string 経路の課金（✅ 実装済み）: これらは tree/VM で dispatch を
  経由しないため、生成点で同じ `track_result`（cumulative `charge_string` ＋ live heap
  `new_str`）を通す。tree は `Evaluator::eval_expr` の `Expr::Str`・`BinOp`（`eval_binop`
  結果）・`Expr::FStr` で、VM は `OpCode::LoadConst`（String 定数のみ）・`OpCode::Add`
  （結果が String のとき）・`OpCode::FStrConcat` で課金する。既定上限では観測挙動を
  変えない。tree/VM の f-string リテラル部分の課金差は第14節 Slice 6 で解消済み: VM は
  f-string のリテラル部分を課金しない `LoadConstRaw`（定数表からの非課金 load）で積み、
  tree と同じく結合後の最終 body だけを `FStrConcat` が課金する。これにより tree は
  リテラルを `push_str` で結合し個別課金しない会計と一致し、リテラル部分を含む f-string の
  cumulative `StringAllocations`/`StringBytes` が両 engine で等しくなる。
- source / import accounting（✅ 実装済み）: per-item `SingleSourceBytes` と cumulative
  `SourceCount`/`SourceBytes`、`ImportCount`/`ImportBytes` を
  `BudgetLedger::charge_source` / `charge_import` で課金する。§5.3 のとおり root を
  `source_count` の 1 本目として数え、初めて解決した各 import module を `charge_source`
  （`source_bytes`）と `charge_import`（`import_bytes`）の両方へ課金する。§7.2 の固定
  優先順位（`SingleSourceBytes` < `SourceCount` < `SourceBytes`、`ImportCount` <
  `ImportBytes`）に従い、cancel を charge 前に確認する。課金は `Link` フェーズで最初の
  文を実行する前に行い、超過なら 1 文も実行しない。`ModuleLoader::link` が初めて解決した
  import の生 byte 長（`LoadedModule`）を返し、tree（`Evaluator::charge_link`）と
  VM（`Vm::charge_link`）が共通規則で課金する。root source byte 長は `CompiledScript`
  （tree）と CLI 入口（VM）から渡す。cache hit（解決済み module ID）は数えない。
  legacy env `TSUMUGI_MAX_SINGLE_SOURCE_BYTES` / `TSUMUGI_MAX_SOURCE_COUNT` /
  `TSUMUGI_MAX_SOURCE_BYTES` / `TSUMUGI_MAX_IMPORT_COUNT` / `TSUMUGI_MAX_IMPORT_BYTES`
  を追加。既定上限では観測挙動を変えない。
- I-O accounting（✅ 実装済み）: input / output / host call の count + bytes を
  reserve/commit/refund（§7）で課金する。`BudgetLedger::charge_input`（`InputCalls` +1・
  `InputBytes`）・`charge_output`（`OutputCalls` +1・`OutputBytes`）・`charge_host_call_request`
  （`HostCalls` +1・`HostRequestBytes`）・`charge_host_response_bytes`（`HostResponseBytes`）
  を追加した。§6.1 のとおり stdio（`print` / `input`）は host call でもあるため、input /
  output 固有 counter に加えて `HostCalls` / `HostRequestBytes` / `HostResponseBytes` /
  `HostCallBytes` へも co-charge する（input は request 0・response = payload、output は
  request = payload・response 0）。host call は request 側（count 含む）を境界へ入る前に、
  response 側を結果確定後に課金する 2 phase で、`HostCalls` は request 側で 1 回だけ数える。
  §7.2 の固定優先順位（`InputCalls` < `InputBytes` < `OutputCalls` < `OutputBytes` <
  `HostCalls` < `HostRequestBytes` < `HostResponseBytes` < `HostCallBytes`）に従い、cancel を
  charge 前に確認する。tree（`builtin.rs` の `print` / `input` と PureCore dispatch wrapper）と
  VM（`vm.rs` の `OpCode::Print`・`input` arm・`exec_builtin` dispatch wrapper）が同じ論理
  位置で共通に課金する。byte 数は host 境界で実際に受け渡す UTF-8 payload の長さ（`print` は
  join 後の出力・`input` は受け取った行・`write_file`/`append_file` は書き込み内容・
  `read_file`/`read_lines` は読み込み内容）で、Rust object の capacity や transport header は
  含めない（§6.1）。本 Slice の host call 対象は payload byte が明確な filesystem read/write
  builtin（`read_file`/`read_lines`/`write_file`/`append_file`、`builtin_core::is_host_call_builtin`）
  と stdio に限る。path 判定・env・clock 等その他の host 境界 builtin の課金は Phase 2 の
  capability / host function 配線で扱う（本 Slice では未対象）。エラー写像は
  `control_stop_to_error` に I-O resource の arm を追加し、`error.rs` の
  `input_calls_limit` / `input_bytes_limit` / `output_calls_limit` / `output_bytes_limit` /
  `host_calls_limit` / `host_request_bytes_limit` / `host_response_bytes_limit` /
  `host_call_bytes_limit`（`ErrorKind::IoLimit` = `io_limit`）へ tree/VM 共通で写す。legacy env
  `TSUMUGI_MAX_INPUT_CALLS` / `TSUMUGI_MAX_INPUT_BYTES` / `TSUMUGI_MAX_OUTPUT_CALLS` /
  `TSUMUGI_MAX_OUTPUT_BYTES` / `TSUMUGI_MAX_HOST_CALLS` / `TSUMUGI_MAX_HOST_REQUEST_BYTES` /
  `TSUMUGI_MAX_HOST_RESPONSE_BYTES` / `TSUMUGI_MAX_HOST_CALL_BYTES` を追加。既定上限では観測
  挙動を変えない。descriptor 上限・stream 途中停止・capability 拒否時の課金差
  （§6.1 後半・§6.2）は host function / capability 面が入る Phase 2/4 で扱う。
- heap accounting 基盤（✅ 実装済み）: `AllocationId(u64)` と per-execution
  `AllocationLedger` を `src/budget.rs` に導入した。§5.1 の論理サイズ表を `heap_size`
  純関数群として固定し（Value slot / String body / List・Dict body / tree・VM function
  instance / AST root・node / bytecode chunk / import record / continuation frame /
  handler / rollback journal entry）、`BudgetLedger::charge_heap` / `release_heap` が
  live heap を checked add / saturating sub で管理する。上限超過・overflow は §7.2 の
  固定優先順位（`HeapBytes` は Fuel の次）で `BudgetExceeded(HeapBytes)`、`AllocationId`
  の発行 overflow は 0 へ wrap せず `ControlStop::InternalFailure(AllocationIdExhausted)`
  にする。`usage()` が `live_heap_bytes` / `peak_heap_bytes` を反映する。legacy env
  `TSUMUGI_MAX_LIVE_HEAP_BYTES` を追加。§5.2 の context baseline 走査
  （`charge_context_baseline`、反復 worklist + `Rc` pointer visited set）は純関数として
  実装・単体テスト済みだが、collection の per-drop release 導入（下記）に伴い engine の
  `charge_link` からは呼ばない（tracked collection を二重計上するため）。非 collection の
  pre-existing context 状態（String / 関数 instance / cell）を baseline 課金する配線は、
  それらを per-drop 化する後続 PR で再導入する。
- collection（`List`/`Dict`）の per-drop release（✅ 実装済み、REV-015 案A PR-a）:
  `Value::List`/`Dict` の backing を `Rc<Tracked<T>>` にした。`Tracked<T>` は
  `AllocationId`・論理サイズ・heap 台帳への `Weak` を持ち、生成（`Value::new_list` /
  `new_dict`、`Tracked::new`）で §5.1 の body サイズを課金し、この backing を指す最後の
  `Rc` が drop した瞬間に `Drop` で release する（§5.2）。`Rc::clone`（共有）は無課金。
  COW mutation（index 代入・push/pop）は `detach`＋`retrack` で、単独所有なら in-place・
  共有なら新 `AllocationId` を発番して複製し、要素数変化ぶんの delta だけ課金/release する。
  compile 時の空リテラル定数は untracked（`Tracked::constant`）で、最初の mutation か
  dispatch 境界の `track_result` で tracked へ昇格する。builtin_core が返す collection は
  untracked で作られ、engine（tree=`builtin.rs`, VM=`vm.rs`）の dispatch 境界 `track_result`
  が tracked へ変換して課金する。tree/VM 両対応。未捕捉エラーの AUD-024 rollback では、
  巻き戻しで drop される tracked collection が `Drop` で自動 release される。既定上限では
  観測挙動を変えない。
  - push/pop の full-clone 差（✅ Slice 6 で解消済み）: 以前は VM の `push`/`pop` が
    `CallBuiltin` + 書き戻しで backing を full-clone してから `track_result` で丸ごと課金
    したため、tree の in-place delta 課金と異なり操作のたびに新旧 backing が一瞬同時に
    live になり peak_heap がスパイクしていた。`pop` は `OpCode::PopUpdate`（→
    `list_pop_tracked`）、`push` は `OpCode::ListPushBinding`（→ `list_push_tracked`）で、
    どちらも対象 binding storage を直接書き換える in-place 経路へ切り替え、tree engine と
    同じ primitive を通すようにして解消した。`ListPushBinding` は `SetIndex` と同じ binding
    storage 経路で、backing の `Rc` を operand stack へ複製しないため、一意所有では COW
    複製なしの delta（+32 byte）だけを課金する（共有 backing は tree と同じく `detach_list`
    が COW 複製する）。`builtin_push` / `builtin_pop` は builtin registry 契約・エラー
    メッセージ parity のため残置する。paired test は `tests/scaling.rs` の
    `push_peak_heap_has_no_full_clone_spike_across_engines`（push 後に
    `peak_heap == live_heap` となり full-clone スパイクが消えたことを両 engine で固定）・
    `push_to_shared_backing_has_no_spike_across_engines`（共有 backing への push も
    スパイクしないことを固定）・`push_live_heap_is_linear_not_quadratic_across_engines`
    （live heap が O(N) に収まる回帰ゲート）。
  - 空リテラル差（✅ Slice 6 で解消済み）: 以前は VM の空リテラル `x=[]` / `x={}` が
    untracked 定数のまま変数へ渡り、最初の mutation まで body 24 byte を課金しなかった。
    `OpCode::LoadConst` が String だけでなく List / Dict も `track_result` に通すように
    して、空 collection も materialize 時に body を課金し、tree engine（`eval.rs` の
    `Expr::List`/`Expr::Dict` が `new_list`/`new_dict` で生成時課金）と揃えた。非空リテラル
    は `track_result` が空 body を 1 度課金したあと `ListPush`/`DictInsert` が tracked
    backing へ delta 課金するため合計は tree と同じで二重課金しない。paired test は
    `tests/scaling.rs` の `empty_collection_literal_live_heap_matches_across_engines`
    （スカラー変数の増分を基準に、空 List/Dict の増分がちょうど body ぶん多いことを両
    engine で固定）。
- 未実装（後続 PR）: AST / bytecode chunk / imported module record / rollback journal の
  per-drop 追跡（PR-d）。cell・tree/VM 関数 instance・String body は PR-b/PR-c で実装済み。
  - サブスライス分割（実装順）: collection per-drop（案A PR-a、✅ 済み）に続けて、
    依存の少ない順に切り出す。
    - **PR-b: String body の per-drop 追跡（✅ 実装済み）** — `Value::Str` の backing を
      `String` から `Rc<Tracked<String>>` へ変更し、§5.1 の String body（24 + byte 長）を
      生成時に live heap（`HeapBytes`）へ課金し、最後の参照 drop で release する。
      collection と同じ `Tracked<T>` + dispatch 境界 `track_result` パターンを横展開する。
      cumulative `StringAllocations`/`StringBytes`（§5.3、解放で減らさない累積値）は
      **据え置き**、live heap 課金を**追加**する。両会計は独立で、既定上限では観測挙動を
      変えない。`Value::Str` は他の per-drop 対象（cell / 関数 instance）と backing を
      共有しないため、単独で切り出せる。§5.3 の「substring が既存 body を共有する実装なら
      新規 allocation として数えない」共有最適化は本 PR の範囲外（`Rc<Tracked<String>>`
      は共有可能な backing だが、現行の連結・substring は依然 copy を作る）。
    - **PR-c: cell（`SharedValue`）＋ tree/VM 関数 instance の per-drop 追跡（✅ 実装済み）**
      — `SharedValue` の backing を `RefCell<Value>` から `Rc<Tracked<RefCell<Value>>>`
      へ変更し、cell 生成点（tree=`Env::set`、VM=`ensure_local_cell`）で §5.1 captured
      cell（32 byte）を live heap へ課金し、最後の参照 drop で release する。`Env` に heap
      台帳への `Weak` を持たせ `Env::set` を fallible 化した（呼び出し元へ `?` 伝播）。
      関数 instance は `Value::Fn` / `Value::VmFn` へ `header: Rc<Tracked<()>>` を持たせ、
      生成時（FnDef/Lambda 評価・`MakeClosure`）に §5.1 の header（tree: 64 + 16×captured
      / VM: 48 + 16×upvalue）を課金し drop で release する。captured / upvalue cell 実体は
      cell 側で別途課金するため header ぶんだけ計上し、二重計上しない。`Rc::clone`（共有・
      capture）は無課金。cell は関数 instance（captured / upvalue）の前提となるため一体で
      扱った。collection・String・cell・関数 header がすべて per-drop 追跡されたことで、
      同じ台帳を跨ぐ REPL/実行では pre-existing な live heap が自動的に持ち越され、Link
      境界での baseline 再走査は不要になった（`charge_link` は baseline 課金を行わない。
      `charge_context_baseline` は fresh ledger モデルの埋め込み API 用に温存）。既定上限
      では観測挙動を変えない。
    - **PR-d: AST / bytecode chunk / imported module record / rollback journal の per-drop
      追跡（✅ 実装済み）** — `Value` ツリーに現れず所有構造側が保持する heap object を、
      関数 header（PR-c）と同じ課金トークン（`HeapToken = Tracked<()>`、`Value::new_heap_token`
      で §5.1 論理サイズを課金し drop で release）で追跡する。§5.3「AST または bytecode」に
      従い、tree engine は AST を、VM は bytecode chunk を課金する（両者は同じ source の
      別 artifact で、engine ごとに一方だけを持つ）。
      - **imported module record**（両 engine）: `ModuleLoader` が canonical path をキーに
        record token（`imported_module_record` = 96 + module ID の UTF-8 byte 長）を保持し、
        `loaded` set と寿命を揃える。`charge_link` が Link フェーズで課金し、`forget`
        （未捕捉エラー rollback）または loader drop で release する。VM は loader を所有
        しないため、`Vm::charge_link` が token を返し、loader 所有者（CLI 入口）が登録する。
      - **AST**（tree のみ）: linked program（root + 全 Stmt / Expr node）の §5.1 論理サイズ
        （`AST program root` 64 + 各 node の `ast_node` = 64 + 所有 identifier / string literal
        の byte 長）を `Evaluator::charge_link` が Link フェーズで一括課金し、execution の
        寿命で token を保持する。REPL は入力ごとに前 AST token を release してから課金する。
      - **bytecode chunk**（VM のみ）: root chunk と全 prototype chunk（`MakeClosure` が
        `VmFn` へ clone 共有する `Rc<Chunk>`）を transitive に走査し、各 distinct chunk を
        1 回ずつ `bytecode_chunk`（64 + 16×opcode + 32×constant）で課金する（§5.2 の共有は
        1 回課金）。`Vm::run` は execution 寿命で保持し、`run_repl_chunk` は入力単位で
        入れ替えて release する。持続 closure が prototype chunk を retain する REPL ケースの
        parity 差は第14節 Slice 6 で closure-retain 課金（下記「closure が retain する code の
        課金（A-1）」）として扱う（VM が experimental の間）。
      - **rollback journal entry**（両 engine、AUD-024）: undo journal（tree=`SubmissionJournal`、
        VM=`ReplStackCheckpoint`）が entry ごとに `rollback_journal_entry` の固定 overhead
        （48 byte）を課金するトークンを積み、submission の commit / rollback で journal ごと
        drop して release する（§10「work 量は journal entry 数で有限」）。保持する旧 value の
        到達 payload（List / Dict / String body）は、entry の握る `Rc` が対象 backing の
        `Tracked` を生かし続けるため既に live heap に計上されており、ここで再課金しない
        （§5.2 の共有は 1 回課金）。tree は journal 記録経路を fallible 化して超過を伝播し、
        VM は infallible な checkpoint 経路の超過を保留して次の命令の dispatch 前に
        surface する（F-1 以降は fuel 課金と切り離した全命令の `surface_pending_heap_stop`）。既定上限では観測挙動を変えない。
  - `charge_context_baseline`（全 heap object を 1 回ずつ論理課金する純関数）は、collection・
    String・cell・関数 header・AST・chunk・import record・journal がすべて per-drop 追跡された
    ことで engine の `charge_link` からは呼ばない（tracked 分を二重計上するため）。execution
    ごとに台帳を作り直す埋め込み API（fresh ledger モデル、後続 Phase）用に温存し、単体
    テストで固定する。

### Slice 3: explicit continuation

- tree evaluatorの再帰的な実行状態を明示frame/cursorへ変換
- `ExecutionHandle`と全`ExecutionState`、`poll`、slice fuel、yieldを実装
- 同期`Engine::execute`はhandleをterminalまでpollする互換wrapperにする

Slice 3 は実行の「形」を変える最初の slice であり blast radius が大きいため、Slice 2 と
同様に依存の少ない順で 4 つの sub-PR へ分割する。各 PR は独立して緑（`cargo fmt --check`
/ `cargo clippy --all-targets -- -D warnings` / `cargo test`）にし、前 PR の受入基準を満たして
から次へ進む。VM の continuation / charge parity は第14節 Slice 6（VM が experimental の間）
で扱い、Slice 3 は tree evaluator を対象とする。cancel / pause / transaction（AUD-024 の
journal は既に一部実装済み）と admission / scheduler / host pending は Slice 4/5 の範囲で、
Slice 3 には含めない。

- **PR-a: 公開 state-machine surface + poll-to-terminal core（✅ 実装済み、最初の一手）** —
  §9 の公開型（`ExecutionHandle` / `ExecutionState`（`Created` / `Linked` / `Ready` /
  `Running` / `Yielded(YieldReason)` / `Paused(PausedState)` / `Terminal`）/ `PollSlice` /
  `PollResult` / `HandleError` / `ExecutionRequest`）と、拡張版 `ExecutionOutcome`（§9 の
  terminal payload）を `engine.rs` / `lib.rs` へ追加する。`Engine::create_execution`（Created
  から）/ `Engine::start`（Linked から）と `poll` を実装するが、**この PR の `poll` は初回に
  既存の再帰 `Evaluator::run` を一気に terminal まで回して `PollResult::Terminal` を返す**
  （continuation の書き換えはまだ行わない）。同期 `Engine::execute` は handle を terminal まで
  poll する互換 wrapper にする。§1.1 の統合規則（`ExecutionRequest.budget` は有限
  `BudgetConfig`、公開 `ExecutionHandle` は `!Send + !Sync`、state は §9 の enum だけ）を満たす。
  状態遷移（`Created→Linked→Ready→Running→Terminal`）・`HandleError::Terminal` /
  `InvalidState` の合法/不合法 matrix・terminal 後の再 poll 拒否をテストで固定する。
  allocation 形状を変えないため scaling / golden スイートに影響しない。後続 PR は、この
  安定した handle の裏側を差し替える形で進める。
  - 実装（`src/engine.rs` / `src/lib.rs`）: `ExecutionState`（`Created` / `Linked` /
    `Ready` / `Running` / `Yielded(YieldReason)` / `Paused(PausedState)` / `Terminal`）・
    `YieldReason`・`PauseReason`・`PausedState`・`ResumeState`・`PollSlice`・`PollResult`・
    `HandleError`・`ExecutionRequest`・`ExecutionHandle`・拡張 `ExecutionOutcome`
    （`Completed` / `RuntimeError { error }` / `LinkError { error }`）を追加。`Engine::create_execution`
    （Created から）/ `Engine::start`（Linked から）/ `ExecutionHandle::poll` を実装し、`poll` は
    `Created`/`Linked`/`Ready`/`Yielded` から `Running` を経て `run_phased`（Link/Run を区別する
    `Evaluator` の新 API）を terminal まで回し `PollResult::Terminal { outcome, usage }` を返す。
    Link 失敗を `LinkError`、実行中失敗（予算 limit 系を含む）を `RuntimeError`、正常完了を
    `Completed` へ写す。terminal 後の `poll`/`pause`/`resume` は `HandleError::Terminal`。
    `ExecutionHandle` は `PhantomData<*const ()>` で `!Send + !Sync`。`Engine::execute` /
    `execute_repl_submission` は handle を terminal まで poll する互換 wrapper で、戻り値契約
    （`Ok(Completed)` / `Err(TsumugiError)`）を維持する。最終 `BudgetUsage` は `PollResult::Terminal`
    の `usage` で観測し、outcome には二重に持たせない（§1.1）。
  - 本 PR で意図的に未実装（型を骨格に留めるか公開しない）: slice fuel での `Yielded`（PR-d）、
    `pause`/`resume` の実効化（Slice 4。terminal 以外では `InvalidState` を返す骨格）、`Exited`
    （REV-023）、`Denied`/`HostError`/`BudgetExceeded`/`DeadlineExceeded`/`Cancelled` などの構造化
    terminal（Slice 4/5・Phase 2。現状予算超過は `RuntimeError` 内）、scheduler / admission /
    backpressure（Slice 5）、`CapabilitySet` / `ExecutionId` / `LinkOptions`（Phase 2）。
    `ExecutionRequest` は budget/capability/cancellation を持たない最小骨格で、入口
    `ExecutionRequest::new()` を維持したまま後続で field を足す。
- **PR-b: statement / block / loop の明示 frame stack（✅ 実装済み）** —
  `exec_program` / `exec_block` / `exec_stmt` の Rust 再帰を、ヒープ上の frame stack + cursor
  と driver ループへ置き換える。`EvalResult`（`Return` / `Break` / `Continue`）で Rust
  スタックを巻き戻していた制御フローを、loop / block frame 上の保留 control-flow 状態へ移す
  （§9.3）。`eval_expr` は当面再帰のまま（式は浅く bounded）。§5.1 の `continuation_frame`
  （96 + local slot 32×count）heap 課金をここで配線する。
  - 実装（`src/eval.rs` / `src/env.rs` / `src/builtin.rs`）: `exec_stmt` の戻り値を
    `StmtStep`（`Val` / `Flow(FlowSignal)` / `Enter(Frame)`）化し、複合文（`if` / `while` /
    `for` / `try`）は子 `Frame` を組み立てて `Enter` を返す。`drive_body(root: &[Stmt])` が
    ヒープ上の `Vec<Frame>` + cursor を回す driver ループで、`exec_program` / 関数本体
    （`eval_call`）/ callback 本体（`builtin.rs`）/ `if`・`try`・ループ本体がすべてこれを通る。
    `Frame` は実行中の文列・cursor・`FrameKind`（`Block` / `Scoped` / `While` / `For` / `Try`）・
    `scope_base`（pop 時に `env.truncate_scopes` で巻き戻すスコープ深さ）・`heap_charge`
    （pop 時に release する continuation heap）を持つ。制御フローは Rust スタックの巻き戻し
    ではなく `unwind_flow` が frame stack を明示 unwind する（`Return` は関数活性境界まで、
    `Break` は最も近いループ controller frame ごと畳み、`Continue` は本体 Block frame だけ
    畳んで controller を残し次反復へ）。ループ本体は反復ごとに `push_scope` / continuation
    heap 課金 / `pop_frame` で release し、`for` は開始時に materialize した items と反復元
    collection を frame が保持して従来の live-heap 寿命を保つ。`while` / `for` の末尾
    `count_step`（fuel）は controller frame の `pending_step` で「本体 1 反復完了後・次反復
    判定前」に課金する（従来順序を保つ）。`try` 本体は `Try` frame で実行し、捕捉エラーは
    `handle_error` が内側 frame を畳んで try スコープを解放し、独立スコープの catch 本体を
    実行する `Scoped` frame へ差し替える（fuel/collection/heap を含む全 `Err` を捕捉する
    現行挙動を保つ）。continuation_frame heap は各 frame の push で課金し pop / unwind /
    catch 切替で release する。`env` に `scope_depth()` / `truncate_scopes(len)` を追加。
    関数呼び出し（`eval_call`）と式（`eval_expr`）は当面 Rust 再帰のまま（PR-c / 式は bounded）。
  - 観測挙動はほぼ不変で、次の 2 点だけ意図的に変える。(1) ループ外 break/continue の
    エラー行を offending 文の行へ揃える。従来の tree は関数呼び出し位置やトップレベル
    複合文の行を使い VM（compile 時に break/continue 文の行で検出）と食い違っていたが、
    frame stack 化に伴い `EvalResult::Break`/`Continue` に文の行を持たせ、両 engine を
    文の行へ収束させた（tree/VM parity の改善、roadmap の差異縮小方針に沿う）。既存の
    canonical error inventory はトップレベル bare break のみで両 engine 一致は保たれる。
    (2) `if` ブロックが continuation_frame（96 byte）を新たに課金するため、これに依存する
    heap rollback テストの上限を 7400→9600 へ調整した（release 性質は不変）。ループの反復
    進行中（2 回目以降の condition 再評価・末尾 count_step・反復スコープ束縛）に起きる
    エラーも、従来どおり囲む try/catch が捕捉する（advance 経路のエラーも `handle_error`
    を通す）。`tests/fixtures/try_catch` に 2 回目 condition 評価での除算エラーを catch する
    ケースを追加して両 engine で固定した。
- **PR-c: 関数呼び出し・try handler の明示 frame（✅ 実装済み）** —
  `eval_call`（`src/eval.rs`）と callback の `call_fn_value`（`src/builtin.rs`）を、Tsumugi の
  関数呼び出しがヒープ上の明示 call frame を積む形へ変換した。VM の `CallFrame` をミラーする
  `FrameKind::Call { saved_scopes }` を追加し、PR-b の driver ループを共通 `run_driver(frames)`
  へ抽出したうえで、関数本体を実行する `drive_call_body(def, saved_scopes)` を新設した。
  - 実装（`src/eval.rs` / `src/builtin.rs`）: `drive_call_body` は呼び出す関数の
    `Rc<FnDef>` をローカルに保持し、その `&def.body` を root frame（`FrameKind::Call`）の
    `stmts` として渡す。これにより関数本体とそこから派生する `if`/ループ/`try` の子 frame は、
    呼び出しの実行全体を通じて生きるこの本体 AST を借用でき、frame が自分の所有物を借用する
    自己参照を避ける（VM の `CallFrame` が `Rc<Chunk>` を保持し `ip` で index するのに対応する
    tree 側の等価な寿命管理）。呼び出し元（`eval_call` / `call_fn_value`）は従来どおり
    `env.push_call_frame()` でスコープを退避し、captured cell / self-binding / 引数を束縛し、
    call trace を積んでから、退避情報 `saved_scopes` を Call frame へ預けて `drive_call_body`
    を呼ぶ。Call frame の `pop_frame` が全終了経路（正常完了・`return` unwind・エラー unwind）で
    `env.pop_call_frame` と call trace の巻き戻しを一元的に行うため、呼び出し元側の終了経路ごとの
    手動 `pop_call_frame` を廃止した。`return` は `unwind_flow` が最も近い Call frame まで畳んで
    値を produce し、`break` / `continue` は Call 境界を越えられずそのまま surface して
    呼び出し側が「ループ外」エラーへ写す。エラー時の call trace は、frame を畳む前・発生時点の
    `call_stack` を snapshot する `attach_trace`（VM の `attach_trace` と同じ意味論。`with_trace`
    は既存トレースを上書きしないため最深トレースが保たれる）で付加する。
  - `try` handler は PR-b 時点で既に明示 frame stack 上の `FrameKind::Try`（driver の
    `handle_error` が最も近い Try frame へ unwind）として実装済みであり、これが VM の
    `TryHandler` stack をミラーする。PR-c の Call frame 化により、呼び出し境界を越えた
    try/catch の伝播は各呼び出しが独立した frame stack を持つ再入で自然に分離される
    （callee 内の未捕捉エラーは Call frame まで畳んで `Err` を返し、caller の driver が
    自分の Try frame で捕捉する）。
  - 観測挙動は不変（既存の全テストが緑、tree/VM の trace・catch・parity テストを含む）。
    再入モデルのため呼び出しごとに 1 段の Rust フレームは残り、`main.rs` の 8 MiB 実行
    スレッドと呼び出し深度上限 128 はそのまま維持する（式評価が Rust 再帰である以上、
    呼び出しごとの Rust フレーム除去には明示式評価スタックが必要で、それは範囲外。
    スレッド縮小は必須ではない）。
- **PR-d: slice fuel + yield（✅ 実装済み）** —
  slice-fuel accounting 層を足し、driver ループの文/反復境界で `PollSlice::max_fuel` を確認する。
  total 残量はあるが slice 残量を超える場合は、その境界で `Yielded(SliceFuelExhausted)` を返し、
  `poll` が評価器の永続 frame stack（PR-d-1）から再開できるようにする（§4.2）。PR-d-1 で
  continuation を Evaluator 所有の `'static` frame stack へ移したことが前提になる。
  - 実装（`src/eval.rs`）: `Evaluator` に `slice_fuel_used` / `slice_fuel_limit` と実行
    セッション `session: Option<RunSession>` を追加。`count_step` は total fuel を課金しつつ
    `slice_fuel_used` を加算する（accounting のみ。yield 判定はしない）。`run_driver` は
    `(stop_depth, can_yield)` を取り、ループ先頭（= 文/反復ディスパッチ前の安全な境界）で
    `can_yield && slice_exhausted()` なら `DriveOutcome::Yielded` を返す。`slice fuel` は total を
    補充せず、実際に commit した fuel だけが total usage へ乗る（§4.2）。
  - **yield は文/反復境界だけ**（式評価は当面 Rust 再帰のままで、途中では resume 不能）。
    そのため関数呼び出し（`drive_call_body`）と callback、および同期実行
    （`run` / `run_phased` / `run_repl_submission` → `exec_program`）は `can_yield = false` で
    活性を完了まで回す。yield できるのは poll 経路のトップレベル driver だけ。呼び出し中に
    消費した slice fuel は、呼び出しから戻った後トップレベル境界で yield 判定される。
  - resume 契約: `Engine::create_execution` / `start` の初回 `poll` が `Evaluator::begin_execution`
    （link + Link フェーズ課金 + 深度検証 + REPL transaction 開始 + root frame push。実行しない）
    を呼び、以降の `poll` が `Evaluator::run_slice(max_fuel)` で永続 frame stack を 1 slice ぶん
    進める。`run_slice` は `None`（yield）/ `Some(Ok/Err)`（terminal）を返し、terminal で
    transaction を commit/rollback し未完了 module の解決マーカーを巻き戻す（AUD-024 / AUD-030）。
    `ExecutionHandle::poll` は `None` を `PollResult::Yielded { SliceFuelExhausted }` ＋
    `ExecutionState::Yielded(..)` へ、`Some` を `PollResult::Terminal` へ写す。`Engine::execute`
    は引き続き handle を terminal まで poll する互換 wrapper（`drive_to_outcome` が `Yielded` を
    ループで回して terminal まで駆動）。
  - `ExplicitYield`（`YieldReason`）は enum として定義済みだが、script / host の明示 yield surface が
    まだ無いため trigger は未配線（host 要求 pause と同じく Slice 4 で実効化する）。
  - 受入: `tests/engine_api.rs` に、小さな `max_fuel` で複数 slice に分かれて yield しても、大きな
    slice 1 回と同じ terminal かつ同じ total committed fuel へ到達すること、yield 後の resume と
    terminal 後 poll 拒否、slice 分割を跨いだ runtime error terminal を固定した。観測挙動は
    既定 slice（同期経路）では不変（597 tests green）。

#### tree-expr-continuation 実装スライス（Slice 3 PR-e：式の途中での slice yield）

> 状態: **未実装**（設計のみ）。本小節は Slice 3 PR-a〜d が文・反復・関数呼び出し・try を明示
> frame 化し slice fuel/yield を入れた後に残る最後のピース——**式評価（`eval_expr`）を Rust 再帰から
> 明示 continuation へ移し、式の途中でも slice yield/resume できるようにする**——の slice 定義で
> ある。§9.3「continuation へ保存するもの」と §4.2 の slice yield が正本であり、以下は本スライスで
> 実装する範囲・観測挙動・意図的に延期する逸脱の記録である。進捗の唯一の正本は
> [`slices.toml`](slices.toml) と [ロードマップ](roadmap.md)。
>
> **sub-slice への分割（issue #248）**: 本スライスは 1 晩・1 PR に収まらないため、下記
> 「tree-expr-continuation の sub-slice 分割」で **e1〜e4 の 4 つの実装スライス**
> （`tree-expr-continuation-e1` 〜 `-e4`）へ割った。本小節の本文（yield 要件・境界規則・保存する状態・
> heap 課金・観測挙動・逸脱・受入基準 TEC-AT-01〜07）は 4 つに共通の設計であり、各 sub-slice の範囲・
> 担当する受入基準の部分・完了条件はそれぞれの小節に書く。他の小節・スライスが
> 「`tree-expr-continuation` が done」と書くときは **e1〜e4 がすべて done** を意味する。
>
> **後続スライスとの関係**: 本スライスは F-2（§4.1 課金表の主要 charge point への移行）の前段依存
> である。F-2 で charge point が statement / expression / operation 単位になると式評価中に slice
> fuel を使い切りうるが、現状 tree は文/反復境界でしか yield できない（PR-d「yield は文/反復境界
> だけ（式評価は当面 Rust 再帰のままで、途中では resume 不能）」）。本スライスはこの制約を外す。
> **本スライスは課金値を変えない**（課金点・課金値の移行は F-2）。現状の fuel 課金のまま「式の途中で
> yield できる」ようにするだけである。

PR-c の実装注記が明言するとおり、現状は「式評価が Rust 再帰である以上、呼び出しごとの Rust フレーム
除去には明示式評価スタックが必要で、それは範囲外」として式を Rust 再帰（`eval_expr`）のまま残した。
また PR-d では関数本体（`drive_call_body`）と callback（`call_fn_value`）を `can_yield = false` で
完了まで回しており、**yield できるのは poll 経路のトップレベル driver だけ**である。本スライスは
(1) 式評価に明示式評価スタックを導入して式の途中でも yield でき、(2) 関数本体・callback 本体の
文/反復境界も yield できるようにする。

- **yield 要件（manifesto 原則 1・4、§4.2「yield 可能点に挟まれた区間の slice 超過」）**: slice yield の
  目的は「ホストへ制御が戻るまでの最悪時間を有界に保つ」ことである。これを満たすため、**呼び出しと
  反復は必ず yield 可能点にする**。具体的には次を満たす。
  - **関数本体・callback 本体の文/反復境界を必ず yield 可能にする**。現状 `drive_call_body` /
    `call_fn_value` は `can_yield = false` なので、本スライスで yield 可能化する（下記「can_yield の
    伝播」）。再帰やループが何段あっても、各段の文/反復境界で yield できる。
  - **callback を呼ぶ builtin（`map` / `filter` / `each` の 3 つ）も、callback 本体の途中で
    中断・再開できる**ようにする（これらは要素ごとにユーザー関数を呼ぶ。`sort` は callback を取らない
    bulk work なので対象外＝下記「逸脱」と §4.2）。
  - **yield できなくてよいのは「呼び出しも反復も含まない区間」だけ**。その区間は §4.2 の例外（slice を
    超過して実行し、区間を抜けた最初の yield 可能点で yield。超過量は区間の AST 分 + callback なし
    bulk builtin 1 回分で有界）に従う。この区間に限り Rust 再帰を残すハイブリッド（下記 1b）を許す。

- **方針＝ハイブリッド（1b）＋明示側に置く境界規則**: `eval_expr` の全式種を一括で明示スタック化する
  （Rust 再帰を式から完全排除する）案（1a）は blast radius が過大なため採らない。代わりに **yield 要件を
  満たすのに必要な部分を明示側に置き、残りは Rust 再帰を許す**。境界規則は上の yield 要件から次のように
  固定する（実装者が線引きを独自判断しない）。
  - **呼び出し（call 式）と反復を含む部分式は、必ず明示 frame 側に置く**（operand stack 上で段階評価し、
    各境界で yield 可能）。これにより「呼び出し・反復を必ず yield 可能点にする」要件を式レベルでも守る。
  - **呼び出しも反復も含まない部分式（純粋な算術・比較・index・リテラル等）は Rust 再帰を残してよい**。
    その区間は §4.2 の超過例外で有界。
  - **維持する防御**: 深度上限 `MAX_USER_CALL_DEPTH = 128`（`src/limits.rs`）と `main.rs` の 8 MiB
    実行スレッドは**変えない**。呼び出しごとの Rust フレームは残り、停止性・stack overflow 防御として
    引き続き機能する。
  - **過大なら分割してよい**: Slice 3 が PR-a〜d に割れたのと同様、sub-PR へ割ってよい。特に
    **`map` / `filter` / `each` の callback 段階実行化は sub-PR に分けてよいが、本スライスの完了条件に
    含める**（yield 要件の一部であり、別スライスへ外さない）。分割する場合は `slices.toml` を更新し
    本小節を割る。→ 4 つの sub-slice へ分割した（下記「tree-expr-continuation の sub-slice 分割」）。

- **continuation へ保存する式の評価状態（§9.3 の具体化）**: §9.3 は「AST cursor・value stack・
  保留中 operand」を heap 上の明示状態として保存することを要求する。本スライスが式について保存する
  のは次である（PR-b/c が導入した `Frame` / `FrameKind` と同じ continuation heap に載せる）。
  - **operand stack**: 評価済みだがまだ親式に消費されていない中間値（例: 二項演算 `a + b` で `a` を
    評価し終え `b` を評価する間の `a` の値）。
  - **評価中の式 cursor**: 親式のどの operand まで評価したか（二項/単項の左右、index の `obj[idx]` の
    どちら、call の callee と各引数のどこまで）。
  - **callee 引数の部分評価結果**: call 式 `f(e0, e1, …)` で、評価済みの引数値と次に評価する引数の
    index。関数へ入る前（引数評価の途中）で yield できるようにする。
  - **callback builtin の反復状態**: `map` / `filter` / `each`（callback を取る 3 builtin）で、
    処理済み要素・次の要素 index・部分結果。段階実行化（下記）でこれを frame に載せる。
  - **保留中の二項/単項/index 演算**: operand を揃えた後に適用する演算の種別（左右の値が揃うまで
    frame に保持し、揃ったら適用）。

- **式 frame の heap 課金（確定。e2a 判断 Q1 = A で改訂）**: 式評価用の frame は**課金しない**
  （`heap_charge` は 0。Call frame・`while` / `for` の制御 frame と同じ）。式 frame は同期経路の Rust 再帰（未課金）が
  持つ評価状態を heap へ移しただけで、新しい値は作らない。operand stack に積む中間値は **既に別所で課金済みの
  `Value`（§5.1 の Value slot ほか）を参照保持するだけ**とし、operand を二重課金しない（§5.2「共有は 1 回課金」に従う）。
  当初は式 frame を 96 byte 固定で課金すると確定していたが、e1 判断 Q1 = A（terminal まで poll する入口は観測値を
  変えない）と両立しないため改めた（理由は e2a 小節の判断の記録）。
  - **同期経路でも式 frame を積むか**: **積まない**。同期経路（`can_yield = false`）は従来どおり
    `eval_expr` の Rust 再帰で評価し、式 frame を作らない。式 frame を作る（明示段階評価に入る）のは
    **poll 経路（`can_yield = true`）で、かつ呼び出し・反復を含む部分式**に限る。これにより同期経路の
    `live_heap` / `peak_heap` は現状と 1 byte も変わらず、TEC-AT-03（観測不変）と衝突しない。VM との
    heap 差も広げない。
  - **poll 経路の heap 観測**: 式 frame は課金しないので、poll 経路で式 frame を積んでも `live_heap` /
    `peak_heap` は同期経路と一致する。

- **yield 可能点（どの境界で slice 残量を確認するか）**: §4.2「次の論理 charge が slice 残量を
  超える場合は charge せず `Yielded(SliceFuelExhausted)` を返す」を次の境界に適用する。
  - **関数本体・callback 本体の文/反復境界**（新規。PR-d のトップレベル driver と同じ判定を、
    `can_yield = true` を伝えた `drive_call_body` / callback 本体でも行う）。
  - call 式の各引数評価の境界（引数 i を評価後・引数 i+1 を評価前、全引数評価後・本体へ入る前）。
  - 呼び出し・反復を含む部分式の operand 境界（二項/単項/index で、その operand が呼び出し・反復を
    含む場合）。
  - callback builtin の各要素処理の境界（`map`/`filter`/`each` の 1 要素ごと。`sort` は callback を
    取らないため対象外＝下記「`sort` は対象外」）。
  - **呼び出しも反復も含まない区間は yield 可能点を置かない**（§4.2 の超過例外で有界）。
  - **現状課金では一部境界が 0 fuel になりうる**（課金値の付与は F-2）。本スライスは「境界で slice
    残量を確認し超過なら yield する機構」を置き、F-2 がそこへ課金値を載せる。課金 0 の間でも機構が
    resume 可能であることは、下記の強制 yield フックを使った受入基準で固定する。

- **can_yield の伝播（関数本体・callback を yield 可能にする）**: 現状 `drive_call_body` /
  `call_fn_value` は `run_driver(base, false)` で回す。本スライスはトップレベル driver から再入した
  活性にも `can_yield = true` を伝える。Call frame（`FrameKind::Call`）に「この活性が yield 可能か」を
  持たせ、resume 時にトップレベル driver が永続 frame stack を根から回し直す形にする（Rust 再帰で
  再入した活性の途中状態を Call frame 経由で復元する）。
  - **新しい can_yield が効くのは slice fuel の yield（と下記の強制 yield フック）だけ**とする
    （issue #248 の分割設計で確定。設計 PR で kamonabe の判断を仰いだ点）。次の 2 つは本スライスでは
    従来どおり **トップレベル driver の文/反復境界だけ**で判定し、関数本体・callback 本体の中では
    判定しない。
    - cooperative host-call の `may_yield`（`can_yield_ctx`）。関数本体内で Pending を返す adapter は
      従来どおり `may_yield = false` の規約違反（catch 不能 `InternalControl`）になる
      （`tests/scheduler_api.rs` の `may_yield_false_requires_ready_else_internal_terminal` を無改変で
      緑に保つ。関数本体内の host-call Pending yield は resume 時の「同じ文の再評価」規約〔§4.7〕を
      関数本体の文へ広げる別設計が要るため、本スライスの範囲外＝下記「逸脱」）。
    - audit drain queue 上限の yield（[決定性・実行時監査仕様](determinism-and-audit.md) §10.2・B-2 の
      「トップレベル境界で判定する soft limit」。B2-AT-09 を無改変で緑に保つ）。
    - 実装上は「slice fuel を確認してよいか」と「トップレベル（関数本体・callback 本体の外）か」を
      別の可否として持つ。前者は本スライスで関数本体・callback 本体へ伝え、後者は伝えない。

- **callback builtin の段階実行化（`map` / `filter` / `each`）**: callback を取る 3 builtin は
  要素ループを明示 frame の反復に載せ、1 要素ごと（callback 呼び出しの前後）に yield 可能点を置く。
  callback 本体は呼び出しなので、本体の文/反復境界でも yield できる（上記 can_yield 伝播）。
  per-element 課金・エラー伝播・結果の順序は現状維持。この段階実行化は sub-PR に割ってよいが本スライスの
  完了条件に含む（上記）。
  - **`sort` は対象外**: `sort` は callback（ユーザー関数）を取らない bulk work で、`sort_by` の
    comparator は値比較と per-element 課金（`compare_budgeted`）だけを行う。これを中断可能ソートへ
    書き換えると比較回数・比較順・同期経路の fuel（比較ごとの per-element 課金）が変わり TEC-AT-03
    （fuel 値不変）を破るため、**書き換えない**。`sort` を含む callback なし bulk builtin は §4.2 の
    超過例外（1 回分を collection / string 上限で有界な超過として扱う）に従い、chunk 境界 yield は
    後続スライス `fuel-bulk-chunk-yield` へ延期する。

- **変更箇所**（Slice 3 PR-b/c と同じ粒度で列挙）:
  - `src/eval.rs`: `eval_expr` のうち呼び出し・反復を含む部分を、yield 可能点を持つ段階評価
    （operand stack と式 cursor を `Frame` に載せる）へ分解する。`FrameKind` に式評価用 variant
    （仮称 `Expr { … }`：評価中の式・operand stack・cursor・保留演算）を追加。`drive_call_body` /
    callback 本体を `can_yield = true` で回せるようにし（Call frame に yield 可否を持たせ、resume を
    トップレベル driver から駆動）、`run_driver` のループで新境界の slice 確認を行う。呼び出しも反復も
    含まない部分式は Rust 再帰を残す。
  - `src/builtin.rs` / `src/builtin_core.rs`: `map` / `filter` / `each` の callback 呼び出しを
    段階実行へ書き換える（要素ループを明示反復に載せ要素境界で yield 可能に）。per-element 課金・
    エラー伝播・結果順序は現状維持。`sort` は書き換えない（上記）。
  - `src/env.rs`: 式 frame の一時値スコープ管理が要る場合は PR-b の `scope_depth()` /
    `truncate_scopes(len)` 規律に合わせる（新 API は増やさない想定）。
  - continuation heap 課金: 式 frame は課金しない（`heap_charge` 0、上記）。poll 経路でだけ積む。
  - docs: 本小節、§4.2（超過例外・追記済み）、§14 冒頭・第1節の「Slice 3 完了」記述（PR-e 追加に更新
    済み）、roadmap。VM は対象外（下記逸脱）。

- **観測挙動（原則不変・やむを得ない差は明記）**: 既定 slice（同期経路）では観測挙動を不変に保つ
  （fuel 値・成否・エラー行・trace・stdout・exit code・heap が現状と一致。同期経路は式 frame を積まない）。
  原則として明示化は evaluation 順・課金順・エラー発生点を変えない。ただし Slice 3 PR-b が「ループ外
  break/continue のエラー行」を tree/VM parity のため意図的に変えた前例があるとおり、避けられない差が
  出る場合は F-1/PR-b と同じく**意図的な差として本小節と実装 PR 本文に明記し、tree/VM parity を壊さ
  ない方向でのみ許容する**。既存テスト群（597 tests green を含む）は無改変で緑を回帰条件とする。
  期待値が説明できない形で変わる場合は実装を止めて報告する。

- **逸脱（本スライス時点で残るもの、後続スライスへ延期）**:
  - **式の charge point と課金値**（statement/expression/operation への移行）は **F-2**。本スライスは
    yield 機構だけを置き、課金は現状のまま。
  - **VM の式レベル continuation**は Slice 6（VM が experimental の間）。本スライスは tree evaluator
    だけを対象とする。VM の continuation/charge parity は Slice 6 で扱う。
  - **Rust 再帰の完全排除・深度上限や 8 MiB スレッドの見直し**（1a）は本スライスの範囲外。呼び出しも
    反復も含まない部分式は Rust 再帰を残し、既存の防御（128 / 8 MiB）を維持する。
  - **callback なし bulk builtin（`sort` / `join` / 文字列繰り返し / 大 collection コピー等）の
    chunk 境界 slice yield** は本スライスの範囲外。§4.2 の超過例外で 1 回分を有界な超過として扱い、
    chunk（256 element / 16 KiB）単位での中断・再開は後続スライス `fuel-bulk-chunk-yield`
    （[`slices.toml`](slices.toml) に `todo` 登録）へ延期する。本スライス時点では bulk の chunk 前
    確認は deadline / cancel のみ（slice yield はしない）。
  - **cancel / deadline / pause の新境界 checkpoint**: Slice 4 の cancel/deadline checkpoint は現状
    文/反復境界にある。本スライスで増えた callback 本体の文/反復境界には同じ checkpoint が自然に乗る
    （同じ driver ループ境界のため）。式の operand 境界にまで cancel/deadline checkpoint を広げるかは
    本スライスでは行わず、必要になれば Slice 4 側の課題として扱う（yield 要件は呼び出し・反復境界で
    満たされるため、最悪時間の有界性には影響しない）。
  - **関数本体・callback 本体の中での cooperative host-call Pending yield と audit drain queue 上限
    yield** は本スライスの範囲外（上記「can_yield の伝播」）。どちらも従来どおりトップレベル境界だけ。

- **受入基準（tree-expr-continuation）**:
  - **TEC-AT-01（yield 可能点を跨ぐ実行が同期経路 `run` と一致）**: 下記の強制 yield フックを有効に
    して、全 yield 可能点（関数本体・callback 本体の文/反復境界・call 引数境界・呼び出し/反復を含む
    operand 境界）を跨ぐ script を実行したとき、複数 slice に分かれて yield/resume した結果が、
    **同期経路（`can_yield = false` の `Evaluator::run` 系。式 frame を積まない Rust 再帰経路）の結果と一致する**。一致を検査するのは
    **stdout・`committed.fuel`・terminal 種別、および非 `Yielded`/`Resumed` の audit event 列**
    （`Yielded`/`Resumed` は同期経路には現れないため列全体の一致は求めない。対・順序は TEC-AT-06 で
    別途検査する）。`map` / `filter` / `each` の段階実行版についても、同期経路 `run` と同じ結果・同じ
    副作用順序・同じ `committed.fuel` になることを含める（manifesto 原則 5：2 経路が同じ結果を出す）。
  - **TEC-AT-02（resume の忠実性）**: operand stack・式 cursor・callee 引数の部分評価結果・callback
    反復状態が yield を跨いで保存され、resume 後に続きから評価される。深い式・入れ子 call・引数途中・
    callback 途中での yield を強制 yield フックで起こし、resume 後の最終値と副作用列が **同期経路
    （`Evaluator::run` 系）と一致**することを固定する。
  - **TEC-AT-03（観測不変・既定 slice）**: 既定 slice（同期経路と、terminal まで poll する入口）で、既存の全テスト（`tests/`・
    unit test、PR-d の 597 tests green を含む）が無改変で緑。fuel 値・エラー行・trace・stdout・
    exit code・`live_heap`/`peak_heap` が現状と一致する（式 frame は課金しないため heap 不変）。
    terminal まで poll する入口（`Engine::run`〔CLI を含む〕・alpha `execute` / `execute_repl_submission`・
    `run_for_repl`）は driver 経路を通ってよいが、観測値は現状と一致させる（e1 判断 Q1 = A）。
    意図的に変えた差があれば本基準の例外として明記し、tree/VM parity を壊さないことを別途固定する。
  - **TEC-AT-04（深度・停止性の維持）**: `MAX_USER_CALL_DEPTH = 128` と stack overflow 防御（8 MiB
    スレッド）が従来どおり機能する。深い再帰・深い式が従来と同じ `call_depth_limit` / 有限停止で止まる。
  - **TEC-AT-05（transaction / 境界の不変）**: yield は transaction 境界ではない（§10 規則7）。
    関数本体・callback 途中・式途中の yield を跨いでも submission journal が保持され、terminal でだけ
    commit/rollback される（PR-d の transaction 不変条件を新境界 yield でも維持）。
  - **TEC-AT-06（超過量の上限）**: 関数本体のループや callback 本体のループを持つ script を最小 slice
    （`PollSlice::max_fuel = 16`）で実行したとき、1 回の poll で実際に実行される fuel が
    **slice + 規定の超過上限以内**に収まる。超過上限はテストで計算できる形で次と定める（§4.2）。
    - **現行課金（`fuel_accounting_revision = 1`、F-1）下では式に fuel がかからない**ため、呼び出しも
      反復も含まない区間の AST 分の超過は **0**。したがって本スライス時点のテストは実質「超過 ≒
      bulk builtin 1 回分」を見る（式起因の超過が乗るのは F-2 で式を課金してから＝そのとき上限に
      「1 区間の node 数」が加わる）。
    - **callback なし bulk builtin 1 回分**の超過は、その builtin を**同じ入力で単独実行したときの
      fuel**（テストで事前に測る実測値）を上限とする。`sort` は要素数ぶんの走査課金（n）に加えて
      比較ごとの `compare_budgeted` per-element 課金（比較回数 O(n log n)）が乗るため、1 回分の fuel は
      n を超えうる（「collection 上限以内」ではない）。`join` など表示系も per-node 課金で同様。入力
      サイズ自体は `max_collection_elements`・`max_single_string_bytes` 等の config 上限で有界なので、
      1 回分の fuel はそれらから決まる有限値になる。テストは「bulk builtin を単独実行した fuel」を測り、
      その bulk を含む区間を最小 slice で回したときの 1 poll 実行量が「slice + 測った単独 fuel + 区間の
      AST 分」以内であることを exact に確認する。
    - 再帰・ループが何段あっても 1 poll の実行量がこの上限を超えない（呼び出し・反復が必ず yield
      可能点だから）。audit の `Yielded`/`Resumed` が対になり順序が単調であることも併せて検査する。
  - **TEC-AT-07（callback 中断の resume 一致）**: `map` / `filter` / `each`（callback を取る 3
    builtin）の callback の途中で yield/resume しても、同期経路 `run` と結果・副作用順序・
    `committed.fuel` が一致する。要素途中・callback 本体途中の両方で強制 yield を起こして固定する。
    （`sort` は callback を取らないため本基準の対象外。§4.2 の超過例外で扱う。）

- **テスト専用の強制 yield フック**: 現状課金では新 yield 可能点の多くが 0 fuel になり、最小 slice
  fuel でも yield しない。そこで **`unstable-bytecode`（または `cfg(test)`）限定で「全 yield 可能点で
  必ず yield する」モード**を追加し、TEC-AT-01/02/06/07 はこのフックで全境界を通す。production 既定
  経路には影響しない（PR-d の `EngineBuilder::audit_slice_fuel` と同じ、テスト専用の内部 surface）。

- **テスト配置**: yield 可能点を跨ぐ一致・resume 忠実性・超過上限・callback 中断（TEC-AT-01/02/06/07）
  と transaction 不変（TEC-AT-05）は `tests/engine_api.rs`（PR-d の slice 分割テストと同じ場所。
  強制 yield フック使用）、観測不変（TEC-AT-03）は既存 `tests/` の無改変緑、深度・停止性（TEC-AT-04）は
  `tests/defensive*.rs`、式 frame の push/pop・operand stack・callback 反復状態の不変条件は
  `src/eval.rs` / `src/builtin*.rs` の unit test。

#### tree-expr-continuation の sub-slice 分割

> 状態: **未実装**（設計のみ。issue #248 で分割を決定）。各 sub-slice の進捗は [`slices.toml`](slices.toml)
> の `tree-expr-continuation-e1` 〜 `-e4` を正とする。共通の設計（yield 要件・境界規則・保存する状態・
> heap 課金・観測挙動・逸脱・受入基準 TEC-AT-01〜07 の本文）は上の「tree-expr-continuation 実装スライス」
> 小節にあり、ここでは割り方と各 sub-slice の範囲・完了条件だけを書く。

**割り方の考え方**: yield を Rust 再帰の内側から外へ出すには、yield する活性とトップレベル driver の間に
Rust 再帰（`eval_expr` / `eval_call` / builtin の要素ループ）が 1 段も挟まっていてはならない。そこで
「どの呼び出し位置を driver の frame 上で実行するか」を段階的に広げる。

- **e1** は、式の途中を保存しなくてよい唯一の位置＝**文の値そのものがユーザー関数呼び出し**である場合だけを
  driver 上で実行し、関数本体の文/反復境界を yield 可能にする（式 frame は作らない）。
- **e2** は式 frame（`FrameKind::Expr`）を入れ、**ユーザー関数呼び出しを含む任意の部分式**へ広げる。
- **e3** は **builtin / host function / cooperative host function の呼び出しで、引数が呼び出しを含む場合**の
  引数境界へ広げる。
- **e4** は **`map` / `filter` / `each` の要素ループ**を明示反復にし、callback 本体の文/反復境界と要素境界を
  yield 可能にする。

各 sub-slice の完了時点で、まだ driver 上へ移していない呼び出し位置は**従来どおりの Rust 再帰（その内側は
`can_yield = false` で完了まで実行）**のまま残す。これは sub-slice 間の過渡状態で、その位置の yield は
§4.2 の超過として後続 sub-slice が done になるまで残る（各 sub-slice の「範囲外」に列挙する）。
過渡状態でも観測挙動は壊れない。同期経路は全 sub-slice を通じて一切変えず、poll 経路は「yield できる位置が
増える」以外の観測（結果・副作用順序・`committed.fuel`・エラー行・trace・catch 先）を同期経路と一致させる。

**依存順**: e1 → e2 → e3 → e4 の直列とする（e2 は e2a → e2b の 2 つに割った。下の「e2 実装スライス」の注記）。e2・e3・e4 はいずれも e1 の「呼び出しを driver 上の Call frame で
実行し、終了時に値を受け渡し先へ届ける」機構の上に載る。e3 は e2 の式 frame（引数の operand stack）を使う。
e4 の要素ループ自体は e1 だけに依存するが、`map` が式の中や他の呼び出しの引数に現れる組み合わせ
（例: `len(map(xs, f))`、`map(xs, f) + ys`）は e2・e3 の上でしか yield 可能にならず、TEC-AT-01 の全境界を
跨ぐ組み合わせテストを置く場所を 1 つに決めるため、e4 を最後に置く。ループは実装 PR を同時に 1 本しか
開かないため、直列化による待ちは生じない。

**呼び出しを含む式（全 sub-slice 共通の定義）**: 部分式のどこかに `Expr::Call` を含む式をいう（静的に判定する）。
`Expr::Lambda` の本体は定義時に評価されないので数えない。式の中で反復を起こすのは呼び出し（`map` / `filter` /
`each` を含む）だけなので、「呼び出し・反復を含む部分式」はこの定義と一致する。callee が実行時にユーザー関数・
builtin・host のどれに解決されるかは静的には決まらない（user binding が builtin より優先する）ため、判定は
「`Expr::Call` を含むか」で保守的に行い、実行時に callee を解決してから各 sub-slice の経路へ振り分ける。

**全 sub-slice 共通の完了条件**:

- `cargo fmt --check` / `cargo clippy --all-targets --all-features -- -D warnings` / `cargo test --all-features`
  が緑。既存テストは無改変（TEC-AT-03 のその sub-slice 分）。
- 同期経路（`can_yield = false` の `Evaluator::run` / `run_phased` / `run_repl_submission`）のコードパスを変えない。
  式 frame を積まず、fuel・heap（`live_heap` / `peak_heap`）・エラー行・trace・stdout・exit code が現状と一致する。
  terminal まで poll する入口（`Engine::run`〔CLI を含む〕・alpha `execute` / `execute_repl_submission`・`run_for_repl`）は
  コードパスではなく観測値を変えない（e1 判断 Q1 = A。e1 小節「判断の記録」）。
- その sub-slice で driver 上へ移した呼び出し位置について、強制 yield フックで全境界を yield させた poll 経路の
  結果が同期経路と一致することを `tests/engine_api.rs` で固定する（TEC-AT-01 のその sub-slice 分）。
- documentation-guide の Definition of Done に従い、done にする実装 PR で本 spec の逸脱一覧を更新する
  （`execution-control.md` は逸脱一覧をまだ持たないため、最初に done を出す sub-slice（e1）の実装 PR で
  逸脱一覧を新設する＝documentation-guide「逸脱一覧の持ち方」の移行注記）。各 sub-slice の「範囲外」に
  書いた過渡的な未実装範囲は、その sub-slice が done になった時点の逸脱一覧に行として載せ、後続 sub-slice
  が解消したときに削除する。

##### tree-expr-continuation-e1 実装スライス（文レベルの関数呼び出しを driver 上で実行・関数本体の yield・強制 yield フック）

- **範囲**:
  - **文レベルのユーザー関数呼び出しを driver 上で実行する（poll 経路だけ）**。対象は、`can_yield = true` の
    driver が実行する文のうち、次の 4 つの値の位置にある式が **それ自体 `Expr::Call` で、callee と全引数が
    呼び出しを含まない**ものに限る。
    - 式文（`Stmt::ExprStmt`）、`let`（`Stmt::Let`）の右辺、再代入（`Stmt::Assign`）の右辺、`return` の値
  - その呼び出しの callee が実行時にユーザー関数（`Value::Fn`）へ解決されたときだけ、`eval_call` と
    **同じ順序**（builtin / cooperative / host の fallback 判定 → `count_step` → 深度検査 → callee 評価 →
    callable 検査 → arity 検査 → 引数評価 → `push_call_frame` → captured / self-binding / 引数の束縛 →
    call trace の push）で前処理を行い、`drive_call_body` の Rust 再帰に入る代わりに Call frame を
    `StmtStep::Enter` で driver へ渡す。builtin / cooperative / host に解決された場合や、callee・引数が
    呼び出しを含む場合は従来の `eval_expr` 経路（その内側は `can_yield = false`）のまま。
  - Call frame（`FrameKind::Call`）に **「この活性が driver 上で実行されている（yield 可能）か」** と
    **戻り値の受け渡し先**（捨てる / `let` で束縛する名前 / 再代入する名前 / 呼び出し元の `return`）を持たせる。
    Call frame が pop するとき（本体完了・`return`）に値を受け渡し先へ届け、`let` / 再代入は同期経路と同じ
    `env_set` / `env.update` とエラー写像を使う。`return` の受け渡し先は呼び出し元の活性で
    `FlowSignal::Return` として unwind する。
  - 終了経路を同期経路（`eval_call` → `drive_call_body` の再入）と一致させる。callee 内で未捕捉のエラーは
    呼び出し元側の最も近い `try` が捕捉し、`break` / `continue` が Call 境界に達したら同期経路と同じ行・同じ
    trace で「ループ外」エラーになる。エラー時の call trace も同期経路と同じ内容にする（Call frame を畳む
    前後のどちらで snapshot するかを含めて揃える）。
  - **関数本体の文/反復境界での slice yield**: driver 上で実行される関数本体（入れ子の文レベル呼び出しを
    含む）の文/反復境界で、トップレベルと同じ slice fuel 判定を行う。resume は `run_slice` がトップレベル
    driver から永続 frame stack を回し直すだけで、関数本体の途中から続く。
  - cooperative host-call の `may_yield` と audit drain queue 上限の判定は、トップレベル（Call frame の外）
    だけのまま（上の「can_yield の伝播」）。`can_yield_ctx` を「slice fuel を確認してよいか」と
    「トップレベルか」に分けて持つ。
  - **テスト専用の強制 yield フック**（上の小節の定義どおり、`unstable-bytecode` または `cfg(test)` 限定）を
    追加し、e1 時点の全 yield 可能点（トップレベルと driver 上の関数本体の文/反復境界）で必ず yield させる。
    e2〜e4 は同じフックを自分の新境界へ広げる。
  - 深度上限 `MAX_USER_CALL_DEPTH = 128` の検査は driver 上の呼び出しでも同期経路と同じ位置・同じエラーにする。
- **範囲外（後続 sub-slice）**: 式の途中にある呼び出し・callee や引数が呼び出しを含む呼び出し・`if` / `while`
  の条件・`for` の反復元・index 代入の index と値に現れる呼び出し（e2）。builtin / host / cooperative の
  引数に現れる呼び出し（e3）。`map` / `filter` / `each` の callback 本体と要素境界（e4）。これらの内側は
  e1 時点では従来どおり `can_yield = false` で完了まで実行し、§4.2 の超過として扱う。
- **heap**: Call frame の `heap_charge` は従来どおり 0。e1 は式 frame を作らないため、poll 経路の
  continuation heap も増えない。
- **受入基準（e1 が満たす部分）**:
  - **TEC-AT-01（e1 分）**: 強制 yield フックで全境界を yield させ、関数本体の文/反復境界を跨ぐ script
    （4 つの値の位置それぞれ・入れ子の文レベル呼び出し・関数本体のループ・再帰）が、同期経路
    `Engine::run` と stdout・`committed.fuel`・terminal 種別・非 `Yielded`/`Resumed` の audit event 列で
    一致する。終了経路（callee 内エラーを呼び出し元の `try` が捕捉する・callee 内エラーが未捕捉で terminal に
    なる・関数内の `break`/`continue` がループ外エラーになる）についても、エラー種別・行・trace が一致する。
  - **TEC-AT-03（e1 分）**: 既存テスト無改変で緑（`may_yield_false_requires_ready_else_internal_terminal` と
    B2-AT-09 を含む）。同期経路の観測値不変。
  - **TEC-AT-04（e1 分）**: poll 経路で `return f(n - 1)` 形の深い再帰が、同期経路と同じ
    `call_depth_limit`（同じ行）で止まる。既存 `tests/defensive*.rs` は無改変で緑。
  - **TEC-AT-05（e1 分）**: 関数本体の途中での yield を跨いでも submission journal が保持され、terminal で
    だけ commit / rollback される（REPL transaction の成功・失敗の両方）。
  - **TEC-AT-06（e1 分）**: 文レベルで呼んだ関数の本体にループを持つ script を `PollSlice::max_fuel = 16`
    で実行したとき、1 回の poll で実行される fuel が「slice + 区間内の callback なし bulk builtin 1 回分の
    単独実測 fuel」以内（式起因の超過は現行課金で 0）。`Yielded`/`Resumed` が対で順序が単調。
- **検証**: 全 sub-slice 共通の完了条件。テストは `tests/engine_api.rs`（TEC-AT-01/05/06）、
  `tests/defensive*.rs` への追加または既存の無改変緑（TEC-AT-04）、Call frame の受け渡し先と終了経路の
  unit test は `src/eval.rs`。
- **実装（e1 実装 PR の時点の記録）**: `src/eval.rs` の `exec_stmt` が、yield 可能な driver
  （`can_yield_ctx`）で 4 つの値の位置の式が上記の文レベル呼び出しのときだけ `exec_driver_call_stmt` を通す。
  前処理は `eval_call` から分離した `prepare_call` を同期経路と共有し（同じ順序）、ユーザー関数なら
  `FrameKind::Call { driver: Some(DriverCall { dest, .. }) }` を `StmtStep::Enter` で積む。`DriverCall` は
  受け渡し先（`CallDest`：捨てる / `let` / 再代入 / `return`）と関数値（同期経路のローカル `func_value` と
  同じ寿命で live heap を揃える）を持つ。本体完了・`return` は `finish_driver_call` が Call frame を畳んでから
  受け渡し先へ届け、`break` / `continue` は Call frame を畳んでから「ループ外」エラーを呼び出し元の文のエラー
  として捕捉処理する（`drive_flow` / `recover_error`）。callee 内のエラーは `handle_error` が driver 上の
  Call frame を越えて呼び出し元の `try` を探す。`may_yield` と audit drain queue 上限は `in_driver_call()`
  が偽のときだけ判定する。強制 yield フックは `Evaluator::force_yield_every_boundary`（テスト専用 surface は
  `EmbeddingContext::force_yield_at_every_boundary_for_test`、`unstable-bytecode` 限定・`#[doc(hidden)]`）で、
  slice で 1 文・1 反復以上進めた後の全境界で yield する。`abort_session` は yield 中の driver 上の呼び出しの
  呼び出し元スコープと call trace を rollback より前に戻す。同期経路（`Evaluator::run` / `run_phased` /
  `run_repl_submission`、`can_yield = false`）は従来の分岐だけを通り、再帰 1 段あたりの Rust stack 使用量も
  増やさない（新しい処理は `#[inline(never)]` の別関数に置く）。
  - 注記: 本小節と共通の完了条件は「同期経路」を `Engine::run` / `execute` / REPL と書くが、実装では
    埋め込み `Engine::run`（CLI を含む）と alpha `Engine::execute` / `execute_repl_submission` は
    `begin_execution` / `run_slice`（`can_yield = true`）を通る。したがって e1 の driver 上の呼び出しはこれらの
    入口でも有効になる（slice 上限が大きいので通常は yield しない）。観測値は既存テストの無改変緑に加え、
    TEC-AT-01 のテストで stdout・fuel・エラー行・trace を `can_yield = false` の `Evaluator::run` と、
    audit event 列と terminal（usage を含む）を強制 yield なしの実行と、`src/eval.rs` の unit test で
    `live_heap` / `peak_heap` を `Evaluator::run` と比べて一致を確かめた。
- **判断の記録（e1 実装 PR #254 のレビューで判断待ちにしたもの。kamonabe が 2026-10-24 に選択）**:
  - **「同期経路」の扱い（Q1 = A）**: 実装はこのまま受け入れる。tree-expr-continuation の各小節と
    sub-slice 共通の完了条件でいう「同期経路」は、`can_yield = false` の `Evaluator::run` 系
    （`Evaluator::run` / `run_phased` / `run_repl_submission`）と読み替える。`Engine::run`（CLI を含む）・
    alpha `execute` / `execute_repl_submission`・`run_for_repl` のように terminal まで poll する入口は、
    コードパスを変えないことではなく**観測値を変えないこと**を条件にする。
    理由: §9.2 で `Engine::run` は同じ handle を terminal まで poll する入口と定めており、入口ごとに
    実行経路を分けると production に同じ文を実行する経路が 2 つでき（`audit_slice_fuel` を使う
    `Engine::run` は結局 driver 経路を通るため、分けても文言どおりにはならない）、e2〜e4 でさらに広がる。
    観測値の一致は比較テストで固定しており、マニフェストの予測可能性を保てる。
    共通の完了条件と TEC-AT-01/03 の文言（「同期経路 `Engine::run`」など）は、e2 の前の PR で直す。
  - **slice fuel の既存逸脱と TEC-AT-06（Q2 = A）**: e1 はこのまま受け入れる。slice の使用量が
    `count_step` だけを数え、builtin の per-element fuel を数えない挙動（Slice 3 PR-d から）は冒頭の
    逸脱一覧の行で記録し、解消は後続に回す（`fuel-bulk-chunk-yield` の設計範囲に含めるか、
    `needs-design` issue を立てる）。TEC-AT-06（e1 分）は bulk builtin を 1 回だけ呼ぶ script で確かめる。
    理由: 直すと yield の位置（poll の粒度）が変わり既存の slice 系テストに響きうるため、e1 の範囲
    （課金と観測を変えない）とスライスの大きさの目安（仕組みは 1 つまで、[開発ループ手順書](loop-runbook.md)
    「1 回分の上限」）を越える。現状は逸脱一覧で正しく記録する（[ドキュメント運用ガイド](documentation-guide.md)
    「逸脱一覧の持ち方」）。

##### tree-expr-continuation-e2 実装スライス（式 frame：ユーザー関数呼び出しを含む部分式・call 引数境界）

> **e2a / e2b への分割（e2 の設計 PR）**: e2 は 1 PR に収まらない（e1 の実装 PR #254 で `src/` の差分がテストを除いて
> +478 / -76 行だった〔`src/eval.rs` 434・`src/ast.rs` 29・`src/embedding.rs` 15。`mod tests` は数えない〕。e2 はその機構の上に式 frame・7 つの式種・4 つの文の式スロットを足すため、
> [開発ループ手順書](loop-runbook.md)「スライスの大きさの目安」の 400 行を大きく超える。受入基準も 7 項目あり、5 を超える）。そこで下の
> **e2a（式 frame の導入と、e1 の 4 位置での call・二項・単項）** と **e2b（index・list / dict リテラル・
> f-string と、文の式スロット）** の 2 つの実装スライスへ割った。本小節は e2 全体の設計の記録であり、
> 実装範囲・受入基準・完了条件は e2a / e2b の小節を正とする。「e2 が done」は e2a と e2b がともに done を
> 意味する。式 frame の heap は、e2a の判断 Q1 = A で課金なし（0）に確定した（本小節の元の記述は 96 byte 固定だった）。

- **範囲**:
  - `FrameKind::Expr` を追加する。保存するのは上の小節「continuation へ保存する式の評価状態」のうち
    operand stack・式 cursor・callee 引数の部分評価結果・保留中の演算（callback 反復状態は e4）。
  - poll 経路で、呼び出しを含む式を式 frame で段階評価する。対象の式種は二項演算（`and` / `or` の短絡を含む）・
    単項演算・index・list / dict リテラル・f-string・call（callee と引数）。呼び出しを含まない operand は
    従来どおり `eval_expr` の Rust 再帰で 1 つの値として評価してよい（境界規則）。
  - 式 frame の operand として現れた call の callee がユーザー関数に解決されたら、e1 と同じ前処理順序で
    Call frame を driver 上に積み、戻り値の受け渡し先を「親の式 frame の operand stack」とする。builtin /
    host / cooperative に解決され、かつ引数が呼び出しを含む場合は、e2 時点ではその call 全体を従来の
    `eval_expr` 経路で評価する（e3 の範囲）。
  - 文の式スロットを式 frame の対象に広げる: e1 の 4 位置（値が呼び出しを含む任意の式になる）に加え、
    `if` / `while` の条件（`while` は反復ごとの再評価を含む）、`for` の反復元、index 代入の index と値。
    評価順序（index 代入は binding 解決 → index → value の規範順序）とエラー行は同期経路と一致させる。
  - yield 可能点: call の各引数評価の境界（引数 i の評価後・引数 i+1 の評価前、全引数評価後・本体へ入る前）と、
    呼び出しを含む operand の境界。強制 yield フックをこれらへ広げる。
  - heap: 式 frame は課金しない（`heap_charge` 0。operand は二重課金しない。e2a の判断 Q1 = A）。
  - 式 frame の途中で起きたエラーの捕捉（最も近い `try`）、`break` / `continue` / `return` による unwind で
    式 frame を正しく畳み、heap を release する。
- **範囲外（後続 sub-slice）**: builtin / host / cooperative の呼び出しで引数が呼び出しを含む場合（e3）。
  `map` / `filter` / `each` の callback 本体と要素境界（e4）。
- **受入基準（e2 が満たす部分）**:
  - **TEC-AT-01（e2 分）**: 強制 yield フックで、call 引数境界・呼び出しを含む operand 境界・文の式スロット
    （条件・反復元・index 代入）を跨ぐ script が同期経路と一致する（検査項目は TEC-AT-01 本文どおり）。
  - **TEC-AT-02（e2 分）**: 深い式・入れ子 call・引数評価の途中・短絡評価の途中で yield させ、resume 後の
    最終値と副作用列が同期経路と一致する。operand stack・式 cursor・部分評価済み引数が保存されることを
    unit test で固定する。
  - **TEC-AT-03（e2 分）**: 既存テスト無改変で緑。同期経路の `live_heap` / `peak_heap` が不変
    （同期経路は式 frame を積まず、poll 経路の式 frame も課金しない）。
  - **TEC-AT-04（e2 分）**: poll 経路で `f(n - 1) + 1` 形（式の途中の再帰）が同期経路と同じ
    `call_depth_limit` で止まり、深い呼び出しを含まない式は従来どおり有限停止する。
  - **TEC-AT-05（e2 分）**: 式の途中の yield を跨いでも submission journal が保持され、terminal でだけ
    commit / rollback される。
  - **TEC-AT-06（e2 分）**: 式の中で呼んだ関数の本体にループを持つ script を最小 slice で実行し、1 poll の
    実行量が e1 と同じ上限以内。
  - 式 frame の heap: 式 frame は課金されず、push・pop・unwind・エラー捕捉のどこでも `live_heap` が動かない。
    poll 経路の `live_heap` / `peak_heap` が同じ script の同期経路と一致する（unit test）。
- **検証**: 全 sub-slice 共通の完了条件。テストは `tests/engine_api.rs`（TEC-AT-01/02/05/06）、
  `tests/defensive*.rs`（TEC-AT-04）、式 frame の不変条件と heap は `src/eval.rs` の unit test。

###### tree-expr-continuation-e2a 実装スライス（式 frame の導入：e1 の 4 位置での call・二項・単項）

- **範囲**:
  - **式 frame（`FrameKind::Expr`）を追加する**。段階評価中の式ノード 1 つにつき式 frame を 1 個積む。保存するのは
    上の小節「continuation へ保存する式の評価状態」のうち、評価中のノード・式 cursor（次に評価する operand）・
    評価済みの operand（operand stack）・callee 引数の部分評価結果・保留中の演算（callback 反復状態は e4）。
    - Frame は AST を借用しない（PR-d の `'static` 規律）。式ノードは `LoopSite` と同じく **所有する Block と文の
      index**、文の中の式スロット、そこから評価中のノードまでの **子の位置の経路** で辿る。AST の clone・unsafe な
      参照は使わない。
  - **対象の位置**: poll 経路（`can_yield_ctx`）で、e1 の 4 位置（式文・`let` の右辺・再代入の右辺・`return` の値）の
    式が呼び出しを含むとき。ただし e1 の形（式がそれ自体 `Expr::Call` で、callee と全引数が呼び出しを含まない）は
    e1 の経路のままにし、式 frame を作らない。
  - **段階評価する式種（e2a）**: call・二項演算（`and` / `or` の短絡を含む）・単項演算。式 frame の operand のうち
    次のものは従来どおり `eval_expr`（Rust 再帰、その内側は `can_yield = false`）で 1 つの値として評価する。
    - 呼び出しを含まない operand（境界規則）。
    - e2a で段階評価しない式種（index・list / dict リテラル・f-string。e2b）のノード。呼び出しを含んでいても
      まるごと 1 つの operand として評価する。
    - callee が識別子で、その時点で user binding を持たない call（builtin / cooperative host / 登録 host function、
      または未定義名）。call 全体を従来の `eval_call` 経路で評価する（builtin / host / cooperative は `CallPrep::Done`、
      未定義名は `count_step` → callee の評価で未定義名エラー。引数の段階評価は e3）。
  - **call の段階評価**: 同期経路の `prepare_call` と同じ順序で進める。`count_step` → 深度検査 → callee 評価
    （呼び出しを含むなら段階評価）→ callable 検査 → arity 検査（AST の引数の個数で行い、引数評価より前）→ 引数を
    左から 1 つずつ評価（呼び出しを含むものは段階評価）→ `push_call_frame` → captured / self-binding / 引数の束縛 →
    call trace の push → e1 と同じく `FrameKind::Call { driver: Some(DriverCall { .. }) }` を積む。前処理の各段は
    同期経路と共有する関数に分け、同期経路の順序とエラーを変えない。戻り値の受け渡し先 `CallDest` に「親の式 frame の
    operand stack」を加える。
  - **二項・単項の段階評価**: operand を左から評価し、揃ったら同期経路と同じ関数（`eval_binop` + `track_result`、
    `eval_unary`）で適用する。`and` / `or` は左辺の真偽で右辺を評価しない（同期経路と同じ値を返す）。
  - **式 frame の完了**: 根の式 frame が値を出したら、e1 の `deliver_call_result` と同じ処理で文の受け渡し先
    （捨てる / `let` / 再代入 / `return`）へ届ける。エラー写像と行は同期経路と同じ。
  - **yield 可能点**: 式 frame が最上位にある driver ループの境界（operand i の評価後・operand i+1 の評価前、call の
    全引数評価後・本体へ入る前）。ここでは **slice fuel の判定と強制 yield フックだけ** を行い、次の 2 つは従来どおり
    文/反復境界だけで行う（式の operand 境界へは広げない。上の小節「逸脱」の cancel / deadline と同じ扱い）。
    - audit drain queue 上限の yield（B-2。`audit_queue_progress` の意味も変えない）。
    - cancel / deadline の checkpoint。
  - **強制 yield フック**: e1 のフックを上の境界へ広げる。resume 直後に何も進めず再 yield しないよう、前進の条件に
    「この slice で operand を 1 つ以上評価した」を加える。前進は audit drain queue の前進フラグとは別に持つ。
  - **終了経路**: 式 frame の途中のエラーは、`attach_trace` の後に `handle_error` が式 frame を畳んで（heap があれば
    release して）最も近い `try` へ渡す。`return` / `break` / `continue` による unwind、`abort_session` でも式 frame を
    畳む。エラー行・trace は同期経路と同じにする。
  - **cooperative host-call の Pending**: トップレベルの文の式 frame の中で cooperative host function が Pending を
    返した場合（`may_yield` は e1 と同じく driver 上の Call frame の外で真）、その文の式 frame をすべて畳み、文の
    cursor を戻して yield する。resume では文全体を最初から評価し直す（現行 §4.7「同じ文の再評価」と同じ観測。
    式の途中状態からは再開しない）。関数本体の中は e1 と同じく `may_yield = false`。
  - **heap**: 式 frame の `heap_charge` は Call frame・`while` / `for` の制御 frame と同じく **0** とする（下記の判断 Q1 = A）。
    operand は課金済みの `Value` を参照するだけで二重課金しない。
- **範囲外（後続）**: index・list / dict リテラル・f-string の段階評価と、`if` / `while` の条件・`for` の反復元・index
  代入の index と値（e2b）。builtin / host / cooperative の引数に現れる呼び出し（e3）。`map` / `filter` / `each`（e4）。
  これらの内側は e2a 時点では `can_yield = false` で完了まで実行し、§4.2 の超過として扱う。
- **想定の変更規模**: `src/eval.rs` がおおむね 400 行（式 frame の型と経路の辿り方 50、call の段階評価と
  `prepare_call` の段分け 110、二項・単項 50、4 位置からの入口と受け渡し先 80、終了経路・Pending・`abort_session`
  70、境界の判定と強制 yield フック 40）。`src/ast.rs` は経路で子を辿る補助が要れば 30 行以内。触る仕組みは tree
  evaluator の driver 1 つ。受入基準は下の 5 項目。実装中に 400 行を大きく超えそうなら、短絡（`and` / `or`）と単項の
  段階評価を e2b へ回してよい（そのときは実装 PR を止めて、本小節と e2b 小節を直す設計 PR を先に出す）。
- **受入基準（e2a が満たす部分）**:
  - **TEC-AT-01（e2a 分）**: 強制 yield フックで全境界を yield させ、4 位置それぞれで call の引数・callee・二項・単項・
    短絡に呼び出しを含む script（例: `let y = f(x) + g(x)`、`return h(f(x), 1)`、`f(x)(y)`、`not f(x)`、
    `f(x) and g(x)`、e2a の式種の中に index などの e2b の式種が入る組み合わせ）が、同期経路（`Evaluator::run`）と
    stdout・`committed.fuel`・terminal 種別・非 `Yielded`/`Resumed` の audit event 列で一致する。callee 内エラーの
    `try` 捕捉・未捕捉 terminal・callable / arity エラー・未定義名についても、エラー種別・行・trace が一致する。
  - **TEC-AT-02（e2a 分）**: 深い式・入れ子 call・引数評価の途中・短絡評価の途中で yield させ、resume 後の最終値と
    副作用列が同期経路と一致する。operand stack・式 cursor・部分評価済み引数が yield を跨いで保存されることを
    unit test で固定する。
  - **TEC-AT-03（e2a 分）**: 既存テスト無改変で緑（`may_yield_false_requires_ready_else_internal_terminal` と B2-AT-09、
    cooperative host-call の Pending を含む既存テストを含む）。同期経路と、terminal まで poll する入口の観測値が不変。
    poll 経路の `live_heap` / `peak_heap` が同じ script の `Evaluator::run` と一致する（unit test）。
  - **TEC-AT-04（e2a 分）**: poll 経路で `f(n - 1) + 1` 形（式の途中の再帰）が同期経路と同じ `call_depth_limit`
    （同じ行）で止まる。既存 `tests/defensive*.rs` は無改変で緑。
  - **TEC-AT-05（e2a 分）**: 式の途中の yield を跨いでも submission journal が保持され、terminal でだけ commit /
    rollback される（REPL transaction の成功・失敗の両方）。
- **検証**: 全 sub-slice 共通の完了条件。テストは `tests/engine_api.rs`（TEC-AT-01/02/05）、`tests/defensive*.rs` への
  追加または既存の無改変緑（TEC-AT-04）、式 frame の保存状態・終了経路・heap の unit test は `src/eval.rs`。
- **判断の記録（返信なしのため推奨案で確定）**:
  - **Q1. 式 frame の heap 課金: A（式 frame の `heap_charge` を 0 にする）で確定。** kamonabe の返信が 2 時間無かったため、
    [開発ループ手順書](loop-runbook.md) §7.5 により推奨案で確定した（[Slack スレッド](https://kamo-dev.slack.com/archives/C0CB1H2HHJB/p1792869955313449)）。
    上の小節「式 frame の heap 課金」は「同期経路は式 frame を積まないので `live_heap` / `peak_heap` は変わらない」を前提に
    96 byte 固定を課金すると決めていた。ところが e1 判断 Q1 = A により `Engine::run`（CLI を含む）と alpha `execute` は
    driver 経路を通り、それらは観測値を変えないことが条件になった。96 byte を課金すると、これらの入口の `peak_heap` と
    heap 上限で止まる位置が変わる。そこで式 frame を課金しないことにした（Call frame・`while` / `for` の制御 frame と同じ）。
    - **理由**: (i) e1 判断 Q1 = A（terminal まで poll する入口は観測値を変えない）。(ii) manifesto 原則 5「同じ言語
      version、スクリプト、入力…から、同じ結果…が得られる」: 課金すると同じ script が `Evaluator::run` と `Engine::run` で
      heap 上限に達して止まる位置が変わる。(iii) 原則 5「意味論、評価順、エラー、資源消費が実行backendごとに変わる
      ことを許容しない」と上の小節「VM との heap 差も広げない」: 課金すると tree だけが式 frame を課金し、VM（value stack は
      未課金）との heap 差が広がる。(iv) §5.2（共有は 1 回課金）。式 frame は同期経路では Rust 再帰（未課金）が持つ評価状態を
      heap へ移しただけで、新しい値は作らない。operand は課金済みの `Value` を参照するだけ。
    - **原則 3（すべての実行に予算を設ける）との比較**: 式 frame の個数は `MAX_AST_DEPTH`（256）× 呼び出し深度 128 で
      有界（最大で約 3.3 万個）。同期経路では同じ深さの評価状態を Rust stack が持ち、これも課金していない。式 frame は
      その状態を heap へ移しただけなので、課金しなくても「予算の外で増えるメモリ」は同期経路から増えない
      （Rust stack 側の上限は 8 MiB スレッドと深度上限が引き続き抑える）。
    - **§5.1 との関係**: §5.1 の表は continuation frame を種類で区別せず「96 + local slot 32 × count」としていたが、
      実装はすでにスコープを作る frame だけに「96 + local slot 32 × count」を課金し（`while` の反復・`if` の分岐・`try`
      本体は 96、`for` の反復・`catch` 本体は 128）、program 直下の Block・Call・`while` / `for` の制御 frame は 0 である。
      §5.1 の表に「課金するのはスコープを作る frame（96 + local slot 32 × count）。Call・ループ制御・式 frame は課金しない」と
      注記して目標仕様を実装に合わせた（逸脱一覧には足さない）。
    - **反映した箇所**: §5.1 の表の注記、上の小節（式 frame の heap 課金・poll 経路の heap 観測・continuation heap 課金・
      TEC-AT-03）、e2 小節の範囲と受入基準、e2a の heap、e4 の反復 frame の heap。
    - **採らなかった案 B**（確定どおり 96 byte を課金する）: 根拠は §5.1 の課金表を文字どおり適用することと manifesto 原則 3。
      ただし e1 判断 Q1 = A と食い違い、TEC-AT-03 と共通の完了条件に例外を足して既存の heap 系テストの期待値を変える
      必要があり、VM との heap 差も広がるため採らなかった。

- **実装（e2a 実装 PR の時点の記録）**: `src/eval.rs` に `FrameKind::Expr(Box<ExprFrame>)` を足した。`ExprFrame` は
  所有する Block・文の index・式スロットから評価中のノードまでの子の位置の経路（`ast::expr_child` の index 列）・文の行・
  operand stack・call の前処理の段（`started` / `checked`）・受け渡し先（`CallDest`）を持ち、AST を借用しない。
  `exec_driver_call_stmt` は 4 位置の式が e1 の形でなく `needs_expr_frame`（呼び出しを含む call〔callee が user binding を
  持たない識別子のものを除く〕・二項・単項）なら根の式 frame を `StmtStep::Enter` で積む。driver ループは最上位が式 frame の
  とき、slice fuel と強制 yield フックの判定だけをしてから `drive_expr_step` → `step_expr` で 1 段（operand を 1 つ評価する
  か、子の式 frame を積むか、演算を適用して値を届ける）進め、audit drain queue 上限と cancel / deadline の checkpoint は
  文/反復境界だけで行う。call の前処理は `prepare_call` を `begin_user_call`（`count_step` → 深度検査）・
  `check_user_callee`（callable → arity）・`bind_user_call`（`push_call_frame` → 束縛 → call trace）に分けて同期経路と
  共有し、本体は `CallDest::ExprResult` を受け渡し先にした driver 上の Call frame で実行する。子の式 frame の値は
  `CallDest::Operand` で親の operand stack へ積む。強制 yield フックの前進条件に `expr_progress`（この slice で式 frame を
  1 段以上進めた）を足した。cooperative host-call の Pending は、その文の式 frame をすべて畳んで文の cursor を戻して
  yield する。式 frame は Call frame と同じく `abort_session` の巻き戻しで畳まれる（特別な処理は要らない）。

###### tree-expr-continuation-e2b 実装スライス（index・list / dict・f-string と、文の式スロット）

> **設計の更新（e2b を `ready` にした設計 PR）**: 範囲・範囲外・受入基準 ID は e2 分割の設計 PR #262 のまま変えていない。
> e2a の実装 PR #268 で式 frame の形（`ExprFrame` が文の式スロットから子の位置の経路でノードを辿る・1 段ずつ進める・
> `CallDest::Operand` / `ExprResult`）が決まったので、それに合わせて「保存する状態」「式スロットの指し方と入口」
> 「受け渡し先」「受入基準の確かめ方」を書き足し、「想定の変更規模」を e2a の実績で見直した。

- **範囲**:
  - **段階評価する式種を広げる**: e2a の式 frame に index・list リテラル・dict リテラル・f-string を加える（e2a の 4 位置を
    含む、式 frame で評価するすべての位置に効く）。対象は呼び出しを含むノードだけで、呼び出しを含まない子は従来どおり
    `eval_expr` で 1 つの値にする（境界規則）。評価順・課金・エラーの位置は同期経路の `eval_expr` と同じにする。
    - index: object → index の順に評価し、同期経路と同じ `eval_index` で適用する。AUD-041 の参照読み
      （`識別子[副作用のない式]`）は呼び出しを含まないので式 frame を作らない（従来どおり）。
    - list: 要素を左から評価し、要素ごとに `check_collection` を行ってから `Value::new_list` で作る。
    - dict: key → key の型検査 → value → `check_collection`（新しい key のとき）の順を entry ごとに行い、
      `Value::new_dict` で作る。重複 key は同期経路と同じく後の値で上書きする。
    - f-string: 式の部分を評価するたびに `render_display` で描画して結果へ足し（fuel と per-item byte 上限の課金は
      その時点）、リテラル部分はそのまま足し、最後に `track_result` する。
  - **式 frame に保存する状態の追加**（上の小節「continuation へ保存する式の評価状態」の具体化）: e2a の operand stack・
    式 cursor に加え、次を式 frame に持たせる。どれも同期経路では `eval_expr` / `exec_stmt` のローカル変数が持っていたもので、
    新しい値は作らない（heap は e2a と同じく課金しない）。
    - dict: 作りかけの map（key の型検査と `check_collection` を済ませた entry）と、評価済みで value を待つ key。
    - f-string: 描画済みの部分を連結した作りかけの文字列。
    - index 代入: 先に解決した代入先 binding の cell（同期経路の `exec_stmt` がローカルに持つ `cell` と同じもの）と、
      評価済みの index の値。
  - **文の式スロットを式 frame の対象に広げる**: e2a の 4 位置に加え、`if` の条件、`while` の条件（反復ごとの再評価を
    含む。反復完了後の `count_step` は従来どおり条件の評価より前）、`for` の反復元、index 代入の index と値。
    - **式スロットの指し方**: 式 frame の根は「所有する Block・文の index・その文のどの式スロットか」で指す（e2a の
      `stmt_value_expr` が返す値の位置に、条件・反復元・index 代入の index / value を加える）。経路で子を辿る規則は e2a と同じ。
    - **入口**: 新しい式スロットは、式が `needs_expr_frame`（e2b の式種を加えたもの）に当たるとき根の式 frame で評価する。
      e1 の形（それ自体 `Expr::Call` で callee と全引数が呼び出しを含まない）の式も、新しい式スロットでは e1 の直接の経路
      （`exec_driver_call_stmt` から Call frame へ入る経路）を使わず、`needs_expr_frame` に従う。すなわち callee が user binding を
      持つときは根の式 frame で評価し、持たないとき（builtin / 登録 host function / cooperative host function / 未定義名）は
      従来どおり `eval_expr` で評価する（e1 の直接の経路は e1 の 4 位置だけに残す。式 frame の call も同じ前処理順序を通るので
      観測は変わらない）。
    - **受け渡し先**: 根の式 frame の値は、同期経路の `exec_stmt` / `advance_frame` がその値に対して行っていた処理へ届ける。
      `if`: 真偽で分岐を選び `enter_scoped_block` の frame を積む。`while`（最初の評価）: 偽なら完了、真なら `enter_while` の
      frame を積む。`while`（反復ごとの再評価）: `advance_frame` と同じく真なら `pending_step` を立てて次反復の本体を積み、
      偽なら While frame を畳む。`for`: 同期経路と同じ materialize（list / dict / 文字列ごとの `check_collection`、
      反復できない値のエラー）をして `enter_for` の frame を積む。同期経路と共有できる処理は関数に分けて共有し、
      同期経路の順序・エラーを変えない。
    - **index 代入**: binding の解決（未定義名のエラーは index / value の副作用より前）→ index → value の規範順序で評価し、
      `journal_cell` → `assign_index` は同期経路と同じ関数で行う。index と value の一方だけが呼び出しを含む場合も、
      もう一方は同じ順序の位置で `eval_expr` で評価する。
  - `while` の条件と `for` の反復元の評価のエラーは、反復境界（`advance_frame`）でのエラーと同じく最も近い `try` へ渡す。
    エラー行は同期経路と同じく文の行。
  - **cooperative host-call の Pending**: 文の式スロットの式 frame の中で Pending を返した場合は e2a と同じく式 frame を
    畳んで文の評価を最初からやり直す（index 代入は binding の解決からやり直す。cooperative host-call の Pending で文の cursor を戻し、resume でその文を
    最初から評価し直す現行の「同じ文の再評価」と同じ観測）。
    `while` の条件の再評価（反復境界）での Pending は現行の反復境界の扱い（`advance_frame` の Pending をそのまま yield し、
    resume で While frame から条件を再評価する。反復完了後の `count_step` は再評価の前に済んでいるので二重に課金しない）と
    同じ観測にする。
  - yield 可能点・強制 yield フック・heap・終了経路は e2a と同じ規則を新しい式種と式スロットへ当てる（式 frame の 1 段の
    間が yield 可能点。slice fuel の判定と強制 yield フックだけを行い、audit drain queue 上限と cancel / deadline の
    checkpoint は文/反復境界だけ）。
- **範囲外（後続）**: builtin / host / cooperative の引数に現れる呼び出し（e3）。`map` / `filter` / `each`（e4）。
- **想定の変更規模**: `src/eval.rs` がおおむね 360 行（式スロットの指し方と根の入口 40、index・list・dict・f-string の
  段階評価と保存する状態 130、`if` / `for` の受け渡し先と materialize の共有 70、`while` の最初の評価と反復ごとの再評価 70、
  index 代入 50）。`src/ast.rs` は `expr_child` を 4 つの式種へ広げる 25 行。合わせて約 385 行で、[開発ループ手順書](loop-runbook.md)
  「スライスの大きさの目安」の 800 行に収まる（e2a は見積もり 約 430 行に対し実績 458 行。同じ比率で上振れしても 420 行前後）。
  触る仕組みは tree evaluator の driver 1 つ。受入基準は下の 4 項目。
- **TEC-AT-04 / 05 を持たない理由**: 深度検査と journal の扱いは、e2a が段階評価に入れた call の前処理（`prepare_call` と
  共有する段）と driver 上の Call frame の中にあり、e2b は呼び出しの経路を新しく足さない（足すのは式種と、式 frame へ
  入る文の位置だけ）。式 frame の途中の yield が transaction 境界でないことも e2a の TEC-AT-05 で固定済みで、e2b の
  式 frame は同じ frame 種別である。したがって e2a の TEC-AT-04 / 05 がそのまま e2b の位置にも効く。
- **受入基準（e2b が満たす部分）**:
  - **TEC-AT-01（e2b 分）**: 強制 yield フックで、index・list / dict リテラル・f-string の中の呼び出しと、`if` / `while` の
    条件・`for` の反復元・index 代入の index と値に呼び出しを含む script が、同期経路と一致する（検査項目は
    TEC-AT-01 本文どおり）。dict key の型エラー・collection 上限・index エラーの行と trace も一致する。
    - 確かめ方: `tests/engine_api.rs` に、式種・式スロットごとの script（例: `let y = xs[f(i)]`、`let a = [f(1), g(2)]`、
      `let d = {k(): f(x)}`、`let s = f"{f(x)}-{g(x)}"`、`if f(x) > 0`、`while step() < 3`、`for v in f(x)`、`xs[f(i)] = g(i)`。
      呼び出しを builtin の引数に置く形は e3 なので使わない）と
      エラー経路（dict key が文字列でない・`max_collection_elements` 超過・範囲外 index）の script を置き、強制 yield フックありの
      poll 経路と `can_yield = false` の `Evaluator::run` で stdout・`committed.fuel`・terminal 種別・エラー種別・行・trace・
      非 `Yielded`/`Resumed` の audit event 列を比べる。
  - **TEC-AT-02（e2b 分）**: list / dict の要素の途中・f-string の部分の途中・`while` の条件の途中で yield させ、resume 後の
    最終値と副作用列が同期経路と一致する。
    - 確かめ方: `tests/engine_api.rs` で、要素・部分・条件の各評価が副作用（`print`）を持つ script を強制 yield フックで
      1 境界ずつ yield させ、resume 後の値と出力順が同期経路と一致することを見る。`src/eval.rs` の unit test で、dict の
      作りかけの map・f-string の作りかけの文字列・index 代入の解決済み cell と index が yield を跨いで保存されることを固定する。
  - **TEC-AT-03（e2b 分）**: 既存テスト無改変で緑。同期経路と、terminal まで poll する入口の観測値が不変。
    - 確かめ方: 既存の `tests/`・unit test を無改変で通す（`cargo test --all-features`）。加えて `src/eval.rs` の unit test で、
      e2b の式種・式スロットを含む script の poll 経路の `live_heap` / `peak_heap` が `Evaluator::run` と一致することを見る。
  - **TEC-AT-06（e2a・e2b 分）**: e2a / e2b の式種と式スロットの中で呼んだ関数の本体にループを持つ script（例:
    `let y = f(x) + 1`、`return h(f(x), 1)`、`[f(x)]`、`while f(x) > 0`、`for v in f(x)`）を
    `PollSlice::max_fuel = 16` で実行し、1 回の poll の実行量が e1 と同じ上限以内。`Yielded`/`Resumed` が対で順序が単調。
    - 確かめ方: `tests/engine_api.rs` で上の各 script を最小 slice で poll し、poll ごとの fuel 増分が「slice + 区間内の
      callback なし bulk builtin 1 回分の単独実測 fuel」以内であることと、audit の `Yielded`/`Resumed` の対と順序を見る
      （e1 の TEC-AT-06 のテストと同じ測り方）。
- **検証**: 全 sub-slice 共通の完了条件。テストは `tests/engine_api.rs`（TEC-AT-01/02/06）、式種ごとの保存状態と heap の
  unit test は `src/eval.rs`。

##### tree-expr-continuation-e3 実装スライス（builtin・host function の呼び出し引数境界）

> **設計の更新（e3 を `ready` にした設計 PR）**: 範囲と範囲外は分割の設計 PR #250 のまま変えていない。
> e2a / e2b の実装 PR #268・#280 で式 frame の形が決まった（`ExprFrame` が文の式スロットから子の位置の経路でノードを辿る、
> 1 段ずつ進める、`CallDest::Operand` / `ExprResult`）。それに合わせて次を書き足した: 対象の呼び出しの決め方、callee の解決、
> builtin ごとの引数の扱い、保存する状態、cooperative host-call の Pending と resume、受入基準の確かめ方、「想定の変更規模」。
> 受入基準に TEC-AT-06（e3 分）を加えた（下記「TEC-AT-06 を加えた理由」）。

- **範囲**:
  - **対象の呼び出し**: poll 経路（`can_yield_ctx`）で式 frame を使う位置（e2a の 4 位置・e2b の式スロットと、それらの部分式）に
    現れる call のうち、次をすべて満たすもの。e2a の `needs_expr_frame` はこれらの call を「callee が user binding を持たない
    識別子」として式 frame の対象から外していた。e3 はこれらを対象に加える。
    - callee が識別子で、その時点で user binding を持たない。
    - その名前が、同期経路の `prepare_call` と同じ優先順（builtin → cooperative host function → 登録 host function）で、
      **引数を評価する builtin**（`map` / `filter` / `each` を除く。下記「builtin ごとの引数の扱い」）・cooperative host function・
      登録 host function のどれかに解決される。
    - 引数のどれかが呼び出しを含む（「呼び出しを含む式」の定義どおり、静的に判定する）。
  - **引数を評価しない呼び出しは対象にしない**: `input` / `args` / `now`（引数 0 個）と `pop`（第 1 引数が識別子）は、引数が
    呼び出しを含むと、同期経路では引数を評価する前に arity / 代入先のエラーになる。callee が未定義名の call も、引数を評価する
    前に未定義名のエラーになる。これらは引数の境界を持たないので、式 frame を作らず従来どおり `eval_expr` で評価する
    （観測は変わらない）。`map` / `filter` / `each` も従来どおり `eval_expr` で call 全体を評価する（e4）。
  - **callee の解決は 1 回だけ**: call の式 frame の最初の段で、同期経路の `prepare_call` と同じ判定（`env.get_cell(name)` が
    無いこと、builtin 名・cooperative 登録名・host 登録名の順）で解決先を決め、式 frame に保存する。以降の段は保存した解決先に
    従い、判定し直さない（同期経路も call の開始時に 1 回だけ判定する）。callee の識別子（子 0）は値として評価しない。
  - **builtin ごとの引数の扱い**: 同期経路の `eval_builtin` / `builtin_io` / `builtin_collection` と同じ順序・同じ検査・同じ
    課金にする。評価済みの引数から結果を作る部分は、同期経路と共有する関数に分ける（同期経路の順序・エラーを変えない）。
    - **共通**: 最初の段で `validate_context_builtin_call`（arity と、`push` / `pop` の第 1 引数が識別子か）を行う。引数の評価より前。
      builtin は同期経路と同じく `count_step` を行わず、call trace も積まない。
    - **PureCore builtin**（`builtin_registry::is_pure_core_builtin`）: 引数を左から 1 つずつ評価する。揃ったら同期経路と同じく、
      filesystem host-call の request 課金（`charge_host_call_request`）→ dispatch（`to_str` / `join` / `sort` の budgeted 版、
      `dispatch_fs` と host-call triple の push、それ以外は `builtin_core::dispatch`）→ response 課金 → `track_result` を行う。
      `len(識別子)` の最適化（AUD-041）は引数が呼び出しを含まないので対象にならない（従来どおり）。
    - **`print`**: 引数を 1 つ評価するたびに `render_display` で描画し、描画済みの部分に足す（fuel と per-item byte 上限の課金は
      その時点。描画が失敗したら後の引数は評価しない。同期経路と同じ）。揃ったら `charge_output` → `resolve_print`。
    - **`env` / `exit`**: arity 検査（`env` は `check_arity_count`、`exit` は `validate_context_builtin_call`）の後に引数を評価し、
      `env` は型検査 → `resolve_env` → `track_result`、`exit` は型検査 → `resolve_exit` → `record_exit` を行う。
    - **`push`**: `validate_context_builtin_call` の後、第 2 引数の評価より前に代入先 binding の cell を解決する（未定義名のエラーは
      第 2 引数の副作用より前）。第 2 引数を評価したら、型検査 → `check_collection` → `journal_cell` → `budget_list_push` を行う。
  - **登録 host function**: 最初の段で `count_step` を行い、引数を左から評価する。揃ったら `resolve_host_call`（grant・arity 検査・
    host error の写像）→ `track_result` を行う（同期経路の `prepare_call` の host 分岐と共有する関数に分ける）。
  - **cooperative host function**: 最初の段で、resume 中の格納結果（`resumed_host_call`）があれば、`count_step` と引数の評価より前に
    それを取り出して式結果へ写す（同期経路の `eval_cooperative_call` と同じ。この call 自身の引数は評価し直さない）。無ければ
    `count_step` → 引数を左から評価 → `resolve_cooperative_call` → call_id の採番 → `start` を行う（評価済みの引数から `start` 以降を
    行う部分は同期経路と共有する関数に分ける）。`may_yield` は e1 と同じく `can_yield_ctx && !in_driver_call()`。
    - Ready はそのまま式結果へ写す（`finish_cooperative_result`）。
    - Pending は e2a と同じく、トップレベル（driver 上の Call frame の外）なら文の式 frame をすべて畳んで文の cursor を戻して yield する。
      resume では文全体を最初から評価し直す。文の他の部分の呼び出しは再び実行され、この call には上の格納結果が届く（現行の
      「同じ文の再評価」と同じ観測）。関数本体の中なら `may_yield = false` の規約違反（catch 不能 `InternalControl`）のまま。
  - **式 frame に保存する状態の追加**（上の小節「continuation へ保存する式の評価状態」の具体化）: e2a の operand stack（評価済みの
    引数）・式 cursor に加え、次を式 frame に持たせる。どれも同期経路では `prepare_call` / `eval_builtin` 系のローカル変数が持って
    いたもので、新しい値は作らない（heap は e2a と同じく課金しない）。
    - callee の解決先（builtin 名・cooperative host function・登録 host function のどれか）。
    - `print`: 描画済みの部分（同期経路のローカル `parts`）。
    - `push`: 解決済みの代入先 cell。
  - **yield 可能点**: これらの call の引数の境界（引数 i の評価後・引数 i+1 の評価前、全引数の評価後・dispatch の前）。e2a と同じく
    式 frame の 1 段の間であり、slice fuel の判定と強制 yield フックだけを行う。audit drain queue 上限と cancel / deadline の
    checkpoint は文/反復境界だけ。強制 yield フックは式 frame の段の間でそのまま効く（前進の条件も e2a の `expr_progress`）。
  - **builtin・host function の本体は 1 段で完了まで実行し、yield しない**（bulk work を含む。§4.2 の超過で、chunk 境界 yield は
    `fuel-bulk-chunk-yield`）。
  - **終了経路**: 引数の評価の途中や dispatch で起きたエラーは、e2a と同じく `handle_error` が式 frame を畳んで最も近い `try` へ渡す。
    エラー行・trace は同期経路と同じ。`exit` の `ProcessExit` と予算超過は従来どおり catch 不能 terminal として伝播する。
- **範囲外（後続 sub-slice）**: `map` / `filter` / `each`（e4）。その引数や callback の中の呼び出しは、e3 の時点では従来どおり
  `eval_expr` の中で `can_yield = false` のまま完了まで実行し、§4.2 の超過として扱う。
- **逸脱一覧の更新（e3 の実装 PR で行う）**: 「未実装範囲」の e3 の行を削除する。`map` / `filter` / `each` の引数に現れる呼び出しが
  まだ yield できないことを、e4 の行に書き足す。
- **想定の変更規模**: おおむね 650 行（[開発ループ手順書](loop-runbook.md)「スライスの大きさの目安」の 1000 行に収まる）。
  関数へ移して字下げが変わる行は、§14.5 の数え方で追加と削除の両方に数えた。
  - `src/eval.rs` 約 290 行: `needs_expr_frame` と解決先の判定 30、`ExprFrame` の追加の状態 15、`step_expr` の call の段の
    builtin / host / cooperative 分岐（最初の段の検査、引数の段階評価、`print` の描画、適用）150、cooperative の resume 格納結果と
    `start` 以降の共有 45、登録 host function の適用の共有 50。
  - `src/builtin.rs` 約 350 行: PureCore の適用（約 120 行）を共有する関数へ移す 250、`print` / `env` / `exit` / `push` の適用を
    共有する関数に分ける 100。
  - e2b は見積もり約 385 行に対し、実績は `#[cfg(test)]` を除いて 491 行だった（PR #280）。同じ比率で上振れしても約 830 行で
    1000 行に収まる。実装中に 1000 行を超えそうなら、cooperative host function を後続の sub-slice へ回してよい（そのときは実装 PR を止めて、
    本小節を割る設計 PR を先に出す）。
  - 触る仕組みは tree evaluator の call の評価 1 つ。`src/builtin.rs` は `impl Evaluator` の builtin dispatch で、builtin の挙動は
    変えず、引数の評価と適用の境目を分けるだけである。受入基準は下の 4 項目。
- **TEC-AT-04 / 05 を持たない理由**: 深度検査と journal の扱いは、e2a が段階評価に入れたユーザー関数の call の前処理と driver 上の
  Call frame の中にある。e3 は builtin・host function の call の引数を段階評価するだけで、ユーザー関数を呼ぶ経路を新しく足さない
  （引数の中のユーザー関数呼び出しは e2a の経路を通る）。builtin・host function は同期経路でも深度を数えない。`push` の
  `journal_cell` は同期経路と同じ関数で行い、式 frame の途中の yield が transaction 境界でないことは e2a の TEC-AT-05 で固定済み。
- **TEC-AT-06 を加えた理由**: e3 で引数の境界を yield 可能にする目的は、builtin・host function の引数の中で呼んだ関数の本体が
  ループを持っても 1 回の poll の実行量を有界にすること（§4.2、上の小節「yield 要件」）である。e2 で TEC-AT-06（e2a・e2b 分）を
  置いたのと同じく、e3 の位置でもこれを e3 の done の時点で固定する。
- **受入基準（e3 が満たす部分）**:
  - **TEC-AT-01（e3 分）**: 強制 yield フックで、builtin・登録 host function・cooperative host function の引数に呼び出しを含む
    script を跨いで yield/resume した結果が、同期経路と一致する（検査項目は TEC-AT-01 本文どおり。host-call triple を含む
    非 `Yielded`/`Resumed` の audit event 列を含む）。引数の検査・型・arity・未定義名・描画上限のエラーの種別・行・trace も一致する。
    - 確かめ方: `tests/engine_api.rs` に、builtin ごとの script（例: `print(f(1), g(2))`、`let n = len(f(x))`、`push(xs, f(x))`、
      `let s = to_str(f(x)) + join(g(), ",")`、`let v = env(k())`、`if len(f(x)) > 0`、`print(len(f(x)) + 1)`、filesystem builtin の
      引数に呼び出し〔未 grant でも deny の host-call triple が積まれる〕）とエラー経路（`push` の代入先が未定義名〔第 2 引数の
      副作用が出ない〕・`push` の代入先が list でない・PureCore builtin の arity 違反〔`len(f(), g())`〕・`env` の型違反）の script を
      置き、e1 以来の比べ方（強制 yield ありの poll 経路と `can_yield = false` の `Evaluator::run`、監査付きと sink 無しの
      `Engine::run`）で stdout・`committed.fuel`・terminal 種別・エラー種別・行・trace・非 `Yielded`/`Resumed` の audit event 列を
      比べる。登録 host function と cooperative host function（Ready を返す adapter）の引数に呼び出しを置く script は、同じ registry と
      grant を `Evaluator::set_host_registry` で渡した `Evaluator::run` と比べる（既存の `host_fn_id` / `coop_ctx` の helper を使う）。
  - **TEC-AT-02（e3 分）**: builtin・host function の引数の評価の途中で yield させ、評価済みの引数・`print` の描画済みの部分・`push` の
    解決済みの cell・callee の解決先が保存され、resume 後に同期経路と同じ値・副作用列になる。cooperative host function が Pending を
    返した場合は、resume で文を評価し直しても、この call の引数の呼び出しは 1 回だけ実行される。
    - 確かめ方: `tests/engine_api.rs` で、各引数の評価が副作用（`print`）を持つ script（例: `print(a(), b(), c())`、
      `push(xs, f(x))`、登録 host function の 3 引数）を強制 yield フックで 1 境界ずつ yield させ、resume 後の値と出力順が同期経路と
      一致することを見る。cooperative の Pending は `tests/scheduler_api.rs` に、`let x = fetch(one())` と `print(g(), fetch(one()))` の
      Pending → complete → resume を置き、`one()` の副作用が 1 回・`g()` の副作用が文の評価し直しの分だけ出ること（現行の観測。
      e2a の `cooperative_pending_inside_expression_frame_yields_and_resumes` と同じ規則）と、関数本体の中で同じ形が `InternalControl`
      になることを見る。`src/eval.rs` の unit test で、評価済みの引数・描画済みの部分・cell・解決先が yield を跨いで保存されることを固定する。
  - **TEC-AT-03（e3 分）**: 既存テスト無改変で緑（host function・cooperative・audit・filesystem 系のテストと
    `may_yield_false_requires_ready_else_internal_terminal`・B2-AT-09 を含む）。同期経路と、terminal まで poll する入口の観測値が不変。
    - 確かめ方: 既存の `tests/`・unit test を無改変で通す（`cargo test --all-features`）。加えて `src/eval.rs` の unit test で、e3 の
      call を含む script の poll 経路の `live_heap` / `peak_heap` が `Evaluator::run` と一致することを見る。
  - **TEC-AT-06（e3 分）**: builtin・登録 host function の引数の中で呼んだ関数の本体にループを持つ script（例: `print(f(x))`、
    `let n = len(f(x))`、`h(f(x))`）を `PollSlice::max_fuel = 16` で実行し、1 回の poll の実行量が e1 と同じ上限以内。
    `Yielded`/`Resumed` が対で順序が単調。
    - 確かめ方: `tests/engine_api.rs` で上の各 script を最小 slice で poll し、poll ごとの fuel 増分が「slice + 区間内の callback なし
      bulk builtin 1 回分の単独実測 fuel」以内であることと、audit の `Yielded`/`Resumed` の対と順序を見る（e1 の TEC-AT-06 と同じ測り方）。
- **検証**: 全 sub-slice 共通の完了条件。テストは `tests/engine_api.rs`（TEC-AT-01/02/06）、`tests/scheduler_api.rs`（cooperative の
  Pending と resume）、式 frame の保存状態と heap の unit test は `src/eval.rs`。

##### tree-expr-continuation-e4 実装スライス（`map` / `filter` / `each` の段階実行）

> **設計の更新（e4 を `ready` にした設計 PR）**: 下の「範囲」と、範囲外（`sort`・VM・callback 本体の中の host-call Pending）は
> 分割の設計 PR #250 のまま変えていない。e2a〜e3 の実装 PR #268・#280・#296 で式 frame の形が決まった（`ExprFrame` が文の
> 式スロットから子の位置の経路でノードを辿る、1 段ずつ進める、`CallDest::Operand` / `ExprResult`、callee の解決先
> `CallTarget` を最初の段で 1 回だけ決める）。それに合わせて、下の「具体化（e4 を ready にした設計 PR）」に、対象の呼び出しの
> 決め方、同期経路と同じ順序、保存する状態、yield 可能点、終了経路、受入基準の確かめ方、「想定の変更規模」を書き足した。
> e3 の設計 PR #287 が残した「`map` / `filter` / `each` 自身の引数の段階評価」もここで決めた。受入基準の ID の数は、
> 判断役が Q1=A を選んだ（PR #464）。内容と理由は下の「受入基準の ID の数」。

- **範囲**:
  - poll 経路で、`map` / `filter` / `each` の要素ループを明示 frame の反復に載せる（上の小節
    「callback builtin の段階実行化」）。frame には callback 反復状態（処理済み要素・次の要素 index・部分結果）を
    保存する。callback 本体は Call frame として driver 上で実行し（e1 の機構）、戻り値の受け渡し先を
    「この反復 frame」とする。
  - これらの呼び出しが文レベル（e1 の 4 位置）・式の operand（e2）・他の builtin / host の引数（e3）の
    どこに現れても、driver 上の段階実行にする。
  - per-element 課金・callback の arity / 非 callable エラー・エラー伝播・結果の順序は同期経路
    （`call_fn_value` の現行実装）と一致させる。`sort` は書き換えない。
  - yield 可能点: 各要素の処理の境界（callback 呼び出しの前後）と callback 本体の文/反復境界。強制 yield
    フックをここへ広げる。
  - heap: 反復 frame は式 frame と同じく課金しない（`heap_charge` 0。e2a の判断 Q1 = A）。部分結果の
    collection は同期経路と同じ課金規律で作る（同期経路と terminal 時点の `live_heap` が一致する）。
- **受入基準（e4 が満たす部分。e4 の done で TEC-AT-01〜07 がすべて満たされる）**:
  - **TEC-AT-07**: 本文どおり。要素の途中・callback 本体の途中の両方で強制 yield を起こし、`map` / `filter` /
    `each` の結果・副作用順序・`committed.fuel` が同期経路と一致する。
  - **TEC-AT-01（全体）**: 強制 yield フックで、e1〜e4 の全 yield 可能点を 1 つの script の中で跨ぐ組み合わせ
    （例: 式の operand や builtin の引数に置いた `map`、`map` の callback の中の入れ子呼び出しとループ）を
    含めて、同期経路と一致する。
  - **TEC-AT-02（e4 分）**: callback 反復状態が yield を跨いで保存されることを unit test と resume 一致で固定する。
  - **TEC-AT-05（e4 分）**: callback 途中の yield を跨いでも submission journal が保持される。
  - **TEC-AT-06（全体）**: callback 本体にループを持つ script を最小 slice で実行し、1 poll の実行量が上限
    以内（e1 と同じ上限式）。再帰・ループ・callback が何段入れ子でも上限を超えない。
  - **TEC-AT-03（e4 分）**: 既存テスト無改変で緑。
- **検証**: 全 sub-slice 共通の完了条件。テストは `tests/engine_api.rs`（TEC-AT-01/02/05/06/07）、callback
  反復状態の不変条件は `src/builtin*.rs` の unit test。

**具体化（e4 を ready にした設計 PR）**:

- **対象の呼び出し**: poll 経路（`can_yield_ctx`）で式 frame を使う位置（e2a の 4 位置・e2b の式スロット・それらの部分式・
  e3 の builtin / host function の引数）と、e1 の形の文（文の値の位置の式がそれ自体 call で、callee と全引数が呼び出しを
  含まないもの）に現れる call のうち、callee が識別子で、その時点で user binding を持たず、名前が `map` / `filter` / `each` の
  もの。e3 までは `resolve_call_target` がこの 3 つを `None` にし、`needs_expr_frame` も e1 の文の経路（`prepare_call` →
  `CallPrep::Done`）も call 全体を従来の `eval_expr` で評価していた。e4 はこの 3 つに新しい解決先（仮称 `CallTarget::Callback`）を
  与え、**引数が呼び出しを含むかに依らず**式 frame で段階評価する（call 自身が要素ごとに callback を呼ぶので、引数に呼び出しが
  無くても反復を含む）。e1 の形の文に現れた場合も、`prepare_call` へ渡さずに根の式 frame を積む。
  - `map` / `filter` / `each` に user binding があれば従来どおりユーザー関数の call（e2a の経路）で、e4 の対象ではない。
- **同期経路と同じ順序**（`builtin_collection` と `call_fn_value`、`src/builtin.rs`）。式 frame の段は次の順に進める。
  1. 最初の段: `validate_context_builtin_call`（3 つとも arity 2 の `check_arity_count`）。引数の評価より前。builtin なので
     `count_step` はせず、call trace も積まない（同期経路と同じ）。同じ段の中で第 1 引数の評価に進む（e3 と同じ。前処理と
     第 1 引数の間に yield 可能点を置かない）。
  2. 第 1 引数（List）→ 第 2 引数（callback）を左から評価する。呼び出しを含む引数は子の式 frame（`begin_expr_child`）、
     含まない引数は `eval_expr`。
  3. 第 1 引数の型検査（List でなければ `builtin_arg_type(line, <名前>, 1, "List", …)`）。callback の検査はここでは行わない
     （同期経路と同じく、空の List なら callback が呼べない値でもエラーにならない）。
  4. 要素 i ごとに: `count_step` → 深度検査（`MAX_USER_CALL_DEPTH`、`call_depth_limit`）→ callable 検査
     （`callback_not_callable`）→ arity 検査（`callback_arity`。callback は常に 1 引数）→ `push_call_frame` → captured /
     self-binding / 引数の束縛 → call trace の push → callback 本体を driver 上の Call frame（e1 の機構）で実行する。束縛までは
     e2a の `begin_user_call` / `bind_user_call` と同じ段だが、callable / arity のエラーは callback 専用の message なので、
     `call_fn_value` と共有する検査の関数に分ける（同期経路の順序とエラーを変えない）。
  5. callback の戻り値の受け渡し先は新しい `CallDest`（仮称 `CallDest::Callback`）で、最上位の式 frame（この call）へ値を届ける。
     本体が `break` / `continue` で抜けたら、同期経路と同じくその文の行で「ループ外」のエラーにする。届いた値で、
     `map` は `check_collection(len + 1)` → 部分結果へ push、`filter` は truthy なら `check_collection(len + 1)` → 要素を
     部分結果へ push、`each` は値を捨てる。
  6. 全要素の後: `map` は `check_elements_depth` → `Value::new_list`、`filter` は `Value::new_list`、`each` は `Null` で
     式 frame を完了する（`finish_expr_frame`）。結果の組み立ては `builtin_collection` と共有する関数に分ける。
- **式 frame に保存する状態の追加**（上の小節「callback builtin の反復状態」の具体化）。どれも同期経路では
  `builtin_collection` のローカル変数（`list`・`func`・ループの位置・`result`）が持っていたもので、新しい値は作らない。
  heap は e2a と同じく課金しない（`heap_charge` 0）。部分結果の `Vec<Value>` は同期経路と同じく `new_list` まで課金せず、
  反復元の List は `Rc` 共有の backing を保持するだけで複製しない（同期経路と同じ寿命・同じ `live_heap`）。
  - 反復元の List（評価済みの第 1 引数）と callback（評価済みの第 2 引数）。
  - 次に処理する要素の index。
  - 部分結果（`map` は callback の戻り値、`filter` は残す要素）。
  - `filter`: callback へ渡した要素（戻り値を受け取ってから部分結果へ push するため）。
- **yield 可能点**: e2a と同じく式 frame の段の間で、slice fuel の判定と強制 yield フックだけを行う。具体的には、引数の
  評価の境界（第 1 引数の評価後・第 2 引数の評価前、両方の評価後・型検査の前）、各要素の callback 呼び出しの前（要素 i の
  `count_step` の前）と戻り値を受け取った後（要素 i+1 の前・全要素の後の結果の組み立ての前）、callback 本体の文/反復境界
  （driver 上の Call frame。e1 と同じ）。強制 yield フックの前進の条件は e2a の `expr_progress` のまま。audit drain queue 上限と
  cancel / deadline の checkpoint は従来どおり文/反復境界だけ（callback 本体の文/反復境界を含む）。
- **cooperative host-call**: callback 本体の中は driver 上の Call frame の中なので `may_yield = false` のまま（Pending は
  catch 不能 `InternalControl`。範囲外のまま）。`map` / `filter` / `each` の引数の中の cooperative host function がトップレベルで
  Pending を返した場合は e3 と同じく文の式 frame をすべて畳んで文を評価し直す。同じ文の他の位置の `map` の callback が先に
  実行されていれば、評価し直しで再び実行される（同期経路の poll で文を `eval_expr` で評価し直す現行の観測と同じ）。
- **終了経路**: 引数の評価・型検査・callback の検査・callback 本体・部分結果の検査で起きたエラーは、e2a と同じく
  `handle_error` が Call frame と式 frame を畳んで最も近い `try` へ渡す（callback 本体の中の `try` も含む）。エラー行・trace は
  同期経路と同じ。予算超過は従来どおり catch 不能 terminal として伝播する。
- **TEC-AT-04 を持たない理由**: callback の深度検査は上の 4 の段で `call_fn_value` と同じ関数・同じ順序で行い、callback 本体は
  e1 が TEC-AT-04 を固定した driver 上の Call frame で実行する。深度の上限を超える入れ子の callback は、下の TEC-AT-01（全体）の
  エラー経路の script（callback の中から同じ `map` を呼んで `call_depth_limit` に達する）で同期経路との一致として確かめる。
- **受入基準の ID の数（判断役が選択（PR #464）、Q1=A）**: #250 の割り当て（TEC-AT-01〜03・05〜07 の 6 項目）は
  [開発ループ手順書](loop-runbook.md) §14.1 の「受入基準は 5 項目以内」を超える。e4 は 1 スライスのまま、TEC-AT-03（e4 分。
  既存テスト無改変で緑）を `acceptance` の ID から外し、全 sub-slice 共通の完了条件で満たす。
  - **e4 の done の条件として効き続けること**: TEC-AT-03 の ID を外しても、全 sub-slice 共通の完了条件（`cargo fmt --check`・
    `cargo clippy --all-targets --all-features -- -D warnings`・`cargo test --all-features` が緑で既存テストは無改変、同期経路の
    コードパスを変えず、同期経路と terminal まで poll する入口の fuel・heap・エラー行・trace・stdout・exit code が現状と一致）は
    e4 の done の条件のままである。e4 の実装 PR は、既存の `tests/`・unit test を 1 つも変えずに緑であることを確かめて PR 本文に
    書く。上の「受入基準（e4 が満たす部分）」の TEC-AT-03（e4 分）は、この共通の完了条件として満たす。観測値の一致は、加えて
    下の TEC-AT-01（全体）の `live_heap` / `peak_heap` の比較でも確かめる。
  - **理由**: TEC-AT-03 の本文は共通の完了条件の第 1・第 2 項（「TEC-AT-03 のその sub-slice 分」と明記）と同じ内容で、ID を
    外しても確かめる内容は減らず、CI と TEC-AT-01（全体）の比較で再現可能に裏付けられる（マニフェスト原則 9）。e4 に固有の
    検証（TEC-AT-01・02・05・06・07）はすべて残り、callback 途中の yield を跨ぐ transaction 不変（TEC-AT-05、原則 6）も e4 で
    確かめる。約 400 行・仕組み 1 つの変更を 1 スライスに保ち、機構と組み合わせテストが同じ PR で入る（原則 8、手順書 §14.1）。
  - **採らなかった案**: e4a / e4b への分割（B）は、e4b がテストだけのスライスになり、e4a の done の時点で e1〜e4 を跨ぐ組み合わせが
    未検証のまま機構だけが入るうえ、設計・実装の往復が 2 組かかる。TEC-AT-05 を外す案（C）は、TEC-AT-05 本文が明示する「callback
    途中」の yield を確かめるスライスが無いまま tree-expr-continuation が done になりうるため、マニフェスト（原則 6・9）に反しうる
    （e3 が TEC-AT-05 を外せたのは式 frame が e2a と同じ frame 種別だったからで、e4 は新しい反復の状態と受け渡し先を足す）。
- **受入基準の確かめ方（e4 が満たす部分）**:
  - **TEC-AT-07**: `tests/engine_api.rs` に `map` / `filter` / `each` それぞれの script（callback 本体が `print` と `while` を持つ、
    callback が lambda・名前付き関数・captured を使う closure のもの）を置き、強制 yield フックで要素の境界と callback 本体の
    文/反復境界の両方で yield させ、e1 以来の比べ方（強制 yield ありの poll 経路と `can_yield = false` の `Evaluator::run`、
    監査付きと sink 無しの `Engine::run`）で stdout・`committed.fuel`・terminal 種別・非 `Yielded`/`Resumed` の audit event 列を
    比べる。エラー経路（第 1 引数が List でない、callback が呼べない値〔空の List ではエラーにならないことを含む〕、callback の
    arity 違反、callback 本体の実行時エラーを callback の中と外の `try` で捕まえる、callback 本体の `break`、`map` の結果の要素数が
    `max_collection_elements` を超える）の種別・行・trace も一致させる。
  - **TEC-AT-01（全体）**: 同じ比べ方で、e1〜e4 の yield 可能点を 1 つの script の中で跨ぐ組み合わせを置く。例: `print(len(map(xs, f)))`、
    `let ys = map(xs, f) + filter(xs, g)`、`map(map(xs, f), g)`、`map(f(), g)`（引数に呼び出し）、callback の中の入れ子の
    ユーザー関数呼び出し・`while`・`each`、`if len(filter(xs, g)) > 0`、`for y in map(xs, f)`、index 代入の値の `map`、e1 の形の文
    （`each(xs, f)`・`let ys = map(xs, f)`・`return filter(xs, g)`）。上の「TEC-AT-04 を持たない理由」の深度上限の script も置く。
    `src/eval.rs` の unit test で、これらの poll 経路の `live_heap` / `peak_heap` が `Evaluator::run` と一致することを見る。
  - **TEC-AT-02（e4 分）**: `src/eval.rs` の unit test で、反復元の List・callback・次の要素の index・部分結果・`filter` の渡した
    要素が yield を跨いで保存されることを固定する。`tests/engine_api.rs` で、各要素の callback が副作用（`print`）を持つ script を
    強制 yield フックで 1 境界ずつ yield させ、resume 後の値と出力順が同期経路と一致することを見る（各 callback が 1 回だけ
    実行されることを含む）。
  - **TEC-AT-05（e4 分）**: `tests/engine_api.rs` の PR-d・e1 の transaction のテストと同じ形で、callback 本体が context の変数へ
    書き込み・`push` する script を callback の途中で yield させ、yield を跨いでも submission journal が保持され、成功の terminal で
    commit・失敗の terminal で rollback されることを見る。
  - **TEC-AT-06（全体）**: callback 本体にループを持つ `map` / `filter` / `each` と、それを関数本体・builtin の引数・別の callback の
    中に入れ子にした script を `PollSlice::max_fuel = 16` で poll し、poll ごとの fuel 増分が「slice + 区間内の callback なし bulk
    builtin 1 回分の単独実測 fuel」以内であることと、audit の `Yielded`/`Resumed` の対と順序を見る（e1 の TEC-AT-06 と同じ測り方）。
- **逸脱一覧の更新（e4 の実装 PR で行う）**: 「未実装範囲」の e4 の行（`map` / `filter` / `each` の要素境界と callback 本体の
  yield、引数の呼び出し）を削除する。「関数本体・callback 本体の中での cooperative host-call の Pending yield と audit drain queue
  上限の yield」の行は範囲外なので残す。
- **想定の変更規模**: おおむね 400 行（[開発ループ手順書](loop-runbook.md) §14.1 の 1000 行に収まる）。関数へ移して字下げが
  変わる行は追加と削除の両方に数えた。
  - `src/eval.rs` 約 280 行: `resolve_call_target`・`needs_expr_frame`・e1 の形の文の振り分け 25、`CallTarget` と `ExprFrame` の
    追加の状態 20、`step_target_call` の callback builtin の段（最初の段、引数、型検査、要素ごとの前処理と Call frame の push、
    結果の組み立て）140、`CallDest` の追加と戻り値の受け渡し（`break` / `continue` のエラー化を含む）60、強制 yield・前進の条件の
    配線 35。
  - `src/builtin.rs` 約 120 行: callback の callable / arity 検査を `call_fn_value` と共有する関数に分ける 40、`map` / `filter` /
    `each` の部分結果の push と結果の組み立てを `builtin_collection` と共有する関数に分ける 80。
  - e2b は見積もり約 385 行に対し実績 491 行（PR #280）、e3 は約 650 行に対し 691 行（PR #296）だった。e2b の比率で上振れしても
    約 510 行で 1000 行に収まる。
  - 触る仕組みは tree evaluator の call の評価 1 つ（e3 と同じく、`src/builtin.rs` は builtin の挙動を変えず、検査と結果の組み立ての
    境目を分けるだけ）。受入基準は 5 項目（PR #464 の Q1=A）。

#### fuel-bulk-chunk-yield 実装スライス（callback なし bulk builtin の chunk 境界 slice yield）

> 状態: **未実装**（設計のみ）。本小節は設計 PR を書いた時点の記録で、現在の状態は冒頭の逸脱一覧・未実装範囲を見る。
> 判断の候補（Q1〜Q4）は設計 PR #468 に出し、判断役が Q1=A, Q2=A, Q3=A, Q4=A を選択した（下の「決定記録」）。下の記述はこの選択どおりである。

tree-expr-continuation（e1〜e4）で呼び出しと反復は yield 可能点になったが、callback を取らない bulk builtin は
1 段で完了まで実行する（e3 小節「builtin・host function の本体は 1 段で完了まで実行し、yield しない」）。加えて slice の
使用量は `count_step` だけを数え、builtin が台帳へ直接課金する per-element fuel を数えない（冒頭の逸脱一覧の Slice 3 PR-d の行）。
本スライスは §4.1 の「collection/string builtin は長い処理を最大 256 elements または 16 KiB ごとの小 chunk へ分け、chunk 前に fuel と
cancel/deadline を確認する」と §4.2 の「bulk work は chunk へ分割する」を、tree evaluator の poll 経路で実効化する。

- **実装範囲**:
  1. **slice の使用量を台帳の commit 量で数える（Q1 = A）**: `run_slice` の開始時に台帳の commit 済み fuel（`BudgetLedger::committed_fuel`）を
     控え、`slice_exhausted` は「現在の commit 済み fuel − 控えた値」を slice 上限と比べる。`count_step` の `slice_fuel_used` 加算は
     この差分に置き換える（二重に数えない）。これで builtin の per-element 課金（`sort` の走査・比較、`join` の描画、`contains` の
     等価判定、index 代入・`pop` の深度の読み直し）も slice の使用量に入る。yield するのは従来どおり yield 可能点だけで、charge
     そのものは分割しない（§4.2「1 個の固定 charge を分割しない」）。
  2. **chunk 境界を yield 可能点にする builtin（Q2 = A・Q4 = A）**: 要素ループの中で台帳へ fuel を課金しうる次の 3 つ。chunk 境界は
     要素の種類によらず yield 可能点にするが、slice fuel で yield が起きるのは区間で fuel が積もるときだけである（Q4 = A）。現状の
     課金で fuel が積もるのは、revision 0.21 以降の `sort` の走査（1 / element）と、List / Dict の要素の描画（`render_into`）・等価判定
     （`values_equal`）の複合ノードごとの 1 である。スカラー要素（`Str` / `Int` / `Float` / `Bool` / `Null`）の描画・等価判定は fuel を
     課金しないので、スカラー要素の List への `join` / `contains` と revision 0.20 以下の `sort` の描画キーは、chunk 境界があっても
     slice fuel では区切れない（下の「範囲外」）。課金はこのスライスで変えない。
     - `join`: 要素ごとに区切りの追記と `render_display_append`。256 要素ごと、または chunk 内で buffer に追記した byte が 16 KiB
       （16,384 byte）に達したら chunk を区切る（要素の途中では区切らない）。
     - `contains`（第 1 引数が List のとき）: 要素ごとの `values_equal`。256 要素ごとに区切る。見つかった時点で終わる。
     - `sort`: 要素数ぶんの走査課金（revision 0.21 以降）と、描画キーの生成（revision 0.20 以下の `render_display`）を 256 要素
       ごとに区切る。比較の段（`sort_by`）は 1 段のまま（Q3 = A、下の「範囲外」）。
  3. **段階実行の形**: 上の 3 つを `src/builtin_core.rs` で「状態 + 1 chunk 進める関数」の形に分ける（例: `BulkState` と
     `bulk_step(state, budget) -> Result<BulkStep, TsumugiError>`、`BulkStep` は `Continue` / `Done(Value)`）。既存の
     `builtin_join_budgeted` / `builtin_contains_budgeted` / `builtin_sort_budgeted` は、この関数を完了まで回す薄い wrapper にして
     名前・引数・戻り値を変えない。VM と同期経路（`can_yield = false`）はこの wrapper を呼ぶので、課金の順序・量・エラー点は現状と同じ。
  4. **tree の poll 経路**: e3 の式 frame の call の段（`step_target_call`）で、対象の 3 builtin は dispatch の段で `BulkState` を作って
     式 frame に保存し、1 段ごとに `bulk_step` を 1 回呼ぶ。`Continue` のあとの段の間が yield 可能点で、slice fuel の判定と強制 yield
     フックだけを行う（e3 の引数境界と同じ。audit drain queue 上限と cancel / deadline の checkpoint は文/反復境界のまま）。
     cancel / deadline は従来どおり各 charge の前（`reserve_all`）で確かめるので、chunk 境界に新しい checkpoint は足さない。
- **yield を跨いで保存する状態**: 第 1 引数の List（`Rc` を共有。複製しない）、区切り文字列（`join`）、探す値（`contains`）、次の要素の
  index、追記途中の buffer（`join`）、走査課金・描画キーの途中結果（`sort`）、revision。部分結果は同期経路と同じく完了まで
  heap に課金しない（e4 の部分結果と同じ規律）。式 frame 自身の `heap_charge` は 0（e2a の Q1 = A の既定のまま）。
- **範囲外（逸脱として残すもの）**:
  - `sort` の比較の段（Q3 = A）: `sort_by` と比較ごとの `compare_budgeted` は 1 段で実行し、§4.2 の超過（1 回分を collection 上限で
    有界な超過）として扱う。比較の段を中断可能なソートへ書き換えると比較回数・比較順が変わり fuel 値が変わるため、このスライスでは
    書き換えない（e1〜e4 と同じく同期経路の観測を変えない）。逸脱一覧の Slice 3 PR-d の行は、この残りに絞って書き直す。
  - per-element の fuel を課金しない bulk builtin（`range` / `keys` / `values` / `reverse` / `slice` / `split` / 文字列の繰り返し・連結 /
    大きな collection のコピー等）: 課金しないので fuel の slice では区切りが生じない。これらへの per-element 課金は F-2 と
    `fuel-bulk-charge` の範囲で、chunk 境界 yield はその課金が入った後に別スライスで扱う。
  - 1 つの値の描画（`to_str`、`join` の 1 要素）と `replace` / `path_join`: chunk の単位が要素でなく描画の node / byte になり、
    `render_display` の再帰を段階実行にする別の仕組みが要る。
  - VM（slice yield を持たない。Slice 6）。VM は wrapper を呼ぶので fuel・結果は変わらない。
  - スカラー要素の `join` / `contains` と revision 0.20 以下の `sort` の描画キー（Q4 = A）: 要素の描画・等価判定が fuel を課金しない
    ので、slice fuel で yield が起きず、1 回分を §4.2 の超過（collection / string 上限で有界）として扱う（現状から悪化しない）。
    §4.1 の per-element（1 / element）・UTF-8 の copy（`ceil(bytes/64)`）の課金は F-2・`fuel-bulk-charge` の範囲で、その課金が入れば
    本スライスの chunk 境界が追加の仕組みなしで slice yield に効く。
- **観測挙動**: 同期経路（`Evaluator::run`・VM・terminal まで poll する `Engine::run`）の fuel 値・成否・エラー行・trace・stdout・
  exit code・heap は現状と一致させる。poll 経路で変わるのは yield の位置（poll の粒度）だけで、Q1 により builtin の fuel を含む
  slice は従来より早く yield する。既存のテストで poll の回数や yield の位置を固定しているものがこの変更で変わる場合は、実装 PR
  本文に一覧と理由（Q1 による意図的な差）を書く。それ以外の期待値の変更は止めて報告する。
- **受入基準**（`tests/engine_api.rs` と `src/eval.rs`・`src/builtin_core.rs` の unit test）:
  - **BCY-AT-01（slice の使用量）**: 2,000 要素の List を `sort` する文と、2,000 個の List を要素に持つ List を `join` する文を並べた script を `PollSlice::max_fuel = 16` で
    poll し、poll ごとの commit 済み fuel の増分が「max_fuel + chunk 1 つ分の fuel + `sort` の比較の段 1 回分」以内であることと、
    `sort` の文の直後の yield 可能点で `Yielded(SliceFuelExhausted)` になる（slice の使用量が builtin の fuel を数える）ことを見る。
    index 代入・`pop` の深度の読み直しを含む script でも同じ上限を見る。
  - **BCY-AT-02（chunk 境界での yield）**: スカラー 10,000 要素の List への `sort`（revision 0.21 以降の走査）と、複合要素（各要素が
    小さな List）10,000 個の List への `join`・`contains`（最後の要素と等しい List を探す）を `max_fuel = 16` で poll し、builtin の途中で
    `ceil(10000 / 256) − 1` 回以上 yield すること、poll ごとの fuel の増分の上限（BCY-AT-01 と同じ）、audit の `Yielded`/`Resumed` の
    対と順序を見る（Q4 = A）。byte の 16 KiB 境界（1 要素が 16 KiB 近い文字列の List への `join`）は fuel が積もらないので、強制 yield
    フックで chunk 境界として区切られること（要素の途中では区切らないこと）を BCY-AT-03・04 で見る。スカラー要素の `join` /
    `contains` では slice fuel による yield が builtin の途中で起きないこと（範囲外の記録どおり）も 1 件固定する。
  - **BCY-AT-03（同期経路・VM との一致）**: BCY-AT-01・02 の script と、エラー経路（`join` の途中で total fuel が尽きる、
    `max_single_string_bytes` を超える、第 1 引数・区切りの型違い、`contains` が途中で見つかる、revision 0.20 の `sort`）を、強制
    yield フックで全 chunk 境界で yield させた poll 経路・`Evaluator::run`・監査付きと sink 無しの `Engine::run`・VM で走らせ、
    stdout・`committed.fuel`・terminal 種別・エラー行・trace・非 `Yielded`/`Resumed` の audit event 列・`live_heap` / `peak_heap`
    （tree の経路どうし）を比べる。
  - **BCY-AT-04（resume の忠実性と transaction）**: `src/builtin_core.rs` の unit test で、`bulk_step` を 1 回ずつ回した結果が
    wrapper と同じ値・同じ fuel になること（chunk の大きさ 256 要素・16 KiB の境目を含む）を見る。`tests/engine_api.rs` で、
    context の変数へ書き込んだあと bulk builtin の途中で yield させ、yield を跨いで submission journal が保たれ、成功で commit・
    失敗（chunk の途中の予算超過・cancel）で rollback されることを見る。
  - **BCY-AT-05（既定 slice の観測不変）**: 既存の `tests/`・unit test が、上の「観測挙動」に書いた poll の粒度の差を除いて
    無改変で緑。`sort` / `join` / `contains` の fuel 値を固定している既存テスト（tree と VM）が変わらないことを含む。
- **逸脱一覧の更新（実装 PR で行う）**: 未実装範囲の fuel-bulk-chunk-yield の行を、上の「範囲外」の残り（per-element 課金の無い
  bulk builtin、スカラー要素の `join` / `contains` と revision 0.20 以下の `sort` の描画キー〔Q4 = A。備考は F-2・`fuel-bulk-charge`〕、
  1 つの値の描画）に書き直す。逸脱一覧の Slice 3 PR-d の行は、`sort` の比較の段だけが slice を超えうることに
  書き直す。
- **決定記録（判断役が選択（PR #468）、Q1=A, Q2=A, Q3=A, Q4=A）**:
  - **Q1 = A（slice の使用量を台帳の commit 済み fuel の差分で数える）**: 理由: §4.2 の「slice 内で実際に commit した fuel」と
    同じ定義になり、数え方が 1 系統で済む（[マニフェスト](manifesto.md)原則 8）。builtin の fuel を数えない限り chunk 境界を足しても
    yield が起きず、原則 4（ホストへ定期的に制御を返す）と優先順位の表「制御可能な最悪時負荷」を満たせない。変わるのは poll 経路の
    yield の位置だけで、同期経路・VM の fuel 値と結果は変わらない（原則 5）。逸脱一覧の Slice 3 PR-d の行（index 代入・`pop` の
    深度の読み直しを含む）も解け、e1 の Q2 = A の先送り（本スライスの設計範囲に含めるか needs-design issue）とも合う。
    採らなかった案: B（chunk ループの中だけで数える）は chunk の外の builtin fuel を数えず逸脱が残り、数え方が 2 系統になって
    検証しにくい（原則 8・9）。C（数え方を変えず別 issue に分ける）は chunk 境界の仕組みだけが入りホストへ制御が戻るまでの時間が
    変わらないため原則 4 に反しうるうえ、受入基準を書けない。
  - **Q2 = A（対象は `join` / `contains` / `sort` の走査・描画キーの 3 つ）**: 理由: fuel の slice で区切りが生じるのは今 per-element
    fuel を台帳へ課金している builtin だけで（§4.1・§4.2）、仕組み 1 つ・約 450 行で [開発ループ手順書](loop-runbook.md) §14.1 に
    収まる（原則 8）。残りは上の「範囲外」と未実装範囲の行に記録が残る（原則 9）。採らなかった案: B（`range` / `keys` 等も chunk 化）は
    今 per-element fuel を課金しないので、同期経路・VM の fuel 値が変わる課金（F-2・`fuel-bulk-charge` の範囲）を先に入れる必要があり、
    仕組みが 2 つになって規模も超えうる。C（`to_str` と `join` の 1 要素の描画も段階実行）は `render_display` の再帰を明示スタックに
    する別の仕組みが要る。どちらもマニフェストには反しないが、後続スライスで扱える。
  - **Q3 = A（`sort` の比較の段は 1 段のまま §4.2 の有界な超過として残す）**: 理由: 走査・描画キーは chunk 化するので現状より超過は
    減り、残る超過は `max_collection_elements` で有界である（§4.2 の超過例外、原則 1 の最悪値は有限のまま）。同期経路・VM の `sort` の
    fuel 値を変えないので e4 小節「`sort` は対象外」の決定と一貫し、BCY-AT-03・05 で検証できる（原則 5・9）。逸脱一覧の Slice 3 PR-d
    の行はこの残りに絞る。採らなかった案: B（中断可能な安定マージソートへ置き換え）は比較回数が std の実装に依らなくなる利点
    （原則 5）はあるが、既存の `sort` の fuel 値という言語の観測値が変わり、fuel 会計の revision と合わせて扱う別の設計が要る。
    本スライスの「同期経路の観測を変えない」前提と規模を超える。C（`sort` を対象から外す）は前段の要素数ぶんの課金でも yield できず、
    A より原則 4 から遠い。
  - **Q4 = A（課金は変えず、範囲と受入基準を実態に合わせて絞る）**: 設計レビューの指摘（PR #468）で、`values_equal` と
    `render_into` は List / Dict のノードにだけ fuel を課金し、スカラー要素は無課金であることが分かった（`src/builtin_core.rs` の明示の
    per-element 課金は revision 0.21 以降の `sort` の走査だけ）。理由: 同期経路・VM の fuel 値という言語の観測値を変えないので、
    Q2 = A・Q3 = A と本小節の「観測挙動」の前提、F-2 小節・fix-contains-budgeted-equality 小節が per-element / `ceil(bytes/64)` の課金を
    F-2・`fuel-bulk-charge` へ回した既存の決定と一貫する（原則 5、優先順位の表「一貫した意味論」）。触る仕組みは段階実行 1 つのままで
    （原則 8）、BCY-AT-02 の入力を実際に満たせる形に直すので受入基準が再現可能な検証になる（原則 9）。スカラー要素が slice で区切れない
    点は現状から悪化せず §4.2 の超過例外の範囲に残り、「範囲外」と未実装範囲の行に記録される（原則 1 の最悪値は有限のまま。原則 4 への
    到達は §4.1 の課金が入る `fuel-bulk-charge` の後で、chunk 境界の仕組みはそのとき追加の機構なしで効く）。採らなかった案: B
    （スカラー要素にも §4.1 の課金を入れる）は同期経路・VM の fuel 値と既存テストの期待値が変わり、fuel 会計の revision と既定
    total_fuel の再測定（F-2・`fuel-bulk-charge` の範囲）を伴い、仕組みが 2 つになる。C（chunk 数・byte 数でも yield する）は目標の章
    §4.2 の slice の定義と §9.1 の `YieldReason`・`PollSlice::max_fuel` の意味を変える設計が要り、fuel を使っていないのに
    `SliceFuelExhausted` で止まると yield の理由と資源消費の対応が説明できなくなる（原則 5・7）。D（0.21 の `sort` の走査だけに絞る）は
    複合要素の `join` / `contains` も区切れなくなり、Q2 = A を理由なく狭める。
- **想定の変更規模**: おおむね 450 行（[開発ループ手順書](loop-runbook.md) §14.1 の 1000 行に収まる）。
  - `src/builtin_core.rs` 約 230 行: `BulkState` / `BulkStep` と `bulk_step` 30、`join` の chunk 化 60、`contains` の chunk 化 40、
    `sort` の走査・描画キーの chunk 化と比較の段の分離 70、wrapper 化 30。
  - `src/eval.rs` 約 170 行: slice の使用量を commit 量の差分にする 30、式 frame の bulk の状態と `step_target_call` の bulk の段 100、
    強制 yield・前進の条件の配線 40。
  - `src/builtin.rs` 約 50 行: 3 builtin の dispatch を wrapper と段階実行で共有する形に分ける。
  - e2b は見積もりに対し約 1.3 倍の実績（PR #280）だった。同じ比率で上振れしても約 590 行で収まる。
  - 触る仕組みは callback なし bulk builtin の段階実行 1 つ（slice の使用量の数え方はその判定の入力で、`run_slice` と
    `slice_exhausted` だけを変える）。受入基準は 5 項目。

### Slice 4: cancellation・pause・transaction

- `CancellationToken`、pause/resume、checkpointを実装
- mutation journalでAUD-024のrollback規則を実装
- terminal後resume不可とraceのlinearizationを固定

### Slice 5: scheduler・host pending

- bounded active/queue、FIFO round-robin、backpressureを実装済み（`SchedulerShared` + `AdmissionController` が単一 Mutex 下で active/queue slot を atomic 会計し、`create_execution`/`start` が handle 生成前に slot を予約・両満杯は即時 `StartError::Backpressure { active, queued, limit }` で handle を作らず context も変えない。queue slot を得た handle は `Yielded(AdmissionQueued { resume_to })` で semantic work を一切行わず、active が空くと FIFO 先頭を active へ atomic 変換し waker で通知する。Engine 全体の run-turn FIFO queue が先頭 handle の poll だけを 1 slice 進め、非先頭の Ready handle は semantic work なしで `Yielded(SchedulerPreempted)`（reason のみ、`self.state` 不変）を返す。Engine は worker thread を spawn しない）
- nonblocking host call ticket、wake、deadline/cancel伝播を実装済み（host adapter が `HostCallPoll<T> { Ready(Result<T, AdapterError>), Pending(HostCallTicket<T>) }` を返し、`Pending` は execution を `Yielded(HostCallPending { call_id })` にして ticket へ 1 個の `ExecutionWaker` を登録する。登録は「waker 保存 → ready flag 再確認 → ready なら即 wake」の lost-wake-safe 順序で、`ExecutionHandle::set_waker(Some(w))`/`set_waker(None)` が作成 thread から waker を置換・解除する。adapter executor が結果を ticket へ一度だけ格納して wake し、作成 thread の次回 poll が `try_take` する（wake は continuation を別 thread で実行しない）。`CancellationToken::register_waker` が cancel 時に登録 waker を wake し、queue 待ち・pause・host 待ちのいずれでも cancel/deadline の checkpoint が発火する。cancel/deadline terminal は `budget::control_stop_to_error` 経由で catch 不能化し、`CooperativeAdapterYieldInSyncContext` 違反は `ErrorKind::InternalControl`（`is_uncatchable`）の catch 不能 terminal になる。ticket drop/cancel は遅着 result を破棄する）
- adapter executorにも独立したconcurrency/queue上限を設定済み（`CooperativeAdapter<Request, Response>` を Phase-4 でだけ登録でき、`CapabilityCallContext`（`may_yield`・deadline・cancellation・残 request/response bytes・call_id）を介して同期 adapter と同じ capability/budget/deadline/cancellation を使う。Engine 本体と別の有限 thread pool `AdapterExecutor`（`AdapterExecutorLimits { max_threads, max_queued, max_concurrency }`）が結果を駆動し、queue 満杯は caller を block せず `SubmitError::QueueFull`＝cooperative start は `Ready(Err(..))` へ落として backpressure へ変換する。scheduler/context/audit lock を保持したまま adapter を呼ばない）

VM charge parity（Slice 6）はこの slice の対象外で VM は cooperative にせず Ready にフォールバックする（`vm.rs` 不変）。cancel で host call を `Cancelled`/`Detached` に閉じる構造は用意する。audit event の実発行と `AuditBackpressure` yield は本 Slice 5（engine-poll 経路）では据え置いたが、Phase 6 B-1 で **facade（`embedding.rs`）の監査付き poll driver** 側に配線した（sink の `AuditSubmit::Pending` を観測して `Yielded(AuditBackpressure)` へ遷移・ack で resume、永続 Pending→`AuditFailure::BackpressureDeadline`）。scheduler engine の poll が `YieldReason::AuditBackpressure` を返すのは E10 統合後のままで、sink を持たない engine-poll 経路は本 slice どおり変えない。詳細は [決定性・実行時監査仕様](determinism-and-audit.md) §10/§10.2/§14。`ExecutionHandle` は `!Send + !Sync` のままで、`Send + Sync` にするのは `SchedulerShared`/`AdmissionController`/run-turn queue/`ExecutionWaker`/`HostCallTicket`/`CancellationToken` だけ。既定/legacy config の同期経路（CLI/REPL/alpha facade・`embedding::Engine::run`）は観測挙動不変で、scheduler/admission/host-pending surface は新 API で opt-in。

### Slice 6: VM charge parity

- VMへ`Charge` opcodeと同じcontinuation/outcome契約を実装
- treeとのcharge trace、terminal boundaryをpaired testで一致させる
- 一致するまでVMはexperimentalであり、production schedulerへadmitしない

既知差の解消状況（順次 paired test で固定していく）:

- **f-string のリテラル部分の課金差（✅ 解消済み）**: VM は f-string のリテラル部分を
  課金しない `LoadConstRaw`（定数表からの非課金 load）で積み、tree と同じく結合後の最終
  body だけを `FStrConcat` が課金する。これで tree の「リテラルを `push_str` で結合し
  個別課金しない」会計と一致し、cumulative `StringAllocations`/`StringBytes` と live heap が
  両 engine で等しくなる。paired test は `tests/integration.rs` の
  `fstring_literal_parts_not_charged_in_both_engines`（リテラルの余分課金がないこと）と
  `fstring_final_body_is_charged_in_both_engines`（最終 body は両 engine で課金すること）。

- **空 collection リテラルの課金差（✅ 解消済み）**: VM の `x=[]` / `x={}` を materialize 時に
  body 課金し、tree と揃えた（詳細は第5節 collection per-drop の既知差リストを参照）。paired
  test は `tests/scaling.rs` の `empty_collection_literal_live_heap_matches_across_engines`。

- **`pop` の full-clone 差（✅ 解消済み）**: VM の `pop` を `OpCode::PopUpdate`（→
  `list_pop_tracked`）で in-place delta release 化し、tree の `budget_list_pop` と揃えた。

- **`push` の full-clone 差（✅ 解消済み）**: 以前は VM の `push` だけが
  `CallBuiltin(builtin_push)` 経路で、builtin が backing を full-clone した untracked List を
  返し、dispatch 境界の `track_result` が list_body(N+1) を丸ごと課金していた。push 済みの旧
  backing もまだ live なので、push のたびに新旧が一瞬同時に live になり `peak_heap` が
  `live_heap` を上回ってスパイクしていた（tree は `list_push_tracked` の in-place delta 課金で
  スパイクしない）。`pop` の `PopUpdate` と対称に、VM の `push` を対象 binding storage を直接
  書き換える in-place 命令 `OpCode::ListPushBinding`（`SetIndex` と同じ binding storage 経路で
  `list_push_tracked` を呼ぶ）へ切り替えて解消した。backing の `Rc` を operand stack へ複製
  しないため、一意所有では COW 複製なしの delta（+32 byte）だけを課金し、tree と `peak_heap`
  が一致する（共有 backing は tree と同じく `detach_list` が COW 複製する）。`builtin_push` は
  builtin registry 契約・エラーメッセージ parity のため残置する。paired test は
  `tests/scaling.rs` の `push_peak_heap_has_no_full_clone_spike_across_engines`（push 後に
  `peak_heap == live_heap` となりスパイクが消えたことを両 engine で固定）・
  `push_to_shared_backing_has_no_spike_across_engines`（共有 backing への push もスパイク
  しないこと）・`push_live_heap_is_linear_not_quadratic_across_engines`（live heap が O(N) に
  収まる回帰ゲート）。

- **REPL 持続 closure が retain する code の課金差（設計確定 A-1・実装は後続）**: closure を
  定義した入力の次の REPL 入力で、両 engine とも前入力の artifact 課金を丸ごと release する
  （tree=`ast_token = None`、VM=`chunk_tokens.clear()`）。しかし生き残った closure が握る code
  （tree=`FnDef.body` の `Rc<[Stmt]>`、VM=`VmFn.chunk` の prototype `Rc<Chunk>`）はメモリ上に
  残るのに**どちらの engine でも再課金されず**、両 engine が対称に under-charge する。release
  で落ちる論理サイズが engine ごとに違う（tree=AST body / VM=prototype chunk）ため、入力を
  またいだ `live_heap_bytes` / `peak_heap_bytes` が backend 間でズレる。この差は push/pop と
  違い「VM だけが誤り」ではなく「両 engine が retain を取りこぼす」非対称であり、§5.3 の
  **A-1「reachable な code は課金し続ける」モデル**（closure 寿命トークンで課金）で両 engine
  を揃えて閉じる。これにより reachable code の**課金の有無と release 寿命が backend 間で一致**
  する（従来の「次入力で落ちる」under-charge が両 engine から消える）。ただし課金額は engine
  ごとの artifact 単位（tree=`ast_node` / VM=`bytecode_chunk`）のままで、§5.3 の「AST または
  bytecode」どおり `live_heap` / `peak_heap` の数値そのものは engine 固有であってよい（byte
  単位の数値一致まで求める A-2 は採らない）。これにより [決定性・監査仕様](determinism-and-audit.md)
  §3 の「terminal 時 `BudgetUsage` の backend 一致」要件（課金の有無・寿命が揃うこと）および
  manifesto 原則 5（resource 消費の backend 差＝「片方だけ落ちる」非対称を許容しない）を満たす。
  差を仕様として受容する案（A-3）は採らない。**実装済み（PR-6b, commit `5be0ea2`）**で
  tree / VM 双方へ closure 寿命トークンを配線し、`tests/closure_retain.rs` の paired test で
  固定した（PR-6c で differential matrix `tests/differential.rs` へ集約し `closure_retain.rs` は
  harness へ載せ替えた）。実装詳細の課金規則・二重課金回避境界・release 寿命は §5.3
  「AST または bytecode」で定義する。
  closure retain 課金は PR-6b で実装済みであり、**Slice 6 の残作業は §15.5 charge/budget
  parity matrix の自動化のみ**である（下記 §15.5 の PR-6c 線引きを参照）。

- **fuel 課金差（✅ 解消済み：下記「F-1 実装スライス」）**: Slice 6 で揃えたのは heap・string・
  I-O の会計で、fuel は揃っていない（2026-10-20 判明）。tree はループ反復・user function
  呼び出し・callback 呼び出しごとに 1、VM は REV-006（[次期意味論・実装決定](semantic-decisions.md)
  §17.7）以降、全命令ごとに 1 を課金しており、同じ script が tree では成功して VM では step
  上限に達する（既定 1,000,000 で、2 文ループの最大反復数は tree 1,000,000 / VM 66,665、
  再帰 fib が通る最大 n は tree 27 / VM 22）。§15.5 の PR-6c 線引きは「fuel parity は StepLimit
  terminal の observable 一致で担保する」としていたが、differential が両 engine とも上限に
  達する script だけを比べていたため、この差を検出できなかった。§4.1 の課金表は tree・VM の
  どちらにもまだ実装されていない（Slice 1 注記の「fuel 全 charge point の展開は Slice 2」は、
  Slice 2 の項目に含まれないまま Slice 2 完了とされた）。

#### F-1 実装スライス（fuel parity：VM を tree の現行課金点へ、2 メーター化）

> 状態: **実装済み**（2026-10-20）。実装上の注記は本小節末尾を参照。§4.1 の課金表そのものの実装は後続の F-2（未定義）で扱う。

fuel を「論理量」と「停止性の安全網」の 2 つのメーターに分ける（[次期意味論・実装決定](semantic-decisions.md)
§17.7 の 2026-10-20 改訂）。F-1 は、VM の fuel を tree の**現行の**課金点に合わせて tree/VM の
fuel parity を回復し、停止性を fuel から独立した instruction watchdog へ移す。§4.1 の課金表
（statement / expression / operation 単位の課金）への移行は、両 engine の fuel が一致した状態から
F-2 で行う（F-2 は既定 `total_fuel` の見直しと `language_revision` の扱いを含むため別 slice とする）。

- **fuel の課金点（tree は変更しない）**: VM は次の 3 点でだけ fuel を 1 課金し、全命令での課金を
  やめる。いずれも tree の課金点と同じ順序・同じ line で課金する。
  1. **(F1) ループの back-edge**: `OpCode::Loop`。line は loop 文の line（compiler が `Loop` に
     付ける line）。tree の「本体 1 反復の完了後・次反復の判定前」に対応する。
  2. **(F2) user function 呼び出し**: `OpCode::PrepareCall`。line は呼び出し位置。深度検査より前に
     課金する（tree の `count_step` → 深度検査の順と同じ）。
  3. **(F3) builtin の callback 呼び出し**: `Vm::call_fn_value`。line は builtin 呼び出しの line。
     深度検査より前に課金する。

  VM は host function に対応しないため、tree の host / cooperative host 呼び出しの課金点に当たる
  ものはない。sort・比較・表示の per-element 課金は共有コードのため変更しない。
- **`Loop` を back-edge 専用にする**: 現在の compiler は `for` の `continue` を `Loop` で
  increment 位置（前方）へ飛ばしている。このままだと `for` で `continue` した反復が VM だけ 2 回
  課金されるため、`for` の `continue` は前方 `Jump`（`break` と同じ patch 方式）へ変える。`while` の
  `continue` は loop 先頭への後方 `Loop` のままでよい（1 反復につき back-edge は 1 回だけ実行される）。
- **instruction watchdog**: [§17.7](semantic-decisions.md) の (W1)(W2) で tick する VM 内部カウンタ。
  - fuel・`BudgetUsage`・audit・slice yield のどれにも現れない。
  - 上限は `total_fuel × 2 + 16`（saturating）。tick のたびに現在の `total_fuel` から計算する
    （`set_max_steps` や REPL の再設定に追従する）。
  - REPL では fuel と同じく submission ごとに 0 へ戻す。callback の入れ子 `run_frames` を含め、
    1 execution（REPL では 1 submission）で 1 本のカウンタを共有する。
  - 上限に達したら、fuel の上限到達と同じ terminal（kind `limit`、`ステップ上限に達しました
    (上限: {total_fuel})`、catch 不可）へ写す。
- **観測される変化（VM のみ）**: fuel の消費量が tree と同じになり、VM で step 上限に達していた
  script の一部が成功するようになる（制限の緩和）。step 上限エラーの line と trace が tree と
  一致する（例: 呼び出しで上限に達したとき、VM も呼び出し位置の line で止まり、callee の trace は
  付かない）。tree・既定経路の観測挙動は変えない。`language_revision` は bump しない
  （language-spec は既に「tree と `--vm` は同じ観測挙動」と定めており、F-1 はその非適合の修正）。
- **変更箇所**:
  - `src/vm.rs`: メインループの全命令 `count_step` を削除し、(F1)〜(F3) で `count_step` する。
    watchdog（counter・上限計算・(W1) の ip 比較・(W2) を数える frame push の一本化）を追加し、
    REPL submission の境界で fuel と一緒に reset する。
  - `src/compiler.rs`: `for` の `continue` を前方 `Jump` に変える。
  - `src/verifier.rs`: 変更なし（`Loop` の target 範囲検査は従来どおり）。
  - tests: 下記。
  - docs: §17.7（改訂済み）、本小節、§15.5 の PR-6c 注記、[決定性・監査仕様](determinism-and-audit.md)
    §15.5 の同注記、roadmap。
- **受入基準（F-1）**:
  - **F1-AT-01**（fuel 一致・ループ）: `while`・`for`・入れ子ループ・`while`+`continue`・
    `for`+`continue`・`break` で、両 engine の `committed.fuel` が完全一致する（differential の
    in-process 層で exact 比較）。
  - **F1-AT-02**（fuel 一致・呼び出し）: 再帰・相互再帰・closure 呼び出し・`map`/`filter`/`sort`
    の callback で、`committed.fuel` が完全一致する。
  - **F1-AT-03**（上限の境界）: F1-AT-01/02 の各 script について、上限 N = 必要 fuel では両 engine
    とも成功し、N − 1 では両 engine とも step 上限で失敗する。stdout・stderr（line・trace を含む）・
    exit code が一致する。
  - **F1-AT-04**（既定上限での一致）: 既定 1,000,000 で、2 文ループ 1,000,000 反復が両 engine で
    成功し、1,000,001 反復が両 engine で失敗する。fib(27) が両 engine で成功する。
  - **F1-AT-05**（呼び出しの 1 課金）: 上限 1 で `once()` を 1 回呼ぶ script が両 engine で成功し、
    本体がちょうど 1 回実行される（REV-006 で弱めた `call_budget_is_checked_before_callee_in_both_engines`
    の「call = ちょうど 1 step」検査を復元する）。
  - **F1-AT-06**（watchdog による停止性）: `tests/defensive_vm.rs` の `Jump(0)` 自己ループ・後方 `Loop`
    迂回・`PrepareCall` なし raw `Call` 再帰が、従来どおり `limit` で有限停止する。
  - **F1-AT-07**（watchdog の非干渉）: F1-AT-01〜04 の全 script と既存の全 fixture で、watchdog の
    上限に達する前に必ず fuel の上限に達する（テスト用に watchdog の tick 数を観測する内部 API で、
    「tick 数 ≤ fuel 課金数 + root frame 数」を確認する）。
  - **F1-AT-08**（REPL）: VM REPL で submission ごとに fuel と watchdog が reset され、
    tree REPL と同じ入力列で同じ出力になる。
  - **F1-AT-09**（catch 不可）: fuel・watchdog のどちらの上限到達も try/catch で捕捉できない
    （両 engine）。
  - 既存テストのうち、VM の per-instruction 課金を前提にした期待値（例: `call_budget_is_checked_before_callee_in_both_engines`
    の弱めた検査、differential の「向き・符号」比較）は、F-1 の exact parity へ置き換える。
- **テスト配置**: 境界・CLI は `tests/integration.rs`、exact fuel 比較は `tests/differential.rs`、
  停止性は `tests/defensive_vm.rs`、watchdog の内部不変条件は `src/vm.rs` の unit test。
- **F-1 の範囲外**: §4.1 の課金表（F-2）、VM の deadline clock 注入、VM の audit、VM の
  cooperative 実行（いずれも既存の後続項目）。
- **実装上の注記（2026-10-20）**:
  - **消費 fuel の突合方法**: tree は成功 path で `committed.fuel` を 0 として snapshot する
    （入力ごとに reset）ため、F1-AT-01/02 は usage の直接比較ではなく「成功する最小の fuel 上限 N」
    を二分探索で求めて exact 突合する（`tests/differential.rs` の `assert_fuel_parity`）。同じ関数で
    N−1 の terminal 全 field（F1-AT-03）と watchdog の tick 数 ≤ N + 1（F1-AT-07）も検査する。
  - **`while` の `continue` の line**: `continue` が出す back-edge `Loop` には、`continue` 文ではなく
    ループ文の line を付ける（`LoopState.line`）。tree はループ文の line で課金するため、付けないと
    `continue` 直後に上限へ達したときの line が tree/VM で食い違う（F1-AT-03 で検出）。
  - **保留中の heap 超過**: 改訂前の全命令 `count_step` は、rollback journal の heap 超過
    （`pending_heap_stop`）を次の命令で表に出す役目も兼ねていた。fuel の課金点を減らしてもこの
    検査は全命令で残す（`surface_pending_heap_stop`）。
  - **watchdog**: `Vm::watchdog_ticks()` を診断・テスト用に公開した（`unstable-bytecode` の `vm`
    module の一部）。frame を積む経路は `push_call_frame` に一本化した（`Call` と callback。REPL の
    root frame は submission 開始時に watchdog と一緒に reset するため数えない）。
  - **テスト配置**: F1-AT-01/02/03/07 は `tests/differential.rs`、F1-AT-04/05/08/09 は
    `tests/integration.rs`（F1-AT-05 は `call_budget_is_checked_before_callee_in_both_engines` を
    「呼び出し 1 回＝ちょうど 1 fuel」へ復元）、F1-AT-06/09（watchdog 側）は `tests/defensive_vm.rs`。

#### F-2 実装スライス（§4.1 課金表への移行：主要 charge point・既定 total_fuel 見直し・fuel 会計 revision）

> 状態: **未実装**（設計のみ）。本小節は §4.1 の課金表（既に規範として定義済み）の主要 charge point
> へ fuel 課金を揃える縦切り（F-2）の slice 定義である。§4.1〜4.2 の課金表・fuel 意味論が正本で
> あり、以下は F-2 で実装する範囲・観測挙動の変化・意図的に延期する逸脱の記録である。進捗の唯一の
> 正本は [`slices.toml`](slices.toml) と [ロードマップ](roadmap.md)。
>
> **依存（前段スライス）**: F-2 は、slice の yield を式の途中でも行える tree evaluator の continuation
> 拡張（実装スライス `tree-expr-continuation`）が **done** であることを前提にする。理由は本小節
> 「式の途中での slice yield は前段スライスで解決済みとする」を参照。`tree-expr-continuation` は
> F-2 とは独立に先行できるため、別スライスへ切り出して `depends_on` にした。

F-1 は VM の fuel を tree の**現行の**課金点（ループ back-edge 1・user function 呼び出し 1・builtin
callback 1）へ揃えて fuel parity を回復し、停止性を instruction watchdog（§17.7）へ分離した。ただし
F-1 の課金点と課金値は §4.1 の課金表そのものとは一致していない（F-1 小節「F-1 の範囲外」に明記）。
F-2 は、両 engine の fuel が一致した F-1 の状態を起点に、§4.1 の表のうち**主要 charge point**
（statement / expression / operation / function / loop / host）の課金点と課金値を §4.1 どおりへ移行する。

§4.1 の主要 charge point は F-1 の課金粒度より細かく、同じ script の消費 fuel が増える。したがって
F-2 は既定 `total_fuel` の見直しと、課金表の版である `fuel_accounting_revision`（§3.1）の bump を
含む（観測挙動が変わる）。これらは設計判断を要するため F-1 とは別 slice とする（F-1 小節で予告済み）。

- **課金点を §4.1 の主要 charge point どおりへ張り直す（tree/VM 両方）**: 下記の charge point を
  両 engine で同一の順序・同一の line で課金する。VM は §4.1 の要求どおり compiler が statement /
  expression / operation / function / loop の境界へ `Charge` opcode を挿入し、tree evaluator は
  同じ共有 `FuelSchedule` API を呼ぶ。対応は次のとおり。
  - **statement へ入る = 1**: 到達した statement の実行開始時。到達しない statement は課金しない。
  - **expression へ入る = 1**: literal・変数参照・call 式を含む各 expression の評価開始時。
  - **unary / binary / compare / index / assignment の論理 operation = 1**: operand の expression
    charge とは別に、その operation 1 個につき 1。
  - **language-level function invocation = 5**: user function・lambda・core builtin・callback で
    共通。F-1 の「呼び出し 1 課金」を 5 へ置き換える。line は呼び出し位置（F-1 と同じ）。
  - **loop iteration 開始 = 2**: condition・iterable・body の charge とは別に、1 反復につき 2。
    F-1 の back-edge 1 課金を、§4.1 の「iteration 開始 2」へ置き換える（課金のタイミングと値の
    両方が変わる。line はループ文の line で F-1 と同じ）。
  - **host call attempt = 10**: 許可・拒否・host error のいずれでも課金。tree の host-call 境界
    （filesystem builtin）にだけ現れる。VM は host function に対応しないため該当なし（F-1 と同じ）。
  - **VM `Charge` dispatch = 0 / その他 VM 内部 opcode = 0**: lowering 差を script の fuel 差に
    しないため、§4.1 どおり課金しない。compiler optimization で charge point を削除・併合しない。

- **既に §4.1 どおりで F-2 が変えない per-element 課金（再掲のみ）**: collection element の
  per-element fuel 課金（1 / element）が現状あるのは **比較・表示・`sort`** の経路だけである
  （`src/builtin_core.rs` の sort 走査課金＝`render_display` の per-node fuel 代替、`src/value.rs`
  の比較走査、`src/budget.rs` の表示課金）。これらは F-1 以前から fuel メーターへ課金しており §4.1
  と一致するため、F-2 は値も課金点も変えない。**`map` / `filter` / serialize の per-element 課金は
  §4.1 の規範だが未実装**（現状は callback 呼び出しごとの課金しかない）であり、UTF-8 payload・AST
  node と同じく後続スライス `fuel-bulk-charge` へ延期する（下記「逸脱」）。bulk work の 256 element
  / 16 KiB chunk 分割（§4.1）は既存経路でそのまま維持する。

- **変更箇所**（F-1 小節と同じ粒度で列挙）:
  - `src/budget.rs`: `BudgetConfig` に `fuel_accounting_revision: u32` を追加（既定 `1` → F-2 で
    `2`）。受理版の検査を `heap_accounting_revision` と同じ入口（`BudgetConfig::validate` ／
    execution 作成前）へ足す。現行の `ConfigError::UnsupportedAccountingRevision(u32)` は
    `heap_accounting_revision` 専用で、fuel と共用すると**どちらの版が不正か区別できない**。F-2 は
    **どの会計 field が不正かを区別できる形**にする（`UnsupportedHeapAccountingRevision(u32)` /
    `UnsupportedFuelAccountingRevision(u32)` へ分ける、または種別 enum を持たせる）。既存の
    `UnsupportedAccountingRevision` を参照する箇所（`src/embedding.rs` の doc・`src/budget.rs`
    `validate`・両者の unit test）を新形へ更新する。課金表の charge point / 課金値を一元提供する
    共有 API（仮称 `FuelSchedule`：statement=1 / expression=1 / operation=1 / invocation=5 /
    loop iteration=2 / host call=10 の定数と課金ヘルパ）を新設する。
  - `src/audit.rs` と [決定性・監査仕様](determinism-and-audit.md): `AuditEvent::ExecutionStarted`
    は `heap_accounting_revision: u32` と `budget: BudgetConfig` を持つ。`BudgetConfig` に
    `fuel_accounting_revision` を足すと監査 event の内容が変わるため、`ExecutionStarted` の field
    定義を更新する。**`heap_accounting_revision` と対称に、`ExecutionStarted` へ独立 field
    `fuel_accounting_revision: u32` を追加する**（`budget` の中だけで持たせる案は採らない。会計版は
    監査の第一級の属性で、heap と対称に top-level で引ける方が replay/検証時に扱いやすいため）。
    R-1a/B-2 と同じく opt-in・tree-only の監査経路のみに影響し、CBOR tag 表・charge table は変えない。
    determinism-and-audit.md の §7.2 `ExecutionStarted` の記述も `heap_accounting_revision` の隣へ
    `fuel_accounting_revision` を並べて更新する。
  - `src/compiler.rs`: statement / expression / operation / function / loop の各境界へ fuel 課金を
    encode する `Charge` opcode を挿入する。既存の最適化で charge point を削除・併合しない。
  - `src/opcode.rs` / `src/chunk.rs`: `Charge` opcode（§4.1 の「`Charge` opcode dispatch = 0、payload に
    論理 charge を encode」）を追加する。
  - `src/verifier.rs`: `Charge` payload（非負・既知の charge 種別）の検証を追加する。
  - `src/vm.rs`: F-1 で `OpCode::Loop`（back-edge）・`PrepareCall`・`call_fn_value` に置いた現行の
    `count_step` を外し、`Charge` opcode の dispatch で `FuelSchedule` を呼ぶ。instruction watchdog
    （§17.7、停止性）は fuel とは独立なので tick 機構はそのまま残す。
  - `src/eval.rs`（tree）: 各境界で `FuelSchedule` の課金ヘルパを呼ぶ。F-1 の `count_step` 呼び出し
    （5 か所）を新 API へ置き換える。式途中の yield は前段スライス `tree-expr-continuation` で
    入った continuation 機構に乗せる（F-2 は課金点を足すだけで continuation 自体は作らない）。
  - `unstable-bytecode` feature の公開面: `Charge` opcode を公開列挙へ追加するなど、既存の封印
    方針（REV-006）に沿って更新する。
  - docs: 本小節、§3.1（`fuel_accounting_revision` 追記済み）、roadmap。

- **既定 `total_fuel` の見直し（測定して決める）**: 課金が細かくなり、同じ script の消費 fuel が
  F-1 比で数倍〜十数倍に増える。現在の既定 1,000,000 fuel（§3.1）のままでは、F-1 まで通っていた
  代表 script の多くが上限に達する。F-2 は既定値を、代表 script 群が F-1 までと同程度に通る水準へ
  引き上げる。**具体値は実装時にベンチ（`benches/interpreter.rs` の workload と下記「代表 script
  集合」）で新課金の消費量を測って確定する**。設計では「旧挙動の代表 script が通る最小の切りの良い
  値を測定で決める」ことと測定対象（下記の固定集合）だけを固定し、本小節に数値を書かない（数値を
  先に決め打ちすると、実測とずれたとき spec/実装が drift する）。
  - **代表 script 集合（設計時点で固定・測定対象）**: `examples/*.tsg`（`hello` / `math` /
    `import_demo`）・`benches/interpreter.rs` の各 workload・F1-AT-04 相当の「2 文ループを既定上限
    近くまで回す script」と `fib(27)`。この集合が新課金・新既定 `total_fuel` で両 engine とも成功
    することを確認し、既定値決定の根拠（各 script の新消費 fuel）を実装 PR 本文へ記録する。
  - **根拠**: §4.2 は `total_fuel` を「terminal までの上限」と定め、具体値は config 事項として
    §3.1 の既定表に置く設計になっている。F-2 は課金粒度の変更に合わせて既定表の 1 値を更新する
    変更であり、新しい意味論を導入しない。

- **`fuel_accounting_revision` を 1 → 2 へ bump する（`language_revision` は触らない）**: F-2 は
  §4.1 の課金値へ移行する会計の変更であり、§3.1 で新設した `fuel_accounting_revision` を `1` から
  `2` へ上げ、受理版を `2` だけへ更新する。**旧版（`1`）の旧課金を再現する互換経路は持たない**
  （`fuel_accounting_revision = 1` は `ConfigError::UnsupportedAccountingRevision` で拒否されるので
  あって、旧課金で動くのではない）。`heap_accounting_revision` が §5.1 の論理サイズ表の版である
  のと完全に対称で、課金表を変えるたびにこの値だけを上げる。
  - **`language_revision` を使わない理由**: `language_revision`（REV-017 §17.8）は sort / `list_dir`
    の全順序化のように **script から観測できる言語挙動**の版であり、その doc は「0.20 以下は従来
    整列順を温存」と revision ごとの挙動温存を約束している。fuel 課金は言語挙動ではなく**資源会計**
    であり、§3.1 で既に `heap_accounting_revision` が会計版を言語 revision と分離する前例を確立して
    いる。fuel 課金の変更を `language_revision` bump に乗せると、(a) revision ごとに挙動を温存する
    約束と「旧 fuel 課金は温存しない」方針が食い違い、(b) 言語挙動と会計という別種の変更が 1 つの
    版番号に混ざる。そこで F-2 は `language_revision` を **bump しない**。この方針（fuel は会計
    revision で管理し言語 revision で分岐しない）は §3.1 の `fuel_accounting_revision` 定義にも
    明記した。
  - **根拠**: §3.1 の `heap_accounting_revision` が「会計表の版を専用 field で持ち、受理しない版を
    execution 作成前に拒否する」型を確立済み。F-2 はその型に fuel 用の版 field を 1 つ対称に足す
    だけで、新しい revision 機構を導入しない。旧版を温存しない判断は alpha の後方互換無保証
    （[ロードマップ](roadmap.md)・README の alpha 注記）に基づく。

- **式の途中での slice yield は前段スライスで解決済みとする**: 課金点が expression / operation 単位
  になると、式の評価の途中で slice fuel を使い切りうる。§4.2 は「次の論理 charge が slice 残量を
  超える場合は charge せず `Yielded(SliceFuelExhausted)` を返す」と定めるため、式の途中でも yield /
  resume できる必要がある。現行 tree evaluator は文・反復の粒度でしか continuation を持たず（式は
  `eval_expr` の Rust 再帰）、§9.3 が求める「保留中 operand を保存し Rust call stack を continuation
  にしない」を式へ適用できていない。この拡張は F-2 の課金点張り直しとは独立に先行でき、かつ重いので、
  **前段スライス `tree-expr-continuation` として切り出し、F-2 の `depends_on` に置く**（本小節冒頭の
  依存注記）。F-2 は `tree-expr-continuation` が done である前提で、新しい charge point を既存の
  continuation 機構に乗せるだけとし、continuation 自体は設計・実装しない。

- **観測される変化**: 同じ script の `committed.fuel` が §4.1 の主要 charge point どおりに増える。
  F-1 まで既定上限で通っていた script の一部が、既定 `total_fuel` を引き上げてもなお新課金で上限に
  達しうる（上限到達 script の集合と、上限到達時の line が変わる）。tree・VM の両方で同じ値・同じ
  成否・同じ line になる（F-1 の exact parity を維持する）。watchdog（停止性の安全網、§17.7）は
  fuel とは独立なので F-2 で意味論を変えない。ただし watchdog の上限 `total_fuel × 2 + 16` は既定
  `total_fuel` に追従するため、既定値を大きく上げると VM が暴走を止めるまでに許す instruction 数も
  同じ倍率で増える（停止はするが到達までが延びる）。これは資源会計の精度を上げるための許容範囲と
  する（停止性自体は保たれる）。

- **逸脱（F-2 時点で残るもの、後続スライスへ延期）**:
  - **UTF-8 payload（`ceil(bytes/64)`）と AST node（1 / node、compile・import link 中）の fuel
    課金**は F-2 の範囲外とし、後続スライス（`fuel-bulk-charge` 仮称）へ延期する。現状これらは
    fuel メーターへ課金されておらず（`src/budget.rs` 冒頭の注記「fuel の全 charge point 展開…は
    Slice 2 の残りサブスライス」）、新規に課金点を足す作業になるため、主要 charge point の移行とは
    別スライスに分ける。延期中も §4.1 の表はこれらを規範として残す（実装が表に追いつく形）。
  - **VM の deadline clock 注入・VM audit・VM cooperative 実行**は F-1 と同じく範囲外（既存の後続
    項目）。
  - **heap / string / count の課金**（§5）は F-2 の対象外。F-2 は fuel メーターだけを扱う。
  - **`PollSlice::max_fuel`（slice fuel）の既定値**（§4.2 の公平性の量子）は F-2 で変えない。
    F-2 が見直すのは `total_fuel`（terminal までの上限）だけ。

- **受入基準（F-2）**:
  - **F2-AT-01（主要 charge point が §4.1 どおり・tree/VM 一致）**: statement・expression・各論理
    operation・function invocation（5）・loop iteration（2）を含む代表 script 群で、両 engine の
    必要 fuel が §4.1 の表から手計算した期待値と一致する。tree は成功時に `committed.fuel` を 0 で
    snapshot する（入力ごとに reset）ため、usage の直接比較ではなく **F-1 と同じく「成功する最小の
    fuel 上限 N を二分探索で求めて exact 突合」**する（`tests/differential.rs` の `assert_fuel_parity`
    を新課金値へ更新）。**host call（10）は VM が host function に対応しないため tree/VM 一致検査に
    含めない**（下記 F2-AT-01b で tree 単独で検査する）。
  - **F2-AT-01b（host call = 10・tree 単独）**: filesystem builtin（`read_file` / `read_lines` /
    `write_file` / `append_file`）の host call 1 回につき 10 fuel 課金される。**builtin を呼ぶ文を
    1 つだけ増やした** 2 script（例: 同じ `read_file(path)` 文が 1 行 vs 2 行。path 引数は同一リテラル）
    の必要 fuel N の差を取る。増やした 1 文の内訳は host call 10 + builtin invocation 5 + その文の
    statement 1 + call 式の expression 1 + 引数リテラルの expression 1 = **18**（path 引数が変数参照
    1 つの単純形の場合。実際の期待値は選んだ script の式構造から §4.1 の表で手計算し、テストに定数で
    埋める）。この差のうち host call 由来が 10 であることを、invocation のみの差分（F2-AT-02）と
    突き合わせて確認する（差 − invocation 5 − statement/expression 分 = 10）。許可（Allow）・
    拒否（Denied）・host error の **3 経路それぞれ**で host call 10 が課金されることを別々に検査する
    （§4.1「許可・拒否・host error のいずれでも課金」。Denied は capability 未付与、host error は
    存在しないパス等で起こす）。tree 単独で、VM は対象外。
  - **F2-AT-02（function invocation = 5 の差分検査）**: 本体が同一で**呼び出し回数だけが 1 回と 2 回で
    異なる 2 本の script**について、成功する最小の fuel 上限 N の差がちょうど 5 になる（§4.1 の
    invocation = 5）。両 engine で一致する。1 回呼ぶ script の絶対 N を上限に使う検査（旧 F2-AT-02 の
    「上限 5 で成功」）は、statement / 変数参照 / 本体の課金が別途乗り成立しないため採らない。
  - **F2-AT-03（loop iteration = 2 の差分検査）**: body が同一で**反復回数だけが異なる**2 本の script
    について、成功する最小の fuel 上限 N の差が「反復回数差 × (2 + 1 反復分の condition / iterable
    評価の charge + body 1 反復分の charge)」になり、うち iteration 開始分が反復ごとに 2 である
    （`while` は condition の expression / operation charge、`for` は iterable 要素取り出しの charge が
    反復ごとに別途乗る）。`while` / `for` / 入れ子 / `continue` / `break` で両 engine の必要 fuel が
    一致する（F-1 の `continue` 二重課金回避を維持）。
  - **F2-AT-04（境界）**: F2-AT-01〜03 の各 script で、上限 N = 必要 fuel では両 engine とも成功、
    N − 1 では両 engine とも上限で失敗し、stdout・stderr（line・trace を含む）・exit code が一致する。
  - **F2-AT-05（新既定 total_fuel）**: 更新後の既定 `total_fuel` で、上記「代表 script 集合」が
    両 engine で成功する。既定値の決定根拠（各 script の新消費 fuel）が実装 PR 本文に記録される。
  - **F2-AT-06（fuel 会計 revision = 2）**: `BudgetConfig::standard` の `fuel_accounting_revision`
    が `2`。`2` を指定した実行と既定の実行が同じ fuel・同じ成否になる。`fuel_accounting_revision = 1`
    （旧版）を指定すると execution 作成前に `ConfigError::UnsupportedAccountingRevision` で拒否される
    （旧課金で動くのではない）。`language_revision` は F-2 前後で変わらない。
  - **F2-AT-07（watchdog 非干渉）**: 課金粒度を細かくしても、`tests/defensive_vm.rs` の停止性ケース
    （`Jump(0)` 自己ループ・後方 `Loop` 迂回・raw `Call` 再帰）が従来どおり `limit` で有限停止する。
    新課金でも watchdog 上限に達する前に fuel 上限へ達する（F1-AT-07 の不変条件を新課金で再確認）。
  - **F2-AT-08（既存テストの更新）**: VM/tree の fuel 期待値を持つ既存テスト（**F1-AT-04/05 を含む**
    F1-AT-* の N 値・differential の fuel 比較・integration の境界）を §4.1 の新課金値へ F-2 で
    再定義し、両 engine で緑。特に F1-AT-04（既定上限での 2 文ループ境界・fib(27)）と F1-AT-05
    （呼び出し = ちょうど 1 fuel）は新既定 `total_fuel`・invocation = 5 に合わせて数値を更新する。
    期待値が説明できない形で変わる場合は実装を止めて報告する。

- **テスト配置**: exact fuel 比較（F2-AT-01/02/03）は `tests/differential.rs`、host call tree 単独
  （F2-AT-01b）と境界・既定上限・revision（F2-AT-04/05/06）は `tests/integration.rs`、watchdog 非干渉
  （F2-AT-07）は `tests/defensive_vm.rs`、課金点挿入・`Charge` payload・`FuelSchedule` 定数・
  `ConfigError` の会計 field 区別の単体不変条件は `src/vm.rs` / `src/compiler.rs` / `src/budget.rs`
  の unit test。（F2-AT-01b は F2-AT-01 の host call 部分を tree 単独へ切り出した補助基準で、
  `slices.toml` の `acceptance` に明示する。）

- **スライス分割の指針**: F-2 は「主要 charge point の張り直し（実装が重い）」と「既定 `total_fuel`
  見直し ＋ `fuel_accounting_revision` bump」を 1 PR に載せる前提で設計した。`tree-expr-continuation`
  （式途中 yield）と `fuel-bulk-charge`（UTF-8・AST node 課金）は既に別スライスへ切り出してあるため、
  F-2 本体は主要 charge point の移行に絞られている。それでも実装着手時に 1 PR が過大と判断した場合は
  更に分割してよいが、`fuel_accounting_revision` の bump と既定 `total_fuel` の更新は**同一 PR で
  行う**（会計版と既定値を別 PR に割ると、中間状態で「新課金なのに旧既定上限」で代表 script が落ちる
  不整合が main に出るため）。分割する場合は `slices.toml` を更新し、本小節を割る。

#### F-2 の sub-slice 分割（F-2a / F-2b / F-2c）

> 状態: **未実装**（設計のみ。2026-11-21 の設計 PR #470 で割り方を決めた。判断の記録は本小節の末尾）。
> 各 sub-slice の進捗は [`slices.toml`](slices.toml) の `F-2a` / `F-2b` / `F-2c` を正とする。
> 上の「F-2 実装スライス」小節（PR #241 の時点の記録）は、そのままでは [開発ループ手順書](loop-runbook.md)
> §14.1 のスライスの大きさに収まらないため、`ready` にせず sub-slice に割る（documentation-guide「ready の
> 条件」の 4）。課金点・課金値・観測挙動・延期する逸脱の内容は上の小節のとおりで、ここでは割り方・各
> sub-slice の範囲・受入基準・想定の変更規模と、上の小節の記述を現状に合わせて読むための注記だけを書く。
> `slices.toml` の F-2 は sub-slice のエントリに置き換えた（元の id は残さない。「F-2 が done」は F-2a・F-2b・F-2c が
> ともに done を意味する）。

**§14.1 に収まらない理由**（main 7d46a4f で確かめた）:

- 受入基準が 9 個（F2-AT-01..08 と F2-AT-01b）で、上限の 5 個を超える。
- 触る仕組みが 2 つある。(1) budget の config と監査の event（`fuel_accounting_revision` の field・config error・
  `AuditEvent::ExecutionStarted` と strict CBOR の `BudgetConfig` の encode、`src/budget.rs`・`src/embedding.rs`・
  `src/audit.rs`）、(2) fuel の課金点（両 engine）。(2) は VM の bytecode（`Charge` opcode・compiler の挿入・verifier・
  `unstable-bytecode` の公開面、`src/opcode.rs`・`src/chunk.rs`・`src/compiler.rs`・`src/verifier.rs`・`src/vm.rs`）と
  tree evaluator（`src/eval.rs` の `eval_expr` の再帰と式 frame（`FrameKind::Expr`）の段階評価の両方、`src/builtin.rs`）に
  またがるが、F-1 と同じく 1 つの仕組みと数える（片方の engine だけを変えると F-1 の exact parity が main で崩れるため、
  engine ごとには分けられない）。
- 規模: 全体で `src/` 約 1050〜1250 行（下の F-2a・F-2b・F-2c の見積もりの和）の見込みで、§14.1 の 1000 行を超える。
- 現状の課金点は tree の `count_step`（`src/eval.rs` に 7 か所、`src/builtin.rs` に 1 か所）と VM の `count_step`
  （`src/vm.rs` に 3 か所）だけで、statement / expression / operation の課金点は両 engine に無い。式 frame
  （tree-expr-continuation e2a〜e4 で入った）の上で expression / operation の課金順を `eval_expr` の再帰と VM の
  両方に一致させる作業が加わるため、上の小節が想定した 1 PR の大きさを超える見込みである。

**上の小節を現状に合わせて読むための注記**（上の小節は書き換えない）:

- 依存の `tree-expr-continuation` は e1〜e4 に割られ、すべて done である（`slices.toml` の F-2 の `depends_on` は
  e4）。callback なし bulk builtin の chunk 境界の yield は fuel-bulk-chunk-yield（done）で入った。
- 式の途中の課金で slice 残量を使い切った場合の扱いは、§4.2「yield 可能点に挟まれた区間の slice 超過」に
  従う。呼び出しも反復も含まない部分式は式 frame を積まないので、その区間の expression / operation の課金は
  区間を抜けた最初の yield 可能点まで slice を超えてよい（区間の AST node 数で有界）。新しい yield 可能点は作らない。
- 「List / Dict の深度の読み直し」（§4.1）は版 `1` に含まれる（§3.1）。F-2 はこの課金を変えない。
- 上の小節の変更箇所の `src/vm.rs` の箇条（`call_fn_value` の `count_step` を外して `Charge` opcode の dispatch で
  課金する）は、builtin callback については Q5 = A で読み替える。callback の課金は `Charge` にせず、`Vm::call_fn_value`
  から `FuelSchedule` を直接呼ぶ（F-2b・F-2c とも。下の判断の記録）。
- 上の小節の F2-AT-06 は旧版の拒否を `ConfigError::UnsupportedAccountingRevision` と書くが、同じ小節の変更箇所と
  §3.1 は heap と fuel のどちらの版が不正かを区別できる形にするとする。区別の形は Q3 = A（variant を 2 つに
  分ける。下の判断の記録）で、受入基準はその形で書く。
- `BudgetConfig` と `AuditEvent::ExecutionStarted` に field を足すと、strict CBOR（[決定性・監査仕様](determinism-and-audit.md)
  §9.2、audit-strict-cbor-c1 で encoder 本体が done・未配線）の payload が変わる。同仕様は「unknown schema
  field を末尾へ追加せず、schema version を上げる」とする。扱いは Q4 = A（schema version を 2 へ上げる。下の判断の記録）。
- 上の小節の F2-AT-01b の例（増やした 1 文の内訳 18）は、path 引数を「同一リテラル」と「変数参照」の両方で
  書いている。どちらでも引数 1 つは expression 1 なので値は変わらないが、受入基準は選んだ script から §4.1 の
  表で手計算した値をテストに定数で書く（上の小節の指示どおり）。

**割り方（判断 Q1 = A）**: 観測挙動を変えない土台を先に入れ、課金表の移行（版の bump と既定
`total_fuel` の更新を含む）を最後の 1 本にまとめる。版の bump と既定 `total_fuel` の更新を同じ PR で行う
上の小節の制約（「スライス分割の指針」）は F-2c で守る。依存は F-2a → F-2b → F-2c の直列。
`fuel-bulk-charge` の `depends_on` は F-2c に付け替える。

##### F-2a 実装スライス（会計版の field と config error の区別、版は `1` のまま）

- **範囲**: `BudgetConfig` に `fuel_accounting_revision: u32`（§3 の宣言順。`BudgetConfig::standard` は `1`）を
  足し、`BudgetConfig::validate`（execution 作成前）で `1` 以外を拒否する。会計版の不正を heap と fuel で
  区別できる config error にする（Q3 = A）: `ConfigError::UnsupportedAccountingRevision(u32)` を消し、
  `UnsupportedHeapAccountingRevision(u32)` と `UnsupportedFuelAccountingRevision(u32)` に分ける（値は指定された版）。
  検査は `heap_accounting_revision` を先に行う。`AuditEvent::ExecutionStarted` に `heap_accounting_revision` の
  隣で `fuel_accounting_revision: u32` を足し（上の小節の決定どおり top-level の独立 field）、strict CBOR の
  `BudgetConfig`・`ExecutionStarted` の encode と size pass を更新する。field はどちらも `heap_accounting_revision`
  の直後に置き、audit の `schema_version` を `1` から `2` に上げる（Q4 = A。`AuditEnvelope` を作る `src/embedding.rs`
  と strict CBOR の size pass の定数を含む）。
  [決定性・監査仕様](determinism-and-audit.md) §7.2 と §9.2 の該当の記述、`src/embedding.rs` の doc を合わせる。
- **範囲外**: 課金点・課金値の変更（F-2c）、`FuelSchedule`・`Charge` opcode（F-2b）、既定 `total_fuel`。
  fuel の値はどの script でも変わらない。
- **受入基準**:
  - **F2a-AT-01**: `BudgetConfig::standard` の `fuel_accounting_revision` は `1`。`1` を明示した実行と既定の実行が
    同じ fuel・同じ成否になる。`language_revision` は変わらない（`src/budget.rs` の unit test と `tests/integration.rs`）。
  - **F2a-AT-02**: `fuel_accounting_revision = 2`（未受理の版）と `heap_accounting_revision = 2` をそれぞれ
    指定すると、execution 作成前にそれぞれ `UnsupportedFuelAccountingRevision(2)` / `UnsupportedHeapAccountingRevision(2)`
    で拒否される。両方が不正なときは `UnsupportedHeapAccountingRevision` を返す（`src/budget.rs`・`src/embedding.rs` の unit test）。
  - **F2a-AT-03**: 監査を有効にした実行の `AuditEnvelope` が `schema_version = 2`、`ExecutionStarted` が
    `fuel_accounting_revision = 1` を持ち、strict CBOR の golden（SC-AT-02 の該当分）が `heap_accounting_revision`
    の直後に `fuel_accounting_revision` を置いた field 順で一致する（`src/audit.rs` の unit test）。
- **想定の変更規模**: `src/` 約 150 行（`#[cfg(test)]` を除く）、触る仕組みは budget の config と監査の event の
  field 1 つ（同じ値を config と監査へ流す 1 つの経路として数える）、受入基準 3。

##### F-2b 実装スライス（`FuelSchedule` と `Charge` opcode の土台、課金点・課金値は版 `1` のまま）

- **範囲**: §4.1 の charge point の種別と課金値を一元に持つ共有 API `FuelSchedule`（`src/budget.rs`）を新設する。
  版 `1` の課金（ループ反復 1・呼び出し 1・builtin callback 1）を `FuelSchedule` の種別で表し、tree の
  `count_step`（`src/eval.rs`・`src/builtin.rs`）をこの API の呼び出しへ置き換える。VM は `Charge` opcode
  （payload は charge 種別。`src/opcode.rs`・`src/chunk.rs`）を足し、F-1 で `OpCode::Loop`・`PrepareCall` に置いた
  `count_step` を外して、compiler が同じ位置に `Charge` を挿入する。builtin callback の課金（`Vm::call_fn_value`）は
  `Charge` にせず、`call_fn_value` から `FuelSchedule` の callback 種別を直接呼ぶ。位置と、深度検査・callable 検査・
  arity 検査より前に課金する順序は今と同じにする（Q5 = A。下の判断の記録）。verifier は payload
  が既知の種別であることを検査する。`unstable-bytecode` の公開列挙を REV-006 の封印方針に沿って更新する。
  instruction watchdog（§17.7）の tick は変えない。
- **範囲外**: 課金点の追加・課金値の変更と版の bump（F-2c）。`Charge` の dispatch 自体は課金しない（§4.1）。
- **受入基準**:
  - **F2b-AT-01**: 既存の fuel の期待値（F1-AT-*・`tests/differential.rs` の `assert_fuel_parity`・integration の
    境界）が無改変で緑。両 engine の必要 fuel・上限到達の line が F-2b の前後で変わらない。
  - **F2b-AT-02**: compiler がループ反復・呼び出しの位置に `Charge` を出し、最適化で削除・併合しない。verifier は
    bytecode に出てはならない種別（builtin callback・host function 呼び出し）の `Charge` を拒否する（Q2 = A。
    F-2b の設計 PR #474 で「未知の種別」から書き直した）（`src/compiler.rs`・`src/verifier.rs` の unit test）。
  - **F2b-AT-03**: `tests/defensive_vm.rs` の停止性ケース（`Jump(0)` 自己ループ・後方 `Loop` 迂回・raw `Call`
    再帰）が従来どおり有限停止する。
- **想定の変更規模**: `src/` 約 350 行、触る仕組みは fuel の課金点 1 つ（両 engine の課金を同じ API へ寄せる）、
  受入基準 3。

**ready にする時点の注記**（F-2b の設計 PR #474。main d1c7d02 で確かめた。上の範囲は変えず、受入基準は F2b-AT-02
だけを Q2 = A に合わせて書き直した。ほかは実装で読み違えやすい点だけを補う）:

- 置き換える課金点は、tree が `src/eval.rs` の `count_step` 呼び出し 7 か所（`while` / `for` の反復 2・user function
  呼び出し 1（同期経路と式 frame が共有する `begin_user_call`）・host function 呼び出し 4（`prepare_call` の
  host 経路、式 frame の `CallTarget::Host`・`CallTarget::Cooperative`、`eval_cooperative_call`））と `src/builtin.rs` の callback 前処理 1 か所、VM が `OpCode::Loop`・`OpCode::PrepareCall`・
  `Vm::call_fn_value` の 3 か所である。tree の host function 呼び出し（embedder が登録した関数）は版 `1` では
  user function 呼び出しと同じ 1 を課金する。`FuelSchedule` では別の種別にする（Q1 = A。下の判断の記録）。
- compiler は `Loop` を出すすべての位置（ループ末尾の back-edge と、`while` の `continue` が出す `Loop`）の直前に、
  その `Loop` と同じ line（ループ文の行）で `Charge` を出す。`PrepareCall` についても直前に同じ line で出す。どの
  jump も `Charge` と `Loop` / `PrepareCall` の間へ飛び込まない（`Charge` を飛ばすと F2b-AT-01 の観測が変わる）。
- `Loop`・`PrepareCall` は課金しなくなる。`PrepareCall` の深度検査はそのまま残す。手組みの bytecode で `Charge` を
  省いた `Loop` は fuel を課金せず、instruction watchdog の (W1) で止まる（`Jump(0)` と同じ扱い。停止性は watchdog が
  担う、[次期意味論・実装決定](semantic-decisions.md) §17.7）。`tests/defensive_vm.rs` の `backward_loop_halts_with_limit`
  は `limit` での停止を確かめるだけなので assert は変わらないが、「`Loop` は fuel を課金する」という doc comment は
  直す。正規 compiler 出力の「watchdog の tick 数は fuel 課金数 + root frame 数を超えない」不変条件は、各 `Loop` の
  直前に `Charge` があることで保たれる。
- 文書: [次期意味論・実装決定](semantic-decisions.md) §17.7.3 の fuel の論理課金点 (F1)・(F2) の記述（`Loop` 命令・
  `PrepareCall` で課金）を「直前の `Charge`」に合わせ、本仕様 §4.1 の後の段落・`src/vm.rs` の `count_step` の doc を
  合わせる。逸脱一覧の §4.1 の行は F-2c まで残す（F-2b の後も課金は版 `1` のまま）。
- `Charge` の payload は `FuelSchedule` と共有する Rust の enum にする（Q2 = A。下の判断の記録）。上の範囲の
  「verifier は payload が既知の種別であることを検査する」は、Q2 = A の enum では既知であることが型で決まるため、
  F2b-AT-02 と同じく「bytecode に出てよい種別（ループ反復・user function 呼び出し）以外の `Charge`（builtin
  callback・host function 呼び出し）を拒否する」と読む。

**判断の記録**（F-2b の設計 PR #474 の判断の候補。判断役が選択（PR #474）、Q1・Q2=A）:

- **Q1 = A（`FuelSchedule` の種別を今の課金点の種類ごとに 4 つにする）**: ループ反復・user function 呼び出し・
  host function 呼び出し・builtin callback の 4 種別で、版 `1` ではどれも 1。tree の host function の課金 4 か所は
  user function の課金（`begin_user_call`）と別の位置にある。§4.1 は invocation 5 と host call attempt 10 を別の
  charge point とし、embedder が登録した host function をどちらに数えるかは spec に無い。A は種別を今の課金点に
  1 対 1 で付けるだけなので、F-2b がその未決の対応を先に決めず、F-2c は値を変えるだけで済む。版 `1` の値は
  どれも 1 なので、F2b-AT-01 の観測不変は既存のテストが無改変で緑であることで確かめられる（[マニフェスト](manifesto.md)
  原則 5・9）。host function を invocation と host call のどちらに数えるかはこの判断では決めず、F-2c で決める。
  採らなかった案: B（user function と host function を 1 つの「呼び出し」種別にまとめる）は、「host function は
  invocation と同じ」という対応を F-2b が暗に先取りし、F-2c で分けるときに課金点を探し直して種別を付け直す変更が
  増える（原則 8・9）。
- **Q2 = A（`Charge` の payload は `FuelSchedule` と共有する Rust の enum、verifier は bytecode に出てはならない
  種別を拒否する）**: 既存の opcode の payload は型付き（`CallBuiltin(BuiltinId, usize)`・`SetIndex(MutationTarget)`
  など）で、bytecode の直列化は無いので、整数コードにする理由になる読み込み経路が無い。enum なら存在しない種別を
  作れず、VM の dispatch に整数からの変換と再検査の分岐が要らない（原則 8）。verifier が builtin callback・host
  function 呼び出しの種別の `Charge` を拒否するので、手組み bytecode での二重課金や、Q5 = A（callback は
  `call_fn_value` から直接課金）と食い違う課金経路が gate で止まる（原則 5、[次期意味論・実装決定](semantic-decisions.md)
  §17.7 層2）。bytecode に出てよいのはループ反復と user function 呼び出しの 2 種別である。F2b-AT-02 の「未知の種別」は
  この意味に書き直した。確かめる中身（verifier が不正な `Charge` を拒否する unit test）は減らない（原則 9）。
  採らなかった案: B（整数の種別コードにし、verifier が既知のコードかを検査する）は、受入基準の文面を変えないことだけが
  利点で、dispatch 側に変換と防御的な再検査を足し、既存の opcode の形ともそろわない（原則 8）。

##### F-2c 実装スライス（§4.1 の主要 charge point への移行・`fuel_accounting_revision` 1 → 2・既定 `total_fuel`）

- **範囲**: 上の小節の「課金点を §4.1 の主要 charge point どおりへ張り直す」「既定 `total_fuel` の見直し」
  「`fuel_accounting_revision` を 1 → 2 へ bump する」のとおり。F-2b の `FuelSchedule` に statement 1・
  expression 1・operation 1・invocation 5・loop iteration 2・host call 10 を載せ、tree（`eval_expr` の再帰と
  式 frame の段階評価の両方）と VM（compiler の `Charge` 挿入）で同じ順序・同じ line で課金する。ただし builtin
  callback の invocation（5）は、F-2b と同じく `Vm::call_fn_value` から `FuelSchedule` を直接呼ぶ（Q5 = A）。受理する版を
  `2` だけにし、`BudgetConfig::standard` を `2` にする。既定 `total_fuel` は上の小節の代表 script 集合で測って
  決め、§3.1 の表を更新する。
- **範囲外**: 上の小節の「逸脱」のとおり（UTF-8 payload・AST node・`map` / `filter` / serialize の per-element は
  `fuel-bulk-charge`、heap / string / count、`PollSlice::max_fuel`、VM の deadline・audit・cooperative）。
- **受入基準**（上の小節の 9 個を 5 個にまとめる。Q2 = A）:
  - **F2c-AT-01**（旧 F2-AT-01・02・03）: §4.1 の表から手計算した必要 fuel と、両 engine の「成功する最小の
    上限 N」（二分探索）が一致する。invocation は呼び出し回数だけが違う 2 本の N の差が 5、loop iteration は
    反復回数だけが違う 2 本の差が反復数差 ×（2 + 1 反復分の condition / iterable と body の課金）になる。
    `while` / `for` / 入れ子 / `continue` / `break` を含む（`tests/differential.rs`）。
  - **F2c-AT-02**（旧 F2-AT-01b）: filesystem builtin の host call 1 回につき 10。許可・拒否・host error の 3 経路
    それぞれで、呼ぶ文を 1 つ増やした 2 本の N の差から確かめる（tree 単独、`tests/integration.rs`）。
  - **F2c-AT-03**（旧 F2-AT-04）: F2c-AT-01・02 の各 script で N は両 engine とも成功、N − 1 は両 engine とも上限で
    失敗し、stdout・stderr（line・trace を含む）・exit code が一致する。
  - **F2c-AT-04**（旧 F2-AT-05・06）: `BudgetConfig::standard` の `fuel_accounting_revision` は `2` で、`1` は
    execution 作成前に `UnsupportedFuelAccountingRevision(1)` で拒否される。新しい既定 `total_fuel` で代表 script 集合が両
    engine で成功し、各 script の新しい消費 fuel を実装 PR 本文に記録する。`language_revision` は変わらない。
  - **F2c-AT-05**（旧 F2-AT-07・08）: watchdog の停止性ケースが有限停止し、fuel 上限が watchdog 上限より先に
    来る。fuel の期待値を持つ既存テスト（F1-AT-04/05 を含む）を新しい課金値へ更新し、両 engine で緑。期待値が
    §4.1 の表で説明できない形で変わるなら実装を止めて報告する。
- **想定の変更規模**: `src/` 約 550〜750 行（compiler の挿入 約 150、VM 約 50、tree の再帰と式 frame の両方の
  課金点 約 300〜450、budget の既定値・版 約 50）、触る仕組みは fuel の課金点 1 つ、受入基準 5。
  1000 行を超える見込みになったら、実装に入る前に止めて報告する（§14.1）。

**ready にする時点の注記**（F-2c の設計 PR #483。main 722bec0 で確かめた。上の範囲・範囲外・受入基準は変えず、
実装で読み違えやすい点と、spec に無いため判断の候補にした点（PR #483 の Q1〜Q3）を書く）:

- 依存の F-2b は done（実装 PR #477）。今の課金は `charge(ChargeKind)`（tree は `src/eval.rs` の `Evaluator::charge`、
  VM は `src/vm.rs` の `Vm::charge`）が `FuelSchedule::cost` を引く形で、種別は `LoopIteration`・`UserCall`・`HostCall`・
  `BuiltinCallback` の 4 つ（版 `1` ではどれも 1）。課金点は、tree が `src/eval.rs` の 7 か所（`while` / `for` の反復 2・
  `begin_user_call` 1・登録 host function 4）と `src/builtin.rs` の `callback_precheck` 1 か所、VM が compiler の
  `Charge`（`LoopIteration` を `while` の back-edge・`for` の back-edge・`while` の `continue` の 3 か所、`UserCall` を 2 か所）と
  `Vm::call_fn_value` の `BuiltinCallback` である。上の小節の「`count_step`」はこれらの `charge` の呼び出しと読む。
  F-2c は種別を §4.1 の charge point（statement・expression・operation・invocation・loop iteration・host call attempt）に
  合わせて付け直し、`ChargeKind::allowed_in_bytecode` も合わせる（callback の invocation は今と同じく
  `Vm::call_fn_value` から直接課金する。Q5 = A）。
- core builtin の呼び出しには版 `1` の課金点が無く、invocation（5）は F-2c で新しく足す。VM の compiler は呼び出しを
  user 経路と builtin 経路に実行時に分ける（`src/compiler.rs` の `user_jump`）ので、builtin 経路にも課金を出す。
- 上の小節は host call attempt（10）を「tree の host-call 境界（filesystem builtin）にだけ現れる。VM は host function に
  対応しないため該当なし」とするが、今は VM も filesystem builtin 14 個を tree と同じ論理位置で host call として数える
  （fix-fs-builtin-host-call-charge。`src/vm.rs` の `exec_builtin`）。filesystem builtin の host call attempt は両 engine で
  同じ位置・同じ値で課金し（上の小節の「両 engine で同一の順序・同一の line で課金する」）、F2c-AT-02 の 3 経路は
  filesystem builtin について両 engine で確かめ、F2c-AT-03 の境界にも含める。VM に無いのは embedder が登録した host
  function だけで、その課金は tree 単独で確かめる。
- host call attempt（10）を課金する呼び出しの範囲（上の小節は filesystem builtin だけを挙げるが、§2 は clock・env・process
  の呼び出しを、§6.1 は stdio と任意の host call を host call とする）と、登録 host function を invocation と host call の
  どちらに数えるか（F-2b の判断 Q1 で F-2c に回した）は spec に無い。判断の候補 Q1・Q2 にした（Q1・Q2 = A。下の判断の記録）。
- §4.1 は charge point の表だけを持ち、AST のどのノードを expression・operation に数えるか、operation と loop iteration を
  いつ課金するかを決めていない。成功する最小の上限 N は課金の順序に依らないが、`and` / `or` の短絡の数え方は N を変え、
  上限 N − 1 で止まる位置（それより前に出た stdout）は順序で変わる。F2c-AT-03 は両 engine の stdout の一致を求めるので、
  両 engine が従う規則を spec に置く。判断の候補 Q3 にした（Q3 = A。下の判断の記録）。
- 式は AST に line を持たない（`src/ast.rs` の `Expr`）。expression・operation・invocation・host call attempt の課金の line は
  その式を含む文の line（関数本体・lambda 本体の中では、その中の文の line）で、両 engine で同じにする。今の課金と同じ扱いである。
- 既存の per-element 課金（比較・表示・`sort`）、List / Dict の深度の読み直し（§4.1）、`for` の反復列の heap 課金
  （fix-for-iter-snapshot-charge）は変えない。
- watchdog（[次期意味論・実装決定](semantic-decisions.md) §17.7）の上限 `total_fuel × 2 + 16` は既定 `total_fuel` に追従する。
  課金点が増えても正規 compiler 出力ではループの各周回と各呼び出しに fuel の課金が 1 以上あるので、fuel 上限が watchdog
  上限より先に来ることは F2c-AT-05 で確かめる。
- 文書: 実装 PR で、§3.1 の表（`fuel_accounting_revision` を `2`、`total_fuel` を測った値）、§3.1 の本文の「初期版は `1`」の
  後の現在版の記述、[次期意味論・実装決定](semantic-decisions.md) §17.7.3 の fuel の論理課金点 (F1)・(F2)、`src/budget.rs` の
  `ChargeKind`・`FuelSchedule` の doc を合わせ、冒頭の未実装範囲の §4.1 の行を消す（UTF-8 payload・AST node・`map` /
  `filter` / serialize の per-element は `fuel-bulk-charge` の行として残す）。登録 host function と `env`・`now`・`exit` が
  host-call の回数・byte を数えない逸脱一覧の 2 行は fuel ではない（§6.1 の counter）ので F-2c では消さない。
- 想定の変更規模（見直し）: Q1 で host call attempt の課金点が増える分（stdio・`env` / `now` / `exit`・登録 host function、
  両 engine で 約 50 行）を足し、`src/` 約 600〜800 行。§14.1 の 1000 行に収まる。触る仕組みは fuel の課金点 1 つ、受入基準 5。

**判断の記録**（F-2c の設計 PR #483 の判断の候補。判断役が選択（PR #483）、Q1〜Q3=A）:

- **Q1 = A（host call attempt 10 は目標の章が host call とする呼び出しすべてに課金する）**: 対象は filesystem builtin
  14 個、stdio（`print`・`input`）、`env`・`now`・`exit`、登録 host function（同期・cooperative）。§2 の用語は host call を
  「capability 境界を越える clock、env、stdio、filesystem、process、登録 host function 等の呼出し」とし、§6.1 は stdio も
  任意の host call も host call とし、§4.1 は host call attempt を種類で分けていない。版 `2` の課金表を目標の章の定義どおりに
  固定でき、host へ出る呼び出しの費用が経路で変わらない（[マニフェスト](manifesto.md) 原則 3・5）。課金の位置は次のとおり。
  - filesystem builtin と `print`: 今の回数の課金（`charge_host_call_request`・`charge_output`）の直前で、
    内容の描画の後（§6.1 の「描画が上限を超えたら operation 未開始」と合う）。
  - `input`: 今の `charge_input` は `resolve_input`（capability 判定と host からの読み取り）の後にあるので、その直前では
    拒否と host error の経路で課金されない。host call attempt は `resolve_input` より前に置く（tree は `src/builtin.rs`、
    VM は `src/vm.rs` の `input` の分岐。下の `env`・`now`・`exit` と同じ扱い。レビューの指摘で直した）。
  - `env`・`now`・`exit`・登録 host function（回数の counter を今は数えていないもの）: capability 判定より前
    （§4.1「許可・拒否・host error のいずれでも課金」と §6.1「capability 判定前に `host_calls += 1`」による）。
    登録 host function では、引数を評価した後、`resolve_host_call` などの grant 検査より前に置く。
  host-call の回数・byte を数えない逸脱一覧の 2 行は fuel とは別の counter の逸脱なので、F-2c では残す。規模は約 50 行
  増えて §14.1 に収まる。採らなかった案: B（filesystem builtin と登録 host function だけ）は stdio と clock・env・process を
  数えないまま版 `2` を固定して §2・§6.1 と食い違い、直すには版をもう一度上げることになる（原則 3・9）。C（filesystem
  builtin だけ）は登録 host function の host 呼び出しが fuel に現れず資源会計が過小になり、マニフェストに反しうる（原則 3）。
- **Q2 = A（登録 host function の呼び出しは invocation 5 と host call attempt 10 の両方、計 15 を課金する）**: 上の
  「F-2 実装スライス」小節の F2-AT-01b は filesystem builtin の 1 文の内訳を「host call 10 + builtin invocation 5 + …」と
  しており、host へ出る builtin は既に両方を払う。登録 host function も同じ形にすると、同じ形の呼び出し式の費用が callee の
  種類で変わらない（原則 5）。§4.1 の invocation の行の備考（user function、lambda、core builtin、callback）は登録 host
  function を挙げていないが、**script の呼び出し式として呼ぶものは invocation に数える**と読み、登録 host function を含める。
  invocation の位置は今の `ChargeKind::HostCall` の位置（`src/eval.rs`、callee・引数の評価より前）のままで、host call attempt
  は Q1 の位置に置く（原則 8）。[Capability仕様](capability-model.md) §11.2 の `HostCost`（grant 後・callback 前の fuel）は
  この 15 とは別のもので、同仕様の逸脱一覧のとおり F-2c では扱わない。採らなかった案: B（host call attempt 10 だけ）は
  同じ呼び出し式でも callee が filesystem builtin なら 15、登録 host function なら 10 になって規則が 2 つになり、引数評価前の
  今の課金点も消える（原則 5）。C（invocation 5 だけ）は §4.1 の host call attempt を登録 host function に課金せず、Q1 の C と
  同じくマニフェストに反しうる（原則 3）。
- **Q3 = A（実行した分だけを前順で数える）**: 両 engine が次の規則に従う。
  1. statement: `Stmt` ノード 1 つを実行し始めるとき 1（`FnDef`・`Import`・`Break`・`Continue`・`TryCatch` を含む。
     ブロックは数えない）。
  2. expression: `Expr` ノード 1 つを評価し始めるとき 1（前順）。呼び出し式の callee が名前（`Expr::Ident`）のときは callee を
     別の式に数えない（F2-AT-01b の内訳「call 式の expression 1 + 引数の expression 1」）。f-string は `FStr` 1 と式のパートごとに
     1（リテラルのパートは数えない）。lambda は `Lambda` 1 で、本体は呼ばれたときに数える。
  3. operation: `BinOp`（算術・比較・`and` / `or`）・`UnaryOp`・`Index` は operand を評価した後、演算の直前に 1。`and` / `or` は
     左を評価した後、右を評価するかを決める前に 1（右を評価しなくても 1）。assignment は `Let`・`Assign`・`IndexAssign` で値
     （と index）を評価した後、格納の直前に 1。`IndexAssign` に index operation を別に数えない。`for` の変数と `catch` の変数への
     束縛は数えない。operand の評価で失敗した演算には operation を課金しない。
  4. invocation 5: 今の `UserCall`・`HostCall` の課金と同じ位置（callee と引数の評価より前）。core builtin も同じ位置。callback は
     今の `BuiltinCallback` の位置（`Vm::call_fn_value`・`callback_precheck`、Q5 = A）。
  5. loop iteration 2: body に入る直前（`while` は condition が真と決まった後、`for` は次の要素があると決まった後）。最後の
     condition 偽・要素なしでは課金しない。`break` で抜ける反復も 2 を払う。`continue` 文は規則 1 の statement 1 だけを払い、loop iteration は
     課金せず、次の反復の開始で課金する（F-1 の二重課金回避を保つ）。VM は `Charge(LoopIteration)` を back-edge の `Loop` の直前から body の先頭へ移す。`for` の
     反復ごとの iterable の課金は無い（iterable 式はループの前に 1 回）。
  §4.1 の「statement へ入る」「expression へ入る」「loop iteration 開始」と、上の小節の「F-1 の back-edge 1 課金を iteration
  開始 2 へ置き換える（課金のタイミングと値の両方が変わる）」「1 反復につき 2」に合う。上限 N − 1 で止まる位置が AST の評価順
  から一意に決まるので、F2c-AT-03 の両 engine の stdout の一致を規則から説明でき、各反復に 2 以上の課金が残るので F2c-AT-05
  とも食い違わない（原則 5・6・9）。採らなかった案: B（operation を expression の入口でまとめて課金し、loop iteration は F-2b と
  同じ back-edge に置く）は VM の `Charge` を動かさずに済む（原則 8）が、`break` で抜けた反復に 2 が課金されず「1 反復につき 2」と
  食い違い、F2c-AT-01 の反復数差の手計算が `break` の有無で変わる。operand の評価で失敗した演算にも operation を課金して
  「実行した分だけ」にならない（原則 5）。

**判断の記録**（F-2c の実装 PR #485 のレビュー 80844b3 の S1。判断役が選択（PR #485）、Q1=A）:

- **Q1 = A（実装と食い違う他の文書の記述を、この実装 PR で事実に合わせる）**: 対象はレビューが挙げた箇所だけで、
  [言語仕様](language-spec.md)「実行安全性」の段落と「ステップ予算」の数え方・既定値、`README.ja.md` の既定値、
  [Capability仕様](capability-model.md) の逸脱一覧の `HostCost` の行（「`HostCost` が課金に使われない」という逸脱の
  中身は変えず、今の課金（invocation 5 と host call attempt 10）の記述だけを合わせる）。言語仕様は現行実装から
  観測できる規範仕様で、実装との差は逸脱一覧にだけ書く。直さずにマージすると、版 2・20,000,000 の実装に対して
  既定 1,000,000・「ループ反復 + 関数呼び出しの合計回数」の記述が逸脱一覧にも無いまま main に入る（[マニフェスト](manifesto.md)
  原則 5・9）。正しい値と数え方は §3.1・§4.1 と上の Q1〜Q3 で決まっており新しい判断は要らず、文書 3 ファイルの
  数行で済む（原則 8）。言語仕様の既定値は §3.1 を指し、現在の値を括弧で添える形にした。採らなかった案: B（この PR は
  変えず、ずれの修正に回す）は、ずれと分かっているものをマージし、次の整合点検と別の PR まで文書が実装と違うままになる。

**判断の記録**（設計 PR #470 の判断の候補。判断役が選択（PR #470）、Q1〜Q5=A）:

- **Q1 = A（3 つに割る。F-2a → F-2b → F-2c）**: 観測を変えない土台（F-2a・F-2b）を先に入れ、課金表の移行・版 1→2・
  既定 `total_fuel` を F-2c の 1 本にまとめる。F-2b は既存の fuel 期待値が無改変で緑であることで確かめられ、tree と VM の
  課金が食い違う中間状態を main に出さない（[マニフェスト](manifesto.md) 原則 9・5）。版の bump と既定 `total_fuel` を同一
  PR で行う「F-2 実装スライス」の制約を F-2c で守れ、3 本とも §14.1 に収まる。採らなかった案: B（2 つ）は F-2bc が
  `src/` 900〜1100 行の見込みで §14.1 を超えうるうえ、観測不変の置き換えと観測を変える移行が混ざる。C（4 つ）は版の
  bump と既定値の測定が 2 回になり、課金表が半分だけ移行した版 2 が main に残る（原則 8）。
- **Q2 = A（F-2c の受入基準を 5 個にまとめる）**: 旧 F2-AT-01..08・01b はすべて F2c-AT-01..05 のどれかに対応し
  （上の F-2c の各項目に旧 ID を書いた）、確かめる中身を減らさずに §14.1 の 5 項目に収まる（原則 9）。採らなかった案:
  B（9 個のまま）は §14.1 と documentation-guide「ready の条件」4 を満たさない。
- **Q3 = A（config error の variant を 2 つに分ける）**: `UnsupportedHeapAccountingRevision(u32)` /
  `UnsupportedFuelAccountingRevision(u32)`。公開型を増やさず `match` だけで区別でき、既存の利用箇所が少ない（原則 8）。
  既存 variant を消すのは alpha の後方互換無保証の範囲。採らなかった案: B（`{ kind: AccountingKind, revision }`）は
  公開 enum を 1 つ足し既存 variant の形も変える分、公開面が広がる。§3.1 に反映した。
- **Q4 = A（audit の `schema_version` を 1 → 2 に上げる）**: [決定性・監査仕様](determinism-and-audit.md) §7.1 は field 名・型・
  意味を schema version として固定し、§9.2 は「unknown schema field を末尾へ追加せず、schema version を上げる」とする。
  `ExecutionStarted` に top-level の field を足すのは固定の変更なので、版で区別できるようにする（原則 7・5。PR #415 の
  Q2 の決定記録と同じ向き）。`AuditEnvelope` は `schema_version: 1` で既に出ている（`src/embedding.rs`）。採らなかった案:
  B（v1 をその場で改める）は同じ版で field の並びが違う記録が出て読み手が区別できず、マニフェストに反しうる（原則 7・5）。
  決定性・監査仕様 §7.1・§7.2・§9.2 に反映した。
- **Q5 = A（VM の builtin callback の課金は `Charge` にせず `FuelSchedule` を直接呼ぶ）**: 判断役が選択（PR #470）。
  レビュー（dfe9e7e）で、F-2b の範囲の「`call_fn_value` に置いた `count_step` を外して、compiler が同じ位置に `Charge` を
  挿入する」は実装できないと分かった。callback は builtin（Rust 側）から呼ばれるので、compiler が `Charge` を置ける
  bytecode 上の点が無い。`Vm::call_fn_value` は `count_step` → 深度検査 → callable 検査 → arity 検査の順で進み、tree の
  `src/builtin.rs` の callback 前処理も同じ順である。A は課金の位置・値・line と深度検査との順序を今のまま保つので、
  F2b-AT-01 の観測不変を既存のテストが無改変で緑であることで確かめられる（原則 5・9）。課金値は tree と同じ共有
  `FuelSchedule` から取るので、§4.1 の「tree evaluator は同じ共有 `FuelSchedule` API を呼ぶ」「VM `Charge` dispatch = 0」
  と食い違わない。F-1 小節の (F3)（`Vm::call_fn_value` で深度検査より前に課金）もそのまま残る。F2b-AT-02（ループ反復・
  呼び出しの位置の `Charge`）は変えない。文言はこの PR で F-2b の範囲を直した。採らなかった案: B（呼ばれる関数の入口に
  callback 用の `Charge` を出す）は、課金が深度検査・callable 検査・arity 検査の後に移る。深度超過や callable でない値を
  渡したときの fuel と、fuel 上限と深度上限が同時に効くときにどちらの error になるかが tree と食い違いうるので、
  マニフェストに反しうる（原則 5）。`PrepareCall` 経由の呼び出しと区別する仕組みも増える（原則 8）。C（文言を F-2b を
  ready にする回に回す）は、実装できない文言を正本に残し、その回で同じ判断をやり直すことになる（原則 9）。

#### fuel-bulk-charge の sub-slice 分割（fuel-bulk-charge-a / -b / -c）

> 状態: **未実装**（設計のみ。2026-11-23 の設計 PR #501 で割り方を書いた。割り方と
> fuel-bulk-charge-a の課金の規則は判断の候補 Q1・Q2 にしてあり、判断役の選択を本小節の末尾に記録する）。
> 各 sub-slice の進捗は [`slices.toml`](slices.toml) の `fuel-bulk-charge-a` / `-b` / `-c` を正とする。
> `slices.toml` の `fuel-bulk-charge` は sub-slice のエントリに置き換えた（元の id は残さない。「fuel-bulk-charge が
> done」は a・b・c がともに done を意味する）。上の小節で `fuel-bulk-charge` と書いた延期先は a〜c 全体を指す。

`fuel-bulk-charge` の範囲は、冒頭の未実装範囲の §4.1 の行のとおり、§4.1 の課金表のうち版 `2`（F-2c）で課金して
いない 3 つの行である。(1) collection element を走査・copy・生成する 1 / element のうち、比較・表示・`sort`・
`join` / `contains`・深度の読み直し以外（`map` / `filter` と、要素を作る・複製する builtin と COW の複製）、
(2) UTF-8 payload の scan・copy・encode/decode の `ceil(bytes / 64)`（string・source・I/O・host payload）、
(3) compile・import link 中の AST node 生成の 1 / node。

**§14.1 に収まらない理由**（main 87166f4 で確かめた）:

- 触る仕組みが 3 つある。(1) は builtin の境界と値の COW（`src/builtin.rs` の `builtin_collection` と式 frame の
  段階実行、`src/vm.rs` の `exec_builtin` の `map` / `filter` / `each`、`src/builtin_core.rs` の `dispatch` が呼ぶ
  `range` / `keys` / `values` / `slice` / `reverse` / `split`、`src/value.rs` の `detach_list` / `detach_dict`）。
  (2) は budget の byte の課金（`src/budget.rs` の `charge_string` / `charge_source` / `charge_input` /
  `charge_output` / `charge_host_call_request` / `charge_host_response_bytes` と、それを呼ばない scan だけの
  string builtin、host function の引数・結果の変換）。(3) は compile と link（`src/parser.rs`・`src/module.rs`・
  `src/embedding.rs` の `Engine::link`。今の fuel は execution の台帳だけにあり、compile・link の fuel の払い手が
  決まっていない）。
- 受入基準は、仕組みごとに課金点・値・tree/VM の一致・境界・版と既定 `total_fuel` を確かめると 10 個を超える。
- 規模: (1) 約 300〜450 行、(2) 約 400〜600 行、(3) 約 200〜350 行の見込みで、合わせて §14.1 の 1000 行を超える。

**割り方（判断の候補 Q1。推奨は A）**: 仕組みごとに 3 本へ割り、a（per-element）→ b（UTF-8 payload）→
c（AST node）の直列にする。各 sub-slice は課金点を足す同じ PR で `fuel_accounting_revision` を 1 つ上げ、既定
`total_fuel` を代表 script 集合（上の「F-2 実装スライス」小節）で測り直す（a で 2 → 3、b で 3 → 4、c で 4 → 5）。
版の bump と既定 `total_fuel` の更新を同じ PR で行う「F-2 実装スライス」の「スライス分割の指針」の制約は各 sub-slice で
守る。順は §4.1 の表と本スライスの元の title の並びに合わせ、すでに builtin の per-element 課金と chunk の仕組み
（fuel-bulk-chunk-yield）がある (1) を先にする。b・c の範囲の細部（scan だけの builtin の扱い、compile・link の fuel の
払い手と上限）は、それぞれを ready にする設計 PR で決める。

##### fuel-bulk-charge-a 実装スライス（`map` / `filter` / `each` と要素を作る・複製する操作の per-element fuel、版 2 → 3）

- **範囲**: §4.1 の「collection element を走査・copy・生成 1 / element」を、次の操作に両 engine の同じ論理位置・同じ値・
  同じ line で課金する（規則は判断の候補 Q2。推奨 A のとおりに書く）。課金は `ChargeKind` に per-element の種別
  （例: `Element`、`FuelSchedule::cost` は 1）を足して `FuelSchedule` から取る。bytecode には出さず、builtin・値の
  操作（Rust 側）が直接課金する（`ChargeKind::allowed_in_bytecode` は偽。host call attempt と同じ扱い）。
  - `map` / `filter` / `each`: 第 1 引数の List の要素 1 つを callback へ渡す直前に 1（走査）。callback の invocation 5
    （F-2c）とは別に払う。`map` の結果の要素と `filter` が残す要素の追加は数えない（同じ要素の走査と生成を 2 回数えない）。
    tree は同期経路（`src/builtin.rs` の `builtin_collection`）と式 frame の段階実行（tree-expr-continuation-e4）の両方、
    VM は `src/vm.rs` の `exec_builtin` で、同じ順に課金する。
  - 要素を作る・複製する builtin（`range` / `keys` / `values` / List の `slice` / List の `reverse` / `split`）: 結果の要素数 n を
    1 回の課金で払う。順は arity・型の検査 → `max_collection_elements` の検査 → fuel n → 結果の確保。n が引数から
    決まるもの（`range` / `keys` / `values` / `slice` / `reverse`）は `dispatch` の前に、`split` は区切りの走査
    （UTF-8 の scan で b の範囲）の後・結果の List を作る前に払う。Str の `slice` / `reverse` は要素でなく UTF-8 の
    copy なので b の範囲。
  - COW の複製: `Value::detach_list` / `detach_dict`（`src/value.rs`）が共有中の backing を複製するとき、複製する
    要素（entry）数を複製の前に 1 回で払う（`push`・`pop`・index 代入が共有中の値を書き換えるとき）。共有していない
    値は複製しないので 0。List / Dict の深度の読み直し（§4.1、版 `1` から）は今の位置と値のまま、その後に複製を払う。
  - `for` の Dict・Str の反復列の生成（`Value::new_iter_str_list`）: 作る要素数を、要素数の検査の後・backing の確保の前に
    1 回で払う。List の反復列は共有するので 0（fix-for-iter-snapshot-charge）。
  - 版: `fuel_accounting_revision` を 2 → 3 にし（受理する版を `3` だけにする）、`BudgetConfig::standard` を `3` に
    する。既定 `total_fuel` は代表 script 集合を版 `3` で測り、通る最小の切りの良い値にする（今の 20,000,000 で
    通るなら変えない）。
- **範囲外**:
  - 既に per-element を課金している比較・表示・`sort`・`join` / `contains`・深度の読み直し（値も位置も変えない）。
  - UTF-8 payload（string builtin・文字列の連結・f-string・source・I/O・host payload）と serialize（host function の
    引数・結果の変換、監査の値）は fuel-bulk-charge-b。AST node は fuel-bulk-charge-c。
  - List / Dict リテラルの構築（要素は式として F-2c で課金済み）、`len` / `has_key` など要素を走査しない builtin。
  - 1 回の課金 n が slice の残量を超える場合の yield。tree の poll 経路は yield 可能点でだけ slice を確かめるので、
    1 回分は §4.2 の超過（`max_collection_elements` で有界）として扱う。chunk 境界の yield の追加は
    fuel-bulk-chunk-yield の延期（冒頭の未実装範囲）のまま。VM の slice yield（Slice 6）。
- **受入基準**:
  - **FBCa-AT-01**: `map` / `filter` / `each` で、要素数だけが 1 違う 2 本の script の「成功する最小の上限 N」の差が
    1 + callback 1 回分（invocation 5 と本体の課金）で、両 engine で一致する。tree は同期経路と poll 経路（式 frame）の
    両方で同じ N になる（`tests/differential.rs`）。
  - **FBCa-AT-02**: `range` / `keys` / `values` / List の `slice` / List の `reverse` / `split` で、結果の要素数だけが k 違う
    2 本の N の差が k で、両 engine で一致する（`tests/differential.rs`）。
  - **FBCa-AT-03**: 共有中の List / Dict への `push`・`pop`・index 代入と、Dict・Str の `for` で、複製・生成する要素数
    だけが k 違う 2 本の N の差が k で、共有していない値では差が 0 になり、両 engine で一致する。
  - **FBCa-AT-04**: FBCa-AT-01〜03 の各 script で N は両 engine とも成功、N − 1 は両 engine とも fuel の上限で失敗し、
    stdout・stderr（line・trace を含む）・exit code が一致する。`max_collection_elements` と fuel の両方を超える
    `range` は collection の上限の error になる（検査の順）。
  - **FBCa-AT-05**: `BudgetConfig::standard` の `fuel_accounting_revision` は `3` で、`2` は execution 作成前に
    `UnsupportedFuelAccountingRevision(2)` で拒否される。既定 `total_fuel` で代表 script 集合が両 engine で成功し、
    各 script の消費 fuel を実装 PR 本文に記録する。fuel の期待値を持つ既存テストを新しい課金へ更新して両 engine で緑で、
    watchdog の停止性ケースは fuel 上限が先に来る。`language_revision` は変わらない。
- **文書**（実装 PR）: §3.1 の表と本文の現在版（`3` と per-element の課金）、[次期意味論・実装決定](semantic-decisions.md)
  §17.7.3 の fuel の論理課金点、[言語仕様](language-spec.md) の「ステップ予算」の数え方、`src/budget.rs` の `ChargeKind`・
  `FuelSchedule` の doc を合わせる。冒頭の未実装範囲の §4.1 の行は、per-element の部分を消して b・c の範囲に絞る。
  未実装範囲の bulk builtin の行の (1) から、本スライスで per-element を課金した builtin を外す。
- **想定の変更規模**: `src/` 約 300〜450 行（`ChargeKind`・`FuelSchedule`・版と既定値 約 40、`map` / `filter` / `each` の
  tree 2 経路と VM 約 80、要素数の課金の共有 helper と tree・VM の `dispatch` 境界 約 120、COW の複製と反復列 約 40、
  doc 約 30）、触る仕組みは fuel の課金点 1 つ（F-2c と同じ数え方）、受入基準 5。1000 行を超える見込みになったら、
  実装に入る前に止めて報告する（§14.1）。

**設計時の注記**（main 87166f4 で確かめた。実装で読み違えやすい点）:

- 今の per-element の課金は `budget.charge_fuel(1, phase)` などの直接呼び（`src/builtin_core.rs` の `bulk_step` の
  `SortScan`、`src/value.rs` の比較・深度の読み直し）で、`FuelSchedule` を通らない。本スライスは足す課金だけを
  `FuelSchedule` から取り、既存の直接呼びは変えない（値も位置も同じ）。
- `dispatch`（`src/builtin_core.rs`）は budget を受け取らない。要素数の課金は tree（`src/builtin.rs` の
  `apply_pure_core_builtin`）と VM（`src/vm.rs` の `exec_builtin`）の境界で、同じ共有 helper が引数から数えた n を払う。
- 式 frame の段階実行（e4）の `map` / `filter` / `each` は yield を跨ぐので、要素の課金は「次の要素を callback へ渡す」
  位置で 1 回だけ行い、resume で同じ要素を 2 回課金しない。

### fix-contains-budgeted-equality 実装スライス（`contains` の等価判定を予算付き比較へ）

issue #290 から作ったスライス（不具合の修正、`slices.toml` の `fix-contains-budgeted-equality`）。issue の仕分けで「受け入れ」とし、判断は要らないとした部分である（[言語仕様](language-spec.md)「等価比較」の「この規則は`contains`のように等価判定を使う組み込み関数でも共有する」で直し方が決まる）。

- **現状の不具合:** `contains(list, x)` は `src/builtin_core.rs` の `builtin_contains` で `Vec::contains`（`Value` の `PartialEq`）を使う。`PartialEq` は List / Dict を Rust の再帰でたどり、fuel を課金せず、共有された入れ子（DAG）を枝刈りしない。このため `a = [a, a]` を 60 回繰り返した値どうしの比較が指数時間になり、fuel・deadline・cancel のどれでも止まらない（issue #290 の再現 2。CLI の 30 秒の deadline も効かない）。`==` / `!=` は REV-001 で予算付きの `BudgetLedger::values_equal`（`src/budget.rs`、worklist で走査し List / Dict のペアごとに fuel 1 を課金、比較済みペアを枝刈り）へ移っており、`contains` だけが取り残されている。
- **実装範囲:**
  - `contains` の第 1 引数が List のとき、各要素と第 2 引数の等価判定を `values_equal` で行う。`sort` / `join` / `to_str` と同じく、budget を持たない `builtin_core::dispatch` へ入れず、tree（`src/builtin.rs`）と VM（`src/vm.rs`）の dispatch 境界で先に intercept する budget 付きの関数（例: `builtin_contains_budgeted`）にする。tree の式 frame 経路（tree-expr-continuation-e3）を含め、PureCore builtin を呼ぶすべての経路が同じ関数を通る。
  - 要素を先頭から順に比較し、等しい要素が見つかった時点で `true` を返す（残りの要素は比較しない）。
  - fuel 超過は `==` と同じく catch できない terminal（`BudgetExceeded(Fuel)`）になる。
  - `builtin_core::dispatch` の `"contains"` の List 分岐は使われなくなるので、`sort` などと同じく到達しない旨のコメントにするか、List 分岐を budget 付きの関数へ寄せる（Str・Dict 分岐の挙動は変えない）。
- **範囲外:**
  - 第 1 引数が Str（部分文字列）・Dict（key の有無）のときの挙動と課金。値の比較を含まないので変えない。
  - 要素を走査する per-element 課金（§4.1 の「collection elementを走査」）。F-2・`fuel-bulk-charge` の範囲で、本スライスは `==` と同じ `values_equal` の課金だけを加える。
  - 値のネスト深度の上限（同じ issue の別スライス `fix-value-nesting-depth-limit`）。
- **課金の変化:** List / Dict を要素に持つ List への `contains` は、`==` と同じく比較した List / Dict のペアごとに fuel を使うようになる。スカラーだけの List への `contains` の fuel は変わらない（`values_equal` はスカラーの比較に課金しない）。これは §4.1 の課金表に charge point を足す変更ではなく、`==` / `!=` がすでに使う比較の課金を、[言語仕様](language-spec.md)が等価判定を共有すると定める `contains` にも適用する修正なので、`fuel_accounting_revision`（§3.1。実装は F-2 で入る）の版は変えない。
- **想定の変更規模:** `src/` は 40〜80 行程度（`src/builtin_core.rs` に budget 付きの関数 20〜40 行、`src/builtin.rs` と `src/vm.rs` の intercept に各 5〜10 行、dispatch の List 分岐の整理）。テストは `tests/` に 100〜200 行。触る仕組みは「builtin の dispatch 境界での budget 付き intercept」1 つ。受入基準は 4 項目。§11.1（[開発ループ手順書](loop-runbook.md)）に収まる。
- **受入基準:**
  - **CBE-AT-01（`==` と同じ結果）:** `contains([a], b)` が、Int と Float の跨ぎ（`1` と `1.0`）、NaN、入れ子の List / Dict（`[1]` と `[1.0]`）、関数値の同一性、エラー値、型違いの組で `a == b` と同じ真偽を返す。tree-walk 版と VM 版で同じ。確かめ方: 組の表を回し、`contains([a], b)` と `a == b` の出力を比べる統合テスト（両 backend）。
  - **CBE-AT-02（共有 DAG で有限時間）:** issue #290 の再現 2（`a = [a, a]`・`b = [b, b]` を 60 回）の `contains([a], b)` が既定の budget で有限時間に `true` を返す。確かめ方: CLI を subprocess で起動し、壁時計の上限（例: 10 秒）以内に `true` を出して exit 0 で終わることを確かめるテスト（両 backend）。
  - **CBE-AT-03（fuel 超過は terminal）:** 比較が fuel を使い切る `contains`（共有しない大きな入れ子どうしの比較）が、`==` と同じ catch できない `BudgetExceeded(Fuel)` で止まり、`try` / `catch` で捕捉されない。確かめ方: 小さい `total_fuel` を設定した実行で、`try` の内側の `contains` が terminal で終わり catch 節が走らないことを確かめるテスト。
  - **CBE-AT-04（課金が `==` と一致）:** List / Dict を要素に持つ `contains([x], y)` の fuel 消費の増分が、同じ `x` と `y` の `x == y` の比較の増分と一致し、スカラーだけの List への `contains` の fuel 消費は変更前と同じである。確かめ方: 同じ入力で `contains` と `==` の usage の fuel を比べるテストと、スカラーの List で変更前の値を固定するテスト。

### fix-value-nesting-depth-limit 実装スライス（値のネスト深度上限）

issue #290 から作ったスライス（不具合の修正、`slices.toml` の `fix-value-nesting-depth-limit`）。目標の規範は[次期意味論・実装決定](semantic-decisions.md) §21（値のネスト深度上限）で、issue #290 の判断待ち Q1 で返信なしのため推奨案（A: 値を作る時点で深度を検査し、超過は catch できるエラーにする）で確定した（[開発ループ手順書](loop-runbook.md) §7.5）。

- **現状の不具合:** List / Dict の入れ子の深さに上限がなく、`x = [x]` を 20 万回繰り返した値の破棄（`src/value.rs` の `Tracked` の Drop）で host stack があふれ、プロセスが abort する（issue #290 の再現 1。tree-walk 版・VM 版とも exit 134。Dict の `d = {"a": d}` でも同じ）。比較の `TotalOrder::compare_budgeted`・`Display`・`render_into` も値を Rust の再帰でたどる。
- **実装範囲:**
  - [次期意味論・実装決定](semantic-decisions.md) §21.3 の規範のとおり、List / Dict の深度を持ち、深度が 257 以上になる値を作る操作を、値を作る前に `overflow`（`値のネストが深すぎます (上限: 256)`）で拒否する。対象の経路は §21.3 の「対象」のとおり（List / Dict リテラル、`push`、index 代入、`map` の結果）。tree-walk 版（`src/eval.rs`・`src/builtin.rs`）と VM 版（`src/vm.rs` の `ListPush` / `DictInsert` / `ListPushBinding` など）の両方。
  - 深度は値を再帰でたどらずに求める。List / Dict の backing（`src/value.rs` の `Tracked`）に深度を持たせ、作るときは要素の保存済み深度の最大 + 1 を求める（要素ごとに O(1)）。`push` は「既存の深度」と「入れる値の深度 + 1」の大きい方にする。index 代入・`pop` で深度が下がりうる場合も、§21.3 のとおり正確な深度を保つ（要素の保存済み深度を読み直す。値を再帰でたどらない）。index 代入を繰り返すループが要素数の 2 乗の時間にならないよう、読み直しは必要なときに限る（例: 最大深度の要素の数を持ち、0 になったときだけ読み直す）。具体的な持ち方は実装に任せる。
  - 上限はコンパイル時定数 `MAX_VALUE_DEPTH = 256`（`src/limits.rs`。`MAX_AST_DEPTH`・`MAX_USER_CALL_DEPTH` と同じ置き場所）。
  - 拒否したときは対象の List / Dict を変更しない（COW の detach・課金の前に検査する）。
  - 言語仕様への統合: [言語仕様](language-spec.md)の「実行安全性」に「値のネスト深度制限」の小節を足し（「コールフレーム深度制限」と同じ形: 上限・超過時・対象・設定変更）、ランタイムエラーの表に `overflow` の行を足し、章冒頭の要約文に値の深度を足す。[次期意味論・実装決定](semantic-decisions.md) §21 は統合済みと記す（§21.7）。`language_revision` で分岐せず、言語仕様のバージョン（0.21）と `LanguageRevision` は変えない（§21.6。設計 PR #297 の判断待ち Q1=A、返信なしのため推奨案で確定、[開発ループ手順書](loop-runbook.md) §7.5）。
  - [脅威モデル](threat-model.md) TM-007 の緩和策に値のネスト深度の検査を足し、TM-AT-07 に値の深度 256 / 257 の試験を足す。closure の capture の連鎖（下記の範囲外）を TM-007 の残存リスクに書く。
- **範囲外:**
  - closure の capture の連鎖（`f = fn() g() end` で前の関数値を capture し続ける）による stack 枯渇。関数値の深度は 0 と数える（§21.3）ため本スライスでは防げない。issue #290 の設計の際に確かめた事実として、20 万段の連鎖の破棄で tree-walk 版・VM 版とも abort する（exit 134）。直し方は `fix-closure-capture-chain-stack`（`todo`）の設計で決める。
  - host function の戻り値や host から渡された値そのものの深度の検査（§21.3。スクリプトが List / Dict に入れるときは検査する）。
  - 深度の計算への fuel・heap の課金（§21.3。課金しない）。
  - `contains` の予算漏れ（同じ issue の別スライス `fix-contains-budgeted-equality`）。
- **想定の変更規模:** `src/` は 300〜450 行程度（`src/value.rs` の backing の深度と生成・`push`・index 代入・`pop` の更新 150〜200 行、エラーの生成 10〜20 行、`src/eval.rs`・`src/builtin.rs` の検査の配線 50〜80 行、`src/vm.rs` の opcode の配線 50〜80 行、上限定数 5 行）。テストは `tests/` に 200〜350 行。docs は言語仕様・脅威モデル・本書の逸脱一覧。触る仕組みは「値の表現（List / Dict の backing）」1 つ。受入基準は 5 項目。§11.1（[開発ループ手順書](loop-runbook.md)）に収まる。
- **受入基準:**
  - **VND-AT-01（境界と catch）:** 深度 256 の List / Dict を List / Dict リテラル・`push`・index 代入・`map` の結果のそれぞれで作れ、深度 257 になる同じ操作が `overflow`・`値のネストが深すぎます (上限: 256)` で拒否される。`try` / `catch` で捕捉でき、`push` / index 代入の対象は変わらない。tree-walk 版と VM 版で kind・message・line が一致する。確かめ方: 経路ごとに 256 / 257 を作る script を両 backend で実行し、結果・catch した error の type / message / line・対象の `len` を比べる統合テスト。
  - **VND-AT-02（abort しない）:** issue #290 の再現 1（List を 20 万回入れ子にする）と Dict 版が、abort せず `overflow` の診断を出して exit 1 で終わる。確かめ方: debug ビルドの CLI を subprocess で起動し、exit code が 1 で stderr にメッセージがあることを確かめるテスト（両 backend）。
  - **VND-AT-03（深度は正確）:** 深度 256 の要素を index 代入・`pop` で取り除いた List / Dict は、再び入れ子にでき（深度 257 にならない）、深度の読み直しを要する index 代入を要素数ぶん繰り返しても完了する。確かめ方: 取り除いた後に `[x]` を作れることを確かめるテストと、深い要素を並べた List（例: 要素数 10 万）の全要素をスカラーへ index 代入するループが、壁時計の上限（例: 10 秒）以内に終わることを確かめるテスト（両 backend）。深度の読み直しは課金しない（§21.3）ため、fuel では要素数の 2 乗の実装を検出できず、壁時計で確かめる。
  - **VND-AT-04（上限の値で stack が足りる）:** 深度 256 の値を、call depth 128 の再帰の内側で表示（`print`・`to_str`）・比較（`==`）・`sort`・破棄しても abort しない（CLI の 8 MiB の実行スレッド、debug ビルド）。確かめ方: CLI を subprocess で起動して exit 0 で終わることを確かめるテスト（両 backend）。
  - **VND-AT-05（上限内の観測は不変）:** 深度 256 以下の値だけを使う既存のテスト（`tests/`・unit test）が期待値の変更なしで通り、fuel・`live_heap` / `peak_heap` の値が変わらない（深度の計算は課金しない）。確かめ方: 既存のテスト一式と、代表 script の usage を変更前の値で固定するテスト。
- **実装での判断（実装 PR #301 のレビュー C1、判断待ち Q1）:** 深度超過のエラーは公開型 `ControlStop` に variant を足して運ばない。`ControlStop` は変えず（§7 の「どの variant も script から捕捉できない terminal」を保つ）、tree-walk 版・VM 版の呼び出し側が値の生成・変更の primitive（`Value::new_list` / `new_dict` / `list_push_tracked` / `index_set_tracked`）を呼ぶ前に crate 内部の検査（`Value::check_element_depth`）で深度を確かめ、超過なら catch できる `overflow` を直接返す。深度の要約と検査の関数（`ChildDepth`・`depth` など）は `pub(crate)` にし、公開の生成関数（`Tracked::new` / `constant` / `new_via_handle`）に trait 境界を足さない。理由: 公開 API を増やさず（[マニフェスト](manifesto.md)原則 8、§21.6 が公開 API を増やさない理由と同じ）、停止理由の型の意味を一つに保つ（原則 5）。返信なしのため推奨案で確定（Q1=B、[開発ループ手順書](loop-runbook.md) §7.5）。
- **実装での判断（実装 PR #301 の再レビュー 43e885d の C1、判断待ち Q1）:** List / Dict リテラルの深度は、要素・値を 1 つ評価して入れるたびに検査する（VM の `ListPush` / `DictInsert` と同じ位置）。tree-walk 版も同じ位置で検査し（`src/eval.rs` の List / Dict リテラルの 2 経路 × 2 か所。`check_collection` の後、要素を入れる前）、リテラルを作り終えてからまとめて検査しない。`[e, print("side")]`（`e` は深度 256）は両 backend で `side` を出す前に `overflow`、`{"a": e, "a": 1}` も両 backend で `overflow` になる。VM・バイトコードは変えない。VND-AT-01 に両入力の paired test を足した。理由: 「List / Dict に値を入れるたびにその値の深度を見る」という 1 つの規則が §21.3 の「List / Dict に値を入れるすべての経路」と同じ読み方になり、collection の上限の検査順とも揃う（[マニフェスト](manifesto.md)原則 5・8）。返信なしのため推奨案で確定（Q1=A、[開発ループ手順書](loop-runbook.md) §7.5）。
- **実装での判断（実装 PR #301 の再レビュー 4b95225 の C1、判断待ち Q1）:** List の最も深い要素の差し替えや `push` / `pop` の往復で `settle_child_depth` が全要素を読み直し、時間が要素数の 2 乗になる件は、実装を変えずに未実装範囲として記録する（上の「未実装範囲」の表の 1 行）。受入基準 VND-AT-03 は満たしており、1 回の読み直しは `max_collection_elements`、回数は fuel で上限があるので止まらなくはならない。直し方（backing に深度ごとの子の数の histogram を持たせる、または読み直しへ fuel を課金する）は、前者が論理サイズに入らないメモリを足し、後者が §21.3「課金しない」と VND-AT-05 を変えるため、どちらも spec の判断を要するので `needs-design` の [issue #304](https://github.com/kamonabe/Tsumugi/issues/304) に回す。spec の目標の章は変えない。返信なしのため推奨案で確定（Q1=B、[開発ループ手順書](loop-runbook.md) §7.5）。

### fix-limits-call-order-doc 実装スライス（`MAX_USER_CALL_DEPTH` の doc を current と target に分ける）

loop:drift #302 の D-21 から作ったスライス（文書と実装のずれの修正、`slices.toml` の `fix-limits-call-order-doc`）。目標の規範は[次期意味論・実装決定](semantic-decisions.md) §17.5.2（REV-012）の「`src/limits.rs` の doc comment は current と target を混在させず、どちらの revision の契約かを明記する。実装が現行のうちは current を書く」で、#302 の Q3=A（返信なしのため推奨案で確定、当時の[開発ループ手順書](loop-runbook.md) §7.5）でこのスライスを足すことにした。

- **現状のずれ:** `src/limits.rs` の `MAX_USER_CALL_DEPTH` の doc comment（「いつ拒否するか」）は「動的 callee は先に一度だけ評価・分類し、user callable の場合だけ引数評価より前に深度を検査する」と、target（callee 先行、[次期意味論・実装決定](semantic-decisions.md) §5.1）の順を current の区別なしに書く。実装は [言語仕様](language-spec.md)「ユーザー定義関数の呼び出し順序」のとおり「step と深度を検査してから callee を評価する」（tree は `src/eval.rs` の user call の準備で `begin_user_call` が callee の評価より前。VM も観測上同じ）。この差は[次期意味論・実装決定](semantic-decisions.md)の逸脱一覧（§5.1・§17.5）にある。
- **実装範囲:**
  - `MAX_USER_CALL_DEPTH` の doc comment の「いつ拒否するか」を、current と target に分けて書く。current: 128 個目の user frame は実行でき、129 個目を作る直前に拒否する。user call では step と深度の検査が callee の評価より前にあり、検査に失敗すると callee 式と引数を評価しない（言語仕様「ユーザー定義関数の呼び出し順序」へのリンクか節名を書く）。target: 次の `LanguageRevision` で callee 先行へ切り替える予定であること（[次期意味論・実装決定](semantic-decisions.md) §5.1・§17.5.2）と、切り替えの commit でこの doc を target へ書き換えること。
  - 「何を数えるか」と、tree と VM が同じ境界を判定する段落は、current と target で変わらないので残す。
- **範囲外:**
  - 評価順の切り替え（callee 先行への変更）。`LanguageRevision` を上げる破壊的変更で、tree / compiler / VM / callback を同じ commit で変える（§17.5.2）。まだスライスは無い。
  - `src/limits.rs` 以外のコメント・コード、[言語仕様](language-spec.md)・[次期意味論・実装決定](semantic-decisions.md) の本文。逸脱一覧の行（§5.1・§17.5）は評価順のずれそのものを記録しているので、このスライスでは消さない。
- **想定の変更規模:** `src/` はコメントだけで 10〜20 行程度（`src/limits.rs` の doc comment）。コードとテストの変更は無い。触る仕組みは「`src/limits.rs` の doc comment」1 つ。受入基準は 2 項目。§14.1（[開発ループ手順書](loop-runbook.md)）に収まる。
- **受入基準:**
  - **LCD-AT-01（current と target を分ける）:** `MAX_USER_CALL_DEPTH` の doc comment が、current（step と深度の検査が callee の評価より前。言語仕様「ユーザー定義関数の呼び出し順序」）と target（callee 先行。次の `LanguageRevision` で切り替え、[次期意味論・実装決定](semantic-decisions.md) §5.1・§17.5.2）を見出しか前置きで区別して書き、current の記述が tree（`src/eval.rs` の `begin_user_call` の位置）と VM の実装に合う。確かめ方: レビューで doc comment と実装・言語仕様を照合する。`cargo doc --no-deps --document-private-items` で `src/limits.rs` を指す警告が出ない（ほかの警告の件数は main から増えない。`MAX_USER_CALL_DEPTH` は `pub(crate)` なので `--document-private-items` が要る）。
  - **LCD-AT-02（挙動は変えない）:** `src/` の差分がコメント行だけで、既存のテストが期待値の変更なしで通る。確かめ方: 差分の確認と `cargo test`。

### fix-push-snapshot-writeback 実装スライス（`push` を第 1 引数の snapshot へ追加して書き戻す）

issue #293 から作ったスライス（不具合の修正、`slices.toml` の `fix-push-snapshot-writeback`）。issue の仕分けで「受け入れ」とし、実装を仕様に合わせて直すことにした部分である。直し方は[言語仕様](language-spec.md)「コンテキスト依存組み込み関数の呼び出し順序」の「`push`は第1引数の値を読み取ってから第2引数を評価し、そのsnapshotへ値を追加して同じbindingへ書き戻し、`null`を返す。第2引数の評価中に同じbindingが再代入・変更されても、最終的には先に読み取ったsnapshotを基にしたListを書き戻す」（AUD-012 で規範化、[設計メモ](design.md)「context依存builtin契約の統一（AUD-012）」）で決まり、仕様の側は変えない。本節の「Slice 6: VM charge parity」の in-place 化（push/pop の full-clone 差の解消）は peak_heap の parity のための実装の選択で、この意味論を変える決定は記録されていない。

- **現状の不具合:** `push` は第 2 引数を評価した後の binding の現在値へ追加する。tree-walk 版は代入先の cell だけを第 2 引数の評価より前に解決し（`src/builtin.rs` の `push_target_cell`）、値は読まずに、評価後に `apply_push` で cell の現在値へ in-place で追加する（同期経路 `builtin_collection` と、tree-expr-continuation-e3 の式 frame 経路 `src/eval.rs` の両方）。VM 版は第 2 引数だけを compile してから `ListPushBinding`（`src/compiler.rs`、`src/vm.rs` の `list_push_binding`）で binding storage の現在値へ in-place で追加する。このため issue #293 の再現（`let xs = [1]`、`g` の中で `xs = [7, 8]` として `2` を返し、`push(xs, g())`）は両 backend とも `[7, 8, 2]` を出す。仕様どおりなら `[1, 2]` である。第 2 引数の評価中の in-place の変更（`push(xs, 9)`・`xs[0] = 5` など）も、仕様では捨てられるべきところが残る。言語仕様の「現在の既知非適合」はこのスライスの設計 PR で載せた。
- **実装範囲:**
  - `push(識別子, 式)` の 3 経路（tree の同期経路、tree の式 frame 経路、VM）で、第 2 引数を評価する前に第 1 引数の binding の値を snapshot として読む。snapshot は値の clone（List なら backing の `Rc` を 1 本増やすだけで、要素は複製しない）で、heap の課金は増やさない。未定義の binding は今と同じく第 2 引数の評価より前に報告する（tree は `get_cell` の失敗、VM は global なら `RequireGlobal` に相当する検査）。VM は、第 2 引数の bytecode の前に snapshot を operand stack へ積む命令を出し、`ListPushBinding` は snapshot と値の 2 つを pop する形にする（命令を足すか `ListPushBinding` の stack 効果を変えるかは実装で選んでよい。`src/verifier.rs` の stack 効果の検査も合わせて直す）。
  - 第 2 引数を評価した後、言語仕様の手順 4 のとおり、評価済みの値（snapshot と第 2 引数の値）について型と上限を検査する。snapshot が List でなければ `builtin_type`（`builtin_arg_type(line, "push", 1, "List", snapshot)`）、snapshot の長さ + 1 が collection の上限を超えれば今と同じ上限のエラー、値のネスト深度は今と同じく第 2 引数の値で検査する。第 2 引数の評価中に binding が List 以外へ再代入されても、snapshot が List ならエラーにしない。
  - 書き戻しは、snapshot を基にした List を同じ binding へ書く。binding の現在値の backing が snapshot と同じ（`Rc::ptr_eq`。第 2 引数の評価中に再代入も変更も無かった）なら、snapshot を drop して backing を一意所有へ戻してから、今と同じ in-place の delta 課金（`list_push_tracked` / `budget_list_push`）で追加する。違えば（再代入された、または snapshot を共有していたため第 2 引数の評価中の変更が COW で別の backing になった）、snapshot へ追加した List を binding へ書き戻す（snapshot の backing が一意所有なら in-place、共有されていれば今の `push` と同じく `detach_list` の COW 複製）。どちらの場合も、書き戻しの前に今と同じく binding を AUD-024 の rollback journal に記録する（tree は `journal_cell`、VM は `checkpoint_cell` / `checkpoint_stack_slot`）。
  - 第 2 引数の評価がエラーで終わったときは書き戻さない（binding は第 2 引数の評価が残した状態のまま）。snapshot は drop する。
- **範囲外:**
  - `pop`（第 2 引数が無く、評価中に binding が変わる経路が無い）と index 代入（`xs[i] = v`。[設計メモ](design.md)のとおり target 解決後に index・value の評価中の変更を保持する別の契約）。
  - 言語仕様の本文と `LanguageRevision`。本スライスは既にある規範への非適合の修正で、仕様 revision は 0.21 のまま変えない。
  - `builtin_core` の `builtin_push`（registry 契約とエラーメッセージ parity のため残置されているもの）の挙動。
  - VM の builtin 名の呼び出しの lowering の共有（`fix-vm-builtin-call-lowering`）。本スライスの後に実装される（`order` が後）ので、そのスライスでは snapshot を積む命令を builtin 経路の「引数より前」の部分として扱うことになる。
- **課金の変化:** 第 2 引数の評価中に binding が再代入も変更もされない `push`（既存の script のほぼすべて）は、heap・peak_heap・fuel・step が変更前と同じである（snapshot は `Rc` の複製で課金せず、追加の前に drop するので in-place の delta 課金のまま）。第 2 引数の評価中に同じ binding を変更・再代入する script だけは heap の課金が変わる。in-place で変更する script では、snapshot が backing を共有しているため、その変更が COW 複製になり heap の課金が増える。別の値へ再代入する script では、snapshot が古い backing を書き戻しまで保持するため、変更前は再代入の時点で release されていた古い backing が書き戻しまで live のまま残り、その間の live heap と peak_heap が増える（例: issue #293 の再現では、変更前は `xs = [7, 8]` の時点で `[1]` の backing が release されるが、変更後は `[1]` と `[7, 8]` が同時に live になる）。どちらも仕様が snapshot を残すと定めているための複製・保持で、§4.1 の課金表に charge point を足す変更ではないので、`heap_accounting_revision`・`fuel_accounting_revision` の版は変えない。
- **想定の変更規模:** `src/` は 80〜180 行程度（tree の 2 経路で snapshot を読み比べて書き戻す共通の関数 30〜60 行と各経路の呼び出し、式 frame への snapshot の保持 10〜20 行、`src/compiler.rs` の push の lowering 10〜20 行、`src/vm.rs` の `list_push_binding` 30〜60 行、`src/opcode.rs`・`src/verifier.rs` 10〜20 行）。テストは golden fixture と統合テストで 150〜300 行。触る仕組みは「`push` の書き戻し」1 つ（3 経路が同じ規則を通る）。受入基準は 5 項目。§14.1（[開発ループ手順書](loop-runbook.md)）に収まる。
- **受入基準:**
  - **PSW-AT-01（再代入しても snapshot を書き戻す）:** issue #293 の再現が両 backend で `[1, 2]` を出す。同じ形を、global・関数の local・closure が capture した binding・`for` の本体の中のそれぞれで、第 2 引数の関数の中で同じ binding を別の List へ再代入する場合について確かめ、どれも snapshot に値を足した List になる。確かめ方: tree / VM の両方で走る golden fixture（`tests/fixtures/` と `tests/integration.rs` の `fixture_tests!`）。
  - **PSW-AT-02（第 2 引数の評価中の変更も捨てる）:** 第 2 引数の関数の中で同じ binding へ `push(xs, 9)`・`xs[0] = 5`・`pop(xs)` をしてから値を返す場合、結果は snapshot に値を足した List で、その変更は残らない。別名（`let ys = xs` を先に取った binding）から見た値は、COW の規則（[次期意味論・実装決定](semantic-decisions.md) §11）どおり影響を受けない。確かめ方: PSW-AT-01 と同じ golden fixture。
  - **PSW-AT-03（検査は評価済みの snapshot で）:** snapshot が List でない binding への `push` は、第 2 引数で List へ再代入しても、第 2 引数を評価した後に `builtin_type` の catch できるエラーになる（第 2 引数の副作用は起きる）。snapshot が List なら、第 2 引数で `5` へ再代入してもエラーにならず snapshot に足した List を書き戻す。未定義の binding は第 2 引数の副作用より前に catch できる name エラーになる（変更前と同じ）。第 2 引数の評価がエラーで終わったときは binding へ書き戻さない。いずれもエラーの kind・message・行が両 backend で一致する。確かめ方: golden fixture（`try` / `catch` で catch して出力を比べる）。
  - **PSW-AT-04（tree の式 frame 経路と REPL）:** 第 2 引数の関数の本体で slice yield が起きる実行（tree-expr-continuation-e3 の強制 yield フックを使う既存のテストの形）でも PSW-AT-01 と同じ結果になる。REPL で同じ入力の中の再代入を伴う `push` の結果が両 backend で一致し、エラーで終わった入力の後の rollback（AUD-024）で binding が入力の前の値に戻る。確かめ方: 既存の式 frame の yield テストと REPL テストへのケースの追加。
  - **PSW-AT-05（変更しない push の課金は不変）:** REV-015 Slice 6 の paired test（`tests/scaling.rs` の `push_peak_heap_has_no_full_clone_spike_across_engines`・`push_to_shared_backing_has_no_spike_across_engines`・`push_live_heap_is_linear_not_quadratic_across_engines`・`push_delta_charging_live_heap_is_linear_not_quadratic`）と、既存の `tests/differential.rs`・golden fixture・`tests/defensive_vm.rs` が期待値の変更なしで通る。push を含む differential test で fuel・step・heap / peak_heap が tree と VM で一致する。`src/verifier.rs` が変えた push の lowering を受け入れ、snapshot の無い手組みの不正な Chunk を VM が panic せず `internal` error にする。確かめ方: 既存テストと、verifier の unit test・`tests/defensive_vm.rs` への追加。

### fix-value-depth-settle-quadratic 実装スライス（深度の読み直しへの fuel 課金）

判断済みの `needs-design` [issue #304](https://github.com/kamonabe/Tsumugi/issues/304) から作ったスライス（`slices.toml` の `fix-value-depth-settle-quadratic`）。issue #304 は fix-value-nesting-depth-limit の実装 PR #301 の再レビューで見つかった件で、kamonabe の `判断:` で「読み直しへの fuel 課金」に決まった。目標の規範は[次期意味論・実装決定](semantic-decisions.md) §21.3 の「課金」と本書 §4.1 の「List / Dict の深度の読み直し」の行で、どちらもこのスライスの設計 PR で改訂した（決定記録と採らなかった histogram の理由は §21.3）。

- **現状の不具合:** List / Dict の backing（`src/value.rs` の `Tracked`）は「子の深度の最大 `child_max`」と「その深度を持つ子の数 `child_max_count`」を持ち、index 代入・`pop` で `child_max_count` が 0 になると `settle_child_depth` が全要素の保存済み深度を読み直す。この読み直しは fuel に課金しない。最も深い子がただ 1 つの List で `xs[0] = 0` と `xs[0] = e`（`e` は深い値）を往復する、または最も深い子を `pop` して `push` し直す往復では、1 往復ごとに要素数ぶんを読み直すので、時間が要素数の 2 乗になり、fuel に比例しない（issue #304 の計測で N = 50,000 のとき約 24 秒）。Dict でも、最も深い値がただ 1 つの key を浅い値で上書きすると同じく読み直す。
- **実装範囲:**
  - 読み直しが起きる 3 か所（`Value::index_set_tracked` の List と Dict、`Value::list_pop_tracked`）で、読み直す子の数ぶんの fuel を `BudgetLedger::charge_fuel` で課金する。量は、List の index 代入では要素数、Dict の index 代入では entry 数、`pop` では取り除いた後の要素数（0 なら課金しない）。§4.1 の課金表の「1 / 読み直した子」のとおり。
  - 課金は値を変える前に行う。読み直しが起きるかは、値を変える前に backing の要約と入れる値・取り除く値の深度から決まる（List の index 代入: 取り除く子の深度が `child_max` に等しく、`child_max_count` が 1 で、入れる値の深度が `child_max` より小さい。Dict の index 代入: 既存の key の上書きで同じ条件。新しい key は読み直さない。`pop`: 取り除く末尾の子の深度が `child_max` に等しく、`child_max_count` が 1）。これを `Value::detach_list` / `detach_dict`（COW の複製と heap の課金）の前に求めて fuel を課金し、超過なら対象を変えずに `ControlStop`（`BudgetExceeded(Fuel)`）を返す。fuel を heap より先に見るのは §7.2 の優先順位（`Fuel` が `HeapBytes` より先）と同じ向きにするためである。判定を読み直しの実行（`settle_child_depth`）と食い違わせないよう、判定の関数を 1 つにして両方から使う（例: 判定の結果を `settle_child_depth` に渡す）。
  - tree-walk 版（`src/eval.rs` の `budget_list_pop`・`src/builtin_core.rs` の index 代入）と VM 版（`src/vm.rs` の `OpCode::SetIndex`・`OpCode::PopUpdate`）は、どちらも上の `src/value.rs` の primitive を通るので、課金の位置と量は両 backend で同じになる。呼び出し側は primitive の `ControlStop` を既に `control_stop_to_error` で運んでいるので、変えない見込みである。
  - VM の Dict リテラルの構築（`OpCode::DictInsert`）も `index_set_tracked` を通るため、重複した key で最も深い値を浅い値で上書きすると読み直しが起きる。tree の Dict リテラル（`src/eval.rs` の `Expr::Dict`）は `BTreeMap` に入れてから最後に 1 回だけ要約するので読み直さない。§4.1 の「lowering差をscriptのfuel差にしない」のとおり、リテラルの構築では課金しない（`DictInsert` は課金しない経路で primitive を呼ぶ）。課金の対象はスクリプトの index 代入・`pop` だけである。
    - **決定記録（判断役が選択（PR #349）、Q2=A）:** 理由: VM の `DictInsert` の読み直しは lowering の差で、§4.1 の「lowering差をscriptのfuel差にしない」が script の fuel に出さないと定めている。tree と VM の fuel の一致を保ち（[マニフェスト](manifesto.md)原則 5）、変更は値の表現 1 つに収まる（原則 8）。VM に残る課金しない読み直しは、回数も 1 回の量もリテラルの entry 数で決まり、host が `max_single_source_bytes` と `max_collection_elements` で上限を制御できる（原則 1・3）。採らなかった案: B（tree の Dict リテラルでも同じ条件で課金する）は tree が読み直さないのに課金し、VM の lowering の都合を script の fuel に出す。C（VM の `DictInsert` も読み直さないようにする）は VM のリテラルの lowering と primitive の両方を変え、触る仕組みが 2 つになる（[開発ループ手順書](loop-runbook.md) §14.1）。必要になれば別のスライスで扱える。
  - [言語仕様](language-spec.md)「値のネスト深度制限」の「課金」の行を §21.3 に合わせる（読み直しのときだけ、読み直した子の数ぶん fuel を課金する。heap には課金しない）。`fuel_accounting_revision` と `language_revision` の扱いは下の「課金の変化」。
  - done にする実装 PR で、本書の未実装範囲の表から「§4.1 の『List / Dict の深度の読み直し』の課金」の行を削除する。tree の slice fuel が数えないもの（逸脱一覧の 1 行目。builtin が台帳へ直接課金する per-element fuel）に読み直しの課金も入るので、その行に index 代入・`pop` の読み直しを足す。
- **範囲外:**
  - 深度ごとの子の数の histogram（§21.3 の決定記録で採らなかった案）。
  - VM の Dict リテラルの重複 key での読み直しそのもの（課金しないまま残る）。回数と 1 回の量はリテラルの entry 数、つまり source の大きさ（`max_single_source_bytes`）で上限がある（判断役が選択（PR #349）、Q2=A）。
  - COW の複製（`detach_list` / `detach_dict` が共有中の backing を要素ごとに複製する）への per-element fuel 課金。§4.1 の「collection elementを走査・copy・生成」の一部で、`fuel-bulk-charge` の範囲。
  - tree の slice fuel に読み直しの課金を数えること（逸脱一覧 1 行目の直し方と同じ。§4.2）。
  - `BudgetCharged` 監査 event の reason の細分。読み直しの課金は他の `charge_fuel` と同じく `Operation` に集計される（`src/budget.rs` の `charge_reason_for`）。
  - `fuel_accounting_revision` の field の実装（F-2 の範囲）。
- **課金の変化:** 深度の読み直しが起きる index 代入・`pop` だけ、読み直した子の数ぶん fuel が増える。step・heap・peak_heap と、読み直しの起きない操作の fuel は変わらない。fix-value-nesting-depth-limit の VND-AT-05（「fuel・`live_heap` / `peak_heap` の値が変わらない（深度の計算は課金しない）」）は、その小節を書いた時点の記録として残し、このスライスの後は fuel について「読み直しの起きない script では変わらない」と読む（下の VDS-AT-04。`tests/value_nesting_depth_limit.rs` の代表 script は読み直しを起こさないので期待値は変わらない見込み。VND-AT-03 の時間のテストは最後に 1 回だけ 10 万要素を読み直し、設計の際の計測で tree の fuel は変更前 200,254 なので、課金後も既定の `total_fuel`（1,000,000）に収まる）。fuel の会計の変更なので `language_revision` は変えない（§3.1）。`fuel_accounting_revision` は 1 のままにし、§3.1 の「初期版は `1`」の説明に、field を実装する F-2 より前に足した課金点（このスライスの読み直し）は版 1 に含めると書き足した（F-2 は予定どおり 1 → 2）。
  - **決定記録（判断役が選択（PR #349）、Q1=A）:** 理由: `fuel_accounting_revision` は `BudgetConfig` にも監査の `ExecutionStarted` にもまだ無く、版を上げても host が選べる課金・拒否すべき旧版の指定・監査で区別される記録のどれも無い。§3.1 に版の説明を書けば spec の一貫性は保てる（[マニフェスト](manifesto.md)原則 9）。触る仕組みを値の表現 1 つに保ち（原則 8、[開発ループ手順書](loop-runbook.md) §14.1）、F-2 の設計（PR #241）とスライスの境界を変えない。採らなかった案: B（このスライスで field を実装して 1 → 2、F-2 は 2 → 3）は F-2 の範囲（field・config error の区別・監査 event の field）を持ち込み、触る仕組みが 3 つに増える。C（文書の上だけで版を 2 にする）は指定も観測もできないまま番号だけが進み、版と実装の対応が崩れる（原則 7・9）。
- **想定の変更規模:** `src/` は 30〜80 行程度（`src/value.rs` の読み直しの判定の関数 15〜30 行、3 か所の課金 10〜25 行、`settle_child_depth` の判定の共有 5〜15 行、呼び出し側でエラーを運ぶ変更があれば 0〜10 行）。テストは `tests/` と `src/value.rs` の unit test で 150〜250 行。docs は言語仕様の 1 行、本書の逸脱一覧・未実装範囲。触る仕組みは「値の表現（List / Dict の backing の深度の更新）」1 つ。受入基準は 4 項目。§14.1（[開発ループ手順書](loop-runbook.md)）に収まる。
- **受入基準:**
  - **VDS-AT-01（読み直した子の数ぶんを課金）:** 読み直しが起きる操作（最も深い子がただ 1 つの List の index 代入・Dict の既存 key の上書き・`pop`）の fuel の増分が、課金の無い同じ形の操作との差で、読み直した子の数（List の要素数・Dict の entry 数・`pop` の後の要素数）に一致する。読み直しの起きない操作（同じ深度の子が残る取り除き、より深い値・同じ深度の値を入れる index 代入、Dict の新しい key、`push`）と、最も深い値を重複した key で浅い値に上書きする Dict リテラル（`{"a": e, "b": 1, "a": 1}`。VM だけが内部で読み直す）の fuel は変更前と同じである。tree-walk 版と VM 版で fuel が一致する。確かめ方: 同じ script を両 backend で実行して `committed.fuel` を比べる統合テストと、`src/value.rs` の判定の unit test。
  - **VDS-AT-02（往復は fuel で止まる）:** issue #304 の敵対的入力（要素数 50,000 の List の最も深い要素の `xs[0] = 0` / `xs[0] = e` の往復と、最も深い末尾の要素の `pop` / `push` の往復を、`while true` で繰り返す）と、Dict の同じ形が、既定の budget で `BudgetExceeded(Fuel)` の terminal で止まり、壁時計の上限（例: 10 秒）以内に終わる。`try` / `catch` の内側でも catch されない。確かめ方: CLI を subprocess で起動して、exit code・stderr の fuel 超過の診断・経過時間を確かめるテスト（両 backend）。
  - **VDS-AT-03（値を変える前に課金する）:** 残りの fuel が読み直す子の数より少ないときの index 代入・`pop` が `BudgetExceeded(Fuel)` で止まり、超過の記録（`resource`・`requested`）が読み直す子の数を示し、対象の backing を共有する別名があっても COW の複製の heap を課金しない（`live_heap` / `peak_heap` が操作の前と同じ）。tree-walk 版と VM 版で同じ resource・同じ committed fuel になる。確かめ方: 小さい `total_fuel` を設定した実行で usage と超過の記録を比べるテスト（両 backend）。
  - **VDS-AT-04（ほかの観測は不変）:** 既存のテスト（`tests/value_nesting_depth_limit.rs` の VND-AT-01..05 と `tests/` 一式、unit test）が期待値の変更なしで通る。読み直しの起きない script で fuel・step・`live_heap` / `peak_heap` が変更前と同じで、tree と VM の differential test が一致する。確かめ方: 既存のテスト一式。期待値を変えたテストがあれば、読み直しが起きることを PR 本文に示す。

### fix-vm-dict-literal-depth-settle 実装スライス（VM の Dict リテラルの構築で深度を読み直さない）

[issue #374](https://github.com/kamonabe1927/Tsumugi/issues/374)（kamonabe が PR #349 の判断 Q2 を A から C へ変えた `判断:` の反映）から作ったスライス（`slices.toml` の `fix-vm-dict-literal-depth-settle`）。目標の規範は[次期意味論・実装決定](semantic-decisions.md) §21.3 の「リテラル」で、このスライスの設計 PR で足した。現在の差は同書の逸脱一覧の行にある。上の fix-value-depth-settle-quadratic 小節の Q2=A（VM の読み直しを課金しないまま残す）は、その時点の記録として残す。

- **現状の不具合:** VM の Dict リテラルは、空の Dict の定数を積んだあと、key と値の組ごとに `OpCode::DictInsert` で 1 つずつ入れる（`src/compiler.rs` の `Expr::Dict`）。`DictInsert` は `Value::index_set_tracked_uncharged_settle`（`src/value.rs`）を通り、値を入れるたびに backing の深度の要約（`child_max`・`child_max_count`）を更新するので、重複した key で最も深い値を浅い値で上書きすると、その場で全 entry の保存済み深度を読み直す（課金しない。PR #349 の Q2=A）。tree の Dict リテラル（`src/eval.rs` の `Expr::Dict` の 2 経路）は `BTreeMap` に入れてから `Value::new_dict` で最後に 1 回だけ要約するので読み直さない。リテラルはループで何度も評価できるので、`{"a": e, "a": 1, "a": e, "a": 1, …}` のように上書きを繰り返すリテラルを評価するたびに「entry 数 × 上書きの回数」の読み直しが起き、合計の時間と fuel の比が source の大きさに比例して開く（issue #374）。
- **実装範囲:**
  - VM の Dict リテラルの構築を tree と同じ形にする。key と値の組を入れるあいだは backing の深度の要約を更新せず（読み直しもしない）、リテラルを作り終えたときに 1 回だけ全 entry から要約する（`Tracked::summarized` と同じ要約。tree の `Value::new_dict` と同じ量の仕事）。要約は、リテラルの値がスタックから使われる前（外側のリテラルの `ListPush` / `DictInsert` の深度の検査、代入、引数など）に済ませる。作り終えたことを VM が知るための命令（例: 空でない Dict リテラルの最後に 1 回だけ出す命令、または最後の `DictInsert` の operand）は実装 PR で決めてよい（`OpCode` は `unstable-bytecode` feature の下でだけ公開され、安定した公開 API ではない）。空の Dict リテラル `{}` は今と同じ（定数の要約は 0 で正しい）。verifier（`src/verifier.rs`）が新しい命令・operand を受け付けるようにする。
  - List / Dict の backing（`Tracked`）に field を足さない（「要約が古い」の印などを持たない。[次期意味論・実装決定](semantic-decisions.md) §21.3 の決定記録で histogram を採らなかったのと同じく、§5.1 の論理サイズに入らないメモリを backing ごとに足さない）。構築中の Dict はスクリプトから見えず（スタックの上にだけあり、key・値の式からは参照できない）、エラーで構築を抜けるときは捨てられるので、要約の無い backing は外へ出ない。
  - 値を入れるたびの検査は今と同じ位置・同じ順に残す: key の型、新しい key のときの `check_collection`、入れる値の深度の検査（`Value::check_element_depth`。PR #301 の判断 Q1=A）、heap の delta 課金（`retrack_dict`）。`{"a": e, "a": 1}`（`e` は深度 256）は今と同じく `overflow` になる。
  - 課金は足さない。リテラルの構築は今も読み直しに課金していないので、fuel は変わらず、tree と VM の fuel の一致を保つ（§4.1「その他のVM内部opcode dispatch 0」。新しい命令を足す場合もその dispatch は 0）。スクリプトの index 代入・`pop` の読み直しの課金（fix-value-depth-settle-quadratic、PR #371）はそのまま残す。
  - 使われなくなる `Value::index_set_tracked_uncharged_settle` と `index_set_impl` の `charge_settle` 引数は削除してよい（`index_set_tracked` の挙動は変えない）。
  - done にする実装 PR で、[次期意味論・実装決定](semantic-decisions.md) の逸脱一覧から「VM の Dict リテラルの構築で深度を読み直す」行を削除する。
- **範囲外:**
  - `retrack_dict`（Dict の heap の delta 課金が、挿入のたびに全 key の byte 長を数え直す）による、Dict の構築が entry 数の 2 乗の時間になる件。仕組みが別なので、`slices.toml` の `fix-dict-retrack-key-bytes`（`todo`）で扱う（issue #374）。このスライスの後も、重複した key を上書きするたびに `retrack_dict` が全 key を数え直すので、VM の Dict リテラルの構築の時間は「entry 数 × 挿入の回数」のままである（読み直しと同じ形）。そのため、issue #374 の「重複した key を含むリテラルをループで評価したとき、時間が fuel に比例する」ことの壁時計での確かめは、このスライスでは読み直しの回数で確かめ（VDL-AT-01）、時間は `fix-dict-retrack-key-bytes` の受入基準に置く。issue #374 の目標（時間と fuel の比を source の大きさに依らず保つ。[マニフェスト](manifesto.md)原則 1・3）は 2 本がそろって満たされ、`fix-dict-retrack-key-bytes` が done になるまでは未達である（冒頭の逸脱一覧の `retrack_dict` の行）。
    - **決定記録（判断役が選択（PR #447）、Q1=A）:** 理由: issue #374 は「時間が fuel に比例することを受入基準で確かめる」と「`retrack_dict` はこのスライスに含めず別の fix にする」の両方を指示している。`retrack_dict` は挿入のたびに全 key の byte 長を合計し直し、`DictInsert` は上書きのときもこれを通るので、このスライス単独では時間が fuel に比例せず、壁時計では確かめられない。このスライスで直す性質（構築の途中で読み直さない）は読み直しの回数という決定的な量で確かめ（原則 9「再現可能なテストと計測」）、時間の確かめはそれが実際に成り立つ `fix-dict-retrack-key-bytes`（depends_on このスライス）に置く。2 つの指示をどちらも落とさず、スライスごとに触る仕組みを 1 つに保てる（原則 8、[開発ループ手順書](loop-runbook.md) §14.1）。採らなかった案: B（`retrack_dict` の直しをこのスライスに含める）は issue #374 の明示の指示に反し、触る仕組みが VM の lowering と heap の delta 課金の 2 つになる。C（時間の受入基準をこのスライスに置き、「変更前の半分以下」のような相対の壁時計の上限で確かめる）は `retrack_dict` の 2 乗が残るので比例を確かめたことにならず、計測も環境に左右されて再現しにくい（優先順位の表「再現可能な検証」、原則 9）。
  - List リテラル（`ListPush` は子を足すだけで読み直さない）。
  - tree のリテラルの構築（変えない）。
  - F-2 の式・操作の課金点（リテラルの entry ごとの fuel）。現状の fuel は文・反復・呼び出しだけを数える（冒頭の未実装範囲）。
- **課金の変化:** なし。fuel・step・`live_heap` / `peak_heap` は変わらない。`fuel_accounting_revision` と `language_revision` は変えない。
- **想定の変更規模:** `src/` は 40〜100 行程度（`src/compiler.rs` の `Expr::Dict` の lowering 2〜6 行、`src/opcode.rs` 0〜5 行、`src/verifier.rs` 0〜10 行、`src/vm.rs` の `DictInsert` と終わりの処理 10〜30 行、`src/value.rs` の要約を更新しない挿入と最後の要約 15〜35 行、`index_set_tracked_uncharged_settle` の削除 −10〜−20 行）。テストは `tests/` と unit test で 100〜200 行。docs は semantic-decisions の逸脱一覧の 1 行の削除。触る仕組みは「VM の Dict リテラルの lowering（compiler の出す命令・VM の dispatch・それが呼ぶ value の primitive）」1 つ。受入基準は 4 項目。§14.1（[開発ループ手順書](loop-runbook.md)）に収まる。
- **受入基準:**
  - **VDL-AT-01（構築の途中で読み直さない）:** VM で、重複した key で最も深い値を浅い値に上書きすることを繰り返す Dict リテラル（例: 1,000 個の key と、`"a": e` / `"a": 1` の組を 1,000 回）を評価したとき、全 entry の深度の読み直しがリテラル 1 つにつき作り終えたときの 1 回だけで、tree と同じ回数である。ループでそのリテラルを L 回評価したとき、読み直しの回数は L に比例し、entry 数 × 上書きの回数に比例しない。確かめ方: 全 entry から深度を要約する処理の呼び出し回数を数える unit test（例: `cfg(test)` の counter）を、両 backend で同じ script について比べる。
  - **VDL-AT-02（深度は正確）:** 重複した key で最も深い値を浅い値に上書きした Dict リテラルの深度は、作り終えた内容で決まる（`{"a": e, "b": 1, "a": 1}` は値が `1` と `1` だけになるので、深度は `e` の深度に依らず 1。スカラーの深度は 0、Dict の深度は「1 + 値の深度の最大」、[次期意味論・実装決定](semantic-decisions.md) §21.3）。その値をさらに入れ子にした値が 256 段まで作れ 257 段で `overflow` になり、tree と VM で同じ kind・message・line になる。入れ子のリテラル（`{"x": {"a": e, "a": 1}}`、`[{"a": e, "a": 1}]`）でも同じ。確かめ方: 両 backend の paired test。
  - **VDL-AT-03（検査の位置と課金は不変）:** 構築の途中のエラー（key の型、`max_collection_elements` の超過、値の深度の `overflow`、heap の超過）が今と同じ位置・同じ順で起き、`[e, print("side")]` と `{"a": e, "a": 1}`（`e` は深度 256）は両 backend で今と同じく `overflow` になる。重複した key を含む Dict リテラルの fuel・step・`live_heap` / `peak_heap` が変更前と同じで、tree と VM で fuel が一致する（VDS-AT-01 のリテラルの場合を含む）。確かめ方: 両 backend の paired test と既存の VND-AT-01・VDS-AT-01 のテスト。
  - **VDL-AT-04（ほかの観測は不変）:** 既存のテスト一式が期待値の変更なしで通る。ただし `src/compiler.rs` の bytecode の期待文字列は、Dict リテラルの終わりの命令・operand の変化に限って更新してよい（変えた行を PR 本文に示す）。確かめ方: 既存のテスト一式。

### fix-fs-write-content-budgeted-render 実装スライス（`write_file` / `append_file` の内容の予算付き描画）

[issue #356](https://github.com/kamonabe1927/Tsumugi/issues/356) から作ったスライス（不具合の修正、`slices.toml` の `fix-fs-write-content-budgeted-render`）。issue の仕分けで「受け入れ」とした。直し方は §6.1（capability 判定前の count と request bytes の課金）・§7（外部効果・allocation の前の reserve）と、この設計 PR で §6.1 に書き足した「request payload の描画は予算付き」の段落で決まる。

- **現状の不具合:** `write_file` / `append_file` の第 2 引数が `Str` でないとき、`src/builtin_core.rs` の `host_call_request_bytes` は request bytes を求めるために `other.to_string()`（`Value` の `Display`）で値全体を描画する。`Display` は fuel も `max_single_string_bytes` も見ずに再帰し、共有された入れ子（DAG）を枝刈りしない。この描画は tree（`src/builtin.rs` の `apply_pure_core_builtin`）でも VM（`src/vm.rs` の `exec_builtin`）でも capability 判定（`dispatch_fs`）より前に走るので、`a = [a, a]` を 64 回繰り返した値を渡すと、fs の grant が無い既定の safe profile でも 60 秒の timeout まで止まらない（両 backend。issue #356 の再現）。24 回なら約 16 秒かけて描画し切ってから `HostRequestBytes` の超過になる。本体の書き込み（`fs_content_arg`）と監査の request body（`host_call_body_values`）も同じ `Display` を使う。`print(a)` は REV-001 で予算付きの `render_display` に移っており、同じ値で fuel の超過で即座に止まる。
- **実装範囲:**
  - tree（`apply_pure_core_builtin`）と VM（`exec_builtin`）の request 課金の位置で、`write_file` / `append_file` の第 2 引数を `BudgetLedger::render_display` で 1 回だけ描画する（`Str` はこれまでどおり描画せずにそのまま使う）。描画が `ControlStop` を返したら、`print` と同じく `control_stop_to_error` で terminal にし、request の課金・capability 判定・adapter 呼び出しを行わない。tree の式 frame 経路（tree-expr-continuation-e3）も `apply_pure_core_builtin` を通るので同じになる。
  - 順序は、host call 開始前の flush（tree の `flush_charges_into_audit_events`、[決定性・実行時監査仕様](determinism-and-audit.md) §7.3）→ 描画 → request の課金（`charge_host_call_request`）→ dispatch（`dispatch_fs`）とする。flush の位置は今と同じで、第 2 引数が `Str` かどうかで変わらない。描画の fuel は request bytes と同じく host call 自身の課金として扱い、監査付き実行では次の flush 点、つまり `HostCallStarted` の後の `BudgetCharged` に出る。VM は監査の flush を持たないので、描画 → request の課金 → dispatch の順だけを守る。
    - **決定記録（判断役が選択（PR #381）、Q2=A）:** 理由: マニフェストに反しうる選択肢は無く、A でも B でも描画は予算付きで capability 判定の前に止まり（原則 1・3）、event 列は決定的で（原則 5）、描画の fuel は `BudgetCharged` に出て Terminal の usage と突合できる（原則 7）。そこで既存の決定との一貫性・変更の小ささ（原則 8）・検証のしやすさ（原則 9）で比べた。[決定性・実行時監査仕様](determinism-and-audit.md) の fix-audit-flush-before-host-call 小節（D-50 の Q1=A）は、flush の位置を fs host call の処理の先頭（request 側の課金より前）とし、その後に積まれるものを host call 自身の課金として次の flush 点で出すと決めている。A はこの位置を保ったまま、今 `host_call_request_bytes` が描画している場所を予算付きの描画に置き換えるだけで済む。描画は request payload を作る作業で、request bytes はその長さなので、request bytes と同じ群に入れるのは §6.1（描画結果を request bytes・payload・監査の body で共有する）とも合う。flush の位置が第 2 引数の型で変わらないので、「host call より前の消費」の範囲を 1 つの規則で説明できる。採らなかった案: B（flush の前に描画し、描画の fuel を `HostCallStarted` より前の `BudgetCharged` に出す）は、fix-audit-flush-before-host-call 小節が決めた flush の位置づけを request payload を作る作業についてだけ変える。host call 前の消費に含まれるものが第 2 引数の型で変わり、説明する規則が 1 つ増える。マニフェストには反しないが、既存の決定との一貫性と変更の小ささ（原則 8）で A に劣る。
  - 描画した文字列を、request bytes の算出・`dispatch_fs` の書き込み内容・tree の監査の request body（`host_call_body_values`）で共有する。`host_call_request_bytes`・`fs_content_arg`・`host_call_body_values` から `Value` の `Display` による描画を無くす（描画済みの内容を受け取る形にするなど。関数の形は実装に任せる）。
  - 引数が 2 つ未満のときは、これまでどおり描画せず request bytes を 0 とし、arity のエラーは `dispatch_fs` が返す。
  - done にする実装 PR で、本書の逸脱一覧から `write_file` / `append_file` の描画の行を削除する。
- **範囲外:**
  - 第 2 引数が `Str` のときの挙動と課金（変えない）。
  - 描画した payload の文字列への heap・string allocation の課金。`print` の描画と同じく、script から見える値を作らないので課金しない（`to_str` は結果の `Str` を `track_result` で課金するが、ここでは値を返さない）。
  - capability 判定・path の形式の検査と、request の課金・描画との順序（§6.1 のとおり request の課金を先にする現行の順序を変えない）。このため、fs の grant が無い場合も描画が予算を超えれば `sandbox` エラーでなく予算超過の terminal になる（issue #356 の「期待する挙動」のうち「grant が無いので `sandbox` エラーで即座に止まる」は、§6.1 の「capability 拒否でも count と request bytes は課金」と合わないので採らない。描画が予算内で終われば従来どおり `sandbox` エラーになる）。
  - 描画の途中での deadline の確認。`render_display` は複合ノードごとに fuel と cancel を確認し、描画は fuel と `max_single_string_bytes` で有限に上限されるので、deadline は従来どおり次の確認点で効く。
  - VM の監査（VM は host-call triple を組まない。[決定性・実行時監査仕様](determinism-and-audit.md) の逸脱一覧）。
- **課金の変化:** 第 2 引数が List / Dict（またはそれを含む値）の `write_file` / `append_file` は、`print` と同じく描画した複合ノードごとに fuel 1 を使うようになる。`Str`・スカラーの第 2 引数の fuel は変わらない。request bytes の値は描画結果の長さで、変更前の `Display` の長さと同じ（`render_display` は `Display` と同じ文字列を作る。`src/budget.rs` の unit test `render_display_matches_existing_display`）。これは §4.1 の課金表に charge point を足す変更ではなく、`print` / `to_str` がすでに使う描画の課金を host request の描画にも適用する修正なので、`fuel_accounting_revision`（§3.1）の版は変えない（fix-contains-budgeted-equality と同じ扱い）。`language_revision` も変えない。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 32（fix-heap-charge-audit-notify）で、0〜31 は done を含めてすべて使われていた。そのため設計 PR では仮にすべてのスライスより後（97）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR を遅らせずに出して、今ある `ready` より前へ移す。§8 は付け直しの PR を「先に出す」と定めており、この扱いは順序が逆（設計 PR のマージ後）になる。この外れをここに記録として残す。付け直しの PR が出るまで、このスライスは §8 の既定を満たしていない。
  - **決定記録（判断役が選択（PR #381）、Q1=A）:** 理由: A でも B でも、fix は今ある `ready` より前に置かれる。`order` は着手順だけを決め、言語挙動・権限・予算には触れないので、どちらも[マニフェスト](manifesto.md)に反しない。そのうえで A は、§8 が定める「付け直しだけの状態整理 PR」への分け方を保つ。設計 PR の差分は fix の設計だけになり、既存スライスの着手順の変更は単独で検証できる（原則 8・9）。同じ状況の先例（PR #368 の Q1=A と、それを反映した PR #370）とも一致する。採らなかった案: B（この PR の中で既存スライスの `order` を付け直す）は、設計 PR に付け直しが混ざって §8 に沿わず、差分とレビュー・審査の範囲が広がる（原則 8・9）。C（97 のまま付け直さない）は、fs の grant が無くても止まらない経路の fix を理由なく §8 の既定から外し、最後尾に固定する。原則 1・3 に反しうるので選ばない。
- **想定の変更規模:** `src/` は 50〜120 行程度（描画の共有の関数 15〜30 行、`host_call_request_bytes`・`fs_content_arg`・`host_call_body_values` と `dispatch_fs` の引数の変更 20〜50 行、`src/builtin.rs` と `src/vm.rs` の呼び出し側 各 10〜20 行）。テストは `tests/` に 150〜250 行。触る仕組みは「filesystem host call の request payload の作り方」1 つ。受入基準は 4 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **FWR-AT-01（grant 無しで有限時間に止まる）:** issue #356 の再現（`a = [a, a]` を 64 回繰り返した値の `write_file("x.txt", a)`、`append_file` も同じ）を既定の safe profile（fs の grant 無し）の CLI で実行すると、壁時計の上限（例: 10 秒）以内に予算超過（`print(a)` と同じ fuel の超過の診断）の terminal で終わり、`x.txt` が作られない。tree-walk 版と VM 版で同じ。確かめ方: CLI を subprocess で起動し、exit code・stderr・経過時間・ファイルの有無を確かめるテスト（両 backend）。
  - **FWR-AT-02（grant 有りでも外部効果の前に止まる）:** fs の write を grant した実行で、描画が fuel または `max_single_string_bytes` を超える第 2 引数の `write_file` / `append_file` が、adapter を呼ばずに `BudgetExceeded` の terminal で止まり、`try` / `catch` で捕捉されない。committed の `host_request_bytes` と `host_calls` は増えない。これを、描画が fuel を超える場合（`BudgetExceeded(Fuel)`）と描画が `max_single_string_bytes` を超える場合（`BudgetExceeded(SingleStringBytes)`）のそれぞれで確かめる。確かめ方: 呼び出しを記録する fake の filesystem adapter（または一時 directory の root）を使い、2 つの資源の超過それぞれについて、超過の resource・adapter 呼び出し 0・ファイル無し・usage を確かめるテスト（両 backend）。
  - **FWR-AT-03（書き込み内容と request bytes は変わらない）:** 予算内の List / Dict / 入れ子 / スカラー（Int・Float・Bool・Null）を第 2 引数にした `write_file` / `append_file` の書き込まれた内容と committed の `host_request_bytes` が変更前と同じ（`Display` の描画と同じ文字列・同じ長さ）で、`Str` の第 2 引数の fuel・`host_request_bytes` も変更前と同じである。tree-walk 版と VM 版で同じ。確かめ方: 一時 directory に書いた内容と usage を比べる統合テスト（両 backend）。
  - **FWR-AT-04（描画の fuel は `print` と同じ・監査の body も同じ）:** 同じ値を `print` したときと `write_file` したときの描画による fuel の増分が一致する。body の Reveal を許す redaction policy の監査付き実行（tree）で、`HostCallStarted` の request body が書き込まれた内容と同じ文字列になる。監査付き実行（tree）で、描画の fuel を含む `BudgetCharged` は、その host call の `HostCallStarted` の後に出る（host call より前の消費の `BudgetCharged` は従来どおり `HostCallStarted` の前で、描画の fuel を含まない）。確かめ方: usage の fuel を比べるテスト（両 backend）と、`tests/audit_redaction.rs` か `tests/audit_hostcall.rs` の形の監査のテスト（request body と、`BudgetCharged` と `HostCallStarted` の相対順と、それぞれの fuel の `committed_delta`）。

### fix-for-iter-snapshot-charge 実装スライス（`for` の反復列を共有・課金する）

[issue #358](https://github.com/kamonabe1927/Tsumugi/issues/358) から作ったスライス（不具合の修正、`slices.toml` の `fix-for-iter-snapshot-charge`）。issue の仕分けで「受け入れ」とした。直し方は §5.2（`Rc` 共有は追加課金しない・新 object は allocation 前に課金する）・§5.3（新規 String body を累積に数える）と [次期意味論・実装決定](semantic-decisions.md) §11.2 の「for は開始時の Rc snapshot を保持する」で決まる。この設計 PR で §5.2 に `for` の反復列の項を書き足した（新しい規則ではなく、§5.1〜§5.3 と §11.2 を `for` に当てはめたもの）。

- **現状の不具合:** 冒頭の逸脱一覧の `for` の反復列の行のとおり。tree の `materialize_for_items`（`src/eval.rs`）は List を `(**list).clone()` で全要素複製し（要素は `Rc` の複製だが、要素数ぶんの `Vec<Value>` を未課金で確保する）、Dict の key・Str の文字は `Value::str_constant` の未課金の `Str` で作る。結果の `Vec<Value>` は `FrameKind::For` の `items` に入り、frame は `heap_charge: 0` である。同期の `exec_stmt` 経路と、式 frame から反復元を受け取る `CallDest::ForIter` 経路の両方が同じ関数を通る。issue #358 の再現（`xs = range(0, 1000000)` を深さ 120 の再帰の各段で `for`）は、tree では深さの数だけ未課金の複製が積み上がって Rust の allocation 失敗で abort（exit 134）し、`--vm` は `done` を出して完了する。VM の `ToIterList`（`src/vm.rs`）は List をそのまま積む（共有）が、Dict・Str は `str_constant` の要素で `Value::new_list` を作るので、List body（24 + 32 × 要素数）だけが課金され、要素の String body と §5.3 の累積は数えない。VM は要素を作ってから `check_collection` を呼ぶ。
- **実装範囲:**
  - **List:** tree の反復列は反復元の List の backing を `Rc` で共有して持ち（`Value::List` の clone）、要素を複製しない。For frame は共有した List と次の index を持ち、反復ごとに要素を 1 つ取り出す（要素の `Value` の clone）。`heap_charge` は 0 のまま（共有は追加課金しない、§5.2）。反復中に元の binding を変更・再代入しても、変更側が COW で detach するので反復列は開始時の要素のまま（§11.2。今の観測挙動を変えない）。VM の List の扱いは今のまま。
  - **Dict・Str:** 反復列を作る処理を tree と VM で 1 つの関数にまとめ（例: `src/value.rs` か `src/budget.rs` に `BudgetLedger` を受け取る関数を置く。形は実装に任せる）、両 backend がそれを通る。手順は次の順とする。
    1. `check_collection(要素数)`（今と同じ上限・同じエラー。VM も要素を作る前に検査する）。
    2. 反復の順（Dict は key のコードポイント順、Str は文字の順）に要素ごとに、§5.3 の累積（`charge_string`：`string_allocations` +1・`string_bytes` + byte 長・`max_single_string_bytes`）を課金してから文字列を複製し、§5.1 の String body（24 + byte 長）を live heap に課金した tracked な `Str` にする。
    3. 最後に List body（24 + 32 × 要素数）を課金した tracked な List にする。
    量と順序は builtin の List 結果に `track_result`（`src/budget.rs`）がかけるもの（子の `Str` を累積 → String body の順に課金し、最後に List body）と同じである。どこかで上限を超えたら、その時点の `ControlStop` を `control_stop_to_error` で terminal にし（catch できない予算超過）、作りかけの要素は drop で release する。1 要素を複製する前に累積の課金（`max_single_string_bytes` の検査を含む）を済ませるので、未課金の確保は高々 1 要素ぶんで、その大きさは単一文字列の上限で抑えられる。
  - tree の For frame は、Dict・Str でもこの tracked な List を共有した List と同じ形で持つ（List / Dict / Str で frame の形を分けない）。反復列はループの終わり（最後の要素を処理した後・`break`・`return`・error による frame の巻き戻し）で frame とともに drop され release される。反復元の値（今の `_collection`）を frame が保持し続けるかは今のままでよい（live heap の寿命を変えない）。
  - done にする実装 PR で、本書の逸脱一覧の `for` の反復列の行と、[言語仕様](language-spec.md) の逸脱一覧の同じ行を削除する。
- **範囲外:**
  - Dict・Str の反復列を、開始時に全部作らず反復ごとに 1 要素ずつ作る形（遅延）への変更。今の両 backend の形（開始時にまとめて作る。VM の `ToIterList` の「Dict → keys のリスト、Str → 1 文字ずつのリスト」）を保ち、課金だけを §5.1〜§5.3 に合わせる。遅延にすると live heap・peak・`string_allocations` の数え方と超過の位置が変わり、VM の命令も変わるので、ここでは扱わない。
  - `for` の fuel（反復ごとの `count_step`）。変えない。
  - `map` / `filter` / `each` など、`for` 以外で collection を反復する builtin の課金。
  - [次期意味論・実装決定](semantic-decisions.md) §11.10（実装状況）の「for は `(**list).clone()` で開始時点の要素を snapshot する」は当時の記録なので書き換えない（規範は §11.2 で、本スライスはそれに合わせる）。
- **課金の変化:**
  - List の `for`: heap・peak_heap・fuel・string の累積は変わらない（今も複製は未課金で、共有にしても課金は増減しない）。物理的な確保だけが要素数ぶん減る。
  - Dict・Str の `for`: tree は反復列の List body と各要素の String body が live heap・peak_heap に載り、要素数と byte 長の合計が `string_allocations`・`string_bytes` に加わる。VM は各要素の String body と同じ累積が加わる（List body は今も課金している）。どちらもループの終わりで live heap は元に戻る（累積は減らない）。この結果、たとえば 100 万文字の文字列の `for` は既定の `max_string_allocations`（1,000,000）に当たりうる。§5.1 の論理サイズ表と §4.1 の課金表は変えず、既存の表の対象（新規の List body・String body と §5.3 の累積）への課金の漏れを埋める修正なので、`heap_accounting_revision`・`fuel_accounting_revision`・`language_revision` は変えない（fix-push-snapshot-writeback と同じ扱い）。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜34 は done を含めてすべて使われていた。そのため設計 PR では仮にすべてのスライスより後（97）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR を遅らせずに出して、今ある `ready` より前へ移す。97 は仮の値である。§8 は付け直しの PR を「先に出す」と定めており、この扱いは順序が逆（設計 PR のマージ後）になる。この外れをここに記録として残す。付け直しの PR が出るまで、このスライスは §8 の既定を満たしていない。
  - **決定記録（判断役が選択（PR #388）、Q1=A）:** 理由: issue #358 の不具合は heap 上限に当たらないままプロセスが abort する経路で（原則 1・3・6 に反する現状）、その fix を理由なく最後尾に固定する C は原則 1・3 に反しうるので外した。A でも B でも fix は今ある `ready`（35）より前に置かれ、`order` は着手順だけを決めて言語挙動・権限・予算に触れないので、どちらも[マニフェスト](manifesto.md)に反しない。そのうえで、0〜34 が done を含めてすべて使用済みで `tests/slices_contract.rs` が `order` の重複を弾くため、この PR の中では既存の `order` を変えずに既定の位置に置けない。A は §8 が定める「付け直しだけの状態整理 PR」への分け方を保ち、設計 PR の差分は fix の設計だけになり、既存スライスの着手順の変更は単独で検証できる（原則 8・9）。同じ状況の先例（PR #368・#381 の Q1=A）とも一致する。採らなかった案: B（この PR の中で既存の `ready`・`todo` の `order` を付け直す）は、設計 PR に多数の付け直しが混ざって §8 に沿わず、差分とレビュー・審査の範囲が広がる（原則 8・9）。C（97 のまま付け直さない）は上のとおり原則 1・3 に反しうるので選ばない。
- **想定の変更規模:** `src/` は 60〜140 行程度（Dict・Str の反復列を作る共通の関数 30〜60 行、tree の `materialize_for_items`・`FrameKind::For`・`enter_for`・反復の取り出しの書き換え 20〜50 行、VM の `ToIterList` を共通の関数へ寄せる 10〜30 行）。テストは `tests/` に 150〜300 行。触る仕組みは「`for` の反復列の作り方」1 つ（tree の 2 経路と VM が同じ規則を通る）。受入基準は 4 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **FIS-AT-01（List の反復で複製しない）:** 関数の再帰の各段で同じ List を `for` する script（issue #358 の再現を、CI で走る大きさに縮めたもの。例: 要素 10 万・深さ 120）が、tree-walk 版でも VM 版と同じ stdout（`done`）・exit 0 で終わる。tree で、深さを固定して要素数を 10 倍にしたときの確保バイト数の増分が、反復元の List を 1 回作る量の定数倍に収まり、深さに比例しない。確かめ方: CLI を subprocess で起動して stdout・exit code を比べるテスト（両 backend）と、`tests/scaling.rs` のカウントアロケータで確保量を測るテスト（両 backend）。
  - **FIS-AT-02（反復の意味は変わらない）:** ループ中に元の binding へ `push`・`pop`・index 代入・再代入をしても反復列が開始時の要素のままであること（List・Dict）、Dict の key のコードポイント順、Str の文字の順、`break`・`continue`・`return`・`try` の中での脱出が、変更前と同じ出力になる。tree-walk 版と VM 版で同じ。既存の golden fixture（`for_iteration_snapshot`・`collection_cow_alias` など）・`tests/differential.rs`・`tests/scaling.rs` が、Dict・Str の `for` の heap・string の累積の期待値（上の「課金の変化」のとおり）を除いて変更なしで通る。確かめ方: 既存のテストと、足りない形を足した golden fixture（両 backend）。期待値を変えたテストは PR 本文に列挙する。
  - **FIS-AT-03（Dict・Str の反復列の課金）:** Dict（長さの違う key を含む）と Str（ASCII と複数 byte の文字を含む）の `for` で、ループの前後の `string_allocations`・`string_bytes` の増分が要素数・byte 長の合計と一致し、tree と VM で完全一致する（`tests/differential.rs` の軸 (A)）。live heap は各 engine の中で比べる。比べる 2 つの script は、どちらも同じ Dict・Str と、同じ要素数の List（整数の要素。反復列を作らず共有される）を同じ順で作り、`for` の対象の識別子だけを同じ長さの別名で変える（Dict・Str を `for` する script と、List を `for` する対照の script。source・AST・bytecode を同じ大きさにし、source の byte 長の差が `peak_heap_bytes` に入らないようにする）。この 2 つの `peak_heap_bytes` の差が、List body（24 + 32 × 要素数）と各要素の String body（24 + byte 長）の合計に等しい（反復ごとの continuation frame・ループ変数のセルの課金は両方の script で同じなので打ち消される）。最後まで回した場合と `break` で抜けた場合のどちらも、ループの後の `live_heap_bytes` は、上の List を `for` する対照の script（同じ値を作り、`for` の対象だけが違うもの）と同じになる（反復列が release される）。tree と VM の間は §5.3 の A-1 どおり、課金の有無と release の寿命の一致にとどめ、byte 単位の一致は求めない。確かめ方: in-process で両 engine を実行して usage（`peak_heap_bytes`・`live_heap_bytes`・string の累積）を読む統合テスト（`tests/scaling.rs` の `peak_heap_after` / `live_heap_after` の形。両 backend）と、`tests/differential.rs` の軸 (A) の exact 突合。
  - **FIS-AT-04（上限で止まる）:** 関数の再帰の各段で同じ Dict（長い key を含む）・同じ Str を `for` する script が、`max_live_heap_bytes` を小さくした実行で `BudgetExceeded(HeapBytes)`（`heap_limit`）の terminal で止まり、`try` / `catch` で捕捉されず、プロセスは abort しない。これは各 engine で確かめ、止まる深さ・行・trace とその時点までの string の累積は engine 間で比べない（live heap は engine 間で byte 単位が一致しないため。§5.3 の A-1、`tests/differential.rs` の軸 (B)）。`max_string_allocations` を小さくした実行での `BudgetExceeded(StringAllocations)` の terminal と、文字数が `max_collection_elements` を超える Str が要素を作る前に今と同じ collection の上限のエラーになること（Dict は script の中で `max_collection_elements` を超えて作れないので、この確かめは Str に限る。Dict の `check_collection` の位置は実装範囲の手順 1 のとおり Str と同じ）は、terminal の `TsumugiError` 全 field と string の累積が tree と VM で完全一致することまで確かめる（軸 (A)）。確かめ方: 上限を設定した in-process の実行で terminal と usage を確かめるテスト（両 backend。HeapBytes は engine ごとの確認、StringAllocations と collection の上限は `tests/differential.rs` の形の exact 突合）。

### fix-join-replace-path-join-limit 実装スライス（`join` / `replace` / `path_join` の結果の長さを確保前に検査）

[issue #399](https://github.com/kamonabe1927/Tsumugi/issues/399) から作ったスライス（不具合の修正、`slices.toml` の `fix-join-replace-path-join-limit`）。issue の仕分けで「受け入れ」とした。[issue #357](https://github.com/kamonabe1927/Tsumugi/issues/357)（`join` / `replace` の abort）の範囲に `path_join` を加え、閉じた設計 PR #386 の `fix-join-replace-string-limit`（main の `slices.toml` には入っていない）を 1 スライスで設計し直したもので、[issue #387](https://github.com/kamonabe1927/Tsumugi/issues/387)（needs-design）を解く。PR #386 の小節・受入基準・判断 Q1=A を引き継ぐ。直し方は §5.2（新 object は allocation の前に reserve）・§5.3（`max_single_string_bytes`）・§7（外部効果・allocation の前の reserve、§7.1 の overflow）と、この設計 PR で §5.3 に書き足した「builtin が新しい String body を作るときは確保の前に長さを確かめる」の項で決まる。逸脱一覧の「callback なし bulk builtin の chunk 境界での slice yield」（fuel-bulk-chunk-yield、yield の話）とは別の不具合として切る。

- **現状の不具合:** `join`（`src/builtin_core.rs` の `builtin_join_budgeted`）は要素ごとに `BudgetLedger::render_display` で描画し、要素 1 つの長さだけを per-item の上限で見る。描画した要素を `Vec<String>` にすべて持ってから `parts.join(sep)` で連結し、全体の長さは見ない。`replace`（同 `builtin_replace`、`builtin_core::dispatch` から呼ばれ budget を持たない）は `s.replace(old, new)` で結果を作り切る。空パターンでは `new` を文字の境界ごと（文字数 + 1 箇所）に挿すので、出力は約「文字数 × `new` の長さ」になる。`path_join`（同 `builtin_path_join`、`dispatch` から呼ばれる）は全引数の型を検査したあと、すべてを `PathBuf::push` で連結してから `Value::str_constant` にする。可変長引数なので、結果は「引数の個数 × 引数の長さ」まで伸びる。3 つとも長さの検査は、結果を返したあとの `track_result` → `charge_string`（tree は `src/builtin.rs` の `apply_pure_core_builtin`、VM は `src/vm.rs` の `exec_builtin`）が初めて行う。そのため上限を大きく超える入力では `BudgetExceeded(SingleStringBytes)` にならず、Rust の allocation 失敗でプロセスが abort する（exit 134）。main 397a23f の debug ビルドを `ulimit -v 4000000` で実行して、issue #357 の再現（4 MiB の文字列 2000 個の `join(xs, "")`、1 MiB の文字列の `replace(s, "", s)`）と issue #399 の再現（4 MiB の文字列 2000 個の `path_join(s, s, …)`、`memory allocation of 4294967296 bytes failed`）が tree-walk 版・VM 版とも abort することを確かめた。引数・要素が少なければ「文字列の長さが上限を超えました」で止まるが（`path_join(s, s, s)` は 12582914 バイト）、それは結果を確保したあとの検査である。
- **実装範囲:**
  - `join`: 要素と区切りを 1 つの出力 buffer へ順に書き出し、要素の描画（`render_display` と同じ描画と fuel 課金）と区切りの書き足しのたびに、buffer の累積 byte 長が `max_single_string_bytes` を超えていないかを確かめる。超えた時点で `ControlStop::BudgetExceeded(SingleStringBytes)`（`used = reserved = 0`、`requested` は超えた時点の累積 byte 長。下の「`join` の超過の `requested`」）を返し、それ以上の要素を描画しない。要素ごとの `Vec<String>` は作らない。これで `join` が確保する文字列は、上限に要素 1 つ（または区切り 1 つ）の長さを足した程度（多くとも上限の約 2 倍）に収まる。buffer へ書き出す関数の形（`BudgetLedger` のメソッドにするか、`render_into` を crate 内に公開するか）は実装に任せる。
  - `replace`: 結果を作る前に、`old` の一致の数 m を数え（`str::replace` と同じ、左から重ならずに取る一致の数。`s.matches(old).count()` と同じ数え方で、`old` が空なら文字数 + 1）、結果の長さ `len(s) + m × (len(new) − len(old))` を checked な演算で求める。長さが `max_single_string_bytes` を超えれば、結果を作らずに `BudgetExceeded(SingleStringBytes)`（`used = reserved = 0`、`requested` = 求めた結果の長さ）を返す。演算が overflow したら §7.1 のとおり `requested = u64::MAX` の同じ超過とする。超えなければ従来どおり `s.replace(old, new)` で作る。
  - `path_join`: 全引数の型の検査（[次期意味論・実装決定](semantic-decisions.md) §9。非 Str があれば結合を始めずに `builtin_type`）は今の順のまま先に行う。そのあと、引数を左から 1 つずつ `PathBuf::push` し、push のたびに path の長さが `max_single_string_bytes` を超えていないかを確かめる。超えた時点で、残りの引数を push せずに止める（後ろの引数が absolute component で path を置き換え、最終結果が上限以内に収まる場合でも止める。`requested` は超えた時点の path の長さ。下の「`path_join` の超過の判定」）。確保する path の長さは上限に引数 1 つと区切り 1 つを足した程度（多くとも上限の約 2 倍）に収まり、超過は `BudgetExceeded(SingleStringBytes)`（`used = reserved = 0`）とし、長さを足し合わせる計算は checked で、overflow したら §7.1 のとおり `requested = u64::MAX` の同じ超過とする。予算内の結果は今と同じ `PathBuf::push` の規則（[言語仕様](language-spec.md) の `path_join` の行：separator・absolute component・prefix の扱いは実行 OS の Rust `PathBuf` と同じ）で作る。
  - `replace` と `path_join` を `join` と同じく budget（または `max_single_string_bytes`）を受け取る版（例: `builtin_replace_budgeted`・`builtin_path_join_budgeted`）にし、tree（`apply_pure_core_builtin`）と VM（`exec_builtin`）の dispatch 境界で `join` と同じ位置で intercept する。両 backend が同じ共有関数を呼ぶ（`builtin_core::dispatch` の `"replace"`・`"path_join"` の腕は消し、`sort` / `join` / `to_str` / `contains` と同じ注記を足す）。tree の式 frame 経路（tree-expr-continuation-e3）も `apply_pure_core_builtin` を通るので同じになる。
  - 超過は `control_stop_to_error(stop, 0, line)` で terminal にし（既存の `join` と同じ）、`try` / `catch` で捕捉されない。
  - done にする実装 PR で、本書の逸脱一覧から `join` / `replace` / `path_join` の行を、[言語仕様](language-spec.md) の逸脱一覧から `path_join` の行を削除する（本書のその下の、その他の builtin と `+` の行は残す）。
- **範囲外:**
  - 予算内の `join` / `replace` / `path_join` の結果・fuel・string の累積会計（`string_allocations` / `string_bytes`）・live heap の課金（変えない。結果は従来どおり `track_result` で課金する）。ただし `path_join` で途中の長さが上限を超え、後ろの absolute component で置き換わる呼び出しは、成功から超過に変わる（下の「`path_join` の超過の判定」）。
  - `replace` の一致を数える走査と書き出しへの fuel 課金、`join` の Str 要素の byte 長への fuel 課金、`path_join` の引数の byte 長への fuel 課金（§4.1 の UTF-8 payload の課金は fuel-bulk-charge、走査の途中の slice yield は fuel-bulk-chunk-yield の範囲）。数える走査は入力の長さに比例し、入力は `max_single_string_bytes` 以下なので有限である。
  - 文字列の連結 `+` と、`builtin_core::dispatch` のその他の新しい String body を作る builtin（`upper` / `lower` / `trim` / `split` / `slice`（Str）/ `reverse`（Str）/ `format_time` / `keys` / `type`）。これらも結果を作ってから `charge_string` で検査するが、結果の長さは引数の数に依らず入力の定数倍で抑えられる（上限ちょうどの入力で多くとも上限の約 6.5 倍、`format_time`）。§5.2 の「allocation 前に reserve」と §5.3 の項との差は、このスライスでは直さず、逸脱一覧に行として載せた。行には、コードで抑えを確かめ、main 397a23f の debug ビルドで上限ちょうどの入力を `ulimit -v 4000000` の下で実行して tree-walk 版・VM 版とも abort しないことを確かめたものだけを挙げた（issue #399 の「裏付けの無い『abort しない』という主張は書かない」）。
    - **決定記録（判断役が選択（PR #386）、Q2=A。issue #399 で `path_join` を外した）:** この設計 PR で §5.3 に足した項（builtin が新しい String body を作るときは確保の前に長さを確かめる）に対し、確保後に検査する他の builtin と `+` を逸脱一覧に 1 行で載せる（fix スライスは足さない）。理由: §5.3 の項は builtin 全般にかかる目標で、§5.2 の「string連結…で新objectを作る場合は…allocation前に全logical bytesをreserveする」とも揃う。[ドキュメント運用ガイド](documentation-guide.md)「逸脱一覧の持ち方」は目標は本文・現状は逸脱一覧だけが正本と定めるので、目標を書き足した PR で既知の逸脱を同じ表に載せるのが正本の持ち方に合い、マニフェストの「現在地の扱い」・原則 9・「再現可能な検証 ＞ 根拠のない安全性の主張」にも沿う。採らなかった案: B（§5.3 の項を入力の定数倍で抑えられない結果を作る builtin に絞る）は目標を弱め、§5.2 とのずれも残る。C（整合点検に送る）は状態の正本が現状と食い違うことを承知でマージすることになる。PR #386 ではこの行に `path_join` を「引数の合計と区切りで抑えられるので abort には至らない」として入れたが、`path_join` は可変長引数で引数の数だけ伸び、審査（manifesto-gate、PR #386 の 8195b19）の再現で abort した。issue #399（kamonabe）に従い、`path_join` はこのスライスの実装範囲に入れ、行には入れない。
  - `max_single_string_bytes` の既定値と、§7.2 の優先順位（`SingleStringBytes` だけを返す。cumulative の `StringAllocations` / `StringBytes` の検査は従来どおり `track_result` の `charge_string` で行う）。
  - `path_join` の型の検査の順・エラー文言（semantic-decisions §9）と、`PathBuf::push` の platform ごとの規則そのもの。
- **課金の変化:** 予算内の呼び出しでは fuel・string の累積会計・heap は変わらない（`join` の要素の描画は今と同じ `render_display` の規則で、複合ノードごとに fuel 1）。上限を超える呼び出しは、確保の前に同じ resource（`SingleStringBytes`）の terminal で止まる。`join` で超える場合は、超えた時点より後の要素を描画しないので、変更前より fuel が少なくなることがある。§4.1 に charge point を足す変更ではないので `fuel_accounting_revision`（§3.1）は変えない。`path_join` の途中の長さで止める変更は、今も上限を超える body を一度確保している縁の呼び出しだけを超過にするもので、§5.3 の目標と[言語仕様](language-spec.md)の `path_join` の行に合わせる不具合の修正として扱い、`language_revision` は変えない。
- **`join` の超過の `requested`:** 上限を超えたことが分かった時点の buffer の累積 byte 長とする（結果全体の長さではなく、その下限）。§3 の「`requested = candidate size/cardinality`」（per-item 超過の規則）は結果の大きさを先に求められる経路の読み方で、逐次に書き出して打ち切る経路では、打ち切った時点の長さを candidate size として扱う（`render_display` が per-item の上限で打ち切るときの `requested`、`src/budget.rs` の `render_into` の `out.len()` と同じ規則）。`replace` は結果の長さを先に求めるので、結果全体の長さを報告する。
  - **決定記録（判断役が選択（PR #386）、Q1=A）:** 理由: A・B ともマニフェストに反しうる点は無い（どちらも確保前に `SingleStringBytes` の terminal で止まり、fuel で有限）。優先順位の表の「制御可能な最悪時負荷」で A が上回る。A は要素を 1 回だけ描画して超えた時点で止まり、確保は上限の約 2 倍に収まり、予算内の fuel が変わらない（原則 1・3・6、§5.2・§5.3・§7）。値は両 backend が同じ共有関数を通るので決定的で（原則 5）、`render_display` の打ち切りと同じ規則になり `print` / `to_str` と一貫する。範囲外（予算内の fuel・会計と `fuel_accounting_revision` を変えない）とも矛盾せず変更が小さく（原則 8）、受入基準で両 backend の値を確かめられる（原則 9）。採らなかった案: B（書き出す前に長さを数える pass を通し、結果全体の長さを報告する）は、Str 以外の要素を 2 回辿るので予算内の呼び出しでも複合ノードの fuel が 2 倍になり、不具合の修正で fuel の観測値を変える（原則 5・8）。
- **`path_join` の超過の判定:** 引数を左から 1 つずつ push し、push のたびに長さを見て、上限を超えた時点で止める。途中の長さが上限を超えたあと、後ろの引数が `PathBuf::push` の規則で path 全体を置き換えて最終結果が上限以内に収まる呼び出し（例: 8 MiB の `s` で `path_join(s, s, "/x")`。変更前は途中で 16 MiB を確保して `"/x"` で成功する）も超過にする。`requested` は超えた時点の path の長さ（結果の長さではない）とし、上の「`join` の超過の `requested`」と同じ読み方をする。途中で一度も上限を超えない呼び出し（例: `path_join(s, "/x", "y")`）は変わらない。[言語仕様](language-spec.md) の `path_join` の行と §5.3 に同じ規則を書き足した。
  - **決定記録（判断役が選択（PR #403）、Q1=A）:** 理由: 3 案ともマニフェストに反しうる点は無い（どれも確保が上限の定数倍に収まり、超過は捕捉できない `BudgetExceeded(SingleStringBytes)` の terminal で止まる。原則 1・3・6）。優先順位の表で A が上回る。「制御可能な最悪時負荷」「構造化された停止・失敗 ＞ best-effort での実行継続」: A は途中の長さが上限を超えた時点で止まり、後ろの引数で置き換わることを期待して先へ進まない。§5.3 の「上限付きで逐次に書き出して超えた時点で止める」の形そのものである。「一貫した意味論」: 判定は std の `PathBuf::push` の結果の長さを見るだけで置き換えの規則を実装で再現しないので、言語仕様の `path_join` の行（扱いは実行 OS の `PathBuf` と同じ）と食い違う余地が無く、両 backend が同じ共有関数を通るので値は決定的（原則 5）。`requested` の読み方は `join` の決定記録（PR #386 Q1=A）と揃う。「小さく監査可能な中核」: 変更が小さく（原則 8）、JRP-AT-03 で途中超過の例と `requested` を両 backend で確かめられる（原則 9）。予算内の結果が変わるのは途中の長さが上限を超えて後ろの引数で置き換わる縁の呼び出しだけで、その呼び出しは今も上限を超える body を一度確保しているので、§5.2・§5.3 の目標から見れば超過として止めるのが筋である（alpha 段階の完全な後方互換は「目指さないもの」）。採らなかった案: B（最終結果の長さで判定し、後ろに置き換える引数があればやり直す）は `PathBuf::push` の OS ごとの置き換え規則（Windows の root だけ・prefix だけの引数など）を実装で再現する必要があり、std と食い違えば OS ごとに結果・超過がずれる（原則 5・8）。C（引数の長さの和を上界として先に判定する）は途中で一度も上限を超えない予算内の呼び出し（例: `path_join(s, "/x", "y")`）まで超過にして A より広く言語挙動を変え、`requested` も実際の長さより大きくなる（原則 5・7）。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜40 は done を含めてすべて使われていた。先例（PR #368・PR #381・PR #388 の Q1=A。いずれも判断役が選択）と同じく、設計 PR では仮にすべてのスライスより後（98）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR を遅らせずに出して、今ある `ready` より前へ移す。issue #400 から作るスライスはこのスライスより後に置く（issue #400 の仕分け）。§8 は付け直しの PR を「先に出す」と定めており、この扱いは順序が逆（設計 PR のマージ後）になる。この外れをここに記録として残す。
- **想定の変更規模:** `src/` は 80〜200 行程度（`join` の 1 buffer への書き出しと累積長の検査 25〜50 行、`replace` の長さの事前計算と budget 版 25〜50 行、`path_join` の長さを見ながらの組み立て 20〜50 行、`src/builtin.rs` と `src/vm.rs` の intercept 各 5〜20 行、`builtin_core::dispatch` の腕と注記の整理 数行）。テストは unit と `tests/` で 200〜320 行。触る仕組みは「文字列 builtin（`join` / `replace` / `path_join`）の結果の長さの確保前の検査」1 つ。受入基準は 5 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **JRP-AT-01（issue の再現が abort しない）:** issue #357 の 2 つの再現（4 MiB の文字列 2000 個の `join(xs, "")`、1 MiB の文字列の `replace(s, "", s)`）と issue #399 の再現（4 MiB の文字列 2000 個を引数に並べた `path_join(s, s, …)`）を既定の budget の CLI で実行すると、プロセスが abort せず（exit 134 でなく）、単一文字列上限の超過の診断（`文字列の長さが上限を超えました`）で exit 1 になる。tree-walk 版と VM 版で同じ。確かめ方: CLI を subprocess で起動して exit code と stderr を確かめるテスト（両 backend）。Linux では、確保の前に止まることを確かめるため、address space を制限した子プロセス（`sh -c 'ulimit -v 4000000; …'` など、issue の再現と同じ条件）から起動する。
  - **JRP-AT-02（`join` / `replace` の上限ちょうどは成功し、+1 で確保前に止まる）:** 小さい `max_single_string_bytes`（例: 64）を設定した実行で、結果がちょうど上限の長さになる `join` / `replace` は成功し、1 byte 長くなる入力は `BudgetExceeded(SingleStringBytes)` の terminal になり、`try` / `catch` で捕捉されない（§15.1）。`replace` は空パターン・`new` が `old` より短い（結果が縮む）場合・一致なし・重なりうるパターン（例: `"aaa"` に `"aa"` は 1 箇所として数える）を含め、超過の `requested` が結果の長さと一致する。`join` は List / Dict を含む要素と区切りの書き足しで超える場合を含め、`requested` が超えた時点の累積 byte 長（結果全体の長さ以下）になる。tree-walk 版と VM 版で同じ。確かめ方: `BudgetConfig` を設定した embedding API の統合テスト（両 backend）。
  - **JRP-AT-03（`path_join` の上限）:** 小さい `max_single_string_bytes`（例: 64）を設定した実行で、結果がちょうど上限の長さになる `path_join`（引数 2 個と多数の両方）は成功し、1 byte 長くなる入力は `BudgetExceeded(SingleStringBytes)` の terminal になり、`try` / `catch` で捕捉されない。途中の長さが上限を超え、後ろの absolute component で置き換わる呼び出し（例: 上限 64 で 40 byte の `s` の `path_join(s, s, "/x")`）も超過になり、`requested` は超えた時点の path の長さ（例では 81）になる。途中で一度も上限を超えない呼び出し（例: `path_join(s, "/x", "y")`）は従来どおり成功する。非 Str の引数を含む呼び出しは、長さに関わらず今と同じ `builtin_type` になる。tree-walk 版と VM 版で同じ。確かめ方: `BudgetConfig` を設定した embedding API の統合テスト（両 backend。absolute component の例は Unix と Windows で分ける）。
  - **JRP-AT-04（予算内の結果と課金は変わらない）:** 予算内の `join`（空 List・空の区切り・Str 以外の要素・入れ子の List / Dict・多バイト文字）、`replace`（空パターン・一致なし・多バイト文字・結果が縮む場合）、`path_join`（引数 0 個・1 個・多数・空文字列の引数・absolute component を含む場合）の結果の文字列と、usage の fuel・`string_allocations`・`string_bytes` が変更前と同じである。tree-walk 版と VM 版で同じ。確かめ方: 結果と usage を比べる統合テスト（両 backend。既存の `join` / `replace` / `path_join` のテストもそのまま通る）。
  - **JRP-AT-05（長さの計算の overflow）:** `replace` の結果の長さを求める計算（と、実装が `path_join` の長さを足し合わせて求める場合はその計算）が u64（または usize）で overflow する入力の組み合わせでは、wrap せず `requested = u64::MAX` の `SingleStringBytes` 超過になる（§7.1）。確かめ方: 長さを求める関数に大きな長さと一致の数・引数の長さを直接渡す unit test（実際の巨大な文字列は作らない）。

### fix-host-call-resume-requeue 実装スライス（host call の完了を受け取った handle を Ready にして run-turn へ戻す）

[issue #359](https://github.com/kamonabe1927/Tsumugi/issues/359) から作ったスライス（不具合の修正、`slices.toml` の `fix-host-call-resume-requeue`）。issue の仕分けで「受け入れ」とした。直し方は §9.2 の状態遷移（`Running --slice/host/audit待ち--> Yielded --再queue--> Ready`、`Ready` は「link済みでrun-turn queueにいる」、`Yielded` は「理由の解消後にrun-turn queue末尾へ戻る」、`poll` は「run-turn queue先頭のReadyまたは再開可能なYieldedだけをRunningへ遷移させる」）と §12.1 の規則 2・4・5 で決まり、目標の章は変えない。

- **現状の不具合:** 冒頭の逸脱一覧の `ExecutionHandle::poll` の host call 完了の行のとおり。`poll`（`src/engine.rs`）の step 4 は `Yielded(HostCallPending { call_id })` の handle で ticket の完了を受け取ると、`resume_host_call` で結果を注入して `pending_ticket = None` にし、step 5 へ落ちる。step 5 は run-turn queue の末尾へ登録し、先頭でなければ「state は遷移させない」で `SchedulerPreempted` を返す。state は `Yielded(HostCallPending)` のまま残り、次の poll は再び step 4 に入って ticket が無いので `HostCallPending` を返す。step 5・6 へ二度と届かないまま run-turn queue に残るので、先頭に来ると他の handle も `SchedulerPreempted` を返し続ける。完了を受け取った poll でたまたま先頭だった場合（execution 1 本の `tests/scheduler_api.rs` の `host_pending_waker_roundtrip` など）は step 6 で `Running` になるので、この経路を通らない。
- **実装範囲:**
  - step 4 で ticket の完了を受け取り `resume_host_call` が成功したら、`pending_ticket = None` に加えて state を `ExecutionState::Ready` へ遷移させてから step 5 へ落とす（§9.2 の「Yielded --再queue--> Ready」。step 5 が同じ poll で run-turn queue の末尾へ登録するので、`Ready` の「run-turn queueにいる」と一致する）。先頭でなければ step 5 は今のとおり state を変えずに `SchedulerPreempted` を返し、state は `Ready` のまま残る。以後の poll は step 3・4 に入らず step 5 の先頭判定へ進み、先頭になった poll で step 6 が `Running` にして、注入済みの結果から続きを 1 slice 進める。
  - 完了を受け取った poll で先頭だった場合は、`Ready` を経て同じ poll の step 6 で `Running` になる。観測できる挙動（戻り値・slice の進み方）は今と同じである。
  - `resume_host_call` が失敗した場合（call_id の不整合など）と、step 4 の先頭の cancel/deadline の checkpoint の扱いは変えない。
  - step 5 の `SchedulerPreempted` の doc（「self.state は遷移させない」）と、step 4 の完了の分岐のコメントを、完了を受け取った handle が `Ready` で残ることが分かる記述に直す。
  - done にする実装 PR で、本書の逸脱一覧の該当行を削除する。
- **範囲外:**
  - run-turn queue へ戻す時点。今は ticket の完了を受け取った poll（step 4 → 5）で末尾へ登録し、完了の wake の時点では登録しない。§12.1 規則 4 の「host call/audit完了でwakeしたexecutionも末尾へ置く」の位置づけ（wake の時点か、完了を観測した poll の時点か）はこのスライスで変えない。
  - slice fuel で yield した handle（`Yielded(SliceFuelExhausted)` のまま run-turn queue にいる）など、他の理由の `Yielded` の公開 state。止まる不具合ではなく、このスライスでは触らない。
  - run-turn queue の先頭の handle を pause したとき（[issue #361](https://github.com/kamonabe1927/Tsumugi/issues/361)）・terminal の handle の active slot の解放（[issue #362](https://github.com/kamonabe1927/Tsumugi/issues/362)）・非 terminal の handle の drop と cancel token（[issue #363](https://github.com/kamonabe1927/Tsumugi/issues/363)）。どれも同じ scheduler/handle に触れるが、それぞれ pause・admission slot・drop の別の仕組みで、[開発ループ手順書](loop-runbook.md) §14.1 の「触る仕組みは 1 つまで」に合わせて別のスライスにする（このスライスは host call の完了を受け取ったあとの state 遷移 1 つ）。`Ready` にした handle を pause したときに run-turn queue に残る扱いは #361 の範囲である。
  - VM。scheduler の handle 経路は tree evaluator（`ExecutionContext` の評価器）だけで実行し、VM はこの経路に無い（逸脱一覧・未実装範囲の「VM の式レベル continuation」）。言語の挙動は変わらない。
- **課金の変化:** なし。注入する結果・fuel・heap・I/O の課金は今のままで、state の遷移だけを直す。`fuel_accounting_revision`・`heap_accounting_revision`・`language_revision` は変えない。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜34 は done を含めてすべて使われていた。`tests/slices_contract.rs` が `order` の重複を弾くので、先例（PR #368・#381・#388 の Q1=A、PR #403）と同じく、設計 PR では仮にすべてのスライスより後（99）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR で 35 より前へ移す。99 は仮の値である。
- **想定の変更規模:** `src/` は 5〜20 行程度（`src/engine.rs` の `poll` の step 4 の完了の分岐で state を `Ready` にする数行と、step 4・5 のコメントと doc の修正）。テストは `tests/scheduler_api.rs` に 120〜250 行（issue #359 の再現を含む 2 handle の往復）。触る仕組みは「host call の完了を受け取った handle の state 遷移」1 つ。受入基準は 4 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **HRQ-AT-01（issue #359 の再現が完走する）:** `limits(2, 0)` の `Engine` で、常に Pending を返す cooperative 関数 `fetch`（`tests/scheduler_api.rs` の `FakeCooperativeAdapter` の形）を使う h1（`let x = fetch()` のあとに出力か context の値で完了が分かる文）と、`while` で長く回る h2 を作る。h1 を `HostCallPending`、h2 を小さい `PollSlice`（例: `max_fuel: 64`）で `SliceFuelExhausted` にしてから complete し、h1・h2 を交互に小さい slice で poll すると、上限の往復回数（例: 2000）の中で h1 が `Completed` の terminal に達し、h2 も terminal（`Completed`）に達する。h1 の完了後の文が実行され、注入した値が `x` に入っていることを確かめる。確かめ方: `tests/scheduler_api.rs` の統合テスト。
  - **HRQ-AT-02（完了を受け取った非先頭の handle は Ready で残り、先頭で続きを進める）:** HRQ-AT-01 の形で、complete のあと h1 を run-turn queue の先頭でない時点で poll すると `Yielded(SchedulerPreempted)` を返し、`h1.state()` が `ExecutionState::Ready` になる。そのあと h1 が先頭になった poll は `HostCallPending` を返さず、注入済みの結果から続きを実行する（adapter の `start` は 1 回だけ呼ばれ、結果は 1 回だけ注入される）。確かめ方: 同じ統合テストで `state()` と adapter の呼び出し回数を確かめる。
  - **HRQ-AT-03（末尾へ戻り、round-robin で進む）:** complete のあと、h1 は完了を受け取った poll の時点で run-turn queue の末尾に置かれ、その時点で queue にいる h2 より前へ挿入されない（§12.1 規則 3・4）。以後 h1・h2 は先頭の handle の 1 回の poll で 1 slice ずつ交互に進み、どちらかだけが連続して semantic work をすることはない（規則 2・5。他方が terminal に達したあとを除く）。確かめ方: 同じ統合テストで、各 poll の戻り値（`SliceFuelExhausted` / `SchedulerPreempted` / terminal）の並びと `usage()` の fuel の増え方を確かめる。
  - **HRQ-AT-04（ほかの host call の経路は変わらない）:** 完了を受け取った poll で先頭だった場合（execution 1 本）はその poll で続きを進め、complete 前の cancel・deadline は今と同じ terminal（`Cancelled` / `DeadlineExceeded`、ticket の cancel と rollback）になる。`tests/scheduler_api.rs` の既存のテスト（`host_pending_waker_roundtrip`・`lost_wake_avoided_both_interleavings`・`host_waiting_consumes_active_slot`・`cancel_works_while_host_waiting_and_foreign_clock_rejected` ほか）が変更なしで通る。確かめ方: 既存のテストと `cargo test`。期待値を変えたテストがあれば PR 本文に列挙する。

### fix-sync-run-host-call-resume 実装スライス（同期 `Engine::run` が cooperative host call の ticket を待って結果を注入する）

[issue #360](https://github.com/kamonabe1927/Tsumugi/issues/360) から作ったスライス（不具合の修正、`slices.toml` の `fix-sync-run-host-call-resume`）。issue の仕分けで「受け入れ」とした。直し方は §9.2 の「同期`Engine::run`は…同じhandleをterminalまでpoll」する規則（host call 待ちの `Yielded(HostCallPending)` は ticket の完了で解消し、同じ continuation から続ける。新しい外部呼び出しを始めない）と、§6.2 の「作成thread上の次回`poll`が`try_take`する」・§8 の「adapter待機…でも確認」で決まり、目標の章は変えない。仕分けのとおり、同期 run の cooperative の Pending をエラーにする読み方は §9.2 と合わないため採らない。待ち方（下の実装範囲の 3 つめ）は判断役が選択した（PR #410、Q1=A）。

- **現状の不具合:** 冒頭の逸脱一覧の埋め込みの同期 `Engine::run` の行のとおり。`run_transactional` と `run_audited_inner` の poll ステップ loop は `SliceOutcome::YieldedHostCall { .. }` を `YieldedSliceFuel` などと同じ `continue` で扱う。評価器の `pending_host_call` には ticket と `call_id` が残ったまま、`resumed_host_call` は `None` なので、次の `run_slice` で同じ文の呼び出しがもう一度 `start()` を呼び、`pending_host_call` を新しい ticket で上書きする。scheduler の handle 経路（`src/engine.rs` の `ExecutionHandle::poll`）は step 6 で `take_pending_ticket` し、step 4 で cancel→deadline の checkpoint と `try_take_value` → `resume_host_call` をしているので、この不具合は無い。
- **実装範囲:**
  - `run_transactional` と `run_audited_inner` の両方で、`SliceOutcome::YieldedHostCall { call_id }` を他の yield と分け、`context.evaluator.take_pending_ticket()` で ticket を引き取ってから、完了を待つ。待ちの手順は 2 つの driver で共通の関数にまとめる。
  - 待ちの各ステップは、先に `checkpoint_cancel` → `checkpoint_deadline` を評価し（§4.6 の linearization。scheduler の poll の step 4 と同じ順）、そのあと `try_take_value` する。完了していれば `resume_host_call(call_id, result)` で結果を注入し、次の `run_slice` へ戻る（評価器は注入済みの結果を式の結果へ写し、`start()` を呼ばない）。
  - 完了を待つ間は、ticket と request の `CancellationToken` に呼び出し thread を unpark する `ExecutionWaker` を登録し、完了していなければ短い上限（実装の定数。例: 1〜10 ms）つきの `park_timeout` で待ってから checkpoint に戻る。上限は deadline を request の clock で確かめ直すための間隔で、公開 API・言語挙動には出ない。
    - **決定記録（判断役が選択（PR #410）、Q1=A）:** 理由: CPU を使い尽くすまで走り続けず最悪時の負荷を制御でき（マニフェスト原則 1）、deadline を request の clock で上限の間隔ごとに確かめて実行時間を host が制御し（原則 3）、cancel・deadline を構造化された terminal にする（原則 6）。§8（token は host call 待ちを wake する）、§9.1 の waker 契約（「busy-poll は不要である」）、§6.2（ticket に `ExecutionWaker` を登録し作成 thread が `try_take`）、§9.2（同期 run は同じ handle を terminal まで poll）と揃う。待つのは同期 run の呼び出し thread だけで、上限つきの park と必須の deadline で有界なので、§6.2 の「時間上限が証明できない待ち」には当たらない。言語の結果・副作用の順序・fuel は変わらない（原則 5）。追加は park 用の `Wake` 実装 1 つで小さく（原則 8）、SHR-AT-03・05 で cancel の即時性と確認の回数を再現可能に確かめられる（原則 9）。採らなかった案: B（間隔なしの繰り返し。監査の backpressure の待ちと同じ形）は、complete されない ticket で deadline（既定 30 秒）まで 1 コアを使い続けて原則 1 に反しうるうえ、§9.1 の「busy-poll は不要」とも合わない（determinism-and-audit §10.2 は backpressure 中の deadline・cancel の監視を定めるだけで、間隔なしの待ちを定めておらず、先例としての根拠は実装にとどまる）。C（`yield_now` を挟む繰り返し）は、他に runnable な thread が無ければ B と同じく CPU を使い続け、原則 1 に反しうる。
  - cancel・deadline で待ちを終えるときは、ticket を `cancel()` して遅着の結果を破棄し、`abort_session` で language-state を rollback してから、今の mid-run の cancel・deadline と同じ terminal（`ExecutionOutcome::Cancelled` / `DeadlineExceeded`）を返す。`resume_host_call` が失敗した場合（call_id の不整合）は、scheduler の poll と同じく catch 不能の内部エラーの terminal にする。
  - 監査付き driver では、待ちに入る前に今のとおり `flush_events` で slice 境界の event（`Yielded(HostCallPending)` まで）を deliver し、注入後の `run_slice` が積む `Resumed` を次の flush で deliver する。待ちの間に event の順序を変えない。
  - `run_transactional` の loop のコメント（「全 yield 変種を continue で回し切る」）と、監査付き driver の該当分岐のコメントを、host call の yield だけは ticket を待つことが分かる記述に直す。
  - done にする実装 PR で、本書の逸脱一覧の該当行を削除する。
- **範囲外:**
  - 関数本体・callback 本体の中での cooperative host-call の Pending yield（未実装範囲の行のまま。関数本体の中の Pending は今のとおり `may_yield = false` の規約違反の `InternalControl`）。このスライスはトップレベルの文/反復境界の yield だけを扱う。
  - 待ちを cancel・deadline で終えたときの、監査上の host call を `Cancelled` / `Detached` の `HostCallFinished` で閉じること（[決定性・実行時監査仕様](determinism-and-audit.md) の pending host call の close）。scheduler の handle の cancel・drop の経路にも配線が無く、別の仕組み（監査の close）なのでこのスライスでは触らない。
  - scheduler の同期入口 `Engine::execute`・`execute_repl_submission`（`src/engine.rs`）の `drive_to_outcome`。handle の poll が ticket を扱うので `start()` の重複は起きない。`HostCallPending` の `Yielded` を `continue` で poll し続ける待ち方と、他の handle との関係（fix-sync-run-concurrent-handle）はこのスライスで変えない。
  - cooperative host function が返す `HostCallTicket<Value>` の completer を別 thread から complete できるか（`Value` は `!Send`）。今の型のまま、同じ thread（adapter・clock・waker の callback など）か ticket を作った側が complete した結果を待つ。complete されない ticket は deadline か cancel で終わる。
  - VM。cooperative host call は VM では Ready にフォールバックする（§14 の Slice 5）。言語の挙動（結果の値・副作用の順序）は tree-walk 版と VM 版で変わらない。
- **課金の変化:** 不具合の経路では `start()` の重複のぶんだけ fuel・host call の課金が積み上がっていたが、直したあとは host call 1 回ぶんだけになる。`start()` が完了済みの ticket を返す場合（`Pending` で返すので、今は完了を見ずに yield して不具合の経路に入る）も、直したあとは host call 1 回ぶんだけになる。`Ready` を返す host call の結果と課金は変わらない。待ちの間は semantic work をせず、fuel を使わない。`fuel_accounting_revision`・`heap_accounting_revision`・`language_revision` は変えない。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜34 に空きが無い。`tests/slices_contract.rs` が `order` の重複を弾くので、先例（PR #368・#381・#388 の Q1=A、PR #403・#408）と同じく、設計 PR では仮にすべてのスライスより後（100）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR で 35 より前へ移す。100 は仮の値である。
- **想定の変更規模:** `src/` は 60〜150 行程度（`src/embedding.rs` の 2 つの driver の分岐と、共通の待ちの関数、park 用の `Wake` 実装、コメントの修正）。テストは新しい統合テスト（例: `tests/sync_run_host_call.rs`）に 200〜350 行。触る仕組みは「埋め込みの同期 driver の host call 待ち」1 つ。受入基準は 5 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **SHR-AT-01（issue #360 の再現で `start()` が 1 回）:** `start()` のたびにカウンタを増やし、complete しない ticket で `Pending` を返す cooperative 関数 `fetch`（arity 0）を `EngineBuilder::host_functions` で登録し、grant 付きで `let r = fetch()` を run する。deadline は進む test clock（`tests/audit_backpressure.rs` の `AutoAdvanceClock` の形）か短い deadline にする。outcome は `DeadlineExceeded` で、`StepLimit` にならず、`start()` は 1 回だけ呼ばれ、ticket は cancel されている（completer の `is_cancelled()` が `true`）。監査 sink なしの経路と監査付きの経路（`audit_slice_fuel` を設定したものを含む）の両方。確かめ方: 統合テスト。
  - **SHR-AT-02（完了した結果が注入され、続きを実行する）:** 同じ形で、ticket を complete する（`start()` が complete 済みの ticket を返す場合と、待ちの途中で同じ thread の callback（test clock の `now()` など）が complete する場合）と、outcome は `Completed` で、`r` に complete した値が入り、`fetch()` のあとの文が 1 回だけ実行され、`start()` は 1 回だけ呼ばれる。`AdapterError` で complete した場合は今の host error の扱いと同じ結果になる。監査付きの経路では host call の event が 1 組だけで、`Yielded(HostCallPending)` と `Resumed` が 1 組ずつ順に並ぶ。確かめ方: 統合テスト（両方の経路）。
  - **SHR-AT-03（待ちの間の cancel）:** 待ちの間に別の thread が request の `CancellationToken` を cancel すると、deadline（例: 30 秒）を待たずに `Cancelled` で終わり、ticket は cancel され、language-state は rollback され（実行前の context の値が残る）、`start()` は 1 回だけ呼ばれる。確かめ方: 統合テスト（cancel から戻るまでの時間が deadline より十分短いことを確かめる）。
  - **SHR-AT-04（待ちの間は fuel を使わず、`Ready` の経路は変わらない）:** SHR-AT-02 の complete までの待ちの前後で `usage` の fuel が増えない（待ちの間に semantic work をしない）。`Ready` を返す cooperative 関数と同期 host function の run は、結果・usage・監査の event 列が変更前と同じである。確かめ方: 統合テストと、既存の `tests/engine_api.rs`・`tests/audit_hostcall.rs`・`tests/audit_yield_resume.rs`・`tests/audit_backpressure.rs` が変更なしで通ること。期待値を変えたテストがあれば PR 本文に列挙する。
  - **SHR-AT-05（待ちは上限つきの park で、間隔なしに繰り返さない）:** 完了していない ticket を待つ間、checkpoint と `try_take_value` を間隔なしに繰り返さず、park の上限の間隔ごとか wake のたびにだけ確かめる（Q1=A、判断役が選択（PR #410））。確かめ方: 実装のレビューと、SHR-AT-03 の形（時刻の進まない clock と遠い deadline、別の thread が一定時間（例: 200 ms）のあとに cancel）で、待ちの間の clock の `now()` の呼び出し回数が、待った時間を park の上限の間隔で割った回数の定数倍（wake のぶんを足す）を超えないことを確かめる統合テスト。

### fix-for-iter-charge-before-copy 実装スライス（`for` の反復列の要素を課金してから複製する）

[loop:drift #395](https://github.com/kamonabe1927/Tsumugi/issues/395) の D-98 から作ったスライス（不具合の修正、`slices.toml` の `fix-for-iter-charge-before-copy`）。直し方は §5.2（新 object は allocation 前に全 logical bytes を reserve する。`for` の反復列の項の「各要素の String body（§5.1）を allocation 前に課金し」）・§5.3 と、fix-for-iter-snapshot-charge 小節の実装範囲の手順 2（「§5.3 の累積を課金してから文字列を複製し」）で決まり、目標の章は変えない。

- **決定記録（判断役が選択（issue #395）、D-98 の Q1=A）:** 実装を §5.2 に合わせる fix スライスを足す（key・文字を借用で受け取り、課金のあとで複製する）。理由: §5.2 は新 object を「allocation前に全logical bytesをreserveする」と定め、`for` の反復列の項も「各要素のString bodyをallocation前に課金」とする。fix-for-iter-snapshot-charge 小節の手順 2 も「1 要素を複製する前に累積の課金を済ませる」と書いており、今の実装は目標の文とも小節の手順とも合っていない。[マニフェスト](manifesto.md)原則 3（すべての実行に予算）と優先順位の表「制御可能な最悪時負荷」「再現可能な検証」に照らし、検査を確保の前に置く形を `for` だけ例外にする理由が無い。直しは借用で受け取って課金のあとで複製するだけで小さく（原則 8）、tree・VM 共通の 1 関数なので backend 間の差も生じない（原則 5）。採らなかった案: B（§5.2 に「未課金の確保は高々 1 要素ぶんまで許す」と書き、実装は変えない）は、マニフェストには反しない（未課金の確保は単一文字列の上限で抑えられる）が、目標の文を実装に合わせて緩め、§5.2 の一般則に `for` だけの例外を作る。fix-for-iter-snapshot-charge 小節の「高々 1 要素ぶん」は「複製の前に課金する」前提で書かれた文で、B の根拠にはならない。
- **現状の不具合:** 冒頭の逸脱一覧の `for` の反復列の要素の複製の行のとおり。`Value::new_iter_str_list`（`src/value.rs`）は `parts: impl Iterator<Item = String>` を受け取り、呼び出し側（tree の `materialize_for_items`（`src/eval.rs`）、VM の `ToIterList`（`src/vm.rs`））は `map.keys().cloned()`・`s.chars().map(|c| c.to_string())` を渡す。反復子が次の要素を返す時点で key・文字はすでに複製されており、そのあとで `charge_string`（§5.3 の累積と `max_single_string_bytes`）、`Value::new_str` → `Tracked::new` の String body の課金（§5.1）が走る。課金で止まる要素も、止まる前に 1 つ複製される。複製の大きさは課金済みの Dict の key と同じか Str の 1 文字なので、上限を回避する経路にはならない（D-98 の点検で確かめた）。
- **実装範囲:**
  - `new_iter_str_list` が要素を借用（例: `impl Iterator<Item = &str>`。形は実装に任せる）で受け取り、要素ごとに次の順で処理する。(1) `charge_string`（§5.3 の累積。`max_single_string_bytes` の検査を含む）、(2) String body（24 + byte 長、§5.1）の live heap への課金、(3) (1)・(2) が成功したあとで文字列を複製して tracked な `Str` にする。(2) と (3) の順にするため、課金してから値を包む `Tracked` の作り方（例: 値を作る closure を受け取る版）を足してよい。課金の順序と量（累積 → String body → 最後に List body）は今と同じにし、上限を超えたときの resource・`used`・`requested` と、それまでの usage は変えない。
  - tree の `materialize_for_items` と VM の `ToIterList` の呼び出しを、借用の要素を渡す形に直す（Dict は `map.keys()` の `&str`、Str は各文字の `&str`。例: `char_indices` で切り出す）。両 backend が同じ関数を通ることは変えない。
  - `new_iter_str_list` の doc の手順の記述を、課金のあとで複製することが分かる記述に直す。
  - done にする実装 PR で、本書の逸脱一覧の該当行（要素の複製の行）を削除する。
  - **決定記録（判断役が選択（PR #414）、Q1=A）:** FCB-AT-03 の `SingleStringBytes` は、tree では embedding の `ExecutionContext` を 2 回の実行で使い回す統合テスト（1 回目は既定の budget で長い key の Dict を作り、2 回目は `max_single_string_bytes` を key より小さくして `for` する）で Dict の `for` の経路から確かめ、VM は context を 2 回の実行で引き継ぐ経路が無い（CLI・REPL は 1 つの budget を共有し、key を作る時点で同じ上限に掛かる）ので、tree・VM 共通の `new_iter_str_list` を直接呼ぶテストで代える。理由: spec の確かめ方に最も近い形で、確かめられる backend では確かめる（[マニフェスト](manifesto.md)原則 5・9）。足すのはテスト 1 本で、課金・上限・terminal の挙動は変えない（原則 8）。採らなかった案: B（テストは関数の直接呼び出しのままにし、理由だけを直す）は、Dict の `for` の経路（`materialize_for_items` → `new_iter_str_list`）で上限に掛かる場合を、確かめられる backend があるのに確かめないまま残す。
- **範囲外:**
  - List body の backing（`Vec::with_capacity(要素数)`）を List body の課金より前に確保している点。逸脱一覧の別の行のとおりで、このスライスでは扱わず、逸脱一覧の行を残す。
    - **決定記録（判断役が選択（PR #412）、Q1=A）:** 理由: 先に確保する量は、要素数の検査（`check_collection_elements`）のあとの `Vec::with_capacity(要素数)` で、要素数は課金済みの反復元の Dict の key 数・Str の文字数なので、`max_collection_elements` と反復元の大きさの両方で抑えられる。作っている間は script が走らず、途中で上限を超えれば drop されるので、上限を回避して実行を続ける経路ではない（[マニフェスト](manifesto.md)原則 3・6）。§5.2 の `for` の項は課金の有無・量・順序を tree と VM で揃えるとし、fix-for-iter-snapshot-charge 小節の手順 3 は「最後に List body」を builtin の `track_result` と同じ順と定めている。範囲外にすれば、この既存の決定と上限での terminal の resource・`used`・`requested` を変えずに済む（原則 5）。また、このスライスの「課金の変化: なし」と、既存のテストを変えずに通して確かめる受入基準の小ささを保てる（原則 8・9）。ずれは逸脱一覧の行として残り、§5.2 の目標の文は緩めないので、後から別のスライスで直す道を閉じない（原則 9）。残る未課金の確保は 32 byte × 要素数（既定の上限で約 32 MB）の一時的なもので、`max_live_heap_bytes` を小さくした実行では論理上限より大きくなりうる。これは許容の決定ではなく、このスライスで扱わないという範囲の決定である。採らなかった案: B（このスライスで List body を要素より先に課金する）は §5.2 の文にそのまま合うが、課金の順序と上限での terminal の値が変わる観測できる挙動の変更で、builtin の `track_result` の順と `for` だけがずれ、FIS-AT-04 の期待値の見直しも要る（原則 5・8・9）。直すなら builtin 側と揃えて順序を決める別のスライスがふさわしい。C（課金しない事前検査を足す）は、検査と課金が 2 箇所に分かれ、事前検査で止まるときの `used`・`requested` に spec の定めが無く、§7 の「1 個の atomic reservation」とも形が違うので、新しい決定が要る（原則 8）。
  - 反復列を遅延で作る形、`for` の fuel、`map` / `filter` / `each` の課金（fix-for-iter-snapshot-charge 小節の範囲外のまま）。
  - builtin が `track_result` で結果を課金する経路（`Value::new_str` に複製済みの `String` を渡す一般の形）。逸脱一覧の `builtin_core::dispatch` の行の範囲である。
- **課金の変化:** なし。fuel・heap・peak heap・string の累積とその順序は変わらず、上限で止まる位置と terminal の値も変わらない。物理的な確保だけが課金のあとへ移る。`heap_accounting_revision`・`fuel_accounting_revision`・`language_revision` は変えない。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §12.3・§8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜34 は done を含めてすべて使われていた。`tests/slices_contract.rs` が `order` の重複を弾くので、先例（PR #368・#381・#388 の Q1=A、PR #403・#408・#410）と同じく、設計 PR では仮にすべてのスライスより後（101）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR で 35 より前へ移す。101 は仮の値である。
- **想定の変更規模:** `src/` は 20〜60 行程度（`new_iter_str_list` の受け取りと順序 10〜25 行、課金してから包む `Tracked` の作り方 5〜20 行、tree・VM の呼び出し 各 3〜10 行）。テストは `tests/` に 60〜150 行。触る仕組みは「`for` の反復列の要素の作り方」1 つ（tree・VM 共通の 1 関数）。受入基準は 3 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **FCB-AT-01（課金で止まる要素を複製しない）:** 長い key（例: 1 MiB）を 1 つ持つ Dict の `for` で、その key の §5.3 の累積の課金が上限を超える実行（例: key を作る分だけ `max_string_bytes` を残す `BudgetConfig`）が `BudgetExceeded`（`string_limit`）の terminal で止まり、その実行の確保バイト数が、同じ Dict を作って `for` しない対照の script との差で key の長さより小さい（止まる要素が複製されない）。String body の live heap の課金で止まる実行（`max_live_heap_bytes` を小さくしたもの）でも同じ。tree-walk 版と VM 版で確かめる。確かめ方: `tests/scaling.rs` のカウントアロケータ（`allocated_bytes`）で確保量を測る統合テスト（両 backend）。
  - **FCB-AT-02（成功時の要素は変わらない）:** Dict（長さの違う key・複数 byte の文字を含む key）と Str（ASCII と複数 byte の文字）の `for` の反復の値と順序、ループの前後の `string_allocations`・`string_bytes`・`peak_heap_bytes`・`live_heap_bytes` が変更前と同じで、tree と VM で string の累積が完全一致する。確かめ方: 既存の `tests/for_iter_charge.rs`（FIS-AT-01..04）・`tests/differential.rs` の `for` の反復列の突合・golden fixture `for_iter_shared` が変更なしで通ること。期待値を変えたテストがあれば PR 本文に列挙する。
  - **FCB-AT-03（上限の terminal の値は変わらない）:** FIS-AT-04 の上限の実行（`StringAllocations`・collection の上限・`HeapBytes`）の terminal の resource・`used`・`requested` と、その時点の usage が変更前と同じである（`StringAllocations` と collection の上限は tree と VM で完全一致）。確かめ方: 既存の FIS-AT-04 のテストが変更なしで通ることと、`max_single_string_bytes` を key より小さくした Dict の `for` が `SingleStringBytes` の terminal（`requested` = key の byte 長）になることを確かめる統合テスト（両 backend）。

### fix-pause-run-turn-remove 実装スライス（pause した handle を run-turn から外し、resume で末尾へ戻す）

[issue #361](https://github.com/kamonabe1927/Tsumugi/issues/361) から作ったスライス（不具合の修正、`slices.toml` の `fix-pause-run-turn-remove`）。issue の仕分けで「受け入れ」とした。直し方は §9.2 の `Paused`（「hostが明示resumeするまでrun-turn queueへ戻らない」）と `Paused --resume--> resume_to`、§11（Paused 中に許可する操作）、§12.1 の規則 1・3（Ready になった execution ID を queue 末尾へ置き、同時点で Ready な execution より前へ挿入しない）で決まり、目標の章は変えない。resume で run-turn へ戻す時点は判断役が選択した（PR #416、Q1=A）。

- **現状の不具合:** 冒頭の逸脱一覧の `ExecutionHandle::pause` の行のとおり。`pause`（`src/engine.rs`）は state を `Paused(PausedState { resume_to, .. })` にするだけで、`run_turn_remove` を呼ばず `run_turn_registered` も `true` のまま残す。`poll` は step 2 で `Paused` を `InvalidState` にして返すので、paused の handle は step 5・6 に届かない。それが run-turn queue の先頭にいると、ほかの handle の poll はすべて step 5 で非先頭になって `SchedulerPreempted` を返し、resume されるまで Engine 全体が進まない（issue #361 の再現: `limits(2, 0)` で h1 を `SliceFuelExhausted`、h2 を `SchedulerPreempted` にしてから h1 を pause すると、h2 を 1000 回 poll しても fuel が 0 のまま）。`pause` の doc の「resume まで run-turn へ戻らない」は、queue から外していないので事実と異なる。
- **実装範囲:**
  - `pause` が成功するとき、handle が run-turn queue に登録されていれば（`run_turn_registered`）`run_turn_remove` で外し、`run_turn_registered = false` にする。外したかどうかを handle に覚えておく（例: `bool` の field 1 つ。形は実装に任せる）。登録されていなかった handle（まだ poll していない `Created`・`Linked`、`Yielded(AdmissionQueued)`、`Yielded(HostCallPending)`）では run-turn に触らない。
  - `resume` が成功するとき、pause で run-turn から外した handle なら、`run_turn_push_back` で run-turn queue の末尾へ登録して `run_turn_registered = true` に戻す（Q1=A、判断役が選択（PR #416）。§9.2 の `Ready` は「link済みでrun-turn queueにいる」、§12.1 の規則 1 は「Readyになったexecution IDをqueue末尾へ置く」）。外していなかった handle は今と同じく run-turn に触らず、次の poll の step 3〜5 で扱う。どちらの場合も state は今と同じく `resume_to` へ戻す。
    - **決定記録（判断役が選択（PR #416）、Q1=A）:** resume の時点で、pause で外した handle だけを run-turn queue の末尾へ登録する。理由: §9.2 は `Ready` を「link済みでrun-turn queueにいる」、`Paused` を「hostが明示resumeするまでrun-turn queueへ戻らない」（明示の resume が戻る契機）と定め、§12.1 規則 1 は「Readyになったexecution IDをqueue末尾へ置く」とする。A は resume で `resume_to` に戻った時点で公開 state と queue の登録が一致し、目標の章に新しい逸脱を作らない。queue の順序が host の明示の操作（resume）の順で決まり、あとの poll の順に左右されないので、[マニフェスト](manifesto.md)の優先順位の表「一貫した意味論」と原則 5・7 に合う。fix-host-call-resume-requeue が poll の時点の登録にしたのは、完了の wake が別 thread から来て handle に触れないためで、resume は handle の作成 thread で host 自身が呼ぶので同じ制約が無い。追加は field 1 つと数行で原則 8 の範囲に収まり、PRT-AT-02 の h3 の検査で resume の時点の登録を再現可能に確かめられる（原則 9）。resume 済みの handle が先頭になって poll されなければ進行が止まる点は、§9.1 が Ready の handle 一般について host の責任として定めている範囲で、A に固有ではない。採らなかった案: B（resume は state を戻すだけにし、次の poll の step 5 で末尾へ登録する）は実装が数行小さいが、resume から次の poll までの間、公開 state が `Ready`（または slice yield の `Yielded`）なのに run-turn queue にいない状態ができ、§9.2 の `Ready` の定義に対する新しい逸脱になる。その間に初めて poll された別の handle が先に登録され、resume した順ではなく poll の順で位置が決まる。
  - pause 中は run-turn にいないので、pause 中の drop・cancel の経路（`release_active` の run-turn の除去、resume 後の poll の checkpoint）は今のままで、run-turn に古い登録が残らない。
  - `pause`・`resume` の doc と、`run_turn_registered` の field の doc を、pause で run-turn から外し resume で末尾へ戻すことが分かる記述に直す。
  - done にする実装 PR で、本書の逸脱一覧の該当行を削除する。
- **範囲外:**
  - terminal の handle の active slot の解放（[issue #362](https://github.com/kamonabe1927/Tsumugi/issues/362)）・非 terminal の handle の drop と cancel token（[issue #363](https://github.com/kamonabe1927/Tsumugi/issues/363)）。同じ scheduler/handle に触れるが、admission slot・drop の別の仕組みで、[開発ループ手順書](loop-runbook.md) §14.1 の「触る仕組みは 1 つまで」に合わせて別のスライスにする（このスライスは pause・resume と run-turn queue の登録 1 つ）。
  - pause 中の active slot・deadline の扱い（§9.2・§12 のとおり pause 中も active slot を消費し deadline も進む。今の実装のまま変えない）。pause 中の `poll` が `InvalidState` を返すことも変えない。
  - resume で waker を鳴らすこと。§9.1 の waker を鳴らす契機（admission 取得、host/audit ticket 完了、cancel、deadline timer）に resume は無く、resume は host 自身が呼ぶので変えない。
  - 同期入口 `Engine::execute`・`execute_repl_submission` の `ConcurrentRunRequiresPolling`（fix-sync-run-concurrent-handle の範囲）。
  - VM。scheduler の handle 経路は tree evaluator（`ExecutionContext` の評価器）だけで実行し、VM はこの経路に無い（逸脱一覧・未実装範囲の「VM の式レベル continuation」）。言語の挙動は変わらない。
- **課金の変化:** なし。pause・resume は semantic work をせず、fuel・heap・I/O の課金は変わらない。run-turn queue の登録だけを直す。`fuel_accounting_revision`・`heap_accounting_revision`・`language_revision` は変えない。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜34 は done を含めてすべて使われていた。`tests/slices_contract.rs` が `order` の重複を弾くので、先例（PR #368・#381・#388 の Q1=A、PR #403・#408・#410・#412）と同じく、設計 PR では仮にすべてのスライスより後（102）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR で 35 より前へ移す。102 は仮の値である。
- **想定の変更規模:** `src/` は 10〜40 行程度（`src/engine.rs` の `pause`・`resume` の run-turn の除去と再登録、覚えておく field 1 つ、doc の修正）。テストは `tests/scheduler_api.rs` に 150〜300 行（issue #359 の再現の形にならった 2〜3 handle の往復）。触る仕組みは「pause・resume と run-turn queue の登録」1 つ。受入基準は 4 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **PRT-AT-01（issue #361 の再現で他の handle が進む）:** `limits(2, 0)` の `Engine` で、`while` で長く回る script の h1・h2（別の context）を作る。h1 を `PollSlice { max_fuel: 64 }` で poll して `SliceFuelExhausted`、h2 を同じ slice で poll して `SchedulerPreempted` にしてから h1 を pause する。そのあと h2 を同じ slice で poll すると、最初の poll から `SchedulerPreempted` を返さず slice を進め（`usage()` の fuel が増える）、上限の往復回数（例: 5000）の中で `Completed` の terminal に達する。その間 h1 の `state()` は `Paused` のままで、`usage()` は pause の時点から変わらない。確かめ方: `tests/scheduler_api.rs` の統合テスト。
  - **PRT-AT-02（resume で末尾へ戻り、round-robin で進む）:** PRT-AT-01 の形で h2 が terminal に達する前に h1 を resume すると、h1 は resume の時点で run-turn queue の末尾に置かれ、その時点で queue にいる h2 より前へ挿入されない（§12.1 規則 1・3）。以後 h1・h2 は先頭の handle の 1 回の poll で 1 slice ずつ交互に進み（規則 2・5。他方が terminal に達したあとを除く）、どちらも `Completed` に達する。h1 の結果は pause しなかった場合と同じである（continuation を保ったまま続ける）。resume のあと h1 を poll する前に、まだ poll していない 3 つめの handle h3（`limits(3, 0)` にした場合）を初めて poll すると、h3 は h1 より後ろに登録される（Q1=A の resume の時点の登録）。確かめ方: 同じ統合テストで、各 poll の戻り値（`SliceFuelExhausted` / `SchedulerPreempted` / terminal）の並びと `usage()` の fuel の増え方を確かめる。
  - **PRT-AT-03（run-turn にいた handle も、いなかった handle も、pause の前後で整合する）:** 非先頭の poll を受けて run-turn にいる `Created` の handle（`create_execution` のあと非先頭で `SchedulerPreempted` を受けたもの）を pause しても、先頭の handle が今と同じく進み、pause した handle が先頭になって他を止めることがない。run-turn にいなかった handle（まだ poll していない `Created`、`Yielded(HostCallPending)`）を pause して resume すると、resume では run-turn に入らず（ほかの handle の poll が `SchedulerPreempted` にならない）、次の poll から今と同じに進む（`HostCallPending` は ticket の完了を確かめる step 4 に入る）。確かめ方: 同じファイルの統合テスト。
  - **PRT-AT-04（ほかの pause・scheduler の経路は変わらない）:** pause 中も active slot を消費し、pause 中の cancel は resume 後の poll で `Cancelled` になり、paused の handle を drop すると slot が解放されて run-turn に古い登録が残らない（drop のあと他の handle が先頭で進む）。`tests/engine_api.rs` の pause のテスト（`pause_from_created_resumes_to_created_and_completes`・`pause_from_yielded_preserves_reason_and_completes`・`cancel_during_pause_then_resume_reaches_cancelled`・`pause_resume_returns_to_resume_to`・`yield_pause_resume_preserve_continuation_host_pending` ほか）と `tests/scheduler_api.rs` の既存のテスト（`paused_consumes_active_slot`・`host_resume_non_head_becomes_ready_and_round_robin_completes` ほか）が変更なしで通る。確かめ方: 統合テストと `cargo test`。期待値を変えたテストがあれば PR 本文に列挙する。

### fix-terminal-release-slot 実装スライス（Terminal に達した時点で admission slot を 1 回だけ解放する）

[issue #362](https://github.com/kamonabe1927/Tsumugi/issues/362) から作ったスライス（不具合の修正、`slices.toml` の `fix-terminal-release-slot`）。issue の仕分けで「受け入れ」とした。直し方は §9.2 の `Terminal`（「active/queue slot、pending reservation、continuationを解放する。context commit/rollback後の状態と`ExecutionOutcome`だけをcallerへ公開する」）と §12（「active slot解放時は最古のAdmissionQueued handleのqueue slotをactiveへatomic変換する」「link失敗、cancel、drop時もqueue/active slotを1回だけ解放する」）で決まり、目標の章は変えない。

- **現状の不具合:** 冒頭の逸脱一覧の Terminal の handle の slot の行のとおり。`ExecutionHandle::finish_terminal`（`src/engine.rs`）は state を `Terminal` にして outcome を保存するだけで、末尾 field の `slot: AdmissionSlot` には触れない。slot を解放するのは `AdmissionSlot` の `Drop`（`src/scheduler.rs`。`release_active` / `release_queued`）だけで、`ExecutionHandle` の `Drop` も Terminal なら何もせず field drop に任せる。Terminal の handle を `outcome()`・`usage()` を読むために保持していると active slot が使われたままになり、後続の `create_execution` / `start` が `Backpressure` になる（issue #362 の再現: `limits(1, 0)` で h1 を `Completed` まで poll して保持したまま、別の context で `create_execution` すると `Err(Backpressure { active: 1, queued: 0, limit: 1 })`）。queue がある場合は後続の handle が `AdmissionQueued` のまま昇格しない。`Yielded(AdmissionQueued)` の handle が step 3 の cancel・deadline で Terminal になった場合も queue slot が drop まで残る。
- **実装範囲:**
  - `finish_terminal` で、state を `Terminal` にするのと同じ呼び出しの中で、handle が持つ admission slot を解放する。解放の中身は今の `AdmissionSlot` の `Drop` と同じ（`SlotState` が Active なら `release_active`、Queued なら `release_queued`）で、active を解放したときは今と同じく同じ lock の下で admission queue の最古の entry を active へ昇格させる（§12）。`finish_terminal` を通る Terminal の経路（step 3 の AdmissionQueued 中の cancel・deadline、step 4 の HostCallPending 中の cancel・deadline と `resume_host_call` の失敗、step 6 の `SlicePoll::Terminal`。link 失敗を含む）はすべてこれで解放される。
  - 解放は 1 回だけにする。Terminal で解放したあとの handle の drop では、`AdmissionSlot` の `Drop` が `SlotState` の Released の判定（`release_active` / `release_queued` の先頭の no-op）で二重解放しない。`active` の数がずれない（減りすぎない）ことと、昇格が 2 回起きないことを受入基準で確かめる。`AdmissionSlot` に解放の入口（例: `Drop` と共有する `release(&self)` のような関数）を足すか、`finish_terminal` から `SchedulerShared` の解放を直接呼ぶかは実装に任せる。どちらでも RAII（drop での解放）は非 terminal の handle のために残す。
  - 解放は run-turn queue からの除去を含む（`release_active` が今と同じく run-turn から外す）。今 `finish_terminal` の前に呼んでいる `run_turn_remove` は残してよい（重ねて呼んでも no-op）。
  - Terminal のあとの `state()`・`outcome()`・`usage()` は今と同じ値を返す（context と outcome は handle が持ち続ける。§9.2 の「`ExecutionOutcome`だけをcallerへ公開する」）。
  - `finish_terminal` の doc、`ExecutionHandle` の `Drop` のコメント（「slot は field drop で解放される」）、`slot` field の doc を、Terminal に達した時点で解放し drop では二重解放しないことが分かる記述に直す。
  - done にする実装 PR で、本書の逸脱一覧の該当行を削除する。
- **範囲外:**
  - active slot の解放で昇格した handle の waker を鳴らすこと。`release_active` は昇格した entry の waker を placeholder（`Option<()>`）として退避するだけで鳴らさない。§9.1 の「admission取得 … は状態をreadyにした後でwakerを呼ぶ」に対する別の差で、waker の配線の仕組みなのでこのスライスでは触らない（昇格は今と同じく次の poll の step 3 で観測する）。
  - pause した handle の run-turn の登録（fix-pause-run-turn-remove、[issue #361](https://github.com/kamonabe1927/Tsumugi/issues/361)）・非 terminal の handle の drop と cancel token（[issue #363](https://github.com/kamonabe1927/Tsumugi/issues/363)）・同期入口の `ConcurrentRunRequiresPolling`（fix-sync-run-concurrent-handle）。同じ scheduler/handle に触れるが、それぞれ pause・drop・同期入口の別の仕組みで、[開発ループ手順書](loop-runbook.md) §14.1 の「触る仕組みは 1 つまで」に合わせて別のスライスにする（このスライスは Terminal での admission slot の解放 1 つ）。非 terminal の handle の drop で slot を解放する経路は今のまま変えない。
  - pending reservation・continuation の解放（§9.2 の `Terminal` の残り）。continuation と transaction は Terminal の前に `run_slice`・`abort_session` が始末しており、このスライスでは触らない。
  - 同期の `Engine::execute`・`execute_repl_submission`。handle を内部で作って terminal まで poll し、戻る前に drop するので、今も解放の遅れは外から観測できない（このスライスの変更で挙動は変わらない）。
  - VM。scheduler の handle 経路は tree evaluator（`ExecutionContext` の評価器）だけで実行し、VM はこの経路に無い（逸脱一覧・未実装範囲の「VM の式レベル continuation」）。言語の挙動は変わらない。
- **課金の変化:** なし。slot の解放は semantic work をせず、fuel・heap・I/O の課金は変わらない。admission の会計（`active`・admission queue）の解放の時点だけを直す。`fuel_accounting_revision`・`heap_accounting_revision`・`language_revision` は変えない。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜34 は done を含めてすべて使われていた。`tests/slices_contract.rs` が `order` の重複を弾くので、先例（PR #368・#381・#388 の Q1=A、PR #403・#408・#410・#412・#416）と同じく、設計 PR では仮にすべてのスライスより後（103）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR で 35 より前へ移す。103 は仮の値である。
- **想定の変更規模:** `src/` は 15〜50 行程度（`src/engine.rs` の `finish_terminal` の解放の呼び出しと doc・`Drop` のコメント、`src/scheduler.rs` の `AdmissionSlot` の解放の入口と doc）。テストは `tests/scheduler_api.rs` に 150〜300 行（issue #362 の再現と、queue・cancel・drop の往復）。触る仕組みは「Terminal での admission slot の解放」1 つ。受入基準は 4 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **TRS-AT-01（issue #362 の再現で後続が active を取れる）:** `limits(1, 0)` の `Engine` で、`let a = 1` の h1 を `create_execution` して `Completed` の terminal まで poll し、h1 を保持したまま（drop せず）別の context で `create_execution` すると `Backpressure` を返さず handle h2 を作れる。h2 は poll で `Completed` に達する。その間 h1 の `state()` は `Terminal`、`outcome()` は `Some(Completed)` のままである。`RuntimeError`（`tests/engine_api.rs` の実行時エラーのテストの形）と `LinkError`（同じファイルの存在しない import のテストの形）の terminal でも同じく後続が active を取れる。確かめ方: `tests/scheduler_api.rs` の統合テスト。
  - **TRS-AT-02（queue の最古が Terminal の時点で昇格する）:** `limits(1, 1)` で h1 を active、h2 を `Yielded(AdmissionQueued)` にし、h1 を terminal まで poll する（h1 は保持したまま）。h1 が terminal を返した直後の h2 の poll は `AdmissionQueued` を返さず、`resume_to` から進んで terminal に達する。h1 が terminal を返す前の h2 の poll は今と同じく `AdmissionQueued` を返す。h1 を terminal にしたあと、別の context で 3 つめを `create_execution` すると、active は h2、queue は空なので queue に入る（`Yielded(AdmissionQueued)`）。確かめ方: 同じファイルの統合テスト。
  - **TRS-AT-03（解放は 1 回だけで、drop で二重解放しない）:** TRS-AT-01・02 の形で Terminal の handle を drop しても、active の数は減りすぎず、昇格は二度起きない。`limits(1, 1)` で h1 を terminal にして h2 が昇格したあと h1 を drop すると、h2 が active を持ったまま、別の context の `create_execution` は queue に入り（active を取らない）、さらにもう 1 つは `Backpressure { active: 1, queued: 1, limit: 1 }` を返す。`Yielded(AdmissionQueued)` の handle を cancel して poll で `Cancelled` の terminal にすると、その時点で queue slot が空き（別の `create_execution` が queue に入れる）、そのあとの drop でも queue の数がずれない。昇格済みでまだ poll で観測していない `Yielded(AdmissionQueued)` の handle（`AdmissionSlot` の kind は Queued のまま `SlotState` が Active）を poll の前に cancel して `Cancelled` の terminal にすると、step 3 の checkpoint の経路でも active が解放されて queue の次が昇格し、そのあとの drop でも active の数がずれない（例: `limits(1, 2)` で h1 を active、h2・h3 を queue に入れ、h1 を terminal にして h2 を昇格させる。h2 は poll しないまま cancel して poll すると `Cancelled` を返し、そのあとの h3 の poll は `AdmissionQueued` を返さずに `resume_to` から進む）。確かめ方: 同じファイルの統合テスト。
  - **TRS-AT-04（ほかの slot・scheduler の経路は変わらない）:** 非 terminal の handle の drop は今と同じく slot を解放する（`tests/scheduler_api.rs` の `paused_consumes_active_slot` の drop 後の admission など）。Paused と host 待ちの handle は今と同じく active slot を消費し続ける（`paused_consumes_active_slot`・`host_waiting_consumes_active_slot`）。`active_and_queue_limits_exact_then_backpressure`・`n_ready_executions_advance_fifo_one_slice_each`、`tests/engine_api.rs` の `admission_full_create_returns_no_handle_no_context_change`、`src/scheduler.rs` の単体テスト（`release_active_promotes_fifo_head_and_keeps_active_single_sourced` ほか）が変更なしで通る。確かめ方: 既存のテストと `cargo test`。期待値を変えたテストがあれば PR 本文に列挙する。

### fix-handle-execution-cancel-token 実装スライス（scheduler の handle ごとに cancellation token を作り、drop・cancel がその execution だけを止める）

[issue #363](https://github.com/kamonabe1927/Tsumugi/issues/363) から作ったスライス（不具合の修正、`slices.toml` の `fix-handle-execution-cancel-token`）。issue の仕分けで「受け入れ」とした。直し方は §8（cancel は token の `false -> true` を linearization point とし、「正常結果がterminalへcommitした後のcancelは結果を変えない」）、§9.1（`ExecutionHandle::cancellation_token` は handle の token を返し、「非terminal handleのdropはcancelをlinearizeし … language-stateをrollbackし、logical `Terminal(Cancelled)`をappendする」）、§9.2（`Created` は「root の`CompiledScript`、context、config、tokenを所有」する。token は execution が所有する）、§10（cancel の terminal は language-state を rollback するだけで、context を以後使えなくしない）と、[脅威モデル](threat-model.md) TM-020（context の再利用）で決まり、目標の章は変えない。逸脱一覧の「scheduler の handle 経路の `ExecutionRequest` は field を持たない骨格」の行（request で token を渡せない）とは別の、帰結の不具合として切る。

- **現状の不具合:** 冒頭の逸脱一覧の handle 経路の cancellation token の行のとおり。`ExecutionHandle::cancellation_token`（`src/engine.rs`）は `self.context.evaluator.cancellation_token()`、つまり `ExecutionContext` の評価器の `BudgetLedger` が持つ 1 つの token の clone を返す。この token は `ExecutionContext::new` で作られ、`Engine::create_execution`・`start`（`new_handle`）も初回 poll の `begin_execution` も据え直さない。非 terminal の handle の `Drop` は `self.cancellation_token().cancel()` を呼ぶので、その context の token が cancel 済みのまま残り、以後の `create_execution`・`start`・`execute`・`execute_repl_submission` はすべて最初の checkpoint で `Cancelled` になる（issue #363 の再現: `limits(2, 0)` で `let a = 1` を `create_execution` し、poll せずに drop すると、同じ context の `execute` は 2 回とも `Err(実行がキャンセルされました)`、新しい context では `Ok(Completed)`）。`Drop` のコメントの「context は poison しない」は事実と異なる。前の execution の token の clone を host が保持していて、その execution の terminal のあとに `cancel()` した場合も、同じ context の次の execution が止まる（§8 の「commit 後の cancel は結果を変えない」に反し、後続の execution まで変える）。token に登録した waker（`set_waker`）も次の execution へ持ち越される。
- **実装範囲:**
  - `Engine::create_execution`・`start` が admission に成功して handle を組み立てるとき（`new_handle`）、新しい `CancellationToken` を作って context の評価器の `BudgetLedger` へ据える。これで token は execution ごとになり、handle の `cancellation_token()` は作成時点から terminal・drop まで同じ token（その execution の token）の clone を返す。同期の `execute`・`execute_repl_submission` は `create_execution` を通るので同じく execution ごとの token になる。
  - 据えるのは token だけにする。`BudgetLedger` の config・counters・heap ledger・clock は今のまま変えない（`reset_config_with_token` は ledger 全体を作り直すので、この用途には使わない）。token だけを差し替える入口（例: `BudgetLedger` と `Evaluator` に token を置き換える関数を 1 つずつ）を足すかどうか、その名前と形は実装に任せる。
  - 据えるのは admission が成功したあとだけにする。`StartError::Backpressure` のときは今と同じく handle を作らず、context（token を含む）を変えない（§9.1）。
  - 非 terminal の handle の `Drop` は今と同じく `cancellation_token().cancel()` で cancel を linearize し、pending host call を cancel し、`abort_session` で language-state を rollback する（§9.1）。cancel するのはその execution の token だけなので、同じ context の次の execution は新しい token で始まる。
  - `cancellation_token`・`set_waker`・`Drop` の doc とコメント（「context は poison しない」を含む）、`new_handle` の doc を、token が execution ごとに作られ、drop・cancel がその execution だけを止めることが分かる記述に直す。
  - done にする実装 PR で、本書の逸脱一覧の該当行を削除する。
- **範囲外:**
  - `ExecutionRequest`（scheduler の handle 経路）で host が token を渡せるようにすること（§3 の `ExecutionRequest.cancellation`）。逸脱一覧の「field を持たない骨格」の行の範囲で、公開 API の変更なのでこのスライスでは触らない。このスライスは request に field を足さず、handle ごとに `CancellationToken::new()` を作る。
  - facade（`src/embedding.rs` の `Engine::run`・`run_audited`）の経路。実行の前に request が所有する token を `reset_budget` で据えており、この不具合は無い。今のまま変えない。
  - terminal の handle の active slot の解放（fix-terminal-release-slot、[issue #362](https://github.com/kamonabe1927/Tsumugi/issues/362)）・pause した handle の run-turn の登録（fix-pause-run-turn-remove、[issue #361](https://github.com/kamonabe1927/Tsumugi/issues/361)）・同期入口の `ConcurrentRunRequiresPolling`（fix-sync-run-concurrent-handle）。同じ scheduler/handle の `Drop` の近くに触れるが、admission slot・run-turn・同期入口の別の仕組みで、[開発ループ手順書](loop-runbook.md) §14.1 の「触る仕組みは 1 つまで」に合わせて別のスライスにする（このスライスは handle の cancellation token の持ち方 1 つ）。
  - scheduler の handle 経路で budget の counters・config を execution ごとに据え直すこと。token 以外の ledger の持ち方はこのスライスでは変えない。
  - drop の監査（orphan-audit queue・`AuditUnavailable`。§9.1）。今の `Drop` のコメントのとおり Phase 6 の範囲で、このスライスでは触らない。
  - VM。scheduler の handle 経路は tree evaluator（`ExecutionContext` の評価器）だけで実行し、VM はこの経路に無い（逸脱一覧・未実装範囲の「VM の式レベル continuation」）。言語の挙動は変わらない。
- **課金の変化:** なし。token の差し替えは semantic work をせず、fuel・heap・I/O の課金は変わらない。`fuel_accounting_revision`・`heap_accounting_revision`・`language_revision` は変えない。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜34 は done を含めてすべて使われていた。`tests/slices_contract.rs` が `order` の重複を弾くので、先例（PR #368・#381・#388 の Q1=A、PR #403・#408・#410・#412・#416・#419）と同じく、設計 PR では仮にすべてのスライスより後（105）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR で 35 より前へ移す。105 は仮の値である。
- **想定の変更規模:** `src/` は 15〜50 行程度（`src/engine.rs` の `new_handle` での token の据え付けと doc・`Drop` のコメント、`src/eval.rs`・`src/budget.rs` の token だけを差し替える入口と doc）。テストは `tests/scheduler_api.rs`（または `tests/engine_api.rs`）に 150〜300 行（issue #363 の再現と、drop・cancel・waker の往復）。触る仕組みは「handle の cancellation token の持ち方」1 つ。受入基準は 4 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **HCT-AT-01（issue #363 の再現で同じ context の次の execution が完了する）:** `limits(2, 0)` の `Engine` で `let a = 1` を compile し、context `ctx` で `create_execution` した handle を poll せずに drop したあと、同じ `ctx` の `engine.execute` は 2 回とも `Ok(Completed)` を返し、同じ `ctx` で `create_execution` した handle は poll で `Completed` の terminal に達する。`Linked`（`start` のあと poll せず）・`Yielded(SliceFuelExhausted)`（`while` の script を `PollSlice { max_fuel: 16 }` で 1 回 poll）・`Paused` の handle を drop した場合も同じく、同じ context の次の execution が `Completed` に達する。確かめ方: 統合テスト。
  - **HCT-AT-02（前の execution の token は次の execution を止めない）:** 同じ context で h1 の `cancellation_token()` の clone t1 を取り、h1 を `Completed` まで poll して drop したあとに `t1.cancel()` を呼んでも、同じ context の次の execution h2 は `Completed` に達する。h1 を t1 の cancel で `Cancelled` にしたあと（t1 は cancel 済み）も、同じ context の h2 は `Completed` に達し、h2 の `cancellation_token()` の t2 は `is_cancelled() == false` で始まり、`t2.cancel()` は `true` を返す。h2 の作成のあとに t1 を cancel しても h2 は止まらず、t2 を cancel すると h2 は次の poll で `Cancelled` になる。確かめ方: 統合テスト。
  - **HCT-AT-03（token は作成時点から execution に固定される）:** `create_execution` 直後（`Created`）に取った token の clone を最初の poll の前に cancel すると、最初の poll で `Cancelled` の terminal になる（`cancel_before_poll_reaches_cancelled_terminal` と同じ）。同じ handle から `Created`・poll 後の `Yielded`・terminal で取った clone はどれも同じ token で、どれを cancel しても他の clone の `is_cancelled()` が `true` になる。h1 で `set_waker` した waker は、h1 の drop のあと同じ context で作った h2 の token の cancel では鳴らない（h2 で `set_waker` した waker だけが鳴る）。確かめ方: 統合テスト（waker は呼び出し回数を数える fake）。
  - **HCT-AT-04（ほかの cancel・drop の経路は変わらない）:** 非 terminal の drop は今と同じくその execution の token を cancel し（drop 前に取った clone の `is_cancelled()` が `true`）、pending host call の ticket を cancel し、language-state を execution 開始時点へ rollback する。terminal 後の cancel は outcome を変えない。`Backpressure` のときは handle を作らず context を変えない。`tests/engine_api.rs` の `cancel_before_poll_reaches_cancelled_terminal`・`cancel_across_yield_reaches_cancelled_terminal`・`cancel_after_completion_does_not_change_outcome`・`cancel_during_pause_then_resume_reaches_cancelled`・`cancel_token_is_idempotent_across_clones`・`set_waker_registers_on_cancellation_token`・`nonterminal_drop_cancels_and_rolls_back_without_blocking`・`cancel_from_each_nonterminal_state_single_terminal`・`host_response_vs_cancel_linearization`・`admission_full_create_returns_no_handle_no_context_change`、`tests/scheduler_api.rs` の `cancel_from_another_thread_smoke`・`cancel_works_while_host_waiting_and_foreign_clock_rejected`・`prt_at_04_cancel_and_drop_while_paused_leave_no_stale_run_turn_entry`、facade の request token の cancel のテスト（`tests/sync_run_host_call.rs` の `cancel_during_wait_ends_before_deadline_and_rolls_back`、`tests/audit_core.rs` の `with_sink_pre_cancel_has_started_and_cancelled_terminal`）が変更なしで通る。確かめ方: 既存のテストと `cargo test`。期待値を変えたテストがあれば PR 本文に列挙する。

### fix-fs-builtin-host-call-charge 実装スライス（filesystem builtin のすべてを host call として回数・byte を課金する）

[issue #364](https://github.com/kamonabe1927/Tsumugi/issues/364) から作ったスライス（不具合の修正、`slices.toml` の `fix-fs-builtin-host-call-charge`）。issue の仕分けで「受け入れ」とした。直し方は §2 の用語（host call は「capability境界を越える … filesystem … 等の呼出し」）と §6.1（任意 host call は capability 判定の前に `host_calls += 1`、request を `host_request_bytes`、response を `host_response_bytes`、双方を `host_call_bytes` へ commit。capability 拒否でも count と request bytes を課金し、response bytes は 0）で決まり、目標の章は変えない。filesystem builtin ごとの request/response の byte の数え方（下の「byte の数え方」）は、§6.1 が「serialize形式は[Capability仕様](capability-model.md)で操作ごとに固定する」とするのに対して Capability 仕様が filesystem の操作ごとの形式を定めていないため、設計 PR の判断の候補 Q1 に出し、判断役が A を選択した（PR #425、Q1=A）。

- **現状の不具合:** 冒頭の逸脱一覧の filesystem builtin の host call の行のとおり。tree（`src/builtin.rs` の `apply_pure_core_builtin`）と VM（`src/vm.rs` の `exec_builtin`）は、`builtin_core::is_host_call_builtin`（`src/builtin_core.rs`）が真のときだけ `charge_host_call_request`・`charge_host_response_bytes` を呼ぶ。この述語は `read_file` / `read_lines` / `write_file` / `append_file` の 4 つだけを真にするので、`is_filesystem_builtin` に含まれる残りの 10 個（`path_exists` / `is_file` / `is_dir` / `file_size` / `list_dir` / `mkdir` / `remove` / `remove_dir` / `remove_tree` / `rename`）は capability 判定と adapter の呼び出しを通るのに `host_calls` を数えず、`max_host_calls`（既定 10,000）を素通りする。issue #364 の再現: `--fs-root d=./d --fs-op d=metadata,read` で `path_exists("@d/x")` を 20,000 回呼ぶ loop は `done 20000` まで進み、同じ loop で `read_file` を `try` で呼ぶと 10,001 回目で「host call の呼び出し回数が上限を超えました」で止まる。述語の doc の「path 判定・env・clock 等その他の host 境界 builtin の課金は Phase 2 の capability / host function 配線で扱う」は、filesystem builtin の C10 の capability 配線のあとも課金に引き継がれていない。
- **実装範囲:**
  - 課金の対象を `is_filesystem_builtin` の 14 個すべてにする。tree の `apply_pure_core_builtin` と VM の `exec_builtin` で、14 個のどれでも `dispatch_fs` の前（capability 判定・パスの形式検査・adapter の呼び出しより前）に `charge_host_call_request` で count と request bytes を課金し、結果が `Ok` のときだけ `charge_host_response_bytes` で response bytes を課金する。課金の位置と順序は今の 4 つと同じにする（tree と VM で同じ論理位置）。
  - **byte の数え方（判断役が選択（PR #425）、Q1=A）:** 今の 4 つの規則（path 引数の byte は request に含めない。返り値の `Bool` は payload ではない）をそのまま 10 個へ当てはめる。request bytes は 10 個とも 0（`write_file` / `append_file` の書き込み内容に当たる payload が無い）。response bytes は `list_dir` だけが返した名前の UTF-8 byte 長の合計（`read_lines` の各行の合計と同じ形）で、`path_exists` / `is_file` / `is_dir` / `mkdir` / `remove` / `remove_dir` / `remove_tree` / `rename` の `Bool` と `file_size` の `Int` は 0。`host_call_request_bytes`・`host_call_response_bytes` をこの規則に合わせる。理由: 10 個すべての回数が `max_host_calls` に入り、host から受け取る量が呼び出しごとに変わる唯一の操作である `list_dir` の名前が `host_response_bytes`・`host_call_bytes` の予算に入る（[マニフェスト](manifesto.md)原則 3、§6.1 の「response を `host_response_bytes` へ」）。path 引数は script 側の文字列で、fuel・heap・`max_single_string_bytes` と呼び出し回数で既に有限なので、request に数えなくても予算の外へ出る量は生じない。現行の決定（REV-015 Slice 2 の「path は request に含めない」「`Bool` は payload ではない」）をそのまま延ばし、14 個が 1 つの規則で揃い、既存の 4 つの観測値と境界を変えない（原則 5・原則 8）。採らなかった案: B（14 個すべてで path の byte を request に数える）は、既存の 4 つの request bytes と `TSUMUGI_MAX_HOST_REQUEST_BYTES`・`TSUMUGI_MAX_HOST_CALL_BYTES` の境界、監査の `BudgetCharged` の量まで変え、issue #364 の範囲と「触る仕組みは 1 つ」を超える。path を request payload とするかは、Capability 仕様で filesystem の操作ごとの形式を定めるときに 14 個まとめて決める。C（回数だけを数え byte は 0）は、`list_dir` が host から受け取る名前の量が予算に入らず原則 3 と §6.1 に反しうるので選ばない。
  - 監査の host-call triple（`HostCallStarted` → `CapabilityDecision` → `HostCallFinished`）と、その前の `BudgetCharged` の flush（`flush_charges_into_audit_events`）は、今の 4 つだけに出す形を変えない。tree の `apply_pure_core_builtin` で「課金するか」と「triple を出すか」を別の述語に分ける（名前と形は実装に任せる）。triple を 10 個へ広げるのは [決定性・実行時監査仕様](determinism-and-audit.md) の逸脱一覧の行（fs builtin のうち 4 つだけが triple を出す）の延期先 `audit-fs-builtin-scope` の範囲である。
  - `is_host_call_builtin` の doc（課金対象を 4 つに限る理由の記述）と、課金の位置のコメント（`src/builtin.rs`・`src/vm.rs` の「read 系は request 0 byte」など）を、14 個すべてを host call として課金し、triple は 4 つだけに出すことが分かる記述に直す。述語を改名・分割したら、[決定性・実行時監査仕様](determinism-and-audit.md) の逸脱一覧の該当行（`src/builtin_core.rs::is_host_call_builtin` を triple の述語として挙げる）と [設計](design.md) の構成図の関数名を、実装の名前に合わせる（行の内容は変えない）。
  - done にする実装 PR で、本書の逸脱一覧の filesystem builtin の行を削除する。
- **範囲外:**
  - 登録 host function（同期・cooperative）の呼び出しの host-call 回数と byte の課金（本書の逸脱一覧の登録 host function の行、loop:drift #420 D-108。issue #364 の本文の補足）。課金の位置が `src/eval.rs` の `apply_host_call`・`start_cooperative_call` で、filesystem builtin の課金とは別の仕組みなので、[開発ループ手順書](loop-runbook.md) §14.1 の「触る仕組みは 1 つまで」に合わせて別のスライス `fix-host-function-call-charge`（todo）にする。
  - `env`・`now`・`exit` の host call の課金（本書の逸脱一覧の Context builtin の行）。tree の `eval_builtin` の Context の arm と VM の Context builtin の arm で、filesystem builtin の PureCore の課金とは別の位置なので、このスライスでは触らない。
  - path 引数の byte を request bytes に数えるかどうか（Q1 の B。判断役が選ばなかった）。今の 4 つと揃えて数えない。
  - `list_dir` の結果を作り切ってから response bytes を課金する順序（有限の上限 N で N+1 byte を溜める前に止める規定。[Capability Model仕様](capability-model.md) の逸脱一覧の byte 上限 N の行と同じ種類）。今の `read_file` / `read_lines` と同じ順のまま変えない。
  - fuel・heap の課金、capability 判定・パスの形式検査の順序、filesystem builtin の結果の値とエラー。
- **課金の変化:** 10 個の filesystem builtin の 1 回の呼び出しごとに `host_calls` が 1 増え、`list_dir` の成功では `host_response_bytes`・`host_call_bytes` が名前の byte 長の合計だけ増える。`TSUMUGI_MAX_HOST_CALLS`（`max_host_calls`）・`TSUMUGI_MAX_HOST_RESPONSE_BYTES`・`TSUMUGI_MAX_HOST_CALL_BYTES` の境界がこの 10 個の呼び出しを含めて数えられ、上限を超えると今の 4 つと同じ catch 不能の予算超過（「host call の呼び出し回数が上限を超えました」など）で止まる。tree の `BudgetCharged` 監査 event にこれらの課金が現れる（triple は出ない）。fuel・heap は変わらない。I/O の会計は `fuel_accounting_revision`・`heap_accounting_revision` の対象でないので、どちらの版も変えない。`language_revision` も変えない。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜34 は done を含めてすべて使われていた。`tests/slices_contract.rs` が `order` の重複を弾くので、先例（PR #368・#381・#388 の Q1=A、PR #403・#408・#410・#412・#416・#419・#423）と同じく、設計 PR では仮にすべてのスライスより後（106）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR で 35 より前へ移す。106 は仮の値である。範囲外にした `fix-host-function-call-charge`（todo）は 107 に置いた。
- **想定の変更規模:** `src/` は 20〜60 行程度（`src/builtin_core.rs` の課金の述語と `host_call_response_bytes` の `list_dir`、`src/builtin.rs` の課金と triple の述語の分離、`src/vm.rs` の課金の述語、doc とコメント）。テストは `tests/integration.rs`（または新しい `tests/fs_builtin_host_call_charge.rs`）に 150〜300 行。触る仕組みは「filesystem builtin の host-call 課金」1 つ。受入基準は 5 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **FHC-AT-01（issue #364 の再現が上限で止まる）:** `--fs-root d=./d --fs-op d=metadata,read` で `path_exists("@d/x")` を 20,000 回呼ぶ loop（issue #364 の script）は、tree・VM とも 10,001 回目の呼び出しで「host call の呼び出し回数が上限を超えました」で止まり、`done` を出力しない。同じ呼び出しを `try` / `catch` で囲んでも catch されない（今の `read_file` と同じ）。確かめ方: subprocess の統合テスト（`host_calls_limit_gates_filesystem_calls_in_both_engines` と同じ形）。上限は `TSUMUGI_MAX_HOST_CALLS` で小さくしてよい（N 回目は成功し N+1 回目で止まることを確かめる）。
  - **FHC-AT-02（10 個すべてが 1 回ずつ数えられる）:** `path_exists` / `is_file` / `is_dir` / `file_size` / `list_dir` / `mkdir` / `remove` / `remove_dir` / `remove_tree` / `rename` のそれぞれ 1 回の呼び出しで、実行後の `BudgetUsage` の `host_calls` が 1 増える。成功・capability 拒否（grant 無し・操作の許可無し）・host error（存在しない path の `file_size` など）・パスの形式エラーのどれでも 1 増える。tree と VM で一致する。確かめ方: 埋め込み API の usage（`committed.host_calls`）を照合する統合テスト。
  - **FHC-AT-03（byte の数え方）:** Q1=A（判断役が選択（PR #425））のとおりに request/response bytes が課金される。`list_dir` の成功で `host_response_bytes` と `host_call_bytes` が返した名前の UTF-8 byte 長の合計だけ増え、拒否・error では 0 で、`list_dir` の request bytes は 0。他の 9 個は request・response とも 0（`host_request_bytes`・`host_response_bytes`・`host_call_bytes` が増えない）。`TSUMUGI_MAX_HOST_RESPONSE_BYTES` を名前の合計 − 1 にすると `list_dir` が「host call response の総バイト数が上限を超えました」で止まる（ちょうどなら成功する）。tree と VM で一致する。確かめ方: 統合テスト（usage の照合と subprocess の境界）。
  - **FHC-AT-04（課金は外部効果の前）:** `TSUMUGI_MAX_HOST_CALLS=N` で、N 回の host call のあとの `mkdir` / `rename` / `remove` は directory の作成・名前の変更・削除をせずに止まる（呼び出しの後で filesystem を確かめる）。tree と VM で一致する。確かめ方: subprocess の統合テスト。
  - **FHC-AT-05（監査と既存の 4 つは変わらない）:** 監査付きの実行で、10 個の filesystem builtin は host-call triple（`HostCallStarted`・`CapabilityDecision`・`HostCallFinished`）を出さず、`read_file` / `read_lines` / `write_file` / `append_file` の triple・request/response bytes・`BudgetCharged` の順序は変わらない。既存の `tests/integration.rs` の `host_calls_limit_gates_filesystem_calls_in_both_engines`、`tests/audit_hostcall.rs`、`tests/fs_write_content_budgeted_render.rs`、`tests/differential.rs` の host call の照合が変更なしで通る（10 個を含む script の usage の期待値を持つテストがあれば、上の課金の変化の分だけ直し、PR 本文に挙げる）。確かめ方: 既存のテストと、10 個を監査付きで呼んで triple が 0 件であることを確かめる統合テスト。

### fix-handle-execution-rollback 実装スライス（scheduler の handle 経路の失敗の terminal で language-state を execution 開始時点へ rollback する）

[issue #431](https://github.com/kamonabe1927/Tsumugi/issues/431) から作ったスライス（不具合の修正、`slices.toml` の `fix-handle-execution-rollback`）。issue の仕分けで「受け入れ」とした。issue #431 は [issue #427](https://github.com/kamonabe1927/Tsumugi/issues/427)（実装 PR #426 の審査の疑いで fix-handle-execution-cancel-token を `blocked` にした）の進め方を A（先に handle の rollback を直す fix スライスを設計し、その done を fix-handle-execution-cancel-token の `depends_on` にしてから `blocked` を解く）に決めたものである。直し方は §9.1（「非terminal handleのdropは … language-stateをrollbackし」）、§9.2（`Terminal` は「context commit/rollback後の状態」だけを公開する）、§10（AUD-024。未捕捉`RuntimeError`・budget/deadline・cancel・internal failure は execution 開始時点まで全 language-state を rollback し、`Completed` と `Exited` だけが commit する。yield と pause は transaction 境界ではない）で決まり、目標の章は変えない。

- **現状の不具合:** 冒頭の逸脱一覧の「handle 経路の失敗の terminal が language-state を rollback しない」行のとおり。`Engine::new_handle`（`src/engine.rs`）は handle を `transactional: false` で作り、`true` にするのは `execute_repl_submission` だけである。`begin_execution`（`src/eval.rs`）は `transactional` のときだけ submission journal（`Env::begin_submission`）を張り、`finalize_session` は `session.transactional` のときだけ `rollback_submission` を呼ぶ。そのため `create_execution`・`start`・同期の `execute` の handle が未捕捉の RuntimeError（予算超過を含む）・`Cancelled`・`DeadlineExceeded` の terminal に達したときも、非 terminal の handle を drop したとき（`Drop` → `abort_session` → `finalize_session(false)`）も、途中まで実行した binding の追加・代入・List/Dict の mutation・captured cell の書き換えが context に残る（import の解決マーカーは今も `forget` で巻き戻す）。例: 同じ context で `let a = 1` を `execute` したあと、`a = 10\nlet b = 2\nnope` を `execute` すると `Err`（未定義の `nope`）が返るが、context の `a` は 10、`b` は 2 のまま残る。cancel・drop のあとは context の token が cancel 済みのまま残る（逸脱一覧の cancellation token の行）ので今は次の execution から途中の状態は読めないが、fix-handle-execution-cancel-token が token を execution ごとにすると読めるようになる（#426 の審査の疑い 1）。`tests/engine_api.rs` の `nonterminal_drop_cancels_and_rolls_back_without_blocking` は名前に反して token の cancel しか確かめていない。
- **実装範囲:**
  - `create_execution`・`start`（`new_handle`）が作るすべての handle で、初回 poll の `begin_execution` に `transactional = true` を渡し、execution 開始時点で submission journal を張る。これで §10 の commit / rollback が handle 経路のすべての terminal に効く: `Completed` と `exit` の `ProcessExit`（今の `run_slice` の `is_exit`）は commit、それ以外の terminal（未捕捉の RuntimeError・予算超過・`Cancelled`・`DeadlineExceeded`・poll index の枯渇など）と非 terminal の drop は、今ある `finalize_session(false)`・`abort_session` の経路で rollback する。journal は yield・pause・host 待ちを跨いで保持する（今の `run_slice` の yield 分岐のとおり。§10 規則 7）。
  - terminal の outcome の分類は変えない。今の `transactional` は「rollback する」と「Link の失敗を `RuntimeError` にする（REPL の約束）」の 2 つを兼ねているので、後者を `execute_repl_submission` だけに残す（例: handle の field を 2 つに分ける。形と名前は実装に任せる）。`create_execution`・`start`・`execute` の Link の失敗は今と同じく `LinkError`（`begin_execution` が `RunPhase::Run` で返す深度の失敗は `RuntimeError`）、`execute_repl_submission` は今と同じく `RuntimeError` にする。Link の失敗では今と同じく journal を破棄または rollback し、language-state を変えない（§10 規則 5）。
  - 同期の `execute` の戻り値の約束（`Ok(Completed)` / `Err(TsumugiError)`）と `drive_to_outcome` の写し方は変えない。`Err` を返した execution の language-state が execution 開始時点へ戻ることだけが変わる。`execute` の doc に、失敗したときは context の language-state を開始時点へ戻す（§10）ことを書く。
  - `new_handle`・`ExecutionHandle` の `transactional` field・`Drop`・`execute`・`execute_repl_submission` の doc とコメント、`RunSession.transactional` の doc（「REPL transaction 経路か」）を、handle 経路のすべての execution が transaction であることが分かる記述に直す。
  - done にする実装 PR で、本書の逸脱一覧の「handle 経路の失敗の terminal が language-state を rollback しない」行を削除する。
- **範囲外:**
  - cancellation token を execution ごとに作ること（fix-handle-execution-cancel-token、issue #363）。このスライスのあとに入れ、その `depends_on` にこのスライスを置く（issue #431）。このスライスの間は、cancel・drop のあと同じ context の次の execution が今と同じく `Cancelled` で止まる（途中の状態は rollback 済みで、漏れない）。
  - facade（`src/embedding.rs` の `Engine::run`・`run_audited`）の経路。`begin_execution_for` が `transactional = true` で始めており、この不具合は無い。今のまま変えない。
  - REPL（`execute_repl_submission`、`src/main.rs` の REPL）。今も transaction で、outcome の分類も変えない。`execute` と `execute_repl_submission` の観測できる違い（`drive_to_outcome` はどちらの失敗も `Err` へ写す）を整理して入口を 1 つにすることは公開 API の変更なので、このスライスでは触らない。
  - drop の監査（orphan-audit queue・`AuditUnavailable`・`context_committed`。§9.1・[決定性・実行時監査仕様](determinism-and-audit.md)）。scheduler の `Engine` は監査 sink を持たない（同書の逸脱一覧の B-1）。
  - admission slot・run-turn・同期入口の `ConcurrentRunRequiresPolling`（fix-terminal-release-slot・fix-pause-run-turn-remove・fix-sync-run-concurrent-handle）。同じ handle に触れるが別の仕組みで、[開発ループ手順書](loop-runbook.md) §14.1 に合わせて別のスライスにする（このスライスは handle 経路の transaction の張り方 1 つ）。
  - context 全体の deep clone や journal の形の変更。今の submission journal（`src/env.rs` の `SubmissionJournal`）をそのまま使う（§10「context全体のdeep cloneを禁止する」）。
  - VM。scheduler の handle 経路は tree evaluator だけで実行し、VM はこの経路に無い（未実装範囲の「VM の式レベル continuation」）。VM の file/stdin・REPL の経路は変えない。
- **課金の変化:** handle 経路の execution で、§5.1 の rollback journal entry（1 件 48 byte。同じ binding・cell の 2 回目以降の変更は積まない）が live heap へ課金され、terminal（commit・rollback）で journal とともに解放される。§10 の「journal自身と旧valueの保持はheap budgetへ課金する」を、facade・REPL で今も課金している同じ規則のまま handle 経路にも当てはめるもので、規則は変えないので `heap_accounting_revision` は変えない。`max_live_heap_bytes` の境界に近い script は、handle 経路でも facade・REPL と同じく journal の分だけ早く heap の予算超過になりうる（`PollResult` の `usage` の live heap が journal の分だけ増える）。fuel・I/O は変わらない。`fuel_accounting_revision`・`language_revision` は変えない。
- **着手順（`order`）:** [開発ループ手順書](loop-runbook.md) §8 の「不具合の報告」の既定は「今ある `ready` より前」だが、この設計の時点で `ready` の最小は 35（fix-heap-charge-audit-notify）で、0〜34 は done を含めてすべて使われていた。`tests/slices_contract.rs` が `order` の重複を弾くので、先例（PR #368・#381・#388 の Q1=A、PR #403・#408・#410・#412・#416・#419・#423・#425）と同じく、設計 PR では仮にすべてのスライスより後（108）に置き、マージ後に PM が `order` の付け直しだけの状態整理 PR で 35 より前へ移す。108 は仮の値である。fix-handle-execution-cancel-token（`order` 105、`todo`）は `depends_on` でこのスライスの後に固定される。
- **想定の変更規模:** `src/` は 15〜50 行程度（`src/engine.rs` の `new_handle`・`drive_one_slice` の flag の分離と doc・`Drop` のコメント、`src/eval.rs` の `RunSession` の doc）。テストは `tests/engine_api.rs` と、cancel・drop のあとの binding を直接読む crate 内の単体テスト（`src/engine.rs` の `#[cfg(test)]`。`ExecutionContext` の評価器の `env` は crate 内から読める）に合わせて 200〜350 行。触る仕組みは「scheduler の handle 経路の transaction の張り方」1 つ。受入基準は 5 項目。[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **受入基準:**
  - **HER-AT-01（同期 `execute` の未捕捉 error は rollback する）:** 同じ context で `let a = 1\nlet xs = [1]` を `engine.execute` で `Ok(Completed)` にしたあと、`a = 10\nlet b = 2\npush(xs, 2)\nnope` の `execute` は今と同じく `Err`（未定義の `nope`）を返し、その後の同じ context の `execute` で `a == 1`・`len(xs) == 1` が成り立ち、`b` の参照は未定義の `Err` になる。予算超過（fuel など。単体テストで context の budget を小さくする）で止まった execution も同じく rollback する。確かめ方: 統合テスト（`tests/engine_api.rs`）。値は後続の `execute` で確かめる（例: 条件が偽なら未定義の名前を参照して `Err` にする script）か、単体テストで評価器の binding を読む。
  - **HER-AT-02（yield を跨いだ handle の失敗の terminal は rollback する）:** `create_execution` と `start` の handle で、`let a = 1` を commit 済みの context に対し、`a = 10\nlet b = 2\nlet i = 0\nwhile i < 100\n  i = i + 1\nend\nnope` を `PollSlice { max_fuel: 16 }` で 2 回以上 `Yielded(SliceFuelExhausted)` を挟んで poll すると、`RuntimeError` の terminal のあと同じ context で `a == 1` が成り立ち `b`・`i` は未定義になる。同じ script の途中で token を cancel して `Cancelled` の terminal にした場合、注入 clock の deadline で `DeadlineExceeded` の terminal にした場合も、context の `a` は 1、`b`・`i` は未定義である。確かめ方: 統合テスト（`RuntimeError`）と、cancel・deadline のあとの binding を読む単体テスト（このスライスの間は token が cancel 済みのまま残るため）。
  - **HER-AT-03（非 terminal の drop は rollback する）:** HER-AT-02 と同じ script の handle を `Yielded(SliceFuelExhausted)`・`Paused`・`Yielded(HostCallPending)`（cooperative host function を呼ぶ script）のそれぞれで drop すると、drop 前に取った token の `is_cancelled()` が `true` になり（今と同じ）、context の `a` は 1、`b`・`i` は未定義である。`Created`・`Linked`・`Yielded(AdmissionQueued)` で drop した場合は context の language-state（binding）を変えない（token は今と同じく cancel される）。確かめ方: 単体テスト（評価器の binding を読む）。`nonterminal_drop_cancels_and_rolls_back_without_blocking` は名前どおり rollback も確かめる形にしてよい（単体テストへの参照をコメントに書くだけでもよい）。
  - **HER-AT-04（成功の terminal は commit し、outcome の分類は変わらない）:** handle 経路の `Completed`、`try` で catch したあとの `Completed`、`exit` の `ProcessExit` は、途中の binding・mutation を context に残す（§10 規則 1・2・5）。`create_execution` の handle で存在しない module を import する script は今と同じく `LinkError` の terminal になり、context を変えず、同じ module の解決マーカーを残さない。同じ script の `execute_repl_submission` は今と同じく `Err` を返す（内部の outcome は `RuntimeError`）。確かめ方: 統合テスト。
  - **HER-AT-05（既存の経路は変わらない）:** `tests/engine_api.rs`・`tests/scheduler_api.rs`・`tests/sync_run_host_call.rs`・`tests/audit_core.rs`・`tests/integration.rs`・`tests/differential.rs` の既存テストと REPL のテスト（`tests/cli_*`）が変更なしで通る。handle 経路の usage の live heap を完全一致で見るテストがあれば、上の課金の変化（journal entry）の分だけ期待値を直し、PR 本文に列挙する。確かめ方: 既存のテストと `cargo test`。

### legacy env の入口の実装記録（旧「§13 実装注記（2026-10-20）」）

この小節は、§13（目標の章）にあった実装注記を loop:drift #373 の D-73 (5) で移したものである（目標の章に実装状態・経緯を置かない。[ドキュメント運用ガイド](documentation-guide.md)「逸脱一覧の持ち方」）。2026-10-20 時点の記録で、現在の状態は冒頭の逸脱一覧を見る。

> legacy envの入口はCLI adapter（`src/main.rs`の`LegacyBudgetEnv`）だけに置いた。起動時に1回だけ
> process envを読み、tree/VMのfile・stdin・REPLの全経路で同じ上書きを適用する（tree/VMは
> `BudgetConfig::standard`、clockを持たないVM実行は`for_legacy(1_000_000, 1_000_000)`を土台にする）。
> 上書きが1つでも効くと、stderrへ1回
> 「警告: 環境変数 KEY[, KEY...] による実行予算の上書きは非推奨であり、将来のバージョンで削除されます」を出す。
> libraryは env を読まない: `BudgetConfig::from_legacy_env`を削除し、env非依存の
> `BudgetConfig::apply_legacy_overrides(lookup)`と`LEGACY_OVERRIDE_KEYS`へ置き換えた。
> `Evaluator::new` / `Vm::new` / `Vm::new_repl`（alpha facadeの`ExecutionContext::new`を含む）は
> §3.1既定の上限を使う。VMへ明示configを渡す`Vm::with_budget` / `Vm::new_repl_with_budget`を追加した。
> これにより、option B（最終形移行Slice 1）以降に残っていた「tree file/stdinだけがenvを無視する」
> tree/VM差を解消した。削除予定versionは、最初のreleaseを切る時点で決める（未決）。executionごとに新しい`BudgetUsage`を作る。REPLの各入力は1 executionとし、contextだけを引き継ぐ。

移すときに、旧注記の「`ExecutionContext::reset_step_budget()`は削除し」を事実に合わせて外した（loop:drift #373 の D-71）。2026-10-20 に削除されていたのは facade の `embedding::ExecutionContext` の側だけで、scheduler 側の `ExecutionContext`（`src/engine.rs`、crate root から公開）には fuel だけを戻す `reset_step_budget()` が残っている。§13 の目標（execution ごとに新しい `BudgetUsage`）に合わせ、残っているメソッドを削除する fix スライス fix-scheduler-context-reset-step-budget を足し、それまでの差は逸脱一覧に載せた。判断役が選択（issue #373 の D-71、Q1=A）。理由: 呼び出し元が無く、同じ ledger の fuel だけを手で戻す公開 API が残ると「どの予算で実行したか」の説明が 2 通りになる。alpha では組み込み API の後方互換を目標にしない（マニフェスト「目指さないもの」）ので、削除で守る相手もいない。採らなかった案 B（メソッドを残し逸脱として載せるだけ）は、§13 の目標を変えない限り解消のスライスがいずれ要り、目標と食い違う公開 API を残す期間が延びるだけになる。

### §1 にあった実装状況の記録（2026-11-01 時点）

この小節は、§1（目標の章）の段落にあった各スライスの実装状況を loop:drift #373 の D-73 (1) で移したものである（目標の章に実装状態を置かない。[ドキュメント運用ガイド](documentation-guide.md)「逸脱一覧の持ち方」）。2026-11-01 時点の記録で、現在の状態は冒頭の逸脱一覧と [`slices.toml`](slices.toml) を見る。

> 第14節 Slice 1（budget型・legacy adapter・共有BudgetLedger）は `src/budget.rs` に実装済みで、Slice 2 のうち string accounting サブスライス（per-item `SingleStringBytes`・cumulative `StringAllocations`/`StringBytes` の課金を共有 builtin handler へ tree/VM 共通で配線し、さらに string リテラル・`+` 連結・f-string の生成点でも `track_result` で課金）と source / import accounting サブスライス（per-item `SingleSourceBytes`・cumulative `SourceCount`/`SourceBytes`・`ImportCount`/`ImportBytes` を Link フェーズで root と import へ tree/VM 共通で課金）、heap accounting 基盤サブスライス（`AllocationId`・`AllocationLedger`・§5.1 論理サイズ関数・`HeapBytes` reserve/超過写像）、collection（`List`/`Dict`）の per-drop release サブスライス（`Rc<Tracked<T>>` で生成時に課金し最後の参照 drop で release、COW は delta 課金、tree/VM 両対応）、および String body の per-drop release サブスライス（PR-b、`Value::Str` を `Rc<Tracked<String>>` 化し、builtin 結果を dispatch 境界の `track_result` で live heap 課金・最後の参照 drop で release）、cell と tree/VM 関数 instance の per-drop release サブスライス（PR-c、cell 生成点で captured cell を課金し `Value::Fn`/`VmFn` の header token で function instance header を課金、drop で release）、および AST / bytecode chunk / imported module record / rollback journal の per-drop 追跡サブスライス（PR-d、所有構造側が `HeapToken = Tracked<()>` トークンで §5.1 論理サイズを課金し drop で release。§5.3「AST または bytecode」に従い tree は AST・VM は bytecode chunk を課金、import record は両 engine、rollback journal entry は両 engine で entry の固定 overhead を課金）、および string リテラル/連結/f-string 経路の課金サブスライス（生成点で `track_result` を通し cumulative + live heap を課金、tree/VM 両対応）、および I-O accounting サブスライス（input / output / host call の count + bytes を reserve/commit/refund（§7）で課金し、stdio を host call として co-charge。tree/VM の `print` / `input` / filesystem dispatch 境界へ共通配線）も実装済みで、Slice 2 は完了した。Slice 3 は PR-a（公開 state-machine surface + poll-to-terminal core）と PR-b（tree evaluator の statement / block / loop を明示 frame stack + driver ループへ変換し continuation_frame heap を配線）、PR-c（関数呼び出し・try handler の明示 frame。`eval_call` / `call_fn_value` を `FrameKind::Call` 化し、共通 `run_driver` + `drive_call_body` で本体を駆動。VM の `CallFrame` / `TryHandler` をミラー）、PR-d（slice fuel + yield。PR-d-1 で continuation を Evaluator 所有の `'static` 永続 frame stack へ移し、PR-d-2 で slice-fuel accounting と文/反復境界での `Yielded(SliceFuelExhausted)`・`begin_execution` / `run_slice` resume を実装）まで実装済み。PR-e（式の途中での slice yield。§14「tree-expr-continuation 実装スライス」）の現在の実装範囲は冒頭の逸脱一覧を見ること。cancel/pause（Slice 4）と scheduler・host pending（Slice 5）は実装済みである（Slice 5 は `SchedulerShared`/`AdmissionController` の bounded admission・run-turn FIFO backpressure、nonblocking host-call ticket〔`HostCallTicket`/`ExecutionWaker`/`HostCallPoll`〕と wake・queue 待ち/pause/host 待ち中の deadline/cancel 伝播、独立上限の `AdapterExecutor` + `CooperativeAdapter`/`CapabilityCallContext.may_yield`。`AuditBackpressure`-yield（sink-Pending 由来）は Phase 6 B-1 で facade の監査付き poll driver に配線済み〔§10.2、[決定性・実行時監査仕様](determinism-and-audit.md) §14〕。audit drain queue byte 会計〔§10.2〕は B-2 で per-execution 上限を同 driver に配線済み、engine 全体の上限は E10 統合後）。VM charge parity（Slice 6）も heap・string・I-O の会計について完了済み（fuel の課金差は第14節「F-1 実装スライス」で扱う）で、最終形移行のうち `BudgetConfig` 必須所有（Slice 1: `ExecutionRequest::new(budget, clock)`・deadline を `BudgetConfig.deadline` へ統合・`EngineConfig.budget` collapse・CLI 既定の有限化＝option B）は実装済みである。最終形の `CancellationToken` 必須所有（Slice 2: `ExecutionRequest` が `cancellation: CancellationToken` を必須所有し `.cancellation(value)` builder で設定、`Engine::run` が実行直前に request-owned token を評価器へ install）も実装済みで、REV-015 最終形移行（budget + cancellation 必須所有）は完了した。残りは Phase 6（audit）である。

### §5.3 にあった設計レビュー対応メモ（REV-015 Slice 6 PR-6b）

この小節は、§5.3（目標の章）の「closure が retain する code の課金」の末尾にあった設計レビュー対応メモを loop:drift #373 の D-73 (4) で移したものである（目標の章に経緯を置かない。[ドキュメント運用ガイド](documentation-guide.md)「逸脱一覧の持ち方」）。PR-6b の設計レビュー 2 巡目の時点の記録で、src の行番号はその時点のものである。メモの中の「上記」と「」で挙げた項目名は §5.3「closure が retain する code の課金」の各項を指す。現在の状態は冒頭の逸脱一覧を見る。

> **設計レビュー対応メモ（REV-015 Slice 6 PR-6b, 2 巡目）**: 本 A-1 設計はレビュー指摘を次の
> とおり反映した。
> - *[HIGH] VM の `FunctionPrototype.retain_token` slot 方式が実装不能*: `Chunk` は `Rc<Chunk>`
>   共有・`charge_chunk_tree_into` は `&Chunk` 走査で interior mutability なしに slot へ書けない
>   （`src/vm.rs:299-317`, `:363-388`, `src/chunk.rs:53`）。→ **slot 方式を却下し、VM も tree と
>   対称な `Rc::as_ptr` key の side-table 方式へ統一**した（上記「token と code object の対応付け」）。
> - *[MEDIUM] side-table 寿命と跨入力 ABA*: closure 生成は adopt/Link 入力と別入力になりうる
>   （VM の `MakeClosure` は複数 `run_repl_chunk` を跨ぐ、tree の内側 closure も外側 call 入力で
>   初生成）。→ **side-table を `Weak` の長命 index にし、生成時に subtree 全体の token を一括
>   `Rc::clone` 移譲**する不変量を明記（「side-table は `Weak` の index」「跨入力生成の不変量」
>   「生成時はトークンを移譲する（subtree 一括）」）。ABA は upgrade 可否が token の生死で判定
>   されるため実害なしと固定。
> - *[MEDIUM] tree の per-body 分割が `ast_heap_size` の flat worklist では成立しない*: 現行走査は
>   body を個々の `Stmt` へ平坦展開し per-body 小計を保持しない（`src/ast.rs:548-549`, `:585-586`）。
>   → **「走査を body 単位の小計へ再構成する必須作業」である旨を明記**し「再利用」表現を撤回
>   （「全ネスト深度を eager に列挙」tree 項）。
> - *[NIT] body の `Rc` 共有有無*: → tree の body は原則 distinct `Rc`、共有されても backing
>   同一性で 1 token に畳まれ §5.2 と整合、の一文を追加（「ネスト closure の transitive 規則」tree 項）。

### §5.3 にあった closure の retained code の課金の経緯（REV-015 Slice 6）

この小節は、§5.3（目標の章）の「closure が retain する code の課金」の本文にあった経緯（従来の under-charge、却下した案、当時の実装の説明と src の行番号の参照）を loop:drift #373 の D-73 (4) で移したものである（目標の章に経緯を置かない。[ドキュメント運用ガイド](documentation-guide.md)「逸脱一覧の持ち方」）。§5.3 には目標の規則だけを残した。REV-015 Slice 6 PR-6b の設計の時点の記録で、src の行番号はその時点のものである。現在の状態は冒頭の逸脱一覧を見る。移す前の本文は main ff630a0 の §5.3 にある。

- **閉じた under-charge**: A-1 の前は、closure が retain する code が次入力の artifact release で課金から外れていた（両 engine 対称の under-charge）。A-1 モデルはこれを閉じるために入れた。
- **却下した案（VM の `retain_token` slot 方式）**:

  > 当初案（VM だけ `FunctionPrototype` に `retain_token` slot を足す）は却下する。理由は
  > 次のとおり現行コードでは実装不能だからである: `FunctionPrototype` は `Rc<Chunk>` の
  > 中に共有保持され（`src/chunk.rs:53`、`CallFrame.chunk: Rc<Chunk>` `src/vm.rs:28`）、
  > `charge_chunk_tree_into` は `&Chunk` 共有参照で走査する（`src/vm.rs:299-317`）。
  > interior mutability なしに prototype 内の slot へは書き込めず、REPL 経路では adopt 前に
  > 既に `Rc` の strong ref が複数本あり `Rc::get_mut` も不可（`src/vm.rs:363-388`）。
  > したがって slot 書き込み方式は成立しない。両 engine を次の side-table 方式に統一する。

- **tree の走査を改造する必要があった理由**:

  > tree は VM と違い **既存の `ast_heap_size`（`src/ast.rs:489`）の走査を
  > そのまま再利用できない**。現行 `ast_heap_size` は 1 本の flat worklist で、`FnDef` /
  > `Lambda` に当たると body を個々の `Stmt` へ平坦展開して push し
  > （`src/ast.rs:548-549`, `:585-586` 近傍）、どの `Stmt` がどの body slice に属するか・
  > その body の論理サイズ小計を保持しない。（中略）「`ast_heap_size` をそのまま再利用」は
  > per-body 小計を保持しない以上成立しないため、この走査構造の改造は本設計の必須作業である
  > （「再利用」という語で過小評価しない）。

- **当時の token の持ち方**: tree の `ast_token` は `ast_heap_size(linked_program)` を 1 本の lump token で課金していた。§5.3 の per-code-object 分割はこの lump token からの差し引きではなく、Link 時の分割で境界を作る。
- **外した src の参照**: `FunctionPrototype.chunk`（`src/chunk.rs:22`）、`Chunk` の `PartialEq`/`Clone` 派生（`src/chunk.rs:11,41`）、`Block = Rc<[Stmt]>`（`src/ast.rs:14`）、closure 生成点（`src/eval.rs` の `Stmt::FnDef` / `Expr::Lambda` 評価）、`MakeClosure`（`src/vm.rs:1273` 近傍）、`ExecutionPhase::Compile`（`src/vm.rs:310` 近傍）、`chunk_tokens.clear()`（`src/vm.rs:383` 近傍）、`Tracked::Drop`（`src/value.rs:180-186`）。
- **外した語**: 跨入力生成の不変量の見出しの「finding 2」（PR-6b の設計レビューの指摘番号）、実現機構の見出しの「設計として確定・実装者が選ぶ余地を残さない」、関数 header の token を指す「PR-c の」、engine 共通の単一基準へ寄せる却下案の名前「A-2」（2 か所。「A-2 のような」「A-2 のように」。A-2 の定義は §14 Slice 6 にある）。
- **直前の小節のメモとの対応**: 直前の小節「§5.3 にあった設計レビュー対応メモ」の [MEDIUM] の 3 項目めが撤回したと書く「再利用」の表現と、「走査を body 単位の小計へ再構成する必須作業」の文は、§5.3 から外し、この小節の「tree の走査を改造する必要があった理由」の引用へ移した（§5.3 には走査の規則だけが残る）。

### fix-tree-while-condition-once 実装スライス（tree の `while` が初回の条件を 2 回評価しない）

[issue #400](https://github.com/kamonabe1927/Tsumugi/issues/400)（kamonabe、[issue #393](https://github.com/kamonabe1927/Tsumugi/issues/393) の不具合の取り込み。issue の仕分けで「受け入れ」）から作ったスライス（`slices.toml` の `fix-tree-while-condition-once`）。issue は原因の調査を設計に含めるとしたので、この設計 PR で調べた。目標の規範は[言語仕様](language-spec.md)「while」（この設計 PR で条件の評価回数を書き足した。判断役が選択（PR #449）、Q1=A）と「エラーの kind とメッセージ（正本）」で、現在の差は同書の逸脱一覧の行にある。

- **決定記録（判断役が選択（PR #449）、Q1=A）:** `while` の条件は各イテレーションの前に 1 回だけ評価する（初回も同じ）と決め、tree を VM に揃える（[言語仕様](language-spec.md)「while」に書き足した）。理由: A・B ともマニフェストに反しないが、A は条件の副作用と課金の回数が反復の回数だけで決まり（n 回実行なら n + 1 回、`break` なら n 回）、利用者が予測できる（原則 5 の説明可能性）。[決定性・実行時監査仕様](determinism-and-audit.md) §2.1 は tree を規範 backend としつつ tree の bug を無条件に仕様へ昇格しないと定めており、初回だけの 2 回評価は driver の作りから生じたもので意図した意味ではない。`if`（条件 1 回）・`for`（反復元 1 回）とも揃い、§14 F-1 の (F2)「呼び出し 1 回につき 1」の定義を変えずに二重課金が消える。変更は tree の `while` の driver の初回 1 か所で、VM・課金点・`fuel_accounting_revision` は変えない（原則 8）。「反復ごとに 1 回」は TWC-AT-01..04 の paired test でそのまま確かめられる（原則 9）。採らなかった案: B（tree の初回 2 回評価を規範にして VM を合わせる）は、条件の副作用と課金が初回だけ重複する説明しにくい意味を言語仕様に固定し、§2.1 の「tree の bug を仕様へ昇格しない」とも合わず、正しく動いている VM の lowering に重複した評価を足すことになる。issue #400 は「tree が規範なので VM 側を tree に揃える」と書くが、同じ issue が原因の調査を設計に含めるとしており、調べた結果、原因は tree 側だった。issue の期待する挙動（終了理由の kind・message・line が tree と VM で完全に一致する）は A で満たす。
- **原因（main e9e7aa8 で確かめた）:** issue #393 の行番号の差は、fuel の課金点と行の対応のずれではなく、tree の `while` が、初回の条件が真のとき条件を 2 回評価することによる。`Stmt::While`（`src/eval.rs` の `exec_stmt`）と、条件が式 frame を要するときの `CallDest::WhileFirst`（同 `exec_driver_slot_stmt` から積む根の式 frame の受け渡し先）は、どちらも条件を 1 回評価して真なら `enter_while` で While controller frame（空の stmts、`pending_step: false`）を積むだけで、本体は積まない。driver はその frame をすぐ `advance_frame` の `LoopAction::While` へ渡し、そこで条件をもう一度評価してから最初の反復の本体を積む。そのため条件の副作用が初回だけ 2 回起きる。
  - 出力: `fn t(x)` が `print("c")` して `x` を返すとき、`while t(n < 1)` で本体が `n = n + 1` のプログラムは、tree が `c` を 3 回、`--vm` 版が 2 回出す。`while t(true)` の本体が `break` だけなら tree 2 回・VM 1 回。builtin の副作用（`while print("c") == null and n < 1`）でも同じ（tree 3 回・VM 2 回）。どれも CLI（既定の safe profile）で確かめた。
  - fuel: 条件にユーザー関数の呼び出しがあると (F2) `PrepareCall` の課金が初回だけ 1 つ多い。`fn f(p) return p end` のもとで `while f(n < 1)` が 1 回回るのに要る最小の fuel は tree 4・VM 3、2 回なら tree 6・VM 5（条件に呼び出しの無い `while n < 1` は両方 1、`while f(false)` は両方 1）。課金の順序が 1 つずれるので、ステップ上限で止まったときの行が fuel によって入れ替わる（issue #393 のプログラムでは、課金の 1 周期が「6 行目の `f(f)`・9 行目の `f(g)`・6 行目の back-edge」の 3 つで、fuel 3 の倍数のときだけ一致する）。外側の `try` と内側の `while` は関係せず、`while f(f)` を `while true` にすると条件の呼び出しが無くなるので一致する。
  - VM（`src/compiler.rs` の `Stmt::While` の lowering）は反復ごとに条件を 1 回だけ評価しており、変えない。
- **実装範囲:**
  - tree の `while` で、条件が真になった評価 1 回につき本体を 1 回積む。`Stmt::While` の直接の経路と `CallDest::WhileFirst` の経路の両方で、初回の条件が真なら While controller frame を積んだうえで、条件を評価し直さずに最初の反復の本体を積む（`pending_step` を立てて `begin_loop_iteration` を呼ぶのと同じ状態にする）。2 回目以降の反復（`LoopAction::While` と `CallDest::WhileIter`）は変えない。形（`enter_while` に最初の反復を積ませるか、controller に「初回の条件は評価済み」の印を持たせるか）は実装 PR で決めてよい。
  - fuel の課金点（(F1) back-edge・(F2) `PrepareCall`・(F3) callback、§14「F-1 実装スライス」）とその行は変えない。条件の評価が 1 回減ることで、条件の中の呼び出しの課金が初回だけ 1 つ減り、VM と一致する。
  - slice yield と resume（tree-expr-continuation-e2b の式 frame の経路）でも、条件の評価回数と fuel が同期の実行と同じになる。
  - done にする実装 PR で、`tests/random_input_robustness.rs` の `known_divergence_step_limit_line_after_caught_error` の `#[ignore]` を外し、`KNOWN_STEP_LIMIT_LINE_DIVERGENCES` から `program#143` を消す（中身が空になる定数と `without_step_limit_line` を残すか消すかは実装 PR で決めてよい）。[言語仕様](language-spec.md)の逸脱一覧から issue #393 の行を削除する。
- **範囲外:**
  - VM の `while`、`for`（反復元は 1 回だけ評価し、items を controller が持つ）、`if`（条件は 1 回）。どれもこの不具合を持たない。
  - F-2 の課金表への移行（式・操作の課金点）。現状の fuel は文・反復・呼び出しだけを数える（冒頭の未実装範囲）。
- **課金の変化:** 条件にユーザー関数の呼び出しがあり初回の条件が真になる `while` で、tree の fuel が初回の条件の呼び出しの課金ぶん（呼び出し 1 つにつき 1）減り、VM と同じになる。VM は変わらない。tree の fuel が変わるが、変更前の値は §14「F-1 実装スライス」の (F2) の課金点の定義（呼び出し 1 回につき 1。条件の評価 1 回につき呼び出しを 1 回課金）に反した二重課金なので、`fuel_accounting_revision` は変えない（実装 PR の「仕様外で判断した点」で扱う。課金点の定義は変わらない）。`language_revision` も変えない（目標の挙動は VM と同じで、tree の挙動を目標へ戻す修正）。
- **想定の変更規模:** `src/` は 15〜60 行程度（`src/eval.rs` の `Stmt::While`・`CallDest::WhileFirst`・`enter_while` まわり）。テストは `tests/` と unit test で 80〜200 行。docs は言語仕様の逸脱一覧の 1 行の削除。触る仕組みは「tree の `while` の driver（controller frame の初回）」1 つ。受入基準は 4 項目。§14.1（[開発ループ手順書](loop-runbook.md)）に収まる。
- **受入基準:**
  - **TWC-AT-01（条件は反復ごとに 1 回）:** 条件に副作用（`print` するユーザー関数、`print` の builtin）がある `while` で、本体が n 回実行されるとき条件の副作用がちょうど n + 1 回（`break` で抜けるときは n 回）起き、tree と VM で stdout が一致する。n = 0・1・2 と、初回の反復で `break`・`continue` する場合を含む。確かめ方: 両 backend の paired test（CLI と `Evaluator` / `Vm` の両方）。
  - **TWC-AT-02（fuel の一致）:** 条件にユーザー関数の呼び出しがある `while`（`while f(n < k)`、k = 1・2・3、入れ子の `while` と `try` の中を含む）で、tree と VM の `committed.fuel` が一致し、上限 N に対する成功・失敗の境界が一致する（`while f(n < 1)` は fuel 3 で成功、2 で `limit`）。確かめ方: 両 backend の paired test（semantic-decisions の「fuel parity」と同じ形）。
  - **TWC-AT-03（issue #393 の再現）:** issue #393 のプログラムを fuel 1〜30 と 20,000 で実行したとき、終了理由の kind・message・行が tree と VM で完全に一致する。`known_divergence_step_limit_line_after_caught_error` が `#[ignore]` なしで通り、`KNOWN_STEP_LIMIT_LINE_DIVERGENCES` に `program#143` が無い状態で `random_valid_programs_never_panic_and_engines_agree` が通る。確かめ方: `tests/random_input_robustness.rs` と、fuel を振る paired test。
  - **TWC-AT-04（yield をまたいでも同じ）:** 条件の呼び出しの途中・本体の途中で slice yield する実行（scheduler の handle 経路で小さい `max_fuel` の slice に分ける）で、条件の副作用の回数・stdout・`committed.fuel` が同期の実行と同じになる。確かめ方: 既存の tree-expr-continuation-e2b のテストと同じ形の test。

### fix-link-context-baseline-charge 実装スライス（facade の `Engine::run` で context に残る値を新 execution の baseline として課金する）

loop:drift #429 の D-137 を直す（親 REV-015、`slices.toml` の `fix-link-context-baseline-charge`）。§5.2 末尾は、`ExecutionContext` に以前から残る変数・closure・collection を
新 execution を `Linked` へ進めるときに baseline live heap として課金し、上限を超えれば 1 文も実行せず `BudgetExceeded(HeapBytes)` とする。§9.2 の `Linked` は
context baseline を課金済みの状態である。facade の `Engine::run` はこれを満たさない（逸脱一覧の D-137 の行）。挙動の直し方（baseline の課金の置き場所と
二重計上の回避）は、PR #451 の Q1=A で本スライスの設計に回され、判断役が選択（PR #499）の Q1=A・Q2=A で決めた（下の「決定記録」）。

- **前提（設計時点の main feca544 で確かめたこと）**:
  - facade の `Engine::run`・`run_audited`（`src/embedding.rs`）は `install_execution_state` で、REPL の 2 回目以降の submission を除き
    `Evaluator::reset_budget`（`src/eval.rs`）→ `BudgetLedger::reset_config_with_token`（`src/budget.rs`、`*self = Self::new(..)`）を呼び、heap 台帳
    （`Rc<RefCell<AllocationLedger>>`）ごと作り直す。そのあと `Env::set_heap_ledger` で新しい台帳の handle を配り直す。
  - tracked な値（`Tracked<T>` の List・Dict・String・cell、artifact の `HeapToken`）は台帳を `HeapLedgerWeak` で参照し、`Drop` でその台帳へ release する。
    台帳を作り直すと、context に残る値の `Weak` は古い台帳（drop 済み）を指し、新しい台帳には課金されず、drop しても新しい台帳から release されない。
  - `BudgetLedger::charge_context_baseline`（`src/budget.rs`）は root の `Value` から反復 worklist で走査して新しい台帳へ課金する部品だが、本番の経路から
    呼ぶ箇所は無い（unit test だけ）。これで新しい台帳へ課金しても、残った値の `Tracked` は古い台帳の `Weak` を持つので、execution の途中でその値を手放しても
    新しい台帳から release されず、live が過大のまま残る。走査は Value slot・body・function instance だけを数え、closure が retain する code の token（§5.3 A-1）・
    展開済み module の record（`graph_record_tokens`）・captured cell の課金とは一致しない。
  - 逸脱一覧の D-137 の行が挙げる `Evaluator::charge_link`・`Vm::charge_link` は、いまは `charge_link_graph`（`src/eval.rs`・`src/vm.rs`）で、どちらも
    source・import・module record（tree は続けて AST）を課金するだけで baseline は課金しない。facade の run は tree だけを使い（`Backend::VmExperimental` でも
    facade の run は tree の評価器を通る）、VM には facade の context を持ち越す経路が無い。VM REPL の持ち越しは fix-repl-submission-budget で済んでいる。
  - REPL の 2 回目以降の submission（`Evaluator::rearm_budget_for_repl` → `BudgetLedger::begin_repl_submission`）は、前の submission の artifact token
    （`ast_token`・`body_tokens`）を先に release し、heap 台帳を作り直さずに使用量（committed・reserved の全 counter と peaks）を 0 から数え直し、
    live を持ち越して peak を live に戻し、live 上限を据え直す（fix-repl-submission-budget、判断役が選択（PR #496）の Q1=A）。context に残る live が上限を
    超えていれば、Link の最初の heap 課金で `BudgetExceeded(HeapBytes)` になり 1 文も実行しない（RSB-AT-03）。
  - 再現: 1 回目の `Engine::run` で `let big = range(0, 20000)` を context に残し（live 640,448）、2 回目を `max_live_heap_bytes = 4096` で `print(len(big))` に
    すると `Completed`（`20000` を出力、live_heap_bytes 459）になる（`src/embedding.rs` の test module に一時的に置いた test で確かめた）。
- **実装範囲**（Q1=A）:
  1. facade の `Engine::run`・`run_audited` の `install_execution_state` は、REPL かどうか・初回かどうかに依らず、heap 台帳を作り直さない据え直しを使う。
     中身は REPL の 2 回目以降の submission と同じで、前の execution の artifact token（`ast_token`・`body_tokens`）を先に release してから、
     `BudgetLedger::begin_repl_submission` と同じ台帳の操作（request の `BudgetConfig`・clock・cancellation token を据え、使用量と peaks を 0 に戻し、
     live を持ち越して peak を live に戻し、live 上限を据え直し、監査の集約器を空にする）を呼ぶ。監査・host-call・yield の buffer の据え直しは
     `reset_budget` と同じに保つ。REPL と共通の操作にするため、操作と `Evaluator` 側の入口の名前（`begin_repl_submission`・`rearm_budget_for_repl`）を
     execution 一般の名前へ変えてよい。`ExecutionContext.repl_budget_installed` の分岐を残すか消すかは実装で決めてよい（観測できる挙動は同じ）。
  2. context に残る live heap は持ち越しによってそのまま新 execution の baseline になる（tracked 値は同じ台帳に 1 回だけ課金されたままで、§5.2 の visited の
     dedup と同じく二重に数えない）。baseline が新しい request の `max_live_heap_bytes` を超えるときは、Link の最初の heap 課金（import があれば module record、
     無ければ AST の token）で `BudgetExceeded(HeapBytes)` になり、1 文も実行しない。context は変更せず poison しない（§10 の root link 失敗と同じく
     script を実行していない）。別の走査は足さない。
  3. context に残っていた値を execution の途中で手放したとき（再代入・`pop` など）は、§10 の journal が元の値を保持するので、その execution の間は live に
     残る。commit で journal が破棄されたときに同じ台帳から release され、次の execution の baseline から外れる（rollback なら元の値が context に戻り、
     baseline に残る）（判断役が選択（PR #499 のレビュー指摘、Q1=A））。
  4. `reset_budget`・`reset_config_with_token` の doc comment と、`install_execution_state`・`run_for_repl` のコメントにある「run ごとに台帳を作り直す」
     「REPL だけ台帳を持ち越す」の記述を直す。facade の経路で使わなくなった `reset_budget` を消すか残すかは実装で決めてよい（scheduler の handle 経路は
     使っていない。`src/engine.rs` の経路は D-136 の範囲）。
  5. done にする実装 PR で逸脱一覧の D-137 の行を削除する。§5.2 は Q2=A の反映で台帳の持ち越しを baseline の求め方の 1 つとしたので、facade で
     `charge_context_baseline` を本番で呼ばないことは逸脱ではない。
- **範囲外**:
  - scheduler の handle 経路（`src/engine.rs` の `ExecutionContext`・`create_execution`・`start`・`execute`）。台帳を execution ごとに作り直さない差は
    D-136 の行で fix-execution-budget-usage-fresh（todo）が直す。その設計で台帳を作り直すかどうかを決めるときに、本スライスと同じ baseline の扱いに揃える。
  - VM（facade に VM の context を持ち越す経路が無い。VM REPL は fix-repl-submission-budget で済み）。
  - `charge_context_baseline` の削除（§5.2 の台帳を作り直す経路の部品として残す。Q2=A）。
  - 前の execution が展開した module の record（`graph_record_tokens`）の扱い。loaded module marker と同じ context の状態として live に残り、本スライスでは
    持ち越しの baseline に含まれるだけで、課金・release の規則は変えない。
- **観測される変化**: 同じ `ExecutionContext` で 2 回目以降の `Engine::run`・`run_audited` の `BudgetUsage.live_heap_bytes` と `peak_heap_bytes` が、context に
  残る値の分を含む（今は含まない）。pre-cancel（命令 0 の `Cancelled`）の usage の live も、context に値が残っていればその分を含む（新しい context なら 0 のまま）。
  committed・reserved の counter と peaks は今と同じく execution ごとに 0 から数える。fuel と `fuel_accounting_revision`・`HEAP_ACCOUNTING_REVISION` は変えない
  （§5.1 の論理サイズも課金点も変えず、context に残る値を判定に含めるだけ）。
- **受入基準（fix-link-context-baseline-charge）**:
  - **LCB-AT-01**（baseline が上限を超えると 1 文も実行しない）: 1 回目の `Engine::run` で `let big = range(0, 20000)` を context に残し、2 回目を
    `max_live_heap_bytes = 4096` で `print(len(big))` にすると `BudgetExceeded`（`HeapBytes`）になり、何も出力せず、context は poison されない。3 回目を
    緩い budget にすると `Completed` で `20000` を出力する。`run_audited`（監査 sink あり）でも 2 回目が同じ terminal になり、Terminal event の
    `context_committed` が `false` になる。2 回目の上限を、同じ source を同じ context で緩い budget で実行したときの Link 直後の live heap（baseline と
    AST の分の合計）ちょうどにすると 1 文目を実行し、その値 − 1 にすると 1 文も実行せず `BudgetExceeded`（`HeapBytes`）になる（§15.2 の境界）。確かめ方: `src/embedding.rs` の unit test。
  - **LCB-AT-02**（baseline を usage に含める）: 上の 1 回目のあと緩い budget で `let n = len(big)` を実行すると、`live_heap_bytes` が
    `heap_size::list_body(20000)` 以上で、`peak_heap_bytes` は 1 回目の途中の一時的な最大値（1 回目で作って捨てた大きな List）と 1 回目の AST の分を含まない。
    committed の counter は同じ source を新しい context で実行したときと一致する。`live_heap_bytes` は `2 × heap_size::list_body(20000)`（1,280,048）未満で、
    持ち越した `big` を二重に数えない。確かめ方: unit test。
  - **LCB-AT-03**（commit の境界で同じ台帳から release する）: 1 回目で `let big = range(0, 20000)` を残し、2 回目を緩い budget で `big = null` にすると
    `Completed` になる。3 回目を緩い budget で `let k = 1` にすると、その `live_heap_bytes` は、`big = null` の代わりに `big = big`（binding を足さず、`big` を残す）を
    2 回目で実行した対照の context の 3 回目より `heap_size::list_body(20000)`（24 + 32 × 20,000 = 640,024）ちょうど小さい（2 回目の AST は 3 回目の開始で
    release されるので差に入らず、3 回目は同じ source にそろえる）。続けて
    4 回目を `max_live_heap_bytes = list_body(20000) + 4096`（644,120）で `let other = range(0, 20000)` にすると `Completed` になり、対照の context で
    同じ 4 回目を実行すると `BudgetExceeded`（`HeapBytes`）になる。上限の根拠: `big` が release されていれば必要な live は新しい List の 640,024 と、
    小さな script の AST・cell などの数百 byte（PR #499 のレビューの計測で、`big` を残した context の `let k = 1` の live が list_body(20000) より 257 大きく、`big = null` と新しい List を同じ execution で作ったときの peak が 2 × list_body(20000) より 717 大きい）で、
    4096 に収まる。`big` が残っていれば baseline の 640,024 以上に新しい List の 640,024 が加わって 1,280,048 以上になり、上限を超える。
    execution の途中で手放した値は §10 の journal が保持するので、2 回目と 4 回目を 1 つの execution にまとめた形では確かめない（実装範囲 3）。確かめ方: unit test。
  - **LCB-AT-04**（失敗した execution のあとの baseline）: 2 回目を未捕捉の runtime error で終わらせ（途中で大きな List を作る）、3 回目の開始時の live が
    2 回目の開始時の live と同じになる（rollback した値と 2 回目の AST が残らない）。context に closure を残した場合、3 回目の live がその closure の
    retained code（§5.3 A-1）の分を含む。確かめ方: unit test。
- **想定の変更規模**: 本体（`#[cfg(test)]` を除く `src/`）は `src/embedding.rs` が ±15〜40 行（`install_execution_state` の分岐とコメント）、`src/eval.rs` が
  ±10〜30 行（据え直しの入口の共通化と doc comment）、`src/budget.rs` が ±5〜20 行（操作の名前と doc comment）で、合計 30〜90 行。テストは unit test で
  +120〜220 行（同じ context で usage の live を前提にした既存 test があれば期待値の直しを含む）。受入基準は 4 項目、触る仕組みは「facade の execution の開始での
  budget 台帳の据え直し」1 つで、[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
- **依存**: なし（fix-repl-submission-budget は done で、その台帳の操作を使う）。
- **決定記録**（判断役が選択（PR #499））:
  - **Q1=A**（facade の `Engine::run`・`run_audited` でも heap 台帳を作り直さず、REPL の 2 回目以降の submission と同じ据え直し〔artifact token を先に release →
    使用量と peaks を 0 に → live を持ち越して peak を live に → live 上限を据え直す〕をすべての run で使う）。理由: context に残る値が新 execution の live 上限の
    判定に入り、D-137 の「20,000 要素を残しても 4096 の上限で `Completed`」が塞がる（[マニフェスト](manifesto.md) 原則 3）。live が §2 の heap の定義（execution から
    到達可能な論理 allocation の live byte 数）と一致し、手放した値は同じ台帳から release され、closure の retained code・module record・cell も既存の課金のまま
    含まれる（原則 5）。据え直しの仕組みが REPL と facade で 1 つになり、走査も `Tracked` の構造変更も足さない（原則 8）。§9.2 の `Linked`（context baseline を
    課金済み）と §5.2 の規範を満たし、§13 の「使用量は execution ごとに新しく数える」は committed・reserved・peaks を 0 から数えることで保つ。
    fix-repl-submission-budget の Q1=A（PR #496）と同じ扱いである。採らなかった案 B（台帳を作り直し、走査で新しい台帳へ課金して tracked 値の台帳の参照を
    付け替える）は、`Tracked` の構造変更と cell・retained code・module record まで付け替える走査が要り、据え直しと付け替えの 2 つの仕組みを持つことになる
    （原則 8 に不利。走査と既存の課金が一致しなければ live がずれる危険も増える、原則 5）。採らなかった案 C（作り直した台帳へ `charge_context_baseline` を
    呼ぶだけ）は、release が新しい台帳に届かず live が過大に残り、走査が retained code・module record・cell を数えず過小にもなるので、上限の判定が到達可能な量で
    説明できず、原則 3・5 に反しうる。
  - **Q2=A**（§5.2 末尾の規範〔baseline live heap として課金し、上限を超えれば 1 文も実行せず `BudgetExceeded(HeapBytes)`、同じ object を二重に数えない〕は変えず、
    求め方を「台帳の持ち越し」と「台帳を作り直す経路での反復 worklist の走査 + visited set」の両方として書き直す。この PR で直した）。理由: Q1=A の挙動は規範を
    満たすので、目標の文を判断の反映として直すのが置き場所として正しく（原則 9、documentation-guide の目標と逸脱一覧の区別）、scheduler の経路
    （fix-execution-budget-usage-fresh、D-136）の設計もどちらの求め方も選べる。採らなかった案 B（目標の文を変えず逸脱一覧に行を足す）は、規範を満たす挙動を
    解消する予定の無い逸脱として残し、後の設計が同じ問いに戻る（原則 9 に不利）。
  - 反映で合わせて直したこと（判断コメントの申し送り）: §5.2 冒頭の「execution ごとに `AllocationLedger` を持つ」に、同じ context の次の execution が前の台帳を
    据え直して使ってよいこと（使用量と上限は新しい execution のもの、引き継ぐのは context に残る値の live だけ）を書き足した。§15.2 の「context baseline が limit
    ちょうどなら Linked、`+1` なら失敗」は、持ち越しの経路では Link の heap 課金（AST・module record）が baseline に足されてから判定されるので、判定の対象を
    「context baseline と Link の heap 課金の合計（`Linked` 時点の live heap）」と書き直し、§5.2 末尾の「baseline と Link の heap 課金の合計が上限を超える場合」と
    揃えた。この境界は LCB-AT-01 で確かめる。
  - **レビュー指摘への判断 Q1=A**（判断役が選択（PR #499 のレビュー指摘への判断））: 実装範囲 3 と LCB-AT-03 は、execution の途中で手放した値がその execution の
    live から外れる前提で書いていたが、facade の run は transactional で、§10 の journal（`Env::journal_cell` の `UndoEntry::CellValue`）が元の値を commit /
    rollback まで保持するので、そのままでは満たせなかった。release を確かめる時点を commit の境界に移し、実装範囲 3 を「commit で journal が破棄されたときに
    同じ台帳から release され、次の execution の baseline から外れる」に直し、LCB-AT-03 を書き直して上限の数値の根拠を書き、二重に数えないことは LCB-AT-02 の
    「live が list_body(20000) の 2 倍未満」で確かめる。§5.2 末尾の持ち越しの項の release の時点（最後の参照が消えた時点。journal が保持する間は commit /
    rollback まで残る）も揃えた。理由: §10 は「journal自身と旧valueの保持はheap budgetへ課金する」と定めており、途中で手放した値が live に残るのは規範どおりで、
    直すのは小節と受入基準の側である。journal が実際に保持している量を live に数え続けるので上限の判定が保持量と一致し（原則 1・3）、rollback の仕組みに
    触れない（原則 6）。触る仕組みと見積もりは変わらない（原則 8）。採らなかった案 B（execution の途中での release を目標にして journal の持ち方を変える）は、
    保持したまま live から外す（原則 1・3）か保持をやめる（全 language-state の rollback が成り立たない、原則 6）かのどちらかになり、§10 とも食い違い、
    本スライスの範囲を超えるため採らない。同じ execution の中で大きな値を手放して作り直すと journal のために peak が 2 倍になる点は §10 の規範どおりで、
    本スライスでは扱わない。

## 15. 境界受入テスト

実装完了には、通常unit testに加えて次をすべて自動化する。

### 15.1 budget

- 全fieldについてlimitちょうどが成功し、同一操作の`+1`が対応する`BudgetExceeded`になる
- 0上限、`u64::MAX`近傍、`usize`変換、加算・乗算overflowでwrapしない
- 複合reservationが部分的に成功せず、固定優先順位のresourceを返す
- operation開始前の失敗は全額refundし、開始後は規則どおりcommitする
- deadline直前は実行でき、fake clockがdeadlineと等しい時点で停止する
- budget超過を`try` / `catch`で囲んでもcatch bodyを実行しない
- host responseのdescriptor上限・execution累積上限のN/N+1と同時超過を試し、いずれもHostErrorではなく`BudgetExceeded(HostResponseBytes)`になる

### 15.2 heap

- 同じ`Rc`を複数変数・List・closureから参照しても1回だけ課金する
- copy-on-writeで新AllocationId分を課金し、最後の参照dropでlive heapをreleaseする
- root AST、VM chunk、function、import record、rollback journalが表の論理サイズと一致する
- context baselineとLinkのheap課金の合計（`Linked`時点のlive heap）がlimitちょうどならLinkedになり、`+1`ならscriptを実行せず失敗する
- 大量のallocate/freeはlive heapを回復してもstring allocation/countの累積上限を迂回できない

### 15.3 state・race

- 各非terminal stateからcancelでき、terminal eventは1回だけになる
- completion対cancel、deadline対cancel、host response対cancelをbarrierで同時発生させ、規定linearizationになる
- terminal後のpoll/resume/config変更をすべて拒否する
- yield/pause/resume後もstack、frame、handler、import、context、budget usageが失われない
- `Denied`、未捕捉error、HostError、budget/deadline、cancel、audit/record/replay/internal failureでbinding/List/upvalue/import markerがrollbackし、最終outcomeがCompleted/Exitedのときだけcatch済みerrorを含む変更をcommitする
- 全state×`poll`/`pause`/`resume`の合法・不合法matrixが`HandleError`表どおりで、terminal後はoutcome参照以外を拒否する
- active/queue満杯時のcreate/startがhandleを返さずcontextを変更しない。非terminal dropがrollback、Detached close、Terminal appendをblockingなしで完了する
- pause/resumeがCreated/Linked/Ready/Yieldedの元`resume_to`へ戻る
- rollback不能なfake host effectは残り、その事実がterminal監査へ反映される

### 15.4 fairness・backpressure・host

- 常にReadyなN executionが各1 sliceずつFIFOで進み、1 executionが連続独占しない
- active上限とqueue上限ちょうどを受理し、`+1`を即時Backpressureにする
- Pausedとhost待ちもactive slotを消費し、無制限admissionにならない
- queue待ち・pause・host待ち中にもdeadline/cancelが機能する
- blocking fake hostを使ってもcaller threadが無期限blockせず、run-turn queue上の別executionが進む
- response byte上限より大きいstreamを全量bufferせず途中で停止する

### 15.5 differential

- 同一fixtureのtree/VMで、charge trace、usage、yield位置、terminal reason、context commit結果、host effect順序が完全一致する
- AUD-022の網羅matrixへ各budget境界、REPL継続、pause/resume、cancel、host pendingを追加する
- subprocess timeout付きstress/fuzzでpanic、abort、OOM前の無制限allocation、terminal後実行がない

> **PR-6c 線引き（REV-015 Slice 6）**: PR-6c-min の射程は、同一 fixture を tree/VM で走らせて
> **budget usage（engine 間不変な軸）+ terminal reason（`TsumugiError` 全 field）+ stdout/stderr の
> tree/VM parity** を突合する differential matrix（`tests/differential.rs`）までとする。以下は defer:
> - **(1) execution-trace determinism**（yield 位置 / host-effect 順序ログ / audit payload /
>   charge 系列 trace の完全一致）→ Phase 5/6 の differential gate。基盤（`DeterminismInput` /
>   `normalize_for_conformance` / effect log 比較 / charge trace 記録 API）が未実装のため Slice 6
>   単独では成立しない。
> - **(2) pause/resume・cancel・host pending**（上記 2 番目の箇条書き）→ **VM 対象外**。VM は
>   scheduler 非経由（Ready フォールバック・`vm.rs` 不変、本節 Slice 5 本文）で該当実行経路を
>   持たず differential が原理的に成立しない。tree 側は `tests/scheduler_api.rs` で担保済み。
> - **(3) fuzz/stress/生成 matrix**（上記 3 番目の箇条書き）→ AUD-022 / REV-024 / Phase 7
>   （[検証・リリース・運用設計](verification-release-operations.md) Slice 3）。
>
> **比較は軸別ポリシーを用いる**: engine 間不変な軸（I/O counter・string/source/import counts・
> per-item peak・terminal reason の kind/message）は exact 一致を要求し、**fuel count と absolute
> heap（`live_heap`/`peak_heap`）は向き・符号・release 寿命の一致**で突合する（byte-exact 一致は
> §5.3 の A-1 どおり backend 間で成立しないため要求しない。fuel 枯渇の terminal `line` も step
> 粒度差で engine 間に差があり、fuel parity は StepLimit terminal の observable 一致で担保する）。
> **（2026-10-20 注記）** この線引きは不十分だった: differential が両 engine とも上限に達する
> script だけを比べていたため、「tree では成功し VM では step 上限」という observable 差を検出
> できなかった。fuel は §14「F-1 実装スライス」で tree の課金点に揃え、`committed.fuel` の
> exact 一致と上限 N−1/N の境界一致で突合する。
>
> **PR-6c-min 完了 = Slice 6 charge-parity-matrix 自動化の完了であって、VM の experimental 卒業
> ではない。** 卒業 gate は determinism/audit conformance（§16・[決定性・監査仕様](determinism-and-audit.md)
> Slice 6）であり後続フェーズの管轄である。

## 16. 完了条件

Phase 3は、全`BudgetConfig` fieldが全生成・I/O・host経路へ適用され、超過がcatch不能terminalとなり、logical heapとdeadline/cancelを含む境界テストが通った時点で完了とする。

Phase 4は、treeの全制御状態がcontinuationへ保存され、slice実行、yield、pause/resume、bounded admission、FIFO fairness、nonblocking host call、backpressure、race testが通った時点で完了とする。VMは[決定性・監査仕様](determinism-and-audit.md)の適合gateを通るまでexperimentalのままとする。

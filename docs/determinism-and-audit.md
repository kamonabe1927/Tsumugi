# Tsumugi 決定性・実行時監査仕様

最終更新: 2026-11-26

設計ステータス: **到達目標の確定仕様（§1〜§13）＋ 実装の現在地（下記の逸脱一覧）**

本文書の §1〜§13 は **到達目標の確定仕様**であり、現在の実装状態ではない。現在の実装が目標仕様からどう
逸脱しているか・何が未実装かは、下の **逸脱一覧** と **未実装範囲** の 2 表が唯一の正本である（状態の正本は
この 2 表に置き、§14 のスライス小節は再掲しない）。進捗の唯一の正本は [ロードマップ](roadmap.md)。

2 表は現在の逸脱と未実装範囲だけを持つ。done にする実装 PR は、この 2 表に新しい逸脱・未実装範囲を追加し、
解消した行を削除する（どのスライスで解消したかはスライス小節と git 履歴に任せる。[ドキュメント運用ガイド](documentation-guide.md)
「逸脱一覧の持ち方」）。

## 逸脱一覧（目標仕様 §1〜§13 に対する現在の逸脱）

| 逸脱内容 | 対象の節(§) | 発生スライス |
|---|---|---|
| 監査は `EngineBuilder::audit_sink` 設定時だけ有効（opt-in）。sink 未設定時は event を emission せず既存挙動と bit-identical。§1.1 の sink 必須（`ConfigError::AuditSinkRequired`）は未適用 | §1.1 | A-1 |
| Terminal 配送失敗後に完了済み script の language-state を rollback しない（commit が Terminal 配送より前に確定するため）。host は `AuditFailed` を受けた context を再利用してはならない | §10.1 | A-1 |
| envelope の `timestamp.unix_nanoseconds` に wall/unix `AuditClock` でなく request の monotonic tick を流用（順序の正本は `sequence` なので影響限定） | §7.1 | A-1 |
| host-call triple の §8 規則5/6 の timing（allow ack 前に外部効果を止める／`HostCallFinished` ack 前に response を露出させない）を満たさない。外部効果は `dispatch_fs` 内で同期完結し、triple は評価器 buffer から後で drain する。満たすのは triple の論理順序（規則3/4） | §8/§11 | A-1 |
| §11 の host call 開始前 atomic reserve を pre-effect reservation でなく drain 時の容量チェックで代用。外部効果自体を reserve 失敗で止める faithful 実装は未了 | §11 | A-1 |
| `AdmissionQueued` / `SchedulerPreempted` / `HostPaused` / `HostCallPending` の yield は engine-poll 経路（`engine.rs::poll`、sink 無し）でのみ生じるため、写像は定義するが監査付き経路では emission しない。sink-bearing な scheduler/pause/cooperative host-call 経路の配線が未了 | §7.2/§8 | A-1 |
| VM は host-call triple と flush 点の event を内部 buffer へ積むが、監査付き実行（`run_audited`）の経路が無いため配送しない。yield/resume は tree-only（監査 sink と `VmExperimental` の同時設定は build 時に `ConfigError::AuditSinkWithExperimentalBackend` で拒否。audit-vm-triple-b・c で解消予定）。buffer の `BudgetCharged` のうち `HeapBytes` は engine 固有の artifact・frame 構造で値が決まり tree と一致しない（§15.5、`normalize_for_conformance` は未実装） | §2.1/§7.1 | A-1（audit-vm-triple-a で更新） |
| §10 の「次回 `ExecutionHandle::poll` が `try_take` して再開」を、facade 内の単一 execution poll driver（`run_audited_driver`）が sink round-trip を担う形で代用。scheduler engine（`engine.rs::poll`）は sink を持たず `YieldReason::AuditBackpressure` を返さない（E10 統合後） | §10/§10.2 | B-1 |
| audit drain queue byte 上限の判定がトップレベル境界だけの soft limit。関数・callback 本体の中では効かず、queue は上限を超えて溜まり得る（超過量は host-call 回数 budget で有界） | §10.2 | B-2 |
| host-call triple を emission するのは fs builtin のうち読み書きの payload byte が明確な 4 つ（`read_file` / `read_lines` / `write_file` / `append_file`、`src/builtin_core.rs::emits_host_call_triple`）だけ。`rename` / `mkdir` / `remove` 系 / `list_dir` / `file_size` / `is_file` / `is_dir` / `path_exists` など capability 判定を通る他の fs builtin は監査対象外（「fs builtin 監査範囲」スライスへ延期） | §7.2/§8 | R-1a |
| **（セキュリティ）** 既定 `hmac_key` が crate 内の固定値（`src/audit.rs` の `DEFAULT_HMAC_KEY`）で秘匿性がない。raw path を直接載せないことだけを担保し、key が公開されているため digest から候補 path を総当たりで推測・復元できる（§9.1 の辞書攻撃耐性は得られない）。秘匿性のある key を host から渡す経路は sink 必須化スライスへ延期 | §9.1 | R-1a |
| Terminal の実 encode 長が `terminal_reserve_bytes` を超えても append する。journal は Terminal の実長を `terminal_bytes` に数えるが上限と比べず、§11 の Terminal reserve（64 KiB）を超える Terminal を拒否・縮退しない（60 KiB 縮退と Terminal の `Emergency` への置き換えは audit-strict-cbor-c4、SC-AT-18）。Terminal が reserve を超えうる状態は c2 の前から同じで、c2 が上限を緩めたものではない | §11 | audit-strict-cbor-c4 |
| `AuditField` の `original_bytes` を §9.1 の canonical subtree 長（§9.2 の encoding で測る）ではなく値の UTF-8 byte 長で代用（Path は canonical label または requested path の長さ、引数型エラーは 0）。strict CBOR スライスで揃える | §9.1 | R-1a |
| Path の HMAC-SHA-256 の入力が canonical label の UTF-8 生 byte（`src/audit.rs::RedactionPolicy::hmac_path`）。§9.1/§9.2 の「redaction 前 subtree の canonical encoding」（label を値とする `AuditValue` の deterministic CBOR）ではないため、digest 値が目標仕様と異なる（秘匿性・決定性は保たれる）。strict CBOR スライスで揃える | §9.1/§9.2 | R-1a |
| `AuditFrame.module_id` / `AuditError.module_id` が §7.2 の `AuditField`（Identifier/Path policy 適用済み）ではなく plain `String`（`Option<String>`）。現在は常に空文字列 / `None` で埋めており（`src/embedding.rs` の Terminal error 構築）module 情報を記録しない。§9.2 の encoding も `AuditField` ではなく text / `Option` になる | §7.2/§9.1 | A-1 |
| `AuditError.message_id` が §4.6 の `ErrorMessageId`（canonical message の template ごとの安定 ID）ではなく error code の文字列の流用（`src/embedding.rs` の `audit_error_payload_from` が `code` と同じ `error.code.as_str()` を入れる）。同じ関数は `AuditError.column` も常に `None` で埋め、source 位置の列を記録しない。§9.1 の「安定 `ErrorCode`、`message_id`、source 位置」のうち `message_id` と列が目標と異なる。loop:drift #429 D-126 | §7.2/§9.1 | A-1 |
| Identifier class の mode を `HmacSha256` にしたとき digest を出さず `Omitted` に畳む（`src/audit.rs` の `fold_audit_field`）。§9.1 の表は Identifier を host が hash/omit へ変更可とするが、hash は実現しない（omit は実現する）。mode を変える公開 setter は無く（未実装範囲の監査設定の公開 setter の行）、今はテスト用 shim からだけ届く。loop:drift #429 D-127 | §9.1 | R-1a |
| 公開 `ExecutionOutcome`（`src/embedding.rs`、crate root では `EmbeddingOutcome`）は `Completed` / `Exited` / `RuntimeError` / `BudgetExceeded` / `DeadlineExceeded` / `Cancelled` / `InternalFailure` の 7 variant だけで、§7.2 の `TerminalOutcome` と 1 対 1 に対応しない。link 時の拒否と resolver の host 失敗は `Engine::link` の `Result<_, LinkError>`（`LinkError::Denied` / `LinkError::Resolve(HostError)` ほか）で返し、run 中の capability 拒否と host function の失敗は `RuntimeError`（`ErrorKind::Capability` / `ErrorKind::Host`）に含める。監査失敗は監査付き経路だけの別型 `AuditedOutcome::AuditFailed` で返す。`RecordFailure` / `ReplayMismatch` は record/replay が未実装のため無い | §7.2 | A-1（7 variant の構成は [組み込みAPI仕様](embedding-api.md) の E 系スライス由来） |
| fs 以外の host call（stdout の `print`、stdin の `input`、clock の `now`、`env`、`exit`、登録 host function、import resolver）は host-call triple を出さない。§5 の `HostBoundary` の各 request と §8 規則3（import resolver を含む host request）に対し、triple を出すのは上の行の fs 4 builtin だけ（audit-host-boundary-scope スライスへ延期） | §5/§8 | A-1 |
| heap の増減の一部が `BudgetCharged` の accumulator へ通知されない。通知するのは `BudgetLedger::release_heap`（retrack の縮小と frame・try の戻し）だけで、`BudgetLedger::charge_heap` の確保、`Tracked` の `Drop` の release（`src/value.rs`）、`Tracked::new_via_handle` の確保（変数 cell・関数 header・heap token）は通知しない。heap の `Allocation` の確保の `BudgetCharged` が出ず、寿命終了の release も `released_delta` に入らないため、Terminal の live heap と `BudgetCharged` の delta を突合できない（fix-heap-charge-audit-notify スライスで解消予定。issue #369） | §7.3/§15.3 | A-1 |
| cancel・deadline・drop で待ちを終えた pending host call を `Cancelled` / `Detached` の `HostCallFinished` で閉じない。§7.2 は `HostCallOutcome` に `Cancelled` / `Detached` を定め、§10.1 は監査失敗の fail-closed のとき pending host call をそれらの `HostCallFinished` で論理的に閉じると定める。cancel・drop のときに閉じる規定は [実行予算・協調実行仕様](execution-control.md) の §6.2（cancel 時は監査上の host call を `Cancelled` または `Detached` で閉じる）と §9.1（非 terminal handle の drop は pending host call を `Detached` で閉じる）にある。これらに対し、同期 `Engine::run` の待ち（`src/embedding.rs` の `wait_for_host_call`）も、scheduler の handle の cancel・drop の経路も、その event を出さない（[実行予算・協調実行仕様](execution-control.md) の fix-sync-run-host-call-resume 小節の範囲外の項）。loop:drift #420 D-113 | §7.2・§10.1 | — |

上の表の `ExecutionOutcome` が 7 variant だけである行（D-8）は、loop:drift #263 を受けて載せた。本文（§7.2 の 1 対 1 対応）を実装に合わせて後退させず、逸脱として記録する案を選んだ（issue #263 Q1=A。返信なしのため推奨案で確定、[開発ループ手順書](loop-runbook.md) §7.5）。理由: spec が実装の正本であり、現在の逸脱は事実を確かめて逸脱一覧に置く（[ドキュメント運用ガイド](documentation-guide.md)「逸脱一覧の持ち方」）。D-8 の `ExecutionOutcome` は `#[non_exhaustive]` で、後続 Phase で variant を足す前提になっている。[組み込みAPI仕様](embedding-api.md) の `ExecutionOutcome`（13 variant）との差は、同 spec の逸脱一覧に載っている（loop:drift #429 D-128）。

host call 開始前に `BudgetCharged` を flush しない行（loop:drift #342 の D-50）は、§7.3 を正とし、本文を実装に合わせて後退させず逸脱として載せ、fix スライスで HostCallStarted の前に flush する案を選んだ（issue #342 の D-50 Q1=A。判断役が選択（issue #342））。理由: host call の前の資源消費を Started と対応づけられ、監査で説明できる（[マニフェスト](manifesto.md) 原則 5・原則 7）。A-1 小節の deferred drain モデルは buffer 内の順序を決めていないため、buffer 内の flush は A-1 の逸脱を増やさない。採らなかった案 B（§7.3 から「host call開始前」を外す）は、目標を実装へ後退させ、host call 前の消費を監査上 Started と対応づけられなくなる（原則 7 に反しうる）。

`host_effects_may_remain` が常に `false` の行（D-51）は、逸脱として載せ、fix スライスで §13 AUD-024 に書いた規則（`effect_status` が `Committed` または `Unknown` の host call が 1 件でもあれば outcome にかかわらず `true`）を実装する案を選んだ（issue #342 の D-51 Q1=A。判断役が選択（issue #342））。理由は AUD-024 の該当段落に書く。

## 未実装範囲（目標仕様 §1〜§13 のうち未着手・延期中）

| 未実装範囲 | 対象の節(§) | 備考 |
|---|---|---|
| sink 必須化（§1.1、`ConfigError::AuditSinkRequired`。opt-in を既定から外す） | §1.1 | CLI 側の sink 構築が整う後続スライスへ延期 |
| 監査設定の公開 setter（`EngineBuilder::audit_redaction_policy` / `EngineBuilder::audit_budget`）と `ConfigError` への `AuditConfigError` 接続。現状は `#[doc(hidden)]` のテスト用 shim（`audit_redaction_policy_for_test` / `audit_budget_for_test`、不正 budget は panic）だけ | §1.1/§9.1/§10.2 | sink 必須化スライスへ延期 |
| 60 KiB 縮退規則（normal event の最大 60 KiB、超過時 Body → Path → Identifier の順で `Omitted` へ縮退、なお超過で `EventTooLarge`）。`AuditFailure::EventTooLarge` は型としてあるが、判定は単一 event が通常 byte 枠全体（`max_encoded_bytes` − Terminal 予約）を超える場合だけで、60 KiB 判定と縮退は無い | §7.2/§9 | 正確な byte 推定が前提のため strict CBOR スライスへ延期 |
| record/replay | §6 | — |
| record/replay での redaction 再適用（record 済み redacted payload を盲目コピーせず現行 policy を適用） | §12 | record/replay スライスへ延期 |
| VM の audit triple 対応（監査 wiring） | §2.1 | audit-vm-triple-b・c（Slice 6、VM conformance）へ延期 |
| VM 適合 gate のうち audit 以外の残り。差分テストの土台と既知の差分一覧の CI gate（`tests/vm_parity.rs`。print だけで動く fixture・書き換えの種類 × 値の持ち方 × 変数の置き場所の matrix・REPL 継続を、値・error・出力・fuel・budget の各カウンタ・heap で比べ、一覧と実際の差を完全一致させる）は vm-parity-diff-a で入った。capability の要る fixture（env・fs・clock・import。import される側も単体の root として）を CLI と同じ `Engine::link` の組み立てで同じ軸で比べる流し方と、全 fixture の CLI（`tsumugi` と `tsumugi --vm`）の stdout・stderr（VM の experimental の警告の 1 行を除く）・終了コードの byte 比較は vm-parity-diff-b で入り、この 2 つでは差が出なかった（既知の差分一覧に項目は増えていない）。埋め込み API の `Engine::run` で context を使い回す流し方（`Mode::Context`）は vm-engine-run-route で tree・VM とも `Engine::run` で流すようになり、KD-1（経路なし）を消した（context の継続・transaction・制御と注入の `context/` のケースでは差が出なかった）。残りは、limit 境界（各上限の N と N−1）の網羅、pause/resume、fuzz gate と、既知の差分一覧の 3 項目（KD-2/KD-3 `for` の反復の peak heap・KD-4 REPL の error の入力の fuel）を 0 件にすること | §2.1 | Slice 6（VM conformance）。フェーズのスライス（手順書 §6.7）で減らす |
| `normalize_for_conformance`（backend 間比較の前の envelope 正規化） | §3 | Slice 6（VM conformance）へ延期 |
| `DeterminismInput` tuple と `ContextSnapshot` | §3.1 | record/replay スライスへ延期 |
| `HostBoundary` dispatcher と `FakeHost` | §5 | Slice 2（HostBoundaryとFakeHost）へ延期 |
| engine 全体の audit drain queue byte 上限（16 MiB）。per-execution 上限（`max_pending_bytes`）は B-2 で実装済み | §10.2 | 複数 execution 共有の queue 会計と wake 経路が scheduler（E10 統合）と不可分なため延期 |
| sink callback 中の再入検出は facade の入口だけが対象で、scheduler の `ExecutionHandle` の操作（§10 の start/poll/pause/resume）の再入検出と、拒否に返す値は未実装（scheduler の `Engine` は sink を持たない） | §10 | sink を持つ scheduler 経路は E10 統合のスライスで扱う |

## 1. 位置づけ

本文書は、[Tsumugi Manifesto](manifesto.md)と[ロードマップ](roadmap.md)のうち、マニフェスト実現ロードマップ Phase 5「規範意味論と決定的境界」とPhase 6「実行時監査」の実装仕様を定める。

本文書は次の文書と一体で実装する。

- [組み込みAPI仕様](embedding-api.md): Phase 1/2の`Engine`、compile済みscript、実行context、構造化outcomeの先行契約
- [Capability Model仕様](capability-model.md): Phase 2のdeny-by-default policy、host function registry、各外部効果の境界
- [実行予算・協調実行仕様](execution-control.md): Phase 3/4の最終budget、continuation、yield、pause、cancel、scheduler、transaction
- [次期意味論・実装決定](semantic-decisions.md): 次期言語挙動、CLI、canonical error、catch可否
- [言語仕様](language-spec.md): 現行実装から観測できる意味論

監査は秘密情報の保存機能ではなく、hostが実行の事実・許可・外部操作・資源消費・終了理由を検証するためのcontrol-plane機能である。audit sinkはscriptから直接参照できず、host capabilityにも公開しない。

### 1.1 Phase 1/2文書との統合規則

Phase 5/6実装では、[組み込みAPI仕様](embedding-api.md)と[Capability Model仕様](capability-model.md)の先行契約を次の最終契約へ統合する。本文書がaudit eventとfailure policyの正本である。

- `EngineConfig`の暫定`BestEffortWithCounter`既定は廃止し、10節の`FailClosed`だけを既定かつ初期実装で唯一のpolicyとする。sink未指定はin-memory fallbackへ暗黙変更せず`ConfigError::AuditSinkRequired`とする。
- capability文書の`CapabilityAllowed` / `CapabilityDenied`暫定eventは、`AuditEvent::CapabilityDecision { decision: Allow | Deny }`へ統合する。別event名を残さない。
- Phase 2の同期adapterはPhase 4以前の契約である。Phase 4のcooperative extensionだけが`Pending`を返せ、Phase 5/6は両者を同じhost call correlationへ記録する。
- budget、deadline、yield/pause、terminal stateは[実行予算・協調実行仕様](execution-control.md)の最終型を使う。
- context commit/rollbackとfunction identityは次期意味論の`FunctionId`および全language-state transactionへ統一する。Phase 1のprefix commit記述を残さない。

これらは同じbreaking alpha releaseで更新し、同じbuildにbest-effort/fail-closedや旧/新event enumを併存させない。

## 2. 規範backendと適合方針

### 2.1 treeを規範backendとする

Phase 5のproduction backendはtree-walk evaluator（以下`tree`）だけとする。`tree`は[言語仕様](language-spec.md)を実行する規範backendであり、VMは全適合gateを通るまで`experimental`である。

新規6設計文書の規範優先順位は領域別に次のとおりとする。

1. 次期言語挙動、CLI、canonical error、catch可否は[次期意味論・実装決定](semantic-decisions.md)
2. Phase 3/4の公開API、状態機械、budget、AUD-024 transactionは[実行予算・協調実行仕様](execution-control.md)
3. Phase 5/6のaudit schema、record/replay、redaction、fail-closedは本文書
4. Phase 0の保証境界とhost/Tsumugi/OS責任は[脅威モデル](threat-model.md)
5. Phase 1/2の組み込み・capability契約は上記最終契約の内部subset
6. 規範fixtureとtreeの観測結果、実装内部の都合

現行releaseで利用者が依存できる挙動は[言語仕様](language-spec.md)に従うが、次期revisionの設計・実装ではsemantic-decisionsを先に満たし、完了後にlanguage-specへ統合する。仕様とtreeが矛盾する場合は該当revisionの仕様を正としてtreeを修正し、treeのbugを無条件に仕様へ昇格しない。

VMは次を満たすまで、embedding APIのproduction選択肢、record作成backend、監査保証対象にしない。

- 全規範fixtureでvalue、error、context commit、side effect、fuel、`normalize_for_conformance`後のaudit semantic eventがtreeと一致する
- AUD-016、AUD-017、AUD-019、AUD-024、AUD-048を含む既知非適合が0件である
- differential matrix、limit境界、REPL継続、pause/resume、record/replay、fuzz gateを通る

backend名は監査metadataには記録するが、backendは結果差を許すdeterminism inputではない。

## 3. 決定性の定義

同一のdeterminism input tupleから、次が同一になることを決定性と呼ぶ。

- scriptの最終valueまたは構造化terminal outcome
- catch可能errorの安定code、script-visible type/message/line
- `ExecutionContext`へcommitされるbinding、collection、function identity、module状態
- host call requestの順序、operation、引数、許可・拒否、外部効果の論理順序
- output byte列とinput消費順序
- budget charge列とterminal時`BudgetUsage`。同じpoll scheduleを与えるexecution-trace比較ではyield reasonも含む
- timestamp等の運用fieldを除いたaudit semantic event列

engine全体で同時に走る別execution間の外部効果順序は決定性の対象外である。1 execution内の順序だけを保証する。複数executionを跨ぐ順序が必要なhost functionは、host側でtransaction keyやserialize policyを提供する。

### 3.1 DeterminismInput tuple

```rust
pub struct DeterminismInput {
    pub language_revision: LanguageRevision,
    pub root_source: SourceBlob,
    pub modules: BTreeMap<ModuleId, SourceBlob>,
    pub initial_context: ContextSnapshot,
    pub input_events: Vec<InputEvent>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub semantic_clock: Vec<WallClockValue>,
    pub monotonic_clock: Vec<MonotonicTick>,
    pub capability_policy: FrozenCapabilityPolicy,
    pub budget: BudgetConfig,
    pub host_responses: Vec<RecordedHostResponse>,
    pub poll_schedule: Vec<PollSlice>,
    pub control_events: Vec<ControlEvent>,
    pub rules_revision: DeterminismRulesRevision,
}

pub struct ContextSnapshot {
    pub generation: u64,
    pub bindings: BTreeMap<String, RecordedValue>,
    pub loaded_modules: BTreeSet<ModuleId>,
    pub next_function_id: u64,
    pub next_class_id: u64,
}
```

各fieldの意味を次で固定する。

| field | 内容 |
|---|---|
| `language_revision` | 文法、Unicode data、float・error・path規則を含む言語仕様revision |
| `root_source` | 正規化前の有効UTF-8 source byte列とroot `ModuleId` |
| `modules` | import実行前解決に使うimmutableなmodule snapshot。key順は`BTreeMap` |
| `initial_context` | 実行開始前のbinding/cell/value、loaded module、rollbackで再利用しない単調identity counterのsnapshot。fresh contextは全map空・counter 0 |
| `input_events` | input payload、EOF、I/O errorの順序付き列 |
| `args` | hostが明示注入する引数。process argvを直接読まない |
| `env` | 許可済み環境snapshot。process environmentを直接読まない |
| `semantic_clock` | `now()`等、scriptから観測できるwall-clock response列 |
| `monotonic_clock` | deadline、host latency、scheduler control用の単調tick列 |
| `capability_policy` | execution作成時にfreezeしたgrant、deny rule、provider version |
| `budget` | 全上限、deadline、heap accounting revision |
| `host_responses` | host callごとのvalue/error/effect status/byte数/完了tick |
| `poll_schedule` | 呼出し順の`PollSlice.max_fuel`列。yield回数・位置を比較する場合は同一列を使う |
| `control_events` | pause、resume、cancelをどのcheckpointで観測させるかを示す列 |
| `rules_revision` | 本文書のbackend非依存規則revision。初期値1 |

`ContextSnapshot.bindings`の`RecordedValue`はtype tag付きDAG encodingを使い、List/Dict backing、cell、function code/captureをlocal object IDで1回だけ表現する。Dict/bindingはkey byte順、Listはindex順、functionは`FunctionId`・source hash・source node ID・capture cell IDを保存する。snapshot採取中はcontextを排他borrowし、秘密値を含み得るsnapshot/record storeはaudit redactionとは別に暗号化・access controlする。snapshotを復元できない型をhost valueとしてcontextへ保存してはならない。

`execution_id`、worker thread、OS process ID、実時間scheduler順はsemantic tupleへ含めない。audit byte列まで再現する場合は、注入`execution_id`、`AuditClock`、redaction keyもreplay metadataとして同じ値を渡す。

決定性を2層に分ける。どちらも同じ完全な`DeterminismInput`、したがって同じ`poll_schedule`と`control_events`を前提とする。**semantic determinism**はvalue/outcome/context/host effect/budget totalを比較し、yield eventを出力比較から除く。**execution-trace determinism**はyield/resume/poll index/audit sequenceまで比較し、tree/VM differential gateはこちらを使う。

slice量だけを変えるmetamorphic testは別入力間の強い追加性質であり、fake monotonic clockが全scheduler-only checkpointで同じtickを返し、terminalまでdeadline未到達、pauseなし、host response completion checkpoint固定の場合だけterminal/context/effect/budget totalの一致を要求する。production clockやdeadline到達可能な実行ではpoll scheduleが待ち時間とdeadline outcomeを変え得るため、異なるslice間の一致を保証しない。deadline check自体は各scheduleで[実行予算・協調実行仕様](execution-control.md)どおりqueue/yieldを含めて行う。

監査eventをbackend間比較する前に`normalize_for_conformance`を適用し、envelopeの`execution_id` / `timestamp`、ExecutionStartedの`engine_version` / `backend` / `mode`だけを除外する。`source_hash`、language revision、sequence、resource、decision、call ID、usage、error code等は除外しない。同じbackend・modeのrecord/replay byte一致試験では正規化せず比較する。

## 4. Backend非依存の規則

### 4.1 評価順とside effect順

1. statementはsource順に実行する。
2. unary operand、binaryのleft/right、callのcallee/arguments、list要素、dict key/valueは左から右へ評価する。
3. `and` / `or`は左を先に評価し、短絡した右辺は評価・課金・監査しない。
4. index assignmentはtarget解決、index、value、mutationの順とする。
5. call先の存在・arity・破壊対象妥当性を、現在の言語仕様で定めた位置に検査し、検査前に評価してはならないargumentを揃える。
6. 1 executionが同時に未完了にできるscript host callは1個だけとする。responseを受け取るまで次のscript statementへ進まない。
7. host call開始、capability decision、外部効果、結果公開の順序は監査順序と一致させる。
8. 最初の未捕捉error、budget、cancel、audit failureより後のscript side effectを実行しない。

### 4.2 importとsource

importは[言語仕様](language-spec.md)どおり、最初のscript statementより前に全件解決する。module resolverは[Capability仕様](capability-model.md)の注入adapterだけを使い、次を固定する。

- source内のimport specifierは正規化せずresolverへ渡す。filesystem resolverの相対path検査はCapability仕様の`RelativePath`規則に従う。
- resolverが返す`ModuleId`はopaqueで安定なUTF-8 identifierであり、同じlogical moduleには同じIDを返す。engineはpathとして再解釈・case fold・Unicode normalizeしない。このno-normalization契約（§4.4のlocale/Unicode非正規化と一致）は`ModuleId`の型のdoc commentにも書く。
- `ModuleId`にabsolute local path、credential、query secretを含めない。
- 同一execution中のmodule graphはimmutable snapshotであり、host filesystemやresolver登録の後続変更を観測しない。
- 解決・parse・capability・budget失敗時は1文も実行しない。
- 正常時のmodule top-level statementはimport文の位置へ展開した順で実行する。

`source_hash`はroot sourceの正規化前byte列に対するSHA-256である。BOM除去、改行変換、Unicode normalizationを行わない。`import_graph_hash`は[組み込みAPI仕様](embedding-api.md)の`LinkedScript.import_graph().graph_hash()`を正本とし、同文書のbyte-level仕様をそのまま共有helperで呼ぶ。domain separatorは末尾NULを含むASCII `TSUMUGI-IMPORT-GRAPH-V1\0`、integerはu64 big-endian、stringは`length || UTF-8 bytes`であり、language revision、root hash、root import ID列、`ModuleId` byte順のnode列、各nodeのsource hashとsource順import ID列をencodeする。本文書側に別algorithmを実装しない。

### 4.3 path

scriptが扱うfilesystem pathは[Capability仕様](capability-model.md)の`RelativePath`とpath-handle adapter契約に従う。deterministic modeではOS固有separator、current directory、drive letter、case folding、symlink解決結果をscript意味論へ直接混入させない。filesystem providerのresponse、canonical resource label、errorはhost responseとしてrecordする。

pathの表示順・比較はproviderが返すpublic UTF-8 labelのbyte順とする。host pathをscriptへ公開しないproviderではopaque labelだけを使う。auditへraw pathを既定で記録しない。

### 4.4 localeとUnicode

- sourceとStringは有効UTF-8であり、内部の順序はUnicode scalar value順とする。
- 暗黙のNFC/NFD normalizationを行わない。見た目が同じでも異なるscalar列は異なる文字列である。
- Dictは`BTreeMap<String, Value>`を使い、key iteration、`keys`、`values`、Dict表示、deterministic serializationを同じ順序にする。
- process localeを参照しない。数値parse/formatの小数点は`.`、時刻formatはUTC、error codeはlocale非依存とする。
- 大文字・小文字変換等がUnicode tableへ依存する場合、`language_revision`へUnicode data revisionを固定する。Rust標準libraryのversion追従で無言に意味を変えない。

### 4.5 FloatとNaN

- FloatはIEEE 754 binary64で、fast-math、暗黙fused operation、backend別拡張精度を使わない。
- operationで生じたNaNはquiet NaN bit pattern `0x7ff8_0000_0000_0000`へcanonicalizeする。
- `NaN == NaN`は`false`、`NaN != NaN`は`true`とする。大小比較はすべて`false`とする。
- `-0.0 == 0.0`は`true`。deterministic binary encodingではsign bitを保持し、表示規則は言語仕様に従う。
- host responseからFloatを受け取る時もNaNをcanonicalizeし、Infinity・NaNをIntへ変換する規則は安定error codeで固定する。
- Floatのparse、format、serializeはlocale非依存の同一algorithmをtree/VM/record-replayで共有する。
- 上記のNaN canonicalization・parse・format・deterministic bit化はbackend間で1つのshared helperを共有し、backendごとに別実装を持たない。

### 4.6 error

内部Rust型名やdebug表示を規範errorへ埋め込まない。構造化errorは少なくとも次を持つ。

```rust
pub struct StableError {
    pub code: ErrorCode,
    pub script_type: String,
    pub message_id: ErrorMessageId,
    pub script_message: String,
    pub module_id: ModuleId,
    pub line: u32,
    pub column: Option<u32>,
    pub trace: Vec<StableFrame>,
}
```

`code`、`script_type`、`message_id`、source位置、trace frame順をbackend間で一致させる。`script_message`も`try` / `catch`から観測可能なため同じ日本語templateと値serializationを共有する。host向けlocalized displayは別層とし、determinism比較とaudit schemaへ使わない。

budget、deadline、cancel、audit failure、internal invariant、replay mismatchはhost control-plane terminalであり、script-visible `Value::Error`へ変換しない。

`ErrorCode`は独立enumを作らず`ErrorKind`の別名とする（`ExecutionError.code`と同じ型）。`ErrorMessageId`はcanonical messageのtemplate 1個に1 IDを採番する（per-template）。内部errorからの射影は1つの関数で行い、安定型文字列・canonical message・traceを二重管理しない。uncatchable terminal（`ErrorKind::is_uncatchable`が判定の正本）は`StableError`へ射影しない。

### 4.7 function identity

AUD-048は、規範言語仕様どおり「同じ動的function instanceだけが等しい」で解消する。

- function/lambda式を評価するたびに新しい`FunctionId`を発行する。
- 値clone、変数代入、collection格納、captureは同じIDを保持する。
- 同じsource位置・同じchunk・upvalueなしでも、別評価なら別IDとする。
- treeは現在のobject identityをこのIDへ明示化し、VMはclosure生成opcodeごとに新IDを付与する。
- IDはexecution内の比較専用で、scriptとaudit eventへ生値を公開しない。`ContextSnapshot`および暗号化・access controlされたrecord storeでは、同一executionのDAG identityを再現するlocal IDとして保存できるが、execution外の永続identityや認可tokenとして扱わない。

### 4.8 compileの資源と失敗

VMのcompileも、script由来の入力に対してhostを止めない処理として扱う（issue #292 の受け入れで追加）。

- compileはどのASTに対してもpanicしない。引数の個数や形が誤った呼び出しでも、compileは失敗せず、[言語仕様](language-spec.md)「コンテキスト依存組み込み関数の呼び出し順序」「ユーザー定義関数の呼び出し順序」のとおり、到達したときにtreeと同じruntime errorを出すbytecodeを生成する。
- compileが生成するbytecodeの量（命令数と定数の数）は、ASTのnode数に対して線形に抑える。1つのsource式のbytecodeを、入れ子の深さに応じて何倍にも複製しない。
- VMは、treeが受け付けて実行するプログラムを、compile時の資源の都合（bytecodeの大きさなど）を理由に拒否しない。compile前に拒否してよいのは、両backendに共通の上限（[言語仕様](language-spec.md)「構文・AST深度制限」、source budget）だけである。backend固有のcompile上限を置くと、同じscriptの成否がbackendで変わり、§2.1の適合方針に反する。

## 5. 注入hostとfake host

ambientなOS clock、process env/argv、stdin/stdout、filesystem、process exitへbackendから直接接続しない。[Capability仕様](capability-model.md)の個別adapterとcall contextを唯一の外部入口とする。以下の`HostBoundary`はそれらを束ねるengine内部dispatcherの擬似型であり、Capability仕様の公開trait群を置換したり第2のregistryを作ったりしない。Phase 2同期adapterは`Ready`へwrapし、Phase 4のcooperative extensionだけが`Pending`を返せる。

```rust
pub trait HostBoundary {
    fn resolve_module(&self, request: ModuleRequest, cx: HostCallContext)
        -> HostCallPoll<SourceBlob>;
    fn read_input(&self, request: InputRequest, cx: HostCallContext)
        -> HostCallPoll<InputEvent>;
    fn write_output(&self, request: OutputRequest, cx: HostCallContext)
        -> HostCallPoll<()>;
    fn wall_clock(&self, request: ClockRequest, cx: HostCallContext)
        -> HostCallPoll<WallClockValue>;
    fn call(&self, request: HostFunctionRequest, cx: HostCallContext)
        -> HostCallPoll<HostValue>;
}
```

`HostCallContext`はexecution ID、call ID、frozen capability、deadline、cancellation token、request/response残量、audit correlationを含む。providerはprocess-global stateを裏で参照してよいが、その観測結果をresponseとして明示し、record可能にする。

`FakeHost`は全providerをmemory上のqueue/mapで実装し、次を提供する。

- immutable module map、env、args
- scripted input/clock/host response/error列
- output、filesystem相当操作、業務operationのeffect log
- responseを返すmonotonic tickとcancel barrier
- requestの完全一致検査と未消費response検査

unit testとdifferential testは原則`FakeHost`を使い、実OS providerはadapter integration testだけで扱う。

## 6. Record / replay

record/replay logはaudit logと別物である。auditはredactedな説明記録であり、replayに必要なsecret/bodyを欠くため、audit streamからreplayしてはならない。

```rust
pub struct RecordIntent {
    pub index: u64,
    pub call_id: u64,
    pub operation: HostOperation,
    pub request_fingerprint: [u8; 32],
    pub prepared_at: MonotonicTick,
}

pub struct RecordCompletion {
    pub response: Option<RecordedHostResponse>,
    pub effect_status: EffectStatus,
    pub wall_timestamp: HostTimestamp,
    pub completed_at: MonotonicTick,
    pub disposition: RecordDisposition, // Finished | Cancelled | Detached
}

pub struct RecordToken { pub index: u64, pub nonce: u128 }

pub trait RecordStore: Send + Sync {
    fn prepare(&self, intent: &RecordIntent) -> Result<RecordToken, RecordStoreError>;
    fn finish(
        &self,
        token: &RecordToken,
        completion: &RecordCompletion,
    ) -> Result<(), RecordStoreError>;
}

pub struct ReplayEntry {
    pub intent: RecordIntent,
    pub completion: RecordCompletion,
}
```

`prepare`と`finish`はreturn前にdurableでなければならない。storeはindexを一意にし、同じtokenへの同一内容のretryをidempotent成功、異なる内容をprotocol errorとする。crash recoveryで`prepare`だけが残るentryは`effect_status = Unknown`相当であり、自動replayせずhost operatorの解決を要求する。

### 6.1 record

- host境界へ渡す直前のcanonical requestからfingerprintと`RecordIntent`を作り、`prepare`成功後だけ外部callを開始する。
- capability decision、raw response/error、effect status、byte数、clockを`RecordCompletion`へ保存し、`finish`成功後だけresponseをscriptへ公開する。
- cancelが外部効果開始前なら`Cancelled/None`、開始後に完了不明なら`Detached/Unknown`でfinishする。
- `prepare`失敗は外部call回数0の`RecordFailure`、外部call後の`finish`失敗は`RecordFailure { host_effects_may_remain: true }`でfail-closedにする。
- replayに必要なcredential/bodyを含み得るため、record storeはhostが暗号化・access control・retentionを実装する。

### 6.2 replay

- 実host effectを実行せず、次entryの`intent.operation` / `call_id` / `request_fingerprint`が一致した場合だけcompletionを返す。
- `Finished`以外、`effect_status = Unknown`、response必須operationのresponse欠落、不一致、entry不足、余剰entry、byte数不一致はcatch不能`ReplayMismatch` terminalとする。
- outputも実streamへ書かず、recordされたrequestと比較してeffect logへ追加する。
- wall/monotonic clockはentryから返す。scheduler sliceの違いでresponse順を変えない。
- replay中もcapabilityとbudgetを再評価し、record時より厳しいpolicy/budgetならその時点でdeny/terminalにする。recordを権限bypassに使わない。

同じexecution ID、AuditClock、redaction keyを注入すればaudit envelopeもbyte単位で再現できる。通常はそれらを変え、semantic event種別・payload・sequenceだけを比較する。

## 7. Audit schema

### 7.1 envelope

field名、型、意味をschema version 2として固定する。version 1との違いは、`ExecutionStarted`と`BudgetConfig`が`heap_accounting_revision`の直後に`fuel_accounting_revision: u32`を持つことだけである（[実行予算・協調実行仕様](execution-control.md) §14「F-2 の sub-slice 分割」の判断Q4=A。判断役が選択（PR #470））。理由: field の追加は§9.2のとおりschema versionを上げ、同じ版で field の並びが違う記録を出さない（[マニフェスト](manifesto.md) 原則 7・5）。

```rust
pub struct AuditEnvelope {
    pub schema_version: u16,          // 常に2
    pub execution_id: ExecutionId,   // host供給128-bit ID
    pub source_hash: [u8; 32],        // root source SHA-256
    pub language_revision: String,
    pub sequence: u64,                // 0始まり、gapなし
    pub timestamp: HostTimestamp,     // 注入AuditClockのunix ns
    pub event: AuditEvent,
}

pub struct HostTimestamp {
    pub unix_nanoseconds: i128,
}
```

`timestamp`は注入host clockの値であり、順序の正本ではない。wall clockが後退しても書き換えず、`sequence`で順序を決める。replay時はrecord値を使う。`execution_id`はhostが一意に生成して渡し、engineがambient random sourceへ直接アクセスしない。

sequenceはStartedが0で、eventをlogical audit journalへappendする時にchecked incrementする。overflowは`AuditFailure::SequenceExhausted`でfail-closedとし、wrapしない。

### 7.2 event enum

```rust
pub enum AuditYieldReason {
    AdmissionQueued { resume_to: AdmissionPhase },
    SliceFuelExhausted,
    ExplicitYield,
    HostCallPending { call_id: u64 },
    AuditBackpressure,
    SchedulerPreempted,
    HostPaused,
}

pub enum AuditEvent {
    ExecutionStarted {
        engine_version: String,
        backend: Backend,                 // productionはTree
        rules_revision: u32,
        heap_accounting_revision: u32,
        fuel_accounting_revision: u32,
        budget: BudgetConfig,
        capability_policy_hash: [u8; 32],
        redaction_policy_id: String,
        mode: ExecutionMode,                // Live | Record | Replay
    },
    CapabilityDecision {
        operation_id: u64,
        call_id: Option<u64>,
        capability: String,
        action: String,
        resource: AuditPayload,
        decision: CapabilityDecisionKind,   // Allow | Deny
        rule_id: String,
    },
    HostCallStarted {
        operation_id: u64,
        call_id: u64,
        function: String,
        request: AuditPayload,
        request_bytes: u64,
    },
    HostCallFinished {
        operation_id: u64,
        call_id: u64,
        outcome: HostCallOutcome,
        response: AuditPayload,
        response_bytes: u64,
        error_code: Option<String>,
        effect_status: EffectStatus,        // None | Committed | Unknown
    },
    BudgetCharged {
        resource: BudgetResource,
        committed_delta: u64,
        released_delta: u64,
        usage_after: u64,
        peak_after: Option<u64>,
        charge_count: u64,
        reason: BudgetChargeReason,
    },
    Yielded {
        reason: AuditYieldReason,
        poll_index: u64,
        usage: BudgetUsage,
    },
    Resumed {
        previous_reason: AuditYieldReason,
        poll_index: u64,
        usage: BudgetUsage,
    },
    Terminal {
        outcome: TerminalOutcome,
        error: Option<AuditErrorPayload>,
        usage: BudgetUsage,
        import_graph_hash: Option<[u8; 32]>,
        context_committed: bool,
        host_effects_may_remain: bool,
    },
}
```

```rust
pub enum ExecutionMode { Live, Record, Replay }
pub enum CapabilityDecisionKind { Allow, Deny }
pub enum HostCallOutcome { Success, Denied, HostError, Cancelled, Detached }
pub enum EffectStatus { None, Committed, Unknown }
pub enum BudgetChargeReason {
    Statement,
    Expression,
    Operation,
    Function,
    Loop,
    HostCall,
    BulkElements,
    BulkBytes,
    Allocation,
    Source,
    Input,
    Output,
}
pub enum TerminalOutcome {
    Completed,
    Exited(u8),
    Denied,
    LinkError,
    RuntimeError,
    HostError,
    BudgetExceeded(BudgetResource),
    DeadlineExceeded,
    Cancelled,
    AuditFailure,
    InternalFailure,
    RecordFailure,
    ReplayMismatch,
}
pub struct AuditFrame {
    pub function: String,             // 最大256 UTF-8 bytes
    pub module_id: AuditField,        // Identifier/Path policy適用済み
    pub line: u32,
}
pub struct AuditError {
    pub code: String,                 // 最大128 UTF-8 bytes
    pub message_id: String,           // 最大128 UTF-8 bytes
    pub module_id: Option<AuditField>,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub trace: Vec<AuditFrame>,       // 最大32 frames
    pub omitted_trace_frames: u32,
}
pub enum AuditErrorPayload {
    Full(AuditError),
    Emergency {
        code: String,                 // 最大128 UTF-8 bytes
        message_id: String,           // 最大128 UTF-8 bytes
        full_error_bytes: u64,
        full_error_sha256: [u8; 32],
    },
}
```

各normal audit eventのencoded長は最大60 KiBとする。redaction後payloadが超える場合、Body/Path/Identifierの順に`Omitted`へ縮退し（classごとにまとめて行う。まずevent内のBodyをすべて`Omitted`にして測り直し、なお超えればPathをすべて、次にIdentifierをすべて`Omitted`にする。§14「audit-strict-cbor-c4 実装スライス」の判断Q4）、それでも超えるeventは`AuditFailure::EventTooLarge`とする。Terminalだけは失敗を再帰させず、`Full(AuditError)`を60 KiB以内にできない場合に`Emergency`へ置換する。Emergencyはredaction済みfull errorのbyte長とSHA-256だけを持ち、Terminal全体を4 KiB以下にする。したがって64 KiB Terminal reserveで必ずencodeできる。traceは先頭32 frameだけを保持し、残数を`omitted_trace_frames`へ記録する。

`ExecutionOutcome`と`TerminalOutcome`は上の全variantを1対1に対応させる。APIの`Exited { code, usage }`は`TerminalOutcome::Exited(code)`、`InternalFailure`は同名へ写像する。Phase 1/2内部bootstrapに別名の`InternalFault` terminal、Phase 3/4に別のusage型、Phase 6に別event enumを残さない。raw Rust error、backtrace、source本文をpayloadへ入れない。

operation IDとcall IDはexecution内で0から単調増加し、overflow時はfail-closedにする。1 host call requestは同じ`operation_id` / `call_id`を`HostCallStarted`、`CapabilityDecision`、`HostCallFinished`で使う。host call以外のmodule/path等のdecisionは`call_id = None`を許す。

### 7.3 BudgetChargedの集約

各statementごとのaudit eventで監査自体をDoSにしないよう、budget chargeはresourceごとに集約する。

- 同じpoll内の同resource・同reasonをaccumulatorへ足す。
- host call開始前、yield前、pause前、terminal前、または256 chargeごとにflushする。
- `charge_count`は集約した論理charge回数、`committed_delta`は合計、`usage_after`はflush後の累積値である。
- heap releaseは`released_delta`へ記録し、`usage_after`をlive heap、`peak_after`をpeak heapとする。
- Terminalのusageと全BudgetChargedのdeltaをcumulative/live resourceで突合し、per-item resourceは`BudgetPeaks`を突合できなければならない。

## 8. event順序と完全性

1 executionのlogical streamは次を満たす。

1. `ExecutionStarted`は必ずsequence 0に1件だけある。
2. root scriptのsemantic statementより前にStartedをjournalへappendし、sinkがackする。
3. import resolverを含むhost requestはlifecycle 3 event分をatomic reserveし、`HostCallStarted`、`CapabilityDecision`、`HostCallFinished`の順にする。
4. denyでもStarted/Finishedを両方出し、Finished outcomeを`Denied`、effectを`None`にする。
5. allowのdecisionがsinkにackされる前に外部効果を開始しない。
6. HostCallFinishedがackされる前にresponseをscriptへ公開しない。
7. budget accumulatorをflushしてからYielded/Terminalを出す。
8. resume後の最初のsemantic workより前にResumedを出す。
9. `Terminal`は1件だけで最後のeventである。Terminal append後はいかなるeventも発行しない。
10. Startedのあるexecutionには、sink delivery成否にかかわらずlogical journal上のTerminalが必ず1件ある。

compile自体がrootのparse errorを返し、`ExecutionHandle`を作成できなかった場合はexecution auditの対象外とする。handle作成後のlink/import/capability/budget失敗はStartedとTerminalを持つ。

pauseは`Yielded { reason: AuditYieldReason::HostPaused }`として記録し、resume時に`Resumed { previous_reason: HostPaused }`を出す。script executionの公開`YieldReason`へ`HostPaused`は追加しない。

## 9. Redaction policy

### 9.1 field class

```rust
pub enum AuditFieldClass {
    Public,
    Identifier,
    Path,
    Body,
    Secret,
}

pub enum RedactionMode {
    Reveal,
    HmacSha256,
    Omit,
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub enum AuditDirection { Request, Response }

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct AuditFieldKey {
    pub operation: String,
    pub direction: AuditDirection,
    pub field: String,             // nameまたは0始まりargument index
}

pub enum AuditScalar {
    Null,
    Bool(bool),
    Int(i64),
    Float(u64),             // canonicalized binary64 bits
    Utf8(String),
    Bytes(Vec<u8>),
}

pub enum AuditValue {
    Scalar(AuditScalar),
    Sequence(Vec<AuditField>),
    Object(BTreeMap<String, AuditField>),
}

pub struct AuditPayload {
    pub fields: BTreeMap<String, AuditField>,
}

pub enum AuditField {
    Revealed {
        class: AuditFieldClass,
        value: AuditValue,
        original_bytes: u64,
    },
    HmacSha256 {
        class: AuditFieldClass,
        digest: [u8; 32],
        original_bytes: u64,
    },
    Omitted {
        class: AuditFieldClass,
        original_bytes: u64,
    },
}

pub struct RedactionPolicy {
    pub policy_id: String,
    pub identifier_mode: RedactionMode, // default Reveal
    pub path_mode: RedactionMode,       // default HmacSha256
    pub body_mode: RedactionMode,       // default Omit
    pub reveal_body_fields: BTreeSet<AuditFieldKey>,
    pub hmac_key: SecretBytes,
    pub max_revealed_body_bytes: u64,   // default 0
}
```

canonical CBORでは`AuditField`のvariantを`0=Revealed`、`1=HmacSha256`、`2=Omitted`、classは表の順を0始まりのtagとし、fieldを持たないenumとして9.2節の一般規則どおり`[tag]`でencodeする（bare integerにしない。`AuditField`のvariantはfieldを持つので常に`[tag, field...]`であり、ここで定めるのはtagの値だけ。§14「audit-strict-cbor-c1 実装スライス」の判断Q1）。`AuditPayload.fields`と`AuditValue::Object`はUTF-8 key byte順、Sequenceはargument/element順を維持する。host call引数は`fields["arguments"] = Sequence(...)`、operation固有metadataは別keyにする。各sequence element/object valueを独立した`AuditField`にするため、argument indexやnested fieldごとにredactionできる。nestingは64段までとし、それ以上のsubtreeはBody/Omittedへ縮退する。

`original_bytes`はredaction前のcanonical subtree payload長であり、omit/hash後も保持する。redaction前subtreeは、子fieldをすべて`Revealed`（各子のclassはそのまま）とし、子の`original_bytes`も同じ規則で下から再帰的に求めた値として9.2節のencodingで表し、その長さを測る。ただし`AuditValue`へ写せない子は`Omitted{class, original_bytes: 0}`のまま置く（要素は落とさず、Sequenceの長さとargument indexを保つ）。host call引数のうち`AuditValue`へ写せない値（型エラーで文字列でない引数など）は`original_bytes = 0`の`Omitted`とする（§14「audit-strict-cbor-c3 実装スライス」の判断Q2・Q3）。Secret classの親は子を走査・記録せず必ずsubtree全体を`Omitted`、Pathの既定は`HmacSha256`、Bodyは`body_mode == Reveal`かつkeyが`reveal_body_fields`にあり上限内の場合だけ`Revealed`となる。

| class | 例 | 既定 |
|---|---|---|
| Public | event種別、ID、hash、language revision、budget、count、error code | raw |
| Identifier | capability名、host function名、env key、rule ID、module logical name | raw。ただしhostがhash/omitへ変更可 |
| Path | filesystem path、module source location、URL resource identity | HMAC-SHA-256。rawを記録しない |
| Body | source、stdin/stdout、file/network body、host args/result、errorの自由文 | omitし、型・byte数・必要ならHMACだけ |
| Secret | credential、token、password、secret env value、redaction key | 常にomit。mode変更不可 |

`Secret<T>`を型で区別し、`Debug` / `Display` / generic serializerを実装しない。secretの存在とbyte数だけをPublic metadataとして記録できる。

BodyをRevealできるのは、policyのallow-listで特定host function fieldを`audit_safe`と明示し、`max_revealed_body_bytes > 0`の場合だけである。上限と比べる長さは`original_bytes`（CBOR header込みのencode長）とする。上限を超えたbodyはtruncationせず全体をomitし、`truncated = true`ではなく`omitted = true`とbyte数を記録する。prefixだけを残してsecretを漏らさない。

pathはproviderが認可に使ったcanonical resource labelのfield payload全体（labelを値とする`AuditValue::Scalar(AuditScalar::Utf8(label))`の9.2節のencoding）へHMAC-SHA-256を適用する。host absolute pathをlabelへ含めない。単純SHA-256は辞書攻撃に弱いため使わない。deny eventでもraw requested pathを出さない。

error messageはpath、argument、host responseを含み得るため、auditでは安定`ErrorCode`、`message_id`、source位置、redacted parameter summaryだけを保存する。script-visible messageをそのまま保存しない。

`HostCallFinished.error_code`は安定`ErrorCode`文字列（canonical errorのkind。例: `capability`、`host`）である。ただし、canonical messageが閉じた`{category}`の列挙を持つ`host` error（[意味論の決定](semantic-decisions.md) §16.12 のHTTP adapter）では`host.{category}`（例: `host.header_limit`）とし、停止理由のcategoryを残す。`{category}`はspecで列挙を閉じた値でOS/libraryの生messageを含まないので、安定codeだけを残す本節の規則のままである。`error_code`は`AuditPayload`ではない`Option<String>`でredactionにより縮退しないため、payloadが`Omitted`になっても停止理由が監査に残る。field・宣言順・encoding（§7.2・§9.2）は変えず、schema versionも上げない。§7.1がschema version 1として固定する`error_code`の意味は「script可視messageを含まない安定`ErrorCode`文字列」であり、`host.{category}`は未実装のHTTP adapterの`host` errorにだけその範囲で値を足すもので、既存の記録の値と意味は変わらないので、§7.1の固定と両立する。loop:drift #395 の D-97（issue #406）を反映するPR #415の判断の候補Q2=A（判断役が選択（PR #415））。理由: 停止理由が必ず監査に残り、先頭の`host`でkindも読める（[マニフェスト](manifesto.md) 原則 7）。既存のfieldのままで済む（原則 8、優先順位の表「小さく監査可能な中核」）。採らなかった案B（`HostCallFinished`にcategoryのfieldを足す）は、field名・型・意味をschema version 1として固定している（§7.1）ためschema versionを上げることになり（§9.2の「unknown schema fieldを末尾へ追加せず、schema versionを上げる」と同じ向き）、着手gateが閉じている未実装のHTTP adapterのためにaudit schemaを広げるため採らない。案C（`response`の`AuditPayload`へ入れる）は、payloadがredactionで`Omitted`へ縮退しうるので停止理由が読めないことがあり（原則 7 に反しうる）、採らない。

### 9.2 Canonical encodingとrequest fingerprint

schema version 2（§7.1）のbyte表現はRFC 8949 deterministic CBORの次のsubsetへ固定する。

- structはRust field宣言順のCBOR array、enumは`[u16 tag, field0, ...]`、`Option`は`[0]`または`[1, value]`でencodeする。mapとしてfield名をencodeしない。
- unsigned/signed integerは値を表せる最短幅、Stringは正規化しないUTF-8 text、byte列はCBOR byte string、bool/nullは標準primitiveとする。Rustの符号なし整数（u8〜u64）はmajor type 0、符号付き整数（i64）は0以上ならmajor type 0、負ならmajor type 1とし、宣言型の幅ではなく値で最短幅を選ぶ。array・text・byte stringは常に定長（definite length）で、indefinite-length encodingを使わない。CBOR tag（major type 6）は使わない。
- enumのvariantは、fieldなしなら`[tag]`、tuple variantなら`[tag, field0, ...]`、struct variantなら`[tag, field...]`（7.2節・9.1節の宣言順）とする。tagは各enumの宣言順を0始まりで振る。
- `HostTimestamp.unix_nanoseconds`は16 byte two's-complement big-endian byte string、Floatはcanonicalized binary64 bitsの8 byte big-endian byte stringとし、CBOR float型を使わない。
- `BTreeMap`は`[[key, value], ...]`としてUTF-8 key byte順、`BTreeSet`もsort済みarray、Vec/sequenceは元順でencodeする。
- `BudgetConfig`、`BudgetCounters`、`BudgetUsage`は[実行予算・協調実行仕様](execution-control.md)のfield宣言順を使う。unknown schema fieldを末尾へ追加せず、schema versionを上げる。
- `AuditEnvelope`は`[schema_version, execution_id(16 bytes), source_hash(32 bytes), language_revision, sequence, timestamp, event]`である。`execution_id`はu128を16 byte big-endianのbyte string、`[u8; 32]`のhash/digestは32 byteのbyte stringとする。
- `MonotonicInstant`（`BudgetConfig.deadline`）は[実行予算・協調実行仕様](execution-control.md)のとおりopaqueなu64 nanosecond tickとしてu64でencodeする。clock domainを区別するための実装内部の識別子（clock instanceごとに異なる値）はencodeしない。実行ごとに変わる値をencodeすると§15.1の決定性を壊すため。
- `AuditEvent` tagは`0=ExecutionStarted`、`1=CapabilityDecision`、`2=HostCallStarted`、`3=HostCallFinished`、`4=BudgetCharged`、`5=Yielded`、`6=Resumed`、`7=Terminal`とする。payload fieldは7.2節の宣言順である。
- `AuditField` / class / direction / nested valueのtagは9.1節の宣言順を0始まりで使う。HMAC対象と`original_bytes`はredaction前subtreeのこのencodingである。

record用`RecordedHostRequest`は、operationのstable ID、host function IDまたはcapability operation ID、language revision、capability set ID、型tag付きraw argument/resource payloadを持つ。ValueはNull/Bool/Int/Float/String/List/Dict/Errorの順を0始まりtagとし、Listは要素順、Dictはkey UTF-8 byte順、Floatは上記8 bytes、String/Bytesは長さ付きCBORでencodeする。execution ID、call ID、timestamp、worker、redaction結果は含めない。

```text
fingerprint_input = "TSUMUGI-HOST-REQUEST-V1\0"
                  || deterministic_cbor(RecordedHostRequest)
request_fingerprint = SHA-256(fingerprint_input)
```

domain separatorは末尾NULを含むASCII生byteである。record storeはraw canonical requestまたは再計算に必要な暗号化payloadを保持し、replayは同じ共有encoderでfingerprintを計算する。audit encoder、record encoder、予算byte counterが別実装のsize推定を持つことを禁止する。

## 10. Audit sink契約

```rust
pub trait AuditSink: Send + Sync {
    fn submit(
        &self,
        batch: Arc<[AuditEnvelope]>,
        waker: AuditWaker,
    ) -> AuditSubmit;
}

pub enum AuditSubmit {
    Ack(AuditAck),
    Pending(AuditTicket),
    Failed(AuditSinkError),
}

#[derive(Clone)]
pub struct AuditWaker { /* Arc<WakeState>, Send + Sync */ }
impl AuditWaker { pub fn wake(&self); }

pub struct AuditTicket { /* private, Send + Sync */ }

impl AuditTicket {
    pub fn register_waker(&self, waker: AuditWaker);
    pub fn try_take(&self) -> Option<Result<AuditAck, AuditSinkError>>;
    pub fn cancel(&self);
}

pub struct AuditAck {
    pub execution_id: ExecutionId,
    pub through_sequence: u64,
}
```

- `Pending(ticket)`ならexecutionを`Yielded(AuditBackpressure)`にする。sinkはack/errorをticketへ一度だけ格納して`AuditWaker`をwakeし、作成thread上の次回`ExecutionHandle::poll`が`try_take`して再開する。Pending batchの`Arc`はack/errorまで不変で、再送も同一byte列を使う。
- cancel/drop時はticketへ取消要求を送る。ただし未ack eventを捨てず、logical journalまたはbounded orphan-audit queueがdeliveryを引き継ぐ。
- sinkは`(execution_id, sequence)`をidempotency keyとして重複を除去する。
- ackは連続sequenceだけを認め、gap、別execution ID、未送信sequenceのackをsink protocol errorとする。
- deliveryはat-least-once、logical eventはexactly-onceである。ack喪失時は同じenvelopeを再送する。
- 1 execution内のbatchとevent順序を維持する。異なるexecution間の順序は保証しない。
- sink callback中に同じEngineのcompile/start/poll/pause/resume/cancel、host function、別audit emitを呼んではならない。thread-local reentrancy guardで検出し`AuditSinkError::Reentrant`にする。
  - 対象の入口: start/poll/pause/resume/cancelはschedulerの`ExecutionHandle`の操作を指す。sinkを持つfacadeの`Engine`では、`compile`・`link`・`run`・`run_for_repl`・`run_audited`の5入口を対象にする（startとpollはrun系3入口の中にあり、pause/resumeの入口は無い）。host functionと別audit emitはrun系の入口を通らなければ呼べないため、この5入口で覆う。`CancellationToken::cancel`は任意threadから呼べるflag（[組み込みAPI仕様](embedding-api.md)第7節・第9節）でEngineの入口ではないため、callbackから呼んでよい。`ExecutionHandle`の操作で拒否に返す値は、sinkをscheduler経路へ配線するとき（E10統合）に決める。
  - 再入した呼び出しへ返す値: 何も実行せず、既存のInternalFailure系で拒否する。公開型は増やさない。`run`・`run_for_repl`は`ExecutionOutcome::InternalFailure`、`run_audited`は`AuditedOutcome::Outcome(InternalFailure)`、`compile`は`CompileDiagnosticCode::InternalFault`の診断1件の`CompileErrors`、`link`は`LinkError::InternalFailure`を返す。渡されたcontextは変えず、poisonもしない。
  - 外側のexecution: guardは「このthreadで今sink callbackの中にいるEngineの列」を持ち、再入の印を列の要素ごとに持つ。再入を拒否したら、再入されたEngineの要素にだけ印を立てる。`submit`から戻ったとき自分の要素に印があれば、sinkが返した値（`Ack`を含む）を捨てて`Failed(AuditSinkError::Reentrant)`として扱い、§10.1のとおり`AuditFailure::Sink`でfail-closedにする（`AuditFailure`にvariantは足さない）。
  - 入れ子: Engine Aのcallbackの中で別のEngine Bを実行し、Bのcallbackから Engine Aへ再入した場合、fail-closedになるのはAのexecutionだけで、Bのexecutionは通常どおり進む。Aへの再入は、Aのcallbackを出るまで（Bのcallbackを出たあとも）拒否する。別のEngineをcallbackから呼ぶこと自体は許す。
  - 検出はthread-localであり、別threadからの呼び出しは対象外とする。engine lockを保持したままsinkへ入らない（次項）ため、別threadからの呼び出しでdeadlockしない。
- engine/scheduler/context lockを保持したままsinkへ入らない。

### 10.1 fail-closedを既定とする

安全性を優先し、初期実装のpolicyは`AuditFailurePolicy::FailClosed`だけとする。fail-open optionは提供しない。

engineはsinkの前にper-execution bounded logical journalへeventをappendする。Started時にTerminal 1件分のevent-count slotと64 KiB emergency byte slotを予約し、通常eventはこの領域を使えない。

- Startedがackされるまでscript/import host callを開始しない。
- permanent sink error、protocol error、audit budget超過では新しいscript workを停止する。
- pending host callを`Cancelled`または`Detached`のHostCallFinishedで論理的に閉じる。
- script内状態を[実行予算・協調実行仕様](execution-control.md)どおりrollbackする。
- emergency slotへ`Terminal(AuditFailure)`をappendし、その後のevent生成を禁止する。
- callerにはTerminalがjournalへappendされた後でのみ`AuditFailed`を返す。sinkが回復すればjournalを再送できる。

process OOM等で事前予約したTerminal領域すら使えない状況はhost process障害であり、言語level保証外である。通常のbudget/backpressure/sink error経路ではStart/Terminal各1件を必ず維持する。

### 10.2 backpressure

各executionのaudit drain queueは最大1 MiB、engine全体は最大16 MiBとする。上限へ達したらsemantic executionを進めずAuditBackpressureでyieldする。queueを捨てたり古いeventを上書きしない。

backpressure中もexecution deadlineとcancelを監視する。deadline/cancel時は通常event領域を使わず、未flush budgetを既存bufferへ可能な範囲で統合し、予約済みTerminalをappendする。sinkが永久に`Pending`のままdeadlineへ達した場合のoutcomeは`AuditFailure::BackpressureDeadline`とし、scriptのDeadlineより優先する。これにより「実行結果は成功だが監査が欠ける」状態を作らない。

## 11. 監査自体の予算

監査はscriptのfuel、output、host-call budgetへ課金しない。監査eventがさらにBudgetChargedを発生させる再帰を避ける。代わりに独立したcontrol-plane budgetを使う。

```rust
pub struct AuditBudget {
    pub max_events: u64,                    // default 100,000、2以上
    pub max_encoded_bytes: u64,             // default 16 MiB
    pub max_pending_bytes: u64,             // default 1 MiB / execution
    pub terminal_reserve_events: u64,       // fixed 1
    pub terminal_reserve_bytes: u64,        // fixed 64 KiB
    pub host_call_close_reserve_events: u64,// fixed 2
    pub host_call_close_reserve_bytes: u64, // fixed 128 KiB
}
```

encoded byte数は9.2節のdeterministic CBORでencodeした実byte長とする。encoding前に同じencoderのsize passで最大長をchecked arithmeticによりreserveし、write passの長さが一致しなければinternal audit failureとする。

- StartedとTerminalもevent count・encoded bytesへ含める。Started append時にevent 1件と64 KiBをTerminal専用にreserveし、通常eventは`committed_events + requested_events <= max_events - terminal_reserve_events - active_close_reserve_events`かつ通常byte残量内の場合だけappendできる。
- host call開始前に、`HostCallStarted`自身の通常1 event/最大60 KiBと、後続`CapabilityDecision` + `HostCallFinished`用の2 events/128 KiBをatomicにreserveする。予約できなければHostCallStartedも外部callも行わずAuditFailureにする。Decision/Finishedはclose reserveを消費し、pair完了時に未使用byteをrefundする。1 executionの未完了host callは1個だけなのでclose reserveも最大1組である。
- `max_events < 2`、`terminal_reserve_events != 1`、`terminal_reserve_bytes < 64 KiB`、`host_call_close_reserve_events != 2`、`host_call_close_reserve_bytes < 128 KiB`はconfig errorとする。Terminalは専用event/byte reserveだけを使い、通常上限を使い切っても必ずappendできる。低いmax_events/bytesによりhost lifecycle 3 eventsを予約できないconfigは構築可能だが、host callを効果開始前のAuditFailureにする。`max_events`の下限は、どのexecutionも必ず持つStartedとTerminalの2件である（上の`AuditBudget`定義のコメントが「4以上」としていたのを「2以上」へ直した。loop:drift #263 D-10。この節の`max_events < 2`と実装〔`AuditBudget::validate`〕を正にする案〔Q3=A〕は返信なしのため推奨案で確定、[開発ループ手順書](loop-runbook.md) §7.5）。理由: 3 events を予約できない低い`max_events`もhost callをAuditFailureにする形で扱いが決まっており、下限を4にする理由が無い。
- 上限超過は`AuditFailure::BudgetExceeded`でfail-closedにする。
- BudgetChargedを前節どおり集約し、監査event数をscript statement数へ単純比例させない。
- hashing、HMAC、encodingはhost control-plane workでfuelへ課金しないが、入力sizeはsource/I/O/host budgetで既に有限でなければならない。

## 12. Record/replayとauditの関係

| 項目 | audit | record/replay |
|---|---|---|
| 目的 | 誰が何を許可・実行し、どう終了したかの説明 | 外部responseを再現して同じ実行を得る |
| body | 既定omit/redacted | replayに必要なら暗号化して保存 |
| sink失敗 | fail-closed | record時fail-closed、replay時は事前読込失敗 |
| 順序key | execution ID + sequence | entry index + call ID |
| host effect | 要約とeffect status | request fingerprintとraw response |

record entryとaudit host callは同じcall IDを使う。`RecordStore::prepare`をdurableにし、auditのCapabilityDecision ackも完了してから外部効果を開始する。外部call終了後は`RecordStore::finish`をdurableにし、HostCallFinishedをackしてからscriptへ値を返す。

replayでは実外部効果を起こさず、監査に`mode = Replay`を記録する。auditはreplay entryから再生成し、record済みのredacted audit payloadを盲目的にコピーしない。現在のredaction policyを適用する。

## 13. 指定AUD項目との関係

### AUD-019: engine固有error kind/message

Phase 5のproduction gateで解消する。StableErrorのcode、script type、message ID、script-visible message、source位置、traceをtree/VMで完全一致させる。auditはraw messageではなくcode/message IDを使うためschemaは表示文変更から分離されるが、VMのexperimental解除にはmessageを含むscript観測も一致が必要である。

### AUD-022: differential・limit境界・fuzz

既存harnessのtimeout、完全一致golden、一時directory分離を維持し、determinism tuple、host effect log、budget charge、audit streamを比較対象へ追加する。型の組合せだけでなく、同一source位置から複数instanceを作る生成履歴、pause/resume、cancel race、record/replay、sink failureをmatrix/fuzzへ含める。

### AUD-024: import・REPLの状態commit

[実行予算・協調実行仕様](execution-control.md)で方針を確定した。Completed/Exitedで終了したexecutionだけscript stateをcommitし、`Denied`、未捕捉runtime error、`HostError`、budget、deadline、cancel、audit/record/replay/internal failureはexecution開始時点へrollbackする。execution中にcatchされ最終的にCompleted/Exitedへ到達した通常errorはcommit側である。既にcommitした外部host effectはrollbackせず、Terminalへ`host_effects_may_remain`を記録する。tree/VMは同じmutation journal契約を使う。

`host_effects_may_remain`は、そのexecutionに`HostCallFinished.effect_status`が`Committed`または`Unknown`のhost callが1件でもあれば、outcomeにかかわらず（Completed/Exitedでもrollbackする終了でも）`true`とし、無ければ`false`とする。`context_committed`とは独立のflagであり（[組み込みAPI仕様](embedding-api.md) §10の6）、Startedのack失敗などscript workを行わずに終わるTerminalは`false`である。loop:drift #342 の D-51 Q1=A（判断役が選択（issue #342））。理由: 規則が1つでjournal（`HostCallFinished.effect_status`）だけから決まり（[マニフェスト](manifesto.md) 原則 8）、Completed・runtime error・host callなしの3系でテストでき（原則 9）、どの外部操作を行いどう終わったかをTerminal単体で読める（原則 7）。`Unknown`を含めるのは、§6.1が外部call後の`finish`失敗を`host_effects_may_remain: true`とするfail-closedの向きに合わせるためである。採らなかった案B（`context_committed = false`のときだけ`true`）は、Completedのときの意味を別に決める判断が残り、CompletedのTerminalから外部効果の有無が読めなくなる。

### AUD-048: function identity

本文書4.7の`FunctionId`で解消する。同じsource/chunkでも動的評価ごとに別instanceとし、VMへclosure生成単位のidentity tokenを追加する。treeをcompile chunk共有単位へ寄せて別instanceを等しくする案は、現行言語仕様に反するため採用しない。

## 14. 実装slice

各スライス小節はそのスライスを実装した時点の記録であり、書き換えない。現在の逸脱・未実装範囲は冒頭の逸脱一覧を見ること。

各sliceは独立PRとし、前sliceの受入基準を満たしてから次へ進む。

### Slice 1: deterministic primitive

- `LanguageRevision`、`DeterminismRulesRevision`、StableError、ModuleId、source/import graph hashを実装
- locale/Unicode/Float/NaN/BTreeMap/path規則を共有helperへ集約
- AUD-048のFunctionIdをtreeへ導入

Float/NaNのcanonicalization・parse・formatとserialize用bit化は`src/determinism.rs`へ、`DeterminismRulesRevision`固定値と`ModuleId`のno-normalization契約は`src/embedding.rs`のshared primitiveへ集約済み。StableErrorは`src/stable_error.rs`へ実装済み（内部`TsumugiError`/`ErrorKind`からtree backendで決定的に射影する。VM parity・FakeHost gate・audit normalizeは後続slice）。これでSlice 1のdeterministic primitiveは一通り揃った。状態は[ロードマップ](roadmap.md)「ステータス一覧」を参照する。

### Slice 2: HostBoundaryとFakeHost

- clock、env、args、stdio、module resolver、exit、filesystemをambient APIから注入hostへ移す
- FakeHostのscripted response/effect logを実装
- CLIは必要capabilityと実OS adapterを明示構築する

### Slice 3: tree規範化

- 全言語fixtureをFakeHost経由にし、value/error/effect/budgetを規範化
- AUD-019とAUD-024をtreeのStableError・transactionへ反映
- import実行前解決とmodule snapshot/hashを固定

### Slice 4: record/replay

- canonical request fingerprint、encrypted store interface、strict replay matcherを実装
- clock、input、output、host error、effect statusをrecord/replayする
- mismatchをcatch不能terminalへ接続

### Slice 5: audit core

- schema version 1、logical journal、sequence、BudgetCharged集約、redactionを実装
- nonblocking sink、ack/retry、reentrancy guard、fail-closed、emergency Terminal slotを実装
- execution-controlのyield/resume/terminalへeventを接続

#### A-1 実装スライス（tree engine、opt-in）

Slice 5 の最小縦切り（A-1）を tree engine にのみ実装した時点の、仕様に対する逸脱と
未実装範囲を記録する。これは一時的な実装状態であり、仕様（本章 §1〜§13）が正本である。

- **opt-in（§1.1 からの逸脱）**: A-1 では監査 sink を `EngineBuilder::audit_sink` で設定した
  ときだけ監査が有効になる。sink 未設定時は監査 event を一切 emission せず、既存挙動と
  bit-identical。§1.1 が規定する sink 必須ポリシー（`ConfigError::AuditSinkRequired`）は、
  CLI 側の sink 構築が整う後続スライスへ延期する。
- **emission する event**: `ExecutionStarted`（sequence 0、最初の semantic work の前に ack 待ち）、
  `Terminal`（各 terminal commit 点で 1 件、最後の event）、`BudgetCharged`（§7.3 の集約、
  Started と Terminal の間）、および host-call lifecycle の 3 event（`HostCallStarted` /
  `CapabilityDecision` / `HostCallFinished`、§7.2/§8）、および yield/resume lifecycle の 2 event
  （`Yielded` / `Resumed`、§7.2 tag 5/6）。host-call 3 event は filesystem builtin
  （`read_file` / `read_lines` / `write_file` / `append_file`、tree 唯一の host-call 境界）で
  emission する。`Yielded` / `Resumed` は tree evaluator の slice 境界で emission する（下記
  「yield/resume lifecycle 2 event を tree backend で emission」）。backpressure 由来の yield も
  B-1 で facade の poll driver に配線済み（下記「`AuditBackpressure`-yield 配線」）。これで §7.2 の
  8 event 型が tree backend で emission される。
- **`capability_policy_hash` を実配線（§7.1）**: `ExecutionStarted.capability_policy_hash` には、
  実行を認可した frozen `CapabilitySet` の決定的な相関 ID（`CapabilitySet::id()` の 32 byte）を
  載せる。これにより deny-by-default（空 set）と stdout/filesystem/exit 等を grant した実行を監査上
  区別できる。`redaction_policy_id` は redaction 本体が未実装のため A-1 では安定の既定値のまま。
- **tree-only backend 境界（§2.1/§7.1）**: A-1 の run 経路には backend dispatch が無く、常に tree
  evaluator で実行する。`ExecutionStarted.backend` が実行実体と食い違わないよう、監査 sink と
  `Backend::VmExperimental` を同時に設定した engine の build は `ConfigError::AuditSinkWithExperimentalBackend`
  で拒否する。VM の監査 wiring は Slice 6（VM conformance）へ延期する。
- **fail-closed（§10.1）**: Started が ack されるまで script/import work を開始しない。ack は
  §10 の規則どおり、submit した exact な `execution_id` と連続 `sequence`（Started なら 0）を指す
  ものだけを受理し、別 execution の ack / gap / 未送信 sequence の ack は protocol 違反として
  fail-closed にする。Started の ack 失敗（`Failed` / 不正 ack / A-1 では起きない `Pending`）では
  emergency slot へ `Terminal(AuditFailure)` を append し、script work を一切行わない。実行後の
  `Terminal` の配送失敗（`Failed` / 不正 ack）も fail-closed にし、journal 上の Terminal は確定
  済み（§8 規則10）でも監査は成立していないため success な outcome を返さない。いずれの場合も
  監査付き入口 `Engine::run_audited` は success ではない `AuditedOutcome::AuditFailed` を返す。
  既存の `ExecutionOutcome` enum は変更せず、`AuditFailure` を `InternalFailure` へ畳まない
  （監査失敗は追加型 `AuditedOutcome` でのみ surface する）。
- **Terminal 配送失敗後の language-state は rollback しない（§10.1 からの逸脱、A-1）**:
  tree engine では Completed/Exited の language-state commit が評価器内（`run_slice` →
  `finalize_session(true)` → `commit_submission`）で、Terminal を sink へ配送する前に確定する。
  このため実行後の `Terminal` 配送が失敗して `AuditedOutcome::AuditFailed` を返したときでも、
  **完了済み script の context 変更（binding 等）は commit されたまま残り、roll back されない**。
  journal 上の唯一の Terminal も `AuditFailure`/`context_committed: false` へは書き換えず、確定済みの
  `Completed`/`context_committed: true` のまま（§8 規則10: started execution は配送結果に関わらず
  確定した Terminal を 1 件だけ持ち、§10 のモデルはその実 Terminal の再配送であって `AuditFailure`
  への書き換えではない）。fail-closed の要件（success を返さない）は満たすが、host は
  `AuditFailed` を受け取った context を**再利用してはならない**（完了 script の変更が観測され得る）。
  監査確定を跨いで言語トランザクションを rollback 可能に保つ §10.1 本来の挙動は、retry/backpressure
  基盤が入る後続スライスへ延期する。
- **timestamp は monotonic tick（§7.1 の簡略）**: A-1 では envelope の `timestamp.unix_nanoseconds`
  に、注入された wall/unix `AuditClock` ではなく request の monotonic clock（`request.clock.now()`）の
  ns を流用する。順序の正本は `sequence` であり `timestamp` は非正本（§7.1）なので影響は限定的だが、
  値は unix 時刻ではなく monotonic tick である。専用 `AuditClock` の注入は後続スライスへ延期する。
- **BudgetExceeded resource は原本を保持（§7.2）**: `control_stop_to_error` が細粒度の
  `BudgetResource` を粗い `ErrorKind` へ縮約するため、監査付き経路では評価器が縮約前に退避した
  原本 resource を `Terminal(BudgetExceeded(resource))` に載せる（原本が無い場合のみ `ErrorKind`
  から粗く復元する）。これにより String/Source/I-O 系の細分 resource も §7.2 どおり記録される。
- **host-call lifecycle 3 event を tree backend で emission（§7.2/§8/§11、A-1 の逸脱を明記）**:
  filesystem host-call 境界（tree 唯一の host-call）で `HostCallStarted` → `CapabilityDecision`
  → `HostCallFinished` の triple を emission する。実装上の逸脱は次のとおり。
  - **deferred post-run drain**: triple は `BudgetCharged` と同じく実行中に評価器へバッファし、
    監査付き facade（`run_audited_inner`）が Started と Terminal の間で drain して emission する。
    A-1 facade は 1 回の `execute_program` 内で script 全体を同期実行し、mid-execution の sink
    round-trip 配線を持たないため、`BudgetCharged` が確立した deferred モデルに合わせる。
  - **§8 規則5/6 の timing を延期**: 外部 filesystem 効果は `dispatch_fs` 内で同期完結し、drain
    時点では効果も response も既に確定している。したがって allow decision ack 前に外部効果を
    止める（規則5）・`HostCallFinished` ack 前に response を script へ露出させない（規則6）という
    timing は A-1 では満たせず、retry/backpressure スライスへ延期する。A-1 が満たすのは triple の
    論理順序（規則3）、deny-path の pair/triple（規則4、Deny/Denied/`effect_status=None`）、
    operation_id/call_id の 0 始まり単調・triple 内再利用、journal/sink の fail-closed である。
  - **§11 reserve は drain 時の容量チェック**: host call 開始前の atomic reserve（規則8 相当）は
    A-1 では pre-effect reservation ではなく、drain 時に `AuditJournal::can_reserve_host_call()`
    で「`HostCallStarted`（1 event / ≤60 KiB）＋ close reserve（2 events / 128 KiB）」が通常枠へ
    収まるかを triple ごとに確認し、収まらなければ triple を 1 件も出さず
    `AuditFailure::BudgetExceeded` にする。外部効果自体を reserve 失敗で拒否する faithful 実装は
    規則5/6 と同じ後続スライスへ延期する。1 execution の未完了 host call は tree の同期実行で
    最大 1 個なので close reserve も最大 1 組（§11）。
  - **tree-only / payload は leak-free プレースホルダ**: VM への配線は Slice 6 へ延期し、
    tree/VM 差は既知の受容済み差分とする。payload（`request` / `resource` / `response`）は
    redaction 本体未実装のため原本 byte 長だけを `Omitted` で載せ、生 path/body は漏らさない
    （§9 既定）。`error_code` は安定 `ErrorCode` 文字列であり script 可視 message は載せない。
- **yield/resume lifecycle 2 event を tree backend で emission（§7.2/§8 規則7/8/9、A-1 の逸脱を明記）**:
  tree evaluator の slice 境界（`run_slice`）で `Yielded { reason, poll_index, usage }`（tag 5）と
  `Resumed { previous_reason, poll_index, usage }`（tag 6）を emission する。実装上の決定・逸脱は
  次のとおり。
  - **per-step drain（B-1 で poll driver 化、旧 deferred post-run drain から移行）**: `Yielded`/
    `Resumed` は `BudgetCharged`・host-call triple と同じく実行中に評価器へ buffer する。B-1 で
    監査付き driver を同期 loop（`run_transactional`）から **facade 内の単一 execution poll driver**
    （`run_audited_driver`）へ移行し、1 poll ステップ（`run_slice` 1 回）ごとに評価器 buffer を
    従来 drain 順で `AuditDelivery` 経由で submit する（§10.2 の sink round-trip を実行中に行うため）。
    drain 順は「host-call triple・`Yielded`/`Resumed`・slice 境界で flush した `BudgetCharged` を
    発生した temporal 順に並べた 1 本の列」→ accumulator に残る `BudgetCharged`（per-slice flush）→
    `Terminal`。B-1 時点では host-call triple と `Yielded`/`Resumed` を別 buffer から「host-call 群 →
    yield/resume 群」の固定順で drain しており、resume 後の slice で起きた host call が対の `Resumed`
    より前に配送されて §8 規則8 に反していた（yield と host call が同じ実行で起きる場合だけ）。B-2 で
    評価器 buffer を 1 本にまとめて修正した（下記「B-2 実装スライス」）。backpressure が一度も起きない実行では、per-step drain の envelope
    列は旧 post-run 一括 drain と完全一致する（評価器 buffer を同じ順で引くため。既存
    `tests/audit_yield_resume.rs` / `tests/audit_core.rs` を無改変で緑に保つ回帰条件で固定）。
  - **実 yield を起こす slice fuel の橋渡し（D3）**: 監査付き run 経路（poll driver）は
    従来 `run_slice(u64::MAX)` で単一 slice だったため監査下で実 yield が起きなかった。
    `EngineBuilder::audit_slice_fuel` で小さい slice fuel を与えると監査付き実行が複数 slice へ
    分かれ、文/反復境界で実 `Yielded(SliceFuelExhausted)` が起きて 2 event が流れる。既定（未設定＝
    `u64::MAX`）は従来挙動そのもので、観測結果・出力 byte 列・課金は不変。sink 無し経路
    （`run_guarded`）は常に `u64::MAX` で、この knob の影響を受けない（§1.1 opt-in・非破壊）。
  - **§8 規則7/8/9 を満たす**: 規則7（flush-before-Yielded）は各 yield 境界で budget accumulator を
    flush し、その slice ぶんの `BudgetCharged` を同 slice の `Yielded` より前へ確定させる。規則8
    （Resumed-before-work）は次 slice 先頭で、`run_driver` を呼ぶ前（まだ 1 文も実行していない）に
    `Resumed` を buffer する。規則9（Terminal last）は従来どおり facade が全 deferred buffer を
    drain した後に `Terminal` を append する。`poll_index` は execution ごとに 0 始まり単調
    （`checked_add` で overflow fail-closed）で、1 回の yield/resume pair の `Yielded` と `Resumed`
    は同じ値を共有し、`Resumed.previous_reason` は対の `Yielded.reason` と一致する。
  - **yield-cause → `AuditYieldReason` 写像**: `SliceFuelExhausted`（主経路・テストで実際に起きる）/
    `HostCallPending { call_id }`（cooperative host-call、A-1 同期経路では Ready フォールバックのため
    実際には起きないが写像は定義）/ `ExplicitYield`（型として写像、未 trigger）を純粋関数で写す。
    `AdmissionQueued` / `SchedulerPreempted` / `HostPaused` は engine-poll 経路（sink を持たない）
    でのみ生じるため、写像は定義するが A-1 監査付き facade では emission しない（下記）。
    host pause は `Yielded { reason: HostPaused }`・resume は `Resumed { previous_reason: HostPaused }`
    へ写すが、`HostPaused` を script 可視の公開 `YieldReason` へは追加しない（§8）。
  - **tree-only / VM 未配線**: VM には scheduler/pause 経路が無く、yield/resume emission は tree-only。
    tree/VM 差は既知の受容済み差分（Slice 6 で解消予定）。
  - **mapped-but-not-emitted（engine-poll 経路）**: `AdmissionQueued` / `SchedulerPreempted`
    / `HostPaused` の yield は `engine.rs::poll`（sink を持たない alpha facade）でのみ生じる。
    監査付き driver（facade の poll driver）は slice-fuel yield と backpressure yield を起こすが、
    これら 3 reason は起こさないため emission されない。sink-bearing な scheduler/pause 経路の配線は
    後続スライスへ延期する。
- **`AuditBackpressure`-yield 配線（§10/§10.2、B-1、実装済み）**: audit sink が batch を
  `AuditSubmit::Pending` にしたとき、facade の poll driver（`AuditDelivery::deliver`）が execution を
  実際に `Yielded(AuditBackpressure)` へ遷移させ、ack（同一 `Arc` batch の再送で Ack される、または
  `AuditTicket::try_take`）で `Resumed{previous_reason: AuditBackpressure}` へ resume する。§10 の
  「次回 `ExecutionHandle::poll` が `try_take` して再開」は、A-1（tree-only・facade 内 driver）では
  **facade の poll driver が round-trip を担う**形で実現する（scheduler engine は sink を持たない。
  公開 `YieldReason::AuditBackpressure` は facade driver 経由で returnable な公開 variant になり、
  scheduler engine 側で返すのは E10 統合後）。backpressure は sink 応答由来で評価器の意味論
  （`SliceOutcome`）を跨がず、`Yielded`/`Resumed` の poll_index 発番・pair 共有は評価器の単一
  `poll_index` カウンタに乗る（driver 独自カウンタなし）。backpressure yield が SliceFuel yield の
  event 配送中に起きるとネスト（外側 SliceFuel の Resumed はまだ）するため、外側 yield の pair 情報を
  退避し backpressure Resumed 後に復元する。各 poll ステップ先頭で cancel→deadline を `try_take` より
  **先に** checkpoint し（§4.6 linearization）、永続 `Pending` のまま deadline へ達した場合は
  `AuditFailure::BackpressureDeadline` の terminal にして script の `Deadline` より優先する（§10.2、
  「実行結果は成功だが監査が欠ける」状態を作らない）。公開 API の破壊的変更は
  `YieldReason::AuditBackpressure` / `AuditFailure::BackpressureDeadline` の追加（additive）で、
  `language_revision` bump は無い（既存 tag 5/6 + 既存 `AuditYieldReason::AuditBackpressure`）。§10.2 の
  audit drain queue byte 会計（per-execution 1 MiB / engine 全体 16 MiB・`AuditBudget.max_pending_bytes`
  消費）は B-2 へ分離（sink-Pending 由来の backpressure だけ先行）。これにより §15.3 の受入基準
  「永続 Pending で script が成功扱いにならず AuditFailure terminal になる」が sink-Pending 経路で
  満たされ、§15.3「Yielded と Resumed が交互に対応し audit backpressure を区別できる」も
  `AuditYieldReason::AuditBackpressure` の Yielded/Resumed pair で満たされる（`tests/audit_backpressure.rs`）。
- **未実装（意図的に含めない）**: redaction policy 本体のうち body allow-list Reveal（§9、R-1b。既定 policy の
  redaction tree R-1a は下記「R-1 実装スライス」で実装済み）、
  strict deterministic CBOR（§9.2、A-1 の
  `encoded_bytes` は naive 推定で後で差し替え可能に隔離）、record/replay（§6）、VM の変更、§10.2 の
  audit drain queue byte 会計のうち engine 全体の上限（per-execution 上限は B-2 で実装済み。下記「B-2 実装スライス」）。`AdmissionQueued` / `SchedulerPreempted` / `HostPaused`
  の yield emission（engine-poll 経路に sink が無い、上記）も後続スライスへ延期する。

#### B-2 実装スライス（tree engine、opt-in、audit drain queue byte 会計）

> 状態: **実装済み**（2026-10-20、tree backend・opt-in）。実装上の注記は本小節末尾「実装上の注記」を参照。

B-1 は sink の `Pending` 応答に由来する backpressure だけを配線し、§10.2 の
「audit drain queue の byte 上限」は消費していない（`AuditBudget.max_pending_bytes` は未使用）。
B-2 はこの byte 会計を tree engine の監査付き poll driver へ入れる。仕様（§10.2・§11）が正本であり、
以下は B-2 時点の実装範囲と逸脱の記録である。

- **drain queue の定義**: execution の audit drain queue を「生成済みだが sink の ack を
  まだ得ていない通常 event の encoded byte 合計」と定義する。A-1/B-1 の実装では次の 2 つの和になる。
  1. 評価器 buffer（host-call triple・`Yielded`/`Resumed`・確定済み `BudgetCharged`）にあり、
     journal へまだ append していない event。
  2. journal へ append 済みで、sink の ack をまだ得ていない in-flight batch（現在は 1 envelope/batch
     の同期 deliver なので高々 1 batch）。

  既定 slice fuel（`u64::MAX`）では script 全体が 1 slice で走り、評価器 buffer は terminal まで
  flush されない。B-2 の主目的は、この buffer の無制限な成長を §10.2 の上限で抑えることである。
  `ExecutionStarted` と `Terminal` は queue 会計の対象外とする（Started は最初の work の前に ack 待ち、
  Terminal は §10.1 の専用 reserve を使うため）。`BudgetCharged` は accumulator 内の未確定集約
  （256-charge cut 前）を数えず、確定 event として buffer へ移った時点で数える（未確定集約は
  (resource, reason) ごとに 1 件で有界）。

- **上限判定の位置**: 評価器は event を buffer へ積むたびに queue byte を加算し、既存の slice fuel
  判定と同じ **トップレベル driver の文/反復境界**（`run_driver` の `can_yield && …` 判定と同じ位置）で
  `queue_bytes >= max_pending_bytes` を判定する。達していれば slice を終えて crate 内部の
  `SliceOutcome::YieldedAuditQueue` を返す。式の途中、および関数呼び出し・callback の本体
  （`drive_call_body`、`can_yield = false`）の中では yield しない（tree は Rust 再帰スタックを
  保存できないため。§4.2・[実行予算・協調実行仕様](execution-control.md) Slice 3 PR-d と同じ制約）。
  したがって上限は「トップレベル境界で判定する soft limit」である（下記「逸脱」）。

- **前進保証**: 判定は **その slice で 1 文以上実行した後** にだけ行う。resume 直後（`Resumed` を
  buffer した直後）の境界では判定しない。これにより `max_pending_bytes` がどんな値（0 や
  `Resumed` 1 件より小さい値を含む）でも、yield と resume だけを繰り返して 1 文も進まない livelock は
  起きない。`max_pending_bytes = 0` は「トップレベルの 1 文ごとに flush する」設定として有効であり、
  config error にはしない。

- **yield と event 順序**: queue 上限による yield は §10.2 どおり
  `Yielded { reason: AuditBackpressure }` として記録する（新しい `AuditYieldReason` や
  CBOR tag は追加しない）。sink が即 Ack する場合も必ず `Yielded`/`Resumed` の pair を出す
  （§8: yield は必ず監査される）。順序は SliceFuel yield と同じ規律に従う。
  1. §8 規則7: budget accumulator を flush し、その slice ぶんの `BudgetCharged` を `Yielded` より前に置く。
  2. `Yielded(AuditBackpressure)` を評価器の単一 `poll_index` カウンタで発番する（driver 独自カウンタなし）。
  3. driver は `flush_events` で buffer を全量 deliver する（sink が `Pending` なら B-1 の round-trip に入る）。
     flush 完了時に queue は 0 へ戻る。
  4. 次の `run_slice` 先頭で `Resumed { previous_reason: AuditBackpressure }` を buffer する（§8 規則8）。

  queue 由来か sink `Pending` 由来かは event 上は区別しない（どちらも `AuditBackpressure`）。

- **単一 event が上限を超える場合**: 特別扱いしない。flush 完了で queue は必ず 0 へ戻るため、
  `max_pending_bytes` を超える event が 1 件あっても、次のトップレベル境界で yield・flush して前進する
  （進行不能にはならない）。event 単体の size 検査は従来どおり journal 側（`EventTooLarge` /
  `BudgetExceeded`、§11）に任せ、`EventTooLarge` を queue 上限の意味では使わない。

- **byte の測り方**: queue byte は journal の会計と同じ推定関数で測る（§9.2「予算 byte counter が
  別実装の size 推定を持つことを禁止」）。評価器 buffer の event はまだ envelope ではない（sequence 未確定）
  ため、`audit::estimate_encoded_bytes` を「envelope 固定部（`language_revision` に依存、sequence に
  非依存）＋ `estimate_event_bytes(event)`」の 2 部品へ分解し、journal と queue の双方がこの同じ部品を
  使う。現行 naive 推定は sequence に依存しないので、push 時の計上値は append 時の journal 計上値と
  一致する。

- **engine 全体 16 MiB の延期**: B-2 は **per-execution の上限（`max_pending_bytes`）だけ**を
  実装する。engine 全体の 16 MiB 上限は、複数 execution で共有する queue 会計の持ち主と、上限で
  yield した execution を起こす wake 経路が scheduler（E10 統合）と不可分なため、後続スライスへ延期する。

- **設定経路**: facade は既定で `AuditBudget::default()`（`max_pending_bytes` = 1 MiB）を使う。
  公開 builder setter は追加しない。境界テストのため、`EngineBuilder` に `#[doc(hidden)] pub` の
  shim `audit_budget_for_test(AuditBudget)` を置く（`link_for_repl` / `run_for_repl` と同じ扱いで、
  公開契約は増やさない）。shim は内部で `AuditBudget::validate()` を呼び、不正値なら panic する
  （テスト専用のため）。注入された budget は facade の `AuditJournal::open` と queue 判定の双方に使う。

- **opt-in・非破壊**: sink なし経路（`run_guarded`）は queue 会計を一切行わず、観測結果・出力 byte・
  exit code は不変。sink あり経路では、queue が一度も上限に達しない実行の envelope 列は B-1 と
  完全一致する。新しい公開型・CBOR tag・`language_revision` の変更はない。
  **sink あり経路での挙動変化**: 既定 budget（1 MiB）でも、トップレベル境界までに 1 MiB 超の event を
  溜める script では、envelope 列の途中に `AuditBackpressure` の `Yielded`/`Resumed` pair が入る
  （queue 上限到達は backpressure の一種）。opt-in 経路内の変化であり alpha として受容する。

- **逸脱（B-2 時点で残るもの）**:
  - **関数呼び出し・callback の中では上限が効かない**。判定はトップレベル境界だけなので、例えば
    `main()` の 1 文の中で多数の host call を行うと、`main()` から戻るまで queue は上限を超えて
    溜まり続ける。超過量は host-call 回数 budget（`BudgetConfig::standard` で `max_host_calls`
    = 10,000、event 最大約 3 万件）で有界だが、1 MiB を大きく超え得る。根本対策（関数内 yield、
    または §11 の host call 開始前 reserve）は B-2 の範囲外。
  - engine 全体 16 MiB の未実装。同一 `Engine`（`Send + Sync`、`run_audited(&self, …)`）を複数 thread
    から同時に `run_audited` した場合の合計は束縛しない。
  - byte 数は naive 推定。§9.2 strict CBOR へ差し替えると sequence/timestamp の encode 長が値依存に
    なり、push 時点（sequence 未確定）では正確な長さが分からない。その時点では §11 の size pass と
    同じく「sequence を最大長とみなした上限値」で queue へ計上する。差し替えで yield が起きる位置が
    変わり得るため、回帰テストの期待値を更新する。
  - 公開 setter（`EngineBuilder::audit_budget`）と `ConfigError` への `AuditConfigError` 接続は、
    sink 必須化（§1.1）のスライスへ延期する。

- **変更箇所**:
  - `src/eval.rs`: audit drain queue 計器（`AuditQueueMeter`。buffer への push 時に加算、sink の ack 時に
    減算）、`max_pending_bytes` の保持（`set_audit_queue`、計器が無い経路では計上・判定なし）、
    トップレベル文/反復境界での判定（slice 内で 1 文以上実行済みの場合のみ）、
    `SliceOutcome::YieldedAuditQueue`、`audit_yield_reason_from` の写像追加
    （→ `AuditYieldReason::AuditBackpressure`）。host-call triple 用と yield/resume 用の 2 本の buffer を
    temporal 順の 1 本（`audit_events`）へ統合（下記「実装上の注記」）。
  - `src/builtin.rs`: 変更なし（`push_host_call_events` 経由で計上される）。
  - `src/budget.rs` / `src/audit.rs`: `BudgetChargeAccumulator` が確定済み（256-charge cut 済み）分の
    event byte 合計を保持し、`BudgetLedger::audit_finalized_charge_bytes` で queue 判定へ渡す。
  - `src/audit.rs`: `estimate_encoded_bytes` を envelope 固定部（`estimate_envelope_overhead_bytes`）＋
    event 本体（`estimate_event_bytes`、`pub(crate)`）へ分解。`max_pending_bytes` の doc コメントを更新。
  - `src/embedding.rs`: `run_audited_driver` で計器を張り、`YieldedAuditQueue` を `continue` 扱いにする。
    `flush_events` は temporal 順の 1 本の列を deliver し、各 event を ack 時に queue から外す（in-flight を
    含む）。flush 後に queue が 0 に戻ることを debug assert。`EngineBuilder::audit_budget_for_test` shim と、
    注入 budget の `AuditJournal::open` への受け渡し。
  - `src/engine.rs`: engine-poll 経路の `SliceOutcome` 写像に `YieldedAuditQueue` を追加（計器を張らない
    ので到達しないが、到達時は通常の slice 境界 yield と同じ扱い）。

- **受入基準（B-2）**:
  - **B2-AT-01**: `max_pending_bytes` を小さく注入し、**トップレベルのループから** host call を多数行う
    script を監査付きで実行すると、上限到達の境界で `Yielded{AuditBackpressure}` →
    `Resumed{AuditBackpressure}` の pair が出る。sink が即 Ack する場合も pair が出る。pair は
    poll_index を共有し、`Resumed.previous_reason` は対の `Yielded.reason` と一致する（§15.3）。
  - **B2-AT-02**: 上限到達時に queue の event が捨てられたり上書きされたりしない。sequence は 0 から
    gap・重複がなく、host-call の triple がすべて揃う（§10.2「queueを捨てない」・§15.3）。
  - **B2-AT-03**: §8 規則7（`BudgetCharged` が `Yielded` より前）・規則8（`Resumed` が次の work より前）・
    規則9（Terminal が最後）を満たす。
  - **B2-AT-04**: queue 上限 yield と sink `Pending` が重なる場合（queue 上限で flush 中に sink が
    `PendingThenAck` を返す場合）も、pair が交互に正しく対応し、最終的に Completed になる。
  - **B2-AT-05**: queue 上限 yield 中の flush で sink が `PendingForever` を返し deadline に達した場合は、
    `AuditFailure::BackpressureDeadline` になる（script の Deadline より優先、§10.2）。
  - **B2-AT-06**: 単一 envelope が `max_pending_bytes` を超える設定（`max_pending_bytes = 0` を含む）でも、
    進行不能にならず各トップレベル境界で yield・flush して Completed になる。yield/resume だけを
    繰り返す livelock が起きない（前進保証）。
  - **B2-AT-07（非破壊）**: 既定 budget（1 MiB）で既存の `tests/audit_core.rs` /
    `tests/audit_hostcall.rs` / `tests/audit_yield_resume.rs` / `tests/audit_backpressure.rs` が
    無改変で緑。sink なし経路の出力 byte・exit code が不変。既存テストが上限に到達して期待値が
    変わる場合は、実装を止めて報告する。
  - **B2-AT-08（決定性）**: 同じ script・同じ注入 budget・同じ sink 応答列では、yield が起きる位置と
    envelope 列が 100 回の実行で一致する（§15.1 の部分適用）。
  - **B2-AT-09（関数内 soft limit）**: 関数本体の中で多数の host call を行う script では、関数の中では
    yield せず、関数から戻った後のトップレベル境界で yield する。その間 event は欠落せず、
    sequence・triple の完全性（B2-AT-02 と同条件）を満たす。
  - **B2-AT-10（byte 計上の一致）**: push 時に queue へ計上した byte の総和が、同じ event を journal へ
    append したときの `committed_bytes` の増分と一致する（推定関数の共有）。
  - VM は対象外（監査 sink と `VmExperimental` の同時設定は従来どおり build 時に拒否）。

- **テスト配置**: `tests/audit_queue_bytes.rs`（B2-AT-01〜09、および §8 規則8 の回帰
  `slice_fuel_yield_with_host_calls_keeps_resumed_before_next_work`）。B2-AT-10 は `src/audit.rs`
  （`b2_at_10_queue_estimate_matches_journal_committed_bytes` ほか）/ `src/eval.rs`（`audit_queue_meter`）の
  unit test、B2-AT-07 は既存 `tests/audit_core.rs` / `tests/audit_hostcall.rs` /
  `tests/audit_yield_resume.rs` / `tests/audit_backpressure.rs` の無改変緑で担保する。

- **実装上の注記（B-2 実装時に判明・決定したもの）**:
  - **§8 規則8 違反の修正（B-1 からの既存不具合）**: B-1 までは host-call triple と `Yielded`/`Resumed` を
    評価器の別 buffer に積み、flush で「host-call 群 → yield/resume 群」の固定順に drain していた。この
    ため yield（SliceFuel を含む）と host call が同じ実行で起きると、resume 後の slice の host call が対の
    `Resumed` より前に配送されていた（既存テストは yield と host call を同時に起こさないため未検出）。
    B2-AT-03 で顕在化したため、buffer を temporal 順の 1 本に統合して修正した。yield と host call が
    同時に起きない実行の envelope 列は不変（既存テスト無改変で緑）。
  - **`SliceOutcome` への variant 追加**: `SliceOutcome` は `pub mod eval` の公開 enum だが、crate 文書
    （`src/lib.rs`）どおり個別モジュールはベンチ・テストツール互換のための公開で埋め込み API としての
    安定性を保証しない。埋め込み入口（crate root re-export）には現れないため、`YieldedAuditQueue` の
    追加は安定公開 API の破壊的変更として扱わない。
  - **test 用 budget の保持場所**: `EngineConfig` は全 field 公開の公開構造体で、field 追加は外部の
    literal 構築を壊すため、注入した `AuditBudget` は field 非公開の `EngineBuilder` / `Engine` に持たせる。
  - **accumulator から直接 drain する `BudgetCharged`**: flush 時に accumulator から drain した分は、
    drain で accumulator 側の合計から外れるため、その時点で queue へ計上し直し、ack 時に外す。queue の
    判定は評価器の境界（flush 中ではない）で行うので、判定時点の queue に in-flight 分は現れない。

#### R-1 実装スライス（tree engine、opt-in、redaction policy 本体）

> 状態: **実装済み（R-1a、tree backend・opt-in）**。本小節は A-1 が残した redaction プレースホルダ
> （payload は leak-free の `Omitted`、`redaction_policy_id` は固定値）を §9.1/§9.2 の redaction policy
> 本体へ差し替える最小縦切り（R-1a）の slice 定義である。仕様（本章 §9）が正本であり、以下は R-1 で
> 実装した範囲と意図的に延期する逸脱の記録である。実装は本定義とは別 PR（実装 PR #238）で行った。
> 公開型 `AuditPayload`/`AuditField` 差し替えは破壊的変更だが alpha として受容し、`language_revision`
> の bump は行わない（受容の記録は [組み込みAPI仕様](embedding-api.md) §13.1。当初は roadmap 明記を根拠にしていた。loop:drift #318 の D-22 で判断役が選択（issue #318））。残る strict CBOR・record/replay・VM
> audit triple 対応・sink 必須化・body allow-list Reveal（R-1b）は未実装。

A-1 が残していた redaction プレースホルダ（host-call lifecycle 3 event〔§7.2/§8〕の payload を
crate 内簡略型 `AuditPayloadField::Omitted` で leak-free に載せ、`redaction_policy_id` を固定値
`"default"` にしていた）は R-1a で §9.1 の `AuditField` ツリーへ差し替え済みである（構築は
`src/builtin_core.rs::host_call_triple`）。R-1a は既定 redaction policy を tree engine の監査付き
経路へ配線し、`request`/`response` の `body` は `Omitted{class:Body}`、`resource` の path は Allow で
canonical label の `HmacSha256`・Deny で `Omitted{class:Path}`（raw path/label は非露出）とし、
`arguments` は `Revealed{class:Public, Sequence}` で各要素を Path/Body へ畳む。`ExecutionStarted` の
`redaction_policy_id` は注入 policy の `policy_id`（既定 `"default-r1"`）を反映する。

> このスライス定義は semantic review（本小節末尾「R-1 レビュー反映の記録」）を複数周通し、初版から
> 次の点を改めた: (1) encoded 長の非破壊根拠が成り立たないため **byte 推定（`estimate_event_bytes`）を
> R-1 では据え置く**ことにした、(2) 現状 payload が「空」という事実誤認と `src/builtin.rs` の誤記を正した、
> (3) 既定 `hmac_key` のジレンマを設計判断として明記した、(4) 公開型 `AuditPayload`/`AuditPayloadField`
> 差し替えが破壊的変更であることを明記した。採用・不採用の理由は末尾の記録を参照。

- **粒度は R-1a（既定 policy のみ）**: 既定 policy（`identifier_mode = Reveal` / `path_mode =
  HmacSha256` / `body_mode = Omit`）だけを実装し、filesystem host-call（tree 唯一の host-call 境界）の
  実 payload を §9.1 の `AuditField` ツリーへ載せる。body の allow-list Reveal 経路
  （`reveal_body_fields` と `max_revealed_body_bytes > 0` による `audit_safe` field の明示開示）は
  型として用意するが R-1 では常に `Omitted` に畳み、R-1b（後続スライス）へ延期する。

- **型の差し替え（§9.1）**: crate 内簡略型 `AuditPayload`（`Vec<(String, AuditPayloadField)>`）と
  `AuditPayloadField`（`Public` / `Omitted`）を §9.1 の型へ差し替える。`AuditField`（`Revealed` /
  `HmacSha256` / `Omitted`、tag 0/1/2）・`AuditValue`（`Scalar` / `Sequence` / `Object`）・
  `AuditScalar`・`AuditFieldClass`（`Public` / `Identifier` / `Path` / `Body` / `Secret`）・
  `RedactionMode`（`Reveal` / `HmacSha256` / `Omit`）・`RedactionPolicy` を新設する。`AuditPayload.fields`
  は §9.1 どおり `BTreeMap<String, AuditField>`（UTF-8 key byte 順）とし、`Sequence` は argument/element
  順を維持する。host call 引数は `fields["arguments"] = Sequence(...)` へ、operation 固有 metadata は
  別 key へ入れる。nesting は 64 段までとし、それ以上の subtree は `Body`/`Omitted` へ縮退する。

- **field class 判定と既定モード（§9.1）**: filesystem host-call について次を適用する。
  - **Path**: provider が認可に使った canonical resource label に HMAC-SHA-256 を適用し、
    `HmacSha256 { digest, original_bytes }` で記録する。**canonical resource label の定義**: routing 後の
    `FilesystemTarget`（`src/capability.rs`）＝ **mount 名（`MountName`）＋ mount 相対 path
    （`RelativePath` の component 列）**を、`@<mount>/<component>/...` の正規形へ組んだ文字列とする
    （host absolute path・CWD・symlink 解決結果は含めない）。同じ mount・同じ mount 相対 path は同じ label
    ＝同じ digest になり（R-AT-01 の「同じ label」の意味）、別 mount または別相対 path は別 digest になる。
    host absolute path は label へ含めない。raw path は `Revealed` へ出さない。deny event でも raw
    requested path を出さない。単純 SHA-256 は使わない。この配線のため、評価器は現状 0 固定の
    `path.original_bytes` を実長へ改め、canonical resource label を `host_call_triple` へ渡す
    （下記「変更箇所」）。
    - **Path を HMAC するかは decision で決める（label の有無ではなく判定で分岐）**: HMAC 対象は「provider が
      認可に使った canonical resource label」で、これは `fs_route`（`src/builtin_core.rs`）が
      `FilesystemTarget` を返した**時点**（＝mount routing と operation 認可が通った時点）で確定する。
      `fs_open` 内の `adapter.open_file` 失敗はこの後に起きるので、その host error でも label は既にある。
      **R-1 の確定規則は「`host_call_triple` の `CapabilityDecisionKind` が `Allow` なら Path を
      `HmacSha256`、`Deny` なら label が手元にあっても `Omitted { class: Path }`」**とし、label の有無では
      なく decision で分岐させる（label 有無を基準にすると「label があるなら HMAC してよい」と読み違える
      恐れがあるため）。decision 別の内訳:
      - **Allow / `Success`**（正常完了）→ Path は `HmacSha256`。
      - **Allow / `HostError`**（route は成功して label 確定済み、その後 `adapter.open_file` /
        `read_to_end` / `write_all` / `create_dir` / `remove` 等が失敗。`fs_result` は
        `Err(ErrorKind::Host)`）→ label はあるので Path は `HmacSha256`。`fs_result` が `Err` でも label を
        落とさないことが要件（下記「変更箇所」の戻り値型）。
      - **Deny / `Denied`** → Path は `Omitted { class: Path }`。大半は label 確定前の拒否だが、
        **route 成功後に拒否される経路もある**: Upsert（`write_file`/`append_file`）が Write では通り
        Create 権限が無いとき、`fs_route`（Write 代表 op）成功で target＝label が決まった後に
        `filesystem_denied` になる。この経路でも decision は `Deny` なので label を HMAC せず `Omitted`。
        その他の Deny は label 確定前: (a) filesystem 未 grant（`filesystem()` が `None`）、
        (b) path lexical/argument エラー（`FilesystemTarget::parse` の `PathError`）、
        (c) mount 未登録・operation 未 grant・許可外（provider 問い合わせ前の `sandbox` denial）。

      Deny では HMAC せず raw requested path も `Revealed`/HMAC 入力に出さない（raw path を代用 HMAC する
      のは host absolute path を label へ含めない §9.1 方針に反し、3 OS の path 表現差が digest に漏れる
      ため採らない）。まとめると **Path は `Allow`（Success/HostError 両方）で `HmacSha256`、`Deny` で
      `Omitted`（label の有無を問わず）、raw は常に非露出**（R-AT-01/02 で明記）。
      - **`original_bytes` の取り方**: `Allow` は canonical label の UTF-8 byte 長。`Deny` で label が
        手元にある場合（Upsert の Create 不足）も、`Deny` は label を payload へ出さないため
        `original_bytes` は requested path 文字列の UTF-8 byte 長で統一する（label 由来の長さを漏らさない）。
        `Deny` の (a)/(c) も requested path の UTF-8 byte 長。(b) のうち **引数の型エラー（`require_str`
        失敗で path 文字列すら取れない）**は requested path が無いため `original_bytes = 0` とする。
  - **Body**: file 内容・stdin/stdout・host args/result の自由文は `body_mode = Omit` により
    `Omitted { class: Body, original_bytes }` にする。`original_bytes` は redaction 前の canonical
    subtree 長。R-1 では allow-list Reveal を行わない。
  - **Identifier**: host function 名・capability 名・rule ID・module logical name は既定 `Reveal`
    （`Revealed { class: Identifier, value, original_bytes }`）。
  - **Public**: event 種別・ID・hash・count・error code は `Revealed { class: Public, .. }`。
  - **Secret**: credential/token/password/secret env value/redaction key は常に subtree 全体を
    `Omitted`、mode 変更不可。`Secret<T>` を型で区別し `Debug` / `Display` / generic serializer を
    実装しない。存在と byte 数だけを Public metadata として記録する。R-1 の filesystem 境界には
    Secret field は現れないが、型と畳み規則は本スライスで入れる。

- **`original_bytes` の定義（§9.1）と R-1 での代用**: §9.1 では `original_bytes` は redaction 前の
  canonical subtree payload 長（§9.2 の canonical encoding で測る）と定義される。ただし R-1 は strict CBOR
  （§9.2）を実装せず byte 推定を据え置くため、**strict CBOR スライスまでは `original_bytes` を値の UTF-8
  byte 長で代用する**（scalar は UTF-8 文字列の byte 長、Path は canonical label または requested path の
  UTF-8 byte 長、body は原本文字列の byte 長）。canonical subtree 長との差は strict CBOR 導入時に揃える。
  この代用値は payload の**値**であって event の encode 推定長（128 固定）とは独立で、B-2 の queue 会計には
  影響しない（上記「byte 推定は据え置く」）。

- **`redaction_policy_id` の実配線（§7.1/§7.2）**: `ExecutionStarted.redaction_policy_id` に注入された
  `RedactionPolicy.policy_id` を**そのまま**載せる。`policy_id` は §9.1 の型どおり **host が付ける
  `String`**（内容ハッシュではない。host が policy を識別するための安定ラベル）。A-1 の固定値 `"default"`
  から、注入 policy の `policy_id` 値へ変える。既定 policy は `policy_id = "default-r1"` とし、A-1 の
  `"default"` と区別できる識別子にする（後続スライスで policy が増えたとき既定と区別するため）。

- **byte 推定は R-1 では据え置く（§9.2、非破壊の前提）**: R-1 は `estimate_event_bytes`
  （`src/audit.rs`）を**変更しない**。現状 host-call lifecycle event（`HostCallStarted` /
  `CapabilityDecision` / `HostCallFinished`）・`Yielded` / `Resumed` / `BudgetCharged` は naive 推定の
  **固定 128 byte**（`_ => 128` 分岐）で見積もられ、journal/queue の byte 会計はこの値で動く。R-1 は
  payload の**中身**を §9.1 の `AuditField` ツリーへ差し替える（leak-free を強化する）が、見積もり長は
  128 固定のまま据え置く。これにより host-call event の encoded 推定長・sequence・yield 位置・B-2 の
  queue byte 会計が A-1/B-2 と完全に不変になり、R-AT-05 の非破壊が実際に成立する。
  - **なぜ据え置くか**: payload を実サイズで見積もるように変えると、host-call event の見積もり長は
    128 から payload 依存の値へ動き、B-2 の `max_pending_bytes` queue 会計で yield が起きる位置が変わる。
    これは §9.2 strict CBOR スライス（naive 推定全体を正確な encode 長へ差し替える別スライス、既に延期中）が
    扱う範囲であり、そちらに寄せる方が B-2 の回帰を 1 回で済ませられる。R-1 で見積もりに手を入れると、
    初版定義が自己矛盾していた「`original_bytes`（field の値）を保てば encode 長も不変」という成り立たない
    根拠に逆戻りする（`original_bytes` は payload 中の値であって encode 後の長さそのものではない）。
  - **60 KiB 縮退規則も strict 推定に依存するため R-1 では範囲外**: §9 の「normal event は最大 60 KiB、
    超過時 Body → Path → Identifier の順で `Omitted` へ縮退、なお超過で `EventTooLarge`」は、event の
    実 encode 長に対する判定なので、正確な byte 推定が前提になる。128 固定見積もりのままでは faithful に
    実装できないため、縮退規則は strict CBOR スライスへ延期する（R-1 の受入基準から R-AT-09 を外す）。
    Terminal の `Emergency` 置換は A-1 で実装済みの範囲を踏襲し、R-1 では触らない。

- **policy 注入経路**: facade は既定で `RedactionPolicy::default()`（上記既定モード・空
  `reveal_body_fields`・`max_revealed_body_bytes = 0`・`policy_id = "default-r1"`・固定 `hmac_key`）を
  使う。公開 builder setter は追加しない（sink 必須化 §1.1 のスライスへ延期）。境界テスト用に
  `EngineBuilder` へ `#[doc(hidden)] pub` の shim（`audit_redaction_policy_for_test` 等、
  `audit_budget_for_test` と同じ扱い）を置き、注入 policy を journal と payload 構築の双方へ渡す。
  `hmac_key` は `SecretBytes` で保持し、`Debug` / `Display` を実装せず生 key を出さない。

- **既定 `hmac_key` の性質（設計判断・逸脱として明記）**: R-1 では公開 setter を作らないため、既定
  policy の利用者は既定 `hmac_key` しか使えない。この key の扱いには相反する制約がある。
  - 実行ごとに乱数で生成すると §7.1「engine は ambient な乱数源に触らない」に反し、R-AT-06（決定性・
    HMAC が key 固定で決定的）も壊れる。したがって **R-1 は key を乱数生成しない**。
  - 一方、コードに固定 key を埋め込むと key が公開されているのと同義で、§9.1 が「単純 SHA-256 は辞書攻撃に
    弱いので使わない」と定めた path hashing の秘匿性はほぼ得られない。
  - **採る方針**: R-1 の既定 `hmac_key` は **crate 内の固定値（alpha 用・秘匿性なし）**とし、「audit に
    **raw path をそのまま載せない**」ことだけを目的とする。固定 key では同一 label が同一 digest になるため
    label の相関は残り（これは R-AT-01 の決定性に必要）、かつ **key が公開されている以上、digest から
    候補 path を総当たりで推測・復元できる**（辞書攻撃耐性は得られない）。したがって R-1 が守るのは
    「平文 path の直接露出を防ぐ」ところまでで、推測復元に耐える秘匿性は守れないと明記する。推測復元にも
    耐える秘匿性のある key を host から渡させる公開経路（`EngineBuilder` の key 必須 setter、または sink が
    あるとき key 必須）は **sink 必須化（§1.1）スライス**へ延期する。R-1 の固定 key は §7.1（乱数源に
    触らない）と R-AT-06（決定性）を満たし、秘匿性だけを後続へ送る割り切りである。

- **opt-in・非破壊**: sink なし経路（`run_guarded`）は redaction を一切行わず、観測結果・出力 byte・
  exit code は不変。sink あり経路では、上記「byte 推定は据え置く」により host-call event の encoded
  推定長は 128 固定のまま変わらないため、sequence・yield 位置・B-2 の queue byte 会計が A-1/B-2 と一致する。
  新しい CBOR tag・`language_revision` の変更はない（`AuditField` の tag は §9.2 で既に予約済み）。
  変わるのは **payload の中身**だけである: `request`/`resource`/`response` の `fields` が A-1 の簡略型
  `AuditPayloadField::Omitted`（`body`/`path`）から §9.1 の `AuditField` ツリー（path は Allow 時
  `HmacSha256`・Deny 時 `Omitted{class:Path}`、body は `Omitted{class:Body}`、function 名等は
  `Revealed{class:Identifier}`）へ変わり、`redaction_policy_id` が `"default"` から `"default-r1"` へ
  変わり、`resource` の `path.original_bytes` が 0 固定から実長（Allow は canonical label の UTF-8 byte 長、
  Deny は requested path の UTF-8 byte 長〔Upsert の Create 不足で label があっても requested path 長で
  統一〕、引数型エラーは 0）へ変わる。したがって
  payload 構造・`redaction_policy_id`・`original_bytes` を検査する既存テストの期待値更新が要る
  （下記 R-AT-05）。

- **公開 API の破壊的変更（alpha・受容、`language_revision` bump なし）**: `AuditPayload` と
  `AuditPayloadField` は crate root（`src/lib.rs`）で `pub use` 公開されている。R-1 は簡略
  `AuditPayloadField` を撤去し `AuditPayload.fields` を §9.1 の `BTreeMap<String, AuditField>` へ
  差し替えるため、これらの型に依存する外部コードは壊れる。B-1（`YieldReason::AuditBackpressure` 追加）と
  同じく、alpha（後方互換無保証）につき [組み込みAPI仕様](embedding-api.md) §13.1 への記録を条件に**破壊的変更を受容**する（当初は roadmap 明記を条件にしていた。判断役が選択（issue #318）、D-22）。CBOR tag 表・
  charge table・script 観測値は不変なので `language_revision` bump は無い（既存 tag 1/2/3 の payload 中身が
  簡略型から §9.1 ツリーへ変わるだけ。`AuditField` の tag 0/1/2 は §9.2 で予約済み）。

- **逸脱（R-1 時点で残るもの、後続スライスへ延期）**:
  - **fs builtin 全体の監査範囲**は別 slice で決める（6 周目レビュー反映）。R-1 で host-call triple を
    emission するのは I/O byte 課金対象の 4 builtin（`read_file` / `read_lines` / `write_file` /
    `append_file`）のみで、`rename` / `mkdir` / `remove` 系 / `list_dir` / `file_size` / `is_file` /
    `is_dir` / `path_exists` など capability 判定を通る他の fs builtin は監査対象外のまま据え置く。これらを
    監査対象へ広げると emission 範囲（event 件数・sequence・queue byte 会計）が変わり §7.2/§8 の扱いを要する
    ため、「fs builtin 監査範囲」slice で一括して決める。その slice で `rename`（2 path 操作）を対象化する
    ときは、resource を `source_path` / `destination_path` の 2 field に分け、Allow は 2 つとも canonical
    label の `HmacSha256`、Deny は（移動元の route だけ通った場合も含め）2 つとも `Omitted { class: Path }`、
    `original_bytes` はそれぞれの requested path の UTF-8 byte 長とする（PR #238 でユーザー合意済みの設計を
    引き継ぐ）。
  - body の allow-list Reveal（`reveal_body_fields` / `max_revealed_body_bytes > 0` による
    `audit_safe` 開示）は型のみで常に Omit（R-1b）。
  - **byte 推定の正確化**（`estimate_event_bytes` を payload 依存の実 encode 長へ差し替え）は R-1 の
    範囲外。host-call event は 128 固定見積もりのまま据え置く（上記「byte 推定は据え置く」）。strict
    deterministic CBOR（§9.2）スライスで naive 推定全体をまとめて差し替える。
    - **128 固定は概算であり上限保証ではない**: 現状のコメントは 128 を保守的な上限扱いにしているが、
      R-1 で payload が HMAC digest（32 byte）や Identifier の実データを含むと、実 encode 長が 128 を
      超えうる。R-1 は見積もりを据え置くため、§11 の `max_encoded_bytes` / 60 KiB 判定は実長ではなく
      128 概算に対して行われ、**strict CBOR スライスまで不正確**（過小評価がありうる）。R-1 の host-call
      payload は path=digest・body=Omit・識別子のみで数百 byte 規模に収まり 60 KiB 判定に実害は無いが、
      判定が概算依存である事実を逸脱として残す。
  - **compile-fail に `trybuild` を使わない**: R-AT-07 の compile-fail は、dev-dependency を増やさず
    `compile_fail` doctest で固定する。現状 dev-deps は `criterion` のみで、`trybuild` の stderr
    snapshot は 3 OS（ubuntu/macos/windows）CI で壊れやすい。doctest は error 文言に依存せず「compile
    しない」ことだけを固定できるため、OS 差に強く依存も増えない。`trybuild` 導入は必要になった時点で
    別途判断する。
  - **60 KiB 縮退規則**（§9 の Body → Path → Identifier 縮退・`EventTooLarge`）は正確な byte 推定が
    前提のため strict CBOR スライスへ延期する。
  - 秘匿性のある `hmac_key` の host 注入経路（公開 key 必須 setter、sink 時 key 必須）は sink 必須化
    （§1.1）スライスへ延期する。R-1 の既定 key は固定値（alpha 用・秘匿性なし。raw path を直接載せない
    ことだけを担保し、key 公開ゆえ digest からの推測復元には耐えない）。
  - VM 対応（Slice 6）。監査 sink と `VmExperimental` の同時設定は従来どおり build 時に拒否する。
  - record/replay の redaction 再適用（§12「record 済み redacted payload を盲目コピーせず現行 policy を
    適用」）は record/replay スライス（§6）へ延期する。
  - 公開 setter（`EngineBuilder::audit_redaction_policy`）と `ConfigError` 接続は sink 必須化（§1.1）
    のスライスへ延期する。

- **変更箇所（見込み）**:
  - `src/audit.rs`: §9.1 の型（`AuditField` / `AuditValue` / `AuditScalar` / `AuditFieldClass` /
    `RedactionMode` / `RedactionPolicy` / `Secret` / `SecretBytes`）新設、簡略 `AuditPayloadField` 撤去、
    `AuditPayload.fields` を `BTreeMap<String, AuditField>` 化。**`estimate_event_bytes` は変更しない**
    （128 固定見積もりを据え置く。上記「byte 推定は据え置く」）。60 KiB 縮退規則は実装しない（延期）。
  - `src/builtin_core.rs`（**初版の `src/builtin.rs` は誤記**）: `host_call_triple` の payload 構築を、
    現状の `AuditPayloadField::Omitted`（`body`/`path`）から既定 policy 適用の `AuditField` ツリーへ
    置き換える。path は allow 時に canonical resource label を HMAC・deny 時は `Omitted{class:Path}`、
    body は `Omitted{class:Body}`、function 名は `Revealed{class:Identifier}`。現状 0 固定の
    `path.original_bytes` を実長（allow は label 長、deny は requested path 長）へ改める。
    - **canonical label を triple まで運ぶ配線が必要（host error でも label を落とさない）**: label は現在
      `dispatch_fs` → `dispatch_filesystem_capability` の**内側**、具体的には `fs_route` が
      `FilesystemTarget` を返した時点（`fs_open` 内の `adapter.open_file` 成否より前）で確定し、外へ返って
      いない。実装では label を `fs_route` の直後に受け取り、open/adapter 失敗（HostError）でも保持する。一方 triple は `src/builtin.rs` 側で `dispatch_fs` が返った **後**に
      `fs_result: Result<Value, TsumugiError>` から組み立てている（label は手元に無い）。label の有無は
      `host_call_triple` の分類どおり **decision で決まり outcome の成否と独立**なので、戻り値型は
      **`Ok` でも `Err` でも label を運べる形**にする。具体的には `dispatch_fs` の戻り値を
      `Result<Value, TsumugiError>` から「`{ outcome: Result<Value, TsumugiError>, label: Option<ResourceLabel> }`」
      相当（label は Allow＝`Success`/`HostError` 両方で `Some`、Deny で `None`）へ拡張する。
      **特に host error（route 成功後に `read_to_end`/`write_all` 等が失敗）では `outcome` が `Err` でも
      `label` は `Some`** にする（ここが「allow なら Some」だけでは取りこぼす経路）。`src/builtin.rs` の
      呼び出し側はこの `label` を `host_call_triple` へ渡す。`dispatch_fs` は非監査経路（sink なし）でも
      通るため、label 返却は監査の有無に関わらず常に行い、triple 構築側だけが使う（非監査経路は label を
      無視し観測挙動不変）。
  - `src/builtin.rs`: filesystem builtin dispatch で `dispatch_fs` が返した canonical label（または
    その不在）を `host_call_triple` へ渡す（上記配線の呼び出し側）。
  - `src/embedding.rs`: `EngineBuilder`/`Engine` に `RedactionPolicy` を保持（field 非公開）、
    journal と payload 構築へ受け渡し、`audit_redaction_policy_for_test` shim、`ExecutionStarted` の
    `redaction_policy_id` 実配線（既定 `"default-r1"`）。
    - **`#[derive(Debug)]` との整合**: `EngineBuilder` と `Engine` は現在 `#[derive(Debug)]`。
      `RedactionPolicy`（`hmac_key: SecretBytes` を含む）を field に持たせると、`SecretBytes` が `Debug` を
      実装しない方針（§9.1・R-AT-07）のため derive が壊れる。対応は `SecretBytes` 以外を自動 derive しつつ、
      `RedactionPolicy` に **key を伏せた手書き `Debug`**（`hmac_key` を `"<redacted>"` 等で出す）を実装し、
      `Engine`/`EngineBuilder` の derive はそのまま使えるようにする。`SecretBytes` 自体は `Debug`/`Display`/
      generic serialize を実装しない（R-AT-07 の compile-fail で固定）。
  - `src/eval.rs`: payload 構築 helper が policy と canonical label を参照できるよう受け渡し（host-call
    buffer へ積む前に redaction 適用、temporal 順 1 本 buffer の不変は B-2 のまま）。

- **受入基準（R-1）**:
  - **R-AT-01（Allow 時の Path、Success と HostError 両方）**: 認可が通った filesystem host-call の path が
    raw ではなく `HmacSha256`（32 byte digest）として記録され、同じ canonical label は同じ digest、異なる
    label は異なる digest になる（§9.1）。**正常完了（`Success`）だけでなく、route/open 成功後に adapter が
    失敗した host error（Allow/`HostError`、`fs_result` は `Err`）でも Path は `HmacSha256`**（label は失敗
    前に確定しているため）。host error ケースを明示的にテスト対象へ含める。（`rename` は R-1 の監査対象外
    ＝triple を emission するのは I/O byte 課金対象の 4 builtin〔`read_file`/`read_lines`/`write_file`/
    `append_file`〕のみ。2 path 操作の resource 分割は後続「fs builtin 監査範囲」slice へ延期。下記「逸脱」
    参照。）
  - **R-AT-02（Deny 時の Path と raw 非露出）**: decision が `Deny` の host-call は **label が手元にある
    場合でも** Path field が `Omitted { class: Path }` になる。対象経路——(a) filesystem 未 grant、
    (b) path lexical/argument エラー、(c) mount 未登録・operation 未 grant・許可外、(d) **Upsert の
    Create 権限不足**（`fs_route` 成功で label 確定済みだが `Deny`）——いずれでも Path は `Omitted`。
    そのうえで **全経路で raw requested path も canonical label も payload（`resource`/`request`/error）に
    1 byte も現れない**（`Revealed` にも HMAC 入力にも出ない。§9.1）。(d) を「label があるのに Omitted」の
    テストケースとして明示的に含める。R-1 の不変条件は「Path は `Allow`（Success/HostError）で
    `HmacSha256`・`Deny` で `Omitted`（label の有無を問わず）、raw/label は Deny で常に非露出」。
    （`rename` は R-1 の監査対象外のため R-AT-02 の対象にも含めない。下記「逸脱」参照。）
  - **R-AT-03**: body（file 内容・args の自由文）が `Omitted { class: Body }` で、`original_bytes` に
    redaction 前の canonical byte 長が入る（prefix も残さない。§9.1）。
  - **R-AT-04**: `ExecutionStarted.redaction_policy_id` が注入 policy の `policy_id`（§9.1 の host 指定
    `String`）を**そのまま**反映する。既定 policy では `"default-r1"`、別 `policy_id` を注入すると
    その値になる（§7.1。内容ハッシュではなく host ラベルの反映）。
  - **R-AT-05（非破壊）**: sink なし経路の出力 byte・exit code が不変。既定 policy で既存
    `tests/audit_core.rs` / `tests/audit_hostcall.rs` / `tests/audit_yield_resume.rs` /
    `tests/audit_backpressure.rs` / `tests/audit_queue_bytes.rs` の **sequence・yield 位置・encoded
    推定長（128 固定）**が不変。これは R-1 が `estimate_event_bytes` を据え置くことから機械的に従う
    （payload 中身の型だけが変わり、見積もり長は payload 非依存の 128 固定のまま）。payload 構造・
    `redaction_policy_id`・`path.original_bytes` を検査する assertion は §9.1 ツリーへ期待値を更新する。
    万一 yield 位置が動く差分が出たら、見積もり据え置きが崩れている兆候なので実装を止めて報告する。
  - **R-AT-06（決定性）**: 同じ script・同じ `hmac_key`・同じ注入 policy では、redaction 結果と envelope
    列が 100 回の実行で一致する（§15.1）。既定 key は固定値で乱数生成しない（§7.1）ため HMAC は決定的。
  - **R-AT-07（Secret・§15.4 の compile-fail を含む）**: Secret class の field は存在と byte 数だけが
    記録され、値が payload・`Debug`/`Display` 出力・error message のいずれにも漏れない。加えて §15.4 の
    要求どおり `Secret` / `SecretBytes` が `Debug` / `Display` / generic serialize を**実装しない**ことを
    compile-fail で固定する。**枠組みは `trybuild` を使わず `compile_fail` doctest にする**（下記「逸脱」）。
  - **R-AT-08（byte 計上の不変）**: R-1 は見積もりを据え置くため、host-call event の push 時計上 byte
    （128 固定 + envelope 固定部）が A-1/B-2 と変わらず、journal の `committed_bytes` 増分とも一致し続ける
    （B2-AT-10 の不変条件を維持）。redaction 後 field を実サイズで測る正確化は strict CBOR スライスへ延期。
  - **R-AT-09（error message）**: Terminal/error の payload に安定 `ErrorCode` / `message_id` / source
    位置 / redacted parameter summary だけが入り、script-visible message がそのまま入らない（§9.1）。
  - VM は対象外（監査 sink と `VmExperimental` の同時設定は build 時に拒否）。
  - **範囲外（初版 R-AT-09「60 KiB 縮退」を削除）**: 60 KiB 縮退・`EventTooLarge` は正確な byte 推定が
    前提のため strict CBOR スライスへ延期し、R-1 の受入基準から外した（上記「逸脱」）。

- **テスト配置**: `tests/audit_redaction.rs`（R-AT-01〜04・06・09）。R-AT-05 は既存
  `tests/audit_*.rs` の sequence/yield 位置/encoded 推定長の不変 ＋ payload 期待値更新で担保。R-AT-07 の
  compile-fail は `src/audit.rs`（または `src/lib.rs`）の `compile_fail` doctest。R-AT-08 は
  `src/audit.rs` の unit test（見積もり据え置きの確認）。
  - **compile-fail doctest の偽陽性対策**: `compile_fail` doctest は「どんな理由で compile が落ちても成功」
    になるため、型が参照できない（非公開・パス誤記）だけで通ってしまう。これを防ぐため doctest は次の 2 本を
    1 ペアにする: (1) `compile_fail` 版——**公開経路から `SecretBytes` を参照**し（例えば
    `tsumugi::RedactionPolicy` 経由、または `SecretBytes` を公開 export して参照）、その値に
    `format!("{:?}", ..)` 等 `Debug` を要求して**落ちる**ことを固定する。(2) 通常 doctest 版——**同じ参照
    経路で、通るべき操作**（`Clone` や byte 長取得など `Debug` 以外）を実際に compile・実行し、「型には
    手が届くが `Debug`/`Display`/serialize だけが無い」ことを担保する。(2) が通ることで (1) の失敗が
    「型が見えない」ではなく「`Debug` が無い」由来であることを保証する。

- **R-1 レビュー反映の記録（採用・不採用とその理由）**: 初版の R-1 定義に対する semantic review を
  1 周通し、現行コード（`src/audit.rs` / `src/builtin_core.rs` / `src/embedding.rs` / `src/eval.rs`）と
  仕様 §7・§9〜§11・§15 を突き合わせた指摘を次のとおり処理した。将来この slice を実装・改訂する際の
  判断履歴として残す。

  **採用した指摘（定義を改めた）**
  - **byte 推定の非破壊根拠が成り立たない（最重要）**: 初版は「`original_bytes` を保てば encoded 長も
    yield 位置も不変」としたが、`estimate_event_bytes` は host-call 系を payload 非依存の 128 固定で
    見積もっており（`src/audit.rs` の `_ => 128`）、`original_bytes` は payload 中の値であって encode 後の
    長さではない。見積もりを payload 依存に変えると B-2 の queue 会計で yield 位置が動く。**対応**: R-1 では
    見積もりを**据え置く**（payload の中身だけ §9.1 ツリーへ差し替え、見積もり長は 128 固定のまま）。
    **据え置きを選んだ理由**: レビューは「書き直す／範囲から外す」の 2 択を許したが、naive 推定全体の
    正確化は既に延期済みの strict CBOR スライスが扱う領域で、そこへ寄せれば B-2 の回帰更新を 1 回で済ませ、
    R-AT-05 の非破壊が本当に成立する。見積もりを R-1 で変えると初版の自己矛盾に逆戻りする。
  - **「現状 payload は空」という事実誤認**: 実際は `host_call_triple`（`src/builtin_core.rs`）が `body`/
    `path` を `Omitted` にした field を既に積んでおり、空 `fields` は `eval.rs` のユニットテストだけ。
    **対応**: 冒頭記述を実態（`Omitted` プレースホルダ・`path.original_bytes` は 0 固定）へ修正。
  - **変更箇所 `src/builtin.rs` は `src/builtin_core.rs` の誤記**: **対応**: 修正。あわせて canonical
    resource label を `host_call_triple` へ渡す新規配線と、`path.original_bytes` を実 label 長へ改める
    作業を変更箇所へ明記（初版は欠落）。
  - **既定 `hmac_key` のジレンマ（セキュリティ判断）**: 固定埋め込みは公開鍵同然で §9.1 の辞書攻撃耐性が
    消え、乱数生成は §7.1（ambient 乱数源に触らない）と R-AT-06（決定性）を壊す。**対応**: R-1 の既定 key は
    固定値（alpha 用・秘匿性なし。raw path を直接載せないことだけを担保し、key 公開ゆえ推測復元には
    耐えない）と割り切り、秘匿性のある host 注入経路を sink 必須化スライスへ延期、と逸脱に明記。
    **この割り切りを選んだ理由**: R-1 の主目的は raw path をそのまま載せないことであり、固定 key でもそれは
    達成できる。推測復元に耐える秘匿性は公開 API（key 注入経路）の設計を伴うため sink 必須化と同じスライスに
    まとめる方が設計が一貫する（「相関除去」という当初表現は 3 周目で不正確と認め、下記のとおり言い直した）。
  - **公開 API の破壊的変更に触れていない**: `AuditPayload`/`AuditPayloadField` は `src/lib.rs` で公開
    export 済み。**対応**: 差し替えが破壊的変更であること、B-1 と同じく alpha として記録の上（記録は [組み込みAPI仕様](embedding-api.md) §13.1。当初は roadmap 明記。判断役が選択（issue #318））
    受容し `language_revision` bump は無いことを明記。
  - **invalid-path 時の HMAC 対象が未定義**: **対応**: canonical label が無い場合は Path field を
    `Omitted{class:Path}` にし HMAC しない、を確定規則として追加（raw path は leak させない）。
  - **`policy_id` の決め方が曖昧**: **対応**: §9.1 の型どおり host 指定 `String`（内容ハッシュでない）と
    明記し、既定を `"default-r1"` に。R-AT-04 を「注入値をそのまま反映」に統一。
  - **R-AT-07 が §15.4 より弱い**: 初版は「値が漏れない」だけだった。**対応**: §15.4 の compile-fail
    （`Secret`/`SecretBytes` が `Debug`/`Display`/serialize 不可）を R-AT-07 に含め強度を揃えた。
  - **ロードマップの一貫性**: 初版は「直近完了」に未マージの slice 定義を入れ、「次のタスク」に既定モードの
    詳細を書いていた。**対応**: F-1 の docs PR に倣い「次のタスク」中心の更新へ寄せ、詳細は spec 参照に留める
    （本コミットで roadmap を再修正）。

  **不採用・範囲外にした指摘（理由つき）**
  - **60 KiB 縮退規則（初版 R-AT-09）を R-1 で実装する**: 縮退判定は event の実 encode 長に対する判定で、
    正確な byte 推定が前提。R-1 は見積もりを据え置くため faithful に実装できない。**不採用の理由**: 見積もり
    据え置きと両立しないため strict CBOR スライスへ延期し、受入基準から外した（レビューの指摘 1 対応の
    論理的帰結）。
  - **見積もり関数を「変わる前提に書き直す」案（指摘 1 の第一選択肢）**: レビューが許した 2 択のうち
    採らなかった方。**不採用の理由**: 上記「据え置きを選んだ理由」のとおり、strict CBOR スライスへ寄せる方が
    B-2 回帰を 1 回で済ませられ非破壊が成立するため。

  **2 周目レビューで採用した指摘（上記定義へ反映済み）**
  - **R-AT-02 と invalid-path ルールの矛盾**: 初版改訂で invalid-path を `Omitted` にしたのに R-AT-02 は
    「deny でも Path は HmacSha256」のままだった。コード（`dispatch_fs`/`dispatch_filesystem_capability`）を
    確認すると label が無い deny は invalid-path だけでなく「未 grant」「mount 未登録・未 grant・許可外」も
    該当。**対応**: canonical label が得られるのは allow 時だけと一般化し、R-AT-01（allow=HmacSha256）と
    R-AT-02（deny 3 経路=Omitted・raw 非露出）に書き分けた。
  - **label の渡し方がコード構造と不整合**: triple は `dispatch_fs` が返った後に `fs_result` から組み立てて
    おり、label は `dispatch_fs` 内に閉じている。**対応**: `dispatch_fs` の戻り値を label 同伴型へ拡張し
    呼び出し側が triple へ渡す配線を変更箇所に明記（非監査経路は label を無視し観測挙動不変）。
  - **trybuild ハーネスが存在しない**: dev-deps は criterion のみ。3 OS CI で trybuild の stderr snapshot は
    壊れやすい。**対応**: R-AT-07 の compile-fail を `trybuild` ではなく `compile_fail` doctest で固定する
    方針に変更（依存を増やさず OS 差に強い）。
  - **`SecretBytes` の Debug 未実装で `Engine`/`EngineBuilder` の `#[derive(Debug)]` が壊れる**: **対応**:
    `RedactionPolicy` に key を伏せた手書き `Debug` を実装し derive を保つ、と変更箇所に明記。`SecretBytes`
    自体は `Debug`/`Display`/serialize 非実装（R-AT-07 の compile-fail で固定）。
  - **128 固定見積もりは上限保証でない**: payload が HMAC digest/Identifier の実データを含むと 128 を
    超えうる。**対応**: `max_encoded_bytes`/60 KiB 判定が strict CBOR まで概算依存（過小評価あり）である旨を
    逸脱に明記（R-1 の payload 規模では実害なし）。
  - **「相関除去」は不正確**: 固定 key では同一 label→同一 digest で相関は残り、key 公開なので path は推測
    復元されうる。**対応**: 既定 key の目的を「raw path をそのまま載せない」に正し、推測復元に耐える秘匿性は
    得られないと明記。
  - **記録場所の表現ミス**: 導入文の「§14 末尾」を「本小節末尾」に修正。

  **3 周目レビューで採用した指摘（上記定義へ反映済み）**
  - **host error 経路で label を取りこぼす**: 「allow なら `Some`、deny なら `None`」では
    `HostCallOutcome::HostError`（route/open 成功後に adapter が失敗。`host_call_triple` の分類で
    Allow/`HostError`、`fs_result` は `Err`）が抜ける。コード（`builtin_core.rs` の fs operation は
    `fs_open`/`fs_route` 成功後に `read_to_end`/`write_all` 等が失敗し `fs_host_error`）で確認。
    **対応**: label の有無は decision で決まり outcome の成否と独立と整理し、`dispatch_fs` 戻り値を
    `Err` でも label を運ぶ型へ拡張、Path は Allow（Success/HostError 両方）で `HmacSha256`。R-AT-01 に
    host error ケースを明示的にテスト対象として追加。
  - **compile_fail doctest の偽陽性**: 型が参照できない（非公開・パス誤記）だけで compile が落ちても
    `compile_fail` は成功になる。**対応**: (1) 公開経路から `SecretBytes` を参照して `Debug` 要求で落ちる
    `compile_fail` 版と、(2) 同じ経路で `Debug` 以外（`Clone` 等）が通る通常 doctest 版を 1 ペアにし、
    失敗が「型が見えない」でなく「`Debug` が無い」由来であることを担保する、とテスト配置に明記。
  - **直し漏れ・不整合**: (a)「逸脱」節の `hmac_key` に「相関除去のみ」が残っていた → 「raw path を直接
    載せない／推測復元には耐えない」に統一。(b)「opt-in・非破壊」節の Path 記述が旧ルール（常に
    `HmacSha256`）のまま → Allow=`HmacSha256`/Deny=`Omitted` に、`path.original_bytes` も Allow/Deny/
    引数型エラー別の実長に修正。(c) `original_bytes` の定義ぶれ（§9.1 の canonical subtree 長 vs Path の
    label/path byte 長）→ strict CBOR までは UTF-8 byte 長で代用すると一文で固定。(d) 引数型エラー
    （`require_str` 失敗で path 文字列が無い）の `original_bytes` 未定義 → `0` と定義。
  - **canonical resource label の中身（好みの範囲・採用）**: R-AT-01 の「同じ label」を明確にするため、
    label を routing 後の `FilesystemTarget`（mount 名＋mount 相対 path を `@<mount>/...` 正規形へ）と
    一文で固定した。host absolute path・CWD・symlink 解決結果は含めない。

  **4 周目レビューで採用した指摘（上記定義へ反映済み、いずれも書き方の精密化）**
  - **label 確定は「route 成功時点」であって「open の後」ではない**: `fs_route` が `FilesystemTarget` を
    返した時点で label が決まり、`fs_open` 内の `adapter.open_file` 失敗はその後に起きる（`builtin_core.rs`）。
    **対応**: field class 判定と変更箇所を「route 成功時点で label 確定、open 失敗を含む host error でも
    label あり」に直し、実装は label を `fs_route` 直後に受け取ると明記。
  - **「Deny は label 確定前に拒否」は全経路には当てはまらない**: Upsert（`write_file`/`append_file`）の
    Create 権限不足は、`fs_route`（Write 代表 op）成功で label が決まった**後**に `filesystem_denied` に
    なる（`builtin_core.rs` の `fs_open`）。**対応**: Path のルールを「label の有無」ではなく
    「**decision が `Deny` なら label が手元にあっても `Omitted`**」という判定基準に書き直し（「label が
    あれば HMAC してよい」と読み違えるのを防ぐ）。R-AT-02 に Upsert 経路 (d) を「label があるのに Omitted」の
    テストケースとして追加し、`original_bytes` は Deny では requested path 長で統一（label 長を漏らさない）と
    定義。

  **5 周目レビューで採用した指摘（実装 PR #238 のレビュー反映、上記定義へ反映済み）**
  - **`arguments` field の形**: 初版実装は `arguments` を `Revealed { class: Body, value: Sequence([]) }`
    にしていたが、§9.1 で Body が `Revealed` になるのは allow-list に載り上限内のときだけであり、形の上で
    「Body を開示した」と読めてしまう（値は空なので leak はない）。**対応（R-1 の確定規則）**: host-call
    引数の `fields["arguments"]` は外側を `Revealed { class: Public, value: Sequence([...]) }`（開示するのは
    引数の個数と並びだけ）とし、各要素は引数の種類に応じて `Omitted { class: Path }`（path 引数）または
    `Omitted { class: Body }`（書き込み内容などの自由文）に畳む。path の HMAC は `resource` 側だけで持ち、
    `arguments` 側では HMAC しない。各要素の `original_bytes` はその引数の UTF-8 byte 長（R-1 の代用規則）。
    これにより R-1b の allow-list Reveal を要素単位で差し込める。
  - **`rename` の resource 2 field は R-1 から外し後続 slice へ延期**（6 周目で撤回・下記参照）: 5 周目で
    `rename` の resource を `source_path` / `destination_path` の 2 field にすると定義したが、これは
    「`rename` も監査 triple を出している」という誤前提に基づいていた。実際は triple を emission するのは
    I/O byte 課金対象の 4 builtin（`read_file` / `read_lines` / `write_file` / `append_file`）だけで、
    `rename` は初版（R-1 着手前）から監査対象外である。`rename` を監査対象へ追加すると emission 範囲
    （event 件数・sequence・queue byte 会計）が変わり、その変更が §7.2/§8/§14 に未記載になるうえ、
    capability で判定される他の fs builtin（`mkdir` / `remove` 系 / `list_dir` / `file_size` / `is_file` /
    `is_dir` / `path_exists`）が監査対象外のまま `rename` だけ対象になる非対称を生む。**対応**: `rename` の
    2 field 規則は R-1 の受入基準から外し、下記「逸脱」の「fs builtin 全体の監査範囲」slice へ延期する
    （その slice で `rename` を対象化するときの 2 field 設計は合意済みとして引き継ぐ）。
  - **Secret class の畳み規則を本スライスで入れる**: 初版実装は `AuditFieldClass::Secret` を扱う構築経路が
    無く、`AuditField::Revealed { class: Secret, .. }` を作れてしまった。R-AT-07 は「Secret class の field は
    存在と byte 数だけが記録される」を要求する。**対応**: class と mode から `AuditField` を組む小さな helper
    を `audit.rs` に置き、Secret は mode に関係なく常に `Omitted` にする。`host_call_triple` の body/path も
    この helper を経由させ、畳み規則を 1 か所へ集約する。unit test を添える。
  - **`identifier_mode` を payload へ反映する（ユーザー判断 A＝(イ)）**: slice 定義の「function 名・capability
    名・rule ID は既定 `Reveal`（`Revealed { class: Identifier, .. }`）」を実装にも反映する。初版は
    function/capability を event の素の String のままにしていた（payload の Identifier 二重化なし）。**対応**:
    host-call payload 側にも既定 policy の `identifier_mode` を適用した `Revealed { class: Identifier, value,
    original_bytes }` を載せ、slice 文面と実装を一致させる。
  - **test shim の `debug_assert!(reveal_body_fields.is_empty())` を撤去**: allow-list 非空 policy を注入
    すると debug build で panic し、ホスト安定性原則に反する。**対応**: assert を外し、「R-1 は allow-list を
    無視して常に Omit する」ことをコメントで明記する（R-1b で Reveal 経路を有効化する）。
  - **公開面の整理**: `RedactionPolicy::hmac_path` は spec に無い HMAC 計算の口なので `pub(crate)` にする。
    `path_mode` が HMAC 以外のとき `Omitted` へ畳む fail-safe 挙動を `RedactionPolicy` の doc に明記する。

  **6 周目レビューで採用した指摘（実装 PR #238 の 2 周目レビュー反映、上記定義へ反映済み）**
  - **`rename` の監査対象化を R-1 から外す（1 周目指摘 4 の誤前提の訂正）**: 1 周目レビューの指摘 4
    （`rename` の resource を 2 field に）は「`rename` も監査 triple を出している」という前提だったが、これは
    誤りだった。初版（R-1 着手前）から triple を emission するのは I/O byte 課金対象の 4 builtin
    （`read_file` / `read_lines` / `write_file` / `append_file`）のみで、`rename` は監査対象外である。
    2 周目修正で `is_audited_host_call` を新設して `rename` を監査対象へ追加したが、これは R-1 の範囲
    （既存 payload の中身を §9.1 ツリーへ差し替える最小縦切り）を超える emission 範囲の変更で、§7.2/§8/§14
    に未記載であり、他の capability 判定 fs builtin が対象外のまま `rename` だけ対象になる非対称も生む。
    **対応（ユーザー決定）**: `is_audited_host_call` を撤去し `is_host_call` の元判定へ戻す。`rename` 専用に
    入れた label 運搬（`AuditResourceLabels::Rename` など）と `rename` テストも R-1 から外す（未使用コードを
    残さない。単一 path 操作の label 運搬は R-1 に必要な形へ整理する）。5 周目で R-AT-01/02・本記録に書いた
    `rename` 2 field 規則は受入基準から外し、下記「逸脱」の「fs builtin 全体の監査範囲」slice へ移す。
  - **`arguments` と Identifier field のテスト追加**: `arguments` の構造（外側 `Revealed { class: Public }`・
    各要素の class・`original_bytes`）と payload に載った Identifier field（request の `function`、resource の
    `capability` / `rule_id` / `action`）を検査するテストが無かった。**対応**: `src/builtin_core.rs` の
    `host_call_triple_tests` に read 系（要素 1 つ）と write 系（Path + Body の 2 要素）のケースを
    追加し、raw path や書き込み内容が要素に出ないことも確認する。

#### R-1b 実装スライス（tree engine、opt-in、body allow-list Reveal）

> 状態: **未実装**（設計のみ）。本小節は R-1a が型だけ用意して常に `Omitted` に畳んでいる body
> allow-list Reveal 経路（`reveal_body_fields` と `max_revealed_body_bytes > 0` による `audit_safe`
> field の明示開示）を有効化する最小縦切り（R-1b）の slice 定義である。仕様（本章 §9/§9.1）が正本で
> あり、以下は R-1b で実装する範囲と意図的に延期する逸脱の記録である。R-1a 小節はその時点の記録として
> 触らない（過去のスライス小節を後から直さない）。現在の逸脱・未実装範囲は冒頭の逸脱一覧が正本であり、
> R-1b の done 実装 PR で body allow-list Reveal を未実装範囲から外す。

R-1a は §9.1 の `AuditField` ツリーと `RedactionPolicy`（`reveal_body_fields` /
`max_revealed_body_bytes` を含む）を型として用意したが、body を扱う畳み規則
（`src/audit.rs::fold_audit_field` の `AuditFieldClass::Body` 分岐）は `body_mode` に関わらず
**常に `Omitted`** にしており、allow-list を無視している（R-1a の確定規則・コメントで明記済み）。
R-1b はこの 1 か所の畳み規則を §9.1 の Body Reveal 条件へ差し替え、allow-list に載った body field を
上限内でだけ `Revealed` にする。R-1a と同じ opt-in（sink あり経路）・tree backend だけを対象とし、
新しい公開型・CBOR tag・`language_revision` の変更は行わない。

- **粒度は R-1b（body Reveal 経路の有効化のみ）**: R-1a が常に `Omitted` に畳んでいた Body class を、
  §9.1 の条件——`body_mode == Reveal` **かつ** field の `AuditFieldKey` が `reveal_body_fields` に
  ある **かつ** `original_bytes <= max_revealed_body_bytes`（`max_revealed_body_bytes > 0`）——を
  すべて満たすときだけ `Revealed { class: Body, value, original_bytes }` にする。1 つでも満たさなければ
  R-1a と同じく `Omitted { class: Body, original_bytes }` のままとする。filesystem host-call
  （tree 唯一の監査対象境界、R-1a と同じ 4 builtin〔`read_file` / `read_lines` / `write_file` /
  `append_file`〕）の `request` body・`response` body・`arguments` 各要素（自由文の書き込み内容）を対象に
  する。path 引数（`AuditFieldClass::Path`）・Secret・Identifier・Public の畳み規則は R-1a のまま一切
  変えない。

- **allow-list のキー（§9.1 どおり、新設しない）**: Reveal 可否は §9.1 の `AuditFieldKey`
  （`{ operation: String, direction: AuditDirection, field: String }`）で引く。R-1b はこの型を
  そのまま使い、新しいキー型を作らない。各単位の割り当ては次のとおり固定する。
  - `operation` = host function 名（R-1a が `request.function` / `resource.action` に載せている
    Identifier と同じ文字列。`read_file` / `read_lines` / `write_file` / `append_file`）。
  - `direction` = その body がどちらの payload に現れるかで決める。`request` body と `request` の
    `arguments` 要素は `AuditDirection::Request`、`response` body は `AuditDirection::Response`。
  - `field` = payload の key。`request`/`response` の自由文 body は `"body"`、`arguments` の各要素は
    §9.1 の「name または 0 始まり argument index」どおり **0 始まりの index の 10 進文字列**
    （`"0"`、`"1"`、…）とする。R-1a の `arguments` は arg[0]=path・arg[1]=書き込み内容なので、Body の
    Reveal 対象になり得るのは write/append の `"1"` だけである（arg[0] は Path class で対象外）。
  - **根拠**: §9.1 は `AuditFieldKey` を allow-list の要素型として既に定め、`field` を「name または
    0 始まり argument index」と規定している。R-1b はこの規定を host-call payload の実際の key 名
    （`"body"`）と `arguments` の index 文字列へ機械的に対応づけるだけで、新たな識別子体系を導入しない。
    operation に host function 名を使うのは、R-1a が同じ名前を payload の Identifier として既に露出して
    いるため、host が allow-list を書くときに参照できる安定ラベルになるからである。

- **`max_revealed_body_bytes` の比較対象と上限超過時の表現（§9.1）**: 比較対象は当該 field の
  `original_bytes`、すなわち **redaction 前の canonical subtree 長**（R-1a の代用規則どおり、strict CBOR
  スライスまでは値の UTF-8 byte 長で代用する。body は原本文字列の byte 長）とする。`original_bytes >
  max_revealed_body_bytes` の body は §9.1 どおり **truncation せず subtree 全体を `Omitted { class:
  Body, original_bytes }`** にする。prefix も 1 byte も残さない（prefix を残すと secret が漏れうるため）。
  §9.1 の「`truncated = true` ではなく `omitted = true` と byte 数を記録する」は、R-1a の `AuditField`
  表現では「variant が `Omitted` であること（truncation variant を持たない）」と「`original_bytes` に
  原本 byte 長が入ること」で満たす。`max_revealed_body_bytes` の既定は 0 なので、既定 policy では
  `original_bytes <= 0` を満たす body が無く（空 body の `original_bytes` も 0 だが、空 body は
  allow-list に載っても Reveal する値が無い）、実質すべて `Omitted` のままになる。
  - **根拠**: §9.1 が「上限を超えた body は truncation せず全体を omit」「prefix だけを残して secret を
    漏らさない」と明記している。比較対象を `original_bytes`（redaction 前の subtree 長）にするのは、
    §9.1 が Body Reveal の条件を「上限内」と定め、`original_bytes` を「redaction 前の canonical subtree
    payload 長」と定義しているため、両者の突合が仕様の意図どおりになる。

- **class 間の優先順位（§9.1、allow-list より class 規則が優先）**: Reveal 条件を満たしても、次の
  class 規則が allow-list より優先し、body Reveal には **到達しない**。
  - **Secret は常に `Omitted`**（mode 変更不可・走査しない、§9.1）。allow-list に載っていても Secret
    class の field は `fold_audit_field` の Secret 分岐で先に `Omitted` になる（R-1a の畳み順を変えない）。
  - **Path は decision 由来の規則が優先**（Allow=`HmacSha256` / Deny=`Omitted`、R-1a の R-AT-01/02）。
    `arguments` の path 引数（arg[0]）は Path class なので、allow-list に `"0"` を載せても body Reveal の
    対象にならない（Path 分岐で処理され Body 分岐へ来ない）。
  - **64 段を超える nesting は `Body`/`Omitted` へ縮退**（§9.1）。縮退した subtree は Body class の
    `Omitted` になり、allow-list に載っていても Reveal しない（縮退先が `Omitted` 固定であることは §9.1 の
    既定であり、R-1b はこれを Reveal へ昇格させない）。R-1a の filesystem payload は `arguments` が 1 段
    （`Sequence` の要素はスカラ）なので 64 段には達しないが、縮退規則と allow-list の優先関係を本スライスで
    確定しておく。
  - 優先順位をまとめると **Secret > Path（decision 規則）> 64 段縮退 > body allow-list Reveal** であり、
    body Reveal は「Body class として `fold_audit_field` の Body 分岐に到達し、かつ §9.1 の 3 条件を満たす」
    ときだけ起きる。
  - **根拠**: §9.1 は「Secret の親は必ず subtree 全体を `Omitted`」「Body は `body_mode == Reveal` かつ
    key が allow-list にあり上限内の場合だけ `Revealed`」「64 段を超える subtree は Body/Omitted へ縮退」
    と、class 規則を Body Reveal 条件の前段として規定している。R-1b はこの順序をそのまま実装し、allow-list
    を class 規則の後ろに置く。

- **`redaction_policy_id` の扱い（R-1a の実配線を踏襲）**: `ExecutionStarted.redaction_policy_id` は
  R-1a どおり注入 policy の `policy_id`（§9.1 の host 指定 `String`）を**そのまま**反映する。allow-list が
  非空の policy でも R-1b は `policy_id` の決め方・既定値・format に手を入れない（既定 policy は allow-list
  が空のままなので `"default-r1"` を維持する）。allow-list 非空 policy に特定の `policy_id` を要求するか、
  allow-list の内容を `policy_id` へ反映するか（例: 内容ハッシュを含める）は §9.1 の「`policy_id` は host が
  付ける安定ラベル・内容ハッシュではない」に反し得る API/format の約束になるため、本スライスでは決めず
  PR 本文の「判断待ち」へ回す。
  - **根拠**: §9.1 が `policy_id` を「host が付ける安定ラベル（内容ハッシュではない）」と定義し、R-1a の
    R-AT-04 が「注入値をそのまま反映」を確定している。R-1b は Body の畳み規則だけを変え、この契約は維持する。

- **policy 注入経路は R-1a のまま（公開 setter を新設しない）**: R-1b は facade の既定 policy
  （`RedactionPolicy::default()`、allow-list 空・`max_revealed_body_bytes = 0`）を変えない。allow-list を
  非空にした policy を注入する経路は、R-1a が置いた `#[doc(hidden)] pub` の test shim
  （`EngineBuilder::audit_redaction_policy_for_test`）をそのまま使い、受入基準のテストもこの shim 経由で
  allow-list 入り policy を注入する。host が production で allow-list 付き policy を設定する**公開 API**
  （`EngineBuilder::audit_redaction_policy` などの安定 setter）は、選択肢があり公開契約を増やすため、
  R-1a と同じく sink 必須化（§1.1）スライスへ延期し、R-1b では決めない（詳細は PR 本文「判断待ち」）。
  - **根拠**: R-1a の slice 定義が「公開 setter は追加しない・sink 必須化スライスへ延期」「test shim で
    注入」を確定している。R-1b は Body Reveal の意味論を有効化する縦切りであり、公開 API の設計を伴わない。

- **opt-in・非破壊（既定 policy で R-1a と 1 byte も変わらない）**: sink なし経路（`run_guarded`）は
  redaction を一切行わず、観測結果・出力 byte・exit code は不変（R-1a のまま）。sink あり経路でも、既定
  policy（allow-list 空・`max_revealed_body_bytes = 0`）では §9.1 の Reveal 条件を満たす body が存在せず、
  `fold_audit_field` の Body 分岐は R-1a と同じ `Omitted` を返す。したがって既定 policy での payload・
  `redaction_policy_id`・`original_bytes`・envelope 列は R-1a と **byte 単位で完全一致**する（R1b-AT-06 で
  固定）。挙動が変わるのは allow-list を非空にした policy を test shim 経由で注入したときだけで、その場合も
  R-1a の byte 推定据え置き（host-call event は 128 固定見積もり）を維持するため、sequence・yield 位置・
  B-2 の queue byte 会計は不変である（payload の**中身**だけが `Omitted` から `Revealed` へ変わる）。

- **byte 推定は R-1b でも据え置く（§9.2、非破壊の前提）**: R-1b は `estimate_event_bytes`
  （`src/audit.rs`）を**変更しない**（R-1a と同じ理由。host-call lifecycle event は payload 非依存の
  128 固定見積もりのまま）。Body を Reveal しても見積もり長は 128 固定で動かないため、B-2 の queue byte
  会計・yield 位置は allow-list の有無に関わらず不変になる。payload を実サイズで見積もる正確化は strict
  deterministic CBOR（§9.2）スライスへ延期する（R-1a と同じ割り切り）。

- **逸脱（R-1b 時点で残るもの、後続スライスへ延期）**:
  - **公開 setter**（`EngineBuilder::audit_redaction_policy`）と `ConfigError` への接続は sink 必須化
    （§1.1）スライスへ延期する（R-1a と同じ）。R-1b の allow-list 注入は test shim 経由だけ。
  - **allow-list 非空 policy の `policy_id` 契約**（特定値の要求・内容反映の有無）は決めず、PR 本文の
    「判断待ち」へ回す。R-1b は R-1a の「注入値をそのまま反映」を維持する。
  - **byte 推定の正確化**（`estimate_event_bytes` を payload 依存の実 encode 長へ）は strict CBOR
    （§9.2）スライスへ延期する。Reveal された body が 128 を超える encode 長になっても、R-1b は見積もりを
    据え置くため §11 の `max_encoded_bytes` / 60 KiB 判定は 128 概算に対して行われ、strict CBOR まで
    不正確なまま（R-1a と同じ逸脱）。
  - **60 KiB 縮退規則**（§9 の Body → Path → Identifier 縮退・`EventTooLarge`）は正確な byte 推定が
    前提のため strict CBOR スライスへ延期する（R-1a と同じ）。Reveal した body が大きい場合でも R-1b は
    `max_revealed_body_bytes` 上限（超過時は全体 `Omitted`）で有界化し、縮退規則には依存しない。
  - **fs builtin 全体の監査範囲**（`rename` ほか非課金 fs builtin への監査 triple 拡張）は別 slice
    （R-1a の逸脱と同じ「fs builtin 監査範囲」slice）。R-1b は R-1a と同じ 4 builtin だけを対象とする。
  - **VM 対応**（Slice 6）。監査 sink と `VmExperimental` の同時設定は従来どおり build 時に拒否する。
  - **record/replay の redaction 再適用**（§12）は record/replay スライス（§6）へ延期する。

- **変更箇所（見込み）**:
  - `src/audit.rs`: `fold_audit_field` の `AuditFieldClass::Body` 分岐を、§9.1 の 3 条件
    （`body_mode == Reveal` ∧ key ∈ `reveal_body_fields` ∧ `original_bytes <= max_revealed_body_bytes`
    かつ `max_revealed_body_bytes > 0`）を満たすとき `Revealed { class: Body, value, original_bytes }`、
    満たさなければ `Omitted { class: Body, original_bytes }` にする。allow-list 判定に必要な
    `AuditFieldKey`（`operation` / `direction` / `field`）と Reveal する `value` を呼び出し側から
    受け取れるよう、Body 用の畳み口（`fold_audit_field` の引数追加または Body 専用 helper）を整える。
    Secret / Path / 縮退の畳み規則は変えない。`RedactionPolicy` の Body Reveal 条件を doc に明記する。
  - `src/builtin_core.rs`: `host_call_triple` の `body_field` helper が、`request`/`response`/`arguments`
    の各 body について `AuditFieldKey`（host function 名・direction・key 名または index 文字列）と
    原本の `AuditValue`（Reveal される場合に載せる自由文の `AuditScalar::Utf8` 等）を組んで Body の
    畳み口へ渡すようにする。allow-list に無い・上限超過・既定 policy のときは従来どおり `Omitted`。
    path 引数（Path class）・Identifier・Public の経路は変えない。
  - `src/embedding.rs` / `src/eval.rs`: 変更なし（policy は R-1a の受け渡し経路をそのまま使う。
    test shim・`redaction_policy_id` 配線は R-1a のまま）。

- **受入基準（R-1b）**:
  - **R1b-AT-01（allow-list + 上限内で Reveal）**: `body_mode = Reveal`・当該 body の `AuditFieldKey` を
    `reveal_body_fields` に含め・`max_revealed_body_bytes` を body 長以上にした policy を注入すると、その
    body field が `Revealed { class: Body, value, original_bytes }` になり、`value` に原本の自由文が、
    `original_bytes` に redaction 前の byte 長（R-1b の代用規則＝UTF-8 byte 長）が入る（§9.1）。
  - **R1b-AT-02（上限超過は全体 Omit・truncation しない）**: `original_bytes > max_revealed_body_bytes`
    の body は、allow-list に載り `body_mode = Reveal` でも `Omitted { class: Body, original_bytes }` に
    なる。prefix を含め原本の byte が payload に 1 byte も現れず、`original_bytes` に原本長が入る（§9.1
    「truncation せず全体 omit・prefix を残さない」）。
  - **R1b-AT-03（allow-list 外は Omit）**: `AuditFieldKey` が `reveal_body_fields` に無い body は、
    `body_mode = Reveal`・上限内でも `Omitted { class: Body }` のままになる。`direction` 違い・`field`
    違い・`operation`（host function 名）違いのいずれでもキーが一致しなければ Reveal しない（§9.1）。
  - **R1b-AT-04（class 優先順位）**: allow-list に載っていても (a) Secret class は常に `Omitted`、
    (b) `arguments` の path 引数（arg[0]＝Path class）は decision 規則（Allow=`HmacSha256`/Deny=`Omitted`）に
    従い body Reveal に到達しない、(c) 64 段を超える nesting で Body/Omitted へ縮退した subtree は Reveal
    しない。優先順位 Secret > Path > 64 段縮退 > body allow-list Reveal を満たす（§9.1）。
  - **R1b-AT-05（`body_mode != Reveal` では Reveal しない）**: allow-list に載り上限内でも、
    `body_mode = Omit`（既定）または `body_mode = HmacSha256` の policy では body が `Revealed` にならない
    （§9.1 の 3 条件は `body_mode == Reveal` を含む）。既定 policy（`body_mode = Omit`）がこのケースを兼ねる。
  - **R1b-AT-06（既定 policy で R-1a と byte 一致・非破壊）**: 既定 policy（allow-list 空・
    `max_revealed_body_bytes = 0`・`body_mode = Omit`）では、payload・`redaction_policy_id`・
    `original_bytes`・envelope 列・sequence・yield 位置が R-1a と **1 byte も変わらない**。既存
    `tests/audit_redaction.rs` / `tests/audit_core.rs` / `tests/audit_hostcall.rs` /
    `tests/audit_yield_resume.rs` / `tests/audit_backpressure.rs` / `tests/audit_queue_bytes.rs` が
    無改変で緑。sink なし経路の出力 byte・exit code が不変。既存テストの期待値が変わる場合は実装を止めて
    報告する。
  - **R1b-AT-07（決定性）**: 同じ script・同じ `hmac_key`・同じ allow-list 入り policy では、Reveal 結果と
    envelope 列が 100 回の実行で一致する（§15.1）。Reveal は原本値をそのまま載せるだけで乱数源に触れない。
  - **R1b-AT-08（byte 計上の不変）**: R-1b は `estimate_event_bytes` を据え置くため、allow-list の有無・
    Reveal の有無に関わらず host-call event の push 時計上 byte（128 固定 + envelope 固定部）が R-1a/B-2 と
    変わらず、journal の `committed_bytes` 増分とも一致し続ける（B2-AT-10 の不変条件を維持）。
  - VM は対象外（監査 sink と `VmExperimental` の同時設定は build 時に拒否）。

- **テスト配置**: `tests/audit_redaction.rs`（R1b-AT-01〜05・07 を allow-list 入り policy の test shim
  注入で追加）。R1b-AT-06 は既存 `tests/audit_*.rs` の無改変緑 ＋ 既定 policy payload の期待値不変で担保。
  R1b-AT-08 は `src/audit.rs` の unit test（見積もり据え置きと Body 畳み規則）で担保。allow-list 判定・
  class 優先順位の畳み規則は `src/audit.rs::fold_audit_field` と `src/builtin_core.rs::host_call_triple_tests`
  の unit test に Reveal/Omit の分岐ケースを追加する。

#### audit-strict-cbor の sub-slice 分割

> 状態: 設計のみ（本小節以下 4 小節は設計時点の記録）。現在の逸脱・未実装範囲は冒頭の逸脱一覧が正本である。

`slices.toml` の `audit-strict-cbor`（strict deterministic CBOR、§9.2）は、次の 4 つを含む。

1. §9.2 の encoder 本体（schema v1 の全型の deterministic CBOR と、§11 の size pass / write pass）
2. journal・audit drain queue の byte 計測を naive 推定（`estimate_encoded_bytes` / `estimate_event_bytes`、host-call 系は 128 固定）から encoder の実長へ差し替えること
3. `original_bytes` と Path の HMAC 入力を §9.1/§9.2 の「redaction 前 subtree の canonical encoding」へ揃えること
4. §7.2 の 60 KiB 縮退規則（Body → Path → Identifier の `Omitted` 化、`EventTooLarge`、Terminal の `Emergency` 置換）

まとめると `src/` の差分は 700 行を超え、触る仕組みも encoder・journal/queue 会計・redaction・縮退の 4 つになる。[開発ループ手順書](loop-runbook.md)「スライスの大きさの目安」（400 行・仕組み 1 つ・受入基準 5 項目）に収まらないため、c1〜c4 の 4 スライスに分ける。元の id `audit-strict-cbor` は置き換えて残さない。「strict CBOR スライス」と書かれた既存の延期先（R-1a/R-1b/B-2 小節、逸脱一覧）は c1〜c4 全体を指す。

| スライス | 仕組み | 依存 | 受入基準 |
|---|---|---|---|
| audit-strict-cbor-c1 | encoder 本体（`src/audit.rs` 内、crate 内部） | A-1 | SC-AT-01..05 |
| audit-strict-cbor-c2 | journal・queue の byte 計測の差し替え | c1 | SC-AT-06..10 |
| audit-strict-cbor-c3 | `original_bytes`・HMAC 入力の canonical encoding 化（redaction） | c1 | SC-AT-11..15 |
| audit-strict-cbor-c4 | 60 KiB 縮退規則と Terminal `Emergency` | c2・c3 | SC-AT-16..20 |

c1 は byte 計測にも redaction にも配線しないため、c1 だけでは観測できる挙動は変わらない。挙動（`committed_bytes`・yield 位置・digest・`original_bytes`）が変わるのは c2 以降である。

#### audit-strict-cbor-c1 実装スライス（§9.2 encoder 本体と size pass）

> 状態: **未実装**（設計のみ）。仕様（§9.1・§9.2・§11）が正本であり、以下は c1 で実装する範囲と延期する範囲の記録である。

- **実装範囲**:
  - §9.2 の deterministic CBOR encoder を `src/audit.rs` の中に置く（新しい依存 crate は足さない。SHA-256/HMAC と同じく自前実装）。primitive（unsigned/negative integer の最短幅、定長 text/byte string、array header、bool/null）と、§9.2 の合成規則（struct = 宣言順 array、enum = `[tag, ...]`、`Option` = `[0]` / `[1, v]`、`BTreeMap` = `[[k, v], ...]`、Vec = 元順、`i128` = 16 byte BE byte string、Float = 8 byte BE byte string、u128 の `execution_id` = 16 byte BE byte string、`MonotonicInstant` = u64 tick のみ）を実装する。`HostTimestamp` は field 1 つの struct なので、§9.2 の一般規則どおり 1 要素の array `[bstr(16)]`（array header 1 + byte string header 1 + 16 = 18 byte 固定）として encode する。
  - schema v1 の全型に encode を与える: `AuditEnvelope`・`HostTimestamp`・`AuditEvent`（8 variant、tag 0〜7）・`AuditYieldReason`（`AdmissionPhase` を含む）・`Backend`・`ExecutionMode`・`CapabilityDecisionKind`・`HostCallOutcome`・`EffectStatus`・`BudgetChargeReason`・`BudgetResource`・`TerminalOutcome`・`BudgetConfig`・`BudgetUsage`・`BudgetCounters`・`BudgetPeaks`・`AuditErrorPayload`・`AuditError`・`AuditFrame`・`AuditPayload`・`AuditField`・`AuditFieldClass`・`AuditValue`・`AuditScalar`。tag は各 enum の Rust 宣言順（§7.2・§9.1 の宣言順と同じ）を 0 始まりで振る。fieldless enum（`AuditFieldClass` など。field を持つ variant は常に `[tag, field...]`）は §9.2 の一般規則どおり `[tag]` で encode する（下記の判断 Q1 = A）。`BudgetConfig` 系の field 順は [実行予算・協調実行仕様](execution-control.md) の宣言順（現行 `src/budget.rs` と一致）を使う。
  - §11 の 2 pass を 1 つの encoder で持つ。同じ走査を「長さだけ数える出力」（size pass）と「`Vec<u8>` へ書く出力」（write pass）に通す形にし、size pass は checked arithmetic で長さを返す（overflow は `None` / error）。別実装の size 推定を持たない（§9.2 末尾）。
  - 入口は crate 内部（`pub(crate)`）の 2 関数程度に絞る（例: envelope を encode して bytes を返す関数、event 本体と envelope 固定部の長さを返す関数）。c1 時点では本番経路から呼ばれないため、未使用警告を c1 の間だけ局所的に許す（`#[cfg_attr(not(test), expect(dead_code))]` など。c2 で使われた時点で外れる書き方にする）。
- **範囲外（逸脱・延期）**:
  - journal・queue の byte 計測の差し替えは c2。c1 の間は逸脱一覧の「128 固定見積もり」行と未実装範囲の「strict deterministic CBOR」行はそのまま残る（c1 の done PR では逸脱一覧を変えない）。
  - `original_bytes`・HMAC 入力は c3、60 KiB 縮退と `Emergency` は c4。
  - **encoder の公開**（host が envelope の canonical bytes を得る公開関数、sink へ bytes を渡す API）は行わない。`AuditSink` は従来どおり `Arc<[AuditEnvelope]>` を受け取る。公開面は sink 必須化（§1.1）または record/replay（§6）のスライスで、record store の fingerprint（§9.2 `RecordedHostRequest`）とあわせて決める。`RecordedHostRequest` の encode も c1 の範囲外（record/replay スライス）。
  - `AuditFrame.module_id` / `AuditError.module_id` は現行の型（`String` / `Option<String>`）のまま encode する（逸脱一覧の該当行）。`AuditField` への型変更は c1 の範囲外。
  - VM は対象外。
- **判断 Q1（kamonabe の選択: A）**: §9.1 の class（`AuditFieldClass` などの field を持たない enum。`AuditField` の variant は field を持つので常に `[tag, field...]` で、Q1 の対象外）を、§9.2 の一般規則どおり `[tag]`（array、例 `0x81 0x00`、2 byte）で encode するか、§9.1 の文言どおり bare u8（例 `0x00`、1 byte）で encode するか。変更前の spec は 2 通りに読め、どちらを採るかで全 `AuditField` の byte 列・`original_bytes`・digest・journal の byte 会計が変わる。
  - A（推奨）: 一般規則どおり `[tag]`。例外の無い 1 つの規則で済み、encoder も検証も単純になる。根拠: manifesto 原則 8（小さく理解可能な中核）・原則 5（予測可能性）、§9.2 の一般規則。
  - B: §9.1 の文言どおり bare u8。`AuditField` 1 つあたり 1 byte 小さいが、fieldless enum の中で class だけが例外になる。根拠: §9.1 の文言。
  - **選択: A**（kamonabe の選択、2026-10-24、PR #256）。§9.1 と本小節の実装範囲へ反映した。
- **受入基準（c1）**:
  - **SC-AT-01（primitive の golden）**: unsigned integer の幅の境界（0・23・24・255・256・65535・65536・2^32−1・2^32・u64::MAX）、負の i64（−1・−24・−25・i64::MIN）、空と非空の text / byte string、`Option` の `None` / `Some`、`i128`（0・−1・i128::MIN・i128::MAX）の 16 byte BE、Float bits の 8 byte BE が、手で組んだ期待 byte 列と一致する（RFC 8949 §4.2.1 の最短幅・定長）。
  - **SC-AT-02（schema v1 の全型の golden）**: `AuditEvent` の 8 variant と、上記の全 enum の全 variant について、§9.2 の tag と field 順で組んだ期待 byte 列と一致する。`AuditEnvelope` は `[schema_version, execution_id(16), source_hash(32), language_revision, sequence, timestamp, event]` の順になる。
  - **SC-AT-03（size pass と write pass の一致）**: SC-AT-01/02 の全ケースと、極値（全 u64 field が u64::MAX、長い text、64 段 nesting の `AuditValue`）で、size pass の長さが write pass の byte 長と一致する。size pass の加算は checked で、overflow を検出できる（unit test で overflow 経路を直接呼ぶ）。
  - **SC-AT-04（決定性）**: 同じ値は 100 回 encode しても同じ byte 列になる。`BTreeMap` / `AuditValue::Object` は挿入順に関係なく UTF-8 key byte 順で出る。clock instance だけが異なる（tick は同じ）`MonotonicInstant` を含む `BudgetConfig` は同じ byte 列になる（§9.2・§15.1）。
  - **SC-AT-05（非破壊）**: 既存の `tests/audit_*.rs` が無改変で緑。sink なし経路・sink あり経路とも観測結果（envelope 列・`committed_bytes`・yield 位置・出力 byte・exit code）が変わらない（c1 は計測にも redaction にも配線しない）。
- **想定の変更規模**: `src/audit.rs` に約 350〜400 行（primitive と 2 pass で約 100 行、残りは型ごとの定型の encode）。テストは `src/audit.rs` の unit test（golden の期待 byte 列）。他の `src/` ファイルは変えない。仕組みは encoder 1 つ、受入基準は 5 項目。
- **テスト配置**: SC-AT-01〜04 は `src/audit.rs` の unit test（`cbor_` 接頭辞）。SC-AT-05 は既存の `tests/audit_*.rs` の無改変緑。

#### audit-strict-cbor-c2 実装スライス（journal・audit drain queue の byte 計測を encoder の実長へ差し替え）

> 状態: **未実装**（設計のみ）。c1 が done になってから着手する。

- **実装範囲**:
  - journal（`AuditJournal::append_normal` / `append_terminal`）が計上する byte を、c1 の encoder で測った envelope の実 encode 長にする（§11「encoded byte数は9.2節のdeterministic CBORでencodeした実byte長」）。append の前に size pass で長さを求めて budget 判定に使い、write pass を行う場合はその長さと一致しなければ `AuditFailure::InvariantViolation`（internal audit failure、fail-closed）とする。
  - audit drain queue（B-2、`AuditQueueMeter`）は、sequence 確定前の event を「envelope 固定部の上限値（sequence を u64 の最大幅 9 byte とみなす。timestamp は `[bstr(16)]` の 18 byte 固定、`language_revision` は execution 内で不変）＋ event 本体の実 encode 長」で計上する（B-2 小節「逸脱」に書いた方針どおり）。ack 時は push 時に足した値と同じ値を引く。
  - `estimate_encoded_bytes` / `estimate_envelope_overhead_bytes` / `estimate_event_bytes` を encoder の size pass へ置き換える（関数名を残すかどうかは実装者に任せる。naive 推定の実装は削除する）。
  - `can_reserve_host_call` の 60 KiB / close reserve の判定は現行どおり（定数による上限値）とし、変えない。
- **範囲外（逸脱・延期）**: `original_bytes`・HMAC は c3、60 KiB 縮退は c4。§11 の pre-effect reservation（逸脱一覧の該当行）と関数内の queue soft limit（B-2 の逸脱）は変えない。VM は対象外。
- **c2 の done PR で行う逸脱一覧の更新**: 「128 固定見積もり」の行と未実装範囲の「strict deterministic CBOR」行を削除する。
- **受入基準（c2）**:
  - **SC-AT-06（journal の実長）**: Started・host-call triple・`BudgetCharged`・`Yielded`/`Resumed`・Terminal を含む実行で、journal の `committed_bytes`（Terminal は reserve 側）が各 envelope の c1 encoder による byte 長の和と一致する。Reveal された body を含む host-call event は payload に応じた長さで計上される（128 固定でない）。
  - **SC-AT-07（queue の上限値計上）**: B2-AT-10 を置き換える。各 event の push 時計上値は、同じ event を journal へ append したときの増分以上で、その差は sequence の encode 幅の差（0〜8 byte）に限られる。flush 後に queue は 0 へ戻る。
  - **SC-AT-08（実長での上限判定）**: `max_encoded_bytes` を小さく注入した実行で、実長の合計が通常枠を超えた時点で `AuditFailure::BudgetExceeded`、単一 event の実長が通常枠を超えると `EventTooLarge` になり、Terminal は予約枠で必ず append される（§10.1・§11）。
  - **SC-AT-09（回帰の更新範囲）**: 既存の `tests/audit_*.rs` は、byte 値に依存する期待値（`committed_bytes` の値、`tests/audit_queue_bytes.rs` の yield が起きる位置）だけを更新し、それ以外（event の中身・sequence・triple の完全性・§8 の順序）は無改変で緑。sink なし経路は不変。
  - **SC-AT-10（決定性）**: 同じ script・同じ注入 budget・同じ sink 応答列で、`committed_bytes` と yield 位置と envelope 列が 100 回一致する（§15.1）。
- **想定の変更規模**: `src/audit.rs`・`src/eval.rs`・`src/embedding.rs`・`src/budget.rs` で約 80〜150 行（推定関数の置き換えと呼び出し側）。仕組みは監査の byte 会計 1 つ。

**`ready` にする設計 PR の確かめ**（2026-11-25。c1 が done（実装 PR #261）になった後の main 4d12771 で確かめた。上の実装範囲・範囲外・受入基準・想定の変更規模は変えない。
判断の候補 Q5・Q6 を設計 PR #519 に出した。選ばれた内容はこの下に追記する）:

- c1 の encoder は `src/audit.rs` の `pub(crate) mod cbor` にあり、c2 が使う入口は `cbor::encoded_len`（size pass、checked）・`cbor::encode_to_vec`
  （size pass の長さと write pass の byte 長が違えば `CborError::LengthMismatch`）・`cbor::envelope_fixed_len_upper_bound(language_revision)`
  （sequence を `u64::MAX` の 9 byte とみなした envelope 固定部の上限値。上の queue の計上規則のための入口）の 3 つである。
  `cbor` モジュールの `#![cfg_attr(not(test), expect(dead_code))]` は c1 の間だけの許可なので、c2 で本番経路から使った時点で外す（残る未使用の項目があれば項目ごとに許す）。
- 置き換える呼び出し点は次のとおり（main 4d12771）。journal: `AuditJournal::append_normal` の `estimate_encoded_bytes(envelope)`。
  queue: `Evaluator::audit_queue_admit` / `audit_queue_release`（`src/eval.rs`。`estimate_event_bytes`）、`AuditQueueMeter.envelope_overhead` を張る
  `src/embedding.rs` の `set_audit_queue` の呼び出し（`estimate_envelope_overhead_bytes`）、`BudgetChargeAccumulator::push_finalized` /
  `finalized_queue_bytes`（`src/audit.rs`。確定済み `BudgetCharged` の queue 計上）。release は push と同じ規則で event から測り直すので、
  envelope 固定部の上限値が execution の中で不変である限り「push 時に足した値と同じ値を引く」を満たす。
- `append_terminal` は現在 byte を数えていない（event 数・byte の通常枠を検査せず、sequence だけを払い出す）。SC-AT-06 の「Terminal は reserve 側」の
  数え方は判断の候補 Q6 にした。
- size pass の長さが `u64` を超える（`CborError::LengthOverflow`）event は、その長さが通常枠（`u64` 以下）を必ず超えるので、journal では §11 と
  SC-AT-08 の「単一 event の実長が通常枠を超える」に当たり `AuditFailure::EventTooLarge` とする。queue の計上は現行の飽和加算と同じく `u64::MAX`
  に飽和させる（その event の push で queue は上限を超え、yield・backpressure の既存の経路に入る）。これは上の実装範囲から従うもので、判断ではない。
- naive 推定そのものを固定している `src/` の unit test（`src/audit.rs` の R-AT-08 `r_at_08_estimate_event_bytes_frozen_for_host_call_events` と
  host-call event の `estimate_event_bytes == 128` の検査、`src/audit.rs`・`src/eval.rs` の B2-AT-10 の検査）は、推定の実装を削除するのにあわせて
  SC-AT-06（journal の実長）・SC-AT-07（B2-AT-10 の置き換え）の unit test へ置き換える。SC-AT-09 の「無改変で緑」は `tests/audit_*.rs` についての条件で、
  これらの unit test は対象外である（R-AT-08 は「R-1 では 128 固定を据え置く」ことの確かめで、strict CBOR スライスで naive 推定全体を差し替えることは R-1 小節が予定している）。
- `estimate_encoded_bytes` は `pub fn` で、`pub mod audit` を通して `tsumugi::audit::estimate_encoded_bytes` として crate の外から呼べる（crate root の
  `pub use` には無い）。上の実装範囲は「関数名を残すかどうかは実装者に任せる」としているが、公開 API に当たるので判断の候補 Q5 にした。
  `estimate_envelope_overhead_bytes` / `estimate_event_bytes` は `pub(crate)` なので実装者に任せたままでよい。
- 規模は手順書 §14.1 の目安（`src/` 1000 行以内・仕組み 1 つ・受入基準 5 項目以内）に収まる。`src/budget.rs` は呼び出し点に無く、変えずに済む見込み
  （上の見積もりの範囲に含めたままでよい）。

**判断 Q5・Q6 の記録**（判断役が選択（PR #519、Q5=A、Q6=A）。2026-11-25 の設計 PR #519 の追加の commit で反映）:

- **Q5（公開関数 `tsumugi::audit::estimate_encoded_bytes` の扱い）= A**: 公開面から外す（削除するか `pub(crate)` にする。crate 内は `cbor::encoded_len` を使う）。
  外から呼べる関数を消す破壊的変更なので、c2 の実装 PR で [組み込みAPI仕様](embedding-api.md) §13.1 の表に 1 行記録する（表の文言は crate root の公開型についてだが、
  公開 API の変更の記録を 1 か所に置く原則 9 に沿って同じ表に書く）。上の実装範囲の「関数名を残すかどうかは実装者に任せる」は、この関数については
  この決定で置き換わる（`pub(crate)` の 2 関数は実装者に任せたまま）。理由: manifesto 原則 8（名前が「estimate」のまま実長を返す関数や、encoder の長さだけを
  先に見せる公開関数を増やさない）。c1 小節の範囲外「encoder の公開は行わない」（公開面は sink 必須化・record/replay のスライスで決める）と食い違わないのは A だけ。
  alpha の破壊的変更は §13.1 への記録を条件に受容される。採らなかったもの: B（名前と意味が食い違う関数を公開面に残し、encoder の公開を事実上先に行う）、
  C（公開関数を新しく足し、後続へ送った公開面の判断を c2 で先取りする）。
- **Q6（Terminal の byte の数え方）= A**: `append_terminal` で Terminal の実長を size pass で測り、journal の非公開 field（例 `terminal_bytes`）に記録する。
  上限とは比べず、c4 までは reserve（`terminal_reserve_bytes`）を超えても append する。SC-AT-06 の「Terminal は reserve 側」は、`committed_bytes + terminal_bytes` が
  全 envelope の実長の和と一致することで確かめる。公開 API は変わらない。理由: §11「Started と Terminal も encoded bytes へ含める」「Terminal は専用 reserve だけを使い、
  通常上限を使い切っても必ず append できる」と §10.1 を両方満たすのは A だけで、SC-AT-06 を c2 の中で検証でき受入基準を動かさずに済む（manifesto 原則 7・原則 8）。
  採らなかったもの: B（§11 の「Terminal も数える」が c4 まで満たされず、受入基準の範囲が変わる）、C（Terminal を欠いた監査列を作り、原則 6・7 と §10.1・§11 に反しうる）。
  - **残る逸脱**: c4 までの間は「Terminal の実長が `terminal_reserve_bytes` を超えても append する」ことが §11 の reserve に対する逸脱として残る
    （Terminal が reserve を超えうる状態は c2 の前から同じで、A が新しく上限を緩めるわけではない。60 KiB 縮退と Terminal の `Emergency` は c4、SC-AT-18）。
    c2 の done PR は、上の「c2 の done PR で行う逸脱一覧の更新」に加えて、逸脱一覧にこの行（§11、延期先 c4）を足す。c4 の done PR でその行を削除する。

#### audit-strict-cbor-c3 実装スライス（`original_bytes` と Path HMAC 入力を canonical encoding へ）

> 状態: **未実装**（設計のみ）。c1 が done になってから着手する。判断 Q2・Q3 は kamonabe の選択（Q2 = A、Q3 = 3a・3b・3c とも A）で決まり、以下の実装範囲はその選択で書いている（§9.1 にも反映済み）。

- **実装範囲**:
  - `AuditField` の `original_bytes` を、その field の redaction 前の値（`AuditValue`）の §9.2 encoding の byte 長にする（Q3a = A）。入れ子の値は（Q3b = A）、子 field をすべて `Revealed`（各子の class はそのまま、子自身の `original_bytes` も同じ規則で下から求めた値）とした redaction 前 subtree を encode して測る。`AuditValue` へ写せない子は `Omitted { class, original_bytes: 0 }` のまま置く（要素は落とさず、Sequence の長さと argument index を保つ）。対象は R-1a/R-1b の host-call payload（Path・Body・`arguments` の各要素と外側の `Sequence`）。
  - Path の HMAC-SHA-256 の入力を、canonical label の生 byte から「label を値とする `AuditValue::Scalar(AuditScalar::Utf8(label))` の §9.2 encoding」へ変える（§9.1「field payload全体」・§9.2 末尾。Q3a = A）。Deny で label を使わない規則（R-1a）は変えない。
  - R-1b の `max_revealed_body_bytes` との比較は新しい `original_bytes`（CBOR の header を含む長さ）で行う（Q3c = A）。
  - 引数の型エラーで `AuditValue` へ写せない引数（文字列でない引数など）は、現行どおり `original_bytes = 0` の `Omitted` とする（Q2 = A、§9.1）。
- **範囲外（逸脱・延期）**: 60 KiB 縮退は c4。`AuditFrame.module_id` の型は変えない。VM は対象外。
- **c3 の done PR で行う逸脱一覧の更新**: 「`original_bytes` を UTF-8 byte 長で代用」の行と「HMAC 入力が label の生 byte」の行を削除する。
- **判断 Q2（kamonabe の選択: A）**: 引数の型エラーで引数が文字列でないとき（現在は `original_bytes = 0`）、`original_bytes` をどうするか。script の値（Int・List・関数など）を `AuditValue` へ写す規則は spec に無い。
  - A（推奨）: 0 のままとし、§9.1 に「`AuditValue` へ写せない引数（型エラー）は `original_bytes = 0` の `Omitted`」と明記する。型エラーの引数は Path/Body として意味を持たず、写像の規則を足さずに済む。根拠: manifesto 原則 8・原則 5、§9.1。
  - B: script の値を `AuditValue` へ写す規則（Null/Bool/Int/Float/String → Scalar、List → Sequence、Dict → Object、関数などは型名）を §9.1 に足し、その encode 長を測る。§9.1 の定義を文字どおり満たすが、record 用の Value encoding（§9.2 `RecordedHostRequest`）と別の写像が増える。
- **判断 Q3（kamonabe の選択: 3a・3b・3c とも A）**: redaction 前 subtree の encoding の定義（§9.1「field payload全体」・§9.2「redaction前subtree」はどちらにも読める。`AuditField::Revealed` 全体は `original_bytes` を含み循環するので候補から外す）。
  - **3a. Path の HMAC 入力**: A（推奨）`AuditValue::Scalar(AuditScalar::Utf8(label))` の encoding（field の値として他の field と同じ規則で測れ、`original_bytes` と同じ入力になる。根拠: §9.1「field payload全体」、manifesto 原則 8）。B: label の CBOR text 単体（`AuditValue`/`AuditScalar` の tag を含まない。§9.1「canonical resource label」に近い）。どちらでも digest の値は現行から変わる。
  - **3b. 入れ子の `original_bytes`**: A（推奨）子 field をすべて `Revealed`（各子の class はそのまま）にし、子の `original_bytes` も同じ規則で下から求めた subtree の encode 長（子を単独で測った値と親の中の子の値が一致し、決定的。根拠: §9.1「redaction前のcanonical subtree payload長」、manifesto 原則 5）。B: 子の `original_bytes` を 0 とみなして測る（再帰が無く単純だが、親の中の子の表現が実際の Revealed 子と一致しない）。
  - **3c. R-1b の上限比較**: A（推奨）CBOR header を含む新しい `original_bytes` と比較する（§9.1 の「上限内」と `original_bytes` が同じ量を指し、比較対象が 1 つで済む。Reveal の境界が header 分ずれる。根拠: §9.1、R-1b 小節の「比較対象は `original_bytes`」）。B: 原本の UTF-8 byte 長と比較する（host が設定する上限の意味が R-1b のまま変わらない。比較用の長さを `original_bytes` と別に持つ）。
- **受入基準（c3）**:
  - **SC-AT-11**: Path（Allow / Deny）・Body（write/append の内容、read の response）・`arguments` の各要素の `original_bytes` が、redaction 前の値の §9.2 encoding の長さと一致する（例: 5 byte の ASCII 文字列は `Scalar(Utf8)` の encode 長）。
  - **SC-AT-12**: Path の digest が `HMAC-SHA-256(hmac_key, deterministic_cbor(Scalar(Utf8(label))))` と一致する（既知の key・label による test vector）。同じ key・label なら一致し、key か label が違えば変わる（§15.4）。
  - **SC-AT-13**: R-1b の上限判定が新しい `original_bytes` で行われる。上限と等しい body は Reveal され、1 byte 超えると全体 `Omitted` になる（境界が CBOR header 分ずれることを固定する）。
  - **SC-AT-14**: canary を入れた path・body が encode 後の byte 列に平文で現れない（§15.4）。同じ入力では 100 回一致する。
  - **SC-AT-15**: 外側の `arguments`（`Sequence`）の `original_bytes` が、子をすべて `Revealed` とした redaction 前 subtree の encode 長と一致する。型エラーの引数（`AuditValue` へ写せない引数）を含む `arguments` では、その子を `Omitted { class, original_bytes: 0 }` として測った長さと一致する。
- **想定の変更規模**: `src/audit.rs`・`src/builtin_core.rs` で約 60〜120 行。既存の R-1a/R-1b のテストの `original_bytes` と digest の期待値を更新する。仕組みは redaction 1 つ。

#### audit-strict-cbor-c4 実装スライス（60 KiB 縮退規則と Terminal `Emergency`）

> 状態: **未実装**（設計のみ）。c2・c3 が done になってから着手する。判断 Q4 は kamonabe の選択（A: class ごと）で決まり、以下の実装範囲はその選択で書いている（§7.2 にも反映済み）。

- **実装範囲**:
  - 通常 event の実 encode 長が 60 KiB を超える場合、§7.2 のとおり Body → Path → Identifier の順に、class ごとにまとめて（Q4 = A。その class の `AuditField` をすべて）`Omitted { class, original_bytes }` へ縮退して測り直し、それでも超える event は `AuditFailure::EventTooLarge`（fail-closed）とする。縮退は event を評価器 buffer へ積む前（queue 計上の前）に行い、queue と journal は縮退後の長さを使う。Secret と、`AuditField` でない field（関数名・capability 名・error code など）は縮退の対象にしない。
  - Terminal は、`Full(AuditError)` を含む Terminal 全体の実長が 60 KiB を超える場合に `Emergency { code, message_id, full_error_bytes, full_error_sha256 }` へ置き換える。`full_error_bytes` と `full_error_sha256` は redaction 済み `AuditError` の §9.2 encoding に対して求める。Emergency の Terminal 全体は 4 KiB 以下になる（code / message_id は各 128 byte まで）。
- **範囲外**: `max_revealed_body_bytes` の既定や allow-list の規則は変えない。VM は対象外。
- **c4 の done PR で行う逸脱一覧の更新**: 未実装範囲の「60 KiB 縮退規則」行を削除する。
- **判断 Q4（kamonabe の選択: A）**: §7.2「Body/Path/Identifierの順にOmittedへ縮退」をどの粒度で行うか。
  - A（推奨）: class ごと。event 内の Body をすべて `Omitted` にして測り直し、まだ超えれば Path をすべて、次に Identifier をすべて `Omitted` にする。段は最大 3 つで、結果が入力から一意に決まり、実装・検証が単純。根拠: manifesto 原則 5・原則 8、§7.2 の文言の素直な読み方。
  - B: field ごと。同じ class の中で大きい field から 1 つずつ `Omitted` にし、60 KiB 以下になったら止める。残る情報は多いが、同じ長さのときの順番などの規則を新しく決める必要があり、測り直しの回数も増える。
- **受入基準（c4）**:
  - **SC-AT-16**: Reveal で 60 KiB を超える Body を持つ event が、Body の `Omitted` 化で 60 KiB 以下に収まり、`original_bytes` は保持される。
  - **SC-AT-17**: Body を縮退しても超える場合は Path、次に Identifier を縮退する。全部縮退しても超える event は `EventTooLarge` で AuditFailure terminal になる（§10.1）。
  - **SC-AT-18**: Terminal の `Full` が 60 KiB を超える場合に `Emergency` になり、`full_error_bytes`・`full_error_sha256` が `AuditError` の encoding と一致し、Terminal 全体が 4 KiB 以下になる。
  - **SC-AT-19**: 60 KiB 以下の event は縮退されず、c3 時点と byte 単位で一致する（既存テストは無改変で緑）。
  - **SC-AT-20**: 同じ入力では縮退結果と envelope 列が 100 回一致する（§15.1）。
- **想定の変更規模**: `src/audit.rs`・`src/eval.rs`（または event を積む箇所）・`src/embedding.rs` で約 100〜180 行。仕組みは縮退 1 つ。

#### fix-audit-sink-reentrancy-guard 実装スライス（sink callback 中の同じ Engine への再入検出）

> 状態: **未実装**（設計のみ）。loop:drift #263 D-9（issue #263 の Q2=A、返信なしのため推奨案で確定）で足した `fix` スライス。§10 の「sink callback中に同じEngineの…を呼んではならない。thread-local reentrancy guardで検出し`AuditSinkError::Reentrant`にする」と §15.3 の「sink callbackのEngine再入を拒否し、deadlockしない」を満たす。この設計 PR（#281）の判断待ち 2 つ、PR #281 の Q1（再入した呼び出しへ何を返すか・どの入口を対象にするか）と PR #281 の Q2（入れ子の callback で再入の印をどの実行に付けるか。レビュー指摘 2）は、返信なしのため推奨案で確定した（[開発ループ手順書](loop-runbook.md) §7.5）: Q1=A・Q2=A。以下の実装範囲と受入基準はこの選択で書いており、選んだ内容は下の「本文への反映」のとおり目標の章へ書いた。

設計時点（main `b7ce903`）の事実:

- `AuditSinkError::Reentrant` は型だけで、生成する箇所が無い（`src/audit.rs` の `AuditSinkError`）。
- facade の `Engine`（`src/embedding.rs`）は `Send + Sync`（同ファイルのコンパイル時アサーション）で、`Send + Sync` の部品（`Arc<dyn AuditSink>`・`Arc<HostFunctionRegistry>`）だけを持ち、内部に lock を持たない。sink は `EngineBuilder::audit_sink` で渡すため、sink が `Arc<Engine>` を後から受け取れば、callback の中から同じ Engine の入口を呼べる。
- sink の `submit` を呼ぶのは次の 3 か所で、どれも run を呼んだ thread の上で同期に呼ぶ: `run_audited_inner` の Started の submit と、Started 失敗時の Terminal の submit、`AuditDelivery::deliver`（host-call triple・`Yielded`/`Resumed`・`BudgetCharged`・Terminal の配送と、backpressure 中の同じ batch の再送）。
- facade の公開入口は `compile`・`link`・`link_for_repl`・`run`・`run_for_repl`・`run_audited` の 6 つである。§10 が挙げる start/poll/pause/resume/cancel は scheduler の `ExecutionHandle`（`src/engine.rs`）の操作で、scheduler の `Engine` は sink を持たない（逸脱一覧の B-1 の行）。facade では start と poll は run 系 3 入口の中にあり、pause/resume の入口は無い。cancel は `CancellationToken`（`Send + Sync` の flag）で、Engine の入口ではない。host function と別の audit emit は、run 系の入口を通らなければ呼べない。

- **実装範囲**:
  - `src/audit.rs` に thread-local の guard を置く。中身は「この thread で今 sink callback の中にいる Engine の列」で、列の要素ごとに `EngineId` と「その Engine への再入が試みられたか」の印を持つ（PR #281 の Q2=A。印を thread で 1 つにはしない）。sink の `submit` を呼ぶ 3 か所を、guard に入って（自分の要素を積んで）`submit` を呼び、出るときに自分の要素の印を確かめる 1 つの helper に通す。guard から出る処理は RAII（`Drop`）で行い、sink が panic して unwind した場合も列から外れるようにする。別の Engine の sink callback が入れ子になった場合も、入った順に積み、出るときに自分の要素だけを外す。同じ Engine の要素は列に高々 1 つである（その Engine の callback の中では同じ Engine の実行が拒否され、2 つ目の要素を積む submit に届かないため）。
  - facade の 6 入口は、最初に「この thread で、この Engine（`self.id`）の要素が列にあるか」（列の先頭だけでなく、入れ子の下の要素も含む）を確かめる。あれば、何も実行せずに拒否し、その要素にだけ印を立てる。拒否の返し方（PR #281 の Q1=A）: `run`・`run_for_repl` は `ExecutionOutcome::InternalFailure`、`run_audited` は `AuditedOutcome::Outcome(InternalFailure)`、`compile` は `CompileDiagnosticCode::InternalFault` の診断 1 件の `CompileErrors`、`link`・`link_for_repl` は `LinkError::InternalFailure`。この検査は poison・running の検査より前に行い、渡された context の `running`・`poisoned`・binding を変えない（同じ context への再入を拒否する既存の検査と同じく、拒否で context を poison しない）。
  - `submit` から戻ったとき自分の要素に印が立っていれば、sink が返した値（`Ack` でも）を捨てて `AuditSubmit::Failed(AuditSinkError::Reentrant)` として扱う。以降は既存の `Failed` と同じ経路を通る: Started なら script work を行わず `Terminal(AuditFailure)` を emergency slot へ append して `AuditFailed { failure: AuditFailure::Sink }`、実行途中・Terminal の配送なら `AuditFailure::Sink` で fail-closed（§10.1）。`AuditFailure` に variant は足さない（`AuditSinkError` の他の variant と同じく `Sink` に畳む）。入れ子で、Engine A の callback の中で実行した別の Engine B の callback から A へ再入した場合、印が立つのは A の要素だけなので、B の `submit` は sink が返した値どおりに扱い（B の実行は通常どおり進む）、A の `submit` から戻ったときに A の実行だけが fail-closed になる。
  - `AuditSinkError::Reentrant` の doc comment から「A-1 では検出配線なし」を外す。
- **範囲外**:
  - 別の thread からの呼び出しは thread-local guard の対象外であり、検出しない（§10 の「thread-local reentrancy guard」どおり）。sink が別 thread で同じ Engine を使っても、facade の `Engine` は lock を持たないため deadlock はしない。
  - scheduler の `Engine`（`src/engine.rs`）と `ExecutionHandle` は sink を持たないため対象外。sink を持つ scheduler 経路は E10 統合のスライスで扱う。
  - `CancellationToken::cancel` は止めない（別 thread からの cancel を受けるための flag で、execution-control の契約どおり callback からも呼べる）。
  - `AuditWaker`・`AuditCompleter::complete` から呼ばれる waker の中の再入（[実行予算・協調実行仕様](execution-control.md) の waker 契約）は扱わない。
  - sink の panic の捕捉方法（Started の submit が `catch_host_unwind` の外にあること）は変えない。guard が unwind で外れることだけを保つ。
  - VM は対象外（監査 sink と VM の同時設定は build で拒否される）。
- **本文への反映（この設計 PR のマージ前に行う）**: 再入した呼び出しが何を返すか・どの入口を対象にするかは公開 API の挙動なので、スライス小節だけに置かず目標の章に書く。判断待ち（PR #281 の Q1 と Q2）が確定したら（[開発ループ手順書](loop-runbook.md) §7.3 の 4、返信が無い場合は §7.5）、同じ PR に追加の commit で次を書く。判断待ちの間は本文を変えない（未確定の選択を確定として書かないため）。Q1=A・Q2=A の確定（§7.5）を受けて、同じ PR の追加の commit で次を書いた。
  - (a) §10 の再入の規則: 対象の入口と、再入した呼び出しへ返す値。§10 の「start/poll/pause/resume/cancel」が scheduler の `ExecutionHandle` の操作を指すこと、facade の入口との対応、`CancellationToken::cancel` を callback から呼べるかどうか（小節の範囲外の扱いを本文に写す）。入れ子の callback でどの実行を fail-closed にするか（Q2=A: 再入された Engine の実行だけ）。`ExecutionHandle` の操作で拒否に返す値は、Q1 の選択肢が facade の 6 入口だけを扱ったため決めず、sink を scheduler 経路へ配線するとき（E10 統合）に決めると書いた。
  - (b) [組み込みAPI仕様](embedding-api.md) の再入の規則（§9 の「同じcontext/handleへの再入は許さず`InternalFailure`とする」の段落）と §11 の規則 2: `compile`・`link` の拒否に使う値（Q1=A なので `CompileDiagnosticCode::InternalFault`・`LinkError::InternalFailure`。§11 の規則 2 はこれらを panic の写像としてだけ定義していた）。拒否で context を poison しないことと、§10 の規則 4 との関係も書いた。
  - Q1・Q2 とも推奨案 A になったため、本小節の実装範囲と受入基準は Q1 の選択肢を変えずに保ち、Q2=A に合わせて guard の印の持ち方（実装範囲の 1 項目め・3 項目め）と RG-AT-04（入れ子の A・B それぞれの実行結果）を確定させた。
- **done PR で行う逸脱一覧の更新**: 逸脱一覧の「`AuditSinkError::Reentrant` は型だけで…」の行を削除し、表の下の段落から D-9 についての記述（「`AuditSinkError::Reentrant` が型だけである行（D-9）」と、D-9 の理由の文）を削除する。D-8 についての記述は残す。
- **判断 Q1（PR #281 の Q1。返信なしのため推奨案 A で確定、[開発ループ手順書](loop-runbook.md) §7.5）**: §10 は「呼んではならない」「guard で検出し `AuditSinkError::Reentrant` にする」とだけ書き、再入した呼び出しそのものに何を返すか、facade のどの入口を対象にするかを決めていない。どれを選ぶかで公開 API の挙動（と、B では公開型）が決まる。
  - A（推奨）: facade の 6 入口すべてを対象にし、既存の InternalFailure 系（上記）で拒否する。公開型を増やさない。根拠: [組み込みAPI仕様](embedding-api.md)「同じcontext/handleへの再入は許さず`InternalFailure`とする」、[Capability Model仕様](capability-model.md) §11.3 の host callback の再入禁止と同じ扱いになる。§10 の列挙に compile が含まれる。外側の execution は `Reentrant` で fail-closed になり、host は `AuditFailed` で契約違反を観測できる。manifesto 原則 7（監査可能）・原則 8（小さく理解可能な中核）。
  - B: A と同じ入口を対象にし、拒否を表す新しい公開 variant（`ContextError`／`LinkError`／`CompileDiagnosticCode` に `Reentrant` 相当）を足す。再入した呼び出しの側でも理由を区別できるが、公開 API が 3 つ増え、外側の `AuditFailed` と情報が重なる。
  - C: run 系の 3 入口だけを対象にし、`compile`・`link`・`link_for_repl` は許す（audit を emit せず、execution の状態を持たないため）。変更は小さいが、§10 の列挙（compile）を直す必要がある。
- **判断 Q2（PR #281 の Q2。設計レビューの選択肢ありの指摘 2。返信なしのため推奨案 A で確定、[開発ループ手順書](loop-runbook.md) §7.5。issue #263 の Q2 とは別）**: Engine A の callback の中で別の Engine B を実行し、B の callback が A へ再入したとき、どの実行を fail-closed にするか。印が thread で 1 つか、列の要素ごとかで決まり、公開の挙動（どちらの実行が `AuditFailed` を返すか）にかかわる。
  - A（推奨）: 印を「callback 中の Engine の列」の要素ごとに持ち、再入された Engine（A）の要素にだけ立てる。A の `submit` だけが `Reentrant` になり、B は通常どおり進む。根拠: RG-AT-03「別の Engine は許す」と同じ扱いになり、B は自分の sink 契約に違反していないため失敗させない。§10 の「同じEngine」への再入だけを禁じる文言どおり。manifesto 原則 6（失敗を制御可能な結果にする: 失敗するのは契約に違反した Engine の実行だけで、host がどの実行が失敗したかを特定できる）・原則 7（監査可能）。
  - B: 印を thread で 1 つ持つ。最初に戻る B の `submit` が `Reentrant` になり、A も失敗させるかを別に決める必要がある。
- **受入基準**:
  - **RG-AT-01（Started の submit 中の再入）**: sink の callback から同じ Engine の `run_audited` を新しい context で呼ぶと、その呼び出しは `AuditedOutcome::Outcome(InternalFailure)` を返し、script を 1 文も実行しない（stdout 出力・host call が 0 件、渡した context は poison されない）。外側の実行は `AuditFailed { failure: AuditFailure::Sink }` になり、sink が受け取る envelope 列は Started と `Terminal(AuditFailure)` だけである。確かめ方: `tests/audit_reentrancy.rs` で、`OnceLock<Arc<Engine>>` を持つ記録用 sink を使う。
  - **RG-AT-02（全入口と全配送点）**: callback の中から `compile`・`link`・`link_for_repl`・`run`・`run_for_repl`・`run_audited` を呼ぶと、それぞれ上記の型で拒否される。再入が host-call triple・`Yielded`/`Resumed`・`BudgetCharged`・Terminal の配送で起きた場合も、sink が `Ack` を返していても外側は `AuditFailure::Sink` で fail-closed になる。確かめ方: 入口ごと・「何回目の submit で再入するか」ごとの table-driven test。
  - **RG-AT-03（別の Engine は許す）**: callback の中から別の Engine（sink なし・sink あり）を呼ぶと通常どおり実行され、外側の実行も成功する（envelope 列は再入しない場合と一致する）。確かめ方: 2 つの Engine を作る統合テスト。
  - **RG-AT-04（guard の後始末）**: 再入で fail-closed になった実行・sink が `Failed` を返した実行・sink が panic した実行の後で、同じ thread から同じ Engine の `compile`・`run_audited` を（callback の外で）呼ぶと通常どおり成功する。2 つの Engine の callback が入れ子になった場合（PR #281 の Q2=A）: Engine A（sink あり）の callback の中で Engine B（sink あり）の `run_audited` を呼び、B の callback から A の `run_audited` を呼ぶと、A への呼び出しは `AuditedOutcome::Outcome(InternalFailure)` で拒否される。B の実行は通常どおり完了し（B の sink が受け取る envelope 列は、B の callback が A を呼ばない場合と一致する）、A の実行は `AuditFailed { failure: AuditFailure::Sink }` になる。B の callback を出たあとも A の callback の中では A への再入は引き続き拒否され、A の callback を出たあとは許される。確かめ方: 統合テストと `src/audit.rs` の guard の unit test（列の要素ごとの印、入れ子の下の要素への印、RAII での取り外し）。
  - **RG-AT-05（deadlock しない・非破壊）**: RG-AT-01〜04 の各テストを別 thread で回し、上限時間（例: 10 秒）以内に戻ることを `recv_timeout` で確かめる。既存の `tests/audit_*.rs` は無改変で緑。sink なしの経路と、再入しない sink ありの経路は観測結果（envelope 列・`committed_bytes`・yield 位置・出力 byte・exit code）が変わらない。
- **想定の変更規模**: `src/audit.rs` に約 50〜70 行（thread-local の guard・RAII・入口用の判定関数・doc comment）、`src/embedding.rs` に約 50〜80 行（submit 3 か所を helper へ通す・6 入口の先頭の検査と拒否の値）。合わせて約 100〜150 行。テストは新しい `tests/audit_reentrancy.rs`（約 250〜350 行）と `src/audit.rs` の unit test。触る仕組みは監査の sink 配送の境界 1 つ、受入基準は 5 項目。

#### fix-audit-flush-before-host-call 実装スライス（host call 開始前に `BudgetCharged` を flush する）

> 状態: **未実装**（設計のみ）。loop:drift #342 の D-50（Q1=A、判断役が選択（issue #342））で足した `fix` スライス。§7.3 の「host call開始前…にflushする」と §8 規則7 を、監査付き経路（tree）の fs host call で満たす。

設計時点（main `d665a98`）の事実:

- `src/builtin.rs` の fs host call は、request 側の課金（`charge_host_call_request`）→ `dispatch_fs` → triple の構築 → `src/eval.rs` の `push_host_call_events` の順に進む。`push_host_call_events` は `push_audit_event` へ積むだけで、`flush_charges_into_audit_events` を呼ばない。flush は yield 境界・backpressure・終了時だけである。
- このため、host call より前の文で生じた消費（fuel など）の `BudgetCharged` が、その host call の triple より後に出る。`tests/audit_hostcall.rs` の「host-call triple 群は BudgetCharged より前」の主張（`last_hostcall < first_charged`）はこの順序を固定している。

- **実装範囲**:
  - fs host call の処理の先頭（request 側の課金より前）で accumulator を flush し、確定した `BudgetCharged` を `audit_events` へ移す。これにより、host call より前の消費の `BudgetCharged` は同じ host call の `HostCallStarted` より前に出る。host call 自身の課金（request・response の byte、host call 回数）は flush の後に accumulator へ積まれ、次の flush 点（次の host call の開始前・yield 前・terminal 前）で出る。§7.3 の「host call開始前」を文字どおりに読んだ位置であり、`BudgetCharged` の集約の単位（同 resource・同 reason）は変えない。
  - flush は sink のある経路だけで観測される（sink 無しの経路は drain しないので、出力・課金・exit code は変わらない）。flush で積む event は既存の `push_audit_event` を通し、B-2 の audit drain queue の計上に乗せる。
  - `tests/audit_hostcall.rs` の順序の主張を、§7.3 の順序（host call より前の消費の `BudgetCharged` → triple）に直す。
- **範囲外**:
  - fs 以外の host call（逸脱一覧の audit-host-boundary-scope の行）。triple を出さない host call では flush 点も作らない。
  - 256 charge ごとの flush、pause 前の flush（このスライスでは確かめず、変えない）。
  - §8 規則5/6 の timing（逸脱一覧の A-1 の行）。flush は評価器 buffer の中の操作で、sink round-trip を足さない。
  - VM（監査 sink と VM の同時設定は build で拒否される）。
- **受入基準**:
  - **FBH-AT-01（host call 前の消費が Started より前）**: sink ありで、fuel を消費する文を 3 つ実行してから `write_file` を呼ぶ script の envelope 列で、`HostCallStarted` より前に fuel の `BudgetCharged` が 1 件以上あり、`HostCallStarted` から `HostCallFinished` までの間に `BudgetCharged` が無い。確かめ方: `tests/audit_hostcall.rs` に足すテスト。
  - **FBH-AT-02（2 回の host call）**: `write_file` と `read_file` を続けて呼ぶ script で、2 つ目の `HostCallStarted` より前に、1 つ目の host call 自身の課金を含む `BudgetCharged` が出る。全 `BudgetCharged` の delta の合計は Terminal の usage と一致する（cumulative resource）。確かめ方: 同ファイルのテスト。
  - **FBH-AT-03（既存の主張の更新と非破壊）**: `tests/audit_hostcall.rs` の「triple 群は BudgetCharged より前」の主張を §7.3 の順序に直し、他の `tests/audit_*.rs` は無改変で緑。sink 無しの経路の出力 byte・exit code・fuel は変わらない。
- **done PR で行う逸脱一覧の更新**: 逸脱一覧の「`BudgetCharged` の accumulator を host call 開始前に flush しない」の行を削除する。表の下の D-50 の段落は判断の記録なので残す。
- **想定の変更規模**: `src/builtin.rs` と `src/eval.rs` に約 10〜25 行（flush の呼び出しと、それを crate 内から呼べる入口）。テストは `tests/audit_hostcall.rs` に約 80〜150 行。触る仕組みは監査の host-call 経路 1 つ、受入基準は 3 項目。

#### fix-terminal-host-effects-may-remain 実装スライス（Terminal の `host_effects_may_remain` を host call の `effect_status` から決める）

> 状態: **未実装**（設計のみ）。loop:drift #342 の D-51（Q1=A、判断役が選択（issue #342））で足した `fix` スライス。§13 AUD-024 の `host_effects_may_remain` の規則を、監査付き経路（tree）で満たす。

設計時点（main `d665a98`）の事実:

- Terminal を作るのは `src/embedding.rs` の 2 か所で、どちらも `host_effects_may_remain: false` の literal である。1 つ目（Started の ack 失敗）は script work を行わない Terminal、2 つ目は run の終了の Terminal。
- `HostCallFinished.effect_status` は `src/builtin_core.rs` の triple の構築で `Committed` / `None` / `Unknown` を決めている。

- **実装範囲**:
  - 監査付き実行の中で、`effect_status` が `Committed` または `Unknown` の `HostCallFinished` を積んだかを 1 つの flag で持つ（triple を積む箇所で立てる）。
  - run の終了の Terminal は、その flag を `host_effects_may_remain` に使う。outcome（Completed / Exited / RuntimeError / BudgetExceeded / DeadlineExceeded / Cancelled / InternalFailure）と `context_committed` にかかわらず同じ規則である。
  - Started の ack 失敗の Terminal は `false` のまま（script work が無い）。
- **範囲外**:
  - fs 以外の host call（audit-host-boundary-scope の行）。triple を出さない host call は flag に数えない。その host call だけを行った execution の Terminal は `false` のままになる（D-52 の逸脱に含まれる）。
  - record/replay の `RecordFailure { host_effects_may_remain }`（§6.1、未実装）。
  - 公開 `ExecutionOutcome` へ同じ flag を出すこと（逸脱一覧の D-8 の行の範囲）。
  - VM。
- **受入基準**:
  - **HEM-AT-01（rollback する終了）**: `write_file` が成功（`Committed`）したあと `1 / 0` の未捕捉 runtime error で終わる script の Terminal が `context_committed = false`・`host_effects_may_remain = true`。確かめ方: `tests/audit_hostcall.rs` に足すテスト。
  - **HEM-AT-02（Completed）**: `write_file` が成功して Completed で終わる script の Terminal が `context_committed = true`・`host_effects_may_remain = true`。
  - **HEM-AT-03（外部効果なし）**: host call をしない script、`read_file` だけ（`effect_status = None`）の script、grant が無く `write_file` が拒否される script（`None`）の Terminal は `host_effects_may_remain = false`。Started の ack 失敗の Terminal も `false`。
  - **HEM-AT-04（非破壊）**: 既存の `tests/audit_*.rs` は無改変で緑（Terminal の `host_effects_may_remain` を `false` と主張している既存テストが HEM-AT-01..02 の条件に当たる場合だけ、その主張を規則に合わせて直し、PR 本文に挙げる）。sink 無しの経路は変わらない。
- **done PR で行う逸脱一覧の更新**: 逸脱一覧の「Terminal の `host_effects_may_remain` が常に `false`」の行を削除する。表の下の D-51 の段落は判断の記録なので残す。
- **想定の変更規模**: `src/eval.rs`（flag と立てる箇所）と `src/embedding.rs`（Terminal の構築）に約 15〜30 行。テストは約 80〜140 行。触る仕組みは監査の Terminal の構築 1 つ、受入基準は 4 項目。

#### fix-heap-charge-audit-notify 実装スライス（heap の課金を `BudgetCharged` の accumulator へ通知する）

> 状態: **未実装**（設計のみ）。loop:drift #342 の D-53（機械的）で足した `fix` スライス。§7.3 の「Terminalのusageと全BudgetChargedのdeltaをcumulative/live resourceで突合」と §15.3 を、heap（live resource）で満たす。
>
> **改訂（needs-design [issue #369](https://github.com/kamonabe1927/Tsumugi/issues/369) の反映）**: 初版の設計（`charge_heap` だけに通知を足す、`src/budget.rs` 約 10〜20 行）では HCN-AT-01 の突合が成り立たないことを、実装役が main `92c338c` で試作して確かめた（`print(len([1, 2, 3] + [4]))` で committed − released = 176 に対し Terminal の live heap = 840）。判断役が Q1=A（heap の確保・解放を全経路（`charge_heap`、`Tracked` の `Drop` の release、`new_via_handle` の課金）で heap 台帳側に集め、flush の時点で accumulator へ移す）を選んだ（判断役が選択（issue #369））。このうち届け方の「flush の時点で移す」の部分は、設計 PR #456 の Q1=A（accumulator を共有し、通知のたびに直接書く）で置き換えた（実装範囲の「通知の届け先」の決定）。理由: §7.3 の「`committed_delta` は合計」「heap releaseは`released_delta`へ記録」「Terminalのusageと全BudgetChargedのdeltaを…live resourceで突合」と §15.3 の「BudgetChargedの合計とTerminal usageが…live resourceで一致」を文字どおり満たすのは A だけである（[マニフェスト](manifesto.md) 原則 7、優先順位の表「再現可能な検証」）。採らなかった案: B（突合を `charge_heap`・`release_heap` を通る分だけに弱める）は、Terminal の live heap と delta が一致しないまま §7.3・§15.3 の目標を受入基準の側で後退させ、原則 7 に反しうる。C（flush 時に live heap の差分から `committed_delta`/`released_delta` を合成する）は、突合が Terminal と同じ値から作った数どうしの比較になって通知漏れを検出できず（原則 9）、§7.3 の `committed_delta`・`charge_count`・`released_delta` の意味を変える。この改訂で、実装範囲・範囲外・受入基準・想定の変更規模を下のとおり書き直した（初版の受入基準 HCN-AT-01..03 を HCN-AT-01..05 に置き換えた）。

設計時点（初版 main `d665a98`、改訂 main `84643e2`）の事実:

- live heap の増減はすべて `AllocationLedger`（`src/budget.rs`、`HeapLedger = Rc<RefCell<AllocationLedger>>`）の `charge` / `release` を通る。accumulator（`BudgetChargeAccumulator`）は `BudgetLedger` が値で持ち、通知しているのは次の 2 つだけである。
  - `BudgetLedger::release_heap`（`note_release`）。呼び出し元は `Value::retrack_list` / `retrack_dict` の縮小（`src/value.rs`）と、評価器の frame・try の heap 課金の戻し（`src/eval.rs`）。
  - cumulative の `BudgetLedger::commit`（`note_charge`）。
- 通知しない heap の増減は次の 4 つである。
  1. `BudgetLedger::charge_heap`（`Tracked::new`・retrack の増加・frame の課金）。
  2. `Tracked<T>` の `Drop`（`src/value.rs`）。`HeapLedgerWeak` を `upgrade` して `AllocationLedger::release` を直接呼ぶ。collection・String・変数 cell・関数 header の寿命終了の release はこれで、`released_delta` に入らない。
  3. `Tracked::new_via_handle`（`src/value.rs`）。`HeapLedgerWeak` だけを持ち `AllocationLedger::charge` を直接呼ぶ。変数 cell（`Value::new_cell`）、関数 instance header（`new_tree_fn_header` / `new_vm_fn_header`）、heap token（`new_heap_token`、Link・Compile の確保を含む）がこれを通る。
  4. `BudgetLedger::heap_charge_raw`（`charge_context_baseline` が使う。実行経路では未使用）。
- `Tracked` の `Drop` と `new_via_handle` からは `&mut BudgetLedger` に届かないため、`BudgetLedger` の中だけで通知を足しても 2・3 は拾えない。
- `AllocationLedger::restore_live_bytes`（`BudgetLedger::restore_live_heap_bytes`）は crate 内に呼び出し元が無い。

- **実装範囲**:
  - heap の通知の口を `AllocationLedger` の側に置く。`AllocationLedger::charge` が成功したとき（`bytes > 0`）と `AllocationLedger::release` で live が減ったとき（`bytes > 0`）に、課金・解放後の live・peak を付けて HeapBytes・`Allocation` の charge / release として通知する。これで上の 1〜4 と既存の `release_heap` がすべて 1 か所を通る。
  - 通知の届け先: accumulator を `BudgetLedger` と `AllocationLedger` が共有する（`Rc<RefCell<BudgetChargeAccumulator>>` などの共有ハンドルにし、`AllocationLedger` が通知のたびに `note_charge` 相当・`note_release` を呼ぶ）。`charge_count`・256 charge ごとの確定・B-2 の確定済み byte 数（`audit_finalized_charge_bytes`）の意味は今と変わらない。
    - **決定（PR #456 の Q1=A。判断役が選択（PR #456））**: 理由: §7.3 の「256 chargeごとにflush」「`charge_count`は集約した論理charge回数」を、heap でも今の `note_charge` / `note_release`（`CHARGE_FLUSH_THRESHOLD = 256`、`src/audit.rs`）の意味のまま満たし、目標の章も逸脱一覧も足さずに済む。既定の slice fuel では script 全体が 1 slice で走り Terminal まで flush が来ないが（B-2 小節）、A なら 256 charge ごとに確定 event が切られ、heap の live・peak の途中経過が event 列に残る（[マニフェスト](manifesto.md) 原則 7）。通知漏れ・二重計上を 1 確保・1 解放の単位で数えられ、受入基準の突合がそのまま検証になる（原則 9）。共有ハンドルは既存の `HeapLedger = Rc<RefCell<AllocationLedger>>` と同じ形で、新しい仕組みを増やさない（原則 8）。採らなかった案 B（`AllocationLedger` に集計値だけを溜め、flush の時点でまとめて移す）は、マニフェストには反しないが、flush と flush の間は heap が何回動いても 256 charge の確定が起きず §7.3 の本文と食い違い（目標の章の変更か新しい逸脱の行が要る）、`charge_count` が 256 を超える 1 件になって heap の途中経過が event に残らない。
    - **B-2 の queue 会計への影響**: A では、heap の 256 charge ごとに確定した `BudgetCharged` が `audit_finalized_charge_bytes` を通って B-2 の audit drain queue の byte 数（`audit_queue_bytes`）に入る。このため、同じ `max_pending_bytes` でも、`AuditBackpressure` の yield の回数・位置が今と変わりうる（heap の確定済み event の分だけ早く上限に届く）。これは §10.2 の queue 会計を heap にも同じ規則で当てた結果で、目標仕様と食い違わない。
    - **A の条件**: `Tracked` の `Drop` は任意の時点で走るので、accumulator の `RefCell` の borrow は 1 回の通知・1 回の drain の中で閉じ、borrow を持ったまま `Value` を drop しない（二重 borrow の panic は原則 6 に触れる）。これを HCN-AT-02・03 のテストで裏付ける。
  - `BudgetLedger::release_heap` の `note_release` の直接の呼び出しは消す（`AllocationLedger::release` の通知と二重にしない）。
  - heap の確保の通知は、`committed_delta` に確保 byte を足し、`usage_after` を live heap、`peak_after` を peak heap にする（§7.3）。今の `note_charge` は `peak_after` を設定しないので、heap 用に live と peak を受け取る入口を accumulator に足す。
  - 上限超過・overflow で `charge` が失敗したときは通知しない（課金されていない）。
  - `restore_live_bytes` は通知しない（呼び出し元が無い。使う経路を作るスライスで扱う）。
- **範囲外**:
  - 256 charge ごとの flush の規則と、flush の時点（host call 開始前・Terminal 前など）の変更。
  - per-item resource の `BudgetPeaks` の突合。
  - heap 以外の resource の通知。
  - Link で context に残る値を新 execution の heap の baseline として課金すること（`fix-link-context-baseline-charge`）。その課金が `AllocationLedger::charge` を通れば、このスライスの口で通知される。
  - VM（監査 sink と VM の同時設定は build で拒否される）。
- **受入基準**:
  - **HCN-AT-01（突合）**: sink ありで、List・String・Dict を作り、`let` の変数と closure の capture を持つ script の envelope 列に HeapBytes・`Allocation` の `BudgetCharged` が 1 件以上あり、`committed_delta` の合計から `released_delta` の合計を引いた値が Terminal の `live_heap_bytes` と一致する。最後の HeapBytes の `BudgetCharged` の `peak_after` が Terminal の `peak_heap_bytes` と一致する。`print(len([1, 2, 3] + [4]))` 単体でも同じく一致する。確かめ方: `tests/audit_core.rs` に足すテスト。
  - **HCN-AT-02（寿命終了の解放）**: 関数の中で List と String を作り、関数から戻ると参照が消える script で、HeapBytes の `released_delta` の合計が 0 より大きく、HCN-AT-01 と同じ突合が成り立つ（`Tracked` の `Drop` の release が `released_delta` に入る）。
  - **HCN-AT-03（二重計上なし）**: List の `pop` など retrack の縮小（`release_heap`）を含む script と、frame・try の heap 課金を戻す script で、HCN-AT-01 の突合が成り立つ（同じ解放が 2 回数えられない）。
  - **HCN-AT-04（上限超過）**: heap 上限を超える確保（`Tracked::new` 経由と、変数 cell の `new_via_handle` 経由の 2 通り）で BudgetExceeded になる script で、超過した確保の分は `BudgetCharged` に含まれず、HCN-AT-01 と同じ突合が成り立つ。
  - **HCN-AT-05（非破壊）**: 既存の `tests/audit_*.rs` は、`BudgetCharged` の件数・中身を固定している主張が HeapBytes の行の追加・変化に当たる場合と、`tests/audit_queue_bytes.rs` の `AuditBackpressure` の yield の回数・位置の主張が heap の確定済み event の追加に当たる場合だけ直し（どちらも PR 本文に挙げる）、それ以外は無改変で緑。sink 無しの経路の出力 byte・exit code・fuel・heap の課金（live・peak・BudgetExceeded の値）は変わらない。`src/budget.rs` の `AllocationLedger` の単体テストは無改変で緑。
- **done PR で行う逸脱一覧の更新**: 逸脱一覧の「heap の増減の一部が `BudgetCharged` の accumulator へ通知されない」の行を削除する。B-2 の「audit drain queue byte 上限の判定がトップレベル境界だけの soft limit」の行の「超過量は host-call 回数 budget で有界」を、heap の通知分を含む書き方（host-call 回数 budget と、heap の確保・解放の回数（256 回ごとに確定 1 件）で有界）に直す。
- **想定の変更規模**: `src/budget.rs`（accumulator の共有、`AllocationLedger` の通知、`release_heap` の整理）と `src/audit.rs`（heap の確保を live・peak 付きで受ける入口）に約 60〜100 行。`src/value.rs` は `Drop`・`new_via_handle` が `AllocationLedger` の口を通るため変えないか数行。テストは約 120〜200 行。触る仕組みは budget の accumulator への通知 1 つ、受入基準は 5 項目。

### Slice 6: VM conformance

- Charge opcode、StableError、FunctionId、host boundary、transaction、audit semantic eventをtreeと一致させる
- differential matrix/fuzzを通し、既知非適合が0件になったcommitでのみexperimental flagを外す

#### fix-vm-builtin-call-lowering 実装スライス（builtin 名の呼び出しの lowering を panic せず線形にする）

> 状態: **未実装**（設計のみ）。issue #292（kamonabe、仕分け「受け入れ」）から作った `fix` スライス。§4.8（この設計 PR で追加）と §2.1 の適合方針、[言語仕様](language-spec.md)「コンテキスト依存組み込み関数の呼び出し順序」を VM で満たす。

設計時点（main `b1b72cc`）の事実:

- builtin 名（`builtin_registry` の public builtin。`print` を除く）の呼び出しで、名前が local・upvalue に解決できないとき、Compiler（`src/compiler.rs` の `Expr::Call`）は `JumpIfGlobalDefined(name)` で分岐し、builtin 経路（`compile_builtin_call`）と user 経路（`compile_user_call`）の両方を出す。どちらの経路も引数式をそれぞれ compile するため、引数の中に同じ形の呼び出しがあると、その引数の bytecode も 2 つずつ出る。入れ子 k 段で bytecode は 2^k に比例する（再現: `print(abs(abs(…abs(1)…)))` の 16 段で debug build 0.13 秒、20 段で bytecode の heap が 64 MiB を超えて heap 上限エラー。tree は 22 段で 5 ms）。AST 深度上限は 256 なので、上限内の source で compile が実質止まらなくなる。
- builtin 経路の順序: context builtin なら `ValidateBuiltinCall(name, 引数の数, 第1引数が識別子か)`（個数・識別子の検査。失敗すると引数を評価しない）→ 引数 → `CallBuiltin(id, n)`。user 経路の順序: `PrepareCall`（step/depth）→ callee（`GetGlobal`）→ `ValidateCall(n)`（callable/arity）→ 引数 → `Call(n)`。分岐を決めるのは引数を評価する前の 1 回だけである。
- `push` の lowering（`compile_builtin_call` の中）は、`!args.is_empty()` と第1引数が識別子であることだけを確かめて `args[1]` を読むため、`push(xs)` で index 範囲外の panic になる（`ValidateBuiltinCall` はその前に出しているが、panic は compile 時なので届かない）。`pop` の lowering は引数をすべて compile するので panic しないが、引数が 2 個以上でも特別な経路に入る。`len(識別子)`（`LenLocal`）は識別子 1 個だけの形にしか入らない。
- tree は `push(xs)` を「`push の引数個数が一致しません: 期待 2, 実際 1`」の `argument` error（catch できる）にする。

- **実装範囲**:
  - `push` の特別な lowering（`ListPushBinding`）に入る条件を「引数がちょうど 2 個で、第1引数が識別子」にする。`pop` の特別な lowering（`PopUpdate`）に入る条件を「引数がちょうど 1 個で、第1引数が識別子」にする。それ以外の形は汎用の builtin 経路（引数を compile して `CallBuiltin`）へ落とす。どの形でも、先に出す `ValidateBuiltinCall` が、引数を評価する前に tree と同じ `argument` / `builtin_type` error を出す。
  - builtin 名の呼び出しで、1 つの source の引数式の bytecode を 1 回だけ出すようにする。分岐（`JumpIfGlobalDefined`）は今と同じく引数の評価より前の 1 回で決め、経路ごとに違う部分（引数より前の検査と callee の準備、引数より後の呼び出し）だけを 2 つ出し、引数の bytecode は 2 経路で共有する。どちらの経路を通ったかは、引数の bytecode の後の呼び出しで分かるようにする（例: 経路ごとに operand stack の同じ位置へ callee または builtin 経路の印を置き、引数の後の 1 つの命令がそれを見て `CallBuiltin` と `Call` のどちらの処理をするか決める。命令の形は実装 PR で決めてよい。`OpCode` は `unstable-bytecode` feature の下でだけ公開され、安定した公開 API ではない）。`push` の特別な lowering でも、第2引数の bytecode を 2 経路で共有する（builtin 経路は今と同じく binding storage を in-place で書き換え、対象 List を operand stack へ複製しない）。
  - 命令を足す・意味を変える場合は、`src/verifier.rs` の検査（jump 先・stack の深さ）と、不正な bytecode を `internal` error にする VM の防御（[設計ドキュメント](design.md)「2026-08-28: VMの内部不変条件をinternal errorへ置換（AUD-023）」、`tests/defensive_vm.rs`）を同じ PR で対応させる。
- **変えないもの**: 観測結果。出力、error の kind・message・行、catch できるかどうか、評価順と side effect 順、fuel・step の消費、heap / peak_heap（REV-015 Slice 6 の `push` の in-place 課金を含む）、REPL・import・後から宣言した同名 global を user binding として優先する規則（`JumpIfGlobalDefined` の判定位置）。tree は変えない。
- **範囲外**:
  - ループの外の `break` / `continue` を VM が compile error にする件（issue #292 の補足）。設計時に再現を確かめた（`print(1)` の次の行に `break` を置くと、tree は `1` を出してから runtime error、VM は何も出さず error。`if false` の中に置くと tree は最後まで実行し、VM は script 全体を拒否する）。触る仕組みが別（ループ制御の lowering）で、`try` の中で catch できるかを言語仕様が決めていない（tree は catch しない）ため、別スライス `fix-vm-loop-control-outside-loop`（`todo`）にした。
  - `print` の lowering（予約 token で、経路は 1 つ）。
  - local・upvalue に同名の binding がある呼び出し（今も user 経路だけを出す）。
  - VM の命令数で止める watchdog の上限値。命令の並びが変わるため、watchdog が数える命令数は変わり得るが、watchdog は fuel から独立した停止性の backstop で、fuel・step の消費は変えない（[次期意味論・実装決定](semantic-decisions.md) §17.7）。
- **不採用案**:
  - compile 時に bytecode の大きさの上限を置き、超えたら制御された error で止める: §4.8 の 3 項目めに反する（tree が実行できる script を VM だけが拒否する）。manifesto 原則 5。
  - プログラム全体に同名の global 定義・import が無いときだけ builtin 経路だけを出す: 同名の global を書いた script では今と同じく 2^k に増える。REPL で後の入力が同名の関数を定義した場合、前の入力で定義した関数の中の呼び出しが user binding を優先しなくなり、意味論が変わる。
  - 引数を分岐より前に評価する: builtin 経路の「個数の検査に失敗したら引数を評価しない」と、user 経路の「callee の検査に失敗したら引数を評価しない」（[言語仕様](language-spec.md)「ユーザー定義関数の呼び出し順序」）に反する。
- **決定記録（判断役が選択（PR #317））**: 共有 lowering で命令数が減り、VM の `bytecode_chunk` の課金（64 + 16×命令数 + 32×定数数）が減るため、VM の live_heap / peak_heap の絶対値が減る。これを「変えないもの」の heap / peak_heap の例外として認める（Q1=A）。理由: (1) bytecode を入れ子の深さに線形にするのがこのスライスの目的（§4.8）で、命令数が減れば VM の absolute heap が減るのは必然である。(2) §15.5（PR-6c 線引き）と [実行予算・協調実行仕様](execution-control.md) §5.3 A-1 は、code artifact（VM は `bytecode_chunk`）の heap 数値を engine 固有とし、backend 間で突合するのは課金の有無・向き・release 寿命とする。(3) 「変えないもの」の heap / peak_heap は、`push` の対象 List を複製しないなど課金の向き・構造を変えないことを指す。tree・fuel・bytecode 以外の heap は変わらない。`tests/value_nesting_depth_limit.rs` の VND-AT-05 の VM 期待値は 24260 から 23620 に変えた。採らなかった案: B（`needs-design` で「変えないもの」を設計し直す）— 線形化と VM の heap を 1 byte も変えないことを両立する手が無い。
- **done PR で行う逸脱一覧の更新**: 逸脱一覧の「VM の compile が `push` の引数が 1 個の呼び出し…」の行を削除する。ループの外の `break` / `continue` の行は残す。
- **受入基準**:
  - **VBC-AT-01（引数の個数・形の誤りで panic しない）**: `push(xs)`・`push(xs, 1, 2)`・`push()`・`pop(xs, 1)`・`pop()` と、第1引数が識別子でない `push([1], 2)` を、`let xs = []` の後に置いた script で、VM は panic せず、tree と同じ kind・message・行の error になる。`try` / `catch` で catch でき、catch の後の文が両 backend で同じように実行される。引数に `print` を含む形（例: `push(xs, print("a"), print("b"))`）で、両 backend とも引数を評価しない（出力が無い）。library 経路（`Backend::VmExperimental` の `Engine::compile`）でも compile が panic しない。確かめ方: tree/VM の differential test（`tests/differential.rs` かこのスライスの新しい test file）。
  - **VBC-AT-02（bytecode の量が入れ子の深さに線形）**: `print(abs(abs(…abs(1)…)))` の入れ子 k 段の compile 結果の命令数を k = 1〜200 で測り、隣り合う k の差が k によらず一定である（線形）。`len(str(abs(…)))` のように違う builtin を混ぜた入れ子、`push(xs, push(ys, …))` の入れ子、user 関数の引数の中の builtin の入れ子でも同じく線形である。確かめ方: `src/compiler.rs` の `#[cfg(test)]` の unit test（Chunk の命令数と定数の数を数える）。
  - **VBC-AT-03（深い入れ子を実行できる）**: 入れ子 200 段の `abs`・`str` の混在、`push` の 50 段の入れ子を VM で実行すると、既定の上限の下で tree と同じ出力で完了し、compile と実行が合わせて `cargo test`（debug build）で 10 秒以内に終わる（既存の `tests/contains_budgeted_equality.rs`・`tests/value_nesting_depth_limit.rs` の時間の条件に合わせた。線形性は VBC-AT-02 の命令数で確かめる）。確かめ方: differential test。
  - **VBC-AT-04（user binding の優先と資源の一致）**: 同名の global を、呼び出しより前に宣言した場合・後で宣言した関数から呼ぶ場合・import した場合・REPL の後の入力で定義した場合のそれぞれで、入れ子の builtin 名の呼び出しが tree と同じく user binding を呼ぶ（入れ子の内側と外側で経路が違う混在も含む）。builtin の入れ子と `push` の入れ子で、fuel の消費・heap / peak_heap・出力 byte が tree と一致する。確かめ方: differential test と既存の REPL test の追加ケース。
  - **VBC-AT-05（非破壊と防御）**: 既存の `tests/differential.rs`・golden fixture・`tests/defensive_vm.rs`・REV-015 Slice 6 の `push` の peak_heap parity test（`tests/scaling.rs` の `push_peak_heap_has_no_full_clone_spike_across_engines`）が無改変で緑。足した・意味を変えた命令について、`src/verifier.rs` が compiler の出力を受け入れ、手組みの不正な Chunk（stack の不足・印の位置の誤り）を VM が panic せず `internal` error にする。確かめ方: `tests/defensive_vm.rs` と verifier の unit test への追加。
- **想定の変更規模**: `src/compiler.rs` 約 60〜120 行（`push` / `pop` の条件、builtin 名の呼び出しの共有 lowering）、`src/vm.rs` 約 40〜80 行（経路の印と引数の後の呼び出し）、`src/opcode.rs`・`src/verifier.rs`・`src/chunk.rs` 合わせて約 20〜40 行。合わせて約 120〜240 行。テストは unit test と differential test で約 250〜400 行。触る仕組みは VM の builtin 名の呼び出しの lowering 1 つ、受入基準は 5 項目。

#### fix-vm-loop-control-outside-loop 実装スライス（ループの外の `break` / `continue` を VM でも到達時の runtime error にする）

> 状態: **未実装**（設計のみ）。issue #292（kamonabe、仕分け「受け入れ」）の補足から作った `fix` スライス。[言語仕様](language-spec.md)「break / continue」の「ループの外で使うとランタイムエラー」と、§2.1 の適合方針（tree が規範）・§4.8 の 1 項目め（compile は失敗せず、到達したときに tree と同じ runtime error を出す bytecode を生成する）を、VM のループ制御の lowering で満たす。catch の扱い（下の Q1）は、この設計 PR の「判断の候補」で判断役が A を選んだ（PR #328）。下の実装範囲と受入基準はこの選択で書いており、選んだ規則は言語仕様「break / continue」にも書いた。

設計時点（main `882e093`、debug build の CLI）の事実:

- VM の Compiler（`src/compiler.rs` の `compile_break` / `compile_continue`）は、`self.loops` が空のとき `TsumugiError::break_outside_loop` / `continue_outside_loop` を compile error として返す。`self.loops` は関数・lambda の本体ごとに空から始まるので、ループの中で定義した関数の本体にある `break` もループの外として扱う（tree と同じ）。compile error なので、到達しない分岐にあっても script 全体を実行前に拒否し、それより前の文の出力も出ない。import した module のトップレベルにある場合も、import 元の script を 1 文も実行しない。
- tree（`src/eval.rs`）は `break` / `continue` を制御の信号（`EvalResult::Break` / `Continue`、driver の `FlowSignal`）として外へ伝える。同じ関数本体の中の `try` はこの信号を catch しない（`try` の handler を外して通り抜ける。`catch` ブロックの中にあっても同じ）。信号がループに会わずに関数本体・callback 本体・トップレベル（module のトップレベルを含む）の境界まで来たとき、その境界で `control_flow` の runtime error（message は [言語仕様](language-spec.md)「エラーの kind とメッセージ（正本）」の `control_flow` の行、行は `break` / `continue` の文の行）にする。関数の境界で作った error は、その関数の Call frame を畳んだ後に作るので、ふつうの runtime error として呼び出し元の `try` で catch できる。
- 再現（`--vm` と tree の比較）:

| script | tree | VM（今） |
|---|---|---|
| `print(1)` の次の行に `break` | `1` を出してから `2行目: break はループの中でのみ使用できます`、終了コード 1 | 何も出さず同じ message、終了コード 1 |
| `if false` の中に `break`、その後に `print("done")` | `done`、終了コード 0 | 実行せず error |
| トップレベルの `try` の中に `break`、`catch` で `print("caught")` | catch しない。同じ error で終了 | 同じ message（compile error） |
| `fn f()` の本体に `continue`、トップレベルで `try f() catch e print("caught: " + e) end` | `caught: continue はループの中でのみ使用できます` を出して続行 | 実行せず error |
| `fn f()` の本体の `try` の中に `break`、`for` の中で `f()` を呼ぶ | `f` の中の `catch` は動かず、`3行目: …` で終了（呼び出し元のループも抜けない） | 同じ message（compile error） |
| ループの中で定義した lambda の本体に `break`、ループの中の `try` で呼ぶ | 呼び出し元の `try` が catch して続行 | 実行せず error |
| `fn g()` に `break`、`fn f()` が `g()` を呼び、トップレベルで `f()` | `2行目: break…` の後の call trace は `in f() (7行目)` だけ（`g` の frame を畳んでから error を作るため、`g` の行が入らない） | 実行せず error |
| `map` の callback の lambda 本体に `break` | callback の境界で `3行目: …` | 同じ message（compile error） |
| import した module のトップレベルに `break` | import 元の `print` と module の `print` を出してから error | 何も出さず error |

- **決定記録（判断役が選択（PR #328））: ループの外の `break` / `continue` を `try` で catch できる範囲（Q1=A）。** 設計時点の [言語仕様](language-spec.md)は「ランタイムエラー」とだけ書き、`control_flow` を catch できる種別に挙げるが、同じ関数本体の `try` で catch できるかを決めていなかった（§2.1 の優先順位 1 の [次期意味論・実装決定](semantic-decisions.md) にも無い）。選んだ規則: tree の今の観測結果を規則にする。同じ関数本体（トップレベル・module のトップレベルを含む）の `try` は catch せず（handler を外して通り抜ける。`catch` ブロックの中でも同じ）、関数・callback 本体の境界で文の行の `control_flow` の runtime error になり、呼び出し元の `try` は catch できる。VM をこれに合わせ、言語仕様「break / continue」に書き足した。理由: (1) manifesto 原則 5（backend ごとに意味論・エラーを変えない）と「一貫した意味論 > backend 固有の最適化」、§2.1（tree が規範）・§4.8 の 1 項目め。(2) spec と tree が矛盾しているのではなく spec が決めていない範囲なので、§2.1 の「tree の bug を無条件に仕様へ昇格しない」には当たらない。(3) ループの中の `break` が `try` に catch されずに抜ける既存の決定（`try_break_continue` fixture）と同じ「制御の信号は `try` を素通りする」扱いで一貫する。(4) 変更は VM の lowering 1 つで tree と既存 fixture を変えず（原則 8）、tree との differential test で検証できる（原則 9）。採らなかった案: B（文の位置で error を出し、同じ関数本体の `try` でも catch できる）— tree と VM の両方を変える言語挙動の変更で `fix` スライスの範囲を超え、ループの中の `break` は catch せずループの外だけ catch する形になって既存の決定と一貫しない。`control_flow` が catch 可能種別であることは A でも呼び出し元の `try` で満たされる。C（関数の外の `return`（AUD-043）と同じく parse 時のエラー）— 言語仕様の目標（ランタイムエラー）を変え、到達しない `break` を含む script を両 backend で拒否するようになる。採るなら目標を変える別の issue で扱う。

- **実装範囲**:
  - `compile_break` / `compile_continue` は、囲むループが無いときも compile error を返さず、到達したときに次の順で動く bytecode を出す: (1) 同じ関数本体（トップレベルを含む）の中で、その文を囲んでいる `try` の handler をすべて外す（ループの中の `break` が `LoopState.try_depth` の数だけ `TeardownTry` を出すのと同じ考え方。ループの外では関数本体ごとの `try` の深さを Compiler が数える）。(2) `break_outside_loop` / `continue_outside_loop`（文の行）の runtime error を出し、ふつうの runtime error と同じく frame を巻き戻す。こうすると、同じ関数本体の `try` は catch せず、関数の境界の外（呼び出し元・callback を呼んだ builtin の呼び出し元）の `try` は catch でき、トップレベルでは未捕捉の error になる。
  - error の行と call trace を tree と揃える。tree は error を作る前に、`break` を含む関数の Call frame を畳むため、trace にその関数の行が入らない（上の表）。VM は、error を出す命令で今の frame を trace に積まない、または frame を 1 段畳んでから error を出す（どちらにするか・命令の形は実装 PR で決めてよい。`OpCode` は `unstable-bytecode` feature の下でだけ公開され、安定した公開 API ではない）。トップレベル（frame が 1 段）では trace を付けない。
  - 命令を足す・意味を変える場合は、`src/verifier.rs` の検査と、不正な bytecode を `internal` error にする VM の防御（`tests/defensive_vm.rs`）を同じ PR で対応させる。
- **変えないもの**: tree の挙動（このスライスは VM だけを tree に合わせる）。ループの中の `break` / `continue` の bytecode と、その fuel・step の課金（back-edge はループ文の行、`try_break_continue` fixture の TeardownTry）。error の kind・message（正本表の `control_flow` の 2 行）。到達しない `break` / `continue` が fuel・heap を消費しないこと（error を出す命令は到達したときだけ動く）。REPL で未捕捉の error になった入力は、その入力の language-state を rollback する（AUD-024。VM は今は compile error で 1 文も実行しないので、rollback の対象が新しく生まれる）。
- **範囲外**:
  - ループの外の `break` / `continue` を、関数の外の `return`（AUD-043）と同じく parse 時のエラーにする案。言語仕様「break / continue」の目標（ランタイムエラー）を変える言語挙動の変更で、tree も変わる。issue #292 の期待は「tree と同じ」で、このスライスでは扱わない（下の不採用案）。
  - tree の call trace の欠け（`break` を含む関数の行が trace に入らない）を直すこと。直すなら tree と VM の両方の変更になり、別の issue で扱う。このスライスでは VM を今の tree に合わせる。
  - ループの中の `break` / `continue` の lowering、`try` の lowering の他の部分。
- **不採用案**:
  - compile error のまま残し、tree を compile 時の拒否に合わせる: tree が実行できる script を拒否する方へ変えることになり、言語仕様「ループの外で使うとランタイムエラー」と §4.8 の 3 項目めに反する。
  - parse 時の静的エラーにする（AUD-043 の `return` と同じ）: 上の範囲外のとおり、言語仕様の目標を変える。判断にかけるなら manifesto の原則に照らして別の issue で扱う。
- **done PR で行う逸脱一覧の更新**: 逸脱一覧の「VM の compile が、ループの外の `break` / `continue` を compile error にする」の行を削除する。
- **受入基準**:
  - **VLC-AT-01（到達しなければ実行が続く）**: `if false` の中・呼ばない関数の本体・error が起きない `try` の `catch` ブロックの中にループの外の `break` / `continue` を置いた script を、VM が compile error にせず最後まで実行し、出力・終了コード・fuel・step が tree と一致する。library 経路（`Backend::VmExperimental` の `Engine::compile`）でも compile が成功する。確かめ方: tree/VM の differential test（`tests/differential.rs` かこのスライスの新しい test file）。
  - **VLC-AT-02（到達したら tree と同じ runtime error）**: トップレベル・`if` の中・関数本体・lambda 本体・`map` などの callback 本体・import した module のトップレベルで、ループの外の `break` / `continue` に到達したとき、VM はそれより前の文の出力を出してから、tree と同じ kind・message・行・call trace・終了コードの error になる。`fn g` に `break`・`fn f` が `g()` を呼ぶ形の call trace を含む。確かめ方: CLI の paired test（tree と `--vm` の stdout・stderr・終了コードの比較）と differential test。
  - **VLC-AT-03（catch できる範囲が tree と同じ）**: 同じ関数本体（トップレベルを含む）の `try` の中・`catch` ブロックの中・入れ子の `try` の中の `break` / `continue` を、その `try` が catch しない。関数の境界の外の `try`（呼び出し元・callback を呼んだ builtin の呼び出し元）が catch でき、`e["type"]` が `control_flow`、`e["line"]` が文の行で、catch の後の文が両 backend で同じように実行される。外した handler が残って後の error を誤って catch しない（`try` の中で `break` に到達して catch された後に、別の error を起こす）。確かめ方: differential test と golden fixture（tree・VM の両方で実行される fixture）。
  - **VLC-AT-04（REPL と既存挙動）**: REPL で、ループの外の `break` を含む入力が、それより前の出力を出してから error になり、その入力が加えた binding を rollback し、次の入力が両 backend で同じように続く。既存の `error_break_outside_loop`・`error_continue_outside_loop`・`try_break_continue`・`break_continue` fixture、`tests/differential.rs`・`tests/defensive_vm.rs` が無改変で緑。足した・意味を変えた命令について、`src/verifier.rs` が compiler の出力を受け入れ、手組みの不正な Chunk を VM が panic せず `internal` error にする。確かめ方: REPL の paired test、既存 test、verifier と `tests/defensive_vm.rs` への追加。
- **想定の変更規模**: `src/compiler.rs` 約 30〜60 行（ループの外の `break` / `continue` の lowering、関数本体ごとの `try` の深さ）、`src/vm.rs` 約 20〜50 行（error を出す命令と trace の扱い）、`src/opcode.rs`・`src/verifier.rs`・`src/chunk.rs` 合わせて約 10〜30 行。合わせて約 60〜140 行。テストは differential test・CLI の paired test・fixture・verifier で約 200〜350 行。触る仕組みは VM のループ制御の lowering 1 つ、受入基準は 4 項目。

#### fix-vm-for-hidden-locals 実装スライス（VM の `for` の隠しローカルをスクリプトの名前から見えなくする）

> 状態: **未実装**（設計のみ）。issue #365（kamonabe、仕分け「受け入れ」）から作った `fix` スライス。[言語仕様](language-spec.md)「名前の可視性と解決時期」（この設計 PR で、`for` が内部で持つ状態はスクリプトの名前から読み書きできないことを書き足した）と、§2.1 の適合方針（tree が規範）を、VM の `for` の lowering で満たす。

設計時点（main `69ab342`、debug build の CLI）の事実:

- VM の Compiler（`src/compiler.rs` の `compile_for`）は、反復列と位置を `add_local("__collection__")`・`add_local("__index__")` で名前付きの local slot に置く。名前の解決（`resolve_local`・`resolve_upvalue`、子の Compiler へ渡す `build_ancestor_vars`、子の祖先の upvalue を親が確保する `resolve_child_upvalues` の `ensure_upvalue_for_ancestor`）は local の名前を区別なく引くので、ループ本体と、本体の中で作った関数・lambda から、この 2 つの名前で slot を読み書きできる。`__index__` も `__collection__` も字句解析では通常の識別子である（`_` で始まる英数字の並び）。
- tree（`src/eval.rs`）は反復列と位置を frame（`FrameKind::For`）に持ち、名前の binding を作らない。
- 名前を引く compiler 内部の local は、ほかに lambda の slot 0 の `<lambda>` があるが、`<` を含むので識別子として書けず、スクリプトから引けない。非公開 builtin の `__pop_update` は、両 backend とも `未定義の変数または関数: __pop_update` になる（差は無い）。
- 再現（`--vm` と tree の比較。tree の error は終了コード 1）:

| script | tree | VM（今） |
|---|---|---|
| `for x in ["a", "b"]` の本体で `print(__index__)` | `2行目: 未定義の変数または関数: __index__` | `0`・`1` を出して終了コード 0 |
| 本体で `print(__collection__)` | 同じく `未定義の変数または関数: __collection__` | 反復列 `[1, 2]` を 2 回出す |
| 本体で `print(x)` の後に `__index__ = 3`（要素 4 個） | `a` を出してから `3行目: 未定義の変数に代入: __index__` | `a` だけ出して終了コード 0（反復を飛ばす） |
| 本体で `__collection__ = [9, 9, 9]` の後に `print(x)` | `2行目: 未定義の変数に代入: __collection__` | `1`・`9`・`9`（反復列を差し替える） |
| 本体で `__collection__ = 5` | 同じく代入の error | `1行目: len の第 1 引数は List/Str/Dict である必要があります: Int`（`for` の行の、script に無い操作の error） |
| 本体で `__index__ = -5` の後に `print(x)` | 代入の error | `1` を出してから `1行目: List のインデックスが範囲外です: -4 (長さ: 2)` |
| 本体で `try print(__index__) catch e print(e["type"]) end` | `name` を 2 回 | `0`・`1`（catch しない） |
| 本体の lambda が `__index__` を返し、本体で呼ぶ | `未定義の変数または関数: __index__` と `in <lambda>() (3行目)` | `0`・`1` |
| 関数の中の `for` の本体の lambda が `__collection__` を返す | 同じく未定義名の error と trace | `[1]` |
| `let __index__ = 10` の後の `for` の本体と `for` の後で `print(__index__)` | `10` を 3 回 | `0`・`1`・`10`（利用者の binding を隠す） |
| `fn g()` の中で `let __index__ = 10` の後の `for x in [1]` の本体で、lambda `m` の中の lambda `c`（`return __index__`）を `m` から呼んで出す（2 段の入れ子。PR #441 のレビューで追加） | `10` | `0`（`m` の `ensure_upvalue_for_ancestor` が隠しローカルを捕捉する） |
| ループ変数の名前が `__index__`（`for __index__ in [7, 8]`） | `7`・`8` | `7`・`8`（差なし。ループ変数の slot が後に積まれるため） |

- **実装範囲**:
  - `compile_for` が反復列と位置のために作る 2 つの local を、スクリプトの名前の解決（`resolve_local`・`resolve_upvalue`・`build_ancestor_vars`・`ensure_upvalue_for_ancestor`（`resolve_child_upvalues` から呼ばれ、`enclosing_locals` を名前で引く）を通る、読み・代入・upvalue の捕捉のすべて。2 段以上の入れ子の関数・lambda の捕捉を含む）から引けないようにする。slot の番号・`GetLocal` / `LenLocal` / `IndexLocal` / `SetLocal` で slot を直接指す今の lowering は変えない。引けなくする方法（識別子として字句解析できない名前にする、`Local` に「名前で引かない」印を持たせる、など）は実装 PR で決めてよい（compiler の内部で、公開 API・bytecode の形を変えない）。
  - 結果として、本体（入れ子の関数・lambda を含む）の `__index__` / `__collection__` は、ほかの名前と同じく、外側に利用者の binding があればそれに、無ければ global に解決され、どちらも無ければ tree と同じ `name` の runtime error（`未定義の変数または関数` / `未定義の変数に代入`）になる。
- **変えないもの**: tree の挙動（このスライスは VM だけを tree に合わせる）。この 2 つの名前を書かない script の bytecode（命令列・定数・local の数）と、その出力・fuel・step・heap / peak_heap。`for` の反復の課金（execution-control §5.2・§5.3、`tests/for_iter_charge.rs`・`tests/for_iter_copy_order.rs`）、反復ごとに新しいセルへ bind するループ変数の規則（言語仕様「ブロックスコープ」）。
- **範囲外**:
  - `_` で始まる名前を予約語にする・識別子として禁じること。言語仕様の字句の目標を変える言語挙動の変更で、tree も変わる。issue #365 の期待（tree と同じく未定義になる）はこのスライスで満たせる。
  - 非公開 builtin（`__pop_update` など）の名前。設計時点で両 backend とも未定義名になり、差が無い。
  - `while` の lowering、`for` の反復列の作り方（`ToIterList`）と課金。
- **不採用案**:
  - 隠しローカルを tree にも作り、両 backend で読めるようにする: 処理系の内部状態を言語の名前として公開することになり、スクリプトが反復の位置・反復列を書き換えて `for` の意味を変えられる（上の表の VM の動き）。言語仕様「ループ」の `for` の目標（各要素を順に bind する）に無い挙動で、manifesto 原則 8（小さく理解可能な中核）にも沿わない。
  - parser で `__` で始まる識別子を拒否する: 上の範囲外のとおり言語挙動の変更で、既存の script（`let __index__ = 10` など、tree で今動くもの）を両 backend で拒否するようになる。
- **done PR で行う逸脱一覧の更新**: [言語仕様](language-spec.md)の逸脱一覧の「`--vm` 版の `for` が、反復列と位置を `__collection__` / `__index__` という名前の…」の行を削除する（本書の逸脱一覧には行を置いていない）。
- **受入基準**:
  - **VFH-AT-01（読めない）**: トップレベル・関数本体・入れ子の `for`・本体の中の lambda と関数（upvalue の捕捉）・本体の lambda の中の lambda（2 段の入れ子。`ensure_upvalue_for_ancestor` を通る捕捉）・関数の中の `for` の本体の lambda で、`__index__` / `__collection__` を読むと、VM が tree と同じ kind・message・行・call trace・終了コードの `name` error になり、それより前の出力も一致する。本体の `try` で catch でき、`e["type"]` が `name` で、catch の後の文が両 backend で同じように実行される。確かめ方: tree/VM の differential test（`tests/differential.rs` かこのスライスの新しい test file）と CLI の paired test（tree と `--vm` の stdout・stderr・終了コードの比較）。
  - **VFH-AT-02（書けない）**: 本体（入れ子の lambda を含む）で `__index__` / `__collection__` へ代入すると、VM が tree と同じ `未定義の変数に代入` の error になり、反復の回数・順序が変わらない（上の表の `__index__ = 3`・`__collection__ = [9, 9, 9]`・`__collection__ = 5`・`__index__ = -5` の各形で、出力・error・終了コードが tree と一致する）。確かめ方: differential test と CLI の paired test。
  - **VFH-AT-03（利用者の同名 binding を隠さない）**: `__index__` / `__collection__` をトップレベルの `let`・関数の local・関数から読む global・ループ変数の名前（`for __index__ in …`）・入れ子の `for` の外側のループ変数として宣言した script で、`for` の本体・本体の lambda・本体の lambda の中の lambda（2 段の入れ子）・`for` の後の読み書きが tree と同じ値を返す（上の表の `let __index__ = 10` は `10` を 3 回、`fn g()` の 2 段の入れ子は `10`）。VM の REPL で、前の入力で宣言した `__index__` を後の入力の `for` の本体で読んでも tree と同じ値になる。確かめ方: differential test と REPL の paired test。
  - **VFH-AT-04（非破壊）**: この 2 つの名前を書かない `for`（List・Dict・Str、`break` / `continue`、入れ子、本体でのクロージャの作成）の compile 結果の命令列・定数・local の数が変わらない。既存の `tests/differential.rs`・golden fixture・`tests/for_iter_charge.rs`・`tests/for_iter_copy_order.rs`・`tests/defensive_vm.rs`・`tests/scaling.rs` が無改変で緑。確かめ方: `src/compiler.rs` の `#[cfg(test)]` の unit test（代表の `for` の Chunk を変更前の命令列と比べる）と既存 test。
- **想定の変更規模**: `src/compiler.rs` 約 5〜30 行（隠しローカルの作り方と名前の解決の 4 か所）。合わせて約 5〜30 行。テストは differential test・CLI と REPL の paired test・compiler の unit test で約 150〜250 行。触る仕組みは VM の `for` の隠しローカルの名前の解決 1 つ、受入基準は 4 項目。

#### VM parity フェーズのスライス分割

> 状態: 設計のみ（本小節と次の小節は設計時点の記録）。現在の逸脱・未実装範囲は冒頭の逸脱一覧が正本である。

issue #525（kamonabe、仕分け「受け入れ」、「優先」）と[開発ループ手順書](loop-runbook.md) §6.7 の「tree と VM の差分をなくすフェーズ」を、本節 Slice 6（VM conformance）に置く。理由: 冒頭の未実装範囲の「VM 適合 gate のうち audit 以外（全規範 fixture の differential matrix、limit 境界・REPL 継続・pause/resume の差分 0 件の CI gate、fuzz gate）」と「VM の audit triple 対応」が、どちらも Slice 6 へ延期と記録されており、§2.1 の VM の適合条件（全規範 fixture で value・error・context commit・side effect・fuel が tree と一致、既知非適合 0 件）もここが正本だからである。execution-control・embedding-api の側にも VM の差（fuel・heap・`Engine::run` の backend）が書かれているが、フェーズの入口は 1 つにして、各スライスの小節から該当の spec を参照する。

- フェーズのスライスは、`slices.toml` で `parent = "VM parity"`（[ロードマップ](roadmap.md)「ステータス一覧」の `VM parity` の行）のものと、`audit-vm-triple`（VM の監査の配線、`parent = "Phase 6 残"` のまま）である。
- 最初は網羅的な差分テストで、`vm-parity-diff-a` と `vm-parity-diff-b` の 2 つに分ける（下の表）。以降のスライスは、差分テストの「既知の差分一覧」の項目を減らすもので、`vm-parity-diff-a` の done の後に一覧の中身を見て設計役が切る（項目ごと、または同じ原因の項目の組ごと）。
- フェーズのスライスには[開発ループ手順書](loop-runbook.md) §14.1 の行数・仕組みの数・受入基準の数の上限を当てず、1 回の実装（§14.3 の 90 分、実装役は Opus）で終わる量に割る（§6.7「大きさ」）。
- フェーズの終わり（フェーズのスライスがすべて done で既知の差分一覧が 0 件）の設計 PR で、§2.1 の VM の `experimental` を外すかを判断の候補にする（issue #525）。

| スライス | 範囲 | 依存 | 受入基準 |
|---|---|---|---|
| vm-parity-diff-a | 差分テストの土台（実行器・軸ごとの比較・既知の差分一覧と CI gate）、書き換えの種類 × 変数の置き場所の matrix、REPL 継続、print だけで動く fixture | — | VPD-AT-01..06 |
| vm-parity-diff-b | 残りの fixture（fs・env・stdin・clock・import の capability が要るもの）を同じ軸で比べる。全 fixture の stdout・stderr・終了コードの byte 比較（tree と `--vm` の CLI を直接比べる） | vm-parity-diff-a | b の設計 PR で採番 |

割り方の理由: 1 本にまとめると、94 本の fixture のうち capability の要る 26 本（`import`・fs builtin・`env`・`input`・`now` を使うもの、設計時点の main `a527274` で数えた）のために、in-process の tree と VM の両方へ fs の sandbox・env・stdin・import resolver を渡す実行器を作る作業が加わる。これは `tests/differential.rs` の `link_import_graph` と `tests/integration.rs` の CLI 実行器の両方を読み合わせる作業で、土台・matrix・一覧の初期化（差を 1 つずつ見て項目にする）と合わせると 90 分に収まらない見込みである。a は print だけの capability で閉じる範囲に絞り、一覧の形と gate を先に main へ入れる。b は a の実行器を使って fixture を足すだけになる。a の中で一覧の初期化に時間がかかりすぎる場合（差が多い）は、項目の説明を短くしてよいが、差を一覧に入れずに通すことはしない。

#### vm-parity-diff-a 実装スライス（tree と VM の網羅的な差分テストの土台と、既知の差分一覧の CI gate）

> 状態: **未実装**（設計のみ）。issue #525 から作ったフェーズの最初のスライス。§2.1 の適合方針（tree が規範）と §15.5 の tree/VM differential のうち、value・error・出力・fuel・budget の各カウンタ・REPL 継続を、今の VM が通らない差も含めて機械で数えられるようにする。差を直すのはこのスライスの範囲ではない。

設計時点（main `a527274`）の事実:

- `tests/differential.rs` は、宣言した fixture（source と上限の組）を in-process の tree（`Evaluator`）と VM（`Compiler` + `Vm`）で流し、`TsumugiError` の全 field と、`BudgetUsage` のうち engine 間で不変な軸（I/O の counter・string/source/import の count・per-item peak）を exact に、`committed.fuel` を F-1 以降 exact に、`live_heap_bytes` / `peak_heap_bytes` を向き・符号だけで比べる（§15.5 の PR-6c の注記）。比べるのは個別に書いた fixture だけで、`tests/fixtures/*.tsg` の全件ではない。
- `tests/integration.rs` は `tests/fixtures/*.tsg`（94 本）を CLI の tree と `--vm` の両方で流すが、比べる相手は期待ファイル（`.expected` など）で、fuel・budget・`TsumugiError` の field は見ない。VM だけの期待ファイル（`.vm`）は設計時点で 0 本である。
- heap は engine 間で数値を比べないため、COW の複製の回数の差（issue #502・#510・#517）は、どの検査にもかからずに残っていた。fix-cow-copy-parity 系の test（`tests/cow_copy_journal.rs` ほか）は、同じ engine の中で「操作をした script」と「しない script」の peak の差を取り、List body の大きさと比べて複製を見つけている。
- `Engine::run`（`src/embedding.rs`）と `Engine::execute`（`src/engine.rs`）は backend を見ず、`VmExperimental` の engine でも tree 評価器で実行する（[組み込みAPI仕様](embedding-api.md)の逸脱一覧の行、延期先は E9 の残り）。`ExecutionContext` を使う VM の経路は無い。

- **実装範囲**:
  - 新しい test file `tests/vm_parity.rs`（名前は実装 PR で変えてよい）に、1 つのケース（source、または REPL の入力の列）を tree と VM で流して、次の軸ごとに比べる実行器を置く。tree は `Evaluator`（単発は `run`、REPL は `run_repl_submission` と据え直し）、VM は `Compiler` + `Vm`（単発は `Vm::new(...).run()` と root source の `charge_link_graph`、REPL は `Vm::new_repl` と `compile_repl_line` / `run_repl_chunk`）で、`tests/differential.rs` の `run_tree_single` / `run_vm_single` / `tree_usage_per_input` / `vm_usage_per_input` と同じ入口を使う（共有したいなら `tests/support/` に移してよい。既存の test の期待は変えない）。capability は print だけ（`tests/differential.rs` の `print_capabilities` と同じ）。
    - **値**: 各ケースは観測したい値を `print` で出す。出力の比較で値を比べる。
    - **error**: terminal の `TsumugiError` の全 field（kind・message・line・call trace）を exact に比べる。compile error も含む。
    - **出力**: 出力の byte 列を比べる。in-process で byte 列を取れない engine があれば、I/O の counter（`output_calls`・`output_bytes`）の exact 一致で代え、byte 列の比較は vm-parity-diff-b の CLI 比較に任せる（どちらにしたかを test の冒頭に書く）。
    - **fuel**: `committed.fuel` を exact に比べる（F-1 以降の規則）。REPL は入力ごとの値を比べる。
    - **budget の各カウンタ**: `BudgetUsage` の `committed` と `reserved` の各 counter（fuel 以外）と `peaks` を exact に比べる。`live_heap_bytes` / `peak_heap_bytes` は、同じ engine の中で「操作をしたケース」と「操作をしない基準のケース」の peak・live の差を取り、その差を engine 間で exact に比べる（code artifact の分は差で打ち消す。`tests/cow_copy_journal.rs` と同じ方法）。基準のケースを持たない fixture は、今の `tests/differential.rs` と同じ向き・符号の比較にとどめる（下の Q3=A）。
    - **REPL 継続**: 入力の列を 1 つの session で流し、入力ごとに上の各軸を比べる。error になった入力の後の入力で、前の入力の binding の値と、error の入力の途中の書き換えが残らないこと（rollback）を、両方の engine で比べる。
  - **書き換えの種類 × 変数の置き場所の matrix** を、生成するケースとして持つ。
    - 書き換えの種類: `push`、`pop`、List の index 代入、Dict の index 代入（既存 key・新しい key）、`for` の反復（本体で反復中の List・Dict を書き換えるものと、書き換えないもの）。値の持ち方は、一意に持っている値と、別の変数と共有している値（`let b = a` の後に `a` を書き換える）の 2 つ。
    - 変数の置き場所: トップレベルの local、関数の local、upvalue（関数の中の lambda が外の変数を書き換える。1 段と 2 段の入れ子）、REPL の前の入力で宣言した変数、`Engine::run` / `Engine::execute` の `ExecutionContext` に前の実行で残った変数。
    - 各ケースは、書き換えた後の値（共有した側の値を含む）を出し、上の全軸を比べる。heap は上の「基準のケースとの差」の方法で比べ（各ケースに、書き換えをしない基準のケースを対で持つ）、COW の複製の回数の差を見つけられるようにする。
    - `Engine::run` の context の置き場所も matrix に入れる。tree は `Engine::run` / `Engine::execute` の `ExecutionContext` で流し、VM は経路が無いので「経路なし」として、置き場所ごと 1 項目（「VM に `Engine::run` の経路が無い」）を既知の差分一覧に入れる（下の Q1=A）。このスライスでは VM の経路を作らない。
  - **fixture**: `tests/fixtures/*.tsg` のうち、print だけの capability で動くものをすべて流す（ファイルの一覧を読んで回し、capability の要るものは名前の一覧で外す。外した名前の一覧は vm-parity-diff-b の対象として test に持つ）。`tests/integration.rs` の `fixture_timeout` のような停止性の fixture は、上限を付けて流してよい。
  - **既知の差分一覧**: 今ある差を、test の中の一覧（`const` の表、または `tests/vm_parity_known_diffs.toml` のような data file。形は実装 PR で決めてよい）に持つ。項目は「ケースの名前・軸・差の一行の説明・項目 ID（`KD-<番号>`）」で、同じ原因で多くのケースに出る差は、ケースの名前の pattern（接頭辞など）で 1 項目にまとめてよい。どの項目にも当たらない差が出たら test が落ちる。一覧の項目に当たる差が 1 件も出なくなった（直った）項目が一覧に残っていても test が落ちる（一覧と実際の差を完全一致させる。差を直すスライスは同じ PR で項目を消す。下の Q2=A）。
  - 一覧の初期の中身は、このスライスの実装 PR で test を流して作る。PR 本文に項目の一覧（ID・説明・件数）を書く。
  - CI: `cargo test` の一部として毎回流れる（新しい workflow は作らない。`.github/` は凍結）。
- **変えないもの**: `src/` の挙動（このスライスは test と spec だけを足す）。既存の test（`tests/differential.rs`・`tests/integration.rs`・golden fixture）の期待値。
- **範囲外**:
  - 差を直すこと。見つけた差は一覧に入れ、後続のフェーズのスライスで直す。
  - capability の要る fixture と、CLI の stdout・stderr・終了コードの byte 比較（vm-parity-diff-b）。
  - pause/resume・cancel・host pending（§15.5 の注記のとおり VM に scheduler の経路が無い）、`normalize_for_conformance` 後の audit payload（`audit-vm-triple` と後続）、record/replay、fuzz の生成 matrix（AUD-022）。ただし `Engine::run` の経路が無いことは、上のとおり一覧の 1 項目にする（Q1=A）。
  - limit 境界（各上限の N と N−1）の網羅。既存の `tests/differential.rs` の境界 test はそのまま残し、このスライスでは既定の上限で流す。
- **done PR で行う逸脱一覧の更新**: 冒頭の未実装範囲の「VM 適合 gate のうち audit 以外」の行を、差分テストの土台と既知の差分一覧の gate が入ったことに合わせて書き直す（全規範 fixture のうち capability の要るもの・limit 境界・pause/resume・fuzz と、差分 0 件の gate が残ることを書く）。
- **受入基準**:
  - **VPD-AT-01（軸）**: 実行器が、1 つのケースについて値（出力）・`TsumugiError` の全 field・出力・`committed.fuel`・`BudgetUsage` の fuel 以外の counter と peaks・heap（基準のケースとの差の方法）を tree と VM で比べ、差を「ケース・軸・tree の値・VM の値」の形で報告する。確かめ方: わざと差を作った組（tree と VM に違う source を渡す、など）で、各軸の差を報告することを確かめる test。
  - **VPD-AT-02（matrix）**: 書き換えの種類 × 値の持ち方 × 変数の置き場所の全組（`Engine::run` の context は、VM 側を一覧の 1 項目にする）のケースが生成され、全軸で比べられる。確かめ方: ケースの数を test で数えて組の数と一致させる。
  - **VPD-AT-03（REPL 継続）**: REPL の入力の列（error の入力を途中に含むもの、前の入力の変数を書き換えるもの、前の入力の closure を呼ぶもの）を両方の engine で流し、入力ごとに全軸で比べる。確かめ方: test。
  - **VPD-AT-04（fixture）**: print だけで動く `tests/fixtures/*.tsg` のすべてを全軸で比べる。外したものの名前の一覧が test にあり、外した理由（要る capability）が書いてある。確かめ方: test。fixture のファイルを増やしたとき、外す一覧に無ければ自動で比べる対象に入ることを、一覧の作り方で確かめる。
  - **VPD-AT-05（既知の差分一覧と gate）**: 一覧に無い差が 1 件でもあると test が落ちる。一覧の各項目は ID・軸・説明を持つ。差の出なくなった項目が一覧に残っていても test が落ちる（Q2=A）。確かめ方: 一覧から 1 項目を消すと test が落ちること、差の出ない項目を 1 つ足すと test が落ちることを、実装 PR の中で手で確かめて PR 本文に書く（一覧を引数に取る関数の unit test でもよい）。
  - **VPD-AT-06（非破壊）**: `src/` を変えない。既存の test が無改変で緑。確かめ方: `git diff --stat` と `cargo test`。
- **決定記録（判断役が選択（PR #526）。Q1=A・Q2=A・Q3=A）**:
  - **Q1（`Engine::run` の context の置き場所）= A**: matrix の置き場所に入れ、VM は「経路なし」を既知の差分一覧の 1 項目にする。理由: issue #525 が置き場所に `Engine::run` の context を挙げており、[組み込みAPI仕様](embedding-api.md)の逸脱一覧（`Engine::run` は `VmExperimental` でも tree で実行、延期先 E9）のとおり経路の欠けは今ある差である。一覧に載せれば [開発ループ手順書](loop-runbook.md) §6.7「終わり」（一覧 0 件）で数えられる（[マニフェスト](manifesto.md) 原則 7・原則 9）。§2.1 の「VM は全適合 gate を通るまで experimental」とも合う。このため、フェーズを終えるまでに VM の `Engine::run` の経路（E9 の残り）を扱うスライスが要る。採らなかった案: B（matrix から外して範囲外に書く）は経路の欠けが一覧に現れず、フェーズの終わりの判定から漏れる（原則 7 に反しうる）。C（このスライスで VM を配線する）は「差を直さない」「`src/` を変えない」（VPD-AT-06）と §6.7 の 90 分を破り、配線で入った差と元からの差を分けられない。
  - **Q2（差の出なくなった項目）= A**: 一覧に残っていたら test を落とす（一覧と実際の差を完全一致させる）。理由: 直った差が戻ったときに残った項目が隠してしまうのを防ぎ（原則 5）、実際より多い一覧は裏付けのない記録になる（原則 9）。§6.7「終わり」を一覧だけで判定できる。issue #525 の「一覧が増えたら CI が落ちる」も満たす。採らなかった案: B（増えたときだけ落とす）は消し忘れが通り、同じ差の戻りを検出できない。
  - **Q3（heap の比べ方）= A**: 同じ engine の中で操作をしたケースと基準のケースの peak・live の差を取り、その差を engine 間で exact に比べる。基準を持たない fixture は向き・符号の比較。理由: [実行予算・協調実行仕様](execution-control.md) §5.3 の A-1 と §15.5 は code artifact の heap 数値を engine 固有でよいとするが、差を取れば code artifact の分が打ち消せる。`tests/cow_copy_journal.rs` が同じ方法で複製を見つけた実績があり、原則 5 を `src/` を変えずに検査できる（原則 8）。採らなかった案: B（COW の複製の回数の内部 counter を足す）は `src/` を変え（VPD-AT-06）、検査のためだけのデータ構造を中核に足す。C（向き・符号だけ）は COW の複製の差（#502・#510・#517）を見逃したままになる（原則 5 に反しうる）。
- **想定の変更規模**: `src/` 0 行。test は `tests/vm_parity.rs` 約 600〜1000 行（実行器 200〜300、matrix の生成 150〜250、REPL と fixture 150〜250、一覧と gate 100〜200）。触る仕組みは tree/VM の差分テスト 1 つ、受入基準は 6 項目。§6.7 のとおり §14.1 の上限は当てない。

#### vm-parity-diff-b 実装スライス（capability の要る fixture と CLI の stdout・stderr・終了コードの byte 比較）

> 状態: **未実装**（設計のみ）。上の「VM parity フェーズのスライス分割」小節の表の 2 本目。vm-parity-diff-a（PR #528 で done）の実行器・軸・既知の差分一覧の gate を使い、比べる範囲を capability の要る fixture と CLI の観測（stdout・stderr・終了コード）へ広げる。差を直すのはこのスライスの範囲ではない。

設計時点（main `9b1c6d4`）の事実:

- `tests/vm_parity.rs`（1087 行）は、流し方 `Mode::Single`（tree は `Evaluator::run`、VM は `Compiler` + `Vm::with_budget` + `charge_link_graph(root, &[])` + `run`）・`Mode::Repl`・`Mode::Context`（VM は経路なし）を持ち、stdout だけを grant した capability（`capture_capabilities`）で流す。既知の差分一覧 `KNOWN_DIFFS` は KD-1（`Engine::execute` の VM の経路なし）・KD-2/KD-3（`for` の反復の peak heap）・KD-4（REPL の error の入力の fuel）の 4 項目で、gate は流したケースの名前の接頭辞（`scope`）の範囲で「差の出ない項目」を見る。
- `tests/fixtures/*.tsg` は 94 本で、print だけで流せない 25 本を `FIXTURES_FOR_DIFF_B` に理由付きで持つ。内訳は env 2（`env_allow`・`env_protected_windows`）、fs・env・clock 5（`error_sandbox`・`file_io`・`filesystem`・`local_utils`・`string_utils`）、import の root 9、import される側 9（`tests/integration.rs` の `HELPER_FIXTURES` と同じ）。stdin（`input`）を使う fixture は無い。
- `tests/integration.rs` は全 fixture を CLI の tree と `--vm` で流すが、比べる相手は期待ファイル（`.expected`・`.expected_err`・`.expected_out`。`.vm` は 0 本）で、tree と VM を直接は比べない。CLI の組み立ては fixture 名から決める: `fixture_needs_legacy`（`--profile legacy`）・`fixture_has_import`（`--allow-import-root default=<fixtures>`）・`fixture_envs`（`TSG_TEST_DIR`・`TSUMUGI_SANDBOX`・`TSUMUGI_ENV_ALLOW`・`TSUMUGI_MAX_STEPS`）・`fixture_timeout`。VM の stderr の先頭の experimental の警告（`WARN_VM_EXPERIMENTAL`）は `stderr_without_vm_warning` で 1 行だけ除いてから比べる（警告そのものは `tests/cli_vm_warning.rs`）。`CUSTOM_FIXTURES`（`env_allow`・`env_protected_windows`・`error_stack_overflow`）は専用の test で流し、`env_protected_windows` は Windows でだけ流れる。
- CLI の tree は `Engine::link` → `Engine::run`（`src/main.rs`）、CLI の VM は同じ `Engine::link`（capability の gate）→ `LinkedScript::expand_for_cli` → `Compiler` → `Vm::with_budget` → `set_capabilities` → `charge_link_graph(root, &expanded)` → `run`（`execute_vm_with_path`）である。`tests/differential.rs` の `link_import_graph` / `run_tree_import_graph` / `run_vm_import_graph` が in-process で同じ組み立てを持つ。`Evaluator` は単独では import を解決しない。
- CLI の legacy profile の capability set を組む `build_legacy_capabilities` は binary の中（`src/cli_capability.rs`、`mod cli_capability` は `src/main.rs`）にあり、integration test からは呼べない。公開 API には `CapabilitySet::builder().legacy_fs_syntax()`・`FilesystemCapability::new_legacy`・`EnvironmentSnapshot::from_entries`・`FixedClock`・`FilesystemModuleResolver` がある。

- **実装範囲**:
  - **capability の要る fixture の in-process 比較**: `FIXTURES_FOR_DIFF_B` の 25 本（env 2・fs/env/clock 5・import の root 9・import される側 9。import される側も単体の root として流す。下の Q3=A）を、新しい流し方（例 `Mode::Linked`。名前は実装 PR で決めてよい）で tree と VM に流し、vm-parity-diff-a と同じ軸（error・出力・fuel・budget の counter と peaks・heap）で比べる。heap は基準のケースを持たないので向きの比較にとどめる（vm-parity-diff-a の Q3=A）。
    - tree と VM の入口は CLI と同じ組み立てにする（下の Q1=A）。tree は `Engine::compile` → `Engine::link` → `Engine::run`、VM は同じ `LinkedScript` を `expand_for_cli` で展開して `Compiler` → `Vm::with_budget(config())` → `set_capabilities` → `charge_link_graph(root, &expanded)` → `run` とする（`tests/differential.rs` の `run_tree_import_graph` / `run_vm_import_graph` と同じ。共有したいなら `tests/support/` に移してよい）。link の error は両 engine に同じ観測（parse error と同じ扱い）として返す。上限は vm-parity-diff-a の `config()` を両方に渡す。
    - capability は、公開 API で両 engine に同じ set を組んで渡す: stdout は vm-parity-diff-a の capture、env は fixture ごとに `tests/integration.rs` と同じ key/value の `EnvironmentSnapshot`（`TSG_TEST_DIR` など。`env_protected_windows` は専用 test と同じ secret marker の key を入れ、`env()` が読めないことを両 engine で比べる）、clock は両方に同じ `FixedClock`、fs は legacy の host path 翻訳付き（`legacy_fs_syntax` と `FilesystemCapability::new_legacy`）で `TSUMUGI_SANDBOX` 相当の root を 1 つ、import は fixtures ディレクトリを `default` mount にした `FilesystemModuleResolver`。どの fixture に何を渡すかは fixture 名の表で持ち、表に理由を書く。
    - fs を書く fixture は、engine ごとに空のディレクトリで流す。出力に path が出ても一致するよう、tree と VM は同じ path のディレクトリを使い、tree の実行の後に消して作り直してから VM を流す。
  - **CLI の byte 比較**: すべての `tests/fixtures/*.tsg`（import される側の 9 本も単体の root として含む。下の Q3=A）を、`tsumugi` と `tsumugi --vm` で流し、stdout の byte 列・stderr の byte 列・終了コードを比べる。CLI の引数・環境変数・時間の上限は `tests/integration.rs` の `fixture_needs_legacy`・`fixture_has_import`・`fixture_envs`・`fixture_timeout` と同じにする（`tests/support/` に移して両方から使ってよい。`tests/integration.rs` の期待は変えない）。`CUSTOM_FIXTURES` の `env_allow`・`env_protected_windows` は、それぞれの専用 test と同じ環境変数で流す（`env_protected_windows` は `TSUMUGI_ENV_ALLOW` の有無の 2 通りを別のケースにし、OS を問わず流す。tree と VM の比較は OS に依らないため）。fs を書く fixture は in-process と同じく同じ path のディレクトリを作り直して流す。時間の上限を超えたら差ではなく test の失敗にする。
    - VM の stderr の先頭の experimental の警告は、`tests/integration.rs` の `stderr_without_vm_warning` と同じく、VM の stderr から `WARN_VM_EXPERIMENTAL` の 1 行だけを除いてから byte で比べる（下の Q2=A）。
    - 軸は CLI の stdout・stderr・終了コードの 3 つを足す（例 `Axis::CliStdout`・`Axis::CliStderr`・`Axis::CliExit`）。ケースの名前は `cli/<stem>`（`env_protected_windows` は `cli/env_protected_windows/<allow の有無>` のように分ける）、in-process の新しいケースは `linked/<stem>` とする。差は vm-parity-diff-a と同じ「ケース・軸・tree の値・VM の値」で報告し、byte 列は差の最初の位置と前後が読める形で出す。
  - **fixture の振り分け**: すべての `tests/fixtures/*.tsg` が、in-process では print だけの流し方（`fixture/<stem>`）・capability の流し方（`linked/<stem>`。import される側を含む）のどちらか 1 つに入り、CLI ではすべてが `cli/<stem>` に入ることを test で確かめる。`FIXTURES_FOR_DIFF_B` は役目に合わせて名前を変えてよい（例 `FIXTURES_WITH_CAPABILITY`）。fixture のファイルを足したとき、どの表にも無ければ print だけの流し方と CLI の比較に自動で入る（vm-parity-diff-a の VPD-AT-04 の作り方を保つ）。
  - **既知の差分一覧**: 新しく流すケースで出た差を `KNOWN_DIFFS` に `KD-5` 以降として足す（同じ原因の差は名前の pattern で 1 項目にまとめてよい。vm-parity-diff-a と同じ）。gate の「差の出ない項目」の検査は `linked/` と `cli/` の範囲にも当てる。今ある KD-1〜KD-4 は変えない。PR 本文に足した項目の一覧（ID・軸・説明・件数）を書く。
- **変えないもの**: `src/` の挙動（このスライスは test と spec だけを足す）。既存の test の期待値（`tests/integration.rs`・`tests/differential.rs`・golden fixture・vm-parity-diff-a の各ケースと KD-1〜KD-4）。fixture のファイル。
- **範囲外**:
  - 差を直すこと。見つけた差は一覧に入れ、後続のフェーズのスライスで直す。
  - 新しい fixture を足すこと（stdin の `input` を使う fixture は今 1 本も無く、このスライスでは作らない）。CLI の source を stdin から読む経路（`tsumugi -`）と、CLI の対話 REPL の byte 比較（REPL 継続は vm-parity-diff-a の in-process の比較で見ている）。
  - limit 境界（各上限の N と N−1）の網羅、pause/resume・cancel・host pending、監査の軸（audit-vm-triple-b、上の「audit-vm-triple のスライス分割」小節の Q2=A の条件）、fuzz。
  - CLI の legacy profile の capability set を組む関数（`build_legacy_capabilities`）を library へ出すこと。in-process の比較は公開 API で同じ内容の set を組み、CLI が実際に組む set は CLI の byte 比較で見る。
- **done PR で行う逸脱一覧の更新**: 冒頭の未実装範囲の「VM 適合 gate のうち audit 以外の残り」の行から「capability の要る fixture と CLI の stdout・stderr・終了コードの byte 比較（vm-parity-diff-b）」を消し、既知の差分一覧の項目の数と中身をこの PR で足した項目に合わせて書き直す。
- **受入基準**:
  - **VPD-AT-07（capability の要る fixture）**: capability の要る fixture（設計時点で 25 本。import される側の 9 本を単体の root として含む。Q3=A）のすべてを、CLI と同じ入口（Q1=A）で tree と VM に流し、vm-parity-diff-a と同じ軸で比べて gate にかける。確かめ方: test。
  - **VPD-AT-08（capability の渡し方）**: 両 engine に同じ内容の capability（env・clock・fs・import resolver・stdout）が渡り、fs を書く fixture は engine ごとに同じ path の空のディレクトリで流れる。確かめ方: 同じ流し方で、env の値・`now()`・ファイルの書き込みと `path_exists` を出す小さい source を両 engine で流し、出力が一致し、後に流した engine から前の engine の書いたファイルが見えないことを確かめる test。
  - **VPD-AT-09（CLI の byte 比較）**: `tests/fixtures/*.tsg` のすべて（import される側を含む。Q3=A）を `tsumugi` と `tsumugi --vm` で `tests/integration.rs` と同じ引数・環境変数で流し、stdout・stderr（`WARN_VM_EXPERIMENTAL` の 1 行を除いた後。Q2=A）・終了コードを byte で比べて gate にかける。差は「ケース・軸・tree の値・VM の値」で報告する。確かめ方: test と、わざと差を作った組（tree と VM に違う fixture を渡す、など）で 3 つの軸それぞれの差を報告することを確かめる test。
  - **VPD-AT-10（fixture の振り分け）**: すべての `tests/fixtures/*.tsg` が in-process の流し方のどちらか 1 つと CLI の比較に入り（import される側は `linked/` と `cli/`。Q3=A）、どの表にも無い新しい fixture は print だけの流し方と CLI の比較に自動で入る。確かめ方: 一覧の作り方を確かめる test（表の名前が実在のファイルに当たること、振り分けが重ならず漏れないこと）。
  - **VPD-AT-11（既知の差分一覧の更新）**: 新しく流したケースの差がすべて一覧の項目に当たり、`linked/`・`cli/` の範囲でも差の出ない項目が残っていれば落ちる。KD-1〜KD-4 は変わらない。確かめ方: test と、PR 本文の足した項目の一覧（ID・軸・説明・件数）。
  - **VPD-AT-12（非破壊）**: `src/` を変えない。既存の test が期待を変えずに緑（`tests/support/` へ移した関数を使う形にした test も、期待は同じ）。確かめ方: `git diff --stat` と `cargo test`。
- **決定記録（判断役が選択（PR #533）。Q1=A・Q2=A・Q3=A）**:
  - **Q1（capability の要る fixture の in-process の入口）= A**: CLI と同じ 1 つの組み立てで流す。tree は `Engine::compile` → `Engine::link` → `Engine::run`、VM は同じ `LinkedScript` を `expand_for_cli` → `Compiler` → `Vm::with_budget` → `charge_link_graph(root, &expanded)` → `run`。理由: `src/main.rs` の `execute_vm_with_path` と `tests/differential.rs` の `run_tree_import_graph` / `run_vm_import_graph` がこの組み立てで、利用者が実際に通る経路で tree と VM を比べるので、[マニフェスト](manifesto.md) 原則 5（backend ごとに結果・資源消費が変わらない）の検査として一番直接である。流し方が 1 つで済み、既存の組み立てを使う（原則 8）。採らなかった案: B（import の root だけ A にし、env・fs・clock の 7 本は `Evaluator::run` / `Vm::run` に capability を足す）は、流し方が 2 つに分かれて表と実行器が増え（原則 8）、7 本を CLI が通らない入口で比べることになり、`Engine::run` との違い（link の課金の位置など）が engine の差と混ざって読みにくくなる（原則 9）。
  - **Q2（CLI の VM の stderr の experimental の警告）= A**: VM の stderr から `WARN_VM_EXPERIMENTAL` の 1 行だけを除いて byte で比べる。理由: 警告は §2.1「VM は全適合 gate を通るまで experimental」を知らせる意図した出力で、`tests/cli_vm_warning.rs` が別に確かめており、tree と VM の意味論の差ではない。除くのは最初の 1 行だけなので、ほかの stderr の差は残る。[開発ループ手順書](loop-runbook.md) §6.7「終わり」（一覧 0 件の後に experimental を外すかを判断）とも合う（原則 5・原則 9）。採らなかった案: B（除かずに `cli/*` の stderr を既知の差分一覧の 1 項目にする）は原則 5 に反しうる。`KnownDiff::matches` は軸と名前の pattern だけで当てて値を見ないので、その範囲の stderr のほかの差がすべてその項目に当たって gate にかからなくなる。フェーズの終わりの条件（一覧 0 件）も experimental を外すまで満たせなくなる。
  - **Q3（import される側の 9 本）= A**: 単体の root としても in-process（`linked/`）と CLI（`cli/`）の両方で流す。理由: 上の「VM parity フェーズのスライス分割」小節の表は vm-parity-diff-b を「全 fixture の stdout・stderr・終了コードの byte 比較」としており、そのまま満たす。tree と VM の直接比較は期待ファイルが要らないので、単体での parse error・runtime error の経路も比べられる（原則 5）。振り分けの表から「外すもの」が無くなる（原則 8）。採らなかった案: B（外して、import する側を通してだけ比べる）は 9 本の単体の差を見ないままにし、「全 fixture」から外れる例外の一覧を test に持つことになる。`tests/integration.rs` の `HELPER_FIXTURES` が単体で流さないのは期待ファイルとの比較の都合で、tree と VM の比較には当てはまらない。
- **想定の変更規模**: `src/` 0 行。test は `tests/vm_parity.rs` に約 450〜750 行（capability の流し方と capability の表 150〜250、CLI の実行器と 3 軸の比較 150〜250、振り分けと gate の拡張 50〜100、VPD-AT-08・09 の確かめの test 100〜150）。`tests/integration.rs` の CLI の組み立てを `tests/support/` へ移すなら、その移動の分（約 100 行、期待は変えない）。触る仕組みは tree/VM の差分テスト 1 つ、受入基準は 6 項目。§6.7 のとおり §14.1 の上限は当てない。割り方の理由: 実行器・軸・gate は vm-parity-diff-a にあり、このスライスは流し方を 2 つ（capability の in-process・CLI）足すだけである。CLI の 94 本 × 2 の起動は `tests/integration.rs` が毎回流している量と同じで、一覧の初期化（差を見て項目にする）を含めても 1 回の実装（§14.3 の 90 分）に収まる見込みである。これ以上分けると、CLI の比較だけのスライスが in-process の比較と同じ fixture の表を別々に持つことになる。

#### audit-vm-triple のスライス分割

> 状態: 設計のみ（本小節と次の小節は設計時点の記録）。現在の逸脱・未実装範囲は冒頭の逸脱一覧が正本である。

`slices.toml` の `audit-vm-triple`（VM の audit triple 対応。冒頭の逸脱一覧の「audit triple / yield/resume の emission が tree-only」の行と、未実装範囲の「VM の audit triple 対応（監査 wiring）」の行の延期先）を、[開発ループ手順書](loop-runbook.md) §6.7 のフェーズのスライスとして設計する。§6.7「大きさ」のとおり §14.1 の上限は当てず、1 回の実装（§14.3 の 90 分、実装役は Opus）で終わる量に割る。元の id `audit-vm-triple` は下の 3 つに置き換えて残さない（audit-strict-cbor の分割と同じ扱い）。上の「VM parity フェーズのスライス分割」小節がフェーズのスライスとして挙げた `audit-vm-triple` は、この 3 つを指す。

設計時点（main `a2f3106`）の事実:

- 監査 event を積むのは tree 評価器だけである。`src/eval.rs` の `Evaluator` が buffer（`audit_events`）・host call の `operation_id` の counter（`next_host_operation_id`）・`host_effects_may_remain` の flag・redaction policy を持ち、`src/builtin.rs` の fs host call が「accumulator の flush（`flush_charges_into_audit_events`）→ 内容の描画 → host call attempt と request の課金 → `dispatch_fs` → triple の構築（`builtin_core::host_call_triple_with_bodies`）→ `push_host_call_events`」の順に進む。triple を出すのは `builtin_core::emits_host_call_triple` の 4 builtin だけ。
- `src/vm.rs` の `Vm::exec_builtin` は、課金（描画・attempt・request）と `dispatch_fs` を tree と同じ共有関数で行うが、flush をせず、`dispatch_fs` が返す canonical label を捨て（コメント「VM は監査 triple を組まない」）、triple を作らない。`Vm` は `BudgetLedger`（`self.budget`）を持つので、`BudgetCharged` の accumulator（`drain_budget_charges`）はすでにある。
- `Engine::run` / `Engine::run_audited`（`src/embedding.rs`）は backend を見ず、`VmExperimental` の engine でも tree で実行する（[組み込みAPI仕様](embedding-api.md)の逸脱一覧、延期先 E9 の残り）。監査 sink と `VmExperimental` の同時設定は `EngineBuilder::build` が `ConfigError::AuditSinkWithExperimentalBackend` で拒否する。`Vm` は deadline clock・cancel token・登録 host function の注入も持たない。
- VM には slice の境界での yield が無い（`Vm::run` は最後まで回す）。audit backpressure の `Yielded`/`Resumed`（B-1・B-2）は、tree では `run_audited_driver` が `run_slice` を回して作っている。

| スライス | 範囲 | 依存 | 状態 |
|---|---|---|---|
| audit-vm-triple-a | VM の中で、tree と同じ event 列（fs 4 builtin の host-call triple と、host call 開始前の `BudgetCharged` の flush）を buffer へ積む。facade の配線はしない | — | ready（下の小節） |
| audit-vm-triple-b | `Engine::run_audited` の VM 経路: Started・a の buffer の drain・終了時の `BudgetCharged`・Terminal（`host_effects_may_remain` を含む）を tree と同じ journal・sink の規則で出す。redaction policy の注入 | audit-vm-triple-a と、VM の `Engine::run` の経路のスライス（下の小節の決定記録 Q1=A。vm-parity-diff-a の done の後に設計） | todo |
| audit-vm-triple-c | VM の audit backpressure（audit drain queue の byte 計上と、トップレベル境界での `Yielded`/`Resumed(AuditBackpressure)`）と、`ConfigError::AuditSinkWithExperimentalBackend` の撤去。逸脱一覧の「tree-only」の行を消す。`tests/vm_parity.rs` への監査の軸の追加を受入基準に入れる（audit-vm-triple-b の小節の決定記録 Q2=A の条件 1、PR #535） | audit-vm-triple-b | todo |

割り方の理由:

- 1 本にまとめると、(1) VM の builtin 経路に triple と flush 点を足す、(2) VM を `Engine::run_audited` に載せる（journal・sink・Started/Terminal・fail-closed）、(3) VM に yield の境界を作って backpressure を通す、の 3 つの仕組みに触れる。(2) は VM を `ExecutionContext` で回す経路そのもの（E9 の残り）を前提にし、その経路は今どのスライスにも無い（vm-parity-diff-a の決定記録 Q1 で「フェーズを終えるまでに要る」とされたもの）。前提ごと 1 回で入れると 90 分に収まらない。
- (1) は facade に触れずに、同じ source を `Evaluator` と `Vm` に流して buffer を比べるだけで確かめられる。先に入れれば、b と c は「drain して配送する」だけになり、event の中身の差（課金の順序・label・body）と配線の差を別の PR で切り分けられる。
- (3) を (2) から分けるのは、VM に yield の境界を足す変更が VM の実行ループ（`Vm::run`）に触れ、sink が常に `Ack` を返す経路（b）とは確かめ方も壊れ方も違うためである。`ConfigError::AuditSinkWithExperimentalBackend` は、backpressure まで揃って「監査付きの VM 実行が tree と同じ event 列を出す」と言える c で外す（b の間に外すと、backpressure の無い VM 監査が使えてしまう）。
- b・c は、VM の `Engine::run` の経路のスライスが決まるまで受入基準を採番しない（todo）。a が done になり、その経路のスライスが決まった後の設計 PR で採番して ready にする。その経路のスライスは、vm-parity-diff-a の done の後に既知の差分一覧の項目「VM に `Engine::run` の経路が無い」を減らすフェーズのスライスとして別に設計し、b はそれに依存させる（下の小節の決定記録 Q1=A）。b の設計 PR は、`tests/vm_parity.rs` への監査の軸の追加を受入基準に入れる（同 Q2=A の条件）。

追記（判断役が選択（PR #535）、audit-vm-triple-b の小節の決定記録 Q2=A の条件 1）: 上の「b の設計 PR は、`tests/vm_parity.rs` への監査の軸の追加を受入基準に入れる」（PR #527 の Q2=A の条件）は、移し先を audit-vm-triple-c に改める。c の設計 PR が `tests/vm_parity.rs` への監査の軸の追加を受入基準に入れる。b の間は build が監査 sink と `VmExperimental` の同時設定を拒否し、facade から観測できる監査の差がまだ生じないためである。

#### audit-vm-triple-a 実装スライス（VM の fs host call で host-call triple と flush 点を buffer へ積む）

> 状態: **未実装**（設計のみ）。VM が tree と同じ監査 event を、同じ順序・同じ中身で内部の buffer へ積むようにする。sink への配送（facade の配線）はこのスライスの範囲ではない。

- **実装範囲**:
  - `Vm` に、tree の `Evaluator` と同じ役割の状態を持たせる: 監査 event の buffer、host call の `operation_id` の counter（0 始まり・単調・`checked_add` で溢れたら internal error）、`host_effects_may_remain` の flag、redaction policy（既定は `RedactionPolicy::default()`、tree と同じ）。counter と flag と buffer を 0・空へ戻す時点は tree と同じにする（tree の `reset_budget` に当たる VM の入口: `Vm::with_budget` / `Vm::new` と、REPL の `begin_repl_submission`）。
  - `Vm::exec_builtin` の fs host call を、`src/builtin.rs` と同じ順序にする: `emits_host_call_triple` の builtin では、内容の描画より前に accumulator を flush して確定した `BudgetCharged` を buffer へ積む（§7.3、fix-audit-flush-before-host-call と同じ位置）。`dispatch_fs` が返す canonical label を捨てずに受け取り、`builtin_core::host_call_body_values` と `host_call_triple_with_bodies` で triple を作って buffer へ積む。deny・host error でも triple を積んでから error を伝播する（tree と同じ）。
  - triple の構築・flush・push は tree と同じ共有関数を呼ぶ。VM 専用の写しを作らない（片方だけ直る差を作らないため、原則 5・原則 8）。共有のために `src/eval.rs` 側の処理を `src/builtin_core.rs` か `src/audit.rs` の関数へ移してよい（tree の観測挙動は変えない）。
  - crate 内から buffer を取り出す `pub(crate)` の drain（tree の `drain_audit_events` と同じ意味）と、`host_effects_may_remain` の読み出しを `Vm` に置く。
  - sink 無しの経路の観測挙動（値・出力・error・fuel・`BudgetUsage`）は変えない。buffer へ積むことは tree と同じく常に行ってよい（tree が sink 無しでも積むのと揃える）。
- **範囲外**:
  - `Engine::run` / `Engine::run_audited` の VM 経路、Started・Terminal、journal・sink への配送（audit-vm-triple-b）。
  - audit drain queue の byte 計上（`set_audit_queue` / `audit_queue_admit` に当たるもの）と backpressure の `Yielded`/`Resumed`、`ConfigError::AuditSinkWithExperimentalBackend` の撤去（audit-vm-triple-c）。
  - fs 4 builtin 以外の host call の triple（audit-host-boundary-scope、fs builtin の監査範囲のスライス）。tree が出さないものは VM も出さない。
  - `normalize_for_conformance`（未実装範囲の行のまま）。このスライスの比較は event をそのまま比べる。
  - vm-parity-diff-a の差分テスト（`tests/vm_parity.rs`）への監査の軸の追加。audit-vm-triple-b で行い、b の設計 PR の受入基準に入れる（下の決定記録 Q2=A の条件）。
- **done PR で行う逸脱一覧の更新**: 冒頭の逸脱一覧の「audit triple / yield/resume の emission が tree-only」の行を、「VM は host-call triple と flush 点の event を内部 buffer へ積むが、監査付き実行（`run_audited`）の経路が無いため配送しない。yield/resume は tree-only」と書き直す（行は c で消す）。未実装範囲の「VM の audit triple 対応（監査 wiring）」の行は、延期先を audit-vm-triple-b・c に改める。
- **受入基準**:
  - **AVT-AT-01（event 列の一致）**: fs 4 builtin（`read_file`・`read_lines`・`write_file`・`append_file`）を、allow・deny（capability 未 grant と operation 不足）・host error（存在しないファイルの読み込み）の各経路で呼ぶ source を、同じ capability と budget で `Evaluator` と `Vm` に流し、drain した event 列（`HostCallStarted`・`CapabilityDecision`・`HostCallFinished`・`BudgetCharged` の全 field と順序）が完全に一致する。確かめ方: tree と VM に同じ source を流す table-driven の test（crate 内の `#[cfg(test)]`。下の決定記録 Q2=A）。
  - **AVT-AT-02（flush の位置）**: host call より前の文で fuel などを消費した source で、その消費の `BudgetCharged` が host call の `HostCallStarted` より前に、host call 自身の課金（描画・attempt・request・response）の `BudgetCharged` が次の flush 点以降に出ることを、VM で確かめ、tree と一致する。確かめ方: AVT-AT-01 と同じ test の 1 ケース。
  - **AVT-AT-03（operation_id と flag）**: 1 回の実行の中で `operation_id` が 0 から単調に振られ、triple の中で同じ値を使う。REPL では入力ごとに 0 へ戻る（tree と同じ）。`effect_status` が `Committed` か `Unknown` の host call を行った実行で `host_effects_may_remain` が `true`、deny だけの実行で `false` になる。確かめ方: tree と VM の比較 test（単発と REPL の入力の列）。
  - **AVT-AT-04（redaction と label）**: Path が canonical label の HMAC、Body が既定 policy で `Omitted` になり、label を取れない経路（route 前の拒否）でも tree と同じ `original_bytes` になる。確かめ方: AVT-AT-01 の event の比較に含む。
  - **AVT-AT-05（非破壊）**: sink 無しの経路の観測挙動を変えない。既存の test（`tests/audit_*.rs`・`tests/differential.rs`・`tests/integration.rs`・golden fixture）が無改変で緑。確かめ方: `cargo test`。
- **決定記録（判断役が選択（PR #527）。Q1=A・Q2=A）**:
  - **Q1（VM の `Engine::run` / `Engine::run_audited` の経路をどこで作るか）= A**: vm-parity-diff-a の done の後に、既知の差分一覧の項目「VM に `Engine::run` の経路が無い」を減らすフェーズのスライスとして別に設計し、audit-vm-triple-b はそれに依存させる。理由: vm-parity-diff-a の決定記録 Q1=A（PR #526）が経路の欠けを一覧の 1 項目として数え、「フェーズを終えるまでに VM の `Engine::run` の経路（E9 の残り）を扱うスライスが要る」と書いており、[開発ループ手順書](loop-runbook.md) §6.7「以降のスライスは一覧の項目を減らす」と同じ形で 1 項目を 1 スライスで消せる（既存の決定との一貫性、[マニフェスト](manifesto.md) 原則 9）。経路は[組み込みAPI仕様](embedding-api.md)の逸脱一覧のとおり監査より広く（deadline・cancel・host function・context の継続を含む）、監査の系列に入れずに分けたほうが 1 スライスの範囲が小さく、配線で入る差と監査 event の差を別の PR で切り分けられる（原則 8・原則 5）。採らなかった案: B（audit-vm-triple の分割に b の前の sub-slice として含める）は、E9 の残りを監査のスライス（parent「Phase 6 残」）が抱えて範囲が広がり、PR #526 の Q1=A で決めた「一覧の項目を減らすスライス」と消し方が二重になる。§6.7「大きさ」の 90 分にも収めにくい。
  - **Q2（a の tree/VM の一致をどこで確かめるか）= A**: crate 内の `#[cfg(test)]` の比較 test（同じ source を `Evaluator` と `Vm` に流し、drain した event の全 field と順序を exact に比べる）で確かめ、`tests/vm_parity.rs` への監査の軸の追加は、監査付き実行が VM で通る audit-vm-triple-b で行う。理由: a の範囲は VM の内部 buffer へ積むところまでで facade からは観測できず、buffer の drain は tree でも `pub(crate)`（`Evaluator::drain_audit_events`）なので、integration test から比べるには検査のためだけの公開（`#[doc(hidden)]` など）を足すことになる（原則 8、公開 API を広げない）。A でも同じ入力を tree と VM に流して一致を確かめるので、[開発ループ手順書](loop-runbook.md) §14.1 の趣旨と原則 5・原則 9 は a の時点で満たせる。a は vm-parity-diff-a（未 done）に依存せず、今 ready にできる。**条件**: audit-vm-triple-b の設計 PR は、`tests/vm_parity.rs` への監査の軸の追加を受入基準に入れる（この小節の「範囲外」に書いた先送りを落とさない。§14.1「受入基準に §6.7 の差分テストへの追加を含める」）。採らなかった案: B（`tests/vm_parity.rs` に監査の軸を足す）は、差分テストのために buffer を crate の外へ出す必要があり、a が todo に戻って vm-parity-diff-a 待ちになる。facade から観測できる監査の差は b で初めて生じるので、差分テストの軸は b で足せば足りる。
- **決定記録（判断役が選択（PR #531）。Q1=A）**:
  - **Q1（AVT-AT-01 の比較で、`BudgetCharged { resource: HeapBytes }` を除いた event 列での一致を受入基準の達成と認めるか）= A**: 認める。AVT-AT-01 の比較は `HeapBytes` の `BudgetCharged` を除いた event 列で行い（残りの順序は保つ）、triple と fuel・I/O・string・source の `BudgetCharged` は全 field と順序を exact に比べる。コードは変えない。audit-vm-triple-b の設計で `tests/vm_parity.rs` に監査の軸を足すときも、同じ除外を基準に書く。理由: (1) 既存の決定との一貫性。§15.5 の PR-6c 線引きは absolute heap（`live_heap`/`peak_heap`）を向き・符号・release 寿命で突合し、byte 単位の一致を求めない。§15.5 本文も audit payload の完全一致を `normalize_for_conformance` の後に求めており、生の event の一致は目標仕様の要求ではない。vm-parity-diff-a の決定記録 Q3=A（PR #526）も heap を engine 間で直接比べない。(2) 比べる相手の tree 側がまだ目標に達していない。冒頭の逸脱一覧の「heap の増減の一部が `BudgetCharged` の accumulator へ通知されない」の行のとおり、tree が heap で通知するのは `release_heap`（frame・try の戻しと retrack の縮小）だけで、fix-heap-charge-audit-notify は `blocked` である。今の tree の `HeapBytes` event は tree の frame 構造の写しで、VM をそれに揃えても §7.3 の目標に近づかない。(3) 差は隠れない。逸脱一覧の「VM は host-call triple と flush 点の event を内部 buffer へ積む」の行に「`HeapBytes` は tree と一致しない」と書き、この決定記録にも残す（[マニフェスト](manifesto.md) 原則 9）。heap 以外の検査は弱めない（原則 5）。(4) 変更が最小で、このスライスの範囲（buffer へ積むこと）を広げない（原則 8）。実測: 除外を外すと allow のケースで tree 42 件・VM 41 件になり、最初の差は tree だけにある `HeapBytes`（`committed_delta: 0`・`released_delta: 288`、96 byte の block scope frame 3 つ分）だった。§5.3 の A-1 は code artifact（`ast_node`/`bytecode_chunk`）の決定で block scope frame を直接は扱わないため、A を支える根拠は A-1 より PR-6c 線引きの absolute heap の扱いと上の逸脱一覧の行である。採らなかった案: B（VM の heap 課金を tree の frame・token に合わせてから HeapBytes も比べる）は、VM の `BudgetUsage`（live/peak heap）という観測値を変える別の変更で、§15.5「heap の数値は engine 固有でよい・A-2 は採らない」と食い違い、未完成の tree の通知に合わせる作業にもなる。このスライスの範囲と 90 分の枠を超える（原則 8）。C（HeapBytes の event は数値を比べず、出る位置と resource の並びだけを比べる）は、VM が該当の event を 1 件も出さないので一致せず、B と同じ変更が要るため独立した案にならない。
- **想定の変更規模**: `src/` 約 120〜250 行（`src/vm.rs` の状態・入口のリセット・`exec_builtin` の flush と triple 80〜150、tree と共有するための関数の移動 40〜100）。test は約 250〜400 行（比較の実行器 80〜120、経路ごとのケース 150〜250、REPL 50）。触る仕組みは VM の fs host call の監査 event 1 つ、受入基準は 5 項目。§6.7 のとおり §14.1 の上限は当てない。90 分に収まる根拠: tree 側の順序と共有関数がすでにあり、VM 側は同じ呼び出しを同じ位置に置く作業で、facade・journal・sink には触れない。

#### audit-vm-triple-b 実装スライス（`Engine::run_audited` の VM 経路）

> 状態: **未実装**（設計のみ）。下の「設計時点の事実」から決定記録（PR #535）までは最初の設計 PR（main `295ad0c`、todo のまま）の記録で、ready にする設計は後半の「再設計（main `122fdf6`、todo → ready。PR #539）」に書く。本小節は設計時点の記録で、現在の逸脱・未実装範囲は冒頭の逸脱一覧が正本である。

設計時点の事実:

- 依存の audit-vm-triple-a は done（PR #531）。VM は fs 4 builtin の host-call triple と flush 点の `BudgetCharged` を内部 buffer へ積み、crate 内の drain と `host_effects_may_remain` の読み出しを持つ。
- 上の「audit-vm-triple のスライス分割」小節の決定記録 Q1=A（PR #527）は、b を「VM の `Engine::run` の経路のスライス」に依存させると決めた。その経路のスライスは、vm-parity-diff-a の done（PR #528）の後に既知の差分一覧の項目を減らすフェーズのスライスとして別に設計するとされたが、`slices.toml` にまだ無い。既知の差分一覧（`tests/vm_parity.rs` の `KNOWN_DIFFS`）の KD-1「Engine::execute / Engine::run の ExecutionContext に VM の経路が無い」は残っており、`Engine::run` / `Engine::run_audited`（`src/embedding.rs`）は今も backend を見ずに tree で実行する（[組み込みAPI仕様](embedding-api.md)の逸脱一覧、E9 の残り）。
- したがって [ドキュメントガイド](documentation-guide.md)「ready の条件」の 3（`depends_on` がすべて done）を満たせない。b の受入基準は、上の分割小節のとおり経路のスライスが決まるまで採番しない。
- `EngineBuilder::build` は監査 sink と `VmExperimental` の同時設定を `ConfigError::AuditSinkWithExperimentalBackend` で拒否し、その撤去は audit-vm-triple-c の範囲である。このため b の間は、公開 API（`tests/vm_parity.rs` のような integration test）から VM の監査付き実行に届かない。分割小節の決定記録 Q2=A の条件（b の設計 PR が `tests/vm_parity.rs` への監査の軸の追加を受入基準に入れる）と、c で拒否を外す割り方とが食い違う。

この設計 PR で行うこと:

- `slices.toml` に、経路のスライス `vm-engine-run-route`（VM parity フェーズ、`parent = "VM parity"`、todo）を登録し、b の `depends_on` に足す（決定記録 Q1=A の具体化。範囲は「`Engine::run` / `Engine::execute` が `VmExperimental` のとき `ExecutionContext` で VM を回し、KD-1 を消す」。設計は別の設計 PR で行う）。`order` は b より前の 65。
- b の範囲の見込み（経路のスライスの設計後に確定する。受入基準はそのときに採番する）:
  - 実装範囲の見込み: `run_audited` の VM 経路で、tree と同じ journal・sink の規則で `Started`・a の buffer の drain・終了時の `BudgetCharged`・`Terminal`（`host_effects_may_remain` を a の flag から決める）を出す。redaction policy を engine の設定から VM へ注入する。fail-closed（§10.1）と監査失敗の outcome の写像は tree と同じ関数を通す。
  - 範囲外の見込み: backpressure の `Yielded`/`Resumed` と audit drain queue の byte 計上、`ConfigError::AuditSinkWithExperimentalBackend` の撤去（audit-vm-triple-c）。`normalize_for_conformance`。
  - 受入基準の見込み: tree と VM に同じ source・capability・budget を流し、sink が受け取る event 列（`HeapBytes` の `BudgetCharged` を除く。a の決定記録 Q1=A（PR #531）と同じ除外）が一致すること、`Terminal` の `host_effects_may_remain`、監査失敗の fail-closed、非破壊。どこで確かめるか（`tests/vm_parity.rs` か crate 内の比較 test か）は下の判断の候補 Q2 で決める。
- 想定の変更規模（見込み）: `src/` 約 150〜300 行（`src/embedding.rs` の `run_audited_inner` の backend 分岐と VM の drain の配線）、tests 約 200〜400 行。§6.7「大きさ」のとおり §14.1 の上限は当てないが、経路のスライスが入った後に 90 分へ収まるかを b の設計 PR で確かめ直す。

判断の候補（この PR に書いたもの。下の決定記録で選ばれた）:

- **Q1**: 経路のスライスを、この PR で `vm-engine-run-route`（todo）として登録し b の前提にするか。A（推奨）: 登録して b の `depends_on` に足す。B: 登録せず、b を todo のまま PM の次の分割・設計に任せる。
- **Q2**: b の間は公開 API から VM の監査付き実行に届かないことと、決定記録 Q2=A の条件（`tests/vm_parity.rs` への監査の軸を b で足す）の食い違いをどう扱うか。A（推奨）: b は crate 内の比較 test（tree と VM の `run_audited` の内部入口に同じ入力を流し、sink の受け取る列を比べる）で確かめ、`tests/vm_parity.rs` への監査の軸は拒否を外す c の受入基準へ移す。B: b で `ConfigError::AuditSinkWithExperimentalBackend` を外し、backpressure は c まで VM で起きない（queue の上限を VM では見ない）ことを逸脱一覧に書く。C: 検査のための `#[doc(hidden)]` の builder の入口を足して b の integration test から届かせる。

- **決定記録（判断役が選択（PR #535）。Q1=A・Q2=A）**:
  - **Q1（経路のスライスをこの PR で登録するか）= A**: `vm-engine-run-route`（todo、`parent = "VM parity"`、order 65）を登録し、b の `depends_on` に足す。理由: 「audit-vm-triple のスライス分割」小節の決定記録 Q1=A（PR #527）が、b を VM の `Engine::run` の経路のスライスに依存させ、経路を既知の差分一覧の項目を減らすフェーズのスライスとして別に設計すると決めている。前提の vm-parity-diff-a は done で KD-1 も残っているのに、そのスライスが `slices.toml` に無かった。スライスの一覧と状態の正本は `slices.toml` なので、b の前提を正本に見える形にするのが既存の決定と一貫する（[マニフェスト](manifesto.md) 原則 9）。範囲は KD-1 を消す 1 項目に限り、設計は別の設計 PR に残す（[開発ループ手順書](loop-runbook.md) §6.7、原則 8）。採らなかった案: B（登録しない）は、PR #527 で要るとしたスライスが正本に無いまま残り、b が進めない理由が一覧から読めない。
  - **Q2（b の間に公開 API から VM の監査付き実行に届かないことと、PR #527 の Q2=A の条件の食い違い）= A**: b は crate 内の比較 test（tree と VM の `run_audited` の内部入口に同じ入力を流し、sink の受け取る event 列を `HeapBytes` の `BudgetCharged` を除いて比べる）で確かめる。`tests/vm_parity.rs` への監査の軸の追加は、`ConfigError::AuditSinkWithExperimentalBackend` を外す audit-vm-triple-c の受入基準へ移す。理由: 分割小節は拒否を c で外す理由を「b の間に外すと、backpressure の無い VM 監査が使えてしまう」としており、これは原則 5 に沿う。A はこの割り方を保ったまま同じ入力を tree と VM に流して一致を確かめるので、手順書 §14.1 の趣旨は b でも crate 内で満たせる（原則 5・原則 9）。PR #527 の条件の目的は監査の軸の先送りを落とさないことで、その理由は「facade から観測できる監査の差は b で初めて生じる」だった。b の間は build が拒否するので facade から観測できる差はまだ生じず、差が公開される c に軸を移しても目的は保たれる。公開 API も広げない（原則 8）。**条件**: (1) audit-vm-triple-c の設計 PR は `tests/vm_parity.rs` への監査の軸の追加を受入基準に入れる（PR #527 の Q2=A の条件の移し先。分割小節の追記・c の行・`slices.toml` のコメントにも書いた）。(2) b の設計 PR は crate 内の比較 test を受入基準に入れる。採らなかった案: B（b で拒否を外す）は原則 5 に反しうる。同じ設定・入力で tree は `Yielded`、VM は audit queue の上限を見ずに走り切るという差が公開 API に出て、host の制御が VM で効かない経路を公開することになる（原則 1・原則 3）。C（検査のための `#[doc(hidden)]` の builder の入口）は、検査のためだけに公開 API を足す点で原則 8 に反しうる。backpressure の無い VM 監査に届く点で B と同じ差も持ち込む。

##### 再設計（main `122fdf6`、todo → ready。PR #539）

前提の 2 本（audit-vm-triple-a・vm-engine-run-route）が done になったので、上の「b の範囲の見込み」を確定し、受入基準を採番して ready にする。上の見込みと決定記録（PR #535）は書き換えず、ここで具体化する。

設計時点（main `122fdf6`）の事実:

- `Engine::run_audited`（`src/embedding.rs`）は、sink 無しなら `run_guarded` → `run_inner` に入り、context が VM の状態（`ExecutionContext.vm`、`VmContextState`）を持てば `execute_program_vm` で VM を回す（vm-engine-run-route、PR #538）。sink 有りの `run_audited_inner` は、Started の append・ack までは backend を見ないが、その後の `run_audited_driver` は `context.evaluator` の `install_execution_state`・`set_audit_queue`・`set_redaction_policy`・`run_slice` だけを回し、VM の状態を見ない。Terminal の組み立ても `context.evaluator` の `take_pending_budget_resource` と `host_effects_may_remain` を読み、panic の捕捉と prepare の失敗の usage も `context.evaluator.budget_usage()` を読む。
- 配送の `AuditDelivery::flush_events` は `context.evaluator` の `drain_audit_events`・`drain_budget_charges`・`audit_queue_admit`・`audit_queue_release` を、`AuditDelivery::deliver` は sink の `Pending` のときに `context.evaluator` の `record_backpressure_yield`・`checkpoint_cancel`・`checkpoint_deadline`・`record_backpressure_resume` を呼ぶ。VM の context では、これらはどれも実行に使っていない bootstrap の `Evaluator` を指す。
- `Vm`（`src/vm.rs`）は audit-vm-triple-a で、監査 event の buffer・`operation_id` の counter・`host_effects_may_remain` の flag・redaction policy（既定 `RedactionPolicy::default()`）を持ち、`begin_execution`（clock と token を受け取る入口）で 3 つを 0・空へ戻す。`pub(crate)` の `drain_audit_events` と `host_effects_may_remain` はあるが、accumulator に残る `BudgetCharged` を取り出す入口（tree の `Evaluator::drain_budget_charges`）と redaction policy を差し替える入口（tree の `Evaluator::set_redaction_policy`）は無い。ledger（`self.budget`）は `drain_budget_charges` を持つ。
- `Vm::control_stop_to_error` は共有の `budget::control_stop_to_error` を呼ぶだけで、tree の `Evaluator::control_stop_to_error` のように畳む前の細粒度の `BudgetResource` を退避しない。そのため VM の `BudgetExceeded` の Terminal を今の写像で組むと、`terminal_outcome_from` が `ErrorKind` から代表値を復元し、例えば `OutputBytes` の超過が `IoLimit` から `InputCalls` に化ける（§7.2 と違い、tree とも違う）。
- Started の `backend` は `self.config.backend` を写すので、tree は `TreeWalk`、VM は `VmExperimental` になる（§7.1。backend の違いを記録する field で、一致させるものではない）。
- tree の監査付き経路は `audit_slice_fuel` が未設定なら slice fuel を `u64::MAX` にし、`YieldedHostCall`（VM は build で host function を拒否）と audit drain queue の上限の yield（既定の `max_pending_bytes` は 1 MiB）が起きない限り、`run_slice` は terminal まで 1 回で返り、buffer は terminal の後に 1 回だけ配送される。VM は slice の境界を持たない（`run_repl_chunk` は最後まで回す）。
- `EngineBuilder::build` は監査 sink と `VmExperimental` の同時設定を `ConfigError::AuditSinkWithExperimentalBackend` で拒否する（撤去は audit-vm-triple-c）。

割り方: 分けない（1 本）。触るのは `run_audited_inner` の VM 分岐と、それが読む VM の入口（buffer・accumulator・flag・redaction policy・細粒度 resource）で、監査の配線の 1 つの仕組みである。Started の前の処理と journal・ack の検証・Terminal の予約 slot は tree の関数をそのまま通し、VM 側は「実行してから buffer を 1 回で配送する」だけになる。backpressure と slice 境界の yield は c に残すので、90 分に収まる。

- **実装範囲**:
  - **VM 分岐**: `run_audited_inner` で、Started の ack の後、context が VM の状態を持つときは tree の driver の代わりに VM の driver を回す。VM の driver は、`execute_program_vm` と同じ手順（据え直し → pre-cancel → 注入 → 展開・課金 → compile・実行 → 写像 → capability の後始末）で実行し、terminal の後に VM の buffer（host-call triple と flush 点の `BudgetCharged`）→ accumulator に残る `BudgetCharged` の順で配送する（tree の `flush_events` の順。§8 規則3/7/9）。`execute_program_vm` の手順を写さず、同じ関数を分けて呼べる形にしてよい（据え直しと注入の後に redaction policy を差し込めるように）。sink 無しの経路の観測挙動は変えない。
  - **redaction policy の注入**: engine の `redaction_policy`（既定 `RedactionPolicy::default()`、`audit_redaction_policy_for_test` で差し替わる）を、据え直しの後・実行の前に VM へ渡す（tree の `run_audited_driver` と同じ位置）。このために `Vm` に crate 内の setter を足す。
  - **配送の共有**: journal への append、host-call triple の §11 容量の確認（`can_reserve_host_call`）、ack の検証（`ack_ok`）、envelope の組み立ては、tree と同じ `AuditDelivery` の関数を通す。`flush_events`・`deliver` が読む evaluator の部分（buffer・accumulator の drain、audit drain queue の計上、backpressure の記録と checkpoint）を、context の backend に応じて tree の `Evaluator` か `Vm` を読む形にする。VM には audit drain queue が無いので、VM の側の計上と解放は何もしない（queue は c）。VM 専用の journal・ack の写しを作らない（片方だけ直る差を作らないため。原則 5・原則 8）。
  - **sink の `Pending`**: VM の経路で sink が `Pending` を返したら、ticket を cancel し、`AuditFailure::Sink` で fail-closed にする（Started の `Pending` は今の tree と同じく未 ack の扱いで、`Terminal(AuditFailure)` を emergency slot へ append する。host-call event と終了時の `BudgetCharged` と Terminal の `Pending` は `AuditFailed { withheld, failure: Sink }` を返す）。VM は `Yielded`/`Resumed(AuditBackpressure)` を積まない（下の決定記録 Q1=A。c で置き換える）。
  - **Terminal**: `host_effects_may_remain` は VM の flag から、`BudgetExceeded` の resource は VM が退避した細粒度の `BudgetResource` から、`context_committed` と `usage` と error の payload は tree と同じ関数（`terminal_outcome_from`・`outcome_usage`・`audit_error_payload_from`）で決める。このために `Vm` が `BudgetExceeded` の `ControlStop` を error へ写すときに、畳む前の resource を退避し、crate 内で取り出せるようにする（tree の `last_budget_resource`・`take_pending_budget_resource` と同じ意味。読み取ると消費する）。
  - **mid-run の監査失敗**: 配送の途中で sink が `Failed`・不正 ack を返したら、`AuditFailed { withheld, failure }` を返す。`withheld` は tree の `run_audited_driver` が terminal の slice で使う規則と同じ（実行の outcome を `terminal_outcome_from(&outcome, None)` で写す）にする。VM は配送の前に実行を終えているので、language-state の commit / rollback は実行の結果のままである（tree の terminal の slice の後の失敗と同じ。冒頭の逸脱一覧の「Terminal 配送失敗後に rollback しない」の行の範囲）。
  - **usage の読み先**: `run_audited_inner` の panic の捕捉と prepare の失敗で返す `InternalFailure` の usage を、`context.evaluator.budget_usage()` から `context.budget_usage()`（VM の context では VM の usage）に直す。tree の値は変わらない。
  - **crate 内の検査の入口**: 比較 test のために、`#[cfg(test)]` の builder の入口で build の拒否（`AuditSinkWithExperimentalBackend`）を通らずに監査 sink 付きの `VmExperimental` の engine を組めるようにする。`#[cfg(test)]` なので crate の外（`tests/` と公開 API）からは見えない（PR #535 の Q2=A。案 C の `#[doc(hidden)]` の公開入口は採らない）。
- **範囲外**:
  - `ConfigError::AuditSinkWithExperimentalBackend` の撤去、`tests/vm_parity.rs` への監査の軸の追加（audit-vm-triple-c。PR #535 の Q2=A の条件 1）。
  - VM の audit drain queue の byte 計上と、その上限・sink の `Pending` による `Yielded`/`Resumed(AuditBackpressure)`（audit-vm-triple-c）。
  - `audit_slice_fuel` を設定したときの slice 境界の `Yielded`/`Resumed(SliceFuelExhausted)`。VM の経路は `audit_slice_fuel` を見ず、buffer を terminal の後に 1 回で配送する。tree はこの設定で slice ごとに yield するので、設定したときの event 列は tree と一致しない。b の間は build が拒否するので公開 API からは観測できない。c の設計 PR で、拒否を外す前に、VM のトップレベル境界の yield で揃えるか、VM と `audit_slice_fuel` の同時設定を拒否するかを扱う（c の範囲の「トップレベル境界の `Yielded`/`Resumed`」に含める）。
  - fs 4 builtin 以外の host call の triple（audit-host-boundary-scope）。`normalize_for_conformance`（未実装範囲の行のまま）。VM の host function（vm-engine-run-route の決定記録 Q2=A のとおり build で拒否）。
  - 実行の途中の cancel と deadline の位置を tree と揃えること（vm-engine-run-route の範囲外と同じ。受入基準は pre-cancel と、run の開始時点で過ぎている deadline だけを比べる）。
- **変えないもの**: tree の監査付き経路の event 列と outcome（既存の `tests/audit_*.rs` と crate 内の監査の test が期待値を変えずに緑）。sink 無しの `Engine::run`・`run_audited` の tree・VM の観測挙動。`EngineBuilder::build` の公開の挙動（監査 sink と `VmExperimental` の拒否を含む）。`tests/vm_parity.rs` と既知の差分一覧（KD-2〜KD-4）。CLI・alpha facade。
- **done PR で行う逸脱一覧の更新**: 冒頭の逸脱一覧の「VM は host-call triple と flush 点の event を内部 buffer へ積むが」の行を、「VM の監査付き実行（`run_audited`）は crate 内では Started・buffer の配送・Terminal を tree と同じ journal・sink の規則で出すが、build が監査 sink と `VmExperimental` の同時設定を `ConfigError::AuditSinkWithExperimentalBackend` で拒否するため公開 API からは使えない。VM の経路は audit drain queue を数えず、sink の `Pending` を `AuditFailure::Sink` の fail-closed にし、`audit_slice_fuel` を見ない（`Yielded`/`Resumed` は tree-only。audit-vm-triple-c で解消予定）。`HeapBytes` の `BudgetCharged` は tree と一致しない（§15.5）」と書き直す（行は c で消す）。未実装範囲の「VM の audit triple 対応（監査 wiring）」の行は、延期先を audit-vm-triple-c に改める。
- **受入基準**（確かめ方はどれも crate 内の `#[cfg(test)]` の比較 test。PR #535 の Q2=A の条件 2。同じ source・capability・budget・`execution_id`・`FakeClock` を、`TreeWalk` と `VmExperimental` の監査 sink 付きの engine の `run_audited` に流し、sink が受け取った envelope の列と `AuditedOutcome` を比べる。比較から除くのは、`BudgetCharged { resource: HeapBytes }`（残りの順序は保つ。a の決定記録 Q1=A（PR #531））と、Started の `backend`（tree は `TreeWalk`、VM は `VmExperimental` であることを別に assert する）だけである。source は既知の差分一覧の KD-2〜KD-4 の pattern（`for` の反復、REPL の error の入力）に当たらないものにする）:
  - **AVT-AT-06（event 列の一致）**: fs 4 builtin を allow・deny（capability 未 grant と operation 不足）・host error で呼ぶ source、host call の無い source、import する source（link の課金の `BudgetCharged`）で、sink が受け取る envelope（Started・host-call triple・`BudgetCharged`・Terminal の全 field と sequence と順序）が tree と一致する。同じ context で 2 回続けて `run_audited` した 2 回目も、sequence が 0 から、`operation_id` が 0 から振られて一致する。
  - **AVT-AT-07（Terminal）**: `Completed`・`RuntimeError`・`BudgetExceeded`（`ErrorKind` から復元する代表値と原本が違う resource。例: 出力 byte の超過で `OutputBytes`）・`Exited`（書き込みの host call の後の `exit`。`context_committed` と `host_effects_may_remain` が `true`）・pre-cancel の `Cancelled`（Started と Terminal だけ）・run の開始時点で過ぎた deadline の `DeadlineExceeded` で、Terminal の全 field（`outcome`・`error`・`usage`・`import_graph_hash`・`context_committed`・`host_effects_may_remain`）と `AuditedOutcome` が tree と一致する。
  - **AVT-AT-08（fail-closed）**: `ScriptedAuditSink` で、Started の `Failed`（emergency slot の `Terminal(AuditFailure)` と `AuditFailed`、VM は 1 文も実行せず出力も language-state の変化も無い）、host-call event の `Failed`、Terminal の `Failed` の各経路で、sink が受け取る列と `AuditFailed` の `withheld`・`failure` が tree と一致する。VM の経路で sink が `Pending` を返したときは、ticket を cancel して `AuditFailed { failure: AuditFailure::Sink }` を返し、`Yielded`/`Resumed` を出さない（tree とは比べない。下の決定記録 Q1=A）。
  - **AVT-AT-09（redaction policy の注入）**: `audit_redaction_policy_for_test` で既定と違う `policy_id` と policy を渡した engine で、Started の `redaction_policy_id` と host-call event の Path・Body の field が tree と一致する（VM に engine の policy が届く）。
  - **AVT-AT-10（非破壊）**: 公開の `EngineBuilder::build` は今までどおり監査 sink と `VmExperimental` の同時設定を `ConfigError::AuditSinkWithExperimentalBackend` で拒否する。tree の監査付き経路・sink 無しの tree・VM の経路の既存の test（`tests/audit_*.rs`・`tests/vm_parity.rs`・`tests/differential.rs`・`tests/integration.rs`・golden fixture・crate 内の test）が期待値を変えずに緑で、`KNOWN_DIFFS` は変わらない。確かめ方: `cargo test` と `git diff --stat`（`tests/` の既存ファイルを変えない）。
- **想定の変更規模**: `src/` 約 170〜320 行（`src/embedding.rs` の `run_audited_inner` の VM 分岐と VM の driver 60〜110、`flush_events`・`deliver` が読む evaluator の部分を backend で分ける 50〜90、usage の読み先 5〜10、`#[cfg(test)]` の builder の入口 10〜20、`src/vm.rs` の redaction policy の setter・accumulator の drain の入口・細粒度 resource の退避と取り出し 40〜90）。test は約 350〜550 行（比較の実行器と recording sink の組み立て 80〜130、AVT-AT-06 の経路 100〜160、AVT-AT-07 の outcome 80〜120、AVT-AT-08 の fail-closed 60〜100、AVT-AT-09 30〜40）。触る仕組みは監査付き実行の VM 経路 1 つ、受入基準は 5 項目。§6.7 のとおり §14.1 の上限は当てない。90 分に収まる根拠: Started の前の処理・journal・ack・Terminal の組み立ては tree の関数をそのまま使い、VM の実行は vm-engine-run-route の手順を呼ぶだけで、足すのは「VM の buffer を読む」分岐と、VM の入口 3 つ（setter・drain・resource）である。比較の実行器は a の `audit_triple_parity` の source と組み立てを流用できる。

判断の候補（PR #539 に書いたもの。下の決定記録で選ばれた）:

- **Q1**: b の間、VM の監査付き経路で sink が `Pending` を返したときの扱い。A（推奨）: ticket を cancel して `AuditFailure::Sink` で fail-closed にし、`Yielded`/`Resumed` を出さない。c で tree と同じ backpressure に置き換える。B: `Yielded`/`Resumed(AuditBackpressure)` を積まずに ticket の ack を待つ（tree と同じ resend と deadline の確認だけ行う）。C: このスライスで VM に poll_index の counter と `Yielded`/`Resumed(AuditBackpressure)` を足し、tree と同じ event 列にする。

- **決定記録（判断役が選択（PR #539）。Q1=A）**:
  - **Q1（b の間、VM の監査付き経路で sink が `Pending` を返したときの扱い）= A**: ticket を cancel して `AuditFailure::Sink` で fail-closed にし（`AuditFailed` を返す）、`Yielded`/`Resumed` を出さない。c で tree と同じ backpressure に置き換える（上の「実装範囲」の「sink の `Pending`」の箇条と AVT-AT-08 のとおり）。理由: `AuditDelivery::deliver` の `Pending` の分岐（`src/embedding.rs`）は `context.evaluator` の `record_backpressure_yield`・`checkpoint_cancel`・`checkpoint_deadline`・`record_backpressure_resume` を呼ぶので、VM の context では実行に使っていない `Evaluator` に記録することになり、そのままでは使えない。「audit-vm-triple のスライス分割」小節は backpressure（audit drain queue と `Yielded`/`Resumed(AuditBackpressure)`）を c の範囲とし、b を sink が常に `Ack` を返す経路として分けている。b の間は `ConfigError::AuditSinkWithExperimentalBackend` が残るので、この扱いは公開 API から届かない。§10 の目標（`Pending` なら `Yielded(AuditBackpressure)`）との差は成功を装わない失敗の側に倒れ（[マニフェスト](manifesto.md) 原則 6、優先順位の表「構造化された停止・失敗 ＞ best-effort での実行継続」、§10.1 の fail-closed）、done PR で逸脱一覧に書く（原則 9）。変更も 3 案で最も小さい（原則 8）。採らなかった案: B（`Yielded`/`Resumed` を積まずに ack を待つ）は、§10 が `Pending` なら `Yielded(AuditBackpressure)` と定め tree はそれを記録するのに、VM は記録しない列を成功として配送する。同じ sink の応答で backend ごとに event 列が変わり（原則 5）、監査の列が tree と違うまま成功になる（原則 7、§10.2「実行結果は成功だが監査が欠ける状態を作らない」）ので、マニフェストに反しうる。C（VM に poll_index の counter と `Yielded`/`Resumed(AuditBackpressure)` を足す）は、マニフェストには反しないが、分割小節が「VM に yield の境界を足す変更は確かめ方も壊れ方も違う」として c に分けた決定（PR #527）を b へ前倒しする。VM には poll_index の counter も audit drain queue も無く、その設計は c の設計 PR（`audit_slice_fuel` の扱いを含む）でまとめて行う。b の範囲と受入基準が広がり、1 回の実装に収める前提（[開発ループ手順書](loop-runbook.md) §6.7）が崩れる。

#### vm-engine-run-route 実装スライス（`Engine::run` が `VmExperimental` のとき `ExecutionContext` で VM を回す経路）

> 状態: **未実装**（設計のみ）。上の「audit-vm-triple のスライス分割」小節の決定記録 Q1=A（PR #527）と、audit-vm-triple-b の小節の決定記録 Q1=A（PR #535）の具体化。[開発ループ手順書](loop-runbook.md) §6.7 のフェーズのスライスで、既知の差分一覧（`tests/vm_parity.rs` の `KNOWN_DIFFS`）の KD-1 を消す。[組み込みAPI仕様](embedding-api.md)の E9 の残りのうち、run 経路の Engine 統合を行う。本小節は設計時点の記録で、現在の逸脱・未実装範囲は各 spec の逸脱一覧が正本である。

設計時点（main `82ebf03`）の事実:

- 埋め込み API の `ExecutionContext`（`src/embedding.rs`、crate root では `EmbeddingContext`）は評価状態として `Evaluator` だけを持つ。`Engine::run` / `run_for_repl` / `run_audited`（sink 無し）は `run_guarded`（`catch_host_unwind`）→ `run_inner` → `prepare_program`（engine の照合・保持 source の再 parse・`GraphModules::prepare`）→ `execute_program`（`install_execution_state` で budget・clock・token の据え直し、pre-cancel の判定、引数・capability・言語 revision・host function registry の注入）→ `run_transactional`（`begin_execution_from_graph` と `run_slice` の loop、`terminal_from_run_error` で outcome へ写す）の順に進み、`EngineConfig` の `backend` を見ない。`src/embedding.rs` は `Compiler`・`Vm` を参照しない（[組み込みAPI仕様](embedding-api.md)の逸脱一覧の行、loop:drift #352 D-65）。
- crate root の `Engine`（`src/engine.rs`、alpha facade）は `Engine::new()` / `with_limits` で作り、`Backend` の設定を持たない。§13（[組み込みAPI仕様](embedding-api.md)）は `Engine::new()`・`execute(...)`・引数なしの `ExecutionContext::new()` を alpha API とし、Release N で削除するとしている。
- `tests/vm_parity.rs` の `Mode::Context` は、tree をこの alpha facade の `Engine::execute` で流し、VM は `Run::NoRoute` を返す。KD-1 は `axis: Route`・`pattern: "matrix/context_prev/*"` の 1 項目である。VPD-AT-01 の自己検査（`vpd_at_01_compare_reports_each_axis` の `route` のケース）も `run_vm(Mode::Context, …)` の `NoRoute` で Route の軸の差を作っている。VPD-AT-02 の `vpd_at_02_matrix_covers_every_combination` は、`KNOWN_DIFFS` に `Axis::Route`・`matrix/context_prev/*` の項目があることを assert している。
- VM 側に要る部品はある。CLI の VM REPL（`src/main.rs`）は `Compiler` と `Vm::new_repl_with_budget` を 1 つの session で持ち、submission ごとに `begin_repl_submission(config)`・`LinkedScript::expand_for_repl`（展開済み module の表に対する展開）・`charge_link_graph`・`compile_repl_line`・`run_repl_chunk` を呼ぶ。`Vm` は `set_capabilities`・`set_script_args`・`set_language_revision`・`take_pending_exit`・`budget_usage` を持つ。
- `Vm::begin_repl_submission(config)` は `BudgetLedger::begin_repl_submission(config, None, <ledger が持っていた token>)` を呼び、clock を渡さず token も据え直さない。ledger は reserve のたびに `check_cancel`・`check_deadline`（clock があるときだけ）を見るので、clock と token を渡せば VM でも cancel と deadline が効く。
- `Vm::run_repl_chunk` は `run_frames` が `Err` を返すと、kind を問わず frame・global・stack・fuel を入力の開始時点へ戻す。`exit()`（`ErrorKind::ProcessExit`）でも戻す。§10 規則5（[組み込みAPI仕様](embedding-api.md)）は `Exited` を commit とし、tree はそうしている。CLI の VM REPL は `exit` で session を終えるので、今は観測できない。
- `Vm` は host function registry を持たず、登録 host function を呼ぶ経路が無い。`HostFunctionRegistry::is_empty` はある。
- `EngineBuilder::build` は監査 sink と `VmExperimental` の同時設定を `ConfigError::AuditSinkWithExperimentalBackend` で拒否する（撤去は audit-vm-triple-c）。

割り方の理由: 1 本のスライスにする（分けない）。KD-1 は「前の実行で context に残った変数を次の実行で使う」ケースの経路の欠けで、消すには context の継続（VM の状態の持ち越し）・transaction（commit / rollback）・outcome の写像が揃っている必要がある。「単発の run」と「継続」に分けると、前半の後も `matrix/context_prev/*` は経路を持たず KD-1 は消えないので、前半だけでは差分テストで確かめられない。そのかわり、90 分に収めるため次を範囲外へ出す: CLI の VM 経路を Engine API に載せること、`Engine::compile` の backend compile（`CompileDiagnosticCode::Backend`）、VM の host function、監査（audit-vm-triple-b・c）、alpha facade と scheduler の handle 経路。手順は CLI の VM REPL がすでに持つ並び（展開 → 課金 → compile → 実行）を、tree の `install_execution_state` と同じ据え直しの順で `src/embedding.rs` に置く作業で、新しい仕組みを作らない。

- **実装範囲**:
  - **context の状態**: `ExecutionContext::new(engine)` が `engine.config().backend` を見て、tree なら今の `Evaluator`、`VmExperimental` なら VM の状態（`Compiler`（REPL の compile 状態）・REPL 形の `Vm`・展開済み module の表（`ModuleId` → `SourceHash`））を持つ。field は private のままで、公開 API の形は変えない。`clear_user_state` は VM の状態も作り直す。`budget_usage` は VM の usage を返す。`force_yield_at_every_boundary_for_test` は VM の context では何もしない（doc に書く）。
  - **run の VM 経路**: `Engine::run`・`run_for_repl`・`run_audited`（sink 無し）は、context が VM の状態を持つとき、`run_guarded` の中（panic の捕捉・poison・`running` の扱い・engine の照合・保持 source の再 parse は tree と共有）で次を順に行う。順序は tree の `install_execution_state` → `run_transactional` に合わせる。
    1. request の budget・clock・cancellation token を VM の ledger へ据え直す（使用量と peaks を 0 から数え直し、live heap は持ち越す。tree の `rearm_budget_for_repl` と同じ意味）。このために clock と token を受け取る crate 内の `Vm` の入口を足す。CLI が使う `begin_repl_submission(config)` の挙動は変えない。
    2. pre-cancel: token が cancel 済みなら、命令 0 で `Cancelled { usage }` を返す（tree と同じ。language-state を変えない）。
    3. 引数・capability・言語 revision を `Vm` へ注入する。
    4. link が確かめた graph から、context の展開済みの表に対して import を展開し（`expand_from_graph`、CLI の `expand_for_repl` と同じ規則）、root source と新しく展開した module を `charge_link_graph` で課金する。
    5. `compile_repl_line` で compile し、`run_repl_chunk` で実行する。失敗した実行は compiler の状態も実行の開始時点へ戻す（CLI の VM REPL と同じ）。
    6. outcome は tree と同じ写像（`terminal_from_run_error` の kind ごとの分け方）で作る。写像を tree と VM で 1 つの関数にする（usage と pending exit を引数に取る形へ直してよい。tree の観測挙動は変えない）。
    7. 後始末: capability を context に持ち越さない（[組み込みAPI仕様](embedding-api.md) §10 規則7。tree の `clear_capabilities` と同じ）。
  - **transaction**: `Completed`・`Exited` は commit、それ以外の terminal は実行の開始時点へ rollback する（[組み込みAPI仕様](embedding-api.md) §10 規則5）。VM の `run_repl_chunk` を、`ErrorKind::ProcessExit` では rollback せずに「実行済みの文まで」を commit するよう直す（下の決定記録 Q3=A）。VM は top-level の scope で初期化済みの local の数を数えておき、`Exited` のときは frame を top-level の 1 つへ、stack をその数の slot へ、try handler を空へ切り詰め、compiler の top-level locals もその数へ戻す。block の中の local と、`exit` の呼び出し途中の一時値・callee frame は残さない。`exit` までに実行した global・cell の書き換えは残す。これで `exit` の後の `let` は次の実行で未定義の名前になり、関数・block の中の `exit` でも `exit` までに実行した top-level の binding だけが残る（tree と同じ）。CLI の VM REPL は `exit` で session を終えるので、CLI の観測挙動は変わらない。展開済み module の表へ新しい module を足すのも commit したときだけにする（tree の module marker と同じ）。
  - **host function**: `VmExperimental` で、空でない host function registry を設定した build を、`EngineBuilder::build` が新しい `ConfigError::HostFunctionsWithExperimentalBackend` で拒否する（下の決定記録 Q2=A。`AuditSinkWithExperimentalBackend` と同じ形）。
  - **差分テスト**: `tests/vm_parity.rs` の `Mode::Context` を、tree・VM とも埋め込み API（`Engine::builder()` で `TreeWalk` と `VmExperimental`（`allow_experimental_backend(true)`）の engine を作り、`retain_source` 付きの `compile` → `link` → `EmbeddingContext` で `run`）で流すように変える（下の決定記録 Q1=A）。budget と capability は他の流し方と同じ（`config()` と `capture_capabilities`）。outcome は、`Completed` を `Ok` として、それ以外は variant と `ExecutionError` の field を比べる（`Step` の形は実装 PR で決めてよい）。KD-1 を `KNOWN_DIFFS` から消す。VPD-AT-01 の自己検査は、Route の軸の差を合成した `Run::NoRoute` で作る形に直し、Route の軸と `NoRoute` は残す（後のスライスで経路の欠けを一覧へ載せられるようにするため）。`vpd_at_02_matrix_covers_every_combination` の KD-1 の存在の assert は外す（`matrix/context_prev/*` の Route の項目が一覧に無いことの検査へ直してよい）。
  - **context のケース**: 名前 `context/<名前>` のケースを足し、tree と VM で全軸を比べる: 2 回目の実行の `RuntimeError` の後に 3 回目で値が戻っていること、2 回目の `exit(3)` が `Exited { code: 3 }` で、3 回目でその実行の binding が残っていること、fuel の上限を小さくした実行の `BudgetExceeded` と rollback、pre-cancel の `Cancelled`（usage を含む）、`FakeClock` を request の作成後に deadline へ進めてから run した `DeadlineExceeded`、1 回目に stdout を grant して 2 回目に empty の capability で run した実行の `print` の拒否（capability を持ち越さない）、`ExecutionRequest::with_arguments` の引数を `args()` で読む実行、同じ module を import する実行を 2 回続けたときの 2 回目の import の課金と値（resolver は vm-parity-diff-b の `linked/` と同じ組み立て）。
- **既知の差分一覧**: KD-1 を消す。新しい経路で出た差は、原因がこのスライスの配線（据え直し・展開・写像・commit / rollback）にあれば、このスライスで直す。原因が VM の中の既存の差で、今ある項目と同じ原因なら、その項目の pattern で受ける（KD-2・KD-3 の `matrix/*/for_*` は `context_prev` にも当たる）。それ以外は `KD-5` 以降として足し、PR 本文に一覧（ID・軸・説明・件数）を書く（vm-parity-diff-b と同じ）。KD-2〜KD-4 の中身は変えない。
- **変えないもの**: alpha facade（`src/engine.rs` の `Engine` / `ExecutionContext`、scheduler の handle 経路）。CLI（`src/main.rs`）の tree・VM の経路と観測挙動。tree の観測挙動。既存の test の期待値（`tests/integration.rs`・`tests/differential.rs`・golden fixture・`tests/vm_parity.rs` の他のケースと KD-2〜KD-4）。
- **範囲外**:
  - CLI の VM 経路（file/stdin・REPL）を Engine API に載せること（[組み込みAPI仕様](embedding-api.md)の逸脱一覧の CLI の VM 経路の行。E9 の残り）。
  - `Engine::compile` の backend compile と `CompileDiagnosticCode::Backend`（同じ逸脱一覧の行の compile 側、loop:drift #429 D-132）。このスライスでは VM の compile error は run の中で起き、tree と同じ写像で `RuntimeError` になる。
  - VM で host function を呼ぶこと（決定記録 Q2=A のとおり build で拒否する）。cooperative host function の `YieldedHostCall` の待ちも VM には無い。
  - `Engine::run_audited` の VM 経路と監査 sink（audit-vm-triple-b・c。build の拒否はそのまま）。
  - alpha facade の `Engine::execute` / `create_execution` の VM 経路（決定記録 Q1=A）。VM の slice yield・pause/resume（冒頭の未実装範囲の行のまま）。
  - 実行の途中の cancel の位置と、ledger の reserve 以外の確認点での deadline の位置を tree と揃えること。このスライスの受入基準は pre-cancel と、run の開始時点で過ぎている deadline だけを比べる。
- **done PR で行う逸脱一覧の更新**:
  - 本書冒頭の未実装範囲の「VM 適合 gate のうち audit 以外の残り」の行: 既知の差分一覧の項目から KD-1 を消し、残りの項目（KD-2〜KD-4 と、このスライスで足した項目）に書き直す。
  - [組み込みAPI仕様](embedding-api.md)の逸脱一覧の「`Engine::run` は `EngineConfig` の `Backend` を見ず」の行: run 側の記述を消し、compile 側（`Engine::compile` が backend compile をせず `CompileDiagnosticCode::Backend` を返さない、D-132）と EMB-AT-17 の VM 側（CLI）だけを残す。`VmExperimental` で空でない host function registry を build で `ConfigError::HostFunctionsWithExperimentalBackend` により拒否する行を、延期先（E9 の残り。VM に host function が入るときに拒否を外す）つきで足す（決定記録 Q2=A の条件）。
- **受入基準**:
  - **VRR-AT-01（経路）**: `VmExperimental` の engine の `ExecutionContext` は VM の状態を持ち、`Engine::run`・`run_for_repl`・`run_audited`（sink 無し）は VM で実行する。`TreeWalk` の engine は今までどおり tree で実行する。確かめ方: crate 内の `#[cfg(test)]` の test（context の状態の種類と、run の後の VM の usage）。
  - **VRR-AT-02（KD-1 の消去）**: `tests/vm_parity.rs` の `matrix/context_prev/*` を tree と VM の両方の `Engine::run` で流して全軸で比べ、KD-1 が `KNOWN_DIFFS` から消え、gate（一覧に無い差・差の出ない項目の検査）が通る。確かめ方: test と、PR 本文の差分一覧（消した項目・足した項目）。
  - **VRR-AT-03（継続と transaction）**: 上の「context のケース」のうち `RuntimeError`・`Exited`・`BudgetExceeded` の各ケースと、`exit` の commit の境界のケース（`let a = 1` / `exit(3)` / `let b = 2` の次の実行で `a` を読むものと `b` を読むもの、関数の中の `exit`、block の中の `exit`。決定記録 Q3=A の条件 1）で、outcome（variant・`ExecutionError` の field・`Exited` の code）、出力、usage、次の実行で見える値（commit / rollback の結果、未定義の名前の error を含む）が tree と VM で全軸一致する。`Exited` の実行で `exit` までに実行した top-level の binding だけが次の実行に残る。確かめ方: `tests/vm_parity.rs` の `context/` のケース。
  - **VRR-AT-04（制御と注入）**: pre-cancel の `Cancelled`、run の開始時点で過ぎた deadline の `DeadlineExceeded`、capability を持ち越さないこと、引数、2 回続けた import の各ケースで、outcome・出力・usage が tree と VM で一致する。確かめ方: `tests/vm_parity.rs` の `context/` のケース。
  - **VRR-AT-05（host function の拒否）**: `VmExperimental` で空でない registry を設定した build が `ConfigError::HostFunctionsWithExperimentalBackend` を返し、空の registry の `VmExperimental` と、registry を設定した `TreeWalk` の build は今までどおり成功する。確かめ方: test。
  - **VRR-AT-06（非破壊）**: alpha facade・CLI・tree の観測挙動を変えない。既存の test が期待値を変えずに緑で、KD-2〜KD-4 は変わらない。例外は `tests/vm_parity.rs` の、KD-1 と `Mode::Context` の経路なしを前提にした 2 か所（VPD-AT-01 の自己検査の `route` のケースと、`vpd_at_02_matrix_covers_every_combination` の KD-1 の存在の assert）だけで、上の「差分テスト」のとおり直す。確かめ方: `cargo test` と `git diff --stat`（`src/engine.rs`・`src/main.rs` を触らない）。
- **想定の変更規模**: `src/` 約 280〜510 行（`src/embedding.rs` の context の状態と VM 経路 180〜300、`src/vm.rs` の clock・token を受け取る入口と capability の後始末 20〜40、`exit` の「実行済みの文まで」の commit（初期化済みの top-level local の数の記録、frame・stack・try handler の切り詰め、compiler の top-level locals を戻す入口。決定記録 Q3=A）40〜90、outcome の写像の共有 20〜40、`ConfigError` の variant と build の検査 15〜30）。test は約 330〜550 行（`tests/vm_parity.rs` の `Mode::Context` の付け替えと自己検査 80〜150、`context/` のケース 180〜300（`exit` の境界の 4 ケースを含む）、crate 内の経路と build の test 50〜100）。触る仕組みは埋め込み API の run 経路の backend 分岐 1 つ、受入基準は 6 項目。§6.7 のとおり §14.1 の上限は当てない。90 分に収まる根拠: VM 側の手順は CLI の VM REPL に、据え直しの順序は tree の `install_execution_state` に、どちらも既にあり、組み合わせて置く作業である。`exit` の切り詰めは、`run_repl_chunk` が既に持つ checkpoint（frame・stack の長さ・try handler）と同じ場所で、巻き戻しの代わりに「初期化済みの数」まで切る分岐を足すもので、compiler 側は top-level locals の数を戻す入口 1 つで済む。新しい仕組みではなく既存の transaction の分岐なので、追加の 40〜90 行を含めても 90 分に収まる見込みである。

判断の候補（この PR に書いたもの。下の決定記録で選ばれた）:

- **Q1**: KD-1 の「`Engine::execute`」の側、つまり `Backend` を持たない alpha facade（crate root の `Engine`）に VM の経路を作るか。A（推奨）: 作らない。alpha facade は tree のままにし、差分テストの `Mode::Context` を tree・VM とも埋め込み API の `Engine::run` で流す形へ付け替えて KD-1 を消す。B: alpha facade に backend を選ぶ入口（例: `Engine::with_backend`）を足し、`Engine::execute` でも VM を回す。C: tree は alpha facade の `Engine::execute`、VM は埋め込み API の `Engine::run` で流して比べる。
- **Q2**: `VmExperimental` の engine に登録 host function があるときの扱い。A（推奨）: `EngineBuilder::build` が `ConfigError::HostFunctionsWithExperimentalBackend` で拒否する。B: 拒否せず、VM で host function の名前を呼ぶと未定義の名前の `RuntimeError` になる差を逸脱一覧に書く。C: このスライスで VM に host function の呼び出しを足す。

- **決定記録（判断役が選択（PR #537）。Q1=A・Q2=A・Q3=A）**:
  - **Q1（alpha facade に VM の経路を作るか）= A**: 作らない。alpha facade は tree のままにし、`tests/vm_parity.rs` の `Mode::Context` を tree・VM とも埋め込み API の `Engine::run` で流す形へ付け替えて KD-1 を消す。理由: backend を選ぶ場所は[組み込みAPI仕様](embedding-api.md) §3 の目標では `EngineConfig.backend`（埋め込み API の builder）だけで、alpha facade の `Engine::new()`・`execute(...)` は同 §13 で Release N に削除する alpha API である（main `82ebf03` の `src/engine.rs` に `Backend`・`Vm` の参照は無い）。そこへ入口を足さずに KD-1 を消せるので公開 API を広げない（[マニフェスト](manifesto.md) 原則 8、優先順位の表「小さく監査可能な中核」）。tree と VM を同じ `Engine::run`・同じ budget・capability で流すので、出た差を backend の差として読める（原則 5・原則 9）。alpha facade には backend の選択が無いので、tree のままでも「同じ設定で backend ごとに結果が変わる」経路は生じない。採らなかった案: B（alpha facade に backend の入口を足す）は削除予定の API に公開の入口を足し、scheduler の handle 経路まで VM に載せることになり原則 8 に反しうる。範囲も 1 本のスライスに収まらない（[開発ループ手順書](loop-runbook.md) §6.7）。C（tree は alpha facade、VM は `Engine::run` で比べる）は別の API どうしの比較になり、budget の既定・transaction・outcome の写像の差と backend の差を切り分けられず、原則 5・原則 9 に反しうる。
  - **Q2（`VmExperimental` の engine に登録 host function があるときの扱い）= A**: `EngineBuilder::build` が `ConfigError::HostFunctionsWithExperimentalBackend` で拒否する。理由: `Vm` に host function を呼ぶ経路が無い（`src/vm.rs` に registry の参照が無い）以上、登録された host function が VM で黙って消える設定は、実行前に構造化されたエラーで止めるのが原則 5（backend ごとに結果を変えない）・原則 6（失敗を制御可能な結果にする）と優先順位の表「構造化された停止・失敗 ＞ best-effort での実行継続」に沿う。既存の `ConfigError::AuditSinkWithExperimentalBackend`（PR #535 の Q2）と同じ形で一貫し、VM に host function が入るときに外せる。公開 enum に variant が 1 つ増えるが、alpha 期間の実験 backend に限った拒否で、逸脱一覧に延期先と合わせて書くので原則 8 に反しない。**条件**: 実装 PR は[組み込みAPI仕様](embedding-api.md)の逸脱一覧にこの拒否の行（延期先は E9 の残り）を足す（上の「done PR で行う逸脱一覧の更新」のとおり）。採らなかった案: B（拒否せず未定義の名前の `RuntimeError` の差を逸脱一覧に書く）は、同じ設定・入力で tree は host function を呼び VM は `RuntimeError` になる差が公開 API に出るので原則 5 に反しうる。C（このスライスで VM に host function を足す）は VM の builtin 経路と `YieldedHostCall` の待ちに触れ、KD-1 を消す 1 項目のスライスの範囲を超える（手順書 §6.7、原則 8）。host function の VM 対応は別に設計する。
  - **Q3（VM の `Exited` の commit で、`exit` の後の `let` の compiler local と、関数・block の中の `exit` の frame・stack・block local をどこまで残すか）= A**: 「実行済みの文まで」を commit する（上の「transaction」の箇条の切り詰め）。理由: [組み込みAPI仕様](embedding-api.md) §10 規則5 は `Completed` / `Exited` だけが language-state を commit すると定め、本書 §2.1 は tree を規範 backend とし、backend は結果差を許す determinism input ではないとしている。tree は `exit` の後の文を実行しないので、commit されるのは `exit` までに実行した binding だけである。VM をこれに揃えれば、このスライスが `Engine::run` の VM 経路を公開した時点で、同じ入力・設定から次の実行の結果が backend で分かれない（[マニフェスト](manifesto.md) 原則 5、優先順位の表「一貫した意味論 ＞ backend 固有の最適化」）。レビューの反例（`let a = 1` / `exit(3)` / `let b = 2` の後に `b` を読む）は main `82ebf03` の `src/vm.rs` の `run_repl_chunk`（frame・global・handler の checkpoint と一括の巻き戻し）で確かめた。commit の境界を spec に書き受入基準で確かめられる形にすることは原則 9 にも沿う。**条件**: (1) VRR-AT-03 に「`exit` の後の `let` を次の実行で参照する」「関数の中の `exit`」「block の中の `exit`」のケースを足し、tree と VM で全軸を比べる。(2) 想定の変更規模をこの切り詰めの分だけ見直す（下の「想定の変更規模」に反映した）。採らなかった案: B（このスライスでは VM の `Exited` を rollback のままにし、差を一覧に載せて別スライスへ送る）は、VM の経路を公開しながら `Exited` の commit / rollback が backend で分かれ、§10 規則5 と違う動きが公開 API から観測できるので原則 5 に反しうる。スライスは小さくなるが（原則 8）、優先順位の表では一貫した意味論が先である。C（宣言済みの local をすべて残し、実行しなかった `let` の slot を `null` で埋める）は、`exit` の後の `let` の名前が tree では未定義・VM では `null` になり、名前の解決という言語挙動が backend で分かれるので原則 5 に反しうる。実行していない文の binding を commit する点で §10 規則5 の趣旨とも合わない。

## 15. 受入基準

Phase 5/6実装完了には次をすべて自動化する。

### 15.1 決定性

- same DeterminismInputを100回実行し、value/outcome、context、effect log、budget、semantic audit eventが一致する
- fresh contextだけでなく、binding/loaded module/FunctionId counterが異なる`ContextSnapshot`を復元し、同snapshotでは一致・1 field差では規定差を検出する
- 同じpoll scheduleではyield/resume/poll indexまで一致する。sliceだけを変えるmetamorphic testは、fake monotonic tick固定・deadline未到達・pauseなし・host completion checkpoint固定のprofileでterminal/context/effect/budget totalだけを比較する
- OS locale、timezone、current directory、environment、worker数を変えてもFakeHost実行結果が変わらない
- source hashはLF/CRLF、BOM、Unicode normalization差を区別し、同じbyte列では一致する
- module map挿入順を変えてもimport graph hashと実行結果が変わらない
- Dict iteration、nested serialization、path sortがBTreeMap/normalized byte順になる
- NaN payloadを変えて入力してもcanonical output/error/audit fingerprintになる
- slice fuel、worker割当、pause位置を変えても、control eventを同じsemantic checkpointへ正規化した場合のterminal結果とside effect順が一致する

### 15.2 fake host・record/replay

- clock/env/args/input/output/module/host functionがambient OSへアクセスしないことをdeny adapterで検証する
- recordした全fixtureをnetwork/filesystemなしでreplayし、terminal/context/effect/audit semantic streamが一致する
- RecordStoreのprepare/finish各失敗、idempotent retry、prepareだけ残るcrash recovery、Cancelled/Detached/Unknownを検証し、外部効果前後のfailure channelが規定どおりになる
- request順、引数、call ID、entry countを1つ変えると必ずReplayMismatchになる
- replayがcapabilityまたはbudgetをbypassしない
- record/audit store失敗時、外部効果開始前にfail-closedになる

### 15.3 audit完全性

- 全terminal pathでStartedがsequence 0に1件、Terminalが最後に1件だけある
- sequenceが0からgap・重複なく、再送してもsinkのlogical recordが重複しない
- 全host callにStarted/Finishedのpairがあり、call IDが一致する。deny、cancel、detach、host errorもpairを閉じる
- audit残量がHostCallStartedだけには足りるがlifecycle 3 eventには足りない境界で、Started/外部効果を0件のままAuditFailureになり、予約後のsink failureではclose reserveからDecision/Finishedを出す
- capability allow/denyが外部効果前にackされ、HostCallFinishedがscriptへの結果公開前にackされる
- BudgetChargedの合計とTerminal usageがcumulative/live resourceで一致し、SingleString/SingleSource/CollectionElementsはTerminal `BudgetPeaks`と実測最大candidateが一致する
- YieldedとResumedが交互に対応し、pause、host pending、audit backpressureを区別できる
- sink permanent error、protocol error、永続Pending、event/byte上限でscriptが成功扱いにならずAuditFailure terminalになる
- max_events/max_encoded_bytesを通常eventで使い切っても、予約済みevent 1件・64 KiBからTerminalを必ずappendできる
- Pending ticketのwake、spurious wake、ack/error一度だけ、cancel/drop後のorphan queue引継ぎでeventを欠落・重複しない
- sink callbackのEngine再入を拒否し、deadlockしない

### 15.4 redaction

- path、source、input/output body、host args/result、env value、credentialへ異なるcanary secretを入れ、audit encoded byteとsink diagnosticに平文が1 byteも現れない
- `AuditField`全variant/classをcanonical CBORでround-tripし、original byte count、HMAC digest、Omitted、Body allow-listをschemaどおり保持する
- deny/error/backtrace相当の経路でもraw path・body・secretが漏れない
- HMAC keyが違えばPath hashが変わり、同じkey/normalized pathなら一致する
- Secret型がDebug/Display/serializeできないcompile-fail testを持つ
- Body Reveal allow-listとbyte上限がない限り、hostがRevealを要求してもomitする

### 15.5 tree/VM differential

- stdout/stderrではなくFakeHost effect logをbyte単位・順序込みで比較する
- StableError全field、function identity生成履歴、context rollback、fuel/yield、`normalize_for_conformance`後のaudit payloadを完全一致させる
- §3の「terminal時`BudgetUsage`のbackend一致」に従い、charge traceの突合はREPL持続closureがretainするcode（tree=本体AST／VM=prototype chunk）の課金も含める。[実行予算・協調実行仕様](execution-control.md) §5.3のA-1モデル（closure寿命トークンで課金）により、retained closureのcodeがreachableな限り両engineとも課金し続ける（従来の片側だけrelease-時に落ちるunder-chargeが解消する）ことを突合する。突合対象は**課金の有無とrelease寿命が両engineで一致すること**であり、課金額はengineごとのartifact単位（tree=`ast_node`／VM=`bytecode_chunk`）のままで、§5.3「ASTまたはbytecode」どおりcode artifactの`live_heap`/`peak_heap`数値はengine固有であってよい（byte単位の数値一致まで求めるA-2は採らない）
- AUD-019/024/048の専用fixtureと、AUD-022の生成matrix/fuzzで差分0件を確認する
- 差分が1件でもあるbuildではVM production featureを有効化できないCI gateを置く

> **PR-6c 線引き（REV-015 Slice 6）**: この§15.5のうち、**charge/budget parity**（budget usageの
> engine間不変な軸・terminal reason=`TsumugiError`全field・stdout/stderrのtree/VM一致）は
> REV-015 Slice 6 PR-6cで`tests/differential.rs`に自動化済みである。比較は軸別ポリシーを用い、
> engine間不変な軸（I/O counter・string/source/import counts・per-item peak・terminal reasonの
> kind/message）はexact一致を、fuel countとabsolute heap（`live_heap`/`peak_heap`）は向き・符号・
> release寿命の一致を突合する（byte-exact一致は§5.3のA-1どおりbackend間で成立しないため要求しない。
> fuel枯渇のterminal `line`もstep粒度差でengine間に差があり、fuel parityはStepLimit terminalの
> observable一致で担保する）。
> **（2026-10-20 注記）** 両engineとも上限に達するscriptだけを比べていたため、「treeでは成功しVMでは
> step上限」というobservable差を検出できなかった。fuelは[実行予算・協調実行仕様](execution-control.md)
> §14「F-1 実装スライス」でtreeの課金点に揃え、`committed.fuel`のexact一致で突合する。
>
> 一方、**FakeHost effect logのbyte/順序比較・StableError生成履歴・`normalize_for_conformance`後の
> audit payload・yield/fuelの完全一致・AUD-019/024/048 fixture・AUD-022生成matrix/fuzz**は
> Phase 5/6のdifferential gateの管轄で、PR-6cの射程外である（基盤が未実装）。
> **pause/resume・cancel・host pending**はVMがscheduler非経由（Readyフォールバック・`vm.rs`不変）で
> 実行経路を持たないため**VM対象外**（treeは`tests/scheduler_api.rs`で担保）。
>
> **PR-6c-min完了はVMのexperimental卒業ではない。** 卒業gateは本節§16のconformance（differential
> matrix/fuzz差分0件・差分0件CI gate）であり後続フェーズの管轄である。

## 16. Phase完了条件

Phase 5は、treeが唯一のproduction backendとなり、すべてのambient inputがHostBoundaryへ移り、DeterminismInputとbackend非依存規則が自動検証され、VMのproduction化に差分0件gateが設定された時点で完了とする。

Phase 6は、schema version 2（§7.1）の全event、redaction、bounded journal、fail-closed sink、host call correlation、budget集約、Started/Terminal完全性、record/replay連携が全terminal pathで検証された時点で完了とする。

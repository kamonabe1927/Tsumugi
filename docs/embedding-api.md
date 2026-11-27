# Tsumugi — 組み込みAPI仕様

最終更新: 2026-10-31
設計ステータス: **実装仕様確定・実装進行中**（E1〜E6・E7〔tree REPL の Engine API 統合を含む〕・E8a・E11 実装済み。E9〔VM 経路統合、Phase 5〕のうち **VM file/stdin の import・resolver 統合（link 層 capability gate）は実装済み**で、残りの adapter/conformance（VM の charge parity・StableError・FunctionId・transaction 等）と experimental flag 除去は未実装＝**E9 は未完了**。E8b の capability profile/options 配線（`--profile safe|legacy` と全 capability option、safe/legacy の frozen `CapabilitySet` 構築・注入）は C9/C10/E7/E11 で統合済み、migration warning は §14.4 の N-1 スキップ採択により正式に descope 済み（不実装）、EMB-AT-10/11 は profile 経由 subprocess test で固定済み（下記実装 slice 表・第16節 Phase 2 の受入 ID ↔ test 対応を参照）＝**E8b 完了**。E10 は未実装。詳細は [ロードマップ](roadmap.md) Phase 1/2 を参照）

本文書の §1〜§13・§15 は **到達目標の確定仕様**であり、現在の実装状態ではない。現在の実装が目標仕様からどう
逸脱しているかは、下の **逸脱一覧** が唯一の正本である（[ドキュメント運用ガイド](documentation-guide.md)
「逸脱一覧の持ち方」）。表は現在の逸脱だけを持ち、解消した行は削除する。実装スライスの進捗の正本は
[`slices.toml`](slices.toml)。

逸脱一覧は loop:drift #282 の D-11〜D-13 を受けて新設した。本文（目標）を実装に合わせて後退させず、実装との差を
逸脱として載せる案を選んだ。`ExecutionOutcome` の variant の数（D-11）は loop:drift #263 の D-8 の Q1=A と D-2 の Q1=A の
適用、`StartError` の variant の数（D-12）は #282 の Q1=A、`ExecutionOutcome` の field の形（D-13）は #282 の Q2=A で、
いずれも返信なしのため推奨案で確定した（[開発ループ手順書](loop-runbook.md) §7.5）。理由: spec が実装の正本であり、
現在の逸脱は事実を確かめて逸脱一覧に置く（ドキュメント運用ガイド「逸脱一覧の持ち方」）。§8 の `ExecutionOutcome` は
`#[non_exhaustive]` で後続 Phase に variant を足す前提であり、`Completed.value` は top-level 返値を足しても variant を
変えないための受け皿である。§9 の `StartError` の開始前の mismatch・poison・内部失敗の channel は §10 規則1・§11 規則2 が
前提にしている。本文を実装に合わせると、これらの受け皿が spec から消える。載せたのは 2026-10-28 の整合点検で
§3〜§12 の公開型を実装と照合して確かめた差のうち、上の 3 項目だけである。照合していない節（§13 以降）の逸脱と、
上の設計ステータス段落・§14「実装slice」表にある現在の状態（E9・E10 の残りなど）は、まだ表へ移していない。
監査 builder の差（`audit_sink` が任意である点など）は [決定性・実行時監査仕様](determinism-and-audit.md) の逸脱一覧にある。
loop:drift #352 の D-62〜D-65・D-67・D-68 で、§5 の link 時の課金、§9 の foreign engine の拒否と `ExecutionRequest` の Send/Sync、§12 の VM 経路（E9 の残り）、REPL の budget、§3・§8 の `HostErrorCode` の 7 行を足し、§12 と §15 EMB-AT-05 に残っていた実装状態の注記を表へ移した（2026-11-10 の整合点検で事実を確かめた差）。
loop:drift #429 の D-129〜D-135 で、REPL の budget の行（D-129）と Backend の行（D-132）を書き直し、`Engine::run` の監査失敗の写像（D-130）・§3 の `EngineConfig` の field（D-133）・CLI の REPL の Lexer/Parser の直接呼び出し（D-134）の 3 行を足し、§16 に残っていた REPL の現在状態（D-131）を逸脱一覧への参照にした（2026-11-19 の整合点検で事実を確かめた差）。
REPL の budget の行は fix-repl-submission-budget（PR #497）で削除した。§12「REPLのbudget」と §16 に残っていたその行への参照は loop:drift #503 の D-149 で消した。
§6 が正本を [実行予算・協調実行仕様](execution-control.md) §3 に置く `ExecutionRequest` の差（loop:drift #288 の D-15）と、
同仕様 §9.1 の handle API の差（D-16）は、同仕様の逸脱一覧にある。

## 逸脱一覧（目標仕様に対する現在の逸脱）

| 逸脱内容 | 対象の節(§) | 発生スライス |
|---|---|---|
| 公開 `ExecutionOutcome`（`src/embedding.rs` の `ExecutionOutcome`、crate root では `EmbeddingOutcome`）は `Completed` / `Exited` / `RuntimeError` / `BudgetExceeded` / `DeadlineExceeded` / `Cancelled` / `InternalFailure` の 7 variant だけで、目標の 13 variant のうち `Denied` / `LinkError` / `HostError` / `AuditFailure` / `RecordFailure` / `ReplayMismatch` が無い。link 時の拒否と resolver の host 失敗は `Engine::link` の `Result<_, LinkError>`（`LinkError::Denied` / `LinkError::Resolve(HostError)` ほか）で返し、監査失敗は監査付き経路だけの別型 `AuditedOutcome::AuditFailed` で返す。`RecordFailure` / `ReplayMismatch` は record/replay が未実装のため無い（同じ差を [決定性・実行時監査仕様](determinism-and-audit.md) の逸脱一覧が §7.2 の `TerminalOutcome` との対応として持つ） | §8 | E1・E5・E7・E11 |
| `ExecutionOutcome` の field の形が目標と違う。目標の `Completed { value: Value, usage }` に対し、実装（`src/embedding.rs` の `ExecutionOutcome::Completed`）は `Completed { usage }` で `value` が無い。目標の `BudgetExceeded { failure: BudgetExceeded, usage }` に対し、実装は `BudgetExceeded { error: ExecutionError, usage }` で、超過した resource は `ExecutionError::code` の `ErrorKind` で表す（構造化の `BudgetExceeded` 型は `src/budget.rs` にあるが outcome には載せない）。目標の `DeadlineExceeded { deadline: MonotonicInstant, usage }` に対し、実装は `DeadlineExceeded { usage }` で `deadline` が無い | §8 | E5（`Completed`）、E11（`BudgetExceeded`・`DeadlineExceeded`） |
| `StartError`（`src/scheduler.rs` の `StartError`、`src/lib.rs` で crate root に公開）は `Backpressure { active, queued, limit }` / `Config(ConfigError)` / `ConcurrentRunRequiresPolling` の 3 variant だけで、目標の `EngineMismatch` / `RevisionMismatch` / `ContextBusy` / `ContextPoisoned` / `InvalidRequest` / `AuditUnavailable` / `InternalFailure` が無く、目標に無い `Config(ConfigError)` を持つ。`ConcurrentRunRequiresPolling` は variant だけで、検出は配線していない | §9 | exec-control Slice 5 |
| `Engine::run` に別 engine の `ExecutionContext` か `LinkedScript` を渡すと、開始前の mismatch として拒否せず `ExecutionOutcome::InternalFailure` を返し、context を poison する（`src/embedding.rs` の `run` の engine 照合。unit test `run_rejects_foreign_context` はこの挙動を固定している）。目標は §9・§10 規則1・EMB-AT-03 の開始前の `EngineMismatch` による拒否で、§9 のとおり context を変えず poison しない。同期 `run` の戻り値の形は fix の設計で決める。fix スライス fix-run-foreign-engine-mismatch で直す。loop:drift #352 D-63 | §9・§10・§15 | 既存の挙動（loop:drift #352 D-63 で確認） |
| `ExecutionRequest` が `Send`・`Sync` でない。field の `clock: Arc<dyn MonotonicClock>` の `MonotonicClock`（`src/budget.rs`）に `Send + Sync` の bound が無い（`Engine`・`CompiledScript`・`LinkedScript` は満たす）。EMB-AT-02 の compile-time assertion は `ExecutionRequest` を見ていない。fix スライス fix-execution-request-send-sync で直す。loop:drift #352 D-64 | §9・§15 | 既存の挙動（loop:drift #352 D-64 で確認） |
| `Engine::compile`（`compile_inner`、`src/embedding.rs`）は `EngineConfig` の `Backend` を見ず root source の lex/parse だけを行い、§4・§10 の backend compile をしないため、`CompileDiagnosticCode::Backend` を返さない（`VmExperimental` の engine の VM の compile error は run の中で起き、tree と同じ写像で `RuntimeError` になる）。`Engine::run`・`run_for_repl`・`run_audited`（sink 無し）は vm-engine-run-route で `VmExperimental` の engine の `ExecutionContext` を VM で実行するようになった。VM backend adapter の compile 側と EMB-AT-17 の VM 側（CLI）は未実装で、延期先は E9 の残り。loop:drift #352 D-65、compile 側は loop:drift #429 D-132 | §4・§10・§12・§15 | E9（未実装範囲） |
| `EngineBuilder::build` は、空でない host function registry（`EngineBuilder::host_functions`）を `VmExperimental` の backend と同時に設定した build を `ConfigError::HostFunctionsWithExperimentalBackend` で拒否する（`src/embedding.rs`）。VM には登録 host function を呼ぶ経路が無く、登録した host function が VM で黙って消えないようにするため（determinism-and-audit の vm-engine-run-route 小節の決定記録 Q2=A）。§3・§11 の目標は backend を問わず host function を使える。延期先は E9 の残り（VM に host function が入るときに拒否を外す） | §3・§11 | vm-engine-run-route（E9 の残りへ延期） |
| CLI の VM 経路（file/stdin・REPL。`src/main.rs`）は Engine API を通らず `Compiler` と `Vm` を直接呼び、capability を `Vm::set_capabilities` で注入する。VM file/stdin は import の gate だけのために `Backend::TreeWalk` の engine を build して `compile`→`link` を通し、import 展開は link 済み graph から `expand_for_cli` で行う。VM REPL の import は tree REPL と同じく submission ごとに `link` を通し、`expand_for_repl` で link の graph から展開する（fix-repl-import-gate）。延期先は E9 の残り。loop:drift #352 D-65 | §12・§15 | E9（VM file/stdin の import gate だけを統合した最小統合） |
| audit sink を設定した engine の `Engine::run` は、監査に失敗すると `AuditFailure` でなく `ExecutionOutcome::Cancelled { usage }`（空の usage）を返す（`src/embedding.rs` の `run` の監査付き経路と `AuditedOutcome::into_execution_outcome`）。目標は §8 の `AuditFailure` で、§12 の表の exit code は `AuditFailure` が 70・`Cancelled` が 130 である。最終 `Terminal` の配送失敗では context は commit 済みなのに `Cancelled` になり、§10 規則5（`Completed` / `Exited` 以外は rollback）とも合わない。監査付き経路の `Engine::run_audited` は `AuditedOutcome::AuditFailed` を返す（この表の 1 行目）。fix スライス fix-run-audit-failure-outcome で直す（挙動の直し方はその設計で決める）。loop:drift #429 D-130 | §8・§10・§12 | A-1 |
| `EngineConfig`（`src/embedding.rs`）が目標の `backend`・`language_revision` に加えて pub field の `audit_sink`・`audit_slice_fuel` を持ち、`EngineBuilder::audit_slice_fuel` が `#[doc(hidden)]` なしで公開されている。目標は §3 のとおり `EngineConfig` は 2 field だけで、`audit_sink` は builder に置き、`audit_slice_fuel` は embedder 向けの契約にしない（[実行予算・協調実行仕様](execution-control.md) のテスト専用フックの項は「テスト専用の内部 surface」とする）。fix スライス fix-engine-config-audit-fields で直す。loop:drift #429 D-133 | §3 | A-1（`audit_sink`）、D3（`audit_slice_fuel`） |
| CLI の REPL（`src/main.rs`、tree・VM の両 REPL loop が使う）は、継続入力の判定（`is_incomplete`）で `Lexer` を、EOF の診断（`finish_repl_at_eof`）で `Lexer`・`Parser` を直接呼ぶ。目標は §12 の「parser/compiler/evaluator/VMを直接呼ばない」。fix スライス fix-cli-repl-compile-diagnostics で、EOF の診断を `Engine::compile` の `CompileErrors` から出すようにする（継続判定のトークン走査を許すかはその設計で決める）。loop:drift #429 D-134 | §12 | E7（tree REPL の Engine API 統合） |
| `HostErrorCode::new`（`src/embedding.rs`）は NUL を含まない文字種の違反（`"A"`・`"1a"`）にも `ConfigError::IdentifierContainsNul` を返す。目標は §8 のとおり `ConfigError::InvalidDescriptor { field: "host_error_code", code: "invalid_identifier" }`。fix スライス fix-host-error-code-invalid-descriptor で直す。loop:drift #352 D-68 | §3・§8 | 既存の挙動（loop:drift #352 D-68 で確認） |

## 1. 目的と規範範囲

本文書は、[ロードマップ](roadmap.md) Phase 1/2のembedding API先行実装を、Rust実装者が追加判断せず最終契約へ接続できる型、所有権、error、CLI mappingまで固定する規範仕様である。次期言語挙動とCLIは[次期意味論・実装決定](semantic-decisions.md)、Phase 3/4の公開budget・state・handle・poll・transactionは[実行予算・協調実行仕様](execution-control.md)、Phase 5/6のaudit schemaとfail-closedは[決定性・実行時監査仕様](determinism-and-audit.md)、security前提は[脅威モデル](threat-model.md)、外部効果の認可は[Capability Model仕様](capability-model.md)に従う。

以下のRustは、compile/link/source identity等のPhase 1/2公開APIと、後続正本から参照するterminal型の擬似コードである。Phase 3/4型を本書で再定義せず、先行実装専用型をpublic stable surfaceへ出さない。同じbuildへ旧型と最終型を併存させない。

## 2. Phase境界

| Phase | 本書と後続正本の境界 |
|---|---|
| 1 | tree規範backend、importなしrootのcompile/link、source identity、構造化terminal channel、runtime/internal error、CLI一本化。execution-controlの最終型を使う内部縦切り |
| 2 | `CapabilitySet`接続、import resolver、`Denied`、`Exited`、host function、safe/legacy profile。capabilityの最終catch規則を使う内部縦切り |
| 3 | [実行予算・協調実行仕様](execution-control.md)の有限`BudgetConfig` / `BudgetUsage`、deadline、実行中cancellation、`create_execution`を完全実装 |
| 4 | 同文書の`ExecutionState`、`ExecutionHandle`、`PollSlice` / `PollResult`、yield/pause、cooperative adapterを完全実装 |
| 5/7 | VM conformance完了後のstable化 |
| 6 | [決定性・実行時監査仕様](determinism-and-audit.md)のaudit sink、canonical event列、fail-closed、usage監査を実装 |

Phase 1/2の内部縦切りでも`ExecutionRequest`は有限`BudgetConfig`を必須とする。meter未接続部分を内部bootstrapとして段階実装できるが、無制限budget、optional usage、`Suspended`、旧poll/resume等を公開契約にしない。Phase 3/4でbreakingな二重APIを追加せず、最終型を完成させる。

同期`run`はcaller threadをterminal outcomeまで占有するconvenience methodである。Tsumugi coreはworker threadを暗黙生成しない。

## 3. 識別子、設定、構築

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Backend {
    TreeWalk,
    VmExperimental,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum LanguageRevision {
    V0_19,
    V0_20,
    V0_21,
}

impl LanguageRevision {
    pub const CURRENT: Self = Self::V0_21;
    pub const fn as_str(self) -> &'static str; // 例: V0_21 => "0.21"
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct EngineId(u128); // Engineだけが生成

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct SourceId(String);

impl SourceId {
    pub fn new(value: impl Into<String>) -> Result<Self, ConfigError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct SourceHash([u8; 32]);

impl SourceHash {
    pub const fn as_bytes(&self) -> &[u8; 32];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ExecutionId(NonZeroU128);

impl ExecutionId {
    pub const fn new(value: NonZeroU128) -> Self;
    pub const fn get(self) -> NonZeroU128;
}

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub backend: Backend,
    pub language_revision: LanguageRevision,
}

impl Default for EngineConfig {
    fn default() -> Self; // TreeWalk + CURRENT
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConfigError {
    EmptyIdentifier { field: &'static str },
    IdentifierTooLong { field: &'static str, max_bytes: u16 },
    IdentifierContainsNul { field: &'static str },
    ExperimentalBackendNotEnabled,
    DuplicateCapability { kind: CapabilityKind },
    DuplicateCallableName { name: String },
    InvalidDescriptor { field: &'static str, code: &'static str },
    InvalidFilesystemPolicy { code: &'static str },
}

pub struct EngineBuilder { /* private */ }

impl EngineBuilder {
    pub fn new() -> Self;
    pub fn config(self, config: EngineConfig) -> Self;
    pub fn allow_experimental_backend(self, allow: bool) -> Self;
    pub fn host_functions(self, registry: HostFunctionRegistry) -> Self;
    pub fn build(self) -> Result<Engine, ConfigError>;
}

pub struct Engine { /* immutable after build */ }

impl Engine {
    pub fn builder() -> EngineBuilder;
    pub fn id(&self) -> EngineId;
    pub fn config(&self) -> &EngineConfig;
}
```

`SourceId::new`は1..=256 UTF-8 bytes、NULなしを受理する。表示用IDであり、hostはsecret pathやcredentialを入れてはならない。`EngineId`はprocess内で一意な非ゼロのu128とし（乱数源を使わない）、永続identityには使わない。この形は loop:drift #352 D-66 の Q1=A（判断役が選択、issue #352）による。理由: engineはambient random sourceへ直接アクセスしない（[決定性・実行時監査仕様](determinism-and-audit.md) §7.1）。本節が求める性質は「process内で衝突しない」「永続identityに使わない」で、乱数でなくても満たせる（[マニフェスト](manifesto.md) 原則5・原則8）。§11 規則2のfault IDも同じである。採らなかった案B（hostが注入する乱数源から生成する）は、本節が求めていない衝突回避のために公開APIとfixスライスを増やし、fault IDが実行ごとに変わって再現性も下がるため採らない。

`VmExperimental`は`allow_experimental_backend(true)`がなければbuild errorである。Phase 1/2内部縦切りではaudit sinkをまだ接続しない。Phase 6統合buildではbuilderへ`audit_sink`を追加し、[決定性・実行時監査仕様](determinism-and-audit.md)どおりsink未指定を`ConfigError::AuditSinkRequired`で拒否し、`AuditFailurePolicy::FailClosed`だけを受理する。同じbuildにsinkなしfallbackやbest-effort policyを併存させない。

`EngineConfig`は上の`backend`・`language_revision`の2 fieldだけを持つ。audit sinkは`EngineConfig`のfieldにせずbuilderだけに置き、監査付き経路の1 sliceあたりのfuel上限（`audit_slice_fuel`）はembedder向けの契約にしない。後者はtestから監査下の実yieldを観測するための内部surfaceで、`audit_budget_for_test`と同じく`#[doc(hidden)]`のtest用の扱いとし、`EngineConfig`の外に置く（`tests/`の統合testが使うので外から呼べる形は保つ。形の細部はfixスライス fix-engine-config-audit-fields の設計で決める）。現在の実装との差は上の逸脱一覧にある。この形は loop:drift #429 D-133 の Q1=A（判断役が選択、issue #429）による。理由: [マニフェスト](manifesto.md) 原則8と優先順位の表「小さく監査可能な中核 ＞ 広範な組み込み機能」に照らし、test用のknobを公開契約に入れない。[実行予算・協調実行仕様](execution-control.md)のテスト専用フックの項は`EngineBuilder::audit_slice_fuel`を「テスト専用の内部 surface」とし、embedder向けの契約として決めた記録は無い。sliceの刻みは原則4の平準化に関わる量で、公開するならexecution-controlのscheduler側の設計として決めるものである。採らなかった案B（§3の目標に2つのfieldを足す）は、test専用のknobを理由なく公開契約に足すので原則8に反しうるうえ、`audit_sink`の置き場所が本節の「builderへ`audit_sink`を追加」と二重になるため採らない。

## 4. Compileとsource identity

```rust
pub struct Source<'a> {
    pub id: SourceId,
    pub text: &'a str,
}

impl<'a> Source<'a> {
    pub fn new(id: SourceId, text: &'a str) -> Self;
}

#[derive(Clone, Debug)]
pub struct CompileOptions {
    pub retain_source: bool,
}

impl Default for CompileOptions {
    fn default() -> Self; // retain_source=false
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CompileDiagnosticCode {
    Lex,
    Parse,
    AstDepth,
    Backend,
    InternalFault,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileDiagnostic {
    pub code: CompileDiagnosticCode,
    pub line: Option<u32>,
    pub column: Option<u32>,
    pub safe_message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileErrors {
    pub diagnostics: Vec<CompileDiagnostic>, // 常に1件以上、source位置順
}

#[derive(Clone)]
pub struct CompiledScript(Arc<CompiledScriptInner>);

impl CompiledScript {
    pub fn engine_id(&self) -> EngineId;
    pub fn source_id(&self) -> &SourceId;
    pub fn source_hash(&self) -> SourceHash;
    pub fn language_revision(&self) -> LanguageRevision;
    pub fn backend(&self) -> Backend;
}

impl Engine {
    pub fn compile(
        &self,
        source: Source<'_>,
        options: &CompileOptions,
    ) -> Result<CompiledScript, CompileErrors>;
}
```

`compile`はroot sourceだけをlex/parse/backend compileする。import先、filesystem、environment、clock、stdio、host functionを呼ばない。`retain_source=false`ではline mapとdiagnosticに必要なtoken位置以外のsource本文を保持しない。

`SourceHash`はsourceのUTF-8 bytesそのものに対するSHA-256で、BOM除去、改行変換、Unicode正規化をしない。

### 4.1 実行用 AST の入手方針（案 A採用、案 B却下の記録）

`CompiledScript` と `LinkedScript` は `Send + Sync` 契約（EMB-AT-02）を持つ一方、実行入口（E3 tree backend adapter）は実行スレッド上で runnable な `Program`（AST）を必要とする。現行 AST は `ast::Block = Rc<[Stmt]>`（REV-015 Slice 3 PR-d の永続 continuation）で `!Send`/`!Sync` のため、この 2 つを両立する方針を確定する必要があった。以下は 2026-09-21 の設計判断の記録である。

**採用（案 A）: 実行時に保持 source を再 parse して `Program` を再構築する。**

- `CompiledScript` は identity・`source_hash`・`has_imports`・任意の保持 source だけを持ち、runnable AST を保持しない。実行入口は保持 source を実行スレッド上で `Lexer`→`Parser` で再 parse して `Program` を得る。parse は決定的なので再構築した AST は `source_hash` と必ず一致し、`Evaluator::begin_execution` が root を `Rc<[Stmt]>` フレームへ写した後は `Program` を drop できる。
- 実行するには `retain_source = true` で compile しておく必要がある（保持していない場合は `InternalFailure`）。これは欠点ではなく、原則2「権限・前提はホストが明示的に付与する（ambient を既定にしない）」に沿う明示契約である。
- 評価器・AST の中核に一切触れないため、変更が実行入口スライスに閉じる（原則8「小さく理解可能な中核」）。tree 評価器のホットパスの性能特性（AUD-038/040 でチューニング済み）も不変。

**却下（案 B）: `ast::Block` を `Rc<[Stmt]>` から `Arc<[Stmt]>` へ変え、AST を `Send + Sync` 化して handle に保持する。**

- 変更点自体は小さい（型 alias 1 + `Rc::from` 呼び出し 5 箇所）。AST は `String`/`Expr`/`Block`/`Vec` だけで `Value`/`Rc` を含まないため、`Arc` 化で `Program`/`CompiledScript` は真に `Send + Sync` になり、再 parse も source 保持も不要になる。
- しかし却下した。理由:
  - 実行そのものは `ExecutionContext` が `!Send + !Sync`（第9.1節の stable 契約）で単一スレッドに固定される。`Send + Sync` が必要なのは handle だけで、**AST を跨スレッド共有する能力は思想（単一スレッド実行）が要求していない**。案 B はその不要な能力を中核へ足す（原則8 に反する方向）。
  - tree 評価器はフレームごとに `Block` を clone するホットパスで、`Rc`（非アトミック）→`Arc`（アトミック参照カウント）はわずかに実行コストを増やす。速度は非目標だが「わざわざ遅くする」のは別で、根拠のない性能変更を避ける原則9・計測負債の観点で不利。
  - 「Send な AST が存在する」こと自体が、将来「実行を複数スレッドへ分ければ速い」という原則1（最悪時負荷の制御）・原則5（backend 差を作らない）に反する最適化への誘因・説明負債として中核に residue を残す。
- 将来性の評価: Phase 5（決定性・record/replay）は source を再現の第一級入力として扱うため、案 A の「実行時に source から再構築」は将来像と同じ方向を向く。逆に案 B の「Send な AST」は、`!Send` 実行という確定契約の下では将来も使い道がない。案 B が正当化されるのは Tsumugi が単一スレッド実行の stable 契約を撤回して真の並行 AST 共有へ舵を切る場合だけで、それはマニフェスト原則1・5・「目指さないもの（最大スループット競争）」に照らして現時点では起きない前提の変更であり、実行入口スライスで先取りする話ではない。

したがって、現状でも将来でも案 A を採る。案 B の再検討は、単一スレッド実行契約そのものを見直す意思決定とセットでのみ行う。

## 5. Link、Module graph、link request

```rust
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ModuleId(String);

impl ModuleId {
    pub fn new(value: impl Into<String>) -> Result<Self, ConfigError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Clone, Debug)]
pub struct ImportNode {
    pub module_id: ModuleId,
    pub source_hash: SourceHash,
    pub imports: Vec<ModuleId>, // source内の出現順
}

#[derive(Clone, Debug)]
pub struct ImportGraph {
    pub root: SourceHash,
    pub root_imports: Vec<ModuleId>, // root source内の出現順
    pub nodes: Vec<ImportNode>, // ModuleId UTF-8 byte列の昇順
    pub graph_hash: SourceHash,
}

#[derive(Clone)]
pub struct LinkedScript(Arc<LinkedScriptInner>);

impl LinkedScript {
    pub fn root(&self) -> &CompiledScript;
    pub fn import_graph(&self) -> &ImportGraph;
    pub fn script_hash(&self) -> SourceHash;
}

#[derive(Clone)]
pub struct LinkRequest {
    pub operation_id: ExecutionId,
    pub capabilities: CapabilitySet,
    pub budget: BudgetConfig,
    pub cancellation: CancellationToken,
}

impl LinkRequest {
    pub fn new(
        operation_id: ExecutionId,
        capabilities: CapabilitySet,
        budget: BudgetConfig,
    ) -> Self;

    // cancellation注入の補助builder（additive）。既定は never-cancel token。
    pub fn with_cancellation(self, token: CancellationToken) -> Self;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkError {
    EngineMismatch,
    RevisionMismatch,
    BackendMismatch,
    FeatureUnavailable { feature: &'static str },
    Denied(Denial),
    Resolve(HostError),
    InvalidModule { module: ModuleId, diagnostics: CompileErrors },
    Cycle { chain: Vec<ModuleId> },
    DepthExceeded { limit: u32 },
    BudgetExceeded(BudgetExceeded),
    DeadlineExceeded,
    Cancelled,
    Backend(ExecutionError),
    InternalFailure { fault_id: u128, safe_message: String },
}

impl Engine {
    pub fn link(
        &self,
        script: &CompiledScript,
        request: LinkRequest,
    ) -> Result<LinkedScript, LinkError>;
}
```

`ModuleId::new`は1..=1024 UTF-8 bytes、NULなしを受理する。resolverはcredential、query secret、絶対local pathをIDへ含めてはならない。

`link`はEngine ID、language revision、backendの順に検証し、不一致なら対応する`LinkError`を返してresolverを呼ばない。Phase 2では全importを実行前にresolver capabilityで解決し、**循環・深度・module単体compile・hash**を検証する。static cross-module callable symbol/arityの検証はPhase 2のlink検証に含めず、export/symbolモデルが入る後続Phase（exportモデル導入スライス）へ移す。未解決のcross-module symbolはlinkで静的に落とさず、run時に`RuntimeError`（name error等）へ落ちる。resolver capabilityなしでimportがあればresolver call 0で terminal `Denied`とする。`cancellation`は`with_cancellation`で注入し、link開始前のcancelはresolver call 0で`Cancelled`。run中のdynamic importは禁止する。`LinkedScript`はlinkがresolverから読んだmodule sourceを内部に保持し、runはそのsourceからimportを展開する。runはresolver・filesystemからmoduleを読み直さない（[Capability Model仕様](capability-model.md) §10。保持は内部であり、本節の公開型のfieldは増やさない）。同じ`ExecutionContext`で、すでに展開したmoduleと同じ`ModuleId`・違う`source_hash`のmoduleを含む`LinkedScript`をrunすると、runの`Link`フェーズで1文も実行せずに失敗する（後勝ちにしない。同§10）。import 0件でも空graphを持つ。`FeatureUnavailable`はPhase 2最終形でも列挙に残すが、import解決がEngine APIへ配線された後はimportを理由に返されない（到達しない保持variant）。

`LinkRequest`はresolverへ渡すoperation ID、capability、[実行予算・協調実行仕様](execution-control.md)の有限`BudgetConfig`、cancellationを所有する。deadlineは`BudgetConfig.deadline`を使い、source/module/import count・bytesを同文書の`BudgetUsage`へ課金する。Phase 1/2のmeter実装を内部縦切りにしても、無制限limitや独立optional deadlineをpublic APIへ置かない。`create_execution`からlinkする場合も同じrequest budget/controlを共有し、別の計数系を作らない。

### 5.1 Hashのbyte-level仕様

全整数はunsigned 64-bit big-endian、文字列は`length || UTF-8 bytes`、hashは生32 bytesでencodeする。domain separatorは末尾NULを含むASCII bytesである。

```text
graph = "TSUMUGI-IMPORT-GRAPH-V1\0"
      || str(language_revision.as_str())
      || root_hash
      || u64(root_imports.len)
      || each root import in source order: str(import_module_id)
      || u64(nodes.len)
      || each node sorted by module_id UTF-8 bytes:
           str(module_id) || source_hash || u64(imports.len)
           || each import in source order: str(import_module_id)

graph_hash = SHA-256(graph)

script = "TSUMUGI-LINKED-SCRIPT-V1\0"
       || str(language_revision.as_str())
       || root_hash || graph_hash
script_hash = SHA-256(script)
```

同一`ModuleId`が異なるsource hashで2回解決された場合は`LinkError::Resolve`で拒否し、後勝ちにしない。

## 6. Execution request、budget、context

`BudgetConfig`、`BudgetUsage`、`BudgetExceeded`、`BudgetResource`、`ExecutionRequest`のfieldとconstructorは[実行予算・協調実行仕様](execution-control.md)第3節を唯一の正本とする。`ExecutionRequest`は`execution_id`、frozen `CapabilitySet`、script引数snapshot、有限`BudgetConfig`、`CancellationToken`を所有する。独立したoptional deadline、無制限sentinel、未計測fieldを`Option`で表すusage型を定義しない。

```rust
pub struct ExecutionContext { /* private, !Send, !Sync */ }

impl ExecutionContext {
    pub fn new(engine: &Engine) -> Self;
    pub fn engine_id(&self) -> EngineId;
    pub fn clear_user_state(&mut self) -> Result<(), ContextError>;
    pub fn is_poisoned(&self) -> bool;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContextError {
    Busy,
    Poisoned,
}
```

`ExecutionRequest.arguments`はbinary名、script path、CLI flagを含まないscript引数のUTF-8 snapshotである。run中にprocess argvを再読しない。全executionは作成時から有限budgetを持ち、使用量は全fieldが明示値を持つ`BudgetUsage`で返す。Phase 1/2の内部縦切りで未接続meterがあっても、optional usageや無制限APIを公開せず、Phase 3完了までproduction保証を主張しない。

## 7. Cancellation

```rust
#[derive(Clone, Debug)]
pub struct CancellationToken(Arc<CancellationState>); // Send + Sync

impl CancellationToken {
    pub fn new() -> Self;
    pub fn cancel(&self) -> bool; // 最初のfalse→trueだけtrue
    pub fn is_cancelled(&self) -> bool;
}

impl Default for CancellationToken {
    fn default() -> Self; // new()
}
```

`cancel()`はrequestでありthreadをkillしない。link開始前のcancelはresolver call 0で`LinkError::Cancelled`。execution作成後の確認点とraceは[実行予算・協調実行仕様](execution-control.md)第8節に従い、最初の`poll`でscript命令、callback、OS操作より前に確認する。loop中、bulk chunk間、adapter待機、host call前後、commit前でも確認し、terminal `Cancelled`はscript catch不可で全language-stateをrollbackする。同tokenを複数requestへ渡した場合、一度のcancelで全該当requestがcancel対象になる。

## 8. Runtime errorとterminal outcome

runtime errorのkind、canonical message、line、trace、評価順、catch可否は[次期意味論・実装決定](semantic-decisions.md)第3節の`ErrorKind` inventoryを唯一の正本とする。backendごとの縮小enumやmessage推測を作らない。

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceFrame {
    pub function: String,
    pub line: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionError {
    pub code: ErrorKind,
    pub safe_message: String,
    pub line: Option<u32>,
    pub trace: Vec<TraceFrame>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct HostErrorCode(String);

impl HostErrorCode {
    pub fn new(value: impl Into<String>) -> Result<Self, ConfigError>;
    pub fn as_str(&self) -> &str;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostError {
    pub code: HostErrorCode,
    pub safe_message: String,
    pub retryable: bool,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum ExecutionOutcome {
    Completed { value: Value, usage: BudgetUsage },
    Exited { code: u8, usage: BudgetUsage },
    Denied { denial: Denial, usage: BudgetUsage },
    LinkError { error: LinkError, usage: BudgetUsage },
    RuntimeError { error: ExecutionError, usage: BudgetUsage },
    HostError { error: HostError, usage: BudgetUsage },
    BudgetExceeded { failure: BudgetExceeded, usage: BudgetUsage },
    DeadlineExceeded { deadline: MonotonicInstant, usage: BudgetUsage },
    Cancelled { usage: BudgetUsage },
    AuditFailure { error: AuditFailure, usage: BudgetUsage },
    InternalFailure { fault_id: u128, safe_message: String, usage: BudgetUsage },
    RecordFailure { error: RecordFailure, usage: BudgetUsage },
    ReplayMismatch { error: ReplayMismatch, usage: BudgetUsage },
}
```

`HostErrorCode::new`はASCII lowercase `[a-z][a-z0-9_.-]{0,63}`だけを受理する。空文字列は`ConfigError::EmptyIdentifier`、64 bytesを超えるものは`ConfigError::IdentifierTooLong`、NULを含まない文字種の違反（例: `"A"`・`"1a"`）は`ConfigError::InvalidDescriptor { field: "host_error_code", code: "invalid_identifier" }`を返す（§3）。文字種の違反に`InvalidDescriptor`を返すことは loop:drift #352 D-68 の Q1=A（判断役が選択、issue #352）による。理由: NULを含まない入力に`IdentifierContainsNul`を返すと失敗の理由が観測した事実と合わない（[マニフェスト](manifesto.md) 原則5・原則6）。`InvalidDescriptor`はhost function名の文字種の違反（`code: "invalid_identifier"`）にすでに使われ、`field`・`code`で機械可読に区別でき、公開enumにvariantを足さずに済む（原則8）。採らなかった案B（新しいvariantを足す）は、公開APIを広げ、同じ種類の誤りの返し方が2通りになるため採らない。`safe_message`、`Debug`、`Display`へsecret、absolute path、native backtrace、panic payloadを入れない。

v1の`Completed.value`は`Value::Null`。top-level返値を将来追加してもvariantは変えない。

### 8.1 script catch可否

| 事象 | script内catch | terminal outcome / Phase |
|---|---|---|
| 型、index、name、argument等の通常runtime error | 可 | 未捕捉時`RuntimeError` / Phase 1 |
| script操作中のcapability denial | 可 | canonical `capability` / `sandbox` error。未捕捉時`RuntimeError` / Phase 2 |
| script操作中のsanitized adapter/host function failure | 可 | canonical `host` error。未捕捉時`RuntimeError` / Phase 2 |
| link/import等、script handler開始前のcapability denial | 不可 | `Denied` / Phase 2 |
| link/import等、script handler開始前のhost failure | 不可 | `HostError` / Phase 2 |
| fuel/heap/I/O/source budget | 不可 | `BudgetExceeded` / Phase 3 |
| deadline | 不可 | `DeadlineExceeded` / Phase 3 |
| cancellation | 不可 | `Cancelled` / Phase 3 |
| `exit(code)` | 不可 | grantあり`Exited` / Phase 2。grantなしはcatch可能capability error |
| engine/host callback panic | 不可 | `InternalFailure` / Phase 1/2 |
| audit/record/replay control failure | 不可 | 対応するterminal outcome / Phase 6 |

capability/host errorをcatch可能にしても、denyは対象adapter/OS/callbackより前に成立し、catchによる権限取得はできない。budget、deadline、cancel、audit/internal/record/replay failure、valid `exit`をcatch可能なscript errorへ変換してはならない。`exit`は整数1個または省略時0、0..=255だけを受理する。範囲外・型不一致はcatch可能な`RuntimeError`で、libraryは`std::process::exit`を呼ばない。`Denied` / `HostError` terminalはscript handlerが存在しないlink/control-plane channelに限定する。

## 9. Handle、lifetime、Send/Sync、再利用

`ExecutionState`、`ExecutionHandle`、`PollSlice`、`PollResult`、`HandleError`、`Engine::create_execution` / `start`、`poll` / `pause` / `resume` / `set_waker` / `usage` / `outcome`の最終公開定義は[実行予算・協調実行仕様](execution-control.md)第9節を唯一の正本とする。本書では`Ready/Running/Suspended`等の別stateや`run()`をhandle固有の唯一drive APIとして再定義しない。同期`Engine::run`は、同文書のstate machineをterminalまでpollする制約付きconvenience methodである。

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StartError {
    EngineMismatch,
    RevisionMismatch,
    ContextBusy,
    ContextPoisoned,
    InvalidRequest { field: &'static str, code: &'static str },
    Backpressure { active: usize, queued: usize, limit: usize },
    ConcurrentRunRequiresPolling,
    AuditUnavailable,
    InternalFailure { fault_id: u128, safe_message: String },
}
```

開始前のAPI/config mismatchだけが`StartError`、handle作成後は全て`PollResult`と保存済み`ExecutionOutcome`である。`start(&LinkedScript, ...)`はLinkedから、`create_execution(&CompiledScript, ...)`はCreatedから開始する。

| 型 | Clone | Send | Sync | 契約 |
|---|---:|---:|---:|---|
| `Engine` | `Arc`で共有 | yes | yes | build後immutable |
| `CompiledScript` | shallow | yes | yes | 同engine/revision/backendで再利用可 |
| `LinkedScript` | shallow | yes | yes | immutable graphを再利用可 |
| `ExecutionContext` | no | **no** | **no** | 作成threadのみ、同時実行不可 |
| `ExecutionRequest` | yes | yes | yes | create/startでhandleへmove、finite budgetを所有 |
| `ExecutionHandle` | no | no | no | 作成threadでpoll |
| `CancellationToken` | yes | yes | yes | 任意threadからcancel可 |
| `ExecutionWaker` | yes | yes | yes | 別threadはwakeだけ可能 |

`ExecutionContext`の`!Send + !Sync`はstable契約である。Phase 2同期adapterはcaller thread上で有限時間に`Ready`相当の結果を返す。Phase 4 cooperative adapterだけが`Pending` ticketを返し、別threadはresult格納とwakeだけを行う。同期callbackを暗黙worker threadへ移さない。同じcontext/handleへの再入は許さず`InternalFailure`とする。audit sink callbackの中から同じ`Engine`の入口を呼ぶ再入も許さない。`run`は`ExecutionOutcome::InternalFailure`、`compile`は`CompileDiagnosticCode::InternalFault`の診断1件の`CompileErrors`、`link`は`LinkError::InternalFailure`で、何も実行せずに拒否する。この拒否は実行を始める前に返すもので、渡されたcontextを変えずpoisonもしない（第10節規則4のpoisonは実行したexecutionのterminalに対するもので、この拒否には適用しない）。対象の入口の全体（REPL用の内部入口と監査付き入口を含む）、外側のexecutionの扱い、入れ子のcallbackは[決定性・実行時監査仕様](determinism-and-audit.md)第10節を正本とする。

## 10. 境界、状態遷移、transaction

| 段階 | 許可 | 禁止 | 失敗channel |
|---|---|---|---|
| compile | root lex/parse/backend compile/hash | import、OS、host callback | `CompileErrors` |
| Created/link | resolver import、module単体compile、cycle/depth/hash、source budget（static cross-module symbol/arityはPhase 2外・後続Phase） | script命令、runtime FS、stdio、clock | `ExecutionOutcome`またはpre-handle `LinkError` |
| start/create | engine/context/request整合、finite budget検証、capability freeze、admission | script命令、host callback | `StartError` |
| poll/run | control確認後にscript、capability、host callback、audit | process exit、ambient access、runtime import | `PollResult` / `ExecutionOutcome` |

状態遷移は[実行予算・協調実行仕様](execution-control.md)第9節の`Created → Linked → Ready → Running → Yielded/Paused/Terminal`だけを使う。`poll`だけがsemantic workを進め、`resume()`はPausedを保存済みstateへ戻すだけで、実作業は次の`poll(PollSlice)`が行う。terminal outcomeは1回だけ保存し、terminal後にscriptを再実行しない。

規則:

1. handle作成前のmismatch/backpressureだけを`StartError`にし、作成後のlink/control/runtime結果はterminal `ExecutionOutcome`にする。
2. nonterminal handleのdropはcancelをlinearizeし、pending host callを`Detached`で閉じ、全language-stateをrollbackし、fail-closed audit journalへ最後の`Terminal(Cancelled)`をappendする。dropはblocking I/Oやpanicを行わない。
3. unwind中も内部frame/handlerをtop-levelへ復元し、再unwindしない。
4. `InternalFailure`だけcontextをpoisonする。他terminal後はrollback/commit完了後に再利用できる。
5. AUD-024は全executionに適用する。`Completed` / `Exited`だけが全language-stateをcommitし、catch済みerror後に最終`Completed` / `Exited`ならcatch前後をcommitする。それ以外のterminalはexecution開始時点までbinding、cell、List/Dict、function、module marker等をrollbackする。
6. stdout、filesystem、network、database、process、host function等の完了済み外部効果はrollbackしない。最終`AuditEvent::Terminal`は`context_committed`と`host_effects_may_remain`で結果を表す。
7. capability、arguments、environment、stdio、tokenはcontextへ保存せずterminal後に破棄する。scriptがglobalへ保存した通常Valueが次executionへ残るのは、保存したexecutionが`Completed` / `Exited`でcommitした場合だけである。

## 11. Host panic、abort、exit

1. lexer/parser/linker/backend/host functionのunwind panicを各host boundaryで`catch_unwind(AssertUnwindSafe(...))`する。
2. compile panicは`CompileDiagnosticCode::InternalFault`、link/start panicはそれぞれ`LinkError::InternalFailure` / `StartError::InternalFailure`、handle作成後のrun/poll panicはterminal `ExecutionOutcome::InternalFailure`とし、process内で一意な非ゼロのfault ID（乱数源を使わない。§3の`EngineId`と同じ、loop:drift #352 D-66 Q1=A）を付ける。panic以外では、audit sink callbackからの同じEngineへの再入の拒否（第9節）にも`CompileDiagnosticCode::InternalFault` / `LinkError::InternalFailure` / `ExecutionOutcome::InternalFailure`を使う。
3. panic payload/native backtraceはpublic errorへ含めず、host-only diagnostic hookがある場合だけ渡す。
4. `panic=abort`、OOM、stack overflow abort、FFI UB、callback内process exit/abortは捕捉不能。[脅威モデル](threat-model.md)どおり別process隔離が最終防御である。
5. 公式host function/adapterはpanic、abort、process exitを行ってはならない。
6. Tsumugi 自身の crate（lib、CLI adapter の `src/main.rs`、開発用ツールの `src/bin/`）の production コード（`#[cfg(test)]` の付いたコードを除く）は `unsafe` を使わない。各 crate root に `#![cfg_attr(not(test), forbid(unsafe_code))]` を置き、`unsafe` が入れば `cargo build`・`cargo test` が失敗するようにする。規則 4 の「FFI UB」のうち Tsumugi 自身が持ち込む分をこの規則で閉じる。依存 crate の中の `unsafe` は [脅威モデル](threat-model.md) TM-012 のとおり host 責任で、この規則の対象外である（issue #543）。

## 12. CLI adapter

CLIはroot file/stdin読込み、UTF-8検証、profile/capability構築、legacy budget環境変数（`TSUMUGI_MAX_*`）のadapter（[実行予算・協調実行仕様](execution-control.md) §13）、Engine API、outcome→OS code変換だけを行う。parser/compiler/evaluator/VMを直接呼ばない。REPLのparse診断（EOFで入力が完結しないときを含む）も`Engine::compile`の`CompileErrors`から出す。入力の継続判定のトークン走査を例外として許すかは fixスライス fix-cli-repl-compile-diagnostics の設計で決める。現在の実装との差は上の逸脱一覧にある。この扱いは loop:drift #429 D-134 の Q1=A（判断役が選択、issue #429）による。理由: 本節の境界（CLIはEngine APIのadapterに徹する）は診断・source identity・課金の経路を1本に保つためのもので（[マニフェスト](manifesto.md) 原則5・原則8）、`Engine::compile`が同じlex/parseを既に行うのでそこへ寄せればparse診断の経路が1本になる。本書冒頭の loop:drift #263・#282 の決定どおり、目標を実装に合わせて後退させず差を逸脱一覧に載せる。採らなかった案B（実行を伴わない入力の完結判定に限ってlexerを使ってよいと本節に書く）は、目標の境界へ例外を足して実装に合わせる形で#263・#282の決定と逆向きになり、EOFの診断の`Parser`直接呼び出しはBでも逸脱として残るため採らない。唯一の文法は[次期意味論・実装決定](semantic-decisions.md)第6節である。

```text
tsumugi [OPTIONS] [SCRIPT [ARGS...]]
```

- OPTIONSには`--vm`、`--profile safe|legacy`、capability option、`--help`、`--version`、`--`だけを置く。`--backend`は定義しない。
- option解析中の最初のpositionalをSCRIPTとし、それ以後は`--vm`、`--help`、`--`、未知option風tokenを含め一切再解釈せず、`ExecutionRequest.arguments`へ順序どおり渡す。
- `-`はstdin script、scriptなしと`tsumugi --`はREPLである。
- `args()`はbinary名、script path、CLI flagを含めない。
- 非UTF-8、unknown option、option値欠落、profile/capability usage errorはscript開始前にstderrへ診断しexit 1とする。help/versionだけstdout・0である。
- profile/capability optionの値検証とgrant構築は[Capability Model仕様](capability-model.md)第14節に従う。safe profileのstdout既定はbuilderによる明示`Stdout` grantでありambient accessではない。
- **capability注入経路**: CLIはprofileから組んだfrozen `CapabilitySet`を、tree・VMのfile/stdinとREPLのすべてでEngine API（`ExecutionRequest::with_capabilities`、link時は`LinkRequest.capabilities`）へ注入する。`--allow-import-root`が`module_resolver`をgrantする。REPLはsessionで1つの`ExecutionContext`を再利用し、各submissionの`ExecutionRequest`へ同じfrozen setを載せ、`clear_user_state`やcapability clearを呼ばずに持ち越す。現在の実装との差は上の逸脱一覧にある。
  - **REPLのimport解決**: REPL（tree・VM）もfile/stdinと同じく、各submissionをlink時のEngine resolver gateに通し、runはそのlinkが確かめたgraphから展開する。`module_resolver`が未grantでimportを含むsubmissionはterminal `Denied`とし、下の結果表のとおり表示して次の入力へ進む（[Capability Model仕様](capability-model.md) §10、issue #327 Q1=A）。現在の迂回の正本は同仕様の逸脱一覧のREPLのimportの行である。この箇条を目標の文だけにし、現状を逸脱一覧への参照にすることは loop:drift #352 D-60 の Q1=A（判断役が選択、issue #352）による。理由: [マニフェスト](manifesto.md) 原則2と優先順位の表「明示的なcapability ＞ ambient authorityによる利便性」、[ドキュメント運用ガイド](documentation-guide.md)「逸脱一覧の持ち方」（本文は目標、現状は逸脱一覧だけ）。採らなかった案B（現状の記述と目標の追記を残し、本書の逸脱一覧にもREPLの行を足す）は、目標の章に迂回を規範の形で書いた記述が残り、同じ状態を2つの逸脱一覧に持つことになるため採らない。
  - **REPLのbudget**: REPLの各submissionは1 executionであり、そのsubmissionの`ExecutionRequest`が所有する有限budgetで実行する（§6、[実行予算・協調実行仕様](execution-control.md) §3・§13）。submissionをまたいで引き継ぐのはcontextだけである。この規則は loop:drift #352 D-67 の Q1=A（判断役が選択、issue #352）による。理由: §6は全executionが作成時から有限budgetを持ち`ExecutionRequest`がそれを所有すると定め、REPLを例外とする規定は目標に無い。[マニフェスト](manifesto.md) 原則3（スクリプトが消費できる資源をホストが制御できる）に照らし、目標を実装に合わせて後退させない。累積の扱いは fix スライス fix-repl-submission-budget の設計で決める。採らなかった案B（session累積を目標の規則として書く）は、2回目以降のsubmissionでhostが渡したbudgetを使わない挙動を目標にし、原則3に反しうるため採らない。

| 結果 | file/stdin mode | REPL |
|---|---:|---|
| `Completed` | 0 | 次入力へ |
| `Exited { code, .. }` | code | session終了、codeを返す |
| `RuntimeError` / `Denied` / `LinkError` / `HostError` / `BudgetExceeded` / `DeadlineExceeded` | 1 | 表示し次入力へ |
| `Cancelled` | 130 | 現在入力だけcancelしpromptへ戻る |
| `AuditFailure` / `InternalFailure` / `RecordFailure` / `ReplayMismatch` | 70 | session終了、context破棄 |
| CLI usage / UTF-8 / file / stdin read error | 1 | 該当なし |
| compile error | 1 | REPL chunkなら表示し次入力へ |

`tree`が既定・規範backend。`--vm`はexperimental VMを選択しwarningを要求する。VMも`Engine::compile`と同じexecution-control APIを通り、conformance完了まで別のstable backend名や`--backend` optionを公開しない。

## 13. 既存API移行

現行`Engine::new()`、`compile(&str)`、`execute(...)`、引数なし`ExecutionContext::new()`はalpha APIである。

### Release N-1

1. 最終型を追加する。名前衝突methodは一時的に`compile_v1`、`link_v1`、`run_v1`で提供する。
2. 旧APIへ`#[deprecated]`。
3. 旧wrapperはambient権限を付与しない。外部効果は`Denied`とし新APIへの移行を示す。
4. crate rootから新型をre-export。low-level moduleはstable surface外と明記する。

### Release N

1. breaking alpha releaseとして旧methodを削除し、本書の`compile/link/start/run`へ改名する。
2. `ExecutionContext::new(&Engine)`だけを残す。
3. CLI tree/VM両経路を新APIへ統一する。
4. 旧`TsumugiError`は内部型として残せるがterminal APIから返さない。

compatibility shimもprocess exitやambient accessを復活させず、script操作中のcapability/host errorをcatch可能とするcanonical契約と、link/control-planeのterminal契約を変更してはならない。

### 13.1 alpha 期間に受容した公開 API の破壊的変更

alpha（後方互換無保証）の間に、crate root（`src/lib.rs`）の公開型を変える破壊的変更を受容したときは、この表に 1 行で記録する。
受容の条件は「この表に記録すること」であり、ロードマップには書かない（ロードマップは今後の予定だけを持ち、完了の履歴を持たない。
[ドキュメント運用ガイド](documentation-guide.md)「ロードマップと spec の責務分離」）。設計の理由は各行の「設計の記録」の節が持つ。
CHANGELOG は置いていないので、公開 API の変更の記録は公開 API の正本であるこの仕様にまとめる。

| 変更 | 壊れるもの | `language_revision` | 設計の記録 |
|---|---|---|---|
| `YieldReason::AuditBackpressure` と `AuditFailure::BackpressureDeadline` の追加（B-1） | 公開 enum を網羅的に `match` する外部コード | bump なし（既存 tag 5/6・既存 `AuditYieldReason::AuditBackpressure`） | [決定性・実行時監査仕様](determinism-and-audit.md) の A-1 実装スライスの「`AuditBackpressure`-yield 配線（§10/§10.2、B-1、実装済み）」の項 |
| 簡略 `AuditPayloadField` の撤去と `AuditPayload.fields` の §9.1 の `BTreeMap<String, AuditField>` への差し替え（R-1a） | `AuditPayload`・`AuditPayloadField` に依存する外部コード | bump なし（CBOR tag 表・charge table・script 観測値は不変） | [決定性・実行時監査仕様](determinism-and-audit.md) の R-1 実装スライス |
| 公開関数 `tsumugi::audit::estimate_encoded_bytes`（`pub mod audit` 経由で crate root の `pub use` には無い）の削除（audit-strict-cbor-c2）。journal・audit drain queue の byte 計測は §9.2 の encoder の実 encode 長に差し替え、naive 推定の関数は残さない | `tsumugi::audit::estimate_encoded_bytes` を呼ぶ外部コード | bump なし（CBOR tag 表・charge table・script 観測値は不変） | [決定性・実行時監査仕様](determinism-and-audit.md) の audit-strict-cbor-c2 実装スライス（判断 Q5=A） |

この節は loop:drift #318 の D-22 で足した。受容の記録を roadmap でなく spec に置くこと（Q1=A）は判断役が選択（issue #318）した。
理由: ロードマップは「完了の履歴」を持たないと運用ガイドが定めており、破壊的変更を受容した設計判断はマニフェスト原則 9（動機・保証・
トレードオフの文書化）のとおり spec が持つ。置き場所に CHANGELOG でなくこの節を選んだのは設計役で、CHANGELOG が無く、
公開 API の移行（本章）と同じ文書にあれば利用者が 1 か所で読めるためである。

## 14. 実装slice

| Slice | Phase | 内容 | 完了条件 |
|---|---:|---|---|
| E1 | 1 | ID/config/error/outcome/EngineBuilder | constructor、default、secret-free Debug |
| E2 | 1 | CompiledScript、importなしLinkedScript、source/root hash | byte-level hash、source非保持default |
| E3 | 1 | tree backend adapter | 既存tree意味論、CLI同一入口 |
| E4 | 1/3 | Context/Handle cleanupとtransaction journalの縦切り | 再利用、poison、drop、全language-state rollback |
| E5 | 1 | Completed/Runtime/Internal/Cancelled(pre-run) channel | process継続、catch規則 |
| E6 | 1 | compile/link/run panic隔離 | unwind test、fault ID |
| E7 | 2 | capability/import graph/Denied/Exited/HostError/host function接続 | capability Phase 2基準 |
| E8a | 1 | CLIがEngine APIだけを通る入口統合、基本引数転送（🟡 tree file/stdinはimportの有無を問わずEngine API経由で完了〔importはC6/E7〕。tree REPLもE7でEngine API統合済み〔`compile`→`link`→`run_for_repl`。importはsubmissionごとのlinkのgraphから展開〕。VM file/stdinのimport gateは統合済み〔E9の一片。link層resolver gate経由〕、VM backend adapter本体はE9残） | EMB-AT-15/16/17のPhase 1範囲、AUD-018 |
| E8b | 2 | capability profile/options、safe/legacy移行（✅ 完了。profile/options 配線は C9/C10/E7/E11 で統合済み。migration warning は[capability model](capability-model.md) §14.4 の N-1 スキップ・safe 既定採択により正式に descope（不実装）。EMB-AT-10/11 は profile 経由 subprocess test で固定済み——第16節 Phase 2 参照） | EMB-AT-10/11/16、CAP-AT-23〜26（migration warning は descope——§14.4 参照） |
| E9 | 5/7 | VM experimental adapter/conformance（🟡 VM file/stdinのimport・resolver gateはEngine resolver〔`compile`→`link`、`module_resolver` capability〕経由へ統合済み。残りはadapter/conformance〔charge parity・StableError・FunctionId・transaction 他〕とstable化） | 同API、差分0でstable化 |
| E10 | N-1/N | deprecation/migration | compile test |
| E11 | 3 | budget/deadline/runtime cancel（🟡 tree範囲で実装。`EngineConfig.budget`で有限`BudgetConfig`、`ExecutionRequest::with_deadline_clock`でdeadline clock、`ExecutionContext::cancellation_token()`で実行中cancel。`ExecutionOutcome`に`BudgetExceeded`/`DeadlineExceeded`追加、budget/deadline/cancelをcatch不能terminal化（`ErrorKind::is_uncatchable`をtree/VM共有）。実system monotonic clock（`SystemMonotonicClock`）と全terminal variantへの`usage: BudgetUsage`同梱まで実装済み。さらに最終形移行Slice 1として`ExecutionRequest`を`new(budget, clock) -> Result<_, ConfigError>`へ移し有限`BudgetConfig`とdeadline clockを必須所有（`with_deadline_clock`独立経路と`EngineConfig.budget`二重管理を解消、deadlineは`budget.deadline`のみ、domain/revision/deadline検証を構築時へ前倒し）、CLI既定をoption B＝import なし経路でstandard有限budgetへ変更。`CancellationToken`必須所有も実装済み（`ExecutionRequest`が`cancellation`を所有し`.cancellation(value)`で設定、`Engine::run`が実行直前にinstall）。REV-015最終形移行は完了） | terminal variant境界test |
| E12 | 4 | final state machineとpoll/pause/resume | `PollSlice` / `PollResult`、yield/pause/state test |
| E13 | 6 | canonical audit sink/event/usage完全性 | 8 event、fail-closed、`BudgetUsage` test |

E1→E2→E3→E4→E5→E6→E8a、次にE7→E8b。E11以降をPhase 1/2完了条件へ前倒ししない。

### fix-cli-vm-experimental-warning 実装スライス（CLI の `--vm` の warning と `--help` の文言を spec に合わせる）

loop:drift #318 の D-29 と D-31 を直す。2 件とも CLI の利用者に見える文言（stderr と `--help`）が spec と食い違う件で、
触るのは `src/cli_capability.rs` と CLI の入口だけなので 1 本にした。

- D-29: §12 と [脅威モデル](threat-model.md) §7 の保証 6（VM は conformance 完了まで experimental として明示される）に対し、
  `--vm` は warning を出さない（逸脱一覧の該当行）。warning を出す fix を足すこと（Q1=A）は判断役が選択（issue #318）した。
  理由: 本文（§12・threat-model §7 の保証 6・TM-017）が目標で、library 側はすでに `Backend::VmExperimental` で明示している。
  CLI では warning を要求しないと spec を改める案（B）は、規範でない backend を選んだことを CLI 利用者に知らせない状態を目標に
  することになり、マニフェスト原則 9（根拠のない主張をしない）に反しうるので選ばなかった。
- D-31: `--allow-import-root` の `--help`「（値検証のみ）」、`build_safe_capabilities` の doc comment（「値検証のみで set には寄与しない」）、
  `src/capability.rs` のテストのコメント（「module_resolver を grant しない」）が、[Capability Model仕様](capability-model.md) §14
  （C6-c で filesystem `ModuleResolver` を safe profile へ grant する）と実際の挙動に合わない（capability-model の逸脱一覧の該当行）。

- **実装範囲**:
  1. CLI が `--vm` を受けて VM で実行するとき（file・stdin・REPL のどれでも）、script の開始前に stderr へ次の warning を 1 行だけ出す。
     文言は定数 `WARN_VM_EXPERIMENTAL` として `src/cli_capability.rs` に置く。
     `警告: --vm は experimental な VM backend を選びます。規範の backend は tree-walk です`
     usage error・`--help`・`--version` で終わるときは出さない。exit code・stdout は変えない。
  2. `USAGE` の `--vm` の行を `--vm                        experimental なバイトコード VM で実行する` にする。
  3. `USAGE` の `--allow-import-root` の行を `--allow-import-root NAME=PATH  import root を mount し module_resolver を許可する` にする
     （列の揃えは既存の行に合わせる）。
  4. `build_safe_capabilities` の doc comment と `src/capability.rs` のテストのコメントを、C6-c の挙動（`--allow-import-root` が
     `module_resolver` を grant する）に合わせて書き直す。コードの挙動は変えない。
- **範囲外**:
  - library の `Backend::VmExperimental`・`allow_experimental_backend` の挙動。
  - VM の conformance そのもの（E9 の残り）。
- **観測される変化**: `--vm` を付けた実行で stderr の先頭に 1 の warning が 1 行増える。`--help` の 2 行が変わる。
  warning の 1 行を除いた stdout・stderr と exit code は変えない（`--vm` の stderr を空や固定文字列で照合する既存の test は、
  CVW-AT-03 のとおり warning の 1 行を除いて同じ確認を保つように直す）。
- **受入基準（fix-cli-vm-experimental-warning）**:
  - **CVW-AT-01**（warning）: `tsumugi --vm <file>`・`tsumugi --vm -`（stdin）・`tsumugi --vm`（REPL、入力は空）のそれぞれで、
    stderr に `WARN_VM_EXPERIMENTAL` がちょうど 1 行出て、stdout と exit code は warning の無い場合と同じ。`--vm` を付けない
    実行では出ない。`tsumugi --vm --help`・`tsumugi --vm --unknown` では出ない。
    確かめ方: subprocess test（`tests/` の CLI の test）で stdout・stderr・exit code を照合する。
  - **CVW-AT-02**（help の文言）: `USAGE` の `--vm` と `--allow-import-root` の行が実装範囲 2・3 の文言になり、その
    文言で `--help` の stdout に出る。`USAGE` に「値検証のみ」が残らない。
    確かめ方: `USAGE` の文言を照合する既存の test は無いので、`--help` の stdout を照合する test を足す（`tests/` の CLI の
    subprocess test、または `src/cli_capability.rs` の unit test で `USAGE` の 2 行を照合する）。
  - **CVW-AT-03**（既存の stderr の確認）: `--vm` を付けた実行の stderr を照合する既存の test 全般（tree と `--vm` の stderr の比較、
    stderr が空であることの確認、固定文字列との完全一致を含む。例: `tests/integration.rs` の `for use_vm` の REPL の
    `stderr.is_empty()`（:1500-1516 付近）、VM REPL の `stderr.is_empty()`（:1622 付近）、`use_vm` での stderr の完全一致
    （:3580-3594 付近））は、どれも `WARN_VM_EXPERIMENTAL` の 1 行を除いた残りに対して、今と同じ確認（空・完全一致・比較）を保つように
    直す。確認を緩めたり対象から外したりしない。
    確かめ方: `cargo test` 全体。直した test の一覧を実装 PR の本文に書く。
- **想定の変更規模**: 本体（`#[cfg(test)]` を除く `src/`）は `src/cli_capability.rs` と CLI の入口（`src/main.rs`）で +10〜25 行・
  −3〜5 行程度。テストは、新しい test（CVW-AT-01・02）が +50〜100 行、`--vm` の stderr を照合する既存の test の書き換え
  （CVW-AT-03。`tests/integration.rs` の数か所〜十数か所。warning の行を除く共通の helper を 1 つ置いて各所から使う）が +20〜40 行・
  −10〜30 行程度。受入基準は 3 項目で、[開発ループ手順書](loop-runbook.md) §14.1 に収まる。
  done にする実装 PR で、本書の逸脱一覧の「CLI の `--vm` は experimental VM を選んだことを知らせない」行と、
  [Capability Model仕様](capability-model.md) の逸脱一覧の `--allow-import-root` の文言の行と、[脅威モデル](threat-model.md) の逸脱一覧の
  「CLI の `--vm` は experimental VM を選んだことを stderr にも `--help` にも示さない」行を削除する。

### manifesto-check-no-host-abort 実装スライス（lib の production コードに process::exit・abort・無条件の panic が許可リスト外に無いことを固定する）

loop-guard §4.1 の 1 段目（機械チェック）のうち、[マニフェスト](manifesto.md) 原則 6（スクリプトの失敗によってホストプロセスが panic、abort、
または意図せず exit しない）を、`cargo test` で走る静的検査として固定する（週次計画 #355・#417 の P-1、親 AUD-022）。本書 §11 規則 5
（公式 host function/adapter は panic、abort、process exit を行ってはならない）と規則 1（unwind panic は各 host boundary で
`catch_unwind` する）を、`catch_unwind` に頼る前の段で裏付けるスライスで、§11 の目標の文は変えない。同じ P-1 の先行スライス
manifesto-check-ambient-surface（[Capability Model仕様](capability-model.md) §18 の同名小節）の静的検査の作りを参考にする。

検査の置き場所・`unreachable!` の扱い・`process::exit`／`abort` の扱い・`.unwrap()`／`.expect()` の扱い・許可リストの単位は、判断役が選択（PR #462）の Q1=A・Q2=A・Q3=A・Q4=A・Q5=A・Q6=A による（下の「決定記録」）。

- **前提（設計時点の main c5e5e84 で確かめたこと）**:
  - `std::process::exit`・`abort` は `tests/ambient_access_audit.rs` の `process` の key が `src/` 全体で検出し、許可は `main.rs`×`process` の 1 組だけである（原則 2・8 の検査）。
    原則 6 の側の検査は無く、`panic!` などのマクロを検出する検査も無い。
  - lib の production コード（`src/main.rs` と `src/bin/` を除く `src/` の `*.rs` から、`#[cfg(test)] mod` を除いた部分。`tests/ambient_access_audit.rs` の `production_lines` と同じ切り出し）に、
    `todo!`・`unimplemented!`・`process::exit`・`process::abort` は無い。
  - 同じ範囲の `panic!` は 4 か所: `src/embedding.rs` 403 行（`#[doc(hidden)] pub fn audit_budget_for_test`。ホストが不正な `AuditBudget` を渡したとき。`tests/audit_queue_bytes.rs` が使う）、
    2405・2637 行（`#[cfg(test)]` の付いた `if` 文の中で、production の build には入らない）、`src/stable_error.rs` 125 行（`message_ids!` マクロの `const` の初期化式の中で、build 時に評価される）。
  - 同じ範囲の `unreachable!` は 29 か所: `src/eval.rs` 18・`src/parser.rs` 8・`src/builtin.rs` 2・`src/compiler.rs` 1。どれも直前の match・検査で除いた分岐に置かれた内部不変条件で、
    script の入力から届くかは各箇所で確かめていない。
  - `.unwrap()` は 7 か所（`src/compiler.rs` 6・`src/eval.rs` 1）、`.expect(` は lib で 120 か所余り（`src/parser.rs` の `self.expect(...)` のような自前のメソッドを含む粗い数え）ある。
  - `Cargo.toml` に `panic = "abort"` の profile 設定は無い（unwind panic は §11 規則 1 の `catch_host_unwind` で捕まえられる）。
- **実装範囲**:
  1. 検査は新しい `tests/no_host_abort_audit.rs` に置く。行の切り出し（文字列・コメントの除去、`#[cfg(test)] mod` の除外）と `use` の展開は、`tests/ambient_access_audit.rs` から
     共通の helper module（例: `tests/support/source_scan.rs`。両方のテストが `#[path]` か `mod` で取り込む）へ移して共有する（Q1）。共有する切り出しは実装範囲 3 の規則とし、`tests/ambient_access_audit.rs` もこの規則で production コードを切り出す（Q6）。
     `tests/ambient_access_audit.rs` の main での検出結果は変えない（除くのは production の build に入らないコードだけである）。同ファイル冒頭の doc コメントの範囲の説明は、この規則に合わせて直す。
  2. 走査対象は `src/` 配下の全 `*.rs` から `src/main.rs`（CLI adapter）と `src/bin/`（開発用ツール）を除いたもの。lib の中で script から届く経路は静的に絞れないので、lib の production コード全体を
     上位集合として検査する。
  3. test コードの除外は、`#[cfg(test)]` の直後の item が `mod` のもの（その module の終わりまで）に加え、`#[cfg(test)]` の直後の文・式・関数・impl（その item の終わり。brace のある item は brace が閉じるまで、
     無いものは `;` まで）とする。production の build に入らないコードだからである。ただし、`#[cfg(test)]` の直後の行が `,` で終わるもの（構造体のフィールド・struct literal のフィールド・関数呼びの引数。例: `src/embedding.rs` 1842・2026・2133・2391 行）は、その 1 行だけを除く（次の `;` まで除かない）。`#[cfg(test)]` と item が同じ行にあるもの（例: `src/embedding.rs` 2229 行の `#[cfg(test)] poll_trace: …,`）は、attribute の後ろをその item の頭として読み、上の規則（`,` で終わればその 1 行だけ、`mod` や brace のある item は brace が閉じるまで）を当てる。次の行を item の頭として読まない。
     この規則は共有 helper の切り出しとして 2 つの検査に共通に使う（Q6）。
  4. 検出パターン: `panic!`・`todo!`・`unimplemented!`・`unreachable!`（Q2）、`std::panic::panic_any`・`resume_unwind`、`process::exit`・`process::abort`（Q3。修飾無しの `process::` を含む）と、
     `use` 文でこれらを取り込む行（`use std::process;` の module の取り込みを含む）。`std::panic!` のような修飾つきの呼びも照合する。`assert!` 系・`debug_assert!` 系・`.unwrap()`・`.expect(` は検出しない（Q4）。
  5. 許可リストは「ファイル・パターン・件数」の組の単位にし、各組に理由と原則を書く（Q5）。組の件数と実際の一致数が違えば検査を失敗させる（増えたら新しい箇所に、減ったら古い許可に気付ける）。
  6. 前提の `panic!` の該当のうち、`stable_error.rs`×`panic`（const の初期化式で build 時に評価され、実行時には起きない）と `embedding.rs`×`panic`（ホスト専用の `#[doc(hidden)]` builder で、script から届かない）を
     理由付きで載せる。`unreachable!` の 29 か所は、実装役が各箇所の到達条件を読み、script の入力から届かない内部不変条件であることを確かめてからファイルごとの組（件数つき）で載せる。
     script の入力（source・host から渡る値）から届く箇所が見つかったら、本スライスで `src/` を直さず、`fix-` スライスとして PM に返す（その箇所は組の件数に含めず、テストを `#[ignore]` にせず、
     実装 PR を止めて返す）。
  7. ファイル冒頭に、検査項目・守る原則・本書 §11 の規則の対応表を置く。
- **検査の限界**: 行単位の文字列照合なので、許可した組のファイルの中で箇所が入れ替わっても件数が同じなら区別しない。`assert!`・`debug_assert!` の条件つき panic、`.unwrap()`・`.expect()`、
  slice の添字・`RefCell::borrow_mut` の panic、整数の overflow、OOM・stack overflow の abort、依存 crate の中の panic、`macro_rules!` の展開先・別名を通した呼びは検出しない。
  unwind panic は §11 規則 1 の `catch_host_unwind` が最後の受け皿で、本検査はその前の段である。これらはレビューで見る。
- **範囲外**: `.unwrap()`・`.expect()` の検査（Q4。sub-slice の manifesto-check-no-host-unwrap〔`slices.toml`、todo〕へ分けた）。`src/` の panic 箇所の書き換え（見つかれば fix スライスへ）。構文木による解析。CLI adapter（`src/main.rs`）の `process::exit`
  （§12 の exit code の写像で、`tests/ambient_access_audit.rs` が `main.rs`×`process` として扱う）。`.github/`（凍結）の変更。§11 の目標の文の変更。
- **受入基準**:
  - **NHA-AT-01**（走査の範囲）: `src/main.rs` と `src/bin/` を除く `src/` 配下（サブディレクトリを含む）の `*.rs` が走査され、`#[cfg(test)] mod` と `#[cfg(test)]` の付いた文・関数の中は除かれ、
    それ以外は除かれない。確かめ方: 検査関数を文字列の入力で呼ぶ unit を同じテストファイルに置き、`#[cfg(test)]` の付いた `if` 文の中の `panic!` が検出されず、その直後の文の `panic!` が検出されることと、`#[cfg(test)]` の付いた構造体フィールドと関数呼びの引数（行が `,` で終わるもの）の直後の行の `panic!` が検出されることと、`#[cfg(test)] x: T,` のように attribute と item が同じ行にある引数の直後の行（`) -> T {` と関数本体）の `panic!` が検出されることと、
    除外するパスの判定を確かめる。
  - **NHA-AT-02**（検出パターン）: 実装範囲 4 のパターンと `use` 経由の取り込みが、それぞれ違反として検出され、`assert!`・`debug_assert!`・`.unwrap()`・`.expect(` と文字列・コメントの中の `panic!` は検出されない。
    確かめ方: パターンごとの文字列入力の unit（`use std::process;` のあとの `process::exit(1)`、`std::panic!(...)` を含む）。
  - **NHA-AT-03**（許可リスト）: 許可リストの各組に理由と原則があり、組の件数と一致数が違うと検査が失敗する。確かめ方: 件数の多い・少ない入力の unit と、`cargo test --test no_host_abort_audit` が main で通ること。
  - **NHA-AT-04**（対応表と共有）: テストファイル冒頭に検査項目・守る原則・§11 の規則の対応表があり、`tests/ambient_access_audit.rs` が共有の helper に移したあとも同じ違反を検出する。
    確かめ方: 対応表はレビューで読む。`cargo test --test ambient_access_audit` が通り（実装範囲 3 の規則の共有で ambient の検出結果が main で変わらない。Q6）、同テストの既存の unit（MAS-AT-01〜03）が変更なしか、helper の呼び先の書き換えだけで通ること。
- **想定の変更規模**: `tests/no_host_abort_audit.rs` が +150〜250 行（許可リストと unit を含む）、共有の helper が +200〜250 行で、その分 `tests/ambient_access_audit.rs` が −200〜250 行。
  `src/` は 0 行の見込み（実装範囲 6 のとおり、直す必要が出たら fix スライスへ）。[開発ループ手順書](loop-runbook.md) §14.1 の行数と受入基準（5 項目以内）に収まり、
  触る仕組みは「lib の production コードの静的検査（tests/）」1 つ。逸脱一覧の変更は見込まない。
- **依存**: なし（manifesto-check-ambient-surface は done）。
- **決定記録**（判断役が選択（PR #462））:
  - **Q1=A**（検査は新しい `tests/no_host_abort_audit.rs` に置き、行の切り出しと `use` の展開は `tests/ambient_access_audit.rs` から共有の helper へ移して 1 つだけ持つ）。理由: 守る性質が違う（原則 6 と原則 2・8）ので検査と対応表は分け、
    production コードの範囲を決める切り出しは 1 つにして、2 つの検査が違う範囲を見ないようにする（原則 8・9、優先順位の表「再現可能な検証」）。[Capability Model仕様](capability-model.md) §18 の manifesto-check-ambient-surface の Q1=A
    （同じものを 2 つ持つと、食い違ったときにどちらが正しいか説明できない）と同じ考え方で、性質が別なので検査は 2 本・作りは 1 つになる。`ambient_access_audit.rs` の検出結果を変えないことは NHA-AT-04 で確かめる。
    採らなかった案 B（helper を複製する）は、片方だけ直ったときに 2 つの検査の「production コード」の範囲が食い違い、どちらの裏付けが正しいか説明できなくなる（原則 9）ため採らない。
    案 C（`ambient_access_audit.rs` に足す）は、原則 2・8 の許可リスト・対応表に原則 6 の検査が混ざって許可の理由がどの原則のものか読みにくくなり、対応表を 2 本それぞれのテストの冒頭に持つという `slices.toml` の P-1 のメモとも合わないため採らない。
  - **Q2=A**（`unreachable!` を検出に含め、到達条件を確かめたうえで件数つきの組で許可する）。理由: `unreachable!` は届けば無条件に panic するので、原則 6 と本書 §11 規則 5 の対象である。§11 規則 1 の `catch_unwind` は最後の受け皿で、
    その前の段で新しい箇所に機械で気付けるようにする（優先順位の表「構造化された停止・失敗」「再現可能な検証 ＞ 根拠のない安全性の主張」）。29 か所の確認は `src/` を変えず、届く箇所は fix スライスへ返すので [開発ループ手順書](loop-runbook.md) §14.1 に収まる。
    採らなかった案 B（含めない）は、無条件の panic の大半（設計時点で 33 か所中 29）が検査の外に残り、script の入力から届く `unreachable!` が新しく入っても 1 段目で弾けず、検査が通ることを「無条件の panic が許可リスト外に無い」の裏付けとして扱えなくなるため、原則 6 に反しうるとして採らない。
  - **Q3=A**（`process::exit`・`abort` をこの検査でも検出し、lib の許可は 0 組にする）。理由: `ambient_access_audit.rs` の `process` の key は原則 2・8 の検査で、`Command` なども同じ key に入る。あちらで lib のファイル×`process` の組が理由付きで許可されると、
    そのファイルの exit・abort も通るので、原則 6 の裏付けが別の原則の許可リストの運用に依ってしまう。こちらで独立に弾けば原則 6 の 1 段目がそれだけで閉じる（原則 6、§11 規則 5、優先順位の表「再現可能な検証」）。
    こちらの許可は空で、走査から `main.rs` を除くので、許可リストの二重持ち（manifesto-check-ambient-surface の Q1=A が避けたもの）にはならない。
    採らなかった案 B（検出せず対応表で参照する）は、パターンを 1 か所にできるが、原則 6 の側の検査が「別の検査の許可リストが広がらないこと」を前提にし、広がったときに気付く仕組みが無いため採らない。
  - **Q4=A**（`.unwrap()`・`.expect()` は本スライスに含めず、検査の限界に書き、sub-slice manifesto-check-no-host-unwrap〔todo、order 116〕を `slices.toml` に残す）。理由: `slices.toml` の P-1 のメモ（含めると変更が増えうるので sub-slice に分ける）のとおりで、
    未解決事項を次の作業の正本に記録して追えるようにする（原則 9）。限界を小節に明記するので、検査が通ることを unwrap・expect まで含めた保証として扱う余地は無い（[マニフェスト](manifesto.md)「現在地の扱い」）。order 116 は手順書 §8 の「それ以外」の既定に合う。
    採らなかった案 B（sub-slice を足さない）は、残りが検査の限界の一文だけになり `slices.toml` から追えない（原則 9）ため採らない。案 C（本スライスで含める）は、130 か所近くの到達条件を `unreachable!` 29 か所と合わせて 1 回で確かめることになって確認が粗くなりやすく、
    根拠の弱い許可が「確かめた」ものとして残る（優先順位の表「再現可能な検証 ＞ 根拠のない安全性の主張」、手順書 §14.1）ため採らない。
  - **Q5=A**（許可リストは「ファイル・パターン・件数」の組、各組に理由と原則、件数と一致数が違えば失敗）。理由: `eval.rs`×`unreachable!` のように 1 つの組に 18 か所が入るので、ファイル・パターンの組だけでは新しい箇所が素通りする。
    件数があれば増えたときに新しい箇所に、減ったときに古い許可に気付ける（原則 6・9）。manifesto-check-ambient-surface の Q2=A（許可したファイルの中の新しい呼びを弾けない単位は採らない、一致しない組で失敗させる）と同じ理由を、1 組に多数の箇所が入るこちらの事情に当てたもの。
    件数が同じまま箇所が入れ替わる場合を区別しないことは上の「検査の限界」に書いた。
    採らなかった案 B（ファイル・パターンの組）は、許可したファイルの中に新しい無条件の panic が入っても機械チェックで弾けないため、原則 6 に反しうるとして採らない。案 C（箇所ごと）は、`unreachable!()` のように同じ文字列の行が多く箇所を区別できず、
    行番号に頼ると無関係な編集のたびに検査が壊れて許可リストの直しが形だけになる（原則 8・9）ため採らない。
  - **Q6=A**（共有 helper の切り出しを実装範囲 3 の規則にし、`tests/ambient_access_audit.rs` と `tests/no_host_abort_audit.rs` が同じ範囲を見る。設計 PR のレビュー指摘から出た候補）。理由: `#[cfg(test)]` の付いた文・関数は production の build に入らないので、
    ambient の検査の対象（原則 2・8 の production コードの ambient な直接呼び）が production コードから外れることはない。「production コード」の定義が 1 つなら、2 つの検査が通ることを同じ範囲についての裏付けとして読める
    （原則 9、優先順位の表「再現可能な検証 ＞ 根拠のない安全性の主張」）。上の Q1=A（切り出しは 1 つにして、2 つの検査が違う範囲を見ない）と [Capability Model仕様](capability-model.md) §18 manifesto-check-ambient-surface の Q1=A にそのまま合う。
    NHA-AT-04 で ambient の検出結果が main で変わらないことを確かめるので、共有した切り出しが production コードを誤って除けば、ambient の既存の許可リスト（一致しない組で失敗）と unit が回帰の検査として働く。
    MAS-AT-01 の「フィールド等の `#[cfg(test)]` で打ち切られない」も、除くのがその item だけで走査は続くので成り立つ。同 §18 の実装範囲 3（除外を mod に限る）は、その時点の経緯の記録として残す。
    採らなかった案 B（共有は mod の除外までとし、広い除外は新しいテストだけに置く）は、ambient の検査の挙動に触れずに済む（原則 8）が、切り出しが 2 通りになって 2 つの検査の範囲が違う理由を持ち続けることになり、
    Q1=A で採らなかった案 B と同じ弱点を範囲の定義の側に残す（原則 9）。除かれるのは build に入らないコードだけなので、ambient の側を広いまま残しても原則 2 の裏付けは強まらないため採らない。
    判断の外の気付き（判断役のコメント）を受け、`#[cfg(test)]` と item が同じ行にある場合（`src/embedding.rs` 2229 行）の扱いを実装範囲 3 と NHA-AT-01 に明記した。2 つの検査に同時に効く切り出しの誤りで `run_audited_inner` の本体を丸ごと除かないためである。

### manifesto-check-no-unsafe 実装スライス（Tsumugi 自身の crate の production コードへの `unsafe` の追加を `forbid(unsafe_code)` で止める）

issue #543（kamonabe、手順書 §8 で受け入れ）を受けて、本書 §11 規則 6 を足し、その規則を機械で守る。loop-guard §4.1 の 1 段目（機械チェック）のうち、
[マニフェスト](manifesto.md) 原則 1（ホストの安定性）と 6（ホストプロセスを abort させない）に当たる。`unsafe` の中の未定義動作は §11 規則 1 の
`catch_unwind` でも捕まらず（規則 4 の「FFI UB」）、manifesto-check-no-host-abort（`tests/no_host_abort_audit.rs`）と
manifesto-check-ambient-surface（`tests/ambient_access_audit.rs`）の検出パターンにも入らないので、入る前に止める。
先行の 2 本と同じく親は AUD-022 で、置き場所も同じ並び（本書 §14 の manifesto-check 系の小節）にした。

- **前提（設計時点の main 1a9463e で確かめたこと）**:
  - `src/` 配下で `unsafe` のキーワードは 2 か所だけで、どちらも `src/embedding.rs` の `#[cfg(test)] mod` の中のテスト
    `c3_env_snapshot_is_fixed_and_isolated_from_process_env` が、edition 2024 で `unsafe` になった `std::env::set_var`・`remove_var` を呼ぶもの。
    ほかに `unsafe` を含む字句は `src/capability.rs` の関数名 `reject_if_unsafe_reparse` とそのコメントだけで、キーワードではない。
  - `#[no_mangle]`・`#[export_name]`・`#[link_section]`・`extern` ブロック（`unsafe_code` の lint が対象にするもの）は `src/` に無い。
  - `src/lib.rs` の crate attribute は `#![allow(clippy::…)]` の 3 行だけで、`src/main.rs`（`src/cli_capability.rs` はこの crate の module）と
    `src/bin/gen_builtins_doc.rs` には crate attribute が無い。`Cargo.toml` に `[lints]` は無い。
  - 3 つの crate root に下の attribute を置いた手元の確かめで、`cargo build --all-targets --all-features` と `cargo test --lib --all-features c3_env`
    （上のテストを含む）は通り、`src/value.rs`（lib）と `src/cli_capability.rs`（bin）に `unsafe {}` を足すとそれぞれ `cargo build` が
    `error: usage of an unsafe block` で失敗した。
- **やり方の選択**（issue #543 が設計役に任せた点。下の「設計役の選択」に理由）: A の `#![cfg_attr(not(test), forbid(unsafe_code))]` を採る。
  対象には `src/main.rs` と `src/bin/` を含める。
- **実装範囲**:
  1. `src/lib.rs`・`src/main.rs`・`src/bin/` の各 crate root（今は `gen_builtins_doc.rs` の 1 本）の先頭の inner attribute の並び（`//!` の doc コメントの後、最初の
     `mod`・`use`・item の前）に、`#![cfg_attr(not(test), forbid(unsafe_code))]` を 1 行置く。理由を 1 行の通常のコメントで添え、本書 §11 規則 6 を指す。
     `forbid` なので、下位の module や item で `#[allow(unsafe_code)]` に戻せない（E0453）。例外を作るにはこの attribute 自体を変える PR が要る。
  2. 新しい `tests/no_unsafe_audit.rs` を置き、次を確かめる。ファイル冒頭に、検査項目・守る原則・本書 §11 の規則・脅威モデルの TM-ID の対応表を置く。
     - crate root の列挙: `src/lib.rs`・`src/main.rs` と、`src/bin/` 直下の `*.rs` と `src/bin/*/main.rs`（Cargo の bin の自動検出の規則）。
       `src/bin/` に bin を足せば自動で対象に入る。
     - 各 crate root に、実装範囲 1 の attribute と完全に一致する行（前後の空白を除く）が、最初の `mod`・`use`・`fn`・`pub` などの item より前にある。
       コメントや文字列の中の一致は数えない（`tests/support/source_scan.rs` の `strip_literals_and_comment` を使ってよい）。
     - attribute の意味の確かめ: 実装範囲 1 の attribute（テストの定数で持ち、上の照合と同じ文字列）を先頭に置いた小さな fixture を、一時ディレクトリで
       `rustc --edition 2024 --crate-type lib --emit metadata` に通す（`rustc` は環境変数 `RUSTC` があればそれ、無ければ `PATH` の `rustc`）。
       `unsafe {}` を含む fixture と、その関数に `#[allow(unsafe_code)]` を付けた fixture は失敗し、`--test` を付けて `#[cfg(test)]` の中だけに
       `unsafe` を置いた fixture は成功する。
  3. `Cargo.toml` は変えない（`[lints.rust] unsafe_code = "forbid"` は `#[cfg(test)]` の中の `unsafe` も止めるので採らない。issue #543）。
- **観測される変化**: 無い。言語の挙動・公開 API・CLI の出力・fuel・heap・監査の値は変えない。`unsafe` を足した build が失敗するようになるだけである。
- **検査の限界**: `#[cfg(test)]` の付いたコードの `unsafe` は対象外（issue #543 のとおり）。`cargo test` の lib の unit test の build（`cfg(test)`）では
  attribute が外れるが、同じ `cargo test` が integration test と doctest のために lib と bin を `cfg(test)` 無しで build するので、production コードの `unsafe` は
  そこで失敗する（`--no-default-features` では bin `tsumugi` を作らないので、その構成では lib と `gen_builtins_doc` だけを見る）。依存 crate の中の `unsafe`（TM-012）、`benches/`・`examples/`・`tests/` の crate、build script は対象外。crate root の attribute を消す・
  弱める変更は、実装範囲 2 の照合で `cargo test` が失敗する（そのテストまで消す変更はレビューで見る）。
- **範囲外**: `src/` の production コードの書き換え（`unsafe` は今 0 か所）。`benches/`・`examples/` の crate への attribute。`Cargo.toml` の `[lints]`。
  `src/embedding.rs` のテストの `unsafe`。依存 crate の監査。`.github/`（凍結）と CI の job（今ある `cargo build`・`cargo test`・`cargo clippy` の中で止める）。
- **触る trust boundary と脅威**: 実行時の経路は変えない。守るのは [脅威モデル](threat-model.md) §6 の B3（native code 境界）の内側の Tsumugi 自身のコードで、
  §10 の TM-014 の残余リスク「FFI UB」のうち Tsumugi が自分で持ち込む分を 1 段目で閉じる。受入基準は NUS-AT-01〜03。TM-012（依存 crate）は host 責任のまま扱わない。
- **受入基準**:
  - **NUS-AT-01**（attribute の位置）: `src/lib.rs`・`src/main.rs`・`src/bin/` の全 crate root に、実装範囲 1 の attribute が最初の item より前にあり、
    1 つでも欠ける・文字列が違う・item の後ろにある・コメントの中にあるだけなら検査が失敗する。確かめ方: `tests/no_unsafe_audit.rs` で main の 3 本を照合する test と、
    文字列の入力（欠ける・`warn` に弱めた・item の後ろ・コメントの中）で判定関数を呼ぶ unit。crate root の列挙が `src/bin/` の `*.rs` と `*/main.rs` を拾う unit。
  - **NUS-AT-02**（production の `unsafe` を止める）: 実装範囲 1 の attribute の下で、`unsafe` ブロックを含む lib は `rustc` が失敗し、`#[allow(unsafe_code)]` で
    戻そうとしても失敗し、`--test` の build の `#[cfg(test)]` の中の `unsafe` は通る。確かめ方: 実装範囲 2 の fixture の test。加えて実装 PR の本文に、lib の 1 ファイルと
    `src/cli_capability.rs` に `unsafe {}` を足したときの `cargo build` の失敗を貼る（commit しない）。
  - **NUS-AT-03**（今の main で通る）: `cargo build --all-targets --all-features`、`cargo clippy --all-targets --all-features -- -D warnings`、`cargo test`
    （`src/embedding.rs` の `c3_env_snapshot_is_fixed_and_isolated_from_process_env` を含む）が通る。確かめ方: 手順書 §14.2 の機械チェックと CI。
- **想定の変更規模**: `src/` は 3 ファイルに attribute 1 行とコメント 1 行ずつで +6 行程度（`#[cfg(test)]` を除く）。テストは `tests/no_unsafe_audit.rs` が
  +100〜180 行（fixture と unit を含む）。触る仕組みは「Tsumugi 自身の crate の静的検査（crate attribute と tests/ の 1 本）」1 つで、受入基準は 3 項目。
  [開発ループ手順書](loop-runbook.md) §14.1 に収まる。done にする実装 PR で、本書の逸脱一覧の「crate root に `#![cfg_attr(not(test), forbid(unsafe_code))]` が無い」行（§11 規則 6 の未実装範囲）を削除する。
- **依存**: なし。
- **設計役の選択**（issue #543 が設計役に任せた点。spec に無い判断で公開 API・データ構造・言語挙動に影響するものは無い）:
  - **A を採る**（`#![cfg_attr(not(test), forbid(unsafe_code))]`）。理由: 検出をコンパイラの lint に任せるので、`unsafe` ブロックだけでなく `unsafe fn`・
    `unsafe impl`・`unsafe extern`・`#[no_mangle]` のような unsafe な attribute まで、文字列照合の取りこぼし（`macro_rules!` の展開先、`reject_if_unsafe_reparse`
    のような識別子との区別）無しに止められる。`forbid` は下で `allow` に戻せないので、例外は crate root の attribute を変える差分としてレビューと
    審査に必ず見える（原則 1・9、優先順位の表「再現可能な検証 ＞ 根拠のない安全性の主張」）。採らなかった B（`tests/support/source_scan.rs` を使う許可リスト方式）は、
    行単位の字句照合なので上の取りこぼしがあり、許可 0 組の検査を保つために `cfg(test)` の切り出しと識別子の境界の判定を正しく持ち続ける必要がある。
    それでも A と同じ範囲を止められないため採らない。A の弱点（attribute を消せば止まらない）は、実装範囲 2 の照合（NUS-AT-01）で `cargo test` を失敗させて補う。
  - **`src/main.rs` と `src/bin/` を含める**。理由: CLI adapter はホストの process そのもので、`unsafe` の未定義動作は lib と同じく捕まえられない（§11 規則 4）。
    今 0 か所で、足すのは 1 行ずつなので、「Tsumugi 自身の crate の production コード」を 1 つの規則で言える（原則 9）。`#[cfg(test)]` を外す形は lib と揃え、
    CLI のテストが環境変数を書き換える場合に lib と同じ扱いにする。`benches/`・`examples/` は host に配る production コードではないので含めない。
  - **置き場所は本書 §14 の manifesto-check 系の小節の並び、親は AUD-022**。理由: 規則の目標（§11）が本書にあり、先行の manifesto-check-no-host-abort（§11 規則 5 を
    裏付ける）と同じ形で §11 規則 6 を裏付けるスライスだからである。`slices.toml` の親も先行の 3 本（ambient-surface・no-host-abort・no-host-unwrap）に揃える。
  - **`order` は 120**。手順書 §8 の「それ以外」の既定（すべてのスライスより後。設計時点の最大は 119）で、issue #543 の「順番」のとおり。

### fix-link-request-budget-charge 実装スライス（`Engine::link` で `LinkRequest` の budget に source/import の count・bytes を課金し上限を判定する）

loop:drift #352 の D-62 を直す（親 REV-015）。§5 は `LinkRequest` が [実行予算・協調実行仕様](execution-control.md) の有限 `BudgetConfig` を所有し、
source/module/import の count・bytes を同文書の `BudgetUsage` へ課金すると定めるが、`Engine::link` は課金も上限判定もしない（本書の逸脱一覧の §5 の行）。
本スライスは §5 の目標の文を変えず、`Engine::link` の側を目標に合わせる。

run の課金を残すか・root を課金するか・CLI の表示・import を課金する時点は、判断役が選択（PR #494）の Q1=A・Q2=A・Q3=A・Q4=A による（下の「決定記録」）。

- **前提（設計時点の main 3b1fc4c で確かめたこと）**:
  - `Engine::link`（`src/embedding.rs` の `link_inner`・`resolve_module_imports`・`read_module_source`）は、`request.budget` を
    `link_call_context` で resolver の `CapabilityCallContext` へ deadline と `max_import_bytes`（remaining-bytes の器）として渡すだけで、
    `BudgetLedger` を作らず `charge_source`・`charge_import` を呼ばない。`LinkError::BudgetExceeded` は resolver が
    `AdapterError::Control(ControlStop::BudgetExceeded)` を返したとき（`adapter_error_to_link_error`）にしか出ない。
  - source/import の課金は run の `Link` フェーズで `ExecutionRequest` の budget に行う（tree は `src/eval.rs` の `charge_link_graph`、
    VM は `src/vm.rs` の `charge_link_graph`。root を `charge_source`、初めて展開した module ごとに `charge_source` と `charge_import`）。
    [Capability Model仕様](capability-model.md) §10 の目標の文も「課金はこれまでどおりrunの`Link`フェーズ」とする。
  - `LinkRequest::new` を呼ぶのは、CLI の 4 か所（`src/main.rs` の tree file/stdin・tree REPL・VM REPL・VM file/stdin。どれも
    `LegacyBudgetEnv` から作った同じ `BudgetConfig` を link と run の両方へ渡す）、scheduler の handle 経路（`src/engine.rs` の
    初回 poll の `Link` フェーズ。`evaluator.budget_config()` を渡す）、テストである。
  - CLI の `print_link_error` は `LinkError::BudgetExceeded` を `リンクエラー: {:?}` の Debug 表示で出す。scheduler の handle 経路の
    `link_failure_outcome` は `control_stop_to_error` で budget 系の error に写す（PR #480 の Q1=A）。
  - `BudgetLedger::new(config, cancellation)` は clock を持たず、`set_clock` しなければ deadline を判定しない。`LinkRequest` は clock を持たない。
- **実装範囲**:
  1. `Engine::link` は、engine・revision・backend の検証と step 0 の cancel の確認のあと、resolver の取得より前に、`request.budget` と
     `request.cancellation` から link 専用の `BudgetLedger` を 1 つ作る（clock は据えない）。この台帳は link の間だけ使い、`LinkedScript` にも
     run にも渡さない。
  2. root source を [実行予算・協調実行仕様](execution-control.md) §5.3 のとおり `source_count` の 1 本目として、`CompiledScript` の
     root source の生 UTF-8 byte 長で `charge_source(_, ExecutionPhase::Link)` する。import 0 件の script（resolver の有無を問わず、空 graph を返す経路）でも
     課金し、超えたら空 graph を返さず失敗する（Q2）。課金の位置は、step 1 で resolver 未 grant + import の `LinkError::Denied` を返したあと、
     import 0 件の空 graph を返す前・resolver を呼ぶ前の 1 か所とする（Q5）。したがって import があって resolver が未 grant の script は、
     root が上限を超えていても今どおり `Denied`（resolver call 0）を返す。root で超えたときも resolver call は 0 である。
  3. 各 import は、`read_module_source` で読み終えて同じ `ModuleId` の hash の照合（既解決なら `continue`）を通ったあと、UTF-8 の検証と
     module 単体の parse より前に、その module の生 byte 長で `charge_source` と `charge_import` をこの順に行う（Q4）。既解決の `ModuleId`
     （同じ module の 2 回目の import、diamond）は §5.3 の cache hit なので課金しない。
  4. 課金の `ControlStop` は既存の `adapter_error_to_link_error` と同じ写像で `LinkError` にする（`BudgetExceeded` → `LinkError::BudgetExceeded`、
     `Cancelled` → `LinkError::Cancelled`）。`BudgetExceeded.phase` は `Link` である。複数の resource が同時に超えたときの primary は
     `charge_source`・`charge_import` の既存の §7.2 の優先順位のままにする。
  5. run の `Link` フェーズの `ExecutionRequest` への課金（tree・VM の `charge_link_graph`）は変えない（Q1）。`Engine::run` が返す
     `BudgetUsage` の source/import の値も変わらない。
  6. CLI（`src/main.rs`）は `LinkError::BudgetExceeded(exceeded)` を、run の `BudgetExceeded` と同じ文言で出す（Q3）。
     `tsumugi::budget::control_stop_to_error(ControlStop::BudgetExceeded(exceeded), 0, 0)` の `TsumugiError` の message を、file/stdin は
     `exit_code_for_outcome` の `BudgetExceeded` と同じ形で stderr に出して exit 1、REPL（tree・VM）は `report_repl_error` と同じ
     `  エラー: …` の形で出して次の入力へ進む。`LinkError` は行番号を持たないので行番号は付けない。それ以外の `LinkError` の表示は変えない。
  7. `link_call_context` と `Engine::link` の doc comment の「課金しない」「器のみ」の記述を、本スライスの挙動に合わせて直す。
- **範囲外**:
  - module source を chunk で読む途中で N+1 byte を溜める前に止めること（[Capability Model仕様](capability-model.md) §10 の「Phase 3の
    module/total limit NならN+1を蓄積する前に停止する」と CAP-AT-17 の byte の境界。同仕様の逸脱一覧の C11 の行）。本スライスは読み終えた
    module の byte 長で課金する。resolver へ渡す remaining-bytes の値も変えない。
  - link 中の deadline の判定（`LinkRequest` は clock を持たず、link の台帳は deadline を見ない。resolver の `CapabilityCallContext` が
    `request.budget.deadline` を運ぶ今の形は変えない）。
  - imported module record・AST の heap の課金（execution-control §5.3 の heap は run の `Link` フェーズの課金のまま）。
  - link の課金の監査 event（[決定性・実行時監査仕様](determinism-and-audit.md)。`Engine::link` は監査 event を出さない今の形のまま）。
  - REPL の session 累積の budget（逸脱一覧の REPL の budget の行。fix-repl-submission-budget）。link の台帳は submission ごとに作るので、
    REPL で前の submission が展開した module を再び import すると、link では課金し run では cache hit として課金しない。
  - `ExecutionOutcome` に `LinkError` を足すこと（逸脱一覧の 1 行目）。
  - §5 末尾の「`create_execution`からlinkする場合も同じrequest budget/controlを共有し、別の計数系を作らない」。scheduler の handle 経路
    （`src/engine.rs` の初回 poll）は `evaluator.budget_config()` から `LinkRequest` を作るので、本スライスの後も link の台帳は handle の
    台帳と別に作られる。同じ `BudgetConfig` で判定するので上限は緩まないが、同じ handle の台帳で link を進める形は、
    [実行予算・協調実行仕様](execution-control.md) の逸脱一覧の `create_execution` の行（`LinkOptions` が無く、Link を初回 poll で行う）の
    直し方で扱う。本スライスの done で削除するのは本書の逸脱一覧の §5 の行（`Engine::link` が課金も上限判定もしない）だけである。
  - 既解決の `ModuleId` を 2 回目に import するとき、resolver を呼んで source を読み終えてから hash 照合で skip する今の読み込み
    （`resolve_module_imports`）。この読み込みは課金しない（§5.3 の cache hit）。読み込みの量を上限で抑えることは、上の chunk の途中で
    止める規定（C11）と同じ扱いで範囲外に残す。
- **観測される変化**:
  - `LinkRequest` の budget の source/import の上限を超える script は、`Engine::link` が `Err(LinkError::BudgetExceeded(_))` を返す
    （D-62 の再現: import_count=0 で 1 module を import すると、今は `Ok`、変更後は `ImportCount` の `BudgetExceeded`）。
  - CLI は link と run に同じ budget を渡すので、`TSUMUGI_MAX_*` の source/import の上限を超える実行は run の `Link` フェーズでなく link で止まる。
    stderr の文言（`import の本数が上限を超えました` など）と exit code は変わらず、行番号の接頭辞（`N行目: `）が付かなくなる。
    ただし REPL（tree・VM）の session 累積の `TSUMUGI_MAX_SOURCE_BYTES`・`TSUMUGI_MAX_SOURCE_COUNT` の超過（各 submission は上限内で、
    累積で超えるもの）は、link の台帳が submission ごとなので link では止まらず、今どおり run で止まる（範囲外の REPL の項）。
    submission 単体で上限を超えるとき、VM REPL は link で止まるので、その submission で `charge_link_graph` まで進まない。
  - scheduler の handle 経路は、初回 poll の link で超えたとき、既存の `link_failure_outcome` の budget 系の error になる（今は run 側の課金で同じ種類の error）。
  - 上記以外の既存テストの期待値は変えない。変える必要が出たら実装を止めて判断待ちにする。
- **受入基準（fix-link-request-budget-charge）**:
  - **LRB-AT-01**（import の課金と境界）: `LinkRequest` の budget の `max_import_count`・`max_import_bytes`・`max_source_count`・
    `max_source_bytes`・`max_single_source_bytes` のそれぞれで、N（ちょうど）なら `Engine::link` が `Ok`、N+1 相当なら
    `Err(LinkError::BudgetExceeded(e))` で `e.resource` がその resource、`e.phase` が `Link` になる。D-62 の再現（import_count=0・import_bytes=1 で
    1 module を import）は `Err` になる。確かめ方: `src/embedding.rs` の link の unit test（既存の in-memory resolver の helper を使う）。
  - **LRB-AT-02**（root の課金と `Denied` との順序）: root source が `max_single_source_bytes` を超える script と `max_source_count = 0` の budget は、import 0 件でも
    `Engine::link` が `BudgetExceeded`（`SingleSourceBytes`・`SourceCount`）を返し、import を含む script（resolver は grant）では resolver の呼び出しが 0 回である。
    resolver が未 grant のときは、「import 0 件 + root が上限を超える」が `BudgetExceeded`、「import あり + root が上限を超える」が `LinkError::Denied`（resolver call 0）になる（Q5）。
    確かめ方: 呼び出しを数える resolver を使う unit test と、`CapabilitySet::empty()` で link する unit test。
  - **LRB-AT-03**（数え方と順序）: 同じ module を 2 回 import する script と diamond の graph で、import 1 本ぶんの上限（`max_import_count` = 異なる module の数）なら
    `Ok` になる（cache hit を数えない）。UTF-8 でない module と構文の不正な module は、`max_import_bytes` がその module の byte 長より小さければ
    `InvalidModule` でなく `BudgetExceeded` になる（parse の前に課金する）。上限内で link と run を通した `Engine::run` の `BudgetUsage` の
    source/import の値は今と同じである（`tests/differential.rs` の source/import の一致の test が変更なしで通る）。確かめ方: unit test と `cargo test`。
  - **LRB-AT-04**（CLI、tree・VM の両方）: `tests/integration.rs` の `import_count_limit_gates_file_execution`（`for use_vm`）と REPL の
    `TSUMUGI_MAX_SINGLE_SOURCE_BYTES`・`TSUMUGI_MAX_SOURCE_BYTES` の test が期待値を変えずに通る。加えて、tree・`--vm` の file/stdin と REPL で
    link で止まる上限超過（例: `TSUMUGI_MAX_IMPORT_BYTES=1`）の stderr に budget の文言が出て `リンクエラー:` が出ず、file/stdin は exit 1、
    REPL は次の入力へ進むことを subprocess test で確かめる。
  - **LRB-AT-05**（handle 経路）: scheduler の handle 経路で `max_import_count = 0` の budget の import が、初回 poll の link で budget 系の error
    （kind `source_limit`）になり 1 文も実行しない。確かめ方: `tests/scheduler_api.rs` に、実際の resolver（`BudgetStopResolver` のような
    resolver 側の停止でなく link 側の課金）で確かめる test を足す。
- **想定の変更規模**: 本体（`#[cfg(test)]` を除く `src/`）は `src/embedding.rs` が +30〜60 行（台帳の作成、root と各 module の課金、
  doc comment）、`src/main.rs` が +15〜30 行（`LinkError::BudgetExceeded` の表示を file/stdin と REPL で分ける）。テストは unit test と
  subprocess test で +150〜250 行。受入基準は 5 項目、触る仕組みは「link 層の budget 課金」1 つ（CLI の変更はその結果の表示だけ）で、
  [開発ループ手順書](loop-runbook.md) §14.1 に収まる。done にする実装 PR で、本書の逸脱一覧の §5 の `LinkRequest` の budget の行を削除する。
  範囲外に残すもの（link 中の deadline、chunk の途中の N+1）はそれぞれ既存の行・延期先（C11）で追えるので、新しい行は足さない見込み。
- **依存**: なし。
- **決定記録**（判断役が選択（PR #494））:
  - **Q1=A**（run の `Link` フェーズでの `ExecutionRequest` の budget への課金を残し、link は link 専用の台帳で上限を判定する）。
    理由: `LinkRequest` と `ExecutionRequest` はそれぞれ有限 `BudgetConfig` を所有する別の request である（§5・§6）。link が link の台帳で上限を判定し、
    run が自分の budget に課金し続ければ、§5 の目標（link での課金と判定）を満たしつつ、[Capability Model仕様](capability-model.md) §10 の目標の文
    「課金はこれまでどおりrunの`Link`フェーズ」と [実行予算・協調実行仕様](execution-control.md) §5.3 の run の usage、tree・VM の `charge_link_graph` の一致
    （[マニフェスト](manifesto.md) 原則 5）を変えずに済む。目標の章を動かさない最小の変更でもある（原則 8）。台帳は別々なので、1 つの台帳が同じ source を
    2 度数えることは無い。採らなかった案 B（run 側の課金をやめ link だけで課金する）は、`LinkedScript` を link とは別の budget を持つ `ExecutionRequest` で
    何度でも run でき、run は保持した source を毎回再 parse・展開する（capability-model §10）ので、ホストがその execution に付けた source/import の上限が
    判定されず `BudgetUsage` からもその消費が消えるため、原則 3・7 に反しうるとして採らない（目標の章 2 か所の書き換えも要る）。
  - **Q2=A**（root を `source_count` の 1 本目として、resolver を呼ぶ前に link で課金する。import 0 件でも課金する）。理由: execution-control §5.3 は
    「`source_count`はrootを1として」数え、§5 は `LinkRequest` の budget に source の count・bytes を課金するとする。root を 1 本目に数えれば、同じ budget に
    対する判定が link と run で同じ数え方になる（原則 5）。root の課金は resolver を呼ぶ前なので、超えたら resolver call 0 で止まる（原則 1・3）。
    採らなかった案 B（import だけ課金する）は、link の `source_count`・`source_bytes`・`max_single_source_bytes` が root を欠いて §5 の逸脱が一部残り、
    同じ上限でも link と run で判定が 1 本ずれるため採らない。
  - **Q3=A**（CLI は `LinkError::BudgetExceeded` を run の `BudgetExceeded` と同じ文言で出し、行番号は付けない）。理由: §12 の結果表は `LinkError` も
    `BudgetExceeded` も file/stdin は 1、REPL は「表示し次入力へ」である。CLI は link と run に同じ budget を渡す（`src/main.rs` の 4 か所）ので、同じ超過が
    止まる場所だけが run から link へ移る。同じ文言なら利用者に見える失敗の種類が変わらず（原則 5・6）、`import_count_limit_gates_file_execution` の期待値
    （行番号は見ていない）も変わらない。handle 経路の `link_failure_outcome`（`control_stop_to_error` で budget 系の error、PR #480 の Q1=A）とも揃う。
    採らなかった案 B（今の `リンクエラー: {:?}` のまま）は、内部の型の Debug 表示が利用者に出て、同じ予算超過が入口（handle 経路と CLI）で違う形になり、
    既存テストの期待値も変えることになるため採らない。
  - **Q4=A**（各 import は、読み終えて同じ `ModuleId` の hash 照合を通ったあと、UTF-8 検証と module 単体 parse の前に `charge_source`→`charge_import` で課金する）。
    理由: 上限を超えた module に UTF-8 検証と parse の仕事をさせない（原則 1・3、優先順位の表「制御可能な最悪時負荷」）。順序は run の `charge_link_graph` と
    同じ source → import で、同時超過の primary は execution-control §7.2 の順のまま。hash 照合のあとなので、既解決の `ModuleId` は §5.3 の cache hit として
    数えない。採らなかった案 B（parse が通ったあとに課金する）は、反しはしないが、上限を超えた大きな module でも parse まで進み、予算で止めるべき入力に
    余分な仕事をする。不正な module を先に `InvalidModule` で返せる利点は、予算内に直せば同じ診断が得られるので小さく、採らない。
  - 判断の外の気付き（判断役のコメント）を受け、handle 経路の link の台帳と §5 末尾の「別の計数系を作らない」の関係、既解決の `ModuleId` の再読み込みの扱いを
    範囲外に明記した。
  - **Q5=B**（import があって resolver が未 grant の script は、root の予算超過より先に今どおり `Denied` を返し、root の課金はそのあとの 1 か所で行う。
    設計 PR のレビュー指摘から出た候補）。理由: A・B ともマニフェストに反しうる点は無い（どちらも resolver call 0 で、`Denied` の経路は何も読まず何も実行しない。原則 2・3）ので、
    spec の既存の決定との一貫性と変更の小ささで比べた。目標の章は resolver 未 grant + import を条件なしの terminal `Denied` と定めている（§5「resolver capabilityなしでimportがあれば
    resolver call 0で terminal `Denied`とする」、[Capability Model仕様](capability-model.md) §19 の CAP-AT-16、同 §14 の CLI の文、同 §18 の fix-handle-import-from-link の実装範囲 1 が書く
    `Engine::link` の規則の順）。B はこれらをそのまま保ち、本小節の「§5 の目標の文を変えず」とも合う。粗い capability の確認を budget の判定より先に置く順は、
    [次期意味論・実装決定](semantic-decisions.md) §3.7 の host function の行（symbol/arity → capability → host-call 回数 budget）と同じ向きである（原則 5）。今の main ではこの入力は
    link の `Denied` で止まるので、B は観測される結果を変えない（原則 8）。権限の欠如の診断が上限の設定に左右されない（原則 2・6）。
    課金の位置は、`Denied` を返したあと・import 0 件の空 graph を返す前の 1 か所に置けるので B でも経路ごとに分かれず、実装範囲 1 の「台帳は resolver の取得より前に作る」と
    Q2=A の「root の課金は resolver を呼ぶ前」とも両立する。採らなかった案 A（root の課金を step 1 より前に行い、`BudgetExceeded` を `Denied` より先に返す）は、反しはしないが、
    残る利点（root の超過が capability によらず同じ結果になる）が、目標の章の上の文に「予算内の script では」という条件を足す重さを上回らないため採らない。
    判断の外の気付き（判断役のコメント）を受け、課金の位置を 1 か所として実装範囲 2 に書き、LRB-AT-02 に resolver 未 grant の 2 つの場合を分けて足した。

### fix-repl-submission-budget 実装スライス（REPL の各 submission を、その submission の budget で数え直して実行する）

loop:drift #352 の D-67 を直す（親 REV-007）。§12 の「REPLのbudget」は、REPL の各 submission を 1 execution とし、その submission の
`ExecutionRequest` が所有する有限 budget で実行すると定める（§6、[実行予算・協調実行仕様](execution-control.md) §3・§13）。tree REPL
（`run_for_repl`）は 2 回目以降の submission で fuel だけを戻し、ほかの使用量を session の累積のまま持ち越す（本書の逸脱一覧の REPL の budget の行）。
本スライスは §6・§12 の目標の文を変えず、REPL の budget の扱いを目標に合わせる。累積の扱い（§12 が「fix スライスの設計で決める」とした点）と
VM REPL を含めるかは、判断役が選択（PR #496）の Q1=A・Q2=A による（下の「決定記録」）。

- **前提（設計時点の main ba9e88a で確かめたこと）**:
  - `Engine::run_for_repl`（`src/embedding.rs`）は `run_guarded(.., repl = true)` を通り、`install_execution_state` は同じ context の 2 回目以降の
    submission（`ExecutionContext.repl_budget_installed`）で `Evaluator::rearm_budget_for_repl`（`src/eval.rs`）→ `BudgetLedger::rearm_for_repl_submission`
    （`src/budget.rs`）を呼ぶ。これは request の `BudgetConfig`・clock・cancellation token を据え直し、`reset_fuel` で fuel の committed・reserved
    だけを 0 に戻す。committed・reserved のほかの counter（string・source・import・input・output・host）と `peaks`、heap 台帳（live と peak）は持ち越す。
    1 回目の submission は facade の `Engine::run` と同じ `reset_budget`（台帳を作り直す）を通る。
  - VM REPL（`src/main.rs` の `run_repl_vm`）は Engine API の run を通らず、session の `Vm`（`Vm::new_repl_with_budget(budget_env.vm_budget())`）が
    1 本の `BudgetLedger` を持つ。`Vm::run_repl_chunk`（`src/vm.rs`）は submission ごとに `reset_fuel` だけを呼ぶ。この差は
    [実行予算・協調実行仕様](execution-control.md) の逸脱一覧の D-136 の行が持つ。VM REPL の source/import は、CLI の `execute_repl_submission_vm` が
    `run_repl_chunk` の**前**に `Vm::charge_link_graph`（root と import の `charge_source`・`charge_import`、module record の heap 課金）で同じ台帳へ課金する
    （tree は link が別の台帳を使い、run の `install_execution_state` で据え直してから `charge_link_graph` するので、この順の問題は無い）。
  - heap 台帳の live 上限は `BudgetConfig` と別に `AllocationLedger` の `limit` が持ち、`set_max_live_heap_bytes` → `set_limit` だけが変える。
    今の `rearm_for_repl_submission` は `self.config` を差し替えるだけで heap 台帳の limit を据え直さない（1 回目の submission の値のまま）。
    VM の `Vm::new_repl_with_budget` の台帳も同じである。
  - heap 台帳は tracked な値（collection・cell・token）が `HeapLedgerWeak` で参照し、値の drop で release する。tree・VM とも、前の submission の
    artifact token（tree は `charge_linked_ast` の `ast_token`・`body_tokens`、VM は `chunk_tokens`）を release してから今回ぶんを Link で課金する。
    この release は、tree は `charge_link_graph` の最後の `charge_linked_ast` の中、VM は `run_repl_chunk` の中で行い、どちらも source・import・
    module record の課金より後である。
    REPL の context に残る変数・closure・collection と、展開済み module の record（`graph_record_tokens`）は live のまま残る。
  - CLI の 4 本の subprocess test（`tests/integration.rs` の `source_bytes_cumulative_limit_gates_across_inputs_in_both_engines`・
    `output_calls_limit_gates_print_in_both_engines`・`output_bytes_limit_gates_print_in_both_engines`・
    `string_bytes_cumulative_limit_gates_shared_builtin_in_both_engines`）は、tree・VM とも 1 行ずつの submission の
    累積で上限を超えることを期待値として固定している。
  - `tests/differential.rs` の REPL 継続の harness（`tree_usage_per_input` は `Evaluator::run_repl_submission` で台帳を一度も据え直さず、
    `vm_usage_per_input` は `charge_link` → `run_repl_chunk`）は、I/O・string・source の counter を入力ごとに tree・VM で突き合わせる。
  - [実行予算・協調実行仕様](execution-control.md) §5.2 は、context に以前から残る値を新 execution の baseline live heap とし、上限を超えれば
    1 文も実行せず `BudgetExceeded(HeapBytes)` とする。
- **実装範囲**:
  1. `BudgetLedger` に、REPL の submission の開始で使う操作を 1 つ置く（`rearm_for_repl_submission` を置き換える）。request の `BudgetConfig`・clock・
     cancellation token を据え、`committed`・`reserved` の全 counter（fuel を含む）と `peaks` を 0 に戻し、heap 台帳は作り直さずに peak だけを
     その時点の live に戻し、heap 台帳の live 上限を据えた `BudgetConfig` の `max_live_heap_bytes` に据え直す（`AllocationLedger::set_limit`）（Q1=A）。
     live heap はその submission から到達できる値の量なので持ち越し、§5.2 の baseline として扱う。
     監査の集約器（`BudgetChargeAccumulator`）も submission の境界で空にする。台帳の操作は artifact token を持たないので、呼ぶ側（2・3）が
     前の submission の artifact token を**先に** release してから 1 の操作を呼ぶ（peak を live に戻す時点の live に前の artifact を含めない）。
  2. tree REPL（`install_execution_state` の `repl && repl_budget_installed` の経路と `Evaluator::rearm_budget_for_repl`）は 1 の操作を使う。
     `Evaluator` 側の rearm で前の submission の `ast_token`・`body_tokens` を先に release してから 1 の操作を呼ぶ。
     返す `ExecutionOutcome` の `BudgetUsage` は、その submission だけの使用量と、その時点の live heap になる。
  3. VM REPL は、submission ごとに 1 の操作を `Vm::charge_link_graph` より**前**に呼ぶ（CLI が submission ごとに呼ぶ `Vm` の入口に置く。
     `run_repl_chunk` の中に置くと、その submission の source/import の使用量が消え、source の上限が前の submission の使用量を残したまま判定される）。
     この入口で前の submission の `chunk_tokens` を先に release してから 1 の操作を呼ぶ。
     `run_repl_chunk` の `reset_fuel` は 1 の操作に含まれるので外してよい。config は CLI が submission ごとに `budget_env.vm_budget()` から作って渡す
     （VM REPL は clock を持たないので、clock・deadline の扱いは今と同じ）（Q2=A）。受け取る入口の形は実装で決めてよい。
  4. context に残る live heap が submission の `max_live_heap_bytes` を超えるときは、Link の最初の heap 課金（import があれば module record、無ければ artifact token）で
     `BudgetExceeded(HeapBytes)` になり、1 文も実行しない（§5.2）。前の submission の artifact token は 2・3 のとおり先に release する。
     別の走査は足さない（live の持ち越しが baseline を兼ねる）。context は poison せず、REPL は §12 の結果表のとおり表示して次の入力へ進む。
  5. `run_for_repl`・`rearm_budget_for_repl`・`run_repl_chunk`・CLI の `execute_repl_submission` の doc comment とコメントにある
     「session 累積」「fuel のみ reset」の記述を、submission ごとの budget の記述に直す。
  6. CLI の 4 本の subprocess test の期待値を、submission ごとの判定に合わせて書き直す（RSB-AT-04）。`tests/differential.rs` の REPL 継続の harness
     （`tree_usage_per_input`・`vm_usage_per_input`）を、tree・VM とも CLI の流れのとおり submission ごとに台帳を据え直す形に直し、入力ごとの
     counter の突き合わせが通るようにする。
- **範囲外**:
  - scheduler の handle 経路と alpha の `Engine::execute`・`execute_repl_submission`（`src/engine.rs`）が execution ごとに使用量を数えない件
    （execution-control の逸脱一覧の D-136 の行のうち scheduler の部分。fix-execution-budget-usage-fresh）。
  - facade の `Engine::run` が context に残る値を baseline として課金しない件（D-137。fix-link-context-baseline-charge）。facade は run ごとに台帳を
    作り直すので、本スライスの live の持ち越しとは仕組みが違う。
  - REPL で前の submission が展開した module を再び import したとき、link では課金し run では cache hit として課金しない差
    （fix-link-request-budget-charge 小節の範囲外の項）。
  - session 全体の使用量の上限（submission をまたぐ合計の上限）。目標の章に無く、本スライスでは足さない（Q1 の案 C）。
  - VM REPL の Engine API への統合（E9 の残り）と、scheduler 側の `ExecutionContext::reset_step_budget` の削除（fix-scheduler-context-reset-step-budget）。
- **受入基準（fix-repl-submission-budget）**:
  - **RSB-AT-01**（使用量を submission ごとに数える、tree）: 同じ `ExecutionContext` で `run_for_repl` を 2 回呼ぶと、2 回目の outcome の
    `BudgetUsage` の `committed`・`reserved`・`peaks` が、同じ source を新しい context で 1 回目として実行したときと一致する（live heap と
    peak heap は除く）。確かめ方: `src/embedding.rs` の unit test（print・文字列の連結・source を含む submission）。
  - **RSB-AT-02**（その submission の上限で判定する、tree）: submission 1 を緩い budget で `print` 3 回、submission 2 を `max_output_calls = 2` で
    `print` 2 回にすると submission 2 は `Completed` になる。submission 3 を `max_output_calls = 1` で `print` 2 回にすると
    `BudgetExceeded`（`OutputCalls`）になり、submission 4 を緩い budget にすると `Completed` に戻る。確かめ方: unit test。
  - **RSB-AT-03**（live heap の持ち越しと baseline）: submission 1 で大きな List を変数に残すと、submission 2 の `BudgetUsage.live_heap_bytes` は
    その List の分を含み、`peak_heap_bytes` は submission 1 の途中の一時的な最大値を含まない。submission 2 の `max_live_heap_bytes` を残した live より
    小さくすると、1 文も実行せず（`print` が出ない）`BudgetExceeded`（`HeapBytes`）になり、context は poison されず、submission 3（緩い budget）で
    その変数を読める。submission 1 の AST（VM は chunk）が大きいとき、submission 2 の `peak_heap_bytes` はその artifact の分を含まない。確かめ方: tree は unit test、VM は `src/vm.rs` の unit test（submission ごとに実装範囲 3 の `Vm` の入口 → `charge_link_graph` → `run_repl_chunk` を回す）。
  - **RSB-AT-04**（CLI、tree・VM の両方）: 上の 4 本の subprocess test を、1 行ずつの submission がそれぞれ上限内なら累積で上限を超えてもエラーに
    ならず、1 つの submission（ブロックで複数行をまとめた入力）が上限を超えると今と同じ文言のエラーを出して次の入力へ進む、という期待値に書き直す
    （`TSUMUGI_MAX_SOURCE_BYTES`・`TSUMUGI_MAX_OUTPUT_CALLS`・`TSUMUGI_MAX_OUTPUT_BYTES`・`TSUMUGI_MAX_STRING_BYTES`）。`for use_vm in [false, true]` で両方とも同じ期待値で通る。
  - **RSB-AT-05**（VM REPL の使用量、Q2=A）: 実装範囲 3 の流れ（submission ごとに、開始の操作〔実装範囲 3 の `Vm` の入口〕→ `charge_link_graph` →
    `run_repl_chunk`）を 2 回回したあとの台帳の `committed`・`peaks` が 2 回目の submission の分〔source/import を含む〕だけになり、
    tree の RSB-AT-01 と同じ source で fuel 以外の counter が一致する。確かめ方: `src/vm.rs` の unit test か `tests/differential.rs`。
- **想定の変更規模**: 本体（`#[cfg(test)]` を除く `src/`）は `src/budget.rs` が +20〜40 行（submission の開始の操作、heap 台帳の peak を live に戻す操作、
  `rearm_for_repl_submission` の置き換え）、`src/eval.rs` が ±10〜20 行、`src/embedding.rs` が ±20〜40 行（主に doc comment）、`src/vm.rs` が +5〜15 行、
  `src/main.rs` が ±10〜20 行（VM REPL の submission ごとの config とコメント）で、合計 60〜140 行。テストは unit test と subprocess test で +150〜250 行
  （既存 4 本の書き直しと differential の harness の直しを含む）。受入基準は 5 項目、触る仕組みは「REPL の submission の開始での budget 台帳の据え直し」1 つ（tree・VM は同じ台帳の操作を
  呼ぶだけ）で、[開発ループ手順書](loop-runbook.md) §14.1 に収まる。done にする実装 PR で、本書の逸脱一覧の REPL の budget の行を削除し、
  [実行予算・協調実行仕様](execution-control.md) の逸脱一覧の D-136 の行から VM REPL の文を削る（延期先を本スライスに付け替える書き換えは設計 PR #496 で済ませた）。
- **依存**: なし。
- **決定記録**（判断役が選択（PR #496））:
  - **Q1=A**（使用量〔committed・reserved の全 counter と peaks〕は submission ごとに 0 から数え、heap 台帳は作り直さず live heap を持ち越して baseline とし、
    peak だけを live に戻す）。理由: [実行予算・協調実行仕様](execution-control.md) §13「budgetの使用量はexecutionごとに新しい`BudgetUsage`で数え」
    「REPLの各入力は1 executionとし、execution間ではcontextだけを引き継ぐ」と、本書 §6・§12（各 submission の `ExecutionRequest` が有限 budget を所有する）に合う。
    同 §2 の heap の定義（execution から到達可能な live byte 数）と §5.2（context に残る値は新 execution の baseline live heap で、超えたら 1 文も実行せず
    `BudgetExceeded(HeapBytes)`）を、context に残る値を上限で判定し続ける形で満たす（[マニフェスト](manifesto.md) 原則 3、優先順位の表「制御可能な最悪時負荷」）。
    台帳の操作 1 つで済み、別の走査や新しい設定を足さない（原則 8）。採らなかった案 B（facade の `Engine::run` と同じく submission ごとに台帳を作り直す）は、
    `reset_config_with_token` が台帳ごと作り直すので context に残る tracked 値が新しい台帳に課金されず、execution-control の逸脱一覧の D-137 の行と同じ穴
    （残した heap が以後の submission の `max_live_heap_bytes` で判定されない）が REPL にも開き、原則 3 に反しうるため採らない。採らなかった案 C
    （submission ごとの上限に加えて session 累積の上限も持つ）は、原則には反しないが、目標の章（§6・§12・execution-control §13）に無い session budget の設定と
    API を足すことになり、host は submission ごとに budget を渡し live heap は A でも持ち越して判定されるので、足す重さが上回らない（原則 8）。
  - **Q2=A**（VM REPL〔`Vm::run_repl_chunk`〕の同じ差も本スライスで直し、tree・VM が同じ `BudgetLedger` の操作を呼ぶ。fix-execution-budget-usage-fresh の
    範囲は scheduler の handle 経路だけにする）。理由: 原則 5（意味論・エラー・資源消費が backend ごとに変わることを許容しない）と優先順位の表「一貫した意味論」、
    execution-control §2 不変条件 5（tree と VM で同じ script・入力・budget なら同じ charge 列）、§13（legacy env の上書きを tree/VM の file・stdin・REPL の
    全経路へ同じく適用）に照らし、同じ入力・同じ上限で tree と VM の REPL の結果が違う期間を作らない。VM の分は小さく（`src/vm.rs` +5〜15 行、受入基準・仕組みの
    数は変わらない）、CLI の 3 本の test を `for use_vm` の同じ期待値で確かめられる（原則 9）。採らなかった案 B（tree REPL だけを直し、VM REPL は
    fix-execution-budget-usage-fresh に残す）は、スライスの境界を変えずに済むが、そのスライスが済むまで CLI の REPL の予算超過の観測が engine ごとに分かれ
    test の期待値も engine 別になる。差は逸脱一覧で追えるのでマニフェストに反するとまでは見ないが、いま揃っている tree・VM の REPL の観測を新たにずらすことになり、
    原則 5 に照らして A に劣る。

## 15. 受入基準

| ID | Phase | 基準 |
|---|---:|---|
| EMB-AT-01 | 1 | importなしpublic exampleがlow-level moduleなしでcompile→link→runできる |
| EMB-AT-02 | 1 | Compiled/Linkedは`Send+Sync`、Context/Handleは`!Send+!Sync`のcompile-time assertion |
| EMB-AT-03 | 1 | foreign CompiledScriptをlink時、foreign LinkedScript/contextをstart時に対応するmismatchで拒否しresolver/命令0 |
| EMB-AT-04 | 1 | 同contextの同時startをborrowまたはContextBusyで拒否 |
| EMB-AT-05 | 1/2 | compile時resolver/OS/callback 0、link時script/stdio 0、run時resolver 0。Phase 2でimportはlink時にresolver capabilityで解決（resolver未grantは terminal `Denied`） |
| EMB-AT-06 | 1 | Ready dropは命令0、context再利用可 |
| EMB-AT-07 | 1〜4 | InternalFailure以外のterminal後はframe cleanでcommit/rollback規則どおり再利用可、InternalFailureだけpoison |
| EMB-AT-08 | 1〜4 | Completed/Exitedだけ全language-state commit、caught error後のCompleted/Exitedもcommit、その他terminalは開始時点へrollback。完了済み外部I/Oは戻さずauditへ記録しtree/VM一致 |
| EMB-AT-09 | 1/2 | Phase 1でCompleted/Runtime/InternalFailure/pre-run Cancelled、Phase 2でExitedとlink/control-plane Denied/HostErrorを構造化terminal channelだけで返す |
| EMB-AT-10 | 2〜6 | 通常runtimeとscript操作中のcapability/host errorはcatch可。link/control-plane deny/host、budget/deadline/cancel/exit/panic/audit/record/replayはcatch不可 |
| EMB-AT-11 | 2 | exit(-1/256)はRuntimeError、0/255はExited、capabilityなしはcatch可能capability error、library process継続 |
| EMB-AT-12 | 1/3 | pre-cancelは命令0。Phase 3でloop/host call境界cancelを各1 terminalにする |
| EMB-AT-13 | 1/2 | Phase 1でroot hash、Phase 2でimport graph hashのgolden bytes/SHA-256を固定し1 byte変更で差が出る |
| EMB-AT-14 | 1/2 | fake secretがpublic error/Debugへ現れない |
| EMB-AT-15 | 1 | CLI引数0/1/複数/`--`/非UTF-8がAUD-018契約どおりで、Engine APIの`ExecutionRequest.arguments`へ転送される |
| EMB-AT-16 | 1〜3 | 実装済みoutcomeのCLI exit mappingをsubprocess完全一致検証 |
| EMB-AT-17 | 1/5 | tree/VM CLIがEngine APIだけを通る |
| EMB-AT-18 | N-1 | 旧sampleはdeprecation warning、新sampleはwarningなし |
| EMB-AT-19 | N | 旧sampleはcompile失敗、migration後成功 |
| EMB-AT-20 | N-1/N | compatibility wrapperでambient権限が復活しない |
| EMB-AT-21 | 3 | budget N-1/N成功、N+1 BudgetExceeded。deadline/cancelもterminal channelのみ |
| EMB-AT-22 | 4 | final `ExecutionState`全状態のpoll/pause/resume合法・不正matrix、slice N-1/N/N+1を検証 |
| EMB-AT-23 | 6 | `ExecutionStarted`から`Terminal`までのcanonical sequence、fail-closed、`BudgetUsage`完全性を検証 |

## 16. ロードマップ・監査項目との関係

- **Phase 0:** fault非保証、panic/abort責任分界は[脅威モデル](threat-model.md)。
- **Phase 1:** E1〜E6・E8a、EMB-AT-01〜09・13・15〜17を完了条件とする。E8aでCLIのtree/VM両経路をEngine APIだけへ統合し、基本引数転送を行う。現行tree-only facadeだけでは不足する。E8aの実装は、CLIのtree file/stdin実行のうちimportなしrootをEngine API（`compile`→`link`→`run`）だけへ統合し、`ExecutionRequest.arguments`で引数転送・`ExecutionOutcome`→exit code変換を第12節どおりに行うところまで。importを含むtree file/stdin rootはC6/E7でEngine API（`compile`→`link`→`run`）へ統合済み（`link`がresolver capabilityでimportを解決し、resolver未grant + importは terminal `Denied`）。tree REPL（入力間の状態継続とimport解決が必要）もE7でEngine API（`compile`→`link`→`run_for_repl`、`EmbeddingContext`再利用）へ統合済み。REPLのbudgetの差（loop:drift #429 D-131）はfix-repl-submission-budgetで、REPLのimportの差はfix-repl-import-gateで解消した。VM file/stdinのimport gateはE9でlink層のEngine resolver gate経由へ統合済み（未grant + importはtreeと同じくterminal `Denied`）だが、VM backend adapter本体（run経路のEngine統合）はE9残。したがってEMB-AT-17はtree経路（file/stdin・REPL、importの有無を問わず）とVM file/stdinのimport側について満たし、残り（VM backend adapter本体の統合とconformance）はE9で満たす。
- **Phase 2:** E7・E8b、EMB-AT-10/11/16、CAP-AT-23〜26と[capability model](capability-model.md)のPhase 2基準を完了する。E8bでcapability profile/optionsとsafe/legacy移行を接続する。この profile/options 配線（`--profile safe|legacy`・全 capability option・safe/legacy の frozen `CapabilitySet` 構築と tree/VM 両経路への注入）はC9/C10/E7/E11で統合済みで、CAP-AT-23〜26・exit mapping（EMB-AT-16）・catch 規則（EMB-AT-10/11）に相当する実装・テストが存在する。E8b 受入条件のうち **migration warning は正式に descope する（不実装）**: deprecation / migration warning は本来 N-1 段階（legacy 既定・移行促進）のためのもので、[capability model](capability-model.md) §14.4 の「N-1 を飛ばし safe 既定（N 相当）を採る」採択により本 revision では役割がない。legacy 選択時の既存 security warning（全アクセス許可の警告、§14.3）で十分とする。したがって E8b の Phase 2 受入は EMB-AT-10/11/16・CAP-AT-23〜26 とし、migration warning は受入条件から外す。EMB-AT-10/11 は下記の受入 ID ↔ test 対応どおり profile 経由 subprocess test で固定済みで、**E8b は完了**（profile/options 配線は統合済み、migration warning は descope 済み、EMB-AT-10/11 を profile 経由で固定済み）。ただし Phase 2 全体の完了は[capability model](capability-model.md)側の Phase 2 基準（CAP-AT 等）にも依存するため、本行は embedding 側 E8b の完了を述べるにとどめる。

  - **EMB-AT-10/11 の受入 ID ↔ test 対応（トレーサビリティ）**: catch 規則・exit 写像・range 写像は outcome 種別レベルを tree 生 `CapabilitySet` 経由の embedding 層 unit test で、CLI safe profile が組む frozen set 経由の挙動を tree/VM 両 engine の subprocess test で、それぞれ固定する。両者は補完関係にある（unit test は outcome 種別の正本、subprocess test は profile 配線を通した観測挙動の正本）。
    - **EMB-AT-10a**（通常 runtime の capability-deny が script の try/catch で catch 可能）: unit = `src/embedding.rs` の `c7_exit_ungranted_is_catchable` / `c3_env_ungranted_is_catchable` / `c3_now_ungranted_is_catchable`（tree 生 set、catch して Completed）。profile 経由 = `tests/cli_safe_profile.rs::safe_capability_deny_is_catchable_via_profile`（tree/VM、exit/env/now を try/catch で囲み `e["type"]` が `capability` で正常終了）。
    - **EMB-AT-10b**（未捕捉の capability-deny が RuntimeError outcome）: unit = `c7_exit_ungranted_is_runtime_error` / `c3_env_ungranted_is_runtime_error` / `c3_now_ungranted_is_runtime_error`（`ErrorKind::Capability`）。profile 経由 = `tests/cli_safe_profile.rs::safe_default_grants_only_stdout`・`tests/ambient_access_audit.rs::safe_default_denies_all_ambient_operations`（tree/VM、未捕捉 deny → 非ゼロ exit）。
    - **EMB-AT-10c**（link/control-plane deny は catch 不可）: `tests/engine_api.rs::link_error_surfaces_as_link_error_outcome`、`tests/cli_safe_profile.rs::relative_import_tree_requires_import_root` / `relative_import_vm_requires_import_root`（terminal Denied、exit 1、tree/VM）。
    - **EMB-AT-11a**（exit(0/255) が Exited、granted exit code が実 process exit code へ伝播）: unit = `c7_exit_boundary_codes_are_exited` / `c7_exit_granted_maps_to_exited`（Exited outcome）。profile 経由 = `tests/cli_safe_profile.rs::allow_exit_propagates_exit_code_via_profile`（tree/VM、`--allow-exit` + exit(0/42/255) → 実 process exit code 0/42/255）。exit mapping は `main.rs::exit_code_for_outcome` の `Exited { code } => code as i32`。
    - **EMB-AT-11b**（exit(-1/256) が RuntimeError 写像）: unit = `c7_exit_out_of_range_is_runtime_error`（`ErrorKind::Argument`）。profile 経由 = `tests/cli_safe_profile.rs::allow_exit_out_of_range_maps_to_runtime_error_via_profile`（tree/VM、`--allow-exit` + exit(256)/exit(0 - 1) → exit 1、process は panic せず終了）。
    - **EMB-AT-11c**（exit capability なしは catch 可能な capability error）: unit = `c7_exit_ungranted_is_runtime_error` + `c7_exit_ungranted_is_catchable`。profile 経由 = `tests/cli_safe_profile.rs::safe_capability_deny_is_catchable_via_profile`（exit(0) を含む）。
    - **EMB-AT-11d**（library process 継続）: unit = `c7_exit_does_not_poison_context`。profile 経由 = `tests/cli_safe_profile.rs::safe_capability_deny_is_catchable_via_profile`（catch 後に `print("alive")` が走る）、REPL = `tests/integration.rs::repl_continues_session_after_uncaught_error_in_both_engines`。
- **AUD-018:** process argvをExecutionRequest snapshotへ置換し、複数script引数とCLI syntaxを固定した。E8a/C6-cでtree file/stdin経路（importの有無を問わず）の引数転送を`EmbeddingRequest::with_arguments`→`Engine::run`経由へ載せ替えた。tree REPLはE7でEngine API（`compile`→`link`→`run_for_repl`、`EmbeddingContext`再利用）へ統合済み（REPLはscript引数を持たないため`with_arguments`は使わない）。
- **AUD-020:** Engineでpath文字列checkをせず、path-handle adapterへ委譲する。
- **AUD-049:** compile/link callable解決は単一catalogを使い、host registryを第4のbuiltin名一覧にしない。

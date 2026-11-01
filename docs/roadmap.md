# Tsumugi — ロードマップ

最終更新: 2026-11-01（今後の予定だけを持つ形に縮めた。次の作業と完了の履歴は持たない）

> このロードマップは「今後の予定」（マイルストーンと項目、その状態）だけを持つ。機能の詳細・採用/不採用の理由は各設計正本に置く
> （ドキュメントの役割分担は [ドキュメント運用ガイド](documentation-guide.md) を参照）。
> 項目の状態の唯一の正本は下記「ステータス一覧」表であり、他の節・表はこの表を参照する。
> 次に何をやるか（スライスの一覧・状態・着手順）は [`slices.toml`](slices.toml) が正本で、ここには書かない。
> 日々の進捗（何がいつ終わったか）は git log と各 spec の実装スライス小節にあり、ここには残さない。

## 現在地

- **フェーズ:** Phase 2（deny-by-default capability）・Phase 3/4（包括的な実行予算・協調実行）を並行して進行中。
- **次の作業:** [`slices.toml`](slices.toml) の `status = "ready"` のうち `order` が最小のスライス（選び方は[開発ループ手順書](loop-runbook.md) §6.2）。スライスごとの状態はロードマップに再掲しない。

## マイルストーン（Phase 0〜7）

目標アーキテクチャへの移行順序。各 Phase の状態は粗い 1 記号で、Phase 内の個別項目の状態は下記「ステータス一覧」を参照する（ここでは再掲しない）。現行の step 上限・collection 上限・深度制限・filesystem/env allow-list・構造化エラー・差分テストは土台として再利用するが、それだけで各 Phase が完了したとはみなさない。

| Phase | 目的 | 設計正本 | 状態 | 完了 gate |
|---|---|---|---|---|
| 0 | 保証範囲と脅威モデル | [脅威モデル](threat-model.md) | 🟡 | 第11節 Phase 0、TM-AT-07・TM-AT-12（host 部分） |
| 1 | 安定した組み込み API | [組み込みAPI仕様](embedding-api.md) | 🟡 | 第14〜15節 E1〜E6・E8a、EMB-AT-01〜09・13・15〜17 |
| 2 | deny-by-default capability | [Capability Model仕様](capability-model.md)・[組み込みAPI仕様](embedding-api.md) | 🟡 | Capability 第18〜19節 C1〜C10・Phase 2 CAP-AT、Embedding E7〔tree REPL 統合済み〕・E8b〔完了：profile/options 配線済み・migration warning descope 済み・EMB-AT-10/11 を profile 経由 subprocess で固定済み〕。embedding 側 E7・E8b は完了、Phase 2 全体の状態は capability model 側 Phase 2 基準（CAP-AT 等）にも依存 |
| 3 | 包括的な実行予算 | [実行予算・協調実行仕様](execution-control.md) | 🟡 | 第3〜8・10・15.1〜15.3・16節、EMB-AT-21 |
| 4 | 協調実行と負荷制御 | [実行予算・協調実行仕様](execution-control.md) | 🟡 | 第6.2・9・11〜12・15.3〜15.5・16節、EMB-AT-22 |
| 5 | 規範意味論と決定的境界 | [決定性・実行時監査仕様](determinism-and-audit.md)・[次期意味論・実装決定](semantic-decisions.md) | 🟡 | Determinism 第2〜6・14節 Slice 1〜4/6・15.1/15.2/15.5・16節 |
| 6 | 実行時監査 | [決定性・実行時監査仕様](determinism-and-audit.md) | 🟡 | 第7〜12・14節 Slice 5・15.3/15.4・16節、EMB-AT-23・CAP-AT-30 |
| 7 | 運用保証と検証 | [検証・リリース・運用設計](verification-release-operations.md) | 🟡 | 第6節 matrix、第17節 VRO-AT-01〜15、第18節 |

> 長時間処理について「確実に終わる」とは、任意の script の成功を保証することではない。ホストを不安定にせず、完了・停止・失敗を観測可能な結果として扱い、有限の処理を設定された負荷の中で着実に進められることを目標とする。
>
> **OS 対応の保証レベル:** 最終的に OS 非依存を目指すが、Windows の機能完全性の保証は release（Phase 7）の対象とし、開発フェーズ（alpha、Phase 1〜6）では必須としない。ただし開発フェーズでも Windows CI による退行検出とセキュリティ境界（capability / secure resolution / sandbox）の Windows 健全性は維持する。正本は [検証・リリース・運用設計](verification-release-operations.md) §4.1.1。

## ステータス一覧（状態の唯一の正本）

未完了（🟡 部分実装 / ⬜ 未実装）と、矛盾解消のため明示する完了済み（✅）項目を AUD・REV 横断で並べる。ここが全項目の状態の唯一の正本であり、他の節・表はこの表を参照する。✅ 完了項目の実装詳細は各 owning spec を参照する。深層監査（AUD-001〜050）の大半は実装済みで、ここに載らない AUD は完了（✅）とみなしてよい。

| ID | 一行概要 | 状態 | owning spec |
|---|---|---|---|
| REV-001 | 共有 DAG の比較・表示・join・sort を有限化（指数化対策は完結、全順序化は REV-017 で実装済み・revision 0.21。残りは包括 budget 側の Phase 6 伝播で、うち `BudgetCharged`・host-call lifecycle 3 event・yield/resume 2 event emission は実装済み） | 🟡 | [実行予算・協調実行仕様](execution-control.md) |
| REV-015 | source/string/heap/I-O/bulk work の包括 budget・deadline・cancel（budget 本体は tree/VM 完了。fuel の課金点は F-1 で tree/VM を揃えた。§4.1 課金表の主要 charge point への移行〔F-2〕は設計確定〔PR #241〕だが前段 `tree-expr-continuation`〔式レベル continuation・Slice 3 PR-e、e1〜e4 に分割〕待ち（e1・e2a・e2b・e3 は done、e4 は設計済み todo）、CAP-AT-17 の source/import 本数 N 境界受入試験は tree 経路で完了。残りは Phase 6 への伝播。うち `BudgetCharged`・host-call lifecycle 3 event・yield/resume 2 event（`Yielded` / `Resumed`）emission は tree backend で実装済み） | 🟡 | [実行予算・協調実行仕様](execution-control.md) |
| REV-007 | `ExecutionContext` の session state と run meter を request ごとに分離 | ⬜ | [実行予算・協調実行仕様](execution-control.md) |
| REV-008 | 全 stable execution へ transaction を適用（REPL は AUD-024 で完了） | 🟡 | [組み込みAPI仕様](embedding-api.md)・[実行予算・協調実行仕様](execution-control.md) |
| REV-010 | 32-bit target の `i64 as usize` wrap を `try_from`/checked へ | ⬜ | [実行予算・協調実行仕様](execution-control.md) |
| REV-011 | language revision・engine 差・status を機械可読な単一正本から生成 | ⬜ | — |
| REV-012 | call 評価順の status / doc drift 解消（意味論正本は第5節＝AUD-017）。`depth-counting`（128 user frame の統一）: 済み。`callee-precedence`（callee 先行への precedence 変更）: 未着手。`src/limits.rs` の doc の整理は fix-limits-call-order-doc | 🟡 | [次期意味論・実装決定](semantic-decisions.md) §17.5 |
| REV-014 | sandbox/env/limits/stdio/clock を `EngineConfig`/`ExecutionRequest` へ移す | ⬜ | [Capability Model仕様](capability-model.md)・[組み込みAPI仕様](embedding-api.md) |
| REV-016 | script からの Rc cycle による回収不能 heap（baseline 課金・clear・tenant 分離） | ⬜ | [設計](design.md) |
| REV-017 | HumanDisplay/CanonicalRepr/TotalOrder の分離による sort/list_dir 全順序化（設計確定・実装済み、revision 0.21、REV-001 と連動） | ✅ | [次期意味論・実装決定](semantic-decisions.md) §17.8 |
| REV-019 | `now()` の epoch 前/clock error と `u64 as i64` を clock capability + checked で扱う | ⬜ | [Capability Model仕様](capability-model.md)・[実行予算・協調実行仕様](execution-control.md) |
| REV-022 | safe profile で denial と OS 失敗を区別（EOF/I-O error を Null/false へ畳まない）。outcome の型の形の差は組み込みAPI仕様の逸脱一覧（§8） | ✅ | [Capability Model仕様](capability-model.md)・[組み込みAPI仕様](embedding-api.md) |
| REV-024 | MSRV 固定・action pin・release/fuzz/stress gate の段階導入 | 🟡 | [検証・リリース・運用設計](verification-release-operations.md) |
| REV-025 | parse diagnostic 件数の上限 | ⬜ | [次期意味論・実装決定](semantic-decisions.md) |
| AUD-018 | CLI script 引数を `ExecutionRequest.arguments` snapshot へ（profile/options は E8b。profile/options 配線は C9/C10/E7/E11 で統合済み、migration warning は descope 済み、EMB-AT-10/11 は profile 経由 subprocess で固定済み——E8b 完了） | ✅ | [次期意味論・実装決定](semantic-decisions.md) §6・[組み込みAPI仕様](embedding-api.md) 第12・14〜16節 |
| AUD-020 | sandbox の TOCTOU/oracle/dangling symlink を path-handle で受入試験化 | 🟡 | [脅威モデル](threat-model.md) TM-002〜004・[Capability Model仕様](capability-model.md) §8・CAP-AT-10〜14 |
| AUD-021 | language-spec / LANG_GUIDE / design の drift 解消 | 🟡 | [言語仕様](language-spec.md)・[設計](design.md) |
| AUD-022 | REPL/differential/limit/defensive に加え fuzz・stress・failure injection gate | 🟡 | [検証・リリース・運用設計](verification-release-operations.md) §4〜6・17節 |
| AUD-036 | lossy 数値変換（Float→Int・file_size・exit。`exit` の `Exited { code, usage }` 化は C7 で完了） | ✅ | [次期意味論・実装決定](semantic-decisions.md) §10 |
| AUD-045 | MSRV 1.97・stable/MSRV CI・install/release・artifact・署名/SBOM/OCI・運用 gate | 🟡 | [検証・リリース・運用設計](verification-release-operations.md) §3〜10・17〜18節 |
| AUD-048 | 動的評価ごとの per-instance `FunctionId` による関数等価性 | ✅ | [次期意味論・実装決定](semantic-decisions.md) §12・[決定性・実行時監査仕様](determinism-and-audit.md) §4.7 |
| Phase 5 残 | deterministic shared helpers（NaN canonicalization `0x7ff8_0000_0000_0000`・Float format）・DeterminismRulesRevision/ModuleId 正規化・StableError（`src/stable_error.rs`、tree 射影）は完了。Slice 1 の deterministic primitive は一通り揃った。残りは Slice 2〜6（HostBoundary/FakeHost・VM parity・audit normalize・record/replay 等） | 🟡 | [決定性・実行時監査仕様](determinism-and-audit.md) 第14節 Slice 1〜6 |
| Phase 6 残 | audit drain queue の engine 全体 16 MiB 上限（§10.2、E10 統合後。per-execution 上限は B-2 で実装済み）・strict CBOR〔c2〜c4〕・record/replay・VM audit triple 対応・sink 必須化・fs builtin 全体の監査範囲〔rename ほか非課金 fs builtin への監査 triple 拡張〕・`AdmissionQueued`/`SchedulerPreempted`/`HostPaused` の yield emission（engine-poll 経路に sink が無い）（A-1 ＋ `BudgetCharged` ＋ host-call lifecycle 3 event `CapabilityDecision` / `HostCallStarted` / `HostCallFinished` ＋ yield/resume 2 event `Yielded` / `Resumed` emission ＋ `AuditBackpressure`-yield 配線〔B-1、sink-Pending 由来〕＋ audit drain queue byte 会計〔B-2、per-execution〕＋ redaction 本体〔R-1a・tree engine・opt-in・破壊的変更〔`AuditPayload`/`AuditField` 差し替え〕を alpha として受容・`language_revision` bump なし〕＋ body allow-list Reveal〔R-1b・tree engine・opt-in・非破壊〕＋ sink callback の再入検出〔§10、facade の 6 入口・fix-audit-sink-reentrancy-guard〕は tree backend で実装済み。strict CBOR の encoder 本体〔c1・crate 内部・未配線〕も実装済み） | 🟡 | [決定性・実行時監査仕様](determinism-and-audit.md) §6〜12・14 Slice 5 |
| 品質改善 | 未着手の処理改善（`dispatch` の match 分割・`exec_stmt` 分割・sandbox `OnceLock` テスタビリティ・fuzzing 導入） | ⬜ | [設計](design.md) |

## 今後の予定（フェーズ単位の優先順）

先頭ほど優先。各ステップの状態は上記「ステータス一覧」の対応 ID を参照する（ここでは再掲しない）。スライス単位の着手順は [`slices.toml`](slices.toml) の `order` が正本である。

1. **包括 budget の残り（REV-001/REV-015、Phase 3/4）** — 残 I/O/host budget と Phase 6 audit への budget/event 伝播。budget 本体は tree/VM 完了で、Phase 6 伝播のうち `BudgetCharged`・host-call lifecycle の 3 event（`CapabilityDecision` / `HostCallStarted` / `HostCallFinished`）・yield/resume lifecycle の 2 event（`Yielded` / `Resumed`）・`AuditBackpressure`-yield 配線（§10.2、B-1、sink-Pending 由来）は tree backend で実装済み。これで host-call + yield/resume + backpressure の audit 伝播は完了（tree-only・opt-in・非破壊）。B-2 の audit drain queue byte 会計（§10.2、per-execution）も実装済みで、残るのは VM 対応（別フェーズ）。REV-001 の全順序化（REV-017）は revision 0.21 で実装済み（§17.8）。正本: [実行予算・協調実行仕様](execution-control.md)、全順序化の設計は [次期意味論・実装決定](semantic-decisions.md) §17.8。
2. **Phase 1/2 の残り** — E8b（capability profile/options の safe/legacy 移行）は完了。profile/options 配線は C9/C10/E7/E11 で統合済み、migration warning は正式に descope 済み（理由は [Capability Model仕様](capability-model.md) §14.4）、EMB-AT-10/11 は profile 経由 subprocess test で固定済み。これで embedding 側の Phase 1/2（E7・E8b）が揃った。VM/E9 の import・resolver 統合（Phase 5）・tree REPL の Engine API 統合（E7 残）・CAP-AT-17 の budget N 境界（Phase 3/C11）は完了。VM/E9 の import・resolver 統合は VM file/stdin の link 層 capability gate のみ done で、E9 の adapter/conformance（charge parity 他）・VM stable 化は別途残る（Phase 5／着手順 3 側）。Phase 2 全体の完了可否は capability model 側の Phase 2 基準（CAP-AT 等）にも依存する（「ステータス一覧」を正とする）。正本: [組み込みAPI仕様](embedding-api.md)・[Capability Model仕様](capability-model.md)。
3. **Phase 6 audit の残り** — **R-1 redaction R-1a** は shipped（既定 policy の redaction tree を tree engine の監査付き host-call 経路へ配線・opt-in・破壊的変更を alpha として受容・正本は [決定性・実行時監査仕様](determinism-and-audit.md) §14「R-1 実装スライス」）。**R-1b（body allow-list Reveal）** も shipped。strict CBOR は c1（encoder 本体・未配線）が shipped で、c2〜c4 が残る。以降は audit drain queue の engine 全体上限（§10.2、E10 統合後）・strict CBOR〔c2〜c4〕・record/replay・VM audit triple 対応・sink 必須化ほか（`Yielded` / `Resumed` emission ＋ `AuditBackpressure`-yield 配線〔B-1〕＋ audit drain queue byte 会計〔B-2〕＋ redaction 本体〔R-1a〕＋ body allow-list Reveal〔R-1b〕は shipped）。正本: [決定性・実行時監査仕様](determinism-and-audit.md)。
4. **Phase 7 運用保証** — MSRV・release/install・artifact・OCI・運用資材。正本: [検証・リリース・運用設計](verification-release-operations.md)。

> 設計 slice に沿う詳細な実装順は各 spec（特に [次期意味論・実装決定](semantic-decisions.md) 第17節「移行順」と [実行予算・協調実行仕様](execution-control.md) 第14節 Slice 定義）を正本とする。

## 実装済み（完了マイルストーンの粗い一覧）

完了した update-image の粗い進捗一覧。各項目の実装詳細は owning spec を参照する。

- [x] 言語中核: 基本型・変数・演算・制御フロー・関数・第一級関数/lambda・クロージャ・List/Dict・index・f-string・try/catch・構造化エラー・module/import
- [x] 組み込み関数（print, len, push, pop, keys, type, slice, contains, split, join, to_int, to_str, range ほか）
- [x] ファイル I/O（read_file, read_lines, write_file, append_file）とサンドボックス
- [x] REPL（複数行入力）・行番号付きエラー・スタックトレース
- [x] バイトコード VM（Phase 0〜8）: OpCode/Chunk/Compiler/VM、変数、制御フロー、関数、クロージャ、組み込み関数 54 個、互換性修正、IEEE 754 統一
- [x] bytecode 検証・API 封印（REV-006/004/005/018）: `VerifiedChunk`/verifier、per-instruction step 課金、`MakeClosure(proto_index)`+`CaptureDesc`、`patch_jump` fallible 化、`unstable-bytecode` feature 封印
- [x] ステップ予算（ループ反復 + 関数呼び出しのカウント、無限ループ/再帰防止、tree/VM 両対応）→ 詳細は [実行予算・協調実行仕様](execution-control.md)
- [x] 深層監査 AUD-001〜050 の大半（REPL transaction・collection 上限・深度制限・canonical error・COW collection・FunctionId ほか）→ 状態は「ステータス一覧」、詳細は各 spec
- [x] Phase 6 実行時監査 A-1（schema v1・8 event 型・logical journal・`AuditSink`・fail-closed・opt-in、tree-only）＋ `BudgetCharged` emission ＋ host-call lifecycle 3 event emission（`CapabilityDecision` / `HostCallStarted` / `HostCallFinished`、§7.2/§8/§11）＋ yield/resume lifecycle 2 event emission（`Yielded` / `Resumed`、§7.2/§8 規則7/8/9、tree backend、opt-in・非破壊）→ 残件は「ステータス一覧」Phase 6 残

> 歴史的なベンチマーク測定値・検証スナップショットは [`docs/history/benchmarks.md`](history/benchmarks.md) に退避した。

## 将来機能（設計済み・未着手）

- **class（継承なし）:** 設計済み・低優先度。基盤と総 heap budget 後の実装候補 → [次期意味論・実装決定](semantic-decisions.md) 第15節
- **HTTP:** 設計済み・着手禁止（具体ユースケース承認待ち）。Phase 1〜6 完了と着手 gate 承認後だけ別計画を作る → [次期意味論・実装決定](semantic-decisions.md) 第16節
- **表示レイヤ i18n:** 現時点で見送り → [次期意味論・実装決定](semantic-decisions.md) §3.12

## 設計方針: 言語中核とホスト機能の境界

言語中核には外部状態へ触れない純粋な計算（文字列・数値・List/Dict・型変換）を置き、filesystem・環境変数・時刻・標準入出力・process・network・業務操作などの外部効果は、ホストが実行単位で明示的に付与する capability または host function として提供する。この境界の目標・現行実装からの移行表・採用理由（最小権限・資源制御・監査可能性・予測可能性・依存分離・テスト容易性）・外部機能の追加順序は [マニフェスト](manifesto.md) 原則 2〜8 と各 spec（[組み込みAPI仕様](embedding-api.md)・[Capability Model仕様](capability-model.md)・[実行予算・協調実行仕様](execution-control.md)・[決定性・実行時監査仕様](determinism-and-audit.md)）を正本とする。

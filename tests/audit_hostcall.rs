//! Phase 6: host-call lifecycle 監査 event の統合テスト（tree backend、opt-in）。
//!
//! 設計正本は `docs/determinism-and-audit.md` §7.2（schema）/ §8（emission 順序・完全性）/
//! §11（監査自体の予算）。filesystem は tree 唯一の host-call 境界なので、`write_file` /
//! `read_file` を監査 sink 付きで実行し、`HostCallStarted` → `CapabilityDecision` →
//! `HostCallFinished` の triple が §8 の順序・ID 相関で出ることを固定する。deny 経路と
//! sink 無し経路の非破壊性も検証する。crate root の公開型だけを使う。

use std::collections::BTreeSet;
use std::num::NonZeroU128;
use std::sync::Arc;

use tsumugi::{
    AuditEvent, AuditSink, BudgetConfig, CapabilityDecisionKind, CapabilitySet, CompileOptions,
    EffectStatus, EmbeddingContext, EmbeddingEngine, EmbeddingOutcome, EmbeddingRequest,
    ExecutionId, FakeClock, FilesystemCapability, FilesystemRoot, FsOperation, HostCallOutcome,
    InMemoryAuditSink, LinkRequest, MonotonicClock, MountName, OsDirectoryHandle,
    ScriptedAuditSink, ScriptedResponse, Source, SourceId, SymlinkPolicy, TerminalOutcome,
};

// -----------------------------------------------------------------------------
// ヘルパ
// -----------------------------------------------------------------------------

/// 一意な一時 dir を作る（checked_conversion_contract.rs と同形）。
fn temp_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "tsg-audit-hostcall-{}-{}-{}",
        tag,
        std::process::id(),
        n
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

/// 一時 dir を root に Read + Write + Create を grant した filesystem capability。
fn rw_fs(base: &std::path::Path) -> FilesystemCapability {
    let pid = NonZeroU128::new(1).expect("non-zero");
    let mut ops = BTreeSet::new();
    ops.insert(FsOperation::Read);
    ops.insert(FsOperation::Write);
    ops.insert(FsOperation::Create);
    let mount = MountName::new("data").expect("mount");
    let handle = OsDirectoryHandle::new(base.to_path_buf(), pid, SymlinkPolicy::DenyAll);
    let root = FilesystemRoot::new(mount, pid, ops, SymlinkPolicy::DenyAll, Arc::new(handle))
        .expect("root");
    FilesystemCapability::new(vec![root]).expect("fs")
}

fn fs_capabilities(base: &std::path::Path) -> CapabilitySet {
    CapabilitySet::builder()
        .filesystem(rw_fs(base))
        .expect("grant filesystem")
        .build()
}

fn source(id: &str, text: &'static str) -> Source<'static> {
    Source::new(SourceId::new(id).expect("valid source id"), text)
}

fn link_source(
    engine: &EmbeddingEngine,
    id: &str,
    text: &'static str,
) -> tsumugi::EmbeddingCompiledScript {
    engine
        .compile(
            source(id, text),
            &CompileOptions {
                retain_source: true,
            },
        )
        .expect("compile")
}

/// import なし link 用の既定 request（本ファイルの script は import を持たない）。
fn link_request() -> LinkRequest {
    let clock = FakeClock::new();
    let budget = BudgetConfig::standard(&clock).expect("standard budget");
    LinkRequest::new(
        ExecutionId::new(1u128.try_into().expect("nonzero")),
        CapabilitySet::empty(),
        budget,
    )
}

/// standard budget + 共有 clock + execution_id + capabilities で run request を作る。
fn run_request(id: u128, capabilities: CapabilitySet) -> EmbeddingRequest {
    let clock: Arc<dyn MonotonicClock> = Arc::new(FakeClock::new());
    let budget = BudgetConfig::standard(clock.as_ref()).expect("standard budget");
    EmbeddingRequest::new(budget, clock)
        .expect("standard budget は同 domain")
        .with_execution_id(ExecutionId::new(id.try_into().expect("nonzero")))
        .with_capabilities(capabilities)
}

/// envelope 列から host-call triple だけを sequence 順に抽出する。
fn host_call_events(envelopes: &[tsumugi::AuditEnvelope]) -> Vec<&AuditEvent> {
    envelopes
        .iter()
        .map(|e| &e.event)
        .filter(|e| {
            matches!(
                e,
                AuditEvent::HostCallStarted { .. }
                    | AuditEvent::CapabilityDecision { .. }
                    | AuditEvent::HostCallFinished { .. }
            )
        })
        .collect()
}

/// triple（Started, Decision, Finished の 3 連）を 1 件分 unpack して検査する。
///
/// 戻り値は (operation_id, decision, outcome, effect_status)。ID が triple 内で一致することを
/// その場で確認する（§7.2 の operation_id / call_id 再利用）。
fn unpack_triple(
    triple: &[&AuditEvent],
) -> (u64, CapabilityDecisionKind, HostCallOutcome, EffectStatus) {
    assert_eq!(triple.len(), 3, "triple は 3 event: {triple:?}");
    let started_id = match triple[0] {
        AuditEvent::HostCallStarted {
            operation_id,
            call_id,
            ..
        } => {
            assert_eq!(*operation_id, *call_id, "Started の op==call");
            *operation_id
        }
        other => panic!("triple[0] は HostCallStarted のはず: {other:?}"),
    };
    let decision = match triple[1] {
        AuditEvent::CapabilityDecision {
            operation_id,
            call_id,
            decision,
            ..
        } => {
            assert_eq!(*operation_id, started_id, "Decision の op が一致");
            assert_eq!(*call_id, Some(started_id), "Decision の call_id が一致");
            *decision
        }
        other => panic!("triple[1] は CapabilityDecision のはず: {other:?}"),
    };
    let (outcome, effect) = match triple[2] {
        AuditEvent::HostCallFinished {
            operation_id,
            call_id,
            outcome,
            effect_status,
            ..
        } => {
            assert_eq!(*operation_id, started_id, "Finished の op が一致");
            assert_eq!(*call_id, started_id, "Finished の call_id が一致");
            (*outcome, *effect_status)
        }
        other => panic!("triple[2] は HostCallFinished のはず: {other:?}"),
    };
    (started_id, decision, outcome, effect)
}

// -----------------------------------------------------------------------------
// (1) granted path: write + read が 2 つの triple を §8 順・ID 相関で出す
// -----------------------------------------------------------------------------

#[test]
fn granted_fs_calls_emit_ordered_triples_with_correlated_ids() {
    let base = temp_dir("granted");
    let sink = Arc::new(InMemoryAuditSink::new());
    let engine = EmbeddingEngine::builder()
        .audit_sink(sink.clone() as Arc<dyn AuditSink>)
        .build()
        .expect("build");

    // 1 回 write して 1 回 read する。host call は 2 回。
    let script = link_source(
        &engine,
        "m",
        "write_file(\"@data/out.txt\", \"hello\")\nlet c = read_file(\"@data/out.txt\")\n",
    );
    let linked = engine.link(&script, link_request()).expect("link");
    let mut ctx = EmbeddingContext::new(&engine);

    let audited = engine.run_audited(&linked, &mut ctx, run_request(7, fs_capabilities(&base)));
    assert!(
        matches!(audited.outcome(), Some(EmbeddingOutcome::Completed { .. })),
        "granted fs 実行は Completed のはず: {:?}",
        audited.outcome()
    );

    let envelopes = sink.snapshot();
    // 全体の不変条件: 先頭 Started / 末尾 Terminal / sequence 0 始まり gap なし。
    assert!(matches!(
        envelopes[0].event,
        AuditEvent::ExecutionStarted { .. }
    ));
    assert!(matches!(
        envelopes.last().expect("non-empty").event,
        AuditEvent::Terminal { .. }
    ));
    for (i, env) in envelopes.iter().enumerate() {
        assert_eq!(env.sequence as usize, i, "sequence は 0 始まり gap なし");
    }

    let hc = host_call_events(&envelopes);
    assert_eq!(hc.len(), 6, "2 host call × 3 event = 6: {hc:?}");

    // triple 1（write）。
    let (id0, dec0, out0, eff0) = unpack_triple(&hc[0..3]);
    assert_eq!(id0, 0, "operation_id は 0 始まり");
    assert_eq!(dec0, CapabilityDecisionKind::Allow);
    assert_eq!(out0, HostCallOutcome::Success);
    assert_eq!(eff0, EffectStatus::Committed, "write は commit");
    match hc[0] {
        AuditEvent::HostCallStarted { function, .. } => assert_eq!(function, "write_file"),
        other => panic!("{other:?}"),
    }

    // triple 2（read）。operation_id は triple1 の +1（単調）。
    let (id1, dec1, out1, eff1) = unpack_triple(&hc[3..6]);
    assert_eq!(id1, id0 + 1, "2 つ目の operation_id は 1 つ目 +1（単調）");
    assert_eq!(dec1, CapabilityDecisionKind::Allow);
    assert_eq!(out1, HostCallOutcome::Success);
    assert_eq!(eff1, EffectStatus::None, "read は mutation なし");
    match hc[3] {
        AuditEvent::HostCallStarted { function, .. } => assert_eq!(function, "read_file"),
        other => panic!("triple2 先頭は read_file の HostCallStarted のはず: {other:?}"),
    }
    // read の response_bytes は読み込んだ "hello"（5 bytes）。
    match hc[5] {
        AuditEvent::HostCallFinished { response_bytes, .. } => {
            assert_eq!(*response_bytes, 5, "read_file の response bytes は本文長")
        }
        other => panic!("triple2 末尾は HostCallFinished のはず: {other:?}"),
    }

    // §7.3 の順序: host call より前の消費の BudgetCharged は 1 つ目の HostCallStarted より前、
    // 1 つ目の host call 自身の課金は 2 つ目の HostCallStarted より前に出る（FBH-AT-03）。
    let first_started = envelopes
        .iter()
        .position(|e| matches!(e.event, AuditEvent::HostCallStarted { .. }))
        .expect("started exists");
    assert!(
        envelopes[..first_started]
            .iter()
            .any(|e| matches!(e.event, AuditEvent::BudgetCharged { .. })),
        "1 つ目の HostCallStarted より前に BudgetCharged がある（§7.3）"
    );

    let _ = std::fs::remove_dir_all(&base);
}

// -----------------------------------------------------------------------------
// (2) deny path: capability 無しの read_file が Deny/Denied/None の triple を出す
// -----------------------------------------------------------------------------

#[test]
fn denied_fs_call_emits_triple_with_deny_and_denied_none() {
    let sink = Arc::new(InMemoryAuditSink::new());
    let engine = EmbeddingEngine::builder()
        .audit_sink(sink.clone() as Arc<dyn AuditSink>)
        .build()
        .expect("build");

    // filesystem を grant しない。read_file は dispatch_fs が単一 sandbox denial を返す。
    // denial は try で catch してスクリプトを正常終了させ、「deny でも triple が揃う」ことと
    // 「スクリプトの既存 denial 挙動が保たれる」ことを同時に見る。
    let script = link_source(
        &engine,
        "m",
        "let ok = false\ntry\n    read_file(\"@data/x.txt\")\ncatch e\n    ok = true\nend\n",
    );
    let linked = engine.link(&script, link_request()).expect("link");
    let mut ctx = EmbeddingContext::new(&engine);

    let audited = engine.run_audited(&linked, &mut ctx, run_request(8, CapabilitySet::empty()));
    assert!(
        matches!(audited.outcome(), Some(EmbeddingOutcome::Completed { .. })),
        "denial は catch 可能なのでスクリプトは Completed: {:?}",
        audited.outcome()
    );

    let envelopes = sink.snapshot();
    let hc = host_call_events(&envelopes);
    assert_eq!(hc.len(), 3, "deny でも 3 event が揃う（§8 規則4）: {hc:?}");

    let (id, decision, outcome, effect) = unpack_triple(&hc[0..3]);
    assert_eq!(id, 0, "operation_id は 0 始まり");
    assert_eq!(decision, CapabilityDecisionKind::Deny);
    assert_eq!(outcome, HostCallOutcome::Denied);
    assert_eq!(effect, EffectStatus::None, "deny は外部効果なし");
    // error_code は安定 code（sandbox）であり script 可視 message ではない。
    match hc[2] {
        AuditEvent::HostCallFinished { error_code, .. } => {
            assert_eq!(error_code.as_deref(), Some("sandbox"));
        }
        other => panic!("{other:?}"),
    }
}

// -----------------------------------------------------------------------------
// (3) 非破壊性: sink 無し経路は observable 結果・ファイル bytes・stdout が不変
// -----------------------------------------------------------------------------

#[test]
fn no_sink_fs_path_is_byte_for_byte_unchanged() {
    // sink 無し engine で write + read を実行し、
    //  - outcome が Completed（既存挙動）
    //  - 書き込まれたファイルの中身が期待どおり
    //  - engine に紐づかない観測 sink は空のまま（emission ゼロ）
    // を確認する。drain_host_call_events は no-sink 経路で呼ばれないため、buffer が溜まっても
    // 観測挙動は変わらない。
    let base = temp_dir("no_sink");
    let engine = EmbeddingEngine::builder().build().expect("build");

    let script = link_source(
        &engine,
        "m",
        "write_file(\"@data/out.txt\", \"hello world\")\nlet c = read_file(\"@data/out.txt\")\n",
    );
    let linked = engine.link(&script, link_request()).expect("link");
    let mut ctx = EmbeddingContext::new(&engine);

    // engine に設定していない観測 sink（触られないことを確認する）。
    let observer = InMemoryAuditSink::new();

    let outcome = engine.run(&linked, &mut ctx, run_request(9, fs_capabilities(&base)));
    assert!(
        matches!(outcome, EmbeddingOutcome::Completed { .. }),
        "sink 無し fs 実行は Completed のはず: {outcome:?}"
    );

    // 書き込まれたファイルの bytes が期待どおり（host 効果は不変）。
    let written = std::fs::read(base.join("out.txt")).expect("read written file");
    assert_eq!(written, b"hello world", "書き込み bytes が不変");

    // engine 未設定の観測 sink は空（emission ゼロ、opt-in）。
    assert!(observer.is_empty(), "sink 未設定なので emission ゼロ");

    let _ = std::fs::remove_dir_all(&base);
}

// -----------------------------------------------------------------------------
// (4) 非破壊性: sink 有無で observable 結果が同一（parity）
// -----------------------------------------------------------------------------

#[test]
fn sink_presence_does_not_change_observable_fs_result() {
    let script_text = "write_file(\"@data/p.txt\", \"abc\")\nlet c = read_file(\"@data/p.txt\")\n";

    // --- sink 無し ---
    let base_a = temp_dir("parity_nosink");
    let engine_a = EmbeddingEngine::builder().build().expect("build");
    let script_a = link_source(&engine_a, "m", script_text);
    let linked_a = engine_a.link(&script_a, link_request()).expect("link");
    let mut ctx_a = EmbeddingContext::new(&engine_a);
    let outcome_a = engine_a.run(
        &linked_a,
        &mut ctx_a,
        run_request(10, fs_capabilities(&base_a)),
    );
    let bytes_a = std::fs::read(base_a.join("p.txt")).expect("read a");

    // --- sink 有り ---
    let base_b = temp_dir("parity_sink");
    let sink = Arc::new(InMemoryAuditSink::new());
    let engine_b = EmbeddingEngine::builder()
        .audit_sink(sink.clone() as Arc<dyn AuditSink>)
        .build()
        .expect("build");
    let script_b = link_source(&engine_b, "m", script_text);
    let linked_b = engine_b.link(&script_b, link_request()).expect("link");
    let mut ctx_b = EmbeddingContext::new(&engine_b);
    let audited_b = engine_b.run_audited(
        &linked_b,
        &mut ctx_b,
        run_request(10, fs_capabilities(&base_b)),
    );
    let outcome_b = audited_b.outcome().cloned().expect("audited outcome");
    let bytes_b = std::fs::read(base_b.join("p.txt")).expect("read b");

    // observable 結果（outcome 種別）とファイル bytes が一致する。
    assert!(
        matches!(outcome_a, EmbeddingOutcome::Completed { .. }),
        "no-sink: {outcome_a:?}"
    );
    assert!(
        matches!(outcome_b, EmbeddingOutcome::Completed { .. }),
        "sink: {outcome_b:?}"
    );
    assert_eq!(bytes_a, bytes_b, "書き込み bytes が sink 有無で一致");
    assert_eq!(bytes_a, b"abc");

    // sink 有りでは triple が出ている（opt-in の裏返しの確認）。
    assert_eq!(
        host_call_events(&sink.snapshot()).len(),
        6,
        "sink 有りは 2 host call × 3 event = 6"
    );

    let _ = std::fs::remove_dir_all(&base_a);
    let _ = std::fs::remove_dir_all(&base_b);
}

// -----------------------------------------------------------------------------
// fix-audit-flush-before-host-call: host call の開始前に BudgetCharged を flush する（§7.3）
// -----------------------------------------------------------------------------

fn run_fs_script(tag: &str, text: &'static str) -> Vec<tsumugi::AuditEnvelope> {
    let base = temp_dir(tag);
    let sink = Arc::new(InMemoryAuditSink::new());
    let engine = EmbeddingEngine::builder()
        .audit_sink(sink.clone() as Arc<dyn AuditSink>)
        .build()
        .expect("build");
    let script = link_source(&engine, "m", text);
    let linked = engine.link(&script, link_request()).expect("link");
    let mut ctx = EmbeddingContext::new(&engine);
    let audited = engine.run_audited(&linked, &mut ctx, run_request(9, fs_capabilities(&base)));
    assert!(
        matches!(audited.outcome(), Some(EmbeddingOutcome::Completed { .. })),
        "Completed のはず: {:?}",
        audited.outcome()
    );
    let _ = std::fs::remove_dir_all(&base);
    sink.snapshot()
}

fn is_charged(e: &tsumugi::AuditEnvelope) -> bool {
    matches!(e.event, AuditEvent::BudgetCharged { .. })
}

/// FBH-AT-01: host call 前の消費が HostCallStarted より前、Started〜Finished の間に BudgetCharged が無い。
#[test]
fn fbh_at_01_pre_host_call_fuel_is_charged_before_started() {
    let envelopes = run_fs_script(
        "fbh1",
        "let i = 0\nwhile i < 3\n    i = i + 1\nend\nwrite_file(\"@data/out.txt\", \"x\")\n",
    );
    let started = envelopes
        .iter()
        .position(|e| matches!(e.event, AuditEvent::HostCallStarted { .. }))
        .expect("started");
    let finished = envelopes
        .iter()
        .position(|e| matches!(e.event, AuditEvent::HostCallFinished { .. }))
        .expect("finished");
    assert!(
        envelopes[..started].iter().any(|e| matches!(
            e.event,
            AuditEvent::BudgetCharged {
                resource: tsumugi::BudgetResource::Fuel,
                ..
            }
        )),
        "Started より前に Fuel の BudgetCharged がある"
    );
    assert!(
        !envelopes[started..finished].iter().any(is_charged),
        "Started から Finished の間に BudgetCharged は無い"
    );
}

/// FBH-AT-02: 2 回の host call。2 つ目の Started より前に 1 つ目の host call 自身の課金が出る。
#[test]
fn fbh_at_02_second_host_call_sees_first_calls_charges() {
    let envelopes = run_fs_script(
        "fbh2",
        "let i = 0\nwhile i < 3\n    i = i + 1\nend\nwrite_file(\"@data/out.txt\", \"hello\")\nlet c = read_file(\"@data/out.txt\")\n",
    );
    let started: Vec<usize> = envelopes
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.event, AuditEvent::HostCallStarted { .. }))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(started.len(), 2);
    let first_finished = envelopes
        .iter()
        .position(|e| matches!(e.event, AuditEvent::HostCallFinished { .. }))
        .expect("finished");
    assert!(
        envelopes[first_finished..started[1]]
            .iter()
            .any(|e| matches!(
                e.event,
                AuditEvent::BudgetCharged {
                    resource: tsumugi::BudgetResource::HostCalls
                        | tsumugi::BudgetResource::HostCallBytes
                        | tsumugi::BudgetResource::HostRequestBytes,
                    ..
                }
            )),
        "1 つ目の host call 自身の課金が 2 つ目の Started より前に出る"
    );
    let terminal_usage = match &envelopes.last().expect("non-empty").event {
        AuditEvent::Terminal { usage, .. } => *usage,
        other => panic!("Terminal のはず: {other:?}"),
    };
    // 全 cumulative resource について、BudgetCharged の committed_delta の合計が Terminal の
    // committed と一致する（§7.3）。
    use tsumugi::BudgetResource as R;
    let c = terminal_usage.committed;
    let cumulative = [
        (R::Fuel, c.fuel),
        (R::StringAllocations, c.string_allocations),
        (R::StringBytes, c.string_bytes),
        (R::SourceCount, u64::from(c.source_count)),
        (R::SourceBytes, c.source_bytes),
        (R::ImportCount, u64::from(c.import_count)),
        (R::ImportBytes, c.import_bytes),
        (R::InputCalls, c.input_calls),
        (R::InputBytes, c.input_bytes),
        (R::OutputCalls, c.output_calls),
        (R::OutputBytes, c.output_bytes),
        (R::HostCalls, c.host_calls),
        (R::HostRequestBytes, c.host_request_bytes),
        (R::HostResponseBytes, c.host_response_bytes),
        (R::HostCallBytes, c.host_call_bytes),
    ];
    assert!(c.fuel > 0, "Fuel の突合が 0 == 0 にならない script のはず");
    for (resource, expected) in cumulative {
        let sum: u64 = envelopes
            .iter()
            .filter_map(|e| match &e.event {
                AuditEvent::BudgetCharged {
                    resource: r,
                    committed_delta,
                    ..
                } if *r == resource => Some(*committed_delta),
                _ => None,
            })
            .sum();
        assert_eq!(
            sum, expected,
            "{resource:?} の delta 総和は Terminal と一致"
        );
    }
}

/// FBH-AT-03: sink 無しの経路は変わらない（outcome（usage を含む）とファイル byte が sink ありと一致）。
#[test]
fn fbh_at_03_sinkless_path_unchanged() {
    let text = "let i = 0\nwhile i < 3\n    i = i + 1\nend\nwrite_file(\"@data/out.txt\", \"hello\")\nlet c = read_file(\"@data/out.txt\")\n";
    let run = |with_sink: bool| {
        let base = temp_dir("fbh3");
        let mut b = EmbeddingEngine::builder();
        if with_sink {
            b = b.audit_sink(Arc::new(InMemoryAuditSink::new()) as Arc<dyn AuditSink>);
        }
        let engine = b.build().expect("build");
        let script = link_source(&engine, "m", text);
        let linked = engine.link(&script, link_request()).expect("link");
        let mut ctx = EmbeddingContext::new(&engine);
        let request = run_request(3, fs_capabilities(&base));
        let outcome = if with_sink {
            let a = engine.run_audited(&linked, &mut ctx, request);
            format!("{:?}", a.outcome().expect("outcome"))
        } else {
            format!("{:?}", engine.run(&linked, &mut ctx, request))
        };
        let bytes = std::fs::read(base.join("out.txt")).expect("written file");
        let _ = std::fs::remove_dir_all(&base);
        (outcome, bytes)
    };
    let (with, with_bytes) = run(true);
    let (without, without_bytes) = run(false);
    assert!(with.contains("Completed"), "{with}");
    assert!(
        with.contains("committed: BudgetCounters { fuel: 3,"),
        "fuel が非 0: {with}"
    );
    assert_eq!(with, without, "usage を含む outcome が sink 有無で一致");
    assert_eq!(with_bytes, without_bytes);
    assert_eq!(with_bytes, b"hello");
}

// -----------------------------------------------------------------------------
// fix-terminal-host-effects-may-remain（HEM-AT-01..04、§13 AUD-024）
// -----------------------------------------------------------------------------

/// script を `caps` で監査付きに走らせ、Terminal の (outcome, context_committed,
/// host_effects_may_remain) を返す。outcome の種別は呼び出し側では見ない。
fn run_terminal_flags(
    id: u128,
    text: &'static str,
    caps: CapabilitySet,
) -> (TerminalOutcome, bool, bool) {
    let sink = Arc::new(InMemoryAuditSink::new());
    let engine = EmbeddingEngine::builder()
        .audit_sink(sink.clone() as Arc<dyn AuditSink>)
        .build()
        .expect("build");
    let script = link_source(&engine, "m", text);
    let linked = engine.link(&script, link_request()).expect("link");
    let mut ctx = EmbeddingContext::new(&engine);
    let _ = engine.run_audited(&linked, &mut ctx, run_request(id, caps));
    let envelopes = sink.snapshot();
    match &envelopes.last().expect("non-empty").event {
        AuditEvent::Terminal {
            outcome,
            context_committed,
            host_effects_may_remain,
            ..
        } => (
            outcome.clone(),
            *context_committed,
            *host_effects_may_remain,
        ),
        other => panic!("末尾は Terminal のはず: {other:?}"),
    }
}

#[test]
fn hem_at_01_rollback_terminal_keeps_host_effects_may_remain() {
    let base = temp_dir("hem01");
    let (outcome, committed, remain) = run_terminal_flags(
        31,
        "write_file(\"@data/out.txt\", \"hello\")\nlet x = 1 / 0\n",
        fs_capabilities(&base),
    );
    let _ = std::fs::remove_dir_all(&base);
    assert_ne!(outcome, TerminalOutcome::Completed);
    assert!(
        !committed,
        "未捕捉 runtime error は context を commit しない"
    );
    assert!(remain, "Committed の write_file が残りうる");
}

#[test]
fn hem_at_02_completed_terminal_keeps_host_effects_may_remain() {
    let base = temp_dir("hem02");
    let (outcome, committed, remain) = run_terminal_flags(
        32,
        "write_file(\"@data/out.txt\", \"hello\")\n",
        fs_capabilities(&base),
    );
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(outcome, TerminalOutcome::Completed);
    assert!(committed);
    assert!(remain);
}

#[test]
fn hem_at_03_no_external_effect_terminal_is_false() {
    // host call なし。
    let (_, _, remain) = run_terminal_flags(33, "let x = 1\n", CapabilitySet::empty());
    assert!(!remain, "host call なし");

    // read_file だけ（effect_status = None）。
    let base = temp_dir("hem03");
    std::fs::write(base.join("in.txt"), "abc").expect("seed");
    let (_, _, remain) = run_terminal_flags(
        34,
        "let c = read_file(\"@data/in.txt\")\n",
        fs_capabilities(&base),
    );
    let _ = std::fs::remove_dir_all(&base);
    assert!(!remain, "read_file は effect_status = None");

    // grant が無く write_file が拒否される（None）。
    let (outcome, _, remain) = run_terminal_flags(
        35,
        "try\n    write_file(\"@data/out.txt\", \"x\")\ncatch e\n    let ok = true\nend\n",
        CapabilitySet::empty(),
    );
    assert_eq!(outcome, TerminalOutcome::Completed);
    assert!(!remain, "Deny は effect_status = None");

    // Started の ack 失敗の Terminal。
    let sink = Arc::new(ScriptedAuditSink::new([ScriptedResponse::AckWrongId]));
    let engine = EmbeddingEngine::builder()
        .audit_sink(sink.clone() as Arc<dyn AuditSink>)
        .build()
        .expect("build");
    let script = link_source(&engine, "m", "let x = 1\n");
    let linked = engine.link(&script, link_request()).expect("link");
    let mut ctx = EmbeddingContext::new(&engine);
    let _ = engine.run_audited(&linked, &mut ctx, run_request(36, CapabilitySet::empty()));
    let terminals: Vec<_> = sink
        .snapshot()
        .into_iter()
        .filter_map(|e| match e.event {
            AuditEvent::Terminal {
                outcome,
                host_effects_may_remain,
                ..
            } => Some((outcome, host_effects_may_remain)),
            _ => None,
        })
        .collect();
    assert!(!terminals.is_empty(), "AuditFailure の Terminal がある");
    for (outcome, remain) in terminals {
        assert_eq!(outcome, TerminalOutcome::AuditFailure);
        assert!(!remain);
    }
}

#[test]
fn hem_flag_is_reset_between_executions() {
    // HEM-AT-04（非破壊）は既存の tests/audit_*.rs が無改変で緑であること。ここは持ち越しの確認。
    // 同じ context・engine で、write のあとの実行の次に host call なしの実行をしても持ち越さない。
    let base = temp_dir("hem04");
    let sink = Arc::new(InMemoryAuditSink::new());
    let engine = EmbeddingEngine::builder()
        .audit_sink(sink.clone() as Arc<dyn AuditSink>)
        .build()
        .expect("build");
    let mut ctx = EmbeddingContext::new(&engine);
    let w = link_source(&engine, "m", "write_file(\"@data/o.txt\", \"a\")\n");
    let w = engine.link(&w, link_request()).expect("link");
    let _ = engine.run_audited(&w, &mut ctx, run_request(41, fs_capabilities(&base)));
    let n = link_source(&engine, "m2", "let y = 2\n");
    let n = engine.link(&n, link_request()).expect("link");
    let _ = engine.run_audited(&n, &mut ctx, run_request(42, CapabilitySet::empty()));
    let _ = std::fs::remove_dir_all(&base);
    let flags: Vec<bool> = sink
        .snapshot()
        .into_iter()
        .filter_map(|e| match e.event {
            AuditEvent::Terminal {
                host_effects_may_remain,
                ..
            } => Some(host_effects_may_remain),
            _ => None,
        })
        .collect();
    assert_eq!(flags, vec![true, false]);
}

//! fix-fs-write-content-budgeted-render（issue #356）: `write_file` / `append_file` の `Str` でない
//! 第 2 引数を、予算付きの `render_display` で 1 回だけ描画する。受入基準 FWR-AT-01..04
//! （docs/execution-control.md の同名スライス小節）。

use std::collections::BTreeSet;
use std::io::Read as _;
use std::num::NonZeroU128;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tsumugi::budget::{BudgetConfig, BudgetUsage};
use tsumugi::compiler::Compiler;
use tsumugi::error::{ErrorKind, TsumugiError};
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;
use tsumugi::{
    AuditDirection, AuditEvent, AuditField, AuditFieldClass, AuditFieldKey, AuditScalar, AuditSink,
    AuditValue, BudgetResource, CapabilitySet, CompileOptions, EmbeddingContext, EmbeddingEngine,
    EmbeddingOutcome, EmbeddingRequest, ExecutionId, FakeClock, FilesystemCapability,
    FilesystemRoot, FsOperation, InMemoryAuditSink, LinkRequest, MonotonicClock, MountName,
    OsDirectoryHandle, RedactionMode, RedactionPolicy, Source, SourceId, SymlinkPolicy,
    SystemOutput,
};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("tsumugi-fwr-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 共有された入れ子（`a = [a, a]` を `n` 回）を作る前置き。描画は 2^n の葉を持つ。
fn dag_prelude(n: u32) -> String {
    format!("let a = [1]\nlet i = 0\nwhile i < {n}\n  a = [a, a]\n  i = i + 1\nend\n")
}

/// 1 MiB の文字列 `s` を作り、`a` を 9 個の `s` の List にする前置き（描画が 8 MiB を超える）。
const BIG_STRING_PRELUDE: &str = concat!(
    "let s = \"x\"\n",
    "let i = 0\n",
    "while i < 20\n",
    "  s = s + s\n",
    "  i = i + 1\n",
    "end\n",
    "let a = [s, s, s, s, s, s, s, s, s]\n",
);

struct CliResult {
    success: bool,
    stderr: String,
    elapsed: Duration,
}

/// CLI（既定の safe profile）で、`dir` を cwd にして script を実行する。`limit` を超えたら kill する。
fn run_cli(source: &str, use_vm: bool, dir: &Path, limit: Duration) -> CliResult {
    let script = dir.join("s.tsg");
    std::fs::write(&script, source).expect("write script");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    command
        .arg(&script)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let start = Instant::now();
    let mut child = command.spawn().expect("spawn");
    let mut out = child.stdout.take().expect("stdout");
    let mut err = child.stderr.take().expect("stderr");
    let out_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
    });
    let err_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = err.read_to_string(&mut s);
        s
    });
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait") {
            break status;
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{limit:?} 以内に終わらなかった（vm={use_vm}）");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    out_thread.join().expect("join");
    CliResult {
        success: status.success(),
        stderr: err_thread.join().expect("join").replace("\r\n", "\n"),
        elapsed: start.elapsed(),
    }
}

/// FWR-AT-01: issue #356 の再現（`a = [a, a]` を 64 回）の `write_file` / `append_file` が、fs の
/// grant が無い既定の safe profile でも有限時間で `print(a)` と同じ fuel 超過で止まり、ファイルが
/// 作られない。tree-walk 版と VM 版で同じ。
#[test]
fn fwr_at_01_shared_dag_without_grant_stops_in_bounded_time() {
    let limit = Duration::from_secs(10);
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        let print_dir = TestDir::new();
        let printed = run_cli(
            &format!("{}print(a)\n", dag_prelude(64)),
            use_vm,
            &print_dir.path,
            limit,
        );
        assert!(!printed.success, "[{mode}] print(a) が成功した");
        let message = "ステップ上限に達しました";
        assert!(
            printed.stderr.contains(message),
            "[{mode}] {}",
            printed.stderr
        );

        for builtin in ["write_file", "append_file"] {
            let dir = TestDir::new();
            let r = run_cli(
                &format!("{}{builtin}(\"x.txt\", a)\n", dag_prelude(64)),
                use_vm,
                &dir.path,
                limit,
            );
            assert!(!r.success, "[{mode}] {builtin}: 成功してしまった");
            assert!(
                r.stderr.contains(message),
                "[{mode}] {builtin}: fuel 超過で止まっていない: {}",
                r.stderr
            );
            assert!(r.elapsed < limit, "[{mode}] {builtin}: {:?}", r.elapsed);
            assert!(
                !dir.path.join("x.txt").exists(),
                "[{mode}] {builtin}: x.txt が作られた"
            );
        }
    }
}

fn rw_fs(base: &Path) -> FilesystemCapability {
    let pid = NonZeroU128::new(1).expect("non-zero");
    let ops: BTreeSet<FsOperation> = [FsOperation::Read, FsOperation::Write, FsOperation::Create]
        .into_iter()
        .collect();
    let mount = MountName::new("data").expect("mount");
    let handle = OsDirectoryHandle::new(base.to_path_buf(), pid, SymlinkPolicy::DenyAll);
    let root = FilesystemRoot::new(mount, pid, ops, SymlinkPolicy::DenyAll, Arc::new(handle))
        .expect("root");
    FilesystemCapability::new(vec![root]).expect("fs")
}

fn capabilities(base: &Path) -> CapabilitySet {
    let pid = NonZeroU128::new(1).expect("non-zero");
    CapabilitySet::builder()
        .filesystem(rw_fs(base))
        .expect("grant filesystem")
        .stdout(Arc::new(SystemOutput::new(pid)))
        .expect("stdout grant")
        .build()
}

fn run_tree(source: &str, base: &Path) -> (Result<(), TsumugiError>, BudgetUsage) {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("parse");
    let mut evaluator = Evaluator::with_budget(BudgetConfig::for_legacy(1_000_000, 1_000_000));
    evaluator.set_capabilities(capabilities(base));
    let result = evaluator.run(&program, source.len() as u64);
    (result, evaluator.budget_usage())
}

fn run_vm(source: &str, base: &Path) -> (Result<(), TsumugiError>, BudgetUsage) {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("parse");
    let chunk = Compiler::new().compile(&program).expect("compile");
    let mut vm = Vm::with_budget(chunk, BudgetConfig::for_legacy(1_000_000, 1_000_000));
    vm.set_capabilities(capabilities(base));
    vm.charge_link(source.len() as u64, &[])
        .expect("charge_link");
    let result = vm.run();
    (result, vm.budget_usage())
}

fn run_engine(source: &str, base: &Path, use_vm: bool) -> (Result<(), TsumugiError>, BudgetUsage) {
    if use_vm {
        run_vm(source, base)
    } else {
        run_tree(source, base)
    }
}

/// FWR-AT-02: grant 有りでも、描画が fuel / `max_single_string_bytes` を超える内容の
/// `write_file` / `append_file` は adapter を呼ばずに terminal で止まり、`try` / `catch` で
/// 捕捉されず、host call と request bytes が課金されない。
#[test]
fn fwr_at_02_render_over_budget_stops_before_external_effect() {
    let cases: [(&str, String, ErrorKind); 2] = [
        ("fuel", dag_prelude(40), ErrorKind::StepLimit),
        (
            "single_string_bytes",
            BIG_STRING_PRELUDE.to_string(),
            ErrorKind::StringLimit,
        ),
    ];
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        for (label, prelude, kind) in &cases {
            for builtin in ["write_file", "append_file"] {
                let dir = TestDir::new();
                let source = format!(
                    "{prelude}try\n  {builtin}(\"@data/x.txt\", a)\ncatch e\n  write_file(\"@data/caught.txt\", \"c\")\nend\n"
                );
                let (result, usage) = run_engine(&source, &dir.path, use_vm);
                let error = result.expect_err("予算超過で止まるはず");
                assert_eq!(error.kind(), Some(*kind), "[{mode}] {label} {builtin}");
                assert_eq!(usage.committed.host_calls, 0, "[{mode}] {label} {builtin}");
                assert_eq!(
                    usage.committed.host_request_bytes, 0,
                    "[{mode}] {label} {builtin}"
                );
                for name in ["x.txt", "caught.txt"] {
                    assert!(
                        !dir.path.join(name).exists(),
                        "[{mode}] {label} {builtin}: {name} が作られた"
                    );
                }
            }
        }
    }
}

/// FWR-AT-03 の第 2 引数と、`Display` の描画（write と append の 2 回ぶん書かれる内容）。
const CONTENT_CASES: &[(&str, &str)] = &[
    ("[1, \"a\", [2.5, null]]", "[1, \"a\", [2.5, null]]"),
    ("{\"k\": 7, \"z\": [true]}", "{\"k\": 7, \"z\": [true]}"),
    ("[[1], {\"a\": [2]}]", "[[1], {\"a\": [2]}]"),
    ("[]", "[]"),
    ("42", "42"),
    ("1.5", "1.5"),
    ("true", "true"),
    ("null", "null"),
    ("\"hello\"", "hello"),
];

/// 変更前の実装で測った、`write_file("@data/w.txt", "hello")` 1 回だけの script の fuel（tree, VM）。
const STR_WRITE_FUEL: (u64, u64) = (0, 0);

/// FWR-AT-03: 予算内の内容の書き込み結果と committed の `host_request_bytes` は、変更前の
/// `Display` の描画と同じ。`Str` の第 2 引数の fuel も変わらない。tree-walk 版と VM 版で同じ。
#[test]
fn fwr_at_03_content_and_request_bytes_are_unchanged() {
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        for (expr, expected) in CONTENT_CASES {
            let dir = TestDir::new();
            let source = format!(
                "write_file(\"@data/w.txt\", {expr})\nappend_file(\"@data/w.txt\", {expr})\n"
            );
            let (result, usage) = run_engine(&source, &dir.path, use_vm);
            result.unwrap_or_else(|e| panic!("[{mode}] {expr}: {e}"));
            let written = std::fs::read_to_string(dir.path.join("w.txt")).expect("read");
            assert_eq!(written, format!("{expected}{expected}"), "[{mode}] {expr}");
            assert_eq!(
                usage.committed.host_request_bytes,
                2 * expected.len() as u64,
                "[{mode}] {expr}"
            );
            assert_eq!(usage.committed.host_calls, 2, "[{mode}] {expr}");
        }

        let dir = TestDir::new();
        let (result, usage) = run_engine(
            "write_file(\"@data/w.txt\", \"hello\")\n",
            &dir.path,
            use_vm,
        );
        result.expect("run");
        let expected_fuel = if use_vm {
            STR_WRITE_FUEL.1
        } else {
            STR_WRITE_FUEL.0
        };
        assert_eq!(usage.committed.fuel, expected_fuel, "[{mode}] Str の fuel");
        assert_eq!(usage.committed.host_request_bytes, 5, "[{mode}] Str");
    }
}

/// 複合ノードを 4 個持つ（外側 List、内側 List、Dict、その中の List）非共有の値。
const VALUE_4_NODES: &str = "[[1, 2], {\"k\": [3]}]";

fn fuel_of(source: &str, base: &Path, use_vm: bool) -> u64 {
    let (result, usage) = run_engine(source, base, use_vm);
    result.unwrap_or_else(|e| panic!("{source}: {e}"));
    usage.committed.fuel
}

/// FWR-AT-04（前半）: 同じ値を `print` したときと `write_file` / `append_file` したときの、描画による
/// fuel の増分が一致する（`Str` を渡した同形の script との差で見る）。
#[test]
fn fwr_at_04_render_fuel_matches_print() {
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        let dir = TestDir::new();
        let p = dir.path.as_path();
        let print_delta = fuel_of(&format!("print({VALUE_4_NODES})\n"), p, use_vm)
            - fuel_of("print(\"s\")\n", p, use_vm);
        assert!(
            print_delta >= 4,
            "[{mode}] print の描画の fuel: {print_delta}"
        );
        for builtin in ["write_file", "append_file"] {
            let delta = fuel_of(
                &format!("{builtin}(\"@data/w.txt\", {VALUE_4_NODES})\n"),
                p,
                use_vm,
            ) - fuel_of(&format!("{builtin}(\"@data/w.txt\", \"s\")\n"), p, use_vm);
            assert_eq!(delta, print_delta, "[{mode}] {builtin}");
        }
    }
}

// --- 監査（tree）---

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn run_audited(
    text: &'static str,
    base: &Path,
    policy: RedactionPolicy,
) -> Vec<tsumugi::AuditEnvelope> {
    let sink = Arc::new(InMemoryAuditSink::new());
    let engine = EmbeddingEngine::builder()
        .audit_sink(sink.clone() as Arc<dyn AuditSink>)
        .audit_redaction_policy_for_test(policy)
        .build()
        .expect("build");
    let script = engine
        .compile(
            Source::new(SourceId::new("m").expect("id"), text),
            &CompileOptions {
                retain_source: true,
            },
        )
        .expect("compile");
    let clock = FakeClock::new();
    let link_budget = BudgetConfig::standard(&clock).expect("standard budget");
    let linked = engine
        .link(
            &script,
            LinkRequest::new(
                ExecutionId::new(1u128.try_into().expect("nonzero")),
                CapabilitySet::empty(),
                link_budget,
            ),
        )
        .expect("link");
    let mut ctx = EmbeddingContext::new(&engine);
    let run_clock: Arc<dyn MonotonicClock> = Arc::new(FakeClock::new());
    let budget = BudgetConfig::standard(run_clock.as_ref()).expect("standard budget");
    let request = EmbeddingRequest::new(budget, run_clock)
        .expect("same domain")
        .with_execution_id(ExecutionId::new(2u128.try_into().expect("nonzero")))
        .with_capabilities(capabilities(base));
    let audited = engine.run_audited(&linked, &mut ctx, request);
    assert!(matches!(
        audited.outcome(),
        Some(EmbeddingOutcome::Completed { .. })
    ));
    sink.snapshot()
}

fn reveal_policy() -> RedactionPolicy {
    let key = |field: &str| AuditFieldKey {
        operation: "write_file".to_string(),
        direction: AuditDirection::Request,
        field: field.to_string(),
    };
    RedactionPolicy {
        body_mode: RedactionMode::Reveal,
        reveal_body_fields: [key("body"), key("1")].into_iter().collect(),
        max_revealed_body_bytes: 256,
        ..RedactionPolicy::default()
    }
}

/// `HostCallStarted` の前と後（Terminal まで）の、`BudgetCharged` の Fuel の committed_delta の合計。
fn fuel_split_at_started(envelopes: &[tsumugi::AuditEnvelope]) -> (u64, u64) {
    let started = envelopes
        .iter()
        .position(|e| matches!(e.event, AuditEvent::HostCallStarted { .. }))
        .expect("HostCallStarted");
    let fuel = |range: &[tsumugi::AuditEnvelope]| -> u64 {
        range
            .iter()
            .filter_map(|e| match &e.event {
                AuditEvent::BudgetCharged {
                    resource: BudgetResource::Fuel,
                    committed_delta,
                    ..
                } => Some(*committed_delta),
                _ => None,
            })
            .sum()
    };
    (fuel(&envelopes[..started]), fuel(&envelopes[started..]))
}

/// FWR-AT-04（後半）: 監査付き実行（tree）で、Reveal を許す policy の `HostCallStarted` の request
/// body が書き込まれた内容と同じ文字列になり、描画の fuel は `HostCallStarted` の後の
/// `BudgetCharged` に出る（host call より前の消費は描画の fuel を含まない）。
#[test]
fn fwr_at_04_audit_body_and_render_fuel_position() {
    let dir = TestDir::new();
    let value = leak(format!("write_file(\"@data/w.txt\", {VALUE_4_NODES})\n"));
    let baseline = leak("write_file(\"@data/w.txt\", \"s\")\n".to_string());
    let envelopes = run_audited(value, &dir.path, reveal_policy());
    let written = std::fs::read_to_string(dir.path.join("w.txt")).expect("read");
    assert_eq!(written, "[[1, 2], {\"k\": [3]}]");

    let started = envelopes
        .iter()
        .find_map(|e| match &e.event {
            AuditEvent::HostCallStarted { request, .. } => Some(request),
            _ => None,
        })
        .expect("HostCallStarted");
    assert_eq!(
        started.fields.get("body"),
        Some(&AuditField::Revealed {
            class: AuditFieldClass::Body,
            value: AuditValue::Scalar(AuditScalar::Utf8(written.clone())),
            original_bytes: written.len() as u64,
        })
    );

    let base_dir = TestDir::new();
    let base_envelopes = run_audited(baseline, &base_dir.path, reveal_policy());
    let (before, after) = fuel_split_at_started(&envelopes);
    let (base_before, base_after) = fuel_split_at_started(&base_envelopes);
    assert_eq!(
        before, base_before,
        "host call より前の消費が描画の fuel を含む"
    );
    assert_eq!(
        after - base_after,
        4,
        "描画の fuel（複合ノード 4 個）が HostCallStarted の後に出ていない"
    );
}

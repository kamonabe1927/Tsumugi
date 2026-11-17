//! fix-fs-builtin-host-call-charge（issue #364）: filesystem builtin の 14 個すべてを host call として
//! 回数・byte を課金する。受入基準 FHC-AT-01..05（docs/execution-control.md の同名スライス小節）。

use std::collections::BTreeSet;
use std::num::NonZeroU128;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tsumugi::budget::{BudgetConfig, BudgetUsage};
use tsumugi::compiler::Compiler;
use tsumugi::error::TsumugiError;
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;
use tsumugi::{
    AuditEvent, AuditSink, BudgetResource, CapabilitySet, CompileOptions, EmbeddingContext,
    EmbeddingEngine, EmbeddingOutcome, EmbeddingRequest, ExecutionId, FakeClock,
    FilesystemCapability, FilesystemRoot, FsOperation, InMemoryAuditSink, LinkRequest,
    MonotonicClock, MountName, OsDirectoryHandle, Source, SourceId, SymlinkPolicy, SystemOutput,
};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    /// `f`（"hello"）、`sub/{aa,bbb}`、空の `sub2`、`tree/x` を持つ directory を作る。
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("tsumugi-fhc-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(path.join("sub")).expect("mkdir");
        std::fs::create_dir_all(path.join("sub2")).expect("mkdir");
        std::fs::create_dir_all(path.join("tree")).expect("mkdir");
        std::fs::write(path.join("f"), "hello").expect("write");
        std::fs::write(path.join("sub/aa"), "1").expect("write");
        std::fs::write(path.join("sub/bbb"), "2").expect("write");
        std::fs::write(path.join("tree/x"), "3").expect("write");
        Self { path }
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// 14 個のうち、このスライスで新しく課金される 10 個と、成功する呼び出し。
const TEN: [(&str, &str); 10] = [
    ("path_exists", "path_exists(\"@d/f\")"),
    ("is_file", "is_file(\"@d/f\")"),
    ("is_dir", "is_dir(\"@d/sub\")"),
    ("file_size", "file_size(\"@d/f\")"),
    ("list_dir", "list_dir(\"@d/sub\")"),
    ("mkdir", "mkdir(\"@d/newdir\")"),
    ("remove", "remove(\"@d/f\")"),
    ("remove_dir", "remove_dir(\"@d/sub2\")"),
    ("remove_tree", "remove_tree(\"@d/tree\")"),
    ("rename", "rename(\"@d/f\", \"@d/g\")"),
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Grant {
    Full,
    NoGrant,
    ReadOnly,
}

fn capabilities(base: &Path, grant: Grant) -> CapabilitySet {
    let pid = NonZeroU128::new(1).expect("non-zero");
    if grant == Grant::NoGrant {
        return CapabilitySet::empty();
    }
    let ops: BTreeSet<FsOperation> = if grant == Grant::Full {
        [
            FsOperation::Read,
            FsOperation::Write,
            FsOperation::Create,
            FsOperation::Delete,
            FsOperation::Metadata,
            FsOperation::List,
        ]
        .into_iter()
        .collect()
    } else {
        [FsOperation::Read].into_iter().collect()
    };
    let handle = OsDirectoryHandle::new(base.to_path_buf(), pid, SymlinkPolicy::DenyAll);
    let root = FilesystemRoot::new(
        MountName::new("d").expect("mount"),
        pid,
        ops,
        SymlinkPolicy::DenyAll,
        Arc::new(handle),
    )
    .expect("root");
    let fs = FilesystemCapability::new(vec![root]).expect("fs");
    CapabilitySet::builder()
        .filesystem(fs)
        .expect("grant filesystem")
        .stdout(Arc::new(SystemOutput::new(pid)))
        .expect("stdout grant")
        .build()
}

fn run_engine(
    source: &str,
    base: &Path,
    grant: Grant,
    use_vm: bool,
) -> (Result<(), TsumugiError>, BudgetUsage) {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("parse");
    let config = BudgetConfig::for_legacy(1_000_000, 1_000_000);
    if use_vm {
        let chunk = Compiler::new().compile(&program).expect("compile");
        let mut vm = Vm::with_budget(chunk, config);
        vm.set_capabilities(capabilities(base, grant));
        vm.charge_link(source.len() as u64, &[])
            .expect("charge_link");
        let result = vm.run();
        (result, vm.budget_usage())
    } else {
        let mut evaluator = Evaluator::with_budget(config);
        evaluator.set_capabilities(capabilities(base, grant));
        let result = evaluator.run(&program, source.len() as u64);
        (result, evaluator.budget_usage())
    }
}

/// 呼び出しを `try` / `catch` で囲んで 1 回だけ実行する script（拒否・error でも最後まで進む）。
fn guarded(call: &str) -> String {
    format!("try\n  {call}\ncatch e\n  let swallowed = 1\nend\n")
}

/// FHC-AT-02: 10 個のそれぞれ 1 回の呼び出しで host_calls が 1 増える（成功・capability 拒否・
/// host error・パスの形式エラーのどれでも）。tree と VM で一致する。
#[test]
fn fhc_at_02_each_builtin_counts_one_host_call() {
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        for (name, call) in TEN {
            let missing = call
                .replace("@d/f", "@d/missing")
                .replace("@d/sub", "@d/missing");
            let malformed = call.replace("@d/", "nomount/");
            let cases = [
                ("成功", call.to_string(), Grant::Full),
                ("grant 無し", call.to_string(), Grant::NoGrant),
                ("操作の許可無し", call.to_string(), Grant::ReadOnly),
                ("host error", missing, Grant::Full),
                ("パスの形式エラー", malformed, Grant::Full),
            ];
            for (label, call, grant) in cases {
                let dir = TestDir::new();
                let (result, usage) = run_engine(&guarded(&call), &dir.path, grant, use_vm);
                result.unwrap_or_else(|e| panic!("[{mode}] {name} {label}: {e}"));
                assert_eq!(
                    usage.committed.host_calls, 1,
                    "[{mode}] {name} {label}: host_calls"
                );
            }
        }
    }
}

/// FHC-AT-03（usage の照合）: `list_dir` の成功だけ response bytes が名前の byte 長の合計
/// （"aa" + "bbb" = 5）だけ増え、拒否・error では 0。request bytes は 10 個とも 0、他の 9 個は
/// request・response・call bytes とも 0。
#[test]
fn fhc_at_03_byte_accounting() {
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        for (name, call) in TEN {
            let dir = TestDir::new();
            let (result, usage) = run_engine(&guarded(call), &dir.path, Grant::Full, use_vm);
            result.unwrap_or_else(|e| panic!("[{mode}] {name}: {e}"));
            let c = usage.committed;
            let expected = if name == "list_dir" { 5 } else { 0 };
            assert_eq!(c.host_request_bytes, 0, "[{mode}] {name} request");
            assert_eq!(c.host_response_bytes, expected, "[{mode}] {name} response");
            assert_eq!(c.host_call_bytes, expected, "[{mode}] {name} call bytes");

            for (label, grant, call) in [
                ("grant 無し", Grant::NoGrant, call.to_string()),
                ("操作の許可無し", Grant::ReadOnly, call.to_string()),
                (
                    "host error",
                    Grant::Full,
                    call.replace("@d/sub", "@d/missing"),
                ),
            ] {
                let dir = TestDir::new();
                let (result, usage) = run_engine(&guarded(&call), &dir.path, grant, use_vm);
                result.unwrap_or_else(|e| panic!("[{mode}] {name} {label}: {e}"));
                let c = usage.committed;
                assert_eq!(c.host_request_bytes, 0, "[{mode}] {name} {label}");
                assert_eq!(c.host_response_bytes, 0, "[{mode}] {name} {label}");
                assert_eq!(c.host_call_bytes, 0, "[{mode}] {name} {label}");
            }
        }
    }
}

fn run_cli(source: &str, use_vm: bool, dir: &Path, env: &[(&str, &str)]) -> Output {
    let script = dir.join("s.tsg");
    std::fs::write(&script, source).expect("write script");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    command
        .arg("--fs-root")
        .arg(format!("d={}", dir.display()))
        .arg("--fs-op")
        .arg("d=metadata,read,list,write,create,delete")
        .arg(&script)
        .current_dir(dir);
    for (k, v) in env {
        command.env(k, v);
    }
    command.output().expect("run tsumugi")
}

fn text(output: &Output) -> (String, String) {
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const CALLS_MSG: &str = "host call の呼び出し回数が上限を超えました";

/// n 回 `call` を呼ぶ script（出力なし）。
fn repeat(call: &str, n: u32) -> String {
    format!("let i = 0\nwhile i < {n}\n  {call}\n  i = i + 1\nend\n")
}

/// FHC-AT-01: issue #364 の再現。`path_exists` を上限（N）回呼ぶと成功し、N+1 回目で止まる。
/// `try` / `catch` で囲んでも catch されない。issue の 20,000 回の loop も既定の上限で止まる。
#[test]
fn fhc_at_01_path_exists_is_gated_by_max_host_calls() {
    let limit = [("TSUMUGI_MAX_HOST_CALLS", "5")];
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        let dir = TestDir::new();
        let ok = run_cli(
            &repeat("path_exists(\"@d/x\")", 5),
            use_vm,
            &dir.path,
            &limit,
        );
        let (_, stderr) = text(&ok);
        assert!(
            ok.status.success(),
            "[{mode}] N 回目まで成功するはず: {stderr}"
        );

        let over = run_cli(
            &repeat("path_exists(\"@d/x\")", 6),
            use_vm,
            &dir.path,
            &limit,
        );
        let (_, stderr) = text(&over);
        assert!(!over.status.success(), "[{mode}] N+1 回目で止まるはず");
        assert!(stderr.contains(CALLS_MSG), "[{mode}] {stderr}");

        let caught = format!(
            "try\n{}catch e\n  print(\"caught\")\nend\n",
            repeat("path_exists(\"@d/x\")", 6)
        );
        let caught = run_cli(&caught, use_vm, &dir.path, &limit);
        let (stdout, stderr) = text(&caught);
        assert!(!caught.status.success(), "[{mode}] catch されてしまった");
        assert!(stderr.contains(CALLS_MSG), "[{mode}] {stderr}");
        assert!(!stdout.contains("caught"), "[{mode}] {stdout}");

        // issue #364 の script（既定の上限 10,000）。
        let issue = format!(
            "{}print(\"done\")\n",
            repeat("path_exists(\"@d/x\")", 20_000)
        );
        let issue = run_cli(&issue, use_vm, &dir.path, &[]);
        let (stdout, stderr) = text(&issue);
        assert!(!issue.status.success(), "[{mode}] 20,000 回が完了した");
        assert!(stderr.contains(CALLS_MSG), "[{mode}] {stderr}");
        assert!(!stdout.contains("done"), "[{mode}] {stdout}");
    }
}

/// FHC-AT-03（上限の境界）: `TSUMUGI_MAX_HOST_RESPONSE_BYTES` を名前の合計 − 1 にすると
/// `list_dir` が止まり、ちょうどなら成功する。
#[test]
fn fhc_at_03_list_dir_response_bytes_boundary() {
    let message = "host call response の総バイト数が上限を超えました";
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        let dir = TestDir::new();
        let src = "list_dir(\"@d/sub\")\n";
        let ok = run_cli(
            src,
            use_vm,
            &dir.path,
            &[("TSUMUGI_MAX_HOST_RESPONSE_BYTES", "5")],
        );
        let (_, stderr) = text(&ok);
        assert!(
            ok.status.success(),
            "[{mode}] ちょうどなら成功するはず: {stderr}"
        );

        let over = run_cli(
            src,
            use_vm,
            &dir.path,
            &[("TSUMUGI_MAX_HOST_RESPONSE_BYTES", "4")],
        );
        let (_, stderr) = text(&over);
        assert!(!over.status.success(), "[{mode}] 合計 − 1 で止まるはず");
        assert!(stderr.contains(message), "[{mode}] {stderr}");
    }
}

/// FHC-AT-04: N 回の host call のあとの `mkdir` / `rename` / `remove` は、外部効果の前に止まる。
#[test]
fn fhc_at_04_charge_precedes_external_effect() {
    let limit = [("TSUMUGI_MAX_HOST_CALLS", "2")];
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        for call in [
            "mkdir(\"@d/newdir\")",
            "rename(\"@d/f\", \"@d/g\")",
            "remove(\"@d/f\")",
        ] {
            let dir = TestDir::new();
            let src = format!("{}{call}\n", repeat("path_exists(\"@d/x\")", 2));
            let output = run_cli(&src, use_vm, &dir.path, &limit);
            let (_, stderr) = text(&output);
            assert!(!output.status.success(), "[{mode}] {call}: 止まらなかった");
            assert!(stderr.contains(CALLS_MSG), "[{mode}] {call}: {stderr}");
            assert!(!dir.path.join("newdir").exists(), "[{mode}] {call}");
            assert!(!dir.path.join("g").exists(), "[{mode}] {call}");
            assert!(dir.path.join("f").exists(), "[{mode}] {call}");
        }
    }
}

/// FHC-AT-05: 監査付きの実行で、10 個は host-call triple を出さず、`read_file` の triple は 1 組のまま。
/// 課金は `BudgetCharged`（HostCalls）に現れる。
#[test]
fn fhc_at_05_no_triple_for_the_ten_and_read_file_unchanged() {
    let dir = TestDir::new();
    // rename 後の g を読むため、read_file は try で囲む（成功・失敗のどちらでも 1 回課金される）。
    let text: &'static str = Box::leak(guarded("read_file(\"@d/g\")").into_boxed_str());
    let source: &'static str = Box::leak(
        TEN.iter()
            .map(|(_, call)| guarded(call))
            .collect::<String>()
            .into_boxed_str(),
    );
    let script_text: &'static str = Box::leak(format!("{source}{text}").into_boxed_str());

    let sink = Arc::new(InMemoryAuditSink::new());
    let engine = EmbeddingEngine::builder()
        .audit_sink(sink.clone() as Arc<dyn AuditSink>)
        .build()
        .expect("build");
    let script = engine
        .compile(
            Source::new(SourceId::new("m").expect("id"), script_text),
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
        .with_capabilities(capabilities(&dir.path, Grant::Full));
    let audited = engine.run_audited(&linked, &mut ctx, request);
    assert!(matches!(
        audited.outcome(),
        Some(EmbeddingOutcome::Completed { .. })
    ));

    let envelopes = sink.snapshot();
    let count = |pred: fn(&AuditEvent) -> bool| envelopes.iter().filter(|e| pred(&e.event)).count();
    assert_eq!(
        count(|e| matches!(e, AuditEvent::HostCallStarted { .. })),
        1,
        "triple は read_file の 1 組だけ"
    );
    assert_eq!(
        count(|e| matches!(e, AuditEvent::CapabilityDecision { .. })),
        1
    );
    assert_eq!(
        count(|e| matches!(e, AuditEvent::HostCallFinished { .. })),
        1
    );
    let host_calls_charged: u64 = envelopes
        .iter()
        .filter_map(|e| match &e.event {
            AuditEvent::BudgetCharged {
                resource: BudgetResource::HostCalls,
                committed_delta,
                ..
            } => Some(*committed_delta),
            _ => None,
        })
        .sum();
    assert_eq!(host_calls_charged, 11, "10 個 + read_file の 1 回ずつ");
}

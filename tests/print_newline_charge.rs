//! fix-print-newline-charge（loop:drift #318 D-41）: `print` の output 課金に改行の 1 byte を含める。
//! 受入基準 PNC-AT-01..02（docs/capability-model.md の同名スライス小節）。

use std::num::NonZeroU128;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tsumugi::budget::{BudgetConfig, BudgetUsage};
use tsumugi::compiler::Compiler;
use tsumugi::error::TsumugiError;
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;
use tsumugi::{CapabilitySet, SystemOutput};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("tsumugi-pnc-{}-{}", std::process::id(), n));
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

/// `print("ab")` の script を CLI で実行し、(成功か, stdout, stderr) を返す。
fn run_cli(use_vm: bool, max_output_bytes: &str) -> (bool, String, String) {
    let dir = TestDir::new();
    let script = dir.path.join("p.tsg");
    std::fs::write(&script, "print(\"ab\")\n").expect("write script");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    let output = command
        .arg(&script)
        .env("TSUMUGI_MAX_OUTPUT_BYTES", max_output_bytes)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("run tsumugi");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
    )
}

/// PNC-AT-01（N 境界）: 上限 3 で `print("ab")`（"ab" + 改行 = 3 byte）は成功し、上限 2 では
/// output の上限超過で止まって stdout に何も書かれない。tree と VM の両方。
#[test]
fn pnc_at_01_output_limit_boundary_includes_newline() {
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };

        let (ok, stdout, stderr) = run_cli(use_vm, "3");
        assert!(ok, "{mode}: 上限 3 で print(\"ab\") が失敗: {stderr}");
        assert_eq!(stdout, "ab\n", "{mode}: 上限 3 の stdout");

        let (ok, stdout, stderr) = run_cli(use_vm, "2");
        assert!(!ok, "{mode}: 上限 2 で print(\"ab\") が成功した");
        assert!(
            stderr.contains("output の総バイト数が上限を超えました"),
            "{mode}: 上限超過の診断が無い: {stderr}"
        );
        assert_eq!(stdout, "", "{mode}: 拒否された print が stdout へ書かれた");
    }
}

fn capabilities() -> CapabilitySet {
    let pid = NonZeroU128::new(1).expect("non-zero");
    CapabilitySet::builder()
        .stdout(Arc::new(SystemOutput::new(pid)))
        .expect("stdout grant")
        .build()
}

fn tree_usage(source: &str) -> BudgetUsage {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("parse");
    let mut evaluator = Evaluator::with_budget(BudgetConfig::for_legacy(100_000, 1_000_000));
    evaluator.set_capabilities(capabilities());
    let result: Result<(), TsumugiError> = evaluator.run(&program, source.len() as u64);
    result.expect("tree run");
    evaluator.budget_usage()
}

fn vm_usage(source: &str) -> BudgetUsage {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("parse");
    let chunk = Compiler::new().compile(&program).expect("compile");
    let mut vm = Vm::new(chunk);
    vm.set_capabilities(capabilities());
    vm.set_max_steps(100_000);
    vm.charge_link(source.len() as u64, &[])
        .expect("charge_link");
    vm.run().expect("vm run");
    vm.budget_usage()
}

/// PNC-AT-02（計上）: `print("ab")` の後の `output_bytes` が 3、`print()` の後が 1 で、
/// tree と VM で一致する。同じ量を計上する host request / host call の bytes も改行を含む。
#[test]
fn pnc_at_02_output_bytes_include_newline_and_match_across_engines() {
    for (source, expected) in [("print(\"ab\")\n", 3u64), ("print()\n", 1u64)] {
        let tree = tree_usage(source);
        let vm = vm_usage(source);
        assert_eq!(tree.committed.output_bytes, expected, "tree: {source}");
        assert_eq!(vm.committed.output_bytes, expected, "VM: {source}");
        assert_eq!(
            tree.committed.host_request_bytes, vm.committed.host_request_bytes,
            "host_request_bytes: {source}"
        );
        assert_eq!(
            tree.committed.host_call_bytes, vm.committed.host_call_bytes,
            "host_call_bytes: {source}"
        );
        assert_eq!(tree.committed.output_calls, vm.committed.output_calls);
    }
}

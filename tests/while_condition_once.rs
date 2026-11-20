//! tree の `while` は条件を反復ごとに 1 回だけ評価する（fix-tree-while-condition-once、
//! execution-control §14「fix-tree-while-condition-once 実装スライス」、language-spec「while」）。
//!
//! - TWC-AT-01: 条件の副作用の回数と stdout が tree と VM で一致する（CLI と in-process の両方）。
//! - TWC-AT-02: 条件にユーザー関数の呼び出しがある `while` の消費 fuel と上限の境界が一致する。
//! - TWC-AT-03: issue #393 のプログラムの終了理由（kind・message・行）が fuel 1〜30 と 20,000 で一致する。
//!   `tests/random_input_robustness.rs` の `known_divergence_step_limit_line_after_caught_error` も同じ。
//!
//! TWC-AT-04（slice yield をまたいでも同じ）は `tests/engine_api.rs` の `twc_at_04_*`。

use std::num::NonZeroU128;
use std::process::Command;
use std::sync::{Arc, Mutex};

use tsumugi::budget::BudgetConfig;
use tsumugi::capability::{CapabilitySet, Output};
use tsumugi::compiler::Compiler;
use tsumugi::error::TsumugiError;
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;

struct CapturingOutput {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl Output for CapturingOutput {
    fn policy_id(&self) -> NonZeroU128 {
        NonZeroU128::new(1).expect("non-zero")
    }
    fn write_all(&self, bytes: &[u8]) -> Result<(), tsumugi::AdapterError> {
        self.buffer
            .lock()
            .expect("buffer lock")
            .extend_from_slice(bytes);
        Ok(())
    }
    fn flush(&self) -> Result<(), tsumugi::AdapterError> {
        Ok(())
    }
}

fn capabilities(buffer: &Arc<Mutex<Vec<u8>>>) -> CapabilitySet {
    CapabilitySet::builder()
        .stdout(Arc::new(CapturingOutput {
            buffer: Arc::clone(buffer),
        }))
        .expect("stdout grant")
        .build()
}

/// 実行結果: stdout と、失敗なら (kind, message, line)。
type Run = (String, Result<(), (String, String, usize)>);

fn summarize(stdout: &Arc<Mutex<Vec<u8>>>, result: Result<(), TsumugiError>) -> Run {
    let out = String::from_utf8_lossy(&stdout.lock().expect("lock")).into_owned();
    let result = result.map_err(|e| (format!("{:?}", e.kind()), e.message().to_owned(), e.line()));
    (out, result)
}

fn run_tree(source: &str, fuel: u64) -> Run {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("パースに失敗");
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let mut evaluator = Evaluator::with_budget(BudgetConfig::for_legacy(fuel, 1_000_000));
    evaluator.set_capabilities(capabilities(&buffer));
    let result = evaluator.run(&program, source.len() as u64);
    summarize(&buffer, result)
}

fn run_vm(source: &str, fuel: u64) -> Run {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("パースに失敗");
    let chunk = Compiler::new().compile(&program).expect("compile に失敗");
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let mut vm = Vm::with_budget(chunk, BudgetConfig::for_legacy(fuel, 1_000_000));
    vm.set_capabilities(capabilities(&buffer));
    let result = vm
        .charge_link(source.len() as u64, &[])
        .and_then(|_tokens| vm.run());
    summarize(&buffer, result)
}

const BIG: u64 = 1_000_000;

/// 条件が `print("c")` するユーザー関数 `t`。引数をそのまま返す。
const T_PRELUDE: &str = "fn t(x)\n  print(\"c\")\n  return x\nend\n";

fn assert_same_stdout(label: &str, source: &str, expected_c: usize) {
    let tree = run_tree(source, BIG);
    let vm = run_vm(source, BIG);
    assert_eq!(tree, vm, "[{label}] tree と VM の結果が違う\n{source}");
    assert!(tree.1.is_ok(), "[{label}] 失敗した: {:?}\n{source}", tree.1);
    let c = tree.0.matches('c').count();
    assert_eq!(c, expected_c, "[{label}] 条件の評価回数\n{source}");
}

/// TWC-AT-01: 本体が n 回実行されるとき、条件の副作用はちょうど n + 1 回（`break` なら n 回）。
#[test]
fn twc_at_01_condition_side_effects_run_once_per_iteration() {
    for n in 0..=2usize {
        // ユーザー関数の副作用
        let src = format!("{T_PRELUDE}let n = 0\nwhile t(n < {n})\n  n = n + 1\nend\n");
        assert_same_stdout(&format!("user fn n={n}"), &src, n + 1);
        // builtin の副作用
        let src = format!("let n = 0\nwhile print(\"c\") == null and n < {n}\n  n = n + 1\nend\n");
        assert_same_stdout(&format!("builtin n={n}"), &src, n + 1);
    }
    // 初回の反復で break: 条件 1 回
    let src = format!("{T_PRELUDE}while t(true)\n  break\nend\n");
    assert_same_stdout("break", &src, 1);
    // n = 2 の途中で break: 条件は本体の実行回数と同じ 2 回
    let src = format!(
        "{T_PRELUDE}let n = 0\nwhile t(true)\n  n = n + 1\n  if n == 2\n    break\n  end\nend\n"
    );
    assert_same_stdout("break at 2", &src, 2);
    // 初回の反復で continue
    let src = format!(
        "{T_PRELUDE}let n = 0\nwhile t(n < 2)\n  n = n + 1\n  continue\n  n = 100\nend\nprint(n)\n"
    );
    let tree = run_tree(&src, BIG);
    assert_eq!(tree, run_vm(&src, BIG));
    assert_eq!(tree.0, "c\nc\nc\n2\n");
    // 入れ子と try の中
    let src = format!(
        "{T_PRELUDE}let n = 0\ntry\n  while t(n < 2)\n    let m = 0\n    while t(m < 1)\n      m = m + 1\n    end\n    n = n + 1\n  end\ncatch e\nend\n"
    );
    let tree = run_tree(&src, BIG);
    assert_eq!(tree, run_vm(&src, BIG));
    // 外側 3 回 + 内側 (2 + 2) 回 = 7 回
    assert_eq!(tree.0.matches('c').count(), 7);
}

/// TWC-AT-01（CLI）: 既定の safe profile で、tree と `--vm` の stdout が一致する。
#[test]
fn twc_at_01_cli_stdout_matches_between_backends() {
    let source = format!("{T_PRELUDE}let n = 0\nwhile t(n < 1)\n  n = n + 1\nend\n");
    let dir = std::env::temp_dir().join(format!("twc_at_01_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("cond.tsg");
    std::fs::write(&script, &source).expect("write script");
    let mut outputs = Vec::new();
    for use_vm in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
        if use_vm {
            command.arg("--vm");
        }
        let output = command.arg(&script).output().expect("起動に失敗");
        assert_eq!(output.status.code(), Some(0), "vm={use_vm}");
        outputs.push(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(outputs[0], outputs[1]);
    assert_eq!(outputs[0].matches('c').count(), 2);
}

/// `run` が fuel 上限 `n` で成功する最小の `n`（二分探索）。
fn min_fuel(label: &str, run: impl Fn(u64) -> bool) -> u64 {
    const MAX: u64 = 1 << 20;
    assert!(run(MAX), "[{label}] fuel {MAX} でも成功しない");
    if run(0) {
        return 0;
    }
    let (mut lo, mut hi) = (0u64, MAX);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if run(mid) {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    hi
}

fn assert_fuel_parity(label: &str, source: &str) -> u64 {
    let tree = min_fuel(label, |n| run_tree(source, n).1.is_ok());
    let vm = min_fuel(label, |n| run_vm(source, n).1.is_ok());
    assert_eq!(tree, vm, "[{label}] 消費 fuel が違う\n{source}");
    if tree > 0 {
        let t = run_tree(source, tree - 1);
        let v = run_vm(source, tree - 1);
        assert!(t.1.is_err(), "[{label}] N-1 で tree が失敗しない");
        assert_eq!(t, v, "[{label}] N-1 の終了理由が違う\n{source}");
    }
    tree
}

/// TWC-AT-02: 条件にユーザー関数の呼び出しがある `while` の fuel が一致し、境界も一致する。
#[test]
fn twc_at_02_fuel_matches_and_boundary_agrees() {
    let prelude = "fn f(p)\n  return p\nend\n";
    for k in 1..=3 {
        let src = format!("{prelude}let n = 0\nwhile f(n < {k})\n  n = n + 1\nend\n");
        assert_fuel_parity(&format!("k={k}"), &src);
    }
    // `while f(n < 1)` は fuel 3 で成功し、2 で limit。
    let src = format!("{prelude}let n = 0\nwhile f(n < 1)\n  n = n + 1\nend\n");
    assert_eq!(assert_fuel_parity("k=1 exact", &src), 3);
    let two = run_tree(&src, 2);
    assert_eq!(
        two.1.as_ref().err().map(|e| e.0.as_str()),
        Some("Some(StepLimit)")
    );
    // 入れ子の while と try の中
    let src = format!(
        "{prelude}let n = 0\nwhile f(n < 2)\n  let m = 0\n  while f(m < 2)\n    m = m + 1\n  end\n  n = n + 1\nend\n"
    );
    assert_fuel_parity("nested", &src);
    let src =
        format!("{prelude}let n = 0\ntry\n  while f(n < 2)\n    n = n + 1\n  end\ncatch e\nend\n");
    assert_fuel_parity("in try", &src);
    // 偽で始まる条件・呼び出しの無い条件は変わらない
    let src = format!("{prelude}while f(false)\n  let x = 1\nend\n");
    assert_eq!(assert_fuel_parity("false first", &src), 1);
    assert_eq!(
        assert_fuel_parity("no call", "let n = 0\nwhile n < 1\n  n = n + 1\nend\n"),
        1
    );
}

/// issue #393 のプログラム。
const ISSUE_393: &str = "let xs = [1, 2, 3]\nfn f(p)\n  return p\nend\ntry\n  while f(f)\n    try\n      while xs\n        a = f(g)\n      end\n    catch e\n    end\n  end\ncatch e\nend\n";

/// TWC-AT-03: issue #393 の終了理由が fuel 1〜30 と 20,000 で tree と VM で完全に一致する。
#[test]
fn twc_at_03_issue_393_terminal_matches_across_fuel() {
    for fuel in (1..=30).chain([20_000]) {
        let tree = run_tree(ISSUE_393, fuel);
        let vm = run_vm(ISSUE_393, fuel);
        assert_eq!(tree, vm, "fuel {fuel}");
    }
}

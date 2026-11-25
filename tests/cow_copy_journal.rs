//! fix-cow-copy-parity-journal（CCJ-AT-01..04）: transaction の journal が、その入力・execution で
//! 作った cell の元の値を握らず、最初の push・pop・index 代入が一意所有の backing を複製しない
//! （tree と VM、`Engine::execute` で揃える）。
//!
//! 仕様: `docs/execution-control.md`「fix-cow-copy-parity-journal 実装スライス」。
//! CCJ-AT-04（既存の挙動の維持）は `cargo test` 全体で確かめる（既存テストの期待値は変えていない）。

use std::sync::Arc;

use tsumugi::budget::BudgetConfig;
use tsumugi::compiler::Compiler;
use tsumugi::error::TsumugiError;
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;
use tsumugi::{Engine, ExecutionContext, ExecutionOutcome};

const N: u64 = 1000;

/// List body（要素数 n）の論理サイズ（`tests/scaling.rs` の `ccp_list_body` と同じ）。
fn list_body(n: u64) -> u64 {
    24 + 32 * n
}

fn parse(src: &str) -> tsumugi::ast::Program {
    Parser::new(Lexer::new(src).tokenize())
        .parse()
        .expect("パースに失敗")
}

/// REPL の継続する 1 つの engine。入力ごとに budget を据え直す。
enum Repl {
    Tree(Box<Evaluator>),
    Vm(Box<Vm>, Box<Compiler>),
}

impl Repl {
    fn new(use_vm: bool) -> Self {
        if use_vm {
            Repl::Vm(Box::new(Vm::new_repl()), Box::new(Compiler::new()))
        } else {
            Repl::Tree(Box::new(Evaluator::new()))
        }
    }

    /// `fuel` が `None` のときは十分大きい上限で実行する。
    fn submit_with(&mut self, src: &str, fuel: Option<u64>) -> Result<(), TsumugiError> {
        let config = BudgetConfig::for_legacy(fuel.unwrap_or(10_000_000), 10_000_000);
        let program = parse(src);
        match self {
            Repl::Tree(evaluator) => {
                evaluator.rearm_budget_for_repl(
                    config,
                    Arc::new(tsumugi::SystemMonotonicClock::new()),
                    tsumugi::CancellationToken::new(),
                );
                evaluator.run_repl_submission(&program, src.len() as u64)
            }
            Repl::Vm(vm, compiler) => {
                vm.begin_repl_submission(config);
                vm.charge_link_graph(src.len() as u64, &[])?;
                let checkpoint = compiler.clone();
                let chunk = compiler.compile_repl_line(&program)?;
                let result = vm.run_repl_chunk(chunk);
                if result.is_err() {
                    *compiler = checkpoint;
                }
                result
            }
        }
    }

    fn submit(&mut self, src: &str) -> Result<(), TsumugiError> {
        self.submit_with(src, None)
    }

    fn usage(&self) -> tsumugi::budget::BudgetUsage {
        match self {
            Repl::Tree(evaluator) => evaluator.budget_usage(),
            Repl::Vm(vm, _) => vm.budget_usage(),
        }
    }
}

fn init_of(op: &str) -> String {
    if op == "dict_set" {
        let entries = (0..N)
            .map(|i| format!("\"k{i}\": {i}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{{entries}}}")
    } else {
        format!("range(0, {N})")
    }
}

fn stmt_of(op: &str) -> &'static str {
    match op {
        "push" => "push(xs, 1)",
        "pop" => "pop(xs)",
        "list_set" => "xs[0] = 7",
        "dict_set" => "xs[\"k0\"] = 7",
        _ => unreachable!(),
    }
}

/// その入力の中で値を作って書き換える入力の形。`k` = 0 は書き換えない入力（基準）。
fn input_of(place: &str, op: &str, k: usize) -> String {
    let init = init_of(op);
    let body = |indent: &str| {
        (0..k)
            .map(|_| format!("{indent}{}\n", stmt_of(op)))
            .collect::<String>()
    };
    match place {
        // その入力の top-level の binding。
        "top" => format!("let xs = {init}\n{}", body("")),
        // 関数の local。
        "local" => format!(
            "fn f()\n  let xs = {init}\n{}  return 0\nend\nf()\n",
            body("  ")
        ),
        // 同じ入力で closure に捕捉した binding（VM では cell に昇格する）。
        "captured" => format!(
            "let xs = {init}\nfn g()\n  return len(xs)\nend\n{}g()\n",
            body("")
        ),
        // 同じ入力で作った closure の中から書き換える。
        "closure" => format!(
            "let xs = {init}\nfn f()\n{}  return 0\nend\nf()\n",
            body("  ")
        ),
        _ => unreachable!(),
    }
}

const PLACES: [&str; 4] = ["top", "local", "captured", "closure"];
const OPS: [&str; 4] = ["push", "pop", "list_set", "dict_set"];

fn repl_peak(use_vm: bool, second: &str) -> u64 {
    let mut repl = Repl::new(use_vm);
    repl.submit("let z = 1\n").expect("入力 1");
    repl.submit(second).expect("入力 2");
    repl.usage().peak_heap_bytes
}

fn engine_peak(source: &str) -> u64 {
    let engine = Engine::new();
    let script = engine.compile(source).expect("compile");
    let mut context = ExecutionContext::new();
    assert_eq!(
        engine.execute(&script, &mut context),
        Ok(ExecutionOutcome::Completed)
    );
    context.budget_usage().peak_heap_bytes
}

/// CCJ-AT-01: その入力・execution で作った cell の最初の書き換えは、一意所有の backing を複製しない。
#[test]
fn ccj_at_01_first_write_on_created_cell_does_not_copy() {
    let body = list_body(N - 1);
    for place in PLACES {
        for op in OPS {
            let base_second = input_of(place, op, 0);
            let base_repl = [false, true].map(|vm| repl_peak(vm, &base_second));
            let base_engine = engine_peak(&base_second);
            for k in [1usize, 5] {
                let second = input_of(place, op, k);
                for (use_vm, base) in [false, true].into_iter().zip(base_repl) {
                    let mode = if use_vm { "VM" } else { "tree" };
                    let peak = repl_peak(use_vm, &second);
                    assert!(
                        peak < base + body,
                        "REPL {mode}/{place}/{op} x{k}: backing を複製している (base={base}, peak={peak})"
                    );
                }
                let peak = engine_peak(&second);
                assert!(
                    peak < base_engine + body,
                    "Engine::execute/{place}/{op} x{k}: backing を複製している (base={base_engine}, peak={peak})"
                );
            }
        }
    }
}

/// CCJ-AT-01: `let ys = xs` で共有した場合と前の入力で作った値は、両 engine とも 1 回だけ複製し、
/// `ys` と rollback 後の値は変わらない。
#[test]
fn ccj_at_01_shared_and_previous_input_values_copy_once() {
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        let shared = |k: usize| {
            format!(
                "let xs = range(0, {N})\nlet ys = xs\n{}if len(ys) != {N}\n  let z = 1 / 0\nend\n",
                "push(xs, 1)\n".repeat(k)
            )
        };
        let (base, one, five) = (
            repl_peak(use_vm, &shared(0)),
            repl_peak(use_vm, &shared(1)),
            repl_peak(use_vm, &shared(5)),
        );
        assert!(one >= base + list_body(N), "{mode}: 共有中が複製しない");
        assert!(
            five < one + list_body(N),
            "{mode}: 共有中が 2 回目以降も複製"
        );

        let previous = |k: usize| {
            let mut repl = Repl::new(use_vm);
            repl.submit(&format!("let xs = range(0, {N})\n")).unwrap();
            repl.submit(&"push(xs, 1)\n".repeat(k)).unwrap();
            repl.usage().peak_heap_bytes
        };
        let (base, one, five) = (previous(0), previous(1), previous(5));
        assert!(
            one >= base + list_body(N),
            "{mode}: 前の入力の値が複製しない"
        );
        assert!(
            five < one + list_body(N),
            "{mode}: 前の入力の値が 2 回目以降も複製"
        );
    }
}

/// CCJ-AT-02: REPL の入力ごとに、tree と VM で入力が成功する最小の fuel が一致し、N − 1 で
/// 同じ terminal の失敗になる。深度の読み直しが起きる形（最も深い要素を `pop` で取り除く）を含める。
#[test]
fn ccj_at_02_fuel_parity_between_engines() {
    fn run(use_vm: bool, second: &str, fuel: u64) -> Result<(), String> {
        let mut repl = Repl::new(use_vm);
        repl.submit("let z = 1\n").expect("入力 1");
        repl.submit_with(second, Some(fuel))
            .map_err(|e| format!("{e:?}"))
    }
    fn min_fuel(use_vm: bool, second: &str) -> u64 {
        let (mut lo, mut hi) = (0u64, 1u64 << 22);
        assert!(run(use_vm, second, hi).is_ok(), "上限 {hi} でも失敗する");
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if run(use_vm, second, mid).is_ok() {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        hi
    }
    let mut sources = Vec::new();
    for place in PLACES {
        for op in OPS {
            for k in [1usize, 2] {
                sources.push((format!("{place}/{op}/x{k}"), input_of(place, op, k)));
            }
        }
    }
    // issue #517 の表の形（`xs[0] = 1`、関数の local、要素数 40）。
    sources.push((
        "issue517 xs[0]".into(),
        "let xs = range(0, 40)\nxs[0] = 1\n".into(),
    ));
    sources.push((
        "issue517 local".into(),
        "fn f()\n  let ys = range(0, 40)\n  push(ys, 1)\n  return len(ys)\nend\nf()\n".into(),
    ));
    // 深度の読み直しが起きる形。
    let flat = (0..50)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    for pops in [1usize, 2] {
        sources.push((
            format!("深度 top x{pops}"),
            format!("let xs = [{flat}, [[1]]]\n{}", "pop(xs)\n".repeat(pops)),
        ));
        sources.push((
            format!("深度 captured x{pops}"),
            format!(
                "let xs = [{flat}, [[1]], [2]]\nfn g()\n  return len(xs)\nend\n{}g()\n",
                "pop(xs)\n".repeat(pops)
            ),
        ));
    }
    for (label, source) in &sources {
        let tree = min_fuel(false, source);
        let vm = min_fuel(true, source);
        assert_eq!(tree, vm, "[{label}] 消費 fuel が tree/VM で不一致");
        if tree > 0 {
            assert!(run(false, source, tree - 1).is_err(), "[{label}] tree N-1");
            assert!(run(true, source, tree - 1).is_err(), "[{label}] VM N-1");
        }
    }
}

/// CCJ-AT-02: 同じ script の `Engine::execute` と `Evaluator::run` の `committed.fuel` が一致する。
#[test]
fn ccj_at_02_engine_run_fuel_matches_evaluator_run() {
    for place in PLACES {
        for op in OPS {
            let source = input_of(place, op, 3);
            let engine = Engine::new();
            let script = engine.compile(&source).expect("compile");
            let mut context = ExecutionContext::new();
            assert_eq!(
                engine.execute(&script, &mut context),
                Ok(ExecutionOutcome::Completed)
            );
            let engine_fuel = context.budget_usage().committed.fuel;

            let mut evaluator = Evaluator::new();
            evaluator
                .run(&parse(&source), source.len() as u64)
                .expect("Evaluator::run");
            let evaluator_fuel = evaluator.budget_usage().committed.fuel;
            assert!(engine_fuel > 0, "[{place}/{op}] fuel を測れていない");
            assert_eq!(
                engine_fuel, evaluator_fuel,
                "[{place}/{op}] Engine と Evaluator"
            );
        }
    }
}

/// CCJ-AT-03: REPL の rollback。作った binding・captured cell を書き換えて失敗する入力は開始時点へ戻り、
/// 前の入力からある binding も元の値へ戻る。
#[test]
fn ccj_at_03_repl_rollback_in_both_engines() {
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        let mut repl = Repl::new(use_vm);
        repl.submit("let xs = range(0, 10)\nlet h = [1, 2]\n")
            .unwrap();
        // 前の入力からある xs を closure 経由・直接書き換え、作った binding・captured cell も書き換えて失敗する。
        let failing = "let w = range(0, 5)\nfn g()\n  return len(w)\nend\npush(w, 1)\nw[0] = 9\n\
                       fn f()\n  push(xs, 1)\n  xs[0] = 99\n  return 0\nend\nf()\npush(xs, 2)\n\
                       h[0] = w\nlet boom = 1 / 0\n";
        repl.submit(failing)
            .expect_err(&format!("{mode}: 失敗する入力"));
        // 作った binding は残らない。
        repl.submit("w\n")
            .expect_err(&format!("{mode}: w は残らない"));
        // 前の入力からある binding は元の値へ戻る。
        repl.submit(
            "if len(xs) != 10 or xs[0] != 0 or xs[9] != 9\n  let boom = 1 / 0\nend\n\
             if len(h) != 2 or h[0] != 1\n  let boom = 1 / 0\nend\n",
        )
        .unwrap_or_else(|e| panic!("{mode}: xs・h が戻っていない: {e:?}"));
        // 同じ名前を定義し直せる。
        repl.submit("let w = range(0, 3)\npush(w, 1)\nif len(w) != 4\n  let boom = 1 / 0\nend\n")
            .unwrap_or_else(|e| panic!("{mode}: 定義し直せない: {e:?}"));
    }
}

/// CCJ-AT-03: VM で前の入力からある top-level の slot を入力の中で昇格して書き換えて失敗する入力も戻る。
#[test]
fn ccj_at_03_vm_promoted_previous_slot_rolls_back() {
    for use_vm in [false, true] {
        let mode = if use_vm { "VM" } else { "tree" };
        let mut repl = Repl::new(use_vm);
        repl.submit("let xs = range(0, 100)\n").unwrap();
        repl.submit(
            "fn f()\n  push(xs, 1)\n  pop(xs)\n  pop(xs)\n  return len(xs)\nend\nf()\nlet boom = 1 / 0\n",
        )
        .expect_err("失敗する入力");
        repl.submit("if len(xs) != 100 or xs[99] != 99\n  let boom = 1 / 0\nend\nxs[0] = 5\n")
            .unwrap_or_else(|e| panic!("{mode}: 昇格した slot が戻っていない: {e:?}"));
    }
}

/// CCJ-AT-03: `Engine::execute` で同じ context を再利用し、失敗する execution の後に context が
/// execution 開始時点のままである。
#[test]
fn ccj_at_03_engine_context_rolls_back_after_failure() {
    let engine = Engine::new();
    let mut context = ExecutionContext::new();
    let run = |context: &mut ExecutionContext, src: &str| {
        let script = engine.compile(src).expect("compile");
        engine.execute(&script, context)
    };
    run(&mut context, "let xs = range(0, 10)\nlet h = [1, 2]\n").expect("定義");
    run(
        &mut context,
        "let w = range(0, 5)\nfn g()\n  return len(w)\nend\npush(w, 1)\nw[0] = 9\n\
         fn f()\n  push(xs, 1)\n  xs[0] = 99\n  return 0\nend\nf()\nh[0] = w\nlet boom = 1 / 0\n",
    )
    .expect_err("失敗する execution");
    let error = run(&mut context, "w\n").expect_err("w は残らない");
    assert_eq!(error.error_type(), "name");
    run(
        &mut context,
        "if len(xs) != 10 or xs[0] != 0 or len(h) != 2 or h[0] != 1\n  let boom = 1 / 0\nend\n",
    )
    .expect("xs・h が開始時点のまま");
}

//! vm-parity-diff-a（VPD-AT-01..06）: tree と VM の網羅的な差分テストの土台と、既知の差分一覧の gate。
//!
//! 仕様: `docs/determinism-and-audit.md`「vm-parity-diff-a 実装スライス」、開発ループ手順書 §6.7。
//!
//! 1 つのケース（単発の source、REPL の入力の列、または `Engine::execute` の `ExecutionContext` を
//! 使い回す入力の列）を tree と VM で流し、入力ごとに次の軸で比べる。
//!
//! - **値**: 各ケースは観測したい値を `print` で出す。値は出力の比較で比べる。
//! - **error**（`Axis::Error`）: terminal の `TsumugiError` の全 field（kind・message・line・call trace）。
//!   compile error も含む（VM は compile error を、tree と同じく root source を課金した後の terminal とする）。
//! - **出力**（`Axis::Output`）: 出力の byte 列。in-process で byte 列を取れる（`Output` を実装した
//!   capture を両 engine の stdout へ grant する）ので、I/O の counter で代えずに byte 列を exact に比べる。
//! - **fuel**（`Axis::Fuel`）: `committed.fuel` を exact に比べる。REPL は入力ごとの値。
//! - **budget の各カウンタ**（`Axis::Counters` / `Axis::Peaks`）: `committed` と `reserved` の fuel 以外の
//!   counter と、`peaks` を exact に比べる。
//! - **heap**（`Axis::Heap`）: 基準のケース（書き換えをしない対のケース）を持つものは、同じ engine の
//!   中で「ケース − 基準」の `live_heap_bytes`・`peak_heap_bytes`・`reserved_heap_bytes` の差を取り、
//!   その差を engine 間で exact に比べる（code artifact の分は差で打ち消す。`tests/cow_copy_journal.rs`
//!   と同じ方法）。基準を持たないケースは、`tests/differential.rs` と同じ向き（> 0 か）の比較にとどめる。
//! - **経路**（`Axis::Route`）: 片方の engine に実行の経路が無い（`Engine::execute` の VM）こと。
//!
//! 差は「ケース・軸・tree の値・VM の値」で報告し、`KNOWN_DIFFS`（既知の差分一覧）と完全一致させる。
//! 一覧に無い差が出ても、一覧の項目に当たる差が 1 件も出なくても test が落ちる（spec の Q2=A）。
//! 差を直すスライスは、同じ PR で一覧の項目を消す。
//!
//! VPD-AT-06（非破壊）: この test は `src/` を変えず、既存の test の期待値も変えない（PR の
//! `git diff --stat` と `cargo test` で確かめる）。
//!
//! capability は stdout だけを grant する（`tests/differential.rs` の `print_capabilities` と同じ範囲）。
//! capability の要る fixture と CLI の byte 比較は vm-parity-diff-b で扱う（`FIXTURES_FOR_DIFF_B`）。

use std::num::NonZeroU128;
use std::sync::{Arc, Mutex};

use tsumugi::budget::{BudgetConfig, BudgetCounters, BudgetUsage};
use tsumugi::capability::{AdapterError, Output};
use tsumugi::compiler::Compiler;
use tsumugi::error::TsumugiError;
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;
use tsumugi::{CapabilitySet, Engine, ExecutionContext};

// =============================================================================
// 実行器
// =============================================================================

/// 両 engine に同じ上限を渡す（fuel 1_000_000・collection 1_000_000。CLI の REPL と同じ既定）。
fn config() -> BudgetConfig {
    BudgetConfig::for_legacy(1_000_000, 1_000_000)
}

/// stdout に書かれた byte 列を貯める `Output`。
struct CaptureOutput(Arc<Mutex<Vec<u8>>>);

impl Output for CaptureOutput {
    fn policy_id(&self) -> NonZeroU128 {
        NonZeroU128::new(1).expect("non-zero")
    }
    fn write_all(&self, bytes: &[u8]) -> Result<(), AdapterError> {
        self.0.lock().expect("lock").extend_from_slice(bytes);
        Ok(())
    }
    fn flush(&self) -> Result<(), AdapterError> {
        Ok(())
    }
}

/// stdout だけを grant した capability set と、書かれた byte 列の置き場所。
fn capture_capabilities() -> (CapabilitySet, Arc<Mutex<Vec<u8>>>) {
    let sink = Arc::new(Mutex::new(Vec::new()));
    let capabilities = CapabilitySet::builder()
        .stdout(Arc::new(CaptureOutput(Arc::clone(&sink))))
        .expect("stdout grant")
        .build();
    (capabilities, sink)
}

fn take_output(sink: &Arc<Mutex<Vec<u8>>>) -> Vec<u8> {
    std::mem::take(&mut *sink.lock().expect("lock"))
}

/// ケースの流し方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// 1 入力の単発実行（tree は `Evaluator::run`、VM は `Vm::with_budget(...).run()`）。
    Single,
    /// REPL の 1 session（tree は `run_repl_submission`、VM は `compile_repl_line` / `run_repl_chunk`）。
    Repl,
    /// `Engine::execute` で 1 つの `ExecutionContext` を使い回す。VM には経路が無い。
    Context,
}

/// 1 入力の観測。
#[derive(Debug, Clone)]
struct Step {
    result: Result<(), TsumugiError>,
    output: Vec<u8>,
    usage: BudgetUsage,
}

/// 1 つの engine の観測。
#[derive(Debug, Clone)]
enum Run {
    Steps(Vec<Step>),
    /// その engine に、この流し方の経路が無い。
    NoRoute(&'static str),
}

fn parse(source: &str) -> Result<tsumugi::ast::Program, TsumugiError> {
    Parser::new(Lexer::new(source).tokenize())
        .parse()
        .map_err(|mut errors| errors.remove(0))
}

/// parse は両 engine で共有の前段なので、parse error は両方に同じ観測として返す。
fn parse_error_step(error: TsumugiError) -> Step {
    Step {
        result: Err(error),
        output: Vec::new(),
        usage: BudgetUsage::default(),
    }
}

fn run_tree(mode: Mode, inputs: &[String]) -> Run {
    let (capabilities, sink) = capture_capabilities();
    match mode {
        Mode::Single => {
            let source = &inputs[0];
            let program = match parse(source) {
                Ok(program) => program,
                Err(error) => return Run::Steps(vec![parse_error_step(error)]),
            };
            let mut evaluator = Evaluator::with_budget(config());
            evaluator.set_capabilities(capabilities);
            let result = evaluator.run(&program, source.len() as u64);
            Run::Steps(vec![Step {
                result,
                output: take_output(&sink),
                usage: evaluator.budget_usage(),
            }])
        }
        Mode::Repl => {
            let mut evaluator = Evaluator::with_budget(config());
            evaluator.set_capabilities(capabilities);
            let steps = inputs
                .iter()
                .map(|source| {
                    let program = match parse(source) {
                        Ok(program) => program,
                        Err(error) => return parse_error_step(error),
                    };
                    evaluator.rearm_budget_for_repl(
                        config(),
                        Arc::new(tsumugi::SystemMonotonicClock::new()),
                        tsumugi::CancellationToken::new(),
                    );
                    let result = evaluator.run_repl_submission(&program, source.len() as u64);
                    Step {
                        result,
                        output: take_output(&sink),
                        usage: evaluator.budget_usage(),
                    }
                })
                .collect();
            Run::Steps(steps)
        }
        Mode::Context => {
            let engine = Engine::new();
            let mut context = ExecutionContext::new();
            context.set_capabilities(capabilities);
            let steps = inputs
                .iter()
                .map(|source| {
                    let result = match engine.compile(source) {
                        Ok(script) => engine.execute(&script, &mut context).map(|_| ()),
                        Err(mut errors) => Err(errors.remove(0)),
                    };
                    Step {
                        result,
                        output: take_output(&sink),
                        usage: context.budget_usage(),
                    }
                })
                .collect();
            Run::Steps(steps)
        }
    }
}

fn run_vm(mode: Mode, inputs: &[String]) -> Run {
    let (capabilities, sink) = capture_capabilities();
    match mode {
        Mode::Single => {
            let source = &inputs[0];
            let program = match parse(source) {
                Ok(program) => program,
                Err(error) => return Run::Steps(vec![parse_error_step(error)]),
            };
            // tree の `run` は root source を link で課金してから評価する。VM の compile error も、
            // root source を課金した台帳の上の terminal として観測する（空の chunk で台帳を作る）。
            let (chunk, compile_error) = match Compiler::new().compile(&program) {
                Ok(chunk) => (chunk, None),
                Err(error) => {
                    let empty = parse("").expect("空の source");
                    (
                        Compiler::new().compile(&empty).expect("空の chunk"),
                        Some(error),
                    )
                }
            };
            let mut vm = Vm::with_budget(chunk, config());
            vm.set_capabilities(capabilities);
            let result = match vm.charge_link_graph(source.len() as u64, &[]) {
                Err(error) => Err(error),
                Ok(_tokens) => match compile_error {
                    Some(error) => Err(error),
                    None => vm.run(),
                },
            };
            Run::Steps(vec![Step {
                result,
                output: take_output(&sink),
                usage: vm.budget_usage(),
            }])
        }
        Mode::Repl => {
            let mut compiler = Compiler::new();
            let mut vm = Vm::new_repl_with_budget(config());
            vm.set_capabilities(capabilities);
            let steps = inputs
                .iter()
                .map(|source| {
                    let program = match parse(source) {
                        Ok(program) => program,
                        Err(error) => return parse_error_step(error),
                    };
                    vm.begin_repl_submission(config());
                    let result = vm
                        .charge_link_graph(source.len() as u64, &[])
                        .map(|_| ())
                        .and_then(|()| {
                            // CLI と同じく、失敗した submission の compiler の状態は巻き戻す。
                            let checkpoint = compiler.clone();
                            let result = compiler
                                .compile_repl_line(&program)
                                .and_then(|chunk| vm.run_repl_chunk(chunk));
                            if result.is_err() {
                                compiler = checkpoint;
                            }
                            result
                        });
                    Step {
                        result,
                        output: take_output(&sink),
                        usage: vm.budget_usage(),
                    }
                })
                .collect();
            Run::Steps(steps)
        }
        Mode::Context => Run::NoRoute("Engine::execute / Engine::run に VM の経路が無い"),
    }
}

// =============================================================================
// 軸ごとの比較
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Route,
    Error,
    Output,
    Fuel,
    Counters,
    Peaks,
    Heap,
}

/// 1 つの差。軸ごとに、最初に差の出た入力の値を持つ。
#[derive(Debug, Clone)]
struct Diff {
    case: String,
    axis: Axis,
    tree: String,
    vm: String,
}

impl std::fmt::Display for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} [{:?}]\n    tree: {}\n    vm:   {}",
            self.case, self.axis, self.tree, self.vm
        )
    }
}

/// fuel 以外の counter（committed / reserved で同じ並び）。
fn non_fuel_counters(counters: &BudgetCounters) -> BudgetCounters {
    BudgetCounters {
        fuel: 0,
        ..*counters
    }
}

/// heap の観測: (live, peak, reserved)。
fn heap_of(usage: &BudgetUsage) -> [u64; 3] {
    [
        usage.live_heap_bytes,
        usage.peak_heap_bytes,
        usage.reserved_heap_bytes,
    ]
}

fn heap_delta(case: &Step, base: &Step) -> [i128; 3] {
    let (c, b) = (heap_of(&case.usage), heap_of(&base.usage));
    [0, 1, 2].map(|i| i128::from(c[i]) - i128::from(b[i]))
}

/// 1 つのケースの tree と VM の観測（と、あれば基準のケースの観測）を軸ごとに比べる。
fn compare(name: &str, tree: &Run, vm: &Run, base: Option<(&Run, &Run)>) -> Vec<Diff> {
    let diff = |axis: Axis, tree: String, vm: String| Diff {
        case: name.to_string(),
        axis,
        tree,
        vm,
    };
    let (tree_steps, vm_steps) = match (tree, vm) {
        (Run::Steps(t), Run::Steps(v)) if t.len() == v.len() => (t, v),
        _ => {
            let describe = |run: &Run| match run {
                Run::Steps(steps) => format!("{} 入力を実行", steps.len()),
                Run::NoRoute(reason) => format!("経路なし（{reason}）"),
            };
            return vec![diff(Axis::Route, describe(tree), describe(vm))];
        }
    };
    let base_steps = match base {
        Some((Run::Steps(bt), Run::Steps(bv)))
            if bt.len() == tree_steps.len() && bv.len() == tree_steps.len() =>
        {
            Some((bt, bv))
        }
        Some(_) => panic!("[{name}] 基準のケースが両 engine で同じ入力数を実行しない"),
        None => None,
    };

    // 軸ごとに、入力 i の (tree, vm) の表示を作る。None なら一致。
    type Probe<'a> = Box<dyn Fn(usize, &Step, &Step) -> Option<(String, String)> + 'a>;
    let probes: Vec<(Axis, Probe)> = vec![
        (
            Axis::Error,
            Box::new(|_, t, v| {
                (t.result != v.result)
                    .then(|| (format!("{:?}", t.result), format!("{:?}", v.result)))
            }),
        ),
        (
            Axis::Output,
            Box::new(|_, t, v| {
                (t.output != v.output).then(|| {
                    (
                        format!("{:?}", String::from_utf8_lossy(&t.output)),
                        format!("{:?}", String::from_utf8_lossy(&v.output)),
                    )
                })
            }),
        ),
        (
            Axis::Fuel,
            Box::new(|_, t, v| {
                (t.usage.committed.fuel != v.usage.committed.fuel).then(|| {
                    (
                        t.usage.committed.fuel.to_string(),
                        v.usage.committed.fuel.to_string(),
                    )
                })
            }),
        ),
        (
            Axis::Counters,
            Box::new(|_, t, v| {
                let tc = (
                    non_fuel_counters(&t.usage.committed),
                    non_fuel_counters(&t.usage.reserved),
                );
                let vc = (
                    non_fuel_counters(&v.usage.committed),
                    non_fuel_counters(&v.usage.reserved),
                );
                (tc != vc).then(|| (format!("{tc:?}"), format!("{vc:?}")))
            }),
        ),
        (
            Axis::Peaks,
            Box::new(|_, t, v| {
                (t.usage.peaks != v.usage.peaks).then(|| {
                    (
                        format!("{:?}", t.usage.peaks),
                        format!("{:?}", v.usage.peaks),
                    )
                })
            }),
        ),
        (
            Axis::Heap,
            Box::new(move |i, t, v| match base_steps {
                Some((bt, bv)) => {
                    let (td, vd) = (heap_delta(t, &bt[i]), heap_delta(v, &bv[i]));
                    (td != vd).then(|| {
                        (
                            format!("基準との差 [live, peak, reserved] = {td:?}"),
                            format!("基準との差 [live, peak, reserved] = {vd:?}"),
                        )
                    })
                }
                None => {
                    let direction =
                        |s: &Step| [s.usage.live_heap_bytes > 0, s.usage.peak_heap_bytes > 0];
                    (direction(t) != direction(v)).then(|| {
                        (
                            format!("[live > 0, peak > 0] = {:?}", direction(t)),
                            format!("[live > 0, peak > 0] = {:?}", direction(v)),
                        )
                    })
                }
            }),
        ),
    ];

    let mut diffs = Vec::new();
    for (axis, probe) in &probes {
        let first = tree_steps
            .iter()
            .zip(vm_steps)
            .enumerate()
            .find_map(|(i, (t, v))| probe(i, t, v).map(|(a, b)| (i, a, b)));
        if let Some((i, t, v)) = first {
            diffs.push(diff(
                *axis,
                format!("入力{i}: {t}"),
                format!("入力{i}: {v}"),
            ));
        }
    }
    diffs
}

// =============================================================================
// ケース
// =============================================================================

#[derive(Debug, Clone)]
struct Case {
    name: String,
    mode: Mode,
    inputs: Vec<String>,
    /// 書き換えをしない基準のケース（heap の差の比較に使う）。
    baseline: Option<Vec<String>>,
}

impl Case {
    fn run(&self) -> Vec<Diff> {
        let tree = run_tree(self.mode, &self.inputs);
        let vm = run_vm(self.mode, &self.inputs);
        let base = self
            .baseline
            .as_ref()
            .map(|inputs| (run_tree(self.mode, inputs), run_vm(self.mode, inputs)));
        compare(&self.name, &tree, &vm, base.as_ref().map(|(t, v)| (t, v)))
    }
}

// -----------------------------------------------------------------------------
// 書き換えの種類 × 値の持ち方 × 変数の置き場所の matrix（VPD-AT-02）
// -----------------------------------------------------------------------------

/// 書き換えの種類。(名前, List か Dict か, 本体の行)。
const OPS: &[(&str, bool, &[&str])] = &[
    ("push", false, &["push(xs, 1)"]),
    ("pop", false, &["pop(xs)"]),
    ("list_set", false, &["xs[0] = 7"]),
    ("dict_set_existing", true, &["xs[\"k0\"] = 7"]),
    ("dict_set_new", true, &["xs[\"new\"] = 7"]),
    (
        "for_list_mutate",
        false,
        &["for x in xs", "  push(xs, x)", "end"],
    ),
    (
        "for_list_plain",
        false,
        &["let s = 0", "for x in xs", "  s = s + x", "end", "print(s)"],
    ),
    (
        "for_dict_mutate",
        true,
        &["for k in xs", "  xs[k] = 0", "end"],
    ),
    (
        "for_dict_plain",
        true,
        &[
            "let s = 0",
            "for k in xs",
            "  s = s + xs[k]",
            "end",
            "print(s)",
        ],
    ),
];

/// 値の持ち方。`shared` は `let ys = xs` の後に `xs` を書き換える。
const HOLDINGS: &[&str] = &["unique", "shared"];

/// 変数の置き場所。
const PLACES: &[&str] = &[
    "top",
    "fn_local",
    "upvalue1",
    "upvalue2",
    "repl_prev",
    "context_prev",
];

/// 要素数。複製の有無は基準との差で見るので、小さくてよい。
const ELEMENTS: usize = 20;

fn indent(lines: &[String], depth: usize) -> Vec<String> {
    let pad = " ".repeat(depth);
    lines.iter().map(|line| format!("{pad}{line}")).collect()
}

fn join(lines: &[String]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

/// matrix の 1 組の入力の列。`enabled` が false なら基準のケース。
///
/// 基準のケースは同じ code（`let on = false` だけが違う）で、書き換えの本体を `if on` で飛ばす。
/// code artifact（tree の AST・VM の chunk）の大きさは同じなので、heap の差を取ると打ち消し合う。
fn matrix_inputs(
    place: &str,
    is_dict: bool,
    shared: bool,
    body: &[&str],
    enabled: bool,
) -> (Mode, Vec<String>) {
    let init = if is_dict {
        let entries = (0..ELEMENTS)
            .map(|i| format!("\"k{i}\": {i}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{{{entries}}}")
    } else {
        format!("range(0, {ELEMENTS})")
    };
    let mut decl = vec![format!("let xs = {init}")];
    let mut tail = vec!["print(xs)".to_string()];
    if shared {
        decl.push("let ys = xs".to_string());
        tail.push("print(ys)".to_string());
    }
    let mut wrapped = vec!["if on".to_string()];
    wrapped.extend(body.iter().map(|line| format!("  {line}")));
    wrapped.push("end".to_string());
    let body = wrapped;
    let switch = format!("let on = {enabled}");
    let mut lines: Vec<String> = vec![switch.clone()];
    match place {
        "top" => {
            lines.extend(decl);
            lines.extend(body);
            lines.extend(tail);
            (Mode::Single, vec![join(&lines)])
        }
        "fn_local" => {
            lines.push("fn f()".into());
            lines.extend(indent(&decl, 2));
            lines.extend(indent(&body, 2));
            lines.extend(indent(&tail, 2));
            lines.extend(["end".into(), "f()".into()]);
            (Mode::Single, vec![join(&lines)])
        }
        "upvalue1" => {
            lines.push("fn outer()".into());
            lines.extend(indent(&decl, 2));
            lines.push("  let g = fn()".into());
            lines.extend(indent(&body, 4));
            lines.extend(["  end".into(), "  g()".into()]);
            lines.extend(indent(&tail, 2));
            lines.extend(["end".into(), "outer()".into()]);
            (Mode::Single, vec![join(&lines)])
        }
        "upvalue2" => {
            lines.push("fn outer()".into());
            lines.extend(indent(&decl, 2));
            lines.push("  let g = fn()".into());
            lines.push("    let h = fn()".into());
            lines.extend(indent(&body, 6));
            lines.extend(["    end".into(), "    h()".into()]);
            lines.extend(["  end".into(), "  g()".into()]);
            lines.extend(indent(&tail, 2));
            lines.extend(["end".into(), "outer()".into()]);
            (Mode::Single, vec![join(&lines)])
        }
        "repl_prev" | "context_prev" => {
            let mode = if place == "repl_prev" {
                Mode::Repl
            } else {
                Mode::Context
            };
            lines.extend(body);
            lines.extend(tail);
            (mode, vec![join(&decl), join(&lines)])
        }
        _ => unreachable!("未知の置き場所 {place}"),
    }
}

fn matrix_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for place in PLACES {
        for (op, is_dict, body) in OPS {
            for holding in HOLDINGS {
                let shared = *holding == "shared";
                let (mode, inputs) = matrix_inputs(place, *is_dict, shared, body, true);
                let (_, baseline) = matrix_inputs(place, *is_dict, shared, body, false);
                cases.push(Case {
                    name: format!("matrix/{place}/{op}/{holding}"),
                    mode,
                    inputs,
                    baseline: Some(baseline),
                });
            }
        }
    }
    cases
}

// -----------------------------------------------------------------------------
// REPL 継続（VPD-AT-03）
// -----------------------------------------------------------------------------

fn repl_cases() -> Vec<Case> {
    let case = |name: &str, inputs: &[&str]| Case {
        name: format!("repl/{name}"),
        mode: Mode::Repl,
        inputs: inputs.iter().map(|s| s.to_string()).collect(),
        baseline: None,
    };
    vec![
        // error の入力の途中の書き換え（前の入力の binding・新しい binding）が残らない。
        case(
            "rollback_after_error",
            &[
                "let xs = [1, 2, 3]\nlet n = 10\nlet d = {\"a\": 1}\n",
                "push(xs, 4)\nxs[0] = 99\nd[\"b\"] = 2\nn = 20\nlet fresh = 1\nprint(\"before\")\nlet boom = 1 / 0\n",
                "print(xs)\nprint(d)\nprint(n)\n",
                "print(fresh)\n",
                "let fresh = [5]\npush(fresh, 6)\nprint(fresh)\n",
            ],
        ),
        // 前の入力の closure を呼び、closure の中から前の入力の値を書き換える。
        case(
            "closure_from_previous_input",
            &[
                "fn make()\n  let c = 0\n  return fn()\n    c = c + 1\n    return c\n  end\nend\nlet counter = make()\nlet ys = [0]\nfn add_y(v)\n  push(ys, v)\n  return len(ys)\nend\n",
                "print(counter())\nprint(add_y(5))\n",
                "print(counter())\nprint(add_y(6))\nlet boom = 1 / 0\n",
                "print(counter())\nprint(ys)\n",
                "let again = counter\nprint(again())\nprint(add_y(7))\n",
            ],
        ),
        // 前の入力の変数を書き換える入力が続く。
        case(
            "rewrite_previous_bindings",
            &[
                "let xs = range(0, 5)\nlet d = {\"k\": [1]}\n",
                "push(xs, 5)\nd[\"k\"] = xs\n",
                "let ys = xs\npop(xs)\nprint(xs)\nprint(ys)\nprint(d)\n",
                "for x in xs\n  push(xs, x)\nend\nprint(len(xs))\n",
            ],
        ),
        // compile 時に見つかる error（loop の外の break など）の後も session が続く。
        case(
            "compile_error_then_continue",
            &[
                "let a = 1\n",
                "a = 2\nbreak\n",
                "print(a)\n",
                "fn f()\n  continue\nend\nprint(a)\n",
                "print(undefined_name)\n",
                "print(a + 1)\n",
            ],
        ),
    ]
}

// -----------------------------------------------------------------------------
// fixture（VPD-AT-04）
// -----------------------------------------------------------------------------

/// print だけでは動かないため外す fixture と、外す理由（要る capability）。vm-parity-diff-b の対象。
/// ここに無い `tests/fixtures/*.tsg` は自動で比べる対象に入る。
const FIXTURES_FOR_DIFF_B: &[(&str, &str)] = &[
    ("env_allow", "env"),
    ("env_protected_windows", "env"),
    ("error_sandbox", "fs（read_file）"),
    ("file_io", "env・fs"),
    ("filesystem", "env・fs"),
    ("local_utils", "env・clock（now）"),
    ("string_utils", "env・fs"),
    ("error_import_before_side_effects", "import・fs"),
    ("error_import_non_top_level", "import"),
    ("error_import_not_found", "import"),
    ("function_id_import", "import"),
    ("function_id_import_lib", "import（import される側）"),
    ("import_bad_syntax", "import（import される側）"),
    ("import_basic", "import"),
    ("import_circular", "import"),
    ("import_circular_a", "import（import される側）"),
    ("import_circular_b", "import（import される側）"),
    ("import_lib", "import（import される側）"),
    ("import_nested", "import"),
    ("import_nested_base", "import（import される側）"),
    ("import_nested_mid", "import（import される側）"),
    ("import_static_resolution", "import"),
    ("runtime_global_failed_import", "import"),
    ("runtime_global_import", "import"),
    ("runtime_global_import_lib", "import（import される側）"),
];

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// `tests/fixtures/*.tsg` の stem の一覧（名前順）。
fn fixture_stems() -> Vec<String> {
    let mut stems: Vec<String> = std::fs::read_dir(fixture_dir())
        .expect("tests/fixtures を読めない")
        .filter_map(|entry| {
            let path = entry.expect("entry").path();
            (path.extension().and_then(|e| e.to_str()) == Some("tsg"))
                .then(|| path.file_stem()?.to_str().map(str::to_string))
                .flatten()
        })
        .collect();
    stems.sort();
    stems
}

fn fixture_cases() -> Vec<Case> {
    fixture_stems()
        .into_iter()
        .filter(|stem| !FIXTURES_FOR_DIFF_B.iter().any(|(name, _)| name == stem))
        .map(|stem| {
            let source = std::fs::read_to_string(fixture_dir().join(format!("{stem}.tsg")))
                .expect("fixture を読めない");
            Case {
                name: format!("fixture/{stem}"),
                mode: Mode::Single,
                inputs: vec![source],
                baseline: None,
            }
        })
        .collect()
}

// =============================================================================
// 既知の差分一覧と gate（VPD-AT-05）
// =============================================================================

/// 既知の差分の 1 項目。`pattern` は case の名前で、`*` は任意の文字列（空を含む）に当たる。
#[derive(Debug, Clone, Copy)]
struct KnownDiff {
    id: &'static str,
    axis: Axis,
    pattern: &'static str,
    description: &'static str,
}

/// `*` だけを特別扱いする glob。
fn glob_match(pattern: &str, text: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == text,
        Some((head, rest)) => {
            let Some(text) = text.strip_prefix(head) else {
                return false;
            };
            (0..=text.len())
                .filter(|&i| text.is_char_boundary(i))
                .any(|i| glob_match(rest, &text[i..]))
        }
    }
}

impl KnownDiff {
    fn matches(&self, diff: &Diff) -> bool {
        self.axis == diff.axis && glob_match(self.pattern, &diff.case)
    }
}

/// 今ある tree と VM の差（vm-parity-diff-a の実装 PR で test を流して作った）。
/// 差を直すスライスは、同じ PR でその項目を消す。0 件になったらフェーズの終わり（手順書 §6.7）。
const KNOWN_DIFFS: &[KnownDiff] = &[
    KnownDiff {
        id: "KD-1",
        axis: Axis::Route,
        pattern: "matrix/context_prev/*",
        description: "Engine::execute / Engine::run の ExecutionContext に VM の経路が無い（VmExperimental でも tree で実行する。embedding-api の逸脱一覧、延期先 E9 の残り）",
    },
    KnownDiff {
        id: "KD-2",
        axis: Axis::Heap,
        pattern: "matrix/*/for_list_*",
        description: "for で List を反復すると、基準との peak_heap_bytes の差が tree の方が大きい（単発で 160〜192 bytes、REPL で 208〜288 bytes。live は一致）",
    },
    KnownDiff {
        id: "KD-3",
        axis: Axis::Heap,
        pattern: "matrix/*/for_dict_*",
        description: "for で Dict を反復すると、基準との peak_heap_bytes の差が tree の方が大きい（単発で 160〜192 bytes、REPL で 208〜288 bytes。live は一致）",
    },
    KnownDiff {
        id: "KD-4",
        axis: Axis::Fuel,
        pattern: "repl/*",
        description: "REPL で error で終わった入力の committed.fuel が、VM は 0（run_repl_chunk が入力開始時点へ戻す。compile error は実行しない）、tree は消費した分を残す",
    },
];

/// `diffs` を `known` と突き合わせ、問題（一覧に無い差・差の出ない項目）を返す。
/// `scope` は今回流したケースの名前の接頭辞で、その範囲の項目だけを「差の出ない項目」の対象にする。
fn gate(diffs: &[Diff], known: &[KnownDiff], scope: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for diff in diffs {
        if !known.iter().any(|item| item.matches(diff)) {
            problems.push(format!("一覧に無い差: {diff}"));
        }
    }
    for item in known.iter().filter(|item| item.pattern.starts_with(scope)) {
        if !diffs.iter().any(|diff| item.matches(diff)) {
            problems.push(format!(
                "差の出なくなった項目が一覧に残っている: {} [{:?}] {} — {}",
                item.id, item.axis, item.pattern, item.description
            ));
        }
    }
    problems
}

/// tree の再帰評価のため、大きいスタックの thread で流す。
fn with_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(f)
        .expect("thread")
        .join()
        .expect("thread panicked")
}

/// ケースを流して gate にかけ、問題があれば全件を出して落とす。
fn assert_gate(scope: &'static str, cases: fn() -> Vec<Case>) {
    let diffs = with_stack(move || cases().iter().flat_map(Case::run).collect::<Vec<_>>());
    let problems = gate(&diffs, KNOWN_DIFFS, scope);
    assert!(
        problems.is_empty(),
        "{scope}: 既知の差分一覧と実際の差が一致しない（{} 件）\n{}",
        problems.len(),
        problems.join("\n")
    );
}

// =============================================================================
// テスト
// =============================================================================

/// VPD-AT-01: わざと差を作った組（tree と VM に違う source を渡す）で、各軸の差を
/// 「ケース・軸・tree の値・VM の値」で報告する。経路の欠けも報告する。
#[test]
fn vpd_at_01_compare_reports_each_axis() {
    let diffs = with_stack(|| {
        let tree_src = vec!["let xs = range(0, 3)\nprint(1)\n".to_string()];
        let vm_src = vec![
            "let xs = range(0, 300)\nlet s = upper(\"ab\")\nfor i in range(0, 3)\nend\nprint(22)\nlet z = 1 / 0\n"
                .to_string(),
        ];
        let base = vec!["let q = 0\n".to_string()];
        let tree = run_tree(Mode::Single, &tree_src);
        let vm = run_vm(Mode::Single, &vm_src);
        let (tree_base, vm_base) = (run_tree(Mode::Single, &base), run_vm(Mode::Single, &base));
        let mut diffs = compare("probe", &tree, &vm, Some((&tree_base, &vm_base)));
        // 同じ source なら差は出ない。
        assert!(compare("same", &tree, &run_tree(Mode::Single, &tree_src), None).is_empty());
        // 経路の欠け。
        let inputs = vec!["print(1)\n".to_string()];
        diffs.extend(compare(
            "route",
            &run_tree(Mode::Context, &inputs),
            &run_vm(Mode::Context, &inputs),
            None,
        ));
        diffs
    });
    for axis in [
        Axis::Error,
        Axis::Output,
        Axis::Fuel,
        Axis::Counters,
        Axis::Peaks,
        Axis::Heap,
    ] {
        let diff = diffs
            .iter()
            .find(|d| d.case == "probe" && d.axis == axis)
            .unwrap_or_else(|| panic!("{axis:?} の差を報告しない: {diffs:#?}"));
        assert_ne!(diff.tree, diff.vm);
        assert!(diff.tree.starts_with("入力0: "), "{diff}");
    }
    let route = diffs.iter().find(|d| d.case == "route").expect("経路の差");
    assert_eq!(route.axis, Axis::Route);
    assert!(route.vm.contains("経路なし"), "{route}");
}

/// VPD-AT-02: matrix のケースが、書き換えの種類 × 値の持ち方 × 変数の置き場所の全組で生成され、
/// 各ケースが基準のケースを対で持つ。
#[test]
fn vpd_at_02_matrix_covers_every_combination() {
    let cases = matrix_cases();
    assert_eq!(OPS.len(), 9);
    assert_eq!(HOLDINGS.len(), 2);
    assert_eq!(PLACES.len(), 6);
    assert_eq!(cases.len(), OPS.len() * HOLDINGS.len() * PLACES.len());
    let mut names: Vec<&str> = cases.iter().map(|c| c.name.as_str()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), cases.len(), "ケースの名前が重複している");
    for case in &cases {
        let baseline = case.baseline.as_ref().expect("基準のケース");
        assert_eq!(baseline.len(), case.inputs.len(), "{}", case.name);
        assert_ne!(baseline, &case.inputs, "{}", case.name);
    }
    // `Engine::execute` の context の置き場所は、VM 側が経路なしとして既知の差分一覧の 1 項目に当たる。
    assert!(
        KNOWN_DIFFS
            .iter()
            .any(|k| k.axis == Axis::Route && k.pattern == "matrix/context_prev/*"),
        "context の置き場所の経路なしが一覧に無い"
    );
}

/// VPD-AT-02・05: matrix の全ケースを全軸で比べ、差が既知の差分一覧と完全一致する。
#[test]
fn vpd_at_02_matrix_matches_known_diffs() {
    assert_gate("matrix/", matrix_cases);
}

/// VPD-AT-03・05: REPL 継続のケースを入力ごとに全軸で比べ、差が既知の差分一覧と完全一致する。
#[test]
fn vpd_at_03_repl_continuation_matches_known_diffs() {
    let cases = repl_cases();
    // error の入力・前の入力の変数の書き換え・前の入力の closure の呼び出しを含む。
    assert!(cases.iter().any(|c| c.name == "repl/rollback_after_error"));
    assert!(
        cases
            .iter()
            .any(|c| c.name == "repl/closure_from_previous_input")
    );
    assert!(
        cases
            .iter()
            .any(|c| c.name == "repl/rewrite_previous_bindings")
    );
    assert_gate("repl/", repl_cases);
}

/// VPD-AT-04: 外す一覧はすべて実在する fixture を指し、外した理由（要る capability）を持つ。
/// 外す一覧に無い fixture は、ファイルの一覧から自動で比べる対象に入る。
#[test]
fn vpd_at_04_fixture_selection() {
    let stems = fixture_stems();
    for (name, reason) in FIXTURES_FOR_DIFF_B {
        assert!(
            stems.iter().any(|s| s == name),
            "外す一覧の {name} が tests/fixtures に無い"
        );
        assert!(!reason.is_empty(), "{name} に外す理由が無い");
    }
    let cases = fixture_cases();
    assert_eq!(cases.len() + FIXTURES_FOR_DIFF_B.len(), stems.len());
    // print だけの capability で実際に出力が取れている（期待ファイルと一致する）。
    let hello = cases
        .iter()
        .find(|c| c.name == "fixture/hello")
        .expect("hello");
    let expected = std::fs::read(fixture_dir().join("hello.expected")).expect("hello.expected");
    for run in [
        run_tree(hello.mode, &hello.inputs),
        run_vm(hello.mode, &hello.inputs),
    ] {
        let Run::Steps(steps) = run else {
            panic!("hello に経路が無い")
        };
        assert_eq!(steps[0].result, Ok(()));
        assert_eq!(steps[0].output, expected);
    }
}

/// VPD-AT-04・05: print だけで動く fixture のすべてを全軸で比べ、差が既知の差分一覧と完全一致する。
#[test]
fn vpd_at_04_fixtures_match_known_diffs() {
    assert_gate("fixture/", fixture_cases);
}

/// VPD-AT-05: gate は一覧に無い差と、差の出なくなった項目の両方で落ちる。
#[test]
fn vpd_at_05_gate_requires_exact_match() {
    let diff = |case: &str, axis| Diff {
        case: case.to_string(),
        axis,
        tree: "1".into(),
        vm: "2".into(),
    };
    let item = |id, axis, pattern| KnownDiff {
        id,
        axis,
        pattern,
        description: "test",
    };
    let diffs = vec![
        diff("matrix/top/push/unique", Axis::Heap),
        diff("matrix/top/pop/unique", Axis::Heap),
        diff("matrix/fn_local/push/shared", Axis::Fuel),
    ];
    let known = [
        item("KD-T1", Axis::Heap, "matrix/top/*"),
        item("KD-T2", Axis::Fuel, "matrix/fn_local/push/shared"),
    ];
    // 完全一致なら通る。
    assert!(gate(&diffs, &known, "matrix/").is_empty());
    // 一覧から 1 項目を消すと落ちる。
    let problems = gate(&diffs, &known[..1], "matrix/");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].starts_with("一覧に無い差"));
    // 軸が違えば当たらない。
    let problems = gate(
        &diffs,
        &[
            known[0],
            item("KD-T2", Axis::Heap, "matrix/fn_local/push/shared"),
        ],
        "matrix/",
    );
    assert_eq!(problems.len(), 2, "{problems:?}");
    // 差の出ない項目を 1 つ足すと落ちる。
    let mut extra = known.to_vec();
    extra.push(item("KD-T3", Axis::Output, "matrix/upvalue1/*"));
    let problems = gate(&diffs, &extra, "matrix/");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("KD-T3"));
    // `*` は途中にも置ける。
    assert!(
        gate(
            &diffs[..2],
            &[item("KD-T5", Axis::Heap, "matrix/*/p*/unique")],
            "matrix/"
        )
        .is_empty()
    );
    // 範囲外の項目は、その範囲の gate では数えない。
    let mut other = known.to_vec();
    other.push(item("KD-T4", Axis::Output, "repl/*"));
    assert!(gate(&diffs, &other, "matrix/").is_empty());
    // 実際の一覧の ID は重複しない。
    let mut ids: Vec<&str> = KNOWN_DIFFS.iter().map(|k| k.id).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        KNOWN_DIFFS.len(),
        "既知の差分一覧の ID が重複している"
    );
}

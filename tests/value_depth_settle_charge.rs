//! fix-value-depth-settle-quadratic（issue #304）: List / Dict の深度の読み直しに、読み直した子の数ぶん
//! fuel を課金する。受入基準 VDS-AT-01..04（docs/execution-control.md の同名スライス小節、
//! 規範は docs/semantic-decisions.md §21.3 と docs/execution-control.md §4.1）。
//! 値を変える前の課金・heap を課金しないことの primitive 単位の確認は `src/value.rs` の unit test。

use std::io::Read as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tsumugi::budget::{BudgetConfig, BudgetUsage};
use tsumugi::compiler::Compiler;
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("tsumugi-vds-{}-{}", std::process::id(), n));
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

struct CliResult {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

/// CLI（debug ビルド、8 MiB の実行スレッド）で script を実行する。`limit` を超えたら kill して panic する。
fn run_cli(source: &str, use_vm: bool, limit: Duration) -> CliResult {
    let dir = TestDir::new();
    let script = dir.path.join("s.tsg");
    std::fs::write(&script, source).expect("write script");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    command
        .arg(&script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let start = Instant::now();
    let mut child = command.spawn().expect("spawn");
    let mut out = child.stdout.take().expect("stdout");
    let mut err = child.stderr.take().expect("stderr");
    let out_thread = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
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
    CliResult {
        stdout: out_thread.join().expect("join").replace("\r\n", "\n"),
        stderr: err_thread.join().expect("join").replace("\r\n", "\n"),
        code: status.code(),
    }
}

fn tree_usage(source: &str) -> BudgetUsage {
    let tokens = Lexer::new(source).tokenize();
    let program = Parser::new(tokens).parse().expect("parse");
    let mut evaluator = Evaluator::new();
    evaluator.run(&program, source.len() as u64).expect("run");
    evaluator.budget_usage()
}

fn vm_usage(source: &str) -> BudgetUsage {
    let tokens = Lexer::new(source).tokenize();
    let program = Parser::new(tokens).parse().expect("parse");
    let chunk = Compiler::new().compile(&program).expect("compile");
    let mut vm = Vm::new(chunk);
    vm.run().expect("run");
    vm.budget_usage()
}

/// 両 backend の committed fuel（一致を確かめて返す）。
fn fuel(source: &str) -> u64 {
    let t = tree_usage(source).committed.fuel;
    let v = vm_usage(source).committed.fuel;
    assert_eq!(t, v, "tree と VM の fuel が違う: {source}");
    t
}

/// 最も深い子（深度 2）を `pos` に 1 つだけ持つ、要素数 `n` の List を `xs` に作る script 断片。
fn list_with_deep_at(n: usize, pos: usize) -> String {
    let items: Vec<String> = (0..n)
        .map(|i| {
            if i == pos {
                "[[1]]".to_string()
            } else {
                "0".to_string()
            }
        })
        .collect();
    format!("let xs = [{}]\n", items.join(", "))
}

/// 最も深い値を key `"a"` に 1 つだけ持つ、entry 数 `n` の Dict を `d` に作る script 断片。
fn dict_with_deep_a(n: usize) -> String {
    let mut items = vec!["\"a\": [[1]]".to_string()];
    items.extend((1..n).map(|i| format!("\"k{i}\": 0")));
    format!("let d = {{{}}}\n", items.join(", "))
}

/// VDS-AT-01: 読み直しが起きる操作は、起きない同じ形の操作より読み直した子の数ぶん fuel が多い。
/// 読み直しの起きない操作の fuel は増えない。tree と VM で一致する。
#[test]
fn vds_at_01_settle_charges_children_read_again() {
    for n in [5usize, 12] {
        // List の index 代入: 最も深い子（先頭）を浅い値で置き換える。比較の相手は、最も深くない
        // 子（末尾）を同じ値で置き換える同じ形の script。
        let settle = fuel(&format!("{}xs[0] = 0\n", list_with_deep_at(n, 0)));
        let control = fuel(&format!("{}xs[{}] = 0\n", list_with_deep_at(n, 0), n - 1));
        assert_eq!(settle - control, n as u64, "List の index 代入（n={n}）");

        // pop: 最も深い末尾を取り除く。相手は最も深くない末尾を取り除く script。
        let settle = fuel(&format!("{}pop(xs)\n", list_with_deep_at(n, n - 1)));
        let control = fuel(&format!("{}pop(xs)\n", list_with_deep_at(n, 0)));
        assert_eq!(settle - control, (n - 1) as u64, "pop（n={n}）");

        // Dict の既存 key の上書き。
        let settle = fuel(&format!("{}d[\"a\"] = 0\n", dict_with_deep_a(n)));
        let control = fuel(&format!("{}d[\"k1\"] = 0\n", dict_with_deep_a(n)));
        assert_eq!(settle - control, n as u64, "Dict の上書き（n={n}）");
    }

    // 読み直しの起きない操作は、スカラーを置き換える同じ形の操作と fuel が同じ。
    let n = 6;
    let base = list_with_deep_at(n, 0);
    let control = fuel(&format!("{base}xs[1] = 0\n"));
    for (label, stmt) in [
        ("同じ深度の値を入れる", "xs[0] = [[2]]"),
        ("より深い値を入れる", "xs[0] = [[[2]]]"),
        ("最も深くない子を置き換える", "xs[2] = [1]"),
        ("push", "push(xs, [[3]])"),
    ] {
        assert_eq!(fuel(&format!("{base}{stmt}\n")), control, "{label}");
    }
    // 同じ深度の子が残る取り除き（最も深い子が 2 つ）。
    let two = "let xs = [0, [[1]], 0, [[1]]]\n";
    assert_eq!(
        fuel(&format!("{two}pop(xs)\n")),
        fuel("let xs = [0, [[1]], 0, 0]\npop(xs)\n"),
        "同じ深度の子が残る pop"
    );
    assert_eq!(
        fuel(&format!("{two}xs[1] = 0\n")),
        fuel("let xs = [0, [[1]], 0, [[1]]]\nxs[0] = 0\n"),
        "同じ深度の子が残る index 代入"
    );
    // Dict: 新しい key は読み直さない。
    let d = dict_with_deep_a(n);
    assert_eq!(
        fuel(&format!("{d}d[\"new\"] = 0\n")),
        fuel(&format!("{d}d[\"k1\"] = 0\n")),
        "Dict の新しい key"
    );
    // VM だけが内部で読み直す Dict リテラルの重複 key は課金しない（tree と一致し、重複しない形と同じ）。
    assert_eq!(
        fuel("let d = {\"a\": [[1]], \"b\": 1, \"a\": 1}\n"),
        fuel("let d = {\"a\": 1, \"b\": 1, \"c\": 1}\n"),
        "Dict リテラルの重複 key"
    );
}

const STOPPED: &str = "ステップ上限に達しました";

/// 100 要素ぶんのスカラーを持つ List を作る script 断片（`fill(n)` を使う）。
fn fill_prelude(n: usize) -> String {
    format!("let xs = []\nlet i = 0\nwhile i < {n}\n  push(xs, 0)\n  i = i + 1\nend\n")
}

/// VDS-AT-02: issue #304 の往復は既定の budget で fuel 超過の terminal になり、壁時計の上限以内に終わる。
/// `try` / `catch` の内側でも catch されない。
#[test]
fn vds_at_02_roundtrips_stop_on_fuel() {
    let n = 50_000;
    let scripts: Vec<(&str, String)> = vec![
        (
            "List の index 代入の往復",
            format!(
                "{}xs[0] = [[1]]\ntry\n  while true\n    xs[0] = 0\n    xs[0] = [[1]]\n  end\ncatch e\n  print(\"caught\")\nend\nprint(\"after\")\n",
                fill_prelude(n)
            ),
        ),
        (
            "pop / push の往復",
            format!(
                "{}push(xs, [[1]])\ntry\n  while true\n    pop(xs)\n    push(xs, [[1]])\n  end\ncatch e\n  print(\"caught\")\nend\nprint(\"after\")\n",
                fill_prelude(n)
            ),
        ),
        (
            "Dict の上書きの往復",
            String::from(
                "let d = {\"a\": [[1]]}\nlet i = 0\nwhile i < 3000\n  d[\"k\" + to_str(i)] = 0\n  i = i + 1\nend\ntry\n  while true\n    d[\"a\"] = 0\n    d[\"a\"] = [[1]]\n  end\ncatch e\n  print(\"caught\")\nend\nprint(\"after\")\n",
            ),
        ),
    ];
    for (label, source) in &scripts {
        for use_vm in [false, true] {
            let start = Instant::now();
            let r = run_cli(source, use_vm, Duration::from_secs(10));
            assert!(
                r.stderr.contains(STOPPED),
                "[vm={use_vm}] {label}: fuel 超過で止まっていない: stdout={} stderr={}",
                r.stdout,
                r.stderr
            );
            assert_ne!(r.code, Some(0), "[vm={use_vm}] {label}: 成功してしまった");
            assert!(
                !r.stdout.contains("caught") && !r.stdout.contains("after"),
                "[vm={use_vm}] {label}: fuel 超過が catch された: {}",
                r.stdout
            );
            assert!(start.elapsed() < Duration::from_secs(10), "{label}");
        }
    }
}

fn tree_run_with_fuel(source: &str, total_fuel: u64) -> (bool, BudgetUsage) {
    let tokens = Lexer::new(source).tokenize();
    let program = Parser::new(tokens).parse().expect("parse");
    let mut evaluator = Evaluator::with_budget(BudgetConfig::for_legacy(total_fuel, 1_000_000));
    let ok = evaluator.run(&program, source.len() as u64).is_ok();
    (ok, evaluator.budget_usage())
}

fn vm_run_with_fuel(source: &str, total_fuel: u64) -> (bool, BudgetUsage) {
    let tokens = Lexer::new(source).tokenize();
    let program = Parser::new(tokens).parse().expect("parse");
    let chunk = Compiler::new().compile(&program).expect("compile");
    let mut vm = Vm::new(chunk);
    vm.set_max_steps(total_fuel);
    let ok = vm.run().is_ok();
    (ok, vm.budget_usage())
}

/// VDS-AT-03: 残りの fuel が読み直す子の数より少ないと、操作は fuel 超過で止まり、共有する別名があっても
/// COW の複製の heap を課金しない（peak_heap が操作の直前までと同じ）。両 backend で同じ結果になる。
/// （超過の記録の `requested` は `src/value.rs` の unit test で確かめる。）
#[test]
fn vds_at_03_charges_before_mutating() {
    let n = 40;
    for (label, setup, op) in [
        (
            "List の index 代入",
            format!("{}let ys = xs\n", list_with_deep_at(n, 0)),
            "xs[0] = 0\n",
        ),
        (
            "pop",
            format!("{}let ys = xs\n", list_with_deep_at(n, n - 1)),
            "pop(xs)\n",
        ),
        (
            "Dict の上書き",
            format!("{}let ys = d\n", dict_with_deep_a(n)),
            "d[\"a\"] = 0\n",
        ),
    ] {
        let prefix = format!("{setup}let marker = 1\n");
        let full = format!("{prefix}{op}");
        let cow_control = format!(
            "{prefix}{}",
            op.replace("\"a\"", "\"k1\"")
                .replace("xs[0]", "xs[1]")
                .replace("pop(xs)", "push(xs, 0)")
        );
        for use_vm in [false, true] {
            let run = |s: &str, f: u64| {
                if use_vm {
                    vm_run_with_fuel(s, f)
                } else {
                    tree_run_with_fuel(s, f)
                }
            };
            // 操作の直前までに要る fuel（最小）。
            let (mut lo, mut hi) = (0u64, 100_000u64);
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if run(&prefix, mid).0 {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            let before = lo;
            // 操作を含む script が成功する最小 fuel は、読み直さない操作より n（以上）多い。
            let (ok, _) = run(&full, before + (n as u64) / 2);
            assert!(
                !ok,
                "[vm={use_vm}] {label}: 残り fuel が足りないのに成功した"
            );
            // 同じ形で読み直さない操作（別名があるので COW で複製する）の peak_heap より、複製の分だけ小さい。
            let (_, stopped) = run(&full, before + (n as u64) / 2);
            let (_, cow) = run(&cow_control, before + 1_000);
            assert!(
                stopped.peak_heap_bytes + 24 + 32 * n as u64 <= cow.peak_heap_bytes,
                "[vm={use_vm}] {label}: COW の複製を課金した（停止 {} / 複製あり {}）",
                stopped.peak_heap_bytes,
                cow.peak_heap_bytes
            );
            // 十分な fuel があれば成功する。
            assert!(run(&full, before + 1_000).0, "[vm={use_vm}] {label}");
        }
    }
}

/// VDS-AT-04: 読み直しの起きない script の fuel・heap は変更前と同じ。VND-AT-05 の script の期待値
/// （tests/value_nesting_depth_limit.rs）が変わらないことに加え、tree と VM の fuel が一致する。
#[test]
fn vds_at_04_usage_unchanged_without_settle() {
    let source = "let xs = [1, [2], [[3]]]\nxs[0] = 5\npush(xs, [4])\npop(xs)\nxs[2] = [[6]]\nlet d = {\"a\": [1], \"b\": 2}\nd[\"b\"] = [3]\nd[\"c\"] = 0\n";
    let t = tree_usage(source);
    let v = vm_usage(source);
    assert_eq!(t.committed.fuel, v.committed.fuel);
}

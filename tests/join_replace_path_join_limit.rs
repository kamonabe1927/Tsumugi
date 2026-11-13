//! fix-join-replace-path-join-limit（issue #357・#399）: `join` / `replace` / `path_join` が
//! 結果の長さを確保の前に `max_single_string_bytes` と比べる。受入基準 JRP-AT-01..05
//! （docs/execution-control.md の同名スライス小節）。

use tsumugi::budget::{BudgetConfig, BudgetUsage};
use tsumugi::builtin_core::replace_result_len;
use tsumugi::compiler::Compiler;
use tsumugi::error::{ErrorKind, TsumugiError};
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;

type Outcome = (Result<(), TsumugiError>, BudgetUsage);

fn config(max_single: Option<u64>) -> BudgetConfig {
    let mut c = BudgetConfig::for_legacy(10_000_000, 1_000_000);
    if let Some(m) = max_single {
        c.max_single_string_bytes = m;
    }
    c
}

fn run(source: &str, use_vm: bool, max_single: Option<u64>) -> Outcome {
    let program = Parser::new(Lexer::new(source).tokenize())
        .parse()
        .expect("parse");
    if use_vm {
        let chunk = Compiler::new().compile(&program).expect("compile");
        let mut vm = Vm::with_budget(chunk, config(max_single));
        vm.charge_link(source.len() as u64, &[])
            .expect("charge_link");
        let result = vm.run();
        (result, vm.budget_usage())
    } else {
        let mut ev = Evaluator::with_budget(config(max_single));
        let result = ev.run(&program, source.len() as u64);
        (result, ev.budget_usage())
    }
}

/// `expr` を評価して `expected`（Tsumugi のリテラル）と等しくなければ 0 除算で失敗する script。
fn eq_script(expr: &str, expected: &str) -> String {
    format!("let r = {expr}\nif r != {expected}\n  let z = 1 / 0\nend\n")
}

fn a(n: usize) -> String {
    "a".repeat(n)
}

/// 超過（`StringLimit`）で止まり、`requested` と `limit` がメッセージに出ていて、`try` / `catch`
/// で捕捉されない（script が成功扱いにならない）ことを両 backend で確かめる。
fn assert_over(body: &str, limit: u64, requested: u64) {
    for vm in [false, true] {
        let src = format!("try\n  {body}\ncatch e\n  let caught = 1\nend\n");
        let (r, _) = run(&src, vm, Some(limit));
        let e = r.expect_err(body);
        assert_eq!(e.kind(), Some(ErrorKind::StringLimit), "[vm={vm}] {body}");
        let want = format!("{requested} バイト (上限: {limit} バイト)");
        assert!(e.to_string().contains(&want), "[vm={vm}] {body}: {e}");
    }
}

fn assert_ok(body: &str, limit: u64) {
    for vm in [false, true] {
        let (r, _) = run(&format!("{body}\n"), vm, Some(limit));
        if let Err(e) = r {
            panic!("[vm={vm}] {body}: {e}");
        }
    }
}

/// JRP-AT-01: issue #357・#399 の再現が abort せず、単一文字列上限の診断で exit 1 になる。
#[cfg(unix)]
#[test]
fn jrp_at_01_issue_repros_do_not_abort() {
    use std::process::Command;
    let big = |n: u32| {
        format!("let s = \"x\"\nlet i = 0\nwhile i < {n}\n  s = s + s\n  i = i + 1\nend\n")
    };
    let join_src = format!(
        "{}let xs = []\nlet j = 0\nwhile j < 2000\n  push(xs, s)\n  j = j + 1\nend\nlet r = join(xs, \"\")\n",
        big(22)
    );
    let replace_src = format!("{}let r = replace(s, \"\", s)\n", big(20));
    let args = vec!["s"; 2000].join(", ");
    let path_src = format!("{}let r = path_join({args})\n", big(22));
    let dir = std::env::temp_dir().join(format!("tsumugi-jrp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    for (label, src) in [
        ("join", join_src),
        ("replace", replace_src),
        ("path_join", path_src),
    ] {
        let script = dir.join(format!("{label}.tsg"));
        std::fs::write(&script, src).expect("write");
        for vm in [false, true] {
            let mut cmd = Command::new("sh");
            cmd.arg("-c")
                .arg("ulimit -v 4000000; exec \"$0\" \"$@\"")
                .arg(env!("CARGO_BIN_EXE_tsumugi"));
            if vm {
                cmd.arg("--vm");
            }
            let out = cmd.arg(&script).output().expect("spawn");
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(out.status.code(), Some(1), "[{label} vm={vm}] {stderr}");
            assert!(
                stderr.contains("文字列の長さが上限を超えました"),
                "[{label} vm={vm}] {stderr}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// JRP-AT-02: `join` の上限ちょうどは成功し、+1 は超えた時点の累積長で止まる。
#[test]
fn jrp_at_02_join_boundary() {
    let l = 64;
    // 30 + 1（区切り）+ 33 = 64 ちょうど。
    assert_ok(
        &format!("let r = join([\"{}\", \"{}\"], \",\")", a(30), a(33)),
        l,
    );
    // 要素を書き終えた時点で 65。
    assert_over(
        &format!("let r = join([\"{}\", \"{}\"], \",\")", a(30), a(34)),
        l,
        65,
    );
    // 区切りの書き足しで 65 になる。
    assert_over(
        &format!("let r = join([\"{}\", \"x\"], \",\")", a(64)),
        l,
        65,
    );
    // 区切りだけで超える（空要素）。
    assert_over(
        &format!("let r = join([\"\", \"\", \"\"], \"{}\")", a(40)),
        l,
        80,
    );
    // List の要素: 描画の途中（次の要素の入口の検査）で止まる。累積 67 は結果全体（72）以下。
    let nine = "[1, 2, 3]";
    let items = [nine; 8].join(", ");
    assert_over(&format!("let r = join([{items}], \"\")"), l, 67);
    // Dict の要素: key の書き出しを含めた入口の検査で止まる。
    let nums = (1..=30)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    // 描画の途中、17 個目の数の入口（累積 66）で止まる。結果全体より短い。
    assert_over(&format!("let r = join([{{\"k\": [{nums}]}}], \"\")"), l, 66);
}

/// JRP-AT-02: `replace` の上限ちょうどは成功し、+1 は結果の長さ（`requested`）で止まる。
#[test]
fn jrp_at_02_replace_boundary() {
    let l = 64;
    // 空パターン: "ab" は 3 箇所。2 + 3 × 20 = 62 は成功、2 + 3 × 22 = 68 は超過。
    assert_ok(
        &format!("let r = replace(\"ab\", \"\", \"{}\")", "n".repeat(20)),
        l,
    );
    assert_over(
        &format!("let r = replace(\"ab\", \"\", \"{}\")", "n".repeat(22)),
        l,
        68,
    );
    // 小さい上限でちょうど / +1: "abc" は 4 箇所、3 + 4 = 7。
    assert_ok("let r = replace(\"abc\", \"\", \"x\")", 7);
    assert_over("let r = replace(\"abc\", \"\", \"x\")", 6, 7);
    // 重なりうるパターン: "aaa" の "aa" は 1 箇所。3 − 2 + 63 = 64 は成功、+1 で超過。
    assert_ok(
        &format!("let r = replace(\"aaa\", \"aa\", \"{}\")", "n".repeat(63)),
        l,
    );
    assert_over(
        &format!("let r = replace(\"aaa\", \"aa\", \"{}\")", "n".repeat(64)),
        l,
        65,
    );
    // 一致なし・結果が縮む場合は入力の長さのまま / 短くなる。
    assert_ok(
        &format!(
            "let r = replace(\"{}\", \"z\", \"{}\")",
            a(64),
            "n".repeat(64)
        ),
        l,
    );
    assert_ok(&format!("let r = replace(\"{}\", \"aa\", \"\")", a(64)), l);
    // 多バイト文字: "あ" は 3 byte。空パターンで 3 × 2 + 2 × 1 ... "ああ" (6) に "x" を 3 箇所で 9。
    assert_ok("let r = replace(\"ああ\", \"\", \"x\")", 9);
    assert_over("let r = replace(\"ああ\", \"\", \"x\")", 8, 9);
}

/// JRP-AT-03: `path_join` の上限（absolute component の例は OS ごとに規則が違うので cfg で分ける）。
#[test]
fn jrp_at_03_path_join_limit() {
    let l = 64;
    // 引数 2 個: 30 + 1 + 33 = 64 ちょうど / 65。
    assert_ok(
        &format!("let r = path_join(\"{}\", \"{}\")", a(30), a(33)),
        l,
    );
    assert_over(
        &format!("let r = path_join(\"{}\", \"{}\")", a(30), a(34)),
        l,
        65,
    );
    // 多数: 20 + 1 + 20 + 1 + 22 = 64 ちょうど / 65。
    assert_ok(
        &format!(
            "let r = path_join(\"{}\", \"{}\", \"{}\")",
            a(20),
            a(20),
            a(22)
        ),
        l,
    );
    assert_over(
        &format!(
            "let r = path_join(\"{}\", \"{}\", \"{}\")",
            a(20),
            a(20),
            a(23)
        ),
        l,
        65,
    );
    // 途中で超え、後ろの absolute component で置き換わる呼び出しも超過（requested は超えた時点の長さ）。
    // 40 + 1（separator）+ 40 = 81。置き換える引数は OS ごとに違う。
    #[cfg(unix)]
    let (abs, keep) = ("/x", "\"/x/y\"");
    #[cfg(windows)]
    let (abs, keep) = ("C:\\\\x", "\"C:\\\\x\\\\y\"");
    assert_over(
        &format!("let r = path_join(\"{0}\", \"{0}\", \"{abs}\")", a(40)),
        l,
        81,
    );
    // 途中で一度も超えなければ従来どおり成功する。
    for vm in [false, true] {
        let (r, _) = run(
            &eq_script(&format!("path_join(\"{}\", \"{abs}\", \"y\")", a(40)), keep),
            vm,
            Some(l),
        );
        r.unwrap_or_else(|e| panic!("[vm={vm}] {e}"));
    }
    // 非 Str の引数は長さに関わらず今と同じ型のエラー（結合を始めない）。
    for vm in [false, true] {
        let src = format!("let r = path_join(\"{0}\", \"{0}\", 1)\n", a(40));
        let (r, _) = run(&src, vm, Some(l));
        let e = r.expect_err("非 Str");
        assert_ne!(e.kind(), Some(ErrorKind::StringLimit), "[vm={vm}] {e}");
        assert!(e.to_string().contains("path_join"), "[vm={vm}] {e}");
    }
}

/// JRP-AT-04: 予算内の結果と課金（fuel・string の累積）が変更前と同じ。値は変更前の main で測った。
#[cfg(unix)]
#[test]
fn jrp_at_04_in_budget_results_and_usage_unchanged() {
    // (式, 期待する結果のリテラル, fuel, string_allocations, string_bytes)
    let cases: &[(&str, &str, u64, u64, u64)] = CASES;
    let mut report = String::new();
    for (expr, expected, fuel, allocs, bytes) in cases {
        for vm in [false, true] {
            let (r, u) = run(&eq_script(expr, expected), vm, None);
            let got = (
                u.committed.fuel,
                u.committed.string_allocations,
                u.committed.string_bytes,
            );
            let ok = r.is_ok() && got == (*fuel, *allocs, *bytes);
            if !ok {
                report.push_str(&format!(
                    "[vm={vm}] {expr} => {:?} {got:?}\n",
                    r.err().map(|e| e.to_string())
                ));
            }
        }
    }
    assert!(report.is_empty(), "{report}");
}

const CASES: &[(&str, &str, u64, u64, u64)] = &[
    ("join([], \",\")", "\"\"", 0, 3, 1),
    ("join([1, 2, 3], \"\")", "\"123\"", 0, 3, 6),
    ("join([\"a\"], \",\")", "\"a\"", 0, 4, 4),
    (
        "join([1, \"a\", null, true, 1.5], \"-\")",
        "\"1-a-null-true-1.5\"",
        0,
        4,
        36,
    ),
    (
        "join([[1, \"a\"], {\"k\": [2]}], \"|\")",
        "\"[1, \\\"a\\\"]|{\\\"k\\\": [2]}\"",
        3,
        5,
        41,
    ),
    ("join([\"あい\", \"う\"], \"、\")", "\"あい、う\"", 0, 5, 36),
    ("replace(\"abc\", \"\", \"-\")", "\"-a-b-c-\"", 0, 5, 18),
    ("replace(\"abc\", \"z\", \"y\")", "\"abc\"", 0, 5, 11),
    (
        "replace(\"あいう\", \"い\", \"XY\")",
        "\"あXYう\"",
        0,
        5,
        30,
    ),
    ("replace(\"aaa\", \"aa\", \"\")", "\"a\"", 0, 5, 7),
    ("replace(\"abab\", \"ab\", \"\")", "\"\"", 0, 5, 6),
    ("path_join()", "\"\"", 0, 2, 0),
    ("path_join(\"a\")", "\"a\"", 0, 3, 3),
    ("path_join(\"a\", \"\", \"b\")", "\"a/b\"", 0, 5, 8),
    (
        "path_join(\"a\", \"b\", \"c\", \"d\")",
        "\"a/b/c/d\"",
        0,
        6,
        18,
    ),
    ("path_join(\"a\", \"/b\", \"c\")", "\"/b/c\"", 0, 5, 12),
];

/// JRP-AT-05: `replace` の結果の長さの計算は wrap せず、overflow は `None`（§7.1）。
/// ledger 側の `None` → `requested = u64::MAX` は `src/budget.rs` の同名の unit test が確かめる。
#[test]
fn jrp_at_05_replace_result_len_overflow() {
    assert_eq!(replace_result_len(3, 4, 0, 1), Some(7));
    assert_eq!(replace_result_len(3, 1, 2, 63), Some(64));
    assert_eq!(replace_result_len(100, 50, 2, 0), Some(0));
    assert_eq!(replace_result_len(u64::MAX, 0, 1, 1), Some(u64::MAX));
    assert_eq!(replace_result_len(u64::MAX, 1, 0, 1), None);
    assert_eq!(replace_result_len(0, u64::MAX, 0, 2), None);
    assert_eq!(replace_result_len(1, u64::MAX, 0, u64::MAX), None);
    assert_eq!(replace_result_len(1, u64::MAX, u64::MAX, u64::MAX), Some(1));
}

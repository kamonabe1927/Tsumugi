//! 行カバー率の測定（2026-11-22、`cargo llvm-cov`）で、tree-walk と VM の両方で
//! 一度も通っていなかった言語挙動を埋めるテスト。実装は変えず、今の挙動を両 engine で固定する。
//!
//! - Int と Float を混ぜた `-` `*` `/` `%` と、Float 同士の `%`（language-spec「算術」）。
//!   結果は Float の値として `==` で比べ、表示形式（`2.0` を `2` と出すかなど）には依存しない。
//! - Error 値を左に置いた `+` による文字列結合（language-spec「try / catch」: 文字列結合では
//!   エラーメッセージへ自動変換される）。
//! - 文字列と f-string のエスケープ `\t` `\\` `\"`（language-spec「文字列エスケープ」）と、
//!   f-string の式の中の入れ子の `{}`、式の中の文字列リテラルにあるエスケープ
//!   （language-spec「f-string」）。
//! - 入れ子の f-string のネスト上限（language-spec「ネスト深度」: 256）。上限の手前は通り、
//!   上限を超えるとパースエラーになり、境界の前後で tree と VM が同じ結果になる。
//!
//! 各ケースは CLI（tree-walk 既定と `--vm`）で実行し、stdout・終了コード・stderr を比べる。

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(30);

/// `--vm` が stderr の先頭に出す experimental の警告（`src/cli_capability.rs` の `WARN_VM_EXPERIMENTAL`）。
const WARN_VM_EXPERIMENTAL: &str =
    "警告: --vm は experimental な VM backend を選びます。規範の backend は tree-walk です";

#[derive(Debug, PartialEq)]
struct RunOutput {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn script_path(label: &str, use_vm: bool) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "tsumugi-numfstr-{}-{}-{}-{}.tsg",
        label,
        if use_vm { "vm" } else { "tree" },
        std::process::id(),
        unique
    ))
}

/// ソースを一時ファイルへ書き、CLI で実行する。子プロセスは制限時間付きで待つ。
fn run(label: &str, source: &str, use_vm: bool) -> RunOutput {
    let path = script_path(label, use_vm);
    std::fs::write(&path, source).expect("一時ファイルの書き込みに失敗");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    let mut child = command
        .arg(&path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("tsumugi バイナリの起動に失敗");
    let deadline = Instant::now() + TIMEOUT;
    let output = loop {
        match child.try_wait() {
            Ok(Some(_)) => break child.wait_with_output().expect("出力の取得に失敗"),
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&path);
                panic!("{label}: {}秒以内に完了しませんでした", TIMEOUT.as_secs());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => panic!("{label}: プロセス状態の取得に失敗: {error}"),
        }
    };
    let _ = std::fs::remove_file(&path);
    let stderr = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    let stderr = stderr
        .lines()
        .filter(|line| *line != WARN_VM_EXPERIMENTAL)
        .collect::<Vec<_>>()
        .join("\n");
    RunOutput {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        stderr,
    }
}

/// `source` が両 engine で正常終了し、stdout が `expected` と一致することを確かめる。
fn assert_both_print(label: &str, source: &str, expected: &str) {
    for use_vm in [false, true] {
        let mode = if use_vm { "vm" } else { "tree" };
        let out = run(label, source, use_vm);
        assert_eq!(
            out.code,
            Some(0),
            "{label} [{mode}]: 正常終了しませんでした。stderr: {}",
            out.stderr
        );
        assert_eq!(out.stdout, expected, "{label} [{mode}]: stdout が違います");
        assert_eq!(
            out.stderr, "",
            "{label} [{mode}]: stderr が空ではありません"
        );
    }
}

/// 入れ子の f-string を `depth` 段重ねた式（最内は `1`）。
fn nested_fstring(depth: usize) -> String {
    let mut expr = String::from("1");
    for _ in 0..depth {
        expr = format!("f\"{{{expr}}}\"");
    }
    expr
}

// ---- Int と Float の混合演算 ------------------------------------------------------

/// Int と Float を混ぜた `-` `*` `/` `%` は、両 engine で同じ Float の値になる。
#[test]
fn mixed_int_float_arithmetic_yields_float() {
    let cases = [
        ("int_sub_float", "1 - 2.5", "-1.5"),
        ("float_sub_int", "2.5 - 1", "1.5"),
        ("int_mul_float", "3 * 0.5", "1.5"),
        ("float_mul_int", "0.5 * 3", "1.5"),
        ("int_div_float", "1 / 4.0", "0.25"),
        ("float_div_int", "7.5 / 2", "3.75"),
        ("int_mod_float", "7 % 2.5", "2.0"),
        ("float_mod_int", "7.5 % 2", "1.5"),
    ];
    for (label, expr, value) in cases {
        let source = format!("let r = {expr}\nprint(r == {value})\nprint(type(r) == type(0.5))\n");
        assert_both_print(label, &source, "true\ntrue\n");
    }
}

/// Float 同士の `%` は Float の剰余になる（割り切れるときは 0.0）。
#[test]
fn float_modulo_float() {
    let source = "let a = 7.5 % 2.5\nlet b = 5.5 % 2.0\n\
                  print(a == 0.0)\nprint(b == 1.5)\n\
                  print(type(a) == type(0.5))\nprint(type(b) == type(0.5))\n";
    assert_both_print("float_mod", source, "true\ntrue\ntrue\ntrue\n");
}

/// 変数経由（定数でない被演算子）でも混合演算の結果は同じになる。
#[test]
fn mixed_arithmetic_through_variables() {
    let source = "let i = 3\nlet f = 1.5\n\
                  print(i - f == 1.5)\nprint(f - i == -1.5)\n\
                  print(i * f == 4.5)\nprint(f * i == 4.5)\n\
                  print(i / f == 2.0)\nprint(f / i == 0.5)\n\
                  print(i % f == 0.0)\nprint(f % i == 1.5)\n";
    assert_both_print(
        "mixed_vars",
        source,
        "true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\n",
    );
}

// ---- Error 値の文字列結合 ---------------------------------------------------------

/// catch した Error 値を左に置いた `+` は、エラーメッセージとの文字列結合になる。
#[test]
fn error_value_plus_string_concatenates_message() {
    let source = "try\n  let x = 1 / 0\ncatch e\n  \
                  let s = e + \"!\"\n  \
                  print(s == e[\"message\"] + \"!\")\n  \
                  print(type(s) == type(\"\"))\n\
                  end\n";
    assert_both_print("error_plus_str", source, "true\ntrue\n");
}

// ---- 文字列と f-string のエスケープ -------------------------------------------------

/// 通常文字列の `\\` はバックスラッシュ 1 文字になる。
#[test]
fn string_backslash_escape() {
    let source = "let s = \"a\\\\b\"\nprint(s)\nprint(len(s))\n";
    assert_both_print("str_backslash", source, "a\\b\n3\n");
}

/// f-string のリテラル部分でも `\t` `\\` `\"` が通常文字列と同じ意味になる。
#[test]
fn fstring_literal_escapes() {
    let source = "let n = 1\n\
                  print(f\"a\\tb{n}\" == \"a\\tb1\")\n\
                  print(f\"a\\\\b{n}\")\n\
                  print(f\"say \\\"hi\\\" {n}\")\n\
                  print(len(f\"\\t\\\\\\\"\"))\n";
    assert_both_print("fstr_escapes", source, "true\na\\b1\nsay \"hi\" 1\n3\n");
}

/// f-string の式の中では、辞書リテラルの `{}` が入れ子になっても式として読まれる。
#[test]
fn fstring_expression_with_nested_braces() {
    let source = "print(f\"v={ {\"k\": 2}[\"k\"] }\")\n\
                  print(f\"n={len([{\"a\": 1}, {\"b\": {\"c\": 3}}])}\")\n";
    assert_both_print("fstr_nested_braces", source, "v=2\nn=2\n");
}

/// f-string の式の中の文字列リテラルにある `\"` は、その文字列の一部として扱われる。
#[test]
fn fstring_expression_string_literal_with_escape() {
    let source = "print(f\"r={replace(\"a\\\"b\", \"\\\"\", \"-\")}\")\n\
                  print(f\"{len(\"x\\\\y\")}\")\n";
    assert_both_print("fstr_expr_escape", source, "r=a-b\n3\n");
}

// ---- 入れ子の f-string のネスト上限 -------------------------------------------------

/// 上限より十分浅い入れ子の f-string は両 engine で評価できる。
#[test]
fn nested_fstring_below_limit_evaluates() {
    let source = format!("print({})\n", nested_fstring(100));
    assert_both_print("fstr_depth_100", &source, "1\n");
}

/// 上限（256）を超える入れ子の f-string はパースエラーになり、何も実行されない。
#[test]
fn nested_fstring_beyond_limit_is_rejected() {
    let source = format!("print(\"start\")\nlet x = {}\n", nested_fstring(300));
    for use_vm in [false, true] {
        let mode = if use_vm { "vm" } else { "tree" };
        let out = run("fstr_depth_300", &source, use_vm);
        assert_eq!(out.code, Some(1), "[{mode}]: 終了コードが 1 ではありません");
        assert_eq!(out.stdout, "", "[{mode}]: 実行前に止まっていません");
        assert!(
            out.stderr.contains("ネストが深すぎます"),
            "[{mode}]: ネスト上限の診断がありません: {}",
            out.stderr
        );
    }
}

/// 上限の境界の前後で、tree と VM の結果（終了コード・stdout・stderr）が一致する。
#[test]
fn nested_fstring_limit_boundary_agrees_between_engines() {
    for depth in 250..=258 {
        let source = format!("let x = {}\nprint(\"ok\")\n", nested_fstring(depth));
        let label = format!("fstr_depth_{depth}");
        let tree = run(&label, &source, false);
        let vm = run(&label, &source, true);
        assert_eq!(tree, vm, "depth {depth}: tree と VM の結果が違います");
    }
}

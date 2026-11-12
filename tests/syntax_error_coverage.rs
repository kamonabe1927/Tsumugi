//! 構文エラー（パースエラー）の網羅テスト。
//!
//! 文法上ありえない入力が、実行前にパースエラーとして拒否されることを確かめる。
//! 各ケースを tree-walk（既定）と VM（`--vm`）の両方の CLI で実行し、次を検査する。
//!
//! - 終了コードが 1 で、stdout が空（実行前に止まり、どの文も実行されない。language-spec
//!   「キャッチできないエラー」）
//! - stderr の各行が `N行目: ` で始まる診断で、最初の診断の行番号が期待どおり（language-spec
//!   「エラーメッセージ」、semantic-decisions §3.4 の「parse error は不正 token または EOF token の行」）
//! - tree と VM の stderr が一致する（VM の experimental 警告の 1 行を除く）
//!
//! 文言は spec が定めているもの（`return` の位置、f-string など）だけを部分一致で固定し、
//! それ以外は token 名を含む内部表現に依存しないよう行番号だけを見る。

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(30);

/// `--vm` が stderr の先頭に出す experimental の警告（`src/cli_capability.rs` の `WARN_VM_EXPERIMENTAL`）。
const WARN_VM_EXPERIMENTAL: &str =
    "警告: --vm は experimental な VM backend を選びます。規範の backend は tree-walk です";

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
        "tsumugi-syntax-{}-{}-{}-{}.tsg",
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
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr,
    }
}

/// 診断行 `N行目: ...` から行番号を取り出す。形式に合わなければ `None`。
fn diagnostic_line(line: &str) -> Option<usize> {
    let (number, _) = line.split_once("行目: ")?;
    number.parse().ok()
}

/// `source` がパースエラーで拒否され、最初の診断が `first_line` 行目であることを両 engine で確かめる。
/// `message` を渡したときは、最初の診断がその文字列を含むことも確かめる。返り値は tree の診断行。
fn assert_syntax_error(
    label: &str,
    source: &str,
    first_line: usize,
    message: Option<&str>,
) -> Vec<String> {
    let tree = run(label, source, false);
    let vm = run(label, source, true);
    for (mode, out) in [("tree", &tree), ("vm", &vm)] {
        assert_eq!(
            out.code,
            Some(1),
            "{label} [{mode}]: 終了コードが 1 ではありません。stderr: {}",
            out.stderr
        );
        assert_eq!(
            out.stdout, "",
            "{label} [{mode}]: パースエラーなのに実行された出力があります"
        );
        let lines: Vec<&str> = out.stderr.lines().collect();
        assert!(!lines.is_empty(), "{label} [{mode}]: 診断がありません");
        for line in &lines {
            assert!(
                diagnostic_line(line).is_some(),
                "{label} [{mode}]: `N行目: ` 形式でない診断行: {line:?}"
            );
        }
        assert_eq!(
            diagnostic_line(lines[0]),
            Some(first_line),
            "{label} [{mode}]: 最初の診断の行番号が違います: {:?}",
            lines[0]
        );
        if let Some(message) = message {
            assert!(
                lines[0].contains(message),
                "{label} [{mode}]: 最初の診断に {message:?} が含まれません: {:?}",
                lines[0]
            );
        }
    }
    assert_eq!(
        tree.stderr, vm.stderr,
        "{label}: tree と VM でパースエラーの診断が違います"
    );
    tree.stderr.lines().map(str::to_owned).collect()
}

// ---- 実行前に止まること・try で捕まらないこと --------------------------------

/// 構文エラーより前の文も実行されない（language-spec「キャッチできないエラー」: 実行前に発生する）。
#[test]
fn statements_before_syntax_error_are_not_executed() {
    assert_syntax_error("before_error", "print(\"ran\")\nlet = 1\n", 2, None);
}

/// try の中の構文エラーは catch されない（language-spec「キャッチできないエラー」）。
#[test]
fn syntax_error_inside_try_is_not_caught() {
    assert_syntax_error(
        "inside_try",
        "try\n  let = 1\ncatch e\n  print(\"caught\")\nend\n",
        2,
        None,
    );
}

/// 構文エラーの後ろにある文も実行されない。
#[test]
fn statements_after_syntax_error_are_not_executed() {
    assert_syntax_error("after_error", "let x = (1\nprint(\"ran\")\n", 1, None);
}

// ---- ブロックの end の欠落 ------------------------------------------------------

/// `end` が無いブロックは、入力の終わり（EOF token の行）で未完結として報告される。
#[test]
fn missing_end_is_reported_at_eof() {
    let cases = [
        ("if", "if true\n  print(1)\n", 3),
        ("if_else", "if true\n  print(1)\nelse\n  print(2)\n", 5),
        ("while", "while true\n  break\n", 3),
        ("for", "for x in [1]\n  print(x)\n", 3),
        ("fn", "fn f()\n  return 1\n", 3),
        ("try", "try\n  print(1)\ncatch e\n  print(e)\n", 5),
        ("lambda", "let f = fn(x)\n  x\n", 3),
        ("nested", "fn f(a)\n  if a\n    return 1\n  end\n", 5),
    ];
    for (label, source, line) in cases {
        assert_syntax_error(label, source, line, Some("end が必要です"));
    }
}

// ---- 閉じていない括弧 -----------------------------------------------------------

/// 閉じていない `(`・`[`・`{` は、不正 token（ここでは EOF）の行で報告される。
#[test]
fn unclosed_brackets_are_rejected() {
    let cases = [
        ("paren_call", "print(1", 1),
        ("paren_group", "let x = 1\nlet y = (x", 2),
        ("list", "let xs = [1, 2", 1),
        ("dict", "let d = {\"a\": 1", 1),
    ];
    for (label, source, line) in cases {
        assert_syntax_error(label, source, line, None);
    }
}

/// 辞書リテラルのキーの後に `:` が無いと拒否される。
#[test]
fn dict_entry_without_colon_is_rejected() {
    assert_syntax_error("dict_colon", "let d = {\"a\" 1}", 1, None);
}

/// 呼び出しの引数の間に `,` が無いと拒否される。
#[test]
fn call_arguments_without_comma_are_rejected() {
    assert_syntax_error("args_comma", "print(1 2)", 1, None);
    assert_syntax_error("args_comma_str", "print(\"a\" \"b\")", 1, None);
}

// ---- 式の欠落・不正な位置の token ------------------------------------------------

/// 式が必要な位置で式が無い・式を始められない token が来ると拒否される。
#[test]
fn missing_operand_is_rejected() {
    let cases = [
        ("binary_rhs", "let x = 1 +"),
        ("not_operand", "let x = not"),
        ("rparen", "let x = )"),
        ("let_value", "let x ="),
    ];
    for (label, source) in cases {
        assert_syntax_error(label, source, 1, None);
    }
}

/// 1 行に 2 つの文を並べると拒否される（文の区切りは改行）。
#[test]
fn two_statements_on_one_line_are_rejected() {
    assert_syntax_error("two_stmts", "let x = 1 let y = 2", 1, None);
}

/// 対応する `if`・`try` が無い `else`・`elif`・`catch`・`end` は拒否される。
#[test]
fn stray_block_keywords_are_rejected() {
    let cases = [
        ("else", "else\n"),
        ("elif", "elif true\nend\n"),
        ("end", "end\n"),
        ("catch", "catch e\nend\n"),
    ];
    for (label, source) in cases {
        assert_syntax_error(label, source, 1, None);
    }
}

/// 同じ `if` に `else` を 2 つ書くと、2 つ目の `else` の行で拒否される。
#[test]
fn second_else_is_rejected() {
    assert_syntax_error(
        "double_else",
        "if true\n  print(1)\nelse\n  print(2)\nelse\n  print(3)\nend\n",
        5,
        None,
    );
}

/// catch 節を持たない try は拒否される（language-spec の try / catch の形）。
#[test]
fn try_without_catch_is_rejected() {
    assert_syntax_error("try_no_catch", "try\n  print(1)\nend\n", 3, None);
}

/// catch の後に変数名が無いと拒否される（language-spec: `catch 変数名`）。
#[test]
fn catch_without_variable_is_rejected() {
    assert_syntax_error(
        "catch_no_var",
        "try\n  print(1)\ncatch\n  print(2)\nend\n",
        3,
        None,
    );
}

// ---- 宣言・代入の対象 -----------------------------------------------------------

/// `let` の後は識別子でなければならない。予約語や数値は名前に使えない。
#[test]
fn let_requires_identifier() {
    let cases = [
        ("let_int", "let 1 = 2"),
        ("let_kw_if", "let if = 1"),
        ("let_kw_fn", "let fn = 1"),
        ("let_kw_true", "let true = 1"),
        ("let_str", "let \"x\" = 1"),
    ];
    for (label, source) in cases {
        assert_syntax_error(label, source, 1, Some("let の後に識別子が必要です"));
    }
}

/// 代入の左辺は変数か、変数に直接格納されたコレクションのインデックスだけ
/// （language-spec「インデックス代入」の制約）。それ以外は拒否される。
#[test]
fn invalid_assignment_targets_are_rejected() {
    let cases = [
        ("literal", "1 = 2", 1),
        ("binary", "let x = 1\nx + 1 = 3", 2),
        ("call", "fn f()\n  return 1\nend\nf() = 2", 4),
        ("nested_index", "let xs = [[1]]\nxs[0][0] = 2", 2),
        ("literal_index", "[1, 2][0] = 3", 1),
    ];
    for (label, source, line) in cases {
        assert_syntax_error(label, source, line, None);
    }
}

// ---- for・fn の形 ---------------------------------------------------------------

/// `for` は `for 変数名 in 式` の形でなければならない。
#[test]
fn malformed_for_is_rejected() {
    let cases = [
        ("for_no_in", "for x [1]\nend\n"),
        ("for_no_var", "for in [1]\nend\n"),
        ("for_int_var", "for 1 in [1]\nend\n"),
    ];
    for (label, source) in cases {
        assert_syntax_error(label, source, 1, None);
    }
}

/// 名前付き関数の定義には関数名と識別子の引数が要る。
#[test]
fn malformed_fn_definition_is_rejected() {
    let cases = [
        ("fn_int_param", "fn f(1)\nend\n"),
        ("fn_trailing_comma", "fn f(a, )\nend\n"),
        ("fn_kw_param", "fn f(if)\nend\n"),
        ("fn_kw_name", "fn while()\nend\n"),
    ];
    for (label, source) in cases {
        assert_syntax_error(label, source, 1, None);
    }
}

// ---- spec が文言を定めているもの ------------------------------------------------

/// `return` には式が必要で、単体はパースエラーになる（language-spec「関数」）。
#[test]
fn bare_return_inside_function_is_rejected() {
    assert_syntax_error("bare_return", "fn f()\n  return\nend\n", 2, None);
}

/// 関数の外の `return` は実行前に拒否される（language-spec「関数」）。
#[test]
fn return_outside_function_is_rejected_before_execution() {
    let cases = [
        ("top", "print(\"ran\")\nreturn 1\n", 2),
        ("top_block", "if true\n  return 1\nend\n", 2),
        ("loop", "while true\n  return 1\nend\n", 2),
    ];
    for (label, source, line) in cases {
        assert_syntax_error(
            label,
            source,
            line,
            Some("return は関数の中でのみ使用できます"),
        );
    }
}

/// import はトップレベル以外では読み込み前に拒否される（language-spec「モジュール / import」）。
#[test]
fn import_outside_top_level_is_rejected() {
    let cases = [
        ("in_fn", "fn f()\n  import \"x.tsg\"\nend\n", 2),
        ("in_if", "if true\n  import \"x.tsg\"\nend\n", 2),
        ("in_try", "try\n  import \"x.tsg\"\ncatch e\nend\n", 2),
        ("in_lambda", "let f = fn()\n  import \"x.tsg\"\nend\n", 2),
    ];
    for (label, source, line) in cases {
        assert_syntax_error(
            label,
            source,
            line,
            Some("import はトップレベルでのみ使用できます"),
        );
    }
}

/// import の後は文字列パスでなければならない。
#[test]
fn import_requires_string_path() {
    assert_syntax_error("import_ident", "import x", 1, None);
    assert_syntax_error("import_empty", "import", 1, None);
}

/// f-string の不正な形はパースエラーになる（language-spec「f-string」）。
#[test]
fn malformed_fstrings_are_rejected() {
    let cases = [
        ("fstr_empty", "let x = f\"a {}\""),
        ("fstr_unclosed_expr", "let x = 1\nprint(f\"a {x\")"),
        ("fstr_unclosed", "let x = f\"a"),
    ];
    for (label, source) in cases {
        let line = source.lines().count();
        assert_syntax_error(label, source, line, None);
    }
}

// ---- 複数のエラー ---------------------------------------------------------------

/// 互いに独立した構文エラーは、すべて元の順で報告される（回復して続きを読む）。
#[test]
fn independent_errors_are_all_reported_in_source_order() {
    let lines = assert_syntax_error(
        "multi",
        "let = 1\nprint(\"ok\")\nlet 2 = 3\nprint(\"ok\")\nfn g()\n  return\nend\nlet x = 1 let y = 2\n",
        1,
        None,
    );
    let numbers: Vec<usize> = lines.iter().filter_map(|l| diagnostic_line(l)).collect();
    for expected in [1, 3, 6, 8] {
        assert!(
            numbers.contains(&expected),
            "{expected}行目の診断がありません: {lines:?}"
        );
    }
    let mut sorted = numbers.clone();
    sorted.sort_unstable();
    assert_eq!(numbers, sorted, "診断が行の順に並んでいません: {lines:?}");
}

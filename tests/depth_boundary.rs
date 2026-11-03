//! fix-depth-boundary-tests（loop:drift #318 の D-27）: threat-model TM-AT-07 の AST 深度 256/257 と
//! import 深度 128/129 の境界を、tree-walk と `--vm` の両方で固定する。受入基準 DBT-AT-01..02
//! （docs/threat-model.md の同名スライス小節）。
//!
//! 上限の値は `src/limits.rs` の `MAX_AST_DEPTH`（256）と `MAX_IMPORT_DEPTH`（128）。integration test は
//! crate の内部定数を見られないので、TM-AT-07 の数字をそのまま書く。実装がずれたらこの test が落ちる。
//!
//! 深さの数え方は既存の検査のとおり（変えない）:
//! - AST 深度は文・式の node を根から数える。`let x = [[...1...]]` は `let` 文が 1、各 List が 1、
//!   最内の literal が 1 なので、List が 254 重で深さ 256 になる。`if` の入れ子は `if` が 253 重と
//!   その中の `print(1)`（文 1・呼び出し 1・引数 1）で深さ 256 になる。
//! - import 深度は root script を除く active import chain の長さ。モジュール 128 個の chain は link でき、
//!   129 個目で `DepthExceeded { limit: 128 }`（CLI では「import のネストが深すぎます (上限: 128)」）になる。

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const AST_LIMIT: usize = 256;
const IMPORT_LIMIT: usize = 128;

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!("tsumugi-dbt-{}-{}", std::process::id(), n));
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

struct Run {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

impl Run {
    fn from(output: Output) -> Self {
        Self {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            code: output.status.code(),
        }
    }

    /// panic・abort でなく、構造化された error として exit 1 で終わったこと。
    fn assert_structured_failure(&self, expected_message: &str, label: &str) {
        assert_eq!(
            self.code,
            Some(1),
            "{label}: exit 1 のはず（panic・abort なら 101 や signal）: stderr={}",
            self.stderr
        );
        assert!(
            !self.stderr.contains("panicked"),
            "{label}: panic してはならない: {}",
            self.stderr
        );
        assert!(
            self.stderr.contains(expected_message),
            "{label}: stderr に {expected_message:?} が無い: {}",
            self.stderr
        );
    }

    fn assert_success(&self, expected_stdout: &str, label: &str) {
        assert_eq!(self.code, Some(0), "{label}: stderr={}", self.stderr);
        assert_eq!(self.stdout.trim_end(), expected_stdout, "{label}");
    }
}

fn run_script(source: &str, use_vm: bool) -> Run {
    let dir = TestDir::new();
    let script = dir.path.join("s.tsg");
    std::fs::write(&script, source).expect("write script");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    command.arg(&script);
    Run::from(command.output().expect("spawn"))
}

/// List の入れ子で、AST 深度がちょうど `depth` になる script（`let` 1 + List `depth - 2` + literal 1）。
fn nested_list_script(depth: usize) -> String {
    let lists = depth - 2;
    format!(
        "let x = {}1{}\nprint(len(x))\n",
        "[".repeat(lists),
        "]".repeat(lists)
    )
}

/// `if` の入れ子で、AST 深度がちょうど `depth` になる script（`if` `depth - 3` 重 + `print(1)` の 3）。
fn nested_if_script(depth: usize) -> String {
    let ifs = depth - 3;
    format!(
        "{}print(1)\n{}",
        "if true\n".repeat(ifs),
        "end\n".repeat(ifs)
    )
}

const AST_MESSAGE: &str = "ネストが深すぎます (上限: 256)";

/// DBT-AT-01: 式の入れ子。深さ 256 は成功、257 は構造化 compile error。
#[test]
fn dbt_at_01_expression_nesting_boundary_tree_and_vm() {
    for use_vm in [false, true] {
        let ok = run_script(&nested_list_script(AST_LIMIT), use_vm);
        ok.assert_success("1", &format!("式 256 [vm={use_vm}]"));

        let over = run_script(&nested_list_script(AST_LIMIT + 1), use_vm);
        over.assert_structured_failure(AST_MESSAGE, &format!("式 257 [vm={use_vm}]"));
    }
}

/// DBT-AT-01: 文（block）の入れ子。深さ 256 は成功、257 は構造化 compile error。
#[test]
fn dbt_at_01_block_nesting_boundary_tree_and_vm() {
    for use_vm in [false, true] {
        let ok = run_script(&nested_if_script(AST_LIMIT), use_vm);
        ok.assert_success("1", &format!("block 256 [vm={use_vm}]"));

        let over = run_script(&nested_if_script(AST_LIMIT + 1), use_vm);
        over.assert_structured_failure(AST_MESSAGE, &format!("block 257 [vm={use_vm}]"));
    }
}

/// `chain_len` 個のモジュール `m0 → m1 → ... → m(chain_len-1)` を import root `r` の下に書き出し、
/// root script（`@r/m0.tsg` を import する）を CLI で走らせる。
fn run_import_chain(chain_len: usize, use_vm: bool) -> Run {
    let dir = TestDir::new();
    for i in 0..chain_len {
        let body = if i + 1 < chain_len {
            format!("import \"@r/m{}.tsg\"\n", i + 1)
        } else {
            "let z = 1\n".to_string()
        };
        std::fs::write(dir.path.join(format!("m{i}.tsg")), body).expect("write module");
    }
    let script = dir.path.join("main.tsg");
    std::fs::write(&script, "import \"@r/m0.tsg\"\nprint(z)\n").expect("write script");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    command
        .arg("--allow-import-root")
        .arg(format!("r={}", dir.path.display()))
        .arg(&script);
    Run::from(command.output().expect("spawn"))
}

/// DBT-AT-02: import chain 128 個は link 成功、129 個は `DepthExceeded { limit: 128 }`。
#[test]
fn dbt_at_02_import_depth_boundary_tree_and_vm() {
    for use_vm in [false, true] {
        let ok = run_import_chain(IMPORT_LIMIT, use_vm);
        ok.assert_success("1", &format!("import 128 [vm={use_vm}]"));

        let over = run_import_chain(IMPORT_LIMIT + 1, use_vm);
        over.assert_structured_failure(
            "import のネストが深すぎます (上限: 128)",
            &format!("import 129 [vm={use_vm}]"),
        );
    }
}

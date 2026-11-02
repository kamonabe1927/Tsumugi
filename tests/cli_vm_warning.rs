//! fix-cli-vm-experimental-warning: CLI の `--vm` の warning と `--help` の文言
//! （embedding-api §12、CVW-AT-01..03）。subprocess で stdout・stderr・exit code を照合する。

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

/// `src/cli_capability.rs` の `WARN_VM_EXPERIMENTAL` と同じ文言（bin crate の定数は test から見えない）。
const WARN: &str =
    "警告: --vm は experimental な VM backend を選びます。規範の backend は tree-walk です";

fn run(args: &[&str], stdin: Option<&str>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("tsumugi の起動に失敗");
    let mut pipe = child.stdin.take().expect("stdin");
    pipe.write_all(stdin.unwrap_or("").as_bytes())
        .expect("stdin への書き込みに失敗");
    drop(pipe);
    child.wait_with_output().expect("tsumugi の終了待ちに失敗")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

fn warn_lines(output: &Output) -> usize {
    text(&output.stderr).lines().filter(|l| *l == WARN).count()
}

/// warning の 1 行を除いた stderr。
fn stderr_rest(output: &Output) -> String {
    text(&output.stderr)
        .lines()
        .filter(|l| *l != WARN)
        .collect::<Vec<_>>()
        .join("\n")
}

fn script_file(name: &str, source: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("tsumugi-cvw-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("a.tsg");
    std::fs::write(&path, source).expect("script");
    path
}

#[test]
fn cvw_at_01_vm_file_warns_once_before_script() {
    let path = script_file("file", "print(1)\n");
    let path = path.to_str().expect("UTF-8");
    let tree = run(&[path], None);
    let vm = run(&["--vm", path], None);
    assert_eq!(warn_lines(&vm), 1, "stderr: {}", text(&vm.stderr));
    assert!(
        text(&vm.stderr).starts_with(WARN),
        "script の開始前（stderr の先頭）に出る"
    );
    assert_eq!(text(&vm.stdout), text(&tree.stdout));
    assert_eq!(vm.status.code(), tree.status.code());
    assert_eq!(warn_lines(&tree), 0, "--vm を付けなければ出ない");
}

#[test]
fn cvw_at_01_vm_stdin_warns_once() {
    let tree = run(&["-"], Some("print(2)\n"));
    let vm = run(&["--vm", "-"], Some("print(2)\n"));
    assert_eq!(warn_lines(&vm), 1, "stderr: {}", text(&vm.stderr));
    assert_eq!(text(&vm.stdout), text(&tree.stdout));
    assert_eq!(vm.status.code(), tree.status.code());
    assert_eq!(warn_lines(&tree), 0);
}

#[test]
fn cvw_at_01_vm_repl_warns_once() {
    let vm = run(&["--vm"], None);
    assert_eq!(warn_lines(&vm), 1, "stderr: {}", text(&vm.stderr));
    assert_eq!(vm.status.code(), Some(0));
    let tree = run(&[], None);
    assert_eq!(warn_lines(&tree), 0);
}

#[test]
fn cvw_at_01_vm_failing_script_keeps_exit_code() {
    let path = script_file("fail", "print(1 / 0)\n");
    let path = path.to_str().expect("UTF-8");
    let tree = run(&[path], None);
    let vm = run(&["--vm", path], None);
    assert_eq!(warn_lines(&vm), 1);
    assert_eq!(vm.status.code(), tree.status.code());
    assert_ne!(vm.status.code(), Some(0));
}

#[test]
fn cvw_at_01_no_warning_for_help_or_usage_error() {
    let help = run(&["--vm", "--help"], None);
    assert_eq!(warn_lines(&help), 0);
    assert_eq!(help.status.code(), Some(0));
    let version = run(&["--vm", "--version"], None);
    assert_eq!(warn_lines(&version), 0);
    let unknown = run(&["--vm", "--unknown"], None);
    assert_eq!(warn_lines(&unknown), 0);
    assert_eq!(unknown.status.code(), Some(1));
}

#[test]
fn cvw_at_02_help_wording() {
    let help = run(&["--help"], None);
    let out = text(&help.stdout);
    assert!(
        out.contains("  --vm                        experimental なバイトコード VM で実行する\n"),
        "{out}"
    );
    assert!(
        out.contains(
            "  --allow-import-root NAME=PATH  import root を mount し module_resolver を許可する\n"
        ),
        "{out}"
    );
    assert!(!out.contains("値検証のみ"), "{out}");
}

#[test]
fn cvw_at_03_stderr_apart_from_warning_matches_tree() {
    let path = script_file("rest", "print(1 / 0)\n");
    let path = path.to_str().expect("UTF-8");
    let tree = run(&[path], None);
    let vm = run(&["--vm", path], None);
    assert_eq!(stderr_rest(&vm), stderr_rest(&tree));
}

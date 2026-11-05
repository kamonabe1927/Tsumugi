//! fix-vm-loop-control-outside-loop（issue #292）: VM がループの外の `break` / `continue` を
//! compile error にせず、tree と同じく到達したときの `control_flow` runtime error にする。
//! 受入基準 VLC-AT-01..04（docs/determinism-and-audit.md の同名スライス小節）。
//!
//! 各 test は tree と `--vm` の CLI を同じ script で実行し、stdout・stderr・終了コードの一致と、
//! tree 側の期待値（両方が同じ誤りをしていないこと）を確かめる。命令の verifier 側は
//! `src/verifier.rs` の unit test、VM の防御は `tests/defensive_vm.rs` で確かめる。

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use tsumugi::{
    Backend, CompileOptions, EmbeddingEngine, EngineConfig, LanguageRevision, Source, SourceId,
};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("tsumugi-vlc-{}-{}", std::process::id(), n));
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

#[derive(Debug, PartialEq, Eq)]
struct Observed {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

const WARN_VM_EXPERIMENTAL: &str =
    "警告: --vm は experimental な VM backend を選びます。規範の backend は tree-walk です";

fn finish(output: std::process::Output) -> Observed {
    let text = String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n");
    let stderr: String = text
        .split_inclusive('\n')
        .filter(|line| line.trim_end_matches('\n') != WARN_VM_EXPERIMENTAL)
        .collect();
    Observed {
        stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        stderr,
        code: output.status.code(),
    }
}

fn run_script(files: &[(&str, &str)], main: &str, use_vm: bool) -> Observed {
    let dir = TestDir::new();
    for (name, text) in files {
        std::fs::write(dir.path.join(name), text).expect("write file");
    }
    std::fs::write(dir.path.join("main.tsg"), main).expect("write main");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    if !files.is_empty() {
        command
            .arg("--allow-import-root")
            .arg(format!("default={}", dir.path.display()));
    }
    command
        .arg("main.tsg")
        .current_dir(&dir.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    finish(command.output().expect("run tsumugi"))
}

/// tree と VM が同じ観測になることを確かめ、tree の観測を返す。
fn assert_same(files: &[(&str, &str)], main: &str) -> Observed {
    let tree = run_script(files, main, false);
    let vm = run_script(files, main, true);
    assert_eq!(tree, vm, "tree と VM が不一致: {main}");
    tree
}

fn run_repl(source: &str, use_vm: bool) -> Observed {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("spawn repl");
    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(source.as_bytes()).expect("write stdin");
    drop(stdin);
    finish(child.wait_with_output().expect("wait repl"))
}

fn visible_lines(stdout: &str, use_vm: bool) -> Vec<String> {
    let prompt = if use_vm { "tsumugi:vm> " } else { "tsumugi> " };
    stdout
        .lines()
        .filter_map(|line| {
            line.rsplit_once(prompt)
                .map(|(_, value)| value.to_string())
                .map(|value| value.trim_start_matches([' ', '.']).to_string())
                .filter(|value| !value.is_empty())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// VLC-AT-01: 到達しなければ実行が続く
// ---------------------------------------------------------------------------

#[test]
fn vlc_at_01_unreached_loop_control_does_not_reject_script() {
    for stmt in ["break", "continue"] {
        let source = format!(
            "print(1)\n\
             if false\n  {stmt}\nend\n\
             fn never()\n  {stmt}\nend\n\
             try\n  print(2)\ncatch e\n  {stmt}\nend\n\
             let f = fn()\n  {stmt}\nend\n\
             print(\"done\")\n"
        );
        let observed = assert_same(&[], &source);
        assert_eq!(observed.stdout, "1\n2\ndone\n", "{stmt}: {observed:?}");
        assert_eq!(observed.code, Some(0), "{stmt}: {observed:?}");
    }
}

#[test]
fn vlc_at_01_library_compile_succeeds() {
    let config = EngineConfig {
        backend: Backend::VmExperimental,
        language_revision: LanguageRevision::CURRENT,
        audit_sink: None,
        audit_slice_fuel: None,
    };
    let engine = EmbeddingEngine::builder()
        .config(config)
        .allow_experimental_backend(true)
        .build()
        .expect("build engine");
    for source in ["if false\n  break\nend\n", "fn f()\n  continue\nend\n"] {
        let result = engine.compile(
            Source::new(SourceId::new("vlc").expect("source id"), source),
            &CompileOptions::default(),
        );
        assert!(result.is_ok(), "{source}: compile が失敗した");
    }
}

// ---------------------------------------------------------------------------
// VLC-AT-02: 到達したら tree と同じ runtime error
// ---------------------------------------------------------------------------

#[test]
fn vlc_at_02_reached_loop_control_is_runtime_error() {
    for (stmt, message) in [
        ("break", "break はループの中でのみ使用できます"),
        ("continue", "continue はループの中でのみ使用できます"),
    ] {
        let cases = [
            format!("print(1)\n{stmt}\nprint(2)\n"),
            format!("print(1)\nif true\n  {stmt}\nend\n"),
            format!("fn f()\n  print(1)\n  {stmt}\nend\nf()\nprint(2)\n"),
            format!("let g = fn()\n  {stmt}\nend\nprint(1)\ng()\n"),
            format!("fn g()\n  {stmt}\nend\nfn f()\n  g()\nend\nprint(1)\nf()\n"),
            format!("print(1)\nlet r = map([1, 2], fn(x)\n  {stmt}\nend)\n"),
            format!("for i in [1]\n  fn h()\n    {stmt}\n  end\n  print(i)\n  h()\nend\n"),
        ];
        for source in &cases {
            let observed = assert_same(&[], source);
            assert_eq!(observed.code, Some(1), "{source}: {observed:?}");
            assert!(observed.stderr.contains(message), "{source}: {observed:?}");
            assert!(!observed.stdout.contains('2'), "{source}: {observed:?}");
            assert!(observed.stdout.starts_with("1\n"), "{source}: {observed:?}");
        }
    }
}

#[test]
fn vlc_at_02_call_trace_excludes_the_frame_that_holds_the_statement() {
    let source = "fn g()\n  break\nend\nfn f()\n  g()\nend\nprint(1)\nf()\n";
    let observed = assert_same(&[], source);
    assert_eq!(observed.code, Some(1), "{observed:?}");
    assert!(observed.stderr.contains("2行目: break"), "{observed:?}");
    // `g` の frame を畳んでから error を作るので、trace は `f` の呼び出し行だけ。
    assert!(observed.stderr.contains("  in f() (8行目)"), "{observed:?}");
    assert!(!observed.stderr.contains("in g()"), "{observed:?}");
}

#[test]
fn vlc_at_02_module_top_level_loop_control() {
    let files = [("lib.tsg", "print(\"lib\")\nbreak\n")];
    let main = "print(\"main\")\nimport \"lib.tsg\"\nprint(\"unreachable\")\n";
    let observed = assert_same(&files, main);
    assert_eq!(observed.code, Some(1), "{observed:?}");
    assert_eq!(observed.stdout, "main\nlib\n", "{observed:?}");
    assert!(observed.stderr.contains("break はループの中でのみ"));
}

// ---------------------------------------------------------------------------
// VLC-AT-03: catch できる範囲が tree と同じ
// ---------------------------------------------------------------------------

#[test]
fn vlc_at_03_same_body_try_does_not_catch() {
    for stmt in ["break", "continue"] {
        let cases = [
            // トップレベルの try の中
            format!("print(1)\ntry\n  {stmt}\ncatch e\n  print(\"caught\")\nend\nprint(2)\n"),
            // catch ブロックの中
            format!("print(1)\ntry\n  throw_it\ncatch e\n  {stmt}\nend\nprint(2)\n"),
            // 入れ子の try の中
            format!(
                "print(1)\ntry\n  try\n    {stmt}\n  catch a\n    print(\"inner\")\n  end\ncatch b\n    print(\"outer\")\nend\n"
            ),
            // 関数本体の try の中。呼び出し元のループも抜けない。
            format!(
                "fn f()\n  try\n    {stmt}\n  catch e\n    print(\"in f\")\n  end\nend\nprint(1)\nfor i in [1, 2]\n  f()\nend\n"
            ),
        ];
        for source in &cases {
            let observed = assert_same(&[], source);
            assert_eq!(observed.code, Some(1), "{source}: {observed:?}");
            for banned in ["caught", "inner", "outer", "in f"] {
                assert!(!observed.stdout.contains(banned), "{source}: {observed:?}");
            }
        }
    }
}

#[test]
fn vlc_at_03_caller_try_catches_control_flow() {
    for stmt in ["break", "continue"] {
        let source = format!(
            "fn f()\n  {stmt}\nend\n\
             try\n  f()\ncatch e\n  print(e[\"type\"])\n  print(e[\"line\"])\nend\n\
             let g = fn()\n  {stmt}\nend\n\
             for i in [1]\n  try\n    g()\n  catch e\n    print(\"lambda \" + e[\"type\"])\n  end\nend\n\
             try\n  let r = map([1], fn(x)\n    {stmt}\n  end)\ncatch e\n    print(\"cb \" + to_str(e[\"line\"]))\nend\n\
             print(\"after\")\n"
        );
        let observed = assert_same(&[], &source);
        assert_eq!(observed.code, Some(0), "{stmt}: {observed:?}");
        assert!(
            observed
                .stdout
                .starts_with("control_flow\n2\nlambda control_flow\n"),
            "{stmt}: {observed:?}"
        );
        assert!(observed.stdout.ends_with("after\n"), "{stmt}: {observed:?}");
    }
}

#[test]
fn vlc_at_03_torn_down_handler_does_not_catch_later_errors() {
    // try の中で break に到達し、呼び出し元の try で catch された後に、別の error を起こす。
    // 外した handler が残っていると、後の error を誤って catch する。
    let source = "fn f()\n  try\n    break\n  catch e\n    print(\"in f\")\n  end\nend\n\
                  try\n  f()\ncatch e\n  print(\"caught control_flow\")\nend\n\
                  print(1)\nlet x = 1 / 0\nprint(\"unreachable\")\n";
    let observed = assert_same(&[], source);
    assert_eq!(observed.code, Some(1), "{observed:?}");
    assert_eq!(observed.stdout, "caught control_flow\n1\n", "{observed:?}");
}

// ---------------------------------------------------------------------------
// VLC-AT-04: REPL
// ---------------------------------------------------------------------------

#[test]
fn vlc_at_04_repl_rolls_back_and_continues() {
    let input =
        "let a = 1\nif true\n  a = 2\n  print(\"in\")\n  break\nend\nprint(a)\nprint(\"end\")\n";
    let tree = run_repl(input, false);
    let vm = run_repl(input, true);
    let tree_lines = visible_lines(&tree.stdout, false);
    let vm_lines = visible_lines(&vm.stdout, true);
    assert_eq!(tree_lines, vm_lines, "{tree:?} {vm:?}");
    assert_eq!(tree.stderr, vm.stderr);
    assert_eq!(tree.code, vm.code);
    assert!(
        tree.stderr.contains("4行目: break はループの中でのみ"),
        "{tree:?}"
    );
    // 失敗した入力が `a = 2` でトップレベルの状態を変えた後に break へ到達しても、rollback で `a` は 1 に戻る。
    assert!(tree_lines.iter().any(|l| l == "in"), "{tree_lines:?}");
    assert!(tree_lines.iter().any(|l| l == "1"), "{tree_lines:?}");
    assert!(!tree_lines.iter().any(|l| l == "2"), "{tree_lines:?}");
    assert!(tree_lines.iter().any(|l| l == "end"), "{tree_lines:?}");
}

//! fix-vm-builtin-call-lowering（issue #292）: VM の builtin 名の呼び出しの lowering が
//! `push(xs)` で panic せず、入れ子の深さに線形である。受入基準 VBC-AT-01..05
//! （docs/determinism-and-audit.md の同名スライス小節）。
//!
//! VBC-AT-02（命令数の線形性）は `src/compiler.rs` の unit test、VBC-AT-05 の verifier 側は
//! `src/verifier.rs` の unit test で確かめる。

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tsumugi::chunk::Chunk;
use tsumugi::opcode::OpCode;
use tsumugi::value::Value;
use tsumugi::verifier::VerifiedChunk;
use tsumugi::vm::Vm;
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
        let mut path = std::env::temp_dir();
        path.push(format!("tsumugi-vbc-{}-{}", std::process::id(), n));
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
    success: bool,
}

fn finish(output: std::process::Output) -> Observed {
    Observed {
        stdout: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
        success: output.status.success(),
    }
}

/// `files` を置いた作業ディレクトリで `main.tsg` を実行する。
fn run_script(files: &[(&str, &str)], main: &str, use_vm: bool) -> (Observed, Duration) {
    let dir = TestDir::new();
    for (name, text) in files {
        std::fs::write(dir.path.join(name), text).expect("write file");
    }
    std::fs::write(dir.path.join("main.tsg"), main).expect("write main");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    command
        .arg("main.tsg")
        .current_dir(&dir.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let start = Instant::now();
    let output = command.output().expect("run tsumugi");
    (finish(output), start.elapsed())
}

/// tree と VM で同じ観測結果（stdout・stderr・終了状態）になることを確かめて返す。
fn assert_same(files: &[(&str, &str)], main: &str) -> Observed {
    let (tree, _) = run_script(files, main, false);
    let (vm, _) = run_script(files, main, true);
    assert_eq!(tree, vm, "tree と VM が食い違った: {main}");
    assert!(
        !vm.stderr.contains("panicked"),
        "VM が panic した: {}",
        vm.stderr
    );
    tree
}

fn run_repl(inputs: &str, use_vm: bool) -> Observed {
    use std::io::Write as _;
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
    stdin.write_all(inputs.as_bytes()).expect("write stdin");
    drop(stdin);
    let mut observed = finish(child.wait_with_output().expect("wait repl"));
    // banner と prompt（VM mode の表示）の違いを除く。
    observed.stdout = observed
        .stdout
        .replace("tsumugi:vm>", "tsumugi>")
        .replace(" [VM mode]", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    observed
}

fn nested(open: &str, inner: &str, close: &str, depth: usize) -> String {
    format!("{}{}{}", open.repeat(depth), inner, close.repeat(depth))
}

// ---------------------------------------------------------------------------
// VBC-AT-01: 引数の個数・形の誤りで panic しない
// ---------------------------------------------------------------------------

#[test]
fn vbc_at_01_wrong_arity_and_shape_match_tree() {
    let calls = [
        "push(xs)",
        "push(xs, 1, 2)",
        "push()",
        "pop(xs, 1)",
        "pop()",
        "push([1], 2)",
        "push(xs, print(\"a\"), print(\"b\"))",
    ];
    for call in calls {
        // catch できて、catch の後の文が実行される。
        let caught =
            format!("let xs = []\ntry\n  {call}\ncatch e\n  print(e)\nend\nprint(\"after\")\n");
        let observed = assert_same(&[], &caught);
        assert!(observed.success, "{call}: {observed:?}");
        assert!(observed.stdout.contains("after"), "{call}: {observed:?}");
        // 引数を評価しない: print("a") / print("b") の出力が無い。
        assert!(
            !observed
                .stdout
                .lines()
                .any(|line| line == "a" || line == "b"),
            "{call}: 引数が評価された: {observed:?}"
        );
        // catch しない形は、両 backend とも同じ error で止まる。
        let uncaught = format!("let xs = []\n{call}\nprint(\"after\")\n");
        let observed = assert_same(&[], &uncaught);
        assert!(!observed.success, "{call}: {observed:?}");
        assert!(!observed.stdout.contains("after"), "{call}: {observed:?}");
    }
}

#[test]
fn vbc_at_01_library_compile_does_not_panic() {
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
    for call in [
        "push(xs)",
        "push(xs, 1, 2)",
        "push()",
        "pop(xs, 1)",
        "pop()",
    ] {
        let source = format!("let xs = []\n{call}\n");
        let result = engine.compile(
            Source::new(SourceId::new("vbc").expect("source id"), &source),
            &CompileOptions::default(),
        );
        // compile は panic（InternalFault）にならない。
        if let Err(errors) = result {
            for diagnostic in &errors.diagnostics {
                assert_ne!(
                    diagnostic.code,
                    tsumugi::CompileDiagnosticCode::InternalFault,
                    "{call}: {diagnostic:?}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// VBC-AT-03: 深い入れ子を実行できる
// ---------------------------------------------------------------------------

#[test]
fn vbc_at_03_deep_nesting_runs_like_tree() {
    // abs / to_int の混在 200 段。
    let mut inner = String::from("1");
    for i in 0..200 {
        inner = if i % 2 == 0 {
            format!("abs({inner})")
        } else {
            format!("to_int({inner})")
        };
    }
    let (tree, _) = run_script(&[], &format!("print({inner})\n"), false);
    let (vm, elapsed) = run_script(&[], &format!("print({inner})\n"), true);
    assert_eq!(tree, vm);
    assert_eq!(vm.stdout, "1\n", "{vm:?}");
    assert!(elapsed < Duration::from_secs(10), "遅すぎる: {elapsed:?}");

    // push の 50 段（push は Null を返すので、引数の中の push は Null を積む）。
    let mut source = String::from("let xs = []\n");
    let mut expr = String::from("1");
    for _ in 0..50 {
        expr = format!("push(xs, {expr})");
    }
    source.push_str(&format!("{expr}\nprint(len(xs))\nprint(xs)\n"));
    let (tree, _) = run_script(&[], &source, false);
    let (vm, elapsed) = run_script(&[], &source, true);
    assert_eq!(tree, vm);
    assert!(vm.success, "{vm:?}");
    assert!(elapsed < Duration::from_secs(10), "遅すぎる: {elapsed:?}");
}

// ---------------------------------------------------------------------------
// VBC-AT-04: user binding の優先と資源の一致
// ---------------------------------------------------------------------------

#[test]
fn vbc_at_04_user_binding_wins_in_nested_calls() {
    // 呼び出しより前に宣言した同名関数。
    let before =
        "fn abs(x)\n  return x + 100\nend\nprint(abs(abs(1)))\nprint(len(to_str(abs(2))))\n";
    let observed = assert_same(&[], before);
    assert_eq!(observed.stdout, "201\n3\n");

    // 後で宣言した関数から呼ぶ。
    let later =
        "fn f(x)\n  return abs(abs(x))\nend\nfn abs(x)\n  return x + 100\nend\nprint(f(1))\n";
    let observed = assert_same(&[], later);
    assert_eq!(observed.stdout, "201\n");

    // 入れ子の内側だけ user binding（str を差し替える）。
    let mixed = "fn to_str(x)\n  return \"s\"\nend\nprint(abs(len(to_str(abs(-3)))))\n";
    assert_same(&[], mixed);

    // 同名の user 関数が push / pop の入れ子の途中にある。
    let push_user = "fn push(a, b)\n  return 7\nend\nlet xs = []\nprint(push(xs, push(xs, 1)))\n";
    let observed = assert_same(&[], push_user);
    assert_eq!(observed.stdout, "7\n");

    // import した関数。
    let module = "export fn abs(x)\n  return x + 100\nend\n";
    let main = "import \"./m.tsg\"\nprint(abs(abs(1)))\n";
    let (tree, _) = run_script(&[("m.tsg", module)], main, false);
    let (vm, _) = run_script(&[("m.tsg", module)], main, true);
    assert_eq!(tree, vm);
}

#[test]
fn vbc_at_04_repl_later_definition_wins() {
    // 前の入力で builtin として解決される呼び出しを含む関数を定義し、後の入力で同名の
    // 関数を定義する。実行時の binding が優先される。
    let inputs = "fn g(x)\n  return abs(abs(x))\nend\nprint(g(-2))\nfn abs(x)\n  return x + 100\nend\nprint(g(-2))\nlet xs = []\npush(xs, abs(1))\nprint(xs)\n";
    let tree = run_repl(inputs, false);
    let vm = run_repl(inputs, true);
    assert_eq!(tree, vm);
}

#[test]
fn vbc_at_04_budget_and_output_match_on_nested_builtins() {
    let mut source = String::from("let xs = []\nlet ys = []\n");
    source.push_str(&format!("print({})\n", nested("abs(", "-1", ")", 30)));
    let mut expr = String::from("1");
    for i in 0..20 {
        expr = if i % 2 == 0 {
            format!("push(xs, {expr})")
        } else {
            format!("push(ys, {expr})")
        };
    }
    source.push_str(&format!("{expr}\nprint(len(xs), len(ys))\n"));
    // 予算を絞った環境でも同じ観測結果（同じ step / heap の上限で同じ成否）になる。
    for fuel in ["100000", "1000", "40"] {
        let (tree, _) = run_with_env(&source, false, fuel);
        let (vm, _) = run_with_env(&source, true, fuel);
        assert_eq!(tree.success, vm.success, "fuel={fuel}: {tree:?} / {vm:?}");
        assert_eq!(tree.stdout, vm.stdout, "fuel={fuel}");
    }
    assert_same(&[], &source);
}

fn run_with_env(main: &str, use_vm: bool, steps: &str) -> (Observed, Duration) {
    let dir = TestDir::new();
    std::fs::write(dir.path.join("main.tsg"), main).expect("write main");
    let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
    if use_vm {
        command.arg("--vm");
    }
    command
        .arg("main.tsg")
        .env("TSUMUGI_MAX_STEPS", steps)
        .current_dir(&dir.path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let start = Instant::now();
    let output = command.output().expect("run tsumugi");
    (finish(output), start.elapsed())
}

// ---------------------------------------------------------------------------
// VBC-AT-05: 防御（不正な bytecode は internal error）
// ---------------------------------------------------------------------------

fn run_chunk(chunk: Chunk) -> tsumugi::error::TsumugiError {
    Vm::new(VerifiedChunk::from_trusted(chunk))
        .run()
        .expect_err("不正な命令列が成功しました")
}

#[test]
fn vbc_at_05_jump_if_builtin_mark_without_operands_is_internal_error() {
    // 印の位置（トップから 引数の数 + 1 番目）に要素が無い。
    let mut chunk = Chunk::new();
    chunk.emit(OpCode::JumpIfBuiltinMark(2, 1), 1);
    chunk.emit(OpCode::Return, 1);
    assert_eq!(run_chunk(chunk).error_type(), "internal");

    // 引数の数が usize::MAX でも panic しない。
    let mut chunk = Chunk::new();
    chunk.emit(OpCode::JumpIfBuiltinMark(usize::MAX, 1), 1);
    chunk.emit(OpCode::Return, 1);
    assert_eq!(run_chunk(chunk).error_type(), "internal");
}

#[test]
fn vbc_at_05_misplaced_mark_is_internal_error() {
    // 印（Null）の後の stack が builtin の後処理に足りない: CallBuiltin が internal error。
    let mut chunk = Chunk::new();
    let null = chunk.add_constant(Value::Null);
    chunk.emit(OpCode::LoadConst(null), 1); // 印
    chunk.emit(OpCode::JumpIfBuiltinMark(0, 3), 1); // 取り除いて jump
    chunk.emit(OpCode::Return, 1);
    let abs = tsumugi::builtin_registry::id_of("abs").expect("abs");
    chunk.emit(OpCode::CallBuiltin(abs, 1), 1); // 引数が無い
    chunk.emit(OpCode::Return, 1);
    assert_eq!(run_chunk(chunk).error_type(), "internal");

    // 印の位置が Null でない（user 経路の扱い）なら Call に進み、callee 検査で止まる。
    // どちらも host panic せず、構造化 error になる。
    let mut chunk = Chunk::new();
    let one = chunk.add_constant(Value::Int(1));
    chunk.emit(OpCode::LoadConst(one), 1);
    chunk.emit(OpCode::JumpIfBuiltinMark(0, 4), 1);
    chunk.emit(OpCode::Call(0), 1);
    chunk.emit(OpCode::Return, 1);
    let error = run_chunk(chunk);
    assert_ne!(error.error_type(), "");
}

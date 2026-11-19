//! fix-vm-for-hidden-locals（issue #365）: VM の `for` が内部で持つ反復列と位置を、スクリプトの名前
//! `__collection__` / `__index__` から読み書きできないようにし、tree と同じ未定義名の error にする。
//! 受入基準 VFH-AT-01..03（docs/determinism-and-audit.md の同名スライス小節）。VFH-AT-04 は
//! `src/compiler.rs` の unit test と既存 test。
//!
//! 各 test は tree と `--vm` の CLI（と REPL）を同じ script で実行し、stdout・stderr・終了コードの
//! 一致と、tree 側の期待値（両方が同じ誤りをしていないこと）を確かめる。

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("tsumugi-vfh-{}-{}", std::process::id(), n));
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

const READ_ERR_INDEX: &str = "未定義の変数または関数: __index__";
const READ_ERR_COLLECTION: &str = "未定義の変数または関数: __collection__";

fn assert_fails(main: &str, stdout: &str, message: &str) {
    let o = assert_same(&[], main);
    assert_eq!(o.stdout, stdout, "{main}");
    assert_eq!(o.code, Some(1), "{main}: {o:?}");
    assert!(o.stderr.contains(message), "{main}: {o:?}");
}

// ---------------------------------------------------------------------------
// VFH-AT-01: 読めない
// ---------------------------------------------------------------------------

#[test]
fn vfh_at_01_reading_hidden_names_is_name_error() {
    for (name, msg) in [
        ("__index__", READ_ERR_INDEX),
        ("__collection__", READ_ERR_COLLECTION),
    ] {
        // トップレベル
        assert_fails(&format!("for x in [1, 2]\n  print({name})\nend\n"), "", msg);
        // 関数本体
        assert_fails(
            &format!(
                "fn f()\n  for x in [1, 2]\n    print(x)\n    print({name})\n  end\nend\nf()\n"
            ),
            "1\n",
            msg,
        );
        // 入れ子の for
        assert_fails(
            &format!("for a in [1]\n  for b in [2]\n    print({name})\n  end\nend\n"),
            "",
            msg,
        );
        // 本体の lambda（upvalue の捕捉）
        assert_fails(
            &format!(
                "for x in [1, 2]\n  let g = fn()\n    return {name}\n  end\n  print(g())\nend\n"
            ),
            "",
            msg,
        );
        // 本体の関数
        assert_fails(
            &format!("for x in [1]\n  fn h()\n    return {name}\n  end\n  print(h())\nend\n"),
            "",
            msg,
        );
        // 2 段の入れ子の lambda
        assert_fails(
            &format!(
                "for x in [1]\n  let m = fn()\n    let c = fn()\n      return {name}\n    end\n    return c()\n  end\n  print(m())\nend\n"
            ),
            "",
            msg,
        );
        // 関数の中の for の本体の lambda
        assert_fails(
            &format!(
                "fn f()\n  for x in [1]\n    let g = fn()\n      return {name}\n    end\n    print(g())\n  end\nend\nf()\n"
            ),
            "",
            msg,
        );
    }
}

#[test]
fn vfh_at_01_hidden_name_read_can_be_caught() {
    for name in ["__index__", "__collection__"] {
        let source = format!(
            "for x in [1, 2]\n  try\n    print({name})\n  catch e\n    print(e[\"type\"])\n  end\n  print(x)\nend\nprint(\"end\")\n"
        );
        let o = assert_same(&[], &source);
        assert_eq!(o.stdout, "name\n1\nname\n2\nend\n", "{name}: {o:?}");
        assert_eq!(o.code, Some(0), "{name}: {o:?}");
    }
}

// ---------------------------------------------------------------------------
// VFH-AT-02: 書けない
// ---------------------------------------------------------------------------

#[test]
fn vfh_at_02_assigning_hidden_names_is_name_error() {
    assert_fails(
        "for x in [\"a\", \"b\", \"c\", \"d\"]\n  print(x)\n  __index__ = 3\nend\n",
        "a\n",
        "未定義の変数に代入: __index__",
    );
    assert_fails(
        "for x in [1, 2]\n  __collection__ = [9, 9, 9]\n  print(x)\nend\n",
        "",
        "未定義の変数に代入: __collection__",
    );
    assert_fails(
        "for x in [1, 2]\n  __collection__ = 5\nend\n",
        "",
        "未定義の変数に代入: __collection__",
    );
    assert_fails(
        "for x in [1, 2]\n  __index__ = -5\n  print(x)\nend\n",
        "",
        "未定義の変数に代入: __index__",
    );
    // 入れ子の lambda からの代入
    assert_fails(
        "for x in [1, 2]\n  let g = fn()\n    let h = fn()\n      __index__ = 1\n    end\n    h()\n  end\n  g()\n  print(x)\nend\n",
        "",
        "未定義の変数に代入: __index__",
    );
}

// ---------------------------------------------------------------------------
// VFH-AT-03: 利用者の同名 binding を隠さない
// ---------------------------------------------------------------------------

#[test]
fn vfh_at_03_user_bindings_with_same_names_are_visible() {
    let cases: &[(&str, &str)] = &[
        (
            "let __index__ = 10\nfor x in [1, 2]\n  print(__index__)\nend\nprint(__index__)\n",
            "10\n10\n10\n",
        ),
        (
            "let __collection__ = \"c\"\nfor x in [1, 2]\n  print(__collection__)\nend\n",
            "c\nc\n",
        ),
        (
            "let __index__ = 10\nfor x in [1, 2]\n  let g = fn()\n    return __index__\n  end\n  print(g())\n  __index__ = __index__ + 1\nend\nprint(__index__)\n",
            "10\n11\n12\n",
        ),
        (
            "fn g()\n  let __index__ = 10\n  for x in [1]\n    let m = fn()\n      let c = fn()\n        return __index__\n      end\n      return c()\n    end\n    print(m())\n  end\nend\ng()\n",
            "10\n",
        ),
        (
            "fn f()\n  for x in [1, 2]\n    print(__index__)\n  end\nend\nlet __index__ = 5\nf()\n",
            "5\n5\n",
        ),
        (
            "for __index__ in [7, 8]\n  print(__index__)\nend\n",
            "7\n8\n",
        ),
        (
            "for __collection__ in [7, 8]\n  print(__collection__)\nend\n",
            "7\n8\n",
        ),
        (
            "for __index__ in [1, 2]\n  for y in [3]\n    print(__index__)\n    let g = fn()\n      return __index__\n    end\n    print(g())\n  end\nend\n",
            "1\n1\n2\n2\n",
        ),
    ];
    for (source, expected) in cases {
        let o = assert_same(&[], source);
        assert_eq!(o.stdout, *expected, "{source}: {o:?}");
        assert_eq!(o.code, Some(0), "{source}: {o:?}");
    }
}

#[test]
fn vfh_at_03_repl_user_binding_is_visible_in_later_for() {
    let source = "let __index__ = 10\nfor x in [1, 2]\n  print(__index__)\nend\nprint(__index__)\nfor y in [3]\n  print(__index__)\nend\n";
    let tree = run_repl(source, false);
    let vm = run_repl(source, true);
    assert_eq!(tree.code, vm.code);
    // REPL の出力は prompt と継続行の記号が混ざるので、`10` で終わる行の数を数える（4 回出る）。
    let count = |o: &Observed| {
        o.stdout
            .lines()
            .filter(|l| l.trim_end().ends_with("10"))
            .count()
    };
    assert_eq!(count(&tree), 4, "tree={tree:?}");
    assert_eq!(count(&vm), 4, "vm={vm:?}");
    assert_eq!(tree.stderr, vm.stderr);
}

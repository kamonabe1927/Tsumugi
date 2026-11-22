//! CAP-AT-27: core から ambient OS access が除去されたことを静的 + 動的に固定する（C10）。
//!
//! 静的: `src/` 配下（サブディレクトリを含む）の全 `*.rs` の production コードに、ambient な
//! OS・network・process・clock・thread の直接呼びが、理由付きの許可リスト（ファイル×パターンの組）
//! 以外に無いことを固定する（loop-guard §4.1 の 1 段目。manifesto-check-ambient-surface、
//! docs/capability-model.md の同名小節）。production コードは、`#[cfg(test)]` の付いた item（`mod`・
//! 文・関数・impl・フィールド・引数。production の build に入らないもの）を除いた部分で、切り出しは
//! tests/support/source_scan.rs の `production_lines` を tests/no_host_abort_audit.rs と共有する
//! （除く範囲はそこに書く。除くのはその item だけで、走査は続く）。許可リストの組が 1 度も一致しなければ
//! 検査は失敗する（古い許可を残さない）。
//! legacy budget env（`TSUMUGI_MAX_*`）は CLI adapter（`main.rs`）だけが読み、library の `budget.rs`
//! は env を読まない（execution-control §13 / §14 Slice 1）。
//!
//! 動的: safe 既定で fs/env/clock/stdin/exit を試み、未 grant で Denial（ambient へ漏れない）
//! ことを tree/VM で確認する。
//!
//! 検査項目・守る原則・CAP-AT-27 の対応表（MAS-AT-04）。原則はマニフェストの番号。
//!
//! | 検査項目（パターンの key） | 守る原則 | CAP-AT-27 の対象 |
//! |---|---|---|
//! | `env`（`std::env::` のすべて） | 2・8 | process env の直接利用 0 |
//! | `fs`（`std::fs`） | 2・8 | runtime fs の直接利用 0 |
//! | `stdio`（`io::stdin`・`stdout`・`stderr`） | 2・8 | stdio の直接利用 0 |
//! | `process`（`exit`・`Command`・`abort`・`id`） | 2・8 | process exit の直接利用 0 |
//! | `net`（`std::net`） | 2・8 | 原則 8（外部効果はホスト境界の後ろ）の network 側 |
//! | `SystemTime`・`Instant::now`・`UNIX_EPOCH` | 2・8 | ambient clock の直接利用 0 |
//! | `thread::spawn`・`Builder`・`park`・`sleep`・`std::thread` の取り込み | 1・3（と 8） | CAP-AT-27 の対象外。ホストの安定性と同時実行の制御 |
//!
//! 限界: 行単位の文字列照合なので、許可した組のファイルの中の同じパターンの別の使い方（例:
//! `builtin_core.rs` の `UNIX_EPOCH.elapsed()`）、capability から受け取った `SystemTime` への
//! `.elapsed()`、`use std::io as x;` のような別名・再 export を通した呼び、`use ... as` の別名の
//! あとの短縮名の呼びは検出しない（取り込み自体を違反として数え、短縮名の呼びは追わない）。
//! これらはレビューで見る。

use std::path::{Path, PathBuf};
use std::process::Command;

// production コードの切り出しと照合は、原則 6 の検査（tests/no_host_abort_audit.rs）と共有する。
#[path = "support/source_scan.rs"]
mod source_scan;
use source_scan::{Pattern, Violation, collect_rs};

fn tsumugi_bin() -> &'static str {
    env!("CARGO_BIN_EXE_tsumugi")
}

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

const PATTERNS: &[Pattern] = &[
    Pattern {
        key: "env",
        texts: &["std::env::"],
        use_paths: &["std::env"],
        use_modules: &[],
    },
    Pattern {
        key: "fs",
        texts: &["std::fs"],
        use_paths: &["std::fs"],
        use_modules: &[],
    },
    Pattern {
        key: "stdio",
        texts: &["io::stdin", "io::stdout", "io::stderr"],
        use_paths: &["std::io::stdin", "std::io::stdout", "std::io::stderr"],
        use_modules: &[],
    },
    Pattern {
        key: "process",
        texts: &[
            "std::process::exit",
            "std::process::Command",
            "std::process::abort",
            "std::process::id",
        ],
        use_paths: &[
            "std::process::exit",
            "std::process::Command",
            "std::process::abort",
            "std::process::id",
        ],
        use_modules: &["std::process"],
    },
    Pattern {
        key: "net",
        texts: &["std::net::"],
        use_paths: &["std::net"],
        use_modules: &[],
    },
    Pattern {
        key: "thread::spawn",
        texts: &["std::thread::spawn"],
        use_paths: &["std::thread::spawn"],
        use_modules: &[],
    },
    Pattern {
        key: "thread::Builder",
        texts: &["std::thread::Builder"],
        use_paths: &["std::thread::Builder"],
        use_modules: &[],
    },
    Pattern {
        key: "thread::park",
        texts: &["std::thread::park*"],
        use_paths: &["std::thread::park", "std::thread::park_timeout"],
        use_modules: &[],
    },
    Pattern {
        key: "thread::sleep",
        texts: &["std::thread::sleep"],
        use_paths: &["std::thread::sleep"],
        use_modules: &[],
    },
    Pattern {
        key: "thread",
        texts: &[],
        use_paths: &[],
        use_modules: &["std::thread"],
    },
    Pattern {
        key: "SystemTime",
        texts: &["std::time::SystemTime", "SystemTime::now"],
        use_paths: &["std::time::SystemTime"],
        use_modules: &[],
    },
    Pattern {
        key: "Instant::now",
        texts: &["Instant::now"],
        use_paths: &[],
        use_modules: &[],
    },
    Pattern {
        key: "UNIX_EPOCH",
        texts: &["UNIX_EPOCH"],
        use_paths: &["std::time::UNIX_EPOCH"],
        use_modules: &[],
    },
];

/// 許可リストの 1 組。ファイル（`src/` からの相対 path）×パターンの key に、理由と原則を持たせる。
struct Allow {
    file: &'static str,
    key: &'static str,
    reason: &'static str,
    principle: &'static str,
}

const ALLOWLIST: &[Allow] = &[
    // CLI adapter。profile から frozen set を組む境界と、プロセスの入口で OS を触る。
    Allow {
        file: "main.rs",
        key: "process",
        reason: "CLI adapter の exit code でプロセスを終える（script の exit は capability 経由）",
        principle: "2・8",
    },
    Allow {
        file: "main.rs",
        key: "fs",
        reason: "CLI adapter が root source を読む",
        principle: "2・8",
    },
    Allow {
        file: "main.rs",
        key: "env",
        reason: "CLI adapter が budget env などを読み、profile へ写す（budget env は CLI だけが読む）",
        principle: "2・8",
    },
    Allow {
        file: "main.rs",
        key: "stdio",
        reason: "CLI adapter が stdin・stdout を capability の実体として渡す",
        principle: "2・8",
    },
    Allow {
        file: "main.rs",
        key: "thread::Builder",
        reason: "CLI adapter の実行スレッドのスタック確保（ホスト側の thread で、script が作るものではない）",
        principle: "1・3",
    },
    Allow {
        file: "cli_capability.rs",
        key: "env",
        reason: "CLI adapter。profile から frozen set を組む境界で env を snapshot する",
        principle: "2・8",
    },
    // capability adapter（OS 実装そのもの）。
    Allow {
        file: "capability.rs",
        key: "fs",
        reason: "System* / OsDirectoryHandle adapter（OS 実装そのもの）",
        principle: "2・8",
    },
    Allow {
        file: "capability.rs",
        key: "env",
        reason: "SystemEnvironment adapter の current_dir など",
        principle: "2・8",
    },
    Allow {
        file: "capability.rs",
        key: "stdio",
        reason: "SystemStdin・SystemStdout adapter",
        principle: "2・8",
    },
    Allow {
        file: "capability.rs",
        key: "SystemTime",
        reason: "SystemClock adapter。Clock capability の実体",
        principle: "2・8",
    },
    // ホスト側の adapter・待機・開発用ツール。
    Allow {
        file: "host_pending.rs",
        key: "thread::spawn",
        reason: "ホスト側の adapter executor が adapter の待ちを別 thread で動かす（script が作る thread ではない）",
        principle: "1・3（と 8）",
    },
    Allow {
        file: "embedding.rs",
        key: "thread::park",
        reason: "同期 wrapper がホストの thread で Pending の待機をする（ホストが選んだ同期 API の中）",
        principle: "1・3（と 8）",
    },
    Allow {
        file: "budget.rs",
        key: "Instant::now",
        reason: "SystemMonotonicClock。wall-clock の予算を測る monotonic clock の実体（script からは見えない）",
        principle: "2・8",
    },
    Allow {
        file: "bin/gen_builtins_doc.rs",
        key: "fs",
        reason: "開発用ツール（builtin 一覧の文書生成）。library の経路ではない",
        principle: "2・8",
    },
    Allow {
        file: "builtin_core.rs",
        key: "UNIX_EPOCH",
        reason: "Clock capability の now_utc() から受け取った値を秒へ写す変換で、system clock は読まない（ambient な呼びの例外ではなく capability 経由の値の変換）",
        principle: "2",
    },
];

/// 1 つのファイルの production コードを走査する（切り出しと照合は共有の helper）。
fn scan_source(rel: &str, content: &str) -> Vec<Violation> {
    source_scan::scan_source(rel, content, PATTERNS)
}

/// 違反と許可リストを突き合わせて、許可されていない違反と、一致しない許可の組を文章にして返す。
fn audit(found: &[Violation], allow: &[Allow]) -> Vec<String> {
    let mut problems = Vec::new();
    for v in found {
        if !allow.iter().any(|a| a.file == v.file && a.key == v.key) {
            problems.push(format!("{}:{} [{}]: {}", v.file, v.line, v.key, v.text));
        }
    }
    for a in allow {
        if !found.iter().any(|v| v.file == a.file && v.key == a.key) {
            problems.push(format!(
                "許可リストの組 {}×{} は 1 度も一致しない（古い許可は削除する）",
                a.file, a.key
            ));
        }
    }
    problems
}

#[test]
fn core_has_no_direct_ambient_os_access() {
    let dir = src_dir();
    let mut files = Vec::new();
    collect_rs(&dir, &mut files);
    files.sort();
    let mut found = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(&dir)
            .expect("under src")
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(path).expect("read file");
        found.extend(scan_source(&rel, &content));
    }
    let problems = audit(&found, ALLOWLIST);
    assert!(
        problems.is_empty(),
        "src/ の production コードの ambient 直接呼びが許可リストと合いません:\n{}",
        problems.join("\n")
    );
}

#[test]
fn allowlist_entries_have_reason_and_principle() {
    // MAS-AT-03: 各組に理由と原則があり、key は実在するパターンで、組は重複しない。
    for (i, a) in ALLOWLIST.iter().enumerate() {
        assert!(
            !a.reason.trim().is_empty(),
            "{}×{}: 理由が無い",
            a.file,
            a.key
        );
        assert!(
            !a.principle.trim().is_empty(),
            "{}×{}: 原則が無い",
            a.file,
            a.key
        );
        assert!(
            PATTERNS.iter().any(|p| p.key == a.key),
            "{}×{}: 未知のパターン key",
            a.file,
            a.key
        );
        assert!(
            !ALLOWLIST[..i]
                .iter()
                .any(|b| b.file == a.file && b.key == a.key),
            "{}×{}: 許可の組が重複している",
            a.file,
            a.key
        );
    }
}

#[test]
fn audit_fails_on_unmatched_or_unlisted() {
    // MAS-AT-03: 許可の無い違反と、一致しない許可の組の両方が検査を失敗させる。
    let v = scan_source("x.rs", "fn f() { let _ = std::env::args(); }\n");
    assert_eq!(v.len(), 1);
    let none: &[Allow] = &[];
    assert_eq!(audit(&v, none).len(), 1, "許可の無い違反");
    let allow = [Allow {
        file: "x.rs",
        key: "env",
        reason: "r",
        principle: "2",
    }];
    assert!(audit(&v, &allow).is_empty(), "組が一致すれば通る");
    let stale = [
        Allow {
            file: "x.rs",
            key: "env",
            reason: "r",
            principle: "2",
        },
        Allow {
            file: "y.rs",
            key: "fs",
            reason: "r",
            principle: "2",
        },
    ];
    let p = audit(&v, &stale);
    assert_eq!(p.len(), 1, "一致しない組: {p:?}");
    assert!(p[0].contains("y.rs×fs"));
    // 同じファイルの別パターンは許可の組に含まれない（ファイル単位の素通りは無い）。
    let v2 = scan_source(
        "x.rs",
        "fn f() { std::fs::read(\"a\"); std::env::args(); }\n",
    );
    assert_eq!(audit(&v2, &allow).len(), 1);
}

fn keys(rel: &str, src: &str) -> Vec<&'static str> {
    let mut k: Vec<&'static str> = scan_source(rel, src).iter().map(|v| v.key).collect();
    k.sort();
    k.dedup();
    k
}

#[test]
fn scan_reaches_subdirectories_and_field_cfg_test() {
    // MAS-AT-01: サブディレクトリ相当の path でも検出され、フィールド・関数に付いた
    // `#[cfg(test)]` では打ち切られない。`#[cfg(test)] mod` の中は除かれる。
    let src = "\
struct S {
    #[cfg(test)]
    probe: u32,
    a: u32,
}
#[cfg(test)]
fn helper() {}
fn prod() { let _ = std::env::var(\"X\"); }
#[cfg(test)]
mod tests {
    fn t() { let _ = std::fs::read(\"a\"); }
    fn braces() { let _ = \"{\"; let _ = '{'; }
}
fn after() { let _ = std::net::TcpStream::connect(\"h\"); }
";
    let v = scan_source("bin/tool.rs", src);
    assert!(v.iter().all(|x| x.file == "bin/tool.rs"));
    assert_eq!(keys("bin/tool.rs", src), vec!["env", "net"]);
    // `#[cfg(test)] mod x;`（1 行）も除かれ、その後ろは走査される。
    let src2 = "#[cfg(test)]\nmod tests;\nfn f() { std::process::exit(1); }\n";
    assert_eq!(keys("lib.rs", src2), vec!["process"]);
    // コメント・文字列の中は数えない。
    let src3 = "// std::env::var\nfn f() { let _ = \"std::fs::read\"; }\n";
    assert!(keys("lib.rs", src3).is_empty());
}

#[test]
fn scan_detects_each_pattern_and_use_imports() {
    // MAS-AT-02: パターンごとの検出と、`use` 経由の取り込み。
    let cases: &[(&str, &str)] = &[
        ("fn f() { std::env::set_var(\"a\", \"b\"); }", "env"),
        ("fn f() { let _ = std::env::args(); }", "env"),
        ("fn f() { std::fs::write(\"a\", \"b\"); }", "fs"),
        ("fn f() { let _ = std::io::stdin(); }", "stdio"),
        ("fn f() { let _ = std::io::stderr(); }", "stdio"),
        ("fn f() { std::process::exit(1); }", "process"),
        ("fn f() { std::process::Command::new(\"ls\"); }", "process"),
        ("fn f() { std::process::abort(); }", "process"),
        ("fn f() { let _ = std::process::id(); }", "process"),
        (
            "fn f() { let _ = std::net::TcpStream::connect(\"h\"); }",
            "net",
        ),
        ("fn f() { std::thread::spawn(|| ()); }", "thread::spawn"),
        ("fn f() { std::thread::Builder::new(); }", "thread::Builder"),
        ("fn f() { std::thread::park_timeout(d); }", "thread::park"),
        ("fn f() { std::thread::sleep(d); }", "thread::sleep"),
        (
            "fn f() { let _ = std::time::SystemTime::now(); }",
            "SystemTime",
        ),
        (
            "fn f() { let _ = std::time::Instant::now(); }",
            "Instant::now",
        ),
        ("fn f() { let _ = UNIX_EPOCH; }", "UNIX_EPOCH"),
        // use 経由の取り込み。
        ("use std::fs;", "fs"),
        ("use std::env as e;", "env"),
        ("use std::process;", "process"),
        ("use std::net::TcpStream;", "net"),
        ("use std::thread;", "thread"),
        ("use std::thread::spawn;", "thread::spawn"),
        ("use std::io::{stdin, Read};", "stdio"),
        ("use std::{fs, io};", "fs"),
        ("use std::time::UNIX_EPOCH;", "UNIX_EPOCH"),
        ("pub use std::fs::File;", "fs"),
        // グロブの取り込み。
        ("use std::process::*;", "process"),
        ("use std::io::*;", "stdio"),
        ("use std::{process::*};", "process"),
        ("use std::env::*;", "env"),
    ];
    for (src, key) in cases {
        assert_eq!(keys("a.rs", src), vec![*key], "src={src}");
    }
    // `std::thread::*` は thread の item をすべて持ち込むので、thread 系の key がすべて当たる。
    assert_eq!(
        keys("a.rs", "use std::thread::*;"),
        vec![
            "thread",
            "thread::Builder",
            "thread::park",
            "thread::sleep",
            "thread::spawn"
        ]
    );
    // 修飾無しの呼び。
    assert_eq!(
        keys("a.rs", "fn f() { SystemTime::now(); }"),
        vec!["SystemTime"]
    );
    assert_eq!(
        keys("a.rs", "fn f() { Instant::now(); }"),
        vec!["Instant::now"]
    );
    assert_eq!(
        keys("a.rs", "fn f() { io::stdin(); io::stdout(); }"),
        vec!["stdio"]
    );
    // 複数行の use のあとの短縮名の呼び。
    let multi = "use std::time::{\n    SystemTime,\n};\nfn f() { let _ = SystemTime::now(); }\n";
    let v = scan_source("a.rs", multi);
    assert!(v.iter().all(|x| x.key == "SystemTime"));
    assert!(v.iter().any(|x| x.line == 1), "use 文の取り込み自体: {v:?}");
    assert!(v.iter().any(|x| x.line == 4), "短縮名の呼び: {v:?}");
    let multi_io = "use std::io;\nfn f() { let _ = io::stdin(); }\n";
    assert_eq!(keys("a.rs", multi_io), vec!["stdio"]);
    // 当たらないもの: trait の取り込み、`std::thread::current`、識別子の途中、module の `use std::io::{self}`。
    for src in [
        "use std::io::{self, Read, Write};",
        "use std::io::BufRead;",
        "use std::time::{Duration, Instant};",
        "fn f() { let _ = std::thread::current(); }",
        "fn f() { let _ = std::thread::available_parallelism(); }",
        "fn f() { MyInstant::now(); }",
        "fn f() { let _ = std::fsx::a(); }",
        "use std::process::ExitCode;",
        "use std::io::prelude::*;",
        "use super::*;",
    ] {
        assert!(keys("a.rs", src).is_empty(), "src={src}");
    }
}

fn run_safe(use_vm: bool, source: &str) -> bool {
    let dir = std::env::temp_dir().join(format!(
        "tsumugi-cap27-{}-{}",
        std::process::id(),
        source.len()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let script = dir.join("s.tsg");
    std::fs::write(&script, source).expect("write");
    let mut cmd = Command::new(tsumugi_bin());
    if use_vm {
        cmd.arg("--vm");
    }
    cmd.arg(&script);
    // 旧 env を除去して safe 既定の deny を純粋に観測する。
    cmd.env_remove("TSUMUGI_SANDBOX");
    cmd.env_remove("TSUMUGI_ENV_ALLOW");
    let ok = cmd.output().expect("spawn").status.success();
    let _ = std::fs::remove_dir_all(&dir);
    ok
}

#[test]
fn safe_default_denies_all_ambient_operations() {
    for use_vm in [false, true] {
        // safe 既定で fs/env/clock/stdin/exit を試みると未 grant で Denial（ambient へ漏れない）。
        for script in [
            "read_file(\"/etc/hostname\")",
            "env(\"HOME\")",
            "now()",
            "input()",
            "exit(0)",
        ] {
            assert!(
                !run_safe(use_vm, script),
                "safe 既定で未 grant 操作は Denial になる [vm={use_vm}] script={script}"
            );
        }
    }
}

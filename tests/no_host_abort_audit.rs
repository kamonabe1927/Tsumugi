//! 原則 6 の静的検査: lib の production コードに、ホストプロセスを panic・abort・exit させうる
//! 無条件の呼びが、理由付きの許可リスト以外に無いことを固定する（loop-guard §4.1 の 1 段目。
//! manifesto-check-no-host-abort、docs/embedding-api.md §14 の同名小節）。
//!
//! 走査するのは `src/` 配下（サブディレクトリを含む）の全 `*.rs` から、CLI adapter（`src/main.rs`）と
//! 開発用ツール（`src/bin/`）を除いたもの。lib の中で script から届く経路は静的に絞れないので、
//! lib の production コード全体を上位集合として見る。production コードの切り出し（`#[cfg(test)]` の
//! 付いた item の除外）は tests/support/source_scan.rs を tests/ambient_access_audit.rs と共有する。
//! 許可リストは「ファイル・パターン・件数」の組で、組の件数と実際の一致数が違えば失敗する
//! （増えたら新しい箇所に、減ったら古い許可に気付ける）。
//!
//! 検査項目・守る原則・embedding-api §11 の規則の対応表（NHA-AT-04）。原則はマニフェストの番号。
//!
//! | 検査項目（パターンの key） | 守る原則 | embedding-api §11 の規則 |
//! |---|---|---|
//! | `panic`（`panic!`。`std::panic!` を含む） | 6 | 規則 5（公式 host function/adapter は panic しない）。unwind は規則 1 の `catch_unwind` が最後の受け皿 |
//! | `todo`・`unimplemented`（`todo!`・`unimplemented!`） | 6 | 規則 5 |
//! | `unreachable`（`unreachable!`） | 6 | 規則 5。届けば無条件に panic するので、内部不変条件であることを許可の理由に書く |
//! | `panic_any`・`resume_unwind`（`std::panic::` の関数） | 6 | 規則 1・5（catch した panic の再送出を含む） |
//! | `process`（`process::exit`・`process::abort`、`use std::process;`） | 6 | 規則 5（process exit・abort）。lib の許可は 0 組（CLI adapter は走査から除く） |
//!
//! 限界: 行単位の文字列照合なので、許可した組のファイルの中で箇所が入れ替わっても件数が同じなら区別しない。
//! `assert!`・`debug_assert!` の条件つき panic、`.unwrap()`・`.expect()`（tests/no_host_unwrap_audit.rs が検査する）、
//! slice の添字・`RefCell::borrow_mut` の panic、整数の overflow、OOM・stack overflow の abort、
//! 依存 crate の中の panic、`macro_rules!` の展開先・別名を通した呼びは検出しない。unwind panic は §11 規則 1 の
//! `catch_host_unwind` が最後の受け皿で、本検査はその前の段である。これらはレビューで見る。

#[path = "support/source_scan.rs"]
mod source_scan;
use source_scan::{Pattern, Violation, collect_rs};
use std::collections::BTreeMap;
use std::path::Path;

const PATTERNS: &[Pattern] = &[
    Pattern {
        key: "panic",
        texts: &["panic!"],
        use_paths: &[],
        use_modules: &[],
    },
    Pattern {
        key: "todo",
        texts: &["todo!"],
        use_paths: &[],
        use_modules: &[],
    },
    Pattern {
        key: "unimplemented",
        texts: &["unimplemented!"],
        use_paths: &[],
        use_modules: &[],
    },
    Pattern {
        key: "unreachable",
        texts: &["unreachable!"],
        use_paths: &[],
        use_modules: &[],
    },
    Pattern {
        key: "panic_any",
        texts: &["panic_any"],
        use_paths: &["std::panic::panic_any"],
        use_modules: &[],
    },
    Pattern {
        key: "resume_unwind",
        texts: &["resume_unwind"],
        use_paths: &["std::panic::resume_unwind"],
        use_modules: &[],
    },
    Pattern {
        key: "process",
        texts: &["process::exit", "process::abort"],
        use_paths: &["std::process::exit", "std::process::abort"],
        use_modules: &["std::process"],
    },
];

/// 許可リストの 1 組。ファイル（`src/` からの相対 path）×パターンの key×件数に、理由と原則を持たせる。
struct Allow {
    file: &'static str,
    key: &'static str,
    count: usize,
    reason: &'static str,
    principle: &'static str,
}

/// lib の `process` の許可は 0 組（CLI adapter の `main.rs` は走査から除く）。
/// `unreachable!` の各箇所は、直前の match・検査・型で除いた分岐に置かれた内部不変条件で、
/// script の入力・host から渡る値では届かないことを実装時に読んで確かめた（NHA 実装範囲 6）。
const ALLOWLIST: &[Allow] = &[
    Allow {
        file: "embedding.rs",
        key: "panic",
        count: 1,
        reason: "`audit_budget_for_test`（`#[doc(hidden)]` のホスト専用 builder）が、ホストの渡した不正な AuditBudget を拒む。script の入力では届かない",
        principle: "6",
    },
    Allow {
        file: "stable_error.rs",
        key: "panic",
        count: 1,
        reason: "`message_ids!` の const 初期化式の中で build 時に評価される（実行時には起きない）",
        principle: "6",
    },
    Allow {
        file: "builtin.rs",
        key: "unreachable",
        count: 2,
        reason: "push・pop の対象が識別子であることは `validate_context_builtin_call` が評価前に検査し、違えば catch 可能なエラーを返す（push_target_cell・pop の else 分岐）",
        principle: "6",
    },
    Allow {
        file: "compiler.rs",
        key: "unreachable",
        count: 1,
        reason: "二項演算の `And`・`Or` は直前の match arm が短絡評価として処理するので、opcode への写像には来ない",
        principle: "6",
    },
    Allow {
        file: "eval.rs",
        key: "unreachable",
        count: 18,
        reason: "frame・LoopSite・CallDest の種別と Stmt の種別の対応（作る側が保つ内部不変条件）、can_yield=false の駆動が yield しないこと、`And`・`Or` が eval_expr で短絡評価され比較の arm に来ないこと、大小比較の arm の演算子が 4 種に限られること。script の入力では値が変わらない",
        principle: "6",
    },
    Allow {
        file: "parser.rs",
        key: "unreachable",
        count: 8,
        reason: "直前に peek・先読みで確かめた token の種類を、advance したあとに同じ種類として読む（parse_assign・parse_index_assign は is_assign_stmt・is_index_assign_stmt が通した形だけ、parse_primary は peek した variant だけ）。lexer の出力が変わらない限り届かない",
        principle: "6",
    },
];

/// 走査の対象にする path か（`src/` からの相対 path、`/` 区切り）。CLI adapter（`main.rs`）と
/// 開発用ツール（`bin/`）は除く。
fn is_scanned(rel: &str) -> bool {
    rel.ends_with(".rs") && rel != "main.rs" && !rel.starts_with("bin/")
}

fn scan_source(rel: &str, content: &str) -> Vec<Violation> {
    source_scan::scan_source(rel, content, PATTERNS)
}

/// 違反と許可リストを突き合わせて、許可されていない違反と、件数の合わない許可の組を文章にして返す。
fn audit(found: &[Violation], allow: &[Allow]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut counts: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for v in found {
        *counts.entry((v.file.as_str(), v.key)).or_default() += 1;
    }
    for v in found {
        if !allow.iter().any(|a| a.file == v.file && a.key == v.key) {
            problems.push(format!("{}:{} [{}]: {}", v.file, v.line, v.key, v.text));
        }
    }
    for a in allow {
        let n = counts.get(&(a.file, a.key)).copied().unwrap_or(0);
        if n != a.count {
            problems.push(format!(
                "許可リストの組 {}×{} は {} 件のはずが {} 件ある（増えたら新しい箇所を確かめて許可し、減ったら古い許可を直す）",
                a.file, a.key, a.count, n
            ));
        }
    }
    problems
}

#[test]
fn lib_has_no_unconditional_host_abort_outside_allowlist() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
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
        if !is_scanned(&rel) {
            continue;
        }
        let content = std::fs::read_to_string(path).expect("read file");
        found.extend(scan_source(&rel, &content));
    }
    let problems = audit(&found, ALLOWLIST);
    assert!(
        problems.is_empty(),
        "lib の production コードの panic・abort・exit が許可リストと合いません:\n{}",
        problems.join("\n")
    );
}

fn lines_of(rel: &str, src: &str, key: &str) -> Vec<usize> {
    scan_source(rel, src)
        .iter()
        .filter(|v| v.key == key)
        .map(|v| v.line)
        .collect()
}

fn keys(src: &str) -> Vec<&'static str> {
    let mut k: Vec<&'static str> = scan_source("a.rs", src).iter().map(|v| v.key).collect();
    k.sort();
    k.dedup();
    k
}

#[test]
fn scan_scope_and_cfg_test_exclusion() {
    // NHA-AT-01: 走査の対象 path。
    assert!(is_scanned("eval.rs"));
    assert!(is_scanned("sub/dir/x.rs"), "サブディレクトリは走査する");
    assert!(!is_scanned("main.rs"), "CLI adapter は除く");
    assert!(!is_scanned("bin/gen_builtins_doc.rs"), "開発用ツールは除く");
    assert!(is_scanned("sub/main.rs"), "除くのは src/main.rs だけ");

    // `#[cfg(test)]` の付いた `if` 文の中は除かれ、直後の文は検出される。
    let src = "\
fn f() {
    #[cfg(test)]
    if x {
        panic!(\"test only\");
    }
    panic!(\"prod\");
}
";
    assert_eq!(lines_of("a.rs", src, "panic"), vec![6]);

    // `#[cfg(test)] mod`（brace つき・`mod x;`）は終わりまで除かれ、その後ろは走査される。
    let src = "\
#[cfg(test)]
mod tests {
    fn t() { panic!(\"t\"); }
}
#[cfg(test)]
mod more;
fn after() { panic!(\"p\"); }
";
    assert_eq!(lines_of("a.rs", src, "panic"), vec![7]);

    // `#[cfg(test)]` の付いた関数・impl の中は除かれる。
    let src = "\
#[cfg(test)]
fn helper() {
    todo!();
}
#[cfg(test)]
impl S {
    fn g() { unimplemented!(); }
}
fn prod() { todo!(); }
";
    assert_eq!(lines_of("a.rs", src, "todo"), vec![9]);
    assert!(lines_of("a.rs", src, "unimplemented").is_empty());

    // 構造体のフィールド・関数呼びの引数（行が `,` で終わる）は、その 1 行だけが除かれ、直後の行は検出される。
    let src = "\
struct S {
    #[cfg(test)]
    probe: u32,
    a: u32,
}
fn g() { panic!(\"after field\"); }
fn f() {
    g(
        #[cfg(test)]
        None,
        panic!(\"arg\"),
    );
}
const S0: S = S {
    #[cfg(test)]
    probe: 0,
    a: panic!(\"lit\"),
};
";
    // フィールドの後ろの `fn g`（6 行目）、関数呼びの引数（11 行目）、struct literal のフィールド `a:`（17 行目）。
    // フィールドの規則が壊れて brace まで除く読み方になれば 6 行目を取りこぼす。
    assert_eq!(lines_of("a.rs", src, "panic"), vec![6, 11, 17]);

    // `#[cfg(test)] x: T,` のように attribute と item が同じ行にある引数は、その行だけが除かれ、
    // 直後の行（`) -> T {`）と関数本体は検出される。
    let src = "\
fn run(
    a: u8,
    #[cfg(test)] trace: Option<&mut Vec<u8>>,
) -> u8 {
    panic!(\"body\")
}
";
    assert_eq!(lines_of("a.rs", src, "panic"), vec![5]);

    // attribute と item が同じ行で brace のある item は、閉じるまで除かれる。
    let src = "\
#[cfg(test)] fn t() {
    panic!(\"t\");
}
fn p() { panic!(\"p\"); }
";
    assert_eq!(lines_of("a.rs", src, "panic"), vec![4]);
}

#[test]
fn scan_detects_each_pattern_and_use_imports() {
    // NHA-AT-02: パターンごとの検出と `use` 経由の取り込み。
    let cases: &[(&str, &str)] = &[
        ("fn f() { panic!(\"x\"); }", "panic"),
        ("fn f() { std::panic!(\"x\"); }", "panic"),
        ("fn f() { todo!(); }", "todo"),
        ("fn f() { unimplemented!(); }", "unimplemented"),
        ("fn f() { unreachable!(); }", "unreachable"),
        ("fn f() { std::panic::panic_any(1); }", "panic_any"),
        ("fn f() { std::panic::resume_unwind(b); }", "resume_unwind"),
        ("fn f() { std::process::exit(1); }", "process"),
        ("fn f() { std::process::abort(); }", "process"),
        ("use std::process;\nfn f() { process::exit(1); }", "process"),
        ("use std::process::exit;", "process"),
        ("use std::process::abort as a;", "process"),
        ("use std::process::*;", "process"),
        ("use std::{process, io};", "process"),
        ("use std::panic::panic_any;", "panic_any"),
        (
            "use std::panic::{resume_unwind, catch_unwind};",
            "resume_unwind",
        ),
        ("use std::panic::*;", "panic_any"),
    ];
    for (src, key) in cases {
        assert!(keys(src).contains(key), "src={src} keys={:?}", keys(src));
    }
    // `use std::process;` のあとの `process::exit(1)` は use 行と呼びの両方で検出される。
    assert_eq!(
        lines_of(
            "a.rs",
            "use std::process;\nfn f() { process::exit(1); }",
            "process"
        ),
        vec![1, 2]
    );
    // 検出しないもの: assert 系・unwrap・expect・文字列・コメントの中・識別子の途中・ExitCode・catch_unwind。
    for src in [
        "fn f() { assert!(x); assert_eq!(a, b); assert_ne!(a, b); }",
        "fn f() { debug_assert!(x); debug_assert_eq!(a, b); }",
        "fn f() { x.unwrap(); y.expect(\"m\"); }",
        "fn f() { let _ = \"panic!(x) unreachable!() process::exit\"; }",
        "// panic!(\"c\") todo!() std::process::exit(1)\nfn f() {}",
        "fn f() { my_panic!(); no_todo!(); }",
        "fn f() { let _ = std::panic::catch_unwind(|| ()); }",
        "use std::process::ExitCode;",
        "use std::panic::catch_unwind;",
    ] {
        assert!(keys(src).is_empty(), "src={src} keys={:?}", keys(src));
    }
}

#[test]
fn allowlist_entries_have_reason_and_principle() {
    // NHA-AT-03: 各組に理由と原則があり、key は実在するパターンで、組は重複せず、件数は 1 以上。
    // lib の `process` の許可は 0 組。
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
        assert!(a.count >= 1, "{}×{}: 件数は 1 以上", a.file, a.key);
        assert!(
            PATTERNS.iter().any(|p| p.key == a.key),
            "{}×{}: 未知のパターン key",
            a.file,
            a.key
        );
        assert!(is_scanned(a.file), "{}×{}: 走査しない path", a.file, a.key);
        assert!(
            !ALLOWLIST[..i]
                .iter()
                .any(|b| b.file == a.file && b.key == a.key),
            "{}×{}: 許可の組が重複している",
            a.file,
            a.key
        );
    }
    assert!(
        ALLOWLIST.iter().all(|a| a.key != "process"),
        "lib の process::exit・abort の許可は 0 組"
    );
}

#[test]
fn audit_fails_on_count_mismatch_or_unlisted() {
    // NHA-AT-03: 許可の無い違反と、件数の多い・少ない組の両方が検査を失敗させる。
    let two = "fn f() { panic!(\"a\"); }\nfn g() { panic!(\"b\"); }\n";
    let v = scan_source("x.rs", two);
    assert_eq!(v.len(), 2);
    let allow = |count| {
        [Allow {
            file: "x.rs",
            key: "panic",
            count,
            reason: "r",
            principle: "6",
        }]
    };
    assert!(audit(&v, &allow(2)).is_empty(), "件数が一致すれば通る");
    let few = audit(&v, &allow(1));
    assert_eq!(few.len(), 1, "実際が多い: {few:?}");
    assert!(few[0].contains("x.rs×panic") && few[0].contains("2 件"));
    let many = audit(&v, &allow(3));
    assert_eq!(many.len(), 1, "実際が少ない: {many:?}");
    let none: &[Allow] = &[];
    assert_eq!(
        audit(&v, none).len(),
        2,
        "許可の無い違反は箇所ごとに報告する"
    );
    // 同じファイルの別パターンは許可の組に含まれない。
    let v2 = scan_source("x.rs", "fn f() { panic!(\"a\"); todo!(); }\n");
    assert_eq!(audit(&v2, &allow(1)).len(), 1);
    // 全く一致しない組（件数 0）は古い許可として失敗する。
    assert_eq!(audit(&[], &allow(1)).len(), 1);
}

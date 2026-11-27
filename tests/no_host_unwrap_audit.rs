//! 原則 6 の静的検査: lib の production コードの `.unwrap()`・`.expect()` が、理由付きの許可リスト以外に
//! 無いことを固定する（loop-guard §4.1 の 1 段目。manifesto-check-no-host-unwrap、docs/embedding-api.md の同名小節）。
//!
//! `.unwrap()`・`.expect()` は値が `None`・`Err` なら無条件に panic するので、embedding-api §11 規則 5 の対象である。
//! 走査の範囲と production コードの切り出しは tests/no_host_abort_audit.rs と同じで、tests/support/source_scan.rs の
//! `collect_rs`・`production_lines` を共有する（`src/` 配下の全 `*.rs` から CLI adapter の `src/main.rs` と開発用ツールの
//! `src/bin/` を除き、文字列・コメントと `#[cfg(test)]` の付いた item を除く）。
//! 許可リストは「ファイル・パターン・件数」の組で、組の件数と実際の一致数（行の数。1 行に 2 つあっても 1 と数える）が
//! 違えば失敗する（増えたら新しい箇所に、減ったら古い許可に気付ける）。
//!
//! 検査項目・守る原則・embedding-api §11 の規則の対応表（NHU-AT-04）。原則はマニフェストの番号。
//!
//! | 検査項目（パターンの key） | 守る原則 | embedding-api §11 の規則 |
//! |---|---|---|
//! | `unwrap`（`.unwrap()`・`.unwrap_err()`、path 形の `::unwrap`・`::unwrap_err`） | 6 | 規則 5（公式 host function/adapter は panic しない）。unwind は規則 1 の `catch_unwind` が最後の受け皿 |
//! | `expect`（`.expect(`・`.expect_err(`、path 形の `::expect`・`::expect_err`） | 6 | 規則 5。`Mutex::lock()` の poison は、lock を持つ間に host callback を呼ばないことを許可の理由に書く（規則 1・5） |
//! | （照合から外す形）`src/parser.rs` の `self.expect(`・`fn expect(` | 6 | 規則 5。parser の自前のメソッドで `Err` を返し panic しない（Q2=A）。外すのはこのファイルのこの 2 形だけ |
//!
//! 検出しないもの: `unwrap_or`・`unwrap_or_else`・`unwrap_or_default`（panic しない）。`unwrap_unchecked` は §11 規則 6 の
//! `forbid(unsafe_code)` が止めるので扱わない。
//!
//! 限界: 行単位の文字列照合なので、許可した組のファイルの中で箇所が入れ替わっても件数が同じなら区別しない。
//! `assert!` 系、slice の添字・`RefCell::borrow_mut` の panic、整数の overflow、OOM・stack overflow の abort、
//! 依存 crate の中の panic、`macro_rules!` の展開先・別名・trait を通した呼び（`Option::unwrap` を別名で取り込む
//! `use` など）は検出しない。unwind panic は §11 規則 1 の `catch_host_unwind` が最後の受け皿で、本検査はその前の段である。
//! これらはレビューで見る。

#[path = "support/source_scan.rs"]
mod source_scan;
use source_scan::{Violation, collect_rs, is_ident, line_has_call, production_lines};
use std::collections::BTreeMap;
use std::path::Path;

/// 検出パターン 1 つ分（key と、`line_has_call` で照合する文字列）。
struct CallPattern {
    key: &'static str,
    texts: &'static [&'static str],
}

const PATTERNS: &[CallPattern] = &[
    CallPattern {
        key: "unwrap",
        texts: &[".unwrap()", ".unwrap_err()", "::unwrap", "::unwrap_err"],
    },
    CallPattern {
        key: "expect",
        texts: &[".expect(", ".expect_err(", "::expect", "::expect_err"],
    },
];

/// 照合から外す形（Q2=A）。ファイル（`src/` からの相対 path）と、そのファイルの中で外す文字列と理由。
/// 外す文字列は左が識別子の途中でないときだけ外す（`myself.expect(` は外さない）。
struct Exempt {
    file: &'static str,
    texts: &'static [&'static str],
    reason: &'static str,
}

const EXEMPT: &[Exempt] = &[Exempt {
    file: "parser.rs",
    texts: &["self.expect(", "fn expect("],
    reason: "parser の自前のメソッド `fn expect(&mut self, expected: Token) -> Result<(), TsumugiError>`（impl Parser に 1 つ）とその呼び。token が合わなければ Err を返し、panic しない",
}];

/// 許可リストの 1 組。ファイル（`src/` からの相対 path）×パターンの key×件数に、理由と原則を持たせる。
struct Allow {
    file: &'static str,
    key: &'static str,
    count: usize,
    reason: &'static str,
    principle: &'static str,
}

/// 各組は、各箇所の到達条件を読み、script の入力（source・host function・resolver・sink から渡る値）では
/// `None`・`Err` に届かないことを実装時に確かめた（NHU 実装範囲 4）。`Mutex::lock()` の組は、lock を持つ間に
/// host callback（audit sink の submit・host function・resolver・waker の wake）を呼ばないことを確かめた
/// （実装範囲 5。waker は lock の中で clone して退避し、解放後に wake する）。
const ALLOWLIST: &[Allow] = &[
    Allow {
        file: "audit.rs",
        key: "expect",
        count: 19,
        reason: "`Mutex::lock()` の poison（AuditTicket・AuditCompleter の ticket 状態 5 行、InMemoryAuditSink・ScriptedAuditSink の記録と応答計画 13 行）。lock の中は状態の読み書き・waker の clone・envelope の複製だけで host callback を呼ばず、wake は解放後。残り 1 行は定数 `NonZeroU128::new(1)`",
        principle: "6",
    },
    Allow {
        file: "budget.rs",
        key: "expect",
        count: 1,
        reason: "`CancellationToken::cancel` の waker の `Mutex::lock()` の poison。lock の中は waker の clone だけで、wake は解放後",
        principle: "6",
    },
    Allow {
        file: "builtin.rs",
        key: "expect",
        count: 1,
        reason: "`filter` の要素: `collect_callback_value` の `filter` の呼び出し側（同期経路の builtin.rs と式 frame の eval.rs）は、必ず `Some(item)` を渡す",
        principle: "6",
    },
    Allow {
        file: "capability.rs",
        key: "expect",
        count: 7,
        reason: "CapabilitySet の正準 encode: groups は直前に同じ builder の `is_some()` の分だけ積むので、各 group の `as_ref().expect` は Some",
        principle: "6",
    },
    Allow {
        file: "cli_capability.rs",
        key: "expect",
        count: 13,
        reason: "CLI の safe・legacy profile の builder: 新しい builder に各 capability を 1 回ずつ grant する。grant の Err は重複（`set_once`）だけなので届かない",
        principle: "6",
    },
    Allow {
        file: "compiler.rs",
        key: "unwrap",
        count: 6,
        reason: "`self.loops` の stack の不変条件: pop・last_mut は同じ関数（compile_while・compile_for）で push した分か、冒頭の let-else でループ外の break・continue を受けたあと（compile_break・compile_continue）。compile の Err では REPL の checkpoint へ戻す",
        principle: "6",
    },
    Allow {
        file: "embedding.rs",
        key: "expect",
        count: 3,
        reason: "直前の検査（HostErrorCode::new の空文字の検査のあとの先頭文字）と定数（64 KiB の NonZeroUsize、link 層の固定 host error code は形式を満たす小文字の literal だけ）",
        principle: "6",
    },
    Allow {
        file: "env.rs",
        key: "expect",
        count: 2,
        reason: "rollback journal: 同じ関数の冒頭で `self.journal.is_none()` なら戻り、その後に journal を外す処理は無い",
        principle: "6",
    },
    Allow {
        file: "eval.rs",
        key: "expect",
        count: 31,
        reason: "tree 評価器の frame・operand stack の不変条件（直前の `len > stop_depth` の検査、作る側が積んだ式 frame・driver の Call frame・callback 状態・pending key・push の代入先）、builtin の arity は評価前に `validate_context_builtin_call`・`check_arity_count` が検査、直前の matches! の検査、数値の組の compare_relational。`run_slice` は begin_execution 済みの session を前提にする埋め込みの契約（ホストの呼び順）で、script の入力では届かない",
        principle: "6",
    },
    Allow {
        file: "eval.rs",
        key: "unwrap",
        count: 1,
        reason: "文字列の添字: 直前の範囲検査で 0 <= actual < 文字数を確かめたあとの `chars().nth(actual)`",
        principle: "6",
    },
    Allow {
        file: "host_pending.rs",
        key: "expect",
        count: 7,
        reason: "`Mutex::lock()` の poison（HostCallTicket・HostCallCompleter の ticket 状態、AdapterExecutor の outstanding）。lock の中は状態の読み書き・waker の clone だけで、host の job（`f`）は lock の外の thread で実行し、wake は解放後",
        principle: "6",
    },
    Allow {
        file: "scheduler.rs",
        key: "expect",
        count: 10,
        reason: "`Mutex::lock()` の poison（SchedulerShared の admission・run-turn の状態 8 行。lock の中は数と queue の操作だけで host callback を呼ばない）、直前の `front() == Some` を確かめたあとの pop_front、定数 `NonZeroU64::new(10_000)`",
        principle: "6",
    },
    Allow {
        file: "value.rs",
        key: "expect",
        count: 8,
        reason: "`Rc::get_mut` の一意性: 直前に strong==1 && weak==0 を確かめたもの、新しく作った Rc、detach_list・detach_dict（一意にする複製）のあと",
        principle: "6",
    },
];

/// 走査の対象にする path か（`src/` からの相対 path、`/` 区切り）。CLI adapter（`main.rs`）と
/// 開発用ツール（`bin/`）は除く（tests/no_host_abort_audit.rs と同じ）。
fn is_scanned(rel: &str) -> bool {
    rel.ends_with(".rs") && rel != "main.rs" && !rel.starts_with("bin/")
}

/// `line` から、左が識別子の途中でない `text` をすべて空白に置き換える。
fn remove_exempt(line: &str, text: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(off) = rest.find(text) {
        let left_ok = rest[..off]
            .chars()
            .next_back()
            .or_else(|| out.chars().next_back())
            .is_none_or(|c| !is_ident(c));
        out.push_str(&rest[..off]);
        if left_ok {
            out.push(' ');
        } else {
            out.push_str(text);
        }
        rest = &rest[off + text.len()..];
    }
    out.push_str(rest);
    out
}

/// 1 つのファイル（`rel` は `src/` からの相対 path）の production コードを走査する。
fn scan_source(rel: &str, content: &str) -> Vec<Violation> {
    let mut found = Vec::new();
    for (lineno, code) in production_lines(content) {
        let mut code = code;
        for ex in EXEMPT.iter().filter(|e| e.file == rel) {
            for t in ex.texts {
                code = remove_exempt(&code, t);
            }
        }
        for pat in PATTERNS {
            if pat.texts.iter().any(|t| line_has_call(&code, t)) {
                found.push(Violation {
                    file: rel.to_string(),
                    key: pat.key,
                    line: lineno,
                    text: code.trim().to_string(),
                });
            }
        }
    }
    found
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
fn lib_has_no_unwrap_or_expect_outside_allowlist() {
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
        "lib の production コードの `.unwrap()`・`.expect()` が許可リストと合いません:\n{}",
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

fn keys(rel: &str, src: &str) -> Vec<&'static str> {
    let mut k: Vec<&'static str> = scan_source(rel, src).iter().map(|v| v.key).collect();
    k.sort();
    k.dedup();
    k
}

#[test]
fn scan_detects_each_pattern_and_skips_non_panicking_forms() {
    // NHU-AT-01: 実装範囲 2 のパターンがそれぞれ検出される。
    let cases: &[(&str, &str)] = &[
        ("fn f() { x.unwrap(); }", "unwrap"),
        ("fn f() { x.unwrap_err(); }", "unwrap"),
        ("fn f() { xs.iter().map(Option::unwrap); }", "unwrap"),
        (
            "fn f() { xs.into_iter().map(Result::unwrap_err); }",
            "unwrap",
        ),
        ("fn f() { x.expect(\"m\"); }", "expect"),
        ("fn f() { x.expect_err(\"m\"); }", "expect"),
        ("fn f() { xs.iter().map(Option::expect); }", "expect"),
        ("fn f() { r.map(Result::expect_err); }", "expect"),
        // 識別子の直後の `.`（`line_has` では一致しない形）。
        ("fn f() { self.items.pop().unwrap(); }", "unwrap"),
    ];
    for (src, key) in cases {
        assert!(
            keys("a.rs", src).contains(key),
            "src={src} keys={:?}",
            keys("a.rs", src)
        );
    }
    // 前の行から続く、行頭の `.expect("")`・`.unwrap()`。
    let src = "\
fn f() {
    let v = m
        .lock()
        .expect(\"poisoned\");
    let w = it
        .next()
        .unwrap();
}
";
    assert_eq!(lines_of("a.rs", src, "expect"), vec![4]);
    assert_eq!(lines_of("a.rs", src, "unwrap"), vec![7]);
    // 1 行に 2 つあっても 1 と数える。
    assert_eq!(
        lines_of("a.rs", "fn f() { a.unwrap(); b.unwrap(); }", "unwrap"),
        vec![1]
    );

    // 検出しないもの: panic しない unwrap_*、文字列・コメントの中、`#[cfg(test)]` の付いた item の中、
    // 似た名前のメソッド・識別子。
    for src in [
        "fn f() { x.unwrap_or(0); y.unwrap_or_else(|| 0); z.unwrap_or_default(); }",
        "fn f() { xs.iter().map(Option::unwrap_or_default); }",
        "fn f() { let _ = \"x.unwrap() y.expect(m) Option::unwrap\"; }",
        "// x.unwrap() y.expect(\"m\")\nfn f() {}",
        "fn f() { p.expect_newline(); q.expects(1); let unwrap = 1; let expect = unwrap; }",
        "#[cfg(test)]\nmod tests {\n    fn t() { x.unwrap(); y.expect(\"m\"); }\n}",
        "#[cfg(test)]\nfn helper() {\n    x.unwrap();\n}",
        "fn f() {\n    #[cfg(test)]\n    let _ = x.expect(\"m\");\n}",
    ] {
        assert!(
            keys("a.rs", src).is_empty(),
            "src={src} keys={:?}",
            keys("a.rs", src)
        );
    }
    // `#[cfg(test)]` の item の後ろは検出される。
    let src = "#[cfg(test)]\nfn t() { x.unwrap(); }\nfn p() { x.unwrap(); }\n";
    assert_eq!(lines_of("a.rs", src, "unwrap"), vec![3]);
}

#[test]
fn parser_own_expect_method_is_exempt_only_in_parser_rs() {
    // NHU-AT-02: parser.rs の `self.expect(`・`fn expect(` は検出されない。
    let src = "\
impl Parser {
    fn expect(&mut self, expected: Token) -> Result<(), TsumugiError> {
        Ok(())
    }
    fn p(&mut self) -> Result<(), TsumugiError> {
        self.expect(Token::LParen)?;
        let t = self.tokens.get(0).expect(\"token\");
        self.expect(Token::RParen)?; let u = o.expect(\"x\");
        myself.expect(1);
        Ok(())
    }
}
";
    // 7 行目（Option の `.expect(`）、8 行目（外す形と同じ行の `.expect(`）、9 行目（`myself` は `self` ではない）だけ。
    assert_eq!(lines_of("parser.rs", src, "expect"), vec![7, 8, 9]);
    // ほかのファイルでは `self.expect(` の行も検出する（`fn expect(` は呼びの形 `.expect(` を含まないので、
    // どのファイルでも検出しない。parser.rs で外す形に挙げるのは、Q2=A の 2 形を定数に残すため）。
    assert_eq!(lines_of("eval.rs", src, "expect"), vec![6, 7, 8, 9]);
    assert_eq!(
        lines_of("sub/parser.rs", "fn f() { self.expect(1); }", "expect"),
        vec![1],
        "外すのは src/parser.rs だけ"
    );
    // 外す形は parser.rs の 2 つだけで、理由がある。
    assert_eq!(EXEMPT.len(), 1);
    assert_eq!(EXEMPT[0].file, "parser.rs");
    assert_eq!(EXEMPT[0].texts, &["self.expect(", "fn expect("]);
    assert!(!EXEMPT[0].reason.trim().is_empty());
}

#[test]
fn allowlist_entries_have_reason_and_principle() {
    // NHU-AT-03: 各組に理由と原則があり、key は実在するパターンで、組は重複せず、件数は 1 以上。
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
    // 走査の対象 path（tests/no_host_abort_audit.rs と同じ）。
    assert!(is_scanned("eval.rs"));
    assert!(is_scanned("sub/dir/x.rs"));
    assert!(!is_scanned("main.rs"));
    assert!(!is_scanned("bin/gen_builtins_doc.rs"));
}

#[test]
fn audit_fails_on_count_mismatch_or_unlisted() {
    // NHU-AT-03: 許可の無い違反と、件数の多い・少ない組の両方が検査を失敗させる。
    let two = "fn f() { a.unwrap(); }\nfn g() { b.unwrap(); }\n";
    let v = scan_source("x.rs", two);
    assert_eq!(v.len(), 2);
    let allow = |count| {
        [Allow {
            file: "x.rs",
            key: "unwrap",
            count,
            reason: "r",
            principle: "6",
        }]
    };
    assert!(audit(&v, &allow(2)).is_empty(), "件数が一致すれば通る");
    let few = audit(&v, &allow(1));
    assert_eq!(few.len(), 1, "実際が多い: {few:?}");
    assert!(few[0].contains("x.rs×unwrap") && few[0].contains("2 件"));
    let many = audit(&v, &allow(3));
    assert_eq!(many.len(), 1, "実際が少ない: {many:?}");
    let none: &[Allow] = &[];
    assert_eq!(
        audit(&v, none).len(),
        2,
        "許可の無い違反は箇所ごとに報告する"
    );
    // 同じファイルの別パターンは許可の組に含まれない。
    let v2 = scan_source("x.rs", "fn f() { a.unwrap(); b.expect(\"m\"); }\n");
    assert_eq!(audit(&v2, &allow(1)).len(), 1);
    // 全く一致しない組（件数 0）は古い許可として失敗する。
    assert_eq!(audit(&[], &allow(1)).len(), 1);
}

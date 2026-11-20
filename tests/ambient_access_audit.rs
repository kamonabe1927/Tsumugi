//! CAP-AT-27: core から ambient OS access が除去されたことを静的 + 動的に固定する（C10）。
//!
//! 静的: `src/` 配下（サブディレクトリを含む）の全 `*.rs` の production コードに、ambient な
//! OS・network・process・clock・thread の直接呼びが、理由付きの許可リスト（ファイル×パターンの組）
//! 以外に無いことを固定する（loop-guard §4.1 の 1 段目。manifesto-check-ambient-surface、
//! docs/capability-model.md の同名小節）。production コードは `#[cfg(test)]` の直後が `mod` の
//! item（その module の終わりまで）を除いた部分で、フィールド・文・関数に付いた `#[cfg(test)]` では
//! 打ち切らない。許可リストの組が 1 度も一致しなければ検査は失敗する（古い許可を残さない）。
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

fn tsumugi_bin() -> &'static str {
    env!("CARGO_BIN_EXE_tsumugi")
}

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// 検出パターン 1 つ分。
struct Pattern {
    /// 許可リストと報告で使う名前。
    key: &'static str,
    /// 行の中の文字列照合（左は識別子の途中でないこと。末尾が `*` か `::` なら前方一致、無ければ右も識別子の途中でないこと）。
    texts: &'static [&'static str],
    /// `use` 文で取り込まれた path がこれと等しいか、これの下にあれば違反。
    use_paths: &'static [&'static str],
    /// `use` 文で取り込まれた path がこれと厳密に等しいとき（module の取り込み）だけ違反。
    use_modules: &'static [&'static str],
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
    // import 解決（C6）。runtime fs ではない（§6.6）。C6/E7 で撤去予定。
    Allow {
        file: "module.rs",
        key: "fs",
        reason: "import 解決（C6）。runtime fs ではない（§6.6）",
        principle: "2・8",
    },
    Allow {
        file: "module.rs",
        key: "env",
        reason: "import 解決の基準 directory（current_dir）。C6/E7 で撤去予定",
        principle: "2・8",
    },
    Allow {
        file: "sandbox.rs",
        key: "env",
        reason: "legacy の TSUMUGI_SANDBOX の読み取り。C6/E7 で撤去予定",
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

/// 違反 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Violation {
    file: String,
    key: &'static str,
    line: usize,
    text: String,
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `line` の中に `pat` が（左右の境界つきで）あるか。
fn line_has(line: &str, pat: &str) -> bool {
    let (pat, prefix) = match pat.strip_suffix('*') {
        Some(p) => (p, true),
        None => (pat, false),
    };
    let mut from = 0;
    while let Some(off) = line[from..].find(pat) {
        let start = from + off;
        let end = start + pat.len();
        let left_ok = line[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_ident(c));
        let right_ok =
            prefix || pat.ends_with(':') || line[end..].chars().next().is_none_or(|c| !is_ident(c));
        if left_ok && right_ok {
            return true;
        }
        from = start + pat.len().max(1);
    }
    false
}

/// 行から文字列・文字リテラルと行末コメントを除く（検出と brace の数え上げ用の粗い処理）。
fn strip_literals_and_comment(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            break;
        }
        if c == '"' {
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            out.push_str("\"\"");
            continue;
        }
        if c == '\'' {
            // 文字リテラル（`'{'`・`'\n'`）だけを飛ばす。lifetime（`'a`）はそのまま残す。
            if chars.get(i + 1) == Some(&'\\') && chars.get(i + 3) == Some(&'\'') {
                i += 4;
                continue;
            }
            if chars.get(i + 2) == Some(&'\'') && chars.get(i + 1) != Some(&'\'') {
                i += 3;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn brace_delta(code: &str) -> i64 {
    code.chars()
        .map(|c| match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        })
        .sum()
}

/// production コードの（行番号, 文字列・コメントを除いた行）。`#[cfg(test)]` の直後の item が
/// `mod` のものだけを、その module の終わりまで除く。
fn production_lines(content: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if trimmed.starts_with("#[cfg(test)]") {
            // 後続の attribute・空行・コメントを飛ばして、次の item の頭を見る。
            let mut j = i + 1;
            while j < lines.len() {
                let t = lines[j].trim_start();
                if t.is_empty() || t.starts_with("#[") || t.starts_with("//") {
                    j += 1;
                } else {
                    break;
                }
            }
            let item = lines.get(j).map(|l| l.trim_start()).unwrap_or("");
            let item = item
                .strip_prefix("pub(crate) ")
                .or_else(|| item.strip_prefix("pub "))
                .unwrap_or(item);
            if item.starts_with("mod ") {
                // `mod x;` は 1 行、`mod x { ... }` は brace が閉じるまで。
                let mut depth: i64 = 0;
                let mut opened = false;
                let mut k = j;
                while k < lines.len() {
                    let code = strip_literals_and_comment(lines[k]);
                    depth += brace_delta(&code);
                    if code.contains('{') {
                        opened = true;
                    }
                    if (opened && depth <= 0) || (!opened && code.contains(';')) {
                        break;
                    }
                    k += 1;
                }
                i = k + 1;
                continue;
            }
            // フィールド・文・関数に付いた `#[cfg(test)]` では打ち切らない。
            i += 1;
            continue;
        }
        let code = strip_literals_and_comment(lines[i]);
        let t = code.trim_start();
        if !t.is_empty() && !t.starts_with('*') && !t.starts_with("/*") {
            out.push((i + 1, code));
        }
        i += 1;
    }
    out
}

/// `use` 文の本体（`use` と `;` を除いた部分）を path の一覧へ展開する。`as` の別名は捨て、
/// `self` はその親 path にする。
fn expand_use(body: &str) -> Vec<String> {
    fn parse(chars: &[char], pos: &mut usize, prefix: &str, out: &mut Vec<String>) {
        // 1 つの項目（`a::b::{...}` か `a::b as c` か `self`）を読む。
        let mut cur = prefix.to_string();
        let mut last_self = false;
        loop {
            while *pos < chars.len() && chars[*pos].is_whitespace() {
                *pos += 1;
            }
            if *pos >= chars.len() {
                break;
            }
            if chars[*pos] == '{' {
                *pos += 1;
                loop {
                    parse(chars, pos, &cur, out);
                    while *pos < chars.len() && chars[*pos].is_whitespace() {
                        *pos += 1;
                    }
                    match chars.get(*pos) {
                        Some(',') => *pos += 1,
                        Some('}') => {
                            *pos += 1;
                            break;
                        }
                        _ => break,
                    }
                }
                return;
            }
            if chars[*pos] == ',' || chars[*pos] == '}' {
                break;
            }
            let start = *pos;
            while *pos < chars.len() && (is_ident(chars[*pos]) || chars[*pos] == '*') {
                *pos += 1;
            }
            if *pos == start {
                *pos += 1;
                continue;
            }
            let word: String = chars[start..*pos].iter().collect();
            if word == "as" && !cur.is_empty() {
                // 別名を読み飛ばす。
                while *pos < chars.len() && chars[*pos].is_whitespace() {
                    *pos += 1;
                }
                while *pos < chars.len() && is_ident(chars[*pos]) {
                    *pos += 1;
                }
                continue;
            }
            last_self = word == "self";
            if !last_self {
                if !cur.is_empty() {
                    cur.push_str("::");
                }
                cur.push_str(&word);
            }
            while *pos < chars.len() && chars[*pos].is_whitespace() {
                *pos += 1;
            }
            if chars.get(*pos) == Some(&':') && chars.get(*pos + 1) == Some(&':') {
                *pos += 2;
            }
        }
        if !cur.is_empty() && (cur != prefix || last_self) {
            out.push(cur);
        }
    }
    let chars: Vec<char> = body.chars().collect();
    let mut pos = 0;
    let mut out = Vec::new();
    parse(&chars, &mut pos, "", &mut out);
    out
}

fn path_is_or_under(path: &str, base: &str) -> bool {
    path == base || path.starts_with(&format!("{base}::"))
}

/// 1 つのファイル（`rel` は `src/` からの相対 path、`/` 区切り）の production コードを走査する。
fn scan_source(rel: &str, content: &str) -> Vec<Violation> {
    let mut found: Vec<Violation> = Vec::new();
    let mut push = |key: &'static str, line: usize, text: &str| {
        let v = Violation {
            file: rel.to_string(),
            key,
            line,
            text: text.trim().to_string(),
        };
        if !found.contains(&v) {
            found.push(v);
        }
    };
    let mut pending_use: Option<(usize, String)> = None;
    for (lineno, code) in production_lines(content) {
        for pat in PATTERNS {
            if pat.texts.iter().any(|t| line_has(&code, t)) {
                push(pat.key, lineno, &code);
            }
        }
        // `use` 文（複数行・波括弧つき）の取り込み。
        let t = code.trim_start();
        let t = t
            .strip_prefix("pub(crate) ")
            .or_else(|| t.strip_prefix("pub "))
            .unwrap_or(t);
        if pending_use.is_none() && t.starts_with("use ") {
            pending_use = Some((lineno, String::new()));
        }
        if let Some((start, body)) = pending_use.as_mut() {
            let piece = if body.is_empty() {
                t.strip_prefix("use ").unwrap_or(t)
            } else {
                code.as_str()
            };
            body.push_str(piece);
            body.push(' ');
            if piece.contains(';') {
                let start = *start;
                let stmt = body.split(';').next().unwrap_or("").to_string();
                for path in expand_use(&stmt) {
                    for pat in PATTERNS {
                        let hit = pat.use_paths.iter().any(|b| path_is_or_under(&path, b))
                            || pat.use_modules.contains(&path.as_str());
                        if hit {
                            push(pat.key, start, &format!("use {}", stmt.trim()));
                        }
                    }
                }
                pending_use = None;
            }
        }
    }
    found
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

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
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
    ];
    for (src, key) in cases {
        assert_eq!(keys("a.rs", src), vec![*key], "src={src}");
    }
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

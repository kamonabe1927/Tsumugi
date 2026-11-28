//! 文書の整合を機械的に検査する契約テスト。
//!
//! 整合点検（loop-runbook §13）のうち、正本の規則から機械的に判定できる項目をここで検査する。
//! LLM による点検は、ここで検査しない項目（文章の意味・spec と実装の食い違いなど）に絞る。
//!
//! 検査する項目:
//!
//! 1. `docs/slices.toml` のフィールド定義（ファイル冒頭のコメント）どおりに各スライスが書かれている
//!    - 必須フィールド・`status` の値・`id` の一意性
//!    - `ready` / `done` の `spec` が指す文書と見出しが実在する（見出しは完全一致）
//!    - `ready` の `acceptance` が空でなく、`design_pr` がある。`done` に `impl_pr` がある
//!    - `todo` / `ready` に `order` があり、`order` が重複しない。`blocked` に `blocked_reason` がある
//!    - `depends_on` の id が実在し、`ready` / `done` の依存がすべて `done` である
//!    - `parent` がロードマップ「ステータス一覧」表の ID 列にある
//! 2. リポジトリ内の Markdown の相対リンクが、実在するファイルを指している
//!
//! 失敗したときは、正本（documentation-guide「状態の正本」）でない側を直す。
//!
//! `slices.toml` の読み取りには依存 crate を足さず、TOML の部分集合
//! （`[[slice]]`、文字列・整数・配列・inline table、行末コメント）だけを読む小さなパーサ（`tests/support/toml_subset.rs`）を使う。
//! 部分集合の外の書き方を見つけたら、黙って読み飛ばさずに失敗させる。

#[path = "support/toml_subset.rs"]
mod toml_subset;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use toml_subset::{Value, headings, parse_tables};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} を読めません: {e}", path.display()))
}

// ── TOML の部分集合パーサ（本体は support/toml_subset.rs） ─────────

/// `[[slice]]` の並びを読む。各要素は (開始行, キー → 値)。
fn parse_slices(text: &str, file: &str) -> Vec<(usize, BTreeMap<String, Value>)> {
    parse_tables(text, file, &["slice"])
        .into_iter()
        .map(|(line, _, fields)| (line, fields))
        .collect()
}

// ── Markdown の補助 ───────────────────────────────────────────────

/// `## <title>` で始まる節の本文（次の同じ深さ以上の見出しの手前まで）。
fn section<'a>(markdown: &'a str, title: &str) -> Option<&'a str> {
    let marker = format!("\n## {title}");
    let start = markdown.find(&marker)? + 1;
    let body = &markdown[start..];
    let body_start = body.find('\n').map_or(body.len(), |n| n + 1);
    let rest = &body[body_start..];
    let end = rest.find("\n## ").map_or(rest.len(), |n| n + 1);
    Some(&rest[..end])
}

/// インラインコード（`...`）とフェンス付きコードブロックを取り除いた本文。
fn strip_code(markdown: &str) -> String {
    let mut out = String::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            out.push('\n');
            continue;
        }
        if in_fence {
            out.push('\n');
            continue;
        }
        let mut in_code = false;
        for c in line.chars() {
            if c == '`' {
                in_code = !in_code;
            } else if !in_code {
                out.push(c);
            }
        }
        out.push('\n');
    }
    out
}

// ── 検査 ─────────────────────────────────────────────────────────

const SLICES: &str = "docs/slices.toml";
const ROADMAP: &str = "docs/roadmap.md";
const STATUSES: [&str; 4] = ["todo", "ready", "done", "blocked"];

struct Slice {
    line: usize,
    fields: BTreeMap<String, Value>,
}

impl Slice {
    fn id(&self) -> &str {
        self.str("id").unwrap_or("<id なし>")
    }
    fn str(&self, key: &str) -> Option<&str> {
        self.fields.get(key).and_then(Value::as_str)
    }
    fn status(&self) -> &str {
        self.str("status").unwrap_or("")
    }
    fn has(&self, key: &str) -> bool {
        self.fields.contains_key(key)
    }
    fn depends_on(&self) -> Vec<&str> {
        self.fields
            .get("depends_on")
            .and_then(Value::as_arr)
            .map(|v| v.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }
}

fn load_slices() -> Vec<Slice> {
    parse_slices(&read(SLICES), SLICES)
        .into_iter()
        .map(|(line, fields)| Slice { line, fields })
        .collect()
}

/// ロードマップ「ステータス一覧」表の ID 列。
fn roadmap_status_ids(roadmap: &str) -> BTreeSet<String> {
    let body =
        section(roadmap, "ステータス一覧").expect("roadmap に「## ステータス一覧」節がありません");
    body.lines()
        .filter(|l| l.starts_with('|'))
        .filter_map(|l| l.split('|').nth(1))
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty() && c != "ID" && !c.starts_with('-'))
        .collect()
}

fn report(problems: &[String], what: &str, hint: &str) {
    assert!(
        problems.is_empty(),
        "{what} に {} 件の問題があります（{hint}）:\n- {}",
        problems.len(),
        problems.join("\n- ")
    );
}

#[test]
fn slices_toml_follows_field_definitions() {
    let slices = load_slices();
    assert!(!slices.is_empty(), "{SLICES} に [[slice]] がありません");
    let roadmap_ids = roadmap_status_ids(&read(ROADMAP));
    let status_of: BTreeMap<&str, &str> = slices.iter().map(|s| (s.id(), s.status())).collect();

    let mut problems = Vec::new();
    let mut seen_ids = BTreeSet::new();
    let mut seen_orders: BTreeMap<i64, &str> = BTreeMap::new();
    let mut heading_cache: BTreeMap<String, Option<BTreeSet<String>>> = BTreeMap::new();

    for s in &slices {
        let at = format!("{SLICES}:{} ({})", s.line, s.id());
        for key in ["id", "title", "status", "parent"] {
            if s.str(key).is_none() {
                problems.push(format!("{at}: 必須の {key} が無い（または文字列でない）"));
            }
        }
        if !seen_ids.insert(s.id().to_string()) {
            problems.push(format!("{at}: id が重複している"));
        }
        let status = s.status();
        if !STATUSES.contains(&status) {
            problems.push(format!(
                "{at}: status = \"{status}\" は {STATUSES:?} のどれでもない"
            ));
        }

        // spec（ready / done で必須）
        if matches!(status, "ready" | "done") && !s.has("spec") {
            problems.push(format!("{at}: {status} なのに spec が無い"));
        }
        if let Some(spec) = s.fields.get("spec") {
            let empty = BTreeMap::new();
            let t = match spec {
                Value::Table(t) => t,
                _ => &empty,
            };
            match (
                t.get("doc").and_then(Value::as_str),
                t.get("section").and_then(Value::as_str),
            ) {
                (Some(doc), Some(sec)) => {
                    let hs = heading_cache.entry(doc.to_string()).or_insert_with(|| {
                        fs::read_to_string(repo_root().join(doc))
                            .ok()
                            .map(|t| headings(&t))
                    });
                    match hs {
                        None => problems.push(format!("{at}: spec.doc = \"{doc}\" が存在しない")),
                        Some(hs) if !hs.contains(sec) => problems.push(format!(
                            "{at}: spec.section = \"{sec}\" が {doc} の見出しに無い（完全一致で探す）"
                        )),
                        Some(_) => {}
                    }
                }
                _ => problems.push(format!("{at}: spec に doc と section の両方が要る")),
            }
        }

        // acceptance・design_pr（ready で必須）、impl_pr（done で必須）
        if status == "ready" {
            match s.fields.get("acceptance").and_then(Value::as_arr) {
                Some(a) if !a.is_empty() => {}
                _ => problems.push(format!("{at}: ready なのに acceptance が空（または無い）")),
            }
            if s.fields.get("design_pr").and_then(Value::as_int).is_none() {
                problems.push(format!("{at}: ready なのに design_pr が無い"));
            }
        }
        if status == "done" && s.fields.get("impl_pr").and_then(Value::as_int).is_none() {
            problems.push(format!("{at}: done なのに impl_pr が無い"));
        }

        // order（todo / ready で必須、重複不可）
        match s.fields.get("order") {
            Some(v) => match v.as_int() {
                Some(n) => {
                    if let Some(other) = seen_orders.insert(n, s.id()) {
                        problems.push(format!("{at}: order = {n} が {other} と重複している"));
                    }
                }
                None => problems.push(format!("{at}: order が整数でない")),
            },
            None if matches!(status, "todo" | "ready") => {
                problems.push(format!("{at}: {status} なのに order が無い"));
            }
            None => {}
        }

        if status == "blocked" && s.str("blocked_reason").is_none() {
            problems.push(format!("{at}: blocked なのに blocked_reason が無い"));
        }

        // depends_on
        for dep in s.depends_on() {
            match status_of.get(dep) {
                None => problems.push(format!(
                    "{at}: depends_on の \"{dep}\" が slices.toml に無い"
                )),
                Some(&dep_status) if matches!(status, "ready" | "done") && dep_status != "done" => {
                    problems.push(format!(
                        "{at}: {status} なのに depends_on の {dep} が {dep_status}（done でない）"
                    ));
                }
                Some(_) => {}
            }
        }

        if let Some(parent) = s.str("parent")
            && !roadmap_ids.contains(parent)
        {
            problems.push(format!(
                "{at}: parent = \"{parent}\" が {ROADMAP}「ステータス一覧」の ID 列に無い"
            ));
        }
    }

    report(
        &problems,
        SLICES,
        "slices.toml 冒頭のフィールド定義に合わせる。状態は正本どおりに、正本でない側を直す",
    );
}

/// 相対リンクを検査する Markdown。履歴として凍結した `semantic-review/` は対象外。
fn markdown_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    let root = repo_root();
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&root) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_file() && p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    walk(&root.join("docs"), &mut out);
    walk(&root.join(".github"), &mut out);
    walk(&root.join(".claude"), &mut out);
    out.sort();
    out
}

#[test]
fn markdown_relative_links_resolve() {
    let root = repo_root();
    let mut problems = Vec::new();
    let files = markdown_files();
    assert!(!files.is_empty(), "検査する Markdown が見つかりません");
    for file in &files {
        let text = strip_code(&fs::read_to_string(file).unwrap());
        let rel = file
            .strip_prefix(&root)
            .unwrap_or(file)
            .display()
            .to_string();
        let base = file.parent().unwrap();
        let mut rest = text.as_str();
        while let Some(i) = rest.find("](") {
            rest = &rest[i + 2..];
            let end = rest.find([')', ' ', '\n']).unwrap_or(rest.len());
            let target = &rest[..end];
            rest = &rest[end..];
            let is_url = target.split_once(':').is_some_and(|(scheme, _)| {
                !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphabetic())
            });
            if target.is_empty() || target.starts_with('#') || is_url {
                continue;
            }
            let path = target.split('#').next().unwrap();
            if path.is_empty() {
                continue;
            }
            if !base.join(path).exists() {
                problems.push(format!("{rel}: リンク先 \"{target}\" が存在しない"));
            }
        }
    }
    report(
        &problems,
        "Markdown の相対リンク",
        "リンク先を実在するファイルに直す",
    );
}

// ── パーサ自体のテスト ───────────────────────────────────────────

#[test]
fn subset_parser_reads_supported_forms() {
    let text = r#"
# comment
[[slice]]
id   = "a"   # trailing
n    = 12
arr  = ["x", "y"]
none = []
spec = { doc = "d.md", section = "S（全角）" }
"#;
    let parsed = parse_slices(text, "test");
    assert_eq!(parsed.len(), 1);
    let f = &parsed[0].1;
    assert_eq!(f["id"], Value::Str("a".into()));
    assert_eq!(f["n"], Value::Int(12));
    assert_eq!(f["arr"].as_arr().unwrap().len(), 2);
    assert!(f["none"].as_arr().unwrap().is_empty());
    let Value::Table(t) = &f["spec"] else {
        panic!()
    };
    assert_eq!(t["section"].as_str(), Some("S（全角）"));
}

#[test]
#[should_panic(expected = "TOML の部分集合として読めません")]
fn subset_parser_rejects_unsupported_forms() {
    parse_slices("[[slice]]\nacceptance = [\n  \"a\",\n]\n", "test");
}

#[test]
fn heading_and_section_helpers() {
    let md = "# T\n\n## 現在地\n- **次のタスク:** x\n### 小\n## 次\nbody\n```\n## code\n```\n";
    let hs = headings(md);
    assert!(hs.contains("現在地") && hs.contains("小") && !hs.contains("code"));
    let cur = section(md, "現在地").unwrap();
    assert!(cur.contains("次のタスク") && cur.contains("### 小") && !cur.contains("body"));
}

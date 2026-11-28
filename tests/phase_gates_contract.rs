//! Phase の完了 gate の一覧 `docs/phase-gates.toml` の契約テスト（verification-release-operations §19）。
//!
//! 一覧の形と、一覧が指す見出し・テスト・スライスの実在を機械的に確かめ、Phase ごとの「満たした数 / 全数」を出す。
//!
//! 1. 各 `[[gate]]`・`[[unnumbered]]` に必須フィールドがあり、`phase` が 0〜7。`[[gate]]` は同じ `phase` と `id` の組が 1 行だけ
//! 2. `source.doc` が実在し、`source.section` が見出しとして完全一致で存在する。`[[gate]]` は `id` がその節の本文に現れる
//! 3. `tests` の各項目が `tests/` か `src/` の下の `.rs` で、ファイルと `fn <関数名>` が実在する。`open` の id が `slices.toml` にあり `done` でない
//! 4. Phase ごとの数を 1 行ずつ標準出力に出す（`-- --nocapture` で見える）。数によって失敗しない（§19.3、目標値・閾値を置かない）
//!
//! 1〜3 に反する行があればまとめて報告して失敗する。TOML の部分集合パーサは `tests/support/toml_subset.rs`（`slices_contract.rs` と共有）。

#[path = "support/toml_subset.rs"]
mod toml_subset;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use toml_subset::{Value, headings, parse_tables};

const GATES: &str = "docs/phase-gates.toml";
const SLICES: &str = "docs/slices.toml";

fn read(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} を読めません: {e}", path.display()))
}

fn read_opt(rel: &str) -> Option<String> {
    fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel)).ok()
}

/// 見出し `title`（深さは問わない・コードブロックの外）の節の本文。次の同じ深さ以下の見出しの手前まで。
fn section_body(markdown: &str, title: &str) -> Option<String> {
    let mut in_fence = false;
    let mut level = 0;
    let mut body = String::new();
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        let hashes = if in_fence {
            0
        } else {
            line.chars().take_while(|&c| c == '#').count()
        };
        let is_heading = (1..=6).contains(&hashes) && line[hashes..].starts_with(' ');
        if level == 0 {
            if is_heading && line[hashes..].trim() == title {
                level = hashes;
            }
            continue;
        }
        if is_heading && hashes <= level {
            break;
        }
        body.push_str(line);
        body.push('\n');
    }
    (level != 0).then_some(body)
}

type Fields = BTreeMap<String, Value>;

fn str_field<'a>(f: &'a Fields, key: &str) -> Option<&'a str> {
    f.get(key).and_then(Value::as_str)
}

fn str_list(f: &Fields, key: &str) -> Option<Vec<String>> {
    f.get(key)?
        .as_arr()?
        .iter()
        .map(|v| v.as_str().map(str::to_string))
        .collect()
}

/// `slices.toml` の id → status。
fn slice_statuses() -> BTreeMap<String, String> {
    parse_tables(&read(SLICES), SLICES, &["slice"])
        .into_iter()
        .filter_map(|(_, _, f)| {
            Some((
                str_field(&f, "id")?.to_string(),
                str_field(&f, "status")?.to_string(),
            ))
        })
        .collect()
}

#[derive(Default)]
struct Counts {
    satisfied: usize,
    total: usize,
    unnumbered: usize,
}

/// 一覧の本文を検査し、(問題の一覧, Phase ごとの数) を返す。
fn check(text: &str, statuses: &BTreeMap<String, String>) -> (Vec<String>, BTreeMap<i64, Counts>) {
    let mut problems = Vec::new();
    let mut counts: BTreeMap<i64, Counts> = BTreeMap::new();
    let mut seen: BTreeSet<(i64, String)> = BTreeSet::new();
    let mut docs: BTreeMap<String, Option<String>> = BTreeMap::new();

    for (line, kind, f) in parse_tables(text, GATES, &["gate", "unnumbered"]) {
        let at = format!("{GATES}:{line} [[{kind}]]");
        let gate = kind == "gate";
        let required: &[&str] = if gate {
            &["phase", "id", "source", "tests"]
        } else {
            &["phase", "source", "note"]
        };
        let mut ok = true;
        for key in required {
            if !f.contains_key(*key) {
                problems.push(format!("{at}: 必須フィールド {key} がない"));
                ok = false;
            }
        }
        if !gate {
            for key in ["id", "tests", "open"] {
                if f.contains_key(key) {
                    problems.push(format!("{at}: [[unnumbered]] は {key} を持たない"));
                }
            }
        }
        let phase = match f.get("phase").map(|v| v.as_int()) {
            Some(Some(n @ 0..=7)) => Some(n),
            Some(_) => {
                problems.push(format!("{at}: phase は 0〜7 の整数"));
                None
            }
            None => None,
        };
        let id = str_field(&f, "id").map(str::to_string);
        if gate {
            if f.contains_key("id") && id.is_none() {
                problems.push(format!("{at}: id は文字列"));
            }
            if let (Some(p), Some(i)) = (phase, &id)
                && !seen.insert((p, i.clone()))
            {
                problems.push(format!("{at}: phase {p} の {i} が重複"));
            }
        }

        // source: 見出しの実在と、id が節の本文に現れること。
        if let Some(Value::Table(src)) = f.get("source") {
            match (str_field(src, "doc"), str_field(src, "section")) {
                (Some(doc), Some(sec)) => {
                    let md = docs
                        .entry(doc.to_string())
                        .or_insert_with(|| read_opt(doc))
                        .clone();
                    match md {
                        None => problems.push(format!("{at}: source.doc {doc} が実在しない")),
                        Some(md) if !headings(&md).contains(sec) => {
                            problems.push(format!("{at}: {doc} に見出し「{sec}」が完全一致で無い"))
                        }
                        Some(md) => {
                            if let Some(i) = id.as_deref().filter(|_| gate) {
                                let body = section_body(&md, sec).unwrap_or_default();
                                if !body.contains(i) {
                                    problems.push(format!(
                                        "{at}: {doc} の「{sec}」の節に {i} が現れない"
                                    ));
                                }
                            }
                        }
                    }
                }
                _ => problems.push(format!(
                    "{at}: source は doc と section の文字列を持つ inline table"
                )),
            }
        } else if f.contains_key("source") {
            problems.push(format!("{at}: source は inline table"));
        }
        if !gate && f.contains_key("note") && str_field(&f, "note").is_none_or(str::is_empty) {
            problems.push(format!("{at}: note は空でない文字列"));
        }

        // tests・open（[[gate]] だけ）。
        let mut tests_ok = true;
        let mut open_len = 0;
        if gate {
            match str_list(&f, "tests") {
                None if f.contains_key("tests") => {
                    problems.push(format!("{at}: tests は文字列の配列"));
                    tests_ok = false;
                }
                None => tests_ok = false,
                Some(list) => {
                    for item in &list {
                        if let Some(p) = check_test(item) {
                            problems.push(format!("{at}: tests の {item}: {p}"));
                        }
                    }
                    tests_ok = !list.is_empty();
                }
            }
            if f.contains_key("open") {
                match str_list(&f, "open") {
                    None => problems.push(format!("{at}: open は文字列の配列")),
                    Some(list) => {
                        open_len = list.len();
                        for s in &list {
                            match statuses.get(s).map(String::as_str) {
                                None => problems
                                    .push(format!("{at}: open の {s} が slices.toml に無い")),
                                Some("done") => problems
                                    .push(format!("{at}: open の {s} は done（open から外す）")),
                                Some(_) => {}
                            }
                        }
                    }
                }
            }
        }

        if let (Some(p), true) = (phase, ok) {
            let c = counts.entry(p).or_default();
            if gate {
                c.total += 1;
                if tests_ok && open_len == 0 {
                    c.satisfied += 1;
                }
            } else {
                c.unnumbered += 1;
            }
        }
    }
    (problems, counts)
}

/// `tests` の 1 項目 `<パス>::<関数名>` の問題（無ければ None）。
fn check_test(item: &str) -> Option<String> {
    let Some((path, name)) = item.split_once("::") else {
        return Some("<パス>::<関数名> の形でない".into());
    };
    let under = path.starts_with("tests/") || path.starts_with("src/");
    if !under || !path.ends_with(".rs") || path.contains("..") {
        return Some("パスは tests/ か src/ の下の .rs".into());
    }
    let Some(src) = read_opt(path) else {
        return Some(format!("{path} が実在しない"));
    };
    if !src.contains(&format!("fn {name}(")) {
        return Some(format!("{path} に fn {name} が無い"));
    }
    None
}

fn print_counts(counts: &BTreeMap<i64, Counts>) {
    for phase in 0..=7 {
        match counts.get(&phase) {
            Some(c) => println!(
                "Phase {phase}: 満たした {} / 全 {}（残り {}。ID 未採番 {}）",
                c.satisfied,
                c.total,
                c.total - c.satisfied,
                c.unnumbered
            ),
            None => println!("Phase {phase}: 一覧なし"),
        }
    }
}

#[test]
fn phase_gates_follow_field_definitions() {
    let (problems, counts) = check(&read(GATES), &slice_statuses());
    print_counts(&counts);
    assert!(
        problems.is_empty(),
        "{GATES} に {} 件の問題があります（§19.2・§19.3 に合わせて一覧を直す）:\n- {}",
        problems.len(),
        problems.join("\n- ")
    );
}

// ── 検査自体のテスト（PGT-AT-01・02）──────────────────────────────

fn problems_of(text: &str) -> Vec<String> {
    let mut statuses = BTreeMap::new();
    statuses.insert("open-slice".to_string(), "todo".to_string());
    statuses.insert("done-slice".to_string(), "done".to_string());
    check(text, &statuses).0
}

fn assert_reports(text: &str, needle: &str) {
    let problems = problems_of(text);
    assert!(
        problems.iter().any(|p| p.contains(needle)),
        "「{needle}」を含む問題が報告されるはず: {problems:?}"
    );
}

const GOOD_SOURCE: &str = r#"source = { doc = "docs/threat-model.md", section = "Phase 0" }"#;

#[test]
fn pgt_at_01_shape_violations_are_reported() {
    let good = format!("[[gate]]\nphase = 0\nid = \"TM-AT-07\"\n{GOOD_SOURCE}\ntests = []\n");
    assert!(problems_of(&good).is_empty(), "{:?}", problems_of(&good));
    // 必須フィールドの欠け
    for key in ["phase", "id", "source", "tests"] {
        let broken: String = good
            .lines()
            .filter(|l| !l.starts_with(key))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_reports(&broken, &format!("必須フィールド {key} がない"));
    }
    let un = format!("[[unnumbered]]\nphase = 0\n{GOOD_SOURCE}\nnote = \"x\"\n");
    assert!(problems_of(&un).is_empty());
    for key in ["phase", "source", "note"] {
        let broken: String = un
            .lines()
            .filter(|l| !l.starts_with(key))
            .map(|l| format!("{l}\n"))
            .collect();
        assert_reports(&broken, &format!("必須フィールド {key} がない"));
    }
    assert_reports(
        &format!("{un}tests = []\n"),
        "[[unnumbered]] は tests を持たない",
    );
    // phase の範囲外
    assert_reports(&good.replace("phase = 0", "phase = 8"), "phase は 0〜7");
    // 見出しの不在・doc の不在
    assert_reports(
        &good.replace("\"Phase 0\"", "\"Phase 99\""),
        "見出し「Phase 99」",
    );
    assert_reports(&good.replace("threat-model.md", "no-such.md"), "実在しない");
    // id が節に現れない
    assert_reports(&good.replace("TM-AT-07", "TM-AT-99"), "TM-AT-99 が現れない");
    // 重複
    assert_reports(&format!("{good}\n{good}"), "重複");
}

#[test]
fn pgt_at_02_test_and_open_links_are_checked() {
    let with = |extra: &str| {
        format!("[[gate]]\nphase = 0\nid = \"TM-AT-07\"\n{GOOD_SOURCE}\ntests = []\n{extra}\n")
    };
    let tests = |item: &str| with("").replace("tests = []", &format!("tests = [\"{item}\"]"));
    assert_reports(
        &tests("docs/phase-gates.toml::x"),
        "tests/ か src/ の下の .rs",
    );
    assert_reports(&tests("tests/no_such_file.rs::x"), "実在しない");
    assert_reports(
        &tests("tests/slices_contract.rs::no_such_fn"),
        "fn no_such_fn が無い",
    );
    assert_reports(
        &tests("tests/slices_contract.rs"),
        "<パス>::<関数名> の形でない",
    );
    assert!(
        problems_of(&tests(
            "tests/slices_contract.rs::slices_toml_follows_field_definitions"
        ))
        .is_empty()
    );
    assert_reports(&with("open = [\"no-such-slice\"]"), "slices.toml に無い");
    assert_reports(&with("open = [\"done-slice\"]"), "done");
    assert!(problems_of(&with("open = [\"open-slice\"]")).is_empty());
}

#[test]
fn counts_distinguish_satisfied_open_and_empty() {
    let text = format!(
        "[[gate]]\nphase = 0\nid = \"TM-AT-07\"\n{GOOD_SOURCE}\ntests = [\"tests/slices_contract.rs::slices_toml_follows_field_definitions\"]\n\
         [[gate]]\nphase = 0\nid = \"TM-AT-12\"\n{GOOD_SOURCE}\ntests = [\"tests/slices_contract.rs::slices_toml_follows_field_definitions\"]\nopen = [\"open-slice\"]\n\
         [[gate]]\nphase = 1\nid = \"TM-AT-07\"\n{GOOD_SOURCE}\ntests = []\n\
         [[unnumbered]]\nphase = 0\n{GOOD_SOURCE}\nnote = \"x\"\n"
    );
    let mut statuses = BTreeMap::new();
    statuses.insert("open-slice".to_string(), "todo".to_string());
    let (problems, counts) = check(&text, &statuses);
    assert!(problems.is_empty(), "{problems:?}");
    let p0 = &counts[&0];
    assert_eq!((p0.satisfied, p0.total, p0.unnumbered), (1, 2, 1));
    let p1 = &counts[&1];
    assert_eq!((p1.satisfied, p1.total), (0, 1));
    assert!(!counts.contains_key(&2));
}

#[test]
fn section_body_stops_at_next_same_level_heading() {
    let md = "# T\n## A\n### Phase 0\nx TM-1\n### Phase 1\ny\n```\n### Phase 0\n```\n";
    let body = section_body(md, "Phase 0").unwrap();
    assert!(body.contains("TM-1") && !body.contains('y'));
    assert!(section_body(md, "無い").is_none());
}

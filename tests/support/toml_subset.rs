//! 文書の契約テスト（tests/slices_contract.rs・tests/phase_gates_contract.rs）が共有する、TOML の部分集合パーサと見出しの抽出。
//!
//! 読める書き方は `[[name]]` の並び、文字列・整数・配列・inline table、行末コメントだけである。
//! 部分集合の外の書き方を見つけたら、黙って読み飛ばさずに失敗させる。

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Int(i64),
    Arr(Vec<Value>),
    Table(BTreeMap<String, Value>),
}

impl Value {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) => Some(*n),
            _ => None,
        }
    }
    pub fn as_arr(&self) -> Option<&[Value]> {
        match self {
            Value::Arr(v) => Some(v),
            _ => None,
        }
    }
}

pub struct Cursor<'a> {
    pub chars: Vec<char>,
    pub pos: usize,
    pub line: usize,
    pub file: &'a str,
}

impl Cursor<'_> {
    pub fn fail(&self, msg: &str) -> ! {
        panic!(
            "{}:{}: TOML の部分集合として読めません（{msg}）。このテストのパーサが読める書き方に揃えるか、パーサを広げてください",
            self.file, self.line
        )
    }
    pub fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }
    pub fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.pos += 1;
        }
    }
    pub fn expect(&mut self, c: char) {
        self.skip_ws();
        if self.peek() != Some(c) {
            self.fail(&format!("'{c}' が必要"));
        }
        self.pos += 1;
    }
    pub fn key(&mut self) -> String {
        self.skip_ws();
        let start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            self.pos += 1;
        }
        if start == self.pos {
            self.fail("キーが必要");
        }
        self.chars[start..self.pos].iter().collect()
    }
    pub fn value(&mut self) -> Value {
        self.skip_ws();
        match self.peek() {
            Some('"') => {
                self.pos += 1;
                let mut s = String::new();
                loop {
                    match self.peek() {
                        Some('"') => {
                            self.pos += 1;
                            return Value::Str(s);
                        }
                        Some('\\') => self.fail("エスケープは未対応"),
                        Some(c) => {
                            s.push(c);
                            self.pos += 1;
                        }
                        None => self.fail("文字列が閉じていない"),
                    }
                }
            }
            Some('[') => {
                self.pos += 1;
                let mut items = Vec::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some(']') {
                        self.pos += 1;
                        return Value::Arr(items);
                    }
                    items.push(self.value());
                    self.skip_ws();
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some(']') => {}
                        _ => self.fail("配列は 1 行で書き、要素は ',' で区切る"),
                    }
                }
            }
            Some('{') => {
                self.pos += 1;
                let mut table = BTreeMap::new();
                loop {
                    self.skip_ws();
                    if self.peek() == Some('}') {
                        self.pos += 1;
                        return Value::Table(table);
                    }
                    let k = self.key();
                    self.expect('=');
                    let v = self.value();
                    if table.insert(k.clone(), v).is_some() {
                        self.fail(&format!("inline table のキー {k} が重複"));
                    }
                    self.skip_ws();
                    match self.peek() {
                        Some(',') => self.pos += 1,
                        Some('}') => {}
                        _ => self.fail("inline table の区切りは ','"),
                    }
                }
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.pos;
                while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                    self.pos += 1;
                }
                let text: String = self.chars[start..self.pos].iter().collect();
                Value::Int(
                    text.parse()
                        .unwrap_or_else(|_| self.fail("整数が大きすぎる")),
                )
            }
            _ => self.fail("値は文字列・整数・配列・inline table のどれか"),
        }
    }
    pub fn end_of_line(&mut self) {
        self.skip_ws();
        match self.peek() {
            None | Some('#') => {}
            _ => self.fail("値の後ろに余計な文字がある"),
        }
    }
}

/// `names` に挙げた `[[name]]` の並びを読む。各要素は (開始行, 表名, キー → 値)。
/// 部分集合の外の書き方（挙げていない table・table より前のキー・重複キーなど）は失敗させる。
pub fn parse_tables(
    text: &str,
    file: &str,
    names: &[&str],
) -> Vec<(usize, String, BTreeMap<String, Value>)> {
    let mut tables: Vec<(usize, String, BTreeMap<String, Value>)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line_no = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = names.iter().find(|n| line == format!("[[{n}]]")) {
            tables.push((line_no, (*name).to_string(), BTreeMap::new()));
            continue;
        }
        let mut cur = Cursor {
            chars: raw.chars().collect(),
            pos: 0,
            line: line_no,
            file,
        };
        if line.starts_with('[') {
            let listed: Vec<String> = names.iter().map(|n| format!("[[{n}]]")).collect();
            cur.fail(&format!("{} 以外の table は未対応", listed.join("・")));
        }
        let Some((_, _, current)) = tables.last_mut() else {
            cur.fail("最初の [[table]] より前にキーがある");
        };
        let key = cur.key();
        cur.expect('=');
        let value = cur.value();
        cur.end_of_line();
        if current.insert(key.clone(), value).is_some() {
            cur.fail(&format!("キー {key} が重複"));
        }
    }
    tables
}

/// フェンス付きコードブロックの外にある見出しのテキスト（`#` と前後の空白を除いたもの）。
pub fn headings(markdown: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let hashes = line.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
            out.insert(line[hashes..].trim().to_string());
        }
    }
    out
}

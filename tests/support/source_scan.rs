//! 静的検査（tests/ambient_access_audit.rs と tests/no_host_abort_audit.rs）が共有する、`src/` の
//! production コードの切り出しと、パターン・`use` の照合。
//!
//! 「production コード」の定義はここに 1 つだけ持つ（manifesto-check-no-host-abort の Q1=A・Q6=A）。
//! 文字列・コメントを除いた行から、`#[cfg(test)]` の付いた item を除いたものである。除外の規則は
//! [`production_lines`] に書く。2 つの検査が違う範囲を見ないようにするための共有で、検査項目・許可リスト・
//! 対応表は各テストが持つ。

#![allow(dead_code)]

use std::path::{Path, PathBuf};

/// 検出パターン 1 つ分。
pub struct Pattern {
    /// 許可リストと報告で使う名前。
    pub key: &'static str,
    /// 行の中の文字列照合（左は識別子の途中でないこと。末尾が `*` か `::` なら前方一致、無ければ右も識別子の途中でないこと）。
    pub texts: &'static [&'static str],
    /// `use` 文で取り込まれた path がこれと等しいか、これの下にあれば違反。
    pub use_paths: &'static [&'static str],
    /// `use` 文で取り込まれた path がこれと厳密に等しいとき（module の取り込み）だけ違反。
    pub use_modules: &'static [&'static str],
}

/// 違反 1 件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub file: String,
    pub key: &'static str,
    pub line: usize,
    pub text: String,
}

pub fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `line` の中に `pat` が（左右の境界つきで）あるか。
pub fn line_has(line: &str, pat: &str) -> bool {
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
pub fn strip_literals_and_comment(line: &str) -> String {
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

pub fn brace_delta(code: &str) -> i64 {
    code.chars()
        .map(|c| match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        })
        .sum()
}

/// production コードの（行番号, 文字列・コメントを除いた行）。
///
/// `#[cfg(test)]` の直後の item を除く（production の build に入らないコードだから）。除く範囲:
///
/// - 直後が `mod` のもの: その module の終わりまで（`mod x;` は 1 行）。
/// - 直後の行が `,` で終わるもの（構造体のフィールド・struct literal のフィールド・関数呼びの引数）: その 1 行だけ。
/// - それ以外の文・式・関数・impl: その item の終わりまで（brace のある item は brace が閉じるまで、無いものは `;` まで）。
///
/// `#[cfg(test)]` と item が同じ行にあるもの（`#[cfg(test)] x: T,`）は、attribute の後ろをその item の頭として
/// 読み、上の規則を当てる（次の行を item の頭として読まない）。
pub fn production_lines(content: &str) -> Vec<(usize, String)> {
    const ATTR: &str = "#[cfg(test)]";
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if let Some(rest) = trimmed.strip_prefix(ATTR) {
            // item の頭の行（と、その行のうち item に当たる部分）を決める。
            let rest = rest.trim_start();
            let (head_idx, head_text) = if !rest.is_empty() && !rest.starts_with("//") {
                (i, rest)
            } else {
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
                (j, lines.get(j).map(|l| l.trim_start()).unwrap_or(""))
            };
            let head_code = strip_literals_and_comment(head_text);
            let head_code = head_code.trim();
            let item = head_code
                .strip_prefix("pub(crate) ")
                .or_else(|| head_code.strip_prefix("pub "))
                .unwrap_or(head_code);
            if !item.starts_with("mod ") && head_code.ends_with(',') {
                // フィールド・引数・struct literal のフィールド: その 1 行だけ。
                i = head_idx + 1;
                continue;
            }
            // `mod x;` は `;` まで、それ以外の item は brace が閉じるまで（brace が無ければ `;` まで）。
            let mut depth: i64 = 0;
            let mut opened = false;
            let mut k = head_idx;
            while k < lines.len() {
                let text = if k == head_idx { head_text } else { lines[k] };
                let code = strip_literals_and_comment(text);
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
pub fn expand_use(body: &str) -> Vec<String> {
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

pub fn path_is_or_under(path: &str, base: &str) -> bool {
    path == base || path.starts_with(&format!("{base}::"))
}

/// 1 つのファイル（`rel` は `src/` からの相対 path、`/` 区切り）の production コードを走査する。
pub fn scan_source(rel: &str, content: &str, patterns: &[Pattern]) -> Vec<Violation> {
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
        for pat in patterns {
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
                    for pat in patterns {
                        // グロブ（`X::*`）は X の下の item をすべて持ち込む。X がパターンの path か module か、
                        // その親なら違反。
                        let glob = path.strip_suffix("::*");
                        let hit = pat.use_paths.iter().any(|b| path_is_or_under(&path, b))
                            || pat.use_modules.contains(&path.as_str())
                            || glob.is_some_and(|g| {
                                pat.use_modules.contains(&g)
                                    || pat.use_paths.iter().any(|b| path_is_or_under(b, g))
                            });
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

pub fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

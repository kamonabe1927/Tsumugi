//! 原則 1・6 の静的検査: Tsumugi 自身の crate（lib・CLI adapter・開発用ツール）の production コードに
//! `unsafe` を足せないことを固定する（loop-guard §4.1 の 1 段目。manifesto-check-no-unsafe、
//! docs/embedding-api.md §14 の同名小節、issue #543）。
//!
//! 検出は各 crate root の `#![cfg_attr(not(test), forbid(unsafe_code))]` に任せ（コンパイラの lint が
//! `unsafe` ブロック・`unsafe fn`・`unsafe impl`・`#[no_mangle]` などを止める）、このテストは
//! (1) 全 crate root にその attribute が最初の item より前にあること（NUS-AT-01）、
//! (2) その attribute が `rustc` で実際に production の `unsafe` を止め、`#[cfg(test)]` の中は通すこと（NUS-AT-02）
//! を確かめる。NUS-AT-03（今の main で build・clippy・test が通る）は、このテストを含む `cargo test` 自体と CI で見る。
//!
//! 検査項目・守る原則・embedding-api §11 の規則・脅威モデルの TM-ID の対応表（NUS-AT-01..03）。
//!
//! | 検査項目 | 守る原則 | embedding-api §11 の規則 | 脅威モデル |
//! |---|---|---|---|
//! | crate root の attribute の位置（NUS-AT-01） | 1・6 | 規則 6 | TM-014（FFI UB の残余リスク）・B3 |
//! | attribute の意味（production の `unsafe` を止め、`allow` で戻せず、`cfg(test)` は通す）（NUS-AT-02） | 1・6 | 規則 4・6 | TM-014 |
//!
//! 限界: `#[cfg(test)]` の付いたコードの `unsafe`、依存 crate の中の `unsafe`（TM-012）、`benches/`・`examples/`・
//! `tests/` の crate、build script は対象外。attribute とこのテストの両方を消す変更はレビューで見る。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 各 crate root の先頭に置く attribute（行全体。前後の空白は無視する）。
const ATTR: &str = "#![cfg_attr(not(test), forbid(unsafe_code))]";

/// `src/` の crate root（Cargo の自動検出の規則: `src/lib.rs`・`src/main.rs`・`src/bin/*.rs`・`src/bin/*/main.rs`）。
fn crate_roots(src: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for name in ["lib.rs", "main.rs"] {
        let p = src.join(name);
        if p.is_file() {
            roots.push(p);
        }
    }
    let bin = src.join("bin");
    if let Ok(rd) = fs::read_dir(&bin) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_file() && p.extension().is_some_and(|e| e == "rs") {
                roots.push(p);
            } else if p.is_dir() && p.join("main.rs").is_file() {
                roots.push(p.join("main.rs"));
            }
        }
    }
    roots.sort();
    roots
}

/// attribute が最初の item より前にあるか。コメント（`//`・`/* */`）と空行と他の inner attribute
/// （`#![...]`。複数行も）は飛ばす。コメントの中の一致は数えない。
fn has_attr_before_first_item(content: &str) -> bool {
    let mut in_block = false;
    let mut in_attr = false;
    for line in content.lines() {
        let t = line.trim();
        if in_block {
            if let Some(i) = t.find("*/") {
                in_block = false;
                if !t[i + 2..].trim().is_empty() {
                    return false;
                }
            }
            continue;
        }
        if in_attr {
            if t.ends_with(']') {
                in_attr = false;
            }
            continue;
        }
        if t.is_empty() || t.starts_with("//") {
            continue;
        }
        if t.starts_with("/*") {
            if !t.contains("*/") {
                in_block = true;
            }
            continue;
        }
        if t == ATTR {
            return true;
        }
        if t.starts_with("#![") {
            if !t.ends_with(']') {
                in_attr = true;
            }
            continue;
        }
        return false; // 最初の item に着いた
    }
    false
}

fn rustc() -> String {
    std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string())
}

/// fixture を `rustc --edition 2024 --emit metadata` に通して成功したか。
fn compiles(name: &str, source: &str, as_test: bool) -> bool {
    let dir =
        std::env::temp_dir().join(format!("tsumugi_no_unsafe_{}_{}", std::process::id(), name));
    fs::create_dir_all(&dir).expect("create fixture dir");
    let file = dir.join("fixture.rs");
    fs::write(&file, source).expect("write fixture");
    let mut cmd = Command::new(rustc());
    cmd.args(["--edition", "2024", "--emit", "metadata", "--out-dir"])
        .arg(&dir)
        .arg(&file);
    if as_test {
        cmd.arg("--test");
    } else {
        cmd.args(["--crate-type", "lib"]);
    }
    let ok = cmd.output().expect("run rustc").status.success();
    let _ = fs::remove_dir_all(&dir);
    ok
}

#[test]
fn nus_at_01_every_crate_root_has_the_attribute() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let roots = crate_roots(&src);
    assert!(roots.len() >= 3, "crate root が見つからない: {roots:?}");
    let missing: Vec<String> = roots
        .iter()
        .filter(|p| !has_attr_before_first_item(&fs::read_to_string(p).expect("read crate root")))
        .map(|p| p.display().to_string())
        .collect();
    assert!(
        missing.is_empty(),
        "次の crate root の最初の item より前に `{ATTR}` が無い（embedding-api §11 規則 6）: {missing:?}"
    );
}

#[test]
fn nus_at_01_detector_unit() {
    let ok = format!("//! doc\n\n// 理由\n{ATTR}\n\n#![allow(dead_code)]\nmod a;\n");
    assert!(has_attr_before_first_item(&ok));
    assert!(has_attr_before_first_item(&format!(
        "  {ATTR}  \nfn f() {{}}\n"
    )));
    // 他の inner attribute（複数行を含む）の後ろでもよい。
    assert!(has_attr_before_first_item(&format!(
        "#![allow(\n    dead_code\n)]\n{ATTR}\nfn f() {{}}\n"
    )));
    // 欠ける。
    assert!(!has_attr_before_first_item("//! doc\nmod a;\n"));
    assert!(!has_attr_before_first_item(""));
    // 弱めた（warn）・違う文字列。
    assert!(!has_attr_before_first_item(
        "#![cfg_attr(not(test), warn(unsafe_code))]\nmod a;\n"
    ));
    assert!(!has_attr_before_first_item(
        "#![forbid(unsafe_code)]\nmod a;\n"
    ));
    // item の後ろ。
    assert!(!has_attr_before_first_item(&format!("mod a;\n{ATTR}\n")));
    // コメントの中だけ。
    assert!(!has_attr_before_first_item(&format!("// {ATTR}\nmod a;\n")));
    assert!(!has_attr_before_first_item(&format!(
        "/*\n{ATTR}\n*/\nmod a;\n"
    )));
}

#[test]
fn nus_at_01_crate_roots_enumeration_unit() {
    let dir = std::env::temp_dir().join(format!("tsumugi_no_unsafe_roots_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("bin/tool")).unwrap();
    fs::create_dir_all(dir.join("bin/not_a_bin")).unwrap();
    for f in [
        "lib.rs",
        "main.rs",
        "other.rs",
        "bin/a.rs",
        "bin/tool/main.rs",
        "bin/tool/x.rs",
        "bin/not_a_bin/y.rs",
    ] {
        fs::write(dir.join(f), "").unwrap();
    }
    let got: Vec<String> = crate_roots(&dir)
        .iter()
        .map(|p| {
            p.strip_prefix(&dir)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(got, ["bin/a.rs", "bin/tool/main.rs", "lib.rs", "main.rs"]);
}

#[test]
fn nus_at_02_attribute_stops_production_unsafe() {
    // 土台: unsafe の無い lib は通る（rustc の呼び出し自体が壊れていないことの確認）。
    assert!(compiles(
        "clean",
        &format!("{ATTR}\npub fn f() {{}}\n"),
        false
    ));
    // production の unsafe ブロックは失敗する。
    assert!(!compiles(
        "unsafe_block",
        &format!("{ATTR}\npub fn f() {{ unsafe {{}} }}\n"),
        false
    ));
    // `unsafe fn` も失敗する。
    assert!(!compiles(
        "unsafe_fn",
        &format!("{ATTR}\npub unsafe fn f() {{}}\n"),
        false
    ));
    // `#[allow(unsafe_code)]` で戻そうとしても失敗する（forbid、E0453）。
    assert!(!compiles(
        "allow_back",
        &format!("{ATTR}\n#[allow(unsafe_code)]\npub fn f() {{ unsafe {{}} }}\n"),
        false
    ));
    // `--test` の build の `#[cfg(test)]` の中の unsafe は通る。
    assert!(compiles(
        "cfg_test",
        &format!(
            "{ATTR}\npub fn f() {{}}\n#[cfg(test)]\nmod t {{\n    #[test]\n    fn x() {{ unsafe {{}} }}\n}}\n"
        ),
        true
    ));
    // attribute が無ければ unsafe は通る（attribute が効いている対照）。
    assert!(compiles("no_attr", "pub fn f() { unsafe {} }\n", false));
}

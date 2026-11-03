//! `unstable-bytecode` を無効にした構成（`default-features = false`）の build 検査
//! （NDF-AT-01..03、semantic-decisions §17.7.9、issue #294）。
//!
//! 子の cargo を専用の target directory で呼び、warning を error にして成功を確かめる。

use std::path::PathBuf;
use std::process::Command;

fn run_cargo(args: &[&str]) {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let target = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("no-default-features");
    let out = Command::new(env!("CARGO"))
        .args(args)
        .args(["--no-default-features", "--locked", "--manifest-path"])
        .arg(&manifest)
        .arg("--target-dir")
        .arg(&target)
        .env("RUSTFLAGS", "-D warnings")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .output()
        .expect("cargo を起動できない");
    assert!(
        out.status.success(),
        "cargo {args:?} --no-default-features が失敗した:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// NDF-AT-01: library の check が warning なしで成功する。
#[test]
fn ndf_at_01_lib_checks_without_default_features() {
    run_cargo(&["check", "--lib"]);
}

/// NDF-AT-02: bin 群の build が成功する（bin `tsumugi` は required-features で作られない）。
#[test]
fn ndf_at_02_bins_build_without_default_features() {
    run_cargo(&["build", "--bins"]);
}

/// NDF-AT-03: 既定の構成では bin `tsumugi` が `required-features` 付きで宣言されている。
#[test]
fn ndf_at_03_tsumugi_bin_requires_feature() {
    let toml = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    assert!(toml.contains("required-features = [\"unstable-bytecode\"]"));
}

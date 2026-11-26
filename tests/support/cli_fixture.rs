//! tree と `--vm` の CLI で fixture を流すときの組み立て（引数・環境変数・時間の上限・stderr の
//! experimental の警告の除き方）。`tests/integration.rs`（期待ファイルとの比較）と `tests/vm_parity.rs`
//! （tree と VM の CLI の byte 比較、vm-parity-diff-b）が同じ組み立てを使うために共有する。

#![allow(dead_code)]

/// 子プロセスの既定実行上限。ハングをテスト失敗として検出する。
pub const DEFAULT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// fixture がスクリプトへ渡す一時ディレクトリのキー。
///
/// `TSUMUGI_` 始まりは処理系が保護して `env()` から読めないため、別prefixを使う。
pub const TEST_DIR_ENV: &str = "TSG_TEST_DIR";

/// 子プロセスの完了を待つ。制限時間を超えたら kill して失敗させる。
pub fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: std::time::Duration,
    context: &str,
) -> std::process::Output {
    let deadline = std::time::Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child.wait_with_output().unwrap_or_else(|error| {
                    panic!("{context}: プロセスの出力取得に失敗: {error}")
                });
            }
            Ok(None) if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{context}: {}秒以内に完了しませんでした", timeout.as_secs());
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(10)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("{context}: プロセス状態の取得に失敗: {error}");
            }
        }
    }
}

/// fixture 固有の追加環境変数。fixture 名からここだけで決める。
pub fn fixture_envs(name: &str, test_dir: &str) -> Vec<(String, String)> {
    let mut envs = vec![(TEST_DIR_ENV.to_string(), test_dir.to_string())];
    // legacy profile で実行する fixture は、空 env-allow の stderr 警告が golden の「stderr 空」
    // 判定を壊すため、TSUMUGI_ENV_ALLOW を設定して env 警告を抑止する（C10）。
    if fixture_needs_legacy(name) {
        envs.push(("TSUMUGI_ENV_ALLOW".to_string(), "*".to_string()));
    }
    match name {
        // ファイルI/O系はテスト専用ディレクトリだけを許可する。
        // error_sandbox はその範囲外（/etc/hostname）への読み取りを拒否させる。
        "file_io" | "filesystem" | "string_utils" | "error_sandbox" => {
            envs.push(("TSUMUGI_SANDBOX".to_string(), test_dir.to_string()));
        }
        // local_utils は任意の絶対パスの path_exists（範囲外は false）を検証するため、
        // sandbox を設定せず unrestricted（/ 全体を default mount）で実行する。空 sandbox 警告が
        // stderr に 1 件出るのは許容する（run_fixture の golden stderr 判定が legacy 警告を許す）。
        // 無限ループを短い予算で止める。legacy env は CLI adapter（execution-control §13）が
        // tree/VM の全経路へ同じく適用するため、期待は共通の `.expected_err`（上限 100）で、
        // 先頭に deprecation warning が 1 行出る。
        "error_step_limit" => {
            envs.push(("TSUMUGI_MAX_STEPS".to_string(), "100".to_string()));
        }
        _ => {}
    }
    envs
}

/// fixture 固有の実行上限。既定より短くするのは停止性を検証する fixture だけ。
pub fn fixture_timeout(name: &str) -> std::time::Duration {
    match name {
        // AUD-026: i64極値のtimestampでも定数時間で完了することの検証
        "format_time_extreme" => std::time::Duration::from_secs(2),
        _ => DEFAULT_TIMEOUT,
    }
}

/// C10 後、ambient の env / clock / filesystem に依存する fixture は safe 既定では動かない。
/// これらは `--profile legacy`（env/clock/stdin/stdout/exit grant + TSUMUGI_SANDBOX 翻訳 fs）で
/// 実行する。純粋な言語挙動 fixture は safe 既定（stdout のみ）のまま。
pub fn fixture_needs_legacy(name: &str) -> bool {
    matches!(
        name,
        "file_io" | "filesystem" | "string_utils" | "local_utils" | "error_sandbox"
    )
}

/// import を含む fixture（C6-c で tree 経路、E9 で VM 経路が Engine resolver gate を通る）。
///
/// C6-c 以降、**tree 経路の file import は `module_resolver` capability が必須**になった（Q2 =
/// Denied all-in、設計 §4.5/§8）。E9 で VM file/stdin も link 層 capability gate（Engine resolver）を
/// 前置するようになり、未 grant + import は tree と同じく terminal `Denied` になる。これらの fixture は
/// bare-path import（`import "import_lib.tsg"`）を使い、fixtures ディレクトリ内の兄弟ファイルを
/// import するため、tree/VM 両 arm とも `--allow-import-root default=<fixtures_dir>` を付けて
/// `default` mount へ routing する（bare → `@default/...` 正規化）。従来あった「VM arm は import root
/// 不要」という tree/VM 既知差は E9 で解消された。
pub fn fixture_has_import(name: &str) -> bool {
    matches!(
        name,
        "import_basic"
            | "import_nested"
            | "import_static_resolution"
            | "import_circular"
            | "function_id_import"
            | "runtime_global_import"
            | "error_import_not_found"
            | "error_import_before_side_effects"
    )
}

/// `--vm` が stderr の先頭に出す experimental の警告（`src/cli_capability.rs` の `WARN_VM_EXPERIMENTAL`、
/// fix-cli-vm-experimental-warning、CVW-AT-03）。
pub const WARN_VM_EXPERIMENTAL: &str =
    "警告: --vm は experimental な VM backend を選びます。規範の backend は tree-walk です";

/// stderr を文字列にし、`WARN_VM_EXPERIMENTAL` の 1 行を除く。残りに対して、各 test は今までと同じ
/// 確認（空・完全一致・tree との比較）を行う。警告の有無そのものは `tests/cli_vm_warning.rs` が確かめる。
pub fn stderr_without_vm_warning(raw: &[u8]) -> String {
    let text = String::from_utf8_lossy(raw).replace("\r\n", "\n");
    let mut removed = false;
    text.split_inclusive('\n')
        .filter(|line| {
            let hit = !removed && line.trim_end_matches('\n') == WARN_VM_EXPERIMENTAL;
            removed |= hit;
            !hit
        })
        .collect()
}

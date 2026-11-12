//! threat-model の受入試験（TM-AT）とテストの対応表。
//!
//! [`docs/threat-model.md`] 第 11 節「Phase別受入試験」の TM-AT ごとに、それを確かめている
//! 既存のテストを並べる。多くのテストは capability-model の CAP-AT や embedding-api の EMB-AT など
//! 別の受入 ID で書かれているため、TM-AT からはテストを辿れなかった。この表がその橋渡しになる。
//!
//! この表は状態の正本ではない。どの TM-AT が満たされたかは各 spec の逸脱一覧とロードマップが持つ。
//! ここに書くのは「どのテストがその TM-AT の観点を確かめているか」と、テストが無い・一部しか
//! 無い観点のメモだけである。
//!
//! 次の 2 つを機械的に確かめ、表が腐らないようにする。
//!
//! 1. 第 11 節に出てくる TM-AT の ID と、この表の ID が過不足なく一致する
//! 2. 表に書いたテスト関数が、書いたファイルに実在する
//!
//! 第 11 節に TM-AT を足したら、ここにも 1 行足す（テストが無ければ `Gap` で理由を書く）。

use std::path::Path;

/// TM-AT ごとの対応の程度。
enum Coverage {
    /// 受入試験の観点をひととおり確かめるテストがある。
    Covered,
    /// 一部の観点だけテストがある。足りない観点を書く。
    Partial(&'static str),
    /// 確かめるテストが無い。理由（未実装の Phase、文書だけの項目など）を書く。
    Gap(&'static str),
}

/// `(TM-AT の ID, 対応の程度, 確かめているテスト "ファイル::関数名")`。
type Row = (&'static str, Coverage, &'static [&'static str]);

const TABLE: &[Row] = &[
    // ---- Phase 0 ----
    (
        "TM-AT-07",
        Coverage::Covered,
        &[
            "tests/depth_boundary.rs::dbt_at_01_expression_nesting_boundary_tree_and_vm",
            "tests/depth_boundary.rs::dbt_at_01_block_nesting_boundary_tree_and_vm",
            "tests/depth_boundary.rs::dbt_at_02_import_depth_boundary_tree_and_vm",
            "tests/integration.rs::user_call_depth_boundary_is_identical_across_engines",
            "tests/integration.rs::user_call_depth_limit_applies_to_all_call_forms",
            "tests/value_nesting_depth_limit.rs::vnd_at_01_boundary_and_catch_in_both_engines",
            "tests/value_nesting_depth_limit.rs::vnd_at_01_literal_checks_each_element_in_both_engines",
            "tests/value_nesting_depth_limit.rs::vnd_at_02_repro_does_not_abort",
        ],
    ),
    (
        "TM-AT-12",
        Coverage::Partial(
            "host 部分（dependency lock・review・OS 隔離が host 責任と明記されること）は文書の項目で、テストの対象外",
        ),
        &[
            "src/embedding.rs::compile_produces_source_hash_over_raw_bytes",
            "src/embedding.rs::source_hash_changes_with_one_byte_diff",
            "src/embedding.rs::import_graph_is_populated_and_hash_deterministic",
            "src/embedding.rs::golden_hashes_are_stable",
        ],
    ),
    // ---- Phase 1 ----
    (
        "TM-AT-15",
        Coverage::Covered,
        &[
            "src/embedding.rs::e6_run_panic_maps_to_internal_failure_and_poisons",
            "src/embedding.rs::e6_catch_host_unwind_captures_panic_as_fault_id",
            "src/embedding.rs::c7_exit_boundary_codes_are_exited",
            "src/embedding.rs::c7_exit_granted_maps_to_exited",
        ],
    ),
    (
        "TM-AT-17",
        Coverage::Partial(
            "security 関連の golden を tree / VM で比べるテストは個別にあるが、「security 関連」の一覧を決めて全部を比べる仕組みは無い",
        ),
        &[
            "tests/integration.rs::import_outside_sandbox_is_blocked_in_both_engines",
            "tests/canonical_error_inventory.rs::tree_and_vm_produce_identical_canonical_errors",
            "tests/differential.rs::cap_at_17_source_count_charge_parity",
        ],
    ),
    (
        "TM-AT-20",
        Coverage::Covered,
        &[
            "tests/engine_api.rs::context_reuse_preserves_bindings_without_leaking_between_contexts",
            "tests/engine_api.rs::args_is_empty_without_injection",
            "tests/engine_api.rs::args_returns_injected_snapshot",
            "src/embedding.rs::c3_env_snapshot_is_fixed_and_isolated_from_process_env",
            "src/embedding.rs::e11_reset_budget_isolates_fuel_between_runs",
        ],
    ),
    (
        "TM-AT-23",
        Coverage::Covered,
        &[
            "src/embedding.rs::debug_output_does_not_leak_injected_secret",
            "src/embedding.rs::compile_diagnostic_debug_is_secret_free",
            "src/embedding.rs::e6_internal_fault_message_is_secret_free",
            "src/embedding.rs::e11_execution_request_debug_is_secret_free",
            "src/capability.rs::debug_is_secret_free",
            "src/vm.rs::existence_oracle_denied_and_missing_are_indistinguishable",
        ],
    ),
    (
        "TM-AT-24",
        Coverage::Covered,
        &[
            "src/embedding.rs::cap_at_16_import_with_resolver_resolves_at_link",
            "src/embedding.rs::emb_at_05_run_does_not_call_resolver",
            "src/embedding.rs::ilg_at_02a_run_uses_in_memory_module_without_resolver_calls",
        ],
    ),
    // ---- Phase 2 ----
    (
        "TM-AT-01",
        Coverage::Covered,
        &[
            "src/capability.rs::empty_denies_all_authorities",
            "src/embedding.rs::c3_env_ungranted_is_runtime_error",
            "src/embedding.rs::c3_env_ungranted_is_catchable",
            "src/embedding.rs::c3_now_ungranted_is_runtime_error",
            "src/embedding.rs::c4_input_ungranted_is_runtime_error",
            "src/embedding.rs::c4_print_ungranted_is_runtime_error",
            "src/embedding.rs::c7_exit_ungranted_is_runtime_error",
            "src/eval.rs::registered_but_ungranted_is_capability_error_callback_zero",
            "src/embedding.rs::cap_at_16_import_without_resolver_is_denied_call_0",
        ],
    ),
    (
        "TM-AT-02",
        Coverage::Covered,
        &[
            "src/capability.rs::fs_target_parse_rejects_lexical_errors",
            "src/capability.rs::fs_target_parse_rejects_colon_component_ads",
            "src/capability.rs::duplicate_mount_rejected",
            "src/vm.rs::lexical_path_error_is_argument",
        ],
    ),
    (
        "TM-AT-03",
        Coverage::Partial(
            "dangling symlink と create / write / append を組み合わせた deterministic fake adapter の試験が見当たらない（実 OS の adapter での試験が中心）",
        ),
        &[
            "src/capability.rs::intermediate_symlink_escaping_root_denied",
            "src/capability.rs::intermediate_symlink_denied_under_deny_all",
            "src/capability.rs::deny_all_rejects_final_symlink_open",
            "src/capability.rs::symlink_escaping_root_is_denied_under_deny_all",
            "src/capability.rs::rename_src_symlink_is_rejected",
            "src/capability.rs::remove_final_symlink_does_not_touch_target",
        ],
    ),
    (
        "TM-AT-04",
        Coverage::Covered,
        &["src/vm.rs::existence_oracle_denied_and_missing_are_indistinguishable"],
    ),
    (
        "TM-AT-09",
        Coverage::Gap(
            "secret snapshot を読める実行で、grant されていない stdout / file / host function へ流せないことを 1 本で確かめるテストが無い（個々の capability の拒否と Debug の secret 無しは TM-AT-01・TM-AT-23 の側にある）",
        ),
        &[],
    ),
    (
        "TM-AT-10",
        Coverage::Partial(
            "metadata 部分（Phase 2）は確かめている。Phase 6 の sink event 側は audit_redaction.rs にあるが、host function の値の redaction は fs builtin の範囲だけ",
        ),
        &[
            "src/host_function.rs::redact_produces_no_value_body",
            "tests/audit_redaction.rs::r_at_03_body_is_omitted_with_original_bytes",
            "tests/audit_redaction.rs::r_at_09_terminal_error_has_no_script_visible_message",
        ],
    ),
    (
        "TM-AT-13",
        Coverage::Partial(
            "grant / arity 部分（Phase 2）だけ。host call の cost N-1 / N / N+1（Phase 3）は未実装",
        ),
        &[
            "src/eval.rs::granted_call_invokes_callback",
            "src/eval.rs::registered_but_ungranted_is_capability_error_callback_zero",
            "src/eval.rs::arity_mismatch_is_argument_error_callback_zero",
            "src/eval.rs::unknown_name_is_normal_name_error",
        ],
    ),
    (
        "TM-AT-14",
        Coverage::Partial(
            "unwind する host function の panic は確かめている。`panic=abort` の非保証は文書の項目",
        ),
        &["src/embedding.rs::e6_catch_host_unwind_captures_panic_as_fault_id"],
    ),
    (
        "TM-AT-16",
        Coverage::Covered,
        &[
            "src/embedding.rs::c3_env_ungranted_is_catchable",
            "src/embedding.rs::c4_print_ungranted_is_catchable",
            "src/eval.rs::host_failure_is_catchable_host_error",
            "src/embedding.rs::c7_granted_exit_is_uncatchable",
            "src/embedding.rs::e11_budget_exceeded_is_uncatchable",
            "src/embedding.rs::e11_deadline_is_uncatchable",
            "src/embedding.rs::e11_runtime_cancel_is_uncatchable",
            "tests/integration.rs::f1_fuel_limit_is_not_catchable_in_both_engines",
        ],
    ),
    (
        "TM-AT-21",
        Coverage::Partial(
            "clone の同一性は確かめている。start 後に元の builder を変えても set が変わらないことを直接確かめるテストは無い（revoke API が無いことは型で保証）",
        ),
        &["src/capability.rs::clone_has_equal_id_and_permissions"],
    ),
    (
        "TM-AT-22",
        Coverage::Covered,
        &[
            "tests/builtin_registry_contract.rs::vm_compiler_accepts_every_public_builtin",
            "tests/builtin_registry_contract.rs::tree_engine_resolves_every_public_builtin",
            "tests/builtin_registry_contract.rs::generated_docs_match_registry",
            "tests/builtin_registry_contract.rs::public_names_have_no_duplicates",
        ],
    ),
    // ---- Phase 3・6・7 ----
    (
        "TM-AT-05",
        Coverage::Partial(
            "fuel の境界は代表的な経路だけ。全 loop / function / callback / collection builtin / host call を N-1 / N / N+1 で回す網羅は無い",
        ),
        &[
            "tests/differential.rs::budget_boundary_fuel_exact_limit_in_process",
            "tests/integration.rs::f1_fuel_limit_is_not_catchable_in_both_engines",
        ],
    ),
    (
        "TM-AT-06",
        Coverage::Partial(
            "heap の境界は確かめている。OS の memory 上限の下での隔離 process の試験は無い",
        ),
        &[
            "src/budget.rs::heap_charge_at_limit_succeeds_and_plus_one_exceeds",
            "tests/integration.rs::heap_accumulation_trips_limit_in_both_engines",
            "tests/differential.rs::budget_boundary_heap_subprocess",
        ],
    ),
    (
        "TM-AT-08",
        Coverage::Partial(
            "deadline は確かめている。blocking する native callback を隔離 process の timeout で止める試験は無い",
        ),
        &[
            "src/eval.rs::deadline_stops_at_exact_deadline",
            "src/eval.rs::deadline_allows_execution_just_before_deadline",
            "src/embedding.rs::e11_deadline_exceeded_is_terminal",
        ],
    ),
    (
        "TM-AT-11",
        Coverage::Covered,
        &[
            "tests/audit_core.rs::with_sink_emits_started_then_terminal_in_order",
            "tests/audit_core.rs::failing_sink_is_fail_closed_and_not_success",
            "tests/audit_core.rs::started_wrong_sequence_ack_is_fail_closed_before_script_work",
        ],
    ),
    (
        "TM-AT-18",
        Coverage::Partial(
            "stdout の境界は確かめている。file と host result の N-1 / N / N+1 は未実装（capability-model の逸脱一覧）",
        ),
        &[
            "tests/print_newline_charge.rs::pnc_at_01_output_limit_boundary_includes_newline",
            "tests/integration.rs::output_bytes_limit_gates_print_in_both_engines",
            "tests/differential.rs::budget_boundary_output_calls_subprocess",
        ],
    ),
    (
        "TM-AT-19",
        Coverage::Partial(
            "module の本数は確かめている。import source の byte の N 境界は未実装（capability-model の逸脱一覧）",
        ),
        &[
            "tests/integration.rs::cap_at_17_source_count_boundary_tree",
            "tests/integration.rs::cap_at_17_import_count_boundary_tree",
            "tests/differential.rs::cap_at_17_source_count_charge_parity",
        ],
    ),
];

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// threat-model 第 11 節に出てくる TM-AT の ID を、重複を除いて出現順に返す。
fn ids_in_section_11() -> Vec<String> {
    let text = std::fs::read_to_string(root().join("docs/threat-model.md"))
        .expect("docs/threat-model.md を読めません");
    let start = text
        .find("## 11. Phase別受入試験")
        .expect("第 11 節の見出しが見つかりません");
    let rest = &text[start..];
    let end = rest[3..].find("\n## ").map_or(rest.len(), |i| i + 3);
    let section = &rest[..end];
    let mut ids = Vec::new();
    let mut search = section;
    while let Some(at) = search.find("TM-AT-") {
        let digits: String = search[at + 6..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if !digits.is_empty() {
            let id = format!("TM-AT-{digits}");
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
        search = &search[at + 6..];
    }
    ids
}

#[test]
fn table_lists_every_acceptance_id_in_section_11_exactly_once() {
    let documented = ids_in_section_11();
    assert!(
        documented.len() >= 20,
        "第 11 節から TM-AT を読めていません: {documented:?}"
    );
    let mut listed: Vec<&str> = TABLE.iter().map(|(id, _, _)| *id).collect();
    for id in &documented {
        assert!(
            listed.contains(&id.as_str()),
            "{id} が対応表にありません。テストが無ければ Coverage::Gap で理由を書いてください"
        );
    }
    for id in &listed {
        assert!(
            documented.iter().any(|d| d == id),
            "{id} は threat-model 第 11 節にありません"
        );
    }
    listed.sort_unstable();
    let before = listed.len();
    listed.dedup();
    assert_eq!(before, listed.len(), "対応表に同じ ID が 2 回あります");
}

#[test]
fn every_listed_test_exists() {
    for (id, coverage, tests) in TABLE {
        match coverage {
            Coverage::Covered => {
                assert!(!tests.is_empty(), "{id}: Covered なのにテストがありません")
            }
            Coverage::Partial(note) => {
                assert!(!tests.is_empty(), "{id}: Partial なのにテストがありません");
                assert!(!note.is_empty(), "{id}: 足りない観点を書いてください");
            }
            Coverage::Gap(reason) => {
                assert!(tests.is_empty(), "{id}: Gap なのにテストがあります");
                assert!(!reason.is_empty(), "{id}: 理由を書いてください");
            }
        }
        for entry in *tests {
            let (file, name) = entry
                .split_once("::")
                .unwrap_or_else(|| panic!("{id}: `ファイル::関数名` の形ではありません: {entry}"));
            let source = std::fs::read_to_string(root().join(file))
                .unwrap_or_else(|error| panic!("{id}: {file} を読めません: {error}"));
            assert!(
                source.contains(&format!("fn {name}(")),
                "{id}: {file} にテスト関数 {name} がありません（名前が変わったら表も直してください）"
            );
        }
    }
}

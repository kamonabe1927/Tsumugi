//! `for` の反復列の共有・課金（fix-for-iter-snapshot-charge、execution-control §5.2・§5.3）。
//!
//! - FIS-AT-01 の stdout・exit code（subprocess、両 backend）。確保量は `tests/scaling.rs`。
//! - FIS-AT-03: Dict・Str の反復列の課金（string の累積は tree と VM で完全一致、live / peak heap は
//!   engine ごとに List を `for` する対照の script との差で見る）。
//! - FIS-AT-04: 上限で止まる（HeapBytes は engine ごと、StringAllocations・collection の上限は exact 突合）。
//!
//! FIS-AT-02（反復の意味が変わらない）は golden fixture `for_iter_shared`（`tests/integration.rs`）。

use std::process::Command;

use tsumugi::budget::{BudgetConfig, BudgetUsage};
use tsumugi::compiler::Compiler;
use tsumugi::error::{ErrorKind, TsumugiError};
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::vm::Vm;

fn run_tree(source: &str, config: BudgetConfig) -> (BudgetUsage, Result<(), TsumugiError>) {
    let tokens = Lexer::new(source).tokenize();
    let program = Parser::new(tokens).parse().expect("パースに失敗");
    let mut evaluator = Evaluator::with_budget(config);
    let result = evaluator.run(&program, source.len() as u64);
    (evaluator.budget_usage(), result)
}

fn run_vm(source: &str, config: BudgetConfig) -> (BudgetUsage, Result<(), TsumugiError>) {
    let tokens = Lexer::new(source).tokenize();
    let program = Parser::new(tokens).parse().expect("パースに失敗");
    let chunk = Compiler::new().compile(&program).expect("コンパイルに失敗");
    let mut vm = Vm::with_budget(chunk, config);
    vm.charge_link(source.len() as u64, &[])
        .expect("charge_link に失敗");
    let result = vm.run();
    (vm.budget_usage(), result)
}

fn config() -> BudgetConfig {
    BudgetConfig::for_legacy(100_000_000, 1_000_000)
}

type Runner = fn(&str, BudgetConfig) -> (BudgetUsage, Result<(), TsumugiError>);
const ENGINES: [(&str, Runner); 2] = [("tree", run_tree), ("vm", run_vm)];

// ---- FIS-AT-01: stdout・exit code -------------------------------------------------------

/// 関数の再帰の各段で同じ List を `for` する script（issue #358 の再現を縮めたもの）。
#[test]
fn fis_at_01_recursive_list_for_completes_in_both_backends() {
    let source = "let xs = range(0, 100000)\n\
                  fn rec(d)\n    if d == 0\n        return 0\n    end\n\
                  \x20   for x in xs\n        rec(d - 1)\n        break\n    end\n    return 0\nend\n\
                  rec(120)\nprint(\"done\")\n";
    let dir = std::env::temp_dir().join(format!("fis_at_01_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("rec.tsg");
    std::fs::write(&script, source).expect("write script");
    for use_vm in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsumugi"));
        if use_vm {
            command.arg("--vm");
        }
        let output = command.arg(&script).output().expect("起動に失敗");
        let mode = if use_vm { "VM" } else { "tree-walk" };
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "done",
            "{mode}: stdout"
        );
        assert_eq!(output.status.code(), Some(0), "{mode}: exit code");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- FIS-AT-03: Dict・Str の反復列の課金 ----------------------------------------------------

/// 前置き（Dict・Str・整数 List）は `for` の対象の識別子（`dct` / `ctl` など同じ長さ）以外が同一。
fn script(target: &str, prelude_list_len: usize, tail: &str) -> String {
    let ctl = (0..prelude_list_len)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "let dct = {{\"a\": 1, \"bcd\": 2, \"é\": 3, \"日本語\": 4}}\n\
         let stg = \"aé日😀bc\"\n\
         let ctl = [{ctl}]\n\
         let n = 0\n\
         for k in {target}\n    n = n + 1\n{tail}end\n"
    )
}

fn assert_charge(target: &str, elems: &[&str], list_len: usize, tail: &str) {
    let count = elems.len() as u64;
    let bytes: u64 = elems.iter().map(|e| e.len() as u64).sum();
    let body: u64 = 24 + 32 * count + elems.iter().map(|e| 24 + e.len() as u64).sum::<u64>();
    let src = script(target, list_len, tail);
    let ctl_src = script("ctl", list_len, tail);
    let mut string_counters = vec![];
    for (name, run) in ENGINES {
        let (u, r) = run(&src, config());
        r.unwrap_or_else(|e| panic!("{name}: {target}: {e:?}"));
        let (c, r) = run(&ctl_src, config());
        r.unwrap_or_else(|e| panic!("{name}: ctl: {e:?}"));
        assert_eq!(
            u.committed.string_allocations - c.committed.string_allocations,
            count,
            "{name}: {target}{tail}: string_allocations の増分"
        );
        assert_eq!(
            u.committed.string_bytes - c.committed.string_bytes,
            bytes,
            "{name}: {target}{tail}: string_bytes の増分"
        );
        assert_eq!(
            u.peak_heap_bytes - c.peak_heap_bytes,
            body,
            "{name}: {target}{tail}: peak_heap の増分は List body と各要素の String body の合計"
        );
        assert_eq!(
            u.live_heap_bytes, c.live_heap_bytes,
            "{name}: {target}{tail}: ループ後に反復列が release される"
        );
        string_counters.push((u.committed.string_allocations, u.committed.string_bytes));
    }
    assert_eq!(
        string_counters[0], string_counters[1],
        "tree と VM で累積が一致"
    );
}

#[test]
fn fis_at_03_dict_iteration_list_is_charged() {
    let keys = ["a", "bcd", "é", "日本語"];
    // Dict の反復順は key のコードポイント順だが、課金量は順序に依らない。
    assert_charge("dct", &keys, keys.len(), "");
    assert_charge("dct", &keys, keys.len(), "    break\n");
}

#[test]
fn fis_at_03_str_iteration_list_is_charged() {
    let chars = ["a", "é", "日", "😀", "b", "c"];
    assert_charge("stg", &chars, chars.len(), "");
    assert_charge("stg", &chars, chars.len(), "    break\n");
}

// ---- FIS-AT-04: 上限で止まる ------------------------------------------------------------

/// 長い key の Dict・長い Str を、再帰の各段で `for` する script。
fn recursive_source(target: &str, depth: usize) -> String {
    format!(
        "let big = \"ab\"\n\
         for i in range(0, 9)\n    big = big + big\nend\n\
         let dct = {{}}\n\
         dct[big] = 1\n\
         let stg = \"ab\"\n\
         for i in range(0, 5)\n    stg = stg + stg\nend\n\
         fn rec(d)\n    if d == 0\n        return 0\n    end\n\
         \x20   try\n        for k in {target}\n            rec(d - 1)\n            break\n        end\n\
         \x20   catch e\n        return 1\n    end\n    return 0\nend\n\
         rec({depth})\n"
    )
}

#[test]
fn fis_at_04_heap_limit_is_terminal_in_each_engine() {
    // 再帰が深いので、テストスレッドの既定スタックではなく大きなスタックで走らせる。
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(heap_limit_body)
        .expect("spawn")
        .join()
        .expect("heap 上限のテストが panic");
}

fn heap_limit_body() {
    // `dct` の key は 1024 byte、`stg` は 64 文字。1 段ごとに List body + String body が積まれる。
    for target in ["dct", "stg"] {
        for (name, run) in ENGINES {
            let mut limited = config();
            limited.max_live_heap_bytes = 60_000;
            // 同じ上限でも浅い再帰は最後まで終わる（深い再帰で止まるのは反復列の課金による）。
            let (_, ok) = run(&recursive_source(target, 5), limited);
            ok.unwrap_or_else(|e| panic!("{name}: {target}: 浅い再帰は成功する: {e:?}"));
            let (_, r) = run(&recursive_source(target, 100), limited);
            let err = r.expect_err(&format!("{name}: {target}: heap 上限で止まる"));
            assert_eq!(
                err.kind(),
                Some(ErrorKind::HeapLimit),
                "{name}: {target}: try / catch で捕捉されず HeapLimit の terminal: {err:?}"
            );
        }
    }
}

#[test]
fn fis_at_04_string_allocations_limit_matches_across_engines() {
    for target in ["dct", "stg"] {
        let src = script(target, 4, "");
        let mut limited = config();
        // 前置きの文字列（dct の key 4 個・stg）の分を測って、反復列の 3 個目で超える上限にする。
        let (base, _) = run_tree(&script("ctl", 4, ""), config());
        limited.max_string_allocations = base.committed.string_allocations + 2;
        let (tree_usage, tree) = run_tree(&src, limited);
        let (vm_usage, vm) = run_vm(&src, limited);
        let tree = tree.expect_err("tree: StringAllocations で止まる");
        let vm = vm.expect_err("vm: StringAllocations で止まる");
        assert_eq!(
            tree.kind(),
            Some(ErrorKind::StringLimit),
            "{target}: {tree:?}"
        );
        assert_eq!(tree, vm, "{target}: terminal の全 field が一致");
        assert_eq!(
            (
                tree_usage.committed.string_allocations,
                tree_usage.committed.string_bytes
            ),
            (
                vm_usage.committed.string_allocations,
                vm_usage.committed.string_bytes
            ),
            "{target}: 止まった時点の string の累積が一致"
        );
    }
}

#[test]
fn fis_at_04_str_over_collection_limit_fails_before_building_elements() {
    let src = "let s = \"abcdef\"\nfor c in s\n    let x = 1\nend\n";
    let mut limited = config();
    limited.max_collection_elements = 5;
    let (tree_usage, tree) = run_tree(src, limited);
    let (vm_usage, vm) = run_vm(src, limited);
    let tree = tree.expect_err("tree: collection の上限");
    let vm = vm.expect_err("vm: collection の上限");
    assert_eq!(tree.kind(), Some(ErrorKind::CollectionLimit), "{tree:?}");
    assert_eq!(tree, vm, "terminal の全 field が一致");
    // 要素を作る前に検査するので、反復列の文字列は 1 つも累積に入らない。
    assert_eq!(
        tree_usage.committed.string_allocations,
        vm_usage.committed.string_allocations
    );
    assert_eq!(
        tree_usage.committed.string_bytes,
        vm_usage.committed.string_bytes
    );
    assert_eq!(
        tree_usage.committed.string_allocations, 1,
        "前置きの \"abcdef\" だけ"
    );
}

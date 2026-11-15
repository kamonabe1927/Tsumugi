//! `for` の Dict・Str の反復列の要素を、課金してから複製する（fix-for-iter-charge-before-copy、
//! execution-control §5.2・§5.3）。
//!
//! - FCB-AT-01: 課金で止まる要素を複製しない（確保量を数える。tree / VM）。
//! - FCB-AT-03: `max_single_string_bytes` を超える要素の terminal。tree は embedding の
//!   `ExecutionContext` を 2 回の実行で使い回して Dict の `for` で確かめ、VM は context を引き継ぐ
//!   経路が無い（CLI・REPL は 1 つの budget を共有し、key を作る時点で同じ上限に掛かる）ので、
//!   tree・VM 共通の `new_iter_str_list` を直接呼ぶ。
//!
//! FCB-AT-02（成功時の要素・課金が変わらない）と FCB-AT-03 の既存の期待値は
//! `tests/for_iter_charge.rs`（FIS-AT-01..04）・`tests/differential.rs`・golden fixture
//! `for_iter_shared` が変更なしで通ることで確かめる。
//!
//! 確保量はプロセス全体のカウンタで数えるので、測定は `MEASURE_LOCK` で直列化する。

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsumugi::budget::{
    BudgetConfig, BudgetLedger, BudgetResource, BudgetUsage, ControlStop, ExecutionPhase,
};
use tsumugi::compiler::Compiler;
use tsumugi::error::{ErrorKind, TsumugiError};
use tsumugi::eval::Evaluator;
use tsumugi::lexer::Lexer;
use tsumugi::parser::Parser;
use tsumugi::value::Value;
use tsumugi::vm::Vm;

static MEASURE_LOCK: Mutex<()> = Mutex::new(());
static ALLOCATED: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATED.fetch_add(new_size.saturating_sub(layout.size()), Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

type Outcome = (BudgetUsage, Result<(), TsumugiError>, usize);

/// 実行フェーズだけの確保量（parse / compile は含めない）と usage・結果を返す。
fn run(source: &str, config: BudgetConfig, use_vm: bool) -> Outcome {
    let _guard = MEASURE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tokens = Lexer::new(source).tokenize();
    let program = Parser::new(tokens).parse().expect("パースに失敗");
    if use_vm {
        let chunk = Compiler::new().compile(&program).expect("コンパイルに失敗");
        let mut vm = Vm::with_budget(chunk, config);
        vm.charge_link(source.len() as u64, &[])
            .expect("charge_link に失敗");
        let before = ALLOCATED.load(Ordering::Relaxed);
        let result = vm.run();
        let bytes = ALLOCATED.load(Ordering::Relaxed).saturating_sub(before);
        (vm.budget_usage(), result, bytes)
    } else {
        let mut evaluator = Evaluator::with_budget(config);
        let before = ALLOCATED.load(Ordering::Relaxed);
        let result = evaluator.run(&program, source.len() as u64);
        let bytes = ALLOCATED.load(Ordering::Relaxed).saturating_sub(before);
        (evaluator.budget_usage(), result, bytes)
    }
}

fn config() -> BudgetConfig {
    BudgetConfig::for_legacy(100_000_000, 1_000_000)
}

/// 1 MiB の key を 1 つ持つ Dict を作り、`tail` を実行する script。
fn big_key_script(tail: &str) -> String {
    format!(
        "let big = \"ab\"\n\
         for i in range(0, 19)\n    big = big + big\nend\n\
         let dct = {{}}\n\
         dct[big] = 1\n\
         {tail}"
    )
}

const KEY_LEN: usize = 1 << 20;
const FOR_TAIL: &str = "for k in dct\n    let n = 1\nend\n";

/// 止まる実行が対照の実行より確保した量が key の長さより小さい（key が複製されていない）ことを確かめる。
fn assert_not_copied(limit: impl Fn(&mut BudgetConfig, &BudgetUsage), expected: ErrorKind) {
    for use_vm in [false, true] {
        let name = if use_vm { "vm" } else { "tree" };
        let (base, ok, control_bytes) = run(&big_key_script(""), config(), use_vm);
        ok.unwrap_or_else(|e| panic!("{name}: 対照の script は成功する: {e:?}"));
        let mut limited = config();
        limit(&mut limited, &base);
        let (_, result, stopped_bytes) = run(&big_key_script(FOR_TAIL), limited, use_vm);
        let err = result.expect_err(&format!("{name}: 反復列の課金で止まる"));
        assert_eq!(err.kind(), Some(expected), "{name}: {err:?}");
        let extra = stopped_bytes.saturating_sub(control_bytes);
        assert!(
            extra < KEY_LEN,
            "{name}: 止まる要素の key を複製していない（対照との確保量の差 {extra} < {KEY_LEN}）"
        );
    }
}

#[test]
fn fcb_at_01_string_limit_stops_before_copying_the_key() {
    // 対照の累積に key の半分だけ足した上限にして、反復列の key の課金（§5.3）で止める。
    assert_not_copied(
        |c, base| c.max_string_bytes = base.committed.string_bytes + (KEY_LEN as u64) / 2,
        ErrorKind::StringLimit,
    );
}

#[test]
fn fcb_at_01_heap_limit_stops_before_copying_the_key() {
    // 対照の peak に少しだけ足した上限にして、key の String body の live heap の課金で止める。
    assert_not_copied(
        |c, base| c.max_live_heap_bytes = base.peak_heap_bytes + 1_000,
        ErrorKind::HeapLimit,
    );
}

// ---- FCB-AT-03: max_single_string_bytes ---------------------------------------------------

#[test]
fn fcb_at_03_single_string_limit_reports_the_element_length() {
    let mut config = config();
    config.max_single_string_bytes = 4;
    let mut budget = BudgetLedger::with_config(config);
    let parts = ["abc", "defgh", "i"];
    let stop = match Value::new_iter_str_list(
        parts.iter().copied(),
        parts.len(),
        &mut budget,
        ExecutionPhase::Run,
    ) {
        Err(stop) => stop,
        Ok(_) => panic!("5 byte の要素で止まる"),
    };
    match stop {
        ControlStop::BudgetExceeded(e) => {
            assert_eq!(e.resource, BudgetResource::SingleStringBytes);
            assert_eq!(e.requested, 5, "requested は要素の byte 長");
            assert_eq!(e.limit, 4);
        }
        other => panic!("BudgetExceeded を期待: {other:?}"),
    }
}

/// tree: embedding の `ExecutionContext` を 2 回の実行で使い回す。1 回目（既定の budget）で長い key の
/// Dict を作り、2 回目（`max_single_string_bytes` を key より小さくした budget）で `for` する。
/// key を作る時点の上限に掛からずに、反復列の要素（`materialize_for_items` → `new_iter_str_list`）の
/// 検査だけで `SingleStringBytes` の terminal（`requested` = key の byte 長）になる。
/// VM は context を 2 回の実行で引き継ぐ経路が無い（CLI・REPL は 1 つの budget を共有する）ので、
/// 上の関数を直接呼ぶテストで代える。
#[test]
fn fcb_at_03_dict_for_stops_with_single_string_bytes_in_tree() {
    use std::num::NonZeroU128;
    use std::sync::Arc;
    use tsumugi::embedding::{Engine, ExecutionContext, ExecutionOutcome, ExecutionRequest};
    use tsumugi::{
        CapabilitySet, CompileOptions, ExecutionId, LinkRequest, MonotonicClock, Source, SourceId,
        SystemMonotonicClock,
    };

    let engine = Engine::builder().build().expect("engine");
    let clock: Arc<dyn MonotonicClock> = Arc::new(SystemMonotonicClock::new());
    let mut context = ExecutionContext::new(&engine);
    let mut run_script = |text: &str, id: u128, max_single: Option<u64>| {
        let script = engine
            .compile(
                Source::new(SourceId::new("main").expect("id"), text),
                &CompileOptions {
                    retain_source: true,
                },
            )
            .expect("compile");
        let link_budget = BudgetConfig::standard(clock.as_ref()).expect("budget");
        let linked = engine
            .link(
                &script,
                LinkRequest::new(
                    ExecutionId::new(NonZeroU128::new(id).expect("non-zero")),
                    CapabilitySet::empty(),
                    link_budget,
                ),
            )
            .expect("link");
        let mut budget = BudgetConfig::standard(clock.as_ref()).expect("budget");
        if let Some(limit) = max_single {
            budget.max_single_string_bytes = limit;
        }
        let request = ExecutionRequest::new(budget, Arc::clone(&clock)).expect("request");
        engine.run(&linked, &mut context, request)
    };

    let first = run_script("let d = {\"abcdefgh\": 1}\n", 1, None);
    assert!(
        matches!(first, ExecutionOutcome::Completed { .. }),
        "{first:?}"
    );
    let second = run_script("let n = 0\nfor k in d\n    n = n + 1\nend\n", 2, Some(4));
    match second {
        ExecutionOutcome::BudgetExceeded { error, .. } => {
            assert_eq!(error.code, ErrorKind::StringLimit);
            assert_eq!(error.line, Some(2), "止まる位置は `for` の行");
            assert_eq!(
                error.safe_message, "文字列の長さが上限を超えました: 8 バイト (上限: 4 バイト)",
                "SingleStringBytes（requested = key の byte 長 8、limit = 4）"
            );
        }
        other => panic!("SingleStringBytes の terminal を期待: {other:?}"),
    }
}

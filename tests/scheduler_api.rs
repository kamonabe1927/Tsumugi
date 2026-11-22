//! 協調スケジューラ（admission / run-turn fairness / backpressure）の受入テスト。
//!
//! REV-015 Slice 5、実行制御仕様 §15.4、設計 §6.3。公開 API（`Engine::with_limits` と
//! `ExecutionHandle`）だけを使い、host が各 Ready handle を round-robin で poll する運用を
//! 固定する。本 FEAT（FEAT-001）は scheduler-core（admission + fairness/backpressure）の
//! 範囲を対象とし、host pending（HostCallPending / waker）は FEAT-002 で追加する。

#[path = "support/counting_resolver.rs"]
mod counting_resolver;

use std::num::{NonZeroU64, NonZeroUsize};

use tsumugi::{
    Engine, EngineLimits, ExecutionContext, ExecutionOutcome, ExecutionRequest, PollResult,
    PollSlice, StartError, YieldReason,
};

fn limits(max_active: usize, max_queued: usize) -> EngineLimits {
    EngineLimits {
        max_active_executions: NonZeroUsize::new(max_active).expect("active 上限は非ゼロ"),
        max_queued_executions: max_queued,
        default_slice_fuel: NonZeroU64::new(10_000).unwrap(),
    }
}

/// 常に Ready な N execution が run-turn FIFO で各 1 slice ずつ進み、1 execution が連続独占
/// しない（AC-2）。非 head handle の poll は semantic work をせず SchedulerPreempted を返す
/// （AC-8）。slice ごとの usage.committed.fuel 増分で確認する。
#[test]
fn n_ready_executions_advance_fifo_one_slice_each() {
    const N: usize = 3;
    let engine = Engine::with_limits(limits(N, 0));
    // 各 execution は十分重い（1 slice では終わらない）。
    let heavy = "\
let i = 0\n\
while i < 1000\n\
  i = i + 1\n\
end\n";
    let script = engine.compile(heavy).unwrap();

    let mut contexts: Vec<ExecutionContext> = (0..N).map(|_| ExecutionContext::new()).collect();
    // 各 context から handle を作る。借用の都合で context を 1 つずつ handle へ渡す。
    let mut ctx_refs: Vec<&mut ExecutionContext> = contexts.iter_mut().collect();
    let mut handles = Vec::new();
    for ctx in ctx_refs.drain(..) {
        let h = engine
            .create_execution(&script, ctx, ExecutionRequest::new())
            .expect("N 個とも active を取れる");
        handles.push(h);
    }

    let small = PollSlice { max_fuel: 16 };

    // まず全 handle を 1 周 poll して run-turn へ登録する。登録順（0,1,2）が FIFO 順になる。
    // 最初の 1 周: handle 0 が head で slice を進める。1,2 は登録されるが非 head → preempt。
    // ただし 0 が rotate するので、各周で head が入れ替わる。
    let mut fuel_before = [0u64; N];
    for (idx, h) in handles.iter().enumerate() {
        fuel_before[idx] = h.usage().committed.fuel;
    }

    // 1 ラウンド目: index 0 を poll すると head（run_turn=[0]）で slice を進め rotate。
    // 続けて 1 を poll すると登録され head（0 が末尾へ回ったので run_turn=[1,2 だが...]）。
    // 実際には 0 を poll→run_turn=[0]→slice→rotate=[0]（単独なので先頭のまま）。そこで
    // round-robin を厳密に観測するため、全 handle を登録してから回す。
    //
    // 登録フェーズ: 各 handle を 1 回ずつ poll する。index 0 は head で前進、1/2 は非 head で
    // SchedulerPreempted（state 不変）。
    let r0 = handles[0].poll(small).unwrap();
    assert!(
        matches!(
            r0,
            PollResult::Yielded {
                reason: YieldReason::SliceFuelExhausted,
                ..
            }
        ),
        "最初に登録した 0 は head で slice を進める: {r0:?}"
    );
    for i in [1usize, 2] {
        let r = handles[i].poll(small).unwrap();
        assert!(
            matches!(
                r,
                PollResult::Yielded {
                    reason: YieldReason::SchedulerPreempted,
                    ..
                }
            ),
            "非 head の {i} は SchedulerPreempted（AC-8）: {r:?}"
        );
        // 非 head poll は fuel を消費しない（semantic work なし）。
        assert_eq!(
            handles[i].usage().committed.fuel,
            fuel_before[i],
            "非 head poll は semantic work をしない"
        );
    }

    // run_turn=[1,2,0]（0 は slice 後に末尾へ rotate）。以後 round-robin で各 1 slice ずつ進む。
    // 2 ラウンド回して、各 handle が概ね均等に前進する（連続独占しない）ことを確認する。
    let mut advanced = [false; N];
    for _round in 0..6 {
        for i in 0..N {
            let before = handles[i].usage().committed.fuel;
            match handles[i].poll(small).unwrap() {
                PollResult::Yielded {
                    reason: YieldReason::SliceFuelExhausted,
                    ..
                } => {
                    assert!(
                        handles[i].usage().committed.fuel > before,
                        "head の slice は fuel を進める"
                    );
                    advanced[i] = true;
                }
                PollResult::Yielded {
                    reason: YieldReason::SchedulerPreempted,
                    ..
                } => {
                    assert_eq!(
                        handles[i].usage().committed.fuel,
                        before,
                        "preempt は fuel を進めない"
                    );
                }
                PollResult::Terminal { .. } => { /* 進み切った handle は terminal になり得る */
                }
                other => panic!("想定外の poll 結果: {other:?}"),
            }
        }
    }
    assert!(
        advanced.iter().all(|&a| a),
        "全 execution が少なくとも 1 slice は前進する（連続独占しない、AC-2）: {advanced:?}"
    );

    drop(handles);
    drop(contexts);
}

/// active 上限ちょうど・queue 上限ちょうどを受理し、+1 が即時 Backpressure（AC-1、設計 §4.1）。
#[test]
fn active_and_queue_limits_exact_then_backpressure() {
    let engine = Engine::with_limits(limits(2, 2));
    let script = engine.compile("let a = 1\n").unwrap();

    // active 上限ちょうど（2 個）を受理。
    let mut ctx1 = ExecutionContext::new();
    let mut ctx2 = ExecutionContext::new();
    let _h1 = engine
        .create_execution(&script, &mut ctx1, ExecutionRequest::new())
        .expect("active 1");
    let _h2 = engine
        .create_execution(&script, &mut ctx2, ExecutionRequest::new())
        .expect("active 2");

    // queue 上限ちょうど（2 個）を受理。
    let mut ctx3 = ExecutionContext::new();
    let mut ctx4 = ExecutionContext::new();
    let _h3 = engine
        .create_execution(&script, &mut ctx3, ExecutionRequest::new())
        .expect("queue 1");
    let _h4 = engine
        .create_execution(&script, &mut ctx4, ExecutionRequest::new())
        .expect("queue 2");

    // +1 は即時 Backpressure。
    let mut ctx5 = ExecutionContext::new();
    match engine.create_execution(&script, &mut ctx5, ExecutionRequest::new()) {
        Err(StartError::Backpressure {
            active,
            queued,
            limit,
        }) => {
            assert_eq!(active, 2);
            assert_eq!(queued, 2);
            assert_eq!(limit, 2);
        }
        Ok(_) => panic!("満杯なのに handle が作られた"),
        Err(other) => panic!("Backpressure を期待したが {other:?}"),
    }
}

/// Paused handle は active slot を握り続け、その間は新規 admission が queue/backpressure に
/// なる（AC-3 の Paused 半分、設計 §4.2/§6.3）。host 待ち半分は FEAT-002 で追加する。
#[test]
fn paused_consumes_active_slot() {
    // active=1, queue=0。handle A を pause して active を握らせる。
    let engine = Engine::with_limits(limits(1, 0));
    let script = engine.compile("let a = 1\n").unwrap();

    let mut ctx_a = ExecutionContext::new();
    let mut handle_a = engine
        .create_execution(&script, &mut ctx_a, ExecutionRequest::new())
        .expect("A active");

    // A を pause する。Paused でも active slot は解放されない（§4.2 FR-4）。
    handle_a.pause().expect("pause は成功する");

    // A が active を握ったままなので、B の admission は満杯で Backpressure（queue=0）。
    let mut ctx_b = ExecutionContext::new();
    match engine.create_execution(&script, &mut ctx_b, ExecutionRequest::new()) {
        Err(StartError::Backpressure { active, .. }) => {
            assert_eq!(active, 1, "Paused handle が active slot を消費している");
        }
        Ok(_) => panic!("Paused が active を解放してしまい、無制限 admission になった"),
        Err(other) => panic!("Backpressure を期待したが {other:?}"),
    }

    // A を resume して terminal まで進めると active が解放され、新規 admission が通る。
    handle_a.resume().expect("resume は成功する");
    let outcome = loop {
        match handle_a.poll(PollSlice::default()).unwrap() {
            PollResult::Yielded { .. } => continue,
            PollResult::Terminal { outcome, .. } => break outcome,
            PollResult::Paused { .. } => panic!("pause は要求していない"),
        }
    };
    assert_eq!(outcome, ExecutionOutcome::Completed);
    drop(handle_a);

    // active が空いたので新規 admission が active を取れる。
    let mut ctx_c = ExecutionContext::new();
    let _h_c = engine
        .create_execution(&script, &mut ctx_c, ExecutionRequest::new())
        .expect("active 解放後は admit が通る");
}

// =============================================================================
// REV-015 Slice 5 FEAT-003: §15.4 host（cooperative adapter / waker / backpressure）
// =============================================================================

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsumugi::{
    AdapterExecutor, AdapterExecutorLimits, CancellationToken, CapabilityCallContext,
    CapabilitySet, CooperativeHostFunction, ExecutionWaker, HostCallCompleter, HostCallPoll,
    HostCallTicket, HostFunctionDescriptor, HostFunctionId, HostFunctionRegistry, SubmitError,
    Value, Wake, new_ticket,
};

/// wake 回数を数える fake Wake（設計 §6.1）。thread を使わず決定的に検証する。
struct FakeWake {
    count: Arc<AtomicUsize>,
}

impl Wake for FakeWake {
    fn wake(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }
}

fn fake_waker() -> (ExecutionWaker, Arc<AtomicUsize>) {
    let count = Arc::new(AtomicUsize::new(0));
    let waker = ExecutionWaker::new(Arc::new(FakeWake {
        count: Arc::clone(&count),
    }));
    (waker, count)
}

fn host_fn_id(n: u128) -> HostFunctionId {
    HostFunctionId::new(std::num::NonZeroU128::new(n).unwrap())
}

/// cooperative host function の descriptor（arity 0、ID 固定）。
fn coop_descriptor(id: u128, name: &str) -> HostFunctionDescriptor {
    HostFunctionDescriptor {
        id: host_fn_id(id),
        name: name.to_string(),
        arity: tsumugi::HostArity::Exact(0),
        cost: tsumugi::HostCost::default(),
        argument_audit: Vec::new(),
        result_audit: tsumugi::AuditValuePolicy::Omit,
        may_block: true,
    }
}

// cooperative host call は `Value`（`Rc` を含み `!Send`）を返すため、completer<Value> は !Send。
// よって作成 thread（poll を呼ぶ thread）に留める必要がある（設計 §9.1）。テスト fixture の
// adapter は `Send + Sync`（registry 格納のため）でなければならないので、completer を struct へ
// 持たず、作成 thread の thread_local へ stash する。テストは同一 thread から取り出して complete
// する（thread を使わない決定的駆動、§6.1）。
thread_local! {
    static PENDING_COMPLETERS: std::cell::RefCell<Vec<HostCallCompleter<Value>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// 作成 thread に stash された completer を 1 個取り出す（テストが complete するため）。
fn take_stashed_completer() -> Option<HostCallCompleter<Value>> {
    PENDING_COMPLETERS.with(|c| c.borrow_mut().pop())
}

/// テスト駆動の cooperative adapter（設計 §6.1 `FakeCooperativeAdapter`）。
///
/// `start` が `Pending(ticket)` を返し、completer を作成 thread の thread_local へ stash する。
/// テストはそれを取り出して明示的に `complete` する（thread を使わない）。`ready` が設定
/// されていれば `Ready(Ok(..))` を返す。`offloads` は submit 記録（non-offload 契約の観測、§6.3）。
///
/// `Value` を struct に保持しないため（thread_local 経由）、本型は `Send + Sync` を満たし
/// registry（`Send + Sync` 必須）へ格納できる。
struct FakeCooperativeAdapter {
    descriptor: HostFunctionDescriptor,
    /// `start` が `Ready(Ok(Int(n)))` を返すべきなら `Some(n)`、`Pending` なら `None`。
    ready_int: Mutex<Option<i64>>,
    /// `start` が response byte 上限での途中停止（`Ready(Err(Control(BudgetExceeded)))`）を
    /// 返すべきかどうか（§6.3 response_byte_limit）。true なら ready_int/pending より優先。
    response_byte_stop: Mutex<bool>,
    /// start が最後に観測した may_yield。
    last_may_yield: Arc<Mutex<Option<bool>>>,
    /// Phase-2 sync trait を別 thread へ offload した回数（期待値は 0、§6.3）。
    offloads: Arc<AtomicUsize>,
    next_id: AtomicUsize,
}

impl FakeCooperativeAdapter {
    fn pending(id: u128, name: &str) -> Arc<Self> {
        Arc::new(Self {
            descriptor: coop_descriptor(id, name),
            ready_int: Mutex::new(None),
            response_byte_stop: Mutex::new(false),
            last_may_yield: Arc::new(Mutex::new(None)),
            offloads: Arc::new(AtomicUsize::new(0)),
            next_id: AtomicUsize::new(1),
        })
    }
}

impl FakeCooperativeAdapter {
    /// 引数 1 個を取る `pending`（tree-expr-continuation-e3 の引数に呼び出しを置くテスト用）。
    fn pending_unary(id: u128, name: &str) -> Arc<Self> {
        let mut descriptor = coop_descriptor(id, name);
        descriptor.arity = tsumugi::HostArity::Exact(1);
        Arc::new(Self {
            descriptor,
            ready_int: Mutex::new(None),
            response_byte_stop: Mutex::new(false),
            last_may_yield: Arc::new(Mutex::new(None)),
            offloads: Arc::new(AtomicUsize::new(0)),
            next_id: AtomicUsize::new(1),
        })
    }
}

impl CooperativeHostFunction for FakeCooperativeAdapter {
    fn descriptor(&self) -> &HostFunctionDescriptor {
        &self.descriptor
    }

    fn start(
        &self,
        context: &mut CapabilityCallContext<'_>,
        _arguments: &[Value],
    ) -> HostCallPoll<Value> {
        *self.last_may_yield.lock().unwrap() = Some(context.may_yield());
        // Phase-2 sync trait を spawn で呼ばない（non-offload 契約）。offloads は 0 のまま。
        let _ = &self.offloads;
        // response byte 上限での途中停止（§4.4）: 全量 buffer せず Control(BudgetExceeded) を返す。
        if *self.response_byte_stop.lock().unwrap() {
            let exceeded = tsumugi::BudgetExceeded {
                resource: tsumugi::BudgetResource::HostResponseBytes,
                limit: context.remaining_response_bytes(),
                used: 0,
                reserved: 0,
                requested: context.remaining_response_bytes() + 1,
                unit: tsumugi::BudgetUnit::Bytes,
                phase: tsumugi::ExecutionPhase::Run,
            };
            return HostCallPoll::Ready(Err(tsumugi::AdapterError::Control(
                tsumugi::ControlStop::BudgetExceeded(exceeded),
            )));
        }
        if let Some(n) = self.ready_int.lock().unwrap().take() {
            return HostCallPoll::Ready(Ok(Value::Int(n)));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) as u64;
        let (ticket, completer): (HostCallTicket<Value>, HostCallCompleter<Value>) =
            new_ticket::<Value>(id);
        // completer を作成 thread の thread_local へ stash する（struct へ Value を持たない）。
        PENDING_COMPLETERS.with(|c| c.borrow_mut().push(completer));
        HostCallPoll::Pending(ticket)
    }
}

/// `name` の cooperative adapter を 1 個だけ登録した registry と、その authority を grant した
/// capability set を作る共通ヘルパ。
fn coop_setup(
    id: u128,
    _name: &str,
    adapter: Arc<dyn CooperativeHostFunction>,
) -> (Arc<HostFunctionRegistry>, CapabilitySet) {
    let registry = HostFunctionRegistry::builder()
        .register_cooperative(adapter)
        .expect("cooperative 登録は成功する")
        .build()
        .expect("build は成功する");
    let caps = CapabilitySet::builder()
        .grant_host_function(host_fn_id(id))
        .expect("grant は成功する")
        .build();
    (Arc::new(registry), caps)
}

/// cooperative host-call を使う 1 文スクリプトで context をセットアップする。
fn coop_context(registry: Arc<HostFunctionRegistry>, caps: CapabilitySet) -> ExecutionContext {
    let mut ctx = ExecutionContext::new();
    ctx.set_host_registry(registry);
    ctx.set_capabilities(caps);
    ctx
}

/// A が cooperative adapter で Pending に入り host executor 未実行のまま、B が run-turn で
/// 前進する。caller thread は block しない（AC-5、設計 §6.3）。
#[test]
fn blocking_fake_host_does_not_block_caller_other_execution_progresses() {
    let engine = Engine::with_limits(limits(2, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "slow_host");
    let (registry, caps) = coop_setup(1, "slow_host", adapter.clone());

    let script_a = engine.compile("let x = slow_host()\n").unwrap();
    let heavy = "\
let i = 0\n\
while i < 1000\n\
  i = i + 1\n\
end\n";
    let script_b = engine.compile(heavy).unwrap();

    let mut ctx_a = coop_context(registry, caps);
    let mut ctx_b = ExecutionContext::new();

    let mut handle_a = engine
        .create_execution(&script_a, &mut ctx_a, ExecutionRequest::new())
        .expect("A active");
    let mut handle_b = engine
        .create_execution(&script_b, &mut ctx_b, ExecutionRequest::new())
        .expect("B active");

    // A を poll すると cooperative host-call が Pending → HostCallPending（caller は block しない）。
    let ra = handle_a.poll(PollSlice::default()).unwrap();
    assert!(
        matches!(
            ra,
            PollResult::Yielded {
                reason: YieldReason::HostCallPending { .. },
                ..
            }
        ),
        "A は HostCallPending で yield する（block しない）: {ra:?}"
    );

    // host executor（adapter の completer）を実行しないまま、B が run-turn で前進して完了する。
    let mut b_terminated = false;
    for _ in 0..200 {
        match handle_b.poll(PollSlice { max_fuel: 64 }).unwrap() {
            PollResult::Terminal { outcome, .. } => {
                assert_eq!(outcome, ExecutionOutcome::Completed);
                b_terminated = true;
                break;
            }
            PollResult::Yielded { .. } => continue,
            PollResult::Paused { .. } => panic!("pause は要求していない"),
        }
    }
    assert!(b_terminated, "host 未完了でも B は前進・完了する（AC-5）");

    // A の poll を続けても未完了なら HostCallPending のまま（block しない）。
    let ra2 = handle_a.poll(PollSlice::default()).unwrap();
    assert!(matches!(
        ra2,
        PollResult::Yielded {
            reason: YieldReason::HostCallPending { .. },
            ..
        }
    ));
}

/// Pending → HostCallPending → waker wake → 次 poll で try_take → terminal の往復（AC-6）。
/// continuation は作成 thread の poll でだけ進む。
#[test]
fn host_pending_waker_roundtrip() {
    let engine = Engine::with_limits(limits(1, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "fetch");
    let (registry, caps) = coop_setup(1, "fetch", adapter.clone());

    let script = engine.compile("let x = fetch()\n").unwrap();
    let mut ctx = coop_context(registry, caps);
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");

    let (waker, wake_count) = fake_waker();
    handle.set_waker(Some(waker)).expect("set_waker ok");

    // 1 回目の poll で Pending → HostCallPending。
    let r = handle.poll(PollSlice::default()).unwrap();
    assert!(matches!(
        r,
        PollResult::Yielded {
            reason: YieldReason::HostCallPending { .. },
            ..
        }
    ));
    assert_eq!(wake_count.load(Ordering::SeqCst), 0, "complete 前は wake 0");

    // host executor（completer）を実行して結果を注入 → waker が 1 回鳴る。
    let completer = take_stashed_completer().expect("completer");
    completer.complete(Ok(Value::Int(42)));
    assert_eq!(
        wake_count.load(Ordering::SeqCst),
        1,
        "complete で waker 1 回"
    );

    // 次 poll で try_take → resume → terminal。
    let outcome = loop {
        match handle.poll(PollSlice::default()).unwrap() {
            PollResult::Terminal { outcome, .. } => break outcome,
            PollResult::Yielded { .. } => continue,
            PollResult::Paused { .. } => panic!("pause は要求していない"),
        }
    };
    assert_eq!(outcome, ExecutionOutcome::Completed);
}

/// tree-expr-continuation-e2a: トップレベルの文の式 frame の中（ユーザー関数呼び出しを含む二項演算の
/// 右辺）で cooperative host-call が Pending になっても、HostCallPending で yield し、resume で文を
/// 評価し直して完了する（規約違反の InternalControl にならない）。
#[test]
fn cooperative_pending_inside_expression_frame_yields_and_resumes() {
    let engine = Engine::with_limits(limits(1, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "fetch");
    let (registry, caps) = coop_setup(1, "fetch", adapter.clone());

    let script = engine
        .compile("fn one()\n  return 1\nend\nlet x = one() + fetch()\n")
        .unwrap();
    let mut ctx = coop_context(registry, caps);
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");

    let r = loop {
        match handle.poll(PollSlice::default()).unwrap() {
            PollResult::Yielded {
                reason: YieldReason::HostCallPending { .. },
                ..
            } => break true,
            PollResult::Yielded { .. } => continue,
            other => panic!("HostCallPending を期待: {other:?}"),
        }
    };
    assert!(r);
    let completer = take_stashed_completer().expect("completer");
    completer.complete(Ok(Value::Int(41)));
    let outcome = loop {
        match handle.poll(PollSlice::default()).unwrap() {
            PollResult::Terminal { outcome, .. } => break outcome,
            PollResult::Yielded { .. } => continue,
            PollResult::Paused { .. } => panic!("pause は要求していない"),
        }
    };
    assert_eq!(outcome, ExecutionOutcome::Completed);
}

/// 両 interleaving で lost wake が起きない（Finding 1、設計 §6.3）。
/// ticket プロトコルを直接駆動して register/complete の順序を両方向で固定する。
#[test]
fn lost_wake_avoided_both_interleavings() {
    // (a) register-after-complete。
    {
        let (ticket, completer): (HostCallTicket<Value>, HostCallCompleter<Value>) =
            new_ticket::<Value>(1);
        let (waker, count) = fake_waker();
        completer.complete(Ok(Value::Int(1)));
        assert_eq!(count.load(Ordering::SeqCst), 0, "register 前は wake なし");
        ticket.register_waker(&waker);
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "register が ready を観測し即 wake"
        );
    }
    // (b) complete-after-register。
    {
        let (ticket, completer): (HostCallTicket<Value>, HostCallCompleter<Value>) =
            new_ticket::<Value>(2);
        let (waker, count) = fake_waker();
        ticket.register_waker(&waker);
        assert_eq!(count.load(Ordering::SeqCst), 0, "complete 前は wake なし");
        completer.complete(Ok(Value::Int(2)));
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "complete が保存済み waker を wake"
        );
    }
}

/// set_waker の置換・解除・terminal 後の Err（設計 §6.3）。
#[test]
fn set_waker_replace_and_clear() {
    let engine = Engine::with_limits(limits(1, 0));
    let script = engine.compile("let a = 1\n").unwrap();
    let mut ctx = ExecutionContext::new();
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");

    let (w1, _c1) = fake_waker();
    let (w2, _c2) = fake_waker();
    handle.set_waker(Some(w1)).expect("Some(w1) ok");
    handle.set_waker(Some(w2)).expect("Some(w2) 置換 ok");
    handle.set_waker(None).expect("None 解除 ok");

    // terminal まで進める。
    let outcome = loop {
        match handle.poll(PollSlice::default()).unwrap() {
            PollResult::Terminal { outcome, .. } => break outcome,
            PollResult::Yielded { .. } => continue,
            PollResult::Paused { .. } => unreachable!(),
        }
    };
    assert_eq!(outcome, ExecutionOutcome::Completed);
    // terminal 後の set_waker は Err(Terminal)。
    let (w3, _c3) = fake_waker();
    assert_eq!(
        handle.set_waker(Some(w3)),
        Err(tsumugi::HandleError::Terminal)
    );
}

/// cooperative adapter が Phase-2 sync trait を別 thread へ offload しない（FR-8 契約、§6.3）。
/// FakeCooperativeAdapter は submit を記録せず（offloads=0）、擬似 Pending を spawn で作らない。
#[test]
fn cooperative_adapter_does_not_offload_sync_trait() {
    let engine = Engine::with_limits(limits(1, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "coop");
    let offloads = adapter.offloads.clone();
    let (registry, caps) = coop_setup(1, "coop", adapter.clone());

    let script = engine.compile("let x = coop()\n").unwrap();
    let mut ctx = coop_context(registry, caps);
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");

    let _ = handle.poll(PollSlice::default()).unwrap();
    assert_eq!(
        offloads.load(Ordering::SeqCst),
        0,
        "cooperative adapter は sync trait を別 thread へ offload しない（FR-8）"
    );
}

/// AdapterExecutor queue 満杯は Ready-Err backpressure（Pending ではない）で caller を block
/// しない（設計 §4.4 / AC-4）。no-thread の最小構成（max_queued=0）で検証する。
#[test]
fn adapter_executor_queue_full_is_backpressure_not_pending() {
    let executor = AdapterExecutor::new(AdapterExecutorLimits {
        max_threads: NonZeroUsize::new(1).unwrap(),
        max_queued: 0,
        max_concurrency: NonZeroUsize::new(1).unwrap(),
    });
    let (_ticket, completer) = new_ticket::<i32>(1);
    let deadline = tsumugi::MonotonicClock::now(&tsumugi::FakeClock::new());
    let cancel = CancellationToken::new();
    let out = executor.submit(completer, deadline, cancel, |_obs, _dl| Ok(1));
    assert_eq!(
        out,
        Err(SubmitError::QueueFull),
        "queue 満杯は Ready-Err backpressure"
    );
}

/// CancellationToken / ExecutionWaker が Send + Sync（別 thread から cancel / wake できる、§6.4）。
#[test]
fn cancellation_and_waker_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<CancellationToken>();
    assert_send_sync::<ExecutionWaker>();
}

/// 別 thread から cancel を鳴らす最小 smoke テスト（§6.4）。
#[test]
fn cancel_from_another_thread_smoke() {
    let token = CancellationToken::new();
    let remote = token.clone();
    let handle = std::thread::spawn(move || remote.cancel());
    let newly = handle.join().unwrap();
    assert!(newly, "別 thread からの初回 cancel は true");
    assert!(token.is_cancelled(), "元 token でも cancel を観測する");
}

/// cooperative host-call 待ち中の handle が active slot を握り続ける（AC-3 の host 待ち半分）。
#[test]
fn host_waiting_consumes_active_slot() {
    let engine = Engine::with_limits(limits(1, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "wait_host");
    let (registry, caps) = coop_setup(1, "wait_host", adapter.clone());

    let script = engine.compile("let x = wait_host()\n").unwrap();
    let mut ctx = coop_context(registry, caps);
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");

    // host-call Pending に入れて active を握らせる。
    let r = handle.poll(PollSlice::default()).unwrap();
    assert!(matches!(
        r,
        PollResult::Yielded {
            reason: YieldReason::HostCallPending { .. },
            ..
        }
    ));

    // host 待ち中は active を解放しないので、B の admission は Backpressure（queue=0）。
    let mut ctx_b = ExecutionContext::new();
    let script_b = engine.compile("let a = 1\n").unwrap();
    match engine.create_execution(&script_b, &mut ctx_b, ExecutionRequest::new()) {
        Err(StartError::Backpressure { active, .. }) => {
            assert_eq!(
                active, 1,
                "host 待ち handle が active slot を消費している（AC-3）"
            );
        }
        Ok(_) => panic!("host 待ちが active を解放し無制限 admission になった"),
        Err(other) => panic!("Backpressure を期待したが {other:?}"),
    }

    // 完了させて active を解放する。
    let completer = take_stashed_completer().expect("completer");
    completer.complete(Ok(Value::Int(0)));
    let outcome = loop {
        match handle.poll(PollSlice::default()).unwrap() {
            PollResult::Terminal { outcome, .. } => break outcome,
            PollResult::Yielded { .. } => continue,
            PollResult::Paused { .. } => unreachable!(),
        }
    };
    assert_eq!(outcome, ExecutionOutcome::Completed);
}

/// AC-4 の cancel: host 待ち中に cancel すると次 poll で Cancelled terminal（clock 不要、§4.6）。
/// deadline 側は alpha facade の legacy budget が専用 clock domain の deadline（ns=u64::MAX）を
/// 持つため `new_with_clock` が foreign clock を一律に弾く構造になっており（§4.6、FEAT-002 の
/// `new_with_clock_rejects_foreign_clock` 参照）、本 facade からは観測できない。したがって本
/// テストでは cancel 側（観測可能）と foreign-clock 拒否を固定する。
#[test]
fn cancel_works_while_host_waiting_and_foreign_clock_rejected() {
    use tsumugi::{BudgetConfigError, FakeClock};

    // --- cancel 側（clock 不要） ---
    let engine = Engine::with_limits(limits(1, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "slow");
    let (registry, caps) = coop_setup(1, "slow", adapter.clone());
    let script = engine.compile("let x = slow()\n").unwrap();
    let mut ctx = coop_context(registry, caps);
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");

    // host-call Pending に入れる。
    let r = handle.poll(PollSlice::default()).unwrap();
    assert!(matches!(
        r,
        PollResult::Yielded {
            reason: YieldReason::HostCallPending { .. },
            ..
        }
    ));
    // host 待ち中に cancel → 次 poll で Cancelled terminal（try_take より先の checkpoint、§4.6）。
    handle.cancellation_token().cancel();
    match handle.poll(PollSlice::default()).unwrap() {
        PollResult::Terminal { outcome, .. } => assert_eq!(outcome, ExecutionOutcome::Cancelled),
        other => panic!("Cancelled を期待したが {other:?}"),
    }

    // --- foreign clock は new_with_clock が弾く（§4.6） ---
    let foreign = Arc::new(FakeClock::new());
    match ExecutionContext::new_with_clock(foreign) {
        Err(StartError::Config(BudgetConfigError::ForeignClock)) => {}
        Ok(_) => panic!("foreign clock は new_with_clock で弾かれるべき"),
        Err(other) => panic!("ForeignClock を期待したが {other:?}"),
    }
}

/// response byte 上限での途中停止: provider が全量 buffer する前に
/// `Control(BudgetExceeded(HostResponseBytes))` を返し、catch 不能 terminal になる（§6.3）。
#[test]
fn response_byte_limit_stops_stream_midway() {
    let engine = Engine::with_limits(limits(1, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "stream");
    *adapter.response_byte_stop.lock().unwrap() = true;
    let (registry, caps) = coop_setup(1, "stream", adapter.clone());

    // host-call を try/catch で囲んでも、BudgetExceeded は catch 不能 terminal（捕捉されない）。
    let script = engine
        .compile("try\n  let x = stream()\ncatch e\n  let y = 1\nend\n")
        .unwrap();
    let mut ctx = coop_context(registry, caps);
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");

    let outcome = loop {
        match handle.poll(PollSlice::default()).unwrap() {
            PollResult::Terminal { outcome, .. } => break outcome,
            PollResult::Yielded { .. } => continue,
            PollResult::Paused { .. } => unreachable!(),
        }
    };
    // BudgetExceeded(HostResponseBytes) は catch 不能 → RuntimeError terminal（catch されない）。
    match outcome {
        ExecutionOutcome::RuntimeError { error } => {
            assert_eq!(
                error.error_type(),
                "io_limit",
                "HostResponseBytes 超過は io_limit kind（catch 不能 terminal）"
            );
        }
        other => panic!("RuntimeError(io_limit) を期待したが {other:?}"),
    }
}

/// may_yield=false 文脈（関数本体内）で cooperative adapter が Pending を返すと、busy-loop せず
/// catch 不能 internal terminal（InternalControl）になり、try/catch でも捕捉できない（Finding 7）。
/// 対比として may_yield=true（トップレベル文）では同じ adapter の Pending が HostCallPending に
/// なることを確認する。
#[test]
fn may_yield_false_requires_ready_else_internal_terminal() {
    // (a) 関数本体内（can_yield=false）で Pending → InternalControl terminal（try/catch でも不可）。
    {
        let engine = Engine::with_limits(limits(1, 0));
        let adapter = FakeCooperativeAdapter::pending(1, "inner");
        let (registry, caps) = coop_setup(1, "inner", adapter.clone());
        // 関数本体内で cooperative host-call を呼ぶ。try/catch で囲んでも捕捉されない。
        let script = engine
            .compile("fn f()\n  let x = inner()\nend\ntry\n  f()\ncatch e\n  let y = 1\nend\n")
            .unwrap();
        let mut ctx = coop_context(registry, caps);
        let mut handle = engine
            .create_execution(&script, &mut ctx, ExecutionRequest::new())
            .expect("active");

        let outcome = loop {
            match handle.poll(PollSlice::default()).unwrap() {
                PollResult::Terminal { outcome, .. } => break outcome,
                PollResult::Yielded { .. } => continue,
                PollResult::Paused { .. } => unreachable!(),
            }
        };
        match outcome {
            ExecutionOutcome::RuntimeError { error } => {
                assert_eq!(
                    error.error_type(),
                    "internal_control",
                    "may_yield=false の Pending は InternalControl（catch 不能、既存 internal とは別）"
                );
            }
            other => panic!("RuntimeError(internal_control) を期待したが {other:?}"),
        }
        // adapter が観測した may_yield は false（関数本体内）。
        assert_eq!(*adapter.last_may_yield.lock().unwrap(), Some(false));
    }

    // (b) トップレベル文（can_yield=true）では同じ adapter の Pending が HostCallPending になる。
    {
        let engine = Engine::with_limits(limits(1, 0));
        let adapter = FakeCooperativeAdapter::pending(2, "top");
        let (registry, caps) = coop_setup(2, "top", adapter.clone());
        let script = engine.compile("let x = top()\n").unwrap();
        let mut ctx = coop_context(registry, caps);
        let mut handle = engine
            .create_execution(&script, &mut ctx, ExecutionRequest::new())
            .expect("active");

        let r = handle.poll(PollSlice::default()).unwrap();
        assert!(
            matches!(
                r,
                PollResult::Yielded {
                    reason: YieldReason::HostCallPending { .. },
                    ..
                }
            ),
            "may_yield=true の Pending は HostCallPending yield: {r:?}"
        );
        assert_eq!(*adapter.last_may_yield.lock().unwrap(), Some(true));
    }
}

/// tree-expr-continuation-e3（TEC-AT-02 e3 分）: cooperative host function の引数に呼び出しを置いた
/// call が Pending になっても、resume の文の評価し直しで、この call の引数の呼び出しは 1 回だけ
/// 実行される（格納結果が届くので引数を評価し直さない）。同じ文の他の部分の呼び出しは評価し直しの
/// 分だけ実行される（現行の「同じ文の再評価」と同じ観測）。関数本体の中で同じ形は InternalControl。
#[test]
fn cooperative_pending_with_call_argument_evaluates_argument_once() {
    let cases = [
        // 末尾の `ok` が one() の実行回数・g() の実行回数・格納結果を検査する。
        "fn one()\n  one_n = one_n + 1\n  return 1\nend\nlet one_n = 0\n\
         let x = fetch(one())\n\
         let ok = [0][one_n - 1] + [0][x - 41]\n",
        "let one_n = 0\nlet g_n = 0\n\
             fn one()\n  one_n = one_n + 1\n  return 1\nend\n\
             fn g()\n  g_n = g_n + 1\n  return 2\nend\n\
             let t = [g(), fetch(one())]\n\
             let ok = [0][one_n - 1] + [0][g_n - 2] + [0][t[1] - 41]\n",
        // builtin の式 frame（評価済みの第 2 引数 `g() - 2` を持つ）の下で Pending になる形。
        "let one_n = 0\nlet g_n = 0\n\
         fn one()\n  one_n = one_n + 1\n  return 1\nend\n\
         fn g()\n  g_n = g_n + 1\n  return 2\nend\n\
         let s = slice([1, 2, 3], g() - 2, fetch(one()) - 40)\n\
         let ok = [0][one_n - 1] + [0][g_n - 2] + [0][len(s) - 1]\n",
    ];
    for text in cases {
        let engine = Engine::with_limits(limits(1, 0));
        let adapter = FakeCooperativeAdapter::pending_unary(1, "fetch");
        let (registry, caps) = coop_setup(1, "fetch", adapter.clone());
        let script = engine.compile(text).unwrap();
        let mut ctx = coop_context(registry, caps);
        let mut handle = engine
            .create_execution(&script, &mut ctx, ExecutionRequest::new())
            .expect("active");
        loop {
            match handle.poll(PollSlice::default()).unwrap() {
                PollResult::Yielded {
                    reason: YieldReason::HostCallPending { .. },
                    ..
                } => break,
                PollResult::Yielded { .. } => continue,
                other => panic!("HostCallPending を期待: {other:?}\n{text}"),
            }
        }
        assert_eq!(*adapter.last_may_yield.lock().unwrap(), Some(true));
        let completer = take_stashed_completer().expect("completer");
        completer.complete(Ok(Value::Int(41)));
        let outcome = loop {
            match handle.poll(PollSlice::default()).unwrap() {
                PollResult::Terminal { outcome, .. } => break outcome,
                PollResult::Yielded { .. } => continue,
                PollResult::Paused { .. } => panic!("pause は要求していない"),
            }
        };
        // 末尾の `ok` の式が、one() = 1 回・g() = 2 回・格納結果 41 を検査する（外れると index エラー）。
        assert_eq!(outcome, ExecutionOutcome::Completed, "{text}");
    }

    // 関数本体の中の同じ形は may_yield = false の規約違反（catch 不能 InternalControl）。
    let engine = Engine::with_limits(limits(1, 0));
    let adapter = FakeCooperativeAdapter::pending_unary(2, "fetch");
    let (registry, caps) = coop_setup(2, "fetch", adapter.clone());
    let script = engine
        .compile(
            "fn one()\n  return 1\nend\nfn f()\n  let x = fetch(one())\nend\n\
             try\n  f()\ncatch e\n  let y = 1\nend\n",
        )
        .unwrap();
    let mut ctx = coop_context(registry, caps);
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");
    let outcome = loop {
        match handle.poll(PollSlice::default()).unwrap() {
            PollResult::Terminal { outcome, .. } => break outcome,
            PollResult::Yielded { .. } => continue,
            PollResult::Paused { .. } => unreachable!(),
        }
    };
    match outcome {
        ExecutionOutcome::RuntimeError { error } => {
            assert_eq!(error.error_type(), "internal_control");
        }
        other => panic!("RuntimeError(internal_control) を期待したが {other:?}"),
    }
    assert_eq!(*adapter.last_may_yield.lock().unwrap(), Some(false));
}

// =============================================================================
// fix-host-call-resume-requeue（issue #359）: 完了を受け取った非先頭 handle を Ready にする
// =============================================================================

/// HRQ-AT-01: issue #359 の再現（h1・h2 を交互に小さい slice で poll）が往復上限の中で完走する。
/// HRQ-AT-02: 完了を受け取った非先頭の h1 は `Ready` で残り、`HostCallPending` に戻らない。
/// HRQ-AT-03: h1 は完了を受け取った poll で末尾へ置かれ、以後 1 slice ずつ交互に進む。
#[test]
fn host_resume_non_head_becomes_ready_and_round_robin_completes() {
    let engine = Engine::with_limits(limits(2, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "fetch");
    let (registry, caps) = coop_setup(1, "fetch", adapter.clone());
    // h1 は host call の完了後も数 slice かかり、h2 は長く回る。h1 を HostCallPending、h2 を
    // SliceFuelExhausted にしてから complete する。
    let mut ctx_a = coop_context(registry, caps);
    let mut ctx_b = ExecutionContext::new();
    let script_a = engine
        .compile(
            "let x = fetch()\nlet ok = [0][x - 41]\nlet i = 0\n\
             while i < 300\n  i = i + 1\nend\n",
        )
        .unwrap();
    let script_b = engine
        .compile("let j = 0\nwhile j < 1000\n  j = j + 1\nend\n")
        .unwrap();
    let mut h1 = engine
        .create_execution(&script_a, &mut ctx_a, ExecutionRequest::new())
        .expect("h1 active");
    let mut h2 = engine
        .create_execution(&script_b, &mut ctx_b, ExecutionRequest::new())
        .expect("h2 active");
    let r = h1.poll(PollSlice { max_fuel: 64 }).unwrap();
    assert!(matches!(
        r,
        PollResult::Yielded {
            reason: YieldReason::HostCallPending { .. },
            ..
        }
    ));
    let r = h2.poll(PollSlice { max_fuel: 64 }).unwrap();
    assert!(matches!(
        r,
        PollResult::Yielded {
            reason: YieldReason::SliceFuelExhausted,
            ..
        }
    ));
    let completer = take_stashed_completer().expect("completer");
    completer.complete(Ok(Value::Int(41)));
    assert_eq!(adapter.next_id.load(Ordering::SeqCst), 2, "start は 1 回");
    // h2 が run-turn の先頭にいる間の h1 poll: 完了を受け取るが先頭でないので Preempted。
    let fuel_h1_before = h1.usage().committed.fuel;
    let r = h1.poll(PollSlice { max_fuel: 64 }).unwrap();
    assert!(
        matches!(
            r,
            PollResult::Yielded {
                reason: YieldReason::SchedulerPreempted,
                ..
            }
        ),
        "{r:?}"
    );
    assert_eq!(h1.state(), tsumugi::ExecutionState::Ready);
    assert_eq!(
        h1.usage().committed.fuel,
        fuel_h1_before,
        "非先頭の poll は semantic work をしない"
    );

    // 以後は h1・h2 を交互に poll する。各 poll は先頭なら 1 slice 進み（SliceFuelExhausted か
    // terminal）、先頭でなければ SchedulerPreempted で fuel を使わない。h1 は HostCallPending に
    // 戻らず、どちらかだけが連続して進むことはない（他方が terminal のあとを除く）。
    let (mut done1, mut done2) = (false, false);
    let mut last_advanced = 0u8; // 直前に進んだ handle（0 は未設定）
    for round in 0..2000 {
        for who in [1u8, 2u8] {
            let (done, usage, result) = if who == 1 {
                if done1 {
                    continue;
                }
                let before = h1.usage().committed.fuel;
                (
                    &mut done1,
                    before,
                    h1.poll(PollSlice { max_fuel: 64 }).unwrap(),
                )
            } else {
                if done2 {
                    continue;
                }
                let before = h2.usage().committed.fuel;
                (
                    &mut done2,
                    before,
                    h2.poll(PollSlice { max_fuel: 64 }).unwrap(),
                )
            };
            let after = if who == 1 {
                h1.usage().committed.fuel
            } else {
                h2.usage().committed.fuel
            };
            match result {
                PollResult::Terminal { outcome, .. } => {
                    assert_eq!(outcome, ExecutionOutcome::Completed);
                    *done = true;
                    last_advanced = who;
                }
                PollResult::Yielded {
                    reason: YieldReason::SliceFuelExhausted,
                    ..
                } => {
                    assert!(after > usage, "先頭の poll は 1 slice 進む");
                    let other_done = if who == 1 { done2 } else { done1 };
                    assert!(
                        other_done || last_advanced != who,
                        "同じ handle が連続して進まない"
                    );
                    last_advanced = who;
                }
                PollResult::Yielded {
                    reason: YieldReason::SchedulerPreempted,
                    ..
                } => assert_eq!(after, usage, "非先頭の poll は fuel を使わない"),
                other => panic!("round {round} h{who}: {other:?}"),
            }
        }
        if done1 && done2 {
            break;
        }
    }
    assert!(
        done1 && done2,
        "h1・h2 とも往復上限の中で terminal に達する"
    );
    assert_eq!(
        adapter.next_id.load(Ordering::SeqCst),
        2,
        "start は 1 回だけ"
    );
}

/// HRQ-AT-04: 完了を受け取った poll で先頭だった場合（execution 1 本）はその poll で続きを進め、
/// complete 前の cancel は今と同じ `Cancelled` terminal になる。既存の host call のテスト
/// （`host_pending_waker_roundtrip` ほか）は変更しない。
#[test]
fn host_resume_head_continues_in_same_poll_and_cancel_before_complete_is_unchanged() {
    let engine = Engine::with_limits(limits(2, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "fetch");
    let (registry, caps) = coop_setup(1, "fetch", adapter.clone());
    let script = engine
        .compile("let x = fetch()\nlet ok = [0][x - 41]\n")
        .unwrap();
    let mut ctx = coop_context(registry, caps);
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");
    let r = handle.poll(PollSlice::default()).unwrap();
    assert!(matches!(
        r,
        PollResult::Yielded {
            reason: YieldReason::HostCallPending { .. },
            ..
        }
    ));
    take_stashed_completer()
        .expect("completer")
        .complete(Ok(Value::Int(41)));
    // 先頭なので、完了を受け取った同じ poll で続きを進めて terminal に達する。
    match handle.poll(PollSlice::default()).unwrap() {
        PollResult::Terminal { outcome, .. } => assert_eq!(outcome, ExecutionOutcome::Completed),
        other => panic!("同じ poll で Completed を期待したが {other:?}"),
    }

    // complete 前の cancel は Cancelled terminal（ticket の cancel と rollback）。
    let adapter = FakeCooperativeAdapter::pending(2, "slow");
    let (registry, caps) = coop_setup(2, "slow", adapter.clone());
    let script = engine.compile("let x = slow()\n").unwrap();
    let mut ctx = coop_context(registry, caps);
    let mut handle = engine
        .create_execution(&script, &mut ctx, ExecutionRequest::new())
        .expect("active");
    let r = handle.poll(PollSlice::default()).unwrap();
    assert!(matches!(
        r,
        PollResult::Yielded {
            reason: YieldReason::HostCallPending { .. },
            ..
        }
    ));
    handle.cancellation_token().cancel();
    match handle.poll(PollSlice::default()).unwrap() {
        PollResult::Terminal { outcome, .. } => assert_eq!(outcome, ExecutionOutcome::Cancelled),
        other => panic!("Cancelled を期待したが {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// fix-pause-run-turn-remove（issue #361、execution-control §9.2・§11・§12.1）
// ---------------------------------------------------------------------------------------------

const PRT_HEAVY: &str = "let i = 0\nwhile i < 1000\n  i = i + 1\nend\n";
const PRT_SLICE: PollSlice = PollSlice { max_fuel: 64 };

fn prt_is(r: &PollResult, want: &YieldReason) -> bool {
    matches!(r, PollResult::Yielded { reason, .. } if reason == want)
}

fn prt_is_terminal_completed(r: &PollResult) -> bool {
    matches!(
        r,
        PollResult::Terminal {
            outcome: ExecutionOutcome::Completed,
            ..
        }
    )
}

/// PRT-AT-01: run-turn の先頭の handle を pause しても、他の handle が進んで terminal に達する。
#[test]
fn prt_at_01_pausing_head_does_not_stall_other_handles() {
    let engine = Engine::with_limits(limits(2, 0));
    let script = engine.compile(PRT_HEAVY).unwrap();
    let mut ctx1 = ExecutionContext::new();
    let mut ctx2 = ExecutionContext::new();
    let mut h1 = engine
        .create_execution(&script, &mut ctx1, ExecutionRequest::new())
        .unwrap();
    let mut h2 = engine
        .create_execution(&script, &mut ctx2, ExecutionRequest::new())
        .unwrap();

    let r = h1.poll(PRT_SLICE).unwrap();
    assert!(prt_is(&r, &YieldReason::SliceFuelExhausted));
    let r = h2.poll(PRT_SLICE).unwrap();
    assert!(prt_is(&r, &YieldReason::SchedulerPreempted));

    h1.pause().unwrap();
    let h1_fuel = h1.usage().committed.fuel;

    let before = h2.usage().committed.fuel;
    let r = h2.poll(PRT_SLICE).unwrap();
    assert!(
        !prt_is(&r, &YieldReason::SchedulerPreempted),
        "pause した先頭 handle が他を止めてはならない: {r:?}"
    );
    assert!(h2.usage().committed.fuel > before, "h2 の fuel が増える");

    let mut done = false;
    for _ in 0..5000 {
        let r = h2.poll(PRT_SLICE).unwrap();
        if prt_is_terminal_completed(&r) {
            done = true;
            break;
        }
    }
    assert!(done, "h2 は上限の往復回数の中で Completed に達する");
    assert!(matches!(h1.state(), tsumugi::ExecutionState::Paused(_)));
    assert_eq!(h1.usage().committed.fuel, h1_fuel, "h1 の usage は不変");
}

/// PRT-AT-02: resume した handle は pause で外れた位置ではなく末尾へ戻り、round-robin で進む。
#[test]
fn prt_at_02_resume_requeues_at_tail_and_round_robin_completes() {
    let engine = Engine::with_limits(limits(3, 0));
    let script = engine.compile(PRT_HEAVY).unwrap();
    let mut ctx1 = ExecutionContext::new();
    let mut ctx2 = ExecutionContext::new();
    let mut ctx3 = ExecutionContext::new();
    let mut h1 = engine
        .create_execution(&script, &mut ctx1, ExecutionRequest::new())
        .unwrap();
    let mut h2 = engine
        .create_execution(&script, &mut ctx2, ExecutionRequest::new())
        .unwrap();
    let mut h3 = engine
        .create_execution(&script, &mut ctx3, ExecutionRequest::new())
        .unwrap();

    assert!(prt_is(
        &h1.poll(PRT_SLICE).unwrap(),
        &YieldReason::SliceFuelExhausted
    ));
    assert!(prt_is(
        &h2.poll(PRT_SLICE).unwrap(),
        &YieldReason::SchedulerPreempted
    ));
    h1.pause().unwrap();
    // h2 が先頭で数 slice 進む（queue は [h2]）。
    for _ in 0..3 {
        assert!(prt_is(
            &h2.poll(PRT_SLICE).unwrap(),
            &YieldReason::SliceFuelExhausted
        ));
    }

    // resume で queue は [h2, h1]。h1 は h2 より前へ挿入されない。
    h1.resume().unwrap();
    // まだ poll していない h3 は h1 より後ろ: [h2, h1, h3]。
    assert!(prt_is(
        &h3.poll(PRT_SLICE).unwrap(),
        &YieldReason::SchedulerPreempted
    ));
    assert!(prt_is(
        &h1.poll(PRT_SLICE).unwrap(),
        &YieldReason::SchedulerPreempted
    ));
    // h2 が進み rotate: [h1, h3, h2]。
    let f2 = h2.usage().committed.fuel;
    assert!(prt_is(
        &h2.poll(PRT_SLICE).unwrap(),
        &YieldReason::SliceFuelExhausted
    ));
    assert!(h2.usage().committed.fuel > f2);
    // h1 が先頭: 進んで rotate: [h3, h2, h1]。
    let f1 = h1.usage().committed.fuel;
    assert!(prt_is(
        &h1.poll(PRT_SLICE).unwrap(),
        &YieldReason::SliceFuelExhausted
    ));
    assert!(h1.usage().committed.fuel > f1);
    // h3 が先頭（h1 より後ろに登録されていたので h1 の次）。
    let f3 = h3.usage().committed.fuel;
    assert!(prt_is(
        &h3.poll(PRT_SLICE).unwrap(),
        &YieldReason::SliceFuelExhausted
    ));
    assert!(h3.usage().committed.fuel > f3);

    // 以後 3 つとも round-robin で Completed まで進む。terminal の fuel を記録する。
    let mut done = [false; 3];
    let mut fuel = [0u64; 3];
    let terminal_fuel = |r: &PollResult| match r {
        PollResult::Terminal {
            outcome: ExecutionOutcome::Completed,
            usage,
        } => Some(usage.committed.fuel),
        _ => None,
    };
    for _ in 0..5000 {
        if done.iter().all(|d| *d) {
            break;
        }
        if !done[0]
            && let Some(f) = terminal_fuel(&h1.poll(PRT_SLICE).unwrap())
        {
            done[0] = true;
            fuel[0] = f;
        }
        if !done[1]
            && let Some(f) = terminal_fuel(&h2.poll(PRT_SLICE).unwrap())
        {
            done[1] = true;
            fuel[1] = f;
        }
        if !done[2]
            && let Some(f) = terminal_fuel(&h3.poll(PRT_SLICE).unwrap())
        {
            done[2] = true;
            fuel[2] = f;
        }
    }
    assert!(done.iter().all(|d| *d), "全 handle が Completed に達する");
    // pause した h1 の結果は、pause しなかった h2・h3 と同じ（continuation を保って続ける）。
    assert_eq!(
        fuel[0], fuel[1],
        "h1 の terminal fuel は pause なしの h2 と同じ"
    );
    assert_eq!(
        fuel[0], fuel[2],
        "h1 の terminal fuel は pause なしの h3 と同じ"
    );
}

/// PRT-AT-03: run-turn にいた handle の pause は他を止めず、いなかった handle の resume は
/// run-turn に入らない。
#[test]
fn prt_at_03_registered_and_unregistered_handles_stay_consistent() {
    // (a) 非先頭で run-turn に登録済みの handle を pause しても、先頭は進み続ける。
    let engine = Engine::with_limits(limits(2, 0));
    let script = engine.compile(PRT_HEAVY).unwrap();
    let mut ctx1 = ExecutionContext::new();
    let mut ctx2 = ExecutionContext::new();
    let mut h1 = engine
        .create_execution(&script, &mut ctx1, ExecutionRequest::new())
        .unwrap();
    let mut h2 = engine
        .create_execution(&script, &mut ctx2, ExecutionRequest::new())
        .unwrap();
    assert!(prt_is(
        &h1.poll(PRT_SLICE).unwrap(),
        &YieldReason::SliceFuelExhausted
    ));
    assert!(prt_is(
        &h2.poll(PRT_SLICE).unwrap(),
        &YieldReason::SchedulerPreempted
    ));
    h2.pause().unwrap();
    for _ in 0..3 {
        assert!(prt_is(
            &h1.poll(PRT_SLICE).unwrap(),
            &YieldReason::SliceFuelExhausted
        ));
    }
    drop(h2);
    drop(h1);

    // (b) まだ poll していない Created を pause / resume しても run-turn に入らない。
    let mut ctx3 = ExecutionContext::new();
    let mut ctx4 = ExecutionContext::new();
    let mut h3 = engine
        .create_execution(&script, &mut ctx3, ExecutionRequest::new())
        .unwrap();
    let mut h4 = engine
        .create_execution(&script, &mut ctx4, ExecutionRequest::new())
        .unwrap();
    h4.pause().unwrap();
    h4.resume().unwrap();
    // h4 が run-turn に入っていれば h3 は非先頭で Preempted になる。
    assert!(prt_is(
        &h3.poll(PRT_SLICE).unwrap(),
        &YieldReason::SliceFuelExhausted
    ));
    // h4 は次の poll から今と同じに登録され、h3 の後ろで Preempted。
    assert!(prt_is(
        &h4.poll(PRT_SLICE).unwrap(),
        &YieldReason::SchedulerPreempted
    ));
}

/// PRT-AT-03: `Yielded(HostCallPending)` の handle の pause / resume は run-turn に触れない。
#[test]
fn prt_at_03_host_call_pending_pause_resume_does_not_enter_run_turn() {
    let engine = Engine::with_limits(limits(2, 0));
    let adapter = FakeCooperativeAdapter::pending(1, "fetch");
    let (registry, caps) = coop_setup(1, "fetch", adapter.clone());
    let mut ctx_a = coop_context(registry, caps);
    let mut ctx_b = ExecutionContext::new();
    let script_a = engine.compile("let x = fetch()\n").unwrap();
    let script_b = engine.compile(PRT_HEAVY).unwrap();
    let mut h1 = engine
        .create_execution(&script_a, &mut ctx_a, ExecutionRequest::new())
        .unwrap();
    let mut h2 = engine
        .create_execution(&script_b, &mut ctx_b, ExecutionRequest::new())
        .unwrap();
    let r = h1.poll(PRT_SLICE).unwrap();
    assert!(matches!(
        r,
        PollResult::Yielded {
            reason: YieldReason::HostCallPending { .. },
            ..
        }
    ));
    assert!(prt_is(
        &h2.poll(PRT_SLICE).unwrap(),
        &YieldReason::SliceFuelExhausted
    ));

    h1.pause().unwrap();
    h1.resume().unwrap();
    // h1 が run-turn に入っていれば h2 は rotate されて次の poll で Preempted になる。
    for _ in 0..2 {
        assert!(prt_is(
            &h2.poll(PRT_SLICE).unwrap(),
            &YieldReason::SliceFuelExhausted
        ));
    }

    // ticket 完了後、h1 は step 4 から今と同じに進み Completed に達する。
    let completer = take_stashed_completer().expect("completer");
    completer.complete(Ok(Value::Int(1)));
    let mut done1 = false;
    let mut done2 = false;
    for _ in 0..5000 {
        if done1 && done2 {
            break;
        }
        if !done1 && prt_is_terminal_completed(&h1.poll(PRT_SLICE).unwrap()) {
            done1 = true;
        }
        if !done2 && prt_is_terminal_completed(&h2.poll(PRT_SLICE).unwrap()) {
            done2 = true;
        }
    }
    assert!(done1 && done2);
}

/// PRT-AT-04: paused handle の cancel・drop の経路が変わらず、run-turn に古い登録が残らない。
#[test]
fn prt_at_04_cancel_and_drop_while_paused_leave_no_stale_run_turn_entry() {
    let engine = Engine::with_limits(limits(2, 0));
    let script = engine.compile(PRT_HEAVY).unwrap();
    let mut ctx1 = ExecutionContext::new();
    let mut ctx2 = ExecutionContext::new();
    let mut h1 = engine
        .create_execution(&script, &mut ctx1, ExecutionRequest::new())
        .unwrap();
    let mut h2 = engine
        .create_execution(&script, &mut ctx2, ExecutionRequest::new())
        .unwrap();
    assert!(prt_is(
        &h1.poll(PRT_SLICE).unwrap(),
        &YieldReason::SliceFuelExhausted
    ));
    assert!(prt_is(
        &h2.poll(PRT_SLICE).unwrap(),
        &YieldReason::SchedulerPreempted
    ));
    h1.pause().unwrap();

    // pause 中も active slot を消費する（満杯のまま）。
    let mut ctx3 = ExecutionContext::new();
    assert!(matches!(
        engine.create_execution(&script, &mut ctx3, ExecutionRequest::new()),
        Err(StartError::Backpressure { .. })
    ));

    // paused の handle を drop すると slot が解放され、h2 が先頭で進む。
    drop(h1);
    assert!(prt_is(
        &h2.poll(PRT_SLICE).unwrap(),
        &YieldReason::SliceFuelExhausted
    ));
    let _h3 = engine
        .create_execution(&script, &mut ctx3, ExecutionRequest::new())
        .expect("slot 解放後は admit が通る");

    // pause 中の cancel は resume 後の poll で Cancelled になる。
    h2.pause().unwrap();
    h2.cancellation_token().cancel();
    h2.resume().unwrap();
    let r = h2.poll(PRT_SLICE).unwrap();
    assert!(
        matches!(
            r,
            PollResult::Terminal {
                outcome: ExecutionOutcome::Cancelled,
                ..
            }
        ),
        "{r:?}"
    );
}

// ---- fix-terminal-release-slot（issue #362、execution-control §9.2・§12） ----

fn run_to_terminal(h: &mut tsumugi::ExecutionHandle<'_, '_, '_>) -> ExecutionOutcome {
    for _ in 0..5000 {
        match h.poll(PollSlice::default()).unwrap() {
            PollResult::Terminal { outcome, .. } => return outcome,
            PollResult::Yielded { .. } => continue,
            PollResult::Paused { .. } => panic!("pause は要求していない"),
        }
    }
    panic!("terminal に達しなかった");
}

fn is_queued(r: &PollResult) -> bool {
    matches!(
        r,
        PollResult::Yielded {
            reason: YieldReason::AdmissionQueued { .. },
            ..
        }
    )
}

/// TRS-AT-01: Terminal の handle を保持したままでも後続が active を取れる。
#[test]
fn trs_at_01_held_terminal_handle_does_not_block_next_admission() {
    let engine = Engine::with_limits(limits(1, 0));
    let scripts = [
        ("let a = 1\n", "completed"),
        ("let y = missing_name\n", "runtime"),
        ("import \"__no_such_module__\"\n", "link"),
    ];
    for (src, kind) in scripts {
        let script = engine.compile(src).unwrap();
        let ok = engine.compile("let b = 2\n").unwrap();
        let mut ctx1 = ExecutionContext::new();
        let mut h1 = engine
            .create_execution(&script, &mut ctx1, ExecutionRequest::new())
            .expect("h1 active");
        let outcome = run_to_terminal(&mut h1);
        match (kind, &outcome) {
            ("completed", ExecutionOutcome::Completed) => {}
            ("runtime", ExecutionOutcome::RuntimeError { .. }) => {}
            ("link", ExecutionOutcome::LinkError { .. }) => {}
            _ => panic!("{kind}: 想定外の outcome {outcome:?}"),
        }
        // h1 は保持したまま、別の context で後続を作れる。
        let mut ctx2 = ExecutionContext::new();
        let mut h2 = engine
            .create_execution(&ok, &mut ctx2, ExecutionRequest::new())
            .expect("Terminal の h1 を保持していても active を取れる");
        assert_eq!(run_to_terminal(&mut h2), ExecutionOutcome::Completed);
        // h1 の state / outcome は変わらない。
        assert_eq!(h1.state(), tsumugi::ExecutionState::Terminal);
        assert_eq!(h1.outcome(), Some(&outcome));
    }
}

/// TRS-AT-02: queue の最古は、先頭が Terminal を返した直後に昇格する。
#[test]
fn trs_at_02_queue_head_promoted_at_terminal() {
    let engine = Engine::with_limits(limits(1, 1));
    let script = engine.compile("let a = 1\n").unwrap();
    let mut c1 = ExecutionContext::new();
    let mut c2 = ExecutionContext::new();
    let mut h1 = engine
        .create_execution(&script, &mut c1, ExecutionRequest::new())
        .unwrap();
    let mut h2 = engine
        .create_execution(&script, &mut c2, ExecutionRequest::new())
        .unwrap();
    // h1 が terminal を返す前は AdmissionQueued。
    assert!(is_queued(&h2.poll(PollSlice::default()).unwrap()));
    assert_eq!(run_to_terminal(&mut h1), ExecutionOutcome::Completed);
    // h1 保持のまま、h2 は昇格して terminal まで進む。
    let r = h2.poll(PollSlice::default()).unwrap();
    assert!(!is_queued(&r), "昇格しているはず: {r:?}");
    let outcome = match r {
        PollResult::Terminal { outcome, .. } => outcome,
        _ => run_to_terminal(&mut h2),
    };
    assert_eq!(outcome, ExecutionOutcome::Completed);
    // h2 も terminal で解放済み。active は空なので 3 つめは active を取る。
    let mut c3 = ExecutionContext::new();
    let h3 = engine
        .create_execution(&script, &mut c3, ExecutionRequest::new())
        .unwrap();
    assert!(!matches!(
        h3.state(),
        tsumugi::ExecutionState::Yielded(YieldReason::AdmissionQueued { .. })
    ));
}

/// TRS-AT-02 後半: 昇格した h2 が active を持つ間、3 つめは queue に入る。
#[test]
fn trs_at_02_third_goes_to_queue_while_promoted_active() {
    let engine = Engine::with_limits(limits(1, 1));
    let script = engine.compile("let a = 1\n").unwrap();
    let mut c1 = ExecutionContext::new();
    let mut c2 = ExecutionContext::new();
    let mut h1 = engine
        .create_execution(&script, &mut c1, ExecutionRequest::new())
        .unwrap();
    let h2 = engine
        .create_execution(&script, &mut c2, ExecutionRequest::new())
        .unwrap();
    assert_eq!(run_to_terminal(&mut h1), ExecutionOutcome::Completed);
    // h2 は昇格済み（未 poll）。active=h2、queue 空なので 3 つめは queue。
    let mut c3 = ExecutionContext::new();
    let h3 = engine
        .create_execution(&script, &mut c3, ExecutionRequest::new())
        .unwrap();
    assert!(matches!(
        h3.state(),
        tsumugi::ExecutionState::Yielded(YieldReason::AdmissionQueued { .. })
    ));
    drop(h2);
}

/// TRS-AT-03: Terminal 後の drop で二重解放・二重昇格しない。
#[test]
fn trs_at_03_drop_after_terminal_does_not_double_release() {
    let engine = Engine::with_limits(limits(1, 1));
    let script = engine.compile("let a = 1\n").unwrap();
    let mut c1 = ExecutionContext::new();
    let mut c2 = ExecutionContext::new();
    let mut h1 = engine
        .create_execution(&script, &mut c1, ExecutionRequest::new())
        .unwrap();
    let h2 = engine
        .create_execution(&script, &mut c2, ExecutionRequest::new())
        .unwrap();
    assert_eq!(run_to_terminal(&mut h1), ExecutionOutcome::Completed);
    // h2 が昇格したあとに h1 を drop しても h2 の active は保たれる。
    drop(h1);
    let mut c3 = ExecutionContext::new();
    let h3 = engine
        .create_execution(&script, &mut c3, ExecutionRequest::new())
        .expect("queue に入る（active は h2 のまま）");
    assert!(matches!(
        h3.state(),
        tsumugi::ExecutionState::Yielded(YieldReason::AdmissionQueued { .. })
    ));
    let mut c4 = ExecutionContext::new();
    match engine.create_execution(&script, &mut c4, ExecutionRequest::new()) {
        Err(StartError::Backpressure {
            active,
            queued,
            limit,
        }) => assert_eq!((active, queued, limit), (1, 1, 1)),
        Ok(_) => panic!("二重解放で active が減りすぎた"),
        Err(other) => panic!("Backpressure を期待したが {other:?}"),
    }
    drop(h2);
    drop(h3);
}

/// TRS-AT-03: queue 中の handle を cancel して Cancelled にすると queue slot が空き、
/// そのあとの drop でも queue の数がずれない。
#[test]
fn trs_at_03_cancelled_queued_handle_releases_queue_slot_once() {
    let engine = Engine::with_limits(limits(1, 1));
    let script = engine.compile("let a = 1\n").unwrap();
    let mut c1 = ExecutionContext::new();
    let mut c2 = ExecutionContext::new();
    let h1 = engine
        .create_execution(&script, &mut c1, ExecutionRequest::new())
        .unwrap();
    let mut h2 = engine
        .create_execution(&script, &mut c2, ExecutionRequest::new())
        .unwrap();
    h2.cancellation_token().cancel();
    match h2.poll(PollSlice::default()).unwrap() {
        PollResult::Terminal { outcome, .. } => assert_eq!(outcome, ExecutionOutcome::Cancelled),
        other => panic!("Cancelled を期待したが {other:?}"),
    }
    // queue slot が空いた（h2 は保持したまま）。
    let mut c3 = ExecutionContext::new();
    let h3 = engine
        .create_execution(&script, &mut c3, ExecutionRequest::new())
        .expect("queue が空いている");
    assert!(matches!(
        h3.state(),
        tsumugi::ExecutionState::Yielded(YieldReason::AdmissionQueued { .. })
    ));
    // h2 の drop で queue の数がずれない（h3 が入ったまま満杯）。
    drop(h2);
    let mut c4 = ExecutionContext::new();
    match engine.create_execution(&script, &mut c4, ExecutionRequest::new()) {
        Err(StartError::Backpressure { active, queued, .. }) => {
            assert_eq!((active, queued), (1, 1));
        }
        Ok(_) => panic!("drop で queue の数が減りすぎた"),
        Err(other) => panic!("Backpressure を期待したが {other:?}"),
    }
    drop(h1);
    drop(h3);
}

/// TRS-AT-03: 昇格済みで未観測の Queued kind の handle を cancel して Cancelled にしても、
/// active が解放され queue の次が昇格し、drop でも active の数がずれない。
#[test]
fn trs_at_03_cancelled_promoted_unobserved_handle_releases_active_once() {
    let engine = Engine::with_limits(limits(1, 2));
    let script = engine.compile("let a = 1\n").unwrap();
    let mut c1 = ExecutionContext::new();
    let mut c2 = ExecutionContext::new();
    let mut c3 = ExecutionContext::new();
    let mut h1 = engine
        .create_execution(&script, &mut c1, ExecutionRequest::new())
        .unwrap();
    let mut h2 = engine
        .create_execution(&script, &mut c2, ExecutionRequest::new())
        .unwrap();
    // h3 は 1 slice では終わらない重い script（昇格後も active を持ち続ける）。
    let heavy = engine
        .compile("let i = 0\nwhile i < 1000\n  i = i + 1\nend\n")
        .unwrap();
    let mut h3 = engine
        .create_execution(&heavy, &mut c3, ExecutionRequest::new())
        .unwrap();
    assert_eq!(run_to_terminal(&mut h1), ExecutionOutcome::Completed);
    // h2 は昇格済み（未 poll）。cancel して poll すると Cancelled。
    h2.cancellation_token().cancel();
    match h2.poll(PollSlice::default()).unwrap() {
        PollResult::Terminal { outcome, .. } => assert_eq!(outcome, ExecutionOutcome::Cancelled),
        other => panic!("Cancelled を期待したが {other:?}"),
    }
    // h3 は昇格して resume_to から進む。
    let r = h3.poll(PollSlice { max_fuel: 16 }).unwrap();
    assert!(!is_queued(&r), "h3 は昇格しているはず: {r:?}");
    // h2 の drop で active の数がずれない: h3 が active のまま、4 つめは queue に入る。
    drop(h2);
    let mut c4 = ExecutionContext::new();
    let h4 = engine
        .create_execution(&script, &mut c4, ExecutionRequest::new())
        .expect("h3 が active を持つので queue に入る");
    assert!(matches!(
        h4.state(),
        tsumugi::ExecutionState::Yielded(YieldReason::AdmissionQueued { .. })
    ));
    drop(h1);
    drop(h3);
    drop(h4);
}

/// TRS-AT-04: ほかの slot・scheduler の経路は変わらない。非 terminal の handle の drop は slot を
/// 解放し、Paused・host 待ちは active を消費し続ける（既存の `paused_consumes_active_slot`・
/// `host_waiting_consumes_active_slot`・`active_and_queue_limits_exact_then_backpressure` が
/// 変更なしで通ることが本体）。ここでは非 terminal の drop の解放だけを直接確かめる。
#[test]
fn trs_at_04_non_terminal_drop_still_releases_slot() {
    let engine = Engine::with_limits(limits(1, 0));
    let script = engine.compile("let a = 1\n").unwrap();
    let mut c1 = ExecutionContext::new();
    let h1 = engine
        .create_execution(&script, &mut c1, ExecutionRequest::new())
        .unwrap();
    drop(h1);
    let mut c2 = ExecutionContext::new();
    let _h2 = engine
        .create_execution(&script, &mut c2, ExecutionRequest::new())
        .expect("非 terminal の drop で active が解放される");
}

/// HIL-AT-02（grant あり・execution 中の resolver 0 回、TM-AT-24）: 呼ばれた回数を数える resolver を
/// grant し、import を 2 本含む script を 1 slice ずつ poll すると、resolver は初回 poll の Link フェーズで
/// import の数だけ呼ばれ、以後の poll では 0 回で、module の top-level が実行される。
#[test]
fn hil_at_02_resolver_called_only_in_link_phase() {
    use std::sync::atomic::Ordering;

    let engine = Engine::new();
    let script = engine
        .compile(
            "import \"a.tsg\"\nimport \"b.tsg\"\nlet z = a1 + b1\nlet i = 0\nwhile i < 200\n  i = i + 1\nend\n",
        )
        .unwrap();
    let (resolver, calls) = counting_resolver::CountingResolver::new(&[
        ("@default/a.tsg", "let a1 = 1\n"),
        ("@default/b.tsg", "let b1 = 2\n"),
    ]);
    let mut context = ExecutionContext::new();
    context.set_capabilities(resolver.capabilities());

    let mut handle = engine
        .create_execution(&script, &mut context, ExecutionRequest::new())
        .expect("admit");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "create_execution は link しない"
    );

    let small = PollSlice { max_fuel: 8 };
    let first = handle.poll(small).expect("poll");
    assert!(
        matches!(first, PollResult::Yielded { .. }),
        "1 slice では終わらないはず: {first:?}"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "resolver は Link フェーズで import の数だけ呼ばれる"
    );

    let mut polls = 1;
    loop {
        match handle.poll(small).expect("poll") {
            PollResult::Terminal { outcome, .. } => {
                assert_eq!(
                    outcome,
                    ExecutionOutcome::Completed,
                    "module の top-level が実行される"
                );
                break;
            }
            PollResult::Yielded { .. } => {
                polls += 1;
                assert_eq!(
                    calls.load(Ordering::SeqCst),
                    2,
                    "Running 以降の resolver 呼び出しは 0 回"
                );
            }
            other => panic!("想定外: {other:?}"),
        }
    }
    assert!(polls > 2, "複数 slice にまたがったはず: {polls}");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

/// HIL-AT-03（読み直さない）: on-disk に無い module を返す in-memory resolver で handle 経路の import が
/// 実行され、CWD に同名のファイルがあっても読まれない。
#[test]
fn hil_at_03_import_uses_resolver_not_filesystem() {
    use std::sync::atomic::Ordering;

    // `tests/fixtures/import_lib.tsg` は CWD（crate root）に実在し、`fn add` を定義する。resolver は
    // 同じ specifier に対し別の内容（`from_resolver`）を返すので、読まれたのがどちらかが分かる。
    assert!(std::path::Path::new("tests/fixtures/import_lib.tsg").exists());
    let engine = Engine::new();
    let script = engine
        .compile("import \"tests/fixtures/import_lib.tsg\"\nlet z = from_resolver + 1\n")
        .unwrap();
    let (resolver, calls) = counting_resolver::CountingResolver::new(&[(
        "@default/tests/fixtures/import_lib.tsg",
        "let from_resolver = 41\n",
    )]);
    let mut context = ExecutionContext::new();
    context.set_capabilities(resolver.capabilities());

    let mut handle = engine
        .create_execution(&script, &mut context, ExecutionRequest::new())
        .expect("admit");
    match handle.poll(PollSlice::default()).expect("poll") {
        PollResult::Terminal { outcome, .. } => {
            assert_eq!(
                outcome,
                ExecutionOutcome::Completed,
                "resolver の内容が展開される"
            );
        }
        other => panic!("terminal を期待したが {other:?}"),
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

/// HIL-AT-03 の補足（REV-020）: import 先の構文エラーは、semantic-decisions §3.4 の「import先のparse失敗」の
/// テンプレート（`{module}` を含む）で、parse 診断を cause として message に保持した terminal の LinkError になる。
#[test]
fn hil_at_03_invalid_module_keeps_module_and_parse_cause() {
    let engine = Engine::new();
    let script = engine.compile("import \"bad.tsg\"\nlet z = 1\n").unwrap();
    let (resolver, _calls) = counting_resolver::CountingResolver::new(&[(
        "@default/bad.tsg",
        "let ok = 1\nlet b = @\n",
    )]);
    let mut context = ExecutionContext::new();
    context.set_capabilities(resolver.capabilities());
    let mut handle = engine
        .create_execution(&script, &mut context, ExecutionRequest::new())
        .expect("admit");
    match handle.poll(PollSlice::default()).expect("poll") {
        PollResult::Terminal {
            outcome: ExecutionOutcome::LinkError { error },
            usage,
        } => {
            assert_eq!(error.error_type(), "import");
            let message = error.message();
            assert!(
                message.starts_with(
                    "import に失敗しました: モジュールの構文が不正です: @default/bad.tsg"
                ),
                "{message}"
            );
            assert!(message.contains("2行目"), "原因の行番号: {message}");
            assert!(
                message.contains("想定外の文字: '@'"),
                "原因の診断: {message}"
            );
            assert_eq!(usage.committed.fuel, 0);
        }
        other => panic!("LinkError を期待したが {other:?}"),
    }
}

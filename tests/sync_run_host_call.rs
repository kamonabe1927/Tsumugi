//! fix-sync-run-host-call-resume（issue #360、`docs/execution-control.md` §6.2・§9.2）:
//! 埋め込みの同期 `Engine::run` が cooperative host call の `Pending` で ticket を引き取り、
//! cancel・deadline を確かめながら完了を待って結果を注入する。`start()` を重複して呼ばない。
//!
//! 受入基準 SHR-AT-01..05。監査 sink なしの経路と、監査付きの経路（`audit_slice_fuel` つきを含む）の
//! 両方で確かめる。`Value` は `!Send` なので completer は作成 thread の thread_local へ stash し、
//! 同じ thread の clock の `now()` から complete する（thread を使わず決定的に駆動する）。

use std::cell::RefCell;
use std::num::NonZeroU128;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tsumugi::{
    AdapterError, AuditEnvelope, AuditEvent, AuditSink, AuditValuePolicy, AuditYieldReason,
    BudgetConfig, BudgetUsage, CancellationToken, CapabilityCallContext, CapabilitySet,
    CompileOptions, CooperativeHostFunction, EmbeddingContext, EmbeddingEngine, EmbeddingOutcome,
    EmbeddingRequest, ExecutionId, FakeClock, HostArity, HostCallCompleter, HostCallError,
    HostCallPoll, HostCallTicket, HostCost, HostFunction, HostFunctionDescriptor, HostFunctionId,
    HostFunctionRegistry, LinkRequest, MonotonicClock, MonotonicInstant, ScriptedAuditSink, Source,
    SourceId, Value, new_ticket,
};

thread_local! {
    static COMPLETERS: RefCell<Vec<HostCallCompleter<Value>>> = const { RefCell::new(Vec::new()) };
}

fn take_completer() -> Option<HostCallCompleter<Value>> {
    COMPLETERS.with(|c| c.borrow_mut().pop())
}

fn host_id(n: u128) -> HostFunctionId {
    HostFunctionId::new(NonZeroU128::new(n).unwrap())
}

fn descriptor(id: u128, name: &str, arity: u16, may_block: bool) -> HostFunctionDescriptor {
    HostFunctionDescriptor {
        id: host_id(id),
        name: name.to_string(),
        arity: HostArity::Exact(arity),
        cost: HostCost::default(),
        argument_audit: Vec::new(),
        result_audit: AuditValuePolicy::Omit,
        may_block,
    }
}

/// `start` のたびに回数を数える cooperative `fetch`（arity 0、id=1）。
struct Fetch {
    descriptor: HostFunctionDescriptor,
    starts: Arc<AtomicUsize>,
    mode: Mode,
}

#[derive(Clone, Copy)]
enum Mode {
    /// complete しない ticket で `Pending` を返す。
    Pending,
    /// `Ready(Ok(Int(n)))` を返す。
    Ready(i64),
    /// `Ready(Err(Host))` を返す。
    ReadyHostError,
}

impl CooperativeHostFunction for Fetch {
    fn descriptor(&self) -> &HostFunctionDescriptor {
        &self.descriptor
    }

    fn start(
        &self,
        _context: &mut CapabilityCallContext<'_>,
        _arguments: &[Value],
    ) -> HostCallPoll<Value> {
        let n = self.starts.fetch_add(1, Ordering::SeqCst) as u64 + 1;
        match self.mode {
            Mode::Ready(v) => HostCallPoll::Ready(Ok(Value::Int(v))),
            Mode::ReadyHostError => {
                HostCallPoll::Ready(Err(AdapterError::Host("fetch".to_string())))
            }
            Mode::Pending => {
                let (ticket, completer): (HostCallTicket<Value>, HostCallCompleter<Value>) =
                    new_ticket::<Value>(n);
                COMPLETERS.with(|c| c.borrow_mut().push(completer));
                HostCallPoll::Pending(ticket)
            }
        }
    }
}

/// 呼ばれた引数を記録する同期 host function `mark`（arity 1、id=2）。
struct Mark {
    descriptor: HostFunctionDescriptor,
    seen: Arc<Mutex<Vec<i64>>>,
}

impl HostFunction for Mark {
    fn descriptor(&self) -> &HostFunctionDescriptor {
        &self.descriptor
    }

    fn call(&self, arguments: &[Value]) -> Result<Value, HostCallError> {
        if let Some(Value::Int(n)) = arguments.first() {
            self.seen.lock().unwrap().push(*n);
        }
        Ok(Value::Null)
    }
}

/// 進む clock。`now()` のたびに `tick_ns` 進む。`complete_after` が `Some(k)` なら、completer が
/// stash されてから k 回目以降の `now()` で同じ thread から `value` で complete する。
struct TestClock {
    clock_id: u64,
    now_ns: AtomicU64,
    tick_ns: u64,
    calls: AtomicUsize,
    complete_after: Option<usize>,
    waited: AtomicUsize,
    outcome: Mutex<Option<Result<i64, ()>>>,
}

impl TestClock {
    fn new(tick_ns: u64) -> Self {
        Self {
            clock_id: FakeClock::new().clock_id(),
            now_ns: AtomicU64::new(0),
            tick_ns,
            calls: AtomicUsize::new(0),
            complete_after: None,
            waited: AtomicUsize::new(0),
            outcome: Mutex::new(None),
        }
    }

    fn completing(tick_ns: u64, after: usize, result: Result<i64, ()>) -> Self {
        let mut clock = Self::new(tick_ns);
        clock.complete_after = Some(after);
        *clock.outcome.lock().unwrap() = Some(result);
        clock
    }
}

impl MonotonicClock for TestClock {
    fn clock_id(&self) -> u64 {
        self.clock_id
    }

    fn now(&self) -> MonotonicInstant {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(after) = self.complete_after
            && let Some(completer) = take_completer()
        {
            // completer が見えた最初の呼び出しは待ちの前半。after 回数ぶん待ってから complete する。
            let seen = self.waited.fetch_add(1, Ordering::SeqCst) + 1;
            if seen > after {
                match self
                    .outcome
                    .lock()
                    .unwrap()
                    .take()
                    .expect("一度だけ complete")
                {
                    Ok(v) => completer.complete(Ok(Value::Int(v))),
                    Err(()) => completer.complete(Err(AdapterError::Host("fetch".to_string()))),
                }
            } else {
                COMPLETERS.with(|c| c.borrow_mut().push(completer));
            }
        }
        let prev = self.now_ns.fetch_add(self.tick_ns, Ordering::Relaxed);
        self.instant_at(prev)
    }
}

struct Setup {
    engine: EmbeddingEngine,
    starts: Arc<AtomicUsize>,
    marks: Arc<Mutex<Vec<i64>>>,
    sink: Option<Arc<ScriptedAuditSink>>,
}

fn setup(mode: Mode, audited: bool, slice_fuel: Option<u64>) -> Setup {
    let starts = Arc::new(AtomicUsize::new(0));
    let marks = Arc::new(Mutex::new(Vec::new()));
    let registry = HostFunctionRegistry::builder()
        .register_cooperative(Arc::new(Fetch {
            descriptor: descriptor(1, "fetch", 0, true),
            starts: Arc::clone(&starts),
            mode,
        }))
        .expect("register fetch")
        .register(Arc::new(Mark {
            descriptor: descriptor(2, "mark", 1, false),
            seen: Arc::clone(&marks),
        }))
        .expect("register mark")
        .build()
        .expect("build registry");
    let mut builder = EmbeddingEngine::builder().host_functions(registry);
    let mut sink = None;
    if audited {
        let s = Arc::new(ScriptedAuditSink::new([]));
        builder = builder.audit_sink(s.clone() as Arc<dyn AuditSink>);
        if let Some(fuel) = slice_fuel {
            builder = builder.audit_slice_fuel(fuel);
        }
        sink = Some(s);
    }
    Setup {
        engine: builder.build().expect("build engine"),
        starts,
        marks,
        sink,
    }
}

fn caps() -> CapabilitySet {
    CapabilitySet::builder()
        .grant_host_function(host_id(1))
        .unwrap()
        .grant_host_function(host_id(2))
        .unwrap()
        .build()
}

fn request(clock: Arc<dyn MonotonicClock>, token: Option<CancellationToken>) -> EmbeddingRequest {
    let budget = BudgetConfig::standard(clock.as_ref()).expect("standard budget");
    let mut request = EmbeddingRequest::new(budget, clock)
        .expect("同 domain")
        .with_execution_id(ExecutionId::new(7u128.try_into().unwrap()))
        .with_capabilities(caps());
    if let Some(token) = token {
        request = request.cancellation(token);
    }
    request
}

fn compile_link(engine: &EmbeddingEngine, text: &'static str) -> tsumugi::LinkedScript {
    let script = engine
        .compile(
            Source::new(SourceId::new("m").unwrap(), text),
            &CompileOptions {
                retain_source: true,
            },
        )
        .expect("compile");
    let clock = FakeClock::new();
    let budget = BudgetConfig::standard(&clock).expect("budget");
    engine
        .link(
            &script,
            LinkRequest::new(
                ExecutionId::new(1u128.try_into().unwrap()),
                CapabilitySet::empty(),
                budget,
            ),
        )
        .expect("link")
}

/// 経路ごとの実行。`audited` なら `run_audited` の outcome を取り出す。
fn run(
    setup: &Setup,
    text: &'static str,
    ctx: &mut EmbeddingContext,
    request: EmbeddingRequest,
) -> EmbeddingOutcome {
    let linked = compile_link(&setup.engine, text);
    if setup.sink.is_some() {
        let audited = setup.engine.run_audited(&linked, ctx, request);
        audited.outcome().cloned().expect("監査は成立する")
    } else {
        setup.engine.run(&linked, ctx, request)
    }
}

/// 各テストを (監査なし, 監査付き, 監査付き + 小さい slice fuel) の 3 経路で回す。
fn paths() -> [(bool, Option<u64>); 3] {
    [(false, None), (true, None), (true, Some(16))]
}

fn fuel(usage: &BudgetUsage) -> u64 {
    usage.committed.fuel
}

fn usage_of(outcome: &EmbeddingOutcome) -> BudgetUsage {
    match outcome {
        EmbeddingOutcome::Completed { usage }
        | EmbeddingOutcome::Cancelled { usage }
        | EmbeddingOutcome::DeadlineExceeded { usage }
        | EmbeddingOutcome::RuntimeError { usage, .. } => *usage,
        other => panic!("usage を持つ outcome を期待したが {other:?}"),
    }
}

fn host_call_yield_resume(envelopes: &[AuditEnvelope]) -> Vec<&'static str> {
    envelopes
        .iter()
        .filter_map(|e| match &e.event {
            AuditEvent::Yielded {
                reason: AuditYieldReason::HostCallPending { .. },
                ..
            } => Some("yielded"),
            AuditEvent::Resumed { .. } => Some("resumed"),
            _ => None,
        })
        .collect()
}

// SHR-AT-01
#[test]
fn never_completing_ticket_starts_once_and_ends_at_deadline() {
    for (audited, slice_fuel) in paths() {
        let s = setup(Mode::Pending, audited, slice_fuel);
        let mut ctx = EmbeddingContext::new(&s.engine);
        // 1 tick = 1 s、deadline は 30 s 先。
        let clock: Arc<dyn MonotonicClock> = Arc::new(TestClock::new(1_000_000_000));
        let outcome = run(&s, "let r = fetch()\n", &mut ctx, request(clock, None));
        assert!(
            matches!(outcome, EmbeddingOutcome::DeadlineExceeded { .. }),
            "audited={audited} fuel={slice_fuel:?}: DeadlineExceeded を期待: {outcome:?}"
        );
        assert_eq!(s.starts.load(Ordering::SeqCst), 1, "start() は 1 回だけ");
        let completer = take_completer().expect("ticket の completer");
        assert!(
            completer.is_cancelled(),
            "待ちを打ち切ったら ticket は cancel される"
        );
    }
}

// SHR-AT-02 / SHR-AT-04（前半）
#[test]
fn completed_value_is_injected_and_following_statement_runs_once() {
    for (audited, slice_fuel) in paths() {
        let mut fuels = Vec::new();
        // 待ちの長さを変えても usage の fuel は同じ（待ちの間は semantic work をしない）。
        for after in [0usize, 1, 40] {
            let s = setup(Mode::Pending, audited, slice_fuel);
            let mut ctx = EmbeddingContext::new(&s.engine);
            let clock: Arc<dyn MonotonicClock> =
                Arc::new(TestClock::completing(1_000, after, Ok(41)));
            let outcome = run(
                &s,
                "let r = fetch()\nmark(r + 1)\n",
                &mut ctx,
                request(clock, None),
            );
            assert!(
                matches!(outcome, EmbeddingOutcome::Completed { .. }),
                "audited={audited} fuel={slice_fuel:?} after={after}: {outcome:?}"
            );
            assert_eq!(*s.marks.lock().unwrap(), vec![42], "続きの文は 1 回だけ");
            assert_eq!(s.starts.load(Ordering::SeqCst), 1, "start() は 1 回だけ");
            fuels.push(fuel(&usage_of(&outcome)));
            if let Some(sink) = &s.sink {
                let events = sink.snapshot();
                assert_eq!(
                    host_call_yield_resume(&events)
                        .into_iter()
                        .filter(|e| *e == "yielded")
                        .count(),
                    1,
                    "Yielded(HostCallPending) は 1 回"
                );
                let seq = host_call_yield_resume(&events);
                let y = seq.iter().position(|e| *e == "yielded").unwrap();
                assert_eq!(seq[y + 1], "resumed", "Yielded の次は Resumed");
            }
        }
        assert!(
            fuels.windows(2).all(|w| w[0] == w[1]),
            "待ちの長さで fuel が変わる: {fuels:?}"
        );
    }
}

// SHR-AT-02（start() が complete 済みの ticket を返す場合）
#[test]
fn ready_after_start_value_is_unchanged() {
    for (audited, slice_fuel) in paths() {
        let s = setup(Mode::Ready(5), audited, slice_fuel);
        let mut ctx = EmbeddingContext::new(&s.engine);
        let clock: Arc<dyn MonotonicClock> = Arc::new(TestClock::new(1_000));
        let outcome = run(
            &s,
            "let r = fetch()\nmark(r)\n",
            &mut ctx,
            request(clock, None),
        );
        assert!(
            matches!(outcome, EmbeddingOutcome::Completed { .. }),
            "{outcome:?}"
        );
        assert_eq!(*s.marks.lock().unwrap(), vec![5]);
        assert_eq!(s.starts.load(Ordering::SeqCst), 1);
    }
}

// SHR-AT-02（AdapterError で complete）と SHR-AT-04（Ready の経路は変わらない）
#[test]
fn adapter_error_completion_matches_ready_error() {
    for (audited, slice_fuel) in paths() {
        let text = "let r = fetch()\nmark(1)\n";
        // 待ちの途中で Host error を complete する。
        let s = setup(Mode::Pending, audited, slice_fuel);
        let mut ctx = EmbeddingContext::new(&s.engine);
        let clock: Arc<dyn MonotonicClock> = Arc::new(TestClock::completing(1_000, 1, Err(())));
        let pending = run(&s, text, &mut ctx, request(clock, None));
        // 同じ error を start() が直接返す場合（Ready 経路）。
        let r = setup(Mode::ReadyHostError, audited, slice_fuel);
        let mut ctx = EmbeddingContext::new(&r.engine);
        let clock: Arc<dyn MonotonicClock> = Arc::new(TestClock::new(1_000));
        let ready = run(&r, text, &mut ctx, request(clock, None));
        match (&pending, &ready) {
            (
                EmbeddingOutcome::RuntimeError { error: a, .. },
                EmbeddingOutcome::RuntimeError { error: b, .. },
            ) => assert_eq!(a.code, b.code, "host error の扱いが同じ"),
            other => panic!("RuntimeError 同士を期待: {other:?}"),
        }
        assert!(
            s.marks.lock().unwrap().is_empty(),
            "error の後の文は実行されない"
        );
        assert_eq!(s.starts.load(Ordering::SeqCst), 1);
    }
}

// SHR-AT-03
#[test]
fn cancel_during_wait_ends_before_deadline_and_rolls_back() {
    for (audited, slice_fuel) in paths() {
        let s = setup(Mode::Pending, audited, slice_fuel);
        let mut ctx = EmbeddingContext::new(&s.engine);
        // 先に値を持つ状態を作る。
        let clock: Arc<dyn MonotonicClock> = Arc::new(TestClock::new(1_000));
        let first = run(&s, "let base = 1\n", &mut ctx, request(clock, None));
        assert!(
            matches!(first, EmbeddingOutcome::Completed { .. }),
            "{first:?}"
        );

        // 時刻が進まない clock（deadline は 30 s 先のまま）。別 thread が 100 ms 後に cancel。
        let clock: Arc<dyn MonotonicClock> = Arc::new(TestClock::new(0));
        let token = CancellationToken::new();
        let canceller = {
            let token = token.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                token.cancel();
            })
        };
        let started = Instant::now();
        let outcome = run(
            &s,
            "base = 2\nlet r = fetch()\n",
            &mut ctx,
            request(clock, Some(token)),
        );
        let elapsed = started.elapsed();
        canceller.join().unwrap();
        assert!(
            matches!(outcome, EmbeddingOutcome::Cancelled { .. }),
            "audited={audited}: {outcome:?}"
        );
        assert!(
            elapsed < Duration::from_secs(10),
            "deadline を待たない: {elapsed:?}"
        );
        assert_eq!(s.starts.load(Ordering::SeqCst), 1);
        assert!(take_completer().expect("completer").is_cancelled());

        // rollback: `base = 2` は残らない。
        let clock: Arc<dyn MonotonicClock> = Arc::new(TestClock::new(1_000));
        let after = run(&s, "mark(base)\n", &mut ctx, request(clock, None));
        assert!(
            matches!(after, EmbeddingOutcome::Completed { .. }),
            "{after:?}"
        );
        assert_eq!(*s.marks.lock().unwrap(), vec![1], "実行前の値が残る");
    }
}

// SHR-AT-04（Ready を返す cooperative 関数と同期 host function の run は変わらない）
#[test]
fn ready_and_sync_host_functions_are_unchanged() {
    for (audited, slice_fuel) in paths() {
        let s = setup(Mode::Ready(3), audited, slice_fuel);
        let mut ctx = EmbeddingContext::new(&s.engine);
        let clock: Arc<dyn MonotonicClock> = Arc::new(TestClock::new(1_000));
        let outcome = run(
            &s,
            "let a = fetch()\nmark(a)\nmark(a + 1)\n",
            &mut ctx,
            request(clock, None),
        );
        assert!(
            matches!(outcome, EmbeddingOutcome::Completed { .. }),
            "{outcome:?}"
        );
        assert_eq!(*s.marks.lock().unwrap(), vec![3, 4]);
        if let Some(sink) = &s.sink {
            assert!(
                host_call_yield_resume(&sink.snapshot())
                    .iter()
                    .all(|e| *e != "yielded" || audited && slice_fuel.is_some()),
                "slice fuel なしでは host call の Yielded は出ない"
            );
        }
    }
}

// SHR-AT-05
#[test]
fn wait_checks_at_park_interval_not_in_a_busy_loop() {
    for (audited, slice_fuel) in paths() {
        let s = setup(Mode::Pending, audited, slice_fuel);
        let mut ctx = EmbeddingContext::new(&s.engine);
        let clock = Arc::new(TestClock::new(0));
        let dyn_clock: Arc<dyn MonotonicClock> = clock.clone();
        let token = CancellationToken::new();
        let canceller = {
            let token = token.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(200));
                token.cancel();
            })
        };
        let started = Instant::now();
        let outcome = run(
            &s,
            "let r = fetch()\n",
            &mut ctx,
            request(dyn_clock, Some(token)),
        );
        let elapsed = started.elapsed();
        canceller.join().unwrap();
        assert!(
            matches!(outcome, EmbeddingOutcome::Cancelled { .. }),
            "{outcome:?}"
        );
        // park の上限（実装は 5 ms）ごとにだけ確かめる。busy loop なら数十万回になる。
        let intervals = (elapsed.as_millis() / 5) as usize + 1;
        let calls = clock.calls.load(Ordering::SeqCst);
        assert!(
            calls <= intervals * 3 + 50,
            "now() が {calls} 回（待ち {elapsed:?}、上限 {} 回）",
            intervals * 3 + 50
        );
        let _ = take_completer();
    }
}

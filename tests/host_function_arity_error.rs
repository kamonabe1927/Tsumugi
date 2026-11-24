//! 登録 host function の arity 不一致の error の文言を固定する（loop:drift #503 D-152、Q1=A）。
//!
//! 正本は `docs/semantic-decisions.md` §3.4 の「登録 host function の arity 不一致」の行と、
//! `docs/language-spec.md` の canonical 表の同じ行。kind は `argument`（catch 可能。
//! `docs/capability-model.md` §11.2 規則 1）、message は
//! `{name} の引数個数が契約と一致しません: 実際 {actual}`、message_id は `argument.host_function_arity`。
//! 同期の host function（`Exact`・`Range`）と cooperative host function の両方の call site で確かめる。
//! host function は Engine API の tree 経路だけで動く（VM には host registry の経路が無い）。

use std::num::NonZeroU128;
use std::sync::Arc;

use tsumugi::error::{ErrorKind, TsumugiError};
use tsumugi::stable_error::StableError;
use tsumugi::{
    AuditValuePolicy, BudgetConfig, CapabilityCallContext, CapabilitySet, CompileOptions,
    CooperativeHostFunction, EmbeddingContext, EmbeddingEngine, EmbeddingOutcome, EmbeddingRequest,
    FakeClock, HostArity, HostCallError, HostCallPoll, HostCost, HostFunction,
    HostFunctionDescriptor, HostFunctionId, HostFunctionRegistry, LinkRequest, MonotonicClock,
    Source, SourceId, Value,
};

fn host_id(n: u128) -> HostFunctionId {
    HostFunctionId::new(NonZeroU128::new(n).unwrap())
}

fn descriptor(id: u128, name: &str, arity: HostArity) -> HostFunctionDescriptor {
    HostFunctionDescriptor {
        id: host_id(id),
        name: name.to_string(),
        arity,
        cost: HostCost::default(),
        argument_audit: Vec::new(),
        result_audit: AuditValuePolicy::Omit,
        may_block: false,
    }
}

/// 同期の host function。呼ばれたら `Int(1)` を返す。
struct SyncFn(HostFunctionDescriptor);

impl HostFunction for SyncFn {
    fn descriptor(&self) -> &HostFunctionDescriptor {
        &self.0
    }

    fn call(&self, _arguments: &[Value]) -> Result<Value, HostCallError> {
        Ok(Value::Int(1))
    }
}

/// cooperative host function。呼ばれたら `Ready(Int(1))` を返す。
struct Coop(HostFunctionDescriptor);

impl CooperativeHostFunction for Coop {
    fn descriptor(&self) -> &HostFunctionDescriptor {
        &self.0
    }

    fn start(
        &self,
        _context: &mut CapabilityCallContext<'_>,
        _arguments: &[Value],
    ) -> HostCallPoll<Value> {
        HostCallPoll::Ready(Ok(Value::Int(1)))
    }
}

fn engine() -> EmbeddingEngine {
    let registry = HostFunctionRegistry::builder()
        .register(Arc::new(SyncFn(descriptor(
            1,
            "lookup",
            HostArity::Exact(1),
        ))))
        .expect("register lookup")
        .register(Arc::new(SyncFn(descriptor(
            2,
            "between",
            HostArity::Range { min: 1, max: 2 },
        ))))
        .expect("register between")
        .register_cooperative(Arc::new(Coop(descriptor(3, "fetch", HostArity::Exact(0)))))
        .expect("register fetch")
        .build()
        .expect("build registry");
    EmbeddingEngine::builder()
        .host_functions(registry)
        .build()
        .expect("build engine")
}

fn caps() -> CapabilitySet {
    CapabilitySet::builder()
        .grant_host_function(host_id(1))
        .unwrap()
        .grant_host_function(host_id(2))
        .unwrap()
        .grant_host_function(host_id(3))
        .unwrap()
        .build()
}

fn run(engine: &EmbeddingEngine, text: &'static str) -> EmbeddingOutcome {
    let script = engine
        .compile(
            Source::new(SourceId::new("m").unwrap(), text),
            &CompileOptions {
                retain_source: true,
            },
        )
        .expect("compile");
    let link_clock = FakeClock::new();
    let link_budget = BudgetConfig::standard(&link_clock).expect("budget");
    let linked = engine
        .link(
            &script,
            LinkRequest::new(
                tsumugi::ExecutionId::new(1u128.try_into().unwrap()),
                CapabilitySet::empty(),
                link_budget,
            ),
        )
        .expect("link");
    let clock: Arc<dyn MonotonicClock> = Arc::new(FakeClock::new());
    let budget = BudgetConfig::standard(clock.as_ref()).expect("standard budget");
    let request = EmbeddingRequest::new(budget, clock)
        .expect("同 domain")
        .with_capabilities(caps());
    let mut ctx = EmbeddingContext::new(engine);
    engine.run(&linked, &mut ctx, request)
}

/// 未捕捉の arity 不一致が kind `argument` と canonical message の `RuntimeError` になる。
#[test]
fn host_function_arity_mismatch_message_is_canonical() {
    let engine = engine();
    let cases: [(&'static str, &str); 4] = [
        (
            "lookup()\n",
            "lookup の引数個数が契約と一致しません: 実際 0",
        ),
        (
            "lookup(1, 2)\n",
            "lookup の引数個数が契約と一致しません: 実際 2",
        ),
        (
            "between(1, 2, 3)\n",
            "between の引数個数が契約と一致しません: 実際 3",
        ),
        ("fetch(1)\n", "fetch の引数個数が契約と一致しません: 実際 1"),
    ];
    for (source, message) in cases {
        match run(&engine, source) {
            EmbeddingOutcome::RuntimeError { error, .. } => {
                assert_eq!(error.code, ErrorKind::Argument, "{source}");
                assert_eq!(error.safe_message, message, "{source}");
                assert_eq!(error.line, Some(1), "{source}");
            }
            other => panic!("{source}: RuntimeError を期待したが {other:?}"),
        }
    }

    // catch 可能（capability-model §11.2 規則 1）: script の try / catch で捕まえて続行できる。
    let caught = run(
        &engine,
        "let t = \"\"\ntry\n    lookup()\ncatch e\n    t = e[\"type\"] + \" | \" + e[\"message\"]\nend\nif t != \"argument | lookup の引数個数が契約と一致しません: 実際 0\"\n    lookup()\nend\n",
    );
    assert!(
        matches!(caught, EmbeddingOutcome::Completed { .. }),
        "{caught:?}"
    );

    // message_id は `argument.host_function_arity`（`src/stable_error.rs`）。
    let err = TsumugiError::host_function_arity(1, "lookup", 0);
    let stable = StableError::project(&err, tsumugi::embedding::ModuleId::new("m").unwrap())
        .expect("catch 可能 error は射影される");
    assert_eq!(stable.message_id.as_str(), "argument.host_function_arity");
    assert_eq!(stable.script_type, "argument");
}

#![cfg(feature = "jit")]
//! Compiled native continuations parked behind the Green Baton.
//!
//! Native witnesses make an interpreter fallback fail the fixture instead of
//! silently certifying it. Every fixture explicitly joins before propagating
//! an assertion panic. No process-global knob is changed.

use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::error::{EvalResult, Flow};
use crate::emacs_core::error::{LispCondition, signal};
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::jit::{
    cache,
    compile::{NativeRun, ProfitGateTestPolicy, replace_profit_gate_for_test},
};
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::{FixedMin1, SubrSpec};
use crate::emacs_core::value::LambdaParams;
use crate::emacs_core::value::Value;
use std::marker::PhantomData;
use std::num::NonZeroUsize;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::rc::Rc;

#[derive(Debug)]
#[must_use = "retain the exact owner-local profitability policy through compiled execution"]
struct ProfitFixtureScope {
    previous: ProfitGateTestPolicy,
    _confined: PhantomData<Rc<()>>,
}
static_assertions::assert_not_impl_any!(ProfitFixtureScope: Send, Sync);

impl ProfitFixtureScope {
    fn enter() -> Self {
        Self {
            previous: replace_profit_gate_for_test(ProfitGateTestPolicy::Bypass),
            _confined: PhantomData,
        }
    }
}

impl Drop for ProfitFixtureScope {
    fn drop(&mut self) {
        replace_profit_gate_for_test(self.previous);
    }
}

#[derive(Clone, Copy, Debug)]
enum PreviousTier {
    ForcedInterpreter,
    RuntimePolicy,
}
static_assertions::assert_impl_all!(PreviousTier: Send, Sync);

#[derive(Debug)]
#[must_use = "retain the owner-local JIT policy until the fixture finishes"]
struct JitFixtureScope {
    previous: PreviousTier,
    _confined: PhantomData<Rc<()>>,
}
static_assertions::assert_not_impl_any!(JitFixtureScope: Send, Sync);

impl JitFixtureScope {
    fn enter() -> Self {
        let previous = if crate::emacs_core::jit::jit_forced_off_for_test() {
            PreviousTier::ForcedInterpreter
        } else {
            PreviousTier::RuntimePolicy
        };
        crate::emacs_core::jit::force_jit_off_for_test(false);
        Self {
            previous,
            _confined: PhantomData,
        }
    }
}

impl Drop for JitFixtureScope {
    fn drop(&mut self) {
        let forced = match self.previous {
            PreviousTier::ForcedInterpreter => true,
            PreviousTier::RuntimePolicy => false,
        };
        crate::emacs_core::jit::force_jit_off_for_test(forced);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StackLimit(NonZeroUsize);
static_assertions::assert_impl_all!(StackLimit: Send, Sync);

/// Metadata only, with no method that reconstructs an executable pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LeafIdentity(NonZeroUsize);
static_assertions::assert_impl_all!(LeafIdentity: Send, Sync);

impl From<LeafIdentity> for Value {
    fn from(identity: LeafIdentity) -> Self {
        Value::make_int(identity.0.get() as i64)
    }
}

fn with_compiled_green(action: impl FnOnce(&mut Context)) {
    crate::test_utils::init_test_tracing();
    let _scope = JitFixtureScope::enter();
    let _profit = ProfitFixtureScope::enter();
    assert!(crate::emacs_core::jit::jit_runtime_enabled());
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.set_lexical_binding(true);
    context.register_subr(SubrSpec::fixed0("f5-jit-witness", native_witness));
    context.register_subr(SubrSpec::fixed0("f5-jit-allocate", native_allocate));
    context.register_subr(SubrSpec::fixed1(
        "f5-jit-argument-yield",
        native_argument_yield,
        FixedMin1::One,
    ));
    context.register_subr(SubrSpec::fixed0("f5-jit-signal", native_signal));
    context.register_subr(SubrSpec::fixed0("f5-jit-throw", native_throw));
    context.register_subr(SubrSpec::fixed0("f5-jit-worker-deopt", worker_deopt));
    context.register_subr(SubrSpec::fixed0("f5-jit-stack-limit", native_stack_limit));
    context.register_subr(SubrSpec::fixed0("f5-jit-leaf-address", native_leaf_address));
    context.register_subr(SubrSpec::fixed0("f5-worker-limit", worker_limit));
    context.register_subr(SubrSpec::fixed0("f5-worker-leaf", worker_leaf));
    context.register_subr(SubrSpec::fixed0("f5-worker-error", worker_error));
    context.register_subr(SubrSpec::fixed0("f5-worker-binding", worker_binding));
    context.register_subr(SubrSpec::fixed0("f5-worker-gc-loop", worker_gc_loop));
    context.eval_str("(setq f5-jit-witness-count 0)").unwrap();
    let result = catch_unwind(AssertUnwindSafe(|| action(&mut context)));
    context.green_teardown();
    if context.green_key().is_some() {
        context.with_green_runtime(|runtime| {
            assert!(
                runtime.parked.borrow().is_empty(),
                "all parked batches retired"
            );
        });
    }
    assert_value(&mut context, "(length (all-threads))", "1");
    assert!(context.specpdl.is_empty());
    assert_eq!(context.jit_root_stack_top, 0);
    assert_eq!(cache::native_depth(), 0);
    if let Err(payload) = result {
        resume_unwind(payload);
    }
}

#[derive(Clone, Copy, Debug)]
enum FixtureArity {
    Nullary,
    Unary,
}
static_assertions::assert_impl_all!(FixtureArity: Send, Sync);

fn bytecode(ops: Vec<Op>, constants: Vec<Value>, arity: FixtureArity) -> ByteCodeFunction {
    let required = match arity {
        FixtureArity::Nullary => Vec::new(),
        FixtureArity::Unary => vec![crate::emacs_core::intern::intern("f5-jit-arg")],
    };
    let mut function = ByteCodeFunction::new(LambdaParams::simple(required));
    function.lexical = true;
    function.ops = ops;
    function.constants = constants.into();
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    function.seal_hand_assembled_ops();
    // Call-bearing fixtures enter through the compiled policy rather than the
    // ordinary cold dispatcher. A separate scoped override bypasses their
    // first-probe profitability refusal without changing the global knob.
    function
        .jit_runtime()
        .set_heat_for_test(crate::emacs_core::jit::hot_threshold().saturating_mul(64));
    function
}

fn install(context: &mut Context, name: &str, function: ByteCodeFunction) -> Value {
    let value = Value::make_bytecode(function);
    let symbol = crate::emacs_core::intern::intern(name);
    context.with_obarray_mut(|mut obarray| obarray.set_symbol_function_id(symbol, value));
    value
}

fn assert_compiled(context: &Context, function: Value) {
    assert!(context.tagged_heap.owns_heap_value_for_test(function));
    let id = function
        .get_bytecode_data()
        .expect("source object survives every parked collection")
        .jit_runtime()
        .compiled_id()
        .expect("real compiled-entry probe assigned an identity");
    assert!(
        cache::is_compiled_for_test(id),
        "real compiled leaf remains cached"
    );
}

fn assert_value(context: &mut Context, source: &str, expected: &str) {
    let value = context.eval_str(source).unwrap_or_else(|error| {
        panic!("compiled Green fixture evaluates: {error}; source: {source}")
    });
    assert_eq!(print_value(&value), expected);
}

fn require_native() -> Result<(), Flow> {
    if cache::native_depth() == 0 {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string(
                "F5 witness was reached without a compiled native frame",
            )],
        ));
    }
    Ok(())
}

fn native_witness(context: &mut Context) -> EvalResult {
    require_native()?;
    let count = context
        .visible_variable_value_or_nil("f5-jit-witness-count")
        .as_int()
        .unwrap_or_default();
    context.set_variable("f5-jit-witness-count", Value::fixnum(count + 1));
    Ok(Value::T)
}

fn native_allocate(_context: &mut Context) -> EvalResult {
    require_native()?;
    Ok(Value::cons(
        Value::fixnum(793),
        Value::string("parked-jit-only"),
    ))
}

fn native_argument_yield(context: &mut Context, argument: Value) -> EvalResult {
    require_native()?;
    context.apply(Value::symbol("thread-yield"), Vec::new())?;
    if !context.tagged_heap.owns_heap_value_for_test(argument) {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string("lost native/JIT argument root")],
        ));
    }
    Ok(argument)
}

fn native_signal(_context: &mut Context) -> EvalResult {
    require_native()?;
    Err(signal(
        LispCondition::Error,
        vec![Value::string("f5 compiled signal")],
    ))
}

fn native_throw(context: &mut Context) -> EvalResult {
    require_native()?;
    context.apply(
        Value::symbol("throw"),
        vec![
            Value::symbol("f5-jit-tag"),
            Value::cons(Value::fixnum(887), Value::NIL),
        ],
    )
}

fn worker_deopt(context: &mut Context) -> EvalResult {
    let _profit = ProfitFixtureScope::enter();
    let function = Value::make_bytecode(bytecode(
        // The native witness forces the baseline's precise continuation:
        // a pure MIR Add1 may instead return the valid rerun-from-start
        // fallback, which does not test B's deopt frame materialization.
        vec![
            Op::Constant(0),
            Op::Call(0),
            Op::Pop,
            Op::StackRef(0),
            Op::Add1,
            Op::Return,
        ],
        vec![Value::symbol("f5-jit-witness")],
        FixtureArity::Unary,
    ));
    let roots = context.retain_gc_roots([function]);
    let source = function
        .get_bytecode_data()
        .expect("worker arithmetic source");
    let before = crate::emacs_core::jit::stats::compile_stats_snapshot().deopts();
    let precise_before = cache::native_run_counters().2;
    let answer = cache::try_run_compiled(
        context,
        source,
        function,
        &[Value::fixnum(Value::MOST_POSITIVE_FIXNUM)],
    )?;
    let Some(bits) = answer else {
        return Err(signal(
            LispCondition::Error,
            vec![
                Value::string("worker precise-deopt fixture returned a rerun fallback"),
                Value::make_int(before as i64),
                Value::make_int(
                    crate::emacs_core::jit::stats::compile_stats_snapshot().deopts() as i64,
                ),
            ],
        ));
    };
    let value = Value::from_bits(bits);
    let after = crate::emacs_core::jit::stats::compile_stats_snapshot().deopts();
    let precise_after = cache::native_run_counters().2;
    if after <= before
        || precise_after <= precise_before
        || print_value(&value) != (Value::MOST_POSITIVE_FIXNUM + 1).to_string()
    {
        return Err(signal(
            LispCondition::Error,
            vec![
                Value::string("worker did not take precise overflow deopt"),
                value,
                Value::make_int(before as i64),
                Value::make_int(after as i64),
                Value::make_int(precise_before as i64),
                Value::make_int(precise_after as i64),
            ],
        ));
    }
    drop(roots);
    Ok(Value::T)
}

fn native_stack_limit(context: &mut Context) -> EvalResult {
    require_native()?;
    let Some(limit) = NonZeroUsize::new(context.jit_stack_limit).map(StackLimit) else {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string("owner JIT stack limit was not installed")],
        ));
    };
    let here = crate::emacs_core::eval::native_stack::jit_stack_limit_here();
    if here == 0 || limit.0.get().abs_diff(here) > 65_536 {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string("JIT limit names a foreign native stack")],
        ));
    }
    Ok(Value::list(vec![
        Value::make_int(context.context_instance_id as i64),
        Value::make_int(limit.0.get() as i64),
    ]))
}

fn native_leaf_address(context: &mut Context) -> EvalResult {
    require_native()?;
    let source = context
        .obarray
        .symbol_function("f5-jit-owner-leaf")
        .and_then(Value::get_bytecode_data)
        .ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("missing shared source")],
            )
        })?;
    let pointer = source
        .jit_runtime()
        .compiled_id()
        .and_then(cache::compiled_leaf_ptr_for_test)
        .ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("no leaf in the running owner's TLS")],
            )
        })?;
    // The integer is a diagnostic identity only: it is never reconstructed or
    // dereferenced, and no executable/Context pointer crosses a scheduler queue.
    let identity = NonZeroUsize::new(pointer as usize)
        .map(LeafIdentity)
        .ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("cached leaf identity is null")],
            )
        })?;
    Ok(Value::from(identity))
}

#[derive(Clone, Copy, Debug)]
enum WorkerTask {
    Limit,
    Leaf,
    Signal,
    Binding,
}
static_assertions::assert_impl_all!(WorkerTask: Send, Sync);

/// The worker constructs its own profitability scope on B's OS thread. The
/// borrowed native parameter belongs to B and remains on B's original stack;
/// the witness in the callee still requires an actual compiled native frame.
fn run_worker_task(context: &mut Context, task: WorkerTask) -> EvalResult {
    let _profit = ProfitFixtureScope::enter();
    let (function, arguments) = match task {
        WorkerTask::Limit => ("f5-jit-limit", Vec::new()),
        WorkerTask::Leaf => ("f5-jit-owner-leaf", Vec::new()),
        WorkerTask::Signal => ("f5-jit-worker-error", Vec::new()),
        WorkerTask::Binding => ("f5-jit-own-binding", vec![Value::symbol("worker")]),
    };
    context.apply(Value::symbol(function), arguments)
}

fn worker_limit(context: &mut Context) -> EvalResult {
    run_worker_task(context, WorkerTask::Limit)
}
fn worker_leaf(context: &mut Context) -> EvalResult {
    run_worker_task(context, WorkerTask::Leaf)
}
fn worker_error(context: &mut Context) -> EvalResult {
    run_worker_task(context, WorkerTask::Signal)
}
fn worker_binding(context: &mut Context) -> EvalResult {
    run_worker_task(context, WorkerTask::Binding)
}

fn yield_then(name: &str) -> ByteCodeFunction {
    bytecode(
        vec![
            Op::Constant(0),
            Op::Call(0),
            Op::Pop,
            Op::Constant(1),
            Op::Call(0),
            Op::Return,
        ],
        vec![Value::symbol("thread-yield"), Value::symbol(name)],
        FixtureArity::Nullary,
    )
}

#[derive(Clone, Copy, Debug)]
enum BlockingCall {
    Join,
    Mutex,
    Condition,
}
static_assertions::assert_impl_all!(BlockingCall: Send, Sync);

/// The argument designator stays at the bottom of the original JIT frame.
/// The allocation above it is otherwise reachable only from that live frame
/// while the blocking call publishes the parked owner's immutable snapshot.
fn compiled_blocking_call(operation: BlockingCall) -> ByteCodeFunction {
    let function = match operation {
        BlockingCall::Join => "thread-join",
        BlockingCall::Mutex => "mutex-lock",
        BlockingCall::Condition => "condition-wait",
    };
    let mut operations = vec![
        Op::Constant(0),
        Op::Call(0),
        Op::Constant(1),
        Op::Call(0),
        Op::Pop,
        Op::Constant(2),
        Op::StackRef(2),
        Op::Call(1),
        Op::Pop,
        Op::Constant(1),
        Op::Call(0),
        Op::Pop,
    ];
    match operation {
        BlockingCall::Mutex => {
            operations.extend([Op::Constant(3), Op::StackRef(2), Op::Call(1), Op::Pop])
        }
        BlockingCall::Join | BlockingCall::Condition => {}
    }
    operations.push(Op::Return);
    bytecode(
        operations,
        vec![
            Value::symbol("f5-jit-allocate"),
            Value::symbol("f5-jit-witness"),
            Value::symbol(function),
            Value::symbol("mutex-unlock"),
        ],
        FixtureArity::Unary,
    )
}

fn assert_parked_payload(context: &Context, value: Value) {
    assert!(context.tagged_heap.owns_heap_value_for_test(value));
    assert_eq!(value.cons_car(), Value::fixnum(793));
    assert_eq!(print_value(&value.cons_cdr()), "\"parked-jit-only\"");
}

#[test]
fn green_jit_switch_compiled_join_publishes_frame_until_target_exits() {
    with_compiled_green(|context| {
        let function = install(
            context,
            "f5-jit-joining",
            compiled_blocking_call(BlockingCall::Join),
        );
        context
            .eval_str(
                "(setq f5-jit-main (current-thread) f5-jit-observed nil
                  f5-jit-worker
                  (make-thread (lambda ()
                    (setq f5-jit-observed
                          (and (= f5-jit-witness-count 1)
                               (eq (thread--blocker f5-jit-main) (current-thread))))
                    (garbage-collect) 'joined)))",
            )
            .unwrap();
        let answer = context.eval_str("(f5-jit-joining f5-jit-worker)").unwrap();
        assert_parked_payload(context, answer);
        assert_compiled(context, function);
        assert_value(
            context,
            "(list f5-jit-observed (thread-join f5-jit-worker)
                   f5-jit-witness-count (thread-live-p f5-jit-worker))",
            "(t joined 2 nil)",
        );
    });
}

#[test]
fn green_jit_switch_compiled_mutex_lock_resumes_with_collected_frame() {
    with_compiled_green(|context| {
        let function = install(
            context,
            "f5-jit-locking",
            compiled_blocking_call(BlockingCall::Mutex),
        );
        context
            .eval_str(
                "(setq f5-jit-mutex (make-mutex \"f5-compiled-mutex\")
                       f5-jit-ready nil f5-jit-observed nil
                       f5-jit-worker
                       (make-thread (lambda ()
                         (mutex-lock f5-jit-mutex)
                         (setq f5-jit-ready t)
                         (thread-yield)
                         (setq f5-jit-observed (= f5-jit-witness-count 1))
                         (garbage-collect)
                         (mutex-unlock f5-jit-mutex) 'released)))
                 (while (null f5-jit-ready) (thread-yield))",
            )
            .unwrap();
        // B owns the mutex and is suspended at its explicit yield. It can
        // resume only when A's actual compiled mutex-lock blocks.
        let answer = context.eval_str("(f5-jit-locking f5-jit-mutex)").unwrap();
        assert_parked_payload(context, answer);
        assert_compiled(context, function);
        assert_value(
            context,
            "(list f5-jit-observed (thread-join f5-jit-worker) f5-jit-witness-count)",
            "(t released 2)",
        );
    });
}

#[test]
fn green_jit_switch_compiled_condition_wait_restores_recursive_depth() {
    with_compiled_green(|context| {
        let function = install(
            context,
            "f5-jit-waiting",
            compiled_blocking_call(BlockingCall::Condition),
        );
        context
            .eval_str(
                "(setq f5-jit-mutex (make-mutex \"f5-compiled-condition-mutex\")
                       f5-jit-condition
                       (make-condition-variable f5-jit-mutex \"f5-compiled-condition\")
                       f5-jit-observed nil f5-jit-reacquired nil)
                 (mutex-lock f5-jit-mutex)
                 (mutex-lock f5-jit-mutex)
                 (setq f5-jit-worker
                       (make-thread (lambda ()
                         (mutex-lock f5-jit-mutex)
                         (setq f5-jit-observed (= f5-jit-witness-count 1))
                         (garbage-collect)
                         (condition-notify f5-jit-condition)
                         (mutex-unlock f5-jit-mutex)
                         (thread-yield)
                         (mutex-lock f5-jit-mutex)
                         (setq f5-jit-reacquired t)
                         (mutex-unlock f5-jit-mutex) 'notified)))",
            )
            .unwrap();
        let answer = context
            .eval_str("(f5-jit-waiting f5-jit-condition)")
            .unwrap();
        assert_parked_payload(context, answer);
        assert_compiled(context, function);
        // Notify does not switch owners. B explicitly yields after unlocking.
        // A's condition-wait must restore BOTH recursive acquisitions: after
        // one unlock, B's second lock remains blocked until A unlocks again.
        assert_value(
            context,
            "(progn (mutex-unlock f5-jit-mutex) (thread-yield) f5-jit-reacquired)",
            "nil",
        );
        assert_value(
            context,
            "(progn (mutex-unlock f5-jit-mutex)
                    (list (thread-join f5-jit-worker) f5-jit-observed
                          f5-jit-reacquired f5-jit-witness-count))",
            "(notified t t 2)",
        );
    });
}

#[test]
fn green_jit_switch_compiled_mid_loop_resumes_original_frame() {
    with_compiled_green(|context| {
        let function = install(
            context,
            "f5-jit-loop",
            bytecode(
                vec![
                    Op::Constant(0),
                    Op::StackRef(0),
                    Op::Constant(1),
                    Op::Lss,
                    Op::GotoIfNil(13),
                    Op::Constant(2),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(3),
                    Op::Call(0),
                    Op::Pop,
                    Op::Add1,
                    Op::Goto(1),
                    Op::Return,
                ],
                vec![
                    Value::fixnum(0),
                    Value::fixnum(4),
                    Value::symbol("thread-yield"),
                    Value::symbol("f5-jit-witness"),
                ],
                FixtureArity::Nullary,
            ),
        );
        context
            .eval_str("(setq f5-jit-worker (make-thread (lambda () (thread-yield) 'worker)))")
            .unwrap();
        assert_value(
            context,
            "(list (f5-jit-loop) (thread-join f5-jit-worker) f5-jit-witness-count)",
            "(4 worker 4)",
        );
        assert_compiled(context, function);
    });
}

#[test]
fn green_jit_switch_parked_operand_is_a_real_collection_root() {
    with_compiled_green(|context| {
        let function = install(
            context,
            "f5-jit-operand",
            bytecode(
                vec![
                    Op::Constant(0),
                    Op::Call(0),
                    Op::Constant(1),
                    Op::Call(0),
                    Op::Pop,
                    Op::Return,
                ],
                vec![
                    Value::symbol("f5-jit-allocate"),
                    Value::symbol("thread-yield"),
                ],
                FixtureArity::Nullary,
            ),
        );
        context
            .eval_str("(setq f5-jit-worker (make-thread (lambda () (garbage-collect) 'collected)))")
            .unwrap();
        let answer = context.eval_str("(f5-jit-operand)").unwrap();
        assert!(context.tagged_heap.owns_heap_value_for_test(answer));
        assert_eq!(answer.cons_car(), Value::fixnum(793));
        assert_eq!(print_value(&answer.cons_cdr()), "\"parked-jit-only\"");
        assert_compiled(context, function);
        assert_value(context, "(thread-join f5-jit-worker)", "collected");
    });
}

#[test]
fn green_jit_switch_native_argument_is_rooted_through_parked_collection() {
    with_compiled_green(|context| {
        let function = install(
            context,
            "f5-jit-argument",
            bytecode(
                vec![
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Call(0),
                    Op::Call(1),
                    Op::Return,
                ],
                vec![
                    Value::symbol("f5-jit-argument-yield"),
                    Value::symbol("f5-jit-allocate"),
                ],
                FixtureArity::Nullary,
            ),
        );
        context
            .eval_str("(setq f5-jit-worker (make-thread (lambda () (garbage-collect))))")
            .unwrap();
        let answer = context.eval_str("(f5-jit-argument)").unwrap();
        assert!(context.tagged_heap.owns_heap_value_for_test(answer));
        assert_eq!(answer.cons_car(), Value::fixnum(793));
        assert_compiled(context, function);
        context.eval_str("(thread-join f5-jit-worker)").unwrap();
    });
}

#[test]
fn green_jit_switch_redefinition_twice_keeps_old_source_and_code_alive() {
    with_compiled_green(|context| {
        let old = install(
            context,
            "f5-jit-redefined",
            bytecode(
                vec![
                    Op::Constant(0),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(0),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(1),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(2),
                    Op::Return,
                ],
                vec![
                    Value::symbol("thread-yield"),
                    Value::symbol("f5-jit-witness"),
                    Value::cons(Value::fixnum(997), Value::string("old-source")),
                ],
                FixtureArity::Nullary,
            ),
        );
        context
            .eval_str(
                "(setq f5-jit-worker
              (make-thread (lambda ()
                (fset 'f5-jit-redefined (lambda () 'first))
                (garbage-collect) (thread-yield)
                (fset 'f5-jit-redefined (lambda () 'second))
                (garbage-collect) 'redefined)))",
            )
            .unwrap();
        let answer = context.eval_str("(f5-jit-redefined)").unwrap();
        assert!(
            context.tagged_heap.owns_heap_value_for_test(old),
            "snapshot/pin retains old bytecode; Rust locals are not roots"
        );
        assert!(context.tagged_heap.owns_heap_value_for_test(answer));
        assert_eq!(answer.cons_car(), Value::fixnum(997));
        assert_eq!(print_value(&answer.cons_cdr()), "\"old-source\"");
        assert_value(
            context,
            "(list (thread-join f5-jit-worker) (f5-jit-redefined))",
            "(redefined second)",
        );
    });
}

#[test]
fn green_jit_switch_worker_precise_deopt_does_not_read_parked_frames() {
    with_compiled_green(|context| {
        let function = install(context, "f5-jit-parked", yield_then("f5-jit-witness"));
        context
            .eval_str("(setq f5-jit-worker (make-thread (lambda () (f5-jit-worker-deopt))))")
            .unwrap();
        assert_value(
            context,
            "(list (f5-jit-parked) (thread-join f5-jit-worker)
                   (thread-last-error f5-jit-worker))",
            "(t t nil)",
        );
        assert_compiled(context, function);
    });
}

#[test]
fn green_jit_switch_osr_entry_yields_with_its_own_native_roots() {
    with_compiled_green(|context| {
        let function = install(
            context,
            "f5-jit-osr",
            bytecode(
                vec![
                    Op::Constant(0),
                    Op::StackRef(0),
                    Op::StackRef(2),
                    Op::Lss,
                    Op::GotoIfNil(13),
                    Op::Constant(1),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(2),
                    Op::Call(0),
                    Op::Pop,
                    Op::Add1,
                    Op::Goto(1),
                    Op::Return,
                ],
                vec![
                    Value::fixnum(0),
                    Value::symbol("thread-yield"),
                    Value::symbol("f5-jit-witness"),
                ],
                FixtureArity::Unary,
            ),
        );
        context
            .eval_str("(setq f5-jit-worker (make-thread (lambda () (garbage-collect) 'worker)))")
            .unwrap();
        let source = function.get_bytecode_data().unwrap();
        let run = cache::try_run_osr(
            context,
            source,
            1,
            &[Value::fixnum(3), Value::fixnum(0)],
            &[],
        )
        .expect("the exact header snapshot enters a real OSR leaf");
        match run {
            NativeRun::Ok(bits) => assert_eq!(Value::from_bits(bits), Value::fixnum(3)),
            NativeRun::Deopt | NativeRun::DeoptAt(_) | NativeRun::Signal => {
                panic!("OSR yield should return normally: {run:?}");
            }
        }
        assert!(cache::osr_leaf_ptr_for_test(source, 1).is_some());
        assert_value(
            context,
            "(list f5-jit-witness-count (thread-join f5-jit-worker))",
            "(3 worker)",
        );
    });
}

#[test]
fn green_jit_switch_compiled_signal_restores_dynamic_bindings() {
    with_compiled_green(|context| {
        context
            .eval_str("(defvar f5-jit-dynamic 'canonical)")
            .unwrap();
        let function = install(
            context,
            "f5-jit-signalling",
            bytecode(
                vec![
                    Op::Constant(1),
                    Op::VarBind(0),
                    Op::Constant(2),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(3),
                    Op::Call(0),
                    Op::Unbind(1),
                    Op::Return,
                ],
                vec![
                    Value::symbol("f5-jit-dynamic"),
                    Value::cons(Value::fixnum(773), Value::NIL),
                    Value::symbol("thread-yield"),
                    Value::symbol("f5-jit-signal"),
                ],
                FixtureArity::Nullary,
            ),
        );
        context
            .eval_str(
                "(setq f5-jit-worker (make-thread (lambda () (garbage-collect) f5-jit-dynamic)))",
            )
            .unwrap();
        assert_value(
            context,
            "(list (condition-case nil (f5-jit-signalling) (error 'caught))
                   f5-jit-dynamic (thread-join f5-jit-worker))",
            "(caught canonical canonical)",
        );
        assert_compiled(context, function);
    });
}

#[test]
fn green_jit_switch_compiled_throw_runs_own_cleanup_after_resume() {
    with_compiled_green(|context| {
        let function = install(context, "f5-jit-throwing", yield_then("f5-jit-throw"));
        context.eval_str("(setq f5-jit-cleaned nil f5-jit-worker (make-thread (lambda () (garbage-collect) nil)))").unwrap();
        assert_value(
            context,
            "(list (unwind-protect (catch 'f5-jit-tag (f5-jit-throwing))
                     (setq f5-jit-cleaned t))
                   f5-jit-cleaned (thread-join f5-jit-worker))",
            "((887) t nil)",
        );
        assert_compiled(context, function);
    });
}

#[test]
fn green_jit_switch_per_owner_stack_limits_survive_handoff() {
    with_compiled_green(|context| {
        install(context, "f5-jit-limit", yield_then("f5-jit-stack-limit"));
        context
            .eval_str("(setq f5-jit-worker (make-thread #'f5-worker-limit))")
            .unwrap();
        let original_limit = context.jit_stack_limit;
        let main = context.eval_str("(f5-jit-limit)").unwrap();
        let main_root = context.retain_gc_roots([main]);
        let worker = context.eval_str("(thread-join f5-jit-worker)").unwrap();
        assert_ne!(
            main.cons_car(),
            worker.cons_car(),
            "private owner identities"
        );
        assert_ne!(
            main.cons_cdr().cons_car(),
            worker.cons_cdr().cons_car(),
            "different native OS stacks"
        );
        assert_eq!(
            context.jit_stack_limit, original_limit,
            "only original returning owner refreshes its limit"
        );
        drop(main_root);
    });
}

#[test]
fn green_jit_switch_shared_source_has_distinct_owner_local_leaves() {
    with_compiled_green(|context| {
        install(
            context,
            "f5-jit-owner-leaf",
            yield_then("f5-jit-leaf-address"),
        );
        context
            .eval_str("(setq f5-jit-worker (make-thread #'f5-worker-leaf))")
            .unwrap();
        let main = context.eval_str("(f5-jit-owner-leaf)").unwrap();
        let worker = context.eval_str("(thread-join f5-jit-worker)").unwrap();
        assert_ne!(
            main, worker,
            "World source identity is shared; executable leaf ownership is native-TLS local"
        );
    });
}

#[test]
fn green_jit_switch_compiled_worker_error_owns_terminal_flow() {
    with_compiled_green(|context| {
        install(context, "f5-jit-worker-error", yield_then("f5-jit-signal"));
        assert_value(
            context,
            "(progn (thread-last-error t)
               (let ((worker (make-thread #'f5-worker-error)))
                 (list (thread-join worker) (thread-live-p worker)
                       (thread-last-error t) (thread-last-error))))",
            "(nil nil (error \"f5 compiled signal\") nil)",
        );
    });
}

#[test]
fn green_jit_switch_compiled_bindings_belong_to_each_continuation() {
    with_compiled_green(|context| {
        context
            .eval_str("(defvar f5-jit-dynamic 'canonical)")
            .unwrap();
        let function = install(
            context,
            "f5-jit-own-binding",
            bytecode(
                vec![
                    Op::StackRef(0),
                    Op::VarBind(0),
                    Op::Constant(1),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(2),
                    Op::Call(0),
                    Op::Pop,
                    Op::VarRef(0),
                    Op::Unbind(1),
                    Op::Return,
                ],
                vec![
                    Value::symbol("f5-jit-dynamic"),
                    Value::symbol("thread-yield"),
                    Value::symbol("f5-jit-witness"),
                ],
                FixtureArity::Unary,
            ),
        );
        assert_value(
            context,
            "(let ((worker (make-thread #'f5-worker-binding)))
               (list (f5-jit-own-binding 'main) (thread-join worker)
                     f5-jit-dynamic f5-jit-witness-count))",
            "(main worker canonical 2)",
        );
        assert_compiled(context, function);
    });
}

/// Each worker constructs its policy on its own native thread; the returned
/// cons has no Lisp reference while its compiled continuation is parked.
fn worker_gc_loop(context: &mut Context) -> EvalResult {
    let _profit = ProfitFixtureScope::enter();
    context.apply(Value::symbol("f5-jit-gc-loop"), Vec::new())
}

#[test]
fn green_jit_switch_eight_compiled_threads_keep_roots_across_yield_and_gc() {
    with_compiled_green(|context| {
        let function = install(
            context,
            "f5-jit-gc-loop",
            bytecode(
                vec![
                    Op::Constant(0),
                    Op::Call(0),
                    Op::Constant(1),
                    Op::StackRef(0),
                    Op::Constant(2),
                    Op::Lss,
                    Op::GotoIfNil(18),
                    Op::Constant(3),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(4),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(5),
                    Op::Call(0),
                    Op::Pop,
                    Op::Add1,
                    Op::Goto(3),
                    Op::Pop,
                    Op::Return,
                ],
                vec![
                    Value::symbol("f5-jit-allocate"),
                    Value::fixnum(0),
                    Value::fixnum(4),
                    Value::symbol("thread-yield"),
                    Value::symbol("garbage-collect"),
                    Value::symbol("f5-jit-witness"),
                ],
                FixtureArity::Nullary,
            ),
        );
        assert_value(
            context,
            "(let ((threads (mapcar (lambda (_) (make-thread #'f5-worker-gc-loop))
                                    '(1 2 3 4 5 6 7 8))))
               (list (mapcar #'thread-join threads) f5-jit-witness-count
                     (mapcar #'thread-live-p threads) (thread-last-error)))",
            "(((793 . \"parked-jit-only\") (793 . \"parked-jit-only\") (793 . \"parked-jit-only\") (793 . \"parked-jit-only\") (793 . \"parked-jit-only\") (793 . \"parked-jit-only\") (793 . \"parked-jit-only\") (793 . \"parked-jit-only\")) 32 (nil nil nil nil nil nil nil nil) nil)",
        );
        assert!(context.tagged_heap.owns_heap_value_for_test(function));
    });
}

#[test]
fn green_jit_switch_overflow_deopts_after_parked_frame_resumes() {
    with_compiled_green(|context| {
        let function = install(
            context,
            "f5-jit-parked-overflow",
            bytecode(
                vec![
                    Op::Constant(0),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(1),
                    Op::Call(0),
                    Op::Pop,
                    Op::StackRef(0),
                    Op::Add1,
                    Op::Return,
                ],
                vec![
                    Value::symbol("f5-jit-witness"),
                    Value::symbol("thread-yield"),
                ],
                FixtureArity::Unary,
            ),
        );
        context
            .eval_str("(setq f5-jit-worker (make-thread (lambda () (garbage-collect) 'collected)))")
            .unwrap();
        let before = cache::native_run_counters().2;
        let value = context
            .apply(function, vec![Value::fixnum(Value::MOST_POSITIVE_FIXNUM)])
            .unwrap();
        assert_eq!(
            print_value(&value),
            (Value::MOST_POSITIVE_FIXNUM + 1).to_string()
        );
        assert!(
            cache::native_run_counters().2 > before,
            "resumed frame uses precise overflow deopt"
        );
        assert_value(
            context,
            "(list (thread-join f5-jit-worker) f5-jit-witness-count)",
            "(collected 1)",
        );
    });
}

/// Warm both variable accesses before admission, then admit in the middle of
/// the same leaf. Projected cells must divert that old machine code on its
/// next read/write, including the previously cached BLV address.
fn preadmission_variable_leaf(context: &mut Context, buffer_local: bool) {
    context.eval_str("(defvar f5-jit-projected 1)").unwrap();
    if buffer_local {
        context
            .eval_str("(set (make-local-variable 'f5-jit-projected) 10)")
            .unwrap();
    }
    context
        .eval_str(
            "(setq f5-jit-admit-now nil)
         (fset 'f5-jit-admit-mid-frame
               (lambda ()
                 (if f5-jit-admit-now
                     (thread-join
                      (make-thread
                       (lambda ()
                         (if (local-variable-p 'f5-jit-projected)
                             (progn (kill-local-variable 'f5-jit-projected)
                                    (set (make-local-variable 'f5-jit-projected) 30))
                           (setq f5-jit-projected 3))))))))",
        )
        .unwrap();
    let function = install(
        context,
        "f5-jit-projection-leaf",
        bytecode(
            vec![
                Op::VarRef(0),
                Op::Pop,
                Op::Constant(1),
                Op::Call(0),
                Op::Pop,
                Op::Constant(2),
                Op::Call(0),
                Op::Pop,
                Op::StackRef(0),
                Op::VarSet(0),
                Op::VarRef(0),
                Op::Return,
            ],
            vec![
                Value::symbol("f5-jit-projected"),
                Value::symbol("f5-jit-admit-mid-frame"),
                Value::symbol("f5-jit-witness"),
            ],
            FixtureArity::Unary,
        ),
    );
    assert_value(context, "(f5-jit-projection-leaf 11)", "11");
    assert_compiled(context, function);
    assert!(
        !context.threads_admitted(),
        "leaf compiled before admission"
    );
    context
        .eval_str(if buffer_local {
            "(setq f5-jit-projected 10 f5-jit-admit-now t)"
        } else {
            "(setq f5-jit-projected 1 f5-jit-admit-now t)"
        })
        .unwrap();
    assert_value(
        context,
        "(list (let ((f5-jit-projected 20))
                 (list (f5-jit-projection-leaf 9) f5-jit-projected))
               f5-jit-projected)",
        if buffer_local {
            "((9 9) 30)"
        } else {
            "((9 9) 3)"
        },
    );
}

#[test]
fn green_jit_switch_preadmission_inline_var_accesses_divert_after_admission() {
    with_compiled_green(|context| preadmission_variable_leaf(context, false));
}

#[test]
fn green_jit_switch_preadmission_blv_pointer_diverts_after_peer_kill_and_remake() {
    with_compiled_green(|context| preadmission_variable_leaf(context, true));
}

/// Same driver as the 3e JIT-off baseline, with a compiled worker loop and a
/// native witness that prevents an interpreter fallback from passing.
#[test]
#[cfg(feature = "perf-probes")]
fn green_yield_jit_switch_micro() {
    let n: usize = std::env::var("TGY_MICRO_N")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1000);
    with_compiled_green(|context| {
        install(
            context,
            "f5-jit-micro-loop",
            bytecode(
                vec![
                    Op::VarRef(0),
                    Op::GotoIfNotNil(9),
                    Op::Constant(1),
                    Op::Call(0),
                    Op::Pop,
                    Op::Constant(2),
                    Op::Call(0),
                    Op::Pop,
                    Op::Goto(0),
                    Op::Constant(3),
                    Op::Return,
                ],
                vec![
                    Value::symbol("f5-jit-micro-stop"),
                    Value::symbol("thread-yield"),
                    Value::symbol("f5-jit-witness"),
                    Value::NIL,
                ],
                FixtureArity::Nullary,
            ),
        );
        // Keep the normal worker wrapper's owner-local profitability policy.
        context.register_subr(SubrSpec::fixed0("f5-worker-micro", worker_micro));
        assert_value(
            context,
            "(progn (setq f5-jit-micro-stop nil
                           f5-jit-micro-thread (make-thread #'f5-worker-micro))
                    (thread-yield))",
            "nil",
        );
        assert_value(
            context,
            "(list (thread-live-p f5-jit-micro-thread)
                   (thread-last-error f5-jit-micro-thread)
                   f5-jit-witness-count)",
            "(t nil 0)",
        );
        for _ in 0..n {
            context
                .green_wait(crate::emacs_core::eval::green::Wait::Runnable)
                .expect("yield");
        }
        assert_value(
            context,
            "(list (thread-live-p f5-jit-micro-thread)
                   (thread-last-error f5-jit-micro-thread))",
            "(t nil)",
        );
        // The worker calls native_witness once after each measured return
        // from its yield. Its ordinary Lisp counter is World-shared, unlike
        // the JIT's private native TLS counters. Check even N=0 so the health
        // checks remain fixed overhead in the Callgrind subtraction.
        let witnessed = context
            .visible_variable_value_or_nil("f5-jit-witness-count")
            .as_int()
            .expect("the worker's native witness counter is an integer");
        assert_eq!(
            usize::try_from(witnessed).expect("a native witness count is nonnegative"),
            n,
            "each measured worker turn executes its existing native witness"
        );
        assert_value(
            context,
            "(progn (setq f5-jit-micro-stop t) (thread-join f5-jit-micro-thread))",
            "nil",
        );
        assert_value(
            context,
            "(list (thread-live-p f5-jit-micro-thread)
                   (thread-last-error f5-jit-micro-thread))",
            "(nil nil)",
        );
    });
}

fn worker_micro(context: &mut Context) -> EvalResult {
    let _profit = ProfitFixtureScope::enter();
    context.apply(Value::symbol("f5-jit-micro-loop"), Vec::new())
}

/// Exact Lisp workload and driver of the 3e baseline, with JIT enabled.
/// The separate compiled-loop micro above also proves a native frame survives.
#[test]
#[cfg(feature = "perf-probes")]
fn green_yield_jit_enabled_switch_micro() {
    let n: usize = std::env::var("TGY_MICRO_N")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(1000);
    let _jit = JitFixtureScope::enter();
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context
        .eval_str(
            "(progn
           (setq tgy-stop nil)
           (setq tgy-th (make-thread (lambda () (while (not tgy-stop) (thread-yield)))))
           (thread-yield))",
        )
        .unwrap();
    for _ in 0..n {
        context
            .green_wait(crate::emacs_core::eval::green::Wait::Runnable)
            .expect("yield");
    }
    assert_value(
        &mut context,
        "(progn (setq tgy-stop t) (thread-join tgy-th))",
        "nil",
    );
    context.green_teardown();
}

std::thread_local! {
    /// Each Lisp owner measures only its own completed ticks. Scalar test
    /// metadata stays on that owner's OS thread and never enters the World.
    static TGT_TICKS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TGT_SOURCE_MISSES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TGT_SOURCE_RECOVERIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static TGT_PENDING_RECOVERY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static TGT_HELPER_RAN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn throughput_tick(context: &mut Context) -> EvalResult {
    require_native()?;
    finish_throughput_source_recovery(context);
    if context.threading_mode() == ThreadingMode::Green {
        context.apply(Value::symbol("thread-yield"), Vec::new())?;
        require_native()?;
    }
    // This is immediately before the loop's constant-bytecode helper call.
    // A resumed owner's shared source slot must be empty under the new
    // epoch, and that very call must restore its own cached executable.
    let helper = context.obarray.symbol_function("tgt-helper").unwrap();
    let missing = helper
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .armed_leaf_slot(cache::leaf_slot_epoch())
        .is_none();
    assert!(
        !TGT_PENDING_RECOVERY.with(|pending| pending.replace(missing)),
        "the preceding helper call completed its source recovery"
    );
    if missing {
        TGT_SOURCE_MISSES.with(|misses| misses.set(misses.get() + 1));
    }
    TGT_TICKS.with(|ticks| ticks.set(ticks.get() + 1));
    Ok(Value::NIL)
}

fn throughput_helper_witness(_context: &mut Context) -> EvalResult {
    require_native()?;
    if TGT_PENDING_RECOVERY.with(std::cell::Cell::get) {
        assert!(
            !TGT_HELPER_RAN.with(|ran| ran.replace(true)),
            "exactly one helper call follows each tick"
        );
    }
    Ok(Value::fixnum(7))
}

/// The strict seam arms the source only AFTER its compiled invocation returns.
/// Check before the following yield (or immediately after the loop's final
/// return), while no intervening handoff can invalidate the recovered slot.
fn finish_throughput_source_recovery(context: &Context) {
    if TGT_PENDING_RECOVERY.with(|pending| pending.replace(false)) {
        assert!(TGT_HELPER_RAN.with(|ran| ran.replace(false)));
        let helper = context.obarray.symbol_function("tgt-helper").unwrap();
        let runtime = helper.get_bytecode_data().unwrap().jit_runtime();
        let own_leaf = cache::compiled_leaf_ptr_for_test(runtime.compiled_id().unwrap())
            .expect("the helper was compiled on this owner before measurement");
        assert_eq!(
            runtime.armed_leaf_slot(cache::leaf_slot_epoch()),
            Some(own_leaf),
            "postyield helper call recovered this owner's cached leaf"
        );
        TGT_SOURCE_RECOVERIES.with(|recoveries| recoveries.set(recoveries.get() + 1));
    }
}

/// This owner builds and warms both leaves in its own TLS. The tiny helper
/// contains a call so the loop cannot inline it; every measured tick calls
/// its independently compiled leaf after returning from thread-yield.
fn throughput_worker(context: &mut Context) -> EvalResult {
    let _jit = JitFixtureScope::enter();
    let _profit = ProfitFixtureScope::enter();
    let _background =
        crate::emacs_core::jit::bg::TestModeScope::enter(crate::emacs_core::jit::bg::BgMode::Sync);
    let function = context.obarray.symbol_function("tgt-loop").unwrap();
    let helper = context.obarray.symbol_function("tgt-helper").unwrap();
    assert_eq!(
        function.get_bytecode_data().unwrap().constants[3],
        helper,
        "the call names the bytecode object, bypassing named-function spec slots"
    );
    TGT_PENDING_RECOVERY.with(|pending| pending.set(false));
    TGT_HELPER_RAN.with(|ran| ran.set(false));
    for _ in 0..3 {
        assert_eq!(
            context.apply(function, vec![Value::fixnum(0)])?,
            Value::fixnum(0)
        );
        assert_eq!(context.apply(helper, Vec::new())?, Value::fixnum(7));
    }
    let loop_id = function
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .compiled_id()
        .unwrap();
    let helper_id = helper
        .get_bytecode_data()
        .unwrap()
        .jit_runtime()
        .compiled_id()
        .unwrap();
    let loop_before =
        cache::compiled_leaf_ptr_for_test(loop_id).expect("loop warmed on this owner");
    let helper_before =
        cache::compiled_leaf_ptr_for_test(helper_id).expect("helper warmed on this owner");
    // SAFETY: the pointer belongs to this running owner's live cache; this
    // bounded inspection neither calls Lisp nor reaches a collection.
    let loop_leaf = unsafe { &*loop_before };
    assert!(
        !loop_leaf
            .spec_slot_kinds
            .contains(&crate::emacs_core::jit::compile::spec_slot::SpecSlotKind::Bytecode),
        "helper calls cannot use the named bytecode slot that ignores the leaf epoch"
    );
    let before = crate::emacs_core::jit::stats::compile_stats_snapshot();
    TGT_TICKS.with(|ticks| ticks.set(0));
    TGT_SOURCE_MISSES.with(|misses| misses.set(0));
    TGT_SOURCE_RECOVERIES.with(|recoveries| recoveries.set(0));
    let n = context.visible_variable_value_or_nil("tgt-iterations");
    let stride = context
        .visible_variable_value_or_nil("tgt-stride")
        .as_fixnum()
        .unwrap();
    let answer = context.apply(function, vec![n])?;
    finish_throughput_source_recovery(context);
    let after = crate::emacs_core::jit::stats::compile_stats_snapshot();
    let same_leaf = cache::compiled_leaf_ptr_for_test(loop_id) == Some(loop_before)
        && cache::compiled_leaf_ptr_for_test(helper_id) == Some(helper_before);
    let ticks = TGT_TICKS.with(std::cell::Cell::get);
    let source_misses = TGT_SOURCE_MISSES.with(std::cell::Cell::get);
    let source_recoveries = TGT_SOURCE_RECOVERIES.with(std::cell::Cell::get);
    let green = context.threading_mode() == ThreadingMode::Green;
    let iterations = n.as_fixnum().unwrap();
    eprintln!(
        concat!(
            "TGT_WITNESS {{\"owner\":{},\"mode\":\"{}\",",
            "\"compile_before\":{},\"compile_after\":{},",
            "\"retiers_before\":{},\"retiers_after\":{},",
            "\"deopts_before\":{},\"deopts_after\":{},",
            "\"same_leaf\":{},\"successful_iterations\":{},",
            "\"yield_count\":{},\"tick_count\":{},\"stride\":{},",
            "\"source_misses\":{},\"source_recoveries\":{}}}"
        ),
        context.context_instance_id,
        if green { "green" } else { "sim" },
        before.total_compiles,
        after.total_compiles,
        before.retiers,
        after.retiers,
        before.deopts(),
        after.deopts(),
        same_leaf,
        answer.as_fixnum().unwrap(),
        if green { ticks } else { 0 },
        ticks,
        stride,
        source_misses,
        source_recoveries,
    );
    assert_eq!(source_recoveries, source_misses);
    assert!(!TGT_PENDING_RECOVERY.with(std::cell::Cell::get));
    if green && ticks > 0 {
        assert!(
            source_misses > 0,
            "real handoffs invalidated the shared helper source slot"
        );
    }
    if !green {
        assert_eq!(source_misses, 0, "Sim retains the shared source slot");
    }
    assert_eq!(answer, n, "all loop iterations completed in compiled code");
    assert_eq!(ticks, (iterations / stride) as u64);
    assert!(
        same_leaf,
        "Baton switches retain each owner's loop/helper leaves"
    );
    assert_eq!(
        after.total_compiles, before.total_compiles,
        "steady execution never recompiles"
    );
    assert_eq!(
        after.retiers, before.retiers,
        "steady execution never retiers"
    );
    assert_eq!(
        after.deopts(),
        before.deopts(),
        "steady execution never tiers down"
    );
    Ok(answer)
}

fn throughput_loop(stride: usize, helper: Value) -> ByteCodeFunction {
    bytecode(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            // [limit, index, next_tick]. Both paths return to this header.
            Op::StackRef(1),
            Op::StackRef(3),
            Op::Lss,
            Op::GotoIfNil(22),
            Op::StackRef(1),
            Op::Add1,
            Op::StackSet(2),
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Eqlsign,
            Op::GotoIfNil(2),
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::Constant(3),
            Op::Call(0),
            Op::Pop,
            Op::Constant(1),
            Op::Add,
            Op::Goto(2),
            Op::Pop,
            Op::Return,
        ],
        vec![
            Value::fixnum(0),
            Value::fixnum(stride as i64),
            Value::symbol("tgt-tick"),
            helper,
        ],
        FixtureArity::Unary,
    )
}

fn run_throughput(mode: ThreadingMode, n: usize, stride: usize) {
    crate::test_utils::init_test_tracing();
    assert!(stride > 0);
    assert!(n <= Value::MOST_POSITIVE_FIXNUM as usize);
    assert!(stride <= Value::MOST_POSITIVE_FIXNUM as usize);
    let _jit = JitFixtureScope::enter();
    let _profit = ProfitFixtureScope::enter();
    let mut context = Context::new_with_threading(mode);
    context.set_lexical_binding(true);
    context.register_subr(SubrSpec::fixed0("tgt-worker", throughput_worker));
    context.register_subr(SubrSpec::fixed0("tgt-tick", throughput_tick));
    context.register_subr(SubrSpec::fixed0(
        "tgt-helper-witness",
        throughput_helper_witness,
    ));
    let helper = install(
        &mut context,
        "tgt-helper",
        bytecode(
            vec![Op::Constant(0), Op::Call(0), Op::Return],
            vec![Value::symbol("tgt-helper-witness")],
            FixtureArity::Nullary,
        ),
    );
    install(&mut context, "tgt-loop", throughput_loop(stride, helper));
    context.set_variable("tgt-iterations", Value::fixnum(n as i64));
    context.set_variable("tgt-stride", Value::fixnum(stride as i64));
    let result = catch_unwind(AssertUnwindSafe(|| {
        if mode == ThreadingMode::Green {
            assert_value(
                &mut context,
                "(let ((first (make-thread #'tgt-worker))
                       (second (make-thread #'tgt-worker)))
                   (list (thread-join first) (thread-join second)))",
                &format!("({n} {n})"),
            );
        } else {
            assert_value(
                &mut context,
                "(list (tgt-worker) (tgt-worker))",
                &format!("({n} {n})"),
            );
        }
    }));
    context.green_teardown();
    assert!(context.specpdl.is_empty());
    assert_eq!(context.jit_root_stack_top, 0);
    assert_eq!(cache::native_depth(), 0);
    if let Err(payload) = result {
        resume_unwind(payload);
    }
}

fn throughput_parameters() -> (usize, usize) {
    let parameter = |name, fallback| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(fallback)
    };
    (
        parameter("TGT_MICRO_N", 100_000),
        parameter("TGT_MICRO_YIELD_EVERY", 1000),
    )
}

#[test]
#[cfg(feature = "perf-probes")]
fn green_jit_two_thread_throughput_micro() {
    let (n, stride) = throughput_parameters();
    run_throughput(ThreadingMode::Green, n, stride);
}

#[test]
#[cfg(feature = "perf-probes")]
fn sim_jit_two_loop_throughput_micro() {
    let (n, stride) = throughput_parameters();
    run_throughput(ThreadingMode::Sim, n, stride);
}

#[test]
fn green_jit_two_compiled_loops_keep_leaves_without_recompile_or_deopt() {
    run_throughput(ThreadingMode::Green, 2000, 1000);
}

fn native_scratch_reuse_park(context: &mut Context) -> EvalResult {
    // Warm source pins and all four buffers before capturing their identities.
    for _ in 0..4 {
        native_witness(context)?;
        context.green_wait(crate::emacs_core::eval::green::Wait::Runnable)?;
    }
    let expected = context
        .green_seat()
        .expect("compiled Green owner")
        .parked_jit_scratch
        .storage_for_test();
    if expected.iter().any(|(_, capacity)| *capacity == 0) {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string("compiled scratch buffers were not exercised")],
        ));
    }
    for _ in 0..32 {
        native_witness(context)?;
        let collections = context.tagged_heap.gc_collections();
        context.green_wait(crate::emacs_core::eval::green::Wait::Runnable)?;
        require_native()?;
        let scratch = &context
            .green_seat()
            .expect("returning compiled owner")
            .parked_jit_scratch;
        if scratch.storage_for_test() != expected || !scratch.empty_for_test() {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string(
                    "compiled park changed warmed scratch storage or retained raw words",
                )],
            ));
        }
        if context.tagged_heap.gc_collections() <= collections {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string(
                    "the peer did not collect behind the compiled frame",
                )],
            ));
        }
    }
    Ok(Value::fixnum(32))
}

#[test]
fn green_jit_switch_reuses_scratch_behind_a_native_frame_while_peer_collects() {
    with_compiled_green(|context| {
        context.register_subr(SubrSpec::fixed0(
            "f5-scratch-reuse-park",
            native_scratch_reuse_park,
        ));
        let function = install(
            context,
            "f5-scratch-reuse-leaf",
            bytecode(
                vec![Op::Constant(0), Op::Call(0), Op::Return],
                vec![Value::symbol("f5-scratch-reuse-park")],
                FixtureArity::Nullary,
            ),
        );
        context
            .eval_str(
                "(progn
            (setq f5-scratch-stop nil)
            (setq f5-scratch-peer (make-thread (lambda ()
                (while (null f5-scratch-stop) (garbage-collect) (thread-yield)))))
            (thread-yield))",
            )
            .unwrap();
        let result = context.apply(Value::symbol("f5-scratch-reuse-leaf"), Vec::new());
        // Join before asserting results: a failed native witness must not
        // silently turn into a nil-valued worker result or strand a peer.
        let health = print_value(
            &context
                .eval_str(
                    "(list (thread-live-p f5-scratch-peer) (thread-last-error f5-scratch-peer))",
                )
                .unwrap(),
        );
        let joined = context
            .eval_str(
                "(progn (setq f5-scratch-stop t)
            (thread-join f5-scratch-peer))",
            )
            .unwrap();
        assert_eq!(health, "(t nil)");
        assert!(joined.is_nil());
        assert_value(
            context,
            "(list (thread-live-p f5-scratch-peer) (thread-last-error f5-scratch-peer))",
            "(nil nil)",
        );
        assert_eq!(result.unwrap().as_int(), Some(32));
        assert_value(context, "f5-jit-witness-count", "36");
        assert_value(context, "(thread-last-error)", "nil");
        assert_compiled(context, function);
    });
}

fn native_select_fine_timeout(context: &mut Context) -> EvalResult {
    native_witness(context)?;
    // Exclude compilation and the first witness from the floor measurement.
    // Switched is this continuation's proof that a real Select park occurred,
    // even if a notification legitimately made it Runnable before main ran.
    let duration = std::time::Duration::from_millis(50);
    let started = std::time::Instant::now();
    let deadline = started + duration;
    if !matches!(
        context.green_select_switch(duration)?,
        super::switch::SelectTurn::Switched
    ) {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string(
                "compiled worker did not traverse the Select park",
            )],
        ));
    }
    // Readiness may return Select early. Retry the original fine deadline;
    // never restart its duration or mistake activity for timeout completion.
    context.wait_until(deadline)?;
    let elapsed = started.elapsed();
    native_witness(context)?;
    if elapsed < std::time::Duration::from_millis(50) {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string(
                "compiled Green select returned before its fine timeout",
            )],
        ));
    }
    Ok(Value::fixnum(
        elapsed.as_nanos().min(i64::MAX as u128) as i64
    ))
}

fn worker_native_select_timeout(context: &mut Context) -> EvalResult {
    let _profit = ProfitFixtureScope::enter();
    context.apply(Value::symbol("f5-native-select-leaf"), Vec::new())
}

#[test]
fn green_jit_switch_native_select_keeps_its_fine_timeout_while_idle() {
    with_compiled_green(|context| {
        // This test targets the normal owned poller, not its degraded fallback.
        assert!(context.processes.green_select_handle().is_some());
        context.register_subr(SubrSpec::fixed0(
            "f5-native-select-wait",
            native_select_fine_timeout,
        ));
        context.register_subr(SubrSpec::fixed0(
            "f5-native-select-worker",
            worker_native_select_timeout,
        ));
        install(
            context,
            "f5-native-select-leaf",
            bytecode(
                vec![Op::Constant(0), Op::Call(0), Op::Return],
                vec![Value::symbol("f5-native-select-wait")],
                FixtureArity::Nullary,
            ),
        );
        context
            .eval_str("(setq noninteractive t timer-list nil timer-idle-list nil)")
            .unwrap();
        context
            .eval_str("(setq f5-native-select-thread (make-thread #'f5-native-select-worker))")
            .unwrap();
        // Main remains Runnable during this handoff, so the worker must
        // traverse Select. In the quiet case main's subsequent join leaves
        // only its deadline ready, and pick_next uses the owned idle poller.
        // Notifications may legitimately make the worker Runnable sooner;
        // its own Switched result is the authoritative traversal proof.
        let first_turn = context.eval_str("(thread-yield)");
        let health_before_join = context
            .eval_str(
                "(list (thread-live-p f5-native-select-thread)
                   (thread-last-error f5-native-select-thread) f5-jit-witness-count)",
            )
            .map(|value| print_value(&value));
        let joined = context.eval_str("(thread-join f5-native-select-thread)");
        let health_after_join = context
            .eval_str(
                "(list (thread-live-p f5-native-select-thread)
                   (thread-last-error f5-native-select-thread) f5-jit-witness-count)",
            )
            .map(|value| print_value(&value));
        // Assert only after joining: a self-signalled worker must fail via its
        // last-error, not look like a successful nil-valued sleep. No upper
        // timing bound is imposed; scheduling delays may make the wait late.
        assert!(first_turn.unwrap().is_nil());
        assert_eq!(health_before_join.unwrap(), "(t nil 1)");
        assert!(
            joined
                .unwrap()
                .as_int()
                .is_some_and(|nanoseconds| nanoseconds >= 50_000_000)
        );
        assert_eq!(health_after_join.unwrap(), "(nil nil 2)");
    });
}

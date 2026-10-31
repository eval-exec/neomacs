//! Cold owner admission and cache retirement. These tests use real compiled
//! leaves; no generated code or owner-local leaf pointer crosses OS threads.

use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::bg::{self, BgMode};
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::compile::{
    self, Inline2Mode, NativeRun, OptMode, ProfitGateTestPolicy,
};
use crate::emacs_core::jit::green_owner::{self, AdmissionError};
use crate::emacs_core::value::{LambdaParams, Value};
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone, Copy, Debug)]
enum JitSelection {
    Enabled,
    Disabled,
}
static_assertions::assert_impl_all!(JitSelection: Send, Sync);

impl JitSelection {
    fn current() -> Self {
        if crate::emacs_core::jit::jit_forced_off_for_test() {
            Self::Disabled
        } else {
            Self::Enabled
        }
    }

    fn install(self) {
        match self {
            Self::Enabled => crate::emacs_core::jit::force_jit_off_for_test(false),
            Self::Disabled => crate::emacs_core::jit::force_jit_off_for_test(true),
        }
    }
}

/// Scalar configuration is test-thread local. Drop only restores scalar
/// overrides; it does not acquire a lock, collect, join, or run Lisp.
#[derive(Debug)]
struct AdmissionScope {
    inline2: Inline2Mode,
    background: BgMode,
    jit: JitSelection,
    _confined: PhantomData<Rc<()>>,
}
static_assertions::assert_not_impl_any!(AdmissionScope: Send, Sync);

impl AdmissionScope {
    fn enter(inline2: Inline2Mode, background: BgMode) -> Self {
        let scope = Self {
            inline2: compile::jit_inline2_mode(),
            background: bg::mode(),
            jit: JitSelection::current(),
            _confined: PhantomData,
        };
        compile::force_inline2_for_test(Some(inline2));
        bg::force_mode_for_test(Some(background));
        JitSelection::Enabled.install();
        scope
    }
}

impl Drop for AdmissionScope {
    fn drop(&mut self) {
        compile::force_inline2_for_test(Some(self.inline2));
        bg::force_mode_for_test(Some(self.background));
        self.jit.install();
    }
}

#[derive(Clone, Copy, Debug)]
enum Inlining {
    Enabled,
    Disabled,
}
static_assertions::assert_impl_all!(Inlining: Send, Sync);

impl Inlining {
    fn current() -> Self {
        if crate::emacs_core::jit::inline::jit_inline_on() {
            Self::Enabled
        } else {
            Self::Disabled
        }
    }

    fn install(self) {
        match self {
            Self::Enabled => crate::emacs_core::jit::inline::force_inline_for_test(Some(true)),
            Self::Disabled => crate::emacs_core::jit::inline::force_inline_for_test(Some(false)),
        }
    }
}

#[derive(Debug)]
struct InlineScope(Inlining, PhantomData<Rc<()>>);
static_assertions::assert_not_impl_any!(InlineScope: Send, Sync);

impl InlineScope {
    fn enter() -> Self {
        let scope = Self(Inlining::current(), PhantomData);
        Inlining::Enabled.install();
        scope
    }
}

impl Drop for InlineScope {
    fn drop(&mut self) {
        self.0.install();
    }
}

/// Restore the exact previous override, including the Runtime/unset state.
#[derive(Debug)]
struct ProfitScope(ProfitGateTestPolicy, PhantomData<Rc<()>>);
static_assertions::assert_not_impl_any!(ProfitScope: Send, Sync);

impl ProfitScope {
    fn enter(policy: ProfitGateTestPolicy) -> Self {
        Self(compile::replace_profit_gate_for_test(policy), PhantomData)
    }
}

impl Drop for ProfitScope {
    fn drop(&mut self) {
        let _ = compile::replace_profit_gate_for_test(self.0);
    }
}

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("green-owner-argument")],
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.ops = ops;
    function.constants = constants.into();
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    function.seal_hand_assembled_ops();
    function.jit_runtime().set_hot_for_test();
    function
}

#[test]
fn green_owner_turn_invalidates_shared_source_leaf_slots() {
    crate::test_utils::init_test_tracing();
    let _admission = AdmissionScope::enter(Inline2Mode::Off, BgMode::Sync);
    let _backend = compile::opt_mode_scope_for_test(OptMode::Legacy);
    let mut context = Context::new();
    let source = function(vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    let leaf = cache::resolve_compiled_leaf_ptr(&mut context, &source).expect("compiled source");
    let epoch = cache::leaf_slot_epoch();
    source.jit_runtime().arm_leaf_slot(leaf, epoch);
    assert_eq!(source.jit_runtime().armed_leaf_slot(epoch), Some(leaf));

    green_owner::begin_owner_turn();
    let returning_epoch = cache::leaf_slot_epoch();
    assert_ne!(returning_epoch, epoch);
    assert_eq!(source.jit_runtime().armed_leaf_slot(returning_epoch), None);
    assert_eq!(
        cache::compiled_leaf_ptr_for_test(source.jit_runtime().compiled_id_or_assign()),
        Some(leaf),
        "handoff invalidates shared admission without freeing owner code"
    );

    source.jit_runtime().arm_leaf_slot(leaf, returning_epoch);
    assert_eq!(
        source.jit_runtime().armed_leaf_slot(returning_epoch),
        Some(leaf)
    );
    green_owner::begin_owner_turn();
    assert_eq!(
        source
            .jit_runtime()
            .armed_leaf_slot(cache::leaf_slot_epoch()),
        None
    );
}

#[test]
fn green_inline_retirement_keeps_native_leaf_allocation_alive() {
    crate::test_utils::init_test_tracing();
    let _admission = AdmissionScope::enter(Inline2Mode::Off, BgMode::Sync);
    let _backend = compile::opt_mode_scope_for_test(OptMode::Legacy);
    let _inlining = InlineScope::enter();
    let _profit = ProfitScope::enter(ProfitGateTestPolicy::Bypass);
    let mut context = Context::new();
    let callee = intern("green-owner-retired-callee");
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function_id(
            callee,
            Value::make_bytecode(function(vec![Op::Dup, Op::Mul, Op::Return], vec![])),
        )
    });
    let caller = function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::from_sym_id(callee)],
    );
    let caller_value = Value::make_bytecode(caller.clone());
    let frame = FrameScope::enter(&mut context, caller_value);
    assert_eq!(
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        unsafe {
            cache::try_run_compiled(frame.context, &caller, caller_value, &[Value::make_int(4)])
        }
        .expect("compiled caller returns normally"),
        Some(Value::make_int(16).bits())
    );
    let id = caller.jit_runtime().compiled_id_or_assign();
    let pointer = cache::compiled_leaf_ptr_for_test(id).expect("strict seam cached inline caller");
    assert!(cache::inline_dependent_count_for_test(callee) > 0);
    // SAFETY: this owner's cache retains the allocation through retirement.
    // No whole-cache clear or heap replacement occurs while this reference is
    // live; another owner could only retire its own separate allocation.
    let retained = unsafe { &*pointer };
    assert!(!retained.retired.get());
    let entry = retained.entry;

    cache::retire_green_inline_dependents();
    assert_eq!(cache::compiled_leaf_ptr_for_test(id), None);
    assert_eq!(cache::inline_dependent_count_for_test(callee), 0);
    assert!(retained.retired.get());
    assert_eq!(retained.entry, entry);
    assert_eq!(
        retained.call(
            frame.context as *mut Context as *mut u8,
            &[Value::make_int(4)]
        ),
        NativeRun::Ok(Value::make_int(16).bits()),
        "an original continuation can finish in retired code"
    );
}

#[test]
fn green_admission_accepts_all_inline_and_backend_configurations() {
    crate::test_utils::init_test_tracing();
    let _admission = AdmissionScope::enter(Inline2Mode::Off, BgMode::Legacy);
    if !crate::emacs_core::jit::jit_runtime_enabled() {
        return; // A deliberate VM-only gate has no compiled admission shape.
    }
    // Build every startup Context before any Green runtime freezes the native
    // subr tables. Admission itself may not register another subr afterward.
    let mut cases = Vec::new();
    for mode in [OptMode::Legacy, OptMode::Off, OptMode::Opt] {
        for background in [BgMode::Legacy, BgMode::Sync, BgMode::Threaded] {
            cases.push((
                mode,
                Inline2Mode::Off,
                background,
                Context::new_with_threading(crate::emacs_core::eval::ThreadingMode::Green),
            ));
        }
        for inline in [
            Inline2Mode::Named,
            Inline2Mode::Closure,
            Inline2Mode::Hof,
            Inline2Mode::All,
        ] {
            cases.push((
                mode,
                inline,
                BgMode::Sync,
                Context::new_with_threading(crate::emacs_core::eval::ThreadingMode::Green),
            ));
        }
    }
    for (mode, inline, background, mut context) in cases {
        let _backend = compile::opt_mode_scope_for_test(mode);
        let _configuration = AdmissionScope::enter(inline, background);
        context.setup_thread_locals();
        context
            .start_green_threads()
            .expect("JIT configuration admission");
        context.green_teardown();
    }
}

#[derive(Debug)]
struct ArithmeticScope(Inlining, PhantomData<Rc<()>>);
static_assertions::assert_not_impl_any!(ArithmeticScope: Send, Sync);

impl ArithmeticScope {
    fn enter() -> Self {
        let previous = if compile::jit_inline_arith_on() {
            Inlining::Enabled
        } else {
            Inlining::Disabled
        };
        compile::force_inline_arith_for_test(true);
        Self(previous, PhantomData)
    }
}

impl Drop for ArithmeticScope {
    fn drop(&mut self) {
        match self.0 {
            Inlining::Enabled => compile::force_inline_arith_for_test(true),
            Inlining::Disabled => compile::force_inline_arith_for_test(false),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SpecDepth(usize);
static_assertions::assert_impl_all!(SpecDepth: Send, Sync);

/// This fixture pushes exactly a Backtrace1, which owns no side-stack payload.
/// Panic restoration truncates only that fixture; it does not invoke Lisp.
struct FrameScope<'a> {
    context: &'a mut Context,
    base: SpecDepth,
    _confined: PhantomData<Rc<()>>,
}
static_assertions::assert_not_impl_any!(FrameScope<'_>: Send, Sync);

impl std::fmt::Debug for FrameScope<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FrameScope")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl<'a> FrameScope<'a> {
    fn enter(context: &'a mut Context, function: Value) -> Self {
        let base = SpecDepth(context.specpdl.len());
        context.push_backtrace_frame(function, &[Value::make_int(8)]);
        Self {
            context,
            base,
            _confined: PhantomData,
        }
    }
}

impl Drop for FrameScope<'_> {
    fn drop(&mut self) {
        self.context.specpdl.truncate(self.base.0);
    }
}

fn arithmetic_loop() -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(14),
            Op::Constant(1),
            Op::StackRef(1),
            Op::Constant(2),
            Op::Call(2),
            Op::DiscardN(1),
            Op::StackRef(0),
            Op::Add1,
            Op::StackSet(1),
            Op::Goto(1),
            Op::Return,
        ],
        vec![
            Value::make_int(0),
            Value::symbol("logand"),
            Value::make_int(1),
        ],
    )
}

#[test]
fn green_park_accepts_entry_and_unregistered_osr_arithmetic() {
    crate::test_utils::init_test_tracing();
    let _admission = AdmissionScope::enter(Inline2Mode::Off, BgMode::Sync);
    let _backend = compile::opt_mode_scope_for_test(OptMode::Legacy);
    let _arithmetic = ArithmeticScope::enter();
    {
        let mut context = Context::new();
        let source = Value::make_bytecode(arithmetic_loop());
        let symbol = intern("green-owner-unguarded-entry");
        context.with_obarray_mut(|mut obarray| obarray.set_symbol_function_id(symbol, source));
        let frame = FrameScope::enter(&mut context, Value::from_sym_id(symbol));
        assert!(
            green_owner::ParkedJit::prepare(frame.context).is_ok(),
            "an interpreted source alone is admissible"
        );
        let function = source.get_bytecode_data().expect("bytecode fixture");
        cache::resolve_compiled_leaf_ptr(frame.context, function).expect("native entry");
        assert!(green_owner::ParkedJit::prepare(frame.context).is_ok());
    }
    {
        let mut context = Context::new();
        let source = Value::make_bytecode(arithmetic_loop());
        let symbol = intern("green-owner-unguarded-osr");
        context.with_obarray_mut(|mut obarray| obarray.set_symbol_function_id(symbol, source));
        // The direct object frame also covers a funcall, unlike the named arm.
        let frame = FrameScope::enter(&mut context, source);
        let function = source.get_bytecode_data().expect("bytecode fixture");
        assert_eq!(
            // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
            unsafe {
                cache::try_run_osr(
                    frame.context,
                    function,
                    1,
                    &[Value::make_int(8), Value::make_int(0)],
                    &[],
                )
            },
            Some(NativeRun::Ok(Value::make_int(8).bits()))
        );
        let id = function.jit_runtime().compiled_id_or_assign();
        assert_eq!(
            cache::compiled_leaf_ptr_for_test(id),
            None,
            "this is OSR-only native state"
        );
        let pointer = cache::osr_leaf_ptr_for_test(function, 1).expect("OSR variant");
        // SAFETY: this owner retains the cached OSR leaf, and neither eviction
        // nor whole-cache clear happens before inspecting this metadata.
        assert!(
            unsafe { &*pointer }.inline_deps().is_empty(),
            "legacy OSR has no registered arithmetic deps"
        );
        assert!(green_owner::ParkedJit::prepare(frame.context).is_ok());
        cache::retire_green_inline_dependents();
        assert_eq!(
            cache::osr_leaf_ptr_for_test(function, 1),
            None,
            "return retirement also removes unregistered OSR variants"
        );
    }
}

#[test]
fn green_checkpoint_rejects_foreign_return_before_epoch_mutation() {
    crate::test_utils::init_test_tracing();
    let _admission = AdmissionScope::enter(Inline2Mode::Off, BgMode::Sync);
    let _backend = compile::opt_mode_scope_for_test(OptMode::Legacy);
    let mut original = Context::new();
    let checkpoint = green_owner::ParkedJit::prepare(&original).expect("original owner checkpoint");
    assert!(checkpoint.roots().is_empty());
    {
        let foreign = Context::new();
        // Context construction may itself invalidate the cache. Measure only
        // the rejected restoration, which must not consume global authority.
        let epoch = cache::leaf_slot_epoch();
        assert!(matches!(
            checkpoint.resume(&foreign),
            Err(AdmissionError::ReturningOwner)
        ));
        assert_eq!(cache::leaf_slot_epoch(), epoch);
    }
    original.setup_thread_locals();
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static GREEN_BG_WORKER_HOLD: std::sync::Mutex<Option<bg::WorkerHold>> = std::sync::Mutex::new(None);

/// Runs on B with the Baton while A's originating cache and native stack are
/// parked. Only backend output can be published here; B has no A cache entry.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn publish_background_on_peer(context: &mut Context) -> crate::emacs_core::error::EvalResult {
    drop(
        GREEN_BG_WORKER_HOLD
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take(),
    );
    assert!(bg::quiesce_for_test(std::time::Duration::from_secs(60)));
    let source = context
        .obarray
        .symbol_function("green-bg-parked-source")
        .expect("shared source");
    let runtime = source.get_bytecode_data().unwrap().jit_runtime();
    assert_eq!(
        cache::cache_entry_kind_for_test(runtime.compiled_id().unwrap()),
        "none",
        "a backend publishes code; it cannot install into the Baton peer's TLS"
    );
    assert!(runtime.armed_leaf_slot(cache::leaf_slot_epoch()).is_none());
    context.apply(Value::symbol("garbage-collect"), Vec::new())?;
    Ok(Value::T)
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn green_background_fixture(redefine: bool) {
    use crate::emacs_core::eval::ThreadingMode;
    use crate::emacs_core::subr::SubrSpec;
    let _admission = AdmissionScope::enter(Inline2Mode::Off, BgMode::Threaded);
    let _backend = compile::opt_mode_scope_for_test(OptMode::Legacy);
    let _profit = ProfitScope::enter(ProfitGateTestPolicy::Bypass);
    let _inlining = InlineScope::enter();
    bg::force_inline_insts_for_test(Some(0));
    struct RestoreWorkers;
    impl Drop for RestoreWorkers {
        fn drop(&mut self) {
            GREEN_BG_WORKER_HOLD
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .take();
            bg::force_inline_insts_for_test(None);
        }
    }
    let _restore = RestoreWorkers;
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.register_subr(SubrSpec::fixed0(
        "green-bg-publish-on-peer",
        publish_background_on_peer,
    ));
    context.set_lexical_binding(true);
    context
        .eval_str("(thread-join (make-thread (lambda () nil)))")
        .unwrap();
    let callee = intern("green-bg-inline-callee");
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function_id(
            callee,
            Value::make_bytecode(function(vec![Op::Dup, Op::Mul, Op::Return], vec![])),
        );
    });
    let source = Value::make_bytecode(function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::from_sym_id(callee)],
    ));
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function("green-bg-parked-source", source);
    });
    let source_root = context.retain_gc_roots([source]);
    let source_data = source.get_bytecode_data().unwrap();
    *GREEN_BG_WORKER_HOLD
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(bg::hold_workers_for_test());
    assert_eq!(
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        unsafe { cache::try_run_compiled(&mut context, source_data, source, &[Value::fixnum(4)]) }
            .unwrap(),
        None
    );
    let id = source_data.jit_runtime().compiled_id().unwrap();
    assert_eq!(cache::cache_entry_kind_for_test(id), "pending");
    assert!(cache::inline_dependent_count_for_test(callee) > 0);
    let worker = if redefine {
        "(setq green-bg-worker (make-thread (lambda ()
          (green-bg-publish-on-peer)
          (fset 'green-bg-inline-callee (lambda (x) 37))
          (garbage-collect) 'background-done)))"
    } else {
        "(setq green-bg-worker (make-thread (lambda () (green-bg-publish-on-peer) 'background-done)))"
    };
    context.eval_str(worker).unwrap();
    let outcome = context
        .eval_str("(list (thread-join green-bg-worker) (thread-last-error green-bg-worker))")
        .unwrap();
    assert_eq!(
        crate::emacs_core::print::print_value(&outcome),
        "(background-done nil)",
        "the peer must complete publication, redefinition and collection"
    );
    // A has its Baton again. An epoch move retires its pending inline leaf,
    // and a fresh compile must observe B's new canonical function definition.
    if redefine {
        assert_eq!(cache::inline_dependent_count_for_test(callee), 0);
        cache::prepare_entry_for_test(&mut context, source_data);
    } else {
        cache::drain_ready_pending(Some(&context));
    }
    assert_eq!(cache::cache_entry_kind_for_test(id), "compiled");
    let result =
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        unsafe { cache::try_run_compiled(&mut context, source_data, source, &[Value::fixnum(4)]) }.unwrap();
    assert_eq!(
        result,
        Some(Value::fixnum(if redefine { 37 } else { 16 }).bits())
    );
    context.green_teardown();
    drop(source_root);
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn green_background_publication_while_peer_holds_baton_installs_on_returning_owner() {
    green_background_fixture(false);
}

#[test]
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
fn green_background_inline_compile_redefined_while_parked_is_retired_before_install() {
    green_background_fixture(true);
}

#[test]
fn green_osr_retirement_preserves_relocation_roots_after_publication_retires() {
    let _admission = AdmissionScope::enter(Inline2Mode::Off, BgMode::Sync);
    let _backend = compile::opt_mode_scope_for_test(OptMode::Legacy);
    let mut context = Context::new();
    let literal = Value::cons(Value::fixnum(991), Value::string("retired-osr-only"));
    let source = function(
        vec![
            Op::Constant(0),
            Op::StackRef(0),
            Op::StackRef(2),
            Op::Lss,
            Op::GotoIfNil(7),
            Op::Add1,
            Op::Goto(1),
            Op::Pop,
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::fixnum(0), literal],
    );
    assert_eq!(
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        unsafe {
            cache::try_run_osr(
                &mut context,
                &source,
                1,
                &[Value::fixnum(2), Value::fixnum(0)],
                &[],
            )
        },
        Some(NativeRun::Ok(literal.bits()))
    );
    assert!(cache::osr_leaf_ptr_for_test(&source, 1).is_some());
    cache::retire_green_inline_dependents();
    assert!(cache::osr_leaf_ptr_for_test(&source, 1).is_none());
    let mut relocations = Vec::new();
    cache::collect_jit_reloc_gc_roots(&mut relocations);
    assert!(
        relocations.contains(&literal),
        "evicted OSR relocations remain roots after the parked RootBatch retires"
    );
}

fn evicted_osr_native_witness(_context: &mut Context) -> crate::emacs_core::error::EvalResult {
    if cache::native_depth() == 0 {
        return Err(crate::emacs_core::error::signal(
            "error",
            vec![Value::string(
                "evicted OSR continuation lost its native frame",
            )],
        ));
    }
    Ok(Value::T)
}

fn evict_running_osr_then_collect_and_park(
    context: &mut Context,
) -> crate::emacs_core::error::EvalResult {
    evicted_osr_native_witness(context)?;
    let source = context
        .obarray
        .symbol_function("green-evicted-osr-source")
        .unwrap();
    let function = source.get_bytecode_data().unwrap();
    let literal = function.constants[3];
    let id = function.jit_runtime().compiled_id().unwrap();
    assert_eq!(
        cache::cache_entry_kind_for_test(id),
        "none",
        "OSR-only source"
    );
    cache::evict_compiled(id);
    assert!(cache::osr_leaf_ptr_for_test(function, 1).is_none());
    assert!(
        cache::green_has_native_state(),
        "an evicted active OSR leaf must still prevent the empty-cache source shortcut"
    );
    let mut relocations = Vec::new();
    cache::collect_jit_reloc_gc_roots(&mut relocations);
    assert!(
        relocations.contains(&literal),
        "eviction retains exact relocation roots"
    );
    context.apply(Value::symbol("garbage-collect"), Vec::new())?;
    context.apply(Value::symbol("thread-yield"), Vec::new())?;
    evicted_osr_native_witness(context)?;
    // The peer replaced the function cell while this original frame was parked.
    // Its restored exact-source pin must now carry the old object into this GC.
    context.apply(Value::symbol("garbage-collect"), Vec::new())?;
    assert!(context.tagged_heap.owns_heap_value_for_test(source));
    assert!(context.tagged_heap.owns_heap_value_for_test(literal));
    Ok(Value::T)
}

#[test]
fn green_evicted_active_osr_retains_code_relocations_and_exact_source_through_park() {
    use crate::emacs_core::eval::ThreadingMode;
    use crate::emacs_core::subr::SubrSpec;
    let _admission = AdmissionScope::enter(Inline2Mode::Off, BgMode::Sync);
    let _backend = compile::opt_mode_scope_for_test(OptMode::Off);
    let _profit = ProfitScope::enter(ProfitGateTestPolicy::Bypass);
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.set_lexical_binding(true);
    context.register_subr(SubrSpec::fixed0(
        "green-evicted-osr-park",
        evict_running_osr_then_collect_and_park,
    ));
    context.register_subr(SubrSpec::fixed0(
        "green-evicted-osr-witness",
        evicted_osr_native_witness,
    ));
    context
        .eval_str("(thread-join (make-thread (lambda () nil)))")
        .unwrap();
    let literal = Value::cons(Value::fixnum(1991), Value::string("evicted-osr-relocation"));
    let source = Value::make_bytecode(function(
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
            Op::Pop,
            Op::Constant(3),
            Op::Return,
        ],
        vec![
            Value::fixnum(0),
            Value::symbol("green-evicted-osr-park"),
            Value::symbol("green-evicted-osr-witness"),
            literal,
        ],
    ));
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function("green-evicted-osr-source", source);
    });
    context
        .eval_str(
            "(setq green-evicted-osr-peer (make-thread (lambda ()
          (fset 'green-evicted-osr-source (lambda (x) 'replacement))
          (garbage-collect) 'peer-finished)))",
        )
        .unwrap();
    let source_data = source.get_bytecode_data().unwrap();
    cache::prepare_osr_for_test(&mut context, source_data, 1);
    let leaf = cache::osr_leaf_ptr_for_test(source_data, 1).unwrap();
    let frame = FrameScope::enter(&mut context, Value::symbol("green-evicted-osr-source"));
    // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
    let result = unsafe {
        cache::try_run_osr(
            frame.context,
            source_data,
            1,
            &[Value::fixnum(1), Value::fixnum(0)],
            &[],
        )
    };
    assert_eq!(result, Some(NativeRun::Ok(literal.bits())));
    // SAFETY: the original owner's retirement store retains this exact leaf.
    assert!(unsafe { &*leaf }.retired.get());
    assert!(frame.context.tagged_heap.owns_heap_value_for_test(source));
    assert!(frame.context.tagged_heap.owns_heap_value_for_test(literal));
    let peer = frame.context.eval_str(
        "(list (thread-join green-evicted-osr-peer) (thread-last-error green-evicted-osr-peer))",
    ).unwrap();
    assert_eq!(
        crate::emacs_core::print::print_value(&peer),
        "(peer-finished nil)"
    );
    drop(frame);
    context.green_teardown();
    assert_eq!(cache::native_depth(), 0);
    assert_eq!(context.jit_root_stack_top, 0);
}

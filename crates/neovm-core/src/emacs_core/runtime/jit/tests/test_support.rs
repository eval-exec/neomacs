//! Native-observation fixtures: finish code generation on the worker and
//! install on the owning mutator before observing one execution. Preparation
//! never executes a Lisp body or changes the selected compilation mode.

use crate::emacs_core::{bytecode::ByteCodeFunction, eval::Context};
use std::time::Duration;

pub(crate) fn settle(ctx: *mut Context) {
    assert!(
        super::bg::quiesce_for_test(Duration::from_secs(60)),
        "backend readiness timed out"
    );
    // SAFETY: test caller owns the dormant Context, or passes null for shim-free fixtures.
    super::cache::drain_ready_pending(unsafe { ctx.as_ref() });
}

pub(crate) fn prepare_entry(ctx: *mut Context, f: &ByteCodeFunction) {
    super::cache::prepare_entry_for_test(ctx, f);
}

pub(crate) fn prepare_osr(ctx: *mut Context, f: &ByteCodeFunction, pc: usize) {
    super::cache::prepare_osr_for_test(ctx, f, pc);
}

pub(crate) fn prepare_upgrade(ctx: *mut Context, f: &ByteCodeFunction) {
    super::cache::prepare_upgrade_for_test(ctx, f);
}

/// Observe one native entry. Only an unexecuted pending result is awaited;
/// deopts, rejection, invalid arity and interpreter-only verdicts are returned
/// unchanged, so their tests do not replay a body's side effects.
pub(crate) fn run_compiled(
    ctx: *mut Context,
    f: &ByteCodeFunction,
    value: crate::emacs_core::value::Value,
    args: &[crate::emacs_core::value::Value],
) -> Result<Option<usize>, crate::emacs_core::error::Flow> {
    let before = native_dispatch_count();
    let result = super::cache::try_run_compiled(ctx, f, value, args);
    if before == native_dispatch_count()
        && matches!(result, Ok(None))
        && f.jit_runtime()
            .compiled_id()
            .is_some_and(|id| super::cache::cache_entry_kind_for_test(id) == "pending")
    {
        prepare_entry(ctx, f);
        super::cache::try_run_compiled(ctx, f, value, args)
    } else {
        result
    }
}

/// A one-shot OSR observation after lookup/codegen/install, without an earlier
/// transfer that could perturb poll counts, bindings or heap mutations.
pub(crate) fn run_osr(
    ctx: *mut Context,
    f: &ByteCodeFunction,
    pc: usize,
    stack: &[crate::emacs_core::value::Value],
    binds: &[usize],
) -> Option<super::compile::NativeRun> {
    prepare_osr(ctx, f, pc);
    super::cache::try_run_osr(ctx, f, pc, stack, binds)
}

/// Stage named fixture functions without calling them. The owner retains each
/// function in its function cell throughout the front/backend/install sequence.
pub(crate) fn prepare_named(ctx: *mut Context, names: &[&str]) {
    assert!(!ctx.is_null());
    for name in names {
        // SAFETY: fixture owns this dormant Context; compile preparation never
        // evaluates a body or permits Lisp mutation of the function cell.
        let value = unsafe { &*ctx }
            .obarray
            .symbol_function_id(crate::emacs_core::intern::intern(name))
            .expect("fixture function defined");
        let f = value
            .get_bytecode_data()
            .expect("fixture function bytecompiled");
        prepare_entry(ctx, f);
    }
}

/// Resolve a first-sight fixture leaf without executing its body, retaining the
/// production origin and the resolver's own eligibility/refusal rules.
pub(crate) fn resolve_leaf(
    ctx: *mut Context,
    f: &ByteCodeFunction,
) -> Option<*const super::compile::CompiledLeaf> {
    let first = super::cache::resolve_compiled_leaf_ptr(ctx, f);
    if first.is_some() {
        return first;
    }
    if f.jit_runtime()
        .compiled_id()
        .is_some_and(|id| super::cache::cache_entry_kind_for_test(id) == "pending")
    {
        // The resolver installs with this source's RuntimeState. An idle
        // drain has no RuntimeState and would leave the job's admission hold
        // behind, making the first observed callback interpreted.
        assert!(
            super::bg::quiesce_for_test(Duration::from_secs(60)),
            "first-sight backend readiness timed out"
        );
        super::cache::resolve_compiled_leaf_ptr(ctx, f)
    } else {
        first
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OsrObservation {
    Pending,
    AwaitNative,
}

thread_local! {
    static OSR_OBSERVATION: std::cell::Cell<OsrObservation> = const {
        std::cell::Cell::new(OsrObservation::Pending)
    };
}

/// Exact native-transition observations wait for the real worker at the cold
/// lookup boundary. Pending protocol tests never enter this scope.
#[must_use]
pub(crate) struct AwaitOsrScope {
    prior: OsrObservation,
    _owner: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl AwaitOsrScope {
    pub(crate) fn enter() -> Self {
        let prior = OSR_OBSERVATION.with(|mode| mode.replace(OsrObservation::AwaitNative));
        Self {
            prior,
            _owner: std::marker::PhantomData,
        }
    }
}

impl Drop for AwaitOsrScope {
    fn drop(&mut self) {
        OSR_OBSERVATION.with(|mode| mode.set(self.prior));
    }
}

pub(super) fn awaits_osr() -> bool {
    match OSR_OBSERVATION.with(std::cell::Cell::get) {
        OsrObservation::Pending => false,
        OsrObservation::AwaitNative => true,
    }
}

/// Witness a native dispatch attempt independently of the eventual cache state.
/// A body can invalidate/recompile itself before returning a deopt; that
/// pending cache entry must never cause this fixture to replay its effects.
#[derive(Clone, Copy, PartialEq, Eq)]
struct NativeDispatchCount(u64);
thread_local! {
    static NATIVE_DISPATCH_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}
fn native_dispatch_count() -> NativeDispatchCount {
    NativeDispatchCount(NATIVE_DISPATCH_COUNT.with(std::cell::Cell::get))
}
pub(super) fn note_native_dispatch() {
    NATIVE_DISPATCH_COUNT.with(|count| {
        count.set(
            count
                .get()
                .checked_add(1)
                .expect("test native count overflow"),
        )
    });
}

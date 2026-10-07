//! Tier-0 `Bcall` of a leaf builtin (`NEOVM_VM_LEAF`; design
//! `p1-0-integration` §3.10 "Tier-0 calls the leaves", P1.2 commit 12).
//!
//! An `Op::Call` whose symbol's function cell holds a builtin with a Bcall
//! leaf (`gethash`, `plist-get`, `get-char-property`, `buffer-local-value`)
//! -- in the interpreter, and in compiled code's generic call, which
//! resolves the same way -- runs the leaf body on the stack arguments
//! instead of the subr dispatch: no backtrace frame is pushed or popped on
//! success. What makes the skipped frame unobservable is GNU `Bcall`'s own
//! order: the quit poll, the `debug-on-next-call` test and the depth count
//! have run in the caller before the call is dispatched (they are
//! unchanged), and a leaf runs no Lisp, reaches no safe point and changes
//! none of the evaluator's stacks, so nothing can read the frame while it
//! would be live -- except the signal machinery, which gets it: when the
//! leaf signals, the frame is pushed over the same stack span and the
//! signal dispatched under it exactly as the builtin's own error would be
//! (`finish_traced_builtin_call`: signal hook, `handler-bind`, debugger,
//! then the pop). A declined shape (a user hash test, a `plist-get`
//! PREDICATE) and a call outside the leaf's arity take the unchanged
//! builtin path from scratch; a leaf declines before any side effect.
//!
//! The inline opcodes (`Bnth`, `Bmemq`, `Bget`, ... and `Bsymbol_value`)
//! already call the leaves' own bodies: each opcode arm's
//! `builtin_*_1`/`_2` is a one-line wrapper over the `*_values` function the
//! leaf calls, and like GNU's opcodes they push no frame at all.
//!
//! The knob is read when a call target is resolved into the symbol call
//! cache, not per call: under the default every builtin resolves exactly as
//! before.

use super::*;
use crate::emacs_core::subr::leaf::{LeafActive, LeafEntry, LeafExit, LeafId, LeafOutcome};
use std::sync::atomic::{AtomicU64, Ordering};

/// What `NEOVM_VM_LEAF` turns on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct VmLeafKnob {
    /// `Op::Call` of a builtin with a Bcall leaf runs the leaf.
    pub(crate) bcall: bool,
}

impl VmLeafKnob {
    pub(crate) const OFF: Self = Self { bcall: false };
    pub(crate) const ALL: Self = Self { bcall: true };

    /// Unset/`off`/`0`: nothing; `on`/`1`/`all`/`bcall`: the Bcall leaves.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            None | Some("" | "0" | "off" | "false" | "no") => Self::OFF,
            Some("1" | "on" | "all" | "true" | "yes" | "bcall") => Self::ALL,
            Some(other) => {
                tracing::warn!(
                    value = other,
                    "NEOVM_VM_LEAF: unknown value ignored (expected off, on, bcall)"
                );
                Self::OFF
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    static KNOB_TEST_OVERRIDE: std::cell::Cell<Option<VmLeafKnob>> =
        const { std::cell::Cell::new(None) };
    /// Test hook: leaf bodies the interpreter ran on this thread.
    static LEAF_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Force the knob on the current thread (tests only); `None` returns to the
/// environment's. A context's call cache keeps what it resolved before.
#[cfg(test)]
pub(crate) fn force_vm_leaf_knob_for_test(knob: Option<VmLeafKnob>) {
    KNOB_TEST_OVERRIDE.with(|c| c.set(knob));
}

/// Test hook: leaf bodies the interpreter ran on this thread so far.
#[cfg(test)]
pub(crate) fn vm_leaf_calls_for_test() -> u64 {
    LEAF_CALLS.with(std::cell::Cell::get)
}

/// The `NEOVM_VM_LEAF` setting (read once).
pub(crate) fn vm_leaf_knob() -> VmLeafKnob {
    #[cfg(test)]
    if let Some(knob) = KNOB_TEST_OVERRIDE.with(|c| c.get()) {
        return knob;
    }
    use std::sync::OnceLock;
    static KNOB: OnceLock<VmLeafKnob> = OnceLock::new();
    *KNOB.get_or_init(|| VmLeafKnob::parse(std::env::var("NEOVM_VM_LEAF").ok().as_deref()))
}

/// Counters of the census line: call targets resolved to a leaf (cache
/// fills), and the leaf calls that took the builtin's path instead -- a
/// declined shape or an arity outside the leaf's -- or signalled. Only
/// cold outcomes count: a hit is the hot path.
struct VmLeafStats {
    resolved: AtomicU64,
    generic: AtomicU64,
    signal: AtomicU64,
}

static STATS: VmLeafStats = VmLeafStats {
    resolved: AtomicU64::new(0),
    generic: AtomicU64::new(0),
    signal: AtomicU64::new(0),
};

/// The census entry (`vm_leaf:resolved=,generic=,signal=`), when anything
/// resolved to a leaf.
pub(crate) fn render_vm_leaf_stats() -> Option<String> {
    let resolved = STATS.resolved.load(Ordering::Relaxed);
    (resolved > 0).then(|| {
        format!(
            "vm_leaf:resolved={resolved},generic={},signal={}",
            STATS.generic.load(Ordering::Relaxed),
            STATS.signal.load(Ordering::Relaxed)
        )
    })
}

/// The Bcall leaf a resolved builtin callee runs under `NEOVM_VM_LEAF`: the
/// one attached to the builtin's subr (so an alias of `gethash` gets it
/// too), found once, when the target is cached.
#[inline]
pub(super) fn bcall_leaf_of(callee: ResolvedBuiltinCallee) -> Option<LeafId> {
    if !vm_leaf_knob().bcall {
        return None;
    }
    bcall_leaf_of_slow(callee)
}

#[cold]
#[inline(never)]
fn bcall_leaf_of_slow(callee: ResolvedBuiltinCallee) -> Option<LeafId> {
    let (sym, ..) = callee.dispatch_parts();
    let leaf = crate::emacs_core::subr::leaf::subr_leaf(sym)?;
    STATS.resolved.fetch_add(1, Ordering::Relaxed);
    Some(leaf.id)
}

impl Vm<'_> {
    /// `Bcall` of `callee`, whose leaf is `leaf`, over the stack arguments
    /// `bc_buf[args_start..args_start + nargs]` (see the module docs). The
    /// caller has polled quit, taken an armed `debug-on-next-call` elsewhere
    /// and counted the depth, as for [`Vm::call_resolved_builtin_from_stack_args`],
    /// which a call outside the leaf's arity or a declined shape takes.
    #[inline(never)]
    pub(super) fn call_builtin_leaf_from_stack_args(
        ctx: &mut crate::emacs_core::eval::Context,
        func_val: Value,
        args_start: usize,
        nargs: usize,
        callee: ResolvedBuiltinCallee,
        leaf: LeafId,
    ) -> EvalResult {
        let spec = leaf.spec();
        let (_, _, min_args, max_args) = callee.dispatch_parts();
        if nargs < usize::from(min_args)
            || nargs > spec.entry_slots()
            || max_args.is_some_and(|max| nargs > usize::from(max))
        {
            return Self::call_resolved_builtin_from_stack_args(
                ctx, func_val, args_start, nargs, callee,
            );
        }
        debug_assert!(
            !ctx.debug_on_next_call_is_armed(),
            "an armed debug-on-next-call takes the debugged call path"
        );
        let arg = |ctx: &crate::emacs_core::eval::Context, i: usize| {
            if i < nargs {
                ctx.bc_buf[args_start + i]
            } else {
                Value::NIL
            }
        };
        let result = {
            let shared: &crate::emacs_core::eval::Context = ctx;
            let active = LeafActive::enter(spec, shared);
            let result = match spec.entry {
                LeafEntry::L1(body) => body(shared, arg(shared, 0)),
                LeafEntry::L2(body) => body(shared, arg(shared, 0), arg(shared, 1)),
                LeafEntry::L3(body) => body(shared, arg(shared, 0), arg(shared, 1), arg(shared, 2)),
            };
            active.exit(shared, LeafOutcome::of(&result));
            result
        };
        #[cfg(test)]
        LEAF_CALLS.with(|c| c.set(c.get() + 1));
        match result {
            Ok(value) => Ok(value),
            Err(LeafExit::Generic) => Self::leaf_declined(ctx, func_val, args_start, nargs, callee),
            Err(LeafExit::Signal(flow)) => {
                Self::leaf_signalled(ctx, func_val, args_start, nargs, flow)
            }
        }
    }

    /// A declined shape: the builtin's own protocol, from scratch.
    #[cold]
    #[inline(never)]
    fn leaf_declined(
        ctx: &mut crate::emacs_core::eval::Context,
        func_val: Value,
        args_start: usize,
        nargs: usize,
        callee: ResolvedBuiltinCallee,
    ) -> EvalResult {
        STATS.generic.fetch_add(1, Ordering::Relaxed);
        Self::call_resolved_builtin_from_stack_args(ctx, func_val, args_start, nargs, callee)
    }

    /// The leaf signalled: push the frame `Bcall` would have recorded
    /// (`record_in_backtrace`, the call's symbol over its stack arguments)
    /// and dispatch the undispatched signal under it, then pop -- the
    /// builtin path's own error sequence.
    #[cold]
    #[inline(never)]
    fn leaf_signalled(
        ctx: &mut crate::emacs_core::eval::Context,
        func_val: Value,
        args_start: usize,
        nargs: usize,
        flow: crate::emacs_core::error::Flow,
    ) -> EvalResult {
        STATS.signal.fetch_add(1, Ordering::Relaxed);
        let backtrace = ctx.push_backtrace_frame_from_bc_stack(func_val, args_start, nargs);
        ctx.finish_traced_builtin_call(backtrace, Err(flow))
    }
}

#[cfg(test)]
#[path = "tests/vm_leaf_test.rs"]
mod vm_leaf_tests;

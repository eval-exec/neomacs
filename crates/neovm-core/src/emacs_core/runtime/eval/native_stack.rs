//! The native-stack guard of compiled Lisp: GNU `setup_frame`'s
//! `error ("Bytecode stack overflow")` (src/bytecode.c:514-515) for the stack
//! JIT leaves recurse on.
//!
//! GNU byte code keeps its frames on a bytecode stack of its own (512K words,
//! `BC_STACK_SIZE`, src/bytecode.c:337), so a deep byte-compiled recursion
//! under a raised `max-lisp-eval-depth` ends in that error. Compiled leaves
//! here recurse on the thread's native stack, while the Tier-0 interpreter
//! and the tree walker probe it and continue on a fresh stacker segment.
//! Nothing bounded a leaf-to-leaf recursion but `max-lisp-eval-depth`, so
//! past a few hundred thousand levels the process died of SIGSEGV.
//!
//! The guard is one field, [`Context::jit_stack_limit`]: the lowest address a
//! compiled leaf's caller-owned result slot may sit at when the leaf is
//! entered. A leaf whose body can re-enter Lisp compares its `out` pointer
//! against it in its entry block (`jit::compile::stack_guard`) -- one compare
//! with a memory operand and a branch, since `out` is a word of the calling
//! frame -- and below it calls `neovm_jit_stack_check`, which measures the
//! real stack ([`native_stack_exhausted`]) and either signals GNU's error or
//! lets the leaf run (a stale limit: the code runs on a stack segment below
//! the one the limit names).
//!
//! The limit follows the stack segment the thread runs on:
//! [`Context::setup_thread_locals`] sets it for the thread's own stack, and
//! the stacker probes that switch segments ([`maybe_grow_tracking_jit_limit`])
//! point it at the new segment for the callback's duration and restore it
//! after, on unwinds too. A segment the limit does not know sits either below
//! it (every guarded entry takes the measuring slow path, which is exact) or
//! above it (no guard there: the state before this module, never a false
//! signal).

use super::Context;

/// Native stack a compiled leaf that can re-enter Lisp must find below its
/// caller's frame, or it signals "Bytecode stack overflow". It covers the
/// Rust frames between one leaf entry and the next guard (the leaf's own
/// frame, the call shims, a builtin, the signal's construction) and the
/// Tier-0 interpreter's and tree walker's own red zone, whose probes switch
/// segments only below 128 KiB.
pub(crate) const JIT_STACK_RED_ZONE: usize = 1024 * 1024;

/// The [`Context::jit_stack_limit`] for the stack segment the calling thread
/// runs on now: the segment's low end plus [`JIT_STACK_RED_ZONE`], or 0 when
/// stacker cannot tell the segment's bounds (no guard; never a false signal).
#[inline(never)]
pub(crate) fn jit_stack_limit_here() -> usize {
    let Some(remaining) = stacker::remaining_stack() else {
        return 0;
    };
    let marker = 0u8;
    // An address in this frame: `remaining` was measured a few words away.
    let sp = std::hint::black_box(core::ptr::addr_of!(marker)) as usize;
    sp.saturating_sub(remaining)
        .saturating_add(JIT_STACK_RED_ZONE)
}

/// Whether the native stack left on the current segment is below
/// [`JIT_STACK_RED_ZONE`]: the exact test behind a compiled leaf's entry
/// guard. Unknown bounds answer `false`.
pub(crate) fn native_stack_exhausted() -> bool {
    stacker::remaining_stack().is_some_and(|remaining| remaining < JIT_STACK_RED_ZONE)
}

/// `stacker::maybe_grow(red_zone, segment, ..)` over `owner`, keeping the
/// Context's `jit_stack_limit` (reached through `limit`) on the segment the
/// callback runs on. The enough-stack branch is `stacker::maybe_grow`'s.
#[inline(always)]
pub(crate) fn maybe_grow_tracking_jit_limit<T: ?Sized, R>(
    owner: &mut T,
    limit: fn(&mut T) -> &mut usize,
    red_zone: usize,
    segment: usize,
    f: impl FnOnce(&mut T) -> R,
) -> R {
    match stacker::remaining_stack() {
        Some(remaining) if remaining >= red_zone => f(owner),
        _ => grow_tracking_jit_limit(owner, limit, segment, f),
    }
}

/// The growing branch of [`maybe_grow_tracking_jit_limit`]: run `f` on a
/// fresh `segment`-byte stack with the limit naming it, then restore the
/// caller's limit -- after an unwind as well, since a limit left on a freed
/// segment below this stack would disable the guard here.
#[cold]
#[inline(never)]
pub(crate) fn grow_tracking_jit_limit<T: ?Sized, R>(
    owner: &mut T,
    limit: fn(&mut T) -> &mut usize,
    segment: usize,
    f: impl FnOnce(&mut T) -> R,
) -> R {
    let saved = *limit(owner);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stacker::grow(segment, || {
            *limit(owner) = jit_stack_limit_here();
            f(owner)
        })
    }));
    *limit(owner) = saved;
    result.unwrap_or_else(|payload| std::panic::resume_unwind(payload))
}

impl Context {
    /// Point [`Self::jit_stack_limit`] at the stack segment the calling
    /// thread runs on now (see the module docs).
    pub(crate) fn refresh_jit_stack_limit(&mut self) {
        self.jit_stack_limit = jit_stack_limit_here();
    }

    /// The field [`maybe_grow_tracking_jit_limit`] keeps on the segment.
    pub(crate) fn jit_stack_limit_mut(&mut self) -> &mut usize {
        &mut self.jit_stack_limit
    }
}

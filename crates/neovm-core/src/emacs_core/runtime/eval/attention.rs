//! Poll sources that send a safe point off its loads-only fast path.
//!
//! GNU's safe point is `if (!NILP (Vquit_flag) || pending_signals)
//! probably_quit ();` (src/lisp.h:3897-3901): two loads, and everything that
//! can need attention arrives through one of the two words.  This module holds
//! neomacs's counterparts.
//!
//! * [`ASYNC_ATTENTION`], one process-wide word for everything that arrives
//!   from another thread or from signal context: the Lisp profiler's tick, a
//!   handled OS signal, and raised [`QuitRequest`]s.  It is GNU's
//!   `pending_signals` (src/keyboard.c:105), a hint the slow path re-derives.
//! * `Context::attention`, one per-Context word derived from the evaluator's
//!   own state that a safe point must look at ([`AttentionBit`]).  GNU's
//!   `Vquit_flag` half of the test.
//!
//! A safe point is clear exactly when both words are clear under its
//! [`AttentionMask`]: two loads, as in GNU.
//!
//! A child module of `eval`, like its siblings, so it keeps the same view of
//! `Context` and the parent's private items (`use super::*`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::Context;
use crate::emacs_core::value::Value;

/// A Context condition that sends a safe point off its loads-only fast path.
/// A SET bit means "may need attention" (a superset); the slow path always
/// re-derives the exact answer from the canonical fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum AttentionBit {
    /// `quit-flag` is non-nil: GNU's `!NILP (Vquit_flag)` (src/lisp.h:3899).
    QuitFlag = 1 << 0,
    /// `throw-on-input` is non-nil: the safe point may have to poll the host
    /// input channel for it (neomacs-only; GNU's input layer sets
    /// `Vquit_flag` itself, src/keyboard.c:3869-3871).  Conservative: set
    /// even where there is no channel to poll (a batch session), which costs
    /// that session one cold trip per poll and nothing else -- the slow path
    /// re-checks `has_throw_on_input_poll_source`.
    ThrowOnInput = 1 << 1,
    /// `internal--compiler-function-overrides` is a cons: the overrides
    /// shadow function cells from a VARIABLE, invisible to the function
    /// epoch, so a speculated subr site must bounce to the generic call
    /// (neomacs-only state; GNU has no such cache).
    CompilerOverrides = 1 << 2,
    /// The `NEOVM_JIT_FORCE_SLOW_SPEC=1` verification harness: every
    /// speculated call site re-validates its binding on every call
    /// (process-constant; never set in a production run).
    ForceSlowSpec = 1 << 3,
}

/// Which [`AttentionBit`]s a particular safe point or call gate must see
/// clear, besides [`ASYNC_ATTENTION`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AttentionMask(u32);

impl AttentionMask {
    /// `maybe_quit`'s fast test: GNU's `Vquit_flag`, plus the host-input
    /// poll `throw-on-input` needs here.
    pub(crate) const QUIT: Self =
        Self(AttentionBit::QuitFlag as u32 | AttentionBit::ThrowOnInput as u32);

    /// `neovm_jit_call_spec`'s gate: the quit poll it makes first, plus the
    /// force harness, which sends every call to the re-validating slow half.
    /// (That half polls with [`Self::QUIT`] only, so the harness bit cannot
    /// make it loop.)
    pub(crate) const SPEC_CALL: Self = Self(Self::QUIT.0 | AttentionBit::ForceSlowSpec as u32);

    /// The subr spec shims' gate: [`Self::SPEC_CALL`], plus active compiler
    /// overrides, which refuse a direct subr call.
    pub(crate) const SPEC_SUBR: Self =
        Self(Self::SPEC_CALL.0 | AttentionBit::CompilerOverrides as u32);

    /// What `subr_spec_armed` must see clear before it trusts an equal epoch:
    /// the two conditions besides a stale epoch that leave its compare.
    pub(crate) const SUBR_ARMING: Self =
        Self(AttentionBit::CompilerOverrides as u32 | AttentionBit::ForceSlowSpec as u32);

    /// The JIT's inline entry guards (an inlined callee, an inline
    /// `type-of`): the quit poll the skipped call would make, plus active
    /// compiler overrides. Not the force harness: an inlined call has no spec
    /// slot to re-validate, and the harness's compile-time switch already
    /// withholds the inline `type-of`.
    pub(crate) const INLINE_ENTRY: Self =
        Self(Self::QUIT.0 | AttentionBit::CompilerOverrides as u32);

    /// The mask as the word compiled code ANDs with the attention word.
    #[inline(always)]
    pub(crate) const fn bits(self) -> u32 {
        self.0
    }
}

/// The attention word the canonical fields imply.  `Context::attention` must
/// equal this at every read ([`Context::attention_clear`] asserts it in debug
/// builds); [`Context::refresh_attention`] is the only way to move it.
pub(super) fn attention_of(quit_flag: Value, throw_on_input: Value, overrides: bool) -> u32 {
    let mut word = 0;
    if !quit_flag.is_nil() {
        word |= AttentionBit::QuitFlag as u32;
    }
    if !throw_on_input.is_nil() {
        word |= AttentionBit::ThrowOnInput as u32;
    }
    if overrides {
        word |= AttentionBit::CompilerOverrides as u32;
    }
    #[cfg(feature = "jit")]
    if crate::emacs_core::jit::compile::jit_force_slow_spec() {
        word |= AttentionBit::ForceSlowSpec as u32;
    }
    word
}

impl Context {
    /// Re-derive the attention word after a write to one of its inputs.
    /// Every writer of `quit_flag`, `throw_on_input` and
    /// `compiler_function_overrides_active` calls this
    /// (`sync_cached_runtime_binding_by_id`, `set_quit_flag_value`); the
    /// constructors initialize the word from the same inputs.
    #[inline]
    pub(super) fn refresh_attention(&mut self) {
        self.attention = attention_of(
            self.quit_flag,
            self.throw_on_input,
            self.compiler_function_overrides_active,
        );
    }

    /// Whether this Context's own word is clear under MASK, without the
    /// asynchronous word: for a gate that polls separately
    /// (`subr_spec_armed`, which its callers reach after their quit poll).
    #[inline(always)]
    pub(crate) fn attention_word_clear(&self, mask: AttentionMask) -> bool {
        #[cfg(debug_assertions)]
        self.assert_attention_is_derived();
        self.attention & mask.0 == 0
    }

    /// True when neither this Context's attention word under MASK nor the
    /// process's asynchronous word asks for attention: two loads, GNU's
    /// `!NILP (Vquit_flag) || pending_signals` shape.
    #[inline(always)]
    pub(crate) fn attention_clear(&self, mask: AttentionMask) -> bool {
        #[cfg(debug_assertions)]
        self.assert_attention_is_derived();
        (self.attention & mask.0) | ASYNC_ATTENTION.load() == 0
    }

    /// The invariant [`Self::attention_clear`] relies on, checked at every
    /// read in debug builds: a writer of an input that skipped
    /// [`Self::refresh_attention`] would lose a C-g or a `throw-on-input`.
    #[cfg(debug_assertions)]
    #[inline(never)]
    fn assert_attention_is_derived(&self) {
        assert_eq!(
            self.attention,
            attention_of(
                self.quit_flag,
                self.throw_on_input,
                self.compiler_function_overrides_active,
            ),
            "stale attention word: a writer of quit-flag/throw-on-input/\
             compiler overrides skipped refresh_attention"
        );
    }

    /// The attention word as stored, and as its inputs imply it.
    #[cfg(test)]
    pub(crate) fn attention_words_for_test(&self) -> (u32, u32) {
        (
            self.attention,
            attention_of(
                self.quit_flag,
                self.throw_on_input,
                self.compiler_function_overrides_active,
            ),
        )
    }

    /// Re-derive the word after a test flipped the force harness's
    /// thread-local override (`jit::compile::force_slow_spec_for_test`).
    #[cfg(test)]
    pub(crate) fn refresh_attention_for_test(&mut self) {
        self.refresh_attention();
    }
}

/// A process-wide asynchronous poll source: GNU's `pending_signals`
/// (src/keyboard.c:105) generalized to the things that arrive from other
/// threads or from signal context.  Each owns one bit of [`ASYNC_ATTENTION`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum AsyncSource {
    /// The Lisp profiler's watchdog asked for a sample
    /// (`Context::profiler_sample_tick` consumes it).
    ProfilerTick = 1 << 0,
    /// A handled OS signal is pending (`os_signal::drain_pending_os_signals`
    /// consumes it).
    OsSignal = 1 << 1,
}

/// Bits 8..32 of [`ASYNC_ATTENTION`] count the raised [`QuitRequest`]s of the
/// process. Only a lowered-to-raised transition adds a unit and only a
/// raised-to-lowered one removes it, so the count never drifts.
const QUIT_REQUEST_UNIT: u32 = 1 << 8;

/// The word behind [`ASYNC_ATTENTION`], on a cache line of its own: other
/// threads and signal handlers write it, the evaluator reads it at every
/// safe point.
#[repr(C, align(64))]
pub(crate) struct AsyncAttention {
    word: AtomicU32,
}

/// Nonzero while any asynchronous source may need a safe point's attention.
/// A set bit is a hint: the slow path re-derives the exact answer from each
/// source's own state, so a stale bit costs one cold trip, never a lost event.
pub(crate) static ASYNC_ATTENTION: AsyncAttention = AsyncAttention {
    word: AtomicU32::new(0),
};

impl AsyncAttention {
    /// The safe point's fast test: zero means nothing asynchronous is due.
    #[inline(always)]
    pub(crate) fn load(&self) -> u32 {
        self.word.load(Ordering::Relaxed)
    }

    /// Raise SOURCE.  One lock-free atomic RMW (`lock or`), so it is
    /// async-signal-safe: `os_signal`'s handler calls it.
    #[inline(always)]
    pub(crate) fn raise(&self, source: AsyncSource) {
        self.word.fetch_or(source as u32, Ordering::Release);
    }

    /// Lower SOURCE; true when it was raised.
    #[inline]
    pub(crate) fn take(&self, source: AsyncSource) -> bool {
        self.word.fetch_and(!(source as u32), Ordering::AcqRel) & source as u32 != 0
    }

    /// Whether SOURCE is raised, without consuming it.
    #[inline(always)]
    pub(crate) fn is_raised(&self, source: AsyncSource) -> bool {
        self.load() & source as u32 != 0
    }

    /// The word's address, for compiled code that inlines the safe point's
    /// fast test.  Only JIT code may bake it (an AOT artifact outlives the
    /// process whose address it would carry).
    pub(crate) fn addr(&self) -> usize {
        std::ptr::from_ref(&self.word) as usize
    }

    /// How many raised quit requests the word counts.
    #[cfg(test)]
    pub(crate) fn raised_quit_requests_for_test(&self) -> u32 {
        self.load() / QUIT_REQUEST_UNIT
    }
}

/// The cross-thread C-g request.
///
/// GNU's `handle_interrupt` writes `Vquit_flag = Qt` from the signal handler
/// (src/keyboard.c:12719).  Neomacs's input-bridge threads cannot reach the
/// `Context`, so they raise this instead, and the next safe point drains it
/// into `quit-flag` (`Context::maybe_quit_slow`).
///
/// The flag itself is private: [`QuitRequest::request`] is the only way
/// another thread can raise it, and it also counts the raise in
/// [`ASYNC_ATTENTION`], which is the word every safe point tests.
#[derive(Clone, Debug)]
pub struct QuitRequest(Arc<QuitRequestCell>);

#[derive(Debug)]
struct QuitRequestCell {
    raised: AtomicBool,
}

impl QuitRequest {
    /// A lowered request.
    pub fn new() -> Self {
        Self(Arc::new(QuitRequestCell {
            raised: AtomicBool::new(false),
        }))
    }

    /// Raise the request (the input-bridge threads, on a C-g).
    ///
    /// The flag goes up before the count, so a safe point that sees the
    /// count finds the flag; one that polls in between notices the request
    /// at its next poll, which is GNU's own latency ("the next maybe_quit").
    pub fn request(&self) {
        if !self.0.raised.swap(true, Ordering::AcqRel) {
            ASYNC_ATTENTION
                .word
                .fetch_add(QUIT_REQUEST_UNIT, Ordering::Release);
        }
    }

    /// Whether the request is raised, without consuming it.
    #[inline(always)]
    pub(crate) fn is_requested(&self) -> bool {
        self.0.raised.load(Ordering::Relaxed)
    }

    /// Consume the request on the evaluator thread: true when it was raised.
    #[inline]
    pub(crate) fn take(&self) -> bool {
        if self.0.raised.swap(false, Ordering::AcqRel) {
            ASYNC_ATTENTION
                .word
                .fetch_sub(QUIT_REQUEST_UNIT, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Lower the request without looking at it.
    #[inline]
    pub(crate) fn clear(&self) {
        let _ = self.take();
    }
}

impl Default for QuitRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for QuitRequestCell {
    /// The last handle is gone while the request is raised: nobody can take
    /// it any more, so release its unit of [`ASYNC_ATTENTION`].
    fn drop(&mut self) {
        if *self.raised.get_mut() {
            ASYNC_ATTENTION
                .word
                .fetch_sub(QUIT_REQUEST_UNIT, Ordering::Release);
        }
    }
}

#[cfg(test)]
#[path = "tests/quit_request.rs"]
mod quit_request_tests;

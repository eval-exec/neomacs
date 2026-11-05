//! Poll sources that send a safe point off its loads-only fast path.
//!
//! The asynchronous half — [`ASYNC_ATTENTION`], the process-wide word for the
//! Lisp profiler's tick, a handled OS signal and raised `QuitRequest`s — moved
//! to `neovm_value::quit` with the stage-3b T4.1 crate move, together with the
//! thread-local quit-request slot.  This module keeps the evaluator's
//! per-Context word and the masks a safe point tests.
//!
//! * `Context::attention`, one per-Context word derived from the evaluator's
//!   own state that a safe point must look at ([`AttentionBit`]).  GNU's
//!   `Vquit_flag` half of the test.
//! * [`ASYNC_ATTENTION`], GNU's `pending_signals` half (src/keyboard.c:105),
//!   now read from the lower crate.
//!
//! A safe point is clear exactly when both words are clear under its
//! [`AttentionMask`]: two loads, as in GNU.
//!
//! A child module of `eval`, like its siblings, so it keeps the same view of
//! `Context` and the parent's private items (`use super::*`).

use neovm_value::quit::ASYNC_ATTENTION;

use super::Context;
use neovm_value::value::Value;

#[cfg(test)]
use neovm_value::quit::QuitRequest;

/// A Context condition that sends a safe point off its loads-only fast path.
/// A SET bit means "may need attention" (a superset); the slow path always
/// re-derives the exact answer from the canonical fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
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
    /// The process-constant callback census. Only bytecode callback entry
    /// includes this bit; ordinary quit polls and compiled guards ignore it.
    #[cfg(feature = "jit")]
    InlineCensus = 1 << 4,
}

/// Which [`AttentionBit`]s a particular safe point or call gate must see
/// clear, besides [`ASYNC_ATTENTION`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AttentionMask(u32);

impl AttentionMask {
    /// `maybe_quit`'s fast test: GNU's `Vquit_flag`, plus the host-input
    /// poll `throw-on-input` needs here.
    #[doc(hidden)]
    pub const QUIT: Self = Self(AttentionBit::QuitFlag as u32 | AttentionBit::ThrowOnInput as u32);

    /// Bytecode callback entry: census recording precedes the quit poll.
    /// Sharing its guard avoids an extra flag load on every mapped element.
    #[cfg(feature = "jit")]
    #[doc(hidden)]
    pub const CALLBACK_ENTRY: Self = Self(Self::QUIT.0 | AttentionBit::InlineCensus as u32);

    /// `neovm_jit_call_spec`'s gate: the quit poll it makes first, plus the
    /// force harness, which sends every call to the re-validating slow half.
    /// (That half polls with [`Self::QUIT`] only, so the harness bit cannot
    /// make it loop.)
    #[doc(hidden)]
    pub const SPEC_CALL: Self = Self(Self::QUIT.0 | AttentionBit::ForceSlowSpec as u32);

    /// The subr spec shims' gate: [`Self::SPEC_CALL`], plus active compiler
    /// overrides, which refuse a direct subr call.
    #[doc(hidden)]
    pub const SPEC_SUBR: Self = Self(Self::SPEC_CALL.0 | AttentionBit::CompilerOverrides as u32);

    /// What `subr_spec_armed` must see clear before it trusts an equal epoch:
    /// the two conditions besides a stale epoch that leave its compare.
    #[doc(hidden)]
    pub const SUBR_ARMING: Self =
        Self(AttentionBit::CompilerOverrides as u32 | AttentionBit::ForceSlowSpec as u32);

    /// The JIT's inline entry guards (an inlined callee, an inline
    /// `type-of`): the quit poll the skipped call would make, plus active
    /// compiler overrides. Not the force harness: an inlined call has no spec
    /// slot to re-validate, and the harness's compile-time switch already
    /// withholds the inline `type-of`.
    #[doc(hidden)]
    pub const INLINE_ENTRY: Self = Self(Self::QUIT.0 | AttentionBit::CompilerOverrides as u32);

    /// The mask as the word compiled code ANDs with the attention word.
    #[inline(always)]
    #[doc(hidden)]
    pub const fn bits(self) -> u32 {
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
    #[cfg(feature = "jit")]
    if crate::emacs_core::jit::stats::inline_census::enabled() {
        word |= AttentionBit::InlineCensus as u32;
    }
    word
}

impl Context {
    /// Re-derive the attention word after a write to one of its inputs.
    /// Every writer of `quit_flag`, `throw_on_input` and
    /// `compiler_function_overrides_active` calls this
    /// (`sync_cached_runtime_binding_by_id`, `set_quit_flag_value`); the
    /// constructors initialize the word from the same inputs. Process knobs
    /// (force-slow-spec and the callback census) are derived here as well.
    #[inline]
    #[doc(hidden)]
    pub fn refresh_attention(&mut self) {
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
    #[doc(hidden)]
    pub fn attention_word_clear(&self, mask: AttentionMask) -> bool {
        #[cfg(debug_assertions)]
        self.assert_attention_is_derived();
        self.attention & mask.0 == 0
    }

    /// True when neither this Context's attention word under MASK nor the
    /// process's asynchronous word asks for attention: two loads, GNU's
    /// `!NILP (Vquit_flag) || pending_signals` shape.
    #[inline(always)]
    #[doc(hidden)]
    pub fn attention_clear(&self, mask: AttentionMask) -> bool {
        #[cfg(debug_assertions)]
        self.assert_attention_is_derived();
        (self.attention & mask.0) | ASYNC_ATTENTION.load() == 0
    }

    /// The invariant [`Self::attention_clear`] relies on, checked at every
    /// read in debug builds: a writer of an input that skipped
    /// [`Self::refresh_attention`] would lose a C-g or a `throw-on-input`.
    #[cfg(debug_assertions)]
    #[inline(never)]
    #[doc(hidden)]
    pub fn assert_attention_is_derived(&self) {
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
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn attention_words_for_test(&self) -> (u32, u32) {
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
    /// thread-local override (`jit::compile::force_slow_spec_for_test`), or
    /// the callback census's scalar test override.
    #[cfg(test)]
    #[doc(hidden)]
    pub fn refresh_attention_for_test(&mut self) {
        self.refresh_attention();
    }
}

#[cfg(test)]
#[path = "tests/quit_request_test.rs"]
mod quit_request_tests;

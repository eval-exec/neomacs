//! Per-source feedback (design `p2-1-feedback-reopt` §3.2): what the
//! interpreter observes about one byte-code source, shared by every
//! `make-closure` instance through its [`RuntimeState`](super::RuntimeState).
//!
//! [`SourceFeedback`] is the one owner of a source's observations. Today it
//! holds the per-op word ([`FeedbackVec`]): the numeric lattice at arithmetic
//! pcs (and the legacy symbol-only call lattice at call pcs). Compiles read
//! it once, through the compile-time snapshot (`compile::snapshot`), never
//! piecemeal.
//!
//! Every structure here holds only integers and `SymId`s: none holds a Lisp
//! `Value`, so the collector never traces feedback and feedback never
//! dangles. Relaxed atomics throughout: the mutator is the only writer.

use super::{CallFeedback, FeedbackVec, NumericFeedback};
use crate::emacs_core::intern::SymId;

/// One source's feedback (see the module docs).
#[derive(Debug, Default)]
pub(crate) struct SourceFeedback {
    /// Per-op word: `NumericFeedback` at arithmetic pcs (the encoding the
    /// lowering's one predicate, `arith_site_takes_generic`, reads), the
    /// legacy symbol-only `CallFeedback` at call pcs.
    ops: FeedbackVec,
}

impl SourceFeedback {
    /// No feedback yet (nothing allocated).
    #[inline]
    pub(crate) const fn new() -> Self {
        Self {
            ops: FeedbackVec::new(),
        }
    }

    /// Record callee `sym` at call site `pc` (the legacy lattice).
    #[inline]
    pub(crate) fn record_call(&self, pc: usize, ops_len: usize, sym: SymId) {
        self.ops.record_call(pc, ops_len, sym);
    }

    /// The legacy call lattice at `pc`.
    #[inline]
    pub(crate) fn call_at(&self, pc: usize) -> CallFeedback {
        self.ops.call_at(pc)
    }

    /// Join `seen` into arithmetic site `pc`; whether the slot moved.
    #[inline]
    pub(crate) fn record_numeric(&self, pc: usize, ops_len: usize, seen: NumericFeedback) -> bool {
        self.ops.record_numeric(pc, ops_len, seen)
    }

    /// The numeric lattice at arithmetic site `pc`.
    #[inline]
    pub(crate) fn numeric_at(&self, pc: usize) -> NumericFeedback {
        self.ops.numeric_at(pc)
    }
}

//! The compile-time feedback snapshot (design `p2-1-feedback-reopt` §3.2.5):
//! everything a compile reads from a source's feedback, taken ONCE per
//! compile and published for its whole lowering.
//!
//! [`FeedbackSnapshot::take`] reads the source (`SourceFeedback`, the retreat
//! table, the `ReoptLevel` ceiling) into plain vectors; [`FeedbackSnapshot::
//! publish`] makes them the ambient answers of [`active_numeric_feedback`],
//! [`call_site_inlinable_at`] and [`arith_site_takes_generic`] until the
//! returned scope drops. Holding no `Value`, a snapshot is `Send`, which is
//! what a later background compile needs.

use super::*;
use crate::emacs_core::jit::NumericFeedback;

thread_local! {
    /// Per-pc operand-type feedback for the body being compiled.
    ///
    /// A thread-local for the same reason `ACTIVE_CALL_HEAVY` is one: the
    /// compile is synchronous on the eval thread, and the alternative is
    /// threading a slice through four signatures (`compile_bytecode_function_inner`
    /// -> the emit fn -> `build_leaf_fn` -> `lower_simple_op`) for a value the
    /// whole lowering treats as ambient.
    ///
    /// Read by the baseline lowering and by `build_mir_with_feedback` at MIR
    /// BUILD time — the only point where a MIR inst's pc indexes this body.
    /// The MIR LOWERING never reads it: after inlining, a spliced callee inst
    /// carries the call site's pc, which would index the wrong body.
    static ACTIVE_NUMERIC_FEEDBACK: std::cell::RefCell<Vec<NumericFeedback>> =
        const { std::cell::RefCell::new(Vec::new()) };

    /// Per ORIGINAL-body pc of the body being compiled: `true` where a deopt
    /// barred the call site from being spliced (the fuser), MIR-inlined or
    /// intrinsified inline (LEVEL-B) — `RuntimeState::call_site_no_inline`,
    /// or every site once the source's `ReoptLevel` reached `NoInline`.
    /// Published with the numeric feedback, by the same scope; empty (every
    /// site inlinable) outside a compile. Read through
    /// [`call_site_inlinable_at`].
    static ACTIVE_NO_INLINE_CALL_SITES: std::cell::RefCell<Vec<bool>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// What one compile reads from its source's feedback (see the module docs).
#[derive(Debug, Default)]
pub(crate) struct FeedbackSnapshot {
    /// Per pc: the numeric lattice, under the source's `ReoptLevel` ceiling
    /// (every arithmetic site reads `Other` at `Generic`).
    pub(crate) numeric: Vec<NumericFeedback>,
    /// Per pc: a deopt barred inlining the call there (every pc at
    /// `NoInline` and above).
    pub(crate) no_inline: Vec<bool>,
    /// The source, when its call sites record (`NEOVM_JIT_FEEDBACK`): the
    /// lowering bakes pointers into its call-site table (built here if the
    /// interpreter never recorded), and reads the targets recorded there
    /// when `read_call_targets` (`=use`). The lattice is read at the site
    /// being lowered: the compile is synchronous, so that is this moment.
    pub(crate) call_source: Option<std::sync::Arc<crate::emacs_core::jit::RuntimeState>>,
    /// Whether the compile may read the recorded call targets.
    pub(crate) read_call_targets: bool,
}

impl FeedbackSnapshot {
    /// Read `f`'s feedback. Taking the snapshot marks the body's numeric
    /// feedback CONSUMED: from here the interpreter stops recording it.
    pub(crate) fn take(f: &ByteCodeFunction) -> Self {
        use crate::emacs_core::jit::ReoptLevel;
        let rt = f.jit_runtime();
        let ops = f.executable_ops();
        let level = rt.reopt_level();
        let generic = level >= ReoptLevel::Generic;
        let numeric = ops
            .iter()
            .enumerate()
            .map(|(pc, op)| {
                if generic && ArithGenericKind::from_op(op).is_some() {
                    NumericFeedback::Other
                } else {
                    rt.numeric_feedback(pc)
                }
            })
            .collect();
        let no_inline = if level >= ReoptLevel::NoInline {
            vec![true; ops.len()]
        } else {
            (0..ops.len())
                .map(|pc| rt.call_site_no_inline(pc))
                .collect()
        };
        rt.note_numeric_feedback_consumed();
        let mode = crate::emacs_core::jit::feedback::feedback_mode();
        let call_source = mode.records().then(|| {
            rt.call_sites_for(ops, &f.constants);
            rt.share_state()
        });
        FeedbackSnapshot {
            numeric,
            no_inline,
            call_source,
            read_call_targets: mode.uses(),
        }
    }

    /// Make this snapshot the compile's ambient feedback until the returned
    /// scope drops (which restores the one it replaced).
    pub(crate) fn publish(self) -> NumericFeedbackScope {
        NumericFeedbackScope(
            Some(
                ACTIVE_NUMERIC_FEEDBACK
                    .with(|v| std::mem::replace(&mut *v.borrow_mut(), self.numeric)),
            ),
            Some(
                ACTIVE_NO_INLINE_CALL_SITES
                    .with(|v| std::mem::replace(&mut *v.borrow_mut(), self.no_inline)),
            ),
            Some(super::call_feedback::CallSourceScope::enter(
                self.call_source,
                self.read_call_targets,
            )),
        )
    }
}

/// Whether the call site at `pc` of the body being compiled may be spliced,
/// MIR-inlined or intrinsified inline — the one predicate every inliner
/// reads (see `ACTIVE_NO_INLINE_CALL_SITES`). `pc` indexes the ops being
/// LOWERED: under a fused scope it is mapped back to the original body's pc,
/// where the deopt that set the bit resumed.
pub(crate) fn call_site_inlinable_at(pc: usize) -> bool {
    let pc = match inline::active_fused() {
        Some(fused) => match fused.caller_pc(pc) {
            Some(caller) => caller,
            None => return true,
        },
        None => pc,
    };
    ACTIVE_NO_INLINE_CALL_SITES.with(|v| !v.borrow().get(pc).copied().unwrap_or(false))
}

/// Operand-type feedback recorded for the arithmetic site at `pc` of the body
/// currently being compiled.
pub(crate) fn active_numeric_feedback(pc: usize) -> NumericFeedback {
    ACTIVE_NUMERIC_FEEDBACK.with(|v| {
        v.borrow()
            .get(pc)
            .copied()
            .unwrap_or(NumericFeedback::FixnumOnly)
    })
}

/// Whether the arithmetic/comparison site `op` at `pc` of the body being
/// compiled is lowered with a GENERIC fallback — an inline fixnum fast path
/// whose miss (a non-fixnum operand, an overflow, a zero divisor) calls the
/// interpreter's own builtin through `neovm_jit_arith_generic` — instead of
/// deopting.
///
/// That is the lowering for a site whose feedback says the fixnum guard
/// fails there: `Other` (bignums, markers, overflow) at `+ - * / = < > <= >=`,
/// which have a float lowering for `Float`; and any non-`FixnumOnly` site at
/// `% max min 1+ 1- -`, which have none. Deopting was a round trip to the
/// interpreter per call: `pidigits` deopted 4,298 times per repeat, and the
/// JIT made it 3.7% SLOWER than no JIT at all.
///
/// ONE predicate for every reader, as the float lowering learned the hard
/// way: the lowering, the known-fixnum analysis (such a site's result is not a
/// fixnum), `baseline_needs_rt` (the fallback calls a shim) and the MIR tier
/// gate (MIR guards fixnum and would rerun-from-start).
pub(crate) fn arith_site_takes_generic(op: &Op, pc: usize) -> bool {
    match op {
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Eqlsign
        | Op::Lss
        | Op::Gtr
        | Op::Leq
        | Op::Geq => active_numeric_feedback(pc) == NumericFeedback::Other,
        Op::Rem | Op::Max | Op::Min | Op::Add1 | Op::Sub1 | Op::Negate => {
            active_numeric_feedback(pc) != NumericFeedback::FixnumOnly
        }
        _ => false,
    }
}

/// Restores the numeric feedback (and, for a whole-body publish, the
/// no-inline call sites and the call-feedback source) the compile inside it
/// replaced.
pub(crate) struct NumericFeedbackScope(
    Option<Vec<NumericFeedback>>,
    Option<Vec<bool>>,
    #[expect(
        dead_code,
        reason = "held for its Drop, which restores the outer source"
    )]
    Option<super::call_feedback::CallSourceScope>,
);

impl Drop for NumericFeedbackScope {
    fn drop(&mut self) {
        if let Some(prev) = self.0.take() {
            ACTIVE_NUMERIC_FEEDBACK.with(|v| *v.borrow_mut() = prev);
        }
        if let Some(prev) = self.1.take() {
            ACTIVE_NO_INLINE_CALL_SITES.with(|v| *v.borrow_mut() = prev);
        }
    }
}

/// Publish a numeric-feedback vector built elsewhere — the fused body's,
/// whose spliced slots carry the CALLEE's feedback. The no-inline call sites
/// stay as published for the original body: they are keyed by original pc,
/// which [`call_site_inlinable_at`] maps a fused pc back to.
pub(crate) fn publish_numeric_feedback_vec(seen: Vec<NumericFeedback>) -> NumericFeedbackScope {
    NumericFeedbackScope(
        Some(ACTIVE_NUMERIC_FEEDBACK.with(|v| std::mem::replace(&mut *v.borrow_mut(), seen))),
        None,
        None,
    )
}

/// Take and publish `f`'s feedback snapshot for the compile in progress (see
/// [`FeedbackSnapshot`]). Every lowering that reads `active_numeric_feedback`
/// — the tier-up path AND the OSR path — must run inside one of these:
/// `compile_osr_leaf` lowered outside it, so an OSR-entered float loop read
/// `FixnumOnly` at every site, failed its fixnum guards, set
/// `OSR_TRIED_FLAG`, and stayed interpreted for good (a fixnum loop OSR'd
/// 2.8x faster; the same loop on floats got nothing).
pub(crate) fn publish_numeric_feedback(f: &ByteCodeFunction) -> NumericFeedbackScope {
    FeedbackSnapshot::take(f).publish()
}

#[path = "snapshot/array_feedback.rs"]
mod array_feedback;
pub(crate) use array_feedback::{
    FrontFeedbackScope, SelectedFeedbackSnapshot, publish_numeric_feedback_with_arrays,
};

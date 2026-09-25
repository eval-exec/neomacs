//! Deopt-driven reoptimization: what a deopt says about the speculation it
//! broke, and (from the policy commits on) what to do about it.
//!
//! Every point that consumes a deopt calls [`note_deopt`] once, on its cold
//! arm, BEFORE the resumed interpreter frame seeds `bc_buf`:
//!
//! | consumer | origin | event |
//! |---|---|---|
//! | `cache::finish_native_run` (the tier-up seam) | entry | rerun, precise |
//! | `cache::finish_framed_run` (framed native-to-native, spec shim) | entry | rerun |
//! | `cache::deopt_resume_outcome` (framed and direct precise resumes) | entry | precise |
//! | `cache::direct_call_cold` (handler-free native-to-native) | entry | rerun |
//! | `cache::try_run_osr` (loop transfer) | OSR | rerun, precise |
//!
//! The deopt is classified from data it already carries — the op at the
//! resume pc and the spilled pre-op operand stack — so the generated code,
//! the ABI and the hot paths are untouched. The per-pc counts come from the
//! leaf's release counters ([`super::compile::LeafObs`]), which every deopt
//! has already bumped by the time it reaches the hook.
//!
//! GC window: the hook runs while the resume stack is still unrooted (it is
//! seeded into the traced `bc_buf` only afterwards). It must not allocate on
//! the Lisp heap, reach a safepoint or run Lisp. Rust-heap allocation is
//! fine, as in `CompiledLeaf::deopt_at_outcome`.

use super::NumericFeedback;
use super::compile::CompiledLeaf;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

/// Which kind of leaf deopted.
#[derive(Clone, Copy, Debug)]
pub(crate) enum LeafOrigin<'a> {
    /// A function-entry leaf (the `COMPILED` cache, live or retired).
    Entry,
    /// An OSR leaf entered at `header_pc` with the interpreter's operand
    /// stack `snapshot` (the `OSR_CACHE`).
    Osr {
        header_pc: usize,
        snapshot: &'a [Value],
    },
}

impl LeafOrigin<'_> {
    fn is_osr(self) -> bool {
        matches!(self, LeafOrigin::Osr { .. })
    }
}

/// What the deopt carried.
#[derive(Clone, Copy, Debug)]
pub(crate) enum DeoptEvent<'a> {
    /// `STATUS_DEOPT_AT`: the resume pc and the pre-op operand stack
    /// (`DeoptResume`).
    Precise { pc: usize, stack: &'a [Value] },
    /// `STATUS_DEOPT`: rerun from the start. No pc (pure MIR bodies).
    Rerun,
}

/// Why a deopt happened, derived from data the deopt already carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeoptCause {
    /// An arithmetic site met operands its lowering does not take (a float
    /// or a bignum/marker/non-number at a fixnum site). Conclusive.
    ArithOperands(NumericFeedback),
    /// All-fixnum operands at an arithmetic site: an overflow, a zero
    /// divisor, `1+` at the boundary.
    ArithOverflow,
    /// A spliced region, MIR-inlined callee or inline bit-op guard failed at
    /// a call site.
    InlinedCall,
    /// A MIR `InlineEntry` guard failed because `function_epoch` moved.
    InlineEpochMoved,
    /// `car`/`cdr` of a non-list: the interpreter is about to signal.
    TypeError,
    /// An OSR entry guard refused the transfer (pc == header, stack ==
    /// snapshot).
    OsrEntry,
    /// Rerun-from-start (no pc).
    Rerun,
    /// A precise deopt at an op none of the above covers.
    Unattributed,
}

impl DeoptCause {
    /// Census buckets: [`Self::census_index`] into these names.
    pub(crate) const CENSUS_NAMES: [&'static str; 9] = [
        "arith_float",
        "arith_other",
        "overflow",
        "inlined_call",
        "inline_epoch",
        "type_error",
        "osr_entry",
        "rerun",
        "unattributed",
    ];

    /// This cause's census bucket.
    pub(crate) fn census_index(self) -> usize {
        match self {
            // `FixnumOnly` operands are an overflow, never classified here.
            DeoptCause::ArithOperands(NumericFeedback::Float) => 0,
            DeoptCause::ArithOperands(_) => 1,
            DeoptCause::ArithOverflow => 2,
            DeoptCause::InlinedCall => 3,
            DeoptCause::InlineEpochMoved => 4,
            DeoptCause::TypeError => 5,
            DeoptCause::OsrEntry => 6,
            DeoptCause::Rerun => 7,
            DeoptCause::Unattributed => 8,
        }
    }
}

/// Classify a precise deopt at `pc` with pre-op operand stack `stack`.
///
/// Sound because the resume pc always indexes the ORIGINAL body: fused
/// non-region pcs are mapped back through `caller_pc` (`deopt_site`), a
/// region deopt writes its call-site pc, MIR insts carry their original pc
/// (or the call-site pc once spliced), and `stack` is the pre-op stack, so
/// an arithmetic op's operands are its top `nargs` slots.
pub(crate) fn classify(
    ctx: *const Context,
    ops: &[Op],
    leaf: &CompiledLeaf,
    origin: LeafOrigin<'_>,
    pc: usize,
    stack: &[Value],
) -> DeoptCause {
    let Some(op) = ops.get(pc) else {
        return DeoptCause::Unattributed;
    };
    let arith = Vm::arith_generic_kind(op);
    if let Some((_, nargs)) = arith
        && let Some(base) = stack.len().checked_sub(nargs)
    {
        match NumericFeedback::of_operands(&stack[base..]) {
            // An overflow, or an OSR entry guard at an arithmetic header.
            NumericFeedback::FixnumOnly => {}
            seen => return DeoptCause::ArithOperands(seen),
        }
    }
    if let LeafOrigin::Osr {
        header_pc,
        snapshot,
    } = origin
        && pc == header_pc
        && stack.len() == snapshot.len()
        && stack
            .iter()
            .zip(snapshot)
            .all(|(a, b)| a.bits() == b.bits())
    {
        return DeoptCause::OsrEntry;
    }
    match op {
        _ if arith.is_some() => DeoptCause::ArithOverflow,
        Op::Call(_)
            if !ctx.is_null()
                // SAFETY: the dormant seam-provided Context (the deopt
                // consumers' contract); a shared scalar read.
                && leaf
                    .inline_epoch()
                    .is_some_and(|e| unsafe { (*ctx).obarray.function_epoch() } != e) =>
        {
            DeoptCause::InlineEpochMoved
        }
        Op::Call(_) => DeoptCause::InlinedCall,
        Op::Car | Op::Cdr => DeoptCause::TypeError,
        _ => DeoptCause::Unattributed,
    }
}

/// The deopt hook (see the module docs). Cold: runs only on deopt exits.
#[cold]
#[inline(never)]
pub(crate) fn note_deopt(
    ctx: *const Context,
    func: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    origin: LeafOrigin<'_>,
    event: DeoptEvent<'_>,
) {
    let cause = match event {
        DeoptEvent::Rerun => DeoptCause::Rerun,
        DeoptEvent::Precise { pc, stack } => {
            classify(ctx, func.executable_ops(), leaf, origin, pc, stack)
        }
    };
    super::stats::record_deopt(cause, origin.is_osr());
    tracing::trace!(
        target: "neovm_jit::deopt",
        id = leaf.obs.id,
        ?cause,
        osr = origin.is_osr(),
        "deopt"
    );
}

#[cfg(test)]
#[path = "reopt/tests/reopt_test.rs"]
mod tests;

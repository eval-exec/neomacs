//! Compile-time profitability selection for the opt backend.
//!
//! Threading: all decisions use immutable process configuration and scalar
//! facts borrowed by one compiler. Source heat already uses atomics; no Lisp
//! handles, runtime caches, feedback recording or mutator state are added.

use super::{CompileRequest, Op, OptMode, jit_opt_mode, jit_opt_profit};
use crate::emacs_core::jit::tier2::CompileTier;

/// The frontier to try for this compilation. Threading: compiler-local scalar
/// selection; it is not published into a leaf or consulted at native entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FrontChoice {
    /// Follow the existing backend selector exactly.
    Current,
    /// Keep the original MIR-first and late static-fuser frontend.
    Legacy,
    /// Attempt SSA, returning to Legacy before any fallback code generation.
    Selected,
    /// Preserve successful legacy MIR; select SSA only after MIR declines.
    SelectedAfterMir,
}

/// A bounded kernel's source operations. Threading: immutable compiler counts;
/// they contain no Lisp values, per-site state or assumed mutator identity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Work {
    useful: usize,
    backedge: bool,
    unsupported_control: bool,
}

impl Work {
    fn read(ops: &[Op]) -> Self {
        let mut work = Self::default();
        for (pc, op) in ops.iter().enumerate() {
            match op {
                // build::Builder::new refuses these reachable operations:
                // handler edges need push-time stacks that opt does not model.
                // Avoid a CFG build here and conservatively reject dead copies
                // too; they may otherwise cause a second legacy MIR attempt.
                Op::PushConditionCase(_)
                | Op::PushConditionCaseRaw(_)
                | Op::PushCatch(_)
                | Op::PopHandler
                | Op::Throw => work.unsupported_control = true,
                Op::Goto(target)
                | Op::GotoIfNil(target)
                | Op::GotoIfNotNil(target)
                | Op::GotoIfNilElsePop(target)
                | Op::GotoIfNotNilElsePop(target) => {
                    work.backedge |= (*target as usize) <= pc;
                }
                Op::Add
                | Op::Sub
                | Op::Mul
                | Op::Div
                | Op::Rem
                | Op::Add1
                | Op::Sub1
                | Op::Negate
                | Op::Max
                | Op::Min
                | Op::Eqlsign
                | Op::Lss
                | Op::Gtr
                | Op::Leq
                | Op::Geq
                | Op::Eq
                | Op::Not
                | Op::Consp
                | Op::Listp
                | Op::Integerp
                | Op::Numberp
                | Op::Car
                | Op::Cdr
                | Op::CarSafe
                | Op::CdrSafe
                | Op::Setcar
                | Op::Setcdr
                | Op::Cons => work.useful += 1,
                _ => {}
            }
        }
        work
    }
}

/// Consume the caller's existing j17 profitability verdict; do not repeat its
/// symbol/intrinsic analysis. Numeric feedback is not execution evidence:
/// FixnumOnly also means unseen. This policy admits floating-point kernels by
/// shape and leaves representation selection to the existing feedback passes.
#[cold]
#[inline(never)]
pub(crate) fn body_admitted(
    mode: super::OptProfitMode,
    ops: &[Op],
    call_heavy: bool,
    hot: bool,
) -> bool {
    if mode == super::OptProfitMode::Off {
        return true;
    }
    if call_heavy || (super::jit_opt_max_ops() != 0 && ops.len() > super::jit_opt_max_ops()) {
        return false;
    }
    let work = Work::read(ops);
    if work.unsupported_control {
        return false;
    }
    if work.backedge {
        return work.useful > 0;
    }
    mode == super::OptProfitMode::Kernels && hot && ops.len() <= 64 && work.useful >= 2
}

/// Preserve legacy T1 code while allowing selected existing Retier upgrades.
/// Feedback upgrades are already stable/hot; Retier upgrades already crossed
/// the native allocator threshold. Neither needs additional heat recording.
#[cold]
#[inline(never)]
pub(super) fn front(
    request: CompileRequest,
    ops: &[Op],
    call_heavy: bool,
    source: &crate::emacs_core::jit::RuntimeState,
) -> FrontChoice {
    if jit_opt_mode() != OptMode::Opt || jit_opt_profit() == super::OptProfitMode::Off {
        return FrontChoice::Current;
    }
    if !matches!(request.tier, CompileTier::Upgrade(_)) {
        let mode = super::jit_opt_early();
        if mode == super::OptEarlyMode::Off
            || request.tier != CompileTier::T1
            || !body_admitted(super::OptProfitMode::Loops, ops, call_heavy, false)
        {
            return FrontChoice::Legacy;
        }
        // Existing atomic heat is compile evidence only. A concurrent change
        // can alter optimization selection, never call behavior/publication.
        let heat = source.heat();
        let hot = source.is_hot();
        let first_sight =
            request.origin == crate::emacs_core::jit::stats::CompileOrigin::FirstSight;
        if hot || (mode == super::OptEarlyMode::On && first_sight) {
            tracing::debug!(target: "neovm_jit::opt", heat, hot, first_sight,
                source_id = ?source.compiled_id(), origin = ?request.origin,
                threshold = crate::emacs_core::jit::hot_threshold(), ops = ops.len(),
                max_ops = super::jit_opt_max_ops(),
                "early loop candidate keeps legacy MIR before attempting SSA");
            return FrontChoice::SelectedAfterMir;
        }
        return FrontChoice::Legacy;
    }
    if body_admitted(jit_opt_profit(), ops, call_heavy, true) {
        FrontChoice::Selected
    } else {
        FrontChoice::Legacy
    }
}

/// OSR already proves loop heat and is a cold compile seam. Off returns before
/// any additional opcode or profitability scan. Its fallback remains the
/// existing baseline OSR emitter, whose entry snapshot is authoritative.
#[cold]
#[inline(never)]
pub(crate) fn osr_admitted(ops: &[Op], constants: &[super::Value], source_ops_len: usize) -> bool {
    let mode = jit_opt_profit();
    mode == super::OptProfitMode::Off
        || (final_size_admitted(FrontChoice::Selected, source_ops_len)
            && body_admitted(mode, ops, super::body_is_call_heavy(ops, constants), true))
}

#[cfg(test)]
#[path = "opt_profit/tests/policy_test.rs"]
mod tests;

#[cfg(test)]
#[path = "opt_profit/tests/frontend_test.rs"]
mod frontend_tests;

#[cfg(test)]
#[path = "opt_profit/tests/unsupported_test.rs"]
mod unsupported_tests;

/// Check the actual emission slice after fusion. Threading: compiler-local
/// lengths and immutable configuration only, never native-entry state. The
/// unselected frontend returns before reading the size knob.
#[inline]
pub(super) fn final_size_admitted(front: FrontChoice, ops_len: usize) -> bool {
    if !matches!(front, FrontChoice::Selected | FrontChoice::SelectedAfterMir) {
        return true;
    }
    let max = super::jit_opt_max_ops();
    max == 0 || ops_len <= max
}

#[cfg(test)]
#[path = "opt_profit/tests/early_test.rs"]
mod early_tests;

#[cfg(test)]
#[path = "opt_profit/tests/final_size_test.rs"]
mod final_size_tests;

#[cfg(test)]
#[path = "opt_profit/tests/final_size_native_test.rs"]
mod final_size_native_tests;

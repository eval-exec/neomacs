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
    list_backedge: bool,
    unsupported_control: bool,
    bytecode_calls: bool,
}

impl Work {
    fn read(ops: &[Op]) -> Self {
        let mut work = Self::default();
        let mut last_list_pc = None;
        for (pc, op) in ops.iter().enumerate() {
            match op {
                // Aset's live function-cell fallback can run arbitrary Lisp
                // and return a different value from its stored operand. Keep
                // it outside PrimitiveLists until that contract is modeled.
                Op::Aset
                | Op::Call(_)
                | Op::Apply(_)
                | Op::CallBuiltin(..)
                | Op::CallBuiltinSym(..) => work.bytecode_calls = true,
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
                    if (*target as usize) <= pc {
                        work.backedge = true;
                        // Bound the list evidence to a backedge's lexical
                        // body; a list operation in a numeric loop's prefix
                        // or suffix must not qualify that loop. No CFG or
                        // reachability analysis is needed for this filter.
                        work.list_backedge |=
                            last_list_pc.is_some_and(|list_pc| list_pc >= *target as usize);
                    }
                }
                Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe | Op::Setcar | Op::Setcdr => {
                    work.useful += 1;
                    last_list_pc = Some(pc);
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
    if work.unsupported_control
        || (mode == super::OptProfitMode::PrimitiveLists && work.bytecode_calls)
    {
        return false;
    }
    if work.backedge {
        return work.useful > 0
            && (!matches!(
                mode,
                super::OptProfitMode::Lists | super::OptProfitMode::PrimitiveLists
            ) || work.list_backedge);
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
        let early_profit = match jit_opt_profit() {
            super::OptProfitMode::Lists => super::OptProfitMode::Lists,
            super::OptProfitMode::PrimitiveLists => super::OptProfitMode::PrimitiveLists,
            _ => super::OptProfitMode::Loops,
        };
        if mode == super::OptEarlyMode::Off
            || request.tier != CompileTier::T1
            || !body_admitted(early_profit, ops, call_heavy, false)
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

/// Require installed Opt OSR evidence only for a selected normal frontend.
/// Threading: borrowed source facts and the owning mutator's existing cache are
/// read only. No Lisp handle, new runtime state or observation counter escapes.
/// A rejection keeps the original MIR/static-fuser frontend before SSA starts.
#[cold]
#[inline(never)]
pub(super) fn ready_osr_front(
    front: FrontChoice,
    source: &super::ByteCodeFunction,
    obarray: Option<&super::Obarray>,
) -> FrontChoice {
    if !matches!(front, FrontChoice::Selected | FrontChoice::SelectedAfterMir)
        || !super::jit_opt_require_osr()
    {
        return front;
    }
    let lists = matches!(
        jit_opt_profit(),
        super::OptProfitMode::Lists | super::OptProfitMode::PrimitiveLists
    );
    let mut last_list_pc = None;
    // Query exact original source headers, not fused PCs or the entire cache.
    // Lists requires evidence at a list-containing backedge; a separate hot
    // numeric loop must not qualify a cold list loop in the same source.
    let headers = source
        .executable_ops()
        .iter()
        .enumerate()
        .filter_map(|(pc, op)| match op {
            Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe | Op::Setcar | Op::Setcdr => {
                last_list_pc = Some(pc);
                None
            }
            Op::Goto(target)
            | Op::GotoIfNil(target)
            | Op::GotoIfNotNil(target)
            | Op::GotoIfNilElsePop(target)
            | Op::GotoIfNotNilElsePop(target) => {
                let target = *target as usize;
                (target <= pc && (!lists || last_list_pc.is_some_and(|list_pc| list_pc >= target)))
                    .then_some(target)
            }
            _ => None,
        });
    if crate::emacs_core::jit::cache::has_ready_opt_osr(
        source.jit_runtime(),
        obarray.map(super::Obarray::generation),
        headers,
    ) {
        front
    } else {
        FrontChoice::Legacy
    }
}

/// Preserve original callback evidence even when the OSR slice/fuser omits it.
/// Threading: immutable source/configuration reads at the cold cache compile
/// seam. No source state, Lisp handle or runtime observation is recorded.
/// Other modes return before scanning, retaining their existing OSR policy.
#[cold]
#[inline(never)]
pub(crate) fn primitive_osr_source_admitted(source_ops: &[Op]) -> bool {
    jit_opt_profit() != super::OptProfitMode::PrimitiveLists
        || !source_ops.iter().any(|op| {
            matches!(
                op,
                Op::Aset
                    | Op::Call(_)
                    | Op::Apply(_)
                    | Op::CallBuiltin(..)
                    | Op::CallBuiltinSym(..)
            )
        })
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

#[cfg(test)]
#[path = "opt_profit/tests/lists_policy_test.rs"]
mod lists_policy_tests;

#[cfg(test)]
#[path = "opt_profit/tests/lists_frontend_test.rs"]
mod lists_frontend_tests;

#[cfg(test)]
#[path = "opt_profit/tests/lists_osr_test.rs"]
mod lists_osr_tests;

#[cfg(test)]
#[path = "opt_profit/tests/ready_osr_test.rs"]
mod ready_osr_tests;

#[cfg(test)]
#[path = "opt_profit/tests/primitive_lists_test.rs"]
mod primitive_lists_tests;

#[cfg(test)]
#[path = "opt_profit/tests/advised_opaque_test.rs"]
mod advised_opaque_tests;

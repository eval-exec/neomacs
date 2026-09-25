//! Call contracts shared by baseline JIT, typed MIR and AOT lowering.
//!
//! A specialization describes its guarded fast path separately from the whole
//! call. A failed guard can run arbitrary Lisp through the generic fallback;
//! callers must never use fast-path effects to discard that path's GC roots or
//! to move state reads across it.

use super::*;

/// Independent effects, rather than a strongest-effect ordering: declared
/// beside the leaf builtins that carry them (`subr::leaf`).
pub use crate::emacs_core::subr::leaf::Effects;

#[derive(Clone, Copy, Debug)]
pub(crate) struct NamedBuiltinCall {
    pub kind: SpecCalleeKind,
    pub fast_effects: Effects,
    pub effects: Effects,
}

pub(crate) fn named_builtin_call(op: &Op) -> Option<NamedBuiltinCall> {
    let Op::CallBuiltinSym(sym, nargs) = op else {
        return None;
    };
    let kind = cbsym_spec_kind(*sym, *nargs as usize)?;
    let fast_effects = match kind {
        SpecCalleeKind::CbsymTierA { which } => {
            let reads = Effects::READ_BUFFER;
            let reads = if matches!(which, CBSYM_A_MATCH_BEGINNING | CBSYM_A_MATCH_END) {
                reads.with(Effects::READ_MATCH)
            } else {
                reads
            };
            // Type/arity errors signal; the successful Tier-A read itself
            // neither allocates nor calls Lisp. Only exact nullary reads are
            // currently qualified for admission as read-only loop adapters.
            if *nargs == 0 && dispatch::cbsym_read_expected_nargs(which) == 0 {
                reads
            } else {
                reads.with(Effects::MAY_SIGNAL)
            }
        }
        _ => Effects::UNKNOWN,
    };
    Some(NamedBuiltinCall {
        kind,
        fast_effects,
        effects: Effects::UNKNOWN,
    })
}

/// The whole-call effects of an opcode site's lowering (design
/// `p1-2-builtin-intrinsics` §2.8): the declared effects of the leaf it
/// runs when that lowering is a leaf trampoline or a GC-free shim -- a value
/// shim (`aref`, `memq`, `assq`, `setcar`, `setcdr`) or a `JitBuiltin*Pure`
/// table entry, each a `&Context` body that cannot collect, run Lisp or
/// deopt -- and `UNKNOWN` for anything else (an op that is not a direct
/// builtin, or a rooted table entry such as `string=` without its leaf).
/// Intrinsic prefixes (`intrinsics`) only read, so they add nothing.
pub(crate) fn opcode_site_effects(op: &Op, aot: bool) -> Effects {
    use crate::emacs_core::subr::leaf::LeafId;
    if let Some(id) = super::leaf_abi::opcode_leaf_site(op, aot) {
        return id.spec().effects;
    }
    let gc_free = match op {
        Op::Aref | Op::Memq | Op::Assq | Op::Setcar | Op::Setcdr => true,
        _ => match super::dispatch::direct_builtin_spec(op) {
            Some((1, idx)) => super::dispatch::JIT_BUILTIN1_PURE[idx].is_some(),
            Some((2, idx)) => super::dispatch::JIT_BUILTIN2_PURE[idx].is_some(),
            _ => false,
        },
    };
    if !gc_free {
        return Effects::UNKNOWN;
    }
    // The value shims of `memq`/`assq` run those leaves' bodies.
    let leaf = match op {
        Op::Memq => Some(LeafId::Memq),
        Op::Assq => Some(LeafId::Assq),
        _ => super::leaf_abi::opcode_leaf(op),
    };
    match (leaf, op) {
        (Some(id), _) => id.spec().effects,
        (None, Op::Setcar | Op::Setcdr | Op::Nreverse) => {
            Effects::WRITE_HEAP.with(Effects::MAY_SIGNAL)
        }
        (None, _) => Effects::READ_HEAP.with(Effects::MAY_SIGNAL),
    }
}

/// Whether an opcode site is a leaf call in the MIR tier's sense
/// (`NEOVM_JIT_LEAF_EFFECTS`): it calls a leaf builtin's body -- through its
/// trampoline, the `memq`/`assq` value shims or a pure table entry -- and
/// its lowering cannot collect, run Lisp or deopt, so a loop may contain it
/// and the values live across it need no roots.
///
/// The GC-free ops the baseline lowers INLINE (`aref`, `setcar`, `setcdr`)
/// and the pure entries without a leaf (`symbol-function`, `nreverse`) are
/// not leaf calls here: admitting their loops into the MIR tier was
/// measured slower (bubble-no-cons +4.1%, inclist +1.6% instructions, the
/// `setcar` loops; nbody's `aref` loop +-0).
pub(crate) fn opcode_site_is_leaf_call(op: &Op, aot: bool) -> bool {
    use crate::emacs_core::subr::leaf::LeafId;
    let leaf = match op {
        Op::Memq => Some(LeafId::Memq),
        Op::Assq => Some(LeafId::Assq),
        _ => super::leaf_abi::opcode_leaf(op),
    };
    leaf.is_some()
        && !opcode_site_effects(op, aot).intersects(
            Effects::MAY_GC
                .with(Effects::MAY_REENTER)
                .with(Effects::MAY_DEOPT),
        )
}

/// Select the same named-builtin specialization for baseline JIT, MIR and
/// AOT. These bytecodes name the static builtin table and bypass function-cell
/// advice/redefinition. Every generated fast path rechecks the live entry;
/// a changed entry takes the generic fallback.
///
/// Audited state reads use Tier A. Other Rust builtins use Tier B's direct
/// dispatch, with the exact argument slice and ordinary arity checks. VM-owned
/// special names and writeback operations keep their general dispatch. No
/// obarray or per-site epoch slot is required.
pub(super) fn cbsym_spec_kind(sym: SymId, _nargs: usize) -> Option<SpecCalleeKind> {
    let entry = lookup_global_subr_entry(sym)?;
    if entry.dispatch_kind != SubrDispatchKind::Builtin {
        return None;
    }
    let name = resolve_sym(sym);
    // These operations need the VM-owned dispatch or writeback protocol.
    if CBSYM_SPECIAL_NAMES.contains(&name)
        || matches!(name, "aset" | "fillarray" | "funcall" | "apply" | "eval")
    {
        return None;
    }
    // Tier-A: provably-trivial GC-free reads (COMMIT 5 shims). char-after is
    // Tier-A only in its 0-arg form; a 1-arg / marker call bounces to Tier-B's
    // generic path via the None fall-through here.
    let tier_a = match name {
        "point" => Some(CBSYM_A_POINT),
        "point-min" => Some(CBSYM_A_POINT_MIN),
        "point-max" => Some(CBSYM_A_POINT_MAX),
        "bolp" => Some(CBSYM_A_BOLP),
        "eolp" => Some(CBSYM_A_EOLP),
        "bobp" => Some(CBSYM_A_BOBP),
        "eobp" => Some(CBSYM_A_EOBP),
        "following-char" => Some(CBSYM_A_FOLLOWING_CHAR),
        "preceding-char" => Some(CBSYM_A_PRECEDING_CHAR),
        "char-after" if _nargs == 0 => Some(CBSYM_A_CHAR_AFTER),
        "current-buffer" => Some(CBSYM_A_CURRENT_BUFFER),
        "match-beginning" => Some(CBSYM_A_MATCH_BEGINNING),
        "match-end" => Some(CBSYM_A_MATCH_END),
        _ => None,
    };
    if let Some(which) = tier_a {
        return Some(SpecCalleeKind::CbsymTierA { which });
    }
    // Tier-B: every other plain builtin reaches its primitive through
    // `neovm_jit_cbsym_spec`, which dispatches it directly like the
    // interpreter's `Op::CallBuiltinSym` arm (GNU inline-opcode semantics: no
    // funcall, no backtrace frame). An allowlist used to gate this while the
    // shim still went through `funcall_general`; the direct dispatch is the
    // same code for every builtin, so the only exclusions left are the
    // specials above and entries without a Rust function pointer.
    entry.function.map(|_| SpecCalleeKind::CbsymTierB)
}

#[cfg(test)]
#[path = "../tests/mir_leaf_effects.rs"]
mod mir_leaf_effects_tests;

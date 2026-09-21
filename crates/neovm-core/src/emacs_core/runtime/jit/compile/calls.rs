//! Call contracts shared by baseline JIT, typed MIR and AOT lowering.
//!
//! A specialization describes its guarded fast path separately from the whole
//! call. A failed guard can run arbitrary Lisp through the generic fallback;
//! callers must never use fast-path effects to discard that path's GC roots or
//! to move state reads across it.

use super::*;

/// Independent effects, rather than a strongest-effect ordering. Allocating
/// does not imply collecting, and signaling does not subsume state mutation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effects(u16);

impl Effects {
    pub const PURE: Self = Self(0);
    pub const READ_HEAP: Self = Self(1 << 0);
    pub const WRITE_HEAP: Self = Self(1 << 1);
    pub const READ_BUFFER: Self = Self(1 << 2);
    pub const WRITE_BUFFER: Self = Self(1 << 3);
    pub const READ_MATCH: Self = Self(1 << 4);
    pub const WRITE_MATCH: Self = Self(1 << 5);
    pub const READ_BINDINGS: Self = Self(1 << 6);
    pub const WRITE_BINDINGS: Self = Self(1 << 7);
    pub const ALLOCATES: Self = Self(1 << 8);
    pub const MAY_GC: Self = Self(1 << 9);
    pub const MAY_REENTER: Self = Self(1 << 10);
    pub const MAY_SIGNAL: Self = Self(1 << 11);
    pub const MAY_DEOPT: Self = Self(1 << 12);
    pub const UNKNOWN: Self = Self((1 << 13) - 1);

    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// A read-only fast path still observes mutable runtime state. This is an
    /// admission fact, never permission to hoist it or elide fallback roots.
    pub const fn is_read_only(self) -> bool {
        let reads =
            Self::READ_HEAP.0 | Self::READ_BUFFER.0 | Self::READ_MATCH.0 | Self::READ_BINDINGS.0;
        self.0 & !reads == 0
    }
}

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

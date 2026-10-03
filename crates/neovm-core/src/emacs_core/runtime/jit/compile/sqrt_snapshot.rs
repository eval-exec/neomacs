//! Front-owned immutable actual sqrt witness capture for selected sink recipes.
//! Capture an actual plain A1 sqrt Subr on the source-owning front. Workers
//! receive only immutable compiler scalars, never a Subr reference or Value.

use super::*;

/// A one-call immutable witness. The original source owns its constant callee
/// symbol; static Subr objects are leaked by the existing runtime. This table
/// is compiler-local, contains opaque comparison bits only, and is Send. No
/// concurrent mutator reads a cached Subr entry from it: generated code checks
/// the current Context function epoch, current function cell, actual Subr A1
/// entry and call-entry controls on every call. Equal epochs are not ownership.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SqrtCallWitness {
    pub(super) pc: usize,
    pub(super) symbol_bits: u64,
    pub(super) expected_subr_bits: u64,
    pub(super) epoch: u64,
}

pub(super) type SqrtCallWitnesses = HashMap<usize, SqrtCallWitness>;

/// Invoke only on the owning FRONT, after final fusion/site selection. Unlike
/// classification by spelling, this checks the actual current function entry.
/// An alias to the exact builtin is eligible; a symbol called sqrt bound to
/// another builtin, Lisp function, advice or context callable is not.
pub(super) fn capture(
    ops: &[Op],
    sites: &HashMap<usize, SpecSite>,
    obarray: &Obarray,
) -> SqrtCallWitnesses {
    if jit_opt_mode() != OptMode::Opt || !jit_opt_passes().sink || jit_force_slow_spec() {
        return HashMap::new();
    }
    let epoch = obarray.function_epoch();
    let mut answer = HashMap::new();
    for (&pc, site) in sites {
        if ops.get(pc) != Some(&Op::Call(1))
            || site.kind != SpecCalleeKind::SubrGeneral
            || !call_site_inlinable_at(pc)
        {
            continue;
        }
        let sym = crate::emacs_core::intern::SymId(site.sym);
        let Some(binding) = obarray.symbol_function_id(sym) else {
            continue;
        };
        if binding.bits() as u64 != site.expected_bits {
            continue;
        }
        // Front/runtime typed helper avoids profiling and optional collection
        // observation hooks; no worker may inspect a Subr object.
        if !super::sqrt_binding::is_actual_sqrt(binding) {
            continue;
        }
        answer.insert(
            pc,
            SqrtCallWitness {
                pc,
                symbol_bits: Value::from_sym_id(sym).bits() as u64,
                expected_subr_bits: binding.bits() as u64,
                epoch,
            },
        );
    }
    // Existing mutator discipline makes this stable during front construction;
    // reject a torn capture defensively rather than certify two epochs.
    if obarray.function_epoch() != epoch {
        answer.clear();
    }
    answer
}

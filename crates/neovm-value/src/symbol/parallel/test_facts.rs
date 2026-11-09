//! Copied scalar snapshots for the re-homed symbol-intern tests.
//! The private World facts stay crate-private. These copies carry no roots.

use super::PlainSymbolBacking;
use crate::intern::SymId;
use crate::tagged::gc::HeapIdentity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct InitialInternFacts {
    pub members: usize,
    pub epoch: u64,
    pub slots: usize,
    pub chunks: usize,
    pub sides: usize,
    pub bytes: usize,
    pub spine_current: bool,
    pub spine_address: usize,
    pub present: bool,
    pub member: bool,
    pub cells: Option<(usize, usize, bool, bool)>,
    pub backing: Option<PlainSymbolBacking>,
    pub projections: usize,
    pub projection: Option<(usize, usize, usize)>,
    pub shared_void: Option<(usize, usize)>,
    pub projection_bytes: usize,
}

impl InitialInternFacts {
    pub(crate) fn from_private(facts: super::intern::InitialInternFacts) -> Self {
        Self {
            members: facts.members,
            epoch: facts.epoch,
            slots: facts.slots,
            chunks: facts.chunks,
            sides: facts.sides,
            bytes: facts.bytes,
            spine_current: facts.spine_current,
            spine_address: facts.spine_address,
            present: facts.present,
            member: facts.member,
            cells: facts.cells,
            backing: facts.backing,
            projections: facts.projections,
            projection: facts.projection,
            shared_void: facts.shared_void,
            projection_bytes: facts.projection_bytes,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[doc(hidden)]
pub enum InitialInternFault {
    ReservationAfterSpineMove = 1,
    AfterSlotPublication = 2,
    InitialTablePreparation = 3,
}

impl InitialInternFault {
    pub(crate) fn to_private(self) -> super::intern::InitialInternFault {
        match self {
            Self::ReservationAfterSpineMove => {
                super::intern::InitialInternFault::ReservationAfterSpineMove
            }
            Self::AfterSlotPublication => super::intern::InitialInternFault::AfterSlotPublication,
            Self::InitialTablePreparation => {
                super::intern::InitialInternFault::InitialTablePreparation
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum TableCommitFault {
    AfterDescriptorPublication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct FreshRegistryFacts {
    pub epoch: u64,
    pub symbols: usize,
    pub atoms: usize,
    pub atom_names: usize,
    pub canonical_names: usize,
    pub name_values: usize,
    pub heap_buckets: usize,
    pub heap_names: usize,
    pub input_rooted: bool,
    pub id_present: bool,
    pub canonical_id: Option<SymId>,
    pub atom_present: bool,
    pub bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct SymbolTableCensus {
    pub address: usize,
    pub slots: usize,
    pub chunk_slots: usize,
    pub chunk0: Option<usize>,
    pub generation: u64,
    pub retired: usize,
    pub retained_bytes: usize,
}

impl SymbolTableCensus {
    #[doc(hidden)]
    pub fn generation_successor(self) -> Option<u64> {
        self.generation.checked_add(1)
    }
}

/// Which ledger limit the re-homed refusal tests install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum TableLimitProbe {
    Bytes,
    Retirees,
    Production,
}

#[doc(hidden)]
pub fn fresh_registry_facts_for_test(
    heap: HeapIdentity,
    id: SymId,
    input: usize,
    spelling: &str,
) -> FreshRegistryFacts {
    let facts = crate::intern::parallel::fresh_registry_facts_for_test(heap, id, input, spelling);
    FreshRegistryFacts {
        epoch: facts.epoch,
        symbols: facts.symbols,
        atoms: facts.atoms,
        atom_names: facts.atom_names,
        canonical_names: facts.canonical_names,
        name_values: facts.name_values,
        heap_buckets: facts.heap_buckets,
        heap_names: facts.heap_names,
        input_rooted: facts.input_rooted,
        id_present: facts.id_present,
        canonical_id: facts.canonical_id,
        atom_present: facts.atom_present,
        bytes: facts.bytes,
    }
}

#[doc(hidden)]
pub fn with_symbol_registry_writer_for_test(body: impl FnOnce()) {
    crate::intern::parallel::with_registry_writer_for_test(body);
}

/// Prepare a C6 name capability and return its symbol handle. The capability
/// value stays in value; the handle is the same key the capability stores.
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn prepare_name_symbol_for_test<K: crate::tagged::gc::mutator_registration::EraKeeper>(
    era: &crate::tagged::gc::ProofEra<'_, K>,
    key: &crate::tagged::gc::mutator_registration::SymbolHandle,
) -> Result<
    crate::tagged::gc::mutator_registration::SymbolHandle,
    crate::tagged::gc::mutator_registration::FunctionError,
> {
    crate::tagged::gc::mutator_registration::name_ops::prepare_name_symbol(era, key)
}

/// `RunAccess::sym_name_fset_into` for a handle previously returned by
/// [`prepare_name_symbol_for_test`].
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn sym_name_fset_into_for_test<'era>(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, 'era>,
    symbol: crate::tagged::gc::mutator_registration::SymbolHandle,
    src: &crate::tagged::gc::mutator_registration::OwnerRootSlot<'era>,
    dst: &mut crate::tagged::gc::mutator_registration::OwnerRootSlot<'era>,
) -> Result<(), crate::tagged::gc::mutator_registration::FunctionError> {
    crate::tagged::gc::mutator_registration::name_ops::sym_name_fset_into(run, symbol, src, dst)
}

/// `RunAccess::sym_name_call_begin` for a handle previously returned by
/// [`prepare_name_symbol_for_test`].
#[cfg(not(feature = "gc-memory-telemetry"))]
#[doc(hidden)]
pub fn sym_name_call_begin_for_test<'era>(
    run: &mut crate::tagged::gc::mutator_registration::RunAccess<'_, 'era>,
    symbol: crate::tagged::gc::mutator_registration::SymbolHandle,
    cache: &mut crate::tagged::gc::mutator_registration::FunctionCallCache<'era>,
    activation: &mut crate::tagged::gc::mutator_registration::FunctionActivation<'era>,
) -> Result<(), crate::tagged::gc::mutator_registration::FunctionError> {
    crate::tagged::gc::mutator_registration::name_ops::sym_name_call_begin(
        run, symbol, cache, activation,
    )
}

/// Handoff resume with the control gate uncontended. The caller supplies the
/// era from eval's symbol-backing fence. Assertions stay in the value body.
#[cfg(all(not(feature = "gc-memory-telemetry"), not(miri)))]
#[doc(hidden)]
pub fn handoff_resume_uncontended_for_test<
    K: crate::tagged::gc::mutator_registration::EraKeeper,
>(
    era: crate::tagged::gc::ProofEra<'_, K>,
) {
    crate::tagged::gc::mutator_registration::restricted_lisp::globals::handoff::worker::contention_tests::handoff_resume_uncontended(era);
}

/// Handoff resume when the control gate is held past the yield budget.
/// The caller supplies the era from eval's symbol-backing fence.
#[cfg(all(not(feature = "gc-memory-telemetry"), not(miri)))]
#[doc(hidden)]
pub fn handoff_resume_outlasts_yield_budget_for_test<
    K: crate::tagged::gc::mutator_registration::EraKeeper,
>(
    era: crate::tagged::gc::ProofEra<'_, K>,
) {
    crate::tagged::gc::mutator_registration::restricted_lisp::globals::handoff::worker::contention_tests::handoff_resume_outlasts_yield_budget(era);
}

//! Copied scalar snapshots for the re-homed symbol-intern tests.
//! The private World facts stay crate-private. These copies carry no roots.

use super::PlainSymbolBacking;
use crate::intern::SymId;
use crate::tagged::gc::HeapIdentity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct InitialInternFacts {
    pub(crate) members: usize,
    pub(crate) epoch: u64,
    pub(crate) slots: usize,
    pub(crate) chunks: usize,
    pub(crate) sides: usize,
    pub(crate) bytes: usize,
    pub(crate) spine_current: bool,
    pub(crate) spine_address: usize,
    pub(crate) present: bool,
    pub(crate) member: bool,
    pub(crate) cells: Option<(usize, usize, bool, bool)>,
    pub(crate) backing: Option<PlainSymbolBacking>,
    pub(crate) projections: usize,
    pub(crate) projection: Option<(usize, usize, usize)>,
    pub(crate) shared_void: Option<(usize, usize)>,
    pub(crate) projection_bytes: usize,
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

impl InitialInternFacts {
    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn members(self) -> usize {
        self.members
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn epoch(self) -> u64 {
        self.epoch
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn slots(self) -> usize {
        self.slots
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn chunks(self) -> usize {
        self.chunks
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn sides(self) -> usize {
        self.sides
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn bytes(self) -> usize {
        self.bytes
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn spine_current(self) -> bool {
        self.spine_current
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn spine_address(self) -> usize {
        self.spine_address
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn present(self) -> bool {
        self.present
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn member(self) -> bool {
        self.member
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn cells(self) -> Option<(usize, usize, bool, bool)> {
        self.cells
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn backing(self) -> Option<PlainSymbolBacking> {
        self.backing
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn projections(self) -> usize {
        self.projections
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn projection(self) -> Option<(usize, usize, usize)> {
        self.projection
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn shared_void(self) -> Option<(usize, usize)> {
        self.shared_void
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn projection_bytes(self) -> usize {
        self.projection_bytes
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
    pub(crate) epoch: u64,
    pub(crate) symbols: usize,
    pub(crate) atoms: usize,
    pub(crate) atom_names: usize,
    pub(crate) canonical_names: usize,
    pub(crate) name_values: usize,
    pub(crate) heap_buckets: usize,
    pub(crate) heap_names: usize,
    pub(crate) input_rooted: bool,
    pub(crate) id_present: bool,
    pub(crate) canonical_id: Option<SymId>,
    pub(crate) atom_present: bool,
    pub(crate) bytes: usize,
}

impl FreshRegistryFacts {
    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn epoch(self) -> u64 {
        self.epoch
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn symbols(self) -> usize {
        self.symbols
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn atoms(self) -> usize {
        self.atoms
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn atom_names(self) -> usize {
        self.atom_names
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn canonical_names(self) -> usize {
        self.canonical_names
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn name_values(self) -> usize {
        self.name_values
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn heap_buckets(self) -> usize {
        self.heap_buckets
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn heap_names(self) -> usize {
        self.heap_names
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn input_rooted(self) -> bool {
        self.input_rooted
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn id_present(self) -> bool {
        self.id_present
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn canonical_id(self) -> Option<SymId> {
        self.canonical_id
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn atom_present(self) -> bool {
        self.atom_present
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn bytes(self) -> usize {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct SymbolTableCensus {
    pub(crate) address: usize,
    pub(crate) slots: usize,
    pub(crate) chunk_slots: usize,
    pub(crate) chunk0: Option<usize>,
    pub(crate) generation: u64,
    pub(crate) retired: usize,
    pub(crate) retained_bytes: usize,
}

impl SymbolTableCensus {
    #[doc(hidden)]
    pub fn generation_successor(self) -> Option<u64> {
        self.generation.checked_add(1)
    }
}

impl SymbolTableCensus {
    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn address(self) -> usize {
        self.address
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn slots(self) -> usize {
        self.slots
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn chunk_slots(self) -> usize {
        self.chunk_slots
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn chunk0(self) -> Option<usize> {
        self.chunk0
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn generation(self) -> u64 {
        self.generation
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn retired(self) -> usize {
        self.retired
    }

    #[cfg(feature = "test-support")]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[doc(hidden)]
    pub(crate) fn retained_bytes(self) -> usize {
        self.retained_bytes
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

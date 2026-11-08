//! Type-checking dependencies for the real scan-contract module.
//!
//! Collector bodies are never exercised. Keep the heap Send + Sync so the
//! confinement fixtures reject the real capability's marker, not a stub field.
#![allow(dead_code)]

use std::marker::PhantomData;

#[path = "../../../../neovm-value/src/tagged/gc/scan_contract.rs"]
pub mod scan_contract;

#[derive(Default)]
pub struct TaggedHeap {
    satb_handoff: SatbHandoff,
    gray_queue: Vec<usize>,
    _satb_layout_reserve: YoungRegistry,
    cons_allocator: cons_allocator::ConsAllocatorOwner,
    cons_blocks: Vec<ConsBlock>,
}

impl TaggedHeap {
    pub fn tagged_heap_identity(&self) -> usize {
        1
    }

    /// The owner-alias fixture takes an immutable borrow through the
    /// historical spelling to pin its E0502 golden; keep it as an alias so
    /// that fixture and its golden stay unchanged.
    pub fn identity(&self) -> usize {
        1
    }

    fn concurrent_mark_running(&self) -> bool {
        unimplemented!()
    }

    fn record_termination_partial_words(
        &mut self,
        _: usize,
        _: &scan_contract::StoppedMutatorsProof,
    ) {
        unimplemented!()
    }

    fn mutator_gc_entries_world_stopped(
        &mut self,
        _: &scan_contract::StoppedMutatorsProof,
    ) -> std::iter::Empty<StoppedMutator<'_>> {
        unimplemented!()
    }

    fn mutator_satb_states(&self) -> std::iter::Empty<&MutatorSatb> {
        unimplemented!()
    }
}

struct StoppedMutator<'a> {
    gc: &'a mut (),
    satb: &'a mut MutatorSatb,
}

struct MutatorSatb {
    satb: SatbBuffer,
    satb_snapshotted_owners: rustc_hash::FxHashSet<usize>,
}

impl MutatorSatb {
    fn begin_cycle(
        &mut self,
        _: &scan_contract::StoppedMutatorsProof,
    ) -> Result<(), satb::SatbPoisoned> {
        unimplemented!()
    }
}

struct SatbBuffer;
impl SatbBuffer {
    fn take_partial(&mut self, _: &scan_contract::StoppedMutatorsProof) -> Option<Chunk> {
        unimplemented!()
    }

    fn is_empty(&self) -> bool {
        unimplemented!()
    }
}

#[derive(Default)]
struct SatbHandoff;
impl SatbHandoff {
    fn take_all(&self) -> Result<Vec<Chunk>, satb::SatbPoisoned> {
        unimplemented!()
    }

    fn is_empty(&self) -> Result<bool, satb::SatbPoisoned> {
        unimplemented!()
    }
}

struct Chunk;
impl Chunk {
    fn len(&self) -> usize {
        unimplemented!()
    }

    fn into_words(self) -> Vec<MarkWord> {
        unimplemented!()
    }
}

struct MarkWord;
impl MarkWord {
    fn value(self) -> usize {
        unimplemented!()
    }
}

pub struct AllStopped<'a>(PhantomData<&'a ()>);
pub struct ConsBlock;

pub mod satb {
    #[derive(Debug, thiserror::Error)]
    #[error("fixture SATB poison")]
    pub struct SatbPoisoned;
}

pub mod cons_allocator {
    #[derive(Default)]
    pub struct ConsAllocatorOwner;
}

#[derive(Default)]
struct YoungRegistry;
impl YoungRegistry {
    fn ensure_young_logs_stopped(
        &self,
        _: &scan_contract::StoppedMutatorsProof,
    ) -> Result<(), young_registry::YoungRegistryError> {
        unimplemented!()
    }

    fn take_young_loan(
        &mut self,
        _: young_registry::YoungRegistrationId,
        _: &scan_contract::StoppedMutatorsProof,
    ) -> Result<young_registry::YoungLogLoan, young_registry::YoungRegistryError> {
        unimplemented!()
    }

    fn return_young_loan(
        &mut self,
        _: young_registry::YoungLogLoan,
        _: &scan_contract::StoppedMutatorsProof,
    ) -> Result<(), young_registry::YoungLogReturnError> {
        unimplemented!()
    }

    fn current_young_id(&self) -> young_registry::YoungRegistrationId {
        unimplemented!()
    }
}

pub mod young_registry {
    use super::{YoungRegistry, scan_contract::StoppedMutatorsProof};
    use std::marker::PhantomData;

    pub struct YoungRegistryError;
    pub struct YoungLogReturnError;
    pub struct YoungRegistrationId;
    pub struct YoungLogLoan;
    pub struct StoppedYoungLogs<'a>(PhantomData<&'a mut YoungRegistry>);

    impl<'a> StoppedYoungLogs<'a> {
        pub(super) fn new(
            _: &'a mut YoungRegistry,
            _: &StoppedMutatorsProof,
        ) -> Result<Self, YoungRegistryError> {
            unimplemented!()
        }
    }
}

//! GC state written only by the mutator.
//!
//! The current runtime has one mutator. Keeping its allocation accounting and
//! barrier state in one inline value gives collector consumers the same
//! accessor surface a future mutator registry will expose, without adding a
//! pointer indirection to allocation or a new thread-local cache.

use super::*;

#[path = "root_seed_scratch.rs"]
mod root_seed_scratch;

pub(super) struct MutatorGcState {
    /// Charged objects, including the unused tail of an open P0.8 region.
    pub(super) allocated_count: usize,
    pub(super) memory_use_counts: [u64; MEMORY_USE_COUNT_LEN],
    pub(super) bytes_since_gc: usize,
    /// Bytes charged before the most recent accounting reset.
    pub(super) bytes_banked_at_resets: u64,
    /// Ordinary old owners recorded since the previous collection began.
    pub(super) remset: Vec<TaggedValue>,
    /// Mapped owners whose children must be scanned in the next cycle.
    pub(super) r_mapped_seen: FxHashSet<usize>,
    /// Non-cons objects born black during a concurrent major (C2.6).
    pub(super) black_born: Vec<*mut GcHeader>,
    pub(super) black_born_regions: Vec<birth_logs::BlackBornRegion>,
    pub(super) black_cons_region_start: Option<usize>,
    pub(super) black_float_region_start: Option<usize>,
    /// Symbol side-table preimages and inserted symbols belong to this mutator.
    pub(super) major_symbol_preimages: Vec<SymId>,
    /// Cons owners written during a concurrent major. Their current children
    /// are traced once per owner after the stopped-world join; old insertions
    /// are not necessarily allocate-black births. No free precedes this drain.
    pub(super) major_cons_writes: Vec<TaggedValue>,
    /// Direct-mapped repeat-owner reject, cleared at every cycle begin.
    pub(super) remembered_cache: [usize; BARRIER_CACHE_SLOTS],
    /// Generation pacing obligations owned by this mutator, summed at poll.
    pub(super) pacing: pacing::GenerationPacingCounters,
}

/// Unused region capacity, derived from the cursors shared with compiled code.
/// Deriving this at a read keeps exact accounting out of the allocation path.
#[derive(Clone, Copy)]
pub(super) struct AllocationRegionDeltas {
    pub(super) conses: usize,
    pub(super) floats: usize,
}

impl AllocationRegionDeltas {
    #[inline]
    fn objects(self) -> usize {
        self.conses + self.floats
    }

    #[inline]
    fn bytes(self) -> usize {
        self.conses * size_of::<ConsCell>() + self.floats * size_of::<FloatObj>()
    }
}

impl MutatorGcState {
    pub(super) fn new() -> Self {
        Self {
            allocated_count: 0,
            memory_use_counts: [0; MEMORY_USE_COUNT_LEN],
            bytes_since_gc: 0,
            bytes_banked_at_resets: 0,
            remset: Vec::new(),
            r_mapped_seen: FxHashSet::default(),
            black_born: Vec::new(),
            black_born_regions: Vec::new(),
            black_cons_region_start: None,
            black_float_region_start: None,
            major_symbol_preimages: Vec::new(),
            major_cons_writes: Vec::new(),
            remembered_cache: [0; BARRIER_CACHE_SLOTS],
            pacing: pacing::GenerationPacingCounters::default(),
        }
    }

    #[inline]
    pub(super) fn exact_allocated_count(&self, delta: AllocationRegionDeltas) -> usize {
        self.allocated_count - delta.objects()
    }

    #[inline]
    pub(super) fn exact_bytes_since_gc(&self, delta: AllocationRegionDeltas) -> usize {
        self.bytes_since_gc - delta.bytes()
    }

    #[inline]
    pub(super) fn exact_memory_use_counts(
        &self,
        delta: AllocationRegionDeltas,
    ) -> [u64; MEMORY_USE_COUNT_LEN] {
        let mut counts = self.memory_use_counts;
        let conses = MemoryUseCountSlot::ConsCells.index();
        counts[conses] = counts[conses].wrapping_sub(delta.conses as u64);
        let floats = MemoryUseCountSlot::Floats.index();
        counts[floats] = counts[floats].wrapping_sub(delta.floats as u64);
        counts
    }
}

impl TaggedHeap {
    #[inline]
    pub(super) fn current_mutator_gc(&self) -> &MutatorGcState {
        &self.mutator_gc
    }

    #[inline]
    pub(super) fn current_mutator_gc_mut(&mut self) -> &mut MutatorGcState {
        &mut self.mutator_gc
    }

    #[inline]
    pub(super) fn mutators(&self) -> impl Iterator<Item = &MutatorGcState> {
        #[cfg(not(test))]
        {
            std::iter::once(self.current_mutator_gc())
        }
        #[cfg(test)]
        {
            std::iter::once(&self.mutator_gc).chain(
                self._satb_layout_reserve
                    .0
                    .registered
                    .iter()
                    .map(|entry| &entry.gc),
            )
        }
    }

    #[inline]
    pub(super) fn mutators_mut(&mut self) -> impl Iterator<Item = &mut MutatorGcState> {
        #[cfg(not(test))]
        {
            std::iter::once(self.current_mutator_gc_mut())
        }
        #[cfg(test)]
        {
            std::iter::once(&mut self.mutator_gc).chain(
                self._satb_layout_reserve
                    .0
                    .registered
                    .iter_mut()
                    .map(|entry| &mut entry.gc),
            )
        }
    }

    #[inline]
    pub(super) fn allocation_region_deltas(&self) -> AllocationRegionDeltas {
        AllocationRegionDeltas {
            conses: self.open_cons_unused(),
            floats: self.open_float_unused(),
        }
    }
}

// U35's cold per-mutator accessors share this accessor family. Hot allocation
// and barrier callers keep the original inline MutatorGcState above.
impl TaggedHeap {
    pub(super) fn current_concurrent_hash_mutator(
        &self,
    ) -> std::sync::Arc<std::sync::Mutex<cold_gc::ConcurrentHashMutatorState>> {
        self.concurrent_claims_state()
            .expect("claims enabled")
            .current_mutator()
    }

    /// The caller has stopped all registered mutators. Copy Arc handles under
    /// the map lock, then release it before locking any log/descriptor entry.
    pub(super) fn concurrent_hash_mutators(
        &self,
    ) -> impl Iterator<Item = std::sync::Arc<std::sync::Mutex<cold_gc::ConcurrentHashMutatorState>>>
    {
        self.concurrent_claims_state()
            .map(|state| state.mutators_world_stopped())
            .unwrap_or_default()
            .into_iter()
    }
}

use super::satb::{SatbBuffer, SatbHandoff, SatbPoisoned, SatbRetired};
use super::scan_contract::{SatbDrainError, StoppedMutatorsProof};
use super::young_registry::{
    YoungBlockLog, YoungLogLoan, YoungLogReturnError, YoungLogSlot, YoungRegistrationId,
    YoungRegistrationLocation, YoungRegistryControl, YoungRegistryError,
};

#[derive(Debug, thiserror::Error)]
pub(super) enum MutatorRetireError {
    #[error(transparent)]
    Young(#[from] YoungRegistryError),
    #[error(transparent)]
    Satb(#[from] SatbPoisoned),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum SatbMutatorLifecycle {
    #[default]
    Active,
    Retired,
}

/// Owner-confined cold half of the same logical MutatorGcState. The existing
/// inline hot half stays exactly 800 bytes; no inactive-mark path follows this
/// sidecar pointer. A retired half remains registered until stopped termination
/// has folded its partial and consumed its dirty-owner insertion repair log.
#[derive(Debug)]
pub(super) struct SatbMutatorState {
    pub(super) satb: SatbBuffer,
    pub(super) satb_snapshotted_owners: FxHashSet<usize>,
    pub(super) satb_string_preimage_addrs: FxHashSet<usize>,
    lifecycle: SatbMutatorLifecycle,
    /// Same cold registration and retire path as SATB; never a ThreadId map.
    pub(super) young: YoungLogSlot,
}

static_assertions::assert_not_impl_any!(SatbMutatorState: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(SatbMutatorState: std::fmt::Debug);

impl SatbMutatorState {
    pub(super) fn new(young: YoungLogSlot) -> Self {
        Self {
            satb: SatbBuffer::new(),
            satb_snapshotted_owners: FxHashSet::default(),
            satb_string_preimage_addrs: FxHashSet::default(),
            lifecycle: SatbMutatorLifecycle::Active,
            young,
        }
    }

    #[must_use = "retirement must succeed before removing the registration"]
    pub(super) fn retire(
        &mut self,
        handoff: &SatbHandoff,
    ) -> Result<SatbRetired, MutatorRetireError> {
        // Reject before taking/publishing SATB or changing lifecycle. A worker
        // must return its young payload before its canonical owner can retire.
        self.young.resident()?;
        let retired = std::mem::take(&mut self.satb).retire(handoff);
        self.lifecycle = SatbMutatorLifecycle::Retired;
        // Preserve SATB's existing poison/lifecycle semantics after preflight.
        // The retained retired entry still owns its young log and dirty-owner
        // dedup sets until the stopped-world consumer has drained them.
        retired.map_err(MutatorRetireError::from)
    }

    /// Residual words are an error, never a reason to clear the log at begin.
    pub(super) fn begin_cycle(&mut self, _: &StoppedMutatorsProof) -> Result<(), SatbDrainError> {
        if !self.satb.is_empty() {
            return Err(SatbDrainError::Residual);
        }
        self.satb_snapshotted_owners.clear();
        self.satb_string_preimage_addrs.clear();
        Ok(())
    }
}

/// One registered test owner includes a real hot MutatorGcState, not merely a
/// transport buffer. The ordinary iterator family and the stopped SATB fold
/// both enumerate this exact same set of registrations.
#[cfg(test)]
struct RegisteredMutatorGc {
    gc: MutatorGcState,
    satb: SatbMutatorState,
}

/// Cold storage owned by the existing inline mutator's pointer-sized reserve.
/// This is the legacy one-mutator representation, not a parallel registry:
/// a real future registry must own each hot/cold pair as one registration.
struct MutatorSatbRegistry {
    root_seed_scratch: root_seed_scratch::RootSeedScratch,
    current: SatbMutatorState,
    /// Canonical ID issuance, owner permutation index, and O(1) loan preflight.
    /// Payloads reside only in the hot/cold registrations above and below.
    young_control: YoungRegistryControl,
    /// Owner-resident SATB words folded by the last successful termination.
    /// Keep diagnostics in this cold allocation so heap/JIT offsets do not move.
    last_termination_partial_words: usize,
    #[cfg(test)]
    registered: Vec<RegisteredMutatorGc>,
    /// Scoped pointer to the actual AllStopped guard; never a manufactured proof.
    /// Access and lifetime erasure are confined to collector_tables.
    collector_tables: Option<std::rc::Rc<Cell<*const ()>>>,
    /// The World's carried collection protocol, outside the pinned hot heap.
    observations: std::cell::OnceCell<super::super::collection_reads::ObservationWorld>,
}

#[repr(transparent)]
pub(super) struct MutatorSatbSlot(Box<MutatorSatbRegistry>);

impl std::fmt::Debug for MutatorSatbSlot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutatorSatbSlot")
            .field("current", &self.0.current)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(MutatorSatbSlot: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(MutatorSatbSlot: std::fmt::Debug);
const _: () = {
    assert!(std::mem::size_of::<MutatorSatbSlot>() == std::mem::size_of::<std::ptr::NonNull<()>>());
    assert!(
        std::mem::align_of::<MutatorSatbSlot>() == std::mem::align_of::<std::ptr::NonNull<()>>()
    );
    assert!(
        std::mem::size_of::<Option<MutatorSatbSlot>>() == std::mem::size_of::<MutatorSatbSlot>()
    );
};

impl MutatorSatbSlot {
    pub(super) fn collector_tables(&self) -> *const () {
        self.0
            .collector_tables
            .as_ref()
            .map_or(std::ptr::null(), |slot| slot.get())
    }

    pub(super) fn collector_tables_slot(&mut self) -> std::rc::Rc<Cell<*const ()>> {
        std::rc::Rc::clone(
            self.0
                .collector_tables
                .get_or_insert_with(|| std::rc::Rc::new(Cell::new(std::ptr::null()))),
        )
    }

    #[inline]
    pub(super) fn young_heap_identity(&self) -> super::HeapIdentity {
        self.0.young_control.heap_identity()
    }

    /// This all-owner counter belongs to the canonical confined registry.
    /// Check it before any shutdown work and in the nonblocking Drop path.
    #[inline]
    pub(super) fn has_outstanding_young_loans(&self) -> bool {
        self.0.young_control.has_outstanding_loans()
    }

    pub(super) fn new(identity: HeapIdentity) -> Self {
        let (young_control, young) = YoungRegistryControl::new(identity);
        Self(Box::new(MutatorSatbRegistry {
            root_seed_scratch: root_seed_scratch::RootSeedScratch::default(),
            current: SatbMutatorState::new(young),
            young_control,
            last_termination_partial_words: 0,
            #[cfg(test)]
            registered: Vec::new(),
            collector_tables: None,
            observations: std::cell::OnceCell::new(),
        }))
    }

    pub(super) fn observations(&self) -> &super::super::collection_reads::ObservationWorld {
        self.0
            .observations
            .get_or_init(super::super::collection_reads::ObservationWorld::for_heap)
    }

    pub(super) fn existing_observations(
        &self,
    ) -> Option<&super::super::collection_reads::ObservationWorld> {
        self.0.observations.get()
    }

    /// Include this Box, the cfg(test) descriptor/location index allocations,
    /// and resident young-log Vec payloads. SATB buffers/hash payloads remain
    /// accounted separately per owner, without double counting. A worker loan
    /// owns its Vec outside the registry until return; stopped preflight rejects
    /// outstanding loans, so a stopped inventory contains every young payload.
    #[cfg(feature = "gc-memory-telemetry")]
    pub(super) fn allocation_bytes(&self) -> usize {
        let bytes = std::mem::size_of::<MutatorSatbRegistry>()
            + self.0.young_control.allocation_bytes()
            + self.0.current.young.resident_allocation_bytes();
        #[cfg(not(test))]
        {
            bytes
        }
        #[cfg(test)]
        {
            bytes
                + self.0.registered.capacity() * std::mem::size_of::<RegisteredMutatorGc>()
                + self
                    .0
                    .registered
                    .iter()
                    .map(|entry| entry.satb.young.resident_allocation_bytes())
                    .sum::<usize>()
        }
    }

    /// Owner-side access to the current canonical payload. Its unique Rust
    /// borrow excludes a simultaneous young-only worker loan for this owner.
    /// Root refill integration splits this field from ConsAllocatorOwner.
    #[inline]
    pub(super) fn current_young_log_mut(
        &mut self,
    ) -> Result<&mut YoungBlockLog, YoungRegistryError> {
        if self.0.current.lifecycle == SatbMutatorLifecycle::Retired {
            return Err(YoungRegistryError::Retired);
        }
        self.0.current.young.resident_mut()
    }

    #[inline]
    pub(super) fn current_young_id(&self) -> YoungRegistrationId {
        self.0.current.young.id()
    }

    /// Resolves logical IDs through the canonical registry's physical-location
    /// index, including nested test-owner swaps and panic restoration.
    fn young_parts(
        &mut self,
        id: YoungRegistrationId,
    ) -> Result<(&mut YoungRegistryControl, &mut SatbMutatorState), YoungRegistryError> {
        let registry = &mut *self.0;
        let location = registry.young_control.location(id)?;
        let owner = match location {
            YoungRegistrationLocation::Current => &mut registry.current,
            #[cfg(test)]
            YoungRegistrationLocation::Registered(index) => {
                &mut registry
                    .registered
                    .get_mut(index.index())
                    .ok_or(YoungRegistryError::UnknownRegistration)?
                    .satb
            }
        };
        if owner.young.id() != id {
            return Err(YoungRegistryError::UnknownRegistration);
        }
        Ok((&mut registry.young_control, owner))
    }

    pub(super) fn take_young_loan(
        &mut self,
        id: YoungRegistrationId,
        _: &StoppedMutatorsProof,
    ) -> Result<YoungLogLoan, YoungRegistryError> {
        let (control, owner) = self.young_parts(id)?;
        if owner.lifecycle == SatbMutatorLifecycle::Retired {
            return Err(YoungRegistryError::Retired);
        }
        control.take_loan(&mut owner.young)
    }

    #[cfg(test)]
    pub(super) fn retire_young_registration(
        &mut self,
        id: YoungRegistrationId,
        handoff: &SatbHandoff,
        _: &StoppedMutatorsProof,
    ) -> Result<SatbRetired, MutatorRetireError> {
        let (_, owner) = self.young_parts(id)?;
        owner.retire(handoff)
    }

    pub(super) fn return_young_loan(
        &mut self,
        loan: YoungLogLoan,
        _: &StoppedMutatorsProof,
    ) -> Result<(), YoungLogReturnError> {
        match self.young_parts(loan.id()) {
            Ok((control, owner)) => control.return_loan(&mut owner.young, loan),
            Err(reason) => Err(YoungLogReturnError::new(reason, loan)),
        }
    }

    /// O(1), independent of the number of retained retired registrations.
    pub(super) fn ensure_young_logs_stopped(
        &self,
        _: &StoppedMutatorsProof,
    ) -> Result<(), YoungRegistryError> {
        self.0.young_control.ensure_stopped()
    }

    pub(super) fn young_log_world_stopped(
        &mut self,
        id: YoungRegistrationId,
        _: &StoppedMutatorsProof,
    ) -> Result<&mut YoungBlockLog, YoungRegistryError> {
        let (_, owner) = self.young_parts(id)?;
        // Retired owners remain resolvable for capture and empty-block unlink.
        owner.young.resident_mut()
    }

    pub(super) fn young_logs_world_stopped(
        &self,
        _: &StoppedMutatorsProof,
    ) -> impl Iterator<Item = &YoungBlockLog> {
        let current = std::iter::once(&self.0.current);
        #[cfg(not(test))]
        let owners = current;
        #[cfg(test)]
        let owners = current.chain(self.0.registered.iter().map(|entry| &entry.satb));
        owners.map(|owner| {
            owner
                .young
                .resident()
                .expect("stopped young guard preflight requires all payloads resident")
        })
    }

    #[cfg(test)]
    pub(super) fn registered_young_id(
        &self,
        id: TestMutatorGcId,
        _: &StoppedMutatorsProof,
    ) -> Result<YoungRegistrationId, YoungRegistryError> {
        self.0
            .registered
            .get(id.index())
            .map(|entry| entry.satb.young.id())
            .ok_or(YoungRegistryError::UnknownRegistration)
    }

    /// Terminal abandonment only. Heap storage has already been retained;
    /// destructor fallback cannot take locks, wait, or publish any new work.
    pub(super) fn abandon(&mut self) {
        self.0.current.satb.abandon();
        #[cfg(test)]
        for registration in &mut self.0.registered {
            registration.satb.satb.abandon();
        }
    }
}

pub(super) struct StoppedMutatorGc<'h> {
    pub(super) gc: &'h mut MutatorGcState,
    pub(super) satb: &'h mut SatbMutatorState,
}

impl std::fmt::Debug for StoppedMutatorGc<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StoppedMutatorGc")
            .field("bytes_since_gc", &self.gc.bytes_since_gc)
            .field("satb", &self.satb)
            .finish()
    }
}

static_assertions::assert_not_impl_any!(StoppedMutatorGc<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(StoppedMutatorGc<'static>: std::fmt::Debug);

impl TaggedHeap {
    pub(super) fn last_termination_partial_words(&self) -> usize {
        self._satb_layout_reserve.0.last_termination_partial_words
    }

    /// Only the stopped-world termination fold records this diagnostic.
    pub(super) fn record_termination_partial_words(
        &mut self,
        words: usize,
        _: &StoppedMutatorsProof,
    ) {
        self._satb_layout_reserve.0.last_termination_partial_words = words;
    }

    #[inline]
    pub(super) fn current_mutator_satb(&self) -> &SatbMutatorState {
        &self._satb_layout_reserve.0.current
    }

    #[inline]
    pub(super) fn current_mutator_satb_mut(&mut self) -> &mut SatbMutatorState {
        debug_assert_eq!(
            self._satb_layout_reserve.0.current.lifecycle,
            SatbMutatorLifecycle::Active
        );
        &mut self._satb_layout_reserve.0.current
    }

    pub(super) fn satb_parts(&mut self) -> (&mut SatbMutatorState, &SatbHandoff) {
        (&mut self._satb_layout_reserve.0.current, &self.satb_handoff)
    }

    /// Inventory reads the same registrations as the hot iterator family.
    pub(super) fn mutator_satb_states(&self) -> impl Iterator<Item = &SatbMutatorState> {
        let current = std::iter::once(self.current_mutator_satb());
        #[cfg(not(test))]
        {
            current
        }
        #[cfg(test)]
        {
            current.chain(
                self._satb_layout_reserve
                    .0
                    .registered
                    .iter()
                    .map(|entry| &entry.satb),
            )
        }
    }

    /// All noncurrent writes are gated by the closed stopped-world proof.
    pub(super) fn mutator_gc_entries_world_stopped(
        &mut self,
        _: &StoppedMutatorsProof,
    ) -> impl Iterator<Item = StoppedMutatorGc<'_>> {
        let registry = &mut self._satb_layout_reserve.0;
        let current = std::iter::once(StoppedMutatorGc {
            gc: &mut self.mutator_gc,
            satb: &mut registry.current,
        });
        #[cfg(not(test))]
        {
            current
        }
        #[cfg(test)]
        {
            current.chain(
                registry
                    .registered
                    .iter_mut()
                    .map(|entry| StoppedMutatorGc {
                        gc: &mut entry.gc,
                        satb: &mut entry.satb,
                    }),
            )
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct TestMutatorGcId(std::num::NonZeroUsize);

#[cfg(test)]
impl TestMutatorGcId {
    fn for_index(index: usize) -> Self {
        Self(
            index
                .checked_add(1)
                .and_then(std::num::NonZeroUsize::new)
                .expect("test mutator registration ID space exhausted"),
        )
    }

    fn index(self) -> usize {
        self.0.get() - 1
    }
}

#[cfg(test)]
impl TaggedHeap {
    /// Only the capability exposes registration to tests; IDs stay valid
    /// through retirement and final fold, so exited logs cannot disappear.
    pub(super) fn register_mutator_gc_for_test(
        &mut self,
        _: &StoppedMutatorsProof,
    ) -> TestMutatorGcId {
        let registry = &mut *self._satb_layout_reserve.0;
        let id = TestMutatorGcId::for_index(registry.registered.len());
        // Reserve before issuing the canonical ID: allocation failure must not
        // leave a registry location pointing to an entry that was never pushed.
        registry.registered.reserve(1);
        let young = registry
            .young_control
            .register()
            .expect("test mutator registration ID space exhausted");
        registry.registered.push(RegisteredMutatorGc {
            gc: MutatorGcState::new(),
            satb: SatbMutatorState::new(young),
        });
        id
    }

    pub(super) fn registered_mutator_gc_for_test(
        &mut self,
        id: TestMutatorGcId,
        _: &StoppedMutatorsProof,
    ) -> StoppedMutatorGc<'_> {
        let entry = &mut self._satb_layout_reserve.0.registered[id.index()];
        StoppedMutatorGc {
            gc: &mut entry.gc,
            satb: &mut entry.satb,
        }
    }
}

// Pin the actual hot physical layout on every x86-64 compilation, including
// unit tests. A new cold field must never silently migrate into this prefix.
#[cfg(all(target_arch = "x86_64", target_pointer_width = "64"))]
const _: () = {
    use std::mem::{align_of, offset_of, size_of};
    assert!(size_of::<MutatorGcState>() == 800);
    assert!(align_of::<MutatorGcState>() == 8);
    assert!(offset_of!(MutatorGcState, black_cons_region_start) == 0);
    assert!(offset_of!(MutatorGcState, black_float_region_start) == 16);
    assert!(offset_of!(MutatorGcState, remset) == 32);
    assert!(offset_of!(MutatorGcState, black_born) == 56);
    assert!(offset_of!(MutatorGcState, black_born_regions) == 80);
    assert!(offset_of!(MutatorGcState, major_symbol_preimages) == 104);
    assert!(offset_of!(MutatorGcState, major_cons_writes) == 128);
    assert!(offset_of!(MutatorGcState, r_mapped_seen) == 152);
    assert!(offset_of!(MutatorGcState, allocated_count) == 184);
    assert!(offset_of!(MutatorGcState, memory_use_counts) == 192);
    assert!(offset_of!(MutatorGcState, bytes_since_gc) == 248);
    assert!(offset_of!(MutatorGcState, bytes_banked_at_resets) == 256);
    assert!(offset_of!(MutatorGcState, remembered_cache) == 264);
    assert!(offset_of!(MutatorGcState, pacing) == 776);
};

#[cfg(test)]
#[derive(Debug, thiserror::Error)]
pub(super) enum TestMutatorSwitchError {
    #[error("unknown registered test mutator")]
    Unknown,
    #[error("retired test mutator cannot become a writer")]
    Retired,
}

#[cfg(test)]
struct TestMutatorSwitch {
    heap: std::ptr::NonNull<TaggedHeap>,
    id: TestMutatorGcId,
}

#[cfg(test)]
impl TestMutatorSwitch {
    /// Refresh the existing heap's owner mirrors without a fallible TLS access.
    /// Same-heap owner switching changes no compiled barrier-window field.
    fn refresh_tls(heap: &mut TaggedHeap) {
        let _ = TAGGED_HEAP_ID.try_with(|slot| slot.set(Some(heap.identity())));
        let _ =
            TAGGED_HEAP_WRITE_TRACKING_MODE.try_with(|slot| slot.set(heap.write_tracking_mode()));
        let _ = TAGGED_HEAP_PARTITION_ACTIVE.try_with(|slot| slot.set(heap.partition_dump));
        let _ =
            TAGGED_HEAP_CONCURRENT_ACTIVE.try_with(|slot| slot.set(heap.concurrent_mark_running));
        let _ = TAGGED_HEAP_CONCURRENT_HASH_ACTIVE.try_with(|slot| {
            slot.set(
                heap.concurrent_mark_running
                    && heap.concurrent_claims()
                    && heap.concurrent_hash_snapshot().is_some(),
            )
        });
        let _ =
            TAGGED_HEAP_DUMP_SPAN.try_with(|slot| slot.set((heap.dump_addr_lo, heap.dump_addr_hi)));
        let window = heap.barrier_window();
        let _ = TAGGED_HEAP_BARRIER_WINDOW.try_with(|slot| slot.set(window));
        let _ = TAGGED_HEAP_CONS_BARRIER_WINDOW
            .try_with(|slot| slot.set(heap.cons_barrier_window(window)));
        // Synthetic switching stays on this fixture's executing OS thread.
        let owner = heap._satb_layout_reserve.observations();
        if owner.is_detached() {
            unsafe { owner.root().as_ref() }
                .publish_windows(heap.cons_barrier_window(window), window);
        } else {
            unsafe { heap.store.assume_init_ref() }
                .publish_windows(heap.cons_barrier_window(window), window);
        }
        let _ = TAGGED_HEAP_SATB_CACHE.try_with(|slots| {
            for slot in slots {
                slot.set(0);
            }
        });
        heap.current_mutator_gc_mut().remembered_cache.fill(0);
    }
}

#[cfg(test)]
impl Drop for TestMutatorSwitch {
    fn drop(&mut self) {
        // Construction validated the ID; no test API removes registrations.
        // get_mut keeps Drop total even if a future test API violates that
        // assumption. There is no allocation, lock, callback or assertion.
        // SAFETY: the fixture's root owner outlives this guard; the body has
        // returned (or unwound), so no body heap loan remains live.
        let heap = unsafe { self.heap.as_mut() };
        let registry = &mut *heap._satb_layout_reserve.0;
        if let Some(entry) = registry.registered.get_mut(self.id.index()) {
            std::mem::swap(&mut heap.mutator_gc, &mut entry.gc);
            registry
                .young_control
                .swap_locations(registry.current.young.id(), entry.satb.young.id());
            std::mem::swap(&mut registry.current, &mut entry.satb);
        }
        Self::refresh_tls(heap);
    }
}

#[cfg(test)]
impl TaggedHeap {
    /// Run actual barriers with the selected artificial owner's hot/SATB
    /// state. Neither this frame nor the body receives a protected heap loan.
    /// Same-heap switching refreshes mirrors, preserving the TLS root pointer.
    ///
    /// # Safety
    /// `root` is the live allocation's ONE provenance root, installed in TLS.
    /// The caller owns every artificial mutator, and no heap loan is live.
    /// The body may only write preallocated objects/logs: no Lisp allocation,
    /// GC, registry membership changes, allocation regions, or escaping loans.
    pub(super) unsafe fn with_registered_mutator_gc_for_test<R>(
        mut root: std::ptr::NonNull<TaggedHeap>,
        id: TestMutatorGcId,
        _: &StoppedMutatorsProof,
        body: impl FnOnce(std::ptr::NonNull<TaggedHeap>) -> R,
    ) -> Result<R, TestMutatorSwitchError> {
        assert!(TAGGED_HEAP.with(|slot| slot.get() == root.as_ptr()));
        {
            // SAFETY: sole fixture owner; this temporary loan ends before body.
            let heap = unsafe { root.as_mut() };
            let registry = &mut *heap._satb_layout_reserve.0;
            let entry = registry
                .registered
                .get_mut(id.index())
                .ok_or(TestMutatorSwitchError::Unknown)?;
            if entry.satb.lifecycle == SatbMutatorLifecycle::Retired {
                return Err(TestMutatorSwitchError::Retired);
            }
            std::mem::swap(&mut heap.mutator_gc, &mut entry.gc);
            registry
                .young_control
                .swap_locations(registry.current.young.id(), entry.satb.young.id());
            std::mem::swap(&mut registry.current, &mut entry.satb);
        }
        let guard = TestMutatorSwitch { heap: root, id };
        // Guard exists before any TLS mirror access that could unwind. No heap
        // reference or borrow-derived pointer is retained across the callback.
        TestMutatorSwitch::refresh_tls(unsafe { root.as_mut() });
        let result = body(root);
        drop(guard);
        Ok(result)
    }
}

#[cfg(test)]
impl AliasableBox<TaggedHeap> {
    /// Root-owning fixture entry, excluding loans from this owner for the whole
    /// selection. Raw callback access must obey the switch helper's contract.
    pub(super) fn with_mutator_gc_for_test<R>(
        &mut self,
        id: TestMutatorGcId,
        body: impl FnOnce(std::ptr::NonNull<TaggedHeap>) -> R,
    ) -> Result<R, TestMutatorSwitchError> {
        // SAFETY: exclusive fixture ownership; the test body is responsible for
        // restricting itself to preallocated object/log writes and short loans.
        unsafe {
            TaggedHeap::with_registered_mutator_gc_for_test(
                self.root(),
                id,
                &StoppedMutatorsProof::for_test(),
                body,
            )
        }
    }
}

impl TaggedHeap {
    /// Loan collector-owned storage only for a stopped root-seeding window.
    ///
    /// # Safety
    /// The caller must keep mutators stopped until the returned loan is dropped.
    /// No borrow of the heap is retained, so seeding may mutate it meanwhile.
    pub(crate) unsafe fn root_seed_scratch(&mut self) -> root_seed_scratch::RootSeedLoan {
        debug_assert!(!self.sweep_in_progress());
        self._satb_layout_reserve.0.root_seed_scratch.loan()
    }
}

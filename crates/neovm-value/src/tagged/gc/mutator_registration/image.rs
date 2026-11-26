//! Owned stopped facts, never a reference to an owner's hot state.
//!
//! Producing an image does not establish stop-all authority. Encoded words and
//! addresses remain opaque until a stopped collector admission authorizes their
//! decoding. Publication/registration identity belongs to the parent module.
//! The parent's sealed state also prevents publishing an emptied shell twice.

use super::super::alloc_region::{ConsRegionSource, FloatRegionSource, RegionBook, RegionStats};
use super::super::birth_logs::BlackBornRegion;
use super::super::cons_allocator::ConsBlockBase;
use super::super::jit_state::JitHeapState;
use super::super::mark_word::MarkWord;
use super::super::mutator_gc::{MutatorGcState, SatbMutatorState};
use super::super::owner_alloc_region::OwnerAllocRegion;
use super::super::pacing::GenerationPacingCounters;
use super::super::young_registry::{
    YoungBlockLog, YoungLogSlot, YoungRegistrationId, YoungRegistryError,
};
use super::super::{HeapIdentity, MEMORY_USE_COUNT_LEN, TaggedHeap};
use crate::intern::SymId;
use crate::tagged::value::TaggedValue;
use rustc_hash::FxHashSet;

/// Response-local pacing policy, distinct from the major/minor cycle kind.
#[derive(Clone, Copy, Debug)]
pub(super) enum ResponsePacing {
    Preserve,
    Reset,
}

#[derive(Debug, thiserror::Error)]
pub(in crate::tagged::gc) enum ImageError {
    #[error("owner root scratch capacity cannot be reserved")]
    Capacity,
    #[error("allocation regions must be closed before publication")]
    RegionsOpen,
    #[error("active user-test hold blocks publication")]
    OutstandingLoan,
    #[error("proof-one publication refuses residual SATB words")]
    SatbResidual,
    #[error("image destination still owns GC facts")]
    DestinationOccupied,
    #[error("nonlegacy young blocks require the future backlink adapter")]
    NonemptyYoung,
    #[error("legacy restore requires execution roots transferred to a root owner")]
    ExecutionRoots,
    #[error(transparent)]
    Young(#[from] YoungRegistryError),
}

/// Every failure preserves the original owned payload for retry or retention.
#[derive(Debug)]
#[must_use = "retain the failed image or return it to its original owner"]
pub(in crate::tagged::gc) struct RestoreImageFailure {
    pub(in crate::tagged::gc) error: ImageError,
    pub(in crate::tagged::gc) image: PublishedGcImage,
}

/// Destination-thread storage. Its existing confined fields prohibit Send/Sync.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(in crate::tagged::gc) struct OwnerGcState {
    pub(in crate::tagged::gc) hot: MutatorGcState,
    pub(in crate::tagged::gc) cold: SatbMutatorState,
    pub(in crate::tagged::gc) jit: JitHeapState,
    pub(in crate::tagged::gc) regions: RegionBook,
    pub(in crate::tagged::gc) region_stats: RegionStats,
    /// Private owner root bank. Encapsulation prevents safe code in
    /// `tagged::gc` from injecting arbitrary cross-thread [`MarkWord`]s into
    /// slots that `mutator_registration` consumers later materialize.
    root_scratch: Vec<MarkWord>,
    pub(super) root_slot_limit: usize,
    pub(super) root_slot_generation: u64,
    pub(super) catalog_region: Option<super::super::cons_storage::CatalogRegion>,
    /// Owner-exclusive cons tails awaiting ACK/native-stop publication. Held
    /// tails block publication exactly like an open region: the collector's
    /// stopped picture must see one coherent catalog.
    pub(super) cons_tails: Vec<super::super::cons_storage::OwnedConsTail>,
    pub(super) numeric: super::numeric_allocation::NumericOwnerBook,
    /// Active [`super::table_authority::UserTestHold`] guards minted against
    /// this owner's root bank. While nonzero, root clearing and owner
    /// publication/finish refuse with `OutstandingLoan` so every hold's raw
    /// table pointer remains rooted until drop.
    pub(super) user_test_holds: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl std::fmt::Debug for OwnerGcState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerGcState")
            .field("allocated_count", &self.hot.allocated_count)
            .field("bytes_since_gc", &self.hot.bytes_since_gc)
            .field("root_words", &self.root_scratch.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Facts {
    allocated_count: usize,
    memory_use_counts: [u64; MEMORY_USE_COUNT_LEN],
    bytes_since_gc: usize,
    bytes_banked_at_resets: u64,
    pacing: GenerationPacingCounters,
    remset: Vec<MarkWord>,
    mapped_seen: FxHashSet<usize>,
    // Exposed addresses only; this module never dereferences them.
    black_headers: Vec<usize>,
    black_regions: Vec<BlackBornRegion>,
    symbol_preimages: Vec<SymId>,
    cons_writes: Vec<MarkWord>,
    snapshotted_owners: FxHashSet<usize>,
    string_preimage_addrs: FxHashSet<usize>,
    region_stats: RegionStats,
    roots: Vec<MarkWord>,
}

#[derive(Debug)]
#[must_use = "published facts must be restored, merged, or conservatively retained"]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(in crate::tagged::gc) struct PublishedGcImage {
    facts: Facts,
    young: YoungBlockLog,
}

/// Owned cycle obligations; address families retain their original meaning.
pub(super) struct CollectionWork {
    pub(super) roots: Vec<MarkWord>,
    pub(super) remembered: Vec<MarkWord>,
    pub(super) mapped: Vec<usize>,
    pub(super) black_headers: Vec<usize>,
    pub(super) black_regions: Vec<BlackBornRegion>,
    pub(super) symbols: Vec<SymId>,
    pub(super) cons_writes: Vec<MarkWord>,
    pub(super) written_owners: Vec<usize>,
}

/// Only the serial owner's cumulative history is restored after a temporary
/// stopped aggregate. The post-sweep live balance remains canonical in owner 0.
pub(super) struct CollectionHistory {
    memory: [u64; MEMORY_USE_COUNT_LEN],
    banked: u64,
    pending: usize,
    pacing: GenerationPacingCounters,
}
impl CollectionHistory {
    pub(super) fn save(heap: &TaggedHeap) -> Self {
        let h = heap.current_mutator_gc();
        Self {
            memory: h.memory_use_counts,
            banked: h.bytes_banked_at_resets,
            pending: h.bytes_since_gc,
            pacing: h.pacing,
        }
    }
    pub(super) fn restore(self, heap: &mut TaggedHeap, generational: bool) {
        let h = heap.current_mutator_gc_mut();
        h.memory_use_counts = self.memory;
        h.bytes_banked_at_resets = self.banked.saturating_add(self.pending as u64);
        h.bytes_since_gc = 0;
        if !generational {
            h.pacing = self.pacing;
        }
    }
}

/// A borrowed census item. Visiting preserves the underlying pending facts;
/// word-bearing variants expose only raw `usize` bits (never `MarkWord`) so
/// read-only observation cannot mint or transfer cross-thread `MarkWord`s.
#[derive(Debug)]
pub(in crate::tagged::gc) enum PublishedGcWork<'a> {
    Remembered(usize),
    ConsWrite(usize),
    Root(usize),
    MappedOwner(usize),
    BlackHeader(usize),
    BlackRegion(&'a BlackBornRegion),
    SymbolPreimage(SymId),
    SnapshottedOwner(usize),
    StringPreimageAddress(usize),
    YoungBlock(ConsBlockBase),
}

/// Counts only: observing this does not authorize decoding any heap word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::tagged::gc) struct StoppedGcSummary {
    pub(in crate::tagged::gc) allocated_count: usize,
    pub(in crate::tagged::gc) memory_use_counts: [u64; MEMORY_USE_COUNT_LEN],
    pub(in crate::tagged::gc) bytes_since_gc: usize,
    pub(in crate::tagged::gc) bytes_banked_at_resets: u64,
    pub(in crate::tagged::gc) promoted_since_major: usize,
    pub(in crate::tagged::gc) minors_since_major: usize,
    pub(in crate::tagged::gc) stress_cycles_since_major: usize,
    pub(in crate::tagged::gc) remembered_words: usize,
    pub(in crate::tagged::gc) mapped_owners: usize,
    pub(in crate::tagged::gc) black_headers: usize,
    pub(in crate::tagged::gc) black_regions: usize,
    pub(in crate::tagged::gc) symbol_preimages: usize,
    pub(in crate::tagged::gc) cons_writes: usize,
    pub(in crate::tagged::gc) snapshotted_owners: usize,
    pub(in crate::tagged::gc) string_preimage_addrs: usize,
    pub(in crate::tagged::gc) root_words: usize,
    pub(in crate::tagged::gc) young_blocks: usize,
}

static_assertions::assert_not_impl_any!(OwnerGcState: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(PublishedGcImage: Send, Sync, std::fmt::Debug);
static_assertions::assert_not_impl_any!(PublishedGcImage: Clone, Copy);

fn validate_closed(
    hot: &MutatorGcState,
    cold: &SatbMutatorState,
    jit: &JitHeapState,
    regions: &RegionBook,
    owner: &OwnerAllocRegion,
) -> Result<(), ImageError> {
    if !owner.is_closed()
        || regions.cons != ConsRegionSource::Closed
        || regions.float != FloatRegionSource::Closed
        || jit.has_open_cursors()
        || hot.black_cons_region_start.is_some()
        || hot.black_float_region_start.is_some()
    {
        return Err(ImageError::RegionsOpen);
    }
    if !cold.satb.is_empty() {
        return Err(ImageError::SatbResidual);
    }
    cold.young.resident()?;
    Ok(())
}

fn empty_destination(hot: &MutatorGcState, cold: &SatbMutatorState, roots: &[MarkWord]) -> bool {
    hot.allocated_count == 0
        && hot.memory_use_counts == [0; MEMORY_USE_COUNT_LEN]
        && hot.bytes_since_gc == 0
        && hot.bytes_banked_at_resets == 0
        && hot.pacing == GenerationPacingCounters::default()
        && hot.remset.is_empty()
        && hot.r_mapped_seen.is_empty()
        && hot.black_born.is_empty()
        && hot.black_born_regions.is_empty()
        && hot.major_symbol_preimages.is_empty()
        && hot.major_cons_writes.is_empty()
        && cold.satb_snapshotted_owners.is_empty()
        && cold.satb_string_preimage_addrs.is_empty()
        && roots.is_empty()
}

impl Facts {
    fn take(
        hot: &mut MutatorGcState,
        cold: &mut SatbMutatorState,
        roots: Vec<MarkWord>,
        stats: RegionStats,
    ) -> Self {
        let old = std::mem::replace(hot, MutatorGcState::new());
        Self {
            allocated_count: old.allocated_count,
            memory_use_counts: old.memory_use_counts,
            bytes_since_gc: old.bytes_since_gc,
            bytes_banked_at_resets: old.bytes_banked_at_resets,
            pacing: old.pacing,
            // SAFETY: `old` belongs to this owner's heap; the published image
            // stays within the originating heap's era protocol until restore
            // or stopped-collector merge before any sweep.
            remset: old
                .remset
                .into_iter()
                .map(|value| unsafe { MarkWord::of(value) })
                .collect(),
            mapped_seen: old.r_mapped_seen,
            black_headers: old.black_born.into_iter().map(|ptr| ptr as usize).collect(),
            black_regions: old.black_born_regions,
            symbol_preimages: old.major_symbol_preimages,
            // SAFETY: same owner-publication contract as `remset` above.
            cons_writes: old
                .major_cons_writes
                .into_iter()
                .map(|value| unsafe { MarkWord::of(value) })
                .collect(),
            snapshotted_owners: std::mem::take(&mut cold.satb_snapshotted_owners),
            string_preimage_addrs: std::mem::take(&mut cold.satb_string_preimage_addrs),
            region_stats: stats,
            roots,
        }
    }

    /// # Safety
    /// The caller must hold owning-mutator or stopped-collector authority on
    /// the thread owning `hot`/`cold` for the heap era that published these
    /// facts, and no sweep may have reclaimed any queued `MarkWord`.
    unsafe fn restore(
        self,
        hot: &mut MutatorGcState,
        cold: &mut SatbMutatorState,
    ) -> (Vec<MarkWord>, RegionStats) {
        hot.allocated_count = self.allocated_count;
        hot.memory_use_counts = self.memory_use_counts;
        hot.bytes_since_gc = self.bytes_since_gc;
        hot.bytes_banked_at_resets = self.bytes_banked_at_resets;
        hot.pacing = self.pacing;
        // SAFETY: caller upholds owning-thread and live-era contract for all
        // queued `MarkWord`s in `self`.
        hot.remset = self
            .remset
            .into_iter()
            .map(|word| unsafe { word.value() })
            .collect();
        hot.r_mapped_seen = self.mapped_seen;
        hot.black_born = self
            .black_headers
            .into_iter()
            .map(|addr| addr as *mut _)
            .collect();
        hot.black_born_regions = self.black_regions;
        hot.major_symbol_preimages = self.symbol_preimages;
        // SAFETY: same caller contract as `remset` above.
        hot.major_cons_writes = self
            .cons_writes
            .into_iter()
            .map(|word| unsafe { word.value() })
            .collect();
        // Do not restore a stale direct-mapped reject cache.
        hot.remembered_cache.fill(0);
        cold.satb_snapshotted_owners = self.snapshotted_owners;
        cold.satb_string_preimage_addrs = self.string_preimage_addrs;
        (self.roots, self.region_stats)
    }

    /// # Safety
    /// The caller must hold stopped-collector authority on the thread owning
    /// `heap` for the heap era that published these facts, and no sweep may
    /// have reclaimed any queued `MarkWord`.
    unsafe fn merge_into_heap(self, heap: &mut TaggedHeap) {
        let hot = heap.current_mutator_gc_mut();
        hot.allocated_count = hot.allocated_count.saturating_add(self.allocated_count);
        for (dst, src) in hot.memory_use_counts.iter_mut().zip(self.memory_use_counts) {
            *dst = dst.wrapping_add(src);
        }
        hot.bytes_since_gc = hot.bytes_since_gc.saturating_add(self.bytes_since_gc);
        hot.bytes_banked_at_resets = hot
            .bytes_banked_at_resets
            .saturating_add(self.bytes_banked_at_resets);
        hot.pacing.promoted_since_major = hot
            .pacing
            .promoted_since_major
            .saturating_add(self.pacing.promoted_since_major);
        hot.pacing.minors_since_major = hot
            .pacing
            .minors_since_major
            .saturating_add(self.pacing.minors_since_major);
        hot.pacing.stress_cycles_since_major = hot
            .pacing
            .stress_cycles_since_major
            .saturating_add(self.pacing.stress_cycles_since_major);
        // SAFETY: caller upholds stopped-collector and live-era contract for
        // all queued `MarkWord`s in `self`.
        hot.remset
            .extend(self.remset.into_iter().map(|word| unsafe { word.value() }));
        hot.r_mapped_seen.extend(self.mapped_seen);
        hot.black_born
            .extend(self.black_headers.into_iter().map(|addr| addr as *mut _));
        hot.black_born_regions.extend(self.black_regions);
        hot.major_symbol_preimages.extend(self.symbol_preimages);
        // SAFETY: same caller contract as `remset` above.
        hot.major_cons_writes.extend(
            self.cons_writes
                .into_iter()
                .map(|word| unsafe { word.value() }),
        );
        hot.remembered_cache.fill(0);
        let cold = heap.current_mutator_satb_mut();
        cold.satb_snapshotted_owners.extend(self.snapshotted_owners);
        cold.satb_string_preimage_addrs
            .extend(self.string_preimage_addrs);
        macro_rules! add_stats { ($($field:ident),* $(,)?) => { $(heap.region_stats.$field = heap.region_stats.$field.saturating_add(self.region_stats.$field);)* }; }
        add_stats!(
            cons_refills,
            cons_cells_granted,
            cons_short_runs,
            cons_from_free_list,
            cons_from_block_tail,
            cons_from_fresh_block,
            cons_tail_closes,
            float_refills,
            float_slots_granted,
            float_short_runs,
            float_tail_closes
        );
    }
}

impl OwnerGcState {
    pub(in crate::tagged::gc) fn new(
        heap: HeapIdentity,
        id: YoungRegistrationId,
        root_capacity: usize,
    ) -> Result<Self, ImageError> {
        if id.heap_identity() != heap {
            return Err(YoungRegistryError::WrongHeap.into());
        }
        let mut root_scratch = Vec::new();
        root_scratch
            .try_reserve_exact(root_capacity)
            .map_err(|_| ImageError::Capacity)?;
        Ok(Self {
            hot: MutatorGcState::new(),
            cold: SatbMutatorState::new(YoungLogSlot::new_parallel(id)),
            jit: JitHeapState::new(),
            regions: RegionBook::new(),
            region_stats: RegionStats::default(),
            root_scratch,
            root_slot_limit: root_capacity,
            root_slot_generation: 1,
            catalog_region: None,
            cons_tails: Vec::new(),
            numeric: super::numeric_allocation::NumericOwnerBook::new(),
            user_test_holds: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }

    pub(in crate::tagged::gc) fn validate_publication(&self) -> Result<(), ImageError> {
        // The serial image has no frame arena. An owner publication passes
        // that arena's book to [`Self::validate_publication_with_owner`].
        self.validate_publication_with_owner(&OwnerAllocRegion::new())
    }

    /// Same checks as [`Self::validate_publication`], plus this owner's book.
    /// An open book is `ImageError::RegionsOpen` and moves nothing.
    pub(in crate::tagged::gc) fn validate_publication_with_owner(
        &self,
        owner: &OwnerAllocRegion,
    ) -> Result<(), ImageError> {
        if self
            .user_test_holds
            .load(std::sync::atomic::Ordering::Relaxed)
            != 0
        {
            return Err(ImageError::OutstandingLoan);
        }
        if self.catalog_region.is_some() || !self.cons_tails.is_empty() || !self.numeric.is_closed()
        {
            return Err(ImageError::RegionsOpen);
        }
        validate_closed(&self.hot, &self.cold, &self.jit, &self.regions, owner)
    }

    /// Freeze this owner's closed local GC state into a `Send + Sync`
    /// [`PublishedGcImage`].
    ///
    /// # Safety
    /// The caller must hold exclusive owning-thread authority for this owner on
    /// the live heap identified by `self.cold.young.id().heap_identity()`, and
    /// every `TaggedValue` / [`MarkWord`] currently stored in `self.hot.remset`,
    /// `self.hot.major_cons_writes`, and `self.root_scratch` must belong to that
    /// heap and stay live until the returned image is restored, collected, or
    /// conservatively retained.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[cfg_attr(feature = "test-support", allow(private_interfaces))]
    pub(in crate::tagged::gc) unsafe fn publish(&mut self) -> Result<PublishedGcImage, ImageError> {
        self.validate_publication()?;
        let young = self.cold.young.take_owner_payload()?;
        let roots = std::mem::take(&mut self.root_scratch);
        let stats = std::mem::take(&mut self.region_stats);
        Ok(PublishedGcImage {
            facts: Facts::take(&mut self.hot, &mut self.cold, roots, stats),
            young,
        })
    }

    /// Restore a published owner image back into this destination-thread state.
    ///
    /// # Safety
    /// `image` must have been published from the same live heap era on this
    /// owner's thread (or handed back by the stopped-world rendezvous protocol
    /// for this exact owner), and no sweep may have reclaimed any `MarkWord`
    /// stored inside `image`.
    pub(in crate::tagged::gc) unsafe fn restore(
        &mut self,
        image: PublishedGcImage,
    ) -> Result<(), RestoreImageFailure> {
        let result = validate_closed(
            &self.hot,
            &self.cold,
            &self.jit,
            &self.regions,
            &OwnerAllocRegion::new(),
        )
        .and_then(|()| {
            if !self.numeric.is_closed() {
                Err(ImageError::RegionsOpen)
            } else if empty_destination(&self.hot, &self.cold, &self.root_scratch) {
                Ok(())
            } else {
                Err(ImageError::DestinationOccupied)
            }
        });
        if let Err(error) = result {
            return Err(RestoreImageFailure { error, image });
        }
        let PublishedGcImage { facts, young } = image;
        if let Err((error, young)) = self.cold.young.restore_owner_payload(young) {
            return Err(RestoreImageFailure {
                error: error.into(),
                image: PublishedGcImage { facts, young },
            });
        }
        // SAFETY: caller upholds owning-thread and live-era contract for `facts`.
        (self.root_scratch, self.region_stats) =
            unsafe { facts.restore(&mut self.hot, &mut self.cold) };
        Ok(())
    }

    pub(super) fn root_scratch_len(&self) -> usize {
        self.root_scratch.len()
    }

    pub(super) fn root_scratch_capacity(&self) -> usize {
        self.root_scratch.capacity()
    }

    pub(super) fn root_scratch_is_empty(&self) -> bool {
        self.root_scratch.is_empty()
    }

    pub(super) fn root_slot_bits(&self, index: usize) -> Option<usize> {
        self.root_scratch.get(index).map(|word| word.bits())
    }

    pub(super) fn root_slot_word(&self, index: usize) -> Option<MarkWord> {
        self.root_scratch.get(index).copied()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn root_words_bits(&self) -> Vec<usize> {
        self.root_scratch.iter().map(|word| word.bits()).collect()
    }

    pub(super) fn push_nil_root_slot(&mut self) {
        self.root_scratch.push(MarkWord::nil());
    }

    pub(super) fn set_root_slot_nil(&mut self, index: usize) -> Option<()> {
        *self.root_scratch.get_mut(index)? = MarkWord::nil();
        Some(())
    }

    pub(super) fn clear_root_scratch(&mut self) {
        self.root_scratch.clear();
    }

    /// Push a same-heap [`TaggedValue`] into this owner's root bank.
    ///
    /// # Safety
    /// `value` must belong to this owner's heap (or be an immediate) and must
    /// remain valid under this owner's active era protocol.
    pub(super) unsafe fn push_root_slot(&mut self, value: TaggedValue) {
        // SAFETY: caller guarantees `value` belongs to this owner's heap.
        self.root_scratch.push(unsafe { MarkWord::of(value) });
    }

    /// Overwrite one resident slot in this owner's root bank with a same-heap
    /// [`TaggedValue`].
    ///
    /// # Safety
    /// `value` must belong to this owner's heap (or be an immediate) and must
    /// remain valid under this owner's active era protocol.
    pub(super) unsafe fn set_root_slot(&mut self, index: usize, value: TaggedValue) -> Option<()> {
        let slot = self.root_scratch.get_mut(index)?;
        // SAFETY: caller guarantees `value` belongs to this owner's heap.
        *slot = unsafe { MarkWord::of(value) };
        Some(())
    }

    /// Split resident owner fields for [`super::run_slice::RunSlice`].
    ///
    /// # Safety
    /// The caller must hold exclusive [`super::RunAccess`] on this owner and
    /// only write same-heap live values through validated root slots.
    #[allow(clippy::type_complexity)]
    pub(super) unsafe fn split_for_slice(
        &mut self,
    ) -> (
        &mut MutatorGcState,
        &mut SatbMutatorState,
        &mut RegionStats,
        &mut Option<super::super::cons_storage::CatalogRegion>,
        &mut Vec<super::super::cons_storage::OwnedConsTail>,
        u64,
        &mut [MarkWord],
    ) {
        (
            &mut self.hot,
            &mut self.cold,
            &mut self.region_stats,
            &mut self.catalog_region,
            &mut self.cons_tails,
            self.root_slot_generation,
            &mut self.root_scratch,
        )
    }

    #[cfg(test)]
    pub(super) fn root_scratch_words_for_test(&self) -> &[MarkWord] {
        &self.root_scratch
    }

    #[cfg(test)]
    pub(super) fn push_immediate_root_slot_for_test(&mut self, value: TaggedValue) {
        self.root_scratch.push(
            MarkWord::from_immediate(value)
                .expect("push_immediate_root_slot_for_test requires an immediate"),
        );
    }

    /// Test-only synthetic cursor probe for `image_tests` open-region refusal checks.
    ///
    /// Safe because `OwnerGcState` is an offline fact container whose `jit`
    /// field is only inspected by `validate_closed` (never dereferenced by
    /// `TaggedHeap` allocation or JIT code).
    #[cfg(test)]
    pub(super) fn set_synthetic_jit_cursors_for_test(
        &mut self,
        cons_cur: usize,
        cons_lim: usize,
        float_cur: usize,
        float_lim: usize,
    ) {
        // SAFETY: `OwnerGcState.jit` is never allocated from by Rust or JIT
        // code; only `validate_closed` reads `has_open_cursors()`.
        unsafe {
            self.jit
                .set_raw_cursors_for_test(cons_cur, cons_lim, float_cur, float_lim);
        }
    }
}

impl PublishedGcImage {
    pub(super) fn charge_symbol_tables(
        &mut self,
        bytes: super::super::symbol_table::publication::RetainedTableBytes,
    ) {
        self.facts.bytes_since_gc = self.facts.bytes_since_gc.saturating_add(bytes.get());
    }

    /// Seed test words into a published image before collection.
    ///
    /// # Safety
    /// `root`, `remembered`, and `cons_write` must be immediates or live
    /// objects belonging to the heap that owns this image.
    #[cfg(any(test, feature = "test-support"))]
    pub(in crate::tagged::gc) unsafe fn seed_collection_words_for_test(
        &mut self,
        root: MarkWord,
        remembered: MarkWord,
        cons_write: MarkWord,
    ) {
        self.facts.roots.push(root);
        self.facts.remset.push(remembered);
        self.facts.cons_writes.push(cons_write);
    }

    /// Copy into collector-owned work before retiring any original cycle log.
    /// Root scratch is owner state and survives the response unchanged.
    pub(super) fn take_collection_work(&mut self) -> CollectionWork {
        let f = &mut self.facts;
        let work = CollectionWork {
            roots: f.roots.clone(),
            remembered: f.remset.clone(),
            mapped: f.mapped_seen.iter().copied().collect(),
            black_headers: f.black_headers.clone(),
            black_regions: f.black_regions.clone(),
            symbols: f.symbol_preimages.clone(),
            cons_writes: f.cons_writes.clone(),
            written_owners: f.snapshotted_owners.iter().copied().collect(),
        };
        f.remset.clear();
        f.mapped_seen.clear();
        f.black_headers.clear();
        f.black_regions.clear();
        f.symbol_preimages.clear();
        f.cons_writes.clear();
        f.snapshotted_owners.clear();
        // This set only deduplicates string preimages. Publication proved the
        // actual SATB word buffer empty; its addresses are not tagged roots.
        f.string_preimage_addrs.clear();
        work
    }

    /// Aggregate only scalars into the stopped collector's inline scratch.
    /// All regions are closed; no allocation or observation can race this sum.
    pub(super) fn add_collection_scalars(&self, heap: &mut TaggedHeap) {
        let f = &self.facts;
        let h = heap.current_mutator_gc_mut();
        h.allocated_count = h.allocated_count.saturating_add(f.allocated_count);
        h.bytes_since_gc = h.bytes_since_gc.saturating_add(f.bytes_since_gc);
        h.bytes_banked_at_resets = h
            .bytes_banked_at_resets
            .saturating_add(f.bytes_banked_at_resets);
        for (dst, src) in h.memory_use_counts.iter_mut().zip(f.memory_use_counts) {
            *dst = dst.wrapping_add(src);
        }
        h.pacing.promoted_since_major = h
            .pacing
            .promoted_since_major
            .saturating_add(f.pacing.promoted_since_major);
        h.pacing.minors_since_major = h
            .pacing
            .minors_since_major
            .saturating_add(f.pacing.minors_since_major);
        h.pacing.stress_cycles_since_major = h
            .pacing
            .stress_cycles_since_major
            .saturating_add(f.pacing.stress_cycles_since_major);
        macro_rules! add_stats { ($($field:ident),* $(,)?) => { $(heap.region_stats.$field = heap.region_stats.$field.saturating_add(f.region_stats.$field);)* }; }
        add_stats!(
            cons_refills,
            cons_cells_granted,
            cons_short_runs,
            cons_from_free_list,
            cons_from_block_tail,
            cons_from_fresh_block,
            cons_tail_closes,
            float_refills,
            float_slots_granted,
            float_short_runs,
            float_tail_closes
        );
    }

    pub(super) fn finish_collection_response(&mut self, pacing: ResponsePacing) {
        // This is a charged live balance, not lifetime allocation history or
        // object ownership. Owner 0 retains the exact post-sweep aggregate;
        // peers start accruing new local credits after this response.
        self.facts.allocated_count = 0;
        self.facts.bytes_banked_at_resets = self
            .facts
            .bytes_banked_at_resets
            .saturating_add(self.facts.bytes_since_gc as u64);
        self.facts.bytes_since_gc = 0;
        match pacing {
            ResponsePacing::Preserve => {}
            ResponsePacing::Reset => self.facts.pacing = GenerationPacingCounters::default(),
        }
        self.facts.region_stats = RegionStats::default();
    }

    /// A4's stopped fixture consumes only its known immediate work marker.
    /// It cannot discard another payload or act as a production collector.
    #[cfg(any(test, feature = "test-support"))]
    pub(in crate::tagged::gc) fn consume_synthetic_remembered_for_test(
        &mut self,
        expected: MarkWord,
    ) -> Result<usize, ()> {
        if self.facts.remset.iter().any(|word| *word != expected) {
            return Err(());
        }
        let count = self.facts.remset.len();
        self.facts.remset.clear();
        Ok(count)
    }

    pub(in crate::tagged::gc) fn young_id(&self) -> YoungRegistrationId {
        self.young.id()
    }

    pub(in crate::tagged::gc) fn roots_empty(&self) -> bool {
        self.facts.roots.is_empty()
    }

    pub(in crate::tagged::gc) fn young_empty(&self) -> bool {
        self.young.blocks().is_empty() && self.young.pending_index().is_none()
    }

    pub(in crate::tagged::gc) fn young(&self) -> &YoungBlockLog {
        &self.young
    }

    pub(in crate::tagged::gc) fn young_mut(&mut self) -> &mut YoungBlockLog {
        &mut self.young
    }

    pub(in crate::tagged::gc) fn pacing(&self) -> GenerationPacingCounters {
        self.facts.pacing
    }

    /// Visit the encoded-word families as raw `usize` bits without consuming or
    /// clearing a log and without exporting `MarkWord` values.
    /// Address/region/symbol families are separate `visit_work` variants and
    /// have separate summary counts; their raw addresses are not tagged words.
    pub(in crate::tagged::gc) fn visit_logs(&self, mut visitor: impl FnMut(usize)) {
        for &word in self
            .facts
            .remset
            .iter()
            .chain(&self.facts.cons_writes)
            .chain(&self.facts.roots)
        {
            visitor(word.bits());
        }
    }

    /// VIS row: copied count of the image's root words (the value
    /// `summary().root_words` returns) for the re-homed root-slot tests.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn root_words_for_test(&self) -> usize {
        self.facts.roots.len()
    }

    /// VIS row: copied bits of every word `visit_logs` yields, in the same
    /// order, for the re-homed root-slot tests.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn log_value_bits_for_test(&self) -> Vec<usize> {
        let mut out = Vec::new();
        self.visit_logs(|bits| out.push(bits));
        out
    }

    /// Complete read-only work census, including families that cannot be
    /// represented as tagged words. Word-bearing variants yield raw `usize`
    /// bits (never `MarkWord`). This does not perform a collector fold.
    pub(in crate::tagged::gc) fn visit_work(&self, mut visitor: impl FnMut(PublishedGcWork<'_>)) {
        let f = &self.facts;
        for &word in &f.remset {
            visitor(PublishedGcWork::Remembered(word.bits()));
        }
        for &word in &f.cons_writes {
            visitor(PublishedGcWork::ConsWrite(word.bits()));
        }
        for &word in &f.roots {
            visitor(PublishedGcWork::Root(word.bits()));
        }
        for &addr in &f.mapped_seen {
            visitor(PublishedGcWork::MappedOwner(addr));
        }
        for &addr in &f.black_headers {
            visitor(PublishedGcWork::BlackHeader(addr));
        }
        for region in &f.black_regions {
            visitor(PublishedGcWork::BlackRegion(region));
        }
        for &id in &f.symbol_preimages {
            visitor(PublishedGcWork::SymbolPreimage(id));
        }
        for &addr in &f.snapshotted_owners {
            visitor(PublishedGcWork::SnapshottedOwner(addr));
        }
        for &addr in &f.string_preimage_addrs {
            visitor(PublishedGcWork::StringPreimageAddress(addr));
        }
        for &base in self.young.blocks() {
            visitor(PublishedGcWork::YoungBlock(base));
        }
    }

    /// Freeze the serial collector heap's closed current-mutator state into a
    /// `Send + Sync` [`PublishedGcImage`].
    ///
    /// # Safety
    /// The caller must hold exclusive stopped-world / owning-thread authority
    /// over `heap`, and every `TaggedValue` stored in
    /// `heap.current_mutator_gc().{remset, major_cons_writes}` must belong to
    /// `heap` and stay live until the returned image is restored or collected
    /// on `heap`.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    #[cfg_attr(feature = "test-support", allow(private_interfaces))]
    pub(in crate::tagged::gc) unsafe fn from_legacy(
        heap: &mut TaggedHeap,
    ) -> Result<Self, ImageError> {
        validate_closed(
            heap.current_mutator_gc(),
            heap.current_mutator_satb(),
            &heap.jit,
            &heap.region_book,
            &OwnerAllocRegion::new(),
        )?;
        let young = heap.current_mutator_satb_mut().young.take_owner_payload()?;
        let stats = std::mem::take(&mut heap.region_stats);
        let mut hot = std::mem::replace(heap.current_mutator_gc_mut(), MutatorGcState::new());
        Ok(Self {
            facts: Facts::take(&mut hot, heap.current_mutator_satb_mut(), Vec::new(), stats),
            young,
        })
    }

    /// Restore a frozen serial image back into `heap`.
    ///
    /// # Safety
    /// `self` must belong to `heap`'s current live era, the caller must hold
    /// stopped-collector / owning-thread authority over `heap`, and no sweep
    /// may have reclaimed any `MarkWord` stored inside `self`.
    pub(in crate::tagged::gc) unsafe fn restore_legacy(
        self,
        heap: &mut TaggedHeap,
    ) -> Result<(), RestoreImageFailure> {
        let result = validate_closed(
            heap.current_mutator_gc(),
            heap.current_mutator_satb(),
            &heap.jit,
            &heap.region_book,
            &OwnerAllocRegion::new(),
        )
        .and_then(|()| {
            if !self.facts.roots.is_empty() {
                Err(ImageError::ExecutionRoots)
            } else if !empty_destination(
                heap.current_mutator_gc(),
                heap.current_mutator_satb(),
                &[],
            ) {
                Err(ImageError::DestinationOccupied)
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            return Err(RestoreImageFailure { error, image: self });
        }
        let Self { facts, young } = self;
        if let Err((error, young)) = heap
            .current_mutator_satb_mut()
            .young
            .restore_owner_payload(young)
        {
            return Err(RestoreImageFailure {
                error: error.into(),
                image: Self { facts, young },
            });
        }
        let mut hot = MutatorGcState::new();
        // SAFETY: caller upholds stopped-collector and live-era contract for `facts`.
        let (_, stats) = unsafe { facts.restore(&mut hot, heap.current_mutator_satb_mut()) };
        *heap.current_mutator_gc_mut() = hot;
        heap.region_stats = stats;
        Ok(())
    }

    /// Like [`Self::restore_legacy`], but folds any live serial-turn /
    /// `WorldTurn` GC facts and resident young-log blocks accrued directly on
    /// `heap` while entry 0 was frozen into the restored legacy state instead
    /// of refusing with `DestinationOccupied` or `LogNotLoaned`.
    ///
    /// When both `self.young` and `heap`'s resident shell hold pending young
    /// blocks, removing the shell's duplicate `pending_mutators` entry may
    /// swap-remove another live registration into the freed slot; `resolve_peer`
    /// resolves that registration's `YoungBlockLog` during preflight (before
    /// any mutation) so its pending index can be validated and repaired.
    ///
    /// # Safety
    /// Same stopped-collector and live-era contract as [`Self::restore_legacy`].
    pub(in crate::tagged::gc) unsafe fn restore_legacy_for_collection<'a>(
        mut self,
        heap: &mut TaggedHeap,
        resolve_peer: impl FnOnce(YoungRegistrationId) -> Option<&'a mut YoungBlockLog>,
    ) -> Result<(), RestoreImageFailure> {
        let young_shell_empty = heap
            .current_mutator_satb()
            .young
            .resident()
            .is_ok_and(|shell| shell.blocks().is_empty() && shell.pending_index().is_none());
        if young_shell_empty
            && empty_destination(heap.current_mutator_gc(), heap.current_mutator_satb(), &[])
        {
            // SAFETY: upheld by caller contract.
            return unsafe { self.restore_legacy(heap) };
        }
        let result = validate_closed(
            heap.current_mutator_gc(),
            heap.current_mutator_satb(),
            &heap.jit,
            &heap.region_book,
            &OwnerAllocRegion::new(),
        )
        .and_then(|()| {
            if !self.facts.roots.is_empty() {
                Err(ImageError::ExecutionRoots)
            } else {
                Ok(())
            }
        });
        if let Err(error) = result {
            return Err(RestoreImageFailure { error, image: self });
        }
        if !young_shell_empty {
            let merge_result = match heap._satb_layout_reserve.current_young_log_mut() {
                Ok(shell) => heap.cons_allocator.merge_legacy_young_shell(
                    &mut self.young,
                    shell,
                    resolve_peer,
                ),
                Err(error) => Err(error),
            };
            if let Err(error) = merge_result {
                return Err(RestoreImageFailure {
                    error: error.into(),
                    image: self,
                });
            }
        }
        let stats = std::mem::take(&mut heap.region_stats);
        let mut live_hot = std::mem::replace(heap.current_mutator_gc_mut(), MutatorGcState::new());
        let serial_delta = Facts::take(
            &mut live_hot,
            heap.current_mutator_satb_mut(),
            Vec::new(),
            stats,
        );
        // SAFETY: upheld by caller contract.
        if let Err(failure) = unsafe { self.restore_legacy(heap) } {
            let mut restored_hot = MutatorGcState::new();
            // SAFETY: `serial_delta` was just taken from `heap` on this thread.
            let (_, restored_stats) =
                unsafe { serial_delta.restore(&mut restored_hot, heap.current_mutator_satb_mut()) };
            *heap.current_mutator_gc_mut() = restored_hot;
            heap.region_stats = restored_stats;
            return Err(failure);
        }
        // SAFETY: `serial_delta` was just taken from `heap` on this thread.
        unsafe { serial_delta.merge_into_heap(heap) };
        Ok(())
    }

    pub(in crate::tagged::gc) fn summary(&self) -> StoppedGcSummary {
        let f = &self.facts;
        StoppedGcSummary {
            allocated_count: f.allocated_count,
            memory_use_counts: f.memory_use_counts,
            bytes_since_gc: f.bytes_since_gc,
            bytes_banked_at_resets: f.bytes_banked_at_resets,
            promoted_since_major: f.pacing.promoted_since_major,
            minors_since_major: f.pacing.minors_since_major,
            stress_cycles_since_major: f.pacing.stress_cycles_since_major,
            remembered_words: f.remset.len(),
            mapped_owners: f.mapped_seen.len(),
            black_headers: f.black_headers.len(),
            black_regions: f.black_regions.len(),
            symbol_preimages: f.symbol_preimages.len(),
            cons_writes: f.cons_writes.len(),
            snapshotted_owners: f.snapshotted_owners.len(),
            string_preimage_addrs: f.string_preimage_addrs.len(),
            root_words: f.roots.len(),
            young_blocks: self.young.blocks().len(),
        }
    }

    /// Test-only: drain pending root scratch as raw `usize` bits (never
    /// exporting `MarkWord`). Sole caller is `mutator_registration/tests.rs`.
    #[cfg(any(test, feature = "test-support"))]
    pub(in crate::tagged::gc) fn take_root_scratch_bits_for_test(&mut self) -> Vec<usize> {
        std::mem::take(&mut self.facts.roots)
            .into_iter()
            .map(MarkWord::bits)
            .collect()
    }

    /// Pending reachability/log obligations, excluding historical counters.
    pub(in crate::tagged::gc) fn is_empty(&self) -> bool {
        let s = self.summary();
        s.remembered_words == 0
            && s.mapped_owners == 0
            && s.black_headers == 0
            && s.black_regions == 0
            && s.symbol_preimages == 0
            && s.cons_writes == 0
            && s.snapshotted_owners == 0
            && s.string_preimage_addrs == 0
            && s.root_words == 0
            && s.young_blocks == 0
            && self.young.pending_index().is_none()
    }

    /// Merge once at explicit era finish. Young backlinks must already have
    /// been drained by collection; terminal transfer never silently loses them.
    pub(in crate::tagged::gc) fn merge_non_young(
        &mut self,
        other: Self,
    ) -> Result<(), RestoreImageFailure> {
        let error = if self.young.id().heap_identity() != other.young.id().heap_identity() {
            Some(ImageError::Young(YoungRegistryError::WrongHeap))
        } else if !other.young.blocks().is_empty() || other.young.pending_index().is_some() {
            Some(ImageError::NonemptyYoung)
        } else {
            None
        };
        if let Some(error) = error {
            return Err(RestoreImageFailure {
                error,
                image: other,
            });
        }
        let dst = &mut self.facts;
        let mut src = other.facts;
        dst.allocated_count = dst.allocated_count.saturating_add(src.allocated_count);
        dst.bytes_since_gc = dst.bytes_since_gc.saturating_add(src.bytes_since_gc);
        dst.bytes_banked_at_resets = dst
            .bytes_banked_at_resets
            .saturating_add(src.bytes_banked_at_resets);
        for (total, count) in dst.memory_use_counts.iter_mut().zip(src.memory_use_counts) {
            *total = total.wrapping_add(count);
        }
        dst.pacing.promoted_since_major = dst
            .pacing
            .promoted_since_major
            .saturating_add(src.pacing.promoted_since_major);
        dst.pacing.minors_since_major = dst
            .pacing
            .minors_since_major
            .saturating_add(src.pacing.minors_since_major);
        dst.pacing.stress_cycles_since_major = dst
            .pacing
            .stress_cycles_since_major
            .saturating_add(src.pacing.stress_cycles_since_major);
        dst.remset.append(&mut src.remset);
        dst.mapped_seen.extend(src.mapped_seen);
        dst.black_headers.append(&mut src.black_headers);
        dst.black_regions.append(&mut src.black_regions);
        dst.symbol_preimages.append(&mut src.symbol_preimages);
        dst.cons_writes.append(&mut src.cons_writes);
        dst.snapshotted_owners.extend(src.snapshotted_owners);
        dst.string_preimage_addrs.extend(src.string_preimage_addrs);
        dst.roots.append(&mut src.roots);
        macro_rules! add_stats { ($($field:ident),* $(,)?) => { $(dst.region_stats.$field = dst.region_stats.$field.saturating_add(src.region_stats.$field);)* }; }
        add_stats!(
            cons_refills,
            cons_cells_granted,
            cons_short_runs,
            cons_from_free_list,
            cons_from_block_tail,
            cons_from_fresh_block,
            cons_tail_closes,
            float_refills,
            float_slots_granted,
            float_short_runs,
            float_tail_closes
        );
        Ok(())
    }
}

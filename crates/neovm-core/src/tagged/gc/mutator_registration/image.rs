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
use super::super::pacing::GenerationPacingCounters;
use super::super::young_registry::{
    YoungBlockLog, YoungLogSlot, YoungRegistrationId, YoungRegistryError,
};
use super::super::{HeapIdentity, MEMORY_USE_COUNT_LEN, TaggedHeap};
use crate::emacs_core::intern::SymId;
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
pub(in crate::tagged::gc) struct OwnerGcState {
    pub(in crate::tagged::gc) hot: MutatorGcState,
    pub(in crate::tagged::gc) cold: SatbMutatorState,
    pub(in crate::tagged::gc) jit: JitHeapState,
    pub(in crate::tagged::gc) regions: RegionBook,
    pub(in crate::tagged::gc) region_stats: RegionStats,
    pub(in crate::tagged::gc) root_scratch: Vec<MarkWord>,
    pub(super) root_slot_limit: usize,
    pub(super) root_slot_generation: u64,
    pub(super) catalog_region: Option<super::super::cons_storage::CatalogRegion>,
    /// Owner-exclusive cons tails awaiting ACK/native-stop publication. Held
    /// tails block publication exactly like an open region: the collector's
    /// stopped picture must see one coherent catalog.
    pub(super) cons_tails: Vec<super::super::cons_storage::OwnedConsTail>,
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
/// interpreting an address or decoding a word still requires collector authority.
#[derive(Debug)]
pub(in crate::tagged::gc) enum PublishedGcWork<'a> {
    Remembered(MarkWord),
    ConsWrite(MarkWord),
    Root(MarkWord),
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
) -> Result<(), ImageError> {
    if regions.cons != ConsRegionSource::Closed
        || regions.float != FloatRegionSource::Closed
        || jit.cons_cur.get() != 0
        || jit.cons_lim.get() != 0
        || jit.float_cur.get() != 0
        || jit.float_lim.get() != 0
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
            remset: old.remset.into_iter().map(MarkWord::of).collect(),
            mapped_seen: old.r_mapped_seen,
            black_headers: old.black_born.into_iter().map(|ptr| ptr as usize).collect(),
            black_regions: old.black_born_regions,
            symbol_preimages: old.major_symbol_preimages,
            cons_writes: old
                .major_cons_writes
                .into_iter()
                .map(MarkWord::of)
                .collect(),
            snapshotted_owners: std::mem::take(&mut cold.satb_snapshotted_owners),
            string_preimage_addrs: std::mem::take(&mut cold.satb_string_preimage_addrs),
            region_stats: stats,
            roots,
        }
    }

    fn restore(
        self,
        hot: &mut MutatorGcState,
        cold: &mut SatbMutatorState,
    ) -> (Vec<MarkWord>, RegionStats) {
        hot.allocated_count = self.allocated_count;
        hot.memory_use_counts = self.memory_use_counts;
        hot.bytes_since_gc = self.bytes_since_gc;
        hot.bytes_banked_at_resets = self.bytes_banked_at_resets;
        hot.pacing = self.pacing;
        hot.remset = self.remset.into_iter().map(MarkWord::value).collect();
        hot.r_mapped_seen = self.mapped_seen;
        hot.black_born = self
            .black_headers
            .into_iter()
            .map(|addr| addr as *mut _)
            .collect();
        hot.black_born_regions = self.black_regions;
        hot.major_symbol_preimages = self.symbol_preimages;
        hot.major_cons_writes = self.cons_writes.into_iter().map(MarkWord::value).collect();
        // Do not restore a stale direct-mapped reject cache.
        hot.remembered_cache.fill(0);
        cold.satb_snapshotted_owners = self.snapshotted_owners;
        cold.satb_string_preimage_addrs = self.string_preimage_addrs;
        (self.roots, self.region_stats)
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
        })
    }

    pub(in crate::tagged::gc) fn validate_publication(&self) -> Result<(), ImageError> {
        if self.catalog_region.is_some() || !self.cons_tails.is_empty() {
            return Err(ImageError::RegionsOpen);
        }
        validate_closed(&self.hot, &self.cold, &self.jit, &self.regions)
    }

    pub(in crate::tagged::gc) fn publish(&mut self) -> Result<PublishedGcImage, ImageError> {
        self.validate_publication()?;
        let young = self.cold.young.take_owner_payload()?;
        let roots = std::mem::take(&mut self.root_scratch);
        let stats = std::mem::take(&mut self.region_stats);
        Ok(PublishedGcImage {
            facts: Facts::take(&mut self.hot, &mut self.cold, roots, stats),
            young,
        })
    }

    pub(in crate::tagged::gc) fn restore(
        &mut self,
        image: PublishedGcImage,
    ) -> Result<(), RestoreImageFailure> {
        let result =
            validate_closed(&self.hot, &self.cold, &self.jit, &self.regions).and_then(|()| {
                if empty_destination(&self.hot, &self.cold, &self.root_scratch) {
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
        (self.root_scratch, self.region_stats) = facts.restore(&mut self.hot, &mut self.cold);
        Ok(())
    }
}

impl PublishedGcImage {
    #[cfg(test)]
    pub(in crate::tagged::gc) fn seed_collection_words_for_test(
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
    #[cfg(test)]
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

    /// Visit the encoded-word families without consuming or clearing a log.
    /// Address/region/symbol families are separate `visit_work` variants and
    /// have separate summary counts; their raw addresses are not tagged words.
    pub(in crate::tagged::gc) fn visit_logs(&self, mut visitor: impl FnMut(MarkWord)) {
        for &word in self
            .facts
            .remset
            .iter()
            .chain(&self.facts.cons_writes)
            .chain(&self.facts.roots)
        {
            visitor(word);
        }
    }

    /// Complete read-only work census, including families that cannot be
    /// represented as MarkWord. This does not perform a collector fold.
    pub(in crate::tagged::gc) fn visit_work(&self, mut visitor: impl FnMut(PublishedGcWork<'_>)) {
        let f = &self.facts;
        for &word in &f.remset {
            visitor(PublishedGcWork::Remembered(word));
        }
        for &word in &f.cons_writes {
            visitor(PublishedGcWork::ConsWrite(word));
        }
        for &word in &f.roots {
            visitor(PublishedGcWork::Root(word));
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

    pub(in crate::tagged::gc) fn from_legacy(heap: &mut TaggedHeap) -> Result<Self, ImageError> {
        validate_closed(
            heap.current_mutator_gc(),
            heap.current_mutator_satb(),
            &heap.jit,
            &heap.region_book,
        )?;
        let young = heap.current_mutator_satb_mut().young.take_owner_payload()?;
        let stats = std::mem::take(&mut heap.region_stats);
        let mut hot = std::mem::replace(heap.current_mutator_gc_mut(), MutatorGcState::new());
        Ok(Self {
            facts: Facts::take(&mut hot, heap.current_mutator_satb_mut(), Vec::new(), stats),
            young,
        })
    }

    pub(in crate::tagged::gc) fn restore_legacy(
        self,
        heap: &mut TaggedHeap,
    ) -> Result<(), RestoreImageFailure> {
        let result = validate_closed(
            heap.current_mutator_gc(),
            heap.current_mutator_satb(),
            &heap.jit,
            &heap.region_book,
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
        let (_, stats) = facts.restore(&mut hot, heap.current_mutator_satb_mut());
        *heap.current_mutator_gc_mut() = hot;
        heap.region_stats = stats;
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

    pub(in crate::tagged::gc) fn take_root_scratch(&mut self) -> Vec<MarkWord> {
        std::mem::take(&mut self.facts.roots)
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

//! Buffer-owned indexes for live overlays.
//!
//! `OverlayIndex` is the single mutation boundary for overlay membership,
//! endpoint lookup, and interval queries.  Lisp object state remains owned by
//! `OverlayList`; this module owns the structural invariants needed to find
//! those objects efficiently.
//!
//! | Knob | Default | Effect |
//! | --- | --- | --- |
//! | `NEOVM_OVERLAY_LOCAL_MOVE` | off | Update neighbor-preserving B+ records in place and reinsert every start-changing move in the GNU topology mirror using its authoritative successor. |

use std::cell::Cell;
#[cfg(test)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};

use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use super::overlay_bplus::{
    OrderedFilterMask, OrderedFilteredRecord, OrderedRecordRelocation, OrderedShiftRecord,
    OrderedShiftTree, OrderedTreeMatches, OrderedTreeQuery,
};
use super::overlay_order::GnuOverlayOrder;
use neovm_value::heap_types::OverlayData;
pub(crate) use neovm_value::heap_types::overlay_storage::{
    EndpointKind, OverlayBatchOrder, OverlayIdentity, OverlayPositionHandle,
};
use neovm_value::heap_types::overlay_storage::{
    IntervalBPlusQuery, IntervalBPlusTree, IntervalDeletionQuery, IntervalIntersectionQuery,
};
use neovm_value::plist;
use neovm_value::value::Value;

use super::position::{EmacsByteDelta, EmacsByteLen, EmacsBytePos, EmacsByteRange};

/// Process configuration only, published by `OnceLock`; it contains no Lisp
/// state and is safe for independent buffer-owning mutators to read.
#[inline]
fn local_overlay_moves_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NEOVM_OVERLAY_LOCAL_MOVE").as_deref() == Ok("on"))
}

/// A text mutation expressed in the coordinate space owned by the overlay
/// index.  Keeping insertion and deletion distinct makes their endpoint
/// gravity rules exhaustive at the mutation boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverlayTextEdit {
    Insert {
        position: EmacsBytePos,
        length: EmacsByteLen,
        before_markers: bool,
    },
    Delete {
        range: EmacsByteRange,
    },
}

/// Whether a text edit re-ranked any overlay through GNU's front-advance
/// reinsertion.  That path reads the red-black topology mirror, which a
/// batch-built observer copy does not share, so only an order-preserving
/// edit can be replayed on such a copy with the same result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverlayEditOrder {
    Preserved,
    Reinserted,
}

/// Object-side writes and ordering consequence of one text edit.
#[derive(Debug)]
pub(super) struct OverlayTextEditOutcome {
    pub(super) effects: Vec<OverlayEditEffect>,
    pub(super) order: OverlayEditOrder,
}

impl OverlayTextEditOutcome {
    fn preserved(effects: Vec<OverlayEditEffect>) -> Self {
        Self {
            effects,
            order: OverlayEditOrder::Preserved,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverlayEditEffect {
    Resized {
        overlay: Value,
        range: EmacsByteRange,
    },
    Evaporated {
        overlay: Value,
        collapsed_at: EmacsBytePos,
    },
}

/// Structural consequence of changing one indexed overlay region.
///
/// GNU's itree preserves tree position when only the end changes, but removes
/// and reinserts a node when its start changes.  Encoding that distinction as
/// an exhaustive enum prevents callers from accidentally routing both cases
/// through the same ordering operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IndexedRegionChange {
    Unchanged,
    EndOnly,
    StartChanged,
}

impl IndexedRegionChange {
    fn between(old: EmacsByteRange, new: EmacsByteRange) -> Self {
        if old == new {
            Self::Unchanged
        } else if old.start() == new.start() {
            Self::EndOnly
        } else {
            Self::StartChanged
        }
    }
}

/// Whether an insertion changes GNU's structural ordering for an overlay.
///
/// This is intentionally separate from endpoint movement: several overlays
/// have their end changed in place, while only the exact-start,
/// front-advancing subset is removed and reinserted by GNU's itree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertionOrderChange {
    PreservePlace,
    ReinsertFromPreorderStack,
}

impl InsertionOrderChange {
    fn for_overlay(
        range: EmacsByteRange,
        position: EmacsBytePos,
        before_markers: bool,
        front_advance: bool,
        rear_advance: bool,
    ) -> Self {
        if !before_markers
            && range.start() == position
            && front_advance
            && (!range.is_empty() || rear_advance)
        {
            Self::ReinsertFromPreorderStack
        } else {
            Self::PreservePlace
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextEditReattachment {
    PreservePlace { attachment_order: u64 },
    Reinsert,
}

/// Conservative endpoint-tree filter for a property-aware overlay sweep.
///
/// The signature is derived generically from Lisp plist keys: the index does
/// not know which property the caller is resolving. Hash collisions only
/// retain extra endpoints; they can never hide a carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayPropertyFilter(Option<OrderedFilterMask>);

impl OverlayPropertyFilter {
    pub const fn unfiltered() -> Self {
        Self(None)
    }

    pub fn for_properties(properties: impl IntoIterator<Item = Value>) -> Self {
        let mask = properties
            .into_iter()
            .fold(OrderedFilterMask::EMPTY, |mask, property| {
                mask.with_bit(overlay_property_signature_bit(property))
            });
        Self(Some(mask))
    }

    fn record_may_match(self, record_mask: OrderedFilterMask) -> bool {
        self.0.is_none_or(|filter| record_mask.intersects(filter))
    }

    fn subtree_may_match(self, subtree_mask: OrderedFilterMask) -> bool {
        self.record_may_match(subtree_mask)
    }
}

fn overlay_property_signature_bit_raw(bits: usize) -> usize {
    // SplitMix64 finalizer: symbol ids are densely allocated and their tagged
    // representation has low zero bits, so using the raw low byte would create
    // systematic collisions rather than a conservative bloom signature.
    let mut value = bits as u64;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((value ^ (value >> 31)) & 0xff) as usize
}

fn overlay_property_signature_bit(property: Value) -> usize {
    overlay_property_signature_bit_raw(property.bits())
}

fn filter_mask_for_direct_keys(keys: &[usize]) -> OrderedFilterMask {
    let mut mask = OrderedFilterMask::EMPTY;
    for &bits in keys {
        mask = mask.with_bit(overlay_property_signature_bit_raw(bits));
    }
    mask
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct EndpointIdentity {
    overlay: OverlayIdentity,
    kind: EndpointKind,
}

impl EndpointIdentity {
    fn of(overlay: Value, kind: EndpointKind) -> Self {
        Self {
            overlay: OverlayIdentity::of(overlay),
            kind,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct EndpointKey {
    position: EmacsBytePos,
    order: u64,
}
#[derive(Clone, Copy, Debug)]
struct EndpointRecord {
    key: EndpointKey,
    identity: EndpointIdentity,
    overlay: Value,
    kind: EndpointKind,
    property_mask: OrderedFilterMask,
}

impl OrderedShiftRecord for EndpointRecord {
    type Identity = EndpointIdentity;
    type Key = EndpointKey;

    fn identity(self) -> Self::Identity {
        self.identity
    }

    fn key(self) -> Self::Key {
        self.key
    }

    fn key_position(self) -> EmacsBytePos {
        self.key.position
    }

    fn end_position(self) -> EmacsBytePos {
        self.key.position
    }

    fn filter_mask(self) -> OrderedFilterMask {
        self.property_mask
    }

    fn shifted(mut self, delta: EmacsByteDelta) -> Self {
        self.key.position = delta.apply_to_pos(self.key.position);
        self
    }

    fn shifted_key(mut key: Self::Key, delta: EmacsByteDelta) -> Self::Key {
        key.position = delta.apply_to_pos(key.position);
        key
    }
}

impl OrderedFilteredRecord for EndpointRecord {
    fn with_filter_mask(self, property_mask: OrderedFilterMask) -> Self {
        Self {
            property_mask,
            ..self
        }
    }
}

/// Numeric pre-write keys, consumed by the same index after a plist write.
/// No Lisp values or old plist roots escape the exclusive owner operation.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
#[derive(Debug)]
pub(super) struct OverlayPropertyKeys {
    overlay: OverlayIdentity,
    // Clone only the already-minted weak handle. It keeps its Arc control
    // identity from reuse without retaining the buffer or any Lisp owner.
    handle: OverlayPositionHandle,
    attachment_order: u64,
    keys: SmallVec<[usize; 8]>,
}

// Count actual plist-loop work, including repeated walks of the same owners.
// Distinct-owner capture counts cannot reveal these redundant traversals.
#[cfg(test)]
static DIRECT_PROPERTY_KEY_PLIST_STEPS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static PROPERTY_MASK_PLIST_STEPS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(super) fn reset_property_index_plist_steps() {
    DIRECT_PROPERTY_KEY_PLIST_STEPS.store(0, std::sync::atomic::Ordering::Relaxed);
    PROPERTY_MASK_PLIST_STEPS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(super) fn property_index_plist_steps() -> (usize, usize) {
    (
        DIRECT_PROPERTY_KEY_PLIST_STEPS.load(std::sync::atomic::Ordering::Relaxed),
        PROPERTY_MASK_PLIST_STEPS.load(std::sync::atomic::Ordering::Relaxed),
    )
}

// Per-test-process, single-target probe for real owner queries. Snapshot
// mirrors have distinct index addresses and must not consume the live budget.
#[cfg(test)]
static OWNER_QUERY_TARGET: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(test)]
static OWNER_QUERY_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(super) fn start_owner_query_count_for_test(index: &OverlayIndex) {
    use std::sync::atomic::Ordering;
    OWNER_QUERY_COUNT.store(0, Ordering::Relaxed);
    OWNER_QUERY_TARGET.store(index as *const OverlayIndex as usize, Ordering::Relaxed);
}

#[cfg(test)]
pub(super) fn finish_owner_query_count_for_test(index: &OverlayIndex) -> usize {
    use std::sync::atomic::Ordering;
    assert_eq!(
        OWNER_QUERY_TARGET.swap(0, Ordering::Relaxed),
        index as *const OverlayIndex as usize,
        "owner query probe must finish on its selected, unmoved index"
    );
    OWNER_QUERY_COUNT.load(Ordering::Relaxed)
}

#[cfg(test)]
fn record_owner_query_for_test(index: &OverlayIndex) {
    use std::sync::atomic::Ordering;
    if OWNER_QUERY_TARGET.load(Ordering::Relaxed) == index as *const OverlayIndex as usize {
        OWNER_QUERY_COUNT.fetch_add(1, Ordering::Relaxed);
    }
}

fn overlay_direct_property_keys(overlay: Value) -> SmallVec<[usize; 8]> {
    let mut keys = SmallVec::new();
    let Some(data) = overlay.as_overlay_data() else {
        return keys;
    };
    let mut tail = data.plist;
    while tail.is_cons() {
        #[cfg(test)]
        DIRECT_PROPERTY_KEY_PLIST_STEPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let property = tail.cons_car();
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            break;
        }
        // plist_get / overlay_plist_put use eq_value: exact tagged bits.
        // Count duplicate pairs too, so attachment and removal balance.
        keys.push(property.bits());
        tail = rest.cons_cdr();
    }
    keys
}

#[cfg(any(test, feature = "test-support"))]
fn overlay_indexed_property_mask(overlay: Value) -> OrderedFilterMask {
    let Some(data) = overlay.as_overlay_data() else {
        return OrderedFilterMask::EMPTY;
    };
    let mut mask = OrderedFilterMask::EMPTY;
    let mut tail = data.plist;
    while tail.is_cons() {
        #[cfg(test)]
        PROPERTY_MASK_PLIST_STEPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let property = tail.cons_car();
        let values = tail.cons_cdr();
        if !values.is_cons() {
            break;
        }
        mask = mask.with_bit(overlay_property_signature_bit(property));
        tail = values.cons_cdr();
    }
    mask
}

#[derive(Clone, Copy)]
struct DirectEndpointRangeQuery {
    bounds: EmacsByteRange,
    property_filter: OverlayPropertyFilter,
}

impl OrderedTreeQuery<EndpointRecord> for DirectEndpointRangeQuery {
    #[inline]
    fn subtree_may_match(
        self,
        minimum: EmacsBytePos,
        maximum: EmacsBytePos,
        _maximum_end: EmacsBytePos,
    ) -> bool {
        minimum < self.bounds.end() && maximum > self.bounds.start()
    }

    #[inline]
    fn record_matches(self, record: EndpointRecord) -> bool {
        self.bounds.start() < record.key.position
            && record.key.position < self.bounds.end()
            && self.property_filter.record_may_match(record.property_mask)
    }

    #[inline]
    fn subtree_filter_may_match(self, filter_mask: OrderedFilterMask) -> bool {
        self.property_filter.subtree_may_match(filter_mask)
    }

    #[inline]
    fn maximum_end_is_too_small(self, maximum_end: EmacsBytePos) -> bool {
        maximum_end <= self.bounds.start()
    }

    #[inline]
    fn minimum_start_is_too_large(self, minimum_start: EmacsBytePos) -> bool {
        minimum_start >= self.bounds.end()
    }

    #[cfg(test)]
    fn record_subtree_visit(self) {
        super::overlay::record_endpoint_search_node_visit();
    }
}

#[derive(Clone, Copy)]
struct LiveEndpointRangeQuery {
    bounds: EmacsByteRange,
    property_filter: OverlayPropertyFilter,
    after_key: Option<EndpointKey>,
    before_key: Option<EndpointKey>,
}

impl OrderedTreeQuery<EndpointRecord> for LiveEndpointRangeQuery {
    fn subtree_may_match(
        self,
        minimum: EmacsBytePos,
        maximum: EmacsBytePos,
        _maximum_end: EmacsBytePos,
    ) -> bool {
        minimum < self.bounds.end()
            && maximum > self.bounds.start()
            && self.after_key.is_none_or(|after| maximum >= after.position)
            && self
                .before_key
                .is_none_or(|before| minimum <= before.position)
    }

    fn record_matches(self, record: EndpointRecord) -> bool {
        self.bounds.start() < record.key.position
            && record.key.position < self.bounds.end()
            && self.after_key.is_none_or(|after| record.key > after)
            && self.before_key.is_none_or(|before| record.key < before)
            && self.property_filter.record_may_match(record.property_mask)
    }

    fn subtree_filter_may_match(self, filter_mask: OrderedFilterMask) -> bool {
        self.property_filter.subtree_may_match(filter_mask)
    }

    fn maximum_end_is_too_small(self, maximum_end: EmacsBytePos) -> bool {
        maximum_end <= self.bounds.start()
            || self
                .after_key
                .is_some_and(|after| maximum_end < after.position)
    }

    fn minimum_start_is_too_large(self, minimum_start: EmacsBytePos) -> bool {
        minimum_start >= self.bounds.end()
            || self
                .before_key
                .is_some_and(|before| minimum_start > before.position)
    }

    #[cfg(test)]
    fn record_subtree_visit(self) {
        super::overlay::record_endpoint_search_node_visit();
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct OverlayEndpoint {
    pub(super) position: EmacsBytePos,
    pub(super) overlay: Value,
    pub(super) kind: EndpointKind,
}

const ENDPOINT_CHUNK_SIZE: usize = 64;

#[derive(Clone, Copy)]
enum EndpointScanDirection {
    Forward,
    Reverse,
}

/// Guard-free chunked endpoint stream for `LiveOverlayView`. Each chunk takes
/// a short `core.read()` guard, checks the generation stamp, copies up to
/// `ENDPOINT_CHUNK_SIZE` matching endpoints, and releases the guard before
/// yielding. No `RwLockReadGuard` ever outlives a call.
struct LiveEndpointRecords<'a> {
    intervals: &'a Arc<RwLock<IntervalBPlusTree>>,
    core: &'a Arc<RwLock<OverlayIndexCore>>,
    #[cfg(test)]
    cold_intervals_probe: &'a ColdIntervalsLockProbe,
    stale: &'a Cell<bool>,
    stamp: u64,
    bounds: EmacsByteRange,
    property_filter: OverlayPropertyFilter,
    direction: EndpointScanDirection,
    cursor_key: Option<EndpointKey>,
    buffer: SmallVec<[(OverlayEndpoint, EndpointKey); ENDPOINT_CHUNK_SIZE]>,
    index: usize,
    exhausted: bool,
}

impl<'a> LiveEndpointRecords<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        intervals: &'a Arc<RwLock<IntervalBPlusTree>>,
        core: &'a Arc<RwLock<OverlayIndexCore>>,
        #[cfg(test)] cold_intervals_probe: &'a ColdIntervalsLockProbe,
        stale: &'a Cell<bool>,
        stamp: u64,
        bounds: EmacsByteRange,
        property_filter: OverlayPropertyFilter,
        direction: EndpointScanDirection,
    ) -> Self {
        let exhausted = !publish_endpoints_for(
            intervals,
            core,
            Some((stale, stamp)),
            #[cfg(test)]
            Some(cold_intervals_probe),
        );
        Self {
            intervals,
            core,
            #[cfg(test)]
            cold_intervals_probe,
            stale,
            stamp,
            bounds,
            property_filter,
            direction,
            cursor_key: None,
            buffer: SmallVec::new(),
            index: 0,
            exhausted,
        }
    }

    fn refill(&mut self) {
        if self.exhausted {
            return;
        }
        if self.stale.get() {
            self.exhausted = true;
            return;
        }
        if !publish_endpoints_for(
            self.intervals,
            self.core,
            Some((self.stale, self.stamp)),
            #[cfg(test)]
            Some(self.cold_intervals_probe),
        ) {
            self.exhausted = true;
            return;
        }
        let core = self.core.read();
        if core.generation != self.stamp {
            self.stale.set(true);
            self.exhausted = true;
            return;
        }
        let Some(endpoints) = core.endpoints.get() else {
            self.exhausted = true;
            return;
        };
        let query = match self.direction {
            EndpointScanDirection::Forward => LiveEndpointRangeQuery {
                bounds: self.bounds,
                property_filter: self.property_filter,
                after_key: self.cursor_key,
                before_key: None,
            },
            EndpointScanDirection::Reverse => LiveEndpointRangeQuery {
                bounds: self.bounds,
                property_filter: self.property_filter,
                after_key: None,
                before_key: self.cursor_key,
            },
        };
        self.buffer.clear();
        self.index = 0;
        let map_record = |record: EndpointRecord| {
            (
                OverlayEndpoint {
                    position: record.key.position,
                    overlay: record.overlay,
                    kind: record.kind,
                },
                record.key,
            )
        };
        match self.direction {
            EndpointScanDirection::Forward => {
                for record in endpoints.records.matches(query).take(ENDPOINT_CHUNK_SIZE) {
                    self.buffer.push(map_record(record));
                }
            }
            EndpointScanDirection::Reverse => {
                for record in endpoints
                    .records
                    .matches_reverse(query)
                    .take(ENDPOINT_CHUNK_SIZE)
                {
                    self.buffer.push(map_record(record));
                }
            }
        }
        drop(core);
        super::overlay::note_overlay_live_records_visited(self.buffer.len());
        if self.buffer.len() < ENDPOINT_CHUNK_SIZE {
            self.exhausted = true;
        } else {
            self.cursor_key = self.buffer.last().map(|&(_, key)| key);
        }
    }

    #[inline]
    fn next_item(&mut self) -> Option<OverlayEndpoint> {
        if self.index >= self.buffer.len() {
            self.refill();
            if self.index >= self.buffer.len() {
                return None;
            }
        }
        let item = self.buffer[self.index].0;
        self.index += 1;
        Some(item)
    }
}

/// Endpoint cursor shared by `OverlayList` (single-pass direct B+ tree
/// traversal under one mapped `core.read()` guard) and `LiveOverlayView`
/// (guard-free chunked stream with per-chunk generation checks).
pub(super) struct OverlayEndpointRecords<'a>(OverlayEndpointRecordsInner<'a>);

enum OverlayEndpointRecordsInner<'a> {
    Direct(
        OrderedTreeMatches<
            MappedRwLockReadGuard<'a, OrderedShiftTree<EndpointRecord>>,
            EndpointRecord,
            DirectEndpointRangeQuery,
        >,
    ),
    Live(Box<LiveEndpointRecords<'a>>),
    Empty,
}

impl OverlayEndpointRecords<'_> {
    #[inline]
    pub(super) fn empty() -> Self {
        Self(OverlayEndpointRecordsInner::Empty)
    }
}

impl Iterator for OverlayEndpointRecords<'_> {
    type Item = OverlayEndpoint;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.0 {
            OverlayEndpointRecordsInner::Direct(records) => {
                records.next().map(|record| OverlayEndpoint {
                    position: record.key.position,
                    overlay: record.overlay,
                    kind: record.kind,
                })
            }
            OverlayEndpointRecordsInner::Live(live) => live.next_item(),
            OverlayEndpointRecordsInner::Empty => None,
        }
    }
}

#[derive(Clone, Debug)]
struct EndpointBPlusTree {
    records: OrderedShiftTree<EndpointRecord>,
    next_order: u64,
}

impl EndpointBPlusTree {
    fn from_entries(
        entries: &[(Value, EndpointKind, EmacsBytePos)],
        property_masks: &FxHashMap<OverlayIdentity, OrderedFilterMask>,
    ) -> Self {
        let records = entries
            .iter()
            .enumerate()
            .map(|(order, (overlay, kind, position))| {
                let property_mask = *property_masks
                    .get(&OverlayIdentity::of(*overlay))
                    .expect("attached overlay missing cached property mask");
                EndpointRecord {
                    key: EndpointKey {
                        position: *position,
                        order: order as u64,
                    },
                    identity: EndpointIdentity::of(*overlay, *kind),
                    overlay: *overlay,
                    kind: *kind,
                    property_mask,
                }
            })
            .collect();
        Self {
            records: OrderedShiftTree::from_records(records),
            next_order: entries.len() as u64,
        }
    }

    fn insert(
        &mut self,
        position: EmacsBytePos,
        kind: EndpointKind,
        overlay: Value,
        property_mask: OrderedFilterMask,
    ) -> bool {
        let identity = EndpointIdentity::of(overlay, kind);
        let order = self.next_order;
        self.next_order = self
            .next_order
            .checked_add(1)
            .expect("overlay endpoint order exhausted");
        self.records.insert(EndpointRecord {
            key: EndpointKey { position, order },
            identity,
            overlay,
            kind,
            property_mask,
        })
    }

    fn remove(&mut self, overlay: Value, kind: EndpointKind) -> Option<EmacsBytePos> {
        self.records
            .remove(EndpointIdentity::of(overlay, kind))
            .map(|record| record.key.position)
    }

    #[cfg(any(test, feature = "test-support"))]
    fn replace_overlay_property_mask(&mut self, overlay: Value, property_mask: OrderedFilterMask) {
        for kind in [EndpointKind::Start, EndpointKind::End] {
            // The signature conservatively tracks plist key classes,
            // independent of their values. Changing a value or adding a
            // colliding key leaves it unchanged: one lookup per endpoint.
            // New diagnostic overlays gain classes as properties are added.
            // Republish only the filter unions above those endpoints; their
            // positions and pending lazy shifts need no summary rebuild.
            let _previous = self
                .records
                .replace_filter_mask(EndpointIdentity::of(overlay, kind), property_mask);
        }
    }

    fn add_overlay_property_key(&mut self, overlay: Value, property: Value) {
        let added = OrderedFilterMask::EMPTY.with_bit(overlay_property_signature_bit(property));
        for kind in [EndpointKind::Start, EndpointKind::End] {
            self.records
                .grow_filter_mask(EndpointIdentity::of(overlay, kind), added);
        }
    }

    fn next_after(&self, position: EmacsBytePos, limit: EmacsBytePos) -> Option<EmacsBytePos> {
        self.records.next_position_after(position, limit)
    }

    fn next_after_filtered(
        &self,
        position: EmacsBytePos,
        limit: EmacsBytePos,
        property_filter: OverlayPropertyFilter,
    ) -> Option<EmacsBytePos> {
        if position >= limit {
            return None;
        }
        // `DirectEndpointRangeQuery` matches strictly inside `(start, end)`.
        // Extend `end` by 1 so endpoints at `limit` are included, matching
        // `next_after`.
        let exclusive_end = EmacsBytePos::new(limit.get().saturating_add(1));
        let bounds = EmacsByteRange::new(position, exclusive_end);
        self.records
            .matches(DirectEndpointRangeQuery {
                bounds,
                property_filter,
            })
            .next()
            .map(|record| record.key.position)
    }

    fn previous_before(&self, position: EmacsBytePos, limit: EmacsBytePos) -> Option<EmacsBytePos> {
        self.records.previous_position_before(position, limit)
    }

    fn shift_at_or_after(
        &mut self,
        position: EmacsBytePos,
        inclusive: bool,
        delta: EmacsByteDelta,
    ) {
        self.records.shift_at_or_after(position, inclusive, delta);
    }

    #[cfg(any(test, feature = "test-support"))]
    fn assert_invariants(&self) {
        self.records.assert_invariants();
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn len(&self) -> usize {
        self.records.len()
    }
}

fn set_overlay_position_handle(overlay: Value, intervals: &Arc<RwLock<IntervalBPlusTree>>) {
    let handle = OverlayPositionHandle::new(overlay, intervals);
    let _ = overlay.with_overlay_data_mut(|data| {
        *data.position_handle_mut() = Some(handle);
    });
}

fn materialize_overlay_position(overlay: Value, range: EmacsByteRange) {
    let _ = overlay.with_overlay_data_mut(|data| {
        data.start = range.start().get();
        data.end = range.end().get();
        *data.position_handle_mut() = None;
    });
}

/// Shared read/write state beside `intervals`: lazily published endpoint tree
/// (with its property masks), exact direct-property key counts, and the write
/// generation.
///
/// Lock order: whenever both locks are held, acquire `intervals` before
/// `core`. Never hold `core` while acquiring `intervals` on any writer or
/// `LiveOverlayIndex` path.
///
/// Every core writer updates endpoints/keys and advances `generation` inside
/// one `write_core_lock` critical section, after any `intervals` edit has
/// finished, so a reader cannot pair a new generation with pre-edit state.
///
/// Parallel-VM / E-spine preconditions:
/// 1. `write_core_lock` is atomic for `core` only; interval-tree edits in
///    `attach`, `remap`, `adjust_for_insert`, and `adjust_for_delete` run in
///    separate `intervals.write()` sections before the core bump. Today
///    `Value` is thread-confined (`!Send + !Sync`) and `LiveOverlayView` holds
///    `Cell<bool>`, so reader and writer are strictly serialized on the
///    evaluator thread and cannot observe a mid-edit interval tree under the
///    old stamp. Before any cross-thread reader of `LiveOverlayIndex` is
///    introduced, writers must either hold `intervals.write()` across the core
///    bump (as `move_to` already does) or protect interval+core edits with an
///    odd/even seqlock generation.
/// 2. `OverlayEndpointRecords::Direct` (used only by the default `&OverlayList`
///    path, never by `LiveOverlayView`) holds a mapped `core.read()` guard for
///    the lifetime of the endpoint iterator while the consumer's
///    `apply_endpoint -> overlay_range` takes `intervals.read()` (read-read
///    `core -> intervals` nesting). Today this is single-threaded and borrowck
///    forbids same-thread writers while `&OverlayList` is borrowed; before any
///    cross-thread writer can contend on `intervals.write()` during a `Direct`
///    sweep, `Direct` must either stop holding `core.read()` across yields
///    (like `LiveEndpointRecords`) or store the overlay range on
///    `EndpointRecord` so `apply_endpoint` does not acquire `intervals.read()`.
#[derive(Debug)]
struct OverlayIndexCore {
    /// Published lazily. The property masks consulted by
    /// [`OverlayPropertyFilter`] live on these records.
    endpoints: OnceLock<EndpointBPlusTree>,
    // Exact direct-key pair counts, maintained by this index's owner.
    // Keys are eq identities, not rooted Lisp values or alias/category meaning.
    direct_property_keys: FxHashMap<usize, usize>,
    /// Conservative per-overlay property signature masks for attached overlays,
    /// maintained alongside `direct_property_keys` at mutation time so lazy
    /// endpoint publication reads cached masks without walking Lisp plists.
    property_masks: FxHashMap<OverlayIdentity, OrderedFilterMask>,
    generation: u64,
}

impl OverlayIndexCore {
    fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
    }

    fn add_property_keys(&mut self, keys: &[usize]) -> bool {
        let mut distinct_changed = false;
        for key in keys {
            let entry = self.direct_property_keys.entry(*key).or_default();
            if *entry == 0 {
                distinct_changed = true;
            }
            *entry += 1;
        }
        distinct_changed
    }

    fn remove_property_keys(&mut self, keys: &[usize]) -> bool {
        let mut distinct_changed = false;
        for key in keys {
            let count = self
                .direct_property_keys
                .get_mut(key)
                .expect("attached overlay property key was not counted");
            *count -= 1;
            if *count == 0 {
                self.direct_property_keys.remove(key);
                distinct_changed = true;
            }
        }
        distinct_changed
    }
}

/// Run `edit` on `core` and advance `generation` before releasing the write
/// guard, keeping every core mutation and its generation bump in one critical
/// section.
fn write_core_lock<R>(
    core_lock: &RwLock<OverlayIndexCore>,
    edit: impl FnOnce(&mut OverlayIndexCore) -> R,
) -> R {
    let mut core = core_lock.write();
    let result = edit(&mut core);
    core.bump_generation();
    result
}

/// Deterministic test-only lock-order probe for cold-path `intervals.read()`
/// acquisitions.
///
/// When armed on a single-threaded test index, every cold-path acquisition of
/// `intervals.read()` records itself and checks `core.try_write().is_none()`
/// at the exact acquisition point, proving deterministically that `core` was
/// dropped beforehand (`intervals -> core` lock order) without helper-thread
/// timing races.
#[cfg(test)]
#[derive(Debug, Default)]
struct ColdIntervalsLockProbe {
    armed: AtomicBool,
    intervals_reads: AtomicUsize,
    saw_core_held: AtomicBool,
}

#[cfg(test)]
impl ColdIntervalsLockProbe {
    fn arm(&self) {
        self.intervals_reads.store(0, Ordering::Relaxed);
        self.saw_core_held.store(false, Ordering::Relaxed);
        self.armed.store(true, Ordering::Release);
    }

    fn disarm(&self) {
        self.armed.store(false, Ordering::Release);
    }

    #[inline]
    fn record_intervals_read(&self, core_lock: &RwLock<OverlayIndexCore>) {
        if !self.armed.load(Ordering::Acquire) {
            return;
        }
        self.intervals_reads.fetch_add(1, Ordering::Relaxed);
        if core_lock.try_write().is_none() {
            self.saw_core_held.store(true, Ordering::Relaxed);
        }
    }

    fn snapshot(&self) -> (usize, bool) {
        (
            self.intervals_reads.load(Ordering::Acquire),
            self.saw_core_held.load(Ordering::Acquire),
        )
    }
}

/// Publish the lazy endpoint index if absent. For live views (`live = Some`),
/// checks the generation stamp before and after building; returns `false` and
/// latches `stale` if the index changed, without publishing stale state.
///
/// Lock order: never holds `core` while acquiring `intervals`.
fn publish_endpoints_for(
    intervals_lock: &RwLock<IntervalBPlusTree>,
    core_lock: &RwLock<OverlayIndexCore>,
    live: Option<(&Cell<bool>, u64)>,
    #[cfg(test)] probe: Option<&ColdIntervalsLockProbe>,
) -> bool {
    {
        let core = core_lock.read();
        if let Some((stale, stamp)) = live
            && core.generation != stamp
        {
            stale.set(true);
            return false;
        }
        if core.endpoints.get().is_some() {
            return true;
        }
    }
    #[cfg(test)]
    super::overlay::record_endpoint_publication_interval_read();
    let built = {
        // Lock order: acquire `intervals` before `core`, capture the attached
        // endpoints, then drop `intervals` while keeping `core.read()` held so
        // `core.property_masks` stays coherent with `entries` without walking
        // Lisp plists inside an observed read capture.
        #[cfg(test)]
        if let Some(probe) = probe {
            probe.record_intervals_read(core_lock);
        }
        let intervals = intervals_lock.read();
        let core = core_lock.read();
        if let Some((stale, stamp)) = live
            && core.generation != stamp
        {
            stale.set(true);
            return false;
        }
        if core.endpoints.get().is_some() {
            return true;
        }
        let entries = intervals.endpoint_entries_in_attachment_order();
        drop(intervals);
        neovm_value::tagged::collection_reads::assert_no_observed_reads(|| {
            EndpointBPlusTree::from_entries(&entries, &core.property_masks)
        })
    };
    let core = core_lock.write();
    if let Some((stale, stamp)) = live
        && core.generation != stamp
    {
        stale.set(true);
        return false;
    }
    let _ = core.endpoints.get_or_init(|| built);
    true
}

/// Shared, generation-stamped read handle onto a live `OverlayIndex`.
///
/// Holds cloned `Arc` handles to `intervals` and `core`, never `&Buffer`.
/// Every read takes a short guard in `intervals -> core` order, checks
/// `core.generation == stamp`, latches `stale` on mismatch, and drops all
/// guards before returning.
#[derive(Clone, Debug)]
pub(super) struct LiveOverlayIndex {
    intervals: Arc<RwLock<IntervalBPlusTree>>,
    core: Arc<RwLock<OverlayIndexCore>>,
    #[cfg(test)]
    cold_intervals_probe: Arc<ColdIntervalsLockProbe>,
    stamp: u64,
}

impl LiveOverlayIndex {
    pub(super) fn stamp(&self) -> u64 {
        self.stamp
    }

    pub(super) fn is_stale(&self, stale: &Cell<bool>) -> bool {
        if stale.get() {
            return true;
        }
        if self.core.read().generation != self.stamp {
            stale.set(true);
            return true;
        }
        false
    }

    /// Acquire `intervals.read()` and verify `core.generation == self.stamp`
    /// while holding it (strict `intervals -> core` lock order). Returns `None`
    /// and latches `stale` on any mismatch.
    #[inline]
    fn lock_intervals_if_fresh<'a>(
        &'a self,
        stale: &Cell<bool>,
    ) -> Option<RwLockReadGuard<'a, IntervalBPlusTree>> {
        if stale.get() {
            return None;
        }
        #[cfg(test)]
        self.cold_intervals_probe.record_intervals_read(&self.core);
        let intervals = self.intervals.read();
        if self.core.read().generation != self.stamp {
            stale.set(true);
            return None;
        }
        Some(intervals)
    }

    pub(super) fn is_empty(&self, stale: &Cell<bool>) -> bool {
        let Some(intervals) = self.lock_intervals_if_fresh(stale) else {
            return true;
        };
        intervals.is_empty()
    }

    pub(super) fn may_contain_property(&self, property: Value, stale: &Cell<bool>) -> bool {
        if stale.get() {
            return false;
        }
        let core = self.core.read();
        if core.generation != self.stamp {
            stale.set(true);
            return false;
        }
        core.direct_property_keys.contains_key(&property.bits())
    }

    pub(super) fn contains(&self, overlay: Value, stale: &Cell<bool>) -> bool {
        let Some(intervals) = self.lock_intervals_if_fresh(stale) else {
            return false;
        };
        intervals.contains(overlay)
    }

    pub(super) fn range(&self, overlay: Value, stale: &Cell<bool>) -> Option<EmacsByteRange> {
        let intervals = self.lock_intervals_if_fresh(stale)?;
        intervals.range_by_identity(OverlayIdentity::of(overlay))
    }

    pub(super) fn overlays_at(&self, pos: EmacsBytePos, stale: &Cell<bool>) -> Vec<Value> {
        let Some(intervals) = self.lock_intervals_if_fresh(stale) else {
            return Vec::new();
        };
        let mut overlays = Vec::new();
        intervals
            .records()
            .for_each_match(IntervalBPlusQuery::Point(pos), |record| {
                overlays.push(record.overlay());
            });
        drop(intervals);
        super::overlay::note_overlay_live_records_visited(overlays.len());
        overlays
    }

    pub(super) fn overlays_at_small(
        &self,
        pos: EmacsBytePos,
        stale: &Cell<bool>,
    ) -> SmallVec<[Value; 8]> {
        let Some(intervals) = self.lock_intervals_if_fresh(stale) else {
            return SmallVec::new();
        };
        let mut overlays = SmallVec::new();
        intervals
            .records()
            .for_each_match(IntervalBPlusQuery::Point(pos), |record| {
                overlays.push(record.overlay());
            });
        drop(intervals);
        super::overlay::note_overlay_live_records_visited(overlays.len());
        overlays
    }

    pub(super) fn overlays_in_region(
        &self,
        range: EmacsByteRange,
        accessible_end: EmacsBytePos,
        stale: &Cell<bool>,
    ) -> Vec<Value> {
        let Some(intervals) = self.lock_intervals_if_fresh(stale) else {
            return Vec::new();
        };
        let mut overlays = Vec::new();
        intervals.records().for_each_match(
            IntervalBPlusQuery::Region {
                range,
                accessible_end,
            },
            |record| overlays.push(record.overlay()),
        );
        drop(intervals);
        super::overlay::note_overlay_live_records_visited(overlays.len());
        overlays
    }

    pub(super) fn overlays_in_region_small(
        &self,
        range: EmacsByteRange,
        accessible_end: EmacsBytePos,
        stale: &Cell<bool>,
    ) -> SmallVec<[Value; 8]> {
        let Some(intervals) = self.lock_intervals_if_fresh(stale) else {
            return SmallVec::new();
        };
        let mut overlays = SmallVec::new();
        intervals.records().for_each_match(
            IntervalBPlusQuery::Region {
                range,
                accessible_end,
            },
            |record| overlays.push(record.overlay()),
        );
        drop(intervals);
        super::overlay::note_overlay_live_records_visited(overlays.len());
        overlays
    }

    pub(super) fn all_ascending(&self, stale: &Cell<bool>) -> Vec<Value> {
        let Some(intervals) = self.lock_intervals_if_fresh(stale) else {
            return Vec::new();
        };
        let mut overlays = Vec::with_capacity(intervals.len());
        intervals.all_ascending(&mut overlays);
        drop(intervals);
        super::overlay::note_overlay_live_records_visited(overlays.len());
        overlays
    }

    pub(super) fn next_boundary_after(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
        stale: &Cell<bool>,
    ) -> Option<EmacsBytePos> {
        if pos >= limit || stale.get() {
            return None;
        }
        {
            let core = self.core.read();
            if core.generation != self.stamp {
                stale.set(true);
                return None;
            }
            if let Some(endpoints) = core.endpoints.get() {
                let next = endpoints.next_after(pos, limit);
                drop(core);
                super::overlay::note_overlay_live_records_visited(usize::from(next.is_some()));
                return next;
            }
        }
        // Drop `core` before acquiring `intervals` (lock order: intervals -> core).
        let intervals = self.lock_intervals_if_fresh(stale)?;
        let mut next = intervals.next_start_after(pos, limit);
        let mut visited = usize::from(next.is_some());
        intervals
            .records()
            .for_each_match(IntervalBPlusQuery::Point(pos), |record| {
                visited += 1;
                let end = record.range().end();
                if end > pos && end <= limit && next.is_none_or(|current| end < current) {
                    next = Some(end);
                }
            });
        drop(intervals);
        super::overlay::note_overlay_live_records_visited(visited);
        next
    }

    pub(super) fn next_boundary_after_matching_no_publish(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
        property_filter: OverlayPropertyFilter,
        stale: &Cell<bool>,
    ) -> Option<EmacsBytePos> {
        if pos >= limit || stale.get() {
            return None;
        }
        {
            let core = self.core.read();
            if core.generation != self.stamp {
                stale.set(true);
                return None;
            }
            if let Some(endpoints) = core.endpoints.get() {
                let next = endpoints.next_after_filtered(pos, limit, property_filter);
                drop(core);
                super::overlay::note_overlay_live_records_visited(usize::from(next.is_some()));
                return next;
            }
        }
        self.next_boundary_after(pos, limit, stale)
    }

    pub(super) fn endpoint_records_strictly_within<'a>(
        &'a self,
        bounds: EmacsByteRange,
        property_filter: OverlayPropertyFilter,
        stale: &'a Cell<bool>,
    ) -> OverlayEndpointRecords<'a> {
        OverlayEndpointRecords(OverlayEndpointRecordsInner::Live(Box::new(
            LiveEndpointRecords::new(
                &self.intervals,
                &self.core,
                #[cfg(test)]
                self.cold_intervals_probe.as_ref(),
                stale,
                self.stamp,
                bounds,
                property_filter,
                EndpointScanDirection::Forward,
            ),
        )))
    }

    pub(super) fn endpoint_records_strictly_within_reverse<'a>(
        &'a self,
        bounds: EmacsByteRange,
        property_filter: OverlayPropertyFilter,
        stale: &'a Cell<bool>,
    ) -> OverlayEndpointRecords<'a> {
        OverlayEndpointRecords(OverlayEndpointRecordsInner::Live(Box::new(
            LiveEndpointRecords::new(
                &self.intervals,
                &self.core,
                #[cfg(test)]
                self.cold_intervals_probe.as_ref(),
                stale,
                self.stamp,
                bounds,
                property_filter,
                EndpointScanDirection::Reverse,
            ),
        )))
    }
}

#[inline(always)]
fn direct_property_key_bloom_bit(bits: usize) -> u64 {
    #[cfg(target_pointer_width = "64")]
    const MULT: usize = 0x9E37_79B9_7F4A_7C15;
    #[cfg(not(target_pointer_width = "64"))]
    const MULT: usize = 0x9E37_79B9;
    let shift = usize::BITS - 6;
    1u64 << ((bits.wrapping_mul(MULT) >> shift) & 63)
}

const DIRECT_PROPERTY_KEYS_INLINE_CAP: usize = 8;

type DirectPropertyKeyCache = (u64, [usize; DIRECT_PROPERTY_KEYS_INLINE_CAP], u8);

fn snapshot_direct_property_key_cache(keys: &FxHashMap<usize, usize>) -> DirectPropertyKeyCache {
    let mut bloom = 0u64;
    let mut inline = [0usize; DIRECT_PROPERTY_KEYS_INLINE_CAP];
    let len = if keys.len() <= DIRECT_PROPERTY_KEYS_INLINE_CAP {
        for (idx, &k) in keys.keys().enumerate() {
            bloom |= direct_property_key_bloom_bit(k);
            inline[idx] = k;
        }
        keys.len() as u8
    } else {
        for &k in keys.keys() {
            bloom |= direct_property_key_bloom_bit(k);
        }
        u8::MAX
    };
    (bloom, inline, len)
}

#[derive(Debug)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(super) struct OverlayIndex {
    intervals: Arc<RwLock<IntervalBPlusTree>>,
    /// Topology-only mirror for the one GNU red-black-tree detail observable
    /// during front-advancing insertion.  Positions remain authoritative in
    /// `intervals`; this field stores only identity and tree shape.
    gnu_order: GnuOverlayOrder<OverlayIdentity>,
    core: Arc<RwLock<OverlayIndexCore>>,
    #[cfg(test)]
    cold_intervals_probe: Arc<ColdIntervalsLockProbe>,
    /// Lock-free owner-side cache of `core.direct_property_keys` so the
    /// default (`OverlayList`) `may_contain_property` path pays no `core.read()`
    /// lock or HashMap probe for absent keys or when <= 8 distinct keys exist.
    direct_property_key_bloom: u64,
    direct_property_keys_inline: [usize; DIRECT_PROPERTY_KEYS_INLINE_CAP],
    direct_property_keys_inline_len: u8,
}

impl OverlayIndex {
    pub(super) fn new() -> Self {
        Self {
            intervals: Arc::new(RwLock::new(IntervalBPlusTree::new())),
            gnu_order: GnuOverlayOrder::new(),
            core: Arc::new(RwLock::new(OverlayIndexCore {
                endpoints: OnceLock::new(),
                direct_property_keys: FxHashMap::default(),
                property_masks: FxHashMap::default(),
                generation: 0,
            })),
            #[cfg(test)]
            cold_intervals_probe: Arc::new(ColdIntervalsLockProbe::default()),
            direct_property_key_bloom: 0,
            direct_property_keys_inline: [0; DIRECT_PROPERTY_KEYS_INLINE_CAP],
            direct_property_keys_inline_len: 0,
        }
    }

    fn apply_direct_property_key_cache(&mut self, cache: DirectPropertyKeyCache) {
        self.direct_property_key_bloom = cache.0;
        self.direct_property_keys_inline = cache.1;
        self.direct_property_keys_inline_len = cache.2;
    }

    fn sync_direct_property_key_cache(&mut self, keys: &FxHashMap<usize, usize>) {
        self.apply_direct_property_key_cache(snapshot_direct_property_key_cache(keys));
    }

    #[inline]
    fn note_direct_property_key_added(&mut self, key: usize) {
        self.direct_property_key_bloom |= direct_property_key_bloom_bit(key);
        let len = self.direct_property_keys_inline_len as usize;
        if len <= DIRECT_PROPERTY_KEYS_INLINE_CAP {
            if self.direct_property_keys_inline[..len].contains(&key) {
                return;
            }
            if len < DIRECT_PROPERTY_KEYS_INLINE_CAP {
                self.direct_property_keys_inline[len] = key;
                self.direct_property_keys_inline_len = (len + 1) as u8;
            } else {
                self.direct_property_keys_inline_len = u8::MAX;
            }
        }
    }

    /// Capture a live, generation-stamped read handle onto this index's shared
    /// `intervals` and `core` without copying overlays.
    pub(super) fn live_handle(&self) -> LiveOverlayIndex {
        let stamp = self.core.read().generation;
        LiveOverlayIndex {
            intervals: Arc::clone(&self.intervals),
            core: Arc::clone(&self.core),
            #[cfg(test)]
            cold_intervals_probe: Arc::clone(&self.cold_intervals_probe),
            stamp,
        }
    }

    /// Monotonic stamp of index writes. A no-op remap does not advance it.
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn generation(&self) -> u64 {
        self.core.read().generation
    }

    fn write_core<R>(&mut self, edit: impl FnOnce(&mut OverlayIndexCore) -> R) -> R {
        write_core_lock(&self.core, edit)
    }

    pub(super) fn bump_generation(&mut self) {
        self.write_core(|_| ());
    }

    #[cfg(test)]
    fn publish_endpoints(&self) {
        let _ = publish_endpoints_for(
            &self.intervals,
            &self.core,
            None,
            Some(self.cold_intervals_probe.as_ref()),
        );
    }

    #[cfg(test)]
    fn is_endpoints_published(&self) -> bool {
        self.core.read().endpoints.get().is_some()
    }

    #[cfg(test)]
    fn with_published_endpoints<R>(&self, f: impl FnOnce(&EndpointBPlusTree) -> R) -> Option<R> {
        let core = self.core.read();
        core.endpoints.get().map(f)
    }

    /// Capture numeric old keys before any plist-head replacement or write.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(super) fn property_keys_before_write(&self, overlay: Value) -> Option<OverlayPropertyKeys> {
        let data = overlay.as_overlay_data()?;
        let handle = data.position_handle().as_ref()?;
        if !handle.belongs_to(overlay, &self.intervals) {
            return None;
        }
        Some(OverlayPropertyKeys {
            overlay: OverlayIdentity::of(overlay),
            handle: handle.clone(),
            attachment_order: handle.attachment_order()?,
            keys: overlay_direct_property_keys(overlay),
        })
    }

    /// Attach a live overlay at `range`.
    ///
    /// Returns `false` without changing the index when the overlay is already
    /// attached.  Keeping all three writes here prevents membership and query
    /// indexes from drifting apart.
    pub(super) fn attach(&mut self, overlay: Value, range: EmacsByteRange) -> bool {
        let mut intervals = self.intervals.write();
        if !intervals.insert(overlay, range) {
            return false;
        }
        let identity = OverlayIdentity::of(overlay);
        let inserted = self.gnu_order.insert_by(identity, |existing| {
            range.start().cmp(
                &intervals
                    .range_by_identity(existing)
                    .expect("GNU order mirror contains an unindexed overlay")
                    .start(),
            )
        });
        assert!(
            inserted.is_ok(),
            "new interval already existed in GNU order mirror"
        );
        drop(intervals);
        // Initialize the fresh attachment before property reads observe it.
        // A later handle write would invalidate an enclosing read capture.
        set_overlay_position_handle(overlay, &self.intervals);
        let keys = overlay_direct_property_keys(overlay);
        let property_mask = filter_mask_for_direct_keys(&keys);
        for &key in &keys {
            self.note_direct_property_key_added(key);
        }
        self.write_core(|core| {
            core.add_property_keys(&keys);
            core.property_masks.insert(identity, property_mask);
            if let Some(endpoints) = core.endpoints.get_mut() {
                assert!(endpoints.insert(
                    range.start(),
                    EndpointKind::Start,
                    overlay,
                    property_mask,
                ));
                assert!(endpoints.insert(range.end(), EndpointKind::End, overlay, property_mask,));
            }
        });
        true
    }

    /// Publish a complete batch atomically, constructing balanced arenas
    /// directly from sorted records instead of replaying `n` tree insertions.
    pub(super) fn attach_batch(
        &mut self,
        entries: &[(Value, EmacsByteRange)],
        order: OverlayBatchOrder,
    ) -> bool {
        if !self.is_empty() {
            return false;
        }
        let mut identities = FxHashSet::default();
        for (overlay, _) in entries {
            if !identities.insert(OverlayIdentity::of(*overlay)) {
                return false;
            }
        }
        let intervals = IntervalBPlusTree::from_entries(entries, order);
        let mut gnu_order = GnuOverlayOrder::new();
        let mut starts = FxHashMap::default();
        let mut attach = |overlay: Value, range: EmacsByteRange| {
            let identity = OverlayIdentity::of(overlay);
            let inserted = gnu_order.insert_by(identity, |existing| {
                range.start().cmp(
                    starts
                        .get(&existing)
                        .expect("batch GNU order references an unattached overlay"),
                )
            });
            assert!(
                inserted.is_ok(),
                "validated batch contains duplicate identity"
            );
            starts.insert(identity, range.start());
        };
        match order {
            OverlayBatchOrder::AttachmentSequence => {
                for (overlay, range) in entries.iter().copied() {
                    attach(overlay, range);
                }
            }
            OverlayBatchOrder::AscendingQueryOrder => {
                for (overlay, range) in entries.iter().rev().copied() {
                    attach(overlay, range);
                }
            }
        }
        self.intervals = Arc::new(RwLock::new(intervals));
        self.gnu_order = gnu_order;
        let mut direct_property_keys = FxHashMap::default();
        let mut property_masks =
            FxHashMap::with_capacity_and_hasher(entries.len(), Default::default());
        for (overlay, _) in entries {
            // Snapshot attachment can run inside a collection-read capture.
            // Finish owner initialization before observing its property keys.
            set_overlay_position_handle(*overlay, &self.intervals);
            let keys = overlay_direct_property_keys(*overlay);
            property_masks.insert(
                OverlayIdentity::of(*overlay),
                filter_mask_for_direct_keys(&keys),
            );
            for key in keys {
                *direct_property_keys.entry(key).or_default() += 1;
            }
        }
        self.sync_direct_property_key_cache(&direct_property_keys);
        self.write_core(|core| {
            core.endpoints = OnceLock::new();
            core.direct_property_keys = direct_property_keys;
            core.property_masks = property_masks;
        });
        true
    }

    /// Detach an overlay and return its indexed range.
    pub(super) fn detach(&mut self, overlay: Value) -> Option<EmacsByteRange> {
        let (range, _) = self.intervals.write().take(overlay)?;
        let identity = OverlayIdentity::of(overlay);
        let keys = overlay_direct_property_keys(overlay);
        assert!(
            self.gnu_order.remove(identity).is_ok(),
            "indexed overlay missing from GNU order mirror"
        );
        materialize_overlay_position(overlay, range);
        let cache = self.write_core(|core| {
            let distinct_changed = core.remove_property_keys(&keys);
            assert!(
                core.property_masks.remove(&identity).is_some(),
                "detached overlay missing cached property mask"
            );
            if let Some(endpoints) = core.endpoints.get_mut() {
                assert_eq!(
                    endpoints.remove(overlay, EndpointKind::Start),
                    Some(range.start())
                );
                assert_eq!(
                    endpoints.remove(overlay, EndpointKind::End),
                    Some(range.end())
                );
            }
            distinct_changed.then(|| snapshot_direct_property_key_cache(&core.direct_property_keys))
        });
        if let Some(cache) = cache {
            self.apply_direct_property_key_cache(cache);
        }
        Some(range)
    }

    /// Move an attached overlay, returning its old range.
    pub(super) fn move_to(
        &mut self,
        overlay: Value,
        new_range: EmacsByteRange,
    ) -> Option<EmacsByteRange> {
        let mut intervals = self.intervals.write();
        let old_range = intervals.range_by_identity(OverlayIdentity::of(overlay))?;
        match IndexedRegionChange::between(old_range, new_range) {
            IndexedRegionChange::Unchanged => {}
            IndexedRegionChange::EndOnly => {
                let previous = intervals
                    .update_end_preserving_order(overlay, new_range)
                    .expect("indexed overlay disappeared during end-only move");
                debug_assert_eq!(previous, old_range);
                // Lock order: `intervals` is already held before acquiring `core`.
                write_core_lock(&self.core, |core| {
                    if let Some(endpoints) = core.endpoints.get_mut() {
                        let property_mask = *core
                            .property_masks
                            .get(&OverlayIdentity::of(overlay))
                            .expect("attached overlay missing cached property mask");
                        assert_eq!(
                            endpoints.remove(overlay, EndpointKind::End),
                            Some(old_range.end())
                        );
                        assert!(endpoints.insert(
                            new_range.end(),
                            EndpointKind::End,
                            overlay,
                            property_mask,
                        ));
                    }
                });
            }
            IndexedRegionChange::StartChanged => {
                let local_moves = local_overlay_moves_enabled();
                let replaced_locally = local_moves
                    && intervals
                        .try_move_preserving_neighbors(overlay, new_range)
                        .is_some_and(|previous| {
                            debug_assert_eq!(previous, old_range);
                            true
                        });
                if !replaced_locally {
                    let taken = intervals
                        .take(overlay)
                        .expect("indexed overlay disappeared during relocation");
                    debug_assert_eq!(taken.0, old_range);
                }
                assert!(
                    self.gnu_order.remove(OverlayIdentity::of(overlay)).is_ok(),
                    "relocated overlay missing from GNU order mirror"
                );
                if !replaced_locally {
                    let inserted = intervals.insert(overlay, new_range);
                    debug_assert!(inserted, "removed overlay retained an interval node");
                }
                // Fresh full-key order guarantees this successor's start is
                // at least the new start; None means a strictly greatest
                // start. GNU deletion can preserve another order within an
                // equal-start cluster, so check the structural predecessor
                // once before accepting the B+ insertion gap. A mismatched
                // gap retains GNU's ordinary coordinate descent, without a
                // linear scan of the collapsed cluster.
                let successor = if local_moves {
                    intervals
                        .records()
                        .successor_identity(OverlayIdentity::of(overlay))
                } else {
                    None
                };
                let valid_local_gap = local_moves
                    && successor
                        .and_then(|successor| self.gnu_order.predecessor_identity(successor))
                        .is_none_or(|predecessor| {
                            intervals
                                .range_by_identity(predecessor)
                                .expect("GNU order predecessor is not indexed")
                                .start()
                                < new_range.start()
                        });
                let order_inserted = if valid_local_gap {
                    // Both indexes remain under this buffer owner's exclusive
                    // mutation and write guard. The validated gap preserves
                    // GNU topology without per-descent position resolution.
                    self.gnu_order
                        .insert_before(OverlayIdentity::of(overlay), successor)
                } else {
                    self.gnu_order
                        .insert_by(OverlayIdentity::of(overlay), |existing| {
                            new_range.start().cmp(
                                &intervals
                                    .range_by_identity(existing)
                                    .expect("GNU order mirror contains an unindexed overlay")
                                    .start(),
                            )
                        })
                };
                assert!(
                    order_inserted.is_ok(),
                    "relocated overlay retained a GNU order node"
                );
                // Lock order: `intervals` is already held before acquiring `core`.
                write_core_lock(&self.core, |core| {
                    if let Some(endpoints) = core.endpoints.get_mut() {
                        let property_mask = *core
                            .property_masks
                            .get(&OverlayIdentity::of(overlay))
                            .expect("attached overlay missing cached property mask");
                        assert_eq!(
                            endpoints.remove(overlay, EndpointKind::Start),
                            Some(old_range.start())
                        );
                        assert_eq!(
                            endpoints.remove(overlay, EndpointKind::End),
                            Some(old_range.end())
                        );
                        assert!(endpoints.insert(
                            new_range.start(),
                            EndpointKind::Start,
                            overlay,
                            property_mask,
                        ));
                        assert!(endpoints.insert(
                            new_range.end(),
                            EndpointKind::End,
                            overlay,
                            property_mask,
                        ));
                    }
                });
            }
        }
        Some(old_range)
    }

    /// GNU itree_node_set_region (itree.c:744-762) reinserts only when its
    /// numeric begin changes. Conversion compares against the other nodes'
    /// current numeric begins, which can be a mixture of old and new values;
    /// byte coordinates cannot substitute for that temporary comparison space.
    /// All remaps/maps are local under the owning buffer's exclusive mutation.
    #[cold]
    #[inline(never)]
    pub(super) fn remap_positions(&mut self, remaps: &[super::overlay::OverlayPositionRemap]) {
        assert_eq!(
            remaps.len(),
            self.len(),
            "conversion snapshot covers every overlay"
        );
        let mut ordered = Vec::with_capacity(remaps.len());
        if remaps
            .iter()
            .all(|remap| remap.old_begin == remap.new_begin)
        {
            // GNU changes only end augmentation when every numeric begin is
            // unchanged. The snapshot is already in the unchanged tree's
            // ascending order; byte remapping preserves that order as well.
            ordered.extend(remaps.iter().map(|remap| (remap.overlay, remap.range)));
        } else {
            // A single call-local map owns both the temporary numeric begins
            // and the final byte ranges. Entries start at old numeric begins;
            // only the current node changes before its GNU reinsertion.
            let mut current: FxHashMap<_, _> = remaps
                .iter()
                .map(|remap| (OverlayIdentity::of(remap.overlay), (remap.old_begin, remap)))
                .collect();
            for remap in remaps {
                if remap.old_begin != remap.new_begin {
                    let identity = OverlayIdentity::of(remap.overlay);
                    current.get_mut(&identity).expect("snapshot identity").0 = remap.new_begin;
                    assert!(
                        self.gnu_order
                            .reinsert_by(identity, |existing| {
                                remap.new_begin.cmp(&current[&existing].0)
                            })
                            .is_ok()
                    );
                }
            }
            self.gnu_order.for_each_inorder(|identity| {
                let remap = current[&identity].1;
                ordered.push((remap.overlay, remap.range));
            });
        }
        // Keep the Arc, so existing live position handles retain their one
        // authoritative index. Observer snapshots already own separate indexes.
        *self.intervals.write() =
            IntervalBPlusTree::from_entries(&ordered, OverlayBatchOrder::AscendingQueryOrder);
        self.write_core(|core| {
            core.endpoints = OnceLock::new();
        });
    }

    /// Apply an edit in `O(log n + k log n)`, where `k` is the number of
    /// overlays whose ranges touch the edited boundary.
    ///
    /// Whole intervals in the unaffected-prefix/affected-suffix partition are
    /// shifted by lazy subtree tags.  Only boundary-crossing intervals are
    /// removed and reinserted so endpoint gravity and evaporation remain
    /// GNU-compatible.
    #[inline]
    pub(super) fn adjust_for_text_edit(&mut self, edit: OverlayTextEdit) -> OverlayTextEditOutcome {
        // Most buffers carry no overlays, and every insertion and deletion
        // comes through here: with nothing to shift, the queries and vectors
        // below cost more than the edit itself (479 instructions per edit on
        // an empty index). GNU's `adjust_overlays_for_insert' likewise walks
        // a tree that is empty.
        if self.is_empty() {
            return OverlayTextEditOutcome::preserved(Vec::new());
        }
        match edit {
            OverlayTextEdit::Insert {
                position,
                length,
                before_markers,
            } => self.adjust_for_insert(position, length, before_markers),
            OverlayTextEdit::Delete { range } => {
                OverlayTextEditOutcome::preserved(self.adjust_for_delete(range))
            }
        }
    }

    #[inline]
    fn adjust_for_insert(
        &mut self,
        position: EmacsBytePos,
        length: EmacsByteLen,
        before_markers: bool,
    ) -> OverlayTextEditOutcome {
        if length.is_empty() {
            return OverlayTextEditOutcome::preserved(Vec::new());
        }
        let exceptions = self.overlays_touching(position);
        if exceptions.is_empty() {
            // Nothing touches the insertion, so no overlay grows, evaporates,
            // or changes order: shift the suffix in place and bump generation
            // in one core lock. Keeping this fast path compact lets
            // `adjust_for_text_edit` inline cleanly into `apply_text_edit`.
            let delta = EmacsByteDelta::insertion(length);
            self.intervals
                .write()
                .shift_at_or_after(position, before_markers, delta);
            self.write_core(|core| {
                if let Some(endpoints) = core.endpoints.get_mut() {
                    endpoints.shift_at_or_after(position, before_markers, delta);
                }
            });
            return OverlayTextEditOutcome::preserved(Vec::new());
        }
        self.adjust_for_insert_touching(position, length, before_markers, exceptions)
    }

    #[inline(never)]
    fn adjust_for_insert_touching(
        &mut self,
        position: EmacsBytePos,
        length: EmacsByteLen,
        before_markers: bool,
        mut exceptions: Vec<Value>,
    ) -> OverlayTextEditOutcome {
        // An overlay that STRICTLY contains the insertion keeps its start and
        // grows its end, whatever its advance flags say -- those decide only
        // what happens at a boundary. Its order against every other overlay is
        // therefore unchanged, which is exactly the case GNU's itree handles
        // by leaving the node where it is. Growing it in place spares it the
        // detach-and-reattach path, and typing inside an overlay -- inside the
        // region, inside hl-line's, inside a flycheck span -- is what editing
        // a buffer with overlays mostly is.
        //
        // Its end endpoint lies after the insertion, so the endpoint shift
        // below already moves it; only the interval record, which carries both
        // bounds at once, has to be told.
        let mut grown = Vec::new();
        exceptions.retain(|overlay| {
            let Some(range) = self.range(*overlay) else {
                return true;
            };
            if range.start() < position && range.end() > position {
                grown.push((*overlay, range));
                false
            } else {
                true
            }
        });
        let mut grown_effects = Vec::with_capacity(grown.len());
        if !grown.is_empty() {
            let delta = EmacsByteDelta::insertion(length);
            let mut intervals = self.intervals.write();
            for (overlay, range) in &grown {
                let new_range = EmacsByteRange::new(range.start(), delta.apply_to_pos(range.end()));
                intervals
                    .update_end_preserving_order(*overlay, new_range)
                    .expect("overlay containing the insertion is indexed");
                grown_effects.push(OverlayEditEffect::Resized {
                    overlay: *overlay,
                    range: new_range,
                });
            }
        }
        if exceptions.is_empty() {
            // Nothing starts or ends here, so no overlay can change its order
            // against the insertion: every one of them either lies wholly
            // before it or is shifted wholly, which the shift below does in
            // place. The detach-and-reattach dance is for the boundary cases
            // only, and typing away from every overlay boundary -- the usual
            // case -- should not build its sets and vectors.
            let delta = EmacsByteDelta::insertion(length);
            self.intervals
                .write()
                .shift_at_or_after(position, before_markers, delta);
            self.write_core(|core| {
                if let Some(endpoints) = core.endpoints.get_mut() {
                    endpoints.shift_at_or_after(position, before_markers, delta);
                }
            });
            return OverlayTextEditOutcome::preserved(grown_effects);
        }
        sort_and_dedup_overlay_identities(&mut exceptions);

        let front_candidates: Vec<_> = exceptions
            .iter()
            .filter_map(|overlay| {
                let range = self.range(*overlay)?;
                let data = overlay.as_overlay_data()?;
                (InsertionOrderChange::for_overlay(
                    range,
                    position,
                    before_markers,
                    data.front_advance,
                    data.rear_advance,
                ) == InsertionOrderChange::ReinsertFromPreorderStack)
                    .then(|| OverlayIdentity::of(*overlay))
            })
            .collect();
        let front_preorder = self.gnu_order.subset_in_preorder(&front_candidates);
        let order = if front_preorder.is_empty() {
            OverlayEditOrder::Preserved
        } else {
            OverlayEditOrder::Reinserted
        };
        for identity in &front_preorder {
            assert!(
                self.gnu_order.remove(*identity).is_ok(),
                "front-advancing overlay missing from GNU order mirror"
            );
        }
        let front_set: FxHashSet<_> = front_candidates.into_iter().collect();

        let mut detached = Vec::with_capacity(exceptions.len());
        for overlay in exceptions {
            if let Some((range, attachment_order)) = self.take_for_text_edit(overlay) {
                detached.push((overlay, range, attachment_order));
            }
        }

        let delta = EmacsByteDelta::insertion(length);
        self.intervals
            .write()
            .shift_at_or_after(position, before_markers, delta);

        let mut effects = grown_effects;
        effects.reserve(detached.len());
        let mut restored = Vec::with_capacity(detached.len());
        let mut front_updates = FxHashMap::default();
        for &(overlay, old_range, attachment_order) in &detached {
            #[cfg(test)]
            super::overlay::record_overlay_edit_candidate_inspection();
            let (front_advance, rear_advance) = overlay
                .as_overlay_data()
                .map(|data| (data.front_advance, data.rear_advance))
                .unwrap_or((false, false));
            let order_change = InsertionOrderChange::for_overlay(
                old_range,
                position,
                before_markers,
                front_advance,
                rear_advance,
            );
            let move_start = if before_markers {
                old_range.start() >= position
            } else {
                old_range.start() > position
                    || order_change == InsertionOrderChange::ReinsertFromPreorderStack
            };
            let move_end = if before_markers {
                old_range.end() >= position
            } else {
                old_range.end() > position || (old_range.end() == position && rear_advance)
            };
            let new_range = EmacsByteRange::new(
                if move_start {
                    delta.apply_to_pos(old_range.start())
                } else {
                    old_range.start()
                },
                if move_end {
                    delta.apply_to_pos(old_range.end())
                } else {
                    old_range.end()
                },
            );
            let identity = OverlayIdentity::of(overlay);
            if front_set.contains(&identity) {
                let previous = front_updates.insert(identity, (overlay, new_range));
                debug_assert!(previous.is_none(), "duplicate front-advance update");
            } else {
                self.restore_after_text_edit(
                    overlay,
                    new_range,
                    TextEditReattachment::PreservePlace { attachment_order },
                );
                restored.push((overlay, new_range));
            }
            if new_range != old_range {
                effects.push(OverlayEditEffect::Resized {
                    overlay,
                    range: new_range,
                });
            }
        }
        // GNU pushes candidates in tree pre-order and pops the stack for
        // reinsertion.  Both the B+ precedence serial and topology mirror must
        // observe that reverse order.
        for identity in front_preorder.into_iter().rev() {
            let (overlay, new_range) = front_updates
                .remove(&identity)
                .expect("front-advance candidate disappeared during insertion");
            self.restore_after_text_edit(overlay, new_range, TextEditReattachment::Reinsert);
            restored.push((overlay, new_range));
        }
        debug_assert!(front_updates.is_empty());
        // Apply all endpoint updates and the generation bump in one critical
        // section after all interval edits have finished.
        self.write_core(|core| {
            if let Some(endpoints) = core.endpoints.get_mut() {
                for &(overlay, old_range, _) in &detached {
                    assert_eq!(
                        endpoints.remove(overlay, EndpointKind::Start),
                        Some(old_range.start())
                    );
                    assert_eq!(
                        endpoints.remove(overlay, EndpointKind::End),
                        Some(old_range.end())
                    );
                }
                endpoints.shift_at_or_after(position, before_markers, delta);
                for &(overlay, new_range) in &restored {
                    let property_mask = *core
                        .property_masks
                        .get(&OverlayIdentity::of(overlay))
                        .expect("attached overlay missing cached property mask");
                    assert!(endpoints.insert(
                        new_range.start(),
                        EndpointKind::Start,
                        overlay,
                        property_mask,
                    ));
                    assert!(endpoints.insert(
                        new_range.end(),
                        EndpointKind::End,
                        overlay,
                        property_mask,
                    ));
                }
            }
        });
        OverlayTextEditOutcome { effects, order }
    }

    #[inline(never)]
    fn adjust_for_delete(&mut self, range: EmacsByteRange) -> Vec<OverlayEditEffect> {
        if range.is_empty() {
            return Vec::new();
        }
        let (mut exceptions, starts_contract) = self.deletion_exceptions(range);
        // A containing overlay with an unchanged start can update its end
        // in place. An existing START tie also stays in place when no starts
        // contract; otherwise it participates in the one ranked restore pass.
        // Text remains after END, so these in-place cases cannot evaporate.
        // The endpoint suffix shift below moves their end endpoints.
        let delta = EmacsByteDelta::deletion(range.len());
        let mut shrunk_effects = SmallVec::<[(Value, EmacsByteRange); 4]>::new();
        exceptions.retain(|overlay| {
            let Some(old) = self.range(*overlay) else {
                return true;
            };
            if (old.start() < range.start() || (!starts_contract && old.start() == range.start()))
                && old.end() > range.end()
            {
                shrunk_effects.push((*overlay, old));
                false
            } else {
                true
            }
        });
        if !shrunk_effects.is_empty() {
            let mut intervals = self.intervals.write();
            for (overlay, old) in &shrunk_effects {
                let new_range = EmacsByteRange::new(old.start(), delta.apply_to_pos(old.end()));
                intervals
                    .update_end_preserving_order(*overlay, new_range)
                    .expect("overlay containing the deletion is indexed");
            }
        }
        let mut effects = Vec::with_capacity(shrunk_effects.len() + exceptions.len());
        effects.extend(shrunk_effects.into_iter().map(|(overlay, old)| {
            OverlayEditEffect::Resized {
                overlay,
                range: EmacsByteRange::new(old.start(), delta.apply_to_pos(old.end())),
            }
        }));

        let mut detached = SmallVec::<[(Value, EmacsByteRange, u64); 8]>::new();
        for overlay in exceptions {
            if let Some((old_range, attachment_order)) = self.take_for_text_edit(overlay) {
                detached.push((overlay, old_range, attachment_order));
            }
        }

        self.intervals
            .write()
            .shift_at_or_after(range.end(), true, delta);

        // Candidates were captured in the original ascending query order,
        // which equals GNU structural in-order. itree_delete_gap (itree.c:1173)
        // preserves that order. Assign fresh descending B+ attachment keys
        // during this one restore loop, only when distinct starts collapse.
        // START and END boundary records are included in that same snapshot.
        let collapsed_count = detached
            .iter()
            .filter(|(_, old, _)| old.start() >= range.start())
            .count();
        let mut collapsed_orders = starts_contract.then(|| {
            self.intervals
                .write()
                .reserve_attachment_orders(collapsed_count)
                .rev()
        });
        let mut evaporated = SmallVec::<[OverlayIdentity; 8]>::new();
        let mut evaporated_keys = SmallVec::<[SmallVec<[usize; 8]>; 4]>::new();
        let mut restored = SmallVec::<[(Value, EmacsByteRange); 8]>::new();
        for &(overlay, old_range, mut attachment_order) in &detached {
            if old_range.start() >= range.start() {
                if let Some(orders) = &mut collapsed_orders {
                    attachment_order = orders.next().expect("reserved collapsed-start rank");
                }
            }
            #[cfg(test)]
            super::overlay::record_overlay_edit_candidate_inspection();
            let new_start = if old_range.start() >= range.end() {
                delta.apply_to_pos(old_range.start())
            } else if old_range.start() > range.start() {
                range.start()
            } else {
                old_range.start()
            };
            let new_end = if old_range.end() >= range.end() {
                delta.apply_to_pos(old_range.end())
            } else if old_range.end() > range.start() {
                range.start()
            } else {
                old_range.end()
            };
            let new_range = EmacsByteRange::new(new_start, new_end);
            let evaporates = new_range.is_empty()
                && overlay.as_overlay_data().is_some_and(|data| {
                    plist::plist_get(data.plist, &Value::symbol("evaporate"))
                        .is_some_and(|value| value.is_truthy())
                });
            if evaporates {
                evaporated.push(OverlayIdentity::of(overlay));
                evaporated_keys.push(overlay_direct_property_keys(overlay));
                materialize_overlay_position(overlay, new_range);
                effects.push(OverlayEditEffect::Evaporated {
                    overlay,
                    collapsed_at: new_range.start(),
                });
            } else {
                self.restore_after_text_edit(
                    overlay,
                    new_range,
                    TextEditReattachment::PreservePlace { attachment_order },
                );
                restored.push((overlay, new_range));
                if new_range != old_range {
                    effects.push(OverlayEditEffect::Resized {
                        overlay,
                        range: new_range,
                    });
                }
            }
        }
        // GNU discovers evaporated overlays in ascending itree order, conses
        // them, then deletes the resulting reversed list.  Preserve that
        // removal order because red-black topology affects later insertion.
        for &identity in evaporated.iter().rev() {
            assert!(
                self.gnu_order.remove(identity).is_ok(),
                "evaporated overlay missing from GNU order mirror"
            );
        }
        // Apply evaporated key removals, endpoint updates, and the generation
        // bump in one critical section after all interval edits have finished.
        let cache = self.write_core(|core| {
            let mut distinct_changed = false;
            for &identity in &evaporated {
                assert!(
                    core.property_masks.remove(&identity).is_some(),
                    "evaporated overlay missing cached property mask"
                );
            }
            for keys in &evaporated_keys {
                distinct_changed |= core.remove_property_keys(keys);
            }
            if let Some(endpoints) = core.endpoints.get_mut() {
                for &(overlay, old_range, _) in &detached {
                    assert_eq!(
                        endpoints.remove(overlay, EndpointKind::Start),
                        Some(old_range.start())
                    );
                    assert_eq!(
                        endpoints.remove(overlay, EndpointKind::End),
                        Some(old_range.end())
                    );
                }
                endpoints.shift_at_or_after(range.end(), true, delta);
                for &(overlay, new_range) in &restored {
                    let property_mask = *core
                        .property_masks
                        .get(&OverlayIdentity::of(overlay))
                        .expect("attached overlay missing cached property mask");
                    assert!(endpoints.insert(
                        new_range.start(),
                        EndpointKind::Start,
                        overlay,
                        property_mask,
                    ));
                    assert!(endpoints.insert(
                        new_range.end(),
                        EndpointKind::End,
                        overlay,
                        property_mask,
                    ));
                }
            }
            distinct_changed.then(|| snapshot_direct_property_key_cache(&core.direct_property_keys))
        });
        if let Some(cache) = cache {
            self.apply_direct_property_key_cache(cache);
        }
        effects
    }

    /// Capture deletion candidates in original structural order, including
    /// overlays beginning exactly at END that contract into the START group.
    /// Inline scratch is private to the owning buffer's exclusive mutation;
    /// ordinary small deletions allocate no ordering/candidate scratch.
    fn deletion_exceptions(&self, range: EmacsByteRange) -> (SmallVec<[Value; 8]>, bool) {
        let mut exceptions = SmallVec::new();
        let mut starts_contract = false;
        self.intervals.read().records().for_each_match(
            IntervalDeletionQuery::new(range),
            |record| {
                starts_contract |= record.range().start() > range.start();
                exceptions.push(record.overlay());
            },
        );
        (exceptions, starts_contract)
    }

    fn take_for_text_edit(&mut self, overlay: Value) -> Option<(EmacsByteRange, u64)> {
        self.intervals.write().take(overlay)
    }

    fn restore_after_text_edit(
        &mut self,
        overlay: Value,
        range: EmacsByteRange,
        reattachment: TextEditReattachment,
    ) {
        let inserted = match reattachment {
            TextEditReattachment::PreservePlace { attachment_order } => self
                .intervals
                .write()
                .insert_with_order(overlay, range, attachment_order),
            TextEditReattachment::Reinsert => self.intervals.write().insert(overlay, range),
        };
        debug_assert!(inserted, "detached overlay retained an interval node");
        if reattachment == TextEditReattachment::Reinsert {
            let intervals = self.intervals.read();
            let order_inserted =
                self.gnu_order
                    .insert_by(OverlayIdentity::of(overlay), |existing| {
                        range.start().cmp(
                            &intervals
                                .range_by_identity(existing)
                                .expect("GNU order mirror contains an unindexed overlay")
                                .start(),
                        )
                    });
            assert!(
                order_inserted.is_ok(),
                "reinserted overlay retained a GNU order node"
            );
        }
    }

    pub(super) fn clear(&mut self) {
        let materialized: Vec<_> = self.intervals.read().entries().collect();
        for (overlay, range) in materialized {
            materialize_overlay_position(overlay, range);
        }
        self.intervals = Arc::new(RwLock::new(IntervalBPlusTree::new()));
        self.gnu_order = GnuOverlayOrder::new();
        self.apply_direct_property_key_cache((0, [0; DIRECT_PROPERTY_KEYS_INLINE_CAP], 0));
        self.write_core(|core| {
            core.endpoints = OnceLock::new();
            core.direct_property_keys.clear();
            core.property_masks.clear();
        });
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(super) fn contains(&self, overlay: Value) -> bool {
        #[cfg(test)]
        record_owner_query_for_test(self);
        self.intervals.read().contains(overlay)
    }

    #[doc(hidden)]
    pub fn range(&self, overlay: Value) -> Option<EmacsByteRange> {
        #[cfg(test)]
        record_owner_query_for_test(self);
        self.intervals
            .read()
            .range_by_identity(OverlayIdentity::of(overlay))
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(super) fn len(&self) -> usize {
        self.intervals.read().len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.intervals.read().is_empty()
    }

    pub(super) fn values(&self) -> impl Iterator<Item = Value> + '_ {
        let records = RwLockReadGuard::map(self.intervals.read(), |tree| tree.records());
        OrderedShiftTree::matches_owned(records, IntervalBPlusQuery::All)
            .map(|record| record.overlay())
    }

    pub(super) fn overlays_at(&self, pos: EmacsBytePos) -> Vec<Value> {
        let intervals = self.intervals.read();
        let mut overlays = Vec::new();
        intervals
            .records()
            .for_each_match(IntervalBPlusQuery::Point(pos), |record| {
                overlays.push(record.overlay());
            });
        overlays
    }

    /// Visit all point matches in the same ascending record order as
    /// overlays_at_iter. The visitor owns no resumable traversal state; this
    /// lookup keeps the owning buffer's read lock for its complete reduction.
    #[inline]
    pub(super) fn for_each_overlay_at(&self, pos: EmacsBytePos, mut visit: impl FnMut(Value)) {
        let intervals = self.intervals.read();
        intervals
            .records()
            .for_each_match(IntervalBPlusQuery::Point(pos), |record| {
                visit(record.overlay());
            });
    }

    pub(super) fn overlays_at_iter(&self, pos: EmacsBytePos) -> impl Iterator<Item = Value> + '_ {
        let records = RwLockReadGuard::map(self.intervals.read(), |tree| tree.records());
        OrderedShiftTree::matches_owned(records, IntervalBPlusQuery::Point(pos))
            .map(|record| record.overlay())
    }

    /// Return intervals whose closed bounds touch `pos`.
    ///
    /// Character-property lookup uses half-open coverage, while GNU's
    /// `get-pos-property` additionally considers overlays beginning or ending
    /// exactly at the insertion position before applying advance flags.
    pub(super) fn overlays_touching(&self, pos: EmacsBytePos) -> Vec<Value> {
        let mut overlays = Vec::new();
        self.intervals.read().overlays_touching(pos, &mut overlays);
        overlays
    }

    /// Lazy `overlays_touching`, in the same order; holds the read lock
    /// until dropped.
    pub(super) fn overlays_touching_iter(
        &self,
        pos: EmacsBytePos,
    ) -> impl Iterator<Item = Value> + '_ {
        let records = RwLockReadGuard::map(self.intervals.read(), |tree| tree.records());
        OrderedShiftTree::matches_owned(records, IntervalBPlusQuery::ClosedPoint(pos))
            .map(|record| record.overlay())
    }

    pub(super) fn overlays_in_region_iter(
        &self,
        range: EmacsByteRange,
        accessible_end: EmacsBytePos,
    ) -> impl Iterator<Item = Value> + '_ {
        let records = RwLockReadGuard::map(self.intervals.read(), |tree| tree.records());
        OrderedShiftTree::matches_owned(
            records,
            IntervalBPlusQuery::Region {
                range,
                accessible_end,
            },
        )
        .map(|record| record.overlay())
    }

    pub(super) fn overlays_intersecting(&self, range: EmacsByteRange) -> Vec<Value> {
        let intervals = self.intervals.read();
        let mut overlays = Vec::with_capacity(intervals.len());
        intervals
            .records()
            .for_each_match(IntervalIntersectionQuery::new(range), |record| {
                overlays.push(record.overlay());
            });
        overlays
    }

    pub(super) fn overlays_in_region(
        &self,
        range: EmacsByteRange,
        accessible_end: EmacsBytePos,
    ) -> Vec<Value> {
        let intervals = self.intervals.read();
        let mut overlays = Vec::new();
        intervals.records().for_each_match(
            IntervalBPlusQuery::Region {
                range,
                accessible_end,
            },
            |record| overlays.push(record.overlay()),
        );
        overlays
    }

    pub(super) fn all_ascending(&self) -> Vec<Value> {
        let mut overlays = Vec::with_capacity(self.len());
        self.intervals.read().all_ascending(&mut overlays);
        overlays
    }

    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(super) fn may_contain_property(&self, property: Value) -> bool {
        let bits = property.bits();
        if (self.direct_property_key_bloom & direct_property_key_bloom_bit(bits)) == 0 {
            return false;
        }
        let len = self.direct_property_keys_inline_len as usize;
        if len <= DIRECT_PROPERTY_KEYS_INLINE_CAP {
            return self.direct_property_keys_inline[..len].contains(&bits);
        }
        self.core.read().direct_property_keys.contains_key(&bits)
    }

    pub(super) fn endpoint_records_strictly_within(
        &self,
        bounds: EmacsByteRange,
        property_filter: OverlayPropertyFilter,
    ) -> OverlayEndpointRecords<'_> {
        let _ = publish_endpoints_for(
            &self.intervals,
            &self.core,
            None,
            #[cfg(test)]
            Some(self.cold_intervals_probe.as_ref()),
        );
        let records = RwLockReadGuard::map(self.core.read(), |core| {
            &core
                .endpoints
                .get()
                .expect("overlay endpoints published before direct scan")
                .records
        });
        OverlayEndpointRecords(OverlayEndpointRecordsInner::Direct(
            OrderedShiftTree::matches_owned(
                records,
                DirectEndpointRangeQuery {
                    bounds,
                    property_filter,
                },
            ),
        ))
    }

    pub(super) fn endpoint_records_strictly_within_reverse(
        &self,
        bounds: EmacsByteRange,
        property_filter: OverlayPropertyFilter,
    ) -> OverlayEndpointRecords<'_> {
        let _ = publish_endpoints_for(
            &self.intervals,
            &self.core,
            None,
            #[cfg(test)]
            Some(self.cold_intervals_probe.as_ref()),
        );
        let records = RwLockReadGuard::map(self.core.read(), |core| {
            &core
                .endpoints
                .get()
                .expect("overlay endpoints published before reverse direct scan")
                .records
        });
        OverlayEndpointRecords(OverlayEndpointRecordsInner::Direct(
            OrderedShiftTree::matches_owned_reverse(
                records,
                DirectEndpointRangeQuery {
                    bounds,
                    property_filter,
                },
            ),
        ))
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(super) fn overlay_properties_changed(
        &mut self,
        overlay: Value,
        previous: Option<OverlayPropertyKeys>,
    ) {
        debug_assert_eq!(self.contains(overlay), previous.is_some());
        let new_keys = previous.as_ref().map(|previous| {
            debug_assert_eq!(previous.overlay, OverlayIdentity::of(overlay));
            debug_assert!(
                previous.handle.belongs_to(overlay, &self.intervals),
                "property token crossed an index owner"
            );
            debug_assert!(
                overlay.as_overlay_data().is_some_and(|data| {
                    data.position_handle()
                        .as_ref()
                        .is_some_and(|handle| handle.belongs_to(overlay, &self.intervals))
                }),
                "property write replaced its live attachment handle"
            );
            debug_assert_eq!(
                previous.handle.attachment_order(),
                Some(previous.attachment_order),
                "property token crossed an attachment change"
            );
            overlay_direct_property_keys(overlay)
        });
        let new_mask = new_keys
            .as_deref()
            .map(filter_mask_for_direct_keys)
            .unwrap_or(OrderedFilterMask::EMPTY);
        let cache = self.write_core(|core| {
            let distinct_changed = if let (Some(previous), Some(new_keys)) = (previous, new_keys) {
                let removed = core.remove_property_keys(&previous.keys);
                let added = core.add_property_keys(&new_keys);
                core.property_masks.insert(previous.overlay, new_mask);
                removed || added
            } else {
                false
            };
            if let Some(endpoints) = core.endpoints.get_mut() {
                endpoints.replace_overlay_property_mask(overlay, new_mask);
            }
            distinct_changed.then(|| snapshot_direct_property_key_cache(&core.direct_property_keys))
        });
        if let Some(cache) = cache {
            self.apply_direct_property_key_cache(cache);
        }
    }

    /// Record the one pair prepended to an already-resolved member by
    /// overlay-put. The caller holds this owner's exclusive loan, resolves
    /// membership before writing, and runs no Lisp or attachment change
    /// between that resolution and this update. Existing-key writes change
    /// neither key counts nor endpoint classes.
    pub(super) fn overlay_property_key_added_to_member(&mut self, overlay: Value, property: Value) {
        let bits = property.bits();
        let identity = OverlayIdentity::of(overlay);
        let bit = overlay_property_signature_bit_raw(bits);
        self.note_direct_property_key_added(bits);
        self.write_core(|core| {
            *core.direct_property_keys.entry(bits).or_default() += 1;
            let mask = core
                .property_masks
                .get_mut(&identity)
                .expect("attached overlay missing cached property mask");
            *mask = mask.with_bit(bit);
            if let Some(endpoints) = core.endpoints.get_mut() {
                endpoints.add_overlay_property_key(overlay, property);
            }
        });
    }

    #[cfg(test)]
    pub(super) fn clear_direct_property_key_bloom_bit_for_test(&mut self, property: Value) {
        self.direct_property_key_bloom &= !direct_property_key_bloom_bit(property.bits());
    }

    pub(super) fn next_boundary_after_matching_no_publish(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
        property_filter: OverlayPropertyFilter,
    ) -> Option<EmacsBytePos> {
        if pos >= limit {
            return None;
        }
        {
            let core = self.core.read();
            if let Some(endpoints) = core.endpoints.get() {
                return endpoints.next_after_filtered(pos, limit, property_filter);
            }
        }
        self.next_boundary_after_no_alloc(pos, limit)
    }

    pub(super) fn next_boundary_after(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.next_boundary_after_no_alloc(pos, limit)
    }

    /// Borrow an existing index without publishing an allocating endpoint tree.
    /// A cold index combines the next start with ends of intervals containing
    /// pos. Future empty overlays contribute their starts. The read guard and
    /// visitor stack allocate no endpoint index; the warm tree stays unchanged.
    pub(super) fn next_boundary_after_no_alloc(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        if pos >= limit {
            return None;
        }
        {
            let core = self.core.read();
            if let Some(endpoints) = core.endpoints.get() {
                return endpoints.next_after(pos, limit);
            }
        }
        // Drop `core` before acquiring `intervals` so readers never invert the
        // `intervals -> core` lock order used by writers.
        #[cfg(test)]
        self.cold_intervals_probe.record_intervals_read(&self.core);
        let intervals = self.intervals.read();
        let mut next = intervals.next_start_after(pos, limit);
        // The nearest later endpoint is either the next start, found above,
        // or an end of an interval containing pos. Keep the strict-after guard
        // below independent of the point query's endpoint convention.
        intervals
            .records()
            .for_each_match(IntervalBPlusQuery::Point(pos), |record| {
                #[cfg(test)]
                cold_boundary_instrumentation::record_visit();
                let end = record.range().end();
                if end > pos && end <= limit && next.is_none_or(|current| end < current) {
                    next = Some(end);
                }
            });
        next
    }

    pub(super) fn previous_boundary_before(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        if pos <= limit {
            return None;
        }
        {
            let core = self.core.read();
            if let Some(endpoints) = core.endpoints.get() {
                return endpoints.previous_before(pos, limit);
            }
        }
        // Drop `core` before acquiring `intervals` so readers never invert the
        // `intervals -> core` lock order used by writers.
        #[cfg(test)]
        self.cold_intervals_probe.record_intervals_read(&self.core);
        let intervals = self.intervals.read();
        let start = intervals.previous_start_before(pos, limit);
        let candidate = start.unwrap_or(limit);
        // No start lies strictly between candidate and pos. Every end that
        // can improve candidate therefore belongs to an interval touching it.
        // ClosedPoint also preserves a real endpoint exactly at lower LIMIT
        // when candidate is only a clamp, including an empty overlay there.
        let mut previous = start;
        intervals
            .records()
            .for_each_match(IntervalBPlusQuery::ClosedPoint(candidate), |record| {
                let end = record.range().end();
                if end >= candidate && end < pos && previous.is_none_or(|current| end > current) {
                    previous = Some(end);
                }
            });
        previous
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn interval_height(&self) -> usize {
        self.intervals.read().height()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(super) fn assert_invariants(&self) {
        let interval_len = {
            let intervals = self.intervals.read();
            intervals.assert_invariants();
            intervals.len()
        };
        {
            let core = self.core.read();
            if let Some(endpoints) = core.endpoints.get() {
                endpoints.assert_invariants();
                assert_eq!(interval_len * 2, endpoints.len());
            }
        }
        self.gnu_order.assert_invariants();
        assert_eq!(interval_len, self.gnu_order.len());
        let mut expected_keys = FxHashMap::default();
        let mut expected_masks = FxHashMap::default();
        for overlay in self.values() {
            for key in overlay_direct_property_keys(overlay) {
                *expected_keys.entry(key).or_insert(0usize) += 1;
            }
            expected_masks.insert(
                OverlayIdentity::of(overlay),
                overlay_indexed_property_mask(overlay),
            );
        }
        let (actual_keys, actual_masks) = {
            let core = self.core.read();
            (
                core.direct_property_keys.clone(),
                core.property_masks.clone(),
            )
        };
        assert_eq!(actual_keys, expected_keys);
        assert_eq!(actual_masks, expected_masks);
        let (expected_bloom, _, expected_len) = snapshot_direct_property_key_cache(&expected_keys);
        assert_eq!(self.direct_property_key_bloom, expected_bloom);
        assert_eq!(self.direct_property_keys_inline_len, expected_len);
        if (expected_len as usize) <= DIRECT_PROPERTY_KEYS_INLINE_CAP {
            for &key in expected_keys.keys() {
                assert!(
                    self.direct_property_keys_inline[..expected_len as usize].contains(&key),
                    "inline direct_property_keys cache missing key {key:#x}"
                );
            }
        }
    }
}

/// Sort and deduplicate by Lisp object identity, not Rust `Value::eq`.
///
/// `Value::eq` intentionally implements Lisp `equal`, under which distinct
/// overlays with the same logical fields compare equal. Index membership is
/// identity-based, matching GNU's overlay object semantics.
fn sort_and_dedup_overlay_identities(overlays: &mut Vec<Value>) {
    overlays.sort_unstable_by_key(|overlay| overlay.bits());
    overlays.dedup_by_key(|overlay| overlay.bits());
}
/// GNU descends left when inserting another node at the same begin position,
/// so an ascending traversal observes the newest attachment first.  Encoding
/// that tie-break explicitly prevents rotations from changing Lisp-visible
/// traversal order.

#[cfg(test)]
#[path = "overlay_index/tests/cold_boundary_instrumentation.rs"]
mod cold_boundary_instrumentation;

#[cfg(test)]
#[path = "overlay_index/tests/overlay_index_test.rs"]
mod tests;

#[cfg(test)]
#[path = "overlay_index/tests/nonalloc_boundary.rs"]
mod nonalloc_boundary;

#[cfg(test)]
#[path = "overlay_index/tests/observed_attachment.rs"]
mod observed_attachment;

#[cfg(test)]
#[path = "overlay_index/tests/core_locking.rs"]
mod core_locking;

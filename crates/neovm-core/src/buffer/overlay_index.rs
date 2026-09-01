//! Buffer-owned indexes for live overlays.
//!
//! `OverlayIndex` is the single mutation boundary for overlay membership,
//! endpoint lookup, and interval queries.  Lisp object state remains owned by
//! `OverlayList`; this module owns the structural invariants needed to find
//! those objects efficiently.

use std::cmp::Ordering;
use std::sync::{Arc, OnceLock, Weak};

use parking_lot::{RwLock, RwLockReadGuard};
use rustc_hash::{FxHashMap, FxHashSet};

use super::overlay_bplus::{
    OrderedFilterMask, OrderedShiftRecord, OrderedShiftTree, OrderedTreeMatches, OrderedTreeQuery,
};
use super::overlay_order::GnuOverlayOrder;
use crate::emacs_core::plist;
use crate::emacs_core::value::Value;
use crate::heap_types::OverlayData;

use super::position::{EmacsByteDelta, EmacsByteLen, EmacsBytePos, EmacsByteRange};

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

fn overlay_property_signature_bit(property: Value) -> usize {
    // SplitMix64 finalizer: symbol ids are densely allocated and their tagged
    // representation has low zero bits, so using the raw low byte would create
    // systematic collisions rather than a conservative bloom signature.
    let mut value = property.bits() as u64;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((value ^ (value >> 31)) & 0xff) as usize
}

/// Meaning of the order in which a bulk publisher supplies overlay records.
///
/// GNU's `copy_overlays` walks the source tree in ascending query order and
/// then attaches each copy, so later records become newer at an equal start.
/// A pdump or immutable snapshot instead restores an already-observed query
/// order.  Making that distinction explicit prevents a linear rebuild from
/// silently reversing equal-start overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OverlayBatchOrder {
    AttachmentSequence,
    AscendingQueryOrder,
}

/// Lisp object identity for index membership.
///
/// `Value`'s Rust equality is Lisp `equal`; overlay membership is GNU `eq`.
/// Keeping the raw tagged bits behind a distinct key type makes those two
/// domains impossible to mix at map/set call sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct OverlayIdentity(usize);

impl OverlayIdentity {
    pub(super) fn of(overlay: Value) -> Self {
        Self(overlay.bits())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum EndpointKind {
    Start,
    End,
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

fn overlay_indexed_property_mask(overlay: Value) -> OrderedFilterMask {
    let Some(data) = overlay.as_overlay_data() else {
        return OrderedFilterMask::EMPTY;
    };
    let mut mask = OrderedFilterMask::EMPTY;
    let mut tail = data.plist;
    while tail.is_cons() {
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
struct EndpointRangeQuery {
    bounds: EmacsByteRange,
    property_filter: OverlayPropertyFilter,
}

impl OrderedTreeQuery<EndpointRecord> for EndpointRangeQuery {
    fn subtree_may_match(
        self,
        minimum: EmacsBytePos,
        maximum: EmacsBytePos,
        _maximum_end: EmacsBytePos,
    ) -> bool {
        minimum < self.bounds.end() && maximum > self.bounds.start()
    }

    fn record_matches(self, record: EndpointRecord) -> bool {
        self.bounds.start() < record.key.position
            && record.key.position < self.bounds.end()
            && self.property_filter.record_may_match(record.property_mask)
    }

    fn subtree_filter_may_match(self, filter_mask: OrderedFilterMask) -> bool {
        self.property_filter.subtree_may_match(filter_mask)
    }

    fn maximum_end_is_too_small(self, maximum_end: EmacsBytePos) -> bool {
        maximum_end <= self.bounds.start()
    }

    fn minimum_start_is_too_large(self, minimum_start: EmacsBytePos) -> bool {
        minimum_start >= self.bounds.end()
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

pub(super) struct OverlayEndpointRecords<'a> {
    records: OrderedTreeMatches<
        &'a OrderedShiftTree<EndpointRecord>,
        EndpointRecord,
        EndpointRangeQuery,
    >,
}

impl Iterator for OverlayEndpointRecords<'_> {
    type Item = OverlayEndpoint;

    fn next(&mut self) -> Option<Self::Item> {
        self.records.next().map(|record| OverlayEndpoint {
            position: record.key.position,
            overlay: record.overlay,
            kind: record.kind,
        })
    }
}

#[derive(Clone, Debug)]
struct EndpointBPlusTree {
    records: OrderedShiftTree<EndpointRecord>,
    next_order: u64,
}

impl EndpointBPlusTree {
    fn from_entries(entries: &[(Value, EndpointKind, EmacsBytePos)]) -> Self {
        let records = entries
            .iter()
            .enumerate()
            .map(|(order, (overlay, kind, position))| EndpointRecord {
                key: EndpointKey {
                    position: *position,
                    order: order as u64,
                },
                identity: EndpointIdentity::of(*overlay, *kind),
                overlay: *overlay,
                kind: *kind,
                property_mask: overlay_indexed_property_mask(*overlay),
            })
            .collect();
        Self {
            records: OrderedShiftTree::from_records(records),
            next_order: entries.len() as u64,
        }
    }

    fn insert(&mut self, position: EmacsBytePos, kind: EndpointKind, overlay: Value) -> bool {
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
            property_mask: overlay_indexed_property_mask(overlay),
        })
    }

    fn remove(&mut self, overlay: Value, kind: EndpointKind) -> Option<EmacsBytePos> {
        self.records
            .remove(EndpointIdentity::of(overlay, kind))
            .map(|record| record.key.position)
    }

    fn records_strictly_within(
        &self,
        bounds: EmacsByteRange,
        property_filter: OverlayPropertyFilter,
    ) -> OverlayEndpointRecords<'_> {
        OverlayEndpointRecords {
            records: self.records.matches(EndpointRangeQuery {
                bounds,
                property_filter,
            }),
        }
    }

    fn records_strictly_within_reverse(
        &self,
        bounds: EmacsByteRange,
        property_filter: OverlayPropertyFilter,
    ) -> OverlayEndpointRecords<'_> {
        OverlayEndpointRecords {
            records: self.records.matches_reverse(EndpointRangeQuery {
                bounds,
                property_filter,
            }),
        }
    }

    fn refresh_overlay_property_mask(&mut self, overlay: Value) {
        let property_mask = overlay_indexed_property_mask(overlay);
        for kind in [EndpointKind::Start, EndpointKind::End] {
            let identity = EndpointIdentity::of(overlay, kind);
            let Some(mut record) = self.records.record(identity) else {
                continue;
            };
            record.property_mask = property_mask;
            let previous = self
                .records
                .replace_same_key(record)
                .expect("published overlay endpoint disappeared during property update");
            debug_assert_eq!(previous.identity, identity);
        }
    }

    fn next_after(&self, position: EmacsBytePos, limit: EmacsBytePos) -> Option<EmacsBytePos> {
        self.records.next_position_after(position, limit)
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

    #[cfg(test)]
    fn assert_invariants(&self) {
        self.records.assert_invariants();
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.records.len()
    }
}

/// Runtime-only link from an overlay object to the buffer-owned position
/// index that currently owns its endpoints.
///
/// The weak store reference cannot keep a dead buffer alive, while the typed
/// identity prevents Lisp `equal` from being confused with overlay `eq`.
#[derive(Clone, Debug)]
pub(crate) struct OverlayPositionHandle {
    intervals: Weak<RwLock<IntervalBPlusTree>>,
    identity: OverlayIdentity,
}

impl OverlayPositionHandle {
    fn new(overlay: Value, intervals: &Arc<RwLock<IntervalBPlusTree>>) -> Self {
        Self {
            intervals: Arc::downgrade(intervals),
            identity: OverlayIdentity::of(overlay),
        }
    }

    fn current_range(&self) -> Option<EmacsByteRange> {
        let intervals = self.intervals.upgrade()?;
        intervals.read().range_by_identity(self.identity)
    }
}

fn set_overlay_position_handle(overlay: Value, intervals: &Arc<RwLock<IntervalBPlusTree>>) {
    let handle = OverlayPositionHandle::new(overlay, intervals);
    let _ = overlay.with_overlay_data_mut(|data| {
        data.position_handle = Some(handle);
    });
}

fn materialize_overlay_position(overlay: Value, range: EmacsByteRange) {
    let _ = overlay.with_overlay_data_mut(|data| {
        data.start = range.start().get();
        data.end = range.end().get();
        data.position_handle = None;
    });
}

/// Resolve the current indexed range for a live overlay.
///
/// `OverlayData::start/end` remain a pdump-compatible materialized cache. A
/// lazy suffix edit intentionally does not rewrite every cache entry; live
/// objects resolve through their typed position handle before falling back to
/// the cache for detached or not-yet-attached overlays.
pub(crate) fn current_overlay_range(data: &OverlayData) -> Option<EmacsByteRange> {
    data.buffer?;
    data.position_handle.as_ref()?.current_range()
}

#[derive(Debug)]
pub(super) struct OverlayIndex {
    intervals: Arc<RwLock<IntervalBPlusTree>>,
    /// Topology-only mirror for the one GNU red-black-tree detail observable
    /// during front-advancing insertion.  Positions remain authoritative in
    /// `intervals`; this field stores only identity and tree shape.
    gnu_order: GnuOverlayOrder<OverlayIdentity>,
    /// Published lazily on the first boundary query.  `OnceLock` makes the
    /// ready-state read path one atomic load; mutations already require
    /// `&mut OverlayIndex`, so they can update the tree through `get_mut`
    /// without a second runtime lock.
    endpoints: OnceLock<EndpointBPlusTree>,
}

impl OverlayIndex {
    pub(super) fn new() -> Self {
        Self {
            intervals: Arc::new(RwLock::new(IntervalBPlusTree::new())),
            gnu_order: GnuOverlayOrder::new(),
            endpoints: OnceLock::new(),
        }
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
        assert!(inserted, "new interval already existed in GNU order mirror");
        drop(intervals);
        if let Some(endpoints) = self.endpoints.get_mut() {
            assert!(endpoints.insert(range.start(), EndpointKind::Start, overlay));
            assert!(endpoints.insert(range.end(), EndpointKind::End, overlay));
        }
        set_overlay_position_handle(overlay, &self.intervals);
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
            assert!(inserted, "validated batch contains duplicate identity");
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
        self.endpoints = OnceLock::new();
        for (overlay, _) in entries {
            set_overlay_position_handle(*overlay, &self.intervals);
        }
        true
    }

    fn endpoint_index(&self) -> &EndpointBPlusTree {
        self.endpoints.get_or_init(|| {
            #[cfg(test)]
            super::overlay::record_endpoint_publication_interval_read();
            let intervals = self.intervals.read();
            let entries = intervals.endpoint_entries_in_attachment_order();
            EndpointBPlusTree::from_entries(&entries)
        })
    }

    fn with_endpoint_index<T>(&self, use_index: impl FnOnce(&EndpointBPlusTree) -> T) -> T {
        use_index(self.endpoint_index())
    }

    /// Detach an overlay and return its indexed range.
    pub(super) fn detach(&mut self, overlay: Value) -> Option<EmacsByteRange> {
        let (range, _) = self.intervals.write().take(overlay)?;
        assert!(
            self.gnu_order.remove(OverlayIdentity::of(overlay)),
            "indexed overlay missing from GNU order mirror"
        );
        if let Some(endpoints) = self.endpoints.get_mut() {
            assert_eq!(
                endpoints.remove(overlay, EndpointKind::Start),
                Some(range.start())
            );
            assert_eq!(
                endpoints.remove(overlay, EndpointKind::End),
                Some(range.end())
            );
        }
        materialize_overlay_position(overlay, range);
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
                if let Some(endpoints) = self.endpoints.get_mut() {
                    assert_eq!(
                        endpoints.remove(overlay, EndpointKind::End),
                        Some(old_range.end())
                    );
                    assert!(endpoints.insert(new_range.end(), EndpointKind::End, overlay));
                }
                let previous = intervals
                    .update_end_preserving_order(overlay, new_range)
                    .expect("indexed overlay disappeared during end-only move");
                debug_assert_eq!(previous, old_range);
            }
            IndexedRegionChange::StartChanged => {
                let taken = intervals
                    .take(overlay)
                    .expect("indexed overlay disappeared during relocation");
                debug_assert_eq!(taken.0, old_range);
                assert!(
                    self.gnu_order.remove(OverlayIdentity::of(overlay)),
                    "relocated overlay missing from GNU order mirror"
                );
                if let Some(endpoints) = self.endpoints.get_mut() {
                    assert_eq!(
                        endpoints.remove(overlay, EndpointKind::Start),
                        Some(old_range.start())
                    );
                    assert_eq!(
                        endpoints.remove(overlay, EndpointKind::End),
                        Some(old_range.end())
                    );
                    assert!(endpoints.insert(new_range.start(), EndpointKind::Start, overlay));
                    assert!(endpoints.insert(new_range.end(), EndpointKind::End, overlay));
                }
                let inserted = intervals.insert(overlay, new_range);
                debug_assert!(inserted, "removed overlay retained an interval node");
                let order_inserted =
                    self.gnu_order
                        .insert_by(OverlayIdentity::of(overlay), |existing| {
                            new_range.start().cmp(
                                &intervals
                                    .range_by_identity(existing)
                                    .expect("GNU order mirror contains an unindexed overlay")
                                    .start(),
                            )
                        });
                assert!(
                    order_inserted,
                    "relocated overlay retained a GNU order node"
                );
            }
        }
        Some(old_range)
    }

    /// Apply an edit in `O(log n + k log n)`, where `k` is the number of
    /// overlays whose ranges touch the edited boundary.
    ///
    /// Whole intervals in the unaffected-prefix/affected-suffix partition are
    /// shifted by lazy subtree tags.  Only boundary-crossing intervals are
    /// removed and reinserted so endpoint gravity and evaporation remain
    /// GNU-compatible.
    pub(super) fn adjust_for_text_edit(&mut self, edit: OverlayTextEdit) -> Vec<OverlayEditEffect> {
        match edit {
            OverlayTextEdit::Insert {
                position,
                length,
                before_markers,
            } => self.adjust_for_insert(position, length, before_markers),
            OverlayTextEdit::Delete { range } => self.adjust_for_delete(range),
        }
    }

    fn adjust_for_insert(
        &mut self,
        position: EmacsBytePos,
        length: EmacsByteLen,
        before_markers: bool,
    ) -> Vec<OverlayEditEffect> {
        if length.is_empty() {
            return Vec::new();
        }
        let mut exceptions = self.overlays_touching(position);
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
        for identity in &front_preorder {
            assert!(
                self.gnu_order.remove(*identity),
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
        if let Some(endpoints) = self.endpoints.get_mut() {
            endpoints.shift_at_or_after(position, before_markers, delta);
        }

        let mut effects = Vec::with_capacity(detached.len());
        let mut front_updates = FxHashMap::default();
        for (overlay, old_range, attachment_order) in detached {
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
        }
        debug_assert!(front_updates.is_empty());
        effects
    }

    fn adjust_for_delete(&mut self, range: EmacsByteRange) -> Vec<OverlayEditEffect> {
        if range.is_empty() {
            return Vec::new();
        }
        let exceptions = self.deletion_exceptions(range);

        let mut detached = Vec::with_capacity(exceptions.len());
        for overlay in exceptions {
            if let Some((old_range, attachment_order)) = self.take_for_text_edit(overlay) {
                detached.push((overlay, old_range, attachment_order));
            }
        }

        let delta = EmacsByteDelta::deletion(range.len());
        self.intervals
            .write()
            .shift_at_or_after(range.end(), true, delta);
        if let Some(endpoints) = self.endpoints.get_mut() {
            endpoints.shift_at_or_after(range.end(), true, delta);
        }

        let mut effects = Vec::with_capacity(detached.len());
        let mut evaporated = Vec::new();
        for (overlay, old_range, attachment_order) in detached {
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
                evaporated.push((
                    IntervalKey {
                        start: new_range.start(),
                        attachment_order,
                    },
                    OverlayIdentity::of(overlay),
                ));
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
        evaporated.sort_unstable_by_key(|(key, _)| *key);
        for (_, identity) in evaporated.into_iter().rev() {
            assert!(
                self.gnu_order.remove(identity),
                "evaporated overlay missing from GNU order mirror"
            );
        }
        effects
    }

    fn deletion_exceptions(&self, range: EmacsByteRange) -> Vec<Value> {
        let mut exceptions = self.overlays_touching(range.start());
        // This is the same set the old endpoint walk produced: overlays
        // touching the deletion start plus intervals with either endpoint
        // strictly inside the deletion.  Passing a non-matching accessible
        // end deliberately excludes an empty overlay exactly at `range.end`,
        // which belongs to the lazily shifted suffix.
        exceptions.extend(self.overlays_in_region_iter(range, EmacsBytePos::new(usize::MAX)));
        sort_and_dedup_overlay_identities(&mut exceptions);
        exceptions
    }

    fn take_for_text_edit(&mut self, overlay: Value) -> Option<(EmacsByteRange, u64)> {
        let taken = self.intervals.write().take(overlay)?;
        let range = taken.0;
        if let Some(endpoints) = self.endpoints.get_mut() {
            assert_eq!(
                endpoints.remove(overlay, EndpointKind::Start),
                Some(range.start())
            );
            assert_eq!(
                endpoints.remove(overlay, EndpointKind::End),
                Some(range.end())
            );
        }
        Some(taken)
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
        if let Some(endpoints) = self.endpoints.get_mut() {
            assert!(endpoints.insert(range.start(), EndpointKind::Start, overlay));
            assert!(endpoints.insert(range.end(), EndpointKind::End, overlay));
        }
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
                order_inserted,
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
        self.endpoints = OnceLock::new();
    }

    pub(super) fn contains(&self, overlay: Value) -> bool {
        self.intervals.read().contains(overlay)
    }

    pub(super) fn range(&self, overlay: Value) -> Option<EmacsByteRange> {
        self.intervals
            .read()
            .range_by_identity(OverlayIdentity::of(overlay))
    }

    pub(super) fn len(&self) -> usize {
        self.intervals.read().len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.intervals.read().is_empty()
    }

    pub(super) fn values(&self) -> impl Iterator<Item = Value> + '_ {
        let records = RwLockReadGuard::map(self.intervals.read(), |tree| &tree.records);
        OrderedShiftTree::matches_owned(records, IntervalBPlusQuery::All)
            .map(|record| record.overlay)
    }

    pub(super) fn overlays_at(&self, pos: EmacsBytePos) -> Vec<Value> {
        let intervals = self.intervals.read();
        let mut overlays = Vec::new();
        intervals
            .records
            .for_each_match(IntervalBPlusQuery::Point(pos), |record| {
                overlays.push(record.overlay);
            });
        overlays
    }

    pub(super) fn overlays_at_iter(&self, pos: EmacsBytePos) -> impl Iterator<Item = Value> + '_ {
        let records = RwLockReadGuard::map(self.intervals.read(), |tree| &tree.records);
        OrderedShiftTree::matches_owned(records, IntervalBPlusQuery::Point(pos))
            .map(|record| record.overlay)
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

    pub(super) fn overlays_in_region_iter(
        &self,
        range: EmacsByteRange,
        accessible_end: EmacsBytePos,
    ) -> impl Iterator<Item = Value> + '_ {
        let records = RwLockReadGuard::map(self.intervals.read(), |tree| &tree.records);
        OrderedShiftTree::matches_owned(
            records,
            IntervalBPlusQuery::Region {
                range,
                accessible_end,
            },
        )
        .map(|record| record.overlay)
    }

    pub(super) fn all_ascending(&self) -> Vec<Value> {
        let mut overlays = Vec::with_capacity(self.len());
        self.intervals.read().all_ascending(&mut overlays);
        overlays
    }

    pub(super) fn endpoint_records_strictly_within(
        &self,
        bounds: EmacsByteRange,
        property_filter: OverlayPropertyFilter,
    ) -> OverlayEndpointRecords<'_> {
        self.endpoint_index()
            .records_strictly_within(bounds, property_filter)
    }

    pub(super) fn endpoint_records_strictly_within_reverse(
        &self,
        bounds: EmacsByteRange,
        property_filter: OverlayPropertyFilter,
    ) -> OverlayEndpointRecords<'_> {
        self.endpoint_index()
            .records_strictly_within_reverse(bounds, property_filter)
    }

    pub(super) fn overlay_properties_changed(&mut self, overlay: Value) {
        if let Some(endpoints) = self.endpoints.get_mut() {
            endpoints.refresh_overlay_property_mask(overlay);
        }
    }

    pub(super) fn next_boundary_after(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        if pos >= limit {
            return None;
        }
        self.with_endpoint_index(|endpoints| endpoints.next_after(pos, limit))
            .filter(|boundary| *boundary <= limit)
    }

    pub(super) fn previous_boundary_before(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        if pos <= limit {
            return None;
        }
        self.with_endpoint_index(|endpoints| endpoints.previous_before(pos, limit))
            .filter(|boundary| *boundary >= limit)
    }

    #[cfg(test)]
    pub(super) fn interval_height(&self) -> usize {
        self.intervals.read().height()
    }

    #[cfg(test)]
    pub(super) fn assert_invariants(&self) {
        let interval_len = {
            let intervals = self.intervals.read();
            intervals.assert_invariants();
            intervals.len()
        };
        if let Some(endpoints) = self.endpoints.get() {
            endpoints.assert_invariants();
            assert_eq!(interval_len * 2, endpoints.len());
        }
        self.gnu_order.assert_invariants();
        assert_eq!(interval_len, self.gnu_order.len());
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct IntervalKey {
    start: EmacsBytePos,
    attachment_order: u64,
}

impl Ord for IntervalKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.start
            .cmp(&other.start)
            .then_with(|| other.attachment_order.cmp(&self.attachment_order))
    }
}

impl PartialOrd for IntervalKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug)]
struct IntervalRecord {
    key: IntervalKey,
    identity: OverlayIdentity,
    range: EmacsByteRange,
    overlay: Value,
}

impl OrderedShiftRecord for IntervalRecord {
    type Identity = OverlayIdentity;
    type Key = IntervalKey;

    fn identity(self) -> Self::Identity {
        self.identity
    }

    fn key(self) -> Self::Key {
        self.key
    }

    fn key_position(self) -> EmacsBytePos {
        self.key.start
    }

    fn end_position(self) -> EmacsBytePos {
        self.range.end()
    }

    fn shifted(mut self, delta: EmacsByteDelta) -> Self {
        self.key.start = delta.apply_to_pos(self.key.start);
        self.range = delta.apply_to_range(self.range);
        self
    }

    fn shifted_key(mut key: Self::Key, delta: EmacsByteDelta) -> Self::Key {
        key.start = delta.apply_to_pos(key.start);
        key
    }
}

#[derive(Clone, Copy)]
enum IntervalBPlusQuery {
    Point(EmacsBytePos),
    ClosedPoint(EmacsBytePos),
    Region {
        range: EmacsByteRange,
        accessible_end: EmacsBytePos,
    },
    All,
}

impl OrderedTreeQuery<IntervalRecord> for IntervalBPlusQuery {
    fn subtree_may_match(
        self,
        minimum: EmacsBytePos,
        _maximum: EmacsBytePos,
        maximum_end: EmacsBytePos,
    ) -> bool {
        match self {
            Self::Point(position) => minimum <= position && maximum_end > position,
            Self::ClosedPoint(position) => minimum <= position && maximum_end >= position,
            Self::Region { range, .. } => minimum <= range.end() && maximum_end >= range.start(),
            Self::All => true,
        }
    }

    fn record_matches(self, record: IntervalRecord) -> bool {
        match self {
            Self::Point(position) => {
                record.range.start() <= position && position < record.range.end()
            }
            Self::ClosedPoint(position) => {
                record.range.start() <= position && position <= record.range.end()
            }
            Self::Region {
                range,
                accessible_end,
            } => ranges_overlap_region(record.range, range, accessible_end),
            Self::All => true,
        }
    }

    fn can_use_non_overlapping_single_path(self) -> bool {
        matches!(self, Self::Point(_))
    }

    fn maximum_end_is_too_small(self, maximum_end: EmacsBytePos) -> bool {
        match self {
            Self::Point(position) => maximum_end <= position,
            Self::ClosedPoint(position) => maximum_end < position,
            Self::Region { range, .. } => maximum_end < range.start(),
            Self::All => false,
        }
    }

    fn minimum_start_is_too_large(self, minimum_start: EmacsBytePos) -> bool {
        match self {
            Self::Point(position) | Self::ClosedPoint(position) => minimum_start > position,
            Self::Region { range, .. } => minimum_start > range.end(),
            Self::All => false,
        }
    }

    #[cfg(test)]
    fn record_subtree_visit(self) {
        if matches!(self, Self::Point(_)) {
            super::overlay::record_overlays_at_node_visit();
        }
    }
}

#[derive(Clone, Debug)]
struct IntervalBPlusTree {
    records: OrderedShiftTree<IntervalRecord>,
    next_attachment_order: u64,
}

impl IntervalBPlusTree {
    fn new() -> Self {
        Self {
            records: OrderedShiftTree::new(),
            next_attachment_order: 0,
        }
    }

    fn from_entries(entries: &[(Value, EmacsByteRange)], order: OverlayBatchOrder) -> Self {
        let last_attachment_order = entries.len().saturating_sub(1);
        let records = entries
            .iter()
            .enumerate()
            .map(|(index, (overlay, range))| IntervalRecord {
                key: IntervalKey {
                    start: range.start(),
                    attachment_order: match order {
                        OverlayBatchOrder::AttachmentSequence => index,
                        OverlayBatchOrder::AscendingQueryOrder => last_attachment_order - index,
                    } as u64,
                },
                identity: OverlayIdentity::of(*overlay),
                range: *range,
                overlay: *overlay,
            })
            .collect();
        Self {
            records: OrderedShiftTree::from_records(records),
            next_attachment_order: entries.len() as u64,
        }
    }

    fn insert(&mut self, overlay: Value, range: EmacsByteRange) -> bool {
        let attachment_order = self.next_attachment_order;
        self.next_attachment_order = self
            .next_attachment_order
            .checked_add(1)
            .expect("overlay attachment order exhausted");
        self.insert_with_order(overlay, range, attachment_order)
    }

    fn insert_with_order(
        &mut self,
        overlay: Value,
        range: EmacsByteRange,
        attachment_order: u64,
    ) -> bool {
        self.records.insert(IntervalRecord {
            key: IntervalKey {
                start: range.start(),
                attachment_order,
            },
            identity: OverlayIdentity::of(overlay),
            range,
            overlay,
        })
    }

    fn take(&mut self, overlay: Value) -> Option<(EmacsByteRange, u64)> {
        let record = self.records.remove(OverlayIdentity::of(overlay))?;
        Some((record.range, record.key.attachment_order))
    }

    fn update_end_preserving_order(
        &mut self,
        overlay: Value,
        range: EmacsByteRange,
    ) -> Option<EmacsByteRange> {
        let identity = OverlayIdentity::of(overlay);
        let current = self.records.record(identity)?;
        debug_assert_eq!(current.range.start(), range.start());
        let previous = self
            .records
            .replace_same_key(IntervalRecord { range, ..current })?;
        Some(previous.range)
    }

    fn range_by_identity(&self, identity: OverlayIdentity) -> Option<EmacsByteRange> {
        self.records.record(identity).map(|record| record.range)
    }

    fn contains(&self, overlay: Value) -> bool {
        self.records.contains(OverlayIdentity::of(overlay))
    }

    fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    fn entries(&self) -> impl Iterator<Item = (Value, EmacsByteRange)> + '_ {
        self.records
            .matches(IntervalBPlusQuery::All)
            .map(|record| (record.overlay, record.range))
    }

    fn endpoint_entries_in_attachment_order(&self) -> Vec<(Value, EndpointKind, EmacsBytePos)> {
        let mut records: Vec<_> = self.records.matches(IntervalBPlusQuery::All).collect();
        records.sort_unstable_by_key(|record| record.key.attachment_order);
        records
            .into_iter()
            .flat_map(|record| {
                [
                    (record.overlay, EndpointKind::Start, record.range.start()),
                    (record.overlay, EndpointKind::End, record.range.end()),
                ]
            })
            .collect()
    }

    fn overlays_touching(&self, position: EmacsBytePos, out: &mut Vec<Value>) {
        out.extend(
            self.records
                .matches(IntervalBPlusQuery::ClosedPoint(position))
                .map(|record| record.overlay),
        );
    }

    fn all_ascending(&self, out: &mut Vec<Value>) {
        out.extend(self.records.matches(IntervalBPlusQuery::All).map(|record| {
            #[cfg(test)]
            super::overlay::record_overlay_full_enumeration_visit();
            record.overlay
        }));
    }

    fn shift_at_or_after(
        &mut self,
        position: EmacsBytePos,
        inclusive: bool,
        delta: EmacsByteDelta,
    ) {
        self.records.shift_at_or_after(position, inclusive, delta);
    }

    #[cfg(test)]
    fn height(&self) -> usize {
        self.records.height()
    }

    #[cfg(test)]
    fn assert_invariants(&self) {
        self.records.assert_invariants();
    }

    fn len(&self) -> usize {
        self.records.len()
    }
}

fn ranges_overlap_region(
    overlay: EmacsByteRange,
    range: EmacsByteRange,
    accessible_end: EmacsBytePos,
) -> bool {
    if overlay.is_empty() {
        return overlay.start() == range.start()
            || (range.start() < overlay.start() && overlay.start() < range.end())
            || (overlay.start() == range.end() && range.end() == accessible_end);
    }
    if range.is_empty() {
        return overlay.start() < range.start() && range.start() < overlay.end();
    }
    overlay.start() < range.end() && overlay.end() > range.start()
}

#[cfg(test)]
#[path = "overlay_index_test.rs"]
mod tests;

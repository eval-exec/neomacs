//! Concrete overlay interval storage and read-only live endpoint handles.
//! Buffer retains attachment/mutation policy; no public handle mint is provided.

#[cfg(test)]
#[path = "tests/instrumentation.rs"]
pub(crate) mod instrumentation;
pub(crate) mod ordered;

use super::byte_positions::{EmacsByteDelta, EmacsBytePos, EmacsByteRange};
use crate::tagged::value::TaggedValue as Value;
use ordered::{OrderedRecordRelocation, OrderedShiftRecord, OrderedShiftTree, OrderedTreeQuery};
use parking_lot::RwLock;
use std::cmp::Ordering;
use std::sync::{Arc, Weak};

/// Meaning of the order in which a bulk publisher supplies overlay records.
///
/// GNU's `copy_overlays` walks the source tree in ascending query order and
/// then attaches each copy, so later records become newer at an equal start.
/// A pdump or immutable snapshot instead restores an already-observed query
/// order.  Making that distinction explicit prevents a linear rebuild from
/// silently reversing equal-start overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverlayBatchOrder {
    AttachmentSequence,
    AscendingQueryOrder,
}

/// Lisp object identity for index membership.
///
/// `Value`'s Rust equality is Lisp `equal`; overlay membership is GNU `eq`.
/// Keeping the raw tagged bits behind a distinct key type makes those two
/// domains impossible to mix at map/set call sites.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OverlayIdentity(usize);

impl OverlayIdentity {
    pub(crate) fn of(overlay: Value) -> Self {
        Self(overlay.bits())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum EndpointKind {
    Start,
    End,
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
    pub(crate) fn new(overlay: Value, intervals: &Arc<RwLock<IntervalBPlusTree>>) -> Self {
        Self {
            intervals: Arc::downgrade(intervals),
            identity: OverlayIdentity::of(overlay),
        }
    }

    pub(crate) fn current_range(&self) -> Option<EmacsByteRange> {
        let intervals = self.intervals.upgrade()?;
        intervals.read().range_by_identity(self.identity)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct IntervalKey {
    pub(crate) start: EmacsBytePos,
    pub(crate) attachment_order: u64,
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
pub(crate) struct IntervalRecord {
    pub(crate) key: IntervalKey,
    pub(crate) identity: OverlayIdentity,
    pub(crate) range: EmacsByteRange,
    pub(crate) overlay: Value,
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

/// Closed deletion-boundary query. END-start records are included so their
/// structural rank is assigned in the same restore pass as contracted starts.
/// This call-local query remains separate from ordinary hot point coverage.
#[derive(Clone, Copy)]
pub(crate) struct IntervalDeletionQuery(pub(crate) EmacsByteRange);

impl OrderedTreeQuery<IntervalRecord> for IntervalDeletionQuery {
    fn subtree_may_match(
        self,
        minimum: EmacsBytePos,
        _maximum: EmacsBytePos,
        maximum_end: EmacsBytePos,
    ) -> bool {
        minimum <= self.0.end() && maximum_end >= self.0.start()
    }
    fn record_matches(self, record: IntervalRecord) -> bool {
        record.range.start() <= self.0.end() && record.range.end() >= self.0.start()
    }
    fn maximum_end_is_too_small(self, maximum_end: EmacsBytePos) -> bool {
        maximum_end < self.0.start()
    }
    fn minimum_start_is_too_large(self, minimum_start: EmacsBytePos) -> bool {
        minimum_start > self.0.end()
    }
}

/// GNU itree.c:1200-1205 intersection, including empty records at BEGIN.
/// Query coordinates/cursor are local to the owning buffer's read borrow;
/// this independent query leaves ordinary point-coverage dispatch unchanged.
#[derive(Clone, Copy)]
pub(crate) struct IntervalIntersectionQuery(pub(crate) EmacsByteRange);

impl OrderedTreeQuery<IntervalRecord> for IntervalIntersectionQuery {
    fn subtree_may_match(
        self,
        minimum: EmacsBytePos,
        _maximum: EmacsBytePos,
        maximum_end: EmacsBytePos,
    ) -> bool {
        minimum <= self.0.end() && maximum_end >= self.0.start()
    }
    fn record_matches(self, record: IntervalRecord) -> bool {
        (self.0.start() < record.range.end() && record.range.start() < self.0.end())
            || (record.range.is_empty() && record.range.start() == self.0.start())
    }
    fn maximum_end_is_too_small(self, maximum_end: EmacsBytePos) -> bool {
        maximum_end < self.0.start()
    }
    fn minimum_start_is_too_large(self, minimum_start: EmacsBytePos) -> bool {
        minimum_start > self.0.end()
    }
}

#[derive(Clone, Copy)]
pub(crate) enum IntervalBPlusQuery {
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
            instrumentation::record_overlays_at_node_visit();
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct IntervalBPlusTree {
    pub(crate) records: OrderedShiftTree<IntervalRecord>,
    next_attachment_order: u64,
}

impl IntervalBPlusTree {
    pub(crate) fn new() -> Self {
        Self {
            records: OrderedShiftTree::new(),
            next_attachment_order: 0,
        }
    }

    pub(crate) fn from_entries(
        entries: &[(Value, EmacsByteRange)],
        order: OverlayBatchOrder,
    ) -> Self {
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

    /// Reserve call-local ranks under the buffer owner's exclusive mutation.
    /// Reversing this range preserves ascending structural order within new
    /// equal-start groups, with no per-overlay path allocation or second move.
    pub(crate) fn reserve_attachment_orders(&mut self, count: usize) -> std::ops::Range<u64> {
        let first = self.next_attachment_order;
        self.next_attachment_order = first
            .checked_add(count as u64)
            .expect("overlay attachment order exhausted");
        first..self.next_attachment_order
    }

    pub(crate) fn insert(&mut self, overlay: Value, range: EmacsByteRange) -> bool {
        let attachment_order = self.next_attachment_order;
        self.next_attachment_order = self
            .next_attachment_order
            .checked_add(1)
            .expect("overlay attachment order exhausted");
        self.insert_with_order(overlay, range, attachment_order)
    }

    pub(crate) fn insert_with_order(
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

    pub(crate) fn take(&mut self, overlay: Value) -> Option<(EmacsByteRange, u64)> {
        let record = self.records.remove(OverlayIdentity::of(overlay))?;
        Some((record.range, record.key.attachment_order))
    }

    /// Change real endpoints while retaining a neighbor-preserving leaf slot.
    ///
    /// Every accepted start change receives GNU's fresh equal-start order;
    /// an outside-neighbor attempt consumes no order so generic reinsertion
    /// assigns the same next serial exactly once. The caller retains the
    /// buffer owner's exclusive write guard and still mutates GNU topology.
    #[inline]
    pub(crate) fn try_move_preserving_neighbors(
        &mut self,
        overlay: Value,
        range: EmacsByteRange,
    ) -> Option<EmacsByteRange> {
        let attachment_order = self.next_attachment_order;
        let next_attachment_order = attachment_order
            .checked_add(1)
            .expect("overlay attachment order exhausted");
        let replacement = IntervalRecord {
            key: IntervalKey {
                start: range.start(),
                attachment_order,
            },
            identity: OverlayIdentity::of(overlay),
            range,
            overlay,
        };
        match self.records.replace_between_neighbors(replacement) {
            OrderedRecordRelocation::Replaced { previous } => {
                self.next_attachment_order = next_attachment_order;
                Some(previous.range)
            }
            OrderedRecordRelocation::OutsideNeighbors => None,
            OrderedRecordRelocation::Missing => {
                panic!("indexed overlay disappeared during local relocation")
            }
        }
    }

    pub(crate) fn update_end_preserving_order(
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

    pub(crate) fn range_by_identity(&self, identity: OverlayIdentity) -> Option<EmacsByteRange> {
        self.records.record(identity).map(|record| record.range)
    }

    pub(crate) fn contains(&self, overlay: Value) -> bool {
        self.records.contains(OverlayIdentity::of(overlay))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (Value, EmacsByteRange)> + '_ {
        self.records
            .matches(IntervalBPlusQuery::All)
            .map(|record| (record.overlay, record.range))
    }

    pub(crate) fn endpoint_entries_in_attachment_order(
        &self,
    ) -> Vec<(Value, EndpointKind, EmacsBytePos)> {
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

    pub(crate) fn overlays_touching(&self, position: EmacsBytePos, out: &mut Vec<Value>) {
        out.extend(
            self.records
                .matches(IntervalBPlusQuery::ClosedPoint(position))
                .map(|record| record.overlay),
        );
    }

    pub(crate) fn all_ascending(&self, out: &mut Vec<Value>) {
        out.extend(self.records.matches(IntervalBPlusQuery::All).map(|record| {
            #[cfg(test)]
            instrumentation::record_overlay_full_enumeration_visit();
            record.overlay
        }));
    }

    pub(crate) fn shift_at_or_after(
        &mut self,
        position: EmacsBytePos,
        inclusive: bool,
        delta: EmacsByteDelta,
    ) {
        self.records.shift_at_or_after(position, inclusive, delta);
    }

    #[cfg(test)]
    pub(crate) fn next_attachment_order_for_test(&self) -> u64 {
        self.next_attachment_order
    }

    #[cfg(test)]
    pub(crate) fn height(&self) -> usize {
        self.records.height()
    }

    #[cfg(test)]
    pub(crate) fn assert_invariants(&self) {
        self.records.assert_invariants();
    }

    pub(crate) fn len(&self) -> usize {
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

/// Resolve a live handle without retaining its buffer owner.
pub(crate) fn current_overlay_range(
    data: &crate::heap_types::OverlayData,
) -> Option<EmacsByteRange> {
    data.buffer?;
    data.position_handle.as_ref()?.current_range()
}

#[cfg(test)]
#[path = "tests/handle_test.rs"]
mod handle_tests;

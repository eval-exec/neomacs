//! Overlay system for buffers.
//!
//! GNU Emacs exposes overlays as first-class Lisp objects whose identity
//! outlives deletion. The buffer owns the interval index, while the overlay
//! object owns plist, buffer membership, and endpoint state. NeoVM models that
//! split by keeping overlay objects on the GC heap and storing only live object
//! ids in each buffer's overlay index.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};

use crate::buffer::BufferId;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::plist;
use crate::emacs_core::value::{Value, ValueKind, eq_value};
use crate::gc_trace::GcTrace;
use crate::heap_types::OverlayData;

pub use super::overlay_index::OverlayPropertyFilter;
use super::overlay_index::{
    EndpointKind, OverlayBatchOrder, OverlayEditEffect, OverlayEndpoint, OverlayEndpointRecords,
    OverlayIdentity, OverlayIndex, OverlayTextEdit,
};
use super::position::{EmacsByteLen, EmacsBytePos, EmacsByteRange};
use super::text::{TextEditRange, TextInsertion, TextReplacement};

pub type Overlay = OverlayData;

/// Caller-owned property semantics used by the core overlay precedence and
/// sweep machinery.
///
/// Layout uses this seam to compose category and alias lookup without moving
/// those policies into the interval index. Closures implement it directly,
/// while hot callers may use a concrete resolver and avoid dynamic dispatch.
pub trait OverlayPropertyResolver {
    fn value_for_overlay(&mut self, overlay: Value) -> Option<Value>;

    /// Conservative endpoint-index filter for this resolver. Arbitrary
    /// properties remain unfiltered; typed hot-property resolvers may opt into
    /// a mutation-maintained summary.
    fn endpoint_filter(&self) -> OverlayPropertyFilter {
        OverlayPropertyFilter::unfiltered()
    }
}

impl<F> OverlayPropertyResolver for F
where
    F: FnMut(Value) -> Option<Value>,
{
    fn value_for_overlay(&mut self, overlay: Value) -> Option<Value> {
        self(overlay)
    }
}

/// A property value proven to be non-nil.
///
/// This proof is required before an absent overlay property can initiate an
/// endpoint traversal.  It makes the redisplay performance contract explicit:
/// a wholly negative lookup has no operation capable of scanning the buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonNilPropertyValue(Value);

impl NonNilPropertyValue {
    pub fn new(value: Value) -> Option<Self> {
        (!value.is_nil()).then_some(Self(value))
    }

    pub fn value(self) -> Value {
        self.0
    }
}

#[cfg(test)]
static OVERLAYS_AT_NODE_VISITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_overlays_at_node_visit_count() {
    OVERLAYS_AT_NODE_VISITS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn overlays_at_node_visit_count() -> usize {
    OVERLAYS_AT_NODE_VISITS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn record_overlays_at_node_visit() {
    OVERLAYS_AT_NODE_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
static OVERLAY_ITERATOR_FRAME_PUSHES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_overlay_iterator_frame_push_count() {
    OVERLAY_ITERATOR_FRAME_PUSHES.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn overlay_iterator_frame_push_count() -> usize {
    OVERLAY_ITERATOR_FRAME_PUSHES.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn record_overlay_iterator_frame_push() {
    OVERLAY_ITERATOR_FRAME_PUSHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
static OVERLAY_PROPERTY_EXTENT_INSPECTIONS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_overlay_property_extent_inspection_count() {
    OVERLAY_PROPERTY_EXTENT_INSPECTIONS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn overlay_property_extent_inspection_count() -> usize {
    OVERLAY_PROPERTY_EXTENT_INSPECTIONS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
static BEST_OVERLAY_CANDIDATE_INSPECTIONS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_best_overlay_candidate_inspection_count() {
    BEST_OVERLAY_CANDIDATE_INSPECTIONS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn best_overlay_candidate_inspection_count() -> usize {
    BEST_OVERLAY_CANDIDATE_INSPECTIONS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
static OVERLAY_EDIT_CANDIDATE_INSPECTIONS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_overlay_edit_candidate_inspection_count() {
    OVERLAY_EDIT_CANDIDATE_INSPECTIONS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn overlay_edit_candidate_inspection_count() -> usize {
    OVERLAY_EDIT_CANDIDATE_INSPECTIONS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn record_overlay_edit_candidate_inspection() {
    OVERLAY_EDIT_CANDIDATE_INSPECTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
static OVERLAY_SHIFT_NODE_VISITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_overlay_shift_node_visit_count() {
    OVERLAY_SHIFT_NODE_VISITS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn overlay_shift_node_visit_count() -> usize {
    OVERLAY_SHIFT_NODE_VISITS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn record_overlay_shift_node_visit() {
    OVERLAY_SHIFT_NODE_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
static ENDPOINT_SEARCH_NODE_VISITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_endpoint_search_node_visit_count() {
    ENDPOINT_SEARCH_NODE_VISITS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn endpoint_search_node_visit_count() -> usize {
    ENDPOINT_SEARCH_NODE_VISITS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn record_endpoint_search_node_visit() {
    ENDPOINT_SEARCH_NODE_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
static ENDPOINT_SEARCH_SUMMARY_SHIFTS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_endpoint_search_summary_shift_count() {
    ENDPOINT_SEARCH_SUMMARY_SHIFTS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn endpoint_search_summary_shift_count() -> usize {
    ENDPOINT_SEARCH_SUMMARY_SHIFTS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn record_endpoint_search_summary_shift() {
    ENDPOINT_SEARCH_SUMMARY_SHIFTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
static ENDPOINT_PUBLICATION_INTERVAL_READS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_endpoint_publication_interval_read_count() {
    ENDPOINT_PUBLICATION_INTERVAL_READS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn endpoint_publication_interval_read_count() -> usize {
    ENDPOINT_PUBLICATION_INTERVAL_READS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn record_endpoint_publication_interval_read() {
    ENDPOINT_PUBLICATION_INTERVAL_READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
static OVERLAY_FULL_ENUMERATION_VISITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_overlay_full_enumeration_visit_count() {
    OVERLAY_FULL_ENUMERATION_VISITS.store(0, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn overlay_full_enumeration_visit_count() -> usize {
    OVERLAY_FULL_ENUMERATION_VISITS.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
pub(super) fn record_overlay_full_enumeration_visit() {
    OVERLAY_FULL_ENUMERATION_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub struct OverlayList {
    index: OverlayIndex,
}

#[derive(Clone, Copy)]
enum OverlayCloneIdentity {
    /// GNU `copy_overlays`: the destination owns distinct Lisp objects.
    Fresh,
    /// A read snapshot owns distinct objects and position handles, but keeps
    /// the precedence identity observed at snapshot time.
    PreservePrecedence,
}

impl Clone for OverlayList {
    /// GNU `copy_overlays`: clone each Lisp overlay object and publish a fresh
    /// buffer-owned index. Sharing either the object or a partially cloned
    /// index would let a move in one indirect buffer corrupt the other.
    fn clone(&self) -> Self {
        self.clone_with_identity(OverlayCloneIdentity::Fresh)
    }
}

impl OverlayList {
    /// Copy the list for an immutable observer such as redisplay.
    ///
    /// Snapshot overlays require independent position handles, so they cannot
    /// share the live Lisp objects. They do preserve each source object's
    /// precedence serial: changing that serial would change GNU's final
    /// `compare_overlays` tiebreak merely because redisplay took a snapshot.
    pub fn snapshot_clone(&self) -> Self {
        self.clone_with_identity(OverlayCloneIdentity::PreservePrecedence)
    }

    fn clone_with_identity(&self, identity: OverlayCloneIdentity) -> Self {
        let saved_roots = save_scratch_gc_roots();
        let entries: Vec<_> = self
            .index
            .all_ascending()
            .into_iter()
            .map(|overlay| {
                let data = overlay
                    .as_overlay_data()
                    .expect("overlay index contains a non-overlay value");
                let range = overlay_data_range(data);
                let plist = crate::emacs_core::builtins::builtin_copy_sequence(vec![data.plist])
                    .expect("a live overlay must carry a copyable plist");
                push_scratch_gc_root(plist);
                let copy = Value::make_overlay(OverlayData {
                    serial: match identity {
                        OverlayCloneIdentity::Fresh => 0,
                        OverlayCloneIdentity::PreservePrecedence => data.serial,
                    },
                    plist,
                    buffer: data.buffer,
                    start: range.start().get(),
                    end: range.end().get(),
                    position_handle: None,
                    front_advance: data.front_advance,
                    rear_advance: data.rear_advance,
                });
                push_scratch_gc_root(copy);
                (copy, range)
            })
            .collect();
        let mut cloned = Self::new();
        let batch_order = match identity {
            // GNU `copy_overlays` attaches copies in the source tree's
            // ascending traversal order, intentionally reversing equal-start
            // nodes in the destination tree.
            OverlayCloneIdentity::Fresh => OverlayBatchOrder::AttachmentSequence,
            // Redisplay snapshots are observers, so their raw query order must
            // remain identical to the live buffer.
            OverlayCloneIdentity::PreservePrecedence => OverlayBatchOrder::AscendingQueryOrder,
        };
        let attached = cloned.index.attach_batch(&entries, batch_order);
        debug_assert!(attached, "fresh overlay clone index rejected its batch");
        restore_scratch_gc_roots(saved_roots);
        cloned
    }
}

/// A non-nil overlay property together with the overlay that won GNU
/// precedence at one position.
///
/// Construction is private so an absent or nil property cannot be passed to an
/// exact-extent query.  Callers must discover a positive winner first.
#[derive(Clone, Copy, Debug)]
pub struct OverlayPropertyWinner {
    overlay: Value,
    value: NonNilPropertyValue,
}

impl PartialEq for OverlayPropertyWinner {
    fn eq(&self, other: &Self) -> bool {
        eq_value(&self.overlay, &other.overlay) && self.value == other.value
    }
}

impl Eq for OverlayPropertyWinner {}

impl OverlayPropertyWinner {
    fn new(overlay: Value, value: NonNilPropertyValue) -> Self {
        Self { overlay, value }
    }

    pub fn overlay(self) -> Value {
        self.overlay
    }

    pub fn value(self) -> Value {
        self.value.value()
    }
}

/// Result of a cheap overlay-property lookup at one position.
pub enum OverlayPropertyAtPoint<'a, R> {
    Present(OverlayPropertyResolution<'a, R>),
    Vacant(OverlayPropertyVacancy<'a, R>),
}

/// A positive overlay-property resolution, bound to the index and resolver
/// that proved it.
///
/// Its exact-extent operation consumes the resolution, so callers cannot mix
/// a winner with another overlay list, window, position, or property resolver.
pub struct OverlayPropertyResolution<'a, R> {
    overlays: &'a OverlayList,
    at: EmacsBytePos,
    window_id: Option<u64>,
    winner: OverlayPropertyWinner,
    active: ActivePropertyOverlays,
    property_value: R,
}

impl<R> OverlayPropertyResolution<'_, R> {
    pub fn winner(&self) -> OverlayPropertyWinner {
        self.winner
    }

    pub fn overlay(&self) -> Value {
        self.winner.overlay()
    }

    pub fn value(&self) -> Value {
        self.winner.value()
    }
}

impl<'a, R> OverlayPropertyResolution<'a, R>
where
    R: OverlayPropertyResolver,
{
    pub fn extent(mut self, bounds: EmacsByteRange) -> Option<OverlayPropertyExtent> {
        let range = self.overlays.property_frontier_extent(
            self.at,
            self.window_id,
            Some(self.winner.overlay()),
            self.active,
            bounds,
            &mut self.property_value,
        )?;
        Some(OverlayPropertyExtent {
            winner: self.winner,
            range,
        })
    }

    /// Consume this at-point proof as the initial frontier of a monotonic
    /// forward sweep. No second at-point lookup is performed.
    pub fn sweep(mut self, bounds: EmacsByteRange) -> Option<OverlayPropertySweep<'a, R>> {
        if bounds.is_empty() || self.at < bounds.start() || self.at >= bounds.end() {
            return None;
        }
        let semantic_start = self.overlays.property_frontier_start(
            self.at,
            self.window_id,
            Some(self.winner.overlay()),
            self.active.clone(),
            bounds,
            &mut self.property_value,
        )?;
        Some(OverlayPropertySweep::from_frontier(
            self.overlays,
            EmacsByteRange::new(self.at, bounds.end()),
            self.window_id,
            self.property_value,
            self.active,
            semantic_start,
        ))
    }
}

/// Proof that no window-visible overlay supplied a non-nil property at one
/// position, bound to the index and resolver that established the proof.
///
/// This type deliberately exposes no endpoint traversal. Most callers discard
/// it immediately. A caller with a proven non-nil text property can promote it
/// to [`OverlayPropertyFallback`], which permits a bounded sweep.
pub struct OverlayPropertyVacancy<'a, R> {
    overlays: &'a OverlayList,
    at: EmacsBytePos,
    window_id: Option<u64>,
    active: ActivePropertyOverlays,
    property_value: R,
}

impl<'a, R> OverlayPropertyVacancy<'a, R>
where
    R: OverlayPropertyResolver,
{
    pub fn with_fallback(self, fallback: NonNilPropertyValue) -> OverlayPropertyFallback<'a, R> {
        OverlayPropertyFallback {
            vacancy: self,
            fallback,
        }
    }
}

/// A vacant overlay lookup paired with a proven non-nil fallback property.
///
/// The fallback value is intentionally part of the type even though overlay
/// partitioning only needs its presence: construction is the positive proof
/// that makes a bounded shadowing traversal useful.
pub struct OverlayPropertyFallback<'a, R> {
    vacancy: OverlayPropertyVacancy<'a, R>,
    fallback: NonNilPropertyValue,
}

impl<'a, R> OverlayPropertyFallback<'a, R>
where
    R: OverlayPropertyResolver,
{
    /// Consume this positive fallback proof as the initial frontier of a
    /// bounded monotonic sweep.
    pub fn sweep(mut self, bounds: EmacsByteRange) -> Option<OverlayPropertySweep<'a, R>> {
        let _fallback_proof = self.fallback;
        if bounds.is_empty() || self.vacancy.at < bounds.start() || self.vacancy.at >= bounds.end()
        {
            return None;
        }
        let semantic_start = self.vacancy.overlays.property_frontier_start(
            self.vacancy.at,
            self.vacancy.window_id,
            None,
            self.vacancy.active.clone(),
            bounds,
            &mut self.vacancy.property_value,
        )?;
        Some(OverlayPropertySweep::from_frontier(
            self.vacancy.overlays,
            EmacsByteRange::new(self.vacancy.at, bounds.end()),
            self.vacancy.window_id,
            self.vacancy.property_value,
            self.vacancy.active,
            semantic_start,
        ))
    }
}

/// The maximal contiguous range over which one positive overlay-property
/// winner remains effective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayPropertyExtent {
    winner: OverlayPropertyWinner,
    range: EmacsByteRange,
}

impl OverlayPropertyExtent {
    pub fn winner(self) -> OverlayPropertyWinner {
        self.winner
    }

    pub fn overlay(self) -> Value {
        self.winner.overlay()
    }

    pub fn value(self) -> Value {
        self.winner.value()
    }

    pub fn range(self) -> EmacsByteRange {
        self.range
    }
}

/// One maximal partition of a bounded, monotonic overlay-property sweep.
///
/// `winner = None` is an explicitly bounded vacancy, not a claim about the
/// whole buffer.  Adjacent endpoint groups that leave the effective winner
/// unchanged are coalesced into one run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayPropertyRun {
    range: EmacsByteRange,
    winner: Option<OverlayPropertyWinner>,
}

impl OverlayPropertyRun {
    pub fn range(self) -> EmacsByteRange {
        self.range
    }

    pub fn winner(self) -> Option<OverlayPropertyWinner> {
        self.winner
    }
}

/// Stateful overlay-property partitions over one bounded display range.
///
/// Active overlays are initialized once. Endpoint records are then consumed
/// in ascending order from one B+ tree traversal; no per-boundary vectors or
/// predecessor/successor root searches are performed. The resolver is owned by
/// the sweep so all partitions use the same property semantics.
pub struct OverlayPropertySweep<'a, R>
where
    R: OverlayPropertyResolver,
{
    overlays: &'a OverlayList,
    bounds: EmacsByteRange,
    window_id: Option<u64>,
    property_value: R,
    active: ActivePropertyOverlays,
    endpoints: std::iter::Peekable<OverlayEndpointRecords<'a>>,
    cursor: EmacsBytePos,
    first_run_start: EmacsBytePos,
    pending_run_start: EmacsBytePos,
    last: Option<OverlayPropertyRun>,
}

impl<'a, R> OverlayPropertySweep<'a, R>
where
    R: OverlayPropertyResolver,
{
    #[cfg(test)]
    fn new(
        overlays: &'a OverlayList,
        bounds: EmacsByteRange,
        window_id: Option<u64>,
        mut property_value: R,
    ) -> Self {
        let active = if bounds.is_empty() {
            ActivePropertyOverlays::new()
        } else {
            overlays.active_property_overlays_at(bounds.start(), window_id, &mut property_value)
        };
        Self::from_frontier(
            overlays,
            bounds,
            window_id,
            property_value,
            active,
            bounds.start(),
        )
    }

    fn from_frontier(
        overlays: &'a OverlayList,
        bounds: EmacsByteRange,
        window_id: Option<u64>,
        property_value: R,
        active: ActivePropertyOverlays,
        first_run_start: EmacsBytePos,
    ) -> Self {
        let endpoint_filter = property_value.endpoint_filter();
        Self {
            overlays,
            bounds,
            window_id,
            property_value,
            active,
            endpoints: overlays
                .index
                .endpoint_records_strictly_within(bounds, endpoint_filter)
                .peekable(),
            cursor: bounds.start(),
            first_run_start,
            pending_run_start: first_run_start,
            last: None,
        }
    }

    fn restart(&mut self) {
        self.active = if self.bounds.is_empty() {
            ActivePropertyOverlays::new()
        } else {
            self.overlays.active_property_overlays_at(
                self.bounds.start(),
                self.window_id,
                &mut self.property_value,
            )
        };
        let endpoint_filter = self.property_value.endpoint_filter();
        self.endpoints = self
            .overlays
            .index
            .endpoint_records_strictly_within(self.bounds, endpoint_filter)
            .peekable();
        self.cursor = self.bounds.start();
        self.pending_run_start = self.first_run_start;
        self.last = None;
    }

    /// End of the endpoint range traversed by this sweep.
    pub fn traversal_end(&self) -> EmacsBytePos {
        self.bounds.end()
    }

    /// Return the partition containing `pos`, advancing monotonically when
    /// possible and restarting the bounded sweep only for a backwards seek.
    pub fn partition_at(&mut self, pos: EmacsBytePos) -> Option<OverlayPropertyRun> {
        if pos < self.bounds.start() || pos >= self.bounds.end() {
            return None;
        }
        if let Some(run) = self.last
            && run.range.start() <= pos
            && pos < run.range.end()
        {
            return Some(run);
        }
        if self.last.is_some_and(|run| pos < run.range.start()) || pos < self.cursor {
            self.restart();
        }
        self.by_ref()
            .find(|run| run.range.start() <= pos && pos < run.range.end())
    }
}

impl<R> Iterator for OverlayPropertySweep<'_, R>
where
    R: OverlayPropertyResolver,
{
    type Item = OverlayPropertyRun;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor >= self.bounds.end() {
            return None;
        }

        let start = self.pending_run_start;
        let winner = self.active.winner();
        loop {
            let boundary = self
                .endpoints
                .peek()
                .map_or(self.bounds.end(), |endpoint| endpoint.position);
            if boundary >= self.bounds.end() {
                self.cursor = self.bounds.end();
                let run = OverlayPropertyRun {
                    range: EmacsByteRange::new(start, self.bounds.end()),
                    winner,
                };
                self.pending_run_start = self.bounds.end();
                self.last = Some(run);
                return Some(run);
            }

            while self
                .endpoints
                .peek()
                .is_some_and(|endpoint| endpoint.position == boundary)
            {
                let endpoint = self.endpoints.next().expect("peeked overlay endpoint");
                self.active.apply_endpoint(
                    endpoint,
                    EndpointTraversalDirection::Forward,
                    self.window_id,
                    &mut self.property_value,
                );
            }
            self.cursor = boundary;
            if !same_overlay_identity(
                self.active.winner().map(OverlayPropertyWinner::overlay),
                winner.map(OverlayPropertyWinner::overlay),
            ) {
                let run = OverlayPropertyRun {
                    range: EmacsByteRange::new(start, boundary),
                    winner,
                };
                self.pending_run_start = boundary;
                self.last = Some(run);
                return Some(run);
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct OverlayByPrecedence(Value);

impl PartialEq for OverlayByPrecedence {
    fn eq(&self, other: &Self) -> bool {
        eq_value(&self.0, &other.0)
    }
}

impl Eq for OverlayByPrecedence {}

impl PartialOrd for OverlayByPrecedence {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OverlayByPrecedence {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_overlay_precedence(self.0, other.0)
    }
}

#[derive(Clone, Debug)]
struct ActivePropertyOverlays {
    active: BTreeMap<OverlayIdentity, (Value, NonNilPropertyValue)>,
    by_precedence: BinaryHeap<OverlayByPrecedence>,
}

#[derive(Clone, Copy)]
enum EndpointTraversalDirection {
    Forward,
    Reverse,
}

impl ActivePropertyOverlays {
    fn new() -> Self {
        Self {
            active: BTreeMap::new(),
            by_precedence: BinaryHeap::new(),
        }
    }

    fn inspect_and_insert(&mut self, overlay: Value, property_value: Option<Value>) {
        #[cfg(test)]
        OVERLAY_PROPERTY_EXTENT_INSPECTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(value) = property_value.and_then(NonNilPropertyValue::new)
            && self
                .active
                .insert(OverlayIdentity::of(overlay), (overlay, value))
                .is_none()
        {
            self.by_precedence.push(OverlayByPrecedence(overlay));
        }
    }

    fn remove(&mut self, overlay: Value) {
        self.active.remove(&OverlayIdentity::of(overlay));
    }

    fn apply_endpoint(
        &mut self,
        endpoint: OverlayEndpoint,
        direction: EndpointTraversalDirection,
        window_id: Option<u64>,
        property_value: &mut impl OverlayPropertyResolver,
    ) {
        if overlay_range(endpoint.overlay).is_none_or(|range| range.is_empty())
            || !overlay_applies_to_window(endpoint.overlay, window_id)
        {
            return;
        }
        match (direction, endpoint.kind) {
            (EndpointTraversalDirection::Forward, EndpointKind::Start)
            | (EndpointTraversalDirection::Reverse, EndpointKind::End) => self.inspect_and_insert(
                endpoint.overlay,
                property_value.value_for_overlay(endpoint.overlay),
            ),
            (EndpointTraversalDirection::Forward, EndpointKind::End)
            | (EndpointTraversalDirection::Reverse, EndpointKind::Start) => {
                self.remove(endpoint.overlay)
            }
        }
    }

    fn winner(&mut self) -> Option<OverlayPropertyWinner> {
        while self
            .by_precedence
            .peek()
            .is_some_and(|candidate| !self.active.contains_key(&OverlayIdentity::of(candidate.0)))
        {
            self.by_precedence.pop();
        }
        let overlay = self.by_precedence.peek()?.0;
        let (_, value) = self.active.get(&OverlayIdentity::of(overlay))?;
        Some(OverlayPropertyWinner::new(overlay, *value))
    }
}

impl OverlayList {
    pub fn new() -> Self {
        Self {
            index: OverlayIndex::new(),
        }
    }

    #[cfg(test)]
    pub(crate) fn interval_index_height(&self) -> usize {
        self.index.interval_height()
    }

    pub fn insert_overlay(&mut self, overlay: Value) {
        let data = overlay.as_overlay_data().unwrap();
        // Attach consumes the object's materialized coordinates. A live
        // overlay already owned by this index is rejected, while cross-buffer
        // moves materialize and rewrite the coordinates before reattaching.
        let range = EmacsByteRange::from_usize(data.start, data.end);
        self.index.attach(overlay, range);
    }

    pub fn detach_overlay(&mut self, overlay: Value) -> bool {
        self.index.detach(overlay).is_some()
    }

    pub fn delete_overlay(&mut self, overlay: Value) -> bool {
        if !self.detach_overlay(overlay) {
            return false;
        }
        let _ = overlay.with_overlay_data_mut(|data| {
            data.buffer = None;
        });
        true
    }

    pub fn delete_all_overlays(&mut self) {
        let live: Vec<Value> = self.index.values().collect();
        for overlay in live {
            let _ = overlay.with_overlay_data_mut(|data| {
                data.buffer = None;
            });
        }
        self.index.clear();
    }

    pub(crate) fn retarget_buffer(&mut self, from: BufferId, to: BufferId) {
        for overlay in self.index.values() {
            let _ = overlay.with_overlay_data_mut(|data| {
                if data.buffer == Some(from) {
                    data.buffer = Some(to);
                }
            });
        }
    }

    pub fn overlay_put(&mut self, overlay: Value, prop: Value, value: Value) -> Result<bool, Flow> {
        let changed = overlay
            .with_overlay_data_mut(|data| {
                let (plist, changed) = overlay_plist_put(data.plist, prop, value);
                data.plist = plist;
                Ok::<bool, Flow>(changed)
            })
            .unwrap()?;
        if changed {
            self.index.overlay_properties_changed(overlay);
        }
        Ok(changed)
    }

    pub fn overlay_get(&self, overlay: Value, prop: &Value) -> Option<Value> {
        plist::plist_get(overlay.as_overlay_data().unwrap().plist, prop)
    }

    pub fn overlay_get_named(&self, overlay: Value, prop_name: Value) -> Option<Value> {
        overlay_property_named(overlay, prop_name)
    }

    /// Whether `overlay`'s contributions apply in the window identified by
    /// `window_id` -- GNU `overlay_matches_window` (src/window.h):
    /// `! WINDOWP (window) || XWINDOW (window) == w`.
    ///
    /// Exposed because the rule must be the SAME one the property resolvers above
    /// apply: a path that collects overlays itself (overlay strings, which order
    /// by GNU `compare_overlay_entries` rather than by property precedence) still
    /// has to filter windowed overlays identically, and a second copy of the rule
    /// is how hl-line's per-window highlight leaked into every window before.
    pub fn overlay_applies_to_window(&self, overlay: Value, window_id: Option<u64>) -> bool {
        overlay_applies_to_window(overlay, window_id)
    }

    pub fn overlay_plist(&self, overlay: Value) -> Option<Value> {
        if self.index.contains(overlay) || overlay_live_buffer(overlay).is_none() {
            return Some(overlay.as_overlay_data().unwrap().plist);
        }
        None
    }

    pub fn overlay_start_emacs_byte_pos(&self, overlay: Value) -> Option<EmacsBytePos> {
        overlay_live_buffer(overlay)?;
        self.index.range(overlay).map(EmacsByteRange::start)
    }

    pub fn overlay_end_emacs_byte_pos(&self, overlay: Value) -> Option<EmacsBytePos> {
        overlay_live_buffer(overlay)?;
        self.index.range(overlay).map(EmacsByteRange::end)
    }

    pub fn move_overlay_to_emacs_byte_range(&mut self, overlay: Value, range: EmacsByteRange) {
        if self.index.move_to(overlay, range).is_none() {
            return;
        }
        let _ = overlay.with_overlay_data_mut(|data| {
            data.start = range.start().get();
            data.end = range.end().get();
        });
        // GNU Emacs drops empty overlays created by move-overlay when
        // `evaporate' is non-nil. Minibuffer shadow overlays depend on this
        // to avoid leaking stale before/after-strings into later prompts.
        if range.is_empty()
            && self
                .overlay_get_named(overlay, Value::symbol("evaporate"))
                .is_some_and(|value| value.is_truthy())
        {
            let _ = self.delete_overlay(overlay);
        }
    }

    pub fn overlays_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Vec<Value> {
        self.index.overlays_at(pos)
    }

    /// Borrow matching overlays without allocating an intermediate vector.
    pub fn iter_overlays_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> impl Iterator<Item = Value> + '_ {
        self.index.overlays_at_iter(pos)
    }

    pub fn overlays_in_emacs_byte_range(&self, range: EmacsByteRange) -> Vec<Value> {
        self.overlays_in_accessible_emacs_byte_range(range, range.end())
    }

    /// Return every live overlay of this buffer in GNU's `overlay-lists` order.
    ///
    /// Mirrors `Foverlay_lists` (buffer.c): the buffer's interval tree is
    /// walked `BEG..Z` descending and consed, producing all overlays in
    /// ascending `begin` order. Used to build the `(BEFORE . AFTER)` pair that
    /// `overlay-lists` returns; since Emacs 29.1 the "overlay center" is gone,
    /// so every overlay lands in the `BEFORE` (car) list and the `AFTER` (cdr)
    /// list is always empty.
    pub fn overlays_in_gnu_lists_order(&self) -> Vec<Value> {
        self.index.all_ascending()
    }

    /// A content digest of every live overlay: its span and its whole
    /// property list, hashed by VALUE rather than by identity.
    ///
    /// Redisplay's coarse signal is the overlay tick, which GNU also uses
    /// (`xdisp.c:22593` `if (w->last_overlay_modified != OVERLAY_MODIFF)
    /// GIVE_UP (200)`), and which cannot tell "the overlays changed" from
    /// "the same overlays were rebuilt".  Tooling rebuilds them constantly:
    /// an LSP client deletes and re-creates its diagnostic overlays on every
    /// buffer change, with the same spans and the same faces.  A digest lets
    /// a retained layout survive that, which GNU's own comment two lines
    /// below the give-up asks for and cannot have, having no per-window
    /// retained key to hang it on.
    ///
    /// Correctness rules for anyone extending this: it must cover EVERYTHING
    /// redisplay can read from an overlay, so it hashes the entire plist
    /// rather than a chosen set of display properties, and it must not use
    /// `sxhash`, whose `equal` form stops after a bounded depth and length
    /// (GNU `SXHASH_MAX_LEN`) and would therefore be systematically blind to
    /// a change in the eighth property.  Values it cannot hash structurally
    /// are folded in by identity, which can only make the digest differ more
    /// often -- over-invalidating costs a redisplay, under-invalidating
    /// leaves a stale frame on screen.
    pub fn content_digest(&self) -> Option<u64> {
        use std::hash::Hasher;

        let properties = DisplayOverlayProperties::intern();
        let mut hasher = rustc_hash::FxHasher::default();
        for overlay in self.index.all_ascending() {
            hasher.write_u64(
                self.overlay_start_emacs_byte_pos(overlay)
                    .map_or(u64::MAX, |p| p.get() as u64),
            );
            hasher.write_u64(
                self.overlay_end_emacs_byte_pos(overlay)
                    .map_or(u64::MAX, |p| p.get() as u64),
            );
            let Some(plist) = self.overlay_plist(overlay) else {
                hasher.write_u8(0xff);
                continue;
            };
            let mut cursor = plist;
            while cursor.is_cons() {
                let key = cursor.cons_car();
                let rest = cursor.cons_cdr();
                if !rest.is_cons() {
                    break;
                }
                match properties.classify(key) {
                    OverlayPropertyRelevance::CategoryIndirection => return None,
                    OverlayPropertyRelevance::Rendered => {
                        hash_value_content(key, 0, &mut hasher);
                        hash_value_content(rest.cons_car(), 0, &mut hasher);
                    }
                    OverlayPropertyRelevance::Unrendered => {}
                }
                cursor = rest.cons_cdr();
            }
        }
        Some(hasher.finish())
    }

    pub fn overlays_in_accessible_emacs_byte_range(
        &self,
        range: EmacsByteRange,
        accessible_end: EmacsBytePos,
    ) -> Vec<Value> {
        self.iter_overlays_in_accessible_emacs_byte_range(range, accessible_end)
            .collect()
    }

    /// Borrow region matches without allocating an intermediate vector.
    pub fn iter_overlays_in_accessible_emacs_byte_range(
        &self,
        range: EmacsByteRange,
        accessible_end: EmacsBytePos,
    ) -> impl Iterator<Item = Value> + '_ {
        self.index.overlays_in_region_iter(range, accessible_end)
    }

    pub fn highest_priority_overlay_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        property: Value,
    ) -> Option<Value> {
        self.best_overlay_among(
            property,
            self.index.overlays_at_iter(pos),
            |overlay| overlay_covers_pos(overlay, pos),
            &overlay_plist_property,
        )
    }

    /// Select the highest-precedence window-visible overlay whose caller-owned
    /// property resolver returns a non-nil value.
    ///
    /// The closure keeps category/alias lookup outside the interval index while
    /// this module remains the sole owner of GNU overlay precedence and window
    /// filtering.  The typed result retains both the winning object and value.
    pub fn resolve_overlay_property_at_emacs_byte_pos<R>(
        &self,
        pos: EmacsBytePos,
        window_id: Option<u64>,
        mut property_value: R,
    ) -> OverlayPropertyAtPoint<'_, R>
    where
        R: OverlayPropertyResolver,
    {
        let mut active = self.active_property_overlays_at(pos, window_id, &mut property_value);
        match active.winner() {
            Some(winner) => OverlayPropertyAtPoint::Present(OverlayPropertyResolution {
                overlays: self,
                at: pos,
                window_id,
                winner,
                active,
                property_value,
            }),
            None => OverlayPropertyAtPoint::Vacant(OverlayPropertyVacancy {
                overlays: self,
                at: pos,
                window_id,
                active,
                property_value,
            }),
        }
    }

    fn active_property_overlays_at(
        &self,
        pos: EmacsBytePos,
        window_id: Option<u64>,
        property_value: &mut impl OverlayPropertyResolver,
    ) -> ActivePropertyOverlays {
        let mut active = ActivePropertyOverlays::new();
        for overlay in self.iter_overlays_at_emacs_byte_pos(pos) {
            if overlay_applies_to_window(overlay, window_id) {
                active.inspect_and_insert(overlay, property_value.value_for_overlay(overlay));
            }
        }
        active
    }

    /// Create a bounded monotonic property sweep for redisplay.
    ///
    /// Unlike an exact at-point extent query, this initializes active overlays
    /// once at `bounds.start()` and streams endpoint groups in ascending order.
    #[cfg(test)]
    pub(crate) fn overlay_property_sweep<R>(
        &self,
        bounds: EmacsByteRange,
        window_id: Option<u64>,
        property_value: R,
    ) -> OverlayPropertySweep<'_, R>
    where
        R: OverlayPropertyResolver,
    {
        OverlayPropertySweep::new(self, bounds, window_id, property_value)
    }

    /// GNU `get_char_property_and_overlay` (src/textprop.c): PROPERTY's value from
    /// the highest-precedence overlay at POS that carries it, with
    /// window-specific overlays filtered against `window_id`.
    ///
    /// `None` means *no* overlay carries the property, and only then may the
    /// caller fall back to the **text** property. An overlay that carries it
    /// SHADOWS the text property outright: the value never merges with the
    /// text-property value, and no lower-precedence overlay gets a say -- not even
    /// when the winner's value happens to mean "inactive" (an `invisible` value
    /// absent from `buffer-invisibility-spec`, say). That is the policy for
    /// `display`, `invisible`, `fontified` and `mouse-face`. `face` is the sole
    /// exception and uses
    /// [`Self::overlay_property_values_ascending_at_emacs_byte_pos`].
    ///
    /// This is the value-only form for callers whose runs are already bounded
    /// at every overlay boundary and therefore do not need an extent query or
    /// monotonic sweep.
    pub fn highest_priority_overlay_property_value_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        property: Value,
        window_id: Option<u64>,
    ) -> Option<Value> {
        match self.resolve_overlay_property_at_emacs_byte_pos(pos, window_id, |overlay| {
            overlay_property_named(overlay, property)
        }) {
            OverlayPropertyAtPoint::Present(resolution) => Some(resolution.value()),
            OverlayPropertyAtPoint::Vacant(_) => None,
        }
    }

    /// Single-winner overlay lookup with GNU `overlay-get` alias semantics.
    /// The canonical property is first in `property_lookup_order`, followed by
    /// its `char-property-alias-alist` fallbacks. Property lookup happens per
    /// overlay before the highest-precedence carrier is selected.
    pub fn highest_priority_overlay_effective_property_value_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        property_lookup_order: &[Value],
        window_id: Option<u64>,
    ) -> Option<Value> {
        match self.resolve_overlay_property_at_emacs_byte_pos(pos, window_id, |overlay| {
            overlay_property_in_lookup_order(overlay, property_lookup_order)
        }) {
            OverlayPropertyAtPoint::Present(resolution) => Some(resolution.value()),
            OverlayPropertyAtPoint::Vacant(_) => None,
        }
    }

    /// The overlay half of GNU `face_at_buffer_position` (src/xfaces.c): every
    /// window-visible overlay's PROPERTY value at POS in ASCENDING precedence
    /// (`sort_overlays` order), for the one policy where overlay values MERGE
    /// instead of shadowing -- `face`. Higher precedence merges last and so wins.
    ///
    /// Ordering is GNU `compare_overlays`: priority, then containment weighed
    /// against the secondary priority of a `(PRIMARY . SECONDARY)` `priority`
    /// value, then a stable tiebreak. A bare `priority`-integer comparison is not
    /// equivalent -- it silently reads a cons `priority` as 0 and drops the
    /// containment rule.
    pub fn overlay_property_values_ascending_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        property: Value,
        window_id: Option<u64>,
    ) -> Vec<Value> {
        self.overlay_effective_property_values_ascending_at_emacs_byte_pos(
            pos,
            std::slice::from_ref(&property),
            window_id,
        )
    }

    /// GNU `overlay-get` property lookup composed with the ascending overlay
    /// merge order used by `face_at_buffer_position`.
    ///
    /// `property_lookup_order` contains the canonical property first followed
    /// by its `char-property-alias-alist` fallbacks.  Lookup happens WITHIN
    /// each overlay before overlay precedence is considered: a high-priority
    /// overlay carrying an alias must still merge after a lower-priority
    /// overlay carrying the canonical name.  Querying each name across all
    /// overlays separately would reverse that GNU ordering.
    pub fn overlay_effective_property_values_ascending_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        property_lookup_order: &[Value],
        window_id: Option<u64>,
    ) -> Vec<Value> {
        let mut carriers: Vec<Value> = self
            .iter_overlays_at_emacs_byte_pos(pos)
            .filter(|overlay| {
                overlay_applies_to_window(*overlay, window_id)
                    && overlay_property_in_lookup_order(*overlay, property_lookup_order)
                        .is_some_and(|value| !value.is_nil())
            })
            .collect();
        // One carrier is the common case on a redisplay line, and it is
        // already in order: key nothing and allocate nothing for it.
        if carriers.len() > 1 {
            sort_overlays_by_precedence_ascending(&mut carriers);
        }
        carriers
            .into_iter()
            .filter_map(|overlay| overlay_property_in_lookup_order(overlay, property_lookup_order))
            .collect()
    }

    /// Sweep outward from an already-resolved point until the winning overlay
    /// identity changes.
    fn property_frontier_extent(
        &self,
        pos: EmacsBytePos,
        window_id: Option<u64>,
        target: Option<Value>,
        mut active: ActivePropertyOverlays,
        bounds: EmacsByteRange,
        property_value: &mut impl OverlayPropertyResolver,
    ) -> Option<EmacsByteRange> {
        if bounds.is_empty() || pos < bounds.start() || pos >= bounds.end() {
            return None;
        }

        debug_assert!(same_overlay_identity(
            active.winner().map(OverlayPropertyWinner::overlay),
            target,
        ));

        let start = self.property_frontier_start(
            pos,
            window_id,
            target,
            active.clone(),
            bounds,
            property_value,
        )?;

        let mut end = bounds.end();
        let forward_bounds = EmacsByteRange::new(pos, bounds.end());
        let endpoint_filter = property_value.endpoint_filter();
        let mut endpoints = self
            .index
            .endpoint_records_strictly_within(forward_bounds, endpoint_filter)
            .peekable();
        while let Some(boundary) = endpoints.peek().map(|endpoint| endpoint.position) {
            while endpoints
                .peek()
                .is_some_and(|endpoint| endpoint.position == boundary)
            {
                let endpoint = endpoints.next().expect("peeked overlay endpoint");
                active.apply_endpoint(
                    endpoint,
                    EndpointTraversalDirection::Forward,
                    window_id,
                    property_value,
                );
            }
            if !same_overlay_identity(active.winner().map(OverlayPropertyWinner::overlay), target) {
                end = boundary;
                break;
            }
        }

        Some(EmacsByteRange::new(start, end))
    }

    /// Find the semantic start of the at-point winner with one reverse
    /// endpoint traversal.
    ///
    /// Crossing a boundary backwards reverses the forward update: starts are
    /// removed and ends are inserted. The B+ tree is searched once and then
    /// streamed in descending order, avoiding a root search and allocation at
    /// every predecessor boundary.
    fn property_frontier_start(
        &self,
        pos: EmacsBytePos,
        window_id: Option<u64>,
        target: Option<Value>,
        mut active: ActivePropertyOverlays,
        bounds: EmacsByteRange,
        property_value: &mut impl OverlayPropertyResolver,
    ) -> Option<EmacsBytePos> {
        if bounds.is_empty() || pos < bounds.start() || pos >= bounds.end() {
            return None;
        }

        debug_assert!(same_overlay_identity(
            active.winner().map(OverlayPropertyWinner::overlay),
            target,
        ));

        let mut start = bounds.start();
        let reverse_bounds = EmacsByteRange::new(
            bounds.start(),
            EmacsBytePos::new(pos.get().saturating_add(1)),
        );
        let endpoint_filter = property_value.endpoint_filter();
        let mut endpoints = self
            .index
            .endpoint_records_strictly_within_reverse(reverse_bounds, endpoint_filter)
            .peekable();
        while let Some(boundary) = endpoints.peek().map(|endpoint| endpoint.position) {
            while endpoints
                .peek()
                .is_some_and(|endpoint| endpoint.position == boundary)
            {
                let endpoint = endpoints.next().expect("peeked overlay endpoint");
                active.apply_endpoint(
                    endpoint,
                    EndpointTraversalDirection::Reverse,
                    window_id,
                    property_value,
                );
            }
            if !same_overlay_identity(active.winner().map(OverlayPropertyWinner::overlay), target) {
                start = boundary;
                break;
            }
        }
        Some(start)
    }

    pub fn highest_priority_overlay_for_inserted_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        property: &Value,
    ) -> Option<Value> {
        self.highest_priority_overlay_for_inserted_emacs_byte_pos_with(
            pos,
            property,
            &overlay_plist_property,
        )
    }

    /// [`Self::highest_priority_overlay_for_inserted_emacs_byte_pos`] reading
    /// `priority' the way the caller says (see [`OverlayPriorityLookup`]).
    pub fn highest_priority_overlay_for_inserted_emacs_byte_pos_with(
        &self,
        pos: EmacsBytePos,
        property: &Value,
        value_of: OverlayPropertyLookup,
    ) -> Option<Value> {
        self.best_overlay_among(
            *property,
            self.index.overlays_touching(pos),
            |overlay| {
                let Some(data) = overlay.as_overlay_data() else {
                    return false;
                };
                if data.buffer.is_none() {
                    return false;
                }
                let range = overlay_data_range(data);
                !(range.start() == pos && data.front_advance
                    || range.end() == pos && !data.rear_advance)
                    && range.start() <= pos
                    && pos <= range.end()
            },
            value_of,
        )
    }

    pub fn sort_overlay_ids_by_priority_desc(&self, overlay_ids: &mut [Value]) {
        self.sort_overlay_ids_by_priority_desc_with(overlay_ids, &overlay_plist_priority);
    }

    /// [`Self::sort_overlay_ids_by_priority_desc`] reading `priority' the way
    /// the caller says -- which is how a caller holding an obarray lets the
    /// `category' fallback decide, as GNU's `sort_overlays' does.
    pub fn sort_overlay_ids_by_priority_desc_with(
        &self,
        overlay_ids: &mut [Value],
        priority_of: OverlayPriorityLookup,
    ) {
        if overlay_ids.len() < 2 {
            return;
        }
        // Key once per overlay, then sort (see `OverlayPrecedence`).
        let mut keyed: Vec<(Value, Option<OverlayPrecedence>)> = overlay_ids
            .iter()
            .map(|overlay| (*overlay, overlay_precedence(*overlay, priority_of)))
            .collect();
        keyed.sort_by(|(left, left_key), (right, right_key)| {
            compare_overlay_precedence_keys(*right, *right_key, *left, *left_key)
        });
        for (slot, (overlay, _)) in overlay_ids.iter_mut().zip(keyed) {
            *slot = overlay;
        }
    }

    pub fn adjust_for_insert_at_emacs_byte_pos(
        &mut self,
        pos: EmacsBytePos,
        len: EmacsByteLen,
        before_markers: bool,
    ) {
        if len.is_empty() {
            return;
        }
        let effects = self.index.adjust_for_text_edit(OverlayTextEdit::Insert {
            position: pos,
            length: len,
            before_markers,
        });
        self.apply_edit_effects(effects);
    }

    pub fn adjust_for_inserted_text(&mut self, insertion: TextInsertion, before_markers: bool) {
        self.adjust_for_insert_at_emacs_byte_pos(
            insertion.byte_pos(),
            insertion.extent().emacs_bytes(),
            before_markers,
        );
    }

    pub fn adjust_for_delete_emacs_byte_range(&mut self, range: EmacsByteRange) {
        if range.is_empty() {
            return;
        }
        let effects = self
            .index
            .adjust_for_text_edit(OverlayTextEdit::Delete { range });
        self.apply_edit_effects(effects);
    }

    fn apply_edit_effects(&mut self, effects: Vec<OverlayEditEffect>) {
        for effect in effects {
            match effect {
                OverlayEditEffect::Resized { overlay, range } => {
                    let _ = overlay.with_overlay_data_mut(|object| {
                        object.start = range.start().get();
                        object.end = range.end().get();
                    });
                }
                OverlayEditEffect::Evaporated {
                    overlay,
                    collapsed_at,
                } => {
                    let _ = overlay.with_overlay_data_mut(|object| {
                        object.start = collapsed_at.get();
                        object.end = collapsed_at.get();
                        object.buffer = None;
                    });
                }
            }
        }
    }

    pub fn adjust_for_deleted_text(&mut self, range: TextEditRange) {
        self.adjust_for_delete_emacs_byte_range(range.byte_range());
    }

    pub fn adjust_for_replace_at_emacs_byte_pos(
        &mut self,
        start: EmacsBytePos,
        old_len: EmacsByteLen,
        new_len: EmacsByteLen,
    ) {
        if old_len.is_empty() {
            self.adjust_for_insert_at_emacs_byte_pos(start, new_len, false);
            return;
        }

        self.adjust_for_insert_at_emacs_byte_pos(start.add_len(old_len), new_len, true);
        self.adjust_for_delete_emacs_byte_range(EmacsByteRange::from_start_len(start, old_len));
    }

    pub fn adjust_for_replaced_text(&mut self, replacement: TextReplacement) {
        self.adjust_for_replace_at_emacs_byte_pos(
            replacement.byte_start(),
            replacement.old_byte_len(),
            replacement.new_byte_len(),
        );
    }

    pub fn set_front_advance(&mut self, overlay: Value, advance: bool) {
        let _ = overlay.with_overlay_data_mut(|data| {
            data.front_advance = advance;
        });
    }

    pub fn set_rear_advance(&mut self, overlay: Value, advance: bool) {
        let _ = overlay.with_overlay_data_mut(|data| {
            data.rear_advance = advance;
        });
    }

    pub fn get(&self, overlay: Value) -> Option<Overlay> {
        self.index
            .contains(overlay)
            .then(|| overlay.as_overlay_data().unwrap().clone())
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn next_boundary_after_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<EmacsBytePos> {
        self.next_boundary_after_until_emacs_byte_pos(pos, EmacsBytePos::new(usize::MAX))
    }

    pub fn next_boundary_after_until_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.index.next_boundary_after(pos, limit)
    }

    pub fn previous_boundary_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.previous_boundary_before_since_emacs_byte_pos(pos, EmacsBytePos::ZERO)
    }

    pub fn previous_boundary_before_since_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.index.previous_boundary_before(pos, limit)
    }

    pub(crate) fn dump_overlays(&self) -> Vec<Value> {
        self.index.values().collect()
    }

    pub(crate) fn from_dump(overlays: Vec<Value>) -> Self {
        let mut list = Self::new();
        let entries: Vec<_> = overlays
            .into_iter()
            .filter_map(|overlay| {
                let data = overlay.as_overlay_data()?;
                data.buffer?;
                Some((
                    overlay,
                    EmacsByteRange::new(EmacsBytePos::new(data.start), EmacsBytePos::new(data.end)),
                ))
            })
            .collect();
        let attached = list
            .index
            .attach_batch(&entries, OverlayBatchOrder::AscendingQueryOrder);
        debug_assert!(attached, "fresh overlay list rejected its dump batch");
        list
    }

    fn best_overlay_among<I, F>(
        &self,
        property: Value,
        candidates: I,
        predicate: F,
        value_of: OverlayPropertyLookup,
    ) -> Option<Value>
    where
        I: IntoIterator<Item = Value>,
        F: Fn(Value) -> bool,
    {
        let priority_of = |overlay: Value| value_of(overlay, priority_value());
        let priority_of: OverlayPriorityLookup = &priority_of;
        let mut best: Option<Value> = None;
        for overlay in candidates {
            #[cfg(test)]
            BEST_OVERLAY_CANDIDATE_INSPECTIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if !predicate(overlay) {
                continue;
            }
            let Some(value) = value_of(overlay, property) else {
                continue;
            };
            if value.is_nil() {
                continue;
            }
            match best {
                None => best = Some(overlay),
                Some(current)
                    if compare_overlay_precedence_keys(
                        current,
                        overlay_precedence(current, priority_of),
                        overlay,
                        overlay_precedence(overlay, priority_of),
                    ) == Ordering::Less =>
                {
                    best = Some(overlay);
                }
                _ => {}
            }
        }
        best
    }
}

fn overlay_live_buffer(overlay: Value) -> Option<crate::buffer::BufferId> {
    overlay.as_overlay_data().and_then(|d| d.buffer)
}

fn overlay_data_range(data: &OverlayData) -> EmacsByteRange {
    let (start, end) = data.current_range();
    EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(end))
}

fn overlay_range(overlay: Value) -> Option<EmacsByteRange> {
    let data = overlay.as_overlay_data()?;
    data.buffer.map(|_| overlay_data_range(data))
}

fn overlay_covers_pos(overlay: Value, pos: EmacsBytePos) -> bool {
    let Some(data) = overlay.as_overlay_data() else {
        return false;
    };
    if data.buffer.is_none() {
        return false;
    }
    let range = overlay_data_range(data);
    range.start() <= pos && pos < range.end()
}

fn same_overlay_identity(left: Option<Value>, right: Option<Value>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => eq_value(&left, &right),
        (None, None) => true,
        _ => false,
    }
}

fn overlay_property_named(overlay: Value, prop_name: Value) -> Option<Value> {
    let plist = overlay.as_overlay_data()?.plist;
    plist::plist_get(plist, &prop_name)
}

fn overlay_property_in_lookup_order(
    overlay: Value,
    property_lookup_order: &[Value],
) -> Option<Value> {
    let (canonical, aliases) = property_lookup_order.split_first()?;

    // GNU `lookup_char_property` returns a directly present canonical value
    // immediately, even when that value is nil.  A canonical nil therefore
    // blocks alias fallback; the caller subsequently decides that this
    // overlay is not a carrier and may continue to a lower overlay.
    if let Some(value) = overlay_property_named(overlay, *canonical) {
        return Some(value);
    }

    // Aliases are fallback candidates: GNU keeps scanning while their values
    // are nil and returns the first non-nil one.
    aliases.iter().find_map(|property| {
        overlay_property_named(overlay, *property).filter(|value| !value.is_nil())
    })
}

/// Whether `overlay`'s contributions apply in the window being laid out. GNU
/// restricts an overlay carrying a `window` property to that window only (e.g.
/// hl-line with a non-sticky flag). A missing or non-window `window` property is
/// unrestricted, and `window_id == None` (no window context) applies every
/// overlay. Mirrors the layout engine's same-named check, one abstraction level
/// down (raw overlay `Value` rather than a buffer view).
fn overlay_applies_to_window(overlay: Value, window_id: Option<u64>) -> bool {
    let Some(window_prop) = overlay_property_named(overlay, Value::symbol("window")) else {
        return true;
    };
    let Some(target) = window_prop.as_window_id() else {
        return true;
    };
    window_id.is_none_or(|current| current == target)
}

/// Everything `compare_overlay_precedence` reads from one overlay: its
/// priorities, its range and its identity.  A sort reads this once per
/// overlay instead of recomputing the range and walking the plist on every
/// comparison (GNU `sort_overlays` likewise loads each overlay once).
#[derive(Clone, Copy)]
struct OverlayPrecedence {
    priority: i64,
    subpriority: i64,
    range: EmacsByteRange,
    identity: u64,
}

fn overlay_precedence(
    overlay: Value,
    priority_of: OverlayPriorityLookup,
) -> Option<OverlayPrecedence> {
    let data = overlay.as_overlay_data().filter(|d| d.buffer.is_some())?;
    let (priority, subpriority) = overlay_priority(overlay, priority_of);
    Some(OverlayPrecedence {
        priority,
        subpriority,
        range: overlay_data_range(data),
        identity: overlay_identity_key(overlay),
    })
}

/// Order `overlays` by ascending precedence, reading each overlay's key once.
fn sort_overlays_by_precedence_ascending(overlays: &mut [Value]) {
    let mut keyed: Vec<(Value, Option<OverlayPrecedence>)> = overlays
        .iter()
        .map(|overlay| {
            (
                *overlay,
                overlay_precedence(*overlay, &overlay_plist_priority),
            )
        })
        .collect();
    keyed.sort_by(|(left, left_key), (right, right_key)| {
        compare_overlay_precedence_keys(*left, *left_key, *right, *right_key)
    });
    for (slot, (overlay, _)) in overlays.iter_mut().zip(keyed) {
        *slot = overlay;
    }
}

fn compare_overlay_precedence(left: Value, right: Value) -> Ordering {
    compare_overlay_precedence_keys(
        left,
        overlay_precedence(left, &overlay_plist_priority),
        right,
        overlay_precedence(right, &overlay_plist_priority),
    )
}

fn compare_overlay_precedence_keys(
    left: Value,
    left_key: Option<OverlayPrecedence>,
    right: Value,
    right_key: Option<OverlayPrecedence>,
) -> Ordering {
    let Some(left_key) = left_key else {
        return Ordering::Less;
    };
    let Some(right_key) = right_key else {
        return Ordering::Greater;
    };
    let (left_priority, left_subpriority) = (left_key.priority, left_key.subpriority);
    let (right_priority, right_subpriority) = (right_key.priority, right_key.subpriority);
    let left_range = left_key.range;
    let right_range = right_key.range;

    if left_priority != right_priority {
        return left_priority.cmp(&right_priority);
    }
    if left_range.start() < right_range.start() {
        if left_range.end() < right_range.end() && left_subpriority > right_subpriority {
            Ordering::Greater
        } else {
            Ordering::Less
        }
    } else if left_range.start() > right_range.start() {
        if left_range.end() > right_range.end() && left_subpriority < right_subpriority {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    } else if left_range.end() != right_range.end() {
        if right_range.end() < left_range.end() {
            Ordering::Less
        } else {
            Ordering::Greater
        }
    } else if left_subpriority != right_subpriority {
        left_subpriority.cmp(&right_subpriority)
    } else if eq_value(&left, &right) {
        Ordering::Equal
    } else if left_key.identity < right_key.identity {
        // GNU `compare_overlays` uses raw Lisp object identity as the final
        // stable tiebreaker for otherwise equal overlays.  Neomacs stores an
        // overlay allocation serial because Rust heap addresses are not
        // monotonic like GNU's Lisp object representation in this path.
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

fn overlay_identity_key(overlay: Value) -> u64 {
    overlay
        .as_overlay_data()
        .map(|data| data.serial)
        .filter(|serial| *serial != 0)
        .unwrap_or(overlay.bits() as u64)
}

/// The symbol `priority', interned once.
fn priority_symbol_id() -> crate::emacs_core::intern::SymId {
    static ID: std::sync::OnceLock<crate::emacs_core::intern::SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| crate::emacs_core::intern::intern("priority"))
}

/// How the sort reads an overlay's `priority'.
///
/// GNU's `sort_overlays' asks `Foverlay_get', which falls back to the
/// `category' symbol's plist when the overlay carries none of its own. That
/// fallback needs an obarray, which this layer has none of, so a caller that
/// has one passes the lookup in; callers without one read the overlay's own
/// plist, which is what this layer did for everyone before.
pub type OverlayPriorityLookup<'a> = &'a dyn Fn(Value) -> Option<Value>;

/// How a caller reads any property of an overlay -- `Foverlay_get' when the
/// caller has an obarray (so `category' is followed), the overlay's own plist
/// otherwise.
pub type OverlayPropertyLookup<'a> = &'a dyn Fn(Value, Value) -> Option<Value>;

fn overlay_plist_property(overlay: Value, property: Value) -> Option<Value> {
    overlay_property_named(overlay, property)
}

fn priority_value() -> Value {
    Value::from_sym_id(priority_symbol_id())
}

fn overlay_plist_priority(overlay: Value) -> Option<Value> {
    plist_get_by_symbol_id(overlay.as_overlay_data()?.plist, priority_symbol_id())
}

fn overlay_priority(overlay: Value, priority_of: OverlayPriorityLookup) -> (i64, i64) {
    match priority_of(overlay).filter(|value| !value.is_nil()) {
        None => (0, 0),
        Some(value) => match value.kind() {
            ValueKind::Fixnum(n) => (n, 0),
            ValueKind::Cons => (
                priority_component(value.cons_car()),
                priority_component(value.cons_cdr()),
            ),
            _ => (0, 0),
        },
    }
}

fn priority_component(value: Value) -> i64 {
    match value.kind() {
        ValueKind::Fixnum(n) => n,
        _ => 0,
    }
}

/// A plist lookup by symbol identity.  Resolving each key to its NAME and
/// comparing strings cost a name lookup per entry, on every comparison of
/// the sort `get-char-property' makes over the overlays at a position.
fn plist_get_by_symbol_id(plist: Value, prop: crate::emacs_core::intern::SymId) -> Option<Value> {
    let mut tail = plist;
    loop {
        if !tail.is_cons() {
            return None;
        };
        let pair_car = tail.cons_car();
        let pair_cdr = tail.cons_cdr();
        if !pair_cdr.is_cons() {
            return None;
        };
        if pair_car.as_symbol_id() == Some(prop) {
            return Some(pair_cdr.cons_car());
        }
        tail = pair_cdr.cons_cdr();
    }
}

fn overlay_plist_put(plist: Value, prop: Value, value: Value) -> (Value, bool) {
    let mut tail = plist;
    while tail.is_cons() {
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            break;
        }
        if eq_value(&tail.cons_car(), &prop) {
            let changed = !eq_value(&rest.cons_car(), &value);
            rest.set_car(value);
            return (plist, changed);
        }
        tail = rest.cons_cdr();
    }
    (
        Value::cons(prop, Value::cons(value, plist)),
        !value.is_nil(),
    )
}

impl Default for OverlayList {
    fn default() -> Self {
        Self::new()
    }
}

impl GcTrace for OverlayList {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for overlay in self.index.values() {
            roots.push(overlay);
        }
    }
}

#[cfg(test)]
#[path = "overlay_test.rs"]
mod tests;

/// Fold a Lisp value into `hasher` by CONTENT, following conses and strings.
///
/// `depth` only bounds pathological self-referential structure; a plist that
/// deep is folded by identity from there down, which over-invalidates rather
/// than under-invalidates. Unlike `sxhash`, nothing is skipped for being long.
/// What an overlay property means to [`OverlayList::content_digest`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayPropertyRelevance {
    /// Redisplay reads it, so the digest must fold its value.
    Rendered,
    /// `category`: the value is a SYMBOL whose plist supplies the real
    /// properties, and that plist can be rewritten without touching any
    /// overlay, so no digest of the overlay itself can be trusted.
    CategoryIndirection,
    /// Nothing that builds a glyph can see it.
    Unrendered,
}

/// The overlay properties redisplay reads, and therefore the only ones a
/// content digest has to cover.
///
/// This is an ALLOWLIST rather than "hash the whole plist", and the reason is
/// concrete: application code routinely parks freshly allocated objects on
/// overlays that nothing renders. The `rust-lsp-typing` fixture puts a
/// `json-parse-string` hash table under `lsp-diagnostic` on every overlay it
/// makes, so a digest that folded the whole plist would move on every frame --
/// not conservatively slow, simply inert.
///
/// The list is auditable, not guessed: redisplay resolves a fixed set of
/// property NAMES, so a property outside the set cannot reach a glyph. It was
/// taken by reading the layout engine's property lookups, and anything added
/// there has to be added here. `help-echo` is deliberately absent: the layout
/// engine names it only in its own tests, and mouse highlight re-reads overlays
/// live rather than consulting the glyph matrix.
struct DisplayOverlayProperties {
    rendered: [Value; 12],
    category: Value,
}

impl DisplayOverlayProperties {
    /// Interning once per digest costs thirteen obarray lookups per frame,
    /// against one lookup per plist key if this were done per comparison.
    /// Symbol ids are NOT cached across calls: a pdump load replaces the
    /// interner, and a stale id here would silently classify every key as
    /// unrendered.
    fn intern() -> Self {
        Self {
            rendered: [
                Value::symbol("face"),
                Value::symbol("display"),
                Value::symbol("invisible"),
                Value::symbol("before-string"),
                Value::symbol("after-string"),
                Value::symbol("line-prefix"),
                Value::symbol("wrap-prefix"),
                Value::symbol("priority"),
                Value::symbol("window"),
                Value::symbol("cursor"),
                Value::symbol("mouse-face"),
                Value::symbol("neomacs-cursor-effect"),
            ],
            category: Value::symbol("category"),
        }
    }

    fn classify(&self, key: Value) -> OverlayPropertyRelevance {
        if key.bits() == self.category.bits() {
            return OverlayPropertyRelevance::CategoryIndirection;
        }
        if self
            .rendered
            .iter()
            .any(|rendered| rendered.bits() == key.bits())
        {
            return OverlayPropertyRelevance::Rendered;
        }
        OverlayPropertyRelevance::Unrendered
    }
}

fn hash_value_content(value: Value, depth: u32, hasher: &mut rustc_hash::FxHasher) {
    use crate::emacs_core::value::ValueKind;
    use std::hash::Hasher;

    const MAX_DEPTH: u32 = 32;
    if depth > MAX_DEPTH {
        hasher.write_u64(value.bits() as u64);
        return;
    }
    match value.kind() {
        ValueKind::Nil => hasher.write_u8(1),
        ValueKind::T => hasher.write_u8(2),
        ValueKind::Fixnum(n) => {
            hasher.write_u8(3);
            hasher.write_i64(n);
        }
        ValueKind::Symbol(id) | ValueKind::Subr(id) => {
            hasher.write_u8(4);
            hasher.write_u32(id.0);
        }
        ValueKind::String => {
            hasher.write_u8(5);
            match value.as_lisp_string() {
                Some(s) => {
                    hasher.write(s.as_bytes());
                    // A `before-string`'s own text properties are part of what
                    // it renders as, and `put-text-property` on it moves no
                    // overlay tick. Folding the interval table's mutation
                    // counter catches that; two strings built the same way
                    // reach the same counter, so it does not over-invalidate
                    // an honest rebuild.
                    hasher.write_u64(s.intervals().mutation_tick());
                }
                None => hasher.write_u64(value.bits() as u64),
            }
        }
        ValueKind::Cons => {
            hasher.write_u8(6);
            let mut cursor = value;
            // Iterative along the cdr so a long plist cannot recurse deeply.
            let mut seen = 0u32;
            while cursor.is_cons() && seen < 4096 {
                hash_value_content(cursor.cons_car(), depth + 1, hasher);
                cursor = cursor.cons_cdr();
                seen += 1;
            }
            if !cursor.is_nil() {
                hash_value_content(cursor, depth + 1, hasher);
            }
        }
        // Floats, vectors, records, buffers, markers, hash tables and anything
        // else fold in by identity: a rebuilt one differs, which escalates.
        _ => {
            hasher.write_u8(7);
            hasher.write_u64(value.bits() as u64);
        }
    }
}

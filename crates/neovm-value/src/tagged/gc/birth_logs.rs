//! Mutator-owned allocate-black births during a concurrent generational major.
//!
//! A region records only its handed-out prefix when it closes. Neither the
//! Rust nor compiled cons/float bump path logs individual cells. Collector
//! consumers must stop every mutator, join the marker and close every region
//! before reading these descriptors; storage remains live until promotion.

use super::*;

#[derive(Clone, Copy, Debug)]
enum BirthRegionKind {
    Cons,
    Float,
}

/// A closed region's used prefix. All stride arithmetic stays in this module.
#[derive(Clone, Copy, Debug)]
pub(super) struct BlackBornRegion {
    first: usize,
    used: usize,
    kind: BirthRegionKind,
}

impl BlackBornRegion {
    fn new_cons(first: usize, cur: usize) -> Option<Self> {
        Self::new(first, cur, size_of::<ConsCell>(), BirthRegionKind::Cons)
    }

    fn new_float(first: usize, cur: usize) -> Option<Self> {
        Self::new(
            first,
            cur,
            <FloatObj as PagedObject>::SLOT_BYTES,
            BirthRegionKind::Float,
        )
    }

    fn new(first: usize, cur: usize, stride: usize, kind: BirthRegionKind) -> Option<Self> {
        debug_assert!(first <= cur, "a used region prefix runs forwards");
        debug_assert_eq!((cur - first) % stride, 0, "a whole-slot used prefix");
        let used = (cur - first) / stride;
        (used != 0).then_some(Self { first, used, kind })
    }

    /// Used float headers for P-all promotion. Cons regions yield no headers;
    /// their promotion remains owned by the cons bitmap pass.
    #[inline]
    pub(super) fn float_headers(&self) -> impl Iterator<Item = *mut GcHeader> + '_ {
        let used = match self.kind {
            BirthRegionKind::Float => self.used,
            BirthRegionKind::Cons => 0,
        };
        (0..used).map(|index| {
            (self.first + index * <FloatObj as PagedObject>::SLOT_BYTES) as *mut GcHeader
        })
    }

    #[inline]
    pub(super) fn cons_owners(&self) -> impl Iterator<Item = TaggedValue> + '_ {
        let used = match self.kind {
            BirthRegionKind::Cons => self.used,
            BirthRegionKind::Float => 0,
        };
        (0..used).map(|index| {
            let cell = (self.first + index * size_of::<ConsCell>()) as *mut ConsCell;
            // SAFETY: the descriptor covers initialized handed-out cells of
            // an owned block; stop-all/join prevents mutation or reclaim.
            unsafe { TaggedValue::from_cons_ptr(cell) }
        })
    }
}

impl TaggedHeap {
    /// Called only after every field of a new non-cons object is initialized.
    /// The major flag implies the knob is enabled. GEN0 and GEN1 outside a
    /// major skip the cold recording path with one flag branch. The cold
    /// helper checks the concurrent phase because major scope includes sweep.
    #[inline]
    pub(super) fn note_black_born(&mut self, header: *mut GcHeader) {
        if self.generational.major_in_progress {
            self.record_black_born_major(header);
        }
    }

    #[cold]
    #[inline(never)]
    fn record_black_born_major(&mut self, header: *mut GcHeader) {
        // Keep the concurrent-phase test out of the ordinary allocation
        // path: with GEN0, the major flag alone rejects this cold helper.
        if !self.concurrent_mark_running {
            return;
        }
        debug_assert!(self.generational.enabled && self.mark_in_progress);
        debug_assert!(!header.is_null());
        // SAFETY: the allocating mutator has fully initialized this owned,
        // unpublished header and its payload before calling the log hook.
        debug_assert!(unsafe { (*header).is_marked_at(self.mark_parity) });
        debug_assert!(!unsafe { (*header).tenured });
        self.current_mutator_gc_mut().black_born.push(header);
    }

    #[cold]
    #[inline(never)]
    pub(super) fn note_black_cons_region_start(&mut self, first: *mut ConsCell) {
        debug_assert!(self.current_mutator_gc().black_cons_region_start.is_none());
        if self.generational.major_in_progress && self.concurrent_mark_running {
            debug_assert!(self.generational.enabled && self.mark_in_progress);
            self.current_mutator_gc_mut().black_cons_region_start = Some(first as usize);
        }
    }

    #[cold]
    #[inline(never)]
    pub(super) fn note_black_float_region_start(&mut self, first: *mut FloatObj) {
        debug_assert!(self.current_mutator_gc().black_float_region_start.is_none());
        if self.generational.major_in_progress && self.concurrent_mark_running {
            debug_assert!(self.generational.enabled && self.mark_in_progress);
            self.current_mutator_gc_mut().black_float_region_start = Some(first as usize);
        }
    }

    /// Close before the caller's zero-unused-tail early return. An exhausted
    /// region has no tail to refund but every used cell still needs logging.
    #[cold]
    #[inline(never)]
    pub(super) fn close_black_cons_region(&mut self, cur: usize) {
        if let Some(first) = self.current_mutator_gc_mut().black_cons_region_start.take()
            && let Some(region) = BlackBornRegion::new_cons(first, cur)
        {
            self.current_mutator_gc_mut()
                .black_born_regions
                .push(region);
        }
    }

    #[cold]
    #[inline(never)]
    pub(super) fn close_black_float_region(&mut self, cur: usize) {
        if let Some(first) = self
            .current_mutator_gc_mut()
            .black_float_region_start
            .take()
            && let Some(region) = BlackBornRegion::new_float(first, cur)
        {
            self.current_mutator_gc_mut()
                .black_born_regions
                .push(region);
        }
    }

    /// Born-black owners fail a normal mark claim, so trace their initialized
    /// children explicitly before finalizer and weak-table decisions. Keep
    /// every log intact: promotion consumes these same P-all facts afterwards.
    #[cold]
    #[inline(never)]
    pub(super) fn trace_black_born_children_world_stopped(&mut self) {
        if !self.is_generational_major_marking() {
            return;
        }
        assert!(!self.concurrent_mark_running, "the marker must have joined");
        debug_assert!(!self.alloc_regions_open(), "stop-all closes birth regions");
        // These scratch containers hold raw headers/descriptors, not Values.
        // No Lisp allocation or safepoint occurs before they are consumed.
        let headers: Vec<*mut GcHeader> = self
            .mutators()
            .flat_map(|mutator| mutator.black_born.iter().copied())
            .collect();
        let regions: Vec<BlackBornRegion> = self
            .mutators()
            .flat_map(|mutator| mutator.black_born_regions.iter().copied())
            .collect();
        for header in headers {
            // SAFETY: every logged allocation is initialized and pre-marked;
            // stop-all/join prohibits access racing or free until promotion.
            let owner = unsafe {
                match (*header).kind {
                    HeapObjectKind::String => TaggedValue::from_string_ptr(header.cast()),
                    HeapObjectKind::Float => TaggedValue::from_float_ptr(header.cast()),
                    HeapObjectKind::VecLike => TaggedValue::from_veclike_ptr(header.cast()),
                }
            };
            self.push_value_children_to_gray(owner, "black-born-header-child");
        }
        for region in regions {
            for owner in region.cons_owners() {
                self.push_value_children_to_gray(owner, "black-born-cons-child");
            }
        }
    }
}

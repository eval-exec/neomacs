//! Centralized tagged-heap mutation helpers.
//!
//! These functions apply generational and concurrent write barriers in the
//! tagged runtime.

#[cfg(any(test, feature = "test-support"))]
use crate::heap_types::LispString;
use crate::heap_types::{LispMarker, OverlayData};
#[cfg(any(test, feature = "test-support"))]
use crate::tagged::bytecode::chunk::ByteCodeFunction;
use crate::value::LispHashTable;
use crate::value::text_props::TextPropertyTable;

use super::collection_reads::{StoreState, WriteProjection, record_projected_write_in_store};
#[cfg(any(test, feature = "test-support"))]
use super::gc::SerialStringStorageCharge;
use super::gc::{
    HeapWriteKind, note_heap_slot_write_in_store, note_heap_write,
    note_string_backing_allocation_in_store, with_store_state,
};
use super::header::{
    BoolVectorObj, ByteCodeObj, ByteCodeSlotObject, ConsCell, HashTableObj, LambdaObj, MacroObj,
    MarkerObj, OverlayObj, RecordObj, StringObj, VecLikeHeader, VecLikeType, VectorObj, XwidgetObj,
    XwidgetViewObj,
};
use super::value::{TAG_MASK, TaggedValue};

mod chartable;
pub use chartable::CharTableWrite;
pub(crate) use chartable::SubCharTableWrite;
pub(crate) use chartable::{with_char_table_write, with_sub_char_table_write};

/// Short-lived observations can depend on mutable Lisp collections nested
/// inside display properties without a buffer-property write. Allocating a
/// fresh object does not change this revision; mutating an existing one does.
/// Keep this separate from retained-row revisions: unrelated temporary-list
/// edits must not cancel off-screen acquisition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LispCollectionRevision(u64);

impl LispCollectionRevision {
    #[inline]
    pub(super) fn from_sequence(sequence: u64) -> Self {
        Self(sequence)
    }

    /// Exact mutation count for regression tests, including sequence wrap.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn steps_since_for_test(self, earlier: Self) -> u64 {
        self.0.wrapping_sub(earlier.0)
    }

    pub fn current() -> Self {
        Self(super::collection_reads::with_store(|store| {
            store.sequence()
        }))
    }

    #[inline]
    pub(super) fn sequence(self) -> u64 {
        self.0
    }

    /// Same revision word. Production `sequence` stays `pub(super)`.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn sequence_for_test(self) -> u64 {
        self.sequence()
    }

    #[inline]
    pub(super) fn advance() -> Self {
        Self(super::collection_reads::with_store(|store| store.advance()))
    }

    #[inline]
    #[doc(hidden)]
    pub fn changed(value: TaggedValue) {
        if !super::collection_reads::history_required() {
            return;
        }
        super::collection_reads::with_store(|store| {
            let next = store.advance();
            super::collection_reads::record_write_in_store(store, value, next);
        });
    }
}

/// Sentinel used by anonymous test/probe scopes (`HeapMutClosureGuard::enter`)
/// that do not correspond to a specific heap object address.
const ANONYMOUS_HEAP_MUT_OWNER: usize = usize::MAX;

struct ActiveHeapMutClosures {
    /// Fast-path active object pointer (`0` when idle, `ANONYMOUS_HEAP_MUT_OWNER`
    /// for anonymous scopes). The common 0 -> 1 -> 0 extent touches only this
    /// `Cell<usize>` and `nested_len` with zero `RefCell` or heap traffic.
    primary: std::cell::Cell<usize>,
    /// Mirror of `nested.borrow().len()` so fast-path `Drop` and idle checks
    /// never touch `RefCell`.
    nested_len: std::cell::Cell<u32>,
    /// Cold overflow stack for nested mutations on *distinct* heap objects.
    nested: std::cell::RefCell<Vec<usize>>,
}

impl ActiveHeapMutClosures {
    const fn new() -> Self {
        Self {
            primary: std::cell::Cell::new(0),
            nested_len: std::cell::Cell::new(0),
            nested: std::cell::RefCell::new(Vec::new()),
        }
    }

    #[inline(always)]
    fn is_active(&self) -> bool {
        self.primary.get() != 0 || self.nested_len.get() != 0
    }

    #[inline(always)]
    fn enter(&self, object: usize) -> bool {
        assert_ne!(object, 0, "heap object pointer must be non-null");
        let current = self.primary.get();
        if current == 0 {
            self.primary.set(object);
            return true;
        }
        self.enter_nested(current, object);
        false
    }

    #[cold]
    #[inline(never)]
    fn enter_nested(&self, current: usize, object: usize) {
        if object != ANONYMOUS_HEAP_MUT_OWNER {
            assert_ne!(
                current, object,
                "re-entrant with_*_mut on the same heap object (U3)"
            );
            let mut nested = self.nested.borrow_mut();
            assert!(
                !nested.contains(&object),
                "re-entrant with_*_mut on the same heap object (U3)"
            );
            nested.push(object);
            self.nested_len.set(nested.len() as u32);
        } else {
            let mut nested = self.nested.borrow_mut();
            nested.push(object);
            self.nested_len.set(nested.len() as u32);
        }
    }

    #[inline(always)]
    fn exit(&self, object: usize, was_primary: bool) {
        if was_primary && self.nested_len.get() == 0 && self.primary.get() == object {
            self.primary.set(0);
            return;
        }
        self.exit_slow(object);
    }

    #[cold]
    #[inline(never)]
    fn exit_slow(&self, object: usize) {
        let mut nested = self.nested.borrow_mut();
        if let Some(pos) = nested.iter().rposition(|&active| active == object) {
            nested.remove(pos);
            self.nested_len.set(nested.len() as u32);
            return;
        }
        if self.primary.get() == object {
            let next_primary = nested.pop().unwrap_or(0);
            self.nested_len.set(nested.len() as u32);
            self.primary.set(next_primary);
        }
    }
}

std::thread_local! {
    /// Phase 0c / GEN-5: tracks live `with_*_mut` mutation extents on this
    /// thread in both debug and release builds so:
    /// - re-entering `with_*_mut` on the same heap object panics before a
    ///   second `&mut` reference is formed (closing U3), and
    /// - GC collection entry panics while any bulk mutation closure is active.
    static IN_HEAP_MUT_CLOSURE: ActiveHeapMutClosures = const { ActiveHeapMutClosures::new() };
}

/// Test hook: is a bulk heap mutation closure active on this thread?
///
/// C1: a pure test-only reader takes `any(test, feature = "test-support")`
/// rather than the family gate; it must not compile in plain debug, where
/// it is unused.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn in_heap_mut_closure_for_test() -> bool {
    IN_HEAP_MUT_CLOSURE.with(ActiveHeapMutClosures::is_active)
}

#[must_use = "the thread-local mutation extent ends when this guard drops"]
#[derive(Debug)]
#[doc(hidden)]
pub struct HeapMutClosureGuard {
    object: usize,
    was_primary: bool,
    _not_send_sync: std::marker::PhantomData<*mut ()>,
}
static_assertions::assert_not_impl_any!(HeapMutClosureGuard: Send, Sync);

impl HeapMutClosureGuard {
    #[inline]
    #[doc(hidden)]
    pub fn enter() -> Self {
        Self::enter_object(ANONYMOUS_HEAP_MUT_OWNER)
    }

    #[inline(always)]
    pub(crate) fn enter_object(object: usize) -> Self {
        let was_primary = IN_HEAP_MUT_CLOSURE.with(|state| state.enter(object));
        Self {
            object,
            was_primary,
            _not_send_sync: std::marker::PhantomData,
        }
    }
}

impl Drop for HeapMutClosureGuard {
    #[inline(always)]
    fn drop(&mut self) {
        let _ = IN_HEAP_MUT_CLOSURE.try_with(|state| state.exit(self.object, self.was_primary));
    }
}

/// Release backstop (Phase 0c / GEN-5): panic if a GC safe point, collection,
/// or carrier handoff runs while any `with_*_mut` mutation closure is live.
#[inline(always)]
#[doc(hidden)]
pub fn assert_no_heap_mut_closure() {
    IN_HEAP_MUT_CLOSURE.with(|state| {
        assert!(
            !state.is_active(),
            "GC safe point inside a with_*_mut heap mutation closure (GEN-5)"
        );
    });
}

#[inline(always)]
#[doc(hidden)]
pub fn debug_assert_no_heap_mut_closure() {
    assert_no_heap_mut_closure();
}

/// A reused carrier must have finished every previous mutation extent.
/// Diagnose a leaked guard before changing its flag so the failed boundary
/// leaves the invariant available for diagnosis and the guard's own cleanup.
pub(super) fn reset_carrier_tls() {
    IN_HEAP_MUT_CLOSURE.with(|state| {
        assert!(
            !state.is_active(),
            "carrier reuse with a live heap mutation closure (GEN-5)"
        );
        state.primary.set(0);
        state.nested_len.set(0);
        state.nested.borrow_mut().clear();
    });
}

/// Read `VecLikeHeader::type_tag` directly while holding
/// `HeapMutClosureGuard::enter_object(raw)`, without calling
/// `collection_reads::observe` before `LispCollectionRevision::changed`.
///
/// # Safety
/// `raw` must be the untagged address of a live veclike object, and the
/// caller must already hold `HeapMutClosureGuard::enter_object(raw)`.
#[inline(always)]
pub(crate) unsafe fn checked_veclike_type_under_guard(raw: usize) -> VecLikeType {
    // SAFETY: caller proved `raw` is a live `VecLikeHeader` and holds
    // `HeapMutClosureGuard::enter_object(raw)`, excluding any active `&mut`
    // loan of this object on this thread.
    unsafe { (*(raw as *const VecLikeHeader)).type_tag }
}

/// Mutate packed bits through the same collection journal as other mutable
/// values. Bool-vectors have no Lisp children, so they need no pointer write
/// barrier, but layout observations can depend on their contents.
#[inline]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn with_bool_vector_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut super::header::BoolVectorObj) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    if unsafe { checked_veclike_type_under_guard(raw) } != VecLikeType::BoolVector {
        return None;
    }
    LispCollectionRevision::changed(value);
    let ptr = value.as_veclike_ptr().unwrap() as *mut BoolVectorObj;
    // SAFETY: the type was validated above and `_guard` excludes re-entry on
    // `ptr`. The mutation closure cannot run Lisp or collect.
    Some(f(unsafe { &mut *ptr }))
}

// Keep the recorder and barrier rejects in the caller as the generation-only
// arm grows: a rejected store must not gain a setter call under either mode.
#[inline(always)]
pub fn set_cons_car(cell: TaggedValue, value: TaggedValue) -> bool {
    with_store_state(|store| {
        let _ = set_cons_car_in_store(store, cell, value);
    });
    // The tag is the only failure condition. Keep it visible to callers
    // even when the outlined null-heap entry performs the shared store.
    cell.is_cons()
}

/// Same store as `set_cons_car_in_store`. The production function stays `pub(crate)`.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn set_cons_car_in_store_for_test(
    store: &StoreState,
    cell: TaggedValue,
    value: TaggedValue,
) -> bool {
    set_cons_car_in_store(store, cell, value)
}

#[inline(always)]
pub(crate) fn set_cons_car_in_store(
    store: &StoreState,
    cell: TaggedValue,
    value: TaggedValue,
) -> bool {
    if !record_projected_write_in_store(store, cell, WriteProjection::Cons) {
        return false;
    }
    // SAFETY: the shared recorder validated this live Cons tag.
    unsafe { finish_cons_store_in_store(store, cell, value, false) }
}

#[inline(always)]
pub fn set_cons_cdr(cell: TaggedValue, value: TaggedValue) -> bool {
    with_store_state(|store| {
        let _ = set_cons_cdr_in_store(store, cell, value);
    });
    cell.is_cons()
}

#[inline(always)]
pub(crate) fn set_cons_cdr_in_store(
    store: &StoreState,
    cell: TaggedValue,
    value: TaggedValue,
) -> bool {
    if !record_projected_write_in_store(store, cell, WriteProjection::Cons) {
        return false;
    }
    // SAFETY: the shared recorder validated this live Cons tag.
    unsafe { finish_cons_store_in_store(store, cell, value, true) }
}

/// Complete a prepared cons store with its unchanged GC barrier.
///
/// # Safety
/// `cell` must name a live, exclusively mutated ConsCell. No safe point or
/// callback may intervene between validating that owner and this operation.
#[inline(always)]
#[doc(hidden)]
pub unsafe fn finish_cons_store_in_store(
    store: &StoreState,
    cell: TaggedValue,
    value: TaggedValue,
    cdr: bool,
) -> bool {
    let kind = if cdr {
        HeapWriteKind::ConsCdr
    } else {
        HeapWriteKind::ConsCar
    };
    note_heap_slot_write_in_store(store, cell, kind, usize::from(cdr), value);
    let owner = (cell.bits() & !TAG_MASK) as *mut ConsCell;
    unsafe {
        if cdr {
            (*owner).set_cdr(value)
        } else {
            (*owner).set_car(value)
        }
    }
    true
}

/// Complete a prepared vector/record store without retaining a payload loan
/// across the barrier's preimage reads.
///
/// # Safety
/// `data` must be this live owner's slot backing and `index` must be in range.
/// The caller owns mutation and retains no payload reference across this call.
#[inline(always)]
#[doc(hidden)]
pub unsafe fn finish_slot_store_in_store(
    store: &StoreState,
    owner: TaggedValue,
    kind: HeapWriteKind,
    index: usize,
    item: TaggedValue,
    data: *mut super::header::LispValueVec,
) -> bool {
    note_heap_slot_write_in_store(store, owner, kind, index, item);
    unsafe { (*data).store_atomic(index, item) };
    true
}

/// Complete a cached native BLV store without turning its implementation
/// access into a Lisp collection read. The caller has validated its variable
/// shape and executes under this Context's exclusive mutator installation;
/// this path allocates no Lisp object, calls no Lisp and has no safe point.
///
/// Certificates, revisions and the observation envelope remain mutator-local.
/// Exact marks use shared atomic lifetime metadata, but do not supply a
/// cross-mutator journal. Ordinary tracking and GEN1 retain the full setter.
#[cfg(feature = "jit")]
#[inline]
#[doc(hidden)]
pub fn set_compiled_cons_cdr(cell: TaggedValue, value: TaggedValue) -> bool {
    use super::collection_reads::{CompiledJournalMode, compiled_journal_mode, is_observed};
    if compiled_journal_mode() != CompiledJournalMode::Observed
        || super::gc::current_heap_generational_enabled()
        || super::gc::current_write_tracking_enabled()
    {
        return set_cons_cdr(cell, value);
    }
    if !cell.is_cons() {
        return false;
    }
    let address = cell.bits() & !TAG_MASK;
    let (lo, hi) = super::collection_reads::compiled_observation_window();
    // SAFETY: `cell` passed the Cons tag check above; cons observation queries
    // side-bitmap metadata only and never dereferences `cell`.
    let observed = lo <= address && address < hi && unsafe { is_observed(cell.bits()) };
    if !observed {
        // SAFETY: `cell` passed the cons check above and is a live word this mutator owns.
        unsafe {
            super::gc::neovm_jit_unobserved_collection_owner(cell.bits() as i64);
        }
    }
    with_store_state(|store| {
        if observed {
            record_projected_write_in_store(store, cell, WriteProjection::Cons);
        }
        // SAFETY: the preceding Cons tag check validated this BLV owner.
        unsafe { finish_cons_store_in_store(store, cell, value, true) }
    })
}

#[inline]
pub fn with_vector_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut [TaggedValue]) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    if unsafe { checked_veclike_type_under_guard(raw) } != VecLikeType::Vector {
        return None;
    }
    LispCollectionRevision::changed(value);
    let ptr = value.as_veclike_ptr().unwrap() as *mut VectorObj;
    // SATB reachability barrier (pre-image children) — must fire before the mutation.
    note_heap_write(value, HeapWriteKind::VectorBulk);
    // Stage 2 Tier B CONCURRENT VECTOR SCAN clone-on-write: during a concurrent mark
    // the GC thread holds a start-of-cycle snapshot pointer into this vector's OWNED
    // backing. Before mutating, clone+retire that backing (once per owner per cycle)
    // so the snapshot keeps addressing an immutable, live buffer and the closure below
    // mutates the fresh clone. No-op outside a concurrent mark or for a MAPPED backing
    // (see the heap-side hook) — a single thread-local load, so the idle path pays
    // essentially nothing.
    if super::gc::concurrent_mark_active() {
        // SAFETY: invoked at the exclusive vector bulk-mutation entry point
        // (no live HeapRef borrow of this vector); clones and retires the old
        // backing for the concurrent marker without collecting, re-entering
        // `with_tagged_heap`, or minting capability tokens.
        unsafe { super::gc::with_tagged_heap(|heap| heap.concurrent_clone_on_write_vector(value)) };
    }
    // SAFETY: `ptr` was validated as a live `VectorObj`, `_guard` excludes
    // re-entry or collection, and any concurrent marker snapshot was retired
    // above before exposing `&mut [TaggedValue]` in place.
    Some(unsafe { f((*ptr).data.ensure_owned().as_mut_slice()) })
}

/// Bulk slot mutation without retaining a Rust reference to the Vec header
/// or backing across reads of aliased Lisp values. The caller owns exclusive
/// mutation of this vector; collector snapshots use the existing SATB/COW
/// protocol. No shared state or assumption of a single global mutator is added.
///
/// # Safety
/// The closure must not collect, replace/reallocate the backing, or retain the
/// pointer after returning. It may only access slots within the supplied length.
#[inline]
#[doc(hidden)]
pub unsafe fn with_vector_slots_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(*mut TaggedValue, usize) -> R,
) -> Option<R> {
    let (slots, len) = with_vector_data_mut(value, |data| (data.as_mut_ptr(), data.len()))?;
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    Some(f(slots, len))
}

#[inline]
pub fn replace_vector_data(value: TaggedValue, items: Vec<TaggedValue>) -> bool {
    if !value.is_veclike() {
        return false;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    if unsafe { checked_veclike_type_under_guard(raw) } != VecLikeType::Vector {
        return false;
    }
    LispCollectionRevision::changed(value);
    let ptr = value.as_veclike_ptr().unwrap() as *mut VectorObj;
    note_heap_write(value, HeapWriteKind::VectorBulk);
    if super::gc::concurrent_mark_active() {
        // SAFETY: exclusive vector bulk replacement; clones and retires any
        // concurrent marker snapshot before replacing the backing.
        unsafe { super::gc::with_tagged_heap(|heap| heap.concurrent_clone_on_write_vector(value)) };
    }
    // SAFETY: `ptr` was validated as a live `VectorObj`, `_guard` excludes
    // re-entry or collection, and any concurrent marker snapshot was retired
    // before replacing the owned backing.
    unsafe { (*ptr).data.replace_owned(items) };
    true
}

#[inline]
pub fn set_vector_slot(value: TaggedValue, index: usize, item: TaggedValue) -> bool {
    with_store_state(
        #[inline(always)]
        move |store| set_vector_slot_in_store(store, value, index, item),
    )
}

/// Same store as `set_vector_slot_in_store`. The production function stays `pub(crate)`.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn set_vector_slot_in_store_for_test(
    store: &StoreState,
    value: TaggedValue,
    index: usize,
    item: TaggedValue,
) -> bool {
    set_vector_slot_in_store(store, value, index, item)
}

#[inline(always)]
pub(crate) fn set_vector_slot_in_store(
    store: &StoreState,
    value: TaggedValue,
    index: usize,
    item: TaggedValue,
) -> bool {
    if !record_projected_write_in_store(store, value, WriteProjection::VecLike) {
        return false;
    }
    // The observed header is reused for subtype and payload. Wrong veclike
    // subtypes still contributed a read, exactly as veclike_type did before.
    let header = (value.bits() & !TAG_MASK) as *const VecLikeHeader;
    if unsafe { (*header).type_tag } != VecLikeType::Vector {
        return false;
    }
    let ptr = header as *mut VectorObj;
    let data = unsafe { &(*ptr).data };
    if index >= data.len() {
        return false;
    }
    // SAFETY: the subtype and bounds guards proved this live owner/backing.
    unsafe {
        finish_slot_store_in_store(
            store,
            value,
            HeapWriteKind::VectorSlot,
            index,
            item,
            std::ptr::addr_of_mut!((*ptr).data),
        )
    }
}

#[inline]
pub fn with_record_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut [TaggedValue]) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    if unsafe { checked_veclike_type_under_guard(raw) } != VecLikeType::Record {
        return None;
    }
    LispCollectionRevision::changed(value);
    let ptr = value.as_veclike_ptr().unwrap() as *mut RecordObj;
    note_heap_write(value, HeapWriteKind::RecordBulk);
    // SAFETY: `ptr` was validated as a live `RecordObj`, and `_guard` excludes
    // re-entry or collection while `&mut [TaggedValue]` is live in place.
    Some(unsafe { f((*ptr).data.ensure_owned().as_mut_slice()) })
}

#[inline]
pub fn replace_record_data(value: TaggedValue, items: Vec<TaggedValue>) -> bool {
    if !value.is_veclike() {
        return false;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    if unsafe { checked_veclike_type_under_guard(raw) } != VecLikeType::Record {
        return false;
    }
    LispCollectionRevision::changed(value);
    let ptr = value.as_veclike_ptr().unwrap() as *mut RecordObj;
    note_heap_write(value, HeapWriteKind::RecordBulk);
    // SAFETY: `ptr` was validated as a live `RecordObj`, and `_guard` excludes
    // re-entry or collection while replacing the owned backing.
    unsafe { (*ptr).data.replace_owned(items) };
    true
}

#[inline]
pub fn set_record_slot(value: TaggedValue, index: usize, item: TaggedValue) -> bool {
    with_store_state(
        #[inline(always)]
        move |store| set_record_slot_in_store(store, value, index, item),
    )
}

/// Same store as `set_record_slot_in_store`. The production function stays `pub(crate)`.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn set_record_slot_in_store_for_test(
    store: &StoreState,
    value: TaggedValue,
    index: usize,
    item: TaggedValue,
) -> bool {
    set_record_slot_in_store(store, value, index, item)
}

#[inline(always)]
pub(crate) fn set_record_slot_in_store(
    store: &StoreState,
    value: TaggedValue,
    index: usize,
    item: TaggedValue,
) -> bool {
    if !record_projected_write_in_store(store, value, WriteProjection::VecLike) {
        return false;
    }
    let header = (value.bits() & !TAG_MASK) as *const VecLikeHeader;
    if unsafe { (*header).type_tag } != VecLikeType::Record {
        return false;
    }
    let ptr = header as *mut RecordObj;
    let data = unsafe { &(*ptr).data };
    if index >= data.len() {
        return false;
    }
    // SAFETY: the subtype and bounds guards proved this live owner/backing.
    unsafe {
        finish_slot_store_in_store(
            store,
            value,
            HeapWriteKind::RecordSlot,
            index,
            item,
            std::ptr::addr_of_mut!((*ptr).data),
        )
    }
}

#[inline]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn with_closure_slots_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut [TaggedValue]) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    match unsafe { checked_veclike_type_under_guard(raw) } {
        VecLikeType::Lambda => {
            LispCollectionRevision::changed(value);
            let ptr = value.as_veclike_ptr().unwrap() as *mut LambdaObj;
            note_heap_write(value, HeapWriteKind::ClosureBulk);
            // SAFETY: `ptr` was validated as a live `LambdaObj`, and `_guard`
            // excludes re-entry or collection while `&mut [TaggedValue]` is live.
            unsafe {
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
                Some(f(obj.data.ensure_owned().as_mut_slice()))
            }
        }
        VecLikeType::Macro => {
            LispCollectionRevision::changed(value);
            let ptr = value.as_veclike_ptr().unwrap() as *mut MacroObj;
            note_heap_write(value, HeapWriteKind::ClosureBulk);
            // SAFETY: `ptr` was validated as a live `MacroObj`, and `_guard`
            // excludes re-entry or collection while `&mut [TaggedValue]` is live.
            unsafe {
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
                Some(f(obj.data.ensure_owned().as_mut_slice()))
            }
        }
        _ => None,
    }
}

#[inline]
pub fn replace_closure_slots(value: TaggedValue, slots: Vec<TaggedValue>) -> bool {
    if !value.is_veclike() {
        return false;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    match unsafe { checked_veclike_type_under_guard(raw) } {
        VecLikeType::Lambda => {
            LispCollectionRevision::changed(value);
            let ptr = value.as_veclike_ptr().unwrap() as *mut LambdaObj;
            note_heap_write(value, HeapWriteKind::ClosureBulk);
            // SAFETY: `ptr` was validated as a live `LambdaObj`, and `_guard`
            // excludes re-entry or collection while replacing owned slots.
            unsafe {
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
                obj.data.replace_owned(slots);
            }
            true
        }
        VecLikeType::Macro => {
            LispCollectionRevision::changed(value);
            let ptr = value.as_veclike_ptr().unwrap() as *mut MacroObj;
            note_heap_write(value, HeapWriteKind::ClosureBulk);
            // SAFETY: `ptr` was validated as a live `MacroObj`, and `_guard`
            // excludes re-entry or collection while replacing owned slots.
            unsafe {
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
                obj.data.replace_owned(slots);
            }
            true
        }
        _ => false,
    }
}

#[inline]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn with_string_text_props_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut TextPropertyTable) -> R,
) -> Option<R> {
    with_string_text_props_mut_with_had(value, f).map(|(_, result)| result)
}

#[inline]
pub(crate) fn with_string_text_props_mut_with_had<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut TextPropertyTable) -> R,
) -> Option<(bool, R)> {
    let ptr = value.as_string_ptr()? as *mut StringObj;
    let _guard = HeapMutClosureGuard::enter_object(ptr as usize);
    // SAFETY: `as_string_ptr` validated `ptr` as a live `StringObj`, and `_guard`
    // excludes re-entry or collection before inspecting `has_intervals`.
    let had = unsafe { (*ptr).data.has_intervals() };
    note_heap_write(value, HeapWriteKind::StringTextProps);
    // SAFETY: the published string is exclusively borrowed until `f` returns.
    // The journal sentinel is armed here and recorded by the caller only when
    // the closure changes property contents. The GC barrier stays first.
    let table = unsafe { (*ptr).data.intervals_mut() };
    table.arm_journal_mark();
    Some((had, f(table)))
}

/// Fixture-only unrestricted edits; production callers use typed string operations.
#[cfg(any(test, feature = "test-support"))]
#[inline]
pub fn with_lisp_string_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispString) -> R,
) -> Option<R> {
    if !value.is_string() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    if raw == 0 {
        return None;
    }
    let _guard = HeapMutClosureGuard::enter_object(raw);
    LispCollectionRevision::changed(value);
    let ptr = value.as_string_ptr()? as *mut StringObj;
    note_heap_write(value, HeapWriteKind::StringData);
    // SAFETY: this published string and its serial owner outlive the payload
    // edit, with no callback or safepoint. Declaring `_storage_charge` after
    // `_guard` ensures accounting runs while `_guard` still protects `ptr`.
    let _storage_charge = unsafe { SerialStringStorageCharge::new(ptr) };
    Some(f(unsafe { &mut (*ptr).data }))
}

/// Edit a published string's existing byte extent. A slice cannot replace or
/// grow its backing; only borrowed bytes need allocation accounting.
#[inline]
pub fn with_string_bytes_mut<R>(value: TaggedValue, f: impl FnOnce(&mut [u8]) -> R) -> Option<R> {
    if !value.is_string() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    if raw == 0 {
        return None;
    }
    let _guard = HeapMutClosureGuard::enter_object(raw);
    LispCollectionRevision::changed(value);
    let ptr = value.as_string_ptr()? as *mut StringObj;
    note_heap_write(value, HeapWriteKind::StringData);
    // SAFETY: the published string is exclusively borrowed until the closure
    // ends. No callback, collector safepoint, or escaping payload loan is allowed.
    let string = unsafe { &mut (*ptr).data };
    if !string.has_owned_storage() {
        let allocated = string.materialize_owned_storage();
        // Bill before invoking f, including when its fixed-size edit unwinds.
        with_store_state(|store| note_string_backing_allocation_in_store(store, allocated));
    }
    Some(string.mutate_fixed_bytes(f))
}

/// Clear bytes and properties through the published owner's mutation barrier.
#[inline]
pub fn clear_string_contents(value: TaggedValue) -> bool {
    if !value.is_string() {
        return false;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    if raw == 0 {
        return false;
    }
    let _guard = HeapMutClosureGuard::enter_object(raw);
    LispCollectionRevision::changed(value);
    let Some(ptr) = value.as_string_ptr() else {
        return false;
    };
    note_heap_write(value, HeapWriteKind::StringData);
    // SAFETY: this operation has serial access to the initialized string,
    // with no callback or safepoint before the payload loan ends.
    let allocated = unsafe {
        (*ptr.cast_mut())
            .data
            .clear_contents_to_unibyte_allocation()
    };
    if allocated != 0 {
        with_store_state(|store| note_string_backing_allocation_in_store(store, allocated));
    }
    true
}

/// Pin published bytecode storage; an owned payload remains in place.
#[inline]
pub fn pin_string_immovable(value: TaggedValue) -> bool {
    if !value.is_string() {
        return false;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    if raw == 0 {
        return false;
    }
    let _guard = HeapMutClosureGuard::enter_object(raw);
    LispCollectionRevision::changed(value);
    let Some(ptr) = value.as_string_ptr() else {
        return false;
    };
    note_heap_write(value, HeapWriteKind::StringData);
    // SAFETY: serial bytecode construction keeps the string live and uniquely
    // mutated, with no callback, collection, or escaping payload loan.
    let string = unsafe { &mut (*ptr.cast_mut()).data };
    if !string.has_owned_storage() {
        let allocated = string.materialize_owned_storage();
        with_store_state(|store| note_string_backing_allocation_in_store(store, allocated));
    }
    string.pin_immovable();
    true
}

/// Remove properties without changing the string's byte backing.
///
/// A string with no interval tree returns false and does not journal.
/// GNU `set_text_properties` returns `Qnil` before `set_string_intervals`
/// in that case (`src/textprop.c`, emacs-31.1). The absent tree is not a
/// mutation; journaling it advances `LispCollectionRevision` on the
/// canonical empty string.
#[inline]
pub fn clear_string_intervals(value: TaggedValue) -> bool {
    if !value.is_string() {
        return false;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    if raw == 0 {
        return false;
    }
    let ptr = raw as *const StringObj;
    // Acquire the release re-entrancy guard BEFORE reading `(*ptr).data` so a
    // re-entrant call while `&mut LispString` is live panics without aliasing.
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_string` means this word addresses a live `StringObj`, and
    // `_guard` proves no mutable loan of this object is active on this thread.
    // The load is atomic and records no collection read, so a string that
    // does have intervals keeps the journal-before-read order below.
    if unsafe { !(*ptr).data.has_intervals() } {
        return false;
    }
    LispCollectionRevision::changed(value);
    let Some(ptr) = value.as_string_ptr() else {
        return false;
    };
    note_heap_write(value, HeapWriteKind::StringTextProps);
    // SAFETY: serial owner access outlives this metadata-only mutation, and
    // clear_intervals enforces its interval-child SATB preimage barrier.
    unsafe { (*ptr.cast_mut()).data.clear_intervals() };
    true
}

/// Drop a table `intervals_mut` installed when the closure changed nothing.
///
/// The string's observable properties are still absent, so this is not a
/// collection write. The heap barrier still covers the pointer store.
#[inline]
pub(crate) fn drop_unmodified_string_intervals(value: TaggedValue) {
    let Some(ptr) = value.as_string_ptr() else {
        return;
    };
    let _guard = HeapMutClosureGuard::enter_object(ptr as usize);
    note_heap_write(value, HeapWriteKind::StringTextProps);
    // SAFETY: same exclusive string borrow as `clear_string_intervals`.
    unsafe { (*ptr.cast_mut()).data.clear_intervals() };
}

/// Overwrite one byte of a string's text in place (see
/// `LispString::set_byte_same_char_count`), WITHOUT the heap write barrier.
/// The barrier logs the owner's object children for the concurrent collector
/// and remembers dumped owners that may have gained heap children; a string's
/// bytes are not objects, and this touches nothing else (its text-property
/// intervals keep their own enforced barrier). `aset` on a string paid the
/// full barrier -- 43 instructions per character on dhrystone.
#[inline]
pub fn set_string_byte_same_char_count(value: TaggedValue, byte_pos: usize, byte: u8) -> bool {
    with_store_state(
        #[inline(always)]
        move |store| set_string_byte_same_char_count_in_store(store, value, byte_pos, byte),
    )
}

/// Same store as `set_string_byte_same_char_count_in_store`. The production function stays `pub(crate)`.
#[cfg(any(test, feature = "test-support"))]
#[doc(hidden)]
pub(crate) fn set_string_byte_same_char_count_in_store_for_test(
    store: &StoreState,
    value: TaggedValue,
    byte_pos: usize,
    byte: u8,
) -> bool {
    set_string_byte_same_char_count_in_store(store, value, byte_pos, byte)
}

#[inline(always)]
pub(crate) fn set_string_byte_same_char_count_in_store(
    store: &StoreState,
    value: TaggedValue,
    byte_pos: usize,
    byte: u8,
) -> bool {
    super::collection_reads::record_unprojected_write_in_store(store, value);
    if !value.is_string() {
        return false;
    }
    super::collection_reads::observe_store_projection_in_store(store, value);
    let ptr = (value.bits() & !TAG_MASK) as *mut StringObj;
    // SAFETY: a live string object, exclusively mutated with no callback or
    // safepoint. Guards make the raw byte store total even for invalid callers.
    let string = unsafe { &mut (*ptr).data };
    if byte_pos >= string.sbytes()
        || (string.is_multibyte()
            && (byte >= 0x80
                || string
                    .as_bytes()
                    .get(byte_pos)
                    .is_none_or(|old| *old >= 0x80)))
    {
        return false;
    }
    if string.has_owned_storage() {
        string.set_byte_same_char_count(byte_pos, byte);
    } else {
        string.set_byte_same_char_count(byte_pos, byte);
        note_string_backing_allocation_in_store(store, string.owned_capacity());
    }
    true
}

#[inline]
pub fn with_hash_table_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> Option<R> {
    if super::gc::concurrent_hash_mutation_active() {
        return with_hash_table_mut_concurrent(value, f);
    }
    // SAFETY: this mutation contains no collector transition. The caller's
    // closure obeys the ordinary heap-mutation contract, which forbids GC.
    unsafe { with_hash_table_mut_inactive_metered(value, f) }
}

/// Base mutation body for a caller that has already rejected Tier-H activation.
///
/// # Safety
/// The installed mutator's Tier-H snapshot must stay inactive from the caller's
/// mode check until this accessor returns. Do not carry that decision across a
/// Lisp callback or GC safepoint. The closure must not invoke Lisp or collect.
#[inline]
pub(crate) unsafe fn with_hash_table_mut_inactive<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> Option<R> {
    // SAFETY: the caller supplies the inactive mutation contract. Sealed
    // owners and typed writers retain their own allocation accounting.
    unsafe { with_hash_table_mut_inactive_impl::<false, R>(value, f) }
}

/// Generic mutation retains a positive-net storage charge. Validation and
/// barriers precede the normal snapshot, without repeating type observations.
///
/// # Safety
/// Same inactive-owner and no-callback/no-GC contract as the unmetered entry.
#[inline]
pub(crate) unsafe fn with_hash_table_mut_inactive_metered<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> Option<R> {
    // SAFETY: the caller retains the same serialized payload extent.
    unsafe { with_hash_table_mut_inactive_impl::<true, R>(value, f) }
}

#[inline]
unsafe fn with_hash_table_mut_inactive_impl<const METERED: bool, R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    // Acquire the release re-entrancy guard BEFORE reading `(*header).type_tag`,
    // write-barrier reads, or dereferencing `ptr` so re-entry panics without
    // forming an aliasing reference.
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    if unsafe { checked_veclike_type_under_guard(raw) } != VecLikeType::HashTable {
        return None;
    }
    let ptr = record_hash_table_mut_inactive(value);
    // SAFETY: validation established this initialized table; the serialized
    // barrier enumerates pending entries directly and does not hydrate them.
    if unsafe { (*ptr).table.needs_hydration() } {
        // SAFETY: the cold scope keeps one net charge over both hydration and
        // the callback, including replacement with smaller detached storage.
        return Some(unsafe { with_pending_hash_table_mut::<METERED, R>(ptr, f) });
    }
    // SAFETY: the validated table is hydrated, and the caller retains serial
    // access until the callback's borrow and its accounting guard have ended.
    Some(unsafe { with_verified_hash_table_mut::<METERED, R>(ptr, f) })
}

/// Typed inactive hash-table write path (`Value::with_hash_table_write_inactive`).
///
/// Acquires `HeapMutClosureGuard::enter_object` strictly BEFORE reading
/// `(*header).type_tag`, hydrating pending storage, or forming `&mut LispHashTable`
/// so re-entering through `with_hash_table_write_inactive` while an outer
/// `with_hash_table_mut` (or another write) is live panics without aliasing.
///
/// # Safety
/// Same inactive-owner and no-callback/no-GC contract as `with_hash_table_mut_inactive`.
#[inline]
pub(crate) unsafe fn with_hash_table_write_inactive_impl<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    if unsafe { checked_veclike_type_under_guard(raw) } != VecLikeType::HashTable {
        return None;
    }
    let ptr = record_hash_table_mut_inactive(value);
    // SAFETY: `_guard` excludes re-entry on `ptr`, and validation above proved
    // `ptr` is a live `HashTableObj`.
    if unsafe { (*ptr).table.needs_hydration() } {
        // SAFETY: cold hydration bills lazy dump materialization separately from
        // `HashTableWrite`'s own growth accounting, then runs the unmetered body.
        return Some(unsafe { with_pending_hash_table_write_inactive(ptr, f) });
    }
    // SAFETY: the validated table is hydrated, and `HashTableWrite` reports its
    // own growth and cannot replace the table.
    Some(unsafe { with_verified_hash_table_mut::<false, R>(ptr, f) })
}

/// Keep revision, observation, and write-barrier ordering shared by both serial entries.
#[inline(always)]
fn record_hash_table_mut_inactive(value: TaggedValue) -> *mut HashTableObj {
    debug_assert!(!super::gc::concurrent_hash_mutation_active());
    LispCollectionRevision::changed(value);
    let ptr = value.as_veclike_ptr().unwrap() as *mut HashTableObj;
    note_heap_write(value, HeapWriteKind::HashTableData);
    ptr
}

#[cold]
#[inline(never)]
unsafe fn with_pending_hash_table_write_inactive<R>(
    ptr: *mut HashTableObj,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> R {
    {
        // SAFETY: `ptr` is exclusively guarded by the caller's `HeapMutClosureGuard`;
        // this guard charges only pending hydration so `HashTableWrite`'s own
        // per-growth accounting is not double-billed during `f`.
        let _hydration_charge = unsafe { super::gc::SerialHashTableStorageCharge::new(ptr) };
        // SAFETY: pending hydration precedes publication of the mutable payload.
        unsafe { (*ptr).table.hydrate_pending() };
    }
    // SAFETY: `ptr` is now hydrated and still guarded by the caller's outer
    // `HeapMutClosureGuard` across `f` and any unwind.
    unsafe { with_verified_hash_table_mut::<false, R>(ptr, f) }
}

#[cold]
#[inline(never)]
unsafe fn with_pending_hash_table_mut<const METERED: bool, R>(
    ptr: *mut HashTableObj,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> R {
    let _storage_charge = if METERED {
        // SAFETY: this guard precedes the first materialization, and its
        // serialized table extent also covers the verified edit below.
        Some(unsafe { super::gc::SerialHashTableStorageCharge::new(ptr) })
    } else {
        None
    };
    // SAFETY: pending hydration precedes publication of the mutable payload.
    unsafe { (*ptr).table.hydrate_pending() };
    // SAFETY: the cold outer guard is the sole generic net biller. The outer
    // `HeapMutClosureGuard` covers the entire scope including unwind.
    unsafe { with_verified_hash_table_mut::<false, R>(ptr, f) }
}

/// Preserve the original inlined payload-loan body. An opaque forwarding ABI
/// here spills the callback environment and result storage on every edit.
///
/// # Safety
/// `ptr` names a live, hydrated table in the caller's exclusive inactive-owner
/// extent (guarded by `HeapMutClosureGuard`). The callback must not call Lisp,
/// collect, or escape its payload loan.
#[inline(always)]
unsafe fn with_verified_hash_table_mut<const METERED: bool, R>(
    ptr: *mut HashTableObj,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> R {
    unsafe {
        // Whatever `f` does to the table, a `switch` plan compiled from its
        // keys no longer describes it. A wholesale replacement (`*table =
        // other`) installs a table whose cache starts empty.
        (*ptr).table.data.switch_plan.invalidate();
    }
    // Nor does JIT code that answers it inline behind its epoch. The new
    // epoch is stored before `f`, so even a mutation `f` abandons half way
    // is covered, and again after it: a wholesale replacement installs
    // another table's epoch (a fresh table's 0, a copy's), which could
    // equal one compiled against this object.
    let epoch = unsafe { (*ptr).table.data.switch_epoch.wrapping_add(1) };
    unsafe { (*ptr).table.data.switch_epoch = epoch };
    // Both concrete extents use the same payload loan. Expanding its lexical
    // scope keeps the payload loan inside the metered charge's drop scope,
    // without constructing an Option<charge> in the unmetered extent.
    macro_rules! payload_loan {
        () => {{
            // SAFETY: the caller retains the verified hydrated payload loan.
            let result = f(unsafe { &mut (*ptr).table });
            // SAFETY: the loan ended above; restore this table's write epoch.
            unsafe { (*ptr).table.data.switch_epoch = epoch };
            result
        }};
    }
    if METERED {
        // Retain the existing metered representation and unwind consumer;
        // only the unmetered branch needs to omit the optional charge object.
        // SAFETY: all non-payload bookkeeping is complete; the callback may
        // replace this table, but its borrow cannot outlive the guard.
        let _storage_charge = Some(unsafe { super::gc::SerialHashTableStorageCharge::new(ptr) });
        payload_loan!()
    } else {
        // The unmetered extent has no charge object, including on unwind.
        payload_loan!()
    }
}

/// Keep the snapshot, mutation lease and unwind/drop paths in the active clone.
/// The caller has already selected an active Tier-H snapshot. The lease covers
/// preimage enumeration and the mutable borrow, and its Arc lives throughout.
#[cold]
#[inline(never)]
pub(crate) fn with_hash_table_mut_concurrent<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    if unsafe { checked_veclike_type_under_guard(raw) } != VecLikeType::HashTable {
        return None;
    }
    let snapshot = super::gc::concurrent_hash_snapshot(value);
    let mut mutation = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.lock_mutation(raw));
    // The shared entry mutex must precede even journal/SATB preimage reads.
    LispCollectionRevision::changed(value);
    let ptr = value.as_veclike_ptr().unwrap() as *mut HashTableObj;
    note_heap_write(value, HeapWriteKind::HashTableData);
    if let Some(guard) = mutation.as_mut() {
        super::gc::prepare_concurrent_hash_write(value, guard);
    }
    // SAFETY: serial Tier-H access holds the mutation lease through this
    // scope; the table's mutable borrow ends before the charge guard drops.
    let _storage_charge = unsafe { super::gc::SerialHashTableStorageCharge::new(ptr) };
    unsafe {
        // Lazy dump hydration before the caller sees the table (see
        // `Value::as_hash_table`).
        if (*ptr).table.needs_hydration() {
            (*ptr).table.hydrate_pending();
        }
        // Whatever `f` does to the table, a `switch` plan compiled from its
        // keys no longer describes it. A wholesale replacement (`*table =
        // other`) installs a table whose cache starts empty.
        (*ptr).table.data.switch_plan.invalidate();
    }
    // Nor does JIT code that answers it inline behind its epoch. The new
    // epoch is stored before `f`, so even a mutation `f` abandons half way
    // is covered, and again after it: a wholesale replacement installs
    // another table's epoch (a fresh table's 0, a copy's), which could
    // equal one compiled against this object.
    let epoch = unsafe { (*ptr).table.data.switch_epoch.wrapping_add(1) };
    unsafe { (*ptr).table.data.switch_epoch = epoch };
    let result = f(unsafe { &mut (*ptr).table });
    unsafe { (*ptr).table.data.switch_epoch = epoch };
    Some(result)
}

/// Install the Lisp object `aref` hands out for one of a byte-code
/// function's GNU slots (see [`super::header::ByteCodeSlotObjects`]).
///
/// The one post-publication write into a `ByteCodeObj`, and it never touches
/// `data`: the constants-immutability invariant below still holds. Each slot
/// is written at most once (from `nil`), so there is no overwritten value to
/// log, but the barrier still runs first: a dumped or tenured function must
/// enter the remembered set, or the next cycle would never trace the young
/// object it now holds. The concurrent GC thread reads the word atomically;
/// `object` was either allocated this cycle (born black) or is already
/// reachable from another traced owner (a prototype's code string).
///
/// Returns `false` for a non-bytecode `value` or an already-filled slot.
pub fn install_bytecode_slot_object(
    value: TaggedValue,
    slot: ByteCodeSlotObject,
    object: TaggedValue,
) -> bool {
    if value.veclike_type() != Some(VecLikeType::ByteCode) {
        return false;
    }
    let ptr = value.as_veclike_ptr().unwrap() as *const ByteCodeObj;
    // SAFETY: a live byte-code object (the type test above); the slot
    // words are atomics, written through a shared reference.
    let objects = unsafe { &(*ptr).slot_objects };
    if !objects.get(slot).is_nil() {
        return false;
    }
    note_heap_write(value, HeapWriteKind::ByteCodeData);
    objects.set(slot, object);
    true
}

/// Install the GNU closure-slot vector `aref` will report for every slot of
/// a byte-code function, on first use by the reader's substitutions.
///
/// The walk stores GNU's `ASET (closure, i, value)` results without mutating
/// the immutable `ByteCodeFunction`: the vector starts as a snapshot of the
/// observable slots and later stores go through `set_vector_slot`, which
/// already runs the write barrier and keeps the GC's view of the vector
/// sound.  `slot_objects.children()` includes it, so it is traced with the
/// function.
///
/// Returns `false` for a non-bytecode `value` or an already-installed slot
/// vector.
pub fn install_bytecode_slots_vector(value: TaggedValue, slots: TaggedValue) -> bool {
    if value.veclike_type() != Some(VecLikeType::ByteCode) {
        return false;
    }
    let ptr = value.as_veclike_ptr().unwrap() as *const ByteCodeObj;
    // SAFETY: a live byte-code object (the type test above); the word is an
    // atomic written through a shared reference.
    let objects = unsafe { &(*ptr).slot_objects };
    if !objects.slots_vector().is_nil() {
        return false;
    }
    note_heap_write(value, HeapWriteKind::ByteCodeData);
    objects.set_slots_vector(slots);
    true
}

/// TEST-ONLY mutation seam for a live bytecode object's `ByteCodeFunction`.
///
/// CONSTANTS-IMMUTABILITY IS A HARD INVARIANT (task 03/3a, load-bearing for
/// task 01's concurrent bytecode claiming): once a bytecode value has been
/// PUBLISHED (escaped `alloc_bytecode`), no production code may mutate its
/// `data` — in particular `constants`, whose backing a future GC-thread arm
/// will read without synchronization. The production surface upholds this by
/// construction: this seam is `#[cfg(test)]`, `aset` has no ByteCode arm, and
/// the only other `&mut` into a `ByteCodeObj`
/// (`pdump::convert::install_restored_bytecode_data`) is restore-time
/// initialization of a fresh placeholder BEFORE the value is user-observable
/// (pre-publish, like `alloc_bytecode`'s own header write). Any new mutation
/// path must first add vector-style clone-on-write (see
/// `with_vector_data_mut`) — do NOT simply un-gate this function.
///
/// The seam still fires the SATB pre-write barrier so tests that mutate
/// mid-cycle (e.g. self-referential print tests) keep snapshot marking sound.
#[cfg(any(test, feature = "test-support"))]
#[inline]
pub fn with_bytecode_data_mut_for_test<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut ByteCodeFunction) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    if value.veclike_type()? != VecLikeType::ByteCode {
        return None;
    }
    let ptr = raw as *mut ByteCodeObj;
    note_heap_write(value, HeapWriteKind::ByteCodeData);
    // SAFETY: `ptr` was validated as a live `ByteCodeObj` and `_guard` excludes re-entry.
    Some(f(unsafe { &mut (*ptr).data }))
}

#[inline]
pub fn with_marker_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispMarker) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    if value.veclike_type()? != VecLikeType::Marker {
        return None;
    }
    let ptr = raw as *mut MarkerObj;
    note_heap_write(value, HeapWriteKind::LispMarker);
    // SAFETY: `ptr` was validated as a live `MarkerObj` and `_guard` excludes re-entry.
    Some(f(unsafe { &mut (*ptr).data }))
}

#[inline]
pub fn with_overlay_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut OverlayData) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    // SAFETY: `is_veclike()` proved `raw` is a live veclike header and `_guard` excludes re-entry.
    if unsafe { checked_veclike_type_under_guard(raw) } != VecLikeType::Overlay {
        return None;
    }
    LispCollectionRevision::changed(value);
    let ptr = value.as_veclike_ptr().unwrap() as *mut OverlayObj;
    note_heap_write(value, HeapWriteKind::OverlayData);
    // SAFETY: `ptr` was validated as a live `OverlayObj` and `_guard` excludes re-entry.
    Some(f(unsafe { &mut (*ptr).data }))
}

#[inline]
pub fn with_xwidget_mut<R>(value: TaggedValue, f: impl FnOnce(&mut XwidgetObj) -> R) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    if value.veclike_type()? != VecLikeType::Xwidget {
        return None;
    }
    let ptr = raw as *mut XwidgetObj;
    note_heap_write(value, HeapWriteKind::XwidgetData);
    // SAFETY: `ptr` was validated as a live `XwidgetObj` and `_guard` excludes re-entry.
    Some(f(unsafe { &mut *ptr }))
}

#[inline]
pub fn with_xwidget_view_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut XwidgetViewObj) -> R,
) -> Option<R> {
    if !value.is_veclike() {
        return None;
    }
    let raw = (value.bits() & !TAG_MASK) as usize;
    let _guard = HeapMutClosureGuard::enter_object(raw);
    if value.veclike_type()? != VecLikeType::XwidgetView {
        return None;
    }
    let ptr = raw as *mut XwidgetViewObj;
    note_heap_write(value, HeapWriteKind::XwidgetViewData);
    // SAFETY: `ptr` was validated as a live `XwidgetViewObj` and `_guard` excludes re-entry.
    Some(f(unsafe { &mut *ptr }))
}

#[cfg(test)]
pub(crate) mod b13_test_hooks {
    use super::*;
    pub(crate) fn set_closure_slot(value: TaggedValue, index: usize, item: TaggedValue) -> bool {
        LispCollectionRevision::changed(value);
        match value.veclike_type() {
            // Atomic stores, as for vector slots: a concurrent reader of the slot
            // (the GC thread, once closures are traced there) sees a whole value.
            Some(VecLikeType::Lambda) => unsafe {
                let ptr = value.as_veclike_ptr().unwrap() as *mut LambdaObj;
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
                if index >= obj.data.len() {
                    return false;
                }
                crate::tagged::gc::gc_thread::b13_test_hooks::note_heap_slot_write(
                    value,
                    HeapWriteKind::ClosureSlot,
                    index,
                    item,
                );
                obj.data.store_atomic(index, item);
                true
            },
            Some(VecLikeType::Macro) => unsafe {
                let ptr = value.as_veclike_ptr().unwrap() as *mut MacroObj;
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
                if index >= obj.data.len() {
                    return false;
                }
                crate::tagged::gc::gc_thread::b13_test_hooks::note_heap_slot_write(
                    value,
                    HeapWriteKind::ClosureSlot,
                    index,
                    item,
                );
                obj.data.store_atomic(index, item);
                true
            },
            _ => false,
        }
    }
}

//! Centralized tagged-heap mutation helpers.
//!
//! These functions are the single place to hook future generational or
//! incremental write barriers into the tagged runtime.

use crate::buffer::text_props::TextPropertyTable;
#[cfg(test)]
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::value::LispHashTable;
use crate::heap_types::{LispMarker, LispString, OverlayData};

use super::collection_reads::{WriteProjection, record_projected_write};
use super::gc::{HeapWriteKind, note_heap_slot_write, note_heap_write};
use super::header::{
    ByteCodeObj, ByteCodeSlotObject, ConsCell, HashTableObj, LambdaObj, MacroObj, MarkerObj,
    OverlayObj, RecordObj, StringObj, VecLikeHeader, VecLikeType, VectorObj, XwidgetObj,
    XwidgetViewObj,
};
use super::value::{TAG_MASK, TaggedValue};

mod chartable;
mod string_observed;
pub use chartable::{CharTableWrite, SubCharTableWrite};
pub(crate) use chartable::{with_char_table_write, with_sub_char_table_write};

thread_local! {
    static COLLECTION_REVISION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Short-lived observations can depend on mutable Lisp collections nested
/// inside display properties without a buffer-property write. Allocating a
/// fresh object does not change this revision; mutating an existing one does.
/// Keep this separate from retained-row revisions: unrelated temporary-list
/// edits must not cancel off-screen acquisition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LispCollectionRevision(u64);

impl LispCollectionRevision {
    /// Exact mutation count for regression tests, including sequence wrap.
    #[cfg(test)]
    pub(crate) fn steps_since_for_test(self, earlier: Self) -> u64 {
        self.0.wrapping_sub(earlier.0)
    }

    pub fn current() -> Self {
        Self(COLLECTION_REVISION.with(std::cell::Cell::get))
    }

    #[inline]
    pub(super) fn sequence(self) -> u64 {
        self.0
    }

    #[inline]
    pub(super) fn advance() -> Self {
        COLLECTION_REVISION.with(|revision| {
            let next = revision.get().wrapping_add(1);
            revision.set(next);
            Self(next)
        })
    }

    #[inline]
    pub(crate) fn changed(value: TaggedValue) {
        if !super::collection_reads::history_required() {
            return;
        }
        COLLECTION_REVISION.with(|revision| {
            let next = revision.get().wrapping_add(1);
            revision.set(next);
            super::collection_reads::record_write(value, next);
        });
    }
}

#[cfg(debug_assertions)]
std::thread_local! {
    /// GEN-5: a bulk mutation may install new children after its pre-write
    /// barrier, so no collector safe point may occur until the closure ends.
    static IN_HEAP_MUT_CLOSURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(debug_assertions)]
#[must_use = "the thread-local extent ends when this guard drops"]
#[derive(Debug)]
pub(crate) struct HeapMutClosureGuard {
    _scope: crate::tls_scope::TlsScope<bool, std::cell::Cell<bool>>,
}
#[cfg(debug_assertions)]
static_assertions::assert_not_impl_any!(HeapMutClosureGuard: Send, Sync);

#[cfg(debug_assertions)]
impl HeapMutClosureGuard {
    #[inline]
    pub(crate) fn enter() -> Self {
        Self {
            _scope: crate::tls_scope::TlsScope::new(&IN_HEAP_MUT_CLOSURE, true),
        }
    }
}

#[cfg(debug_assertions)]
#[inline]
pub(crate) fn debug_assert_no_heap_mut_closure() {
    IN_HEAP_MUT_CLOSURE.with(|active| {
        assert!(
            !active.get(),
            "GC safe point inside a with_*_mut heap mutation closure (GEN-5)"
        );
    });
}

#[inline]
pub fn set_cons_car(cell: TaggedValue, value: TaggedValue) -> bool {
    if !record_projected_write(cell, WriteProjection::Cons) {
        return false;
    }
    note_heap_slot_write(cell, HeapWriteKind::ConsCar, 0, value);
    // The fused recorder proved the Cons tag and observed this pointer after
    // journaling. Its STATE borrow ended before the barrier; no callback or
    // collection revision change occurs between that observation and this store.
    unsafe {
        (*((cell.bits() & !TAG_MASK) as *mut ConsCell)).set_car(value);
    }
    true
}

#[inline]
pub fn set_cons_cdr(cell: TaggedValue, value: TaggedValue) -> bool {
    if !record_projected_write(cell, WriteProjection::Cons) {
        return false;
    }
    note_heap_slot_write(cell, HeapWriteKind::ConsCdr, 1, value);
    // As for set_cons_car: projection is already observed, and the barrier
    // has no Lisp callback or collection revision change.
    unsafe {
        (*((cell.bits() & !TAG_MASK) as *mut ConsCell)).set_cdr(value);
    }
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
pub(crate) fn set_compiled_cons_cdr(cell: TaggedValue, value: TaggedValue) -> bool {
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
    if lo <= address && address < hi && is_observed(cell.bits()) {
        super::gc::TaggedHeap::record_compiled_collection_write(cell.bits());
    } else {
        // A refusal into the original BLV shim learns the same empty gap as
        // the native cons guard. Its return does not rejoin a pre-store block,
        // so unobserved BLV loops keep their original register allocation.
        super::gc::neovm_jit_unobserved_collection_owner(cell.bits() as i64);
    }
    note_heap_slot_write(cell, HeapWriteKind::ConsCdr, 1, value);
    // SAFETY: the Cons tag names a live validated BLV owner. The barrier has
    // completed, and no callback or collection intervenes before this store.
    unsafe { (*((cell.bits() & !TAG_MASK) as *mut ConsCell)).set_cdr(value) };
    true
}

#[inline]
pub fn with_vector_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut Vec<TaggedValue>) -> R,
) -> Option<R> {
    LispCollectionRevision::changed(value);
    if value.veclike_type()? != VecLikeType::Vector {
        return None;
    }
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
        super::gc::with_tagged_heap(|heap| heap.concurrent_clone_on_write_vector(value));
    }
    let ptr = value.as_veclike_ptr().unwrap() as *mut VectorObj;
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    Some(f(unsafe { (*ptr).data.ensure_owned() }))
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
pub(crate) unsafe fn with_vector_slots_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(*mut TaggedValue, usize) -> R,
) -> Option<R> {
    let (slots, len) = with_vector_data_mut(value, |data| (data.as_mut_ptr(), data.len()))?;
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    Some(f(slots, len))
}

#[inline]
pub fn replace_vector_data(value: TaggedValue, items: Vec<TaggedValue>) -> bool {
    with_vector_data_mut(value, |data| *data = items).is_some()
}

#[inline]
pub fn set_vector_slot(value: TaggedValue, index: usize, item: TaggedValue) -> bool {
    if !record_projected_write(value, WriteProjection::VecLike) {
        return false;
    }
    // The observed header is reused for subtype and payload. Wrong veclike
    // subtypes still contributed a read, exactly as veclike_type did before.
    let header = (value.bits() & !TAG_MASK) as *const VecLikeHeader;
    if unsafe { (*header).type_tag } != VecLikeType::Vector {
        return false;
    }
    let ptr = header as *mut VectorObj;
    let data = unsafe { (*ptr).data.ensure_owned() };
    if index >= data.len() {
        return false;
    }
    note_heap_slot_write(value, HeapWriteKind::VectorSlot, index, item);
    // Atomic store so a concurrent GC read of this slot sees a whole value.
    unsafe { (*ptr).data.store_atomic(index, item) };
    true
}

#[inline]
pub fn with_record_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut Vec<TaggedValue>) -> R,
) -> Option<R> {
    LispCollectionRevision::changed(value);
    if value.veclike_type()? != VecLikeType::Record {
        return None;
    }
    note_heap_write(value, HeapWriteKind::RecordBulk);
    let ptr = value.as_veclike_ptr().unwrap() as *mut RecordObj;
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    Some(f(unsafe { (*ptr).data.ensure_owned() }))
}

#[inline]
pub fn replace_record_data(value: TaggedValue, items: Vec<TaggedValue>) -> bool {
    with_record_data_mut(value, |data| *data = items).is_some()
}

#[inline]
pub fn set_record_slot(value: TaggedValue, index: usize, item: TaggedValue) -> bool {
    if !record_projected_write(value, WriteProjection::VecLike) {
        return false;
    }
    let header = (value.bits() & !TAG_MASK) as *const VecLikeHeader;
    if unsafe { (*header).type_tag } != VecLikeType::Record {
        return false;
    }
    let ptr = header as *mut RecordObj;
    let data = unsafe { (*ptr).data.ensure_owned() };
    if index >= data.len() {
        return false;
    }
    note_heap_slot_write(value, HeapWriteKind::RecordSlot, index, item);
    // Atomic store, as for vector slots: a concurrent reader of the slot
    // (the GC thread, once records are traced there) sees a whole value.
    unsafe { (*ptr).data.store_atomic(index, item) };
    true
}

#[inline]
pub fn with_closure_slots_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut Vec<TaggedValue>) -> R,
) -> Option<R> {
    LispCollectionRevision::changed(value);
    note_heap_write(value, HeapWriteKind::ClosureBulk);
    match value.veclike_type()? {
        VecLikeType::Lambda => {
            let ptr = value.as_veclike_ptr().unwrap() as *mut LambdaObj;
            unsafe {
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
                #[cfg(debug_assertions)]
                let _guard = HeapMutClosureGuard::enter();
                Some(f(obj.data.ensure_owned()))
            }
        }
        VecLikeType::Macro => {
            let ptr = value.as_veclike_ptr().unwrap() as *mut MacroObj;
            unsafe {
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
                #[cfg(debug_assertions)]
                let _guard = HeapMutClosureGuard::enter();
                Some(f(obj.data.ensure_owned()))
            }
        }
        _ => None,
    }
}

#[inline]
pub fn replace_closure_slots(value: TaggedValue, slots: Vec<TaggedValue>) -> bool {
    with_closure_slots_mut(value, |data| *data = slots).is_some()
}

#[inline]
pub fn set_closure_slot(value: TaggedValue, index: usize, item: TaggedValue) -> bool {
    LispCollectionRevision::changed(value);
    match value.veclike_type() {
        // Atomic stores, as for vector slots: a concurrent reader of the slot
        // (the GC thread, once closures are traced there) sees a whole value.
        Some(VecLikeType::Lambda) => unsafe {
            let ptr = value.as_veclike_ptr().unwrap() as *mut LambdaObj;
            let obj = &mut *ptr;
            let _ = obj.parsed_params.take();
            if index >= obj.data.ensure_owned().len() {
                return false;
            }
            note_heap_slot_write(value, HeapWriteKind::ClosureSlot, index, item);
            obj.data.store_atomic(index, item);
            true
        },
        Some(VecLikeType::Macro) => unsafe {
            let ptr = value.as_veclike_ptr().unwrap() as *mut MacroObj;
            let obj = &mut *ptr;
            let _ = obj.parsed_params.take();
            if index >= obj.data.ensure_owned().len() {
                return false;
            }
            note_heap_slot_write(value, HeapWriteKind::ClosureSlot, index, item);
            obj.data.store_atomic(index, item);
            true
        },
        _ => false,
    }
}

#[inline]
pub fn with_string_text_props_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut TextPropertyTable) -> R,
) -> Option<R> {
    LispCollectionRevision::changed(value);
    let ptr = value.as_string_ptr()? as *mut StringObj;
    note_heap_write(value, HeapWriteKind::StringTextProps);
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    Some(f(unsafe { (*ptr).data.intervals_mut() }))
}

#[inline]
pub fn with_lisp_string_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispString) -> R,
) -> Option<R> {
    LispCollectionRevision::changed(value);
    let ptr = value.as_string_ptr()? as *mut StringObj;
    note_heap_write(value, HeapWriteKind::StringData);
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    // SAFETY: the owner is retained through this mutation, including unwind.
    let _observation = unsafe { string_observed::StringStorageObservationGuard::new(ptr) };
    Some(f(unsafe { &mut (*ptr).data }))
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
    LispCollectionRevision::changed(value);
    let Some(ptr) = value.as_string_ptr() else {
        return false;
    };
    let ptr = ptr as *mut StringObj;
    // SAFETY: a live string object; the caller holds the only mutation.
    unsafe {
        (*ptr)
            .data
            .set_byte_same_char_count_with_owned_observer(byte_pos, byte, |storage| {
                string_observed::observe_materialized_string_storage(ptr, storage);
            });
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
    unsafe { with_hash_table_mut_inactive(value, f) }
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
    debug_assert!(!super::gc::concurrent_hash_mutation_active());
    LispCollectionRevision::changed(value);
    if value.veclike_type()? != VecLikeType::HashTable {
        return None;
    }
    note_heap_write(value, HeapWriteKind::HashTableData);
    let ptr = value.as_veclike_ptr().unwrap() as *mut HashTableObj;
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
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    let result = f(unsafe { &mut (*ptr).table });
    unsafe { (*ptr).table.data.switch_epoch = epoch };
    Some(result)
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
    if value.veclike_type()? != VecLikeType::HashTable {
        return None;
    }
    let snapshot = super::gc::concurrent_hash_snapshot(value);
    let mut mutation = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.lock_mutation(value.as_veclike_ptr().unwrap() as usize));
    // The shared entry mutex must precede even journal/SATB preimage reads.
    LispCollectionRevision::changed(value);
    note_heap_write(value, HeapWriteKind::HashTableData);
    if let Some(guard) = mutation.as_mut() {
        super::gc::prepare_concurrent_hash_write(value, guard);
    }
    let ptr = value.as_veclike_ptr().unwrap() as *mut HashTableObj;
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
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
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
#[cfg(test)]
#[inline]
pub fn with_bytecode_data_mut_for_test<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut ByteCodeFunction) -> R,
) -> Option<R> {
    if value.veclike_type()? != VecLikeType::ByteCode {
        return None;
    }
    note_heap_write(value, HeapWriteKind::ByteCodeData);
    let ptr = value.as_veclike_ptr().unwrap() as *mut ByteCodeObj;
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    Some(f(unsafe { &mut (*ptr).data }))
}

#[inline]
pub fn with_marker_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispMarker) -> R,
) -> Option<R> {
    if value.veclike_type()? != VecLikeType::Marker {
        return None;
    }
    note_heap_write(value, HeapWriteKind::LispMarker);
    let ptr = value.as_veclike_ptr().unwrap() as *mut MarkerObj;
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    Some(f(unsafe { &mut (*ptr).data }))
}

#[inline]
pub fn with_overlay_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut OverlayData) -> R,
) -> Option<R> {
    LispCollectionRevision::changed(value);
    if value.veclike_type()? != VecLikeType::Overlay {
        return None;
    }
    note_heap_write(value, HeapWriteKind::OverlayData);
    let ptr = value.as_veclike_ptr().unwrap() as *mut OverlayObj;
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    Some(f(unsafe { &mut (*ptr).data }))
}

#[inline]
pub fn with_xwidget_mut<R>(value: TaggedValue, f: impl FnOnce(&mut XwidgetObj) -> R) -> Option<R> {
    if value.veclike_type()? != VecLikeType::Xwidget {
        return None;
    }
    note_heap_write(value, HeapWriteKind::XwidgetData);
    let ptr = value.as_veclike_ptr().unwrap() as *mut XwidgetObj;
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    Some(f(unsafe { &mut *ptr }))
}

#[inline]
pub fn with_xwidget_view_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut XwidgetViewObj) -> R,
) -> Option<R> {
    if value.veclike_type()? != VecLikeType::XwidgetView {
        return None;
    }
    note_heap_write(value, HeapWriteKind::XwidgetViewData);
    let ptr = value.as_veclike_ptr().unwrap() as *mut XwidgetViewObj;
    #[cfg(debug_assertions)]
    let _guard = HeapMutClosureGuard::enter();
    Some(f(unsafe { &mut *ptr }))
}

#[cfg(test)]
#[cfg(debug_assertions)]
#[path = "gc/tests/heap_mut_closure_test.rs"]
mod gc_heap_mut_closure_tests;

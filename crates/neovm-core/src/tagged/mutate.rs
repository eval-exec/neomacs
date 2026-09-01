//! Centralized tagged-heap mutation helpers.
//!
//! These functions are the single place to hook future generational or
//! incremental write barriers into the tagged runtime.

use crate::buffer::text_props::TextPropertyTable;
#[cfg(test)]
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::value::LispHashTable;
use crate::heap_types::{LispMarker, LispString, OverlayData};

use super::gc::{HeapWriteKind, note_heap_slot_write, note_heap_write};
#[cfg(test)]
use super::header::ByteCodeObj;
use super::header::{
    ConsCell, HashTableObj, LambdaObj, MacroObj, MarkerObj, OverlayObj, RecordObj, StringObj,
    VecLikeType, VectorObj, XwidgetObj, XwidgetViewObj,
};
use super::value::TaggedValue;

#[inline]
pub fn set_cons_car(cell: TaggedValue, value: TaggedValue) -> bool {
    if !cell.is_cons() {
        return false;
    }
    note_heap_slot_write(cell, HeapWriteKind::ConsCar, 0, value);
    unsafe {
        (*(cell.xcons_ptr() as *mut ConsCell)).set_car(value);
    }
    true
}

#[inline]
pub fn set_cons_cdr(cell: TaggedValue, value: TaggedValue) -> bool {
    if !cell.is_cons() {
        return false;
    }
    note_heap_slot_write(cell, HeapWriteKind::ConsCdr, 1, value);
    unsafe {
        (*(cell.xcons_ptr() as *mut ConsCell)).set_cdr(value);
    }
    true
}

#[inline]
pub fn with_vector_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut Vec<TaggedValue>) -> R,
) -> Option<R> {
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
    Some(f(unsafe { (*ptr).data.ensure_owned() }))
}

#[inline]
pub fn replace_vector_data(value: TaggedValue, items: Vec<TaggedValue>) -> bool {
    with_vector_data_mut(value, |data| *data = items).is_some()
}

#[inline]
pub fn set_vector_slot(value: TaggedValue, index: usize, item: TaggedValue) -> bool {
    if value.veclike_type() != Some(VecLikeType::Vector) {
        return false;
    }
    let ptr = value.as_veclike_ptr().unwrap() as *mut VectorObj;
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
    if value.veclike_type()? != VecLikeType::Record {
        return None;
    }
    note_heap_write(value, HeapWriteKind::RecordBulk);
    let ptr = value.as_veclike_ptr().unwrap() as *mut RecordObj;
    Some(f(unsafe { (*ptr).data.ensure_owned() }))
}

#[inline]
pub fn replace_record_data(value: TaggedValue, items: Vec<TaggedValue>) -> bool {
    with_record_data_mut(value, |data| *data = items).is_some()
}

#[inline]
pub fn set_record_slot(value: TaggedValue, index: usize, item: TaggedValue) -> bool {
    if value.veclike_type() != Some(VecLikeType::Record) {
        return false;
    }
    let ptr = value.as_veclike_ptr().unwrap() as *mut RecordObj;
    let data = unsafe { (*ptr).data.ensure_owned() };
    let slot = match data.get_mut(index) {
        Some(slot) => slot,
        None => return false,
    };
    note_heap_slot_write(value, HeapWriteKind::RecordSlot, index, item);
    *slot = item;
    true
}

#[inline]
pub fn with_closure_slots_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut Vec<TaggedValue>) -> R,
) -> Option<R> {
    note_heap_write(value, HeapWriteKind::ClosureBulk);
    match value.veclike_type()? {
        VecLikeType::Lambda => {
            let ptr = value.as_veclike_ptr().unwrap() as *mut LambdaObj;
            unsafe {
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
                Some(f(obj.data.ensure_owned()))
            }
        }
        VecLikeType::Macro => {
            let ptr = value.as_veclike_ptr().unwrap() as *mut MacroObj;
            unsafe {
                let obj = &mut *ptr;
                let _ = obj.parsed_params.take();
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
    match value.veclike_type() {
        Some(VecLikeType::Lambda) => unsafe {
            let ptr = value.as_veclike_ptr().unwrap() as *mut LambdaObj;
            let obj = &mut *ptr;
            let _ = obj.parsed_params.take();
            let slot = match obj.data.get_mut(index) {
                Some(slot) => slot,
                None => return false,
            };
            note_heap_slot_write(value, HeapWriteKind::ClosureSlot, index, item);
            *slot = item;
            true
        },
        Some(VecLikeType::Macro) => unsafe {
            let ptr = value.as_veclike_ptr().unwrap() as *mut MacroObj;
            let obj = &mut *ptr;
            let _ = obj.parsed_params.take();
            let slot = match obj.data.get_mut(index) {
                Some(slot) => slot,
                None => return false,
            };
            note_heap_slot_write(value, HeapWriteKind::ClosureSlot, index, item);
            *slot = item;
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
    let ptr = value.as_string_ptr()? as *mut StringObj;
    note_heap_write(value, HeapWriteKind::StringTextProps);
    Some(f(unsafe { (*ptr).data.intervals_mut() }))
}

#[inline]
pub fn with_lisp_string_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispString) -> R,
) -> Option<R> {
    let ptr = value.as_string_ptr()? as *mut StringObj;
    note_heap_write(value, HeapWriteKind::StringData);
    Some(f(unsafe { &mut (*ptr).data }))
}

#[inline]
pub fn with_hash_table_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut LispHashTable) -> R,
) -> Option<R> {
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
    }
    Some(f(unsafe { &mut (*ptr).table }))
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
    Some(f(unsafe { &mut (*ptr).data }))
}

#[inline]
pub fn with_overlay_data_mut<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut OverlayData) -> R,
) -> Option<R> {
    if value.veclike_type()? != VecLikeType::Overlay {
        return None;
    }
    note_heap_write(value, HeapWriteKind::OverlayData);
    let ptr = value.as_veclike_ptr().unwrap() as *mut OverlayObj;
    Some(f(unsafe { &mut (*ptr).data }))
}

#[inline]
pub fn with_xwidget_mut<R>(value: TaggedValue, f: impl FnOnce(&mut XwidgetObj) -> R) -> Option<R> {
    if value.veclike_type()? != VecLikeType::Xwidget {
        return None;
    }
    note_heap_write(value, HeapWriteKind::XwidgetData);
    let ptr = value.as_veclike_ptr().unwrap() as *mut XwidgetObj;
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
    Some(f(unsafe { &mut *ptr }))
}

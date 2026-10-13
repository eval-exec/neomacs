//! Fixed-shape char-table writes. A capability exposes stores, never writable
//! slots or a growable backing. The marker captures backings at the stopped
//! start handshake, so pdump copy-on-write changes only mutator-local metadata.

use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering, fence};

use super::{LispCollectionRevision, note_heap_write};
use crate::tagged::gc::HeapWriteKind;
use crate::tagged::header::{CharTableObj, LispValueVec, SubCharTableObj};
use crate::tagged::value::TaggedValue;

static_assertions::assert_eq_size!(TaggedValue, AtomicUsize);
static_assertions::assert_eq_align!(TaggedValue, AtomicUsize);

/// The sole slot-store seam. Its constructor is private to the barriered
/// capability constructors below; there is no unbarriered public writer.
#[derive(Debug)]
struct SlotWrite;

impl SlotWrite {
    /// # Safety
    /// `slot` is a live aligned writable word, retained until marker join.
    /// The capability's pre-write barrier has run, with no intervening safe
    /// point. Concurrent marker access uses Acquire atomic loads.
    #[inline(always)]
    unsafe fn publish(&self, slot: *mut TaggedValue, value: TaggedValue) {
        // Publish newly constructed pointees too: a Release fence followed by
        // this Relaxed store synchronizes with the marker's Acquire load that
        // reads it. Both compile away to the original mov on x86; omitting the
        // fence would lose constructor publication on arm64.
        fence(Ordering::Release);
        // SAFETY: the caller supplies the retained aligned word and the
        // barrier-before-store contract; the pins above admit its atomic view.
        unsafe { (*slot.cast::<AtomicUsize>()).store(value.bits(), Ordering::Relaxed) };
    }

    #[inline(always)]
    fn backing_slot(&self, backing: &mut LispValueVec, index: usize, value: TaggedValue) {
        // Table shapes never grow after construction. On a mapped backing,
        // ensure_owned copies the immutable image; the marker keeps its
        // start-captured span and never rereads this storage enum.
        let data = backing.ensure_owned();
        if index < data.len() {
            // SAFETY: the bounds check proves an initialized aligned slot.
            // This capability cannot resize or retire an owned table backing.
            unsafe { self.publish(data.as_mut_ptr().add(index), value) };
        }
    }
}

/// Barriered, thread-confined writes to a char-table's fixed shape. No method
/// exposes a mutable object, mutable slot, or growable extra-slot vector.
#[derive(Debug)]
pub struct CharTableWrite<'a> {
    object: NonNull<CharTableObj>,
    slots: SlotWrite,
    _loan: PhantomData<&'a mut CharTableObj>,
}
static_assertions::assert_not_impl_any!(CharTableWrite<'static>: Send, Sync, Clone, Copy);

impl CharTableWrite<'_> {
    #[inline(always)]
    pub fn set_default(&mut self, value: TaggedValue) {
        // SAFETY: the capability retains the validated table and its barrier.
        unsafe {
            self.slots.publish(
                std::ptr::addr_of_mut!((*self.object.as_ptr()).defalt),
                value,
            )
        };
    }

    #[inline(always)]
    pub fn set_parent(&mut self, value: TaggedValue) {
        // SAFETY: the capability retains the validated table and its barrier.
        unsafe {
            self.slots.publish(
                std::ptr::addr_of_mut!((*self.object.as_ptr()).parent),
                value,
            )
        };
    }

    #[inline(always)]
    pub fn set_purpose(&mut self, value: TaggedValue) {
        // SAFETY: the capability retains the validated table and its barrier.
        unsafe {
            self.slots.publish(
                std::ptr::addr_of_mut!((*self.object.as_ptr()).purpose),
                value,
            )
        };
    }

    #[inline(always)]
    pub fn set_ascii(&mut self, value: TaggedValue) {
        // SAFETY: the capability retains the validated table and its barrier.
        unsafe {
            self.slots
                .publish(std::ptr::addr_of_mut!((*self.object.as_ptr()).ascii), value)
        };
    }

    #[inline(always)]
    pub fn set_contents(&mut self, index: usize, value: TaggedValue) {
        if index < crate::tagged::header::CHAR_TABLE_TOP_SLOTS {
            // SAFETY: the bounds check selects a live fixed inline slot;
            // the capability retains the table and its pre-write barrier.
            unsafe {
                self.slots.publish(
                    std::ptr::addr_of_mut!((*self.object.as_ptr()).contents)
                        .cast::<TaggedValue>()
                        .add(index),
                    value,
                )
            };
        }
    }

    #[inline]
    pub fn fill_contents(&mut self, value: TaggedValue) {
        for index in 0..crate::tagged::header::CHAR_TABLE_TOP_SLOTS {
            self.set_contents(index, value);
        }
    }

    #[inline(always)]
    pub fn set_extra(&mut self, index: usize, value: TaggedValue) {
        // SAFETY: only the mutator accesses backing metadata; the marker
        // reads its start snapshot. The capability forbids backing growth.
        let backing = unsafe { &mut (*self.object.as_ptr()).extras };
        self.slots.backing_slot(backing, index, value);
    }

    pub fn copy_contents(
        &mut self,
        values: &[TaggedValue; crate::tagged::header::CHAR_TABLE_TOP_SLOTS],
    ) {
        for (index, &value) in values.iter().enumerate() {
            self.set_contents(index, value);
        }
    }

    pub fn copy_extras(&mut self, values: &[TaggedValue]) {
        for (index, &value) in values.iter().enumerate() {
            self.set_extra(index, value);
        }
    }
}

/// Barriered writes to an interior node. Depth, minimum character and slot
/// count are construction-time metadata, inaccessible to this capability.
#[derive(Debug)]
pub struct SubCharTableWrite<'a> {
    object: NonNull<SubCharTableObj>,
    slots: SlotWrite,
    _loan: PhantomData<&'a mut SubCharTableObj>,
}
static_assertions::assert_not_impl_any!(SubCharTableWrite<'static>: Send, Sync, Clone, Copy);

impl SubCharTableWrite<'_> {
    #[inline(always)]
    pub fn set_contents(&mut self, index: usize, value: TaggedValue) {
        // SAFETY: backing metadata is mutator-only, and this capability
        // permits only in-place stores (or the immutable pdump COW copy).
        let backing = unsafe { &mut (*self.object.as_ptr()).contents };
        self.slots.backing_slot(backing, index, value);
    }

    pub fn copy_contents(&mut self, values: &[TaggedValue]) {
        for (index, &value) in values.iter().enumerate() {
            self.set_contents(index, value);
        }
    }
}

#[inline]
pub(crate) fn with_char_table_write<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut CharTableWrite<'_>) -> R,
) -> Option<R> {
    if !value.is_char_table() {
        return None;
    }
    LispCollectionRevision::changed(value);
    note_heap_write(value, HeapWriteKind::CharTableData);
    let object = NonNull::new(value.as_veclike_ptr()?.cast_mut().cast::<CharTableObj>())?;
    #[cfg(debug_assertions)]
    let _guard = super::HeapMutClosureGuard::enter();
    Some(f(&mut CharTableWrite {
        object,
        slots: SlotWrite,
        _loan: PhantomData,
    }))
}

#[inline]
pub(crate) fn with_sub_char_table_write<R>(
    value: TaggedValue,
    f: impl FnOnce(&mut SubCharTableWrite<'_>) -> R,
) -> Option<R> {
    if !value.is_sub_char_table() {
        return None;
    }
    LispCollectionRevision::changed(value);
    note_heap_write(value, HeapWriteKind::SubCharTableData);
    let object = NonNull::new(value.as_veclike_ptr()?.cast_mut().cast::<SubCharTableObj>())?;
    #[cfg(debug_assertions)]
    let _guard = super::HeapMutClosureGuard::enter();
    Some(f(&mut SubCharTableWrite {
        object,
        slots: SlotWrite,
        _loan: PhantomData,
    }))
}

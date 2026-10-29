//! Frozen 64-bit serial registry layout. Effect metadata belongs in sidecars.

use super::{InlineSubr, SubrEntry};
use std::mem::{align_of, offset_of, size_of, size_of_val};
use std::thread::LocalKey;

const fn slot_size<T: 'static>(_: &LocalKey<T>) -> usize {
    size_of::<T>()
}

const fn slot_align<T: 'static>(_: &LocalKey<T>) -> usize {
    align_of::<T>()
}

// These are the existing x86_64/aarch64 shapes, not a requested new repr.
// Keep repr(Rust): adding repr(C) here would itself change the serial layout.
#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(size_of::<SubrEntry>() == 48);
    assert!(align_of::<SubrEntry>() == 8);
    assert!(offset_of!(SubrEntry, interactive_spec) == 0);
    assert!(offset_of!(SubrEntry, function) == 24);
    assert!(offset_of!(SubrEntry, max_args) == 40);
    assert!(offset_of!(SubrEntry, min_args) == 44);
    assert!(offset_of!(SubrEntry, dispatch_kind) == 46);
    assert!(size_of::<Option<SubrEntry>>() == 48);
    assert!(size_of::<InlineSubr>() == 24);
    assert!(align_of::<InlineSubr>() == 8);
    assert!(offset_of!(InlineSubr, function) == 0);
    assert!(offset_of!(InlineSubr, kind) == 16);
    assert!(slot_size(&super::GLOBAL_SUBR_TABLE) == 32);
    assert!(slot_align(&super::GLOBAL_SUBR_TABLE) == 8);
    assert!(slot_size(&super::INLINE_SUBR_TABLE) == 32);
    assert!(slot_align(&super::INLINE_SUBR_TABLE) == 8);
    assert!(size_of_val(&super::BUILTIN_SYM_BITS) == 131_072);
};

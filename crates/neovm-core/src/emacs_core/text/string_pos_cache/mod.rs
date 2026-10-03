//! One-entry character<->byte position cache for multibyte strings.
//!
//! Mirrors GNU `string_char_to_byte` / `string_byte_to_char` (fns.c) and
//! their `string_char_byte_cache_*` variables: a conversion walks from
//! whichever of the start, the end, or the last conversion on the same
//! string is nearest, then remembers where it landed.  Without it every
//! conversion scanned from one end, so a loop over a multibyte string --
//! `aref' by index, `substring' pieces, `string-match' with START as
//! `split-string' calls it -- was quadratic.
//!
//! The entry names its string by object identity, as GNU's `EQ` does, and
//! roots it (GNU `staticpro`s the cache variable), so the object cannot be
//! freed and its address reused while it is cached.  Any change to a
//! string's bytes moves [`EPOCH`] (see `LispString::recompute_size`, which
//! every byte mutation ends in), so a cached pair never outlives the layout
//! it describes. Activation also invalidates an entry after an uncovered
//! collection or thread migration: another thread cannot root this entry and
//! has its own byte-mutation epoch.

use std::cell::Cell;

use crate::emacs_core::emacs_char;
use crate::emacs_core::value::Value;
use crate::heap_types::LispString;

#[derive(Clone, Copy)]
struct Entry {
    string: Value,
    data: *const u8,
    sbytes: usize,
    epoch: u64,
    char_pos: usize,
    byte_pos: usize,
}

thread_local! {
    static CACHE: Cell<Option<Entry>> = const { Cell::new(None) };
    static EPOCH: Cell<u64> = const { Cell::new(0) };
    static CACHE_HEAP: Cell<usize> = const { Cell::new(0) };
    // Last collection whose root enumeration included the entry, or the
    // completed count at which an empty cache was activated.
    static CACHE_COLLECTION_EPOCH: Cell<Option<usize>> = const { Cell::new(None) };
}

/// Some string's bytes changed: every cached pair is suspect.
#[inline]
pub(crate) fn note_string_bytes_changed() {
    EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
}

/// Forget the entry (heap reset, pdump load).
pub(crate) fn reset_string_pos_cache() {
    CACHE.with(|cache| cache.set(None));
    CACHE_HEAP
        .with(|owner| owner.set(crate::tagged::gc::current_tagged_heap_identity().unwrap_or(0)));
    CACHE_COLLECTION_EPOCH.with(|epoch| epoch.set(None));
}

/// Validate ownership at activation, never during character/byte conversion.
/// A collection on another thread cannot see this entry. Reject an uncovered
/// running collection before it sweeps, and retain covered same-thread cycles.
pub(crate) fn activate_string_pos_cache(
    heap_identity: usize,
    collection_epoch: usize,
    collection_in_progress: bool,
    thread_changed: bool,
) {
    CACHE_HEAP.with(|owner| {
        CACHE_COLLECTION_EPOCH.with(|epoch| {
            let valid = epoch.get() == Some(collection_epoch + 1)
                || (!collection_in_progress && epoch.get() == Some(collection_epoch));
            if owner.get() != heap_identity || !valid || thread_changed {
                CACHE.with(|cache| cache.set(None));
                owner.set(heap_identity);
                epoch.set(Some(collection_epoch));
            }
        });
    });
}

/// The cached string is a GC root, like GNU's staticpro'd cache variable.
pub(crate) fn collect_string_pos_cache_gc_roots(
    roots: &mut Vec<Value>,
    heap_identity: usize,
    collection_epoch: usize,
    scan: crate::tagged::gc::CacheRootScan,
) {
    if CACHE_HEAP.with(Cell::get) != heap_identity {
        return;
    }
    match scan {
        crate::tagged::gc::CacheRootScan::Collection => {
            CACHE_COLLECTION_EPOCH.with(|epoch| {
                if epoch.get() != Some(collection_epoch)
                    && epoch.get() != Some(collection_epoch + 1)
                {
                    CACHE.with(|cache| cache.set(None));
                }
                epoch.set(Some(collection_epoch + 1));
            });
        }
        #[cfg(test)]
        crate::tagged::gc::CacheRootScan::Snapshot {
            collection_in_progress,
        } => {
            activate_string_pos_cache(
                heap_identity,
                collection_epoch,
                collection_in_progress,
                false,
            );
        }
    }
    CACHE.with(|cache| {
        if let Some(entry) = cache.get() {
            roots.push(entry.string);
        }
    });
}

fn cached_pair(string: Value, s: &LispString) -> Option<(usize, usize)> {
    let entry = CACHE.with(Cell::get)?;
    (entry.string.bits() == string.bits()
        && entry.data == s.as_bytes().as_ptr()
        && entry.sbytes == s.sbytes()
        && entry.epoch == EPOCH.with(Cell::get))
    .then_some((entry.char_pos, entry.byte_pos))
}

fn remember(string: Value, s: &LispString, char_pos: usize, byte_pos: usize) {
    let entry = Entry {
        string,
        data: s.as_bytes().as_ptr(),
        sbytes: s.sbytes(),
        epoch: EPOCH.with(Cell::get),
        char_pos,
        byte_pos,
    };
    CACHE.with(|cache| cache.set(Some(entry)));
}

/// The byte offset of character `char_index` of `string` (`s` is its
/// payload), clamped to the end.  GNU `string_char_to_byte`.
pub(crate) fn string_char_to_byte(string: Value, s: &LispString, char_index: usize) -> usize {
    let schars = s.schars();
    let sbytes = s.sbytes();
    let char_index = char_index.min(schars);
    if !s.is_multibyte() || schars == sbytes {
        return char_index;
    }
    let bytes = s.as_bytes();
    let (mut below, mut below_byte, mut above, mut above_byte) = (0, 0, schars, sbytes);
    if let Some((char_pos, byte_pos)) = cached_pair(string, s) {
        if char_pos < char_index {
            (below, below_byte) = (char_pos, byte_pos);
        } else {
            (above, above_byte) = (char_pos, byte_pos);
        }
    }
    let byte_index = if char_index - below < above - char_index {
        below_byte + emacs_char::char_to_byte_pos(&bytes[below_byte..], char_index - below)
    } else {
        emacs_char::char_to_byte_pos_from_end(&bytes[..above_byte], above - char_index)
    };
    remember(string, s, char_index, byte_index);
    byte_index
}

/// The number of characters of `string` that start before byte offset
/// `byte_index` (clamped to the end).  GNU `string_byte_to_char`.
pub(crate) fn string_byte_to_char(string: Value, s: &LispString, byte_index: usize) -> usize {
    let schars = s.schars();
    let sbytes = s.sbytes();
    let byte_index = byte_index.min(sbytes);
    if !s.is_multibyte() || schars == sbytes {
        return byte_index;
    }
    let bytes = s.as_bytes();
    let (mut below, mut below_byte, mut above, mut above_byte) = (0, 0, schars, sbytes);
    if let Some((char_pos, byte_pos)) = cached_pair(string, s) {
        if byte_pos < byte_index {
            (below, below_byte) = (char_pos, byte_pos);
        } else {
            (above, above_byte) = (char_pos, byte_pos);
        }
    }
    let char_index = if byte_index - below_byte < above_byte - byte_index {
        below
            + emacs_char::byte_to_char_pos(&bytes[below_byte..byte_index], byte_index - below_byte)
    } else {
        above
            - emacs_char::byte_to_char_pos(&bytes[byte_index..above_byte], above_byte - byte_index)
    };
    // Only a character boundary is a pair a later conversion can start from.
    if byte_index == sbytes || (bytes[byte_index] & 0xC0) != 0x80 {
        remember(string, s, char_index, byte_index);
    }
    char_index
}

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/gc_tls_ownership.rs"]
mod gc_tls_ownership_tests;

#[cfg(test)]
#[path = "tests/gc_collection_epoch.rs"]
mod gc_collection_epoch_tests;

#[cfg(test)]
#[path = "tests/gc_collection_epoch_minor.rs"]
mod gc_collection_epoch_minor_tests;

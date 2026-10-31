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
//! string's bytes moves [`EPOCH`] (see `LispString::recompute_size` and
//! `LispString::clear_contents_to_unibyte`), so a cached pair never outlives
//! the layout it describes. Activation also invalidates an entry after an uncovered
//! collection or thread migration: another thread cannot root this entry and
//! has its own byte-mutation epoch.

use std::cell::Cell;

use crate::emacs_core::emacs_char;
use crate::emacs_core::value::Value;
use crate::heap_types::{GnuStringCharPos, LispString};

#[derive(Clone, Copy, Debug)]
struct Entry {
    string: Value,
    data: *const u8,
    sbytes: usize,
    epoch: u64,
    char_pos: GnuStringCharPos,
    byte_pos: usize,
}

// This existing TLS entry remains owned by the activated mutator/heap.
// Signed numeric metadata adds no new TLS or cross-mutator ownership.

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
        crate::tagged::gc::CacheRootScan::Parked => {
            // A publication preserves roots without advancing a collection stamp.
            activate_string_pos_cache(heap_identity, collection_epoch, false, false);
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

fn cached_pair(string: Value, s: &LispString) -> Option<(GnuStringCharPos, usize)> {
    let entry = CACHE.with(Cell::get)?;
    (entry.string.bits() == string.bits()
        && entry.data == s.as_bytes().as_ptr()
        && entry.sbytes == s.sbytes()
        && entry.epoch == EPOCH.with(Cell::get))
    .then_some((entry.char_pos, entry.byte_pos))
}

fn remember(string: Value, s: &LispString, char_pos: GnuStringCharPos, byte_pos: usize) {
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

/// The byte offset of nonnegative character `char_index`, clamped to SCHARS.
/// GNU fns.c:1194-1241 keeps even a negative cached anchor signed.
/// Valid GNU metadata fits ptrdiff_t (owned bytes and supplied counts are
/// bounded by isize::MAX); absolute signed distances fit usize on that domain.
pub(crate) fn string_char_to_byte(string: Value, s: &LispString, char_index: usize) -> usize {
    let schars = s.schars();
    let sbytes = s.sbytes();
    let char_index = char_index.min(schars);
    if !s.is_multibyte() || schars == sbytes {
        return char_index;
    }
    let target = char_index as i64;
    let bytes = s.as_bytes();
    let (mut below, mut below_byte, mut above, mut above_byte) = (0_i64, 0, schars as i64, sbytes);
    if let Some((char_pos, byte_pos)) = cached_pair(string, s) {
        if char_pos.get() < target {
            (below, below_byte) = (char_pos.get(), byte_pos);
        } else {
            (above, above_byte) = (char_pos.get(), byte_pos);
        }
    }
    // abs_diff is defined even across zero: target - a negative cached anchor
    // must not wrap or overflow. Both distances fit the string's usize domain.
    let below_distance = target.abs_diff(below) as usize;
    let above_distance = above.abs_diff(target) as usize;
    let byte_index = if below_distance < above_distance {
        below_byte + emacs_char::char_to_byte_pos(&bytes[below_byte..], below_distance)
    } else {
        emacs_char::char_to_byte_pos_from_end(&bytes[..above_byte], above_distance)
    };
    remember(string, s, GnuStringCharPos::new(target), byte_index);
    byte_index
}

/// GNU's signed character coordinate at byte_index, clamped to physical SBYTES.
/// fns.c:1260-1308 preserves signed cached anchors and SCHARS-minus-suffix.
/// Initialized physical bytes bound every scan; GNU-representable metadata
/// bounds its signed result. Invalid Rust-only dump headers are not admitted
/// by the supplied-count constructor and must be rejected by their caller.
pub(crate) fn string_byte_to_gnu_char(
    string: Value,
    s: &LispString,
    byte_index: usize,
) -> GnuStringCharPos {
    let schars = s.schars();
    let sbytes = s.sbytes();
    let byte_index = byte_index.min(sbytes);
    if !s.is_multibyte() || schars == sbytes {
        return GnuStringCharPos::new(byte_index as i64);
    }
    let bytes = s.as_bytes();
    let (mut below, mut below_byte, mut above, mut above_byte) = (0_i64, 0, schars as i64, sbytes);
    if let Some((char_pos, byte_pos)) = cached_pair(string, s) {
        if byte_pos < byte_index {
            (below, below_byte) = (char_pos.get(), byte_pos);
        } else {
            (above, above_byte) = (char_pos.get(), byte_pos);
        }
    }
    let char_index = if byte_index - below_byte < above_byte - byte_index {
        below
            + emacs_char::byte_to_char_pos(&bytes[below_byte..byte_index], byte_index - below_byte)
                as i64
    } else {
        above
            - emacs_char::byte_to_char_pos(&bytes[byte_index..above_byte], above_byte - byte_index)
                as i64
    };
    let char_pos = GnuStringCharPos::new(char_index);
    // Preserve the existing boundary-only cache rule. Interior continuation
    // offsets are not safe physical anchors for another conversion.
    if byte_index == sbytes || (bytes[byte_index] & 0xC0) != 0x80 {
        remember(string, s, char_pos, byte_index);
    }
    char_pos
}

#[cfg(test)]
#[path = "tests/string_pos_cache_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/gc_tls_ownership_test.rs"]
mod gc_tls_ownership_tests;

#[cfg(test)]
#[path = "tests/gc_collection_epoch_test.rs"]
mod gc_collection_epoch_tests;

#[cfg(test)]
#[path = "tests/gc_collection_epoch_minor_test.rs"]
mod gc_collection_epoch_minor_tests;

#[cfg(test)]
#[path = "tests/signed_string_positions.rs"]
mod signed_string_positions_tests;

/// Carrier park: reset the owner-installed cache and the per-thread epoch.
pub(crate) fn reset_carrier_tls() {
    reset_string_pos_cache();
    EPOCH.with(|slot| slot.set(0));
}

/// Debug-only carrier TLS fingerprint for this module's classified records.
#[cfg(debug_assertions)]
pub(crate) fn carrier_tls_fingerprint(h: &mut u64) {
    crate::tls_fingerprint::mix(h, EPOCH.with(|slot| slot.get() as u64));
}

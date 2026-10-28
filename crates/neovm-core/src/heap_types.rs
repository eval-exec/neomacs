//! Shared heap payload types used by both the tagged runtime and pdump code.
//!
//! Keeping them behind a neutral module boundary lets the tagged runtime and
//! dump/load code share the same payload structs without reviving old heap
//! module boundaries.

use crate::buffer::text_props::{EmptyTextProperties, TextPropertiesRef};
use crate::buffer::{BufferId, CharLen, CharPos0, CharRange, EmacsBytePos, TextPropertyTable};
use crate::emacs_core::emacs_char;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::{Mutex, OnceLock};

/// A Lisp string.
///
/// Backing bytes use Emacs internal encoding (a UTF-8 superset). For standard
/// Unicode text the bytes are valid UTF-8; raw bytes 0x80-0xFF are encoded as
/// overlong two-byte sequences (C0/C1 lead byte) which makes `as_str()` return
/// `None` for those strings. Like GNU, `SDATA` has a trailing NUL byte after
/// `SBYTES`; that terminator is not part of the Lisp string contents.
///
/// - **Multibyte:** `size_byte >= 0`. Masked `size` is the char count.
/// - **Unibyte:**   `size_byte < 0`. `size` = byte count (each byte is one char).
///   GNU distinguishes `-1` normally allocated, `-2` rodata, and `-3`
///   immovable bytecode storage.
#[repr(C)]
pub struct LispString {
    /// Character count plus immutable count-proof metadata in the spare top bit.
    /// Counts fit isize::MAX; SCHARS and compiled consumers mask metadata.
    size: usize,
    /// Byte count for multibyte strings, or GNU's negative unibyte marker.
    size_byte: i64,
    /// GNU Lisp_String-compatible interval pointer.  A null pointer in GNU
    /// means no interval tree; null is also what a raw mapped string object
    /// contains before load installs Neomacs sidecars.
    ///
    /// CONCURRENT GC (interval-free string claiming): retyped from
    /// `Option<Box<TextPropertyTable>>` to a raw atomic pointer — same size,
    /// same null niche, same `#[repr(C)]` offset, and the mapped pdump image
    /// still stores 0 here — so the concurrent GC thread can read the pointer
    /// WORD without a data race while the mutator installs (`ensure_intervals`)
    /// or frees (`clear_intervals`) tables. The GC thread may ONLY null-check
    /// this word (`intervals_ptr`); it must NEVER dereference the table, which
    /// the mutator can free at any moment. Managed via `Box::into_raw` /
    /// `Box::from_raw`; every store that publishes a table is `Release` of a
    /// fully-constructed table.
    intervals: AtomicPtr<TextPropertyTable>,
    /// Direct string byte pointer, like GNU's `Lisp_String.u.s.data`.
    ///
    data: *const u8,
    /// Capacity of allocator-owned storage, including the trailing NUL. Zero
    /// denotes borrowed mapped/static bytes. The logical length comes from
    /// `size`/`size_byte`, so this one word is enough to reconstruct the Vec
    /// for mutation or drop without a separately allocated storage sidecar.
    storage_capacity: usize,
}

const SIZE_BYTE_UNIBYTE_NORMAL: i64 = -1;
const SIZE_BYTE_UNIBYTE_RODATA: i64 = -2;
const SIZE_BYTE_UNIBYTE_IMMOVABLE: i64 = -3;

// Retaining a small cleared payload avoids a second allocation and zero fill.
// Larger copied payloads are recycled so their materialized storage can be
// reused before the string object is swept. This is a storage policy only;
// both paths securely erase all initialized bytes and preserve Lisp identity.
const CLEAR_STRING_RETAIN_BYTES: usize = 4096;

/// The two GNU string storage representations.
///
/// Keep this distinction typed at allocation boundaries: an empty unibyte
/// string and an empty multibyte string are different Lisp objects even though
/// both have zero bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LispStringStorageKind {
    Unibyte,
    Multibyte,
}

// The byte pointer alone does not justify sharing this owner: its interval
// table can hold mutator-local Values. Collector access to the atomic interval
// pointer word is admitted separately; it never dereferences that table.
static_assertions::assert_not_impl_any!(LispString: Send, Sync);
static_assertions::assert_not_impl_any!(TextPropertyTable: Send, Sync);

/// Whether the declared count has a decoded-payload proof.
///
/// Supplied counts are valid GNU metadata without requiring agreement with a
/// payload scan. Computed permits normal syntax-property fast paths. Immutable
/// numeric metadata has no Lisp state and may cross mutator threads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StringCharacterCountKind {
    Computed,
    Supplied,
}
static_assertions::assert_impl_all!(StringCharacterCountKind: Send, Sync);

bitflags::bitflags! {
    /// Private immutable metadata within the size word; no payload layout change.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct StringCharacterCountFlags: usize {
        const SUPPLIED = 1usize << (usize::BITS - 1);
    }
}
static_assertions::assert_impl_all!(StringCharacterCountFlags: Send, Sync);

// Compiled code reads the size word without masking (`LispString::
// JIT_SIZE_OFFSET`): the proof bit sits above every byte count, so `size ==
// size_byte` fails for a supplied count, and fixnum tagging shifts it out of
// a compiled `length`.
const _: () = assert!(StringCharacterCountFlags::SUPPLIED.bits() > isize::MAX as usize);
const _: () = assert!(
    StringCharacterCountFlags::all().bits().trailing_zeros()
        >= usize::BITS - crate::tagged::value::FIXNUM_SHIFT
);

#[derive(Clone, Copy)]
struct StaticRoDataEntry {
    ptr: usize,
    len: usize,
}

fn static_rodata_registry() -> &'static Mutex<HashMap<u64, StaticRoDataEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, StaticRoDataEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn static_rodata_key(bytes_with_nul: &[u8]) -> u64 {
    // Stable FNV-1a over the exact executable rodata bytes, including GNU's
    // trailing NUL.
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes_with_nul {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn register_static_rodata(data_with_nul: &'static [u8]) -> u64 {
    let key = static_rodata_key(data_with_nul);
    let len = data_with_nul.len() - 1;
    let mut registry = static_rodata_registry()
        .lock()
        .expect("static rodata registry poisoned");
    if let Some(existing) = registry.get(&key) {
        let existing_bytes =
            unsafe { std::slice::from_raw_parts(existing.ptr as *const u8, existing.len + 1) };
        assert_eq!(
            existing_bytes, data_with_nul,
            "static rodata string key collision"
        );
    } else {
        registry.insert(
            key,
            StaticRoDataEntry {
                ptr: data_with_nul.as_ptr() as usize,
                len,
            },
        );
    }
    key
}

fn lookup_static_rodata(key: u64, len: usize) -> Option<*const u8> {
    let registry = static_rodata_registry()
        .lock()
        .expect("static rodata registry poisoned");
    let entry = registry.get(&key)?;
    if entry.len == len {
        Some(entry.ptr as *const u8)
    } else {
        None
    }
}

/// GNU's supplied string character count, independently of payload decoding.
///
/// Zero is valid even for nonempty bytes. Negative counts never reach an
/// allocated string. Threading: this is immutable numeric metadata with no
/// Lisp state, and may cross mutator threads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GnuDeclaredCharacterCount(usize);

/// A supplied GNU count that cannot be represented by an allocated string.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum GnuDeclaredCharacterCountError {
    #[error("Negative symbol-name character count")]
    Negative,
    #[error("Symbol-name character count is too large")]
    TooLarge,
}

impl TryFrom<i128> for GnuDeclaredCharacterCount {
    type Error = GnuDeclaredCharacterCountError;

    fn try_from(value: i128) -> Result<Self, Self::Error> {
        if value < 0 {
            return Err(GnuDeclaredCharacterCountError::Negative);
        }
        let count = isize::try_from(value).map_err(|_| GnuDeclaredCharacterCountError::TooLarge)?;
        Ok(Self(count as usize))
    }
}

impl From<GnuDeclaredCharacterCount> for usize {
    fn from(value: GnuDeclaredCharacterCount) -> Self {
        value.0
    }
}

static_assertions::assert_impl_all!(GnuDeclaredCharacterCount: Send, Sync);
static_assertions::assert_impl_all!(GnuDeclaredCharacterCountError: Send, Sync);
const _: () =
    assert!(std::mem::size_of::<GnuDeclaredCharacterCount>() == std::mem::size_of::<usize>());

/// GNU's signed string character coordinate (`ptrdiff_t`), distinct from a
/// validated nonnegative buffer/string position. Supplied SCHARS can make a
/// backward byte conversion negative even while all payload bytes are ASCII.
/// Threading: immutable numeric metadata, safe to move between mutator threads.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct GnuStringCharPos(i64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum GnuStringCharPosError {
    #[error("Negative GNU string character coordinate")]
    Negative,
    #[error("String character coordinate is too large")]
    TooLarge,
}

impl GnuStringCharPos {
    pub(crate) const fn new(position: i64) -> Self {
        Self(position)
    }

    pub(crate) const fn get(self) -> i64 {
        self.0
    }
}

impl TryFrom<CharPos0> for GnuStringCharPos {
    type Error = GnuStringCharPosError;

    fn try_from(position: CharPos0) -> Result<Self, Self::Error> {
        i64::try_from(position.get())
            .map(Self)
            .map_err(|_| GnuStringCharPosError::TooLarge)
    }
}

impl TryFrom<GnuStringCharPos> for CharPos0 {
    type Error = GnuStringCharPosError;

    fn try_from(position: GnuStringCharPos) -> Result<Self, Self::Error> {
        if position.get() < 0 {
            return Err(GnuStringCharPosError::Negative);
        }
        usize::try_from(position.get())
            .map(Self::new)
            .map_err(|_| GnuStringCharPosError::TooLarge)
    }
}

static_assertions::assert_impl_all!(GnuStringCharPos: Send, Sync, Copy);
static_assertions::assert_impl_all!(GnuStringCharPosError: Send, Sync);
const _: () = assert!(std::mem::size_of::<GnuStringCharPos>() == std::mem::size_of::<i64>());

struct OwnedStringDataGuard<'a> {
    string: &'a mut LispString,
    data: ManuallyDrop<Vec<u8>>,
}

impl Deref for OwnedStringDataGuard<'_> {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl DerefMut for OwnedStringDataGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl Drop for OwnedStringDataGuard<'_> {
    fn drop(&mut self) {
        self.string.data = self.data.as_ptr();
        self.string.storage_capacity = self.data.capacity();
    }
}

// Read only by the JIT's inline string paths.
#[cfg_attr(not(feature = "jit"), allow(dead_code))]
impl LispString {
    // -- Layout for compiled code -----------------------------------------
    //
    // `#[repr(C)]` pins these. The JIT's inline string `aref`/`aset`
    // (`lowering::emit_inline_string_aref`/`_aset`) read the fields through
    // them and must answer exactly as `aref_fast`/`aset_fast` do: a
    // character is one stored byte when the string is unibyte (`size_byte <
    // 0`) or multibyte with `size_byte == size` (all ASCII, see
    // [`LispString::has_one_byte_characters`]), and a byte may be stored in
    // place only into owned storage (`storage_capacity != 0`) of a normally
    // allocated unibyte string or an all-ASCII multibyte one.

    /// Offset of the encoded character count word: `schars` plus the count
    /// proof bit, which compiled code never has to mask. `length` tags the
    /// word as a fixnum, whose shift drops the bit (asserted below); inline
    /// `aref`/`aset` use the word only behind `size_byte < 0 || size ==
    /// size_byte`, which a supplied count never passes, and a unibyte string
    /// never carries the bit.
    pub(crate) const JIT_SIZE_OFFSET: usize = std::mem::offset_of!(LispString, size);
    /// Offset of `size_byte`: the byte count, or a negative unibyte marker.
    pub(crate) const JIT_SIZE_BYTE_OFFSET: usize = std::mem::offset_of!(LispString, size_byte);
    /// Offset of the byte pointer.
    pub(crate) const JIT_DATA_OFFSET: usize = std::mem::offset_of!(LispString, data);
    /// Offset of the owned-storage capacity (zero for borrowed bytes).
    pub(crate) const JIT_STORAGE_CAPACITY_OFFSET: usize =
        std::mem::offset_of!(LispString, storage_capacity);
    /// `size_byte` of a normally allocated unibyte string (GNU's `-1`).
    pub(crate) const JIT_SIZE_BYTE_UNIBYTE: i64 = SIZE_BYTE_UNIBYTE_NORMAL;
}

impl LispString {
    /// The character count bits of the encoded size word.
    pub(crate) const CHARACTER_COUNT_MASK: usize = !StringCharacterCountFlags::all().bits();

    // -- Constructors --------------------------------------------------------

    fn normalize_size_byte(size_byte: i64, static_rodata: bool) -> i64 {
        if size_byte == SIZE_BYTE_UNIBYTE_RODATA && !static_rodata {
            SIZE_BYTE_UNIBYTE_NORMAL
        } else {
            size_byte
        }
    }

    fn assert_valid_size_byte(size_byte: i64) {
        debug_assert!(
            size_byte >= 0
                || matches!(
                    size_byte,
                    SIZE_BYTE_UNIBYTE_NORMAL
                        | SIZE_BYTE_UNIBYTE_RODATA
                        | SIZE_BYTE_UNIBYTE_IMMOVABLE
                ),
            "invalid GNU Lisp_String size_byte {size_byte}"
        );
    }

    /// Only a multibyte string carries a supplied count: compiled `aref`/`aset`
    /// compare a unibyte string's size word with an index directly.
    fn assert_unibyte_count_has_no_proof(size: usize, size_byte: i64) {
        debug_assert!(
            size_byte >= 0 || size & !Self::CHARACTER_COUNT_MASK == 0,
            "unibyte string size word carries count proof metadata"
        );
    }

    fn payload_len_for(size: usize, size_byte: i64) -> usize {
        if size_byte < 0 {
            size & Self::CHARACTER_COUNT_MASK
        } else {
            size_byte as usize
        }
    }

    /// Copy `bytes` into a payload Vec with one spare slot so
    /// `from_owned_payload`'s trailing-NUL push never reallocates (an
    /// exact-capacity Vec would double and re-copy the whole payload there).
    fn copy_payload(bytes: &[u8]) -> Vec<u8> {
        let mut payload = Vec::with_capacity(bytes.len() + 1);
        payload.extend_from_slice(bytes);
        payload
    }

    /// Append the NUL every owned payload carries. A producer that sized its
    /// buffer exactly (most do: `to_vec`, `with_capacity(len)`, an encoder's
    /// output) leaves no room for it, and a plain `push` would then double
    /// the capacity -- a fresh allocation and a copy of the whole payload.
    /// Growing by exactly one byte usually stays inside the allocation's
    /// size class, where the allocator extends it in place.
    #[inline]
    fn push_terminator(payload: &mut Vec<u8>) {
        if payload.len() == payload.capacity() {
            payload.reserve_exact(1);
        }
        payload.push(0);
    }

    fn from_owned_payload(mut payload: Vec<u8>, size: usize, size_byte: i64) -> Self {
        let size_byte = Self::normalize_size_byte(size_byte, false);
        Self::assert_valid_size_byte(size_byte);
        Self::assert_unibyte_count_has_no_proof(size, size_byte);
        debug_assert_eq!(
            payload.len(),
            Self::payload_len_for(size, size_byte),
            "LispString storage length must match GNU size/size_byte fields"
        );
        Self::push_terminator(&mut payload);
        let data = payload.as_ptr();
        let storage_capacity = payload.capacity();
        std::mem::forget(payload);
        Self {
            size,
            size_byte,
            intervals: AtomicPtr::new(std::ptr::null_mut()),
            data,
            storage_capacity,
        }
    }

    unsafe fn from_borrowed_bytes(
        ptr: *const u8,
        len: usize,
        size: usize,
        size_byte: i64,
        static_rodata: bool,
    ) -> Self {
        let size_byte = Self::normalize_size_byte(size_byte, static_rodata);
        Self::assert_valid_size_byte(size_byte);
        Self::assert_unibyte_count_has_no_proof(size, size_byte);
        debug_assert_eq!(len, Self::payload_len_for(size, size_byte));
        debug_assert!(!ptr.is_null());
        debug_assert_eq!(unsafe { *ptr.add(len) }, 0);
        Self {
            size,
            size_byte,
            intervals: AtomicPtr::new(std::ptr::null_mut()),
            data: ptr,
            storage_capacity: 0,
        }
    }

    fn release_owned_storage(&mut self) {
        if self.storage_capacity == 0 {
            return;
        }
        let len = self.sbytes() + 1;
        let capacity = self.storage_capacity;
        debug_assert!(capacity >= len);
        drop(unsafe { Vec::from_raw_parts(self.data as *mut u8, len, capacity) });
        self.data = std::ptr::null();
        self.storage_capacity = 0;
    }

    fn replace_owned_payload(&mut self, mut payload: Vec<u8>) {
        // Finish allocating the replacement before detaching the current payload.
        Self::push_terminator(&mut payload);
        let old_data = self.data;
        let old_len = self.sbytes() + 1;
        let old_capacity = self.storage_capacity;
        self.data = payload.as_ptr();
        self.storage_capacity = payload.capacity();
        std::mem::forget(payload);
        if old_capacity != 0 {
            // SAFETY: the old payload is a separate owned Vec allocation whose
            // original length and capacity were saved before replacement. The
            // exclusive string borrow prevents another payload owner.
            drop(unsafe { Vec::from_raw_parts(old_data as *mut u8, old_len, old_capacity) });
        }
    }

    fn ensure_owned(&mut self) {
        if self.storage_capacity != 0 {
            return;
        }
        let payload = Self::copy_payload(self.as_bytes());
        self.replace_owned_payload(payload);
    }

    fn owned_data_guard(&mut self) -> OwnedStringDataGuard<'_> {
        self.ensure_owned();
        let len = self.sbytes() + 1;
        let capacity = self.storage_capacity;
        let data = unsafe { Vec::from_raw_parts(self.data as *mut u8, len, capacity) };
        OwnedStringDataGuard {
            string: self,
            data: ManuallyDrop::new(data),
        }
    }

    /// SATB write barrier for the interval table, ENFORCED IN CODE at the only
    /// two mutation choke points (`ensure_intervals` handing out `&mut`,
    /// `clear_intervals` freeing the table) so no call site — wrapper or raw —
    /// can drop a published string's interval children unlogged while the
    /// concurrent GC thread may already have claimed this string as
    /// interval-free (never to be re-traced this cycle). Logs the CURRENT
    /// (pre-mutation) child values to the shared SATB buffer, deduped once per
    /// string per cycle: the first pre-image is a superset of the
    /// start-of-cycle children, and later mutations only unlink already-logged
    /// or born-black values (same argument as
    /// `push_value_children_to_satb_shared`). For UNPUBLISHED strings (fresh
    /// locals under construction) this logs values that are live via the
    /// mutator anyway — harmless floating garbage at worst. No-op (one
    /// thread-local load) unless a concurrent mark is active.
    ///
    /// The `mutate.rs` wrappers (`with_string_text_props_mut` /
    /// `with_lisp_string_mut`) remain the required route for PUBLISHED strings:
    /// they additionally maintain the dump remembered set and dirty-owner
    /// tracking for the owner value, which this value-only barrier cannot.
    fn note_interval_preimage_for_satb(&self) {
        if !crate::tagged::gc::concurrent_mark_active() {
            return;
        }
        let ptr = self.intervals.load(Ordering::Acquire);
        if ptr.is_null() {
            return;
        }
        // Safety: we are on the mutator thread (the only thread that mutates
        // strings), called from a `&mut self` context; the table is live
        // because only `clear_intervals`/`Drop` (both `&mut self`) free it.
        let table = unsafe { &*ptr };
        crate::tagged::gc::note_string_interval_preimage(self as *const Self as usize, table);
    }

    fn ensure_intervals(&mut self) -> &mut TextPropertyTable {
        // Barrier BEFORE the mutation the returned `&mut` enables.
        self.note_interval_preimage_for_satb();
        let mut ptr = self.intervals.load(Ordering::Acquire);
        if ptr.is_null() {
            ptr = Box::into_raw(Box::new(TextPropertyTable::new()));
            // Release-publish the fully-constructed (empty) table: the
            // concurrent GC thread's raw word load must never observe the
            // pointer before the table's memory is initialized.
            self.intervals.store(ptr, Ordering::Release);
        }
        // Safety: `ptr` is this string's live, uniquely-owned table (`&mut
        // self` excludes other mutator references; the GC thread never
        // dereferences it — see `intervals_ptr`).
        unsafe { &mut *ptr }
    }

    /// Backward-compat shim: create from a Rust `String` + multibyte flag.
    /// For multibyte, the bytes are already valid UTF-8 (standard Unicode ==
    /// Emacs encoding for Unicode codepoints).  For unibyte, each byte is one
    /// character.
    pub fn new(text: String, multibyte: bool) -> Self {
        if multibyte {
            Self::from_utf8(&text)
        } else {
            Self::from_unibyte(text.into_bytes())
        }
    }

    /// Create a multibyte string from raw Emacs-internal-encoding bytes.
    /// The caller must ensure the bytes are valid Emacs encoding.
    pub fn from_emacs_bytes(data: Vec<u8>) -> Self {
        let size = emacs_char::chars_in_multibyte(&data);
        let size_byte = data.len() as i64;
        Self::from_owned_payload(data, size, size_byte)
    }

    /// Preserve GNU `make_multibyte_string`'s supplied character count.
    ///
    /// GNU shorthand replacement calculates counts arithmetically, so the
    /// declared count may differ from a scan of the raw concatenated bytes
    /// (`lread.c:5004`, `alloc.c:2241-2247`). This constructor does not assert
    /// a decode-count invariant. Accessors must bound all payload reads.
    /// As GNU `alloc.c:2332` does, zero bytes select the empty multibyte shape.
    pub(crate) fn from_emacs_bytes_with_declared_chars(
        data: Vec<u8>,
        chars: GnuDeclaredCharacterCount,
    ) -> Self {
        let size = if data.is_empty() { 0 } else { chars.0 };
        let size_byte = data.len() as i64;
        let size = Self::encode_character_count(size, StringCharacterCountKind::Supplied);
        Self::from_owned_payload(data, size, size_byte)
    }

    /// [`Self::from_emacs_bytes`] for a caller that already knows the
    /// character count, so the bytes are not walked a second time to
    /// recover it.
    ///
    /// GNU builds a buffer substring this way: `make_uninit_multibyte_string
    /// (end - start, end_byte - start_byte)` (`editfns.c:1608`) takes both
    /// lengths as arithmetic on positions it already holds, then does one
    /// memcpy.  `size` must be the true character count of `data`; the
    /// `debug_assert` below is the check, because `from_owned_payload` only
    /// validates the BYTE length.
    pub fn from_emacs_bytes_with_chars(data: Vec<u8>, size: usize) -> Self {
        debug_assert_eq!(
            size,
            emacs_char::chars_in_multibyte(&data),
            "caller-supplied character count must match the bytes"
        );
        let size_byte = data.len() as i64;
        Self::from_owned_payload(data, size, size_byte)
    }

    /// Reconstruct a `LispString` from pdump data with pre-computed fields.
    /// The caller is responsible for passing consistent `data`, `size`, and
    /// `size_byte` values (as stored in the dump file).
    pub fn from_dump(data: Vec<u8>, size: usize, size_byte: i64) -> Self {
        let size = Self::character_count_word_for_image(&data, size, size_byte);
        Self::from_owned_payload(data, size, size_byte)
    }

    /// Build a Lisp string whose bytes live in a mapped pdump image.
    ///
    /// # Safety
    /// `ptr..ptr+len+1` must remain mapped and immutable for the lifetime of
    /// the returned `LispString`, with `ptr[len] == 0`. Mutation first copies
    /// these bytes into owned storage.
    pub(crate) unsafe fn from_mapped_bytes(
        ptr: *const u8,
        len: usize,
        size: usize,
        size_byte: i64,
    ) -> Self {
        let mut string = unsafe { Self::from_borrowed_bytes(ptr, len, size, size_byte, false) };
        string.size = Self::character_count_word_for_image(string.as_bytes(), size, size_byte);
        string
    }

    /// `from_unibyte` from a borrowed slice, copying with the spare NUL slot
    /// up front so the constructor never reallocates the fresh copy.
    pub(crate) fn from_unibyte_slice(bytes: &[u8]) -> Self {
        Self::from_unibyte(Self::copy_payload(bytes))
    }

    /// Create a unibyte string.  Each byte is one character; `size_byte` = -1.
    pub fn from_unibyte(data: Vec<u8>) -> Self {
        let size = data.len();
        Self::from_owned_payload(data, size, SIZE_BYTE_UNIBYTE_NORMAL)
    }

    /// Create a unibyte string whose bytes live in static read-only storage.
    ///
    /// This mirrors GNU's `size_byte == -2` state for C string constants.  If
    /// later mutated, Neomacs copies the data and demotes it to ordinary
    /// unibyte storage because it no longer points at rodata.
    pub fn from_rodata_unibyte(data_with_nul: &'static [u8]) -> Self {
        assert!(
            data_with_nul.last().is_some_and(|byte| *byte == 0),
            "GNU rodata strings must include the trailing NUL"
        );
        let size = data_with_nul.len() - 1;
        register_static_rodata(data_with_nul);
        unsafe {
            Self::from_borrowed_bytes(
                data_with_nul.as_ptr(),
                size,
                size,
                SIZE_BYTE_UNIBYTE_RODATA,
                true,
            )
        }
    }

    pub(crate) fn from_registered_rodata_unibyte(
        key: u64,
        len: usize,
        size: usize,
    ) -> Option<Self> {
        if size != len {
            return None;
        }
        let ptr = lookup_static_rodata(key, len)?;
        Some(unsafe { Self::from_borrowed_bytes(ptr, len, size, SIZE_BYTE_UNIBYTE_RODATA, true) })
    }

    /// Install runtime ownership metadata for a raw string object loaded from
    /// a mapped pdump image.  The GNU-visible fields (`size`, `size_byte`,
    /// `data`) are expected to already have come from the mapped object image.
    ///
    /// # Safety
    /// `ptr..ptr+len+1` must remain mapped and immutable for the lifetime of
    /// this string, with `ptr[len] == 0`.
    #[cfg_attr(not(debug_assertions), allow(dead_code))] // debug-only verification caller (pdump convert.rs)
    pub(crate) unsafe fn install_mapped_storage_sidecar(
        &mut self,
        ptr: *const u8,
        len: usize,
    ) -> Result<(), String> {
        self.validate_storage_install(ptr, len)?;
        self.data = ptr;
        self.storage_capacity = 0;
        Ok(())
    }

    /// Install runtime ownership metadata for a raw rodata string object
    /// loaded from a mapped pdump image.
    pub(crate) fn install_registered_rodata_sidecar(
        &mut self,
        key: u64,
        len: usize,
    ) -> Result<(), String> {
        if self.size_byte != SIZE_BYTE_UNIBYTE_RODATA {
            return Err(format!(
                "static rodata string has non-rodata size_byte {}",
                self.size_byte
            ));
        }
        let ptr = lookup_static_rodata(key, len).ok_or_else(|| {
            format!("static rodata string key {key:#x} length {len} is not registered")
        })?;
        self.validate_storage_install(ptr, len)?;
        self.data = ptr;
        self.storage_capacity = 0;
        Ok(())
    }

    fn validate_storage_install(&self, ptr: *const u8, len: usize) -> Result<(), String> {
        if ptr.is_null() {
            return Err("LispString storage pointer is null".into());
        }
        if len != self.sbytes() {
            return Err(format!(
                "LispString storage length {len} does not match SBYTES {}",
                self.sbytes()
            ));
        }
        if !self.data.is_null() && self.data != ptr {
            return Err(format!(
                "LispString data pointer {:p} does not match sidecar pointer {:p}",
                self.data, ptr
            ));
        }
        let trailing_nul = unsafe { *ptr.add(len) };
        if trailing_nul != 0 {
            return Err("GNU Lisp_String data is not NUL-terminated after SBYTES".into());
        }
        Ok(())
    }

    pub(crate) const fn data_field_offset() -> usize {
        std::mem::offset_of!(LispString, data)
    }

    /// Rebuild a string from its raw internal bytes and character count.
    ///
    /// `schars == bytes.len()` means every byte is one character, which is
    /// the unibyte shape; anything else is Emacs's multibyte encoding.
    ///
    /// The multibyte FLAG is deliberately not recoverable here, and must not
    /// be: `equal` compares SCHARS, SBYTES and bytes (GNU `Fequal`), so an
    /// all-ASCII unibyte string and its multibyte twin are `equal` and are
    /// therefore the same hash key. Distinguishing them would be wrong.
    pub(crate) fn from_bytes_with_schars(bytes: &[u8], schars: usize) -> Self {
        if schars == bytes.len() {
            Self::from_unibyte_slice(bytes)
        } else {
            let data = Self::copy_payload(bytes);
            let size_byte = bytes.len() as i64;
            Self::from_dump(data, schars, size_byte)
        }
    }

    /// Create a multibyte string from valid UTF-8.
    /// Standard Unicode == Emacs encoding, so just copy the bytes.
    pub fn from_utf8(s: &str) -> Self {
        let data = Self::copy_payload(s.as_bytes());
        let size = s.chars().count();
        let size_byte = data.len() as i64;
        Self::from_owned_payload(data, size, size_byte)
    }

    // -- Accessors -----------------------------------------------------------

    /// Raw byte access.
    pub fn as_bytes(&self) -> &[u8] {
        let len = self.sbytes();
        if len == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(self.data, len) }
        }
    }

    /// Whether this string header has been RECLAIMED by the collector.
    ///
    /// GNU's own free-list test, in as many words: `sweep_strings` nulls
    /// `s->u.s.data` on a dead string "so that we know it's free"
    /// (src/alloc.c:1878-1882) and branches on `if (s->u.s.data)` at :1851.
    /// Every live constructor here writes a non-null payload pointer
    /// (`from_owned_payload` pushes the trailing NUL before taking the
    /// pointer; `from_borrowed_bytes` and both sidecar installers reject
    /// null), so a null `data` means exactly one thing.
    #[inline]
    pub(crate) fn is_reclaimed(&self) -> bool {
        self.data.is_null()
    }

    /// Try to view the data as a UTF-8 `&str`.
    /// Returns `None` if the bytes contain non-UTF-8 sequences (e.g. overlong
    /// C0/C1 raw-byte encodings from `.elc` files).
    ///
    /// Prefer `as_bytes()` for byte-level equality: two different non-UTF-8
    /// strings both return `None`, so `as_utf8_str() == as_utf8_str()` would
    /// silently treat them as equal.
    pub fn as_utf8_str(&self) -> Option<&str> {
        std::str::from_utf8(self.as_bytes()).ok()
    }

    #[inline]
    fn encode_character_count(count: usize, kind: StringCharacterCountKind) -> usize {
        debug_assert!(count <= Self::CHARACTER_COUNT_MASK);
        count
            | match kind {
                StringCharacterCountKind::Computed => StringCharacterCountFlags::empty().bits(),
                StringCharacterCountKind::Supplied => StringCharacterCountFlags::SUPPLIED.bits(),
            }
    }

    /// Regenerate count proof at a cold image/reconstruction boundary. Numeric
    /// dump sizes remain SCHARS; mapped images bake the encoded private word.
    /// The scan happens while building an image, never during regex lookup.
    pub(crate) fn character_count_word_for_image(
        payload: &[u8],
        count: usize,
        size_byte: i64,
    ) -> usize {
        let kind = if size_byte < 0 || Self::is_decoded_multibyte_count(payload, count) {
            StringCharacterCountKind::Computed
        } else {
            StringCharacterCountKind::Supplied
        };
        Self::encode_character_count(count, kind)
    }

    /// Whether COUNT is the decoded character count of the multibyte PAYLOAD.
    /// A count equal to the byte length is decoded only for ASCII bytes:
    /// every other character takes two or more bytes, and equal counts are
    /// what [`Self::has_one_byte_characters`] and compiled code test.
    fn is_decoded_multibyte_count(payload: &[u8], count: usize) -> bool {
        if count == payload.len() {
            payload.is_ascii()
        } else {
            emacs_char::chars_in_multibyte(payload) == count
        }
    }

    #[inline]
    pub(crate) fn character_count_kind(&self) -> StringCharacterCountKind {
        let flags = StringCharacterCountFlags::from_bits_truncate(self.size);
        if flags.contains(StringCharacterCountFlags::SUPPLIED) {
            StringCharacterCountKind::Supplied
        } else {
            StringCharacterCountKind::Computed
        }
    }

    /// Whether each character is one stored byte, so a character index is a
    /// byte index and the byte is the character: unibyte storage, or a
    /// multibyte string whose computed count equals its byte count (all
    /// ASCII). This compares the encoded size word, so a supplied count never
    /// qualifies, even one equal to the byte count. Compiled `aref`/`aset`
    /// test the same raw words.
    #[inline]
    pub(crate) fn has_one_byte_characters(&self) -> bool {
        self.size_byte < 0 || self.size == self.size_byte as usize
    }

    /// Character count, excluding immutable proof metadata.
    #[inline]
    pub fn schars(&self) -> usize {
        self.size & Self::CHARACTER_COUNT_MASK
    }

    /// Byte count.  For multibyte strings this is `data.len()`; for unibyte
    /// strings this is also `data.len()` (= size, since each byte is one char).
    pub fn sbytes(&self) -> usize {
        if self.size_byte < 0 {
            // Unibyte constructors keep the size word free of count-proof metadata.
            self.size
        } else {
            self.size_byte as usize
        }
    }

    /// Whether this is a multibyte string (`size_byte >= 0`).
    pub fn is_multibyte(&self) -> bool {
        self.size_byte >= 0
    }

    pub(crate) fn storage_kind(&self) -> LispStringStorageKind {
        if self.is_multibyte() {
            LispStringStorageKind::Multibyte
        } else {
            LispStringStorageKind::Unibyte
        }
    }

    /// Raw GNU `Lisp_String.u.s.size_byte` value.
    pub fn size_byte(&self) -> i64 {
        self.size_byte
    }

    pub(crate) fn rodata_key(&self) -> Option<u64> {
        if !self.is_rodata() {
            return None;
        }
        let bytes_with_nul = unsafe { std::slice::from_raw_parts(self.data, self.sbytes() + 1) };
        Some(static_rodata_key(bytes_with_nul))
    }

    /// True for GNU's `size_byte == -2`: unibyte bytes in read-only storage.
    pub fn is_rodata(&self) -> bool {
        self.size_byte == SIZE_BYTE_UNIBYTE_RODATA
    }

    /// True for GNU's `size_byte == -3`: unibyte bytes that must not move.
    pub fn is_immovable(&self) -> bool {
        self.size_byte == SIZE_BYTE_UNIBYTE_IMMOVABLE
    }

    /// Mirror GNU `pin_string`: mark a unibyte string as immovable bytecode
    /// storage.  Multibyte strings cannot be pinned this way.
    pub fn pin_immovable(&mut self) {
        debug_assert!(
            !self.is_multibyte(),
            "GNU pin_string only accepts unibyte strings"
        );
        self.ensure_owned();
        self.size = self.sbytes();
        self.size_byte = SIZE_BYTE_UNIBYTE_IMMOVABLE;
    }

    /// Raw interval-table pointer word (null = no table). The ONLY string
    /// accessor the concurrent GC thread may use: reading the word is safe
    /// from any thread, but DEREFERENCING the result is mutator-side only
    /// (see `intervals`). Acquire pairs with `ensure_intervals`' Release
    /// publish — the null check alone would be sound Relaxed, but Acquire is
    /// the safe default (free on x86) and covers any reader that goes on to
    /// dereference.
    #[inline]
    pub(crate) fn intervals_ptr(&self) -> *mut TextPropertyTable {
        self.intervals.load(Ordering::Acquire)
    }

    /// Text-property interval tree attached to this string, like GNU's
    /// `Lisp_String.u.s.intervals`.
    /// An interval-free string returns a typed empty view without table
    /// storage; an attached table stays borrowed from this string.
    ///
    /// MUTATOR-SIDE ONLY: dereferences the interval table, which the mutator
    /// can free at any time via `clear_intervals` — the concurrent GC thread
    /// must never call this (it reads only the pointer word via
    /// `intervals_ptr`; a dereference here on the GC thread is a
    /// use-after-free).
    pub fn intervals(&self) -> TextPropertiesRef<'_> {
        let ptr = self.intervals.load(Ordering::Acquire);
        if ptr.is_null() {
            EmptyTextProperties::new().view()
        } else {
            // SAFETY: non-null means a live Box-allocated table; it is freed
            // only by `clear_intervals`/`Drop` (both `&mut self`), which the
            // caller's `&self` excludes on the mutator thread. The view keeps
            // that same borrow and cannot leave this thread.
            TextPropertiesRef::from_table(unsafe { &*ptr })
        }
    }

    pub fn has_intervals(&self) -> bool {
        !self.intervals.load(Ordering::Acquire).is_null()
    }

    pub fn clear_intervals(&mut self) {
        // SATB (enforced): log the children being dropped BEFORE unlinking —
        // a concurrently-claimed interval-free string is never re-traced this
        // cycle, so this log is the only thing keeping the dropped children
        // of a mid-mark clear alive.
        self.note_interval_preimage_for_satb();
        let ptr = self.intervals.swap(std::ptr::null_mut(), Ordering::AcqRel);
        if !ptr.is_null() {
            // Safety: the swap took the unique owning pointer. The concurrent
            // GC thread may still read the STALE non-null word this cycle but
            // never dereferences it (a spurious defer at worst).
            drop(unsafe { Box::from_raw(ptr) });
        }
    }

    /// Erase owned bytes before they can be released or reused.
    ///
    /// Mutator-only: the exclusive borrow covers the complete live allocation.
    /// Unlike an ordinary memset followed by free, these writes cannot be
    /// eliminated as dead stores. Borrowed dump and static storage is untouched.
    fn erase_owned_payload(&mut self) {
        if self.storage_capacity == 0 {
            return;
        }
        let len = self.sbytes() + 1;
        // SAFETY: owned storage contains `len` initialized bytes, including the
        // trailing NUL, and `&mut self` excludes other mutator access.
        let bytes = unsafe { std::slice::from_raw_parts_mut(self.data as *mut u8, len) };
        #[cfg(all(target_os = "linux", any(target_env = "gnu", target_env = "musl")))]
        {
            // SAFETY: the entire slice is writable; explicit_bzero preserves
            // the erasure even when the allocation is immediately freed.
            unsafe { libc::explicit_bzero(bytes.as_mut_ptr().cast(), bytes.len()) };
        }
        #[cfg(target_os = "macos")]
        {
            // SAFETY: both lengths describe the writable slice exactly. Handle
            // a library rejection without leaving any sensitive bytes behind.
            if unsafe { libc::memset_s(bytes.as_mut_ptr().cast(), bytes.len(), 0, bytes.len()) }
                != 0
            {
                for byte in bytes {
                    // SAFETY: `byte` exclusively references a live payload byte.
                    unsafe { std::ptr::write_volatile(byte, 0) };
                }
            }
        }
        #[cfg(not(any(
            all(target_os = "linux", any(target_env = "gnu", target_env = "musl")),
            target_os = "macos"
        )))]
        for byte in bytes {
            // SAFETY: `byte` exclusively references a live payload byte.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
        std::sync::atomic::compiler_fence(Ordering::SeqCst);
    }

    /// Clear the bytes and properties, then expose every byte as a character.
    ///
    /// GNU `Fclear_string` erases the old contents. Small owned payloads stay
    /// in place; larger ordinary payloads are erased before replacement so
    /// subsequent copies can reuse their materialized storage before GC.
    /// Pinned bytecode storage stays in place for this mutation; GNU's unibyte
    /// conversion then demotes its marker. Borrowed dump or static bytes receive
    /// a separate zero payload.
    /// Published owners must use the `with_lisp_string_mut` mutation wrapper.
    pub(crate) fn clear_contents_to_unibyte(&mut self) {
        let len = self.sbytes();
        self.clear_intervals();
        if len != 0 || self.is_multibyte() {
            if self.storage_capacity != 0
                && (len <= CLEAR_STRING_RETAIN_BYTES || self.is_immovable())
            {
                self.erase_owned_payload();
            } else {
                // Every valid string already carries a NUL at `len`. Reserve
                // that byte with the zero allocation so replacement does not
                // grow and copy the payload merely to append its terminator.
                let mut replacement = vec![0; len + 1];
                replacement.truncate(len);
                self.erase_owned_payload();
                self.replace_owned_payload(replacement);
            }
            self.size = len;
            self.size_byte = SIZE_BYTE_UNIBYTE_NORMAL;
        }
        crate::emacs_core::string_pos_cache::note_string_bytes_changed();
    }

    /// Mutable text-property interval tree attached to this string.
    ///
    /// SATB enforcement lives inside `ensure_intervals` (the pre-mutation
    /// child values are logged), so no caller can drop interval children
    /// unlogged. PUBLISHED strings must still be mutated via the `mutate.rs`
    /// wrappers (`with_string_text_props_mut` / `with_lisp_string_mut`), which
    /// also maintain the dump remembered set + dirty-owner tracking for the
    /// OWNER value; direct calls are only appropriate on strings not yet
    /// reachable from the Lisp graph (fresh locals under construction).
    pub fn intervals_mut(&mut self) -> &mut TextPropertyTable {
        self.ensure_intervals()
    }

    /// Backward-compat accessor matching the old `pub multibyte` field.
    pub fn multibyte(&self) -> bool {
        self.is_multibyte()
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.sbytes()
    }

    /// Return the byte offset for a character position.
    ///
    /// Mirrors GNU `string_char_to_byte`: when `SCHARS == SBYTES`, every
    /// character occupies one byte even if the string is marked multibyte, so
    /// the conversion is an O(1) identity operation.
    pub(crate) fn char_to_byte_pos(&self, char_pos: usize) -> usize {
        let schars = self.schars();
        let char_pos = char_pos.min(schars);
        if !self.is_multibyte() || schars == self.sbytes() {
            char_pos
        } else if char_pos > 0 && char_pos >= schars - char_pos {
            // GNU chooses the backward route on equal distances too
            // (fns.c:1220-1241). Keep zero at the initialized payload start.
            emacs_char::char_to_byte_pos_from_end(self.as_bytes(), schars - char_pos)
        } else {
            emacs_char::char_to_byte_pos(self.as_bytes(), char_pos)
        }
    }

    /// GNU's signed character coordinate for a physical byte offset.
    ///
    /// Like `string_byte_to_char` (fns.c:1260-1294), the nearest end and
    /// backward ties use supplied SCHARS rather than a payload recount.
    /// Negative results are valid GNU positions, not unsigned coordinates.
    /// String metadata must be representable by GNU ptrdiff_t: normal owned
    /// sizes are bounded by Vec's isize::MAX and supplied counts validate that
    /// bound. Existing dump constructors require valid GNU metadata too.
    /// Physical scans stay inside the initialized byte slice.
    pub(crate) fn byte_to_gnu_char_pos(&self, byte_pos: usize) -> GnuStringCharPos {
        let sbytes = self.sbytes();
        let byte_pos = byte_pos.min(sbytes);
        let char_pos = if !self.is_multibyte() || self.schars() == sbytes {
            byte_pos as i64
        } else if byte_pos < sbytes - byte_pos {
            emacs_char::byte_to_char_pos(self.as_bytes(), byte_pos) as i64
        } else {
            self.schars() as i64
                - emacs_char::byte_to_char_pos(&self.as_bytes()[byte_pos..], sbytes - byte_pos)
                    as i64
        };
        GnuStringCharPos::new(char_pos)
    }

    /// Heap bytes reserved by an owned byte backing, including capacity for
    /// GNU's trailing NUL. Mapped/static strings reserve no allocator-backed
    /// payload bytes in this process; their bytes belong to the pdump mapping
    /// or executable image instead.
    pub(crate) fn owned_capacity(&self) -> usize {
        self.storage_capacity
    }

    pub(crate) fn has_owned_storage(&self) -> bool {
        self.storage_capacity != 0
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sbytes() == 0
    }

    pub(crate) fn is_ascii(&self) -> bool {
        self.as_bytes().is_ascii()
    }

    /// Mutate the logical string bytes and restore GNU string invariants before
    /// returning: trailing NUL after `SBYTES`, direct data pointer, and cached
    /// character/byte sizes.
    /// Overwrite the byte at `byte_pos`, for a replacement that cannot change
    /// the character count: any byte of a unibyte string, or an ASCII byte
    /// over an ASCII byte of a multibyte one -- the only replacements `aset`
    /// makes. [`Self::mutate_bytes`] would pop and re-push the trailing NUL
    /// and recount a multibyte string's characters (a scan of the whole
    /// string per `aset`).
    pub fn set_byte_same_char_count(&mut self, byte_pos: usize, byte: u8) {
        debug_assert!(byte_pos < self.sbytes(), "byte position in range");
        debug_assert!(
            !self.is_multibyte() || (byte < 0x80 && self.as_bytes()[byte_pos] < 0x80),
            "a multibyte string only takes ASCII over ASCII in place"
        );
        if self.is_rodata() {
            self.size_byte = SIZE_BYTE_UNIBYTE_NORMAL;
        }
        self.ensure_owned();
        // SAFETY: owned storage holds `sbytes() + 1` initialized bytes.
        unsafe { *(self.data as *mut u8).add(byte_pos) = byte };
    }

    pub fn mutate_bytes<R>(&mut self, f: impl FnOnce(&mut Vec<u8>) -> R) -> R {
        if self.is_rodata() {
            self.size_byte = SIZE_BYTE_UNIBYTE_NORMAL;
        }
        let (result, byte_len) = {
            let mut data = self.owned_data_guard();
            debug_assert_eq!(
                data.last().copied(),
                Some(0),
                "owned LispString storage must include trailing NUL"
            );
            data.pop();
            let result = f(&mut data);
            let byte_len = data.len();
            data.push(0);
            (result, byte_len)
        };
        self.recompute_size(byte_len);
        result
    }

    /// Recompute cached `size` (and `size_byte`) from the current data.
    ///
    /// General byte mutations end here and invalidate cached positions.
    /// `clear_contents_to_unibyte` restores its known shape and invalidates
    /// the cache directly, without scanning the zeroed bytes.
    fn recompute_size(&mut self, byte_len: usize) {
        crate::emacs_core::string_pos_cache::note_string_bytes_changed();
        if self.size_byte >= 0 {
            // multibyte
            let data = if byte_len == 0 {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(self.data, byte_len) }
            };
            let size = emacs_char::chars_in_multibyte(data);
            self.size = size;
            self.size_byte = byte_len as i64;
        } else {
            // unibyte
            self.size = byte_len;
        }
    }

    fn slice_bytes_no_properties_with_char_len(
        &self,
        start: usize,
        end: usize,
        char_len: Option<usize>,
    ) -> Option<Self> {
        if end > self.as_bytes().len() || start > end {
            return None;
        }
        let slice = &self.as_bytes()[start..end];
        Some(if self.size_byte >= 0 {
            // multibyte
            if let Some(char_len) = char_len {
                let count = Self::encode_character_count(char_len, self.character_count_kind());
                Self::from_owned_payload(Self::copy_payload(slice), count, slice.len() as i64)
            } else {
                Self::from_emacs_bytes(Self::copy_payload(slice))
            }
        } else {
            Self::from_unibyte(Self::copy_payload(slice))
        })
    }

    fn slice_bytes_no_properties(&self, start: usize, end: usize) -> Option<Self> {
        self.slice_bytes_no_properties_with_char_len(start, end, None)
    }

    /// Byte-index slice without text properties, matching GNU
    /// `substring-no-properties`.
    pub fn slice_no_properties(&self, start: usize, end: usize) -> Option<Self> {
        self.slice_bytes_no_properties(start, end)
    }

    /// The same text with every text property dropped.
    ///
    /// Callers use this where GNU produces a string by printing into a buffer
    /// and reading it back, a route that never carries properties across.
    pub fn without_properties(&self) -> Self {
        // GNU copies the declaration; rescanning can discard shorthand counts.
        Self::from_owned_payload(
            Self::copy_payload(self.as_bytes()),
            self.size,
            self.size_byte,
        )
    }

    /// Byte-index slice without text properties when the caller already knows
    /// the corresponding character bounds.
    pub(crate) fn slice_no_properties_with_char_bounds(
        &self,
        start: usize,
        end: usize,
        char_start: usize,
        char_end: usize,
    ) -> Option<Self> {
        if char_start > char_end || char_end > self.schars() {
            return None;
        }
        self.slice_bytes_no_properties_with_char_len(start, end, Some(char_end - char_start))
    }

    /// Byte-index slice that preserves text properties, matching GNU
    /// `substring`/`substring_both`.
    pub fn slice(&self, start: usize, end: usize) -> Option<Self> {
        let (char_start, char_end) = if self.is_multibyte() {
            (
                CharPos0::try_from(self.byte_to_gnu_char_pos(start))
                    .ok()?
                    .get(),
                CharPos0::try_from(self.byte_to_gnu_char_pos(end))
                    .ok()?
                    .get(),
            )
        } else {
            (start, end)
        };
        self.slice_with_char_bounds(start, end, char_start, char_end)
    }

    /// Byte-index slice that preserves text properties when the caller already
    /// knows the corresponding character bounds, matching GNU
    /// `substring_both`.
    pub(crate) fn slice_with_char_bounds(
        &self,
        start: usize,
        end: usize,
        char_start: usize,
        char_end: usize,
    ) -> Option<Self> {
        let mut result =
            self.slice_no_properties_with_char_bounds(start, end, char_start, char_end)?;
        let intervals = self.intervals().slice_char_range(CharRange::new(
            CharPos0::new(char_start),
            CharPos0::new(char_end),
        ));
        if !intervals.is_empty() {
            *result.intervals_mut() = intervals;
        }
        Some(result)
    }

    pub fn concat(&self, other: &Self) -> Self {
        let multibyte = self.is_multibyte() || other.is_multibyte();
        // Issue #131: when unifying a unibyte piece into a multibyte result,
        // promote its raw bytes to the Emacs multibyte encoding (high bytes ->
        // eight-bit chars) like GNU `concat`, rather than splicing raw bytes
        // into a multibyte string and producing a malformed sequence.
        let mut data = if multibyte && !self.is_multibyte() {
            crate::emacs_core::emacs_char::str_to_multibyte(self.as_bytes())
        } else {
            self.as_bytes().to_vec()
        };
        if multibyte && !other.is_multibyte() {
            data.extend_from_slice(&crate::emacs_core::emacs_char::str_to_multibyte(
                other.as_bytes(),
            ));
        } else {
            data.extend_from_slice(other.as_bytes());
        }
        let mut result = if multibyte {
            Self::from_emacs_bytes(data)
        } else {
            Self::from_unibyte(data)
        };

        let mut intervals = self.intervals().to_owned_table();
        let empty;
        let other_intervals = match other.intervals().as_table() {
            Some(table) => table,
            None => {
                // Appending an empty source still advances the destination's
                // revision ticks. This local table has no heap allocation.
                empty = TextPropertyTable::new();
                &empty
            }
        };
        intervals.append_shifted_at_char_offset(other_intervals, CharLen::new(self.schars()));
        if !intervals.is_empty() {
            *result.intervals_mut() = intervals;
        }
        result
    }

    /// Replace the entire contents with a UTF-8 string, preserving the
    /// multibyte/unibyte flag.
    pub fn set_from_str(&mut self, s: &str) {
        let was_rodata = self.is_rodata();
        self.replace_owned_payload(Self::copy_payload(s.as_bytes()));
        if was_rodata {
            self.size_byte = SIZE_BYTE_UNIBYTE_NORMAL;
        }
        self.recompute_size(s.len());
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn has_trailing_nul(&self) -> bool {
        !self.data.is_null() && unsafe { *self.data.add(self.sbytes()) == 0 }
    }
}

impl LispString {
    /// Cheap alias of a string whose bytes are BORROWED (mapped pdump image /
    /// static rodata, `storage_capacity == 0`) and interval-free: copies the
    /// header words and shares the byte pointer — no allocation, no byte
    /// copy, and dropping the alias frees nothing. `None` for allocator-owned
    /// or interval-carrying strings (use `clone`).
    ///
    /// SAFETY-BY-CONTRACT: the caller may hold the alias only while the
    /// borrowed bytes stay mapped; for pdump image strings that is the
    /// process lifetime (the same contract the aliased string itself relies
    /// on).
    pub(crate) fn borrowed_alias(&self) -> Option<LispString> {
        if self.storage_capacity != 0 || self.has_intervals() {
            return None;
        }
        Some(LispString {
            size: self.size,
            size_byte: self.size_byte,
            intervals: AtomicPtr::new(std::ptr::null_mut()),
            data: self.data,
            storage_capacity: 0,
        })
    }
}

impl Clone for LispString {
    /// Mutator-side only (dereferences the interval table via `intervals`).
    fn clone(&self) -> Self {
        let intervals = if self.has_intervals() {
            Box::into_raw(Box::new(self.intervals().to_owned_table()))
        } else {
            std::ptr::null_mut()
        };
        let mut cloned = Self::from_owned_payload(
            Self::copy_payload(self.as_bytes()),
            self.size,
            self.size_byte,
        );
        cloned.intervals = AtomicPtr::new(intervals);
        cloned
    }
}

/// Rust-owned string payloads retain the Lisp objects in their interval plists.
/// Heap strings and Context-held copies have the same tracing obligation.
impl crate::gc_trace::GcTrace for LispString {
    fn trace_roots(&self, roots: &mut Vec<crate::emacs_core::value::Value>) {
        self.intervals().for_each_root(|root| roots.push(root));
    }

    fn trace_roots_with(&self, visit: &mut dyn FnMut(crate::emacs_core::value::Value)) {
        self.intervals().for_each_root(visit);
    }
}

impl Drop for LispString {
    fn drop(&mut self) {
        // `&mut self` during drop: no concurrent GC read can be in flight for
        // a string being freed (sweeps never overlap a concurrent mark), so
        // `get_mut` needs no atomics.
        let ptr = *self.intervals.get_mut();
        if !ptr.is_null() {
            // Safety: unique owner of a table created by `Box::into_raw` in
            // `ensure_intervals`/`clone`.
            drop(unsafe { Box::from_raw(ptr) });
        }
        self.release_owned_storage();
        // GNU `sweep_strings` ends a dead string with (src/alloc.c:1878-1882)
        //
        //     /* Reset the strings's `data' member so that we
        //        know it's free.  */
        //     s->u.s.data = NULL;
        //
        // beside `data->string = NULL` on the sdata (:1877), and reads the
        // marker back at :1851 (`if (s->u.s.data)`) and :1892 ("S was on the
        // free-list before").  It is the string-side equivalent of
        // `dead_object ()` for conses: "is this header reclaimed" answered in
        // O(1) — DIVERGENCES.md 161 §6, half two.
        //
        // `release_owned_storage` above only nulls a string that OWNED its
        // bytes; a borrowed / mapped / static-rodata payload
        // (`storage_capacity == 0`) returns early, which left a swept pdump
        // string byte-identical to a live one and a stale borrow of it
        // silently readable.  Null it unconditionally, as GNU does.
        self.data = std::ptr::null();
    }
}

impl std::fmt::Debug for LispString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text: String = self
            .as_utf8_str()
            .map(|s| s.to_owned())
            .unwrap_or_else(|| format!("<{} bytes>", self.as_bytes().len()));
        f.debug_struct("LispString")
            .field("text", &text)
            .field("multibyte", &self.is_multibyte())
            .finish()
    }
}

// Preserve the existing heap accessor seam; raw decoding belongs to the
// character module, which establishes the EmacsChar invariant.
pub(crate) use crate::emacs_core::emacs_char::decode_declared_multibyte_char;

impl PartialEq for LispString {
    fn eq(&self, other: &Self) -> bool {
        self.is_multibyte() == other.is_multibyte() && self.as_bytes() == other.as_bytes()
    }
}

impl Eq for LispString {}

impl std::hash::Hash for LispString {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_bytes().hash(state);
        self.is_multibyte().hash(state);
    }
}

impl Serialize for LispString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("LispString", 3)?;
        state.serialize_field("data", self.as_bytes())?;
        state.serialize_field("size", &self.schars())?;
        state.serialize_field("size_byte", &self.size_byte)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for LispString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct LispStringOwned {
            data: Vec<u8>,
            size: usize,
            size_byte: i64,
        }

        let owned = LispStringOwned::deserialize(deserializer)?;
        Ok(Self::from_dump(owned.data, owned.size, owned.size_byte))
    }
}

#[cfg(test)]
#[path = "heap_types/tests/heap_types_test.rs"]
mod tests;

#[cfg(test)]
#[path = "heap_types/tests/declared_character_count.rs"]
mod declared_character_count_tests;

#[cfg(test)]
#[path = "heap_types/tests/string_collection_capacity_test.rs"]
mod string_collection_capacity_tests;

#[cfg(test)]
#[path = "heap_types/tests/text_property_view_test.rs"]
mod text_property_view_tests;

#[cfg(test)]
#[path = "heap_types/tests/clear_string_in_place_test.rs"]
mod clear_string_in_place_tests;

#[cfg(test)]
#[path = "heap_types/tests/declared_string_safety.rs"]
mod declared_string_safety_tests;

/// Creation-order identity of one overlay, used only for the final
/// precedence tie-break in GNU `compare_overlays` (`src/buffer.c:3256-3282`).
///
/// GNU breaks a fully equal tie (same priority, same range, same secondary
/// priority) by the overlay object's address and documents that ordering as
/// arbitrary and possibly different between invocations. Neomacs keeps the
/// branch deterministic: every freshly created overlay is minted a strictly
/// larger serial from its World's counter, so later creation sorts later and
/// wins. Distinct-key precedence is untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct OverlaySerial(std::num::NonZeroU64);

impl OverlaySerial {
    /// An identity read back from an image. Zero was never a valid serial.
    #[inline]
    pub fn from_u64(raw: u64) -> Option<Self> {
        std::num::NonZeroU64::new(raw).map(Self)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

// The serial rides inside `OverlayData`; it must not grow the overlay object.
const _: () = assert!(std::mem::size_of::<OverlaySerial>() == 8);
const _: () = assert!(std::mem::size_of::<Option<OverlaySerial>>() == 8);
// The World-owned counter is one atomic word appended to the heap's tail.
const _: () = assert!(std::mem::size_of::<OverlaySerialCounter>() == 8);

/// Which precedence identity a not-yet-published overlay gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlaySerialSource {
    /// A new Lisp object: mint the World's next creation serial.
    Mint,
    /// A redisplay observer copy, buffered copy or loaded image keeps the
    /// identity it already carries.
    Preserved(OverlaySerial),
}

/// The World's overlay creation counter. Exactly one per `TaggedHeap`, which
/// is itself owned by the World allocation: not process-global, not TLS. It
/// is atomic so the World is ready for several mutators; ordering between
/// distinct serials only needs the atomic read-modify-write, hence `Relaxed`.
#[derive(Debug, Default)]
pub struct OverlaySerialCounter(std::sync::atomic::AtomicU64);

impl OverlaySerialCounter {
    pub const fn new() -> Self {
        Self(std::sync::atomic::AtomicU64::new(0))
    }

    /// Issue the next creation serial. The counter stores the last issued
    /// value; a `fetch_add` therefore yields the successor.
    #[inline]
    pub fn mint(&self) -> OverlaySerial {
        let next = self
            .0
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            .checked_add(1)
            .expect("overlay serial space exhausted");
        OverlaySerial(
            std::num::NonZeroU64::new(next).expect("a minted overlay serial is never zero"),
        )
    }

    /// Resume above an image's largest preserved serial, so every later
    /// creation still sorts after every restored overlay.
    #[inline]
    pub fn observe(&self, serial: OverlaySerial) {
        self.0
            .fetch_max(serial.get(), std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub struct OverlayData {
    /// Creation-order identity for the final precedence tie-break. Minted
    /// before the owning mutator publishes a fresh allocation; observer
    /// copies and loaded images preserve it (see [`OverlaySerial`]).
    pub serial: OverlaySerial,
    pub plist: crate::emacs_core::value::Value,
    pub buffer: Option<BufferId>,
    pub start: usize,
    pub end: usize,
    /// Runtime-only authority for lazily shifted live endpoints. Detached and
    /// deserialized overlays materialize `start`/`end` and carry no handle.
    pub(crate) position_handle: Option<crate::buffer::overlay_index::OverlayPositionHandle>,
    pub front_advance: bool,
    pub rear_advance: bool,
}

/// Public construction state for an overlay that has not yet been attached to
/// an [`OverlayList`](crate::buffer::overlay::OverlayList).
///
/// Keeping this distinct from [`OverlayData`] makes it impossible for callers
/// outside `neovm-core` to forge or retain the runtime-only position handle.
/// [`Value::make_overlay`](crate::emacs_core::value::Value::make_overlay)
/// converts this value into the live heap representation with no position
/// authority until an overlay list attaches it.
#[derive(Clone, Debug)]
pub struct OverlayDataInit {
    /// `Mint` for a new Lisp object; `Preserved` for observer copies,
    /// buffered copies and image loads.
    pub serial: OverlaySerialSource,
    pub plist: crate::emacs_core::value::Value,
    pub buffer: Option<BufferId>,
    pub start: usize,
    pub end: usize,
    pub front_advance: bool,
    pub rear_advance: bool,
}

impl OverlayData {
    /// Build the live payload with the identity the allocator resolved for it.
    /// The serial is passed separately because `Mint` needs the World's
    /// counter, which only the heap owns.
    pub(crate) fn from_init(init: OverlayDataInit, serial: OverlaySerial) -> Self {
        Self {
            serial,
            plist: init.plist,
            buffer: init.buffer,
            start: init.start,
            end: init.end,
            position_handle: None,
            front_advance: init.front_advance,
            rear_advance: init.rear_advance,
        }
    }
}

impl Clone for OverlayData {
    fn clone(&self) -> Self {
        let (start, end) = self.current_range();
        Self {
            serial: self.serial,
            plist: self.plist,
            buffer: self.buffer,
            start,
            end,
            position_handle: None,
            front_advance: self.front_advance,
            rear_advance: self.rear_advance,
        }
    }
}

impl OverlayData {
    pub(crate) fn current_range(&self) -> (usize, usize) {
        crate::buffer::overlay_index::current_overlay_range(self)
            .map(|range| (range.start().get(), range.end().get()))
            .unwrap_or((self.start, self.end))
    }
}

/// Where a marker points, in one 8-byte word: `None` (the whole word is
/// zero) or the owning buffer's id as a `NonZeroU64`.
///
/// `BufferManager` ids start at 1 (`next_id: 1`) and only real buffers get
/// one, so every attached marker's id has a nonzero niche. The constructor
/// keeps that as a debug assertion instead of a checked return because the
/// alternative -- storing `Option<BufferId>` -- costs a second word on every
/// marker, and rejecting a null buffer id is a caller bug, never a runtime
/// condition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct MarkerBuffer(Option<std::num::NonZeroU64>);

impl MarkerBuffer {
    /// A marker that points nowhere (GNU `marker->buffer == NULL`).
    pub const DETACHED: Self = Self(None);

    #[inline]
    pub fn attached(buffer: BufferId) -> Self {
        debug_assert_ne!(
            buffer.0, 0,
            "BufferId(0) is not a live buffer; a marker buffer must name a real buffer"
        );
        Self(std::num::NonZeroU64::new(buffer.0))
    }

    #[inline]
    pub const fn is_attached(self) -> bool {
        self.0.is_some()
    }

    #[inline]
    pub fn buffer_id(self) -> Option<BufferId> {
        self.0.map(|id| BufferId(id.get()))
    }

    #[inline]
    pub fn detach(&mut self) {
        self.0 = None;
    }
}

/// The intrusive marker-chain link, 8 bytes: three flag bits in an
/// 8-byte-aligned `MarkerObj` pointer. A null pointer with flags set is the
/// chain tail of a marker whose flags are on; it is a valid encoded value.
///
/// All pointer/flag bit arithmetic lives here. Marker objects are allocated
/// by `ObjectArena<MarkerObj>` (64-byte slots) or mapped by the pdump at a
/// `MarkerObj` alignment boundary, so the low three bits are always free.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct MarkerLink(usize);

const MARKER_LINK_FLAG_BITS: usize = 0b111;
const MARKER_LINK_INSERTION_TYPE: usize = 0b001;
const MARKER_LINK_LAST_POSITION_VALID: usize = 0b010;
const MARKER_LINK_CHAINED: usize = 0b100;

impl MarkerLink {
    pub const NONE: Self = Self(0);

    #[inline]
    pub fn from_ptr(next: *mut crate::tagged::header::MarkerObj) -> Self {
        debug_assert_eq!(
            next as usize & MARKER_LINK_FLAG_BITS,
            0,
            "MarkerObj pointer must be 8-byte aligned for flag tagging"
        );
        Self(next as usize)
    }

    #[inline]
    pub const fn next_ptr(self) -> *mut crate::tagged::header::MarkerObj {
        (self.0 & !MARKER_LINK_FLAG_BITS) as *mut _
    }

    /// GNU `Lisp_Marker.insertion_type`: true advances over text inserted at
    /// the marker's position.
    #[inline]
    pub const fn insertion_type(self) -> bool {
        self.0 & MARKER_LINK_INSERTION_TYPE != 0
    }

    /// True once the marker has had a real position (GNU keeps `charpos`
    /// across detach; this bit separates "never positioned" from position 0).
    #[inline]
    pub const fn last_position_valid(self) -> bool {
        self.0 & MARKER_LINK_LAST_POSITION_VALID != 0
    }

    /// Whether this marker is currently spliced into a buffer's chain. The
    /// tail's null `next` cannot answer this, so the bit is maintained at
    /// every chain-membership change.
    #[inline]
    pub const fn chained(self) -> bool {
        self.0 & MARKER_LINK_CHAINED != 0
    }

    #[inline]
    pub fn with_next(self, next: *mut crate::tagged::header::MarkerObj) -> Self {
        debug_assert_eq!(
            next as usize & MARKER_LINK_FLAG_BITS,
            0,
            "MarkerObj pointer must be 8-byte aligned for flag tagging"
        );
        Self((self.0 & MARKER_LINK_FLAG_BITS) | next as usize)
    }

    #[inline]
    pub const fn with_insertion_type(self, value: bool) -> Self {
        if value {
            Self(self.0 | MARKER_LINK_INSERTION_TYPE)
        } else {
            Self(self.0 & !MARKER_LINK_INSERTION_TYPE)
        }
    }

    #[inline]
    pub const fn with_last_position_valid(self, value: bool) -> Self {
        if value {
            Self(self.0 | MARKER_LINK_LAST_POSITION_VALID)
        } else {
            Self(self.0 & !MARKER_LINK_LAST_POSITION_VALID)
        }
    }

    #[inline]
    pub const fn with_chained(self, value: bool) -> Self {
        if value {
            Self(self.0 | MARKER_LINK_CHAINED)
        } else {
            Self(self.0 & !MARKER_LINK_CHAINED)
        }
    }
}

/// One marker object's Lisp-visible state: 32 bytes, so `MarkerObj` (24-byte
/// header + this) is 56 bytes and fits the zero-slack 64-byte arena class.
///
/// GNU's `struct Lisp_Marker` (lisp.h) is the same shape: a buffer identity,
/// a chain link carrying `insertion_type`/`need_adjustment`, `charpos` and
/// `bytepos`. Marker identity is the `MarkerObj` pointer; the object carries
/// no integer id. Dump/load allocates ephemeral ids (see pdump convert) and
/// the loader resolves them to these pointers.
#[derive(Clone, Debug)]
#[repr(C)]
pub struct LispMarker {
    /// Owning buffer, or detached.
    pub buffer: MarkerBuffer,
    /// Intrusive chain link plus the three marker flags. `next_ptr()` is null
    /// at the chain tail. `unchain_dead_markers` walks these before
    /// `sweep_objects` frees unmarked markers.
    pub next_marker: MarkerLink,
    /// Char offset in buffer (0-based; authoritative after T6/T7).
    pub charpos: CharPos0,
    /// Byte offset in buffer (0-based Emacs byte position; authoritative).
    pub bytepos: EmacsBytePos,
}

impl LispMarker {
    pub fn new(
        buffer: MarkerBuffer,
        charpos: CharPos0,
        bytepos: EmacsBytePos,
        insertion_type: bool,
        last_position_valid: bool,
    ) -> Self {
        Self {
            buffer,
            next_marker: MarkerLink::NONE
                .with_insertion_type(insertion_type)
                .with_last_position_valid(last_position_valid),
            charpos,
            bytepos,
        }
    }

    #[inline]
    pub const fn insertion_type(&self) -> bool {
        self.next_marker.insertion_type()
    }

    #[inline]
    pub fn set_insertion_type(&mut self, value: bool) {
        self.next_marker = self.next_marker.with_insertion_type(value);
    }

    #[inline]
    pub const fn last_position_valid(&self) -> bool {
        self.next_marker.last_position_valid()
    }

    #[inline]
    pub fn set_last_position_valid(&mut self, value: bool) {
        self.next_marker = self.next_marker.with_last_position_valid(value);
    }

    #[inline]
    pub const fn chained(&self) -> bool {
        self.next_marker.chained()
    }

    #[inline]
    pub fn set_chained(&mut self, value: bool) {
        self.next_marker = self.next_marker.with_chained(value);
    }

    #[inline]
    pub const fn next_marker_ptr(&self) -> *mut crate::tagged::header::MarkerObj {
        self.next_marker.next_ptr()
    }

    #[inline]
    pub fn set_next_marker(&mut self, next: *mut crate::tagged::header::MarkerObj) {
        self.next_marker = self.next_marker.with_next(next);
    }

    #[inline]
    pub fn buffer_id(&self) -> Option<BufferId> {
        self.buffer.buffer_id()
    }

    #[inline]
    pub fn attach_to(&mut self, buffer: BufferId) {
        self.buffer = MarkerBuffer::attached(buffer);
    }

    #[inline]
    pub fn detach(&mut self) {
        self.buffer.detach();
    }
}

const _: () = {
    assert!(std::mem::size_of::<LispMarker>() == 32);
    assert!(std::mem::align_of::<LispMarker>() >= 8);
    assert!(std::mem::size_of::<MarkerBuffer>() == 8);
    assert!(std::mem::size_of::<MarkerLink>() == 8);
    assert!(std::mem::size_of::<crate::tagged::header::MarkerObj>() <= 56);
    assert!(std::mem::align_of::<crate::tagged::header::MarkerObj>() >= 8);
};
#[cfg(test)]
#[path = "tests/heap_types_marker_test.rs"]
mod marker_test;

#[cfg(test)]
#[path = "heap_types/tests/char_byte_midpoint.rs"]
mod char_byte_midpoint_tests;

#[cfg(test)]
#[path = "heap_types/tests/signed_string_positions.rs"]
mod signed_string_positions_tests;

#[cfg(test)]
#[path = "heap_types/tests/character_count_kind.rs"]
mod character_count_kind_tests;

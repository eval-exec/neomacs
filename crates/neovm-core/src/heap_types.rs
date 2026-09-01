//! Shared heap payload types used by both the tagged runtime and pdump code.
//!
//! Keeping them behind a neutral module boundary lets the tagged runtime and
//! dump/load code share the same payload structs without reviving old heap
//! module boundaries.

use crate::buffer::{BufferId, CharLen, CharPos0, CharRange, TextPropertyTable};
use crate::emacs_core::emacs_char;
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

/// A Lisp string.
///
/// Backing bytes use Emacs internal encoding (a UTF-8 superset). For standard
/// Unicode text the bytes are valid UTF-8; raw bytes 0x80-0xFF are encoded as
/// overlong two-byte sequences (C0/C1 lead byte) which makes `as_str()` return
/// `None` for those strings. Like GNU, `SDATA` has a trailing NUL byte after
/// `SBYTES`; that terminator is not part of the Lisp string contents.
///
/// - **Multibyte:** `size_byte >= 0`.  `size` = char count, `size_byte` = byte count.
/// - **Unibyte:**   `size_byte < 0`. `size` = byte count (each byte is one char).
///   GNU distinguishes `-1` normally allocated, `-2` rodata, and `-3`
///   immovable bytecode storage.
#[repr(C)]
pub struct LispString {
    /// Character count (cached).
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

// `data` always points into owned Vec storage or an immutable mapped/static
// region. Moving the Rust owner does not move Vec allocations, and mutation
// requires `&mut self`.
unsafe impl Send for LispString {}
unsafe impl Sync for LispString {}

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

fn empty_text_property_table() -> &'static TextPropertyTable {
    static EMPTY: OnceLock<TextPropertyTable> = OnceLock::new();
    EMPTY.get_or_init(TextPropertyTable::new)
}

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

impl LispString {
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

    fn payload_len_for(size: usize, size_byte: i64) -> usize {
        if size_byte < 0 {
            size
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

    fn from_owned_payload(mut payload: Vec<u8>, size: usize, size_byte: i64) -> Self {
        let size_byte = Self::normalize_size_byte(size_byte, false);
        Self::assert_valid_size_byte(size_byte);
        debug_assert_eq!(
            payload.len(),
            Self::payload_len_for(size, size_byte),
            "LispString storage length must match GNU size/size_byte fields"
        );
        payload.push(0);
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
        self.release_owned_storage();
        payload.push(0);
        self.data = payload.as_ptr();
        self.storage_capacity = payload.capacity();
        std::mem::forget(payload);
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

    /// Reconstruct a `LispString` from pdump data with pre-computed fields.
    /// The caller is responsible for passing consistent `data`, `size`, and
    /// `size_byte` values (as stored in the dump file).
    pub fn from_dump(data: Vec<u8>, size: usize, size_byte: i64) -> Self {
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
        unsafe { Self::from_borrowed_bytes(ptr, len, size, size_byte, false) }
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

    /// Character count.
    pub fn schars(&self) -> usize {
        self.size
    }

    /// Byte count.  For multibyte strings this is `data.len()`; for unibyte
    /// strings this is also `data.len()` (= size, since each byte is one char).
    pub fn sbytes(&self) -> usize {
        if self.size_byte < 0 {
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
    ///
    /// MUTATOR-SIDE ONLY: dereferences the interval table, which the mutator
    /// can free at any time via `clear_intervals` — the concurrent GC thread
    /// must never call this (it reads only the pointer word via
    /// `intervals_ptr`; a dereference here on the GC thread is a
    /// use-after-free).
    pub fn intervals(&self) -> &TextPropertyTable {
        let ptr = self.intervals.load(Ordering::Acquire);
        if ptr.is_null() {
            empty_text_property_table()
        } else {
            // Safety: non-null means a live Box-allocated table; it is freed
            // only by `clear_intervals`/`Drop` (both `&mut self`), which the
            // caller's `&self` excludes on the mutator thread.
            unsafe { &*ptr }
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
        let char_pos = char_pos.min(self.schars());
        if !self.is_multibyte() || self.schars() == self.sbytes() {
            char_pos
        } else {
            emacs_char::char_to_byte_pos(self.as_bytes(), char_pos)
        }
    }

    /// Return the character position for a byte offset.
    ///
    /// Mirrors GNU `string_byte_to_char`'s `SCHARS == SBYTES` fast path.
    pub(crate) fn byte_to_char_pos(&self, byte_pos: usize) -> usize {
        let byte_pos = byte_pos.min(self.sbytes());
        if !self.is_multibyte() || self.schars() == self.sbytes() {
            byte_pos
        } else {
            emacs_char::byte_to_char_pos(self.as_bytes(), byte_pos)
        }
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
    fn recompute_size(&mut self, byte_len: usize) {
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
                Self::from_owned_payload(Self::copy_payload(slice), char_len, slice.len() as i64)
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
        self.slice_no_properties(0, self.as_bytes().len())
            .unwrap_or_else(|| self.clone())
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
            (self.byte_to_char_pos(start), self.byte_to_char_pos(end))
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

        let mut intervals = self.intervals().clone();
        intervals.append_shifted_at_char_offset(other.intervals(), CharLen::new(self.schars()));
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
            Box::into_raw(Box::new(self.intervals().clone()))
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
        state.serialize_field("size", &self.size)?;
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
mod tests {
    use super::LispString;

    #[test]
    fn lisp_string_layout_keeps_gnu_fields_before_storage_metadata() {
        assert_eq!(std::mem::offset_of!(LispString, size), 0);
        assert!(
            std::mem::offset_of!(LispString, size_byte) > std::mem::offset_of!(LispString, size)
        );
        assert!(
            std::mem::offset_of!(LispString, intervals)
                > std::mem::offset_of!(LispString, size_byte)
        );
        assert!(
            std::mem::offset_of!(LispString, data) > std::mem::offset_of!(LispString, intervals)
        );
        assert!(
            std::mem::offset_of!(LispString, storage_capacity)
                > std::mem::offset_of!(LispString, data)
        );
        #[cfg(target_pointer_width = "64")]
        assert_eq!(std::mem::size_of::<LispString>(), 40);
        // The interval field is an AtomicPtr (concurrent GC null-check reads):
        // same size + null niche as the GNU-compatible raw interval pointer,
        // and as the Option<Box<_>> it replaced.
        assert_eq!(
            std::mem::size_of::<std::sync::atomic::AtomicPtr<crate::buffer::TextPropertyTable>>(),
            std::mem::size_of::<usize>()
        );
    }

    #[test]
    fn mapped_lisp_string_borrows_until_mutation() {
        let bytes = b"abc\0".to_vec();
        let mut string = unsafe { LispString::from_mapped_bytes(bytes.as_ptr(), 3, 3, 3) };

        assert_eq!(string.as_bytes(), b"abc");
        assert!(string.has_trailing_nul());
        string.mutate_bytes(|bytes| bytes.push(b'd'));

        drop(bytes);
        assert_eq!(string.as_bytes(), b"abcd");
        assert_eq!(string.schars(), 4);
        assert_eq!(string.sbytes(), 4);
        assert!(string.has_trailing_nul());
    }

    #[test]
    fn mapped_lisp_string_clone_is_owned() {
        let bytes = b"abc\0".to_vec();
        let string = unsafe { LispString::from_mapped_bytes(bytes.as_ptr(), 3, 3, 3) };
        let cloned = string.clone();

        drop(bytes);
        assert_eq!(cloned.as_bytes(), b"abc");
        assert!(cloned.has_trailing_nul());
    }

    #[test]
    fn gnu_unibyte_size_byte_states_are_distinct() {
        let normal = LispString::from_unibyte(b"abc".to_vec());
        assert_eq!(normal.size_byte(), -1);
        assert!(!normal.is_multibyte());
        assert!(!normal.is_rodata());
        assert!(!normal.is_immovable());

        let rodata = LispString::from_rodata_unibyte(b"abc\0");
        assert_eq!(rodata.size_byte(), -2);
        assert!(!rodata.is_multibyte());
        assert!(rodata.is_rodata());
        assert!(!rodata.is_immovable());
        assert_eq!(rodata.as_bytes(), b"abc");
        assert!(rodata.has_trailing_nul());

        let mut immovable = LispString::from_unibyte(b"abc".to_vec());
        immovable.pin_immovable();
        assert_eq!(immovable.size_byte(), -3);
        assert!(!immovable.is_multibyte());
        assert!(!immovable.is_rodata());
        assert!(immovable.is_immovable());
    }

    #[test]
    fn rodata_unibyte_demotes_to_normal_on_mutation() {
        let mut string = LispString::from_rodata_unibyte(b"abc\0");
        string.mutate_bytes(|bytes| bytes[0] = b'X');

        assert_eq!(string.as_bytes(), b"Xbc");
        assert_eq!(string.size_byte(), -1);
        assert!(!string.is_rodata());
        assert!(string.has_trailing_nul());
    }

    #[test]
    fn replacing_string_contents_updates_cached_lengths() {
        let mut multibyte = LispString::from_utf8("é");
        multibyte.set_from_str("longer");
        assert_eq!(multibyte.as_bytes(), b"longer");
        assert_eq!(multibyte.schars(), 6);
        assert_eq!(multibyte.sbytes(), 6);
        assert!(multibyte.has_trailing_nul());

        let mut rodata = LispString::from_rodata_unibyte(b"abc\0");
        rodata.set_from_str("longer");
        assert_eq!(rodata.as_bytes(), b"longer");
        assert_eq!(rodata.size_byte(), -1);
        assert!(rodata.has_owned_storage());
        assert!(rodata.has_trailing_nul());
    }

    #[test]
    fn mutate_bytes_recomputes_multibyte_size_and_preserves_nul() {
        let mut string = LispString::from_utf8("é");
        assert_eq!(string.schars(), 1);
        assert_eq!(string.sbytes(), 2);

        string.mutate_bytes(|bytes| bytes.extend_from_slice("x".as_bytes()));

        assert_eq!(string.as_bytes(), "éx".as_bytes());
        assert_eq!(string.schars(), 2);
        assert_eq!(string.sbytes(), 3);
        assert_eq!(string.size_byte(), 3);
        assert!(string.has_trailing_nul());
    }

    #[test]
    fn owned_and_dump_strings_have_gnu_trailing_nul_after_sbytes() {
        let strings = [
            LispString::from_utf8("abc"),
            LispString::from_unibyte(b"abc".to_vec()),
            LispString::from_dump(b"abc".to_vec(), 3, 3),
        ];

        for string in strings {
            assert_eq!(string.as_bytes(), b"abc");
            assert!(string.has_trailing_nul());
        }
    }

    #[test]
    fn owned_dump_data_cannot_claim_rodata_size_byte() {
        let string = LispString::from_dump(b"abc".to_vec(), 3, -2);

        assert_eq!(string.as_bytes(), b"abc");
        assert_eq!(string.size_byte(), -1);
        assert!(!string.is_rodata());
        assert!(string.has_trailing_nul());
    }

    #[test]
    fn equal_unibyte_storage_classes_hash_identically() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let normal = LispString::from_unibyte(b"abc".to_vec());
        let mut immovable = LispString::from_unibyte(b"abc".to_vec());
        immovable.pin_immovable();

        assert_eq!(normal, immovable);

        let mut normal_hash = DefaultHasher::new();
        normal.hash(&mut normal_hash);
        let mut immovable_hash = DefaultHasher::new();
        immovable.hash(&mut immovable_hash);
        assert_eq!(normal_hash.finish(), immovable_hash.finish());
    }
}

#[derive(Debug)]
pub struct OverlayData {
    /// Stable allocation identity used where GNU compares overlay Lisp object
    /// identity (`XLI (overlay)`).  Rust heap addresses are not monotonic, so
    /// this preserves GNU's allocation-order tiebreakers without depending on
    /// allocator layout.
    pub serial: u64,
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
    pub serial: u64,
    pub plist: crate::emacs_core::value::Value,
    pub buffer: Option<BufferId>,
    pub start: usize,
    pub end: usize,
    pub front_advance: bool,
    pub rear_advance: bool,
}

impl From<OverlayDataInit> for OverlayData {
    fn from(init: OverlayDataInit) -> Self {
        Self {
            serial: init.serial,
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

static NEXT_OVERLAY_SERIAL: AtomicU64 = AtomicU64::new(1);

pub fn next_overlay_serial() -> u64 {
    NEXT_OVERLAY_SERIAL.fetch_add(1, Ordering::Relaxed)
}

pub fn observe_overlay_serial(serial: u64) {
    if serial == 0 {
        return;
    }
    let mut current = NEXT_OVERLAY_SERIAL.load(Ordering::Relaxed);
    while current <= serial {
        match NEXT_OVERLAY_SERIAL.compare_exchange_weak(
            current,
            serial + 1,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(next) => current = next,
        }
    }
}

#[derive(Clone, Debug)]
pub struct LispMarker {
    pub buffer: Option<BufferId>,
    pub insertion_type: bool,
    pub marker_id: Option<u64>,
    /// Byte offset in buffer (authoritative after T6/T7).
    pub bytepos: usize,
    /// Char offset in buffer (authoritative after T6/T7).
    pub charpos: usize,
    /// True once this marker has been positioned at a real location.
    /// GNU's `unchain_marker` (marker.c:684) preserves a marker's
    /// `charpos` across detach, so `Fmarker_last_position` can report
    /// the last position even after the buffer is killed.  Neomacs
    /// stores positions in 0-based form, which collides with the
    /// "fresh" sentinel for a marker once attached at Lisp position 1.
    /// This flag disambiguates: false for `make-marker` results, true
    /// once `set-marker` (or insertion) has placed the marker.
    pub last_position_valid: bool,
    /// Intrusive link to next marker in the owning buffer's chain.
    /// `null` if not on a chain. GC sweep order: `unchain_dead_markers`
    /// walks these BEFORE `sweep_objects` frees unmarked markers.
    pub next_marker: *mut crate::tagged::header::MarkerObj,
}

#[cfg(test)]
#[path = "heap_types_marker_test.rs"]
mod marker_test;

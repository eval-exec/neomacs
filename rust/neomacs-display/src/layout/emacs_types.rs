//! Rust representations of Emacs C data structures.
//!
//! These types allow direct field access to Emacs structs without FFI calls,
//! eliminating per-call overhead for hot paths like buffer text access and
//! metadata reads.
//!
//! # Safety
//!
//! All offsets are validated at runtime against C `offsetof()` values on
//! first use. A mismatch (e.g., from `HAVE_TREE_SITTER` changing the field
//! count) panics with a clear diagnostic message.
//!
//! These types must only be used on the Emacs main thread during layout,
//! when buffer content is stable (after `ensure_fontified`, before GC).

use std::ffi::c_void;
use std::sync::OnceLock;

// ============================================================================
// Lisp_Object operations (x86-64, USE_LSB_TAG)
// ============================================================================

/// Emacs `Lisp_Object`: a tagged machine word.
/// On x86-64 with USE_LSB_TAG, this is a 64-bit signed integer where
/// the lower 3 bits hold the type tag.
pub type LispObject = i64;

/// GCTYPEBITS = 3 (number of tag bits)
const _GCTYPEBITS: u32 = 3;

/// INTTYPEBITS = GCTYPEBITS - 1 = 2 (bits used for fixnum tag discrimination)
const INTTYPEBITS: u32 = 2;

/// Check if a `Lisp_Object` is nil.
/// `Qnil` = 0 (Lisp_Symbol tag 0 + symbol index 0).
#[inline(always)]
pub fn nilp(obj: LispObject) -> bool {
    obj == 0
}

/// Check if a `Lisp_Object` is a fixnum.
/// Fixnum tags: `Lisp_Int0` = 2 (0b010), `Lisp_Int1` = 6 (0b110).
/// Both have lower 2 bits = 0b10.
#[inline(always)]
pub fn fixnump(obj: LispObject) -> bool {
    (obj & 3) == 2
}

/// Extract the integer value from a fixnum `Lisp_Object`.
/// Arithmetic right shift by INTTYPEBITS (2).
#[inline(always)]
pub fn xfixnum(obj: LispObject) -> i64 {
    obj >> INTTYPEBITS
}

/// Check if a `Lisp_Object` is a non-negative fixnum.
#[inline(always)]
pub fn fixnatp(obj: LispObject) -> bool {
    fixnump(obj) && xfixnum(obj) >= 0
}

/// Extract non-negative fixnum value, or return `None`.
#[inline(always)]
pub fn xfixnat(obj: LispObject) -> Option<i64> {
    if fixnatp(obj) {
        Some(xfixnum(obj))
    } else {
        None
    }
}

// ============================================================================
// Buffer text structure (partial: first 6 fields of struct buffer_text)
// ============================================================================

/// First 6 fields of Emacs `struct buffer_text`.
/// These are sufficient for gap buffer text access.
///
/// # Warning
///
/// This is a **partial** struct — the actual C struct has more fields after
/// `gap_size`. Do NOT use `std::mem::size_of::<EmacsBufferText>()` for
/// anything related to the C struct size. Always access through a pointer
/// obtained from `buf_text_ptr()`.
#[repr(C)]
pub struct EmacsBufferText {
    /// Actual address of buffer contents.
    pub beg: *mut u8,
    /// Char pos of gap in buffer.
    pub gpt: isize,
    /// Char pos of end of buffer.
    pub z: isize,
    /// Byte pos of gap in buffer.
    pub gpt_byte: isize,
    /// Byte pos of end of buffer.
    pub z_byte: isize,
    /// Size of buffer's gap in bytes.
    pub gap_size: isize,
}

// ============================================================================
// Buffer BVAR (Lisp_Object field) indices
// ============================================================================

/// Number of `Lisp_Object` fields in `struct buffer` (with `HAVE_TREE_SITTER=1`).
/// Validated at runtime against C.
pub const BUFFER_LISP_FIELD_COUNT: usize = 76;

/// Indices into the `Lisp_Object` array of `struct buffer`.
/// These correspond to the `BVAR(buf, field)` macro in C,
/// where the array starts immediately after the `vectorlike_header` (offset 8).
///
/// All indices are validated at runtime against C `offsetof()` values.
pub mod bvar {
    /// `name_` — buffer name
    pub const NAME: usize = 0;
    /// `tab_width_` — buffer-local tab width
    pub const TAB_WIDTH: usize = 20;
    /// `fill_column_` — buffer-local fill column
    pub const FILL_COLUMN: usize = 21;
    /// `truncate_lines_` — non-nil means don't wrap
    pub const TRUNCATE_LINES: usize = 28;
    /// `word_wrap_` — non-nil means word-wrap
    pub const WORD_WRAP: usize = 29;
    /// `selective_display_` — selective display level
    pub const SELECTIVE_DISPLAY: usize = 35;
    /// `enable_multibyte_characters_` — multibyte flag
    pub const ENABLE_MULTIBYTE_CHARACTERS: usize = 41;
    /// `pt_marker_` — point marker (for indirect buffers)
    pub const PT_MARKER: usize = 47;
    /// `begv_marker_` — BEGV marker (for indirect buffers)
    pub const BEGV_MARKER: usize = 48;
    /// `zv_marker_` — ZV marker (for indirect buffers)
    pub const ZV_MARKER: usize = 49;
}

// ============================================================================
// Direct struct field access functions
// ============================================================================

/// Offset of the Lisp_Object fields array in `struct buffer`.
/// Always 8 bytes (sizeof `vectorlike_header`).
const BUFFER_LISP_FIELDS_OFFSET: usize = 8;

/// Read a BVAR (`Lisp_Object` field) from a raw buffer pointer.
///
/// # Safety
///
/// `buf` must be a valid `struct buffer *` from Emacs.
/// `index` must be < `BUFFER_LISP_FIELD_COUNT`.
/// Offsets must have been validated via `ensure_offsets_valid()`.
#[inline(always)]
pub unsafe fn buf_bvar(buf: *const c_void, index: usize) -> LispObject {
    debug_assert!(index < BUFFER_LISP_FIELD_COUNT);
    let ptr = (buf as *const u8).add(BUFFER_LISP_FIELDS_OFFSET + index * 8) as *const LispObject;
    ptr.read()
}

/// Read the `text` pointer from `struct buffer`.
///
/// # Safety
///
/// `buf` must be a valid `struct buffer *`.
#[inline(always)]
pub unsafe fn buf_text_ptr(buf: *const c_void) -> *const EmacsBufferText {
    let off = offsets();
    let ptr = (buf as *const u8).add(off.buf_text) as *const *const EmacsBufferText;
    ptr.read()
}

/// Read `pt` (point char position) from `struct buffer`.
#[inline(always)]
pub unsafe fn buf_pt(buf: *const c_void) -> isize {
    let off = offsets();
    let ptr = (buf as *const u8).add(off.buf_pt) as *const isize;
    ptr.read()
}

/// Read `pt_byte` (point byte position) from `struct buffer`.
#[inline(always)]
pub unsafe fn buf_pt_byte(buf: *const c_void) -> isize {
    let off = offsets();
    let ptr = (buf as *const u8).add(off.buf_pt_byte) as *const isize;
    ptr.read()
}

/// Read `begv` (beginning of accessible range, char position) from `struct buffer`.
#[inline(always)]
pub unsafe fn buf_begv(buf: *const c_void) -> isize {
    let off = offsets();
    let ptr = (buf as *const u8).add(off.buf_begv) as *const isize;
    ptr.read()
}

/// Read `begv_byte` from `struct buffer`.
#[inline(always)]
pub unsafe fn buf_begv_byte(buf: *const c_void) -> isize {
    let off = offsets();
    let ptr = (buf as *const u8).add(off.buf_begv_byte) as *const isize;
    ptr.read()
}

/// Read `zv` (end of accessible range, char position) from `struct buffer`.
#[inline(always)]
pub unsafe fn buf_zv(buf: *const c_void) -> isize {
    let off = offsets();
    let ptr = (buf as *const u8).add(off.buf_zv) as *const isize;
    ptr.read()
}

/// Read `zv_byte` from `struct buffer`.
#[inline(always)]
pub unsafe fn buf_zv_byte(buf: *const c_void) -> isize {
    let off = offsets();
    let ptr = (buf as *const u8).add(off.buf_zv_byte) as *const isize;
    ptr.read()
}

/// Read `base_buffer` pointer from `struct buffer`.
/// Returns null for ordinary buffers, non-null for indirect buffers.
#[inline(always)]
pub unsafe fn buf_base_buffer(buf: *const c_void) -> *const c_void {
    let off = offsets();
    let ptr = (buf as *const u8).add(off.buf_base_buffer) as *const *const c_void;
    ptr.read()
}

// ============================================================================
// Higher-level buffer metadata accessors
// ============================================================================

/// Check if buffer uses multibyte encoding.
/// Equivalent to `!NILP(BVAR(buf, enable_multibyte_characters))`.
#[inline]
pub unsafe fn buffer_multibyte_p(buf: *const c_void) -> bool {
    !nilp(buf_bvar(buf, bvar::ENABLE_MULTIBYTE_CHARACTERS))
}

/// Get buffer-local tab-width.
/// Equivalent to `FIXNATP(BVAR(buf, tab_width)) ? XFIXNAT(...) : 8`.
#[inline]
pub unsafe fn buffer_tab_width(buf: *const c_void) -> i32 {
    xfixnat(buf_bvar(buf, bvar::TAB_WIDTH)).unwrap_or(8) as i32
}

/// Get buffer-local truncate-lines setting.
/// Equivalent to `!NILP(BVAR(buf, truncate_lines))`.
#[inline]
pub unsafe fn buffer_truncate_lines(buf: *const c_void) -> bool {
    !nilp(buf_bvar(buf, bvar::TRUNCATE_LINES))
}

/// Get buffer-local word-wrap setting.
#[inline]
pub unsafe fn buffer_word_wrap(buf: *const c_void) -> bool {
    !nilp(buf_bvar(buf, bvar::WORD_WRAP))
}

/// Get buffer point position, with marker fallback for indirect buffers.
///
/// For normal buffers (pt_marker is nil), reads `buf->pt` directly.
/// For indirect buffers, falls back to `marker_position()` via FFI.
#[inline]
pub unsafe fn buffer_point(buf: *const c_void) -> i64 {
    let pt_marker = buf_bvar(buf, bvar::PT_MARKER);
    if nilp(pt_marker) {
        buf_pt(buf) as i64
    } else {
        neomacs_layout_marker_position(pt_marker)
    }
}

/// Get buffer narrowing bounds (BEGV, ZV), with marker fallback.
///
/// For normal buffers, reads `buf->begv` and `buf->zv` directly.
/// For indirect buffers, falls back to `marker_position()` via FFI.
#[inline]
pub unsafe fn buffer_bounds(buf: *const c_void) -> (i64, i64) {
    let begv_marker = buf_bvar(buf, bvar::BEGV_MARKER);
    let zv_marker = buf_bvar(buf, bvar::ZV_MARKER);

    let begv = if nilp(begv_marker) {
        buf_begv(buf) as i64
    } else {
        neomacs_layout_marker_position(begv_marker)
    };

    let zv = if nilp(zv_marker) {
        buf_zv(buf) as i64
    } else {
        neomacs_layout_marker_position(zv_marker)
    };

    (begv, zv)
}

// ============================================================================
// Gap buffer direct access
// ============================================================================

/// BEG_BYTE constant: Emacs byte positions are 1-based.
const BEG_BYTE: isize = 1;

/// Compute the memory address of a byte in the gap buffer.
///
/// Equivalent to `BUF_BYTE_ADDRESS` in buffer.h:
/// ```c
/// buf->text->beg + pos - BEG_BYTE
///     + (pos < buf->text->gpt_byte ? 0 : buf->text->gap_size)
/// ```
///
/// # Safety
///
/// `text` must be a valid `struct buffer_text *`.
/// `byte_pos` must be within buffer bounds.
#[inline(always)]
pub unsafe fn buf_byte_address(text: *const EmacsBufferText, byte_pos: isize) -> *const u8 {
    let t = &*text;
    let offset = byte_pos - BEG_BYTE;
    let gap_adjust = if byte_pos < t.gpt_byte { 0 } else { t.gap_size };
    t.beg.add((offset + gap_adjust) as usize)
}

/// Read a single byte from the gap buffer at a byte position.
///
/// # Safety
///
/// `text` must be a valid `struct buffer_text *`.
/// `byte_pos` must be within buffer bounds.
#[inline(always)]
pub unsafe fn buf_fetch_byte(text: *const EmacsBufferText, byte_pos: isize) -> u8 {
    *buf_byte_address(text, byte_pos)
}

// ============================================================================
// Gap buffer bulk text copy
// ============================================================================

/// Copy raw bytes from the gap buffer into a Vec<u8>.
///
/// For multibyte buffers, copies the Emacs internal encoding (essentially UTF-8,
/// with rare 0xC0/0xC1 sequences for eight-bit characters).
/// For unibyte buffers, converts bytes >= 0x80 to proper UTF-8 (Latin-1 encoding).
///
/// `byte_from` and `byte_to` are 1-based Emacs byte positions.
/// The output vec is cleared first, then filled with the text bytes.
///
/// # Safety
///
/// `buf` must be a valid `struct buffer *`. Byte positions must be within bounds.
/// Must be called on the Emacs thread during layout (no GC, stable buffer content).
pub unsafe fn gap_buffer_copy_text(
    buf: *const c_void,
    byte_from: isize,
    byte_to: isize,
    out: &mut Vec<u8>,
) {
    out.clear();
    if byte_from >= byte_to {
        return;
    }

    let text = buf_text_ptr(buf);
    if text.is_null() {
        return;
    }
    let t = &*text;
    let multibyte = buffer_multibyte_p(buf);
    let gpt_byte = t.gpt_byte;
    let gap_size = t.gap_size;
    let beg = t.beg;

    if multibyte {
        // Multibyte: copy raw bytes from gap buffer (Emacs internal ≈ UTF-8).
        // Handle the gap: may need to copy in two parts.
        let total_bytes = (byte_to - byte_from) as usize;
        out.reserve(total_bytes);

        if byte_to <= gpt_byte {
            // Entire range is before gap
            let src = beg.add((byte_from - BEG_BYTE) as usize);
            let slice = std::slice::from_raw_parts(src, total_bytes);
            out.extend_from_slice(slice);
        } else if byte_from >= gpt_byte {
            // Entire range is after gap
            let src = beg.add((byte_from - BEG_BYTE + gap_size) as usize);
            let slice = std::slice::from_raw_parts(src, total_bytes);
            out.extend_from_slice(slice);
        } else {
            // Range spans the gap
            let before_gap = (gpt_byte - byte_from) as usize;
            let after_gap = (byte_to - gpt_byte) as usize;

            let src1 = beg.add((byte_from - BEG_BYTE) as usize);
            let slice1 = std::slice::from_raw_parts(src1, before_gap);
            out.extend_from_slice(slice1);

            let src2 = beg.add((gpt_byte - BEG_BYTE + gap_size) as usize);
            let slice2 = std::slice::from_raw_parts(src2, after_gap);
            out.extend_from_slice(slice2);
        }
    } else {
        // Unibyte: each byte is a character. Bytes >= 0x80 need to be
        // encoded as UTF-8 (Latin-1 supplement: U+0080 - U+00FF).
        let total_bytes = (byte_to - byte_from) as usize;
        out.reserve(total_bytes * 2); // worst case: all bytes >= 0x80 → 2 bytes each

        for pos in byte_from..byte_to {
            let b = buf_fetch_byte(text, pos);
            if b < 0x80 {
                out.push(b);
            } else {
                // Encode byte as UTF-8: U+0080-U+00FF
                out.push(0xC0 | (b >> 6));
                out.push(0x80 | (b & 0x3F));
            }
        }
    }
}

// ============================================================================
// Pseudovector type checking (Lisp_Object → struct pointer)
// ============================================================================

/// Lisp_Vectorlike tag value (lower 3 bits of tagged pointer).
const LISP_VECTORLIKE: i64 = 5;

/// PSEUDOVECTOR_SIZE_BITS = 12 on x86-64.
const PSEUDOVECTOR_SIZE_BITS: u32 = 12;

/// PSEUDOVECTOR_REST_BITS = 12 on x86-64.
const PSEUDOVECTOR_REST_BITS: u32 = 12;

/// PSEUDOVECTOR_AREA_BITS = SIZE + REST = 24.
const PSEUDOVECTOR_AREA_BITS: u32 = PSEUDOVECTOR_SIZE_BITS + PSEUDOVECTOR_REST_BITS;

/// PSEUDOVECTOR_FLAG = PTRDIFF_MAX - PTRDIFF_MAX / 2 = 2^62 on 64-bit.
const PSEUDOVECTOR_FLAG: i64 = i64::MAX - i64::MAX / 2;

/// PVEC_TYPE_MASK = 0x3F << PSEUDOVECTOR_AREA_BITS.
const PVEC_TYPE_MASK: i64 = 0x3F_i64 << PSEUDOVECTOR_AREA_BITS;

/// pvec_type enum values (from lisp.h).
const PVEC_FRAME: u32 = 10;
const PVEC_WINDOW: u32 = 11;
const PVEC_BUFFER: u32 = 13;

/// Check if a Lisp_Object is a vectorlike (tag check only).
#[inline(always)]
pub fn vectorlikep(obj: LispObject) -> bool {
    (obj & 7) == LISP_VECTORLIKE
}

/// Extract a raw struct pointer from a vectorlike Lisp_Object.
/// Clears the lower 3 tag bits.
///
/// # Safety
///
/// Caller must verify `vectorlikep(obj)` first.
#[inline(always)]
pub unsafe fn xuntag_vectorlike(obj: LispObject) -> *const c_void {
    (obj & !7_i64) as *const c_void
}

/// Check if a Lisp_Object is a specific pseudovector type.
///
/// Equivalent to `PSEUDOVECTORP(obj, pvec_type)` in C:
/// 1. Check vectorlike tag
/// 2. Read `vectorlike_header.size` (ptrdiff_t at offset 0)
/// 3. Check pseudovector type bits
///
/// # Safety
///
/// If `obj` has the vectorlike tag but points to invalid memory, this is UB.
/// Only call during layout when Lisp_Objects are known valid.
#[inline(always)]
pub unsafe fn pseudovectorp(obj: LispObject, pvec_type: u32) -> bool {
    if !vectorlikep(obj) {
        return false;
    }
    let ptr = xuntag_vectorlike(obj);
    // vectorlike_header.size is at offset 0, type isize (ptrdiff_t)
    let header_size = *(ptr as *const i64);
    let expected = PSEUDOVECTOR_FLAG | ((pvec_type as i64) << PSEUDOVECTOR_AREA_BITS);
    (header_size & (PSEUDOVECTOR_FLAG | PVEC_TYPE_MASK)) == expected
}

/// Check if a Lisp_Object is a window (`WINDOWP`).
#[inline(always)]
pub unsafe fn windowp(obj: LispObject) -> bool {
    pseudovectorp(obj, PVEC_WINDOW)
}

/// Check if a Lisp_Object is a buffer (`BUFFERP`).
#[inline(always)]
pub unsafe fn bufferp(obj: LispObject) -> bool {
    pseudovectorp(obj, PVEC_BUFFER)
}

/// Extract `struct window *` from a Lisp_Object (`XWINDOW`).
///
/// # Safety
///
/// Caller must verify `windowp(obj)` first.
#[inline(always)]
pub unsafe fn xwindow(obj: LispObject) -> *const c_void {
    xuntag_vectorlike(obj)
}

/// Extract `struct frame *` from a Lisp_Object (`XFRAME`).
///
/// # Safety
///
/// Caller must verify the object is a frame.
#[inline(always)]
pub unsafe fn xframe(obj: LispObject) -> *const c_void {
    xuntag_vectorlike(obj)
}

// ============================================================================
// Window/frame field accessors
// ============================================================================

/// Read `w->frame_` (Lisp_Object) from a window struct.
#[inline(always)]
pub unsafe fn win_frame(win: *const c_void) -> LispObject {
    let off = offsets();
    let ptr = (win as *const u8).add(off.win_frame) as *const LispObject;
    ptr.read()
}

/// Read `w->next_` (Lisp_Object) from a window struct.
#[inline(always)]
pub unsafe fn win_next(win: *const c_void) -> LispObject {
    let off = offsets();
    let ptr = (win as *const u8).add(off.win_next) as *const LispObject;
    ptr.read()
}

/// Read `w->contents_` (Lisp_Object) from a window struct.
/// If WINDOWP(contents) → internal node (has children).
/// If BUFFERP(contents) → leaf node (displays a buffer).
#[inline(always)]
pub unsafe fn win_contents(win: *const c_void) -> LispObject {
    let off = offsets();
    let ptr = (win as *const u8).add(off.win_contents) as *const LispObject;
    ptr.read()
}

/// Read `f->root_window` (Lisp_Object) from a frame struct.
#[inline(always)]
pub unsafe fn frame_root_window(frame: *const c_void) -> LispObject {
    let off = offsets();
    let ptr = (frame as *const u8).add(off.frame_root_window) as *const LispObject;
    ptr.read()
}

/// Read `f->selected_window` (Lisp_Object) from a frame struct.
#[inline(always)]
pub unsafe fn frame_selected_window(frame: *const c_void) -> LispObject {
    let off = offsets();
    let ptr = (frame as *const u8).add(off.frame_selected_window) as *const LispObject;
    ptr.read()
}

/// Read `f->minibuffer_window` (Lisp_Object) from a frame struct.
#[inline(always)]
pub unsafe fn frame_minibuffer_window(frame: *const c_void) -> LispObject {
    let off = offsets();
    let ptr = (frame as *const u8).add(off.frame_minibuffer_window) as *const LispObject;
    ptr.read()
}

/// Check if a frame owns its minibuffer window (`FRAME_HAS_MINIBUF_P`).
///
/// Equivalent to:
/// ```c
/// WINDOWP(f->minibuffer_window)
///     && XFRAME(XWINDOW(f->minibuffer_window)->frame) == f
/// ```
#[inline]
pub unsafe fn frame_has_minibuf_p(frame: *const c_void) -> bool {
    let mini = frame_minibuffer_window(frame);
    if !windowp(mini) {
        return false;
    }
    let mini_win = xwindow(mini);
    let mini_frame_obj = win_frame(mini_win);
    // Compare frame pointers
    xframe(mini_frame_obj) == frame
}

// ============================================================================
// Window tree operations
// ============================================================================

/// Count the number of leaf windows in a frame.
///
/// Equivalent to `neomacs_layout_frame_window_count()` in C.
/// Uses stack-based window tree traversal.
///
/// # Safety
///
/// `frame` must be a valid `struct frame *`.
pub unsafe fn frame_window_count(frame: *const c_void) -> i32 {
    if frame.is_null() {
        return 0;
    }

    let root = frame_root_window(frame);
    if !windowp(root) {
        return 0;
    }

    let mut count = 0i32;
    let mut stack: [*const c_void; 64] = [std::ptr::null(); 64];
    let mut sp = 0usize;
    stack[sp] = xwindow(root);
    sp += 1;

    while sp > 0 {
        sp -= 1;
        let w = stack[sp];
        let contents = win_contents(w);

        if windowp(contents) {
            // Internal node: push all children
            let mut child = xwindow(contents);
            while !child.is_null() {
                if sp < 64 {
                    stack[sp] = child;
                    sp += 1;
                }
                let next = win_next(child);
                child = if nilp(next) {
                    std::ptr::null()
                } else {
                    xwindow(next)
                };
            }
        } else {
            // Leaf node (buffer window)
            count += 1;
        }
    }

    // Count minibuffer if frame owns it
    if frame_has_minibuf_p(frame) {
        count += 1;
    }

    count
}

// ============================================================================
// Struct offset validation
// ============================================================================

/// Struct offsets reported by C `neomacs_get_struct_offsets()`.
/// Each field stores the `offsetof()` value for the corresponding C struct field.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct StructOffsets {
    // struct buffer offsets
    pub buf_text: usize,
    pub buf_pt: usize,
    pub buf_pt_byte: usize,
    pub buf_begv: usize,
    pub buf_begv_byte: usize,
    pub buf_zv: usize,
    pub buf_zv_byte: usize,
    pub buf_base_buffer: usize,
    pub buf_lisp_field_count: usize,
    // struct buffer_text offsets
    pub buftext_beg: usize,
    pub buftext_gpt: usize,
    pub buftext_z: usize,
    pub buftext_gpt_byte: usize,
    pub buftext_z_byte: usize,
    pub buftext_gap_size: usize,
    // BVAR field offsets (for index validation)
    pub buf_tab_width: usize,
    pub buf_truncate_lines: usize,
    pub buf_enable_multibyte: usize,
    pub buf_pt_marker: usize,
    pub buf_begv_marker: usize,
    pub buf_zv_marker: usize,
    pub buf_word_wrap: usize,
    pub buf_selective_display: usize,
    // struct window offsets
    pub win_frame: usize,
    pub win_next: usize,
    pub win_contents: usize,
    // struct frame offsets
    pub frame_root_window: usize,
    pub frame_selected_window: usize,
    pub frame_minibuffer_window: usize,
    // Pseudovector type constants
    pub pvec_window: usize,
    pub pvec_buffer: usize,
    pub pseudovector_area_bits: usize,
    pub pseudovector_flag: usize,
}

impl Default for StructOffsets {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

extern "C" {
    fn neomacs_get_struct_offsets(out: *mut StructOffsets);
    fn neomacs_layout_marker_position(marker: LispObject) -> i64;
}

/// Lazily-initialized and validated struct offsets.
static OFFSETS: OnceLock<StructOffsets> = OnceLock::new();

/// Get validated struct offsets. Initializes and validates on first call.
///
/// # Panics
///
/// Panics if C struct offsets don't match our Rust assumptions.
/// This catches ABI mismatches from config changes (e.g., HAVE_TREE_SITTER).
fn offsets() -> &'static StructOffsets {
    OFFSETS.get_or_init(|| {
        let mut off = StructOffsets::default();
        unsafe { neomacs_get_struct_offsets(&mut off) };
        validate_offsets(&off);
        log::info!("Emacs struct offsets validated successfully (lisp_fields={})", off.buf_lisp_field_count);
        off
    })
}

/// Validate that our compile-time assumptions match C's struct layout.
fn validate_offsets(off: &StructOffsets) {
    // Validate buffer_text field offsets (first 6 fields, all 8 bytes, no padding)
    assert_eq!(off.buftext_beg, 0,
        "buffer_text.beg offset mismatch: expected 0, got {}", off.buftext_beg);
    assert_eq!(off.buftext_gpt, 8,
        "buffer_text.gpt offset mismatch: expected 8, got {}", off.buftext_gpt);
    assert_eq!(off.buftext_z, 16,
        "buffer_text.z offset mismatch: expected 16, got {}", off.buftext_z);
    assert_eq!(off.buftext_gpt_byte, 24,
        "buffer_text.gpt_byte offset mismatch: expected 24, got {}", off.buftext_gpt_byte);
    assert_eq!(off.buftext_z_byte, 32,
        "buffer_text.z_byte offset mismatch: expected 32, got {}", off.buftext_z_byte);
    assert_eq!(off.buftext_gap_size, 40,
        "buffer_text.gap_size offset mismatch: expected 40, got {}", off.buftext_gap_size);

    // Validate Lisp_Object field count
    assert_eq!(off.buf_lisp_field_count, BUFFER_LISP_FIELD_COUNT,
        "Buffer Lisp field count mismatch: expected {}, got {}. \
         Check HAVE_TREE_SITTER and other config flags.",
        BUFFER_LISP_FIELD_COUNT, off.buf_lisp_field_count);

    // Validate BVAR index calculations: offset should be 8 + index * 8
    let check_bvar = |name: &str, c_offset: usize, index: usize| {
        let expected = BUFFER_LISP_FIELDS_OFFSET + index * 8;
        assert_eq!(c_offset, expected,
            "BVAR {} offset mismatch: C says {}, we computed {} (index {})",
            name, c_offset, expected, index);
    };

    check_bvar("tab_width", off.buf_tab_width, bvar::TAB_WIDTH);
    check_bvar("truncate_lines", off.buf_truncate_lines, bvar::TRUNCATE_LINES);
    check_bvar("enable_multibyte_characters", off.buf_enable_multibyte, bvar::ENABLE_MULTIBYTE_CHARACTERS);
    check_bvar("pt_marker", off.buf_pt_marker, bvar::PT_MARKER);
    check_bvar("begv_marker", off.buf_begv_marker, bvar::BEGV_MARKER);
    check_bvar("zv_marker", off.buf_zv_marker, bvar::ZV_MARKER);
    check_bvar("word_wrap", off.buf_word_wrap, bvar::WORD_WRAP);
    check_bvar("selective_display", off.buf_selective_display, bvar::SELECTIVE_DISPLAY);

    // Validate pseudovector constants
    assert_eq!(off.pseudovector_area_bits, PSEUDOVECTOR_AREA_BITS as usize,
        "PSEUDOVECTOR_AREA_BITS mismatch: C={}, Rust={}",
        off.pseudovector_area_bits, PSEUDOVECTOR_AREA_BITS);
    assert_eq!(off.pseudovector_flag, PSEUDOVECTOR_FLAG as usize,
        "PSEUDOVECTOR_FLAG mismatch: C={}, Rust={}",
        off.pseudovector_flag, PSEUDOVECTOR_FLAG);
    assert_eq!(off.pvec_window, PVEC_WINDOW as usize,
        "PVEC_WINDOW mismatch: C={}, Rust={}", off.pvec_window, PVEC_WINDOW);
    assert_eq!(off.pvec_buffer, PVEC_BUFFER as usize,
        "PVEC_BUFFER mismatch: C={}, Rust={}", off.pvec_buffer, PVEC_BUFFER);

    // Log window/frame offsets (validated dynamically, not hardcoded)
    log::info!("Window offsets: frame={}, next={}, contents={}",
        off.win_frame, off.win_next, off.win_contents);
    log::info!("Frame offsets: root_window={}, selected_window={}, minibuffer_window={}",
        off.frame_root_window, off.frame_selected_window, off.frame_minibuffer_window);
}

/// Explicitly trigger offset validation. Call this on first layout frame.
/// Returns true on first call (when validation actually runs), false on subsequent calls.
pub fn ensure_offsets_valid() -> bool {
    let first = OFFSETS.get().is_none();
    let _ = offsets(); // triggers validation if needed
    first
}

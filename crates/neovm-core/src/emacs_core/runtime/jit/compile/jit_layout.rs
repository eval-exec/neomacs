//! The JIT's one view of the host layouts it bakes into generated code
//! (p1-0-integration §3.3 and §3.5, p2-0-integration S0.7,
//! p3-0-integration §3.8 = U1.1).
//!
//! Generated code reads and writes Rust data in place: the specpdl `Vec`,
//! its `SpecBinding` entries, a symbol's cells, a byte-code function's words
//! and the heap's objects. Every fact such code relies on lives here, in one
//! of two forms:
//!
//! * a **constant** (`offset_of!`, `size_of!`), pinned with const asserts,
//!   when the type's layout is ours to fix (`repr(C)`, the specpdl's
//!   primitive-representation variant records, or an exported field); and
//! * a **probe**, when std or rustc owns the layout (`Vec`'s field order,
//!   an `Option<Runtime>` niche). A probe measures live values, cross-checks
//!   two differently shaped samples, and answers `None` rather than a guess
//!   when anything disagrees. `None` turns the feature that asked off (with
//!   one `tracing::warn!`); it never miscompiles.
//!
//! Recognizing an entry the JIT wrote uses a *masked* compare,
//! `(word & header_mask) == header`: Rust leaves padding bytes undefined, so
//! only the bytes a variant defines (its tag, and small fields such as
//! `debug_on_exit` or `nargs`) take part.
//!
//! Everything here is JIT-only: a baked host offset is a fact of the process
//! that generated the code, so AOT code may use only the plain constants, and
//! only through `compute_abi_tag`'s salt.
//!
//! The heap's object layouts (the `GcHeader` byte map, the cons-block
//! trailer, float and vector words) are the [`heap`] submodule.

use crate::emacs_core::eval::{
    Context, SpecBinding, SpecBindingTag, specbinding_records as records,
};
use crate::emacs_core::symbol::LispSymbol;
use crate::emacs_core::value::Value;
use std::mem::{ManuallyDrop, offset_of, size_of};
use std::sync::OnceLock;

pub(crate) mod heap;

/// The buffer walk's probes (`Context -> BufferManager -> current buffer`),
/// kept with the buffer types they measure.
pub(crate) use crate::buffer::buffer::jit_layout as buffer_walk;

const WORD: usize = size_of::<usize>();

// ---------------------------------------------------------------------------
// `Vec` offsets.
// ---------------------------------------------------------------------------

/// Byte offsets, within a `Vec<T>` itself, of its data pointer, length and
/// capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct VecOffsets {
    pub(crate) ptr: usize,
    pub(crate) len: usize,
    pub(crate) cap: usize,
}

fn words_of<T>(value: &T) -> Vec<usize> {
    let base = std::ptr::from_ref(value).cast::<usize>();
    // SAFETY: reads whole words inside `value`.
    (0..size_of::<T>() / WORD)
        .map(|i| unsafe { base.add(i).read_unaligned() })
        .collect()
}

/// The byte offset of the one word in `words` equal to `want`; `None` when
/// no word or more than one does (an ambiguous probe proves nothing).
fn find_unique_word(words: &[usize], want: usize) -> Option<usize> {
    let mut hits = words.iter().enumerate().filter(|&(_, &w)| w == want);
    let (index, _) = hits.next()?;
    hits.next().is_none().then_some(index * WORD)
}

/// [`VecOffsets`] for `Vec<T>`, measured on live vectors whose elements
/// `sample` makes (so the probe never holds an uninitialised element).
/// Two vectors of different shapes, each with length and capacity kept
/// apart, must agree; `None` otherwise. `std` promises nothing about the
/// order of the three words, so this is the only sound way to bake them.
pub(crate) fn vec_offsets_with<T>(sample: impl Fn() -> T) -> Option<VecOffsets> {
    if size_of::<T>() == 0 || size_of::<Vec<T>>() != 3 * WORD {
        return None;
    }
    let probe = |capacity: usize, length: usize| -> Option<VecOffsets> {
        let mut v: Vec<T> = Vec::with_capacity(capacity);
        v.extend((0..length).map(|_| sample()));
        if v.capacity() == v.len() {
            return None;
        }
        let words = words_of(&v);
        let found = VecOffsets {
            ptr: find_unique_word(&words, v.as_ptr() as usize)?,
            len: find_unique_word(&words, v.len())?,
            cap: find_unique_word(&words, v.capacity())?,
        };
        let distinct = found.ptr != found.len && found.len != found.cap && found.ptr != found.cap;
        distinct.then_some(found)
    };
    let first = probe(5, 3)?;
    (first == probe(11, 7)?).then_some(first)
}

/// [`VecOffsets`] of `Context::specpdl`'s `Vec<SpecBinding>`.
pub(crate) fn specpdl_vec_offsets() -> Option<VecOffsets> {
    static OFFSETS: OnceLock<Option<VecOffsets>> = OnceLock::new();
    *OFFSETS.get_or_init(|| vec_offsets_with(|| SpecBinding::GcRoot { value: Value::NIL }))
}

/// [`VecOffsets`] of `Context::jit_bind_stack`'s `Vec<usize>`.
pub(crate) fn jit_bind_stack_vec_offsets() -> Option<VecOffsets> {
    static OFFSETS: OnceLock<Option<VecOffsets>> = OnceLock::new();
    *OFFSETS.get_or_init(|| vec_offsets_with(|| 0usize))
}

// ---------------------------------------------------------------------------
// Context words.
// ---------------------------------------------------------------------------

/// `Context::specpdl` (the `Vec` itself; add a [`VecOffsets`] field).
pub(crate) const CONTEXT_SPECPDL_OFFSET: usize = offset_of!(Context, specpdl);
/// `Context::jit_bind_stack` (the `Vec` itself).
pub(crate) const CONTEXT_JIT_BIND_STACK_OFFSET: usize = offset_of!(Context, jit_bind_stack);
/// `Context::depth`: the Lisp evaluation depth (`lisp_eval_depth`), a `usize`.
pub(crate) const CONTEXT_DEPTH_OFFSET: usize = offset_of!(Context, depth);
/// `Context::max_depth`: `max-lisp-eval-depth` as the evaluator reads it.
pub(crate) const CONTEXT_MAX_DEPTH_OFFSET: usize = offset_of!(Context, max_depth);
/// `Context::obarray` (inline); add an `OBARRAY_*` offset.
pub(crate) const CONTEXT_OBARRAY_OFFSET: usize = offset_of!(Context, obarray);
/// `Context::jit_stack_limit`, the native stack guard's floor.
pub(crate) const CONTEXT_JIT_STACK_LIMIT_OFFSET: usize = offset_of!(Context, jit_stack_limit);
pub(crate) use crate::emacs_core::eval::runtime_projection::{
    CONTEXT_ATTENTION_OFFSET, CONTEXT_BUFFERS_OFFSET, CONTEXT_TAGGED_HEAP_OFFSET,
};

const _: () = {
    assert!(size_of::<usize>() == 8, "the JIT targets 64-bit hosts");
    // The depth words are plain machine words compiled code compares and
    // bumps in place.
    assert!(size_of::<usize>() == size_of::<u64>());
};

// ---------------------------------------------------------------------------
// The symbol byte window and cells (§3.5).
// ---------------------------------------------------------------------------

pub(crate) use crate::emacs_core::symbol::{
    LISP_SYMBOL_FLAGS_OFFSET, LISP_SYMBOL_INTERNED_GLOBAL_OFFSET, LISP_SYMBOL_SIZE,
    LISP_SYMBOL_VAL_OFFSET, LISP_SYMBOL_WRITE_WINDOW_OFFSET, OBARRAY_CHUNK_BITS,
    OBARRAY_CHUNK_SLOTS, OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET, OBARRAY_FUNCTION_EPOCH_OFFSET,
    OBARRAY_JIT_LEN_OFFSET, OBARRAY_JIT_SPINE_OFFSET, SYMBOL_FLAGS_REDIRECT_MASK,
};

/// A symbol's function cell (`LispSymbol::function`, one `Value` word):
/// what a named call's callee is read from (P2.3's function-cell guard).
pub(crate) const LISP_SYMBOL_FUNCTION_OFFSET: usize = offset_of!(LispSymbol, function);

const _: () = {
    assert!(size_of::<Value>() == WORD);
    assert!(LISP_SYMBOL_FUNCTION_OFFSET % WORD == 0);
    assert!(LISP_SYMBOL_FUNCTION_OFFSET + WORD <= LISP_SYMBOL_SIZE);
    // The write window is one aligned 4-byte word holding the flags byte
    // and `interned_global`; the function cell never overlaps it.
    assert!(LISP_SYMBOL_WRITE_WINDOW_OFFSET % 4 == 0);
    assert!(LISP_SYMBOL_FLAGS_OFFSET & !3 == LISP_SYMBOL_WRITE_WINDOW_OFFSET);
    assert!(LISP_SYMBOL_INTERNED_GLOBAL_OFFSET & !3 == LISP_SYMBOL_WRITE_WINDOW_OFFSET);
    assert!(
        LISP_SYMBOL_FUNCTION_OFFSET + WORD <= LISP_SYMBOL_WRITE_WINDOW_OFFSET
            || LISP_SYMBOL_WRITE_WINDOW_OFFSET + 4 <= LISP_SYMBOL_FUNCTION_OFFSET
    );
};

// ---------------------------------------------------------------------------
// Variable cells beyond the symbol: the buffer-local cache record, the
// forwarder slots, the current buffer (P1.4 Stage B, `inline_vars`).
// ---------------------------------------------------------------------------

pub(crate) use crate::emacs_core::forward::{
    LISP_BOOL_FWD_VALUE_OFFSET, LISP_INT_FWD_VALUE_OFFSET, LISP_KBOARD_OBJ_FWD_VALUE_OFFSET,
    LISP_OBJ_FWD_VALUE_OFFSET,
};
pub(crate) use crate::emacs_core::symbol::blv_alist_epoch_addr;

/// `LispBufferLocalValue` (`#[repr(C)]`): GNU `local_if_set`, a `bool`.
pub(crate) const BLV_LOCAL_IF_SET_OFFSET: usize = offset_of!(
    crate::emacs_core::symbol::LispBufferLocalValue,
    local_if_set
);
/// GNU `found`, a `bool`: the byte after `local_if_set` (asserted), so the
/// two read as one `u16`, `local_if_set | found << 8`.
pub(crate) const BLV_FOUND_OFFSET: usize =
    offset_of!(crate::emacs_core::symbol::LispBufferLocalValue, found);
/// GNU `fwd`: an `Option<&'static LispFwd>`, null for none.
pub(crate) const BLV_FWD_OFFSET: usize =
    offset_of!(crate::emacs_core::symbol::LispBufferLocalValue, fwd);
/// The raw id of the buffer the cache is loaded for (`NO_WHERE_BUF` when
/// none, never 0).
pub(crate) const BLV_WHERE_BUF_ID_OFFSET: usize = offset_of!(
    crate::emacs_core::symbol::LispBufferLocalValue,
    where_buf_id
);
/// `(SYMBOL . DEFAULT-VALUE)`.
pub(crate) const BLV_DEFCELL_OFFSET: usize =
    offset_of!(crate::emacs_core::symbol::LispBufferLocalValue, defcell);
/// `(SYMBOL . CURRENT-VALUE)`, the loaded cell.
pub(crate) const BLV_VALCELL_OFFSET: usize =
    offset_of!(crate::emacs_core::symbol::LispBufferLocalValue, valcell);
/// The structural epoch the cache was loaded at (a `u64`).
pub(crate) const BLV_ALIST_EPOCH_OFFSET: usize =
    offset_of!(crate::emacs_core::symbol::LispBufferLocalValue, alist_epoch);

/// From a `*mut Context`, the current buffer's raw id (0 for none).
pub(crate) const CONTEXT_CURRENT_BUFFER_RAW_OFFSET: usize =
    CONTEXT_BUFFERS_OFFSET + buffer_walk::BUFFER_MANAGER_CURRENT_RAW_OFFSET;

/// From the untagged address of a cons, its cdr.
pub(crate) const CONS_CAR_OFFSET: usize = offset_of!(crate::tagged::header::ConsCell, car);
pub(crate) const CONS_CDR_OFFSET: usize = offset_of!(crate::tagged::header::ConsCell, cdr_or_next);

const _: () = {
    assert!(BLV_FOUND_OFFSET == BLV_LOCAL_IF_SET_OFFSET + 1);
    assert!(size_of::<bool>() == 1);
    assert!(size_of::<Option<&'static crate::emacs_core::forward::LispFwd>>() == WORD);
    assert!(BLV_WHERE_BUF_ID_OFFSET % WORD == 0 && BLV_ALIST_EPOCH_OFFSET % WORD == 0);
    assert!(BLV_DEFCELL_OFFSET % WORD == 0 && BLV_VALCELL_OFFSET % WORD == 0);
    assert!(BLV_FWD_OFFSET % WORD == 0 && CONS_CDR_OFFSET % WORD == 0);
    // The inline read of the 16-bit write window is little-endian.
    assert!(cfg!(target_endian = "little"));
};

// ---------------------------------------------------------------------------
// Byte-code function words (p2-0-integration S0.7).
// ---------------------------------------------------------------------------

/// From the untagged address of a veclike object, its `VecLikeType` byte
/// (the type test of a closure source guard, P2.1 C5).
pub(crate) const VECLIKE_TYPE_TAG_OFFSET: usize =
    offset_of!(crate::tagged::header::VecLikeHeader, type_tag);

const _: () = {
    assert!(size_of::<crate::tagged::header::VecLikeType>() == 1);
    // A byte-code object starts with its veclike header.
    assert!(offset_of!(crate::tagged::header::ByteCodeObj, header) == 0);
};

/// From the untagged address of a byte-code object, its `ByteCodeFunction`.
pub(crate) const BYTECODE_OBJ_DATA_OFFSET: usize =
    offset_of!(crate::tagged::header::ByteCodeObj, data);

/// From the untagged address of a byte-code object, the one word of its
/// `runtime: Option<Runtime>` (the tiering handle every `make-closure`
/// instance of a source shares). The word holds what
/// [`runtime_identity_word`] answers -- NOT `Arc::as_ptr`, which is 16 bytes
/// further on (P2.1 correction 5): compare against this word only.
pub(crate) const BYTECODE_RUNTIME_WORD_OFFSET: usize =
    BYTECODE_OBJ_DATA_OFFSET + offset_of!(crate::emacs_core::bytecode::ByteCodeFunction, runtime);

const _: () = assert!(size_of::<Option<crate::emacs_core::jit::Runtime>>() == WORD);

/// The word a byte-code object's `runtime` field holds when it carries
/// `runtime`: the source identity a closure source guard compares
/// (`callee.runtime word == baked word`). Read out of an `Option<Runtime>`
/// the way the object stores it, so it is exactly the field's bit pattern.
pub(crate) fn runtime_identity_word(runtime: &crate::emacs_core::jit::Runtime) -> usize {
    let held: ManuallyDrop<Option<crate::emacs_core::jit::Runtime>> =
        ManuallyDrop::new(Some(runtime.clone()));
    // SAFETY: `Option<Runtime>` is one word (asserted above); read it whole.
    let word = unsafe { std::ptr::from_ref(&*held).cast::<usize>().read() };
    drop(ManuallyDrop::into_inner(held));
    word
}

/// Stable address of the source's monotone `AtomicU32` capture width. Taking
/// the actual field's address avoids assuming `Arc` header or Rust field order.
/// Threading: the owning relocated template keeps its Runtime Arc alive; an
/// emitted atomic load reads the shared counter, never mutator Lisp state.
pub(crate) fn runtime_patched_prefix_address(runtime: &crate::emacs_core::jit::Runtime) -> usize {
    std::ptr::from_ref(&runtime.patched_prefix) as usize
}

/// From the untagged address of a byte-code object, the `(data pointer,
/// length)` words of its constant vector: the closure constant base a
/// `make-closure`-patched leaf loads through. `None` when the vector
/// storage's layout is not the same for owned and mapped pools
/// (`LispValueVec::jit_slice_offsets`).
pub(crate) fn bytecode_constants_offsets() -> Option<(usize, usize)> {
    let (ptr, len) = crate::tagged::header::LispValueVec::jit_slice_offsets()?;
    let base = BYTECODE_OBJ_DATA_OFFSET
        + offset_of!(crate::emacs_core::bytecode::ByteCodeFunction, constants);
    Some((base + ptr, base + len))
}

// ---------------------------------------------------------------------------
// `SpecBinding` templates (§3.3).
// ---------------------------------------------------------------------------

const ENTRY_WORDS: usize = size_of::<SpecBinding>() / WORD;
const _: () = assert!(size_of::<SpecBinding>() == 4 * WORD && ENTRY_WORDS == 4);
const _: () = assert!(std::mem::align_of::<SpecBinding>() == WORD);

/// How generated code writes and recognizes one `SpecBinding` variant: a
/// header word holding the variant's tag and its one small field, plus up
/// to three word-sized fields. Derived from the C-layout variant records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EntryTemplate {
    /// Byte offset of the header word within the entry.
    pub(crate) header_offset: u32,
    /// The header word with its small field at 0 and its padding at 0.
    pub(crate) header: u64,
    /// The bytes of the header word the variant defines: the tag and the
    /// small field. Recognize an entry with `(word & mask) == header`.
    pub(crate) header_mask: u64,
    /// Bit shift of the small field (`debug_on_exit`, `nargs`, `sym_id`)
    /// inside the header word; `None` for a variant without one.
    pub(crate) small_shift: Option<u32>,
    /// Byte offsets of the word fields, in the constructor's order; unused
    /// entries are `u32::MAX`.
    pub(crate) fields: [u32; 3],
}

impl EntryTemplate {
    /// The header word for small-field value `small` (0 when the variant
    /// has none).
    pub(crate) fn header_with(&self, small: u32) -> u64 {
        match self.small_shift {
            Some(shift) => self.header | (u64::from(small) << shift),
            None => {
                debug_assert_eq!(small, 0, "variant has no small field");
                self.header
            }
        }
    }

    /// Byte offset of word field `i`.
    pub(crate) fn field(&self, i: usize) -> u32 {
        debug_assert_ne!(self.fields[i], u32::MAX, "no such field");
        self.fields[i]
    }

    /// Whether the entry image `words` carries this variant with small
    /// field `small` (the masked compare generated code makes).
    #[cfg(test)]
    pub(crate) fn matches(&self, words: &[u64; ENTRY_WORDS], small: u32) -> bool {
        words[self.header_offset as usize / WORD] & self.header_mask == self.header_with(small)
    }

    /// The image generated code writes: the header word, the fields, and
    /// zero everywhere else.
    #[cfg(test)]
    pub(crate) fn image(&self, small: u32, fields: &[u64]) -> [u64; ENTRY_WORDS] {
        let mut words = [0u64; ENTRY_WORDS];
        words[self.header_offset as usize / WORD] = self.header_with(small);
        for (i, &f) in fields.iter().enumerate() {
            words[self.field(i) as usize / WORD] = f;
        }
        words
    }
}

/// Build a template from the primitive-representation tag and the C-layout
/// record's offsets. Padding is never sampled or interpreted as a tag.
const fn entry_template(
    tag: SpecBindingTag,
    small: Option<(usize, usize)>,
    fields: [u32; 3],
) -> EntryTemplate {
    let (small_shift, small_mask) = match small {
        Some((offset, size)) => {
            assert!(offset + size <= WORD);
            let shift = (offset * 8) as u32;
            (Some(shift), ((1u64 << (size * 8)) - 1) << shift)
        }
        None => (None, 0),
    };
    EntryTemplate {
        header_offset: 0,
        header: tag as u64,
        header_mask: u8::MAX as u64 | small_mask,
        small_shift,
        fields,
    }
}

const _: () = {
    assert!(size_of::<SpecBindingTag>() == 1);
    assert!(offset_of!(records::Let, tag) == 0);
    assert!(offset_of!(records::Let, sym_id) == 4);
    assert!(offset_of!(records::Let, old_value) == 8);
    assert!(offset_of!(records::LetLocal, tag) == 0);
    assert!(offset_of!(records::LetLocal, sym_id) == 4);
    assert!(offset_of!(records::LetLocal, old_value) == 8);
    assert!(offset_of!(records::LetLocal, buffer_id) == 16);
    assert!(offset_of!(records::LetDefault, tag) == 0);
    assert!(offset_of!(records::LetDefault, sym_id) == 4);
    assert!(offset_of!(records::LetDefault, old_value) == 8);
    assert!(offset_of!(records::LetDefault, buffer_id) == 16);
    assert!(offset_of!(records::Backtrace1, tag) == 0);
    assert!(offset_of!(records::Backtrace1, debug_on_exit) == 1);
    assert!(offset_of!(records::Backtrace1, function) == 8);
    assert!(offset_of!(records::Backtrace1, arg) == 16);
    assert!(offset_of!(records::Backtrace2, tag) == 0);
    assert!(offset_of!(records::Backtrace2, function) == 8);
    assert!(offset_of!(records::Backtrace2, arg0) == 16);
    assert!(offset_of!(records::Backtrace2, arg1) == 24);
    assert!(offset_of!(records::BacktraceNative, tag) == 0);
    assert!(offset_of!(records::BacktraceNative, nargs) == 4);
    assert!(offset_of!(records::BacktraceNative, function) == 8);
    assert!(offset_of!(records::BacktraceNative, args_ptr) == 16);
    assert!(size_of::<records::Let>() == 16);
    assert!(size_of::<records::LetLocal>() == 24);
    assert!(size_of::<records::LetDefault>() == 24);
    assert!(size_of::<records::Backtrace1>() == 24);
    assert!(size_of::<records::Backtrace2>() == 32);
    assert!(size_of::<records::BacktraceNative>() == 24);
};

/// The lean backtrace frames a speculated call pushes, the three shapes
/// `Context::push_backtrace_frame_from_native_args` writes: `Backtrace1`
/// (one argument inline), `Backtrace2` (two) and `BacktraceNative` (a
/// pointer to the caller's argument words and their count).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BacktraceLayout {
    /// Fields `[function, arg]`; small field `debug_on_exit` (always false
    /// when generated code writes it: a flagged frame no longer matches).
    pub(crate) bt1: EntryTemplate,
    /// Fields `[function, arg0, arg1]`; no small field.
    pub(crate) bt2: EntryTemplate,
    /// Fields `[function, args_ptr]`; small field `nargs`.
    pub(crate) native: EntryTemplate,
}

impl BacktraceLayout {
    /// The template and small-field value of the frame a call with `nargs`
    /// arguments pushes (the shim's own choice of shape).
    pub(crate) fn frame_for(&self, nargs: usize) -> (EntryTemplate, u32) {
        match nargs {
            1 => (self.bt1, 0),
            2 => (self.bt2, 0),
            n => (self.native, n as u32),
        }
    }
}

/// The lean frame records are fixed by `SpecBinding`'s primitive repr.
/// Keep the optional interface shared by the direct-call layout admission.
pub(crate) const fn backtrace_layout() -> Option<BacktraceLayout> {
    Some(BacktraceLayout {
        bt1: entry_template(
            SpecBindingTag::Backtrace1,
            Some((
                offset_of!(records::Backtrace1, debug_on_exit),
                size_of::<bool>(),
            )),
            [
                offset_of!(records::Backtrace1, function) as u32,
                offset_of!(records::Backtrace1, arg) as u32,
                u32::MAX,
            ],
        ),
        bt2: entry_template(
            SpecBindingTag::Backtrace2,
            None,
            [
                offset_of!(records::Backtrace2, function) as u32,
                offset_of!(records::Backtrace2, arg0) as u32,
                offset_of!(records::Backtrace2, arg1) as u32,
            ],
        ),
        native: entry_template(
            SpecBindingTag::BacktraceNative,
            Some((
                offset_of!(records::BacktraceNative, nargs),
                size_of::<u32>(),
            )),
            [
                offset_of!(records::BacktraceNative, function) as u32,
                offset_of!(records::BacktraceNative, args_ptr) as u32,
                u32::MAX,
            ],
        ),
    })
}

/// The `let` entries P1.4's inline binds would push: `Let` (fields
/// `[old_value]`), `LetLocal` and `LetDefault` (fields `[old_value,
/// buffer_id]`), each with the bound symbol's id as the small field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LetLayout {
    pub(crate) let_: EntryTemplate,
    pub(crate) let_local: EntryTemplate,
    pub(crate) let_default: EntryTemplate,
}

/// The binding records are fixed by `SpecBinding`'s primitive repr: the
/// symbol and discriminant share the header word on every supported host.
pub(crate) const fn let_layout() -> LetLayout {
    LetLayout {
        let_: entry_template(
            SpecBindingTag::Let,
            Some((
                offset_of!(records::Let, sym_id),
                size_of::<crate::emacs_core::intern::SymId>(),
            )),
            [
                offset_of!(records::Let, old_value) as u32,
                u32::MAX,
                u32::MAX,
            ],
        ),
        let_local: entry_template(
            SpecBindingTag::LetLocal,
            Some((
                offset_of!(records::LetLocal, sym_id),
                size_of::<crate::emacs_core::intern::SymId>(),
            )),
            [
                offset_of!(records::LetLocal, old_value) as u32,
                offset_of!(records::LetLocal, buffer_id) as u32,
                u32::MAX,
            ],
        ),
        let_default: entry_template(
            SpecBindingTag::LetDefault,
            Some((
                offset_of!(records::LetDefault, sym_id),
                size_of::<crate::emacs_core::intern::SymId>(),
            )),
            [
                offset_of!(records::LetDefault, old_value) as u32,
                offset_of!(records::LetDefault, buffer_id) as u32,
                u32::MAX,
            ],
        ),
    }
}

#[cfg(test)]
#[path = "jit_layout/tests/golden_test.rs"]
mod tests;

#[cfg(test)]
#[path = "jit_layout/tests/runtime_prefix_test.rs"]
mod runtime_prefix_tests;

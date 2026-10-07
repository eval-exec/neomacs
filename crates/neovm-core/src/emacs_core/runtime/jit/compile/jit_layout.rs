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
//!   when the type's layout is ours to fix (`repr(C)` or a field the host
//!   module exports); and
//! * a **probe**, when std or rustc owns the layout (`Vec`'s field order,
//!   a `repr(Rust)` enum's tag). A probe measures live values, cross-checks
//!   two differently shaped samples, round-trips the image the JIT would
//!   write through a Rust `match`, and answers `None` rather than a guess
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

use crate::buffer::BufferId;
use crate::emacs_core::eval::{Context, SavedBindingValue, SavedBufferId, SpecBinding};
use crate::emacs_core::intern::SymId;
use crate::emacs_core::symbol::LispSymbol;
use crate::emacs_core::value::Value;
use std::mem::{ManuallyDrop, MaybeUninit, offset_of, size_of};
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
const _: () = assert!(size_of::<SpecBinding>() == 4 * WORD);
const _: () = assert!(std::mem::align_of::<SpecBinding>() == WORD);

/// How generated code writes and recognizes one `SpecBinding` variant: a
/// header word holding the variant's tag and its one small field, plus up
/// to three word-sized fields. Measured by [`probe_template`].
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
    pub(crate) fn matches(&self, words: &[u64; ENTRY_WORDS], small: u32) -> bool {
        words[self.header_offset as usize / WORD] & self.header_mask == self.header_with(small)
    }

    /// The image generated code writes: the header word, the fields, and
    /// zero everywhere else.
    pub(crate) fn image(&self, small: u32, fields: &[u64]) -> [u64; ENTRY_WORDS] {
        let mut words = [0u64; ENTRY_WORDS];
        words[self.header_offset as usize / WORD] = self.header_with(small);
        for (i, &f) in fields.iter().enumerate() {
            words[self.field(i) as usize / WORD] = f;
        }
        words
    }
}

type Image = [u64; ENTRY_WORDS];

/// Leave `residue` in the stack below the caller, where the next call's
/// frame -- [`construct_over`]'s -- will sit.
#[inline(never)]
fn smear_stack(residue: u8) {
    let junk = [residue; 1024];
    std::hint::black_box(&junk);
}

/// Build an entry with `make` and write it over a buffer filled with `fill`.
#[inline(never)]
fn construct_over(fill: u8, make: &dyn Fn() -> SpecBinding) -> Image {
    let entry = std::hint::black_box(make());
    let mut buf = MaybeUninit::<Image>::uninit();
    // SAFETY: `buf` is `ENTRY_WORDS` aligned words; fill every byte, write
    // the entry over it, and read the words back as bits.
    unsafe {
        std::ptr::write_bytes(buf.as_mut_ptr().cast::<u8>(), fill, size_of::<Image>());
        buf.as_mut_ptr().cast::<SpecBinding>().write(entry);
        buf.assume_init()
    }
}

/// Images of the entry `make` builds, each built over a different fill and
/// a different stack residue: rustc copies a variant's padding from its
/// temporary, which holds whatever the stack held, so a byte the images
/// agree on is defined by the variant (its tag, a field) or constant
/// padding, and a byte they disagree on is padding.
fn images_of(make: impl Fn() -> SpecBinding) -> Vec<Image> {
    [(0x00u8, 0x11u8), (0xff, 0xee), (0x5a, 0xa5), (0x33, 0xcc)]
        .into_iter()
        .map(|(fill, residue)| {
            smear_stack(residue);
            construct_over(fill, &make)
        })
        .collect()
}

/// What a variant's `match` arm finds in an entry: each word field's address
/// and bits, and the small field's address, size and value.
struct Found {
    words: [(*const u8, u64); 3],
    n_words: usize,
    small: Option<(*const u8, usize, u32)>,
}

impl Found {
    fn new(words: &[(*const u8, u64)], small: Option<(*const u8, usize, u32)>) -> Self {
        let mut all = [(std::ptr::null(), 0); 3];
        all[..words.len()].copy_from_slice(words);
        Found {
            words: all,
            n_words: words.len(),
            small,
        }
    }
}

/// The address and bits of a word-sized field.
fn word_field<T>(field: &T, bits: u64) -> (*const u8, u64) {
    debug_assert_eq!(size_of::<T>(), WORD);
    (std::ptr::from_ref(field).cast(), bits)
}

/// The address, size and value of a small field.
fn small_field<T>(field: &T, value: u32) -> Option<(*const u8, usize, u32)> {
    Some((std::ptr::from_ref(field).cast(), size_of::<T>(), value))
}

/// Look at an image in place, as the `SpecBinding` it holds (borrowed, so
/// field addresses are the image's own, and never dropped).
///
/// # Safety
///
/// `words` must be a valid `SpecBinding` (the probe only decodes images
/// whose tag bytes it copied from real entries of the same variant).
unsafe fn decode_image<R>(words: &Image, look: impl FnOnce(&SpecBinding) -> R) -> R {
    // SAFETY: `Image` is `SpecBinding`'s size and alignment (asserted) and,
    // per the contract, a valid entry.
    look(unsafe { &*words.as_ptr().cast::<SpecBinding>() })
}

/// Every probed variant, and two others, with every field zero (nil, null,
/// id 0): what tells tag bytes from padding.
fn reference_images() -> Vec<Vec<Image>> {
    let makes: [fn() -> SpecBinding; 8] = [
        || SpecBinding::Backtrace1 {
            function: Value::NIL,
            arg: Value::NIL,
            debug_on_exit: false,
        },
        || SpecBinding::Backtrace2 {
            function: Value::NIL,
            arg0: Value::NIL,
            arg1: Value::NIL,
        },
        || SpecBinding::BacktraceNative {
            function: Value::NIL,
            args_ptr: std::ptr::null(),
            nargs: 0,
        },
        || SpecBinding::Let {
            sym_id: SymId(0),
            old_value: SavedBindingValue::from_plain(Value::NIL),
        },
        || SpecBinding::LetLocal {
            sym_id: SymId(0),
            old_value: Value::NIL,
            buffer_id: BufferId(0),
        },
        || SpecBinding::LetDefault {
            sym_id: SymId(0),
            old_value: SavedBindingValue::from_plain(Value::NIL),
            buffer_id: SavedBufferId::from_option(None),
        },
        || SpecBinding::GcRoot { value: Value::NIL },
        || SpecBinding::LexicalEnv {
            old_lexenv: Value::NIL,
        },
    ];
    makes.iter().map(|make| images_of(make)).collect()
}

/// The byte at `(word, byte)` of every image, if they all agree.
fn constant_byte(images: &[Image], w: usize, b: usize) -> Option<u8> {
    let first = (images[0][w] >> (8 * b)) as u8;
    images
        .iter()
        .all(|image| (image[w] >> (8 * b)) as u8 == first)
        .then_some(first)
}

/// Measure the template of one variant. `make(small, fields)` builds it and
/// `find` is its `match` arm ([`Found`]; `None` for another variant).
/// `smalls` are two nonzero sample values of the small field (`[0, 0]` when
/// the variant has none; `[1, 1]` for a bool). `None` when any step cannot
/// prove its answer.
///
/// The fields' offsets come from the `match` itself. The tag bytes are the
/// bytes outside every field that hold one value in every construction of
/// this variant and a different one in every construction of some other
/// variant; padding holds garbage, or the same zero everywhere, so it is
/// neither. The header word is the one word holding the tag, and it may hold
/// nothing else but the small field. The template is then round-tripped.
fn probe_template(
    n_fields: usize,
    smalls: [u32; 2],
    make: impl Fn(u32, &[u64]) -> SpecBinding,
    find: impl Fn(&SpecBinding) -> Option<Found>,
) -> Option<EntryTemplate> {
    const SAMPLES: [[u64; 3]; 2] = [
        [
            0x5a5a_0000_0000_1230,
            0x5a5a_0000_0000_4560,
            0x5a5a_0000_0000_7890,
        ],
        [
            0x6b6b_0000_0001_0010,
            0x6b6b_0000_0002_0020,
            0x6b6b_0000_0003_0030,
        ],
    ];
    const ZERO: [u64; 3] = [0; 3];
    // The fields' offsets, from the variant's own `match`.
    let image = images_of(|| make(smalls[0], &SAMPLES[0][..n_fields]))[0];
    let base = image.as_ptr() as usize;
    // SAFETY: a real entry of the variant.
    let found = unsafe { decode_image(&image, &find) }?;
    if found.n_words != n_fields || found.small.is_some() != (smalls[0] != 0) {
        return None;
    }
    let mut fields = [u32::MAX; 3];
    let mut field_bytes = [0u64; ENTRY_WORDS];
    for (i, &(at, bits)) in found.words[..n_fields].iter().enumerate() {
        let off = (at as usize).checked_sub(base)?;
        if off % WORD != 0 || off >= size_of::<Image>() || bits != SAMPLES[0][i] {
            return None;
        }
        fields[i] = off as u32;
        field_bytes[off / WORD] = u64::MAX;
    }
    let small_at = match found.small {
        Some((at, size, value)) => {
            let off = (at as usize).checked_sub(base)?;
            let (w, b) = (off / WORD, off % WORD);
            if value != smalls[0] || size == 0 || b + size > WORD || field_bytes[w] != 0 {
                return None;
            }
            let mask = if size == WORD {
                u64::MAX
            } else {
                ((1u64 << (8 * size)) - 1) << (8 * b)
            };
            field_bytes[w] |= mask;
            Some((w, (8 * b) as u32, mask))
        }
        None => None,
    };
    // The tag bytes.
    let mine = images_of(|| make(0, &ZERO[..n_fields]));
    let others = reference_images();
    let mut tag = [0u64; ENTRY_WORDS];
    for (w, tag_word) in tag.iter_mut().enumerate() {
        for b in 0..WORD {
            let byte = 0xffu64 << (8 * b);
            if field_bytes[w] & byte != 0 {
                continue;
            }
            let Some(value) = constant_byte(&mine, w, b) else {
                continue;
            };
            if others
                .iter()
                .any(|other| constant_byte(other, w, b).is_some_and(|o| o != value))
            {
                *tag_word |= byte;
            }
        }
    }
    let mut tag_words = (0..ENTRY_WORDS).filter(|&w| tag[w] != 0);
    let h = tag_words.next()?;
    if tag_words.next().is_some() || fields[..n_fields].contains(&((h * WORD) as u32)) {
        return None;
    }
    if small_at.is_some_and(|(w, _, _)| w != h) {
        return None;
    }
    let template = EntryTemplate {
        header_offset: (h * WORD) as u32,
        header: mine[0][h] & tag[h],
        header_mask: tag[h] | small_at.map_or(0, |(_, _, mask)| mask),
        small_shift: small_at.map(|(_, shift, _)| shift),
        fields,
    };
    // Round trip: the image generated code would write decodes, through the
    // variant's `match`, as exactly the values it meant; every real entry of
    // the variant matches the template, and no other variant's does.
    for (s, sample) in SAMPLES.iter().enumerate() {
        let sample = &sample[..n_fields];
        for small in [0, smalls[s]] {
            let image = template.image(small, sample);
            // SAFETY: the tag bytes come from real entries of the variant.
            let decoded = unsafe { decode_image(&image, &find) }?;
            let values_ok = decoded.words[..n_fields]
                .iter()
                .zip(sample)
                .all(|(&(_, bits), &want)| bits == want);
            if !values_ok || decoded.small.map_or(0, |(_, _, v)| v) != small {
                return None;
            }
            if !images_of(|| make(small, sample))
                .iter()
                .all(|real| template.matches(real, small))
            {
                return None;
            }
        }
    }
    let matched_other = others.iter().flatten().any(|other| {
        // SAFETY: a real entry (of some variant).
        let same_variant = unsafe { decode_image(other, &find) }.is_some();
        !same_variant && template.matches(other, 0)
    });
    (!matched_other).then_some(template)
}

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

/// [`BacktraceLayout`], probed once; `None` turns direct calls off.
pub(crate) fn backtrace_layout() -> Option<BacktraceLayout> {
    static LAYOUT: OnceLock<Option<BacktraceLayout>> = OnceLock::new();
    *LAYOUT.get_or_init(|| {
        let layout = probe_backtrace_layout();
        if layout.is_none() {
            tracing::warn!(
                target: "neovm_jit",
                "SpecBinding backtrace layout probe failed; direct calls stay off"
            );
        }
        layout
    })
}

fn value_of(bits: u64) -> Value {
    Value::from_bits(bits as usize)
}

fn probe_backtrace_layout() -> Option<BacktraceLayout> {
    let bt1 = probe_template(
        2,
        [1, 1],
        |small, f| SpecBinding::Backtrace1 {
            function: value_of(f[0]),
            arg: value_of(f[1]),
            debug_on_exit: small != 0,
        },
        |e| match e {
            SpecBinding::Backtrace1 {
                function,
                arg,
                debug_on_exit,
            } => Some(Found::new(
                &[
                    word_field(function, function.bits() as u64),
                    word_field(arg, arg.bits() as u64),
                ],
                small_field(debug_on_exit, u32::from(*debug_on_exit)),
            )),
            _ => None,
        },
    )?;
    let bt2 = probe_template(
        3,
        [0, 0],
        |_, f| SpecBinding::Backtrace2 {
            function: value_of(f[0]),
            arg0: value_of(f[1]),
            arg1: value_of(f[2]),
        },
        |e| match e {
            SpecBinding::Backtrace2 {
                function,
                arg0,
                arg1,
            } => Some(Found::new(
                &[
                    word_field(function, function.bits() as u64),
                    word_field(arg0, arg0.bits() as u64),
                    word_field(arg1, arg1.bits() as u64),
                ],
                None,
            )),
            _ => None,
        },
    )?;
    let native = probe_template(
        2,
        [0x0003_0201, 0x7a00_00ff],
        |small, f| SpecBinding::BacktraceNative {
            function: value_of(f[0]),
            args_ptr: f[1] as usize as *const i64,
            nargs: small,
        },
        |e| match e {
            SpecBinding::BacktraceNative {
                function,
                args_ptr,
                nargs,
            } => Some(Found::new(
                &[
                    word_field(function, function.bits() as u64),
                    word_field(args_ptr, *args_ptr as usize as u64),
                ],
                small_field(nargs, *nargs),
            )),
            _ => None,
        },
    )?;
    Some(BacktraceLayout { bt1, bt2, native })
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

/// [`LetLayout`], probed once; `None` turns inline binds off.
pub(crate) fn let_layout() -> Option<LetLayout> {
    static LAYOUT: OnceLock<Option<LetLayout>> = OnceLock::new();
    *LAYOUT.get_or_init(|| {
        let layout = probe_let_layout();
        if layout.is_none() {
            tracing::warn!(
                target: "neovm_jit",
                "SpecBinding let layout probe failed; inline binds stay off"
            );
        }
        layout
    })
}

fn probe_let_layout() -> Option<LetLayout> {
    const SYMS: [u32; 2] = [0x0012_3457, 0x7654_3211];
    let let_ = probe_template(
        1,
        SYMS,
        |sym, f| SpecBinding::Let {
            sym_id: SymId(sym),
            old_value: SavedBindingValue::from_plain(value_of(f[0])),
        },
        |e| match e {
            SpecBinding::Let { sym_id, old_value } => Some(Found::new(
                &[word_field(old_value, old_value.as_plain().bits() as u64)],
                small_field(sym_id, sym_id.0),
            )),
            _ => None,
        },
    )?;
    let let_local = probe_template(
        2,
        SYMS,
        |sym, f| SpecBinding::LetLocal {
            sym_id: SymId(sym),
            old_value: value_of(f[0]),
            buffer_id: BufferId(f[1]),
        },
        |e| match e {
            SpecBinding::LetLocal {
                sym_id,
                old_value,
                buffer_id,
            } => Some(Found::new(
                &[
                    word_field(old_value, old_value.bits() as u64),
                    word_field(buffer_id, buffer_id.0),
                ],
                small_field(sym_id, sym_id.0),
            )),
            _ => None,
        },
    )?;
    let let_default = probe_template(
        2,
        SYMS,
        |sym, f| SpecBinding::LetDefault {
            sym_id: SymId(sym),
            old_value: SavedBindingValue::from_plain(value_of(f[0])),
            // Id 0 (the tag probe's all-zero entry) is "no buffer".
            buffer_id: SavedBufferId::from_option((f[1] != 0).then_some(BufferId(f[1]))),
        },
        |e| match e {
            SpecBinding::LetDefault {
                sym_id,
                old_value,
                buffer_id,
            } => Some(Found::new(
                &[
                    word_field(old_value, old_value.as_plain().bits() as u64),
                    word_field(buffer_id, buffer_id.get().map_or(0, |b| b.0)),
                ],
                small_field(sym_id, sym_id.0),
            )),
            _ => None,
        },
    )?;
    Some(LetLayout {
        let_,
        let_local,
        let_default,
    })
}

#[cfg(test)]
#[path = "jit_layout/tests/golden_test.rs"]
mod tests;

#[cfg(test)]
#[path = "jit_layout/tests/runtime_prefix_test.rs"]
mod runtime_prefix_tests;

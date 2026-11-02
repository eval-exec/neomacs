//! Mapped bytecode-stub image layout owned by the bottom layer.
//!
//! The pdump writer (upper `emacs_core::pdump`) bakes one `BytecodeExtras`
//! header plus its id/value/doc spans into the tail of a bytecode object's
//! mapped veclike span. The collector legs live in this layer, so the layout
//! type, the flag vocabulary and the synchronous child walker move here with
//! them; the writer and every loader validation stay upper, because they own
//! the dump representation and the image-integrity contract.

use super::value::TaggedValue;

/// Fixed header of the self-describing "bytecode extras" region that sits
/// immediately after a mapped `ByteCodeObj` inside its (extended) veclike
/// span. Everything the object-extra descriptor used to carry rides here
/// instead: scalars in this header, then `n_required + n_optional` u32 dump
/// symbol ids, padding to 8, `n_extra_slots` raw value words (fixup-patched
/// like slot spans), then the docstring bytes. Only Gnu-instruction
/// bytecode uses the region; `Decoded` instruction vectors (test-only in
/// practice) stay descriptor-driven.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct BytecodeExtras {
    pub max_stack: u64,
    pub n_required: u64,
    pub n_optional: u64,
    pub flags: u16,
    pub params_kind: u8,
    pub _pad_params: u8,
    pub rest_sym: u32,
    pub closure_slot_count: u32,
    pub n_extra_slots: u32,
    pub docstring_size: u32,
    pub _pad0: u32,
    pub docstring_size_byte: i64,
    /// v14: GNU byte-span offset RELATIVE to the owning object's veclike
    /// span offset (0 when absent — presence is BC_FLAG_HAS_GNU, not this
    /// field). Relative so a stub walker can locate the bytes from the
    /// object's own address without any load-time side table.
    pub gnu_rel: i64,
    pub gnu_len: u64,
    pub arglist_word: u64,
    pub env_word: u64,
    pub doc_form_word: u64,
    pub interactive_word: u64,
    /// v14: constants slot-span offset relative to the object span offset.
    pub const_rel: i64,
    /// v14: constants slot count (DumpSlotSpan::len semantics).
    pub const_count: u32,
    pub _pad: u32,
}

const _: () = assert!(std::mem::size_of::<BytecodeExtras>() == 120);

/// Walk every heap-reference word a LAZY bytecode stub's mapped regions
/// carry, WITHOUT materializing and without allocating: the GC's stub legs
/// (trace, collect, seed, verify) call this to reach children that live
/// only in the patched image — the four metadata value words, the
/// extra-slot words, and the constants slot span. Param SymId words are
/// deliberately not emitted (symbols are not heap-traced children), and
/// docstring/GNU bytes are not Values.
///
/// # Safety
/// `obj` must point at a live mapped `ByteCodeObj` whose extras region of
/// `extras_len` bytes is span-tail-adjacent (the dump reserves
/// object+extras as one span; `bytecode_extras_span` proves the same
/// arithmetic on the load side). The image words must already be PATCHED
/// (relocations + value fixups applied): the GC only runs after load
/// completes, which the caller doctrine in gc.rs guarantees.
///
/// Precondition, discharged by the upper loader before the object can be
/// reachable to any collector: `bytecode_extras_span` validated the extras
/// span of `extras_len` bytes and the baked-stub template check matched it
/// before the stub was published, and the mapped image is immutable after
/// load — so a concurrent GEN1 marker reading these words while the mutator
/// runs races with nothing. Callers are exactly the two collector legs
/// (`tagged::gc::mark_sweep` and `tagged::gc::incremental`) and the unit
/// test in `tagged/tests/pdump_image_test.rs`.
pub(crate) unsafe fn for_each_stub_bytecode_child(
    obj: *const crate::tagged::header::ByteCodeObj,
    extras_len: usize,
    mut f: impl FnMut(TaggedValue),
) {
    if extras_len < std::mem::size_of::<BytecodeExtras>() {
        return;
    }
    let base = obj as *const u8;
    let extras_ptr = unsafe { base.add(std::mem::size_of::<crate::tagged::header::ByteCodeObj>()) };
    // Spans are 8-aligned (alignment-checked at map time), so the header
    // reads are plain unaligned-safe loads of a Pod struct.
    let header: BytecodeExtras =
        unsafe { std::ptr::read_unaligned(extras_ptr.cast::<BytecodeExtras>()) };
    let word_at = |ptr: *const u8| -> TaggedValue {
        TaggedValue::from_bits(unsafe { std::ptr::read_unaligned(ptr.cast::<usize>()) })
    };
    if header.flags & BC_FLAG_HAS_ARGLIST != 0 {
        f(TaggedValue::from_bits(header.arglist_word as usize));
    }
    if header.flags & BC_FLAG_HAS_ENV != 0 {
        f(TaggedValue::from_bits(header.env_word as usize));
    }
    if header.flags & BC_FLAG_HAS_DOC_FORM != 0 {
        f(TaggedValue::from_bits(header.doc_form_word as usize));
    }
    if header.flags & BC_FLAG_HAS_INTERACTIVE != 0 {
        f(TaggedValue::from_bits(header.interactive_word as usize));
    }
    let ids = header.n_required as usize + header.n_optional as usize;
    let extra_off = (std::mem::size_of::<BytecodeExtras>() + ids * 4 + 7) & !7;
    let mut cursor = unsafe { extras_ptr.add(extra_off) };
    for _ in 0..header.n_extra_slots {
        f(word_at(cursor));
        cursor = unsafe { cursor.add(8) };
    }
    if header.const_count > 0 {
        let mut slot = unsafe { base.offset(header.const_rel as isize) };
        for _ in 0..header.const_count {
            f(word_at(slot));
            slot = unsafe { slot.add(std::mem::size_of::<TaggedValue>()) };
        }
    }
}

pub(crate) const BC_FLAG_LEXICAL: u16 = 1;
pub(crate) const BC_FLAG_OPS_SEALED: u16 = 2;
pub(crate) const BC_FLAG_HAS_REST: u16 = 4;
pub(crate) const BC_FLAG_HAS_DOCSTRING: u16 = 8;
pub(crate) const BC_FLAG_HAS_ARGLIST: u16 = 16;
pub(crate) const BC_FLAG_HAS_ENV: u16 = 32;
pub(crate) const BC_FLAG_HAS_DOC_FORM: u16 = 64;
pub(crate) const BC_FLAG_HAS_INTERACTIVE: u16 = 128;
/// v14: the function carries a GNU byte region (`gnu_rel`/`gnu_len` valid).
/// Replaces the ambiguous `gnu_len > 0 || gnu_offset > 0` presence test — a
/// zero-length region at offset zero was indistinguishable from absence.
pub(crate) const BC_FLAG_HAS_GNU: u16 = 256;

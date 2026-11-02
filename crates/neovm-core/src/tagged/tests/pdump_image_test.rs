//! Ordering and bounds oracle for the mapped bytecode-stub child walker.
//!
//! Both collector legs emit children through one synchronous walker; this
//! test pins the exact image-word order and the short-span guard so a move
//! or an emission edit cannot silently change which words the GC traces.

use crate::tagged::header::ByteCodeObj;
use crate::tagged::pdump_image::{
    BC_FLAG_HAS_ARGLIST, BC_FLAG_HAS_DOC_FORM, BC_FLAG_HAS_ENV, BC_FLAG_HAS_INTERACTIVE,
    BytecodeExtras, for_each_stub_bytecode_child,
};
use crate::tagged::value::TaggedValue;

const OBJ: usize = std::mem::size_of::<ByteCodeObj>();
const HDR: usize = std::mem::size_of::<BytecodeExtras>();
const N_EXTRA: usize = 2;
const N_CONST: usize = 3;
const EXTRAS_LEN: usize = HDR + N_EXTRA * 8;
const CONST_OFF: usize = OBJ + EXTRAS_LEN;

fn put_word(buf: &mut [u8], offset: usize, value: TaggedValue) {
    buf[offset..offset + 8].copy_from_slice(&value.bits().to_ne_bytes());
}

fn write_header(buf: &mut [u8], header: &BytecodeExtras) {
    buf[OBJ..OBJ + HDR].copy_from_slice(bytemuck::bytes_of(header));
}

fn walk(buf: &[u8], extras_len: usize) -> Vec<i64> {
    let mut words = Vec::new();
    // SAFETY: `buf` is a synthetic stand-in for the mapped object+extras
    // span. The walker never dereferences `obj` as a `ByteCodeObj`; it only
    // performs unaligned reads at OBJ..OBJ+HDR (checked by `extras_len`),
    // at the extra-slot words inside the declared extras length, and at the
    // constants span this test writes inside the same buffer.
    unsafe {
        for_each_stub_bytecode_child(buf.as_ptr().cast::<ByteCodeObj>(), extras_len, |word| {
            words.push(word.as_fixnum().expect("test words are fixnums"));
        });
    }
    words
}

#[test]
fn stub_child_walk_emits_exactly_the_image_words_in_order() {
    let mut buf = vec![0u8; OBJ + EXTRAS_LEN + N_CONST * 8];
    let mut header: BytecodeExtras = bytemuck::Zeroable::zeroed();
    header.flags =
        BC_FLAG_HAS_ARGLIST | BC_FLAG_HAS_ENV | BC_FLAG_HAS_DOC_FORM | BC_FLAG_HAS_INTERACTIVE;
    header.n_extra_slots = N_EXTRA as u32;
    header.const_count = N_CONST as u32;
    header.const_rel = CONST_OFF as i64;
    header.arglist_word = TaggedValue::fixnum(101).bits() as u64;
    header.env_word = TaggedValue::fixnum(102).bits() as u64;
    header.doc_form_word = TaggedValue::fixnum(103).bits() as u64;
    header.interactive_word = TaggedValue::fixnum(104).bits() as u64;
    write_header(&mut buf, &header);
    put_word(&mut buf, OBJ + HDR, TaggedValue::fixnum(201));
    put_word(&mut buf, OBJ + HDR + 8, TaggedValue::fixnum(202));
    for (index, value) in [301, 302, 303].into_iter().enumerate() {
        put_word(&mut buf, CONST_OFF + index * 8, TaggedValue::fixnum(value));
    }

    assert_eq!(
        walk(&buf, EXTRAS_LEN),
        vec![101, 102, 103, 104, 201, 202, 301, 302, 303],
        "metadata words, extra slots, then constants, each in image order"
    );

    // A flagless header with no extra slots or constants emits nothing:
    // absent metadata flags must not read their (here stale) words.
    header.flags = 0;
    header.n_extra_slots = 0;
    header.const_count = 0;
    write_header(&mut buf, &header);
    assert!(walk(&buf, EXTRAS_LEN).is_empty());
}

#[test]
fn stub_child_walk_refuses_a_short_extras_span() {
    let mut buf = vec![0u8; OBJ + EXTRAS_LEN];
    let mut header: BytecodeExtras = bytemuck::Zeroable::zeroed();
    header.flags = BC_FLAG_HAS_ARGLIST;
    header.arglist_word = TaggedValue::fixnum(101).bits() as u64;
    write_header(&mut buf, &header);

    assert!(
        walk(&buf, HDR - 1).is_empty(),
        "a span shorter than the header must emit no children"
    );
}

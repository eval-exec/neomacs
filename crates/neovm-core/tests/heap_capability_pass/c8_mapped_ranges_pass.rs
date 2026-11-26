//! Positive control for value-gc C8: `unsafe` construction plus safe getters
//! and mark setters on `MappedConsRange`, `MappedFloatRange`,
//! `MappedVecLikeObject`, and `MappedStringObject` compile cleanly.
#![allow(dead_code)]

const CONS_MARK_BITS_PER_WORD: usize = usize::BITS as usize;
fn cons_mark_words(len: usize) -> usize {
    len.div_ceil(CONS_MARK_BITS_PER_WORD)
}
struct MarkBit {
    word_index: usize,
    mask: usize,
}
struct ConsBlock;
impl ConsBlock {
    fn mark_bit(index: usize) -> MarkBit {
        MarkBit {
            word_index: index / CONS_MARK_BITS_PER_WORD,
            mask: 1usize << (index % CONS_MARK_BITS_PER_WORD),
        }
    }
}
struct ConsCell([usize; 2]);
struct FloatObj([usize; 4]);
struct VecLikeHeader([usize; 2]);
struct StringObj([usize; 4]);

#[path = "../../../neovm-value/src/tagged/gc/mapped_ranges.rs"]
mod mapped_ranges;

fn main() {
    // SAFETY: compile-only positive control with empty/never-dereferenced descriptors.
    let mut cons = unsafe { mapped_ranges::MappedConsRange::new(std::ptr::null_mut(), 0) };
    let mut float = unsafe { mapped_ranges::MappedFloatRange::new(std::ptr::null_mut(), 0) };
    let mut veclike = unsafe { mapped_ranges::MappedVecLikeObject::new(std::ptr::null_mut(), 0) };
    let mut string = unsafe { mapped_ranges::MappedStringObject::new(std::ptr::null_mut(), 0) };

    assert!(cons.start().is_null());
    assert_eq!(cons.len(), 0);
    cons.clear_marks();

    assert_eq!(float.len(), 0);
    float.clear_marks();

    assert!(veclike.header().is_null());
    assert_eq!(veclike.byte_len(), 0);
    assert!(!veclike.is_marked());
    veclike.set_marked(true);
    assert!(veclike.is_marked());

    assert!(string.ptr().is_null());
    assert_eq!(string.byte_len(), 0);
    assert!(!string.is_marked());
    string.set_marked(true);
    assert!(string.is_marked());
}

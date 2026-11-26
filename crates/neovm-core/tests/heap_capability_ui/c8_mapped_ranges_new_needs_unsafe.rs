//! value-gc C8 regression guard: constructing `MappedConsRange`,
//! `MappedFloatRange`, `MappedVecLikeObject`, or `MappedStringObject` from raw
//! pointers requires `unsafe`.
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
    let _ = mapped_ranges::MappedConsRange::new(std::ptr::null_mut(), 0);
    let _ = mapped_ranges::MappedFloatRange::new(std::ptr::null_mut(), 0);
    let _ = mapped_ranges::MappedVecLikeObject::new(std::ptr::null_mut(), 0);
    let _ = mapped_ranges::MappedStringObject::new(std::ptr::null_mut(), 0);
}

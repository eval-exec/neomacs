//! Invalid class bits must never masquerade as an empty heap granule.
use super::*;

#[test]
#[should_panic(expected = "invalid chunk class code 31 in sealed map")]
fn corrupt_chunk_class_is_not_an_empty_granule() {
    let corrupt = ChunkEntry(ChunkEntry::CLASS_MASK);
    let _ = corrupt.class();
}

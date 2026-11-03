//! Explicit immutable descriptor offsets, never a Box or slice representation.
//! The single descriptor obtained through publication supplies every bound and
//! pointer. Reading these fields does not authorize a mutable symbol-cell read.
use super::{OwnedChunkAddresses, PublishedSymbolTable, SymbolChunkAddress, SymbolTableShape};
use std::mem::{align_of, offset_of, size_of};

/// A byte position in the one immutable descriptor, not a slot or chunk index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DescriptorOffset(usize);
impl DescriptorOffset {
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}
/// An array-entry stride, kept distinct from a descriptor field position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AddressStride(usize);
impl AddressStride {
    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

pub(crate) const TABLE_SLOT_COUNT_OFFSET: DescriptorOffset =
    DescriptorOffset(offset_of!(PublishedSymbolTable, shape) + offset_of!(SymbolTableShape, slots));
pub(crate) const TABLE_CHUNK_COUNT_OFFSET: DescriptorOffset = DescriptorOffset(
    offset_of!(PublishedSymbolTable, shape) + offset_of!(SymbolTableShape, chunks),
);
pub(crate) const TABLE_CHUNK_SLOTS_OFFSET: DescriptorOffset = DescriptorOffset(
    offset_of!(PublishedSymbolTable, shape) + offset_of!(SymbolTableShape, chunk_slots),
);
pub(crate) const TABLE_ADDRESSES_OFFSET: DescriptorOffset = DescriptorOffset(
    offset_of!(PublishedSymbolTable, chunks) + offset_of!(OwnedChunkAddresses, root),
);
pub(crate) const TABLE_ADDRESS_STRIDE: AddressStride =
    AddressStride(size_of::<SymbolChunkAddress>());

const _: () = assert!(TABLE_ADDRESS_STRIDE.get() > 0);
const _: () = assert!(size_of::<SymbolChunkAddress>() == size_of::<*mut ()>());
const _: () = assert!(align_of::<SymbolChunkAddress>() == align_of::<*mut ()>());

#[cfg(test)]
#[path = "abi/tests/abi_test.rs"]
mod tests;

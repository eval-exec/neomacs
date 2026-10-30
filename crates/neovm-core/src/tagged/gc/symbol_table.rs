//! Immutable, separately owned symbol pointer tables for a parallel era.
//!
//! Tables own their descriptor and address-array allocations, never the chunk
//! allocations named by entries. No reader is exposed before initialization.
//! The era integration must retain the backing and every published table until
//! its terminal or grace proof; these types do not mint that proof.
pub(crate) mod publication;

use std::mem::MaybeUninit;
use std::num::{NonZeroU64, NonZeroUsize};
use std::ptr::NonNull;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableError {
    Capacity,
    Shape,
    GenerationExhausted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct SymbolSlotCount(usize);
impl From<usize> for SymbolSlotCount {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
impl SymbolSlotCount {
    pub(crate) fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct SymbolChunkCount(usize);
impl From<usize> for SymbolChunkCount {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
impl SymbolChunkCount {
    pub(crate) fn get(self) -> usize {
        self.0
    }
}

/// The caller supplies the backing's one existing chunk-size definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct SymbolChunkSlots(NonZeroUsize);
impl From<NonZeroUsize> for SymbolChunkSlots {
    fn from(value: NonZeroUsize) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct SymbolChunkIndex(usize);
impl From<usize> for SymbolChunkIndex {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// This clock is neither a registry epoch, function epoch nor stop receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct PointerTableGeneration(NonZeroU64);
impl PointerTableGeneration {
    pub(crate) const INITIAL: Self = Self(NonZeroU64::MIN);

    pub(crate) fn checked_successor(self) -> Result<Self, TableError> {
        self.0
            .get()
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .map(Self)
            .ok_or(TableError::GenerationExhausted)
    }
}

/// A nonowning provenance-preserving allocation address, not a Lisp value.
/// Copying it never copies ownership or creates a reference to chunk contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct SymbolChunkAddress(NonNull<()>);
impl SymbolChunkAddress {
    /// # Safety
    /// The retained backing owns this chunk allocation. It must outlive every
    /// published table/view that can return this address. The caller derives
    /// it from the allocation root, never from an integer or a movable spine.
    pub(crate) unsafe fn from_allocation(root: NonNull<()>) -> Self {
        Self(root)
    }

    pub(crate) fn as_ptr(self) -> *mut () {
        self.0.as_ptr()
    }
}

/// One checked relationship between logical coverage and pointer-array size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SymbolTableShape {
    slots: SymbolSlotCount,
    chunks: SymbolChunkCount,
    chunk_slots: SymbolChunkSlots,
}
impl SymbolTableShape {
    pub(crate) fn new(
        slots: SymbolSlotCount,
        chunks: SymbolChunkCount,
        chunk_slots: SymbolChunkSlots,
    ) -> Result<Self, TableError> {
        let covered = chunks
            .0
            .checked_mul(chunk_slots.0.get())
            .ok_or(TableError::Capacity)?;
        if slots.0 != covered {
            return Err(TableError::Shape);
        }
        Ok(Self {
            slots,
            chunks,
            chunk_slots,
        })
    }
}

/// Fully initialized immutable payload. Mutable symbol cells remain protected
/// by their separate symbol authority; a table is not a collector root.
pub(crate) struct PublishedSymbolTable {
    shape: SymbolTableShape,
    generation: PointerTableGeneration,
    chunks: Box<[SymbolChunkAddress]>,
}
impl PublishedSymbolTable {
    pub(crate) fn slots(&self) -> SymbolSlotCount {
        self.shape.slots
    }

    pub(crate) fn chunk_count(&self) -> SymbolChunkCount {
        self.shape.chunks
    }

    pub(crate) fn generation(&self) -> PointerTableGeneration {
        self.generation
    }

    pub(crate) fn chunk(&self, index: SymbolChunkIndex) -> Option<SymbolChunkAddress> {
        self.chunks.get(index.0).copied()
    }
}

/// All allocation happens before a World publication transaction starts.
/// The descriptor and address-array boxes already have their final sizes.
#[must_use = "initialize before publication, or drop the unexposed preparation"]
pub(crate) struct PreparedSymbolTable {
    shape: SymbolTableShape,
    generation: PointerTableGeneration,
    descriptor: Box<MaybeUninit<PublishedSymbolTable>>,
    chunks: Box<[MaybeUninit<SymbolChunkAddress>]>,
}
impl PreparedSymbolTable {
    pub(crate) fn new(
        shape: SymbolTableShape,
        generation: PointerTableGeneration,
    ) -> Result<Self, TableError> {
        let mut chunks = Vec::new();
        chunks
            .try_reserve_exact(shape.chunks.0)
            .map_err(|_| TableError::Capacity)?;
        chunks.resize_with(shape.chunks.0, MaybeUninit::uninit);
        // Any shrink by into_boxed_slice and descriptor allocation occur NOW,
        // before the caller may publish backing slots. Like existing detached
        // chunks, allocator abort is not claimed as recoverable refusal.
        let chunks = chunks.into_boxed_slice();
        Ok(Self {
            shape,
            generation,
            descriptor: Box::new_uninit(),
            chunks,
        })
    }

    /// Finish using addresses derived after chunks reach their owning backing.
    /// There is no reserve, shrink or descriptor allocation on this path.
    pub(crate) fn initialize(
        mut self,
        addresses: impl IntoIterator<Item = SymbolChunkAddress>,
    ) -> Result<OwnedSymbolTable, TableError> {
        let mut addresses = addresses.into_iter();
        for slot in &mut self.chunks {
            let address = addresses.next().ok_or(TableError::Shape)?;
            slot.write(address);
        }
        if addresses.next().is_some() {
            return Err(TableError::Shape);
        }
        // SAFETY: the loop initialized every element exactly once. No table
        // or address-array reference escaped this unpublished preparation.
        let chunks = unsafe { self.chunks.assume_init() };
        self.descriptor.write(PublishedSymbolTable {
            shape: self.shape,
            generation: self.generation,
            chunks,
        });
        // SAFETY: write initialized the entire descriptor, and no reader can
        // have observed the uninitialized allocation through this API.
        let descriptor = unsafe { self.descriptor.assume_init() };
        // Derive the ONE ownership root only after the owning Box is consumed.
        // Moving the returned handle never retags/moves the descriptor itself.
        let root = Box::into_raw(descriptor);
        Ok(OwnedSymbolTable {
            // SAFETY: Box::into_raw returns a nonnull live allocation root.
            root: unsafe { NonNull::new_unchecked(root) },
        })
    }
}

/// Unique ownership of a descriptor, independent of the publication pointer.
/// It deliberately supplies no ownership conversion for any chunk entry.
#[must_use = "retain ownership before publishing this table's address"]
pub(crate) struct OwnedSymbolTable {
    root: NonNull<PublishedSymbolTable>,
}
impl OwnedSymbolTable {
    pub(crate) fn table(&self) -> &PublishedSymbolTable {
        // SAFETY: this handle uniquely retains the initialized allocation.
        // It never exposes mutable access, and the borrow excludes its Drop.
        unsafe { self.root.as_ref() }
    }

    pub(crate) fn as_ptr(&self) -> *mut PublishedSymbolTable {
        self.root.as_ptr()
    }
}
impl Drop for OwnedSymbolTable {
    fn drop(&mut self) {
        // SAFETY: initialize consumed exactly one Box into this unique root.
        // No other owner can be manufactured through the API. This drops only
        // the descriptor/address array; chunk addresses are nonowning values.
        unsafe { drop(Box::from_raw(self.root.as_ptr())) };
    }
}

static_assertions::assert_not_impl_any!(PreparedSymbolTable: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(OwnedSymbolTable: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(PublishedSymbolTable: Send, Sync, Clone, Copy);
static_assertions::assert_eq_size!(SymbolChunkAddress, usize);
static_assertions::assert_eq_size!(PointerTableGeneration, u64);

#[cfg(test)]
mod tests;

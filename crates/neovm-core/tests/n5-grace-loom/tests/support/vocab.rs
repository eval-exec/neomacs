//! The named stand-in for `symbol_table.rs`'s descriptor vocabulary (N5
//! design rev 2 section 2.2, "the named stand-in").
//!
//! `publication.rs` draws these seven names from `super::`, but the file
//! that defines them also carries the obarray runtime (chunk boxes, spine,
//! `LispSymbol`) and cannot be included wholesale in a model. This module
//! reproduces that descriptor half field-for-field and method-for-method
//! from the definitions at `src/tagged/gc/symbol_table.rs:13-261` (read at
//! the pinned main; the mirroring is mechanical). It is safe because the
//! ledger never dereferences chunk entries — `SymbolChunkAddress` values
//! are nonowning addresses, per the SAFETY comments on the ledger's own
//! `Send`/`Sync` impls — so the modeled invariants (retention, publication
//! ordering, refusal) never observe the difference.
//!
//! Drift control: the real `publication.rs` compiles inside this model
//! against these definitions, so any ledger-side API drift breaks the
//! model's build. The residual — a layout change in this half alone — is
//! named in the design and accepted. Visibilities are widened to `pub` for
//! the model crate's module layout; nothing here is reachable outside the
//! model workspace.
use std::num::{NonZeroU64, NonZeroUsize};
use std::ptr::NonNull;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableError {
    Capacity,
    Shape,
    GenerationExhausted,
}

// The tuple fields the ledger reads directly (`shape.chunks.0`,
// `shape.slots.0`, publication.rs:293-294) are `pub` here: the real file
// reaches them through child-module visibility, which the model's layout
// cannot reproduce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SymbolSlotCount(pub usize);
impl From<usize> for SymbolSlotCount {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
impl SymbolSlotCount {
    pub const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SymbolChunkCount(pub usize);
impl From<usize> for SymbolChunkCount {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
impl SymbolChunkCount {
    pub const fn get(self) -> usize {
        self.0
    }
}

/// The caller supplies the backing's one existing chunk-size definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SymbolChunkSlots(NonZeroUsize);
impl SymbolChunkSlots {
    pub const fn get(self) -> usize {
        self.0.get()
    }
}
impl From<NonZeroUsize> for SymbolChunkSlots {
    fn from(value: NonZeroUsize) -> Self {
        Self(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct SymbolChunkIndex(usize);
impl From<usize> for SymbolChunkIndex {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// This clock is neither a registry epoch, function epoch nor stop receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct PointerTableGeneration(NonZeroU64);
impl PointerTableGeneration {
    pub const INITIAL: Self = Self(NonZeroU64::MIN);

    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub fn checked_successor(self) -> Result<Self, TableError> {
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
pub struct SymbolChunkAddress(NonNull<()>);
impl SymbolChunkAddress {
    /// # Safety
    /// The retained backing owns this chunk allocation. It must outlive every
    /// published table/view that can return this address. The caller derives
    /// it from the allocation root, never from an integer or a movable spine.
    pub unsafe fn from_allocation(root: NonNull<()>) -> Self {
        Self(root)
    }

    pub fn as_ptr(self) -> *mut () {
        self.0.as_ptr()
    }
}

/// One checked relationship between logical coverage and pointer-array size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymbolTableShape {
    pub slots: SymbolSlotCount,
    pub chunks: SymbolChunkCount,
    pub chunk_slots: SymbolChunkSlots,
}
impl SymbolTableShape {
    pub fn new(
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

    pub fn chunk_count(&self) -> SymbolChunkCount {
        self.chunks
    }
}

/// Fully initialized immutable payload. Mutable symbol cells remain protected
/// by their separate symbol authority; a table is not a collector root.
pub struct PublishedSymbolTable {
    pub shape: SymbolTableShape,
    pub generation: PointerTableGeneration,
    pub chunks: Box<[SymbolChunkAddress]>,
}
impl PublishedSymbolTable {
    pub fn slots(&self) -> SymbolSlotCount {
        self.shape.slots
    }

    pub fn chunk_count(&self) -> SymbolChunkCount {
        self.shape.chunks
    }

    pub fn chunk_slots(&self) -> SymbolChunkSlots {
        self.shape.chunk_slots
    }

    pub fn generation(&self) -> PointerTableGeneration {
        self.generation
    }

    pub fn chunk(&self, index: SymbolChunkIndex) -> Option<SymbolChunkAddress> {
        self.chunks.get(index.0).copied()
    }
}

/// All allocation happens before a World publication transaction starts.
/// The descriptor and address-array boxes already have their final sizes.
#[must_use = "initialize before publication, or drop the unexposed preparation"]
pub struct PreparedSymbolTable {
    shape: SymbolTableShape,
    generation: PointerTableGeneration,
    descriptor: Box<std::mem::MaybeUninit<PublishedSymbolTable>>,
    chunks: Box<[std::mem::MaybeUninit<SymbolChunkAddress>]>,
}
impl PreparedSymbolTable {
    pub fn new(
        shape: SymbolTableShape,
        generation: PointerTableGeneration,
    ) -> Result<Self, TableError> {
        let mut chunks = Vec::new();
        chunks
            .try_reserve_exact(shape.chunks.0)
            .map_err(|_| TableError::Capacity)?;
        chunks.resize_with(shape.chunks.0, std::mem::MaybeUninit::uninit);
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
    pub fn initialize(
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
pub struct OwnedSymbolTable {
    root: NonNull<PublishedSymbolTable>,
}
impl OwnedSymbolTable {
    pub fn table(&self) -> &PublishedSymbolTable {
        // SAFETY: this handle uniquely retains the initialized allocation.
        // It never exposes mutable access, and the borrow excludes its Drop.
        unsafe { self.root.as_ref() }
    }

    pub fn as_ptr(&self) -> *mut PublishedSymbolTable {
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

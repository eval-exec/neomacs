//! Writer/admission bridge into the lower immutable pointer-table vocabulary.
use super::SymbolBacking;
use crate::tagged::gc::symbol_table::publication::{PreparedTablePublication, PublicationChange};
use crate::tagged::gc::symbol_table::{
    OwnedSymbolTable, PointerTableGeneration, PreparedSymbolTable, SymbolChunkAddress,
    SymbolChunkCount, SymbolChunkSlots, SymbolSlotCount, SymbolTableBacking, SymbolTableShape,
    publication_error, table_error,
};
use crate::tagged::gc::{RegistrationError, SymbolWriteToken};
use std::num::NonZeroUsize;
use std::ptr::NonNull;

impl SymbolBacking {
    /// # Safety
    /// Exclusive admission or this backing's WRITER excludes structural edits.
    unsafe fn table_shape(
        &self,
        minimum: SymbolSlotCount,
    ) -> Result<SymbolTableShape, RegistrationError> {
        // SAFETY: caller retains the backing and excludes its structural writer.
        let symbols = unsafe { &(*self.obarray.as_ptr()).symbols };
        // SAFETY: same admission/writer exclusion covers the generation read.
        if unsafe { (*self.obarray.as_ptr()).generation } != self.generation
            || symbols.chunks.len() != symbols.sides.len()
        {
            return Err(RegistrationError::StaleRegistration);
        }
        let span =
            NonZeroUsize::new(super::super::OBARRAY_CHUNK).ok_or(RegistrationError::Publication)?;
        let current = symbols
            .chunks
            .len()
            .checked_mul(span.get())
            .ok_or(RegistrationError::Capacity)?;
        if current != symbols.len {
            return Err(RegistrationError::Publication);
        }
        let required = minimum
            .get()
            .checked_add(span.get() - 1)
            .ok_or(RegistrationError::Capacity)?
            / span.get();
        let chunks = symbols.chunks.len().max(required);
        let slots = chunks
            .checked_mul(span.get())
            .ok_or(RegistrationError::Capacity)?;
        SymbolTableShape::new(
            SymbolSlotCount::from(slots),
            SymbolChunkCount::from(chunks),
            SymbolChunkSlots::from(span),
        )
        .map_err(table_error)
    }

    /// # Safety
    /// The retained backing owns every chunk; admission or WRITER excludes
    /// structural changes for the returned iterator's entire short borrow.
    unsafe fn table_addresses(&self) -> impl Iterator<Item = SymbolChunkAddress> + '_ {
        // SAFETY: original root, with the matching exclusion held by the caller.
        let symbols = unsafe { &(*self.obarray.as_ptr()).symbols };
        symbols.chunks.iter().map(|chunk| {
            // Raw place export avoids creating an immutable array/symbol loan.
            let root = (&raw const **chunk).cast_mut().cast::<()>();
            // SAFETY: a live, nonempty uniquely owned chunk allocation. These
            // are its payload addresses, never addresses of Vec/Box handles.
            unsafe { SymbolChunkAddress::from_allocation(NonNull::new_unchecked(root)) }
        })
    }
}

// SAFETY: the era retains this original backing (including on abandonment).
// Calls below require exclusive admission or this exact backing's writer;
// pointer entries are nonowning and no array or symbol reference escapes.
unsafe impl SymbolTableBacking for SymbolBacking {
    #[cfg(test)]
    unsafe fn arm_initial_table_refusal_for_test(&self) {
        self.intern_fault.store(
            super::InitialInternFault::InitialTablePreparation as u8,
            std::sync::atomic::Ordering::Release,
        );
    }

    unsafe fn prepare_initial_symbol_table(&self) -> Result<OwnedSymbolTable, RegistrationError> {
        #[cfg(test)]
        if self
            .intern_fault
            .compare_exchange(
                super::InitialInternFault::InitialTablePreparation as u8,
                0,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
        {
            return Err(RegistrationError::Capacity);
        }
        // SAFETY: caller holds exclusive admission before participants exist.
        let shape = unsafe { self.table_shape(SymbolSlotCount::from(0)) }?;
        let prepared = PreparedSymbolTable::new(shape, PointerTableGeneration::INITIAL)
            .map_err(table_error)?;
        // SAFETY: same exclusive admission retains the owning chunk allocations.
        prepared
            .initialize(unsafe { self.table_addresses() })
            .map_err(table_error)
    }

    unsafe fn symbol_table_shape(
        &self,
        minimum: SymbolSlotCount,
        _authority: &SymbolWriteToken<'_>,
    ) -> Result<SymbolTableShape, RegistrationError> {
        // SAFETY: the matching writer token excludes structural edits.
        unsafe { self.table_shape(minimum) }
    }

    unsafe fn publish_symbol_table(
        &self,
        prepared: PreparedTablePublication<'_>,
        _authority: &SymbolWriteToken<'_>,
    ) -> Result<PublicationChange, RegistrationError> {
        // SAFETY: original backing is retained and the writer still spans
        // cell, registry and table publication; no mutable slot loan remains.
        prepared
            .publish(unsafe { self.table_addresses() })
            .map_err(publication_error)
    }
}

#[cfg(test)]
impl super::super::Obarray {
    pub(crate) fn prepare_symbol_table_slots_for_test(&mut self, minimum: SymbolSlotCount) {
        if let Some(last) = minimum.get().checked_sub(1) {
            self.symbols.grow_for(last);
        }
    }
}

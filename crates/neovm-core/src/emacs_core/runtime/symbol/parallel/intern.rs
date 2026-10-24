//! Prepared initial membership on retained World roots, without serial hooks.
use super::super::{LispSymbol, OBARRAY_CHUNK, SymbolChunks, fn_stamps};
use super::*;
use crate::emacs_core::intern::parallel::ExistingCanonicalSymbol;
use crate::tagged::gc::{RegistrationError, SymbolWriteToken};
use std::mem::size_of;
use std::ptr::addr_of_mut;

/// Per-World test injection, absent from production types and instructions.
#[cfg(test)]
#[derive(Clone, Copy)]
#[repr(u8)]
pub(crate) enum InitialInternFault {
    ReservationAfterSpineMove = 1,
    AfterSlotPublication = 2,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct InitialInternFacts {
    pub(crate) members: usize,
    pub(crate) epoch: u64,
    pub(crate) slots: usize,
    pub(crate) chunks: usize,
    pub(crate) sides: usize,
    pub(crate) bytes: usize,
    pub(crate) spine_current: bool,
    pub(crate) spine_address: usize,
    pub(crate) present: bool,
    pub(crate) member: bool,
    pub(crate) cells: Option<(usize, usize, bool, bool)>,
    pub(crate) backing: Option<PlainSymbolBacking>,
    pub(crate) projections: usize,
    /// Descriptor address, dense slot and canonical word; scalar test facts.
    pub(crate) projection: Option<(usize, usize, usize)>,
    pub(crate) shared_void: Option<(usize, usize)>,
    pub(crate) projection_bytes: usize,
}

#[cfg(test)]
impl Obarray {
    pub(crate) fn prepare_initial_nonmember_for_test(&mut self, id: SymId, nonmember: bool) {
        self.seed_value_id(id, super::super::Value::fixnum(73));
        let symbol = self.ensure_symbol_id(id);
        symbol
            .function
            .store_atomic(super::super::Value::fixnum(31));
        symbol.plist.store_atomic(super::super::Value::fixnum(47));
        symbol.set_declared_special(true);
        if nonmember {
            assert!(self.clear_global_member(id));
        }
    }
}

// Known retained payload, matching the GC's ordinary Rust-storage charges.
// Allocator/control-block headers and detached temporary staging are excluded.
fn storage_bytes(symbols: &SymbolChunks) -> usize {
    symbols
        .chunks
        .len()
        .saturating_mul(
            size_of::<[LispSymbol; OBARRAY_CHUNK]>() + size_of::<fn_stamps::ChunkSide>(),
        )
        .saturating_add(symbols.chunks.capacity().saturating_mul(size_of::<usize>()))
        .saturating_add(symbols.sides.capacity().saturating_mul(size_of::<usize>()))
}

struct PreparedChunks {
    chunks: Vec<Box<[LispSymbol; OBARRAY_CHUNK]>>,
    sides: Vec<Box<fn_stamps::ChunkSide>>,
}
impl PreparedChunks {
    fn new(count: usize, floor: u64) -> Result<Self, RegistrationError> {
        let mut prepared = Self {
            chunks: Vec::new(),
            sides: Vec::new(),
        };
        prepared
            .chunks
            .try_reserve_exact(count)
            .map_err(|_| RegistrationError::Capacity)?;
        prepared
            .sides
            .try_reserve_exact(count)
            .map_err(|_| RegistrationError::Capacity)?;
        for _ in 0..count {
            let mut chunk = Vec::new();
            chunk
                .try_reserve_exact(OBARRAY_CHUNK)
                .map_err(|_| RegistrationError::Capacity)?;
            chunk.resize_with(OBARRAY_CHUNK, LispSymbol::empty);
            let chunk: Box<[LispSymbol; OBARRAY_CHUNK]> = chunk
                .into_boxed_slice()
                .try_into()
                .map_err(|_| RegistrationError::Publication)?;
            prepared.chunks.push(chunk);
            // These are ordinary Rust allocations, before live publication.
            // Global-allocator abort is not claimed as a recoverable refusal.
            prepared
                .sides
                .push(Box::new(fn_stamps::ChunkSide::new(floor)));
        }
        Ok(prepared)
    }
}

impl SymbolBacking {
    /// # Safety
    /// The test owns this World's matching writer, with no operation active.
    #[cfg(test)]
    pub(crate) unsafe fn arm_initial_fault_for_test(
        &self,
        fault: InitialInternFault,
        _authority: &SymbolWriteToken<'_>,
    ) {
        self.intern_fault_receipt
            .store(0, std::sync::atomic::Ordering::Release);
        self.intern_fault
            .store(fault as u8, std::sync::atomic::Ordering::Release);
    }

    /// Scalar diagnostics only, never a World view after a poisoned commit.
    #[cfg(test)]
    pub(crate) fn initial_fault_receipt_for_test(&self) -> u8 {
        self.intern_fault_receipt
            .load(std::sync::atomic::Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn record_initial_poison_for_test(&self, writer_held: bool) {
        if writer_held
            && self.initial_fault_receipt_for_test()
                == InitialInternFault::AfterSlotPublication as u8
        {
            self.intern_fault_receipt
                .fetch_or(128, std::sync::atomic::Ordering::Release);
        }
    }

    /// # Safety
    /// Test assertions only, under this actual World's exclusive writer.
    #[cfg(test)]
    pub(crate) unsafe fn initial_facts_for_test(
        &self,
        id: SymId,
        _authority: &SymbolWriteToken<'_>,
    ) -> Result<InitialInternFacts, RegistrationError> {
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation {
            return Err(RegistrationError::Publication);
        }
        let symbol = obarray.get_by_id(id);
        let table = unsafe { self.projections.as_ref() }
            .try_borrow()
            .map_err(|_| RegistrationError::Busy)?;
        let projection = symbol.and_then(|symbol| match symbol.value_cell() {
            ValueCell::Forwarded(fwd) => table.projection(fwd).and_then(|projection| {
                (projection.arm() == CanonicalArm::Plain && !projection.host_projected()).then(
                    || {
                        (
                            projection as *const _ as usize,
                            projection.index().slot(),
                            projection.plain_value().bits(),
                        )
                    },
                )
            }),
            _ => None,
        });
        let shared_void = table
            .parallel_void_header(_authority)
            .and_then(|fwd| table.projection(fwd))
            .map(|projection| {
                (
                    projection as *const _ as usize,
                    projection.plain_value().bits(),
                )
            });
        Ok(InitialInternFacts {
            members: obarray.global_member_count,
            epoch: obarray.members_epoch,
            slots: obarray.symbols.len,
            chunks: obarray.symbols.chunks.len(),
            sides: obarray.symbols.sides.len(),
            bytes: storage_bytes(&obarray.symbols),
            spine_current: obarray.symbols.spine_addr == obarray.symbols.chunks.as_ptr() as usize,
            spine_address: obarray.symbols.spine_addr,
            present: symbol.is_some(),
            member: obarray.is_global_member(id),
            cells: symbol.map(|s| {
                (
                    s.function.get().bits(),
                    s.plist.get().bits(),
                    s.function_unbound,
                    s.flags().declared_special(),
                )
            }),
            backing: unsafe { self.classify(id) },
            projections: table.len(),
            projection,
            shared_void,
            projection_bytes: table.parallel_storage_bytes(_authority),
        })
    }

    /// # Safety
    /// The matching writer excludes structural mutation and covers the table
    /// borrow in classify. Nonmember and unsupported-arm roots are refused.
    pub(crate) unsafe fn classify_initial_member(
        &self,
        id: SymId,
        _authority: &SymbolWriteToken<'_>,
    ) -> Option<PlainSymbolBacking> {
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation || !obarray.is_global_member(id) {
            return None;
        }
        unsafe { self.classify(id) }
    }

    /// Reserve every required chunk/side pair, including real registry-ID
    /// gaps, before publishing the target. Existing boxes never move.
    ///
    /// # Safety
    /// Matching WRITER excludes all World readers/legacy views/markers; the
    /// caller retains the process registry guard that supplied metadata.
    pub(crate) unsafe fn ensure_initial_member(
        &self,
        metadata: ExistingCanonicalSymbol,
        authority: &SymbolWriteToken<'_>,
    ) -> (Result<(), RegistrationError>, usize) {
        let root = self.obarray.as_ptr();
        // Original root subfields only: do not exclusively reborrow dispatch,
        // BLV/forwarder roots, completion state or the whole Obarray.
        let symbols = unsafe { addr_of_mut!((*root).symbols) };
        let before = storage_bytes(unsafe { &*symbols });
        let result = unsafe { self.commit_initial_member(metadata, authority) };
        let grown = storage_bytes(unsafe { &*symbols }).saturating_sub(before);
        (result, grown)
    }

    unsafe fn commit_initial_member(
        &self,
        metadata: ExistingCanonicalSymbol,
        authority: &SymbolWriteToken<'_>,
    ) -> Result<(), RegistrationError> {
        let root = self.obarray.as_ptr();
        if unsafe { (*root).generation } != self.generation
            || metadata.name == super::super::SYMBOL_NAME_SENTINEL
        {
            return Err(RegistrationError::Publication);
        }
        let index = metadata.id.0 as usize;
        #[cfg(test)]
        let fault = self
            .intern_fault
            .swap(0, std::sync::atomic::Ordering::AcqRel);
        let symbols = unsafe { &mut *addr_of_mut!((*root).symbols) };
        if let Some(symbol) = symbols.get(index) {
            if symbol.name() != metadata.name {
                return Err(RegistrationError::Publication);
            }
            if symbol.is_interned_global() {
                return Ok(());
            }
        }
        let count = unsafe { (*root).global_member_count }
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        let epoch = unsafe { (*root).members_epoch }
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        let void = unsafe { self.projections.as_ref() }
            .try_borrow()
            .map_err(|_| RegistrationError::Busy)?
            .parallel_void_header(authority);
        let required = (index / OBARRAY_CHUNK)
            .checked_add(1)
            .ok_or(RegistrationError::Capacity)?;
        if symbols.chunks.len() != symbols.sides.len()
            || symbols.len != symbols.chunks.len().saturating_mul(OBARRAY_CHUNK)
        {
            return Err(RegistrationError::Publication);
        }
        let added = required.saturating_sub(symbols.chunks.len());
        let final_len = symbols
            .chunks
            .len()
            .max(required)
            .checked_mul(OBARRAY_CHUNK)
            .ok_or(RegistrationError::Capacity)?;
        // Capacity-only changes are retained and charged even if a subsequent
        // reservation refuses; logical length/membership remain untouched.
        #[cfg(test)]
        if fault == InitialInternFault::ReservationAfterSpineMove as u8 {
            // Force a larger reservation at this allocation seam. The old
            // buffer remains allocated while its replacement reserves, so a
            // distinct address and retained growth are certain. The test is
            // independent of malloc extending in place; only Box handles move.
            let mut replacement = Vec::new();
            let capacity = symbols
                .chunks
                .len()
                .checked_add(added)
                .and_then(|required| {
                    symbols
                        .chunks
                        .capacity()
                        .checked_add(1)
                        .map(|grown| required.max(grown))
                })
                .ok_or(RegistrationError::Capacity)?;
            replacement
                .try_reserve_exact(capacity)
                .map_err(|_| RegistrationError::Capacity)?;
            replacement.append(&mut symbols.chunks);
            symbols.chunks = replacement;
        }
        symbols
            .chunks
            .try_reserve(added)
            .map_err(|_| RegistrationError::Capacity)?;
        // A successful reservation can move the existing pointer spine even
        // if a later preparation refuses. Keep the old logical slots usable
        // on that clean failure path as well as on the final publication path.
        symbols.spine_addr = symbols.chunks.as_ptr() as usize;
        #[cfg(test)]
        if fault == InitialInternFault::ReservationAfterSpineMove as u8 {
            self.intern_fault_receipt
                .store(fault, std::sync::atomic::Ordering::Release);
            return Err(RegistrationError::Capacity);
        }
        symbols
            .sides
            .try_reserve(added)
            .map_err(|_| RegistrationError::Capacity)?;
        let floor = symbols.sides.first().map_or(0, |side| side.floor());
        let prepared = PreparedChunks::new(added, floor)?;
        // All fallible preparation ended. Any unexpected refusal below this
        // point is sticky poison before the outer WRITER is released.
        symbols.chunks.extend(prepared.chunks);
        symbols.sides.extend(prepared.sides);
        symbols.spine_addr = symbols.chunks.as_ptr() as usize;
        symbols.len = final_len;
        let slot = symbols
            .chunks
            .get_mut(index / OBARRAY_CHUNK)
            .and_then(|chunk| chunk.get_mut(index & (OBARRAY_CHUNK - 1)))
            .ok_or(RegistrationError::Poisoned)?;
        if slot.is_present() {
            slot.parallel_mark_initial(authority);
        } else {
            slot.publish_fill(LispSymbol::parallel_initial_member(
                metadata.name,
                void,
                authority,
            ));
        }
        #[cfg(test)]
        if fault == InitialInternFault::AfterSlotPublication as u8 {
            // Target presence/membership is now live, but its counters and
            // the destination are deliberately not committed. Resume is
            // forbidden: the operator must publish sticky poison under WRITER.
            self.intern_fault_receipt
                .store(fault, std::sync::atomic::Ordering::Release);
            return Err(RegistrationError::Poisoned);
        }
        unsafe {
            (*root).global_member_count = count;
            (*root).members_epoch = epoch;
        }
        Ok(())
    }
}

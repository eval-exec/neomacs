//! Generative, owner-confined execution slices. Validation belongs to entry;
//! the resident primitive touches only borrowed root homes and an exclusive run.
use super::super::ConsCell;
use super::super::mark_word::MarkWord;
use super::cons_allocation::{ConsRunState, initialize_next_cons};
use super::*;
use crate::tagged::value::{TAG_MASK, TaggedValue};
use std::ptr::NonNull;

type Brand<'brand> = PhantomData<fn(&'brand ()) -> &'brand ()>;

fn slot_value(root: &SliceRoot<'_>) -> TaggedValue {
    // SAFETY: bind minted this home against the live bank. The slice borrow
    // keeps that bank resident, so the word is initialized.
    unsafe { root.word.as_ptr().read().value() }
}

fn write_slot(root: &mut SliceRoot<'_>, value: TaggedValue) {
    unsafe { root.word.as_ptr().write(MarkWord::of(value)) };
}

fn cons_ptr(owner: TaggedValue) -> Result<*mut ConsCell, RegistrationError> {
    if !owner.is_cons() {
        return Err(RegistrationError::Publication);
    }
    let ptr = (owner.bits() & !TAG_MASK) as *mut ConsCell;
    if ptr.is_null() || (ptr as usize) & TAG_MASK != 0 {
        return Err(RegistrationError::Publication);
    }
    Ok(ptr)
}

/// A prevalidated home in this slice's canonical root bank. The invariant
/// generative brand prevents use with any other slice, even on the same owner.
/// Clone aliases the same home, without another root or a host allocation.
#[derive(Clone)]
pub struct SliceRoot<'brand> {
    word: NonNull<MarkWord>,
    _brand: Brand<'brand>,
    _thread: PhantomData<Rc<()>>,
}

/// This loan pins the root bank and the original region until wrapper-controlled
/// close. No heap reference, raw payload reader or public early finish exists.
pub struct RunSlice<'brand, 'run> {
    allocation: ConsRunState<'run>,
    bank: &'run mut Vec<MarkWord>,
    key: MutatorKey,
    generation: u64,
    cursor: *mut ConsCell,
    limit: *mut ConsCell,
    cursor_live: bool,
    closed: bool,
    _brand: Brand<'brand>,
}

impl<'brand> RunSlice<'brand, '_> {
    /// Validate a persistent slot once. Only the returned branded home enters
    /// the fast primitive; importing a foreign/stale persistent handle refuses.
    #[cold]
    #[inline(never)]
    pub fn bind(
        &mut self,
        source: &OwnerRootSlot<'_>,
    ) -> Result<SliceRoot<'brand>, RegistrationError> {
        source.check_binding(self.key, self.generation, self.bank.len())?;
        // Vec::as_mut_ptr does not materialize a reference to its contents.
        // The wrapper pins length/capacity and splits allocator fields BEFORE
        // this derivation; cold refill never reborrows this bank or whole owner.
        let word = unsafe { NonNull::new_unchecked(self.bank.as_mut_ptr().add(source.index)) };
        Ok(SliceRoot {
            word,
            _brand: PhantomData,
            _thread: PhantomData,
        })
    }

    /// The branded operands and destination belong to this unique slice. Both
    /// old words load before result installation, including cloned-home aliases.
    #[inline(always)]
    pub fn cons_into(
        &mut self,
        car: &SliceRoot<'brand>,
        cdr: &SliceRoot<'brand>,
        result: &mut SliceRoot<'brand>,
    ) -> Result<(), RegistrationError> {
        if self.cursor == self.limit {
            self.refill()?;
        }
        // SAFETY: only bind constructs a home, with this invariant brand.
        // The bank cannot clear/grow/publish while the slice is borrowed.
        // All cells of the finite run are disjoint from the root allocation.
        unsafe {
            let car = car.word.as_ptr().read().value().bits();
            let cdr = cdr.word.as_ptr().read().value().bits();
            let value = initialize_next_cons(&mut self.cursor, car, cdr);
            result.word.as_ptr().write(MarkWord::of(value));
        }
        Ok(())
    }

    /// Install nil, t, or a fixnum. Any other bit pattern is refused: a heap
    /// pointer store has to go through the cons field operations so the card
    /// mark is not skipped.
    ///
    /// No `NEOVM_THREADS` entry calls the field operations yet. The restricted
    /// executor is `cfg(test)` only.
    #[allow(dead_code)]
    pub fn store_immediate(
        &mut self,
        result: &mut SliceRoot<'brand>,
        bits: usize,
    ) -> Result<(), RegistrationError> {
        let value = TaggedValue::from_bits(bits);
        if !(value.is_fixnum() || value.is_nil() || value.is_t()) {
            return Err(RegistrationError::Publication);
        }
        write_slot(result, value);
        Ok(())
    }

    /// Read car into a branded slot. The caller has already rejected a
    /// non-cons; this still refuses one rather than manufacturing a pointer.
    #[allow(dead_code)]
    pub fn load_car(
        &mut self,
        cell: &SliceRoot<'brand>,
        result: &mut SliceRoot<'brand>,
    ) -> Result<(), RegistrationError> {
        let owner = slot_value(cell);
        let ptr = cons_ptr(owner)?;
        write_slot(result, unsafe { (*ptr).load_car() });
        Ok(())
    }

    /// Read cdr into a branded slot. Same cons precondition as [`Self::load_car`].
    #[allow(dead_code)]
    pub fn load_cdr(
        &mut self,
        cell: &SliceRoot<'brand>,
        result: &mut SliceRoot<'brand>,
    ) -> Result<(), RegistrationError> {
        let owner = slot_value(cell);
        let ptr = cons_ptr(owner)?;
        write_slot(result, unsafe { (*ptr).load_cdr() });
        Ok(())
    }

    /// Store into car. Both words are loaded before the release store. A
    /// non-fixnum value marks the owner's published card and records the owner
    /// cell for the next major trace. No heap TLS and no catalog lock.
    #[allow(dead_code)]
    pub fn store_car(
        &mut self,
        cell: &SliceRoot<'brand>,
        value: &SliceRoot<'brand>,
    ) -> Result<(), RegistrationError> {
        self.store_field(cell, value, true)
    }

    /// Store into cdr. Same barrier as [`Self::store_car`].
    #[allow(dead_code)]
    pub fn store_cdr(
        &mut self,
        cell: &SliceRoot<'brand>,
        value: &SliceRoot<'brand>,
    ) -> Result<(), RegistrationError> {
        self.store_field(cell, value, false)
    }

    #[allow(dead_code)]
    fn store_field(
        &mut self,
        cell: &SliceRoot<'brand>,
        value: &SliceRoot<'brand>,
        car: bool,
    ) -> Result<(), RegistrationError> {
        let owner = slot_value(cell);
        let stored = slot_value(value);
        let ptr = cons_ptr(owner)?;
        if car {
            unsafe { (*ptr).set_car(stored) };
        } else {
            unsafe { (*ptr).set_cdr(stored) };
        }
        if !stored.is_fixnum() {
            self.allocation
                .registry
                .cons_catalog
                .mark_published_cons_card(ptr as usize);
            self.allocation.hot.major_cons_writes.push(owner);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn word_bits_for_test(&self, root: &SliceRoot<'brand>) -> usize {
        slot_value(root).bits()
    }

    #[cfg(test)]
    pub(super) fn plant_word_for_test(&mut self, result: &mut SliceRoot<'brand>, bits: usize) {
        write_slot(result, TaggedValue::from_bits(bits));
    }

    // Commit before any potentially failing/caught operation, and invalidate
    // the cache before releasing a suffix. A caught refill error/panic can
    // never resume through the old range. Region remains canonical in owner.
    fn commit(&mut self) {
        if self.cursor_live {
            let region = self.allocation.region.as_mut().expect("live slice region");
            unsafe { region.commit_slice_cursor(self.cursor) };
            self.cursor_live = false;
        }
        self.cursor = NonNull::<ConsCell>::dangling().as_ptr();
        self.limit = self.cursor;
    }

    #[cold]
    #[inline(never)]
    fn refill(&mut self) -> Result<(), RegistrationError> {
        self.commit();
        self.allocation.refill()?;
        let region = self.allocation.region.as_ref().expect("fresh slice region");
        (self.cursor, self.limit) =
            region.slice_bounds(&self.allocation.registry.cons_catalog, self.key)?;
        self.cursor_live = true;
        Ok(())
    }

    #[cold]
    #[inline(never)]
    fn finish(&mut self) -> Result<(), RegistrationError> {
        self.commit();
        self.allocation.close()?;
        self.closed = true;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn remaining_for_test(&self) -> usize {
        if !self.cursor_live {
            return 0;
        }
        unsafe { self.limit.offset_from(self.cursor) as usize }
    }

    #[cfg(test)]
    pub(super) fn stats_for_test(&self) -> super::super::alloc_region::RegionStats {
        self.allocation.stats.clone()
    }
}

impl Drop for RunSlice<'_, '_> {
    fn drop(&mut self) {
        if !self.closed {
            // No catalog lock/refund in Drop. Keep the canonical region, bank,
            // run obligation and backing; failed slices never ACK or restore.
            self.commit();
            self.allocation.registry.poison();
        }
    }
}

impl RunAccess<'_, '_> {
    /// Open one execution slice. The wrapper owns the actual guard, so
    /// forgetting the callback's &mut reference cannot suppress its close.
    /// HRTB makes every invocation a distinct invariant root-bank brand.
    #[cold]
    #[inline(never)]
    pub fn with_slice<R>(
        &mut self,
        body: impl for<'brand> FnOnce(&mut RunSlice<'brand, '_>) -> R,
    ) -> Result<R, RegistrationError> {
        self.owner.check_thread()?;
        let local = self.owner.local.as_mut().expect("resident owner state");
        let allocation = ConsRunState {
            registry: self.owner.registry,
            entry: &self.owner.entry,
            thread: self.owner.thread,
            hot: &mut local.hot,
            cold: &mut local.cold,
            stats: &mut local.region_stats,
            region: &mut local.catalog_region,
            tails: &mut local.cons_tails,
        };
        allocation.check_boundary()?;
        allocation
            .registry
            .cons_catalog
            .admit_slice(allocation.entry.key)?;
        let (cursor, limit, cursor_live) = match allocation.region.as_ref() {
            Some(region) => {
                let (cursor, limit) =
                    region.slice_bounds(&allocation.registry.cons_catalog, allocation.entry.key)?;
                (cursor, limit, true)
            }
            None => {
                let empty = NonNull::<ConsCell>::dangling().as_ptr();
                (empty, empty, false)
            }
        };
        let mut slice = RunSlice {
            key: allocation.entry.key,
            generation: local.root_slot_generation,
            allocation,
            bank: &mut local.root_scratch,
            cursor,
            limit,
            cursor_live,
            closed: false,
            _brand: PhantomData,
        };
        let result = body(&mut slice);
        slice.finish()?;
        Ok(result)
    }
}

static_assertions::assert_not_impl_any!(RunSlice<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SliceRoot<'static>: Send, Sync, Copy);

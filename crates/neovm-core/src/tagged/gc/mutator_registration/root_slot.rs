//! Indexed homes in the owner's existing published root census.
use super::*;
use crate::tagged::transport::{ConsRootSeal, RootSlotOps, RootedSlot};
use crate::tagged::value::TaggedValue;

/// An opaque owner-local root home, not an object reference or a raw value.
/// Values stay in the canonical owner census even if a handle is dropped or
/// forgotten. ACK/native entry moves that census into the existing GC image;
/// restore returns the same indexed values. Explicit clear invalidates every
/// handle before reusing indices. No handle contains a pointer into the Vec.
/// Cloning aliases that same home; it does not allocate or create a new root.
#[derive(Clone)]
pub struct OwnerRootSlot<'era> {
    key: MutatorKey,
    generation: u64,
    pub(super) index: usize,
    _era: PhantomData<&'era ()>,
    _thread: PhantomData<Rc<()>>,
}
static_assertions::assert_impl_all!(OwnerRootSlot<'static>: Clone);
static_assertions::assert_not_impl_any!(OwnerRootSlot<'static>: Send, Sync, Copy);

impl OwnerRootSlot<'_> {
    #[inline]
    pub(super) fn check(&self, run: &RunAccess<'_, '_>) -> Result<(), RegistrationError> {
        let local = run.owner.local.as_ref().expect("resident owner roots");
        self.check_binding(
            run.owner.entry.key,
            local.root_slot_generation,
            local.root_scratch.len(),
        )
    }

    pub(super) fn check_binding(
        &self,
        key: MutatorKey,
        generation: u64,
        len: usize,
    ) -> Result<(), RegistrationError> {
        if self.key.heap != key.heap {
            return Err(RegistrationError::WrongHeap);
        }
        if self.key.owner != key.owner {
            return Err(RegistrationError::WrongOwner);
        }
        if self.key != key || self.generation != generation || self.index >= len {
            return Err(RegistrationError::StaleRegistration);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn word_for_test(&self, run: &RunAccess<'_, '_>) -> usize {
        self.check(run).unwrap();
        run.owner.local.as_ref().unwrap().root_scratch[self.index]
            .value()
            .bits()
    }
}

impl RootSlotOps for OwnerRootSlot<'_> {
    #[inline]
    fn validate(&self, run: &RunAccess<'_, '_>, _: ConsRootSeal) -> Result<(), RegistrationError> {
        self.check(run)
    }
    #[inline]
    fn word(&self, run: &RunAccess<'_, '_>, _: ConsRootSeal) -> usize {
        run.owner
            .local
            .as_ref()
            .expect("validated owner roots")
            .root_scratch[self.index]
            .value()
            .bits()
    }
    #[inline]
    fn install(&mut self, run: &mut RunAccess<'_, '_>, value: TaggedValue, _: ConsRootSeal) {
        // Validation preceded allocation. The allocation core cannot resize,
        // clear, publish or otherwise change the root bank, and runs no Lisp.
        run.owner
            .local
            .as_mut()
            .expect("validated owner roots")
            .root_scratch[self.index] = super::super::mark_word::MarkWord::of(value);
    }
}
impl RootedSlot for OwnerRootSlot<'_> {}

impl<'owner, 'era> RunAccess<'owner, 'era> {
    /// Terminal negative-fixture cleanup only. This erases this resident
    /// owner's private homes after poison; it grants no World/heap authority
    /// and cannot make the failed era resumable. Production poison checks
    /// remain intact, including ordinary clear_root_slots.
    #[cfg(test)]
    pub(super) fn clear_terminal_roots_for_test(&mut self) -> Result<(), RegistrationError> {
        if self.owner.thread != std::thread::current().id() {
            return Err(RegistrationError::WrongThread);
        }
        if !self.owner.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Publication);
        }
        if self.symbol_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        let local = self
            .owner
            .local
            .as_mut()
            .ok_or(RegistrationError::StaleRegistration)?;
        let next = local
            .root_slot_generation
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        local.root_slot_generation = next;
        local.root_scratch.clear();
        Ok(())
    }

    /// C4's opaque cache/activation owns this freshly reserved home. Unlike
    /// operand homes it is never exposed or clonable through that API. NIL
    /// needs no heap import seal, and the pre-reserved bank never grows here.
    pub(super) fn private_nil_root(&mut self) -> Result<OwnerRootSlot<'era>, RegistrationError> {
        self.owner.check_thread()?;
        if self.symbol_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        let local = self
            .owner
            .local
            .as_mut()
            .ok_or(RegistrationError::StaleRegistration)?;
        let index = local.root_scratch.len();
        if index >= local.root_slot_limit || index >= local.root_scratch.capacity() {
            return Err(RegistrationError::Capacity);
        }
        local
            .root_scratch
            .push(super::super::mark_word::MarkWord::of(TaggedValue::NIL));
        Ok(OwnerRootSlot {
            key: self.owner.entry.key,
            generation: local.root_slot_generation,
            index,
            _era: PhantomData,
            _thread: PhantomData,
        })
    }

    /// Only the sealed transport bridge can admit a live source word.
    pub(crate) fn admit_root_slot(
        &mut self,
        word: usize,
        _: ConsRootSeal,
    ) -> Result<OwnerRootSlot<'era>, RegistrationError> {
        self.owner.check_thread()?;
        let local = self.owner.local.as_mut().expect("resident owner roots");
        if local.root_scratch.len() >= local.root_slot_limit {
            return Err(RegistrationError::Capacity);
        }
        let index = local.root_scratch.len();
        // register(root_capacity) reserved this bank once. Never grow it here.
        debug_assert!(local.root_scratch.capacity() >= local.root_slot_limit);
        local
            .root_scratch
            .push(super::super::mark_word::MarkWord::of(
                TaggedValue::from_bits(word),
            ));
        Ok(OwnerRootSlot {
            key: self.owner.entry.key,
            generation: local.root_slot_generation,
            index,
            _era: PhantomData,
            _thread: PhantomData,
        })
    }

    /// End the current root frame. No raw value can escape from these handles;
    /// a checked generation change invalidates all of them before index reuse.
    /// Call only after transferring any needed result into another root home.
    #[cold]
    #[inline(never)]
    pub fn clear_root_slots(&mut self) -> Result<(), RegistrationError> {
        self.owner.check_thread()?;
        let local = self.owner.local.as_mut().expect("resident owner roots");
        let next = local
            .root_slot_generation
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        local.root_slot_generation = next;
        local.root_scratch.clear();
        Ok(())
    }
}

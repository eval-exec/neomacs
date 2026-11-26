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
    /// Value-FENCE hook (C1): a forged-index copy of this root home for the
    /// relocated transition fixtures. Callers go through
    /// neovm_value::test_support::authority::owner_root_slot_with_index_for_test.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn with_index_for_test(&self, index: usize) -> Self {
        Self {
            index,
            ..self.clone()
        }
    }

    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(super) fn check(&self, run: &RunAccess<'_, '_>) -> Result<(), RegistrationError> {
        let local = run.owner.local.as_ref().expect("resident owner roots");
        self.check_binding(
            run.owner.entry.key,
            local.root_slot_generation,
            local.root_scratch_len(),
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

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn word_for_test(&self, run: &RunAccess<'_, '_>) -> usize {
        self.check(run).unwrap();
        run.owner
            .local
            .as_ref()
            .unwrap()
            .root_slot_bits(self.index)
            .unwrap()
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
            .root_slot_bits(self.index)
            .expect("validated owner root index")
    }
    #[inline]
    fn install(&mut self, run: &mut RunAccess<'_, '_>, value: TaggedValue, _: ConsRootSeal) {
        // Validation preceded allocation. The allocation core cannot resize,
        // clear, publish or otherwise change the root bank, and runs no Lisp.
        // SAFETY: `ConsRootSeal` proves `value` was allocated/rooted on this
        // owner's heap under the active era protocol.
        unsafe {
            run.owner
                .local
                .as_mut()
                .expect("validated owner roots")
                .set_root_slot(self.index, value)
                .expect("validated owner root index");
        }
    }
}
impl RootedSlot for OwnerRootSlot<'_> {}

impl<'owner, 'era> RunAccess<'owner, 'era> {
    /// Closed C6 executor primitive: discard one census value, without opening
    /// an allocator slice or lending any graph mutation/decoding authority.
    #[cfg(all(
        any(test, feature = "test-support"),
        not(feature = "gc-memory-telemetry")
    ))]
    pub(super) fn clear_owner_root_for_test(
        &mut self,
        home: &OwnerRootSlot<'era>,
    ) -> Result<(), RegistrationError> {
        self.owner.check_thread()?;
        if self.symbol_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        if self
            .owner
            .local
            .as_ref()
            .is_some_and(|local| local.user_test_holds.load(Ordering::Relaxed) > 0)
        {
            return Err(RegistrationError::OutstandingLoan);
        }
        symbol_authority::check_owner_home_for_test(self, home)?;
        self.owner
            .local
            .as_mut()
            .and_then(|local| local.set_root_slot_nil(home.index))
            .ok_or(RegistrationError::StaleRegistration)?;
        Ok(())
    }

    /// Terminal negative-fixture cleanup only. This erases this resident
    /// owner's private homes after poison; it grants no World/heap authority
    /// and cannot make the failed era resumable. Production poison checks
    /// remain intact, including ordinary clear_root_slots.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn clear_terminal_roots_for_test(&mut self) -> Result<(), RegistrationError> {
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
        if local.user_test_holds.load(Ordering::Relaxed) > 0 {
            return Err(RegistrationError::OutstandingLoan);
        }
        let next = local
            .root_slot_generation
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        local.root_slot_generation = next;
        local.clear_root_scratch();
        Ok(())
    }

    /// Reserve a NIL census home without a managed import. C4 cache/activation
    /// keep their returned handles private; C6's closed test executor also
    /// uses this leaf to initialize operand homes. NIL needs no heap import
    /// seal, and the pre-reserved bank never grows here.
    pub(crate) fn private_nil_root(&mut self) -> Result<OwnerRootSlot<'era>, RegistrationError> {
        self.owner.check_thread()?;
        if self.symbol_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        let local = self
            .owner
            .local
            .as_mut()
            .ok_or(RegistrationError::StaleRegistration)?;
        let index = local.root_scratch_len();
        if index >= local.root_slot_limit || index >= local.root_scratch_capacity() {
            return Err(RegistrationError::Capacity);
        }
        local.push_nil_root_slot();
        Ok(OwnerRootSlot {
            key: self.owner.entry.key,
            generation: local.root_slot_generation,
            index,
            _era: PhantomData,
            _thread: PhantomData,
        })
    }

    /// Only the sealed transport bridge can admit a live source word.
    ///
    /// # Safety
    /// `word` must encode a live `TaggedValue` belonging to this run's heap.
    pub(crate) unsafe fn admit_root_slot(
        &mut self,
        word: usize,
        _: ConsRootSeal,
    ) -> Result<OwnerRootSlot<'era>, RegistrationError> {
        self.owner.check_thread()?;
        let local = self.owner.local.as_mut().expect("resident owner roots");
        if local.root_scratch_len() >= local.root_slot_limit {
            return Err(RegistrationError::Capacity);
        }
        let index = local.root_scratch_len();
        // register(root_capacity) reserved this bank once. Never grow it here.
        debug_assert!(local.root_scratch_capacity() >= local.root_slot_limit);
        // SAFETY: caller guarantees `word` encodes a live `TaggedValue` on
        // this run's heap.
        unsafe { local.push_root_slot(TaggedValue::from_bits(word)) };
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
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn clear_root_slots(&mut self) -> Result<(), RegistrationError> {
        self.owner.check_thread()?;
        let local = self.owner.local.as_mut().expect("resident owner roots");
        if local.user_test_holds.load(Ordering::Relaxed) > 0 {
            return Err(RegistrationError::OutstandingLoan);
        }
        let next = local
            .root_slot_generation
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        local.root_slot_generation = next;
        local.clear_root_scratch();
        Ok(())
    }
}

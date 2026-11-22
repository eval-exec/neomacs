//! Receipt of one native publication. It is not a Lisp frame, a heap loan,
//! or a resume right. Dropping it does not poison the owner.

use super::super::rendezvous::entry_state;
use super::super::{
    EntryPhase, Mode, Mutator, MutatorEntry, Registry, RegistryState, RendezvousError,
};
use crate::tagged::transport::{ParallelRootIdentity, SharedRoot};
use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::atomic::Ordering;
use std::sync::{Arc, MutexGuard};
use std::thread::ThreadId;

#[derive(Clone, Copy, PartialEq, Eq)]
struct RegistryAddress(usize);

impl RegistryAddress {
    fn of(registry: &Registry) -> Self {
        Self(std::ptr::from_ref(registry).addr())
    }
}

/// Owns the canonical entry. The address is comparison metadata only.
#[must_use]
#[doc(hidden)]
pub struct PublishedOwner {
    entry: Arc<MutatorEntry>,
    registry: RegistryAddress,
    identity: ParallelRootIdentity,
    thread: ThreadId,
    _not_sync: PhantomData<Cell<()>>,
}

impl std::fmt::Debug for PublishedOwner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PublishedOwner")
            .field("key", &self.identity.key())
            .field("generation", &self.identity.publication_generation())
            .finish_non_exhaustive()
    }
}

fn current_native(
    registry: &Registry,
    gate: &RegistryState,
    entry: &Arc<MutatorEntry>,
    thread: ThreadId,
) -> Result<ParallelRootIdentity, RendezvousError> {
    gate.rendezvous.live()?;
    if registry.park.poisoned.load(Ordering::Acquire) {
        return Err(RendezvousError::Poisoned);
    }
    if !Arc::ptr_eq(registry.validate_ref(gate, entry.key)?, entry) {
        return Err(RendezvousError::Identity);
    }
    let state: MutexGuard<'_, super::super::EntryState> = entry_state(entry)?;
    if state.phase != EntryPhase::Published
        || state.rendezvous.mode != Mode::NativeStopped
        || state.rendezvous.retiring
        || entry.cancelled_start.load(Ordering::Acquire)
    {
        return Err(RendezvousError::Publication);
    }
    if state.owner_thread != Some(thread) {
        return Err(RendezvousError::Identity);
    }
    if state.run_loan || state.worker_loan || entry.admitted() {
        return Err(RendezvousError::Loan);
    }
    if state.readers != 0 {
        return Err(RendezvousError::Readers);
    }
    let image = state.image.as_ref().ok_or(RendezvousError::Publication)?;
    let identity = state
        .roots
        .as_ref()
        .ok_or(RendezvousError::Publication)?
        .identity();
    if image.young_id() != entry.young
        || identity.key() != entry.key
        || identity.heap_identity() != registry.heap
        || identity.publication_generation().get() != state.publication_generation
        || identity.publication_epoch() != state.epoch
    {
        return Err(RendezvousError::Identity);
    }
    Ok(identity)
}

impl PublishedOwner {
    pub(in crate::tagged::gc::mutator_registration) fn validate_current(
        &self,
        registry: &Registry,
        gate: &RegistryState,
    ) -> Result<(), RendezvousError> {
        if self.registry != RegistryAddress::of(registry)
            || !gate.rendezvous.frozen.contains(&self.identity.key())
        {
            return Err(RendezvousError::Identity);
        }
        let current = current_native(registry, gate, &self.entry, self.thread)?;
        if current != self.identity {
            return Err(RendezvousError::Generation);
        }
        Ok(())
    }
}

impl Mutator<'_> {
    /// Cold receipt, minted only after `enter_native` has installed the image
    /// and the root lease. Failure leaves the owner where that transaction left it.
    #[cold]
    #[doc(hidden)]
    pub fn publish_native_owner(
        &mut self,
        roots: &[SharedRoot],
    ) -> Result<PublishedOwner, RendezvousError> {
        self.enter_native(roots)?;
        let gate = self.registry.lock()?;
        if !self.protocol.participating {
            return Err(RendezvousError::Publication);
        }
        let identity = current_native(self.registry, &gate, &self.entry, self.thread)?;
        Ok(PublishedOwner {
            entry: Arc::clone(&self.entry),
            registry: RegistryAddress::of(self.registry),
            identity,
            thread: self.thread,
            _not_sync: PhantomData,
        })
    }
}

static_assertions::assert_impl_all!(PublishedOwner: Send);
static_assertions::assert_not_impl_any!(PublishedOwner: Sync, Clone, Copy);
const _: fn() = || {
    fn reference_free<T: 'static>() {}
    reference_free::<PublishedOwner>();
};

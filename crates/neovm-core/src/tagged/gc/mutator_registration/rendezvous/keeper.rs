//! Unique keeper admission and epoch-bounded stopped access.

use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;

#[cfg(test)]
use super::super::a8_probes;
use super::super::image::PublishedGcImage;
use super::super::{EraCore, MutatorEntry, MutatorKey, RegistrationError, Registry, RegistryState};
use super::control::Phase;
use super::{
    RendezvousError, StoppedRootAdmission, entry_state, finish_debt, membership, request,
    verify_stopped, wait,
};
use crate::tagged::gc::scan_contract::ParallelStoppedMutatorsProof;
use crate::tagged::transport::ParallelRootBatchReader;

fn registration_error(error: RegistrationError) -> RendezvousError {
    error.into()
}

fn keeper(registry: &Registry) -> Result<(), RendezvousError> {
    if registry.keeper_thread != std::thread::current().id() {
        return Err(RendezvousError::Registration(
            RegistrationError::WrongThread,
        ));
    }
    Ok(())
}

struct CallbackAttempt<'a> {
    registry: &'a Registry,
    finished: bool,
}

impl Drop for CallbackAttempt<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.registry.poison();
        }
    }
}

#[must_use = "complete the stopped epoch explicitly; abandonment poisons and retains"]
pub struct AllStopped<'era> {
    registry: &'era Registry,
    epoch: NonZeroU64,
    traced: bool,
    finished: bool,
    _thread: PhantomData<Rc<()>>,
}

impl fmt::Debug for AllStopped<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AllStopped")
            .field("heap", &self.registry.heap)
            .field("epoch", &self.epoch)
            .field("traced", &self.traced)
            .finish_non_exhaustive()
    }
}

#[must_use = "recover the stopped guard or retain the poisoned epoch"]
pub struct AllStoppedFailure<'era> {
    pub reason: RendezvousError,
    stopped: AllStopped<'era>,
}

impl<'era> AllStoppedFailure<'era> {
    pub fn into_stopped(self) -> AllStopped<'era> {
        self.stopped
    }
}

impl fmt::Debug for AllStoppedFailure<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AllStoppedFailure")
            .field("reason", &self.reason)
            .field("epoch", &self.stopped.epoch)
            .finish_non_exhaustive()
    }
}

impl EraCore<'_> {
    pub fn try_stop(&self) -> Result<Option<AllStopped<'_>>, RendezvousError> {
        let registry = self.registry();
        keeper(registry)?;
        let mut gate = registry.lock().map_err(registration_error)?;
        gate.rendezvous.live()?;
        let epoch = match gate.rendezvous.phase {
            Phase::Stopping(epoch) => epoch,
            Phase::Tracing(_) => return Err(RendezvousError::Busy),
            _ => return Ok(None),
        };
        let owners = membership(&gate)?;
        if !gate.rendezvous.ready(&owners)? {
            return Ok(None);
        }
        // State::ready checks modes only. The sealed constructor additionally
        // requires every actual root/image/loan/epoch witness to match.
        verify_stopped(registry, &gate)?;
        gate.rendezvous.begin_trace(epoch, &owners)?;
        Ok(Some(AllStopped {
            registry,
            epoch,
            traced: false,
            finished: false,
            _thread: PhantomData,
        }))
    }

    pub fn stop(&self) -> Result<AllStopped<'_>, RendezvousError> {
        let registry = self.registry();
        keeper(registry)?;
        request(registry)?;
        let _park = registry.park_ticket();
        let mut gate = registry.lock().map_err(registration_error)?;
        loop {
            gate.rendezvous.live()?;
            if registry.park.poisoned.load(Ordering::Acquire) {
                return Err(RendezvousError::Poisoned);
            }
            match gate.rendezvous.phase {
                Phase::Idle => {
                    // The shared request helper also publishes the Release
                    // epoch and rebinds NativeStopped coverage. Never bypass it.
                    drop(gate);
                    request(registry)?;
                    gate = registry.lock().map_err(registration_error)?;
                    continue;
                }
                Phase::Stopping(epoch) => {
                    let owners = membership(&gate)?;
                    if gate.rendezvous.ready(&owners)? {
                        verify_stopped(registry, &gate)?;
                        gate.rendezvous.begin_trace(epoch, &owners)?;
                        return Ok(AllStopped {
                            registry,
                            epoch,
                            traced: false,
                            finished: false,
                            _thread: PhantomData,
                        });
                    }
                }
                // A single keeper cannot wait for its own outstanding guard.
                Phase::Tracing(_) => return Err(RendezvousError::Busy),
                Phase::DrainingResumes => {}
                Phase::Poisoned => return Err(RendezvousError::Poisoned),
            }
            for key in &gate.rendezvous.frozen {
                let entry = registry.validate(&gate, *key)?;
                let state = entry_state(&entry)?;
                if state.owner_thread == Some(registry.keeper_thread)
                    && matches!(
                        state.rendezvous.mode,
                        super::Mode::Running | super::Mode::Publishing
                    )
                {
                    return Err(RendezvousError::Busy);
                }
            }
            if gate.rendezvous.phase == Phase::DrainingResumes {
                for key in &gate.rendezvous.debt {
                    let entry = registry.validate(&gate, *key)?;
                    if entry_state(&entry)?.owner_thread == Some(registry.keeper_thread) {
                        return Err(RendezvousError::Busy);
                    }
                }
            }
            // Event-driven wait atomically releases this predicate mutex.
            // Poison uses that same gate; a wake supplies no stop authority.
            gate = wait(registry, gate)?;
        }
    }
}

impl<'era> AllStopped<'era> {
    pub fn symbols(&mut self) -> Result<super::super::KeeperSymbols<'_>, RendezvousError> {
        let registry = self.registry;
        super::super::symbol_authority::KeeperSymbols::mint(self, registry)
    }
    pub(crate) fn check_world_roots(
        &self,
        heap: super::super::HeapIdentity,
    ) -> Result<(), RendezvousError> {
        keeper(self.registry)?;
        if self.registry.heap != heap {
            return Err(RendezvousError::Identity);
        }
        let gate = self.registry.lock()?;
        self.validate(&gate)
    }

    pub(in crate::tagged::gc) fn gate_is_held_by_current_thread(&self) -> bool {
        self.registry.gate_is_held_by_current_thread()
    }

    pub(in crate::tagged::gc::mutator_registration) fn matches_registry(
        &self,
        registry: &Registry,
    ) -> bool {
        std::ptr::eq(self.registry, registry)
    }

    pub fn epoch(&self) -> NonZeroU64 {
        self.epoch
    }

    fn validate(&self, gate: &RegistryState) -> Result<(), RendezvousError> {
        if self.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RendezvousError::Poisoned);
        }
        if gate.rendezvous.phase != Phase::Tracing(self.epoch) {
            return Err(RendezvousError::Publication);
        }
        verify_stopped(self.registry, gate)
    }

    pub(in crate::tagged::gc) fn trace<R>(
        &mut self,
        body: impl for<'stop> FnOnce(
            &'stop AllStopped<'era>,
            &'stop ParallelStoppedMutatorsProof<'stop>,
        ) -> Result<R, RendezvousError>,
    ) -> Result<R, RendezvousError> {
        if self.traced {
            return Err(RendezvousError::Publication);
        }
        {
            let gate = self.registry.lock().map_err(registration_error)?;
            self.validate(&gate)?;
        }
        // A callback may have already changed the image. Error or unwind is
        // terminal; retry must not refold a partially visited image.
        let mut attempt = CallbackAttempt {
            registry: self.registry,
            finished: false,
        };
        let result = {
            let proof = ParallelStoppedMutatorsProof::from_all_stopped(self);
            body(self, &proof)?
        };
        if self.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RendezvousError::Poisoned);
        }
        self.traced = true;
        attempt.finished = true;
        Ok(result)
    }

    fn entry(&self, key: MutatorKey) -> Result<Arc<MutatorEntry>, RendezvousError> {
        let gate = self.registry.lock().map_err(registration_error)?;
        self.validate(&gate)?;
        if key.owner != 0 && !gate.rendezvous.frozen.contains(&key) {
            return Err(RendezvousError::Identity);
        }
        self.registry
            .validate(&gate, key)
            .map_err(registration_error)
    }

    pub(crate) fn with_root_reader<R>(
        &self,
        key: MutatorKey,
        body: impl for<'read> FnOnce(ParallelRootBatchReader<'read>) -> R,
    ) -> Result<R, RendezvousError> {
        let entry = self.entry(key)?;
        // No registry gate survives entry lookup. Root-table acquisition and
        // user code must never run while a control predicate mutex is held.
        let state = entry_state(&entry)?;
        let roots = state.roots.as_ref().ok_or(RendezvousError::Publication)?;
        let admission = StoppedRootAdmission {
            identity: roots.identity(),
            epoch: self.epoch.get(),
            _stop: PhantomData,
            _thread: PhantomData,
        };
        let reader = roots.reader(&admission).map_err(RendezvousError::from)?;
        let mut attempt = CallbackAttempt {
            registry: self.registry,
            finished: false,
        };
        let result = body(reader);
        attempt.finished = true;
        if self.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RendezvousError::Poisoned);
        }
        Ok(result)
    }

    pub(in crate::tagged::gc) fn with_owner_image<R>(
        &self,
        key: MutatorKey,
        body: impl FnOnce(&mut PublishedGcImage) -> R,
    ) -> Result<R, RendezvousError> {
        let entry = self.entry(key)?;
        let mut state = entry_state(&entry)?;
        let image = state.image.as_mut().ok_or(RendezvousError::Publication)?;
        let mut attempt = CallbackAttempt {
            registry: self.registry,
            finished: false,
        };
        let result = body(image);
        attempt.finished = true;
        if self.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RendezvousError::Poisoned);
        }
        Ok(result)
    }

    fn prepare_complete(&mut self) -> Result<(), RendezvousError> {
        if !self.traced {
            return Err(RendezvousError::Publication);
        }
        let entries = {
            let gate = self.registry.lock().map_err(registration_error)?;
            self.validate(&gate)?;
            let mut entries = Vec::new();
            entries
                .try_reserve_exact(gate.entries.len())
                .map_err(|_| RendezvousError::Capacity)?;
            entries.extend(gate.entries.iter().cloned());
            entries
        };
        for entry in entries {
            let state = entry_state(&entry)?;
            if let Some(roots) = &state.roots {
                roots.validate_no_readers().map_err(RendezvousError::from)?;
            }
            for roots in &state.retained_roots {
                roots.validate_no_readers().map_err(RendezvousError::from)?;
            }
        }
        let mut gate = self.registry.lock().map_err(registration_error)?;
        self.validate(&gate)?;
        let owners = membership(&gate)?;
        // This may still refuse capacity/counter exhaustion. Do not move or
        // update an owner's image before the last fallible control preflight.
        gate.rendezvous.complete(self.epoch, &owners)?;
        let completion = (|| {
            for key in &gate.rendezvous.frozen {
                let entry = self
                    .registry
                    .validate(&gate, *key)
                    .map_err(registration_error)?;
                entry_state(&entry)?.rendezvous.image_epoch = self.epoch.get();
            }
            self.registry
                .completed_epoch
                .store(self.epoch.get(), Ordering::Release);
            finish_debt(self.registry, &mut gate)
        })();
        if completion.is_err() {
            // The control commit already opened DrainingResumes. An unexpected
            // lock/census failure must become terminal before releasing its
            // predicate mutex; no owner may observe a partial image update.
            gate.rendezvous.poison();
            self.registry.poison();
        }
        #[cfg(test)]
        if completion.is_ok() {
            a8_probes::mark(a8_probes::KEEPER_COMPLETED, gate.rendezvous.completed, 0);
        }
        self.registry.park.wake.notify_all();
        completion
    }

    pub fn complete(mut self) -> Result<(), AllStoppedFailure<'era>> {
        match self.prepare_complete() {
            Ok(()) => {
                self.finished = true;
                Ok(())
            }
            Err(reason) => Err(AllStoppedFailure {
                reason,
                stopped: self,
            }),
        }
    }
}

impl Drop for AllStopped<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.registry.poison();
            self.registry.park.wake.notify_all();
        }
    }
}

static_assertions::assert_not_impl_any!(AllStopped<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(AllStoppedFailure<'static>: Send, Sync, Clone, Copy);

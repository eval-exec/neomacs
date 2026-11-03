//! One-shot owner image restoration, serialized with request admission.

use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::Ordering;

#[cfg(test)]
use super::super::a8_probes;
use super::super::image::PublishedGcImage;
use super::super::{EntryPhase, Mutator, RegistrationError};
use super::{Mode, Phase, RendezvousError, entry_state, finish_debt, wait};

#[must_use = "apply on the original owner; abandonment poisons and retains"]
pub struct ResumeGcImage<'owner, 'era> {
    owner: &'owner mut Mutator<'era>,
    image: Option<PublishedGcImage>,
    epoch: u64,
    generation: u64,
    finished: bool,
    _thread: PhantomData<Rc<()>>,
}

impl fmt::Debug for ResumeGcImage<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResumeGcImage")
            .field("key", &self.owner.entry.key)
            .field("epoch", &self.epoch)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

#[must_use = "retry the retained image or retain the poisoned owner"]
pub struct ResumeGcImageFailure<'owner, 'era> {
    pub reason: RendezvousError,
    image: ResumeGcImage<'owner, 'era>,
}

impl<'owner, 'era> ResumeGcImageFailure<'owner, 'era> {
    pub fn into_image(self) -> ResumeGcImage<'owner, 'era> {
        self.image
    }
}

impl fmt::Debug for ResumeGcImageFailure<'_, '_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResumeGcImageFailure")
            .field("reason", &self.reason)
            .field("epoch", &self.image.epoch)
            .finish_non_exhaustive()
    }
}

impl<'era> Mutator<'era> {
    pub fn begin_restore(
        &mut self,
        native: bool,
    ) -> Result<ResumeGcImage<'_, 'era>, RendezvousError> {
        let (image, epoch, generation) = self.prepare_restore(native)?;
        Ok(ResumeGcImage {
            owner: self,
            image: Some(image),
            epoch,
            generation,
            finished: false,
            _thread: PhantomData,
        })
    }

    #[cfg(test)]
    pub(in crate::tagged::gc::mutator_registration) fn begin_restore_retry(
        &mut self,
        native: bool,
    ) -> Result<ResumeGcImage<'_, 'era>, RendezvousError> {
        let (image, epoch, generation) = loop {
            match self.prepare_restore(native) {
                Ok(parts) => break parts,
                Err(RendezvousError::Busy) => std::thread::yield_now(),
                Err(error) => return Err(error),
            }
        };
        Ok(ResumeGcImage {
            owner: self,
            image: Some(image),
            epoch,
            generation,
            finished: false,
            _thread: PhantomData,
        })
    }

    #[cold]
    pub(super) fn restore_wait(&mut self, native: bool) -> Result<u64, RendezvousError> {
        let (image, epoch, generation) = loop {
            match self.prepare_restore(native) {
                Ok(parts) => break parts,
                Err(RendezvousError::Busy) => std::thread::yield_now(),
                Err(error) => return Err(error),
            }
        };
        ResumeGcImage {
            owner: self,
            image: Some(image),
            epoch,
            generation,
            finished: false,
            _thread: PhantomData,
        }
        .apply_wait()
        .map_err(|failure| failure.reason)?;
        Ok(epoch)
    }

    fn prepare_restore(
        &mut self,
        native: bool,
    ) -> Result<(PublishedGcImage, u64, u64), RendezvousError> {
        self.check_thread()?;
        let registry = self.registry;
        // A stopped-summary or root callback may reenter on this thread.
        // Retained control/entry guards must cause Busy rather than a spin.
        let _park = registry.park_ticket();
        let mut gate = registry.lock()?;
        loop {
            registry.validate(&gate, self.entry.key)?;
            let mut state = entry_state(&self.entry)?;
            let expected = if native {
                Mode::NativeStopped
            } else {
                Mode::Stopped
            };
            // Reject a Running caller before waiting: it cannot wait for its
            // own missing ACK. NativeStopped is already covered by its image.
            if state.rendezvous.mode != expected || state.rendezvous.retiring {
                return Err(RendezvousError::Busy);
            }
            if state.phase != EntryPhase::Published || !self.protocol.participating {
                return Err(RendezvousError::Publication);
            }
            match gate.rendezvous.phase {
                Phase::Stopping(_) | Phase::Tracing(_) => {
                    if self.thread == registry.keeper_thread {
                        return Err(RendezvousError::Busy);
                    }
                    drop(state);
                    #[cfg(test)]
                    if let Some(hook) = self.protocol.restore_wait_hook.take() {
                        hook();
                    }
                    // The predicate mutex is released by Condvar. No entry,
                    // image, or allocation loan survives across this wait.
                    gate = wait(registry, gate)?;
                    continue;
                }
                Phase::Poisoned => return Err(RendezvousError::Poisoned),
                Phase::Idle | Phase::DrainingResumes => {}
            }
            if registry.completed_epoch.load(Ordering::Acquire) != gate.rendezvous.completed {
                return Err(RendezvousError::Publication);
            }
            if state.run_loan || state.worker_loan || self.entry.admitted() {
                return Err(RendezvousError::Loan);
            }
            if state.readers != 0 {
                return Err(RendezvousError::Readers);
            }
            if self.local.is_none() || state.image.is_none() {
                return Err(RendezvousError::Publication);
            }
            state
                .rendezvous
                .image_takes
                .checked_add(1)
                .ok_or(RendezvousError::Generation)?;
            let roots = state.roots.as_ref().ok_or(RendezvousError::Publication)?;
            let identity = roots.identity();
            if identity.key() != self.entry.key
                || identity.publication_generation().get() != state.publication_generation
                || identity.publication_epoch() != state.epoch
            {
                return Err(RendezvousError::Identity);
            }
            let generation = state.publication_generation;
            let epoch =
                gate.rendezvous
                    .begin_restore(self.entry.key, &mut state.rendezvous, native)?;
            // The debt reservation and Resuming transition precede this move.
            // A request can no longer activate until apply finishes the debt.
            let image = state
                .image
                .take()
                .expect("image preflighted under entry guard");
            drop(state);
            drop(gate);
            return Ok((image, epoch, generation));
        }
    }

    pub fn leave_native(&mut self) -> Result<u64, RendezvousError> {
        let image = self.begin_restore(true)?;
        let epoch = image.epoch();
        image.apply_wait().map_err(|failure| failure.reason)?;
        Ok(epoch)
    }
}

impl<'owner, 'era> ResumeGcImage<'owner, 'era> {
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    #[cold]
    pub(super) fn apply_wait(mut self) -> Result<(), ResumeGcImageFailure<'owner, 'era>> {
        loop {
            match self.apply() {
                Ok(()) => return Ok(()),
                Err(failure) if failure.reason == RendezvousError::Busy => {
                    self = failure.into_image();
                    std::thread::yield_now();
                }
                Err(failure) => return Err(failure),
            }
        }
    }

    fn try_apply(&mut self) -> Result<(), RendezvousError> {
        self.owner.check_thread()?;
        let registry = self.owner.registry;
        let mut gate = registry.lock()?;
        registry.validate(&gate, self.owner.entry.key)?;
        let mut state = entry_state(&self.owner.entry)?;
        if gate.rendezvous.phase != Phase::DrainingResumes
            || state.rendezvous.mode != Mode::Resuming
            || state.phase != EntryPhase::Published
            || state.rendezvous.retiring
            || state.image.is_some()
            || state.rendezvous.image_epoch != self.epoch
            || state.publication_generation != self.generation
            || !gate.rendezvous.debt.contains(&self.owner.entry.key)
        {
            return Err(RendezvousError::Publication);
        }
        if registry.completed_epoch.load(Ordering::Acquire) != gate.rendezvous.completed {
            return Err(RendezvousError::Publication);
        }
        if state.run_loan || state.worker_loan || self.owner.entry.admitted() {
            return Err(RendezvousError::Loan);
        }
        if state.readers != 0 {
            return Err(RendezvousError::Readers);
        }
        // All lock refusals occur before restoration. The confined restore
        // itself takes no mutex and preserves its image on validation failure.
        let local = self
            .owner
            .local
            .as_mut()
            .ok_or(RendezvousError::Publication)?;
        let image = self.image.take().ok_or(RendezvousError::Publication)?;
        if let Err(failure) = local.restore(image) {
            self.image = Some(failure.image);
            return Err(RegistrationError::from(failure.error).into());
        }
        let completion = gate
            .rendezvous
            .finish_restore(self.owner.entry.key, &mut state.rendezvous);
        if completion.is_ok() {
            state.phase = EntryPhase::Running;
            #[cfg(test)]
            a8_probes::mark(
                a8_probes::OWNER_RUNNING,
                self.epoch,
                self.owner.entry.key.owner,
            );
        }
        drop(state);
        let completion = completion.and_then(|()| finish_debt(registry, &mut gate));
        if completion.is_err() {
            // The payload is now resident in the owner. Never return it to a
            // publication shell or let a new request activate after failure.
            gate.rendezvous.poison();
            registry.poison();
        }
        registry.park.wake.notify_all();
        completion
    }

    pub fn apply(mut self) -> Result<(), ResumeGcImageFailure<'owner, 'era>> {
        match self.try_apply() {
            Ok(()) => {
                self.finished = true;
                Ok(())
            }
            Err(reason) => Err(ResumeGcImageFailure {
                reason,
                image: self,
            }),
        }
    }
}

impl Drop for ResumeGcImage<'_, '_> {
    fn drop(&mut self) {
        if !self.finished {
            self.owner.registry.poison();
            // The canonical entry remains registered in Resuming. Neither its
            // image nor any rooted handle may be destroyed on abandonment.
            if let Some(image) = self.image.take() {
                std::mem::forget(image);
            }
        }
    }
}

static_assertions::assert_not_impl_any!(ResumeGcImage<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ResumeGcImageFailure<'static, 'static>: Send, Sync, Clone, Copy);

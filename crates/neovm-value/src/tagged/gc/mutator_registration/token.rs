//! One-shot metadata bootstrap and destination-thread owner storage.
use super::super::young_registry::YoungBlockLog;
use super::image::ImageError;
use super::*;

impl From<ImageError> for RegistrationError {
    fn from(error: ImageError) -> Self {
        match error {
            ImageError::Capacity => Self::Capacity,
            ImageError::RegionsOpen => Self::OpenRegion,
            ImageError::SatbResidual => Self::ResidualSatb,
            ImageError::NonemptyYoung => Self::PendingYoung,
            ImageError::ExecutionRoots => Self::PendingRoots,
            ImageError::Young(error) => error.into(),
            ImageError::DestinationOccupied => Self::Publication,
        }
    }
}

#[must_use = "activate on the destination thread or cancel the unused start"]
pub struct MutatorPermit<'era> {
    pub(super) registry: &'era Registry,
    pub(super) entry: Arc<MutatorEntry>,
    pub(super) root_capacity: usize,
    pub(super) armed: bool,
    pub(super) _unshared: PhantomData<Cell<()>>,
}

impl std::fmt::Debug for MutatorPermit<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MutatorPermit")
            .field(&self.entry.key)
            .finish()
    }
}

pub struct MutatorStartFailure<'era> {
    pub reason: RegistrationError,
    permit: MutatorPermit<'era>,
}
impl<'era> MutatorStartFailure<'era> {
    pub fn into_permit(self) -> MutatorPermit<'era> {
        self.permit
    }
}
impl std::fmt::Debug for MutatorStartFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MutatorStartFailure")
            .field(&self.reason)
            .finish()
    }
}

impl<'era> MutatorPermit<'era> {
    pub fn key(&self) -> MutatorKey {
        self.entry.key
    }

    #[cold]
    #[inline(never)]
    pub fn activate(
        mut self,
        heap: HeapIdentity,
        owner: u64,
    ) -> Result<Mutator<'era>, MutatorStartFailure<'era>> {
        let result = (|| {
            if heap != self.registry.heap {
                return Err(RegistrationError::WrongHeap);
            }
            if owner != self.entry.key.owner {
                return Err(RegistrationError::WrongOwner);
            }
            {
                let state = self.registry.lock()?;
                self.registry.validate(&state, self.entry.key)?;
            }
            // All confined buffers are created HERE, never on the sender.
            let local = OwnerGcState::new(heap, self.entry.young, self.root_capacity)
                .map_err(RegistrationError::from)?;
            let local = Box::new(local);
            let gate = self.registry.lock()?;
            let entry = self.registry.validate(&gate, self.entry.key)?;
            let mut state = entry.state.try_lock().map_err(entry_lock_error)?;
            if state.phase != EntryPhase::Starting || entry.cancelled_start.load(Ordering::Acquire)
            {
                return Err(RegistrationError::StaleRegistration);
            }
            let thread = std::thread::current().id();
            state.owner_thread = Some(thread);
            state.phase = EntryPhase::Running;
            state.rendezvous.mode = if gate.rendezvous.phase == super::rendezvous::Phase::Idle {
                super::rendezvous::Mode::Running
            } else {
                super::rendezvous::Mode::StartingStopped
            };
            Ok((local, thread))
        })();
        match result {
            Ok((local, thread)) => {
                self.armed = false;
                Ok(Mutator {
                    registry: self.registry,
                    entry: Arc::clone(&self.entry),
                    local: Some(local),
                    thread,
                    published: None,
                    finished: false,
                    run_admitted: false,
                    young_loan: false,
                    protocol: super::rendezvous::OwnerLocal::default(),
                    _thread: PhantomData,
                })
            }
            Err(reason) => Err(MutatorStartFailure {
                reason,
                permit: self,
            }),
        }
    }
}
impl Drop for RunAccess<'_, '_> {
    fn drop(&mut self) {
        if self.symbol_loan {
            // A forgotten symbol lease must not clear IN_SLICE and allow ACK.
            // Its word and the retained World stay pinned in a poisoned era.
            self.owner.registry.symbols.poison();
            self.owner.registry.poison();
            return;
        }
        if self
            .owner
            .local
            .as_ref()
            .is_some_and(|local| local.catalog_region.is_some())
        {
            // No allocator work in Drop. Retain the run obligation and all
            // backing until terminal era cleanup; never ACK an open region.
            // The admission word stays IN_SLICE: a forgotten open region
            // keeps refusing exactly like the retained loan flag did.
            self.owner.registry.poison();
            return;
        }
        self.owner.run_admitted = false;
        self.owner
            .entry
            .admission
            .store(AdmissionWord::OUT, Ordering::Release);
        // Exit half of the admission Dekker pair: an authority scan that
        // read IN_SLICE is SC-before this store in the fence total order,
        // so this load observes the published request and the owner reaches
        // the gated slow path at its slice end, not merely at its next
        // admission attempt. The load is Acquire (never removable).
        fence(Ordering::SeqCst);
        let _stop_observed = self.owner.registry.phase.load();
        if self.owner.registry.park.parked.load(Ordering::SeqCst) != 0 {
            // Loan predicates (entry admission/resume) are evaluated under the
            // gate, and the condvar wait releases exactly that gate, so
            // acquiring it here orders this notify after any in-progress park.
            // Gate holders never wait, so this block is bounded and is reached
            // only while a waiter is parked. On a gate error (poison, or the
            // impossible same-thread re-entry) the bare notify still matches
            // the previous unconditional behaviour.
            if let Ok(gate) = self.owner.registry.lock_blocking() {
                drop(gate);
            }
            self.owner.registry.park.wake.notify_all();
        }
    }
}

impl Drop for MutatorPermit<'_> {
    fn drop(&mut self) {
        if self.armed {
            // A cancelled start never acquired roots or allocation authority.
            // No wait, lock, deregistration or borrowed heap access in Drop.
            self.entry.cancelled_start.store(true, Ordering::Release);
        }
    }
}

#[derive(Clone, Copy)]
struct PublicationBinding {
    epoch: NonZeroU64,
    generation: NonZeroU64,
}

#[must_use = "finish on the original owner; abandonment poisons and retains"]
pub struct Mutator<'era> {
    pub(super) registry: &'era Registry,
    pub(super) entry: Arc<MutatorEntry>,
    pub(super) local: Option<Box<OwnerGcState>>,
    pub(super) thread: ThreadId,
    published: Option<PublicationBinding>,
    pub(super) finished: bool,
    /// Owner-local mirrors of the gated entry state, replacing the fast
    /// path's gate reads. Sound because for a Running entry every mutation
    /// site that moves `EntryPhase`/`Mode` off Running, or flips
    /// `worker_loan`, is owner-thread code; the owner cannot be surprised
    /// by its own entry between two of its own gated calls. A
    /// `mem::forget`ted `RunAccess` leaves `run_admitted` set, so the next
    /// admission refuses with `OutstandingLoan` — the fail-safe shape of
    /// the old `swap(true)` returning true, without the RMW.
    run_admitted: bool,
    young_loan: bool,
    pub(super) protocol: super::rendezvous::OwnerLocal,
    _thread: PhantomData<Rc<()>>,
}
impl std::fmt::Debug for Mutator<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Mutator").field(&self.entry.key).finish()
    }
}

pub struct MutatorFinishFailure<'era> {
    pub reason: RegistrationError,
    owner: Mutator<'era>,
}
impl<'era> MutatorFinishFailure<'era> {
    pub fn into_owner(self) -> Mutator<'era> {
        self.owner
    }
}
impl std::fmt::Debug for MutatorFinishFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MutatorFinishFailure")
            .field(&self.reason)
            .finish()
    }
}

/// A unique borrow of owner state, not an authority to dereference heap
/// objects. A4/A5 must supply the separate shared-storage/stop protocol.
pub struct RunAccess<'owner, 'era> {
    pub(super) owner: &'owner mut Mutator<'era>,
    pub(super) symbol_loan: bool,
}

/// The owner publication capability is produced before stop-all. Its unique
/// borrow excludes a simultaneous RunAccess; runtime worker loans are checked
/// again before the indivisible ownership transfer.
#[must_use]
pub struct OwnerPublication<'owner, 'era> {
    owner: &'owner mut Mutator<'era>,
    epoch: NonZeroU64,
}

impl<'owner, 'era> OwnerPublication<'owner, 'era> {
    pub fn publish(self) -> Result<(), RegistrationError> {
        self.owner.publish(self.epoch)
    }
}

impl<'era> Mutator<'era> {
    pub(super) fn check_thread(&self) -> Result<(), RegistrationError> {
        if self.thread != std::thread::current().id() {
            return Err(RegistrationError::WrongThread);
        }
        if self.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        Ok(())
    }

    pub fn key(&self) -> MutatorKey {
        self.entry.key
    }
    pub fn entry(&self) -> Arc<MutatorEntry> {
        Arc::clone(&self.entry)
    }

    /// Lock-free slice admission (the safepoint poll): zero RMWs on shared
    /// lines — one plain store to the owner-private admission word, one
    /// full fence, one plain load of the read-mostly phase word.
    ///
    /// The gated entry/rendezvous checks the old gate-held path performed
    /// are carried by the owner-local mirrors (`finished`, `published`,
    /// `run_admitted`, `young_loan`) plus the phase word. A refusal after
    /// the fence backs out to the caller's gated slow path, which is the
    /// same place the old `Busy` landed; `check_thread` on the retry then
    /// distinguishes poison.
    pub fn run_access(&mut self) -> Result<RunAccess<'_, 'era>, RegistrationError> {
        self.check_thread()?;
        if self.finished || self.published.is_some() {
            return Err(RegistrationError::Busy);
        }
        if self.run_admitted || self.young_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        let snapshot = self.registry.phase.load();
        if !PhaseLine::admissible(snapshot) {
            return Err(self.admission_backout());
        }
        self.entry.admission.store(
            AdmissionWord::in_slice(PhaseLine::epoch(snapshot)),
            Ordering::Release,
        );
        fence(Ordering::SeqCst);
        let phase = self.registry.phase.load();
        if !PhaseLine::admissible(phase) {
            return Err(self.admission_backout());
        }
        self.run_admitted = true;
        Ok(RunAccess {
            owner: self,
            symbol_loan: false,
        })
    }

    /// Cold backout: clear the admission word and report the gated refusal.
    /// Never inline — it must not inflate the admitted fast path.
    #[cold]
    #[inline(never)]
    fn admission_backout(&self) -> RegistrationError {
        #[cfg(test)]
        a8_probes::mark(a8_probes::ADMISSION_BACKOUT, 0, self.entry.key.owner);
        self.entry
            .admission
            .store(AdmissionWord::OUT, Ordering::Release);
        RegistrationError::Busy
    }

    pub fn begin_publication(
        &mut self,
        epoch: NonZeroU64,
    ) -> Result<OwnerPublication<'_, 'era>, RegistrationError> {
        self.check_thread()?;
        if self.published.is_some() {
            return Err(RegistrationError::Busy);
        }
        Ok(OwnerPublication { owner: self, epoch })
    }

    fn publish(&mut self, epoch: NonZeroU64) -> Result<(), RegistrationError> {
        self.check_thread()?;
        let gate = self.registry.lock()?;
        let entry = self.registry.validate(&gate, self.entry.key)?;
        let mut state = entry.state.try_lock().map_err(entry_lock_error)?;
        if self.protocol.participating
            || gate.rendezvous.phase != super::rendezvous::Phase::Idle
            || state.rendezvous.mode != super::rendezvous::Mode::Running
        {
            return Err(RegistrationError::Busy);
        }
        if self.entry.admitted() {
            return Err(RegistrationError::OutstandingLoan);
        }

        if state.phase != EntryPhase::Running || self.published.is_some() {
            return Err(RegistrationError::Busy);
        }
        if state.worker_loan || state.run_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        if epoch.get() <= state.epoch {
            return Err(RegistrationError::Publication);
        }
        let next = state
            .publication_generation
            .checked_add(1)
            .and_then(NonZeroU64::new)
            .ok_or(RegistrationError::Exhausted)?;
        // Every fallible identity/loan/phase check precedes moving any log.
        // Held cons tails publish back to the catalog before the image does.
        self.publish_cons_tails()?;
        let image = self
            .local
            .as_mut()
            .expect("owner shells remain local")
            .publish()?;
        state.image = Some(image);
        state.publication_generation = next.get();
        state.epoch = epoch.get();
        state.phase = EntryPhase::Published;
        self.published = Some(PublicationBinding {
            epoch,
            generation: next,
        });
        Ok(())
    }

    /// Metadata-only image restoration. It grants no heap or collection
    /// authority; A4 must add its matching resume capability at integration.
    pub(crate) fn restore(&mut self, epoch: NonZeroU64) -> Result<(), RegistrationError> {
        self.check_thread()?;
        let binding = self.published.ok_or(RegistrationError::Publication)?;
        if binding.epoch != epoch {
            return Err(RegistrationError::Publication);
        }
        let gate = self.registry.lock()?;
        let entry = self.registry.validate(&gate, self.entry.key)?;
        let mut state = entry.state.try_lock().map_err(entry_lock_error)?;
        if self.protocol.participating || gate.rendezvous.phase != super::rendezvous::Phase::Idle {
            return Err(RegistrationError::Busy);
        }

        if state.phase != EntryPhase::Published
            || state.epoch != epoch.get()
            || state.publication_generation != binding.generation.get()
        {
            return Err(RegistrationError::Publication);
        }
        if state.readers != 0 || state.worker_loan {
            return Err(RegistrationError::OutstandingLoan);
        }
        let image = state.image.take().ok_or(RegistrationError::Publication)?;
        if let Err(failure) = self.local.as_mut().expect("local shells").restore(image) {
            let reason = failure.error.into();
            state.image = Some(failure.image);
            return Err(reason);
        }
        state.phase = EntryPhase::Running;
        self.published = None;
        Ok(())
    }

    pub fn finish(mut self, epoch: NonZeroU64) -> Result<(), MutatorFinishFailure<'era>> {
        let result = (|| {
            self.check_thread()?;
            if self.published.is_none() {
                self.publish(epoch)?;
            }
            let binding = self.published.ok_or(RegistrationError::Publication)?;
            if binding.epoch != epoch {
                return Err(RegistrationError::Publication);
            }
            let gate = self.registry.lock()?;
            let entry = self.registry.validate(&gate, self.entry.key)?;
            let mut state = entry.state.try_lock().map_err(entry_lock_error)?;
            if state.phase != EntryPhase::Published || state.worker_loan || state.run_loan {
                return Err(RegistrationError::OutstandingLoan);
            }
            state.phase = EntryPhase::Retiring;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.finished = true;
                Ok(())
            }
            Err(reason) => Err(MutatorFinishFailure {
                reason,
                owner: self,
            }),
        }
    }

    pub(crate) fn take_young_loan(&mut self) -> Result<YoungWorkerLoan<'era>, RegistrationError> {
        self.check_thread()?;
        let gate = self.registry.lock()?;
        let entry = self.registry.validate(&gate, self.entry.key)?;
        let mut state = entry.state.try_lock().map_err(entry_lock_error)?;
        if state.phase != EntryPhase::Running
            || state.worker_loan
            || state.run_loan
            || self.entry.admitted()
        {
            return Err(RegistrationError::OutstandingLoan);
        }
        if state.rendezvous.mode != super::rendezvous::Mode::Running
            || matches!(
                gate.rendezvous.phase,
                super::rendezvous::Phase::Stopping(_) | super::rendezvous::Phase::Tracing(_)
            )
        {
            return Err(RegistrationError::Busy);
        }
        let log = self
            .local
            .as_mut()
            .expect("local owner")
            .cold
            .young
            .take_owner_payload()?;
        state.worker_loan = true;
        self.young_loan = true;
        Ok(YoungWorkerLoan {
            registry: self.registry,
            key: self.entry.key,
            log: Some(log),
            _unshared: PhantomData,
        })
    }

    pub(crate) fn return_young_loan(
        &mut self,
        mut loan: YoungWorkerLoan<'era>,
    ) -> Result<(), YoungLoanFailure<'era>> {
        let result = (|| {
            self.check_thread()?;
            if !std::ptr::eq(self.registry, loan.registry) || self.entry.key.heap != loan.key.heap {
                return Err(RegistrationError::WrongHeap);
            }
            if self.entry.key.owner != loan.key.owner {
                return Err(RegistrationError::WrongOwner);
            }
            if self.entry.key != loan.key {
                return Err(RegistrationError::StaleRegistration);
            }
            let gate = self.registry.lock()?;
            let entry = self.registry.validate(&gate, self.entry.key)?;
            let mut state = entry.state.try_lock().map_err(entry_lock_error)?;
            if !state.worker_loan || state.phase != EntryPhase::Running {
                return Err(RegistrationError::OutstandingLoan);
            }
            let log = loan.log.take().ok_or(RegistrationError::OutstandingLoan)?;
            if let Err((error, log)) = self
                .local
                .as_mut()
                .expect("local owner")
                .cold
                .young
                .restore_owner_payload(log)
            {
                loan.log = Some(log);
                return Err(error.into());
            }
            state.worker_loan = false;
            self.young_loan = false;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(()),
            Err(reason) => Err(YoungLoanFailure { reason, loan }),
        }
    }
}

impl RunAccess<'_, '_> {
    pub(in crate::tagged::gc) fn state(&mut self) -> &mut OwnerGcState {
        self.owner.local.as_mut().expect("resident owner state")
    }
}

impl Drop for Mutator<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.registry.poison();
            if let Some(local) = self.local.take() {
                // Owner-confined storage stays on its original thread.
                std::mem::forget(local);
            }
        }
    }
}

/// Transfer only the payload, never the owner's SATB buffer or hot state.
/// A lost worker poisons instead of returning or losing its canonical log.
pub struct YoungWorkerLoan<'era> {
    registry: &'era Registry,
    key: MutatorKey,
    log: Option<YoungBlockLog>,
    _unshared: PhantomData<Cell<()>>,
}
pub(crate) struct YoungLoanFailure<'era> {
    pub(crate) reason: RegistrationError,
    pub(crate) loan: YoungWorkerLoan<'era>,
}
impl<'era> YoungLoanFailure<'era> {
    pub(crate) fn into_loan(self) -> YoungWorkerLoan<'era> {
        self.loan
    }
}
impl std::fmt::Debug for YoungLoanFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("YoungLoanFailure")
            .field(&self.reason)
            .finish()
    }
}
impl Drop for YoungWorkerLoan<'_> {
    fn drop(&mut self) {
        if let Some(log) = self.log.take() {
            self.registry.poison();
            std::mem::forget(log);
        }
    }
}

static_assertions::assert_impl_all!(MutatorPermit<'static>: Send);
static_assertions::assert_not_impl_any!(MutatorPermit<'static>: Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(Mutator<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(OwnerPublication<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(RunAccess<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(YoungWorkerLoan<'static>: Send);
static_assertions::assert_not_impl_any!(YoungWorkerLoan<'static>: Sync, Clone, Copy);

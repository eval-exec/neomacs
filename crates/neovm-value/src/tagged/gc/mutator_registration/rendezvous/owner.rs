//! Exact-root publication, then owner-side parking. No heap or World access.
#[cfg(any(test, feature = "test-support"))]
use super::super::a8_probes;
use super::*;
use crate::tagged::transport::PreparedParallelRootBatch;

/// What the engine does after one poll.
///
/// `Continue` did not park: the phase word was admissible, this owner is the
/// keeper, or a non-admissible phase word (`Phase::DrainingResumes`, a
/// transient catalog `BLOCKED_BIT`, or a raced `Phase::Idle`) had no active
/// stop targeting this owner under the canonical gate. `Resumed` is only
/// returned after image restore. Parking stays inside the poll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use = "a resumed owner has a restored image; continuing does not"]
pub enum PollOutcome {
    Continue,
    Resumed { epoch: NonZeroU64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PollTarget {
    Continue,
    Stop(NonZeroU64),
    RetryLock,
}

/// Classify `(phase, mode)` under the canonical gate for `Mutator::poll`.
///
/// Fails closed on broken stop-protocol invariants: only a `StartingStopped`
/// owner can legitimately observe `Stopping(_)` outside `frozen` or observe
/// `Tracing(_)`; a `Running` owner there returns `Identity` / `Publication`.
/// Non-`Running`/`StartingStopped` modes return a non-retryable `Busy` refusal
/// (only this thread can change its own mode, so retrying would spin forever).
fn classify_poll_target(
    phase: Phase,
    mode: Mode,
    is_frozen: bool,
    request_epoch: u64,
) -> Result<PollTarget, RendezvousError> {
    match (phase, mode) {
        (Phase::Poisoned, _) => Err(RendezvousError::Poisoned),
        (Phase::Stopping(epoch), Mode::Running) => {
            if !is_frozen {
                return Err(RendezvousError::Identity);
            }
            if request_epoch != epoch.get() {
                return Err(RendezvousError::Publication);
            }
            Ok(PollTarget::Stop(epoch))
        }
        (Phase::Tracing(_), Mode::Running) => Err(RendezvousError::Publication),
        (Phase::Idle | Phase::DrainingResumes, Mode::Running) => Ok(PollTarget::Continue),
        (
            Phase::Idle | Phase::DrainingResumes | Phase::Stopping(_) | Phase::Tracing(_),
            Mode::StartingStopped,
        ) => Ok(PollTarget::Continue),
        (
            _,
            Mode::NativeStopped | Mode::Publishing | Mode::Stopped | Mode::Resuming | Mode::Retired,
        ) => Err(RendezvousError::Busy),
    }
}

#[inline(always)]
fn phase_admissible(registry: &super::super::Registry) -> bool {
    super::super::admission::PhaseLine::admissible(registry.phase.load())
}

impl<'era> Mutator<'era> {
    /// Acquire the real request, never a relaxed attention hint. No wait here.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn requested_epoch(&self) -> Result<Option<NonZeroU64>, RendezvousError> {
        self.check_thread()?;
        let gate = self.registry.lock()?;
        // request() publishes the phase and word under this same gate. Read
        // the word only after admission: a pre-gate sample could precede a
        // new request and falsely refuse its coherent Stopping phase.
        let request = self.registry.request_epoch.load(Ordering::Acquire);
        let e = entry_state(&self.entry)?;
        match gate.rendezvous.phase {
            Phase::Stopping(epoch) if gate.rendezvous.frozen.contains(&self.entry.key) => {
                if request != epoch.get() || e.rendezvous.mode != Mode::Running {
                    return Err(RendezvousError::Publication);
                }
                Ok(Some(epoch))
            }
            _ => Ok(None),
        }
    }
    pub(crate) fn request_collection(&self) -> Result<Request, RendezvousError> {
        self.check_thread()?;
        request(self.registry)
    }
    /// StartingStopped cannot obtain a run slice until its bootstrap is opened.
    pub(crate) fn wait_started(&mut self) -> Result<(), RendezvousError> {
        self.check_thread()?;
        let _park = self.registry.park_ticket();
        let mut gate = self.registry.lock()?;
        loop {
            let mode = entry_state(&self.entry)?.rendezvous.mode;
            if mode == Mode::Running {
                return Ok(());
            }
            if mode != Mode::StartingStopped {
                return Err(RendezvousError::Busy);
            }
            if self.thread == self.registry.keeper_thread
                && matches!(
                    gate.rendezvous.phase,
                    Phase::Stopping(_) | Phase::Tracing(_)
                )
            {
                return Err(RendezvousError::Busy);
            }
            gate = wait(self.registry, gate)?;
        }
    }
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn wait_request(&self) -> Result<NonZeroU64, RendezvousError> {
        self.check_thread()?;
        let _park = self.registry.park_ticket();
        let mut gate = self.registry.lock()?;
        loop {
            let e = entry_state(&self.entry)?;
            if !matches!(e.rendezvous.mode, Mode::Running | Mode::StartingStopped)
                || (e.rendezvous.mode == Mode::StartingStopped
                    && self.thread == self.registry.keeper_thread
                    && matches!(
                        gate.rendezvous.phase,
                        Phase::Stopping(_) | Phase::Tracing(_)
                    ))
            {
                return Err(RendezvousError::Busy);
            }
            if let Phase::Stopping(epoch) = gate.rendezvous.phase {
                if gate.rendezvous.frozen.contains(&self.entry.key)
                    && e.rendezvous.mode == Mode::Running
                {
                    if self.registry.request_epoch.load(Ordering::Acquire) != epoch.get() {
                        return Err(RendezvousError::Publication);
                    }
                    return Ok(epoch);
                }
            }
            drop(e);
            gate = wait(self.registry, gate)?;
        }
    }

    /// [`Self::wait_request`] with a deadline. Expiry is `Busy`. A real stop
    /// still returns the epoch, and a panic poison still returns `Poisoned`.
    /// Production parking stays on the unbounded wait.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn wait_request_for(
        &self,
        budget: std::time::Duration,
    ) -> Result<NonZeroU64, RendezvousError> {
        self.check_thread()?;
        let _park = self.registry.park_ticket();
        let mut gate = self.registry.lock()?;
        let deadline = std::time::Instant::now() + budget;
        loop {
            let e = entry_state(&self.entry)?;
            if !matches!(e.rendezvous.mode, Mode::Running | Mode::StartingStopped)
                || (e.rendezvous.mode == Mode::StartingStopped
                    && self.thread == self.registry.keeper_thread
                    && matches!(
                        gate.rendezvous.phase,
                        Phase::Stopping(_) | Phase::Tracing(_)
                    ))
            {
                return Err(RendezvousError::Busy);
            }
            if let Phase::Stopping(epoch) = gate.rendezvous.phase {
                if gate.rendezvous.frozen.contains(&self.entry.key)
                    && e.rendezvous.mode == Mode::Running
                {
                    if self.registry.request_epoch.load(Ordering::Acquire) != epoch.get() {
                        return Err(RendezvousError::Publication);
                    }
                    return Ok(epoch);
                }
            }
            drop(e);
            if std::time::Instant::now() >= deadline {
                return Err(RendezvousError::Busy);
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            gate = wait_timeout(self.registry, gate, left)?;
        }
    }
    /// A stale/duplicate ACK is a refusal, never a count toward another epoch.
    pub fn acknowledge(
        &mut self,
        epoch: NonZeroU64,
        roots: &[SharedRoot],
    ) -> Result<(), RendezvousError> {
        self.publish_roots(Some(epoch), false, roots)
    }
    pub fn enter_native(&mut self, roots: &[SharedRoot]) -> Result<(), RendezvousError> {
        self.publish_roots(None, true, roots)
    }
    pub(crate) fn park_once(&mut self, roots: &[SharedRoot]) -> Result<u64, RendezvousError> {
        let epoch = self.wait_request()?;
        self.acknowledge(epoch, roots)?;
        self.restore_wait(false)
    }

    /// Safepoint poll. The admissible path is one Acquire load of the phase
    /// word and does not call `roots`, take the gate, or allocate.
    ///
    /// When the phase word is not admissible and this owner is not the keeper,
    /// `poll_stopped` checks under the canonical gate (`poll_target_epoch`)
    /// whether a `Phase::Stopping(epoch)` actually targets this owner. A
    /// non-targeting word (`Phase::DrainingResumes`, a transient catalog
    /// `BLOCKED_BIT`, or a raced `Phase::Idle`) returns `Ok(PollOutcome::Continue)`
    /// without calling `roots`. Otherwise `roots` runs once after the targeting
    /// epoch is confirmed; `Busy` retries on `acknowledge` reuse that batch,
    /// and a closure error returns before `acknowledge`.
    #[inline(never)]
    pub fn poll(
        &mut self,
        roots: impl FnOnce() -> Result<Vec<SharedRoot>, RendezvousError>,
    ) -> Result<PollOutcome, RendezvousError> {
        // Poison and the wrong thread stay on the park line. This load is the
        // only shared line the fast path touches; the slow path reports them.
        if phase_admissible(self.registry) {
            return Ok(PollOutcome::Continue);
        }
        self.poll_stopped(roots)
    }

    /// Check under the canonical gate whether an active stop epoch targets
    /// this owner. Distinguishes transient gate/entry lock contention
    /// (`PollTarget::RetryLock`) from non-retryable owner mode refusals
    /// (`Err(RendezvousError::Busy)`) and fail-closed stop-protocol errors
    /// (`Err(Identity)` / `Err(Publication)`).
    fn poll_target_epoch(&self) -> Result<PollTarget, RendezvousError> {
        self.check_thread()?;
        let gate = match self.registry.lock() {
            Ok(gate) => gate,
            Err(RegistrationError::Busy) => return Ok(PollTarget::RetryLock),
            Err(error) => return Err(error.into()),
        };
        let request = self.registry.request_epoch.load(Ordering::Acquire);
        let e = match entry_state(&self.entry) {
            Ok(entry) => entry,
            Err(RendezvousError::Busy) => return Ok(PollTarget::RetryLock),
            Err(error) => return Err(error),
        };
        classify_poll_target(
            gate.rendezvous.phase,
            e.rendezvous.mode,
            gate.rendezvous.frozen.contains(&self.entry.key),
            request,
        )
    }

    #[cold]
    #[inline(never)]
    fn poll_stopped(
        &mut self,
        roots: impl FnOnce() -> Result<Vec<SharedRoot>, RendezvousError>,
    ) -> Result<PollOutcome, RendezvousError> {
        // Same identity `keeper()` compares. A caller-supplied flag can be wrong.
        if self.thread == self.registry.keeper_thread {
            return Ok(PollOutcome::Continue);
        }
        // `!phase_admissible` also fires during `Phase::DrainingResumes`
        // (after `end_epoch` while a peer is still exiting `restore_wait`) or
        // a transient catalog `BLOCKED_BIT`. Verify under the gate that a stop
        // epoch actually targets this owner before building roots or parking:
        // otherwise a resumed owner polling during `DrainingResumes` would
        // enter `wait_request()` and deadlock once the era returns to `Idle`.
        let epoch = loop {
            match self.poll_target_epoch()? {
                PollTarget::Stop(epoch) => break epoch,
                PollTarget::Continue => return Ok(PollOutcome::Continue),
                PollTarget::RetryLock => std::thread::yield_now(),
            }
        };
        let batch = roots()?;
        loop {
            match self.acknowledge(epoch, &batch) {
                Ok(()) => break,
                Err(RendezvousError::Busy) => std::thread::yield_now(),
                Err(error) => return Err(error),
            }
        }
        // `restore_wait` already retries its own Busy and does not re-acknowledge.
        let restored = self.restore_wait(false)?;
        match NonZeroU64::new(restored) {
            Some(epoch) => Ok(PollOutcome::Resumed { epoch }),
            None => Err(RendezvousError::Publication),
        }
    }
    /// Publish terminated-owner roots before leaving. The canonical entry and
    /// its image remain retained and participate in later frozen memberships.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn retire_rooted(
        mut self,
        roots: &[SharedRoot],
    ) -> Result<(), RootedRetireFailure<'era>> {
        let result = (|| {
            // Refuse before transitioning mode if this registration still
            // holds an open owner-cons run.
            self.registry
                .check_owner_cons_closed(self.entry.key.registration)?;
            // Every Running/Resuming entry access shares the canonical gate.
            // An ungated mode probe could make another owner's post-restore
            // membership census fail Busy after its image had already moved.
            let already_native = {
                let gate = self.registry.lock()?;
                self.registry.validate(&gate, self.entry.key)?;
                let current = entry_state(&self.entry)?;
                current.rendezvous.mode == Mode::NativeStopped
            };
            if !already_native {
                self.publish_roots(None, true, roots)?;
            }
            let gate = self.registry.lock()?;
            self.registry.validate(&gate, self.entry.key)?;
            let mut e = entry_state(&self.entry)?;
            if e.rendezvous.mode != Mode::NativeStopped {
                return Err(RendezvousError::Busy);
            }
            // The owner thread cannot borrow the collector. Refuse if a cons
            // run is still open, skip owners that hold no lease, and queue
            // closed leases for `reclaim_retired_owner_cons`.
            self.registry
                .note_retired_owner_cons(self.entry.key.registration)?;
            e.rendezvous.retiring = true;
            e.phase = EntryPhase::Retiring;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.finished = true;
                Ok(())
            }
            Err(reason) => Err(RootedRetireFailure {
                reason,
                owner: self,
            }),
        }
    }

    fn publish_roots(
        &mut self,
        wanted: Option<NonZeroU64>,
        native: bool,
        roots: &[SharedRoot],
    ) -> Result<(), RendezvousError> {
        self.check_thread()?;
        #[cfg(any(test, feature = "test-support"))]
        a8_probes::mark(
            a8_probes::PUBLISH_ENTERED,
            wanted.map(NonZeroU64::get).unwrap_or(0),
            self.entry.key.owner,
        );
        for root in roots {
            if root.heap_identity() != self.entry.key.heap {
                return Err(RendezvousError::Identity);
            }
        }
        // Reserve host memory before claiming Publishing. All source handles
        // stay rooted across this operation; no raw Value crosses a thread.
        let (generation, epoch) = {
            let gate = self.registry.lock()?;
            self.registry.validate(&gate, self.entry.key)?;
            let mut e = entry_state(&self.entry)?;
            if e.phase != EntryPhase::Running
                || e.rendezvous.mode != Mode::Running
                || e.image.is_some()
            {
                return Err(RendezvousError::Busy);
            }
            if e.run_loan || e.worker_loan || self.entry.admitted() {
                return Err(RendezvousError::Loan);
            }
            let epoch = match gate.rendezvous.phase {
                Phase::Stopping(epoch) => {
                    if !native && wanted != Some(epoch) {
                        return Err(RendezvousError::Publication);
                    }
                    epoch.get()
                }
                Phase::Idle | Phase::DrainingResumes if native => gate.rendezvous.completed,
                _ => return Err(RendezvousError::Busy),
            };
            if !native && !gate.rendezvous.frozen.contains(&self.entry.key) {
                return Err(RendezvousError::Identity);
            }
            let generation = e
                .publication_generation
                .checked_add(1)
                .and_then(NonZeroU64::new)
                .ok_or(RendezvousError::Generation)?;
            e.retained_roots
                .try_reserve(1)
                .map_err(|_| RendezvousError::Capacity)?;
            (generation, epoch)
        };
        // Held cons tails are owner-exclusive storage: every unhanded suffix
        // returns under the catalog gate before this image can publish. The
        // loan checks above prove no run borrow can hold a tail right now.
        self.publish_cons_tails()?;
        self.local
            .as_ref()
            .ok_or(RendezvousError::Publication)?
            .validate_publication()
            .map_err(RegistrationError::from)?;
        #[cfg(any(test, feature = "test-support"))]
        a8_probes::mark(a8_probes::REGION_CLOSED, epoch, self.entry.key.owner);
        let identity = ParallelRootIdentity::new(self.entry.key, generation, epoch);
        let retention = self
            .registry
            .root_epoch
            .retain()
            .map_err(|_| RendezvousError::Generation)?;
        let prepared = PreparedParallelRootBatch::reserve(identity, retention, roots.len())?;
        {
            let gate = self.registry.lock()?;
            let mut e = entry_state(&self.entry)?;
            if e.rendezvous.mode != Mode::Running || e.phase != EntryPhase::Running {
                return Err(RendezvousError::Busy);
            }
            if e.publication_generation.checked_add(1) != Some(generation.get()) {
                return Err(RendezvousError::Generation);
            }
            if !native
                && gate.rendezvous.phase
                    != Phase::Stopping(wanted.ok_or(RendezvousError::Publication)?)
            {
                return Err(RendezvousError::Publication);
            }
            if matches!(gate.rendezvous.phase, Phase::Tracing(_)) {
                return Err(RendezvousError::Busy);
            }
            if e.run_loan || e.worker_loan || self.entry.admitted() {
                return Err(RendezvousError::Loan);
            }
            e.rendezvous.mode = Mode::Publishing;
        }
        #[cfg(any(test, feature = "test-support"))]
        if let Some(hook) = self.protocol.hooks.take_publication() {
            hook();
        }
        let witness = OwnerRootAdmission {
            identity,
            roots,
            _owner: PhantomData,
            _thread: PhantomData,
        };
        let publication = match prepared.publish(&witness) {
            Ok(lease) => lease,
            Err(failure) => {
                let (_, error) = failure.into_parts();
                self.registry.poison();
                return Err(error.into());
            }
        };
        let image = match self
            .local
            .as_mut()
            .ok_or(RendezvousError::Publication)?
            .publish()
        {
            Ok(image) => image,
            Err(error) => {
                std::mem::forget(publication);
                self.registry.poison();
                return Err(RegistrationError::from(error).into());
            }
        };
        #[cfg(any(test, feature = "test-support"))]
        a8_probes::mark(a8_probes::HOSTALLOC_DONE, epoch, self.entry.key.owner);
        // From this point a failure must retain BOTH payloads. Their Drop cannot
        // release the root lease, but the GC image must be retained explicitly.
        let commit = loop {
            let attempt = (|| {
                let gate = self.registry.lock()?;
                self.registry.validate(&gate, self.entry.key)?;
                let e = entry_state(&self.entry)?;
                if e.rendezvous.mode != Mode::Publishing
                    || e.publication_generation.checked_add(1) != Some(generation.get())
                {
                    return Err(RendezvousError::Publication);
                }
                if !native
                    && gate.rendezvous.phase
                        != Phase::Stopping(wanted.ok_or(RendezvousError::Publication)?)
                {
                    return Err(RendezvousError::Publication);
                }
                Ok((gate, e))
            })();
            match attempt {
                Err(RendezvousError::Busy) => {
                    std::thread::yield_now();
                }
                result => break result,
            }
        };
        let (gate, mut e) = match commit {
            Ok(pair) => pair,
            Err(error) => {
                std::mem::forget(image);
                std::mem::forget(publication);
                self.registry.poison();
                return Err(error);
            }
        };
        if let Some(old) = e.roots.replace(publication) {
            e.retained_roots.push(old);
        }
        e.rendezvous.image_epoch = epoch;
        e.image = Some(image);
        e.publication_generation = generation.get();
        e.epoch = epoch;
        e.phase = EntryPhase::Published;
        e.rendezvous.coverage_epoch = match gate.rendezvous.phase {
            Phase::Stopping(current) => current.get(),
            _ => epoch,
        };
        e.rendezvous.mode = if native {
            Mode::NativeStopped
        } else {
            Mode::Stopped
        };
        self.protocol.participating = true;
        #[cfg(any(test, feature = "test-support"))]
        a8_probes::mark(
            if native {
                a8_probes::NATIVE_ACK
            } else {
                a8_probes::ACK_COMMITTED
            },
            epoch,
            self.entry.key.owner,
        );
        drop(e);
        drop(gate);
        self.registry.park.wake.notify_all();
        Ok(())
    }
}

pub struct RootedRetireFailure<'era> {
    pub reason: RendezvousError,
    owner: Mutator<'era>,
}
impl<'era> RootedRetireFailure<'era> {
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn into_owner(self) -> Mutator<'era> {
        self.owner
    }
}
impl std::fmt::Debug for RootedRetireFailure<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RootedRetireFailure")
            .field(&self.reason)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_poll_target_fails_closed_on_running_and_non_running_modes() {
        let ep1 = NonZeroU64::new(1).unwrap();

        // Targeting stop for a frozen Running owner.
        assert_eq!(
            classify_poll_target(Phase::Stopping(ep1), Mode::Running, true, 1),
            Ok(PollTarget::Stop(ep1))
        );
        // Running owner missing from frozen during Stopping -> fail closed (Identity).
        assert_eq!(
            classify_poll_target(Phase::Stopping(ep1), Mode::Running, false, 1),
            Err(RendezvousError::Identity)
        );
        // Request epoch mismatch -> fail closed (Publication).
        assert_eq!(
            classify_poll_target(Phase::Stopping(ep1), Mode::Running, true, 0),
            Err(RendezvousError::Publication)
        );
        // Running owner observing Tracing -> fail closed (Publication).
        assert_eq!(
            classify_poll_target(Phase::Tracing(ep1), Mode::Running, true, 1),
            Err(RendezvousError::Publication)
        );
        assert_eq!(
            classify_poll_target(Phase::Tracing(ep1), Mode::Running, false, 1),
            Err(RendezvousError::Publication)
        );

        // Idle / DrainingResumes for Running -> Continue without parking.
        assert_eq!(
            classify_poll_target(Phase::Idle, Mode::Running, false, 0),
            Ok(PollTarget::Continue)
        );
        assert_eq!(
            classify_poll_target(Phase::DrainingResumes, Mode::Running, false, 1),
            Ok(PollTarget::Continue)
        );

        // StartingStopped may observe any non-poisoned phase outside frozen -> Continue.
        for phase in [
            Phase::Idle,
            Phase::DrainingResumes,
            Phase::Stopping(ep1),
            Phase::Tracing(ep1),
        ] {
            assert_eq!(
                classify_poll_target(phase, Mode::StartingStopped, false, 1),
                Ok(PollTarget::Continue)
            );
        }

        // Poison always refuses regardless of mode.
        for mode in [
            Mode::StartingStopped,
            Mode::Running,
            Mode::NativeStopped,
            Mode::Publishing,
            Mode::Stopped,
            Mode::Resuming,
            Mode::Retired,
        ] {
            assert_eq!(
                classify_poll_target(Phase::Poisoned, mode, false, 1),
                Err(RendezvousError::Poisoned)
            );
        }

        // Non-Running/StartingStopped modes return non-retryable Busy (LOW-2).
        for mode in [
            Mode::NativeStopped,
            Mode::Publishing,
            Mode::Stopped,
            Mode::Resuming,
            Mode::Retired,
        ] {
            for phase in [
                Phase::Idle,
                Phase::DrainingResumes,
                Phase::Stopping(ep1),
                Phase::Tracing(ep1),
            ] {
                assert_eq!(
                    classify_poll_target(phase, mode, true, 1),
                    Err(RendezvousError::Busy)
                );
            }
        }
    }
}

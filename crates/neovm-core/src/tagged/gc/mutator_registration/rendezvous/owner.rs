//! Exact-root publication, then owner-side parking. No heap or World access.
#[cfg(test)]
use super::super::a8_probes;
use super::*;
use crate::tagged::transport::PreparedParallelRootBatch;

impl<'era> Mutator<'era> {
    /// Acquire the real request, never a relaxed attention hint. No wait here.
    pub fn requested_epoch(&self) -> Result<Option<NonZeroU64>, RendezvousError> {
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
    pub fn request_collection(&self) -> Result<Request, RendezvousError> {
        self.check_thread()?;
        request(self.registry)
    }
    /// StartingStopped cannot obtain a run slice until its bootstrap is opened.
    pub fn wait_started(&mut self) -> Result<(), RendezvousError> {
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
    pub fn wait_request(&self) -> Result<NonZeroU64, RendezvousError> {
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
    pub fn park_once(&mut self, roots: &[SharedRoot]) -> Result<u64, RendezvousError> {
        let epoch = self.wait_request()?;
        self.acknowledge(epoch, roots)?;
        self.restore_wait(false)
    }
    /// Publish terminated-owner roots before leaving. The canonical entry and
    /// its image remain retained and participate in later frozen memberships.
    pub fn retire_rooted(mut self, roots: &[SharedRoot]) -> Result<(), RootedRetireFailure<'era>> {
        let result = (|| {
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
        #[cfg(test)]
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
        #[cfg(test)]
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
        #[cfg(test)]
        if let Some(hook) = self.protocol.publication_hook.take() {
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
        #[cfg(test)]
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
        #[cfg(test)]
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
    pub fn into_owner(self) -> Mutator<'era> {
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

//! Unique keeper admission and epoch-bounded stopped access.

use super::sync::atomic::Ordering;
use std::convert::Infallible;
use std::fmt;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(any(test, feature = "test-support"))]
use super::super::a8_probes;
use super::super::image::PublishedGcImage;
use super::super::{
    EraCore, EraKeeper, MutatorEntry, MutatorKey, RegistrationError, Registry, RegistryGuard,
    RegistryState,
};
use super::control::{Mode, Phase};
use super::{
    RendezvousError, StoppedRootAdmission, entry_state, finish_debt, membership, request,
    verify_stopped, wait, wait_timeout,
};
use crate::tagged::gc::scan_contract::ParallelStoppedMutatorsProof;
use crate::tagged::transport::ParallelRootBatchReader;

fn registration_error(error: RegistrationError) -> RendezvousError {
    error.into()
}

fn keeper(registry: &Registry) -> Result<(), RendezvousError> {
    keeper_thread(registry)
}

/// The keeper-thread check, for the rendezvous entry points outside a stop.
pub(super) fn keeper_thread(registry: &Registry) -> Result<(), RendezvousError> {
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

/// One serial grant issued under a keeper's stop (L6.7 K3). Opaque and
/// keeper-thread confined; `AllStopped::retire_serial_grant` consumes it.
#[must_use = "retire the grant once its ticket has come back"]
#[doc(hidden)]
#[derive(Debug)]
pub struct SerialGrant {
    id: super::control::RequestId,
    owner: MutatorKey,
    _thread: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(SerialGrant: Send, Sync, Clone, Copy);

impl SerialGrant {
    /// The owner this grant's ticket goes to.
    #[doc(hidden)]
    pub fn owner(&self) -> MutatorKey {
        self.owner
    }
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
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn into_stopped(self) -> AllStopped<'era> {
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

/// One bounded attempt at the keeper's stop (L6.7 K8).
#[must_use = "a stopped epoch must be completed; a pending one still stands"]
#[derive(Debug)]
pub enum StopAttempt<'era> {
    Stopped(AllStopped<'era>),
    /// The bound passed before every owner stopped.
    Pending(StopPending),
}

/// A stop the owners have not all met yet. The request stands: owners that
/// stopped stay stopped, and the next attempt continues the same epoch.
/// Diagnostic data only; it carries no authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopPending {
    epoch: Option<NonZeroU64>,
    waiting_on: Vec<MutatorKey>,
}

impl StopPending {
    /// The standing epoch, or `None` while the request waits for the last
    /// stop's resumes to drain.
    pub fn epoch(&self) -> Option<NonZeroU64> {
        self.epoch
    }

    /// The owners the attempt was still waiting on.
    pub fn waiting_on(&self) -> &[MutatorKey] {
        &self.waiting_on
    }
}

impl<K: EraKeeper> EraCore<'_, K> {
    pub(crate) fn try_stop(&self) -> Result<Option<AllStopped<'_>>, RendezvousError> {
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

    /// [`Self::stop`] bounded by BUDGET (L6.7 K8). When the bound passes
    /// first, the attempt is pending and the request stands: the owners that
    /// stopped stay stopped, and the next attempt continues the same epoch.
    pub(crate) fn stop_within(&self, budget: Duration) -> Result<StopAttempt<'_>, RendezvousError> {
        Ok(match self.stop_by(&Until(Instant::now() + budget))? {
            Ok(stopped) => StopAttempt::Stopped(stopped),
            Err(((), pending)) => StopAttempt::Pending(pending),
        })
    }

    pub(crate) fn stop(&self) -> Result<AllStopped<'_>, RendezvousError> {
        match self.stop_by(&Unbounded)? {
            Ok(stopped) => Ok(stopped),
            Err((never, _)) => match never {},
        }
    }

    fn stop_by<B: StopBound>(
        &self,
        bound: &B,
    ) -> Result<Result<AllStopped<'_>, (B::Expired, StopPending)>, RendezvousError> {
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
                        return Ok(Ok(AllStopped {
                            registry,
                            epoch,
                            traced: false,
                            finished: false,
                            _thread: PhantomData,
                        }));
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
            if let Some(expired) = bound.expired() {
                return Ok(Err((expired, pending(&gate)?)));
            }
            // Event-driven wait atomically releases this predicate mutex.
            // Poison uses that same gate; a wake supplies no stop authority.
            gate = bound.wait(registry, gate)?;
        }
    }
}

/// How long one stop attempt waits for its owners.
trait StopBound {
    /// What the attempt yields when the bound passes first.
    type Expired;
    fn expired(&self) -> Option<Self::Expired>;
    fn wait<'a>(
        &self,
        registry: &'a Registry,
        gate: RegistryGuard<'a>,
    ) -> Result<RegistryGuard<'a>, RendezvousError>;
}

/// [`EraCore::stop`]: wait as long as the owners take.
struct Unbounded;

impl StopBound for Unbounded {
    type Expired = Infallible;

    fn expired(&self) -> Option<Infallible> {
        None
    }

    fn wait<'a>(
        &self,
        registry: &'a Registry,
        gate: RegistryGuard<'a>,
    ) -> Result<RegistryGuard<'a>, RendezvousError> {
        wait(registry, gate)
    }
}

/// [`EraCore::stop_within`]: give up waiting at the deadline.
struct Until(Instant);

impl StopBound for Until {
    type Expired = ();

    fn expired(&self) -> Option<()> {
        (Instant::now() >= self.0).then_some(())
    }

    fn wait<'a>(
        &self,
        registry: &'a Registry,
        gate: RegistryGuard<'a>,
    ) -> Result<RegistryGuard<'a>, RendezvousError> {
        wait_timeout(
            registry,
            gate,
            self.0.saturating_duration_since(Instant::now()),
        )
    }
}

/// What a pending stop still waits on: the frozen owners that have not
/// stopped, or, while the request is queued behind the last stop's resumes,
/// the owners that still owe one.
fn pending(gate: &RegistryState) -> Result<StopPending, RendezvousError> {
    Ok(match gate.rendezvous.phase {
        Phase::Stopping(epoch) => {
            let owners = membership(gate)?;
            let waiting_on = gate
                .rendezvous
                .frozen
                .iter()
                .copied()
                .filter(|key| {
                    owners.iter().any(|(owner, mode)| {
                        owner == key && !matches!(mode, Mode::Stopped | Mode::NativeStopped)
                    })
                })
                .collect();
            StopPending {
                epoch: Some(epoch),
                waiting_on,
            }
        }
        _ => StopPending {
            epoch: None,
            waiting_on: gate.rendezvous.debt.clone(),
        },
    })
}

impl<'era> AllStopped<'era> {
    pub fn symbols(&mut self) -> Result<super::super::KeeperSymbols<'_>, RendezvousError> {
        let registry = self.registry;
        super::super::symbol_authority::KeeperSymbols::mint(self, registry)
    }
    #[doc(hidden)]
    pub fn check_world_roots(
        &self,
        heap: super::super::HeapIdentity,
    ) -> Result<(), RendezvousError> {
        keeper(self.registry)?;
        if self.registry.heap != heap {
            return Err(RendezvousError::Identity);
        }
        let gate = self
            .registry
            .lock_for_keeper()
            .map_err(registration_error)?;
        self.validate(&gate)
    }

    /// The metadata census under this stop. The borrow keeps `trace`,
    /// `complete` and `bind` from running while the census holds the gate;
    /// the era's own `stopped_summaries` refuses the keeper during a stop.
    pub fn summaries(&mut self) -> Result<super::super::StoppedSummaries<'_>, RegistrationError> {
        if keeper(self.registry).is_err() {
            return Err(RegistrationError::WrongThread);
        }
        super::super::StoppedSummaries::under_stop(self.registry)
    }

    pub(in crate::tagged::gc) fn gate_is_held_by_current_thread(&self) -> bool {
        self.registry.gate_is_held_by_current_thread()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(in crate::tagged::gc) fn note_owner_cons_lease_grant(
        &self,
        registration: NonZeroU64,
        lease: std::sync::Weak<crate::tagged::gc::cons_allocator::OwnerConsLease>,
    ) -> Result<(), crate::tagged::gc::ReserveError> {
        self.registry
            .note_owner_cons_lease_grant(registration, lease)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub(in crate::tagged::gc) fn note_owner_cons_lease_returned(
        &self,
        registration: NonZeroU64,
        owner_key: u64,
    ) {
        self.registry
            .note_owner_cons_lease_returned(registration, owner_key);
    }

    /// Cold look at the gate this stop already holds. The body cannot finish
    /// the epoch or take a heap loan. The keeper-thread check is the same
    /// predicate `stop` used to mint this guard.
    ///
    /// The keeper holds no other guard here, and no gate holder waits on the
    /// keeper: every wait under the gate is the condvar wait that releases
    /// it, and the one blocking lock taken under the gate (the cons catalog,
    /// in `Mutator::publish`) is taken only while Idle, by holders that never
    /// take the gate. So this waits out a brief holder (an owner's refused
    /// restore or publish attempt) instead of failing `Busy`: the callers
    /// bind a stop, or validate a receipt they consume, and a transient
    /// refusal would drop either. The test-only restore hooks are the
    /// exception; see `Hooks`.
    pub(in crate::tagged::gc::mutator_registration) fn with_stopped_gate<R>(
        &self,
        body: impl FnOnce(&Registry, &RegistryState) -> Result<R, RendezvousError>,
    ) -> Result<R, RendezvousError> {
        keeper(self.registry)?;
        let gate = self
            .registry
            .lock_for_keeper()
            .map_err(registration_error)?;
        self.validate(&gate)?;
        body(self.registry, &gate)
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

    pub(in crate::tagged::gc::mutator_registration) fn heap_identity(
        &self,
    ) -> crate::tagged::gc::HeapIdentity {
        self.registry.heap
    }

    fn validate(&self, gate: &RegistryState) -> Result<(), RendezvousError> {
        if self.registry.park.poisoned.load(Ordering::Acquire) {
            return Err(RendezvousError::Poisoned);
        }
        if gate.rendezvous.phase != Phase::Tracing(self.epoch) {
            return Err(RendezvousError::Publication);
        }
        // `Busy` from verify_stopped means "not stopped yet", which `stop`
        // waits on. Under this stop its predicates (admissions quiescent, no
        // resume debt, every frozen owner stopped) and its entry locks (owners
        // take entries only under the gate the keeper holds) cannot fail, so
        // a failure is a broken census: the same refusal as the wrong phase,
        // never a `Busy` a retrying keeper would spin on.
        verify_stopped(self.registry, gate).map_err(|error| match error {
            RendezvousError::Busy => RendezvousError::Publication,
            other => other,
        })
    }

    #[doc(hidden)]
    pub fn trace<R>(
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
            let gate = self
                .registry
                .lock_for_keeper()
                .map_err(registration_error)?;
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

    /// The keeper's stop with nothing to collect (L6.7): run the trace step
    /// with an empty visit, so serial turns can be granted. The stopped-proof
    /// type stays crate-private.
    #[doc(hidden)]
    pub fn trace_serial(&mut self) -> Result<(), RendezvousError> {
        self.trace(|_stopped, _proof| Ok(()))
    }

    /// L6.7 K3: grant the serial ring's head inside this stop. Only the
    /// keeper, only after `trace`, and only for this stop's epoch. One
    /// ticket is out at a time and an unready head is never skipped.
    /// `ready` sees
    /// the head's owner key and the publication generation it was queued
    /// under. It runs under the registry gate, so it must not call back into
    /// the gate: a refused re-entry would read as an unready head. The `&mut`
    /// borrow keeps `trace` and `complete` out while the grant is decided,
    /// and `complete` refuses while the grant is out.
    #[doc(hidden)]
    pub fn grant_serial_head(
        &mut self,
        ready: impl FnOnce(MutatorKey, NonZeroU64) -> bool,
    ) -> Result<Option<SerialGrant>, RendezvousError> {
        keeper(self.registry)?;
        // `stop` already moved the ring's phase to Tracing, so the ring
        // cannot tell a traced stop from an untraced one: this proof can.
        if !self.traced {
            return Err(RendezvousError::Publication);
        }
        let mut gate = self
            .registry
            .lock_for_keeper()
            .map_err(registration_error)?;
        self.validate(&gate)?;
        let mut owner = None;
        let granted = gate.rendezvous.grant_head_in_stop(self.epoch, |entry| {
            owner = Some(entry.owner());
            ready(entry.owner(), entry.generation())
        })?;
        Ok(granted.zip(owner).map(|(id, owner)| SerialGrant {
            id,
            owner,
            _thread: PhantomData,
        }))
    }

    /// The ring head's owner and the publication generation it was queued
    /// under, so the keeper can check that owner's receipt before granting.
    #[doc(hidden)]
    pub fn serial_head(&self) -> Result<Option<(MutatorKey, NonZeroU64)>, RendezvousError> {
        self.with_stopped_gate(|_registry, gate| {
            Ok(gate
                .rendezvous
                .ring_head_entry()
                .map(|entry| (entry.owner(), entry.generation())))
        })
    }

    /// Serial requests queued or granted under this stop. The keeper's batch
    /// bound (L6.7 K4) is this length when the stop began.
    #[doc(hidden)]
    pub fn serial_ring_len(&self) -> Result<usize, RendezvousError> {
        self.with_stopped_gate(|_registry, gate| Ok(gate.rendezvous.ring_len()))
    }

    /// Retire a grant after its ticket came back. Consuming the grant makes
    /// the retire exactly once by construction.
    #[doc(hidden)]
    pub fn retire_serial_grant(&mut self, grant: SerialGrant) -> Result<(), RendezvousError> {
        keeper(self.registry)?;
        let mut gate = self
            .registry
            .lock_for_keeper()
            .map_err(registration_error)?;
        gate.rendezvous.retire_serial(grant.id)
    }

    fn entry(&self, key: MutatorKey) -> Result<Arc<MutatorEntry>, RendezvousError> {
        keeper(self.registry)?;
        let gate = self
            .registry
            .lock_for_keeper()
            .map_err(registration_error)?;
        self.validate(&gate)?;
        if key.owner != 0 && !gate.rendezvous.frozen.contains(&key) {
            return Err(RendezvousError::Identity);
        }
        self.registry
            .validate(&gate, key)
            .map_err(registration_error)
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn with_root_reader<R>(
        &self,
        key: MutatorKey,
        body: impl for<'read> FnOnce(ParallelRootBatchReader<'read>) -> R,
    ) -> Result<R, RendezvousError> {
        let entry = self.entry(key)?;
        // No registry gate survives entry lookup. Root-table acquisition and
        // user code must never run while a control predicate mutex is held.
        let state = super::keeper_hold(&entry)?;
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
        let mut state = super::keeper_hold(&entry)?;
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

    /// Copied owner-work census. Root words are bits; other items keep their Debug text.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn owner_work_census_for_test(
        &self,
        key: MutatorKey,
    ) -> Result<(Vec<usize>, Vec<String>), RendezvousError> {
        self.with_owner_image_for_test(key, |image| {
            let mut roots = Vec::new();
            let mut other = Vec::new();
            image.visit_work(|work| match work {
                super::super::image::PublishedGcWork::Root(word) => roots.push(word.value().bits()),
                other_work => other.push(format!("{other_work:?}")),
            });
            (roots, other)
        })
    }

    /// VIS row: shared-borrow stopped-owner image observation for the re-homed
    /// root-slot tests. The mutable image never leaves the crate.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_owner_image_for_test<R>(
        &self,
        key: MutatorKey,
        body: impl FnOnce(&PublishedGcImage) -> R,
    ) -> Result<R, RendezvousError> {
        let entry = self.entry(key)?;
        let state = super::keeper_hold(&entry)?;
        let image = state.image.as_ref().ok_or(RendezvousError::Publication)?;
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

    /// `complete` consumes the stop, and its only failure exit drops it,
    /// which poisons the era. So a brief holder must not become a failure
    /// here. The gate is the keeper's waiting acquisition. The reader scan
    /// stays outside the gate (root-table locks never overlap it), and an
    /// owner holds its entry only for a moment under the gate, so a `Busy`
    /// in the scan is retried: nothing has changed yet.
    fn prepare_complete(&mut self) -> Result<(), RendezvousError> {
        if !self.traced {
            return Err(RendezvousError::Publication);
        }
        let entries = {
            let gate = self
                .registry
                .lock_for_keeper()
                .map_err(registration_error)?;
            self.validate(&gate)?;
            let mut entries = Vec::new();
            entries
                .try_reserve_exact(gate.entries.len())
                .map_err(|_| RendezvousError::Capacity)?;
            entries.extend(gate.entries.iter().cloned());
            entries
        };
        super::readers_finished_waiting(&entries)?;
        // With the gate held, no owner holds an entry lock (owners take them
        // only under the gate), so the entry locks below cannot meet `Busy`,
        // the failure that poisons once the control commit has opened.
        let mut gate = self
            .registry
            .lock_for_keeper()
            .map_err(registration_error)?;
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
        #[cfg(any(test, feature = "test-support"))]
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

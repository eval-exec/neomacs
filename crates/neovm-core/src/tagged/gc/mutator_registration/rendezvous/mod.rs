//! Cold per-heap stop protocol. There is no evaluator or heap accessor here.
//! The canonical registry gate serializes membership, ACKs and restoration.
pub(super) mod control;
mod keeper;
mod owner;
mod resume;

pub use control::{Mode, Phase, Request};
pub use keeper::{AllStopped, AllStoppedFailure};
pub use owner::RootedRetireFailure;
pub use resume::{ResumeGcImage, ResumeGcImageFailure};

use super::*;
use crate::tagged::transport::{ParallelRootBatchError, ParallelRootIdentity, SharedRoot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RendezvousError {
    Poisoned,
    Identity,
    Loan,
    Publication,
    Generation,
    Busy,
    Readers,
    Capacity,
    Root,
    Registration(RegistrationError),
}
impl std::fmt::Display for RendezvousError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rendezvous refused: {self:?}")
    }
}
impl std::error::Error for RendezvousError {}
impl From<RegistrationError> for RendezvousError {
    fn from(e: RegistrationError) -> Self {
        match e {
            RegistrationError::Poisoned => Self::Poisoned,
            RegistrationError::Busy => Self::Busy,
            RegistrationError::WrongHeap
            | RegistrationError::WrongOwner
            | RegistrationError::StaleRegistration => Self::Identity,
            RegistrationError::OutstandingLoan => Self::Loan,
            RegistrationError::Exhausted => Self::Generation,
            RegistrationError::Capacity => Self::Capacity,
            other => Self::Registration(other),
        }
    }
}
impl From<ParallelRootBatchError> for RendezvousError {
    fn from(e: ParallelRootBatchError) -> Self {
        use crate::tagged::transport::RootBatchError;
        match e {
            ParallelRootBatchError::Transport(RootBatchError::Busy) => Self::Busy,
            ParallelRootBatchError::Transport(RootBatchError::ReadersOutstanding { .. }) => {
                Self::Readers
            }
            ParallelRootBatchError::Transport(
                RootBatchError::Capacity { .. } | RootBatchError::StorageAllocation(_),
            ) => Self::Capacity,
            ParallelRootBatchError::Identity
            | ParallelRootBatchError::Transport(
                RootBatchError::ForeignHeap { .. } | RootBatchError::StaleReservation,
            ) => Self::Identity,
            ParallelRootBatchError::StopEpoch { .. } => Self::Publication,
            _ => Self::Root,
        }
    }
}

/// Rooted input admission, created only by the original owner's Publishing seam.
/// It grants no assertion about other mutators having stopped.
pub(crate) struct OwnerRootAdmission<'a> {
    identity: ParallelRootIdentity,
    roots: &'a [SharedRoot],
    _owner: PhantomData<&'a mut ()>,
    _thread: PhantomData<Rc<()>>,
}
impl<'a> OwnerRootAdmission<'a> {
    #[cfg(test)]
    pub(crate) fn for_test(identity: ParallelRootIdentity, roots: &'a [SharedRoot]) -> Self {
        Self {
            identity,
            roots,
            _owner: PhantomData,
            _thread: PhantomData,
        }
    }
    pub(crate) fn identity(&self) -> ParallelRootIdentity {
        self.identity
    }
    pub(crate) fn roots(&self) -> &[SharedRoot] {
        self.roots
    }
}
/// A scoped root-read admission. Only AllStopped's implementation creates it.
pub(crate) struct StoppedRootAdmission<'a> {
    pub(super) identity: ParallelRootIdentity,
    pub(super) epoch: u64,
    pub(super) _stop: PhantomData<&'a ()>,
    pub(super) _thread: PhantomData<Rc<()>>,
}
impl StoppedRootAdmission<'_> {
    #[cfg(test)]
    pub(crate) fn for_test(identity: ParallelRootIdentity, epoch: u64) -> Self {
        Self {
            identity,
            epoch,
            _stop: PhantomData,
            _thread: PhantomData,
        }
    }
    pub(crate) fn identity(&self) -> ParallelRootIdentity {
        self.identity
    }
    pub(crate) fn current_epoch(&self) -> u64 {
        self.epoch
    }
}

#[derive(Default)]
pub(super) struct OwnerLocal {
    pub(super) participating: bool,
    #[cfg(test)]
    pub(super) publication_hook: Option<Box<dyn FnOnce()>>,
    #[cfg(test)]
    pub(super) restore_wait_hook: Option<Box<dyn FnOnce()>>,
}
impl std::fmt::Debug for OwnerLocal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerLocal")
            .field("participating", &self.participating)
            .finish_non_exhaustive()
    }
}

/// A Send request handle. It cannot wait while retaining a Running owner, mint
/// stop authority, or return any owner image. Requests themselves never block.
#[derive(Clone, Copy)]
pub struct Rendezvous<'era> {
    registry: &'era Registry,
}
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub phase: Phase,
    pub request_epoch: u64,
    pub completed_epoch: u64,
    pub queued: bool,
    pub frozen: Vec<MutatorKey>,
    pub debt: Vec<MutatorKey>,
    pub owners: Vec<(MutatorKey, Mode)>,
    pub traces: usize,
}
impl std::fmt::Debug for Rendezvous<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Rendezvous")
            .field(&self.registry.heap)
            .finish()
    }
}
impl Rendezvous<'_> {
    pub fn request(&self) -> Result<Request, RendezvousError> {
        request(self.registry)
    }
    pub fn snapshot(&self) -> Result<Snapshot, RendezvousError> {
        let gate = self.registry.lock()?;
        Ok(Snapshot {
            phase: gate.rendezvous.phase,
            request_epoch: gate.rendezvous.epoch,
            completed_epoch: gate.rendezvous.completed,
            queued: gate.rendezvous.queued,
            frozen: gate.rendezvous.frozen.clone(),
            debt: gate.rendezvous.debt.clone(),
            owners: membership(&gate)?,
            traces: gate.rendezvous.traces,
        })
    }
    /// A notification is only a hint; predicate loops decide every transition.
    pub fn notify(&self) {
        self.registry.park.wake.notify_all();
    }
}
impl EraCore<'_> {
    #[cfg(test)]
    pub(crate) fn test_root_epoch_retention(&self) -> super::super::FacadeEpochRetention {
        self.registry()
            .root_epoch
            .retain()
            .expect("live fixture epoch")
    }
    pub fn rendezvous(&self) -> Rendezvous<'_> {
        Rendezvous {
            registry: self.registry(),
        }
    }
}

pub(super) fn entry_state(
    entry: &MutatorEntry,
) -> Result<MutexGuard<'_, EntryState>, RendezvousError> {
    match entry.state.try_lock() {
        Ok(g) => Ok(g),
        Err(std::sync::TryLockError::WouldBlock) => Err(RendezvousError::Busy),
        Err(std::sync::TryLockError::Poisoned(_)) => Err(RendezvousError::Poisoned),
    }
}
pub(super) fn membership(gate: &RegistryState) -> Result<Vec<(MutatorKey, Mode)>, RendezvousError> {
    let mut result = Vec::new();
    result
        .try_reserve(gate.entries.len())
        .map_err(|_| RendezvousError::Capacity)?;
    // Entry zero is the keeper's frozen serial state, never an executing peer.
    // A5 must add its frozen World root census before actual collection.
    for entry in gate.entries.iter().skip(1) {
        if !entry.cancelled_start.load(Ordering::Acquire) {
            result.push((entry.key, entry_state(entry)?.rendezvous.mode));
        }
    }
    Ok(result)
}
pub(super) fn request(registry: &Registry) -> Result<Request, RendezvousError> {
    let mut gate = registry.lock()?;
    let members = membership(&gate)?;
    for entry in gate.entries.iter().skip(1) {
        if entry.cancelled_start.load(Ordering::Acquire) {
            continue;
        }
        let e = entry_state(entry)?;
        // A serial-only publication has no parallel exact-root admission.
        if matches!(e.phase, EntryPhase::Published | EntryPhase::Retiring) && e.roots.is_none() {
            return Err(RendezvousError::Publication);
        }
    }
    let old = gate.rendezvous.epoch;
    let request = gate.rendezvous.request(&members)?;
    if gate.rendezvous.epoch != old {
        for entry in gate.entries.iter().skip(1) {
            let mut e = entry_state(entry)?;
            if e.rendezvous.mode == Mode::NativeStopped {
                e.rendezvous.coverage_epoch = gate.rendezvous.epoch;
            }
        }
        registry
            .request_epoch
            .store(gate.rendezvous.epoch, Ordering::Release);
        #[cfg(test)]
        a8_probes::mark(a8_probes::REQUEST_PUBLISHED, gate.rendezvous.epoch, 0);
    }
    registry.park.wake.notify_all();
    Ok(request)
}
pub(super) fn finish_debt(
    registry: &Registry,
    gate: &mut RegistryState,
) -> Result<(), RendezvousError> {
    let members = membership(gate)?;
    if gate.rendezvous.finish_debt(&members)? {
        for entry in gate.entries.iter().skip(1) {
            let mut e = entry_state(entry)?;
            if e.rendezvous.mode == Mode::StartingStopped && e.phase == EntryPhase::Running {
                e.rendezvous.mode = Mode::Running;
            }
        }
    }
    registry.park.wake.notify_all();
    Ok(())
}
pub(super) fn verify_stopped(
    registry: &Registry,
    gate: &RegistryState,
) -> Result<(), RendezvousError> {
    gate.rendezvous.live()?;
    registry.symbols.ready()?;
    if registry.park.poisoned.load(Ordering::Acquire) {
        return Err(RendezvousError::Poisoned);
    }
    // Authority half of the admission Dekker pair. Publishing here (not only
    // at the request's guard drop) keeps the pairing sound even when the
    // request and this verification share one gate hold: no slice admitted
    // against the Idle word can slip past the scan into the trace.
    registry.phase.publish(gate);
    if !admissions_quiescent(&gate.entries) {
        return Err(RendezvousError::Busy);
    }
    if !gate.rendezvous.debt.is_empty() {
        return Err(RendezvousError::Busy);
    }
    for key in &gate.rendezvous.frozen {
        let entry = registry.validate(gate, *key)?;
        let e = entry_state(&entry)?;
        if !matches!(e.rendezvous.mode, Mode::Stopped | Mode::NativeStopped) {
            return Err(RendezvousError::Busy);
        }
        if e.run_loan || e.worker_loan || entry.admitted() {
            return Err(RendezvousError::Loan);
        }
        if e.image.is_none() {
            return Err(RendezvousError::Publication);
        }
        let roots = e.roots.as_ref().ok_or(RendezvousError::Publication)?;
        let id = roots.identity();
        if id.key() != *key || id.publication_generation().get() != e.publication_generation {
            return Err(RendezvousError::Identity);
        }
        if e.rendezvous.coverage_epoch != gate.rendezvous.epoch || e.epoch != id.publication_epoch()
        {
            return Err(RendezvousError::Publication);
        }
        if (e.rendezvous.mode == Mode::Stopped && id.publication_epoch() != gate.rendezvous.epoch)
            || id.publication_epoch() > gate.rendezvous.epoch
        {
            return Err(RendezvousError::Publication);
        }
    }
    Ok(())
}
pub(super) fn wait<'a>(
    registry: &'a Registry,
    gate: RegistryGuard<'a>,
) -> Result<RegistryGuard<'a>, RendezvousError> {
    wait_after_poison_check(registry, gate)
}

// Separated to force poison after a caller's predicate check in regressions.
fn wait_after_poison_check<'a>(
    registry: &'a Registry,
    gate: RegistryGuard<'a>,
) -> Result<RegistryGuard<'a>, RendezvousError> {
    let gate = registry.wait(gate)?;
    gate.rendezvous.live()?;
    Ok(gate)
}

static_assertions::assert_impl_all!(Rendezvous<'static>:Send,Sync,Clone,Copy);
static_assertions::assert_not_impl_any!(OwnerRootAdmission<'static>:Send,Sync,Clone,Copy);
static_assertions::assert_not_impl_any!(StoppedRootAdmission<'static>:Send,Sync,Clone,Copy);

#[cfg(test)]
mod tests;

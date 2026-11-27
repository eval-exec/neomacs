//! Cold per-heap stop protocol. There is no evaluator or heap accessor here.
//! The canonical registry gate serializes membership, ACKs and restoration.
pub(super) mod control;
mod keeper;
mod owner;
mod resume;
mod run_admission;
mod sync;

pub use control::{Mode, Phase, Request, RingClient, SerialClient};
pub use keeper::AllStopped;
pub use keeper::AllStoppedFailure;
#[doc(hidden)]
pub use keeper::EraGraceReceipt;
pub use keeper::SerialGrant;
#[doc(hidden)]
pub use keeper::SymbolTableGraceReclaim;
pub(in crate::tagged::gc) use keeper::TraceWitness;
pub use keeper::{StopAttempt, StopPending};
pub use owner::PollOutcome;
pub use owner::RootedRetireFailure;
pub use resume::ResumeGcImage;
pub use resume::ResumeGcImageFailure;

use std::num::NonZeroU64;

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
    /// E3.3: this build carries `gc-memory-telemetry`, for which the stopped
    /// collector takes the documented serial-only route (DESIGN §4: a complete
    /// stopped aggregation or an explicit serial-only route, never partial
    /// data). The heap stays on the typed Baton-serial fallback.
    TelemetryStoppedGcUnavailable,
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
            // A live owner-cons lease is permanent. Name it so the catch-all
            // cannot turn it into Busy. The payload stays OwnerReserveHeld.
            held @ RegistrationError::OwnerReserveHeld => Self::Registration(held),
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
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct OwnerRootAdmission<'a> {
    identity: ParallelRootIdentity,
    roots: &'a [SharedRoot],
    _owner: PhantomData<&'a mut ()>,
    _thread: PhantomData<Rc<()>>,
}
impl<'a> OwnerRootAdmission<'a> {
    #[cfg(any(test, feature = "test-support"))]
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
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct StoppedRootAdmission<'a> {
    pub(super) identity: ParallelRootIdentity,
    pub(super) epoch: u64,
    pub(super) _stop: PhantomData<&'a ()>,
    pub(super) _thread: PhantomData<Rc<()>>,
}
impl StoppedRootAdmission<'_> {
    /// # Safety
    /// The returned witness gates stopped-world root reads: the caller must
    /// hold the stopped `AllStopped` world for this exact identity and epoch,
    /// and no other owner may resume while the witness lives.
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) unsafe fn for_test(identity: ParallelRootIdentity, epoch: u64) -> Self {
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

/// Test-only publication/restore hooks on the owner protocol.
///
/// Only this crate's tests set them; the hook closures are one-shot and run
/// inside the rendezvous paths below.  Keeping the real fields out of every
/// non-test build preserves the production auto-trait diagnostics the
/// compile-fail contracts pin (CG-5): in a dependency `test-support` build
/// `Hooks` is the empty arm, exactly like production, so `Mutator` cannot
/// grow the `dyn FnOnce(): Send` note.  The two `mod` declarations are the
/// split-cfg guard's exemption for a bare `test` predicate; `Hooks` exists in
/// every build, so `OwnerLocal` needs no cfg on its field.
#[cfg(any(test, feature = "test-support"))]
mod owner_test_hooks {
    #[derive(Default)]
    pub(in crate::tagged::gc::mutator_registration) struct Hooks {
        /// Runs after `Publishing` is visible, with the gate released.
        pub(super) publication_hook: Option<Box<dyn FnOnce()>>,
        /// Runs while `prepare_restore` HOLDS the gate, just before its
        /// condvar wait. A hook that blocks here is a gate holder that
        /// waits: a keeper in `with_stopped_gate` (bind, receipt check)
        /// waits it out, so a row must not block this hook on the keeper.
        pub(super) restore_wait_hook: Option<Box<dyn FnOnce()>>,
        /// Runs after a native publication commits, with the gate released.
        pub(super) receipt_hook: Option<Box<dyn FnOnce()>>,
    }
    impl Hooks {
        pub(super) fn take_publication(&mut self) -> Option<Box<dyn FnOnce()>> {
            self.publication_hook.take()
        }
        pub(super) fn take_restore_wait(&mut self) -> Option<Box<dyn FnOnce()>> {
            self.restore_wait_hook.take()
        }
        /// Runs after a native publication commits, before its receipt is read.
        pub(in crate::tagged::gc::mutator_registration) fn take_receipt(
            &mut self,
        ) -> Option<Box<dyn FnOnce()>> {
            self.receipt_hook.take()
        }
    }
}
#[cfg(not(any(test, feature = "test-support")))]
mod owner_test_hooks {
    #[derive(Default)]
    pub(super) struct Hooks;
    impl Hooks {
        pub(super) fn take_publication(&mut self) -> Option<Box<dyn FnOnce()>> {
            None
        }
        pub(super) fn take_restore_wait(&mut self) -> Option<Box<dyn FnOnce()>> {
            None
        }
    }
}
use owner_test_hooks::Hooks;

#[derive(Default)]
pub(super) struct OwnerLocal {
    pub(super) participating: bool,
    /// True while this owner's image is published/parked (`Stopped` or
    /// `NativeStopped`) and has not yet been restored to `Running`.
    pub(super) stopped: bool,
    pub(super) hooks: Hooks,
}
impl std::fmt::Debug for OwnerLocal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerLocal")
            .field("participating", &self.participating)
            .field("stopped", &self.stopped)
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
    pub(crate) phase: Phase,
    pub(crate) request_epoch: u64,
    pub(crate) completed_epoch: u64,
    pub(crate) queued: bool,
    pub(crate) frozen: Vec<MutatorKey>,
    pub(crate) debt: Vec<MutatorKey>,
    pub owners: Vec<(MutatorKey, Mode)>,
    pub(crate) traces: usize,
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
    pub(crate) fn notify(&self) {
        self.registry.park.wake.notify_all();
    }
}
impl<K: EraKeeper> EraCore<'_, K> {
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn test_root_epoch_retention(&self) -> super::super::FacadeEpochRetention {
        self.registry()
            .root_epoch
            .retain()
            .expect("live fixture epoch")
    }
    pub(crate) fn rendezvous(&self) -> Rendezvous<'_> {
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

/// An owner holds its entry only for a moment, under the gate. A keeper that
/// meets that moment outside the gate retries it instead of refusing `Busy`.
/// The retry is bounded: a hold this long is not an owner's moment, and it
/// refuses `Busy` as before. The keeper's own hold across a body never
/// reaches this wait (`keeper_hold`).
pub(super) fn wait_out_owner_moment<T>(
    mut attempt: impl FnMut() -> Result<T, RendezvousError>,
) -> Result<T, RendezvousError> {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(hook) = super::test_fences::take_owner_moment_hook_for_test() {
        hook();
    }
    let deadline = std::time::Instant::now() + super::native_extent::CONTENTION_WAIT;
    loop {
        match attempt() {
            Err(RendezvousError::Busy) if std::time::Instant::now() < deadline => {
                #[cfg(any(test, feature = "test-support"))]
                super::test_fences::note_gate_contention();
                std::thread::yield_now();
            }
            other => return other,
        }
    }
}

/// The stopped keeper's hold on one entry across a body it runs.
pub(super) struct KeeperHold<'entry> {
    entry: &'entry MutatorEntry,
    state: MutexGuard<'entry, EntryState>,
}

impl std::ops::Deref for KeeperHold<'_> {
    type Target = EntryState;
    fn deref(&self) -> &EntryState {
        &self.state
    }
}

impl std::ops::DerefMut for KeeperHold<'_> {
    fn deref_mut(&mut self) -> &mut EntryState {
        &mut self.state
    }
}

impl Drop for KeeperHold<'_> {
    fn drop(&mut self) {
        // Cleared before the guard field drops: the flag never outlives the
        // keeper's guard.
        self.entry
            .keeper_held
            .store(false, sync::atomic::Ordering::Relaxed);
    }
}

/// The stopped keeper takes ENTRY for a body it runs, waiting out an owner's
/// moment. A body can reach the same entry again through its stop. That hold
/// is the keeper's own and is not a moment, so waiting cannot end it: it
/// refuses at once as `Publication`, the class `validate` gives the keeper's
/// own hold on a frozen entry.
pub(super) fn keeper_hold(entry: &MutatorEntry) -> Result<KeeperHold<'_>, RendezvousError> {
    if entry.keeper_held.load(sync::atomic::Ordering::Relaxed) {
        return Err(RendezvousError::Publication);
    }
    let state = wait_out_owner_moment(|| entry_state(entry))?;
    entry
        .keeper_held
        .store(true, sync::atomic::Ordering::Relaxed);
    Ok(KeeperHold { entry, state })
}

/// Every published root batch, current or retained, has no reader left.
fn readers_finished(entries: &[Arc<MutatorEntry>]) -> Result<(), RendezvousError> {
    for entry in entries {
        let state = entry_state(entry)?;
        if let Some(roots) = &state.roots {
            roots.validate_no_readers().map_err(RendezvousError::from)?;
        }
        for roots in &state.retained_roots {
            roots.validate_no_readers().map_err(RendezvousError::from)?;
        }
    }
    Ok(())
}

/// The keeper's reader scan (completion and collection). It stays outside the
/// gate because root-table locks never overlap it, so an owner's moment on an
/// entry is waited out.
pub(super) fn readers_finished_waiting(
    entries: &[Arc<MutatorEntry>],
) -> Result<(), RendezvousError> {
    wait_out_owner_moment(|| readers_finished(entries))
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
        #[cfg(any(test, feature = "test-support"))]
        a8_probes::mark(a8_probes::REQUEST_PUBLISHED, gate.rendezvous.epoch, 0);
    }
    registry.park.wake.notify_all();
    Ok(request)
}

/// L6.7 K4: the keeper commits one serial request at the ring's tail.
/// Commit order is the keeper's receive order. Keeper thread only; the gate
/// is the keeper's waiting acquisition.
pub(super) fn enqueue_serial(
    registry: &Registry,
    owner: MutatorKey,
    generation: NonZeroU64,
) -> Result<(), RendezvousError> {
    keeper::keeper_thread(registry)?;
    let mut gate = registry.lock_for_keeper()?;
    gate.rendezvous
        .enqueue_serial(owner, generation)
        .map(|_| ())
}

/// L6.7 K7: the keeper poisons the era on a proven invariant break (an
/// owner that unwound or leaked its lend, a message that cannot belong to the
/// protocol). Parked owners wake with `Poisoned`. Keeper thread only.
pub(super) fn poison_era(registry: &Registry) -> Result<(), RendezvousError> {
    keeper::keeper_thread(registry)?;
    registry.poison();
    Ok(())
}

/// L6.7 K6: the keeper cancels `owner`'s queued request. A granted entry
/// refuses with `Identity`; its grant's retire ends it.
pub(super) fn cancel_serial(registry: &Registry, owner: MutatorKey) -> Result<(), RendezvousError> {
    cancel_queued(registry, SerialClient::Owner(owner))
}

/// L6.7b b3: the keeper commits the Baton group's request at the ring's
/// tail. The group is one client: a second entry while one is queued or
/// granted refuses with `Identity`. Keeper thread only.
pub(super) fn enqueue_baton_group(registry: &Registry) -> Result<(), RendezvousError> {
    keeper::keeper_thread(registry)?;
    let mut gate = registry.lock_for_keeper()?;
    gate.rendezvous.enqueue_baton_group().map(|_| ())
}

/// L6.7b b5: the keeper puts the Baton group's request back at the ring's
/// HEAD after the group's turn came back asking for a collection. Keeper
/// thread only; refused while another group entry exists or a turn is out.
pub(super) fn requeue_baton_group_at_head(registry: &Registry) -> Result<(), RendezvousError> {
    keeper::keeper_thread(registry)?;
    let mut gate = registry.lock_for_keeper()?;
    gate.rendezvous.requeue_baton_group_at_head().map(|_| ())
}

/// L6.7c (c): the keeper commits an owner's request at the ring's HEAD: the
/// owner's turn came back asking for a collection, and this is its next
/// request. Keeper thread only; refused while the owner has an entry or a
/// turn is out.
pub(super) fn requeue_serial_at_head(
    registry: &Registry,
    owner: MutatorKey,
    generation: NonZeroU64,
) -> Result<(), RendezvousError> {
    keeper::keeper_thread(registry)?;
    let mut gate = registry.lock_for_keeper()?;
    gate.rendezvous
        .requeue_serial_at_head(owner, generation)
        .map(|_| ())
}

/// L6.7b b3: the keeper cancels the Baton group's queued request when the
/// group leaves the era. A granted entry refuses with `Identity`.
pub(super) fn cancel_baton_group(registry: &Registry) -> Result<(), RendezvousError> {
    cancel_queued(registry, SerialClient::BatonGroup)
}

fn cancel_queued(registry: &Registry, client: SerialClient) -> Result<(), RendezvousError> {
    keeper::keeper_thread(registry)?;
    let mut gate = registry.lock_for_keeper()?;
    gate.rendezvous.cancel_queued(client)
}

/// Test-only: queue one serial request without the keeper-thread check.
/// The product commit is [`enqueue_serial`]. `generation` is recorded and is
/// not checked here; the keeper's `ready` callback checks it at the grant.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn enqueue_serial_for_test(
    registry: &Registry,
    owner: MutatorKey,
    generation: NonZeroU64,
) -> Result<u64, RendezvousError> {
    let mut gate = registry.lock()?;
    Ok(gate.rendezvous.enqueue_serial(owner, generation)?.get())
}

/// Head of the live ring, without granting it.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn serial_ring_head_for_test(
    registry: &Registry,
) -> Result<(usize, Option<u64>), RendezvousError> {
    let gate = registry.lock()?;
    gate.rendezvous.live()?;
    Ok((
        gate.rendezvous.ring_len(),
        gate.rendezvous.ring_head().map(|id| id.get()),
    ))
}

/// Retire the current head when it is `id`. A granted head retires the same way.
#[cfg(any(test, feature = "test-support"))]
pub(super) fn retire_serial_head_for_test(
    registry: &Registry,
    id: u64,
) -> Result<(), RendezvousError> {
    let mut gate = registry.lock()?;
    let head = gate
        .rendezvous
        .ring_head()
        .ok_or(RendezvousError::Identity)?;
    if head.get() != id {
        return Err(RendezvousError::Identity);
    }
    gate.rendezvous.retire_serial(head)
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

/// Bounded form of [`wait`]. The caller owns the deadline; this helper
/// returns the guard either way so the predicate runs again. Poison still
/// refuses.
pub(super) fn wait_timeout<'a>(
    registry: &'a Registry,
    gate: RegistryGuard<'a>,
    timeout: std::time::Duration,
) -> Result<RegistryGuard<'a>, RendezvousError> {
    let gate = registry.wait_timeout(gate, timeout)?;
    gate.rendezvous.live()?;
    Ok(gate)
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

#[cfg(any(test, feature = "test-support"))]
pub(crate) mod fixture_helpers;
#[cfg(any(test, feature = "test-support"))]
pub(crate) mod rooted_tests;
#[cfg(any(test, feature = "test-support"))]
pub(crate) use rooted_tests::run_rooted_mutator_case;

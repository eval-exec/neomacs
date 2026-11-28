//! E6-P.4 live-Green parallel admission, era-open/teardown, and opt-in owner
//! execution loop.
//!
//! Closes the three architectural seams locked in `E6-P4-ADMISSION-PREREAD.md` r1:
//! - **Seam 1 (`prepare_era_from_live_green` + `E6-P4-I1`):** unwind-safe live
//!   `&mut Context` era-open transition `(a) surrender Solo -> (b) open keeper &
//!   Driver & Control::open_era -> (c) acquire initial Era turn` guarded by
//!   [`LiveEraOpenGuard`] so any error or panic either rolls back cleanly to
//!   `TurnRight::Solo(Baton)` or closes out the keeper/era cleanly.
//! - **Seam 2 (`teardown_live_parallel_era` + `E6-P4-I2`):** retire-by-wake
//!   (`Control::close()`, which wakes every blocked `Participant::Parallel`
//!   via `OwnerWake` and surfaces `Flow::shutdown`) followed by releasing
//!   Main's `TurnRight::Era` and exit-gated `Control::close_era(baton)` once
//!   every opt-in owner has retired via G5 Steps 1–6 and joined inside the
//!   keeper's `std::thread::scope`.
//! - **Seam 3 (`spawn_parallel_opt_in_thread` + `run_parallel_opt_in_owner`):**
//!   `b6`-capable opt-in owner running on E5.3a/E5.3b/E5.4a (`ChannelPort` +
//!   `ParallelOwnerControl` + `SerialRebindFrame` composing the independent
//!   `SERIAL_TURN_COLLECT_SCOPE` and `SERIAL_TURN_PARK_SCOPE` TLS bridges +
//!   single-turn G5 retirement), supporting Lisp execution, explicit
//!   `(garbage-collect)` (`L6.7c (c)`), and in-stack wait parking
//!   (`condition-wait`, `mutex-lock`, `thread-join`, `thread-yield`).

use super::super::Context;
use super::super::green::{
    Control, Lifecycle, ThreadKey, clear_current_thread_observations,
    restore_current_thread_observations, stash_current_thread_observations,
};
use super::super::world::{
    Baton, EraWorldHandle, TurnRight, freeze_live_legacy_roots, prepare_reference_free_transfer,
    recover_transferred_source,
};
use super::keeper_driver::{ChannelPort, Driver, DriverEnd, GroupPort, KeeperMsg};
use super::owned_source::{
    KeeperEra, KeeperStartNotice, PreparedEraStart, ReturnedEraEnd, keeper_serve,
};
use super::serial_park::{KeeperPort, LiveSerial, PortRefusal, SerialHome, receive_initial_seat};
use super::serial_ticket::LocalSerialOwner;
use super::serial_turn::OwnerSerialState;
use super::source_jit_backend::{self, SourceJitBackend};
use super::source_jit_cache::{self, SourceJitCache};
use super::source_jit_compiler::{self, SourceJitCompiler};
use super::thread_control::{
    ParallelOwnerControl, RegisteredParallelOwner, complete_parallel_owner_retirement,
    finish_live_turn_for_retire, with_park_scope_on_frame,
};
use super::world_seat::{ParkedWorldSeat, prove_stowed_for_era_switch};
use neovm_value::error::Flow;
use neovm_value::tagged::gc::heap_transfer::PendingHeapReturn;
use neovm_value::tagged::gc::{
    Gen1Admission, Mutator, MutatorPermit, RegistrationError, RendezvousError,
};
use neovm_value::value::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

/// Default per-owner root capacity registered on the keeper era.
const OWNER_ROOT_CAPACITY: usize = 64;

/// Bounded wait for parked opt-in owners to wake and exit during teardown
/// (`E6-P4-I2`).
#[cfg(not(test))]
const TEARDOWN_OWNER_EXIT_BOUND: Duration = Duration::from_secs(30);
#[cfg(test)]
const TEARDOWN_OWNER_EXIT_BOUND: Duration = Duration::from_secs(5);

/// Why live-Green era-open refused before admitting an opt-in owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveEraOpenError {
    NotSeatedMain,
    NotSolo,
    GcBusy,
    GenerationalRequired,
    Closing,
    SeatPark,
    DrainFailed,
    HeapUnsupported,
    TransferFailed,
    KeeperStartFailed,
    OpenEraFailed,
    InitialTurnFailed,
    NativeActivation,
    LegacyJitBusy,
    LegacyJitWithdrawalFailed,
    DeferredToBetweenCommands,
    ActivationRefused,
    #[cfg(test)]
    InjectedFault,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LiveEraOpenFault {
    FailAfterSurrenderBeforeOpen,
    PanicAfterSurrenderBeforeOpen,
    FailKeeperSpawn,
    PanicAfterIdleKeeperSpawn,
    FailAfterOpenBeforeAcquire,
    FailLegacyJitWithdrawalStructural,
}

#[cfg(test)]
static INJECTED_ERA_OPEN_FAULT: Mutex<Option<LiveEraOpenFault>> = Mutex::new(None);

#[cfg(test)]
static ERA_OPEN_ATTEMPTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn era_open_attempts_for_test() -> usize {
    ERA_OPEN_ATTEMPTS.load(Ordering::Relaxed)
}

#[cfg(test)]
pub(crate) fn set_era_open_fault_for_test(fault: Option<LiveEraOpenFault>) {
    *INJECTED_ERA_OPEN_FAULT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = fault;
}

#[cfg(test)]
fn take_era_open_fault_matching(expected: LiveEraOpenFault) -> bool {
    let mut slot = INJECTED_ERA_OPEN_FAULT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if *slot == Some(expected) {
        *slot = None;
        true
    } else {
        false
    }
}

/// Request sent from Main/Baton to the keeper thread's `between_stops_hook`
/// to register and spawn a newly admitted opt-in parallel owner.
struct PendingOwnerSpawn {
    owner_id: u64,
    owner_inbox: mpsc::Sender<KeeperMsg>,
    registered: RegisteredParallelOwner,
    world_handle: EraWorldHandle,
}

/// Join handle for the dedicated keeper OS thread.
///
/// Returns `Ok(None)` when the keeper was spawned idle and rolled back before
/// receiving `PreparedEraStart`, `Ok(Some(ReturnedEraEnd))` after a normal era
/// exit, or `Err(KeeperStartNotice)` if `keeper_serve` refused admission.
type KeeperJoinHandle = std::thread::JoinHandle<Result<Option<ReturnedEraEnd>, KeeperStartNotice>>;

/// Keeper OS thread already spawned and waiting for its `PreparedEraStart`.
///
/// Spawning the OS thread BEFORE transferring `Baton` out of
/// `LiveEraOpenGuard<SoloSurrendered>` ensures OS thread creation failure
/// (`EAGAIN` / `ENOMEM` / injected `FailKeeperSpawn`) happens while `Baton` is
/// still owned non-optionally by the guard and rolls back cleanly.
struct SpawnedIdleKeeper {
    start_tx: mpsc::SyncSender<PreparedEraStart>,
    ready_rx: mpsc::Receiver<Result<(GroupPort, mpsc::Sender<KeeperMsg>), KeeperStartNotice>>,
    keeper_join: KeeperJoinHandle,
    driver_drained: Arc<AtomicBool>,
    pending_spawns: Arc<Mutex<VecDeque<PendingOwnerSpawn>>>,
}

/// Main-thread-confined receipts proving Main's legacy serial JIT cache,
/// backend, and compiler scopes were withdrawn for the lifetime of an open
/// parallel era.
struct WithdrawnLegacyJit {
    _cache: SourceJitCache,
    _backend: SourceJitBackend,
    _compiler: SourceJitCompiler,
}
/// Live parallel era state owned by `GreenRuntime` from the first opt-in
/// admission until `green_teardown`.
pub(crate) struct LiveEraRuntime {
    pending_return: Option<PendingHeapReturn>,
    saved_observation_placeholder: Option<Box<dyn std::any::Any>>,
    keeper_join: Option<KeeperJoinHandle>,
    driver_drained: Arc<AtomicBool>,
    inbox: Option<mpsc::Sender<KeeperMsg>>,
    pending_spawns: Arc<Mutex<VecDeque<PendingOwnerSpawn>>>,
    next_owner_id: AtomicU64,
    dead_windows_recovery: super::super::world::SerialRecovery,
    /// Type-erased `WithdrawnLegacyJit` receipts retained for the open parallel
    /// era. Erased to `Box<dyn Any>` (matching `saved_observation_placeholder`)
    /// so `LiveEraRuntime`'s auto-trait graph stays unchanged.
    _withdrawn_legacy_jit: Option<Box<dyn std::any::Any>>,
}

impl std::fmt::Debug for LiveEraRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveEraRuntime")
            .field("has_pending_return", &self.pending_return.is_some())
            .field("has_keeper_join", &self.keeper_join.is_some())
            .field("has_inbox", &self.inbox.is_some())
            .finish_non_exhaustive()
    }
}

/// Fail-closed RAII guard armed ONLY across irreversible by-value state
/// transitions (`transfer_to_keeper`, `open_era_on_control`, `commit`) where
/// ownership is temporarily on the local stack between two typestate guards.
///
/// If an unexpected panic unwinds while armed, `Drop` aborts fail-closed
/// rather than silently returning to Lisp with `Control::SoloSurrendered` or
/// parked `TAGGED_HEAP` TLS. Disarmed via [`Self::disarm`] as soon as the
/// target typestate guard is constructed or rollback finishes.
struct AbortOnUnwind(&'static str);

impl AbortOnUnwind {
    #[inline]
    fn arm(step: &'static str) -> Self {
        Self(step)
    }

    #[inline]
    fn disarm(self) {
        std::mem::forget(self);
    }
}

impl Drop for AbortOnUnwind {
    fn drop(&mut self) {
        tracing::error!(
            step = self.0,
            "E6-P4-I1: panic during non-recoverable era-open transition; aborting fail-closed"
        );
        std::process::abort();
    }
}

/// Main-thread resources shared across every active typestate phase of
/// [`LiveEraOpenGuard`]. Plain struct with no `Drop` and zero `Option` fields.
struct OpenContext<'a> {
    ctx: &'a mut Context,
    control: Arc<Control>,
    parked: ParkedWorldSeat,
    dead_windows_recovery: super::super::world::SerialRecovery,
}

impl<'a> OpenContext<'a> {
    /// Restore `Control::SoloActive` and Main's `TAGGED_HEAP` TLS directly
    /// from an un-transferred `Baton`.
    fn restore_from_baton(self, baton: Baton) {
        let solo_right = self
            .control
            .restore_surrendered_solo(baton)
            .expect("E6-P4-I1 rollback: Control must be in SoloSurrendered");
        self.parked.resume_with_turn_right(self.ctx, solo_right);
        if matches!(
            self.dead_windows_recovery,
            super::super::world::SerialRecovery::RecreateDeadWindows
        ) {
            self.ctx.install_empty_dead_windows_table();
        }
    }

    /// Unified rollback restore from a joined `ReturnedEraEnd` back to
    /// `Control::SoloActive` with Main's `TAGGED_HEAP` TLS reinstalled.
    fn restore_from_returned_end(
        self,
        pending_return: PendingHeapReturn,
        saved_observation_placeholder: Box<dyn std::any::Any>,
        returned_end: ReturnedEraEnd,
        close_control_era: bool,
    ) {
        self.ctx.with_green_runtime(|rt| {
            rt.carried.borrow_mut().reset_observations_to_placeholder();
        });
        restore_current_thread_observations(saved_observation_placeholder);
        let restored = match pending_return.restore(returned_end.into_heap_transfer()) {
            Ok(restored) => restored,
            Err(failure) => {
                tracing::error!(
                    ?failure,
                    "E6-P4-I1 rollback: PendingHeapReturn::restore failed; aborting fail-closed"
                );
                std::process::abort();
            }
        };
        // SAFETY: Main's OS thread is the original transfer source; the keeper
        // joined and no other World loan exists.
        let (baton, _roots, transfer_pub) = match unsafe { recover_transferred_source(restored) } {
            Ok(parts) => parts,
            Err(failure) => {
                tracing::error!(
                    ?failure,
                    "E6-P4-I1 rollback: recover_transferred_source failed; aborting fail-closed"
                );
                std::process::abort();
            }
        };
        // Put the restored canonical ObservationWorld into stowed
        // `GreenRuntime::carried` so `resume_with_turn_right`'s
        // `unstow_serial_world_state` installs it cleanly onto Main's TLS.
        self.ctx
            .with_green_runtime(|rt| rt.carried.borrow_mut().swap_observations_only());
        let solo_right = if close_control_era {
            let (right, era_wh, _unused) = self
                .control
                .close_era(baton)
                .expect("E6-P4-I1 rollback: close_era succeeds with zero owners");
            drop(era_wh);
            right
        } else {
            self.control
                .restore_surrendered_solo(baton)
                .expect("E6-P4-I1 rollback: restore_surrendered_solo succeeds")
        };
        self.parked.resume_with_turn_right(self.ctx, solo_right);
        let _ = super::super::green::retire_transfer_publication_on_main(self.ctx, transfer_pub);
        if matches!(
            self.dead_windows_recovery,
            super::super::world::SerialRecovery::RecreateDeadWindows
        ) {
            self.ctx.install_empty_dead_windows_table();
        }
    }
}

/// Marker trait implemented ONLY by active, uncommitted era-open phases.
///
/// There is NO `Committed` sentinel variant or no-op rollback state anywhere:
/// every [`LiveEraOpenGuard`] unconditionally runs its phase's rollback on
/// `Drop` unless consumed by a typed state transition or [`LiveEraOpenGuard::commit`].
trait OpenPhaseRollback<'a>: Sized {
    fn rollback(self, open: OpenContext<'a>);
}

/// Linear typestate RAII guard enforcing **`E6-P4-I1`** across the live-Green
/// era-open transition:
/// `SoloSurrendered -> IdleKeeperSpawned -> KeeperStarting -> KeeperStarted -> EraOpened -> commit()`.
///
/// Invalid states (`(SoloSurrendered, baton = None)`, `Committed` sentinel
/// during transitions, partial `Option` fields) are unrepresentable by
/// construction.
struct LiveEraOpenGuard<'a, S: OpenPhaseRollback<'a>> {
    inner: std::mem::ManuallyDrop<(OpenContext<'a>, S)>,
}

impl<'a, S: OpenPhaseRollback<'a>> LiveEraOpenGuard<'a, S> {
    fn new(open: OpenContext<'a>, state: S) -> Self {
        Self {
            inner: std::mem::ManuallyDrop::new((open, state)),
        }
    }

    fn into_parts(self) -> (OpenContext<'a>, S) {
        let mut this = std::mem::ManuallyDrop::new(self);
        // SAFETY: `this` is wrapped in `ManuallyDrop`, suppressing `Drop`, so
        // `this.inner` is taken out exactly once and never read again.
        unsafe { std::mem::ManuallyDrop::take(&mut this.inner) }
    }
}

impl<'a, S: OpenPhaseRollback<'a>> Drop for LiveEraOpenGuard<'a, S> {
    fn drop(&mut self) {
        // SAFETY: `Drop::drop` runs at most once and never runs after
        // `into_parts()` suppressed drop via `ManuallyDrop`.
        let (open, state) = unsafe { std::mem::ManuallyDrop::take(&mut self.inner) };
        state.rollback(open);
    }
}

/// Phase 1: `Control::surrender_solo_for_era` succeeded; `Baton` is owned
/// non-optionally on Main's OS thread and no keeper thread has been spawned yet.
struct SoloSurrendered {
    baton: Baton,
}

impl<'a> OpenPhaseRollback<'a> for SoloSurrendered {
    fn rollback(self, open: OpenContext<'a>) {
        open.restore_from_baton(self.baton);
    }
}

/// Phase 2: the idle keeper OS thread is spawned and waiting on `start_rx`,
/// while `Baton` is STILL owned non-optionally on Main's OS thread across all
/// preflights (`pull_observations_to_tls`, `era_world_handle`,
/// `parallel_epoch_retention`, `drain_to_quiescent`, `Gen1Admission::check`,
/// `preflight_era_supported`, `freeze_live_legacy_roots`).
struct IdleKeeperSpawned {
    baton: Baton,
    observations_pulled_out: bool,
    idle_keeper: SpawnedIdleKeeper,
}

impl<'a> OpenPhaseRollback<'a> for IdleKeeperSpawned {
    fn rollback(self, open: OpenContext<'a>) {
        let Self {
            baton,
            observations_pulled_out,
            idle_keeper,
        } = self;
        // Dropping `start_tx` wakes the idle keeper thread to exit cleanly
        // with `Ok(None)` without ever touching the World.
        drop(idle_keeper.start_tx);
        let _ = idle_keeper.keeper_join.join();
        if observations_pulled_out {
            open.ctx
                .with_green_runtime(|rt| rt.carried.borrow_mut().swap_observations_only());
        }
        open.restore_from_baton(baton);
    }
}

/// Phase 3: `Baton` has been transferred into `PreparedHeapTransfer` and sent
/// to the already-spawned keeper thread; Main is awaiting `ready_rx.recv()`.
struct KeeperStarting {
    pending_return: PendingHeapReturn,
    saved_observation_placeholder: Box<dyn std::any::Any>,
    keeper_join: KeeperJoinHandle,
    driver_drained: Arc<AtomicBool>,
    world_handle: EraWorldHandle,
}

impl<'a> OpenPhaseRollback<'a> for KeeperStarting {
    fn rollback(self, open: OpenContext<'a>) {
        let Self {
            pending_return,
            saved_observation_placeholder,
            keeper_join,
            driver_drained: _,
            world_handle,
        } = self;
        drop(world_handle);
        match keeper_join.join() {
            Ok(Ok(Some(returned_end))) => {
                open.restore_from_returned_end(
                    pending_return,
                    saved_observation_placeholder,
                    returned_end,
                    false,
                );
            }
            Ok(Err(notice)) => {
                // `ScopedEraFailure` permanently poisons and retains
                // `WorldLoan` inside `!Send ManuallyDrop<EraScope>`. Never
                // return to Lisp with `SoloSurrendered` and parked `TAGGED_HEAP`
                // TLS; fail closed.
                tracing::error!(
                    ?notice,
                    "E6-P4-I1 rollback: unrecoverable keeper admission failure after transfer; aborting fail-closed"
                );
                std::process::abort();
            }
            Ok(Ok(None)) | Err(_) => {
                tracing::error!(
                    "E6-P4-I1 rollback: keeper thread terminated unexpectedly during startup; aborting fail-closed"
                );
                std::process::abort();
            }
        }
    }
}

/// Phase 4: the keeper acknowledged readiness and is running `keeper_serve` +
/// `Driver::run()`, before `Control::open_era` has committed.
struct KeeperStarted {
    pending_return: PendingHeapReturn,
    saved_observation_placeholder: Box<dyn std::any::Any>,
    keeper_join: KeeperJoinHandle,
    driver_drained: Arc<AtomicBool>,
    inbox: mpsc::Sender<KeeperMsg>,
    world_handle: EraWorldHandle,
    group_port: GroupPort,
}

impl<'a> OpenPhaseRollback<'a> for KeeperStarted {
    fn rollback(self, open: OpenContext<'a>) {
        let Self {
            pending_return,
            saved_observation_placeholder,
            keeper_join,
            driver_drained: _,
            inbox,
            world_handle,
            group_port,
        } = self;
        group_port.exit();
        drop(inbox);
        drop(world_handle);
        match keeper_join.join() {
            Ok(Ok(Some(returned_end))) => {
                open.restore_from_returned_end(
                    pending_return,
                    saved_observation_placeholder,
                    returned_end,
                    false,
                );
            }
            Ok(Err(notice)) => {
                tracing::error!(
                    ?notice,
                    "E6-P4-I1 rollback: started keeper exited with error; aborting fail-closed"
                );
                std::process::abort();
            }
            Ok(Ok(None)) | Err(_) => {
                tracing::error!(
                    "E6-P4-I1 rollback: started keeper terminated unexpectedly; aborting fail-closed"
                );
                std::process::abort();
            }
        }
    }
}

/// Phase 5: `Control::open_era` committed; Main is about to acquire its
/// initial `TurnRight::Era` and commit into [`LiveEraRuntime`].
struct EraOpened {
    pending_return: PendingHeapReturn,
    saved_observation_placeholder: Box<dyn std::any::Any>,
    keeper_join: KeeperJoinHandle,
    driver_drained: Arc<AtomicBool>,
    inbox: mpsc::Sender<KeeperMsg>,
}

impl<'a> OpenPhaseRollback<'a> for EraOpened {
    fn rollback(self, open: OpenContext<'a>) {
        let Self {
            pending_return,
            saved_observation_placeholder,
            keeper_join,
            driver_drained: _,
            inbox,
        } = self;
        if let Some(gp) = open.control.take_era_port_for_exit() {
            gp.exit();
        }
        drop(inbox);
        match keeper_join.join() {
            Ok(Ok(Some(returned_end))) => {
                open.restore_from_returned_end(
                    pending_return,
                    saved_observation_placeholder,
                    returned_end,
                    true,
                );
            }
            Ok(Err(notice)) => {
                tracing::error!(
                    ?notice,
                    "E6-P4-I1 rollback: opened-era keeper exited with error; aborting fail-closed"
                );
                std::process::abort();
            }
            Ok(Ok(None)) | Err(_) => {
                tracing::error!(
                    "E6-P4-I1 rollback: opened-era keeper terminated unexpectedly; aborting fail-closed"
                );
                std::process::abort();
            }
        }
    }
}

impl<'a> LiveEraOpenGuard<'a, SoloSurrendered> {
    /// Spawn the idle keeper OS thread BEFORE moving `Baton` out of
    /// `SoloSurrendered`. If OS thread creation fails (or `FailKeeperSpawn`
    /// is injected), `self` drops as `SoloSurrendered` and cleanly restores
    /// `Control::SoloActive` and Main's `TAGGED_HEAP` TLS.
    fn spawn_idle_keeper(
        self,
    ) -> Result<LiveEraOpenGuard<'a, IdleKeeperSpawned>, LiveEraOpenError> {
        #[cfg(test)]
        if take_era_open_fault_matching(LiveEraOpenFault::FailKeeperSpawn) {
            return Err(LiveEraOpenError::KeeperStartFailed);
        }

        let pending_spawns: Arc<Mutex<VecDeque<PendingOwnerSpawn>>> =
            Arc::new(Mutex::new(VecDeque::new()));
        let keeper_spawns = Arc::clone(&pending_spawns);
        let driver_drained = Arc::new(AtomicBool::new(false));
        let keeper_drained = Arc::clone(&driver_drained);
        let (start_tx, start_rx) = mpsc::sync_channel::<PreparedEraStart>(1);
        let (ready_tx, ready_rx) = mpsc::sync_channel::<
            Result<(GroupPort, mpsc::Sender<KeeperMsg>), KeeperStartNotice>,
        >(1);

        let keeper_join = std::thread::Builder::new()
            .name("neomacs-keeper".to_string())
            .spawn(move || {
                let Ok(prepared) = start_rx.recv() else {
                    // Main rolled back during preflight before transferring
                    // the heap; exit cleanly without touching any World state.
                    return Ok(None);
                };
                let outcome = keeper_serve(prepared, |era| {
                    std::thread::scope(|scope| {
                        let (inbox_tx, inbox_rx) = mpsc::channel::<KeeperMsg>();
                        let mut driver = Driver::new(era, inbox_rx);
                        #[cfg(test)]
                        {
                            // E7-P D3: test suites use 100ms overdue bound so
                            // production's 5s threshold stays out of test noise.
                            driver = driver.with_stop_bound(Duration::from_millis(100));
                        }
                        let group_link = driver.enroll_group();
                        let group_port = GroupPort::new(inbox_tx.clone(), group_link);
                        if ready_tx.send(Ok((group_port, inbox_tx))).is_err() {
                            return;
                        }
                        let spawns_ref = &keeper_spawns;
                        let owner_handles = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
                        let hook_handles = std::rc::Rc::clone(&owner_handles);
                        driver = driver.with_between_stops_hook(move |drv| {
                            let mut handles = hook_handles.borrow_mut();
                            drain_pending_spawns(era, scope, drv, spawns_ref, &mut handles);
                        });
                        if driver.run() == DriverEnd::Drained {
                            keeper_drained.store(true, Ordering::Release);
                        }
                        // Explicitly join every scoped owner OS thread (real
                        // OS join, completing all TLS destructors) before
                        // `keeper_serve` finishes the era.
                        for handle in owner_handles.borrow_mut().drain(..) {
                            let _ = handle.join();
                        }
                    });
                });
                match outcome {
                    Ok((end, ())) => Ok(Some(end)),
                    Err(failure) => {
                        let notice = failure.notice();
                        let _ = ready_tx.send(Err(notice));
                        Err(notice)
                    }
                }
            })
            .map_err(|_| LiveEraOpenError::KeeperStartFailed)?;

        let idle_keeper = SpawnedIdleKeeper {
            start_tx,
            ready_rx,
            keeper_join,
            driver_drained,
            pending_spawns,
        };
        let (open, SoloSurrendered { baton }) = self.into_parts();
        Ok(LiveEraOpenGuard::new(
            open,
            IdleKeeperSpawned {
                baton,
                observations_pulled_out: false,
                idle_keeper,
            },
        ))
    }
}

impl<'a> LiveEraOpenGuard<'a, IdleKeeperSpawned> {
    fn baton_ref(&self) -> &Baton {
        &self.inner.1.baton
    }

    fn heap_ptr(&self) -> *mut neovm_value::tagged::gc::TaggedHeap {
        self.inner.0.ctx.tagged_heap.heap_root().as_ptr()
    }

    fn pull_observations_to_tls(&mut self) {
        if !self.inner.1.observations_pulled_out {
            self.inner
                .0
                .ctx
                .with_green_runtime(|rt| rt.carried.borrow_mut().swap_observations_only());
            self.inner.1.observations_pulled_out = true;
        }
    }

    /// Atomically transfer `Baton` into the already-spawned keeper thread and
    /// advance to `LiveEraOpenGuard<KeeperStarting>`.
    ///
    /// `AbortOnUnwind` protects the narrow by-value transfer window so even an
    /// unexpected panic inside `prepare_reference_free_transfer` or channel
    /// send fails closed instead of silently skipping rollback.
    #[allow(clippy::type_complexity)]
    fn transfer_to_keeper(
        self,
        admission: Gen1Admission<'_>,
        frozen: neovm_value::tagged::gc::FrozenLegacyRoots,
        world_handle: EraWorldHandle,
    ) -> Result<
        (
            LiveEraOpenGuard<'a, KeeperStarting>,
            mpsc::Receiver<Result<(GroupPort, mpsc::Sender<KeeperMsg>), KeeperStartNotice>>,
            Arc<Mutex<VecDeque<PendingOwnerSpawn>>>,
        ),
        LiveEraOpenError,
    > {
        let abort_guard = AbortOnUnwind::arm("transfer_to_keeper");
        let (
            open,
            IdleKeeperSpawned {
                baton,
                observations_pulled_out,
                idle_keeper,
            },
        ) = self.into_parts();

        // SAFETY: `park_seated` uninstalled carrier heap TLS, `drain_to_quiescent`
        // quiesced the heap, and `baton` is the sole World authority.
        let (pending_return, transfer) =
            match unsafe { prepare_reference_free_transfer(admission, baton, frozen) } {
                Ok(pair) => pair,
                Err(failure) => {
                    let (reason, admission, owner, _roots, _obs) = failure.into_parts();
                    tracing::warn!(?reason, "E6-P.4 prepare_reference_free_transfer refused");
                    drop(admission);
                    let baton = Baton::from_world_loan(owner);
                    let restored_guard = LiveEraOpenGuard::new(
                        open,
                        IdleKeeperSpawned {
                            baton,
                            observations_pulled_out,
                            idle_keeper,
                        },
                    );
                    abort_guard.disarm();
                    drop(restored_guard);
                    return Err(LiveEraOpenError::TransferFailed);
                }
            };

        // Preserve `PendingHeapReturn`'s exact observation placeholder out of
        // Main's TLS so resuming Main (and any Baton workers) during the live
        // era cannot displace it before `PendingHeapReturn::restore()`.
        let saved_observation_placeholder = stash_current_thread_observations();

        let SpawnedIdleKeeper {
            start_tx,
            ready_rx,
            keeper_join,
            driver_drained,
            pending_spawns,
        } = idle_keeper;
        let prepared = PreparedEraStart::from_heap_transfer(transfer);
        if let Err(mpsc::SendError(prepared)) = start_tx.send(prepared) {
            // The idle keeper exited before reading `start_rx`, so `prepared`
            // never entered `keeper_serve` and returns cleanly to Main.
            let _ = keeper_join.join();
            drop(world_handle);
            let returned_end = ReturnedEraEnd::from_transfer(prepared.into_heap_transfer())
                .expect("unstarted PreparedHeapTransfer returns cleanly to source");
            open.restore_from_returned_end(
                pending_return,
                saved_observation_placeholder,
                returned_end,
                false,
            );
            abort_guard.disarm();
            return Err(LiveEraOpenError::KeeperStartFailed);
        }

        let next = LiveEraOpenGuard::new(
            open,
            KeeperStarting {
                pending_return,
                saved_observation_placeholder,
                keeper_join,
                driver_drained,
                world_handle,
            },
        );
        abort_guard.disarm();
        Ok((next, ready_rx, pending_spawns))
    }
}

impl<'a> LiveEraOpenGuard<'a, KeeperStarting> {
    /// Block on the keeper's readiness handshake while `KeeperStarting` stays
    /// armed, advancing to `LiveEraOpenGuard<KeeperStarted>` only on success.
    fn await_ready(
        self,
        ready_rx: mpsc::Receiver<Result<(GroupPort, mpsc::Sender<KeeperMsg>), KeeperStartNotice>>,
    ) -> Result<LiveEraOpenGuard<'a, KeeperStarted>, LiveEraOpenError> {
        let (group_port, inbox) = match ready_rx.recv() {
            Ok(Ok(pair)) => pair,
            Ok(Err(notice)) => {
                tracing::warn!(?notice, "E6-P.4 keeper thread refused era admission");
                return Err(LiveEraOpenError::KeeperStartFailed);
            }
            Err(_) => {
                tracing::warn!("E6-P.4 keeper readiness channel closed before startup");
                return Err(LiveEraOpenError::KeeperStartFailed);
            }
        };

        let (
            open,
            KeeperStarting {
                pending_return,
                saved_observation_placeholder,
                keeper_join,
                driver_drained,
                world_handle,
            },
        ) = self.into_parts();
        Ok(LiveEraOpenGuard::new(
            open,
            KeeperStarted {
                pending_return,
                saved_observation_placeholder,
                keeper_join,
                driver_drained,
                inbox,
                world_handle,
                group_port,
            },
        ))
    }
}

impl<'a> LiveEraOpenGuard<'a, KeeperStarted> {
    fn open_era_on_control(self) -> Result<LiveEraOpenGuard<'a, EraOpened>, LiveEraOpenError> {
        let abort_guard = AbortOnUnwind::arm("open_era_on_control");
        let (
            open,
            KeeperStarted {
                pending_return,
                saved_observation_placeholder,
                keeper_join,
                driver_drained,
                inbox,
                world_handle,
                group_port,
            },
        ) = self.into_parts();

        match open.control.open_era(world_handle, group_port) {
            Ok(()) => {
                let next = LiveEraOpenGuard::new(
                    open,
                    EraOpened {
                        pending_return,
                        saved_observation_placeholder,
                        keeper_join,
                        driver_drained,
                        inbox,
                    },
                );
                abort_guard.disarm();
                Ok(next)
            }
            Err((world_handle, group_port, _err)) => {
                let rollback_guard = LiveEraOpenGuard::new(
                    open,
                    KeeperStarted {
                        pending_return,
                        saved_observation_placeholder,
                        keeper_join,
                        driver_drained,
                        inbox,
                        world_handle,
                        group_port,
                    },
                );
                abort_guard.disarm();
                drop(rollback_guard);
                Err(LiveEraOpenError::OpenEraFailed)
            }
        }
    }
}

impl<'a> LiveEraOpenGuard<'a, EraOpened> {
    /// Commit the already-opened era and withdraw Main's legacy serial JIT at
    /// the exact commit boundary (after all fallible transfer/open/turn steps
    /// have already succeeded).
    ///
    /// Rollback safety (P1):
    /// - Any refusal before this function (`transfer_to_keeper`, `await_ready`,
    ///   `open_era_on_control`, `acquire_initial_era_turn`) never touches the
    ///   one-way `ManuallyDrop` withdrawal receipts and rolls back cleanly to
    ///   `SoloActive` with `legacy_jit_withdrawn() == false` and intact JIT.
    /// - If `source_jit_cache::withdraw` refuses here, zero JIT TLS slots were
    ///   mutated: `era_right` is returned to `Idle` and `EraOpened::drop` rolls
    ///   back cleanly to `SoloActive` with `legacy_jit_withdrawn() == false`.
    /// - Once `source_jit_cache::withdraw` succeeds (the single irreversible
    ///   point of no return, since `DetachedSourceCache` permanently retains
    ///   its `ManuallyDrop` payload), `AbortOnUnwind` is armed and any refusal
    ///   in `source_jit_backend::withdraw` or `source_jit_compiler::close`
    ///   aborts fail-closed so Lisp can never resume with leaked JIT state and
    ///   a false withdrawal flag.
    ///
    /// Session-lifetime contract (P2):
    /// - On success, `mark_legacy_jit_withdrawn()` sets `World::legacy_jit_withdrawn`
    ///   monotonically `false -> true` for the remainder of the session. It is
    ///   never cleared on `teardown_live_parallel_era()`, matching the one-way
    ///   `ManuallyDrop` retention of `WithdrawnLegacyJit`.
    fn commit(
        mut self,
        mut era_right: TurnRight,
        pending_spawns: Arc<Mutex<VecDeque<PendingOwnerSpawn>>>,
    ) -> Result<LiveEraRuntime, LiveEraOpenError> {
        #[cfg(test)]
        if take_era_open_fault_matching(LiveEraOpenFault::FailLegacyJitWithdrawalStructural) {
            let proof = prove_stowed_for_era_switch(self.inner.0.ctx);
            era_right.set_stowed_for_return(proof);
            self.inner.0.control.release_era_turn_to_idle(era_right);
            return Err(LiveEraOpenError::LegacyJitWithdrawalFailed);
        }

        // SAFETY: `ParkedWorldSeat::park_seated` uninstalled `TAGGED_HEAP` on
        // Main, Main holds the exclusive initial `TurnRight::Era` (no parallel
        // owners are spawned yet), and `native_depth() == 0` is enforced.
        let cache = match unsafe { source_jit_cache::withdraw(self.inner.0.ctx) } {
            Ok(cache) => cache,
            Err(err) => {
                // `source_jit_cache::withdraw` guarantees zero mutation on
                // refusal. Return `era_right` to `Idle` so dropping `self`
                // runs `EraOpened::rollback` cleanly back to `SoloActive`.
                let proof = prove_stowed_for_era_switch(self.inner.0.ctx);
                era_right.set_stowed_for_return(proof);
                self.inner.0.control.release_era_turn_to_idle(era_right);
                #[cfg(feature = "jit")]
                {
                    use crate::emacs_core::jit::cache::source_detachment::SourceCacheError;
                    return Err(match err {
                        SourceCacheError::NativeActivation => LiveEraOpenError::NativeActivation,
                        SourceCacheError::PendingJobs | SourceCacheError::Borrowed(_) => {
                            LiveEraOpenError::LegacyJitBusy
                        }
                        _ => LiveEraOpenError::LegacyJitWithdrawalFailed,
                    });
                }
                #[cfg(not(feature = "jit"))]
                match err {}
            }
        };

        // Point of no return: `DetachedSourceCache` has irreversibly moved
        // Main's legacy cache into `ManuallyDrop`. Any failure or unwind from
        // here until `mark_legacy_jit_withdrawn()` completes MUST fail closed.
        let abort_guard = AbortOnUnwind::arm("commit");
        // SAFETY: `cache` is the actual withdrawn cache receipt for `open.ctx`.
        let backend = match unsafe { source_jit_backend::withdraw(&cache) } {
            Ok(backend) => backend,
            Err(err) => {
                tracing::error!(
                    ?err,
                    "legacy JIT backend withdrawal failed after irreversible cache withdrawal; aborting fail-closed"
                );
                std::process::abort();
            }
        };
        // SAFETY: `backend` is the actual preceding withdrawn backend receipt.
        let compiler = match unsafe { source_jit_compiler::close(&backend) } {
            Ok(compiler) => compiler,
            Err(err) => {
                tracing::error!(
                    ?err,
                    "legacy JIT compiler scope closure failed after irreversible cache/backend withdrawal; aborting fail-closed"
                );
                std::process::abort();
            }
        };
        let withdrawn_legacy_jit = WithdrawnLegacyJit {
            _cache: cache,
            _backend: backend,
            _compiler: compiler,
        };

        let (
            open,
            EraOpened {
                pending_return,
                saved_observation_placeholder,
                keeper_join,
                driver_drained,
                inbox,
            },
        ) = self.into_parts();
        open.parked.resume_with_turn_right(open.ctx, era_right);
        open.ctx.mark_legacy_jit_withdrawn();
        abort_guard.disarm();
        Ok(LiveEraRuntime {
            pending_return: Some(pending_return),
            saved_observation_placeholder: Some(saved_observation_placeholder),
            keeper_join: Some(keeper_join),
            driver_drained,
            inbox: Some(inbox),
            pending_spawns,
            next_owner_id: AtomicU64::new(1),
            dead_windows_recovery: open.dead_windows_recovery,
            _withdrawn_legacy_jit: Some(Box::new(withdrawn_legacy_jit) as Box<dyn std::any::Any>),
        })
    }
}

/// Drain any pending opt-in owner spawn requests strictly between stops on the
/// keeper OS thread (`Driver::with_between_stops_hook`).
fn drain_pending_spawns<'scope, 'k, 'service, 'heap>(
    era: &'k KeeperEra<'service, 'heap>,
    scope: &'scope std::thread::Scope<'scope, '_>,
    driver: &mut Driver<'k, 'service, 'heap>,
    pending_spawns: &Mutex<VecDeque<PendingOwnerSpawn>>,
    owner_handles: &mut Vec<std::thread::ScopedJoinHandle<'scope, ()>>,
) where
    'k: 'scope,
    'service: 'scope,
    'heap: 'scope,
{
    let queued: Vec<PendingOwnerSpawn> = {
        let mut guard = pending_spawns
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.drain(..).collect()
    };
    for req in queued {
        if req.registered.control().lifecycle() == Lifecycle::Closing {
            // Dropping `req.registered` calls `control.unregister_parallel(key)`.
            continue;
        }
        let permit = match era.register(req.owner_id, OWNER_ROOT_CAPACITY) {
            Ok(permit) => permit,
            Err(error) => {
                tracing::error!(
                    owner_id = req.owner_id,
                    ?error,
                    "keeper failed to register admitted parallel owner"
                );
                // Dropping `req.registered` unregisters the participant cleanly.
                continue;
            }
        };
        let key = permit.key();
        let home = SerialHome {
            heap: era.heap_identity(),
            owner: req.owner_id,
        };
        let (grants_tx, grants_rx) = mpsc::sync_channel(1);
        driver.enroll(req.owner_id, key, grants_tx);
        let publishing_owners = driver.publishing_owners_counter();
        let thread_name = format!("neomacs-par-{}", req.owner_id);
        let exit_inbox = req.owner_inbox.clone();
        let spawn_result =
            std::thread::Builder::new()
                .name(thread_name)
                .spawn_scoped(scope, move || {
                    run_parallel_opt_in_owner(
                        home,
                        permit,
                        req.owner_inbox,
                        grants_rx,
                        publishing_owners,
                        req.world_handle,
                        req.registered,
                    );
                });
        match spawn_result {
            Ok(handle) => owner_handles.push(handle),
            Err(error) => {
                tracing::error!(
                    owner_id = req.owner_id,
                    ?error,
                    "keeper failed to spawn OS thread for admitted parallel owner"
                );
                let _ = exit_inbox.send(KeeperMsg::Exit {
                    owner: req.owner_id,
                });
                // `req.registered` was moved into the unstarted closure and
                // drops here, unregistering `Participant::Parallel`.
            }
        }
    }
}

fn activate_owner_permit<'era>(
    mut permit: MutatorPermit<'era>,
    home: SerialHome,
    inbox: &mpsc::Sender<KeeperMsg>,
) -> Mutator<'era> {
    let start = std::time::Instant::now();
    loop {
        match permit.activate(home.heap, home.owner) {
            Ok(mutator) => return mutator,
            Err(failure)
                if failure.reason == RegistrationError::Busy
                    && start.elapsed() < TEARDOWN_OWNER_EXIT_BOUND =>
            {
                permit = failure.into_permit();
                let _ = inbox.send(KeeperMsg::Wake);
                std::thread::yield_now();
            }
            Err(failure) => {
                tracing::error!(
                    owner = home.owner,
                    reason = ?failure.reason,
                    "admitted parallel owner failed to activate MutatorPermit"
                );
                std::process::abort();
            }
        }
    }
}

fn retire_owner_mutator(
    mut mutator: Mutator<'_>,
    inbox: &mpsc::Sender<KeeperMsg>,
) -> Result<(), RendezvousError> {
    let start = std::time::Instant::now();
    loop {
        match mutator.retire_rooted(&[]) {
            Ok(()) => return Ok(()),
            Err(failure)
                if matches!(failure.reason, RendezvousError::Busy)
                    && start.elapsed() < TEARDOWN_OWNER_EXIT_BOUND =>
            {
                mutator = failure.into_owner();
                let _ = inbox.send(KeeperMsg::Wake);
                std::thread::yield_now();
            }
            Err(failure) => return Err(failure.reason),
        }
    }
}

fn standalone_serial_activate(
    _local: &mut LocalSerialOwner,
) -> Result<(), std::convert::Infallible> {
    Ok(())
}

/// Opt-in parallel owner OS-thread entry point (`E6-P.4` Seam 3).
///
/// Runs inside the keeper's `std::thread::scope` so `MutatorPermit<'era>` and
/// `ChannelPort<'era>` require zero lifetime erasure and every owner OS thread
/// is guaranteed joined before `era.finish()`. Composes the two independent
/// TLS scopes (`SERIAL_TURN_COLLECT_SCOPE` and `SERIAL_TURN_PARK_SCOPE`) over
/// one [`SerialRebindFrame`](super::serial_park::SerialRebindFrame) and
/// retires directly through G5 Steps 1–6 via [`finish_live_turn_for_retire`]
/// and [`complete_parallel_owner_retirement`].
#[allow(clippy::too_many_arguments)]
fn run_parallel_opt_in_owner<'era>(
    home: SerialHome,
    permit: MutatorPermit<'era>,
    inbox: mpsc::Sender<KeeperMsg>,
    grants: mpsc::Receiver<super::keeper_driver::GrantMsg>,
    publishing_owners: Arc<std::sync::atomic::AtomicUsize>,
    world_handle: EraWorldHandle,
    registered: RegisteredParallelOwner,
) {
    let mut owner = ParallelOwnerControl::from_registered(registered);
    let thread_key = owner.key();
    let control = Arc::clone(owner.control());
    let wake_inbox = inbox.clone();
    let mutator = activate_owner_permit(permit, home, &wake_inbox);
    let mut port = ChannelPort::new(home.owner, mutator, inbox, grants, publishing_owners);

    // Acquire our first SerialSeat BEFORE touching World or constructing Context.
    let initial_grant = loop {
        match port.request_turn() {
            Ok(granted) => break granted,
            Err(PortRefusal::Retry) => std::thread::yield_now(),
            Err(PortRefusal::Poisoned | PortRefusal::Failed) => {
                let mutator = port.exit();
                let _ = retire_owner_mutator(mutator, &wake_inbox);
                drop(wake_inbox);
                clear_current_thread_observations();
                owner.unregister();
                return;
            }
        }
    };
    let (seat, local, guard) =
        match receive_initial_seat(home, initial_grant, &mut port, standalone_serial_activate) {
            Ok(received) => received,
            Err(_) => {
                let mutator = port.exit();
                let _ = retire_owner_mutator(mutator, &wake_inbox);
                drop(wake_inbox);
                clear_current_thread_observations();
                owner.unregister();
                return;
            }
        };

    // Holding `seat` proves exclusive World ownership on this OS thread.
    let ctx = world_handle.spawn_unseated_serial_context(&seat);
    let mut state = match OwnerSerialState::new(home.owner, Box::new(ctx)) {
        Ok(state) => state,
        Err((_ctx, _refusal)) => {
            // Fall back to clean port exit without leaking the ticket.
            super::serial_park::give_back_initial_seat(seat, local, guard, &mut port);
            let mutator = port.exit();
            let _ = retire_owner_mutator(mutator, &wake_inbox);
            drop(wake_inbox);
            clear_current_thread_observations();
            owner.unregister();
            return;
        }
    };

    let finished = match state.begin(local) {
        Ok(turn) => {
            let mut live = LiveSerial::from_parts(turn, seat, guard, home);
            let exit_kind =
                live.with_rebind_frame(&mut port, standalone_serial_activate, |ev, frame| {
                    frame.with_collect_scope(ev, |ev| {
                        with_park_scope_on_frame(frame, ev, &mut owner, |ev| {
                            run_opt_in_owner_lisp_body(ev, &control, thread_key)
                        })
                    })
                });
            // Transition directly from this live turn into G5 Steps 1–3a so
            // zero unrooted `EvalResult`/`Value` crosses a turn boundary and
            // `finish_live_turn_for_retire` unwinds specpdl to depth 0, mints
            // `CompletingThread`, detaches the dynamic-module registry, and
            // runs final detach (`turn.end()`).
            Some(finish_live_turn_for_retire(
                live,
                thread_key,
                exit_kind,
                |_| {},
            ))
        }
        Err((local, _refusal)) => {
            super::serial_park::give_back_initial_seat(seat, local, guard, &mut port);
            None
        }
    };

    if let Some(finished) = finished {
        let _ = complete_parallel_owner_retirement(state, owner, None, port, finished, |mutator| {
            retire_owner_mutator(mutator, &wake_inbox)
        });
        drop(wake_inbox);
        drop(world_handle);
    } else {
        let mutator = port.exit();
        let _ = retire_owner_mutator(mutator, &wake_inbox);
        drop(wake_inbox);
        drop(state);
        drop(world_handle);
        clear_current_thread_observations();
        owner.unregister();
    }
}

/// Execute the admitted opt-in parallel owner's Lisp function inside its live
/// `SerialTurn`, recording its result/error (or `ShutdownRequest`) while the
/// turn is still active so zero unrooted `Value` crosses a turn boundary.
/// Returns the [`CompletionExit`](crate::emacs_core::threads::CompletionExit)
/// for [`finish_live_turn_for_retire`].
fn run_opt_in_owner_lisp_body(
    ev: &mut Context,
    control: &Control,
    key: ThreadKey,
) -> crate::emacs_core::threads::CompletionExit {
    let thread_id = key.id();
    let function =
        ev.with_green_runtime(|runtime| runtime.pending_starts.borrow_mut().remove(&key));

    if control.lifecycle() == Lifecycle::Closing {
        return crate::emacs_core::threads::CompletionExit::ClosingBeforeStart;
    }

    let Some(function) = function else {
        return crate::emacs_core::threads::CompletionExit::Executed;
    };

    if let Some(buf) = ev.threads.thread_current_buffer(thread_id) {
        let _ = ev.edit.buffers.set_current(buf);
    }
    ev.threads.mark_handlerlist_installed(thread_id);

    let specpdl_base = ev.specpdl.len();
    let result = ev.apply(function, vec![]);
    // Unwind any dynamic bindings introduced by `function` before recording
    // its outcome; `finish_live_turn_for_retire` independently asserts
    // `ev.unbind_to(0)` / `ev.specpdl.is_empty()` for G5 Step 2.
    ev.unbind_to(specpdl_base);
    debug_assert_eq!(
        ev.specpdl.len(),
        specpdl_base,
        "G5: specpdl must unwind to depth 0 before final SerialTurn::end"
    );

    if let Err(ref flow) = result {
        if let Some(shutdown) = Flow::shutdown_request(flow) {
            if control.lifecycle() != Lifecycle::Closing {
                ev.with_green_runtime(|runtime| {
                    if runtime.exit.get().is_none() {
                        runtime.exit.set(Some(shutdown));
                    }
                });
            }
            ev.threads.finish_thread(thread_id, Value::NIL);
        } else {
            let _ = crate::emacs_core::threads::finish_make_thread_result(
                &mut ev.threads,
                thread_id,
                result,
            );
        }
    } else {
        let _ = crate::emacs_core::threads::finish_make_thread_result(
            &mut ev.threads,
            thread_id,
            result,
        );
    }

    crate::emacs_core::threads::CompletionExit::Executed
}

/// Outcome of attempting to spawn an admitted opt-in parallel thread.
#[derive(Debug)]
pub(crate) enum SpawnParallelError {
    /// Lisp arity or argument-type validation error (must be surfaced to Lisp,
    /// matching `make-thread`, never swallowed by Baton fallback).
    Lisp(Flow),
    /// Runtime admission refusal (not an error; caller falls back to Baton
    /// `builtin_make_thread` with `neomacs-thread-parallel-p` = `nil`).
    Refused(LiveEraOpenError),
}

impl Context {
    /// Ensure a live parallel keeper era is open on this Green World (`E6-P.4`
    /// Seam 1 + `E6-P4-I1`).
    ///
    /// - If a live era is already open on `GreenRuntime`, returns `Ok(())`.
    /// - Otherwise, requires Main seated with `TurnRight::Solo(Baton)` and GEN1
    ///   enabled, performs `(a) surrender Solo -> (b) start keeper & open_era ->
    ///   (c) acquire_initial_era_turn` under [`LiveEraOpenGuard`].
    #[cold]
    #[inline(never)]
    pub(crate) fn ensure_live_parallel_era(&mut self) -> Result<(), LiveEraOpenError> {
        #[cfg(test)]
        ERA_OPEN_ATTEMPTS.fetch_add(1, Ordering::Relaxed);

        if self
            .green_runtime_ref()
            .as_ref()
            .is_some_and(|rt| rt.live_era.borrow().is_some())
        {
            return Ok(());
        }
        if self.gc_driver_active || self.gc_inhibit_depth > 0 {
            return Err(LiveEraOpenError::GcBusy);
        }
        if !self.tagged_heap.tagged_heap_generational_enabled() {
            return Err(LiveEraOpenError::GenerationalRequired);
        }
        let Some(seat) = self.green_seat() else {
            return Err(LiveEraOpenError::NotSeatedMain);
        };
        if seat.key != ThreadKey::Main {
            return Err(LiveEraOpenError::NotSeatedMain);
        }
        if !seat.baton_ref().is_some_and(TurnRight::is_solo) {
            return Err(LiveEraOpenError::NotSolo);
        }
        let control = Arc::clone(seat.control());
        if control.lifecycle() != Lifecycle::Running {
            return Err(LiveEraOpenError::Closing);
        }
        #[cfg(feature = "jit")]
        {
            if crate::emacs_core::jit::cache::native_depth() != 0 {
                return Err(LiveEraOpenError::NativeActivation);
            }
            crate::emacs_core::jit::cache::drain_ready_pending(Some(self));
        }

        // Pre-clean GNU's empty `window-dead-windows-table` so the weak-table
        // fence admits a fresh Context / pdump session cleanly.
        let dead_windows_recovery = self.retire_empty_dead_windows_table();
        self.tagged_heap.close_alloc_regions();
        if self.prepare_parked_root_retirement().is_err() {
            if matches!(
                dead_windows_recovery,
                super::super::world::SerialRecovery::RecreateDeadWindows
            ) {
                self.install_empty_dead_windows_table();
            }
            return Err(LiveEraOpenError::DrainFailed);
        }

        // SAFETY: `LiveEraOpenGuard` unconditionally resumes `parked` onto `self`
        // on every success, error, and panic-unwind exit path.
        let parked = match unsafe { ParkedWorldSeat::park_seated(self) } {
            Ok(parked) => parked,
            Err(_) => {
                if matches!(
                    dead_windows_recovery,
                    super::super::world::SerialRecovery::RecreateDeadWindows
                ) {
                    self.install_empty_dead_windows_table();
                }
                return Err(LiveEraOpenError::SeatPark);
            }
        };

        let right = self
            .green_seat()
            .expect("Main is seated")
            .take_baton()
            .expect("Main holds Solo Baton");
        let baton = match control.surrender_solo_for_era(right) {
            Ok(baton) => baton,
            Err((right, _err)) => {
                parked.resume_with_turn_right(self, right);
                if matches!(
                    dead_windows_recovery,
                    super::super::world::SerialRecovery::RecreateDeadWindows
                ) {
                    self.install_empty_dead_windows_table();
                }
                return Err(LiveEraOpenError::NotSolo);
            }
        };

        // Arm E6-P4-I1 linear typestate guard immediately after surrendering Solo.
        let guard = LiveEraOpenGuard::new(
            OpenContext {
                ctx: self,
                control: Arc::clone(&control),
                parked,
                dead_windows_recovery,
            },
            SoloSurrendered { baton },
        );

        #[cfg(test)]
        {
            if take_era_open_fault_matching(LiveEraOpenFault::FailAfterSurrenderBeforeOpen) {
                return Err(LiveEraOpenError::InjectedFault);
            }
            if take_era_open_fault_matching(LiveEraOpenFault::PanicAfterSurrenderBeforeOpen) {
                panic!("E6-P4-I1 test fault: panic between surrender_solo_for_era and open_era");
            }
        }

        // Spawn the idle keeper OS thread FIRST while `Baton` is still owned
        // non-optionally in `LiveEraOpenGuard<SoloSurrendered>`, so OS thread
        // creation failure rolls back cleanly without moving `Baton`.
        let mut guard = guard.spawn_idle_keeper()?;

        // Move the heap's canonical `ObservationWorld` back into Main's TLS
        // `STATE` so `prepare_reference_free_transfer` -> `detach_source_observations`
        // finds `RefOwners::HeapAndTls` and installs its receipt placeholder.
        guard.pull_observations_to_tls();

        #[cfg(test)]
        if take_era_open_fault_matching(LiveEraOpenFault::PanicAfterIdleKeeperSpawn) {
            panic!("E6-P4-I1 test fault: panic after idle keeper spawn and observation pull");
        }

        // Run all fallible preflights while `Baton` is still non-optionally
        // owned by `LiveEraOpenGuard<IdleKeeperSpawned>`, so any refusal or
        // unwind stops the idle keeper, restores observations, returns
        // `Control` to `SoloActive`, and reinstalls Main's `TAGGED_HEAP` TLS.
        let world_handle = guard
            .baton_ref()
            .era_world_handle()
            .map_err(|_| LiveEraOpenError::TransferFailed)?;
        // Verify epoch retention is valid before transferring `Baton`
        // (dropping `FacadeEpochRetention` immediately releases the count).
        let _epoch_check = guard
            .baton_ref()
            .parallel_epoch_retention()
            .map_err(|_| LiveEraOpenError::TransferFailed)?;
        drop(_epoch_check);

        let heap_ptr = guard.heap_ptr();
        // SAFETY: `guard` exclusively holds `Baton` on this OS thread, Main's
        // roots and all parked Baton roots are published in
        // `GreenRuntime::parked`, World carriers are `Stowed`, and carrier heap
        // TLS is uninstalled.
        let collector = unsafe { (*heap_ptr).drain_to_quiescent(|_| {}) }
            .map_err(|_| LiveEraOpenError::DrainFailed)?;
        let mut admission = match Gen1Admission::check(collector) {
            Ok(admission) => admission,
            Err(failure) => {
                let (_err, collector) = failure.into_parts();
                drop(collector);
                return Err(LiveEraOpenError::GenerationalRequired);
            }
        };
        if let Err(err) = admission.preflight_era_supported() {
            tracing::warn!(?err, "E6-P.4 heap stopped-collection preflight refused");
            drop(admission);
            return Err(LiveEraOpenError::HeapUnsupported);
        }

        // SAFETY: `guard` still exclusively holds `Baton` on this OS thread
        // (so even a panic here rolls back `IdleKeeperSpawned`), Main's live
        // roots stay published in `GreenRuntime::parked` for the whole open
        // era, and `freeze_live_legacy_roots` mints `MayBeActive` inside
        // `eval/world/` (`keeper_world_designator_seam_guard`).
        let frozen = unsafe { freeze_live_legacy_roots(guard.baton_ref()) };

        let (guard, ready_rx, pending_spawns) =
            guard.transfer_to_keeper(admission, frozen, world_handle)?;
        let guard = guard.await_ready(ready_rx)?;
        let guard = guard.open_era_on_control()?;

        #[cfg(test)]
        if take_era_open_fault_matching(LiveEraOpenFault::FailAfterOpenBeforeAcquire) {
            return Err(LiveEraOpenError::InjectedFault);
        }

        let era_right = control
            .acquire_initial_era_turn()
            .map_err(|_| LiveEraOpenError::InitialTurnFailed)?;
        let live_era = guard.commit(era_right, pending_spawns)?;

        self.with_green_runtime(|runtime| {
            *runtime.live_era.borrow_mut() = Some(live_era);
        });
        Ok(())
    }

    /// Tear down an open live parallel era during `green_teardown` (`E6-P.4`
    /// Seam 2 + `E6-P4-I2`).
    ///
    /// 1. **`E6-P4-I2(a)` (retire-by-wake):** wakes all parked opt-in parallel
    ///    owners (which sit in `ParkedSerial` / `condition-wait` and will never
    ///    observe `kill-emacs` unless explicitly woken).
    /// 2. Releases Main's `TurnRight::Era` to `Idle` so woken opt-in owners can
    ///    acquire their final `SerialTurn` to unwind `specpdl`, record exit, and
    ///    send `KeeperMsg::Exit`.
    /// 3. **`E6-P4-I2(b)` (exit-gated `close_era`):** waits until every admitted
    ///    parallel owner has reached `Exit` (zero live tickets or parked images),
    ///    sends `GroupExit`, drops the last `inbox` sender so `Driver::run()`
    ///    drains and joins all owner threads in `std::thread::scope`, joins the
    ///    keeper thread, restores `PendingHeapReturn`, calls `Control::close_era`
    ///    (which independently verifies zero live parallel owners remain), and
    ///    resumes Main under `TurnRight::Solo(Baton)`.
    #[cold]
    #[inline(never)]
    pub(crate) fn teardown_live_parallel_era(&mut self) {
        let Some(mut live_era) = self
            .green_runtime_ref()
            .as_ref()
            .and_then(|rt| rt.live_era.borrow_mut().take())
        else {
            self.reset_parallel_activation_on_teardown();
            return;
        };
        let control = Arc::clone(
            self.green_seat()
                .expect("Main seated for era teardown")
                .control(),
        );
        // E6-P4-I2(a): closing `Control` wakes every blocked
        // `Participant::Parallel` owner via `OwnerWake` so parked waits return
        // `Flow::shutdown` instead of blocking forever.
        control.close();
        if let Some(inbox) = live_era.inbox.as_ref() {
            let _ = inbox.send(KeeperMsg::Wake);
        }

        // Park Main's live seat and release its `TurnRight::Era` to `Idle` so
        // woken opt-in owners can take their exit `SerialTurn`s without deadlock.
        //
        // SAFETY: upholds `ParkedWorldSeat::park_seated`'s 2-part contract
        // (`world_seat.rs:236-242`): no Lisp evaluation, allocation, or
        // collection runs on `self` between here and `parked.resume_with_turn_right`
        // below, and `clear_tagged_heap_if_installed` detaches the store protocol
        // immediately after parking.
        let parked = unsafe { ParkedWorldSeat::park_seated(self) }
            .expect("Main parks cleanly for live era teardown");
        // `park_carrier_tls()` cleared `TAGGED_HEAP` without detaching the
        // observation store protocol; detach it via the existing dormant-heap
        // uninstall path while Main still holds the exclusive `TurnRight::Era`
        // so `era.finish()` observes a detached store.
        //
        // SAFETY: upholds `clear_tagged_heap_if_installed`'s 3-part contract
        // (`gc_thread.rs:1954-1960`):
        // 1. `&self.tagged_heap` is Main's own heap on its owning OS thread;
        // 2. `ParkedWorldSeat::park_seated(self)` just parked Main dormant until
        //    `parked.resume_with_turn_right(...)` restores it below;
        // 3. No TLS-derived heap loan or reference is live after `park_seated`.
        unsafe { neovm_value::tagged::gc::clear_tagged_heap_if_installed(&self.tagged_heap) };
        let mut era_right = self
            .green_seat()
            .expect("Main is seated")
            .take_baton()
            .expect("Main holds TurnRight::Era");
        let proof = prove_stowed_for_era_switch(self);
        era_right.set_stowed_for_return(proof);
        control.release_era_turn_to_idle(era_right);

        // E6-P4-I2(b): gate `GroupExit` and `Control::close_era` on every
        // admitted `Participant::Parallel` owner having completed G5 Steps 1–6.
        let all_exited = control.wait_all_parallel_participants_exited(TEARDOWN_OWNER_EXIT_BOUND);
        assert!(
            all_exited,
            "E6-P4-I2(b): timed out waiting for admitted parallel owners to wake and exit (remaining: {})",
            control.live_parallel_participant_count()
        );

        // Every Baton worker is idle and every opt-in parallel owner has now
        // finished stowing and exited. Drop any `Rc<ObservationWorld>` clone in
        // `GreenRuntime::carried` and Main's TLS `STATE` BEFORE `group_port.exit()`
        // so `keeper_serve` -> `ReturnedEraEnd::from_transfer` observes
        // `RefOwners::HeapOnly`.
        self.with_green_runtime(|rt| {
            rt.carried.borrow_mut().reset_observations_to_placeholder();
        });
        clear_current_thread_observations();

        if let Some(group_port) = control.take_era_port_for_exit() {
            group_port.exit();
        }
        // Drop the last `inbox` sender clone so `Driver::run()` sees
        // `Disconnected`, joins all scoped owner threads, and runs `era.finish()`.
        drop(live_era.inbox.take());

        let join = live_era.keeper_join.take().expect("keeper thread handle");
        let returned_end = join
            .join()
            .expect("keeper thread did not panic")
            .expect("keeper era finished cleanly")
            .expect("started keeper returned ReturnedEraEnd");
        debug_assert!(
            live_era.driver_drained.load(Ordering::Acquire),
            "E6-P4-I2(b): Driver must drain cleanly on teardown"
        );

        // Restore the exact placeholder `PendingHeapReturn` installed at
        // era-open onto Main's TLS `STATE` before `PendingHeapReturn::restore()`.
        if let Some(placeholder) = live_era.saved_observation_placeholder.take() {
            restore_current_thread_observations(placeholder);
        }

        let pending_return = live_era
            .pending_return
            .take()
            .expect("Main holds PendingHeapReturn");
        let restored = pending_return
            .restore(returned_end.into_heap_transfer())
            .expect("PendingHeapReturn::restore succeeds on Main OS thread");
        // SAFETY: Main's OS thread is the original transfer source; keeper
        // joined after every owner exited and no other World loan exists.
        let (baton, _roots, transfer_pub) = unsafe { recover_transferred_source(restored) }
            .expect("recover_transferred_source succeeds after keeper exit");
        // Move the restored canonical `ObservationWorld` into stowed
        // `GreenRuntime::carried` so `resume_with_turn_right`'s
        // `unstow_serial_world_state` installs it cleanly onto Main's TLS.
        self.with_green_runtime(|rt| {
            rt.carried.borrow_mut().swap_observations_only();
        });

        let (solo_right, era_world_handle, _unused_port) = control
            .close_era(baton)
            .expect("E6-P4-I2(b): Control::close_era succeeds once all parallel owners exited");
        drop(era_world_handle);

        parked.resume_with_turn_right(self, solo_right);
        // Session-lifetime withdrawal contract (P2): `World::legacy_jit_withdrawn`
        // remains `true` after `teardown_live_parallel_era()` because
        // `live_era._withdrawn_legacy_jit` drops its one-way `ManuallyDrop`
        // receipts (retaining their payloads rather than restoring TLS).
        // `reset_parallel_activation_on_teardown()` resets only activation state
        // and fast flag while leaving `legacy_jit_withdrawn` monotonic.
        self.reset_parallel_activation_on_teardown();
        let _ = super::super::green::retire_transfer_publication_on_main(self, transfer_pub);
        if matches!(
            live_era.dead_windows_recovery,
            super::super::world::SerialRecovery::RecreateDeadWindows
        ) {
            self.install_empty_dead_windows_table();
        }
    }

    /// Spawn one admitted opt-in parallel owner (`E6-P.4` Seam 3).
    ///
    /// Opens the live parallel era on first opt-in (if not already open),
    /// registers the Lisp thread in `ThreadManager` (same object/APIs as
    /// `make-thread`), records the thread ID in `GreenRuntime::parallel_admitted`,
    /// enqueues its spawn onto the keeper thread's `between_stops_hook`, and
    /// sends `KeeperMsg::Wake` so the keeper enrolls and starts the owner at
    /// the next stop boundary.
    #[cold]
    #[inline(never)]
    pub(crate) fn spawn_parallel_opt_in_thread(
        &mut self,
        args: &[Value],
    ) -> Result<Value, SpawnParallelError> {
        // Validate args before opening the keeper era or arming PendingUpgrade
        // so bad arity/type signals GNU's error without side effects.
        crate::emacs_core::threads::expect_args_range("make-thread", args, 1, 3)
            .map_err(SpawnParallelError::Lisp)?;
        let _ = crate::emacs_core::threads::ThreadName::from_lisp(
            args.get(1).copied().unwrap_or(Value::NIL),
        )
        .map_err(SpawnParallelError::Lisp)?;

        // E7-P D2: inside an interactive command loop (`CommandLoopBaseline`
        // active), mid-command `neomacs-make-parallel-thread` before the era
        // has been opened at a quiescent top-level `BetweenCommands` boundary
        // arms `PendingUpgrade` and falls back to an ordinary Baton thread
        // (hint nil, never runtime-upgraded). `--batch --eval` without an
        // interactive baseline keeps E6-P.4 G4 immediate admission.
        match self.parallel_activation_tag() {
            super::between_commands::ParallelActivationTag::Refused => {
                return Err(SpawnParallelError::Refused(
                    LiveEraOpenError::ActivationRefused,
                ));
            }
            super::between_commands::ParallelActivationTag::PendingUpgrade => {
                return Err(SpawnParallelError::Refused(
                    LiveEraOpenError::DeferredToBetweenCommands,
                ));
            }
            super::between_commands::ParallelActivationTag::Inactive
                if self.command_loop_baseline_active() =>
            {
                self.arm_pending_parallel_upgrade();
                return Err(SpawnParallelError::Refused(
                    LiveEraOpenError::DeferredToBetweenCommands,
                ));
            }
            super::between_commands::ParallelActivationTag::Inactive
            | super::between_commands::ParallelActivationTag::Active => {}
        }

        // Ensure Green runtime is started first (if configured Green).
        if self.green_seat().is_none() && !self.in_active_serial_turn() {
            if self.start_green_threads().is_err() {
                return Err(SpawnParallelError::Refused(LiveEraOpenError::NotSeatedMain));
            }
        }

        // Open the live era on first opt-in (or reuse an already-open era).
        self.ensure_live_parallel_era()
            .map_err(SpawnParallelError::Refused)?;

        // Create the ordinary Lisp thread object.
        let current = self.edit.buffers.current_buffer_id();
        let (id, function) =
            crate::emacs_core::threads::prepare_make_thread(&mut self.threads, args)
                .map_err(SpawnParallelError::Lisp)?;
        self.threads.set_thread_current_buffer(id, current);
        let handle = self.threads.thread_handle(id).unwrap_or(Value::NIL);
        let key = ThreadKey::from_id(id);
        let control = self.green_control();

        let world_handle = {
            let from_seat = self
                .green_seat()
                .and_then(|seat| seat.baton_ref())
                .and_then(TurnRight::try_clone_era_world_handle);
            match from_seat.or_else(|| control.try_clone_era_world_handle()) {
                Some(wh) => wh,
                None => {
                    self.threads.discard_unstarted_thread(id);
                    return Err(SpawnParallelError::Refused(LiveEraOpenError::OpenEraFailed));
                }
            }
        };

        let mut registered = Some(RegisteredParallelOwner::register(Arc::clone(&control), key));
        let enqueued = self.with_green_runtime(|runtime| {
            let live_ref = runtime.live_era.borrow();
            let Some(live) = live_ref.as_ref() else {
                return false;
            };
            let Some(inbox) = live.inbox.as_ref() else {
                return false;
            };
            let owner_id = live.next_owner_id.fetch_add(1, Ordering::Relaxed);
            runtime.pending_starts.borrow_mut().insert(key, function);
            runtime.parallel_admitted.borrow_mut().insert(id);
            live.pending_spawns
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push_back(PendingOwnerSpawn {
                    owner_id,
                    owner_inbox: inbox.clone(),
                    registered: registered
                        .take()
                        .expect("RegisteredParallelOwner created above"),
                    world_handle,
                });
            let _ = inbox.send(KeeperMsg::Wake);
            true
        });
        if !enqueued {
            // Dropping `registered` unregisters `Participant::Parallel`.
            drop(registered);
            self.with_green_runtime(|runtime| {
                runtime.pending_starts.borrow_mut().remove(&key);
                runtime.parallel_admitted.borrow_mut().remove(&id);
            });
            self.threads.discard_unstarted_thread(id);
            return Err(SpawnParallelError::Refused(LiveEraOpenError::Closing));
        }
        Ok(handle)
    }

    /// Whether `thread_id` was admitted to parallel execution via
    /// `neomacs-make-parallel-thread` (readable live and after `thread-join`).
    pub(crate) fn is_thread_parallel_admitted(&self, thread_id: u64) -> bool {
        self.green_runtime_ref()
            .as_ref()
            .is_some_and(|rt| rt.parallel_admitted.borrow().contains(&thread_id))
    }
}

#[cfg(test)]
#[path = "tests/live_admission_test.rs"]
mod tests;

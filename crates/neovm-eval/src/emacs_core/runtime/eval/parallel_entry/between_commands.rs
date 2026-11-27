//! E7-P.2 `BetweenCommands` activation, typed `CommandLoopBaseline` witness (D1),
//! `PendingUpgrade` deferred era-open (D2), cooperative fairness (Q-A / Q-B),
//! and Main-thread legacy serial JIT withdrawal (D4).
//!
//! # Product Contract Disclosures (E7-P D2 & D4)
//!
//! - **D2 (first mid-command thread order-dependence):** Calling
//!   `(neomacs-make-parallel-thread fn)` inside an interactive command before a
//!   parallel era has been opened falls back to an ordinary Baton thread
//!   (`neomacs-thread-parallel-p` is `nil`, never runtime-upgraded) and arms
//!   [`ParallelActivationState::PendingUpgrade`]. Main opens the era once at
//!   the next quiescent top-level `BetweenCommands` boundary (`recursive-depth == 0`
//!   and `minibuffer-depth == 0` with the typed [`CommandLoopBaseline`] witness),
//!   after which subsequent opt-in calls are admitted as parallel owners
//!   (`neomacs-thread-parallel-p` is `t`). Batch `--batch --eval` sessions
//!   without an interactive command loop keep E6-P.4 G4 immediate admission.
//! - **D4 (Main interpreter-only during open era; era lifetime):** Once an
//!   interactive session opens a parallel era at `BetweenCommands`, Main's
//!   legacy serial JIT is withdrawn (`DetachedSourceCache`, `DetachedSourceBackend`,
//!   `SourceCompilerClosed`) and Main runs bytecode through the interpreter for
//!   the lifetime of the open era until session shutdown. `close-on-zero-owners`
//!   is intentionally deferred post-E7-P/E8-P. Default, single-thread, and
//!   ordinary `make-thread`-only sessions stay [`ParallelActivationState::Inactive`]
//!   and pay zero cost (legacy serial JIT remains active, zero keeper spawn,
//!   zero `RingWaiter` atomic loads).

//! - **Q-B (Cooperative fairness scope):** At each top-level `command_loop_1`
//!   boundary while an era is `Active`, Main checks `RingWaiter` lock-free
//!   (Q-A) and, during sustained non-idle command bursts with live parallel
//!   owners, releases `WorldTurn` to `GroupTurn::Idle` for
//!   [`PARALLEL_FAIRNESS_SLICE`] (`1 ms`) after [`BATON_BURST_QUANTUM`] (`4 ms`)
//!   — even when a sibling cooperative Baton `make-thread` is runnable (HIGH #4).
//!   **Known non-burst limitation:** Q-B is checked at `command_loop_1` entry,
//!   so a single uninterrupted long-running Main command or two Baton workers
//!   yielding only to each other while Main is blocked does not cross
//!   `command_loop_1` (deferred to a future slice). User-facing runtime
//!   documentation lives in `docs/design/parallel-threads-contract.md`.

use super::super::green::ThreadKey;
use super::super::world::WorldLease;
use super::super::{Context, NativeUnwindAction, SpecBinding};
use super::live_admission::LiveEraOpenError;
use neovm_value::error::Flow;
use neovm_value::intern::{SymId, intern};
use neovm_value::value::Value;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Maximum uninterrupted non-idle Baton hold before Main yields `WorldTurn` to
/// `GroupTurn::Idle` when pure-compute parallel owners are live (E7-P §1.7 Q-B).
pub(crate) const BATON_BURST_QUANTUM: Duration = Duration::from_millis(4);

/// Cooperative fairness window during which Main leaves `GroupTurn::Idle` open
/// so `AllStopped::complete()` lets pure-compute parallel owners execute in
/// `Phase::Idle` (E7-P §1.7 Q-B).
pub(crate) const PARALLEL_FAIRNESS_SLICE: Duration = Duration::from_millis(1);

/// Minimum non-zero yield floor so coarse virtualized clocks cannot collapse
/// [`PARALLEL_FAIRNESS_SLICE`] to zero (E7-P §1.7 Q-B).
pub(crate) const MIN_FAIRNESS_SLICE_FLOOR: Duration = Duration::from_micros(100);

const _: () = {
    assert!(BATON_BURST_QUANTUM.as_nanos() > PARALLEL_FAIRNESS_SLICE.as_nanos());
    assert!(PARALLEL_FAIRNESS_SLICE.as_nanos() >= MIN_FAIRNESS_SLICE_FLOOR.as_nanos());
    assert!(MIN_FAIRNESS_SLICE_FLOOR.as_nanos() > 0);
};

/// Permanent conservative refusal recorded on [`ParallelActivationState::Refused`].
///
/// Only invariants that cannot change during a session belong here (E7-P.2 R2).
/// Transient or resource-bounded failures (`NotSeatedMain`, `KeeperStartFailed`,
/// `GcBusy`, `LegacyJitBusy`, `NativeActivation`, `OpenEraFailed`, etc.) keep
/// [`ParallelActivationState::PendingUpgrade`] and retry at the next quiescent
/// `BetweenCommands` boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActivationRefusal {
    GenerationalDisabled,
    NotGreenWorld,
    HeapUnsupported,
}

/// Active `BetweenCommands` era record published by Main at the quiescent
/// top-level command-loop boundary (E7-P §1.3).
pub(crate) struct BetweenCommandsActivation {
    pub(crate) idle_epoch: Arc<AtomicU64>,
    pub(crate) baton_burst_since: Instant,
    pub(crate) last_idle_epoch: u64,
    #[cfg(test)]
    pub(crate) burst_yields: usize,
    #[cfg(test)]
    pub(crate) fairness_releases: usize,
    #[cfg(test)]
    pub(crate) quantum_override: Option<Duration>,
    #[cfg(test)]
    pub(crate) slice_override: Option<Duration>,
}

/// Explicit state machine governing interactive `BetweenCommands` parallel era
/// activation (E7-P §1.3). No boolean flags.
pub(crate) enum ParallelActivationState {
    Inactive,
    PendingUpgrade,
    Active(BetweenCommandsActivation),
    Refused(ActivationRefusal),
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParallelActivationTag {
    Inactive = 0,
    PendingUpgrade = 1,
    Active = 2,
    Refused = 3,
}

impl ParallelActivationTag {
    #[inline(always)]
    pub(crate) fn from_u8(raw: u8) -> Self {
        match raw {
            0 => Self::Inactive,
            1 => Self::PendingUpgrade,
            2 => Self::Active,
            3 => Self::Refused,
            _ => Self::Inactive,
        }
    }
}

impl ParallelActivationState {
    #[cfg(test)]
    #[inline]
    pub(crate) fn tag(&self) -> ParallelActivationTag {
        match self {
            Self::Inactive => ParallelActivationTag::Inactive,
            Self::PendingUpgrade => ParallelActivationTag::PendingUpgrade,
            Self::Active(_) => ParallelActivationTag::Active,
            Self::Refused(_) => ParallelActivationTag::Refused,
        }
    }
}

/// Typed baseline witness captured exclusively at top-level
/// `run_exit_wrapped_command_loop` via [`Context::enter_command_loop_baseline_scope`]
/// right after binding GNU's two canonical outer `specpdl` symbols
/// (`inhibit-redisplay`, `undo-auto--undoably-changed-buffers`) (E7-P D1 /
/// HIGH #5). All fields are module-private so no external caller can forge a
/// record with arbitrary symbols.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandLoopBaselineRecord {
    outer_specpdl_count: usize,
    baseline_specpdl_len: usize,
    inhibit_redisplay: SymId,
    undo_changed_buffers: SymId,
}

/// Scoped RAII guard keeping [`CommandLoopBaselineRecord`] live on Main's
/// `World` for the duration of the outermost interactive command loop (D1).
#[must_use = "command-loop baseline witness ends when this guard drops"]
pub struct CommandLoopBaselineGuard {
    lease: WorldLease,
    previous: Option<CommandLoopBaselineRecord>,
    _not_send: PhantomData<Rc<()>>,
}

static_assertions::assert_not_impl_any!(CommandLoopBaselineGuard: Send, Sync, Clone, Copy);

impl Drop for CommandLoopBaselineGuard {
    fn drop(&mut self) {
        self.lease.set_command_loop_baseline(self.previous);
    }
}

#[inline]
fn specpdl_binding_sym(binding: &SpecBinding) -> Option<SymId> {
    match binding {
        SpecBinding::Let { sym_id, .. }
        | SpecBinding::LetLocal { sym_id, .. }
        | SpecBinding::LetDefault { sym_id, .. } => Some(*sym_id),
        SpecBinding::NativeUnwind {
            action: NativeUnwindAction::OwnerBinding(owner_binding),
        } => Some(owner_binding.symbol()),
        _ => None,
    }
}

fn verify_command_loop_baseline_bindings(
    specpdl: &[SpecBinding],
    record: CommandLoopBaselineRecord,
) -> bool {
    if specpdl.len() != record.baseline_specpdl_len {
        return false;
    }
    let first = specpdl
        .get(record.outer_specpdl_count)
        .and_then(specpdl_binding_sym);
    let second = specpdl
        .get(record.outer_specpdl_count + 1)
        .and_then(specpdl_binding_sym);
    let ok = first == Some(record.inhibit_redisplay) && second == Some(record.undo_changed_buffers);
    debug_assert!(
        ok,
        "E7-P D1: command-loop baseline specpdl bindings drifted: expected ({:?}, {:?}), found ({:?}, {:?})",
        record.inhibit_redisplay, record.undo_changed_buffers, first, second
    );
    ok
}

impl Context {
    /// Whether Main currently has an active outermost interactive
    /// [`CommandLoopBaselineRecord`] witness (E7-P D1 / D2).
    #[inline]
    pub(crate) fn command_loop_baseline_active(&self) -> bool {
        self.world_lease().command_loop_baseline().is_some()
    }

    /// Whether this `Context` is Main (either unseated pre-Green Main or
    /// seated with `ThreadKey::Main`). Non-Main Baton worker threads never
    /// establish `CommandLoopBaseline`, open an era at `BetweenCommands`, or
    /// mutate Main's fairness state (HIGH #3).
    #[inline]
    fn is_main_for_between_commands(&mut self) -> bool {
        self.green_key().is_none_or(|key| key == ThreadKey::Main)
    }

    /// Whether Main is quiescent enough for top-level interactive
    /// `BetweenCommands` activation (no active kbd-macro playback, no active
    /// Lisp/bytecode frames on Main's stack; HIGH #3).
    #[inline]
    fn is_quiescent_top_level_command_loop(&mut self) -> bool {
        self.is_main_for_between_commands()
            && !self.command_loop_noninteractive()
            && self.command_loop().recursive_depth == 1
            && self.recursive_command_loop_depth() == 0
            && self.minibuffers().depth() == 0
            && self
                .command_loop()
                .keyboard
                .kboard
                .executing_kbd_macro
                .is_none()
            && self.depth == 0
            && self.bc_buf.is_empty()
    }

    /// Current E7-P.2 activation state tag (lock-free `AtomicU8` read;
    /// MEDIUM #7).
    #[inline(always)]
    pub(crate) fn parallel_activation_tag(&self) -> ParallelActivationTag {
        ParallelActivationTag::from_u8(self.world_lease().parallel_activation_tag_raw())
    }

    /// Arm [`ParallelActivationState::PendingUpgrade`] when mid-command
    /// `neomacs-make-parallel-thread` runs inside an interactive command loop
    /// before an era has been opened (E7-P D2).
    pub(crate) fn arm_pending_parallel_upgrade(&self) {
        let mut state = self.world_lease().lock_parallel_activation();
        if matches!(*state, ParallelActivationState::Inactive) {
            *state = ParallelActivationState::PendingUpgrade;
            self.world_lease()
                .set_parallel_activation_tag_raw(ParallelActivationTag::PendingUpgrade as u8);
            self.world_lease().set_parallel_activation_fast(true);
        }
    }

    /// Reset E7-P.2 activation state on terminal `teardown_live_parallel_era`.
    ///
    /// Per E7-P D4 / P2 session-lifetime withdrawal contract, `legacy_jit_withdrawn`
    /// is monotonic (`false -> true`) for the lifetime of this `World` and is
    /// deliberately NOT cleared here (because `DetachedSourceCache::drop` retains
    /// payloads rather than restoring legacy JIT; sessions that never opened an
    /// era keep `legacy_jit_withdrawn() == false`).
    pub(crate) fn reset_parallel_activation_on_teardown(&self) {
        *self.world_lease().lock_parallel_activation() = ParallelActivationState::Inactive;
        self.world_lease()
            .set_parallel_activation_tag_raw(ParallelActivationTag::Inactive as u8);
        self.world_lease().set_parallel_activation_fast(false);
    }

    /// Atomically bind GNU's two outer `recursive_edit_1` baseline symbols
    /// (`inhibit-redisplay`, `undo-auto--undoably-changed-buffers`) and, when
    /// entering the outermost interactive command loop on Main, mint the
    /// unforgeable [`CommandLoopBaselineGuard`] witness (E7-P D1 / HIGH #5).
    ///
    /// Returns `Ok(None)` when `increment_depth == false` or outside the
    /// quiescent outermost interactive command loop on Main (`noninteractive`,
    /// nested `recursive-edit`, active minibuffer, `execute-kbd-macro`, active
    /// Lisp/VM frames, or non-Main Baton worker thread).
    #[doc(hidden)]
    pub fn enter_command_loop_baseline_scope(
        &mut self,
        outer_specpdl_count: usize,
        increment_depth: bool,
    ) -> Result<Option<CommandLoopBaselineGuard>, Flow> {
        let inhibit_redisplay = intern("inhibit-redisplay");
        let undo_changed_buffers = intern("undo-auto--undoably-changed-buffers");
        self.try_specbind_or_unwind_to(outer_specpdl_count, inhibit_redisplay, Value::NIL)?;
        self.try_specbind_or_unwind_to(outer_specpdl_count, undo_changed_buffers, Value::NIL)?;

        if !increment_depth || !self.is_quiescent_top_level_command_loop() {
            return Ok(None);
        }
        let Some(baseline_specpdl_len) = outer_specpdl_count.checked_add(2) else {
            return Ok(None);
        };
        let record = CommandLoopBaselineRecord {
            outer_specpdl_count,
            baseline_specpdl_len,
            inhibit_redisplay,
            undo_changed_buffers,
        };
        if !verify_command_loop_baseline_bindings(self.specpdl(), record) {
            return Ok(None);
        }
        let lease = self.world_lease().duplicate_for_view();
        let previous = lease.command_loop_baseline();
        lease.set_command_loop_baseline(Some(record));
        Ok(Some(CommandLoopBaselineGuard {
            lease,
            previous,
            _not_send: PhantomData,
        }))
    }

    /// Top-of-loop `BetweenCommands` activation & fairness boundary called at
    /// `command_loop_1` before `begin_command()` (E7-P §1.2–§1.7).
    ///
    /// Default / no-opt-in / `make-thread`-only fast path: one predicted-not-taken
    /// non-atomic byte load (`parallel_activation_fast == false`), zero atomics,
    /// zero `RingWaiter` loads, zero clock reads, zero keeper activity.
    #[doc(hidden)]
    #[inline]
    pub fn parallel_between_commands_boundary(&mut self) -> Result<(), Flow> {
        if !self.world_lease().parallel_activation_fast() {
            return Ok(());
        }
        self.parallel_between_commands_slow()
    }

    #[cold]
    #[inline(never)]
    fn parallel_between_commands_slow(&mut self) -> Result<(), Flow> {
        let tag = self.parallel_activation_tag();
        match tag {
            ParallelActivationTag::Inactive | ParallelActivationTag::Refused => {
                self.world_lease().set_parallel_activation_fast(false);
                Ok(())
            }
            ParallelActivationTag::PendingUpgrade => {
                self.try_activate_pending_upgrade_at_between_commands();
                Ok(())
            }
            ParallelActivationTag::Active => self.service_active_between_commands_fairness(),
        }
    }

    fn try_activate_pending_upgrade_at_between_commands(&mut self) {
        // D1 / HIGH #3: require Main thread, typed `CommandLoopBaseline`
        // witness, quiescent top-level depth (`recursive_command_loop_depth == 0
        // && minibuffers().depth() == 0`), no active kbd-macro playback, zero
        // active Lisp/VM frames (`depth == 0 && bc_buf.is_empty()`), and exact
        // baseline `specpdl` bindings (`inhibit-redisplay`,
        // `undo-auto--undoably-changed-buffers`).
        if !self.is_quiescent_top_level_command_loop() {
            return;
        }
        let Some(record) = self.world_lease().command_loop_baseline() else {
            return;
        };
        if !verify_command_loop_baseline_bindings(self.specpdl(), record) {
            return;
        }

        // Preflight permanent eligibility before attempting era open.
        if !self.tagged_heap.tagged_heap_generational_enabled() {
            self.record_activation_refusal(ActivationRefusal::GenerationalDisabled);
            return;
        }
        if !crate::emacs_core::eval::green_world(self) {
            self.record_activation_refusal(ActivationRefusal::NotGreenWorld);
            return;
        }

        // Start Green runtime if Main has not started it yet. Transient startup
        // or resource failures keep `PendingUpgrade` and retry on the next
        // quiescent `BetweenCommands` boundary (E7-P.2 R2).
        if self.green_seat().is_none() && self.start_green_threads().is_err() {
            tracing::warn!(
                "E7-P.2 BetweenCommands Green runtime startup failed; keeping PendingUpgrade for retry"
            );
            return;
        }

        match self.ensure_live_parallel_era() {
            Ok(()) => {
                let control = self.green_control();
                let Some((_waiter, idle_epoch, _parallel_count)) =
                    control.era_between_commands_handles()
                else {
                    tracing::warn!(
                        "E7-P.2 BetweenCommands era handles unavailable after open; keeping PendingUpgrade for retry"
                    );
                    return;
                };
                let last_idle_epoch = idle_epoch.load(Ordering::Relaxed);
                let activation = BetweenCommandsActivation {
                    idle_epoch,
                    baton_burst_since: Instant::now(),
                    last_idle_epoch,
                    #[cfg(test)]
                    burst_yields: 0,
                    #[cfg(test)]
                    fairness_releases: 0,
                    #[cfg(test)]
                    quantum_override: None,
                    #[cfg(test)]
                    slice_override: None,
                };
                *self.world_lease().lock_parallel_activation() =
                    ParallelActivationState::Active(activation);
                self.world_lease()
                    .set_parallel_activation_tag_raw(ParallelActivationTag::Active as u8);
                self.world_lease().set_parallel_activation_fast(true);
            }
            // Permanent conservative refusal: conditions that cannot change
            // during this session. Record once and stop re-probing (E7-P.2 R2).
            Err(LiveEraOpenError::GenerationalRequired) => {
                self.record_activation_refusal(ActivationRefusal::GenerationalDisabled);
            }
            Err(LiveEraOpenError::HeapUnsupported) => {
                self.record_activation_refusal(ActivationRefusal::HeapUnsupported);
            }
            // Transient or resource-bounded conditions stay `PendingUpgrade`
            // and retry on the next quiescent `BetweenCommands` boundary
            // (E7-P.2 R2). Exhaustive match forces explicit classification of
            // any new `LiveEraOpenError` variant.
            Err(
                err @ (LiveEraOpenError::NotSeatedMain
                | LiveEraOpenError::NotSolo
                | LiveEraOpenError::GcBusy
                | LiveEraOpenError::Closing
                | LiveEraOpenError::SeatPark
                | LiveEraOpenError::DrainFailed
                | LiveEraOpenError::TransferFailed
                | LiveEraOpenError::KeeperStartFailed
                | LiveEraOpenError::OpenEraFailed
                | LiveEraOpenError::InitialTurnFailed
                | LiveEraOpenError::NativeActivation
                | LiveEraOpenError::LegacyJitBusy
                | LiveEraOpenError::LegacyJitWithdrawalFailed
                | LiveEraOpenError::DeferredToBetweenCommands
                | LiveEraOpenError::ActivationRefused),
            ) => {
                tracing::debug!(
                    ?err,
                    "E7-P.2 BetweenCommands era open transiently deferred; keeping PendingUpgrade"
                );
            }
            #[cfg(test)]
            Err(LiveEraOpenError::InjectedFault) => {}
        }
    }

    fn record_activation_refusal(&self, refusal: ActivationRefusal) {
        tracing::warn!(
            ?refusal,
            "E7-P.2 BetweenCommands activation permanently refused; falling back to Baton"
        );
        *self.world_lease().lock_parallel_activation() = ParallelActivationState::Refused(refusal);
        self.world_lease()
            .set_parallel_activation_tag_raw(ParallelActivationTag::Refused as u8);
        self.world_lease().set_parallel_activation_fast(false);
    }

    fn service_active_between_commands_fairness(&mut self) -> Result<(), Flow> {
        // HIGH #3: Only Main holding an Era turn services BetweenCommands
        // fairness and updates Main's burst timer. Non-Main Baton workers
        // entering `command_loop_1` (e.g. via kbd macro) are a no-op here.
        let Some(seat) = self.green_seat() else {
            return Ok(());
        };
        if seat.key != ThreadKey::Main || !seat.baton_ref().is_some_and(|b| b.is_era()) {
            return Ok(());
        }
        let control = seat.control();

        // MEDIUM #7 / Q-A: lock-free fast path — 1 Acquire load on RingWaiter
        // + 1 Acquire load on `parallel_count`, zero mutex locks and zero
        // clock reads when no parallel owner is queued or live.
        let waiter_raised = control.era_ring_waiter_raised_fast();
        if !waiter_raised && control.live_parallel_participant_count_fast() == 0 {
            return Ok(());
        }

        enum FairnessAction {
            None,
            BurstYield,
            FairnessSlice(Duration),
        }
        let action = if waiter_raised {
            FairnessAction::BurstYield
        } else {
            let mut state = self.world_lease().lock_parallel_activation();
            let ParallelActivationState::Active(activation) = &mut *state else {
                return Ok(());
            };
            // Q-B: reset `baton_burst_since` whenever Main parked to
            // `GroupTurn::Idle` during the previous command's input wait.
            let cur_idle_epoch = activation.idle_epoch.load(Ordering::Relaxed);
            if cur_idle_epoch != activation.last_idle_epoch {
                activation.last_idle_epoch = cur_idle_epoch;
                activation.baton_burst_since = Instant::now();
                FairnessAction::None
            } else {
                #[cfg(test)]
                let quantum = activation.quantum_override.unwrap_or(BATON_BURST_QUANTUM);
                #[cfg(not(test))]
                let quantum = BATON_BURST_QUANTUM;
                if activation.baton_burst_since.elapsed() >= quantum {
                    #[cfg(test)]
                    let slice = activation.slice_override.unwrap_or(PARALLEL_FAIRNESS_SLICE);
                    #[cfg(not(test))]
                    let slice = PARALLEL_FAIRNESS_SLICE;
                    FairnessAction::FairnessSlice(slice.max(MIN_FAIRNESS_SLICE_FLOOR))
                } else {
                    FairnessAction::None
                }
            }
        };

        match action {
            FairnessAction::None => Ok(()),
            FairnessAction::BurstYield => {
                // Release `WorldTurn` once to the keeper (re-enqueuing BatonGroup
                // at ring tail behind all committed parallel serial turns).
                self.green_wait(super::super::green::Wait::Runnable)?;
                let mut state = self.world_lease().lock_parallel_activation();
                if let ParallelActivationState::Active(activation) = &mut *state {
                    activation.baton_burst_since = Instant::now();
                    activation.last_idle_epoch = activation.idle_epoch.load(Ordering::Relaxed);
                    #[cfg(test)]
                    {
                        activation.burst_yields += 1;
                    }
                }
                Ok(())
            }
            FairnessAction::FairnessSlice(slice) => {
                // Release `WorldTurn` to `GroupTurn::Idle` for `slice` so
                // `AllStopped::complete()` lets pure-compute parallel owners
                // execute in `Phase::Idle` without starving.
                self.green_fairness_slice(slice)?;
                let mut state = self.world_lease().lock_parallel_activation();
                if let ParallelActivationState::Active(activation) = &mut *state {
                    activation.baton_burst_since = Instant::now();
                    activation.last_idle_epoch = activation.idle_epoch.load(Ordering::Relaxed);
                    #[cfg(test)]
                    {
                        activation.fairness_releases += 1;
                    }
                }
                Ok(())
            }
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn arm_pending_upgrade_for_test(&self) {
        self.arm_pending_parallel_upgrade();
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn is_parallel_activation_active_for_test(&self) -> bool {
        self.parallel_activation_tag() == ParallelActivationTag::Active
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn is_command_loop_baseline_active_for_test(&self) -> bool {
        self.command_loop_baseline_active()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn legacy_jit_withdrawn_for_test(&self) -> bool {
        self.legacy_jit_withdrawn()
    }

    #[cfg(test)]
    pub(crate) fn parallel_activation_tag_for_test(&self) -> ParallelActivationTag {
        self.parallel_activation_tag()
    }

    #[cfg(test)]
    pub(crate) fn parallel_activation_fast_for_test(&self) -> bool {
        self.world_lease().parallel_activation_fast()
    }

    #[cfg(test)]
    pub(crate) fn parallel_activation_refusal_for_test(&self) -> Option<ActivationRefusal> {
        let state = self.world_lease().lock_parallel_activation();
        match &*state {
            ParallelActivationState::Refused(refusal) => Some(*refusal),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_fairness_overrides_for_test(
        &self,
        quantum: Option<Duration>,
        slice: Option<Duration>,
    ) {
        let mut state = self.world_lease().lock_parallel_activation();
        if let ParallelActivationState::Active(activation) = &mut *state {
            activation.quantum_override = quantum;
            activation.slice_override = slice;
            activation.baton_burst_since = Instant::now();
            activation.last_idle_epoch = activation.idle_epoch.load(Ordering::Relaxed);
        }
    }

    #[cfg(test)]
    pub(crate) fn between_commands_counters_for_test(&self) -> Option<(usize, usize)> {
        let state = self.world_lease().lock_parallel_activation();
        match &*state {
            ParallelActivationState::Active(activation) => {
                Some((activation.burst_yields, activation.fairness_releases))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "tests/e7p2_between_commands_test.rs"]
mod tests;

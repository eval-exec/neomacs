//! L6.7b b3 + b4.2 (pentry seat + parked roots): activate a seated Green World
//! host under a [`WorldTurn`], switch Baton-group threads inside one interval,
//! publish parked private roots as a `RootBatchLease`, and stow World carriers
//! before returning the turn.
//!
//! Unlike unseated parallel owners (which use [`super::serial_turn::SerialHost`]
//! and are refused when seated via `HostRefusal::Seated`), Baton-group threads
//! (main and `make-thread` workers) hold a Green seat and stow/unstow through
//! `GreenRuntime`'s carried TLS and warm `OwnerCarried` slots (`green/switch.rs`).
//!
//! Contracts:
//! - [`ParkedWorldSeat::park_seated`] accepts only a seated Green `Context`
//!   whose own heap is currently installed on this OS thread and that holds the
//!   World's carried state here; it closes open bump-allocation regions, parks
//!   host bindings + JIT pins, publishes the seated `Context`'s private roots
//!   into `GreenRuntime::parked` as a `RootBatchLease` (P1), stows the World's
//!   carriers to `CarriedHolder::Stowed`, and parks carrier TLS.
//! - [`ParkedWorldSeat::enter`] and [`ParkedWorldSeat::switch_in`] fail closed
//!   on every invariant before mutating TLS or World state: seated Green host,
//!   matching parked `OwnerSlab` identity (`WorldSeatRefusal::WrongContext`,
//!   Low 4), structural heap identity matching [`WorldTurnIdentity::heap`]
//!   (Condition A-3), no foreign heap installed on this OS thread, and
//!   `CarriedHolder::Stowed` (Condition A-1). Any refusal hands back
//!   `(self, turn, lend, refusal)` unchanged so the armed [`WorldLend`] is never
//!   leaked silently. On success, the Context is restored FIRST (`resume_owner`
//!   and any `ParkedJit` resume), and only then is its parked `RootBatchLease`
//!   retired under the Q2 non-marking phase guard without `drain_to_quiescent`
//!   (failing closed to retain on busy/held readers).
//! - [`ActiveWorldSeat::switch_out`] publishes parked roots, stows World
//!   carriers to `CarriedHolder::Stowed`, and parks carrier TLS WITHOUT
//!   consuming [`WorldTurn`], handing back `(ParkedWorldSeat, WorldTurn, WorldLend)`
//!   so another Baton thread in the group can [`ParkedWorldSeat::switch_in`]
//!   during the same keeper stop interval (b4.2).
//! - [`ActiveWorldSeat::exit`] runs [`ActiveWorldSeat::switch_out`] strictly
//!   BEFORE converting `WorldTurn` into [`WorldReturned`] via
//!   [`WorldReturned::from_stowed_seat`] (Nit 6 — sole production mint of
//!   `WorldReturned`), discharging A-1 liveness for subsequent parallel-owner
//!   serial turns.
//! - Both [`ParkedWorldSeat`] and [`ActiveWorldSeat`] carry one retained
//!   `FacadeEpochRetention` (Q3) across open-era cycles; dropping the seat drops
//!   that retention before `World::finish_green_registry` / final `TaggedHeap::drop`.
//! - A panic unwinding between the multi-step TLS/stow transitions aborts the
//!   process (matching `serial_detach.rs` and `green/switch.rs`); a panic
//!   inside the active Lisp interval (or between `switch_out` and `switch_in`)
//!   drops the armed [`WorldLend`], which automatically sends
//!   `KeeperMsg::GroupAbandon { cause: Unwound }` (K7).
#![deny(unsafe_code)]

use super::super::Context;
use super::super::green::WorldSeatParkedRoots;
use super::super::green::carrier_tls::park_carrier_tls;
use super::keeper_driver::WorldLend;
use super::world_turn::{WorldReturned, WorldTurn, WorldTurnIdentity};
use neovm_value::tagged::gc::FacadeEpochRetention;
use std::marker::PhantomData;

/// Private proof constructed solely by `world_seat` after verifying that open
/// bump-allocation regions are closed, World carriers are `CarriedHolder::Stowed`,
/// and carrier TLS is parked (Nit 6).
#[derive(Debug)]
#[must_use = "consume StowedForReturn in WorldReturned::from_stowed_seat or drop on seat entry"]
pub(crate) struct StowedForReturn(());

/// Mint [`StowedForReturn`] after a seated Green switch point (`green_park_impl`)
/// has closed open alloc regions, published parked roots, stowed World carriers
/// to `CarriedHolder::Stowed`, and parked carrier TLS.
#[cold]
#[inline(never)]
pub(crate) fn prove_stowed_for_era_switch(ctx: &mut Context) -> StowedForReturn {
    ctx.tagged_heap.close_alloc_regions();
    #[cfg(any(test, feature = "test-support"))]
    assert!(
        !ctx.tagged_heap.alloc_regions_open(),
        "Nit 6: open alloc regions must be closed before returning an era WorldTurn"
    );
    assert!(
        ctx.has_published_parked_roots_for_seat(),
        "Nit 6 / P1: seated Context must publish parked roots in GreenRuntime::parked before returning an era WorldTurn"
    );
    assert!(
        ctx.serial_world_is_stowed(),
        "Nit 6 / A-1: World carriers must be Stowed before returning an era WorldTurn"
    );
    assert!(
        neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
        "Nit 6: carrier TLS must be parked before returning an era WorldTurn"
    );
    StowedForReturn(())
}

/// Mint [`StowedForReturn`] when an exiting Green worker has retired its
/// Context, closed alloc regions, stowed World carriers, and parked carrier TLS.
#[cold]
#[inline(never)]
pub(crate) fn prove_retired_worker_stowed_for_era(
    baton: &super::super::world::TurnRight,
) -> StowedForReturn {
    assert!(
        baton.is_era(),
        "Nit 6: retired worker stow proof requires TurnRight::Era"
    );
    assert!(
        baton.held_here(),
        "Nit 6: retired worker stow proof must be minted on the OS thread holding TurnRight::Era"
    );
    assert!(
        baton.serial_world_is_stowed(),
        "Nit 6 / A-1: World carriers must be Stowed on retired worker exit"
    );
    assert!(
        neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
        "Nit 6: carrier TLS must be parked on retired worker exit"
    );
    StowedForReturn(())
}

/// Mint [`StowedForReturn`] for a freshly keeper-lent [`WorldTurn`] before any
/// Baton thread enters it (World carriers are already `CarriedHolder::Stowed`
/// and carrier TLS is parked on this thread).
#[cold]
#[inline(never)]
pub(crate) fn prove_lent_world_stowed(
    turn: &WorldTurn,
    world: &super::super::world::EraWorldHandle,
) -> StowedForReturn {
    assert_eq!(
        turn.identity().heap,
        world.heap_identity(),
        "A-3: freshly lent WorldTurn heap must match EraWorldHandle"
    );
    assert!(
        world.serial_world_is_stowed_under_turn(turn),
        "A-1: World carriers must be Stowed when keeper lends WorldTurn"
    );
    assert!(
        neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start(),
        "Nit 6: carrier TLS must be parked when receiving a keeper-lent WorldTurn"
    );
    StowedForReturn(())
}

/// Why a seated Green host refused to park or enter a [`WorldTurn`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorldSeatRefusal {
    /// Context holds no Green seat (`SerialHost` serves unseated parallel owners).
    Unseated,
    /// Context has no `GreenRuntime` (e.g. a Sim World).
    NotGreenWorld,
    /// `park_seated`: this Context's heap is not the one installed on this OS thread.
    NotInstalled,
    /// `park_seated`: this Context on this OS thread does not hold the World's carried state.
    WorldNotHeldHere,
    /// `park_seated`: failed to retain epoch exclusion or publish the parked root batch.
    RootPublication,
    /// `enter`/`switch_in` (Low 4): the `&mut Context` does not match the parked `OwnerSlab`.
    WrongContext,
    /// `enter`/`switch_in` (A-3): `WorldTurnIdentity::heap` does not match this Context's own heap.
    WrongHeap,
    /// `enter`/`switch_in`: another Context's heap is already installed on this OS thread.
    HeapInstalled,
    /// `enter`/`switch_in` (A-1): the Green World's carriers are held elsewhere rather than stowed.
    WorldHeldElsewhere,
}

/// Proof that a seated Green `Context` is currently parked off this OS thread
/// with its host bindings unbound, private roots published in
/// `GreenRuntime::parked`, and carrier TLS parked.
#[must_use = "enter the parked World seat under a WorldTurn"]
#[derive(Debug)]
pub(crate) struct ParkedWorldSeat {
    slab: usize,
    roots: WorldSeatParkedRoots,
    retention: FacadeEpochRetention,
    _thread: PhantomData<*const ()>,
}

/// Live Baton-group World interval holding a [`WorldTurn`] and its `Send` K7
/// [`WorldLend`] guard.
#[must_use = "switch_out or exit the active World seat to stow carriers"]
#[derive(Debug)]
pub(crate) struct ActiveWorldSeat {
    slab: usize,
    turn: WorldTurn,
    lend: WorldLend,
    retention: FacadeEpochRetention,
    _thread: PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(ParkedWorldSeat: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ActiveWorldSeat: Send, Sync, Clone, Copy);

/// Aborts the process when a panic unwinds midway through a park/enter/exit
/// TLS transition (matching `serial_detach.rs` and `green/switch.rs`).
///
/// Uses an explicit `armed` flag rather than `std::thread::panicking()` alone
/// so callers invoked during an outer unwind (such as `LiveEraOpenGuard::drop`
/// rolling back `resume_with_turn_right` under `E6-P4-I1`) abort ONLY on a
/// nested panic inside the transition, never on a clean completion.
struct AbortOnPanic {
    step: &'static str,
    armed: bool,
}

impl AbortOnPanic {
    fn new(step: &'static str) -> Self {
        Self { step, armed: true }
    }

    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        if self.armed {
            tracing::error!(
                step = self.step,
                "a seated Green World host panicked between carrier-TLS transition steps; aborting"
            );
            std::process::abort();
        }
    }
}

impl ParkedWorldSeat {
    /// Park a live seated Green `Context` (holding its `Baton` inline on its
    /// `GreenSeat`) off this OS thread so a parallel era can lend World turns
    /// through the keeper ring.
    ///
    /// # Safety
    ///
    /// On `Ok(parked)`, `ctx`'s TLS heap slot is cleared via
    /// [`park_carrier_tls`] and its owner/World state is stowed while `ctx`
    /// remains live:
    ///
    /// 1. The caller must not re-enter, allocate, collect, or evaluate on
    ///    `ctx` until `parked.enter(...)` / `parked.switch_in(...)` reinstalls
    ///    it (or `ctx` is retired/dropped).
    /// 2. The caller must not reuse this OS thread for another `Context`
    ///    without first completing [`reset_carrier_tls`] (e.g. via `enter` /
    ///    `switch_in`).
    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    pub(crate) unsafe fn park_seated(ctx: &mut Context) -> Result<Self, WorldSeatRefusal> {
        // SAFETY: caller upholds `park_seated`'s dormancy and deferred-reset
        // contract above.
        unsafe { Self::park_seated_inner(ctx, None) }
    }

    /// Test-only helper: park a live seated Green `Context` during an
    /// already-open era test fixture where the keeper retains the World's
    /// `Baton`, using an explicit retained facade epoch token (Q3). Callers
    /// must serialize against the era's turns; production callers must hold a
    /// `Baton` (`park_seated`) or `WorldTurn` (`ActiveWorldSeat::switch_out`).
    ///
    /// # Safety
    ///
    /// Same dormancy and deferred-reset contract as [`Self::park_seated`].
    #[cfg(test)]
    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    pub(crate) unsafe fn park_seated_with_retention(
        ctx: &mut Context,
        retention: &FacadeEpochRetention,
    ) -> Result<Self, WorldSeatRefusal> {
        // SAFETY: caller upholds `park_seated`'s dormancy and deferred-reset
        // contract above.
        unsafe { Self::park_seated_inner(ctx, Some(retention)) }
    }

    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    unsafe fn park_seated_inner(
        ctx: &mut Context,
        retention: Option<&FacadeEpochRetention>,
    ) -> Result<Self, WorldSeatRefusal> {
        let Some(slab_ref) = ctx.owner.slab() else {
            return Err(WorldSeatRefusal::Unseated);
        };
        if slab_ref.green_seat().is_none() {
            return Err(WorldSeatRefusal::Unseated);
        }
        let slab = slab_ref as *const _ as usize;
        if ctx.green_runtime_ref().is_none() {
            return Err(WorldSeatRefusal::NotGreenWorld);
        }
        if !neovm_value::tagged::gc::tagged_heap_is_current(&ctx.tagged_heap) {
            return Err(WorldSeatRefusal::NotInstalled);
        }
        if !ctx.holds_serial_world_here() {
            return Err(WorldSeatRefusal::WorldNotHeldHere);
        }
        // Guard the entire park + root-publication + stow + TLS sequence
        // (matching `switch_out`): an `Err` return does not panic, while an
        // unwind midway aborts before bindings and carriers can split.
        let abort = AbortOnPanic::new("world-seat-park");
        let (roots, retention) = match ctx.park_world_seat_with_roots(retention) {
            Ok(pair) => pair,
            Err(_) => {
                abort.disarm();
                return Err(WorldSeatRefusal::RootPublication);
            }
        };
        let stowed = ctx.stow_serial_world_state();
        assert!(
            stowed,
            "verified World carrier holder must stow into GreenRuntime"
        );
        // SAFETY: `ctx` parked its seat/roots and stowed carried World state
        // under `AbortOnPanic`, no TLS heap loan is live, and the caller of
        // `park_seated` / `park_seated_with_retention` upholds the dormancy and
        // deferred `reset_carrier_tls()` obligations.
        unsafe {
            park_carrier_tls();
        }
        abort.disarm();
        Ok(Self {
            slab,
            roots,
            retention,
            _thread: PhantomData,
        })
    }

    /// Activate the seated Green `Context` under `turn` and its K7 `lend`
    /// guard. Fails closed on every invariant before mutating TLS or World
    /// state.
    #[cold]
    #[inline(never)]
    pub(crate) fn enter(
        self,
        ctx: &mut Context,
        turn: WorldTurn,
        lend: WorldLend,
    ) -> Result<ActiveWorldSeat, (Self, WorldTurn, WorldLend, WorldSeatRefusal)> {
        let Some(slab_ref) = ctx.owner.slab() else {
            return Err((self, turn, lend, WorldSeatRefusal::Unseated));
        };
        if slab_ref.green_seat().is_none() {
            return Err((self, turn, lend, WorldSeatRefusal::Unseated));
        }
        if ctx.green_runtime_ref().is_none() {
            return Err((self, turn, lend, WorldSeatRefusal::NotGreenWorld));
        }
        if (slab_ref as *const _ as usize) != self.slab {
            return Err((self, turn, lend, WorldSeatRefusal::WrongContext));
        }
        if turn.identity().heap != ctx.tagged_heap.heap_identity() {
            return Err((self, turn, lend, WorldSeatRefusal::WrongHeap));
        }
        if !neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start() {
            return Err((self, turn, lend, WorldSeatRefusal::HeapInstalled));
        }
        if !ctx.serial_world_is_stowed() {
            return Err((self, turn, lend, WorldSeatRefusal::WorldHeldElsewhere));
        }
        let abort = AbortOnPanic::new("world-seat-enter");
        let restored = ctx.reset_carrier_tls_for_resume();
        debug_assert!(restored, "ParkedWorldSeat::enter requires a GreenRuntime");
        ctx.unstow_serial_world_state();
        ctx.resume_world_seat_with_roots(self.roots);
        abort.disarm();
        Ok(ActiveWorldSeat {
            slab: self.slab,
            turn,
            lend,
            retention: self.retention,
            _thread: PhantomData,
        })
    }

    /// Intra-interval Baton-group handoff (b4.2): resume this parked seated
    /// Green `Context` under an already-live `(WorldTurn, WorldLend)` handed
    /// off from another Baton thread's [`ActiveWorldSeat::switch_out`].
    #[cold]
    #[inline(never)]
    pub(crate) fn switch_in(
        self,
        ctx: &mut Context,
        turn: WorldTurn,
        lend: WorldLend,
    ) -> Result<ActiveWorldSeat, (Self, WorldTurn, WorldLend, WorldSeatRefusal)> {
        self.enter(ctx, turn, lend)
    }

    /// Resume a parked seated Green `Context` in place under a newly installed
    /// `TurnRight` (`TurnRight::Era` after live-Green era-open, or
    /// `TurnRight::Solo` on era-open rollback / era teardown, E6-P.4).
    #[cold]
    #[inline(never)]
    pub(crate) fn resume_with_turn_right(
        self,
        ctx: &mut Context,
        mut right: super::super::world::TurnRight,
    ) {
        let abort = AbortOnPanic::new("world-seat-resume-turn-right");
        right.clear_stowed_for_return();
        right.enter();
        let restored = ctx.reset_carrier_tls_for_resume();
        debug_assert!(
            restored,
            "ParkedWorldSeat::resume_with_turn_right requires a GreenRuntime"
        );
        ctx.unstow_serial_world_state();
        ctx.resume_world_seat_with_roots(self.roots);
        let seat_key = {
            let seat = ctx.green_seat().expect("resumed Green seat is seated");
            let key = seat.key;
            seat.install_baton(right);
            key
        };
        ctx.threads.enter_thread(seat_key.id());
        drop(self.retention);
        abort.disarm();
    }

    /// Test-only teardown helper: restore `ctx` briefly to retire its parked
    /// `RootBatchLease`, clear any cached `PreparedRootBatch`, stow World
    /// carriers back to `Stowed`, and drop the seat's `FacadeEpochRetention`
    /// before `World::finish_green_registry` / final `TaggedHeap::drop` (Q3).
    ///
    /// # Safety
    ///
    /// Leaves `ctx` parked with carrier TLS cleared via [`park_carrier_tls`];
    /// caller must drop `ctx` without re-entering it and must not reuse this OS
    /// thread for another `Context` without running [`reset_carrier_tls`].
    #[cfg(test)]
    #[allow(unsafe_code)]
    pub(crate) unsafe fn retire_parked_for_teardown_for_test(self, ctx: &mut Context) {
        let abort = AbortOnPanic::new("world-seat-teardown");
        let restored = ctx.reset_carrier_tls_for_resume();
        debug_assert!(
            restored,
            "ParkedWorldSeat::retire_parked_for_teardown_for_test requires a GreenRuntime"
        );
        ctx.unstow_serial_world_state();
        ctx.resume_world_seat_with_roots(self.roots);
        ctx.clear_cached_parked_root_batch_for_test();
        let _parked = ctx.park_owner();
        let stowed = ctx.stow_serial_world_state();
        assert!(stowed, "teardown must stow World carriers back to Stowed");
        // SAFETY: test teardown parked `ctx` and stowed World carriers under
        // `AbortOnPanic`, no TLS heap loan is live, and caller upholds the
        // teardown dormancy/deferred-reset contract above.
        unsafe {
            park_carrier_tls();
        }
        drop(self.retention);
        abort.disarm();
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "L6.7b b4.3 (Green switch points) is the caller")
)]
impl ActiveWorldSeat {
    pub(crate) fn identity(&self) -> WorldTurnIdentity {
        self.turn.identity()
    }

    /// Adopt a currently running seated Green `Context` that holds an active
    /// `TurnRight::Era` (decomposed into `(turn, lend, retention)`) into an
    /// [`ActiveWorldSeat`] so `b4.4` (`gc_collect_from_current_roots_impl_flow`)
    /// can end the interval via [`Self::exit`] and re-enter via
    /// [`ParkedWorldSeat::enter`] while preserving Nit 6.
    ///
    /// Note for `b4.4` (lead review LOW-2): `ActiveWorldSeat` holds the armed
    /// `(WorldTurn, WorldLend)` directly without `EraTurnRight`'s
    /// `KeeperHolderRestore` guard. Before `b4.4` activates this path in
    /// production, restore `World::baton_holder` to the era's escrowing holder
    /// strictly before `WorldLend::drop` sends `KeeperMsg::GroupAbandon`
    /// (preferably as the root fix inside `WorldLend::drop`).
    #[cold]
    #[inline(never)]
    pub(crate) fn from_active_green_seat(
        ctx: &Context,
        turn: WorldTurn,
        lend: WorldLend,
        retention: FacadeEpochRetention,
    ) -> Result<Self, (WorldTurn, WorldLend, FacadeEpochRetention, WorldSeatRefusal)> {
        let Some(slab_ref) = ctx.owner.slab() else {
            return Err((turn, lend, retention, WorldSeatRefusal::Unseated));
        };
        if slab_ref.green_seat().is_none() {
            return Err((turn, lend, retention, WorldSeatRefusal::Unseated));
        }
        if ctx.green_runtime_ref().is_none() {
            return Err((turn, lend, retention, WorldSeatRefusal::NotGreenWorld));
        }
        if turn.identity().heap != ctx.tagged_heap.heap_identity()
            || retention.heap_identity() != turn.identity().heap
        {
            return Err((turn, lend, retention, WorldSeatRefusal::WrongHeap));
        }
        if !neovm_value::tagged::gc::tagged_heap_is_current(&ctx.tagged_heap) {
            return Err((turn, lend, retention, WorldSeatRefusal::NotInstalled));
        }
        if !ctx.holds_serial_world_here() {
            return Err((turn, lend, retention, WorldSeatRefusal::WorldNotHeldHere));
        }
        Ok(Self {
            slab: slab_ref as *const _ as usize,
            turn,
            lend,
            retention,
            _thread: PhantomData,
        })
    }

    /// Decompose a resumed [`ActiveWorldSeat`] back into `(turn, lend, retention)`
    /// so `GreenSeat` can restore its `TurnRight::Era`.
    #[cold]
    #[inline(never)]
    pub(crate) fn into_active_green_parts(self) -> (WorldTurn, WorldLend, FacadeEpochRetention) {
        (self.turn, self.lend, self.retention)
    }

    /// Intra-interval Baton-group handoff (b4.2): publish parked private
    /// roots, unbind host bindings, stow World carriers to
    /// `CarriedHolder::Stowed`, and park carrier TLS WITHOUT consuming
    /// `WorldTurn`, returning `(ParkedWorldSeat, WorldTurn, WorldLend)` so
    /// another Baton thread can `switch_in` during the same keeper stop.
    ///
    /// # Safety
    ///
    /// Same dormancy and deferred-reset contract as
    /// [`ParkedWorldSeat::park_seated`]: after returning, `ctx`'s TLS heap slot
    /// is cleared via [`park_carrier_tls`] and caller must not re-enter `ctx`
    /// or reuse this OS thread for another `Context` without completing
    /// [`reset_carrier_tls`] (e.g. via `enter`/`switch_in`).
    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    pub(crate) unsafe fn switch_out(
        self,
        ctx: &mut Context,
    ) -> (ParkedWorldSeat, WorldTurn, WorldLend) {
        let (roots, retention) = {
            let abort = AbortOnPanic::new("world-seat-switch-out");
            let current_slab = ctx.owner.slab().map(|s| s as *const _ as usize);
            assert_eq!(
                current_slab,
                Some(self.slab),
                "active World seat must switch_out the same Context slab it entered"
            );
            assert!(
                neovm_value::tagged::gc::tagged_heap_is_current(&ctx.tagged_heap),
                "active World seat must still have its own heap installed on switch_out"
            );
            let (roots, retention) = ctx
                .park_world_seat_with_roots(Some(&self.retention))
                .expect("active World seat must publish parked roots on switch_out");
            let stowed = ctx.stow_serial_world_state();
            assert!(
                stowed,
                "active World seat must stow World carriers into GreenRuntime on switch_out"
            );
            // SAFETY: active World seat parked its roots and stowed carried
            // World state under `AbortOnPanic`, no TLS heap loan is live, and
            // caller of `switch_out` upholds the dormancy and deferred
            // `reset_carrier_tls()` obligations.
            unsafe {
                park_carrier_tls();
            }
            abort.disarm();
            (roots, retention)
        };
        drop(self.retention);
        (
            ParkedWorldSeat {
                slab: self.slab,
                roots,
                retention,
                _thread: PhantomData,
            },
            self.turn,
            self.lend,
        )
    }

    /// End the Baton-group interval: run [`Self::switch_out`] to publish
    /// parked roots, stow World carriers to `CarriedHolder::Stowed`, and park
    /// carrier TLS, and ONLY THEN consume `WorldTurn` into [`WorldReturned`]
    /// via [`WorldReturned::from_stowed_seat`] (Nit 6).
    ///
    /// # Safety
    ///
    /// Same dormancy and deferred-reset contract as [`Self::switch_out`].
    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    pub(crate) unsafe fn exit(
        self,
        ctx: &mut Context,
    ) -> (ParkedWorldSeat, WorldLend, WorldReturned) {
        // SAFETY: caller upholds `switch_out`'s dormancy and deferred-reset
        // contract above.
        let (parked, turn, lend) = unsafe { self.switch_out(ctx) };
        let returned = WorldReturned::from_stowed_seat(turn, StowedForReturn(()));
        (parked, lend, returned)
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
#[path = "tests/world_seat_test.rs"]
mod tests;

//! L6.7b b3 (pentry seat): activate a seated Green World host under a
//! [`WorldTurn`] and stow its World carriers before returning the turn.
//!
//! Unlike unseated parallel owners (which use [`super::serial_turn::SerialHost`]
//! and are refused when seated via `HostRefusal::Seated`), Baton-group threads
//! (main and `make-thread` workers) hold a Green seat and stow/unstow through
//! `GreenRuntime`'s carried TLS and warm `OwnerCarried` slots (`green/switch.rs`).
//!
//! Contracts:
//! - [`ParkedWorldSeat::park_seated`] accepts only a seated Green `Context`
//!   whose own heap is currently installed on this OS thread and that holds
//!   the World's carried state here; it parks host bindings, stows the World's
//!   carriers to `CarriedHolder::Stowed`, and parks carrier TLS.
//! - [`ParkedWorldSeat::enter`] fails closed on every invariant before
//!   mutating TLS or World state: seated Green host, structural heap identity
//!   matching [`WorldTurnIdentity::heap`] (Condition A-3), no foreign heap
//!   installed on this OS thread, and `CarriedHolder::Stowed` (Condition A-1).
//!   Any refusal hands back `(self, turn, lend, refusal)` unchanged so the
//!   armed [`WorldLend`] is never leaked silently.
//! - [`ActiveWorldSeat::exit`] parks host bindings, stows the World carriers
//!   back to `CarriedHolder::Stowed`, and parks carrier TLS strictly BEFORE
//!   converting `WorldTurn` into [`WorldReturned`] via
//!   [`WorldReturned::from_end`], discharging A-1 liveness for subsequent
//!   parallel-owner serial turns.
//! - A panic unwinding between the multi-step TLS/stow transitions aborts the
//!   process (matching `serial_detach.rs` and `green/switch.rs`); a panic
//!   inside the active Lisp interval drops `ActiveWorldSeat`, whose armed
//!   [`WorldLend`] automatically sends `KeeperMsg::GroupAbandon { cause: Unwound }`
//!   (K7).
#![forbid(unsafe_code)]

use super::super::green::carrier_tls::{park_carrier_tls, reset_carrier_tls};
use super::super::{Context, ParkedOwner};
use super::keeper_driver::WorldLend;
use super::world_turn::{WorldReturned, WorldTurn, WorldTurnIdentity};
use std::marker::PhantomData;

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
    /// `enter` (A-3): `WorldTurnIdentity::heap` does not match this Context's own heap.
    WrongHeap,
    /// `enter`: another Context's heap is already installed on this OS thread.
    HeapInstalled,
    /// `enter` (A-1): the Green World's carriers are held elsewhere rather than stowed.
    WorldHeldElsewhere,
}

/// Proof that a seated Green `Context` is currently parked off this OS thread
/// with its host bindings unbound and carrier TLS parked.
#[must_use = "enter the parked World seat under a WorldTurn"]
#[derive(Debug)]
pub(crate) struct ParkedWorldSeat {
    parked: ParkedOwner,
    _thread: PhantomData<*const ()>,
}

/// Live Baton-group World interval holding a [`WorldTurn`] and its `Send` K7
/// [`WorldLend`] guard.
#[must_use = "exit the active World seat to stow carriers and return the turn"]
#[derive(Debug)]
pub(crate) struct ActiveWorldSeat {
    turn: WorldTurn,
    lend: WorldLend,
    _thread: PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(ParkedWorldSeat: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ActiveWorldSeat: Send, Sync, Clone, Copy);

/// Aborts the process when a panic unwinds midway through a park/enter/exit
/// TLS transition (matching `serial_detach.rs` and `green/switch.rs`).
struct AbortOnPanic(&'static str);

impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            tracing::error!(
                step = self.0,
                "a seated Green World host panicked between carrier-TLS transition steps; aborting"
            );
            std::process::abort();
        }
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "L6.7b b4 (Green switch points) is the caller")
)]
impl ParkedWorldSeat {
    /// Park a live seated Green `Context` off this OS thread so a parallel era
    /// can lend World turns through the keeper ring.
    #[cold]
    #[inline(never)]
    pub(crate) fn park_seated(ctx: &mut Context) -> Result<Self, WorldSeatRefusal> {
        if ctx.owner.slab().and_then(|s| s.green_seat()).is_none() {
            return Err(WorldSeatRefusal::Unseated);
        }
        if ctx.green_runtime_ref().is_none() {
            return Err(WorldSeatRefusal::NotGreenWorld);
        }
        if !neovm_value::tagged::gc::tagged_heap_is_current(&ctx.tagged_heap) {
            return Err(WorldSeatRefusal::NotInstalled);
        }
        if !ctx.holds_serial_world_here() {
            return Err(WorldSeatRefusal::WorldNotHeldHere);
        }
        let _abort = AbortOnPanic("world-seat-park");
        let parked = ctx.park_owner();
        let stowed = ctx.stow_serial_world_state();
        assert!(
            stowed,
            "verified World carrier holder must stow into GreenRuntime"
        );
        park_carrier_tls();
        Ok(Self {
            parked,
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
        if ctx.owner.slab().and_then(|s| s.green_seat()).is_none() {
            return Err((self, turn, lend, WorldSeatRefusal::Unseated));
        }
        if ctx.green_runtime_ref().is_none() {
            return Err((self, turn, lend, WorldSeatRefusal::NotGreenWorld));
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
        let _abort = AbortOnPanic("world-seat-enter");
        reset_carrier_tls();
        ctx.unstow_serial_world_state();
        ctx.resume_owner(self.parked);
        Ok(ActiveWorldSeat {
            turn,
            lend,
            _thread: PhantomData,
        })
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "L6.7b b4 (Green switch points) is the caller")
)]
impl ActiveWorldSeat {
    pub(crate) fn identity(&self) -> WorldTurnIdentity {
        self.turn.identity()
    }

    /// End the Baton-group interval: unbind host bindings, stow the World's
    /// carriers back to `CarriedHolder::Stowed`, park carrier TLS, and only
    /// then consume `WorldTurn` into `WorldReturned`.
    #[cold]
    #[inline(never)]
    pub(crate) fn exit(self, ctx: &mut Context) -> (ParkedWorldSeat, WorldLend, WorldReturned) {
        let parked = {
            let _abort = AbortOnPanic("world-seat-exit");
            assert!(
                neovm_value::tagged::gc::tagged_heap_is_current(&ctx.tagged_heap),
                "active World seat must still have its own heap installed on exit"
            );
            let parked = ctx.park_owner();
            let stowed = ctx.stow_serial_world_state();
            assert!(
                stowed,
                "active World seat must stow World carriers into GreenRuntime on exit"
            );
            park_carrier_tls();
            parked
        };
        let returned = WorldReturned::from_end(self.turn);
        (
            ParkedWorldSeat {
                parked,
                _thread: PhantomData,
            },
            self.lend,
            returned,
        )
    }
}

#[cfg(test)]
#[path = "tests/world_seat_test.rs"]
mod tests;

//! L6.7b b3: the Baton group's World turn.
//!
//! The keeper lends it from its stop custody, as it lends an owner's serial
//! ticket, but exports nothing: the World is not an owner image, so the
//! keeper keeps the stopped interval while the turn is out and no parallel
//! owner runs. The group's runner (pentry's seat) activates the World host
//! under the turn, stows it, and gives the turn back with
//! [`WorldReturned::from_end`]; the keeper accepts exactly the turn it cut.
#![forbid(unsafe_code)]

use neovm_value::tagged::gc::HeapIdentity;
use neovm_value::tagged::gc::heap_transfer::{StoppedInterval, UnenteredReservation};
use std::cell::Cell;
use std::marker::PhantomData;
use std::num::NonZeroU64;

/// Facts the keeper checks when a turn comes back. Copyable identity, not
/// a loan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorldTurnIdentity {
    pub heap: HeapIdentity,
    /// Epoch of the stop that cut this turn.
    pub epoch: NonZeroU64,
}

/// The Baton group's turn at the World, cut under the keeper's stop. `Send`,
/// so it reaches the group's runner and moves with the Baton between Baton
/// threads; neither `Sync` nor `Clone`. Ending it is the only way to give it
/// back.
#[derive(Debug)]
#[must_use = "end the World turn so the keeper can complete its stop"]
pub(crate) struct WorldTurn {
    identity: WorldTurnIdentity,
    _not_sync: PhantomData<Cell<()>>,
}

/// A World turn on its way back to the keeper.
#[derive(Debug)]
#[must_use = "send the returned World turn to the keeper"]
pub(crate) struct WorldReturned {
    identity: WorldTurnIdentity,
    _not_sync: PhantomData<Cell<()>>,
}

static_assertions::assert_impl_all!(WorldTurn: Send);
static_assertions::assert_not_impl_any!(WorldTurn: Sync, Clone, Copy);
static_assertions::assert_impl_all!(WorldReturned: Send);
static_assertions::assert_not_impl_any!(WorldReturned: Sync, Clone, Copy);

impl WorldTurn {
    /// Cut under the keeper's own stop (lead #14075). The exclusive borrow
    /// of the stopped interval is the proof: only the keeper's custody holds
    /// one, and only while every member is stopped. The turn carries that
    /// stop's heap and epoch.
    pub(super) fn cut_in_stop(interval: &mut StoppedInterval<'_, '_>) -> Self {
        Self::cut(WorldTurnIdentity {
            heap: interval.heap_identity(),
            epoch: interval.epoch(),
        })
    }

    /// Cut under the reservation a returned serial ticket restored: the same
    /// stop, after an owner's turn in it.
    pub(super) fn cut_in_reservation(reservation: &mut UnenteredReservation<'_, '_>) -> Self {
        Self::cut(WorldTurnIdentity {
            heap: reservation.heap_identity(),
            epoch: reservation.epoch(),
        })
    }

    fn cut(identity: WorldTurnIdentity) -> Self {
        Self {
            identity,
            _not_sync: PhantomData,
        }
    }

    #[cfg(test)]
    pub(crate) fn cut_for_test(identity: WorldTurnIdentity) -> Self {
        Self::cut(identity)
    }

    pub(crate) fn identity(&self) -> WorldTurnIdentity {
        self.identity
    }
}

impl WorldReturned {
    /// Production mint (Nit 6): only `world_seat` can construct
    /// `StowedForReturn`, proving alloc regions are closed, World carriers are
    /// stowed, parked roots are published, and carrier TLS is parked before the
    /// turn returns to the keeper.
    pub(crate) fn from_stowed_seat(
        turn: WorldTurn,
        _proof: super::world_seat::StowedForReturn,
    ) -> Self {
        Self {
            identity: turn.identity,
            _not_sync: PhantomData,
        }
    }

    /// Test-only return. Production return is [`Self::from_stowed_seat`].
    #[cfg(test)]
    pub(crate) fn from_end(turn: WorldTurn) -> Self {
        Self {
            identity: turn.identity,
            _not_sync: PhantomData,
        }
    }

    pub(crate) fn identity(&self) -> WorldTurnIdentity {
        self.identity
    }
}

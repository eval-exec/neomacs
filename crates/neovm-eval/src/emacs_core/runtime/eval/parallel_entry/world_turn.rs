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
    /// Cut by the keeper's custody only: the driver binding lends it.
    pub(super) fn cut(identity: WorldTurnIdentity) -> Self {
        Self {
            identity,
            _not_sync: PhantomData,
        }
    }

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "pentry's World seat (L6.7b b3) is the caller")
    )]
    pub(crate) fn identity(&self) -> WorldTurnIdentity {
        self.identity
    }
}

impl WorldReturned {
    /// End the group's interval: consume the turn so it goes back to the
    /// keeper. The seat stows the World host first.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "pentry's World seat (L6.7b b3) is the caller")
    )]
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

//! E5.2: a serial turn parks over the L6 ring and resumes on a later grant
//! (E5-DESIGN §2; E5.2-PARK-RESUME r0).
//!
//! A serial turn runs inside a stop: the keeper lends the World to one owner
//! while every other owner is stopped. A turn that reaches a GNU release
//! point must not hold that stop while it waits (DESIGN §4), so it parks:
//! the host leaves the OS thread (E5.1 `park`, the L6.4 detach), the ticket
//! goes back to the keeper, and the owner waits with its image still
//! published. On wake it asks for a new turn at the ring tail and, on the
//! grant, receives the new ticket and resumes the SAME turn on its own stack.
//!
//! The owner side never holds a ticket across the wait: [`LiveSerial::park`]
//! returns the ticket to the keeper itself, and a [`ParkedSerial`] holds only
//! the parked turn and the owner's fixed facts. The return and the later
//! request travel through one [`KeeperPort`], in order, because the ring
//! holds one entry per owner.
//!
//! A publication receipt is single-use (`ready_request` consumes it) and only
//! a native publication mints one, so every request publishes afresh: the
//! generation is per grant, like the stop epoch. The owner is NativeStopped
//! for the whole wait; it leaves native only inside `request_turn`, after
//! the wake, and publishes again before the request is committed.
//!
//! The keeper side is L6.7's service; the rows drive it with a test keeper.

use super::keeper_custody::ReturnedSerialTicket;
use super::serial_ticket::{
    LocalSerialOwner, ReceiveReason, SerialDestination, StoppedSerialTicket, TicketIdentity,
};
use super::serial_turn::{
    OwnerSerialState, ParkedTurn, SerialHost, SerialTurn, TicketOwner, TurnRefusal,
};
use neovm_value::tagged::gc::HeapIdentity;
use std::num::NonZeroU64;

/// The owner key of a received ticket is the owner id alone (#12338); the
/// generation and the stop epoch are checked by `receive_serial`.
impl TicketOwner for LocalSerialOwner {
    type Owner = u64;
    fn owner(&self) -> u64 {
        self.identity().owner
    }
}

/// What a serial owner is across all its turns: its owner id and the era
/// heap it is registered on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SerialHome {
    pub owner: u64,
    pub heap: HeapIdentity,
}

/// One grant: the lent ticket, the publication its request was committed
/// under, and the epoch of the stop that cut it. The generation is the
/// owner's own fact (the port reads it from the receipt it published); the
/// epoch is the keeper's.
#[derive(Debug)]
pub(crate) struct Grant {
    pub ticket: StoppedSerialTicket,
    pub generation: NonZeroU64,
    pub epoch: NonZeroU64,
}

impl Grant {
    /// The identity a seat for this grant expects.
    fn expected(&self, home: SerialHome) -> TicketIdentity {
        TicketIdentity {
            owner: home.owner,
            generation: self.generation,
            heap: home.heap,
            epoch: self.epoch,
        }
    }
}

/// Why the keeper did not grant a requested turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))] // L6.7's port refuses; the rows do now.
pub(crate) enum PortRefusal {
    /// The era was poisoned; no further grant will come.
    Poisoned,
}

/// The owner's side of the serial ring.
///
/// One port per owner, and its calls are ordered: a ticket given back is
/// processed by the keeper before a later request from the same owner, so
/// the request never meets the owner's previous ring entry.
pub(crate) trait KeeperPort {
    /// Give a finished turn's ticket back to the keeper.
    fn give_back(&mut self, returned: ReturnedSerialTicket);
    /// Publish this owner's image, commit a request at the ring tail under
    /// that publication, and wait, NativeStopped, for the grant.
    fn request_turn(&mut self) -> Result<Grant, PortRefusal>;
    /// A granted ticket this owner could not receive goes back unopened.
    fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason);
}

/// A turn in progress: the E5.1 turn and the seat that received its ticket.
#[must_use = "a serial turn must end or park"]
pub(crate) struct LiveSerial<'t, C: SerialHost> {
    turn: SerialTurn<'t, C, LocalSerialOwner>,
    seat: SerialDestination,
    home: SerialHome,
}

/// A parked turn. It holds no ticket: only the parked E5.1 turn and the
/// owner's fixed facts.
#[must_use = "a parked serial turn must resume"]
pub(crate) struct ParkedSerial<'t, C: SerialHost, W> {
    parked: ParkedTurn<'t, C, LocalSerialOwner, W>,
    home: SerialHome,
}

/// Why a turn did not begin or resume. The ticket already went back.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TurnStartRefusal<R, A> {
    Port(PortRefusal),
    Receive(ReceiveReason),
    /// The caller's activation refused the received owner.
    Activate(A),
    Turn(TurnRefusal<R>),
}

/// Receive a granted ticket into a fresh seat armed for `home` at the grant's
/// generation and epoch, then run the caller's activation on the received
/// owner. A receive
/// refusal sends the ticket back unopened; an activation refusal gives the
/// received owner back through its seat. Activation owns its own undo:
/// nothing is half-activated when it refuses.
fn receive<R, A>(
    home: SerialHome,
    grant: Grant,
    port: &mut impl KeeperPort,
    activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
) -> Result<(SerialDestination, LocalSerialOwner), TurnStartRefusal<R, A>> {
    let mut seat = SerialDestination::new(grant.expected(home));
    let mut local = match seat.receive_serial(grant.ticket) {
        Ok(received) => received.guard(),
        Err(refusal) => {
            let reason = refusal.reason;
            port.refuse(refusal.ticket, reason);
            return Err(TurnStartRefusal::Receive(reason));
        }
    };
    match activate(&mut local) {
        Ok(()) => Ok((seat, local)),
        Err(refusal) => {
            give_back(seat, local, port);
            Err(TurnStartRefusal::Activate(refusal))
        }
    }
}

/// Finish a local owner on the seat that received it and give its ticket
/// back. The pairing is structural: a `LiveSerial` holds the seat beside the
/// turn whose owner that seat received, and E5.1 hands back the same owner.
/// A mismatch would leave the keeper's stop open forever, so it fails loudly.
fn give_back(mut seat: SerialDestination, local: LocalSerialOwner, port: &mut impl KeeperPort) {
    match local.finish(&mut seat) {
        Ok(returnable) => port.give_back(returnable.return_ticket()),
        Err(local) => panic!(
            "a serial seat refused the owner it received: {:?}",
            local.identity()
        ),
    }
}

/// Begin a turn on a grant. `home` names the owner and the era heap; the
/// grant supplies the publication generation and the stop epoch.
/// ACTIVATE is the heap loan into the received owner (pentry's activation in
/// production; the rows' standalone owners need none).
#[cold]
#[inline(never)]
#[cfg_attr(not(test), allow(dead_code))] // E5.3/E6-P call it; the rows do now.
pub(crate) fn begin_turn<'t, C: SerialHost, A>(
    state: &'t mut OwnerSerialState<C, LocalSerialOwner>,
    home: SerialHome,
    grant: Grant,
    port: &mut impl KeeperPort,
    activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
) -> Result<LiveSerial<'t, C>, TurnStartRefusal<C::Refusal, A>> {
    let (seat, local) = receive(home, grant, port, activate)?;
    match state.begin(local) {
        Ok(turn) => Ok(LiveSerial { turn, seat, home }),
        Err((local, refusal)) => {
            give_back(seat, local, port);
            Err(TurnStartRefusal::Turn(refusal))
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))] // E5.3/E6-P call it; the rows do now.
impl<'t, C: SerialHost> LiveSerial<'t, C> {
    /// Lend the owner's Context to BODY (E5.1 `with_world`).
    pub(crate) fn with_world<R>(&mut self, body: impl for<'g> FnOnce(&'g mut C) -> R) -> R {
        self.turn.with_world(body)
    }

    /// End the turn: the host leaves the thread and the ticket goes back.
    /// A host that refuses to detach keeps the turn live.
    #[cold]
    #[inline(never)]
    pub(crate) fn end(self, port: &mut impl KeeperPort) -> Result<(), (Self, C::Refusal)> {
        let LiveSerial { turn, seat, home } = self;
        match turn.end() {
            Ok(local) => {
                give_back(seat, local, port);
                Ok(())
            }
            Err((turn, refusal)) => Err((LiveSerial { turn, seat, home }, refusal)),
        }
    }

    /// Park on WAIT: the host leaves the thread and the ticket goes back
    /// before this returns, so nothing ticket-shaped survives the wait. A
    /// host that refuses to detach keeps the turn live and hands WAIT back.
    #[cold]
    #[inline(never)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn park<W>(
        self,
        wait: W,
        port: &mut impl KeeperPort,
    ) -> Result<ParkedSerial<'t, C, W>, (Self, W, C::Refusal)> {
        let LiveSerial { turn, seat, home } = self;
        match turn.park(wait) {
            Ok((parked, local)) => {
                give_back(seat, local, port);
                Ok(ParkedSerial { parked, home })
            }
            Err((turn, wait, refusal)) => Err((LiveSerial { turn, seat, home }, wait, refusal)),
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))] // E5.3/E6-P call it; the rows do now.
impl<'t, C: SerialHost, W> ParkedSerial<'t, C, W> {
    /// What this turn waits on.
    pub(crate) fn wait(&self) -> &W {
        self.parked.wait()
    }

    /// Ask for a new turn at the ring tail and resume this one on the grant,
    /// activating the received owner as [`begin_turn`] does. Any refusal
    /// keeps the turn parked; a received ticket that cannot be used goes back
    /// to the keeper first.
    #[cold]
    #[inline(never)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn resume<A>(
        self,
        port: &mut impl KeeperPort,
        activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
    ) -> Result<LiveSerial<'t, C>, (Self, TurnStartRefusal<C::Refusal, A>)> {
        let ParkedSerial { parked, home } = self;
        let grant = match port.request_turn() {
            Ok(grant) => grant,
            Err(refusal) => {
                return Err((
                    ParkedSerial { parked, home },
                    TurnStartRefusal::Port(refusal),
                ));
            }
        };
        let (seat, local) = match receive(home, grant, port, activate) {
            Ok(received) => received,
            Err(refusal) => return Err((ParkedSerial { parked, home }, refusal)),
        };
        match parked.resume(local) {
            Ok(turn) => Ok(LiveSerial { turn, seat, home }),
            Err((parked, local, refusal)) => {
                give_back(seat, local, port);
                Err((
                    ParkedSerial { parked, home },
                    TurnStartRefusal::Turn(refusal),
                ))
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/serial_park_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/serial_park_live_test.rs"]
mod live_tests;

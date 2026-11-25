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
pub(crate) enum PortRefusal {
    /// The era was poisoned; no further grant will come.
    Poisoned,
    /// An owner-side refusal (K8 r2): this owner could not publish within the
    /// contention bound, or its request was sent back to publish more often
    /// than the republish limit. Nothing was lent and nothing is poisoned:
    /// request again. A slow keeper stop never answers this; the requester
    /// just waits for its grant.
    Retry,
}

/// The owner's hold on one lent ticket (L6.7 K7). It is armed while the
/// ticket is out and disarmed right after the ticket goes back. A guard that
/// drops armed means the ticket was lost: the port's guard reports that to
/// the keeper, which poisons, because only a Rust unwind or a leaked turn
/// can drop it armed. A Lisp signal ends the turn normally.
pub(crate) trait LendGuard {
    /// The ticket went back to the keeper.
    fn disarm(self);
}

/// The unit-test ports lend with no keeper to tell.
impl LendGuard for () {
    fn disarm(self) {}
}

/// The owner's side of the serial ring.
///
/// One port per owner, and its calls are ordered: a ticket given back is
/// processed by the keeper before a later request from the same owner, so
/// the request never meets the owner's previous ring entry.
pub(crate) trait KeeperPort {
    /// The guard over one lent ticket.
    type LendGuard: LendGuard;
    /// Where a collection's report goes (L6.7c (c)).
    type CollectReply;
    /// Give a finished turn's ticket back to the keeper.
    fn give_back(&mut self, returned: ReturnedSerialTicket);
    /// Give a parked turn's ticket back asking for a collection (L6.7c (c)).
    /// The keeper holds the ring's HEAD for this owner's next request and
    /// collects first in the stop that grants it; REPLY gets the report.
    fn give_back_to_collect(&mut self, returned: ReturnedSerialTicket, reply: Self::CollectReply);
    /// Publish this owner's image, commit a request at the ring tail under
    /// that publication, and wait, NativeStopped, for the grant. The guard
    /// is armed: the ticket is out from here until it goes back.
    fn request_turn(&mut self) -> Result<(Grant, Self::LendGuard), PortRefusal>;
    /// A granted ticket this owner could not receive goes back unopened.
    fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason);
}

/// A turn in progress: the E5.1 turn, the seat that received its ticket,
/// and the armed guard over that ticket.
#[must_use = "a serial turn must end or park"]
pub(crate) struct LiveSerial<'t, C: SerialHost, G: LendGuard = ()> {
    turn: SerialTurn<'t, C, LocalSerialOwner>,
    seat: SerialDestination,
    home: SerialHome,
    guard: G,
}

/// What a turn parked for a collection waits on (L6.7c (c)): the grant the
/// keeper lends it at the ring's HEAD, after the collection.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Collecting;

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
fn receive<P: KeeperPort, R, A>(
    home: SerialHome,
    grant: Grant,
    guard: P::LendGuard,
    port: &mut P,
    activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
) -> Result<(SerialDestination, LocalSerialOwner, P::LendGuard), TurnStartRefusal<R, A>> {
    let mut seat = SerialDestination::new(grant.expected(home));
    let mut local = match seat.receive_serial(grant.ticket) {
        Ok(received) => received.guard(),
        Err(refusal) => {
            let reason = refusal.reason;
            port.refuse(refusal.ticket, reason);
            guard.disarm();
            return Err(TurnStartRefusal::Receive(reason));
        }
    };
    // An unwind out of `activate` drops `guard` armed: the keeper learns the
    // ticket is lost instead of waiting for it.
    match activate(&mut local) {
        Ok(()) => Ok((seat, local, guard)),
        Err(refusal) => {
            give_back(seat, local, port);
            guard.disarm();
            Err(TurnStartRefusal::Activate(refusal))
        }
    }
}

/// Finish a local owner on the seat that received it, for its ticket's
/// return. The pairing is structural: a `LiveSerial` holds the seat beside
/// the turn whose owner that seat received, and E5.1 hands back the same
/// owner. A mismatch would leave the keeper's stop open forever, so it fails
/// loudly.
fn returned(mut seat: SerialDestination, local: LocalSerialOwner) -> ReturnedSerialTicket {
    match local.finish(&mut seat) {
        Ok(returnable) => returnable.return_ticket(),
        Err(local) => panic!(
            "a serial seat refused the owner it received: {:?}",
            local.identity()
        ),
    }
}

/// Finish a local owner and give its ticket back.
fn give_back(seat: SerialDestination, local: LocalSerialOwner, port: &mut impl KeeperPort) {
    port.give_back(returned(seat, local));
}

/// Step (a) of SERIAL-TURN-CONTEXT r1 point 2: verify that the received
/// ticket's local serial heap loan matches the dormant host's heap identity
/// before `OwnerSerialState::begin` / `ParkedTurn::resume` runs steps (b)-(d)
/// (reattach: heap TLS install, frame/slab/buffer, and carried/warm World
/// restore). Refuses a mismatch with [`ReceiveReason::WrongHeap`] without
/// touching TLS.
#[cold]
#[inline(never)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E5.3, E5.4 and E6-P pass this closure to begin_turn and ParkedSerial::resume; the rows call it now"
    )
)]
pub(crate) fn verify_serial_host_heap(
    host_heap: HeapIdentity,
) -> impl FnOnce(&mut LocalSerialOwner) -> Result<(), ReceiveReason> {
    move |local| local.verify_host_heap(host_heap)
}

/// Begin a turn on a grant. `home` names the owner and the era heap; the
/// grant supplies the publication generation and the stop epoch.
/// ACTIVATE is the heap loan check on the received owner
/// ([`verify_serial_host_heap`] in production; the standalone rows pass a
/// no-op).
#[cold]
#[inline(never)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "E5.3, E5.4 and E6-P call it; the rows do now")
)]
pub(crate) fn begin_turn<'t, C: SerialHost, P: KeeperPort, A>(
    state: &'t mut OwnerSerialState<C, LocalSerialOwner>,
    home: SerialHome,
    (grant, guard): (Grant, P::LendGuard),
    port: &mut P,
    activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
) -> Result<LiveSerial<'t, C, P::LendGuard>, TurnStartRefusal<C::Refusal, A>> {
    let (seat, local, guard) = receive(home, grant, guard, port, activate)?;
    match state.begin(local) {
        Ok(turn) => Ok(LiveSerial {
            turn,
            seat,
            home,
            guard,
        }),
        Err((local, refusal)) => {
            give_back(seat, local, port);
            guard.disarm();
            Err(TurnStartRefusal::Turn(refusal))
        }
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "E5.3, E5.4 and E6-P call it; the rows do now")
)]
impl<'t, C: SerialHost, G: LendGuard> LiveSerial<'t, C, G> {
    /// Lend the owner's Context to BODY (E5.1 `with_world`).
    pub(crate) fn with_world<R>(&mut self, body: impl for<'g> FnOnce(&'g mut C) -> R) -> R {
        self.turn.with_world(body)
    }

    /// End the turn: the host leaves the thread and the ticket goes back.
    /// A host that refuses to detach keeps the turn live.
    #[cold]
    #[inline(never)]
    pub(crate) fn end<P: KeeperPort<LendGuard = G>>(
        self,
        port: &mut P,
    ) -> Result<(), (Self, C::Refusal)> {
        let LiveSerial {
            turn,
            seat,
            home,
            guard,
        } = self;
        match turn.end() {
            Ok(local) => {
                give_back(seat, local, port);
                guard.disarm();
                Ok(())
            }
            Err((turn, refusal)) => Err((
                LiveSerial {
                    turn,
                    seat,
                    home,
                    guard,
                },
                refusal,
            )),
        }
    }

    /// Park on WAIT: the host leaves the thread and the ticket goes back
    /// before this returns, so nothing ticket-shaped survives the wait. A
    /// host that refuses to detach keeps the turn live and hands WAIT back.
    #[cold]
    #[inline(never)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn park<W, P: KeeperPort<LendGuard = G>>(
        self,
        wait: W,
        port: &mut P,
    ) -> Result<ParkedSerial<'t, C, W>, (Self, W, C::Refusal)> {
        let LiveSerial {
            turn,
            seat,
            home,
            guard,
        } = self;
        match turn.park(wait) {
            Ok((parked, local)) => {
                give_back(seat, local, port);
                guard.disarm();
                Ok(ParkedSerial { parked, home })
            }
            Err((turn, wait, refusal)) => Err((
                LiveSerial {
                    turn,
                    seat,
                    home,
                    guard,
                },
                wait,
                refusal,
            )),
        }
    }

    /// Park for a collection (L6.7c (c), the b5 shape for an owner): the
    /// host leaves the thread and the ticket goes back asking the keeper to
    /// collect, before this returns. The keeper holds the ring's HEAD for
    /// this owner, so the [`ParkedSerial::resume`] that follows asks again
    /// under a new publication, is granted in the stop that collects first,
    /// and no other client's turn comes in between. REPLY gets that
    /// collection's report. No stop runs until that resume asks, so a resume
    /// refused `Retry` (this owner could not publish) keeps the HEAD held:
    /// resume again. A host that refuses to detach keeps the turn live and
    /// hands REPLY back.
    #[cold]
    #[inline(never)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn park_to_collect<P: KeeperPort<LendGuard = G>>(
        self,
        reply: P::CollectReply,
        port: &mut P,
    ) -> Result<ParkedSerial<'t, C, Collecting>, (Self, P::CollectReply, C::Refusal)> {
        let LiveSerial {
            turn,
            seat,
            home,
            guard,
        } = self;
        match turn.park(Collecting) {
            Ok((parked, local)) => {
                port.give_back_to_collect(returned(seat, local), reply);
                guard.disarm();
                Ok(ParkedSerial { parked, home })
            }
            Err((turn, Collecting, refusal)) => Err((
                LiveSerial {
                    turn,
                    seat,
                    home,
                    guard,
                },
                reply,
                refusal,
            )),
        }
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "E5.3, E5.4 and E6-P call it; the rows do now")
)]
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
    pub(crate) fn resume<P: KeeperPort, A>(
        self,
        port: &mut P,
        activate: impl FnOnce(&mut LocalSerialOwner) -> Result<(), A>,
    ) -> Result<LiveSerial<'t, C, P::LendGuard>, (Self, TurnStartRefusal<C::Refusal, A>)> {
        let ParkedSerial { parked, home } = self;
        let (grant, guard) = match port.request_turn() {
            Ok(granted) => granted,
            Err(refusal) => {
                return Err((
                    ParkedSerial { parked, home },
                    TurnStartRefusal::Port(refusal),
                ));
            }
        };
        let (seat, local, guard) = match receive(home, grant, guard, port, activate) {
            Ok(received) => received,
            Err(refusal) => return Err((ParkedSerial { parked, home }, refusal)),
        };
        match parked.resume(local) {
            Ok(turn) => Ok(LiveSerial {
                turn,
                seat,
                home,
                guard,
            }),
            Err((parked, local, refusal)) => {
                give_back(seat, local, port);
                guard.disarm();
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

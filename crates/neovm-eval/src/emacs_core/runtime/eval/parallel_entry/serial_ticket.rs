//! L6.3: the dormant serial-ticket transport.
//!
//! The ticket carries the owner, the generation, the heap, and the stop epoch
//! the destination validates. It does not carry a Context, a collector, a
//! slab, or a manager.
//! The Baton stays with the held keeper loan. `receive_serial` names the
//! ticket's heap in a local loan and does not install it. Installing the
//! heap, and restoring the frame, slab, buffer, and warm carrier, are the
//! existing reattach. A refusal returns the ticket. Nothing is half-received.
//!
//! Production mint is [`StoppedSerialTicket::from_lend`]: it consumes the
//! keeper lend, so a ticket exists only because that lend was cut. The
//! identity constructor [`StoppedSerialTicket::transport`] exists only in
//! tests. A non-test build has no way to mint a ticket from a copied
//! [`TicketIdentity`].

use neovm_value::tagged::gc::HeapIdentity;
use std::cell::Cell;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;

/// Facts `receive_serial` checks. Copyable identity, not a loan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TicketIdentity {
    pub owner: u64,
    pub generation: NonZeroU64,
    pub heap: HeapIdentity,
    /// Epoch of the stop that cut this ticket. A later stop of the same
    /// owner is a different epoch.
    pub epoch: NonZeroU64,
}

/// Send ticket from the keeper to the destination owner.
#[derive(Debug)]
#[must_use]
#[allow(dead_code)] // Dormant L6.3; L6.4 begin is the caller.
pub(crate) struct StoppedSerialTicket {
    identity: TicketIdentity,
    _not_sync: PhantomData<Cell<()>>,
}

/// The lend after its one ticket has been cut.
///
/// The ticket cannot hold this loan: the ticket is `Send` and the stopped
/// reservation is not. Dropping the reservation would poison the epoch.
#[must_use]
pub(crate) struct SerialLendHeld<'s, 'era> {
    lend: super::keeper_custody::SuspendedKeeper<'s, 'era>,
    cut: TicketIdentity,
}

impl StoppedSerialTicket {
    /// Test-only mint from copied facts. A non-test build does not have this
    /// function. Production mint is [`Self::from_lend`].
    #[cfg(test)]
    pub(crate) fn transport(identity: TicketIdentity) -> Self {
        Self {
            identity,
            _not_sync: PhantomData,
        }
    }

    // The L6.7 driver lends and accepts through these two, so they carry
    // no `expect(dead_code)` (see the note on `KeeperEra`'s driver methods).
    /// Cut the one ticket `lend` can issue.
    ///
    /// `lend` is consumed, so a second ticket cannot be cut from it. The
    /// returned loan still holds the stopped reservation.
    pub(crate) fn from_lend<'s, 'era>(
        lend: super::keeper_custody::SuspendedKeeper<'s, 'era>,
    ) -> (Self, SerialLendHeld<'s, 'era>) {
        let (owner, generation, heap, epoch) = lend.serial_facts();
        let ticket = Self {
            identity: TicketIdentity {
                owner,
                generation,
                heap,
                epoch,
            },
            _not_sync: PhantomData,
        };
        let cut = ticket.identity;
        (ticket, SerialLendHeld { lend, cut })
    }

    #[allow(dead_code)] // Dormant L6.3; L6.4 begin is the caller.
    pub(crate) fn identity(&self) -> TicketIdentity {
        self.identity
    }
}

impl<'s, 'era> SerialLendHeld<'s, 'era> {
    /// Hand the still-held lend to the existing return path.
    pub(crate) fn accept_return(
        self,
        returned: super::keeper_custody::ReturnedSerialTicket,
    ) -> Result<
        super::keeper_custody::RestoredKeeper<'s, 'era>,
        (Self, super::keeper_custody::ReturnedSerialTicket),
    > {
        if returned.identity() != self.cut {
            return Err((self, returned));
        }
        match self.lend.accept_return(returned) {
            Ok(restored) => Ok(restored),
            Err(failure) => {
                let _ = failure.complete_traced();
                panic!("the lend refused the ticket whose identity it cut");
            }
        }
    }
}

/// Names the ticket heap on the destination thread.
///
/// Construction does not install the heap. The loan is `!Send` and stays on
/// `LocalSerialOwner` until `end_serial` consumes it; the return envelope does
/// not carry it. Before reattaching the dormant Context, activation verifies
/// that the loaned heap matches the dormant Context's `HeapIdentity`.
#[derive(Debug)]
#[must_use]
struct LocalSerialHeapLoan {
    heap: HeapIdentity,
    _local: PhantomData<Rc<()>>,
}

impl LocalSerialHeapLoan {
    fn new(heap: HeapIdentity) -> Self {
        Self {
            heap,
            _local: PhantomData,
        }
    }

    fn verify_heap(&self, host_heap: HeapIdentity) -> Result<(), ReceiveReason> {
        if self.heap == host_heap {
            Ok(())
        } else {
            Err(ReceiveReason::WrongHeap)
        }
    }

    #[cfg(test)]
    fn heap(&self) -> HeapIdentity {
        self.heap
    }
}

/// One successful receive. `!Send`: it stays on the destination thread.
#[derive(Debug)]
#[must_use]
#[allow(dead_code)] // Dormant L6.3; L6.4 begin is the caller.
pub(crate) struct ReceivedSerial {
    identity: TicketIdentity,
    loan: LocalSerialHeapLoan,
    _local: PhantomData<Rc<()>>,
}

impl ReceivedSerial {
    #[allow(dead_code)] // Dormant L6.3; L6.4 begin is the caller.
    pub(crate) fn identity(&self) -> TicketIdentity {
        self.identity
    }

    /// The heap this receive loaned. Receive does not install it.
    #[cfg(test)]
    fn local_heap_loan(&self) -> Option<HeapIdentity> {
        Some(self.loan.heap())
    }
}

/// Why `receive_serial` gave the ticket back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Dormant L6.3; L6.4 begin is the caller.
pub(crate) enum ReceiveReason {
    WrongOwner,
    StaleGeneration,
    /// The heap word does not match.
    WrongHeap,
    /// The stop epoch does not match. The owner, generation, and heap did.
    Era,
    SecondReceive,
}

/// The ticket is inside the error. The seat is unchanged.
#[derive(Debug)]
#[must_use]
#[allow(dead_code)] // Dormant L6.3; L6.4 begin is the caller.
pub(crate) struct ReceiveRefusal {
    pub ticket: StoppedSerialTicket,
    pub reason: ReceiveReason,
}

/// The destination seat. `end_serial` is the only Occupied -> Empty transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Seat {
    Empty,
    Occupied,
}

/// Destination seat. Occupied only after every check has passed.
#[allow(dead_code)] // Dormant L6.3; L6.4 begin is the caller.
pub(crate) struct SerialDestination {
    expected: TicketIdentity,
    seat: Seat,
}

impl SerialDestination {
    /// Heap identity certified by this received serial turn seat (`E6-P.4`).
    pub(crate) fn heap_identity(&self) -> HeapIdentity {
        self.expected.heap
    }
}

/// The local guard around one received ticket. `finish` consumes it.
#[derive(Debug)]
#[must_use]
pub(crate) struct LocalSerialOwner {
    received: ReceivedSerial,
    _local: PhantomData<Rc<()>>,
}

/// What `end_serial` yields. `return_ticket` consumes it.
#[derive(Debug)]
#[must_use]
pub(crate) struct ReturnableSerialOwner {
    identity: TicketIdentity,
    _local: PhantomData<Rc<()>>,
}

#[allow(dead_code)] // L6.4 rows call this; production activation is the next edit.
impl ReceivedSerial {
    /// Enter the local guard. The received ticket cannot be used after this.
    pub(crate) fn guard(self) -> LocalSerialOwner {
        LocalSerialOwner {
            received: self,
            _local: PhantomData,
        }
    }
}

#[allow(dead_code)] // L6.4 rows call this; production activation is the next edit.
impl LocalSerialOwner {
    pub(crate) fn identity(&self) -> TicketIdentity {
        self.received.identity()
    }

    /// Verify that the received ticket's loaned heap matches the dormant
    /// host's heap identity before reattaching it. Does not install any heap.
    pub(crate) fn verify_host_heap(&self, host_heap: HeapIdentity) -> Result<(), ReceiveReason> {
        self.received.loan.verify_heap(host_heap)
    }

    /// End the turn and re-arm the seat. A mismatch returns this guard.
    pub(crate) fn finish(
        self,
        seat: &mut SerialDestination,
    ) -> Result<ReturnableSerialOwner, Self> {
        seat.end_serial(self)
    }
}

#[allow(dead_code)] // L6.4 rows call this; production activation is the next edit.
impl ReturnableSerialOwner {
    pub(crate) fn identity(&self) -> TicketIdentity {
        self.identity
    }

    /// The production return envelope. Consumes this owner.
    pub(crate) fn return_ticket(self) -> super::keeper_custody::ReturnedSerialTicket {
        super::keeper_custody::ReturnedSerialTicket::from_end(self)
    }
}

impl SerialDestination {
    #[allow(dead_code)] // Dormant L6.3; L6.4 begin is the caller.
    pub(crate) fn new(expected: TicketIdentity) -> Self {
        Self {
            expected,
            seat: Seat::Empty,
        }
    }

    #[allow(dead_code)] // Dormant L6.3; L6.4 begin is the caller.
    pub(crate) fn receive_serial(
        &mut self,
        ticket: StoppedSerialTicket,
    ) -> Result<ReceivedSerial, ReceiveRefusal> {
        let reason = if self.seat == Seat::Occupied {
            Some(ReceiveReason::SecondReceive)
        } else if ticket.identity.owner != self.expected.owner {
            Some(ReceiveReason::WrongOwner)
        } else if ticket.identity.generation != self.expected.generation {
            Some(ReceiveReason::StaleGeneration)
        } else if ticket.identity.heap != self.expected.heap {
            Some(ReceiveReason::WrongHeap)
        } else if ticket.identity.epoch != self.expected.epoch {
            Some(ReceiveReason::Era)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ReceiveRefusal { ticket, reason });
        }
        self.seat = Seat::Occupied;
        let heap = ticket.identity.heap;
        Ok(ReceivedSerial {
            identity: ticket.identity,
            loan: LocalSerialHeapLoan::new(heap),
            _local: PhantomData,
        })
    }

    #[allow(dead_code)] // finish is the caller; both wait for the activation edit.
    fn end_serial(
        &mut self,
        local: LocalSerialOwner,
    ) -> Result<ReturnableSerialOwner, LocalSerialOwner> {
        if self.seat != Seat::Occupied || local.identity() != self.expected {
            return Err(local);
        }
        self.seat = Seat::Empty;
        let identity = local.received.identity;
        // The return envelope keeps the identity. Dropping the guard ends the loan.
        drop(local);
        Ok(ReturnableSerialOwner {
            identity,
            _local: PhantomData,
        })
    }
}

static_assertions::assert_impl_all!(StoppedSerialTicket: Send);
static_assertions::assert_not_impl_any!(StoppedSerialTicket: Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SerialLendHeld<'static, 'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(LocalSerialHeapLoan: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ReceivedSerial: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(LocalSerialOwner: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ReturnableSerialOwner: Send, Sync, Clone, Copy);
static_assertions::assert_impl_all!(ReceiveRefusal: Send);

#[cfg(test)]
mod tests {
    use super::{ReceiveReason, SerialDestination, StoppedSerialTicket, TicketIdentity};
    use std::num::NonZeroU64;
    use std::sync::mpsc;
    use std::thread;

    fn heap(word: usize) -> neovm_value::tagged::gc::HeapIdentity {
        neovm_value::tagged::gc::HeapIdentity::from_legacy_word(word).expect("nonzero heap")
    }

    fn identity(owner: u64, generation: u64, heap_word: usize) -> TicketIdentity {
        identity_at(owner, generation, heap_word, 1)
    }

    fn identity_at(owner: u64, generation: u64, heap_word: usize, epoch: u64) -> TicketIdentity {
        TicketIdentity {
            owner,
            generation: NonZeroU64::new(generation).expect("nonzero generation"),
            heap: heap(heap_word),
            epoch: NonZeroU64::new(epoch).expect("nonzero epoch"),
        }
    }

    #[test]
    fn pentry_l63_wrong_owner_returns_the_ticket_and_receives_nothing() {
        let expected = identity(7, 1, 1);
        let mut seat = SerialDestination::new(expected);
        let foreign = StoppedSerialTicket::transport(identity(8, 1, 1));
        let refusal = seat.receive_serial(foreign).expect_err("wrong owner");
        assert_eq!(refusal.reason, ReceiveReason::WrongOwner);
        assert_eq!(refusal.ticket.identity(), identity(8, 1, 1));
        let received = seat
            .receive_serial(StoppedSerialTicket::transport(expected))
            .expect("the seat was not half-received");
        assert_eq!(received.identity(), expected);
    }

    #[test]
    fn pentry_l63_stale_generation_and_wrong_era_return_the_ticket() {
        let expected = identity(7, 2, 1);
        let mut seat = SerialDestination::new(expected);
        let stale = seat
            .receive_serial(StoppedSerialTicket::transport(identity(7, 1, 1)))
            .expect_err("stale generation");
        assert_eq!(stale.reason, ReceiveReason::StaleGeneration);
        assert_eq!(stale.ticket.identity().generation.get(), 1);
        let era = seat
            .receive_serial(StoppedSerialTicket::transport(identity(7, 2, 2)))
            .expect_err("wrong era");
        assert_eq!(era.reason, ReceiveReason::WrongHeap);
        assert_eq!(era.ticket.identity().heap, heap(2));
        assert!(
            seat.receive_serial(StoppedSerialTicket::transport(expected))
                .is_ok(),
            "neither refusal occupied the seat"
        );
    }

    #[test]
    fn pentry_l63_second_receive_returns_the_new_ticket() {
        let expected = identity(7, 1, 1);
        let mut seat = SerialDestination::new(expected);
        let first = seat
            .receive_serial(StoppedSerialTicket::transport(expected))
            .expect("first receive");
        let second = seat
            .receive_serial(StoppedSerialTicket::transport(expected))
            .expect_err("second receive");
        assert_eq!(second.reason, ReceiveReason::SecondReceive);
        assert_eq!(second.ticket.identity(), expected);
        assert_eq!(first.identity(), expected);
    }

    #[test]
    fn pentry_l63_receive_serial_hands_off_over_a_channel() {
        let expected = identity(7, 1, 1);
        let (ready_tx, ready_rx) = mpsc::channel();
        let (ticket_tx, ticket_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let destination = thread::spawn(move || {
            ready_tx.send(()).expect("destination is waiting");
            let ticket = ticket_rx.recv().expect("ticket");
            let mut seat = SerialDestination::new(expected);
            let received = seat.receive_serial(ticket).unwrap_or_else(|refusal| {
                panic!("handoff refused: {:?}", refusal.reason);
            });
            done_tx
                .send(received.identity())
                .expect("report the receive");
        });
        ready_rx.recv().expect("destination barrier");
        ticket_tx
            .send(StoppedSerialTicket::transport(expected))
            .expect("send the ticket");
        let got = done_rx.recv().expect("receive result");
        destination.join().expect("destination thread");
        assert_eq!(got, expected);
    }

    #[test]
    fn pentry_l64_wrong_epoch_is_era_and_returns_the_ticket() {
        let expected = identity_at(7, 2, 1, 4);
        let mut seat = SerialDestination::new(expected);
        let other_stop = identity_at(7, 2, 1, 3);
        let refusal = seat
            .receive_serial(StoppedSerialTicket::transport(other_stop))
            .expect_err("a different stop epoch");
        assert_eq!(refusal.reason, ReceiveReason::Era);
        assert_eq!(refusal.ticket.identity(), other_stop);
        let received = seat
            .receive_serial(StoppedSerialTicket::transport(expected))
            .expect("the seat was not half-received");
        assert_eq!(received.identity(), expected);
    }

    #[test]
    fn pentry_l64_end_serial_rearms_the_seat() {
        let expected = identity(7, 1, 1);
        let mut seat = SerialDestination::new(expected);
        let local = seat
            .receive_serial(StoppedSerialTicket::transport(expected))
            .expect("receive")
            .guard();
        let mut other = SerialDestination::new(identity(9, 1, 1));
        let local = match local.finish(&mut other) {
            Err(local) => local,
            Ok(_) => panic!("a different seat must not take the guard"),
        };
        let returned = local.finish(&mut seat).expect("re-arm this seat");
        assert_eq!(returned.identity(), expected);
        let _envelope = returned.return_ticket();
        let again = seat
            .receive_serial(StoppedSerialTicket::transport(expected))
            .expect("the seat is empty again");
        assert_eq!(again.identity(), expected);
    }

    #[test]
    fn pentry_l64_receive_serial_loans_the_heap_without_installing_it() {
        let installed = neovm_value::tagged::gc::tagged_heap_is_installed();
        let expected = identity(7, 1, 1);
        let mut seat = SerialDestination::new(expected);
        let received = seat
            .receive_serial(StoppedSerialTicket::transport(expected))
            .expect("the identity matches");
        assert_eq!(
            neovm_value::tagged::gc::tagged_heap_is_installed(),
            installed,
            "the local loan does not install the heap"
        );
        assert_eq!(
            received.local_heap_loan(),
            Some(expected.heap),
            "receive constructs the local serial heap loan"
        );
    }

    #[test]
    fn pentry_l64_local_heap_loan_refuses_mismatched_host_heap_without_installing() {
        let installed = neovm_value::tagged::gc::tagged_heap_is_installed();
        let expected = identity(7, 1, 1);
        let mut seat = SerialDestination::new(expected);
        let local = seat
            .receive_serial(StoppedSerialTicket::transport(expected))
            .expect("the identity matches")
            .guard();
        assert_eq!(
            local.verify_host_heap(heap(1)),
            Ok(()),
            "matching host heap passes verification"
        );
        assert_eq!(
            local.verify_host_heap(heap(2)),
            Err(ReceiveReason::WrongHeap),
            "mismatched host heap is refused with WrongHeap"
        );
        assert_eq!(
            neovm_value::tagged::gc::tagged_heap_is_installed(),
            installed,
            "loan verification does not install any heap"
        );
        let _returned = local.finish(&mut seat).expect("re-arm seat");
    }
}

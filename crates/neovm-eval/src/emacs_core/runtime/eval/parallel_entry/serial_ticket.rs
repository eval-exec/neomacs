//! L6.3: the dormant serial-ticket transport.
//!
//! The ticket carries the owner, the generation, and the era the destination
//! validates. It does not carry a Context, a collector, a slab, or a manager.
//! The Baton and the heap transfer stay the held keeper loan until L6.4
//! activates the owner's dormant Context. A refusal returns the ticket.
//! Nothing is half-received.

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
}

/// Send ticket from the keeper to the destination owner.
#[derive(Debug)]
#[must_use]
pub(crate) struct StoppedSerialTicket {
    identity: TicketIdentity,
    _not_sync: PhantomData<Cell<()>>,
}

impl StoppedSerialTicket {
    pub(crate) fn transport(identity: TicketIdentity) -> Self {
        Self {
            identity,
            _not_sync: PhantomData,
        }
    }

    pub(crate) fn identity(&self) -> TicketIdentity {
        self.identity
    }
}

/// One successful receive. `!Send`: it stays on the destination thread.
#[derive(Debug)]
#[must_use]
pub(crate) struct ReceivedSerial {
    identity: TicketIdentity,
    _local: PhantomData<Rc<()>>,
}

impl ReceivedSerial {
    pub(crate) fn identity(&self) -> TicketIdentity {
        self.identity
    }
}

/// Why `receive_serial` gave the ticket back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReceiveReason {
    WrongOwner,
    StaleGeneration,
    Era,
    SecondReceive,
}

/// The ticket is inside the error. The seat is unchanged.
#[derive(Debug)]
#[must_use]
pub(crate) struct ReceiveRefusal {
    pub ticket: StoppedSerialTicket,
    pub reason: ReceiveReason,
}

/// Destination seat. Occupied only after every check has passed.
pub(crate) struct SerialDestination {
    expected: TicketIdentity,
    occupied: bool,
}

impl SerialDestination {
    pub(crate) fn new(expected: TicketIdentity) -> Self {
        Self {
            expected,
            occupied: false,
        }
    }

    pub(crate) fn receive_serial(
        &mut self,
        ticket: StoppedSerialTicket,
    ) -> Result<ReceivedSerial, ReceiveRefusal> {
        let reason = if self.occupied {
            Some(ReceiveReason::SecondReceive)
        } else if ticket.identity.owner != self.expected.owner {
            Some(ReceiveReason::WrongOwner)
        } else if ticket.identity.generation != self.expected.generation {
            Some(ReceiveReason::StaleGeneration)
        } else if ticket.identity.heap != self.expected.heap {
            Some(ReceiveReason::Era)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(ReceiveRefusal { ticket, reason });
        }
        self.occupied = true;
        Ok(ReceivedSerial {
            identity: ticket.identity,
            _local: PhantomData,
        })
    }
}

static_assertions::assert_impl_all!(StoppedSerialTicket: Send);
static_assertions::assert_not_impl_any!(StoppedSerialTicket: Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(ReceivedSerial: Send, Sync, Clone, Copy);
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
        TicketIdentity {
            owner,
            generation: NonZeroU64::new(generation).expect("nonzero generation"),
            heap: heap(heap_word),
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
        assert_eq!(era.reason, ReceiveReason::Era);
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
}

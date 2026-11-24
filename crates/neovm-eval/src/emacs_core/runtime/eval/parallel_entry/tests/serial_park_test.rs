//! E5.2 owner-side park/resume over one ordered port. These rows use a
//! single-thread port that mints test tickets; the real-keeper rows (a stop,
//! a lend, a second owner) drive the same functions from a keeper thread.

use super::super::keeper_custody::ReturnedSerialTicket;
use super::super::serial_detach::HostRefusal;
use super::super::serial_ticket::{
    LocalSerialOwner, ReceiveReason, StoppedSerialTicket, TicketIdentity,
};
use super::super::serial_turn::{OwnerSerialState, TurnRefusal};
use super::{
    Grant, KeeperPort, ParkedSerial, PortRefusal, SerialHome, TurnStartRefusal, begin_turn,
    verify_serial_host_heap,
};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use std::convert::Infallible;
use std::num::NonZeroU64;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn names_no_heap() -> bool {
    !neovm_value::tagged::gc::tagged_heap_is_installed()
}

/// What the port saw, in order.
#[derive(Debug, PartialEq, Eq)]
enum Seen {
    GiveBack(u64),
    Request(u64),
    Refuse(ReceiveReason),
}

/// One way the next grant's ticket differs from the grant.
#[derive(Clone, Copy)]
enum Fault {
    /// Cut in a later stop than the grant names.
    Epoch,
    /// Cut from the owner's previous publication.
    Generation,
    /// Cut for another owner.
    Owner(u64),
}

/// One owner's ordered port. Each request publishes again and is granted at
/// the next stop epoch.
struct FakePort {
    home: SerialHome,
    generation: u64,
    epoch: u64,
    seen: Vec<Seen>,
    fault: Option<Fault>,
    refuse_next: Option<PortRefusal>,
}

impl FakePort {
    fn new(home: SerialHome) -> Self {
        Self {
            home,
            generation: 0,
            epoch: 0,
            seen: Vec::new(),
            fault: None,
            refuse_next: None,
        }
    }
}

fn nonzero(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).expect("counters start at 1")
}

impl KeeperPort for FakePort {
    type LendGuard = ();

    fn give_back(&mut self, returned: ReturnedSerialTicket) {
        self.seen
            .push(Seen::GiveBack(returned.identity().epoch.get()));
    }

    fn request_turn(&mut self) -> Result<(Grant, ()), PortRefusal> {
        if let Some(refusal) = self.refuse_next.take() {
            return Err(refusal);
        }
        self.generation += 1;
        self.epoch += 1;
        self.seen.push(Seen::Request(self.epoch));
        let generation = nonzero(self.generation);
        let epoch = nonzero(self.epoch);
        let mut cut = TicketIdentity {
            owner: self.home.owner,
            generation,
            heap: self.home.heap,
            epoch,
        };
        match self.fault.take() {
            None => {}
            Some(Fault::Epoch) => cut.epoch = nonzero(self.epoch + 1),
            Some(Fault::Generation) => cut.generation = nonzero(self.generation - 1),
            Some(Fault::Owner(owner)) => cut.owner = owner,
        }
        Ok((
            Grant {
                ticket: StoppedSerialTicket::transport(cut),
                generation,
                epoch,
            },
            (),
        ))
    }

    fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason) {
        drop(ticket);
        self.seen.push(Seen::Refuse(reason));
    }
}

type State = OwnerSerialState<Context, LocalSerialOwner>;

/// An owner built on this thread and kept dormant, with a binding to read
/// back after a resume.
fn dormant(owner: u64) -> (State, FakePort) {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    eval(&mut ev, "(progn (defvar sp-state 0) (setq sp-state 1))");
    let port = FakePort::new(SerialHome {
        owner,
        heap: ev.tagged_heap.heap_identity(),
    });
    let state = OwnerSerialState::new(owner, Box::new(ev))
        .ok()
        .expect("a freshly built Context detaches");
    (state, port)
}

/// The standalone owners need no heap loan.
fn standalone(_: &mut LocalSerialOwner) -> Result<(), Infallible> {
    Ok(())
}

/// Begin a turn on the port's first grant and park it on WAIT.
fn parked<'t, W>(
    state: &'t mut State,
    port: &mut FakePort,
    wait: W,
) -> ParkedSerial<'t, Context, W> {
    let grant = port
        .request_turn()
        .ok()
        .expect("the first request is granted");
    let home = port.home;
    begin_turn(state, home, grant, port, standalone)
        .ok()
        .expect("the first grant begins")
        .park(wait, port)
        .ok()
        .expect("the host detaches")
}

/// Resume expecting REASON; the turn comes back parked.
fn refused<'t, W>(
    parked: ParkedSerial<'t, Context, W>,
    port: &mut FakePort,
    reason: ReceiveReason,
) -> ParkedSerial<'t, Context, W> {
    let (parked, why) = match parked.resume(port, standalone) {
        Ok(_) => panic!("a ticket refused with {reason:?} must not resume the turn"),
        Err(refused) => refused,
    };
    assert_eq!(why, TurnStartRefusal::Receive(reason));
    assert_eq!(
        port.seen.last(),
        Some(&Seen::Refuse(reason)),
        "the unusable ticket went back unopened"
    );
    assert!(names_no_heap(), "the refusal left the turn parked");
    parked
}

#[test]
fn e52_a_park_gives_the_ticket_back_and_a_resume_runs_on_a_later_grant() {
    let (mut state, mut port) = dormant(7);
    let grant = port.request_turn().ok().expect("granted");
    let home = port.home;
    let mut turn = begin_turn(&mut state, home, grant, &mut port, standalone)
        .ok()
        .expect("the first grant begins");
    assert_eq!(turn.with_world(|ev| eval(ev, "(setq sp-state 2)")), "2");

    let parked: ParkedSerial<'_, Context, &str> = turn
        .park("mutex", &mut port)
        .ok()
        .expect("the host detaches");
    assert_eq!(
        port.seen,
        [Seen::Request(1), Seen::GiveBack(1)],
        "the ticket went back before the park returned"
    );
    assert!(
        names_no_heap(),
        "a parked turn leaves no heap on this thread"
    );
    assert_eq!(*parked.wait(), "mutex");

    let mut turn = parked
        .resume(&mut port, standalone)
        .ok()
        .expect("the next grant resumes");
    assert_eq!(
        turn.with_world(|ev| eval(ev, "sp-state")),
        "2",
        "the resumed turn reads the state the parked turn left"
    );
    turn.end(&mut port).ok().expect("the host detaches");
    assert_eq!(
        port.seen,
        [
            Seen::Request(1),
            Seen::GiveBack(1),
            Seen::Request(2),
            Seen::GiveBack(2),
        ],
        "return and request alternate on the one port, each at its own epoch"
    );
    assert!(names_no_heap());
}

#[test]
fn e52_a_ticket_from_another_stop_is_refused_and_the_turn_stays_parked() {
    let (mut state, mut port) = dormant(7);
    let parked = parked(&mut state, &mut port, "join");
    port.fault = Some(Fault::Epoch);
    let parked = refused(parked, &mut port, ReceiveReason::Era);
    let turn = parked
        .resume(&mut port, standalone)
        .ok()
        .expect("the next grant resumes");
    turn.end(&mut port).ok().expect("the host detaches");
}

#[test]
fn e52_a_ticket_from_an_older_publication_is_refused() {
    let (mut state, mut port) = dormant(7);
    let parked = parked(&mut state, &mut port, "join");
    port.fault = Some(Fault::Generation);
    let parked = refused(parked, &mut port, ReceiveReason::StaleGeneration);
    let turn = parked
        .resume(&mut port, standalone)
        .ok()
        .expect("the next grant resumes");
    turn.end(&mut port).ok().expect("the host detaches");
}

#[test]
fn e52_another_owners_ticket_is_refused_and_the_turn_stays_parked() {
    let (mut state, mut port) = dormant(7);
    let parked = parked(&mut state, &mut port, "mutex");
    port.fault = Some(Fault::Owner(8));
    let parked = refused(parked, &mut port, ReceiveReason::WrongOwner);
    assert_eq!(*parked.wait(), "mutex", "the parked turn came back too");
    let mut turn = parked
        .resume(&mut port, standalone)
        .ok()
        .expect("its own ticket resumes");
    assert_eq!(turn.with_world(|ev| eval(ev, "sp-state")), "1");
    turn.end(&mut port).ok().expect("the host detaches");
}

#[test]
fn e52_a_refused_request_keeps_the_turn_parked() {
    let (mut state, mut port) = dormant(7);
    let parked = parked(&mut state, &mut port, "yield");
    port.refuse_next = Some(PortRefusal::Poisoned);
    let (parked, why) = match parked.resume(&mut port, standalone) {
        Ok(_) => panic!("a poisoned era grants nothing"),
        Err(refused) => refused,
    };
    assert_eq!(why, TurnStartRefusal::Port(PortRefusal::Poisoned));
    assert_eq!(*parked.wait(), "yield", "the parked turn keeps its wait");
    let turn = parked
        .resume(&mut port, standalone)
        .ok()
        .expect("a later grant resumes");
    turn.end(&mut port).ok().expect("the host detaches");
}

#[test]
fn e52_a_refused_activation_gives_the_received_ticket_back() {
    let (mut state, mut port) = dormant(7);
    let parked = parked(&mut state, &mut port, "mutex");
    let (parked, why) = match parked.resume(&mut port, |_| Err("no loan")) {
        Ok(_) => panic!("a refused activation must not resume the turn"),
        Err(refused) => refused,
    };
    assert_eq!(why, TurnStartRefusal::Activate("no loan"));
    assert_eq!(
        port.seen.last(),
        Some(&Seen::GiveBack(2)),
        "the received ticket went back through its seat"
    );
    assert!(names_no_heap(), "the refusal left the turn parked");
    let turn = parked
        .resume(&mut port, standalone)
        .ok()
        .expect("a later grant resumes");
    turn.end(&mut port).ok().expect("the host detaches");
}

#[test]
fn e52_a_resume_while_another_owners_turn_is_live_gives_the_ticket_back() {
    let (mut first, mut port) = dormant(7);
    let (mut second, mut second_port) = dormant(8);
    let parked = parked(&mut first, &mut port, "mutex");

    let grant = second_port.request_turn().ok().expect("granted");
    let second_home = second_port.home;
    let other = begin_turn(
        &mut second,
        second_home,
        grant,
        &mut second_port,
        standalone,
    )
    .ok()
    .expect("the thread is free while the first turn is parked");
    let (parked, why) = match parked.resume(&mut port, standalone) {
        Ok(_) => panic!("one live turn per thread"),
        Err(refused) => refused,
    };
    assert_eq!(
        why,
        TurnStartRefusal::Turn(TurnRefusal::Host(HostRefusal::HeapInstalled))
    );
    assert_eq!(
        port.seen.last(),
        Some(&Seen::GiveBack(2)),
        "the received ticket went back to the keeper"
    );
    other.end(&mut second_port).ok().expect("the host detaches");

    let mut turn = parked
        .resume(&mut port, standalone)
        .ok()
        .expect("the thread is free");
    assert_eq!(turn.with_world(|ev| eval(ev, "sp-state")), "1");
    turn.end(&mut port).ok().expect("the host detaches");
}

#[test]
fn l64_activation_closure_verifies_host_heap_on_begin_and_resume() {
    let (mut state, mut port) = dormant(7);
    let real_heap = port.home.heap;
    let candidate =
        neovm_value::tagged::gc::HeapIdentity::from_legacy_word(1).expect("nonzero foreign heap");
    let foreign_heap = if candidate == real_heap {
        neovm_value::tagged::gc::HeapIdentity::from_legacy_word(2).expect("nonzero foreign heap")
    } else {
        candidate
    };
    assert_ne!(foreign_heap, real_heap);

    // Mismatch at begin_turn: verify_serial_host_heap refuses with WrongHeap,
    // gives the ticket back through its seat, and never installs a heap.
    let grant = port.request_turn().ok().expect("grant 1");
    let home = port.home;
    let why = match begin_turn(
        &mut state,
        home,
        grant,
        &mut port,
        verify_serial_host_heap(foreign_heap),
    ) {
        Ok(_) => panic!("mismatched host heap must be refused in activation"),
        Err(refusal) => refusal,
    };
    assert_eq!(why, TurnStartRefusal::Activate(ReceiveReason::WrongHeap));
    assert_eq!(
        port.seen.last(),
        Some(&Seen::GiveBack(1)),
        "refused activation returns the ticket through its seat"
    );
    assert!(names_no_heap(), "refused activation leaves TLS down");

    // Matching host heap enters the turn, parks, refuses a foreign host heap on
    // resume, and then resumes cleanly with the real host heap.
    let grant = port.request_turn().ok().expect("grant 2");
    let mut turn = begin_turn(
        &mut state,
        home,
        grant,
        &mut port,
        verify_serial_host_heap(real_heap),
    )
    .ok()
    .expect("matching host heap activates the turn");
    assert_eq!(turn.with_world(|ev| eval(ev, "sp-state")), "1");
    let parked = turn.park("condvar", &mut port).ok().expect("park");
    assert!(names_no_heap(), "park takes the heap back down");

    let (parked, why) = match parked.resume(&mut port, verify_serial_host_heap(foreign_heap)) {
        Ok(_) => panic!("mismatched host heap must be refused on resume"),
        Err(refused) => refused,
    };
    assert_eq!(why, TurnStartRefusal::Activate(ReceiveReason::WrongHeap));
    assert_eq!(
        port.seen.last(),
        Some(&Seen::GiveBack(3)),
        "refused resume activation returns the ticket through its seat"
    );
    assert!(names_no_heap(), "refused resume leaves TLS down");

    let mut turn = parked
        .resume(&mut port, verify_serial_host_heap(real_heap))
        .ok()
        .expect("matching host heap resumes the parked turn");
    assert_eq!(turn.with_world(|ev| eval(ev, "sp-state")), "1");
    turn.end(&mut port).ok().expect("the host detaches");
    assert!(names_no_heap(), "end takes the heap back down");
}

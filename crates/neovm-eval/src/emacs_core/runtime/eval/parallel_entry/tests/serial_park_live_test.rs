//! E5.2 and L6.7 over a real era (E5.2-PARK-RESUME §4). Owners run on
//! their own threads with standalone Contexts and drive `serial_park`
//! through the product port; the keeper service thread runs the product
//! driver (L6.7a), which commits each request in the ring, stops, grants
//! the head, lends, accepts the ticket back, and retires the entry (C3:
//! E5.2's rows run here, not against a test keeper). R7 (a collection at
//! the park) waits on L6.5's session.

use super::super::keeper_custody::{ReturnedSerialTicket, StoppedKeeper};
use super::super::owned_source::era_fixture::{
    KeeperEra, KeeperPanicGuard, activate, close, finish_fixture, keeper_serve, ready, retire,
    retry_native,
};
use super::super::serial_ticket::{LocalSerialOwner, ReceiveReason, StoppedSerialTicket};
use super::super::serial_turn::OwnerSerialState;
use super::{Grant, KeeperPort, PortRefusal, SerialHome, begin_turn};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::intern::intern;
use neovm_value::tagged::gc::heap_transfer::{PublishedOwner, with_stop_scope};
use neovm_value::tagged::gc::{
    AllStopped, AllStoppedFailure, Mutator, MutatorPermit, RendezvousError, StopPending,
};
use neovm_value::value::Value;
use std::convert::Infallible;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Every wait in these rows is bounded, so a lost ticket is a red row and
/// not a hang.
const BOUND: Duration = Duration::from_secs(30);

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

type State = OwnerSerialState<Context, LocalSerialOwner>;

/// The standalone owners need no heap loan.
fn standalone(_: &mut LocalSerialOwner) -> Result<(), Infallible> {
    Ok(())
}

/// What a parked turn of these rows waits on.
#[derive(Debug, PartialEq, Eq)]
struct Wake(&'static str);

/// Run BODY as the keeper of a fresh era on its own service thread, then
/// return the era and tear the fixture down.
fn with_live_era(
    label: &'static str,
    body: impl for<'k, 'service, 'heap> FnOnce(&'k KeeperEra<'service, 'heap>) + Send + 'static,
) {
    let mut source = ready();
    let private = Value::string(label);
    source.lexenv = private;
    let (shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |keeper| body(keeper));
        match outcome {
            Ok((end, ())) => tx.send(Ok(end)).expect("send keeper end"),
            Err(failure) => tx.send(Err(failure.notice())).expect("send keeper notice"),
        }
    });
    let outcome = rx.recv().expect("keeper outcome");
    service.join().expect("keeper service thread");
    let end = outcome.unwrap_or_else(|notice| panic!("{label}: era refused: {notice:?}"));
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
}

/// Run BODY as the keeper of a fresh era that BODY leaves poisoned, and
/// return what it reports. The era's refused finish is the expected end.
fn with_poisoned_era<R: Send + 'static>(
    label: &'static str,
    body: impl for<'k, 'service, 'heap> FnOnce(&'k KeeperEra<'service, 'heap>) -> R + Send + 'static,
) -> R {
    let mut source = ready();
    source.lexenv = Value::string(label);
    let (_shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |era| {
            tx.send(body(era)).expect("send the outcome");
        });
        drop(outcome);
    });
    // Bounded, so a driver that never ends fails here instead of hanging.
    let report = rx.recv_timeout(BOUND).expect("the keeper outcome");
    service.join().expect("keeper service thread");
    report
}

/// A running owner that counts its own progress and acks every stop.
fn run_peer(mut owner: Mutator<'_>, progress: &AtomicU64, done: &AtomicBool) {
    while !done.load(Ordering::Acquire) {
        match owner.wait_request_for(Duration::from_millis(2)) {
            Ok(_epoch) => {
                retry_native("ack the stop", || {
                    owner.publish_native_owner(&[]).map(|_| ())
                });
                retry_native("run after the stop", || owner.leave_native().map(|_| ()));
            }
            Err(RendezvousError::Busy) => {
                progress.fetch_add(1, Ordering::AcqRel);
            }
            Err(reason) => panic!("the running peer: {reason:?}"),
        }
    }
    retire(owner);
}

const O: u64 = 7201;
const B: u64 = 7202;
const P: u64 = 7203;

// ---------------------------------------------------------------------------
// L6.7a: the same owners against the product keeper driver and port.
// ---------------------------------------------------------------------------

use super::super::keeper_driver::{
    Breach, ChannelLend, ChannelPort, CollectRefusal, Driver, DriverEnd, GrantMsg, KeeperMsg,
    LendDrop,
};

/// What one owner thread takes for the driver rows.
struct DriverKit<'era> {
    permit: MutatorPermit<'era>,
    home: SerialHome,
    inbox: mpsc::Sender<KeeperMsg>,
    grants: mpsc::Receiver<GrantMsg>,
}

/// OWNER's standalone Context, kept dormant.
fn dormant_state(owner: u64) -> State {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    eval(
        &mut ev,
        "(progn (defvar sp-live 0) (setq sp-live 1) (defvar sp-bound nil))",
    );
    OwnerSerialState::new(owner, Box::new(ev))
        .ok()
        .expect("a freshly built Context detaches")
}

impl<'era> DriverKit<'era> {
    /// The owner's standalone Context, kept dormant, and its product port.
    fn start(self) -> (State, SerialHome, ChannelPort<'era>) {
        let state = dormant_state(self.home.owner);
        let port = ChannelPort::new(
            self.home.owner,
            activate(self.permit, self.home.heap, self.home.owner),
            self.inbox,
            self.grants,
        );
        (state, self.home, port)
    }

    /// Run SCRIPT, then exit and retire on a live era.
    fn run<R>(self, script: impl FnOnce(&mut State, SerialHome, &mut ChannelPort<'era>) -> R) -> R {
        let (mut state, home, mut port) = self.start();
        let out = script(&mut state, home, &mut port);
        retire(port.exit());
        out
    }
}

/// Enroll OWNER with the driver on the keeper thread; the permit crosses.
fn enroll_driver<'k>(
    era: &'k KeeperEra<'_, '_>,
    driver: &mut Driver<'k, '_, '_>,
    inbox: &mpsc::Sender<KeeperMsg>,
    owner: u64,
) -> DriverKit<'k> {
    let permit = era.register(owner, 0).expect("register the owner");
    let (grants_tx, grants) = mpsc::sync_channel(1);
    driver.enroll(owner, permit.key(), grants_tx);
    DriverKit {
        permit,
        home: SerialHome {
            owner,
            heap: era.heap_identity(),
        },
        inbox: inbox.clone(),
        grants,
    }
}

/// Request a turn on the product port and run FORM in it.
fn driver_turn<'t>(
    state: &'t mut State,
    home: SerialHome,
    port: &mut ChannelPort<'_>,
    form: &str,
) -> super::LiveSerial<'t, Context, ChannelLend> {
    let granted = port
        .request_turn()
        .ok()
        .expect("the driver grants the request");
    let mut turn = begin_turn(state, home, granted, port, standalone)
        .ok()
        .expect("the grant begins");
    turn.with_world(|ev| eval(ev, form));
    turn
}

/// What an owner sent through its port, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PortCall {
    /// A request, granted in the stop of this epoch.
    Granted(NonZeroU64),
    GiveBack,
    Refuse,
}

/// The product port, recording every call and the stop each grant was cut
/// in, so a row can see where a resume ran and that a parked owner sends
/// nothing (C3, lead #14072).
struct EpochPort<'p, 'era> {
    port: &'p mut ChannelPort<'era>,
    calls: Vec<PortCall>,
}

impl<'p, 'era> EpochPort<'p, 'era> {
    fn new(port: &'p mut ChannelPort<'era>) -> Self {
        Self {
            port,
            calls: Vec::new(),
        }
    }

    /// The stop epoch of each grant, in order.
    fn grants(&self) -> Vec<NonZeroU64> {
        self.calls
            .iter()
            .filter_map(|call| match call {
                PortCall::Granted(epoch) => Some(*epoch),
                PortCall::GiveBack | PortCall::Refuse => None,
            })
            .collect()
    }
}

impl KeeperPort for EpochPort<'_, '_> {
    type LendGuard = ChannelLend;

    fn give_back(&mut self, returned: ReturnedSerialTicket) {
        self.calls.push(PortCall::GiveBack);
        self.port.give_back(returned);
    }

    fn request_turn(&mut self) -> Result<(Grant, ChannelLend), PortRefusal> {
        let granted = self.port.request_turn()?;
        self.calls.push(PortCall::Granted(granted.0.epoch));
        Ok(granted)
    }

    fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason) {
        self.calls.push(PortCall::Refuse);
        self.port.refuse(ticket, reason);
    }
}

/// C3 + D-1. Two owners, one turn each: the driver lends both through its
/// own stops and drains when both have exited.
#[test]
fn l67a_driver_lends_each_owner_a_turn_and_drains() {
    with_live_era("l67a-drain", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let o = enroll_driver(era, &mut driver, &inbox, O);
        let b = enroll_driver(era, &mut driver, &inbox, B);
        drop(inbox);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let first = scope.spawn(move || {
                o.run(|state, home, port| {
                    let mut turn = driver_turn(state, home, port, "(setq sp-live 31)");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            let second = scope.spawn(move || {
                b.run(|state, home, port| {
                    let mut turn = driver_turn(state, home, port, "(setq sp-live 32)");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            assert_eq!(first.join().expect("O"), "31");
            assert_eq!(second.join().expect("B"), "32");
        });
    });
}

/// C3 (R4 on the product port). O parks and its ticket goes back; it
/// re-requests at once and resumes in a later stop with its state intact.
#[test]
fn l67a_driver_resumes_a_parked_turn_in_a_later_stop() {
    with_live_era("l67a-park", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let o = enroll_driver(era, &mut driver, &inbox, O);
        drop(inbox);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                o.run(|state, home, port| {
                    let mut port = EpochPort::new(port);
                    let granted = port.request_turn().ok().expect("O is granted");
                    let mut turn = begin_turn(state, home, granted, &mut port, standalone)
                        .ok()
                        .expect("the grant begins");
                    turn.with_world(|ev| eval(ev, "(setq sp-live 33)"));
                    let parked = turn.park(Wake("fired"), &mut port).ok().expect("parks");
                    let mut turn = parked
                        .resume(&mut port, standalone)
                        .ok()
                        .expect("the next grant resumes");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(&mut port).ok().expect("the host detaches");
                    (seen, port.grants())
                })
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            let (seen, grants) = owner.join().expect("O");
            assert_eq!(seen, "33");
            assert_eq!(grants.len(), 2);
            assert!(
                grants[1] > grants[0],
                "O resumes in a later stop than the one it parked in: {grants:?}"
            );
        });
    });
}

/// C3 (E5.2 R1 + R2 + R3 on the product driver). O and B are committed in
/// that order while the stop waits on the lagging owner L, so both are in
/// stop S1. O parks inside S1 and its ticket comes back there; the keeper
/// lends B in the same stop. S1 completes while O is parked: the running
/// peer P, which advances only while no stop holds it, makes progress. O
/// then wakes, re-requests on the same port, and resumes (necessarily in a
/// later stop) with its turn state intact: the dynamic binding it made
/// before the park, its match data, and the global it set.
#[test]
fn l67a_c3_a_parked_turn_gives_its_stop_back_and_resumes_in_a_later_one() {
    with_live_era("l67a-c3-r1", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let (notices_tx, notices) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx).observe_pending(
            Duration::from_millis(5),
            move |_pending: &StopPending, queued| {
                let _ = notices_tx.send(queued);
            },
        );
        let o = enroll_driver(era, &mut driver, &inbox, O);
        let b = enroll_driver(era, &mut driver, &inbox, B);
        let slow = era.register(L, 0).expect("register L");
        let peer = era.register(P, 0).expect("register P");
        let heap = era.heap_identity();
        drop(inbox);
        let (l_running_tx, l_running) = mpsc::channel::<()>();
        let (l_go_tx, l_go) = mpsc::channel::<()>();
        let (b_running_tx, b_running) = mpsc::channel::<()>();
        let (b_go_tx, b_go) = mpsc::channel::<()>();
        let (parked_tx, parked) = mpsc::channel::<NonZeroU64>();
        let (b_done_tx, b_done) = mpsc::channel::<()>();
        let (wake_tx, wake) = mpsc::channel::<()>();
        let progress = AtomicU64::new(0);
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let lagging = scope.spawn(move || lagging_owner(slow, heap, L, l_running_tx, l_go));
            l_running.recv_timeout(BOUND).expect("L is running");
            let (progress, done) = (&progress, &done);
            let peer = scope.spawn(move || run_peer(activate(peer, heap, P), progress, done));
            let parked_owner = scope.spawn(move || {
                o.run(|state, home, port| {
                    let mut port = EpochPort::new(port);
                    // B and P are running members before O's request starts
                    // the stop: a mutator activated during a stop is not in
                    // its frozen set and waits for it to end.
                    b_running.recv_timeout(BOUND).expect("B is running");
                    let deadline = Instant::now() + BOUND;
                    while progress.load(Ordering::Acquire) == 0 {
                        assert!(Instant::now() < deadline, "P is running");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    let (grant, lend) = port.request_turn().ok().expect("O is granted");
                    let s1 = grant.epoch;
                    let mut turn = begin_turn(state, home, (grant, lend), &mut port, standalone)
                        .ok()
                        .expect("O's grant begins");
                    turn.with_world(|ev| {
                        eval(
                            ev,
                            "(progn (setq sp-live 2) (string-match \"b+\" \"abbbc\"))",
                        )
                    });
                    let count = turn.with_world(|ev| {
                        let count = ev.specpdl.len();
                        ev.try_specbind(intern("sp-bound"), Value::T)
                            .expect("bind sp-bound");
                        count
                    });
                    let parked_turn = turn.park(Wake("mutex"), &mut port).ok().expect("O parks");
                    parked_tx.send(s1).expect("tell the conductor");
                    wake.recv_timeout(BOUND).expect("woken");
                    // Parked, O held nothing and asked for nothing: its port
                    // sent one request and the park's give-back, no more.
                    let while_parked = port.calls.clone();
                    let mut turn = parked_turn
                        .resume(&mut port, standalone)
                        .ok()
                        .expect("the later grant resumes");
                    let seen = turn.with_world(|ev| {
                        let seen = eval(ev, "(list sp-live sp-bound (match-end 0))");
                        ev.unbind_to(count);
                        (seen, eval(ev, "sp-bound"))
                    });
                    turn.end(&mut port).ok().expect("O's host detaches");
                    (s1, seen, while_parked, port.grants())
                })
            });
            let other = scope.spawn(move || {
                b.run(|state, home, port| {
                    b_running_tx.send(()).expect("B runs");
                    b_go.recv_timeout(BOUND).expect("B may request");
                    let (grant, lend) = port.request_turn().ok().expect("B is granted");
                    let epoch = grant.epoch;
                    let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                        .ok()
                        .expect("B's grant begins");
                    let seen = turn.with_world(|ev| eval(ev, "(setq sp-live 20)"));
                    turn.end(port).ok().expect("B's host detaches");
                    b_done_tx.send(()).expect("tell the conductor");
                    (epoch, seen)
                })
            });
            let conductor = scope.spawn(move || {
                // O's request waits on L; B is committed behind it before L
                // is let stop.
                loop {
                    match notices
                        .recv_timeout(BOUND)
                        .expect("a pending stop is reported")
                    {
                        1 => {
                            let _ = b_go_tx.send(());
                        }
                        2 => break,
                        _ => {}
                    }
                }
                l_go_tx.send(()).expect("tell L");
                let s1 = parked.recv_timeout(BOUND).expect("O parked");
                b_done.recv_timeout(BOUND).expect("B's turn ran");
                let before = progress.load(Ordering::Acquire);
                let deadline = Instant::now() + BOUND;
                while progress.load(Ordering::Acquire) < before + 8 {
                    assert!(Instant::now() < deadline, "the peer ran while O was parked");
                    std::thread::sleep(Duration::from_millis(1));
                }
                wake_tx.send(()).expect("wake O");
                s1
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            done.store(true, Ordering::Release);
            let s1 = conductor.join().expect("the conductor");
            let l_epoch = lagging.join().expect("L");
            let (o_epoch, seen, while_parked, grants) = parked_owner.join().expect("O");
            let (b_epoch, b_seen) = other.join().expect("B");
            peer.join().expect("P");
            assert_eq!(o_epoch, s1);
            assert_eq!(
                while_parked,
                [PortCall::Granted(s1), PortCall::GiveBack],
                "the parked owner holds nothing and asks for nothing"
            );
            assert_eq!(grants.len(), 2);
            assert!(grants[1] > s1, "O resumed in a later stop: {grants:?}");
            assert_eq!(l_epoch, s1, "S1 is the stop L met");
            assert_eq!(b_epoch, s1, "B was lent in the stop O parked in");
            assert_eq!(b_seen, "20");
            assert_eq!(
                seen,
                ("(2 t 4)".to_string(), "nil".to_string()),
                "O's binding, global and match data survived the park; unbinding \
                 after the resume restores the value the binding hid"
            );
        });
    });
}

/// C3 (E5.2 R5 on the product driver). While O is parked and not queued, B
/// is lent in a stop of its own. O is NativeStopped for the whole wait, so
/// that stop completes without O's ack. O then resumes in the stop after
/// it. (R4, an immediate wake, is `l67a_driver_resumes_a_parked_turn_in_a_later_stop`.)
#[test]
fn l67a_c3_a_stop_while_parked_needs_no_ack_from_the_parked_owner() {
    with_live_era("l67a-c3-r5", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let o = enroll_driver(era, &mut driver, &inbox, O);
        let b = enroll_driver(era, &mut driver, &inbox, B);
        drop(inbox);
        let (parked_tx, parked) = mpsc::channel::<NonZeroU64>();
        let (b_done_tx, b_done) = mpsc::channel::<NonZeroU64>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                o.run(|state, home, port| {
                    let mut port = EpochPort::new(port);
                    let (grant, lend) = port.request_turn().ok().expect("O is granted");
                    let s1 = grant.epoch;
                    let mut turn = begin_turn(state, home, (grant, lend), &mut port, standalone)
                        .ok()
                        .expect("O's grant begins");
                    turn.with_world(|ev| eval(ev, "(setq sp-live 5)"));
                    let parked_turn = turn.park(Wake("condvar"), &mut port).ok().expect("O parks");
                    parked_tx.send(s1).expect("tell B");
                    let s2 = b_done.recv_timeout(BOUND).expect("B's stop completed");
                    let mut turn = parked_turn
                        .resume(&mut port, standalone)
                        .ok()
                        .expect("the next grant resumes");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(&mut port).ok().expect("O's host detaches");
                    (s1, s2, seen, port.grants())
                })
            });
            let other = scope.spawn(move || {
                let s1 = parked.recv_timeout(BOUND).expect("O parked");
                b.run(|state, home, port| {
                    let (grant, lend) = port
                        .request_turn()
                        .ok()
                        .expect("B is granted while O is parked");
                    let s2 = grant.epoch;
                    let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                        .ok()
                        .expect("B's grant begins");
                    let seen = turn.with_world(|ev| eval(ev, "(setq sp-live 50)"));
                    turn.end(port).ok().expect("B's host detaches");
                    b_done_tx.send(s2).expect("tell O");
                    (s1, s2, seen)
                })
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            let (o_s1, s2, seen, grants) = owner.join().expect("O");
            let (b_s1, b_s2, b_seen) = other.join().expect("B");
            assert_eq!(o_s1, b_s1);
            assert_eq!(s2, b_s2);
            assert!(
                s2 > o_s1,
                "B's stop came after the one O parked in, and completed without O"
            );
            assert_eq!(grants.len(), 2);
            assert!(grants[1] > s2, "O resumes after B's stop: {grants:?}");
            assert_eq!((seen.as_str(), b_seen.as_str()), ("5", "50"));
        });
    });
}

/// C3 (E5.2 R6 on the product driver). The product keeper sends each ticket
/// to the owner it was cut for, so a refusal can only come from a broken
/// owner: the driver poisons (K7) and the refused ticket drops with the
/// stop. The owner side of R6, another owner's ticket coming back unopened
/// while the parked turn stays, is `serial_park_test`'s.
#[test]
fn l67a_c3_a_refused_ticket_poisons() {
    let end = with_poisoned_era("l67a-c3-r6", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let o = enroll_driver(era, &mut driver, &inbox, O);
        drop(inbox);
        std::thread::scope(|scope| {
            let refusing = scope.spawn(move || {
                let (_state, _home, mut raw, grants) = o.start_raw();
                let receipt = raw.publish();
                raw.request(receipt);
                let Ok(GrantMsg::Grant { ticket, .. }) = grants.recv_timeout(BOUND) else {
                    panic!("O is granted");
                };
                raw.port().refuse(ticket, ReceiveReason::WrongOwner);
                let told = matches!(grants.recv_timeout(BOUND), Ok(GrantMsg::Poisoned));
                drop(raw);
                told
            });
            let end = driver.run();
            (end, refusing.join().expect("O"))
        })
    });
    let (end, told) = end;
    assert_eq!(
        end,
        DriverEnd::Poisoned(Breach::Refused {
            owner: O,
            reason: ReceiveReason::WrongOwner,
        })
    );
    assert!(told, "O learns the era is poisoned");
}

/// D-9 (lead #13898). A Lisp signal ends only that turn, as GNU's
/// run_thread records it and keeps the world running (thread.c:818,
/// :753-758). The turn ends normally, nothing is poisoned, and the owner
/// queued behind is granted and completes.
#[test]
fn l67a_a_lisp_signal_ends_only_that_turn() {
    with_live_era("l67a-signal", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let o = enroll_driver(era, &mut driver, &inbox, O);
        let b = enroll_driver(era, &mut driver, &inbox, B);
        drop(inbox);
        let (begun_tx, begun_rx) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let signalling = scope.spawn(move || {
                o.run(|state, home, port| {
                    let granted = port.request_turn().ok().expect("granted");
                    let mut turn = begin_turn(state, home, granted, port, standalone)
                        .ok()
                        .expect("begins");
                    // B requests only now, so it is queued behind O's turn.
                    begun_tx.send(()).expect("tell B");
                    let error = turn.with_world(|ev| eval(ev, "(signal 'error '(\"boom\"))"));
                    turn.end(port).ok().expect("the turn ends normally");
                    error
                })
            });
            let after = scope.spawn(move || {
                begun_rx.recv_timeout(BOUND).expect("O's turn began");
                b.run(|state, home, port| {
                    let mut turn = driver_turn(state, home, port, "(setq sp-live 34)");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            assert_eq!(driver.run(), DriverEnd::Drained, "nothing was poisoned");
            assert!(
                signalling.join().expect("O").starts_with("ERR"),
                "the signal escaped the form"
            );
            assert_eq!(after.join().expect("B"), "34", "the next owner ran");
        });
    });
}

/// D-4 (K7). O unwinds from a Rust panic inside its turn while it holds the
/// ticket. Its lend guard reports the lost ticket; the driver poisons the
/// era, and the other owner's request is answered `Poisoned`.
#[test]
fn l67a_an_owner_unwinding_mid_turn_poisons_and_wakes_the_rest() {
    let mut source = ready();
    source.lexenv = Value::string("l67a-unwind");
    let (_shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |era| {
            let (inbox, inbox_rx) = mpsc::channel();
            let mut driver = Driver::new(era, inbox_rx);
            let o = enroll_driver(era, &mut driver, &inbox, O);
            let b = enroll_driver(era, &mut driver, &inbox, B);
            drop(inbox);
            let (begun_tx, begun_rx) = mpsc::channel::<()>();
            let (idle_tx, idle_rx) = mpsc::channel::<()>();
            std::thread::scope(|scope| {
                let unwinding = scope.spawn(move || {
                    let (mut state, home, mut port) = o.start();
                    // O asks only once B is NativeStopped and cannot hold O's
                    // stop open.
                    idle_rx.recv_timeout(BOUND).expect("B is idle");
                    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut turn =
                            driver_turn(&mut state, home, &mut port, "(setq sp-live 35)");
                        // B requests only once O holds the ticket.
                        begun_tx.send(()).expect("tell B");
                        turn.with_world(|_ev| panic!("an invariant break inside the turn"));
                    }));
                    assert!(caught.is_err(), "the turn unwound");
                    // The era is poisoned; the mutator is dropped, not retired.
                    drop(port.exit());
                });
                let queued = scope.spawn(move || {
                    let (_state, _home, mut port) = b.start();
                    // NativeStopped while it waits, so O's stop does not wait
                    // on it.
                    port.publish_idle().expect("B publishes");
                    idle_tx.send(()).expect("tell O");
                    begun_rx.recv_timeout(BOUND).expect("O holds the ticket");
                    let answer = port.request_turn().map(|_granted| ());
                    drop(port.exit());
                    answer
                });
                let end = driver.run();
                unwinding.join().expect("O's thread");
                let answer = queued.join().expect("B's thread");
                tx.send((end, answer)).expect("send the outcome");
            });
        });
        drop(outcome);
    });
    let (end, answer) = rx.recv().expect("the keeper outcome");
    service.join().expect("keeper service thread");
    assert_eq!(
        end,
        DriverEnd::Poisoned(Breach::Abandoned {
            owner: O,
            cause: LendDrop::Unwound,
        })
    );
    assert_eq!(
        answer,
        Err(PortRefusal::Poisoned),
        "B learns the era is poisoned"
    );
}

const L: u64 = 7204;

/// D-10 (K8 r2). Running owner L has not met the stop A's request started.
/// Each bounded attempt comes back pending and leaves the request standing:
/// the keeper serves its inbox, so D's later request joins the same stop,
/// and once L stops both turns are lent in that one epoch. Nothing is
/// poisoned and nobody is told to retry.
#[test]
fn l67a_a_slow_owner_delays_the_stop_and_poisons_nothing() {
    with_live_era("l67a-slow", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let (notices_tx, notices) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx).observe_pending(
            Duration::from_millis(5),
            move |pending: &StopPending, queued| {
                let _ = notices_tx.send((pending.clone(), queued));
            },
        );
        let a = enroll_driver(era, &mut driver, &inbox, O);
        let d = enroll_driver(era, &mut driver, &inbox, B);
        let slow = era.register(L, 0).expect("register the slow owner");
        let slow_key = slow.key();
        let heap = era.heap_identity();
        drop(inbox);
        let (d_go_tx, d_go) = mpsc::channel::<()>();
        let (l_go_tx, l_go) = mpsc::channel::<()>();
        let (l_running_tx, l_running) = mpsc::channel::<()>();
        let (d_running_tx, d_running) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let lagging = scope.spawn(move || {
                let mut owner = activate(slow, heap, L);
                l_running_tx.send(()).expect("L runs");
                // Running and not polling until D's request is committed.
                l_go.recv_timeout(BOUND).expect("L may poll");
                let epoch = retry_native("L sees the stop", || owner.wait_request());
                retry_native("L stops", || owner.publish_native_owner(&[]).map(|_| ()));
                retry_native("L runs again", || owner.leave_native().map(|_| ()));
                retire(owner);
                epoch
            });
            l_running.recv_timeout(BOUND).expect("L is running");
            let first = scope.spawn(move || {
                a.run(|state, home, port| {
                    // D runs before A's request starts the stop, so D is a
                    // running member of it.
                    d_running.recv_timeout(BOUND).expect("D is running");
                    let (grant, lend) = port.request_turn().ok().expect("A is granted");
                    let epoch = grant.epoch;
                    let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                        .ok()
                        .expect("A's grant begins");
                    let seen = turn.with_world(|ev| eval(ev, "(setq sp-live 41)"));
                    turn.end(port).ok().expect("A's host detaches");
                    (epoch, seen)
                })
            });
            let second = scope.spawn(move || {
                d.run(|state, home, port| {
                    d_running_tx.send(()).expect("D runs");
                    d_go.recv_timeout(BOUND).expect("D may request");
                    let (grant, lend) = port.request_turn().ok().expect("D is granted");
                    let epoch = grant.epoch;
                    let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                        .ok()
                        .expect("D's grant begins");
                    let seen = turn.with_world(|ev| eval(ev, "(setq sp-live 42)"));
                    turn.end(port).ok().expect("D's host detaches");
                    (epoch, seen)
                })
            });
            let conductor = scope.spawn(move || {
                let mut first_pending = None;
                loop {
                    let (pending, queued) = notices
                        .recv_timeout(BOUND)
                        .expect("a pending stop attempt is reported");
                    if queued == 1 && first_pending.is_none() {
                        first_pending = Some(pending);
                        d_go_tx.send(()).expect("tell D");
                    } else if queued == 2 {
                        l_go_tx.send(()).expect("tell L");
                        return (first_pending, pending);
                    }
                }
            });
            let end = driver.run();
            let (first_pending, both_queued) = conductor.join().expect("the conductor");
            let slow_epoch = lagging.join().expect("L");
            let (a_epoch, a_seen) = first.join().expect("A");
            let (d_epoch, d_seen) = second.join().expect("D");
            assert_eq!(end, DriverEnd::Drained, "nothing is poisoned");
            let first_pending = first_pending.expect("A's request waits on a pending stop");
            assert_eq!(
                first_pending.epoch(),
                Some(slow_epoch),
                "the pending stop is the one L later meets"
            );
            assert!(
                first_pending.waiting_on().contains(&slow_key),
                "the keeper names L: {first_pending:?}"
            );
            assert_eq!(
                both_queued.waiting_on(),
                &[slow_key],
                "D's request is committed while the stop waits on L only"
            );
            assert_eq!(a_epoch, slow_epoch, "A is lent in the standing stop");
            assert_eq!(d_epoch, slow_epoch, "D joins that same stop");
            assert_eq!((a_seen.as_str(), d_seen.as_str()), ("41", "42"));
        });
    });
}

const C: u64 = 7205;

/// A grant record: which owner, in which stop.
type Order = std::sync::Mutex<Vec<(u64, NonZeroU64)>>;

impl<'era> DriverKit<'era> {
    /// The owner's dormant Context, its activated mutator and its two
    /// channels, for a row that speaks the driver's protocol by hand.
    fn start_raw(self) -> (State, SerialHome, RawOwner<'era>, mpsc::Receiver<GrantMsg>) {
        let state = dormant_state(self.home.owner);
        let raw = RawOwner {
            owner: self.home.owner,
            mutator: activate(self.permit, self.home.heap, self.home.owner),
            inbox: self.inbox,
        };
        (state, self.home, raw, self.grants)
    }
}

/// An owner driven by hand, for what a ChannelPort never sends: a stale
/// receipt, an exit while lent, a request from an owner whose grant
/// channel is closed.
struct RawOwner<'era> {
    owner: u64,
    mutator: Mutator<'era>,
    inbox: mpsc::Sender<KeeperMsg>,
}

impl RawOwner<'_> {
    fn publish(&mut self) -> PublishedOwner {
        retry_native("publish", || self.mutator.publish_native_owner(&[]))
    }

    fn leave(&mut self) {
        retry_native("leave native", || self.mutator.leave_native().map(|_| ()));
    }

    fn request(&self, receipt: PublishedOwner) {
        self.inbox
            .send(KeeperMsg::Request {
                owner: self.owner,
                receipt,
            })
            .expect("the keeper is serving");
    }

    fn port(&self) -> RawPort {
        RawPort {
            owner: self.owner,
            inbox: self.inbox.clone(),
        }
    }
}

/// The port a raw owner's granted turn ends through.
struct RawPort {
    owner: u64,
    inbox: mpsc::Sender<KeeperMsg>,
}

impl KeeperPort for RawPort {
    type LendGuard = ();

    fn give_back(&mut self, returned: ReturnedSerialTicket) {
        self.inbox
            .send(KeeperMsg::GiveBack {
                owner: self.owner,
                returned,
            })
            .expect("the keeper takes the ticket back");
    }

    fn request_turn(&mut self) -> Result<(Grant, ()), PortRefusal> {
        panic!("a raw owner requests by hand")
    }

    fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason) {
        let _ = self.inbox.send(KeeperMsg::Refuse {
            owner: self.owner,
            ticket,
            reason,
        });
    }
}

/// Run FORM in the turn GRANT lends and hand the ticket back.
fn raw_turn(state: &mut State, home: SerialHome, port: &mut RawPort, grant: Grant, form: &str) {
    let mut turn = begin_turn(state, home, (grant, ()), port, standalone)
        .ok()
        .expect("the raw grant begins");
    turn.with_world(|ev| eval(ev, form));
    turn.end(port).ok().expect("the host detaches");
}

/// A running owner that does not poll until GO, then stops for the standing
/// stop, runs again and retires. Returns the epoch it stopped in.
fn lagging_owner(
    permit: MutatorPermit<'_>,
    heap: neovm_value::tagged::gc::HeapIdentity,
    owner: u64,
    running: mpsc::Sender<()>,
    go: mpsc::Receiver<()>,
) -> NonZeroU64 {
    let mut mutator = activate(permit, heap, owner);
    running.send(()).expect("the slow owner runs");
    go.recv_timeout(BOUND).expect("the slow owner may poll");
    let epoch = retry_native("see the stop", || mutator.wait_request());
    retry_native("stop", || mutator.publish_native_owner(&[]).map(|_| ()));
    retry_native("run again", || mutator.leave_native().map(|_| ()));
    retire(mutator);
    epoch
}

/// Report running, wait for GO, request on the product port, record the
/// grant in ORDER, run FORM, end, and exit.
fn requester<'era>(
    kit: DriverKit<'era>,
    running: mpsc::Sender<()>,
    go: mpsc::Receiver<()>,
    order: &Order,
    form: &str,
) {
    kit.run(|state, home, port| {
        running.send(()).expect("the requester runs");
        go.recv_timeout(BOUND).expect("the requester may ask");
        let (grant, lend) = port.request_turn().ok().expect("the request is granted");
        order.lock().expect("order").push((home.owner, grant.epoch));
        let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
            .ok()
            .expect("the grant begins");
        turn.with_world(|ev| eval(ev, form));
        turn.end(port).ok().expect("the host detaches");
    });
}

/// D-1 + K4 (lead #13966 C2). O, B and C are committed in that order while
/// the stop waits on L, and are lent in that order in one stop. L's own
/// request reaches the keeper's inbox only after that stop is taken: it is
/// outside the stop's snapshot and is lent in the next stop.
#[test]
fn l67a_driver_serves_its_snapshot_in_order_and_a_later_request_next_stop() {
    with_live_era("l67a-fifo", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let (relay_tx, relay_rx) = mpsc::channel::<KeeperMsg>();
        let (notices_tx, notices) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let mut held = false;
        let mut driver = Driver::new(era, inbox_rx).observe_pending(
            Duration::from_millis(5),
            move |_pending: &StopPending, queued| {
                let _ = notices_tx.send(queued);
                // Once the three are committed, hold the keeper here until
                // L's request is in its inbox.
                if queued == 3 && !held {
                    held = true;
                    release_rx
                        .recv_timeout(BOUND)
                        .expect("L's request is through");
                }
            },
        );
        let kits = [O, B, C, L].map(|owner| enroll_driver(era, &mut driver, &relay_tx, owner));
        drop(relay_tx);
        let order = Order::default();
        let (running_tx, running) = mpsc::channel::<()>();
        let (go, gates): (Vec<_>, Vec<_>) = (0..4).map(|_| mpsc::channel::<()>()).unzip();
        let (relayed_tx, relayed) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            // Forward every message in order; say when L's request is through.
            scope.spawn(move || {
                for message in relay_rx {
                    let from_l = matches!(message, KeeperMsg::Request { owner: L, .. });
                    if inbox.send(message).is_err() {
                        break;
                    }
                    if from_l {
                        let _ = relayed_tx.send(());
                    }
                }
            });
            let order = &order;
            for ((kit, gate), form) in kits.into_iter().zip(gates).zip([
                "(setq sp-live 51)",
                "(setq sp-live 52)",
                "(setq sp-live 53)",
                "(setq sp-live 54)",
            ]) {
                let running = running_tx.clone();
                scope.spawn(move || requester(kit, running, gate, order, form));
            }
            drop(running_tx);
            scope.spawn(move || {
                for _ in 0..4 {
                    running.recv_timeout(BOUND).expect("every owner runs");
                }
                go[0].send(()).expect("O asks");
                let mut next = 1;
                while next < 4 {
                    let queued = notices.recv_timeout(BOUND).expect("a pending attempt");
                    if queued == next {
                        go[next].send(()).expect("the next owner asks");
                        next += 1;
                    }
                }
                relayed.recv_timeout(BOUND).expect("L's request is relayed");
                release_tx.send(()).expect("release the keeper");
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
        });
        let order = order.into_inner().expect("order");
        let owners: Vec<u64> = order.iter().map(|(owner, _)| *owner).collect();
        assert_eq!(owners, [O, B, C, L], "ring order is grant order");
        let first = order[0].1;
        assert!(
            order[..3].iter().all(|(_, epoch)| *epoch == first),
            "O, B and C share one stop: {order:?}"
        );
        assert!(order[3].1 > first, "L waits for the next stop: {order:?}");
    });
}

/// D-3. A's queued receipt is stale when its stop comes: the keeper sends A
/// back to publish, retires its entry and lends nothing more in that stop,
/// so B, queued behind it, is not skipped ahead. B is lent in the next
/// stop, and A's fresh request after it.
#[test]
fn l67a_an_unready_head_is_sent_back_without_skipping_the_ring() {
    with_live_era("l67a-unready", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let (notices_tx, notices) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx).observe_pending(
            Duration::from_millis(5),
            move |_pending: &StopPending, queued| {
                let _ = notices_tx.send(queued);
            },
        );
        let a = enroll_driver(era, &mut driver, &inbox, O);
        let b = enroll_driver(era, &mut driver, &inbox, B);
        let slow = era.register(L, 0).expect("register the slow owner");
        let heap = era.heap_identity();
        drop(inbox);
        let order = Order::default();
        let (running_tx, running) = mpsc::channel::<()>();
        let (b_go_tx, b_go) = mpsc::channel::<()>();
        let (l_go_tx, l_go) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let l_running = running_tx.clone();
            let lagging = scope.spawn(move || lagging_owner(slow, heap, L, l_running, l_go));
            let b_running = running_tx.clone();
            let order = &order;
            scope.spawn(move || requester(b, b_running, b_go, order, "(setq sp-live 62)"));
            drop(running_tx);
            let first = scope.spawn(move || {
                for _ in 0..2 {
                    running.recv_timeout(BOUND).expect("B and L run");
                }
                let (mut state, home, mut raw, grants) = a.start_raw();
                // A requests under a receipt it then replaces.
                let stale = raw.publish();
                raw.leave();
                let fresh = raw.publish();
                raw.request(stale);
                let sent_back = matches!(grants.recv_timeout(BOUND), Ok(GrantMsg::Republish));
                let (_, generation, _) = fresh.serial_facts();
                raw.request(fresh);
                let Ok(GrantMsg::Grant { ticket, epoch }) = grants.recv_timeout(BOUND) else {
                    panic!("A's fresh request is granted");
                };
                order.lock().expect("order").push((O, epoch));
                let mut port = raw.port();
                raw_turn(
                    &mut state,
                    home,
                    &mut port,
                    Grant {
                        ticket,
                        generation,
                        epoch,
                    },
                    "(setq sp-live 61)",
                );
                raw.inbox
                    .send(KeeperMsg::Exit { owner: O })
                    .expect("A exits");
                retire(raw.mutator);
                sent_back
            });
            scope.spawn(move || {
                let mut b_asked = false;
                loop {
                    let queued = notices.recv_timeout(BOUND).expect("a pending attempt");
                    if queued == 1 && !b_asked {
                        b_asked = true;
                        b_go_tx.send(()).expect("B asks");
                    } else if queued == 2 {
                        l_go_tx.send(()).expect("L polls");
                        return;
                    }
                }
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            let stop_one = lagging.join().expect("L");
            assert!(first.join().expect("A"), "A is told to publish again");
            let order = order.lock().expect("order").clone();
            let owners: Vec<u64> = order.iter().map(|(owner, _)| *owner).collect();
            assert_eq!(owners, [B, O], "B keeps its place ahead of A's new request");
            assert!(
                order[0].1 > stop_one,
                "B is not lent in the stop that sent A back: {order:?}, {stop_one}"
            );
            assert!(order[1].1 >= order[0].1, "{order:?}");
        });
    });
}

/// D-6 (queued). A requests and exits before its stop is taken: the keeper
/// cancels the request once, the stop lends nothing, and A's grant channel
/// closes with nothing sent.
#[test]
fn l67a_an_exit_while_queued_cancels_the_request() {
    with_live_era("l67a-exit-queued", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let (notices_tx, notices) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx).observe_pending(
            Duration::from_millis(5),
            move |_pending: &StopPending, queued| {
                let _ = notices_tx.send(queued);
            },
        );
        let a = enroll_driver(era, &mut driver, &inbox, O);
        let slow = era.register(L, 0).expect("register the slow owner");
        let heap = era.heap_identity();
        drop(inbox);
        let (running_tx, running) = mpsc::channel::<()>();
        let (exit_tx, exit_rx) = mpsc::channel::<()>();
        let (l_go_tx, l_go) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            scope.spawn(move || lagging_owner(slow, heap, L, running_tx, l_go));
            let queued_owner = scope.spawn(move || {
                running.recv_timeout(BOUND).expect("L runs");
                let (_state, _home, mut raw, grants) = a.start_raw();
                let receipt = raw.publish();
                raw.request(receipt);
                exit_rx.recv_timeout(BOUND).expect("A may exit");
                raw.inbox
                    .send(KeeperMsg::Exit { owner: O })
                    .expect("A exits");
                let RawOwner { mutator, inbox, .. } = raw;
                drop(inbox);
                retire(mutator);
                matches!(
                    grants.recv_timeout(BOUND),
                    Err(mpsc::RecvTimeoutError::Disconnected)
                )
            });
            scope.spawn(move || {
                let mut exited = false;
                loop {
                    let queued = notices.recv_timeout(BOUND).expect("a pending attempt");
                    if queued == 1 && !exited {
                        exited = true;
                        exit_tx.send(()).expect("tell A");
                    } else if queued == 0 && exited {
                        l_go_tx.send(()).expect("L polls");
                        return;
                    }
                }
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            assert!(
                queued_owner.join().expect("A"),
                "A's channel closed with nothing sent"
            );
        });
    });
}

/// D-6 (lent). An exit while the owner holds the ticket is a protocol
/// break: the ticket is lost, so the keeper poisons.
#[test]
fn l67a_an_exit_while_lent_poisons() {
    let (end, told) = with_poisoned_era("l67a-exit-lent", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let a = enroll_driver(era, &mut driver, &inbox, O);
        drop(inbox);
        std::thread::scope(|scope| {
            let lent = scope.spawn(move || {
                let (_state, _home, mut raw, grants) = a.start_raw();
                let receipt = raw.publish();
                raw.request(receipt);
                let Ok(GrantMsg::Grant { ticket, .. }) = grants.recv_timeout(BOUND) else {
                    panic!("A is granted");
                };
                raw.inbox
                    .send(KeeperMsg::Exit { owner: O })
                    .expect("A exits holding the ticket");
                let told = matches!(grants.recv_timeout(BOUND), Ok(GrantMsg::Poisoned));
                drop(ticket);
                drop(raw);
                told
            });
            let end = driver.run();
            (end, lent.join().expect("A"))
        })
    });
    assert_eq!(
        end,
        DriverEnd::Poisoned(Breach::Unexpected {
            owner: O,
            message: "an exit while its ticket is out",
        })
    );
    assert!(told, "A learns the era is poisoned");
}

/// LOW-3 (lead #13966). A's grant channel is closed when its ticket is cut:
/// the grant bounces, the ticket is lost, and the keeper poisons instead of
/// waiting for a ticket that cannot come back.
#[test]
fn l67a_a_grant_to_a_gone_owner_poisons_without_waiting() {
    let end = with_poisoned_era("l67a-gone", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let a = enroll_driver(era, &mut driver, &inbox, O);
        drop(inbox);
        let (done_tx, done) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            scope.spawn(move || {
                let (_state, _home, mut raw, grants) = a.start_raw();
                drop(grants);
                let receipt = raw.publish();
                raw.request(receipt);
                let RawOwner { mutator, inbox, .. } = raw;
                // No sender is left, so the old keeper would read a
                // disconnect rather than hang.
                drop(inbox);
                done.recv_timeout(BOUND).expect("the keeper is done");
                drop(mutator);
            });
            let end = driver.run();
            done_tx.send(()).expect("tell A");
            end
        })
    });
    assert_eq!(end, DriverEnd::Poisoned(Breach::Unreachable { owner: O }));
}

/// L9 (lead #14072 nit). The keeper thread panics while O's request is
/// outstanding. The unwind drops the driver and with it O's grant sender, so
/// `request_turn` reads the disconnect as Poisoned instead of waiting on a
/// keeper that is gone. O asks through a relay the keeper reads, so the
/// panic comes only once O has asked. The Loom row L9 drops the driver as
/// this unwind does: a panic cannot unwind inside a Loom model, whose
/// threads are coroutines on one OS thread, because the unwind would switch
/// coroutines with that OS thread's panic count set.
#[test]
fn l67a_a_keeper_panic_answers_the_waiting_owner_poisoned() {
    const PANIC: &str = "the keeper panics with O's request outstanding";
    let (unwound, answer) = with_poisoned_era("l67a-keeper-panic", |era| {
        let (_inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let (relay, relayed) = mpsc::channel();
        let o = enroll_driver(era, &mut driver, &relay, O);
        drop(relay);
        std::thread::scope(|scope| {
            let waiting = scope.spawn(move || {
                let (_state, _home, mut port) = o.start();
                let answer = port.request_turn().map(|_granted| ());
                // The era is poisoned; the mutator is dropped, not retired.
                drop(port.exit());
                answer
            });
            let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let _driver = driver;
                // Dropped before the driver, so the era is poisoned before
                // O wakes.
                let _panic_guard = KeeperPanicGuard { keeper: era };
                let asked = relayed.recv_timeout(BOUND);
                assert!(
                    matches!(asked, Ok(KeeperMsg::Request { owner: O, .. })),
                    "O asks before the keeper panics"
                );
                panic!("{PANIC}");
            }));
            let unwound = unwound
                .expect_err("the keeper unwound")
                .downcast_ref::<String>()
                .cloned();
            (unwound, waiting.join().expect("O"))
        })
    });
    assert_eq!(unwound.as_deref(), Some(PANIC), "the keeper's own panic");
    assert_eq!(
        answer,
        Err(PortRefusal::Poisoned),
        "O learns the keeper is gone"
    );
}

/// Lead #14009 nit. The keeper owns a running mutator, so its stop refuses
/// `Busy` with the gate free: it waits on itself, and no retry can help. The
/// driver ends that as `SelfWait` after one contention bound, instead of
/// warning about a held gate forever, and the queued owner reads Poisoned.
#[test]
fn l67a_a_keeper_waiting_on_itself_ends_as_a_self_wait() {
    let (end, answer) = with_poisoned_era("l67a-self-wait", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx).contention_bound(Duration::from_millis(50));
        let a = enroll_driver(era, &mut driver, &inbox, O);
        drop(inbox);
        // The keeper's own running member: a frozen owner the stop waits for
        // on the keeper's own thread.
        let own = activate(
            era.register(L, 0).expect("register the keeper's own owner"),
            era.heap_identity(),
            L,
        );
        std::thread::scope(|scope| {
            let queued = scope.spawn(move || {
                let (_state, _home, mut port) = a.start();
                let answer = port.request_turn().map(|_granted| ());
                drop(port.exit());
                answer
            });
            let end = driver.run();
            let answer = queued.join().expect("A");
            drop(own);
            (end, answer)
        })
    });
    assert_eq!(end, DriverEnd::Poisoned(Breach::SelfWait));
    assert_eq!(
        answer,
        Err(PortRefusal::Poisoned),
        "A learns the era is poisoned"
    );
}

/// Lead #14025 nit. Only a superseded receipt (`Generation`) is sent back
/// to publish. A's receipt is from a publication it has since left: it ran
/// again and then acknowledged the stop as a running owner, so it holds no
/// native publication. The head's check refuses `Publication`, and that is a
/// breach, not a Republish.
#[test]
fn l67a_a_receipt_from_an_owner_no_longer_native_is_a_breach() {
    let (end, told) = with_poisoned_era("l67a-publication", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let a = enroll_driver(era, &mut driver, &inbox, O);
        drop(inbox);
        std::thread::scope(|scope| {
            let owner = scope.spawn(move || {
                let (_state, _home, mut raw, grants) = a.start_raw();
                let receipt = raw.publish();
                raw.leave();
                raw.request(receipt);
                let epoch = retry_native("A sees the stop", || raw.mutator.wait_request());
                retry_native("A stops", || raw.mutator.acknowledge(epoch, &[]));
                let told = matches!(grants.recv_timeout(BOUND), Ok(GrantMsg::Poisoned));
                drop(raw);
                told
            });
            let end = driver.run();
            (end, owner.join().expect("A"))
        })
    });
    assert_eq!(
        end,
        DriverEnd::Poisoned(Breach::Rendezvous(RendezvousError::Publication))
    );
    assert!(told, "A learns the era is poisoned");
}

use super::super::keeper_driver::GroupPort;
use super::super::world_turn::WorldReturned;

/// A running owner the Baton-group rows watch.
const R: u64 = 7206;

/// Enroll the Baton group with the driver; its port crosses to the runner.
fn enroll_group(driver: &mut Driver<'_, '_, '_>, inbox: &mpsc::Sender<KeeperMsg>) -> GroupPort {
    let (world_tx, world) = mpsc::sync_channel(1);
    driver.enroll_group(world_tx);
    GroupPort::new(inbox.clone(), world)
}

/// L6.7b b3. The Baton group and an owner are both lent through the
/// keeper's own stops, each in ring order. The group asks while O holds its
/// ticket, so its request is committed after the stop began and is served
/// in the next stop (K4). Both leave and the driver drains.
#[test]
fn l67b_the_group_and_an_owner_take_turns_in_ring_order() {
    with_live_era("l67b-alternate", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let o = enroll_driver(era, &mut driver, &inbox, O);
        let mut group = enroll_group(&mut driver, &inbox);
        drop(inbox);
        let (holding_tx, holding) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                o.run(|state, home, port| {
                    let (grant, lend) = port.request_turn().ok().expect("O is granted");
                    let epoch = grant.epoch;
                    let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                        .ok()
                        .expect("O's grant begins");
                    holding_tx.send(()).expect("tell the group");
                    let seen = turn.with_world(|ev| eval(ev, "(setq sp-live 51)"));
                    turn.end(port).ok().expect("O's host detaches");
                    (epoch, seen)
                })
            });
            let runner = scope.spawn(move || {
                holding.recv_timeout(BOUND).expect("O holds its ticket");
                let (turn, epoch, lend) = group
                    .request_world()
                    .ok()
                    .expect("the group is lent the World");
                let cut = turn.identity();
                group.give_back(lend, WorldReturned::from_end(turn));
                group.exit();
                (epoch, cut)
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            let (owner_epoch, seen) = owner.join().expect("O");
            let (group_epoch, cut) = runner.join().expect("the group");
            assert_eq!(seen, "51");
            assert_eq!(
                cut.epoch, group_epoch,
                "the turn names the stop it was cut in"
            );
            assert_eq!(cut.heap, era.heap_identity());
            assert!(
                group_epoch > owner_epoch,
                "the group, committed during O's lend, is served in a later stop \
                 ({owner_epoch} then {group_epoch})"
            );
        });
    });
}

/// L6.7b b3 exclusion (K3). While the group holds the World, the keeper
/// holds its stop, so a running parallel owner makes no progress. Its
/// counter advances before the group asks and again after the turn comes
/// back, so the probe can see progress.
#[test]
fn l67b_no_parallel_owner_runs_while_the_group_holds_the_world() {
    with_live_era("l67b-exclusion", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let peer = era.register(R, 0).expect("register the running owner");
        let heap = era.heap_identity();
        let mut group = enroll_group(&mut driver, &inbox);
        drop(inbox);
        let progress = AtomicU64::new(0);
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let (progress, done) = (&progress, &done);
            scope.spawn(move || run_peer(activate(peer, heap, R), progress, done));
            let runner = scope.spawn(move || {
                let advanced = |from: u64| {
                    let start = Instant::now();
                    while progress.load(Ordering::Acquire) <= from {
                        assert!(start.elapsed() < BOUND, "the running owner advances");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                };
                advanced(0);
                let (turn, _epoch, lend) = group
                    .request_world()
                    .ok()
                    .expect("the group is lent the World");
                let before = progress.load(Ordering::Acquire);
                std::thread::sleep(Duration::from_millis(30));
                let during = progress.load(Ordering::Acquire);
                group.give_back(lend, WorldReturned::from_end(turn));
                advanced(during);
                done.store(true, Ordering::Release);
                group.exit();
                (before, during)
            });
            let end = driver.run();
            let (before, during) = runner.join().expect("the group");
            // The exclusion first: a stop taken without the owner's ack (the
            // red mutation) also ends refused, and that end must not mask it.
            assert_eq!(
                before, during,
                "no parallel owner ran while the group held the World"
            );
            assert_eq!(end, DriverEnd::Drained);
        });
    });
}

/// L6.7b b3 (K7 for the group). A Baton thread unwinds while the group
/// holds the World. The guard reports the lost turn, the driver poisons the
/// era, and an owner's request is answered `Poisoned`.
#[test]
fn l67b_a_panic_in_a_group_interval_poisons() {
    let (end, answer) = with_poisoned_era("l67b-unwind", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let b = enroll_driver(era, &mut driver, &inbox, B);
        let mut group = enroll_group(&mut driver, &inbox);
        drop(inbox);
        let (idle_tx, idle_rx) = mpsc::channel::<()>();
        let (holding_tx, holding_rx) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let unwinding = scope.spawn(move || {
                // The group asks only once B is NativeStopped and cannot
                // hold the group's stop open.
                idle_rx.recv_timeout(BOUND).expect("B is idle");
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let (_turn, _epoch, _lend) = group
                        .request_world()
                        .ok()
                        .expect("the group is lent the World");
                    holding_tx.send(()).expect("tell B");
                    panic!("an invariant break inside the group's interval");
                }));
                assert!(caught.is_err(), "the interval unwound");
                group.exit();
            });
            let queued = scope.spawn(move || {
                let (_state, _home, mut port) = b.start();
                port.publish_idle().expect("B publishes");
                idle_tx.send(()).expect("tell the group");
                holding_rx
                    .recv_timeout(BOUND)
                    .expect("the group holds the World");
                let answer = port.request_turn().map(|_granted| ());
                drop(port.exit());
                answer
            });
            let end = driver.run();
            unwinding.join().expect("the group's thread");
            let answer = queued.join().expect("B's thread");
            (end, answer)
        })
    });
    assert_eq!(
        end,
        DriverEnd::Poisoned(Breach::GroupAbandoned {
            cause: LendDrop::Unwound,
        })
    );
    assert_eq!(
        answer,
        Err(PortRefusal::Poisoned),
        "B learns the era is poisoned"
    );
}

fn stop_retrying<'k>(era: &'k KeeperEra<'_, '_>) -> AllStopped<'k> {
    let mut stopped = retry_native("stop", || era.stop());
    retry_native("control trace", || stopped.trace(|_stopped, _proof| Ok(())));
    stopped
}

fn complete_retrying(label: &str, mut result: Result<(), AllStoppedFailure<'_>>) {
    let start = Instant::now();
    loop {
        match result {
            Ok(()) => return,
            Err(failure) if failure.reason == RendezvousError::Busy && start.elapsed() < BOUND => {
                std::thread::yield_now();
                result = failure.into_stopped().complete();
            }
            Err(failure) => panic!("{label}: {:?}", failure.reason),
        }
    }
}

/// Like `with_live_era`, also allocating one World-rooted cons survivor and
/// one unrooted garbage cons on the source heap before admission.
fn with_live_era_with_garbage(
    label: &'static str,
    body: impl for<'k, 'service, 'heap> FnOnce(&'k KeeperEra<'service, 'heap>, usize, usize)
    + Send
    + 'static,
) {
    let mut source = ready();
    let private = Value::string(label);
    source.lexenv = private;
    let survivor = source
        .tagged_heap
        .alloc_cons(Value::fixnum(777), Value::NIL);
    source.set_variable("l67a-k5-survivor", survivor);
    let garbage = source
        .tagged_heap
        .alloc_cons(Value::fixnum(999), Value::NIL)
        .bits();
    let survivor_word = survivor.bits();
    source.tagged_heap.close_alloc_regions();
    let (shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |keeper| body(keeper, survivor_word, garbage));
        match outcome {
            Ok((end, ())) => tx.send(Ok(end)).expect("send keeper end"),
            Err(failure) => tx.send(Err(failure.notice())).expect("send keeper notice"),
        }
    });
    let outcome = rx.recv().expect("keeper outcome");
    service.join().expect("keeper service thread");
    let end = outcome.unwrap_or_else(|notice| panic!("{label}: era refused: {notice:?}"));
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
}

/// D-8 (K5). A `KeeperMsg::Collect` and a queued serial turn ride ONE stop:
/// `report.epoch == grant.epoch`, unrooted garbage is reclaimed, the World
/// survivor stays live, the serial turn runs in that same stop, and the driver
/// drains cleanly.
#[test]
fn l67a_collection_request_rides_batch_stop_once() {
    with_live_era_with_garbage("l67a-k5-d8", |era, survivor, garbage| {
        let before = {
            let stopped = stop_retrying(era);
            assert!(era.owns_heap_word_for_test(&stopped, survivor).unwrap());
            assert!(era.owns_heap_word_for_test(&stopped, garbage).unwrap());
            let cycles = era.gc_collections_for_test(&stopped).unwrap();
            complete_retrying("baseline stop", stopped.complete());
            cycles
        };
        let (inbox, inbox_rx) = mpsc::channel();
        let (notices_tx, notices) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx).observe_pending(
            Duration::from_millis(5),
            move |_pending: &StopPending, queued| {
                let _ = notices_tx.send(queued);
            },
        );
        let a = enroll_driver(era, &mut driver, &inbox, O);
        let slow = era.register(L, 0).expect("register slow owner");
        let heap = era.heap_identity();
        let collect_inbox = inbox.clone();
        drop(inbox);
        let (running_tx, running) = mpsc::channel::<()>();
        let (l_go_tx, l_go) = mpsc::channel::<()>();
        let (c_tx, c_rx) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            scope.spawn(move || lagging_owner(slow, heap, L, running_tx, l_go));
            let owner = scope.spawn(move || {
                running.recv_timeout(BOUND).expect("L runs");
                a.run(|state, home, port| {
                    let granted = port.request_turn().ok().expect("A is granted");
                    let epoch = granted.0.epoch;
                    let mut turn = begin_turn(state, home, granted, port, standalone)
                        .ok()
                        .expect("A begins");
                    let seen = turn.with_world(|ev| eval(ev, "(setq sp-live 81)"));
                    turn.end(port).ok().expect("A ends");
                    (epoch, seen)
                })
            });
            scope.spawn(move || {
                loop {
                    let queued = notices.recv_timeout(BOUND).expect("pending notice");
                    if queued == 1 {
                        collect_inbox
                            .send(KeeperMsg::Collect { reply: Some(c_tx) })
                            .expect("send Collect during pending stop");
                        drop(collect_inbox);
                        // Wait for one more pending tick so `between_stops` has
                        // drained `Collect` before releasing L.
                        let _ = notices.recv_timeout(BOUND).expect("post-collect notice");
                        l_go_tx.send(()).expect("release L");
                        return;
                    }
                }
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            let report = c_rx
                .recv_timeout(BOUND)
                .expect("collect reply")
                .expect("collection succeeded");
            let (grant_epoch, seen) = owner.join().expect("A");
            assert_eq!(seen, "81");
            assert!(report.major);
            assert_eq!(
                report.epoch,
                grant_epoch.get(),
                "collection and serial turn must ride ONE stop"
            );
            assert_eq!(report.cycle, before + 1);
        });
        let stopped = stop_retrying(era);
        assert!(era.owns_heap_word_for_test(&stopped, survivor).unwrap());
        assert!(
            !era.owns_heap_word_for_test(&stopped, garbage).unwrap(),
            "in-stop collection must reclaim unrooted garbage"
        );
        complete_retrying("verify stop", stopped.complete());
    });
}

/// D-8b (K5). With the serial ring empty, `KeeperMsg::Collect` wakes the
/// parked driver, runs one GC-only stop without entering `with_batch`, replies
/// `Ok(report)`, reclaims garbage, and lets later turns proceed normally.
#[test]
fn l67a_collection_runs_with_empty_serial_ring() {
    with_live_era_with_garbage("l67a-k5-d8b", |era, survivor, garbage| {
        let before = {
            let stopped = stop_retrying(era);
            let cycles = era.gc_collections_for_test(&stopped).unwrap();
            complete_retrying("baseline stop", stopped.complete());
            cycles
        };
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let a = enroll_driver(era, &mut driver, &inbox, O);
        let collect_inbox = inbox.clone();
        drop(inbox);
        let (idle_tx, idle_rx) = mpsc::channel::<()>();
        let (after_tx, after_rx) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                a.run(|state, home, port| {
                    port.publish_idle().expect("A parks idle");
                    idle_tx.send(()).expect("A is idle");
                    after_rx.recv_timeout(BOUND).expect("collect finished");
                    let mut turn = driver_turn(state, home, port, "(setq sp-live 82)");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("A ends");
                    seen
                })
            });
            let requester = scope.spawn(move || {
                idle_rx.recv_timeout(BOUND).expect("A parked");
                let (c_tx, c_rx) = mpsc::sync_channel(1);
                collect_inbox
                    .send(KeeperMsg::Collect { reply: Some(c_tx) })
                    .expect("send empty-ring Collect");
                let report = c_rx
                    .recv_timeout(BOUND)
                    .expect("collect reply")
                    .expect("empty-ring collection succeeded");
                drop(collect_inbox);
                after_tx.send(()).expect("release A");
                report
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            let report = requester.join().expect("requester");
            assert!(report.major);
            assert_eq!(report.cycle, before + 1);
            assert_eq!(owner.join().expect("A"), "82");
        });
        let stopped = stop_retrying(era);
        assert!(era.owns_heap_word_for_test(&stopped, survivor).unwrap());
        assert!(!era.owns_heap_word_for_test(&stopped, garbage).unwrap());
        complete_retrying("verify stop", stopped.complete());
    });
}

/// D-8c (K5). With no `KeeperMsg::Collect`, published allocation debt reaching
/// `gc_threshold` piggybacks a collection onto the owner's serial stop,
/// reclaims garbage, resets debt, and does not re-collect on the next turn.
#[test]
fn l67a_pacer_debt_piggybacks_collection_on_serial_stop() {
    with_live_era_with_garbage("l67a-k5-d8c", |era, survivor, garbage| {
        let before = {
            let stopped = stop_retrying(era);
            assert!(era.owns_heap_word_for_test(&stopped, garbage).unwrap());
            // Entry 0's published image already carries the allocation debt
            // from `survivor` and `garbage`; lowering the threshold to 1 makes
            // the next serial stop due without any `KeeperMsg::Collect`.
            era.set_gc_threshold_for_test(&stopped, 1).unwrap();
            let cycles = era.gc_collections_for_test(&stopped).unwrap();
            complete_retrying("setup stop", stopped.complete());
            cycles
        };
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let a = enroll_driver(era, &mut driver, &inbox, O);
        drop(inbox);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                a.run(|state, home, port| {
                    let mut t1 = driver_turn(state, home, port, "(setq sp-live 83)");
                    let s1 = t1.with_world(|ev| eval(ev, "sp-live"));
                    t1.end(port).ok().expect("turn 1 ends");
                    // Second turn after debt reset: must NOT collect again.
                    let mut t2 = driver_turn(state, home, port, "(setq sp-live 84)");
                    let s2 = t2.with_world(|ev| eval(ev, "sp-live"));
                    t2.end(port).ok().expect("turn 2 ends");
                    (s1, s2)
                })
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            assert_eq!(
                owner.join().expect("A"),
                ("83".to_string(), "84".to_string())
            );
        });
        let stopped = stop_retrying(era);
        assert_eq!(
            era.gc_collections_for_test(&stopped).unwrap(),
            before + 1,
            "pacer must collect exactly once on the due stop and reset debt"
        );
        assert!(era.owns_heap_word_for_test(&stopped, survivor).unwrap());
        assert!(!era.owns_heap_word_for_test(&stopped, garbage).unwrap());
        complete_retrying("verify stop", stopped.complete());
    });
}

/// D-8c-serial (lead #13986 condition). With zero published debt across all
/// images and no `KeeperMsg::Collect`, allocation debt charged ONLY on the
/// keeper heap itself (serial-turn debt) drives a due collection on the next
/// serial stop and resets to false afterward.
#[test]
fn l67a_serial_turn_only_debt_drives_due_collection() {
    with_live_era("l67a-k5-d8c-serial", |era| {
        let before = {
            let mut stopped = retry_native("initial stop", || era.stop());
            let report = retry_native("drain baseline debt", || {
                era.collect_requested(&mut stopped)
            });
            assert_eq!(
                era.should_collect_stopped(&mut stopped),
                Ok(false),
                "post-collection baseline debt must be 0"
            );
            let threshold = era.gc_threshold_for_test(&stopped).unwrap();
            // Charge ONLY the keeper heap's own `bytes_since_gc` (zero peer or
            // entry-0 image debt).
            era.charge_serial_bytes_for_test(&stopped, threshold)
                .unwrap();
            complete_retrying("setup stop", stopped.complete());
            report.cycle
        };
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let a = enroll_driver(era, &mut driver, &inbox, O);
        drop(inbox);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                a.run(|state, home, port| {
                    let mut turn = driver_turn(state, home, port, "(setq sp-live 85)");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("turn ends");
                    seen
                })
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            assert_eq!(owner.join().expect("A"), "85");
        });
        let mut stopped = stop_retrying(era);
        assert_eq!(
            era.gc_collections_for_test(&stopped).unwrap(),
            before + 1,
            "serial-turn-only keeper heap debt must drive a collection"
        );
        assert_eq!(
            era.should_collect_stopped(&mut stopped),
            Ok(false),
            "collection must reset serial-turn keeper heap debt"
        );
        complete_retrying("verify stop", stopped.complete());
    });
}

/// D-8d (K5). A permanent in-stop collection failure (`Publication` from a
/// poisoned cons allocator holding a lease) poisons the driver and wakes both
/// the `Collect` waiter (`CollectRefusal::Poisoned`) and queued serial
/// requester (`PortRefusal::Poisoned`) without hanging.
#[test]
fn l67a_collection_failure_in_stop_poisons_and_wakes_waiters() {
    let (end, collect_res, port_res) = with_poisoned_era("l67a-k5-d8d", |era| {
        // Retire one owner with a closed owner-cons lease and poison the cons
        // allocator so `collect_requested` refuses with `Publication`.
        let lease_permit = era.register(B, 0).expect("register leased owner");
        let lease_key = lease_permit.key();
        let heap = era.heap_identity();
        std::thread::scope(|scope| {
            let (parked_tx, parked_rx) = mpsc::channel::<()>();
            let (retire_tx, retire_rx) = mpsc::channel::<()>();
            let leased = scope.spawn(move || {
                let mut owner = activate(lease_permit, heap, B);
                retry_native("enter native", || owner.enter_native(&[]));
                parked_tx.send(()).expect("leased parked");
                retire_rx.recv_timeout(BOUND).expect("retire leased");
                retire(owner);
            });
            parked_rx.recv_timeout(BOUND).expect("leased parked");
            let stopped = stop_retrying(era);
            era.grant_closed_owner_cons_lease_for_test(&stopped, lease_key)
                .unwrap();
            era.poison_cons_allocator_for_test(&stopped).unwrap();
            complete_retrying("poison-setup stop", stopped.complete());
            retire_tx.send(()).expect("retire leased");
            leased.join().expect("leased owner");
        });

        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let a = enroll_driver(era, &mut driver, &inbox, O);
        let (c_tx, c_rx) = mpsc::sync_channel(1);
        let collect_inbox = inbox.clone();
        drop(inbox);
        std::thread::scope(|scope| {
            let owner = scope.spawn(move || {
                let (_state, _home, mut port) = a.start();
                // Queue Collect only after A has activated so A is live when
                // the failing stop poisons the era.
                collect_inbox
                    .send(KeeperMsg::Collect { reply: Some(c_tx) })
                    .expect("queue Collect");
                drop(collect_inbox);
                let res = port.request_turn().map(|_| ());
                drop(port.exit());
                res
            });
            let end = driver.run();
            let collect_res = c_rx.recv_timeout(BOUND).expect("collect waiter woken");
            let port_res = owner.join().expect("A");
            (end, collect_res, port_res)
        })
    });
    assert_eq!(
        end,
        DriverEnd::Poisoned(Breach::Rendezvous(RendezvousError::Publication))
    );
    assert_eq!(collect_res, Err(CollectRefusal::Poisoned));
    assert_eq!(port_res, Err(PortRefusal::Poisoned));
}

/// D-8e' (K5 / K8 r2). A slow owner delays an empty-ring `KeeperMsg::Collect`
/// past the attempt bound without poisoning; a second `KeeperMsg::Collect`
/// arriving during `Pending` is drained via `between_stops`, and once the slow
/// owner parks, BOTH waiters receive `Ok(report)` from the same collection.
#[test]
fn l67a_slow_owner_delays_empty_ring_collect_then_completes() {
    with_live_era("l67a-k5-d8e", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let (notices_tx, notices) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx).observe_pending(
            Duration::from_millis(5),
            move |pending: &StopPending, queued| {
                let _ = notices_tx.send((pending.epoch(), queued));
            },
        );
        let slow = era.register(L, 0).expect("register slow owner");
        let heap = era.heap_identity();
        let collect_inbox = inbox.clone();
        drop(inbox);
        let (running_tx, running) = mpsc::channel::<()>();
        let (l_go_tx, l_go) = mpsc::channel::<()>();
        let (c1_tx, c1_rx) = mpsc::sync_channel(1);
        let (c2_tx, c2_rx) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            scope.spawn(move || lagging_owner(slow, heap, L, running_tx, l_go));
            scope.spawn(move || {
                running.recv_timeout(BOUND).expect("L runs");
                collect_inbox
                    .send(KeeperMsg::Collect { reply: Some(c1_tx) })
                    .expect("send Collect 1");
                let (epoch1, queued1) = notices.recv_timeout(BOUND).expect("pending notice 1");
                assert!(epoch1.is_some(), "stop is standing");
                assert_eq!(queued1, 0, "empty serial ring");
                // Deliver a second Collect while Pending; the driver drains it
                // through `between_stops` and answers both from this stop.
                collect_inbox
                    .send(KeeperMsg::Collect { reply: Some(c2_tx) })
                    .expect("send Collect 2");
                drop(collect_inbox);
                let _ = notices.recv_timeout(BOUND).expect("pending notice 2");
                l_go_tx.send(()).expect("release L");
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            let r1 = c1_rx
                .recv_timeout(BOUND)
                .expect("waiter 1")
                .expect("collect 1 ok");
            let r2 = c2_rx
                .recv_timeout(BOUND)
                .expect("waiter 2")
                .expect("collect 2 ok");
            assert!(r1.major);
            assert_eq!(r1.epoch, r2.epoch, "both waiters share the single stop");
            assert_eq!(r1.cycle, r2.cycle);
        });
    });
}

/// D-8f (K5 / C-R1 / C-R4). A closed registration-tagged owner-cons lease from
/// a retired owner is reclaimed when collection rides a batch stop, draining
/// `OwnerConsRetireQueue`, clearing `has_owner_cons_reserves`, and allowing
/// clean era `finish()`.
#[test]
fn l67a_collection_in_batch_stop_reclaims_abandoned_owner_cons_lease() {
    with_live_era("l67a-k5-d8f", |era| {
        let lease_permit = era.register(B, 0).expect("register leased owner");
        let lease_key = lease_permit.key();
        let heap = era.heap_identity();
        std::thread::scope(|scope| {
            let (parked_tx, parked_rx) = mpsc::channel::<()>();
            let (retire_tx, retire_rx) = mpsc::channel::<()>();
            let leased = scope.spawn(move || {
                let mut owner = activate(lease_permit, heap, B);
                retry_native("enter native", || owner.enter_native(&[]));
                parked_tx.send(()).expect("leased parked");
                retire_rx.recv_timeout(BOUND).expect("retire leased");
                retire(owner);
            });
            parked_rx.recv_timeout(BOUND).expect("leased parked");
            let stopped = stop_retrying(era);
            era.grant_closed_owner_cons_lease_for_test(&stopped, lease_key)
                .unwrap();
            assert!(era.has_owner_cons_reserves_for_test(&stopped).unwrap());
            complete_retrying("grant stop", stopped.complete());
            retire_tx.send(()).expect("retire leased");
            leased.join().expect("leased owner");
        });
        assert_eq!(
            era.retired_owner_cons_len_for_test(),
            1,
            "retire_rooted queued the leased registration"
        );

        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let a = enroll_driver(era, &mut driver, &inbox, O);
        let (c_tx, c_rx) = mpsc::sync_channel(1);
        let collect_inbox = inbox.clone();
        drop(inbox);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                a.run(|state, home, port| {
                    collect_inbox
                        .send(KeeperMsg::Collect { reply: Some(c_tx) })
                        .expect("queue Collect");
                    drop(collect_inbox);
                    let mut turn = driver_turn(state, home, port, "(setq sp-live 86)");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("A ends");
                    seen
                })
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            let report = c_rx
                .recv_timeout(BOUND)
                .expect("collect reply")
                .expect("collection ok");
            assert!(report.major);
            assert_eq!(owner.join().expect("A"), "86");
        });
        assert_eq!(
            era.retired_owner_cons_len_for_test(),
            0,
            "in-stop collection must drain OwnerConsRetireQueue"
        );
        let stopped = stop_retrying(era);
        assert!(
            !era.has_owner_cons_reserves_for_test(&stopped).unwrap(),
            "in-stop collection must discharge owner_reserved_blocks before finish()"
        );
        complete_retrying("verify stop", stopped.complete());
    });
}

/// Lead #13986 condition (a4.2): binding an untraced `AllStopped` through
/// `StoppedKeeper::from_scope` must refuse `grant_for` with
/// `RendezvousError::Publication` before any serial lend can be minted.
#[test]
fn l67a_binding_an_untraced_stop_refuses_grant_before_any_lend() {
    let grant_result = with_poisoned_era("l67a-untraced-bind", |era| {
        let permit = era.register(O, 0).expect("register O");
        let key = permit.key();
        let heap = era.heap_identity();
        std::thread::scope(|scope| {
            let (receipt_tx, receipt_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel::<()>();
            let owner = scope.spawn(move || {
                let mut mutator = activate(permit, heap, O);
                let receipt = retry_native("publish", || mutator.publish_native_owner(&[]));
                receipt_tx.send(receipt).expect("send receipt");
                done_rx.recv_timeout(BOUND).expect("done");
                drop(mutator);
            });
            let receipt = receipt_rx.recv_timeout(BOUND).expect("receipt");
            let (_, generation, _) = receipt.serial_facts();
            retry_native("enqueue", || era.enqueue_serial(key, generation));
            // Deliberately do NOT call `trace_serial` or `collect_requested`.
            let stopped = retry_native("untraced stop", || era.stop());
            let grant_error = with_stop_scope(|scope| {
                let mut keeper = StoppedKeeper::from_scope(scope, stopped)
                    .map_err(|(_, _, reason)| reason)
                    .expect("bind succeeds on untraced stop");
                let ready = keeper.ready_request(receipt).expect("receipt ready");
                let err = keeper.grant_for(&ready).err();
                drop(keeper);
                err
            });
            done_tx.send(()).expect("release owner");
            owner.join().expect("owner");
            grant_error
        })
    });
    assert_eq!(
        grant_result,
        Some(RendezvousError::Publication),
        "binding an untraced stop must refuse grant_serial_head before any lend"
    );
}

/// L6.7b b5 (QB3). The group holds the World while O's request is committed
/// behind it, and a Baton thread collects. The keeper ends that stop,
/// collects first in the next one, and lends the group the World again in
/// that same stop, before O: the collecting thread is the next World user
/// and no other client's turn comes between its collection and its resume.
/// The collection reclaims the unrooted garbage.
#[test]
fn l67b_a_group_collection_runs_first_and_the_group_resumes_next() {
    with_live_era_with_garbage("l67b-b5", |era, survivor, garbage| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let o = enroll_driver(era, &mut driver, &inbox, O);
        let mut group = enroll_group(&mut driver, &inbox);
        drop(inbox);
        let log = std::sync::Mutex::new(Vec::<&'static str>::new());
        let (idle_tx, idle) = mpsc::channel::<()>();
        let (holding_tx, holding) = mpsc::channel::<()>();
        let (sent_tx, sent) = mpsc::channel::<()>();
        let (report_tx, report_rx) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let log = &log;
            let owner = scope.spawn(move || {
                let (mut state, home, mut raw, grants) = o.start_raw();
                // NativeStopped, so the group's stops do not wait on O.
                let receipt = raw.publish();
                let (_, generation, _) = receipt.serial_facts();
                idle_tx.send(()).expect("tell the group");
                holding
                    .recv_timeout(BOUND)
                    .expect("the group holds the World");
                raw.request(receipt);
                sent_tx.send(()).expect("O's request is in the inbox");
                let Ok(GrantMsg::Grant { ticket, epoch }) = grants.recv_timeout(BOUND) else {
                    panic!("O is granted");
                };
                log.lock().expect("log").push("owner");
                let mut port = raw.port();
                raw_turn(
                    &mut state,
                    home,
                    &mut port,
                    Grant {
                        ticket,
                        generation,
                        epoch,
                    },
                    "(setq sp-live 91)",
                );
                raw.inbox
                    .send(KeeperMsg::Exit { owner: O })
                    .expect("O exits");
                retire(raw.mutator);
                epoch
            });
            let runner = scope.spawn(move || {
                idle.recv_timeout(BOUND).expect("O is idle");
                let (turn, first, lend) = group
                    .request_world()
                    .ok()
                    .expect("the group is lent the World");
                log.lock().expect("log").push("group");
                holding_tx.send(()).expect("tell O");
                // O's request precedes the collection in the keeper's inbox.
                sent.recv_timeout(BOUND).expect("O asked");
                let (turn, again, lend) = group
                    .collect(lend, WorldReturned::from_end(turn), Some(report_tx))
                    .ok()
                    .expect("the group resumes after its collection");
                log.lock().expect("log").push("group");
                group.give_back(lend, WorldReturned::from_end(turn));
                group.exit();
                (first, again)
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            let (first, again) = runner.join().expect("the group");
            let owner_epoch = owner.join().expect("O");
            let report = report_rx
                .recv_timeout(BOUND)
                .expect("the collection report")
                .expect("the collection ran");
            assert_eq!(
                *log.lock().expect("log"),
                ["group", "group", "owner"],
                "the collecting group resumes before O's turn"
            );
            assert!(again > first, "the collection ended the group's stop");
            assert_eq!(
                report.epoch,
                again.get(),
                "the collection and the group's resume ride one stop"
            );
            assert!(owner_epoch >= again);
        });
        let stopped = stop_retrying(era);
        assert!(era.owns_heap_word_for_test(&stopped, survivor).unwrap());
        assert!(
            !era.owns_heap_word_for_test(&stopped, garbage).unwrap(),
            "the group's collection reclaimed the garbage"
        );
        complete_retrying("verify stop", stopped.complete());
    });
}

// ---------------------------------------------------------------------------
// The stop-cost rows (PLAN-L6-AMEND K3; lead #14000 QB2/QB3). Measurement
// rows: each also asserts that every turn ran. Test-driver parameters, never
// a product knob:
// - `L6_STOP_PEERS`: running parallel owners that must meet every stop
//   (default 1; the ruled points are 1 and 4).
// - `L6_STOP_ITERS`: turns, intervals or collections per row (default 20).
// Each row prints one `l6_stop_cost=` line with the timed section's elapsed
// nanoseconds; timing runs are local, off the shared hosts.
// ---------------------------------------------------------------------------

fn stop_cost_param(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

/// Which client the stop-cost row drives through the keeper.
#[derive(Clone, Copy)]
enum StopCostRow {
    /// A parallel owner's serial turns, one stop each.
    OwnerTurns,
    /// Baton-group World intervals, one stop each.
    GroupIntervals,
    /// Baton-initiated collections: two stops each (QB3).
    GroupCollections,
}

impl StopCostRow {
    fn name(self) -> &'static str {
        match self {
            StopCostRow::OwnerTurns => "owner_turns",
            StopCostRow::GroupIntervals => "group_intervals",
            StopCostRow::GroupCollections => "group_collections",
        }
    }
}

/// Drive ITERS of ROW through the product driver while PEERS running
/// owners meet every stop, and report the timed section.
fn stop_cost(row: StopCostRow) {
    let peers = stop_cost_param("L6_STOP_PEERS", 1);
    let iters = stop_cost_param("L6_STOP_ITERS", 20);
    with_live_era("l6-stop-cost", move |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let o = enroll_driver(era, &mut driver, &inbox, O);
        let mut group = enroll_group(&mut driver, &inbox);
        let heap = era.heap_identity();
        let permits: Vec<_> = (0..peers)
            .map(|index| {
                let owner = 7300 + index as u64;
                (
                    owner,
                    era.register(owner, 0).expect("register a running peer"),
                )
            })
            .collect();
        drop(inbox);
        let progress = AtomicU64::new(0);
        let done = AtomicBool::new(false);
        let (ready_tx, ready) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let (progress, done) = (&progress, &done);
            for (owner, permit) in permits {
                let ready_tx = ready_tx.clone();
                scope.spawn(move || {
                    let mutator = activate(permit, heap, owner);
                    ready_tx.send(()).expect("the peer runs");
                    run_peer(mutator, progress, done)
                });
            }
            drop(ready_tx);
            let runner = scope.spawn(move || {
                // Every peer is a running member before the first stop.
                for _ in 0..peers {
                    ready.recv_timeout(BOUND).expect("the peers run");
                }
                let mut ran = 0;
                let start = Instant::now();
                match row {
                    StopCostRow::OwnerTurns => {
                        group.exit();
                        o.run(|state, home, port| {
                            for _ in 0..iters {
                                let turn = driver_turn(state, home, port, "(setq sp-live 1)");
                                turn.end(port).ok().expect("the host detaches");
                                ran += 1;
                            }
                        });
                    }
                    StopCostRow::GroupIntervals => {
                        o.run(|_state, _home, _port| ());
                        for _ in 0..iters {
                            let (turn, _epoch, lend) = group
                                .request_world()
                                .ok()
                                .expect("the group is lent the World");
                            group.give_back(lend, WorldReturned::from_end(turn));
                            ran += 1;
                        }
                        group.exit();
                    }
                    StopCostRow::GroupCollections => {
                        o.run(|_state, _home, _port| ());
                        let (mut turn, _epoch, mut lend) = group
                            .request_world()
                            .ok()
                            .expect("the group is lent the World");
                        for _ in 0..iters {
                            (turn, _, lend) = group
                                .collect(lend, WorldReturned::from_end(turn), None)
                                .ok()
                                .expect("the group resumes after its collection");
                            ran += 1;
                        }
                        group.give_back(lend, WorldReturned::from_end(turn));
                        group.exit();
                    }
                }
                (ran, start.elapsed())
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            done.store(true, Ordering::Release);
            let (ran, elapsed) = runner.join().expect("the stop-cost runner");
            assert_eq!(ran, iters, "every turn ran");
            eprintln!(
                "l6_stop_cost={} peers={peers} iters={iters} elapsed_ns={} per_iter_ns={}",
                row.name(),
                elapsed.as_nanos(),
                elapsed.as_nanos() / iters.max(1) as u128
            );
        });
    });
}

#[test]
fn l6_stop_cost_owner_turns() {
    stop_cost(StopCostRow::OwnerTurns);
}

#[test]
fn l6_stop_cost_group_intervals() {
    stop_cost(StopCostRow::GroupIntervals);
}

#[test]
fn l6_stop_cost_group_collections() {
    stop_cost(StopCostRow::GroupCollections);
}

//! E5.2 and L6.7 over a real era (E5.2-PARK-RESUME §4). Owners run on
//! their own threads with standalone Contexts and drive `serial_park`
//! through the product port; the keeper service thread runs the product
//! driver (L6.7a), which commits each request in the ring, stops, grants
//! the head, lends, accepts the ticket back, and retires the entry (C3:
//! E5.2's rows run here, not against a test keeper). R7 (a collection at
//! the park) waits on L6.5's session.

use super::super::keeper_custody::ReturnedSerialTicket;
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
use neovm_value::tagged::gc::heap_transfer::PublishedOwner;
use neovm_value::tagged::gc::{Mutator, MutatorPermit, RendezvousError, StopPending};
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
    Breach, ChannelLend, ChannelPort, Driver, DriverEnd, GrantMsg, KeeperMsg, LendDrop,
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

/// C3 (R1/R4 on the product port). O parks and its ticket goes back; it
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
                    let turn = driver_turn(state, home, port, "(setq sp-live 33)");
                    let parked = turn.park(Wake("fired"), port).ok().expect("parks");
                    let mut turn = parked
                        .resume(port, standalone)
                        .ok()
                        .expect("the next grant resumes");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            assert_eq!(driver.run(), DriverEnd::Drained);
            assert_eq!(owner.join().expect("O"), "33");
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
                    let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
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
                    let parked_turn = turn.park(Wake("mutex"), port).ok().expect("O parks");
                    parked_tx.send(s1).expect("tell the conductor");
                    wake.recv_timeout(BOUND).expect("woken");
                    let mut turn = parked_turn
                        .resume(port, standalone)
                        .ok()
                        .expect("the later grant resumes");
                    let seen = turn.with_world(|ev| {
                        let seen = eval(ev, "(list sp-live sp-bound (match-end 0))");
                        ev.unbind_to(count);
                        (seen, eval(ev, "sp-bound"))
                    });
                    turn.end(port).ok().expect("O's host detaches");
                    (s1, seen)
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
            let (o_epoch, seen) = parked_owner.join().expect("O");
            let (b_epoch, b_seen) = other.join().expect("B");
            peer.join().expect("P");
            assert_eq!(o_epoch, s1);
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
                    let (grant, lend) = port.request_turn().ok().expect("O is granted");
                    let s1 = grant.epoch;
                    let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                        .ok()
                        .expect("O's grant begins");
                    turn.with_world(|ev| eval(ev, "(setq sp-live 5)"));
                    let parked_turn = turn.park(Wake("condvar"), port).ok().expect("O parks");
                    parked_tx.send(s1).expect("tell B");
                    let s2 = b_done.recv_timeout(BOUND).expect("B's stop completed");
                    let mut turn = parked_turn
                        .resume(port, standalone)
                        .ok()
                        .expect("the next grant resumes");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("O's host detaches");
                    (s1, s2, seen)
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
            let (o_s1, s2, seen) = owner.join().expect("O");
            let (b_s1, b_s2, b_seen) = other.join().expect("B");
            assert_eq!(o_s1, b_s1);
            assert_eq!(s2, b_s2);
            assert!(
                s2 > o_s1,
                "B's stop came after the one O parked in, and completed without O"
            );
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
            assert_eq!(driver.run(), DriverEnd::Drained);
            let (before, during) = runner.join().expect("the group");
            assert_eq!(
                before, during,
                "no parallel owner ran while the group held the World"
            );
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

//! E5.2 over a real keeper (E5.2-PARK-RESUME §4). Owners run on their own
//! threads with standalone Contexts and drive only `serial_park`. The keeper
//! service thread plays L6.7's part: it commits each request in the ring,
//! grants the head, stops, lends, accepts the ticket back, and retires the
//! entry. R7 (a collection at the park) waits on L6.5's session.

use super::super::keeper_custody::{ReturnedSerialTicket, StoppedKeeper};
use super::super::owned_source::era_fixture::{
    KeeperEra, KeeperPanicGuard, activate, close, finish_fixture, keeper_serve, ready, retire,
    retry_native,
};
use super::super::serial_ticket::{LocalSerialOwner, ReceiveReason, StoppedSerialTicket};
use super::super::serial_turn::OwnerSerialState;
use super::{
    Grant, KeeperPort, ParkedSerial, PortRefusal, SerialHome, TurnStartRefusal, begin_turn,
};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::intern::intern;
use neovm_value::tagged::gc::heap_transfer::{PublishedOwner, with_stop_scope};
use neovm_value::tagged::gc::{
    AllStopped, AllStoppedFailure, Mutator, MutatorKey, MutatorPermit, RendezvousError,
};
use neovm_value::value::Value;
use std::collections::HashMap;
use std::convert::Infallible;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// Every wait in these rows is bounded, so a lost ticket is a red row and
/// not a hang.
const BOUND: Duration = Duration::from_secs(30);

/// The registry gate is a try-lock, so an owner retrying `leave_native`
/// through a stop can make a keeper call see `Busy`. The keeper retries, as
/// L6.7's service must.
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

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

/// Owner -> keeper. One channel, so each owner's messages arrive in the
/// order its port sent them (I3).
enum ToKeeper {
    Request(u64, PublishedOwner),
    GiveBack(u64, ReturnedSerialTicket),
    Refuse(u64, StoppedSerialTicket, ReceiveReason),
}

impl ToKeeper {
    fn describe(&self) -> String {
        match self {
            ToKeeper::Request(owner, _) => format!("{owner}'s request"),
            ToKeeper::GiveBack(owner, _) => format!("{owner}'s returned ticket"),
            ToKeeper::Refuse(owner, _, reason) => format!("{owner}'s refusal ({reason:?})"),
        }
    }
}

/// Keeper -> one owner: a lent ticket and the stop epoch that cut it.
struct ToOwner(StoppedSerialTicket, NonZeroU64);

/// The owner's port: its era mutator and the two channels. Each request
/// leaves native if the owner is native, and publishes again: a receipt is
/// single-use, and the request is committed under the new one.
struct LivePort<'era> {
    owner: u64,
    mutator: Mutator<'era>,
    native: bool,
    to_keeper: mpsc::Sender<ToKeeper>,
    grants: mpsc::Receiver<ToOwner>,
}

impl KeeperPort for LivePort<'_> {
    type LendGuard = ();

    fn give_back(&mut self, returned: ReturnedSerialTicket) {
        self.to_keeper
            .send(ToKeeper::GiveBack(self.owner, returned))
            .expect("the keeper is serving");
    }

    fn request_turn(&mut self) -> Result<(Grant, ()), PortRefusal> {
        let mutator = &mut self.mutator;
        if self.native {
            retry_native("leave native to publish again", || {
                mutator.leave_native().map(|_| ())
            });
        }
        let receipt = retry_native("publish for the request", || {
            mutator.publish_native_owner(&[])
        });
        self.native = true;
        let (_, generation, _) = receipt.serial_facts();
        self.to_keeper
            .send(ToKeeper::Request(self.owner, receipt))
            .expect("the keeper is serving");
        match self.grants.recv_timeout(BOUND) {
            Ok(ToOwner(ticket, epoch)) => Ok((
                Grant {
                    ticket,
                    generation,
                    epoch,
                },
                (),
            )),
            Err(error) => panic!("owner {}: no grant within the bound: {error}", self.owner),
        }
    }

    fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason) {
        self.to_keeper
            .send(ToKeeper::Refuse(self.owner, ticket, reason))
            .expect("the keeper is serving");
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

/// What one owner thread takes from the keeper thread.
struct OwnerKit<'era> {
    permit: MutatorPermit<'era>,
    home: SerialHome,
    to_keeper: mpsc::Sender<ToKeeper>,
    grants: mpsc::Receiver<ToOwner>,
}

impl<'era> OwnerKit<'era> {
    /// On the owner thread: build the owner's standalone Context and keep it
    /// dormant, start its era mutator, run SCRIPT, then retire the mutator.
    fn run<R>(self, script: impl FnOnce(&mut State, SerialHome, &mut LivePort<'era>) -> R) -> R {
        let mut ev = Context::new(crate::test_utils::unit_runtime());
        eval(
            &mut ev,
            "(progn (defvar sp-live 0) (setq sp-live 1) (defvar sp-bound nil))",
        );
        let mut state = OwnerSerialState::new(self.home.owner, Box::new(ev))
            .ok()
            .expect("a freshly built Context detaches");
        let mut port = LivePort {
            owner: self.home.owner,
            mutator: activate(self.permit, self.home.heap, self.home.owner),
            native: false,
            to_keeper: self.to_keeper,
            grants: self.grants,
        };
        let out = script(&mut state, self.home, &mut port);
        retire(port.mutator);
        out
    }
}

/// One turn the keeper lends in a stop: the owner, its ring entry, and the
/// receipt its request was committed under.
struct Turn {
    owner: u64,
    id: u64,
    receipt: PublishedOwner,
}

/// The keeper thread's side: L6.7's part, in the order the note gives it.
struct TestKeeper<'k, 'service, 'heap> {
    era: &'k KeeperEra<'service, 'heap>,
    inbox: mpsc::Receiver<ToKeeper>,
    to_keeper: mpsc::Sender<ToKeeper>,
    owners: HashMap<u64, (MutatorKey, mpsc::Sender<ToOwner>)>,
}

impl<'k, 'service, 'heap> TestKeeper<'k, 'service, 'heap> {
    fn new(era: &'k KeeperEra<'service, 'heap>) -> Self {
        let (to_keeper, inbox) = mpsc::channel();
        Self {
            era,
            inbox,
            to_keeper,
            owners: HashMap::new(),
        }
    }

    fn enroll(&mut self, owner: u64) -> OwnerKit<'k> {
        let permit = self.era.register(owner, 0).expect("register the owner");
        let (grant_tx, grants) = mpsc::channel();
        self.owners.insert(owner, (permit.key(), grant_tx));
        OwnerKit {
            permit,
            home: SerialHome {
                owner,
                heap: self.era.heap_identity(),
            },
            to_keeper: self.to_keeper.clone(),
            grants,
        }
    }

    fn next(&self) -> ToKeeper {
        self.inbox
            .recv_timeout(BOUND)
            .expect("an owner message within the bound")
    }

    fn send(&self, owner: u64, message: ToOwner) {
        self.owners[&owner]
            .1
            .send(message)
            .expect("the owner is waiting for its grant");
    }

    /// Take one request from each of OWNERS, in any arrival order, commit
    /// each at the ring tail, and return them in OWNERS order.
    fn requests(&self, owners: &[u64]) -> Vec<Turn> {
        let mut got: HashMap<u64, PublishedOwner> = HashMap::new();
        while got.len() < owners.len() {
            match self.next() {
                ToKeeper::Request(owner, receipt) if owners.contains(&owner) => {
                    assert!(got.insert(owner, receipt).is_none(), "{owner} asked twice");
                }
                other => panic!(
                    "expected requests from {owners:?}, got {}",
                    other.describe()
                ),
            }
        }
        owners
            .iter()
            .map(|owner| {
                let receipt = got.remove(owner).expect("collected above");
                let (_, generation, _) = receipt.serial_facts();
                let key = self.owners[owner].0;
                let id = retry_native("commit the request", || {
                    self.era.enqueue_serial_for_test(key, generation)
                });
                Turn {
                    owner: *owner,
                    id,
                    receipt,
                }
            })
            .collect()
    }

    /// Grant the ring head while no stop is open.
    fn grant(&self, id: u64) {
        let (_, head, granted) = retry_native("grant outside a stop", || {
            self.era.serial_stop_priority_for_test()
        });
        assert_eq!((head, granted), (Some(id), Some(id)), "the head is granted");
    }

    fn give_back_from(&self, owner: u64) -> ReturnedSerialTicket {
        match self.next() {
            ToKeeper::GiveBack(from, returned) if from == owner => returned,
            other => panic!(
                "expected {owner}'s returned ticket, got {}",
                other.describe()
            ),
        }
    }

    /// One stop: lend each turn in order, each after the previous ticket came
    /// back and its ring entry retired, then complete. Returns the epoch.
    fn serve_stop(&self, turns: Vec<Turn>) -> NonZeroU64 {
        let stopped = stop_retrying(self.era);
        let epoch = stopped.epoch();
        with_stop_scope(|scope| {
            let bound = StoppedKeeper::from_scope(scope, stopped).unwrap_or_else(
                |(_scope, stopped, reason)| {
                    drop(stopped);
                    panic!("bind the stop: {reason:?}");
                },
            );
            // Every owner's readiness is checked before the first lend
            // consumes the stopped keeper.
            let ready: Vec<_> = turns
                .into_iter()
                .map(|turn| {
                    let request = bound
                        .ready_request(turn.receipt)
                        .unwrap_or_else(|reason| panic!("{} not ready: {reason:?}", turn.owner));
                    (turn.owner, turn.id, request)
                })
                .collect();
            let mut ready = ready.into_iter();
            let (mut owner, mut id, first) = ready.next().expect("a stop with a turn");
            let mut lent = match bound.lend_serial(first) {
                Ok(lent) => lent,
                Err(failure) => {
                    let _ = failure.complete_traced();
                    panic!("lend to {owner}");
                }
            };
            loop {
                let (ticket, held) = StoppedSerialTicket::from_lend(lent);
                assert_eq!(ticket.identity().epoch, epoch, "cut in this stop");
                self.send(owner, ToOwner(ticket, epoch));
                let returned = self.give_back_from(owner);
                let restored = match held.accept_return(returned) {
                    Ok(restored) => restored,
                    Err((_held, wrong)) => panic!("not the cut ticket: {:?}", wrong.identity()),
                };
                retry_native("retire the returned turn's entry", || {
                    self.era.retire_serial_head_for_test(id)
                });
                let (_, head) =
                    retry_native("read the ring", || self.era.serial_ring_head_for_test());
                assert_ne!(head, Some(id), "the ring has no entry for {owner}");
                match ready.next() {
                    Some((next_owner, next_id, request)) => {
                        lent = match restored.lend_next(request) {
                            Ok(lent) => lent,
                            Err(failure) => {
                                let _ = failure.complete_traced();
                                panic!("lend next to {next_owner}");
                            }
                        };
                        (owner, id) = (next_owner, next_id);
                    }
                    None => {
                        complete_retrying("complete the stop", restored.complete_traced());
                        break;
                    }
                }
            }
        });
        epoch
    }

    /// A stop with nothing to lend.
    fn bare_stop(&self) -> NonZeroU64 {
        let stopped = stop_retrying(self.era);
        let epoch = stopped.epoch();
        complete_retrying("complete the bare stop", stopped.complete());
        epoch
    }
}

/// Run BODY as the keeper of a fresh era on its own service thread, then
/// return the era and tear the fixture down.
fn with_live_era(
    label: &'static str,
    body: impl for<'k, 'service, 'heap> FnOnce(&'k KeeperEra<'service, 'heap>) + Send + 'static,
) {
    let Some(mut source) = ready() else {
        return;
    };
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

/// Begin a turn on a fresh request and run FORM in it.
fn first_turn<'t>(
    state: &'t mut State,
    home: SerialHome,
    port: &mut LivePort<'_>,
    form: &str,
) -> super::LiveSerial<'t, Context> {
    let grant = port
        .request_turn()
        .ok()
        .expect("the first request is granted");
    let mut turn = begin_turn(state, home, grant, port, standalone)
        .ok()
        .expect("the first grant begins");
    turn.with_world(|ev| eval(ev, form));
    turn
}

fn park<'t>(
    turn: super::LiveSerial<'t, Context>,
    port: &mut LivePort<'_>,
    wait: &'static str,
) -> ParkedSerial<'t, Context, Wake> {
    turn.park(Wake(wait), port).ok().expect("the host detaches")
}

const O: u64 = 7201;
const B: u64 = 7202;
const P: u64 = 7203;

/// R1 + R2 + R3. O parks inside stop S1 and its ticket comes back there:
/// the keeper retires O's entry and lends B in the same stop. S1 completes
/// while O is parked, and a running peer P makes progress. O wakes,
/// re-requests on the same port, and resumes in a later stop with its turn
/// state intact: the dynamic binding it made before the park, its match
/// data, and the global it set.
#[test]
fn e52_live_a_parked_turn_gives_its_stop_back_and_resumes_in_a_later_one() {
    with_live_era("e52-live-r1", |era| {
        let mut keeper = TestKeeper::new(era);
        let o = keeper.enroll(O);
        let b = keeper.enroll(B);
        let peer_permit = era.register(P, 0).expect("register P");
        let heap = era.heap_identity();
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        let progress = AtomicU64::new(0);
        let done = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let parked_owner = scope.spawn(move || {
                o.run(|state, home, port| {
                    let mut turn = first_turn(
                        state,
                        home,
                        port,
                        "(progn (setq sp-live 2) (string-match \"b+\" \"abbbc\"))",
                    );
                    let count = turn.with_world(|ev| {
                        let count = ev.specpdl.len();
                        ev.try_specbind(intern("sp-bound"), Value::T)
                            .expect("bind sp-bound");
                        count
                    });
                    let parked = park(turn, port, "mutex");
                    wake_rx.recv_timeout(BOUND).expect("woken");
                    let mut turn = parked
                        .resume(port, standalone)
                        .ok()
                        .expect("the later grant resumes");
                    let seen = turn.with_world(|ev| {
                        let seen = eval(ev, "(list sp-live sp-bound (match-end 0))");
                        ev.unbind_to(count);
                        (seen, eval(ev, "sp-bound"))
                    });
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            let other = scope.spawn(move || {
                b.run(|state, home, port| {
                    let mut turn = first_turn(state, home, port, "(setq sp-live 20)");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            let (progress, done) = (&progress, &done);
            let peer =
                scope.spawn(move || run_peer(activate(peer_permit, heap, P), progress, done));

            let turns = keeper.requests(&[O, B]);
            keeper.grant(turns[0].id);
            let s1 = keeper.serve_stop(turns);
            assert!(
                matches!(keeper.inbox.try_recv(), Err(mpsc::TryRecvError::Empty)),
                "the parked owner holds nothing and asks for nothing"
            );
            let before = progress.load(Ordering::Acquire);
            let deadline = Instant::now() + BOUND;
            while progress.load(Ordering::Acquire) < before + 8 {
                assert!(Instant::now() < deadline, "the peer ran while O was parked");
                std::thread::yield_now();
            }

            wake_tx.send(()).expect("wake O");
            let turns = keeper.requests(&[O]);
            keeper.grant(turns[0].id);
            let s2 = keeper.serve_stop(turns);
            assert!(s2 > s1, "O resumed in a later stop");

            done.store(true, Ordering::Release);
            assert_eq!(other.join().expect("B"), "20", "B ran in S1");
            assert_eq!(
                parked_owner.join().expect("O"),
                ("(2 t 4)".to_string(), "nil".to_string()),
                "O's binding, global and match data survived the park; unbinding \
                 after the resume restores the value the binding hid"
            );
            peer.join().expect("P");
        });
    });
}

/// R4. O's wait has already fired when the park returns, so it re-requests
/// at once. Its return and its request travel the same port, so the keeper
/// retires O's entry before the request is committed: no `Identity`.
#[test]
fn e52_live_an_immediate_wake_re_requests_on_the_same_port() {
    with_live_era("e52-live-r4", |era| {
        let mut keeper = TestKeeper::new(era);
        let o = keeper.enroll(O);
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                o.run(|state, home, port| {
                    let turn = first_turn(state, home, port, "(setq sp-live 4)");
                    let parked = park(turn, port, "fired");
                    let mut turn = parked
                        .resume(port, standalone)
                        .ok()
                        .expect("the next grant resumes");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            let turns = keeper.requests(&[O]);
            keeper.grant(turns[0].id);
            let s1 = keeper.serve_stop(turns);
            let turns = keeper.requests(&[O]);
            keeper.grant(turns[0].id);
            let s2 = keeper.serve_stop(turns);
            assert!(s2 > s1);
            assert_eq!(owner.join().expect("O"), "4");
        });
    });
}

/// R5. A stop with nothing to lend runs while O is parked and not queued.
/// O is NativeStopped for the whole wait, so the stop completes without
/// O's ack, and O's next grant names the stop after it.
#[test]
fn e52_live_a_stop_while_parked_needs_no_ack_from_the_parked_owner() {
    with_live_era("e52-live-r5", |era| {
        let mut keeper = TestKeeper::new(era);
        let o = keeper.enroll(O);
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                o.run(|state, home, port| {
                    let turn = first_turn(state, home, port, "(setq sp-live 5)");
                    let parked = park(turn, port, "condvar");
                    wake_rx.recv_timeout(BOUND).expect("woken");
                    let mut turn = parked
                        .resume(port, standalone)
                        .ok()
                        .expect("the next grant resumes");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            let turns = keeper.requests(&[O]);
            keeper.grant(turns[0].id);
            let s1 = keeper.serve_stop(turns);
            let bare = keeper.bare_stop();
            assert!(bare > s1);
            assert!(
                matches!(keeper.inbox.try_recv(), Err(mpsc::TryRecvError::Empty)),
                "the parked owner took no part in the bare stop"
            );
            wake_tx.send(()).expect("wake O");
            let turns = keeper.requests(&[O]);
            keeper.grant(turns[0].id);
            let s2 = keeper.serve_stop(turns);
            assert!(s2 > bare, "O's grant names the stop after the bare one");
            assert_eq!(owner.join().expect("O"), "5");
        });
    });
}

/// R6. O resumes on B's real ticket. O refuses it unopened and keeps its
/// parked turn; the ticket goes back to the keeper, which hands it to B, and
/// B's turn runs in the same stop. The refused grant ends O's request; O
/// requests again and resumes in the next stop.
#[test]
fn e52_live_another_owners_real_ticket_is_refused_and_both_come_back() {
    with_live_era("e52-live-r6", |era| {
        let mut keeper = TestKeeper::new(era);
        let o = keeper.enroll(O);
        let b = keeper.enroll(B);
        let (wake_tx, wake_rx) = mpsc::channel::<()>();
        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let owner = scope.spawn(move || {
                o.run(|state, home, port| {
                    let turn = first_turn(state, home, port, "(setq sp-live 6)");
                    let parked = park(turn, port, "join");
                    wake_rx.recv_timeout(BOUND).expect("woken");
                    let (parked, why) = match parked.resume(port, standalone) {
                        Ok(_) => panic!("B's ticket must not resume O's turn"),
                        Err(refused) => refused,
                    };
                    assert_eq!(why, TurnStartRefusal::Receive(ReceiveReason::WrongOwner));
                    assert_eq!(*parked.wait(), Wake("join"), "the parked turn came back");
                    let mut turn = parked
                        .resume(port, standalone)
                        .ok()
                        .expect("its own ticket resumes");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            let other = scope.spawn(move || {
                // B asks at once: an activated owner that neither polls nor
                // publishes would hold every stop open.
                b.run(|state, home, port| {
                    let mut turn = first_turn(state, home, port, "(setq sp-live 60)");
                    let seen = turn.with_world(|ev| eval(ev, "sp-live"));
                    turn.end(port).ok().expect("the host detaches");
                    seen
                })
            });
            let mut turns = keeper.requests(&[O, B]).into_iter();
            let (first, b_turn) = (turns.next().unwrap(), turns.next().unwrap());
            keeper.grant(first.id);
            let s1 = keeper.serve_stop(vec![first]);

            // B stays queued and NativeStopped across S1; its receipt is
            // still current. O's new entry queues behind it.
            wake_tx.send(()).expect("wake O");
            let o_turn = keeper.requests(&[O]).pop().expect("O's request");
            keeper.grant(b_turn.id);
            let stopped = stop_retrying(keeper.era);
            let s2 = stopped.epoch();
            assert!(s2 > s1);
            with_stop_scope(|scope| {
                let bound = StoppedKeeper::from_scope(scope, stopped).unwrap_or_else(
                    |(_scope, stopped, reason)| {
                        drop(stopped);
                        panic!("bind S2: {reason:?}");
                    },
                );
                drop(o_turn.receipt);
                let request = bound.ready_request(b_turn.receipt).expect("B is ready");
                let lent = match bound.lend_serial(request) {
                    Ok(lent) => lent,
                    Err(failure) => {
                        let _ = failure.complete_traced();
                        panic!("lend to B");
                    }
                };
                let (ticket, held) = StoppedSerialTicket::from_lend(lent);
                // Misrouted on purpose: B's ticket goes to O.
                keeper.send(O, ToOwner(ticket, s2));
                let ticket = match keeper.next() {
                    ToKeeper::Refuse(O, ticket, ReceiveReason::WrongOwner) => ticket,
                    other => panic!("expected O's refusal, got {}", other.describe()),
                };
                assert_eq!(ticket.identity().owner, B, "the refused ticket is B's own");
                keeper.send(B, ToOwner(ticket, s2));
                let returned = keeper.give_back_from(B);
                let restored = match held.accept_return(returned) {
                    Ok(restored) => restored,
                    Err((_held, wrong)) => panic!("not the cut ticket: {:?}", wrong.identity()),
                };
                // B is the head, O queues behind it; the refused grant ends
                // O's request.
                for id in [b_turn.id, o_turn.id] {
                    retry_native("retire", || keeper.era.retire_serial_head_for_test(id));
                }
                complete_retrying("complete S2", restored.complete_traced());
            });

            let turns = keeper.requests(&[O]);
            keeper.grant(turns[0].id);
            let s3 = keeper.serve_stop(turns);
            assert!(s3 > s2);
            assert_eq!(
                other.join().expect("B"),
                "60",
                "B ran on its forwarded ticket"
            );
            assert_eq!(owner.join().expect("O"), "6", "O resumed its own turn");
        });
    });
}

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

impl<'era> DriverKit<'era> {
    /// The owner's standalone Context, kept dormant, and its product port.
    fn start(self) -> (State, SerialHome, ChannelPort<'era>) {
        let mut ev = Context::new(crate::test_utils::unit_runtime());
        eval(
            &mut ev,
            "(progn (defvar sp-live 0) (setq sp-live 1) (defvar sp-bound nil))",
        );
        let state = OwnerSerialState::new(self.home.owner, Box::new(ev))
            .ok()
            .expect("a freshly built Context detaches");
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
    let Some(mut source) = ready() else {
        return;
    };
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

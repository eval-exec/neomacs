//! L6.7b b3 seat rows: seated Green host park/enter/exit under `WorldTurn`,
//! A-1 (`WorldHeldElsewhere`), A-3 (`WrongHeap`), alternation with unseated
//! `SerialHost`, and cross-thread `WorldLend::give_back` + K7 unwind over the
//! real `Driver` + `KeeperEra`.

use super::{ActiveWorldSeat, ParkedWorldSeat, WorldSeatRefusal};
use crate::emacs_core::eval::parallel_entry::keeper_driver::{
    Breach, Driver, DriverEnd, GroupPort, KeeperMsg, LendDrop, WorldLend,
};
use crate::emacs_core::eval::parallel_entry::owned_source::era_fixture::{
    KeeperEra, KeeperPanicGuard, activate, close, finish_fixture, keeper_serve, ready, retire,
    retry_native,
};
use crate::emacs_core::eval::parallel_entry::serial_turn::OwnerSerialState;
use crate::emacs_core::eval::parallel_entry::world_turn::{
    WorldReturned, WorldTurn, WorldTurnIdentity,
};
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use neovm_value::tagged::gc::{HeapIdentity, Mutator, RendezvousError};
use neovm_value::value::Value;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BOUND: Duration = Duration::from_secs(30);

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn names_no_heap() -> bool {
    !neovm_value::tagged::gc::tagged_heap_is_installed()
}

fn nonzero(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).expect("epoch starts at 1")
}

fn foreign_heap_for(real: HeapIdentity) -> HeapIdentity {
    let candidate = HeapIdentity::from_legacy_word(1).expect("nonzero");
    if candidate == real {
        HeapIdentity::from_legacy_word(2).expect("nonzero")
    } else {
        candidate
    }
}

fn seated_green_main() -> Context {
    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let baton = main.start_green_runtime().expect("start Green runtime");
    main.seat_green_main_for_test(baton);
    main
}

fn make_test_turn(
    heap: HeapIdentity,
    epoch: u64,
) -> (WorldTurn, WorldLend, mpsc::Receiver<KeeperMsg>) {
    let (tx, rx) = mpsc::channel();
    let turn = WorldTurn::cut_for_test(WorldTurnIdentity {
        heap,
        epoch: nonzero(epoch),
    });
    let lend = WorldLend::for_test(tx);
    (turn, lend, rx)
}

#[test]
fn l67b_b3_world_seat_refuses_unseated_and_sim_contexts() {
    let mut sim = Context::new(crate::test_utils::unit_runtime());
    assert_eq!(
        ParkedWorldSeat::park_seated(&mut sim).unwrap_err(),
        WorldSeatRefusal::Unseated,
        "a Sim Context has no Green seat"
    );
    drop(sim);

    let mut unseated_green =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let _baton = unseated_green
        .start_green_runtime()
        .expect("start Green runtime");
    assert_eq!(
        ParkedWorldSeat::park_seated(&mut unseated_green).unwrap_err(),
        WorldSeatRefusal::Unseated,
        "an unseated Green Context belongs to SerialHost, not WorldSeat"
    );
}

#[test]
fn l67b_b3_world_seat_refuses_wrong_heap_and_keeps_turn_intact() {
    let mut main = seated_green_main();
    let real_heap = main.tagged_heap.heap_identity();
    let foreign_heap = foreign_heap_for(real_heap);

    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    assert!(names_no_heap(), "park_seated takes carrier TLS down");
    assert!(
        main.serial_world_is_stowed(),
        "park_seated stows World carriers"
    );

    let (turn, lend, rx) = make_test_turn(foreign_heap, 7);
    let (parked, turn, lend, refusal) = parked
        .enter(&mut main, turn, lend)
        .expect_err("A-3: foreign heap WorldTurn must be refused");
    assert_eq!(refusal, WorldSeatRefusal::WrongHeap);
    assert_eq!(turn.identity().heap, foreign_heap);
    assert!(names_no_heap(), "refusal keeps carrier TLS parked");
    assert!(
        main.serial_world_is_stowed(),
        "refusal keeps World carriers stowed"
    );
    lend.give_back(WorldReturned::from_end(turn));
    assert!(matches!(rx.try_recv(), Ok(KeeperMsg::WorldReturn { .. })));

    // Now enter with the real heap to restore main cleanly before drop.
    let (turn, lend, _rx) = make_test_turn(real_heap, 8);
    let active = parked
        .enter(&mut main, turn, lend)
        .expect("matching heap enters");
    assert_eq!(active.identity().epoch, nonzero(8));
    let (_parked, lend, returned) = active.exit(&mut main);
    lend.give_back(returned);
}

#[test]
fn l67b_b3_world_seat_refuses_enter_when_peer_holds_world_carriers_a1() {
    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    assert_eq!(eval(&mut main, "(lossage-size 419)"), "419");
    let real_heap = main.tagged_heap.heap_identity();
    let baton = main.start_green_runtime().expect("start Green runtime");

    // Spawn an unseated parallel owner on the same Green World before seating
    // main, so both share the exact same GreenRuntime and HeapIdentity.
    let (baton_back_tx, baton_back_rx) = mpsc::channel();
    let (peer_ready_tx, peer_ready_rx) = mpsc::channel();
    let (peer_begin_tx, peer_begin_rx) = mpsc::channel();
    let (peer_holding_tx, peer_holding_rx) = mpsc::channel();
    let (peer_release_tx, peer_release_rx) = mpsc::channel();

    let worker = std::thread::spawn(move || {
        baton.enter();
        let owner = Context::new_green_owner(&baton);
        let mut state: OwnerSerialState<Context, (u64, &'static str)> =
            OwnerSerialState::new(7, Box::new(owner))
                .ok()
                .expect("peer detaches cleanly while main still holds World");
        baton_back_tx.send(baton).expect("return baton to main");
        peer_ready_tx.send(()).expect("peer ready");

        peer_begin_rx.recv().expect("signal peer begin");
        let mut turn = state
            .begin((7, "peer-turn"))
            .ok()
            .expect("peer acquires stowed World carriers");
        assert_eq!(turn.with_world(|ev| eval(ev, "(lossage-size)")), "419");
        peer_holding_tx.send(()).expect("peer holding World");

        peer_release_rx.recv().expect("signal peer release");
        let _ = turn.end().ok().expect("peer detaches back to Stowed");
        drop(state);
    });

    let baton = baton_back_rx.recv().expect("baton back");
    peer_ready_rx.recv().expect("peer ready");
    baton.enter();
    main.seat_green_main_for_test(baton);

    // Park seated main -> World carriers are now Stowed.
    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    assert!(main.serial_world_is_stowed());

    // Let the parallel owner acquire the World carriers.
    peer_begin_tx.send(()).expect("start peer");
    peer_holding_rx.recv().expect("peer holding World");
    assert!(
        main.serial_world_held_elsewhere(),
        "peer holds the World carriers"
    );

    // A-1: while the peer holds the World carriers, WorldSeat::enter MUST refuse.
    let (turn, lend, _rx) = make_test_turn(real_heap, 11);
    let (parked, turn, lend, refusal) = parked
        .enter(&mut main, turn, lend)
        .expect_err("A-1: WorldSeat::enter must refuse while peer holds World");
    assert_eq!(refusal, WorldSeatRefusal::WorldHeldElsewhere);
    assert!(names_no_heap(), "refusal keeps main thread TLS parked");

    // Release the peer -> World carriers return to Stowed -> enter succeeds!
    peer_release_tx.send(()).expect("release peer");
    worker.join().expect("worker finishes");
    assert!(main.serial_world_is_stowed());

    let active = parked
        .enter(&mut main, turn, lend)
        .expect("enter succeeds once peer stowed World carriers");
    assert_eq!(eval(&mut main, "(lossage-size)"), "419");
    let (_parked, lend, returned) = active.exit(&mut main);
    lend.give_back(returned);
}

#[test]
fn l67b_b3_world_seat_stows_and_alternates_with_unseated_serial_host() {
    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    assert_eq!(eval(&mut main, "(lossage-size 419)"), "419");
    main.display_var_change_count = 77;
    let real_heap = main.tagged_heap.heap_identity();
    let baton = main.start_green_runtime().expect("start Green runtime");

    let (baton_back_tx, baton_back_rx) = mpsc::channel();
    let (peer_go_tx, peer_go_rx) = mpsc::channel();
    let (peer_done_tx, peer_done_rx) = mpsc::channel();

    let worker = std::thread::spawn(move || {
        baton.enter();
        let owner = Context::new_green_owner(&baton);
        let mut state: OwnerSerialState<Context, (u64, &'static str)> =
            OwnerSerialState::new(9, Box::new(owner))
                .ok()
                .expect("peer detaches");
        baton_back_tx.send(baton).expect("return baton");

        peer_go_rx.recv().expect("peer turn");
        let mut turn = state
            .begin((9, "t1"))
            .ok()
            .expect("peer acquires World stowed by WorldSeat");
        turn.with_world(|ev| {
            assert_eq!(eval(ev, "(lossage-size)"), "419");
            assert_eq!(ev.display_var_change_count, 77);
            assert_eq!(eval(ev, "(lossage-size 527)"), "527");
            ev.display_var_change_count = 88;
        });
        let _ = turn.end().ok().expect("peer detaches back to Stowed");
        drop(state);
        peer_done_tx.send(()).expect("peer done");
    });

    let baton = baton_back_rx.recv().expect("baton back");
    baton.enter();
    main.seat_green_main_for_test(baton);

    // Park seated main (SerialHost::detach would refuse HostRefusal::Seated,
    // while ParkedWorldSeat::park_seated accepts seated Green hosts).
    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    assert!(names_no_heap());
    assert!(main.serial_world_is_stowed());

    // Parallel owner runs its serial turn and updates World carriers.
    peer_go_tx.send(()).expect("run peer");
    peer_done_rx.recv().expect("peer finished");
    worker.join().expect("worker joined");
    assert!(main.serial_world_is_stowed());

    // Baton seat enters under WorldTurn, observes peer's updates, mutates, exits.
    let (turn, lend, rx) = make_test_turn(real_heap, 2);
    let active: ActiveWorldSeat = parked
        .enter(&mut main, turn, lend)
        .expect("enter seated main");
    assert_eq!(active.identity().epoch, nonzero(2));
    assert_eq!(eval(&mut main, "(lossage-size)"), "527");
    assert_eq!(main.display_var_change_count, 88);
    assert_eq!(eval(&mut main, "(lossage-size 613)"), "613");
    main.display_var_change_count = 99;

    let (_parked, lend, returned) = active.exit(&mut main);
    assert!(names_no_heap(), "exit parks carrier TLS");
    assert!(main.serial_world_is_stowed(), "exit stows World carriers");
    assert_eq!(returned.identity().epoch, nonzero(2));
    assert_eq!(returned.identity().heap, real_heap);
    lend.give_back(returned);
    assert!(matches!(rx.try_recv(), Ok(KeeperMsg::WorldReturn { .. })));
}

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
            tx.send(body(era)).expect("send outcome");
        });
        drop(outcome);
    });
    let report = rx.recv_timeout(BOUND).expect("keeper outcome");
    service.join().expect("keeper service thread");
    report
}

fn run_peer(mut owner: Mutator<'_>, progress: &AtomicU64, done: &AtomicBool) {
    while !done.load(Ordering::Acquire) {
        match owner.wait_request_for(Duration::from_millis(2)) {
            Ok(_epoch) => {
                retry_native("ack stop", || owner.publish_native_owner(&[]).map(|_| ()));
                retry_native("run after stop", || owner.leave_native().map(|_| ()));
            }
            Err(RendezvousError::Busy) => {
                progress.fetch_add(1, Ordering::AcqRel);
            }
            Err(reason) => panic!("running peer: {reason:?}"),
        }
    }
    retire(owner);
}

fn enroll_group(driver: &mut Driver<'_, '_, '_>, inbox: &mpsc::Sender<KeeperMsg>) -> GroupPort {
    let (world_tx, world_rx) = mpsc::sync_channel(1);
    driver.enroll_group(world_tx);
    GroupPort::new(inbox.clone(), world_rx)
}

#[test]
fn l67b_b3_world_lend_handoff_to_second_baton_thread_and_give_back() {
    with_live_era("l67b-b3-handoff", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let peer = era.register(10, 0).expect("register peer");
        let heap = era.heap_identity();
        let mut group = enroll_group(&mut driver, &inbox);
        drop(inbox);

        let progress = AtomicU64::new(0);
        let done = AtomicBool::new(false);

        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let (progress, done) = (&progress, &done);
            scope.spawn(move || run_peer(activate(peer, heap, 10), progress, done));

            let (handoff_tx, handoff_rx) = mpsc::channel::<(WorldTurn, WorldLend)>();
            let (done_tx, done_rx) = mpsc::channel::<(u64, u64)>();

            // Baton thread A requests the WorldTurn and hands it off to Baton thread B.
            scope.spawn(move || {
                let start = Instant::now();
                while progress.load(Ordering::Acquire) == 0 {
                    assert!(start.elapsed() < BOUND, "peer advances");
                    std::thread::yield_now();
                }
                let (turn, _epoch, lend) =
                    group.request_world().ok().expect("group lent WorldTurn");
                assert_eq!(turn.identity().heap, heap);
                handoff_tx.send((turn, lend)).expect("hand off to thread B");
                let (_before, during) = done_rx.recv_timeout(BOUND).expect("thread B done");
                let start = Instant::now();
                while progress.load(Ordering::Acquire) <= during {
                    assert!(start.elapsed() < BOUND, "peer resumes after give_back");
                    std::thread::yield_now();
                }
                done.store(true, Ordering::Release);
                group.exit();
            });

            // Baton thread B receives (WorldTurn, WorldLend) and returns it directly
            // via WorldLend::give_back without needing &mut GroupPort.
            let thread_b = scope.spawn(move || {
                let (turn, lend) = handoff_rx.recv_timeout(BOUND).expect("receive handoff");
                let before = progress.load(Ordering::Acquire);
                std::thread::sleep(Duration::from_millis(25));
                let during = progress.load(Ordering::Acquire);
                assert_eq!(
                    before, during,
                    "parallel owner frozen while thread B holds WorldTurn"
                );
                lend.give_back(WorldReturned::from_end(turn));
                done_tx.send((before, during)).expect("notify thread A");
            });

            assert_eq!(driver.run(), DriverEnd::Drained);
            thread_b.join().expect("thread B");
        });
    });
}

#[test]
fn l67b_b3_panic_on_second_baton_thread_poisons_via_send_world_lend() {
    let end = with_poisoned_era("l67b-b3-b2-panic", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let mut group = enroll_group(&mut driver, &inbox);
        drop(inbox);

        std::thread::scope(|scope| {
            let (handoff_tx, handoff_rx) = mpsc::channel::<(WorldTurn, WorldLend)>();
            let (unwound_tx, unwound_rx) = mpsc::channel::<()>();
            let thread_a = scope.spawn(move || {
                let (turn, _epoch, lend) =
                    group.request_world().ok().expect("group lent WorldTurn");
                handoff_tx.send((turn, lend)).expect("hand off to thread B");
                unwound_rx.recv_timeout(BOUND).expect("thread B unwound");
                group.exit();
            });
            let thread_b = scope.spawn(move || {
                let (_turn, _lend) = handoff_rx.recv_timeout(BOUND).expect("receive handoff");
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    let _held = (_turn, _lend);
                    panic!("second Baton thread panics mid-interval");
                }));
                assert!(caught.is_err());
                unwound_tx.send(()).expect("notify thread A");
            });
            let end = driver.run();
            thread_b.join().expect("thread B");
            thread_a.join().expect("thread A");
            end
        })
    });
    assert_eq!(
        end,
        DriverEnd::Poisoned(Breach::GroupAbandoned {
            cause: LendDrop::Unwound,
        })
    );
}

#[test]
fn l67b_b3_world_seat_refuses_enter_when_another_heap_is_installed_on_thread() {
    // Build and park an unrelated Context before starting the Green runtime so
    // we can install its heap on this OS thread while `main` is parked.
    let mut other_state: OwnerSerialState<Context, (u64, &'static str)> = OwnerSerialState::new(
        99,
        Box::new(Context::new(crate::test_utils::unit_runtime())),
    )
    .ok()
    .expect("park other Context");
    assert!(names_no_heap());

    let mut main = seated_green_main();
    let real_heap = main.tagged_heap.heap_identity();
    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    assert!(names_no_heap());

    // Install `other`'s heap on this OS thread via a live turn.
    let mut other_turn = other_state
        .begin((99, "other-live"))
        .ok()
        .expect("begin other turn");
    assert!(
        other_turn
            .with_world(|ev| neovm_value::tagged::gc::tagged_heap_is_current(&ev.tagged_heap))
    );

    let (turn, lend, _rx) = make_test_turn(real_heap, 5);
    let (parked, turn, lend, refusal) = parked
        .enter(&mut main, turn, lend)
        .expect_err("enter must refuse while another heap is installed on this thread");
    assert_eq!(refusal, WorldSeatRefusal::HeapInstalled);
    assert!(
        other_turn
            .with_world(|ev| neovm_value::tagged::gc::tagged_heap_is_current(&ev.tagged_heap)),
        "refusal leaves the live turn's heap untouched"
    );

    let _ = other_turn.end().ok().expect("end other turn");
    assert!(names_no_heap());
    let active = parked
        .enter(&mut main, turn, lend)
        .expect("enter succeeds once thread has no heap installed");
    let (_parked, lend, returned) = active.exit(&mut main);
    lend.give_back(returned);
}

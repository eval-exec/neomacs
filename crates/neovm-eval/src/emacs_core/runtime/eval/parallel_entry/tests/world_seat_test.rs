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
    GroupPort::new(inbox.clone(), driver.enroll_group())
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

fn with_live_green_era(
    label: &'static str,
    body: impl for<'k, 'service, 'heap> FnOnce(&'k KeeperEra<'service, 'heap>) + Send + 'static,
) {
    let mut source = ready();
    let private = Value::string(label);
    source.lexenv = private;
    let bootstrap = source.capture_green_bootstrap_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |keeper| {
            keeper
                .keeper_baton_for_test()
                .install_stowed_green_runtime_for_test(bootstrap);
            body(keeper);
            // Fold any worker `Context::assemble` heap delta back into the
            // frozen legacy image before `EraCore::finish()` runs
            // `restore_legacy(heap)`.
            let mut stopped = loop {
                match keeper.stop() {
                    Ok(stopped) => break stopped,
                    Err(RendezvousError::Busy) => std::thread::yield_now(),
                    Err(other) => panic!("teardown stop: {other:?}"),
                }
            };
            keeper
                .collect_requested(&mut stopped)
                .expect("teardown collection folds worker heap deltas");
            stopped.complete().expect("complete teardown stop");
            keeper
                .keeper_baton_for_test()
                .uninstall_green_runtime_for_test();
        });
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

/// Full `unit_runtime()` live Green era with a pre-freeze cons allocated on
/// `source` AFTER `gc_collect_exact()` and BEFORE `prepare_era()` so
/// `entry0.image.young` is non-empty and `pending_mutators` contains `id_0`
/// (exercising `peer-gcfix`'s young-log merge in `restore_legacy_for_collection`
/// and `prepare_finish` without any extra teardown collection).
fn with_live_full_green_era_pre_freeze_cons(
    label: &'static str,
    setup: impl FnOnce(&mut Context),
    body: impl for<'k, 'service, 'heap> FnOnce(&'k KeeperEra<'service, 'heap>) + Send + 'static,
) {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    {
        crate::emacs_core::jit::force_jit_off_for_test(true);
        crate::emacs_core::jit::cache::reset_carrier_tls();
    }
    crate::emacs_core::eval::parallel_entry::source_entry::install_inactive_source_module_probe_for_test();
    let mut source = Box::new(Context::new_with_threading(
        ThreadingMode::Green,
        crate::test_utils::unit_runtime(),
    ));
    source.setup_thread_locals();
    setup(&mut source);
    let private = Value::string(label);
    source.lexenv = private;
    source.gc_collect_exact();
    assert!(
        source.tagged_heap.tagged_heap_generational_enabled(),
        "L6.7b Lisp-path rows require a generational heap"
    );
    // Allocate a young cons on `source` BEFORE `prepare_era()` with NO
    // intervening GC so `entry0.image.young` is non-empty when the era opens.
    let pre_freeze = source
        .eval_str("(cons 10 20)")
        .expect("pre-freeze cons on source");
    source.set_variable("l67b-b4-pre-freeze", pre_freeze);
    source.tagged_heap.close_alloc_regions();
    let bootstrap = source.capture_green_bootstrap_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |keeper| {
            keeper
                .keeper_baton_for_test()
                .install_stowed_green_runtime_for_test(bootstrap);
            body(keeper);
            keeper
                .keeper_baton_for_test()
                .uninstall_green_runtime_for_test();
        });
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

/// L6.7b b4 P1 (Row a — upgraded Lisp path, #14218/#14220): in a full
/// `unit_runtime()` live Green era with a pre-freeze cons on `source`
/// (`entry0.image.young` non-empty), a seated Baton `Context` conses via real
/// Lisp (`ctx.eval_str`) past one `ConsBlock` inside `WorldTurn`, keeps the
/// ONLY reference to a survivor cons in `ctx.lexenv` alongside an unrooted
/// garbage cons, collects via `GroupPort::collect`, verifies survivor +
/// pre-freeze cons survive and garbage is reclaimed, then performs post-collect
/// Lisp consing with NO further collection before `era.finish()`
/// (`prepare_finish` young-log merge).
#[test]
fn l67b_b4_parked_world_seat_roots_survive_group_collect_and_garbage_reclaims() {
    with_live_full_green_era_pre_freeze_cons(
        "l67b-b4-parked-roots",
        |_| {},
        |era| {
            let (inbox, inbox_rx) = mpsc::channel();
            let mut driver = Driver::new(era, inbox_rx);
            let mut group = enroll_group(&mut driver, &inbox);
            drop(inbox);

            let world_handle = era.keeper_baton_for_test().test_world_handle_for_test();
            let (ready_tx, ready_rx) = mpsc::channel::<()>();
            let (retire_tx, retire_rx) = mpsc::channel::<()>();
            let (words_tx, words_rx) =
                mpsc::sync_channel::<neovm_value::tagged::gc::HeapWord>(1);
            let (report_tx, report_rx) = mpsc::sync_channel(1);

            std::thread::scope(|scope| {
                let _panic_guard = KeeperPanicGuard { keeper: era };
                let runner = scope.spawn(move || {
                    let (mut ctx, retention) = world_handle.spawn_seated_context_for_test(1);
                    let parked = ParkedWorldSeat::park_seated_with_retention(&mut ctx, &retention)
                        .expect("park seated Baton worker");
                    drop(retention);
                    ready_tx.send(()).expect("signal seated worker parked");

                    let (turn, first_epoch, lend) =
                        group.request_world().ok().expect("group lent WorldTurn");
                    let active = parked
                        .enter(&mut ctx, turn, lend)
                        .expect("enter seated Baton worker");

                    // Context-local GC threshold only (never process-wide):
                    // raise threshold so in-turn Lisp consing crosses >1
                    // ConsBlock without triggering maybe_gc (covered by Row 2),
                    // then restore before leaving the era.
                    let saved_threshold = ctx
                        .eval_str("gc-cons-threshold")
                        .expect("read Context-local gc-cons-threshold");
                    ctx.eval_str("(setq gc-cons-threshold most-positive-fixnum)")
                        .expect("raise Context-local gc-cons-threshold");

                    assert_eq!(
                        eval(&mut ctx, "(length (make-list 5000 nil))"),
                        "5000",
                        "real Lisp consing inside WorldTurn crosses >1 ConsBlock"
                    );
                    let survivor = ctx
                        .eval_str("(cons 111 222)")
                        .expect("allocate survivor cons via Lisp inside WorldTurn");
                    let garbage = ctx
                        .eval_str("(cons 333 444)")
                        .expect("allocate garbage cons via Lisp inside WorldTurn");
                    // Positive pre-collect controls: both words live in the heap.
                    assert!(
                        ctx.tagged_heap.owns_heap_value_for_test(survivor),
                        "survivor cons must be owned before GroupPort::collect"
                    );
                    assert!(
                        ctx.tagged_heap.owns_heap_value_for_test(garbage),
                        "garbage cons must be owned before GroupPort::collect"
                    );
                    let survivor_word = survivor.heap_word_for_test();
                    words_tx.send(survivor_word).expect("publish survivor word");

                    // `ctx.lexenv` on this parked Baton Context is the SOLE root
                    // for `survivor`; `garbage` is kept unrooted so we can
                    // verify reclamation right after `GroupPort::collect`
                    // (before WorldTurn #2 allocates and can reuse its slot).
                    ctx.lexenv = survivor;

                    let (parked, lend, returned) = active.exit(&mut ctx);
                    let (turn2, second_epoch, lend2) = group
                        .collect(lend, returned, Some(report_tx))
                        .ok()
                        .expect("group resumes after collection");
                    assert!(second_epoch > first_epoch);

                    let report = report_rx
                        .recv_timeout(BOUND)
                        .expect("collection report")
                        .expect("collection succeeded");
                    assert_eq!(report.epoch, second_epoch.get());

                    let active2 = parked
                        .enter(&mut ctx, turn2, lend2)
                        .expect("re-enter seated Baton worker after collection");
                    // Check survivor & garbage BEFORE any WorldTurn #2 allocation
                    // can reuse `garbage`'s freed cons slot.
                    let survivor_alive = ctx.tagged_heap.owns_heap_value_for_test(ctx.lexenv);
                    let garbage_alive = ctx.tagged_heap.owns_heap_value_for_test(garbage);
                    let survivor_printed = survivor_alive.then(|| print_value(&ctx.lexenv));
                    assert_eq!(
                        eval(&mut ctx, "l67b-b4-pre-freeze"),
                        "(10 . 20)",
                        "pre-freeze source cons must survive GroupPort::collect"
                    );
                    // Post-collect Lisp consing with NO further collection
                    // before era.finish() (exercises prepare_finish young merge).
                    assert_eq!(
                        eval(&mut ctx, "(length (make-list 2000 nil))"),
                        "2000",
                        "post-collect Lisp consing succeeds before prepare_finish"
                    );
                    ctx.set_variable("gc-cons-threshold", saved_threshold);
                    ctx.lexenv = Value::NIL;
                    let (parked, lend2, returned2) = active2.exit(&mut ctx);
                    group.give_back(lend2, returned2);
                    group.exit();

                    retire_rx.recv_timeout(BOUND).expect("teardown signal");
                    parked.retire_parked_for_teardown_for_test(&mut ctx);
                    ctx.retire_green_owner();
                    (survivor_alive, garbage_alive, survivor_printed)
                });

                ready_rx
                    .recv_timeout(BOUND)
                    .expect("wait for seated worker to park before driver.run");
                assert_eq!(driver.run(), DriverEnd::Drained);
                let survivor_word = words_rx.recv_timeout(BOUND).expect("survivor word");

                let mut stopped = loop {
                    match era.stop() {
                        Ok(stopped) => break stopped,
                        Err(RendezvousError::Busy) => std::thread::yield_now(),
                        Err(other) => panic!("verify stop: {other:?}"),
                    }
                };
                let era_owns_survivor = era
                    .owns_heap_word_for_test(&stopped, survivor_word)
                    .expect("check survivor word");
                stopped.trace_serial().expect("trace verify stop");
                stopped.complete().expect("complete verify stop");

                retire_tx.send(()).expect("signal worker teardown");
                let (worker_saw_survivor, garbage_alive_after_collect, survivor_printed) =
                    runner.join().expect("runner joined");

                assert!(
                    era_owns_survivor,
                    "parked WorldSeat root in ctx.lexenv MUST survive GroupPort::collect"
                );
                assert!(
                    worker_saw_survivor,
                    "worker must still see live survivor after GroupPort::collect"
                );
                assert_eq!(survivor_printed.as_deref(), Some("(111 . 222)"));
                assert!(
                    !garbage_alive_after_collect,
                    "unreachable garbage cons MUST be reclaimed by GroupPort::collect"
                );
            });
        },
    );
}

/// L6.7b b4 Q1 + `world.owner_slabs` (Row a' — upgraded Lisp path,
/// #14218/#14220): in a full `unit_runtime()` live Green era with a pre-freeze
/// cons on `source`, a seated Baton `Context` conses via real Lisp
/// (`ctx.eval_str`) past one `ConsBlock` inside `WorldTurn` and holds the ONLY
/// reference to `survivor` in a projected dynamic binding on its `OwnerSlab`
/// (not in `collect_parked_roots()` private roots). `GroupPort::collect`
/// preserves the `OwnerSlab` binding and reclaims unrooted garbage, followed by
/// post-collect Lisp consing with no further collection before `era.finish()`.
#[test]
fn l67b_b4_parked_owner_slab_binding_survives_group_collect() {
    let symbol = neovm_value::intern::intern("l67b-b4-slab-binding");
    with_live_full_green_era_pre_freeze_cons(
        "l67b-b4-owner-slab-roots",
        move |source| {
            source.obarray.make_special_id(symbol);
            source.set_variable("l67b-b4-slab-binding", Value::NIL);
            source.set_threading_mode_for_test(ThreadingMode::Green);
            source.admit_threads().expect("admit thread projections");
            source
                .thread_projection_for_test(symbol)
                .expect("special variable projected for thread OwnerSlabs");
        },
        move |era| {
            let (inbox, inbox_rx) = mpsc::channel();
            let mut driver = Driver::new(era, inbox_rx);
            let mut group = enroll_group(&mut driver, &inbox);
            drop(inbox);

            let world_handle = era.keeper_baton_for_test().test_world_handle_for_test();
            let (ready_tx, ready_rx) = mpsc::channel::<()>();
            let (retire_tx, retire_rx) = mpsc::channel::<()>();
            let (words_tx, words_rx) =
                mpsc::sync_channel::<neovm_value::tagged::gc::HeapWord>(1);
            let (report_tx, report_rx) = mpsc::sync_channel(1);

            std::thread::scope(|scope| {
                let _panic_guard = KeeperPanicGuard { keeper: era };
                let runner = scope.spawn(move || {
                    let (mut ctx, retention) = world_handle.spawn_seated_context_for_test(3);
                    let parked = ParkedWorldSeat::park_seated_with_retention(&mut ctx, &retention)
                        .expect("park seated Baton worker");
                    drop(retention);
                    ready_tx.send(()).expect("signal seated worker parked");

                    let (turn, first_epoch, lend) =
                        group.request_world().ok().expect("group lent WorldTurn");
                    let active = parked
                        .enter(&mut ctx, turn, lend)
                        .expect("enter seated Baton worker");

                    let saved_threshold = ctx
                        .eval_str("gc-cons-threshold")
                        .expect("read Context-local gc-cons-threshold");
                    ctx.eval_str("(setq gc-cons-threshold most-positive-fixnum)")
                        .expect("raise Context-local gc-cons-threshold");

                    assert_eq!(
                        eval(&mut ctx, "(length (make-list 5000 nil))"),
                        "5000",
                        "real Lisp consing inside WorldTurn crosses >1 ConsBlock"
                    );
                    let survivor = ctx
                        .eval_str("(cons 111 222)")
                        .expect("allocate survivor cons via Lisp inside WorldTurn");
                    let garbage = ctx
                        .eval_str("(cons 333 444)")
                        .expect("allocate garbage cons via Lisp inside WorldTurn");
                    assert!(
                        ctx.tagged_heap.owns_heap_value_for_test(survivor),
                        "survivor cons must be owned before GroupPort::collect"
                    );
                    assert!(
                        ctx.tagged_heap.owns_heap_value_for_test(garbage),
                        "garbage cons must be owned before GroupPort::collect"
                    );
                    let survivor_word = survivor.heap_word_for_test();
                    words_tx.send(survivor_word).expect("publish survivor word");

                    let specpdl_base = ctx.specpdl.len();
                    ctx.try_specbind(symbol, survivor)
                        .expect("bind survivor on OwnerSlab");
                    assert_eq!(
                        ctx.owner.slab().expect("slab").records_of(symbol).count(),
                        1,
                        "binding lives in OwnerSlab"
                    );
                    // Prove survivor is NOT in private parked roots — ONLY in OwnerSlab.
                    let private_roots = ctx.collect_parked_roots();
                    assert!(
                        !private_roots
                            .iter()
                            .any(|v| v.bits() == survivor_word.bits()),
                        "OwnerSlab-only dynamic binding must not appear in private parked roots"
                    );
                    ctx.return_parked_root_buffer(private_roots);

                    let (parked, lend, returned) = active.exit(&mut ctx);
                    let (turn2, second_epoch, lend2) = group
                        .collect(lend, returned, Some(report_tx))
                        .ok()
                        .expect("group resumes after collection");
                    assert!(second_epoch > first_epoch);
                    let report = report_rx
                        .recv_timeout(BOUND)
                        .expect("collection report")
                        .expect("collection succeeded");
                    assert_eq!(report.epoch, second_epoch.get());

                    let active2 = parked
                        .enter(&mut ctx, turn2, lend2)
                        .expect("re-enter after collection");
                    // Check survivor & garbage BEFORE any WorldTurn #2 allocation
                    // can reuse `garbage`'s freed cons slot.
                    let bound_after = match ctx.find_symbol_value_by_id(symbol) {
                        Ok(crate::emacs_core::eval::SymbolValueLookup::Bound(val)) => val,
                        other => panic!("expected OwnerSlab binding after collect, got {other:?}"),
                    };
                    let survivor_alive = ctx.tagged_heap.owns_heap_value_for_test(bound_after);
                    let garbage_alive = ctx.tagged_heap.owns_heap_value_for_test(garbage);
                    let survivor_printed = survivor_alive.then(|| print_value(&bound_after));
                    assert_eq!(
                        eval(&mut ctx, "l67b-b4-pre-freeze"),
                        "(10 . 20)",
                        "pre-freeze source cons must survive GroupPort::collect"
                    );
                    assert_eq!(
                        eval(&mut ctx, "(length (make-list 2000 nil))"),
                        "2000",
                        "post-collect Lisp consing succeeds before prepare_finish"
                    );
                    ctx.set_variable("gc-cons-threshold", saved_threshold);
                    ctx.unbind_to(specpdl_base);

                    let (parked, lend2, returned2) = active2.exit(&mut ctx);
                    group.give_back(lend2, returned2);
                    group.exit();

                    retire_rx.recv_timeout(BOUND).expect("teardown signal");
                    parked.retire_parked_for_teardown_for_test(&mut ctx);
                    ctx.retire_green_owner();
                    (survivor_alive, garbage_alive, survivor_printed)
                });

                ready_rx.recv_timeout(BOUND).expect("worker parked");
                assert_eq!(driver.run(), DriverEnd::Drained);
                let survivor_word = words_rx.recv_timeout(BOUND).expect("survivor word");

                let mut stopped = loop {
                    match era.stop() {
                        Ok(stopped) => break stopped,
                        Err(RendezvousError::Busy) => std::thread::yield_now(),
                        Err(other) => panic!("verify stop: {other:?}"),
                    }
                };
                let era_owns_survivor = era
                    .owns_heap_word_for_test(&stopped, survivor_word)
                    .expect("check survivor word");
                stopped.trace_serial().expect("trace verify stop");
                stopped.complete().expect("complete verify stop");

                retire_tx.send(()).expect("signal worker teardown");
                let (worker_saw_survivor, garbage_alive_after_collect, survivor_printed) =
                    runner.join().expect("runner joined");

                assert!(
                    era_owns_survivor,
                    "OwnerSlab binding on parked WorldSeat MUST survive GroupPort::collect"
                );
                assert!(worker_saw_survivor);
                assert_eq!(survivor_printed.as_deref(), Some("(111 . 222)"));
                assert!(
                    !garbage_alive_after_collect,
                    "garbage cons MUST be reclaimed"
                );
            });
        },
    );
}

/// L6.7b b4 Q1 (Row b): `Baton::collect_parallel_world_roots` refuses
/// collection with `RendezvousError::Publication` while a live `GreenRuntime`
/// has `CarriedHolder::Held`, and succeeds once the seated host stows back to
/// `CarriedHolder::Stowed`.
#[test]
fn l67b_b4_collect_parallel_world_roots_refuses_held_green_runtime() {
    with_live_green_era("l67b-b4-held-refusal", |era| {
        let world_handle = era.keeper_baton_for_test().test_world_handle_for_test();
        let (mut ctx, retention) = world_handle.spawn_seated_context_for_test(2);
        assert!(
            !ctx.serial_world_is_stowed(),
            "active seated worker holds GreenRuntime carriers inline (CarriedHolder::Held)"
        );

        let mut stopped = loop {
            match era.stop() {
                Ok(stopped) => break stopped,
                Err(RendezvousError::Busy) => std::thread::yield_now(),
                Err(other) => panic!("stop for held-refusal check: {other:?}"),
            }
        };
        let err = era
            .collect_requested(&mut stopped)
            .expect_err("collection MUST refuse while GreenRuntime is CarriedHolder::Held");
        assert_eq!(err, RendezvousError::Publication);
        stopped.trace_serial().expect("trace held-refusal stop");
        stopped.complete().expect("complete held-refusal stop");

        // Stowing back to CarriedHolder::Stowed unblocks collection.
        let parked = ParkedWorldSeat::park_seated_with_retention(&mut ctx, &retention)
            .expect("stow before collect");
        drop(retention);
        assert!(ctx.serial_world_is_stowed());

        let mut stopped = loop {
            match era.stop() {
                Ok(stopped) => break stopped,
                Err(RendezvousError::Busy) => std::thread::yield_now(),
                Err(other) => panic!("stop after stow: {other:?}"),
            }
        };
        let report = era
            .collect_requested(&mut stopped)
            .expect("stowed GreenRuntime allows parallel collection");
        assert!(report.epoch >= 1);
        stopped.complete().expect("complete stowed collection stop");

        parked.retire_parked_for_teardown_for_test(&mut ctx);
        ctx.retire_green_owner();
    });
}

/// L6.7b b4 Q2 (Row c): when a reader holds an active read on a parked
/// `RootBatchLease`, `ParkedWorldSeat::enter` succeeds and fails closed by
/// keeping the lease live in `GreenRuntime::parked`; once the reader finishes,
/// retiring that deferred lease succeeds through `RootBatchLease::try_recycle`
/// (caching the recycled `PreparedRootBatch` on `GreenSeat`, NOT falling back
/// to `root_registry::finish`). A subsequent `switch_out` while a reader is
/// still held preserves the deferred lease in `retained`, and both retire
/// cleanly on the next safe `switch_in` once the reader finishes.
#[test]
fn l67b_b4_deferred_retirement_under_held_reader_and_later_retire() {
    let mut main = seated_green_main();
    let real_heap = main.tagged_heap.heap_identity();
    let key = main.green_key().expect("seated main has ThreadKey");
    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    assert_eq!(main.parked_owner_len_for_test(), 1);
    assert!(
        !main.parked_root_batch_is_cached_for_test(),
        "no recycled batch cached while initial lease is live"
    );

    let reader = main
        .admit_parked_root_reader_for_test()
        .expect("admit detached reader on parked RootBatchLease");

    // Enter while reader is held -> retirement refuses ReadersOutstanding and
    // fails closed by keeping the lease live in runtime.parked.
    let (turn, lend, _rx) = make_test_turn(real_heap, 21);
    let active = parked
        .enter(&mut main, turn, lend)
        .expect("enter succeeds while reader defers lease retirement");
    assert_eq!(
        main.parked_owner_len_for_test(),
        1,
        "held reader keeps the parked lease live in runtime.parked"
    );
    assert!(
        !main.parked_root_batch_is_cached_for_test(),
        "deferred lease cannot recycle while reader is held"
    );

    // Finish the reader -> later retirement of THAT lease MUST succeed through
    // `RootBatchLease::try_recycle` (caching PreparedRootBatch), not fall back
    // to `root_registry::finish`.
    reader.finish().expect("finish held reader");
    main.try_retire_world_seat_parked_roots(key);
    assert!(
        main.parked_owners_empty_for_test(),
        "deferred lease retired once held reader finished"
    );
    assert!(
        main.parked_root_batch_is_cached_for_test(),
        "later retirement must go through RootBatchLease::try_recycle, not finish fallback"
    );

    // Also verify switch_out while a second reader is STILL held preserves the
    // deferred lease in `retained` alongside the fresh census, and both retire
    // on the next switch_in once that reader finishes.
    let (parked, turn, lend) = active.switch_out(&mut main);
    assert_eq!(main.parked_owner_len_for_test(), 1);
    let reader2 = main
        .admit_parked_root_reader_for_test()
        .expect("admit second detached reader");
    let active = parked
        .switch_in(&mut main, turn, lend)
        .expect("switch_in succeeds while second reader defers retirement");
    assert_eq!(main.parked_owner_len_for_test(), 1);
    let (parked, turn, lend) = active.switch_out(&mut main);
    assert_eq!(
        main.parked_owner_len_for_test(),
        2,
        "deferred lease preserved alongside new parked lease"
    );
    reader2.finish().expect("finish second held reader");
    let active = parked
        .switch_in(&mut main, turn, lend)
        .expect("switch_in after second reader finishes");
    assert!(
        main.parked_owners_empty_for_test(),
        "both deferred and current leases retire once second reader clears"
    );
    assert!(
        main.parked_root_batch_is_cached_for_test(),
        "current lease recycles onto GreenSeat after second reader clears"
    );

    let (parked, lend, returned) = active.exit(&mut main);
    lend.give_back(returned);
    parked.retire_parked_for_teardown_for_test(&mut main);
    assert!(main.parked_owners_empty_for_test());
}

/// L6.7b b4 Q2 (Finding #7): when the Q2 non-marking phase guard is false
/// (`gc_driver_active == true`), `ParkedWorldSeat::enter` succeeds and fails
/// closed by keeping the parked `RootBatchLease` live in `GreenRuntime::parked`;
/// clearing the guard lets later retirement recycle the lease onto `GreenSeat`.
#[test]
fn l67b_b4_q2_phase_guard_refuses_retirement_when_gc_driver_active() {
    let mut main = seated_green_main();
    let real_heap = main.tagged_heap.heap_identity();
    let key = main.green_key().expect("seated main has ThreadKey");
    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    assert_eq!(main.parked_owner_len_for_test(), 1);

    // Violate the Q2 non-marking phase guard during enter.
    main.gc_driver_active = true;
    let (turn, lend, _rx) = make_test_turn(real_heap, 25);
    let active = parked
        .enter(&mut main, turn, lend)
        .expect("enter succeeds while Q2 phase guard defers lease retirement");
    assert_eq!(
        main.parked_owner_len_for_test(),
        1,
        "Q2 guard refusal keeps parked lease live in runtime.parked"
    );
    assert!(
        !main.parked_root_batch_is_cached_for_test(),
        "Q2 guard refusal must not retire or recycle the batch"
    );

    // Clear the guard -> retirement succeeds through try_recycle.
    main.gc_driver_active = false;
    main.try_retire_world_seat_parked_roots(key);
    assert!(main.parked_owners_empty_for_test());
    assert!(
        main.parked_root_batch_is_cached_for_test(),
        "clearing Q2 guard lets retirement succeed via try_recycle"
    );

    let (parked, lend, returned) = active.exit(&mut main);
    lend.give_back(returned);
    parked.retire_parked_for_teardown_for_test(&mut main);
    assert!(main.parked_owners_empty_for_test());
}

/// L6.7b b4 Finding #2 (seat-to-serial-Green handoff): when a prior seat
/// deferral moved an older lease into `WorldRootRegistry::retained`, Green's
/// serial `finish_retained_parked_roots` MUST drain `retained` as well as
/// `owners[key]` so serial Green parking neither panics nor over-roots until
/// World teardown.
#[test]
fn l67b_b4_green_serial_finish_retained_drains_seat_retained_leases() {
    let mut main = seated_green_main();
    let real_heap = main.tagged_heap.heap_identity();
    let key = main.green_key().expect("seated main has ThreadKey");
    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    let reader = main
        .admit_parked_root_reader_for_test()
        .expect("admit reader on initial parked lease");

    // Defer L1 on enter, then switch_out so L1 moves into `retained` while L2
    // occupies `owners[key]`.
    let (turn, lend, _rx) = make_test_turn(real_heap, 26);
    let active = parked
        .enter(&mut main, turn, lend)
        .expect("enter with reader");
    let (parked, turn, lend) = active.switch_out(&mut main);
    assert_eq!(main.parked_owner_len_for_test(), 2);

    // switch_in retires L2 from `owners[key]`, leaving ONLY L1 in `retained`.
    let active = parked
        .switch_in(&mut main, turn, lend)
        .expect("switch_in retires L2 while L1 stays in retained");
    assert_eq!(
        main.parked_owner_len_for_test(),
        1,
        "L1 remains in WorldRootRegistry::retained while reader is held"
    );
    assert!(
        !main.parked_contains_main_for_test(),
        "owners[Main] is now empty; only retained holds L1"
    );

    // Reader finishes -> Green's serial `finish_retained_parked_roots` MUST
    // drain `retained` before a subsequent serial Green park.
    reader.finish().expect("finish reader on retained L1");
    main.finish_retained_parked_roots(key)
        .expect("finish_retained_parked_roots drains retained seat leases");
    assert!(
        main.parked_owners_empty_for_test(),
        "finish_retained_parked_roots must drain WorldRootRegistry::retained"
    );

    let (parked, lend, returned) = active.exit(&mut main);
    lend.give_back(returned);
    parked.retire_parked_for_teardown_for_test(&mut main);
    assert!(main.parked_owners_empty_for_test());
}

/// L6.7b b4 Q2 (Nit #3): when `mark_phase()` is `StoppedMarking`,
/// `ParkedWorldSeat::enter` fails closed on retirement (keeping the lease live
/// in `runtime.parked`); once marking finishes into `MarkPhase::Sweeping`,
/// retirement succeeds via `RootBatchLease::try_recycle`.
#[test]
fn l67b_b4_q2_phase_guard_refuses_retirement_during_stopped_marking() {
    let mut main = seated_green_main();
    let real_heap = main.tagged_heap.heap_identity();
    let key = main.green_key().expect("seated main has ThreadKey");
    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    assert_eq!(main.parked_owner_len_for_test(), 1);

    // Put the heap into MarkPhase::StoppedMarking before enter.
    if main.tagged_heap.tagged_heap_generational_enabled() {
        let _ = main.tagged_heap.begin_minor_collection_for_test();
    } else {
        let _ = main.tagged_heap.begin_stw_collection_for_test();
    }
    assert!(
        main.tagged_heap.mark_in_progress(),
        "begun collection enters StoppedMarking"
    );
    let (turn, lend, _rx) = make_test_turn(real_heap, 27);
    let active = parked
        .enter(&mut main, turn, lend)
        .expect("enter succeeds while StoppedMarking defers lease retirement");
    assert_eq!(
        main.parked_owner_len_for_test(),
        1,
        "StoppedMarking keeps parked lease live in runtime.parked"
    );
    assert!(
        !main.parked_root_batch_is_cached_for_test(),
        "StoppedMarking must not retire or recycle the batch"
    );

    // Finish marking into MarkPhase::Sweeping -> Q2 guard allows try_recycle.
    main.finish_stopped_mark_into_sweep_for_test();
    assert!(!main.tagged_heap.mark_in_progress());
    assert!(main.tagged_heap.sweep_in_progress());

    main.try_retire_world_seat_parked_roots(key);
    assert!(
        main.parked_owners_empty_for_test(),
        "MarkPhase::Sweeping allows deferred lease retirement"
    );
    assert!(
        main.parked_root_batch_is_cached_for_test(),
        "MarkPhase::Sweeping retirement succeeds via try_recycle"
    );

    main.gc_collect_exact();
    let (parked, lend, returned) = active.exit(&mut main);
    lend.give_back(returned);
    parked.retire_parked_for_teardown_for_test(&mut main);
    assert!(main.parked_owners_empty_for_test());
}

/// L6.7b b4 Finding #1 (best-effort serial drain): a still-live lease in
/// `WorldRootRegistry::retained` with a held reader MUST NOT cause an
/// unrelated owner's serial `finish_retained_parked_roots` to fail with a Lisp
/// error; it stays retained fail-closed and drains once the reader finishes.
#[test]
fn l67b_b4_green_serial_finish_retained_does_not_fail_other_owner_on_held_reader() {
    let mut main = seated_green_main();
    let real_heap = main.tagged_heap.heap_identity();
    let other_owner = crate::emacs_core::eval::green::ThreadKey::Worker(nonzero(99));
    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    let reader = main
        .admit_parked_root_reader_for_test()
        .expect("admit reader on initial parked lease");

    // Defer L1 on enter, move L1 to `retained` via switch_out, then retire L2
    // on switch_in so ONLY L1 (with held reader) sits in `retained`.
    let (turn, lend, _rx) = make_test_turn(real_heap, 28);
    let active = parked
        .enter(&mut main, turn, lend)
        .expect("enter with reader");
    let (parked, turn, lend) = active.switch_out(&mut main);
    let active = parked
        .switch_in(&mut main, turn, lend)
        .expect("switch_in retires L2 while L1 stays in retained");
    assert_eq!(main.parked_owner_len_for_test(), 1);
    assert!(!main.parked_contains_main_for_test());

    // An unrelated owner's serial park checks `finish_retained_parked_roots`:
    // the held reader on `retained` MUST NOT turn into a Lisp error.
    main.finish_retained_parked_roots(other_owner)
        .expect("still-live retained lease must not fail another owner's serial park");
    assert_eq!(
        main.parked_owner_len_for_test(),
        1,
        "held-reader lease stays live in retained fail-closed"
    );

    // Once the reader finishes, the next best-effort drain retires L1 cleanly.
    reader.finish().expect("finish held reader");
    main.finish_retained_parked_roots(other_owner)
        .expect("best-effort drain succeeds once reader clears");
    assert!(main.parked_owners_empty_for_test());

    let (parked, lend, returned) = active.exit(&mut main);
    lend.give_back(returned);
    parked.retire_parked_for_teardown_for_test(&mut main);
    assert!(main.parked_owners_empty_for_test());
}

/// L6.7b b4 Q3 teardown order: dropping `ParkedWorldSeat` (its retained
/// `FacadeEpochRetention`) and clearing cached `GreenSeat.parked_root_batch`
/// before final `World::finish_green_registry` / `TaggedHeap::drop` leaves
/// `!facade_mark_is_excluded()`.
#[test]
fn l67b_b4_world_seat_retention_and_cached_batch_release_before_world_drop() {
    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let real_heap = main.tagged_heap.heap_identity();
    let baton = main.start_green_runtime().expect("start Green runtime");
    main.seat_green_main_for_test(baton);

    let parked = ParkedWorldSeat::park_seated(&mut main).expect("park seated main");
    let (turn, lend, _rx) = make_test_turn(real_heap, 31);
    let active = parked
        .enter(&mut main, turn, lend)
        .expect("enter recycles batch");
    let (parked, turn, lend) = active.switch_out(&mut main);
    let active = parked.switch_in(&mut main, turn, lend).expect("switch_in");
    let (parked, lend, returned) = active.exit(&mut main);
    lend.give_back(returned);

    // Retire parked lease, drop cached PreparedRootBatch, and drop the seat's
    // FacadeEpochRetention before releasing the GreenSeat (which drops Baton).
    parked.retire_parked_for_teardown_for_test(&mut main);
    assert!(main.parked_owners_empty_for_test());
    main.green_release_seat();
    assert!(
        !main.tagged_heap.facade_mark_is_excluded(),
        "dropping seat retention, cached batch, and Baton must clear facade exclusion before World drop"
    );
}

/// L6.7b b4.2: intra-interval cross-thread handoff through `ActiveWorldSeat::switch_out`
/// -> `ParkedWorldSeat::switch_in` -> `ActiveWorldSeat::exit` on a live
/// `Driver` + `KeeperEra` with a concurrent parallel owner frozen across the
/// entire interval.
#[test]
fn l67b_b4_world_seat_switch_out_and_switch_in_handoff_between_baton_threads() {
    with_live_green_era("l67b-b4-seat-handoff", |era| {
        let (inbox, inbox_rx) = mpsc::channel();
        let mut driver = Driver::new(era, inbox_rx);
        let peer = era.register(12, 0).expect("register parallel peer");
        let heap = era.heap_identity();
        let mut group = enroll_group(&mut driver, &inbox);
        drop(inbox);

        let handle_a = era.keeper_baton_for_test().test_world_handle_for_test();
        let handle_b = era.keeper_baton_for_test().test_world_handle_for_test();
        let progress = AtomicU64::new(0);
        let done = AtomicBool::new(false);

        let (a_parked_tx, a_parked_rx) = mpsc::channel::<()>();
        let (b_parked_tx, b_parked_rx) = mpsc::channel::<()>();
        let (handoff_tx, handoff_rx) = mpsc::channel::<(WorldTurn, WorldLend, NonZeroU64)>();
        let (b_done_tx, b_done_rx) = mpsc::channel::<u64>();
        let (teardown_a_tx, teardown_a_rx) = mpsc::channel::<()>();
        let (teardown_b_tx, teardown_b_rx) = mpsc::channel::<()>();

        std::thread::scope(|scope| {
            let _panic_guard = KeeperPanicGuard { keeper: era };
            let (progress, done) = (&progress, &done);

            let thread_a = scope.spawn(move || {
                let (mut ctx_a, ret_a) = handle_a.spawn_seated_context_for_test(10);
                let parked_a = ParkedWorldSeat::park_seated_with_retention(&mut ctx_a, &ret_a)
                    .expect("park thread A");
                drop(ret_a);
                a_parked_tx.send(()).expect("signal A parked");
                b_parked_rx.recv_timeout(BOUND).expect("wait B parked");

                let start = Instant::now();
                while progress.load(Ordering::Acquire) == 0 {
                    assert!(start.elapsed() < BOUND, "peer advances before WorldTurn");
                    std::thread::yield_now();
                }

                let (turn, epoch_a, lend) =
                    group.request_world().ok().expect("group lent WorldTurn");
                let active_a = parked_a
                    .enter(&mut ctx_a, turn, lend)
                    .expect("thread A enters WorldTurn");
                assert_eq!(active_a.identity().epoch, epoch_a);
                ctx_a.display_var_change_count = 314;

                // Intra-interval switch_out: stows carriers and publishes parked
                // roots WITHOUT consuming WorldTurn.
                let (parked_a, turn, lend) = active_a.switch_out(&mut ctx_a);
                assert!(names_no_heap());
                assert!(ctx_a.serial_world_is_stowed());
                handoff_tx
                    .send((turn, lend, epoch_a))
                    .expect("hand off WorldTurn to thread B");

                let during = b_done_rx.recv_timeout(BOUND).expect("thread B finished");
                let start = Instant::now();
                while progress.load(Ordering::Acquire) <= during {
                    assert!(start.elapsed() < BOUND, "peer resumes after thread B exit");
                    std::thread::yield_now();
                }
                done.store(true, Ordering::Release);
                group.exit();

                teardown_a_rx.recv_timeout(BOUND).expect("teardown A");
                parked_a.retire_parked_for_teardown_for_test(&mut ctx_a);
                ctx_a.retire_green_owner();
            });

            // Serialize initial seated Context creation on B after A parks.
            a_parked_rx.recv_timeout(BOUND).expect("A parked");
            let (b_ready_tx, b_ready_rx) = mpsc::channel::<()>();
            let thread_b = scope.spawn(move || {
                let (mut ctx_b, ret_b) = handle_b.spawn_seated_context_for_test(11);
                let parked_b = ParkedWorldSeat::park_seated_with_retention(&mut ctx_b, &ret_b)
                    .expect("park thread B");
                drop(ret_b);
                b_ready_tx.send(()).expect("signal keeper B parked");
                b_parked_tx.send(()).expect("signal A B parked");

                let (turn, lend, epoch_a) =
                    handoff_rx.recv_timeout(BOUND).expect("receive handoff");
                let active_b = parked_b
                    .switch_in(&mut ctx_b, turn, lend)
                    .expect("thread B switches in on same WorldTurn");
                assert_eq!(
                    active_b.identity().epoch,
                    epoch_a,
                    "switch_out -> switch_in stays inside the exact same keeper stop epoch"
                );
                assert_eq!(
                    ctx_b.display_var_change_count, 314,
                    "World carrier written by thread A is visible on thread B after switch_in"
                );
                let before = progress.load(Ordering::Acquire);
                std::thread::sleep(Duration::from_millis(25));
                let during = progress.load(Ordering::Acquire);
                assert_eq!(
                    before, during,
                    "parallel peer stays frozen while thread B holds the WorldTurn"
                );
                ctx_b.display_var_change_count = 628;

                // Sole production mint of WorldReturned via ActiveWorldSeat::exit.
                let (parked_b, lend, returned) = active_b.exit(&mut ctx_b);
                assert!(names_no_heap());
                assert!(ctx_b.serial_world_is_stowed());
                lend.give_back(returned);
                b_done_tx.send(during).expect("notify thread A");

                teardown_b_rx.recv_timeout(BOUND).expect("teardown B");
                parked_b.retire_parked_for_teardown_for_test(&mut ctx_b);
                ctx_b.retire_green_owner();
            });

            b_ready_rx.recv_timeout(BOUND).expect("B parked");
            scope.spawn(move || run_peer(activate(peer, heap, 12), progress, done));
            assert_eq!(driver.run(), DriverEnd::Drained);

            // Serialize teardown of A then B on the now-idle era.
            teardown_a_tx.send(()).expect("teardown A");
            thread_a.join().expect("thread A joined");
            teardown_b_tx.send(()).expect("teardown B");
            thread_b.join().expect("thread B joined");
        });
    });
}

/// L6.7b b4.2: a panic unwinding between `ActiveWorldSeat::switch_out` and
/// `ParkedWorldSeat::switch_in` drops the armed `WorldLend` and poisons the
/// era with `Breach::GroupAbandoned { cause: LendDrop::Unwound }` (K7).
#[test]
fn l67b_b4_unwind_between_switch_out_and_switch_in_poisons_era() {
    let mut source = ready();
    source.lexenv = Value::string("l67b-b4-switch-unwind");
    let bootstrap = source.capture_green_bootstrap_for_test();
    let (_shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |era| {
            era.keeper_baton_for_test()
                .install_stowed_green_runtime_for_test(bootstrap);
            let (inbox, inbox_rx) = mpsc::channel();
            let mut driver = Driver::new(era, inbox_rx);
            let mut group = enroll_group(&mut driver, &inbox);
            drop(inbox);

            let handle_a = era.keeper_baton_for_test().test_world_handle_for_test();
            let (ready_tx, ready_rx) = mpsc::channel::<()>();
            let (teardown_tx, teardown_rx) = mpsc::channel::<()>();

            let end = std::thread::scope(|scope| {
                let thread_a = scope.spawn(move || {
                    let (mut ctx_a, ret_a) = handle_a.spawn_seated_context_for_test(20);
                    let parked_a = ParkedWorldSeat::park_seated_with_retention(&mut ctx_a, &ret_a)
                        .expect("park A");
                    drop(ret_a);
                    ready_tx.send(()).expect("A parked");

                    let (turn, _epoch, lend) =
                        group.request_world().ok().expect("group lent WorldTurn");
                    let active_a = parked_a.enter(&mut ctx_a, turn, lend).expect("A enters");
                    let (parked_a, turn, lend) = active_a.switch_out(&mut ctx_a);

                    // Unwind BEFORE another thread can switch_in.
                    let caught =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                            let _held = (turn, lend);
                            panic!("unwind between switch_out and switch_in");
                        }));
                    assert!(caught.is_err());
                    group.exit();

                    teardown_rx.recv_timeout(BOUND).expect("teardown A");
                    parked_a.retire_parked_for_teardown_for_test(&mut ctx_a);
                    ctx_a.retire_green_owner();
                });

                ready_rx.recv_timeout(BOUND).expect("wait A parked");
                let end = driver.run();
                teardown_tx.send(()).expect("signal teardown");
                thread_a.join().expect("thread A joined");
                end
            });
            era.keeper_baton_for_test()
                .uninstall_green_runtime_for_test();
            tx.send(end).expect("send driver end");
        });
        drop(outcome);
    });
    let end = rx.recv_timeout(BOUND).expect("keeper outcome");
    service.join().expect("keeper service thread");
    assert_eq!(
        end,
        DriverEnd::Poisoned(Breach::GroupAbandoned {
            cause: LendDrop::Unwound,
        })
    );
}

/// L6.7b b4.2 (Low 4): `ParkedWorldSeat::enter` and `switch_in` refuse
/// `WorldSeatRefusal::WrongContext` when given a different seated `Context`
/// on the same Green World and `HeapIdentity`.
#[test]
fn l67b_b4_world_seat_refuses_wrong_context_slab() {
    with_live_green_era("l67b-b4-wrong-context", |era| {
        let heap = era.heap_identity();
        let handle = era.keeper_baton_for_test().test_world_handle_for_test();

        let (mut ctx_a, ret_a) = handle.spawn_seated_context_for_test(30);
        let parked_a =
            ParkedWorldSeat::park_seated_with_retention(&mut ctx_a, &ret_a).expect("park ctx_a");
        drop(ret_a);

        let (mut ctx_b, ret_b) = handle.spawn_seated_context_for_test(31);
        let parked_b =
            ParkedWorldSeat::park_seated_with_retention(&mut ctx_b, &ret_b).expect("park ctx_b");
        drop(ret_b);

        let (turn, lend, _rx) = make_test_turn(heap, 41);
        let (parked_a, turn, lend, refusal) = parked_a
            .enter(&mut ctx_b, turn, lend)
            .expect_err("enter with wrong Context slab must refuse WrongContext");
        assert_eq!(refusal, WorldSeatRefusal::WrongContext);
        assert!(names_no_heap());
        assert!(ctx_a.serial_world_is_stowed());

        let (parked_a, turn, lend, refusal) = parked_a
            .switch_in(&mut ctx_b, turn, lend)
            .expect_err("switch_in with wrong Context slab must refuse WrongContext");
        assert_eq!(refusal, WorldSeatRefusal::WrongContext);
        lend.give_back(WorldReturned::from_end(turn));

        parked_b.retire_parked_for_teardown_for_test(&mut ctx_b);
        ctx_b.retire_green_owner();
        parked_a.retire_parked_for_teardown_for_test(&mut ctx_a);
        ctx_a.retire_green_owner();
    });
}

//! L6.7b b4.3 live and counter rows: Green switch points end the Baton
//! group's interval over a real `WorldEra` + `KeeperEra` + `Driver` +
//! `GroupPort` + parallel `ChannelPort` owner sharing the same Green World.

use crate::emacs_core::eval::green::{Control, MutexId, ThreadKey, Wait};
use crate::emacs_core::eval::parallel_entry::keeper_driver::{
    ChannelPort, Driver, DriverEnd, GroupPort, KeeperMsg,
};
use crate::emacs_core::eval::parallel_entry::owned_source::era_fixture::{
    KeeperPanicGuard, activate, close, finish_fixture, keeper_serve, retire,
};
use crate::emacs_core::eval::parallel_entry::serial_park::{KeeperPort, SerialHome, begin_turn};
use crate::emacs_core::eval::parallel_entry::serial_ticket::LocalSerialOwner;
use crate::emacs_core::eval::parallel_entry::serial_turn::OwnerSerialState;
use crate::emacs_core::eval::world::TurnRight;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use neovm_value::tagged::gc::RendezvousError;
use neovm_value::tagged::gc::heap_transfer::PublishedOwner;
use neovm_value::value::Value;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BOUND: Duration = Duration::from_secs(30);
const PEER_OWNER_ID: u64 = 8401;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(neovm_value::error::EvalError::Signal { symbol, data, .. }) => format!(
            "ERR {} {}",
            neovm_value::intern::resolve_sym(symbol),
            print_value(&Value::list(data))
        ),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn green_world() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut ev =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    ev.gc_collect_exact();
    assert!(
        ev.tagged_heap.tagged_heap_generational_enabled(),
        "L6.7b b4.3 live era rows require a generational heap"
    );
    ev.start_green_threads().expect("start Green threads");
    ev
}

fn standalone(_: &mut LocalSerialOwner) -> Result<(), Infallible> {
    Ok(())
}

/// Run a real Green `Context` inside a real `WorldEra` + `KeeperEra` +
/// `Driver` + `GroupPort`, with optional parallel owner work on the SAME World.
///
/// Uses the canonical `owned_source::era_fixture` (`close(source).prepare_era()`
/// + `keeper_serve`) so source observations, `GreenRuntime` stow/unstow, and
/// `era.finish()` follow the exact production era lifecycle with zero `unsafe`.
fn run_in_live_green_era<S, M, P>(
    label: &'static str,
    setup: S,
    with_peer: bool,
    peer_body: P,
    main_body: M,
) where
    S: FnOnce(&mut Context),
    M: FnOnce(&mut Context, &Arc<Control>) + Send + 'static,
    P: FnOnce(
            &mut OwnerSerialState<Context, LocalSerialOwner>,
            &mut ChannelPort<'_>,
            SerialHome,
            PublishedOwner,
        ) + Send
        + 'static,
{
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    {
        crate::emacs_core::jit::force_jit_off_for_test(true);
        crate::emacs_core::jit::cache::reset_carrier_tls();
    }
    super::source_entry::install_inactive_source_module_probe_for_test();
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
        "L6.7b b4.3 live era rows require a generational heap"
    );
    source.tagged_heap.close_alloc_regions();
    let bootstrap = source.capture_green_bootstrap_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();

    let (tx, rx) = mpsc::channel();
    let (panic_tx, panic_rx) = mpsc::sync_channel::<Option<Box<dyn std::any::Any + Send>>>(1);
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |keeper| {
            let _panic_guard = KeeperPanicGuard { keeper };
            keeper
                .keeper_baton_for_test()
                .install_stowed_green_runtime_for_test(bootstrap);
            let world_handle = keeper.keeper_baton_for_test().test_world_handle_for_test();
            let control = world_handle.green_control_for_test();
            let era_world_handle = world_handle.era_world_handle_for_test();
            let heap = keeper.heap_identity();

            let (inbox, inbox_rx) = mpsc::channel();
            let mut driver = Driver::new(keeper, inbox_rx);
            let link = driver.enroll_group();
            let group_port = GroupPort::new(inbox.clone(), link);
            control
                .open_era_stowed_for_test(era_world_handle, group_port)
                .expect("open era on stowed Green Control");

            let (peer_setup_tx, peer_setup_rx) = mpsc::sync_channel::<TurnRight>(1);
            let (peer_right_back_tx, peer_right_back_rx) = mpsc::sync_channel::<TurnRight>(1);

            let (end, body_panic) = std::thread::scope(|scope| {
                let peer_handle = if with_peer {
                    let permit = keeper
                        .register(PEER_OWNER_ID, 0)
                        .expect("register parallel owner");
                    let (grant_tx, grant_rx) = mpsc::sync_channel(1);
                    driver.enroll(PEER_OWNER_ID, permit.key(), grant_tx);
                    let peer_inbox = inbox.clone();
                    let (peer_idle_tx, peer_idle_rx) = mpsc::sync_channel::<()>(1);
                    let handle = scope.spawn(move || {
                        let mutator = activate(permit, heap, PEER_OWNER_ID);
                        let mut port =
                            ChannelPort::new(PEER_OWNER_ID, mutator, peer_inbox, grant_rx);
                        let start = Instant::now();
                        let idle_receipt = loop {
                            match port.publish_idle_receipt_for_test() {
                                Ok(receipt) => break receipt,
                                Err(
                                    crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal::Retry,
                                ) if start.elapsed() < BOUND => {
                                    std::thread::yield_now();
                                }
                                Err(err) => panic!("peer publish idle: {err:?}"),
                            }
                        };
                        peer_idle_tx.send(()).expect("peer idle ready");
                        let right = peer_setup_rx
                            .recv_timeout(BOUND)
                            .expect("peer setup TurnRight");
                        right.enter();
                        let owner = Context::new_green_owner_with_turn(&right);
                        let mut state: OwnerSerialState<Context, LocalSerialOwner> =
                            OwnerSerialState::new(PEER_OWNER_ID, Box::new(owner))
                                .ok()
                                .expect("detach same-World parallel owner");
                        peer_right_back_tx
                            .send(right)
                            .expect("return setup TurnRight");
                        let home = SerialHome {
                            owner: PEER_OWNER_ID,
                            heap,
                        };
                        peer_body(&mut state, &mut port, home, idle_receipt);
                        let mutator = port.exit();
                        retire(mutator);
                        drop(state);
                    });
                    peer_idle_rx.recv_timeout(BOUND).expect("peer idle");
                    Some(handle)
                } else {
                    drop(peer_setup_rx);
                    drop(peer_right_back_tx);
                    None
                };
                drop(inbox);

                let main_control = Arc::clone(&control);
                let main_handle = scope.spawn(move || {
                    let mut initial_turn = main_control
                        .acquire_initial_era_turn()
                        .expect("acquire initial era WorldTurn");
                    if with_peer {
                        peer_setup_tx
                            .send(initial_turn)
                            .expect("send initial TurnRight to peer setup");
                        initial_turn = peer_right_back_rx
                            .recv_timeout(BOUND)
                            .expect("receive initial TurnRight back from peer setup");
                    } else {
                        drop(peer_setup_tx);
                        drop(peer_right_back_rx);
                    }
                    let mut main =
                        Context::new_seated_era_main_for_test(&world_handle, initial_turn);
                    let body_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        main_body(&mut main, &main_control);
                    }))
                    .err();
                    let final_right = main.retire_seated_era_main_for_test();
                    main_control.release_era_turn_to_idle(final_right);
                    let group_port = main_control
                        .take_era_port_for_exit()
                        .expect("take GroupPort for exit");
                    group_port.exit();
                    drop(world_handle);
                    body_panic
                });

                let end = driver.run();
                if let Some(peer_handle) = peer_handle {
                    peer_handle.join().expect("parallel owner thread");
                }
                let body_panic = main_handle.join().expect("main Baton thread");
                (end, body_panic)
            });

            assert_eq!(end, DriverEnd::Drained, "keeper driver must drain cleanly");
            // Fold seated/worker heap deltas back into the frozen legacy image
            // before `EraCore::finish()` runs `restore_legacy(heap)`.
            let mut stopped = loop {
                match keeper.stop() {
                    Ok(stopped) => break stopped,
                    Err(RendezvousError::Busy) => std::thread::yield_now(),
                    Err(other) => panic!("teardown stop: {other:?}"),
                }
            };
            keeper
                .collect_requested(&mut stopped)
                .expect("teardown collection folds Baton/peer heap deltas");
            stopped.complete().expect("complete teardown stop");
            keeper
                .keeper_baton_for_test()
                .uninstall_green_runtime_for_test();
            let _ = panic_tx.send(body_panic);
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
    if let Ok(Some(payload)) = panic_rx.try_recv() {
        std::panic::resume_unwind(payload);
    }
}

/// Row 1 (§7 b4): a `thread-yield` busy loop lets a queued parallel request in
/// at the next yield (via `yield_peer_ready_at` reading `ring_waiter()` and
/// `transfer_era` Case (ii) ending the interval + re-requesting at ring tail).
#[test]
fn l67b_b43_thread_yield_busy_loop_lets_queued_parallel_request_in() {
    let loop_started = Arc::new(AtomicBool::new(false));
    let peer_ran = Arc::new(AtomicBool::new(false));
    let loop_started_peer = Arc::clone(&loop_started);
    let peer_ran_peer = Arc::clone(&peer_ran);

    run_in_live_green_era(
        "b43-yield-busy",
        |src| {
            assert_eq!(eval(src, "(setq b43-yield-stop nil)"), "nil");
        },
        true,
        move |state, port, home, idle_receipt| {
            // Wait until main has entered the busy-yield loop, then send
            // KeeperMsg::Request using our pre-published receipt while main
            // holds the WorldTurn so `driver.await_world()` raises RingWaiter.
            let start = Instant::now();
            while !loop_started_peer.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND, "main entered busy loop");
                std::thread::yield_now();
            }
            let (grant, lend) = port
                .request_turn_with_receipt_for_test(idle_receipt)
                .ok()
                .expect("peer granted serial turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("peer begins serial turn");
            let result = turn.with_world(|ev| eval(ev, "(setq b43-yield-stop 'peer-entered)"));
            assert_eq!(result, "peer-entered");
            peer_ran_peer.store(true, Ordering::Release);
            turn.end(port).ok().expect("peer ends serial turn");
        },
        move |main, control| {
            // Run a single-Baton-thread busy loop: no Baton peer is ready, so
            // only the ring-waiter check in `yield_peer_ready_at` lets the
            // queued parallel request in.
            control.reset_ring_waiter_loads_for_test();
            loop_started.store(true, Ordering::Release);
            let out = eval(
                main,
                "(let ((iters 0))
                   (while (null b43-yield-stop)
                     (setq iters (1+ iters))
                     (thread-yield))
                   (list b43-yield-stop (> iters 0)))",
            );
            assert_eq!(out, "(peer-entered t)");
            assert!(peer_ran.load(Ordering::Acquire));
            assert!(
                control.ring_waiter_loads_for_test() > 0,
                "open era switch points must observe ring_waiter()"
            );
        },
    );
}

/// Row 2 (§7 b4): a Baton thread blocked in `sleep-for` lets parallel owners
/// run during the sleep (Case (i) in `release_main_wait`), and resumes after
/// them.
#[test]
fn l67b_b43_sleep_for_lets_parallel_owners_run_and_resumes_after() {
    let peer_go = Arc::new(AtomicBool::new(false));
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_go_p = Arc::clone(&peer_go);
    let peer_done_p = Arc::clone(&peer_done);

    run_in_live_green_era(
        "b43-sleep-for",
        |src| {
            assert_eq!(eval(src, "(setq b43-sleep-seen 'before)"), "before");
        },
        true,
        move |state, port, home, _idle_receipt| {
            let start = Instant::now();
            while !peer_go_p.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }
            // Give main a moment to enter OS sleep-for, then take a serial turn.
            std::thread::sleep(Duration::from_millis(15));
            let (grant, lend) = port.request_turn().ok().expect("peer granted during sleep");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("peer begins serial turn");
            let result = turn.with_world(|ev| eval(ev, "(setq b43-sleep-seen 'during-sleep)"));
            assert_eq!(result, "during-sleep");
            turn.end(port).ok().expect("peer ends serial turn");
            peer_done_p.store(true, Ordering::Release);
        },
        move |main, _control| {
            peer_go.store(true, Ordering::Release);
            let out = eval(main, "(progn (sleep-for 0.12) b43-sleep-seen)");
            assert_eq!(out, "during-sleep");
            assert!(
                peer_done.load(Ordering::Acquire),
                "parallel owner must have completed while main slept"
            );
        },
    );
}

/// Row 3 (r1 §4, QD2): all Baton threads blocked on a mutex/join that a
/// parallel owner unlocks in its next serial turn: no deadlock is reported,
/// and the Baton thread runs after that turn.
#[test]
fn l67b_b43_all_baton_threads_blocked_on_mutex_unlocked_by_parallel_owner_no_deadlock() {
    let go_peer = Arc::new(AtomicBool::new(false));
    let go_peer_p = Arc::clone(&go_peer);

    run_in_live_green_era(
        "b43-mutex-no-dl",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq b43-mx (make-mutex \"b43-mx\"))
                       (mutex-lock b43-mx)
                       'locked)"
                ),
                "locked"
            );
        },
        true,
        move |state, port, home, _idle_receipt| {
            let start = Instant::now();
            while !go_peer_p.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }
            // Wait long enough that both Baton threads are blocked (worker on
            // Wait::Mutex, main on Wait::Join) and would have triggered
            // DeadlockVerdict::Blocked without QD2.
            std::thread::sleep(Duration::from_millis(25));
            let (grant, lend) = port
                .request_turn()
                .ok()
                .expect("peer granted while all Baton threads blocked");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("peer begins serial turn");
            let out = turn.with_world(|ev| {
                // Unlock the mutex on behalf of its holder (main) so the
                // blocked Baton worker wakes through `control.wake_waiting_for`.
                ev.threads.enter_thread(ThreadKey::Main.id());
                eval(ev, "(progn (mutex-unlock b43-mx) 'unlocked)")
            });
            assert_eq!(out, "unlocked");
            turn.end(port).ok().expect("peer ends serial turn");
        },
        move |main, _control| {
            go_peer.store(true, Ordering::Release);
            let out = eval(
                main,
                "(let ((w (make-thread
                            (lambda ()
                              (mutex-lock b43-mx)
                              (prog1 'worker-resumed
                                (mutex-unlock b43-mx))))))
                   (thread-join w))",
            );
            assert_eq!(
                out, "worker-resumed",
                "QD2: live parallel owner unlocks mutex; no deadlock reported and worker runs"
            );
        },
    );
}

/// QD2 positive check: all Baton threads blocked on a mutex/join with NO live
/// parallel owner and an empty ring: `idle_for_deadlock()` answers `true` and
/// Green reports the same `DeadlockVerdict::Blocked` signal as outside an era.
#[test]
fn l67b_b43_all_baton_threads_blocked_with_no_live_parallel_owner_reports_deadlock() {
    run_in_live_green_era(
        "b43-mutex-dl",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq b43-dl-mx (make-mutex \"b43-dl\"))
                       (mutex-lock b43-dl-mx)
                       'locked)"
                ),
                "locked"
            );
        },
        false,
        |_state, _port, _home, _idle_receipt| {},
        move |main, _control| {
            let out = eval(
                main,
                "(let ((w (make-thread
                            (lambda ()
                              (mutex-lock b43-dl-mx)
                              (mutex-unlock b43-dl-mx)))))
                   (list (thread-join w) (thread-last-error)))",
            );
            assert!(
                out.contains("All threads are blocked"),
                "expected conservative deadlock signal when no parallel owner is live, got: {out}"
            );
            // Unlock cleanly before era teardown.
            assert_eq!(eval(main, "(progn (mutex-unlock b43-dl-mx) 'done)"), "done");
        },
    );
}

/// Row 4 (r1 §3): two Baton threads woken together send ONE coalesced
/// `BatonRequest` (never triggering the ring's `Identity` refusal of a second
/// group entry), and both run to completion.
#[test]
fn l67b_b43_two_baton_threads_woken_together_send_one_coalesced_baton_request() {
    // Part A: deterministic unit count of KeeperMsg::BatonRequest == 1 when
    // two blocked Baton waiters are woken together while GroupTurn::Idle.
    {
        use crate::emacs_core::eval::parallel_entry::driver_core::RingWaiter;
        use std::num::NonZeroU64;

        let mut owner =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let baton = owner.start_green_runtime().unwrap();
        let world_handle = baton.era_world_handle().unwrap();
        let control = owner.with_green_runtime(|runtime| Arc::clone(&runtime.control));
        let _main_rx = control.register_main_guard_for_test();
        let w1 = ThreadKey::Worker(NonZeroU64::new(1).unwrap());
        let w2 = ThreadKey::Worker(NonZeroU64::new(2).unwrap());
        let _w1_rx = control.register_worker_guard_for_test(w1);
        let _w2_rx = control.register_worker_guard_for_test(w2);

        let escrowed = control
            .surrender_solo_for_era(TurnRight::Solo(baton))
            .expect("surrender Solo");
        let (inbox_tx, inbox_rx) = mpsc::channel();
        let (_world_tx, world_rx) = mpsc::sync_channel(1);
        let port = GroupPort::from_raw_parts_for_test(
            inbox_tx,
            world_rx,
            Arc::new(RingWaiter::new_for_test()),
        );
        control.open_era(world_handle, port).expect("open era");

        // Park both workers on the same mutex wait while GroupTurn::Idle.
        let mutex_id = MutexId::new(42);
        control.park_worker_for_coalesce_test(w1, Wait::Mutex(mutex_id));
        control.park_worker_for_coalesce_test(w2, Wait::Mutex(mutex_id));

        // Wake BOTH workers together -> must emit EXACTLY ONE BatonRequest!
        let woken = control.wake_waiting_for(Wait::Mutex(mutex_id));
        assert_eq!(woken, 2, "both waiters woken");
        let mut requests = 0usize;
        while let Ok(msg) = inbox_rx.try_recv() {
            assert!(matches!(msg, KeeperMsg::BatonRequest));
            requests += 1;
        }
        assert_eq!(
            requests, 1,
            "r1 §3: waking two Baton threads together must send ONE coalesced BatonRequest"
        );

        let (_solo, _wh, _port) = control.close_era(escrowed).expect("close era");
    }

    // Part B: live Driver + KeeperEra check — two Baton workers blocked on the
    // same mutex, unlocked together by a parallel owner; Driver never hits
    // Identity refusal and both workers complete cleanly.
    let go_peer = Arc::new(AtomicBool::new(false));
    let go_peer_p = Arc::clone(&go_peer);

    run_in_live_green_era(
        "b43-coalesce-live",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq b43-c-mx (make-mutex \"b43-coal\"))
                       (setq b43-c-log nil)
                       (mutex-lock b43-c-mx)
                       'locked)"
                ),
                "locked"
            );
        },
        true,
        move |state, port, home, _idle_receipt| {
            let start = Instant::now();
            while !go_peer_p.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }
            std::thread::sleep(Duration::from_millis(25));
            let (grant, lend) = port.request_turn().ok().expect("peer granted");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("peer begins");
            let out = turn.with_world(|ev| {
                ev.threads.enter_thread(ThreadKey::Main.id());
                eval(ev, "(progn (mutex-unlock b43-c-mx) 'unlocked)")
            });
            assert_eq!(out, "unlocked");
            turn.end(port).ok().expect("peer ends");
        },
        move |main, _control| {
            go_peer.store(true, Ordering::Release);
            let out = eval(
                main,
                "(let ((w1 (make-thread
                             (lambda ()
                               (mutex-lock b43-c-mx)
                               (setq b43-c-log (cons 'w1 b43-c-log))
                               (mutex-unlock b43-c-mx)
                               'w1-done)))
                       (w2 (make-thread
                             (lambda ()
                               (mutex-lock b43-c-mx)
                               (setq b43-c-log (cons 'w2 b43-c-log))
                               (mutex-unlock b43-c-mx)
                               'w2-done))))
                   (list (thread-join w1) (thread-join w2) (length b43-c-log)))",
            );
            assert_eq!(out, "(w1-done w2-done 2)");
        },
    );
}

/// Row 5 (QB2 condition): a Baton-only World (no era) does ZERO ring-word loads
/// over N yields (both lone yields and peer hand-offs).
#[test]
fn l67b_b43_baton_only_world_does_zero_ring_word_loads_over_n_yields() {
    let mut main = green_world();
    let control = main.green_control_for_test().expect("seated Green main");
    control.reset_ring_waiter_loads_for_test();

    // 200 lone yields (yield_peer_ready_at returns false) + 200 peer yields.
    assert_eq!(
        eval(
            &mut main,
            "(progn
               (let ((i 0))
                 (while (< i 200)
                   (thread-yield)
                   (setq i (1+ i))))
               (setq b43-solo-stop nil)
               (let ((w (make-thread (lambda () (while (null b43-solo-stop) (thread-yield))))))
                 (let ((j 0))
                   (while (< j 200)
                     (thread-yield)
                     (setq j (1+ j))))
                 (setq b43-solo-stop t)
                 (thread-join w))
               'ok)"
        ),
        "ok"
    );
    assert_eq!(
        control.ring_waiter_loads_for_test(),
        0,
        "QB2: a Baton-only World (no era) must do ZERO ring-word loads over N yields"
    );
}

/// Row 1b (§7 b4): two Baton threads yielding back and forth hit `transfer_era`
/// Case (ii) BEFORE `pick_next` when a parallel owner queues, end the interval
/// with a tail re-request (QD3), let the parallel owner run, and then finish.
#[test]
fn l67b_b43_two_baton_threads_yield_loop_lets_queued_parallel_request_in() {
    let loop_started = Arc::new(AtomicBool::new(false));
    let loop_started_peer = Arc::clone(&loop_started);

    run_in_live_green_era(
        "b43-pair-yield",
        |src| {
            assert_eq!(eval(src, "(setq b43-pair-stop nil)"), "nil");
        },
        true,
        move |state, port, home, idle_receipt| {
            let start = Instant::now();
            while !loop_started_peer.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }
            let (grant, lend) = port
                .request_turn_with_receipt_for_test(idle_receipt)
                .ok()
                .expect("peer granted serial turn while 2 Baton threads yield");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("peer begins serial turn");
            let result = turn.with_world(|ev| eval(ev, "(setq b43-pair-stop 'peer-ran)"));
            assert_eq!(result, "peer-ran");
            turn.end(port).ok().expect("peer ends serial turn");
        },
        move |main, control| {
            control.reset_ring_waiter_loads_for_test();
            loop_started.store(true, Ordering::Release);
            let out = eval(
                main,
                "(let ((w (make-thread
                            (lambda ()
                              (while (null b43-pair-stop)
                                (thread-yield))
                              'worker-done))))
                   (while (null b43-pair-stop)
                     (thread-yield))
                   (list b43-pair-stop (thread-join w)))",
            );
            assert_eq!(out, "(peer-ran worker-done)");
            assert!(control.ring_waiter_loads_for_test() > 0);
        },
    );
}

/// b4.4 boundary (#14212/#14213): `Control::collect_era_interval` keeps
/// `EraLink.turn` in `GroupTurn::Collecting` (NOT `Idle`) while `Control.state`
/// is unlocked across `GroupPort::collect`, so waking a sibling Baton thread
/// during collection joins the FIFO and emits ZERO duplicate `BatonRequest`.
#[test]
fn l67b_b43_collect_era_interval_keeps_collecting_state_and_coalesces_sibling_wakes() {
    use crate::emacs_core::eval::parallel_entry::driver_core::{GroupMsg, IntervalEnd, RingWaiter};
    use crate::emacs_core::eval::parallel_entry::world_turn::{
        WorldReturned, WorldTurn, WorldTurnIdentity,
    };
    use std::num::NonZeroU64;

    let mut owner =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let baton = owner.start_green_runtime().unwrap();
    let heap = baton.parallel_heap_identity();
    let world_handle = baton.era_world_handle().unwrap();
    let control = owner.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let _main_rx = control.register_main_guard_for_test();
    let sibling = ThreadKey::Worker(NonZeroU64::new(7).unwrap());
    let _sibling_rx = control.register_worker_guard_for_test(sibling);

    let escrowed = control
        .surrender_solo_for_era(TurnRight::Solo(baton))
        .expect("surrender Solo");
    let (inbox_tx, inbox_rx) = mpsc::channel();
    let (world_tx, world_rx) = mpsc::sync_channel(1);
    let port = GroupPort::from_raw_parts_for_test(
        inbox_tx.clone(),
        world_rx,
        Arc::new(RingWaiter::new_for_test()),
    );
    control.open_era(world_handle, port).expect("open era");

    let epoch1 = NonZeroU64::new(1).unwrap();
    let epoch2 = NonZeroU64::new(2).unwrap();
    let id1 = WorldTurnIdentity {
        heap,
        epoch: epoch1,
    };
    let id2 = WorldTurnIdentity {
        heap,
        epoch: epoch2,
    };

    // Move EraLink to GroupTurn::Holding via acquire_initial_era_turn on a
    // clean scoped thread (which has no TAGGED_HEAP TLS installed).
    let world_tx_init = world_tx.clone();
    let prev_holder = owner.with_green_runtime(|runtime| {
        runtime
            .carried_holder
            .replace(crate::emacs_core::eval::green::CarriedHolder::Stowed)
    });
    let (init_tx, init_rx) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let ctrl = Arc::clone(&control);
        scope.spawn(move || {
            let right = ctrl
                .acquire_initial_era_turn()
                .expect("acquire initial turn");
            init_tx.send(right).expect("send initial right");
        });
        assert!(matches!(
            inbox_rx.recv_timeout(BOUND),
            Ok(KeeperMsg::BatonRequest)
        ));
        world_tx_init
            .send(GroupMsg::World {
                turn: WorldTurn::cut_for_test(id1),
                epoch: epoch1,
            })
            .expect("send initial World");
    });
    let initial_right = init_rx.recv_timeout(BOUND).expect("initial right");
    let (real_turn1, real_lend1, _wh1) = initial_right.into_era().unwrap().into_parts();

    // Park sibling on a mutex wait while GroupTurn::Holding.
    let mutex_id = MutexId::new(99);
    control.park_worker_for_coalesce_test(sibling, Wait::Mutex(mutex_id));

    let returned1 = WorldReturned::from_end(real_turn1);
    std::thread::scope(|scope| {
        let ctrl = Arc::clone(&control);
        let handle = scope.spawn(move || ctrl.collect_era_interval(real_lend1, returned1, None));

        // Keeper observes WorldReturn { end: IntervalEnd::Collect }.
        let msg = inbox_rx
            .recv_timeout(BOUND)
            .expect("collect sends WorldReturn");
        assert!(matches!(
            msg,
            KeeperMsg::WorldReturn {
                end: IntervalEnd::Collect { .. },
                ..
            }
        ));

        // While collect_era_interval is waiting in GroupPort::collect
        // (EraLink.turn == GroupTurn::Collecting), wake sibling!
        let woken = control.wake_waiting_for(Wait::Mutex(mutex_id));
        assert_eq!(woken, 1, "sibling woken into FIFO");

        // MUST NOT emit any duplicate BatonRequest while Collecting!
        assert!(
            inbox_rx.try_recv().is_err(),
            "b4.4 invariant: waking sibling during Collecting must NOT send BatonRequest"
        );

        // Keeper re-lends World at ring HEAD.
        world_tx
            .send(GroupMsg::World {
                turn: WorldTurn::cut_for_test(id2),
                epoch: epoch2,
            })
            .expect("re-lend World after collect");

        let (turn2, got_epoch, lend2) = handle
            .join()
            .expect("collect thread")
            .expect("collect succeeds");
        assert_eq!(got_epoch, epoch2);
        lend2.give_back(WorldReturned::from_end(turn2));
    });

    owner.with_green_runtime(|runtime| {
        runtime.carried_holder.set(prev_holder);
    });
    let (_solo, _wh, _port) = control.close_era(escrowed).expect("close era");
}

/// HIGH-1: dropping or unwinding a `TurnRight::Era` on a Baton OS thread
/// restores `World::baton_holder` to the keeper thread strictly BEFORE
/// `WorldLend::drop` sends `KeeperMsg::GroupAbandon`, so the keeper's escrowed
/// `Baton::held_here()` remains `true` on abandon/teardown.
#[test]
fn l67b_b43_era_turn_right_drop_restores_keeper_baton_holder_before_group_abandon() {
    use crate::emacs_core::eval::parallel_entry::keeper_driver::{LendDrop, WorldLend};
    use crate::emacs_core::eval::parallel_entry::world_turn::{WorldTurn, WorldTurnIdentity};
    use std::num::NonZeroU64;

    let mut owner =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let baton = owner.start_green_runtime().unwrap();
    let heap = baton.parallel_heap_identity();
    assert!(baton.held_here(), "keeper thread initially holds Baton");

    // Case 1: ordinary drop (LendDrop::Leaked) on a different OS thread.
    {
        let world_handle = baton.era_world_handle().unwrap();
        let (tx, rx) = mpsc::channel();
        let turn = WorldTurn::cut_for_test(WorldTurnIdentity {
            heap,
            epoch: NonZeroU64::new(1).unwrap(),
        });
        let lend = WorldLend::for_test(tx);
        let right = TurnRight::from_era(turn, lend, world_handle).expect("build TurnRight::Era");
        std::thread::scope(|scope| {
            scope.spawn(move || {
                right.enter();
                assert!(right.held_here(), "worker entered TurnRight::Era");
                drop(right);
            });
            assert!(matches!(
                rx.recv_timeout(BOUND),
                Ok(KeeperMsg::GroupAbandon {
                    cause: LendDrop::Leaked
                })
            ));
            assert!(
                baton.held_here(),
                "HIGH-1: keeper Baton::held_here() must be restored strictly before GroupAbandon"
            );
        });
    }

    // Case 2: panic unwind (LendDrop::Unwound) on a different OS thread.
    {
        let world_handle = baton.era_world_handle().unwrap();
        let (tx, rx) = mpsc::channel();
        let turn = WorldTurn::cut_for_test(WorldTurnIdentity {
            heap,
            epoch: NonZeroU64::new(2).unwrap(),
        });
        let lend = WorldLend::for_test(tx);
        let right = TurnRight::from_era(turn, lend, world_handle).expect("build TurnRight::Era");
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                    right.enter();
                    assert!(right.held_here());
                    let _held = right;
                    panic!("simulated K7 Baton-thread unwind");
                }));
                assert!(caught.is_err());
            });
            assert!(matches!(
                rx.recv_timeout(BOUND),
                Ok(KeeperMsg::GroupAbandon {
                    cause: LendDrop::Unwound
                })
            ));
            assert!(
                baton.held_here(),
                "HIGH-1: keeper Baton::held_here() must be restored strictly before GroupAbandon::Unwound"
            );
            worker.join().expect("worker joined after caught panic");
        });
    }

    drop(baton);
    drop(owner);

    // Case 3: end-to-end live Driver + KeeperEra: a Baton thread acquires
    // TurnRight::Era, enters it, and panics inside the era interval. The
    // keeper ends Poisoned(GroupAbandoned { cause: LendDrop::Unwound }) and
    // its teardown passes Baton::held_here().
    {
        use crate::emacs_core::eval::parallel_entry::keeper_driver::Breach;

        super::source_entry::install_inactive_source_module_probe_for_test();
        let mut source = Box::new(Context::new_with_threading(
            ThreadingMode::Green,
            crate::test_utils::unit_runtime(),
        ));
        source.setup_thread_locals();
        source.lexenv = Value::string("b43-high1-unwind-driver");
        source.gc_collect_exact();
        source.tagged_heap.close_alloc_regions();
        let bootstrap = source.capture_green_bootstrap_for_test();
        let (_shell, start) = close(source).prepare_era().unwrap();
        let (tx, rx) = mpsc::channel();
        let service = std::thread::spawn(move || {
            let outcome = keeper_serve(start, |era| {
                era.keeper_baton_for_test()
                    .install_stowed_green_runtime_for_test(bootstrap);
                let world_handle = era.keeper_baton_for_test().test_world_handle_for_test();
                let control = world_handle.green_control_for_test();
                let era_world_handle = world_handle.era_world_handle_for_test();

                let (inbox, inbox_rx) = mpsc::channel();
                let mut driver = Driver::new(era, inbox_rx);
                let link = driver.enroll_group();
                let group_port = GroupPort::new(inbox.clone(), link);
                control
                    .open_era_stowed_for_test(era_world_handle, group_port)
                    .expect("open era on stowed Green Control");
                drop(inbox);

                let (end, held_after_poison) = std::thread::scope(|scope| {
                    let baton_control = Arc::clone(&control);
                    let baton_thread = scope.spawn(move || {
                        let right = baton_control
                            .acquire_initial_era_turn()
                            .expect("acquire initial TurnRight::Era");
                        let caught =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                                right.enter();
                                assert!(right.held_here());
                                let _held = right;
                                panic!("HIGH-1 live era Baton-thread panic");
                            }));
                        assert!(caught.is_err());
                        if let Some(port) = baton_control.take_era_port_for_exit() {
                            port.exit();
                        }
                        drop(world_handle);
                    });
                    let end = driver.run();
                    let held_after_poison = era.keeper_baton_for_test().held_here();
                    baton_thread.join().expect("Baton thread joined");
                    (end, held_after_poison)
                });
                assert!(
                    held_after_poison,
                    "HIGH-1: keeper Baton::held_here() must pass immediately after Poisoned(GroupAbandoned)"
                );
                assert!(
                    era.keeper_baton_for_test().held_here(),
                    "HIGH-1: keeper Baton::held_here() must pass on teardown"
                );
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
}

/// HIGH-2a: when `ThreadKey::Main` is `OwnerLocation::NativeReleased` and
/// marked `Wait::Runnable` (`main_excluded == true`), `eligible_runnable_count`
/// is 0 and waking Main's semantic wait emits ZERO spurious `BatonRequest`s.
#[test]
fn l67b_b43_main_excluded_native_released_does_not_livelock_in_era_idle() {
    use crate::emacs_core::eval::parallel_entry::driver_core::RingWaiter;

    let mut owner =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let baton = owner.start_green_runtime().unwrap();
    let world_handle = baton.era_world_handle().unwrap();
    let control = owner.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let _main_rx = control.register_main_guard_for_test();
    let escrowed = control
        .surrender_solo_for_era(TurnRight::Solo(baton))
        .expect("surrender Solo");
    let (inbox_tx, inbox_rx) = mpsc::channel();
    let (_world_tx, world_rx) = mpsc::sync_channel(1);
    let port = GroupPort::from_raw_parts_for_test(
        inbox_tx,
        world_rx,
        Arc::new(RingWaiter::new_for_test()),
    );
    control.open_era(world_handle, port).expect("open era");

    // Main is NativeReleased + Wait::Runnable (excluded until physical acquire).
    control.mark_main_native_released_for_test(Wait::Runnable);
    assert_eq!(
        control.eligible_runnable_count_for_test(),
        0,
        "HIGH-2a: main_excluded Runnable Main must not count as eligible"
    );

    // Waking Main while NativeReleased must NOT emit a spurious BatonRequest.
    let mx = MutexId::new(11);
    control.mark_main_native_released_for_test(Wait::Mutex(mx));
    assert_eq!(control.wake_waiting_for(Wait::Mutex(mx)), 1);
    assert_eq!(control.eligible_runnable_count_for_test(), 0);
    assert!(
        inbox_rx.try_recv().is_err(),
        "HIGH-2a: waking NativeReleased Main must not emit spurious BatonRequest"
    );

    let (_solo, _wh, _port) = control.close_era(escrowed).expect("close era");
}

/// HIGH-2b: when a worker exits via `finish_era` Case (i) while Main is the
/// sole surviving Baton thread and is blocked on a condition variable with NO
/// live parallel owner, `finish_era` runs QD2 `query_era_idle_for_deadlock()`,
/// wakes Main, and returns cleanly so the parked carrier slot never wedges
/// against `carriers.drain()`.
#[test]
fn l67b_b43_worker_exit_with_blocked_main_and_no_peer_reports_deadlock_and_drains_carrier() {
    run_in_live_green_era(
        "b43-finish-era-dl",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq b43-fin-mx (make-mutex \"b43-fin-mx\"))
                       (setq b43-fin-cv (make-condition-variable b43-fin-mx \"b43-fin-cv\"))
                       'ok)"
                ),
                "ok"
            );
        },
        false,
        |_state, _port, _home, _idle_receipt| {},
        move |main, _control| {
            // Spawn a worker that yields once and exits WITHOUT notifying
            // b43-fin-cv, while Main locks b43-fin-mx and blocks in
            // condition-wait. When the worker exits, finish_era enters Case (i)
            // with Main blocked and no live parallel owner -> QD2 wakes Main
            // and the worker's carrier returns cleanly (so retire's
            // carriers.drain() completes instead of wedging).
            let out = eval(
                main,
                "(progn
                   (mutex-lock b43-fin-mx)
                   (make-thread (lambda () (thread-yield) 'worker-exited))
                   (condition-wait b43-fin-cv)
                   (mutex-unlock b43-fin-mx)
                   'woken-by-qd2)",
            );
            assert_eq!(
                out, "woken-by-qd2",
                "HIGH-2b: finish_era QD2 must wake blocked Main and let carrier exit cleanly"
            );
        },
    );
}

/// LOW-1: `ActiveWorldSeat::from_active_green_seat` refuses a foreign-heap
/// `FacadeEpochRetention` with `WorldSeatRefusal::WrongHeap`, and supports the
/// full `b4.4` adopt -> `exit` -> `Control::collect_era_interval` -> `enter` ->
/// `into_active_green_parts` round-trip.
#[test]
fn l67b_b43_active_world_seat_from_active_green_seat_roundtrip_and_wrong_heap_retention() {
    use crate::emacs_core::eval::parallel_entry::driver_core::{GroupMsg, IntervalEnd, RingWaiter};
    use crate::emacs_core::eval::parallel_entry::keeper_driver::WorldLend;
    use crate::emacs_core::eval::parallel_entry::world_seat::{ActiveWorldSeat, WorldSeatRefusal};
    use crate::emacs_core::eval::parallel_entry::world_turn::{
        WorldReturned, WorldTurn, WorldTurnIdentity,
    };
    use std::num::NonZeroU64;

    // Construct both Contexts BEFORE either calls `start_green_runtime` so
    // `NativeTables` registration stays open during Context construction, and
    // keep `foreign` alive on its own OS thread until `foreign_retention` drops.
    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let (foreign_tx, foreign_rx) = mpsc::sync_channel(1);
    let (foreign_done_tx, foreign_done_rx) = mpsc::sync_channel::<()>(1);
    let foreign_thread = std::thread::spawn(move || {
        let mut foreign =
            Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
        let foreign_baton = foreign.start_green_runtime().expect("start foreign Green");
        let retention = foreign_baton
            .parallel_epoch_retention()
            .expect("foreign FacadeEpochRetention");
        foreign_tx.send(retention).expect("send foreign retention");
        foreign_done_rx.recv_timeout(BOUND).expect("foreign done");
        drop(foreign_baton);
        drop(foreign);
    });
    let foreign_retention = foreign_rx
        .recv_timeout(BOUND)
        .expect("receive foreign retention");

    let main_baton = main.start_green_runtime().expect("start main Green");
    let real_heap = main_baton.parallel_heap_identity();
    let world_handle = main_baton.era_world_handle().expect("era WorldHandle");
    let real_retention = main_baton
        .parallel_epoch_retention()
        .expect("real FacadeEpochRetention");
    let control = main.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let escrowed = control
        .surrender_solo_for_era(TurnRight::Solo(main_baton))
        .expect("surrender Solo(Baton) before opening era");
    main.seat_green_main_without_baton_for_test(Arc::clone(&control));

    // 1. Foreign-heap FacadeEpochRetention must be refused with WrongHeap.
    let (tx_bad, rx_bad) = mpsc::channel();
    let bad_turn = WorldTurn::cut_for_test(WorldTurnIdentity {
        heap: real_heap,
        epoch: NonZeroU64::new(1).unwrap(),
    });
    let bad_lend = WorldLend::for_test(tx_bad);
    let (bad_turn, bad_lend, foreign_retention, refusal) =
        ActiveWorldSeat::from_active_green_seat(&main, bad_turn, bad_lend, foreign_retention)
            .expect_err("LOW-1: mismatched retention heap must return WrongHeap");
    assert_eq!(refusal, WorldSeatRefusal::WrongHeap);
    bad_lend.give_back(WorldReturned::from_end(bad_turn));
    assert!(matches!(
        rx_bad.try_recv(),
        Ok(KeeperMsg::WorldReturn { .. })
    ));
    drop(foreign_retention);
    foreign_done_tx.send(()).expect("release foreign thread");
    foreign_thread.join().expect("join foreign thread");

    // 2. Matching retention exercises full b4.4 adopt -> exit -> collect_era_interval
    // -> enter -> into_active_green_parts round-trip.
    let (inbox_tx, inbox_rx) = mpsc::channel();
    let (world_tx, world_rx) = mpsc::sync_channel(1);
    let port = GroupPort::from_raw_parts_for_test(
        inbox_tx.clone(),
        world_rx,
        Arc::new(RingWaiter::new_for_test()),
    );
    control.open_era(world_handle, port).expect("open era");

    let epoch1 = NonZeroU64::new(2).unwrap();
    let epoch2 = NonZeroU64::new(3).unwrap();
    let id1 = WorldTurnIdentity {
        heap: real_heap,
        epoch: epoch1,
    };
    let id2 = WorldTurnIdentity {
        heap: real_heap,
        epoch: epoch2,
    };

    // Move EraLink into Holding via a clean scoped acquire thread while main
    // holds a temporary seat-stowed view, then adopt into ActiveWorldSeat.
    let prev_holder = main.with_green_runtime(|runtime| {
        runtime
            .carried_holder
            .replace(crate::emacs_core::eval::green::CarriedHolder::Stowed)
    });
    let (init_tx, init_rx) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let ctrl = Arc::clone(&control);
        let wtx = world_tx.clone();
        scope.spawn(move || {
            let right = ctrl.acquire_initial_era_turn().expect("acquire initial");
            init_tx.send(right).expect("send initial");
        });
        assert!(matches!(
            inbox_rx.recv_timeout(BOUND),
            Ok(KeeperMsg::BatonRequest)
        ));
        wtx.send(GroupMsg::World {
            turn: WorldTurn::cut_for_test(id1),
            epoch: epoch1,
        })
        .expect("lend epoch1");
    });
    main.with_green_runtime(|runtime| runtime.carried_holder.set(prev_holder));
    let initial_right = init_rx.recv_timeout(BOUND).expect("initial right");
    let (turn1, lend1, _wh1) = initial_right.into_era().unwrap().into_parts();

    let active = ActiveWorldSeat::from_active_green_seat(&main, turn1, lend1, real_retention)
        .expect("matching ActiveWorldSeat adoption succeeds");
    let (parked, lend1, returned1) = active.exit(&mut main);

    let (turn2, got_epoch, lend2) = std::thread::scope(|scope| {
        let ctrl = Arc::clone(&control);
        let h = scope.spawn(move || ctrl.collect_era_interval(lend1, returned1, None));
        assert!(matches!(
            inbox_rx.recv_timeout(BOUND),
            Ok(KeeperMsg::WorldReturn {
                end: IntervalEnd::Collect { .. },
                ..
            })
        ));
        world_tx
            .send(GroupMsg::World {
                turn: WorldTurn::cut_for_test(id2),
                epoch: epoch2,
            })
            .expect("re-lend epoch2 at ring HEAD");
        h.join().expect("collect thread").expect("collect ok")
    });
    assert_eq!(got_epoch, epoch2);

    let resumed = parked
        .enter(&mut main, turn2, lend2)
        .expect("re-enter seated main after collect_era_interval");
    let (turn2, lend2, retention2) = resumed.into_active_green_parts();
    assert_eq!(turn2.identity(), id2);
    assert_eq!(retention2.heap_identity(), real_heap);
    lend2.give_back(WorldReturned::from_end(turn2));
    drop(retention2);

    main.clear_cached_parked_root_batch_for_test();
    main.green_release_seat();
    let (solo_right, _wh, _port) = control.close_era(escrowed).expect("close era");
    drop(solo_right);
    drop(main);
}

/// MEDIUM-1: `prove_stowed_for_era_switch` refuses (panics) to mint
/// `StowedForReturn` when World carriers are stowed and carrier TLS is parked
/// but the seated Context has NOT published its parked roots in
/// `GreenRuntime::parked`.
#[test]
#[should_panic(expected = "Nit 6 / P1")]
fn l67b_b43_prove_stowed_for_era_switch_panics_without_published_parked_roots() {
    use crate::emacs_core::eval::parallel_entry::world_seat::prove_stowed_for_era_switch;

    let mut main = green_world();
    main.tagged_heap.close_alloc_regions();
    let _parked = main.park_owner();
    assert!(main.stow_serial_world_state());
    crate::emacs_core::eval::green::carrier_tls::park_carrier_tls();
    // Deliberately skip publishing parked roots into GreenRuntime::parked:
    // prove_stowed_for_era_switch MUST refuse rather than silently minting
    // StowedForReturn without parked roots.
    let _proof = prove_stowed_for_era_switch(&mut main);
}

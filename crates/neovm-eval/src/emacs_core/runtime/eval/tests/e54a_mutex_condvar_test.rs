//! E5.4a: `mutex-lock` / `mutex-unlock`, generic `ConditionSeat` `condition-wait`,
//! `condition-notify`, and D7 cross-kind wakes (`E5.3-E5.4-THREAD-CONTROL.md`
//! D3, D4, D7, Rows 7–11).

use crate::emacs_core::eval::green::{Control, ThreadKey, Wait};
use crate::emacs_core::eval::parallel_entry::keeper_driver::{
    ChannelPort, Driver, DriverEnd, GroupPort,
};
use crate::emacs_core::eval::parallel_entry::owned_source::era_fixture::{
    KeeperPanicGuard, activate, close, finish_fixture, keeper_serve,
};
use crate::emacs_core::eval::parallel_entry::serial_park::{KeeperPort, SerialHome, begin_turn};
use crate::emacs_core::eval::parallel_entry::serial_ticket::LocalSerialOwner;
use crate::emacs_core::eval::parallel_entry::serial_turn::OwnerSerialState;
use crate::emacs_core::eval::parallel_entry::thread_control::{
    ParallelOwnerControl, retire_mutator_rooted, retire_parallel_owner, with_world_parkable,
};
use crate::emacs_core::eval::world::TurnRight;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use neovm_value::tagged::gc::RendezvousError;
use neovm_value::value::Value;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BOUND: Duration = Duration::from_secs(30);
const PEER1_OWNER_ID: u64 = 9701;

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

fn standalone(_: &mut LocalSerialOwner) -> Result<(), Infallible> {
    Ok(())
}

fn retire_peer(
    state: OwnerSerialState<Context, LocalSerialOwner>,
    owner: ParallelOwnerControl,
    port: ChannelPort<'_>,
    home: SerialHome,
) {
    retire_parallel_owner(
        state,
        owner,
        None,
        port,
        home,
        standalone,
        |_| Ok(Value::NIL),
        retire_mutator_rooted,
    )
    .expect("retire parallel peer");
}

fn create_parallel_thread_handle(ev: &mut Context, var: &str, name: &str) -> ThreadKey {
    let id = ev.threads.create_thread(
        Value::NIL,
        crate::emacs_core::threads::ThreadName::from_lisp(Value::string(name))
            .expect("valid thread name"),
    );
    let val = ev
        .threads
        .thread_handle(id)
        .expect("freshly created thread handle");
    ev.obarray.set_symbol_value(var, val);
    ThreadKey::from_id(id)
}

fn wait_for_flag(flag: &AtomicBool, label: &str) {
    let start = Instant::now();
    while !flag.load(Ordering::Acquire) {
        assert!(
            start.elapsed() < BOUND,
            "{label}: timed out after {BOUND:?}"
        );
        std::thread::yield_now();
    }
}

fn wait_for_flag_on_baton(main: &mut Context, flag: &AtomicBool, label: &str) {
    let start = Instant::now();
    while !flag.load(Ordering::Acquire) {
        assert!(
            start.elapsed() < BOUND,
            "{label}: timed out after {BOUND:?}"
        );
        let _ = eval(main, "(sleep-for 0.005)");
    }
}

fn run_in_e54a_era<S, M, P>(label: &'static str, setup: S, peer_body: P, main_body: M)
where
    S: FnOnce(&mut Context),
    M: FnOnce(&mut Context, &Arc<Control>) + Send + 'static,
    P: FnOnce(
            OwnerSerialState<Context, LocalSerialOwner>,
            ChannelPort<'_>,
            SerialHome,
            &Arc<Control>,
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
        "E5.4a live era rows require a generational heap"
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

            let (end, body_panic, peer_panic) = std::thread::scope(|scope| {
                let permit = keeper
                    .register(PEER1_OWNER_ID, 0)
                    .expect("register parallel owner");
                let (grant_tx, grant_rx) = mpsc::sync_channel(1);
                driver.enroll(PEER1_OWNER_ID, permit.key(), grant_tx);
                let publishing_owners = driver.publishing_owners_counter();
                let peer_inbox = inbox.clone();
                let peer_control = Arc::clone(&control);
                let (peer_idle_tx, peer_idle_rx) = mpsc::sync_channel::<()>(1);
                let peer_handle = scope.spawn(move || {
                    let mutator = activate(permit, heap, PEER1_OWNER_ID);
                    let mut port = ChannelPort::new(
                        PEER1_OWNER_ID,
                        mutator,
                        peer_inbox,
                        grant_rx,
                        publishing_owners,
                    );
                    let start = Instant::now();
                    let _idle_receipt = loop {
                        match port.publish_idle_receipt_for_test() {
                            Ok(receipt) => break receipt,
                            Err(
                                crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal::Retry,
                            ) if start.elapsed() < BOUND => std::thread::yield_now(),
                            Err(err) => panic!("peer publish idle: {err:?}"),
                        }
                    };
                    peer_idle_tx.send(()).expect("peer idle ready");
                    let right = peer_setup_rx
                        .recv_timeout(BOUND)
                        .expect("peer setup TurnRight");
                    right.enter();
                    let owner = Context::new_green_owner_with_turn(&right);
                    let state: OwnerSerialState<Context, LocalSerialOwner> =
                        OwnerSerialState::new(PEER1_OWNER_ID, Box::new(owner))
                            .ok()
                            .expect("detach same-World parallel owner");
                    peer_right_back_tx
                        .send(right)
                        .expect("return setup TurnRight");
                    let home = SerialHome {
                        owner: PEER1_OWNER_ID,
                        heap,
                    };
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        peer_body(state, port, home, &peer_control);
                    }))
                    .err()
                });
                peer_idle_rx.recv_timeout(BOUND).expect("peer idle");
                drop(inbox);

                let main_control = Arc::clone(&control);
                let main_handle = scope.spawn(move || {
                    let mut initial_turn = main_control
                        .acquire_initial_era_turn()
                        .expect("acquire initial era WorldTurn");
                    peer_setup_tx
                        .send(initial_turn)
                        .expect("send initial TurnRight to peer setup");
                    initial_turn = peer_right_back_rx
                        .recv_timeout(BOUND)
                        .expect("receive initial TurnRight back from peer setup");
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
                let peer_panic = peer_handle.join().expect("parallel owner thread");
                let body_panic = main_handle.join().expect("main Baton thread");
                (end, body_panic, peer_panic)
            });

            assert_eq!(end, DriverEnd::Drained, "keeper driver must drain cleanly");
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
            let _ = panic_tx.send(body_panic.or(peer_panic));
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

/// Row 7a: Baton holds mutex; parallel owner contends via `parallel_mutex_lock`,
/// parks in `Wait::Mutex`, wakes on Baton unlock, resumes with its own
/// `(current-thread)` restored, and observes strict mutual-exclusion ordering.
#[test]
fn e54a_row7_mutex_contended_between_baton_and_parallel_owner() {
    let locked_by_main = Arc::new(AtomicBool::new(false));
    let locked_by_main_p = Arc::clone(&locked_by_main);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);

    run_in_e54a_era(
        "e54a-row7-mutex-contention",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-r7-mx (make-mutex \"e54a-r7-mx\"))
                       (setq e54a-r7-log nil)
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            wait_for_flag(&locked_by_main_p, "locked_by_main");
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("begin");
            let key =
                turn.with_world(|ev| create_parallel_thread_handle(ev, "e54a-r7-p1", "e54a-r7-p1"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);
            let out = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                ev.threads.enter_thread(key.id());
                eval(
                    ev,
                    "(progn
                       (mutex-lock e54a-r7-mx)
                       (setq e54a-r7-log (cons (list 'peer-entered (eq (current-thread) e54a-r7-p1)) e54a-r7-log))
                       (mutex-unlock e54a-r7-mx)
                       'peer-ok)",
                )
            });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            peer_done.store(true, Ordering::Release);
            assert_eq!(
                out, "peer-ok",
                "parallel owner must acquire contended mutex after Baton unlocks it"
            );
        },
        move |main, control| {
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e54a-r7-mx)
                       (setq e54a-r7-log (cons 'main-locked e54a-r7-log))
                       'locked)"
                ),
                "locked"
            );
            locked_by_main.store(true, Ordering::Release);
            let start = Instant::now();
            while !peer_done_m.load(Ordering::Acquire) {
                if let Some(peer_key) = *peer_key_m.lock().unwrap()
                    && matches!(control.wait_of(peer_key), Some(Wait::Mutex(_)))
                {
                    break;
                }
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for peer Wait::Mutex"
                );
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (setq e54a-r7-log (cons 'main-unlocking e54a-r7-log))
                       (mutex-unlock e54a-r7-mx)
                       'unlocked)"
                ),
                "unlocked"
            );
            wait_for_flag_on_baton(main, &peer_done_m, "peer_done");
            assert_eq!(
                eval(main, "(nreverse e54a-r7-log)"),
                "(main-locked main-unlocking (peer-entered t))",
                "mutual exclusion history must order main unlock strictly before peer lock"
            );
        },
    );
}

/// Row 7b + D7: Parallel owner locks mutex, Baton main contends and enters
/// `GroupTurn::Idle` (`Wait::Mutex`), parallel owner unlocks in a fresh turn ->
/// D7 `wake_locked -> coalesce_request_locked` immediately wakes Baton from
/// idle and preserves strict mutual exclusion.
#[test]
fn e54a_row7b_parallel_owner_holds_mutex_baton_wakes_from_era_idle() {
    let peer_locked = Arc::new(AtomicBool::new(false));
    let peer_locked_m = Arc::clone(&peer_locked);

    run_in_e54a_era(
        "e54a-row7b-par-holds-baton-wakes",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-7b-mx (make-mutex \"e54a-7b-mx\"))
                       (setq e54a-7b-log nil)
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            let key = {
                let (grant, lend) = port.request_turn().ok().expect("turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin1");
                let key = turn.with_world(|ev| {
                    let k = create_parallel_thread_handle(ev, "e54a-7b-p1", "e54a-7b-p1");
                    ev.threads.enter_thread(k.id());
                    assert_eq!(
                        eval(
                            ev,
                            "(progn
                               (mutex-lock e54a-7b-mx)
                               (setq e54a-7b-log (cons 'peer-locked e54a-7b-log))
                               'ok)"
                        ),
                        "ok"
                    );
                    k
                });
                turn.end(&mut port).ok().expect("end1");
                key
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            peer_locked.store(true, Ordering::Release);

            // Wait until Baton main is parked in `Wait::Mutex` (GroupTurn::Idle).
            let start = Instant::now();
            while !matches!(control.wait_of(ThreadKey::Main), Some(Wait::Mutex(_))) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for Baton main Wait::Mutex"
                );
                std::thread::yield_now();
            }

            // Unlock from a fresh serial turn -> D7 wakes idle Baton group!
            let unlock_out = {
                let (grant, lend) = port.request_turn().ok().expect("turn2");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin2");
                let out = turn.with_world(|ev| {
                    ev.threads.enter_thread(key.id());
                    eval(
                        ev,
                        "(progn
                           (setq e54a-7b-log (cons 'peer-unlocking e54a-7b-log))
                           (mutex-unlock e54a-7b-mx)
                           'unlocked)",
                    )
                });
                turn.end(&mut port).ok().expect("end2");
                out
            };
            retire_peer(state, owner, port, home);
            assert_eq!(unlock_out, "unlocked");
        },
        move |main, _control| {
            wait_for_flag_on_baton(main, &peer_locked_m, "peer_locked");
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e54a-7b-mx)
                       (setq e54a-7b-log (cons 'main-entered e54a-7b-log))
                       (mutex-unlock e54a-7b-mx)
                       'main-ok)"
                ),
                "main-ok"
            );
            assert_eq!(eval(main, "(thread-join e54a-7b-p1)"), "nil");
            assert_eq!(
                eval(main, "(nreverse e54a-7b-log)"),
                "(peer-locked peer-unlocking main-entered)",
                "Baton must acquire mutex strictly after parallel owner unlocks"
            );
        },
    );
}

/// Row 7c (GNU `src/thread.c:237-238`): Parallel owner blocked in fresh
/// `(mutex-lock mx)` interrupted by `thread-signal` raises the signal WITHOUT
/// acquiring the mutex (even when the mutex is simultaneously unlocked).
#[test]
fn e54a_row7c_parallel_mutex_lock_signal_raises_without_acquiring() {
    let locked_by_main = Arc::new(AtomicBool::new(false));
    let locked_by_main_p = Arc::clone(&locked_by_main);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);

    run_in_e54a_era(
        "e54a-row7c-mutex-lock-sig",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn (setq e54a-7c-mx (make-mutex \"e54a-7c-mx\")) 'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            wait_for_flag(&locked_by_main_p, "locked_by_main");
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("begin");
            let key =
                turn.with_world(|ev| create_parallel_thread_handle(ev, "e54a-7c-p1", "e54a-7c-p1"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);
            let (caught, unlock_attempt) =
                with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                    ev.threads.enter_thread(key.id());
                    let c = eval(
                        ev,
                        "(condition-case err
                             (progn (mutex-lock e54a-7c-mx) 'acquired)
                           (error (list 'caught (car (cdr err)))))",
                    );
                    // Prove this parallel owner does NOT hold `e54a-7c-mx` after signal.
                    let u = eval(ev, "(mutex-unlock e54a-7c-mx)");
                    (c, u)
                });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            peer_done.store(true, Ordering::Release);
            assert_eq!(caught, "(caught \"sig-during-lock\")");
            assert_eq!(
                unlock_attempt, "ERR error (\"Cannot unlock mutex owned by another thread\")",
                "interrupted fresh mutex-lock must NOT acquire the mutex (GNU thread.c:237-238)"
            );
        },
        move |main, control| {
            assert_eq!(
                eval(main, "(progn (mutex-lock e54a-7c-mx) 'locked)"),
                "locked"
            );
            locked_by_main.store(true, Ordering::Release);
            let start = Instant::now();
            loop {
                if let Some(peer_key) = *peer_key_m.lock().unwrap()
                    && matches!(control.wait_of(peer_key), Some(Wait::Mutex(_)))
                {
                    break;
                }
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for peer Wait::Mutex"
                );
                let _ = eval(main, "(sleep-for 0.005)");
            }
            // Send signal AND unlock in the same turn so the peer sees BOTH a
            // pending signal and an unlocked mutex on resume (GNU :237-238 must
            // still raise without taking the lock).
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (thread-signal e54a-7c-p1 'error '(\"sig-during-lock\"))
                       (mutex-unlock e54a-7c-mx)
                       'signalled)"
                ),
                "signalled"
            );
            wait_for_flag_on_baton(main, &peer_done_m, "peer_done");
            // Mutex must still be unlocked and immediately acquirable by main.
            assert_eq!(
                eval(
                    main,
                    "(progn (mutex-lock e54a-7c-mx) (mutex-unlock e54a-7c-mx) 'free)"
                ),
                "free"
            );
        },
    );
}

/// Row 8: Recursive depth 3 across `(condition-wait cv)` from a parallel owner
/// is released completely during wait (allowing Baton to lock + GC + notify)
/// and restored to EXACT depth 3 upon return.
#[test]
fn e54a_row8_parallel_condition_wait_restores_recursive_depth_3_across_gc() {
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);

    run_in_e54a_era(
        "e54a-row8-condwait-depth3",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-r8-mx (make-mutex \"e54a-r8-mx\"))
                       (setq e54a-r8-cv (make-condition-variable e54a-r8-mx \"e54a-r8-cv\"))
                       (setq e54a-r8-ready nil)
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("begin");
            let key =
                turn.with_world(|ev| create_parallel_thread_handle(ev, "e54a-r8-p1", "e54a-r8-p1"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);
            let out = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                ev.threads.enter_thread(key.id());
                eval(
                    ev,
                    "(progn
                       (mutex-lock e54a-r8-mx)
                       (mutex-lock e54a-r8-mx)
                       (mutex-lock e54a-r8-mx)
                       (while (null e54a-r8-ready)
                         (condition-wait e54a-r8-cv))
                       (let ((same-thr (eq (current-thread) e54a-r8-p1)))
                         ;; Unlock #1 and #2 must leave mutex still held (depth 3 -> 2 -> 1);
                         ;; unlock #3 releases it; unlock #4 must fail as non-owner!
                         (mutex-unlock e54a-r8-mx)
                         (mutex-unlock e54a-r8-mx)
                         (mutex-unlock e54a-r8-mx)
                         (let ((fourth (condition-case err
                                           (progn (mutex-unlock e54a-r8-mx) 'unexpected-ok)
                                         (error (car (cdr err))))))
                           (list same-thr e54a-r8-ready fourth))))",
                )
            });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            peer_done.store(true, Ordering::Release);
            assert_eq!(
                out, "(t notified \"Cannot unlock mutex owned by another thread\")",
                "condition-wait from parallel owner must restore exact recursive depth 3 and thread identity"
            );
        },
        move |main, control| {
            let start = Instant::now();
            loop {
                if let Some(peer_key) = *peer_key_m.lock().unwrap()
                    && matches!(control.wait_of(peer_key), Some(Wait::Condition(_)))
                {
                    break;
                }
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for peer Wait::Condition"
                );
                let _ = eval(main, "(sleep-for 0.005)");
            }
            // Acquire the mutex (proving all 3 locks were released!), run GC
            // while peer is parked in Wait::Condition, set predicate, notify,
            // and unlock.
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e54a-r8-mx)
                       (garbage-collect)
                       (setq e54a-r8-ready 'notified)
                       (condition-notify e54a-r8-cv)
                       (mutex-unlock e54a-r8-mx)
                       'sent)"
                ),
                "sent"
            );
            wait_for_flag_on_baton(main, &peer_done_m, "peer_done");
        },
    );
}

/// Row 9a + D7: Parallel owner calls `(condition-notify cv)` to wake a Baton
/// waiter parked in `GroupTurn::Idle` (`Wait::Condition`).
#[test]
fn e54a_row9a_parallel_condition_notify_wakes_idle_baton_waiter() {
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));

    run_in_e54a_era(
        "e54a-row9a-par-notify-baton",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-9a-mx (make-mutex \"e54a-9a-mx\"))
                       (setq e54a-9a-cv (make-condition-variable e54a-9a-mx \"e54a-9a-cv\"))
                       (setq e54a-9a-flag nil)
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            let key = {
                let (grant, lend) = port.request_turn().ok().expect("turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin1");
                let key = turn
                    .with_world(|ev| create_parallel_thread_handle(ev, "e54a-9a-p1", "e54a-9a-p1"));
                turn.end(&mut port).ok().expect("end1");
                key
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);

            // Wait until Baton main is parked in `Wait::Condition` (GroupTurn::Idle).
            let start = Instant::now();
            while !matches!(control.wait_of(ThreadKey::Main), Some(Wait::Condition(_))) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for Baton main Wait::Condition"
                );
                std::thread::yield_now();
            }

            let notify_out = {
                let (grant, lend) = port.request_turn().ok().expect("turn2");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin2");
                let out = turn.with_world(|ev| {
                    ev.threads.enter_thread(key.id());
                    eval(
                        ev,
                        "(progn
                           (mutex-lock e54a-9a-mx)
                           (setq e54a-9a-flag 'from-peer)
                           (condition-notify e54a-9a-cv)
                           (mutex-unlock e54a-9a-mx)
                           'notified)",
                    )
                });
                turn.end(&mut port).ok().expect("end2");
                out
            };
            retire_peer(state, owner, port, home);
            assert_eq!(notify_out, "notified");
        },
        move |main, _control| {
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e54a-9a-mx)
                       (while (null e54a-9a-flag)
                         (condition-wait e54a-9a-cv))
                       (mutex-unlock e54a-9a-mx)
                       e54a-9a-flag)"
                ),
                "from-peer",
                "parallel owner condition-notify must wake Baton waiter from GroupTurn::Idle"
            );
            assert_eq!(eval(main, "(thread-join e54a-9a-p1)"), "nil");
        },
    );
}

/// Row 9b: `(condition-notify cv)` with `ALL = nil` (`NotificationScope::One`)
/// across a Baton waiter and a parallel waiter picks the lowest `ThreadKey`.
#[test]
fn e54a_row9b_condition_notify_one_across_kinds_picks_lowest_key() {
    let baton_worker_waiting = Arc::new(AtomicBool::new(false));
    let baton_worker_waiting_p = Arc::clone(&baton_worker_waiting);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);

    run_in_e54a_era(
        "e54a-row9b-notify-one-lowest",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-9b-mx (make-mutex \"e54a-9b-mx\"))
                       (setq e54a-9b-cv (make-condition-variable e54a-9b-mx \"e54a-9b-cv\"))
                       (setq e54a-9b-order nil)
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            // Create the parallel handle AFTER the Baton worker so
            // `key_baton < key_parallel`.
            wait_for_flag(&baton_worker_waiting_p, "baton_worker_waiting");
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("begin");
            let key =
                turn.with_world(|ev| create_parallel_thread_handle(ev, "e54a-9b-p1", "e54a-9b-p1"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);
            let out = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                ev.threads.enter_thread(key.id());
                eval(
                    ev,
                    "(progn
                       (mutex-lock e54a-9b-mx)
                       (condition-wait e54a-9b-cv)
                       (setq e54a-9b-order (cons 'parallel e54a-9b-order))
                       (mutex-unlock e54a-9b-mx)
                       'peer-woke)",
                )
            });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            peer_done.store(true, Ordering::Release);
            assert_eq!(out, "peer-woke");
        },
        move |main, control| {
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (setq e54a-9b-baton
                             (make-thread
                               (lambda ()
                                 (mutex-lock e54a-9b-mx)
                                 (condition-wait e54a-9b-cv)
                                 (setq e54a-9b-order (cons 'baton e54a-9b-order))
                                 (mutex-unlock e54a-9b-mx)
                                 'baton-woke)))
                       (thread-yield)
                       'spawned)"
                ),
                "spawned"
            );
            let baton_key = {
                let val = main.eval_str("e54a-9b-baton").unwrap();
                ThreadKey::from_id(main.threads.thread_id_from_handle(&val).unwrap())
            };
            let start = Instant::now();
            while !matches!(control.wait_of(baton_key), Some(Wait::Condition(_))) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for Baton worker Wait::Condition"
                );
                let _ = eval(main, "(thread-yield)");
            }
            baton_worker_waiting.store(true, Ordering::Release);

            // Wait until parallel owner is ALSO parked in `Wait::Condition`.
            let peer_key = {
                let start = Instant::now();
                loop {
                    if let Some(pk) = *peer_key_m.lock().unwrap()
                        && matches!(control.wait_of(pk), Some(Wait::Condition(_)))
                    {
                        break pk;
                    }
                    assert!(
                        start.elapsed() < BOUND,
                        "timed out waiting for peer Wait::Condition"
                    );
                    let _ = eval(main, "(sleep-for 0.005)");
                }
            };
            assert!(
                baton_key < peer_key,
                "test precondition: Baton worker key ({baton_key:?}) must be < parallel key ({peer_key:?})"
            );

            // First notify-one: MUST wake ONLY `baton_key`, leaving `peer_key` in Wait::Condition!
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e54a-9b-mx)
                       (condition-notify e54a-9b-cv)
                       (mutex-unlock e54a-9b-mx)
                       (thread-join e54a-9b-baton))"
                ),
                "baton-woke"
            );
            assert!(
                matches!(control.wait_of(peer_key), Some(Wait::Condition(_))),
                "notify-one must leave higher-key parallel waiter parked in Wait::Condition"
            );

            // Second notify-one: wakes `peer_key`.
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e54a-9b-mx)
                       (condition-notify e54a-9b-cv)
                       (mutex-unlock e54a-9b-mx)
                       'second-sent)"
                ),
                "second-sent"
            );
            wait_for_flag_on_baton(main, &peer_done_m, "peer_done");
            assert_eq!(
                eval(main, "(nreverse e54a-9b-order)"),
                "(baton parallel)",
                "notify-one across kinds must wake lowest ThreadKey first"
            );
        },
    );
}

/// Row 10 (GNU `src/thread.c:232-234, 470-475`): Parallel owner in
/// `(condition-wait cv)` at recursive depth 2 interrupted by `thread-signal`
/// while Baton still holds `mx` MUST park in `Wait::Mutex` (ignoring signal
/// during saved-count relock), reacquire `mx` at exact depth 2 once Baton
/// unlocks, and ONLY THEN raise the pending signal with `mx` held in the handler.
#[test]
fn e54a_row10_parallel_condition_wait_signal_relocks_saved_depth_before_raising() {
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);

    run_in_e54a_era(
        "e54a-row10-condwait-sig-relock",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-r10-mx (make-mutex \"e54a-r10-mx\"))
                       (setq e54a-r10-cv (make-condition-variable e54a-r10-mx \"e54a-r10-cv\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("begin");
            let key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e54a-r10-p1", "e54a-r10-p1"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);
            let out = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                ev.threads.enter_thread(key.id());
                eval(
                    ev,
                    "(progn
                       (mutex-lock e54a-r10-mx)
                       (mutex-lock e54a-r10-mx)
                       (condition-case err
                           (progn (condition-wait e54a-r10-cv) 'no-signal)
                         (error
                           ;; Inside the handler, mutex MUST be held at exact depth 2!
                           (mutex-unlock e54a-r10-mx)
                           (mutex-unlock e54a-r10-mx)
                           (let ((third (condition-case e2
                                            (progn (mutex-unlock e54a-r10-mx) 'unexpected-ok)
                                          (error (car (cdr e2))))))
                             (list 'caught (car (cdr err)) third)))))",
                )
            });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            peer_done.store(true, Ordering::Release);
            assert_eq!(
                out,
                "(caught \"sig-during-cwait\" \"Cannot unlock mutex owned by another thread\")",
                "signal during condition-wait must reacquire saved depth 2 BEFORE raising"
            );
        },
        move |main, control| {
            let peer_key = {
                let start = Instant::now();
                loop {
                    if let Some(pk) = *peer_key_m.lock().unwrap()
                        && matches!(control.wait_of(pk), Some(Wait::Condition(_)))
                    {
                        break pk;
                    }
                    assert!(
                        start.elapsed() < BOUND,
                        "timed out waiting for peer Wait::Condition"
                    );
                    let _ = eval(main, "(sleep-for 0.005)");
                }
            };
            // Lock `mx` and send `thread-signal` while STILL holding `mx`!
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e54a-r10-mx)
                       (thread-signal e54a-r10-p1 'error '(\"sig-during-cwait\"))
                       'signalled-while-held)"
                ),
                "signalled-while-held"
            );
            // Peer must wake from Wait::Condition and park in Wait::Mutex
            // (MUST NOT raise early while Baton still holds `mx`!).
            let start = Instant::now();
            while !matches!(control.wait_of(peer_key), Some(Wait::Mutex(_))) {
                assert!(
                    !peer_done_m.load(Ordering::Acquire),
                    "peer must NOT exit condition-wait before reacquiring held mutex!"
                );
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for peer Wait::Mutex during relock"
                );
                let _ = eval(main, "(sleep-for 0.005)");
            }
            // Now release `mx` so peer relocks at depth 2 and raises the signal.
            assert_eq!(
                eval(main, "(progn (mutex-unlock e54a-r10-mx) 'released)"),
                "released"
            );
            wait_for_flag_on_baton(main, &peer_done_m, "peer_done");
        },
    );
}

/// Row 11: Non-owner `(mutex-unlock)` and `(condition-wait)` errors from a
/// parallel owner, plus GNU `post_acquire_global_lock` signal delivery on
/// `mutex-unlock` release (only on final release, NOT `StillLocked`) and
/// `condition-notify` (`src/thread.c:258-271, 377-378, 498-499, 533`).
#[test]
fn e54a_row11_non_owner_errors_and_release_notify_signal_delivery() {
    let main_locked = Arc::new(AtomicBool::new(false));
    let main_locked_p = Arc::clone(&main_locked);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);

    run_in_e54a_era(
        "e54a-row11-errors-and-delivery",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-r11-free-mx (make-mutex \"e54a-r11-free-mx\"))
                       (setq e54a-r11-main-mx (make-mutex \"e54a-r11-main-mx\"))
                       (setq e54a-r11-main-cv (make-condition-variable e54a-r11-main-mx \"e54a-r11-main-cv\"))
                       (setq e54a-r11-peer-mx (make-mutex \"e54a-r11-peer-mx\"))
                       (setq e54a-r11-peer-cv (make-condition-variable e54a-r11-peer-mx \"e54a-r11-peer-cv\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            wait_for_flag(&main_locked_p, "main_locked");
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("begin");
            let key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e54a-r11-p1", "e54a-r11-p1"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            let (
                unlock_free,
                unlock_other,
                cwait_unheld,
                still_locked_no_sig,
                release_delivers_sig,
                notify_delivers_sig,
            ) = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                ev.threads.enter_thread(key.id());
                let u_free = eval(ev, "(mutex-unlock e54a-r11-free-mx)");
                let u_other = eval(ev, "(mutex-unlock e54a-r11-main-mx)");
                let cw_unheld = eval(ev, "(condition-wait e54a-r11-main-cv)");

                // Lock peer-mx twice, inject a pending signal on self, then:
                // - 1st unlock decrements depth 2 -> 1 (`StillLocked`): MUST NOT raise!
                // - 2nd unlock releases (`Released`): MUST raise (`thread.c:377-378`)!
                let self_val = ev.threads.thread_handle(key.id()).expect("self handle");
                assert_eq!(
                    eval(
                        ev,
                        "(progn (mutex-lock e54a-r11-peer-mx) (mutex-lock e54a-r11-peer-mx) 'ok)"
                    ),
                    "ok"
                );
                ev.threads.set_thread_error_for_value(
                    self_val,
                    Value::symbol("error"),
                    Value::list(vec![Value::string("sig-on-release")]),
                );
                let still = eval(
                    ev,
                    "(condition-case err
                         (progn (mutex-unlock e54a-r11-peer-mx) 'still-locked-ok)
                       (error (list 'unexpected-early (car (cdr err)))))",
                );
                let released = eval(
                    ev,
                    "(condition-case err
                         (progn (mutex-unlock e54a-r11-peer-mx) 'not-delivered)
                       (error (list 'delivered (car (cdr err)))))",
                );

                // Lock peer-mx, inject a pending signal on self, call
                // `(condition-notify e54a-r11-peer-cv)` -> MUST deliver (`thread.c:533`)!
                assert_eq!(eval(ev, "(progn (mutex-lock e54a-r11-peer-mx) 'ok)"), "ok");
                ev.threads.set_thread_error_for_value(
                    self_val,
                    Value::symbol("error"),
                    Value::list(vec![Value::string("sig-on-notify")]),
                );
                let notified = eval(
                    ev,
                    "(condition-case err
                         (progn (condition-notify e54a-r11-peer-cv) 'not-delivered)
                       (error (list 'delivered (car (cdr err)))))",
                );
                assert_eq!(
                    eval(ev, "(progn (mutex-unlock e54a-r11-peer-mx) 'ok)"),
                    "ok"
                );
                (u_free, u_other, cw_unheld, still, released, notified)
            });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            peer_done.store(true, Ordering::Release);

            assert_eq!(
                unlock_free,
                "ERR error (\"Cannot unlock mutex owned by another thread\")"
            );
            assert_eq!(
                unlock_other,
                "ERR error (\"Cannot unlock mutex owned by another thread\")"
            );
            assert_eq!(
                cwait_unheld,
                "ERR error (\"Condition variable's mutex is not held by current thread\")"
            );
            assert_eq!(
                still_locked_no_sig, "still-locked-ok",
                "recursive mutex-unlock (StillLocked) must NOT deliver pending signal (GNU thread.c:264-265,377)"
            );
            assert_eq!(
                release_delivers_sig, "(delivered \"sig-on-release\")",
                "releasing mutex-unlock must deliver pending signal (GNU thread.c:377-378)"
            );
            assert_eq!(
                notify_delivers_sig, "(delivered \"sig-on-notify\")",
                "condition-notify must deliver pending signal (GNU thread.c:533)"
            );
        },
        move |main, _control| {
            assert_eq!(
                eval(main, "(progn (mutex-lock e54a-r11-main-mx) 'locked)"),
                "locked"
            );
            main_locked.store(true, Ordering::Release);
            wait_for_flag_on_baton(main, &peer_done_m, "peer_done");
            assert_eq!(
                eval(main, "(progn (mutex-unlock e54a-r11-main-mx) 'unlocked)"),
                "unlocked"
            );
        },
    );
}

// ===========================================================================
// E5.4a R2 Row 12a (Gap 1): `Lifecycle::Closing` wakes a parked
// `parallel_thread_join` with `Flow::shutdown` (no infinite spin) and cleans
// up blocker, wait_condvar, and VM root frame.
// ===========================================================================
#[test]
fn e54a_r2_closing_wakes_parked_join_with_shutdown_no_spin() {
    let target_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let target_key_peer = Arc::clone(&target_key_slot);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_main = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let peer_done_m = Arc::clone(&peer_done);
    let watchdog_handle = Arc::new(std::sync::Mutex::new(None::<std::thread::JoinHandle<()>>));
    let watchdog_handle_m = Arc::clone(&watchdog_handle);
    let timed_out = Arc::new(AtomicBool::new(false));
    let timed_out_m = Arc::clone(&timed_out);

    run_in_e54a_era(
        "e54a-r12a-closing-join",
        move |src| {
            let target_key = create_parallel_thread_handle(src, "e54a-r12a-target", "r12a-target");
            *target_key_slot.lock().unwrap() = Some(target_key);
        },
        move |mut state, mut port, home, control| {
            let target_key = target_key_peer.lock().unwrap().expect("target key");
            let target_owner = ParallelOwnerControl::register(Arc::clone(control), target_key);
            let grant = port.request_turn().expect("request turn");
            let mut turn = begin_turn(&mut state, home, grant, &mut port, standalone)
                .ok()
                .expect("begin turn");
            let peer_key = turn.with_world(|ev| {
                let key = create_parallel_thread_handle(ev, "e54a-r12a-peer", "r12a-peer");
                ev.threads.enter_thread(key.id());
                key
            });
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), peer_key);
            *peer_key_slot.lock().unwrap() = Some(peer_key);

            let (eval_res, blocker_after, wait_cv_after, root_depth_before, root_depth_after) =
                with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                    let before = ev.vm_root_frames_len_for_test();
                    let res = ev.eval_str("(thread-join e54a-r12a-target)");
                    let after = ev.vm_root_frames_len_for_test();
                    let blocker = eval(ev, "(thread--blocker e54a-r12a-peer)");
                    let wait_cv = ev.threads.in_wait_condvar(peer_key.id());
                    (res, blocker, wait_cv, before, after)
                });
            turn.end(&mut port).ok().expect("end");
            // Keep target_owner unregistered cleanly on exit.
            drop(target_owner);
            retire_peer(state, owner, port, home);
            {
                let (lock, cvar) = &*peer_done;
                *lock.lock().unwrap() = true;
                cvar.notify_all();
            }
            if let Some(handle) = watchdog_handle.lock().unwrap().take() {
                handle.join().expect("join closing spin watchdog");
            }

            assert!(
                !timed_out.load(Ordering::Acquire),
                "parallel_thread_join spun past Lifecycle::Closing instead of returning Flow::shutdown"
            );
            assert!(
                matches!(eval_res, Err(neovm_value::error::EvalError::Shutdown(_))),
                "expected Flow::shutdown on Lifecycle::Closing, got {eval_res:?}"
            );
            assert_eq!(
                blocker_after, "nil",
                "thread_blocker must be cleared on shutdown"
            );
            assert!(!wait_cv_after, "wait_condvar must be cleared on shutdown");
            assert_eq!(
                root_depth_after, root_depth_before,
                "VM root frame must be popped cleanly on shutdown"
            );
        },
        move |main, control| {
            let start = Instant::now();
            let peer_key = loop {
                if let Some(key) = *peer_key_main.lock().unwrap() {
                    break key;
                }
                assert!(start.elapsed() < BOUND, "timed out waiting for peer key");
                let _ = eval(main, "(sleep-for 0.005)");
            };
            while !matches!(control.wait_of(peer_key), Some(Wait::Join(_))) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for peer Wait::Join"
                );
                let _ = eval(main, "(sleep-for 0.005)");
            }
            let target_key = match control.wait_of(peer_key) {
                Some(Wait::Join(k)) => k,
                other => panic!("expected Wait::Join, got {other:?}"),
            };
            // Bounded hang-cap watchdog: spawn and store its JoinHandle BEFORE
            // `control.close()` so peer always joins it before era teardown
            // (retiring its `Arc<Control>` cleanly). If `parallel_thread_join`
            // spins instead of returning `Flow::shutdown`, break the spin after
            // 1.5s so a regression fails loudly instead of hanging the runner.
            let wd_control = Arc::clone(control);
            let wd_done = Arc::clone(&peer_done_m);
            let wd_timeout = Arc::clone(&timed_out_m);
            let handle = std::thread::spawn(move || {
                let (lock, cvar) = &*wd_done;
                let mut done = lock.lock().unwrap();
                let deadline = Instant::now() + Duration::from_millis(1500);
                while !*done {
                    let Some(rem) = deadline.checked_duration_since(Instant::now()) else {
                        wd_timeout.store(true, Ordering::Release);
                        wd_control.publish_owner_exit(target_key);
                        return;
                    };
                    let (guard, _) = cvar.wait_timeout(done, rem).unwrap();
                    done = guard;
                }
            });
            *watchdog_handle_m.lock().unwrap() = Some(handle);
            control.close();
        },
    );
}

// ===========================================================================
// E5.4a R2 Row 12b (Gap 1): `Lifecycle::Closing` wakes a parked
// `parallel_condition_wait` with `Flow::shutdown` (no relock / no spin) and
// cleans up blocker, wait_condvar, ConditionWait state, and VM root frame.
// ===========================================================================
#[test]
fn e54a_r2_closing_wakes_parked_condition_wait_with_shutdown_and_cleanup() {
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_main = Arc::clone(&peer_key_slot);

    run_in_e54a_era(
        "e54a-r12b-closing-condvar",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-r12b-mx (make-mutex \"r12b-mx\"))
                       (setq e54a-r12b-cv (make-condition-variable e54a-r12b-mx \"r12b-cv\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            let grant = port.request_turn().expect("request turn");
            let mut turn = begin_turn(&mut state, home, grant, &mut port, standalone)
                .ok()
                .expect("begin turn");
            let peer_key = turn.with_world(|ev| {
                let key = create_parallel_thread_handle(ev, "e54a-r12b-peer", "r12b-peer");
                ev.threads.enter_thread(key.id());
                key
            });
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), peer_key);
            *peer_key_slot.lock().unwrap() = Some(peer_key);

            let (eval_res, blocker_after, wait_cv_after, root_depth_before, root_depth_after) =
                with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                    assert_eq!(
                        eval(ev, "(progn (mutex-lock e54a-r12b-mx) 'locked)"),
                        "locked"
                    );
                    let before = ev.vm_root_frames_len_for_test();
                    let res = ev.eval_str("(condition-wait e54a-r12b-cv)");
                    let after = ev.vm_root_frames_len_for_test();
                    let blocker = eval(ev, "(thread--blocker e54a-r12b-peer)");
                    let wait_cv = ev.threads.in_wait_condvar(peer_key.id());
                    (res, blocker, wait_cv, before, after)
                });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);

            assert!(
                matches!(eval_res, Err(neovm_value::error::EvalError::Shutdown(_))),
                "expected Flow::shutdown from condition-wait on Lifecycle::Closing, got {eval_res:?}"
            );
            assert_eq!(
                blocker_after, "nil",
                "thread_blocker must be cleared on shutdown"
            );
            assert!(!wait_cv_after, "wait_condvar must be cleared on shutdown");
            assert_eq!(
                root_depth_after, root_depth_before,
                "VM root frame must be popped cleanly on shutdown"
            );
        },
        move |main, control| {
            let start = Instant::now();
            let peer_key = loop {
                if let Some(key) = *peer_key_main.lock().unwrap() {
                    break key;
                }
                assert!(start.elapsed() < BOUND, "timed out waiting for peer key");
                let _ = eval(main, "(sleep-for 0.005)");
            };
            while !matches!(control.wait_of(peer_key), Some(Wait::Condition(_))) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for peer Wait::Condition"
                );
                let _ = eval(main, "(sleep-for 0.005)");
            }
            control.close();
        },
    );
}

// ===========================================================================
// E5.4a R2 Row 13 (Gap 2 — lead #16120): `(current-thread)` and a self-owned
// `(mutex-unlock m)` succeed from a parallel owner AFTER a park/reattach where
// Main ran Lisp in between (overwriting World-global `current_thread_id`), AND
// `with_world_collectible` preserves `current_thread_id` across a mid-turn GC.
// ===========================================================================
#[test]
fn e54a_r2_current_thread_and_self_mutex_unlock_after_park_reattach() {
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_main = Arc::clone(&peer_key_slot);
    let main_ran_between = Arc::new(AtomicBool::new(false));
    let main_ran_peer = Arc::clone(&main_ran_between);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);

    run_in_e54a_era(
        "e54a-r13-current-thread-reattach",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-r13-mx (make-mutex \"r13-mx\"))
                       (setq e54a-r13-held2 (make-mutex \"r13-held2\"))
                       (setq e54a-r13-cv (make-condition-variable e54a-r13-mx \"r13-cv\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            let grant = port.request_turn().expect("request turn");
            let mut turn = begin_turn(&mut state, home, grant, &mut port, standalone)
                .ok()
                .expect("begin turn");
            let peer_key = turn.with_world(|ev| {
                let key = create_parallel_thread_handle(ev, "e54a-r13-peer", "r13-peer");
                ev.threads.enter_thread(key.id());
                key
            });
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), peer_key);
            *peer_key_slot.lock().unwrap() = Some(peer_key);

            let (ct_before, ct_after_yield, ct_after_cv, unlock_res, unlock_held2) =
                with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                    let before = eval(ev, "(eq (current-thread) e54a-r13-peer)");
                    assert_eq!(
                        eval(
                            ev,
                            "(progn (mutex-lock e54a-r13-mx) (mutex-lock e54a-r13-held2) 'locked)"
                        ),
                        "locked"
                    );
                    assert_eq!(eval(ev, "(progn (thread-yield) 'yielded)"), "yielded");
                    let after_yield = eval(ev, "(eq (current-thread) e54a-r13-peer)");
                    // Park on condition-wait while STILL holding `e54a-r13-held2`
                    // as peer, while Main runs (setting current_thread=0), notifies,
                    // and unlocks `e54a-r13-mx`. Unlocking `e54a-r13-held2` after
                    // reattach cannot pass by luck via a stale relock.
                    assert_eq!(
                        eval(ev, "(progn (condition-wait e54a-r13-cv) 'woke)"),
                        "woke"
                    );
                    let after_cv = eval(ev, "(eq (current-thread) e54a-r13-peer)");
                    let unlock = eval(ev, "(progn (mutex-unlock e54a-r13-mx) 'unlocked)");
                    let unlock2 = eval(ev, "(progn (mutex-unlock e54a-r13-held2) 'unlocked2)");
                    (before, after_yield, after_cv, unlock, unlock2)
                });
            // Also verify `with_world_collectible` preserves `current_thread_id`
            // across a mid-turn `(garbage-collect)` detach/reattach.
            let (ct_after_gc, unlock_after_gc) =
                turn.with_world_collectible(&mut port, standalone, |ev| {
                    ev.threads.enter_thread(peer_key.id());
                    assert_eq!(
                        eval(ev, "(progn (mutex-lock e54a-r13-mx) 'locked)"),
                        "locked"
                    );
                    let _ = eval(ev, "(garbage-collect)");
                    let ct = eval(ev, "(eq (current-thread) e54a-r13-peer)");
                    let ul = eval(ev, "(progn (mutex-unlock e54a-r13-mx) 'unlocked)");
                    (ct, ul)
                });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            peer_done.store(true, Ordering::Release);

            assert!(main_ran_peer.load(Ordering::Acquire));
            assert_eq!(ct_before, "t");
            assert_eq!(
                ct_after_yield, "t",
                "(current-thread) must remain peer after thread-yield park/reattach"
            );
            assert_eq!(
                ct_after_cv, "t",
                "(current-thread) must remain peer after condition-wait park/reattach"
            );
            assert_eq!(
                unlock_res, "unlocked",
                "self-owned (mutex-unlock) must succeed after park/reattach"
            );
            assert_eq!(
                unlock_held2, "unlocked2",
                "second held mutex (owned by peer before condition-wait) must unlock cleanly after park/reattach"
            );
            assert_eq!(
                ct_after_gc, "t",
                "(current-thread) must remain peer after with_world_collectible GC"
            );
            assert_eq!(
                unlock_after_gc, "unlocked",
                "self-owned (mutex-unlock) must succeed after with_world_collectible GC"
            );
        },
        move |main, control| {
            let start = Instant::now();
            let peer_key = loop {
                if let Some(key) = *peer_key_main.lock().unwrap() {
                    break key;
                }
                assert!(start.elapsed() < BOUND, "timed out waiting for peer key");
                let _ = eval(main, "(sleep-for 0.005)");
            };
            while !matches!(control.wait_of(peer_key), Some(Wait::Condition(_))) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for peer Wait::Condition"
                );
                let _ = eval(main, "(sleep-for 0.005)");
            }
            // Main runs Lisp on Baton, confirming World-global current_thread is Main (0).
            assert_eq!(eval(main, "(eq (current-thread) main-thread)"), "t");
            main_ran_between.store(true, Ordering::Release);
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e54a-r13-mx)
                       (condition-notify e54a-r13-cv)
                       (mutex-unlock e54a-r13-mx)
                       'notified)"
                ),
                "notified"
            );
            wait_for_flag_on_baton(main, &peer_done_m, "peer_done");
        },
    );
}

// ===========================================================================
// E5.4a R3 Row 7d (Blocker 3 — GNU `src/thread.c:232-238`, attested Form 5/6):
// When `condition_stack` is empty (bare `eval_str` / newborn before top-level
// handler) and a pending `thread-signal` is present, contended `(mutex-lock m)`
// must NEITHER park NOR acquire `m`, leaving the signal + `SignalHint` pending
// until a handler is installed.
// ===========================================================================
#[test]
fn e54a_r3_row7d_mutex_lock_empty_condition_stack_pending_signal_no_park_no_acquire() {
    let main_locked = Arc::new(AtomicBool::new(false));
    let main_locked_p = Arc::clone(&main_locked);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);

    run_in_e54a_era(
        "e54a-r3-row7d-empty-handler",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn (setq e54a-r7d-mx (make-mutex \"r7d-mx\")) 'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            wait_for_flag(&main_locked_p, "main_locked");
            let grant = port.request_turn().expect("request turn");
            let mut turn = begin_turn(&mut state, home, grant, &mut port, standalone)
                .ok()
                .expect("begin turn");
            let peer_key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e54a-r7d-peer", "r7d-peer"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), peer_key);

            let (lock_res, owned_after, still_pending) =
                with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                    assert!(
                        ev.condition_stack.is_empty(),
                        "precondition: bare eval_str has empty condition_stack"
                    );
                    let peer_val = ev.eval_str("e54a-r7d-peer").unwrap();
                    ev.threads.set_thread_error_for_value(
                        peer_val,
                        Value::symbol("error"),
                        Value::list(vec![Value::string("r7d-pending")]),
                    );
                    control.raise_parallel_signal_hint(peer_key);

                    // Main STILL holds `e54a-r7d-mx` and will NOT unlock it until
                    // `peer_done` is set! If `parallel_mutex_lock` parks instead of
                    // exiting on `!NILP(error_symbol)`, this deadlocks or fails.
                    let lock_res = eval(ev, "(mutex-lock e54a-r7d-mx)");
                    let mx_val = ev.eval_str("e54a-r7d-mx").unwrap();
                    let mx = ev.threads.mutex_id_from_handle(&mx_val).unwrap();
                    let owned = ev.threads.mutex_owned_by_current_thread(mx);
                    let pending = ev.threads.has_pending_thread_signal(peer_key.id());
                    (lock_res, owned, pending)
                });
            let hint_still_set = owner.signal_hint().check_acquire();
            // Once a handler IS active, the pending signal is delivered.
            let delivered_later =
                with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                    eval(
                        ev,
                        "(condition-case err
                             (progn (thread-yield) 'not-delivered)
                           (error (list 'delivered (car (cdr err)))))",
                    )
                });
            let hint_cleared_after = !owner.signal_hint().check_acquire();
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            peer_done.store(true, Ordering::Release);

            assert_eq!(lock_res, "nil");
            assert!(
                !owned_after,
                "Blocker 3: fresh mutex-lock with pending signal and empty condition_stack must NOT acquire the mutex"
            );
            assert!(
                still_pending,
                "pending thread signal must remain set while condition_stack is empty"
            );
            assert!(
                hint_still_set,
                "SignalHint must stay raised while signal delivery is deferred"
            );
            assert_eq!(
                delivered_later, "(delivered \"r7d-pending\")",
                "deferred signal must deliver once a handler is active"
            );
            assert!(
                hint_cleared_after,
                "SignalHint must clear once the deferred signal is delivered"
            );
        },
        move |main, _control| {
            assert_eq!(
                eval(main, "(progn (mutex-lock e54a-r7d-mx) 'locked)"),
                "locked"
            );
            main_locked.store(true, Ordering::Release);
            let deadline = Instant::now() + Duration::from_secs(2);
            while !peer_done_m.load(Ordering::Acquire) && Instant::now() < deadline {
                let _ = eval(main, "(sleep-for 0.005)");
            }
            let peer_finished_while_held = peer_done_m.load(Ordering::Acquire);
            assert_eq!(
                eval(main, "(progn (mutex-unlock e54a-r7d-mx) 'unlocked)"),
                "unlocked"
            );
            assert!(
                peer_finished_while_held,
                "Blocker 3: peer must finish (mutex-lock) without parking while Main still holds e54a-r7d-mx"
            );
        },
    );
}

// ===========================================================================
// E5.4a R3 Row 12c (Nit 2): `Lifecycle::Closing` wakes `parallel_condition_wait`
// specifically while parked in contended `park_mutex_relock` (Main keeps holding
// `mx` across `condition-notify` + `control.close()`), with a 1.5s watchdog.
// ===========================================================================
#[test]
fn e54a_r3_closing_wakes_condition_wait_during_contended_relock() {
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_main = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let peer_done_m = Arc::clone(&peer_done);
    let watchdog_handle = Arc::new(std::sync::Mutex::new(None::<std::thread::JoinHandle<()>>));
    let watchdog_handle_m = Arc::clone(&watchdog_handle);
    let timed_out = Arc::new(AtomicBool::new(false));
    let timed_out_m = Arc::clone(&timed_out);

    run_in_e54a_era(
        "e54a-r12c-closing-relock",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-r12c-mx (make-mutex \"r12c-mx\"))
                       (setq e54a-r12c-cv (make-condition-variable e54a-r12c-mx \"r12c-cv\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            let grant = port.request_turn().expect("request turn");
            let mut turn = begin_turn(&mut state, home, grant, &mut port, standalone)
                .ok()
                .expect("begin turn");
            let peer_key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e54a-r12c-peer", "r12c-peer"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), peer_key);
            *peer_key_slot.lock().unwrap() = Some(peer_key);

            let (
                eval_res,
                blocker_after,
                wait_cv_after,
                root_depth_before,
                root_depth_after,
                wait_after,
            ) = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                assert_eq!(
                    eval(ev, "(progn (mutex-lock e54a-r12c-mx) 'locked)"),
                    "locked"
                );
                let before = ev.vm_root_frames_len_for_test();
                let res = ev.eval_str("(condition-wait e54a-r12c-cv)");
                let after = ev.vm_root_frames_len_for_test();
                let blocker = eval(ev, "(thread--blocker e54a-r12c-peer)");
                let wait_cv = ev.threads.in_wait_condvar(peer_key.id());
                let wait_state = control.wait_of(peer_key);
                (res, blocker, wait_cv, before, after, wait_state)
            });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            {
                let (lock, cvar) = &*peer_done;
                *lock.lock().unwrap() = true;
                cvar.notify_all();
            }
            if let Some(handle) = watchdog_handle.lock().unwrap().take() {
                handle.join().expect("join relock closing watchdog");
            }

            assert!(
                !timed_out.load(Ordering::Acquire),
                "park_mutex_relock spun past Lifecycle::Closing"
            );
            assert!(
                matches!(eval_res, Err(neovm_value::error::EvalError::Shutdown(_))),
                "expected Flow::shutdown from park_mutex_relock on Lifecycle::Closing, got {eval_res:?}"
            );
            assert_eq!(blocker_after, "nil");
            assert!(!wait_cv_after);
            assert_eq!(root_depth_after, root_depth_before);
            assert_eq!(wait_after, Some(Wait::ParallelActive));
        },
        move |main, control| {
            let start = Instant::now();
            let peer_key = loop {
                if let Some(key) = *peer_key_main.lock().unwrap() {
                    break key;
                }
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            };
            while !matches!(control.wait_of(peer_key), Some(Wait::Condition(_))) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            // Main locks `mx` and notifies `cv` WITHOUT unlocking `mx`, then
            // sleeps briefly so peer wakes from `Wait::Condition`, enters
            // `Reacquiring`, and parks on `Wait::Mutex` (`park_mutex_relock`).
            assert_eq!(
                eval(
                    main,
                    "(progn (mutex-lock e54a-r12c-mx) (condition-notify e54a-r12c-cv) 'notified)"
                ),
                "notified"
            );
            while !matches!(control.wait_of(peer_key), Some(Wait::Mutex(_))) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for peer Wait::Mutex (park_mutex_relock)"
                );
                let _ = eval(main, "(sleep-for 0.005)");
            }
            let wd_done = Arc::clone(&peer_done_m);
            let wd_timeout = Arc::clone(&timed_out_m);
            let handle = std::thread::spawn(move || {
                let (lock, cvar) = &*wd_done;
                let mut done = lock.lock().unwrap();
                let deadline = Instant::now() + Duration::from_millis(1500);
                while !*done {
                    let Some(rem) = deadline.checked_duration_since(Instant::now()) else {
                        wd_timeout.store(true, Ordering::Release);
                        return;
                    };
                    let (guard, _) = cvar.wait_timeout(done, rem).unwrap();
                    done = guard;
                }
            });
            *watchdog_handle_m.lock().unwrap() = Some(handle);
            control.close();
        },
    );
}

// ===========================================================================
// E5.4a R3 Row 12d (Nit 2): `Lifecycle::Closing` wakes parked
// `parallel_mutex_lock` with `Flow::shutdown` (no spin) and cleans up blocker,
// `wait_condvar`, VM root frame, and `Control.state.waits`.
// ===========================================================================
#[test]
fn e54a_r3_closing_wakes_parked_mutex_lock_with_shutdown_and_cleanup() {
    let main_locked = Arc::new(AtomicBool::new(false));
    let main_locked_p = Arc::clone(&main_locked);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_main = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
    let peer_done_m = Arc::clone(&peer_done);
    let watchdog_handle = Arc::new(std::sync::Mutex::new(None::<std::thread::JoinHandle<()>>));
    let watchdog_handle_m = Arc::clone(&watchdog_handle);
    let timed_out = Arc::new(AtomicBool::new(false));
    let timed_out_m = Arc::clone(&timed_out);

    run_in_e54a_era(
        "e54a-r12d-closing-mutex-lock",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn (setq e54a-r12d-mx (make-mutex \"r12d-mx\")) 'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            wait_for_flag(&main_locked_p, "main_locked");
            let grant = port.request_turn().expect("request turn");
            let mut turn = begin_turn(&mut state, home, grant, &mut port, standalone)
                .ok()
                .expect("begin turn");
            let peer_key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e54a-r12d-peer", "r12d-peer"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), peer_key);
            *peer_key_slot.lock().unwrap() = Some(peer_key);

            let (
                eval_res,
                blocker_after,
                wait_cv_after,
                root_depth_before,
                root_depth_after,
                wait_after,
            ) = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                let before = ev.vm_root_frames_len_for_test();
                let res = ev.eval_str("(mutex-lock e54a-r12d-mx)");
                let after = ev.vm_root_frames_len_for_test();
                let blocker = eval(ev, "(thread--blocker e54a-r12d-peer)");
                let wait_cv = ev.threads.in_wait_condvar(peer_key.id());
                let wait_state = control.wait_of(peer_key);
                (res, blocker, wait_cv, before, after, wait_state)
            });
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, port, home);
            {
                let (lock, cvar) = &*peer_done;
                *lock.lock().unwrap() = true;
                cvar.notify_all();
            }
            if let Some(handle) = watchdog_handle.lock().unwrap().take() {
                handle.join().expect("join mutex-lock closing watchdog");
            }

            assert!(
                !timed_out.load(Ordering::Acquire),
                "parallel_mutex_lock spun past Lifecycle::Closing"
            );
            assert!(
                matches!(eval_res, Err(neovm_value::error::EvalError::Shutdown(_))),
                "expected Flow::shutdown from mutex-lock on Lifecycle::Closing, got {eval_res:?}"
            );
            assert_eq!(blocker_after, "nil");
            assert!(!wait_cv_after);
            assert_eq!(root_depth_after, root_depth_before);
            assert_eq!(wait_after, Some(Wait::ParallelActive));
        },
        move |main, control| {
            assert_eq!(
                eval(main, "(progn (mutex-lock e54a-r12d-mx) 'locked)"),
                "locked"
            );
            main_locked.store(true, Ordering::Release);
            let start = Instant::now();
            let peer_key = loop {
                if let Some(key) = *peer_key_main.lock().unwrap() {
                    break key;
                }
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            };
            while !matches!(control.wait_of(peer_key), Some(Wait::Mutex(_))) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            let wd_done = Arc::clone(&peer_done_m);
            let wd_timeout = Arc::clone(&timed_out_m);
            let handle = std::thread::spawn(move || {
                let (lock, cvar) = &*wd_done;
                let mut done = lock.lock().unwrap();
                let deadline = Instant::now() + Duration::from_millis(1500);
                while !*done {
                    let Some(rem) = deadline.checked_duration_since(Instant::now()) else {
                        wd_timeout.store(true, Ordering::Release);
                        return;
                    };
                    let (guard, _) = cvar.wait_timeout(done, rem).unwrap();
                    done = guard;
                }
            });
            *watchdog_handle_m.lock().unwrap() = Some(handle);
            control.close();
        },
    );
}

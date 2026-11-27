//! E5.3a: thread control for parallel owners — Control participants (`D1`),
//! `SignalHint` + signal delivery + newborn deferral + `SerialUnwind` guard +
//! `Native`/`Host` non-interruption (`D2`, rows 1–3), `thread-yield` outside
//! and inside turns + Q6(a) fail-loud guard (`D6`, row 5), mixed & parallel-only
//! deadlock verdict + Baton-only byte-identical verdict (`D8`, row 6), and §7
//! S-track newborn Baton regression row.

use crate::emacs_core::eval::green::{
    ConditionId, Control, HostRequestKey, MutexId, ParallelWaitOutcome, ThreadKey, Wait,
};
use crate::emacs_core::eval::parallel_entry::keeper_driver::{
    ChannelPort, Driver, DriverEnd, GroupPort,
};
use crate::emacs_core::eval::parallel_entry::owned_source::era_fixture::{
    KeeperPanicGuard, activate, close, finish_fixture, keeper_serve, retire,
};
use crate::emacs_core::eval::parallel_entry::serial_park::{KeeperPort, SerialHome, begin_turn};
use crate::emacs_core::eval::parallel_entry::serial_ticket::LocalSerialOwner;
use crate::emacs_core::eval::parallel_entry::serial_turn::OwnerSerialState;
use crate::emacs_core::eval::parallel_entry::thread_control::{
    ParallelOwnerControl, ParkWait, TurnEntryKind, parallel_deadlock_flow, with_world_parkable,
};
use crate::emacs_core::eval::world::TurnRight;
use crate::emacs_core::eval::{ConditionFrame, Context, ResumeTarget, ThreadingMode};
use crate::emacs_core::print::print_value;
use neovm_value::tagged::gc::RendezvousError;
use neovm_value::value::Value;
use std::convert::Infallible;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const BOUND: Duration = Duration::from_secs(30);
const PEER_OWNER_ID: u64 = 9501;
const PEER2_OWNER_ID: u64 = 9502;

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

fn dummy_top_level_handler() -> ConditionFrame {
    ConditionFrame::Catch {
        tag: Value::NIL,
        resume: ResumeTarget::InterpreterCatch,
    }
}

fn format_signal_data(flow: &neovm_value::error::Flow) -> (String, String) {
    let sig = flow
        .as_signal()
        .unwrap_or_else(|| panic!("expected Flow::Signal, got {flow:?}"));
    (
        neovm_value::intern::resolve_sym(sig.symbol).to_string(),
        print_value(&Value::list(sig.data.clone())),
    )
}

/// Run a real Green World inside a `WorldEra` + `KeeperEra` + `Driver` +
/// `GroupPort` with one enrolled parallel owner `PEER_OWNER_ID`.
fn run_in_e53a_era<S, M, P>(label: &'static str, setup: S, peer_body: P, main_body: M)
where
    S: FnOnce(&mut Context),
    M: FnOnce(&mut Context, &Arc<Control>) + Send + 'static,
    P: FnOnce(
            &mut OwnerSerialState<Context, LocalSerialOwner>,
            &mut ChannelPort<'_>,
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
        "E5.3a live era rows require a generational heap"
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
                    .register(PEER_OWNER_ID, 0)
                    .expect("register parallel owner");
                let (grant_tx, grant_rx) = mpsc::sync_channel(1);
                driver.enroll(PEER_OWNER_ID, permit.key(), grant_tx);
                let publishing_owners = driver.publishing_owners_counter();
                let peer_inbox = inbox.clone();
                let peer_control = Arc::clone(&control);
                let (peer_idle_tx, peer_idle_rx) = mpsc::sync_channel::<()>(1);
                let peer_handle = scope.spawn(move || {
                    let mutator = activate(permit, heap, PEER_OWNER_ID);
                    let mut port = ChannelPort::new(
                        PEER_OWNER_ID,
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
                    let peer_panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        peer_body(&mut state, &mut port, home, &peer_control);
                    }))
                    .err();
                    let mutator = port.exit();
                    retire(mutator);
                    drop(state);
                    peer_panic
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

/// Helper to spawn a Lisp thread handle for a parallel owner inside a serial
/// turn and bind it to a Lisp variable NAME.
fn create_parallel_thread_handle(ev: &mut Context, var: &str, name: &str) -> ThreadKey {
    let id = ev.threads.create_thread(
        Value::NIL,
        crate::emacs_core::threads::ThreadName::from_lisp(Value::string(name))
            .expect("valid thread name"),
    );
    ev.threads.enter_thread(id);
    let handle = ev.threads.thread_handle(id).expect("parallel handle");
    ev.obarray.set_symbol_value(var, handle);
    ThreadKey::from_id(id)
}

/// D1 / Q3: `Participant::Parallel` uses `Wait::ParallelActive` and `Wait::Native`,
/// never pollutes Baton `runnable_count`/`eligible_runnable_count`, never enters
/// Baton's `ReadyQueue`, and wakes cleanly via `OwnerWake` across `wake_locked`,
/// `wake_waiting_for`, and `wake_one_waiting_for`.
#[test]
fn e53a_d1_parallel_participant_never_pollutes_baton_runnable_count_or_ready_queue() {
    let control = Arc::new(Control::default());
    let _main_rx = control.register_main_guard_for_test();
    assert_eq!(control.eligible_runnable_count_for_test(), 1);

    let p1 = ThreadKey::Worker(NonZeroU64::new(77).unwrap());
    let p2 = ThreadKey::Worker(NonZeroU64::new(78).unwrap());
    let owner1 = ParallelOwnerControl::register(Arc::clone(&control), p1);
    let owner2 = ParallelOwnerControl::register(Arc::clone(&control), p2);

    assert_eq!(control.wait_of(p1), Some(Wait::ParallelActive));
    assert_eq!(control.wait_of(p2), Some(Wait::ParallelActive));
    assert_eq!(
        control.eligible_runnable_count_for_test(),
        1,
        "Wait::ParallelActive must never inflate Baton eligible_runnable_count"
    );

    // Park both parallel owners on a mutex, verify wake_waiting_for wakes both
    // via OwnerWake and leaves eligible_runnable_count unchanged.
    let mx = MutexId::new(1001);
    control.begin_parallel_wait(p1, Wait::Mutex(mx));
    control.begin_parallel_wait(p2, Wait::Mutex(mx));
    assert_eq!(control.wake_waiting_for(Wait::Mutex(mx)), 2);
    assert!(owner1.wake().try_take());
    assert!(owner2.wake().try_take());
    assert_eq!(control.wait_of(p1), Some(Wait::ParallelActive));
    assert_eq!(control.wait_of(p2), Some(Wait::ParallelActive));
    assert_eq!(control.eligible_runnable_count_for_test(), 1);

    // Park both on a condition variable; wake_one_waiting_for wakes exactly one.
    let cv = ConditionId::new(2002);
    control.begin_parallel_wait(p1, Wait::Condition(cv));
    control.begin_parallel_wait(p2, Wait::Condition(cv));
    assert!(control.wake_one_waiting_for(Wait::Condition(cv)));
    assert!(owner1.wake().try_take());
    assert!(!owner2.wake().is_pending());
    assert!(control.wake_one_waiting_for(Wait::Condition(cv)));
    assert!(owner2.wake().try_take());
    assert_eq!(control.eligible_runnable_count_for_test(), 1);

    // Native wait is never woken by Control::wake (Row 3).
    control.begin_parallel_wait(p1, Wait::Native);
    control.wake(p1);
    assert!(!owner1.wake().is_pending());
    assert_eq!(control.wait_of(p1), Some(Wait::Native));
    control.end_parallel_wait(p1);
    assert_eq!(control.wait_of(p1), Some(Wait::ParallelActive));

    owner1.unregister();
    owner2.unregister();
    assert!(control.exited(p1));
    assert!(control.exited(p2));
    assert_eq!(control.eligible_runnable_count_for_test(), 1);
}

/// Q6(a): calling a blocking thread/mutex/condvar builtin from Lisp inside an
/// unbridged `LiveSerial::with_world` must FAIL LOUDLY instead of silently
/// falling through to the Sim backend.
#[test]
fn e53a_q6a_unbridged_blocking_builtins_fail_loudly_in_serial_turn() {
    let go = Arc::new(AtomicBool::new(false));
    let go_peer = Arc::clone(&go);
    let done = Arc::new(AtomicBool::new(false));
    let done_peer = Arc::clone(&done);

    run_in_e53a_era(
        "e53a-q6a-fail-loud",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e53a-mx (make-mutex \"e53a-mx\"))
                       (setq e53a-cv (make-condition-variable e53a-mx \"e53a-cv\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |state, port, home, _control| {
            let start = Instant::now();
            while !go_peer.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }
            let (grant, lend) = port.request_turn().ok().expect("peer granted turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("peer begins turn");
            let (yield_out, lock_out, cond_out, join_out, sleep_out, select_out, sim_out) = turn
                .with_world(|ev| {
                    // Enter a non-zero parallel thread id so `current_thread_id() != 0`
                    // matches production parallel-owner turns (where unguarded
                    // `sleep-for` / select waits would otherwise silently return
                    // Sim's `Flow::thread_blocked`).
                    let q6a_key =
                        create_parallel_thread_handle(ev, "e53a-q6a-peer", "e53a-q6a-peer");
                    let prev_id = ev.threads.current_thread_id();
                    ev.threads.enter_thread(q6a_key.id());
                    let y = eval(ev, "(thread-yield)");
                    let m = eval(ev, "(mutex-lock e53a-mx)");
                    let c = eval(ev, "(condition-wait e53a-cv)");
                    let j = eval(ev, "(thread-join e53a-worker)");
                    let s = eval(ev, "(sleep-for 0.05)");
                    let sel = match ev.green_select_switch(Duration::from_millis(50)) {
                        Ok(_) => "UNEXPECTED_OK".to_string(),
                        Err(flow) => format_signal_data(&flow).1,
                    };
                    let sim = match ev
                        .sim_release_point(crate::emacs_core::threads::SimReleasePoint::Select)
                    {
                        Ok(ran) => format!("UNEXPECTED_OK {ran}"),
                        Err(flow) => format_signal_data(&flow).1,
                    };
                    ev.threads.enter_thread(prev_id);
                    let _ = ev.threads.finish_thread(q6a_key.id(), Value::NIL);
                    (y, m, c, j, s, sel, sim)
                });
            turn.end(port).ok().expect("peer ends turn");
            done_peer.store(true, Ordering::Release);

            assert!(
                yield_out.contains("thread-yield called in an unbridged parallel serial turn"),
                "unbridged (thread-yield) in serial turn must fail loudly, got: {yield_out}"
            );
            assert!(
                lock_out.contains("mutex-lock called in an unbridged parallel serial turn"),
                "unbridged (mutex-lock) in serial turn must fail loudly, got: {lock_out}"
            );
            assert!(
                cond_out.contains("condition-wait called in an unbridged parallel serial turn"),
                "unbridged (condition-wait) in serial turn must fail loudly, got: {cond_out}"
            );
            assert!(
                join_out.contains("thread-join called in an unbridged parallel serial turn"),
                "unbridged (thread-join) in serial turn must fail loudly, got: {join_out}"
            );
            assert!(
                sleep_out.contains("sleep-for cannot fall through"),
                "Blocker 3: (sleep-for) in serial turn must fail loudly, got: {sleep_out}"
            );
            assert!(
                select_out.contains("select wait cannot fall through"),
                "Blocker 3: green_select_switch in serial turn must fail loudly, got: {select_out}"
            );
            assert!(
                sim_out.contains("sim_release_point cannot run"),
                "Blocker 3: sim_release_point in serial turn must fail loudly, got: {sim_out}"
            );
        },
        move |main, _control| {
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e53a-mx)
                       (setq e53a-worker
                             (make-thread (lambda () (mutex-lock e53a-mx) (mutex-unlock e53a-mx))))
                       (thread-yield)
                       'ready)"
                ),
                "ready"
            );
            go.store(true, Ordering::Release);
            let start = Instant::now();
            while !done.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.01)");
            }
            assert_eq!(
                eval(
                    main,
                    "(progn (mutex-unlock e53a-mx) (thread-join e53a-worker) 'ok)"
                ),
                "ok"
            );
        },
    );
}

/// Row 1 (single-violation): owner polls `SignalHint::peek_relaxed()` in an
/// owner-local loop (zero turns), observes `thread-signal` from Baton, enters a
/// serial turn with a handler installed, and delivers the exact signal while
/// clearing `SignalHint`.
#[test]
fn e53a_row1_signal_during_owner_local_loop_delivers_on_normal_entry() {
    let registered = Arc::new(AtomicBool::new(false));
    let registered_m = Arc::clone(&registered);
    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);

    run_in_e53a_era(
        "e53a-row1-loop-signal",
        |_| {},
        move |state, port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("setup turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin setup turn");
            let key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e53a-r1-peer", "e53a-r1-peer"));
            turn.end(port).ok().expect("end setup turn");
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            assert!(!owner.poll_signal_hint());
            registered.store(true, Ordering::Release);

            // Owner-local poll loop: zero serial turns until SignalHint flips.
            let start = Instant::now();
            while !owner.poll_signal_hint() {
                assert!(
                    start.elapsed() < BOUND,
                    "parallel owner must observe SignalHint in owner-local poll loop"
                );
                std::thread::yield_now();
            }

            let (grant, lend) = port.request_turn().ok().expect("delivery turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin delivery turn");
            turn.with_world(|ev| {
                ev.threads.enter_thread(key.id());
                ev.push_condition_frame(dummy_top_level_handler());
                let delivered = owner.deliver_signal_at_entry(ev, TurnEntryKind::Normal);
                let err = delivered.expect_err("normal delivery must raise pending signal");
                let (sym, data) = format_signal_data(&err);
                assert_eq!(sym, "error");
                assert_eq!(data, "(\"loop-signal\")");
                assert!(!owner.poll_signal_hint());
                assert!(!ev.threads.has_pending_thread_signal(key.id()));
                let _ = ev.pop_condition_frame();
                let _ = ev.threads.finish_thread(key.id(), Value::NIL);
            });
            turn.end(port).ok().expect("end delivery turn");
            owner.unregister();
            done.store(true, Ordering::Release);
        },
        move |main, _control| {
            let start = Instant::now();
            while !registered_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(
                eval(
                    main,
                    "(thread-signal e53a-r1-peer 'error '(\"loop-signal\"))"
                ),
                "nil"
            );
            let start = Instant::now();
            while !done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// D2 / Q6c (single-violation, GNU `thread.c:146-154`): when a parallel owner's
/// `condition_stack` is empty (newborn before top-level handler installation),
/// `deliver_signal_at_entry` defers delivery and leaves both `ThreadManager`'s
/// pending error and `SignalHint` intact.
#[test]
fn e53a_row1_newborn_empty_condition_stack_defers_signal_delivery() {
    let registered = Arc::new(AtomicBool::new(false));
    let registered_m = Arc::clone(&registered);
    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);

    run_in_e53a_era(
        "e53a-row1-newborn",
        |_| {},
        move |state, port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("setup turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin setup turn");
            let key = turn.with_world(|ev| {
                create_parallel_thread_handle(ev, "e53a-r1-nb-peer", "e53a-r1-nb-peer")
            });
            turn.end(port).ok().expect("end setup turn");
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            registered.store(true, Ordering::Release);

            let start = Instant::now();
            while !owner.poll_signal_hint() {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }

            let (grant, lend) = port.request_turn().ok().expect("check turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin check turn");
            turn.with_world(|ev| {
                ev.threads.enter_thread(key.id());
                assert_eq!(ev.condition_stack_len(), 0);
                assert!(
                    owner
                        .deliver_signal_at_entry(ev, TurnEntryKind::Normal)
                        .is_ok(),
                    "GNU thread.c:146: empty condition_stack must defer signal delivery"
                );
                assert!(ev.threads.has_pending_thread_signal(key.id()));
                assert!(owner.poll_signal_hint());
                let _ = ev.threads.finish_thread(key.id(), Value::NIL);
            });
            turn.end(port).ok().expect("end check turn");
            owner.unregister();
            done.store(true, Ordering::Release);
        },
        move |main, _control| {
            let start = Instant::now();
            while !registered_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(
                eval(
                    main,
                    "(thread-signal e53a-r1-nb-peer 'error '(\"newborn-sig\"))"
                ),
                "nil"
            );
            let start = Instant::now();
            while !done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// V8 `SerialUnwind` non-delivery guard (single-violation, `E5.1-TYPES.md §6`):
/// `deliver_signal_at_entry(..., TurnEntryKind::SerialUnwind)` MUST NOT deliver
/// a pending signal even when `condition_stack` is non-empty.
#[test]
fn e53a_row1_serial_unwind_turn_entry_does_not_deliver_pending_signal() {
    let registered = Arc::new(AtomicBool::new(false));
    let registered_m = Arc::clone(&registered);
    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);

    run_in_e53a_era(
        "e53a-row1-unwind",
        |_| {},
        move |state, port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("setup turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin setup turn");
            let key = turn.with_world(|ev| {
                create_parallel_thread_handle(ev, "e53a-r1-uw-peer", "e53a-r1-uw-peer")
            });
            turn.end(port).ok().expect("end setup turn");
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            registered.store(true, Ordering::Release);

            let start = Instant::now();
            while !owner.poll_signal_hint() {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }

            let (grant, lend) = port.request_turn().ok().expect("check turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin check turn");
            turn.with_world(|ev| {
                ev.threads.enter_thread(key.id());
                ev.push_condition_frame(dummy_top_level_handler());
                assert!(
                    owner
                        .deliver_signal_at_entry(ev, TurnEntryKind::SerialUnwind)
                        .is_ok(),
                    "E5.1-TYPES §6: SerialUnwind turn entry must not deliver pending signal"
                );
                assert!(ev.threads.has_pending_thread_signal(key.id()));
                assert!(owner.poll_signal_hint());
                let _ = ev.pop_condition_frame();
                let _ = ev.threads.finish_thread(key.id(), Value::NIL);
            });
            turn.end(port).ok().expect("end check turn");
            owner.unregister();
            done.store(true, Ordering::Release);
        },
        move |main, _control| {
            let start = Instant::now();
            while !registered_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(
                eval(
                    main,
                    "(thread-signal e53a-r1-uw-peer 'error '(\"unwind-sig\"))"
                ),
                "nil"
            );
            let start = Instant::now();
            while !done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// Row 2 (part A): parallel sender calling `thread-signal` on a target that is
/// `in_wait_condvar` runs GNU `post_acquire_global_lock` (`thread.c:981,
/// 1029-1030`) and delivers its own pending signal when `condition_stack` is
/// non-empty.
#[test]
fn e53a_row2_parallel_sender_in_wait_condvar_delivers_own_pending_signal() {
    let go = Arc::new(AtomicBool::new(false));
    let go_peer = Arc::clone(&go);
    let done = Arc::new(AtomicBool::new(false));
    let done_peer = Arc::clone(&done);

    run_in_e53a_era(
        "e53a-row2-sender-post-acquire",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e53a-s2-mx (make-mutex \"e53a-s2-mx\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |state, port, home, _control| {
            let start = Instant::now();
            while !go_peer.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }
            let (grant, lend) = port.request_turn().ok().expect("peer granted turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("peer begins turn");
            let out = turn.with_world(|ev| {
                let sender_id = ev.threads.create_thread(
                    Value::NIL,
                    crate::emacs_core::threads::ThreadName::from_lisp(Value::string(
                        "e53a-peer-sender",
                    ))
                    .expect("valid thread name"),
                );
                ev.threads.enter_thread(sender_id);
                let sender_val = ev.threads.thread_handle(sender_id).expect("sender handle");
                ev.threads.set_thread_error_for_value(
                    sender_val,
                    Value::symbol("error"),
                    Value::list(vec![Value::string("sender-own-signal")]),
                );
                let res = eval(
                    ev,
                    "(condition-case err
                         (progn
                           (thread-signal e53a-s2-target 'error '(\"wake-target\"))
                           'not-delivered)
                       (error (list 'caught (car (cdr err)))))",
                );
                let _ = ev.threads.finish_thread(sender_id, Value::NIL);
                res
            });
            turn.end(port).ok().expect("peer ends turn");
            done_peer.store(true, Ordering::Release);

            assert_eq!(
                out, "(caught \"sender-own-signal\")",
                "GNU thread.c:981: parallel sender must run post_acquire_global_lock after waking wait_condvar target"
            );
        },
        move |main, control| {
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e53a-s2-mx)
                       (setq e53a-s2-target
                             (make-thread
                               (lambda ()
                                 (condition-case err
                                     (progn (mutex-lock e53a-s2-mx) 'locked)
                                   (error (list 'target-caught (car (cdr err))))))))
                       (thread-yield)
                       'ready)"
                ),
                "ready"
            );
            let target_id = {
                let target_val = main.eval_str("e53a-s2-target").unwrap();
                main.threads.thread_id_from_handle(&target_val).unwrap()
            };
            let target_key = ThreadKey::from_id(target_id);
            let start = Instant::now();
            while !matches!(control.wait_of(target_key), Some(Wait::Mutex(_))) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(thread-yield)");
            }
            go.store(true, Ordering::Release);
            let start = Instant::now();
            while !done.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.01)");
            }
            assert_eq!(
                eval(main, "(progn (mutex-unlock e53a-s2-mx) 'unlocked)"),
                "unlocked"
            );
            // Wait (bounded) until target actually runs, consumes `wake-target`
            // inside its `condition-case`, and exits; calling `thread-join`
            // before target consumes its signal would snapshot and re-raise
            // `wake-target` per GNU `Fthread_join` (`src/thread.c:1134-1141`).
            let start = Instant::now();
            while !control.exited(target_key) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for e53a-s2-target to consume wake-target and exit"
                );
                let _ = eval(main, "(thread-yield)");
            }
            let joined = eval(main, "(thread-join e53a-s2-target)");
            assert_eq!(
                joined, "(target-caught \"wake-target\")",
                "target must have caught wake-target signal, got: {joined}"
            );
        },
    );
}

/// Row 2 (part B): `thread-signal` wakes a parallel owner parked in
/// `ParkWait::Mutex`, `ParkWait::Condition`, and `ParkWait::Join` via
/// `with_world_parkable`, and delivers the signal on resume.
#[test]
fn e53a_row2_signal_wakes_parallel_owner_in_mutex_condition_and_join_waits() {
    let phase = Arc::new(AtomicUsize::new(0));
    let phase_m = Arc::clone(&phase);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);

    run_in_e53a_era(
        "e53a-row2-park-signal",
        |_| {},
        move |state, port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin");
            let key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e53a-r2-peer", "e53a-r2-peer"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);

            let waits = [
                (ParkWait::Mutex(MutexId::new(501)), "sig-mutex"),
                (ParkWait::Condition(ConditionId::new(502)), "sig-cond"),
                (
                    ParkWait::Join(ThreadKey::Worker(NonZeroU64::new(503).unwrap())),
                    "sig-join",
                ),
            ];
            let results = with_world_parkable(&mut turn, &mut owner, port, standalone, |ev| {
                ev.threads.enter_thread(key.id());
                ev.push_condition_frame(dummy_top_level_handler());
                let mut seen = Vec::new();
                for (idx, (wait, label)) in waits.iter().enumerate() {
                    phase.store(idx + 1, Ordering::Release);
                    let (_k, outcome) = ev
                        .parallel_park_in_turn(label, *wait, None)
                        .expect("parked and resumed");
                    assert!(
                        matches!(outcome, ParallelWaitOutcome::Woken),
                        "{label}: expected Woken, got {outcome:?}"
                    );
                    let err = ev
                        .deliver_parallel_turn_signal(key)
                        .expect_err("expected signal delivery");
                    let (_sym, data) = format_signal_data(&err);
                    seen.push(data);
                }
                let _ = ev.pop_condition_frame();
                let _ = ev.threads.finish_thread(key.id(), Value::NIL);
                seen
            });
            turn.end(port).ok().expect("end");
            owner.unregister();
            phase.store(99, Ordering::Release);
            assert_eq!(
                results,
                vec!["(\"sig-mutex\")", "(\"sig-cond\")", "(\"sig-join\")"]
            );
        },
        move |main, control| {
            let expected = [
                (1usize, "sig-mutex"),
                (2usize, "sig-cond"),
                (3usize, "sig-join"),
            ];
            for (step, msg) in expected {
                let start = Instant::now();
                loop {
                    if phase_m.load(Ordering::Acquire) == step
                        && let Some(key) = *peer_key_m.lock().unwrap()
                        && matches!(
                            control.wait_of(key),
                            Some(Wait::Mutex(_) | Wait::Condition(_) | Wait::Join(_))
                        )
                    {
                        break;
                    }
                    assert!(
                        start.elapsed() < BOUND,
                        "timed out waiting for peer wait {msg}"
                    );
                    let _ = eval(main, "(sleep-for 0.005)");
                }
                let form = format!("(thread-signal e53a-r2-peer 'error '(\"{msg}\"))");
                assert_eq!(eval(main, &form), "nil");
            }
            let start = Instant::now();
            while phase_m.load(Ordering::Acquire) != 99 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// Row 3: `thread-signal` does NOT interrupt `ParkWait::Native` (`Wait::Native`)
/// or `ParkWait::Host` (`Wait::HostRequest`); the wait runs to its own
/// completion and the signal is delivered upon re-entering the serial turn.
#[test]
fn e53a_row3_native_and_host_waits_not_interrupted_by_signal_delivered_on_completion() {
    let (native_rel_tx, native_rel_rx) = mpsc::sync_channel::<()>(1);
    let (host_rel_tx, host_rel_rx) = mpsc::sync_channel::<()>(1);
    let native_finished = Arc::new(AtomicBool::new(false));
    let native_finished_m = Arc::clone(&native_finished);
    let host_finished = Arc::new(AtomicBool::new(false));
    let host_finished_m = Arc::clone(&host_finished);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);
    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);

    run_in_e53a_era(
        "e53a-row3-native-host",
        |_| {},
        move |state, port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin");
            let key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e53a-r3-peer", "e53a-r3-peer"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);

            let (sig_native, sig_host) =
                with_world_parkable(&mut turn, &mut owner, port, standalone, |ev| {
                    ev.threads.enter_thread(key.id());
                    ev.push_condition_frame(dummy_top_level_handler());

                    // 1. ParkWait::Native — blocks on native_rel_rx, ignores thread-signal.
                    let nf = Arc::clone(&native_finished);
                    let (_k, outcome) = ev
                        .parallel_park_in_turn(
                            "native-wait",
                            ParkWait::Native,
                            Some(Box::new(move || {
                                native_rel_rx
                                    .recv_timeout(BOUND)
                                    .expect("native wait released");
                                nf.store(true, Ordering::Release);
                            })),
                        )
                        .expect("native park completed");
                    assert!(matches!(outcome, ParallelWaitOutcome::Woken));
                    let err1 = ev
                        .deliver_parallel_turn_signal(key)
                        .expect_err("expected deferred native signal");
                    let (_sym1, s1) = format_signal_data(&err1);

                    // 2. ParkWait::Host — blocks on host_rel_rx, ignores thread-signal.
                    let hf = Arc::clone(&host_finished);
                    let hk = HostRequestKey::for_test(NonZeroU64::new(42).unwrap());
                    let (_k, outcome) = ev
                        .parallel_park_in_turn(
                            "host-wait",
                            ParkWait::Host(hk),
                            Some(Box::new(move || {
                                host_rel_rx.recv_timeout(BOUND).expect("host wait released");
                                hf.store(true, Ordering::Release);
                            })),
                        )
                        .expect("host park completed");
                    assert!(matches!(outcome, ParallelWaitOutcome::Woken));
                    let err2 = ev
                        .deliver_parallel_turn_signal(key)
                        .expect_err("expected deferred host signal");
                    let (_sym2, s2) = format_signal_data(&err2);

                    let _ = ev.pop_condition_frame();
                    let _ = ev.threads.finish_thread(key.id(), Value::NIL);
                    (s1, s2)
                });
            turn.end(port).ok().expect("end");
            owner.unregister();
            done.store(true, Ordering::Release);
            assert_eq!(sig_native, "(\"after-native\")");
            assert_eq!(sig_host, "(\"after-host\")");
        },
        move |main, control| {
            // Wait for Wait::Native, send signal, confirm peer does NOT exit
            // native wait before we release it, then release.
            let start = Instant::now();
            loop {
                if let Some(key) = *peer_key_m.lock().unwrap()
                    && control.wait_of(key) == Some(Wait::Native)
                {
                    break;
                }
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(
                eval(
                    main,
                    "(thread-signal e53a-r3-peer 'error '(\"after-native\"))"
                ),
                "nil"
            );
            let _ = eval(main, "(sleep-for 0.02)");
            assert!(
                !native_finished_m.load(Ordering::Acquire),
                "Row 3: thread-signal must NOT end ParkWait::Native"
            );
            native_rel_tx.send(()).expect("release native wait");

            // Wait for Wait::HostRequest, send signal, confirm not interrupted,
            // then release.
            let start = Instant::now();
            loop {
                if let Some(key) = *peer_key_m.lock().unwrap()
                    && matches!(control.wait_of(key), Some(Wait::HostRequest(_)))
                {
                    break;
                }
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(
                eval(
                    main,
                    "(thread-signal e53a-r3-peer 'error '(\"after-host\"))"
                ),
                "nil"
            );
            let _ = eval(main, "(sleep-for 0.02)");
            assert!(
                !host_finished_m.load(Ordering::Acquire),
                "Row 3: thread-signal must NOT end ParkWait::Host"
            );
            host_rel_tx.send(()).expect("release host wait");

            let start = Instant::now();
            while !done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// Row 5 (D6):
/// - Outside a serial turn (`yield_outside_turn`): yields OS thread without
///   taking a serial ticket when `SignalHint` is false; when `SignalHint` is
///   true, triggers the signal-delivery callback.
/// - Inside a serial turn (`with_world_parkable` + Lisp `(thread-yield)`):
///   releases the ticket so Main runs a Baton interval (setting a variable and
///   signaling the parallel owner), then reacquires at the ring tail and
///   delivers the signal.
#[test]
fn e53a_row5_thread_yield_outside_turn_takes_no_ticket_and_inside_turn_releases_ticket() {
    let yielded_once = Arc::new(AtomicBool::new(false));
    let yielded_once_m = Arc::clone(&yielded_once);
    let main_interleaved = Arc::new(AtomicBool::new(false));
    let main_interleaved_p = Arc::clone(&main_interleaved);
    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);

    run_in_e53a_era(
        "e53a-row5-yield",
        |src| {
            assert_eq!(eval(src, "(setq e53a-r5-seen nil)"), "nil");
        },
        move |state, port, home, control| {
            // Setup thread handle + ParallelOwnerControl.
            let (grant, lend) = port.request_turn().ok().expect("setup turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin");
            let key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e53a-r5-peer", "e53a-r5-peer"));
            turn.end(port).ok().expect("end");
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);

            // (a) Outside serial turn: no signal pending -> takes ZERO tickets!
            let signal_cb_called = AtomicBool::new(false);
            let outside_res = owner
                .yield_outside_turn(|| {
                    signal_cb_called.store(true, Ordering::Release);
                    Ok(())
                })
                .expect("outside yield ok");
            assert_eq!(outside_res, None);
            assert!(
                !signal_cb_called.load(Ordering::Acquire),
                "D6: outside-turn thread-yield with no signal must take zero serial turns"
            );

            // (b) Inside serial turn via `with_world_parkable`: `(thread-yield)`
            // releases the ticket so Main interleaves, sets `e53a-r5-seen` and
            // signals us; we reacquire at the ring tail and catch the signal.
            let (grant, lend) = port.request_turn().ok().expect("yield turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin yield turn");
            let out = with_world_parkable(&mut turn, &mut owner, port, standalone, |ev| {
                ev.threads.enter_thread(key.id());
                yielded_once.store(true, Ordering::Release);
                // Yield in a loop until Main has interleaved and signalled us.
                let res = eval(
                    ev,
                    "(condition-case err
                         (let ((i 0))
                           (while (< i 500)
                             (thread-yield)
                             (setq i (1+ i)))
                           (list 'no-signal e53a-r5-seen))
                       (error (list 'yield-caught (car (cdr err)) e53a-r5-seen)))",
                );
                let _ = ev.threads.finish_thread(key.id(), Value::NIL);
                res
            });
            turn.end(port).ok().expect("end yield turn");
            owner.unregister();
            done.store(true, Ordering::Release);
            assert!(
                main_interleaved_p.load(Ordering::Acquire),
                "Main must have interleaved while parallel owner yielded"
            );
            assert_eq!(
                out, "(yield-caught \"yield-signal\" from-main)",
                "inside-turn (thread-yield) must release ticket, let Main run, and deliver pending signal"
            );
        },
        move |main, _control| {
            let start = Instant::now();
            while !yielded_once_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (setq e53a-r5-seen 'from-main)
                       (thread-signal e53a-r5-peer 'error '(\"yield-signal\"))
                       'signaled)"
                ),
                "signaled"
            );
            main_interleaved.store(true, Ordering::Release);
            let start = Instant::now();
            while !done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// Row 6 (part A — D8 mixed deadlock): one Baton thread + one registered
/// parallel owner BOTH parked in `Wait::Mutex` -> D8 fires `"All threads are
/// blocked: ..."`!
#[test]
fn e53a_row6_mixed_deadlock_baton_and_parallel_both_parked_signals_deadlock() {
    let main_locked_mx1 = Arc::new(AtomicBool::new(false));
    let main_locked_mx1_p = Arc::clone(&main_locked_mx1);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);
    let main_deadlocked = Arc::new(AtomicBool::new(false));
    let main_deadlocked_p = Arc::clone(&main_deadlocked);

    run_in_e53a_era(
        "e53a-row6-mixed-dl",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e53a-dl-mx1 (make-mutex \"e53a-dl-mx1\"))
                       (setq e53a-dl-mx2 (make-mutex \"e53a-dl-mx2\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |state, port, home, control| {
            let start = Instant::now();
            while !main_locked_mx1_p.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }
            let (grant, lend) = port.request_turn().ok().expect("peer turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin");
            let key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e53a-dl-peer", "e53a-dl-peer"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);

            // Hold mx2 and park on Wait::Mutex(mx1) while Main holds mx1 and
            // blocks on Wait::Mutex(mx2) -> mixed D8 deadlock!
            let peer_outcome = with_world_parkable(&mut turn, &mut owner, port, standalone, |ev| {
                ev.threads.enter_thread(key.id());
                let mx1_id = {
                    let v = ev.eval_str("e53a-dl-mx1").unwrap();
                    ev.threads.mutex_id_from_handle(&v).unwrap()
                };
                let mx2_id = {
                    let v = ev.eval_str("e53a-dl-mx2").unwrap();
                    ev.threads.mutex_id_from_handle(&v).unwrap()
                };
                let _ = ev.threads.mutex_lock(mx2_id);
                let (_k, outcome) = ev
                    .parallel_park_in_turn(
                        "mutex-lock",
                        ParkWait::Mutex(MutexId::new(mx1_id.native_key())),
                        None,
                    )
                    .expect("parked");
                let _ = ev.threads.mutex_unlock(mx2_id);
                control.wake_waiting_for(Wait::Mutex(MutexId::new(mx2_id.native_key())));
                let _ = ev.threads.finish_thread(key.id(), Value::NIL);
                outcome
            });
            turn.end(port).ok().expect("end");
            owner.unregister();
            peer_done.store(true, Ordering::Release);

            // Either the parallel owner OR Main must have detected the D8 deadlock!
            let peer_detected = matches!(peer_outcome, ParallelWaitOutcome::Deadlock { .. });
            if peer_detected {
                let flow = match peer_outcome {
                    ParallelWaitOutcome::Deadlock { invariant } => {
                        parallel_deadlock_flow(invariant)
                    }
                    _ => unreachable!(),
                };
                let (_sym, data) = format_signal_data(&flow);
                assert!(
                    data.contains("All threads are blocked"),
                    "D8 parallel deadlock must match Baton message, got: {data}"
                );
            } else {
                assert!(
                    main_deadlocked_p.load(Ordering::Acquire),
                    "mixed deadlock must be detected by either Baton or Parallel participant"
                );
            }
        },
        move |main, control| {
            assert_eq!(eval(main, "(progn (mutex-lock e53a-dl-mx1) 'ok)"), "ok");
            main_locked_mx1.store(true, Ordering::Release);
            // Wait until peer has locked mx2 and entered Wait::Mutex(mx1) so
            // both participants genuinely block on each other's mutex.
            let start = Instant::now();
            loop {
                if let Some(key) = *peer_key_m.lock().unwrap()
                    && matches!(control.wait_of(key), Some(Wait::Mutex(_)))
                {
                    break;
                }
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            let mx1_id = {
                let v = main.eval_str("e53a-dl-mx1").unwrap();
                main.threads.mutex_id_from_handle(&v).unwrap()
            };
            let out = eval(
                main,
                "(condition-case err
                     (progn (mutex-lock e53a-dl-mx2) (mutex-unlock e53a-dl-mx2) 'acquired)
                   (error (list 'dl (car (cdr err)))))",
            );
            if out.contains("All threads are blocked") {
                main_deadlocked.store(true, Ordering::Release);
            }
            // Unlock mx1 so if Main detected deadlock, peer wakes and exits cleanly.
            assert_eq!(eval(main, "(progn (mutex-unlock e53a-dl-mx1) 'ok)"), "ok");
            control.wake_waiting_for(Wait::Mutex(MutexId::new(mx1_id.native_key())));
            let start = Instant::now();
            while !peer_done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// Row 6 (part B — D8 Q2b parallel-only deadlock): two parallel owners
/// deadlocked with each other while NO Baton thread is registered/blocking ->
/// `wait_parallel_parked` detects deadlock via `IdleQuery` before/after check
/// and returns `ParallelWaitOutcome::Deadlock`!
#[test]
fn e53a_row6_parallel_only_deadlock_detected_by_parallel_owner_park_path() {
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
    let private = Value::string("e53a-row6-p-only");
    source.lexenv = private;
    source.gc_collect_exact();
    source.tagged_heap.close_alloc_regions();
    let bootstrap = source.capture_green_bootstrap_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();

    let (tx, rx) = mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |keeper| {
            let _panic_guard = KeeperPanicGuard { keeper };
            keeper
                .keeper_baton_for_test()
                .install_stowed_green_runtime_for_test(bootstrap);
            let world_handle = keeper.keeper_baton_for_test().test_world_handle_for_test();
            let control = world_handle.green_control_for_test();
            let heap = keeper.heap_identity();

            let (inbox, inbox_rx) = mpsc::channel();
            let mut driver = Driver::new(keeper, inbox_rx);

            let p1_permit = keeper.register(PEER_OWNER_ID, 0).expect("reg p1");
            let p2_permit = keeper.register(PEER2_OWNER_ID, 0).expect("reg p2");
            let (g1_tx, g1_rx) = mpsc::sync_channel(1);
            let (g2_tx, g2_rx) = mpsc::sync_channel(1);
            driver.enroll(PEER_OWNER_ID, p1_permit.key(), g1_tx);
            driver.enroll(PEER2_OWNER_ID, p2_permit.key(), g2_tx);
            let pub1 = driver.publishing_owners_counter();
            let pub2 = driver.publishing_owners_counter();

            let k1 = ThreadKey::Worker(NonZeroU64::new(91).unwrap());
            let k2 = ThreadKey::Worker(NonZeroU64::new(92).unwrap());
            let mx1 = MutexId::new(901);
            let mx2 = MutexId::new(902);
            let detected = Arc::new(AtomicUsize::new(0));

            let (end, p1_ok, p2_ok) = std::thread::scope(|scope| {
                let c1 = Arc::clone(&control);
                let d1 = Arc::clone(&detected);
                let inbox1 = inbox.clone();
                let h1 = scope.spawn(move || {
                    let mutator = activate(p1_permit, heap, PEER_OWNER_ID);
                    let port = ChannelPort::new(PEER_OWNER_ID, mutator, inbox1, g1_rx, pub1);
                    let owner = ParallelOwnerControl::register(Arc::clone(&c1), k1);
                    c1.begin_parallel_wait(k1, Wait::Mutex(mx1));
                    let outcome = c1.wait_parallel_parked(k1, owner.wake(), &port);
                    if matches!(outcome, ParallelWaitOutcome::Deadlock { invariant: Ok(()) }) {
                        d1.fetch_add(1, Ordering::AcqRel);
                        // Wake peer 2 so it can exit cleanly.
                        c1.wake_waiting_for(Wait::Mutex(mx2));
                    }
                    owner.unregister();
                    let mutator = port.exit();
                    retire(mutator);
                    true
                });

                let c2 = Arc::clone(&control);
                let d2 = Arc::clone(&detected);
                let inbox2 = inbox;
                let h2 = scope.spawn(move || {
                    let mutator = activate(p2_permit, heap, PEER2_OWNER_ID);
                    let port = ChannelPort::new(PEER2_OWNER_ID, mutator, inbox2, g2_rx, pub2);
                    let owner = ParallelOwnerControl::register(Arc::clone(&c2), k2);
                    c2.begin_parallel_wait(k2, Wait::Mutex(mx2));
                    let outcome = c2.wait_parallel_parked(k2, owner.wake(), &port);
                    if matches!(outcome, ParallelWaitOutcome::Deadlock { invariant: Ok(()) }) {
                        d2.fetch_add(1, Ordering::AcqRel);
                        c2.wake_waiting_for(Wait::Mutex(mx1));
                    }
                    owner.unregister();
                    let mutator = port.exit();
                    retire(mutator);
                    true
                });

                let end = driver.run();
                (end, h1.join().unwrap(), h2.join().unwrap())
            });

            assert_eq!(end, DriverEnd::Drained);
            assert!(p1_ok && p2_ok);
            assert_eq!(
                detected.load(Ordering::Acquire),
                1,
                "exactly one parallel owner must confirm and report parallel-only D8 deadlock"
            );

            drop(world_handle);
            keeper
                .keeper_baton_for_test()
                .uninstall_green_runtime_for_test();
        });
        match outcome {
            Ok((end, ())) => tx.send(Ok(end)).expect("send"),
            Err(failure) => tx.send(Err(failure.notice())).expect("send"),
        }
    });

    let outcome = rx.recv().expect("keeper outcome");
    service.join().expect("keeper service");
    let end = outcome.unwrap_or_else(|notice| panic!("era refused: {notice:?}"));
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
}

/// Row 6 (part C — negative D8): while a registered parallel owner is running
/// owner-local code (`Wait::ParallelActive`) OR parked in `Wait::Native`,
/// blocked Baton threads MUST NOT report a false deadlock!
#[test]
fn e53a_row6_parallel_active_or_native_wait_blocks_false_deadlock() {
    let go = Arc::new(AtomicBool::new(false));
    let go_peer = Arc::clone(&go);

    run_in_e53a_era(
        "e53a-row6-no-false-dl",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e53a-nodl-mx (make-mutex \"e53a-nodl-mx\"))
                       (mutex-lock e53a-nodl-mx)
                       'ok)"
                ),
                "ok"
            );
        },
        move |state, port, home, control| {
            let key = ThreadKey::Worker(NonZeroU64::new(95).unwrap());
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);

            let start = Instant::now();
            while !go_peer.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }
            // Stay in Wait::ParallelActive for 25ms (well past 5ms DEADLOCK_POLL),
            // then enter Wait::Native for another 25ms -> Baton must NOT report
            // deadlock in either state!
            std::thread::sleep(Duration::from_millis(25));
            control.begin_parallel_wait(key, Wait::Native);
            std::thread::sleep(Duration::from_millis(25));
            control.end_parallel_wait(key);

            // Now unlock the mutex in a serial turn so Baton finishes cleanly.
            let (grant, lend) = port.request_turn().ok().expect("unlock turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin");
            let out = turn.with_world(|ev| {
                ev.threads.enter_thread(ThreadKey::Main.id());
                eval(ev, "(progn (mutex-unlock e53a-nodl-mx) 'unlocked)")
            });
            assert_eq!(out, "unlocked");
            turn.end(port).ok().expect("end");
            owner.unregister();
        },
        move |main, _control| {
            go.store(true, Ordering::Release);
            let out = eval(
                main,
                "(let ((w (make-thread
                            (lambda ()
                              (mutex-lock e53a-nodl-mx)
                              (prog1 'baton-ok
                                (mutex-unlock e53a-nodl-mx))))))
                   (thread-join w))",
            );
            assert_eq!(
                out, "baton-ok",
                "Wait::ParallelActive and Wait::Native must prevent false D8 deadlock"
            );
        },
    );
}

/// Row 6 (part D — Q2b, single-violation): Baton-only deadlock outside an era
/// produces the EXACT byte-identical error signal string.
#[test]
fn e53a_row6_baton_only_deadlock_verdict_unchanged() {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut ev =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    ev.start_green_threads().expect("start Green threads");

    let dl = eval(
        &mut ev,
        "(progn
           (setq e53a-baton-dl-mx (make-mutex \"baton-only-dl\"))
           (mutex-lock e53a-baton-dl-mx)
           (let ((w (make-thread (lambda () (mutex-lock e53a-baton-dl-mx) (mutex-unlock e53a-baton-dl-mx)))))
             (prog1
                 (list (thread-join w) (thread-last-error))
               (mutex-unlock e53a-baton-dl-mx))))",
    );
    assert_eq!(
        dl,
        "(nil (error \"All threads are blocked: this one would wait forever (GNU Emacs hangs here; neomacs signals)\"))",
        "Q2b: Baton-only deadlock verdict must remain byte-identical"
    );
}

/// §7 S-track newborn Baton signal (single-violation, revert-sensitive, Q6c):
/// A `thread-signal` sent to a newborn Baton worker BEFORE it first runs MUST
/// stay pending across first acquire (GNU `thread.c:146-154`,
/// `green/switch.rs:2490-2495`) so the worker's body actually enters and
/// installs its own `condition-case` before `(thread-yield)` delivers the
/// signal. Deleting newborn deferral (raising at first acquire before body
/// entry) leaves `e53a-s-newborn-entered` nil and fails this test.
#[test]
fn e53a_s_track_newborn_baton_signal_deferred_until_body_entry() {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut ev =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    ev.start_green_threads().expect("start Green threads");

    let out = eval(
        &mut ev,
        "(progn
           (setq e53a-s-newborn-entered nil)
           (setq e53a-s-newborn-caught nil)
           (let ((w (make-thread
                      (lambda ()
                        (setq e53a-s-newborn-entered t)
                        (condition-case err
                            (progn (thread-yield) 'not-signaled)
                          (error
                            (setq e53a-s-newborn-caught (car (cdr err)))
                            'caught-in-body))))))
             (thread-signal w 'error '(\"s-track-newborn\"))
             (let ((joined (condition-case err
                               (thread-join w)
                             (error (list 'join-snapshot (car (cdr err)))))))
               (list e53a-s-newborn-entered e53a-s-newborn-caught joined))))",
    );
    assert_eq!(
        out, "(t \"s-track-newborn\" (join-snapshot \"s-track-newborn\"))",
        "§7 S-track: newborn worker must enter body and catch pre-start signal at its first delivery point"
    );
}

/// NIT-1 exact-identity parity (`c3af2c9784`): `ActiveSerialParkScope` keys on
/// `ActiveSerialTurn { ctx_addr, heap, context_id }`, so a distinct `Context`
/// swapped into the EXACT SAME address on the SAME World/heap is rejected with
/// a typed mismatch error naming both expected and actual `context_id`s, while
/// the legitimate owner's scope remains intact.
#[test]
fn e53a_active_serial_park_scope_refuses_foreign_context_with_exact_identity() {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);

    struct NoopParkPort {
        home: SerialHome,
        generation: u64,
        epoch: u64,
    }
    impl KeeperPort for NoopParkPort {
        type LendGuard = ();
        type CollectReply = ();
        fn give_back(
            &mut self,
            _returned: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
        ) {
        }
        fn give_back_to_collect(
            &mut self,
            _returned: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
            _reply: (),
        ) {
        }
        fn request_turn(
            &mut self,
        ) -> Result<
            (
                crate::emacs_core::eval::parallel_entry::serial_park::Grant,
                (),
            ),
            crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal,
        > {
            self.generation += 1;
            self.epoch += 1;
            let generation = NonZeroU64::new(self.generation).unwrap();
            let epoch = NonZeroU64::new(self.epoch).unwrap();
            let ticket =
                crate::emacs_core::eval::parallel_entry::serial_ticket::StoppedSerialTicket::transport(
                    crate::emacs_core::eval::parallel_entry::serial_ticket::TicketIdentity {
                        owner: self.home.owner,
                        generation,
                        heap: self.home.heap,
                        epoch,
                    },
                );
            Ok((
                crate::emacs_core::eval::parallel_entry::serial_park::Grant {
                    ticket,
                    generation,
                    epoch,
                },
                (),
            ))
        }
        fn refuse(
            &mut self,
            _ticket: crate::emacs_core::eval::parallel_entry::serial_ticket::StoppedSerialTicket,
            _reason: crate::emacs_core::eval::parallel_entry::serial_ticket::ReceiveReason,
        ) {
        }
    }

    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let baton = main.start_green_runtime().expect("start Green runtime");
    let control = main.green_control();
    let _main_state: OwnerSerialState<Context, LocalSerialOwner> =
        OwnerSerialState::new(1, Box::new(main))
            .ok()
            .expect("detach main and stow World carriers");

    let worker = std::thread::spawn(move || {
        baton.enter();
        let owner_a = Context::new_green_owner(&baton);
        let home = SerialHome {
            owner: 9,
            heap: owner_a.tagged_heap.heap_identity(),
        };
        let mut state: OwnerSerialState<Context, LocalSerialOwner> =
            OwnerSerialState::new(home.owner, Box::new(owner_a))
                .ok()
                .expect("owner_a detaches cleanly");
        let mut port = NoopParkPort {
            home,
            generation: 0,
            epoch: 0,
        };
        let grant = port.request_turn().ok().expect("grant");
        let mut live = begin_turn(&mut state, home, grant, &mut port, standalone)
            .ok()
            .expect("begin live serial turn");
        let key = ThreadKey::Worker(NonZeroU64::new(99).unwrap());
        let mut owner_ctl = ParallelOwnerControl::register(control, key);

        let owner_b = with_world_parkable(&mut live, &mut owner_ctl, &mut port, standalone, |ev| {
            let mut owner_b = Context::new_green_owner(&baton);
            let addr_a = std::ptr::from_ref(ev) as usize;
            let heap_a = ev.tagged_heap.heap_identity();
            let id_a = ev.context_instance_id();
            let id_b = owner_b.context_instance_id();
            assert_eq!(owner_b.tagged_heap.heap_identity(), heap_a);
            assert_ne!(id_b, id_a);

            // Swap distinct same-World Context into the exact same address.
            std::mem::swap(ev, &mut owner_b);
            assert_eq!(std::ptr::from_ref(ev) as usize, addr_a);
            assert_eq!(ev.tagged_heap.heap_identity(), heap_a);
            assert_eq!(ev.context_instance_id(), id_b);

            let err = ev
                .parallel_park_in_turn("thread-yield", ParkWait::Yield, None)
                .expect_err("same-address/same-heap foreign Context must be refused");
            let (_sym, data) = format_signal_data(&err);
            assert!(
                data.contains("park bridge mismatch")
                    && data.contains(&format!("context_id: {id_a}"))
                    && data.contains(&format!("context_id: {id_b}")),
                "NIT-1: expected mismatch naming expected context_id={id_a} and actual context_id={id_b}, got: {data}"
            );

            // Swap original Context back; scope must remain intact for `ev`
            // (refusing only because NoopParkPort lacks an era FacadeEpochRetention,
            // NOT with a bridge mismatch!).
            std::mem::swap(ev, &mut owner_b);
            let yield_res = eval(ev, "(thread-yield)");
            assert!(
                yield_res.contains("MissingEpoch"),
                "original Context must reach bridge (MissingEpoch on NoopParkPort, not mismatch), got: {yield_res}"
            );
            owner_b
        });
        live.end(&mut port).ok().expect("end live turn");
        owner_b.retire_green_owner();
        owner_ctl.unregister();
        baton
    });
    let _baton = worker.join().expect("worker finished cleanly");
}

/// Blocker 1 regression row (lead #15273): a heap-allocated Lisp value
/// reachable ONLY from a parked parallel owner's private stack (`ev.lexenv` /
/// `specpdl` / `let` frame) MUST survive forced `(garbage-collect)` cycles run
/// by Baton while the owner is parked across ALL SIX `ParkWait` kinds
/// (`Yield`, `Mutex`, `Condition`, `Join`, `Native`, `Host`).
#[test]
fn e53a_blocker1_parked_stack_roots_survive_gc_across_all_six_park_kinds() {
    let parked_step = Arc::new(AtomicUsize::new(0));
    let parked_step_m = Arc::clone(&parked_step);
    let gc_done_step = Arc::new(AtomicUsize::new(0));
    let gc_done_step_p = Arc::clone(&gc_done_step);
    let (native_rel_tx, native_rel_rx) = mpsc::sync_channel::<()>(1);
    let (host_rel_tx, host_rel_rx) = mpsc::sync_channel::<()>(1);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);
    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);

    run_in_e53a_era(
        "e53a-b1-gc-roots",
        |src| {
            assert_eq!(eval(src, "(setq e53a-b1-yield-gc-done nil)"), "nil");
        },
        move |state, port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin");
            let key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e53a-b1-peer", "e53a-b1-peer"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);

            let step_result: Result<(), String> = with_world_parkable(
                &mut turn,
                &mut owner,
                port,
                standalone,
                |ev| {
                    ev.threads.enter_thread(key.id());

                    // 1. ParkWait::Yield — Lisp `let`-bound cons/string reachable
                    //    ONLY from the parked Lisp stack across `(thread-yield)`.
                    //    The loop is gated INSIDE the `(let ((only-on-stack ...)))`
                    //    frame on `e53a-b1-yield-gc-done` (set by Main ONLY after
                    //    `force_gc_and_clobber(main)`) AND bounded by a wall-clock
                    //    `(current-time)` deadline matching `BOUND` (30s) so any
                    //    Main failure or yield-starvation regression exits the turn
                    //    cleanly, drains `driver.run()`, and fails with a named
                    //    timeout error instead of hanging or poisoning the driver.
                    parked_step.store(1, Ordering::Release);
                    let yield_out = eval(
                        ev,
                        "(let ((only-on-stack (cons (concat \"root-\" \"yield\") (cons 11 22)))
                           (current-time-list nil))
                       (let* ((start (current-time))
                              (deadline (+ (car start) (* 30 (cdr start)))))
                         (while (and (null e53a-b1-yield-gc-done)
                                     (< (car (current-time)) deadline))
                           (thread-yield))
                         (if (null e53a-b1-yield-gc-done)
                             'yield-gc-timeout
                           only-on-stack)))",
                    );
                    if yield_out == "yield-gc-timeout" {
                        let _ = ev.threads.finish_thread(key.id(), Value::NIL);
                        return Err(
                        "ParkWait::Yield: timed out after 30s waiting for Main to set e53a-b1-yield-gc-done"
                            .to_string(),
                    );
                    }
                    assert!(
                        gc_done_step_p.load(Ordering::Acquire) >= 1,
                        "Main must have completed forced GC before ParkWait::Yield loop exited (yield_out={yield_out})"
                    );
                    assert_eq!(
                        yield_out, "(\"root-yield\" 11 . 22)",
                        "ParkWait::Yield: stack-only cons/string must survive GC while parked"
                    );

                    // Helper: verify a private stack-only root (`ev.lexenv`, never
                    // stored in any global symbol) survives two forced GCs while
                    // parked on each blocking `ParkWait` kind.
                    let kinds: [(usize, &str, ParkWait); 5] = [
                        (2, "mutex", ParkWait::Mutex(MutexId::new(8101))),
                        (3, "condition", ParkWait::Condition(ConditionId::new(8102))),
                        (
                            4,
                            "join",
                            ParkWait::Join(ThreadKey::Worker(NonZeroU64::new(8103).unwrap())),
                        ),
                        (5, "native", ParkWait::Native),
                        (
                            6,
                            "host",
                            ParkWait::Host(HostRequestKey::for_test(
                                NonZeroU64::new(8104).unwrap(),
                            )),
                        ),
                    ];

                    let mut native_rx = Some(native_rel_rx);
                    let mut host_rx = Some(host_rel_rx);

                    for (step, label, wait) in kinds {
                        let prev_lexenv = ev.lexenv;
                        // Fresh heap string + cons pair rooted ONLY in `ev.lexenv`
                        // (private Context execution root, not in obarray/globals).
                        let expected_str = format!("root-{label}-alive");
                        let fresh = Value::cons(
                            Value::string(expected_str.as_str()),
                            Value::cons(Value::fixnum(step as i64), Value::fixnum(99)),
                        );
                        ev.lexenv = fresh;

                        let body: Option<Box<dyn FnOnce()>> = match wait {
                            ParkWait::Native => {
                                let rx = native_rx.take().unwrap();
                                Some(Box::new(move || {
                                    rx.recv_timeout(BOUND).expect("native release");
                                }))
                            }
                            ParkWait::Host(_) => {
                                let rx = host_rx.take().unwrap();
                                Some(Box::new(move || {
                                    rx.recv_timeout(BOUND).expect("host release");
                                }))
                            }
                            _ => None,
                        };

                        parked_step.store(step, Ordering::Release);
                        let (_k, outcome) = ev
                            .parallel_park_in_turn(label, wait, body)
                            .expect("parked and resumed cleanly");
                        assert!(
                            matches!(outcome, ParallelWaitOutcome::Woken),
                            "step {step} ({label}): expected Woken"
                        );
                        assert_eq!(
                            gc_done_step_p.load(Ordering::Acquire),
                            step,
                            "Main must have completed forced GC while peer was parked in {label}"
                        );

                        // Inspect the value that was reachable ONLY via `ev.lexenv`
                        // while parked across Main's `(garbage-collect)` cycles.
                        let survived = print_value(&ev.lexenv);
                        let expected = format!("(\"{expected_str}\" {step} . 99)");
                        assert_eq!(
                            survived, expected,
                            "ParkWait::{label}: private stack root must survive forced GC while parked"
                        );
                        ev.lexenv = prev_lexenv;
                    }

                    let _ = ev.threads.finish_thread(key.id(), Value::NIL);
                    Ok(())
                },
            );
            turn.end(port).ok().expect("end");
            owner.unregister();
            done.store(true, Ordering::Release);
            if let Err(msg) = step_result {
                panic!("{msg}");
            }
        },
        move |main, control| {
            let force_gc_and_clobber = |m: &mut Context| {
                // Force two full collecting stops and allocate fresh conses and
                // strings in between so any unrooted swept slot is overwritten.
                assert_eq!(
                    eval(
                        m,
                        "(progn
                           (garbage-collect)
                           (let ((i 0))
                             (while (< i 200)
                               (cons (concat \"clobber-\" \"junk\") (cons i 0))
                               (setq i (1+ i))))
                           (garbage-collect)
                           'gc-ok)"
                    ),
                    "gc-ok"
                );
            };

            // Step 1: Yield — wait until peer enters yield loop, force GC across
            // peer's yield parks, then release peer's Lisp loop!
            let start = Instant::now();
            while parked_step_m.load(Ordering::Acquire) < 1 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.002)");
            }
            force_gc_and_clobber(main);
            gc_done_step.store(1, Ordering::Release);
            assert_eq!(eval(main, "(setq e53a-b1-yield-gc-done t)"), "t");

            // Steps 2..=6: Mutex, Condition, Join, Native, Host.
            let checks: [(usize, Wait); 5] = [
                (2, Wait::Mutex(MutexId::new(8101))),
                (3, Wait::Condition(ConditionId::new(8102))),
                (
                    4,
                    Wait::Join(ThreadKey::Worker(NonZeroU64::new(8103).unwrap())),
                ),
                (5, Wait::Native),
                (
                    6,
                    Wait::HostRequest(HostRequestKey::for_test(NonZeroU64::new(8104).unwrap())),
                ),
            ];
            for (step, expected_wait) in checks {
                let start = Instant::now();
                let key = loop {
                    if parked_step_m.load(Ordering::Acquire) >= step
                        && let Some(k) = *peer_key_m.lock().unwrap()
                        && control.wait_of(k) == Some(expected_wait)
                    {
                        break k;
                    }
                    assert!(start.elapsed() < BOUND, "timed out waiting for step {step}");
                    let _ = eval(main, "(sleep-for 0.002)");
                };
                // Force two full GCs + allocations while peer is parked!
                force_gc_and_clobber(main);
                gc_done_step.store(step, Ordering::Release);
                // Wake peer after GC completes.
                match expected_wait {
                    Wait::Mutex(_) | Wait::Condition(_) | Wait::Join(_) => {
                        assert_eq!(control.wake_waiting_for(expected_wait), 1);
                    }
                    Wait::Native => {
                        native_rel_tx.send(()).expect("release native");
                    }
                    Wait::HostRequest(_) => {
                        host_rel_tx.send(()).expect("release host");
                    }
                    _ => unreachable!(),
                }
                let _ = key;
            }

            let start = Instant::now();
            while !done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// Blocker 4 / Row 3: `Control::wake` on a parallel participant parked in
/// `Wait::HostRequest` MUST leave `OwnerWake` un-woken and keep the wait in
/// `Wait::HostRequest`.
#[test]
fn e53a_d1_control_wake_leaves_host_request_untouched_on_parallel_owner() {
    let control = Arc::new(Control::default());
    let p = ThreadKey::Worker(NonZeroU64::new(301).unwrap());
    let owner = ParallelOwnerControl::register(Arc::clone(&control), p);
    let hk = HostRequestKey::for_test(NonZeroU64::new(302).unwrap());
    control.begin_parallel_wait(p, Wait::HostRequest(hk));
    control.wake(p);
    assert!(
        !owner.wake().is_pending(),
        "Control::wake must NOT wake OwnerWake for Wait::HostRequest"
    );
    assert_eq!(control.wait_of(p), Some(Wait::HostRequest(hk)));
    control.end_parallel_wait(p);
    assert_eq!(control.wait_of(p), Some(Wait::ParallelActive));
    owner.unregister();
}

/// Nit D8-1: dropping `ParallelOwnerControl` without calling `unregister()`
/// (e.g. on panic unwind) automatically unregisters the parallel participant
/// from `Control` so no zombie parked entry remains to skew D8 counts.
#[test]
fn e53a_d8_parallel_owner_control_drop_unregisters_on_unwind() {
    let control = Arc::new(Control::default());
    let p = ThreadKey::Worker(NonZeroU64::new(305).unwrap());
    {
        let owner = ParallelOwnerControl::register(Arc::clone(&control), p);
        control.begin_parallel_wait(p, Wait::Mutex(MutexId::new(306)));
        assert!(control.is_parallel_participant(p));
        drop(owner);
    }
    assert!(
        !control.is_parallel_participant(p),
        "Drop on ParallelOwnerControl must unregister from Control"
    );
    assert!(control.exited(p));
}

/// Blocker 4 (revert-sensitive): `!parallel.wake.is_pending()` suppresses D8
/// deadlock even if a parallel participant's wait is still in `Wait::Mutex`.
#[test]
fn e53a_d8_pending_owner_wake_suppresses_deadlock() {
    let control = Arc::new(Control::default());
    let p = ThreadKey::Worker(NonZeroU64::new(311).unwrap());
    let owner = ParallelOwnerControl::register(Arc::clone(&control), p);
    control.begin_parallel_wait(p, Wait::Mutex(MutexId::new(312)));
    assert!(
        control.all_parallel_deadlock_parked_for_test(),
        "parked parallel owner with no pending wake is a deadlock candidate"
    );
    // Simulate a pending OwnerWake while wait is still Mutex.
    owner.wake().wake();
    assert!(
        !control.all_parallel_deadlock_parked_for_test(),
        "pending OwnerWake must suppress D8 deadlock"
    );
    assert!(owner.wake().try_take());
    assert!(control.all_parallel_deadlock_parked_for_test());
    owner.unregister();
}

/// Blocker 4 (revert-sensitive): D8's `deadlock_epoch` ABA guard rejects a
/// stale `IdleQuery` when a participant wakes, runs, and re-parks (or when
/// `era.turn` changes) between BEFORE and AFTER.
#[test]
fn e53a_d8_deadlock_epoch_aba_rejects_stale_idle_query() {
    use crate::emacs_core::eval::parallel_entry::driver_core::RingWaiter;
    let mut main =
        Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime());
    let baton = main.start_green_runtime().expect("start Green runtime");
    let world_handle = baton.era_world_handle().expect("era world handle");
    let control = main.green_control();
    let _main_rx = control.register_main_guard_for_test();

    let escrowed = control
        .surrender_solo_for_era(TurnRight::Solo(baton))
        .expect("surrender Solo");
    let (inbox_tx, _inbox_rx) = mpsc::channel();
    let (_world_tx, world_rx) = mpsc::sync_channel(1);
    let port = GroupPort::from_raw_parts_for_test(
        inbox_tx,
        world_rx,
        Arc::new(RingWaiter::new_for_test()),
    );
    control
        .open_era(world_handle, port)
        .expect("open era in Idle");

    let p = ThreadKey::Worker(NonZeroU64::new(321).unwrap());
    let owner = ParallelOwnerControl::register(Arc::clone(&control), p);
    let mx = MutexId::new(322);
    // Park both Main and Parallel owner in Mutex waits so AFTER conditions hold.
    control.begin_parallel_wait(p, Wait::Mutex(mx));
    control.set_main_wait_for_test(Wait::Mutex(mx));

    let epoch_before = control.deadlock_epoch_for_test();
    assert!(
        control.confirm_era_deadlock_after_for_test(epoch_before, 1),
        "unchanged epoch with 1 parked parallel owner == 1 enrolled must confirm"
    );

    // ABA: wake and re-park the parallel owner into the EXACT SAME Wait::Mutex(mx).
    control.wake(p);
    assert!(owner.wake().try_take());
    control.begin_parallel_wait(p, Wait::Mutex(mx));

    assert!(
        !control.confirm_era_deadlock_after_for_test(epoch_before, 1),
        "ABA: confirm_era_deadlock_after MUST reject stale epoch_before after wake+re-park"
    );
    let epoch_after = control.deadlock_epoch_for_test();
    assert_ne!(epoch_after, epoch_before);
    assert!(control.confirm_era_deadlock_after_for_test(epoch_after, 1));

    owner.unregister();
    let (_solo, _wh, _port) = control.close_era(escrowed).expect("close era");
}

/// Blocker 4 (revert-sensitive): `wait_parallel_parked` MUST NOT report
/// deadlock when `IdleQuery` reports more enrolled owners than registered
/// parallel participants (`enrolled_owners > parallel_participant_count`).
#[test]
fn e53a_d8_enrolled_owners_exceeding_registered_parallel_blocks_parallel_deadlock() {
    struct OverEnrolledPort;
    impl KeeperPort for OverEnrolledPort {
        type LendGuard = ();
        type CollectReply = ();
        fn give_back(
            &mut self,
            _: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
        ) {
        }
        fn give_back_to_collect(
            &mut self,
            _: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
            _: (),
        ) {
        }
        fn request_turn(
            &mut self,
        ) -> Result<
            (
                crate::emacs_core::eval::parallel_entry::serial_park::Grant,
                (),
            ),
            crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal,
        > {
            unreachable!()
        }
        fn refuse(
            &mut self,
            _: crate::emacs_core::eval::parallel_entry::serial_ticket::StoppedSerialTicket,
            _: crate::emacs_core::eval::parallel_entry::serial_ticket::ReceiveReason,
        ) {
        }
        fn idle_status_for_deadlock(
            &self,
            _bound: Duration,
        ) -> Result<
            crate::emacs_core::eval::parallel_entry::driver_core::IdleStatus,
            crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal,
        > {
            Ok(
                crate::emacs_core::eval::parallel_entry::driver_core::IdleStatus::Idle {
                    enrolled_owners: 2,
                },
            )
        }
    }

    let control = Arc::new(Control::default());
    let p = ThreadKey::Worker(NonZeroU64::new(331).unwrap());
    let owner = ParallelOwnerControl::register(Arc::clone(&control), p);
    let mx = MutexId::new(332);
    control.begin_parallel_wait(p, Wait::Mutex(mx));

    // Spawn a waker that waits 25ms (> 5ms DEADLOCK_POLL) and then wakes `p`.
    // Because `OverEnrolledPort` reports 2 enrolled owners while only 1 is
    // registered in `Control`, `wait_parallel_parked` must NOT falsely report
    // deadlock and must return `Woken`.
    let c_wake = Arc::clone(&control);
    let waker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(c_wake.wake_waiting_for(Wait::Mutex(mx)), 1);
    });
    let outcome = control.wait_parallel_parked(p, owner.wake(), &OverEnrolledPort);
    waker.join().expect("waker joined");
    assert!(
        matches!(outcome, ParallelWaitOutcome::Woken),
        "enrolled_owners(2) > parallel_participant_count(1) must prevent false parallel deadlock, got {outcome:?}"
    );
    owner.unregister();
}

/// Blocker 4 (revert-sensitive): proves **Baton specifically** (not the
/// parallel owner) detects a mixed deadlock when the parallel owner's port
/// suppresses its own deadlock check (`IdleStatus::Busy`). Keeping Baton's
/// `confirm_era_deadlock_after` at `enrolled_owners == 0` fails this test.
#[test]
fn e53a_d8_baton_specifically_detects_mixed_deadlock() {
    struct SuppressParallelDeadlockPort<'a, P: KeeperPort> {
        inner: &'a mut P,
    }
    impl<P: KeeperPort> KeeperPort for SuppressParallelDeadlockPort<'_, P> {
        type LendGuard = P::LendGuard;
        type CollectReply = P::CollectReply;
        fn give_back(
            &mut self,
            returned: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
        ) {
            self.inner.give_back(returned);
        }
        fn give_back_to_collect(
            &mut self,
            returned: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
            reply: Self::CollectReply,
        ) {
            self.inner.give_back_to_collect(returned, reply);
        }
        fn request_turn(
            &mut self,
        ) -> Result<
            (
                crate::emacs_core::eval::parallel_entry::serial_park::Grant,
                Self::LendGuard,
            ),
            crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal,
        > {
            self.inner.request_turn()
        }
        fn refuse(
            &mut self,
            ticket: crate::emacs_core::eval::parallel_entry::serial_ticket::StoppedSerialTicket,
            reason: crate::emacs_core::eval::parallel_entry::serial_ticket::ReceiveReason,
        ) {
            self.inner.refuse(ticket, reason);
        }
        fn abandon_head_hold(&mut self) {
            self.inner.abandon_head_hold();
        }
        fn collect_root_epoch(&self) -> Option<neovm_value::tagged::gc::FacadeEpochRetention> {
            self.inner.collect_root_epoch()
        }
        fn idle_status_for_deadlock(
            &self,
            _bound: Duration,
        ) -> Result<
            crate::emacs_core::eval::parallel_entry::driver_core::IdleStatus,
            crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal,
        > {
            // Suppress parallel-side detection so ONLY Baton can confirm D8.
            Ok(crate::emacs_core::eval::parallel_entry::driver_core::IdleStatus::Busy)
        }
    }

    let main_locked_mx1 = Arc::new(AtomicBool::new(false));
    let main_locked_mx1_p = Arc::clone(&main_locked_mx1);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);
    let peer_done = Arc::new(AtomicBool::new(false));
    let peer_done_m = Arc::clone(&peer_done);

    run_in_e53a_era(
        "e53a-d8-baton-wins",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e53a-bdl-mx1 (make-mutex \"e53a-bdl-mx1\"))
                       (setq e53a-bdl-mx2 (make-mutex \"e53a-bdl-mx2\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |state, port, home, control| {
            let start = Instant::now();
            while !main_locked_mx1_p.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }
            let (grant, lend) = port.request_turn().ok().expect("peer turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin");
            let key = turn.with_world(|ev| {
                create_parallel_thread_handle(ev, "e53a-bdl-peer", "e53a-bdl-peer")
            });
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);

            let mut suppressed = SuppressParallelDeadlockPort { inner: port };
            let peer_outcome =
                with_world_parkable(&mut turn, &mut owner, &mut suppressed, standalone, |ev| {
                    ev.threads.enter_thread(key.id());
                    let mx1_id = {
                        let v = ev.eval_str("e53a-bdl-mx1").unwrap();
                        ev.threads.mutex_id_from_handle(&v).unwrap()
                    };
                    let mx2_id = {
                        let v = ev.eval_str("e53a-bdl-mx2").unwrap();
                        ev.threads.mutex_id_from_handle(&v).unwrap()
                    };
                    let _ = ev.threads.mutex_lock(mx2_id);
                    let (_k, outcome) = ev
                        .parallel_park_in_turn(
                            "mutex-lock",
                            ParkWait::Mutex(MutexId::new(mx1_id.native_key())),
                            None,
                        )
                        .expect("parked");
                    let _ = ev.threads.mutex_unlock(mx2_id);
                    control.wake_waiting_for(Wait::Mutex(MutexId::new(mx2_id.native_key())));
                    let _ = ev.threads.finish_thread(key.id(), Value::NIL);
                    outcome
                });
            turn.end(port).ok().expect("end");
            owner.unregister();
            peer_done.store(true, Ordering::Release);
            assert!(
                matches!(peer_outcome, ParallelWaitOutcome::Woken),
                "parallel owner must be woken after Baton detects mixed deadlock"
            );
        },
        move |main, control| {
            assert_eq!(eval(main, "(progn (mutex-lock e53a-bdl-mx1) 'ok)"), "ok");
            main_locked_mx1.store(true, Ordering::Release);
            let start = Instant::now();
            loop {
                if let Some(key) = *peer_key_m.lock().unwrap()
                    && matches!(control.wait_of(key), Some(Wait::Mutex(_)))
                {
                    break;
                }
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            let mx1_id = {
                let v = main.eval_str("e53a-bdl-mx1").unwrap();
                main.threads.mutex_id_from_handle(&v).unwrap()
            };
            let out = eval(
                main,
                "(condition-case err
                     (progn (mutex-lock e53a-bdl-mx2) (mutex-unlock e53a-bdl-mx2) 'acquired)
                   (error (list 'baton-dl (car (cdr err)))))",
            );
            assert!(
                out.contains("baton-dl") && out.contains("All threads are blocked"),
                "Baton specifically must detect mixed deadlock via confirm_era_deadlock_after, got: {out}"
            );
            assert_eq!(eval(main, "(progn (mutex-unlock e53a-bdl-mx1) 'ok)"), "ok");
            control.wake_waiting_for(Wait::Mutex(MutexId::new(mx1_id.native_key())));
            let start = Instant::now();
            while !peer_done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// Blocker 2 (pre-detach refusal): a pre-detach `MissingEpoch` refuses the park
/// BEFORE detaching, leaves the parallel owner in `Wait::ParallelActive` (never
/// stranded in `Wait::Mutex`), and restores `SERIAL_TURN_PARK_SCOPE`.
#[test]
fn e53a_blocker2_pre_detach_missing_epoch_rolls_back_wait_and_preserves_scope() {
    struct NoEpochPort<'a, P: KeeperPort> {
        inner: &'a mut P,
    }
    impl<P: KeeperPort> KeeperPort for NoEpochPort<'_, P> {
        type LendGuard = P::LendGuard;
        type CollectReply = P::CollectReply;
        fn give_back(
            &mut self,
            r: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
        ) {
            self.inner.give_back(r);
        }
        fn give_back_to_collect(
            &mut self,
            r: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
            reply: Self::CollectReply,
        ) {
            self.inner.give_back_to_collect(r, reply);
        }
        fn request_turn(
            &mut self,
        ) -> Result<
            (
                crate::emacs_core::eval::parallel_entry::serial_park::Grant,
                Self::LendGuard,
            ),
            crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal,
        > {
            self.inner.request_turn()
        }
        fn refuse(
            &mut self,
            t: crate::emacs_core::eval::parallel_entry::serial_ticket::StoppedSerialTicket,
            r: crate::emacs_core::eval::parallel_entry::serial_ticket::ReceiveReason,
        ) {
            self.inner.refuse(t, r);
        }
        fn collect_root_epoch(&self) -> Option<neovm_value::tagged::gc::FacadeEpochRetention> {
            None
        }
    }

    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);
    run_in_e53a_era(
        "e53a-b2-pre-detach",
        |_| {},
        move |state, port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin");
            let key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e53a-b2-peer", "e53a-b2-peer"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            let mut no_epoch = NoEpochPort { inner: port };

            with_world_parkable(&mut turn, &mut owner, &mut no_epoch, standalone, |ev| {
                let err1 = ev
                    .parallel_park_in_turn("mutex-lock", ParkWait::Mutex(MutexId::new(8201)), None)
                    .expect_err("MissingEpoch must refuse pre-detach");
                assert!(format_signal_data(&err1).1.contains("MissingEpoch"));
                assert_eq!(
                    control.wait_of(key),
                    Some(Wait::ParallelActive),
                    "pre-detach refusal must not strand owner in Wait::Mutex"
                );
                // Scope must still be intact for a subsequent call (not unbridged).
                let err2 = ev
                    .parallel_park_in_turn("thread-yield", ParkWait::Yield, None)
                    .expect_err("scope still installed after pre-detach refusal");
                assert!(format_signal_data(&err2).1.contains("MissingEpoch"));
                let _ = ev.threads.finish_thread(key.id(), Value::NIL);
            });
            turn.end(port).ok().expect("end");
            owner.unregister();
            done.store(true, Ordering::Release);
        },
        move |main, _control| {
            let start = Instant::now();
            while !done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

/// Blocker 2 (post-detach fail-closed, F6.1): if `request_turn` fails with
/// `PortRefusal::Poisoned` AFTER detach + `give_back`, `parallel_park_in_turn`
/// panics fail-closed without allocating on the detached heap, without
/// returning a catchable Lisp error, and without restoring `SERIAL_TURN_PARK_SCOPE`.
#[test]
fn e53a_blocker2_post_detach_poisoned_fails_closed_without_restoring_scope() {
    struct PoisonOnResumePort<'a, P: KeeperPort> {
        inner: &'a mut P,
    }
    impl<P: KeeperPort> KeeperPort for PoisonOnResumePort<'_, P> {
        type LendGuard = P::LendGuard;
        type CollectReply = P::CollectReply;
        fn give_back(
            &mut self,
            r: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
        ) {
            self.inner.give_back(r);
        }
        fn give_back_to_collect(
            &mut self,
            r: crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket,
            reply: Self::CollectReply,
        ) {
            self.inner.give_back_to_collect(r, reply);
        }
        fn request_turn(
            &mut self,
        ) -> Result<
            (
                crate::emacs_core::eval::parallel_entry::serial_park::Grant,
                Self::LendGuard,
            ),
            crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal,
        > {
            Err(crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal::Poisoned)
        }
        fn refuse(
            &mut self,
            t: crate::emacs_core::eval::parallel_entry::serial_ticket::StoppedSerialTicket,
            r: crate::emacs_core::eval::parallel_entry::serial_ticket::ReceiveReason,
        ) {
            self.inner.refuse(t, r);
        }
        fn collect_root_epoch(&self) -> Option<neovm_value::tagged::gc::FacadeEpochRetention> {
            self.inner.collect_root_epoch()
        }
    }

    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);
    run_in_e53a_era(
        "e53a-b2-post-detach",
        |_| {},
        move |state, port, home, control| {
            let scope_was_active_on_unwind = Arc::new(AtomicBool::new(true));
            let scope_flag = Arc::clone(&scope_was_active_on_unwind);
            struct UnwindScopeCheck(Arc<AtomicBool>);
            impl Drop for UnwindScopeCheck {
                fn drop(&mut self) {
                    self.0.store(
                        crate::emacs_core::eval::parallel_entry::thread_control::has_active_serial_park_scope_for_test(),
                        Ordering::Release,
                    );
                }
            }
            let thread_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
            let thread_key_ref = Arc::clone(&thread_key_slot);

            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let (grant, lend) = port.request_turn().ok().expect("turn");
                let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                    .ok()
                    .expect("begin");
                let key = turn.with_world(|ev| {
                    create_parallel_thread_handle(ev, "e53a-b2p-peer", "e53a-b2p-peer")
                });
                *thread_key_ref.lock().unwrap() = Some(key);
                let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
                let mut poison_port = PoisonOnResumePort { inner: port };
                with_world_parkable(&mut turn, &mut owner, &mut poison_port, standalone, |ev| {
                    // Drops on panic unwind BEFORE `SerialParkScopeGuard`,
                    // proving `parallel_park_in_turn` did NOT restore
                    // `SERIAL_TURN_PARK_SCOPE` on `PostDetach` (F6.1).
                    let _unwind_check = UnwindScopeCheck(Arc::clone(&scope_flag));
                    // Even wrapped in `(condition-case ...)`, a post-detach
                    // Poisoned failure MUST panic fail-closed instead of
                    // returning a catchable Lisp error on a detached Context.
                    let _ = eval(ev, "(condition-case err (thread-yield) (error 'caught))");
                });
            }));
            let payload = caught.expect_err("post-detach Poisoned must panic fail-closed");
            let msg = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("");
            assert!(
                msg.contains("failed closed after detach") && msg.contains("Poisoned"),
                "expected fail-closed post-detach panic naming Poisoned, got: {msg:?}"
            );
            assert!(
                !scope_was_active_on_unwind.load(Ordering::Acquire),
                "F6.1: SERIAL_TURN_PARK_SCOPE must NOT be restored on PostDetach failure"
            );
            assert!(
                !crate::emacs_core::eval::parallel_entry::thread_control::has_active_serial_park_scope_for_test(),
                "SERIAL_TURN_PARK_SCOPE must remain clear after unwind"
            );
            let key = thread_key_slot
                .lock()
                .unwrap()
                .expect("peer thread handle created");
            assert_eq!(
                control.wait_of(key),
                None,
                "ParallelOwnerControl::drop must unregister key on fail-closed unwind"
            );
            // Re-acquire one real turn on the healthy underlying `port` so
            // `state.reattach` retires the parked `RootBatchLease` from
            // `GreenRuntime::parked` before normal era teardown.
            let (grant2, lend2) = loop {
                match port.request_turn() {
                    Ok(granted) => break granted,
                    Err(
                        crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal::Retry,
                    ) => {
                        std::thread::yield_now();
                    }
                    Err(other) => panic!("cleanup turn request: {other:?}"),
                }
            };
            let mut turn2 = begin_turn(state, home, (grant2, lend2), port, standalone)
                .ok()
                .expect("cleanup begin");
            turn2.with_world(|ev| {
                let _ = ev.threads.finish_thread(key.id(), Value::NIL);
            });
            turn2.end(port).ok().expect("cleanup end");
            done.store(true, Ordering::Release);
        },
        move |main, _control| {
            let start = Instant::now();
            while !done_m.load(Ordering::Acquire) {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

// ===========================================================================
// E5.4a R3 Commit 1 (Blocker 1 — #16167 / #16169): thread-identity
// establishment across `with_world_parkable` entry (before first park),
// `begin_turn`, and `ParkedSerial::resume`. Each row runs Main on Baton to
// clobber World-global `current_thread_id = 0` beforehand and asserts BOTH
// `(eq (current-thread) peer)` AND self-owned `(mutex-unlock held-mx)` BEFORE
// any park (plus checking Main's `thread--blocker` and `wait_condvar` stay
// untouched on `with_world_parkable` entry).
// ===========================================================================

fn lock_mutex_var_as_current(ev: &mut Context, var: &str) {
    let mx_val = ev.eval_str(var).expect("mutex var");
    let mx = ev
        .threads
        .mutex_id_from_handle(&mx_val)
        .expect("valid mutex handle");
    assert_eq!(
        ev.threads.mutex_lock(mx),
        crate::emacs_core::threads::MutexAcquisition::Acquired,
        "acquire {var} as current thread"
    );
}

#[test]
fn e54a_r3_identity_path1_with_world_parkable_entry_before_first_park() {
    let phase = Arc::new(AtomicUsize::new(0));
    let phase_m = Arc::clone(&phase);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);

    run_in_e53a_era(
        "e54a-r3-id-parkable-entry",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-id1-held (make-mutex \"id1-held\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |state, port, home, control| {
            // Turn 1: create peer thread handle and lock `e54a-id1-held` as peer,
            // then deliberately restore `current_thread` to 0 before ending turn 1
            // (and let Main run in between) so entering `with_world_parkable` in
            // Turn 2 starts with stale `current_thread_id == 0` if entry does not
            // establish `owner_key.id()`.
            let (grant1, lend1) = port.request_turn().ok().expect("turn 1");
            let mut turn1 = begin_turn(state, home, (grant1, lend1), port, standalone)
                .ok()
                .expect("begin turn 1");
            let peer_key = turn1.with_world(|ev| {
                let key = create_parallel_thread_handle(ev, "e54a-id1-peer", "id1-peer");
                let prev = ev.threads.enter_thread(key.id());
                lock_mutex_var_as_current(ev, "e54a-id1-held");
                // Leave current_thread_id = 0 to prove `with_world_parkable` entry
                // itself (not `begin_turn`) establishes `owner_key.id()`.
                ev.threads.enter_thread(prev);
                key
            });
            turn1.end(port).ok().expect("end turn 1");
            *peer_key_slot.lock().unwrap() = Some(peer_key);
            phase.store(1, Ordering::Release);

            // Wait until Main has run Lisp on Baton (confirming current_thread == 0).
            let start = Instant::now();
            while phase.load(Ordering::Acquire) < 2 {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }

            // Turn 2: enter `with_world_parkable` WITHOUT manually calling
            // `enter_thread`. Immediately assert `(current-thread)` and
            // self-owned `(mutex-unlock e54a-id1-held)` BEFORE any park, and
            // assert Main's blocker/wait_condvar remain clean across a join wait.
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), peer_key);
            let (grant2, lend2) = loop {
                match port.request_turn() {
                    Ok(g) => break g,
                    Err(
                        crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal::Retry,
                    ) => std::thread::yield_now(),
                    Err(e) => panic!("turn 2 request: {e:?}"),
                }
            };
            let mut turn2 = begin_turn(state, home, (grant2, lend2), port, standalone)
                .ok()
                .expect("begin turn 2");
            let (ct_before_park, unlock_before_park, join_err) =
                with_world_parkable(&mut turn2, &mut owner, port, standalone, |ev| {
                    let ct = eval(ev, "(eq (current-thread) e54a-id1-peer)");
                    let ul = eval(ev, "(progn (mutex-unlock e54a-id1-held) 'unlocked)");
                    // Trying to join Main must NOT falsely report
                    // "Cannot join current thread" (which happens if current_thread==0).
                    // Use `thread-join` on an already-exited dummy thread? Even
                    // cleaner: check `current_thread_id() == peer_key.id()` and
                    // Main's blocker/wait_condvar stay untouched.
                    let tid = ev.threads.current_thread_id();
                    assert_eq!(
                        tid,
                        peer_key.id(),
                        "with_world_parkable entry must set owner id"
                    );
                    let main_blocker = eval(ev, "(thread--blocker main-thread)");
                    (ct, ul, main_blocker)
                });
            turn2.end(port).ok().expect("end turn 2");
            drop(owner);
            phase.store(3, Ordering::Release);

            assert_eq!(
                ct_before_park, "t",
                "(current-thread) must be peer on with_world_parkable entry before first park"
            );
            assert_eq!(
                unlock_before_park, "unlocked",
                "self-owned (mutex-unlock) must succeed on with_world_parkable entry before first park"
            );
            assert_eq!(join_err, "nil");
        },
        move |main, _control| {
            let start = Instant::now();
            while phase_m.load(Ordering::Acquire) < 1 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            let _peer_key = peer_key_m.lock().unwrap().expect("peer key");
            assert_eq!(eval(main, "(eq (current-thread) main-thread)"), "t");
            phase_m.store(2, Ordering::Release);
            while phase_m.load(Ordering::Acquire) < 3 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

#[test]
fn e54a_r3_identity_path2_begin_turn_restores_saved_thread_id() {
    let phase = Arc::new(AtomicUsize::new(0));
    let phase_m = Arc::clone(&phase);

    run_in_e53a_era(
        "e54a-r3-id-begin-turn",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-id2-held (make-mutex \"id2-held\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |state, port, home, _control| {
            // Turn 1: enter peer thread id, lock `e54a-id2-held`, end turn 1
            // while holding peer's thread id (so `detach_serial_owner` saves it).
            let (grant1, lend1) = port.request_turn().ok().expect("turn 1");
            let mut turn1 = begin_turn(state, home, (grant1, lend1), port, standalone)
                .ok()
                .expect("begin turn 1");
            let peer_key = turn1.with_world(|ev| {
                let key = create_parallel_thread_handle(ev, "e54a-id2-peer", "id2-peer");
                ev.threads.enter_thread(key.id());
                lock_mutex_var_as_current(ev, "e54a-id2-held");
                key
            });
            turn1.end(port).ok().expect("end turn 1");
            phase.store(1, Ordering::Release);

            // Wait for Main to run Lisp on Baton (overwriting current_thread to 0).
            let start = Instant::now();
            while phase.load(Ordering::Acquire) < 2 {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }

            // Turn 2: `begin_turn` alone (no `with_world_parkable`, no manual
            // `enter_thread`) must restore `peer_key.id()`.
            let (grant2, lend2) = loop {
                match port.request_turn() {
                    Ok(g) => break g,
                    Err(
                        crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal::Retry,
                    ) => std::thread::yield_now(),
                    Err(e) => panic!("turn 2 request: {e:?}"),
                }
            };
            let mut turn2 = begin_turn(state, home, (grant2, lend2), port, standalone)
                .ok()
                .expect("begin turn 2");
            let (ct, ul) = turn2.with_world(|ev| {
                let ct = eval(ev, "(eq (current-thread) e54a-id2-peer)");
                let ul = eval(ev, "(progn (mutex-unlock e54a-id2-held) 'unlocked)");
                let _ = ev.threads.finish_thread(peer_key.id(), Value::NIL);
                (ct, ul)
            });
            turn2.end(port).ok().expect("end turn 2");
            phase.store(3, Ordering::Release);

            assert_eq!(ct, "t", "begin_turn must restore saved current_thread_id");
            assert_eq!(
                ul, "unlocked",
                "self-owned (mutex-unlock) must succeed immediately after begin_turn"
            );
        },
        move |main, _control| {
            let start = Instant::now();
            while phase_m.load(Ordering::Acquire) < 1 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(eval(main, "(eq (current-thread) main-thread)"), "t");
            phase_m.store(2, Ordering::Release);
            while phase_m.load(Ordering::Acquire) < 3 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

#[test]
fn e54a_r3_identity_path3_parked_serial_resume_restores_saved_thread_id() {
    let phase = Arc::new(AtomicUsize::new(0));
    let phase_m = Arc::clone(&phase);

    run_in_e53a_era(
        "e54a-r3-id-parked-resume",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-id3-held (make-mutex \"id3-held\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |state, port, home, _control| {
            // Turn 1: enter peer id, lock `e54a-id3-held`, park via `LiveSerial::park`.
            let (grant1, lend1) = port.request_turn().ok().expect("turn 1");
            let mut turn1 = begin_turn(state, home, (grant1, lend1), port, standalone)
                .ok()
                .expect("begin turn 1");
            let peer_key = turn1.with_world(|ev| {
                let key = create_parallel_thread_handle(ev, "e54a-id3-peer", "id3-peer");
                ev.threads.enter_thread(key.id());
                lock_mutex_var_as_current(ev, "e54a-id3-held");
                key
            });
            let parked = turn1.park("id3-wait", port).ok().expect("park turn 1");
            phase.store(1, Ordering::Release);

            // Wait for Main to run Lisp on Baton (overwriting current_thread to 0).
            let start = Instant::now();
            while phase.load(Ordering::Acquire) < 2 {
                assert!(start.elapsed() < BOUND);
                std::thread::yield_now();
            }

            // Resume via `ParkedSerial::resume` (no `with_world_parkable`, no
            // manual `enter_thread`) -> must restore `peer_key.id()`.
            let mut turn2 = loop {
                match parked.resume(port, standalone) {
                    Ok(t) => break t,
                    Err(_) => panic!("ParkedSerial::resume failed"),
                }
            };
            let (ct, ul) = turn2.with_world(|ev| {
                let ct = eval(ev, "(eq (current-thread) e54a-id3-peer)");
                let ul = eval(ev, "(progn (mutex-unlock e54a-id3-held) 'unlocked)");
                let _ = ev.threads.finish_thread(peer_key.id(), Value::NIL);
                (ct, ul)
            });
            turn2.end(port).ok().expect("end turn 2");
            phase.store(3, Ordering::Release);

            assert_eq!(
                ct, "t",
                "ParkedSerial::resume must restore saved current_thread_id"
            );
            assert_eq!(
                ul, "unlocked",
                "self-owned (mutex-unlock) must succeed immediately after ParkedSerial::resume"
            );
        },
        move |main, _control| {
            let start = Instant::now();
            while phase_m.load(Ordering::Acquire) < 1 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(eval(main, "(eq (current-thread) main-thread)"), "t");
            phase_m.store(2, Ordering::Release);
            while phase_m.load(Ordering::Acquire) < 3 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

#[test]
fn e54a_r3_identity_path4_with_world_collectible_restores_saved_thread_id() {
    use crate::emacs_core::eval::parallel_entry::keeper_custody::ReturnedSerialTicket;
    use crate::emacs_core::eval::parallel_entry::keeper_driver::{ChannelPort, CollectReply};
    use crate::emacs_core::eval::parallel_entry::serial_park::{
        Grant, KeeperPort, LendGuard, PortRefusal,
    };
    use crate::emacs_core::eval::parallel_entry::serial_ticket::{
        ReceiveReason, SerialDestination, StoppedSerialTicket, TicketIdentity,
    };
    use neovm_value::tagged::gc::FacadeEpochRetention;

    struct ClobberCollectPort<'a, 'b> {
        inner: &'a mut ChannelPort<'b>,
        home: SerialHome,
        phase: Arc<AtomicUsize>,
    }

    impl<'b> KeeperPort for ClobberCollectPort<'_, 'b> {
        type LendGuard = <ChannelPort<'b> as KeeperPort>::LendGuard;
        type CollectReply = CollectReply;

        fn give_back(&mut self, returned: ReturnedSerialTicket) {
            self.inner.give_back(returned);
        }

        fn give_back_to_collect(
            &mut self,
            returned: ReturnedSerialTicket,
            reply: Self::CollectReply,
        ) {
            // 1. Release the ticket normally so Main runs on Baton and clobbers
            //    World-global `current_thread_id` to 0 while `owner_a` is
            //    detached inside `with_world_collectible`.
            self.inner.give_back(returned);
            self.phase.store(1, Ordering::Release);
            let start = Instant::now();
            while self.phase.load(Ordering::Acquire) < 2 {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for Main clobber"
                );
                std::thread::yield_now();
            }
            // 2. Request a fresh ticket and hand it right back to collect (real
            //    keeper collection + real CollectionReport while current_thread
            //    is still 0), so `resume_ticket_for_collect` then calls
            //    `reattach_serial_owner` against stale current_thread == 0.
            let (grant, guard) = loop {
                match self.inner.request_turn() {
                    Ok(g) => break g,
                    Err(PortRefusal::Retry) => std::thread::yield_now(),
                    Err(e) => panic!("clobber collect requeue: {e:?}"),
                }
            };
            let expected = TicketIdentity {
                owner: self.home.owner,
                generation: grant.generation,
                heap: self.home.heap,
                epoch: grant.epoch,
            };
            let mut seat = SerialDestination::new(expected);
            let local = seat
                .receive_serial(grant.ticket)
                .ok()
                .expect("receive")
                .guard();
            let returnable = local.finish(&mut seat).ok().expect("finish");
            self.inner
                .give_back_to_collect(returnable.return_ticket(), reply);
            guard.disarm();
        }

        fn request_turn(&mut self) -> Result<(Grant, Self::LendGuard), PortRefusal> {
            self.inner.request_turn()
        }

        fn refuse(&mut self, ticket: StoppedSerialTicket, reason: ReceiveReason) {
            self.inner.refuse(ticket, reason);
        }

        fn collect_root_epoch(&self) -> Option<FacadeEpochRetention> {
            self.inner.collect_root_epoch()
        }
    }

    let phase = Arc::new(AtomicUsize::new(0));
    let phase_m = Arc::clone(&phase);

    run_in_e53a_era(
        "e54a-r3-id-collectible",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e54a-id4-held (make-mutex \"id4-held\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |state, port, home, _control| {
            let (grant, lend) = port.request_turn().ok().expect("turn 1");
            let mut turn = begin_turn(state, home, (grant, lend), port, standalone)
                .ok()
                .expect("begin turn 1");
            let peer_key = turn.with_world(|ev| {
                let key = create_parallel_thread_handle(ev, "e54a-id4-peer", "id4-peer");
                ev.threads.enter_thread(key.id());
                lock_mutex_var_as_current(ev, "e54a-id4-held");
                key
            });
            let mut clobber_port = ClobberCollectPort {
                inner: port,
                home,
                phase: Arc::clone(&phase),
            };
            let (ct, ul) = turn.with_world_collectible(&mut clobber_port, standalone, |ev| {
                ev.gc_collect_serial_turn()
                    .expect("with_world_collectible GC round-trip");
                let ct = eval(ev, "(eq (current-thread) e54a-id4-peer)");
                let ul = eval(ev, "(progn (mutex-unlock e54a-id4-held) 'unlocked)");
                let _ = ev.threads.finish_thread(peer_key.id(), Value::NIL);
                (ct, ul)
            });
            turn.end(port).ok().expect("end turn 1");
            phase.store(3, Ordering::Release);

            assert_eq!(
                ct, "t",
                "with_world_collectible must restore saved current_thread_id after clobber"
            );
            assert_eq!(
                ul, "unlocked",
                "self-owned (mutex-unlock) must succeed after with_world_collectible clobber"
            );
        },
        move |main, _control| {
            let start = Instant::now();
            while phase_m.load(Ordering::Acquire) < 1 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(eval(main, "(eq (current-thread) main-thread)"), "t");
            phase_m.store(2, Ordering::Release);
            while phase_m.load(Ordering::Acquire) < 3 {
                assert!(start.elapsed() < BOUND);
                let _ = eval(main, "(sleep-for 0.005)");
            }
        },
    );
}

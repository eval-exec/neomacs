//! E5.3b: `thread-join` of and by parallel owners (`D5`, Row 4) and G5 6-step
//! `retire_parallel_owner` (`E6P-INTEGRATION-MAP.md` §4 G5, Q7 ruling).

use crate::emacs_core::eval::green::{Control, ThreadKey, Wait};
use crate::emacs_core::eval::owner_bindings::OwnerSlabHandle;
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
const PEER1_OWNER_ID: u64 = 9601;
const PEER2_OWNER_ID: u64 = 9602;

fn dummy_top_level_handler() -> crate::emacs_core::eval::ConditionFrame {
    crate::emacs_core::eval::ConditionFrame::Catch {
        tag: Value::NIL,
        resume: crate::emacs_core::eval::ResumeTarget::InterpreterCatch,
    }
}

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

fn eval_flow(ev: &mut Context, src: &str) -> neovm_value::error::EvalResult {
    ev.eval_str(src).map_err(|err| match err {
        neovm_value::error::EvalError::Signal { symbol, data, .. } => {
            neovm_value::error::signal(neovm_value::intern::resolve_sym(symbol), data)
        }
        other => neovm_value::error::signal("error", vec![Value::string(format!("{other:?}"))]),
    })
}

fn standalone(_: &mut LocalSerialOwner) -> Result<(), Infallible> {
    Ok(())
}

fn retire_peer(
    state: OwnerSerialState<Context, LocalSerialOwner>,
    owner: ParallelOwnerControl,
    slab_handle: Option<OwnerSlabHandle>,
    port: ChannelPort<'_>,
    home: SerialHome,
    finalize: impl FnOnce(&mut Context) -> neovm_value::error::EvalResult,
) -> Result<
    crate::emacs_core::eval::parallel_entry::thread_control::ParallelRetireReport,
    crate::emacs_core::eval::parallel_entry::thread_control::ParallelRetireError,
> {
    retire_parallel_owner(
        state,
        owner,
        slab_handle,
        port,
        home,
        standalone,
        finalize,
        retire_mutator_rooted,
    )
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

fn run_in_e53b_era<S, M, P>(label: &'static str, setup: S, peer_body: P, main_body: M)
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
        "E5.3b live era rows require a generational heap"
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

fn run_in_e53b_two_peer_era<S, M, P1, P2>(
    label: &'static str,
    setup: S,
    peer1_body: P1,
    peer2_body: P2,
    main_body: M,
) where
    S: FnOnce(&mut Context),
    M: FnOnce(&mut Context, &Arc<Control>) + Send + 'static,
    P1: FnOnce(
            OwnerSerialState<Context, LocalSerialOwner>,
            ChannelPort<'_>,
            SerialHome,
            &Arc<Control>,
        ) + Send
        + 'static,
    P2: FnOnce(
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
        "E5.3b live era rows require a generational heap"
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

            let (p1_setup_tx, p1_setup_rx) = mpsc::sync_channel::<TurnRight>(1);
            let (p1_back_tx, p1_back_rx) = mpsc::sync_channel::<TurnRight>(1);
            let (p2_setup_tx, p2_setup_rx) = mpsc::sync_channel::<TurnRight>(1);
            let (p2_back_tx, p2_back_rx) = mpsc::sync_channel::<TurnRight>(1);

            let (end, body_panic, p1_panic, p2_panic) = std::thread::scope(|scope| {
                let permit1 = keeper
                    .register(PEER1_OWNER_ID, 0)
                    .expect("register parallel owner 1");
                let (g1_tx, g1_rx) = mpsc::sync_channel(1);
                driver.enroll(PEER1_OWNER_ID, permit1.key(), g1_tx);

                let permit2 = keeper
                    .register(PEER2_OWNER_ID, 0)
                    .expect("register parallel owner 2");
                let (g2_tx, g2_rx) = mpsc::sync_channel(1);
                driver.enroll(PEER2_OWNER_ID, permit2.key(), g2_tx);
                let p1_pub = driver.publishing_owners_counter();
                let p2_pub = driver.publishing_owners_counter();

                let p1_inbox = inbox.clone();
                let p1_control = Arc::clone(&control);
                let (p1_idle_tx, p1_idle_rx) = mpsc::sync_channel::<()>(1);
                let p1_handle = scope.spawn(move || {
                    let mutator = activate(permit1, heap, PEER1_OWNER_ID);
                    let mut port =
                        ChannelPort::new(PEER1_OWNER_ID, mutator, p1_inbox, g1_rx, p1_pub);
                    let start = Instant::now();
                    let _idle_receipt = loop {
                        match port.publish_idle_receipt_for_test() {
                            Ok(receipt) => break receipt,
                            Err(
                                crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal::Retry,
                            ) if start.elapsed() < BOUND => std::thread::yield_now(),
                            Err(err) => panic!("peer1 publish idle: {err:?}"),
                        }
                    };
                    p1_idle_tx.send(()).expect("peer1 idle ready");
                    let right = p1_setup_rx
                        .recv_timeout(BOUND)
                        .expect("peer1 setup TurnRight");
                    right.enter();
                    let owner = Context::new_green_owner_with_turn(&right);
                    let state: OwnerSerialState<Context, LocalSerialOwner> =
                        OwnerSerialState::new(PEER1_OWNER_ID, Box::new(owner))
                            .ok()
                            .expect("detach peer1");
                    p1_back_tx.send(right).expect("return peer1 TurnRight");
                    let home = SerialHome {
                        owner: PEER1_OWNER_ID,
                        heap,
                    };
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        peer1_body(state, port, home, &p1_control);
                    }))
                    .err()
                });

                let p2_inbox = inbox.clone();
                let p2_control = Arc::clone(&control);
                let (p2_idle_tx, p2_idle_rx) = mpsc::sync_channel::<()>(1);
                let p2_handle = scope.spawn(move || {
                    let mutator = activate(permit2, heap, PEER2_OWNER_ID);
                    let mut port =
                        ChannelPort::new(PEER2_OWNER_ID, mutator, p2_inbox, g2_rx, p2_pub);
                    let start = Instant::now();
                    let _idle_receipt = loop {
                        match port.publish_idle_receipt_for_test() {
                            Ok(receipt) => break receipt,
                            Err(
                                crate::emacs_core::eval::parallel_entry::serial_park::PortRefusal::Retry,
                            ) if start.elapsed() < BOUND => std::thread::yield_now(),
                            Err(err) => panic!("peer2 publish idle: {err:?}"),
                        }
                    };
                    p2_idle_tx.send(()).expect("peer2 idle ready");
                    let right = p2_setup_rx
                        .recv_timeout(BOUND)
                        .expect("peer2 setup TurnRight");
                    right.enter();
                    let owner = Context::new_green_owner_with_turn(&right);
                    let state: OwnerSerialState<Context, LocalSerialOwner> =
                        OwnerSerialState::new(PEER2_OWNER_ID, Box::new(owner))
                            .ok()
                            .expect("detach peer2");
                    p2_back_tx.send(right).expect("return peer2 TurnRight");
                    let home = SerialHome {
                        owner: PEER2_OWNER_ID,
                        heap,
                    };
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        peer2_body(state, port, home, &p2_control);
                    }))
                    .err()
                });

                p1_idle_rx.recv_timeout(BOUND).expect("peer1 idle");
                p2_idle_rx.recv_timeout(BOUND).expect("peer2 idle");
                drop(inbox);

                let main_control = Arc::clone(&control);
                let main_handle = scope.spawn(move || {
                    let mut initial_turn = main_control
                        .acquire_initial_era_turn()
                        .expect("acquire initial era WorldTurn");
                    p1_setup_tx
                        .send(initial_turn)
                        .expect("send initial TurnRight to peer1");
                    initial_turn = p1_back_rx
                        .recv_timeout(BOUND)
                        .expect("receive TurnRight from peer1");
                    p2_setup_tx
                        .send(initial_turn)
                        .expect("send initial TurnRight to peer2");
                    initial_turn = p2_back_rx
                        .recv_timeout(BOUND)
                        .expect("receive TurnRight from peer2");
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
                let p1_panic = p1_handle.join().expect("peer1 thread");
                let p2_panic = p2_handle.join().expect("peer2 thread");
                let body_panic = main_handle.join().expect("main Baton thread");
                (end, body_panic, p1_panic, p2_panic)
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
            let _ = panic_tx.send(body_panic.or(p1_panic).or(p2_panic));
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

/// Row 4a: Baton calls `(thread-join PEER1)` on a live parallel owner; PEER1
/// runs `unwind-protect` and exits through G5 6-step `retire_parallel_owner`.
/// Proves Baton wakes via `publish_parallel_owner_exit`, receives PEER1's
/// freshly allocated result rooted across forced GC, sees `thread-live-p` nil,
/// sees `all-threads` exclude PEER1, and `Control` returns to baseline counts.
#[test]
fn e53b_row4a_baton_joins_parallel_owner_result_gc_rooted_and_retires() {
    let peer_ready = Arc::new(AtomicBool::new(false));
    let peer_ready_m = Arc::clone(&peer_ready);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);

    run_in_e53b_era(
        "e53b-row4a-baton-joins-par",
        |src| {
            assert_eq!(eval(src, "(setq e53b-4a-cleanup nil)"), "nil");
        },
        move |mut state, mut port, home, control| {
            let key = {
                let (grant, lend) = port.request_turn().ok().expect("turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin1");
                let key = turn
                    .with_world(|ev| create_parallel_thread_handle(ev, "e53b-4a-p1", "e53b-4a-p1"));
                turn.end(&mut port).ok().expect("end1");
                key
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);
            peer_ready.store(true, Ordering::Release);

            // Wait until Baton main is actually parked in `Wait::Join(key)` so
            // `publish_parallel_owner_exit` exercises the real cross-kind wake.
            let start = Instant::now();
            while control.wait_of(ThreadKey::Main) != Some(Wait::Join(key)) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for Baton main to park in Wait::Join"
                );
                std::thread::yield_now();
            }

            let report = retire_peer(state, owner, None, port, home, |ev| {
                eval_flow(
                    ev,
                    "(unwind-protect
                             (cons (concat \"p1-\" \"result\") 42)
                           (setq e53b-4a-cleanup 'ran))",
                )
            })
            .expect("G5 retire_parallel_owner must succeed");
            assert!(
                report.slab_unregistered,
                "G5 Step 5 must unregister and free the parallel owner's slab"
            );
        },
        move |main, control| {
            let baseline = control.registry_sizes_for_test();
            wait_for_flag_on_baton(main, &peer_ready_m, "peer_ready");
            let peer_key = peer_key_m.lock().unwrap().expect("peer key recorded");

            let joined = eval(
                main,
                "(let ((res (thread-join e53b-4a-p1)))
                   (garbage-collect)
                   res)",
            );
            assert_eq!(
                joined, "(\"p1-result\" . 42)",
                "Baton joining parallel owner must receive GC-rooted return value"
            );
            assert_eq!(
                eval(main, "e53b-4a-cleanup"),
                "ran",
                "unwind-protect cleanup must run before parallel owner exit"
            );
            assert_eq!(
                eval(main, "(thread-live-p e53b-4a-p1)"),
                "nil",
                "thread-live-p must be nil after G5 retire"
            );
            assert_eq!(
                eval(
                    main,
                    "(if (memq e53b-4a-p1 (all-threads)) 'present 'absent)"
                ),
                "absent",
                "G5 Step 5 must remove retired parallel owner from all-threads"
            );
            assert!(
                !control.worker_registered_for_test(peer_key),
                "G5 Step 6 must remove retired parallel owner from Control"
            );
            assert_eq!(
                control.registry_sizes_for_test(),
                baseline,
                "Control registry sizes must return to baseline after parallel retire"
            );
        },
    );
}

/// Row 4b: Baton snapshots a parallel owner's injected `thread-signal` BEFORE
/// waiting and re-raises that snapshot even when the parallel owner catches and
/// clears it and triggers GC before exiting via `retire_parallel_owner`.
#[test]
fn e53b_row4b_baton_joins_parallel_owner_reraises_signal_snapshot() {
    let peer_ready = Arc::new(AtomicBool::new(false));
    let peer_ready_m = Arc::clone(&peer_ready);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));

    run_in_e53b_era(
        "e53b-row4b-baton-snapshot",
        |_| {},
        move |mut state, mut port, home, control| {
            let key = {
                let (grant, lend) = port.request_turn().ok().expect("turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin1");
                let key = turn
                    .with_world(|ev| create_parallel_thread_handle(ev, "e53b-4b-p1", "e53b-4b-p1"));
                turn.end(&mut port).ok().expect("end1");
                key
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);
            peer_ready.store(true, Ordering::Release);

            let start = Instant::now();
            while control.wait_of(ThreadKey::Main) != Some(Wait::Join(key)) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for main Wait::Join"
                );
                std::thread::yield_now();
            }

            let mut cleared = false;
            retire_peer(state, owner, None, port, home, |ev| {
                // Deliver and catch the injected signal inside the peer with a
                // non-empty condition stack (GNU `post_acquire_global_lock`,
                // `src/thread.c:146-154`), clearing `ThreadPayload.error_*`,
                // then return normally.
                ev.push_condition_frame(dummy_top_level_handler());
                cleared = ev.deliver_parallel_turn_signal(key).is_err();
                let _ = ev.pop_condition_frame();
                eval_flow(ev, "'caught-and-cleared")
            })
            .expect("retire_parallel_owner must succeed");
            assert!(
                cleared,
                "expected pending signal to be delivered and cleared in peer"
            );
        },
        move |main, _control| {
            wait_for_flag_on_baton(main, &peer_ready_m, "peer_ready");
            let out = eval(
                main,
                "(progn
                   (thread-signal e53b-4b-p1 'error (list (concat \"snap-\" \"from-main\")))
                   (condition-case err
                       (thread-join e53b-4b-p1)
                     (error (list 'reraised (car (cdr err))))))",
            );
            assert_eq!(
                out, "(reraised \"snap-from-main\")",
                "Baton thread-join must re-raise pre-wait signal snapshot even after peer cleared it"
            );
        },
    );
}

/// Row 4c: Parallel owner exits with an uncaught internal Lisp error via
/// `retire_parallel_owner`. GNU `record_thread_error` (`src/thread.c:753-758`):
/// `thread-join` returns `nil` (does NOT re-raise) and `thread-last-error`
/// records the error.
#[test]
fn e53b_row4c_baton_joins_parallel_owner_uncaught_internal_error() {
    let peer_ready = Arc::new(AtomicBool::new(false));
    let peer_ready_m = Arc::clone(&peer_ready);

    run_in_e53b_era(
        "e53b-row4c-internal-error",
        |_| {},
        move |mut state, mut port, home, control| {
            let key = {
                let (grant, lend) = port.request_turn().ok().expect("turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin1");
                let key = turn
                    .with_world(|ev| create_parallel_thread_handle(ev, "e53b-4c-p1", "e53b-4c-p1"));
                turn.end(&mut port).ok().expect("end1");
                key
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            peer_ready.store(true, Ordering::Release);

            let start = Instant::now();
            while control.wait_of(ThreadKey::Main) != Some(Wait::Join(key)) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for main Wait::Join"
                );
                std::thread::yield_now();
            }

            retire_peer(state, owner, None, port, home, |ev| {
                eval_flow(ev, "(signal 'error '(\"p1-internal-boom\"))")
            })
            .expect("retire_parallel_owner must succeed on uncaught Lisp error");
        },
        move |main, _control| {
            wait_for_flag_on_baton(main, &peer_ready_m, "peer_ready");
            assert_eq!(
                eval(main, "(thread-join e53b-4c-p1)"),
                "nil",
                "thread-join on uncaught internal error must return nil"
            );
            assert_eq!(
                eval(main, "(thread-last-error)"),
                "(error \"p1-internal-boom\")",
                "thread-last-error must record the parallel owner's uncaught error"
            );
        },
    );
}

/// Row 4d: Baton joiner waiting on a parallel owner receives its own
/// `thread-signal` before the parallel owner exits -> own signal preempts the
/// target's pre-wait signal snapshot (`src/thread.c:1110-1115`).
#[test]
fn e53b_row4d_baton_joiner_own_signal_preempts_parallel_target_snapshot() {
    let peer_ready = Arc::new(AtomicBool::new(false));
    let peer_ready_m = Arc::clone(&peer_ready);
    let release_peer = Arc::new(AtomicBool::new(false));
    let release_peer_p = Arc::clone(&release_peer);

    run_in_e53b_era(
        "e53b-row4d-baton-own-sig",
        |_| {},
        move |mut state, mut port, home, control| {
            let key = {
                let (grant, lend) = port.request_turn().ok().expect("turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin1");
                let key = turn
                    .with_world(|ev| create_parallel_thread_handle(ev, "e53b-4d-p1", "e53b-4d-p1"));
                turn.end(&mut port).ok().expect("end1");
                key
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            peer_ready.store(true, Ordering::Release);

            wait_for_flag(&release_peer_p, "release_peer");
            retire_peer(state, owner, None, port, home, |_| {
                Ok(Value::symbol("p1-done"))
            })
            .expect("retire_parallel_owner must succeed");
        },
        move |main, control| {
            wait_for_flag_on_baton(main, &peer_ready_m, "peer_ready");
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (setq e53b-4d-result nil)
                       (thread-signal e53b-4d-p1 'error '(\"target-snap\"))
                       (setq e53b-4d-joiner
                             (make-thread
                               (lambda ()
                                 (setq e53b-4d-result
                                       (condition-case err
                                           (thread-join e53b-4d-p1)
                                         (error (list 'caught (car (cdr err)))))))))
                       (thread-yield)
                       'spawned)"
                ),
                "spawned"
            );
            let joiner_key = {
                let val = main.eval_str("e53b-4d-joiner").unwrap();
                ThreadKey::from_id(main.threads.thread_id_from_handle(&val).unwrap())
            };
            let start = Instant::now();
            while !matches!(control.wait_of(joiner_key), Some(Wait::Join(_))) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for Baton joiner Wait::Join"
                );
                let _ = eval(main, "(thread-yield)");
            }
            let _ = eval(
                main,
                "(progn
                   (thread-signal e53b-4d-joiner 'error '(\"joiner-own-signal\"))
                   (condition-case nil (thread-join e53b-4d-joiner) (error nil)))",
            );
            let out = eval(main, "e53b-4d-result");
            release_peer.store(true, Ordering::Release);
            let _ = eval(
                main,
                "(condition-case nil (thread-join e53b-4d-p1) (error nil))",
            );
            assert_eq!(
                out, "(caught \"joiner-own-signal\")",
                "joiner's own thread-signal must preempt target's snapshot"
            );
        },
    );
}

/// Row 4e: Parallel owner calls `(thread-join ...)` inside `with_world_parkable`:
/// (1) self-join signals `"Cannot join current thread"`;
/// (2) joining a live Baton worker parks in `ParkWait::Join` and returns its
///     GC-rooted result;
/// (3) joining a live Baton worker with an injected `thread-signal` snapshot
///     roots the snapshot across GC while parked and re-raises it even after
///     the Baton worker catches, clears, and runs `(garbage-collect)`.
#[test]
fn e53b_row4_parallel_owner_joins_baton_worker_result() {
    let worker_ready = Arc::new(AtomicBool::new(false));
    let worker_ready_p = Arc::clone(&worker_ready);
    let peer_joining_w1 = Arc::new(AtomicBool::new(false));
    let peer_joining_w1_m = Arc::clone(&peer_joining_w1);
    let peer_joining_w2 = Arc::new(AtomicBool::new(false));
    let peer_joining_w2_m = Arc::clone(&peer_joining_w2);
    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);

    run_in_e53b_era(
        "e53b-row4e-par-joins-baton",
        |src| {
            assert_eq!(
                eval(
                    src,
                    "(progn
                       (setq e53b-w1-mx (make-mutex \"e53b-w1-mx\"))
                       (setq e53b-w2-mx (make-mutex \"e53b-w2-mx\"))
                       'ok)"
                ),
                "ok"
            );
        },
        move |mut state, mut port, home, control| {
            wait_for_flag(&worker_ready_p, "worker_ready");
            let (grant, lend) = port.request_turn().ok().expect("turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("begin");
            let key = turn.with_world(|ev| {
                create_parallel_thread_handle(ev, "e53b-p-joiner", "e53b-p-joiner")
            });
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), key);
            let (self_join, joined_w1, joined_w2) = with_world_parkable(
                &mut turn,
                &mut owner,
                &mut port,
                standalone,
                |ev| {
                    ev.threads.enter_thread(key.id());
                    let sj = eval(ev, "(thread-join e53b-p-joiner)");
                    peer_joining_w1.store(true, Ordering::Release);
                    let w1 = eval(ev, "(thread-join e53b-baton-w1)");
                    peer_joining_w2.store(true, Ordering::Release);
                    let w2 = eval(
                        ev,
                        "(progn
                           (thread-signal e53b-baton-w2 'error (list (concat \"baton-\" \"snap-gc\")))
                           (condition-case err
                               (thread-join e53b-baton-w2)
                             (error (list 'reraised (car (cdr err))))))",
                    );
                    (sj, w1, w2)
                },
            );
            turn.end(&mut port).ok().expect("end");
            retire_peer(state, owner, None, port, home, |_| Ok(Value::NIL))
                .expect("retire parallel joiner");
            done.store(true, Ordering::Release);

            assert_eq!(
                self_join, "ERR error (\"Cannot join current thread\")",
                "self-join from parallel owner must signal Cannot join current thread"
            );
            assert_eq!(
                joined_w1, "(\"baton-result\" . 77)",
                "parallel owner joining Baton worker must receive its return value"
            );
            assert_eq!(
                joined_w2, "(reraised \"baton-snap-gc\")",
                "parallel owner must root and re-raise Baton worker's pre-wait signal snapshot across GC"
            );
        },
        move |main, _control| {
            assert_eq!(
                eval(
                    main,
                    "(progn
                       (mutex-lock e53b-w1-mx)
                       (mutex-lock e53b-w2-mx)
                       (setq e53b-baton-w1
                             (make-thread
                               (lambda ()
                                 (mutex-lock e53b-w1-mx)
                                 (mutex-unlock e53b-w1-mx)
                                 (cons (concat \"baton-\" \"result\") 77))))
                       (setq e53b-baton-w2
                             (make-thread
                               (lambda ()
                                 (condition-case nil
                                     (progn
                                       (mutex-lock e53b-w2-mx)
                                       (mutex-unlock e53b-w2-mx))
                                   (error nil))
                                 (garbage-collect)
                                 'w2-cleared)))
                       (thread-yield)
                       'spawned)"
                ),
                "spawned"
            );
            worker_ready.store(true, Ordering::Release);

            wait_for_flag_on_baton(main, &peer_joining_w1_m, "peer_joining_w1");
            assert_eq!(eval(main, "(progn (mutex-unlock e53b-w1-mx) 'u1)"), "u1");
            assert_eq!(
                eval(main, "(thread-join e53b-baton-w1)"),
                "(\"baton-result\" . 77)"
            );

            wait_for_flag_on_baton(main, &peer_joining_w2_m, "peer_joining_w2");
            assert_eq!(eval(main, "(progn (mutex-unlock e53b-w2-mx) 'u2)"), "u2");
            let _ = eval(
                main,
                "(condition-case nil (thread-join e53b-baton-w2) (error nil))",
            );

            wait_for_flag_on_baton(main, &done_m, "peer_done");
        },
    );
}

/// Row 4f (part 1): Parallel owner `PEER1` joins parallel owner `PEER2`.
/// `PEER2` exits via G5 `retire_parallel_owner` -> `publish_parallel_owner_exit`
/// wakes `Participant::Parallel(PEER1)` directly and `PEER1` receives `PEER2`'s
/// GC-rooted result.
#[test]
fn e53b_row4f_parallel_owner_joins_parallel_owner_result() {
    let p2_ready = Arc::new(AtomicBool::new(false));
    let p2_ready_1 = Arc::clone(&p2_ready);
    let p1_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let p1_key_2 = Arc::clone(&p1_key_slot);
    let p2_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let p2_key_1 = Arc::clone(&p2_key_slot);
    let done = Arc::new(AtomicBool::new(false));
    let done_m = Arc::clone(&done);

    run_in_e53b_two_peer_era(
        "e53b-row4f-par-joins-par",
        |_| {},
        move |mut state, mut port, home, control| {
            wait_for_flag(&p2_ready_1, "p2_ready");
            let p2_key = p2_key_1.lock().unwrap().expect("p2 key");
            let (grant, lend) = port.request_turn().ok().expect("p1 turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("p1 begin");
            let p1_key =
                turn.with_world(|ev| create_parallel_thread_handle(ev, "e53b-4f-p1", "e53b-4f-p1"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), p1_key);
            *p1_key_slot.lock().unwrap() = Some(p1_key);
            let (joined, p2_exited) =
                with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                    ev.threads.enter_thread(p1_key.id());
                    let res = eval(ev, "(thread-join e53b-4f-p2)");
                    (res, control.exited(p2_key))
                });
            turn.end(&mut port).ok().expect("p1 end");
            retire_peer(state, owner, None, port, home, |_| Ok(Value::NIL)).expect("retire p1");
            done.store(true, Ordering::Release);
            assert!(p2_exited, "p2 must be exited after p1 thread-join returns");
            assert_eq!(
                joined, "(\"peer2-val\" . 99)",
                "parallel owner joining parallel owner must receive its GC-rooted result"
            );
        },
        move |mut state, mut port, home, control| {
            let p2_key = {
                let (grant, lend) = port.request_turn().ok().expect("p2 turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("p2 begin1");
                let key = turn
                    .with_world(|ev| create_parallel_thread_handle(ev, "e53b-4f-p2", "e53b-4f-p2"));
                turn.end(&mut port).ok().expect("p2 end1");
                key
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), p2_key);
            *p2_key_slot.lock().unwrap() = Some(p2_key);
            p2_ready.store(true, Ordering::Release);

            // Wait until PEER1 is parked in `Wait::Join(p2_key)` before retiring.
            let start = Instant::now();
            loop {
                if let Some(p1_key) = *p1_key_2.lock().unwrap()
                    && control.wait_of(p1_key) == Some(Wait::Join(p2_key))
                {
                    break;
                }
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for p1 Wait::Join(p2)"
                );
                std::thread::yield_now();
            }

            retire_peer(state, owner, None, port, home, |ev| {
                eval_flow(ev, "(cons (concat \"peer2-\" \"val\") 99)")
            })
            .expect("retire p2");
        },
        move |main, _control| {
            wait_for_flag_on_baton(main, &done_m, "row4f_done");
        },
    );
}

/// Row 4f (part 2): Parallel owner `PEER1` parked in `(thread-join e53b-4f2-p2)`
/// receives its own `thread-signal` from `main` while `PEER2` also has a
/// pending `thread-signal` snapshot -> `PEER1`'s own signal preempts `PEER2`'s
/// snapshot (`src/thread.c:1110-1115`).
#[test]
fn e53b_row4f2_parallel_joiner_own_signal_preempts_parallel_target_snapshot() {
    let p2_signalled = Arc::new(AtomicBool::new(false));
    let p2_signalled_1 = Arc::clone(&p2_signalled);
    let p2_ready = Arc::new(AtomicBool::new(false));
    let p2_ready_m = Arc::clone(&p2_ready);
    let p1_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let p1_key_m = Arc::clone(&p1_key_slot);
    let p2_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let p2_key_m = Arc::clone(&p2_key_slot);
    let release_p2 = Arc::new(AtomicBool::new(false));
    let release_p2_2 = Arc::clone(&release_p2);
    let p1_done = Arc::new(AtomicBool::new(false));
    let p1_done_m = Arc::clone(&p1_done);

    run_in_e53b_two_peer_era(
        "e53b-row4f2-own-sig-preempt",
        |_| {},
        move |mut state, mut port, home, control| {
            wait_for_flag(&p2_signalled_1, "p2_signalled");
            let (grant, lend) = port.request_turn().ok().expect("p1 turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("p1 begin");
            let p1_key = turn
                .with_world(|ev| create_parallel_thread_handle(ev, "e53b-4f2-p1", "e53b-4f2-p1"));
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), p1_key);
            *p1_key_slot.lock().unwrap() = Some(p1_key);
            let out = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                ev.threads.enter_thread(p1_key.id());
                eval(
                    ev,
                    "(condition-case err
                         (thread-join e53b-4f2-p2)
                       (error (list 'caught (car (cdr err)))))",
                )
            });
            turn.end(&mut port).ok().expect("p1 end");
            retire_peer(state, owner, None, port, home, |_| Ok(Value::NIL)).expect("retire p1");
            p1_done.store(true, Ordering::Release);
            assert_eq!(
                out, "(caught \"p1-own-sig\")",
                "parallel joiner's own thread-signal must preempt target's snapshot"
            );
        },
        move |mut state, mut port, home, control| {
            let p2_key = {
                let (grant, lend) = port.request_turn().ok().expect("p2 turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("p2 begin1");
                let key = turn.with_world(|ev| {
                    create_parallel_thread_handle(ev, "e53b-4f2-p2", "e53b-4f2-p2")
                });
                turn.end(&mut port).ok().expect("p2 end1");
                key
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), p2_key);
            *p2_key_slot.lock().unwrap() = Some(p2_key);
            p2_ready.store(true, Ordering::Release);
            wait_for_flag(&release_p2_2, "release_p2");
            retire_peer(state, owner, None, port, home, |_| {
                Ok(Value::symbol("p2-done"))
            })
            .expect("retire p2");
        },
        move |main, control| {
            wait_for_flag_on_baton(main, &p2_ready_m, "p2_ready");
            let p2_key = p2_key_m.lock().unwrap().expect("p2 key");
            assert_eq!(
                eval(
                    main,
                    "(progn (thread-signal e53b-4f2-p2 'error '(\"p2-snap\")) 'ok)"
                ),
                "ok"
            );
            p2_signalled.store(true, Ordering::Release);

            // Wait until PEER1 is parked in `Wait::Join(p2_key)`.
            let start = Instant::now();
            loop {
                if let Some(p1_key) = *p1_key_m.lock().unwrap()
                    && control.wait_of(p1_key) == Some(Wait::Join(p2_key))
                {
                    break;
                }
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for p1 Wait::Join(p2)"
                );
                let _ = eval(main, "(sleep-for 0.005)");
            }
            assert_eq!(
                eval(
                    main,
                    "(progn (thread-signal e53b-4f2-p1 'error '(\"p1-own-sig\")) 'sent)"
                ),
                "sent"
            );
            wait_for_flag_on_baton(main, &p1_done_m, "p1_done");
            release_p2.store(true, Ordering::Release);
            let _ = eval(
                main,
                "(condition-case nil (thread-join e53b-4f2-p2) (error nil))",
            );
        },
    );
}

/// Row 4g: Proves G5 Step 2 (`ev.unbind_to(0)` unwinds leftover `specpdl`
/// entries to depth 0) and G5 Step 5 (`OwnerSlabRegistry::unregister` removes
/// and frees the owner's slab after dropping `Option<OwnerSlabHandle>` and
/// `Context`, followed by a forced GC with the retired slab and `Mutator` gone).
#[test]
fn e53b_row4g_g5_retire_unwinds_specpdl_and_unregisters_owner_slab() {
    let baseline_slabs_slot = Arc::new(std::sync::Mutex::new(None::<usize>));
    let baseline_slabs_p = Arc::clone(&baseline_slabs_slot);
    let peer_slab_live = Arc::new(AtomicBool::new(false));
    let peer_slab_live_m = Arc::clone(&peer_slab_live);
    let main_checked_live = Arc::new(AtomicBool::new(false));
    let main_checked_live_p = Arc::clone(&main_checked_live);
    let peer_retired = Arc::new(AtomicBool::new(false));
    let peer_retired_m = Arc::clone(&peer_retired);

    run_in_e53b_era(
        "e53b-row4g-g5-specpdl-slab",
        |src| {
            assert_eq!(eval(src, "(setq e53b-4g-var 'outer-val)"), "outer-val");
        },
        move |mut state, mut port, home, control| {
            let start = Instant::now();
            while baseline_slabs_p.lock().unwrap().is_none() {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for baseline_slabs"
                );
                std::thread::yield_now();
            }

            let (key, slab_handle): (ThreadKey, OwnerSlabHandle) = {
                let (grant, lend) = port.request_turn().ok().expect("turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin1");
                let out = turn.with_world(|ev| {
                    let key = create_parallel_thread_handle(ev, "e53b-4g-p1", "e53b-4g-p1");
                    // Allocate this parallel owner's slab and extract its unique
                    // parallel `OwnerSlabHandle` (E4-P2 C1).
                    let handle = ev
                        .take_parallel_owner_slab_handle()
                        .expect("mint unique OwnerSlabHandle for parallel owner");
                    (key, handle)
                });
                turn.end(&mut port).ok().expect("end1");
                out
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            peer_slab_live.store(true, Ordering::Release);
            wait_for_flag(&main_checked_live_p, "main_checked_live");

            let mut bound_in_peer = String::new();
            let mut specpdl_non_empty = false;
            let report = retire_peer(state, owner, Some(slab_handle), port, home, |ev| {
                // Deliberately push a dynamic `specbind` entry onto
                // `ev.specpdl` and leave it unwound when `finalize` returns
                // so G5 Step 2 (`ev.unbind_to(0)`) MUST unwind it!
                let sym = neovm_value::intern::intern("e53b-4g-var");
                ev.try_specbind(sym, Value::symbol("leaked-in-peer"))
                    .expect("specbind dynamic binding");
                bound_in_peer = eval(ev, "e53b-4g-var");
                specpdl_non_empty = !ev.specpdl.is_empty();
                Ok(Value::symbol("g5-ok"))
            })
            .expect("G5 retire_parallel_owner must succeed");
            peer_retired.store(true, Ordering::Release);
            assert_eq!(bound_in_peer, "leaked-in-peer");
            assert!(
                specpdl_non_empty,
                "specpdl must be non-empty before G5 Step 2"
            );
            assert!(
                report.slab_unregistered,
                "G5 Step 5 must unregister and free the parallel owner's slab"
            );
        },
        move |main, _control| {
            let live_slabs = main.owner_slab_count_for_test();
            *baseline_slabs_slot.lock().unwrap() = Some(live_slabs);
            wait_for_flag_on_baton(main, &peer_slab_live_m, "peer_slab_live");
            assert_eq!(
                main.owner_slab_count_for_test(),
                live_slabs,
                "live parallel owner with OwnerSlabHandle keeps its slab registered in OwnerSlabRegistry"
            );
            assert!(
                live_slabs >= 2,
                "expected at least main + parallel owner slabs before retirement, got {live_slabs}"
            );
            main_checked_live.store(true, Ordering::Release);

            assert_eq!(eval(main, "(thread-join e53b-4g-p1)"), "g5-ok");
            wait_for_flag_on_baton(main, &peer_retired_m, "peer_retired");
            // Force a full GC stop after Step 4 + Step 5 so the collector walks
            // `OwnerSlabRegistry` and `Mutator` roster post-retirement.
            let _ = eval(main, "(garbage-collect)");
            assert_eq!(
                eval(main, "e53b-4g-var"),
                "outer-val",
                "G5 Step 2 must unwind specpdl to depth 0 and restore dynamic bindings"
            );
            assert_eq!(
                main.owner_slab_count_for_test(),
                live_slabs - 1,
                "G5 Step 5 must remove and free the retired parallel owner's slab"
            );
        },
    );
}

/// Row 4h: Parallel-owner join cycle deadlock (`PEER1` joins `PEER2`, `PEER2`
/// joins `PEER1`, `Main` joins `PEER1`) -> D8 detects the cross-participant
/// deadlock and signals `"All threads are blocked"`, breaking the cycle cleanly
/// with zero hang.
#[test]
fn e53b_row4h_parallel_owner_join_cycle_deadlock_signals() {
    let handles_ready = Arc::new(AtomicBool::new(false));
    let handles_ready_2 = Arc::clone(&handles_ready);
    let handles_ready_m = Arc::clone(&handles_ready);
    let p1_parked = Arc::new(AtomicBool::new(false));
    let p1_parked_2 = Arc::clone(&p1_parked);
    let p1_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let p1_key_2 = Arc::clone(&p1_key_slot);
    let p2_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let p2_key_1 = Arc::clone(&p2_key_slot);
    let any_deadlock = Arc::new(AtomicBool::new(false));
    let any_deadlock_1 = Arc::clone(&any_deadlock);
    let any_deadlock_2 = Arc::clone(&any_deadlock);
    let any_deadlock_m = Arc::clone(&any_deadlock);
    let p1_done = Arc::new(AtomicBool::new(false));
    let p1_done_m = Arc::clone(&p1_done);
    let p2_done = Arc::new(AtomicBool::new(false));
    let p2_done_m = Arc::clone(&p2_done);

    run_in_e53b_two_peer_era(
        "e53b-row4h-join-cycle-d8",
        |_| {},
        move |mut state, mut port, home, control| {
            let (grant, lend) = port.request_turn().ok().expect("p1 turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("p1 begin");
            let (p1_key, p2_key) = turn.with_world(|ev| {
                let k1 = create_parallel_thread_handle(ev, "e53b-4h-p1", "e53b-4h-p1");
                let k2 = create_parallel_thread_handle(ev, "e53b-4h-p2", "e53b-4h-p2");
                (k1, k2)
            });
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), p1_key);
            *p1_key_slot.lock().unwrap() = Some(p1_key);
            *p2_key_slot.lock().unwrap() = Some(p2_key);
            handles_ready.store(true, Ordering::Release);

            // Wait until PEER2 is registered in `Control` so `control.exited(p2_key)` is false.
            let start = Instant::now();
            while !control.is_parallel_participant(p2_key) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for p2 registration"
                );
                std::thread::yield_now();
            }

            let out = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                ev.threads.enter_thread(p1_key.id());
                p1_parked.store(true, Ordering::Release);
                eval(ev, "(thread-join e53b-4h-p2)")
            });
            if out.contains("All threads are blocked") {
                any_deadlock_1.store(true, Ordering::Release);
            }
            turn.end(&mut port).ok().expect("p1 end");
            retire_peer(state, owner, None, port, home, |_| {
                Ok(Value::symbol("p1-exited"))
            })
            .expect("retire p1");
            p1_done.store(true, Ordering::Release);
        },
        move |mut state, mut port, home, control| {
            wait_for_flag(&handles_ready_2, "handles_ready");
            let p1_key = p1_key_2.lock().unwrap().expect("p1 key");
            let p2_key = p2_key_1.lock().unwrap().expect("p2 key");
            let mut owner = ParallelOwnerControl::register(Arc::clone(control), p2_key);

            // Wait until BOTH Main and PEER1 are already parked in `Wait::Join`
            // so PEER2 closes the cycle and triggers D8 immediately on entry.
            wait_for_flag(&p1_parked_2, "p1_parked");
            let start = Instant::now();
            while control.wait_of(p1_key) != Some(Wait::Join(p2_key))
                || control.wait_of(ThreadKey::Main) != Some(Wait::Join(p1_key))
            {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for Main + PEER1 Wait::Join cycle"
                );
                std::thread::yield_now();
            }

            let (grant, lend) = port.request_turn().ok().expect("p2 turn");
            let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                .ok()
                .expect("p2 begin");
            let out = with_world_parkable(&mut turn, &mut owner, &mut port, standalone, |ev| {
                ev.threads.enter_thread(p2_key.id());
                eval(ev, "(thread-join e53b-4h-p1)")
            });
            if out.contains("All threads are blocked") {
                any_deadlock_2.store(true, Ordering::Release);
            }
            turn.end(&mut port).ok().expect("p2 end");
            retire_peer(state, owner, None, port, home, |_| {
                Ok(Value::symbol("p2-exited"))
            })
            .expect("retire p2");
            p2_done.store(true, Ordering::Release);
        },
        move |main, _control| {
            wait_for_flag_on_baton(main, &handles_ready_m, "handles_ready_m");
            let out = eval(main, "(thread-join e53b-4h-p1)");
            if out.contains("All threads are blocked") {
                any_deadlock_m.store(true, Ordering::Release);
                // If Main happened to win the deadlock race before PEER2,
                // signal PEER1 to break the surviving 2-peer cycle cleanly.
                let _ = eval(main, "(thread-signal e53b-4h-p1 'error '(\"break-cycle\"))");
            }
            wait_for_flag_on_baton(main, &p1_done_m, "p1_done");
            wait_for_flag_on_baton(main, &p2_done_m, "p2_done");
            assert!(
                any_deadlock.load(Ordering::Acquire),
                "D8 must detect the parallel-owner join cycle and signal All threads are blocked (main got: {out})"
            );
        },
    );
}

/// Row 4i: G5 Step 4 `RendezvousError::Poisoned` disposition — even when
/// `retire_mutator` reports `RendezvousError::Poisoned` after the owner is
/// already retired, `retire_parallel_owner` still runs Step 6 exit publication
/// so parked Baton joiners wake cleanly (no wedge), removes the retired
/// participant from `Control`, and returns
/// `Err(ParallelRetireError::EraPoisonedAfterRetire)`.
#[test]
fn e53b_row4i_step4_poisoned_retire_wakes_joiner_and_returns_era_poisoned() {
    let peer_ready = Arc::new(AtomicBool::new(false));
    let peer_ready_m = Arc::clone(&peer_ready);
    let peer_key_slot = Arc::new(std::sync::Mutex::new(None::<ThreadKey>));
    let peer_key_m = Arc::clone(&peer_key_slot);

    run_in_e53b_era(
        "e53b-row4i-step4-poisoned",
        |_| {},
        move |mut state, mut port, home, control| {
            let key = {
                let (grant, lend) = port.request_turn().ok().expect("turn1");
                let mut turn = begin_turn(&mut state, home, (grant, lend), &mut port, standalone)
                    .ok()
                    .expect("begin1");
                let key = turn
                    .with_world(|ev| create_parallel_thread_handle(ev, "e53b-4i-p1", "e53b-4i-p1"));
                turn.end(&mut port).ok().expect("end1");
                key
            };
            let owner = ParallelOwnerControl::register(Arc::clone(control), key);
            *peer_key_slot.lock().unwrap() = Some(key);
            peer_ready.store(true, Ordering::Release);

            let start = Instant::now();
            while control.wait_of(ThreadKey::Main) != Some(Wait::Join(key)) {
                assert!(
                    start.elapsed() < BOUND,
                    "timed out waiting for main Wait::Join"
                );
                std::thread::yield_now();
            }

            let err = retire_parallel_owner(
                state,
                owner,
                None,
                port,
                home,
                standalone,
                |_| Ok(Value::symbol("retired-before-poison")),
                |mutator| {
                    // Retire the live test mutator so the test harness drains
                    // cleanly, then simulate Step 4 observing an already-poisoned era.
                    retire_mutator_rooted(mutator).expect("harness mutator retire");
                    Err(RendezvousError::Poisoned)
                },
            )
            .expect_err("Step 4 Poisoned must return Err(EraPoisonedAfterRetire)");
            assert_eq!(
                err,
                crate::emacs_core::eval::parallel_entry::thread_control::ParallelRetireError::EraPoisonedAfterRetire
            );
        },
        move |main, control| {
            wait_for_flag_on_baton(main, &peer_ready_m, "peer_ready");
            let peer_key = peer_key_m.lock().unwrap().expect("peer key");
            assert_eq!(
                eval(main, "(thread-join e53b-4i-p1)"),
                "retired-before-poison",
                "Step 4 Poisoned must still publish Step 6 exit so parked joiners wake cleanly"
            );
            assert!(
                !control.worker_registered_for_test(peer_key),
                "Step 4 Poisoned must still remove retired participant from Control"
            );
        },
    );
}

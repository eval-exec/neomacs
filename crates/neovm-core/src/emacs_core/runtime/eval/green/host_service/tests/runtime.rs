//! Actual Green publication/restore controls. No wall-clock admission or TLS.

use super::scheduling::{FixtureGcCompleted, NativeServiceScope};
use super::transport::{NativeOutcome, NativeReply, NativeSettlement};
use super::*;
use crate::emacs_core::error::{EvalResult, Flow, signal};
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::intern::intern;
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::SubrSpec;
use crate::emacs_core::value::Value;
use crate::host_effect::contracts::OwnedNativeData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};

fn host_error(error: HostRootError) -> Flow {
    signal("error", vec![Value::string(error.to_string())])
}

fn binding(context: &Context) -> Value {
    context
        .special_variable_value_by_id(intern("s2-service-binding"))
        .unwrap()
}

/// The graph is a Rust local absent from Context's precise ordinary census.
/// Only this request's retained RootBatch roots it while another owner collects.
fn request_from_worker(context: &mut Context) -> EvalResult {
    let worker = std::thread::current().id();
    let before = binding(context);
    let graph = Value::list(vec![
        Value::string("host-record-only-leaf"),
        Value::make_cons(Value::fixnum(17), Value::fixnum(29)),
    ]);
    // SAFETY: this original owner's graph remains live, with no callback,
    // collection or handoff before retained batch publication returns.
    let prepared = unsafe { context.green_prepare_host_call(&[graph]) }.map_err(host_error)?;
    let key = prepared.key();
    let returned = context.green_host_call(
        prepared,
        OwnedNativeData::Text("owned native arguments".into()),
        |restored, settlement| {
            assert_eq!(std::thread::current().id(), worker, "request-induced work resumes on its requester OS thread");
            assert_eq!(binding(restored.context), before, "the requester's dynamic bindings are restored before decoding");
            assert_eq!(print_value(&graph), "(\"host-record-only-leaf\" (17 . 29))");
            restored.with_registry(|registry, _| {
                assert!(registry.records.contains_key(&key), "decoding precedes owner acknowledgement/root retirement");
            });
            assert!(matches!(settlement, NativeSettlement::Completed(NativeOutcome::Reply(NativeReply::Returned(OwnedNativeData::Text(text)))) if text == "native-main-result"));
            Ok(Value::T)
        },
    )?;
    assert_eq!(std::thread::current().id(), worker);
    Ok(Value::list(vec![before, binding(context), returned]))
}

fn collect_and_complete(context: &mut Context) -> EvalResult {
    context.with_green_runtime(|runtime| {
        let parked = runtime.parked.borrow();
        assert!(
            parked.len() >= 2,
            "main's original Join and the requester remain parked"
        );
    });
    context.gc_collect_exact();
    context
        .with_green_runtime(|runtime| runtime.parked.borrow_mut().services.finish_fixture_gc())
        .map_err(host_error)?;
    Ok(Value::T)
}

/// B completes A's actual native wait while it still holds the Lisp Baton,
/// then queues a second host call before main can reacquire A's service turn.
fn collect_complete_and_request(context: &mut Context) -> EvalResult {
    let worker = std::thread::current().id();
    let original_main_wait =
        context.with_green_runtime(|runtime| runtime.control.wait_of(ThreadKey::Main).unwrap());
    assert!(matches!(
        original_main_wait,
        super::super::scheduler::Wait::Join(_)
    ));
    collect_and_complete(context)?;
    context.with_green_runtime(|runtime| {
        assert_eq!(
            runtime.control.wait_of(ThreadKey::Main),
            Some(original_main_wait),
            "completing A's native wait must not complete main's original Join"
        );
        runtime
            .parked
            .borrow_mut()
            .services
            .install_fixture_driver(move |scope| {
                assert_ne!(
                    std::thread::current().id(),
                    worker,
                    "B's native work cannot run on B's requesting OS thread"
                );
                assert_eq!(
                    scope.take_arguments().unwrap(),
                    OwnedNativeData::Text("owned native arguments".into())
                );
                result()
            });
    });
    // No Lisp release point occurs between sending A's completion and B's
    // publication. A's native acquire therefore races only metadata, and must
    // be served before a new service turn can run on the same main OS thread.
    request_from_worker(context)
}

fn collect_after_immediate(context: &mut Context) -> EvalResult {
    context.gc_collect_exact();
    context
        .with_green_runtime(|runtime| runtime.parked.borrow_mut().services.finish_fixture_gc())
        .map_err(host_error)?;
    Ok(Value::T)
}

fn refused_request(context: &mut Context) -> EvalResult {
    let worker = std::thread::current().id();
    // SAFETY: empty original-owner input, published before any handoff.
    let prepared = unsafe { context.green_prepare_host_call(&[]) }.map_err(host_error)?;
    context.green_host_call(prepared, OwnedNativeData::None, |_, settlement| {
        assert_eq!(std::thread::current().id(), worker);
        assert!(matches!(
            settlement,
            NativeSettlement::Completed(NativeOutcome::HostClosed)
        ));
        Ok(Value::T)
    })
}

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-service-request",
        request_from_worker
    )));
    context.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-service-gc-complete",
        collect_and_complete
    )));
    context.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-service-complete-and-request",
        collect_complete_and_request
    )));
    context.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-service-after-immediate",
        collect_after_immediate
    )));
    context.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-service-refusal",
        refused_request
    )));
    context.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed0(
        "s2-main-native-worker",
        main_native_live_worker
    )));
    context.eval_str("(defvar s2-service-binding 10)").unwrap();
    context.start_green_threads().unwrap();
    context
}

fn install(
    context: &Context,
    count: Arc<AtomicUsize>,
    completion: mpsc::Sender<FixtureGcCompleted>,
    native: impl FnOnce(&mut NativeServiceScope<'_>) -> NativeOutcome + Send + 'static,
) {
    context.with_green_runtime(|runtime| {
        let mut parked = runtime.parked.borrow_mut();
        parked
            .services
            .install_fixture_completion(completion, count);
        parked.services.install_fixture_driver(native);
    });
}

fn result() -> NativeOutcome {
    NativeOutcome::Reply(NativeReply::Returned(OwnedNativeData::Text(
        "native-main-result".into(),
    )))
}

#[test]
fn artificial_service_hop_preserves_requester_slice_and_restores_its_owner() {
    let mut main = green();
    let main_os = std::thread::current().id();
    let peer_turns = Arc::new(AtomicUsize::new(0));
    let turns = Arc::clone(&peer_turns);
    let (completed, _completion) = mpsc::channel();
    install(&main, Arc::clone(&peer_turns), completed, move |scope| {
        assert_eq!(
            std::thread::current().id(),
            main_os,
            "native work stays on evaluator-host main"
        );
        assert_eq!(
            turns.load(Ordering::SeqCst),
            0,
            "an artificial host hop cannot run an unrelated Lisp owner"
        );
        assert_eq!(
            scope.take_arguments().unwrap(),
            OwnedNativeData::Text("owned native arguments".into())
        );
        result()
    });
    let value = main.eval_str("(let ((a (make-thread (lambda () (let ((s2-service-binding 20)) (s2-service-request))))) (b (make-thread (lambda () (s2-service-after-immediate))))) (list (thread-join a) (thread-join b) s2-service-binding))").unwrap();
    assert_eq!(print_value(&value), "((20 20 t) t 10)");
    assert_eq!(peer_turns.load(Ordering::SeqCst), 1);
    main.with_green_runtime(|runtime| {
        assert!(
            runtime.parked.borrow().is_empty(),
            "both the ordinary and host publications retired"
        )
    });
}

#[test]
fn real_native_wait_releases_to_peer_collection_and_keeps_host_graph_alive() {
    let mut main = green();
    let main_os = std::thread::current().id();
    let peer_turns = Arc::new(AtomicUsize::new(0));
    let turns = Arc::clone(&peer_turns);
    let (completed, completion) = mpsc::channel();
    install(&main, Arc::clone(&peer_turns), completed, move |scope| {
        assert_eq!(std::thread::current().id(), main_os);
        assert_eq!(turns.load(Ordering::SeqCst), 0);
        let witness = scope.receive_native(completion).unwrap();
        assert_ne!(
            witness.worker, main_os,
            "collection ran on another Lisp owner's OS thread"
        );
        assert_eq!(turns.load(Ordering::SeqCst), 1);
        result()
    });
    let value = main.eval_str("(let ((a (make-thread (lambda () (let ((s2-service-binding 20)) (s2-service-request))))) (b (make-thread (lambda () (s2-service-gc-complete))))) (list (thread-join a) (thread-join b) s2-service-binding))").unwrap();
    assert_eq!(print_value(&value), "((20 20 t) t 10)");
    assert_eq!(peer_turns.load(Ordering::SeqCst), 1);
    main.with_green_runtime(|runtime| assert!(runtime.parked.borrow().is_empty()));
}

#[test]
fn native_acquire_precedes_second_queued_service_and_both_owners_restore() {
    let mut main = green();
    let main_os = std::thread::current().id();
    let peer_turns = Arc::new(AtomicUsize::new(0));
    let turns = Arc::clone(&peer_turns);
    let (completed, completion) = mpsc::channel();
    install(&main, Arc::clone(&peer_turns), completed, move |scope| {
        assert_eq!(std::thread::current().id(), main_os);
        assert_eq!(turns.load(Ordering::SeqCst), 0);
        let witness = scope.receive_native(completion).unwrap();
        assert_ne!(witness.worker, main_os);
        assert_eq!(turns.load(Ordering::SeqCst), 1);
        result()
    });
    let value = main.eval_str("(let ((a (make-thread (lambda () (let ((s2-service-binding 20)) (s2-service-request))))) (b (make-thread (lambda () (let ((s2-service-binding 20)) (s2-service-complete-and-request)))))) (list (thread-join a) (thread-join b) s2-service-binding))").unwrap();
    assert_eq!(print_value(&value), "((20 20 t) (20 20 t) 10)");
    assert_eq!(peer_turns.load(Ordering::SeqCst), 1);
    main.with_green_runtime(|runtime| {
        assert!(
            runtime.parked.borrow().is_empty(),
            "both request records and main/requester parked publications retired"
        );
    });
}

#[test]
fn closed_actor_returns_terminal_without_invoking_native_driver() {
    let mut main = green();
    main.with_green_runtime(|runtime| {
        let mut parked = runtime.parked.borrow_mut();
        parked
            .services
            .install_fixture_driver(|_| panic!("closed admission must suppress native dispatch"));
        parked.services.close().unwrap();
    });
    let value = main
        .eval_str("(thread-join (make-thread (lambda () (s2-service-refusal))))")
        .unwrap();
    assert_eq!(value, Value::T);
    main.with_green_runtime(|runtime| assert!(runtime.parked.borrow().is_empty()));
}

#[test]
fn main_request_refusal_restores_owner_and_retains_no_publication() {
    let mut main = green();
    let value = main
        .eval_str("(let ((s2-service-binding 30)) (list (s2-service-refusal) s2-service-binding))")
        .unwrap();
    assert_eq!(print_value(&value), "(t 30)");
    assert_eq!(
        main.eval_str("s2-service-binding").unwrap(),
        Value::fixnum(10)
    );
    main.with_green_runtime(|runtime| assert!(runtime.parked.borrow().is_empty()));
}

/// A native Rust continuation retains its scratch graph and dynamic binding
/// while main attaches the host. No Context moves between OS threads.
fn main_native_live_worker(context: &mut Context) -> EvalResult {
    use crate::emacs_core::eval::{push_scratch_gc_roots, save_scratch_gc_roots};
    let _scratch = SavedNativeScratch(save_scratch_gc_roots());
    let graph = Value::list(vec![Value::string("native-worker-root"), Value::fixnum(73)]);
    push_scratch_gc_roots(&[graph]);
    let worker = std::thread::current().id();
    let original = binding(context);
    context.green_park(super::super::scheduler::Wait::Runnable)?;
    assert_eq!(std::thread::current().id(), worker);
    assert_eq!(binding(context), original);
    assert_eq!(print_value(&graph), "(\"native-worker-root\" 73)");
    context.with_green_runtime(|runtime| {
        assert!(runtime.parked.borrow().contains_key(&ThreadKey::Main));
    });
    context.gc_collect_exact();
    context
        .with_green_runtime(|runtime| runtime.parked.borrow_mut().services.finish_fixture_gc())
        .map_err(host_error)?;
    Ok(Value::list(vec![graph, original, binding(context)]))
}

struct SavedNativeScratch(usize);
impl Drop for SavedNativeScratch {
    fn drop(&mut self) {
        crate::emacs_core::eval::restore_scratch_gc_roots(self.0);
    }
}

#[test]
fn late_host_input_native_wait_preserves_live_worker_bindings_cleanup_and_roots() {
    use super::super::scheduler::MainNativeWaitEvent;
    use crate::emacs_core::eval::{push_scratch_gc_roots, save_scratch_gc_roots};
    use crate::host_input::HostInputPublisher;
    use crate::keyboard::{InputEvent, KeyEvent};
    let mut main = green();
    let main_os = std::thread::current().id();
    main.eval_str("(defvar s2-main-cleanup 0) (setq s2-main-worker (make-thread (lambda () (let ((s2-service-binding 37)) (unwind-protect (s2-main-native-worker) (setq s2-main-cleanup (1+ s2-main-cleanup))))))) (thread-yield)").unwrap();
    main.with_green_runtime(|runtime| {
        assert_eq!(
            runtime.parked.borrow().len(),
            1,
            "live worker is canonically rooted, not already exited"
        );
    });
    // Collect while the original worker's Rust continuation is parked.
    main.gc_collect_exact();
    let publisher = HostInputPublisher::install(&mut main);
    publisher
        .send(InputEvent::key_press(KeyEvent::char('x')))
        .unwrap();
    let control = main.with_green_runtime(|runtime| Arc::clone(&runtime.control));
    let before = control.main_native_events_for_test();
    assert_eq!(
        main.eval_str("(read-event nil nil 10)").unwrap(),
        Value::fixnum('x' as i64)
    );
    assert_eq!(
        control.main_native_events_for_test(),
        before,
        "queued input cannot fabricate a physical release"
    );
    let _scratch = SavedNativeScratch(save_scratch_gc_roots());
    let graph = Value::list(vec![Value::string("native-main-root"), Value::fixnum(91)]);
    push_scratch_gc_roots(&[graph]);
    let (completed, completion) = mpsc::channel();
    let count = Arc::new(AtomicUsize::new(0));
    main.with_green_runtime(|runtime| {
        runtime
            .parked
            .borrow_mut()
            .services
            .install_fixture_completion(completed, Arc::clone(&count))
    });
    let producer = std::thread::spawn(move || {
        let witness = completion
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        assert_ne!(witness.worker, main_os);
        publisher
            .send(InputEvent::key_press(KeyEvent::char('y')))
            .unwrap();
        publisher
    });
    assert_eq!(
        main.eval_str("(read-event nil nil 10)").unwrap(),
        Value::fixnum('y' as i64)
    );
    assert_eq!(std::thread::current().id(), main_os);
    assert_eq!(print_value(&graph), "(\"native-main-root\" 91)");
    let _publisher = producer.join().unwrap();
    let after = control.main_native_events_for_test();
    let blocks = &after[before.len()..];
    assert!(
        !blocks.is_empty(),
        "worker GC requires an actual released main wait"
    );
    assert_eq!(blocks.len() % 4, 0);
    for block in blocks.chunks_exact(4) {
        assert_eq!(
            block,
            &[
                MainNativeWaitEvent::Released,
                MainNativeWaitEvent::PhysicalBlockEntered,
                MainNativeWaitEvent::PhysicalBlockReturned,
                MainNativeWaitEvent::Reacquired
            ],
            "exactly one release/reacquire surrounds each actual physical block"
        );
    }
    assert_eq!(count.load(Ordering::SeqCst), 1);
    let value = main
        .eval_str("(list (thread-join s2-main-worker) s2-main-cleanup s2-service-binding)")
        .unwrap();
    assert_eq!(
        print_value(&value),
        "(((\"native-worker-root\" 73) 37 37) 1 10)"
    );
    main.with_green_runtime(|runtime| assert!(runtime.parked.borrow().is_empty()));
}

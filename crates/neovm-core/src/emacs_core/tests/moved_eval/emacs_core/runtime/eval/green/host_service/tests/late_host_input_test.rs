//! A native wait keeps the live worker's bindings, cleanup, and roots when
//! host input arrives late. HostInputPublisher lives in the system crate, so
//! this test cannot stay in eval. The assertions are unchanged.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use neovm_eval::test_support::authority::{
    ObservedMainNativeWait, finish_host_fixture_gc_for_test, green_park_runnable_for_test,
    install_host_fixture_completion_for_test, main_native_wait_events_for_test,
    parked_contains_main_for_test, parked_owner_len_for_test, parked_owners_empty_for_test,
    start_green_threads_for_test,
};
use neovm_value::error::EvalResult;
use neovm_value::intern::intern;
use neovm_value::value::Value;

use crate::emacs_core::eval::keyboard::{InputEvent, KeyEvent};
use crate::emacs_core::eval::{
    push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::{SubrSpec, world_subr};
use crate::host_input::HostInputPublisher;

fn binding(context: &crate::Context) -> Value {
    context
        .special_variable_value_by_id(intern("s2-service-binding"))
        .unwrap()
}

/// A native Rust continuation retains its scratch graph and dynamic binding
/// while main attaches the host. No Context moves between OS threads.
fn main_native_live_worker(context: &mut crate::Context) -> EvalResult {
    let _scratch = SavedNativeScratch(save_scratch_gc_roots());
    let graph = Value::list(vec![Value::string("native-worker-root"), Value::fixnum(73)]);
    push_scratch_gc_roots(&[graph]);
    let worker = std::thread::current().id();
    let original = binding(context);
    green_park_runnable_for_test(context)?;
    assert_eq!(std::thread::current().id(), worker);
    assert_eq!(binding(context), original);
    assert_eq!(print_value(&graph), "(\"native-worker-root\" 73)");
    assert!(parked_contains_main_for_test(context));
    context.gc_collect_exact();
    finish_host_fixture_gc_for_test(context)?;
    Ok(Value::list(vec![graph, original, binding(context)]))
}

struct SavedNativeScratch(usize);
impl Drop for SavedNativeScratch {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn green() -> crate::Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut context = crate::test_utils::new_context();
    context.set_green_threading_mode_at_construction_for_test();
    context.register_subr(world_subr!(SubrSpec::fixed0(
        "s2-main-native-worker",
        main_native_live_worker
    )));
    context.eval_str("(defvar s2-service-binding 10)").unwrap();
    start_green_threads_for_test(&mut context).unwrap();
    context
}

#[test]
fn late_host_input_native_wait_preserves_live_worker_bindings_cleanup_and_roots() {
    let mut main = green();
    let main_os = std::thread::current().id();
    main.eval_str("(defvar s2-main-cleanup 0) (setq s2-main-worker (make-thread (lambda () (let ((s2-service-binding 37)) (unwind-protect (s2-main-native-worker) (setq s2-main-cleanup (1+ s2-main-cleanup))))))) (thread-yield)").unwrap();
    assert_eq!(
        parked_owner_len_for_test(&main),
        1,
        "live worker is canonically rooted, not already exited"
    );
    // Collect while the original worker's Rust continuation is parked.
    main.gc_collect_exact();
    let publisher = HostInputPublisher::install(&mut main);
    publisher
        .send(InputEvent::key_press(KeyEvent::char('x')))
        .unwrap();
    let before = main_native_wait_events_for_test(&main);
    assert_eq!(
        main.eval_str("(read-event nil nil 10)").unwrap(),
        Value::fixnum('x' as i64)
    );
    assert_eq!(
        main_native_wait_events_for_test(&main),
        before,
        "queued input cannot fabricate a physical release"
    );
    let _scratch = SavedNativeScratch(save_scratch_gc_roots());
    let graph = Value::list(vec![Value::string("native-main-root"), Value::fixnum(91)]);
    push_scratch_gc_roots(&[graph]);
    let (completed, completion) = std::sync::mpsc::channel();
    let count = Arc::new(AtomicUsize::new(0));
    install_host_fixture_completion_for_test(&main, completed, Arc::clone(&count));
    let producer = std::thread::spawn(move || {
        let worker = completion
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        assert_ne!(worker, main_os);
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
    let after = main_native_wait_events_for_test(&main);
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
                ObservedMainNativeWait::Released,
                ObservedMainNativeWait::PhysicalBlockEntered,
                ObservedMainNativeWait::PhysicalBlockReturned,
                ObservedMainNativeWait::Reacquired
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
    assert!(parked_owners_empty_for_test(&main));
}

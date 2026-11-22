use super::Context;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct NativeFinalizerProbe {
    count: AtomicUsize,
    owner: std::thread::ThreadId,
    ran_on_owner: AtomicBool,
}

unsafe extern "C" fn count_owner_finalizer(data: *mut std::ffi::c_void) {
    // SAFETY: this fixture registers a live probe for the duration of its
    // consuming, synchronous Context shutdown. No other callback uses it.
    let probe = unsafe { &*data.cast::<NativeFinalizerProbe>() };
    probe.ran_on_owner.store(
        std::thread::current().id() == probe.owner,
        Ordering::Relaxed,
    );
    probe.count.fetch_add(1, Ordering::Relaxed);
}

#[test]
fn gc_context_shutdown_finalizes_native_resources_once_on_owner() {
    let probe = NativeFinalizerProbe {
        count: AtomicUsize::new(0),
        owner: std::thread::current().id(),
        ran_on_owner: AtomicBool::new(false),
    };
    let mut context = Box::new(Context::new(crate::test_utils::unit_runtime()));
    context.setup_thread_locals();
    let data = std::ptr::from_ref(&probe).cast_mut().cast();
    context
        .tagged_heap
        .alloc_user_ptr(data, Some(count_owner_finalizer));
    assert_eq!(probe.count.load(Ordering::Relaxed), 0);

    context.shutdown().unwrap();

    assert_eq!(probe.count.load(Ordering::Relaxed), 1);
    assert!(probe.ran_on_owner.load(Ordering::Relaxed));
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

/// E3.2: a stopped sweep may defer a native finalizer, but the drain must run
/// before `(garbage-collect)` returns to Lisp (GNU runs the finalizer inside
/// `garbage_collect`). The probe effect is visible immediately on return, and
/// no batch is left pending.
#[test]
fn gc_drains_a_native_deferral_before_garbage_collect_returns() {
    let probe = NativeFinalizerProbe {
        count: AtomicUsize::new(0),
        owner: std::thread::current().id(),
        ran_on_owner: AtomicBool::new(false),
    };
    let mut context = Box::new(Context::new(crate::test_utils::unit_runtime()));
    context.setup_thread_locals();
    let data = std::ptr::from_ref(&probe).cast_mut().cast();
    context
        .tagged_heap
        .alloc_user_ptr(data, Some(count_owner_finalizer));
    // SAFETY: test-only owner-thread heap setup through the non-swappable
    // mutable accessor contract; the view is used in place.
    unsafe { context.tagged_heap_mut() }.begin_native_deferral_for_test();
    context
        .eval_str("(garbage-collect)")
        .expect("explicit collection");
    assert_eq!(
        probe.count.load(Ordering::Relaxed),
        1,
        "the deferred finalizer must have run before garbage-collect returned"
    );
    assert!(probe.ran_on_owner.load(Ordering::Relaxed));
    assert_eq!(
        context.tagged_heap.pending_native_destroy_count_for_test(),
        None,
        "the completed-cycle drain must consume the batch"
    );
}

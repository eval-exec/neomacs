use super::*;
use crate::emacs_core::eval::Context;

fn roots_for(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_in_flight_flow_gc_roots(&mut roots, ctx.tagged_heap.identity());
    roots
}

#[test]
fn gc_tls_ownership_in_flight_excludes_a_dropped_heap() {
    let (flow, payload_bits) = {
        let mut first = Context::new();
        let payload = first.eval_str("(vector 42)").unwrap();
        (
            Flow::throw(Value::symbol("gc-tls-throw"), payload),
            payload.bits(),
        )
    };
    let mut next = Context::new();
    assert!(
        roots_for(&next)
            .iter()
            .all(|root| root.bits() != payload_bits),
        "an in-flight throw retained a dropped heap's payload"
    );
    next.gc_collect_exact();
    drop(flow);
}

#[test]
fn gc_tls_ownership_in_flight_filters_two_live_heaps() {
    let mut first = Context::new();
    let a = first.eval_str("(vector 41)").unwrap();
    let flow_a = Flow::throw(Value::symbol("gc-tls-first"), a);
    let mut second = Context::new();
    let b = second.eval_str("(vector 42)").unwrap();
    let flow_b = Flow::throw(Value::symbol("gc-tls-second"), b);
    let roots = roots_for(&second);
    assert!(roots.iter().any(|root| root.bits() == b.bits()));
    assert!(roots.iter().all(|root| root.bits() != a.bits()));
    second.gc_collect_exact();
    first.setup_thread_locals();
    let roots = roots_for(&first);
    assert!(roots.iter().any(|root| root.bits() == a.bits()));
    assert!(roots.iter().all(|root| root.bits() != b.bits()));
    first.gc_collect_exact();
    assert_eq!(a.as_vector_data().unwrap()[0], Value::fixnum(41));
    assert_eq!(b.as_vector_data().unwrap()[0], Value::fixnum(42));
    drop((flow_a, flow_b));
}

#[test]
fn gc_tls_ownership_in_flight_clone_keeps_the_original_heap() {
    let mut first = Context::new();
    let payload = first.eval_str("(vector 43)").unwrap();
    let original = Flow::throw(Value::symbol("gc-tls-clone"), payload);
    let mut second = Context::new();
    let cloned = original.clone();
    drop(original);
    assert!(
        roots_for(&second)
            .iter()
            .all(|root| root.bits() != payload.bits())
    );
    second.gc_collect_exact();
    first.setup_thread_locals();
    assert!(
        roots_for(&first)
            .iter()
            .any(|root| root.bits() == payload.bits())
    );
    first.gc_collect_exact();
    assert_eq!(payload.as_vector_data().unwrap()[0], Value::fixnum(43));
    drop(cloned);
    assert!(
        roots_for(&first)
            .iter()
            .all(|root| root.bits() != payload.bits())
    );
}

#[test]
fn gc_tls_ownership_in_flight_signal_roots_survive_same_heap_gc() {
    let mut ctx = Context::new();
    let payload = ctx.eval_str("(vector 44)").unwrap();
    let flow = signal(LispCondition::Error, vec![payload]);
    assert!(
        roots_for(&ctx)
            .iter()
            .any(|root| root.bits() == payload.bits())
    );
    ctx.gc_collect_exact();
    assert_eq!(payload.as_vector_data().unwrap()[0], Value::fixnum(44));
    drop(flow);
    assert!(
        roots_for(&ctx)
            .iter()
            .all(|root| root.bits() != payload.bits())
    );
}

/// Identifies an existing pinned slot; the originating error stays live until
/// the worker acknowledges that it claimed its own local pin from this slot.
#[derive(Clone, Copy)]
struct InFlightSlotId(usize);

static_assertions::assert_impl_all!(InFlightSlotId: Send, Sync, Copy);

fn pinned_slot(error: &EvalError) -> (InFlightRegistryHandle, InFlightSlotId) {
    let EvalError::Signal { pin, .. } = error else {
        panic!("expected a pinned signal");
    };
    let pin = pin
        .in_flight_roots()
        .pin
        .as_ref()
        .expect("signal has traceable payload");
    (pin.registry.clone(), InFlightSlotId(pin.slot))
}

fn claim_registry_slot(registry: InFlightRegistryHandle, slot: InFlightSlotId) -> InFlightRoots {
    // The source error remains alive until the receiver claims this pin.
    // Copy only the mutex-protected root words; no Lisp object is dereferenced.
    let values = registry.lock().slots[slot.0]
        .as_ref()
        .expect("acknowledgement keeps the source slot live")
        .clone();
    InFlightRoots::claim(registry, values)
}

#[test]
fn gc_tls_ownership_in_flight_shared_registry_pin_outlives_owner_local_error() {
    let mut first = Context::new();
    let error = first
        .eval_str("(signal 'error (list (vector 46)))")
        .expect_err("retain a public error on its owner thread");
    let payload_bits = match &error {
        EvalError::Signal { data, .. } => data[0].bits(),
        other => panic!("expected signal, received {other:?}"),
    };
    let (registry, slot) = pinned_slot(&error);
    let (claimed, wait_for_claim) = std::sync::mpsc::channel();
    let (release, wait_for_release) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        // Neither Context nor EvalError crosses threads. The shared registry
        // and scalar slot id let this thread own a separate local root pin.
        let pin = claim_registry_slot(registry, slot);
        claimed.send(()).unwrap();
        wait_for_release.recv().unwrap();
        drop(pin);
    });
    wait_for_claim.recv().unwrap();
    drop(error);
    first.gc_collect_exact();
    assert!(
        roots_for(&first)
            .iter()
            .any(|root| root.bits() == payload_bits)
    );

    let mut second = Context::new();
    let other = second
        .eval_str("(signal 'error (list (vector 47)))")
        .expect_err("pin an error in the independent Context");
    let other_payload = match &other {
        EvalError::Signal { data, .. } => data[0],
        other => panic!("expected signal, received {other:?}"),
    };
    assert!(
        roots_for(&second)
            .iter()
            .all(|root| root.bits() != payload_bits)
    );
    second.gc_collect_exact();
    assert_eq!(
        other_payload.as_vector_data().unwrap()[0],
        Value::fixnum(47)
    );
    release.send(()).unwrap();
    worker
        .join()
        .expect("release the worker's local registry pin");
    first.setup_thread_locals();
    assert!(
        roots_for(&first)
            .iter()
            .all(|root| root.bits() != payload_bits)
    );
    first.gc_collect_exact();
    drop(other);
}

#[test]
fn gc_tls_ownership_in_flight_registry_pins_change_during_owner_gc() {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};

    let mut first = Context::new();
    let error = first
        .eval_str("(signal 'error (list (vector 48)))")
        .expect_err("retain a public error on its owner thread");
    let payload_bits = match &error {
        EvalError::Signal { data, .. } => data[0].bits(),
        other => panic!("expected signal, received {other:?}"),
    };
    let (registry, slot) = pinned_slot(&error);
    let collecting = Arc::new(Barrier::new(2));
    let finished = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicUsize::new(0));
    let worker_collecting = Arc::clone(&collecting);
    let worker_finished = Arc::clone(&finished);
    let worker_completed = Arc::clone(&completed);
    let worker = std::thread::spawn(move || {
        let pin = claim_registry_slot(registry, slot);
        worker_collecting.wait();
        let mut clones = 0;
        while clones < 512 || worker_completed.load(Ordering::Acquire) < 8 {
            drop(pin.clone());
            clones += 1;
            if clones % 64 == 0 {
                std::thread::yield_now();
            }
        }
        drop(pin);
        worker_finished.store(true, Ordering::Release);
    });
    let mut second = Context::new();
    let other = second
        .eval_str("(signal 'error (list (vector 49)))")
        .expect_err("retain a separate pin in the active Context");
    let other_payload = match &other {
        EvalError::Signal { data, .. } => data[0],
        other => panic!("expected signal, received {other:?}"),
    };
    collecting.wait();
    drop(error);
    let mut collections = 0;
    while collections < 8 || !finished.load(Ordering::Acquire) {
        first.gc_collect_exact();
        collections += 1;
        completed.store(collections, Ordering::Release);
    }
    worker
        .join()
        .expect("clone and release registry pins during owner GC");
    first.setup_thread_locals();
    assert!(
        roots_for(&first)
            .iter()
            .all(|root| root.bits() != payload_bits)
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    assert!(
        roots_for(&second)
            .iter()
            .any(|root| root.bits() == other_payload.bits())
    );
    second.gc_collect_exact();
    assert_eq!(
        other_payload.as_vector_data().unwrap()[0],
        Value::fixnum(49)
    );
    drop(other);
}

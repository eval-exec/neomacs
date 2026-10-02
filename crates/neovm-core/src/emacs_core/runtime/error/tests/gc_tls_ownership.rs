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

#[test]
fn gc_tls_ownership_in_flight_roots_follow_a_context_to_another_thread() {
    let mut first = Context::new();
    let error = first
        .eval_str("(signal 'error (list (vector 46)))")
        .expect_err("retain a public error on the source thread");
    let payload = match &error {
        EvalError::Signal { data, .. } => data[0],
        other => panic!("expected signal, received {other:?}"),
    };
    let payload_bits = payload.bits();
    let (ready, proceed) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        proceed.recv().unwrap();
        first.setup_thread_locals();
        assert!(
            roots_for(&first)
                .iter()
                .any(|root| root.bits() == payload_bits),
            "moving the Context lost an error still retained by its source thread"
        );
        first.gc_collect_exact();
        first
    });
    let mut second = Context::new();
    ready.send(()).unwrap();
    let mut first = worker.join().expect("collect on the destination thread");
    assert_eq!(payload.as_vector_data().unwrap()[0], Value::fixnum(46));
    second.setup_thread_locals();
    let other = second
        .eval_str("(signal 'error (list (vector 47)))")
        .expect_err("pin an error in the source thread's new Context");
    let other_payload = match &other {
        EvalError::Signal { data, .. } => data[0],
        other => panic!("expected signal, received {other:?}"),
    };
    drop(error);
    assert!(
        roots_for(&second)
            .iter()
            .any(|root| root.bits() == other_payload.bits())
    );
    second.gc_collect_exact();
    assert_eq!(
        other_payload.as_vector_data().unwrap()[0],
        Value::fixnum(47)
    );
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
fn gc_tls_ownership_in_flight_source_pins_change_during_worker_gc() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier, mpsc};

    let mut first = Context::new();
    let error = first
        .eval_str("(signal 'error (list (vector 48)))")
        .expect_err("retain a public error on the source thread");
    let payload_bits = match &error {
        EvalError::Signal { data, .. } => data[0].bits(),
        other => panic!("expected signal, received {other:?}"),
    };
    let (start, wait_for_start) = mpsc::channel();
    let (ready, wait_for_ready) = mpsc::channel();
    let collecting = Arc::new(Barrier::new(2));
    let finished = Arc::new(AtomicBool::new(false));
    let worker_collecting = Arc::clone(&collecting);
    let worker_finished = Arc::clone(&finished);
    let worker = std::thread::spawn(move || {
        wait_for_start.recv().unwrap();
        first.setup_thread_locals();
        assert!(
            roots_for(&first)
                .iter()
                .any(|root| root.bits() == payload_bits)
        );
        ready.send(()).unwrap();
        worker_collecting.wait();
        let mut collections = 0;
        while collections < 8 || !worker_finished.load(Ordering::Acquire) {
            first.gc_collect_exact();
            collections += 1;
        }
        assert!(
            roots_for(&first)
                .iter()
                .all(|root| root.bits() != payload_bits)
        );
        first.gc_collect_exact();
        first
    });
    let mut second = Context::new();
    let other = second
        .eval_str("(signal 'error (list (vector 49)))")
        .expect_err("retain a separate pin in the source thread's active Context");
    let other_payload = match &other {
        EvalError::Signal { data, .. } => data[0],
        other => panic!("expected signal, received {other:?}"),
    };
    start.send(()).unwrap();
    wait_for_ready.recv().unwrap();
    collecting.wait();
    for _ in 0..512 {
        drop(error.clone());
    }
    drop(error);
    finished.store(true, Ordering::Release);
    let mut first = worker
        .join()
        .expect("collect while source pins clone and drop");
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
    first.setup_thread_locals();
    assert!(
        roots_for(&first)
            .iter()
            .all(|root| root.bits() != payload_bits)
    );
    first.gc_collect_exact();
    drop(other);
}

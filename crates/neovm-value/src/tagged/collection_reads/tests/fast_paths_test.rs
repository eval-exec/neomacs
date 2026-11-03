use super::*;

#[test]
fn first_capture_keeps_writes_after_scope_exit() {
    let source = Value::cons(Value::NIL, Value::NIL);
    // Initialization before any certificate exists need not consume history.
    source.set_car(Value::T);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.unwrap();
    assert!(!is_active());
    source.set_car(Value::NIL);
    assert!(!reads.unchanged());
    let (_, next) = capture(|| source.cons_car());
    assert!(next.unwrap().unchanged());
}

#[test]
fn concurrent_capture_exit_preserves_this_mutators_observations() {
    let entered = std::sync::Barrier::new(2);
    let leaving = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        let worker = threads.spawn(|| {
            capture(|| {
                entered.wait();
                leaving.wait();
            });
        });
        let source = Value::cons(Value::NIL, Value::NIL);
        let (_, reads) = capture(|| {
            entered.wait();
            assert!(is_active());
            assert!(reads_need_observation());
            leaving.wait();
            worker.join().unwrap();
            assert!(is_active());
            assert!(reads_need_observation());
            source.cons_car();
        });
        source.set_car(Value::T);
        assert!(!reads.unwrap().unchanged());
    });
}

#[test]
fn other_mutators_capture_does_not_observe_this_threads_reads() {
    let entered = std::sync::Barrier::new(2);
    let leaving = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        threads.spawn(|| {
            capture(|| {
                entered.wait();
                leaving.wait();
            });
        });
        entered.wait();
        assert!(!is_active());
        assert_eq!(reads_need_observation(), !hoist_reads());
        Value::cons(Value::NIL, Value::NIL).cons_car();
        leaving.wait();
    });
}

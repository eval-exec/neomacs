//! Capture semantics at the fused setter's journal/projection boundary.

use super::*;
use crate::emacs_core::Value;
use crate::tagged::mutate::{set_cons_car, set_cons_cdr, set_record_slot, set_vector_slot};

type Setter = fn(Value, Value) -> bool;

fn vector_slot(owner: Value, item: Value) -> bool {
    set_vector_slot(owner, 0, item)
}

fn record_slot(owner: Value, item: Value) -> bool {
    set_record_slot(owner, 0, item)
}

fn journal_policy() -> JournalPolicy {
    STATE.with(|state| state.borrow().journal_policy)
}

#[test]
fn fused_setter_first_read_follows_its_journal_write() {
    let cases: [(Value, Setter); 4] = [
        (Value::cons(Value::NIL, Value::NIL), set_cons_car),
        (Value::cons(Value::NIL, Value::NIL), set_cons_cdr),
        (Value::vector(vec![Value::NIL]), vector_slot),
        (Value::make_record(vec![Value::NIL]), record_slot),
    ];
    for (owner, setter) in cases {
        let (_, reads) = capture(|| assert!(setter(owner, Value::T)));
        let reads = reads.expect("a first setter projection follows its own write");
        assert!(reads.reads.contains(&owner.bits()));
        assert!(reads.unchanged());
        assert_eq!(journal_policy(), JournalPolicy::JournalOnly);
        assert!(setter(owner, Value::NIL));
        assert!(!reads.unchanged());

        let (_, invalid) = capture(|| {
            owner.heap_ptr();
            assert!(setter(owner, Value::T));
        });
        assert!(
            invalid.is_none(),
            "a setter cannot advance an earlier first read"
        );
    }
}

#[test]
fn fused_setter_projects_newly_linked_cons_dependencies() {
    let first = Value::cons(Value::NIL, Value::NIL);
    let second = Value::cons(Value::T, Value::NIL);
    let (_, reads) = capture(|| assert!(set_cons_cdr(first, second)));
    let reads = reads.unwrap();
    assert!(reads.reads.contains(&first.bits()));
    assert!(
        !reads.reads.contains(&second.bits()),
        "the stored child is not projected"
    );
    assert!(set_cons_car(second, Value::NIL));
    assert!(reads.unchanged());
    assert!(set_cons_cdr(first, Value::NIL));
    assert!(!reads.unchanged());
}

#[test]
fn failed_wrong_tags_journal_without_adding_projection_dependencies() {
    let vector = Value::vector(vec![Value::NIL]);
    let cons = Value::cons(Value::NIL, Value::NIL);
    let string = Value::string("x");
    let failures: [(Value, Setter); 6] = [
        (vector, set_cons_car),
        (string, set_cons_cdr),
        (cons, vector_slot),
        (string, record_slot),
        (Value::fixnum(3), vector_slot),
        (Value::NIL, set_cons_car),
    ];
    for (owner, setter) in failures {
        let (_, before) = capture(|| owner.heap_ptr());
        let before = before.unwrap();
        let revision = LispCollectionRevision::current().sequence();
        let (_, failed) = capture(|| assert!(!setter(owner, Value::T)));
        let failed = failed.unwrap();
        assert!(failed.reads.is_empty());
        assert_eq!(
            LispCollectionRevision::current().sequence(),
            revision.wrapping_add(1)
        );
        if owner.is_heap_object() {
            assert!(
                !before.unchanged(),
                "failed calls retain conservative journaling"
            );
        }
        assert!(!setter(owner, Value::NIL));
        assert!(
            failed.unchanged(),
            "a rejected tag adds no pointer dependency"
        );
    }
}

#[test]
fn failed_veclike_subtypes_still_observe_their_header_after_journaling() {
    let vector = Value::vector(vec![Value::NIL]);
    let record = Value::make_record(vec![Value::NIL]);
    let cases: [(Value, Setter, Setter); 2] = [
        (vector, record_slot, vector_slot),
        (record, vector_slot, record_slot),
    ];
    for (owner, rejected, accepted) in cases {
        let (_, before) = capture(|| owner.heap_ptr());
        let (_, failed) = capture(|| assert!(!rejected(owner, Value::T)));
        let failed = failed.expect("the failed subtype's first header read follows journaling");
        assert!(!before.unwrap().unchanged());
        assert!(failed.reads.contains(&owner.bits()));
        assert!(failed.unchanged());
        assert!(accepted(owner, Value::T));
        assert!(!failed.unchanged());
    }
}

#[test]
fn out_of_bounds_and_same_value_calls_keep_write_history() {
    let cases: [(Value, fn(Value, usize, Value) -> bool); 2] = [
        (Value::vector(vec![Value::NIL]), set_vector_slot),
        (Value::make_record(vec![Value::NIL]), set_record_slot),
    ];
    for (owner, setter) in cases {
        let (_, before) = capture(|| owner.heap_ptr());
        let (_, failed) = capture(|| assert!(!setter(owner, 1, Value::T)));
        let failed = failed.unwrap();
        assert!(!before.unwrap().unchanged());
        assert!(failed.reads.contains(&owner.bits()));
        assert!(failed.unchanged());
        assert!(setter(owner, 0, Value::NIL));
        assert!(!failed.unchanged(), "same-value writes remain journaled");
        let (_, invalid) = capture(|| {
            owner.heap_ptr();
            assert!(!setter(owner, usize::MAX, Value::T));
        });
        assert!(invalid.is_none());
    }
}

#[test]
fn nested_fused_setters_preserve_each_scopes_first_read_and_recent_probe() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let (_, outer) = capture(|| {
        OBSERVATION_STATE_ACCESSES.with(|count| count.set(0));
        assert!(set_cons_car(source, Value::T));
        assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 1);
        source.cons_car();
        assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 1);
        let (_, inner) = capture(|| {
            assert_eq!(journal_policy(), JournalPolicy::JournalAndObserve);
            assert!(set_cons_cdr(source, Value::T));
            source.cons_cdr();
            assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 2);
        });
        let inner = inner.unwrap();
        assert!(inner.reads.contains(&source.bits()));
        assert!(inner.unchanged());
        assert_eq!(journal_policy(), JournalPolicy::JournalAndObserve);
        assert!(set_cons_car(source, Value::NIL));
        assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 3);
        assert!(!inner.unchanged());
    });
    assert!(outer.is_none());
    assert_eq!(journal_policy(), JournalPolicy::JournalOnly);
}

#[test]
fn normalized_fused_setters_rebase_only_the_inner_capture() {
    let mut source = Value::vector(vec![Value::NIL]);
    let (_, outer) = capture(|| {
        source.as_vector_data();
        let (_, normalized) = capture_normalized(
            &mut source,
            |source| assert!(set_vector_slot(*source, 0, Value::T)),
            |_| (),
        );
        let normalized = normalized.unwrap();
        assert!(normalized.reads.contains(&source.bits()));
        assert!(normalized.unchanged());
    });
    assert!(outer.is_none());
    let (_, normalized) = capture_normalized(
        &mut source,
        |source| assert!(set_vector_slot(*source, 0, Value::NIL)),
        |_| (),
    );
    assert!(set_vector_slot(source, 0, Value::T));
    assert!(!normalized.unwrap().unchanged());
}

#[test]
fn fused_write_budgets_and_ring_history_remain_bounded() {
    let values: Vec<_> = (0..=MAX_READS)
        .map(|_| Value::cons(Value::NIL, Value::NIL))
        .collect();
    let (_, reads) = capture_normalized(
        &mut (),
        |_| {
            for value in &values {
                assert!(set_cons_car(*value, Value::T));
            }
        },
        |_| (),
    );
    assert!(
        reads.is_none(),
        "normalization cannot clear a fused read-budget overflow"
    );

    let source = values[0];
    let unrelated = values[1];
    let (_, reads) = capture(|| assert!(set_cons_cdr(source, Value::T)));
    let reads = reads.unwrap();
    for _ in 0..=JOURNAL_SIZE {
        assert!(set_cons_car(unrelated, Value::NIL));
    }
    assert!(!reads.unchanged());
    let (_, reads) = capture(|| assert!(set_cons_cdr(source, Value::NIL)));
    assert!(reads.unwrap().unchanged());
}

#[test]
fn fused_write_policy_survives_depth_rejection_and_panic_unwind() {
    fn nested(source: Value, depth: usize) {
        let (_, reads) = capture(|| {
            assert_eq!(journal_policy(), JournalPolicy::JournalAndObserve);
            assert!(set_cons_car(source, Value::T));
            if depth > 0 {
                nested(source, depth - 1);
            }
        });
        assert!(reads.is_none());
    }
    let source = Value::cons(Value::NIL, Value::NIL);
    nested(source, MAX_DEPTH + 2);
    assert!(!ACTIVE.with(Cell::get));
    assert_eq!(journal_policy(), JournalPolicy::JournalOnly);

    let (_, outer) = capture(|| {
        source.cons_car();
        let unwind = std::panic::catch_unwind(|| {
            capture(|| {
                assert!(set_cons_cdr(source, Value::T));
                panic!("unwind fused projection");
            });
        });
        assert!(unwind.is_err());
        assert!(ACTIVE.with(Cell::get));
        assert_eq!(journal_policy(), JournalPolicy::JournalAndObserve);
    });
    assert!(outer.is_none());
    assert!(!ACTIVE.with(Cell::get));
    assert_eq!(journal_policy(), JournalPolicy::JournalOnly);
    let (_, reads) = capture(|| assert!(set_cons_car(source, Value::NIL)));
    assert!(reads.unwrap().unchanged());
}

#[test]
fn lazy_prefix_observes_another_mutators_monotonic_history_transition() {
    // Run this test in an isolated nextest process. CONFIG is a LazyLock, so
    // WRITE_LAZY is inherited at launch; never change the environment in-test.
    initialize();
    let lazy = std::env::var("NEOVM_COLLECTION_WRITE_LAZY").as_deref() == Ok("on");
    let source = Value::cons(Value::NIL, Value::NIL);
    assert_eq!(
        journal_policy(),
        if lazy {
            JournalPolicy::BeforeFirstCapture
        } else {
            JournalPolicy::JournalOnly
        }
    );
    let before = LispCollectionRevision::current().sequence();
    assert!(set_cons_car(source, Value::T));
    assert_eq!(
        LispCollectionRevision::current().sequence(),
        before.wrapping_add(u64::from(!lazy))
    );

    let entered = std::sync::Barrier::new(2);
    let leaving = std::sync::Barrier::new(2);
    std::thread::scope(|threads| {
        let worker = threads.spawn(|| {
            capture(|| {
                entered.wait();
                leaving.wait();
            });
            assert_eq!(journal_policy(), JournalPolicy::JournalOnly);
        });
        entered.wait();
        assert!(!ACTIVE.with(Cell::get));
        assert!(set_cons_cdr(source, Value::T));
        assert_eq!(
            LispCollectionRevision::current().sequence(),
            before.wrapping_add(u64::from(!lazy) + 1)
        );
        STATE.with(|state| {
            let slot = LispCollectionRevision::current().sequence() as usize % JOURNAL_SIZE;
            assert_eq!(state.borrow().writes[slot], source.bits());
        });
        assert_eq!(journal_policy(), JournalPolicy::JournalOnly);
        leaving.wait();
        worker.join().unwrap();
    });
    assert!(set_cons_car(source, Value::NIL));
    assert_eq!(
        LispCollectionRevision::current().sequence(),
        before.wrapping_add(u64::from(!lazy) + 2)
    );
    assert_eq!(journal_policy(), JournalPolicy::JournalOnly);
    std::thread::spawn(|| {
        assert_eq!(
            journal_policy(),
            JournalPolicy::JournalOnly,
            "new mutators retain process history"
        );
    })
    .join()
    .unwrap();
}

#[test]
fn own_capture_upgrades_lazy_prefix_and_never_returns_to_skipping_writes() {
    initialize();
    let lazy = std::env::var("NEOVM_COLLECTION_WRITE_LAZY").as_deref() == Ok("on");
    let source = Value::make_record(vec![Value::NIL]);
    assert!(set_record_slot(source, 0, Value::T));
    if lazy {
        assert_eq!(journal_policy(), JournalPolicy::BeforeFirstCapture);
    }
    let (_, reads) = capture(|| {
        assert_eq!(journal_policy(), JournalPolicy::JournalAndObserve);
        assert!(set_record_slot(source, 0, Value::NIL));
    });
    let reads = reads.unwrap();
    assert!(reads.reads.contains(&source.bits()));
    assert_eq!(journal_policy(), JournalPolicy::JournalOnly);
    assert!(set_record_slot(source, 0, Value::T));
    assert!(!reads.unchanged());
}

#[test]
fn generic_changed_keeps_journaling_separate_from_pointer_observation() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| {
        OBSERVATION_STATE_ACCESSES.with(|count| count.set(0));
        LispCollectionRevision::changed(source);
        assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 0);
    });
    let reads = reads.unwrap();
    assert!(reads.reads.is_empty());
    assert!(set_cons_car(source, Value::T));
    assert!(reads.unchanged());
    let (_, invalid) = capture(|| {
        source.cons_car();
        LispCollectionRevision::changed(source);
    });
    assert!(invalid.is_none());
}

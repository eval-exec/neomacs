//! The original certificate remains authoritative while repeated consumers
//! at one owner journal sequence share the same validation result.
use super::*;
use crate::value::Value;

#[test]
fn repeated_certificate_validation_scans_once_per_owner_sequence() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.expect("source certificate");
    assert_eq!(reads.full_validation_invocations_for_test(), 0);
    // Model the unrelated command/frame writes between source capture and
    // the many consumers of its retained source proof.
    for i in 0..64 {
        unrelated.set_car(Value::fixnum(i));
    }
    for _ in 0..128 {
        assert!(reads.unchanged());
    }
    assert_eq!(
        reads.full_validation_invocations_for_test(),
        1,
        "unchanged consumers must share one full validation at this sequence"
    );

    unrelated.set_car(Value::T);
    for _ in 0..128 {
        assert!(reads.unchanged());
    }
    assert_eq!(
        reads.full_validation_invocations_for_test(),
        2,
        "one unrelated write requires exactly one additional validation"
    );

    source.set_car(Value::T);
    for _ in 0..128 {
        assert!(!reads.unchanged());
    }
    assert_eq!(
        reads.full_validation_invocations_for_test(),
        3,
        "the original dependency's mutation must reject, then reuse that rejection"
    );
}

#[test]
fn repeated_validation_never_rebases_original_journal_history() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.expect("source certificate");
    let original = reads.reads.dependencies().expect("live lease").1;
    assert!(reads.unchanged());
    for i in 0..JOURNAL_SIZE / 2 {
        unrelated.set_car(Value::fixnum(i as i64));
    }
    assert!(reads.unchanged());
    for i in JOURNAL_SIZE / 2..JOURNAL_SIZE {
        unrelated.set_car(Value::fixnum(i as i64));
    }
    assert!(reads.unchanged(), "the original 8192 writes still fit");
    unrelated.set_car(Value::fixnum(JOURNAL_SIZE as i64));
    for _ in 0..8 {
        assert!(
            !reads.unchanged(),
            "8193 writes lose the original history despite successful intermediate checks"
        );
    }
    assert_eq!(reads.reads.dependencies().expect("live lease").1, original);
    assert_eq!(reads.full_validation_invocations_for_test(), 4);
    assert_eq!(
        reads.journal_slots_scanned_for_test(),
        JOURNAL_SIZE,
        "incremental validation must scan each journal slot at most once (0->4096->8192 = 8192, not 12288)"
    );
}

#[test]
fn incremental_validation_scans_only_new_journal_delta_and_latches_invalidation() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.expect("source certificate");

    const STEPS: usize = 256;
    for i in 0..STEPS {
        unrelated.set_car(Value::fixnum(i as i64));
        assert!(reads.unchanged());
    }
    assert_eq!(
        reads.journal_slots_scanned_for_test(),
        STEPS,
        "validating after every unrelated write must scan O(N) total slots, not O(N^2)"
    );

    // Mutating the dependency invalidates the certificate on the next check
    // (scanning only the 1 new slot), and invalidation is terminal even across
    // subsequent unrelated writes.
    source.set_car(Value::T);
    assert!(!reads.unchanged());
    assert_eq!(reads.journal_slots_scanned_for_test(), STEPS + 1);
    let invocations_at_invalidation = reads.full_validation_invocations_for_test();

    for i in 0..16 {
        unrelated.set_car(Value::fixnum((STEPS + i) as i64));
        assert!(!reads.unchanged());
    }
    assert_eq!(
        reads.journal_slots_scanned_for_test(),
        STEPS + 1,
        "terminal invalidation must never re-scan later journal writes"
    );
    assert_eq!(
        reads.full_validation_invocations_for_test(),
        invocations_at_invalidation
    );
}

#[test]
fn warmed_validation_still_observes_dependencies_in_each_outer_capture() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, inner) = capture(|| source.cons_car());
    let inner = inner.expect("inner source certificate");
    unrelated.set_car(Value::T);
    assert!(inner.unchanged());
    for _ in 0..3 {
        let (_, outer) = capture(|| {
            for _ in 0..8 {
                assert!(inner.unchanged_and_observe());
            }
        });
        let outer = outer.expect("transitive outer certificate");
        assert!(outer.contains_owner_for_test(source.bits()));
        assert!(outer.unchanged());
    }
    assert_eq!(
        inner.full_validation_invocations_for_test(),
        1,
        "transitive observation must not repeat the full journal validation"
    );
    let (_, outer) = capture(|| assert!(inner.unchanged_and_observe()));
    source.set_car(Value::T);
    assert!(!outer.expect("last transitive certificate").unchanged());
    assert!(!inner.unchanged());
}

#[test]
fn warmed_validation_never_admits_a_foreign_world() {
    use crate::tagged::gc::{TaggedHeap, set_tagged_heap};
    use crate::tagged::value::TaggedValue;

    let mut first = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut first);
    let source = first.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let unrelated = first.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.expect("first World certificate");
    unrelated.set_car(TaggedValue::T);
    assert!(reads.unchanged());
    let original_sequence = LispCollectionRevision::current();

    let mut second = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut second);
    let other = second.alloc_cons(TaggedValue::NIL, TaggedValue::NIL);
    other.set_car(TaggedValue::T);
    assert_eq!(LispCollectionRevision::current(), original_sequence);
    let (_, foreign) = capture(|| {
        for _ in 0..8 {
            assert!(!reads.unchanged_and_observe());
        }
    });
    assert!(foreign.expect("foreign capture").reads_empty_for_test());

    set_tagged_heap(&mut first);
    assert!(reads.unchanged_and_observe());
    assert_eq!(reads.full_validation_invocations_for_test(), 1);
    source.set_car(TaggedValue::NIL);
    assert!(!reads.unchanged());
    assert_eq!(reads.full_validation_invocations_for_test(), 2);
}

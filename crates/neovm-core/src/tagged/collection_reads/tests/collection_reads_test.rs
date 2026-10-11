use super::*;
use crate::emacs_core::Value;

#[test]
fn repeated_reads_skip_capture_state_but_preserve_nested_dependencies() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let (_, outer) = capture(|| {
        OBSERVATION_STATE_ACCESSES.with(|count| count.set(0));
        for _ in 0..1024 {
            source.cons_car();
        }
        assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 1);
        let (_, inner) = capture(|| {
            for _ in 0..1024 {
                source.cons_cdr();
            }
        });
        assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 2);
        source.set_car(Value::T);
        assert!(!inner.unwrap().unchanged());
        source.cons_car();
        assert_eq!(OBSERVATION_STATE_ACCESSES.with(Cell::get), 3);
    });
    assert!(
        outer.is_none(),
        "the first read must still precede the mutation"
    );
}

#[test]
fn observed_cons_survives_unrelated_mutation_but_not_its_own() {
    let source = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| source.cons_cdr().cons_car());
    let reads = reads.unwrap();
    unrelated.set_cdr(Value::T);
    assert!(reads.unchanged());
    source.cons_cdr().set_car(Value::fixnum(3));
    assert!(!reads.unchanged());
}

#[test]
fn nested_capture_and_write_during_observation_are_conservative() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let (_, outer) = capture(|| {
        let (_, inner) = capture(|| source.cons_car());
        assert!(inner.unwrap().unchanged());
    });
    let outer = outer.unwrap();
    source.set_car(Value::T);
    assert!(!outer.unchanged());
    let (_, invalid) = capture(|| {
        source.cons_car();
        source.set_car(Value::NIL);
    });
    assert!(invalid.is_none());
    let (_, fresh) = capture(|| {
        source.set_car(Value::T);
        source.cons_car()
    });
    assert!(fresh.unwrap().unchanged());
}

#[test]
fn nested_cache_hit_propagates_dependencies() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let (_, inner) = capture(|| source.cons_car());
    let inner = inner.unwrap();
    let (_, outer) = capture(|| assert!(inner.unchanged_and_observe()));
    source.set_car(Value::T);
    assert!(!outer.unwrap().unchanged());
}

#[test]
fn normalization_retains_dependencies_without_rebasing_outer_reads() {
    let mut source = Value::cons(Value::NIL, Value::NIL);
    let (_, outer) = capture(|| {
        source.cons_car();
        let (result, inner) = capture_normalized(
            &mut source,
            |source| {
                source.cons_car();
                source.set_car(Value::T);
            },
            |source| source.cons_car(),
        );
        assert_eq!(result, Value::T);
        assert!(inner.unwrap().unchanged());
    });
    assert!(outer.is_none());
    let (_, reads) = capture_normalized(
        &mut source,
        |source| {
            source.cons_car();
            source.set_car(Value::NIL);
        },
        |_| (),
    );
    source.set_car(Value::T);
    assert!(!reads.unwrap().unchanged());
}

#[test]
fn nested_scope_budget_is_bounded_and_recovers() {
    fn nested(depth: usize) {
        let (_, reads) = capture(|| {
            assert!(STATE.with(|s| s.borrow().captures.len()) <= MAX_DEPTH);
            if depth > 0 {
                nested(depth - 1);
            }
        });
        assert!(reads.is_none());
    }
    nested(MAX_DEPTH + 5);
    assert!(!ACTIVE.with(Cell::get));
    assert!(capture(|| ()).1.unwrap().unchanged());
}

#[test]
fn vector_string_and_character_table_mutations_invalidate_reads() {
    let vector = Value::vector(vec![Value::NIL]);
    let string = Value::string("abc");
    let table = Value::make_char_table(Value::NIL, Value::NIL, 0);
    let (_, reads) = capture(|| vector.as_vector_data().unwrap().len());
    vector.set_vector_slot(0, Value::T);
    assert!(!reads.unwrap().unchanged());
    let (_, reads) = capture(|| string.as_str_owned());
    string.set_string_byte_same_char_count(0, b'z');
    assert!(!reads.unwrap().unchanged());
    let (_, reads) = capture(|| table.as_char_table_obj().unwrap().defalt);
    table.with_char_table_mut(|table| table.defalt = Value::T);
    assert!(!reads.unwrap().unchanged());
}

#[test]
fn lost_mutation_history_refuses_reuse() {
    let source = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| source.cons_car());
    let reads = reads.unwrap();
    for _ in 0..=JOURNAL_SIZE {
        unrelated.set_car(Value::T);
    }
    assert!(!reads.unchanged());
}

#[test]
fn observation_budget_and_unwind_restore_the_outer_scope() {
    let values: Vec<_> = (0..=MAX_READS)
        .map(|_| Value::cons(Value::NIL, Value::NIL))
        .collect();
    let (_, reads) = capture(|| {
        for value in &values {
            value.cons_car();
        }
    });
    assert!(reads.is_none());
    let _ = std::panic::catch_unwind(|| capture(|| panic!("unwind")));
    assert!(!ACTIVE.with(Cell::get));
    let (_, reads) = capture(|| values[0].cons_car());
    assert!(reads.unwrap().unchanged());
}

#[cfg(test)]
#[path = "fast_paths_test.rs"]
mod fast_paths;

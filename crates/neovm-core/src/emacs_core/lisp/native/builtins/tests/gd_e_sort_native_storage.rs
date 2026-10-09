//! GNU sort.c:70-123 uses raw permutation moves; native predicates need no
//! per-move collection journal entries. Callback boundaries still publish state.
use super::*;
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;

#[test]
fn native_vector_sort_batches_collection_mutations() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    for predicate in [Value::NIL, Value::symbol("<"), Value::symbol("string<")] {
        let strings = predicate.as_symbol_name() == Some("string<");
        let vector = Value::vector(
            (0..50)
                .map(|i| {
                    let n = (i * 17 + 13) % 50;
                    if strings {
                        Value::string(format!("{n:04}"))
                    } else {
                        Value::fixnum(n)
                    }
                })
                .collect(),
        );
        let (revisions, _) = capture(|| {
            let _ = vector.as_vector_data().unwrap();
            let before = LispCollectionRevision::current();
            let args = if predicate.is_nil() {
                vec![vector, Value::symbol(":in-place"), Value::T]
            } else {
                vec![vector, predicate]
            };
            assert_eq!(builtin_sort_slice(&mut eval, &args).unwrap(), vector);
            let revisions = LispCollectionRevision::current().steps_since_for_test(before);
            revisions
        });
        assert!(
            revisions <= 2,
            "native predicate {predicate:?} journalled {revisions} moves"
        );
        let values = vector.as_vector_data().unwrap();
        for (index, value) in values.iter().enumerate() {
            if strings {
                assert_eq!(value.as_utf8_str(), Some(format!("{index:04}").as_str()));
            } else {
                assert_eq!(*value, Value::fixnum(index as i64));
            }
        }
    }
}

#[test]
fn native_vector_sort_roots_temporaries_at_gc_boundaries() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    let vector = Value::vector(
        (0..50)
            .map(|i| Value::string(format!("{:04}", (i * 17 + 13) % 50)))
            .collect(),
    );
    eval.gc_stress = true;
    let result = builtin_sort_slice(&mut eval, &[vector, Value::symbol("string<")]);
    eval.gc_stress = false;
    assert_eq!(result.unwrap(), vector);
    for (index, value) in vector.as_vector_data().unwrap().iter().enumerate() {
        assert_eq!(value.as_utf8_str(), Some(format!("{index:04}").as_str()));
    }
}

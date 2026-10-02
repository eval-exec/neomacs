use super::*;
use crate::tagged::collection_reads::capture;

#[test]
fn copy_sequence_capture_keeps_every_source_cell_and_result_setter_dependency() {
    crate::test_utils::init_test_tracing();
    for changed_position in 0..3 {
        let source = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
        let mut changed = source;
        for _ in 0..changed_position {
            changed = changed.cons_cdr();
        }
        let (copied, reads) = capture(|| copy_sequence_value(source));
        let copied = copied.unwrap();
        let reads = reads.expect("fresh result writes precede their first projection");
        assert_ne!(source.bits(), copied.bits());
        let unrelated = Value::cons(Value::NIL, Value::NIL);
        unrelated.set_car(Value::T);
        assert!(reads.unchanged());
        changed.set_car(Value::T);
        assert!(!reads.unchanged(), "source position {changed_position}");

        let (copied, reads) = capture(|| copy_sequence_value(source));
        let copied = copied.unwrap();
        let reads = reads.unwrap();
        copied.set_cdr(Value::NIL);
        assert!(
            !reads.unchanged(),
            "the existing first result setter projection is still observed"
        );
    }
}

#[test]
fn copy_sequence_capture_preserves_nested_and_first_read_dependencies() {
    crate::test_utils::init_test_tracing();
    let source = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    let (_, outer) = capture(|| {
        let (result, inner) = capture(|| copy_sequence_value(source));
        assert!(result.is_ok());
        assert!(inner.unwrap().unchanged());
    });
    source.cons_cdr().set_car(Value::T);
    assert!(!outer.unwrap().unchanged());

    let (_, reads) = capture(|| {
        assert!(copy_sequence_value(source).is_ok());
        source.set_cdr(Value::NIL);
        assert!(copy_sequence_value(source).is_ok());
    });
    assert!(
        reads.is_none(),
        "the original source revision must be retained"
    );
}

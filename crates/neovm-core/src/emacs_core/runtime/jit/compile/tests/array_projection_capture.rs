use super::*;
use crate::emacs_core::error::FlowRef;
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;

#[derive(Clone, Copy, Debug)]
enum ArrayShape {
    Vector,
    Record,
}

impl ArrayShape {
    fn make(self) -> Value {
        let slots = vec![Value::fixnum(42), Value::NIL];
        match self {
            Self::Vector => Value::vector(slots),
            Self::Record => Value::make_record(slots),
        }
    }

    fn change(self, array: Value) {
        let changed = match self {
            Self::Vector => array.set_vector_slot(0, Value::T),
            Self::Record => array.set_record_slot(0, Value::T),
        };
        assert!(changed);
    }
}

fn shim_aref(ctx: &mut Context, array: Value, index: Value) -> i64 {
    neovm_jit_aref(
        ctx as *mut Context as *mut u8,
        array.bits() as i64,
        index.bits() as i64,
    )
}

fn assert_signal(result: i64) {
    assert_eq!(result, VALUE_SHIM_SIGNAL);
    assert!(matches!(
        take_pending_flow()
            .as_ref()
            .map(crate::emacs_core::error::Flow::kind),
        Some(FlowRef::Signal(_))
    ));
}

#[test]
fn native_array_projection_capture_keeps_vector_record_and_bounds_dependencies() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    for shape in [ArrayShape::Vector, ArrayShape::Record] {
        for index in [Value::fixnum(0), Value::fixnum(2), Value::fixnum(-1)] {
            let array = shape.make();
            let expected = b::builtin_aref_values(array, index);
            let (result, reads) = capture(|| shim_aref(&mut ctx, array, index));
            match expected {
                Ok(value) => assert_eq!(result, value.bits() as i64),
                Err(_) => assert_signal(result),
            }
            let reads = reads.expect("read-only native aref is coherent");
            let unrelated = Value::vector(vec![Value::NIL]);
            unrelated.set_vector_slot(0, Value::T);
            assert!(reads.unchanged(), "an unrelated array is not a dependency");
            shape.change(array);
            assert!(!reads.unchanged(), "{shape:?} at index {index:?}");
        }
    }
}

#[test]
fn native_array_projection_capture_observes_wrong_headers_after_index_validation() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let wrong = Value::make_sub_char_table(1, 0, vec![Value::NIL]);
    let (result, reads) = capture(|| shim_aref(&mut ctx, wrong, Value::fixnum(0)));
    assert_signal(result);
    LispCollectionRevision::changed(wrong);
    assert!(
        !reads.unwrap().unchanged(),
        "a rejected veclike header remains a dependency"
    );

    for array in [wrong, Value::vector(vec![Value::NIL])] {
        let (result, reads) = capture(|| shim_aref(&mut ctx, array, Value::NIL));
        assert_signal(result);
        LispCollectionRevision::changed(array);
        assert!(
            reads.unwrap().unchanged(),
            "a non-fixnum index fails before projecting either array subtype"
        );
    }
}

#[test]
fn native_array_projection_capture_preserves_nested_and_first_read_revisions() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let array = Value::vector(vec![Value::fixnum(42)]);
    let (_, outer) = capture(|| {
        let (result, inner) = capture(|| shim_aref(&mut ctx, array, Value::fixnum(0)));
        assert_eq!(result, Value::fixnum(42).bits() as i64);
        assert!(inner.unwrap().unchanged());
    });
    array.set_vector_slot(0, Value::NIL);
    assert!(
        !outer.unwrap().unchanged(),
        "a nested native array read propagates to its outer capture"
    );

    let (_, reads) = capture(|| {
        assert_eq!(
            shim_aref(&mut ctx, array, Value::fixnum(0)),
            Value::NIL.bits() as i64
        );
        array.set_vector_slot(0, Value::T);
        assert_eq!(
            shim_aref(&mut ctx, array, Value::fixnum(0)),
            Value::T.bits() as i64
        );
    });
    assert!(
        reads.is_none(),
        "a later native projection cannot replace the first-read revision"
    );
}

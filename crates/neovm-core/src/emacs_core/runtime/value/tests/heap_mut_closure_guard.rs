//! Runtime Value wrappers share GEN-5's mutable heap borrow extent.

#[cfg(debug_assertions)]
use super::*;
#[cfg(debug_assertions)]
use crate::tagged::gc::{TaggedHeap, set_tagged_heap};
#[cfg(debug_assertions)]
use crate::tagged::mutate::debug_assert_no_heap_mut_closure;
#[cfg(debug_assertions)]
use std::panic::{AssertUnwindSafe, catch_unwind};

#[cfg(debug_assertions)]
fn assert_guarded() {
    assert!(catch_unwind(AssertUnwindSafe(debug_assert_no_heap_mut_closure)).is_err());
}

#[test]
#[cfg(debug_assertions)]
fn gc_runtime_value_mutation_closures_are_guarded() {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    // Use each owner before allocating the next; no unrooted Value is kept
    // across an allocation.
    {
        let value = Value::make_bool_vector(8, vec![0]);
        assert_eq!(value.with_bool_vector_mut(|_| assert_guarded()), Some(()));
    }
    {
        let value = Value::make_char_table(Value::NIL, Value::NIL, 0);
        assert_eq!(value.with_char_table_mut(|_| assert_guarded()), Some(()));
    }
    {
        let value = Value::make_sub_char_table(3, 128, vec![Value::NIL; 128]);
        assert_eq!(
            value.with_sub_char_table_mut(|_| assert_guarded()),
            Some(())
        );
    }
    {
        let value = Value::obarray(8);
        assert_eq!(value.with_obarray_mut(|_| assert_guarded()), Some(()));
    }
    debug_assert_no_heap_mut_closure();
}

#[test]
#[cfg(debug_assertions)]
fn gc_runtime_value_mutation_guard_unwinds_and_rejects_wrong_kinds() {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let value = Value::make_char_table(Value::NIL, Value::NIL, 0);
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            value.with_char_table_mut(|_| panic!("runtime mutation unwind"));
        }))
        .is_err()
    );
    debug_assert_no_heap_mut_closure();
    assert_eq!(
        Value::NIL.with_bool_vector_mut(|_| panic!("wrong kind")),
        None::<()>
    );
    assert_eq!(
        Value::NIL.with_char_table_mut(|_| panic!("wrong kind")),
        None::<()>
    );
    assert_eq!(
        Value::NIL.with_sub_char_table_mut(|_| panic!("wrong kind")),
        None::<()>
    );
    assert_eq!(
        Value::NIL.with_obarray_mut(|_| panic!("wrong kind")),
        None::<()>
    );
    debug_assert_no_heap_mut_closure();
}

use super::{Value, VectorElementLayout, allocate_vector_elements};
use crate::emacs_core::alloc::AllocationFailure;

#[test]
fn vector_direct_storage_owns_checked_value_layout_and_growth() {
    crate::test_utils::init_test_tracing();

    let empty = allocate_vector_elements(VectorElementLayout::try_from(0).unwrap()).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.capacity(), 0);
    assert!(matches!(
        VectorElementLayout::try_from(usize::MAX),
        Err(AllocationFailure::InvalidLayout(_))
    ));
    let over_isize = isize::MAX as usize / std::mem::size_of::<Value>() + 1;
    assert!(matches!(
        VectorElementLayout::try_from(over_isize),
        Err(AllocationFailure::InvalidLayout(_))
    ));

    for count in [1, 50, 1_000, 100_000] {
        let extent = VectorElementLayout::try_from(count).unwrap();
        let mut values = allocate_vector_elements(extent).unwrap();
        assert!(values.is_empty());
        assert_eq!(values.capacity(), count);
        let pointer = values.as_ptr();
        values.resize(count, Value::fixnum(7));
        assert_eq!(values.as_ptr(), pointer, "fill must use owned capacity");
        assert!(values.iter().all(|value| *value == Value::fixnum(7)));
        values[0] = Value::NIL;
        let adopted = values.into_boxed_slice();
        assert_eq!(adopted.as_ptr(), pointer, "exact Value backing is adopted");
        assert_eq!(adopted[0], Value::NIL);
        assert_eq!(adopted.len(), count);
        // Vec's existing boxed-slice adoption owns and reclaims the same
        // initialized Value extent; no heap or GC contract is changed.
        drop(adopted);
    }

    let extent = VectorElementLayout::try_from(3).unwrap();
    let mut first = allocate_vector_elements(extent).unwrap();
    let mut second = allocate_vector_elements(extent).unwrap();
    assert_ne!(first.as_ptr(), second.as_ptr());
    first.resize(3, Value::fixnum(17));
    second.resize(3, Value::NIL);
    first[1] = Value::T;
    assert_eq!(second, [Value::NIL; 3]);
    first
        .try_reserve_exact(8)
        .expect("ordinary Value Vec growth");
    first.extend_from_slice(&[Value::fixnum(23); 8]);
    assert_eq!(
        &first[..3],
        &[Value::fixnum(17), Value::T, Value::fixnum(17)]
    );
    assert_eq!(&first[3..], &[Value::fixnum(23); 8]);
    // Immediate raw-storage adoption, subsequent realloc and initialized
    // Values are reclaimed solely by the ordinary global-allocator Vec Drop.
    drop(first);
    drop(second);
}

use super::{LispValueSlice, LispValueVec};
use crate::tagged::value::TaggedValue;

#[test]
fn mapped_lisp_value_vec_borrows_until_mutation() {
    let slots = vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)];
    let mut values = unsafe { LispValueVec::mapped(slots.as_ptr(), slots.len()) };

    assert_eq!(values.as_slice(), slots.as_slice());
    values.ensure_owned().push(TaggedValue::fixnum(3));

    drop(slots);
    assert_eq!(
        values.as_slice(),
        &[
            TaggedValue::fixnum(1),
            TaggedValue::fixnum(2),
            TaggedValue::fixnum(3)
        ]
    );
}

#[test]
fn lisp_value_slice_clone_returns_owned_vec_for_compat_callers() {
    let slots = vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)];
    let slice = LispValueSlice::from_slice(&slots);

    let owned = slice.clone();
    drop(slots);
    assert_eq!(owned, vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)]);
}

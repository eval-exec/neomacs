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

#[test]
fn miriub3_vector_scan_keeps_cell_provenance_across_slot_store() {
    let mut values = LispValueVec::owned(vec![TaggedValue::fixnum(1)]);
    let captured = values.scan_entry();
    assert!(values.store_atomic(0, TaggedValue::fixnum(2)));
    let mut seen = Vec::new();
    // SAFETY: the captured allocation remains alive and only cell stores touch it.
    unsafe { captured.scan_values(|value| seen.push(value)) };
    assert_eq!(seen, vec![TaggedValue::fixnum(2)]);
    assert_eq!(values.load_atomic(0), Some(TaggedValue::fixnum(2)));
    assert_eq!(values.load_atomic(1), None);
    assert!(!values.store_atomic(1, TaggedValue::T));
}

#[test]
fn miriub3_vector_bulk_guard_restores_storage_on_unwind() {
    let mut words = Vec::with_capacity(8);
    words.push(TaggedValue::fixnum(1));
    let mut values = LispValueVec::owned(words);
    let original = values.as_slice().as_ptr();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut bulk = values.ensure_owned();
        bulk.push(TaggedValue::fixnum(2));
        panic!("leave the bulk view by unwinding");
    }));
    assert!(unwind.is_err());
    assert_eq!(values.as_slice().as_ptr(), original);
    assert_eq!(values.owned_capacity(), 8);
    assert_eq!(
        values.iter_atomic().collect::<Vec<_>>(),
        vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)]
    );
    assert!(values.store_atomic(1, TaggedValue::fixnum(3)));
    assert_eq!(values[1], TaggedValue::fixnum(3));
}

#[test]
fn miriub3_mapped_vector_scan_stays_immutable_after_promotion() {
    let image = [TaggedValue::fixnum(1)];
    // SAFETY: the image outlives the backing and its captured span.
    let mut values = unsafe { LispValueVec::mapped(image.as_ptr(), image.len()) };
    let captured = values.scan_entry();
    assert!(values.store_atomic(0, TaggedValue::fixnum(2)));
    let mut seen = Vec::new();
    // SAFETY: promotion never mutates or frees the captured image.
    unsafe { captured.scan_values(|value| seen.push(value)) };
    assert_eq!(seen, vec![TaggedValue::fixnum(1)]);
    assert_eq!(values.load_atomic(0), Some(TaggedValue::fixnum(2)));
}

use super::{AtomicTaggedValue, LispValueSlice, LispValueVec};
use crate::tagged::value::TaggedValue;

#[test]
fn mapped_lisp_value_vec_borrows_until_mutation() {
    let slots = vec![
        AtomicTaggedValue::new(TaggedValue::fixnum(1)),
        AtomicTaggedValue::new(TaggedValue::fixnum(2)),
    ];
    let mut values = unsafe { LispValueVec::mapped(slots.as_ptr(), slots.len()) };

    assert_eq!(
        values.as_slice(),
        &[TaggedValue::fixnum(1), TaggedValue::fixnum(2)]
    );
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
    let slots = vec![
        AtomicTaggedValue::new(TaggedValue::fixnum(1)),
        AtomicTaggedValue::new(TaggedValue::fixnum(2)),
    ];
    let slice = LispValueSlice::from_atomic_slice(&slots);

    let owned = slice.clone();
    drop(slots);
    assert_eq!(owned, vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)]);
}

#[test]
fn miri_lisp_value_slice_mapped_read_and_promotion() {
    let image = [
        AtomicTaggedValue::new(TaggedValue::fixnum(10)),
        AtomicTaggedValue::new(TaggedValue::fixnum(20)),
    ];
    // SAFETY: `image` outlives both `ptr` and `mapped_view`.
    let ptr: *mut LispValueVec = Box::into_raw(Box::new(unsafe {
        LispValueVec::mapped(image.as_ptr(), image.len())
    }));

    // Borrow `mapped_view` and keep it LIVE across `store_atomic` promotion
    // (mirroring a live `&'h LispValueSlice` held across `set_vector_slot`).
    // SAFETY: `ptr` points to the live `LispValueVec` allocation and `image` stays
    // live and immutable for the entire test.
    let mapped_view: &LispValueSlice = unsafe { (*ptr).as_value_slice() };
    assert_eq!(mapped_view.len(), 2);
    assert_eq!(mapped_view.at(0), TaggedValue::fixnum(10));
    assert_eq!(mapped_view.get(1), Some(TaggedValue::fixnum(20)));
    assert_eq!(
        mapped_view.iter().collect::<Vec<_>>(),
        vec![TaggedValue::fixnum(10), TaggedValue::fixnum(20)]
    );

    // Promote mapped -> owned while `mapped_view` is still live.
    // SAFETY: `owned_cells()` reads via `as_value_slice()` (never `&[TaggedValue]`)
    // and replaces only the `LispValueVec` enum header with a fresh `Owned` Vec.
    assert!(unsafe { (*ptr).store_atomic(0, TaggedValue::fixnum(77)) });

    // `mapped_view` remains sound to read under SB + TB and observes the
    // immutable pre-promotion mapped snapshot.
    assert_eq!(mapped_view.at(0), TaggedValue::fixnum(10));
    assert_eq!(mapped_view.at(1), TaggedValue::fixnum(20));
    assert_eq!(
        mapped_view,
        &[TaggedValue::fixnum(10), TaggedValue::fixnum(20)]
    );

    // Re-borrowing after promotion observes the new `Owned` buffer and
    // subsequent in-place atomic stores.
    // SAFETY: `ptr` is live and now holds an `Owned` backing.
    let owned_view: &LispValueSlice = unsafe { (*ptr).as_value_slice() };
    assert_eq!(owned_view.at(0), TaggedValue::fixnum(77));
    assert_eq!(owned_view.at(1), TaggedValue::fixnum(20));
    assert!(unsafe { (*ptr).store_atomic(1, TaggedValue::fixnum(88)) });
    assert_eq!(owned_view.at(0), TaggedValue::fixnum(77));
    assert_eq!(owned_view.at(1), TaggedValue::fixnum(88));
    assert_eq!(
        owned_view,
        &[TaggedValue::fixnum(77), TaggedValue::fixnum(88)]
    );
    // Re-read the still-live `mapped_view` one last time after owned mutation.
    assert_eq!(mapped_view.at(0), TaggedValue::fixnum(10));
    assert_eq!(mapped_view.at(1), TaggedValue::fixnum(20));

    // SAFETY: `ptr` was allocated with `Box::into_raw` above and no references
    // derived from it are used after this line.
    unsafe {
        drop(Box::from_raw(ptr));
    }
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
    let image = [AtomicTaggedValue::new(TaggedValue::fixnum(1))];
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

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

#[test]
fn miri_0c_reentrant_with_hash_table_mut_panics_without_aliasing() {
    let mut heap = crate::tagged::gc::TestHeap::new();
    let ht = heap.alloc_hash_table(crate::value::LispHashTable::new(
        crate::value::HashTableTest::Eq,
    ));
    let nested = crate::tagged::mutate::with_hash_table_mut(ht, |outer| {
        let len_before = outer.data.len();
        let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::with_hash_table_mut(ht, |inner| inner.data.len())
        }));
        assert!(
            reenter.is_err(),
            "re-entrant with_hash_table_mut on same object must panic in release & debug"
        );
        assert_eq!(outer.data.len(), len_before);
    });
    assert!(nested.is_some());
    // Sequential re-entry after outer exits must still work.
    assert_eq!(
        crate::tagged::mutate::with_hash_table_mut(ht, |t| t.data.len()),
        Some(0)
    );
}

#[test]
fn miri_0c_reentrant_with_vector_data_mut_panics_without_aliasing() {
    let mut heap = crate::tagged::gc::TestHeap::new();
    let v = heap.alloc_vector(vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)]);
    let nested = crate::tagged::mutate::with_vector_data_mut(v, |outer| {
        outer[0] = TaggedValue::fixnum(10);
        let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::with_vector_data_mut(v, |inner| inner.len())
        }));
        assert!(
            reenter.is_err(),
            "re-entrant with_vector_data_mut on same object must panic in release & debug"
        );
        let replace = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::replace_vector_data(v, Vec::new())
        }));
        assert!(
            replace.is_err(),
            "re-entrant replace_vector_data on same object must panic in release & debug"
        );
        assert_eq!(outer[0], TaggedValue::fixnum(10));
        assert_eq!(outer[1], TaggedValue::fixnum(2));
    });
    assert!(nested.is_some());
    assert_eq!(
        v.as_vector_data().unwrap(),
        &[TaggedValue::fixnum(10), TaggedValue::fixnum(2)]
    );
}

#[test]
fn miri_0c_reentrant_with_string_bytes_mut_panics_without_aliasing() {
    let mut heap = crate::tagged::gc::TestHeap::new();
    let s = heap.alloc_string(crate::heap_types::LispString::from_unibyte(b"abc".to_vec()));
    let nested = crate::tagged::mutate::with_string_bytes_mut(s, |outer| {
        outer[0] = b'z';
        let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::with_string_bytes_mut(s, |inner| inner.len())
        }));
        assert!(
            reenter.is_err(),
            "re-entrant with_string_bytes_mut on same object must panic in release & debug"
        );
        let props = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::with_string_text_props_mut(s, |_| ())
        }));
        assert!(
            props.is_err(),
            "cross-entry with_string_text_props_mut on same string must panic"
        );
        let clear_iv = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::clear_string_intervals(s)
        }));
        assert!(
            clear_iv.is_err(),
            "re-entrant clear_string_intervals on same string must panic before deref"
        );
        assert_eq!(outer, b"zbc");
    });
    assert!(nested.is_some());
}

#[test]
fn miri_0c_indirect_reentrant_outer_object_panics_while_distinct_objects_nest() {
    let mut heap = crate::tagged::gc::TestHeap::new();
    let v1 = heap.alloc_vector(vec![TaggedValue::fixnum(1)]);
    let r2 = heap.alloc_record(vec![TaggedValue::fixnum(2)]);
    crate::tagged::mutate::with_vector_data_mut(v1, |outer| {
        outer[0] = TaggedValue::fixnum(11);
        crate::tagged::mutate::with_record_data_mut(r2, |inner| {
            inner[0] = TaggedValue::fixnum(22);
            let reenter_outer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::tagged::mutate::with_vector_data_mut(v1, |v| v.len())
            }));
            assert!(
                reenter_outer.is_err(),
                "indirect re-entry v1 -> r2 -> v1 must panic"
            );
            let reenter_inner = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::tagged::mutate::with_record_data_mut(r2, |r| r.len())
            }));
            assert!(
                reenter_inner.is_err(),
                "re-entry r2 -> r2 inside nested scope must panic"
            );
            assert_eq!(inner[0], TaggedValue::fixnum(22));
        })
        .unwrap();
        assert_eq!(outer[0], TaggedValue::fixnum(11));
    })
    .unwrap();
}

#[test]
fn miri_0c_owned_value_vec_mutates_in_place_without_move_out() {
    assert_eq!(
        std::mem::size_of::<super::OwnedValueVec<'_>>(),
        std::mem::size_of::<&mut Vec<AtomicTaggedValue>>(),
        "OwnedValueVec must borrow Vec<AtomicTaggedValue> in place (no moved-out Vec field)"
    );
    assert!(
        !std::mem::needs_drop::<super::OwnedValueVec<'_>>(),
        "OwnedValueVec must not rely on Drop to restore a moved-out Vec"
    );
    let mut values = LispValueVec::owned(vec![TaggedValue::fixnum(5), TaggedValue::fixnum(6)]);
    let raw_buf_before = values.as_slice().as_ptr();
    {
        let mut guard = values.ensure_owned();
        guard[0] = TaggedValue::fixnum(55);
        assert_eq!(guard.as_ptr(), raw_buf_before);
        // Even if the guard is forgotten without running Drop, the backing stays
        // intact in place (never emptied by `std::mem::take` and never leaked).
        std::mem::forget(guard);
    }
    assert_eq!(
        values.as_value_slice(),
        &[TaggedValue::fixnum(55), TaggedValue::fixnum(6)]
    );
}

#[test]
fn miri_0c_collect_inside_with_vector_data_mut_panics_in_release() {
    let mut heap = crate::tagged::gc::TestHeap::new();
    let v = heap.alloc_vector(vec![TaggedValue::fixnum(7)]);
    crate::tagged::mutate::with_vector_data_mut(v, |_| {
        let collected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            crate::tagged::gc::with_tagged_heap(|h| {
                h.collect_exact(std::iter::once(v));
            });
        }));
        assert!(
            collected.is_err(),
            "collection inside with_vector_data_mut must panic in release & debug"
        );
    })
    .unwrap();
}

#[test]
fn miri_0c_reentrant_with_hash_table_write_inactive_panics_without_aliasing() {
    let mut heap = crate::tagged::gc::TestHeap::new();
    let ht = heap.alloc_hash_table(crate::value::LispHashTable::new(
        crate::value::HashTableTest::Eq,
    ));
    // 1. Outer `with_hash_table_mut` -> inner `with_hash_table_write_inactive`:
    //    must panic BEFORE forming `&LispHashTable` or hydrating so the outer
    //    `&mut LispHashTable` remains valid under Miri SB + TB.
    crate::tagged::mutate::with_hash_table_mut(ht, |outer| {
        let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            ht.with_hash_table_write_inactive(|w| {
                let _ = w.put(TaggedValue::fixnum(1), TaggedValue::fixnum(2), false);
            })
        }));
        assert!(
            reenter.is_err(),
            "with_hash_table_write_inactive inside with_hash_table_mut must panic before aliasing"
        );
        // Touching `outer` mutably after caught re-entry proves no pre-guard `&LispHashTable` aliased it.
        outer.data.switch_epoch = outer.data.switch_epoch.wrapping_add(1);
    })
    .unwrap();

    // 2. Outer `with_hash_table_write_inactive` -> inner `with_hash_table_mut`
    //    AND nested `with_hash_table_write_inactive`: both must panic without aliasing.
    unsafe {
        ht.with_hash_table_write_inactive(|outer| {
            let reenter_mut = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::tagged::mutate::with_hash_table_mut(ht, |t| t.data.len())
            }));
            assert!(
                reenter_mut.is_err(),
                "with_hash_table_mut inside with_hash_table_write_inactive must panic"
            );
            let reenter_write = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ht.with_hash_table_write_inactive(|w| {
                    let _ = w.put(TaggedValue::fixnum(3), TaggedValue::fixnum(4), false);
                })
            }));
            assert!(
                reenter_write.is_err(),
                "nested with_hash_table_write_inactive on same table must panic before aliasing"
            );
            outer
                .put(TaggedValue::fixnum(10), TaggedValue::fixnum(20), false)
                .unwrap();
        })
        .unwrap();
    }
    assert_eq!(ht.as_hash_table().unwrap().data.len(), 1);
}

#[test]
fn miri_0c_reentrant_whole_object_mutator_panics_without_header_aliasing() {
    let mut heap = crate::tagged::gc::TestHeap::new();
    let ob = heap.alloc_obarray(vec![TaggedValue::NIL; 4]);
    ob.with_obarray_mut(|outer| {
        let reenter =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ob.with_obarray_mut(|_| ())));
        assert!(
            reenter.is_err(),
            "re-entrant with_obarray_mut must panic before reading VecLikeHeader::type_tag"
        );
        // Touching the whole `&mut ObarrayObj` after caught re-entry proves no pre-guard header read aliased it.
        outer.count = outer.count.wrapping_add(1);
    })
    .unwrap();

    let bv = heap.alloc_bool_vector(8, vec![0]);
    bv.with_bool_vector_mut(|outer| {
        let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bv.with_bool_vector_mut(|_| ())
        }));
        assert!(
            reenter.is_err(),
            "re-entrant with_bool_vector_mut must panic before reading VecLikeHeader::type_tag"
        );
        outer.words[0] = 0x5a;
    })
    .unwrap();

    let lam = heap.alloc_lambda(vec![TaggedValue::fixnum(1), TaggedValue::fixnum(2)]);
    crate::tagged::mutate::with_closure_slots_mut(lam, |outer| {
        let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::with_closure_slots_mut(lam, |s| s.len())
        }));
        assert!(
            reenter.is_err(),
            "re-entrant with_closure_slots_mut must panic before reading VecLikeHeader::type_tag"
        );
        outer[0] = TaggedValue::fixnum(99);
    })
    .unwrap();
    assert_eq!(
        lam.closure_slots().unwrap().get(0),
        Some(TaggedValue::fixnum(99))
    );
}

#[test]
fn miri_0c_reentrant_remaining_value_mutators_panic_without_aliasing() {
    let mut heap = crate::tagged::gc::TestHeap::new();

    let ct = heap.alloc_char_table(TaggedValue::NIL, TaggedValue::NIL, 1);
    crate::tagged::mutate::with_char_table_write(ct, |outer| {
        let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::with_char_table_write(ct, |_| ())
        }));
        assert!(
            reenter.is_err(),
            "re-entrant with_char_table_write must panic in release & debug"
        );
        outer.set_default(TaggedValue::fixnum(11));
    })
    .unwrap();

    let sct = heap.alloc_sub_char_table(1, 0, vec![TaggedValue::NIL; 4]);
    crate::tagged::mutate::with_sub_char_table_write(sct, |outer| {
        let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::with_sub_char_table_write(sct, |_| ())
        }));
        assert!(
            reenter.is_err(),
            "re-entrant with_sub_char_table_write must panic in release & debug"
        );
        outer.set_contents(0, TaggedValue::fixnum(22));
    })
    .unwrap();

    let marker = heap.alloc_marker(crate::heap_types::LispMarker::new(
        crate::heap_types::MarkerBuffer::DETACHED,
        crate::value::positions::CharPos0::new(1),
        crate::heap_types::byte_positions::EmacsBytePos::new(1),
        false,
        true,
    ));
    marker
        .with_marker_data_mut(|outer| {
            let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                marker.with_marker_data_mut(|_| ())
            }));
            assert!(
                reenter.is_err(),
                "re-entrant with_marker_data_mut must panic in release & debug"
            );
            outer.charpos = crate::value::positions::CharPos0::new(2);
        })
        .unwrap();

    let overlay = heap.alloc_overlay(crate::heap_types::OverlayDataInit {
        serial: crate::heap_types::OverlaySerialSource::Mint,
        plist: TaggedValue::NIL,
        buffer: None,
        start: 1,
        end: 2,
        front_advance: false,
        rear_advance: false,
    });
    overlay
        .with_overlay_data_mut(|outer| {
            let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                overlay.with_overlay_data_mut(|_| ())
            }));
            assert!(
                reenter.is_err(),
                "re-entrant with_overlay_data_mut must panic in release & debug"
            );
            outer.front_advance = true;
        })
        .unwrap();

    let xw = heap.alloc_xwidget(
        TaggedValue::NIL,
        TaggedValue::NIL,
        TaggedValue::NIL,
        10,
        20,
        1,
        neovm_display_identity::WebViewId::new(0),
    );
    xw.with_xwidget_mut(|outer| {
        let reenter =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| xw.with_xwidget_mut(|_| ())));
        assert!(
            reenter.is_err(),
            "re-entrant with_xwidget_mut must panic in release & debug"
        );
        outer.width = 30;
    })
    .unwrap();

    let xwv = heap.alloc_xwidget_view(xw, TaggedValue::NIL);
    crate::tagged::mutate::with_xwidget_view_mut(xwv, |outer| {
        let reenter = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::tagged::mutate::with_xwidget_view_mut(xwv, |_| ())
        }));
        assert!(
            reenter.is_err(),
            "re-entrant with_xwidget_view_mut must panic in release & debug"
        );
        outer.x = 7;
    })
    .unwrap();

    let s = heap.alloc_string(crate::heap_types::LispString::from_unibyte(b"abc".to_vec()));
    s.with_lisp_string_mut(|outer| {
        let reenter_str = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            s.with_lisp_string_mut(|_| ())
        }));
        assert!(
            reenter_str.is_err(),
            "re-entrant with_lisp_string_mut must panic in release & debug"
        );
        let reenter_clear =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.clear_string_contents()));
        assert!(
            reenter_clear.is_err(),
            "re-entrant clear_string_contents must panic in release & debug"
        );
        let reenter_pin =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| s.pin_string_immovable()));
        assert!(
            reenter_pin.is_err(),
            "re-entrant pin_string_immovable must panic in release & debug"
        );
        let reenter_props = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::value::mutate_string_text_properties(s, |_| ())
        }));
        assert!(
            reenter_props.is_err(),
            "re-entrant mutate_string_text_properties must panic in release & debug"
        );
        assert_eq!(outer.as_bytes(), b"abc");
    })
    .unwrap();
}

#[test]
fn miri_lisp_value_vec_mapped_from_pdump_bytes_reads_and_promotes() {
    let image = [
        AtomicTaggedValue::new(TaggedValue::fixnum(31)),
        AtomicTaggedValue::new(TaggedValue::fixnum(42)),
    ];
    let byte_ptr = image.as_ptr().cast::<u8>();
    // SAFETY: `image` is aligned to `AtomicTaggedValue`, initialized, and outlives `values`.
    let mut values = unsafe { LispValueVec::mapped_from_pdump_bytes(byte_ptr, image.len()) };
    assert_eq!(
        values.as_value_slice(),
        &[TaggedValue::fixnum(31), TaggedValue::fixnum(42)]
    );
    assert!(values.store_atomic(0, TaggedValue::fixnum(99)));
    assert_eq!(
        values.as_value_slice(),
        &[TaggedValue::fixnum(99), TaggedValue::fixnum(42)]
    );
    // Empty span accepts a dangling aligned pointer.
    let empty = unsafe {
        LispValueVec::mapped_from_pdump_bytes(
            std::ptr::NonNull::<AtomicTaggedValue>::dangling()
                .as_ptr()
                .cast::<u8>(),
            0,
        )
    };
    assert!(empty.as_value_slice().is_empty());

    // Non-empty misaligned or null pointers must panic unconditionally (release & debug).
    let misaligned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        LispValueVec::mapped_from_pdump_bytes(byte_ptr.add(1), 1)
    }));
    assert!(
        misaligned.is_err(),
        "misaligned non-empty pdump slot pointer must panic"
    );
    let null_non_empty = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        LispValueVec::mapped_from_pdump_bytes(std::ptr::null(), 1)
    }));
    assert!(
        null_non_empty.is_err(),
        "null non-empty pdump slot pointer must panic"
    );
}

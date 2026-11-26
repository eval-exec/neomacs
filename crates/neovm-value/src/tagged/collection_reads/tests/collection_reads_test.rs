use super::*;
use crate::value::Value;

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
            assert!(with_state(|s| s.borrow().captures.len()) <= MAX_DEPTH);
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
    let (_, reads) = capture(|| table.as_char_table_obj().unwrap().defalt.get());
    table.with_char_table_mut(|table| table.set_default(Value::T));
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
fn empty_capture_remains_valid_across_journal_overflow() {
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let (_, reads) = capture(|| ());
    let reads = reads.expect("empty capture should produce a certificate");
    assert!(reads.reads_empty_for_test());
    for _ in 0..=JOURNAL_SIZE {
        unrelated.set_car(Value::T);
    }
    assert!(
        reads.unchanged(),
        "an empty dependency set cannot be invalidated by unrelated writes"
    );
    assert!(reads.unchanged_and_observe());
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

#[test]
fn bulk_mutators_preserve_journal_before_read_capture_order() {
    let mut heap = crate::tagged::gc::TestHeap::new();

    fn check_journal_before_read(
        label: &str,
        owner: Value,
        mut read: impl FnMut(Value),
        mut mutate: impl FnMut(Value),
    ) {
        let (_, reads) = capture(|| {
            mutate(owner);
            read(owner);
        });
        let reads = reads.unwrap_or_else(|| {
            panic!("{label}: write-before-read capture must return a coherent certificate")
        });
        assert!(
            reads.unchanged(),
            "{label}: fresh write-before-read capture invalidated its own certificate"
        );
        assert!(
            reads.contains_owner_for_test(owner.bits()),
            "{label}: certificate must observe the mutated owner"
        );
        mutate(owner);
        assert!(
            !reads.unchanged(),
            "{label}: subsequent mutation must invalidate the certificate"
        );

        let (_, invalid) = capture(|| {
            read(owner);
            mutate(owner);
        });
        assert!(
            invalid.is_none(),
            "{label}: read-before-write capture must be rejected"
        );
    }

    let overlay = heap.alloc_overlay(crate::heap_types::OverlayDataInit {
        serial: crate::heap_types::OverlaySerialSource::Mint,
        plist: Value::NIL,
        buffer: None,
        start: 1,
        end: 2,
        front_advance: false,
        rear_advance: false,
    });
    check_journal_before_read(
        "with_overlay_data_mut",
        overlay,
        |v| {
            let _ = v.as_overlay_data();
        },
        |v| {
            v.with_overlay_data_mut(|d| d.front_advance = !d.front_advance)
                .unwrap();
        },
    );

    let bv = heap.alloc_bool_vector(8, vec![0]);
    check_journal_before_read(
        "with_bool_vector_mut",
        bv,
        |v| {
            let _ = v.as_bool_vector_obj();
        },
        |v| {
            v.with_bool_vector_mut(|_| ()).unwrap();
        },
    );

    let vec_val = Value::vector(vec![Value::fixnum(1)]);
    check_journal_before_read(
        "with_vector_data_mut",
        vec_val,
        |v| {
            let _ = v.as_vector_data();
        },
        |v| {
            v.with_vector_data_mut(|s| s[0] = Value::T).unwrap();
        },
    );
    check_journal_before_read(
        "replace_vector_data",
        Value::vector(vec![Value::fixnum(1)]),
        |v| {
            let _ = v.as_vector_data();
        },
        |v| {
            assert!(v.replace_vector_data(vec![Value::T]));
        },
    );

    let rec_val = Value::make_record(vec![Value::fixnum(1)]);
    check_journal_before_read(
        "with_record_data_mut",
        rec_val,
        |v| {
            let _ = v.as_record_data();
        },
        |v| {
            v.with_record_data_mut(|s| s[0] = Value::T).unwrap();
        },
    );
    check_journal_before_read(
        "replace_record_data",
        Value::make_record(vec![Value::fixnum(1)]),
        |v| {
            let _ = v.as_record_data();
        },
        |v| {
            assert!(v.replace_record_data(vec![Value::T]));
        },
    );

    let lam = heap.alloc_lambda(vec![Value::fixnum(1)]);
    check_journal_before_read(
        "with_closure_slots_mut(lambda)",
        lam,
        |v| {
            let _ = v.closure_slots();
        },
        |v| {
            crate::tagged::mutate::with_closure_slots_mut(v, |s| s[0] = Value::T).unwrap();
        },
    );
    check_journal_before_read(
        "replace_closure_slots(lambda)",
        heap.alloc_lambda(vec![Value::fixnum(1)]),
        |v| {
            let _ = v.closure_slots();
        },
        |v| {
            assert!(crate::tagged::mutate::replace_closure_slots(
                v,
                vec![Value::T]
            ));
        },
    );

    let ht = heap.alloc_hash_table(crate::value::LispHashTable::new(
        crate::value::HashTableTest::Eq,
    ));
    check_journal_before_read(
        "with_hash_table_mut",
        ht,
        |v| {
            let _ = v.as_hash_table();
        },
        |v| {
            v.with_hash_table_mut(|_| ()).unwrap();
        },
    );
    let ht_write = heap.alloc_hash_table(crate::value::LispHashTable::new(
        crate::value::HashTableTest::Eq,
    ));
    check_journal_before_read(
        "with_hash_table_write_inactive",
        ht_write,
        |v| {
            let _ = v.as_hash_table();
        },
        |v| {
            // SAFETY: standalone `TestHeap` has no concurrent hash mutation active.
            unsafe {
                v.with_hash_table_write_inactive(|w| {
                    let _ = w.put(Value::fixnum(1), Value::T, false);
                })
            }
            .unwrap();
        },
    );
    let ct = heap.alloc_char_table(Value::NIL, Value::NIL, 0);
    check_journal_before_read(
        "with_char_table_mut",
        ct,
        |v| {
            let _ = v.as_char_table_obj();
        },
        |v| {
            v.with_char_table_mut(|t| t.set_default(Value::T)).unwrap();
        },
    );

    let sct = heap.alloc_sub_char_table(1, 0, vec![Value::NIL; 4]);
    check_journal_before_read(
        "with_sub_char_table_write",
        sct,
        |v| {
            let _ = v.as_veclike_ptr();
        },
        |v| {
            crate::tagged::mutate::with_sub_char_table_write(v, |t| t.set_contents(0, Value::T))
                .unwrap();
        },
    );

    let ob = heap.alloc_obarray(vec![Value::NIL; 4]);
    check_journal_before_read(
        "with_obarray_mut",
        ob,
        |v| {
            let _ = v.as_obarray_obj();
        },
        |v| {
            v.with_obarray_mut(|o| o.count = o.count.wrapping_add(1))
                .unwrap();
        },
    );

    let s1 = heap.alloc_string(crate::heap_types::LispString::from_unibyte(b"abc".to_vec()));
    check_journal_before_read(
        "with_lisp_string_mut",
        s1,
        |v| {
            let _ = v.as_lisp_string();
        },
        |v| {
            v.with_lisp_string_mut(|_| ()).unwrap();
        },
    );

    let s2 = heap.alloc_string(crate::heap_types::LispString::from_unibyte(b"abc".to_vec()));
    check_journal_before_read(
        "with_string_bytes_mut",
        s2,
        |v| {
            let _ = v.as_lisp_string();
        },
        |v| {
            v.with_string_bytes_mut(|b| b[0] = b'z').unwrap();
        },
    );

    let s3 = heap.alloc_string(crate::heap_types::LispString::from_unibyte(b"abc".to_vec()));
    check_journal_before_read(
        "clear_string_contents",
        s3,
        |v| {
            let _ = v.as_lisp_string();
        },
        |v| {
            assert!(v.clear_string_contents());
        },
    );

    let s4 = heap.alloc_string(crate::heap_types::LispString::from_unibyte(b"abc".to_vec()));
    check_journal_before_read(
        "pin_string_immovable",
        s4,
        |v| {
            let _ = v.as_lisp_string();
        },
        |v| {
            assert!(v.pin_string_immovable());
        },
    );

    // clear_string_intervals on a string with intervals must also preserve journal-before-read.
    let s5 = heap.alloc_string(crate::heap_types::LispString::from_unibyte(b"abc".to_vec()));
    crate::tagged::mutate::with_string_text_props_mut(s5, |_| ()).unwrap();
    let (_, reads) = capture(|| {
        assert!(crate::tagged::mutate::clear_string_intervals(s5));
        let _ = s5.as_lisp_string();
    });
    let reads = reads.expect("clear_string_intervals write-before-read must return a certificate");
    assert!(reads.unchanged());
    assert!(reads.contains_owner_for_test(s5.bits()));

    // Wrong-subtype veclike rejections must not spuriously invalidate certificates.
    let (_, vec_reads) = capture(|| {
        let _ = vec_val.as_vector_data();
    });
    let vec_reads = vec_reads.unwrap();
    assert!(vec_val.with_overlay_data_mut(|_| ()).is_none());
    assert!(vec_val.with_record_data_mut(|_| ()).is_none());
    assert!(vec_val.with_hash_table_mut(|_| ()).is_none());
    assert!(vec_val.with_char_table_mut(|_| ()).is_none());
    assert!(vec_val.with_obarray_mut(|_| ()).is_none());
    assert!(
        vec_reads.unchanged(),
        "wrong-subtype rejections must not journal spurious writes"
    );
}

#[cfg(test)]
#[path = "fast_paths_test.rs"]
mod fast_paths;

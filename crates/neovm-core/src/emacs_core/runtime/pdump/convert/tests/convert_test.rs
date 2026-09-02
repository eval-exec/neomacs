use super::*;

#[test]
fn preload_tagged_heap_handles_deep_cons_chains_without_recursive_population() {
    crate::test_utils::init_test_tracing();

    let chain_len = 4096usize;
    let objects = (0..chain_len)
        .map(|index| DumpHeapObject::Cons {
            car: DumpValue::Int(index as i64),
            cdr: if index + 1 == chain_len {
                DumpValue::Nil
            } else {
                DumpValue::Cons(DumpHeapRef {
                    index: (index + 1) as u32,
                })
            },
        })
        .collect();
    let heap = DumpTaggedHeap {
        objects,
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };
    let mut decoder = LoadDecoder::new(&heap);

    decoder
        .preload_tagged_heap()
        .expect("deep cons chain should preload without recursive overflow");

    let mut cursor = decoder.load_value(&DumpValue::Cons(DumpHeapRef { index: 0 }));
    for index in 0..chain_len {
        assert_eq!(cursor.cons_car(), Value::fixnum(index as i64));
        cursor = cursor.cons_cdr();
    }
    assert!(cursor.is_nil());
}

#[test]
fn mapped_cons_raw_words_are_loader_source_of_truth_when_no_remap_needed() {
    crate::test_utils::init_test_tracing();
    let mut runtime_heap = Box::new(crate::tagged::gc::TaggedHeap::new());
    crate::tagged::gc::set_tagged_heap(&mut runtime_heap);

    let heap = DumpTaggedHeap {
        objects: vec![DumpHeapObject::Cons {
            car: DumpValue::Int(1),
            cdr: DumpValue::Int(2),
        }],
        mapped_cons: vec![Some(DumpConsSpan { offset: 0 })],
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };
    let mut bytes = vec![0u8; std::mem::size_of::<ConsCell>()];
    write_raw_word(&mut bytes, 0, Value::fixnum(99).bits());
    write_raw_word(
        &mut bytes,
        std::mem::size_of::<TaggedValue>(),
        Value::fixnum(100).bits(),
    );

    let mapped = MappedHeapView::from_mut_slice(&mut bytes);
    let mut decoder = LoadDecoder::new_with_mapped_heap(&heap, Some(mapped));
    decoder.preload_tagged_heap().unwrap();

    assert!(
        decoder.state.cached_value(0).is_none(),
        "mapped cons cells should stay out of the eager load cache"
    );
    assert!(
        !decoder.state.is_populated(0),
        "mapped cons cells should not run descriptor population"
    );

    let value = decoder.load_value(&DumpValue::Cons(DumpHeapRef { index: 0 }));
    assert_eq!(value.cons_car(), Value::fixnum(99));
    assert_eq!(value.cons_cdr(), Value::fixnum(100));
}

#[test]
fn mapped_vector_raw_slots_are_loader_source_of_truth_when_no_remap_needed() {
    crate::test_utils::init_test_tracing();
    let mut runtime_heap = Box::new(crate::tagged::gc::TaggedHeap::new());
    crate::tagged::gc::set_tagged_heap(&mut runtime_heap);

    let slot_offset = std::mem::size_of::<VectorObj>();
    let heap = DumpTaggedHeap {
        objects: vec![DumpHeapObject::Vector(vec![
            DumpValue::Int(1),
            DumpValue::Int(2),
        ])],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: vec![Some(DumpVecLikeSpan {
            offset: 0,
            len: std::mem::size_of::<VectorObj>() as u64,
        })],
        mapped_slots: vec![Some(DumpSlotSpan {
            offset: slot_offset as u64,
            len: 2,
        })],
    };
    let mut bytes = vec![0u8; slot_offset + 2 * std::mem::size_of::<TaggedValue>()];
    write_raw_word(&mut bytes, slot_offset, Value::fixnum(77).bits());
    write_raw_word(
        &mut bytes,
        slot_offset + std::mem::size_of::<TaggedValue>(),
        Value::fixnum(88).bits(),
    );

    let mapped = MappedHeapView::from_mut_slice(&mut bytes);
    let mut decoder = LoadDecoder::new_with_mapped_heap(&heap, Some(mapped));
    decoder.preload_tagged_heap().unwrap();

    assert!(
        decoder.state.cached_value(0).is_some(),
        "mapped vector object headers still need one load-time wrapper initialization"
    );
    assert!(
        !decoder.state.is_populated(0),
        "mapped vector slots should not run the descriptor population pass"
    );

    let value = decoder.load_value(&DumpValue::Vector(DumpHeapRef { index: 0 }));
    let slots = value.as_vector_data().unwrap();
    assert_eq!(slots.as_slice(), &[Value::fixnum(77), Value::fixnum(88)]);
}

#[test]
fn mapped_vector_slot_count_comes_from_span_for_compact_descriptors() {
    crate::test_utils::init_test_tracing();
    let mut runtime_heap = Box::new(crate::tagged::gc::TaggedHeap::new());
    crate::tagged::gc::set_tagged_heap(&mut runtime_heap);

    let slot_offset = std::mem::size_of::<VectorObj>();
    let heap = DumpTaggedHeap {
        objects: vec![DumpHeapObject::Vector(Vec::new())],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: vec![Some(DumpVecLikeSpan {
            offset: 0,
            len: std::mem::size_of::<VectorObj>() as u64,
        })],
        mapped_slots: vec![Some(DumpSlotSpan {
            offset: slot_offset as u64,
            len: 2,
        })],
    };
    let mut bytes = vec![0u8; slot_offset + 2 * std::mem::size_of::<TaggedValue>()];
    write_raw_word(&mut bytes, slot_offset, Value::fixnum(177).bits());
    write_raw_word(
        &mut bytes,
        slot_offset + std::mem::size_of::<TaggedValue>(),
        Value::fixnum(188).bits(),
    );

    let mapped = MappedHeapView::from_mut_slice(&mut bytes);
    let mut decoder = LoadDecoder::new_with_mapped_heap(&heap, Some(mapped));
    decoder.preload_tagged_heap().unwrap();

    let value = decoder.load_value(&DumpValue::Vector(DumpHeapRef { index: 0 }));
    let slots = value.as_vector_data().unwrap();
    assert_eq!(slots.as_slice(), &[Value::fixnum(177), Value::fixnum(188)]);
}

#[test]
fn load_hash_table_makes_all_dumped_entries_iterable() {
    crate::test_utils::init_test_tracing();

    let heap = DumpTaggedHeap {
        objects: Vec::new(),
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };
    let mut decoder = LoadDecoder::new(&heap);
    let table = load_hash_table(
        &mut decoder,
        &DumpLispHashTable {
            test: DumpHashTableTest::Eq,
            test_name: None,
            size: 0,
            weakness: None,
            rehash_size: 1.5,
            rehash_threshold: 0.8125,
            ordered_entries: vec![
                (DumpHashKey::Int(1), DumpValue::True, None),
                (DumpHashKey::Int(2), DumpValue::True, None),
            ],
        },
    );

    assert_eq!(table.data.len(), 2);
    assert_eq!(
        table.live_hash_keys_in_slot_order().len(),
        2,
        "loaded hash tables must keep GNU maphash traversal in sync with live entries"
    );
}

fn write_raw_word(bytes: &mut [u8], offset: usize, word: usize) {
    bytes[offset..offset + std::mem::size_of::<usize>()].copy_from_slice(&word.to_ne_bytes());
}

#[test]
fn serialized_hash_keys_follow_the_consumers_integer_representation() {
    let (minimum, maximum) = crate::tagged::value::fixnum_bounds_for_word_bits(32);
    assert_eq!(
        load_integer_hash_key_for_word_bits(minimum, 32),
        HashKey::Int(minimum)
    );
    assert_eq!(
        load_integer_hash_key_for_word_bits(maximum, 32),
        HashKey::Int(maximum)
    );

    let integer = Value::MOST_POSITIVE_FIXNUM + 1;
    let serialized = DumpHashKey::Int(integer);
    let restored_bignum = Value::bignum(malachite::Integer::from(integer));
    assert!(matches!(
        load_integer_hash_key_for_word_bits(integer, 32),
        HashKey::Bignum(_)
    ));
    assert!(hash_key_requires_restored_identity(
        &serialized,
        HashTableTest::Eq,
        32
    ));
    assert!(!hash_key_requires_restored_identity(
        &serialized,
        HashTableTest::Eql,
        32
    ));

    assert!(matches!(
        rebuild_target_hash_key(
            &serialized,
            restored_bignum,
            HashTableTest::Eq,
            32,
        ),
        Some(HashKey::Ptr(pointer)) if pointer == restored_bignum.bits()
    ));
    assert_eq!(
        rebuild_target_hash_key(&serialized, restored_bignum, HashTableTest::Eql, 32,),
        None,
        "value-based tests are normalized directly from the serialized key"
    );

    let wasm32_bignum = maximum + 1;
    let limbs = malachite::Integer::from(wasm32_bignum).to_twos_complement_limbs_asc();
    let serialized_bignum = DumpHashKey::Bignum(limbs.clone());
    assert_eq!(
        load_bignum_hash_key_for_word_bits(&limbs, 64),
        HashKey::Int(wasm32_bignum),
        "a 32-bit producer bignum that fits a 64-bit fixnum must be normalized"
    );
    assert_eq!(
        rebuild_target_hash_key(
            &serialized_bignum,
            Value::fixnum(wasm32_bignum),
            HashTableTest::Eql,
            64,
        ),
        None
    );
}


#[test]
fn cross_width_equal_key_does_not_depend_on_object_population_order() {
    crate::test_utils::init_test_tracing();
    let integer = Value::MOST_POSITIVE_FIXNUM + 1;
    let key_ref = DumpHeapRef { index: 1 };
    let heap = DumpTaggedHeap {
        objects: vec![
            DumpHeapObject::HashTable(DumpLispHashTable {
                test: DumpHashTableTest::Equal,
                test_name: None,
                size: 1,
                weakness: None,
                rehash_size: 1.5,
                rehash_threshold: 0.8125,
                ordered_entries: vec![(
                    DumpHashKey::EqualCons(
                        Box::new(DumpHashKey::Int(integer)),
                        Box::new(DumpHashKey::Nil),
                    ),
                    DumpValue::True,
                    Some(DumpValue::Cons(key_ref.clone())),
                )],
            }),
            DumpHeapObject::Cons {
                car: DumpValue::Int(integer),
                cdr: DumpValue::Nil,
            },
        ],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };
    let mut decoder = LoadDecoder::new(&heap);
    decoder.preload_tagged_heap().unwrap();

    let table = decoder.load_value(&DumpValue::HashTable(DumpHeapRef { index: 0 }));
    let lookup =
        Value::cons(Value::make_int(integer), Value::NIL).to_hash_key(&HashTableTest::Equal);
    assert_eq!(
        table
            .as_hash_table()
            .and_then(|table| table.data.get(&lookup))
            .copied(),
        Some(Value::T),
        "the key index must be derived without traversing an unpopulated cons placeholder"
    );
}


#[test]
fn cross_width_eq_key_uses_entry_value_when_snapshot_is_elided() {
    crate::test_utils::init_test_tracing();

    let integer = Value::MOST_POSITIVE_FIXNUM + 1;
    let heap = DumpTaggedHeap {
        objects: Vec::new(),
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };
    let mut decoder = LoadDecoder::new(&heap);
    let table = load_hash_table(
        &mut decoder,
        &DumpLispHashTable {
            test: DumpHashTableTest::Eq,
            test_name: None,
            size: 1,
            weakness: None,
            rehash_size: 1.5,
            rehash_threshold: 0.8125,
            ordered_entries: vec![(DumpHashKey::Int(integer), DumpValue::Int(integer), None)],
        },
    );

    let entry = table
        .entries_in_slot_order()
        .next()
        .expect("the restored entry must remain iterable");
    assert_eq!(entry.key, entry.value);
    assert_eq!(
        table.data.get(&HashKey::Ptr(entry.key.bits())),
        Some(&entry.value),
        "None means the entry value is also the original Lisp key"
    );
}


#[test]
fn producer_shaped_eq_bignum_key_demotes_to_consumer_fixnum() {
    crate::test_utils::init_test_tracing();

    let (_, wasm32_maximum) = crate::tagged::value::fixnum_bounds_for_word_bits(32);
    let integer = wasm32_maximum + 1;
    let producer_key = Value::bignum(Integer::from(integer));
    let mut encoder = DumpEncoder::new();
    let serialized_key =
        dump_hash_key(&mut encoder, &producer_key.to_hash_key(&HashTableTest::Eq));
    assert!(
        matches!(serialized_key, DumpHashKey::HeapRef(_)),
        "an eq bignum is serialized from its pointer-identity key"
    );

    let heap = encoder.finalize();
    let mut decoder = LoadDecoder::new(&heap);
    let table = load_hash_table(
        &mut decoder,
        &DumpLispHashTable {
            test: DumpHashTableTest::Eq,
            test_name: None,
            size: 1,
            weakness: None,
            rehash_size: 1.5,
            rehash_threshold: 0.8125,
            ordered_entries: vec![(
                serialized_key,
                DumpValue::True,
                Some(DumpValue::Bignum(integer.to_string())),
            )],
        },
    );

    assert_eq!(
        table.data.get(&HashKey::Int(integer)),
        Some(&Value::T),
        "the 64-bit consumer must use its immediate fixnum identity"
    );
}


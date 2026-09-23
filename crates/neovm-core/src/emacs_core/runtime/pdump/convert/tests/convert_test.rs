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

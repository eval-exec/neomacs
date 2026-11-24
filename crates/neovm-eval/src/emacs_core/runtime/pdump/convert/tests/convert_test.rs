use super::*;

/// A fixnum or a vector is not a bytecode object. The helper must refuse
/// before it casts the header to `ByteCodeObj` and writes a slot word.
#[test]
fn install_restored_bytecode_slot_object_rejects_a_non_bytecode_value() {
    crate::test_utils::init_test_tracing();
    assert!(
        !LoadDecoder::install_restored_bytecode_slot_object(
            Value::fixnum(3),
            ByteCodeSlotObject::Code,
            Value::fixnum(1),
        ),
        "a fixnum is not a bytecode object"
    );
    let vector = Value::make_vector(vec![Value::fixnum(9), Value::fixnum(8)]);
    let before = vector.as_vector_data().expect("vector slots")[0];
    assert!(
        !LoadDecoder::install_restored_bytecode_slot_object(
            vector,
            ByteCodeSlotObject::Code,
            Value::fixnum(1),
        ),
        "a vector is not a bytecode object"
    );
    assert_eq!(vector.as_vector_data().expect("vector slots")[0], before);
}

/// Same hole on the stub publish path: a non-bytecode value returns
/// before any `ByteCodeObj` read, and a vector's slots stay unchanged.
#[test]
fn materialize_and_publish_stub_refuses_a_non_bytecode_value() {
    crate::test_utils::init_test_tracing();
    materialize_and_publish_stub(Value::fixnum(3));
    let vector = Value::make_vector(vec![Value::fixnum(9), Value::fixnum(8)]);
    let before = vector.as_vector_data().expect("vector slots")[0];
    materialize_and_publish_stub(vector);
    assert_eq!(vector.as_vector_data().expect("vector slots")[0], before);
}

#[test]
#[should_panic(expected = "pdump: tree-sitter node objects are not portable")]
fn dump_value_rejects_runtime_treesit_nodes() {
    crate::test_utils::init_test_tracing();
    let node = Value::make_treesit_node(vec![
        Value::symbol("treesit-node"),
        Value::fixnum(1),
        Value::NIL,
    ]);
    DumpEncoder::new().dump_value(&node);
}

#[test]
#[should_panic(expected = "pdump: tree-sitter node objects are not portable")]
fn dump_heap_object_rejects_runtime_treesit_nodes() {
    crate::test_utils::init_test_tracing();
    let node = Value::make_treesit_node(vec![
        Value::symbol("treesit-node"),
        Value::fixnum(1),
        Value::NIL,
    ]);
    dump_heap_object_from_value(&mut DumpEncoder::new(), node);
}

#[test]
fn dump_value_keeps_same_tag_ordinary_records_portable() {
    crate::test_utils::init_test_tracing();
    let forged = Value::make_record(vec![
        Value::symbol("treesit-node"),
        Value::fixnum(1),
        Value::NIL,
    ]);
    let mut encoder = DumpEncoder::new();
    assert!(matches!(encoder.dump_value(&forged), DumpValue::Record(_)));
    assert!(
        matches!(dump_heap_object_from_value(&mut encoder, forged), DumpHeapObject::Record(slots) if slots.len() == 3)
    );
}

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
    let mut runtime_heap = Box::new(neovm_value::tagged::gc::TaggedHeap::new());
    neovm_value::tagged::gc::set_tagged_heap(&mut runtime_heap);

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
    let mut runtime_heap = Box::new(neovm_value::tagged::gc::TaggedHeap::new());
    neovm_value::tagged::gc::set_tagged_heap(&mut runtime_heap);

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
    let mut runtime_heap = Box::new(neovm_value::tagged::gc::TaggedHeap::new());
    neovm_value::tagged::gc::set_tagged_heap(&mut runtime_heap);

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
    // Proof 2 (R6): the dump never serialized in-flight state. The word and
    // the nesting count live on the object, and a restored payload wrapped
    // in a fresh object starts both at zero.
    let object = neovm_value::tagged::header::HashTableObj::new(table);
    assert_eq!(
        object.authority_word(),
        0,
        "restored tables start with a quiescent authority word"
    );
    assert_eq!(
        object.test_depth.load(std::sync::atomic::Ordering::Relaxed),
        0,
        "restored tables start outside any user test"
    );
}

/// `dump_hash_key`, `load_hash_key`, and `load_hash_key_owned` walk the cdr
/// spine in a loop. This row uses the unbounded `to_hash_key` builder, so
/// the chain is one million conses. A hundred thousand still fits the 8MB
/// test stack, so that count does not fail the recursive parent. A real
/// equal-table insert stays capped by `EQUAL_KEY_NODE_BUDGET`. `assert!`
/// avoids the derived `Debug`, which still walks the chain.
#[test]
fn dump_and_load_a_one_million_element_list_key() {
    crate::test_utils::init_test_tracing();
    let mut tail = Value::NIL;
    for i in (0..1_000_000).rev() {
        tail = Value::cons(Value::fixnum(i), tail);
    }
    let key = tail.to_hash_key(&HashTableTest::Equal);
    let dumped = dump_hash_key(&mut DumpEncoder::new(), &key);
    let heap = empty_heap();
    let loaded = load_hash_key(&mut LoadDecoder::new(&heap), &dumped);
    assert!(
        loaded == key,
        "a one-million-element list key survives dump and load"
    );
    let loaded_owned = load_hash_key_owned(&mut LoadDecoder::new(&heap), dumped);
    assert!(
        loaded_owned == key,
        "the owned load walks the same one-million-element spine"
    );
}

/// Codec bytes for a dotted tail, a nested car, a string car, a float car,
/// and a cyclic key. The expected stream is written here, not by a second
/// encoder. Read bytes load back through both loaders.
#[test]
fn mixed_equal_key_keeps_its_tag_stream() {
    crate::test_utils::init_test_tracing();
    let tail = Value::symbol("cired2-codec-tail");
    let car_a = Value::symbol("cired2-codec-a");
    let car_b = Value::symbol("cired2-codec-b");
    let tail_id = tail
        .as_symbol_id(neovm_value::tagged::value::SymbolMode::Bare)
        .expect("tail symbol");
    let a_id = car_a
        .as_symbol_id(neovm_value::tagged::value::SymbolMode::Bare)
        .expect("cycle car a");
    let b_id = car_b
        .as_symbol_id(neovm_value::tagged::value::SymbolMode::Bare)
        .expect("cycle car b");
    let max = tail_id.to_raw().max(a_id.to_raw()).max(b_id.to_raw()) as usize;
    let mut remap = vec![tail_id; max + 1];
    remap[tail_id.to_raw() as usize] = tail_id;
    remap[a_id.to_raw() as usize] = a_id;
    remap[b_id.to_raw() as usize] = b_id;
    struct ClearSymRemap;
    impl Drop for ClearSymRemap {
        fn drop(&mut self) {
            PDUMP_LOAD_SYM_REMAP.with(|slot| *slot.borrow_mut() = None);
        }
    }
    let _clear = ClearSymRemap;
    PDUMP_LOAD_SYM_REMAP.with(|slot| *slot.borrow_mut() = Some(remap));

    let nested = cons_key(HashKey::Int(2), cons_key(HashKey::Int(3), HashKey::Int(4)));
    let mixed = cons_key(
        HashKey::Int(1),
        cons_key(
            nested,
            cons_key(
                HashKey::from_str("s"),
                cons_key(HashKey::Float(5.0_f64.to_bits()), HashKey::Symbol(tail_id)),
            ),
        ),
    );
    let cyclic = cons_key(
        HashKey::Symbol(a_id),
        cons_key(HashKey::Symbol(b_id), HashKey::Cycle(0)),
    );

    let mut mixed_bytes = Vec::new();
    mixed_bytes.push(13);
    push_int(&mut mixed_bytes, 1);
    mixed_bytes.push(13);
    mixed_bytes.push(13);
    push_int(&mut mixed_bytes, 2);
    mixed_bytes.push(13);
    push_int(&mut mixed_bytes, 3);
    push_int(&mut mixed_bytes, 4);
    mixed_bytes.push(13);
    push_string_s(&mut mixed_bytes);
    mixed_bytes.push(13);
    mixed_bytes.push(3);
    mixed_bytes.extend_from_slice(&5.0_f64.to_bits().to_ne_bytes());
    push_sym(&mut mixed_bytes, tail_id.to_raw());

    let mut cyclic_bytes = Vec::new();
    cyclic_bytes.push(13);
    push_sym(&mut cyclic_bytes, a_id.to_raw());
    cyclic_bytes.push(13);
    push_sym(&mut cyclic_bytes, b_id.to_raw());
    cyclic_bytes.push(16);
    cyclic_bytes.extend_from_slice(&0_u32.to_ne_bytes());

    assert_tag_stream_loads(&mixed, &mixed_bytes);
    assert_tag_stream_loads(&cyclic, &cyclic_bytes);
}

fn cons_key(car: HashKey, cdr: HashKey) -> HashKey {
    HashKey::EqualCons(Box::new(car), Box::new(cdr))
}

fn push_int(out: &mut Vec<u8>, value: i64) {
    out.push(2);
    out.extend_from_slice(&value.to_ne_bytes());
}

fn push_sym(out: &mut Vec<u8>, id: u32) {
    out.push(5);
    out.extend_from_slice(&id.to_ne_bytes());
}

fn push_string_s(out: &mut Vec<u8>) {
    // String-content tag, then a u64 length, the byte, and SCHARS.
    out.push(23);
    out.extend_from_slice(&1_u64.to_ne_bytes());
    out.push(b's');
    out.extend_from_slice(&1_u64.to_ne_bytes());
}

fn assert_tag_stream_loads(key: &HashKey, expected: &[u8]) {
    let dumped = dump_hash_key(&mut DumpEncoder::new(), key);
    let mut bytes = Vec::new();
    super::super::object_value_codec::write_hash_key(&mut bytes, &dumped).expect("write hash key");
    assert_eq!(bytes, expected, "equal-key tag stream");
    let mut cursor = super::super::object_value_codec::Cursor::new(&bytes);
    let read = cursor.read_hash_key().expect("read hash key");
    assert!(cursor.is_empty(), "hash key bytes are consumed");
    let heap = empty_heap();
    let loaded = load_hash_key(&mut LoadDecoder::new(&heap), &read);
    assert!(loaded == *key, "borrowed load matches the key");
    let loaded_owned = load_hash_key_owned(&mut LoadDecoder::new(&heap), read);
    assert!(loaded_owned == *key, "owned load matches the key");
}

fn empty_heap() -> DumpTaggedHeap {
    DumpTaggedHeap {
        objects: Vec::new(),
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    }
}

fn write_raw_word(bytes: &mut [u8], offset: usize, word: usize) {
    bytes[offset..offset + std::mem::size_of::<usize>()].copy_from_slice(&word.to_ne_bytes());
}

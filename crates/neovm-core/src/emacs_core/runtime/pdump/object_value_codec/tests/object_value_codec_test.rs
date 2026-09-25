use super::*;

#[test]
fn heap_object_codec_round_trips_representative_objects() {
    let objects = vec![
        DumpHeapObject::Str {
            data: DumpByteData::mapped(24, 3),
            size: 3,
            size_byte: 3,
            text_props: vec![DumpStringTextPropertyRun {
                start: 0,
                end: 1,
                plist: DumpValue::Symbol(DumpSymId(7)),
            }],
        },
        DumpHeapObject::Str {
            data: DumpByteData::static_rodata(0xfeed_beef, 6),
            size: 6,
            size_byte: -2,
            text_props: Vec::new(),
        },
        DumpHeapObject::Cons {
            car: DumpValue::Int(42),
            cdr: DumpValue::Str(DumpHeapRef { index: 0 }),
        },
        DumpHeapObject::ByteCode(DumpByteCodeFunction {
            instructions: DumpByteCodeInstructions::Gnu(DumpByteData::owned(vec![0xC0, 0x87])),
            constants: vec![DumpValue::Bignum("12345678901234567890".into())],
            max_stack: 4,
            params: DumpLambdaParams {
                required: vec![DumpSymId(1)],
                optional: vec![DumpSymId(2)],
                rest: Some(DumpSymId(3)),
            },
            arglist: Some(DumpValue::Nil),
            lexical: true,
            env: Some(DumpValue::Vector(DumpHeapRef { index: 4 })),
            docstring: Some(DumpLispString {
                data: b"doc".to_vec(),
                size: 3,
                size_byte: 3,
            }),
            doc_form: Some(DumpValue::True),
            interactive: Some(DumpValue::Nil),
            closure_slot_count: 6,
            extra_slots: vec![],
            ops_sealed: false,
            code_object: Some(DumpValue::Str(DumpHeapRef { index: 0 })),
            constants_object: Some(DumpValue::Vector(DumpHeapRef { index: 4 })),
        }),
        DumpHeapObject::ByteCode(DumpByteCodeFunction {
            instructions: DumpByteCodeInstructions::Decoded(vec![
                Op::Constant(1),
                // CallBuiltinSym needs the load-time symbol remap TLS
                // (installed by real loads); CallBuiltin has the same
                // record shape and keeps the tag+arg+extra encode covered.
                Op::CallBuiltin(9, 2),
                Op::Return,
            ]),
            constants: vec![DumpValue::Bignum("12345678901234567890".into())],
            max_stack: 4,
            params: DumpLambdaParams {
                required: vec![DumpSymId(1)],
                optional: vec![DumpSymId(2)],
                rest: Some(DumpSymId(3)),
            },
            arglist: Some(DumpValue::Nil),
            lexical: true,
            env: Some(DumpValue::Vector(DumpHeapRef { index: 4 })),
            docstring: Some(DumpLispString {
                data: b"doc".to_vec(),
                size: 3,
                size_byte: 3,
            }),
            doc_form: Some(DumpValue::True),
            interactive: Some(DumpValue::Nil),
            closure_slot_count: 6,
            extra_slots: vec![],
            ops_sealed: false,
            code_object: None,
            constants_object: None,
        }),
        DumpHeapObject::HashTable(DumpLispHashTable {
            test: DumpHashTableTest::Equal,
            test_name: Some(DumpSymId(11)),
            size: 17,
            weakness: Some(DumpHashTableWeakness::KeyOrValue),
            rehash_size: 1.5,
            rehash_threshold: 0.8,
            ordered_entries: vec![
                (
                    DumpHashKey::EqualCons(
                        Box::new(DumpHashKey::Text("a".into())),
                        Box::new(DumpHashKey::Cycle(1)),
                    ),
                    DumpValue::Cons(DumpHeapRef { index: 1 }),
                    Some(DumpValue::Int(8)),
                ),
                (
                    DumpHashKey::Bignum(vec![0, 0xFFFF_FFFF_FFFF_FFFF, 1]),
                    DumpValue::Int(9),
                    None,
                ),
                (
                    DumpHashKey::ByteCode(vec![
                        DumpByteCodeKeyPart::ObservableSlotCount(5),
                        DumpByteCodeKeyPart::Value(DumpHashKey::Int(257)),
                        DumpByteCodeKeyPart::Bytes(vec![0xC0, 0x87]),
                        DumpByteCodeKeyPart::Ops(vec![Op::Constant(0), Op::Return]),
                        DumpByteCodeKeyPart::Values(vec![DumpHashKey::Int(42)]),
                        DumpByteCodeKeyPart::Text {
                            char_count: 3,
                            bytes: b"doc".to_vec(),
                        },
                        DumpByteCodeKeyPart::Absent,
                    ]),
                    DumpValue::Int(10),
                    None,
                ),
                (DumpHashKey::Char('x'), DumpValue::Int(8), None),
                (DumpHashKey::HeapRef(1), DumpValue::True, None),
            ],
        }),
        DumpHeapObject::Marker(DumpMarker {
            buffer: Some(DumpBufferId(5)),
            insertion_type: true,
            marker_id: Some(6),
            bytepos: 7,
            charpos: 8,
            last_position_valid: true,
        }),
        DumpHeapObject::Overlay(DumpOverlay {
            serial: 17,
            plist: DumpValue::Nil,
            buffer: Some(DumpBufferId(9)),
            start: 10,
            end: 11,
            front_advance: true,
            rear_advance: false,
        }),
        DumpHeapObject::Subr {
            name: DumpNameId(13),
            min_args: 1,
            max_args: Some(2),
        },
    ];

    let mut bytes = Vec::new();
    for object in &objects {
        write_heap_object(&mut bytes, object).expect("encode heap object");
    }
    let mut cursor = Cursor::new(&bytes);
    let mut decoded = Vec::new();
    for _ in 0..objects.len() {
        decoded.push(cursor.read_heap_object().expect("decode heap object"));
    }
    assert!(cursor.is_empty());

    assert_eq!(format!("{decoded:?}"), format!("{objects:?}"));
}

#[test]
fn heap_object_codec_rejects_bad_tag() {
    let bytes = [u8::MAX];
    let mut cursor = Cursor::new(&bytes);
    let err = cursor
        .read_heap_object()
        .expect_err("bad object tag should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}

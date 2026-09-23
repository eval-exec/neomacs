use super::*;
use crate::emacs_core::pdump::types::DumpTaggedHeap;

#[test]
fn mapped_veclike_tags_decode_the_full_runtime_domain() {
    let variants = [
        VecLikeType::Vector,
        VecLikeType::HashTable,
        VecLikeType::Lambda,
        VecLikeType::Macro,
        VecLikeType::ByteCode,
        VecLikeType::Record,
        VecLikeType::Overlay,
        VecLikeType::Marker,
        VecLikeType::Buffer,
        VecLikeType::Window,
        VecLikeType::Frame,
        VecLikeType::Timer,
        VecLikeType::Subr,
        VecLikeType::Xwidget,
        VecLikeType::XwidgetView,
        VecLikeType::SurfaceHandle,
        VecLikeType::VideoHandle,
        VecLikeType::Bignum,
        VecLikeType::SymbolWithPos,
        VecLikeType::Finalizer,
        VecLikeType::Sqlite,
        VecLikeType::UserPtr,
        VecLikeType::ModuleFunction,
        VecLikeType::CharTable,
        VecLikeType::SubCharTable,
        VecLikeType::Obarray,
    ];

    for variant in variants {
        assert_eq!(veclike_type_from_tag(u8::from(variant)).unwrap(), variant);
    }

    assert!(matches!(
        veclike_type_from_tag(u8::MAX),
        Err(DumpError::ImageFormatError(_))
    ));
}

#[test]
fn extracts_string_bytes_into_mapped_heap_section() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![DumpHeapObject::Str {
            data: DumpByteData::owned(b"abc".to_vec()),
            size: 3,
            size_byte: 3,
            text_props: Vec::new(),
        }],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };

    let heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );
    assert_eq!(tagged_heap.mapped_strings.len(), 1);
    let string_span = tagged_heap.mapped_strings[0].expect("string object span");
    assert_eq!(string_span.offset, 0);
    assert_eq!(string_span.len as usize, std::mem::size_of::<StringObj>());
    let DumpHeapObject::Str { data, .. } = &tagged_heap.objects[0] else {
        panic!("expected string object");
    };

    let view = MappedHeapView::from_slice(&heap.bytes);
    let mapped = view.bytes(data).unwrap();
    let mapped_bytes = unsafe { std::slice::from_raw_parts(mapped.ptr, mapped.len) };
    assert_eq!(mapped_bytes, b"abc");
    assert_eq!(unsafe { *mapped.ptr.add(mapped.len) }, 0);

    let object_offset = string_span.offset as usize;
    let data_field_offset = object_offset + std::mem::offset_of!(RawStringObj, data);
    assert_eq!(
        heap.bytes[object_offset + 1],
        u8::from(HeapObjectKind::String)
    );
    assert_eq!(
        read_usize(
            &heap.bytes,
            object_offset + std::mem::offset_of!(RawStringObj, size)
        ),
        3
    );
    assert_eq!(
        read_i64(
            &heap.bytes,
            object_offset + std::mem::offset_of!(RawStringObj, size_byte)
        ),
        3
    );
    assert_eq!(
        read_usize(&heap.bytes, data_field_offset),
        mapped.ptr as usize - heap.bytes.as_ptr() as usize
    );
    assert!(
        heap.relocations
            .iter()
            .any(
                |relocation| relocation.location_offset == data_field_offset as u64
                    && relocation.addend == 0
            )
    );
}

#[test]
fn empty_strings_still_create_heap_section_anchor() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![DumpHeapObject::Str {
            data: DumpByteData::owned(Vec::new()),
            size: 0,
            size_byte: 0,
            text_props: Vec::new(),
        }],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };

    let heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );
    assert!(heap.bytes.len() > std::mem::size_of::<StringObj>());
    let DumpHeapObject::Str { data, .. } = &tagged_heap.objects[0] else {
        panic!("expected string object");
    };
    let view = MappedHeapView::from_slice(&heap.bytes);
    let mapped = view.bytes(data).unwrap();
    assert_eq!(mapped.len, 0);
    assert!(mapped.ptr as usize >= heap.bytes.as_ptr() as usize);
    assert_eq!(unsafe { *mapped.ptr }, 0);
}

#[test]
fn reserves_aligned_slot_spans_for_vectorlike_objects() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![DumpHeapObject::Vector(vec![
            crate::emacs_core::pdump::types::DumpValue::Int(1),
            crate::emacs_core::pdump::types::DumpValue::Int(2),
        ])],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };

    let mut heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );
    assert!(heap.bytes.len() >= std::mem::size_of::<VectorObj>());
    assert_eq!(tagged_heap.mapped_veclikes.len(), 1);
    let object_span = tagged_heap.mapped_veclikes[0].expect("vector object span");
    assert_eq!(object_span.offset, 0);
    assert_eq!(object_span.len as usize, std::mem::size_of::<VectorObj>());
    assert_eq!(tagged_heap.mapped_slots.len(), 1);
    let span = tagged_heap.mapped_slots[0].expect("vector slot span");
    assert!(span.offset as usize >= std::mem::size_of::<VectorObj>());
    assert_eq!(span.len, 2);
    let view = MappedHeapView::from_mut_slice(&mut heap.bytes);
    let header = view.veclike_header_mut(object_span).unwrap();
    assert_eq!(header.cast::<u8>(), heap.bytes.as_mut_ptr());
    assert_eq!(view.veclike_type(object_span).unwrap(), VecLikeType::Vector);
    let ptr = view
        .typed_object_mut::<VectorObj>(object_span, "vector")
        .unwrap();
    assert_eq!(ptr.cast::<u8>(), heap.bytes.as_mut_ptr());
}

#[test]
fn reserves_mapped_cons_cells_as_heap_objects() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![
            DumpHeapObject::Cons {
                car: crate::emacs_core::pdump::types::DumpValue::Int(1),
                cdr: crate::emacs_core::pdump::types::DumpValue::Int(2),
            },
            DumpHeapObject::Cons {
                car: crate::emacs_core::pdump::types::DumpValue::Int(3),
                cdr: crate::emacs_core::pdump::types::DumpValue::Nil,
            },
        ],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };

    let mut heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );
    assert_eq!(heap.bytes.len(), 2 * std::mem::size_of::<ConsCell>());
    assert_eq!(tagged_heap.mapped_cons.len(), 2);
    let first = tagged_heap.mapped_cons[0].expect("first cons span");
    let second = tagged_heap.mapped_cons[1].expect("second cons span");
    assert_eq!(first.offset, 0);
    assert_eq!(second.offset as usize, std::mem::size_of::<ConsCell>());

    let view = MappedHeapView::from_mut_slice(&mut heap.bytes);
    let ptr = view.cons_cell_mut(first).unwrap();
    assert_eq!(ptr.cast::<u8>(), heap.bytes.as_mut_ptr());

    assert_eq!(
        read_usize(&heap.bytes, first.offset as usize),
        TaggedValue::fixnum(1).bits()
    );
    assert_eq!(
        read_usize(
            &heap.bytes,
            first.offset as usize + std::mem::size_of::<TaggedValue>()
        ),
        TaggedValue::fixnum(2).bits()
    );
}

#[test]
fn reserves_mapped_float_objects_as_heap_objects() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![DumpHeapObject::Float(1.0), DumpHeapObject::Float(2.0)],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };

    let mut heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );
    assert_eq!(heap.bytes.len(), 2 * std::mem::size_of::<FloatObj>());
    assert_eq!(tagged_heap.mapped_floats.len(), 2);
    let first = tagged_heap.mapped_floats[0].expect("first float span");
    let second = tagged_heap.mapped_floats[1].expect("second float span");
    assert_eq!(first.offset, 0);
    assert_eq!(second.offset as usize, std::mem::size_of::<FloatObj>());

    let view = MappedHeapView::from_mut_slice(&mut heap.bytes);
    let ptr = view.float_obj_mut(first).unwrap();
    assert_eq!(ptr.cast::<u8>(), heap.bytes.as_mut_ptr());

    assert_eq!(
        heap.bytes[first.offset as usize + 1],
        u8::from(HeapObjectKind::Float)
    );
    let value_offset = first.offset as usize + std::mem::size_of::<RawGcHeader>();
    let value = f64::from_ne_bytes(
        heap.bytes[value_offset..value_offset + std::mem::size_of::<f64>()]
            .try_into()
            .unwrap(),
    );
    assert_eq!(value, 1.0);
}

#[test]
fn emits_tagged_relocations_for_heap_values_in_raw_cons_payload() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![
            DumpHeapObject::Str {
                data: DumpByteData::owned(b"child".to_vec()),
                size: 5,
                size_byte: 5,
                text_props: Vec::new(),
            },
            DumpHeapObject::Cons {
                car: crate::emacs_core::pdump::types::DumpValue::Str(
                    crate::emacs_core::pdump::types::DumpHeapRef { index: 0 },
                ),
                cdr: crate::emacs_core::pdump::types::DumpValue::Nil,
            },
        ],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };

    let heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );
    let cons_span = tagged_heap.mapped_cons[1].expect("mapped cons");
    let string_span = tagged_heap.mapped_strings[0].expect("mapped string");

    assert!(
        heap.relocations
            .iter()
            .any(|relocation| relocation.location_offset == cons_span.offset
                && relocation.addend == TAG_STRING as u8)
    );
    assert_eq!(
        read_usize(&heap.bytes, cons_span.offset as usize),
        string_span.offset as usize
    );
}

#[test]
fn writes_raw_vector_slots_into_mapped_heap_payload() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![DumpHeapObject::Vector(vec![
            crate::emacs_core::pdump::types::DumpValue::Int(11),
            crate::emacs_core::pdump::types::DumpValue::True,
        ])],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };

    let heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );
    let slots = tagged_heap.mapped_slots[0].expect("mapped slots");
    let second = slots.offset as usize + std::mem::size_of::<TaggedValue>();

    assert_eq!(
        read_usize(&heap.bytes, slots.offset as usize),
        TaggedValue::fixnum(11).bits()
    );
    assert_eq!(read_usize(&heap.bytes, second), TaggedValue::T.bits());
}

#[test]
fn emits_value_fixups_for_raw_slots_that_need_runtime_remap() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![DumpHeapObject::Vector(vec![
            crate::emacs_core::pdump::types::DumpValue::Symbol(
                crate::emacs_core::pdump::types::DumpSymId(42),
            ),
            crate::emacs_core::pdump::types::DumpValue::Subr(
                crate::emacs_core::pdump::types::DumpNameId(7),
            ),
        ])],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };

    let heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );
    let slots = tagged_heap.mapped_slots[0].expect("mapped slots");

    assert_eq!(heap.value_fixups.len(), 2);
    assert!(matches!(
        heap.value_fixups[0],
        RawValueFixup::Symbol { location_offset } if location_offset == slots.offset
    ));
    assert!(matches!(
        heap.value_fixups[1],
        RawValueFixup::Value {
            location_offset,
            value: crate::emacs_core::pdump::types::DumpValue::Subr(_),
        } if location_offset == slots.offset + std::mem::size_of::<TaggedValue>() as u64
    ));
    // v12: the symbol word is BAKED as Value::symbol bits over the
    // dump-local id, not the raw id.
    assert_eq!(
        read_usize(&heap.bytes, slots.offset as usize),
        TaggedValue::from_sym_id(crate::emacs_core::intern::SymId(42)).bits()
    );
}

#[test]
fn rebuild_heap_metadata_matches_extracted_layout() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![
            DumpHeapObject::Str {
                data: DumpByteData::owned(b"abc".to_vec()),
                size: 3,
                size_byte: 3,
                text_props: Vec::new(),
            },
            DumpHeapObject::Vector(vec![
                crate::emacs_core::pdump::types::DumpValue::Int(1),
                crate::emacs_core::pdump::types::DumpValue::Nil,
            ]),
            DumpHeapObject::Cons {
                car: crate::emacs_core::pdump::types::DumpValue::Int(2),
                cdr: crate::emacs_core::pdump::types::DumpValue::Nil,
            },
        ],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };
    let _heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );
    let expected_cons = tagged_heap.mapped_cons.clone();
    let expected_strings = tagged_heap.mapped_strings.clone();
    let expected_veclikes = tagged_heap.mapped_veclikes.clone();
    let expected_slots = tagged_heap.mapped_slots.clone();

    clear_heap_metadata(&mut tagged_heap);
    rebuild_heap_metadata(&mut tagged_heap).expect("rebuild heap metadata");

    assert_eq!(tagged_heap.mapped_cons, expected_cons);
    assert_eq!(tagged_heap.mapped_strings, expected_strings);
    assert_eq!(tagged_heap.mapped_veclikes, expected_veclikes);
    assert_eq!(tagged_heap.mapped_slots, expected_slots);
}

#[test]
fn reserves_mapped_vectorlike_headers_as_heap_objects() {
    let mut tagged_heap = DumpTaggedHeap {
        objects: vec![
            DumpHeapObject::Vector(Vec::new()),
            DumpHeapObject::Record(Vec::new()),
            DumpHeapObject::Lambda(Vec::new()),
            DumpHeapObject::Macro(Vec::new()),
        ],
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };

    let heap = extract_tagged_heap_payloads(
        &mut tagged_heap,
        &mut crate::emacs_core::pdump::types::DumpObarray {
            symbols: Vec::new(),
            global_members: Vec::new(),
            function_unbound: Vec::new(),
            function_epoch: 0,
            plain_rows: None,
        },
    );

    assert_eq!(tagged_heap.mapped_veclikes.len(), 4);
    assert_eq!(
        tagged_heap.mapped_veclikes[0].unwrap().len as usize,
        std::mem::size_of::<VectorObj>()
    );
    assert_eq!(
        tagged_heap.mapped_veclikes[1].unwrap().len as usize,
        std::mem::size_of::<RecordObj>()
    );
    assert_eq!(
        tagged_heap.mapped_veclikes[2].unwrap().len as usize,
        std::mem::size_of::<LambdaObj>()
    );
    assert_eq!(
        tagged_heap.mapped_veclikes[3].unwrap().len as usize,
        std::mem::size_of::<MacroObj>()
    );
    assert!(heap.bytes.len() >= std::mem::size_of::<VectorObj>());
}

fn read_usize(bytes: &[u8], offset: usize) -> usize {
    usize::from_ne_bytes(
        bytes[offset..offset + std::mem::size_of::<usize>()]
            .try_into()
            .unwrap(),
    )
}

fn read_i64(bytes: &[u8], offset: usize) -> i64 {
    i64::from_ne_bytes(
        bytes[offset..offset + std::mem::size_of::<i64>()]
            .try_into()
            .unwrap(),
    )
}

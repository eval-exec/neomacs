use super::*;
use crate::tagged::header::{GcHeader, LambdaObj, MacroObj, RecordObj, VectorObj};

#[test]
fn object_extra_is_sparse_for_category_a_descriptors() {
    let bytes = build_object_extra(
        &[
            DumpHeapObject::Cons {
                car: DumpValue::Nil,
                cdr: DumpValue::True,
            },
            DumpHeapObject::Vector(vec![DumpValue::Nil, DumpValue::True]),
            DumpHeapObject::Free,
        ],
        &[],
    )
    .expect("build object extra");

    let extras = load_object_extra(&bytes).expect("load object extra");
    assert_eq!(extras.len(), 1);
    assert!(matches!(extras[0], ObjectExtra::Free));
}

#[test]
fn object_extra_loads_sparse_heap_objects_from_spans() {
    let objects = vec![
        DumpHeapObject::Cons {
            car: DumpValue::True,
            cdr: DumpValue::Nil,
        },
        DumpHeapObject::Free,
    ];
    let bytes = build_object_extra(&objects, &[]).expect("build object extra");
    let heap = DumpTaggedHeap {
        objects,
        mapped_cons: vec![Some(DumpConsSpan { offset: 0 }), None],
        mapped_floats: vec![None, None],
        mapped_strings: vec![None, None],
        mapped_veclikes: vec![None, None],
        mapped_slots: vec![None, None],
    };
    let spans = LoadedSpans::from_heap(&heap);

    let objects = load_file_object_descriptors(&bytes, &spans, None)
        .expect("load heap objects from sparse extra");

    assert!(objects.get(0).is_none());
    assert!(matches!(objects.get(1), Some(DumpHeapObject::Free)));
}

#[test]
fn object_extra_round_trips_static_rodata_string_descriptor() {
    let objects = vec![DumpHeapObject::Str {
        data: DumpByteData::static_rodata(0x1234_5678, 7),
        size: 7,
        size_byte: -2,
        text_props: Vec::new(),
    }];
    let bytes = build_object_extra(&objects, &[]).expect("build object extra");
    let extras = load_object_extra(&bytes).expect("load object extra");

    assert!(matches!(
        &extras[0],
        ObjectExtra::String {
            size: 7,
            size_byte: -2,
            byte_data: DumpByteData::StaticRoData { key: 0x1234_5678, len: 7 },
            text_props,
        } if text_props.is_empty()
    ));
}

#[test]
fn compact_object_extra_leaves_mapped_vectorlike_objects_self_contained() {
    let objects = vec![
        DumpHeapObject::Vector(vec![DumpValue::Nil, DumpValue::True]),
        DumpHeapObject::Lambda(vec![DumpValue::Nil, DumpValue::True]),
        DumpHeapObject::Macro(vec![DumpValue::Nil, DumpValue::True]),
        DumpHeapObject::Record(vec![DumpValue::Nil, DumpValue::True]),
    ];
    let bytes = build_object_extra(&objects, &[]).expect("build object extra");
    assert_eq!(bytes.len(), HEADER_SIZE);

    let mut offset = 0u64;
    let vector_span = reserve_test_object::<VectorObj>(&mut offset);
    let lambda_span = reserve_test_object::<LambdaObj>(&mut offset);
    let macro_span = reserve_test_object::<MacroObj>(&mut offset);
    let record_span = reserve_test_object::<RecordObj>(&mut offset);
    let mut heap_bytes = vec![0u8; offset as usize];
    write_test_veclike_type(&mut heap_bytes, vector_span, VecLikeType::Vector);
    write_test_veclike_type(&mut heap_bytes, lambda_span, VecLikeType::Lambda);
    write_test_veclike_type(&mut heap_bytes, macro_span, VecLikeType::Macro);
    write_test_veclike_type(&mut heap_bytes, record_span, VecLikeType::Record);

    let heap = DumpTaggedHeap {
        objects,
        mapped_cons: vec![None; 4],
        mapped_floats: vec![None; 4],
        mapped_strings: vec![None; 4],
        mapped_veclikes: vec![
            Some(vector_span),
            Some(lambda_span),
            Some(macro_span),
            Some(record_span),
        ],
        mapped_slots: vec![None; 4],
    };
    let spans = LoadedSpans::from_heap(&heap);
    let mapped_heap = MappedHeapView::from_mut_slice(&mut heap_bytes);

    let objects = load_file_object_descriptors(&bytes, &spans, Some(mapped_heap))
        .expect("load compact heap objects from extra");

    assert_eq!(objects.len(), 4);
    assert_eq!(objects.descriptor_count(), 0);
    assert!((0..objects.len()).all(|index| objects.get(index).is_none()));
}

#[test]
fn object_extra_rejects_removed_none_tag() {
    let mut bytes = build_object_extra(&[DumpHeapObject::Free], &[]).expect("build object extra");
    bytes[HEADER_SIZE + 4] = 100;

    let err = load_object_extra(&bytes).expect_err("removed NONE tag should be rejected");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}

fn reserve_test_object<T>(offset: &mut u64) -> DumpVecLikeSpan {
    let span = DumpVecLikeSpan {
        offset: *offset,
        len: std::mem::size_of::<T>() as u64,
    };
    *offset += span.len;
    span
}

fn write_test_veclike_type(bytes: &mut [u8], span: DumpVecLikeSpan, type_tag: VecLikeType) {
    bytes[span.offset as usize + std::mem::size_of::<GcHeader>()] = u8::from(type_tag);
}

use super::*;

fn sample_heap() -> DumpTaggedHeap {
    DumpTaggedHeap {
        objects: vec![
            DumpHeapObject::Cons {
                car: DumpValue::Int(1),
                cdr: DumpValue::Nil,
            },
            DumpHeapObject::Float(3.125),
            DumpHeapObject::Free,
            DumpHeapObject::Vector(vec![DumpValue::Nil, DumpValue::True]),
            DumpHeapObject::Str {
                data: DumpByteData::owned(b"hello".to_vec()),
                size: 5,
                size_byte: 5,
                text_props: vec![],
            },
        ],
        mapped_cons: vec![Some(DumpConsSpan { offset: 0 }), None, None, None, None],
        mapped_floats: vec![None, Some(DumpFloatSpan { offset: 32 }), None, None, None],
        mapped_strings: vec![
            None,
            None,
            None,
            None,
            Some(DumpStringSpan {
                offset: 48,
                len: 16,
            }),
        ],
        mapped_veclikes: vec![
            None,
            None,
            None,
            Some(DumpVecLikeSpan {
                offset: 64,
                len: 24,
            }),
            None,
        ],
        mapped_slots: vec![
            None,
            None,
            None,
            Some(DumpSlotSpan {
                offset: 88,
                len: 16,
            }),
            None,
        ],
    }
}

/// The tag-only count must agree with decoding every span.
///
/// `count_vectorlikes_and_strings` reads one byte per row instead of
/// decoding the row, which is only correct while `SpanRow` keeps `tag` as
/// its leading byte.  Re-derive the answer the slow way and compare, so a
/// field reordering fails here rather than silently mis-sizing the
/// registries at load.
#[test]
fn the_tag_only_span_count_agrees_with_decoding_every_span() {
    let heap = sample_heap();
    let bytes = build_object_starts(&heap).unwrap();
    let spans = load_object_starts(&bytes).unwrap();

    let (mut vectorlikes, mut strings) = (0usize, 0usize);
    for (_index, record) in spans.iter() {
        match record {
            LoadedObjectSpan::Vectorlike { .. } => vectorlikes += 1,
            LoadedObjectSpan::String { .. } => strings += 1,
            _ => {}
        }
    }

    assert_eq!(
        spans.count_vectorlikes_and_strings(),
        (vectorlikes, strings),
        "tag-only count disagrees with the decoded spans"
    );
    assert!(
        vectorlikes > 0 && strings > 0,
        "the sample must contain both kinds for this to prove anything"
    );
}

#[test]
fn object_starts_round_trips() {
    let heap = sample_heap();
    let bytes = build_object_starts(&heap).unwrap();
    let spans = load_object_starts(&bytes).unwrap();
    assert_eq!(spans.len(), 5);
    assert_eq!(spans.cons(0), Some(DumpConsSpan { offset: 0 }));
    assert!(spans.cons(1).is_none());
    assert_eq!(spans.float(1), Some(DumpFloatSpan { offset: 32 }));
    assert_eq!(
        spans.string(4),
        Some(DumpStringSpan {
            offset: 48,
            len: 16
        })
    );
    assert_eq!(
        spans.vectorlike(3),
        Some(DumpVecLikeSpan {
            offset: 64,
            len: 24
        })
    );
    assert_eq!(
        spans.slots(3),
        Some(DumpSlotSpan {
            offset: 88,
            len: 16
        })
    );
    assert_eq!(spans.get(2), LoadedObjectSpan::Unmapped);
}

//! The header's generation byte and THE generation predicate (P3.1 C2.1),
//! and the byte map every later header commit builds on (P3.0 §3.1).

use super::*;

fn header_of(value: TaggedValue) -> *mut GcHeader {
    TaggedHeap::value_heap_addr(value).expect("a heap object") as *mut GcHeader
}

/// A new header is all zero outside `kind`: the reserved bytes, the
/// generation byte and `remembered` (the pdump image writes the same).
#[test]
fn a_new_header_zeroes_every_byte_but_kind() {
    let header = GcHeader::new(HeapObjectKind::VecLike);
    // SAFETY: the header is 16 plain bytes (a pointer and eight bytes).
    let bytes: [u8; 16] = unsafe { std::mem::transmute(header) };
    assert_eq!(bytes[1], u8::from(HeapObjectKind::VecLike));
    for (i, byte) in bytes.iter().enumerate() {
        if i != 1 {
            assert_eq!(*byte, 0, "byte {i}");
        }
    }
    let header = GcHeader::new(HeapObjectKind::Float);
    assert_eq!(header.generation, GenBits::NONE);
    assert!(!header.generation.permanent());
}

/// The predicate: a young object is black in no scope; an old object only
/// in a young collection; a permanent in every scope. Permanent implies
/// tenured, so a young-scope answer is exactly the `tenured` byte.
#[test]
fn black_by_generation_per_scope() {
    let mut young = GcHeader::new(HeapObjectKind::VecLike);
    assert!(!young.black_by_generation(CollectionScope::Young));
    assert!(!young.black_by_generation(CollectionScope::Full));

    // An old (promoted, not permanent) object: only P3.1's minors make one.
    young.tenured = true;
    let old = young;
    assert!(old.black_by_generation(CollectionScope::Young));
    assert!(!old.black_by_generation(CollectionScope::Full));

    let mut permanent = GcHeader::new(HeapObjectKind::String);
    permanent.make_permanent();
    assert!(permanent.tenured);
    assert!(permanent.generation.permanent());
    assert!(permanent.black_by_generation(CollectionScope::Young));
    assert!(permanent.black_by_generation(CollectionScope::Full));

    let heap = TaggedHeap::new();
    assert_eq!(heap.collection_scope(), CollectionScope::Young);
}

/// The first partition cycle promotes every survivor — boxed objects and
/// page slots of every class — to tenured AND permanent, and nothing else;
/// the permanents then stay black across later cycles of both parities.
#[test]
fn the_first_partition_cycle_makes_survivors_permanent() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap.extend_dump_span(4096, 16);
    let table = heap.alloc_hash_table(crate::emacs_core::value::LispHashTable::new(
        crate::emacs_core::value::HashTableTest::Eq,
    ));
    let vector = heap.alloc_vector(vec![TaggedValue::NIL; 2]);
    let string = heap.alloc_string(crate::heap_types::LispString::from_utf8("perm"));
    let float = heap.alloc_float(1.5);
    let record = heap.alloc_record(vec![TaggedValue::T]);
    let lambda = heap.alloc_lambda(vec![TaggedValue::NIL]);
    let kept = [table, vector, string, float, record, lambda];
    let mut root = TaggedValue::NIL;
    for &value in &kept {
        root = heap.alloc_cons(value, root);
    }
    heap.collect_exact(std::iter::once(root));
    for &value in &kept {
        let header = unsafe { &*header_of(value) };
        assert!(header.tenured, "{value:?}");
        assert!(header.generation.permanent(), "{value:?}");
        assert!(header.black_by_generation(CollectionScope::Full));
    }
    // A young object allocated after the promotion is neither.
    let young = heap.alloc_vector(vec![TaggedValue::NIL]);
    let young_header = unsafe { &*header_of(young) };
    assert!(!young_header.tenured);
    assert_eq!(young_header.generation, GenBits::NONE);
    // Later cycles leave the permanents alone (tenured-skip unchanged).
    let root2 = heap.alloc_cons(young, root);
    for _ in 0..3 {
        heap.collect_exact(std::iter::once(root2));
        for &value in &kept {
            assert!(heap.is_value_marked(value));
            assert!(unsafe { &*header_of(value) }.generation.permanent());
        }
    }
    assert!(heap.is_value_marked(young));
}

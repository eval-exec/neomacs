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

/// The tri-state mark byte (P3.1 C2.2): 0 is unmarked at rest under both
/// parities; a parity value is marked only at that parity; a claim moves
/// the byte to the claiming parity from rest or from the other parity, once.
#[test]
fn the_mark_byte_is_tri_state() {
    let header = GcHeader::new(HeapObjectKind::VecLike);
    assert_eq!(header.raw_mark(), UNMARKED_AT_REST);
    assert!(!header.is_marked());
    for parity in [MarkParity::One, MarkParity::Two] {
        assert!(
            !header.is_marked_at(parity),
            "at rest is white at {parity:?}"
        );
    }
    assert!(header.mark_claim_at(MarkParity::One), "claim from rest");
    assert!(
        !header.mark_claim_at(MarkParity::One),
        "a second claim loses"
    );
    assert!(header.is_marked_at(MarkParity::One));
    assert!(!header.is_marked_at(MarkParity::Two));
    assert!(
        header.mark_claim_at(MarkParity::Two),
        "claim from the other parity"
    );
    assert_eq!(header.raw_mark(), MarkParity::Two.byte());
    let born = GcHeader::new_marked(HeapObjectKind::Float, MarkParity::One);
    assert!(born.is_marked_at(MarkParity::One));
    assert!(!born.is_marked_at(MarkParity::Two));
    assert_eq!(MarkParity::One.flip(), MarkParity::Two);
    assert_eq!(MarkParity::Two.flip(), MarkParity::One);
}

/// The heap's parity starts at `Two` so the bootstrap cycle runs at `One`,
/// alternates every collection, and objects born between collections are
/// white at the next one; a mark byte reset to rest reads white across two
/// consecutive collections of different parities (the property P3.1's
/// old objects rest on, GEN-3).
#[test]
fn parity_alternates_and_rest_is_white_under_both() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    assert_eq!(heap.mark_parity, MarkParity::Two);
    let v = heap.alloc_vector(vec![TaggedValue::NIL]);
    let header = unsafe { &*header_of(v) };
    assert_eq!(header.raw_mark(), MarkParity::Two.byte(), "born at parity");
    heap.collect_exact(std::iter::once(v));
    assert_eq!(heap.mark_parity, MarkParity::One);
    assert!(
        header.is_marked_at(MarkParity::One),
        "traced at the new parity"
    );
    heap.collect_exact(std::iter::once(v));
    assert_eq!(heap.mark_parity, MarkParity::Two);
    assert!(header.is_marked_at(MarkParity::Two));
    // At rest, the object reads white under the next two parities in turn.
    header.marked.store(UNMARKED_AT_REST, Ordering::Relaxed);
    for parity in [heap.mark_parity.flip(), heap.mark_parity] {
        assert!(!header.is_marked_at(parity));
    }
    heap.collect_exact(std::iter::once(v));
    assert!(heap.is_value_marked(v), "traced again from rest");
}

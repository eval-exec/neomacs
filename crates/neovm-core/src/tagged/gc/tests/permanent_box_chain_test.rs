//! Every reconstructed boxed image owner must survive the partition splice
//! and participate in the initial permanent-to-collectible edge scan.
use super::*;
use crate::emacs_core::value::{HashTableTest, LispHashTable};

#[test]
fn every_boxed_image_resident_keeps_its_cons_child_after_partition() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    heap.extend_dump_span(4096, 16);
    let children = [
        heap.alloc_cons(TaggedValue::fixnum(101), TaggedValue::NIL),
        heap.alloc_cons(TaggedValue::fixnum(202), TaggedValue::NIL),
    ];
    let owners = children.map(|child| {
        let mut table = LispHashTable::new(HashTableTest::Eq);
        table.insert(
            TaggedValue::T.to_hash_key(&HashTableTest::Eq),
            TaggedValue::T,
            child,
        );
        let owner = heap.alloc_hash_table(table);
        assert_eq!(heap.note_image_object(owner), ImageStamp::Stamped);
        owner
    });
    // Keep the children's block allocated even if one edge is missed, so
    // the ownership probe observes a freed cell rather than address reuse.
    let anchor = heap.alloc_cons(TaggedValue::fixnum(303), TaggedValue::NIL);
    heap.collect_exact(owners.into_iter().chain([anchor]));
    assert!(heap.dump_blackened);

    // These roots keep only the image owners and the block anchor. Image
    // owners are black; their children depend on the initial edge scan.
    for cycle in 0..3 {
        heap.collect_exact(owners.into_iter().chain([anchor]));
        for child in children {
            assert!(
                heap.owns_heap_value_for_test(child),
                "boxed image resident lost its cons child in cycle {cycle}"
            );
        }
    }
    // A later write makes the previously missed owner enter the persistent
    // remembered set. Its original children must still be allocated when
    // that set re-traces them (the byte-compile repro's resurrection path).
    for owner in owners {
        crate::tagged::mutate::with_hash_table_mut(owner, |table| {
            table.insert(
                TaggedValue::fixnum(7).to_hash_key(&HashTableTest::Eq),
                TaggedValue::fixnum(7),
                anchor,
            );
        });
    }
    heap.collect_exact(owners.into_iter().chain([anchor]));
    for child in children {
        assert!(heap.owns_heap_value_for_test(child));
    }
}

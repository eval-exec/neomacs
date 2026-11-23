//! Direct product-path isolation of the numeric-shell observation provenance.
//! This is not a World/admission/two-owner qualification row.

use super::*;
use crate::tagged::gc::arena_pages::{ObjectArena, PagedObject};
use crate::tagged::gc::cons_storage::ConsCatalog;
use malachite::integer::Integer;

#[derive(Debug, PartialEq, Eq)]
struct OwnedShape {
    pages: Vec<(usize, usize, usize, Vec<usize>)>,
    page_capacity: usize,
    index: Vec<(usize, usize)>,
    index_capacity: usize,
    spare: Vec<usize>,
    spare_capacity: usize,
    partial_head: usize,
}

fn shape<T: PagedObject>(arena: &ObjectArena<T>) -> OwnedShape {
    let mut index = arena
        .page_index_by_base
        .iter()
        .map(|(&base, &index)| (base, index))
        .collect::<Vec<_>>();
    index.sort_unstable();
    OwnedShape {
        pages: arena
            .pages
            .iter()
            .map(|page| {
                (
                    page.base_addr(),
                    page.next_index,
                    page.allocated,
                    page.alloc_bits.to_vec(),
                )
            })
            .collect(),
        page_capacity: arena.pages.capacity(),
        index,
        index_capacity: arena.page_index_by_base.capacity(),
        spare: arena.spare_storage.iter().map(|&p| p as usize).collect(),
        spare_capacity: arena.spare_storage.capacity(),
        partial_head: arena.partial_head,
    }
}

#[test]
fn numeric_storage_suspension_observation_uses_current_designator() {
    let mut heap = TaggedHeap::new();
    heap.gc_threshold = usize::MAX;
    let float = heap.alloc_float(17.25);
    let bignum = heap.alloc_bignum(Integer::from(i64::MAX));
    let identity = heap.heap_identity();
    let catalog = ConsCatalog::new(identity);
    // SAFETY: this direct fixture is the sole stopped Rust owner, with no TLS
    // heap, callbacks, external mutators, readers or marker start authority.
    // Both initialized numeric values are reseeded if a cycle needs draining.
    let mut collector = unsafe {
        heap.drain_to_quiescent(|heap| {
            heap.seed_root(float);
            heap.seed_root(bignum);
        })
    }
    .unwrap();
    let initial = {
        let heap = collector.registration_heap_mut();
        assert!(heap.owns_heap_value_for_test(float));
        assert!(heap.owns_heap_value_for_test(bignum));
        (shape(&heap.float_arena), shape(&heap.bignum_arena))
    };
    assert!(!initial.0.pages.is_empty());
    assert!(!initial.1.pages.is_empty());

    // SAFETY: actual catalog installation is performed by the sole quiescent
    // owner. There are no run/worker/region loans or concurrent starts. The
    // legitimate collector is suspended immediately after this cold transfer.
    unsafe { catalog.install(collector.registration_heap_mut()) }.unwrap();
    let suspended = collector.suspend_cons_storage();
    // Only the suspended capability's current post-install designator reads
    // the inert placeholder fields; the observation returns owned metadata.
    let (floats, bignums) = suspended.numeric_storage_snapshot_for_test();
    for arena in [&floats, &bignums] {
        assert!(arena.pages.is_empty());
        assert_eq!(arena.pages_capacity, 0);
        assert!(arena.page_index.is_empty());
        assert_eq!(arena.page_index_capacity, 0);
        assert!(arena.spare_storage.is_empty());
        assert_eq!(arena.spare_storage_capacity, 0);
        assert_eq!(arena.spare_unused_floor, 0);
        assert_eq!(arena.partial_head, usize::MAX);
    }

    // SAFETY: no owner was ever admitted. The original heap remains retained,
    // and there is no field/storage borrow across this resumption. Actual
    // catalog restore reunites numeric storage before any heap walk follows.
    let mut collector = unsafe { suspended.resume() };
    // SAFETY: every participant is stopped (the fixture is the only one), no
    // region was lent, and the shell fields have only been observed immutably.
    unsafe { catalog.restore(collector.registration_heap_mut()) }.unwrap();
    let restored = collector.registration_heap_mut();
    assert_eq!(shape(&restored.float_arena), initial.0);
    assert_eq!(shape(&restored.bignum_arena), initial.1);
    assert!(restored.owns_heap_value_for_test(float));
    assert!(restored.owns_heap_value_for_test(bignum));
    assert_eq!(float.as_float(), Some(17.25));
    assert_eq!(
        bignum.as_bignum().unwrap().to_string(),
        "9223372036854775807"
    );
    drop(collector);
    drop(catalog);
}

//! Two disjoint native-proof regions retain one exact Cons exemption.
//!
//! These are envelope model tests backed by actual live owned Cons addresses.
//! No collection or callback occurs while the model owns their geometry; the
//! heap retains every endpoint and mutation target for the whole test.

use super::ObservedEnvelope;
use crate::tagged::collection_reads::NativeWindowCollectionOwner;
use crate::tagged::gc::TaggedHeap;
use crate::tagged::header::ConsCell;
use crate::tagged::value::{TAG_MASK, TaggedValue};

fn address(owner: TaggedValue) -> usize {
    owner.bits() & !TAG_MASK
}

fn live_cells<const N: usize>() -> (Box<TaggedHeap>, [TaggedValue; N]) {
    let mut heap = Box::new(TaggedHeap::new());
    heap.set_gc_threshold(usize::MAX);
    let mut cells =
        std::array::from_fn(|_| heap.alloc_cons(TaggedValue::make_int(97), TaggedValue::NIL));
    cells.sort_unstable_by_key(|&cell| address(cell));
    for pair in cells.windows(2) {
        assert_eq!(
            address(pair[1]) - address(pair[0]),
            size_of::<ConsCell>(),
            "the owned fixture establishes the exact empty-interval geometry",
        );
    }
    (heap, cells)
}

#[test]
fn observed_broad_cons_gap_retains_prior_owner_when_disjoint_gap_replaces_it() {
    let (_heap, cells) = live_cells::<12>();
    let mut envelope = ObservedEnvelope::new();
    for endpoint in [cells[0], cells[4], cells[11]] {
        assert!(envelope.insert(endpoint.bits(), NativeWindowCollectionOwner::Cons));
    }
    let old = cells[2];
    let fresh = cells[7];
    assert!(envelope.exclude_unobserved(old.bits()));
    let first = envelope.gate();
    let broad = first
        .excluded
        .expect("the first refusal learns its empty gap");
    assert!(broad.covers(address(old)));
    assert!(broad.len() > 2 * size_of::<ConsCell>());
    assert_eq!(
        first.single_cons, 0,
        "the original broad-gap fast path remains the primary admission",
    );

    assert!(envelope.exclude_unobserved(fresh.bits()));
    let second = envelope.gate();
    let fresh_gap = second
        .excluded
        .expect("the disjoint interval replaces the generic gap");
    assert!(fresh_gap.covers(address(fresh)));
    assert!(!fresh_gap.covers(address(old)));
    assert_eq!(
        second.single_cons,
        address(old),
        "a broad prior gap's owner must survive generic-gap replacement",
    );
    assert!(!envelope.exclude_unobserved(old.bits()));
    assert!(!envelope.exclude_unobserved(fresh.bits()));
    assert_eq!(envelope.gate().excluded, Some(fresh_gap));

    assert!(envelope.insert(old.bits(), NativeWindowCollectionOwner::Cons));
    let observed = envelope.gate();
    assert_eq!(
        observed.single_cons, 0,
        "new observation closes the exact proof"
    );
    assert_eq!(observed.excluded, Some(fresh_gap));
}

#[test]
fn observed_disjoint_small_cons_gaps_keep_the_first_exact_exemption() {
    let (_heap, cells) = live_cells::<5>();
    let mut envelope = ObservedEnvelope::new();
    for endpoint in [cells[0], cells[2], cells[4]] {
        assert!(envelope.insert(endpoint.bits(), NativeWindowCollectionOwner::Cons));
    }
    let old = cells[1];
    let fresh = cells[3];
    assert!(envelope.exclude_unobserved(old.bits()));
    let first = envelope.gate();
    let old_gap = first.excluded.expect("the first narrow gap is retained");
    assert!(old_gap.len() <= 2 * size_of::<ConsCell>());
    assert_eq!(first.single_cons, address(old));

    assert!(envelope.exclude_unobserved(fresh.bits()));
    let second = envelope.gate();
    let fresh_gap = second
        .excluded
        .expect("the second narrow gap replaces the generic gap");
    assert!(fresh_gap.len() <= 2 * size_of::<ConsCell>());
    assert!(fresh_gap.covers(address(fresh)));
    assert!(!fresh_gap.covers(address(old)));
    assert_eq!(
        second.single_cons,
        address(old),
        "a narrow replacement cannot evict a still-valid disjoint exact exemption",
    );
    assert!(!envelope.exclude_unobserved(old.bits()));
    assert!(!envelope.exclude_unobserved(fresh.bits()));
    assert_eq!(envelope.gate().excluded, Some(fresh_gap));

    assert!(envelope.insert(old.bits(), NativeWindowCollectionOwner::Cons));
    assert_eq!(envelope.gate().single_cons, 0);
    assert_eq!(envelope.gate().excluded, Some(fresh_gap));
}

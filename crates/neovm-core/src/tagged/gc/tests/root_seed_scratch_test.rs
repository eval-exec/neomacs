use super::*;

#[test]
fn seed_capacity_survives_while_values_do_not() {
    let scratch = RootSeedScratch::default();
    let (address, capacity) = {
        let mut loan = scratch.loan();
        let roots = loan.roots();
        roots.extend((0..4096).map(TaggedValue::fixnum));
        (roots.as_ptr(), roots.capacity())
    };
    let mut loan = scratch.loan();
    let roots = loan.roots();
    assert!(roots.is_empty());
    assert_eq!(roots.capacity(), capacity);
    assert_eq!(roots.as_ptr(), address);
    roots.extend((0..4096).map(TaggedValue::fixnum));
    assert_eq!(roots.as_ptr(), address);
}

#[test]
fn unwind_returns_empty_capacity() {
    let scratch = RootSeedScratch::default();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut loan = scratch.loan();
        loan.roots().extend((0..4096).map(TaggedValue::fixnum));
        panic!("unwind during root enumeration");
    }));
    assert!(result.is_err());
    let mut loan = scratch.loan();
    assert!(loan.roots().is_empty());
    assert!(loan.roots().capacity() >= 4096);
}

#[test]
fn retained_seed_capacity_is_not_a_root_between_collections() {
    use crate::tagged::gc::TaggedHeap;
    let mut heap = TaggedHeap::new();
    let value = heap.alloc_vector(vec![TaggedValue::fixnum(73)]);
    heap.begin_collection();
    // SAFETY: this isolated heap has one stopped owner throughout seeding.
    let mut loan = unsafe { heap.root_seed_scratch() };
    loan.roots().push(value);
    for root in loan.roots().drain(..) {
        heap.seed_root(root);
    }
    drop(loan);
    heap.complete_collection();
    assert!(heap.owns_heap_value_for_test(value));
    heap.collect_exact(std::iter::empty());
    assert!(!heap.owns_heap_value_for_test(value));
    // SAFETY: no mutator or collection can race this isolated test owner.
    let mut loan = unsafe { heap.root_seed_scratch() };
    assert!(loan.roots().is_empty());
    assert!(loan.roots().capacity() > 0);
}

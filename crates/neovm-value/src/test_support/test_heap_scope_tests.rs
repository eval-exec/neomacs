//! Canonical no-Context test heap route (LEAD 15:1xZ, amended 15:2xZ):
//! install/restore, nesting, panic restore, allocation placement, and the
//! graveyard's two lifetime rules — an escaped Value stays readable after its
//! scope ends, and a collect in a nested scope never touches the outer heap.
//! The bodies run in value's own test build and in the extracted harness (the
//! file is path-included there with the same `crate::` roots mirrored).

use crate::tagged::gc::current_tagged_heap_identity;
use crate::tagged::gc::with_tagged_heap;
use crate::tagged::value::TaggedValue;
use crate::test_support::authority::TestHeapScope;

#[test]
fn install_and_restore() {
    let before = current_tagged_heap_identity();
    {
        let scope = TestHeapScope::install();
        assert_eq!(current_tagged_heap_identity(), Some(scope.identity()));
    }
    assert_eq!(
        current_tagged_heap_identity(),
        before,
        "drop restores the previous slot value"
    );
}

#[test]
fn nested_scopes_restore_in_lifo_order() {
    let before = current_tagged_heap_identity();
    let outer = TestHeapScope::install();
    let outer_id = outer.identity();
    assert_eq!(current_tagged_heap_identity(), Some(outer_id));
    {
        let inner = TestHeapScope::install();
        assert_ne!(inner.identity(), outer_id, "each scope owns a fresh heap");
        assert_eq!(current_tagged_heap_identity(), Some(inner.identity()));
    }
    assert_eq!(
        current_tagged_heap_identity(),
        Some(outer_id),
        "the inner drop restores the outer scope's heap"
    );
    drop(outer);
    assert_eq!(current_tagged_heap_identity(), before);
}

#[test]
fn panic_inside_the_scope_restores_the_slot() {
    let before = current_tagged_heap_identity();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _scope = TestHeapScope::install();
        let _value = TaggedValue::cons(TaggedValue::NIL, TaggedValue::NIL);
        panic!("boom inside the test heap scope");
    }));
    assert!(result.is_err(), "the panic must unwind through the scope");
    assert_eq!(
        current_tagged_heap_identity(),
        before,
        "the scope's Drop restored the slot during unwinding"
    );
}

#[test]
fn allocation_inside_the_scope_lands_in_the_scope_heap() {
    let before = current_tagged_heap_identity();
    let scope = TestHeapScope::install();
    let id = scope.identity();
    assert_eq!(current_tagged_heap_identity(), Some(id));
    // A cons cell allocates on the installed thread heap; the identity read
    // after the allocation must still be the scope's heap.
    let value = TaggedValue::cons(TaggedValue::NIL, TaggedValue::NIL);
    assert!(value.is_cons());
    assert_eq!(
        current_tagged_heap_identity(),
        Some(id),
        "the allocation ran on the scope's heap"
    );
    drop(value);
    drop(scope);
    assert_eq!(current_tagged_heap_identity(), before);
}

#[test]
fn value_allocated_in_a_scope_stays_readable_after_the_scope_ends() {
    // LEAD 15:2xZ (e): Values are Copy tagged words with no borrow of their
    // heap. The scope must not free the heap on drop, exactly like the
    // pre-split thread-lifetime fallback heap, or this read would be a
    // use-after-free.
    let before = current_tagged_heap_identity();
    let scope = TestHeapScope::install();
    let value = TaggedValue::cons(TaggedValue::fixnum(42), TaggedValue::fixnum(7));
    drop(scope);
    assert_eq!(
        current_tagged_heap_identity(),
        before,
        "the slot is restored even though the heap stays alive"
    );
    assert!(value.is_cons(), "the escaped Value is still a live cons");
    assert_eq!(value.cons_car(), TaggedValue::fixnum(42));
    assert_eq!(value.cons_cdr(), TaggedValue::fixnum(7));
}

#[test]
fn a_collect_in_a_nested_scope_leaves_the_outer_heap_untouched() {
    // LEAD 15:2xZ (f): a collection runs against exactly one heap. The inner
    // collect below admits only the inner heap, so it must not trace, mark,
    // sweep or move the outer heap's objects.
    let before = current_tagged_heap_identity();
    let outer = TestHeapScope::install();
    let outer_id = outer.identity();
    let outer_value = TaggedValue::cons(TaggedValue::fixnum(11), TaggedValue::NIL);
    {
        let inner = TestHeapScope::install();
        assert_ne!(inner.identity(), outer_id);
        let inner_value = TaggedValue::cons(TaggedValue::fixnum(22), TaggedValue::NIL);
        // Root only the inner value: the outer value is not reachable from
        // this collect at all, and another heap owns it.
        with_tagged_heap(|heap| heap.collect_exact(std::iter::once(inner_value)));
        assert_eq!(inner_value.cons_car(), TaggedValue::fixnum(22));
        assert_eq!(
            outer_value.cons_car(),
            TaggedValue::fixnum(11),
            "the inner collect did not move or clear the outer Value"
        );
    }
    assert_eq!(current_tagged_heap_identity(), Some(outer_id));
    // The outer heap's own collector still works after the inner cycle and
    // keeps its rooted object.
    with_tagged_heap(|heap| heap.collect_exact(std::iter::once(outer_value)));
    assert_eq!(outer_value.cons_car(), TaggedValue::fixnum(11));
    drop(outer);
    assert_eq!(current_tagged_heap_identity(), before);
    assert_eq!(outer_value.cons_car(), TaggedValue::fixnum(11));
}

#[test]
fn a_cross_heap_store_is_never_traced_or_moved_by_the_other_heap() {
    // LEAD 15:2xZ (g): storing an outer-heap Value inside an inner-heap
    // object is not allowed. Allocation does not reject the word; the
    // enforced rule is that the inner collector never traces, marks, sweeps
    // or moves an object another heap owns, and the registered-mutator
    // boundaries reject cross-heap identities with WrongHeap.
    let before = current_tagged_heap_identity();
    let outer = TestHeapScope::install();
    let outer_id = outer.identity();
    let outer_value = TaggedValue::cons(TaggedValue::fixnum(33), TaggedValue::NIL);
    {
        let inner = TestHeapScope::install();
        assert_ne!(inner.identity(), outer_id);
        // The inner cons carries the outer heap's Value in its car: the word
        // is stored, but it does not make the outer object a member of the
        // inner heap.
        let holder = TaggedValue::cons(outer_value, TaggedValue::NIL);
        assert_eq!(holder.cons_car(), outer_value);
        // Root the holder: the inner trace reaches the foreign word.
        with_tagged_heap(|heap| heap.collect_exact(std::iter::once(holder)));
        assert_eq!(
            holder.cons_car(),
            outer_value,
            "the foreign word is carried unchanged"
        );
        assert_eq!(
            outer_value.cons_car(),
            TaggedValue::fixnum(33),
            "the outer object is neither moved nor swept by the inner collect"
        );
    }
    assert_eq!(current_tagged_heap_identity(), Some(outer_id));
    assert_eq!(outer_value.cons_car(), TaggedValue::fixnum(33));
    drop(outer);
    assert_eq!(current_tagged_heap_identity(), before);
    assert_eq!(outer_value.cons_car(), TaggedValue::fixnum(33));
}

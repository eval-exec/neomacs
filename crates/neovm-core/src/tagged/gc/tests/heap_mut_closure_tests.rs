//! GEN-5 guards every closure that can bulk-install heap children. Nested
//! mutations and unwinding must preserve the enclosing mutation extent.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::value::{HashTableTest, LambdaParams};
use crate::heap_types::OverlayDataInit;
use crate::tagged::gc::{TaggedHeap, set_tagged_heap};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn assert_in_mutation() {
    assert!(IN_HEAP_MUT_CLOSURE.with(|active| active.get()));
}

#[test]
fn every_heap_mutation_closure_has_a_guard() {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    // Each owner is used before the next allocation; no unrooted owner or
    // container of Values is retained across an allocation.
    {
        let owner = heap.alloc_vector(vec![TaggedValue::NIL]);
        assert_eq!(
            with_vector_data_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    {
        let owner = heap.alloc_record(vec![TaggedValue::NIL]);
        assert_eq!(
            with_record_data_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    {
        let owner = heap.alloc_lambda(vec![TaggedValue::NIL; 3]);
        assert_eq!(
            with_closure_slots_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    {
        let owner = heap.alloc_macro(vec![TaggedValue::NIL; 3]);
        assert_eq!(
            with_closure_slots_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    {
        let owner = heap.alloc_string(LispString::from_utf8("guard"));
        assert_eq!(
            with_string_text_props_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
        assert_eq!(
            with_lisp_string_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    {
        let owner = heap.alloc_hash_table(LispHashTable::new(HashTableTest::Eq));
        assert_eq!(
            with_hash_table_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    {
        let owner = heap.alloc_bytecode(ByteCodeFunction::new(LambdaParams::simple(vec![])));
        assert_eq!(
            with_bytecode_data_mut_for_test(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    {
        let owner = heap.alloc_marker(LispMarker {
            buffer: None,
            insertion_type: false,
            marker_id: None,
            bytepos: 0,
            charpos: 0,
            last_position_valid: false,
            next_marker: std::ptr::null_mut(),
            chained: false,
        });
        assert_eq!(
            with_marker_data_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    {
        let owner = heap.alloc_overlay(
            OverlayDataInit {
                serial: 0,
                plist: TaggedValue::NIL,
                buffer: None,
                start: 0,
                end: 0,
                front_advance: false,
                rear_advance: false,
            }
            .into(),
        );
        assert_eq!(
            with_overlay_data_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    {
        let owner = heap.alloc_xwidget(
            TaggedValue::NIL,
            TaggedValue::NIL,
            TaggedValue::NIL,
            1,
            1,
            0,
            neomacs_display_protocol::WebViewId::new(0),
        );
        assert_eq!(with_xwidget_mut(owner, |_| assert_in_mutation()), Some(()));
    }
    {
        let owner = heap.alloc_xwidget_view(TaggedValue::NIL, TaggedValue::NIL);
        assert_eq!(
            with_xwidget_view_mut(owner, |_| assert_in_mutation()),
            Some(())
        );
    }
    debug_assert_no_heap_mut_closure();
}

#[test]
fn nested_heap_mutations_restore_the_outer_guard_after_an_unwind() {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let outer = heap.alloc_vector(vec![TaggedValue::NIL]);
    let saved_roots = crate::emacs_core::eval::save_scratch_gc_roots();
    crate::emacs_core::eval::push_scratch_gc_root(outer);
    let inner = heap.alloc_record(vec![TaggedValue::NIL]);
    with_vector_data_mut(outer, |_| {
        assert_in_mutation();
        with_record_data_mut(inner, |_| assert_in_mutation()).unwrap();
        assert_in_mutation();
        let unwind = catch_unwind(AssertUnwindSafe(|| {
            with_record_data_mut(inner, |_| panic!("nested mutation unwind"));
        }));
        assert!(unwind.is_err());
        assert_in_mutation();
    })
    .unwrap();
    debug_assert_no_heap_mut_closure();
    let unwind = catch_unwind(AssertUnwindSafe(|| {
        with_vector_data_mut(outer, |_| panic!("outer mutation unwind"));
    }));
    assert!(unwind.is_err());
    debug_assert_no_heap_mut_closure();
    crate::emacs_core::eval::restore_scratch_gc_roots(saved_roots);
}

#[test]
fn heap_mutation_guards_are_thread_local() {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let owner = heap.alloc_vector(Vec::new());
    with_vector_data_mut(owner, |_| {
        assert_in_mutation();
        std::thread::spawn(debug_assert_no_heap_mut_closure)
            .join()
            .unwrap();
        assert_in_mutation();
    })
    .unwrap();
    debug_assert_no_heap_mut_closure();
}

#[test]
fn rejected_heap_mutation_inputs_do_not_enter_a_guard() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        with_vector_data_mut(TaggedValue::NIL, |_| panic!("wrong kind")),
        None::<()>
    );
    assert_eq!(
        with_closure_slots_mut(TaggedValue::NIL, |_| panic!("wrong kind")),
        None::<()>
    );
    debug_assert_no_heap_mut_closure();
}

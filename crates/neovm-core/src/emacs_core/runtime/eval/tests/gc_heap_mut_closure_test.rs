//! Exercise GEN-5 through the evaluator's public safe point, automatic and
//! explicit collection entries, and the VM/JIT backedge safe point.

use crate::emacs_core::eval::Context;
use crate::tagged::mutate::{debug_assert_no_heap_mut_closure, with_vector_data_mut};
use std::panic::{AssertUnwindSafe, catch_unwind};

fn assert_gen5_panic(result: std::thread::Result<()>) {
    let panic = result.expect_err("a GC entry inside a mutation closure must panic");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied());
    assert!(message.is_some_and(|message| message.contains("GEN-5")));
    debug_assert_no_heap_mut_closure();
}

#[test]
fn gc_safe_point_rejects_mutation_even_when_collection_is_inhibited() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let owner = eval.tagged_heap.alloc_vector(Vec::new());
    assert_gen5_panic(catch_unwind(AssertUnwindSafe(|| {
        eval.with_gc_inhibited(|eval| {
            with_vector_data_mut(owner, |_| eval.gc_safe_point()).unwrap();
        });
    })));
}

#[test]
fn automatic_and_explicit_collection_reject_heap_mutation_closures() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let owner = eval.tagged_heap.alloc_vector(Vec::new());
    assert_gen5_panic(catch_unwind(AssertUnwindSafe(|| {
        with_vector_data_mut(owner, |_| eval.gc_collect_from_current_roots_impl(false)).unwrap();
    })));
    assert_gen5_panic(catch_unwind(AssertUnwindSafe(|| {
        with_vector_data_mut(owner, |_| eval.gc_collect_exact()).unwrap();
    })));
}

#[test]
fn bytecode_and_jit_backedge_safe_point_rejects_heap_mutation_closures() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let owner = eval.tagged_heap.alloc_vector(Vec::new());
    assert_gen5_panic(catch_unwind(AssertUnwindSafe(|| {
        with_vector_data_mut(owner, |_| {
            let _ = eval.bytecode_branch_maybe_gc_and_quit();
        })
        .unwrap();
    })));
}

use super::*;
use std::sync::Arc;

use self::command::CclCommand;
use self::expr::{eval_expr_self, eval_set_expr};
use self::extension::{ExtensionStep, MapMultipleState, execute_extension};
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_max_args, expect_min_args};
use neovm_value::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};
use neovm_value::intern::SymId;
use neovm_value::tagged::value::SymbolMode;
use std::collections::HashMap;

use crate::emacs_core::eval::Context;
use neovm_value::value::Value;
use std::cell::Cell;

use neovm_text::emacs_core::ccl::*;

#[test]
fn gc_tls_ownership_ccl_new_heap_reset_drops_old_values() {
    {
        let mut first = crate::test_utils::new_context();
        let value = populate(&mut first);
        assert!(roots(&first).iter().any(|root| root.bits() == value.bits()));
    }
    let mut next = crate::test_utils::new_context();
    assert!(
        roots(&next)
            .iter()
            .all(|value| !value.is_heap_object()
                || next.tagged_heap().owns_heap_value_for_test(*value)),
        "new-heap reset retained a value from the dropped heap"
    );
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_ccl_excludes_another_live_heap() {
    let mut first = crate::test_utils::new_context();
    let mut second = crate::test_utils::new_context();
    let value = populate(&mut second);
    assert!(
        roots(&second)
            .iter()
            .any(|root| root.bits() == value.bits())
    );
    assert!(
        roots(&first)
            .iter()
            .all(|value| !value.is_heap_object()
                || first.tagged_heap().owns_heap_value_for_test(*value)),
        "GC roots contain a value belonging to another live Context"
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    second.gc_collect_exact();
    assert!(second.tagged_heap().owns_heap_value_for_test(value));
}

#[test]
fn gc_tls_ownership_ccl_obarray_is_retracted_on_unwind() {
    let first = crate::test_utils::new_context();
    let before = CCL_VARIABLES.with(Cell::get);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_ccl_obarray(first.variables(), || panic!("unwind CCL callback"));
    }));
    assert!(result.is_err());
    assert_eq!(
        CCL_VARIABLES.with(Cell::get),
        before,
        "CCL variables pointer survived the dynamic scope after an unwind"
    );
}

#[test]
fn gc_tls_ownership_ccl_registration_follows_context_activation() {
    let mut first = crate::test_utils::new_context();
    let program = populate(&mut first);
    let second = crate::test_utils::new_context();
    assert!(!is_registered_ccl_program(neovm_value::intern::intern(
        "gc-tls-owned-ccl"
    )));
    first.setup_thread_locals();
    assert!(
        roots(&first)
            .iter()
            .any(|root| root.bits() == program.bits()),
        "reactivating a Context lost its registered CCL programs"
    );
    first.gc_collect_exact();
    assert_eq!(
        builtin_ccl_program_p_impl(vec![Value::symbol("gc-tls-owned-ccl")]).unwrap(),
        Value::T
    );
    drop(second);
}

#[test]
fn gc_tls_ownership_ccl_registries_collect_on_independent_owner_threads() {
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let worker_barrier = std::sync::Arc::clone(&barrier);
    let worker = std::thread::spawn(move || {
        let mut first = crate::test_utils::new_context();
        let value = populate(&mut first);
        worker_barrier.wait();
        first.setup_thread_locals();
        assert!(
            roots(&first).iter().any(|root| root.bits() == value.bits()),
            "worker Context lost its owning registry"
        );
        first.gc_collect_exact();
        assert!(first.tagged_heap().owns_heap_value_for_test(value));
    });
    barrier.wait();
    // Each Context is created on its owner. Registry collection on one thread
    // must remain independent of activation and collection on the other.
    let mut second = crate::test_utils::new_context();
    populate(&mut second);
    second.gc_collect_exact();
    worker.join().unwrap();
}

fn populate(_ctx: &mut Context) -> Value {
    let program = Value::vector(vec![Value::fixnum(0); 3]);
    builtin_register_ccl_program_impl(vec![Value::symbol("gc-tls-owned-ccl"), program]).unwrap();
    let map = Value::vector(vec![Value::fixnum(42)]);
    builtin_register_code_conversion_map_impl(vec![Value::symbol("gc-tls-owned-map"), map])
        .unwrap();
    program
}

fn roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_ccl_gc_roots(&mut roots, ctx.tagged_heap().tagged_heap_identity());
    roots
}

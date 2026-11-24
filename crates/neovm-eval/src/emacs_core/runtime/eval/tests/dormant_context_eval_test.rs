//! At most one active Context per thread. A second `Context::new` while a
//! heap is installed refuses in release. Parking is the test suspension
//! primitive; it has no eval API. Outer doors debug-assert when a foreign
//! heap is nested over a live Context.

use std::panic::{AssertUnwindSafe, catch_unwind};

use neovm_value::tagged::gc::{
    NestedTaggedHeapInstall, current_tagged_heap_identity, tagged_heap_is_current,
};

use super::*;

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_string())
        })
        .unwrap_or_default()
}

#[test]
fn a_second_context_on_one_thread_refuses_while_one_is_active() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let first_id = first.tagged_heap().tagged_heap_identity();
    assert_eq!(current_tagged_heap_identity(), Some(first_id));

    let refused = catch_unwind(AssertUnwindSafe(|| {
        let _second = Context::new(crate::test_utils::unit_runtime());
    }));
    let text = panic_text(refused.expect_err("a second Context::new must refuse"));
    assert!(
        text.contains("dormant Context"),
        "constructor refusal must name the dormant Context, got {text}"
    );
    assert_eq!(
        current_tagged_heap_identity(),
        Some(first_id),
        "a refused constructor must leave the first heap installed"
    );

    drop(first);
    let second = Context::new(crate::test_utils::unit_runtime());
    assert_eq!(
        current_tagged_heap_identity(),
        Some(second.tagged_heap().tagged_heap_identity())
    );
}

#[test]
fn parked_resume_refuses_heap_installed_and_has_no_eval_api() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let first_id = first.tagged_heap().tagged_heap_identity();
    let parked = first.park_for_test();
    assert_ne!(current_tagged_heap_identity(), Some(first_id));

    let mut second = Context::new(crate::test_utils::unit_runtime());
    let second_id = second.tagged_heap().tagged_heap_identity();
    let refused = match parked.resume_for_test() {
        Err(error) => error,
        Ok(_restored) => panic!("resume under a live heap"),
    };
    assert_eq!(format!("{refused:?}"), "HeapInstalled");
    assert_eq!(refused.to_string(), "HeapInstalled");
    assert_eq!(current_tagged_heap_identity(), Some(second_id));
    assert_eq!(
        second
            .eval_str("7")
            .expect("the active Context still evaluates"),
        Value::fixnum(7)
    );

    let parked_second = second.park_for_test();
    // The refused resume dropped the first Context. A fresh one is the
    // empty-slot install, which is the path resume would have taken.
    drop(parked_second);
    let mut restored = Context::new(crate::test_utils::unit_runtime());
    assert_eq!(
        restored.eval_str("8").expect("empty slot constructs"),
        Value::fixnum(8)
    );
}

#[test]
fn park_then_resume_restores_the_same_context() {
    let mut first = Context::new(crate::test_utils::unit_runtime());
    first
        .eval_str("(setq park-token 3)")
        .expect("bind on the first context");
    let first_id = first.tagged_heap().tagged_heap_identity();
    let parked = first.park_for_test();

    let second = Context::new(crate::test_utils::unit_runtime());
    let parked_second = second.park_for_test();
    let mut first = parked.resume_for_test().expect("slot empty after park");
    assert_eq!(current_tagged_heap_identity(), Some(first_id));
    assert_eq!(
        first.eval_str("park-token").expect("resumed binding"),
        Value::fixnum(3)
    );
    drop(first);
    let mut second = parked_second.resume_for_test().expect("resume second");
    assert_eq!(
        second.eval_str("4").expect("second context evaluates"),
        Value::fixnum(4)
    );
}

#[test]
fn nested_bare_heap_restores_the_context_heap() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let id = ctx.tagged_heap().tagged_heap_identity();
    let foreign =
        neovm_value::tagged::gc::AliasableBox::new(neovm_value::tagged::gc::TaggedHeap::new());
    {
        // SAFETY: `foreign` and `ctx` both outlive `nest` with no live heap
        // references during install or drop.
        let nest = unsafe { NestedTaggedHeapInstall::install_bare(foreign.root()) };
        assert!(tagged_heap_is_current(&foreign));
        assert_ne!(current_tagged_heap_identity(), Some(id));
        drop(nest);
    }
    assert_eq!(current_tagged_heap_identity(), Some(id));
    assert_eq!(ctx.eval_str("1").expect("restored door"), Value::fixnum(1));
}

#[cfg(debug_assertions)]
#[test]
fn outer_doors_debug_assert_under_a_foreign_heap() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let id = ctx.tagged_heap().tagged_heap_identity();
    let foreign =
        neovm_value::tagged::gc::AliasableBox::new(neovm_value::tagged::gc::TaggedHeap::new());
    // SAFETY: `foreign` and `ctx` both outlive `nest` with no live heap
    // references during install or drop.
    let nest = unsafe { NestedTaggedHeapInstall::install_bare(foreign.root()) };

    for (label, call) in [
        ("eval_str", eval_str_door as fn(&mut Context)),
        ("eval_form", eval_form_door as fn(&mut Context)),
        ("eval_value", eval_value_door as fn(&mut Context)),
        ("eval_str_each", eval_str_each_door as fn(&mut Context)),
    ] {
        let caught = catch_unwind(AssertUnwindSafe(|| call(&mut ctx)));
        let text = panic_text(caught.expect_err(label));
        assert!(
            text.contains("dormant Context"),
            "{label} must debug-assert, got {text}"
        );
        assert!(
            tagged_heap_is_current(&foreign),
            "{label} must not install the Context heap"
        );
    }
    drop(nest);
    assert_eq!(current_tagged_heap_identity(), Some(id));
    assert_eq!(
        ctx.eval_str("1").expect("door after restore"),
        Value::fixnum(1)
    );
}

#[cfg(debug_assertions)]
fn eval_str_door(ctx: &mut Context) {
    let _ = ctx.eval_str("1");
}

#[cfg(debug_assertions)]
fn eval_form_door(ctx: &mut Context) {
    let _ = ctx.eval_form(Value::fixnum(1));
}

#[cfg(debug_assertions)]
fn eval_value_door(ctx: &mut Context) {
    let _ = ctx.eval_value(&Value::fixnum(1));
}

#[cfg(debug_assertions)]
fn eval_str_each_door(ctx: &mut Context) {
    let _ = ctx.eval_str_each("1");
}

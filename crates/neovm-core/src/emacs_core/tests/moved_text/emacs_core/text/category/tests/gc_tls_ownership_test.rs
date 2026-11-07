use super::*;

use neovm_edit::buffer::buffer::BUFFER_SLOT_CATEGORY_TABLE;
use neovm_text::emacs_core::text_host::TextHost;
use neovm_value::boolvec::{self, BoolVectorView};
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_max_args, expect_min_args};
use neovm_value::tagged::SymbolMode;
use std::cell::{Cell, RefCell};

use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

use neovm_text::emacs_core::category::*;

#[test]
fn gc_tls_ownership_category_new_heap_reset_drops_old_values() {
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
fn gc_tls_ownership_category_excludes_another_live_heap() {
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

fn populate(_ctx: &mut Context) -> Value {
    ensure_standard_category_table_object().unwrap()
}

fn roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_category_gc_roots(&mut roots, ctx.tagged_heap().tagged_heap_identity());
    roots
}

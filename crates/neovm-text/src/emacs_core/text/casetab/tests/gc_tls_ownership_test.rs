use super::*;
use crate::emacs_core::eval::Context;

fn populate(ctx: &mut Context) -> Value {
    let table = builtin_standard_case_table(ctx, vec![]).unwrap();
    builtin_set_standard_case_table(ctx, vec![table]).unwrap()
}

fn roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_casetab_gc_roots(&mut roots, ctx.tagged_heap.identity());
    roots
}

#[test]
fn gc_tls_ownership_casetab_new_heap_reset_drops_old_values() {
    {
        let mut first = Context::new();
        let value = populate(&mut first);
        assert!(roots(&first).iter().any(|root| root.bits() == value.bits()));
    }
    let mut next = Context::new();
    assert!(
        roots(&next)
            .iter()
            .all(|value| !value.is_heap_object()
                || next.tagged_heap.owns_heap_value_for_test(*value)),
        "new-heap reset retained a value from the dropped heap"
    );
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_casetab_excludes_another_live_heap() {
    let mut first = Context::new();
    let mut second = Context::new();
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
                || first.tagged_heap.owns_heap_value_for_test(*value)),
        "GC roots contain a value belonging to another live Context"
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    second.gc_collect_exact();
    assert!(second.tagged_heap.owns_heap_value_for_test(value));
}

#[test]
fn gc_tls_ownership_casetab_activation_preserves_explicit_cache_semantics() {
    let mut first = Context::new();
    let standard = builtin_standard_case_table(&mut first, vec![]).unwrap();
    builtin_set_case_table(&mut first, vec![standard]).unwrap();
    let canon = buffer_case_canon_table(first.buffers.current_buffer().unwrap()).unwrap();
    first.setup_thread_locals();
    assert_eq!(
        buffer_case_canon_table(first.buffers.current_buffer().unwrap())
            .unwrap()
            .bits(),
        canon.bits(),
        "activation populated a previously empty standard-table cache"
    );
    builtin_set_standard_case_table(&mut first, vec![standard]).unwrap();
    assert!(buffer_case_canon_table(first.buffers.current_buffer().unwrap()).is_none());
    let _second = Context::new();
    first.setup_thread_locals();
    assert!(
        buffer_case_canon_table(first.buffers.current_buffer().unwrap()).is_none(),
        "activation lost the explicitly populated standard-table cache"
    );
    assert!(
        roots(&first)
            .iter()
            .any(|root| root.bits() == standard.bits())
    );
    first.gc_collect_exact();
}

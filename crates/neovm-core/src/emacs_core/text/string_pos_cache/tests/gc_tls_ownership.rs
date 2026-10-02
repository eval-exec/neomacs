use super::*;
use crate::emacs_core::eval::Context;

fn populate(_ctx: &mut Context) -> Value {
    let string = Value::string("aжb");
    string_char_to_byte(string, string.as_lisp_string().unwrap(), 2);
    string
}

fn roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_string_pos_cache_gc_roots(&mut roots, ctx.tagged_heap.identity());
    roots
}

#[test]
fn gc_tls_ownership_string_pos_cache_new_heap_reset_drops_old_values() {
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
fn gc_tls_ownership_string_pos_cache_excludes_another_live_heap() {
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

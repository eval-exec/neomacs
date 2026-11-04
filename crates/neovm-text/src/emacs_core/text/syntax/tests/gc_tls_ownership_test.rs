use super::*;
use crate::emacs_core::eval::Context;

fn populate(_ctx: &mut Context) -> Value {
    let table = ensure_standard_syntax_table_object().unwrap();
    ensure_syntax_code_objects();
    table
}

fn roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_syntax_gc_roots(&mut roots, ctx.tagged_heap.identity());
    roots
}

#[test]
fn gc_tls_ownership_syntax_new_heap_reset_drops_old_values() {
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
fn gc_tls_ownership_syntax_excludes_another_live_heap() {
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
fn gc_tls_ownership_syntax_weak_ascii_keys_invalidate_before_heap_reuse() {
    let old_tick = {
        let mut first = Context::new();
        let chartable = first.eval_str("(progn (set-syntax-table (copy-syntax-table)) (modify-syntax-entry ?a \".\") (syntax-table))").unwrap();
        let table = SyntaxTable { chartable };
        assert_eq!(
            flat_ascii_syntax_entry(&table, b'a').class,
            SyntaxClass::Punctuation
        );
        assert_eq!(
            flat_ascii_entries_for_table(&table)[b'a' as usize].class,
            SyntaxClass::Punctuation
        );
        first.gc_collect_exact();
        crate::emacs_core::chartable::char_table_write_tick()
    };
    let mut next = Context::new();
    assert_ne!(
        crate::emacs_core::chartable::char_table_write_tick(),
        old_tick,
        "new char-tables must invalidate weak identity caches before address reuse"
    );
    let table = SyntaxTable::new_standard();
    assert_eq!(
        flat_ascii_syntax_entry(&table, b'a').class,
        SyntaxClass::Word
    );
    assert_eq!(
        flat_ascii_entries_for_table(&table)[b'a' as usize].class,
        SyntaxClass::Word
    );
    next.gc_collect_exact();
}

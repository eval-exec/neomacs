use super::*;
use crate::emacs_core::eval::Context;

fn roots_for(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_hash_table_test_registry_gc_roots(&ctx.hash_table_test_registry, &mut roots);
    roots
}

fn register_alias(ctx: &mut Context) -> Value {
    ctx.eval_str("(define-hash-table-test 'gc-tls-custom (lambda (a b) (eq a b)) (lambda (a) 1))")
        .expect("define a custom hash table test");
    lookup_hash_table_test_alias("gc-tls-custom")
        .unwrap()
        .user_cmp_function
        .unwrap()
}

#[test]
fn gc_tls_ownership_hash_alias_reset_discards_a_dropped_heap() {
    let old = {
        let mut ctx = Context::new();
        let old = register_alias(&mut ctx);
        assert!(roots_for(&ctx).iter().any(|root| root.bits() == old.bits()));
        old
    };
    let mut next = Context::new();
    assert!(
        !roots_for(&next)
            .iter()
            .any(|root| root.bits() == old.bits())
    );
    assert!(lookup_hash_table_test_alias("gc-tls-custom").is_none());
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_hash_alias_roots_exclude_another_live_heap() {
    let mut first = Context::new();
    let mut second = Context::new();
    let value = register_alias(&mut second);
    assert!(
        roots_for(&second)
            .iter()
            .any(|root| root.bits() == value.bits())
    );
    first.setup_thread_locals();
    assert!(
        roots_for(&first)
            .iter()
            .all(|root| first.tagged_heap.owns_heap_value_for_test(*root)),
        "custom hash functions from the second Context are roots of the first heap"
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    second.gc_collect_exact();
    assert_eq!(second.eval_str("(let ((h (make-hash-table :test 'gc-tls-custom))) (puthash 'x 42 h) (gethash 'x h))").unwrap(), Value::fixnum(42));
}

#[test]
fn gc_tls_ownership_hash_aliases_follow_context_swaps() {
    let mut first = Context::new();
    let a = register_alias(&mut first);
    let mut second = Context::new();
    let b = register_alias(&mut second);
    first.setup_thread_locals();
    assert_eq!(
        lookup_hash_table_test_alias("gc-tls-custom")
            .unwrap()
            .user_cmp_function
            .unwrap()
            .bits(),
        a.bits()
    );
    first.gc_collect_exact();
    second.setup_thread_locals();
    assert_eq!(
        lookup_hash_table_test_alias("gc-tls-custom")
            .unwrap()
            .user_cmp_function
            .unwrap()
            .bits(),
        b.bits()
    );
    second.gc_collect_exact();
}

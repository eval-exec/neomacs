use super::*;
use crate::emacs_core::eval::Context;

fn populate(_ctx: &mut Context) -> Value {
    let payload = Value::vector(vec![Value::fixnum(42)]);
    set_charset_plist_registry(
        intern("ascii"),
        vec![(intern("gc-tls-owned-plist"), payload)],
    );
    payload
}

fn roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_charset_gc_roots(&mut roots, ctx.tagged_heap.identity());
    roots
}

#[test]
fn gc_tls_ownership_charset_new_heap_reset_drops_old_values() {
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
fn gc_tls_ownership_charset_excludes_another_live_heap() {
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
fn gc_tls_ownership_charset_metadata_follows_context_activation() {
    let mut first = Context::new();
    let payload = populate(&mut first);
    let second = Context::new();
    first.setup_thread_locals();
    assert!(
        roots(&first)
            .iter()
            .any(|root| root.bits() == payload.bits()),
        "reactivating a Context lost its charset Lisp attributes"
    );
    first.gc_collect_exact();
    let plist = CHARSET_REGISTRY
        .with(|slot| slot.borrow().plist(intern("ascii")).map(<[_]>::to_vec))
        .unwrap();
    assert!(
        plist
            .iter()
            .any(|(_, value)| value.bits() == payload.bits())
    );
    drop(second);
}

#[test]
fn gc_tls_ownership_charset_registry_follows_context_thread_transfer() {
    let mut first = Context::new();
    let value = populate(&mut first);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let worker_barrier = std::sync::Arc::clone(&barrier);
    let worker = std::thread::spawn(move || {
        worker_barrier.wait();
        first.setup_thread_locals();
        assert!(
            roots(&first).iter().any(|root| root.bits() == value.bits()),
            "Context thread transfer lost its owning registry"
        );
        first.gc_collect_exact();
        assert!(first.tagged_heap.owns_heap_value_for_test(value));
    });
    barrier.wait();
    // Replacing the source thread's installed alias may overlap destination
    // activation/rooting. Its refcount and lifetime must remain thread-safe.
    let mut second = Context::new();
    populate(&mut second);
    second.gc_collect_exact();
    worker.join().unwrap();
}

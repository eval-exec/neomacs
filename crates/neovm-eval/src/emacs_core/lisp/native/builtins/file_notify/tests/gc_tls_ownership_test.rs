use super::*;
use crate::emacs_core::eval::Context;

fn roots_for(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_file_notify_registry_gc_roots(&ctx.file_notify_registry, &mut roots);
    roots
}

fn register_watch() -> Value {
    let callback = Value::vector(vec![Value::fixnum(42)]);
    let file_name = Value::string("gc-tls-owned-file");
    FILE_NOTIFY_STATE.with(|slot| {
        slot.borrow_mut()
            .registry
            .register(WatchId::new(42, 7), callback, file_name);
    });
    callback
}

#[test]
fn gc_tls_ownership_file_notify_reset_discards_a_dropped_heap() {
    let old = {
        let ctx = Context::new(crate::test_utils::unit_runtime());
        let old = register_watch();
        assert!(roots_for(&ctx).iter().any(|root| root.bits() == old.bits()));
        old
    };
    let mut next = Context::new(crate::test_utils::unit_runtime());
    assert!(
        !roots_for(&next)
            .iter()
            .any(|root| root.bits() == old.bits())
    );
    next.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_file_notify_roots_exclude_another_live_heap() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let parked_first = first.park_for_test();
    let second = Context::new(crate::test_utils::unit_runtime());
    let callback = register_watch();
    assert!(
        roots_for(&second)
            .iter()
            .any(|root| root.bits() == callback.bits())
    );
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    assert!(
        roots_for(&first)
            .iter()
            .all(|root| first.tagged_heap.owns_heap_value_for_test(*root)),
        "file notification callbacks from the second Context are roots of the first heap"
    );
    first.gc_collect_exact();
    drop(first);
    let mut second = parked_second.resume_for_test().expect("resume second");
    second.gc_collect_exact();
    assert_eq!(callback.as_vector_data().unwrap()[0], Value::fixnum(42));
}

#[test]
fn gc_tls_ownership_file_notify_registrations_follow_context_swaps() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let a = register_watch();
    let parked_first = first.park_for_test();
    let second = Context::new(crate::test_utils::unit_runtime());
    let b = register_watch();
    let callback = || {
        FILE_NOTIFY_STATE.with(|slot| {
            slot.borrow()
                .registry
                .registration(&WatchId::new(42, 7))
                .unwrap()
                .callback()
        })
    };
    let parked_second = second.park_for_test();
    let mut first = parked_first.resume_for_test().expect("resume first");
    assert_eq!(callback().bits(), a.bits());
    first.gc_collect_exact();
    drop(first);
    let mut second = parked_second.resume_for_test().expect("resume second");
    assert_eq!(callback().bits(), b.bits());
    second.gc_collect_exact();
}

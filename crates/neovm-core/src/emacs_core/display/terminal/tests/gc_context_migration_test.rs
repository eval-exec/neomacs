use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn gc_tls_ownership_terminal_values_survive_owner_collection_and_reactivation() {
    let mut ctx = Context::new();
    let handle = terminal_handle_value();
    let key = Value::symbol("terminal-gc-moved-parameter");
    let child = Value::string("terminal payload belongs to its Context");
    let payload = Value::vector(vec![child]);
    builtin_set_terminal_parameter(&mut ctx, vec![handle, key, payload]).unwrap();
    let expected = [handle.bits(), payload.bits(), child.bits()];
    crate::tagged::gc::clear_tagged_heap_if_installed(&ctx.tagged_heap);

    let _other = Context::new();
    ctx.setup_thread_locals();
    // Do not access terminal TLS before collecting: its original Values
    // must be retained by the owning Context, rather than another local view.
    ctx.gc_collect_exact();
    for bits in expected {
        assert!(
            ctx.tagged_heap
                .owns_heap_value_for_test(Value::from_bits(bits)),
            "owner GC lost terminal Lisp state"
        );
    }
    let handle = terminal_handle_value();
    assert_eq!(handle.bits(), expected[0]);
    assert_eq!(
        builtin_terminal_parameter(&mut ctx, vec![handle, key])
            .unwrap()
            .bits(),
        expected[1]
    );
    crate::tagged::gc::clear_tagged_heap_if_installed(&ctx.tagged_heap);

    ctx.setup_thread_locals();
    let restored_handle = terminal_handle_value();
    assert_eq!(restored_handle.bits(), expected[0]);
    assert_eq!(
        builtin_terminal_parameter(&mut ctx, vec![restored_handle, key])
            .unwrap()
            .bits(),
        expected[1]
    );
    assert_eq!(payload.as_vector_data().unwrap()[0].bits(), expected[2]);
    assert_eq!(
        child.as_utf8_str().unwrap(),
        "terminal payload belongs to its Context"
    );
    ctx.gc_collect_exact();
}

#[test]
fn gc_tls_ownership_terminal_handle_reset_keeps_same_heap_parameters() {
    let mut ctx = Context::new();
    let key = Value::symbol("terminal-gc-reset-parameter");
    let payload = Value::vector(vec![Value::fixnum(42)]);
    builtin_set_terminal_parameter(&mut ctx, vec![Value::NIL, key, payload]).unwrap();
    reset_terminal_handle();
    ctx.gc_collect_exact();
    assert!(ctx.tagged_heap.owns_heap_value_for_test(payload));
    assert_eq!(
        builtin_terminal_parameter(&mut ctx, vec![Value::NIL, key])
            .unwrap()
            .bits(),
        payload.bits()
    );
}

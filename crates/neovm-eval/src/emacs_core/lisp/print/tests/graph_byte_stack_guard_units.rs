//! Existing byte-printer traversal prefixes survive Rust unwinding.
use super::super::*;
static_assertions::assert_not_impl_any!(BytePrintObjectStackGuard: Send, Sync);

#[test]
fn byte_printer_stack_guard_restores_existing_prefix_after_rust_unwind() {
    crate::test_utils::init_test_tracing();
    let outer = Value::vector(vec![Value::NIL]);
    let child = Value::vector(vec![Value::T]);
    let initial = PRINT_BYTES_OBJECT_STACK.with(|stack| stack.borrow().clone());
    let outer_guard = BytePrintObjectStackGuard::enter(&outer).expect("outer identity");
    let prefix = PRINT_BYTES_OBJECT_STACK.with(|stack| stack.borrow().clone());
    let result = std::panic::catch_unwind(|| {
        let _child_guard = BytePrintObjectStackGuard::enter(&child).expect("child identity");
        // Constructor must release its RefCell borrow before descent.
        let mut output = Vec::new();
        assert!(append_bytes_cycle_ref_if_any(&outer, &mut output));
        panic!("byte-render unwind");
    });
    assert!(result.is_err());
    assert_eq!(
        PRINT_BYTES_OBJECT_STACK.with(|stack| stack.borrow().clone()),
        prefix
    );
    drop(outer_guard);
    assert_eq!(
        PRINT_BYTES_OBJECT_STACK.with(|stack| stack.borrow().clone()),
        initial
    );
}

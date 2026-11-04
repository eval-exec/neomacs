//! Sole-owner phases are activation state, not extra per-opcode loans.
use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn sole_owner_cursor_nested_published_phases_keep_operand_roots() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context
        .bc_buf
        .push(Value::list(vec![Value::string("outer-only")]));
    let mut vm = Vm::from_context(&mut context);
    let outer = vm.sole_owner.driver();
    let cursor = RunLoopCursor::acquire(&mut vm, outer);
    let parked = cursor.publish(&mut vm);
    let inner = vm.sole_owner.driver();
    let cursor = RunLoopCursor::acquire(&mut vm, inner);
    let published = cursor.publish(&mut vm);
    vm.ctx.gc_collect_exact();
    let cursor = published.rejoin(&mut vm);
    assert_eq!(
        crate::emacs_core::print::print_value(&cursor.operands[0]),
        "(\"outer-only\")"
    );
    cursor.publish(&mut vm).finish();
    let cursor = parked.rejoin(&mut vm);
    assert_eq!(
        crate::emacs_core::print::print_value(&cursor.operands[0]),
        "(\"outer-only\")"
    );
    cursor.publish(&mut vm).finish();
    vm.ctx.bc_buf.clear();
}

#[test]
fn sole_owner_cursor_published_panic_retires_without_drop_work() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    context
        .bc_buf
        .push(Value::list(vec![Value::string("panic-only")]));
    let mut vm = Vm::from_context(&mut context);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let phase = vm.sole_owner.driver();
        let cursor = RunLoopCursor::acquire(&mut vm, phase);
        let _published = cursor.publish(&mut vm);
        panic!("permitted test panic after operand publication");
    }));
    assert!(result.is_err());
    #[cfg(debug_assertions)]
    debug_assert_no_live_stack_cursor();
    vm.ctx.gc_collect_exact();
    assert_eq!(
        crate::emacs_core::print::print_value(&vm.ctx.bc_buf[0]),
        "(\"panic-only\")"
    );
    let phase = vm.sole_owner.driver();
    let cursor = RunLoopCursor::acquire(&mut vm, phase);
    cursor.publish(&mut vm).finish();
    vm.ctx.bc_buf.clear();
}

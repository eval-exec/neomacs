//! Sole-owner phases are activation state, not extra per-opcode loans.
use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn sole_owner_cursor_nested_published_phases_keep_operand_roots() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new(crate::test_utils::unit_runtime());
    context
        .bc_buf
        .push(Value::list(vec![Value::string("outer-only")]));
    let mut vm = Vm::from_context(&mut context);
    let outer = vm.sole_owner.driver();
    // SAFETY: `vm.ctx.bc_buf` is not mutated or collected before `publish`.
    let cursor = unsafe { RunLoopCursor::acquire(&mut vm, outer) };
    // SAFETY: `cursor` was acquired from `vm` without reallocating `bc_buf`.
    let parked = unsafe { cursor.publish(&mut vm) };
    let inner = vm.sole_owner.driver();
    // SAFETY: `vm.ctx.bc_buf` is not mutated or collected before `publish`.
    let cursor = unsafe { RunLoopCursor::acquire(&mut vm, inner) };
    // SAFETY: `cursor` was acquired from `vm` without reallocating `bc_buf`.
    let published = unsafe { cursor.publish(&mut vm) };
    vm.ctx.gc_collect_exact();
    // SAFETY: `vm` is the same VM whose phase was published above.
    let cursor = unsafe { published.rejoin(&mut vm) };
    assert_eq!(
        crate::emacs_core::print::print_value(&cursor.operands[0]),
        "(\"outer-only\")"
    );
    // SAFETY: `cursor` was rejoined from `vm` without reallocating `bc_buf`.
    unsafe { cursor.publish(&mut vm) }.finish();
    // SAFETY: `vm` is the same VM whose outer phase was published above.
    let cursor = unsafe { parked.rejoin(&mut vm) };
    assert_eq!(
        crate::emacs_core::print::print_value(&cursor.operands[0]),
        "(\"outer-only\")"
    );
    // SAFETY: `cursor` was rejoined from `vm` without reallocating `bc_buf`.
    unsafe { cursor.publish(&mut vm) }.finish();
    vm.ctx.bc_buf.clear();
}

#[test]
fn sole_owner_cursor_published_panic_retires_without_drop_work() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new(crate::test_utils::unit_runtime());
    context
        .bc_buf
        .push(Value::list(vec![Value::string("panic-only")]));
    let mut vm = Vm::from_context(&mut context);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let phase = vm.sole_owner.driver();
        // SAFETY: `vm.ctx.bc_buf` is not mutated or collected before `publish`.
        let cursor = unsafe { RunLoopCursor::acquire(&mut vm, phase) };
        // SAFETY: `cursor` was acquired from `vm` without reallocating `bc_buf`.
        let _published = unsafe { cursor.publish(&mut vm) };
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
    // SAFETY: `vm.ctx.bc_buf` is not mutated or collected before `publish`.
    let cursor = unsafe { RunLoopCursor::acquire(&mut vm, phase) };
    // SAFETY: `cursor` was acquired from `vm` without reallocating `bc_buf`.
    unsafe { cursor.publish(&mut vm) }.finish();
    vm.ctx.bc_buf.clear();
}

/// RT V7: in debug builds, `StackCursor::publish` must assert that
/// `ctx.bc_buf`'s backing pointer matches the acquired `base` (catching both
/// reallocation and publishing to a different `Context`), while remaining
/// zero-cost (`unsafe fn` + `debug_assert!`) in release builds.
#[cfg(debug_assertions)]
#[test]
fn stack_cursor_publish_rejects_reallocated_or_foreign_bc_buf() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new(crate::test_utils::unit_runtime());
    context.bc_buf.push(Value::fixnum(1));
    let old_base = context.bc_buf.as_mut_ptr();
    // SAFETY: we never dereference `cursor` after reallocating; we only verify
    // that `publish` detects the moved backing pointer and panics in debug builds.
    let cursor = unsafe { StackCursor::acquire(&mut context) };
    context.bc_buf.reserve(4096);
    assert_ne!(
        context.bc_buf.as_mut_ptr(),
        old_base,
        "test requires bc_buf reallocation to move its backing pointer"
    );
    let published = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: debug builds panic in `debug_assert_eq!` before touching `set_len`.
        unsafe { cursor.publish(&mut context) };
    }));
    assert!(
        published.is_err(),
        "StackCursor::publish must reject a reallocated or foreign bc_buf"
    );
    context.bc_buf.clear();
}

/// RT V7: acquiring, publishing, or rejoining a raw operand-stack cursor must
/// require `unsafe`.
#[test]
#[deny(unused_unsafe)]
fn stack_cursor_acquire_and_rejoin_require_unsafe() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new(crate::test_utils::unit_runtime());
    // SAFETY: `context.bc_buf` is not mutated or collected while `cursor` is live.
    let cursor = unsafe { StackCursor::acquire(&mut context) };
    // SAFETY: `cursor` was acquired from `context` without reallocating `bc_buf`.
    unsafe { cursor.publish(&mut context) };

    let mut vm = Vm::from_context(&mut context);
    let phase = vm.sole_owner.driver();
    // SAFETY: `vm.ctx.bc_buf` stays un-reallocated until `publish`.
    let run_cursor = unsafe { RunLoopCursor::acquire(&mut vm, phase) };
    // SAFETY: `run_cursor` was acquired from `vm` without reallocating `bc_buf`.
    let published = unsafe { run_cursor.publish(&mut vm) };
    // SAFETY: `vm` is the same VM whose phase was published above.
    let rejoined = unsafe { published.rejoin(&mut vm) };
    // SAFETY: `rejoined` was rejoined from `vm` without reallocating `bc_buf`.
    unsafe { rejoined.publish(&mut vm) }.finish();
}

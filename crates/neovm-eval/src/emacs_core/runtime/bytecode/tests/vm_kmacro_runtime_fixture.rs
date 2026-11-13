//! The private-VM fixture for core's keyboard-macro runtime test.
//!
//! The caller supplies the canonical Context constructor. The original
//! assertions and the single VM borrow stay inside the owning module; no
//! Context or VM handle escapes through the test-support authority route.

use super::Vm;
use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

fn new_vm(eval: &mut Context) -> Vm<'_> {
    Vm::from_context(eval)
}

pub(crate) fn run_vm_kmacro_builtins_use_shared_runtime_state_fixture(
    make_context: fn() -> Context,
) {
    crate::test_utils::init_test_tracing();
    let mut eval = make_context();
    let _ = eval.eval_str_each(
        r#"(progn
             (setq vm-kmacro-shared-count 0)
             (setq vm-kmacro-ignore-direct-called nil)
             (fset 'ignore
                   (lambda ()
                     (setq vm-kmacro-ignore-direct-called t)))
             (fset 'command-execute (lambda (cmd &optional _record _keys _special) (funcall cmd)))
             (let ((g (make-sparse-keymap)))
               (use-global-map g)
               (define-key g [ignore]
                 (lambda ()
                   (interactive)
                   (setq vm-kmacro-shared-count (1+ vm-kmacro-shared-count))))))"#,
    );

    let mut vm = new_vm(&mut eval);
    assert_eq!(
        vm.dispatch_vm_builtin("start-kbd-macro", vec![Value::NIL, Value::NIL])
            .expect("vm start-kbd-macro"),
        Value::NIL
    );
    assert_eq!(
        vm.dispatch_vm_builtin("store-kbd-macro-event", vec![Value::symbol("ignore")])
            .expect("vm store-kbd-macro-event"),
        Value::NIL
    );
    vm.ctx.finalize_kbd_macro_runtime_chars();
    assert_eq!(
        vm.dispatch_vm_builtin("end-kbd-macro", vec![])
            .expect("vm end-kbd-macro"),
        Value::NIL
    );
    assert_eq!(
        vm.ctx
            .eval_symbol("last-kbd-macro")
            .expect("last-kbd-macro"),
        Value::vector(vec![Value::symbol("ignore")])
    );

    assert_eq!(
        vm.dispatch_vm_builtin("call-last-kbd-macro", vec![])
            .expect("vm call-last-kbd-macro"),
        Value::NIL
    );
    assert_eq!(
        vm.dispatch_vm_builtin(
            "execute-kbd-macro",
            vec![Value::vector(vec![Value::symbol("ignore")])]
        )
        .expect("vm execute-kbd-macro"),
        Value::NIL
    );

    assert_eq!(
        vm.ctx
            .eval_symbol("vm-kmacro-shared-count")
            .expect("vm-kmacro-shared-count"),
        Value::fixnum(2)
    );
    assert_eq!(
        vm.ctx
            .eval_symbol("vm-kmacro-ignore-direct-called")
            .expect("vm-kmacro-ignore-direct-called"),
        Value::NIL
    );
}

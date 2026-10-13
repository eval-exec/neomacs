//! GNU EQ for nonlocal control and feature callbacks.
use super::{CommandLoopExit, Context};
use crate::emacs_core::{print::print_value, value::Value};

#[test]
fn issue554_catch_tags_use_throw_time_symbol_policy() {
    for (form, expected) in [
        (
            "(let ((symbols-with-pos-enabled t)) (catch 'x (throw (position-symbol 'x 0) 7)))",
            "7",
        ),
        (
            "(let ((symbols-with-pos-enabled nil)) (catch 'x (let ((symbols-with-pos-enabled t)) (throw (position-symbol 'x 0) 7))))",
            "7",
        ),
        (
            "(let ((symbols-with-pos-enabled nil)) (condition-case err (catch 'x (throw (position-symbol 'x 0) 7)) (error (car err))))",
            "no-catch",
        ),
        (
            "(let ((symbols-with-pos-enabled t)) (catch nil (throw (position-symbol nil 0) 7)))",
            "7",
        ),
    ] {
        let mut ctx = Context::new();
        assert_eq!(
            print_value(&ctx.eval_str(form).expect("catch EQ")),
            expected,
            "{form}"
        );
    }
}

#[test]
fn issue554_provide_after_load_lookup_uses_symbol_policy() {
    let mut ctx = Context::new();
    let value = ctx
        .eval_str(
            "(let ((symbols-with-pos-enabled t) (features nil))
        (setq issue554-hit nil after-load-alist
              (list (list 'issue554-provide (lambda () (setq issue554-hit 7)))))
        (provide (position-symbol 'issue554-provide 0)) issue554-hit)",
        )
        .expect("provide callback EQ");
    assert_eq!(value, Value::fixnum(7));
}

#[test]
fn issue554_command_loop_exit_honors_positioned_true() {
    let mut ctx = Context::new();
    let positioned = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::T, Value::fixnum(0));
    ctx.set_variable("symbols-with-pos-enabled", Value::NIL);
    assert_eq!(
        ctx.classify_command_loop_exit(positioned).unwrap(),
        CommandLoopExit::Normal
    );
    ctx.set_variable("symbols-with-pos-enabled", Value::T);
    assert_eq!(
        ctx.classify_command_loop_exit(positioned).unwrap(),
        CommandLoopExit::Quit
    );
}

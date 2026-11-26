//! A callback named by a symbol whose function cell holds byte-code: GNU
//! `funcall_general` loads that cell once per call and enters the function.
//! The call must keep every observable step of the generic named call: the
//! backtrace frame names the symbol, the cell is read for every call (so a
//! redefinition, advice or `debug-on-entry` wrapper installed by a callback
//! is seen at the next element), the debugger runs before the callee, and
//! every other cell kind keeps the error it signals.

use super::{builtin_mapc_2, builtin_mapcar_2};
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::error::{EvalResult, FlowKind, FlowResultExt};
use neovm_value::intern::intern;
use neovm_value::value::{LambdaParams, Value};

fn context() -> Context {
    crate::test_utils::init_test_tracing();
    Context::new(crate::test_utils::unit_runtime())
}

/// `(lambda (x) x)`, or `(lambda (x) (funcall TARGET x))`, as byte-code.
fn bytecode(target: Option<Value>) -> Value {
    let mut code = ByteCodeFunction::new(LambdaParams::simple(vec![intern("symcallee-item")]));
    code.set_lexical(true);
    code.set_max_stack(crate::emacs_core::bytecode::StackDepth::for_test(8));
    if let Some(target) = target {
        code.set_constants(vec![target].into());
        code.set_ops(vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ]);
    } else {
        code.set_ops(vec![Op::StackRef(0), Op::Return]);
    }
    Value::make_bytecode(code)
}

/// NAME's function cell set to [`bytecode`] of TARGET; returns the symbol.
fn named(ctx: &mut Context, name: &'static str, target: Option<Value>) -> Value {
    let function = bytecode(target);
    ctx.obarray_mut().set_symbol_function(name, function);
    Value::symbol(name)
}

fn outcome(result: EvalResult) -> String {
    match result.kinded() {
        Ok(value) => print_value(&value),
        Err(FlowKind::Signal(sig)) => format!(
            "signal {} {:?}",
            sig.symbol_name(),
            sig.data.iter().map(print_value).collect::<Vec<_>>()
        ),
        Err(other) => format!("{other:?}"),
    }
}

/// Whether the generic named call resolved NAME since its cache entry was
/// dropped: `apply_symbol_callable_untraced` records every resolution there.
fn resolved_generically(ctx: &Context, name: &str) -> bool {
    ctx.named_call_cache.contains_key(&intern(name))
}

fn forget_resolution(ctx: &mut Context, name: &str) {
    ctx.named_call_cache.remove(&intern(name));
}

/// Collect, then reuse whatever the sweep freed: a value the test still
/// holds but no root keeps would now read back as junk.
fn collect_and_reuse(ctx: &mut Context) {
    ctx.gc_collect_exact();
    ctx.eval_str(
        "(let ((i 0) (junk nil))
           (while (< i 2000)
             (setq junk (cons (vector 'junk i) junk))
             (setq i (1+ i)))
           nil)",
    )
    .unwrap();
}

#[test]
fn mapcar_symbol_callee_bytecode_skips_generic_named_call() {
    let mut ctx = context();
    let roots = ctx.save_vm_roots();
    let function = named(&mut ctx, "symcallee-id", None);
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    ctx.push_vm_frame_root(input);
    let before = (ctx.depth, ctx.specpdl.len());

    forget_resolution(&mut ctx, "symcallee-id");
    let output = builtin_mapcar_2(&mut ctx, function, input).unwrap();
    assert_eq!(print_value(&output), "(1 2 3)");
    assert!(
        !resolved_generically(&ctx, "symcallee-id"),
        "mapcar re-resolved a byte-code callee through the generic named call"
    );

    forget_resolution(&mut ctx, "symcallee-id");
    assert_eq!(builtin_mapc_2(&mut ctx, function, input).unwrap(), input);
    assert!(
        !resolved_generically(&ctx, "symcallee-id"),
        "mapc re-resolved a byte-code callee through the generic named call"
    );
    assert_eq!((ctx.depth, ctx.specpdl.len()), before);
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_symbol_callee_redefined_by_itself_is_seen_next_element() {
    let mut ctx = context();
    ctx.eval_str(
        "(fset 'symcallee-swap
               (lambda (x)
                 (fset 'symcallee-self (lambda (y) (+ y 100)))
                 x))",
    )
    .unwrap();
    let function = named(
        &mut ctx,
        "symcallee-self",
        Some(Value::symbol("symcallee-swap")),
    );
    let roots = ctx.save_vm_roots();
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    ctx.push_vm_frame_root(input);
    assert_eq!(
        outcome(builtin_mapcar_2(&mut ctx, function, input)),
        "(1 102 103)"
    );
    collect_and_reuse(&mut ctx);

    // The replacement is byte-code too: the cell is still read per element.
    let replacement = bytecode(None);
    ctx.assign("symcallee-replacement", replacement);
    ctx.eval_str(
        "(fset 'symcallee-swap
               (lambda (x)
                 (fset 'symcallee-self symcallee-replacement)
                 (+ x 100)))",
    )
    .unwrap();
    named(
        &mut ctx,
        "symcallee-self",
        Some(Value::symbol("symcallee-swap")),
    );
    assert_eq!(
        outcome(builtin_mapc_2(&mut ctx, function, input)),
        "(1 2 3)"
    );
    collect_and_reuse(&mut ctx);
    named(
        &mut ctx,
        "symcallee-self",
        Some(Value::symbol("symcallee-swap")),
    );
    assert_eq!(
        outcome(builtin_mapcar_2(&mut ctx, function, input)),
        "(101 2 3)"
    );
    ctx.restore_vm_roots(roots);
}

#[test]
fn mapcar_symbol_callee_backtrace_frame_names_the_symbol() {
    let mut ctx = context();
    ctx.eval_str(
        "(progn
           (setq symcallee-frames nil)
           (fset 'symcallee-record
                 (lambda (x)
                   (mapbacktrace
                    (lambda (_evald fun args _flags)
                      (if (eq fun 'symcallee-traced)
                          (setq symcallee-frames
                                (cons (cons fun args) symcallee-frames)))))
                   x)))",
    )
    .unwrap();
    let function = named(
        &mut ctx,
        "symcallee-traced",
        Some(Value::symbol("symcallee-record")),
    );
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    assert_eq!(
        outcome(builtin_mapcar_2(&mut ctx, function, input)),
        "(1 2)"
    );
    assert_eq!(
        print_value(&ctx.eval_str("symcallee-frames").unwrap()),
        "((symcallee-traced 2) (symcallee-traced 1))"
    );
}

#[test]
fn mapcar_symbol_callee_debugger_runs_before_the_callee() {
    let mut ctx = context();
    ctx.eval_str(
        "(setq symcallee-debug-calls nil
               debugger (lambda (&rest args)
                          (if (eq (car args) 'exit)
                              (car (cdr args))
                            (setq symcallee-debug-calls
                                  (cons args symcallee-debug-calls))
                            nil)))",
    )
    .unwrap();
    let function = named(&mut ctx, "symcallee-debugged", None);
    let input = Value::list(vec![Value::fixnum(11), Value::fixnum(22)]);
    ctx.assign("debug-on-next-call", Value::T);
    assert_eq!(
        outcome(builtin_mapcar_2(&mut ctx, function, input)),
        "(11 22)"
    );
    assert_eq!(
        print_value(&ctx.eval_str("symcallee-debug-calls").unwrap()),
        "((lambda))"
    );
}

/// GNU 31 implements both `advice-add` and `debug-on-entry` by replacing the
/// function cell with a wrapper (`nadvice.el`, `debug.el`). The wrapper is
/// reached through the symbol at the next element, whether it is byte-code
/// (an advice object) or an interpreted closure.
#[test]
fn mapcar_symbol_callee_wrapper_installed_mid_map_is_called_next_element() {
    let mut ctx = context();
    for (label, wrapper_src, expected) in [
        (
            "interpreted advice",
            "(lambda (x) (list 'around (funcall symcallee-original x)))",
            "(1 (around 2) (around 3))",
        ),
        (
            "debug-on-entry",
            "(lambda (x)
               (setq symcallee-entries (cons x symcallee-entries))
               (funcall symcallee-original x))",
            "(1 2 3)",
        ),
    ] {
        ctx.eval_str("(setq symcallee-entries nil)").unwrap();
        let original = bytecode(None);
        ctx.assign("symcallee-original", original);
        let wrapper = ctx.eval_str(wrapper_src).unwrap();
        ctx.assign("symcallee-wrapper", wrapper);
        ctx.eval_str(
            "(fset 'symcallee-install
                   (lambda (x)
                     (fset 'symcallee-advised symcallee-wrapper)
                     x))",
        )
        .unwrap();
        let function = named(
            &mut ctx,
            "symcallee-advised",
            Some(Value::symbol("symcallee-install")),
        );
        let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
        assert_eq!(
            outcome(builtin_mapcar_2(&mut ctx, function, input)),
            expected,
            "{label}"
        );
        if label == "debug-on-entry" {
            assert_eq!(
                print_value(&ctx.eval_str("symcallee-entries").unwrap()),
                "(3 2)"
            );
        }
    }

    // An advice object is itself byte-code: the advised symbol stays on the
    // byte-code path and calls through the advice.
    let advice = bytecode(Some(Value::symbol("symcallee-around")));
    ctx.eval_str("(fset 'symcallee-around (lambda (x) (list 'around x)))")
        .unwrap();
    ctx.obarray_mut()
        .set_symbol_function("symcallee-advised", advice);
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    assert_eq!(
        outcome(builtin_mapcar_2(
            &mut ctx,
            Value::symbol("symcallee-advised"),
            input
        )),
        "((around 1) (around 2))"
    );
}

#[test]
fn mapcar_symbol_callee_other_cell_kinds_keep_their_signals() {
    let mut ctx = context();
    let target = bytecode(None);
    ctx.obarray_mut()
        .set_symbol_function("symcallee-target", target);
    ctx.eval_str(
        "(progn
           (fset 'symcallee-alias-2 'symcallee-target)
           (fset 'symcallee-alias-1 'symcallee-alias-2)
           (fset 'symcallee-autoload
                 '(autoload \"symcallee-no-such-file\" nil nil nil))
           (fset 'symcallee-macro (cons 'macro (lambda (x) x)))
           (fmakunbound 'symcallee-void))",
    )
    .unwrap();
    ctx.assign("symcallee-macro-bytecode", bytecode(None));
    ctx.eval_str("(fset 'symcallee-bytecode-macro (cons 'macro symcallee-macro-bytecode))")
        .unwrap();
    let roots = ctx.save_vm_roots();
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2)]);
    ctx.push_vm_frame_root(input);
    let mut observed = Vec::new();
    for name in [
        "symcallee-alias-1",
        "symcallee-autoload",
        "symcallee-macro",
        "symcallee-bytecode-macro",
        "symcallee-void",
    ] {
        let before = (ctx.depth, ctx.specpdl.len());
        let mapped = outcome(builtin_mapcar_2(&mut ctx, Value::symbol(name), input));
        let applied = outcome(ctx.apply1(Value::symbol(name), Value::fixnum(1)));
        assert_eq!((ctx.depth, ctx.specpdl.len()), before, "{name}");
        observed.push(format!("{name}: {mapped} | {applied}"));
        collect_and_reuse(&mut ctx);
    }
    assert_eq!(print_value(&input), "(1 2)");
    ctx.restore_vm_roots(roots);
    // As on the generic path. GNU `funcall_general` names the symbol in a
    // macro's `invalid-function` (eval.c); the cell value named here is the
    // generic path's existing answer, kept unchanged.
    let missing = "signal file-missing [\"\\\"Cannot open load file\\\"\", \"\\\"No such file or directory\\\"\", \"\\\"symcallee-no-such-file\\\"\"]";
    let macro_signal = "signal invalid-function [\"(macro . #[(x) (x) nil])\"]";
    let bytecode_macro_signal =
        "signal invalid-function [\"(macro . #[(symcallee-item) nil [] 8])\"]";
    let void_signal = "signal void-function [\"symcallee-void\"]";
    assert_eq!(
        observed,
        [
            "symcallee-alias-1: (1 2) | 1".to_string(),
            format!("symcallee-autoload: {missing} | {missing}"),
            format!("symcallee-macro: {macro_signal} | {macro_signal}"),
            format!("symcallee-bytecode-macro: {bytecode_macro_signal} | {bytecode_macro_signal}"),
            format!("symcallee-void: {void_signal} | {void_signal}"),
        ]
    );
}

#[test]
fn apply1_symbol_callee_bytecode_skips_generic_named_call() {
    let mut ctx = context();
    let function = named(&mut ctx, "symcallee-applied", None);
    let before = (ctx.depth, ctx.specpdl.len());

    assert_eq!(
        outcome(ctx.apply(function, vec![Value::fixnum(8), Value::fixnum(9)])),
        "signal wrong-number-of-arguments [\"(1 . 1)\", \"2\"]"
    );

    forget_resolution(&mut ctx, "symcallee-applied");
    assert_eq!(outcome(ctx.apply1(function, Value::fixnum(7))), "7");
    assert!(
        !resolved_generically(&ctx, "symcallee-applied"),
        "apply1 re-resolved a byte-code callee through the generic named call"
    );

    forget_resolution(&mut ctx, "symcallee-applied");
    assert_eq!(outcome(ctx.apply(function, vec![Value::fixnum(8)])), "8");
    assert!(
        !resolved_generically(&ctx, "symcallee-applied"),
        "apply re-resolved a byte-code callee through the generic named call"
    );

    // Lisp `apply` and `funcall` enter through `apply_from_lisp_funcall`.
    forget_resolution(&mut ctx, "symcallee-applied");
    assert_eq!(
        print_value(
            &ctx.eval_str(
                "(list (apply 'symcallee-applied '(10)) (funcall 'symcallee-applied 11))"
            )
            .unwrap()
        ),
        "(10 11)"
    );
    assert!(
        !resolved_generically(&ctx, "symcallee-applied"),
        "Lisp apply/funcall re-resolved a byte-code callee through the generic named call"
    );
    assert_eq!((ctx.depth, ctx.specpdl.len()), before);
}

#[test]
fn run_hook_symbol_callee_bytecode_skips_generic_named_call() {
    let mut ctx = context();
    ctx.eval_str(
        "(progn
           (setq symcallee-hook-calls nil)
           (fset 'symcallee-hook-record
                 (lambda (x)
                   (setq symcallee-hook-calls (cons x symcallee-hook-calls))
                   nil))
           (setq symcallee-hook '(symcallee-hooked symcallee-hooked)))",
    )
    .unwrap();
    named(
        &mut ctx,
        "symcallee-hooked",
        Some(Value::symbol("symcallee-hook-record")),
    );
    forget_resolution(&mut ctx, "symcallee-hooked");
    assert_eq!(
        print_value(
            &ctx.eval_str("(run-hook-with-args 'symcallee-hook 5)")
                .unwrap()
        ),
        "nil"
    );
    assert_eq!(
        print_value(
            &ctx.eval_str("(run-hook-with-args-until-success 'symcallee-hook 6)")
                .unwrap()
        ),
        "nil"
    );
    assert_eq!(
        print_value(&ctx.eval_str("symcallee-hook-calls").unwrap()),
        "(6 6 5 5)"
    );
    assert!(
        !resolved_generically(&ctx, "symcallee-hooked"),
        "a hook re-resolved a byte-code function through the generic named call"
    );
}

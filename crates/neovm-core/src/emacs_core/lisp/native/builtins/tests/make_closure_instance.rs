//! `make-closure` (GNU `Fmake_closure`, alloc.c): a FRESH closure object per
//! call whose constant vector is a fresh copy of the prototype's with the
//! first N slots replaced by the captured values, and every other slot the
//! prototype's. Neomacs builds the instance without an owned argument
//! vector and without a whole-function temporary; these tests pin the
//! observable result.
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{LambdaParams, Value};
use crate::tagged::header::SubrFn;

fn eval(ctx: &mut Context, src: &str) -> Value {
    let v = ctx.eval_str(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

/// A GNU-shaped prototype: `(lambda () V0)` over `[V0 TAIL]`, with a
/// docstring, an interactive spec and two extra slots.
const PROTOTYPE: &str = "(setq mci-proto (make-byte-code 0 \"\\300\\207\" \
                         [V0 mci-tail] 1 \"Doc.\" \"p\" 'mci-x1 'mci-x2))";

#[test]
fn make_closure_receives_its_arguments_as_a_slice() {
    let _ctx = Context::new();
    let entry = crate::emacs_core::eval::lookup_global_subr_entry(intern("make-closure"))
        .expect("make-closure is a builtin subr");
    assert!(
        matches!(entry.function, Some(SubrFn::ManySlice(_))),
        "make-closure must not copy its arguments into an owned Vec per call"
    );
}

#[test]
fn make_closure_is_reachable_through_funcall_apply_and_bytecode() {
    let mut ctx = Context::new();
    eval(&mut ctx, PROTOTYPE);
    assert_eq!(
        eval(
            &mut ctx,
            "(funcall (funcall #'make-closure mci-proto 'by-funcall))"
        ),
        Value::symbol("by-funcall")
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(funcall (apply #'make-closure mci-proto '(by-apply)))"
        ),
        Value::symbol("by-apply")
    );
    assert_eq!(
        eval(&mut ctx, "(funcall (make-closure mci-proto 'by-call))"),
        Value::symbol("by-call")
    );
    // The bytecode `call` op: `(make-closure mci-proto 'by-bytecode)`.
    let proto = eval(&mut ctx, "mci-proto");
    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    caller.lexical = true;
    caller.constants = vec![
        Value::symbol("make-closure"),
        proto,
        Value::symbol("by-bytecode"),
    ]
    .into();
    caller.ops = vec![
        Op::Constant(0),
        Op::Constant(1),
        Op::Constant(2),
        Op::Call(2),
        Op::Return,
    ];
    caller.max_stack = 3;
    caller.seal_hand_assembled_ops();
    let closure = {
        let mut vm = crate::emacs_core::bytecode::Vm::from_context(&mut ctx);
        vm.execute(&caller, vec![]).expect("bytecode make-closure")
    };
    crate::emacs_core::eval::push_scratch_gc_root(closure);
    ctx.set_variable("mci-by-bytecode", closure);
    assert_eq!(
        eval(&mut ctx, "(funcall mci-by-bytecode)"),
        Value::symbol("by-bytecode")
    );
    // Arity and type errors keep GNU's shape.
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (make-closure) (error (car err)))"
        ),
        Value::symbol("wrong-number-of-arguments")
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (make-closure 'not-a-prototype 1) (error err))"
        ),
        eval(
            &mut ctx,
            "'(wrong-type-argument byte-code-function-p not-a-prototype)"
        )
    );
}

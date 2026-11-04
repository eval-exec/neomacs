//! Guarded `car`/`cdr` split on the cons tag: a cons loads its field, nil
//! yields nil, anything else deopts so the interpreter signals GNU's
//! `(wrong-type-argument listp X)`. The J0 harness still fails the guard for
//! every input, conses included.

use super::*;
use crate::emacs_core::print::print_value;

fn run(op: Op, v: Value) -> Option<usize> {
    lower_nullary_leaf(&[Op::Constant(0), op, Op::Return], &[v])
        .unwrap()
        .call_for_test(&[])
}

#[test]
fn guarded_car_cdr_split_cons_nil_and_non_lists() {
    // No GC safepoint in the JIT call, so the heap locals stay alive across it.
    let cons = Value::cons(Value::make_int(11), Value::make_int(22));
    let string = Value::string("x");
    let float = Value::make_float(1.5);
    assert_eq!(run(Op::Car, cons), Some(Value::make_int(11).bits()));
    assert_eq!(run(Op::Cdr, cons), Some(Value::make_int(22).bits()));
    for op in [Op::Car, Op::Cdr] {
        assert_eq!(run(op.clone(), Value::NIL), Some(Value::NIL.bits()));
        for non_list in [Value::make_int(5), Value::T, string, float] {
            assert_eq!(run(op.clone(), non_list), None, "{op:?} of a non-list");
        }
    }
}

#[test]
fn forced_deopt_fails_the_car_cdr_guard_for_a_cons_too() {
    let cons = Value::cons(Value::make_int(11), Value::make_int(22));
    crate::emacs_core::jit::compile::force_deopt_for_test(true);
    let car = run(Op::Car, cons);
    let cdr = run(Op::Cdr, cons);
    let nil = run(Op::Car, Value::NIL);
    crate::emacs_core::jit::compile::force_deopt_for_test(false);
    assert_eq!((car, cdr, nil), (None, None, None));
    assert_eq!(run(Op::Car, cons), Some(Value::make_int(11).bits()));
}

#[test]
fn hot_cadr_loop_keeps_gnu_results_for_cons_nil_and_non_list() {
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.eval_str(
        "(defun jit-car-cdr-split-cadrs (l)
           (let (acc)
             (while l
               (setq acc (cons (car (cdr l)) acc))
               (setq l (cdr (cdr l))))
             (nreverse acc)))",
    )
    .expect("defun");
    ctx.eval_str("(byte-compile 'jit-car-cdr-split-cadrs)")
        .expect("byte-compile");
    ctx.eval_str("(dotimes (_ 3000) (jit-car-cdr-split-cadrs '(1 2 3 4 5 6)))")
        .expect("warm up");
    for (list, want) in [
        ("'(1 2 3 4)", "(2 4)"),
        ("'(1 2 3)", "(2 nil)"),
        ("'(1 . 5)", "(wrong-type-argument listp 5)"),
        ("'(1 2 3 . 7)", "(wrong-type-argument listp 7)"),
    ] {
        let form = format!("(condition-case e (jit-car-cdr-split-cadrs {list}) (error e))");
        let got = ctx.eval_str(&form).expect("call");
        assert_eq!(print_value(&got), want, "{list}");
    }
}

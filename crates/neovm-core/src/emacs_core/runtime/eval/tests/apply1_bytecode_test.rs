#![cfg(feature = "jit")]
//! `Context::apply1` — the one-argument callback path every map builtin uses —
//! folds `apply_internal`'s steps and the armed native entry into one frame
//! for a byte-code function. It must answer exactly as the general path for
//! every parameter shape (the armed leaf's pure pass-through, and the
//! `&optional`/`&rest` marshal), leave depth and specpdl as it found them on a
//! signal, and keep the argument alive across a collecting callee.

use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::error::{Flow, FlowKind, FlowResultExt};
use crate::emacs_core::eval::{
    Context, LispArgVec, push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::intern::SymId;
use crate::emacs_core::jit::cache;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::{LambdaParams, Value};

/// `(lambda (REQUIRED... &optional OPTIONAL... &rest REST) (list ALL...))`.
fn lister(required: u32, optional: u32, rest: bool, hot: bool) -> ByteCodeFunction {
    let nonrest = required + optional;
    let n = nonrest + u32::from(rest);
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=required).map(SymId).collect(),
        optional: (required + 1..=nonrest).map(SymId).collect(),
        rest: rest.then_some(SymId(nonrest + 1)),
    });
    f.lexical = true;
    let mut ops: Vec<Op> = (0..n).map(|_| Op::StackRef((n - 1) as u16)).collect();
    ops.push(Op::List(n as u16));
    ops.push(Op::Return);
    f.ops = ops;
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    if hot {
        f.jit_runtime().set_hot_for_test();
    }
    f
}

fn outcome(r: Result<Value, Flow>) -> String {
    match r.kinded() {
        Ok(v) => print_value(&v),
        Err(FlowKind::Signal(sig)) => format!(
            "signal {} {:?}",
            sig.symbol_name(),
            sig.data.iter().map(print_value).collect::<Vec<_>>()
        ),
        Err(other) => format!("{other:?}"),
    }
}

#[test]
fn apply1_matches_the_general_path_for_every_parameter_shape() {
    crate::test_utils::init_test_tracing();
    for gc_stress in [false, true] {
        let mut ev = Context::new();
        ev.gc_stress = gc_stress;
        let _roots = ev.save_vm_roots();
        for (required, optional, rest) in [
            (1, 0, false),
            (0, 1, false),
            (1, 1, false),
            (0, 0, true),
            (1, 0, true),
            (0, 2, false),
            (2, 0, false),
            (0, 0, false),
        ] {
            let fast = Value::make_bytecode(lister(required, optional, rest, true));
            let slow = Value::make_bytecode(lister(required, optional, rest, false));
            ev.push_vm_frame_root(fast);
            ev.push_vm_frame_root(slow);
            let shape = format!("({required} req, {optional} opt, rest {rest})");
            // The first call tiers the hot function up and arms its slot.
            let _ = ev.apply1(fast, Value::make_int(0));
            let accepts = required <= 1 && (rest || required + optional >= 1);
            if accepts {
                let bc = fast.get_bytecode_data().unwrap();
                assert!(
                    cache::armed_leaf_for_stack_call(bc, 1).is_some(),
                    "{shape}: the hot function must be armed for the fast path"
                );
            }
            for src in ["5", "'x", "(list 1 2)", "\"str\""] {
                let arg = ev.eval_str(src).expect("arg");
                ev.push_vm_frame_root(arg);
                let depth = ev.depth;
                let specpdl = ev.specpdl.len();
                let got = outcome(ev.apply1(fast, arg));
                assert_eq!(
                    (ev.depth, ev.specpdl.len()),
                    (depth, specpdl),
                    "{shape} {src}"
                );
                let mut args = LispArgVec::new();
                args.push(arg);
                let want = outcome(ev.apply(slow, args));
                assert_eq!(got, want, "{shape} on {src} (gc_stress {gc_stress})");
            }
        }
    }
}

/// A signal inside the callback unwinds the frame `apply1` pushed.
#[test]
fn apply1_unwinds_its_frame_on_a_signal() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let _roots = ev.save_vm_roots();
    // (lambda (a) (car a))
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::StackRef(0), Op::Car, Op::Return];
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    f.jit_runtime().set_hot_for_test();
    let func = Value::make_bytecode(f);
    ev.push_vm_frame_root(func);
    let cons = ev.eval_str("(cons 7 8)").expect("cons");
    ev.push_vm_frame_root(cons);
    assert_eq!(outcome(ev.apply1(func, cons)), "7");
    assert_eq!(outcome(ev.apply1(func, cons)), "7", "armed");
    let depth = ev.depth;
    let specpdl = ev.specpdl.len();
    let scratch_base = save_scratch_gc_roots();
    push_scratch_gc_roots(&[cons, func]);
    let caller_roots = save_scratch_gc_roots();
    assert_eq!(
        outcome(ev.apply1(func, Value::make_int(5))),
        "signal wrong-type-argument [\"listp\", \"5\"]"
    );
    assert_eq!((ev.depth, ev.specpdl.len()), (depth, specpdl));
    assert_eq!(save_scratch_gc_roots(), caller_roots);
    restore_scratch_gc_roots(scratch_base);
}

/// End to end through the map builtins.
#[test]
fn map_builtins_call_a_compiled_closure_through_apply1() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let f = Value::make_bytecode(lister(1, 0, false, true));
    let sym = crate::emacs_core::intern::intern("apply1-test-lister");
    ev.obarray.set_symbol_function_id(sym, f);
    for _ in 0..3 {
        assert_eq!(
            print_value(
                &ev.eval_str("(mapcar 'apply1-test-lister '(1 2 3))")
                    .expect("mapcar")
            ),
            "((1) (2) (3))"
        );
        assert_eq!(
            print_value(
                &ev.eval_str("(let ((n 0)) (mapc (lambda (x) (setq n (+ n (car (apply1-test-lister x))))) [1 2 3]) n)")
                    .expect("mapc")
            ),
            "6"
        );
    }
}

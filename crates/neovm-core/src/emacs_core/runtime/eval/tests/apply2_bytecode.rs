#![cfg(feature = "jit")]
//! Two-argument bytecode callbacks must use the same parameter marshaling,
//! collection roots and unwind protocol as the general apply path.

use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    Context, LispArgVec, push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::intern::SymId;
use crate::emacs_core::jit::cache;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::{LambdaParams, Value};

fn lister(required: u32, optional: u32, rest: bool, hot: bool) -> ByteCodeFunction {
    let nonrest = required + optional;
    let n = nonrest + u32::from(rest);
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: (1..=required).map(SymId).collect(),
        optional: (required + 1..=nonrest).map(SymId).collect(),
        rest: rest.then_some(SymId(nonrest + 1)),
    });
    function.lexical = true;
    function.ops = (0..n).map(|_| Op::StackRef((n - 1) as u16)).collect();
    function.ops.extend([Op::List(n as u16), Op::Return]);
    function.max_stack = 16;
    if hot {
        function.jit_runtime().set_hot_for_test();
    }
    function
}

fn outcome(result: Result<Value, Flow>) -> String {
    match result {
        Ok(value) => print_value(&value),
        Err(Flow::Signal(signal)) => format!(
            "signal {} {:?}",
            signal.symbol_name(),
            signal.data.iter().map(print_value).collect::<Vec<_>>()
        ),
        Err(other) => format!("{other:?}"),
    }
}

#[test]
fn apply2_bytecode_matches_general_apply_for_every_parameter_shape() {
    crate::test_utils::init_test_tracing();
    for gc_stress in [false, true] {
        let mut eval = Context::new();
        eval.gc_stress = gc_stress;
        let roots = eval.save_vm_roots();
        for (required, optional, rest) in [
            (2, 0, false),
            (1, 1, false),
            (0, 2, false),
            (2, 1, false),
            (0, 3, false),
            (0, 0, true),
            (1, 0, true),
            (2, 0, true),
            (1, 2, true),
            (3, 0, false),
            (1, 0, false),
        ] {
            let fast = Value::make_bytecode(lister(required, optional, rest, true));
            let slow = Value::make_bytecode(lister(required, optional, rest, false));
            eval.push_vm_frame_root(fast);
            eval.push_vm_frame_root(slow);
            let shape = format!("{required} required, {optional} optional, rest {rest}");
            let _ = eval.apply2_bytecode(fast, Value::NIL, Value::NIL);
            if required <= 2 && (rest || required + optional >= 2) {
                assert!(
                    cache::armed_leaf_for_stack_call(fast.get_bytecode_data().unwrap(), 2)
                        .is_some(),
                    "{shape}: this must exercise an armed leaf"
                );
            }
            for (left, right) in [("5", "'x"), ("(list 1 2)", "\"right\"")] {
                let left = eval.eval_str(left).expect("left argument");
                eval.push_vm_frame_root(left);
                let right = eval.eval_str(right).expect("right argument");
                eval.push_vm_frame_root(right);
                let before = (eval.depth, eval.specpdl.len());
                let expected = outcome(eval.apply(slow, LispArgVec::from_slice(&[left, right])));
                assert_eq!(
                    outcome(eval.apply2_bytecode(fast, left, right)),
                    expected,
                    "{shape}"
                );
                assert_eq!((eval.depth, eval.specpdl.len()), before, "{shape}");
                assert_eq!(
                    outcome(eval.apply2_bytecode_unobserved(fast, left, right)),
                    expected,
                    "unobserved {shape}"
                );
                assert_eq!(
                    (eval.depth, eval.specpdl.len()),
                    before,
                    "unobserved {shape}"
                );
            }
        }
        eval.restore_vm_roots(roots);
    }
}

#[test]
fn apply2_bytecode_unwinds_frame_and_caller_roots_on_signal() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let roots = eval.save_vm_roots();
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1), SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.ops = vec![Op::StackRef(1), Op::Car, Op::Return];
    function.max_stack = 8;
    function.jit_runtime().set_hot_for_test();
    let function = Value::make_bytecode(function);
    eval.push_vm_frame_root(function);
    let valid = Value::cons(Value::fixnum(7), Value::fixnum(8));
    eval.push_vm_frame_root(valid);
    eval.apply2_bytecode(function, valid, Value::NIL)
        .expect("tier up");
    let before = (eval.depth, eval.specpdl.len());
    let scratch_base = save_scratch_gc_roots();
    push_scratch_gc_roots(&[valid, function]);
    let caller_roots = save_scratch_gc_roots();
    let invalid = Value::fixnum(5);
    let expected = outcome(eval.apply(function, LispArgVec::from_slice(&[invalid, valid])));
    assert_eq!(
        outcome(eval.apply2_bytecode(function, invalid, valid)),
        expected
    );
    assert_eq!((eval.depth, eval.specpdl.len()), before);
    assert_eq!(save_scratch_gc_roots(), caller_roots);
    restore_scratch_gc_roots(scratch_base);
    eval.restore_vm_roots(roots);
}

#[test]
fn apply2_bytecode_probed_stack_keeps_two_arguments_and_balances_depth() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let roots = eval.save_vm_roots();
    let function = Value::make_bytecode(lister(2, 0, false, true));
    eval.push_vm_frame_root(function);
    let left = Value::cons(Value::fixnum(1), Value::NIL);
    let right = Value::string("two");
    eval.push_vm_frame_root(left);
    eval.push_vm_frame_root(right);
    eval.depth = super::STACK_GROWTH_PROBE_START_DEPTH - 1;
    let before = (eval.depth, eval.specpdl.len());
    let expected = outcome(eval.apply(function, LispArgVec::from_slice(&[left, right])));
    assert_eq!(
        outcome(eval.apply2_bytecode(function, left, right)),
        expected
    );
    assert_eq!((eval.depth, eval.specpdl.len()), before);
    assert_eq!(
        outcome(eval.apply2_bytecode_unobserved(function, left, right)),
        expected
    );
    assert_eq!((eval.depth, eval.specpdl.len()), before);
    eval.depth = 0;
    eval.restore_vm_roots(roots);
}

#[test]
fn apply2_bytecode_preserves_bytecode_read_observation() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let roots = eval.save_vm_roots();
    let function = Value::make_bytecode(lister(2, 0, false, false));
    eval.push_vm_frame_root(function);
    let (result, reads) = crate::tagged::collection_reads::capture(|| {
        eval.apply2_bytecode(function, Value::fixnum(1), Value::fixnum(2))
    });
    result.expect("an observed bytecode call");
    let reads = reads.expect("the callback has coherent collection reads");
    assert!(reads.unchanged());
    crate::tagged::mutate::LispCollectionRevision::changed(function);
    assert!(
        !reads.unchanged(),
        "the bytecode projection must remain observed"
    );
    eval.restore_vm_roots(roots);
}

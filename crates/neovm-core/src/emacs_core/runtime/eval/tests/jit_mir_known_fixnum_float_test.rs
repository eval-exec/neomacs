#![cfg(feature = "jit")]
//! The MIR tier's known-fixnum fixpoint must honour `Float` feedback, like
//! the baseline's slot analysis (see `jit_known_fixnum_float.rs`).
//!
//! `build_mir_with_feedback` types a `+ - * /` result `Any` at a `Float` site,
//! so `infer_value_types` never proves a loop-carried float a fixnum and the
//! lowering keeps the guard on it. Today the float-site gate routes such a
//! body to the baseline before the MIR lowering runs; this test pins the
//! MIR-level fact so it is load-bearing the day that gate is lifted or a MIR
//! f64 arm lands, and the end-to-end GNU differential alongside it.

use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::decode::{decode_gnu_bytecode, parse_arglist_descriptor};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::mir::{self, LispType, MirOp};
use crate::emacs_core::value::{Value, ValueKind};

/// GNU 31.1's `byte-compile` of the MIR-eligible probe (required-only, no
/// `&optional`):
///
///     (defun probe-m (a n)
///       (let ((x (* a 1.5)))
///         (dotimes (_ n) (setq x (+ x 1.0)))
///         (if (> n 100) (1+ x) x)))
///
/// `x` is a Float-site `*` result in the entry block and a Float-site `+`
/// result on the loop back-edge; the `1+` at pc 28 never runs while n <= 100.
fn probe() -> ByteCodeFunction {
    let bytes: [u8; 31] = [
        1, 192, 95, 193, 137, 3, 87, 131, 21, 0, 194, 2, 195, 92, 178, 3, 136, 84, 130, 4, 0, 136,
        1, 196, 86, 131, 30, 0, 84, 135, 135,
    ];
    let mut constants = vec![
        Value::make_float(1.5),
        Value::make_int(0),
        Value::NIL,
        Value::make_float(1.0),
        Value::make_int(100),
    ];
    let ops = decode_gnu_bytecode(&bytes, &mut constants).expect("GNU bytecode decodes");
    let mut f = ByteCodeFunction::new(parse_arglist_descriptor(514));
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 7;
    f
}

fn call(ev: &mut Context, f: Value, a: Value, n: i64) -> Value {
    ev.funcall_general_untraced(f, vec![a, Value::make_int(n)])
        .expect("call completes")
}

fn as_f64(v: Value) -> f64 {
    assert!(
        matches!(v.kind(), ValueKind::Float),
        "expected a float result, got {v:?}"
    );
    v.xfloat()
}

#[test]
fn a_mir_eligible_float_loop_keeps_the_guard_on_its_cold_add1() {
    let mut ev = Context::new();
    let f = Value::make_bytecode(probe());
    let ValueKind::Symbol(id) = Value::symbol("mir-known-fixnum-float-probe").kind() else {
        panic!("symbol")
    };
    ev.obarray.set_symbol_function_id(id, f);
    let bc = f.get_bytecode_data().expect("bytecode");

    // Warm on the interpreter with a FLOAT `a` and n <= 100: the `*` and `+`
    // sites record Float; the `1+` site never executes.
    for _ in 0..64 {
        assert_eq!(as_f64(call(&mut ev, f, Value::make_float(2.0), 3)), 6.0);
    }
    // The feedback a tier-up would publish, snapshotted BEFORE the compile
    // consumes it.
    let ops = bc.executable_ops().to_vec();
    let snapshot: Vec<NumericFeedback> = (0..ops.len())
        .map(|pc| bc.jit_runtime().numeric_feedback(pc))
        .collect();
    assert!(
        snapshot.contains(&NumericFeedback::Float),
        "warm-up must have recorded Float somewhere, or the probe is vacuous"
    );

    // (b) The MIR-level fact, independent of which tier the gate picks.
    let map = bc.executable_gnu_byte_offset_map();
    let reach = crate::emacs_core::jit::compile::MirReach::OFF;
    let m = mir::build_mir_with_feedback(&ops, &bc.constants, map, 2, reach, &|pc| snapshot[pc])
        .expect("probe builds as MIR");
    let float_bins: Vec<&mir::MirInst> = m
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .filter(|i| matches!(i.op, MirOp::Bin(mir::BinKind::Mul | mir::BinKind::Add, ..)))
        .collect();
    assert_eq!(float_bins.len(), 2, "the `*` and the `+`");
    for b in &float_bins {
        assert_eq!(
            b.ty,
            LispType::Any,
            "a Float-site {:?} is not a known fixnum",
            b.op
        );
    }
    let types = mir::infer_value_types(&m);
    // Two `1+`s: the dotimes counter's (in the loop) and the cold `(1+ x)`
    // after it. The fixpoint must tell them apart.
    let add1_operands: Vec<mir::MirValue> = m
        .blocks
        .iter()
        .flat_map(|b| b.insts.iter())
        .filter_map(|i| match i.op {
            MirOp::Unary(mir::UnaryKind::Add1, x) => Some(x),
            _ => None,
        })
        .collect();
    let [counter, x] = add1_operands[..] else {
        panic!("expected the counter's 1+ and the cold 1+, got {add1_operands:?}");
    };
    assert_eq!(
        types[counter.0 as usize],
        LispType::Fixnum,
        "the dotimes counter is a loop-carried fixnum: its guard goes"
    );
    assert_eq!(
        types[x.0 as usize],
        LispType::Any,
        "the cold 1+'s operand is the loop-carried float: its guard must stay"
    );

    // (a) End to end: tier up with that feedback, then take the cold branch.
    bc.jit_runtime().set_hot_for_test();
    assert_eq!(as_f64(call(&mut ev, f, Value::make_float(2.0), 3)), 6.0);
    assert!(
        bc.jit_runtime().compiled_id().is_some(),
        "the probe must actually have been compiled, or this test is vacuous"
    );
    let cold = call(&mut ev, f, Value::make_float(2.0), 200);
    assert_eq!(
        as_f64(cold),
        204.0,
        "(1+ 203.0) must be 204.0 (GNU); a value near 3.5e13 means a float \
         pointer was shifted as a fixnum"
    );
}

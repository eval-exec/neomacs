#![cfg(feature = "jit")]
//! Regression: the baseline tier's known-fixnum slot analysis must read the
//! same `NumericFeedback` snapshot the lowering reads.
//!
//! `compute_known_fixnum_slots` labelled EVERY `+ - * /` result a known fixnum.
//! But at a site whose feedback is `Float` the lowering emits the f64 path and
//! BOXES the result. A float produced that way, flowing across a block edge
//! into a `1+` whose own site never ran during warm-up (feedback FixnumOnly),
//! reached `stack_as_raw` with its `guard_fixnum` elided — and the float
//! OBJECT POINTER was shifted as if it were a fixnum. Silent wrong value:
//! `(1+ 3.0)` came back as a number near 3.5e13. GNU and the interpreter both
//! say 4.0. Live from the float lowering (92e2dc18f) until this fix.

use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::decode::{decode_gnu_bytecode, parse_arglist_descriptor};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{Value, ValueKind};

/// The reproduced probe, byte for byte — GNU 31.1's `byte-compile` of
///
///     (defun probe-f (a n &optional _u)
///       (let ((x (* a 1.5)))
///         (dotimes (_ n) (setq x (+ x 1.0)))
///         (if (> n 100) (1+ x) x)))
///
/// decoded through the same `decode_gnu_bytecode` a loaded `.elc` takes, so
/// the block structure is exactly the one that miscompiled. `x` is produced
/// by a Float-feedback `*` (entry block) and a Float-feedback `+` on the
/// loop back-edge; the `1+` site never runs while n <= 100.
///
/// The `&optional` is LOAD-BEARING: it fails the MIR tier's gate
/// (`f.params.optional.is_empty()`, compile.rs) so the body is compiled by
/// the BASELINE, where the bug lives — the MIR tier is fixnum-only and would
/// deopt at the `*` on `2.0`, hiding the hole. Two hand-transcribed
/// simplifications of this probe (no loop; branch-target `1+`) PASSED
/// against the unfixed analysis. Do not simplify it. If that gate is ever
/// widened, force the baseline another way (a `VarSet` of a special
/// variable, which `lower_mir_pure` cannot lower).
fn probe() -> ByteCodeFunction {
    // `(aref (symbol-function 'probe-f) 1)` / `2` / `3` / `0` from GNU 31.1.
    let bytes: [u8; 31] = [
        2, 192, 95, 193, 137, 4, 87, 131, 21, 0, 194, 2, 195, 92, 178, 3, 136, 84, 130, 4, 0, 136,
        2, 196, 86, 131, 30, 0, 84, 135, 135,
    ];
    let mut constants = vec![
        Value::make_float(1.5),
        Value::make_int(0),
        Value::NIL,
        Value::make_float(1.0),
        Value::make_int(100),
    ];
    let ops = decode_gnu_bytecode(&bytes, &mut constants).expect("GNU bytecode decodes");
    let mut f = ByteCodeFunction::new(parse_arglist_descriptor(770));
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
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
fn a_float_crossing_a_block_edge_into_a_cold_add1_is_not_treated_as_a_fixnum() {
    let mut ev = Context::new();
    // Bind the probe to a symbol: that roots the bytecode object in the
    // obarray for the whole test (same as `jit_leaf_slot`'s `bind_fn`).
    let f = Value::make_bytecode(probe());
    let ValueKind::Symbol(id) = Value::symbol("known-fixnum-float-probe").kind() else {
        panic!("symbol")
    };
    ev.obarray.set_symbol_function_id(id, f);

    // Warm on the interpreter with a FLOAT `a` and n <= 100: the `Mul` site
    // records Float feedback; the `Add1` site never executes.
    for _ in 0..64 {
        assert_eq!(as_f64(call(&mut ev, f, Value::make_float(2.0), 3)), 6.0);
    }
    // Tier up: the next call compiles the baseline with that feedback.
    f.get_bytecode_data()
        .expect("bytecode")
        .jit_runtime()
        .set_hot_for_test();
    assert_eq!(as_f64(call(&mut ev, f, Value::make_float(2.0), 3)), 6.0);
    assert!(
        f.get_bytecode_data()
            .expect("bytecode")
            .jit_runtime()
            .compiled_id()
            .is_some(),
        "the probe must actually have been compiled, or this test is vacuous"
    );

    // Cold branch: `(1+ x)` on the boxed float, x = 3.0 + 200. Correct is
    // 204.0 — via the guard deopting to the interpreter. The bug returned the
    // float object's pointer bits shifted and retagged (~3.5e13).
    let cold = call(&mut ev, f, Value::make_float(2.0), 200);
    assert_eq!(
        as_f64(cold),
        204.0,
        "(1+ 203.0) must be 204.0; a value near 3.5e13 means a float pointer \
         was shifted as a fixnum (known-fixnum analysis ignoring Float feedback)"
    );
}

/// An OSR-entered float loop must be lowered WITH the body's numeric
/// feedback. `compile_osr_leaf` used to call `lower_leaf_full_osr` outside
/// the publish scope, so every Float site read FixnumOnly, the fixnum guards
/// failed on the first iteration, `OSR_TRIED_FLAG` blocked a retry, and the
/// loop stayed interpreted: a fixnum loop OSR'd 2.8x faster, the same loop
/// on floats gained nothing. The observable is the consumed flag the publish
/// sets — the result is identical either way (deopt reruns the interpreter).
///
///     (lambda (n) (let ((x 1.0) (acc 0.0)) (dotimes (_ n) (setq x (* x 1.0000001)) (setq acc (+ acc x))) acc))
/// GNU 31.1 byte-compile; constants [1.0 0.0 0 nil 1.0000001].
#[test]
fn an_osr_entered_float_loop_is_compiled_with_its_numeric_feedback() {
    // The arglist factory allocates conses as well as these float constants.
    // Construct them in the collecting Context's heap, not a fallback heap.
    let mut ev = Context::new();
    let roots = ev.save_specpdl_roots();
    let one = Value::make_float(1.0);
    ev.push_specpdl_root(one);
    let zero = Value::make_float(0.0);
    ev.push_specpdl_root(zero);
    let multiplier = Value::make_float(1.0000001);
    ev.push_specpdl_root(multiplier);
    let bytes: [u8; 27] = [
        192, 193, 194, 137, 4, 87, 131, 25, 0, 195, 3, 196, 95, 178, 4, 2, 4, 92, 178, 3, 136, 84,
        130, 3, 0, 136, 135,
    ];
    let mut constants = vec![one, zero, Value::make_int(0), Value::NIL, multiplier];
    let ops = decode_gnu_bytecode(&bytes, &mut constants).expect("GNU bytecode decodes");
    let mut f = ByteCodeFunction::new(parse_arglist_descriptor(257));
    ev.push_specpdl_root(f.arglist);
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(7);
    let f = Value::make_bytecode(f);
    let ValueKind::Symbol(id) = Value::symbol("osr-float-loop-probe").kind() else {
        panic!("symbol")
    };
    ev.obarray.set_symbol_function_id(id, f);
    ev.restore_specpdl_roots(roots);
    let bc = f.get_bytecode_data().expect("bytecode");
    assert!(
        bc.jit_runtime().wants_numeric_feedback(),
        "fresh body still records"
    );
    // One call, long enough for the back-edge poll to OSR the loop. The
    // interpreter records Float at the `*` and `+` sites on the way.
    let n = 400_000;
    let acc = ev
        .funcall_general_untraced(f, vec![Value::make_int(n)])
        .expect("call completes");
    // Reference: the same recurrence in Rust.
    let (mut x, mut expect) = (1.0_f64, 0.0_f64);
    for _ in 0..n {
        x *= 1.0000001;
        expect += x;
    }
    assert_eq!(as_f64(acc), expect);
    assert!(
        !bc.jit_runtime().wants_numeric_feedback(),
        "the OSR compile must have PUBLISHED (and thereby consumed) the body's feedback; \
         an OSR lowering outside the publish scope reads FixnumOnly everywhere"
    );
}

//! Builtin results go straight to the caller's slot (P0.2 C5): the stack
//! dispatcher returns the builtin's `EvalResult` in tail position, and every
//! consumer rebuilds `Ok(value)` from the value, sending only failures to a
//! cold helper. Each converted path must still answer exactly as GNU orders
//! it: the value on success; `wrong-number-of-arguments` on a bad arity;
//! on a signal, the signal hook runs while a TRACED call's frame is still on
//! the backtrace (`Bcall`, `funcall_subr`) and sees no frame at all for GNU's
//! frameless opcodes (the arithmetic opcodes, `Bplus` → `Fplus (2, &TOP)`,
//! and the inline ones, `Bchar_after`); and the specpdl is balanced after.

use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

/// Arm a `signal-hook-function` that records whether a backtrace frame for
/// TARGET (its symbol, or its subr object) is live when a signal is raised.
/// Special forms only: the bare `Context::new()` has no `when`, and a hook
/// that signals re-enters itself (GNU does not rebind it either).
fn install_frame_probe(eval: &mut Context, target: &str) {
    eval.eval_str(&format!("(setq brr-target '{target} brr-seen 'no-signal)"))
        .expect("probe target");
    eval.eval_str(
        "(setq signal-hook-function
           (lambda (_sym _data)
             (setq brr-seen 'no-frame)
             (mapbacktrace
               (lambda (_evald func _args _flags)
                 (if (or (eq func brr-target)
                         (and (subrp func)
                              (equal (subr-name func) (symbol-name brr-target))))
                     (setq brr-seen 'frame))))))",
    )
    .expect("install the frame probe");
}

/// What the probe saw: `frame`, `no-frame`, or `no-signal`.
fn probe(eval: &mut Context) -> String {
    let seen = eval.eval_str("brr-seen").expect("probe state");
    print_value(&seen)
}

/// Keep a Rust-local heap value alive across later evaluations: the
/// collector is precise and `NEOVM_GC_STRESS=1` collects at every safe point,
/// so an unrooted local's arena slot is reused by the next allocation.
fn rooted(v: Value) -> Value {
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

fn signal_name(result: &EvalResult) -> String {
    match result {
        Err(Flow::Signal(sig)) => crate::emacs_core::intern::resolve_sym(sig.symbol).to_string(),
        other => panic!("expected a signal, got {other:?}"),
    }
}

fn params(n: usize) -> LambdaParams {
    LambdaParams {
        required: (1..=n as u32)
            .map(crate::emacs_core::intern::SymId)
            .collect(),
        optional: Vec::new(),
        rest: None,
    }
}

/// `(lambda (a1..an) (CALLEE a1..an))` through `Op::Call` (GNU `Bcall`).
fn bcall_fn(callee: &str, nargs: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(params(nargs));
    f.lexical = true;
    let mut ops = vec![Op::Constant(0)];
    for _ in 0..nargs {
        ops.push(Op::StackRef(nargs as u16));
    }
    ops.push(Op::Call(nargs as u16));
    ops.push(Op::Return);
    f.ops = ops;
    f.constants = vec![Value::symbol(callee)].into();
    f.max_stack = 16;
    f
}

/// `(lambda (a [b]) (OP a [b]))` for an arithmetic opcode.
fn arith_fn(op: Op, nargs: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(params(nargs));
    f.lexical = true;
    f.ops = if nargs == 2 {
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return]
    } else {
        vec![Op::StackRef(0), op, Op::Return]
    };
    f.max_stack = 8;
    f
}

fn run(eval: &mut Context, f: &ByteCodeFunction, args: Vec<Value>) -> EvalResult {
    for &arg in &args {
        rooted(arg);
    }
    let before = eval.specpdl.len();
    let result = Vm::from_context(eval).execute(f, args);
    assert_eq!(
        eval.specpdl.len(),
        before,
        "the specpdl is balanced after the call"
    );
    result.map(rooted)
}

/// `Bcall` on a builtin (`call_resolved_builtin_from_stack_args`): fixed and
/// slice subrs, success, arity, and a signal the hook sees under the frame.
#[test]
fn bcall_builtin_results_arity_and_signal_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_frame_probe(&mut eval, "car");
    let car1 = bcall_fn("car", 1);
    let list = rooted(eval.eval_str("'(7 8)").unwrap());
    assert_eq!(run(&mut eval, &car1, vec![list]).unwrap(), Value::fixnum(7));
    assert_eq!(probe(&mut eval), "no-signal");

    let r = run(&mut eval, &car1, vec![Value::fixnum(1)]);
    assert_eq!(signal_name(&r), "wrong-type-argument");
    assert_eq!(probe(&mut eval), "frame", "the hook runs under car's frame");

    let car0 = bcall_fn("car", 0);
    let r = run(&mut eval, &car0, vec![]);
    assert_eq!(signal_name(&r), "wrong-number-of-arguments");

    // A slice subr past its fixnum fast values: `+` on a bignum and on a
    // non-number, through the traced Bcall (not the arithmetic opcode).
    install_frame_probe(&mut eval, "+");
    let plus2 = bcall_fn("+", 2);
    let big = rooted(eval.eval_str("(expt 2 80)").unwrap());
    let got = run(&mut eval, &plus2, vec![big, Value::fixnum(1)]).unwrap();
    assert_eq!(print_value(&got), "1208925819614629174706177");
    let r = run(&mut eval, &plus2, vec![big, Value::symbol("x")]);
    assert_eq!(signal_name(&r), "wrong-type-argument");
    assert_eq!(probe(&mut eval), "frame", "Bcall records a frame for +");
    eval.eval_str("(setq signal-hook-function nil)").unwrap();
}

/// GNU's inline opcodes (`call_inline_builtin_from_stack`) call the
/// primitive with no frame.
#[test]
fn inline_builtin_results_and_frameless_signal() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.eval_str("(insert \"abc\")").unwrap();
    install_frame_probe(&mut eval, "char-after");
    let mut f = ByteCodeFunction::new(params(1));
    f.lexical = true;
    f.ops = vec![
        Op::StackRef(0),
        Op::CallBuiltinSym(intern("char-after"), 1),
        Op::Return,
    ];
    f.max_stack = 4;
    assert_eq!(
        run(&mut eval, &f, vec![Value::fixnum(1)]).unwrap(),
        Value::fixnum(i64::from(b'a'))
    );
    let r = run(&mut eval, &f, vec![Value::string("x")]);
    assert_eq!(signal_name(&r), "wrong-type-argument");
    assert_eq!(
        probe(&mut eval),
        "no-frame",
        "an inline opcode records no frame"
    );
    eval.eval_str("(setq signal-hook-function nil)").unwrap();
}

/// The arithmetic opcodes' slow arm (`call_arith_builtin_on_context`):
/// exact bignum values, and a signal with NO frame for the builtin, as
/// GNU's `Bmult` calls `Ftimes` directly.
#[test]
fn arithmetic_opcode_slow_arm_results_and_frameless_signal() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_frame_probe(&mut eval, "*");
    let big = rooted(eval.eval_str("(expt 3 90)").unwrap());
    let mul = arith_fn(Op::Mul, 2);
    let got = run(&mut eval, &mul, vec![big, Value::fixnum(-7)]).unwrap();
    let want = rooted(eval.eval_str("(* (expt 3 90) -7)").unwrap());
    assert_eq!(print_value(&got), print_value(&want));
    let got = run(&mut eval, &mul, vec![big, big]).unwrap();
    let want = rooted(eval.eval_str("(expt 3 180)").unwrap());
    assert_eq!(print_value(&got), print_value(&want));
    let r = run(&mut eval, &mul, vec![Value::symbol("a"), Value::fixnum(2)]);
    assert_eq!(signal_name(&r), "wrong-type-argument");
    assert_eq!(probe(&mut eval), "no-frame", "Bmult records no frame for *");

    install_frame_probe(&mut eval, "1+");
    let add1 = arith_fn(Op::Add1, 1);
    let got = run(&mut eval, &add1, vec![big]).unwrap();
    let want = rooted(eval.eval_str("(1+ (expt 3 90))").unwrap());
    assert_eq!(print_value(&got), print_value(&want));
    let r = run(&mut eval, &add1, vec![Value::string("s")]);
    assert_eq!(signal_name(&r), "wrong-type-argument");
    assert_eq!(
        probe(&mut eval),
        "no-frame",
        "Badd1 records no frame for 1+"
    );
    eval.eval_str("(setq signal-hook-function nil)").unwrap();
}

/// `call_spec_subr_stack` (the framed stack route of a speculated subr):
/// success, arity, and a signal the hook sees under the frame.
#[test]
fn spec_subr_stack_results_arity_and_signal_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_frame_probe(&mut eval, "car");
    let car = intern("car");
    let subr = Value::subr_from_sym_id(car);
    let mut call = |eval: &mut Context, args: &[Value]| -> EvalResult {
        let before = eval.specpdl.len();
        let args_start = eval.bc_buf.len();
        eval.bc_buf.extend_from_slice(args);
        let result = Vm::from_context(eval)
            .call_spec_subr_stack(car, subr, args_start, args.len())
            .map(rooted);
        eval.bc_buf.truncate(args_start);
        assert_eq!(eval.specpdl.len(), before, "balanced specpdl");
        result
    };
    let list = rooted(eval.eval_str("'(5 6)").unwrap());
    assert_eq!(call(&mut eval, &[list]).unwrap(), Value::fixnum(5));
    let r = call(&mut eval, &[Value::fixnum(3)]);
    assert_eq!(signal_name(&r), "wrong-type-argument");
    assert_eq!(probe(&mut eval), "frame");
    let r = call(&mut eval, &[list, list]);
    assert_eq!(signal_name(&r), "wrong-number-of-arguments");
    eval.eval_str("(setq signal-hook-function nil)").unwrap();
}

/// The JIT's armed stack route for a Many subr
/// (`Context::call_spec_subr_from_bc_stack`), against the interpreter:
/// success, and a signal the hook sees under the frame.
#[cfg(feature = "jit")]
#[test]
fn jit_spec_subr_from_bc_stack_results_and_signal_frame() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut eval = Context::new();
    eval.eval_str("(insert \"hello world\")").unwrap();
    install_frame_probe(&mut eval, "re-search-forward");
    let make = |hot: bool| {
        let f = bcall_fn("re-search-forward", 1);
        if hot {
            f.jit_runtime().set_hot_for_test();
        }
        let v = Value::make_bytecode(f);
        crate::emacs_core::eval::push_scratch_gc_root(v);
        v
    };
    let hot = make(true);
    let cold = make(false);
    #[cfg(debug_assertions)]
    let spec0 =
        crate::emacs_core::jit::compile::SUBR_SPEC_COUNT.load(std::sync::atomic::Ordering::Relaxed);
    for f in [hot, cold] {
        eval.eval_str("(goto-char (point-min))").unwrap();
        let before = eval.specpdl.len();
        let r = eval
            .funcall_general_untraced(f, vec![rooted(Value::string("wor"))])
            .expect("found");
        assert_eq!(r, Value::fixnum(10));
        assert_eq!(eval.specpdl.len(), before);
        eval.eval_str("(goto-char (point-min))").unwrap();
        let r = eval.funcall_general_untraced(f, vec![rooted(Value::string("zzz-none"))]);
        let r: EvalResult = r;
        assert_eq!(signal_name(&r), "search-failed");
        assert_eq!(probe(&mut eval), "frame", "the hook runs under the frame");
        assert_eq!(eval.specpdl.len(), before);
    }
    #[cfg(debug_assertions)]
    assert!(
        crate::emacs_core::jit::compile::SUBR_SPEC_COUNT.load(std::sync::atomic::Ordering::Relaxed)
            > spec0,
        "the hot caller must run through the subr spec shim",
    );
    eval.eval_str("(setq signal-hook-function nil)").unwrap();
}

/// `neovm_jit_arith_generic`: a compiled arithmetic site whose feedback says
/// the fixnum path misses answers the interpreter's value natively, and a
/// signal from it records no frame (GNU `Bmult`).
#[cfg(feature = "jit")]
#[test]
fn jit_arith_generic_results_and_frameless_signal() {
    use crate::emacs_core::jit::NumericFeedback;
    use crate::emacs_core::jit::compile::{
        NativeRun, compile_bytecode_function, take_pending_flow,
    };
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    install_frame_probe(&mut eval, "*");
    let f = arith_fn(Op::Mul, 2);
    f.jit_runtime()
        .record_numeric(2, f.ops.len(), NumericFeedback::Other);
    let leaf = compile_bytecode_function(&f).expect("compiles");
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let big = rooted(eval.eval_str("(expt 7 60)").unwrap());
    crate::emacs_core::eval::push_scratch_gc_root(big);
    let want = rooted(eval.eval_str("(* (expt 7 60) 12)").unwrap());
    match leaf.call(ctx_ptr, &[big, Value::fixnum(12)]) {
        NativeRun::Ok(bits) => {
            assert_eq!(print_value(&Value::from_bits(bits)), print_value(&want))
        }
        other => panic!("the generic site must answer natively: {other:?}"),
    }
    match leaf.call(ctx_ptr, &[Value::symbol("a"), Value::fixnum(2)]) {
        NativeRun::Signal => {
            let flow = take_pending_flow().expect("flow stashed");
            assert_eq!(signal_name(&Err(flow)), "wrong-type-argument");
        }
        other => panic!("expected a signal: {other:?}"),
    }
    assert_eq!(
        probe(&mut eval),
        "no-frame",
        "the generic site records no frame"
    );
    eval.eval_str("(setq signal-hook-function nil)").unwrap();
}

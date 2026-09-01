//! Repro / diagnosis harness for the reported pre-existing JIT heap-corruption
//! crash (task `task3-jitcrash-diag`). DIAGNOSTIC only — NO fix here.
//!
//! Hypothesis under test: a live heap `Value` in a force-tiered compiled body is
//! not on a GC root across an allocating callee, so a GC during the callee's
//! allocation frees a still-live value -> use-after-free -> heap corruption
//! (the reported "malloc_printerr freeing a corrupted bignum Vec" /
//! "list_from_slice SIGSEGV").
//!
//! CRITICAL METHODOLOGY NOTE (the confound the task warned about):
//! `NEOVM_GC_STRESS=1` on a bare `Context::new()` collects at EVERY allocation
//! safe point. A bare Context is NOT a warmed pdump image, so any heap `Value`
//! held only in a *Rust local* (e.g. a COLD `ByteCodeFunction` the test builds
//! but never binds/roots, or a comparison `expected` bignum) is NOT a GC root
//! and gets swept mid-test. Those crashes reproduce with the JIT DISABLED
//! (`NEOVM_JIT=0`) too — they are bare-context artifacts, NOT the JIT bug. The
//! tests below are written to be ARTIFACT-FREE: the caller is rooted by
//! `funcall_general_untraced` (eval.rs push_scratch_gc_root before
//! try_run_compiled), callees are bound into the obarray (rooted), args are
//! immediates/symbols or built IN the body (rooted by the JIT residual
//! machinery), and no heap `Value` is held in a Rust local across a GC. A crash
//! in one of THESE is a genuine JIT rooting gap.
#![cfg(feature = "jit")]

use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::{LambdaParams, Value, ValueKind};

fn bc(required: u32, ops: Vec<Op>, constants: Vec<Value>, hot: bool) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=required).map(SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 64;
    if hot {
        f.jit_runtime().set_hot_for_test();
    }
    f
}

/// Bind `name`'s function cell to `bytecode` (roots it in the obarray) and
/// return the callable symbol Value.
fn bind_fn(ev: &mut Context, name: &str, bytecode: ByteCodeFunction) -> Value {
    let sym = Value::symbol(name);
    let ValueKind::Symbol(id) = sym.kind() else {
        panic!("symbol")
    };
    ev.obarray
        .set_symbol_function_id(id, Value::make_bytecode(bytecode));
    sym
}

/// Confirm a value is an intact heap cons `(1 . 2)` (reads the cons — a swept
/// cons would fault or mismatch).
fn assert_cons_1_2(r: Value) {
    assert!(r.is_cons(), "residual cons must survive (got {r:?})");
    assert_eq!(r.cons_car(), Value::make_int(1), "car intact");
    assert_eq!(r.cons_cdr(), Value::make_int(2), "cdr intact");
}

/// BASELINE, generic path: a fresh heap cons `H=(cons 1 2)` in the residual
/// operand stack across a generic (variable-callee) allocating call
/// `(funcall mkl 200 0)`. Passing = baseline residual rooting is correct.
///
/// (lambda (mkl) (let ((H (cons 1 2))) (funcall mkl 200 0) H))
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_baseline_residual_cons_generic() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    let caller = Value::make_bytecode(bc(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::StackRef(1),
            Op::Constant(2),
            Op::Constant(3),
            Op::Call(2),
            Op::Pop,
            Op::Return,
        ],
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::make_int(200),
            Value::make_int(0),
        ],
        true,
    ));
    let make_list = Value::symbol("make-list");
    for _ in 0..80 {
        let r = ev
            .funcall_general_untraced(caller, vec![make_list])
            .expect("baseline residual-cons generic call");
        assert_cons_1_2(r);
    }
}

/// BASELINE, generic path, HEAP ARGUMENT: a fresh heap cons is passed AS AN
/// ARGUMENT to a generic allocating call. Tests the window where args live in
/// the non-GC-traced `call_args_slot` before the shim copies them onto bc_buf.
///
/// (lambda (mkl) (let ((H (cons 1 2))) (funcall mkl 40 (cons 7 8)) H))
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_baseline_heap_arg_generic() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    let caller = Value::make_bytecode(bc(
        1,
        vec![
            Op::Constant(0), // 1
            Op::Constant(1), // 2
            Op::Cons,        // H=(cons 1 2)  residual
            Op::StackRef(1), // mkl
            Op::Constant(2), // 40
            Op::Constant(3), // 7
            Op::Constant(4), // 8
            Op::Cons,        // (cons 7 8) fresh heap ARG
            Op::Call(2),     // (funcall mkl 40 (cons 7 8))
            Op::Pop,
            Op::Return, // -> H
        ],
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::make_int(40),
            Value::make_int(7),
            Value::make_int(8),
        ],
        true,
    ));
    let make_list = Value::symbol("make-list");
    for _ in 0..80 {
        let r = ev
            .funcall_general_untraced(caller, vec![make_list])
            .expect("baseline heap-arg generic call");
        assert_cons_1_2(r);
    }
}

/// BASELINE, `Op::List` (the interpreter's `Value::list_from_slice` shim,
/// `neovm_jit_list`): build a list from operand-stack slots with a fresh heap
/// cons in the residual. Directly exercises the reported "list_from_slice" path.
///
/// (lambda (x) (let ((H (cons 1 2))) (list 1 2 3) H))
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_baseline_op_list_residual() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    let caller = Value::make_bytecode(bc(
        1,
        vec![
            Op::Constant(0), // 1
            Op::Constant(1), // 2
            Op::Cons,        // H
            Op::Constant(0), // 1
            Op::Constant(1), // 2
            Op::Constant(2), // 3
            Op::List(3),     // (list 1 2 3), residual = [x H]
            Op::Pop,
            Op::Return, // -> H
        ],
        vec![Value::make_int(1), Value::make_int(2), Value::make_int(3)],
        true,
    ));
    for _ in 0..80 {
        let r = ev
            .funcall_general_untraced(caller, vec![Value::make_int(9)])
            .expect("baseline Op::List residual call");
        assert_cons_1_2(r);
    }
}

/// MIR TIER, generic residual call: the caller INLINES a pure bound callee
/// `(jitinc x) = (1+ x)` (forcing the typed-MIR tier) AND holds a fresh heap
/// cons across a generic (variable-callee) allocating call. Exercises the MIR
/// tier's `pre_stack` residual rooting (compile.rs build_mir_leaf_fn Opaque
/// Call), the one call-lowering path distinct from the baseline.
///
/// (lambda (mkl) (let ((H (cons (jitinc 41) 2))) (funcall mkl 200 0) H))
/// with H = (cons 42 2).
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_mir_inlined_plus_residual_generic() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    // Pure, required-only, inlinable callee: (lambda (x) (1+ x)).
    bind_fn(
        &mut ev,
        "jitinc",
        bc(
            1,
            vec![Op::StackRef(0), Op::Add1, Op::Return],
            vec![],
            false,
        ),
    );
    let caller = Value::make_bytecode(bc(
        1,
        vec![
            Op::Constant(0), // 'jitinc
            Op::Constant(1), // 41
            Op::Call(1),     // (jitinc 41) -> 42  [inlined -> MIR tier]
            Op::Constant(2), // 2
            Op::Cons,        // H=(cons 42 2)  residual
            Op::StackRef(1), // mkl
            Op::Constant(3), // 200
            Op::Constant(4), // 0
            Op::Call(2),     // (funcall mkl 200 0)  residual=[mkl H]
            Op::Pop,
            Op::Return, // -> H
        ],
        vec![
            Value::symbol("jitinc"),
            Value::make_int(41),
            Value::make_int(2),
            Value::make_int(200),
            Value::make_int(0),
        ],
        true,
    ));
    let make_list = Value::symbol("make-list");
    for _ in 0..80 {
        let r = ev
            .funcall_general_untraced(caller, vec![make_list])
            .expect("MIR inlined+residual generic call");
        assert!(r.is_cons(), "MIR residual cons survives (got {r:?})");
        assert_eq!(r.cons_car(), Value::make_int(42), "inlined car intact");
        assert_eq!(r.cons_cdr(), Value::make_int(2), "cdr intact");
    }
}

/// MIR TIER, `Any`-TYPED heap residual — the lever-1 runtime-inline path. Unlike
/// every residual above (a fresh `Op::Cons` / bignum, whose MIR type is provably
/// heap, so the codegen emits an UNCONDITIONAL `neovm_jit_gc_push`), the residual
/// here is the RESULT of a generic `(make-list 1 7)` call, whose MIR type is
/// `Any`. For `Any`/`Unknown` residuals the codegen inlines the `is_heap_object`
/// tag test (`emit_conditional_gc_push`) so a fixnum/symbol operand skips the
/// shim call at run time — but a HEAP `Any` value like `(7)` MUST still be pushed.
/// An inverted/wrong tag test would skip rooting the live cons; the `gcchurn`
/// callee then `garbage-collect`s it (exact GC, no native-stack scan -> the
/// unrooted cons is definitely swept) and reallocates 4096 conses that pop its
/// freed slot, so the returned car reads back `0` (verified: the inverted branch
/// fails here with left=0 right=7) instead of `7`.
///
/// The body inlines `jitinc` (a pure single-block callee) so the tier gate routes
/// the call-bearing body to the MIR tier, then keeps the `Any` cons live across
/// the `gcchurn` call.
///
/// (defun f () (jitinc 41) (let ((X (make-list 1 7))) (gcchurn) X))
/// (defun gcchurn () (garbage-collect) (make-list 4096 0))
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_mir_any_typed_heap_residual_generic() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    // Pure, required-only, inlinable callee: (lambda (x) (1+ x)) — earns MIR tier.
    bind_fn(
        &mut ev,
        "jitinc",
        bc(
            1,
            vec![Op::StackRef(0), Op::Add1, Op::Return],
            vec![],
            false,
        ),
    );
    // Callee that DETERMINISTICALLY collects (`garbage-collect` -> gc_collect_exact,
    // which ignores the native stack, so an unrooted residual is definitely swept)
    // and then reallocates 4096 conses, popping the freed cons off the free list so
    // a dropped root reads corrupted contents. Bound (not hot) so it stays
    // interpreted and both builtins run for real.
    bind_fn(
        &mut ev,
        "gcchurn",
        bc(
            0,
            vec![
                Op::Constant(0), // 'garbage-collect
                Op::Call(0),     // (garbage-collect) -> gc_collect_exact
                Op::Pop,
                Op::Constant(1), // 'make-list
                Op::Constant(2), // 4096
                Op::Constant(3), // 0
                Op::Call(2),     // (make-list 4096 0)  reclaims + reuses freed slots
                Op::Return,
            ],
            vec![
                Value::symbol("garbage-collect"),
                Value::symbol("make-list"),
                Value::make_int(4096),
                Value::make_int(0),
            ],
            false,
        ),
    );
    let caller = Value::make_bytecode(bc(
        0,
        vec![
            // --- inline a call to earn the MIR tier ---
            Op::Constant(0), // 'jitinc
            Op::Constant(1), // 41
            Op::Call(1),     // (jitinc 41) -> 42 [inlined]
            Op::Pop,
            // --- X = (make-list 1 7) = (7): a generic-call result, MIR type Any ---
            Op::Constant(2), // 'make-list
            Op::Constant(3), // 1
            Op::Constant(4), // 7
            Op::Call(2),     // X = (7)   [Any-typed heap residual]
            // --- residual X live across a call that GC-collects then reallocates ---
            Op::Constant(5), // 'gcchurn
            Op::Call(0),     // (gcchurn)  residual = [X] -> conditional gc_push
            Op::Pop,
            Op::Return, // -> X
        ],
        vec![
            Value::symbol("jitinc"),
            Value::make_int(41),
            Value::symbol("make-list"),
            Value::make_int(1),
            Value::make_int(7),
            Value::symbol("gcchurn"),
        ],
        true,
    ));
    for _ in 0..20 {
        let r = ev
            .funcall_general_untraced(caller, vec![])
            .expect("MIR Any-typed heap residual generic call");
        assert!(
            r.is_cons(),
            "Any-typed residual cons must survive the collect (got {r:?})"
        );
        assert_eq!(
            r.cons_car(),
            Value::make_int(7),
            "car intact — a dropped root would read the reused slot's contents"
        );
        assert_eq!(r.cons_cdr(), Value::NIL, "cdr intact (single-element list)");
    }
}

/// BASELINE TIER, lever-1 residual rooting (the deterministic counterpart to the
/// weak `make-list`-churn repros above, which never actually collect). A fresh
/// heap cons `H=(cons 1 2)` is held across a call that DETERMINISTICALLY collects
/// (`gcchurn2` = `(garbage-collect) (make-list 4096 0)`). The body inlines
/// NOTHING, so the call-bearing body takes the BASELINE tier (asserted via
/// `inline_epoch().is_none()`), exercising `emit_residual_roots` /
/// `emit_conditional_gc_push` in `lower_op`'s `Op::Call` lowering. `H` is an
/// `Op::Cons` SSA value (not a nonheap const), so it flows through the inlined
/// `is_heap_object` tag test and MUST be pushed; an inverted test frees it and the
/// churn's `make-list` reuses its slot -> car reads `0` not `1` (verified
/// adversarially).
///
/// (defun f () (let ((H (cons 1 2))) (gcchurn2) H))
/// (defun gcchurn2 () (garbage-collect) (make-list 4096 0))
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_baseline_heap_residual_survives_exact_gc() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    bind_fn(
        &mut ev,
        "gcchurn2",
        bc(
            0,
            vec![
                Op::Constant(0), // 'garbage-collect
                Op::Call(0),     // -> gc_collect_exact
                Op::Pop,
                Op::Constant(1), // 'make-list
                Op::Constant(2), // 4096
                Op::Constant(3), // 0
                Op::Call(2),     // reclaims + reuses freed slots
                Op::Return,
            ],
            vec![
                Value::symbol("garbage-collect"),
                Value::symbol("make-list"),
                Value::make_int(4096),
                Value::make_int(0),
            ],
            false,
        ),
    );
    let cbc = bc(
        0,
        vec![
            Op::Constant(0), // 1
            Op::Constant(1), // 2
            Op::Cons,        // H=(cons 1 2)  heap residual
            Op::Constant(2), // 'gcchurn2
            Op::Call(0),     // (gcchurn2)  residual=[H] -> emit_residual_roots
            Op::Pop,
            Op::Return, // -> H
        ],
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::symbol("gcchurn2"),
        ],
        true,
    );
    // The call-bearing body inlines nothing -> BASELINE tier (not MIR): this test
    // exercises lower_op's residual rooting, distinct from the MIR path above.
    {
        let leaf = crate::emacs_core::jit::compile::compile_bytecode_function_with(
            &cbc,
            Some(&ev.obarray),
        )
        .expect("caller compiles");
        assert!(
            leaf.inline_epoch().is_none(),
            "no inlinable callee -> baseline tier, exercising lower_op residual rooting"
        );
    }
    let caller = Value::make_bytecode(cbc);
    for _ in 0..20 {
        let r = ev
            .funcall_general_untraced(caller, vec![])
            .expect("baseline heap residual across exact-GC call");
        assert_cons_1_2(r);
    }
}

/// BASELINE, generic path, BIGNUM (Vec-backed) residual — the reported
/// "corrupted bignum Vec" shape. `B=(* BIG BIG)` (fixnum overflow -> heap bignum
/// with an internal digit Vec) is held across a generic allocating call, then
/// returned and its Vec is READ (print) to detect corruption. `B` is rooted
/// before the read so a swept-B artifact cannot masquerade as the JIT bug.
///
/// (lambda (mul mkl) (let ((B (funcall mul BIG BIG))) (funcall mkl 200 0) B))
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_baseline_bignum_residual_generic() {
    use crate::emacs_core::eval::{
        push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
    };
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    // Expected print string (a Rust String, not a heap Value — artifact-free).
    let expected_str = {
        let b = ev
            .eval_str("(* most-positive-fixnum most-positive-fixnum)")
            .unwrap();
        crate::emacs_core::print::print_value(&b)
    };
    let big = Value::make_int(Value::MOST_POSITIVE_FIXNUM);
    let caller = Value::make_bytecode(bc(
        2,
        vec![
            Op::StackRef(1), // mul
            Op::Constant(0), // BIG
            Op::Constant(0), // BIG
            Op::Call(2),     // B=(* BIG BIG)  bignum residual
            Op::StackRef(1), // mkl
            Op::Constant(1), // 200
            Op::Constant(2), // 0
            Op::Call(2),     // (funcall mkl 200 0)
            Op::Pop,
            Op::Return, // -> B
        ],
        vec![big, Value::make_int(200), Value::make_int(0)],
        true,
    ));
    let times = Value::symbol("*");
    let make_list = Value::symbol("make-list");
    for _ in 0..80 {
        let r = ev
            .funcall_general_untraced(caller, vec![times, make_list])
            .expect("baseline bignum-residual generic call");
        // Root r before reading its Vec, so any crash is JIT corruption, not a
        // swept-r artifact.
        let saved = save_scratch_gc_roots();
        push_scratch_gc_root(r);
        assert!(
            r.is_integer() && !r.is_fixnum(),
            "B must stay a bignum (got {r:?})"
        );
        assert_eq!(
            crate::emacs_core::print::print_value(&r),
            expected_str,
            "bignum residual Vec intact across the allocating call"
        );
        restore_scratch_gc_roots(saved);
    }
}

/// JIT -> JIT (nested compiled re-entry), generic path: the caller holds a heap
/// cons across a generic call to a HOT bytecode callee that itself allocates
/// (`(jitalloc n) = (make-list n 7)`). Exercises the nested compiled-cache
/// re-entry + allocation with a live caller residual.
///
/// (lambda (f) (let ((H (cons 1 2))) (funcall f 200) H))  ; f = jitalloc
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_jit_to_jit_residual_generic() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    // Hot callee that allocates: (lambda (n) (make-list n 7)).
    bind_fn(
        &mut ev,
        "jitalloc",
        bc(
            1,
            vec![
                Op::Constant(0), // 'make-list
                Op::StackRef(1), // n
                Op::Constant(1), // 7
                Op::Call(2),
                Op::Return,
            ],
            vec![Value::symbol("make-list"), Value::make_int(7)],
            true,
        ),
    );
    let caller = Value::make_bytecode(bc(
        1,
        vec![
            Op::Constant(0), // 1
            Op::Constant(1), // 2
            Op::Cons,        // H
            Op::StackRef(1), // f
            Op::Constant(2), // 200
            Op::Call(1),     // (funcall f 200)  residual=[f H]
            Op::Pop,
            Op::Return, // -> H
        ],
        vec![Value::make_int(1), Value::make_int(2), Value::make_int(200)],
        true,
    ));
    let f = Value::symbol("jitalloc");
    for _ in 0..80 {
        let r = ev
            .funcall_general_untraced(caller, vec![f])
            .expect("JIT->JIT residual generic call");
        assert_cons_1_2(r);
    }
}

/// BASELINE, LOOP-CARRIED heap value across an in-loop allocating call: a fresh
/// heap accumulator built once, carried across the loop backedge (Cranelift
/// var-spill / `neovm_jit_backedge`), and live across an allocating call INSIDE
/// the loop body. Exercises a rooting mechanism distinct from a straight-line
/// residual.
///
/// (lambda (n mkl) (let ((acc (cons 1 2))) (while (> n 0) (funcall mkl 40 0)
///                                            (setq n (1- n))) acc))
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_loop_carried_cons_across_call() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    let caller = Value::make_bytecode(bc(
        2,
        vec![
            Op::Constant(0),   // 1        0
            Op::Constant(1),   // 2        1
            Op::Cons,          // acc      2
            Op::StackRef(2),   // n        3  <- loop head
            Op::Constant(2),   // 0        4
            Op::Gtr,           // n>0      5
            Op::GotoIfNil(16), //        6  -> exit
            Op::StackRef(1),   // mkl      7
            Op::Constant(3),   // 40       8
            Op::Constant(2),   // 0        9
            Op::Call(2),       // call     10  residual=[n mkl acc]
            Op::Pop,           //          11
            Op::StackRef(2),   // n        12
            Op::Sub1,          // n-1      13
            Op::StackSet(3),   // n=n-1    14
            Op::Goto(3),       // backedge 15
            Op::StackRef(0),   // acc      16 <- exit
            Op::Return,        //          17 -> acc
        ],
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::make_int(0),
            Value::make_int(40),
        ],
        true,
    ));
    let make_list = Value::symbol("make-list");
    for _ in 0..40 {
        let r = ev
            .funcall_general_untraced(caller, vec![Value::make_int(6), make_list])
            .expect("loop-carried cons across in-loop call");
        assert_cons_1_2(r);
    }
}

/// BASELINE, 3-DEEP residual: three fresh heap conses live across a generic
/// allocating call (the residual gc_push loop must root every one). Returns the
/// top; a dropped deeper residual would corrupt the heap that the others share.
///
/// (lambda (mkl) (let ((h1 (cons 1 2)) (h2 (cons 3 4)) (h3 (cons 5 6)))
///                 (funcall mkl 200 0) h3))
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn repro_deep_residual_three_conses() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    let caller = Value::make_bytecode(bc(
        1,
        vec![
            Op::Constant(0), // 1
            Op::Constant(1), // 2
            Op::Cons,        // h1
            Op::Constant(2), // 3
            Op::Constant(3), // 4
            Op::Cons,        // h2
            Op::Constant(4), // 5
            Op::Constant(5), // 6
            Op::Cons,        // h3
            Op::StackRef(3), // mkl
            Op::Constant(6), // 200
            Op::Constant(7), // 0
            Op::Call(2),     // residual = [mkl h1 h2 h3]
            Op::Pop,
            Op::Return, // -> h3
        ],
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::make_int(3),
            Value::make_int(4),
            Value::make_int(5),
            Value::make_int(6),
            Value::make_int(200),
            Value::make_int(0),
        ],
        true,
    ));
    let make_list = Value::symbol("make-list");
    for _ in 0..80 {
        let r = ev
            .funcall_general_untraced(caller, vec![make_list])
            .expect("3-deep residual across call");
        assert!(
            r.is_cons() && r.cons_car() == Value::make_int(5),
            "h3 intact (got {r:?})"
        );
    }
}

/// Compile-outcome diagnostic: confirms each repro body actually reaches the JIT
/// (and which tier), so a "pass" means the JIT ran — not that the body silently
/// stayed on the interpreter.
#[test]
#[ignore = "diagnostic JIT-rooting repro (gc_stress); run with --run-ignored"]
fn diag_repro_bodies_compile() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    bind_fn(
        &mut ev,
        "jitinc",
        bc(
            1,
            vec![Op::StackRef(0), Op::Add1, Op::Return],
            vec![],
            false,
        ),
    );
    let cases: Vec<(&str, ByteCodeFunction)> = vec![
        (
            "baseline residual cons (generic)",
            bc(
                1,
                vec![
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Cons,
                    Op::StackRef(1),
                    Op::Constant(2),
                    Op::Constant(3),
                    Op::Call(2),
                    Op::Pop,
                    Op::Return,
                ],
                vec![
                    Value::make_int(1),
                    Value::make_int(2),
                    Value::make_int(200),
                    Value::make_int(0),
                ],
                false,
            ),
        ),
        (
            "Op::List residual",
            bc(
                1,
                vec![
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Cons,
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Constant(2),
                    Op::List(3),
                    Op::Pop,
                    Op::Return,
                ],
                vec![Value::make_int(1), Value::make_int(2), Value::make_int(3)],
                false,
            ),
        ),
        (
            "MIR inlined + residual (generic)",
            bc(
                1,
                vec![
                    Op::Constant(0),
                    Op::Constant(1),
                    Op::Call(1),
                    Op::Constant(2),
                    Op::Cons,
                    Op::StackRef(1),
                    Op::Constant(3),
                    Op::Constant(4),
                    Op::Call(2),
                    Op::Pop,
                    Op::Return,
                ],
                vec![
                    Value::symbol("jitinc"),
                    Value::make_int(41),
                    Value::make_int(2),
                    Value::make_int(200),
                    Value::make_int(0),
                ],
                false,
            ),
        ),
    ];
    for (name, f) in cases {
        match crate::emacs_core::jit::compile::compile_bytecode_function_with(&f, Some(&ev.obarray))
        {
            Ok(_) => eprintln!("COMPILE OK   [{name}]"),
            Err(e) => eprintln!("COMPILE ERR  [{name}]: {e:?}"),
        }
    }
}

/// DEFINITIVE warm-context test of the REPORTED shape: `parse-partial-sexp` via
/// the GENERIC JIT call path, with a heap-cons residual, under `gc_stress`, but
/// in a WARM (`runtime_startup_context`) evaluator — the fuller bootstrap the
/// bare `Context::new()` lacks. Run this both ways:
///   * `NEOVM_JIT_THRESHOLD=1 NEOVM_GC_STRESS=1` (JIT on)  -> expect PASS
///   * `NEOVM_JIT=0 NEOVM_GC_STRESS=1`            (JIT off) -> expect PASS
/// If the JIT-on run crashed while JIT-off passed, that would be the real JIT
/// bug. (Ignored by default: `runtime_startup_context` is heavy and this is a
/// manual diagnosis probe.)
#[test]
#[ignore = "manual diagnosis probe: run under NEOVM_JIT_THRESHOLD=1 NEOVM_GC_STRESS=1 vs NEOVM_JIT=0 NEOVM_GC_STRESS=1"]
fn repro_warmctx_parse_partial_sexp_generic() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = crate::test_utils::runtime_startup_context();
    // gc_stress only AROUND the parse-partial-sexp funcall (the JIT call path
    // under test); the warm heap is huge so a full collection per allocation is
    // slow — keep setup/goto out of the stressed window.
    ev.gc_stress = false;
    ev.eval_str("(insert \"(foo (bar baz) (qux (a b c) d) e)\")")
        .expect("buffer");
    // caller (rooted by funcall during each call):
    //   (lambda (ppss) (let ((H (cons 1 2))) (funcall ppss 1 20) H))
    let caller = Value::make_bytecode(bc(
        1,
        vec![
            Op::Constant(0), // 1
            Op::Constant(1), // 2
            Op::Cons,        // H
            Op::StackRef(1), // ppss
            Op::Constant(2), // 1 (from)
            Op::Constant(3), // 20 (to)
            Op::Call(2),     // (funcall ppss 1 20)  residual=[ppss H]
            Op::Pop,
            Op::Return, // -> H
        ],
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::make_int(1),
            Value::make_int(20),
        ],
        true,
    ));
    let ppss = Value::symbol("parse-partial-sexp");
    for _ in 0..6 {
        ev.eval_str("(goto-char (point-min))").unwrap();
        ev.gc_stress = true; // stress GC only across the JIT call
        let r = ev
            .funcall_general_untraced(caller, vec![ppss])
            .expect("warm-context parse-partial-sexp generic call");
        ev.gc_stress = false;
        assert_cons_1_2(r);
    }
}

/// RED HERRING (documented, ignored): `parse-partial-sexp` over a bare
/// `Context::new()` under `gc_stress` SEGFAULTS *with the JIT DISABLED too*
/// (`NEOVM_JIT=0`) — it is a bare-context runtime rooting gap (missing syntax
/// state / cold bootstrap), NOT the JIT bug. Kept only to document the confound.
#[test]
#[ignore = "bare-context gc_stress artifact: crashes with NEOVM_JIT=0 too; NOT the JIT bug"]
fn repro_redherring_parse_partial_sexp_bare_context() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut ev = Context::new();
    ev.gc_stress = true;
    ev.eval_str("(insert \"(foo (bar baz) (qux (a b c) d) e)\")")
        .unwrap();
    let caller = Value::make_bytecode(bc(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::StackRef(1),
            Op::Constant(2),
            Op::Constant(3),
            Op::Call(2),
            Op::Pop,
            Op::Return,
        ],
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::make_int(1),
            Value::make_int(20),
        ],
        true,
    ));
    let ppss = Value::symbol("parse-partial-sexp");
    for _ in 0..100 {
        ev.eval_str("(goto-char (point-min))").unwrap();
        let _ = ev.funcall_general_untraced(caller, vec![ppss]);
    }
}

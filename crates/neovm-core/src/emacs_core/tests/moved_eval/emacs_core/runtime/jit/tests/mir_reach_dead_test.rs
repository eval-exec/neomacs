//! The `dead` MIR reach bit (`NEOVM_JIT_MIR_REACH=dead`, design P2.5 R2): a
//! body whose only MIR blocker is an unreachable leader -- the `Return`
//! `seal_ops` appends after a named-let's final back-edge `goto` -- tiers to
//! MIR with the bit, and stays exactly where it was without it.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::{
    CompileError, CompiledLeaf, NativeRun, compile_bytecode_function_with,
};
use crate::emacs_core::jit::mir;
use neovm_eval::test_support::authority::{
    TestMirReach, TestOptMode, compile_stats_mir_census_for_test,
    executable_gnu_byte_offset_map_for_test, force_deopt_for_test, force_mir_reach_token_for_test,
    force_observe_knobs_for_test, mir_reach_parse_for_test, opt_mode_token_scope_for_test,
    reset_compile_stats_for_test, seal_hand_assembled_ops_for_test,
};
use neovm_value::intern::SymId;
use neovm_value::value::LambdaParams;
use neovm_value::value::Value;

const DEAD: TestMirReach = TestMirReach::ALL;

fn observe_stats() {
    force_observe_knobs_for_test(true, false, false);
}

/// `(lambda (n) (named-let lp ((n n) (acc 0)) (if (> n 0) (lp (1- n) (1+ acc)) acc)))`
/// in the byte compiler's named-let shape; sealing appends the unreachable
/// trailing `Return` (index 14).
fn named_let() -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        // SAFETY: test placeholder id, never resolved (peer seal N8).
        required: vec![unsafe { SymId::from_raw(1) }],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(0),     // 0: acc = 0          [n acc]
        Op::StackRef(1),     // 1: header: n        [n acc n]
        Op::Constant(0),     // 2
        Op::Gtr,             // 3: (> n 0)          [n acc b]
        Op::GotoIfNotNil(7), // 4                   [n acc]
        Op::StackRef(0),     // 5: acc
        Op::Return,          // 6
        Op::StackRef(1),     // 7: n
        Op::Sub1,            // 8
        Op::StackSet(2),     // 9: n = n-1          [n acc]
        Op::StackRef(0),     // 10: acc
        Op::Add1,            // 11
        Op::StackSet(1),     // 12: acc = acc+1     [n acc]
        Op::Goto(1),         // 13: back edge
    ];
    f.constants = vec![Value::make_int(0)].into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
    seal_hand_assembled_ops_for_test(&mut f);
    assert_eq!(f.ops.last(), Some(&Op::Return), "sealed: trailing Return");
    f
}

/// Run `leaf` on `arg` and require the interpreter's answer.
fn native_matches_interpreter(
    ev: &mut Context,
    f: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    arg: i64,
) {
    let interp = Vm::from_context(ev)
        .execute(f, vec![Value::make_int(arg)])
        .expect("interprets");
    let native = leaf.call(ev as *mut Context as *mut u8, &[Value::make_int(arg)]);
    assert_eq!(native, NativeRun::Ok(interp.bits()), "arg {arg}");
}

#[test]
fn jit_mir_reach_knob_parses_bits() {
    assert_eq!(mir_reach_parse_for_test(None), TestMirReach::OFF);
    for off in ["", "0", "off", "none", "no"] {
        assert_eq!(
            mir_reach_parse_for_test(Some(off)),
            TestMirReach::OFF,
            "{off}"
        );
    }
    for all in ["1", "on", "all"] {
        assert_eq!(
            mir_reach_parse_for_test(Some(all)),
            TestMirReach::ALL,
            "{all}"
        );
    }
    assert_eq!(mir_reach_parse_for_test(Some("dead")), DEAD);
    assert_eq!(
        mir_reach_parse_for_test(Some("args, dead")),
        DEAD,
        "an opt-tier admission is not a legacy MIR bit: ignored"
    );
    assert_eq!(mir_reach_parse_for_test(Some("args")), TestMirReach::OFF);
    assert_eq!(TestMirReach::default(), TestMirReach::OFF, "default off");
}

/// Without the bit the named-let body stays a baseline leaf, bailing the MIR
/// build as `mir-unreachable-block` exactly as before; with it the body is a
/// MIR leaf with the interpreter's answers, and the census counts it.
#[test]
fn jit_mir_reach_dead_tiers_a_named_let_body_to_mir() {
    let _backend = opt_mode_token_scope_for_test(TestOptMode::Legacy);
    force_deopt_for_test(false);
    observe_stats();
    let mut ev = crate::test_utils::new_context();
    let f = named_let();

    force_mir_reach_token_for_test(Some(TestMirReach::OFF));
    reset_compile_stats_for_test();
    let off = compile_bytecode_function_with(&f, None).expect("compiles");
    assert_eq!(
        neovm_eval::test_support::compiled_leaf_copy_for_test(&off).tier_name,
        "baseline"
    );
    assert_eq!(
        neovm_eval::test_support::compiled_leaf_copy_for_test(&off)
            .mir_verdict
            .as_deref(),
        Some("build:UnsupportedOp(\"mir-unreachable-block\")")
    );
    let (failed, dead, _, _) = compile_stats_mir_census_for_test();
    assert_eq!((failed, dead), (1, 0));

    force_mir_reach_token_for_test(Some(DEAD));
    reset_compile_stats_for_test();
    let on = compile_bytecode_function_with(&f, None).expect("compiles");
    assert_eq!(
        neovm_eval::test_support::compiled_leaf_copy_for_test(&on).tier_name,
        "mir"
    );
    assert_eq!(
        neovm_eval::test_support::compiled_leaf_copy_for_test(&on)
            .mir_verdict
            .as_deref(),
        Some("taken")
    );
    let (_, dead, taken, leaders) = compile_stats_mir_census_for_test();
    assert_eq!((taken, dead, leaders), (1, 1, 1));
    for arg in [0, 1, 5, 1000] {
        native_matches_interpreter(&mut ev, &f, &off, arg);
        native_matches_interpreter(&mut ev, &f, &on, arg);
    }
}

/// Every guard of the MIR leaf forced to fail: a run deopts, never answers
/// wrong.
#[test]
fn jit_mir_reach_dead_named_let_under_forced_deopt() {
    let _backend = opt_mode_token_scope_for_test(TestOptMode::Legacy);
    force_deopt_for_test(true);
    force_mir_reach_token_for_test(Some(DEAD));
    let mut ev = crate::test_utils::new_context();
    let f = named_let();
    let leaf = compile_bytecode_function_with(&f, None).expect("compiles");
    assert_eq!(
        neovm_eval::test_support::compiled_leaf_copy_for_test(&leaf).tier_name,
        "mir"
    );
    match leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(7)]) {
        NativeRun::Ok(bits) => assert_eq!(bits, Value::make_int(7).bits()),
        NativeRun::Deopt | NativeRun::DeoptAt(_) => {}
        other => panic!("unexpected {other:?}"),
    }
}

/// fibn's `elb-fibn-named-let` as the real byte compiler emits it: the MIR
/// build bails on the unreachable trailing leader without the bit, and the
/// body is a MIR leaf with the interpreter's answers with it.
#[test]
fn jit_mir_reach_dead_fibn_named_let_from_the_byte_compiler() {
    let _backend = opt_mode_token_scope_for_test(TestOptMode::Legacy);
    crate::test_utils::init_test_tracing();
    force_deopt_for_test(false);
    let mut ev = crate::test_utils::runtime_startup_context();
    let fv = ev
        .eval_str(
            "(eval '(byte-compile (lambda (count)
                (named-let loop ((a 1) (b 0) (count count))
                  (if (= count 0) b (loop (+ a b) a (- count 1)))))) t)",
        )
        .expect("byte-compiles");
    crate::emacs_core::eval::push_scratch_gc_root(fv);
    let f = fv.get_bytecode_data().expect("byte-code function");
    let ops = crate::emacs_core::bytecode::executable_ops_for_test(f);
    let map = executable_gnu_byte_offset_map_for_test(f);
    assert!(
        matches!(
            mir::build_mir(&ops, &f.constants, map.as_deref(), 1),
            Err(CompileError::UnsupportedOp("mir-unreachable-block"))
        ),
        "the named-let tail: {ops:?}"
    );
    force_mir_reach_token_for_test(Some(TestMirReach::OFF));
    let off = compile_bytecode_function_with(f, Some(ev.obarray())).expect("compiles");
    assert_eq!(
        neovm_eval::test_support::compiled_leaf_copy_for_test(&off).tier_name,
        "baseline"
    );
    force_mir_reach_token_for_test(Some(DEAD));
    let on = compile_bytecode_function_with(f, Some(ev.obarray())).expect("compiles");
    assert_eq!(
        neovm_eval::test_support::compiled_leaf_copy_for_test(&on).tier_name,
        "mir"
    );
    for arg in [0, 1, 10, 80] {
        native_matches_interpreter(&mut ev, f, &off, arg);
        native_matches_interpreter(&mut ev, f, &on, arg);
    }
}

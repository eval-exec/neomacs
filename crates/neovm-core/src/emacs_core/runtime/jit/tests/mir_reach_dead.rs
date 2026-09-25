//! The `dead` MIR reach bit (`NEOVM_JIT_MIR_REACH=dead`, design P2.5 R2): a
//! body whose only MIR blocker is an unreachable leader -- the `Return`
//! `seal_ops` appends after a named-let's final back-edge `goto` -- tiers to
//! MIR with the bit, and stays exactly where it was without it.

use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::stats;
use crate::emacs_core::value::LambdaParams;

const DEAD: MirReach = MirReach { dead: true };

fn observe_stats() {
    stats::force_observe_for_test(stats::ObserveOverride {
        stats: true,
        naming: false,
        entry_count: false,
    });
}

/// `(lambda (n) (named-let lp ((n n) (acc 0)) (if (> n 0) (lp (1- n) (1+ acc)) acc)))`
/// in the byte compiler's named-let shape; sealing appends the unreachable
/// trailing `Return` (index 14).
fn named_let() -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
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
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
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
    assert_eq!(MirReach::parse(None), MirReach::OFF);
    for off in ["", "0", "off", "none", "no"] {
        assert_eq!(MirReach::parse(Some(off)), MirReach::OFF, "{off}");
    }
    for all in ["1", "on", "all"] {
        assert_eq!(MirReach::parse(Some(all)), MirReach::ALL, "{all}");
    }
    assert_eq!(MirReach::parse(Some("dead")), DEAD);
    assert_eq!(
        MirReach::parse(Some("args, dead")),
        DEAD,
        "an opt-tier admission is not a legacy MIR bit: ignored"
    );
    assert_eq!(MirReach::parse(Some("args")), MirReach::OFF);
    assert_eq!(MirReach::default(), MirReach::OFF, "default off");
}

/// Without the bit the named-let body stays a baseline leaf, bailing the MIR
/// build as `mir-unreachable-block` exactly as before; with it the body is a
/// MIR leaf with the interpreter's answers, and the census counts it.
#[test]
fn jit_mir_reach_dead_tiers_a_named_let_body_to_mir() {
    force_deopt_for_test(false);
    observe_stats();
    let mut ev = Context::new();
    let f = named_let();

    force_mir_reach_for_test(Some(MirReach::OFF));
    stats::reset_compile_stats();
    let off = compile_bytecode_function_with(&f, None).expect("compiles");
    assert_eq!(off.tier().name(), "baseline");
    assert_eq!(
        off.obs.mir_verdict.as_deref(),
        Some("build:UnsupportedOp(\"mir-unreachable-block\")")
    );
    let s = stats::compile_stats_snapshot();
    assert_eq!((s.mir_build_failed, s.mir_reach_dead), (1, 0));

    force_mir_reach_for_test(Some(DEAD));
    stats::reset_compile_stats();
    let on = compile_bytecode_function_with(&f, None).expect("compiles");
    assert_eq!(on.tier().name(), "mir");
    assert_eq!(on.obs.mir_verdict.as_deref(), Some("taken"));
    let s = stats::compile_stats_snapshot();
    assert_eq!(
        (s.mir_taken, s.mir_reach_dead, s.mir_dead_leaders),
        (1, 1, 1)
    );
    for arg in [0, 1, 5, 1000] {
        native_matches_interpreter(&mut ev, &f, &off, arg);
        native_matches_interpreter(&mut ev, &f, &on, arg);
    }
}

/// Every guard of the MIR leaf forced to fail: a run deopts, never answers
/// wrong.
#[test]
fn jit_mir_reach_dead_named_let_under_forced_deopt() {
    force_deopt_for_test(true);
    force_mir_reach_for_test(Some(DEAD));
    let mut ev = Context::new();
    let f = named_let();
    let leaf = compile_bytecode_function_with(&f, None).expect("compiles");
    assert_eq!(leaf.tier().name(), "mir");
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
    let ops = f.executable_ops();
    assert!(
        matches!(
            mir::build_mir(ops, &f.constants, f.executable_gnu_byte_offset_map(), 1),
            Err(CompileError::UnsupportedOp("mir-unreachable-block"))
        ),
        "the named-let tail: {ops:?}"
    );
    force_mir_reach_for_test(Some(MirReach::OFF));
    let off = compile_bytecode_function_with(f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(off.tier().name(), "baseline");
    force_mir_reach_for_test(Some(DEAD));
    let on = compile_bytecode_function_with(f, Some(&ev.obarray)).expect("compiles");
    assert_eq!(on.tier().name(), "mir");
    for arg in [0, 1, 10, 80] {
        native_matches_interpreter(&mut ev, f, &off, arg);
        native_matches_interpreter(&mut ev, f, &on, arg);
    }
}

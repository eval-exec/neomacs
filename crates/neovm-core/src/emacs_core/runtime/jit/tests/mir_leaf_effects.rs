//! MIR leaf effects (`NEOVM_JIT_LEAF_EFFECTS`, design
//! `p1-2-builtin-intrinsics` §2.8): an opcode site whose whole lowering is
//! a leaf call admits a looping body into the MIR tier and is no safepoint
//! there; everything else keeps the former admission and rooting.

use super::super::*;
use super::{Effects, opcode_site_effects, opcode_site_is_leaf_call};
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::LambdaParams;

fn function(ops: Vec<Op>, constants: Vec<Value>, arity: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity).map(|i| SymId(i as u32 + 1)).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f
}

/// Compile under the two knobs (and restore the environment's).
fn compile(ev: &Context, f: &ByteCodeFunction, effects: bool, leaf: LeafKnob) -> CompiledLeaf {
    force_profit_gate_for_test(false);
    force_leaf_knob_for_test(Some(leaf));
    force_leaf_effects_for_test(Some(effects));
    let compiled = compile_bytecode_function_with(f, Some(&ev.obarray)).expect("compiles");
    force_leaf_effects_for_test(None);
    force_leaf_knob_for_test(None);
    compiled
}

/// The classification: leaf trampolines, value shims and pure table entries
/// have their leaf's declared effects, with no GC, Lisp or deopt; a rooted
/// table entry, `aset`, `set` and every non-builtin op are `UNKNOWN`. Only
/// the sites that call a leaf's body are leaf calls for MIR admission: the
/// inline-lowered `aref`/`setcar`/`setcdr` and the leafless pure entries
/// are not.
#[test]
fn opcode_site_effects_follow_the_lowering() {
    let _ev = Context::new();
    let forbidden = Effects::MAY_GC
        .with(Effects::MAY_REENTER)
        .with(Effects::MAY_DEOPT);
    for knob in [LeafKnob::DEFAULT, LeafKnob::OFF, LeafKnob::ALL] {
        force_leaf_knob_for_test(Some(knob));
        for op in [
            Op::Nth,
            Op::Nthcdr,
            Op::Elt,
            Op::Length,
            Op::Member,
            Op::Memq,
            Op::Assq,
            Op::Equal,
            Op::Get,
            Op::Aref,
            Op::Setcar,
            Op::Setcdr,
            Op::SymbolValue,
            Op::SymbolFunction,
            Op::Nreverse,
        ] {
            let effects = opcode_site_effects(&op, false);
            assert!(!effects.intersects(forbidden), "{op:?} {knob:?}");
            assert!(effects.contains(Effects::MAY_SIGNAL), "{op:?}");
            let leaf_backed = !matches!(
                op,
                Op::Aref | Op::Setcar | Op::Setcdr | Op::SymbolFunction | Op::Nreverse
            );
            assert_eq!(
                opcode_site_is_leaf_call(&op, false),
                leaf_backed,
                "{op:?} {knob:?}"
            );
        }
        // `string=`/`string<` are leaf calls only through their leaves: the
        // table shim roots and takes `&mut Context`.
        for op in [Op::StringEqual, Op::StringLessp] {
            assert_eq!(
                opcode_site_is_leaf_call(&op, false),
                knob.opcode,
                "{op:?} {knob:?}"
            );
            assert!(
                !opcode_site_is_leaf_call(&op, true),
                "{op:?}: AOT has no leaf"
            );
        }
        for op in [
            Op::Aset,
            Op::Set,
            Op::Fset,
            Op::Put,
            Op::Call(1),
            Op::VarRef(0),
            Op::Nconc,
        ] {
            assert_eq!(opcode_site_effects(&op, false), Effects::UNKNOWN, "{op:?}");
        }
        assert!(opcode_site_effects(&Op::Setcar, false).contains(Effects::WRITE_HEAP));
        assert!(opcode_site_effects(&Op::Nth, false).is_read_only() == false);
    }
    force_leaf_knob_for_test(None);
}

/// `(lambda (l n) (let ((acc 0)) (while (> n 0) (setq acc (+ (nth 1 l) acc) n (1- n))) acc))`
fn nth_loop() -> ByteCodeFunction {
    function(
        vec![
            Op::Constant(0), // [l n acc]
            Op::StackRef(1), // loop head: [l n acc n]
            Op::Constant(0),
            Op::Gtr,
            Op::GotoIfNil(15),
            Op::Constant(1), // [l n acc 1]
            Op::StackRef(3), // [l n acc 1 l]
            Op::Nth,         // [l n acc x]
            Op::StackRef(1), // [l n acc x acc]
            Op::Add,
            Op::StackSet(1), // [l n acc']
            Op::StackRef(1),
            Op::Sub1,
            Op::StackSet(2), // [l n' acc']
            Op::Goto(1),
            Op::Return,
        ],
        vec![Value::make_int(0), Value::make_int(1)],
        2,
    )
}

/// The loop is refused by the MIR tier with the knob off
/// (`gate:loop-opaque:Nth`) and taken with it on, under every leaf knob;
/// both answer as the interpreter, including a signal from the leaf inside
/// the loop, and under GC stress (the back-edge poll collects while the list
/// is live across the leaf call).
#[test]
fn a_leaf_call_admits_a_loop_into_the_mir_tier() {
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let f = nth_loop();
    let list = ev.eval_str("(list 1 2 3)").expect("list");
    crate::emacs_core::eval::push_scratch_gc_root(list);
    let improper = ev.eval_str("(cons 1 2)").expect("improper");
    crate::emacs_core::eval::push_scratch_gc_root(improper);
    for knob in [LeafKnob::DEFAULT, LeafKnob::OFF] {
        for (effects, tier) in [
            (false, leaf::LeafTier::Baseline),
            (true, leaf::LeafTier::Mir),
        ] {
            let leaf = compile(&ev, &f, effects, knob);
            assert_eq!(leaf.tier, tier, "effects={effects} {knob:?}");
            let args = [list, Value::make_int(700)];
            let want = Vm::from_context(&mut ev)
                .execute(&f, args.to_vec())
                .expect("runs");
            ev.gc_stress = true;
            let got = leaf.call(ctx, &args);
            ev.gc_stress = false;
            assert_eq!(
                got,
                NativeRun::Ok(want.bits()),
                "effects={effects} {knob:?}"
            );
            assert_eq!(ev.jit_root_stack_top, 0);
            // `(nth 1 '(1 . 2))`: Bnth signals (listp 2) from inside the loop.
            assert_eq!(
                leaf.call(ctx, &[improper, Value::make_int(3)]),
                NativeRun::Signal,
                "effects={effects} {knob:?}"
            );
            let payload = |flow: crate::emacs_core::error::Flow| match flow.into_kind() {
                crate::emacs_core::error::FlowKind::Signal(s) => (
                    s.symbol,
                    s.data
                        .iter()
                        .map(crate::emacs_core::print::print_value)
                        .collect::<Vec<_>>(),
                ),
                other => panic!("expected a signal: {other:?}"),
            };
            let flow = take_pending_flow().expect("stashed");
            let expected = Vm::from_context(&mut ev)
                .execute(&f, vec![improper, Value::make_int(3)])
                .expect_err("signals");
            assert_eq!(payload(flow), payload(expected));
        }
    }
}

/// Inside a MIR leaf a leaf call is no safepoint: the raw fixnum live across
/// it is not retagged there.
///
///     (lambda (l n) (+ (1+ n) (nth 1 l)))
#[test]
fn a_leaf_call_keeps_raw_values_raw() {
    let mut ev = Context::new();
    let ctx = &mut ev as *mut Context as *mut u8;
    let f = function(
        vec![
            Op::StackRef(0), // [l n n]
            Op::Add1,        // [l n n+1]
            Op::Constant(0), // [l n n+1 1]
            Op::StackRef(3), // [l n n+1 1 l]
            Op::Nth,         // [l n n+1 x]
            Op::Add,
            Op::Return,
        ],
        vec![Value::make_int(1)],
        2,
    );
    let list = ev.eval_str("(list 10 20 30)").expect("list");
    crate::emacs_core::eval::push_scratch_gc_root(list);
    let mut retags = Vec::new();
    for effects in [false, true] {
        let leaf = compile(&ev, &f, effects, LeafKnob::DEFAULT);
        assert_eq!(
            leaf.tier,
            leaf::LeafTier::Mir,
            "straight-line: MIR either way"
        );
        retags.push((
            super::super::lowering::retags_emitted(),
            super::super::lowering::untags_emitted(),
        ));
        assert_eq!(
            leaf.call(ctx, &[list, Value::make_int(4)]),
            NativeRun::Ok(Value::make_int(25).bits())
        );
    }
    assert!(
        retags[1].0 + retags[1].1 < retags[0].0 + retags[0].1,
        "(retags, untags) off/on: {retags:?}"
    );
}

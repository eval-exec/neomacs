//! Leaf builtin call sites from compiled code (`NEOVM_JIT_LEAF`, design
//! `p1-2-builtin-intrinsics` §2.5): opcode sites call their leaf's bare
//! trampoline. Every case must match the interpreter's opcode arm -- result
//! or signal -- and every test proves the leaf path engaged (a trampoline
//! counter moved), since a correctness assertion alone passes just as well
//! when nothing was emitted.

use super::leaf_abi::{
    LEAF_STATS, bare_trampoline, declared_fast_half, leaf_trampoline_calls, opcode_leaf,
    trampoline_bodies_for_test,
};
use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::leaf::{LEAVES, LeafEntry, LeafId, LeafShape};
use crate::emacs_core::value::LambdaParams;

fn lexical_fn(nargs: u32, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs).map(crate::emacs_core::intern::SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 8;
    f
}

/// `(lambda (a [b]) (OP a [b]))`.
fn opcode_fn(op: Op, nargs: u32) -> ByteCodeFunction {
    let ops = if nargs == 1 {
        vec![Op::StackRef(0), op, Op::Return]
    } else {
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return]
    };
    lexical_fn(nargs, ops, vec![])
}

fn flow_text(flow: crate::emacs_core::error::Flow) -> String {
    match flow {
        crate::emacs_core::error::Flow::Signal(sig) => format!(
            "signal {} {:?}",
            sig.symbol_name(),
            sig.data.iter().map(print_value).collect::<Vec<_>>()
        ),
        other => format!("{other:?}"),
    }
}

fn interpret(eval: &mut Context, f: &ByteCodeFunction, args: Vec<Value>) -> String {
    let mut vm = Vm::from_context(eval);
    match vm.execute(f, args) {
        Ok(v) => print_value(&v),
        Err(flow) => flow_text(flow),
    }
}

fn native(ctx_ptr: *mut u8, leaf: &CompiledLeaf, args: &[Value], what: &str) -> String {
    match leaf.call(ctx_ptr, args) {
        NativeRun::Ok(bits) => print_value(&Value::from_bits(bits)),
        NativeRun::Signal => flow_text(take_pending_flow().expect("flow stashed")),
        other => panic!("{what} must not leave native code: {other:?}"),
    }
}

/// Compile under `knob` (and restore the environment's knob).
fn compile_with_knob(f: &ByteCodeFunction, knob: LeafKnob) -> CompiledLeaf {
    force_profit_gate_for_test(false);
    force_leaf_knob_for_test(Some(knob));
    let leaf = compile_bytecode_function(f).expect("compiles");
    force_leaf_knob_for_test(None);
    leaf
}

const OPCODE_LEAF_OPS: &[(Op, LeafId, u32)] = &[
    (Op::Get, LeafId::Get, 2),
    (Op::Length, LeafId::Length, 1),
    (Op::Nth, LeafId::Nth, 2),
    (Op::Nthcdr, LeafId::Nthcdr, 2),
    (Op::Elt, LeafId::Elt, 2),
    (Op::Member, LeafId::Member, 2),
    (Op::Equal, LeafId::Equal, 2),
    (Op::StringEqual, LeafId::StringEqual, 2),
    (Op::StringLessp, LeafId::StringLessp, 2),
];

/// Printable (non-circular) operands: printing a result must terminate.
const OPERANDS: &[&str] = &[
    "0",
    "1",
    "2",
    "-1",
    "200",
    "(expt 2 70)",
    "1.5",
    "nil",
    "t",
    "'a",
    "'leaf-sym",
    ":kw",
    "\"abc\"",
    "\"abd\"",
    "(string-to-multibyte \"abc\")",
    "(string-to-unibyte \"a\\377c\")",
    "\"a\u{3b2}c\"",
    "'(a b c)",
    "'((a . 1) (b . 2))",
    "'(a . b)",
    "'(a b . c)",
    "[1 2 3]",
    "(record 'foo 1)",
    "(make-bool-vector 3 t)",
];

/// Every opcode leaf site over operand pairs, natively against the
/// interpreter's arm: the same value or signal, and the trampoline ran.
#[test]
fn opcode_leaf_sites_match_the_interpreter() {
    let mut eval = Context::new();
    eval.eval_str("(put 'leaf-sym 'a 'prop-a)").expect("plist");
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let list = eval
        .eval_str(&format!("(list {})", OPERANDS.join(" ")))
        .expect("operands");
    crate::emacs_core::eval::push_scratch_gc_root(list);
    let values = crate::emacs_core::value::list_to_vec(&list).expect("proper");
    for (op, id, nargs) in OPCODE_LEAF_OPS {
        let f = opcode_fn(op.clone(), *nargs);
        let leaf = compile_with_knob(&f, LeafKnob::ALL);
        let calls0 = leaf_trampoline_calls(*id);
        let mut cases = 0u64;
        for (i, &a) in values.iter().enumerate() {
            if *nargs == 1 {
                let what = format!("({op:?} {})", OPERANDS[i]);
                let want = interpret(&mut eval, &f, vec![a]);
                assert_eq!(native(ctx_ptr, &leaf, &[a], &what), want, "{what}");
                cases += 1;
                continue;
            }
            for (j, &b) in values.iter().enumerate() {
                let what = format!("({op:?} {} {})", OPERANDS[i], OPERANDS[j]);
                let want = interpret(&mut eval, &f, vec![a, b]);
                assert_eq!(native(ctx_ptr, &leaf, &[a, b], &what), want, "{what}");
                cases += 1;
            }
        }
        assert_eq!(
            leaf_trampoline_calls(*id) - calls0,
            cases,
            "{op:?}: every call went through the leaf trampoline"
        );
    }
}

/// `NEOVM_JIT_LEAF=off` (the default) compiles the former table-shim call:
/// same answers, and no trampoline runs.
#[test]
fn knob_off_keeps_the_table_shims() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let f = opcode_fn(Op::Nth, 2);
    let leaf = compile_with_knob(&f, LeafKnob::OFF);
    let list = eval.eval_str("'(a b c)").expect("list");
    let calls0 = leaf_trampoline_calls(LeafId::Nth);
    assert_eq!(
        native(ctx_ptr, &leaf, &[Value::fixnum(1), list], "nth"),
        "b"
    );
    assert_eq!(leaf_trampoline_calls(LeafId::Nth), calls0);
    // Only `bcall` or `string`: opcode sites stay on the table shims too.
    let leaf = compile_with_knob(
        &f,
        LeafKnob {
            bcall: true,
            string: true,
            ..LeafKnob::OFF
        },
    );
    assert_eq!(
        native(ctx_ptr, &leaf, &[Value::fixnum(2), list], "nth"),
        "c"
    );
    assert_eq!(leaf_trampoline_calls(LeafId::Nth), calls0);
}

/// `NEOVM_JIT_LEAF_ONLY` restricts leaf sites to the named leaves.
#[test]
fn leaf_only_filter_restricts_the_sites() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    force_leaf_only_for_test(Some(&["length"]));
    let nth = compile_with_knob(&opcode_fn(Op::Nth, 2), LeafKnob::ALL);
    let length = compile_with_knob(&opcode_fn(Op::Length, 1), LeafKnob::ALL);
    force_leaf_only_for_test(None);
    let list = eval.eval_str("'(a b c)").expect("list");
    let (nth0, len0) = (
        leaf_trampoline_calls(LeafId::Nth),
        leaf_trampoline_calls(LeafId::Length),
    );
    assert_eq!(native(ctx_ptr, &nth, &[Value::fixnum(0), list], "nth"), "a");
    assert_eq!(native(ctx_ptr, &length, &[list], "length"), "3");
    assert_eq!(leaf_trampoline_calls(LeafId::Nth), nth0, "nth filtered out");
    assert_eq!(leaf_trampoline_calls(LeafId::Length), len0 + 1);
}

#[test]
fn leaf_knob_parses() {
    assert_eq!(LeafKnob::parse(None), LeafKnob::OFF);
    for off in ["", "0", "off", "false", "no"] {
        assert_eq!(LeafKnob::parse(Some(off)), LeafKnob::OFF, "{off}");
    }
    for on in ["1", "on", "all", "true", "yes"] {
        assert_eq!(LeafKnob::parse(Some(on)), LeafKnob::ALL, "{on}");
    }
    assert_eq!(
        LeafKnob::parse(Some("opcode, string")),
        LeafKnob {
            opcode: true,
            string: true,
            ..LeafKnob::OFF
        }
    );
    assert_eq!(
        LeafKnob::parse(Some("bcall,bogus")),
        LeafKnob {
            bcall: true,
            ..LeafKnob::OFF
        }
    );
}

/// A trampoline names its body as an item (so it inlines); that item must
/// be the body its spec declares, with the declared containment. Every
/// opcode leaf with a trampoline is reachable from its opcode.
#[test]
fn trampolines_call_their_spec_bodies() {
    for (id, body, fast) in trampoline_bodies_for_test() {
        let spec = id.spec();
        let declared = match spec.entry {
            LeafEntry::L1(f) => f as usize,
            LeafEntry::L2(f) => f as usize,
            LeafEntry::L3(f) => f as usize,
        };
        assert_eq!(body, declared, "{}: trampoline body", spec.name);
        assert_eq!(fast, declared_fast_half(spec), "{}: containment", spec.name);
        assert!(bare_trampoline(id).is_some(), "{}", spec.name);
    }
    for spec in LEAVES {
        if bare_trampoline(spec.id).is_some() {
            assert_eq!(spec.shape, LeafShape::Opcode, "{}", spec.name);
            let ops = OPCODE_LEAF_OPS
                .iter()
                .filter(|(op, _, _)| opcode_leaf(op) == Some(spec.id))
                .count();
            assert_eq!(ops, 1, "{}: exactly one opcode reaches it", spec.name);
        }
    }
}

/// A leaf's signal reaches a handler in the same body, natively, with the
/// data the interpreter reports.
///
///     (lambda (n l) (condition-case err (nth n l) (error (list 'caught err))))
#[test]
fn leaf_signals_are_caught_by_a_leaf_local_handler() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for (op, nargs) in [(Op::Nth, 2u32), (Op::Length, 1), (Op::Get, 2)] {
        let mut ops = vec![Op::PushConditionCase(0)];
        if nargs == 2 {
            ops.extend([Op::StackRef(1), Op::StackRef(1)]);
        } else {
            ops.push(Op::StackRef(1));
        }
        ops.extend([op.clone(), Op::PopHandler, Op::Return]);
        let handler = ops.len();
        ops[0] = Op::PushConditionCase(handler as u32);
        ops.extend([Op::Constant(0), Op::StackRef(1), Op::List(2), Op::Return]);
        let f = lexical_fn(2, ops, vec![Value::symbol("caught")]);
        let leaf = compile_with_knob(&f, LeafKnob::ALL);
        let improper = eval.eval_str("'(a . b)").expect("improper list");
        let (a, b) = match op {
            Op::Nth => (Value::fixnum(2), improper),
            Op::Length => (improper, Value::NIL),
            _ => (Value::fixnum(5), Value::symbol("p")),
        };
        let want = interpret(&mut eval, &f, vec![a, b]);
        assert!(
            want.starts_with("(caught (wrong-type-argument"),
            "{op:?}: {want}"
        );
        match leaf.call(ctx_ptr, &[a, b]) {
            NativeRun::Ok(bits) => {
                assert_eq!(print_value(&Value::from_bits(bits)), want, "{op:?}")
            }
            other => panic!("{op:?}: the handler must catch natively, got {other:?}"),
        }
    }
}

/// Live values below a leaf site survive a `signal-hook-function` that
/// collects on the signal edge (the site itself roots nothing: the flow is
/// dispatched at the handler match, which roots).
///
///     (lambda (n l) (let ((h (cons 1 2))) (condition-case nil (nth n l) (error h))))
#[test]
fn leaf_site_signal_edge_keeps_the_residual_alive() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let mut ops = vec![
        Op::Constant(0),          // [n l 1]
        Op::Constant(1),          // [n l 1 2]
        Op::Cons,                 // [n l h]
        Op::PushConditionCase(0), // patched
        Op::StackRef(2),          // [n l h n]
        Op::StackRef(2),          // [n l h n l]
        Op::Nth,                  // [n l h x]
        Op::Pop,
        Op::PopHandler,
        Op::Return, // h
    ];
    let handler = ops.len();
    ops[3] = Op::PushConditionCase(handler as u32);
    ops.extend([Op::Pop, Op::Return]);
    let f = lexical_fn(2, ops, vec![Value::make_int(1), Value::make_int(2)]);
    let leaf = compile_with_knob(&f, LeafKnob::ALL);
    eval.eval_str(
        "(setq signal-hook-function
               (lambda (_sym _data) (garbage-collect) (make-list 4096 (cons 0 0)) nil))",
    )
    .expect("hook");
    let calls0 = leaf_trampoline_calls(LeafId::Nth);
    for _ in 0..3 {
        let l = eval.eval_str("(cons 'a 'b)").expect("improper");
        match leaf.call(ctx_ptr, &[Value::fixnum(3), l]) {
            NativeRun::Ok(bits) => {
                let h = Value::from_bits(bits);
                assert!(h.is_cons(), "h survived");
                assert_eq!(h.cons_car(), Value::make_int(1));
                assert_eq!(h.cons_cdr(), Value::make_int(2));
            }
            other => panic!("must catch natively, got {other:?}"),
        }
    }
    eval.eval_str("(setq signal-hook-function nil)")
        .expect("unhook");
    assert_eq!(leaf_trampoline_calls(LeafId::Nth) - calls0, 3);
}

/// Each compiled opcode leaf site counts in the exit census.
#[test]
fn opcode_leaf_sites_are_counted() {
    let before = LEAF_STATS[LeafId::Elt.index()]
        .opcode_sites
        .load(Ordering::Relaxed);
    let _leaf = compile_with_knob(&opcode_fn(Op::Elt, 2), LeafKnob::ALL);
    let after = LEAF_STATS[LeafId::Elt.index()]
        .opcode_sites
        .load(Ordering::Relaxed);
    assert!(after > before);
    assert!(
        super::leaf_abi::render_leaf_stats().contains("elt:opcode_sites="),
        "{}",
        super::leaf_abi::render_leaf_stats()
    );
}

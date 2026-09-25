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

// ---------------------------------------------------------------------------
// Bcall leaf sites: `Op::Call` on a builtin with a Bcall leaf.
// ---------------------------------------------------------------------------

/// `(lambda (a1..an) (CALLEE a1..an))` through `Op::Call` (GNU `Bcall`).
fn bcall_fn(callee: &str, nargs: usize) -> ByteCodeFunction {
    let mut ops = vec![Op::Constant(0)];
    for _ in 0..nargs {
        ops.push(Op::StackRef(nargs as u16));
    }
    ops.push(Op::Call(nargs as u16));
    ops.push(Op::Return);
    let mut f = lexical_fn(nargs as u32, ops, vec![Value::symbol(callee)]);
    f.max_stack = 16;
    f
}

fn compile_bcall_with_knob(ev: &Context, f: &ByteCodeFunction, knob: LeafKnob) -> CompiledLeaf {
    force_profit_gate_for_test(false);
    force_leaf_knob_for_test(Some(knob));
    let leaf = compile_bytecode_function_with(f, Some(&ev.obarray)).expect("compiles");
    force_leaf_knob_for_test(None);
    leaf
}

fn bcall_knob() -> LeafKnob {
    LeafKnob {
        bcall: true,
        ..LeafKnob::OFF
    }
}

/// Evaluate `(list SRC...)` once, rooted.
fn operands(ev: &mut Context, sources: &[&str]) -> Vec<Value> {
    let list = ev
        .eval_str(&format!("(list {})", sources.join(" ")))
        .expect("operands");
    crate::emacs_core::eval::push_scratch_gc_root(list);
    crate::emacs_core::value::list_to_vec(&list).expect("proper")
}

fn leaf_generic(id: LeafId) -> u64 {
    LEAF_STATS[id.index()].generic.load(Ordering::Relaxed)
}

fn leaf_guard_miss(id: LeafId) -> u64 {
    LEAF_STATS[id.index()].guard_miss.load(Ordering::Relaxed)
}

/// `gethash`, `plist-get` and `get-char-property` at two and three
/// arguments: natively through the leaf against the interpreter's `Bcall`,
/// including the signals and the bounce shapes (a user-test table, a
/// PREDICATE), which the reference protocol answers.
#[test]
fn bcall_leaf_sites_match_the_protocol_call() {
    let mut ev = Context::new();
    ev.eval_str(
        "(progn
           (define-hash-table-test 'leaf-site-test
             #'(lambda (x y) (equal x y)) #'(lambda (k) (sxhash-equal k)))
           (set-buffer (get-buffer-create \" leaf-site\"))
           (insert \"hello world\")
           (put-text-property 3 5 'p 'text)
           (overlay-put (make-overlay 4 7) 'p 'overlay))",
    )
    .expect("setup");
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let cases: &[(&str, LeafId, &[&str], &[&str], &[&str])] = &[
        (
            "gethash",
            LeafId::Gethash,
            &["7", "'a", "\"s\"", "nil"],
            &[
                "(let ((h (make-hash-table))) (puthash 7 'seven h) h)",
                "(let ((h (make-hash-table :test 'equal))) (puthash \"s\" 3 h) h)",
                "(let ((h (make-hash-table :test 'leaf-site-test))) (puthash \"s\" 4 h) h)",
                "5",
                "nil",
            ],
            &["nil", "'dflt"],
        ),
        (
            "plist-get",
            LeafId::PlistGet,
            &["'(a 1 b 2)", "'(a . 1)", "nil", "5"],
            &["'a", "'b", "'z"],
            &["nil", "#'eq"],
        ),
        (
            "get-char-property",
            LeafId::GetCharProperty,
            &["1", "3", "5", "12", "0", "'x"],
            &["'p", "'q"],
            &["nil", "(current-buffer)", "(propertize \"abc\" 'p 'str)"],
        ),
    ];
    for (name, id, firsts, seconds, thirds) in cases {
        let firsts = operands(&mut ev, firsts);
        let seconds = operands(&mut ev, seconds);
        let thirds = operands(&mut ev, thirds);
        for nargs in [2usize, 3] {
            let f = bcall_fn(name, nargs);
            let leaf = compile_bcall_with_knob(&ev, &f, bcall_knob());
            let (runs0, generic0) = (leaf_trampoline_calls(*id), leaf_generic(*id));
            let mut cases = 0u64;
            for &a in &firsts {
                for &b in &seconds {
                    for &c in thirds
                        .iter()
                        .take(if nargs == 3 { thirds.len() } else { 1 })
                    {
                        let args: Vec<Value> = [a, b, c][..nargs].to_vec();
                        let what = format!("({name} {})", args.len());
                        let want = interpret(&mut ev, &f, args.clone());
                        assert_eq!(native(ctx_ptr, &leaf, &args, &what), want, "{what}");
                        cases += 1;
                    }
                }
            }
            assert_eq!(
                leaf_trampoline_calls(*id) - runs0,
                cases,
                "({name} ..{nargs}): every call ran the leaf"
            );
            if matches!(id, LeafId::Gethash) || (nargs == 3 && matches!(id, LeafId::PlistGet)) {
                assert!(
                    leaf_generic(*id) > generic0,
                    "({name} ..{nargs}): a bounce shape reached the reference"
                );
            }
        }
    }
}

/// Arm a `signal-hook-function` that records whether a backtrace frame for
/// TARGET is live when a signal is raised.
fn install_frame_probe(ev: &mut Context, target: &str) {
    ev.eval_str(&format!(
        "(setq leaf-target '{target} leaf-seen 'no-signal)"
    ))
    .expect("probe target");
    ev.eval_str(
        "(setq signal-hook-function
           (lambda (_sym _data)
             (setq leaf-seen 'no-frame)
             (mapbacktrace
               (lambda (_evald func args _flags)
                 (if (eq func leaf-target)
                     (setq leaf-seen (cons 'frame args)))))))",
    )
    .expect("install the frame probe");
}

/// GNU `Bcall` records the builtin's frame, so a signal hook sees
/// `(gethash KEY 5)` under a leaf that signalled -- pushed lazily by
/// `neovm_jit_leaf_signal_frame` -- with the call's own arguments, exactly
/// as the interpreter shows it. The specpdl is balanced afterwards.
#[test]
fn a_leaf_signal_runs_under_the_builtins_frame() {
    let mut ev = Context::new();
    install_frame_probe(&mut ev, "gethash");
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    for nargs in [2usize, 3] {
        let f = bcall_fn("gethash", nargs);
        let leaf = compile_bcall_with_knob(&ev, &f, bcall_knob());
        let args: Vec<Value> =
            [Value::fixnum(1), Value::fixnum(5), Value::symbol("d")][..nargs].to_vec();
        let want = interpret(&mut ev, &f, args.clone());
        let want_probe = print_value(&ev.eval_str("leaf-seen").expect("probe"));
        ev.eval_str("(setq leaf-seen 'no-signal)").expect("reset");
        let specpdl = ev.specpdl.len();
        let runs0 = leaf_trampoline_calls(LeafId::Gethash);
        let got = native(ctx_ptr, &leaf, &args, "gethash");
        assert_eq!(got, want);
        assert_eq!(got, "signal wrong-type-argument [\"hash-table-p\", \"5\"]");
        let got_probe = print_value(&ev.eval_str("leaf-seen").expect("probe"));
        assert_eq!(got_probe, want_probe, "the hook saw the same frame");
        assert!(got_probe.starts_with("(frame 1 5"), "{got_probe}");
        assert_eq!(ev.specpdl.len(), specpdl, "the lazy frame was popped");
        assert_eq!(leaf_trampoline_calls(LeafId::Gethash) - runs0, 1);
    }
    ev.eval_str("(setq signal-hook-function nil)")
        .expect("unhook");
}

/// A redefinition takes effect at the next call (GNU `Bcall` reads the
/// function cell every time): the guard's re-validation fails and the
/// reference protocol calls the new definition; restoring re-arms.
#[test]
fn a_redefined_builtin_runs_from_a_leaf_site() {
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let f = bcall_fn("gethash", 2);
    let leaf = compile_bcall_with_knob(&ev, &f, bcall_knob());
    let table = ev
        .eval_str("(let ((h (make-hash-table))) (puthash 1 'one h) h)")
        .expect("table");
    crate::emacs_core::eval::push_scratch_gc_root(table);
    let args = [Value::fixnum(1), table];
    assert_eq!(native(ctx_ptr, &leaf, &args, "gethash"), "one");
    ev.eval_str(
        "(progn (defvar leaf-orig-gethash (symbol-function 'gethash))
                (fset 'gethash (lambda (_k _h &optional _d) 'redefined)))",
    )
    .expect("redefine");
    let miss0 = leaf_guard_miss(LeafId::Gethash);
    assert_eq!(native(ctx_ptr, &leaf, &args, "redefined"), "redefined");
    assert!(leaf_guard_miss(LeafId::Gethash) > miss0);
    ev.eval_str("(fset 'gethash leaf-orig-gethash)")
        .expect("restore");
    let runs0 = leaf_trampoline_calls(LeafId::Gethash);
    assert_eq!(native(ctx_ptr, &leaf, &args, "restored"), "one");
    assert_eq!(
        leaf_trampoline_calls(LeafId::Gethash) - runs0,
        1,
        "the restored binding re-arms the leaf"
    );
}

/// The guard, condition by condition, on the trampoline itself: each
/// observer of GNU's `Bcall` protocol sends the call to the reference
/// (NEED_GENERIC) before the leaf runs.
#[test]
fn the_bcall_guard_declines_every_observable_protocol_step() {
    use super::leaf_abi::neovm_leaf_bcall_gethash as tramp;
    let mut ev = Context::new();
    let table = ev
        .eval_str("(let ((h (make-hash-table))) (puthash 1 'one h) h)")
        .expect("table");
    crate::emacs_core::eval::push_scratch_gc_root(table);
    let sym = crate::emacs_core::intern::intern("gethash");
    let expected = ev.obarray.symbol_function_id(sym).expect("fbound").bits() as u64;
    let slot = SpecSlot::at_epoch(ev.obarray.function_epoch());
    slot.bind_subr(sym.0, expected);
    let call = |ev: &mut Context, slot: &SpecSlot| {
        tramp(
            ev as *const Context,
            slot as *const SpecSlot,
            Value::fixnum(1).bits() as i64,
            table.bits() as i64,
            Value::NIL.bits() as i64,
        )
    };
    let one = Value::symbol("one").bits() as i64;
    let generic = super::leaf_abi::LEAF_NEED_GENERIC;
    assert_eq!(call(&mut ev, &slot), one, "armed: the leaf answers");

    ev.set_quit_flag_value(Value::T);
    assert_eq!(call(&mut ev, &slot), generic, "a pending quit");
    ev.set_quit_flag_value(Value::NIL);

    let depth = ev.depth;
    ev.depth = ev.max_depth;
    assert_eq!(call(&mut ev, &slot), generic, "at the depth limit");
    ev.depth = depth;

    // Straight through the forwarded cell: evaluating a `setq` with the
    // flag armed would enter the debugger.
    let debug_cell = ev
        .obarray
        .debug_on_next_call_bool_fwd(crate::emacs_core::intern::intern("debug-on-next-call"))
        .expect("a forwarded boolean");
    debug_cell.set(true);
    assert_eq!(call(&mut ev, &slot), generic, "debug-on-next-call");
    debug_cell.set(false);

    // An unrelated redefinition moves the epoch: re-validated and re-armed.
    ev.eval_str("(fset 'leaf-guard-unrelated 'car)")
        .expect("fset");
    assert_ne!(
        slot.epoch.load(Ordering::Relaxed),
        ev.obarray.function_epoch()
    );
    assert_eq!(
        call(&mut ev, &slot),
        one,
        "re-armed after an unrelated fset"
    );
    assert_eq!(
        slot.epoch.load(Ordering::Relaxed),
        ev.obarray.function_epoch()
    );

    // A changed binding: declined, and it stays declined.
    ev.eval_str("(progn (defvar leaf-guard-orig (symbol-function 'gethash)) (fset 'gethash 'car))")
        .expect("rebind");
    assert_eq!(call(&mut ev, &slot), generic, "a changed binding");
    ev.eval_str("(fset 'gethash leaf-guard-orig)")
        .expect("restore");
    assert_eq!(call(&mut ev, &slot), one, "restored");

    // A loader-disarmed slot never re-arms.
    let disarmed = SpecSlot::at_epoch(SPEC_EPOCH_DISARMED);
    disarmed.bind_subr(sym.0, expected);
    assert_eq!(call(&mut ev, &disarmed), generic, "a disarmed slot");
}

/// Live values below a Bcall leaf site survive a `signal-hook-function`
/// that collects while the lazy frame's dispatch runs (the cold signal
/// block roots the residual), and a handler in the same body catches it.
///
///     (lambda (k h) (let ((r (cons 1 2))) (condition-case nil (gethash k h) (error r))))
#[test]
fn a_bcall_leaf_signal_edge_keeps_the_residual_alive() {
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let mut ops = vec![
        Op::Constant(0),          // [k h 1]
        Op::Constant(1),          // [k h 1 2]
        Op::Cons,                 // [k h r]
        Op::PushConditionCase(0), // patched
        Op::Constant(2),          // [k h r gethash]
        Op::StackRef(3),          // [k h r gethash k]
        Op::StackRef(3),          // [k h r gethash k h]
        Op::Call(2),              // [k h r x]
        Op::Pop,
        Op::PopHandler,
        Op::Return, // r
    ];
    let handler = ops.len();
    ops[3] = Op::PushConditionCase(handler as u32);
    ops.extend([Op::Pop, Op::Return]);
    let mut f = lexical_fn(
        2,
        ops,
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::symbol("gethash"),
        ],
    );
    f.max_stack = 16;
    let leaf = compile_bcall_with_knob(&ev, &f, bcall_knob());
    ev.eval_str(
        "(setq signal-hook-function
               (lambda (_sym _data) (garbage-collect) (make-list 4096 (cons 0 0)) nil))",
    )
    .expect("hook");
    let runs0 = leaf_trampoline_calls(LeafId::Gethash);
    for _ in 0..3 {
        match leaf.call(ctx_ptr, &[Value::fixnum(1), Value::fixnum(5)]) {
            NativeRun::Ok(bits) => {
                let r = Value::from_bits(bits);
                assert!(r.is_cons(), "r survived");
                assert_eq!(r.cons_car(), Value::make_int(1));
                assert_eq!(r.cons_cdr(), Value::make_int(2));
            }
            other => panic!("must catch natively, got {other:?}"),
        }
    }
    ev.eval_str("(setq signal-hook-function nil)")
        .expect("unhook");
    assert_eq!(leaf_trampoline_calls(LeafId::Gethash) - runs0, 3);
}

/// With the `bcall` part off, the same site takes the protocol shim.
#[test]
fn bcall_knob_off_keeps_the_protocol_shim() {
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let f = bcall_fn("gethash", 2);
    let leaf = compile_bcall_with_knob(
        &ev,
        &f,
        LeafKnob {
            opcode: true,
            string: true,
            ..LeafKnob::OFF
        },
    );
    let table = ev
        .eval_str("(let ((h (make-hash-table))) (puthash 1 'one h) h)")
        .expect("table");
    crate::emacs_core::eval::push_scratch_gc_root(table);
    let runs0 = leaf_trampoline_calls(LeafId::Gethash);
    assert_eq!(
        native(ctx_ptr, &leaf, &[Value::fixnum(1), table], "gethash"),
        "one"
    );
    assert_eq!(leaf_trampoline_calls(LeafId::Gethash), runs0);
}

/// A subr site's slot carries its binding words, which no slot walker may
/// clear (p1-0-integration §2 P1.2 correction 2): `clear_leaf` refuses in
/// debug builds, and the words survive the calls.
#[test]
fn subr_site_slots_keep_their_binding_words() {
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let f = bcall_fn("gethash", 2);
    let leaf = compile_bcall_with_knob(&ev, &f, bcall_knob());
    let sym = crate::emacs_core::intern::intern("gethash");
    let expected = ev.obarray.symbol_function_id(sym).expect("fbound").bits() as u64;
    let slot = leaf
        .spec_slots
        .iter()
        .find(|s| s.holds_subr_binding())
        .expect("the site's slot is bound");
    assert_eq!(slot.subr_binding(), (sym, expected));
    let _ = native(
        ctx_ptr,
        &leaf,
        &[Value::fixnum(1), Value::fixnum(5)],
        "gethash",
    );
    assert_eq!(slot.subr_binding(), (sym, expected), "unchanged by a call");
    #[cfg(debug_assertions)]
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| slot.clear_leaf())).is_err(),
        "clear_leaf refuses a subr site's slot"
    );
}

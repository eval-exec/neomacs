//! CLIF intrinsics (`NEOVM_JIT_INTRINSICS`, design `p1-2-builtin-intrinsics`
//! §2.7): every intrinsic site must answer exactly as the interpreter's
//! opcode arm -- value or signal -- on the shapes it takes inline and on the
//! ones it leaves to the call, under both miss paths (the leaf trampoline and
//! the table or value shim). Each test also proves the inline path ANSWERED
//! the shapes it claims (the call it falls through to did not run) and
//! missed the others, since a correctness assertion alone passes just as
//! well when nothing was emitted.

use super::super::dispatch::LIST_SEARCH_SHIM_CALLS;
use super::super::leaf_abi::leaf_trampoline_calls;
use super::*;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::leaf::LeafId;
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
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
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
    match flow.into_kind() {
        crate::emacs_core::error::FlowKind::Signal(sig) => format!(
            "signal {} {:?}",
            sig.symbol_name(),
            sig.data.iter().map(print_value).collect::<Vec<_>>()
        ),
        other => format!("{other:?}"),
    }
}

/// Printed so that a circular answer cannot hang the comparison: a cons is
/// compared by identity.
fn show(v: Value) -> String {
    if v.is_cons() {
        format!("#<cons {:x}>", v.bits())
    } else {
        print_value(&v)
    }
}

fn interpret(eval: &mut Context, f: &ByteCodeFunction, args: Vec<Value>) -> String {
    let mut vm = Vm::from_context(eval);
    match vm.execute(f, args) {
        Ok(v) => show(v),
        Err(flow) => flow_text(flow),
    }
}

fn native(ctx_ptr: *mut u8, leaf: &CompiledLeaf, args: &[Value], what: &str) -> String {
    match leaf.call(ctx_ptr, args) {
        NativeRun::Ok(bits) => show(Value::from_bits(bits)),
        NativeRun::Signal => flow_text(take_pending_flow().expect("flow stashed")),
        other => panic!("{what} must not leave native code: {other:?}"),
    }
}

/// Compile under the two knobs (and restore the environment's).
fn compile_with(f: &ByteCodeFunction, leaf: LeafKnob, intrinsics: IntrinsicKnob) -> CompiledLeaf {
    force_profit_gate_for_test(false);
    force_leaf_knob_for_test(Some(leaf));
    force_intrinsic_knob_for_test(Some(intrinsics));
    let compiled = compile_bytecode_function(f).expect("compiles");
    force_intrinsic_knob_for_test(None);
    force_leaf_knob_for_test(None);
    compiled
}

/// Evaluate `(list SRC...)` once, rooted.
fn operands(ev: &mut Context, sources: &[&str]) -> Vec<Value> {
    let list = ev
        .eval_str(&format!("(list {})", sources.join(" ")))
        .expect("operands");
    crate::emacs_core::eval::push_scratch_gc_root(list);
    crate::emacs_core::value::list_to_vec(&list).expect("proper")
}

/// Both miss paths an opcode site may have.
const LEAF_KNOBS: [LeafKnob; 2] = [LeafKnob::DEFAULT, LeafKnob::OFF];

#[test]
fn intrinsic_knob_parses() {
    assert_eq!(IntrinsicKnob::parse(None), IntrinsicKnob::OFF);
    for off in ["", "0", "off", "false", "no"] {
        assert_eq!(IntrinsicKnob::parse(Some(off)), IntrinsicKnob::OFF, "{off}");
    }
    for on in ["1", "on", "all", "true", "yes"] {
        assert_eq!(IntrinsicKnob::parse(Some(on)), IntrinsicKnob::ALL, "{on}");
    }
    assert_eq!(
        IntrinsicKnob::parse(Some("length, symbol-value,bogus")),
        IntrinsicKnob {
            length: true,
            symbol_value: true,
            ..IntrinsicKnob::OFF
        }
    );
}

/// With the knob off nothing is emitted: the census stays put.
#[test]
fn knob_off_emits_no_intrinsic() {
    let before: Vec<u64> = Intrinsic::ALL
        .iter()
        .map(|&w| intrinsic_sites_for_test(w))
        .collect();
    for (op, nargs) in [
        (Op::Length, 1),
        (Op::Nth, 2),
        (Op::Memq, 2),
        (Op::Assq, 2),
        (Op::Member, 2),
        (Op::SymbolValue, 1),
    ] {
        compile_with(&opcode_fn(op, nargs), LeafKnob::DEFAULT, IntrinsicKnob::OFF);
    }
    let after: Vec<u64> = Intrinsic::ALL
        .iter()
        .map(|&w| intrinsic_sites_for_test(w))
        .collect();
    assert_eq!(before, after);
}

// ---------------------------------------------------------------------------
// I3: length.
// ---------------------------------------------------------------------------

/// `length` over every sequence shape, natively against `Blength`; the
/// inline path answers nil, proper lists up to 64 conses, strings and plain
/// vectors and records (the leaf trampoline does not run), and leaves the
/// rest -- longer, improper and circular lists, bool-vectors, char-tables,
/// closures and non-sequences -- to the call.
#[test]
fn length_intrinsic_matches_blength() {
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    // (source, answered inline)
    let cases: &[(&str, bool)] = &[
        ("nil", true),
        ("'(a)", true),
        ("'(a b c)", true),
        ("(make-list 63 'x)", true),
        ("(make-list 64 'x)", true),
        ("(make-list 65 'x)", false),
        ("(make-list 200 'x)", false),
        ("'(a . b)", false),
        ("'(a b . c)", false),
        ("(let ((l (list 1 2 3))) (setcdr (cdr (cdr l)) l) l)", false),
        ("\"\"", true),
        ("\"abc\"", true),
        ("\"a\u{3b2}c\"", true),
        ("(string-to-unibyte \"a\\377c\")", true),
        ("[]", true),
        ("[1]", true),
        ("[1 2 3]", true),
        ("(record 'foo 1 2)", true),
        ("(make-bool-vector 3 t)", false),
        ("(make-bool-vector 70 nil)", false),
        ("(make-char-table 'foo)", false),
        ("(symbol-function 'car)", false),
        ("5", false),
        ("'a", false),
        ("1.5", false),
        ("t", false),
    ];
    let sources: Vec<&str> = cases.iter().map(|(s, _)| *s).collect();
    let values = operands(&mut ev, &sources);
    let f = opcode_fn(Op::Length, 1);
    for leaf_knob in LEAF_KNOBS {
        let sites0 = intrinsic_sites_for_test(Intrinsic::Length);
        let leaf = compile_with(&f, leaf_knob, IntrinsicKnob::ALL);
        assert_eq!(intrinsic_sites_for_test(Intrinsic::Length), sites0 + 1);
        for (&v, (src, inline)) in values.iter().zip(cases) {
            let what = format!("(length {src}) {leaf_knob:?}");
            let want = interpret(&mut ev, &f, vec![v]);
            let calls0 = leaf_trampoline_calls(LeafId::Length);
            assert_eq!(native(ctx_ptr, &leaf, &[v], &what), want, "{what}");
            if leaf_knob.opcode {
                assert_eq!(
                    leaf_trampoline_calls(LeafId::Length) - calls0,
                    u64::from(!inline),
                    "{what}: answered inline = {inline}"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// I4: nth, nthcdr, elt at a constant index.
// ---------------------------------------------------------------------------

/// `(lambda (l) (OP K l))`, or `(lambda (l) (elt l K))`.
fn constant_index_fn(op: Op, k: Value) -> ByteCodeFunction {
    let ops = if op == Op::Elt {
        vec![Op::StackRef(0), Op::Constant(0), op, Op::Return]
    } else {
        vec![Op::Constant(0), Op::StackRef(1), op, Op::Return]
    };
    lexical_fn(1, ops, vec![k])
}

/// Every constant index 0..=6 (the unrolled 0..=4 and two that emit
/// nothing) over list shapes, natively against the opcode arm.
#[test]
fn constant_index_intrinsics_match_their_opcode_arms() {
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let lists = operands(
        &mut ev,
        &[
            "nil",
            "'(a)",
            "'(a b c)",
            "'(a b c d e f)",
            "'(a . b)",
            "'(a b . c)",
            "'(a b c . d)",
            "(let ((l (list 1 2))) (setcdr (cdr l) l) l)",
            "[1 2 3 4 5 6]",
            "\"abcdef\"",
            "5",
            "'x",
        ],
    );
    for (op, id, which) in [
        (Op::Nth, LeafId::Nth, Intrinsic::Nth),
        (Op::Nthcdr, LeafId::Nthcdr, Intrinsic::Nthcdr),
        (Op::Elt, LeafId::Elt, Intrinsic::Elt),
    ] {
        for k in 0..=6i64 {
            let f = constant_index_fn(op.clone(), Value::fixnum(k));
            for leaf_knob in LEAF_KNOBS {
                let sites0 = intrinsic_sites_for_test(which);
                let leaf = compile_with(&f, leaf_knob, IntrinsicKnob::ALL);
                let emitted = intrinsic_sites_for_test(which) - sites0;
                assert_eq!(emitted, u64::from(k <= NTH_INLINE_MAX), "{op:?} {k}");
                let mut inline_answers = 0;
                for &l in &lists {
                    let what = format!("({op:?} {k} {}) {leaf_knob:?}", show(l));
                    let want = interpret(&mut ev, &f, vec![l]);
                    let calls0 = leaf_trampoline_calls(id);
                    assert_eq!(native(ctx_ptr, &leaf, &[l], &what), want, "{what}");
                    if leaf_knob.opcode && leaf_trampoline_calls(id) == calls0 {
                        inline_answers += 1;
                    }
                }
                if leaf_knob.opcode && emitted == 1 {
                    // Proper lists and nil at least are answered inline.
                    assert!(inline_answers >= 3, "{op:?} {k}: {inline_answers}");
                }
            }
        }
    }
}

/// A dynamic index emits nothing: the site is the leaf call alone.
#[test]
fn a_dynamic_index_emits_no_intrinsic() {
    let sites0 = intrinsic_sites_for_test(Intrinsic::Nth);
    compile_with(
        &opcode_fn(Op::Nth, 2),
        LeafKnob::DEFAULT,
        IntrinsicKnob::ALL,
    );
    assert_eq!(intrinsic_sites_for_test(Intrinsic::Nth), sites0);
}

// ---------------------------------------------------------------------------
// I5: memq, assq, member.
// ---------------------------------------------------------------------------

/// `memq`/`assq`/`member` over keys × lists natively against the opcode
/// arm, with `symbols-with-pos-enabled` off and on. A match within the
/// first 16 conses, or the end of a short proper list, is answered inline
/// (no shim or leaf call runs); a longer walk, an improper tail, a
/// `member` key that is neither a fixnum nor a bare symbol, and the flag on
/// reach the call.
#[test]
fn list_search_intrinsics_match_their_opcode_arms() {
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let keys = operands(
        &mut ev,
        &[
            "'a",
            "'z",
            "3",
            "0",
            "nil",
            "\"s\"",
            "1.5",
            "(expt 2 70)",
            "'(1)",
        ],
    );
    let lists = operands(
        &mut ev,
        &[
            "nil",
            "'(a b c)",
            "'(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 a)",
            "(append (make-list 15 'q) '(a))",
            "(append (make-list 16 'q) '(a))",
            "'((a . 1) (3 . 2) (nil . n) b (z . 9))",
            "'(a . b)",
            "'(q q . a)",
            "(list \"s\" 1.5 (expt 2 70) '(1))",
            "(let ((l (list 'q 'q))) (setcdr (cdr l) l) l)",
            "(let ((l (list 'a 'q))) (setcdr (cdr l) l) l)",
            "5",
        ],
    );
    for (op, which) in [
        (Op::Memq, Intrinsic::Memq),
        (Op::Assq, Intrinsic::Assq),
        (Op::Member, Intrinsic::Member),
    ] {
        let f = opcode_fn(op.clone(), 2);
        for leaf_knob in LEAF_KNOBS {
            let sites0 = intrinsic_sites_for_test(which);
            let leaf = compile_with(&f, leaf_knob, IntrinsicKnob::ALL);
            assert_eq!(intrinsic_sites_for_test(which), sites0 + 1, "{op:?}");
            let mut inline_answers = 0;
            for swp in [false, true] {
                ev.symbols_with_pos_enabled = swp;
                for &k in &keys {
                    for &l in &lists {
                        let what =
                            format!("({op:?} {} {}) swp={swp} {leaf_knob:?}", show(k), show(l));
                        let want = interpret(&mut ev, &f, vec![k, l]);
                        let calls0 = LIST_SEARCH_SHIM_CALLS.with(|c| c.get())
                            + leaf_trampoline_calls(LeafId::Member) as usize;
                        assert_eq!(native(ctx_ptr, &leaf, &[k, l], &what), want, "{what}");
                        let calls = LIST_SEARCH_SHIM_CALLS.with(|c| c.get())
                            + leaf_trampoline_calls(LeafId::Member) as usize;
                        let called = calls != calls0;
                        // `member`'s table shim (the leaf knob off) counts
                        // nothing: only its leaf call can be seen.
                        if op == Op::Member && !leaf_knob.opcode {
                            continue;
                        }
                        if swp {
                            assert!(called, "{what}: the flag on is the call's");
                        } else if !called {
                            inline_answers += 1;
                        }
                    }
                }
            }
            ev.symbols_with_pos_enabled = false;
            if op != Op::Member || leaf_knob.opcode {
                assert!(inline_answers > 20, "{op:?}: {inline_answers} inline");
            }
        }
    }
}

/// A constant `member` key: an immediate that `equal` does not decide by
/// identity (a character is a fixnum; there is none) cannot occur, so the
/// immediate keys emit the walk with no key test; a heap key (a string, a
/// float) is loaded, so the site tests it at run time and leaves it to the
/// call. Either way the answer is `Bmember`'s.
#[test]
fn member_with_a_constant_key() {
    let mut ev = Context::new();
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let lists = operands(&mut ev, &["'(a \"s\" 1.5 7 k)", "'(1 2)", "nil"]);
    let keys = operands(&mut ev, &["\"s\"", "1.5", "'k", "7"]);
    for &key in &keys {
        let f = lexical_fn(
            1,
            vec![Op::Constant(0), Op::StackRef(1), Op::Member, Op::Return],
            vec![key],
        );
        let sites0 = intrinsic_sites_for_test(Intrinsic::Member);
        let leaf = compile_with(&f, LeafKnob::DEFAULT, IntrinsicKnob::ALL);
        assert_eq!(intrinsic_sites_for_test(Intrinsic::Member) - sites0, 1);
        for &l in &lists {
            let what = format!("(member {} {})", show(key), show(l));
            let want = interpret(&mut ev, &f, vec![l]);
            let calls0 = leaf_trampoline_calls(LeafId::Member);
            assert_eq!(native(ctx_ptr, &leaf, &[l], &what), want, "{what}");
            let called = leaf_trampoline_calls(LeafId::Member) != calls0;
            assert_eq!(
                called,
                key.is_string() || key.is_float(),
                "{what}: a heap key is the call's"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// I6: symbol-value.
// ---------------------------------------------------------------------------

/// `symbol-value` of a dynamic operand over every variable shape, natively
/// against `Bsymbol_value`, under both miss paths. A plain bound non-nil
/// value is answered inline; nil values (the site cannot tell a dedicated
/// buffer-local), void, buffer-local, forwarded and aliased variables,
/// symbols with position and non-symbols reach the call.
#[test]
fn symbol_value_intrinsic_matches_bsymbol_value() {
    let mut ev = Context::new();
    ev.eval_str(
        "(progn
           (defvar leaf-iv-plain 'plain)
           (defvar leaf-iv-nil nil)
           (defvar leaf-iv-local 'local-default)
           (make-variable-buffer-local 'leaf-iv-local)
           (defvaralias 'leaf-iv-alias 'leaf-iv-plain)
           (setq leaf-iv-local 'here))",
    )
    .expect("setup");
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    // (source, answered inline)
    let cases: &[(&str, bool)] = &[
        ("'leaf-iv-plain", true),
        ("'leaf-iv-nil", false),
        ("'leaf-iv-void", false),
        ("'leaf-iv-local", false),
        ("'leaf-iv-alias", false),
        ("'fill-column", false),
        ("'gc-cons-threshold", false),
        ("'buffer-undo-list", false),
        ("t", true),
        ("nil", false),
        ("(position-symbol 'leaf-iv-plain 3)", false),
        ("5", false),
        ("\"leaf-iv-plain\"", false),
    ];
    let sources: Vec<&str> = cases.iter().map(|(s, _)| *s).collect();
    let values = operands(&mut ev, &sources);
    let f = opcode_fn(Op::SymbolValue, 1);
    let with_vars = LeafKnob {
        vars: true,
        ..LeafKnob::DEFAULT
    };
    for leaf_knob in [with_vars, LeafKnob::DEFAULT, LeafKnob::OFF] {
        let sites0 = intrinsic_sites_for_test(Intrinsic::SymbolValue);
        let leaf = compile_with(&f, leaf_knob, IntrinsicKnob::ALL);
        assert_eq!(intrinsic_sites_for_test(Intrinsic::SymbolValue), sites0 + 1);
        for swp in [false, true] {
            ev.symbols_with_pos_enabled = swp;
            for (&v, (src, inline)) in values.iter().zip(cases) {
                let what = format!("(symbol-value {src}) swp={swp} {leaf_knob:?}");
                let want = interpret(&mut ev, &f, vec![v]);
                let calls0 = leaf_trampoline_calls(LeafId::SymbolValue);
                assert_eq!(native(ctx_ptr, &leaf, &[v], &what), want, "{what}");
                if leaf_knob.vars {
                    assert_eq!(
                        leaf_trampoline_calls(LeafId::SymbolValue) - calls0,
                        u64::from(!inline),
                        "{what}: answered inline = {inline}"
                    );
                }
            }
        }
        ev.symbols_with_pos_enabled = false;
    }
}

/// A constant symbol operand folds the cell address; a constant
/// non-symbol emits nothing.
#[test]
fn symbol_value_of_a_constant() {
    let mut ev = Context::new();
    ev.eval_str("(defvar leaf-iv-const 'const-value)")
        .expect("defvar");
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let with_vars = LeafKnob {
        vars: true,
        ..LeafKnob::DEFAULT
    };
    for (constant, emits) in [
        (Value::symbol("leaf-iv-const"), true),
        (Value::symbol("leaf-iv-const-void"), true),
        (Value::fixnum(3), false),
    ] {
        let f = lexical_fn(
            0,
            vec![Op::Constant(0), Op::SymbolValue, Op::Return],
            vec![constant],
        );
        let sites0 = intrinsic_sites_for_test(Intrinsic::SymbolValue);
        let leaf = compile_with(&f, with_vars, IntrinsicKnob::ALL);
        assert_eq!(
            intrinsic_sites_for_test(Intrinsic::SymbolValue) - sites0,
            u64::from(emits)
        );
        let want = interpret(&mut ev, &f, vec![]);
        assert_eq!(native(ctx_ptr, &leaf, &[], "constant"), want);
    }
    // A later `setq` is seen: the cell is read, not the compile-time value.
    let f = lexical_fn(
        0,
        vec![Op::Constant(0), Op::SymbolValue, Op::Return],
        vec![Value::symbol("leaf-iv-const")],
    );
    let leaf = compile_with(&f, with_vars, IntrinsicKnob::ALL);
    ev.eval_str("(setq leaf-iv-const 'changed)").expect("setq");
    assert_eq!(native(ctx_ptr, &leaf, &[], "after setq"), "changed");
}

/// `Op::VarRef` reads through the same cell-read emitter
/// (`emit_symbol_cell_read`, factored out of its lowering) and still answers
/// a plain variable.
#[test]
fn varref_keeps_its_inline_read() {
    let mut ev = Context::new();
    ev.eval_str("(defvar leaf-iv-ref 'ref-value)")
        .expect("defvar");
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    let f = lexical_fn(
        0,
        vec![Op::VarRef(0), Op::Return],
        vec![Value::symbol("leaf-iv-ref")],
    );
    let leaf = compile_with(&f, LeafKnob::DEFAULT, IntrinsicKnob::ALL);
    assert_eq!(native(ctx_ptr, &leaf, &[], "varref"), "ref-value");
}

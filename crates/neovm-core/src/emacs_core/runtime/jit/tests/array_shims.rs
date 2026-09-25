//! `Op::Aref`, `Op::Aset`, `Op::Memq`, `Op::Assq`, `Op::Setcar` and
//! `Op::Setcdr` from compiled code: `neovm_jit_aref`/`_aset`/`_memq`/`_assq`/
//! `_setcar`/`_setcdr` answer the common shapes on a fast path and everything
//! else through the builtin, returning the result's own bits or a
//! `VALUE_SHIM_*` sentinel word. Every case must match the interpreter's
//! opcode arm — result, signal, and what the array looks like afterwards.

use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
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

/// `(lambda (a i) (aref a i))`
fn aref_fn() -> ByteCodeFunction {
    lexical_fn(
        2,
        vec![Op::StackRef(1), Op::StackRef(1), Op::Aref, Op::Return],
        vec![],
    )
}

/// `(lambda (a i v) (aset a i v))`
fn aset_fn() -> ByteCodeFunction {
    lexical_fn(
        3,
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Aset,
            Op::Return,
        ],
        vec![],
    )
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

/// What an array looks like: its printed form, and for a string the storage
/// facts a width-changing store would disturb.
fn describe(value: Value) -> String {
    match value.as_lisp_string() {
        Some(s) => format!(
            "{} multibyte={} chars={} bytes={}",
            print_value(&value),
            s.is_multibyte(),
            s.schars(),
            s.sbytes()
        ),
        None => print_value(&value),
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

const ARRAYS: &[&str] = &[
    "(vector)",
    "(vector 10)",
    "(vector 10 20 30)",
    "(vector 'x 'y)",
    "(record 'foo 1 2)",
    "(make-bool-vector 5 t)",
    "(make-bool-vector 0 nil)",
    "(make-char-table 'foo 7)",
    // The legacy tagged-vector char-table (a vector whose slot 0 is the tag).
    "(let ((v (make-vector 80 nil))) (aset v 0 '--char-table--) (aset v 3 'dflt) v)",
    "(make-string 3 ?a)",
    "(string-to-multibyte (make-string 3 ?a))",
    "(copy-sequence \"aβc\")",
    "(copy-sequence \"βγδ\")",
    "(string-to-unibyte \"a\\377c\")",
    "(copy-sequence \"\")",
    "(make-hash-table)",
    "(symbol-function 'car)",
    "'sym",
    "nil",
    "(cons 1 2)",
    "42",
];

const INDICES: &[&str] = &[
    "-1",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "97",
    "most-positive-fixnum",
    "most-negative-fixnum",
    "(expt 2 70)",
    "1.0",
    "'x",
    "nil",
];

const VALUES: &[&str] = &[
    "0",
    "65",
    "127",
    "128",
    "255",
    "256",
    "955",
    "#x3fff80",
    "#x3fffff",
    "#x400000",
    "-1",
    "'x",
    "nil",
    "t",
    "(cons 1 2)",
    "1.5",
];

/// Every array shape × index (× value) through the compiled site and the
/// interpreter's opcode arm, each on its own fresh array: the same result or
/// signal, and the same array afterwards. No case may deopt.
#[test]
fn array_sites_match_the_interpreter_natively() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let aref = aref_fn();
    let aset = aset_fn();
    let aref_leaf = compile_bytecode_function(&aref).expect("aref compiles");
    let aset_leaf = compile_bytecode_function(&aset).expect("aset compiles");
    let mut checked = 0;
    for array_src in ARRAYS {
        for index_src in INDICES {
            let what = format!("(aref {array_src} {index_src})");
            let fresh = |eval: &mut Context| {
                let pair = eval
                    .eval_str(&format!("(cons {array_src} {index_src})"))
                    .expect("operands");
                vec![pair.cons_car(), pair.cons_cdr()]
            };
            let args = fresh(&mut eval);
            let want = interpret(&mut eval, &aref, args);
            let args = fresh(&mut eval);
            let got = native(ctx_ptr, &aref_leaf, &args, &what);
            assert_eq!(got, want, "{what}");
            checked += 1;

            for value_src in VALUES {
                let what = format!("(aset {array_src} {index_src} {value_src})");
                let fresh = |eval: &mut Context| {
                    let triple = eval
                        .eval_str(&format!("(list {array_src} {index_src} {value_src})"))
                        .expect("operands");
                    let items: Vec<Value> =
                        crate::emacs_core::value::list_to_vec(&triple).expect("list");
                    items
                };
                let args = fresh(&mut eval);
                let array = args[0];
                let want = interpret(&mut eval, &aset, args);
                let want_array = describe(array);
                let args = fresh(&mut eval);
                let array = args[0];
                let got = native(ctx_ptr, &aset_leaf, &args, &what);
                assert_eq!(got, want, "{what}");
                assert_eq!(describe(array), want_array, "{what}: the array afterwards");
                checked += 1;
            }
        }
    }
    assert!(checked > 4000, "checked {checked}");
}

/// The shapes a loop indexes never leave the fast path: a plain vector or
/// record slot, and a character of a unibyte or all-ASCII string.
#[test]
fn indexing_loops_stay_on_the_fast_path() {
    use super::dispatch::ARRAY_SHIM_SLOW_CALLS;
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let aref_leaf = compile_bytecode_function(&aref_fn()).expect("aref compiles");
    let aset_leaf = compile_bytecode_function(&aset_fn()).expect("aset compiles");
    let cases: &[(&str, &str, &str)] = &[
        ("(vector 1 2 3)", "2", "'z"),
        ("(record 'foo 1 2)", "1", "(cons 1 2)"),
        ("(record 'foo 1 2)", "0", "'bar"),
        ("(make-string 4 ?a)", "3", "255"),
        ("(string-to-multibyte (make-string 4 ?a))", "0", "127"),
    ];
    for (array_src, index_src, value_src) in cases {
        let triple = eval
            .eval_str(&format!("(list {array_src} {index_src} {value_src})"))
            .expect("operands");
        let args = crate::emacs_core::value::list_to_vec(&triple).expect("list");
        ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(0));
        let stored = native(ctx_ptr, &aset_leaf, &args, "aset");
        let read = native(ctx_ptr, &aref_leaf, &args[..2], "aref");
        assert_eq!(read, stored, "({array_src}): aref reads what aset stored");
        assert_eq!(
            ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()),
            0,
            "({array_src} {index_src} {value_src}) took the slow path"
        );
    }
    // And a shape the fast path must refuse does reach the builtin.
    let args = [
        eval.eval_str("(make-bool-vector 3 nil)").expect("bv"),
        Value::make_int(1),
        Value::T,
    ];
    ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(0));
    assert_eq!(native(ctx_ptr, &aset_leaf, &args, "bool-vector aset"), "t");
    assert_eq!(
        native(ctx_ptr, &aref_leaf, &args[..2], "bool-vector aref"),
        "t"
    );
    assert_eq!(ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()), 2);
}

/// A redefined `aset` runs, from compiled code, exactly when the interpreter
/// runs it: the shim answers NEED_GENERIC before storing anything.
#[test]
fn a_redefined_aset_runs_from_compiled_code() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let aset = aset_fn();
    let aset_leaf = compile_bytecode_function(&aset).expect("aset compiles");
    eval.eval_str(
        "(progn
           (defvar ashim-orig (symbol-function 'aset))
           (defvar ashim-calls 0)
           (fset 'aset (lambda (a i v)
                         (setq ashim-calls (1+ ashim-calls))
                         (funcall ashim-orig a i (list 'wrapped v)))))",
    )
    .expect("redefine");
    let v1 = eval.eval_str("(vector 0 0)").expect("v");
    let want = interpret(
        &mut eval,
        &aset,
        vec![v1, Value::make_int(1), Value::make_int(5)],
    );
    let v2 = eval.eval_str("(vector 0 0)").expect("v");
    let got = native(
        ctx_ptr,
        &aset_leaf,
        &[v2, Value::make_int(1), Value::make_int(5)],
        "redefined aset",
    );
    assert_eq!(got, want);
    assert_eq!(print_value(&v2), print_value(&v1));
    assert_eq!(print_value(&v2), "[0 (wrapped 5)]");
    assert_eq!(
        print_value(&eval.eval_str("ashim-calls").expect("calls")),
        "2"
    );
    // A signal from the redefinition propagates from the general call.
    eval.eval_str("(fset 'aset (lambda (_a _i _v) (signal 'wrong-type-argument '(no-aset))))")
        .expect("redefine");
    let v3 = eval.eval_str("(vector 0 0)").expect("v");
    assert_eq!(
        native(
            ctx_ptr,
            &aset_leaf,
            &[v3, Value::make_int(0), Value::make_int(1)],
            "signalling aset"
        ),
        "signal wrong-type-argument [\"no-aset\"]"
    );
    eval.eval_str("(fset 'aset ashim-orig)").expect("restore");
}

/// Signals from both shims reach a handler in the same body, natively.
///
///     (lambda (a i) (condition-case err (aref a i) (error (list 'caught err))))
///     (lambda (a i) (condition-case err (aset a i 1) (error (list 'caught err))))
#[test]
fn array_signals_are_caught_by_a_leaf_local_handler() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for (op, extra) in [(Op::Aref, None), (Op::Aset, Some(Op::Constant(1)))] {
        let mut ops = vec![
            Op::PushConditionCase(0), // patched below
            Op::StackRef(1),
            Op::StackRef(1),
        ];
        if let Some(extra) = extra.clone() {
            ops.push(extra);
        }
        ops.push(op.clone());
        ops.push(Op::PopHandler);
        ops.push(Op::Return);
        let handler = ops.len();
        ops[0] = Op::PushConditionCase(handler as u32);
        ops.extend([Op::Constant(0), Op::StackRef(1), Op::List(2), Op::Return]);
        let f = lexical_fn(2, ops, vec![Value::symbol("caught"), Value::make_int(1)]);
        let leaf = compile_bytecode_function(&f).expect("compiles");
        let v = eval.eval_str("(vector 7 8)").expect("v");
        match leaf.call(ctx_ptr, &[v, Value::make_int(9)]) {
            NativeRun::Ok(bits) => assert_eq!(
                print_value(&Value::from_bits(bits)),
                "(caught (args-out-of-range [7 8] 9))",
                "{op:?}"
            ),
            other => panic!("{op:?}: the handler must catch natively, got {other:?}"),
        }
        match leaf.call(ctx_ptr, &[v, Value::make_int(1)]) {
            NativeRun::Ok(bits) => assert_eq!(
                print_value(&Value::from_bits(bits)),
                if extra.is_some() { "1" } else { "8" },
                "{op:?}"
            ),
            other => panic!("{op:?}: in range must run natively, got {other:?}"),
        }
    }
}

/// Live values below an array site survive what its edges can run: a
/// `signal-hook-function` that collects on the signal edge, and a redefined
/// `aset` that collects on the general-call edge.
///
///     (lambda (a i) (let ((h (cons 1 2))) (condition-case nil (OP a i ...) (error h))))
#[test]
fn array_site_edges_keep_the_residual_alive() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for op in [Op::Aref, Op::Aset] {
        let is_aset = matches!(op, Op::Aset);
        let mut ops = vec![
            Op::Constant(0),          // [a i 1]
            Op::Constant(1),          // [a i 1 2]
            Op::Cons,                 // [a i h]
            Op::PushConditionCase(0), // patched
            Op::StackRef(2),          // [a i h a]
            Op::StackRef(2),          // [a i h a i]
        ];
        if is_aset {
            ops.push(Op::Constant(0)); // [a i h a i 1]
        }
        ops.push(op.clone()); // residual [a i h]
        ops.push(Op::Pop); // [a i h]
        ops.push(Op::PopHandler);
        ops.push(Op::Return); // h
        let handler = ops.len();
        ops[3] = Op::PushConditionCase(handler as u32);
        ops.extend([Op::Pop, Op::Return]); // [a i h err] -> h
        let f = lexical_fn(2, ops, vec![Value::make_int(1), Value::make_int(2)]);
        let leaf = compile_bytecode_function(&f).expect("compiles");
        eval.eval_str(
            "(setq signal-hook-function
                   (lambda (_sym _data) (garbage-collect) (make-list 4096 (cons 0 0)) nil))",
        )
        .expect("hook");
        let check = |bits: usize, what: &str| {
            let h = Value::from_bits(bits);
            assert!(h.is_cons(), "{what}: h survived (got {h:?})");
            assert_eq!(h.cons_car(), Value::make_int(1), "{what}: car intact");
            assert_eq!(h.cons_cdr(), Value::make_int(2), "{what}: cdr intact");
        };
        for _ in 0..3 {
            let v = eval.eval_str("(vector 0 0)").expect("v");
            match leaf.call(ctx_ptr, &[v, Value::make_int(5)]) {
                NativeRun::Ok(bits) => check(bits, &format!("{op:?} signal edge")),
                other => panic!("{op:?}: must catch natively, got {other:?}"),
            }
        }
        eval.eval_str("(setq signal-hook-function nil)")
            .expect("unhook");
        if is_aset {
            eval.eval_str(
                "(progn
                   (defvar ashim-orig2 (symbol-function 'aset))
                   (fset 'aset (lambda (a i v)
                                 (garbage-collect)
                                 (make-list 4096 (cons 0 0))
                                 (funcall ashim-orig2 a i v))))",
            )
            .expect("redefine");
            for _ in 0..3 {
                let v = eval.eval_str("(vector 0 0)").expect("v");
                match leaf.call(ctx_ptr, &[v, Value::make_int(1)]) {
                    NativeRun::Ok(bits) => check(bits, "Aset general-call edge"),
                    other => panic!("general call must run natively, got {other:?}"),
                }
                assert_eq!(print_value(&v), "[0 1]");
            }
            eval.eval_str("(fset 'aset ashim-orig2)").expect("restore");
        }
    }
}

/// `(lambda (x l) (OP x l))`
fn list_fn(op: Op) -> ByteCodeFunction {
    lexical_fn(
        2,
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return],
        vec![],
    )
}

const LISTS: &[&str] = &[
    "nil",
    "'(1 2 3)",
    "'(a b a)",
    "'(1 2 . 3)",
    "'(9 . 3)",
    "(let ((l (list 1 2 3))) (setcdr (nthcdr 2 l) l) l)",
    "(let ((l (list (cons 'a 1) (cons 'b 2)))) (setcdr (cdr l) l) l)",
    "(let ((l nil) (i 100)) (while (> i 0) (setq i (1- i) l (cons i l))) l)",
    "(let ((l nil) (i 100)) (while (> i 0) (setq i (1- i) l (cons (cons i i) l))) l)",
    "'((a . 1) (b . 2) (a . 3))",
    "'(a (b . 2) nil (c))",
    "'((a . 1) . tail)",
    "'((a . 1) (b . 2) . tail)",
    "5",
    "\"str\"",
];

const ELTS: &[&str] = &[
    "1", "3", "'a", "'b", "'c", "70", "99", "'zz", "nil", "'tail",
];

/// `memq` and `assq` on proper, improper, circular, short and long lists
/// through the compiled site and the interpreter's opcode arm.
#[test]
fn list_sites_match_the_interpreter_natively() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let mut checked = 0;
    for op in [Op::Memq, Op::Assq] {
        let f = list_fn(op.clone());
        let leaf = compile_bytecode_function(&f).expect("compiles");
        for list_src in LISTS {
            for elt_src in ELTS {
                let what = format!("({op:?} {elt_src} {list_src})");
                let pair = eval
                    .eval_str(&format!("(cons {elt_src} {list_src})"))
                    .expect("operands");
                let args = vec![pair.cons_car(), pair.cons_cdr()];
                let want = interpret(&mut eval, &f, args.clone());
                let got = native(ctx_ptr, &leaf, &args, &what);
                assert_eq!(got, want, "{what}");
                checked += 1;
            }
        }
    }
    assert!(checked >= 300, "checked {checked}");
}

/// Short lists stay on the fast path; a long list or
/// `symbols-with-pos-enabled` reach the builtin, which still answers.
#[test]
fn short_list_lookups_stay_on_the_fast_path() {
    use super::dispatch::ARRAY_SHIM_SLOW_CALLS;
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let memq = compile_bytecode_function(&list_fn(Op::Memq)).expect("memq compiles");
    let assq = compile_bytecode_function(&list_fn(Op::Assq)).expect("assq compiles");
    let short = eval.eval_str("'(x y z)").expect("list");
    let alist = eval.eval_str("'((x . 1) (y . 2))").expect("alist");
    ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(0));
    assert_eq!(
        native(ctx_ptr, &memq, &[Value::symbol("y"), short], "memq"),
        "(y z)"
    );
    assert_eq!(
        native(ctx_ptr, &memq, &[Value::symbol("w"), short], "memq"),
        "nil"
    );
    assert_eq!(
        native(ctx_ptr, &assq, &[Value::symbol("y"), alist], "assq"),
        "(y . 2)"
    );
    assert_eq!(
        native(ctx_ptr, &assq, &[Value::symbol("w"), alist], "assq"),
        "nil"
    );
    assert_eq!(ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()), 0, "short lists");
    let long = eval
        .eval_str("(let ((l nil) (i 200)) (while (> i 0) (setq i (1- i) l (cons i l))) l)")
        .expect("long");
    assert_eq!(
        native(ctx_ptr, &memq, &[Value::make_int(198), long], "long memq"),
        "(198 199)"
    );
    assert_eq!(ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()), 1, "a long list");
    let positioned = eval
        .eval_str("(position-symbol 'y 3)")
        .expect("symbol with pos");
    assert_eq!(
        native(ctx_ptr, &memq, &[positioned, short], "memq, positions off"),
        "nil"
    );
    eval.eval_str("(setq symbols-with-pos-enabled t)")
        .expect("swp");
    ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(0));
    assert_eq!(
        native(ctx_ptr, &memq, &[Value::symbol("y"), short], "swp memq"),
        "(y z)"
    );
    assert_eq!(
        ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()),
        1,
        "symbols-with-pos"
    );
    // With positions enabled `eq` looks through them, and so must the site.
    assert_eq!(
        native(ctx_ptr, &memq, &[positioned, short], "memq, positions on"),
        "(y z)"
    );
    assert_eq!(
        native(ctx_ptr, &assq, &[positioned, alist], "assq, positions on"),
        "(y . 2)"
    );
    eval.eval_str("(setq symbols-with-pos-enabled nil)")
        .expect("swp off");
}

/// `type-of` of a record, the answer the JIT's predicate intrinsic now gives
/// without the builtin: the type slot, or an EIEIO class record's name.
#[test]
fn record_type_of_is_gnus_record_arm() {
    use crate::emacs_core::builtins::types::record_type_of;
    let mut eval = Context::new();
    for (src, want) in [
        ("(record 'foo 1 2)", Some("foo")),
        (
            "(record (record 'eieio--class 'my-class) 2)",
            Some("my-class"),
        ),
        ("(record (record 'lonely) 2)", Some("#s(lonely)")),
        ("(record 7)", Some("7")),
        ("(vector 'foo 1)", None),
        ("'foo", None),
        ("(make-bool-vector 3 t)", None),
    ] {
        let value = eval.eval_str(src).expect("value");
        assert_eq!(
            record_type_of(value).map(|v| print_value(&v)).as_deref(),
            want,
            "{src}"
        );
    }
}

/// The general `aset` call's store record must MEET the fast path's (which
/// stores nothing): the call site after it roots `h` itself. Were the
/// continuation to inherit the general call's record, that later call would
/// skip storing `h`, and on the fast path — the only one taken here — `h`
/// would be unrooted across a collection.
///
///     (lambda (v) (let ((h (cons 1 2))) (aset v 0 7) (collect-and-allocate) h))
#[test]
fn a_later_call_site_roots_what_only_the_aset_fallback_stored() {
    // A one-call body is below the profitability gate; this test is about
    // the lowering, not the gate.
    super::force_profit_gate_for_test(false);
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    eval.eval_str(
        "(fset 'ashim-collect (lambda () (garbage-collect) (make-list 4096 (cons 0 0)) nil))",
    )
    .expect("collector");
    let f = lexical_fn(
        1,
        vec![
            Op::Constant(0), // [v 1]
            Op::Constant(1), // [v 1 2]
            Op::Cons,        // [v h]
            Op::StackRef(1), // [v h v]
            Op::Constant(2), // [v h v 0]
            Op::Constant(3), // [v h v 0 7]
            Op::Aset,        // [v h r]   residual [v h]
            Op::Pop,         // [v h]
            Op::Constant(4), // [v h f]
            Op::Call(0),     // [v h r]   residual [v h]
            Op::Pop,         // [v h]
            Op::Return,      // h
        ],
        vec![
            Value::make_int(1),
            Value::make_int(2),
            Value::make_int(0),
            Value::make_int(7),
            Value::symbol("ashim-collect"),
        ],
    );
    let leaf = compile_bytecode_function(&f).expect("compiles");
    for _ in 0..3 {
        let v = eval.eval_str("(vector 0 0)").expect("v");
        match leaf.call(ctx_ptr, &[v]) {
            NativeRun::Ok(bits) => {
                let h = Value::from_bits(bits);
                assert!(h.is_cons(), "h survived the collection (got {h:?})");
                assert_eq!(h.cons_car(), Value::make_int(1), "car intact");
                assert_eq!(h.cons_cdr(), Value::make_int(2), "cdr intact");
            }
            other => panic!("must run natively, got {other:?}"),
        }
        assert_eq!(print_value(&v), "[7 0]");
    }
}

/// The aset shim asks the obarray whether `aset` is still the builtin once per
/// function epoch. Warmed up on the builtin, a compiled `aset` must still see
/// a redefinition made afterwards, and the builtin again once it is restored.
#[test]
fn a_warmed_aset_sees_a_later_redefinition_and_its_undoing() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let aset_leaf = compile_bytecode_function(&aset_fn()).expect("aset compiles");
    let v = eval.eval_str("(vector 0 0)").expect("v");
    let run = |v: Value, value: i64| {
        native(
            ctx_ptr,
            &aset_leaf,
            &[v, Value::make_int(1), Value::make_int(value)],
            "aset",
        )
    };
    assert_eq!(run(v, 1), "1");
    assert_eq!(run(v, 2), "2");
    assert_eq!(print_value(&v), "[0 2]");
    eval.eval_str(
        "(progn
           (defvar ashim-warm-orig (symbol-function 'aset))
           (fset 'aset (lambda (a i v) (funcall ashim-warm-orig a i (list 'wrapped v)))))",
    )
    .expect("redefine");
    assert_eq!(run(v, 3), "(wrapped 3)");
    assert_eq!(print_value(&v), "[0 (wrapped 3)]");
    eval.eval_str("(fset 'aset ashim-warm-orig)")
        .expect("restore");
    assert_eq!(run(v, 4), "4");
    assert_eq!(print_value(&v), "[0 4]");
}

/// `(lambda (n l) (nth n l))`
fn nth_fn() -> ByteCodeFunction {
    lexical_fn(
        2,
        vec![Op::StackRef(1), Op::StackRef(1), Op::Nth, Op::Return],
        vec![],
    )
}

/// Byte-code `nth` is GNU `Bnth`: a count of 0..127 walks the list inline
/// and signals with the non-list TAIL it stops at, where the `nth` function
/// signals with the whole list. The interpreter's opcode and compiled code
/// agree, and match GNU Emacs 31.0.90 byte-compiled results.
#[test]
fn byte_code_nth_reports_the_tail_it_stops_at() {
    crate::emacs_core::jit::compile::force_profit_gate_for_test(false);
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let f = nth_fn();
    let leaf = compile_bytecode_function(&f).expect("nth compiles");
    let cases = [
        ("0", "'(a b)", "a"),
        ("1", "'(a b)", "b"),
        ("2", "'(a b)", "nil"),
        ("-1", "'(a b)", "a"),
        ("127", "nil", "nil"),
        ("128", "'(a)", "nil"),
        (
            "2",
            "'(a . b)",
            "signal wrong-type-argument [\"listp\", \"b\"]",
        ),
        (
            "3",
            "'(a b . c)",
            "signal wrong-type-argument [\"listp\", \"c\"]",
        ),
        ("1", "'x", "signal wrong-type-argument [\"listp\", \"x\"]"),
        (
            "200",
            "'(a . b)",
            "signal wrong-type-argument [\"listp\", \"(a . b)\"]",
        ),
        (
            "'z",
            "'(a)",
            "signal wrong-type-argument [\"integerp\", \"z\"]",
        ),
        ("(expt 2 70)", "'(a b)", "nil"),
    ];
    for (n, list, want) in cases {
        let n_value = eval.eval_str(n).expect("n");
        let list_value = eval.eval_str(list).expect("list");
        let interpreted = interpret(&mut eval, &f, vec![n_value, list_value]);
        let compiled = native(ctx_ptr, &leaf, &[n_value, list_value], "nth");
        assert_eq!(interpreted, want, "interpreted (nth {n} {list})");
        assert_eq!(compiled, want, "compiled (nth {n} {list})");
    }
    // The function keeps `Fnth`'s whole-list error.
    assert_eq!(
        print_value(
            &eval
                .eval_str("(condition-case e (nth 2 '(a . b)) (error e))")
                .expect("nth function")
        ),
        "(wrong-type-argument listp (a . b))"
    );
}

/// Compiled `aref` reads a plain vector's or record's slot inline, calling
/// `neovm_jit_aref` for every other shape: a tagged char-table or
/// bool-vector vector, a string, a bool-vector, an out-of-range or
/// non-fixnum index. Answers match the interpreter either way.
#[test]
fn compiled_aref_reads_plain_vectors_and_records_inline() {
    assert!(
        crate::tagged::header::LispValueVec::jit_slice_offsets().is_some(),
        "owned and mapped vectors keep pointer and length at shared offsets"
    );
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let f = aref_fn();
    // The vector path alone: `NEOVM_JIT_LEAF=string` would inline the
    // string case too (`string_intrinsics_stay_off_the_shims`).
    force_leaf_knob_for_test(Some(LeafKnob::OFF));
    let leaf = compile_bytecode_function(&f).expect("aref compiles");
    force_leaf_knob_for_test(None);
    let cases: &[(&str, &str, bool)] = &[
        ("(vector 10 20 30)", "1", true),
        ("(vector 'x)", "0", true),
        ("(record 'foo 1 2)", "2", true),
        (
            "(let ((v (make-vector 3 nil))) (aset v 0 '--char-table--) v)",
            "0",
            true,
        ),
        ("(vector 10 20 30)", "3", false),
        ("(vector 10 20 30)", "-1", false),
        ("(vector 10 20 30)", "'x", false),
        ("(vector)", "0", false),
        (
            "(let ((v (make-vector 80 nil))) (aset v 0 '--char-table--) (aset v 3 'dflt) v)",
            "3",
            false,
        ),
        (
            "(let ((v (make-vector 3 nil))) (aset v 0 '--bool-vector--) v)",
            "1",
            false,
        ),
        (
            "(let ((v (make-vector 1 nil))) (aset v 0 '--bool-vector--) v)",
            "0",
            true,
        ),
        ("(make-string 3 ?a)", "1", false),
        ("(make-bool-vector 5 t)", "1", false),
    ];
    for &(array, index, inline) in cases {
        // One evaluation, so the array is never unrooted across a safe point.
        let pair = eval
            .eval_str(&format!("(cons {array} {index})"))
            .expect("operands");
        let (array_value, index_value) = (pair.cons_car(), pair.cons_cdr());
        let want = interpret(&mut eval, &f, vec![array_value, index_value]);
        let before = super::dispatch::AREF_SHIM_CALLS.with(|c| c.get());
        let got = native(ctx_ptr, &leaf, &[array_value, index_value], "aref");
        let calls = super::dispatch::AREF_SHIM_CALLS.with(|c| c.get()) - before;
        assert_eq!(got, want, "(aref {array} {index})");
        assert_eq!(
            calls == 0,
            inline,
            "(aref {array} {index}) inline: {inline}"
        );
    }
}

/// `setcar` and `setcdr` through the compiled site and the interpreter's
/// opcode arm, each on its own fresh cell: the same result or signal, and
/// the same cell afterwards. A cons stores on the shim's fast path; anything
/// else reaches the builtin, which signals.
#[test]
fn list_stores_match_the_interpreter_natively() {
    use super::dispatch::ARRAY_SHIM_SLOW_CALLS;
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let cells: &[(&str, bool)] = &[
        ("(cons 1 2)", true),
        ("(list 'a 'b 'c)", true),
        ("(nthcdr 2 (list 1 2 3))", true),
        ("nil", false),
        ("'sym", false),
        ("5", false),
        ("(vector 1 2)", false),
        ("\"str\"", false),
        ("(position-symbol 'y 3)", false),
    ];
    for op in [Op::Setcar, Op::Setcdr] {
        let f = list_fn(op.clone());
        let leaf = compile_bytecode_function(&f).expect("compiles");
        for &(cell_src, is_cons) in cells {
            for value_src in VALUES {
                let what = format!("({op:?} {cell_src} {value_src})");
                // One evaluation: a cell held only in a Rust local would be
                // unrooted across the value's safe point.
                let fresh = |eval: &mut Context| {
                    let pair = eval
                        .eval_str(&format!("(cons {cell_src} {value_src})"))
                        .expect("operands");
                    (pair.cons_car(), pair.cons_cdr())
                };
                let (cell, value) = fresh(&mut eval);
                let want = interpret(&mut eval, &f, vec![cell, value]);
                let want_after = describe(cell);
                let (cell, value) = fresh(&mut eval);
                ARRAY_SHIM_SLOW_CALLS.with(|c| c.set(0));
                let got = native(ctx_ptr, &leaf, &[cell, value], &what);
                assert_eq!(got, want, "{what}");
                assert_eq!(describe(cell), want_after, "{what}: the cell afterwards");
                assert_eq!(
                    ARRAY_SHIM_SLOW_CALLS.with(|c| c.get()),
                    usize::from(!is_cons),
                    "{what}: only a non-cons leaves the fast path"
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// String intrinsics (I1/I2, `NEOVM_JIT_LEAF=string`).
// ---------------------------------------------------------------------------

fn compile_string_knob(f: &ByteCodeFunction) -> CompiledLeaf {
    force_leaf_knob_for_test(Some(LeafKnob {
        string: true,
        ..LeafKnob::OFF
    }));
    let leaf = compile_bytecode_function(f).expect("compiles");
    force_leaf_knob_for_test(None);
    leaf
}

/// The whole array × index (× value) matrix with the string intrinsics
/// emitted: the same result or signal, and the same array afterwards, as
/// the interpreter's opcode arm. Both intrinsics were emitted.
#[test]
fn string_intrinsics_match_the_interpreter_natively() {
    #[cfg(debug_assertions)]
    use super::lowering::{STRING_AREF_INLINE_EMITTED, STRING_ASET_INLINE_EMITTED};
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let aref = aref_fn();
    let aset = aset_fn();
    #[cfg(debug_assertions)]
    let (aref0, aset0) = (
        STRING_AREF_INLINE_EMITTED.load(Ordering::Relaxed),
        STRING_ASET_INLINE_EMITTED.load(Ordering::Relaxed),
    );
    let aref_leaf = compile_string_knob(&aref);
    let aset_leaf = compile_string_knob(&aset);
    #[cfg(debug_assertions)]
    {
        assert!(STRING_AREF_INLINE_EMITTED.load(Ordering::Relaxed) > aref0);
        assert!(STRING_ASET_INLINE_EMITTED.load(Ordering::Relaxed) > aset0);
    }
    let mut checked = 0;
    for array_src in ARRAYS {
        for index_src in INDICES {
            let what = format!("(aref {array_src} {index_src})");
            let fresh = |eval: &mut Context| {
                let pair = eval
                    .eval_str(&format!("(cons {array_src} {index_src})"))
                    .expect("operands");
                vec![pair.cons_car(), pair.cons_cdr()]
            };
            let args = fresh(&mut eval);
            let want = interpret(&mut eval, &aref, args);
            let args = fresh(&mut eval);
            assert_eq!(native(ctx_ptr, &aref_leaf, &args, &what), want, "{what}");
            for value_src in VALUES {
                let what = format!("(aset {array_src} {index_src} {value_src})");
                let fresh = |eval: &mut Context| {
                    let triple = eval
                        .eval_str(&format!("(list {array_src} {index_src} {value_src})"))
                        .expect("operands");
                    crate::emacs_core::value::list_to_vec(&triple).expect("list")
                };
                let args = fresh(&mut eval);
                let array = args[0];
                let want = interpret(&mut eval, &aset, args);
                let want_array = describe(array);
                let args = fresh(&mut eval);
                let array = args[0];
                assert_eq!(native(ctx_ptr, &aset_leaf, &args, &what), want, "{what}");
                assert_eq!(describe(array), want_array, "{what}: the array afterwards");
                checked += 1;
            }
        }
    }
    assert!(checked > 4000, "checked {checked}");
}

/// The shapes a string loop takes never reach a shim: reading and storing
/// bytes of a unibyte string and ASCII of an all-ASCII multibyte one.
/// Everything else does -- a width change, a non-ASCII string, a bad index.
#[test]
fn string_intrinsics_stay_off_the_shims() {
    use super::dispatch::{AREF_SHIM_CALLS, ASET_SHIM_CALLS};
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let aref_leaf = compile_string_knob(&aref_fn());
    let aset_leaf = compile_string_knob(&aset_fn());
    // The `aset` gate is armed per function epoch by the shim's first call
    // (`aset_regate`), as for the vector fast path.
    let warm = eval.eval_str("(make-string 1 ?a)").expect("warm-up string");
    crate::emacs_core::eval::push_scratch_gc_root(warm);
    let _ = native(
        ctx_ptr,
        &aset_leaf,
        &[warm, Value::fixnum(0), Value::fixnum(98)],
        "arm",
    );
    let inline_cases: &[(&str, i64, i64)] = &[
        ("(make-string 4 ?a)", 3, 255),
        ("(make-string 4 ?a)", 0, 0),
        ("(string-to-unibyte \"a\\377c\")", 1, 7),
        ("(string-to-multibyte (make-string 4 ?a))", 2, 127),
    ];
    for &(src, index, code) in inline_cases {
        let s = eval.eval_str(src).expect("string");
        crate::emacs_core::eval::push_scratch_gc_root(s);
        let (i, v) = (Value::fixnum(index), Value::fixnum(code));
        AREF_SHIM_CALLS.with(|c| c.set(0));
        ASET_SHIM_CALLS.with(|c| c.set(0));
        assert_eq!(
            native(ctx_ptr, &aset_leaf, &[s, i, v], "aset"),
            code.to_string()
        );
        assert_eq!(
            native(ctx_ptr, &aref_leaf, &[s, i], "aref"),
            code.to_string()
        );
        assert_eq!(AREF_SHIM_CALLS.with(|c| c.get()), 0, "{src}: aref inline");
        assert_eq!(ASET_SHIM_CALLS.with(|c| c.get()), 0, "{src}: aset inline");
    }
    let shim_cases: &[(&str, i64, i64)] = &[
        // A width change and a non-ASCII string.
        ("(string-to-multibyte (make-string 4 ?a))", 1, 200),
        ("(copy-sequence \"a\u{3b2}c\")", 0, 65),
        // Out of range.
        ("(make-string 4 ?a)", 4, 65),
        ("(make-string 4 ?a)", -1, 65),
        ("(make-string 4 ?a)", 0, 256),
    ];
    for &(src, index, code) in shim_cases {
        let s = eval.eval_str(src).expect("string");
        crate::emacs_core::eval::push_scratch_gc_root(s);
        let (i, v) = (Value::fixnum(index), Value::fixnum(code));
        ASET_SHIM_CALLS.with(|c| c.set(0));
        let _ = native(ctx_ptr, &aset_leaf, &[s, i, v], "aset");
        assert_eq!(
            ASET_SHIM_CALLS.with(|c| c.get()),
            1,
            "{src} {index} {code}: aset shim"
        );
    }
    let s = eval
        .eval_str("(copy-sequence \"a\u{3b2}c\")")
        .expect("string");
    AREF_SHIM_CALLS.with(|c| c.set(0));
    assert_eq!(
        native(ctx_ptr, &aref_leaf, &[s, Value::fixnum(1)], "aref"),
        "946"
    );
    assert_eq!(
        AREF_SHIM_CALLS.with(|c| c.get()),
        1,
        "a non-ASCII string: aref shim"
    );
}

/// A redefined `aset` runs from an inline string site exactly when the
/// interpreter runs it: the inline path checks the shim's own gate.
#[test]
fn a_redefined_aset_runs_from_an_inline_string_site() {
    use super::dispatch::ASET_SHIM_CALLS;
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let aset = aset_fn();
    let aset_leaf = compile_string_knob(&aset);
    let s = eval.eval_str("(make-string 2 ?a)").expect("s");
    crate::emacs_core::eval::push_scratch_gc_root(s);
    assert_eq!(
        native(
            ctx_ptr,
            &aset_leaf,
            &[s, Value::fixnum(0), Value::fixnum(66)],
            "aset"
        ),
        "66"
    );
    eval.eval_str(
        "(progn
           (defvar strshim-orig (symbol-function 'aset))
           (defvar strshim-calls 0)
           (fset 'aset (lambda (a i v)
                         (setq strshim-calls (1+ strshim-calls))
                         (funcall strshim-orig a i (1+ v)))))",
    )
    .expect("redefine");
    ASET_SHIM_CALLS.with(|c| c.set(0));
    assert_eq!(
        native(
            ctx_ptr,
            &aset_leaf,
            &[s, Value::fixnum(1), Value::fixnum(66)],
            "aset"
        ),
        "67"
    );
    assert_eq!(
        ASET_SHIM_CALLS.with(|c| c.get()),
        1,
        "the gate sent it to the shim"
    );
    assert_eq!(print_value(&s), "\"BC\"");
    assert_eq!(
        print_value(&eval.eval_str("strshim-calls").expect("calls")),
        "1"
    );
    eval.eval_str("(fset 'aset strshim-orig)").expect("restore");
}

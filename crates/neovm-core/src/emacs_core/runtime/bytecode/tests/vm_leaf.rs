//! Tier-0 `Bcall` of leaf builtins (`NEOVM_VM_LEAF`): every call answers
//! exactly as the builtin protocol does -- values, signals with their data
//! and the backtrace frame the signal machinery sees, declined shapes,
//! redefinitions -- and the tests prove the leaf ran (a correctness
//! assertion alone passes just as well when it did not).

use super::*;
use crate::emacs_core::error::{FlowKind, FlowResultExt};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

/// `(lambda (a1..an) (CALLEE a1..an))` through `Op::Call` (GNU `Bcall`).
fn bcall_fn(callee: &str, nargs: usize) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs as u32).map(SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
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

fn run(ev: &mut Context, f: &ByteCodeFunction, args: &[Value]) -> String {
    match Vm::from_context(ev).execute(f, args.to_vec()).kinded() {
        Ok(v) => print_value(&v),
        Err(FlowKind::Signal(sig)) => format!(
            "signal {} {:?}",
            sig.symbol_name(),
            sig.data.iter().map(print_value).collect::<Vec<_>>()
        ),
        Err(other) => format!("{other:?}"),
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

/// Switch the knob and drop what the context's call cache resolved under
/// the old setting.
fn set_knob(ev: &mut Context, knob: VmLeafKnob) {
    super::vm_leaf::force_vm_leaf_knob_for_test(Some(knob));
    ev.symbol_bytecode_call_cache = SymbolByteCodeCallCache::new();
}

#[test]
fn vm_leaf_knob_parses() {
    assert_eq!(VmLeafKnob::parse(None), VmLeafKnob::OFF);
    for off in ["", "0", "off", "false", "no", "bogus"] {
        assert_eq!(VmLeafKnob::parse(Some(off)), VmLeafKnob::OFF, "{off}");
    }
    for on in ["1", "on", "all", "true", "yes", "bcall"] {
        assert_eq!(VmLeafKnob::parse(Some(on)), VmLeafKnob::ALL, "{on}");
    }
}

/// Every Bcall leaf at every arity it can be called with, over values,
/// signals and the declined shapes (a user-test hash table, a `plist-get`
/// PREDICATE), with the knob on against the knob off; with it on, every
/// call in the leaf's arity ran the leaf.
#[test]
fn bcall_leaf_calls_match_the_builtin_protocol() {
    let mut ev = Context::new();
    ev.eval_str(
        "(progn
           (define-hash-table-test 'vm-leaf-test
             #'(lambda (x y) (equal x y)) #'(lambda (k) (sxhash-equal k)))
           (defvar vm-leaf-plain 'plain)
           (defvar vm-leaf-local 'local-default)
           (make-variable-buffer-local 'vm-leaf-local)
           (set-buffer (get-buffer-create \" vm-leaf\"))
           (setq vm-leaf-local 'here)
           (insert \"hello world\")
           (put-text-property 3 5 'p 'text)
           (overlay-put (make-overlay 4 7) 'p 'overlay))",
    )
    .expect("setup");
    /// `(builtin, arities, first args, second args, third args)`.
    type Case<'a> = (
        &'a str,
        &'a [usize],
        &'a [&'a str],
        &'a [&'a str],
        &'a [&'a str],
    );
    let cases: &[Case] = &[
        (
            "gethash",
            &[1, 2, 3, 4],
            &["7", "'a", "\"s\"", "nil"],
            &[
                "(let ((h (make-hash-table))) (puthash 7 'seven h) h)",
                "(let ((h (make-hash-table :test 'equal))) (puthash \"s\" 3 h) h)",
                "(let ((h (make-hash-table :test 'vm-leaf-test))) (puthash \"s\" 4 h) h)",
                "5",
            ],
            &["nil", "'dflt"],
        ),
        (
            "plist-get",
            &[2, 3],
            &["'(a 1 b 2)", "'(a . 1)", "nil", "5"],
            &["'a", "'b", "'z"],
            &["nil", "#'eq"],
        ),
        (
            "get-char-property",
            &[2, 3],
            &["1", "3", "5", "12", "0", "'x"],
            &["'p", "'q"],
            &["nil", "(current-buffer)", "(propertize \"abc\" 'p 'str)"],
        ),
        (
            "assoc",
            &[2, 3],
            &["'b", "\"s\"", "'z", "1.5"],
            &["'((a . 1) (b . 2) (\"s\" . 3) (1.5 . f))", "nil", "5"],
            &["nil", "#'eq"],
        ),
        (
            "rassq",
            &[2],
            &["2", "'z"],
            &["'((a . 1) (b . 2))", "5"],
            &[],
        ),
        (
            "boundp",
            &[1],
            &["'vm-leaf-plain", "'vm-leaf-void", "nil", "5"],
            &[],
            &[],
        ),
        ("keywordp", &[1], &[":kw", "'a", "5"], &[], &[]),
        (
            "symbol-name",
            &[1],
            &["'vm-leaf-plain", "nil", "5"],
            &[],
            &[],
        ),
        (
            "buffer-local-value",
            &[2],
            &[
                "'vm-leaf-local",
                "'vm-leaf-plain",
                "'vm-leaf-void",
                "'fill-column",
                "5",
            ],
            &[
                "(current-buffer)",
                "(get-buffer-create \" vm-leaf-other\")",
                "nil",
            ],
            &[],
        ),
    ];
    for (name, arities, firsts, seconds, thirds) in cases {
        let firsts = operands(&mut ev, firsts);
        let seconds = operands(&mut ev, seconds);
        let thirds = operands(&mut ev, thirds);
        let leaf = crate::emacs_core::subr::leaf::subr_leaf(intern(name)).expect("a Bcall leaf");
        for &nargs in *arities {
            let f = bcall_fn(name, nargs);
            let mut calls: Vec<Vec<Value>> = Vec::new();
            let seconds = if seconds.is_empty() {
                vec![Value::NIL]
            } else {
                seconds.clone()
            };
            for &a in &firsts {
                for &b in &seconds {
                    let tail: Vec<Value> = if thirds.is_empty() {
                        vec![Value::NIL]
                    } else {
                        thirds.clone()
                    };
                    for &c in &tail {
                        let args = [a, b, c, Value::NIL];
                        calls.push(args[..nargs].to_vec());
                    }
                }
            }
            calls.dedup();
            set_knob(&mut ev, VmLeafKnob::OFF);
            let want: Vec<String> = calls.iter().map(|args| run(&mut ev, &f, args)).collect();
            set_knob(&mut ev, VmLeafKnob::ALL);
            let runs0 = super::vm_leaf::vm_leaf_calls_for_test();
            let got: Vec<String> = calls.iter().map(|args| run(&mut ev, &f, args)).collect();
            assert_eq!(got, want, "({name} ..{nargs})");
            let min_args = crate::emacs_core::eval::lookup_global_subr_entry(intern(name))
                .expect("registered")
                .min_args;
            let in_arity = nargs <= leaf.entry_slots() && nargs >= usize::from(min_args);
            assert_eq!(
                super::vm_leaf::vm_leaf_calls_for_test() - runs0,
                if in_arity { calls.len() as u64 } else { 0 },
                "({name} ..{nargs}): the leaf ran exactly for the calls in its arity"
            );
        }
    }
    super::vm_leaf::force_vm_leaf_knob_for_test(None);
}

/// A signal hook sees GNU `Bcall`'s frame under a leaf that signalled --
/// `(gethash 1 5)`, pushed lazily with the call's own arguments -- exactly
/// as under the builtin path, and the specpdl is balanced afterwards.
#[test]
fn a_leaf_signal_runs_under_the_builtins_frame() {
    let mut ev = Context::new();
    ev.eval_str(
        "(setq vm-leaf-seen nil
               signal-hook-function
               (lambda (_sym _data)
                 (setq vm-leaf-seen 'no-frame)
                 (mapbacktrace
                   (lambda (_evald func args _flags)
                     (if (eq func 'gethash)
                         (setq vm-leaf-seen (cons 'frame args)))))))",
    )
    .expect("probe");
    let f = bcall_fn("gethash", 2);
    let args = [Value::fixnum(1), Value::fixnum(5)];
    let mut seen = Vec::new();
    for knob in [VmLeafKnob::OFF, VmLeafKnob::ALL] {
        set_knob(&mut ev, knob);
        let specpdl = ev.specpdl.len();
        let runs0 = super::vm_leaf::vm_leaf_calls_for_test();
        let result = run(&mut ev, &f, &args);
        assert_eq!(
            result,
            "signal wrong-type-argument [\"hash-table-p\", \"5\"]"
        );
        assert_eq!(ev.specpdl.len(), specpdl, "{knob:?}: balanced");
        assert_eq!(
            super::vm_leaf::vm_leaf_calls_for_test() - runs0,
            u64::from(knob.bcall)
        );
        seen.push(print_value(&ev.eval_str("vm-leaf-seen").expect("probe")));
    }
    assert_eq!(seen[0], seen[1], "the hook saw the same frame");
    assert!(seen[1].starts_with("(frame 1 5"), "{}", seen[1]);
    ev.eval_str("(setq signal-hook-function nil)")
        .expect("unhook");
    super::vm_leaf::force_vm_leaf_knob_for_test(None);
}

/// A handler in the calling body catches a leaf's signal with the
/// builtin's data, and a successful call leaves no frame behind: a
/// `backtrace-frames` taken inside the next call's callee cannot see it.
///
///     (lambda (k h) (condition-case err (gethash k h) (error (list 'caught err))))
#[test]
fn a_leaf_signal_is_caught_in_the_calling_body() {
    let mut ev = Context::new();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1), SymId(2)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    let mut ops = vec![
        Op::PushConditionCase(0),
        Op::Constant(1),
        Op::StackRef(2),
        Op::StackRef(2),
        Op::Call(2),
        Op::PopHandler,
        Op::Return,
    ];
    let handler = ops.len();
    ops[0] = Op::PushConditionCase(handler as u32);
    ops.extend([Op::Constant(0), Op::StackRef(1), Op::List(2), Op::Return]);
    f.ops = ops;
    f.constants = vec![Value::symbol("caught"), Value::symbol("gethash")].into();
    f.max_stack = 16;
    set_knob(&mut ev, VmLeafKnob::ALL);
    let runs0 = super::vm_leaf::vm_leaf_calls_for_test();
    assert_eq!(
        run(&mut ev, &f, &[Value::fixnum(1), Value::fixnum(5)]),
        "(caught (wrong-type-argument hash-table-p 5))"
    );
    assert_eq!(super::vm_leaf::vm_leaf_calls_for_test() - runs0, 1);
    super::vm_leaf::force_vm_leaf_knob_for_test(None);
}

/// A redefinition takes effect at the next call (GNU `Bcall` reads the
/// function cell every time): the new definition runs, and restoring the
/// builtin brings the leaf back. An alias of the builtin gets its leaf.
#[test]
fn redefinitions_and_aliases_are_resolved_at_each_call() {
    let mut ev = Context::new();
    set_knob(&mut ev, VmLeafKnob::ALL);
    let table = ev
        .eval_str("(let ((h (make-hash-table))) (puthash 1 'one h) h)")
        .expect("table");
    crate::emacs_core::eval::push_scratch_gc_root(table);
    let args = [Value::fixnum(1), table];
    let f = bcall_fn("gethash", 2);
    let runs0 = super::vm_leaf::vm_leaf_calls_for_test();
    assert_eq!(run(&mut ev, &f, &args), "one");
    assert_eq!(super::vm_leaf::vm_leaf_calls_for_test() - runs0, 1);
    ev.eval_str(
        "(progn (defvar vm-leaf-orig (symbol-function 'gethash))
                (fset 'gethash (lambda (_k _h &optional _d) 'redefined)))",
    )
    .expect("redefine");
    let runs1 = super::vm_leaf::vm_leaf_calls_for_test();
    assert_eq!(run(&mut ev, &f, &args), "redefined");
    assert_eq!(super::vm_leaf::vm_leaf_calls_for_test(), runs1);
    ev.eval_str("(fset 'gethash vm-leaf-orig)")
        .expect("restore");
    assert_eq!(run(&mut ev, &f, &args), "one");
    assert_eq!(super::vm_leaf::vm_leaf_calls_for_test() - runs1, 1);
    ev.eval_str("(defalias 'vm-leaf-gethash-alias (symbol-function 'gethash))")
        .expect("alias");
    let alias = bcall_fn("vm-leaf-gethash-alias", 2);
    assert_eq!(run(&mut ev, &alias, &args), "one");
    assert_eq!(super::vm_leaf::vm_leaf_calls_for_test() - runs1, 2);
    super::vm_leaf::force_vm_leaf_knob_for_test(None);
}

/// At the depth limit the call signals from the caller's depth test, as
/// before: the leaf never runs.
#[test]
fn the_depth_limit_is_the_callers() {
    let mut ev = Context::new();
    set_knob(&mut ev, VmLeafKnob::ALL);
    let table = ev
        .eval_str("(let ((h (make-hash-table))) (puthash 1 'one h) h)")
        .expect("table");
    crate::emacs_core::eval::push_scratch_gc_root(table);
    let f = bcall_fn("gethash", 2);
    let args = [Value::fixnum(1), table];
    let saved = (ev.depth, ev.max_depth);
    let mut outcomes = Vec::new();
    for knob in [VmLeafKnob::OFF, VmLeafKnob::ALL] {
        set_knob(&mut ev, knob);
        ev.max_depth = 200;
        ev.depth = 200;
        let runs0 = super::vm_leaf::vm_leaf_calls_for_test();
        outcomes.push(run(&mut ev, &f, &args));
        assert_eq!(super::vm_leaf::vm_leaf_calls_for_test(), runs0, "{knob:?}");
        (ev.depth, ev.max_depth) = saved;
    }
    assert_eq!(outcomes[0], outcomes[1]);
    assert!(outcomes[1].starts_with("signal"), "{}", outcomes[1]);
    super::vm_leaf::force_vm_leaf_knob_for_test(None);
}

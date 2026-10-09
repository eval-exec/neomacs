//! Variable ops inline in JIT code (`compile::inline_vars`, P1.4 Stage B):
//! each shape a fast path takes must answer as the interpreter answers and
//! take no shim; each shape or state it must refuse (a watcher, an alias, a
//! cache loaded for another buffer, a concurrent mark, a projected symbol)
//! must reach the unchanged shim; and with the knob off nothing is emitted.
//!
//! The Stage A transcripts (`eval/tests/var_fast.rs`) run every scenario on
//! each knob value too (`INLINE_ENGINES`).

use super::inline_vars::{
    InlineVarOp, inline_var_sites, reset_inline_var_sites, with_compile_env_for_test,
};
use super::shims::{UNBIND_SHIM_CALLS, VARBIND_SHIM_CALLS, VARREF_SHIM_CALLS, VARSET_SHIM_CALLS};
use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

const OFF: InlineVarsKnob = InlineVarsKnob::OFF;
const ALL: InlineVarsKnob = InlineVarsKnob::ALL;

/// A bytecode body over the variable (constant 0) and the body function
/// `ivt-body` (constant 1).
#[derive(Clone)]
struct Prog {
    ops: Vec<Op>,
    arity: usize,
}

impl Prog {
    /// `(lambda () VAR)`
    fn read() -> Self {
        Self {
            ops: vec![Op::VarRef(0), Op::Return],
            arity: 0,
        }
    }

    /// `(lambda (v) (setq VAR v) VAR)`
    fn setq() -> Self {
        Self {
            ops: vec![Op::StackRef(0), Op::VarSet(0), Op::VarRef(0), Op::Return],
            arity: 1,
        }
    }

    /// `(lambda (v) (list (let ((VAR v)) (ivt-body)) VAR))`
    fn let_call() -> Self {
        Self {
            ops: vec![
                Op::StackRef(0),
                Op::VarBind(0),
                Op::Constant(1),
                Op::Call(0),
                Op::Unbind(1),
                Op::VarRef(0),
                Op::List(2),
                Op::Return,
            ],
            arity: 1,
        }
    }

    /// `(lambda (a b) (list (let ((VAR a)) (let ((VAR b)) (ivt-body))) VAR))`
    /// with one `unbind 2`.
    fn let_nested() -> Self {
        Self {
            ops: vec![
                Op::StackRef(1),
                Op::VarBind(0),
                Op::StackRef(0),
                Op::VarBind(0),
                Op::Constant(1),
                Op::Call(0),
                Op::Unbind(2),
                Op::VarRef(0),
                Op::List(2),
                Op::Return,
            ],
            arity: 2,
        }
    }
}

fn constants(var: &str) -> Vec<Value> {
    vec![Value::symbol(var), Value::symbol("ivt-body")]
}

fn flow_text(flow: crate::emacs_core::error::Flow) -> String {
    match flow.into_kind() {
        crate::emacs_core::error::FlowKind::Signal(sig) => format!(
            "ERR {} {}",
            sig.symbol_name(),
            sig.data
                .iter()
                .map(print_value)
                .collect::<Vec<_>>()
                .join(" ")
        ),
        other => format!("ERR {other:?}"),
    }
}

fn interpret(ev: &mut Context, prog: &Prog, var: &str, args: &[Value]) -> String {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..prog.arity)
            .map(|i| intern(&format!("ivt-arg{i}")))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = prog.ops.clone();
    f.constants = constants(var).into();
    f.max_stack = 8;
    let mut vm = Vm::from_context(ev);
    match vm.execute(&f, args.to_vec()) {
        Ok(v) => print_value(&v),
        Err(flow) => flow_text(flow),
    }
}

/// PROG over VAR compiled against EV with the knob forced to KNOB.
fn compile(ev: &Context, knob: InlineVarsKnob, prog: &Prog, var: &str) -> CompiledLeaf {
    force_inline_vars_for_test(Some(knob));
    let leaf = with_compile_env_for_test(ev, || lower_leaf(&prog.ops, &constants(var), prog.arity));
    force_inline_vars_for_test(None);
    leaf.expect("program lowers")
}

/// The four variable shims' call counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Shims {
    varref: usize,
    varset: usize,
    varbind: usize,
    unbind: usize,
}

fn shims() -> Shims {
    Shims {
        varref: VARREF_SHIM_CALLS.with(|c| c.get()),
        varset: VARSET_SHIM_CALLS.with(|c| c.get()),
        varbind: VARBIND_SHIM_CALLS.with(|c| c.get()),
        unbind: UNBIND_SHIM_CALLS.with(|c| c.get()),
    }
}

fn shims_since(before: Shims) -> Shims {
    let now = shims();
    Shims {
        varref: now.varref - before.varref,
        varset: now.varset - before.varset,
        varbind: now.varbind - before.varbind,
        unbind: now.unbind - before.unbind,
    }
}

/// Run LEAF on EV: the answer, and the shims it called.
fn run(ev: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> (String, Shims) {
    let before = shims();
    let ctx = ev as *mut Context as *mut u8;
    let answer = match leaf.call(ctx, args) {
        NativeRun::Ok(bits) => print_value(&Value::from_bits(bits)),
        NativeRun::Signal => flow_text(take_pending_flow().expect("flow stashed")),
        other => panic!("must not leave native code: {other:?}"),
    };
    (answer, shims_since(before))
}

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(v) => print_value(&v),
        Err(e) => format!("ERR {e:?}"),
    }
}

fn eval_ok(ev: &mut Context, src: &str) {
    ev.eval_str(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
}

/// Everything Lisp sees of VAR here and in `ivt-other`.
fn observe(ev: &mut Context, var: &str) -> String {
    eval(
        ev,
        &format!(
            "(list (condition-case nil {var} (void-variable 'void))
                   (condition-case nil (default-value '{var}) (void-variable 'void))
                   (local-variable-p '{var})
                   (save-current-buffer (set-buffer ivt-other)
                     (list (condition-case nil {var} (void-variable 'void))
                           (local-variable-p '{var})))
                   (eq (current-buffer) ivt-home))"
        ),
    )
}

/// One variable per inline shape (and the refused ones), in a fresh context
/// whose current buffer is `ivt-home`.
///
/// | variable | shape |
/// |---|---|
/// | `ivt-plain` | plain special |
/// | `ivt-loc` | buffer-local, local binding here |
/// | `ivt-locd` | buffer-local, default here (local in `ivt-other`) |
/// | `ivt-auto` | `make-variable-buffer-local`, default here |
/// | `ivt-lbool` / `ivt-lint` | buffer-local over a Bool / Int forwarder |
/// | `ivt-obj` / `ivt-bool` / `ivt-int` | forwarded Obj / Bool / Int |
fn fixture() -> Context {
    use crate::emacs_core::defvar_bool::ByteBooleanVars;
    use crate::emacs_core::forward::alloc_objfwd;
    let mut ev = Context::new();
    crate::emacs_core::jit::cache::clear();
    // A fresh context's bind stack has no capacity yet; an inline bind never
    // grows it (its first push is the shim's), and a session's first `let`
    // made it long ago.
    ev.jit_bind_stack.reserve(16);
    ev.obarray.intern("ivt-obj");
    ev.obarray
        .install_objfwd(intern("ivt-obj"), alloc_objfwd(Value::symbol("obj0")));
    ev.obarray
        .define_bool_variable("ivt-bool", false, ByteBooleanVars::ErasedByLreadInit);
    ev.obarray
        .define_bool_variable("ivt-lbool", true, ByteBooleanVars::ErasedByLreadInit);
    ev.obarray.define_int_variable("ivt-int", 7);
    ev.obarray.define_int_variable("ivt-lint", 8);
    eval_ok(
        &mut ev,
        "(progn
           (defvar ivt-home (current-buffer))
           (defvar ivt-other (get-buffer-create \" ivt-other\"))
           (defvar ivt-log nil)
           (fset 'ivt-watcher
                 (lambda (sym newval op where)
                   (setq ivt-log (cons (list sym op newval
                                             (cond ((eq where ivt-home) 'home)
                                                   ((eq where ivt-other) 'other)
                                                   (t where)))
                                       ivt-log))))
           (defvar ivt-plain 1)
           (defvar ivt-loc 10)
           (make-local-variable 'ivt-loc)
           (setq ivt-loc 11)
           (defvar ivt-locd 20)
           (save-current-buffer (set-buffer ivt-other)
             (set (make-local-variable 'ivt-locd) 21))
           (defvar ivt-auto 30)
           (make-variable-buffer-local 'ivt-auto)
           (make-local-variable 'ivt-lbool)
           (make-variable-buffer-local 'ivt-lint)
           (fset 'ivt-body (lambda () 'body)))",
    );
    ev
}

/// Load each variable's BLV cache for the current buffer (the general
/// path's swap-in): what an editor loop's first read does.
fn warm(ev: &mut Context, vars: &[&str]) {
    for var in vars {
        let _ = interpret(ev, &Prog::read(), var, &[]);
    }
}

#[test]
fn inline_vars_knob_parses_every_spelling() {
    assert_eq!(
        InlineVarsKnob::parse(None),
        InlineVarsKnob { read: true, ..OFF },
        "default read only"
    );
    assert_eq!(InlineVarsKnob::parse(Some("bogus")), OFF);
    for off in ["", "0", "off", "no", "none", "false"] {
        assert_eq!(InlineVarsKnob::parse(Some(off)), OFF, "{off:?}");
    }
    for all in ["1", "on", "all", "ALL", "yes"] {
        assert_eq!(InlineVarsKnob::parse(Some(all)), ALL, "{all:?}");
    }
    assert_eq!(
        InlineVarsKnob::parse(Some("read")),
        InlineVarsKnob {
            read: true,
            set: false,
            bind: false
        }
    );
    assert_eq!(
        InlineVarsKnob::parse(Some("set, bind,bogus")),
        InlineVarsKnob {
            read: false,
            set: true,
            bind: true
        },
        "an unknown part is ignored"
    );
    assert_eq!(InlineVarsKnob::parse(Some("read,set,bind")), ALL);
}

/// Every shape a fast path takes answers as the interpreter does and calls
/// no variable shim: reads, `setq`s, `let`s and nested `let`s of plain,
/// buffer-local (own binding and default) and forwarded variables.
#[test]
fn cached_shapes_take_no_shim_and_answer_as_the_interpreter() {
    const VARS: &[&str] = &[
        "ivt-plain",
        "ivt-loc",
        "ivt-locd",
        "ivt-lbool",
        "ivt-obj",
        "ivt-bool",
        "ivt-int",
    ];
    let progs: [(&str, Prog, Vec<Value>); 4] = [
        ("read", Prog::read(), vec![]),
        ("setq", Prog::setq(), vec![Value::make_int(5)]),
        ("let", Prog::let_call(), vec![Value::make_int(6)]),
        (
            "nested",
            Prog::let_nested(),
            vec![Value::make_int(7), Value::make_int(8)],
        ),
    ];
    for &var in VARS {
        for (name, prog, args) in &progs {
            // Each engine in its own fixture: a `setq` changes the variable.
            let mut ev = crate::test_utils::with_legacy_gc(fixture);
            // This test measures the original emitter's shim counts.
            warm(&mut ev, VARS);
            let want = interpret(&mut ev, prog, var, args);
            let want_after = observe(&mut ev, var);
            let mut ev = crate::test_utils::with_legacy_gc(fixture);
            warm(&mut ev, VARS);
            reset_inline_var_sites();
            let leaf = compile(&ev, ALL, prog, var);
            let (got, called) = run(&mut ev, &leaf, args);
            assert_eq!(got, want, "{var} {name}");
            assert_eq!(observe(&mut ev, var), want_after, "{var} {name}: after");
            assert_eq!(called, Shims::default(), "{var} {name}: inline");
            assert!(inline_var_sites(InlineVarOp::Read) > 0, "{var} {name}");
        }
    }
}

/// With the knob off nothing is inlined: every op calls its shim.
#[test]
fn knob_off_inlines_nothing() {
    let mut ev = crate::test_utils::with_legacy_gc(fixture);
    // Isolate the inline-variable knob's legacy fast-path counts.
    warm(&mut ev, &["ivt-plain", "ivt-loc"]);
    for (knob, inline) in [(OFF, false), (ALL, true)] {
        reset_inline_var_sites();
        let leaf = compile(&ev, knob, &Prog::let_call(), "ivt-loc");
        let (got, called) = run(&mut ev, &leaf, &[Value::make_int(3)]);
        assert_eq!(got, "(body 11)");
        let sites =
            [InlineVarOp::Read, InlineVarOp::Bind, InlineVarOp::Unbind].map(inline_var_sites);
        if inline {
            assert_eq!(sites, [1, 1, 1]);
            assert_eq!(called, Shims::default());
        } else {
            assert_eq!(sites, [0, 0, 0]);
            assert_eq!(
                called,
                Shims {
                    varref: 1,
                    varset: 0,
                    varbind: 1,
                    unbind: 1
                }
            );
        }
    }
}

/// The class a leaf was compiled against is re-tested on every run: after a
/// variable becomes buffer-local, aliased, watched or void, the inline op
/// refuses and the shim answers as the interpreter does.
#[test]
fn a_class_change_after_compile_takes_the_shim() {
    // (what, the change, which of read/setq/let must now reach a shim: a
    // watcher traps writes only; a void plain cell refuses the read only --
    // its `setq` and `let` are plain stores, as in `try_set_plain_variable`).
    type Change = (&'static str, &'static str, [bool; 3]);
    let changes: &[Change] = &[
        (
            "make-local",
            "(progn (make-local-variable 'ivt-plain) (setq ivt-plain 111))",
            [true; 3],
        ),
        ("alias", "(defvaralias 'ivt-plain 'ivt-loc)", [true; 3]),
        (
            "watch",
            "(add-variable-watcher 'ivt-plain 'ivt-watcher)",
            [false, true, true],
        ),
        // The `let` program reads the void variable after its unbind.
        ("makunbound", "(makunbound 'ivt-plain)", [true, false, true]),
    ];
    let progs = [
        ("read", Prog::read(), vec![]),
        ("setq", Prog::setq(), vec![Value::make_int(5)]),
        ("let", Prog::let_call(), vec![Value::make_int(6)]),
    ];
    for &(what, change, refused) in changes {
        for ((name, prog, args), refused) in progs.iter().zip(refused) {
            let mut ev = crate::test_utils::with_legacy_gc(fixture);
            // This test identifies class guards through original shim counts.
            eval_ok(&mut ev, change);
            let want = interpret(&mut ev, prog, "ivt-plain", args);
            let want_after = observe(&mut ev, "ivt-plain");
            let want_log = eval(&mut ev, "(prog1 (reverse ivt-log) (setq ivt-log nil))");
            let mut ev = crate::test_utils::with_legacy_gc(fixture);
            // Compiled while the variable was a plain special.
            let leaf = compile(&ev, ALL, prog, "ivt-plain");
            eval_ok(&mut ev, change);
            let (got, called) = run(&mut ev, &leaf, args);
            assert_eq!(got, want, "{what} {name}");
            assert_eq!(observe(&mut ev, "ivt-plain"), want_after, "{what} {name}");
            assert_eq!(
                eval(&mut ev, "(prog1 (reverse ivt-log) (setq ivt-log nil))"),
                want_log,
                "{what} {name}: the watcher saw the same calls"
            );
            assert_eq!(
                called != Shims::default(),
                refused,
                "{what} {name}: refused inline? {called:?}"
            );
        }
    }
}

/// A buffer-local variable's cache loaded for another buffer, or before a
/// structural alist change, is a miss: the shim swaps it in, and the next
/// run hits again.
#[test]
fn a_cache_miss_takes_the_shim_and_the_next_run_hits() {
    let mut ev = crate::test_utils::with_legacy_gc(fixture);
    // A subsequent zero-shim store is a legacy emitter assertion.
    warm(&mut ev, &["ivt-locd"]);
    let leaf = compile(&ev, ALL, &Prog::read(), "ivt-locd");
    assert_eq!(run(&mut ev, &leaf, &[]), ("20".into(), Shims::default()));
    eval_ok(&mut ev, "(set-buffer ivt-other)");
    let (got, called) = run(&mut ev, &leaf, &[]);
    assert_eq!((got.as_str(), called.varref), ("21", 1), "a miss");
    assert_eq!(run(&mut ev, &leaf, &[]), ("21".into(), Shims::default()));
    eval_ok(&mut ev, "(kill-local-variable 'ivt-locd)");
    let (got, called) = run(&mut ev, &leaf, &[]);
    assert_eq!((got.as_str(), called.varref), ("20", 1), "the epoch moved");
    // `setq` of a `make-variable-buffer-local` variable with no binding here
    // auto-creates one through the shim; the next `setq` is inline.
    eval_ok(&mut ev, "(set-buffer ivt-home)");
    warm(&mut ev, &["ivt-auto"]);
    let leaf = compile(&ev, ALL, &Prog::setq(), "ivt-auto");
    let (got, called) = run(&mut ev, &leaf, &[Value::make_int(31)]);
    assert_eq!((got.as_str(), called.varset), ("31", 1), "auto-create");
    assert_eq!(
        eval(
            &mut ev,
            "(list (local-variable-p 'ivt-auto) (default-value 'ivt-auto))"
        ),
        "(t 30)"
    );
    let (got, called) = run(&mut ev, &leaf, &[Value::make_int(32)]);
    assert_eq!((got.as_str(), called), ("32".into(), Shims::default()));
}

/// Type rules: an integer forwarder takes a fixnum inline and signals for
/// anything else through the shim with the specpdl unwound; a Boolean one
/// canonicalises.
#[test]
fn type_rules_match_the_interpreter() {
    let cases: &[(&str, Prog, Vec<Value>)] = &[
        ("ivt-int", Prog::setq(), vec![Value::string("s")]),
        ("ivt-int", Prog::let_call(), vec![Value::string("s")]),
        ("ivt-lint", Prog::let_call(), vec![Value::string("s")]),
        ("ivt-bool", Prog::let_call(), vec![Value::make_int(5)]),
        ("ivt-lbool", Prog::setq(), vec![Value::make_int(5)]),
        ("ivt-lbool", Prog::let_call(), vec![Value::NIL]),
    ];
    for (var, prog, args) in cases {
        let answers: Vec<(String, String)> = [None, Some(ALL)]
            .into_iter()
            .map(|knob| {
                let mut ev = fixture();
                warm(&mut ev, &[var]);
                let depth = ev.specpdl.len();
                let got = match knob {
                    None => interpret(&mut ev, prog, var, args),
                    Some(knob) => {
                        let leaf = compile(&ev, knob, prog, var);
                        run(&mut ev, &leaf, args).0
                    }
                };
                // A signal leaves the leaf's frame to its caller's unwinder.
                if ev.specpdl.len() > depth {
                    ev.unbind_to(depth);
                }
                ev.jit_bind_stack.clear();
                (got, observe(&mut ev, var))
            })
            .collect();
        assert_eq!(answers[0], answers[1], "{var} {:?}", prog.ops);
    }
}

/// While a concurrent mark runs (the barrier window is ALL), stores into a
/// symbol cell, a forwarder and a BLV cons go to the shims, which bracket
/// the seqlock and log the pre-image; after it admitted stores are inline again.
fn check_concurrent_mark_store_shims() {
    let mut ev = crate::test_utils::with_legacy_gc(fixture);
    // This test pins the legacy window's shim counts. Generational Stage A
    // keeps ALL until C2.8, including the BLV store guard's marking test.
    warm(&mut ev, &["ivt-loc"]);
    eval_ok(&mut ev, "(setq ivt-plain (list 'old-plain))");
    let inline_bindings = super::inline_vars::blv_bind_layout_available_for_test();
    reset_inline_var_sites();
    let set_plain = compile(&ev, ALL, &Prog::setq(), "ivt-plain");
    let let_loc = compile(&ev, ALL, &Prog::let_call(), "ivt-loc");
    let set_obj = compile(&ev, ALL, &Prog::setq(), "ivt-obj");
    assert_eq!(inline_var_sites(InlineVarOp::Set), 2);
    assert_eq!(
        inline_var_sites(InlineVarOp::Bind),
        u32::from(inline_bindings)
    );
    assert_eq!(
        inline_var_sites(InlineVarOp::Unbind),
        u32::from(inline_bindings)
    );
    let depths = (ev.specpdl.len(), ev.jit_bind_stack.len());
    ev.tagged_heap.set_concurrent_active_for_test(true);
    let (got, called) = run(&mut ev, &set_plain, &[Value::make_int(1)]);
    assert_eq!((got.as_str(), called.varset), ("1", 1));
    let logged = ev.tagged_heap.take_satb_shared_for_test();
    let (got, called) = run(&mut ev, &let_loc, &[Value::make_int(2)]);
    assert_eq!(got, "(body 11)");
    assert_eq!((called.varbind, called.unbind), (1, 1));
    assert_eq!((ev.specpdl.len(), ev.jit_bind_stack.len()), depths);
    let (got, called) = run(&mut ev, &set_obj, &[Value::make_int(3)]);
    assert_eq!((got.as_str(), called.varset), ("3", 1));
    ev.tagged_heap.set_concurrent_active_for_test(false);
    assert!(
        logged.iter().any(|v| print_value(v) == "(old-plain)"),
        "the overwritten plain value is logged: {logged:?}"
    );
    let bind_shims = Shims {
        varbind: usize::from(!inline_bindings),
        unbind: usize::from(!inline_bindings),
        ..Shims::default()
    };
    for (leaf, args, expected, expected_shims) in [
        (&set_plain, vec![Value::make_int(4)], "4", Shims::default()),
        (&let_loc, vec![Value::make_int(5)], "(body 11)", bind_shims),
        (&set_obj, vec![Value::make_int(6)], "6", Shims::default()),
    ] {
        let (got, called) = run(&mut ev, leaf, &args);
        assert_eq!((got.as_str(), called), (expected, expected_shims));
        assert_eq!((ev.specpdl.len(), ev.jit_bind_stack.len()), depths);
    }
    assert!(ev.tagged_heap.take_satb_shared_for_test().is_empty());
    assert_eq!(eval(&mut ev, "ivt-loc"), "11");
}

#[test]
fn a_concurrent_mark_sends_every_store_to_the_shim() {
    check_concurrent_mark_store_shims();
}

#[test]
fn a_concurrent_mark_sends_every_store_to_the_shim_without_let_layout() {
    super::jit_layout::with_unavailable_let_layout_for_test(|| {
        assert!(!super::inline_vars::blv_bind_layout_available_for_test());
        check_concurrent_mark_store_shims();
    });
}

/// A `let` whose body switches buffers restores the binding in the buffer
/// it was made in (the shim's `LetLocal` arm); one whose body kills the
/// local lets the kill win; one whose body adds a watcher reports the
/// `unlet` -- all as the interpreter.
#[test]
fn let_bodies_that_change_the_world_unbind_as_the_interpreter() {
    let bodies = [
        "(progn (set-buffer ivt-other) VAR)",
        "(progn (kill-local-variable 'VAR) VAR)",
        "(progn (add-variable-watcher 'VAR 'ivt-watcher) VAR)",
        "(progn (kill-all-local-variables) VAR)",
        "(progn (make-local-variable 'VAR) (setq VAR 400) VAR)",
    ];
    for var in ["ivt-plain", "ivt-loc", "ivt-locd", "ivt-obj", "ivt-lbool"] {
        for body in bodies {
            let body = body.replace("VAR", var);
            let setup = format!("(fset 'ivt-body (lambda () {body}))");
            let mut ev = fixture();
            warm(&mut ev, &[var]);
            eval_ok(&mut ev, &setup);
            let want = interpret(&mut ev, &Prog::let_call(), var, &[Value::make_int(9)]);
            let want_after = observe(&mut ev, var);
            let want_log = eval(&mut ev, "(reverse ivt-log)");
            let mut ev = fixture();
            warm(&mut ev, &[var]);
            eval_ok(&mut ev, &setup);
            let leaf = compile(&ev, ALL, &Prog::let_call(), var);
            let (got, _) = run(&mut ev, &leaf, &[Value::make_int(9)]);
            assert_eq!(got, want, "{var} {body}");
            assert_eq!(observe(&mut ev, var), want_after, "{var} {body}");
            assert_eq!(eval(&mut ev, "(reverse ivt-log)"), want_log, "{var} {body}");
        }
    }
}

// ---------------------------------------------------------------------------
// Through the JIT cache, in a dumped runtime
// ---------------------------------------------------------------------------

/// Warm SRC's functions into compiled leaves with the knob at KNOB and
/// return the context.
fn warmed_runtime(knob: InlineVarsKnob, src: &str, legacy_gc: bool) -> Context {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
    force_inline_vars_for_test(Some(knob));
    let mut ev = if legacy_gc {
        crate::test_utils::with_legacy_gc(crate::test_utils::runtime_startup_context)
    } else {
        crate::test_utils::runtime_startup_context()
    };
    ev.eval_str(src).expect("warmed");
    ev
}

/// Reads of the dumped runtime's own variables -- `case-fold-search`
/// (buffer-local over an Obj forwarder), `inhibit-read-only` (forwarded
/// Obj), `indent-tabs-mode` (buffer-local over a Bool forwarder),
/// `gc-cons-threshold` (forwarded Int) and a plain special -- answer the same
/// with the knob on and off, and with it on none reaches the read shim.
#[test]
fn a_dumped_runtime_reads_inline() {
    let src = r#"(progn
      (defvar ivt-count 3)
      (defun ivt-r ()
        (list case-fold-search inhibit-read-only indent-tabs-mode
              gc-cons-threshold ivt-count))
      (byte-compile 'ivt-r)
      (with-temp-buffer (dotimes (_ 1500) (ivt-r))))"#;
    let answers: Vec<(String, Shims)> = [OFF, ALL]
        .into_iter()
        .map(|knob| {
            let mut ev = warmed_runtime(knob, src, false);
            // A new buffer's first read loads its caches (the shim).
            eval_ok(
                &mut ev,
                r#"(progn (set-buffer (get-buffer-create " ivt-probe")) (ivt-r))"#,
            );
            let before = shims();
            let answer = eval(&mut ev, "(ivt-r)");
            force_inline_vars_for_test(None);
            (answer, shims_since(before))
        })
        .collect();
    assert_eq!(answers[0].0, answers[1].0, "knob off vs on");
    assert!(answers[0].0.starts_with("(t nil "), "{}", answers[0].0);
    assert!(answers[0].1.varref >= 3, "{:?}", answers[0].1);
    assert_eq!(answers[1].1, Shims::default(), "{:?}", answers[1].1);
}

/// Programs over the dumped runtime's own variables: `case-fold-search`
/// (buffer-local over an Obj forwarder, its default cell in the dump
/// image), `inhibit-read-only` (forwarded Obj), `indent-tabs-mode`
/// (buffer-local over a Bool forwarder), a `condition-case` inside a `let`,
/// and a watched special.
const RUNTIME_PROGRAM: &str = r#"(progn
  (defvar ivt-count 0)
  (defvar ivt-watched 1)
  (defvar ivt-wlog nil)
  (add-variable-watcher 'ivt-watched
    (lambda (s n op w) (push (list op n (bufferp w)) ivt-wlog)))
  (defun ivt-g () (list case-fold-search inhibit-read-only indent-tabs-mode))
  (defun ivt-f (x)
    (let ((case-fold-search x) (inhibit-read-only x) (indent-tabs-mode x))
      (setq ivt-count (1+ ivt-count))
      (ivt-g)))
  (defun ivt-h (x)
    (condition-case err
        (let ((case-fold-search x)) (setq ivt-watched x) (signal 'error (list case-fold-search)))
      (error (list err case-fold-search ivt-watched))))
  (dolist (f '(ivt-g ivt-f ivt-h)) (byte-compile f))
  (with-temp-buffer
    (dotimes (i 1500) (ivt-f i) (ivt-h i))))"#;

/// The dumped runtime's answers are the same with the knob on and off, and
/// with it on the binds of `case-fold-search` -- whose default cell is a
/// dump-image cons, remembered at compile time -- are inline.
#[test]
fn a_dumped_runtime_answers_the_same_with_binds_inline() {
    // The first binds in a new buffer miss its BLV caches (the shim swaps
    // them in); the probe runs in a buffer that has seen them once.
    let setup = r#"(progn (set-buffer (get-buffer-create " ivt-probe"))
                          (ivt-f 'warm) (ivt-h 'warm))"#;
    let probe = r#"(list (ivt-f 'on) (ivt-h 'x) ivt-count case-fold-search inhibit-read-only
             indent-tabs-mode ivt-watched (length ivt-wlog) (car ivt-wlog))"#;
    let answers: Vec<(String, Shims)> = [OFF, ALL]
        .into_iter()
        .map(|knob| {
            let mut ev = warmed_runtime(knob, RUNTIME_PROGRAM, true);
            eval_ok(&mut ev, setup);
            let before = shims();
            let answer = eval(&mut ev, probe);
            force_inline_vars_for_test(None);
            (answer, shims_since(before))
        })
        .collect();
    assert_eq!(answers[0].0, answers[1].0, "knob off vs on");
    assert!(
        answers[0].0.starts_with("((on on t) ((error x) t x) 1502 "),
        "{}",
        answers[0].0
    );
    // Off: every bind and unbind of the probe's compiled calls is a shim
    // call; on, none is (the watched `setq` is the one write left).
    assert!(answers[0].1.varbind >= 4, "{:?}", answers[0].1);
    assert_eq!(
        (answers[1].1.varbind, answers[1].1.unbind),
        (0, 0),
        "{:?}",
        answers[1].1
    );
    assert_eq!(answers[1].1.varset, 1, "{:?}", answers[1].1);
}

/// `(lambda (n) (let ((osr-bound 3)) (let ((sum 0) (i 0)) (while (< i n)
/// [(let ((osr-bound 7))] (setq sum (+ sum osr-bound)) [)] (setq i (1+ i)))
/// sum)))`, hand-assembled (as `tests/osr_bindings.rs`).
fn binding_sum(nested: bool) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("n")],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.constants = vec![
        Value::make_int(0),
        Value::make_int(3),
        Value::symbol("osr-bound"),
        Value::make_int(7),
    ]
    .into();
    f.ops = vec![
        Op::Constant(1),
        Op::VarBind(2),
        Op::Constant(0),
        Op::Constant(0), // n sum i
        Op::StackRef(0),
        Op::StackRef(3),
        Op::Lss,
        Op::GotoIfNil(0),
    ];
    if nested {
        f.ops.extend([Op::Constant(3), Op::VarBind(2)]);
    }
    f.ops
        .extend([Op::StackRef(1), Op::VarRef(2), Op::Add, Op::StackSet(2)]);
    if nested {
        f.ops.extend([
            Op::Unbind(1),
            Op::StackRef(1),
            Op::VarRef(2),
            Op::Add,
            Op::StackSet(2),
        ]);
    }
    f.ops
        .extend([Op::StackRef(0), Op::Add1, Op::StackSet(1), Op::Goto(4)]);
    f.ops[7] = Op::GotoIfNil(f.ops.len() as u32);
    f.ops.extend([Op::StackRef(1), Op::Unbind(1), Op::Return]);
    f.max_stack = 16;
    f
}

/// OSR into a loop inside a `let`: the interpreter made the binding before
/// the transfer, and the inline `unbind` after the loop restores it; the
/// nested variant binds and unbinds inline on every iteration.
#[test]
fn osr_into_a_let_unbinds_inline_what_the_interpreter_bound() {
    use std::sync::atomic::Ordering;
    for nested in [false, true] {
        let mut answers = Vec::new();
        for knob in [OFF, ALL] {
            force_inline_vars_for_test(Some(knob));
            crate::emacs_core::jit::force_osr_for_test(true);
            let mut ctx = crate::test_utils::with_legacy_gc(Context::new);
            // This test requires the original inline bind/unbind counts.
            crate::emacs_core::jit::cache::clear();
            ctx.eval_str("(setq osr-bound 17)").unwrap();
            let depth = ctx.specpdl.len();
            let mut f = binding_sum(nested);
            f.seal_hand_assembled_ops();
            f.jit_runtime().set_hot_for_test();
            let transfers =
                crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed);
            let before = shims();
            let value = Vm::from_context(&mut ctx)
                .execute(&f, vec![Value::make_int(2000)])
                .unwrap();
            let called = shims_since(before);
            assert_eq!(
                crate::emacs_core::jit::cache::OSR_TRANSFER_COUNT.load(Ordering::Relaxed)
                    - transfers,
                1,
                "the loop transferred"
            );
            crate::emacs_core::jit::force_osr_for_test(false);
            force_inline_vars_for_test(None);
            assert_eq!(ctx.specpdl.len(), depth);
            assert!(ctx.jit_bind_stack.is_empty());
            answers.push((
                print_value(&value),
                print_value(&ctx.eval_str("osr-bound").unwrap()),
            ));
            if knob == ALL {
                assert_eq!(called.unbind, 0, "nested={nested}: {called:?}");
                // The first inner bind grows the bind stack in the shim.
                assert!(called.varbind <= 1, "nested={nested}: {called:?}");
            }
        }
        let want = if nested { "20000" } else { "6000" };
        assert_eq!(answers[0], (want.to_string(), "17".to_string()));
        assert_eq!(answers[0], answers[1], "nested={nested}");
    }
}

/// `(lambda (c) (if c (varbind A 5) (varbind B 5)) (ivt-body) (unbind 1))`,
/// hand-assembled: the `unbind` pops whichever binding its path made.
fn two_path_bind(a: &str, b: &str) -> (Vec<Op>, Vec<Value>) {
    let ops = vec![
        Op::StackRef(0),
        Op::GotoIfNil(5),
        Op::Constant(2),
        Op::VarBind(0),
        Op::Goto(7),
        Op::Constant(2),
        Op::VarBind(1),
        Op::Constant(3),
        Op::Call(0),
        Op::Unbind(1),
        Op::Return,
    ];
    let constants = vec![
        Value::symbol(a),
        Value::symbol(b),
        Value::make_int(5),
        Value::symbol("ivt-body"),
    ];
    (ops, constants)
}

/// The static binding sites of an `unbind` are met over every path into it:
/// when both paths bound the same symbol the `unbind` is inline, when they
/// bound different ones it is the shim's -- and either way each path's
/// binding is undone.
#[test]
fn unbind_sites_meet_across_paths() {
    for (a, b, inline) in [
        ("ivt-plain", "ivt-plain", true),
        ("ivt-plain", "ivt-loc", false),
    ] {
        let mut ev = crate::test_utils::with_legacy_gc(fixture);
        // The CFG test identifies inlined sites through original shim counts.
        warm(&mut ev, &["ivt-loc"]);
        eval_ok(
            &mut ev,
            "(fset 'ivt-body (lambda () (list ivt-plain ivt-loc)))",
        );
        let (ops, constants) = two_path_bind(a, b);
        force_inline_vars_for_test(Some(ALL));
        reset_inline_var_sites();
        let leaf =
            with_compile_env_for_test(&ev, || lower_leaf(&ops, &constants, 1)).expect("lowers");
        force_inline_vars_for_test(None);
        assert_eq!(
            inline_var_sites(InlineVarOp::Unbind),
            u32::from(inline),
            "{a} {b}"
        );
        for c in [Value::T, Value::NIL] {
            let (got, called) = run(&mut ev, &leaf, &[c]);
            let bound = if c.is_nil() { b } else { a };
            let want = if bound == "ivt-plain" {
                "(5 11)"
            } else {
                "(1 5)"
            };
            assert_eq!(got, want, "{a} {b} {c:?}");
            assert_eq!(called.unbind, usize::from(!inline), "{a} {b} {c:?}");
            assert_eq!(eval(&mut ev, "(list ivt-plain ivt-loc)"), "(1 11)");
        }
    }
}

// Append inside jit/tests/inline_vars_test.rs. This uses its existing fixture,
// native runner, shim counters and synthetic concurrent-mark test API. Actual
// execution evaluates Lisp and therefore MUST run through sandbox-run.sh.

#[test]
fn atomic_forwarder_active_mark_routes_set_bind_and_unbind_through_satb() {
    let mut ev = fixture();
    eval_ok(&mut ev, "(setq ivt-obj (list 'old-object))");
    let before_set = ev
        .obarray
        .forwarder(intern("ivt-obj"))
        .and_then(|descriptor| descriptor.owned_value())
        .expect("rooted object forwarder");
    // Archive complete construction CLIF only if the gate runner requests
    // its own scratch directory. Both bind and unbind live in the let body.
    let mut leaves = None;
    let clif = super::compile_pipeline_tests::captured_clif(|| {
        let set = compile(&ev, ALL, &Prog::setq(), "ivt-obj");
        let bind_unbind = compile(&ev, ALL, &Prog::let_call(), "ivt-obj");
        leaves = Some((set, bind_unbind));
    });
    assert_eq!(clif.len(), 2);
    if let Some(directory) = std::env::var_os("NEOVM_P74_CLIF_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("owned CLIF evidence directory");
        std::fs::write(directory.join("forwarder-varset.clif"), &clif[0])
            .expect("archive varset CLIF");
        std::fs::write(directory.join("forwarder-varbind-unbind.clif"), &clif[1])
            .expect("archive varbind and unbind CLIF");
    }
    let (set, bind_unbind) = leaves.expect("both bodies compiled");
    let replacement = Value::list(vec![Value::symbol("new-object")]);
    let binding = Value::list(vec![Value::symbol("bound-object")]);
    ev.tagged_heap.set_concurrent_active_for_test(true);
    let (set_result, set_calls) = run(&mut ev, &set, &[replacement]);
    let set_preimages = ev.tagged_heap.take_satb_shared_for_test();
    let (let_result, let_calls) = run(&mut ev, &bind_unbind, &[binding]);
    let let_preimages = ev.tagged_heap.take_satb_shared_for_test();
    ev.tagged_heap.set_concurrent_active_for_test(false);
    assert_eq!(set_result, "(new-object)");
    assert_eq!(set_calls.varset, 1, "active mark refuses inline varset");
    assert!(
        set_preimages
            .iter()
            .any(|value| value.bits() == before_set.bits())
    );
    assert_eq!(let_result, "(body (new-object))");
    assert_eq!(let_calls.varbind, 1, "active mark refuses inline varbind");
    assert_eq!(let_calls.unbind, 1, "active mark refuses inline unbind");
    assert!(
        let_preimages
            .iter()
            .any(|value| value.bits() == replacement.bits())
    );
    assert!(
        let_preimages
            .iter()
            .any(|value| value.bits() == binding.bits())
    );
}

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
}

fn constants(var: &str) -> Vec<Value> {
    vec![Value::symbol(var), Value::symbol("ivt-body")]
}

fn flow_text(flow: crate::emacs_core::error::Flow) -> String {
    match flow {
        crate::emacs_core::error::Flow::Signal(sig) => format!(
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
    assert_eq!(InlineVarsKnob::parse(None), OFF, "default off");
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
/// no variable shim: reads of plain, buffer-local (own binding and default)
/// and forwarded variables.
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
    let progs: [(&str, Prog, Vec<Value>); 1] = [("read", Prog::read(), vec![])];
    for &var in VARS {
        for (name, prog, args) in &progs {
            // Each engine in its own fixture.
            let mut ev = fixture();
            warm(&mut ev, VARS);
            let want = interpret(&mut ev, prog, var, args);
            let want_after = observe(&mut ev, var);
            let mut ev = fixture();
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
    let mut ev = fixture();
    warm(&mut ev, &["ivt-plain", "ivt-loc"]);
    for (knob, inline) in [(OFF, false), (ALL, true)] {
        reset_inline_var_sites();
        let leaf = compile(&ev, knob, &Prog::read(), "ivt-loc");
        let (got, called) = run(&mut ev, &leaf, &[]);
        assert_eq!(got, "11");
        let sites = inline_var_sites(InlineVarOp::Read);
        if inline {
            assert_eq!(sites, 1);
            assert_eq!(called, Shims::default());
        } else {
            assert_eq!(sites, 0);
            assert_eq!(
                called,
                Shims {
                    varref: 1,
                    ..Shims::default()
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
    // (what, the change, whether the read must now reach the shim: a
    // watcher traps writes only).
    type Change = (&'static str, &'static str, [bool; 1]);
    let changes: &[Change] = &[
        (
            "make-local",
            "(progn (make-local-variable 'ivt-plain) (setq ivt-plain 111))",
            [true],
        ),
        ("alias", "(defvaralias 'ivt-plain 'ivt-loc)", [true]),
        (
            "watch",
            "(add-variable-watcher 'ivt-plain 'ivt-watcher)",
            [false],
        ),
        ("makunbound", "(makunbound 'ivt-plain)", [true]),
    ];
    let progs = [("read", Prog::read(), vec![])];
    for &(what, change, refused) in changes {
        for ((name, prog, args), refused) in progs.iter().zip(refused) {
            let mut ev = fixture();
            eval_ok(&mut ev, change);
            let want = interpret(&mut ev, prog, "ivt-plain", args);
            let want_after = observe(&mut ev, "ivt-plain");
            let want_log = eval(&mut ev, "(prog1 (reverse ivt-log) (setq ivt-log nil))");
            let mut ev = fixture();
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
    let mut ev = fixture();
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
}

// ---------------------------------------------------------------------------
// Through the JIT cache, in a dumped runtime
// ---------------------------------------------------------------------------

/// Warm SRC's functions into compiled leaves with the knob at KNOB and
/// return the context.
fn warmed_runtime(knob: InlineVarsKnob, src: &str) -> Context {
    crate::test_utils::init_test_tracing();
    force_profit_gate_for_test(false);
    crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
    force_inline_vars_for_test(Some(knob));
    let mut ev = crate::test_utils::runtime_startup_context();
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
            let mut ev = warmed_runtime(knob, src);
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

//! P1.4 Stage A pins: every variable shape a cached variable tier (read,
//! `setq`, `let`, unbind) may take or must refuse, driven through bytecode
//! (`varref`, `varset`, `varbind`, `unbind`) on the interpreter and on the JIT
//! baseline, with everything Lisp can observe afterwards recorded.
//!
//! Each scenario runs over every fixture variable in one fresh [`Context`] and
//! yields one transcript: per variable, the program's result and then the
//! variable as Lisp sees it (current value, default value, local in this
//! buffer, value and locality in a second buffer, whether the current buffer
//! is still the home buffer, the watcher log, whether the specpdl came back
//! to its depth). The engines must agree on every line; the scenarios that
//! pin a GNU rule check that rule by name as well.

use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    Context, VarCacheEvent, VarCacheTier, parse_var_cache_knob, reset_var_cache_events,
    set_var_cache_tiers_for_test, var_cache_census_report, var_cache_event_count,
};
use crate::emacs_core::intern::intern;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::{LambdaParams, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Engine {
    Interpreter,
    #[cfg(feature = "jit")]
    Jit,
}

const ENGINES: &[Engine] = &[
    Engine::Interpreter,
    #[cfg(feature = "jit")]
    Engine::Jit,
];

/// A bytecode body over the fixture variable (constant 0) and the body
/// function `vft-body` (constant 1).
struct Prog {
    ops: Vec<Op>,
    arity: usize,
}

impl Prog {
    fn constants(var: &str) -> Vec<Value> {
        vec![Value::symbol(var), Value::symbol("vft-body")]
    }

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

    /// `(lambda (v) (list (let ((VAR v)) (vft-body)) VAR))`
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

    /// `(lambda (a b) (list (let ((VAR a)) (setq VAR b) (vft-body)) VAR))`
    fn let_setq_call() -> Self {
        Self {
            ops: vec![
                Op::StackRef(1),
                Op::VarBind(0),
                Op::StackRef(0),
                Op::VarSet(0),
                Op::Constant(1),
                Op::Call(0),
                Op::Unbind(1),
                Op::VarRef(0),
                Op::List(2),
                Op::Return,
            ],
            arity: 2,
        }
    }

    /// `(lambda (a b) (list (let ((VAR a)) (let ((VAR b)) (vft-body))) VAR))`
    /// with one `unbind 2` for both bindings.
    fn let_nested_call() -> Self {
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

fn flow_text(flow: Flow) -> String {
    match flow {
        Flow::Signal(sig) => format!(
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

/// Run PROG over VAR with ARGS on ENGINE.
fn run(ev: &mut Context, engine: Engine, prog: &Prog, var: &str, args: &[Value]) -> String {
    let constants = Prog::constants(var);
    match engine {
        Engine::Interpreter => {
            let required = (0..prog.arity)
                .map(|i| intern(&format!("vft-arg{i}")))
                .collect();
            let mut f = ByteCodeFunction::new(LambdaParams {
                required,
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = prog.ops.clone();
            f.constants = constants.into();
            f.max_stack = 8;
            let mut vm = Vm::from_context(ev);
            match vm.execute(&f, args.to_vec()) {
                Ok(v) => print_value(&v),
                Err(flow) => flow_text(flow),
            }
        }
        #[cfg(feature = "jit")]
        Engine::Jit => {
            use crate::emacs_core::jit::compile::{NativeRun, lower_leaf, take_pending_flow};
            let leaf = lower_leaf(&prog.ops, &constants, prog.arity).expect("program lowers");
            let ctx = ev as *mut Context as *mut u8;
            match leaf.call(ctx, args) {
                NativeRun::Ok(bits) => print_value(&Value::from_bits(bits)),
                NativeRun::Signal => flow_text(take_pending_flow().expect("flow stashed")),
                other => format!("ERR unexpected {other:?}"),
            }
        }
    }
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

/// Everything Lisp can see of VAR, as one form: its value, default value
/// and locality here; its value and locality in `vft-other`; and `home`, or,
/// when the current buffer is no longer `vft-home`, its value and locality
/// there.
fn observe_form(var: &str) -> String {
    format!(
        "(list (condition-case nil {var} (void-variable 'void))
               (condition-case nil (default-value '{var}) (void-variable 'void))
               (local-variable-p '{var})
               (save-current-buffer
                 (set-buffer vft-other)
                 (list (condition-case nil {var} (void-variable 'void))
                       (local-variable-p '{var})))
               (if (eq (current-buffer) vft-home)
                   'home
                 (save-current-buffer
                   (set-buffer vft-home)
                   (list (condition-case nil {var} (void-variable 'void))
                         (local-variable-p '{var})))))"
    )
}

/// The fixture: one variable per shape, in a fresh context whose current
/// buffer is `vft-home`, with a second buffer `vft-other`.
///
/// | variable | shape here |
/// |---|---|
/// | `vft-plain` | plain special |
/// | `vft-loc` | buffer-local, local binding in this buffer |
/// | `vft-locd` | buffer-local, default here, local in `vft-other` |
/// | `vft-auto` | `make-variable-buffer-local`, default here, local in `vft-other` |
/// | `vft-lbool` | `DEFVAR_BOOL` made local here (BLV with a Boolean forwarder) |
/// | `vft-lint` | `DEFVAR_INT` made buffer-local, default here |
/// | `vft-lobj` | `DEFVAR_LISP` local only in `vft-other` |
/// | `vft-obj` / `vft-bool` / `vft-int` / `vft-kbd` | forwarded Obj / Bool / Int / Kboard |
/// | `vft-watched` | buffer-local, local here, with a variable watcher |
/// | `vft-alias` | alias of a buffer-local variable |
/// | `fill-column` | per-buffer slot |
/// | `case-fold-search` | whatever this build makes it |
/// | `gc-cons-threshold` | forwarded Int in the runtime projection mask |
/// | `inhibit-quit` | plain, host-projected |
/// | `buffer-undo-list` | plain, host-projected, per-buffer undo state |
const FIXTURE_VARS: &[&str] = &[
    "vft-plain",
    "vft-loc",
    "vft-locd",
    "vft-auto",
    "vft-lbool",
    "vft-lint",
    "vft-lobj",
    "vft-obj",
    "vft-bool",
    "vft-int",
    "vft-kbd",
    "vft-watched",
    "vft-alias",
    "fill-column",
    "case-fold-search",
    "gc-cons-threshold",
    "inhibit-quit",
    "buffer-undo-list",
];

fn fixture() -> Context {
    use crate::emacs_core::defvar_bool::ByteBooleanVars;
    use crate::emacs_core::forward::{alloc_kboard_objfwd, alloc_objfwd};
    let mut ev = Context::new();
    // Each transcript builds its own context, so its own heap: re-pin the
    // thread's JIT cache to it now, while no native leaf runs, rather than
    // letting a GC root walk inside a JIT program find the heap changed.
    #[cfg(feature = "jit")]
    {
        crate::emacs_core::jit::cache::clear();
        let _ = crate::emacs_core::jit::cache::prepopulate_aot_leaves(Vec::new());
    }
    for name in ["vft-obj", "vft-kbd", "vft-lobj"] {
        ev.obarray.intern(name);
    }
    ev.obarray
        .install_objfwd(intern("vft-obj"), alloc_objfwd(Value::symbol("obj0")));
    ev.obarray.install_kboard_objfwd(
        intern("vft-kbd"),
        alloc_kboard_objfwd(Value::symbol("kbd0")),
    );
    ev.obarray
        .install_objfwd(intern("vft-lobj"), alloc_objfwd(Value::symbol("lobj0")));
    ev.obarray
        .define_bool_variable("vft-bool", false, ByteBooleanVars::ErasedByLreadInit);
    ev.obarray
        .define_bool_variable("vft-lbool", true, ByteBooleanVars::ErasedByLreadInit);
    ev.obarray.define_int_variable("vft-int", 7);
    ev.obarray.define_int_variable("vft-lint", 8);
    eval_ok(
        &mut ev,
        "(progn
           (defvar vft-home (current-buffer))
           (defvar vft-other (get-buffer-create \" vft-other\"))
           (defvar vft-log nil)
           (defvar vft-body-result nil)
           (fset 'vft-watcher
                 (lambda (sym newval op where)
                   (setq vft-log (cons (list sym op newval
                                             (cond ((eq where vft-home) 'home)
                                                   ((eq where vft-other) 'other)
                                                   (t where)))
                                       vft-log))))
           (defvar vft-plain 1)
           (defvar vft-loc 10)
           (make-local-variable 'vft-loc)
           (setq vft-loc 11)
           (defvar vft-locd 20)
           (save-current-buffer (set-buffer vft-other)
             (set (make-local-variable 'vft-locd) 21))
           (defvar vft-auto 30)
           (make-variable-buffer-local 'vft-auto)
           (save-current-buffer (set-buffer vft-other) (setq vft-auto 31))
           (make-local-variable 'vft-lbool)
           (make-variable-buffer-local 'vft-lint)
           (save-current-buffer (set-buffer vft-other)
             (set (make-local-variable 'vft-lobj) 'lobj-other))
           (defvar vft-watched 40)
           (make-local-variable 'vft-watched)
           (setq vft-watched 41)
           (add-variable-watcher 'vft-watched 'vft-watcher)
           (defvar vft-base 50)
           (make-local-variable 'vft-base)
           (setq vft-base 51)
           (defvaralias 'vft-alias 'vft-base)
           (setq buffer-undo-list nil))",
    );
    ev
}

/// A let-body scenario: what `vft-body` does with VAR inside the binding.
#[derive(Clone, Copy, Debug)]
enum Body {
    /// Just look.
    Observe,
    /// Switch to the other buffer, so the unbind runs there.
    SetBuffer,
    /// `kill-local-variable` inside the binding: the kill wins over the
    /// restore (GNU `do_one_unbind`'s `Flocal_variable_p` test).
    KillLocal,
    /// Make the variable local and set it inside the binding.
    MakeLocal,
    /// Add a watcher inside the binding, so the unbind is watched.
    Watch,
    /// `kill-all-local-variables` inside the binding.
    KillAll,
    /// A `setq` of the variable inside the binding (tree walker).
    Setq,
}

fn body_form(body: Body, var: &str) -> String {
    let obs = observe_form(var);
    match body {
        Body::Observe => obs,
        Body::SetBuffer => format!("(progn (set-buffer vft-other) {obs})"),
        Body::KillLocal => format!("(progn (kill-local-variable '{var}) {obs})"),
        Body::MakeLocal => {
            format!("(progn (make-local-variable '{var}) (setq {var} 400) {obs})")
        }
        Body::Watch => format!("(progn (add-variable-watcher '{var} 'vft-watcher) {obs})"),
        Body::KillAll => format!("(progn (kill-all-local-variables) {obs})"),
        Body::Setq => format!("(progn (setq {var} 500) {obs})"),
    }
}

/// One program applied to each fixture variable in turn.
#[derive(Clone, Copy, Debug)]
enum Scenario {
    Read,
    /// `setq` to a value every shape accepts (an integer).
    SetqInt,
    /// `setq` to a value only some accept (a string: `DEFVAR_INT` signals,
    /// `DEFVAR_BOOL` coerces to `t`).
    SetqString,
    /// `setq` to nil (`DEFVAR_BOOL` stores nil, `DEFVAR_INT` signals).
    SetqNil,
    Let(Body),
    /// `let` to a string: `DEFVAR_INT` signals before the body.
    LetString,
    /// `let` to 5: `DEFVAR_BOOL` binds `t`.
    LetFive,
    /// `varset` inside the binding (bytecode, so the `setq` tier runs under a
    /// `let`: `local_if_set` must not auto-create while a default binding
    /// shadows the buffer, GNU `let_shadows_buffer_binding_p`).
    LetThenSetq,
    /// Two bindings of the variable, one `unbind 2`.
    LetNested,
}

const SCENARIOS: &[Scenario] = &[
    Scenario::Read,
    Scenario::SetqInt,
    Scenario::SetqString,
    Scenario::SetqNil,
    Scenario::Let(Body::Observe),
    Scenario::Let(Body::SetBuffer),
    Scenario::Let(Body::KillLocal),
    Scenario::Let(Body::MakeLocal),
    Scenario::Let(Body::Watch),
    Scenario::Let(Body::KillAll),
    Scenario::Let(Body::Setq),
    Scenario::LetString,
    Scenario::LetFive,
    Scenario::LetThenSetq,
    Scenario::LetNested,
];

/// Run SCENARIO over every fixture variable on ENGINE in a fresh fixture;
/// one transcript line per variable.
fn transcript(engine: Engine, scenario: Scenario) -> Vec<String> {
    let mut ev = fixture();
    let mut lines = Vec::new();
    for &var in FIXTURE_VARS {
        let (prog, args): (Prog, Vec<Value>) = match scenario {
            Scenario::Read => (Prog::read(), vec![]),
            Scenario::SetqInt => (Prog::setq(), vec![Value::make_int(100)]),
            Scenario::SetqString => (Prog::setq(), vec![Value::string("s")]),
            Scenario::SetqNil => (Prog::setq(), vec![Value::NIL]),
            Scenario::Let(body) => {
                eval_ok(
                    &mut ev,
                    &format!("(fset 'vft-body (lambda () {}))", body_form(body, var)),
                );
                (Prog::let_call(), vec![Value::make_int(200)])
            }
            Scenario::LetString => {
                eval_ok(
                    &mut ev,
                    &format!("(fset 'vft-body (lambda () {}))", observe_form(var)),
                );
                (Prog::let_call(), vec![Value::string("s")])
            }
            Scenario::LetFive => {
                eval_ok(
                    &mut ev,
                    &format!("(fset 'vft-body (lambda () {}))", observe_form(var)),
                );
                (Prog::let_call(), vec![Value::make_int(5)])
            }
            Scenario::LetThenSetq => {
                eval_ok(
                    &mut ev,
                    &format!("(fset 'vft-body (lambda () {}))", observe_form(var)),
                );
                (
                    Prog::let_setq_call(),
                    vec![Value::make_int(200), Value::make_int(300)],
                )
            }
            Scenario::LetNested => {
                eval_ok(
                    &mut ev,
                    &format!("(fset 'vft-body (lambda () {}))", observe_form(var)),
                );
                (
                    Prog::let_nested_call(),
                    vec![Value::make_int(1), Value::make_int(2)],
                )
            }
        };
        let depth = ev.specpdl.len();
        let result = run(&mut ev, engine, &prog, var, &args);
        let depth_after = ev.specpdl.len();
        if depth_after > depth {
            ev.unbind_to(depth);
        }
        let after = eval(&mut ev, &observe_form(var));
        let log = eval(&mut ev, "(prog1 (reverse vft-log) (setq vft-log nil))");
        lines.push(format!(
            "{var}: {result} => {after} log={log} specpdl={}",
            if depth_after == depth {
                "same".to_string()
            } else {
                format!("{:+}", depth_after as isize - depth as isize)
            }
        ));
        eval_ok(&mut ev, "(set-buffer vft-home)");
    }
    lines
}

fn line<'a>(lines: &'a [String], var: &str) -> &'a str {
    let prefix = format!("{var}: ");
    lines
        .iter()
        .find(|l| l.starts_with(&prefix))
        .unwrap_or_else(|| panic!("no transcript line for {var}"))
}

/// Every scenario reads, writes and binds each shape the same way on both
/// engines.
fn assert_engines_agree(scenario: Scenario) -> Vec<String> {
    let base = transcript(Engine::Interpreter, scenario);
    for &engine in &ENGINES[1..] {
        let other = transcript(engine, scenario);
        for (a, b) in base.iter().zip(&other) {
            assert_eq!(a, b, "{scenario:?}: interpreter vs {engine:?}");
        }
    }
    base
}

#[test]
fn fixture_shapes_are_what_the_scenarios_assume() {
    use crate::emacs_core::forward::LispFwdType;
    use crate::emacs_core::symbol::SymbolRedirect;
    let ev = fixture();
    let shape = |name: &str| {
        let id = intern(name);
        let sym = ev.obarray.get_by_id(id).expect("fixture symbol");
        let redirect = sym.redirect();
        let fwd = match redirect {
            SymbolRedirect::Forwarded => ev.obarray.forward_type(id),
            SymbolRedirect::Localized => ev.obarray.blv(id).and_then(|b| b.fwd).map(|f| f.ty),
            _ => None,
        };
        (redirect, fwd)
    };
    use LispFwdType as F;
    use SymbolRedirect as R;
    for (name, want) in [
        ("vft-plain", (R::Plainval, None)),
        ("vft-loc", (R::Localized, None)),
        ("vft-locd", (R::Localized, None)),
        ("vft-auto", (R::Localized, None)),
        ("vft-lbool", (R::Localized, Some(F::Bool))),
        ("vft-lint", (R::Localized, Some(F::Int))),
        ("vft-lobj", (R::Localized, Some(F::Obj))),
        ("vft-obj", (R::Forwarded, Some(F::Obj))),
        ("vft-bool", (R::Forwarded, Some(F::Bool))),
        ("vft-int", (R::Forwarded, Some(F::Int))),
        ("vft-kbd", (R::Forwarded, Some(F::KboardObj))),
        ("vft-watched", (R::Localized, None)),
        ("vft-alias", (R::Varalias, None)),
        ("fill-column", (R::Forwarded, Some(F::BufferObj))),
        ("gc-cons-threshold", (R::Forwarded, Some(F::Int))),
        ("inhibit-quit", (R::Forwarded, Some(F::Obj))),
        ("case-fold-search", (R::Localized, Some(F::Obj))),
        ("buffer-undo-list", (R::Plainval, None)),
    ] {
        assert_eq!(shape(name), want, "{name}");
    }
    assert!(ev.runtime_binding_has_projection(intern("gc-cons-threshold")));
    assert!(ev.runtime_binding_has_projection(intern("inhibit-quit")));
}

#[test]
fn reads_agree_on_both_engines() {
    let lines = assert_engines_agree(Scenario::Read);
    assert!(line(&lines, "vft-loc").starts_with("vft-loc: 11 =>"));
    assert!(line(&lines, "vft-locd").starts_with("vft-locd: 20 =>"));
    assert!(line(&lines, "vft-lbool").starts_with("vft-lbool: t =>"));
    assert!(line(&lines, "vft-obj").starts_with("vft-obj: obj0 =>"));
    assert!(line(&lines, "vft-int").starts_with("vft-int: 7 =>"));
    assert!(line(&lines, "vft-kbd").starts_with("vft-kbd: kbd0 =>"));
    assert!(line(&lines, "vft-alias").starts_with("vft-alias: 51 =>"));
}

#[test]
fn setq_agrees_on_both_engines() {
    let lines = assert_engines_agree(Scenario::SetqInt);
    // A local binding is written in place; the default is untouched.
    assert!(
        line(&lines, "vft-loc").starts_with("vft-loc: 100 => (100 10 t (10 nil) home)"),
        "{}",
        line(&lines, "vft-loc")
    );
    // `make-variable-buffer-local`: the first `setq` creates the local.
    assert!(
        line(&lines, "vft-auto").starts_with("vft-auto: 100 => (100 30 t (31 t) home)"),
        "{}",
        line(&lines, "vft-auto")
    );
    // Not `local_if_set`: the default is written.
    assert!(
        line(&lines, "vft-locd").starts_with("vft-locd: 100 => (100 100 nil (21 t) home)"),
        "{}",
        line(&lines, "vft-locd")
    );
    // The watcher sees the write with the buffer as WHERE.
    assert!(
        line(&lines, "vft-watched").contains("log=((vft-watched set 100 home))"),
        "{}",
        line(&lines, "vft-watched")
    );
}

#[test]
fn setq_type_rules_agree_on_both_engines() {
    let lines = assert_engines_agree(Scenario::SetqString);
    assert!(
        line(&lines, "vft-int").starts_with("vft-int: ERR wrong-type-argument integerp \"s\""),
        "{}",
        line(&lines, "vft-int")
    );
    assert!(
        line(&lines, "vft-lint").starts_with("vft-lint: ERR wrong-type-argument integerp \"s\""),
        "{}",
        line(&lines, "vft-lint")
    );
    assert!(line(&lines, "vft-bool").starts_with("vft-bool: t =>"));
    assert!(line(&lines, "vft-lbool").starts_with("vft-lbool: t =>"));
    let lines = assert_engines_agree(Scenario::SetqNil);
    assert!(line(&lines, "vft-bool").starts_with("vft-bool: nil =>"));
    assert!(line(&lines, "vft-lbool").starts_with("vft-lbool: nil =>"));
    assert!(line(&lines, "vft-int").starts_with("vft-int: ERR wrong-type-argument"));
}

#[test]
fn let_observe_agrees_on_both_engines() {
    let lines = assert_engines_agree(Scenario::Let(Body::Observe));
    // A local binding is bound and restored in place.
    assert!(
        line(&lines, "vft-loc").starts_with("vft-loc: ((200 10 t (10 nil) home) 11) => (11 10 t"),
        "{}",
        line(&lines, "vft-loc")
    );
    // No local binding: the default is bound, and the other buffer's local
    // is unaffected.
    assert!(
        line(&lines, "vft-locd")
            .starts_with("vft-locd: ((200 200 nil (21 t) home) 20) => (20 20 nil (21 t) home)"),
        "{}",
        line(&lines, "vft-locd")
    );
    // `let` never auto-creates, even for `make-variable-buffer-local`.
    assert!(
        line(&lines, "vft-auto")
            .starts_with("vft-auto: ((200 200 nil (31 t) home) 30) => (30 30 nil (31 t) home)"),
        "{}",
        line(&lines, "vft-auto")
    );
    assert!(
        line(&lines, "vft-watched")
            .contains("log=((vft-watched let 200 home) (vft-watched unlet 41 home))"),
        "{}",
        line(&lines, "vft-watched")
    );
    for var in FIXTURE_VARS {
        assert!(
            line(&lines, var).ends_with("specpdl=same"),
            "{}",
            line(&lines, var)
        );
    }
}

#[test]
fn let_with_buffer_switch_agrees_on_both_engines() {
    let lines = assert_engines_agree(Scenario::Let(Body::SetBuffer));
    // The local binding made in the home buffer is restored in the home
    // buffer, although the unbind runs in the other one.
    assert!(
        line(&lines, "vft-loc").contains("=> (10 10 nil (10 nil) (11 t))"),
        "{}",
        line(&lines, "vft-loc")
    );
    // A default binding is restored whatever buffer is current.
    assert!(
        line(&lines, "vft-locd").contains("=> (21 20 t (21 t) (20 nil))"),
        "{}",
        line(&lines, "vft-locd")
    );
}

#[test]
fn let_with_kill_local_agrees_on_both_engines() {
    let lines = assert_engines_agree(Scenario::Let(Body::KillLocal));
    // The kill wins: the unbind does not resurrect the local binding.
    assert!(
        line(&lines, "vft-loc").contains("=> (10 10 nil (10 nil) home)"),
        "{}",
        line(&lines, "vft-loc")
    );
}

#[test]
fn let_with_watcher_added_inside_agrees_on_both_engines() {
    let lines = assert_engines_agree(Scenario::Let(Body::Watch));
    // A watcher added inside the binding sees the unbind.
    assert!(
        line(&lines, "vft-loc").contains("log=((vft-loc unlet 11 home))"),
        "{}",
        line(&lines, "vft-loc")
    );
    assert!(
        // A forwarded `let` unwinds through GNU `set_default_internal`, which
        // notifies once as `set` (its `set-default` phase, WHERE nil) and
        // once more from `set_internal` as `unlet`.
        line(&lines, "vft-obj").contains("log=((vft-obj set obj0 nil) (vft-obj unlet obj0 nil))"),
        "{}",
        line(&lines, "vft-obj")
    );
}

#[test]
fn let_with_setq_inside_agrees_on_both_engines() {
    let _ = assert_engines_agree(Scenario::Let(Body::Setq));
    let lines = assert_engines_agree(Scenario::LetThenSetq);
    // A `setq` under a default binding writes the default and does not
    // auto-create a local (GNU `let_shadows_buffer_binding_p`).
    assert!(
        line(&lines, "vft-auto")
            .starts_with("vft-auto: ((300 300 nil (31 t) home) 30) => (30 30 nil (31 t) home)"),
        "{}",
        line(&lines, "vft-auto")
    );
}

#[test]
fn let_type_rules_agree_on_both_engines() {
    let lines = assert_engines_agree(Scenario::LetString);
    assert!(
        line(&lines, "vft-int").starts_with("vft-int: ERR wrong-type-argument integerp \"s\""),
        "{}",
        line(&lines, "vft-int")
    );
    assert!(
        line(&lines, "vft-int").contains("=> (7 7 nil"),
        "{}",
        line(&lines, "vft-int")
    );
    let lines = assert_engines_agree(Scenario::LetFive);
    assert!(
        line(&lines, "vft-bool").starts_with("vft-bool: ((t t nil (t nil) home) nil)"),
        "{}",
        line(&lines, "vft-bool")
    );
    assert!(
        line(&lines, "vft-lbool").starts_with("vft-lbool: ((t t t"),
        "{}",
        line(&lines, "vft-lbool")
    );
}

#[test]
fn nested_let_agrees_on_both_engines() {
    let lines = assert_engines_agree(Scenario::LetNested);
    assert!(
        line(&lines, "vft-loc").starts_with("vft-loc: ((2 10 t (10 nil) home) 11) => (11 10 t"),
        "{}",
        line(&lines, "vft-loc")
    );
    assert!(
        line(&lines, "vft-obj").starts_with("vft-obj: ((2 2 nil (2 nil) home) obj0)"),
        "{}",
        line(&lines, "vft-obj")
    );
}

/// Every scenario, including those without a rule of their own above.
#[test]
fn every_scenario_agrees_on_both_engines() {
    for &scenario in SCENARIOS {
        let _ = assert_engines_agree(scenario);
    }
}

// ---------------------------------------------------------------------------
// The cached tiers (P1.4 Stage A)
// ---------------------------------------------------------------------------

/// SCENARIO's transcript on ENGINE with exactly TIERS enabled.
fn transcript_with(engine: Engine, scenario: Scenario, tiers: &[VarCacheTier]) -> Vec<String> {
    set_var_cache_tiers_for_test(tiers);
    transcript(engine, scenario)
}

/// The cached tiers change nothing Lisp can see: on each engine, SCENARIO's
/// transcript with every tier on is the transcript with every tier off (the
/// general paths alone).
fn assert_tiers_change_nothing(scenario: Scenario) {
    for &engine in ENGINES {
        let off = transcript_with(engine, scenario, &[]);
        let on = transcript_with(engine, scenario, &VarCacheTier::ALL);
        for (a, b) in off.iter().zip(&on) {
            assert_eq!(a, b, "{scenario:?} on {engine:?}: tiers off vs on");
        }
    }
}

#[test]
fn cached_tiers_change_no_transcript() {
    for &scenario in SCENARIOS {
        assert_tiers_change_nothing(scenario);
    }
}

#[test]
fn var_cache_knob_parses_every_spelling() {
    let all = parse_var_cache_knob(None);
    assert_eq!(parse_var_cache_knob(Some("1")), all);
    assert_eq!(parse_var_cache_knob(Some("on")), all);
    assert_eq!(parse_var_cache_knob(Some("all")), all);
    assert_eq!(parse_var_cache_knob(Some("0")), 0);
    assert_eq!(parse_var_cache_knob(Some("off")), 0);
    assert_eq!(parse_var_cache_knob(Some("none")), 0);
    let read_set = parse_var_cache_knob(Some("read,set"));
    assert_ne!(read_set, 0);
    assert_ne!(read_set, all);
    assert_eq!(
        parse_var_cache_knob(Some("read, set,bogus")),
        read_set,
        "an unknown word is ignored"
    );
    assert_eq!(
        parse_var_cache_knob(Some("read,set,bind,unbind")),
        all,
        "the four tiers are all of them"
    );
}

/// `read_var_cached` answers every buffer-local variable whose cache is
/// loaded for the current buffer, every forwarder and every per-buffer slot
/// exactly as the general path reads it, and refuses the rest: plain and
/// aliased symbols, a cache loaded for another buffer or before a structural
/// alist change, a void binding, and everything while its tier is off.
#[test]
fn read_tier_answers_cached_shapes_and_refuses_the_rest() {
    let mut ev = fixture();
    set_var_cache_tiers_for_test(&VarCacheTier::ALL);
    // The general path reads (and swaps in) each variable first.
    let general: Vec<(&str, String)> = FIXTURE_VARS
        .iter()
        .map(|&var| {
            (
                var,
                run(&mut ev, Engine::Interpreter, &Prog::read(), var, &[]),
            )
        })
        .collect();
    reset_var_cache_events();
    let cached =
        |ev: &Context, name: &str| ev.read_var_cached(intern(name)).map(|v| print_value(&v));
    for (var, want) in &general {
        match *var {
            "vft-plain" | "vft-alias" | "buffer-undo-list" => {
                assert_eq!(cached(&ev, var), None, "{var}: not a cached shape")
            }
            _ => assert_eq!(cached(&ev, var).as_deref(), Some(want.as_str()), "{var}"),
        }
    }
    // BLV hits: vft-loc, -locd, -auto, -lbool, -lint, -lobj, -watched and
    // case-fold-search; forwarders: vft-obj, -bool, -int, -kbd,
    // gc-cons-threshold, inhibit-quit; one per-buffer slot.
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadLocalized), 8);
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadForwarded), 6);
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadBufferSlot), 1);
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadRefused), 0);
    // A cache loaded for another buffer is a miss; the general path's swap-in
    // makes the next read a hit.
    eval_ok(&mut ev, "(set-buffer vft-other)");
    assert_eq!(cached(&ev, "vft-locd"), None);
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadRefused), 1);
    assert_eq!(eval(&mut ev, "vft-locd"), "21");
    assert_eq!(cached(&ev, "vft-locd").as_deref(), Some("21"));
    // A structural alist change (the kill bumps the epoch) is a miss.
    eval_ok(&mut ev, "(kill-local-variable 'vft-locd)");
    assert_eq!(cached(&ev, "vft-locd"), None);
    assert_eq!(eval(&mut ev, "vft-locd"), "20");
    // A void binding is refused: the general path signals.
    eval_ok(&mut ev, "(set-buffer vft-home)");
    eval_ok(&mut ev, "(makunbound 'vft-loc)");
    let _ = eval(&mut ev, "(condition-case nil vft-loc (void-variable nil))");
    if cached(&ev, "vft-loc").is_some() {
        assert_eq!(
            cached(&ev, "vft-loc"),
            Some(run(
                &mut ev,
                Engine::Interpreter,
                &Prog::read(),
                "vft-loc",
                &[]
            )),
            "a cached answer must be the general path's"
        );
    } else {
        assert!(
            run(&mut ev, Engine::Interpreter, &Prog::read(), "vft-loc", &[])
                .starts_with("ERR void-variable")
        );
    }
    // Tier off: nothing is answered.
    set_var_cache_tiers_for_test(&[VarCacheTier::Set, VarCacheTier::Bind, VarCacheTier::Unbind]);
    assert_eq!(cached(&ev, "vft-obj"), None);
    assert_eq!(cached(&ev, "vft-lbool"), None);
    assert!(var_cache_census_report("test").contains("enabled: set,bind,unbind"));
}

/// The JIT reads buffer-local and forwarded variables through the read tier
/// (the interpreter keeps its own opcode arm), with the answers the
/// interpreter gives.
#[cfg(feature = "jit")]
#[test]
fn jit_reads_take_the_read_tier() {
    let mut ev = fixture();
    set_var_cache_tiers_for_test(&VarCacheTier::ALL);
    // Load every BLV cache for this buffer through the general path.
    let general: Vec<String> = FIXTURE_VARS
        .iter()
        .map(|&var| run(&mut ev, Engine::Interpreter, &Prog::read(), var, &[]))
        .collect();
    reset_var_cache_events();
    let again: Vec<String> = FIXTURE_VARS
        .iter()
        .map(|&var| run(&mut ev, Engine::Interpreter, &Prog::read(), var, &[]))
        .collect();
    assert_eq!(general, again);
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadLocalized), 0);
    let native: Vec<String> = FIXTURE_VARS
        .iter()
        .map(|&var| run(&mut ev, Engine::Jit, &Prog::read(), var, &[]))
        .collect();
    assert_eq!(general, native);
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadLocalized), 8);
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadForwarded), 6);
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadBufferSlot), 1);
    assert_eq!(var_cache_event_count(VarCacheEvent::ReadRefused), 0);
}

/// `try_set_var_cached` stores every buffer-local variable whose cache is
/// loaded for the current buffer (its own binding, or the default of a
/// variable that is not `local_if_set`) and every forwarder that holds its
/// own value, through the type rule; and refuses, storing nothing, every
/// shape the general path treats specially.
#[test]
fn set_tier_stores_cached_shapes_and_refuses_the_rest() {
    let mut ev = fixture();
    set_var_cache_tiers_for_test(&VarCacheTier::ALL);
    for &var in FIXTURE_VARS {
        let _ = run(&mut ev, Engine::Interpreter, &Prog::read(), var, &[]);
    }
    reset_var_cache_events();
    let set =
        |ev: &mut Context, name: &str, value: Value| ev.try_set_var_cached(intern(name), value);
    let n = Value::make_int(100);
    // Stored.
    for (var, want) in [
        ("vft-loc", "(100 10 t (10 nil) home)"),
        ("vft-locd", "(100 100 nil (21 t) home)"),
        ("vft-lbool", "(t t t (t nil) home)"),
        ("vft-lobj", "(100 100 nil (lobj-other t) home)"),
        ("vft-obj", "(100 100 nil (100 nil) home)"),
        ("vft-bool", "(t t nil (t nil) home)"),
        ("vft-int", "(100 100 nil (100 nil) home)"),
        ("vft-kbd", "(100 100 nil (100 nil) home)"),
    ] {
        assert!(set(&mut ev, var, n), "{var}: a cached shape");
        assert_eq!(eval(&mut ev, &observe_form(var)), want, "{var}");
    }
    assert_eq!(var_cache_event_count(VarCacheEvent::SetLocalizedFound), 2);
    assert_eq!(var_cache_event_count(VarCacheEvent::SetLocalizedDefault), 2);
    assert_eq!(var_cache_event_count(VarCacheEvent::SetForwarded), 4);
    // Refused, nothing stored: the general path's specials.
    let before: Vec<String> = FIXTURE_VARS
        .iter()
        .map(|v| eval(&mut ev, &observe_form(v)))
        .collect();
    for var in [
        "vft-plain",         // plain: try_set_plain_variable's
        "vft-auto",          // local_if_set, no binding: auto-create
        "vft-lint",          // likewise, with an Int forwarder
        "vft-watched",       // a watcher
        "vft-alias",         // an alias
        "fill-column",       // a per-buffer slot
        "case-fold-search",  // local_if_set, no binding here
        "gc-cons-threshold", // republished to the GC pacer
        "inhibit-quit",      // host-projected
        "buffer-undo-list",  // plain, host-projected
    ] {
        assert!(!set(&mut ev, var, n), "{var}: the general path's");
    }
    // Type rules the general path signals for.
    assert!(!set(&mut ev, "vft-int", Value::string("s")));
    assert!(!set(&mut ev, "vft-lint", Value::string("s")));
    let after: Vec<String> = FIXTURE_VARS
        .iter()
        .map(|v| eval(&mut ev, &observe_form(v)))
        .collect();
    assert_eq!(before, after, "a refusal stores nothing");
    assert_eq!(var_cache_event_count(VarCacheEvent::SetRefused), 7);
    // The observations above ended in the other buffer, so every BLV is now
    // loaded for it: a miss here, refused.
    assert!(!set(&mut ev, "vft-loc", n));
    assert_eq!(var_cache_event_count(VarCacheEvent::SetRefused), 8);
    assert_eq!(eval(&mut ev, "vft-loc"), "100");
    // Tier off.
    set_var_cache_tiers_for_test(&[VarCacheTier::Read]);
    assert!(!set(&mut ev, "vft-obj", n));
}

/// Both engines' `varset` takes the set tier.
#[test]
fn bytecode_setq_takes_the_set_tier_on_both_engines() {
    for &engine in ENGINES {
        let mut ev = fixture();
        set_var_cache_tiers_for_test(&VarCacheTier::ALL);
        for &var in FIXTURE_VARS {
            let _ = run(&mut ev, Engine::Interpreter, &Prog::read(), var, &[]);
        }
        reset_var_cache_events();
        for var in ["vft-loc", "vft-locd", "vft-obj", "vft-bool", "vft-int"] {
            let got = run(&mut ev, engine, &Prog::setq(), var, &[Value::make_int(3)]);
            assert!(got == "3" || got == "t", "{engine:?} {var}: {got}");
        }
        assert_eq!(
            var_cache_event_count(VarCacheEvent::SetLocalizedFound),
            1,
            "{engine:?}"
        );
        assert_eq!(
            var_cache_event_count(VarCacheEvent::SetLocalizedDefault),
            1,
            "{engine:?}"
        );
        assert_eq!(
            var_cache_event_count(VarCacheEvent::SetForwarded),
            3,
            "{engine:?}"
        );
    }
}

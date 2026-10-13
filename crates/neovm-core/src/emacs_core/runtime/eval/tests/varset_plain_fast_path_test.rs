//! `setq` of a special variable (`Op::VarSet`, GNU `Bvarset`) through the
//! plain-cell fast path, `Context::try_set_plain_variable`.
//!
//! The fast path is one store; the general path is ~800 instructions. Every
//! shape the general path treats specially must be refused by the fast path,
//! or a write silently skips a constant check, a watcher, an alias, a
//! buffer-local cell or a host projection. Each case below writes through
//! bytecode `VarSet` on BOTH the interpreter and the JIT baseline shim, and
//! checks what Lisp observes afterwards.

use crate::emacs_core::bytecode::{ByteCodeFunction, Vm};
use crate::emacs_core::error::{FlowKind, FlowResultExt};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::{LambdaParams, Value};

/// `(lambda (v) (setq SYM v) nil)` as bytecode.
fn setter(sym: Value) -> (Vec<crate::emacs_core::bytecode::opcode::Op>, Vec<Value>) {
    use crate::emacs_core::bytecode::opcode::Op;
    (
        vec![Op::StackRef(0), Op::VarSet(0), Op::Nil, Op::Return],
        vec![sym],
    )
}

#[derive(Clone, Copy, Debug)]
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

/// Run `(setq SYM value)` on `engine`; `Err(signal name)` if it signals.
fn run_setq(ev: &mut Context, engine: Engine, sym: Value, value: Value) -> Result<(), String> {
    let (ops, constants) = setter(sym);
    match engine {
        Engine::Interpreter => {
            let mut f = ByteCodeFunction::new(LambdaParams {
                required: vec![intern("v")],
                optional: Vec::new(),
                rest: None,
            });
            f.lexical = true;
            f.ops = ops;
            f.constants = constants.into();
            f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
            let mut vm = Vm::from_context(ev);
            match vm.execute(&f, vec![value]).kinded() {
                Ok(_) => Ok(()),
                Err(FlowKind::Signal(sig)) => Err(sig.symbol_name().to_string()),
                Err(other) => Err(format!("{other:?}")),
            }
        }
        #[cfg(feature = "jit")]
        Engine::Jit => {
            use crate::emacs_core::jit::compile::{NativeRun, lower_leaf, take_pending_flow};
            let leaf = lower_leaf(&ops, &constants, 1).expect("setter lowers");
            let ctx = ev as *mut Context as *mut u8;
            match leaf.call(ctx, &[value]) {
                NativeRun::Ok(_) => Ok(()),
                NativeRun::Signal => {
                    match take_pending_flow().map(crate::emacs_core::error::Flow::into_kind) {
                        Some(FlowKind::Signal(sig)) => Err(sig.symbol_name().to_string()),
                        other => Err(format!("{other:?}")),
                    }
                }
                other => Err(format!("unexpected {other:?}")),
            }
        }
    }
}

/// Run `(setq SYM value)` on `engine` and return how many plain value cells
/// were written through the store-only path. The general path writes through
/// `set_symbol_value_id` and never counts, so 1 means the fast path ran — at
/// the call site the engine uses, since `assign_var_id` does not offer it.
fn fast_path_visits(ev: &mut Context, engine: Engine, sym: Value, value: Value) -> usize {
    use crate::emacs_core::symbol::{plain_value_slot_visits, reset_plain_value_slot_visits};
    reset_plain_value_slot_visits();
    run_setq(ev, engine, sym, value).expect("setq succeeds");
    plain_value_slot_visits()
}

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(v) => print_value(&v),
        Err(e) => format!("ERR {e:?}"),
    }
}

#[test]
fn a_plain_special_takes_the_fast_path_and_is_set() {
    let mut ev = Context::new();
    eval(&mut ev, "(defvar vfp-plain 0)");
    let sym = Value::symbol("vfp-plain");
    assert!(
        ev.try_set_plain_variable(intern("vfp-plain"), Value::make_int(1)),
        "a plain defvar is the fast path's shape"
    );
    for (i, &engine) in ENGINES.iter().enumerate() {
        let n = 10 + i as i64;
        assert_eq!(
            fast_path_visits(&mut ev, engine, sym, Value::make_int(n)),
            1,
            "{engine:?}: a plain special is written by the store-only path"
        );
        assert_eq!(eval(&mut ev, "vfp-plain"), n.to_string(), "{engine:?}");
    }
    // An unbound special is still a plain cell: the store binds it.
    eval(&mut ev, "(defvar vfp-unbound)");
    for &engine in ENGINES {
        eval(&mut ev, "(makunbound 'vfp-unbound)");
        assert_eq!(
            fast_path_visits(
                &mut ev,
                engine,
                Value::symbol("vfp-unbound"),
                Value::make_int(3)
            ),
            1,
            "{engine:?}: an unbound plain special too"
        );
        assert_eq!(eval(&mut ev, "vfp-unbound"), "3", "{engine:?}");
    }
    // A let binding is undone by the unbind, whatever the setq inside did.
    for &engine in ENGINES {
        eval(&mut ev, "(setq vfp-plain 1)");
        let depth = ev.specpdl.len();
        ev.try_specbind(intern("vfp-plain"), Value::make_int(2))
            .expect("let");
        assert_eq!(
            fast_path_visits(&mut ev, engine, sym, Value::make_int(7)),
            1,
            "{engine:?}: and under a let"
        );
        assert_eq!(eval(&mut ev, "vfp-plain"), "7", "{engine:?}");
        ev.unbind_to(depth);
        assert_eq!(
            eval(&mut ev, "vfp-plain"),
            "1",
            "{engine:?}: the let restores"
        );
    }
}

#[test]
fn constants_keep_the_setting_constant_rule() {
    let mut ev = Context::new();
    for &engine in ENGINES {
        assert_eq!(
            run_setq(&mut ev, engine, Value::NIL, Value::make_int(1)),
            Err("setting-constant".to_string()),
            "{engine:?}: (setq nil 1)"
        );
        assert_eq!(
            run_setq(&mut ev, engine, Value::T, Value::make_int(1)),
            Err("setting-constant".to_string()),
            "{engine:?}: (setq t 1)"
        );
        let kw = Value::keyword("vfp-key");
        // A keyword given a property still has an obarray slot; it must stay
        // a constant.
        eval(&mut ev, "(put :vfp-key 'vfp-prop 1)");
        assert!(!ev.try_set_plain_variable(intern(":vfp-key"), Value::make_int(1)));
        assert_eq!(
            run_setq(&mut ev, engine, kw, Value::make_int(1)),
            Err("setting-constant".to_string()),
            "{engine:?}: (setq :vfp-key 1)"
        );
        assert_eq!(
            run_setq(&mut ev, engine, kw, kw),
            Ok(()),
            "{engine:?}: a keyword set to itself is a silent no-op"
        );
        eval(&mut ev, "(defconst vfp-const 5)");
        // `defconst` is not `SYMBOL_NOWRITE` in GNU either: setq succeeds.
        assert_eq!(
            run_setq(
                &mut ev,
                engine,
                Value::symbol("vfp-const"),
                Value::make_int(6)
            ),
            Ok(())
        );
    }
}

#[test]
fn watchers_aliases_and_buffer_locals_keep_the_general_path() {
    let mut ev = Context::new();
    eval(
        &mut ev,
        "(progn
           (defvar vfp-watched 0)
           (defvar vfp-events nil)
           (add-variable-watcher 'vfp-watched
             (lambda (sym newval op where) (setq vfp-events (cons (list sym newval op) vfp-events))))
           (defvar vfp-base 0)
           (defvaralias 'vfp-alias 'vfp-base)
           (defvar vfp-local 1))",
    );
    for (i, &engine) in ENGINES.iter().enumerate() {
        let n = 20 + i as i64;
        // Watched: the watcher sees the write.
        run_setq(
            &mut ev,
            engine,
            Value::symbol("vfp-watched"),
            Value::make_int(n),
        )
        .expect("setq");
        assert_eq!(
            eval(&mut ev, "(car vfp-events)"),
            format!("(vfp-watched {n} set)"),
            "{engine:?}: the watcher ran"
        );
        // Alias: the write lands on the target.
        assert_eq!(
            fast_path_visits(
                &mut ev,
                engine,
                Value::symbol("vfp-alias"),
                Value::make_int(n)
            ),
            0,
            "{engine:?}: an alias takes the general path"
        );
        assert_eq!(
            eval(&mut ev, "vfp-base"),
            n.to_string(),
            "{engine:?}: alias target"
        );
        // Buffer-local: the local cell changes, the default does not.
        eval(&mut ev, "(make-local-variable 'vfp-local)");
        run_setq(
            &mut ev,
            engine,
            Value::symbol("vfp-local"),
            Value::make_int(n),
        )
        .expect("setq");
        assert_eq!(
            eval(&mut ev, "vfp-local"),
            n.to_string(),
            "{engine:?}: local"
        );
        assert_eq!(
            eval(&mut ev, "(default-value 'vfp-local)"),
            "1",
            "{engine:?}: default"
        );
    }
    assert!(!ev.try_set_plain_variable(intern("vfp-watched"), Value::NIL));
    assert!(!ev.try_set_plain_variable(intern("vfp-alias"), Value::NIL));
    assert!(!ev.try_set_plain_variable(intern("vfp-local"), Value::NIL));
}

#[test]
fn host_projected_variables_are_still_republished() {
    let mut ev = Context::new();
    for &engine in ENGINES {
        // The cached quit state.
        assert_eq!(
            fast_path_visits(&mut ev, engine, Value::symbol("inhibit-quit"), Value::T),
            0,
            "{engine:?}: a projected variable takes the publishing path"
        );
        assert!(
            ev.inhibit_quit.is_truthy(),
            "{engine:?}: inhibit-quit republished"
        );
        run_setq(&mut ev, engine, Value::symbol("inhibit-quit"), Value::NIL).expect("setq");
        assert!(
            ev.inhibit_quit.is_nil(),
            "{engine:?}: inhibit-quit republished"
        );
        // The evaluator depth limit.
        run_setq(
            &mut ev,
            engine,
            Value::symbol("max-lisp-eval-depth"),
            Value::make_int(4321),
        )
        .expect("setq");
        assert_eq!(
            ev.max_depth, 4321,
            "{engine:?}: max-lisp-eval-depth republished"
        );
        // A display variable marks redisplay. `glyphless-char-display` is a
        // PLAIN cell (no flag excludes it), so only the projection test keeps
        // this write on the publishing path.
        assert!(
            ev.obarray
                .is_plain_value_cell_id(intern("glyphless-char-display"))
        );
        let before = ev.display_var_change_count;
        assert_eq!(
            fast_path_visits(
                &mut ev,
                engine,
                Value::symbol("glyphless-char-display"),
                Value::NIL
            ),
            0,
            "{engine:?}: a plain display variable takes the publishing path"
        );
        assert!(
            ev.display_var_change_count > before,
            "{engine:?}: a plain display variable's write reaches redisplay"
        );
        let before = ev.display_var_change_count;
        run_setq(
            &mut ev,
            engine,
            Value::symbol("truncate-partial-width-windows"),
            Value::make_int(50),
        )
        .expect("setq");
        assert!(
            ev.display_var_change_count > before,
            "{engine:?}: a display variable write reaches redisplay"
        );
    }
    for name in [
        "inhibit-quit",
        "quit-flag",
        "max-lisp-eval-depth",
        "gc-cons-threshold",
        "gc-cons-percentage",
        "input-decode-map",
        "local-function-key-map",
        "truncate-partial-width-windows",
        "glyphless-char-display",
        "buffer-undo-list",
    ] {
        assert!(
            !ev.try_set_plain_variable(intern(name), Value::NIL),
            "{name} must take the publishing path"
        );
    }
}

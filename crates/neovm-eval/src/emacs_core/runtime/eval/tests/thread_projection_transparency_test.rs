//! Phase 0 step 2: a thread-projected variable behaves exactly like the
//! unprojected variable for a single owner, its dynamic bindings living in
//! the owner's slab.
//!
//! Each case runs the same script of forms in fresh contexts -- one with the
//! variable unprojected, one projected, and one projected with a collection
//! before every step, so the slab's values must be rooted -- and compares
//! every result. The comparison is neomacs against itself, so no GNU oracle
//! is involved.

use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::{ByteCodeFunction, StackDepth, Vm};
use crate::emacs_core::defvar_bool::ByteBooleanVars;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::forward::alloc_objfwd;
use neovm_value::intern::intern;
use neovm_value::value::{LambdaParams, Value};

/// The variable shapes a projection can wrap, and how a context gets one.
#[derive(Clone, Copy, Debug)]
enum Shape {
    /// A `defvar`'d special with a value.
    Special,
    /// A `defvar`'d special with no value.
    Void,
    /// An undeclared global set with `setq`.
    Free,
    /// `make-variable-buffer-local`.
    AutoLocal,
    /// A global with a `make-local-variable` binding in another buffer.
    SomeLocal,
    /// `DEFVAR_INT`.
    Int,
    /// `DEFVAR_BOOL`.
    Bool,
    /// `DEFVAR_LISP`.
    Obj,
    /// `DEFVAR_PER_BUFFER` with a local flag (`fill-column`).
    PerBuffer,
}

const SHAPES: &[(Shape, &str)] = &[
    (Shape::Special, "tpx-special"),
    (Shape::Void, "tpx-void"),
    (Shape::Free, "tpx-free"),
    (Shape::AutoLocal, "tpx-auto"),
    (Shape::SomeLocal, "tpx-some"),
    (Shape::Int, "tpx-int"),
    (Shape::Bool, "tpx-bool"),
    (Shape::Obj, "tpx-obj"),
    (Shape::PerBuffer, "fill-column"),
];

fn context(shape: Shape, name: &str, projected: bool) -> Context {
    let mut ev = Context::new();
    match shape {
        Shape::Special => eval(&mut ev, &format!("(defvar {name} 5)")),
        Shape::Void => eval(&mut ev, &format!("(defvar {name})")),
        Shape::Free => eval(&mut ev, &format!("(setq {name} 5)")),
        Shape::AutoLocal => eval(
            &mut ev,
            &format!("(progn (defvar {name} 5) (make-variable-buffer-local '{name}))"),
        ),
        Shape::SomeLocal => eval(
            &mut ev,
            &format!(
                "(progn (defvar {name} 5)
                   (save-current-buffer
                     (set-buffer (get-buffer-create \" tpx-other\"))
                     (set (make-local-variable '{name}) 6)))"
            ),
        ),
        Shape::Int => ev.obarray.define_int_variable(name, 5),
        Shape::Bool => {
            ev.obarray
                .define_bool_variable(name, true, ByteBooleanVars::ErasedByLreadInit);
        }
        Shape::Obj => {
            ev.obarray.intern(name);
            ev.obarray
                .install_objfwd(intern(name), alloc_objfwd(Value::fixnum(5)));
        }
        Shape::PerBuffer => {}
    }
    eval(
        &mut ev,
        "(progn (defvar tpx-log nil)
                (get-buffer-create \" tpx-other\")
                (fset 'tpx-throw (lambda (&rest _) (throw 'tpx-w 'thrown)))
                (fset 'tpx-watch
                      (lambda (sym new op where)
                        (setq tpx-log (cons (list sym new op (bufferp where)) tpx-log)))))",
    );
    if projected {
        ev.project_thread_variable_for_test(intern(name))
            .unwrap_or_else(|| panic!("{name} projects"));
    }
    ev
}

fn eval(ev: &mut Context, src: &str) {
    ev.eval_str(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
}

fn answer(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(error) => format!("ERR {error:?}"),
    }
}

/// The forms each case runs, in order; `V` is the variable and `O` the
/// other buffer. They cover reads, `let`/`let*`, `setq`/`set`,
/// `set-default`, `makunbound`, `local-variable-p`, make/kill-local,
/// watchers (including one that throws out of a `let`), aliases, typed
/// stores, `backtrace-eval`, and specbind unwound by signal and throw.
const SCRIPT: &[&str] = &[
    "V",
    "(symbol-value 'V)",
    "(boundp 'V)",
    "(condition-case e (default-value 'V) (error (car e)))",
    "(default-boundp 'V)",
    "(local-variable-p 'V)",
    "(local-variable-if-set-p 'V)",
    "(variable-binding-locus 'V)",
    "(save-current-buffer (set-buffer O) (list V (local-variable-p 'V)))",
    "(let ((V 20)) (list V (symbol-value 'V) (default-value 'V)))",
    "(list V (default-value 'V))",
    "(let* ((V 21) (x V)) (list V x))",
    "(let ((V 30)) (setq V 31) (list V (let ((V 32)) V) V))",
    "(list V (default-value 'V))",
    "(condition-case nil (let ((V 40)) (signal 'error nil)) (error V))",
    "(catch 'tpx (let ((V 41)) (throw 'tpx V)))",
    "V",
    "(setq V 11)",
    "(list V (default-value 'V))",
    "(set 'V 12)",
    "(save-current-buffer (set-buffer O) (setq V 13) (list V (default-value 'V)))",
    "(list V (default-value 'V))",
    "(set-default 'V 14)",
    "(list V (default-value 'V))",
    "(save-current-buffer (set-buffer O) (list V (local-variable-p 'V)))",
    "(save-current-buffer (set-buffer O) (let ((V 15)) (list V (default-value 'V))))",
    "(list V (default-value 'V))",
    "(save-current-buffer (set-buffer O) (set (make-local-variable 'V) 16) (list V (local-variable-p 'V) (buffer-local-value 'V (current-buffer))))",
    "(list V (default-value 'V) (local-variable-p 'V))",
    "(save-current-buffer (set-buffer O) (let ((V 17)) V))",
    "(save-current-buffer (set-buffer O) (list V (default-value 'V)))",
    "(save-current-buffer (set-buffer O) (kill-local-variable 'V) (list V (local-variable-p 'V)))",
    "(list V (default-value 'V))",
    "(progn (add-variable-watcher 'V 'tpx-watch) (setq V 18) (let ((V 19)) V) (set-default 'V 22) (list V tpx-log))",
    "(progn (remove-variable-watcher 'V 'tpx-watch) (setq tpx-log nil) V)",
    "(progn (defvaralias 'tpx-alias-V 'V) (list tpx-alias-V (indirect-variable 'tpx-alias-V)))",
    "(progn (setq tpx-alias-V 23) (list V tpx-alias-V))",
    "(let ((tpx-alias-V 24)) (list V tpx-alias-V))",
    "(list V tpx-alias-V)",
    "(progn (add-variable-watcher 'V 'tpx-watch) (setq tpx-log nil) (set-default 'tpx-alias-V 28) (prog1 (list V tpx-log) (remove-variable-watcher 'V 'tpx-watch) (setq tpx-log nil)))",
    "(condition-case e (makunbound 'V) (error (list 'err (car e))))",
    "(list (boundp 'V) (condition-case nil V (void-variable 'void)))",
    "(let ((V 25)) (list V (boundp 'V)))",
    "(list (boundp 'V) (condition-case nil V (void-variable 'void)))",
    "(setq V 26)",
    "(list V (default-value 'V))",
    "(condition-case e (setq V \"str\") (error (car e)))",
    "(condition-case e (let ((V \"str\")) V) (error (car e)))",
    "(condition-case e (set-default 'V \"str\") (error (car e)))",
    "(list V (default-value 'V))",
    "(buffer-local-value 'V O)",
    "(let ((V 70)) (funcall (lambda () (let ((V 71)) (list (condition-case nil (backtrace-eval 'V 0) (error 'err)) (condition-case nil (backtrace-eval 'V 1) (error 'err)) (condition-case nil (backtrace-eval 'V 2) (error 'err)) V)))))",
    "(list V (default-value 'V))",
    "(progn (add-variable-watcher 'V 'tpx-throw) (prog1 (catch 'tpx-w (let ((V 90)) V)) (remove-variable-watcher 'V 'tpx-throw)))",
    "(list V (default-value 'V))",
    "(condition-case e (let ((V 80)) (makunbound 'V) (boundp 'V)) (error (list 'err (car e))))",
    "(list V (default-value 'V))",
    "(progn (make-variable-buffer-local 'V) (list (local-variable-if-set-p 'V) V (variable-binding-locus 'V)))",
    "(progn (setq V 27) (list V (default-value 'V) (local-variable-p 'V)))",
    "(save-current-buffer (set-buffer O) (list V (default-value 'V) (local-variable-p 'V)))",
];

fn script_forms(name: &str) -> Vec<String> {
    SCRIPT
        .iter()
        .map(|form| {
            form.replace("tpx-alias-V", &format!("tpx-alias-{name}"))
                .replace('V', name)
                .replace(" O)", " (get-buffer \" tpx-other\"))")
        })
        .collect()
}

/// A bytecode function over NAME: `(lambda () (list (let ((V 50)) (setq V
/// 51) V) V))` through `varbind`, `varset`, `varref` and `unbind`.
fn bytecode_script(name: &str) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Constant(2),
        Op::VarSet(0),
        Op::VarRef(0),
        Op::Unbind(1),
        Op::VarRef(0),
        Op::List(2),
        Op::Return,
    ];
    f.constants = vec![Value::symbol(name), Value::fixnum(50), Value::fixnum(51)].into();
    f.max_stack = StackDepth::for_test(4);
    f
}

fn vm_answer(ev: &mut Context, f: &ByteCodeFunction) -> String {
    let mut vm = Vm::from_context(ev);
    match vm.execute(f, vec![]) {
        Ok(value) => print_value(&value),
        Err(flow) => format!("ERR {flow:?}"),
    }
}

/// The steps of one shape's script: each source form, then the bytecode.
#[derive(Clone)]
enum Step {
    /// A SCRIPT entry, as run.
    Form(String),
    Bytecode,
}

impl Step {
    #[doc(hidden)]
    pub fn run(&self, ev: &mut Context, name: &str) -> String {
        match self {
            Step::Form(form) => answer(ev, form),
            Step::Bytecode => vm_answer(ev, &bytecode_script(name)),
        }
    }

    #[doc(hidden)]
    pub fn label(&self) -> &str {
        match self {
            Step::Form(form) => form,
            Step::Bytecode => "<bytecode>",
        }
    }
}

/// Run every step for every shape, unprojected, projected, and projected
/// with a collection first, returning the divergences.
fn divergences() -> Vec<String> {
    let mut found = Vec::new();
    for &(shape, name) in SHAPES {
        let steps: Vec<Step> = script_forms(name)
            .into_iter()
            .map(Step::Form)
            .chain([Step::Bytecode])
            .collect();
        let mut plain = context(shape, name, false);
        let mut projected = context(shape, name, true);
        let mut collected = context(shape, name, true);
        for step in &steps {
            let want = step.run(&mut plain, name);
            let got = step.run(&mut projected, name);
            collected.gc_collect_exact();
            let got_collected = step.run(&mut collected, name);
            for (arm, got) in [("projected", got), ("collected", got_collected)] {
                if want != got {
                    found.push(format!(
                        "{shape:?} {} [{arm}]\n    unprojected: {want}\n    {arm}:   {got}",
                        step.label()
                    ));
                }
            }
        }
    }
    found
}

#[test]
fn projected_variables_behave_as_unprojected() {
    let found = divergences();
    assert!(
        found.is_empty(),
        "{} divergences:\n{}",
        found.len(),
        found.join("\n")
    );
}

/// GNU notifies a watched plain variable twice when `set-default` reaches
/// it through an alias (`set_default_internal`'s trapped-write phase, then
/// `set_internal`'s) and once by its own name. A projected cell meets its
/// projection only after that phase, so the alias case must reveal first.
#[test]
fn projected_set_default_through_alias_notifies_as_unprojected() {
    let run = |projected: bool| {
        let mut ev = Context::new();
        eval(
            &mut ev,
            "(progn (defvar tpx-special 5) (defvar tpx-log nil)
                    (fset 'tpx-w (lambda (s n o wh) (setq tpx-log (cons (list s n o wh) tpx-log))))
                    (add-variable-watcher 'tpx-special 'tpx-w)
                    (defvaralias 'tpx-alias 'tpx-special))",
        );
        if projected {
            ev.project_thread_variable_for_test(intern("tpx-special"))
                .expect("tpx-special projects");
        }
        [
            "(progn (set-default 'tpx-special 22) (prog1 (reverse tpx-log) (setq tpx-log nil)))",
            "(progn (set-default 'tpx-alias 28) (prog1 (reverse tpx-log) (setq tpx-log nil)))",
            "(list tpx-special tpx-alias (default-value 'tpx-special))",
        ]
        .map(|form| answer(&mut ev, form))
    };
    let unprojected = run(false);
    // The oracle pins GNU's answer
    // (`oracle_variable_watchers_set_default_reports_set_operation`);
    // this only checks that the case exercises both notifications.
    assert_eq!(
        unprojected[1].matches("(tpx-special 28 set nil)").count(),
        2,
        "the alias case notifies twice: {}",
        unprojected[1]
    );
    assert_eq!(run(true), unprojected);
}

//! GNU semantics of a `mapc`/`mapcar` callback loop inlined by the default
//! profile's HOF-only front, observed after an ordinary heat tier-up: a
//! mid-callback error caught two frames out, a throw from `debugger` under
//! `debug-on-error`, `debug-on-next-call` at the mapping and its callback,
//! and a callback that lengthens the list. Each SETUP warms its caller past the
//! heat threshold; the test proves the caller's leaf inlined the mapping
//! before it evaluates OBSERVE. Fixtures come from GNU only with
//! UPDATE_EXPECT=1.

use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::bg::{BgMode, TestModeScope};
use crate::emacs_core::jit::compile::{self, Inline2Mode, OptMode};
use crate::emacs_core::print::print_value;
use std::path::PathBuf;

/// The production front with in-line tier-ups: Opt backend, INLINE2 off,
/// profit gate on, HOF inlining on, sync compiles. Threading: scalar
/// overrides on this test thread.
struct Production(Box<dyn std::any::Any>, TestModeScope);

impl Production {
    fn enter() -> Self {
        compile::force_inline2_for_test(Some(Inline2Mode::Off));
        compile::force_profit_gate_for_test(true);
        compile::force_hof_inline_for_test(Some(true));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
        Self(
            Box::new(compile::opt_mode_scope_for_test(OptMode::Opt)),
            TestModeScope::enter(BgMode::Sync),
        )
    }
}

impl Drop for Production {
    fn drop(&mut self) {
        compile::force_inline2_for_test(None);
        compile::force_hof_inline_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
    }
}

/// Shared by every SETUP: `neovm--hs-frames` lists the backtrace from the
/// innermost `neovm--hs-debugger` frame out to `neovm--hs-entry`, with
/// compiled functions printed as `compiled`.
const PRELUDE: &str = r#"(progn
  (defvar neovm--hs-log nil)
  (defun neovm--hs-norm (v)
    (cond ((byte-code-function-p v) 'compiled)
          ((consp v) (cons (neovm--hs-norm (car v)) (neovm--hs-norm (cdr v))))
          (t v)))
  (defun neovm--hs-frames ()
    (let (frames)
      (mapbacktrace
       (lambda (evald f args flags)
         (push (list evald (neovm--hs-norm f) (neovm--hs-norm args) flags) frames)))
      (setq frames (nreverse frames))
      (while (and frames (not (eq (nth 1 (car frames)) 'neovm--hs-debugger)))
        (setq frames (cdr frames)))
      (let (kept)
        (while frames
          (push (car frames) kept)
          (setq frames (if (eq (nth 1 (car frames)) 'neovm--hs-entry) nil (cdr frames))))
        (nreverse kept)))))"#;

fn gnu_expect(name: &str, setup: &str, observe: &str, fixture: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return fixture.trim_end().to_owned();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tmp = root.join("../../tmp");
    std::fs::create_dir_all(&tmp).expect("oracle temporary directory");
    let script = tmp.join(format!("hof-semantics-{name}-{}.el", std::process::id()));
    std::fs::write(
        &script,
        format!(
            ";;; -*- lexical-binding: t; -*-\n(setq backtrace-on-error-noninteractive nil)\n(eval '{PRELUDE} t)\n(eval '{setup} t)\n\
             (let ((print-length nil) (print-level nil)) (prin1 (eval '{observe} t)))\n"
        ),
    )
    .expect("write GNU oracle input");
    let emacs = crate::test_utils::gnu_oracle().executable_path();
    let result = std::process::Command::new(emacs)
        .args(["--batch", "-Q", "-l"])
        .arg(&script)
        .output()
        .expect("run GNU oracle");
    let _ = std::fs::remove_file(&script);
    assert!(
        result.status.success(),
        "GNU {name}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).expect("UTF-8 GNU output");
    assert!(!output.is_empty(), "GNU produced an expectation");
    std::fs::write(
        root.join("src/emacs_core/runtime/jit/tests/hof_default_semantics")
            .join(format!("{name}.expect")),
        format!("{}\n", output.trim_end()),
    )
    .expect("write GNU fixture");
    output.trim_end().to_owned()
}

/// The caller's installed leaf inlined HOF's callback loop.
fn assert_inlined(ctx: &mut Context, caller: &str, hof: &str) {
    let function = ctx
        .eval_str(&format!("(symbol-function '{caller})"))
        .expect("caller");
    let data = function.get_bytecode_data().expect("byte-compiled caller");
    let id = data
        .jit_runtime()
        .compiled_id()
        .unwrap_or_else(|| panic!("{caller} did not tier up during SETUP"));
    let leaf = crate::emacs_core::jit::cache::compiled_leaf_ptr_for_test(id)
        .unwrap_or_else(|| panic!("{caller} has no installed leaf"));
    // SAFETY: the cache owns the leaf until eviction; nothing evicts while
    // this test thread only reads it.
    let deps = unsafe { &*leaf }.inline_deps();
    assert!(
        deps.contains(&intern(hof)),
        "{caller} inlined its {hof} callback loop: {deps:?}"
    );
}

/// Run SETUP, check CALLER inlined HOF, then OBSERVE; compare with GNU and
/// check the specpdl and depth are balanced around OBSERVE.
fn assert_gnu(name: &str, setup: &str, caller: &str, hof: &str, observe: &str, fixture: &str) {
    let expected = gnu_expect(name, setup, observe, fixture);
    let _production = Production::enter();
    let mut ctx = crate::test_utils::runtime_startup_context();
    ctx.eval_str(&format!("(eval '{PRELUDE} t)"))
        .expect("prelude");
    ctx.eval_str(&format!("(eval '{setup} t)")).expect("setup");
    assert_inlined(&mut ctx, caller, hof);
    let specpdl = ctx.specpdl.len();
    let depth = ctx.depth;
    let value = ctx
        .eval_str(&format!("(eval '{observe} t)"))
        .expect("observe");
    assert_eq!(ctx.specpdl.len(), specpdl, "{name}: specpdl balanced");
    assert_eq!(ctx.depth, depth, "{name}: depth balanced");
    assert_eq!(print_value(&value), expected, "GNU {name}");
}

#[test]
fn mid_callback_error_unwinds_to_a_condition_case_two_frames_out() {
    assert_gnu(
        "condition-case",
        r#"(progn
  (defvar neovm--hs-special 'outer)
  (defun neovm--hs-cars (l) (mapcar (lambda (x) (car x)) l))
  (defun neovm--hs-outer (l) (let ((neovm--hs-special 'inner)) (neovm--hs-cars l)))
  (byte-compile 'neovm--hs-cars)
  (byte-compile 'neovm--hs-outer)
  (dotimes (_ 1100) (neovm--hs-cars '((1) (2))))
  nil)"#,
        "neovm--hs-cars",
        "mapcar",
        r#"(list (condition-case err (neovm--hs-outer '((1) (2) 3 (4)))
         (wrong-type-argument (list 'caught err neovm--hs-special)))
       neovm--hs-special
       (neovm--hs-cars '((5) (6))))"#,
        include_str!("hof_default_semantics/condition-case.expect"),
    );
}

#[test]
fn a_throw_from_the_debugger_crosses_the_inlined_mapping() {
    // `1+` of a symbol signals from `Fadd1` with no frame of its own in GNU
    // (unlike `car`, whose bytecode records one).
    assert_gnu(
        "debug-on-error-throw",
        r#"(progn
  (defun neovm--hs-incs (l) (mapc (lambda (x) (1+ x)) l))
  (defun neovm--hs-debugger (&rest args)
    (throw 'neovm--hs-tag (list 'debugger (neovm--hs-norm args) (neovm--hs-frames))))
  (defun neovm--hs-entry (l) (catch 'neovm--hs-tag (neovm--hs-incs l)))
  (byte-compile 'neovm--hs-incs)
  (byte-compile 'neovm--hs-entry)
  (dotimes (_ 1100) (neovm--hs-incs '(1 2)))
  nil)"#,
        "neovm--hs-incs",
        "mapc",
        r#"(let ((debug-on-error t) (debugger #'neovm--hs-debugger))
  (list (neovm--hs-entry '(1 2 a 4))
        (neovm--hs-incs '(7))))"#,
        include_str!("hof_default_semantics/debug-on-error-throw.expect"),
    );
}

/// An admitted callback has no call and no variable write, so nothing in
/// the native loop can arm `debug-on-next-call`; the caller arms it just
/// before the mapping, and the debugger re-arms it from the mapping's entry
/// so that the first callback enters the debugger as well. Both frames then
/// report `debug-on-exit`, whose debugger value replaces the return value.
#[test]
fn debug_on_next_call_enters_the_debugger_at_the_mapping_and_its_callback() {
    assert_gnu(
        "debug-on-next-call",
        r#"(progn
  (defvar neovm--hs-rearm nil)
  (defun neovm--hs-arm (l arm)
    (when arm (setq debug-on-next-call t))
    (mapcar (lambda (x) (1+ x)) l))
  (defun neovm--hs-debugger (&rest args)
    (push (list (neovm--hs-norm args) (neovm--hs-frames)) neovm--hs-log)
    (cond ((eq (car args) 'exit) (list 'replaced (cadr args)))
          (neovm--hs-rearm (setq neovm--hs-rearm nil) (setq debug-on-next-call t))))
  (defun neovm--hs-entry (l) (neovm--hs-arm l t))
  (byte-compile 'neovm--hs-arm)
  (byte-compile 'neovm--hs-debugger)
  (byte-compile 'neovm--hs-entry)
  (dotimes (_ 1100) (neovm--hs-arm '(1 2) nil))
  nil)"#,
        "neovm--hs-arm",
        "mapcar",
        r#"(let ((debugger #'neovm--hs-debugger))
  (setq neovm--hs-log nil neovm--hs-rearm t)
  (let ((result (neovm--hs-entry '(1 2 3))))
    (list result debug-on-next-call (nreverse neovm--hs-log))))"#,
        include_str!("hof_default_semantics/debug-on-next-call.expect"),
    );
}

#[test]
fn a_callback_that_lengthens_the_list_stops_at_the_original_length() {
    assert_gnu(
        "lengthen",
        r#"(progn
  (defun neovm--hs-grow (l tail)
    (mapcar (lambda (x) (setcdr tail '(9)) (1+ x)) l))
  (byte-compile 'neovm--hs-grow)
  (dotimes (_ 1100) (let ((l (list 1 2))) (neovm--hs-grow l (last l))))
  nil)"#,
        "neovm--hs-grow",
        "mapcar",
        r#"(let* ((l (list 1 2 3)) (r (neovm--hs-grow l (last l))))
  (list r l))"#,
        include_str!("hof_default_semantics/lengthen.expect"),
    );
}

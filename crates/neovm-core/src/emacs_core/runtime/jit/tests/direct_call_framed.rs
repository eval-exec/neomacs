//! Stage 2c: GNU 31.1 observations for actual `EntryShape::Framed` calls.
//! GNU bytecode.c records the original call before setup_frame, Bvarbind can
//! collect, and eval.c:unwind_to_catch unbinds before restoring handlers/depth.
//! Fixtures are produced by GNU only with UPDATE_EXPECT=1. Each observation
//! includes an eligible call and proves it entered the direct framed helper.

use super::*;
use std::path::PathBuf;

fn gnu_expect(name: &str, program: &str, observe: &str, fixture: &str) -> String {
    if std::env::var("UPDATE_EXPECT").as_deref() != Ok("1") {
        return fixture.trim_end().to_owned();
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tmp = root.join("../../tmp");
    std::fs::create_dir_all(&tmp).expect("oracle temporary directory");
    let script = tmp.join(format!("direct-framed-{name}-{}.el", std::process::id()));
    std::fs::write(&script, format!(
        ";;; -*- lexical-binding: t; -*-\n(require 'bytecomp)\n{WARM}\n{program}\n(let ((print-length nil) (print-level nil)) (prin1 {observe}))\n"
    )).expect("write GNU oracle input");
    let emacs = std::env::var_os("EMACS").unwrap_or_else(|| {
        PathBuf::from(std::env::var_os("HOME").expect("HOME"))
            .join(".local/bin/emacs")
            .into_os_string()
    });
    let result = std::process::Command::new(emacs)
        .args(["--batch", "-Q", "-l"])
        .arg(&script)
        .output()
        .expect("run GNU oracle");
    assert!(
        result.status.success(),
        "GNU {name}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = String::from_utf8(result.stdout).expect("UTF-8 GNU output");
    assert!(!output.is_empty(), "GNU produced an expectation");
    std::fs::write(
        root.join("src/emacs_core/runtime/jit/tests/direct_call_framed")
            .join(format!("{name}.expect")),
        format!("{}\n", output.trim_end()),
    )
    .expect("write GNU fixture");
    output.trim_end().to_owned()
}

/// Remove only `framed` from the comparison's shape set. In particular the
/// constant-object comparison still enables `constant` in its control run.
fn assert_gnu_framed(
    name: &str,
    shapes: &str,
    program: &'static str,
    observe: &'static str,
    fixture: &str,
    named: bool,
) -> [Run; 4] {
    let without = shapes
        .split(',')
        .filter(|part| *part != "framed")
        .collect::<Vec<_>>()
        .join(",");
    let expected = gnu_expect(name, program, observe, fixture);
    let modes = [
        (Mode::Shim, DirectShapesKnob::OFF),
        (Mode::Direct, DirectShapesKnob::parse(Some(&without))),
        (Mode::Direct, DirectShapesKnob::parse(Some(shapes))),
        (
            Mode::DirectForcedSlow,
            DirectShapesKnob::parse(Some(shapes)),
        ),
    ];
    let runs = modes.map(|(mode, knob)| run_in_with(mode, knob, program, observe));
    for (run, (mode, knob)) in runs.iter().zip(modes) {
        assert_eq!(run.out, expected, "GNU {name}: {mode:?}, {knob:?}");
    }
    assert_eq!(runs[0].framed_calls, 0, "shim mode");
    assert_eq!(runs[1].framed_calls, 0, "framed bit off");
    assert_eq!(runs[3].framed_calls, 0, "forced shim mode");
    assert!(
        runs[2].framed_calls > 0,
        "the OBSERVATION enters a direct framed callee: {runs:?}"
    );
    assert!(runs[2].direct_sites > 0, "direct sites emitted");
    if named {
        assert!(
            runs[2].shim_calls < runs[1].shim_calls,
            "named framed calls bypass the spec shim: {runs:?}"
        );
    }
    runs
}

const FRAME_PROGRAM: &str = r#"(progn
  (require 'cl-lib)
  (defvar neovm--ff-special 'outside)
  (defvar neovm--ff-target nil)
  (defun neovm--ff-normalize (f)
    (if (byte-code-function-p f)
        (list 'byte-code-function (func-arity f) (eq f neovm--ff-target)) f))
  (defun neovm--ff-show ()
    (let (mapped frames)
      (mapbacktrace
       (lambda (evald f args flags)
         (when (eq f neovm--ff-target)
           (push (list evald (neovm--ff-normalize f) args flags) mapped)))
       'neovm--ff-show)
      (dolist (frame (backtrace-frames 'neovm--ff-show))
        (when (eq (cadr frame) neovm--ff-target)
          (push (cons (car frame) (cons (neovm--ff-normalize (cadr frame)) (cddr frame))) frames)))
      (let ((one (backtrace-frame 1 'neovm--ff-show)))
        (list (nreverse mapped)
              (cons (car one) (cons (neovm--ff-normalize (cadr one)) (cddr one)))
              (nreverse frames) neovm--ff-special))))
  (defun neovm--ff-named (a b c)
    (let ((neovm--ff-special b))
      (if (eq a 'show) (neovm--ff-show) (list a neovm--ff-special c))))
  (defun neovm--ff-caller (a b c) (neovm--ff-named a b c))
  (defun neovm--ff-object (a b c)
    (cl-flet ((local (x y z)
               (let ((neovm--ff-special y))
                 (if (eq x 'show) (neovm--ff-show) (list x neovm--ff-special z)))))
      (list (local a b c) (local a b c))))
  (dolist (f '(neovm--ff-normalize neovm--ff-show neovm--ff-named neovm--ff-caller
               neovm--ff-object)) (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--ff-caller i '(bound) "third")
    (neovm--ff-object i '(bound) "third")))"#;
const NAMED_FRAMES: &str = r#"(progn (setq neovm--ff-target 'neovm--ff-named)
  (list (neovm--ff-caller 7 '(bound) "third")
        (neovm--ff-caller 'show '(bound) "third") neovm--ff-special))"#;
const OBJECT_FRAMES: &str = r#"(progn
  (dolist (c (append (aref (symbol-function 'neovm--ff-object) 2) nil))
    (when (byte-code-function-p c) (setq neovm--ff-target c)))
  (list (byte-code-function-p neovm--ff-target)
        (neovm--ff-object 7 '(bound) "third")
        (neovm--ff-object 'show '(bound) "third") neovm--ff-special))"#;

#[test]
fn gnu_framed_symbol_backtraces_keep_original_three_args() {
    assert_gnu_framed(
        "named-frames",
        "framed",
        FRAME_PROGRAM,
        NAMED_FRAMES,
        include_str!("direct_call_framed/named-frames.expect"),
        true,
    );
}
#[test]
fn gnu_framed_constant_backtraces_keep_original_object_and_args() {
    assert_gnu_framed(
        "object-frames",
        "constant,framed",
        FRAME_PROGRAM,
        OBJECT_FRAMES,
        include_str!("direct_call_framed/object-frames.expect"),
        false,
    );
}

const ARITY_PROGRAM: &str = r#"(progn
  (require 'cl-lib)
  (defvar neovm--fa-special 'outside)
  (defun neovm--fa-read () neovm--fa-special)
  (defun neovm--fa-named (a b c)
    (let ((neovm--fa-special a)) (list (neovm--fa-read) b c)))
  (defun neovm--fa-caller (a b c) (neovm--fa-named a b c))
  (defun neovm--fa-object (n)
    (cl-flet ((local (a b c)
               (let ((neovm--fa-special a)) (list (neovm--fa-read) b c))))
      (cond ((> n 0) (local n 1 2)) ((= n 0) (local)) (t (local n 1 2 3)))))
  (dolist (f '(neovm--fa-read neovm--fa-named neovm--fa-caller neovm--fa-object)) (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--fa-caller i 1 2) (neovm--fa-object (1+ i))))"#;
const NAMED_ARITY: &str = r#"(let ((old (symbol-function 'neovm--fa-named)))
  (list (neovm--fa-caller 4 1 2)
    (progn (fset 'neovm--fa-named
                 (byte-compile (lambda (a b c d)
                   (let ((neovm--fa-special a)) (list a b c d)))))
           (condition-case err (neovm--fa-caller 5 1 2) (error err)))
    (progn (fset 'neovm--fa-named old) (neovm--fa-caller 6 1 2)) neovm--fa-special))"#;
const OBJECT_ARITY: &str = r#"(list (neovm--fa-object 4)
  (condition-case err (neovm--fa-object 0) (error err))
  (condition-case err (neovm--fa-object -1) (error err))
  (neovm--fa-object 6) neovm--fa-special)"#;

#[test]
fn gnu_framed_wrong_arity_errors_keep_complete_data_and_recover() {
    assert_gnu_framed(
        "named-arity",
        "framed",
        ARITY_PROGRAM,
        NAMED_ARITY,
        include_str!("direct_call_framed/named-arity.expect"),
        true,
    );
    assert_gnu_framed(
        "object-arity",
        "constant,framed",
        ARITY_PROGRAM,
        OBJECT_ARITY,
        include_str!("direct_call_framed/object-arity.expect"),
        false,
    );
}

const DEBUG_PROGRAM: &str = r#"(progn
  (require 'cl-lib)
  (defvar neovm--fd-special 'outside)
  (defvar neovm--fd-arm nil)
  (defvar neovm--fd-log nil)
  (defun neovm--fd-named (a b c)
    (let ((neovm--fd-special a)) (+ neovm--fd-special b c)))
  (defun neovm--fd-caller (a b c)
    (when neovm--fd-arm (setq debug-on-next-call t)) (neovm--fd-named a b c))
  (defun neovm--fd-object (a b c)
    (cl-flet ((local (x y z)
               (let ((neovm--fd-special x)) (+ neovm--fd-special y z))))
      (when neovm--fd-arm (setq debug-on-next-call t))
      (list (local a b c) (local a b c))))
  (dolist (f '(neovm--fd-named neovm--fd-caller neovm--fd-object)) (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--fd-caller i 1 2) (neovm--fd-object i 1 2)))"#;
const NAMED_DEBUG: &str = r#"(list (neovm--fd-caller 3 1 2)
  (let ((debugger (lambda (&rest args)
                   (push (car args) neovm--fd-log)
                   (if (eq (car args) 'exit) (cadr args) nil)))
        (neovm--fd-arm t))
    (list (neovm--fd-caller 5 1 2) (reverse neovm--fd-log) debug-on-next-call))
  neovm--fd-special (neovm--fd-caller 4 1 2))"#;
const OBJECT_DEBUG: &str = r#"(list (neovm--fd-object 3 1 2)
  (let ((debugger (lambda (&rest args)
                   (push (car args) neovm--fd-log)
                   (if (eq (car args) 'exit) (cadr args) nil)))
        (neovm--fd-arm t))
    (list (neovm--fd-object 5 1 2) (reverse neovm--fd-log) debug-on-next-call))
  neovm--fd-special (neovm--fd-object 4 1 2))"#;
#[test]
fn gnu_framed_debug_on_next_call_and_exit_restore_bindings() {
    assert_gnu_framed(
        "named-debug",
        "framed",
        DEBUG_PROGRAM,
        NAMED_DEBUG,
        include_str!("direct_call_framed/named-debug.expect"),
        true,
    );
    assert_gnu_framed(
        "object-debug",
        "constant,framed",
        DEBUG_PROGRAM,
        OBJECT_DEBUG,
        include_str!("direct_call_framed/object-debug.expect"),
        false,
    );
}

const DEPTH_PROGRAM: &str = r#"(progn
  (defvar neovm--fl-special 'outside)
  (defvar neovm--fl-count 0)
  (defun neovm--fl-rec (n)
    (let ((neovm--fl-special n))
      (setq neovm--fl-count (1+ neovm--fl-count))
      (if (= n 0) 0 (1+ (neovm--fl-rec (1- n))))))
  (byte-compile 'neovm--fl-rec)
  (dotimes (_ 60) (neovm--fl-rec 60)))"#;
const DEPTH_OBSERVE: &str = r#"(list (neovm--fl-rec 20)
  (let ((max-lisp-eval-depth 300))
    (setq neovm--fl-count 0)
    (list (condition-case err (neovm--fl-rec 1000) (error err))
          (> neovm--fl-count 100) (< neovm--fl-count 300)))
  neovm--fl-special (neovm--fl-rec 20))"#;
#[test]
fn gnu_framed_recursion_obeys_depth_and_recovers() {
    assert_gnu_framed(
        "depth",
        "framed",
        DEPTH_PROGRAM,
        DEPTH_OBSERVE,
        include_str!("direct_call_framed/depth.expect"),
        true,
    );
}

const REDEFINE_PROGRAM: &str = r#"(progn
  (require 'cl-lib)
  (defvar neovm--fx-special 'outside)
  (defvar neovm--fx-arm nil)
  (defvar neovm--fx-live nil)
  (defvar neovm--fx-keys (make-hash-table :test 'eq :weakness 'key))
  (defun neovm--fx-named (n x y)
    (let ((neovm--fx-special (car x)))
      (if (= n 0)
          (if neovm--fx-arm
              (progn (fset 'neovm--fx-named (lambda (_n _x _y) 'new))
                     (garbage-collect)
                     (setq neovm--fx-live (hash-table-count neovm--fx-keys))
                     (+ neovm--fx-special (car y))) 0)
        (1+ (neovm--fx-named (1- n) x y)))))
  (defun neovm--fx-caller (n x y) (neovm--fx-named n x y))
  (defun neovm--fx-object (x y z)
    (cl-flet ((local (a b c)
               (let ((neovm--fx-special (car a)))
                 (if neovm--fx-arm
                     (progn (fset 'neovm--fx-object (lambda (_x _y _z) 'new))
                            (garbage-collect)
                            (setq neovm--fx-live (hash-table-count neovm--fx-keys))
                            (+ neovm--fx-special (car b) c)) 0))))
      (list (1+ (local x y z)) (1+ (local x y z)))))
  (dolist (f '(neovm--fx-named neovm--fx-caller neovm--fx-object)) (byte-compile f))
  (dotimes (_ neovm--dc-warm)
    (neovm--fx-caller 3 '(1) '(2)) (neovm--fx-object '(1) '(2) 3)))"#;
const NAMED_REDEFINE: &str = r#"(progn
  (puthash (symbol-function 'neovm--fx-named) t neovm--fx-keys)
  (list (neovm--fx-caller 3 '(1) '(2))
        (let ((neovm--fx-arm t)) (neovm--fx-caller 3 '(40) '(0.5)))
        neovm--fx-live neovm--fx-special (neovm--fx-caller 3 '(1) '(2))))"#;
const OBJECT_REDEFINE: &str = r#"(progn
  (dolist (c (append (aref (symbol-function 'neovm--fx-object) 2) nil))
    (when (byte-code-function-p c) (puthash c t neovm--fx-keys)))
  (list (neovm--fx-object '(1) '(2) 3)
        (let ((neovm--fx-arm t)) (neovm--fx-object '(40) '(0.5) 3))
        neovm--fx-live neovm--fx-special (neovm--fx-object '(1) '(2) 3)))"#;
#[test]
fn gnu_framed_redefinition_gc_and_deopt_resume_the_entered_old_object() {
    assert_gnu_framed(
        "named-redefine",
        "framed",
        REDEFINE_PROGRAM,
        NAMED_REDEFINE,
        include_str!("direct_call_framed/named-redefine.expect"),
        true,
    );
    assert_gnu_framed(
        "object-redefine",
        "constant,framed",
        REDEFINE_PROGRAM,
        OBJECT_REDEFINE,
        include_str!("direct_call_framed/object-redefine.expect"),
        false,
    );
}

const DEOPT_PROGRAM: &str = r#"(progn
  (defvar neovm--fo-special 'outside)
  (defvar neovm--fo-visits 0)
  (defvar neovm--fo-clean nil)
  (defun neovm--fo-live (x y z)
    (let ((neovm--fo-special x))
      (setq neovm--fo-visits (1+ neovm--fo-visits))
      (unwind-protect (list (+ y z) neovm--fo-special)
        (push neovm--fo-special neovm--fo-clean))))
  (defun neovm--fo-caller (x y z) (neovm--fo-live x y z))
  (dolist (f '(neovm--fo-live neovm--fo-caller)) (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--fo-caller '(bound) i 2)))"#;
const DEOPT_OBSERVE: &str = r#"(progn (setq neovm--fo-visits 0 neovm--fo-clean nil)
  (list (neovm--fo-caller '(normal) 1 2)
        (neovm--fo-caller '(float) 0.5 2)
        (condition-case err (neovm--fo-caller '(signal) 'bad 2) (error err))
        (neovm--fo-caller '(recovered) 3 4)
        neovm--fo-visits (reverse neovm--fo-clean) neovm--fo-special))"#;
#[test]
fn gnu_framed_precise_deopt_keeps_live_bindings_and_does_not_restart() {
    assert_gnu_framed(
        "live-deopt",
        "framed",
        DEOPT_PROGRAM,
        DEOPT_OBSERVE,
        include_str!("direct_call_framed/live-deopt.expect"),
        true,
    );
}

const HANDLER_PROGRAM: &str = r#"(progn
  (define-error 'neovm--fh-error "framed test error")
  (defvar neovm--fh-special 'outside)
  (defvar neovm--fh-clean nil)
  (defun neovm--fh-target (mode x y)
    (let ((neovm--fh-special x))
      (unwind-protect
          (condition-case err
              (cond ((eq mode 'signal) (signal 'neovm--fh-error (list neovm--fh-special y)))
                    ((eq mode 'throw) (throw 'neovm--fh-tag (list neovm--fh-special y)))
                    (t (list mode neovm--fh-special y)))
            (neovm--fh-error (list 'caught err neovm--fh-special)))
        (push (list mode neovm--fh-special) neovm--fh-clean))))
  (defun neovm--fh-caller (mode x y)
    (catch 'neovm--fh-tag (neovm--fh-target mode x y)))
  (dolist (f '(neovm--fh-target neovm--fh-caller)) (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--fh-caller 'normal '(bound) i)))"#;
const HANDLER_OBSERVE: &str = r#"(progn (setq neovm--fh-clean nil)
  (list (neovm--fh-caller 'normal '(one) 1)
        (neovm--fh-caller 'signal '(two) 2)
        (neovm--fh-caller 'throw '(three) 3)
        (neovm--fh-caller 'normal '(four) 4)
        (reverse neovm--fh-clean) neovm--fh-special))"#;
#[test]
fn gnu_framed_handlers_signals_throws_and_unwind_cleanups_restore_state() {
    assert_gnu_framed(
        "handlers",
        "framed",
        HANDLER_PROGRAM,
        HANDLER_OBSERVE,
        include_str!("direct_call_framed/handlers.expect"),
        true,
    );
}

const BOUNDARY_PROGRAM: &str = r#"(progn
  (defvar neovm--fb-special 'outside)
  (defun neovm--fb-read () neovm--fb-special)
  (defun neovm--fb-show (target)
    (let (seen)
      (mapbacktrace (lambda (evald f args flags)
        (when (eq f target) (setq seen (list evald f args flags))))) seen))
  (defun neovm--fb-zero ()
    (let ((neovm--fb-special 'zero)) (list (neovm--fb-read))))
  (defun neovm--fb-zero-caller () (neovm--fb-zero))
  (defun neovm--fb-eight (a b c d e f g h)
    (let ((neovm--fb-special a))
      (if (eq a 'show) (neovm--fb-show 'neovm--fb-eight) (list a b c d e f g h))))
  (defun neovm--fb-eight-caller (x) (neovm--fb-eight x 2 3 4 5 6 7 8))
  (defun neovm--fb-nine (a b c d e f g h i)
    (let ((neovm--fb-special a)) (list (neovm--fb-read) b c d e f g h i)))
  (defun neovm--fb-nine-caller (x) (neovm--fb-nine x 2 3 4 5 6 7 8 9))
  (dolist (f '(neovm--fb-read neovm--fb-show neovm--fb-zero neovm--fb-zero-caller
               neovm--fb-eight neovm--fb-eight-caller neovm--fb-nine
               neovm--fb-nine-caller)) (byte-compile f))
  (dotimes (i neovm--dc-warm)
    (neovm--fb-zero-caller) (neovm--fb-eight-caller i) (neovm--fb-nine-caller i)))"#;
const ZERO_OBSERVE: &str = r#"(list (neovm--fb-zero-caller) neovm--fb-special)"#;
const EIGHT_OBSERVE: &str = r#"(list (neovm--fb-eight-caller 1)
  (neovm--fb-eight-caller 'show) neovm--fb-special)"#;
const NINE_OBSERVE: &str = r#"(list (neovm--fb-nine-caller 1) neovm--fb-special)"#;

#[test]
fn gnu_framed_zero_and_eight_argument_memory_calls_hit_but_nine_falls_back() {
    assert_gnu_framed(
        "zero",
        "framed",
        BOUNDARY_PROGRAM,
        ZERO_OBSERVE,
        include_str!("direct_call_framed/zero.expect"),
        true,
    );
    assert_gnu_framed(
        "eight",
        "framed",
        BOUNDARY_PROGRAM,
        EIGHT_OBSERVE,
        include_str!("direct_call_framed/eight.expect"),
        true,
    );
    let expected = gnu_expect(
        "nine",
        BOUNDARY_PROGRAM,
        NINE_OBSERVE,
        include_str!("direct_call_framed/nine.expect"),
    );
    let baseline = run_in_with(
        Mode::Direct,
        DirectShapesKnob::OFF,
        BOUNDARY_PROGRAM,
        NINE_OBSERVE,
    );
    let framed = run_in_with(
        Mode::Direct,
        DirectShapesKnob::parse(Some("framed")),
        BOUNDARY_PROGRAM,
        NINE_OBSERVE,
    );
    assert_eq!(baseline.out, expected);
    assert_eq!(framed.out, expected);
    assert_eq!(
        framed.framed_calls, 0,
        "nine arguments retain the shim fallback"
    );
    assert!(baseline.shim_calls > 0);
    assert_eq!(framed.shim_calls, baseline.shim_calls);
}

/// Framed entries are typed dispatch tags, never register-entry addresses.
#[test]
fn framed_slots_arm_a_memory_dispatch_tag_only_with_the_framed_bit() {
    for enabled in [false, true] {
        std::thread::spawn(move || {
            force_profit_gate_for_test(false);
            crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
            crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
            force_direct_call_for_test(Some(true));
            force_direct_sites_for_test(Some(DirectSitesMode::All));
            force_direct_shapes_for_test(Some(if enabled {
                DirectShapesKnob::parse(Some("framed"))
            } else {
                DirectShapesKnob::OFF
            }));
            let mut ev = crate::test_utils::runtime_startup_context();
            ev.eval_str(WARM).expect("warm count");
            ev.eval_str(FRAME_PROGRAM).expect("warm exact framed calls");
            let caller = cached_leaf(&ev, "neovm--ff-caller").expect("compiled caller");
            let callee = cached_leaf(&ev, "neovm--ff-named").expect("compiled callee");
            assert!(callee.has_binds);
            assert_eq!(callee.entry_shape, EntryShape::Framed);
            assert_eq!(callee.abi, LeafAbi::Memory);
            let slot = slot_calling(caller, callee).expect("armed named slot");
            if enabled {
                assert_eq!(
                    slot.direct_entry() as usize,
                    crate::emacs_core::jit::compile::spec_slot::DirectEntryTag::Framed as usize
                );
                assert_ne!(slot.direct_entry(), callee.entry);
            } else {
                assert!(slot.direct_entry().is_null());
            }
            force_direct_shapes_for_test(None);
            force_direct_call_for_test(None);
            force_direct_sites_for_test(None);
        })
        .join()
        .expect("arming assertions");
    }
}

/// Rust's private panic builtin has no GNU counterpart. This pins its
/// containment protocol, while the GNU fixtures above pin Lisp signals and
/// unwinds. Three fresh heap arguments exercise a borrowed native span.
/// Its storage dies before GC; no pending frame may still read that span.
#[test]
fn a_contained_framed_panic_detaches_three_args_before_gc_and_caller_healing() {
    std::thread::Builder::new().stack_size(128 * 1024 * 1024).spawn(|| {
        force_profit_gate_for_test(false);
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
        force_direct_call_for_test(Some(true));
        force_direct_sites_for_test(Some(DirectSitesMode::All));
        force_direct_shapes_for_test(Some(DirectShapesKnob::parse(Some("framed"))));
        let mut ev = crate::test_utils::runtime_startup_context();
        ev.eval_str(r#"(progn
          (defvar neovm--fp-arm nil)
          (defvar neovm--fp-special 'outside)
          (defun neovm--fp-target (a b c)
            (let ((neovm--fp-special a))
              (if neovm--fp-arm (neovm--internal-panic "framed-before-heal")
                (list neovm--fp-special b c))))
          (byte-compile 'neovm--fp-target)
          (dotimes (_ 1500) (neovm--fp-target 'warm 1 2))
          (setq neovm--fp-arm t))"#).expect("warm binding framed leaf");
        let callee = ev.obarray.symbol_function_id(intern("neovm--fp-target")).expect("function");
        let leaf = cached_leaf(&ev, "neovm--fp-target").expect("native leaf");
        assert_eq!(leaf.entry_shape, EntryShape::Framed);
        assert!(leaf.has_binds && !leaf.has_handlers, "a binding extent parks the marker across its cleanup");
        let constants = callee.get_bytecode_data().expect("bytecode").constants.as_ptr();
        let depth0 = ev.depth;
        let spec0 = ev.specpdl.len();
        let frames0 = ev.bc_frames.len();
        let buf0 = ev.bc_buf.len();
        let cond0 = ev.condition_stack.len();
        let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();
        let caller_boundary = ev.module_boundary_snapshot();
        let calls0 = super::super::direct_call::DIRECT_FRAMED_CALLS.load(Ordering::Relaxed);
        let expired_span;
        {
            let args = [Value::string("first fresh heap arg").bits() as i64,
                        Value::string("second fresh heap arg").bits() as i64,
                        Value::string("third fresh heap arg").bits() as i64];
            expired_span = args.as_ptr();
            // SAFETY: all three tagged words remain live through the helper.
            unsafe { ev.push_backtrace_frame_from_native_args(Value::symbol("neovm--fp-target"), args.as_ptr(), args.len()) };
            ev.depth += 1;
            let mut output = i64::MIN;
            let status = super::super::direct_call::neovm_jit_direct_framed(
                &mut ev as *mut Context as *mut u8, callee.bits() as i64,
                leaf as *const CompiledLeaf as usize as i64, constants as usize as i64,
                args.as_ptr(), args.len() as i64, spec0 as i64, &mut output);
            assert_eq!(status, STATUS_SIGNAL);
            assert_eq!(output, i64::MIN, "signal leaves the output slot unreadable");
        }
        assert!(shim_panic_pending(), "the caller's heal has not consumed the marker");
        assert_eq!(ev.depth, depth0 + 1, "pending panic leaves counted depth for caller healing");
        assert_eq!(super::super::direct_call::DIRECT_FRAMED_CALLS.load(Ordering::Relaxed), calls0 + 1);
        assert!(ev.specpdl.iter().all(|entry| !matches!(entry,
            crate::emacs_core::eval::SpecBinding::BacktraceNative { args_ptr, .. } if *args_ptr == expired_span)),
            "no frame reads the expired three-word argument span");
        assert!(ev.specpdl[spec0..].iter().any(|entry| matches!(entry,
            crate::emacs_core::eval::SpecBinding::Backtrace { function, .. }
                if *function == Value::symbol("neovm--fp-target"))),
            "the original called symbol remains on an owned detached frame before healing");
        let collected0 = ev.tagged_heap.gc_collections();
        ev.gc_collect_exact();
        assert!(ev.tagged_heap.gc_collections() > collected0, "collection occurs while the panic is pending");
        assert!(shim_panic_pending(), "GC did not materialize the pending panic");
        // The enclosing caller's depth-based unwind after that collection.
        ev.restore_jit_shim_boundary(&caller_boundary, cond0);
        ev.unbind_to(spec0);
        let flow = take_pending_flow().expect("panic materializes at the caller boundary");
        let crate::emacs_core::error::Flow::Signal(signal) = flow else { panic!("panic must become a Lisp signal") };
        assert_eq!(signal.symbol_name(), "error");
        assert!(signal.data[0].as_str_owned().expect("message").contains("framed-before-heal"));
        assert!(!shim_panic_pending());
        assert_eq!((ev.depth, ev.specpdl.len(), ev.bc_frames.len(), ev.bc_buf.len(), ev.condition_stack.len()),
            (depth0, spec0, frames0, buf0, cond0));
        assert_eq!(crate::emacs_core::eval::save_scratch_gc_roots(), roots0);
        ev.eval_str("(setq neovm--fp-arm nil)").expect("disarm");
        let value = ev.eval_str("(neovm--fp-target 'recovered 1 2)").expect("caller recovers");
        assert_eq!(crate::emacs_core::print::print_value(&value), "(recovered 1 2)");
        force_direct_shapes_for_test(None);
        force_direct_call_for_test(None);
        force_direct_sites_for_test(None);
    }).expect("spawn").join().expect("contained panic never crosses JIT/C frames");
}

#[test]
fn a_panic_from_a_generated_framed_site_is_caught_by_its_caller_and_recovers() {
    const PROGRAM: &str = r#"(progn
      (defvar neovm--fg-arm nil)
      (defvar neovm--fg-special 'outside)
      (defun neovm--fg-target (a b c)
        (let ((neovm--fg-special a))
          (if neovm--fg-arm (neovm--internal-panic "framed-generated")
            (list neovm--fg-special b c))))
      (defun neovm--fg-caller (a b c)
        (condition-case err (neovm--fg-target a b c) (error (car err))))
      (dolist (f '(neovm--fg-target neovm--fg-caller)) (byte-compile f))
      (dotimes (_ neovm--dc-warm) (neovm--fg-caller 'warm 1 2)))"#;
    const OBSERVE: &str = r#"(list (neovm--fg-caller 'normal "two" '(three))
      (let ((neovm--fg-arm t)) (neovm--fg-caller "fresh-one" '(fresh-two) "fresh-three"))
      (progn (garbage-collect) (neovm--fg-caller 'recovered "two" '(three)))
      neovm--fg-special)"#;
    let baseline = run_in_with(Mode::Direct, DirectShapesKnob::OFF, PROGRAM, OBSERVE);
    let direct = run_in_with(
        Mode::Direct,
        DirectShapesKnob::parse(Some("framed")),
        PROGRAM,
        OBSERVE,
    );
    let forced = run_in_with(
        Mode::DirectForcedSlow,
        DirectShapesKnob::parse(Some("framed")),
        PROGRAM,
        OBSERVE,
    );
    assert_eq!(direct.out, baseline.out);
    assert_eq!(forced.out, baseline.out);
    assert!(
        direct.out.contains(" error "),
        "the caller's handler catches the contained panic"
    );
    assert!(direct.out.contains("recovered"));
    assert!(
        direct.framed_calls >= 3,
        "normal, panic and recovery calls use generated framed sites"
    );
    assert_eq!(baseline.framed_calls, 0);
    assert_eq!(forced.framed_calls, 0);
}

//! Direct calls of the call shapes beyond a named exact-arity call (design
//! `p1-1-direct-native-calls` Stage 2, P1.0 S2.5; `NEOVM_JIT_DIRECT_SHAPES`):
//! a short call of an `&optional` callee (2a), a call of a `&rest` callee
//! (2d), a call of a constant byte-code object (2b).
//!
//! Every scenario compares the observation -- the value or signal, the
//! frames `mapbacktrace` and `backtrace-frame` see (the call's own
//! arguments, never the nil padding or the consed list), the debugger,
//! `max-lisp-eval-depth`, redefinition, deopts -- between the shim, exact
//! direct calls, every shape direct, and every shape under the force
//! harness; and the shaped run must bypass the shim.

use super::*;

/// The modes a shaped scenario runs in, in [`shaped_differential`]'s order.
const SHAPED_MODES: [(Mode, DirectShapesKnob); 4] = [
    (Mode::Shim, DirectShapesKnob::OFF),
    (Mode::Direct, DirectShapesKnob::OFF),
    (Mode::Direct, DirectShapesKnob::ALL),
    (Mode::DirectForcedSlow, DirectShapesKnob::ALL),
];

/// Run a scenario in every [`SHAPED_MODES`] mode: the observations must be
/// equal, and the shaped run must emit more direct sites than the
/// exact-only direct run (its shaped sites). Returns the runs in that order.
fn shaped_differential(program: &'static str, observe: &'static str) -> [Run; 4] {
    let runs = SHAPED_MODES.map(|(mode, shapes)| run_in_with(mode, shapes, program, observe));
    for (run, (mode, shapes)) in runs.iter().zip(SHAPED_MODES) {
        assert_eq!(
            run.out, runs[0].out,
            "{mode:?} {shapes:?} observes what the shim does"
        );
    }
    assert!(
        runs[2].direct_sites > runs[1].direct_sites,
        "the shapes emit direct sites: {} vs {}",
        runs[2].direct_sites,
        runs[1].direct_sites
    );
    runs
}

/// A named-shape scenario's shaped sites hit: the shaped run enters the
/// spec shim less than the exact-only direct run.
fn assert_shaped_calls_bypass_the_shim(runs: &[Run; 4]) {
    assert!(
        runs[2].shim_calls < runs[1].shim_calls,
        "shaped direct calls bypass the shim: {} vs {}",
        runs[2].shim_calls,
        runs[1].shim_calls
    );
}

/// 2a: short calls of `&optional` callees pass nil for the missing slots;
/// a full call passes through; the frames record the call's own arguments
/// (as GNU's `Bcall` does: `record_in_backtrace` of the call's NARGS).
#[test]
fn optional_short_calls_pass_nil_and_record_their_own_arguments() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dso-seen nil)
  (defun neovm--dso-show ()
    (let (fs)
      (mapbacktrace
       (lambda (_evald f args _flags)
         (unless (eq f 'mapbacktrace)
           (push (list (if (symbolp f) f (type-of f)) args) fs))))
      (setq neovm--dso-seen (seq-take (nreverse fs) 3))
      (list (backtrace-frame 1 'neovm--dso-show))))
  (defun neovm--dso-one (a &optional b c)
    (if (eq a 'show) (neovm--dso-show) (list a b c)))
  (defun neovm--dso-two (a b &optional c d e)
    (if (eq a 'show) (neovm--dso-show) (list a b c d e)))
  (defun neovm--dso-caller (x)
    (list (neovm--dso-one x) (neovm--dso-one x 1) (neovm--dso-one x 1 2)
          (neovm--dso-two x 1) (neovm--dso-two x 1 2 3)))
  (dolist (f '(neovm--dso-show neovm--dso-one neovm--dso-two neovm--dso-caller))
    (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--dso-caller i)))"#;
    let runs = shaped_differential(
        PROGRAM,
        r#"(list (neovm--dso-caller 7)
              (progn (neovm--dso-one 'show) neovm--dso-seen)
              (progn (neovm--dso-caller 'show) neovm--dso-seen))"#,
    );
    assert_shaped_calls_bypass_the_shim(&runs);
    let shim = &runs[0];
    assert!(
        shim.out
            .starts_with("(((7 nil nil) (7 1 nil) (7 1 2) (7 1 nil nil nil) (7 1 2 3 nil))"),
        "{}",
        shim.out
    );
    assert!(
        shim.out
            .contains("(neovm--dso-two (show 1 2 3)) (neovm--dso-caller (show))"),
        "{}",
        shim.out
    );
}

/// 2a: a short call whose callee is redefined to require more arguments
/// signals GNU's `wrong-number-of-arguments` with the new definition's
/// arity, and the site keeps working once it is restored.
#[test]
fn a_short_call_of_a_redefined_callee_signals_wrong_number_of_arguments() {
    const PROGRAM: &str = r#"(progn
  (defun neovm--dsr-callee (a &optional b) (if b (+ a b) (* a 2)))
  (defun neovm--dsr-caller (x) (neovm--dsr-callee x))
  (byte-compile 'neovm--dsr-callee)
  (byte-compile 'neovm--dsr-caller)
  (dotimes (i neovm--dc-warm) (neovm--dsr-caller i)))"#;
    shaped_differential(
        PROGRAM,
        r#"(let ((old (symbol-function 'neovm--dsr-callee)))
          (list (neovm--dsr-caller 4)
                (progn (fset 'neovm--dsr-callee (byte-compile (lambda (a b) (list a b))))
                       (condition-case err (neovm--dsr-caller 5)
                         (wrong-number-of-arguments (list (car err) (car (cddr err))))))
                (progn (fset 'neovm--dsr-callee old) (neovm--dsr-caller 6))))"#,
    );
}

/// 2a: a precise deopt in a short-called callee resumes in Tier-0 with the
/// nil-padded frame, and the caller continues natively.
#[test]
fn a_deopt_in_a_short_called_callee_resumes() {
    const PROGRAM: &str = r#"(progn
  (defun neovm--dsd-callee (a &optional b) (if b (car b) (+ a 1)))
  (defun neovm--dsd-caller (x y) (list (neovm--dsd-callee x) (neovm--dsd-callee x y)))
  (byte-compile 'neovm--dsd-callee)
  (byte-compile 'neovm--dsd-caller)
  (dotimes (i neovm--dc-warm) (neovm--dsd-caller i '(1))))"#;
    shaped_differential(
        PROGRAM,
        r#"(list (neovm--dsd-caller 1.5 '(2))
              (condition-case err (neovm--dsd-caller 1 5) (error err))
              (neovm--dsd-caller 3 '(4)))"#,
    );
}

/// 2a: a recursion through a short call stops at `max-lisp-eval-depth` at
/// the same call, and `debug-on-next-call` takes the short site through the
/// debugger.
#[test]
fn a_short_call_recursion_keeps_the_depth_limit_and_the_debugger() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dsl-count 0)
  (defvar neovm--dsl-log nil)
  (defvar neovm--dsl-arm nil)
  (defun neovm--dsl-rec (n &optional acc)
    (setq neovm--dsl-count (1+ neovm--dsl-count))
    (when (and neovm--dsl-arm (= n 3)) (setq debug-on-next-call t))
    (if (= n 0) (length acc) (neovm--dsl-rec (1- n))))
  (byte-compile 'neovm--dsl-rec)
  (dotimes (_ 60) (neovm--dsl-rec 100)))"#;
    shaped_differential(
        PROGRAM,
        r#"(list (let ((max-lisp-eval-depth 300))
                (setq neovm--dsl-count 0)
                (list (condition-case err (neovm--dsl-rec 1000) (error err)) neovm--dsl-count))
              (let ((debugger (lambda (&rest args)
                                (push (car args) neovm--dsl-log)
                                (if (eq (car args) 'exit) (cadr args) nil)))
                    (neovm--dsl-arm t))
                (list (neovm--dsl-rec 5) (reverse neovm--dsl-log) debug-on-next-call)))"#,
    );
}

/// 2a: a frame flagged by `backtrace-debug` inside a short-called callee
/// returns through the exit debugger.
#[test]
fn a_flagged_short_call_frame_returns_through_the_exit_debugger() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dsx-log nil)
  (defvar neovm--dsx-arm nil)
  (defun neovm--dsx-callee (x &optional y)
    (when neovm--dsx-arm (backtrace-debug 0 t 'neovm--dsx-callee))
    (if y (+ x y) (* x 2)))
  (defun neovm--dsx-caller (x) (list (neovm--dsx-callee x) (neovm--dsx-callee x 1)))
  (byte-compile 'neovm--dsx-callee)
  (byte-compile 'neovm--dsx-caller)
  (dotimes (i neovm--dc-warm) (neovm--dsx-caller i)))"#;
    shaped_differential(
        PROGRAM,
        r#"(let ((debugger (lambda (&rest args)
                           (push args neovm--dsx-log)
                           (if (eq (car args) 'exit) (* 10 (cadr args)) nil)))
              (neovm--dsx-arm t))
          (list (neovm--dsx-caller 3) (reverse neovm--dsx-log)))"#,
    );
}

/// 2d: a call of a `&rest` callee conses a fresh list of the arguments past
/// its slots (nil when there are none), which the callee may mutate, and
/// which survives a collection inside the callee; the frames record the
/// call's own arguments.
#[test]
fn rest_calls_cons_a_fresh_list_and_record_their_own_arguments() {
    const PROGRAM: &str = r#"(progn
  (defvar neovm--dsrl-seen nil)
  (defvar neovm--dsrl-gc nil)
  (defun neovm--dsrl-show ()
    (let (fs)
      (mapbacktrace
       (lambda (_evald f args _flags)
         (unless (eq f 'mapbacktrace)
           (push (list (if (symbolp f) f (type-of f)) args) fs))))
      (setq neovm--dsrl-seen (seq-take (nreverse fs) 3))
      0))
  (defun neovm--dsrl-one (a &rest r)
    (when neovm--dsrl-gc (garbage-collect))
    (if (eq a 'show) (neovm--dsrl-show)
      (when r (setcar r (list (car r))))
      (cons a r)))
  (defun neovm--dsrl-opt (a &optional b &rest r) (list a b r))
  (defun neovm--dsrl-caller (x)
    (list (neovm--dsrl-one x) (neovm--dsrl-one x 1) (neovm--dsrl-one x 1 2 3 4 5)
          (neovm--dsrl-opt x) (neovm--dsrl-opt x 1) (neovm--dsrl-opt x 1 2 3)))
  (dolist (f '(neovm--dsrl-show neovm--dsrl-one neovm--dsrl-opt neovm--dsrl-caller))
    (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--dsrl-caller i)))"#;
    let runs = shaped_differential(
        PROGRAM,
        r#"(list (neovm--dsrl-caller 7)
              (let ((neovm--dsrl-gc t)) (neovm--dsrl-caller 8))
              (progn (neovm--dsrl-caller 'show) neovm--dsrl-seen))"#,
    );
    assert_shaped_calls_bypass_the_shim(&runs);
    let shim = &runs[0];
    assert!(
        shim.out
            .starts_with("(((7) (7 (1)) (7 (1) 2 3 4 5) (7 nil nil) (7 1 nil) (7 1 (2 3)))"),
        "{}",
        shim.out
    );
    assert!(
        shim.out.contains(
            "((neovm--dsrl-show nil) (neovm--dsrl-one (show 1 2 3 4 5)) (neovm--dsrl-caller (show)))"
        ),
        "{}",
        shim.out
    );
}

/// 2d: a `&rest` callee redefined to an exact one of the same arity is
/// called with the arguments as laid out, never the list (the site's key
/// check), and the reverse redefinition conses again.
#[test]
fn a_rest_site_never_enters_a_redefined_exact_callee_with_its_list() {
    const PROGRAM: &str = r#"(progn
  (defun neovm--dsre-callee (a &rest r) (list 'rest a r))
  (defun neovm--dsre-caller (x) (neovm--dsre-callee x 1))
  (byte-compile 'neovm--dsre-callee)
  (byte-compile 'neovm--dsre-caller)
  (dotimes (i neovm--dc-warm) (neovm--dsre-caller i)))"#;
    shaped_differential(
        PROGRAM,
        r#"(let ((old (symbol-function 'neovm--dsre-callee)))
          (list (neovm--dsre-caller 4)
                (progn (fset 'neovm--dsre-callee (byte-compile (lambda (a b) (list 'exact a b))))
                       (dotimes (i 50) (neovm--dsre-caller i))
                       (neovm--dsre-caller 5))
                (progn (fset 'neovm--dsre-callee old)
                       (dotimes (i 50) (neovm--dsre-caller i))
                       (neovm--dsre-caller 6))))"#,
    );
}

/// 2d: a signal raised deep in a recursion through `&rest` calls reaches
/// the outer handler with every frame, and a deopt in a `&rest` callee
/// resumes with its list.
#[test]
fn signals_and_deopts_through_rest_calls() {
    const PROGRAM: &str = r#"(progn
  (define-error 'neovm--dss-err "direct-shapes test error")
  (defun neovm--dss-rec (n &rest tail)
    (if (= n 0) (signal 'neovm--dss-err (list 'bottom (length tail)))
      (1+ (neovm--dss-rec (1- n) n))))
  (defun neovm--dss-catch (n)
    (condition-case err (neovm--dss-rec n) (neovm--dss-err (list 'caught err))))
  (defun neovm--dss-sum (a &rest r) (if r (+ a (car r)) a))
  (defun neovm--dss-caller (x y) (list (neovm--dss-sum x y) (neovm--dss-sum x)))
  (dolist (f '(neovm--dss-rec neovm--dss-catch neovm--dss-sum neovm--dss-caller))
    (byte-compile f))
  (dotimes (i 60) (neovm--dss-catch 50) (neovm--dss-caller i 1)))"#;
    shaped_differential(
        PROGRAM,
        r#"(list (let ((max-lisp-eval-depth 5000)) (neovm--dss-catch 1000))
              (neovm--dss-caller 1.5 2)
              (neovm--dss-caller 3 4.5)
              (condition-case err (neovm--dss-caller 1 'x) (error err)))"#,
    );
}

/// T12 for the shapes: with the knob on a short call's and a `&rest`
/// call's slots arm the callee's own entry, and a constant callee's source
/// slot its leaf's; with it off they arm none (and there is no constant
/// site); a framed `&optional` callee never.
#[test]
fn shaped_slots_arm_the_callee_entry_only_under_the_knob() {
    const PROGRAM: &str = r#"(progn
  (require 'cl-lib)
  (defvar neovm--dsa-special nil)
  (defun neovm--dsa-opt (a &optional b) (if b (list a b) a))
  (defun neovm--dsa-rest (a &rest r) (if r (cons a r) a))
  (defun neovm--dsa-framed (a &optional b) (let ((neovm--dsa-special b)) (list a neovm--dsa-special)))
  (defun neovm--dsa-caller (x)
    (list (neovm--dsa-opt x) (neovm--dsa-rest x 1 2) (neovm--dsa-framed x)
          (cl-flet ((g (y) (list (neovm--dsa-opt y 1))))
            (list (g x) (g x)))))
  (dolist (f '(neovm--dsa-opt neovm--dsa-rest neovm--dsa-framed neovm--dsa-caller))
    (byte-compile f))
  (dotimes (i 1500) (neovm--dsa-caller i)))"#;
    for shapes in [DirectShapesKnob::ALL, DirectShapesKnob::OFF] {
        let armed: Vec<(&str, bool)> = std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                crate::test_utils::init_test_tracing();
                force_profit_gate_for_test(false);
                crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
                crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
                force_direct_call_for_test(Some(true));
                force_direct_sites_for_test(Some(DirectSitesMode::All));
                force_direct_shapes_for_test(Some(shapes));
                let mut ev = crate::test_utils::runtime_startup_context();
                ev.eval_str(PROGRAM).expect("warmed");
                let caller = cached_leaf(&ev, "neovm--dsa-caller").expect("caller");
                let mut out: Vec<(&str, bool)> =
                    ["neovm--dsa-opt", "neovm--dsa-rest", "neovm--dsa-framed"]
                        .into_iter()
                        .map(|name| {
                            let callee = cached_leaf(&ev, name).expect("compiled");
                            let slot = slot_calling(caller, callee).expect("slot");
                            let direct = slot.direct_entry();
                            let framed_tag =
                                crate::emacs_core::jit::compile::spec_slot::DirectEntryTag::Framed
                                    as usize;
                            assert!(
                                direct.is_null()
                                    || direct == callee.entry
                                    || (callee.entry_shape == EntryShape::Framed
                                        && direct as usize == framed_tag),
                                "{name}"
                            );
                            (name, !direct.is_null())
                        })
                        .collect();
                // The `cl-flet` local's site: a source slot holding its
                // own leaf's entry.
                out.push((
                    "constant",
                    caller.source_spec_slots().any(|slot| {
                        !slot.direct_entry().is_null()
                            // SAFETY: an armed source slot's leaf is live.
                            && unsafe { (*slot.leaf_ptr()).entry } == slot.direct_entry()
                    }),
                ));
                force_direct_shapes_for_test(None);
                force_direct_sites_for_test(None);
                force_direct_call_for_test(None);
                out
            })
            .expect("spawn")
            .join()
            .expect("no crash");
        let on = shapes == DirectShapesKnob::ALL;
        assert_eq!(
            armed,
            vec![
                ("neovm--dsa-opt", on),
                ("neovm--dsa-rest", on),
                ("neovm--dsa-framed", false),
                ("constant", on)
            ],
            "{shapes:?}"
        );
    }
}

/// 2b: a `cl-flet` local the fuser leaves a call (its body calls) is
/// called directly, in straight-line code (the callee proven constant) and
/// in a loop (the callee crosses a block boundary and is guarded by its
/// bits); its frame records the called object, as GNU's `Bcall` records
/// `call_fun`; a deopt and a signal inside it behave as through the shim.
#[test]
fn constant_callees_are_called_directly_with_their_frames() {
    const PROGRAM: &str = r#"(progn
  (require 'cl-lib)
  (defvar neovm--dsc-seen nil)
  (defun neovm--dsc-show ()
    (let (fs)
      (mapbacktrace
       (lambda (_evald f args _flags)
         (unless (eq f 'mapbacktrace)
           (push (list (if (symbolp f) f (type-of f)) args) fs))))
      (setq neovm--dsc-seen (seq-take (nreverse fs) 3))
      0))
  (defun neovm--dsc-inner (y) (if (eq y 'show) (neovm--dsc-show) (car y)))
  (defun neovm--dsc-straight (x)
    (cl-flet ((g (y) (list (neovm--dsc-inner y) 2)))
      (list (g x) (g x))))
  (defun neovm--dsc-loop (l)
    (cl-flet ((f (y) (+ (neovm--dsc-inner y) 1)))
      (let ((s 0))
        (dolist (y l) (setq s (+ s (f y))))
        s)))
  (dolist (f '(neovm--dsc-show neovm--dsc-inner neovm--dsc-straight neovm--dsc-loop))
    (byte-compile f))
  (dotimes (i neovm--dc-warm) (neovm--dsc-straight (list i)) (neovm--dsc-loop '((1) (2)))))"#;
    let [shim, ..] = shaped_differential(
        PROGRAM,
        r#"(list (neovm--dsc-straight '(5))
              (neovm--dsc-loop '((1) (2) (3)))
              (progn (neovm--dsc-straight 'show) neovm--dsc-seen)
              (condition-case err (neovm--dsc-loop '((1) 2)) (error err))
              (neovm--dsc-loop '((1.5) (2))))"#,
    );
    assert!(
        shim.out
            .contains("(neovm--dsc-inner (show)) (byte-code-function (show))"),
        "{}",
        shim.out
    );
}

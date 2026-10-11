//! F-I2 (design `p2-3-inlining-multiframe-deopt` §8): does resuming a chain
//! of inlined frames equal Tier-0?
//!
//! Each program runs once under Tier-0 with the per-op probe recording
//! (`resumed_chain_probe`), then once per (op, materialized-prefix) pair
//! with the probe stopping the driver at that op and finishing the call as
//! a resumed chain built from the running frames. The chain run must return
//! the same result, and every op it executes -- in Tier-0 before the stop,
//! in the resumed frames after it -- must see the frame state the reference
//! run saw there: the same function and pc, `lisp_eval_depth` and
//! `max-lisp-eval-depth`, specpdl (each backtrace frame's function,
//! arguments and debug-on-exit flag, each binding), condition stack,
//! operand stack and JIT bind stack. The programs observe their own frames
//! (`mapbacktrace`, `backtrace-frame`), flag a middle frame for the exit
//! debugger, unwind dynamic bindings through signals and throws caught in
//! outer frames, and overflow `max-lisp-eval-depth` both through `Bcall`
//! and through a `mapc` callback, so a divergence in any of those shows in
//! the result or the trace.

use super::*;
use crate::emacs_core::bytecode::vm::resumed_chain_probe::{self as probe, FireRequest, Fired};
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;

/// Helpers every program shares: `neovm--ch-probe` logs the frames visible
/// from it, down to the program's entry function.
const PRELUDE: &str = r#"(progn
  (defvar neovm--ch-log nil)
  (defvar neovm--ch-dyn 0)
  (defvar neovm--ch-dbg nil)
  (defun neovm--ch-frames ()
    (let (fs)
      (mapbacktrace
       (lambda (evald f args flags)
         (push (list evald f args (plist-get flags :debug-on-exit)) fs)))
      (let (out)
        (while (and fs (not (eq (nth 1 (car fs)) 'neovm--ch-frames)))
          (setq fs (cdr fs)))
        (dolist (f fs)
          (push f out)
          (when (memq (nth 1 f) '(neovm--ch-run neovm--ch-drun neovm--ch-srun
                                  neovm--ch-trun neovm--ch-erun neovm--ch-rrun
                                  neovm--ch-mrun neovm--ch-grun))
            (setq fs nil)))
        out)))
  (defun neovm--ch-probe (tag)
    (push (cons tag (neovm--ch-frames)) neovm--ch-log)
    tag)
  (byte-compile 'neovm--ch-frames)
  (byte-compile 'neovm--ch-probe))"#;

/// Results and value flow through three levels, with one, two and three
/// arguments, and a callee that walks the backtrace.
const CALLS: &str = r#"(progn
  (defun neovm--ch-c (x y z) (neovm--ch-probe (list 'c x y z)) (* x 2))
  (defun neovm--ch-b (x y)
    (let ((r (neovm--ch-c (+ x y) y 'z)))
      (neovm--ch-probe 'b)
      (list r (backtrace-frame 1 'neovm--ch-b))))
  (defun neovm--ch-a (x)
    (let ((v (neovm--ch-b x 1)) (w (neovm--ch-b (1+ x) 2)))
      (cons v w)))
  (defun neovm--ch-run ()
    (setq neovm--ch-log nil)
    (list (neovm--ch-a 3) (nreverse neovm--ch-log)))
  (dolist (f '(neovm--ch-c neovm--ch-b neovm--ch-a neovm--ch-run)) (byte-compile f)))"#;

/// Dynamic bindings owned by middle frames, visible to and restored around
/// the inner calls.
const BINDINGS: &str = r#"(progn
  (defun neovm--ch-d3 (x) (neovm--ch-probe (list 'd3 neovm--ch-dyn)) (+ x neovm--ch-dyn))
  (defun neovm--ch-d2 (x)
    (let ((neovm--ch-dyn (* 10 x)))
      (list (neovm--ch-d3 x) neovm--ch-dyn)))
  (defun neovm--ch-d1 (x)
    (let ((neovm--ch-dyn (1+ x)))
      (list (neovm--ch-d2 x) (neovm--ch-d3 0) neovm--ch-dyn)))
  (defun neovm--ch-drun ()
    (setq neovm--ch-log nil)
    (list (neovm--ch-d1 5) neovm--ch-dyn (nreverse neovm--ch-log)))
  (dolist (f '(neovm--ch-d3 neovm--ch-d2 neovm--ch-d1 neovm--ch-drun)) (byte-compile f)))"#;

/// A signal from the innermost frame unwinds a middle frame's binding and is
/// caught by an outer frame's `condition-case`; an uncaught one leaves the
/// program.
const SIGNALS: &str = r#"(progn
  (defun neovm--ch-s3 (x) (neovm--ch-probe 's3) (car x))
  (defun neovm--ch-s2 (x)
    (let ((neovm--ch-dyn 'inner))
      (list (neovm--ch-s3 x) neovm--ch-dyn)))
  (defun neovm--ch-s1 (x)
    (condition-case err
        (neovm--ch-s2 x)
      (wrong-type-argument (list 'caught (cdr err) neovm--ch-dyn))))
  (defun neovm--ch-srun ()
    (setq neovm--ch-log nil)
    (list (neovm--ch-s1 '(1)) (neovm--ch-s1 1) neovm--ch-dyn
          (condition-case err (neovm--ch-s2 2) (error (list 'outer err neovm--ch-dyn)))
          (nreverse neovm--ch-log)))
  (dolist (f '(neovm--ch-s3 neovm--ch-s2 neovm--ch-s1 neovm--ch-srun)) (byte-compile f)))"#;

/// A throw from the innermost frame to a `catch` two frames out.
const THROWS: &str = r#"(progn
  (defun neovm--ch-t3 (x) (neovm--ch-probe 't3) (if (> x 2) (throw 'neovm--ch-tag (* x 10)) x))
  (defun neovm--ch-t2 (x) (let ((neovm--ch-dyn 'thrown)) (+ 1 (neovm--ch-t3 x))))
  (defun neovm--ch-t1 (x) (list (catch 'neovm--ch-tag (neovm--ch-t2 x)) neovm--ch-dyn))
  (defun neovm--ch-trun ()
    (setq neovm--ch-log nil)
    (list (neovm--ch-t1 1) (neovm--ch-t1 3) (nreverse neovm--ch-log)))
  (dolist (f '(neovm--ch-t3 neovm--ch-t2 neovm--ch-t1 neovm--ch-trun)) (byte-compile f)))"#;

/// The innermost frame flags its caller for the exit debugger, whose value
/// replaces the caller's (GNU `Breturn`, src/bytecode.c:899-902).
const DEBUG_ON_EXIT: &str = r#"(progn
  (defun neovm--ch-e3 (x)
    (neovm--ch-probe 'e3-before)
    (backtrace-debug 1 t 'neovm--ch-e3)
    (neovm--ch-probe 'e3-after)
    (* x 3))
  (defun neovm--ch-e2 (x) (+ 1 (neovm--ch-e3 x)))
  (defun neovm--ch-e1 (x) (list (neovm--ch-e2 x) 'after))
  (defun neovm--ch-erun ()
    (setq neovm--ch-log nil neovm--ch-dbg nil)
    (let ((debugger (lambda (&rest args) (push args neovm--ch-dbg) 100)))
      (list (neovm--ch-e1 2) neovm--ch-dbg (nreverse neovm--ch-log))))
  (dolist (f '(neovm--ch-e3 neovm--ch-e2 neovm--ch-e1 neovm--ch-erun)) (byte-compile f)))"#;

/// Recursion past `max-lisp-eval-depth` through `Bcall`: GNU raises the
/// limit to its floor of 100 and signals `error` there.
const DEPTH: &str = r#"(progn
  (defun neovm--ch-r (n) (if (= n 0) 0 (1+ (neovm--ch-r (1- n)))))
  (defun neovm--ch-rrun ()
    (list (condition-case err
              (let ((max-lisp-eval-depth 10)) (neovm--ch-r 500))
            (error (list 'err err)))
          max-lisp-eval-depth))
  (dolist (f '(neovm--ch-r neovm--ch-rrun)) (byte-compile f)))"#;

/// Recursion through `mapc` callbacks advances three GNU depth levels per
/// iteration: `Bcall` of the function, `Bcall` of `mapc`, and `Ffuncall` of
/// its callback. Three adjacent limits cover every phase, independently of
/// the entry depth, and therefore force an `excessive-lisp-nesting` from
/// the callback as well as `Bcall`'s `error`.
const HOF_DEPTH: &str = r#"(progn
  (defun neovm--ch-m (n)
    (if (= n 0) 0 (mapc (lambda (_x) (neovm--ch-m (1- n))) '(1)) n))
  (defun neovm--ch-mrun ()
    (list (condition-case err
              (let ((max-lisp-eval-depth 100)) (neovm--ch-m 500))
            (error (list 'err err)))
          (condition-case err
              (let ((max-lisp-eval-depth 101)) (neovm--ch-m 500))
            (error (list 'err err)))
          (condition-case err
              (let ((max-lisp-eval-depth 102)) (neovm--ch-m 500))
            (error (list 'err err)))))
  (dolist (f '(neovm--ch-m neovm--ch-mrun)) (byte-compile f)))"#;

/// Outer frames whose stacks alone hold fresh conses, across a collection
/// in the innermost frame.
const GC: &str = r#"(progn
  (defun neovm--ch-g3 (x) (neovm--ch-probe 'g3) (garbage-collect) (car x))
  (defun neovm--ch-g2 (c)
    (let ((d (cons (car c) (cdr c))))
      (+ (neovm--ch-g3 (list 7)) (car d) (cdr d) (length (make-list 50 d)))))
  (defun neovm--ch-g1 () (neovm--ch-g2 (cons 1 2)))
  (defun neovm--ch-grun ()
    (setq neovm--ch-log nil)
    (list (neovm--ch-g1) (length neovm--ch-log)))
  (dolist (f '(neovm--ch-g3 neovm--ch-g2 neovm--ch-g1 neovm--ch-grun)) (byte-compile f)))"#;

fn context_with(program: &str) -> Context {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::jit::force_jit_off_for_test(true);
    let mut ev = crate::test_utils::runtime_startup_context();
    ev.eval_str(PRELUDE).expect("prelude");
    ev.eval_str(program).expect("program");
    ev
}

fn printed(result: &Result<Value, crate::emacs_core::error::EvalError>) -> String {
    match result {
        Ok(value) => print_value(value),
        Err(_) => crate::emacs_core::error::format_eval_result(result),
    }
}

/// One run of `call`, the probe armed with `fire`. Collection is inhibited
/// unless `collect`, so heap identities compare across runs.
fn run(
    ev: &mut Context,
    call: &str,
    fire: Option<FireRequest>,
    collect: bool,
) -> (String, probe::ChainProbe) {
    let body = |ev: &mut Context| {
        let entry = CleanupState::of(ev);
        probe::arm(ev, fire);
        let result = ev.eval_str(call);
        let seen = probe::disarm();
        assert_eq!(
            CleanupState::of(ev),
            entry,
            "{call}, {fire:?}: the complete call restored its runtime state"
        );
        (printed(&result), seen)
    };
    if collect {
        body(ev)
    } else {
        ev.with_gc_inhibited(body)
    }
}

/// The state outside the whole call, including the final return's teardown,
/// which a before-op trace cannot see. The limit itself can change as an
/// observable floor raise; the running depth and owned extents must restore.
#[derive(Debug, PartialEq, Eq)]
struct CleanupState {
    depth: usize,
    specpdl: usize,
    conditions: usize,
    operands: usize,
    frames: usize,
    jit_binds: usize,
    backtrace_args: usize,
    lexenv: Value,
}

impl CleanupState {
    fn of(ev: &Context) -> Self {
        Self {
            depth: ev.depth,
            specpdl: ev.specpdl.len(),
            conditions: ev.condition_stack.len(),
            operands: ev.bc_buf.len(),
            frames: ev.bc_frames.len(),
            jit_binds: ev.jit_bind_stack.len(),
            backtrace_args: ev.backtrace_args_stack_len_for_test(),
            lexenv: ev.lexenv,
        }
    }
}

#[derive(Default, Debug)]
struct Verdict {
    resumed: usize,
    refused: usize,
    by_refusal: std::collections::BTreeMap<&'static str, usize>,
    /// Resumes by chain length (levels above the physical frame).
    by_levels: std::collections::BTreeMap<usize, usize>,
    /// Resumes that left some inlined frame virtual.
    with_virtual: usize,
    /// Resumes that materialized some inlined frame.
    with_materialized: usize,
}

/// The materialized-prefix lengths to try for a chain of `levels`.
fn prefixes(levels: usize) -> Vec<usize> {
    if levels <= 4 {
        return (0..=levels).collect();
    }
    let mut picks = vec![0, 1, levels / 2, levels - 1, levels];
    picks.dedup();
    picks
}

/// Run `call` once as the reference, then resumed at every op that `pick`
/// admits, under every prefix length; panic on the first divergence.
fn falsify(
    ev: &mut Context,
    call: &str,
    collect: bool,
    pick: impl Fn(usize, usize) -> bool,
) -> Verdict {
    let (expected, reference) = run(ev, call, None, collect);
    assert!(
        !reference.trace.is_empty(),
        "the program ran byte-code under the probe"
    );
    let mut verdict = Verdict::default();
    let mut runs = 0usize;
    for op in 0..reference.trace.len() {
        let levels = reference.levels[op];
        if !pick(op, levels) {
            continue;
        }
        for materialized in prefixes(levels) {
            runs += 1;
            if !collect && runs.is_multiple_of(64) {
                ev.eval_str("(garbage-collect)").expect("collects");
            }
            let request = FireRequest { op, materialized };
            let (result, seen) = run(ev, call, Some(request), collect);
            match seen.fired {
                Fired::Resumed { .. } => {}
                Fired::Refused(reason) => {
                    verdict.refused += 1;
                    *verdict.by_refusal.entry(reason).or_default() += 1;
                    continue;
                }
                Fired::No => panic!("op {op} was not reached on the rerun"),
            }
            assert_eq!(
                result, expected,
                "resumed at op {op} ({levels} levels, {materialized} materialized): result"
            );
            assert_eq!(
                seen.trace.len(),
                reference.trace.len(),
                "resumed at op {op} ({levels} levels, {materialized} materialized): op count"
            );
            for (index, (got, want)) in seen.trace.iter().zip(&reference.trace).enumerate() {
                let same = if collect {
                    got.shape() == want.shape()
                } else {
                    got == want
                };
                assert!(
                    same,
                    "resumed at op {op} ({levels} levels, {materialized} materialized): \
                     op {index} saw {got:?}, Tier-0 saw {want:?}"
                );
            }
            verdict.resumed += 1;
            *verdict.by_levels.entry(levels).or_default() += 1;
            if materialized < levels {
                verdict.with_virtual += 1;
            }
            if materialized > 0 {
                verdict.with_materialized += 1;
            }
        }
    }
    tracing::info!(target: "neovm::test", ?verdict, ops = reference.trace.len(), "F-I2");
    verdict
}

fn every_op(_op: usize, _levels: usize) -> bool {
    true
}

fn assert_two_and_three_levels(verdict: &Verdict) {
    assert!(
        verdict.by_levels.get(&2).copied().unwrap_or(0) > 0,
        "{verdict:?}"
    );
    assert!(
        verdict.by_levels.get(&3).copied().unwrap_or(0) > 0,
        "{verdict:?}"
    );
}

#[test]
fn chain_resume_equals_tier0_through_three_levels_of_calls() {
    let mut ev = context_with(CALLS);
    let verdict = falsify(&mut ev, "(neovm--ch-run)", false, every_op);
    assert_two_and_three_levels(&verdict);
    assert!(
        verdict.with_virtual > 0 && verdict.with_materialized > 0,
        "{verdict:?}"
    );
}

#[test]
fn chain_resume_equals_tier0_with_dynamic_bindings_in_middle_frames() {
    let mut ev = context_with(BINDINGS);
    let verdict = falsify(&mut ev, "(neovm--ch-drun)", false, every_op);
    assert_two_and_three_levels(&verdict);
    assert!(verdict.resumed > 0 && verdict.refused > 0, "{verdict:?}");
}

#[test]
fn chain_resume_equals_tier0_when_a_signal_unwinds_through_outer_frames() {
    let mut ev = context_with(SIGNALS);
    let verdict = falsify(&mut ev, "(neovm--ch-srun)", false, every_op);
    assert_two_and_three_levels(&verdict);
}

#[test]
fn chain_resume_equals_tier0_when_a_throw_crosses_frames() {
    let mut ev = context_with(THROWS);
    let verdict = falsify(&mut ev, "(neovm--ch-trun)", false, every_op);
    assert_two_and_three_levels(&verdict);
}

#[test]
fn chain_resume_runs_the_exit_debugger_of_a_flagged_middle_frame() {
    let mut ev = context_with(DEBUG_ON_EXIT);
    let (reference, _) = run(&mut ev, "(neovm--ch-erun)", None, false);
    assert!(
        reference.starts_with("((100 after) ((exit 7)) "),
        "the exit debugger ran once and its value replaced the call's: {reference}"
    );
    let verdict = falsify(&mut ev, "(neovm--ch-erun)", false, every_op);
    assert_two_and_three_levels(&verdict);
    assert!(
        verdict.with_materialized > 0 && verdict.refused > 0,
        "{verdict:?}"
    );
}

#[test]
fn chain_resume_equals_tier0_at_the_bcall_depth_limit() {
    let mut ev = context_with(DEPTH);
    let (reference, _) = run(&mut ev, "(neovm--ch-rrun)", None, false);
    assert!(
        reference.contains("Lisp nesting exceeds"),
        "Bcall's error: {reference}"
    );
    // The deep end of the recursion, where a chain is long and the next
    // call overflows; and a sample of the rest.
    let verdict = falsify(&mut ev, "(neovm--ch-rrun)", false, |op, levels| {
        levels >= 70 || op.is_multiple_of(29)
    });
    assert!(
        verdict.by_levels.keys().any(|&levels| levels >= 70),
        "{verdict:?}"
    );
}

#[test]
fn chain_resume_equals_tier0_at_the_mapc_callback_depth_limit() {
    let mut ev = context_with(HOF_DEPTH);
    let (reference, _) = run(&mut ev, "(neovm--ch-mrun)", None, false);
    assert!(
        reference.contains("excessive-lisp-nesting"),
        "mapc's Ffuncall callback crossed the depth limit: {reference}"
    );
    assert!(
        reference.contains("Lisp nesting"),
        "a Bcall also crossed the depth limit: {reference}"
    );
    let verdict = falsify(&mut ev, "(neovm--ch-mrun)", false, |op, levels| {
        levels >= 1 && op.is_multiple_of(3)
    });
    assert!(verdict.resumed > 0, "{verdict:?}");
}

#[test]
fn chain_resume_keeps_outer_stacks_alive_across_collections() {
    let mut ev = context_with(GC);
    ev.gc_stress = true;
    let verdict = falsify(&mut ev, "(neovm--ch-grun)", true, |_, levels| levels >= 2);
    ev.gc_stress = false;
    assert_two_and_three_levels(&verdict);
}

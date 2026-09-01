//! Oracle guards for GNU's entry/exit debugger hooks: `debug-on-next-call`
//! and `debug-on-exit`.
//!
//! Ledgers 135 and 168 both reached `debug-on-next-call` by probing its VALUE
//! and both concluded the value was not the bug.  They were right, and the
//! reason is `do_debug_on_call` (`src/eval.c:335-341`):
//!
//! ```c
//! debug_on_next_call = 0;                                        /* 338 */
//! set_backtrace_debug_on_exit (specpdl_ref_to_ptr (count), true); /* 339 */
//! call_debugger (list1 (code));                                  /* 340 */
//! ```
//!
//! Setting the variable ARMS the debugger and entering the debugger CLEARS it
//! again, so an assignment probe can only ever read back what the mechanism
//! left behind.  Line 339 is why `debug-on-exit` belongs in the same file:
//! the entry and the exit are the same four lines.
//!
//! GNU tests the arm at exactly three dispatch sites -- `eval_sub`
//! (`src/eval.c:2601`, code `Qt`), `Ffuncall` (`src/eval.c:3189`, code
//! `Qlambda`) and the bytecode `Bcall` (`src/bytecode.c:798`, code `Qlambda`)
//! -- and each tests it immediately after `record_in_backtrace`, because line
//! 339 needs a frame.  `Fapply` has no check of its own; it reaches
//! `Ffuncall`.  The inline bytecode opcodes (`src/bytecode.c:1412-1545`) are
//! not `Bcall` and are deliberately not armed.
//!
//! Every pin below binds `debugger` to a recorder and reports the argument
//! lists it was handed, so the tests are about the handshake rather than
//! about stderr rendering.  `((t) (exit 1))` reads: entry debugger with `Qt`,
//! then the SAME frame's exit debugger with the value 1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Ledger 135's probe, unchanged, and the answer entry 172 fixed.
///
/// The middle element is `nil` in GNU because `set-default` coerces 5 to `t`
/// through `Lisp_Fwd_Bool` (`src/data.c:1485-1487`) and thereby ARMS, and the
/// very next cons form disarms before `default-value` can read it.  The third
/// is `t` in both editors because `progn` and `setq` are special forms and a
/// bare symbol never reaches `eval_sub`'s cons arm (`src/eval.c:2560-2576`),
/// so no call intervenes.
#[test]
fn oracle_debug_on_next_call_reads_back_nil_after_a_call_intervenes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(prog1
    (list (default-value 'debug-on-next-call)
          (progn (set-default 'debug-on-next-call 5)
                 (default-value 'debug-on-next-call))
          (progn (setq debug-on-next-call t) debug-on-next-call))
  (setq debug-on-next-call nil))"#;
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The arm reaching each interpreter dispatch shape, with the debugger's own
/// arguments recorded.
///
/// Every row answers `nil` for the value because the recorder returns `nil`
/// and `val = call_debugger (list2 (Qexit, val))` (`src/eval.c:2778`) is an
/// ASSIGNMENT -- the exit debugger's return value replaces the call's.  Every
/// row's code is `t` because `eval_sub` gets there first (`src/eval.c:2602`),
/// including for the special form and for `apply`, whose own dispatch has no
/// check.  `one-shot` proves the arm is spent: two calls, one entry.
/// `under-inhibit-debugger` proves `inhibit-debugger` does not gate this path
/// -- `call_debugger` binds it (`src/eval.c:309`) without testing it.
#[test]
fn oracle_debug_on_next_call_arms_every_interpreter_dispatch_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil) (out nil))
  (let ((debugger (lambda (&rest args) (push args log) nil)))
    (dolist (probe '((interpreted-subr (car '(1 2 3)))
                     (two-argument-frame (cons 1 2))
                     (special-form-frame (if t 'yes 'no))
                     (apply-form (apply #'car '((5 6))))
                     (mapcar-form (mapcar #'1+ '(1 2 3)))
                     (one-shot (progn (car '(1 2)) (cdr '(3 4))))
                     (under-inhibit-debugger (let ((inhibit-debugger t)) (car '(1))))))
      (setq log nil)
      (let ((v (eval (list 'progn '(setq debug-on-next-call t) (cadr probe)) t)))
        (push (list (car probe) v (reverse log) debug-on-next-call) out))
      (setq debug-on-next-call nil)))
  (prog1 (nreverse out) (setq debug-on-next-call nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((interpreted-subr nil ((t) (exit 1)) nil) (two-argument-frame nil ((t) (exit (1 . 2))) nil) (special-form-frame nil ((t) (exit yes)) nil) (apply-form nil ((t) (exit 5)) nil) (mapcar-form nil ((t) (exit (2 3 4))) nil) (one-shot nil ((t) (exit (4))) nil) (under-inhibit-debugger nil ((t) (exit 1)) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The bytecode half and the `backtrace-debug` half.
///
/// `bytecode-bcall` is the only row whose code is `lambda`: it is GNU's
/// `Bcall` check (`src/bytecode.c:799`), reached because the arming `setq`
/// runs inside a byte-compiled body.  The `backtrace-debug` rows use no
/// `debug-on-next-call` at all -- LEVEL 0 is `backtrace-debug`'s OWN frame
/// (`get_backtrace_starting_at (Qnil)` is `backtrace_top ()`,
/// `src/eval.c:3988`), so the caller is untouched and still answers 21, while
/// LEVEL 1 flags the caller and the debugger replaces its 21 with `nil`.
///
/// The last two rows are the silences.  A nil FLAG assigns rather than arms
/// (`src/eval.c:4026`), and a `throw` out of a flagged frame calls nothing at
/// all, because `unbind_to` pops `SPECPDL_BACKTRACE` with a bare `break`
/// (`src/eval.c:3818-3820`).
#[test]
fn oracle_bytecode_bcall_and_backtrace_debug_reach_the_same_mechanism() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil) (out nil))
  (defalias 'l172-callee (lambda (x) (1+ x)))
  (defalias 'l172-bc (byte-compile (lambda () (setq debug-on-next-call t) (l172-callee 4))))
  (defalias 'l172-exit-caller (lambda (x) (backtrace-debug 1 t) (* x 3)))
  (defalias 'l172-exit-self (lambda (x) (backtrace-debug 0 t) (* x 3)))
  (defalias 'l172-clear (lambda (x) (backtrace-debug 1 t) (backtrace-debug 1 nil) (* x 3)))
  (defalias 'l172-throw (lambda () (backtrace-debug 1 t) (throw 'l172 'thrown)))
  (defalias 'l172-flags
    (lambda ()
      (backtrace-debug 1 t)
      (let (seen)
        (backtrace-frame--internal
         (lambda (_evald func _args flags) (setq seen (list func flags)) nil)
         0 'l172-flags)
        seen)))
  (let ((debugger (lambda (&rest args) (push args log) nil)))
    (dolist (probe '((bytecode-bcall (l172-bc))
                     (backtrace-debug-level-1 (l172-exit-caller 7))
                     (backtrace-debug-level-0 (l172-exit-self 7))
                     (backtrace-debug-nil-flag (l172-clear 7))
                     (backtrace-debug-flag-visible (l172-flags))
                     (throw-out-of-flagged-frame (catch 'l172 (l172-throw)))))
      (setq log nil)
      (let ((v (eval (cadr probe) t)))
        (push (list (car probe) v (reverse log) debug-on-next-call) out))
      (setq debug-on-next-call nil)))
  (prog1 (nreverse out) (setq debug-on-next-call nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((bytecode-bcall nil ((lambda) (exit 5)) nil) (backtrace-debug-level-1 nil ((exit 21)) nil) (backtrace-debug-level-0 21 ((exit t)) nil) (backtrace-debug-nil-flag 21 nil nil) (backtrace-debug-flag-visible nil ((exit (l172-flags (:debug-on-exit t)))) nil) (throw-out-of-flagged-frame thrown nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `call_debugger` stamps the input-event count it was entered at.
///
/// `when_entered_debugger = num_nonmacro_input_events` (`src/eval.c:299`) is
/// the second thing `call_debugger` does, and GNU exposes the slot as the
/// `DEFVAR_INT` `internal-when-entered-debugger` (`src/eval.c:4553-4554`).
/// `init_eval` seeds it to `-1` (`src/eval.c:251`), so a batch process that
/// has entered the debugger once reads `0` -- it never consumed a non-macro
/// input event.  Before this entry the port never wrote the slot, because it
/// had no `call_debugger` of its own to write it in.
#[test]
fn oracle_call_debugger_stamps_internal_when_entered_debugger() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((before internal-when-entered-debugger))
  (let ((debugger (lambda (&rest _args) nil)))
    (setq debug-on-next-call t)
    (car '(1)))
  (prog1 (list before internal-when-entered-debugger num-nonmacro-input-events)
    (setq debug-on-next-call nil)))"#;
    let expect = expect_test::expect![[r#""OK (-1 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// The arm must fire ONCE per call, and GNU's `eval_sub` is the only thing
/// that tests it for a form -- the subr and lambda dispatch below it do not.
///
/// This is the pin that would catch an over-eager port.  `eval_sub` tests the
/// arm at `src/eval.c:2601`, BEFORE it evaluates the arguments; if an argument
/// then arms the flag, GNU's own dispatch never looks again -- a subr is
/// invoked directly (`src/eval.c:2666-2700`), a lambda through `apply_lambda`
/// (`src/eval.c:2771`), and neither goes through `Ffuncall`.  So the arm
/// survives the call and is spent by the NEXT form.
///
/// `subr-callee`, `bytecode-callee` and `many-arg-subr-callee` therefore
/// answer their real values (1, 3, `(0 1)`) and the debugger fires on the
/// following `setq`.  `lambda-callee` is the exception and it proves the rule:
/// an interpreted body is itself `eval_sub`, so `(* x 2)` is the next form and
/// the debugger replaces its 2 with `nil`.
///
/// A port that re-tested the arm at its own funcall/apply layer would show
/// `(lambda)` here instead of `t`, or two entries where GNU has one.
#[test]
fn oracle_the_arm_is_spent_by_eval_sub_and_not_re_tested_by_the_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((log nil) (out nil))
  (defalias 'l172-lam (lambda (x) (* x 2)))
  (defalias 'l172-bcfn (byte-compile (lambda (x) (* x 3))))
  (let ((debugger (lambda (&rest args) (push args log) nil)))
    (dolist (probe '((subr-callee (identity (progn (setq debug-on-next-call t) 1)))
                     (lambda-callee (l172-lam (progn (setq debug-on-next-call t) 1)))
                     (bytecode-callee (l172-bcfn (progn (setq debug-on-next-call t) 1)))
                     (many-arg-subr-callee (list 0 (progn (setq debug-on-next-call t) 1)))))
      (setq log nil)
      (setq debug-on-next-call nil)
      (let ((res (eval (cadr probe) t)))
        (setq debug-on-next-call nil)
        (push (list (car probe) res (reverse log)) out))))
  (prog1 (nreverse out) (setq debug-on-next-call nil)))"#;
    let expect = expect_test::expect![[
        r#""OK ((subr-callee 1 ((t) (exit nil))) (lambda-callee nil ((t) (exit 2))) (bytecode-callee 3 ((t) (exit nil))) (many-arg-subr-callee (0 1) ((t) (exit nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `Fbacktrace_debug` checks LEVEL twice with two different predicates, and a
/// single check cannot produce both answers.
///
/// `CHECK_FIXNUM (level)` runs first (`src/eval.c:4022`) and names `fixnump`;
/// `get_backtrace_frame` then runs `CHECK_FIXNAT (nframes)`
/// (`src/eval.c:3987`) and names `wholenump`.  Walking off the end of the
/// backtrace, and naming a BASE function that is not on the stack, are both
/// silent -- `if (backtrace_p (pdl))` (`src/eval.c:4025`) simply finds
/// nothing to set.
#[test]
fn oracle_backtrace_debug_checks_level_with_two_different_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(mapcar (lambda (form) (list form (condition-case e (eval form t) (error e))))
        '((backtrace-debug)
          (backtrace-debug 0)
          (backtrace-debug -1 nil)
          (backtrace-debug "x" nil)
          (backtrace-debug 1.0 nil)
          (backtrace-debug 99999 nil)
          (backtrace-debug 1 2 3 4)
          (backtrace-debug 0 nil 'no-such-function)))"#;
    let expect = expect_test::expect![[
        r#""OK (((backtrace-debug) (wrong-number-of-arguments backtrace-debug 0)) ((backtrace-debug 0) (wrong-number-of-arguments backtrace-debug 1)) ((backtrace-debug -1 nil) (wrong-type-argument wholenump -1)) ((backtrace-debug \"x\" nil) (wrong-type-argument fixnump \"x\")) ((backtrace-debug 1.0 nil) (wrong-type-argument fixnump 1.0)) ((backtrace-debug 99999 nil) nil) ((backtrace-debug 1 2 3 4) (wrong-number-of-arguments backtrace-debug 4)) ((backtrace-debug 0 nil 'no-such-function) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

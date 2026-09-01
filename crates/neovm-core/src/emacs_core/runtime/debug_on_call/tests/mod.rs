//! Ledger 172: the `debug-on-next-call` / `debug-on-exit` arming handshake.
//!
//! Every expectation in this file was measured under GNU Emacs 31.0.90
//! (`emacs -Q --batch`) before it was written; the recording probe is
//! reproduced in each test's doc comment so a future reader can re-run it
//! rather than trust the constant.
//!
//! The shared shape is: bind `debugger` to a recorder, run a case, and read
//! back `(RESULT LOG debug-on-next-call)`.  `LOG` is the list of argument
//! lists the debugger was called with, oldest first -- so `((t) (exit 1))`
//! means "entry debugger with `Qt`, then exit debugger with the value 1".

use super::super::eval::Context;
use super::super::print::print_value;

/// `debugger` is a recorder; every case reports
/// `(RESULT LOG debug-on-next-call)`.
fn recorder_context() -> Context {
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (defvar l172-log nil)
             (setq debugger (lambda (&rest args) (setq l172-log (cons args l172-log)) nil))
             nil)"#,
    )
    .expect("recorder setup should evaluate");
    eval
}

fn case(eval: &mut Context, body: &str) -> String {
    let form = format!(
        "(progn (setq l172-log nil)
                (let ((v (progn {body})))
                  (prog1 (list v (reverse l172-log) debug-on-next-call)
                    (setq debug-on-next-call nil))))"
    );
    let value = eval
        .eval_str(&form)
        .unwrap_or_else(|err| panic!("case should evaluate: {err:?}\n{form}"));
    print_value(&value)
}

/// GNU, `emacs -Q --batch`:
/// `(setq debug-on-next-call t) (car '(1 2 3))` => `(nil ((t) (exit 1)) nil)`.
///
/// Three separate facts in one line: the entry debugger fired with `Qt`
/// (`src/eval.c:2602`, `eval_sub`), the *same* frame fired again on the way out
/// (`src/eval.c:339` set `debug_on_exit`, `src/eval.c:2777` spends it), and the
/// flag reads `nil` afterwards because `do_debug_on_call` cleared it before
/// either of those happened (`src/eval.c:338`).  The result is `nil` and not
/// `1` because the exit debugger's return value REPLACES the call's value.
#[test]
fn arming_debug_on_next_call_enters_the_debugger_and_disarms_it() {
    let mut eval = recorder_context();
    assert_eq!(
        case(
            &mut eval,
            "(setq debug-on-next-call t)
             (car '(1 2 3))"
        ),
        "(nil ((t) (exit 1)) nil)"
    );
}

/// GNU: `(setq debug-on-next-call t) (car '(1 2)) (cdr '(3 4))`
/// => `((4) ((t) (exit 1)) nil)`.
///
/// One arm, one entry: the second call is not debugged.  This is the property
/// `do_debug_on_call`'s first line exists to provide.
#[test]
fn the_arm_is_one_shot() {
    let mut eval = recorder_context();
    assert_eq!(
        case(
            &mut eval,
            "(setq debug-on-next-call t)
             (car '(1 2))
             (cdr '(3 4))"
        ),
        "((4) ((t) (exit 1)) nil)"
    );
}

/// Ledger 135's and 168's probe, whole:
/// `(list (default-value 'debug-on-next-call)
///        (progn (set-default 'debug-on-next-call 5) (default-value 'debug-on-next-call))
///        (progn (setq debug-on-next-call t) debug-on-next-call))`
/// => `(nil nil t)` in GNU, `(nil t t)` here before this entry.
///
/// The middle `nil` is the mechanism: `set-default` coerces 5 to `t` through
/// `Lisp_Fwd_Bool` (`src/data.c:1485-1487`) and arms; the very next cons form
/// disarms before the read.  The third element is `t` in both editors because
/// `progn` and `setq` are special forms and a plain symbol never reaches
/// `eval_sub`'s cons arm (`src/eval.c:2560-2576`), so nothing intervenes.
#[test]
fn ledger_135_probe_answers_nil_in_the_middle() {
    let mut eval = recorder_context();
    let value = eval
        .eval_str(
            r#"(list (default-value 'debug-on-next-call)
                     (progn (set-default 'debug-on-next-call 5)
                            (default-value 'debug-on-next-call))
                     (progn (setq debug-on-next-call t) debug-on-next-call))"#,
        )
        .expect("probe should evaluate");
    assert_eq!(print_value(&value), "(nil nil t)");
}

/// GNU: `(setq debug-on-next-call t) (if t 'yes 'no)` => `(nil ((t) (exit yes)) nil)`.
///
/// `eval_sub` records its backtrace frame and tests the arm at `src/eval.c:2598-2602`,
/// which is *before* the `SUBRP`/`UNEVALLED` dispatch at `2621-2632`.  A special
/// form is therefore armed exactly like a function call.
#[test]
fn special_form_frames_are_armed_too() {
    let mut eval = recorder_context();
    assert_eq!(
        case(
            &mut eval,
            "(setq debug-on-next-call t)
             (if t 'yes 'no)"
        ),
        "(nil ((t) (exit yes)) nil)"
    );
}

/// GNU: `(setq debug-on-next-call t) (apply #'car '((5 6)))` => `(nil ((t) (exit 5)) nil)`.
///
/// `Fapply` has no arm check of its own; it reaches `Ffuncall`
/// (`src/eval.c:3192`).  Here the outer `apply` form is armed by `eval_sub`
/// first, so `Qt` is the code -- exactly as GNU answers.
#[test]
fn apply_is_armed_through_the_call_that_reaches_it() {
    let mut eval = recorder_context();
    assert_eq!(
        case(
            &mut eval,
            "(setq debug-on-next-call t)
             (apply #'car '((5 6)))"
        ),
        "(nil ((t) (exit 5)) nil)"
    );
}

/// GNU: arming, then signalling out of the armed call
/// => `(after ((t)) nil)`: the ENTRY debugger ran, the EXIT debugger did not.
///
/// `unbind_to` pops `SPECPDL_BACKTRACE` with a bare `break`
/// (`src/eval.c:3818-3820`), so a non-local exit never spends `debug_on_exit`.
/// The flag is still down afterwards, because the disarm happened on entry.
#[test]
fn a_signal_out_of_an_armed_call_runs_only_the_entry_debugger() {
    let mut eval = recorder_context();
    assert_eq!(
        case(
            &mut eval,
            "(condition-case nil
                 (progn (setq debug-on-next-call t) (error \"boom\"))
               (error nil))
             'after"
        ),
        "(after ((t)) nil)"
    );
}

/// GNU: `(let ((inhibit-debugger t)) (setq debug-on-next-call t) (car '(1)))`
/// => `(nil ((t) (exit 1)) nil)`.
///
/// `call_debugger` *binds* `inhibit-debugger` to `t` (`src/eval.c:309`) but
/// never tests it; only the signal path consults it (`src/eval.c` `maybe_call_debugger`).
/// So `inhibit-debugger` does not gate `debug-on-next-call`.
#[test]
fn inhibit_debugger_does_not_gate_the_entry_debugger() {
    let mut eval = recorder_context();
    assert_eq!(
        case(
            &mut eval,
            "(let ((inhibit-debugger t))
               (setq debug-on-next-call t)
               (car '(1)))"
        ),
        "(nil ((t) (exit 1)) nil)"
    );
}

/// GNU: a debugger returning `'REPLACED` makes the debugged call return
/// `REPLACED` => `(REPLACED ((t) (exit 1)) nil)`.
///
/// `val = call_debugger (list2 (Qexit, val))` (`src/eval.c:2778`) is an
/// assignment, so this is not incidental -- it is how `debug.el`'s
/// "return a value from this frame" works.
#[test]
fn the_debuggers_return_value_replaces_the_calls_value() {
    let mut eval = recorder_context();
    assert_eq!(
        case(
            &mut eval,
            "(let ((debugger (lambda (&rest args)
                               (setq l172-log (cons args l172-log))
                               'REPLACED)))
               (setq debug-on-next-call t)
               (car '(1 2 3)))"
        ),
        "(REPLACED ((t) (exit 1)) nil)"
    );
}

/// GNU: `(defun f (x) (backtrace-debug 1 t) (* x 3))` then `(f 7)`
/// => `(nil ((exit 21)) nil)`.
///
/// Level 1 is the caller of `backtrace-debug`, i.e. `f`'s own frame
/// (`Fbacktrace_debug`, `src/eval.c:4016-4029`).  `f` computes 21 and the exit
/// debugger replaces it with the recorder's `nil`.  No `debug-on-next-call` is
/// involved: this is the other half of the same mechanism.
#[test]
fn backtrace_debug_flags_the_named_frame_for_exit() {
    let mut eval = recorder_context();
    eval.eval_str("(defalias 'l172-exit-caller (lambda (x) (backtrace-debug 1 t) (* x 3)))")
        .expect("defun should evaluate");
    assert_eq!(
        case(&mut eval, "(l172-exit-caller 7)"),
        "(nil ((exit 21)) nil)"
    );
}

/// GNU: `(defun f (x) (backtrace-debug 0 t) (* x 3))` then `(f 7)`
/// => `(21 ((exit t)) nil)`.
///
/// Level 0 is `backtrace-debug`'s *own* frame -- `get_backtrace_starting_at
/// (Qnil)` is `backtrace_top ()` (`src/eval.c:3988`), which is the running
/// subr.  So the flagged frame exits immediately with `backtrace-debug`'s own
/// return value `t`, and `f` is untouched and answers 21.
#[test]
fn backtrace_debug_level_zero_flags_its_own_frame() {
    let mut eval = recorder_context();
    eval.eval_str("(defalias 'l172-exit-self (lambda (x) (backtrace-debug 0 t) (* x 3)))")
        .expect("defun should evaluate");
    assert_eq!(case(&mut eval, "(l172-exit-self 7)"), "(21 ((exit t)) nil)");
}

/// GNU: a flagged frame reports `(:debug-on-exit t)` in the walker's fourth
/// slot => `(nil ((exit (l172-flags (:debug-on-exit t)))) nil)`.
///
/// `backtrace_frame_apply` builds that list from the same bit
/// (`src/eval.c:4003-4005`), so the flag `backtrace-debug` sets and the flag
/// the walker reports must be one bit, not two.
#[test]
fn debug_on_exit_is_visible_to_backtrace_frames() {
    let mut eval = recorder_context();
    eval.eval_str(
        "(defalias 'l172-flags
           (lambda ()
             (backtrace-debug 1 t)
             (let (out)
               (backtrace-frame--internal
                (lambda (_evald func _args flags) (setq out (list func flags)) nil)
                0 'l172-flags)
               out)))",
    )
    .expect("defun should evaluate");
    assert_eq!(
        case(&mut eval, "(l172-flags)"),
        "(nil ((exit (l172-flags (:debug-on-exit t)))) nil)"
    );
}

/// GNU: `(catch 'x (f))` where `f` flags its caller and throws
/// => `(thrown nil nil)` -- the debugger is not called at all.
///
/// This is the one row of the whole probe table that already matched before
/// this entry, and it matched for the wrong reason (nothing was flagged).  It
/// has to keep matching now that flagging works.
#[test]
fn a_throw_out_of_a_flagged_frame_does_not_enter_the_debugger() {
    let mut eval = recorder_context();
    eval.eval_str("(defalias 'l172-throw (lambda () (backtrace-debug 1 t) (throw 'l172 'thrown)))")
        .expect("defun should evaluate");
    assert_eq!(
        case(&mut eval, "(catch 'l172 (l172-throw))"),
        "(thrown nil nil)"
    );
}

/// `backtrace-debug` with a nil FLAG clears the bit again, so the frame exits
/// silently.  GNU: `set_backtrace_debug_on_exit (pdl, !NILP (flag))`
/// (`src/eval.c:4026`) -- the setter is not "arm", it is "assign".
#[test]
fn backtrace_debug_with_a_nil_flag_clears_the_bit() {
    let mut eval = recorder_context();
    eval.eval_str(
        "(defalias 'l172-clear
           (lambda (x) (backtrace-debug 1 t) (backtrace-debug 1 nil) (* x 3)))",
    )
    .expect("defun should evaluate");
    assert_eq!(case(&mut eval, "(l172-clear 7)"), "(21 nil nil)");
}

/// A two-argument call is the one frame shape the specpdl stores without a
/// `debug_on_exit` field at all (`SpecBinding::Backtrace2`), so flagging it
/// has to promote the frame.  GNU has no such split -- every `bt` has the bit
/// -- which makes this a port-specific way to lose a debugger entry.
///
/// GNU: `(setq debug-on-next-call t) (cons 1 2)` => `(nil ((t) (exit (1 . 2))) nil)`.
#[test]
fn a_two_argument_frame_is_promoted_rather_than_losing_the_flag() {
    let mut eval = recorder_context();
    assert_eq!(
        case(
            &mut eval,
            "(setq debug-on-next-call t)
             (cons 1 2)"
        ),
        "(nil ((t) (exit (1 . 2))) nil)"
    );
}

/// GNU `call_debugger` records the input-event count it was entered at
/// (`src/eval.c:299`) in the `DEFVAR_INT` `internal-when-entered-debugger`
/// (`src/eval.c:4553-4554`).  Measured under `emacs -Q --batch`: `-1` at
/// startup (`init_eval`, `src/eval.c:251`) and `0` after one entry, because
/// batch never reads a non-macro input event.
///
/// Nothing here reads it back yet -- `maybe_call_debugger`'s
/// `when_entered_debugger < num_nonmacro_input_events` guard
/// (`src/eval.c:2212`) is a separate, behaviour-changing gap recorded in the
/// ledger -- but the slot belongs to `call_debugger`, so it is written where
/// GNU writes it.
#[test]
fn entering_the_debugger_stamps_internal_when_entered_debugger() {
    let mut eval = recorder_context();
    let before = eval
        .eval_str("internal-when-entered-debugger")
        .expect("startup value should read");
    assert_eq!(print_value(&before), "-1");
    let after = eval
        .eval_str(
            "(progn (setq debug-on-next-call t)
                    (car '(1))
                    (prog1 internal-when-entered-debugger
                      (setq debug-on-next-call nil)))",
        )
        .expect("probe should evaluate");
    assert_eq!(print_value(&after), "0");
}

// ---------------------------------------------------------------------------
// Ledger 183: the read-back half of ledger 172's stamp.
//
// `call_debugger` writes `when_entered_debugger = num_nonmacro_input_events`
// (`src/eval.c:299`); `maybe_call_debugger` refuses a second entry unless a
// *new* non-macro input event has arrived since (`src/eval.c:2210-2212`).  In
// batch both numbers are 0 after the first entry, so the guard shuts for the
// rest of the session -- which is exactly the property these pins record.
// ---------------------------------------------------------------------------

/// Count debugger entries around N signalled-and-handled errors.
///
/// `debug-on-signal` is the only way to reach `maybe_call_debugger` from under
/// a `condition-case` in batch (`src/eval.c:1699-1703`), and it is what makes
/// the guard the *only* thing that can stop the second entry.
fn signal_debugger_context() -> Context {
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (defvar l183-n 0)
             (setq debugger (lambda (&rest _args) (setq l183-n (1+ l183-n)) nil))
             (setq debug-on-error t)
             (setq debug-on-signal t)
             nil)"#,
    )
    .expect("signal-debugger setup should evaluate");
    eval
}

fn probe(eval: &mut Context, body: &str) -> String {
    let value = eval
        .eval_str(body)
        .unwrap_or_else(|err| panic!("probe should evaluate: {err:?}\n{body}"));
    print_value(&value)
}

/// GNU, `emacs -Q --batch`, `tmp/l183-p2.el`:
///
/// ```text
/// (start when -1 events 0)
/// (after1 n 1 when 0)
/// (after2 n 1 when 0)
/// (after3 n 1 when 0)
/// ```
///
/// Three handled errors, one debugger entry.  `when_entered_debugger` starts
/// at `-1` (`init_eval`, `src/eval.c:251`), the first entry stamps it to
/// `num_nonmacro_input_events` = 0, and `0 < 0` is false for every later
/// signal.  Before this pin the port entered three times.
#[test]
fn the_signal_debugger_fires_once_while_no_new_input_arrives() {
    let mut eval = signal_debugger_context();
    assert_eq!(
        probe(
            &mut eval,
            "(list internal-when-entered-debugger num-nonmacro-input-events)"
        ),
        "(-1 0)"
    );
    assert_eq!(
        probe(
            &mut eval,
            "(progn (condition-case nil (error \"one\") (error nil))
                    (list l183-n internal-when-entered-debugger))"
        ),
        "(1 0)"
    );
    assert_eq!(
        probe(
            &mut eval,
            "(progn (condition-case nil (error \"two\") (error nil))
                    (condition-case nil (error \"three\") (error nil))
                    (list l183-n internal-when-entered-debugger))"
        ),
        "(1 0)"
    );
}

/// GNU, `tmp/l183-p4.el`, row `B-after-rewind-stamp`: `(n 2 when 0)`.
///
/// `internal-when-entered-debugger` is a `DEFVAR_INT` (`src/eval.c:4554`) and
/// its doc string says so out loud -- "Don't set this unless you're sure that
/// can't happen".  Rewinding it from Lisp re-opens the guard for exactly one
/// more entry, and that entry stamps it shut again.
#[test]
fn rewinding_the_stamp_from_lisp_reopens_the_signal_debugger_once() {
    let mut eval = signal_debugger_context();
    assert_eq!(
        probe(
            &mut eval,
            "(progn (condition-case nil (error \"one\") (error nil)) (list l183-n))"
        ),
        "(1)"
    );
    assert_eq!(
        probe(
            &mut eval,
            "(progn (setq internal-when-entered-debugger -1)
                    (condition-case nil (error \"two\") (error nil))
                    (list l183-n internal-when-entered-debugger))"
        ),
        "(2 0)"
    );
    assert_eq!(
        probe(
            &mut eval,
            "(progn (condition-case nil (error \"three\") (error nil)) (list l183-n))"
        ),
        "(2)"
    );
}

/// GNU, `tmp/l183-p4.el`, row `C-after-bump-events`:
/// `(n 3 when 5 events 5)`.
///
/// This is the storage-identity pin.  `num-nonmacro-input-events` is
/// `DEFVAR_INT ("num-nonmacro-input-events", num_nonmacro_input_events, ...)`
/// (`src/keyboard.c:13903`) -- the Lisp name and the counter GNU increments in
/// `record_char` (`src/keyboard.c:3576`) are the *same slot*.  So writing the
/// Lisp variable both re-opens the guard and changes what the next entry
/// stamps.  Before this pin the port had two storages: a `u64` field on
/// `CommandLoop` that counted, and a `DEFVAR_INT` initialized to 0 that
/// nothing ever wrote -- so `when` came back 0 here instead of 5.
#[test]
fn num_nonmacro_input_events_is_the_slot_the_stamp_reads() {
    let mut eval = signal_debugger_context();
    probe(
        &mut eval,
        "(condition-case nil (error \"one\") (error nil))",
    );
    assert_eq!(
        probe(
            &mut eval,
            "(progn (setq num-nonmacro-input-events 5)
                    (condition-case nil (error \"two\") (error nil))
                    (list l183-n internal-when-entered-debugger
                          num-nonmacro-input-events))"
        ),
        "(2 5 5)"
    );
    // 5 < 5 is false: shut again, without any further write.
    assert_eq!(
        probe(
            &mut eval,
            "(progn (condition-case nil (error \"three\") (error nil)) (list l183-n))"
        ),
        "(2)"
    );
}

/// GNU, `tmp/l183-p5.el`:
///
/// ```text
/// (after-signal (error) when 0)
/// (entry-debugger-with-guard-shut (lambda exit lambda exit) when 0)
/// (signal-with-guard-shut nil)
/// ```
///
/// The `code` is `lambda` there and `t` here, and neither is a divergence:
/// `debug-on-next-call` is a one-shot, so in a loaded FILE the arm is spent by
/// whatever `readevalloop` funcalls between two top-level forms
/// (`Ffuncall`, `src/eval.c:3190`, code `Qlambda`), while a single evaluated
/// form reaches the `car` through `eval_sub` (`src/eval.c:2602`, code `Qt`).
/// Re-measured as ONE form (`tmp/l183-p16.el`), which is what `eval_str` does,
/// GNU answers `(((error) 0) ((t exit t exit) 0) nil)` and the merge-base
/// binary answered `(((error) 0) ((t exit t exit) 0) (error))` -- the same
/// single divergent row.
///
/// The guard is a conjunct of `maybe_call_debugger` (`src/eval.c:2212`) and of
/// nothing else: `do_debug_on_call` (`src/eval.c:335-341`) and the six
/// `debug_on_exit` sites call `call_debugger` unconditionally.  So a shut
/// guard silences the *signal* debugger and leaves `debug-on-next-call`
/// working, twice over.  Both editors agree on rows 1 and 2; only row 3
/// diverged.
#[test]
fn the_reentry_guard_gates_the_signal_debugger_only() {
    let mut eval = signal_debugger_context();
    eval.eval_str(
        "(setq debugger (lambda (&rest args) (setq l183-log (cons (car args) l183-log)) nil))",
    )
    .expect("logging debugger should install");
    eval.eval_str("(defvar l183-log nil)")
        .expect("log should define");
    assert_eq!(
        probe(
            &mut eval,
            "(progn (setq l183-log nil)
                    (condition-case nil (error \"one\") (error nil))
                    (list (reverse l183-log) internal-when-entered-debugger))"
        ),
        "((error) 0)"
    );
    assert_eq!(
        probe(
            &mut eval,
            "(progn (setq l183-log nil)
                    (setq debug-on-next-call t)
                    (car '(1 2))
                    (setq debug-on-next-call t)
                    (car '(3 4))
                    (list (reverse l183-log) internal-when-entered-debugger))"
        ),
        "((t exit t exit) 0)"
    );
    assert_eq!(
        probe(
            &mut eval,
            "(progn (setq l183-log nil)
                    (condition-case nil (error \"two\") (error nil))
                    (reverse l183-log))"
        ),
        "nil"
    );
}

/// GNU `call_debugger` installs four bindings (`src/eval.c:306-314`); this
/// port installed two.  Measured under GNU Emacs 31.0.90 `-Q --batch`
/// (`tmp/l183-p9.el`), entering the debugger from inside
/// `(let ((inhibit-redisplay t) (inhibit-changing-match-data t)) ...)`:
///
/// ```text
///                              GNU    this port, before
/// inhibit-redisplay            nil    t
/// inhibit-changing-match-data  nil    t
/// inhibit-debugger             t      t
/// debugger-may-continue        t      t
/// ```
///
/// Both missing bindings have a stated purpose in GNU's own comments: the
/// debugger has to be able to draw when its caller had display switched off,
/// and it has to be able to use match data when its caller was inside
/// `string-match-p`.  The knock-on of the second is the last row of the probe
/// -- `(progn (string-match "b" "abc") (match-beginning 0))` answers `1` in
/// GNU and answered a stale `102` here.
#[test]
fn call_debugger_binds_gnus_four_variables() {
    let mut eval = Context::new();
    eval.eval_str(
        r#"(progn
             (defvar l183-seen nil)
             (setq debugger
                   (lambda (&rest _args)
                     (setq l183-seen
                           (list inhibit-redisplay
                                 inhibit-changing-match-data
                                 inhibit-debugger
                                 debugger-may-continue
                                 (progn (string-match "b" "abc")
                                        (match-beginning 0))))
                     nil))
             nil)"#,
    )
    .expect("recorder setup should evaluate");

    let entry = eval
        .eval_str(
            "(progn (let ((inhibit-redisplay t) (inhibit-changing-match-data t))
                      (setq debug-on-next-call t)
                      (car '(1 2)))
                    (setq debug-on-next-call nil)
                    l183-seen)",
        )
        .expect("entry-debugger probe should evaluate");
    assert_eq!(print_value(&entry), "(nil nil t t 1)");

    // The bindings are unwound again: they belong to the debugger call only.
    let after = eval
        .eval_str("(list inhibit-redisplay inhibit-changing-match-data inhibit-debugger)")
        .expect("post-unwind probe should evaluate");
    assert_eq!(print_value(&after), "(nil nil nil)");
}

/// GNU's `Breturn` spends `debug_on_exit` before its `specpdl_ptr--`
/// (`src/bytecode.c:825-828`), and `backtrace-debug` can raise that flag on
/// ANY live frame by index (`src/eval.c:2830-2846`) -- including a
/// byte-compiled caller that this port's fast bytecode return had already been
/// routed to.  Measured under GNU Emacs 31.0.90 `-Q --batch`
/// (`tmp/l183-p10.el`):
///
/// ```text
///                 GNU              this port, before
/// byte-compiled   log=(exit)       log=nil
/// interpreted     log=(exit)       log=(exit)
/// ```
///
/// Ledger 172 §7 recorded the fast pops as "safe by reachability": no flagged
/// frame could reach them.  This is the path that does, which is why the pop
/// now refuses rather than relying on the argument.
#[test]
fn a_byte_compiled_frame_flagged_by_backtrace_debug_still_calls_the_exit_debugger() {
    // `byte-compile` and `defun` are Lisp, so this is the one pin in the file
    // that needs the full bootstrap surface rather than a bare `Context`.
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        r#"(progn
             (defvar l172-log nil)
             (setq debugger (lambda (&rest args) (setq l172-log (cons args l172-log)) nil))
             nil)"#,
    )
    .expect("recorder setup should evaluate");
    eval.eval_str(
        r#"(progn
             (defun l183-inner (n) (backtrace-debug n t) 'inner-done)
             (defun l183-outer () (l183-inner 1) 'outer-done)
             (byte-compile 'l183-inner)
             (byte-compile 'l183-outer)
             (list (byte-code-function-p (symbol-function 'l183-inner))
                   (byte-code-function-p (symbol-function 'l183-outer))))"#,
    )
    .map(|value| assert_eq!(print_value(&value), "(t t)", "both must byte-compile"))
    .expect("byte-compilation setup should evaluate");

    assert_eq!(
        case(&mut eval, "(l183-outer)"),
        "(outer-done ((exit inner-done)) nil)"
    );

    // The interpreted twin, which agreed with GNU before and must still.
    eval.eval_str(
        "(progn (defun l183-inner-i (n) (backtrace-debug n t) 'inner-done)
                (defun l183-outer-i () (l183-inner-i 1) 'outer-done))",
    )
    .expect("interpreted twin should define");
    assert_eq!(
        case(&mut eval, "(l183-outer-i)"),
        "(outer-done ((exit inner-done)) nil)"
    );
}

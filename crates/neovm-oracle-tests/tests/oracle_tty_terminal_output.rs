//! What GNU Emacs 31.1 writes to a text terminal besides the cells, observed
//! on the byte stream of a test-owned PTY.
//!
//! Refresh expectations only from GNU with NEOVM_ORACLE_MODE=refresh
//! UPDATE_EXPECT=1, running the test driver in the required PID sandbox.
//! The forms open a 120x40 terminal frame on the PTY, as the ttykey
//! differential scenes do; the terminal's byte stream is a redisplay
//! observation like any other (src/term.c decides it), so it is part of the
//! expectation and never written by hand.

#![cfg(unix)]

use crate::common;

#[path = "../src/tty_test_terminal.rs"]
mod tty_test_terminal;

use common::return_if_neovm_enable_oracle_proptest_not_set;
use tty_test_terminal::TestTerminal;

/// A terminal frame on the test PTY named by `TERMINAL_OUTPUT_TEST_TTY`.
const TERMINAL_SUPPORT: &str = r##"
  (setq redisplay-skip-initial-frame nil inhibit-redisplay nil)
  (setq to-frame
    (make-terminal-frame
      (list (cons 'tty (getenv "TERMINAL_OUTPUT_TEST_TTY"))
            '(tty-type . "screen-256color"))))
  (select-frame to-frame)
  (switch-to-buffer (get-buffer-create "to-cursor"))
  (fundamental-mode)
  (dotimes (i 60) (insert (format "line %02d\n" i)))
"##;

/// GNU's term.c has no DECSCUSR (`CSI Ps SP q`): on a text terminal
/// `cursor-type' is a window-system property, and the cursor is whatever the
/// terminal is configured to show, through every redisplay and cursor motion
/// of a session.  The ttykey differential scenes recorded GNU's cursor as the
/// terminal default at all 115 checkpoints while this port wrote a steady
/// block (`CSI 2 SP q`) on every frame.
#[test]
fn tty_default_box_cursor_writes_no_cursor_style_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = format!(
        r##"(progn {TERMINAL_SUPPORT}
  (goto-char (point-min))
  (redisplay t)
  (forward-line 5)
  (redisplay t)
  (goto-char (point-max))
  (redisplay t)
  (forward-line -3)
  (end-of-line)
  (redisplay t)
  (list (window-start) (posn-x-y (posn-at-point))))"##
    );
    common::assert_oracle_parity_with_terminal_output_expect(
        &form,
        || {
            let terminal = TestTerminal::with_size(40, 120);
            let env = vec![("TERMINAL_OUTPUT_TEST_TTY".to_owned(), terminal.name.clone())];
            (terminal, env)
        },
        |terminal| {
            format!(
                "cursor-style {:?}",
                TestTerminal::cursor_style_controls(&terminal.finish_with_output())
            )
        },
        expect_test::expect![[r#""OK (329 (7 . 16))" cursor-style []"#]],
    );
}

/// GNU evaluates a `(when FORM . SPEC)` display spec at every site it
/// meets, with `object`, `position` and `buffer-position` bound there
/// (src/xdisp.c:6152-6154): one spec object on two spans whose FORM reads
/// `buffer-position` is evaluated at each span and replaces only the span
/// whose position satisfies it.  The observation is the set of bindings
/// the FORM saw and what the terminal was drawn: `line 0` stays (position 1
/// is not past 30) and `ORIGINAL` becomes `REPLACED` (position 36 is).
#[test]
fn tty_one_when_spec_on_two_spans_is_evaluated_at_each_with_its_own_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = format!(
        r##"(progn {TERMINAL_SUPPORT}
  (defvar to-seen nil)
  (defvar object) (defvar position) (defvar buffer-position)
  (defun to-late-p ()
    (push (list (bufferp object) position buffer-position) to-seen)
    (> buffer-position 30))
  (setq to-spec '(when (to-late-p) . "REPLACED"))
  (erase-buffer)
  (dotimes (i 5) (insert (format "line %d\n" i)))
  (insert "ORIGINAL text here\n")
  (dotimes (i 5) (insert (format "line %d\n" (+ 6 i))))
  (put-text-property 1 7 'display to-spec)
  (put-text-property 36 44 'display to-spec)
  (goto-char (point-min))
  (redisplay t)
  (list (sort (delete-dups (copy-sequence to-seen))
              (lambda (a b) (< (nth 2 a) (nth 2 b))))
        (posn-x-y (posn-at-point))))"##
    );
    common::assert_oracle_parity_with_terminal_output_expect(
        &form,
        || {
            let terminal = TestTerminal::with_size(40, 120);
            let env = vec![("TERMINAL_OUTPUT_TEST_TTY".to_owned(), terminal.name.clone())];
            (terminal, env)
        },
        |terminal| {
            format!(
                "drawn {:?}",
                TestTerminal::drawn_tokens(
                    &terminal.finish_with_output(),
                    &["line 0", "ORIGINAL", "REPLACED"],
                )
            )
        },
        expect_test::expect![[
            r#""OK (((t 1 1) (t 36 36)) (0 . 0))" drawn ["line 0", "REPLACED"]"#
        ]],
    );
}

/// The `when` probe buffer: five `line N` rows, then `ORIGINAL text here`
/// whose first eight characters carry `(when (to-form) . "REPLACED")`,
/// then five more rows.  `to-form` counts its evaluations in `to-n` and
/// answers `to-flag`.
const WHEN_SUPPORT: &str = r##"
  (defvar to-n 0)
  (defvar to-flag t)
  (defun to-form () (setq to-n (1+ to-n)) to-flag)
  (erase-buffer)
  (dotimes (i 5) (insert (format "line %d\n" i)))
  (insert "ORIGINAL text here\n")
  (dotimes (i 5) (insert (format "line %d\n" (+ 6 i))))
  (put-text-property 36 44 'display '(when (to-form) . "REPLACED"))
  (goto-char (point-min))
"##;

fn when_terminal() -> (TestTerminal, Vec<(String, String)>) {
    let terminal = TestTerminal::with_size(40, 120);
    let env = vec![("TERMINAL_OUTPUT_TEST_TTY".to_owned(), terminal.name.clone())];
    (terminal, env)
}

/// Declared (D2): GNU evaluates a `when` FORM at every iterator pass that
/// meets the property during one redisplay -- `redisplay_window`'s start
/// and cursor lookaheads, the bidi prescan, `display_line` -- while the port
/// evaluates each site once per walk.  The screen is the same; the count is
/// GNU's pass structure.  Pinned so a change of either side is noticed.
#[test]
fn tty_when_form_evaluations_per_redisplay_are_gnu_pass_count_declared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = format!(
        r##"(progn {TERMINAL_SUPPORT} {WHEN_SUPPORT}
  (setq to-n 0)
  (redisplay t)
  to-n)"##
    );
    common::assert_oracle_divergence_with_terminal_output_expect(
        &form,
        when_terminal,
        |terminal| {
            format!(
                "drawn {:?}",
                TestTerminal::drawn_tokens(
                    &terminal.finish_with_output(),
                    &["ORIGINAL", "REPLACED"],
                )
            )
        },
        expect_test::expect![[r#""OK 6" drawn ["REPLACED"]"#]],
    );
}

/// Declared (D3): the Lisp primitives a motion command runs the display
/// iterator through (`posn-at-point`, `vertical-motion`; `line-move-visual`
/// calls both on every C-n of an interactive session) evaluate the FORM in
/// each of their GNU passes; the port answers them from a walk whose span
/// scan evaluates each site once.  (`next-line` itself, in --batch, takes
/// GNU's non-visual path and evaluates nothing, so the primitives are
/// called directly.)
#[test]
fn tty_when_form_evaluations_in_display_queries_are_gnu_pass_count_declared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = format!(
        r##"(progn {TERMINAL_SUPPORT} {WHEN_SUPPORT}
  (forward-line 4)
  (redisplay t)
  (setq to-n 0)
  (list (progn (posn-at-point 37) to-n)
        (progn (vertical-motion 1) to-n)
        (point)))"##
    );
    common::assert_oracle_divergence_with_terminal_output_expect(
        &form,
        when_terminal,
        |terminal| {
            let _ = terminal.finish_with_output();
            String::new()
        },
        expect_test::expect![[r#""OK (15 15 36)" "#]],
    );
}

/// GNU reuses a window's rows across a cursor motion (`try_cursor_movement`)
/// and never evaluates a reused row's `when` FORM again: after the FORM's
/// answer turns nil with no buffer change, a redisplay that only moves the
/// cursor leaves `REPLACED` on the screen and evaluates nothing.  The
/// observation is the evaluations and what is drawn after the marker the
/// form writes between the two redisplays.
#[test]
fn tty_cursor_motion_reuses_rows_without_evaluating_their_when_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = format!(
        r##"(progn {TERMINAL_SUPPORT} {WHEN_SUPPORT}
  (redisplay t)
  (setq to-flag nil to-n 0)
  (send-string-to-terminal "@@TO-MARK@@" (frame-terminal to-frame))
  (forward-line 2)
  (redisplay t)
  (list to-n (point)))"##
    );
    common::assert_oracle_parity_with_terminal_output_expect(
        &form,
        when_terminal,
        |terminal| {
            format!(
                "drawn-after-mark {:?}",
                TestTerminal::drawn_tokens_after(
                    &terminal.finish_with_output(),
                    "@@TO-MARK@@",
                    &["ORIGINAL", "REPLACED"],
                )
            )
        },
        expect_test::expect![[r#""OK (0 15)" drawn-after-mark []"#]],
    );
}

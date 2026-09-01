//! Complex combo batch 445 — 15 final untouched-edge probes: completion-all,
//! pp-to-string circular, edebug-defun, benchmark-elapse, macroexpand deep,
//! cl-eval-when, display-warning deeper, save-selected-window, walk-windows,
//! window-tree with parameters, completion-hilit, pp-display-expression,
//! edebug-eval, eval-when-compile, with-propertized-buffer-substring deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// completion-all-completions / test-completion deeper.
#[test]
fn div_cx445_completion_all_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"forward-char\" \"forward-word\") t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (all-completions "for" '(forward-char forward-word))
      (test-completion "forward-char" '(forward-char forward-word))
      (test-completion "nope" '(forward-char forward-word)))"##,
        expect,
    );
}

/// pp-to-string with circular lists and print-circle.
#[test]
fn div_cx445_pp_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#1=(1 2 3 . #1#)\\n\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((l (list 1 2 3))
       (print-circle t))
  (setcdr (cddr l) l)
  (condition-case e (pp-to-string l) (error (car e))))"##,
        expect,
    );
}

/// edebug-defun: instrument a function for debugging.
#[test]
fn div_cx445_edebug_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'edebug)
  (defun neo-cx445-edebug (x) (* x 2))
  (list (fboundp 'edebug-defun)
        (fboundp 'edebug-eval-top-level-form)))"##,
        expect,
    );
}

/// benchmark-elapse / benchmark-call.
#[test]
fn div_cx445_benchmark() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (stringp (benchmark 1000 '(+ 1 2 3)))
      (numberp (benchmark-elapse (sit-for 0))))"##,
        expect,
    );
}

/// macroexpand on complex nested backquote.
#[test]
fn div_cx445_macroexpand_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (let* ((a (and t 1)) (b (and a 2))) (if b (+ a b)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(macroexpand '(when-let* ((a 1) (b 2)) (+ a b)))"##,
        expect,
    );
}

/// cl-eval-when: conditional evaluation at compile time.
#[test]
fn div_cx445_cl_eval_when() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(r##"(eval-when-compile (+ 1 2))"##, expect);
}

/// display-warning / warn deeper with different types.
#[test]
fn div_cx445_display_warning_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'warnings)
  (let ((warnings ()))
    (display-warning 'neo-cx445 "test warning")
    (display-warning 'neo-cx445 "emacs warning" :error)
    (list (warning-numeric-level :warning)
          (warning-numeric-level :error))))
"##,
        expect,
    );
}

/// save-selected-window / with-selected-window.
#[test]
fn div_cx445_save_selected_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #<window 1 on *scratch*>""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((w (selected-window)))
  (save-selected-window
    (select-window (minibuffer-window))
    (selected-window))
  (selected-window))"##,
        expect,
    );
}

/// walk-windows: iterating over window tree.
#[test]
fn div_cx445_walk_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((windows ()))
  (walk-windows (lambda (w) (push w windows)) 'all)
  (length windows))"##,
        expect,
    );
}

/// window-tree with frame parameter.
#[test]
fn div_cx445_window_tree_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((tree (window-tree (selected-frame))))
  (list (listp tree) (> (length tree) 0)))"##,
        expect,
    );
}

/// completion-hilit-commonality: highlighting completion.
#[test]
fn div_cx445_completion_hilit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 104)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(completion-hilit-commonality "hello" 3)"##,
        expect,
    );
}

/// pp-display-expression: pretty-print display.
#[test]
fn div_cx445_pp_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"(a (b c) d)\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (condition-case e
      (pp-display-expression '(a (b c) d) (current-buffer))
    (error (car e)))
  (buffer-string))"##,
        expect,
    );
}

/// eval-when-compile with side effects.
#[test]
fn div_cx445_eval_when_compile_side() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(eval-when-compile (setq neo-cx445-compiled 'yes))
(fboundp 'neo-cx445-compiled-fn)"##,
        expect,
    );
}

/// with-propertized-buffer-substring deep.
#[test]
fn div_cx445_with_propertized_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (void-function with-propertized-buffer-substring)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc")
  (put-text-property 1 2 'face 'bold)
  (put-text-property 2 3 'face 'italic)
  (let ((s (with-propertized-buffer-substring (point-min) (point-max))))
    (list (length s) (text-properties-at 0 s) (text-properties-at 1 s))))"##,
        expect,
    );
}

/// number-sequence with various start/step/end.
#[test]
fn div_cx445_number_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5) (1 3 5 7 9) (10 8 6 4 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (number-sequence 1 5)
      (number-sequence 1 10 2)
      (number-sequence 10 1 -2))"##,
        expect,
    );
}

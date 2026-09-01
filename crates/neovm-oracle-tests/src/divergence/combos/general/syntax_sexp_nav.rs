//! Divergence tests: syntax + parsing + sexp navigation combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_parse_partial_sexp_multi_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 0 nil nil) (15 2 nil nil) (20 1 nil nil) (30 3 nil nil) (45 3 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(defun foo (x y)\n  (list (+ x 1)\n        (* y 2)))")
  (let ((states nil))
    (dolist (pos '(1 15 20 30 45))
      (goto-char pos)
      (let ((ppss (syntax-ppss pos)))
        (push (list pos (nth 0 ppss) (nth 3 ppss) (nth 4 ppss)) states)))
    (nreverse states))) "#,
        expect,
    );
}

#[test]
fn divergence_scan_lists_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (14 14 nil \"(a (b (c)) d)\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(a (b (c)) d) (e f) (g)")
  (goto-char 1)
  (let ((fwd1 (scan-lists 1 1 0))
        (fwd2 (scan-lists (point) 1 0))
        (back (scan-lists (point) -1 0)))
    (list fwd1 fwd2 back
          (buffer-substring 1 fwd1)
          (buffer-substring fwd1 fwd2)))) "#,
        expect,
    );
}

#[test]
fn divergence_forward_kill_sexp_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 7 15 \"(aaa  ccc ddd eee)\" \"bbb\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(aaa bbb ccc ddd eee)")
  (let ((m1 (set-marker (make-marker) 2))
        (m2 (set-marker (make-marker) 10))
        (m3 (set-marker (make-marker) 18)))
    (goto-char 6)
    (kill-sexp 1)
    (list (marker-position m1) (marker-position m2) (marker-position m3)
          (buffer-string)
          (current-kill 0)))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_ppss_comment_string_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (21 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo \"string here\" bar ; comment\n  baz)")
  (let ((states nil))
    (dotimes (i (point-max))
      (let ((ppss (syntax-ppss (1+ i))))
        (when (or (nth 3 ppss) (nth 4 ppss))
          (push (list (1+ i)
                      (if (nth 3 ppss) 'string (if (eq (nth 4 ppss) t) 'comment 'char))
                      (nth 0 ppss))
                states))))
    (list (length (nreverse states))
          (>= (length (nreverse states)) 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_narrowed_forward_sexp_across_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (21)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(aaa (bbb (ccc) ddd) eee)")
  (narrow-to-region 6 21)
  (goto-char (point-min))
  (let ((pos nil))
    (condition-case err
        (progn (forward-sexp 3) (push (point) pos))
      (scan-error (push (list 'scan-err (cdr err)) pos)))
    (widen)
    (nreverse pos))) "#,
        expect,
    );
}

#[test]
fn divergence_unbalanced_parens_scan_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument number-or-marker-p \"Unbalanced parentheses\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo (bar baz)")
  (goto-char 1)
  (condition-case err
      (progn (forward-sexp 1) 'no-error)
    (scan-error
     (list (car err)
           (nth 1 err)
           (nth 2 err)
           (<= (nth 1 err) (point-max)))))) "#,
        expect,
    );
}

#[test]
fn divergence_beginning_end_of_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (25 45 \"(defun b (x) body2)\\n\" t 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(defun a () \"doc\" body)\n(defun b (x) body2)\n(defun c () body3)")
  (goto-char 30)
  (let ((beg (progn (beginning-of-defun) (point)))
        (end (progn (end-of-defun) (point))))
    (list beg end
          (buffer-substring beg end)
          (<= beg end)
          (string-match "defun b" (buffer-substring beg end))))) "#,
        expect,
    );
}

#[test]
fn divergence_thing_at_point_sexp_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"(foo bar (baz quux))\" t (1 . 21) t \"hello-world\" t (22 . 33))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo bar (baz quux)) hello-world 42")
  (goto-char 1)
  (let ((sexp (thing-at-point 'sexp))
        (bounds (bounds-of-thing-at-point 'sexp)))
    (forward-sexp 1)
    (forward-char 1)
    (let ((word (thing-at-point 'symbol))
              (wbounds (bounds-of-thing-at-point 'symbol)))
      (list sexp (equal sexp "(foo bar (baz quux))")
            bounds
            (<= (car bounds) (cdr bounds))
            word (equal word "hello-world")
            wbounds)))) "#,
        expect,
    );
}

#[test]
fn divergence_insert_balanced_undo_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"(MIDDLE)\" 4 \"(MIDDLE)\" 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "MIDDLE")
  (let ((m (set-marker (make-marker) 3)))
    (undo-boundary)
    (goto-char 1)
    (insert "(")
    (goto-char (point-max))
    (insert ")")
    (let ((s1 (buffer-string))
          (p1 (marker-position m)))
      (undo-boundary)
      (primitive-undo 1 buffer-undo-list)
      (list s1 p1 (buffer-string) (marker-position m))))) "#,
        expect,
    );
}

#[test]
fn divergence_check_parens_via_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (unbalanced \"Unmatched bracket or quote\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(foo (bar))\n(baz quux)\n(unbalanced (open")
  (condition-case err
      (progn (check-parens) 'balanced)
    (error (list 'unbalanced (error-message-string err))))) "#,
        expect,
    );
}

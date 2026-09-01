//! Oracle parity tests for `forward-comment` with various syntax tables.
//!
//! Tests single-line comments, multi-line comments, COUNT parameter
//! (positive/negative/zero), end-of-buffer, nested comments, combined
//! with skip-syntax-forward, and complex non-comment extraction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// forward-comment over single-line comments (Emacs Lisp ;; style)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_single_line_elisp_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In emacs-lisp-mode syntax, ; starts a comment to end-of-line.
    // forward-comment(1) should skip over the comment and trailing newline.
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    ;; Set up ; as a single-line comment starter
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert ";; this is a comment\nreal code here")
    (goto-char (point-min))
    (let ((result (forward-comment 1)))
      (list result (point)
            (buffer-substring (point) (point-max))))))"#;
    let expect = expect_test::expect![[r#""OK (t 22 \"real code here\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment over multi-line comments (C-style /* */ )
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_multiline_c_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set up /* */ as two-char comment delimiters.
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?/ ". 14" st)
    (modify-syntax-entry ?* ". 23" st)
    (set-syntax-table st)
    (insert "/* multi\nline\ncomment */ after")
    (goto-char (point-min))
    (let ((result (forward-comment 1)))
      (list result (point)
            (buffer-substring (point) (point-max))))))"#;
    let expect = expect_test::expect![[r#""OK (t 25 \" after\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment with COUNT parameter (positive, negative, zero)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_count_variations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test COUNT=0 (no movement), COUNT=2 (skip 2 comments),
    // COUNT=-1 (backward), all in one form.
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert ";; first\n;; second\ncode")
    ;; COUNT=0: should return t and not move
    (goto-char (point-min))
    (let ((r0 (forward-comment 0))
          (p0 (point)))
      ;; COUNT=2: skip two comments
      (goto-char (point-min))
      (let ((r2 (forward-comment 2))
            (p2 (point)))
        ;; COUNT=-1: backward from end of second comment
        (goto-char p2)
        (let ((r-1 (forward-comment -1))
              (p-1 (point)))
          (list (list 'zero r0 p0)
                (list 'two r2 p2)
                (list 'neg-one r-1 p-1)))))))"#;
    let expect = expect_test::expect![[r#""OK ((zero t 1) (two t 20) (neg-one t 10))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment at end of buffer / no comment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_at_eob_and_no_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When point is at end of buffer, forward-comment returns nil.
    // When point is on non-comment text, forward-comment returns nil.
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert "no comments here")
    ;; At end of buffer
    (goto-char (point-max))
    (let ((r1 (forward-comment 1))
          (p1 (point)))
      ;; On non-comment, non-whitespace text
      (goto-char (point-min))
      (let ((r2 (forward-comment 1))
            (p2 (point)))
        ;; Backward at beginning of buffer
        (goto-char (point-min))
        (let ((r3 (forward-comment -1))
              (p3 (point)))
          (list (list 'eob r1 p1)
                (list 'non-comment r2 p2)
                (list 'bob-back r3 p3)))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((eob nil 17) (non-comment nil 1) (bob-back nil 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment skips whitespace AND comments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_whitespace_plus_comments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // forward-comment skips both whitespace and comments together.
    // Multiple whitespace + comment blocks should be consumed by COUNT=1.
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert "   ;; comment1\n   ;; comment2\n   code")
    (goto-char (point-min))
    ;; forward-comment with large count to skip all ws+comments
    (let ((r (forward-comment 100)))
      (list r (point)
            (buffer-substring (point) (point-max))))))"#;
    let expect = expect_test::expect![[r#""OK (nil 34 \"code\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment backward through multiple comments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_backward_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Place point after several comments and skip backward through them.
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert "code\n;; c1\n;; c2\n;; c3\n")
    (goto-char (point-max))
    (let ((r (forward-comment -3))
          (p (point)))
      ;; Also try skipping more than available
      (goto-char (point-max))
      (let ((r2 (forward-comment -100))
            (p2 (point)))
        (list (list 'back3 r p)
              (list 'back100 r2 p2))))))"#;
    let expect = expect_test::expect![[r#""OK ((back3 t 6) (back100 nil 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: extract non-comment code lines from mixed buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_extract_code_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk through a buffer with mixed code and comments,
    // collecting only the non-comment, non-empty lines.
    let form = r#"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert ";; header comment\n(setq x 1)\n;; middle comment\n(setq y 2)\n  ;; trailing\n(setq z 3)\n")
    (goto-char (point-min))
    (let ((code-lines nil))
      (while (< (point) (point-max))
        ;; Skip whitespace and comments
        (forward-comment (buffer-size))
        (when (< (point) (point-max))
          (let ((start (point)))
            (end-of-line)
            (let ((line (buffer-substring start (point))))
              (when (> (length line) 0)
                (setq code-lines (cons line code-lines))))
            (when (< (point) (point-max))
              (forward-char 1)))))
      (nreverse code-lines))))"#;
    let expect = expect_test::expect![[r#""OK (\"(setq x 1)\" \"(setq y 2)\" \"(setq z 3)\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

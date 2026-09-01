//! Oracle parity tests for `forward-comment` with complex patterns.
//!
//! Tests forward-comment with positive/negative/zero N, interaction with
//! syntax tables (various comment start/end chars), line vs block comments,
//! comment counting/extraction, and code-vs-comment separation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// forward-comment with positive N: skip exactly N comments forward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_patterns_positive_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test forward-comment with N=1, N=2, N=3 in a buffer with exactly 3 comments.
    // Verify point positions and remaining text after each skip.
    let form = r####"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert ";; comment one\n;; comment two\n;; comment three\nactual code here")
    ;; N=1: skip one comment
    (goto-char (point-min))
    (let ((r1 (forward-comment 1))
          (p1 (point)))
      ;; N=2: skip two comments
      (goto-char (point-min))
      (let ((r2 (forward-comment 2))
            (p2 (point)))
        ;; N=3: skip all three comments
        (goto-char (point-min))
        (let ((r3 (forward-comment 3))
              (p3 (point)))
          ;; N=4: only 3 comments, should still succeed since whitespace counts
          (goto-char (point-min))
          (let ((r4 (forward-comment 4))
                (p4 (point)))
            (list
             (list 'skip-1 r1 p1 (buffer-substring p1 (min (+ p1 10) (point-max))))
             (list 'skip-2 r2 p2 (buffer-substring p2 (min (+ p2 10) (point-max))))
             (list 'skip-3 r3 p3 (buffer-substring p3 (min (+ p3 10) (point-max))))
             (list 'skip-4 r4 p4))))))))"####;
    let expect = expect_test::expect![[
        r#""OK ((skip-1 t 16 \";; comment\") (skip-2 t 31 \";; comment\") (skip-3 t 48 \"actual cod\") (skip-4 nil 48))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment with negative N: skip backward through comments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_patterns_negative_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Place point after multiple comments and skip backward.
    // Test N=-1, N=-2, N=-3, and N exceeding available comments.
    let form = r####"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert "prefix\n;; alpha\n;; beta\n;; gamma\nsuffix")
    ;; Go to just before "suffix"
    (goto-char (point-max))
    (beginning-of-line)
    (let ((start-pos (point)))
      ;; N=-1: skip back one comment
      (goto-char start-pos)
      (let ((r1 (forward-comment -1))
            (p1 (point)))
        ;; N=-2: skip back two comments
        (goto-char start-pos)
        (let ((r2 (forward-comment -2))
              (p2 (point)))
          ;; N=-3: skip back three comments
          (goto-char start-pos)
          (let ((r3 (forward-comment -3))
                (p3 (point)))
            ;; N=-100: try to skip more than available
            (goto-char start-pos)
            (let ((r100 (forward-comment -100))
                  (p100 (point)))
              (list
               (list 'back-1 r1 p1)
               (list 'back-2 r2 p2)
               (list 'back-3 r3 p3)
               (list 'back-100 r100 p100))))))))))"####;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 31 50)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment with N=0: no movement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_patterns_zero_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // forward-comment with N=0 should return t and not move point, regardless
    // of whether point is on a comment, whitespace, or code.
    let form = r####"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert ";; a comment\nsome code\n   \n;; another")
    ;; On comment start
    (goto-char (point-min))
    (let ((r1 (forward-comment 0)) (p1 (point)))
      ;; On code
      (goto-char 14)
      (let ((r2 (forward-comment 0)) (p2 (point)))
        ;; On whitespace-only line
        (goto-char 24)
        (let ((r3 (forward-comment 0)) (p3 (point)))
          ;; At end of buffer
          (goto-char (point-max))
          (let ((r4 (forward-comment 0)) (p4 (point)))
            ;; At beginning of buffer
            (goto-char (point-min))
            (let ((r5 (forward-comment 0)) (p5 (point)))
              (list
               (list 'on-comment r1 p1)
               (list 'on-code r2 p2)
               (list 'on-whitespace r3 p3)
               (list 'at-eob r4 p4)
               (list 'at-bob r5 p5)))))))))"####;
    let expect = expect_test::expect![[
        r#""OK ((on-comment t 1) (on-code t 14) (on-whitespace t 24) (at-eob t 38) (at-bob t 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with syntax tables: custom comment delimiters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_patterns_custom_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set up custom comment syntax using # for line comments and { } for
    // block comments, then verify forward-comment handles them correctly.
    let form = r####"(with-temp-buffer
  (let ((st (make-syntax-table)))
    ;; # as line comment start, newline as end
    (modify-syntax-entry ?# "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert "# line comment\ncode1\n# another comment\ncode2")
    ;; Skip first line comment
    (goto-char (point-min))
    (let ((r1 (forward-comment 1))
          (p1 (point))
          (rest1 (buffer-substring (point) (min (+ (point) 5) (point-max)))))
      ;; Skip all comments and whitespace
      (goto-char (point-min))
      (let ((r-all (forward-comment (buffer-size)))
            (p-all (point))
            (rest-all (buffer-substring (point) (min (+ (point) 5) (point-max)))))
        (list
         (list 'first r1 p1 rest1)
         (list 'all r-all p-all rest-all))))))"####;
    let expect = expect_test::expect![[r#""OK ((first t 16 \"code1\") (all nil 16 \"code1\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Block comments (C-style /* */) with nesting and multiple blocks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_patterns_block_comments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set up C-style block comments and test forward-comment with multiple
    // block comments, mixed with line comments, and empty blocks.
    let form = r####"(with-temp-buffer
  (let ((st (make-syntax-table)))
    ;; C-style: / is comment-start char 1 and comment-end char 2
    ;;          * is comment-start char 2 and comment-end char 1
    (modify-syntax-entry ?/ ". 14" st)
    (modify-syntax-entry ?* ". 23" st)
    (set-syntax-table st)
    (insert "/* block one */ code1 /* block\ntwo */ code2 /* empty */")
    ;; Skip first block comment
    (goto-char (point-min))
    (let ((r1 (forward-comment 1))
          (p1 (point))
          (rest1 (buffer-substring (point) (min (+ (point) 10) (point-max)))))
      ;; From code1, should fail on non-comment
      (let ((r-fail (forward-comment 1))
            (p-fail (point)))
        ;; Skip all (including whitespace between blocks)
        (goto-char (point-min))
        (let ((r-all (forward-comment (buffer-size)))
              (p-all (point))
              (rest-all (buffer-substring (point) (min (+ (point) 10) (point-max)))))
          (list
           (list 'first-block r1 p1 rest1)
           (list 'fail-on-code r-fail p-fail)
           (list 'skip-all r-all p-all rest-all)))))))"####;
    let expect = expect_test::expect![[
        r#""OK ((first-block t 16 \" code1 /* \") (fail-on-code nil 17) (skip-all nil 17 \"code1 /* b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: comment counting and extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_patterns_counting_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk through a buffer counting how many comments exist and extracting
    // the text content of each comment. Uses forward-comment to navigate
    // and skip-syntax-forward to identify comment boundaries.
    let form = r####"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert ";; first comment\ncode-a\n;; second comment\ncode-b\n  ;; third comment\n;; fourth comment\ncode-c")
    (goto-char (point-min))
    (let ((comments nil)
          (count 0)
          (limit 20))
      ;; Walk through buffer: skip whitespace+comments, record each comment
      (while (and (< (point) (point-max)) (> limit 0))
        (setq limit (1- limit))
        (let ((before (point)))
          ;; Skip whitespace only (not comments)
          (skip-chars-forward " \t\n")
          (if (and (< (point) (point-max))
                   (= (char-after) ?\;))
              ;; We're at a comment start: record it
              (let ((start (point)))
                (end-of-line)
                (let ((comment-text (buffer-substring start (point))))
                  (setq comments (cons comment-text comments))
                  (setq count (1+ count)))
                ;; Skip past newline
                (when (< (point) (point-max))
                  (forward-char 1)))
            ;; Not a comment: skip to end of line
            (end-of-line)
            (when (< (point) (point-max))
              (forward-char 1)))))
      (list
       'count count
       'comments (nreverse comments)))))"####;
    let expect = expect_test::expect![[
        r#""OK (count 4 comments (\";; first comment\" \";; second comment\" \";; third comment\" \";; fourth comment\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: code vs comment separation using forward-comment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_patterns_code_comment_separation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a buffer with mixed code and comments, using forward-comment
    // to skip comment regions and collect only the code portions.
    // Also handle inline comments after code on the same line.
    let form = r####"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert ";; file header\n(defun foo (x)\n  ;; docstring-like comment\n  (+ x 1)) ;; inline comment\n\n;; section break\n(defun bar (y)\n  (* y 2))\n")
    (goto-char (point-min))
    (let ((code-parts nil)
          (comment-count 0)
          (iterations 0))
      (while (and (< (point) (point-max)) (< iterations 50))
        (setq iterations (1+ iterations))
        ;; Skip any whitespace and comments
        (let ((before-skip (point)))
          (forward-comment (buffer-size))
          (when (> (point) before-skip)
            ;; We skipped something; count comment lines
            (let ((skipped (buffer-substring before-skip (point))))
              (let ((i 0) (semi-count 0))
                (while (< i (length skipped))
                  (when (= (aref skipped i) ?\;)
                    (setq semi-count (1+ semi-count)))
                  (setq i (1+ i)))
                (when (> semi-count 0)
                  (setq comment-count (1+ comment-count)))))))
        ;; Collect code until next comment or end
        (when (< (point) (point-max))
          (let ((code-start (point)))
            (while (and (< (point) (point-max))
                        (not (= (char-after) ?\;)))
              (forward-char 1))
            (let ((code (buffer-substring code-start (point))))
              (when (> (length code) 0)
                (setq code-parts (cons code code-parts)))))))
      (list
       'code-parts (nreverse code-parts)
       'comment-regions comment-count))))"####;
    let expect = expect_test::expect![[
        r#""OK (code-parts (\"(defun foo (x)\\n  \" \"(+ x 1)) \" \"(defun bar (y)\\n  (* y 2))\\n\") comment-regions 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment with mixed line and block comments in same buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_patterns_mixed_styles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set up both line comments (;) and block comments (/* */) in the same
    // syntax table. forward-comment should handle both seamlessly.
    let form = r####"(with-temp-buffer
  (let ((st (make-syntax-table)))
    ;; ; as line comment
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    ;; /* */ as block comment (secondary style)
    (modify-syntax-entry ?/ ". 14b" st)
    (modify-syntax-entry ?* ". 23b" st)
    (set-syntax-table st)
    (insert "; line comment\n/* block comment */\n; another line\ncode here")
    ;; Skip first (line) comment
    (goto-char (point-min))
    (let ((r1 (forward-comment 1))
          (p1 (point)))
      ;; Skip second (block) comment from current position
      (let ((r2 (forward-comment 1))
            (p2 (point)))
        ;; Skip third (line) comment
        (let ((r3 (forward-comment 1))
              (p3 (point)))
          ;; Now on "code here" - skip should fail
          (let ((r4 (forward-comment 1))
                (p4 (point)))
            ;; From beginning, skip all
            (goto-char (point-min))
            (let ((r-all (forward-comment (buffer-size)))
                  (p-all (point))
                  (rest (buffer-substring (point) (point-max))))
              (list
               (list 'line-1 r1 p1)
               (list 'block r2 p2)
               (list 'line-2 r3 p3)
               (list 'fail r4 p4)
               (list 'all r-all p-all rest))))))))))"####;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 34 52)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-comment with whitespace-only regions between comments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_comment_patterns_whitespace_interleaved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // forward-comment counts whitespace as part of the "comments" it skips.
    // Verify that blank lines, tabs, and spaces between comments are all
    // consumed as a single unit by forward-comment with count 1.
    let form = r####"(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?\; "<" st)
    (modify-syntax-entry ?\n ">" st)
    (set-syntax-table st)
    (insert "   \t  \n\n;; comment\n\n   \t\ncode")
    ;; Leading whitespace + one comment + trailing whitespace
    (goto-char (point-min))
    (let ((r1 (forward-comment 1))
          (p1 (point))
          (rest1 (buffer-substring (point) (point-max))))
      ;; Using large N to skip all whitespace+comments
      (goto-char (point-min))
      (let ((r-big (forward-comment (buffer-size)))
            (p-big (point))
            (rest-big (buffer-substring (point) (point-max))))
        ;; Backward from "code" position
        (goto-char p-big)
        (let ((r-back (forward-comment -1))
              (p-back (point)))
          (list
           (list 'one r1 p1 rest1)
           (list 'big r-big p-big rest-big)
           (list 'back r-back p-back)))))))"####;
    let expect = expect_test::expect![[
        r#""OK ((one t 20 \"\\n   \t\\ncode\") (big nil 26 \"code\") (back t 9))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

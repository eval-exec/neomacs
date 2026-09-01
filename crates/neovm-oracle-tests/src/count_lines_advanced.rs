//! Advanced oracle parity tests for `count-lines` and `line-number-at-pos`.
//!
//! Covers: count-lines between positions, empty buffer, trailing newline,
//! line-number-at-pos at various positions, ABSOLUTE parameter,
//! narrowed buffers, and building a line index table.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// count-lines between two positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_between_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count lines across various sub-ranges of a multi-line buffer.
    // Verify boundary behavior: count-lines counts newlines between START and END.
    let form = r####"(with-temp-buffer
                    (insert "alpha\nbeta\ngamma\ndelta\nepsilon\n")
                    (let ((results nil))
                      ;; Full buffer
                      (setq results (cons (count-lines (point-min) (point-max)) results))
                      ;; First line only (no newline crossed within "alpha")
                      (setq results (cons (count-lines 1 5) results))
                      ;; Across first newline
                      (setq results (cons (count-lines 1 7) results))
                      ;; From middle of one line to middle of another
                      (setq results (cons (count-lines 3 15) results))
                      ;; Single newline character
                      (setq results (cons (count-lines 6 7) results))
                      ;; Same position (zero lines)
                      (setq results (cons (count-lines 10 10) results))
                      ;; Reversed args should also work (count-lines handles both orders)
                      (setq results (cons (count-lines 15 3) results))
                      (nreverse results)))"####;
    let expect = expect_test::expect![[r#""OK (5 1 1 3 1 0 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines with empty buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_empty_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // An empty buffer has point-min = point-max = 1, so count-lines should be 0.
    let form = r####"(with-temp-buffer
                    (list
                      (count-lines (point-min) (point-max))
                      (point-min)
                      (point-max)
                      (= (point-min) (point-max))))"####;
    let expect = expect_test::expect![[r#""OK (0 1 1 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines with trailing newline vs no trailing newline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_trailing_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Trailing newline affects count-lines: "a\nb\n" has 2 newlines,
    // "a\nb" has 1 newline. Verify the difference.
    let form = r####"(let ((results nil))
                    (with-temp-buffer
                      (insert "line1\nline2\nline3\n")
                      (setq results (cons (count-lines (point-min) (point-max)) results)))
                    (with-temp-buffer
                      (insert "line1\nline2\nline3")
                      (setq results (cons (count-lines (point-min) (point-max)) results)))
                    (with-temp-buffer
                      (insert "\n\n\n")
                      (setq results (cons (count-lines (point-min) (point-max)) results)))
                    (with-temp-buffer
                      (insert "\n")
                      (setq results (cons (count-lines (point-min) (point-max)) results)))
                    (with-temp-buffer
                      (insert "no-newline-at-all")
                      (setq results (cons (count-lines (point-min) (point-max)) results)))
                    (nreverse results))"####;
    let expect = expect_test::expect![[r#""OK (3 3 3 1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-number-at-pos at various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_number_at_pos_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // line-number-at-pos returns 1-based line number at given position.
    // Test at beginning, end, and various interior points.
    let form = r####"(with-temp-buffer
                    (insert "aaa\nbbb\nccc\nddd\neee\n")
                    (let ((results nil))
                      ;; At very start: line 1
                      (setq results (cons (line-number-at-pos 1) results))
                      ;; Just after first newline: line 2
                      (setq results (cons (line-number-at-pos 5) results))
                      ;; In the middle of "ccc": still line 3
                      (setq results (cons (line-number-at-pos 10) results))
                      ;; Right on the newline after "ddd": still line 4
                      (setq results (cons (line-number-at-pos 16) results))
                      ;; At point-max (after trailing newline): line 6
                      (setq results (cons (line-number-at-pos (point-max)) results))
                      ;; With no arg, uses current point
                      (goto-char 9)
                      (setq results (cons (line-number-at-pos) results))
                      (nreverse results)))"####;
    let expect = expect_test::expect![[r#""OK (1 2 3 4 6 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-number-at-pos with ABSOLUTE parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_number_at_pos_absolute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When buffer is not narrowed, ABSOLUTE parameter should not change result.
    // Verify that both nil and t produce the same results in a non-narrowed buffer,
    // then verify they differ in a narrowed buffer.
    let form = r####"(with-temp-buffer
                    (insert "one\ntwo\nthree\nfour\nfive\nsix\nseven\neight\n")
                    ;; Without narrowing, absolute vs relative should agree
                    (let ((no-narrow
                            (list
                              (= (line-number-at-pos 1) (line-number-at-pos 1 t))
                              (= (line-number-at-pos 10) (line-number-at-pos 10 t))
                              (= (line-number-at-pos 20) (line-number-at-pos 20 t)))))
                      ;; Now narrow to lines 3-6 (positions 9 to 29 approximately)
                      (goto-char 9)
                      (let ((narrow-start (line-beginning-position)))
                        (goto-char 25)
                        (let ((narrow-end (line-end-position)))
                          (narrow-to-region narrow-start narrow-end)
                          (let ((narrowed-results
                                  (list
                                    ;; Relative (no ABSOLUTE): line 1 within narrowed region
                                    (line-number-at-pos (point-min))
                                    (line-number-at-pos (point-max))
                                    ;; Absolute: actual line in full buffer
                                    (line-number-at-pos (point-min) t)
                                    (line-number-at-pos (point-max) t))))
                            (widen)
                            (list no-narrow narrowed-results))))))"####;
    let expect = expect_test::expect![[r#""OK ((t t t) (1 4 3 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-number-at-pos in narrowed buffer with multiple narrowings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_number_at_pos_narrowed_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Narrow to different regions of the same buffer and check
    // that line-number-at-pos adjusts correctly each time.
    let form = r####"(with-temp-buffer
                    (dotimes (i 20)
                      (insert (format "line-%02d content here\n" (1+ i))))
                    (let ((all-results nil))
                      ;; Narrow to lines 5-10
                      (save-restriction
                        (goto-char (point-min))
                        (forward-line 4)
                        (let ((r-start (point)))
                          (forward-line 6)
                          (narrow-to-region r-start (point))
                          (setq all-results
                                (cons (list
                                        (line-number-at-pos (point-min))
                                        (line-number-at-pos (point-max))
                                        (line-number-at-pos (point-min) t)
                                        (line-number-at-pos (point-max) t))
                                      all-results))))
                      ;; Narrow to lines 15-20
                      (save-restriction
                        (goto-char (point-min))
                        (forward-line 14)
                        (let ((r-start (point)))
                          (goto-char (point-max))
                          (narrow-to-region r-start (point))
                          (setq all-results
                                (cons (list
                                        (line-number-at-pos (point-min))
                                        (line-number-at-pos (point-max))
                                        (line-number-at-pos (point-min) t)
                                        (line-number-at-pos (point-max) t))
                                      all-results))))
                      ;; Narrow to single line (line 1)
                      (save-restriction
                        (goto-char (point-min))
                        (let ((r-start (point)))
                          (forward-line 1)
                          (narrow-to-region r-start (point))
                          (setq all-results
                                (cons (list
                                        (line-number-at-pos (point-min))
                                        (line-number-at-pos (point-max))
                                        (line-number-at-pos (point-min) t)
                                        (line-number-at-pos (point-max) t))
                                      all-results))))
                      (nreverse all-results)))"####;
    let expect = expect_test::expect![[r#""OK ((1 7 5 11) (1 7 15 21) (1 2 1 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build a line index table from a buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_build_line_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complete index: for each line, record its number, start position,
    // end position, length, and content. Then use count-lines to verify
    // consistency, and use line-number-at-pos to cross-check.
    let form = r####"(progn
      (fset 'neovm--test-build-line-index
        (lambda ()
          (let ((index nil)
                (line-num 1))
            (goto-char (point-min))
            (while (not (eobp))
              (let ((line-start (point)))
                (end-of-line)
                (let ((line-end (point))
                      (line-content (buffer-substring-no-properties
                                      (line-beginning-position)
                                      (line-end-position))))
                  (setq index
                        (cons (list line-num line-start line-end
                                    (- line-end line-start)
                                    line-content)
                              index))
                  (setq line-num (1+ line-num))
                  (if (not (eobp))
                      (forward-char 1)))))
            (nreverse index))))
      (unwind-protect
          (with-temp-buffer
            (insert "#!/bin/bash\n")
            (insert "echo \"hello world\"\n")
            (insert "\n")
            (insert "# comment\n")
            (insert "exit 0\n")
            (let ((idx (funcall 'neovm--test-build-line-index)))
              ;; Cross-check: count-lines from start to each line's start
              ;; should equal (line-num - 1)
              (let ((checks nil))
                (dolist (entry idx)
                  (let ((lnum (car entry))
                        (lstart (cadr entry)))
                    (setq checks
                          (cons (list lnum
                                      (count-lines (point-min) lstart)
                                      (line-number-at-pos lstart))
                                checks))))
                (list idx (nreverse checks)
                      (count-lines (point-min) (point-max))))))
        (fmakunbound 'neovm--test-build-line-index)))"####;
    let expect = expect_test::expect![[
        r##""OK (((1 1 12 11 \"#!/bin/bash\") (2 13 31 18 \"echo \\\"hello world\\\"\") (3 32 32 0 \"\") (4 33 42 9 \"# comment\") (5 43 49 6 \"exit 0\")) ((1 0 1) (2 1 2) (3 2 3) (4 3 4) (5 4 5)) 5)""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines with programmatically constructed content
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_constructed_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build buffer content dynamically with varying line lengths,
    // then verify count-lines across non-trivial sub-ranges and
    // compare with line-number-at-pos differences.
    let form = r####"(with-temp-buffer
                    ;; Generate lines of increasing length: "a", "bb", "ccc", ...
                    (let ((lines nil))
                      (dotimes (i 10)
                        (let ((line (make-string (1+ i) (+ ?a i))))
                          (insert line "\n")
                          (setq lines (cons line lines))))
                      ;; Now verify various ranges
                      (let ((total (count-lines (point-min) (point-max)))
                            (first-half nil)
                            (second-half nil)
                            (cross-checks nil))
                        ;; Count lines in first 15 chars vs rest
                        (setq first-half (count-lines 1 16))
                        (setq second-half (count-lines 16 (point-max)))
                        ;; For each position that is a line start, verify
                        ;; line-number-at-pos matches accumulated count-lines + 1
                        (goto-char (point-min))
                        (let ((prev-pos (point)))
                          (dotimes (_ 10)
                            (let ((lnum (line-number-at-pos (point)))
                                  (cl (count-lines (point-min) (point))))
                              (setq cross-checks (cons (list (point) lnum cl) cross-checks)))
                            (forward-line 1)))
                        (list total first-half second-half
                              (nreverse cross-checks)
                              (nreverse lines)))))"####;
    let expect = expect_test::expect![[
        r#""OK (10 5 6 ((1 1 0) (3 2 1) (6 3 2) (10 4 3) (15 5 4) (21 6 5) (28 7 6) (36 8 7) (45 9 8) (55 10 9)) (\"a\" \"bb\" \"ccc\" \"dddd\" \"eeeee\" \"ffffff\" \"ggggggg\" \"hhhhhhhh\" \"iiiiiiiii\" \"jjjjjjjjjj\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

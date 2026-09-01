//! Advanced oracle parity tests for `line-beginning-position` and `line-end-position`.
//!
//! Covers: N argument (forward/backward lines), narrowed buffers, buffer boundaries,
//! empty lines, multi-line text, combined with `count-lines` and `line-number-at-pos`,
//! edge cases at point-min and point-max, and interaction with save-excursion.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// line-beginning-position / line-end-position with N argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_position_n_argument_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test line-beginning-position and line-end-position with various N values
    // including negative (backward), zero, positive, and large N.
    let form = r#"(with-temp-buffer
  (insert "Line-1\nLine-2\nLine-3\nLine-4\nLine-5\n")
  ;; Place point in the middle of Line-3
  (goto-char 18)
  (let ((results nil))
    ;; Current line (N=1 or omitted is the same as default)
    (setq results (cons (list 'lbp-default (line-beginning-position)
                              'lep-default (line-end-position))
                        results))
    ;; N=1 (current line, same as default)
    (setq results (cons (list 'lbp-1 (line-beginning-position 1)
                              'lep-1 (line-end-position 1))
                        results))
    ;; N=2 (next line)
    (setq results (cons (list 'lbp-2 (line-beginning-position 2)
                              'lep-2 (line-end-position 2))
                        results))
    ;; N=3 (two lines forward)
    (setq results (cons (list 'lbp-3 (line-beginning-position 3)
                              'lep-3 (line-end-position 3))
                        results))
    ;; N=0 (previous line)
    (setq results (cons (list 'lbp-0 (line-beginning-position 0)
                              'lep-0 (line-end-position 0))
                        results))
    ;; N=-1 (two lines back)
    (setq results (cons (list 'lbp-neg1 (line-beginning-position -1)
                              'lep-neg1 (line-end-position -1))
                        results))
    ;; Large N beyond buffer (should clamp)
    (setq results (cons (list 'lbp-100 (line-beginning-position 100)
                              'lep-100 (line-end-position 100))
                        results))
    ;; Large negative N beyond buffer
    (setq results (cons (list 'lbp-neg100 (line-beginning-position -100)
                              'lep-neg100 (line-end-position -100))
                        results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((lbp-default 15 lep-default 21) (lbp-1 15 lep-1 21) (lbp-2 22 lep-2 28) (lbp-3 29 lep-3 35) (lbp-0 8 lep-0 14) (lbp-neg1 1 lep-neg1 7) (lbp-100 36 lep-100 36) (lbp-neg100 1 lep-neg100 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_line_position_accepts_bignum_n_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU editfns.c accepts any integer for line-beginning-position and
    // line-end-position, clipping bignums to the accessible buffer bounds.
    // This differs from beginning-of-line/end-of-line commands in cmds.c,
    // which use CHECK_FIXNUM.
    let form = r#"(with-temp-buffer
  (insert "first\nsecond\nthird")
  (goto-char 8)
  (let ((huge 1000000000000000000000000000000)
        (tiny -1000000000000000000000000000000))
    (list (line-beginning-position huge)
          (line-end-position huge)
          (line-beginning-position tiny)
          (line-end-position tiny)
          (point))))"#;
    let expect = expect_test::expect![[r#""OK (19 19 1 1 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-beginning/end-position at buffer boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_position_at_buffer_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test at point-min, point-max, on trailing newline, and in single-line buffer.
    let form = r#"(let ((results nil))
  ;; Multi-line buffer, at very start
  (with-temp-buffer
    (insert "AAA\nBBB\nCCC")
    (goto-char (point-min))
    (setq results (cons (list 'at-min
                              (line-beginning-position)
                              (line-end-position)
                              (line-beginning-position 0)
                              (line-end-position 0))
                        results)))
  ;; At very end (no trailing newline)
  (with-temp-buffer
    (insert "AAA\nBBB\nCCC")
    (goto-char (point-max))
    (setq results (cons (list 'at-max-no-nl
                              (line-beginning-position)
                              (line-end-position)
                              (line-beginning-position 2)
                              (line-end-position 2))
                        results)))
  ;; At very end with trailing newline
  (with-temp-buffer
    (insert "AAA\nBBB\nCCC\n")
    (goto-char (point-max))
    (setq results (cons (list 'at-max-with-nl
                              (line-beginning-position)
                              (line-end-position)
                              (point))
                        results)))
  ;; Single-line buffer
  (with-temp-buffer
    (insert "only-one-line")
    (goto-char 5)
    (setq results (cons (list 'single-line
                              (line-beginning-position)
                              (line-end-position)
                              (line-beginning-position 0)
                              (line-end-position 2))
                        results)))
  ;; Empty buffer
  (with-temp-buffer
    (setq results (cons (list 'empty-buf
                              (line-beginning-position)
                              (line-end-position)
                              (point-min) (point-max))
                        results)))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((at-min 1 4 1 1) (at-max-no-nl 9 12 12 12) (at-max-with-nl 13 13 13) (single-line 1 14 1 14) (empty-buf 1 1 1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-beginning/end-position with empty lines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_position_empty_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Empty lines: consecutive newlines. Verify positions at, before, and after
    // empty lines, and that forward/backward N skips them correctly.
    let form = r#"(with-temp-buffer
  (insert "A\n\n\nB\n\nC\n")
  ;; Buffer: "A\n\n\nB\n\nC\n"
  ;; Line 1: "A"   (pos 1-1)
  ;; Line 2: ""    (pos 3-2, just newline)
  ;; Line 3: ""    (pos 4-3, just newline)
  ;; Line 4: "B"   (pos 5)
  ;; Line 5: ""    (pos 7)
  ;; Line 6: "C"   (pos 8)
  (let ((results nil))
    ;; At first empty line (position 3)
    (goto-char 3)
    (setq results (cons (list 'empty-line-1
                              (line-beginning-position)
                              (line-end-position)
                              (= (line-beginning-position) (line-end-position)))
                        results))
    ;; At second empty line (position 4)
    (goto-char 4)
    (setq results (cons (list 'empty-line-2
                              (line-beginning-position)
                              (line-end-position)
                              (line-beginning-position 2)
                              (line-end-position 2))
                        results))
    ;; From "B" (position 5), go back 2 lines
    (goto-char 5)
    (setq results (cons (list 'from-B-back-2
                              (line-beginning-position -1)
                              (line-end-position -1))
                        results))
    ;; Collect line-beginning-position for all lines using forward-line
    (goto-char (point-min))
    (let ((all-lbp nil))
      (while (not (eobp))
        (setq all-lbp (cons (list (line-beginning-position)
                                  (line-end-position))
                            all-lbp))
        (forward-line 1))
      ;; Also check last empty line after final newline
      (when (= (point) (point-max))
        (setq all-lbp (cons (list (line-beginning-position)
                                  (line-end-position))
                            all-lbp)))
      (setq results (cons (cons 'all-lines (nreverse all-lbp)) results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((empty-line-1 3 3 t) (empty-line-2 4 4 5 6) (from-B-back-2 3 3) (all-lines (1 2) (3 3) (4 4) (5 6) (7 7) (8 9) (10 10)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line positions in narrowed buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_position_narrowed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Narrow to a sub-region and verify that line-beginning-position and
    // line-end-position respect the narrowing boundaries.
    let form = r#"(with-temp-buffer
  (insert "Line-1\nLine-2\nLine-3\nLine-4\nLine-5\n")
  (let ((results nil))
    ;; Narrow to Lines 2-4 (positions 8..28)
    (save-restriction
      (narrow-to-region 8 28)
      ;; At point-min of narrowed region
      (goto-char (point-min))
      (setq results (cons (list 'narrow-start
                                (line-beginning-position)
                                (line-end-position)
                                (point-min) (point-max))
                          results))
      ;; Move to Line-3 within narrowed region
      (forward-line 1)
      (setq results (cons (list 'narrow-line3
                                (line-beginning-position)
                                (line-end-position)
                                (line-beginning-position 0)
                                (line-end-position 2))
                          results))
      ;; Try to go beyond narrowed region with N argument
      (goto-char (point-min))
      (setq results (cons (list 'narrow-beyond
                                (line-beginning-position -5)
                                (line-end-position 20))
                          results))
      ;; count-lines within narrowed region
      (setq results (cons (list 'narrow-count-lines
                                (count-lines (point-min) (point-max)))
                          results)))
    ;; After widening, verify full buffer access
    (goto-char 15)
    (setq results (cons (list 'widened
                              (line-beginning-position)
                              (line-end-position))
                        results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((narrow-start 8 14 8 28) (narrow-line3 15 21 8 28) (narrow-beyond 8 28) (narrow-count-lines 3) (widened 15 21))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines and line-number-at-pos comprehensive
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_position_count_lines_and_line_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test count-lines between various positions and line-number-at-pos
    // at every line of a multi-line buffer.
    let form = r#"(with-temp-buffer
  (insert "Alpha\nBravo\nCharlie\nDelta\nEcho\nFoxtrot\n")
  (let ((results nil))
    ;; count-lines for entire buffer
    (setq results (cons (list 'total-lines
                              (count-lines (point-min) (point-max)))
                        results))
    ;; count-lines for subranges
    (setq results (cons (list 'lines-1-to-20
                              (count-lines 1 20))
                        results))
    ;; count-lines when start = end
    (setq results (cons (list 'lines-same
                              (count-lines 10 10))
                        results))
    ;; count-lines for a single line (no newline crossed)
    (setq results (cons (list 'lines-within-line
                              (count-lines 1 4))
                        results))
    ;; line-number-at-pos at various positions
    (let ((line-nums nil))
      (dolist (pos '(1 6 7 12 13 20 21 27 28 33 34 41 42))
        (when (<= pos (point-max))
          (goto-char pos)
          (setq line-nums (cons (list pos (line-number-at-pos)) line-nums))))
      (setq results (cons (cons 'line-nums (nreverse line-nums)) results)))
    ;; line-number-at-pos at point-min and point-max
    (setq results (cons (list 'lnum-min (progn (goto-char (point-min))
                                               (line-number-at-pos))
                              'lnum-max (progn (goto-char (point-max))
                                               (line-number-at-pos)))
                        results))
    ;; Verify: count-lines from point-min to point = line-number-at-pos - 1
    (goto-char 20)
    (let ((cl (count-lines (point-min) (line-beginning-position)))
          (ln (line-number-at-pos)))
      (setq results (cons (list 'verify-consistency cl ln (= cl (1- ln)))
                          results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((total-lines 6) (lines-1-to-20 3) (lines-same 0) (lines-within-line 1) (line-nums (1 1) (6 1) (7 2) (12 2) (13 3) (20 3) (21 4) (27 5) (28 5) (33 6) (34 6)) (lnum-min 1 lnum-max 7) (verify-consistency 2 3 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line positions combined with save-excursion and goto-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_position_with_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use line-beginning-position and line-end-position inside save-excursion
    // to extract line content at various positions without moving point.
    let form = r#"(with-temp-buffer
  (insert "first line\nsecond line\nthird line\nfourth line\nfifth line\n")
  (goto-char 15) ;; somewhere in "second line"
  (let ((results nil)
        (original-point (point)))
    ;; Extract current line text
    (setq results (cons (buffer-substring (line-beginning-position)
                                          (line-end-position))
                        results))
    ;; Extract previous line text using save-excursion
    (setq results (cons (save-excursion
                          (forward-line -1)
                          (buffer-substring (line-beginning-position)
                                            (line-end-position)))
                        results))
    ;; Extract next line text
    (setq results (cons (save-excursion
                          (forward-line 1)
                          (buffer-substring (line-beginning-position)
                                            (line-end-position)))
                        results))
    ;; Verify point was not moved
    (setq results (cons (= (point) original-point) results))
    ;; Build a map of line-number -> line-content for the whole buffer
    (let ((line-map nil))
      (save-excursion
        (goto-char (point-min))
        (while (not (eobp))
          (setq line-map
                (cons (cons (line-number-at-pos)
                            (buffer-substring (line-beginning-position)
                                              (line-end-position)))
                      line-map))
          (forward-line 1)))
      (setq results (cons (nreverse line-map) results)))
    ;; Collect all line lengths
    (let ((lengths nil))
      (save-excursion
        (goto-char (point-min))
        (while (not (eobp))
          (setq lengths (cons (- (line-end-position) (line-beginning-position))
                              lengths))
          (forward-line 1)))
      (setq results (cons (cons 'lengths (nreverse lengths)) results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"second line\" \"first line\" \"third line\" t ((1 . \"first line\") (2 . \"second line\") (3 . \"third line\") (4 . \"fourth line\") (5 . \"fifth line\")) (lengths 10 11 10 11 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-beginning/end-position with long lines and mixed content
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_position_long_lines_and_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test with very long lines, lines of different lengths, and lines
    // containing tabs and special characters.
    let form = r#"(with-temp-buffer
  (insert (make-string 200 ?X))
  (insert "\n")
  (insert "short\n")
  (insert "\tindented\twith\ttabs\n")
  (insert "\n")
  (insert "last")
  (let ((results nil))
    ;; Long line: beginning and end
    (goto-char 100)
    (setq results (cons (list 'long-line
                              (line-beginning-position)
                              (line-end-position)
                              (- (line-end-position) (line-beginning-position)))
                        results))
    ;; Short line after long line
    (goto-char 203)
    (setq results (cons (list 'short-after-long
                              (line-beginning-position)
                              (line-end-position))
                        results))
    ;; Tab line
    (forward-line 1)
    (setq results (cons (list 'tab-line
                              (line-beginning-position)
                              (line-end-position)
                              (buffer-substring (line-beginning-position)
                                                (line-end-position)))
                        results))
    ;; Empty line
    (forward-line 1)
    (setq results (cons (list 'empty-line
                              (line-beginning-position)
                              (line-end-position)
                              (= (line-beginning-position) (line-end-position)))
                        results))
    ;; Last line (no trailing newline)
    (forward-line 1)
    (setq results (cons (list 'last-no-nl
                              (line-beginning-position)
                              (line-end-position)
                              (= (line-end-position) (point-max)))
                        results))
    ;; Total line count
    (setq results (cons (list 'total
                              (count-lines (point-min) (point-max)))
                        results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((long-line 1 201 200) (short-after-long 202 207) (tab-line 208 227 \"\tindented\twith\ttabs\") (empty-line 228 228 t) (last-no-nl 229 233 t) (total 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Practical: extract lines by number range using line positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_position_extract_line_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a function that extracts lines N through M (1-indexed) from
    // a buffer using line-beginning-position and line-end-position.
    let form = r#"(progn
  (fset 'neovm--test-extract-lines
    (lambda (start-line end-line)
      "Extract lines START-LINE to END-LINE (inclusive, 1-indexed) from current buffer."
      (save-excursion
        (goto-char (point-min))
        (forward-line (1- start-line))
        (let ((region-start (point))
              (lines nil))
          (while (<= (line-number-at-pos) end-line)
            (setq lines (cons (buffer-substring (line-beginning-position)
                                                (line-end-position))
                              lines))
            (when (= (forward-line 1) 1) ;; hit end of buffer
              (setq end-line -1))) ;; force exit
          (cons (nreverse lines) region-start)))))
  (unwind-protect
      (with-temp-buffer
        (insert "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\ntheta\n")
        (list
          ;; Extract lines 2-4
          (funcall 'neovm--test-extract-lines 2 4)
          ;; Extract first line only
          (funcall 'neovm--test-extract-lines 1 1)
          ;; Extract last two lines
          (funcall 'neovm--test-extract-lines 7 8)
          ;; Extract beyond end (should get what exists)
          (funcall 'neovm--test-extract-lines 6 20)
          ;; Single line in middle
          (funcall 'neovm--test-extract-lines 4 4)
          ;; Verify round-trip: extract all lines and rejoin
          (let* ((all (car (funcall 'neovm--test-extract-lines 1 8)))
                 (rejoined (mapconcat 'identity all "\n")))
            (string= (concat rejoined "\n")
                     (buffer-substring (point-min)
                                       (point-max))))))
    (fmakunbound 'neovm--test-extract-lines)))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"beta\" \"gamma\" \"delta\") . 7) ((\"alpha\") . 1) ((\"eta\" \"theta\") . 37) ((\"zeta\" \"eta\" \"theta\" \"\") . 32) ((\"delta\") . 18) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

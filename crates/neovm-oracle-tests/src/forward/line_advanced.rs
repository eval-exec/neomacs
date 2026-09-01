//! Oracle parity tests for advanced `forward-line` behavior:
//! return value semantics (0 on success, N remaining on failure),
//! negative argument (backward movement), behavior at buffer boundaries,
//! interaction with narrowing, empty lines, very large N values,
//! and combined usage with beginning-of-line / end-of-line.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Return value semantics: 0 on success, remaining lines on partial movement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_line_return_value_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "line1\nline2\nline3\nline4\nline5\n")
  (let ((results nil))
    ;; Forward from beginning: move 1 line (success)
    (goto-char (point-min))
    (let ((ret (forward-line 1)))
      (setq results (cons (list 'fwd-1 ret (point)
                                 (buffer-substring (point) (line-end-position)))
                          results)))
    ;; Forward 2 more lines (success)
    (let ((ret (forward-line 2)))
      (setq results (cons (list 'fwd-2 ret (point)
                                 (buffer-substring (point) (line-end-position)))
                          results)))
    ;; Forward 10 lines from line 4 (only 2 lines remain)
    (let ((ret (forward-line 10)))
      (setq results (cons (list 'fwd-10-partial ret (point) (= (point) (point-max))) results)))
    ;; Forward 0 lines: go to beginning of current line, return 0
    (goto-char (point-min))
    (forward-line 2)
    (forward-char 3)
    (let ((before (point))
          (ret (forward-line 0)))
      (setq results (cons (list 'fwd-0 ret (point)
                                 (= (point) (line-beginning-position)))
                          results)))
    ;; Exactly at last newline: forward-line 1 goes to point-max
    (goto-char (point-min))
    (forward-line 4)
    (let ((pos-before (point))
          (ret (forward-line 1)))
      (setq results (cons (list 'fwd-from-last-line ret (point)) results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((fwd-1 0 7 \"line2\") (fwd-2 0 19 \"line4\") (fwd-10-partial 8 31 t) (fwd-0 0 13 t) (fwd-from-last-line 0 31))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_forward_line_accepts_bignum_n_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU cmds.c uses CHECK_INTEGER for forward-line.  For bignums it
    // saturates the movement count to the accessible buffer size, moves as far
    // as possible, and returns the arithmetic remainder relative to N.
    // This differs from forward-char/backward-char, which use CHECK_FIXNUM.
    let form = r#"(with-temp-buffer
  (insert "a\nb\nc")
  (let ((huge 1000000000000000000000000000000)
        (tiny -1000000000000000000000000000000)
        (results nil))
    (goto-char (point-min))
    (push (list 'huge (forward-line huge) (point)) results)
    (goto-char (point-max))
    (push (list 'tiny (forward-line tiny) (point)) results)
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((huge 999999999999999999999999999997 6) (tiny -999999999999999999999999999998 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Negative argument: backward line movement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_line_negative_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "first\nsecond\nthird\nfourth\nfifth\n")
  (let ((results nil))
    ;; Go to end, then backward
    (goto-char (point-max))
    (let ((ret (forward-line -1)))
      (setq results (cons (list 'back-1-from-end ret (point)
                                 (buffer-substring (point) (line-end-position)))
                          results)))
    ;; Backward 2 more
    (let ((ret (forward-line -2)))
      (setq results (cons (list 'back-2 ret (point)
                                 (buffer-substring (point) (line-end-position)))
                          results)))
    ;; Backward from line 2 by 5 (only 1 line above): remainder is -4?
    ;; Actually forward-line -N returns negative remainder
    (goto-char (point-min))
    (forward-line 1)
    (let ((ret (forward-line -5)))
      (setq results (cons (list 'back-5-from-line2 ret (point)
                                 (= (point) (point-min)))
                          results)))
    ;; Backward 0: should do nothing
    (goto-char (point-min))
    (forward-line 2)
    (forward-char 3)
    (let ((pos-before (point))
          (ret (forward-line 0)))
      (setq results (cons (list 'back-0 ret (point)
                                 ;; forward-line 0 moves to bol
                                 (= (point) (line-beginning-position)))
                          results)))
    ;; Backward from middle of buffer
    (goto-char (point-min))
    (forward-line 3)
    (forward-char 2)
    (let ((ret (forward-line -2)))
      (setq results (cons (list 'back-2-from-middle ret (point)
                                 (buffer-substring (point) (line-end-position)))
                          results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((back-1-from-end 0 27 \"fifth\") (back-2 0 14 \"third\") (back-5-from-line2 -4 1 t) (back-0 0 14 t) (back-2-from-middle 0 7 \"second\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Behavior at buffer boundaries: beginning, end, single-line buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_line_buffer_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; Empty buffer
  (with-temp-buffer
    (let ((ret-fwd (forward-line 1))
          (pt-fwd (point)))
      (goto-char (point-min))
      (let ((ret-back (forward-line -1))
            (pt-back (point)))
        (setq results (cons (list 'empty-fwd ret-fwd pt-fwd
                                   'empty-back ret-back pt-back)
                            results)))))
  ;; Single character, no newline
  (with-temp-buffer
    (insert "X")
    (goto-char (point-min))
    (let ((ret (forward-line 1)))
      (setq results (cons (list 'single-char-fwd ret (point)
                                 (= (point) (point-max)))
                          results)))
    (let ((ret (forward-line -1)))
      (setq results (cons (list 'single-char-back-from-end ret (point)) results))))
  ;; Single line with newline
  (with-temp-buffer
    (insert "hello\n")
    (goto-char (point-min))
    (let ((ret1 (forward-line 1)))
      (let ((pt1 (point))
            (ret2 (forward-line 1)))
        (setq results (cons (list 'single-line-with-nl
                                   ret1 pt1
                                   ret2 (point))
                            results)))))
  ;; Only newlines
  (with-temp-buffer
    (insert "\n\n\n\n\n")
    (goto-char (point-min))
    (let ((positions nil))
      (dotimes (i 7)
        (let ((ret (forward-line 1)))
          (setq positions (cons (list i ret (point)) positions))))
      (setq results (cons (list 'only-newlines (nreverse positions)) results))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((empty-fwd 1 1 empty-back -1 1) (single-char-fwd 0 2 t) (single-char-back-from-end -1 1) (single-line-with-nl 0 7 1 7) (only-newlines ((0 0 2) (1 0 3) (2 0 4) (3 0 5) (4 0 6) (5 1 6) (6 1 6))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with narrowing: forward-line confined to narrowed region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_line_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "line-A\nline-B\nline-C\nline-D\nline-E\nline-F\nline-G\n")
  (let ((results nil))
    ;; Narrow to lines C-E (find their positions)
    (goto-char (point-min))
    (forward-line 2)
    (let ((start (point)))
      (forward-line 3)
      (let ((end (point)))
        (save-restriction
          (narrow-to-region start end)
          ;; Content should be "line-C\nline-D\nline-E\n"
          (setq results (cons (list 'narrow-content (buffer-string)) results))
          ;; Forward through all lines in narrowed region
          (goto-char (point-min))
          (let ((traversal nil))
            (let ((ret 0))
              (while (= ret 0)
                (setq traversal (cons (list (point)
                                             (buffer-substring (point)
                                                               (min (line-end-position) (point-max))))
                                      traversal))
                (setq ret (forward-line 1)))
              (setq results (cons (list 'traversal (nreverse traversal)
                                         'final-ret ret 'final-point (point))
                                  results))))
          ;; Backward from end of narrow region
          (goto-char (point-max))
          (let ((ret (forward-line -5)))
            (setq results (cons (list 'back-5-in-narrow ret (point)
                                       (= (point) (point-min)))
                                results)))
          ;; forward-line -1 from middle
          (goto-char (point-min))
          (forward-line 1)
          (let ((ret (forward-line -1)))
            (setq results (cons (list 'back-1-from-middle ret (point)
                                       (buffer-substring (point) (line-end-position)))
                                results))))))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((narrow-content \"line-C\\nline-D\\nline-E\\n\") (traversal ((15 \"line-C\") (22 \"line-D\") (29 \"line-E\") (36 \"\")) final-ret 1 final-point 36) (back-5-in-narrow -2 15 t) (back-1-from-middle 0 15 \"line-C\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty lines: forward-line with consecutive newlines
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_line_empty_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "header\n\n\ndata1\n\ndata2\n\n\n\nfooter")
  (let ((results nil))
    ;; Traverse all lines, recording which are empty
    (goto-char (point-min))
    (let ((lines nil)
          (line-num 0)
          (done nil))
      (while (not done)
        (let ((bol (line-beginning-position))
              (eol (line-end-position)))
          (setq lines (cons (list line-num
                                   (= bol eol)
                                   (buffer-substring bol eol))
                            lines))
          (setq line-num (1+ line-num))
          (let ((ret (forward-line 1)))
            (when (/= ret 0)
              (setq done t)))))
      (setq results (cons (list 'all-lines (nreverse lines)) results)))
    ;; Skip 3 lines from start (should land on first data line)
    (goto-char (point-min))
    (let ((ret (forward-line 3)))
      (setq results (cons (list 'skip-3 ret (point)
                                 (buffer-substring (point) (line-end-position)))
                          results)))
    ;; Count empty lines using forward-line
    (goto-char (point-min))
    (let ((empty-count 0)
          (total-count 0)
          (done nil))
      (while (not done)
        (when (= (line-beginning-position) (line-end-position))
          (setq empty-count (1+ empty-count)))
        (setq total-count (1+ total-count))
        (let ((ret (forward-line 1)))
          (when (/= ret 0) (setq done t))))
      (setq results (cons (list 'empty-count empty-count
                                 'total-lines total-count)
                          results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((all-lines ((0 nil \"header\") (1 t \"\") (2 t \"\") (3 nil \"data1\") (4 t \"\") (5 nil \"data2\") (6 t \"\") (7 t \"\") (8 t \"\") (9 nil \"footer\") (10 nil \"footer\"))) (skip-3 0 10 \"data1\") (empty-count 6 total-lines 11))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Large N values: forward-line with very large positive/negative arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_line_large_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  ;; Build a buffer with 20 lines
  (let ((i 0))
    (while (< i 20)
      (insert (format "line-%02d\n" i))
      (setq i (1+ i))))
  (let ((results nil))
    ;; Forward 1000 lines from start (only 20 exist)
    (goto-char (point-min))
    (let ((ret (forward-line 1000)))
      (setq results (cons (list 'fwd-1000 ret (point) (= (point) (point-max))) results)))
    ;; Backward 1000 lines from end
    (goto-char (point-max))
    (let ((ret (forward-line -1000)))
      (setq results (cons (list 'back-1000 ret (point) (= (point) (point-min))) results)))
    ;; Forward exact number of lines
    (goto-char (point-min))
    (let ((ret (forward-line 20)))
      (setq results (cons (list 'fwd-exact-20 ret (point) (= (point) (point-max))) results)))
    ;; Forward 19: should succeed (line 0 to line 19)
    (goto-char (point-min))
    (let ((ret (forward-line 19)))
      (setq results (cons (list 'fwd-19 ret (point)
                                 (buffer-substring (point) (line-end-position)))
                          results)))
    ;; From middle, forward beyond end
    (goto-char (point-min))
    (forward-line 10)
    (let ((ret (forward-line 500)))
      (setq results (cons (list 'fwd-500-from-10 ret (point)
                                 (= (point) (point-max)))
                          results)))
    ;; From middle, backward beyond start
    (goto-char (point-min))
    (forward-line 10)
    (let ((ret (forward-line -500)))
      (setq results (cons (list 'back-500-from-10 ret (point)
                                 (= (point) (point-min)))
                          results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((fwd-1000 980 161 t) (back-1000 -980 1 t) (fwd-exact-20 0 161 t) (fwd-19 0 153 \"line-19\") (fwd-500-from-10 490 161 t) (back-500-from-10 -490 1 t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined with beginning-of-line and end-of-line operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_forward_line_with_bol_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "short\na much longer line here\nx\n\nmedium line\n")
  (let ((results nil))
    ;; Collect line info: for each line, record bol, eol, line length
    (goto-char (point-min))
    (let ((line-info nil)
          (done nil))
      (while (not done)
        (let ((bol (line-beginning-position))
              (eol (line-end-position)))
          (setq line-info (cons (list 'line bol eol (- eol bol)
                                       (buffer-substring bol eol))
                                line-info)))
        (let ((ret (forward-line 1)))
          (when (/= ret 0) (setq done t))))
      (setq results (cons (list 'line-info (nreverse line-info)) results)))
    ;; forward-line then end-of-line: get to end of target line
    (goto-char (point-min))
    (forward-line 1)
    (end-of-line)
    (setq results (cons (list 'fwd1-eol (point)
                               (buffer-substring (line-beginning-position) (point)))
                        results))
    ;; forward-line 0 from middle of line goes to bol
    (goto-char (point-min))
    (forward-line 1)
    (forward-char 5)
    (let ((mid (point)))
      (forward-line 0)
      (setq results (cons (list 'fwd0-from-mid mid (point)
                                 (= (point) (line-beginning-position)))
                          results)))
    ;; Use forward-line + beginning-of-line + end-of-line to extract line
    (goto-char (point-min))
    (forward-line 1)
    (beginning-of-line)
    (let ((start (point)))
      (end-of-line)
      (let ((extracted (buffer-substring start (point))))
        (setq results (cons (list 'extracted-line-1 extracted) results))))
    ;; forward-line across empty line
    (goto-char (point-min))
    (forward-line 3)
    (let ((on-empty (= (line-beginning-position) (line-end-position))))
      (setq results (cons (list 'on-empty-line on-empty (point)) results))
      ;; forward-line 1 from empty line
      (let ((ret (forward-line 1)))
        (setq results (cons (list 'fwd-from-empty ret
                                   (buffer-substring (point) (line-end-position)))
                            results))))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((line-info ((line 1 6 5 \"short\") (line 7 30 23 \"a much longer line here\") (line 31 32 1 \"x\") (line 33 33 0 \"\") (line 34 45 11 \"medium line\") (line 46 46 0 \"\"))) (fwd1-eol 30 \"a much longer line here\") (fwd0-from-mid 12 7 t) (extracted-line-1 \"a much longer line here\") (on-empty-line t 33) (fwd-from-empty 0 \"medium line\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

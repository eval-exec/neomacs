//! Oracle parity tests for buffer position functions with complex interaction patterns:
//! `point`, `point-min`, `point-max` in normal and narrowed buffers,
//! `goto-char` with various positions, `bolp`, `eolp`, `bobp`, `eobp`,
//! `line-beginning-position`, `line-end-position` with N argument,
//! position tracking through multiple operations, position arithmetic with narrowing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// point, point-min, point-max in normal and narrowed buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_basic_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "abcdefghij\nklmnopqrst\nuvwxyz")
  (let ((results nil))
    ;; Initial state: point is at end after inserts
    (push (list 'after-insert (point) (point-min) (point-max)) results)
    ;; Go to beginning
    (goto-char (point-min))
    (push (list 'at-min (point) (point-min) (point-max)) results)
    ;; Go to middle
    (goto-char 15)
    (push (list 'at-15 (point) (point-min) (point-max)) results)
    ;; Narrow to second line
    (save-restriction
      (narrow-to-region 12 22)
      (push (list 'narrowed (point) (point-min) (point-max)) results)
      ;; point-min/max reflect narrowed region
      (goto-char (point-min))
      (push (list 'narrow-min (point) (point-min) (point-max)) results)
      (goto-char (point-max))
      (push (list 'narrow-max (point) (point-min) (point-max)) results))
    ;; After widen, point should be restored
    (push (list 'after-widen (point) (point-min) (point-max)) results)
    (nreverse results)))"#;
    let expect = expect_test::expect![
        r#""OK ((after-insert 29 1 29) (at-min 1 1 29) (at-15 15 1 29) (narrowed 15 12 22) (narrow-min 12 12 22) (narrow-max 22 12 22) (after-widen 22 1 29))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// goto-char with various positions (beginning, end, middle, beyond range)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_goto_char_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "Hello, World!\nSecond line\nThird line")
  (let ((results nil))
    ;; goto-char to beginning
    (goto-char 1)
    (push (list 'pos-1 (point) (char-after)) results)
    ;; goto-char to last char
    (goto-char (1- (point-max)))
    (push (list 'last-char (point) (char-after)) results)
    ;; goto-char to point-max (past last char)
    (goto-char (point-max))
    (push (list 'at-max (point) (char-after)) results)
    ;; goto-char to middle of second line
    (goto-char 20)
    (push (list 'mid-second (point) (char-after)) results)
    ;; goto-char to newline character
    (goto-char 14)
    (push (list 'at-newline (point) (= (char-after) ?\n)) results)
    ;; goto-char with narrowing: beyond narrow range clips to range
    (save-restriction
      (narrow-to-region 8 20)
      (goto-char 1)
      (push (list 'narrow-beyond-low (point)) results)
      (goto-char 100)
      (push (list 'narrow-beyond-high (point)) results)
      (goto-char 10)
      (push (list 'narrow-inside (point)) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![
        r#""OK ((pos-1 1 72) (last-char 36 101) (at-max 37 nil) (mid-second 20 100) (at-newline 14 t) (narrow-beyond-low 8) (narrow-beyond-high 20) (narrow-inside 10))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// bolp, eolp, bobp, eobp at various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "first\nsecond\nthird")
  (let ((results nil))
    ;; At beginning of buffer
    (goto-char (point-min))
    (push (list 'bob (bobp) (bolp) (eolp) (eobp)) results)
    ;; At end of first line (before newline)
    (goto-char 6)
    (push (list 'eol1 (bobp) (bolp) (eolp) (eobp)) results)
    ;; At beginning of second line (after first newline)
    (goto-char 7)
    (push (list 'bol2 (bobp) (bolp) (eolp) (eobp)) results)
    ;; At end of second line
    (goto-char 13)
    (push (list 'eol2 (bobp) (bolp) (eolp) (eobp)) results)
    ;; At beginning of third line
    (goto-char 14)
    (push (list 'bol3 (bobp) (bolp) (eolp) (eobp)) results)
    ;; At end of buffer (no trailing newline)
    (goto-char (point-max))
    (push (list 'eob (bobp) (bolp) (eolp) (eobp)) results)
    ;; Middle of a line
    (goto-char 3)
    (push (list 'mid (bobp) (bolp) (eolp) (eobp)) results)
    ;; Empty buffer
    (erase-buffer)
    (push (list 'empty (bobp) (bolp) (eolp) (eobp)) results)
    (nreverse results)))"#;
    let expect = expect_test::expect![
        r#""OK ((bob t t nil nil) (eol1 nil nil t nil) (bol2 nil t nil nil) (eol2 nil nil t nil) (bol3 nil t nil nil) (eob nil nil t t) (mid nil nil nil nil) (empty t t t t))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// bolp/eolp/bobp/eobp with narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_predicates_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "aaaa\nbbbb\ncccc\ndddd")
  (save-restriction
    ;; Narrow to second and third lines
    (narrow-to-region 6 16)
    (let ((results nil))
      ;; At point-min of narrowed region
      (goto-char (point-min))
      (push (list 'narrow-start (bobp) (bolp) (eolp) (eobp) (point)) results)
      ;; At point-max of narrowed region
      (goto-char (point-max))
      (push (list 'narrow-end (bobp) (bolp) (eolp) (eobp) (point)) results)
      ;; At a newline within narrowed region
      (goto-char 11)
      (push (list 'narrow-mid-eol (bobp) (bolp) (eolp) (eobp) (point)) results)
      ;; After the newline
      (goto-char 12)
      (push (list 'narrow-mid-bol (bobp) (bolp) (eolp) (eobp) (point)) results)
      (nreverse results))))"#;
    let expect = expect_test::expect![
        r#""OK ((narrow-start t t nil nil 6) (narrow-end nil t t t 16) (narrow-mid-eol nil t nil nil 11) (narrow-mid-bol nil nil nil nil 12))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-beginning-position, line-end-position with N argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_line_positions_with_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "Line one\nLine two\nLine three\nLine four\nLine five\n")
  (let ((results nil))
    ;; From middle of line three
    (goto-char 22)
    ;; Default (no arg) = current line
    (push (list 'default
                (line-beginning-position)
                (line-end-position))
          results)
    ;; N=1 means current line (same as default)
    (push (list 'n=1
                (line-beginning-position 1)
                (line-end-position 1))
          results)
    ;; N=0 means previous line
    (push (list 'n=0
                (line-beginning-position 0)
                (line-end-position 0))
          results)
    ;; N=2 means next line
    (push (list 'n=2
                (line-beginning-position 2)
                (line-end-position 2))
          results)
    ;; N=-1 means two lines back
    (push (list 'n=-1
                (line-beginning-position -1)
                (line-end-position -1))
          results)
    ;; N=3 means two lines ahead
    (push (list 'n=3
                (line-beginning-position 3)
                (line-end-position 3))
          results)
    ;; From first line, N=0 should clip to point-min
    (goto-char 5)
    (push (list 'first-n=0
                (line-beginning-position 0)
                (line-end-position 0))
          results)
    ;; From last line, N=2 should clip to point-max
    (goto-char 42)
    (push (list 'last-n=2
                (line-beginning-position 2)
                (line-end-position 2))
          results)
    (nreverse results)))"#;
    let expect = expect_test::expect![
        r#""OK ((default 19 29) (n=1 19 29) (n=0 10 18) (n=2 30 39) (n=-1 1 9) (n=3 40 49) (first-n=0 1 1) (last-n=2 50 50))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: position tracking through multiple insert/delete operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_tracking_through_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (let ((results nil))
    ;; Start with some text
    (insert "Hello World")
    (push (list 'initial (point) (point-max) (buffer-string)) results)
    ;; Go to middle and insert
    (goto-char 6)
    (insert ", Beautiful")
    (push (list 'after-insert (point) (point-max) (buffer-string)) results)
    ;; Delete a region
    (delete-region 6 17)
    (push (list 'after-delete (point) (point-max) (buffer-string)) results)
    ;; Insert at beginning
    (goto-char (point-min))
    (insert "=> ")
    (push (list 'after-prefix (point) (point-max) (buffer-string)) results)
    ;; Track position through save-excursion
    (goto-char 6)
    (let ((before-point (point)))
      (save-excursion
        (goto-char (point-max))
        (insert "!!")
        (goto-char (point-min))
        (insert "** "))
      (push (list 'after-save-exc (point) before-point (point-max) (buffer-string)) results))
    ;; Multiple inserts at same position
    (goto-char 5)
    (insert "A")
    (insert "B")
    (insert "C")
    (push (list 'multi-insert (point) (buffer-string)) results)
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((initial 12 12 \"Hello World\") (after-insert 17 23 \"Hello, Beautiful World\") (after-delete 6 12 \"Hello World\") (after-prefix 4 15 \"=> Hello World\") (after-save-exc 9 6 20 \"** => Hello World!!\") (multi-insert 8 \"** =ABC> Hello World!!\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: position arithmetic with narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_narrowing_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "0123456789\nabcdefghij\nKLMNOPQRST\nuvwxyz!@#$")
  (let ((results nil))
    ;; Full buffer stats
    (push (list 'full (point-min) (point-max) (- (point-max) (point-min))) results)
    ;; Narrow to lines 2-3 (positions 12 to 33)
    (save-restriction
      (narrow-to-region 12 33)
      (push (list 'narrow-range (point-min) (point-max) (- (point-max) (point-min))) results)
      ;; Navigate within narrowed region
      (goto-char (point-min))
      (push (list 'narrow-begin (point) (following-char)) results)
      ;; Line positions within narrowed region
      (goto-char 15)
      (push (list 'narrow-line-pos
                  (line-beginning-position)
                  (line-end-position)
                  (- (line-end-position) (line-beginning-position)))
            results)
      ;; Move to second line of narrowed region
      (goto-char 25)
      (push (list 'narrow-line2
                  (line-beginning-position)
                  (line-end-position))
            results)
      ;; buffer-substring within narrowed region
      (push (list 'narrow-content (buffer-substring (point-min) (point-max))) results)
      ;; count-lines within narrowed region
      (push (list 'narrow-lines (count-lines (point-min) (point-max))) results))
    ;; After widen: full range restored
    (push (list 'widened (point-min) (point-max)) results)
    ;; Nested narrowing
    (save-restriction
      (narrow-to-region 12 33)
      (save-restriction
        (narrow-to-region 15 25)
        (push (list 'nested-narrow (point-min) (point-max)
                    (buffer-substring (point-min) (point-max)))
              results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((full 1 44 43) (narrow-range 12 33 21) (narrow-begin 12 97) (narrow-line-pos 12 22 10) (narrow-line2 23 33) (narrow-content \"abcdefghij\\nKLMNOPQRST\") (narrow-lines 2) (widened 1 44) (nested-narrow 15 25 \"defghij\\nKL\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building a line index using position functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_line_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complete index mapping line numbers to their start/end positions
    // and lengths, then verify random access into the index
    let form = r#"(with-temp-buffer
  (insert "short\n")
  (insert "a longer second line\n")
  (insert "\n")
  (insert "fourth line with some text\n")
  (insert "5\n")
  (insert "sixth and final line")
  (goto-char (point-min))
  (let ((index nil)
        (line-num 1))
    ;; Build index
    (while (not (eobp))
      (let ((bol (line-beginning-position))
            (eol (line-end-position)))
        (push (list line-num bol eol (- eol bol)
                    (buffer-substring bol eol)
                    (bolp) (eolp))
              index))
      (setq line-num (1+ line-num))
      (forward-line 1))
    (setq index (nreverse index))
    ;; Verify: look up specific lines and check consistency
    (let ((results nil))
      ;; Total line count
      (push (list 'total-lines (length index)) results)
      ;; First line
      (push (list 'first (nth 0 index)) results)
      ;; Empty line (line 3)
      (push (list 'empty-line (nth 2 index)) results)
      ;; Last line (no trailing newline)
      (push (list 'last (car (last index))) results)
      ;; Sum of all line lengths
      (push (list 'total-length
                  (apply #'+ (mapcar (lambda (entry) (nth 3 entry)) index)))
            results)
      ;; Verify: all bol values are strictly increasing
      (push (list 'bols-increasing
                  (let ((prev 0) (ok t))
                    (dolist (entry index ok)
                      (when (<= (nth 1 entry) prev)
                        (setq ok nil))
                      (setq prev (nth 1 entry)))))
            results)
      (nreverse results))))"#;
    let expect = expect_test::expect![[
        r#""OK ((total-lines 6) (first (1 1 6 5 \"short\" t nil)) (empty-line (3 28 28 0 \"\" t t)) (last (6 58 78 20 \"sixth and final line\" t nil)) (total-length 72) (bols-increasing t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: position-based text manipulation pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_manipulation_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A pipeline that uses position functions to indent, prefix, and transform text
    let form = r#"(with-temp-buffer
  (insert "alpha\nbeta\ngamma\ndelta\nepsilon")
  (let ((results nil))
    ;; Step 1: Collect all line starts
    (goto-char (point-min))
    (let ((starts nil))
      (while (not (eobp))
        (push (point) starts)
        (forward-line 1))
      (setq starts (nreverse starts))
      (push (list 'line-starts starts) results))
    ;; Step 2: Prefix each line with its line number using save-excursion
    (goto-char (point-min))
    (let ((n 1))
      (while (not (eobp))
        (let ((prefix (format "%d: " n)))
          (insert prefix))
        (setq n (1+ n))
        (forward-line 1)))
    (push (list 'prefixed (buffer-string)) results)
    ;; Step 3: Verify positions shifted correctly
    (goto-char (point-min))
    (let ((line-lengths nil))
      (while (not (eobp))
        (push (- (line-end-position) (line-beginning-position)) line-lengths)
        (forward-line 1))
      (push (list 'lengths (nreverse line-lengths)) results))
    ;; Step 4: Use save-restriction + narrow to operate on single line
    (goto-char (point-min))
    (forward-line 2) ;; go to third line
    (save-restriction
      (narrow-to-region (line-beginning-position) (line-end-position))
      (push (list 'narrowed-line
                  (buffer-string)
                  (point-min) (point-max)
                  (- (point-max) (point-min)))
            results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((line-starts (1 7 12 18 24)) (prefixed \"1: alpha\\n2: beta\\n3: gamma\\n4: delta\\n5: epsilon\") (lengths (8 7 8 8 10)) (narrowed-line \"3: gamma\" 18 26 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge case: single character buffer, point at various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_position_patterns_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; Single character buffer
  (with-temp-buffer
    (insert "X")
    (goto-char (point-min))
    (push (list 'single-min (point) (bobp) (eobp) (bolp) (eolp)) results)
    (goto-char (point-max))
    (push (list 'single-max (point) (bobp) (eobp) (bolp) (eolp)) results))
  ;; Buffer with only newline
  (with-temp-buffer
    (insert "\n")
    (goto-char (point-min))
    (push (list 'newline-min (point) (bobp) (eobp) (bolp) (eolp)) results)
    (goto-char (point-max))
    (push (list 'newline-max (point) (bobp) (eobp) (bolp) (eolp)) results))
  ;; Buffer with only newlines
  (with-temp-buffer
    (insert "\n\n\n")
    (goto-char 2)
    (push (list 'mid-newlines (point) (bobp) (eobp) (bolp) (eolp)
                (line-beginning-position) (line-end-position))
          results))
  ;; Very long single line
  (with-temp-buffer
    (insert (make-string 1000 ?A))
    (goto-char 500)
    (push (list 'long-line (point) (line-beginning-position) (line-end-position)
                (= (line-beginning-position) (point-min))
                (= (line-end-position) (point-max)))
          results))
  (nreverse results))"#;
    let expect = expect_test::expect![
        r#""OK ((single-min 1 t nil t nil) (single-max 2 nil t nil t) (newline-min 1 t nil t t) (newline-max 2 nil t t t) (mid-newlines 2 nil nil t t 2 2) (long-line 500 1 1001 t t))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

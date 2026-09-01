//! Oracle parity tests for `count-lines` with complex patterns:
//! START/END arguments, empty buffer, trailing newline differences,
//! narrowing interactions, multi-line accuracy, line-based statistics,
//! and paragraph counting using count-lines + search.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// count-lines with START and END arguments across diverse ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_patterns_start_end_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test count-lines with various START/END combinations including
    // zero-width ranges, single-char ranges, ranges spanning multiple
    // newlines, and reversed argument order.
    let form = r#"(with-temp-buffer
  (insert "first\nsecond\nthird\nfourth\nfifth\nsixth\n")
  (let ((results nil))
    ;; Full buffer range
    (push (count-lines (point-min) (point-max)) results)
    ;; Zero-width range at various positions
    (push (count-lines 1 1) results)
    (push (count-lines 6 6) results)
    (push (count-lines (point-max) (point-max)) results)
    ;; Single character range (no newline)
    (push (count-lines 1 2) results)
    ;; Single character range (exactly a newline)
    (push (count-lines 6 7) results)
    ;; Range within a single line (no newlines crossed)
    (push (count-lines 8 12) results)
    ;; Range spanning exactly one newline
    (push (count-lines 4 8) results)
    ;; Range spanning three newlines
    (push (count-lines 1 20) results)
    ;; Range from middle of one line to middle of another
    (push (count-lines 3 15) results)
    ;; Reversed arguments (count-lines handles both orders)
    (push (count-lines 20 1) results)
    (push (count-lines 15 3) results)
    ;; Range starting at a newline character
    (push (count-lines 6 13) results)
    ;; Range ending at a newline character
    (push (count-lines 1 6) results)
    (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (6 0 0 0 1 1 1 2 3 3 3 3 2 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines on empty buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_patterns_empty_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Empty buffer: point-min = point-max = 1, count-lines should be 0.
    // Also verify after inserting and erasing content.
    let form = r#"(with-temp-buffer
  (let ((results nil))
    ;; Initially empty
    (push (count-lines (point-min) (point-max)) results)
    (push (= (point-min) (point-max)) results)
    ;; Insert then erase: should be back to empty
    (insert "temporary\ncontent\nhere\n")
    (push (count-lines (point-min) (point-max)) results)
    (erase-buffer)
    (push (count-lines (point-min) (point-max)) results)
    (push (= (point-min) (point-max)) results)
    ;; Insert only whitespace, erase again
    (insert "   ")
    (push (count-lines (point-min) (point-max)) results)
    (erase-buffer)
    (push (count-lines (point-min) (point-max)) results)
    (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (0 t 3 0 t 1 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines with trailing newline vs without
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_patterns_trailing_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Carefully compare count-lines behavior when content ends with
    // newline vs not, and with varying numbers of trailing newlines.
    let form = r#"(let ((results nil))
  ;; "a\nb\nc" (no trailing newline) vs "a\nb\nc\n" (trailing newline)
  (with-temp-buffer
    (insert "a\nb\nc")
    (push (list :no-trailing (count-lines (point-min) (point-max))
                (buffer-size)) results))
  (with-temp-buffer
    (insert "a\nb\nc\n")
    (push (list :with-trailing (count-lines (point-min) (point-max))
                (buffer-size)) results))
  ;; Single line without newline vs with newline
  (with-temp-buffer
    (insert "hello")
    (push (list :single-no-nl (count-lines (point-min) (point-max))) results))
  (with-temp-buffer
    (insert "hello\n")
    (push (list :single-with-nl (count-lines (point-min) (point-max))) results))
  ;; Multiple trailing newlines
  (with-temp-buffer
    (insert "line1\nline2\n\n\n")
    (push (list :multi-trailing (count-lines (point-min) (point-max))) results))
  ;; Only newlines
  (with-temp-buffer
    (insert "\n")
    (push (list :one-nl (count-lines (point-min) (point-max))) results))
  (with-temp-buffer
    (insert "\n\n\n\n\n")
    (push (list :five-nl (count-lines (point-min) (point-max))) results))
  ;; Empty lines interspersed
  (with-temp-buffer
    (insert "a\n\nb\n\nc\n")
    (push (list :empty-between (count-lines (point-min) (point-max))) results))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((:no-trailing 3 5) (:with-trailing 3 6) (:single-no-nl 1) (:single-with-nl 1) (:multi-trailing 4) (:one-nl 1) (:five-nl 5) (:empty-between 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines with narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_patterns_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test count-lines in narrowed regions and verify it counts
    // correctly within the restriction.
    let form = r#"(with-temp-buffer
  (dotimes (i 15)
    (insert (format "line-%02d: some content here\n" (1+ i))))
  (let ((results nil)
        (total (count-lines (point-min) (point-max))))
    (push (list :total total) results)
    ;; Narrow to lines 3-7
    (save-restriction
      (goto-char (point-min))
      (forward-line 2)
      (let ((start (point)))
        (forward-line 5)
        (narrow-to-region start (point))
        (push (list :narrow-3-to-7
                    (count-lines (point-min) (point-max))
                    (point-min) (point-max)) results)))
    ;; Narrow to single line
    (save-restriction
      (goto-char (point-min))
      (forward-line 5)
      (let ((start (point)))
        (forward-line 1)
        (narrow-to-region start (point))
        (push (list :narrow-single-line
                    (count-lines (point-min) (point-max))) results)))
    ;; Narrow to last 3 lines
    (save-restriction
      (goto-char (point-max))
      (forward-line -3)
      (let ((start (point)))
        (narrow-to-region start (point-max))
        (push (list :narrow-last-3
                    (count-lines (point-min) (point-max))) results)))
    ;; Narrow to empty region (same point)
    (save-restriction
      (goto-char 10)
      (narrow-to-region 10 10)
      (push (list :narrow-empty
                  (count-lines (point-min) (point-max))) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:total 15) (:narrow-3-to-7 5 55 190) (:narrow-single-line 1) (:narrow-last-3 3) (:narrow-empty 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines accuracy on multi-line content with varied line lengths
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_patterns_accuracy_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate content with varied line lengths and verify count-lines
    // at every intermediate position.
    let form = r#"(with-temp-buffer
  ;; Lines: "a", "bb", "ccc", "dddd", "eeeee" (5 lines + trailing newline)
  (insert "a\nbb\nccc\ndddd\neeeee\n")
  (let ((results nil)
        (total-lines (count-lines (point-min) (point-max))))
    (push (list :total total-lines) results)
    ;; Count lines from point-min to every position
    (let ((pos-counts nil))
      (goto-char (point-min))
      (let ((done nil))
        (while (not done)
          (let ((cl (count-lines (point-min) (point))))
            (push (cons (point) cl) pos-counts))
          (if (= (point) (point-max))
              (setq done t)
            (forward-char 1))))
      (push (list :pos-counts (nreverse pos-counts)) results))
    ;; Verify intermediate ranges add up
    ;; count-lines(1,mid) + count-lines(mid,max) should relate to total
    (let ((mid 8))  ;; middle of "ccc"
      (push (list :split-at-mid
                  (count-lines (point-min) mid)
                  (count-lines mid (point-max))
                  (+ (count-lines (point-min) mid)
                     (count-lines mid (point-max)))
                  total-lines) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:total 5) (:pos-counts ((1 . 0) (2 . 1) (3 . 1) (4 . 2) (5 . 2) (6 . 2) (7 . 3) (8 . 3) (9 . 3) (10 . 3) (11 . 4) (12 . 4) (13 . 4) (14 . 4) (15 . 4) (16 . 5) (17 . 5) (18 . 5) (19 . 5) (20 . 5) (21 . 5))) (:split-at-mid 3 3 6 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: line-based statistics (avg length, longest, shortest)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_patterns_line_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compute per-line statistics: line count, average length,
    // longest line, shortest line, total characters.
    let form = r#"(progn
  (fset 'neovm--test-line-stats
    (lambda ()
      "Compute line statistics for the current buffer."
      (let ((line-lengths nil)
            (line-count 0))
        (goto-char (point-min))
        (while (not (eobp))
          (let ((bol (line-beginning-position))
                (eol (line-end-position)))
            (push (- eol bol) line-lengths)
            (setq line-count (1+ line-count))
            (forward-line 1)))
        (setq line-lengths (nreverse line-lengths))
        (if (= line-count 0)
            (list :count 0 :avg 0 :max 0 :min 0 :total 0 :lengths nil)
          (let ((total (apply #'+ line-lengths))
                (mx (apply #'max line-lengths))
                (mn (apply #'min line-lengths)))
            (list :count line-count
                  :avg (/ total line-count)
                  :max mx
                  :min mn
                  :total total
                  :lengths line-lengths))))))
  (unwind-protect
      (let ((results nil))
        ;; Buffer with varied line lengths
        (with-temp-buffer
          (insert "short\nthis is a medium length line\nx\n\na very very very long line indeed\nend\n")
          (push (funcall 'neovm--test-line-stats) results))
        ;; Buffer with uniform lines
        (with-temp-buffer
          (insert "abc\ndef\nghi\njkl\nmno\n")
          (push (funcall 'neovm--test-line-stats) results))
        ;; Buffer with single empty line
        (with-temp-buffer
          (insert "\n")
          (push (funcall 'neovm--test-line-stats) results))
        ;; Buffer with no trailing newline
        (with-temp-buffer
          (insert "only line")
          (push (funcall 'neovm--test-line-stats) results))
        (nreverse results))
    (fmakunbound 'neovm--test-line-stats)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:count 6 :avg 11 :max 33 :min 0 :total 70 :lengths (5 28 1 0 33 3)) (:count 5 :avg 3 :max 3 :min 3 :total 15 :lengths (3 3 3 3 3)) (:count 1 :avg 0 :max 0 :min 0 :total 0 :lengths (0)) (:count 1 :avg 9 :max 9 :min 9 :total 9 :lengths (9)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: paragraph counting using count-lines + search
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_patterns_paragraph_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count paragraphs (separated by blank lines) and compute
    // per-paragraph line counts using count-lines.
    let form = r#"(progn
  (fset 'neovm--test-count-paragraphs
    (lambda ()
      "Count paragraphs and lines-per-paragraph in current buffer."
      (let ((paragraphs nil)
            (para-start nil)
            (para-num 0))
        (goto-char (point-min))
        ;; Skip leading blank lines
        (while (and (not (eobp))
                    (looking-at "^[ \t]*$"))
          (forward-line 1))
        (when (not (eobp))
          (setq para-start (point)))
        (while (not (eobp))
          (cond
           ;; Blank line: end current paragraph
           ((looking-at "^[ \t]*$")
            (when para-start
              (setq para-num (1+ para-num))
              (push (list para-num
                          (count-lines para-start (point))
                          (buffer-substring-no-properties
                           para-start (min (+ para-start 20) (point))))
                    paragraphs)
              (setq para-start nil))
            (forward-line 1)
            ;; Skip consecutive blank lines
            (while (and (not (eobp)) (looking-at "^[ \t]*$"))
              (forward-line 1))
            (when (not (eobp))
              (setq para-start (point))))
           ;; Non-blank line: continue paragraph
           (t (forward-line 1))))
        ;; Handle final paragraph (no trailing blank line)
        (when para-start
          (setq para-num (1+ para-num))
          (push (list para-num
                      (count-lines para-start (point))
                      (buffer-substring-no-properties
                       para-start (min (+ para-start 20) (point))))
                paragraphs))
        (list :total-paragraphs para-num
              :details (nreverse paragraphs)))))

  (unwind-protect
      (let ((results nil))
        ;; Multiple paragraphs separated by blank lines
        (with-temp-buffer
          (insert "First paragraph line 1.\nFirst paragraph line 2.\n\n")
          (insert "Second paragraph.\n\n")
          (insert "Third paragraph line 1.\nThird paragraph line 2.\nThird paragraph line 3.\n")
          (push (funcall 'neovm--test-count-paragraphs) results))
        ;; Single paragraph (no blank lines)
        (with-temp-buffer
          (insert "This is one\ncontinuous\nparagraph\n")
          (push (funcall 'neovm--test-count-paragraphs) results))
        ;; Paragraphs with multiple blank line separators
        (with-temp-buffer
          (insert "Para one.\n\n\n\nPara two.\n\n\nPara three.\n")
          (push (funcall 'neovm--test-count-paragraphs) results))
        ;; Leading and trailing blank lines
        (with-temp-buffer
          (insert "\n\n\nContent here.\nMore content.\n\n\n")
          (push (funcall 'neovm--test-count-paragraphs) results))
        (nreverse results))
    (fmakunbound 'neovm--test-count-paragraphs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:total-paragraphs 3 :details ((1 2 \"First paragraph line\") (2 1 \"Second paragraph.\\n\") (3 3 \"Third paragraph line\"))) (:total-paragraphs 1 :details ((1 3 \"This is one\\ncontinuo\"))) (:total-paragraphs 3 :details ((1 1 \"Para one.\\n\") (2 1 \"Para two.\\n\") (3 1 \"Para three.\\n\"))) (:total-paragraphs 1 :details ((1 2 \"Content here.\\nMore c\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines with programmatic content and cross-validation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_patterns_cross_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Generate content programmatically, then cross-validate count-lines
    // against line-number-at-pos and forward-line navigation.
    let form = r#"(with-temp-buffer
  ;; Generate 20 lines of varying content
  (dotimes (i 20)
    (insert (make-string (1+ (% (* (1+ i) 7) 15)) (+ ?A (% i 26))) "\n"))
  (let ((results nil)
        (total-cl (count-lines (point-min) (point-max))))
    ;; Cross-validate: for each line, count-lines from start should match
    ;; (line-number-at-pos - 1) and forward-line count
    (goto-char (point-min))
    (let ((line-idx 0)
          (checks nil))
      (while (not (eobp))
        (let ((cl (count-lines (point-min) (point)))
              (lnap (line-number-at-pos (point))))
          (push (list :line line-idx
                      :count-lines cl
                      :line-number-at-pos lnap
                      :match (= cl (1- lnap)))
                checks))
        (setq line-idx (1+ line-idx))
        (forward-line 1))
      (push (list :total-lines total-cl
                  :line-checks (length checks)
                  :all-match (let ((ok t))
                               (dolist (c checks ok)
                                 (unless (plist-get c :match)
                                   (setq ok nil)))))
            results))
    ;; Also verify count-lines is additive across consecutive ranges
    (let ((ranges nil)
          (sum 0))
      (goto-char (point-min))
      (let ((prev (point)))
        (dotimes (_ 5)
          (forward-line 4)
          (let ((cl (count-lines prev (point))))
            (push cl ranges)
            (setq sum (+ sum cl))
            (setq prev (point)))))
      (push (list :partial-sums (nreverse ranges)
                  :sum sum
                  :total total-cl)
            results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:total-lines 20 :line-checks 20 :all-match t) (:partial-sums (4 4 4 4 4) :sum 20 :total 20))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

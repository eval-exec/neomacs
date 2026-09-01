//! Comprehensive oracle parity tests for narrowing and widening operations.
//!
//! Tests narrow-to-region with various bounds, widen restoring full buffer,
//! point-min/point-max in narrowed buffers, nested save-restriction with
//! multiple narrows, search operations within narrowed regions, insert/delete
//! within narrowed regions, line-number-at-pos in narrowed buffers, and
//! buffer-substring interaction with narrowing.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// narrow-to-region with boundary edge cases and point-min/point-max
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_boundary_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r###"(with-temp-buffer
  (insert "0123456789ABCDEF")
  ;; Buffer is 16 chars, positions 1..17
  (let ((results nil))
    ;; Case 1: narrow to full buffer (no-op)
    (save-restriction
      (narrow-to-region 1 17)
      (setq results (cons (list 'full (point-min) (point-max)
                                (buffer-string) (buffer-size))
                           results)))
    ;; Case 2: narrow to single char
    (save-restriction
      (narrow-to-region 5 6)
      (setq results (cons (list 'single-char (point-min) (point-max)
                                (buffer-string) (buffer-size))
                           results)))
    ;; Case 3: narrow to empty region (start = end)
    (save-restriction
      (narrow-to-region 8 8)
      (setq results (cons (list 'empty (point-min) (point-max)
                                (buffer-string) (buffer-size)
                                (= (point-min) (point-max)))
                           results)))
    ;; Case 4: narrow with reversed args (start > end) -- Emacs swaps them
    (save-restriction
      (narrow-to-region 10 5)
      (setq results (cons (list 'reversed (point-min) (point-max)
                                (buffer-string))
                           results)))
    ;; Case 5: narrow to beginning
    (save-restriction
      (narrow-to-region 1 4)
      (setq results (cons (list 'beginning (point-min) (point-max)
                                (buffer-string))
                           results)))
    ;; Case 6: narrow to end
    (save-restriction
      (narrow-to-region 14 17)
      (setq results (cons (list 'end (point-min) (point-max)
                                (buffer-string))
                           results)))
    (nreverse results)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// widen restoring full buffer after multiple narrowing operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_restore_full_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "The quick brown fox jumps over the lazy dog")
  (let ((full-text (buffer-string))
        (full-min (point-min))
        (full-max (point-max))
        (results nil))
    ;; Narrow, modify, widen, check full buffer
    (save-restriction
      (narrow-to-region 5 15)
      (setq results (cons (list 'narrowed (buffer-string)) results))
      ;; Widen explicitly
      (widen)
      (setq results (cons (list 'widened
                                (= (point-min) full-min)
                                (= (point-max) full-max)
                                (string= (buffer-string) full-text))
                           results)))
    ;; Multiple narrows and widens in sequence
    (narrow-to-region 1 10)
    (let ((s1 (buffer-string)))
      (widen)
      (narrow-to-region 20 30)
      (let ((s2 (buffer-string)))
        (widen)
        (narrow-to-region 35 44)
        (let ((s3 (buffer-string)))
          (widen)
          (setq results (cons (list 'sequential s1 s2 s3
                                    (string= (buffer-string) full-text))
                               results)))))
    (nreverse results)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Nested save-restriction: multiple levels of narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_triple_nested_save_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "AABBCCDDEEFFFFGGHH")
  ;; 18 chars, positions 1..19
  (let ((results nil))
    ;; Level 0: full buffer
    (setq results (cons (list 'level0 (point-min) (point-max) (buffer-string)) results))
    (save-restriction
      ;; Level 1: narrow to 3..17 = "BBCCDDEEFFFFGG"
      (narrow-to-region 3 17)
      (setq results (cons (list 'level1 (point-min) (point-max) (buffer-string)) results))
      (save-restriction
        ;; Level 2: narrow to 5..13 within level1 = "CCDDEEFF"
        (narrow-to-region 5 13)
        (setq results (cons (list 'level2 (point-min) (point-max) (buffer-string)) results))
        (save-restriction
          ;; Level 3: narrow to 7..11 within level2 = "DDEE"
          (narrow-to-region 7 11)
          (setq results (cons (list 'level3 (point-min) (point-max) (buffer-string)) results))
          ;; Widen at level 3 goes back to level 2 restriction
          (widen)
          (setq results (cons (list 'level3-widened (point-min) (point-max) (buffer-string)) results)))
        ;; After level 3 save-restriction, back to level 2
        (setq results (cons (list 'back-to-level2 (point-min) (point-max) (buffer-string)) results)))
      ;; After level 2 save-restriction, back to level 1
      (setq results (cons (list 'back-to-level1 (point-min) (point-max) (buffer-string)) results)))
    ;; After level 1 save-restriction, back to full buffer
    (setq results (cons (list 'back-to-level0 (point-min) (point-max) (buffer-string)) results))
    (nreverse results)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Search operations confined within narrowed region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_search_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "cat dog cat bird cat fish cat")
  (let ((results nil))
    ;; Count "cat" in full buffer
    (goto-char (point-min))
    (let ((full-count 0))
      (while (search-forward "cat" nil t)
        (setq full-count (1+ full-count)))
      (setq results (cons (list 'full-count full-count) results)))
    ;; Count "cat" in narrowed region (should find fewer)
    (save-restriction
      (narrow-to-region 5 20)
      (goto-char (point-min))
      (let ((narrow-count 0))
        (while (search-forward "cat" nil t)
          (setq narrow-count (1+ narrow-count)))
        (setq results (cons (list 'narrow-region (buffer-string)
                                  'narrow-count narrow-count)
                             results))))
    ;; re-search-forward in narrowed region
    (save-restriction
      (narrow-to-region 1 15)
      (goto-char (point-min))
      (let ((matches nil))
        (while (re-search-forward "\\b[a-z]+\\b" nil t)
          (setq matches (cons (match-string 0) matches)))
        (setq results (cons (list 'regex-narrow (nreverse matches)) results))))
    ;; search-backward in narrowed region
    (save-restriction
      (narrow-to-region 10 29)
      (goto-char (point-max))
      (let ((found (search-backward "cat" nil t)))
        (setq results (cons (list 'search-backward
                                  (buffer-string)
                                  found
                                  (when found (point)))
                             results))))
    (nreverse results)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Insert and delete within narrowed region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_insert_delete_within() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "XXXXX-YYYYY-ZZZZZ")
  ;; 18 chars, positions 1..19
  (let ((results nil))
    ;; Insert within narrowed region expands the narrowed area
    (save-restriction
      (narrow-to-region 7 12)  ;; "YYYYY"
      (setq results (cons (list 'before-insert (buffer-string)
                                (point-min) (point-max))
                           results))
      (goto-char (point-min))
      (insert ">>")
      (setq results (cons (list 'after-insert (buffer-string)
                                (point-min) (point-max))
                           results)))
    ;; Verify full buffer shows the insertion
    (setq results (cons (list 'full-after-insert (buffer-string)) results))
    ;; Delete within narrowed region shrinks it
    (save-restriction
      (narrow-to-region 7 14)  ;; ">>YYYYY"
      (setq results (cons (list 'before-delete (buffer-string)
                                (point-min) (point-max))
                           results))
      (goto-char (point-min))
      (delete-char 2)  ;; remove ">>"
      (setq results (cons (list 'after-delete (buffer-string)
                                (point-min) (point-max))
                           results)))
    ;; Full buffer after delete
    (setq results (cons (list 'full-after-delete (buffer-string)) results))
    ;; Replace within narrowed region
    (save-restriction
      (narrow-to-region 7 12)  ;; "YYYYY"
      (goto-char (point-min))
      (while (re-search-forward "Y" nil t)
        (replace-match "W"))
      (setq results (cons (list 'after-replace (buffer-string)) results)))
    (setq results (cons (list 'full-after-replace (buffer-string)) results))
    (nreverse results)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// line-number-at-pos in narrowed buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_line_number_at_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "Line1\nLine2\nLine3\nLine4\nLine5\nLine6\nLine7\n")
  (let ((results nil))
    ;; Full buffer line numbers
    (goto-char (point-min))
    (setq results (cons (list 'full-line1 (line-number-at-pos)) results))
    (forward-line 3)
    (setq results (cons (list 'full-line4 (line-number-at-pos)) results))
    (goto-char (point-max))
    (setq results (cons (list 'full-end (line-number-at-pos)) results))
    ;; Narrowed: lines 3-5 (Line3\nLine4\nLine5\n)
    ;; Line3 starts at position 13, Line6 starts at position 31
    (save-restriction
      (goto-char 13)
      (let ((start (point)))
        (forward-line 3)
        (narrow-to-region start (point)))
      (goto-char (point-min))
      ;; line-number-at-pos with no ABSOLUTE arg returns line within narrow
      (setq results (cons (list 'narrow-first-line
                                (line-number-at-pos)
                                (buffer-string))
                           results))
      (forward-line 1)
      (setq results (cons (list 'narrow-second-line (line-number-at-pos)) results))
      (forward-line 1)
      (setq results (cons (list 'narrow-third-line (line-number-at-pos)) results))
      (goto-char (point-max))
      (setq results (cons (list 'narrow-at-max (line-number-at-pos)) results)))
    (nreverse results)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// buffer-substring interaction with narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_buffer_substring_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "abcdefghijklmnopqrstuvwxyz")
  ;; 26 chars, positions 1..27
  (let ((results nil))
    ;; buffer-substring in full buffer
    (setq results (cons (list 'full
                              (buffer-substring 1 6)
                              (buffer-substring 22 27))
                         results))
    ;; buffer-substring within narrowed region
    (save-restriction
      (narrow-to-region 5 15)  ;; "efghijklmn"
      ;; Positions within narrow use absolute positions
      (setq results (cons (list 'narrow-content (buffer-string)) results))
      (setq results (cons (list 'substr-within
                                (buffer-substring 5 8)   ;; "efg"
                                (buffer-substring 12 15)) ;; "lmn"
                           results))
      ;; buffer-substring-no-properties
      (setq results (cons (list 'no-props
                                (buffer-substring-no-properties 5 10))
                           results))
      ;; point-min and point-max as boundaries
      (setq results (cons (list 'full-narrow
                                (buffer-substring (point-min) (point-max)))
                           results)))
    ;; After widening, same positions give different context
    (setq results (cons (list 'after-widen
                              (buffer-substring 5 15))
                         results))
    ;; Narrowing doesn't affect what buffer-substring returns for valid positions
    (save-restriction
      (narrow-to-region 10 20)  ;; "jklmnopqrs"
      ;; Can't access positions outside narrowed region via buffer-substring
      ;; (would signal args-out-of-range)
      ;; But within-range access works fine
      (setq results (cons (list 'narrow2
                                (buffer-substring 10 15)  ;; "jklmn"
                                (buffer-substring 15 20)) ;; "opqrs"
                           results)))
    (nreverse results)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Complex: accumulate statistics from narrowed sections
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_accumulate_section_stats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "## Section A\nword1 word2 word3\nfoo bar\n")
  (insert "## Section B\nhello world\ntest data more words here\n")
  (insert "## Section C\na b c d e f g h i j\n")
  (goto-char (point-min))
  ;; Find section boundaries
  (let ((sections nil)
        (boundaries nil))
    ;; Collect header positions
    (while (re-search-forward "^## \\(.*\\)$" nil t)
      (setq boundaries
            (cons (cons (match-string 1) (1+ (match-end 0)))
                  boundaries)))
    (setq boundaries (nreverse boundaries))
    ;; Process each section
    (let ((i 0))
      (while (< i (length boundaries))
        (let* ((entry (nth i boundaries))
               (name (car entry))
               (start (cdr entry))
               (end (if (< (1+ i) (length boundaries))
                        ;; Find the position just before next header
                        (save-excursion
                          (goto-char (cdr (nth (1+ i) boundaries)))
                          (forward-line -1)
                          (line-beginning-position))
                      (point-max))))
          (save-restriction
            (narrow-to-region start end)
            (goto-char (point-min))
            ;; Count words (sequences of non-space)
            (let ((word-count 0)
                  (line-count 0)
                  (char-count (- (point-max) (point-min))))
              (while (re-search-forward "\\b[a-zA-Z0-9]+\\b" nil t)
                (setq word-count (1+ word-count)))
              (goto-char (point-min))
              (while (not (eobp))
                (setq line-count (1+ line-count))
                (forward-line 1))
              (setq sections
                    (cons (list name
                                (cons 'words word-count)
                                (cons 'lines line-count)
                                (cons 'chars char-count))
                          sections)))))
        (setq i (1+ i))))
    (nreverse sections)))"###;
    let expect = expect_test::expect![[
        r#""OK ((\"Section A\" (words . 5) (lines . 2) (chars . 26)) (\"Section B\" (words . 7) (lines . 2) (chars . 38)) (\"Section C\" (words . 10) (lines . 1) (chars . 20)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

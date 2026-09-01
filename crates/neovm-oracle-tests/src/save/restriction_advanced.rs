//! Oracle parity tests for advanced save-restriction/narrowing:
//! narrow-to-region + widen cycles, nested save-restriction with different
//! ranges, buffer operations within narrowed regions, markers under narrowing,
//! section-by-section processing, and multi-region accumulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// narrow-to-region + widen cycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restr_adv_narrow_widen_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple narrow/widen cycles, verifying state at each step
    let form = r#"(with-temp-buffer
                    (insert "ABCDEFGHIJKLMNOPQRST")
                    (let ((results nil))
                      ;; Full buffer
                      (setq results (cons (list (point-min) (point-max) (buffer-string)) results))
                      ;; Narrow to 3-8
                      (narrow-to-region 3 8)
                      (setq results (cons (list (point-min) (point-max) (buffer-string)) results))
                      ;; Widen
                      (widen)
                      (setq results (cons (list (point-min) (point-max) (buffer-string)) results))
                      ;; Narrow to 10-15
                      (narrow-to-region 10 15)
                      (setq results (cons (list (point-min) (point-max) (buffer-string)) results))
                      ;; Widen again
                      (widen)
                      (setq results (cons (list (point-min) (point-max) (buffer-string)) results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 21 \"ABCDEFGHIJKLMNOPQRST\") (3 8 \"CDEFG\") (1 21 \"ABCDEFGHIJKLMNOPQRST\") (10 15 \"JKLMN\") (1 21 \"ABCDEFGHIJKLMNOPQRST\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-restriction preserves narrow state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restr_adv_preserves_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-restriction restores narrowing even after widen inside
    let form = r#"(with-temp-buffer
                    (insert "0123456789ABCDEF")
                    (narrow-to-region 3 10)
                    (let ((before-str (buffer-string))
                          (inner-full nil)
                          (inner-narrow nil))
                      (save-restriction
                        (widen)
                        (setq inner-full (buffer-string))
                        (narrow-to-region 5 12)
                        (setq inner-narrow (buffer-string)))
                      ;; After save-restriction: should be back to 3-10
                      (list before-str
                            inner-full
                            inner-narrow
                            (buffer-string)
                            (point-min)
                            (point-max))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"2345678\" \"0123456789ABCDEF\" \"456789A\" \"2345678\" 3 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested save-restriction with different narrow ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restr_adv_deeply_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of nested save-restriction with different ranges
    let form = r#"(with-temp-buffer
                    (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
                    (let ((results nil))
                      (save-restriction
                        (narrow-to-region 1 20)
                        (setq results (cons (buffer-string) results))
                        (save-restriction
                          (narrow-to-region 5 15)
                          (setq results (cons (buffer-string) results))
                          (save-restriction
                            (narrow-to-region 3 8)
                            ;; 3-8 within 5-15 within 1-20
                            (setq results (cons (buffer-string) results))
                            ;; Widen only undoes innermost narrow (back to 5-15)
                            (widen)
                            (setq results (cons (buffer-string) results)))
                          ;; Back to 5-15
                          (setq results (cons (buffer-string) results)))
                        ;; Back to 1-20
                        (setq results (cons (buffer-string) results)))
                      ;; Back to full buffer
                      (setq results (cons (buffer-string) results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"ABCDEFGHIJKLMNOPQRS\" \"EFGHIJKLMN\" \"CDEFG\" \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\" \"EFGHIJKLMN\" \"ABCDEFGHIJKLMNOPQRS\" \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer operations within narrowed region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restr_adv_insert_delete_in_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert and delete within narrowed region, then verify full buffer
    let form = r#"(with-temp-buffer
                    (insert "Hello, World! Goodbye, World!")
                    (save-restriction
                      (narrow-to-region 8 14)
                      ;; Narrowed view: "World!"
                      (let ((narrow-before (buffer-string)))
                        (goto-char (point-min))
                        (insert "Beautiful ")
                        (let ((narrow-after-insert (buffer-string)))
                          ;; Delete "World"
                          (goto-char (+ (point-min) 10))
                          (delete-char 5)
                          (let ((narrow-after-delete (buffer-string)))
                            (list narrow-before
                                  narrow-after-insert
                                  narrow-after-delete)))))
                    ;; Full buffer after widen
                    (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"Hello, Beautiful ! Goodbye, World!\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_save_restr_adv_search_in_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Searches confined to narrowed region, replace within narrow
    let form = r#"(with-temp-buffer
                    (insert "foo bar foo bar foo bar")
                    (save-restriction
                      (narrow-to-region 5 19)
                      ;; Visible: "bar foo bar fo"
                      (let ((count 0))
                        (goto-char (point-min))
                        (while (search-forward "bar" nil t)
                          (setq count (1+ count)))
                        ;; Now replace "foo" with "XXX" in narrowed region
                        (goto-char (point-min))
                        (while (search-forward "foo" nil t)
                          (replace-match "XXX"))
                        (list count (buffer-string))))
                    ;; Full buffer: only the narrowed region was modified
                    (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"foo bar XXX bar foo bar\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// point-min/point-max under narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restr_adv_point_bounds_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track how point-min/point-max change through a series of operations
    let form = r#"(with-temp-buffer
                    (insert "0123456789ABCDEFGHIJ")
                    (let ((r nil))
                      ;; Full buffer
                      (setq r (cons (list 'full (point-min) (point-max) (- (point-max) (point-min))) r))
                      ;; Narrow to 5-15
                      (narrow-to-region 5 15)
                      (setq r (cons (list 'narrow1 (point-min) (point-max) (- (point-max) (point-min))) r))
                      ;; Insert expands the narrowed region
                      (goto-char (point-max))
                      (insert "XYZ")
                      (setq r (cons (list 'after-insert (point-min) (point-max) (- (point-max) (point-min))) r))
                      ;; Delete shrinks it
                      (goto-char (point-min))
                      (delete-char 3)
                      (setq r (cons (list 'after-delete (point-min) (point-max) (- (point-max) (point-min))) r))
                      ;; Widen: see everything
                      (widen)
                      (setq r (cons (list 'widened (point-min) (point-max) (buffer-string)) r))
                      (nreverse r)))"#;
    let expect = expect_test::expect![[
        r#""OK ((full 1 21 20) (narrow1 5 15 10) (after-insert 5 18 13) (after-delete 5 15 10) (widened 1 21 \"0123789ABCDXYZEFGHIJ\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// narrow-to-region with markers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restr_adv_markers_under_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Markers survive narrowing and track insertions
    let form = r#"(with-temp-buffer
                    (insert "ABCDEFGHIJKLMNOP")
                    (let ((m1 (make-marker))
                          (m2 (make-marker)))
                      (set-marker m1 5)
                      (set-marker m2 12)
                      (narrow-to-region 3 14)
                      ;; Markers are still at their absolute positions
                      (let ((m1-in-narrow (marker-position m1))
                            (m2-in-narrow (marker-position m2)))
                        ;; Insert at marker position
                        (goto-char m1-in-narrow)
                        (insert "***")
                        (let ((m1-after (marker-position m1))
                              (m2-after (marker-position m2))
                              (narrow-str (buffer-string)))
                          (widen)
                          (list m1-in-narrow m2-in-narrow
                                m1-after m2-after
                                narrow-str
                                (buffer-string))))))"#;
    let expect =
        expect_test::expect![[r#""OK (5 12 5 15 \"CD***EFGHIJKLM\" \"ABCD***EFGHIJKLMNOP\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: section-by-section processing with narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restr_adv_section_processing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Process a structured document section by section using narrowing
    let form = r#"(with-temp-buffer
                    (insert "[HEADER]\ntitle=My Doc\nauthor=Test\n")
                    (insert "[BODY]\nLine one of body.\nLine two of body.\nLine three.\n")
                    (insert "[FOOTER]\ncopyright 2026\n")
                    (goto-char (point-min))
                    (let ((sections nil))
                      ;; Find each [SECTION] and process its content
                      (while (re-search-forward "^\\[\\([A-Z]+\\)\\]$" nil t)
                        (let ((name (match-string 1))
                              (content-start (1+ (point))))
                          ;; Find end: next section or end of buffer
                          (let ((content-end
                                 (if (re-search-forward "^\\[" nil t)
                                     (progn (goto-char (match-beginning 0))
                                            (line-beginning-position))
                                   (point-max))))
                            (save-restriction
                              (narrow-to-region content-start content-end)
                              (goto-char (point-min))
                              (let ((line-count 0)
                                    (char-count (- (point-max) (point-min))))
                                (while (not (eobp))
                                  (unless (= (line-beginning-position) (line-end-position))
                                    (setq line-count (1+ line-count)))
                                  (forward-line 1))
                                (setq sections
                                      (cons (list name line-count char-count)
                                            sections)))))))
                      (nreverse sections)))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"HEADER\" 2 25) (\"BODY\" 3 48) (\"FOOTER\" 1 15))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: accumulate results from multiple narrowed regions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_restr_adv_accumulate_from_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract and transform data from specific column ranges of a table
    let form = r#"(with-temp-buffer
                    (insert "Alice     85  A\n")
                    (insert "Bob       92  A+\n")
                    (insert "Carol     78  B+\n")
                    (insert "Dave      95  A+\n")
                    (insert "Eve       88  A\n")
                    (goto-char (point-min))
                    (let ((names nil) (scores nil) (total 0) (count 0))
                      (while (not (eobp))
                        (let ((bol (line-beginning-position))
                              (eol (line-end-position)))
                          (when (> eol bol)
                            ;; Extract name (columns 1-10)
                            (save-restriction
                              (narrow-to-region bol (min (+ bol 10) eol))
                              (let ((name (string-trim (buffer-string))))
                                (setq names (cons name names))))
                            ;; Extract score (columns 11-14)
                            (save-restriction
                              (narrow-to-region (+ bol 10) (min (+ bol 14) eol))
                              (let ((score-str (string-trim (buffer-string))))
                                (let ((score (string-to-number score-str)))
                                  (setq scores (cons score scores))
                                  (setq total (+ total score))
                                  (setq count (1+ count)))))))
                        (forward-line 1))
                      (list (nreverse names)
                            (nreverse scores)
                            total
                            (/ total count))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Alice\" \"Bob\" \"Carol\" \"Dave\" \"Eve\") (85 92 78 95 88) 438 87)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Advanced oracle parity tests for column and line movement operations.
//!
//! Tests move-to-column with FORCE parameter, current-column with tabs
//! and multibyte characters, count-lines across varied content,
//! line-number-at-pos with ABSOLUTE parameter, beginning-of-line/end-of-line
//! with counts, column-based text alignment, and rectangular region extraction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// move-to-column with FORCE parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_move_to_column_force_tab_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // FORCE non-nil: when target column falls inside a tab, the tab is
    // replaced with spaces to reach the exact column
    let form = r#"(with-temp-buffer
                    (insert "\thello\n")
                    (goto-char (point-min))
                    ;; Without force: move to nearest column (tab-width boundary)
                    (let ((r1 (move-to-column 3)))
                      (let ((c1 (current-column))
                            (p1 (point)))
                        (goto-char (point-min))
                        ;; With force: tab should be split to reach exact column
                        (let ((r2 (move-to-column 3 t)))
                          (list r1 c1 p1
                                r2 (current-column) (point)
                                (buffer-string))))))"#;
    let expect = expect_test::expect![[r#""OK (8 8 2 3 3 4 \"   \thello\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_move_to_column_force_past_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // FORCE non-nil and column past end of line: pads with spaces
    let form = r#"(with-temp-buffer
                    (insert "abc\ndef\n")
                    (goto-char (point-min))
                    (let ((r (move-to-column 10 t)))
                      (list r (current-column) (point)
                            (buffer-string))))"#;
    let expect = expect_test::expect![[r#""OK (10 10 7 \"abc\t  \\ndef\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// current-column with tabs and multibyte characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_current_column_tab_stops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tabs expand to tab-width (default 8)
    let form = r#"(with-temp-buffer
                    (insert "\t\thello\n")
                    (insert "a\tb\n")
                    (insert "\t\t\tX\n")
                    (let ((results nil))
                      ;; After two tabs: column 16
                      (goto-char (point-min))
                      (goto-char 3) ;; after two tabs
                      (setq results (cons (current-column) results))
                      ;; "a\tb" - 'b' is at column 8
                      (goto-char (+ (point-min) (length "\t\thello\n")))
                      (forward-char 2) ;; skip 'a' and tab
                      (setq results (cons (current-column) results))
                      ;; Three tabs then X: column 24
                      (goto-char (+ (point-min)
                                    (length "\t\thello\n")
                                    (length "a\tb\n")))
                      (forward-char 3) ;; skip three tabs
                      (setq results (cons (current-column) results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (16 8 24)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_current_column_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multibyte chars typically occupy 1 column each in Emacs
    let form = r#"(with-temp-buffer
                    (insert "AB\n")
                    (goto-char (point-min))
                    (let ((results nil))
                      ;; Column at start
                      (setq results (cons (current-column) results))
                      ;; After 'A' (1 column)
                      (forward-char 1)
                      (setq results (cons (current-column) results))
                      ;; After 'AB' (2 columns)
                      (forward-char 1)
                      (setq results (cons (current-column) results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (0 1 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// count-lines across different buffer content
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_count_lines_varied_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    ;; Mix of empty lines, long lines, single-char lines
                    (insert "first\n\n\nfourth\n\nsixth line is longer\n")
                    (list
                      ;; Total lines
                      (count-lines (point-min) (point-max))
                      ;; Lines in first 3 characters
                      (count-lines 1 4)
                      ;; Across empty lines region
                      (count-lines 7 10)
                      ;; Single char range (no newline crossed)
                      (count-lines 1 2)
                      ;; Entire buffer ending with newline
                      (count-lines (point-min) (point-max))
                      ;; Range that starts at a newline
                      (count-lines 6 10)))"#;
    let expect = expect_test::expect![[r#""OK (6 1 3 1 6 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// line-number-at-pos with narrowing context
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_line_number_at_pos_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // line-number-at-pos with no ABSOLUTE arg returns relative to narrowing
    // With ABSOLUTE non-nil, returns absolute line number
    let form = r#"(with-temp-buffer
                    (insert "line1\nline2\nline3\nline4\nline5\n")
                    ;; First, without narrowing
                    (let ((normal (list
                                    (line-number-at-pos 1)
                                    (line-number-at-pos 7)
                                    (line-number-at-pos 13))))
                      ;; Now narrow to lines 2-4 (positions 7-24)
                      (narrow-to-region 7 24)
                      (let ((narrowed (list
                                        (line-number-at-pos (point-min))
                                        (line-number-at-pos (point-max))))
                            (absolute (list
                                        (line-number-at-pos (point-min) t)
                                        (line-number-at-pos (point-max) t))))
                        (widen)
                        (list normal narrowed absolute))))"#;
    let expect = expect_test::expect![[r#""OK ((1 2 3) (1 3) (2 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// beginning-of-line / end-of-line with counts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bol_eol_with_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // beginning-of-line N: N=1 is current, N=0 is previous, N=2 is next
    // end-of-line N: N=1 is current, N=0 is previous, N=2 is next
    let form = r#"(with-temp-buffer
                    (insert "aaa\nbbb\nccc\nddd\neee\n")
                    (goto-char 10) ;; middle of "ccc"
                    (let ((results nil))
                      ;; beginning-of-line with various N
                      (dolist (n '(1 0 -1 2 3))
                        (goto-char 10)
                        (beginning-of-line n)
                        (setq results (cons (point) results)))
                      ;; end-of-line with various N
                      (dolist (n '(1 0 -1 2 3))
                        (goto-char 10)
                        (end-of-line n)
                        (setq results (cons (point) results)))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (9 5 1 13 17 12 8 4 16 20)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: column-based text alignment / formatting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_column_based_text_formatting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a formatted table by padding each field to a target column
    // using move-to-column with FORCE
    let form = r#"(with-temp-buffer
                    (let ((data '(("Alice" 30 "Engineer")
                                  ("Bob" 25 "Designer")
                                  ("Carol" 35 "Manager")
                                  ("Dave" 28 "Analyst"))))
                      ;; Write each row with columns at 0, 12, 18
                      (dolist (row data)
                        (let ((name (car row))
                              (age (number-to-string (cadr row)))
                              (role (caddr row)))
                          (insert name)
                          (move-to-column 12 t)
                          (insert age)
                          (move-to-column 18 t)
                          (insert role)
                          (insert "\n")))
                      ;; Now read back and verify column positions
                      (goto-char (point-min))
                      (let ((col-checks nil))
                        (dotimes (_ 4)
                          (move-to-column 12)
                          (let ((age-col (current-column)))
                            (move-to-column 18)
                            (let ((role-col (current-column)))
                              (setq col-checks
                                    (cons (list age-col role-col)
                                          col-checks))))
                          (forward-line 1))
                        (list (buffer-string)
                              (nreverse col-checks)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice\t    30\t  Engineer\\nBob\t    25\t  Designer\\nCarol\t    35\t  Manager\\nDave\t    28\t  Analyst\\n\" ((12 18) (12 18) (12 18) (12 18)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: rectangular region extraction (column range across lines)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rectangular_region_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract a "rectangle" of text: columns 5-10 from lines 1-4
    let form = r#"(with-temp-buffer
                    (insert "abcdefghijklmno\n")
                    (insert "ABCDEFGHIJKLMNO\n")
                    (insert "0123456789abcde\n")
                    (insert "zzzzzzzzzzzzzzz\n")
                    (let ((rect nil)
                          (start-col 5)
                          (end-col 10))
                      ;; Extract rectangle
                      (goto-char (point-min))
                      (dotimes (_ 4)
                        (let ((line-start (point)))
                          (move-to-column start-col)
                          (let ((col-start-pt (point)))
                            (move-to-column end-col)
                            (let ((col-end-pt (point)))
                              (setq rect
                                    (cons (buffer-substring col-start-pt col-end-pt)
                                          rect)))))
                        (forward-line 1))
                      ;; Also compute width of each extracted piece
                      (let ((widths (mapcar 'length (reverse rect))))
                        (list (nreverse rect) widths))))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"fghij\" \"FGHIJ\" \"56789\" \"zzzzz\") (5 5 5 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

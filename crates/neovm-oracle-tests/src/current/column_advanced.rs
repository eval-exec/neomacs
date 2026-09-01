//! Advanced oracle parity tests for `current-column` and `move-to-column`.
//!
//! Covers: current-column at various positions, with tab characters,
//! move-to-column basic movement, FORCE parameter, past end of line,
//! and column-based text alignment.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// current-column at various positions in a multi-line buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_current_column_various_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk through a buffer and record current-column at every character
    // position on a given line. Verify column increments correctly.
    let form = r#"(with-temp-buffer
                    (insert "abcdefghij\nKLMNOP\nxyz\n")
                    (let ((results nil))
                      ;; Record column at each char of line 1
                      (goto-char (point-min))
                      (let ((line1-cols nil))
                        (dotimes (_ 10)
                          (setq line1-cols (cons (current-column) line1-cols))
                          (forward-char 1))
                        (setq results (cons (nreverse line1-cols) results)))
                      ;; At the newline itself
                      (setq results (cons (current-column) results))
                      ;; Line 2: start
                      (forward-char 1)
                      (let ((line2-start-col (current-column)))
                        (end-of-line)
                        (let ((line2-end-col (current-column)))
                          (setq results (cons (list line2-start-col line2-end-col) results))))
                      ;; Line 3
                      (forward-line 1)
                      (setq results (cons (current-column) results))
                      (end-of-line)
                      (setq results (cons (current-column) results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK ((0 1 2 3 4 5 6 7 8 9) 10 (0 6) 0 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// current-column with tab characters and custom tab-width
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_current_column_tabs_custom_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tabs expand to the next tab stop. With default tab-width=8,
    // a tab at column 0 goes to column 8, at column 1 goes to column 8, etc.
    // Also test with tab-width=4.
    let form = r#"(let ((results nil))
                    ;; Default tab-width (8)
                    (with-temp-buffer
                      (insert "a\tb\tc\n")
                      (insert "\t\tX\n")
                      (insert "1234567\tY\n")
                      (goto-char (point-min))
                      ;; 'a' at col 0
                      (setq results (cons (current-column) results))
                      ;; after tab: col 8
                      (forward-char 2)
                      (setq results (cons (current-column) results))
                      ;; 'b' at col 8, after tab: col 16
                      (forward-char 2)
                      (setq results (cons (current-column) results))
                      ;; Line 2: two tabs then X
                      (forward-line 1)
                      (beginning-of-line)
                      (forward-char 2)
                      (setq results (cons (current-column) results))
                      ;; Line 3: "1234567" (7 chars) then tab -> col 8 then Y
                      (forward-line 1)
                      (beginning-of-line)
                      (forward-char 7)
                      (setq results (cons (current-column) results))
                      (forward-char 1)
                      (setq results (cons (current-column) results)))
                    ;; tab-width = 4
                    (with-temp-buffer
                      (setq tab-width 4)
                      (insert "a\tb\n")
                      (insert "\tX\n")
                      (insert "123\tY\n")
                      (goto-char (point-min))
                      ;; 'a' at col 0, after tab col 4
                      (forward-char 2)
                      (setq results (cons (current-column) results))
                      ;; Line 2: tab then X -> col 4
                      (forward-line 1)
                      (beginning-of-line)
                      (forward-char 1)
                      (setq results (cons (current-column) results))
                      ;; Line 3: "123" (3 chars) then tab -> col 4
                      (forward-line 1)
                      (beginning-of-line)
                      (forward-char 3)
                      (setq results (cons (current-column) results))
                      (forward-char 1)
                      (setq results (cons (current-column) results)))
                    (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (0 8 16 16 7 8 4 4 3 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// move-to-column basic movement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_move_to_column_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // move-to-column returns the column actually reached. Without FORCE,
    // it stops at the closest column without modifying the buffer.
    let form = r#"(with-temp-buffer
                    (insert "0123456789abcdef\n")
                    (insert "short\n")
                    (insert "\t\tindented\n")
                    (let ((results nil))
                      ;; Move to column 5 on line 1
                      (goto-char (point-min))
                      (let ((r (move-to-column 5)))
                        (setq results (cons (list r (current-column) (point)) results)))
                      ;; Move to column 12 on line 1
                      (goto-char (point-min))
                      (let ((r (move-to-column 12)))
                        (setq results (cons (list r (current-column) (point)) results)))
                      ;; Move to column 0 (beginning)
                      (goto-char (point-min))
                      (let ((r (move-to-column 0)))
                        (setq results (cons (list r (current-column) (point)) results)))
                      ;; Move past end of "short" line (without FORCE)
                      (goto-char (point-min))
                      (forward-line 1)
                      (let ((r (move-to-column 20)))
                        (setq results (cons (list r (current-column) (point)) results)))
                      ;; Move to column on tabbed line
                      (goto-char (point-min))
                      (forward-line 2)
                      (let ((r (move-to-column 16)))
                        (setq results (cons (list r (current-column) (point)) results)))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK ((5 5 6) (12 12 13) (0 0 1) (5 5 23) (16 16 26))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// move-to-column with FORCE parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_move_to_column_force_detailed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // With FORCE non-nil:
    // - If column is past EOL, spaces are inserted to reach it
    // - If column falls inside a tab, the tab is split into spaces
    // Verify buffer modification in both cases.
    let form = r#"(let ((results nil))
                    ;; Case 1: FORCE past end of line inserts spaces
                    (with-temp-buffer
                      (insert "abc\ndef\n")
                      (goto-char (point-min))
                      (let ((r (move-to-column 8 t)))
                        (setq results
                              (cons (list 'past-eol r (current-column) (point)
                                          (buffer-string))
                                    results))))
                    ;; Case 2: FORCE into a tab splits it
                    (with-temp-buffer
                      (insert "\thello\n")
                      (goto-char (point-min))
                      (let ((r (move-to-column 3 t)))
                        (setq results
                              (cons (list 'split-tab r (current-column) (point)
                                          (buffer-string))
                                    results))))
                    ;; Case 3: FORCE at exact tab boundary does not split
                    (with-temp-buffer
                      (insert "\thello\n")
                      (goto-char (point-min))
                      (let ((r (move-to-column 8 t)))
                        (setq results
                              (cons (list 'exact-tab r (current-column) (point)
                                          (buffer-string))
                                    results))))
                    ;; Case 4: FORCE on empty line
                    (with-temp-buffer
                      (insert "\n")
                      (goto-char (point-min))
                      (let ((r (move-to-column 5 t)))
                        (setq results
                              (cons (list 'empty-line r (current-column) (point)
                                          (buffer-string))
                                    results))))
                    (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((past-eol 8 8 5 \"abc\t\\ndef\\n\") (split-tab 3 3 4 \"   \thello\\n\") (exact-tab 8 8 2 \"\thello\\n\") (empty-line 5 5 6 \"     \\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// move-to-column past end of line (without FORCE)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_move_to_column_past_eol_no_force() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Without FORCE, move-to-column stops at end of line and returns the
    // column it reached (not the target column). Buffer is NOT modified.
    let form = r#"(with-temp-buffer
                    (insert "ab\ncd\n\nefghijkl\n")
                    (let ((results nil))
                      ;; Line 1: "ab" (2 chars) - move to col 10
                      (goto-char (point-min))
                      (let ((r (move-to-column 10)))
                        (setq results
                              (cons (list r (current-column) (point)
                                          (buffer-string))
                                    results)))
                      ;; Line 2: "cd" - move to col 1 (should succeed)
                      (goto-char (point-min))
                      (forward-line 1)
                      (let ((r (move-to-column 1)))
                        (setq results
                              (cons (list r (current-column) (point))
                                    results)))
                      ;; Line 3: empty - move to col 5
                      (goto-char (point-min))
                      (forward-line 2)
                      (let ((r (move-to-column 5)))
                        (setq results
                              (cons (list r (current-column) (point))
                                    results)))
                      ;; Line 4: "efghijkl" (8 chars) - move to col 4
                      (goto-char (point-min))
                      (forward-line 3)
                      (let ((r (move-to-column 4)))
                        (setq results
                              (cons (list r (current-column) (point))
                                    results)))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 2 3 \"ab\\ncd\\n\\nefghijkl\\n\") (1 1 5) (0 0 7) (4 4 12))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: column-based text alignment with cleanup
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_column_alignment_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a function that takes rows of data and formats them into
    // aligned columns by computing the max width of each column, then
    // padding each field to align. Uses move-to-column with FORCE.
    let form = r#"(progn
      (fset 'neovm--test-format-table
        (lambda (rows)
          ;; rows is a list of lists of strings
          ;; 1. Compute max width per column
          (let ((ncols (length (car rows)))
                (widths nil))
            (dotimes (c ncols)
              (let ((max-w 0))
                (dolist (row rows)
                  (let ((cell (nth c row)))
                    (when (> (length cell) max-w)
                      (setq max-w (length cell)))))
                (setq widths (append widths (list (+ max-w 2))))))
            ;; 2. Compute column start positions
            (let ((positions (list 0))
                  (acc 0))
              (dolist (w widths)
                (setq acc (+ acc w))
                (setq positions (append positions (list acc))))
              ;; 3. Format each row
              (with-temp-buffer
                (dolist (row rows)
                  (let ((col-idx 0))
                    (dolist (cell row)
                      (move-to-column (nth col-idx positions) t)
                      (insert cell)
                      (setq col-idx (1+ col-idx))))
                  (insert "\n"))
                ;; 4. Verify alignment by reading back column positions
                (goto-char (point-min))
                (let ((verify nil))
                  (dotimes (_ (length rows))
                    (let ((row-check nil))
                      (dotimes (c ncols)
                        (move-to-column (nth c positions))
                        (setq row-check (cons (current-column) row-check)))
                      (setq verify (cons (nreverse row-check) verify)))
                    (forward-line 1))
                  (list (buffer-string) positions (nreverse verify))))))))
      (unwind-protect
          (let ((data '(("Name" "Age" "City" "Role")
                        ("Alice" "30" "NYC" "Engineer")
                        ("Bob" "25" "SF" "Designer")
                        ("Charlemagne" "45" "LA" "Director")
                        ("Di" "22" "Chicago" "Intern"))))
            (funcall 'neovm--test-format-table data))
        (fmakunbound 'neovm--test-format-table)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Name\t     Age  City\t   Role\\nAlice\t     30\t  NYC\t   Engineer\\nBob\t     25\t  SF\t   Designer\\nCharlemagne  45\t  LA\t   Director\\nDi\t     22\t  Chicago  Intern\\n\" (0 13 18 27 37) ((0 13 18 27) (0 13 18 27) (0 13 18 27) (0 13 18 27) (0 13 18 27)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// current-column and move-to-column interaction with control characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_current_column_control_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Control characters like ^A (char 1) display as "^A" (2 columns)
    // in Emacs. Verify current-column accounts for this display width.
    // Also test backspace (char 8) behavior.
    let form = r#"(with-temp-buffer
                    ;; Insert some control chars: char 1 = ^A, char 2 = ^B
                    (insert (string 1 2 ?a ?b ?c ?\n))
                    (goto-char (point-min))
                    (let ((results nil))
                      ;; At ^A: col 0
                      (setq results (cons (current-column) results))
                      ;; After ^A: col 2 (^A displays as 2 columns)
                      (forward-char 1)
                      (setq results (cons (current-column) results))
                      ;; After ^B: col 4
                      (forward-char 1)
                      (setq results (cons (current-column) results))
                      ;; After 'a': col 5
                      (forward-char 1)
                      (setq results (cons (current-column) results))
                      ;; After 'b': col 6
                      (forward-char 1)
                      (setq results (cons (current-column) results))
                      ;; move-to-column 3 should land inside ^B display
                      (goto-char (point-min))
                      (move-to-column 3)
                      (setq results (cons (list 'mtc-3 (current-column) (point)) results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (0 2 4 5 6 (mtc-3 4 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

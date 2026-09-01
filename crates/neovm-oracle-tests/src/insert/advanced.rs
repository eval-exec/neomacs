//! Oracle parity tests for advanced `insert` patterns:
//! `insert` with multiple arguments, `insert-before-markers`,
//! `insert-buffer-substring`, and complex insertion scenarios.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// insert with multiple arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_multi_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello" " " "world" "!")
                    (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"hello world!\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_insert_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert accepts both strings and characters
    let form = r#"(with-temp-buffer
                    (insert ?H "ello" ?\ ?W "orld")
                    (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"Hello World\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_insert_at_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char 6)
                    (insert "beautiful ")
                    (list (buffer-string) (point)))"#;
    let expect = expect_test::expect![[r#""OK (\"hellobeautiful  world\" 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_insert_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "before" "" "after")
                    (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"beforeafter\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert-before-markers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_before_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert-before-markers moves markers that are at point
    let form = r#"(with-temp-buffer
                    (insert "AB")
                    (goto-char 2) ;; between A and B
                    (let ((m (point-marker)))
                      (goto-char 2)
                      (insert "xx")
                      (let ((after-insert (marker-position m)))
                        (goto-char 2)
                        (insert-before-markers "yy")
                        (list (buffer-string)
                              after-insert
                              (marker-position m)))))"#;
    let expect = expect_test::expect![[r#""OK (\"AyyxxB\" 2 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build structured text by insertion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_build_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (let ((data '(("Alice" 30 "Boston")
                                  ("Bob" 25 "NYC")
                                  ("Carol" 35 "London"))))
                      ;; Header
                      (insert (format "%-10s %4s %-10s\n"
                                      "Name" "Age" "City"))
                      (insert (make-string 28 ?-) "\n")
                      ;; Rows
                      (dolist (row data)
                        (insert (format "%-10s %4d %-10s\n"
                                        (nth 0 row)
                                        (nth 1 row)
                                        (nth 2 row))))
                      ;; Footer
                      (insert (make-string 28 ?-) "\n")
                      (insert (format "Total: %d records\n"
                                      (length data)))
                      (buffer-string)))"#;
    let expect = expect_test::expect![[
        r#""OK \"Name        Age City      \\n----------------------------\\nAlice        30 Boston    \\nBob          25 NYC       \\nCarol        35 London    \\n----------------------------\\nTotal: 3 records\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: incremental buffer assembly
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_incremental_assembly() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build an S-expression incrementally
    let form = r#"(with-temp-buffer
                    (let ((items '((defun add (a b) (+ a b))
                                   (defun mul (a b) (* a b))
                                   (defun sub (a b) (- a b)))))
                      (insert "(progn\n")
                      (dolist (item items)
                        (insert "  " (prin1-to-string item) "\n"))
                      (insert ")")
                      (buffer-string)))"#;
    let expect = expect_test::expect![[
        r#""OK \"(progn\\n  (defun add (a b) (+ a b))\\n  (defun mul (a b) (* a b))\\n  (defun sub (a b) (- a b))\\n)\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: interleaved insert and delete
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_insert_and_delete_interleaved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "aaXbbXccXdd")
                    ;; Replace all X with ---
                    (goto-char (point-min))
                    (let ((replacements 0))
                      (while (search-forward "X" nil t)
                        (delete-char -1)
                        (insert "---")
                        (setq replacements (1+ replacements)))
                      (list (buffer-string) replacements)))"#;
    let expect = expect_test::expect![[r#""OK (\"aa---bb---cc---dd\" 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

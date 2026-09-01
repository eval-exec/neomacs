//! Oracle parity tests for alist operations: `assoc`, `assq`,
//! `rassoc`, `rassq`, `alist-get`, `copy-alist`, plus complex
//! alist manipulation patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// rassoc / rassq (reverse association)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rassoc_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((alist '((a . 1) (b . 2) (c . 3) (d . 2))))
                    (list (rassoc 1 alist)
                          (rassoc 2 alist)
                          (rassoc 4 alist)))"#;
    let expect = expect_test::expect![r#""OK ((a . 1) (b . 2) nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rassoc_string_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // rassoc uses equal, so works with strings
    let form = r#"(let ((alist '(("Alice" . "dev")
                                   ("Bob" . "qa")
                                   ("Carol" . "dev")
                                   ("Dave" . "ops"))))
                    (list (rassoc "dev" alist)
                          (rassoc "qa" alist)
                          (rassoc "hr" alist)))"#;
    let expect = expect_test::expect![[r#""OK ((\"Alice\" . \"dev\") (\"Bob\" . \"qa\") nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_rassq_vs_rassoc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // rassq uses eq, rassoc uses equal
    let form = r#"(let ((alist '((a . x) (b . y) (c . x))))
                    (list (rassq 'x alist)
                          (rassq 'y alist)
                          (rassq 'z alist)
                          ;; string values: rassq won't find, rassoc will
                          (let ((al2 '((1 . "hello") (2 . "world"))))
                            (list (rassq "hello" al2)
                                  (rassoc "hello" al2)))))"#;
    let expect = expect_test::expect![[r#""OK ((a . x) (b . y) nil (nil (1 . \"hello\")))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// assoc with TESTFN parameter (Emacs 26+)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_assoc_testfn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // assoc with custom test function
    let form = r#"(let ((alist '(("HELLO" . 1) ("world" . 2) ("FOO" . 3))))
                    (list
                     ;; Case-insensitive match
                     (assoc "hello" alist #'string-equal-ignore-case)
                     (assoc "foo" alist #'string-equal-ignore-case)
                     ;; Default (equal) comparison
                     (assoc "HELLO" alist)
                     (assoc "hello" alist)))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"HELLO\" . 1) (\"FOO\" . 3) (\"HELLO\" . 1) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// alist-get with DEFAULT and TESTFN
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_get_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((alist '((name . "Alice") (age . 30) (city . "Boston"))))
                    (list (alist-get 'name alist)
                          (alist-get 'age alist)
                          (alist-get 'missing alist)
                          (alist-get 'missing alist 'default-val)))"#;
    let expect = expect_test::expect![[r#""OK (\"Alice\" 30 nil default-val)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_alist_get_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // REMOVE parameter: with non-nil REMOVE, returns nil for nil-valued keys
    let form = r#"(let ((alist '((a . 1) (b . nil) (c . 3))))
                    (list (alist-get 'b alist)
                          (alist-get 'b alist nil t)
                          (alist-get 'd alist)
                          (alist-get 'd alist 'default)))"#;
    let expect = expect_test::expect![r#""OK (nil nil nil default)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// copy-alist deep vs shallow
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_copy_alist_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Modifying copy doesn't affect original
    let form = r#"(let ((orig '((a . 1) (b . 2) (c . 3))))
                    (let ((copy (copy-alist orig)))
                      ;; Modify copy
                      (setcdr (assq 'a copy) 99)
                      ;; Original unchanged
                      (list (cdr (assq 'a orig))
                            (cdr (assq 'a copy))
                            ;; But values that are themselves mutable
                            ;; share structure (shallow copy)
                            (eq (cdr (assq 'b orig))
                                (cdr (assq 'b copy))))))"#;
    let expect = expect_test::expect![r#""OK (1 99 t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist-based record system
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_record_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple record system: create, update, query, merge
    let form = r#"(let ((make-record
                         (lambda (&rest pairs)
                           (let ((rec nil))
                             (while pairs
                               (setq rec (cons (cons (car pairs)
                                                     (cadr pairs))
                                               rec)
                                     pairs (cddr pairs)))
                             (nreverse rec))))
                        (record-get
                         (lambda (rec key &optional default)
                           (let ((pair (assq key rec)))
                             (if pair (cdr pair) default))))
                        (record-set
                         (lambda (rec key val)
                           (let ((pair (assq key rec)))
                             (if pair
                                 (progn (setcdr pair val) rec)
                               (append rec (list (cons key val)))))))
                        (record-merge
                         (lambda (base override)
                           (let ((result (copy-alist base)))
                             (dolist (pair override)
                               (let ((existing (assq (car pair) result)))
                                 (if existing
                                     (setcdr existing (cdr pair))
                                   (setq result
                                         (append result
                                                 (list (cons (car pair)
                                                             (cdr pair))))))))
                             result))))
                    (let ((r1 (funcall make-record
                                       'name "Alice" 'age 30 'role "dev")))
                      (let ((r2 (funcall record-set r1 'age 31)))
                        (let ((r3 (funcall record-merge
                                           r2 '((role . "lead")
                                                (team . "core")))))
                          (list (funcall record-get r1 'name)
                                (funcall record-get r2 'age)
                                (funcall record-get r3 'role)
                                (funcall record-get r3 'team)
                                (funcall record-get r3 'missing 'N/A))))))"#;
    let expect = expect_test::expect![[r#""OK (\"Alice\" 31 \"lead\" \"core\" N/A)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist-based database with indexing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_indexed_db() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a secondary index from an alist "table"
    let form = r#"(let ((people '(((id . 1) (name . "Alice") (dept . "eng"))
                                   ((id . 2) (name . "Bob") (dept . "qa"))
                                   ((id . 3) (name . "Carol") (dept . "eng"))
                                   ((id . 4) (name . "Dave") (dept . "qa"))
                                   ((id . 5) (name . "Eve") (dept . "eng")))))
                    ;; Build index: dept -> list of names
                    (let ((index nil))
                      (dolist (person people)
                        (let* ((dept (cdr (assq 'dept person)))
                               (name (cdr (assq 'name person)))
                               (existing (assoc dept index)))
                          (if existing
                              (setcdr existing
                                      (append (cdr existing) (list name)))
                            (setq index
                                  (cons (list dept name) index)))))
                      ;; Query the index
                      (let ((eng (cdr (assoc "eng" index)))
                            (qa (cdr (assoc "qa" index))))
                        (list (sort eng #'string-lessp)
                              (sort qa #'string-lessp)
                              (length eng)
                              (length qa)))))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"Alice\" \"Carol\" \"Eve\") (\"Bob\" \"Dave\") 3 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: alist difference and intersection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_alist_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((a '((x . 1) (y . 2) (z . 3)))
                        (b '((y . 20) (z . 30) (w . 40))))
                    ;; Intersection: keys in both (take values from a)
                    (let ((inter nil))
                      (dolist (pair a)
                        (when (assq (car pair) b)
                          (setq inter (cons pair inter))))
                      ;; Difference: keys in a but not in b
                      (let ((diff nil))
                        (dolist (pair a)
                          (unless (assq (car pair) b)
                            (setq diff (cons pair diff))))
                        ;; Symmetric difference
                        (let ((sym-diff nil))
                          (dolist (pair a)
                            (unless (assq (car pair) b)
                              (setq sym-diff (cons pair sym-diff))))
                          (dolist (pair b)
                            (unless (assq (car pair) a)
                              (setq sym-diff (cons pair sym-diff))))
                          (list (nreverse inter)
                                (nreverse diff)
                                (nreverse sym-diff))))))"#;
    let expect = expect_test::expect![r#""OK (((y . 2) (z . 3)) ((x . 1)) ((x . 1) (w . 40)))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

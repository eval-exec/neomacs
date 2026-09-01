//! Divergence tests: list, sequence, hash-table, and symbol operations.
//!
//! Tests for list manipulation, sequence operations, hash table
//! semantics, and symbol property edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_nconc_mutation_sharing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3 99 5 6) (1 2 3 99 5 6) (99 5 6))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((a (list 1 2 3))
        (b (list 4 5 6)))
  (let ((c (nconc a b)))
    (setcar b 99)
    (list c a b)))"#,
        expect,
    );
}

#[test]
fn divergence_nreverse_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((5 4 3 2 1) (1))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((xs (list 1 2 3 4 5)))
  (let ((rev (nreverse xs)))
    (list rev xs)))"#,
        expect,
    );
}

#[test]
fn divergence_sort_stability() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"b\" \"e\" \"d\" \"a\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((data '((3 . "a") (1 . "b") (3 . "c") (2 . "d") (1 . "e"))))
  (mapcar #'cdr (sort (copy-sequence data)
                      (lambda (a b) (< (car a) (car b))))))"#,
        expect,
    );
}

#[test]
fn divergence_delq_first_element() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((2 3 4) (1 2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((xs (list 1 2 3 1 4)))
  (list (delq 1 xs) xs))"#,
        expect,
    );
}

#[test]
fn divergence_plist_put_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (99 99 nil (b 99 c 3) (a 1 b 99 c 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((pl '(a 1 b 2 c 3)))
  (let ((pl2 (plist-put pl 'b 99)))
    (list (plist-get pl 'b)
          (plist-get pl2 'b)
          (plist-get pl 'd)
          (plist-member pl 'b)
          pl2)))"#,
        expect,
    );
}

#[test]
fn divergence_assoc_assq_string_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"a\" . 1) (\"b\" . 2) (c . 3) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((alist '(("a" . 1) ("b" . 2) (c . 3))))
  (list (assoc "a" alist)
        (assoc "b" alist)
        (assq 'c alist)
        (assoc-default "a" alist)))"#,
        expect,
    );
}

#[test]
fn divergence_copy_tree_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a (b (c (d))) e) (a (b (X (d))) e))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((tree '(a (b (c (d))) e)))
  (let ((copy (copy-tree tree)))
    (setcar (cadr (cadr copy)) 'X)
    (list tree copy)))"#,
        expect,
    );
}

#[test]
fn divergence_hash_table_test_eq_vs_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 20 2 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((h-eq (make-hash-table :test 'eq))
        (h-equal (make-hash-table :test 'equal)))
  (puthash "hello" 1 h-eq)
  (puthash "hello" 2 h-eq)
  (puthash "hello" 10 h-equal)
  (puthash "hello" 20 h-equal)
  (list (gethash "hello" h-eq)
        (gethash "hello" h-equal)
        (hash-table-count h-eq)
        (hash-table-count h-equal)))"#,
        expect,
    );
}

#[test]
fn divergence_hash_table_remprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 2 1 (\"b\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((h (make-hash-table :test 'equal)))
  (puthash "a" 1 h)
  (puthash "b" 2 h)
  (remhash "a" h)
  (list (gethash "a" h)
        (gethash "b" h)
        (hash-table-count h)
        (let (keys) (maphash (lambda (k v) (push k keys)) h) keys)))"#,
        expect,
    );
}

#[test]
fn divergence_symbol_plist_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments get 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((sym (make-symbol "test-sym")))
  (put sym 'prop1 'val1)
  (put sym 'prop2 'val2)
  (list (get sym 'prop1)
        (get sym 'prop2)
        (get sym 'missing)
        (get sym 'missing 'default)
        (symbol-plist sym)))"#,
        expect,
    );
}

#[test]
fn divergence_intern_vs_make_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil t test-intern-symbol \"test-intern-symbol\" \"test-intern-symbol\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((s1 (intern "test-intern-symbol"))
        (s2 (make-symbol "test-intern-symbol")))
  (list (eq s1 s2)
        (eq s1 (intern "test-intern-symbol"))
        (intern-soft "test-intern-symbol")
        (symbol-name s1)
        (symbol-name s2)))"#,
        expect,
    );
}

#[test]
fn divergence_sequence_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (seq-map #'1+ [1 2 3])
  (seq-map #'1+ "abc")
  (seq-filter #'cl-evenp '(1 2 3 4 5 6))
  (seq-reduce #'+ '(1 2 3 4 5) 0))"#,
        expect,
    );
}

#[test]
fn divergence_vector_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (a 0 b 5 [a 0 b 0 c d e])""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((v (make-vector 5 0)))
  (aset v 0 'a)
  (aset v 2 'b)
  (aset v 4 'c)
  (list (aref v 0) (aref v 1) (aref v 2)
        (length v) (vconcat v [d e])))"#,
        expect,
    );
}

#[test]
fn divergence_char_table_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (word digit nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ct (make-char-table 'syntax-table nil)))
  (set-char-table-range ct ?a 'word)
  (set-char-table-range ct '(?0 . ?9) 'digit)
  (list (char-table-range ct ?a)
        (char-table-range ct ?5)
        (char-table-range ct ?z)
        (char-table-p ct)))"#,
        expect,
    );
}

#[test]
fn divergence_number_type_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (integerp 42)
  (integerp (expt 2 65))
  (floatp 3.14)
  (numberp 42)
  (numberp 3.14)
  (numberp "hello")
  (natnump 5)
  (natnump -1)
  (zerop 0))"#,
        expect,
    );
}

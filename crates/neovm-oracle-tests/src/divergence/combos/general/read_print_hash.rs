//! Divergence tests: read + print + circular + hash table combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_read_print_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal)))
    (puthash 'a 1 ht)
    (puthash 'b 2 ht)
    (puthash 'c 3 ht)
    (let ((printed (prin1-to-string ht))
          (a-val (gethash 'a ht))
          (b-val (gethash 'b ht))
          (count (hash-table-count ht)))
      (list a-val b-val count
            (= a-val 1)
            (= b-val 2)
            (= count 3)
            (stringp printed)
            (> (length printed) 0))))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_weak_references() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 t (1 2 3) t nil 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :weakness t :test 'equal)))
    (puthash 'key1 (list 1 2 3) ht)
    (puthash 'key2 (list 4 5 6) ht)
    (list (hash-table-count ht)
          (= (hash-table-count ht) 2)
          (gethash 'key1 ht)
          (equal (gethash 'key1 ht) '(1 2 3))
          (remhash 'key1 ht)
          (hash-table-count ht)
          (= (hash-table-count ht) 1)
          (null (gethash 'key1 ht))))) "#,
        expect,
    );
}

#[test]
fn divergence_read_backquote_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((x 10)
        (y '(a b c)))
    (list (eval (read "`(start ,x ,@y end)"))
          (equal (eval (read "`(start ,x ,@y end)")) '(start 10 a b c end))
          (eval (read "`(list ,(+ 1 2))"))
          (equal (eval (read "`(list ,(+ 1 2))")) '(list 3))
          (eval (read "`(,@y more)"))
          (equal (eval (read "`(,@y more)")) '(a b c more))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_circle_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 1 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((x (list 1 2 3)))
    (nconc x x)
    (let ((circle (list 'a x 'b)))
      (let ((printed (let ((print-circle t)) (prin1-to-string circle))))
        (list (stringp printed)
              (> (length printed) 0)
              (string-match "a" printed)
              (string-match "b" printed)))))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_as_obarray_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 t \"alpha\" t nil eq)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((cache (make-hash-table :test 'eq :size 100)))
    (dolist (name '("alpha" "beta" "gamma" "delta" "epsilon"))
      (let ((sym (intern name)))
        (puthash sym (symbol-name sym) cache)))
    (list (hash-table-count cache)
          (= (hash-table-count cache) 5)
          (gethash (intern "alpha") cache)
          (string= (gethash (intern "alpha") cache) "alpha")
          (maphash (lambda (k v) (equal (symbol-name k) v)) cache)
          (hash-table-test cache)))) "#,
        expect,
    );
}

#[test]
fn divergence_print_read_consistency_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((values (list 0 1 -1 42 most-positive-fixnum
                       1.5 -2.7 0.0 -0.0
                       1.5e10 1.5e-10)))
    (let ((printed (prin1-to-string values))
          (re-read (read (prin1-to-string values))))
      (list (equal values re-read)
            (stringp printed)
            (= (car values) 0)
            (= (cadr values) 1)
            (= (nth 2 values) -1)
            (= (nth 3 values) 42))))) "#,
        expect,
    );
}

#[test]
fn divergence_read_vector_array() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 3 t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((v1 (read "[1 2 3]"))
        (v2 (read "[a b c]"))
        (v3 (read "[]")))
    (list (vectorp v1)
          (equal v1 [1 2 3])
          (length v1)
          (= (aref v1 0) 1)
          (= (aref v1 2) 3)
          (vectorp v3)
          (= (length v3) 0)
          (equal (read "(1 2 3)") '(1 2 3))))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_iterate_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t 25 t 81 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal)))
    (dotimes (i 10) (puthash i (* i i) ht))
    (let ((keys nil)
          (vals nil))
      (maphash (lambda (k v)
                 (push k keys)
                 (push v vals))
               ht)
      (list (= (hash-table-count ht) 10)
            (= (length keys) 10)
            (= (length vals) 10)
            (gethash 5 ht)
            (= (gethash 5 ht) 25)
            (gethash 9 ht)
            (= (gethash 9 ht) 81))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_length_depth_truncation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 1 nil 1 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((deep (list 'a (list 'b (list 'c (list 'd (list 'e))))))
        (long (number-sequence 1 100)))
    (let ((print-depth (let ((print-level 3)) (prin1-to-string deep)))
          (print-len (let ((print-length 5)) (prin1-to-string long))))
      (list (stringp print-depth)
            (stringp print-len)
            (string-match "a" print-depth)
            (string-match "e" print-depth)
            (string-match "1" print-len)
            (string-match "5" print-len))))) "#,
        expect,
    );
}

#[test]
fn divergence_read_special_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((forms (mapcar 'read
                       '("(quote foo)" "'foo"
                         "(1+ 5)" "(list 1 2 3)"
                         "(cons 'a 'b)" "(vector 1 2)"
                         "nil" "t" "()"))))
    (list (equal (nth 0 forms) ''foo)
          (equal (nth 1 forms) ''foo)
          (= (eval (nth 2 forms)) 6)
          (equal (eval (nth 3 forms)) '(1 2 3))
          (equal (eval (nth 4 forms)) '(a . b))
          (equal (eval (nth 5 forms)) [1 2])
          (null (nth 6 forms))
          (eq (nth 7 forms) t)
          (null (nth 8 forms))))) "#,
        expect,
    );
}

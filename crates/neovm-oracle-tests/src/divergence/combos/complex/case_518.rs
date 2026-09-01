/// Batch 518: hash-table test, weakness, iterator, compare-fn deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx518_hash_table_weak_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :weakness 'key :test 'eq))
      (k (cons 1 2)))
  (puthash k 'value ht)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_weak_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :weakness 'value :test 'eq))
      (k (cons 1 2)))
  (puthash k (cons 'v 'al) ht)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (50 100)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :size 100)))
  (dotimes (i 50) (puthash i i ht))
  (list (hash-table-count ht) (hash-table-size ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_test_cmp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid hash table test\" equalp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :test 'equalp)))
  (list (hash-table-test ht) (fboundp 'hash-table-test)))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_clear() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table)))
  (puthash 'a 1 ht) (puthash 'b 2 ht)
  (clrhash ht)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function hash-table-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table)))
  (puthash 'a 1 ht) (puthash 'b 2 ht) (puthash 'c 3 ht)
  (let ((keys (hash-table-keys ht)))
    (sort keys (lambda (a b) (string< (symbol-name a) (symbol-name b))))))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function hash-table-values)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table)))
  (puthash 'a 3 ht) (puthash 'b 1 ht) (puthash 'c 2 ht)
  (sort (hash-table-values ht) #'<))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_maphash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table)) (sum 0))
  (puthash 'a 1 ht) (puthash 'b 2 ht) (puthash 'c 3 ht)
  (maphash (lambda (_ v) (setq sum (+ sum v))) ht)
  sum)
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_iterate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"b\" . 2) (\"a\" . 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht) (puthash "b" 2 ht)
  (let (results)
    (maphash (lambda (k v) (push (cons k v) results)) ht)
    results))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_rehash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 100""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :size 5 :rehash-size 2.0 :rehash-threshold 0.8)))
  (dotimes (i 100) (puthash i i ht))
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_persist_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table)))
  (puthash (list 1 2 3) (list 'a 'b 'c) ht)
  (puthash (vector 1 2) (vector 'x 'y) ht)
  (list (hash-table-count ht)
        (gethash (list 1 2 3) ht)
        (gethash (vector 1 2) ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_eq_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (val1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((k (list 1))
      (ht (make-hash-table :test 'eq)))
  (puthash k 'val1 ht)
  (list (gethash k ht) (gethash (list 1) ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_eql_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :test 'eql)))
  (dotimes (i 5) (puthash (float i) i ht))
  (list (gethash 2.0 ht) (gethash 2 ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_custom_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (= (closure (t) (x) (abs x)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (define-hash-table-test 'cx518-test #'= (lambda (x) (abs x)))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx518_hash_table_plist_to_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function alist-to-hash-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'subr-x)
  (let ((ht (alist-to-hash-table '((a . 1) (b . 2) (c . 3)))))
    (hash-table-count ht)))
"##,
        expect,
    );
}

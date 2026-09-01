//! Divergence tests: alist, plist, hash-map operations deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_alist_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a . 1) (b . 2) nil (a . 1) (b . 2) (a . 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((alist '((a . 1) (b . 2) (c . 3))))
  (list (assoc 'a alist)
        (assoc 'b alist)
        (assoc 'd alist)
        (assq 'a alist)
        (rassoc 2 alist)
        (rassq 1 alist))) "#,
        expect,
    );
}

#[test]
fn divergence_alist_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 20 nil ((b . 20) (a . 10)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((alist '((a . 1))))
  (setf (alist-get 'a alist) 10)
  (setf (alist-get 'b alist) 20)
  (list (alist-get 'a alist)
        (alist-get 'b alist)
        (alist-get 'c alist)
        alist)) "#,
        expect,
    );
}

#[test]
fn divergence_plist_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 nil (b 2 c 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((pl '(a 1 b 2 c 3)))
  (list (plist-get pl 'a)
        (plist-get pl 'b)
        (plist-get pl 'c)
        (plist-get pl 'd)
        (plist-member pl 'b))) "#,
        expect,
    );
}

#[test]
fn divergence_plist_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 (a 1 b 2 c 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((pl '(a 1 b 2)))
  (plist-put pl 'c 3)
  (list (plist-get pl 'a)
        (plist-get pl 'b)
        (plist-get pl 'c)
        pl)) "#,
        expect,
    );
}

#[test]
fn divergence_lax_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((pl '("a" 1 "b" 2)))
  (list (lax-plist-get pl "a")
        (lax-plist-get pl "b")
        (lax-plist-get pl "c")
        (fboundp 'lax-plist-put))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 nil 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht)
  (puthash "b" 2 ht)
  (puthash "c" 3 ht)
  (list (gethash "a" ht)
        (gethash "b" ht)
        (gethash "c" ht)
        (gethash "d" ht)
        (hash-table-count ht))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_iter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 2 t equal)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'equal))
        (keys nil)
        (vals nil))
  (puthash "x" 10 ht)
  (puthash "y" 20 ht)
  (maphash (lambda (k v) (push k keys) (push v vals)) ht)
  (list (length keys)
        (length vals)
        (hash-table-p ht)
        (hash-table-test ht))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_weakness() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (key t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'equal :weakness 'key)))
  (list (hash-table-weakness ht)
        (eq (hash-table-weakness ht) 'key)
        (hash-table-p ht))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 1.5 0.8125)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'eql :size 100)))
  (list (hash-table-size ht)
        (hash-table-rehash-size ht)
        (hash-table-rehash-threshold ht))) "#,
        expect,
    );
}

#[test]
fn divergence_map_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 1 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht1 (make-hash-table :test 'equal))
        (ht2 (make-hash-table :test 'equal)))
  (puthash "a" 1 ht1)
  (puthash "b" 2 ht2)
  (list (hash-table-count ht1)
        (hash-table-count ht2)
        (fboundp 'map-merge)
        (fboundp 'map-merge-with))) "#,
        expect,
    );
}

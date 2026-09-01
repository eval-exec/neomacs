//! Divergence tests: hash tables, structs, records, eieio-core edge cases.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_make_hash_table_custom_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (val1 val2 missing 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'equal)))
  (puthash "key1" 'val1 ht)
  (puthash "key2" 'val2 ht)
  (list (gethash "key1" ht)
        (gethash "key2" ht)
        (gethash "key3" ht 'missing)
        (hash-table-count ht)))"#,
        expect,
    );
}

#[test]
fn divergence_hash_table_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function symbol<)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'eq))
        keys vals)
  (puthash 'a 1 ht)
  (puthash 'b 2 ht)
  (puthash 'c 3 ht)
  (maphash (lambda (k v) (push k keys) (push v vals)) ht)
  (list (sort keys #'symbol<)
        (sort vals #'<)
        (hash-table-count ht)))"#,
        expect,
    );
}

#[test]
fn divergence_hash_table_removal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (gone 20 1 nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table)))
  (puthash 'x 10 ht)
  (puthash 'y 20 ht)
  (remhash 'x ht)
  (list (gethash 'x ht 'gone)
        (gethash 'y ht)
        (hash-table-count ht)
        (remhash 'z ht)
        (hash-table-count ht)))"#,
        expect,
    );
}

#[test]
fn divergence_hash_table_weakness() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (key-and-value (1 2 3) t eql)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :weakness t)))
  (puthash 'wk1 (list 1 2 3) ht)
  (list (hash-table-weakness ht)
        (gethash 'wk1 ht)
        (hash-table-p ht)
        (hash-table-test ht)))"#,
        expect,
    );
}

#[test]
fn divergence_copy_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 nil nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((ht (make-hash-table))
         (ht2 (copy-hash-table ht)))
  (puthash 'a 1 ht)
  (puthash 'b 2 ht2)
  (list (gethash 'a ht)
        (gethash 'a ht2)
        (gethash 'b ht)
        (gethash 'b ht2)))"#,
        expect,
    );
}

#[test]
fn divergence_cl_defstruct_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 20 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'cl-lib)
  (cl-defstruct (my-test-str (:constructor my-test-str-create))
    (x 0) (y 0))
  (let ((s (my-test-str-create :x 10 :y 20)))
    (list (my-test-str-x s)
          (my-test-str-y s)
          (my-test-str-p s))))"#,
        expect,
    );
}

#[test]
fn divergence_record_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function make-record-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((desc (make-record-type 'point '(x y)))
         (ctor (record-constructor desc))
         (inst (funcall ctor 3 4)))
  (list (record-type-p desc)
        (record-p inst)
        (aref inst 1)
        (aref inst 2)
        (record-type-name (record-type-of inst))))"#,
        expect,
    );
}

#[test]
fn divergence_named_record() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function make-record-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((cls (make-record-type 'pair '(car cdr)))
         (inst (funcall (record-constructor cls) 1 2)))
  (list (type-of inst)
        (record-type-length cls)))"#,
        expect,
    );
}

#[test]
fn divergence_obarray_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (my-obtest-sym nil my-obtest-sym t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (intern "my-obtest-sym")
  (list (intern-soft "my-obtest-sym")
        (intern-soft "nonexistent-sym-xyz-123")
        (intern "my-obtest-sym")
        (eq (intern "my-obtest-sym") (intern "my-obtest-sym"))))"#,
        expect,
    );
}

#[test]
fn divergence_mapc_mapcan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((2 3 4) (1 2 3) (a 1 b 2 c 3) \"a-b-c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(require 'cl-lib)
(list
  (mapcar #'1+ '(1 2 3))
  (mapc #'1+ '(1 2 3))
  (cl-mapcan #'list '(a b c) '(1 2 3))
  (mapconcat #'identity '("a" "b" "c") "-"))"#,
        expect,
    );
}

#[test]
fn divergence_assoc_list_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((b . 2) (a . 1) (c . 3) (a . 1) 3 missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((alist '((a . 1) (b . 2) (c . 3))))
  (list (assoc 'b alist)
        (assq 'a alist)
        (rassoc 3 alist)
        (rassq 1 alist)
        (alist-get 'c alist)
        (alist-get 'd alist 'missing)))"#,
        expect,
    );
}

#[test]
fn divergence_plist_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 nil (c 3 d 4) (a 1 b 2 c 3 d 4) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((pl '(a 1 b 2 c 3)))
  (list (plist-get pl 'b)
        (plist-get pl 'z)
        (plist-member pl 'c)
        (plist-put pl 'd 4)
        (lax-plist-get '(a 1 b 2) 'b)))"#,
        expect,
    );
}

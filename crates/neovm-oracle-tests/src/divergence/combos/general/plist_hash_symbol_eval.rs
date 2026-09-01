//! Divergence tests: plist + hash-table + obarray + symbol + eval combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_plist_manipulation_and_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"test\" t 5 t (a b c) t t (:active t :extra new) (:name \"test\" :count 5 :tags (a b c) :active t :extra new) new t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((pl '(:name "test" :count 5 :tags (a b c) :active t)))
    (list (plist-get pl :name)
          (string= (plist-get pl :name) "test")
          (plist-get pl :count)
          (= (plist-get pl :count) 5)
          (plist-get pl :tags)
          (equal (plist-get pl :tags) '(a b c))
          (plist-get pl :active)
          (plist-member pl :active)
          (plist-put pl :extra 'new)
          (plist-get (plist-put pl :extra 'new) :extra)
          (eq (plist-get (plist-put pl :extra 'new) :extra) 'new)
          (plist-get pl :nonexistent)
          (null (plist-get pl :nonexistent))))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (1 t 2 t 3 t nil t 3 t removed t 2 t cleared 0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal :size 10)))
    (puthash 'alpha 1 ht)
    (puthash 'beta 2 ht)
    (puthash 'gamma 3 ht)
    (list (gethash 'alpha ht)
          (= (gethash 'alpha ht) 1)
          (gethash 'beta ht)
          (= (gethash 'beta ht) 2)
          (gethash 'gamma ht)
          (= (gethash 'gamma ht) 3)
          (gethash 'delta ht)
          (null (gethash 'delta ht))
          (hash-table-count ht)
          (= (hash-table-count ht) 3)
          (progn (remhash 'beta ht) 'removed)
          (null (gethash 'beta ht))
          (hash-table-count ht)
          (= (hash-table-count ht) 2)
          (progn (clrhash ht) 'cleared)
          (hash-table-count ht)
          (= (hash-table-count ht) 0)))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((x y z) t (10 20 30) t 3 t 20 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal))
        (keys nil)
        (vals nil))
    (puthash 'x 10 ht)
    (puthash 'y 20 ht)
    (puthash 'z 30 ht)
    (maphash (lambda (k v)
               (push k keys)
               (push v vals))
             ht)
    (let ((sorted-keys (sort keys (lambda (a b) (string< (symbol-name a)
                                                          (symbol-name b)))))
          (sorted-vals (sort vals '<)))
      (list sorted-keys
            (equal sorted-keys '(x y z))
            sorted-vals
            (equal sorted-vals '(10 20 30))
            (hash-table-count ht)
            (= (hash-table-count ht) 3)
            (gethash 'y ht)
            (= (gethash 'y ht) 20))))) "#,
        expect,
    );
}

#[test]
fn divergence_symbol_plist_vs_separate_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (val1 t val2 t val3 t nil t (prop1 val1 prop2 val2 prop3 val3) (prop1 val1 prop2 val2 prop3 val3) (prop2 val2 prop3 val3) \"test-spvs-xxx\" t test-spvs-xxx t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-spvs-xxx nil)
  (put 'test-spvs-xxx 'prop1 'val1)
  (put 'test-spvs-xxx 'prop2 'val2)
  (put 'test-spvs-xxx 'prop3 'val3)
  (list (get 'test-spvs-xxx 'prop1)
        (eq (get 'test-spvs-xxx 'prop1) 'val1)
        (get 'test-spvs-xxx 'prop2)
        (eq (get 'test-spvs-xxx 'prop2) 'val2)
        (get 'test-spvs-xxx 'prop3)
        (eq (get 'test-spvs-xxx 'prop3) 'val3)
        (get 'test-spvs-xxx 'nonexistent)
        (null (get 'test-spvs-xxx 'nonexistent))
        (symbol-plist 'test-spvs-xxx)
        (plist-member (symbol-plist 'test-spvs-xxx) 'prop1)
        (plist-member (symbol-plist 'test-spvs-xxx) 'prop2)
        (symbol-name 'test-spvs-xxx)
        (string= (symbol-name 'test-spvs-xxx) "test-spvs-xxx")
        (intern-soft "test-spvs-xxx")
        (eq (intern-soft "test-spvs-xxx") 'test-spvs-xxx))) "#,
        expect,
    );
}

#[test]
fn divergence_obarray_intern_unintern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((sym-name "test-oiu-xxx"))
    (let ((s1 (intern-soft sym-name)))
      (null s1))
    (let ((s2 (intern sym-name)))
      (symbolp s2)
      (eq s2 (intern-soft sym-name))
      (eq s2 (intern sym-name))
      (put s2 'test-val 42)
      (let ((v (get s2 'test-val)))
        (unintern sym-name obarray)
        (let ((s3 (intern-soft sym-name)))
          (list (null s3)
                (eq s3 nil)
                (= v 42)
                (let ((s4 (intern sym-name)))
                  (not (eq s4 s2))))))))) "#,
        expect,
    );
}

#[test]
fn divergence_eval_with_dynamic_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 t (20 t 20) 6 t (1 2 3) t \"abc\" t 10 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-ewdb-xxx 10)
  (list (eval 'test-ewdb-xxx)
        (= (eval 'test-ewdb-xxx) 10)
        (let ((test-ewdb-xxx 20))
          (list test-ewdb-xxx
                (= test-ewdb-xxx 20)
                (eval 'test-ewdb-xxx)))
        (eval '(+ 1 2 3))
        (= (eval '(+ 1 2 3)) 6)
        (eval '(list 1 2 3))
        (equal (eval '(list 1 2 3)) '(1 2 3))
        (eval '(concat "a" "b" "c"))
        (string= (eval '(concat "a" "b" "c")) "abc")
        test-ewdb-xxx
        (= test-ewdb-xxx 10))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_with_string_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (value1 t value2 t nil t 3 t updated updated t 3 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal)))
    (puthash "key1" 'value1 ht)
    (puthash "key2" 'value2 ht)
    (puthash "key3" 'value3 ht)
    (list (gethash "key1" ht)
          (eq (gethash "key1" ht) 'value1)
          (gethash "key2" ht)
          (eq (gethash "key2" ht) 'value2)
          (gethash "KEY1" ht)
          (null (gethash "KEY1" ht))
          (hash-table-count ht)
          (= (hash-table-count ht) 3)
          (puthash "key1" 'updated ht)
          (gethash "key1" ht)
          (eq (gethash "key1" ht) 'updated)
          (hash-table-count ht)
          (= (hash-table-count ht) 3)))) "#,
        expect,
    );
}

#[test]
fn divergence_plist_lax_vs_strict() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 t 2 t 3 t 1 t 2 t 3 t (a 1 b 2 c 3 d 4) 4 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((strict-pl '(:a 1 :b 2 :c 3))
        (lax-pl '(a 1 b 2 c 3)))
    (list (lax-plist-get lax-pl 'a)
          (= (lax-plist-get lax-pl 'a) 1)
          (lax-plist-get lax-pl 'b)
          (= (lax-plist-get lax-pl 'b) 2)
          (lax-plist-get lax-pl 'c)
          (= (lax-plist-get lax-pl 'c) 3)
          (plist-get strict-pl :a)
          (= (plist-get strict-pl :a) 1)
          (plist-get strict-pl :b)
          (= (plist-get strict-pl :b) 2)
          (plist-get strict-pl :c)
          (= (plist-get strict-pl :c) 3)
          (lax-plist-put lax-pl 'd 4)
          (lax-plist-get (lax-plist-put lax-pl 'd 4) 'd)
          (= (lax-plist-get (lax-plist-put lax-pl 'd 4) 'd) 4)))) "#,
        expect,
    );
}

#[test]
fn divergence_symbol_function_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function advice--cdar)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-sfb-xxx (x) (+ x 1))
  (let ((original (symbol-function 'test-sfb-xxx)))
    (list (funcall 'test-sfb-xxx 5)
          (= (funcall 'test-sfb-xxx 5) 6)
          (functionp 'test-sfb-xxx)
          (subrp original)
          (not (subrp original))
          (advice-add 'test-sfb-xxx :override
            (lambda (x) (+ x 100)))
          (funcall 'test-sfb-xxx 5)
          (= (funcall 'test-sfb-xxx 5) 105)
          (advice-remove 'test-sfb-xxx
            (advice--cdar (advice--symbol-function 'test-sfb-xxx)))
          (funcall 'test-sfb-xxx 5)
          (= (funcall 'test-sfb-xxx 5) 6)))) "#,
        expect,
    );
}

#[test]
fn divergence_eval_with_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((x 10)
        (y 20)
        (items '(a b c)))
    (list (eval '`,x)
          (= (eval '`,x) 10)
          (eval '`,(+ x y))
          (= (eval '`,(+ x y)) 30)
          (eval '`,@items)
          (equal (eval '`,@items) '(a b c))
          (eval '`(list ,x ,y ,@items))
          (equal (eval '`(list ,x ,y ,@items)) '(10 20 a b c))
          (eval '`,(car items))
          (eq (eval '`,(car items)) 'a)
          (eval '`,(length items))
          (= (eval '`,(length items)) 3)))) "#,
        expect,
    );
}

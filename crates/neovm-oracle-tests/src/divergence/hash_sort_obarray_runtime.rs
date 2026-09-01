//! Hash-table (eq/eql/equal tests, maphash, remhash/clrhash, sxhash,
//! copy-hash-table), sort (stability, vectors, :key/:lessp), and obarray/
//! symbol (intern, mapatoms, symbol-plist) parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn hash_copy_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 25 0 changed)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((h (make-hash-table :test 'equal :size 2)))
  (dotimes (i 10) (puthash i (* i i) h))
  (let ((h2 (copy-hash-table h)))
    (puthash 0 'changed h2)
    (list (hash-table-count h) (gethash 5 h) (gethash 0 h) (gethash 0 h2))))"##,
        expect,
    );
}

#[test]
fn hash_iterate_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"a\" \"b\" \"c\") 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((h (make-hash-table :test 'equal)))
  (dolist (k '("a" "b" "c")) (puthash k (upcase k) h))
  (let ((ks nil)) (maphash (lambda (k _v) (push k ks)) h)
    (list (sort ks #'string<) (hash-table-count h))))"##,
        expect,
    );
}

#[test]
fn hash_remove_clear() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 0 default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((h (make-hash-table :test 'eq)))
  (puthash 'a 1 h) (puthash 'b 2 h) (puthash 'c 3 h)
  (remhash 'b h)
  (let ((n1 (hash-table-count h))) (clrhash h)
    (list n1 (hash-table-count h) (gethash 'a h 'default))))"##,
        expect,
    );
}

#[test]
fn hash_test_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 1 x 2 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((he (make-hash-table :test 'eq)) (hl (make-hash-table :test 'equal))
       (hq (make-hash-table :test 'eql)))
  (puthash "k" 1 hl) (puthash "k" 2 hl)
  (puthash 1.5 'x hq)
  (list (gethash "k" hl) (hash-table-count hl) (gethash 1.5 hq)
        (gethash (copy-sequence "k") hl) (gethash (copy-sequence "k") he)))"##,
        expect,
    );
}

#[test]
fn sxhash_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (= (sxhash-equal "abc") (sxhash-equal (concat "ab" "c")))
        (= (sxhash-eq 'sym) (sxhash-eq 'sym))
        (= (sxhash-equal '(1 2 3)) (sxhash-equal (list 1 2 3))))"##,
        expect,
    );
}

#[test]
fn obarray_intern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"bar\" \"foo\") foo nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ob (obarray-make)))
  (intern "foo" ob) (intern "bar" ob)
  (let ((syms nil)) (mapatoms (lambda (s) (push (symbol-name s) syms)) ob)
    (list (sort syms #'string<) (intern-soft "foo" ob) (intern-soft "nope" ob))))"##,
        expect,
    );
}

#[test]
fn sort_key_lessp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((-1 2 3 -5) (-1 2 3 -5) ((3 . a) (2 . m) (1 . z)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (sort (list 3 -1 2 -5) :lessp #'< :key #'abs)
      (sort (list 3 -1 2 -5) :key #'abs)
      (cl-sort (list '(1 . z) '(3 . a) '(2 . m)) #'string< :key #'cdr))"##,
        expect,
    );
}

#[test]
fn sort_stability() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 . \"a\") (1 . \"c\") (1 . \"e\") (2 . \"b\") (2 . \"d\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((data '((1 . "a") (2 . "b") (1 . "c") (2 . "d") (1 . "e"))))
  (sort (copy-sequence data) (lambda (x y) (< (car x) (car y)))))"##,
        expect,
    );
}

#[test]
fn sort_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((1 2 3 4 5) [3 2 1] (\"apple\" \"banana\" \"cherry\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (sort (list 3 1 2 5 4) #'<)
        (sort (vector 3 1 2) #'>)
        (sort (list "banana" "apple" "cherry") #'string<))"##,
        expect,
    );
}

#[test]
fn symbol_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (val1 42 \"uninterned-xyz\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (make-symbol "uninterned-xyz")))
  (put 'neo-test-sym-xyz 'prop1 'val1)
  (put 'neo-test-sym-xyz 'prop2 42)
  (list (get 'neo-test-sym-xyz 'prop1) (get 'neo-test-sym-xyz 'prop2)
        (symbol-name s) (eq s (intern-soft "uninterned-xyz"))))"##,
        expect,
    );
}

//! Deep combo: setf + generalized variables + buffer-local + alist + hash.
//! Tests setf/plist/alist/hash generalized variable setters.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_setf_on_alist_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((c . 3) (a . 99) (b . 2)) 99 2 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((alist '((a . 1) (b . 2))))\n\
         (setf (alist-get 'a alist) 99)\n\
         (setf (alist-get 'c alist) 3)\n\
         (list alist\n\
         (alist-get 'a alist)\n\
         (alist-get 'b alist)\n\
         (alist-get 'c alist))))",
        expect,
    );
}

#[test]
fn deficiency_setf_on_plist_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a 1 b 99 c 3) 1 99 3)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((pl '(a 1 b 2 c 3)))\n\
         (setf (plist-get pl 'b) 99)\n\
         (list pl\n\
         (plist-get pl 'a)\n\
         (plist-get pl 'b)\n\
         (plist-get pl 'c))))",
        expect,
    );
}

#[test]
fn deficiency_setf_on_gethash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (99 20 nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ht (make-hash-table :test 'eql)))\n\
         (setf (gethash 'x ht) 10)\n\
         (setf (gethash 'y ht) 20)\n\
         (setf (gethash 'x ht) 99)\n\
         (list (gethash 'x ht)\n\
         (gethash 'y ht)\n\
         (gethash 'z ht)\n\
         (hash-table-count ht))))",
        expect,
    );
}

#[test]
fn deficiency_setf_on_aref_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([0 2 99 4 5] 0 99 5)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((v (vector 1 2 3 4 5)))\n\
         (setf (aref v 2) 99)\n\
         (setf (aref v 0) 0)\n\
         (list v (aref v 0) (aref v 2) (aref v 4))))",
        expect,
    );
}

#[test]
fn deficiency_setf_on_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"sbs\")))\n\
         (with-current-buffer buf\n\
         (insert \"hello world\")\n\
         (put-text-property 1 6 'face 'bold)\n\
         (setf (buffer-substring 1 6) \"HELLO\")\n\
         (list (buffer-string)\n\
         (get-text-property 1 'face)\n\
         (get-text-property 6 'face)))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_setf_on_nth_and_car_cdr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((X Y Z) X Y)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((lst (list 'a 'b 'c 'd 'e)))\n\
         (setf (nth 0 lst) 'A)\n\
         (setf (nth 2 lst) 'C)\n\
         (setf (car lst) 'X)\n\
         (setf (cdr lst) '(Y Z))\n\
         (list lst (nth 0 lst) (nth 1 lst))))",
        expect,
    );
}

#[test]
fn deficiency_setf_on_symbol_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (99 99 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (set 'test-sym-val 10)\n\
         (setf (symbol-value 'test-sym-val) 99)\n\
         (list test-sym-val\n\
         (symbol-value 'test-sym-val)\n\
         (boundp 'test-sym-val)))",
        expect,
    );
}

#[test]
fn deficiency_push_via_setf_on_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((c . 3) (b . 99) (a . 1)) 1 99)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((alist nil))\n\
         (push '(a . 1) alist)\n\
         (push '(b . 2) alist)\n\
         (push '(c . 3) alist)\n\
         (setf (alist-get 'b alist) 99)\n\
         (list alist\n\
         (alist-get 'a alist)\n\
         (alist-get 'b alist))))",
        expect,
    );
}

#[test]
fn deficiency_cl_incf_cl_decf_on_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-decf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ht (make-hash-table :test 'eql)))\n\
         (setf (gethash 'count ht) 0)\n\
         (cl-incf (gethash 'count ht))\n\
         (cl-incf (gethash 'count ht) 5)\n\
         (cl-decf (gethash 'count ht) 2)\n\
         (list (gethash 'count ht))))",
        expect,
    );
}

#[test]
fn deficiency_setf_on_multiple_places() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-shiftf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((a (list 1))\n\
         (b (list 2))\n\
         (c (list 3)))\n\
         (cl-shiftf (car a) (car b) (car c) 99)\n\
         (list a b c)))",
        expect,
    );
}

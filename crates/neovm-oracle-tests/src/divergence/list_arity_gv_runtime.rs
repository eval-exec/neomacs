//! List/seq utility parity: flatten-tree/ensure-list/proper-list-p,
//! take/ntake/last/butlast, alist-get/plist-get (incl. testfn), assq-delete,
//! number-sequence, func-arity/subr-arity, and gv/setf places (nth, aref,
//! alist-get, plist-get, cl-incf, push).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn func_arity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 . 1) (0 . many) (2 . 3) (1 . many) (2 . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (func-arity #'car) (func-arity #'+) (func-arity (lambda (a b &optional c) a))
        (func-arity #'format) (subr-arity (symbol-function 'cons)))"##,
        expect,
    );
}

#[test]
fn gv_plist_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (:a 100 :b 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(let ((pl (list :a 1 :b 2)))
  (setf (plist-get pl :a) 100)
  (cl-incf (plist-get pl :b) 5)
  pl)"##,
        expect,
    );
}

#[test]
fn gv_setf_places() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((2 99 3 x) ((a . 42)) [7 0])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(let ((l (list 1 2 3)) (al (list (cons 'a 1))) (v (vector 0 0)))
  (setf (nth 1 l) 99) (setf (alist-get 'a al) 42) (setf (aref v 0) 7)
  (cl-incf (car l)) (push 'x (cdr (last l)))
  (list l al v))"##,
        expect,
    );
}

#[test]
fn list_alist_get_testfn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 b 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (alist-get "b" '(("a" . 1) ("b" . 2)) nil nil #'equal)
        (alist-get 2.0 '((1 . a) (2 . b)) 'def nil #'=)
        (plist-get '("a" 1 "b" 2) "b" #'equal))"##,
        expect,
    );
}

#[test]
fn list_alist_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 2 (:b 2) ((b . 2)) (b . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (alist-get 'b '((a . 1) (b . 2))) (plist-get '(:a 1 :b 2) :b)
        (plist-member '(:a 1 :b 2) :b) (assq-delete-all 'a (list '(a . 1) '(b . 2) '(a . 3)))
        (rassq 2 '((a . 1) (b . 2))))"##,
        expect,
    );
}

#[test]
fn list_flatten_ensure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6) (5) (1 2) nil 3 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (flatten-tree '(1 (2 (3 4)) (5) 6))
        (ensure-list 5) (ensure-list '(1 2)) (ensure-list nil)
        (proper-list-p '(1 2 3)) (proper-list-p '(1 2 . 3)))"##,
        expect,
    );
}

#[test]
fn list_set_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-remove-duplicates)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (number-sequence 1 10 2) (number-sequence 5 1 -1)
        (cl-remove-duplicates '(1 2 1 3 2) :from-end t)
        (reverse '(1 2 3)) (nreverse (list 1 2 3)) (append '(1) '(2) '(3) 4))"##,
        expect,
    );
}

#[test]
fn list_take_ntake() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2) nil (1 2) (1 2) (3 4) (1 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (take 2 '(1 2 3 4)) (take 0 '(1 2)) (take 10 '(1 2))
        (ntake 2 (list 1 2 3 4)) (last '(1 2 3 4) 2) (butlast '(1 2 3 4) 2))"##,
        expect,
    );
}

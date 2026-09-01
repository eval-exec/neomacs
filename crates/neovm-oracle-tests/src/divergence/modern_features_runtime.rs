//! Modern Emacs feature parity: oclosure (oclosure-define/-lambda/-type),
//! symbols-with-position (position-symbol/bare-symbol/symbol-with-pos-p/-pos),
//! records (recordp/type-of/copy), cl-defstruct :type vector/list :named,
//! condition-case :success, with-demoted-errors, function-put/get.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn cl_struct_type_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 t neo-pt-l)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-defstruct (neo-pt-l (:type list) :named) a b)
(let ((p (make-neo-pt-l :a 1 :b 2)))
  (list (neo-pt-l-a p) (neo-pt-l-b p) (listp p) (car p)))"##,
        expect,
    );
}

#[test]
fn cl_struct_type_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 4 t 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-defstruct (neo-pt-v (:type vector)) x y)
(let ((p (make-neo-pt-v :x 3 :y 4)))
  (list (neo-pt-v-x p) (neo-pt-v-y p) (vectorp p) (aref p 0)))"##,
        expect,
    );
}

#[test]
fn condition_case_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (30 caught)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case r (+ 1 2) (:success (* r 10)) (error 'err))
        (condition-case r (error "x") (:success 'ok) (error 'caught)))"##,
        expect,
    );
}

#[test]
fn function_put_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (val123 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (function-put 'car 'neo-test-prop-xyz 'val123)
  (list (function-get 'car 'neo-test-prop-xyz) (function-get 'cdr 'neo-test-prop-xyz)))"##,
        expect,
    );
}

#[test]
fn oclosure_define() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 42 neo-oc-xyz)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn (require 'oclosure)
  (oclosure-define neo-oc-xyz (slot))
  (let ((o (oclosure-lambda (neo-oc-xyz (slot 42)) () slot)))
    (list (funcall o) (neo-oc-xyz--slot o) (oclosure-type o)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn oclosure_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t oclosure)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fboundp 'oclosure-type) (fboundp 'oclosure-lambda) (featurep 'oclosure)
        (require 'oclosure nil t))"##,
        expect,
    );
}

#[test]
fn record_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t my-type my-type 1 4 (1 99))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((r (record 'my-type 1 2 3)))
  (list (recordp r) (type-of r) (aref r 0) (aref r 1) (length r)
        (let ((r2 (copy-sequence r))) (aset r2 1 99) (list (aref r 1) (aref r2 1)))))"##,
        expect,
    );
}

#[test]
fn symbols_with_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t (t foo 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fboundp 'position-symbol) (fboundp 'bare-symbol) (fboundp 'symbol-with-pos-p)
        (let ((s (position-symbol 'foo 5))) (list (symbol-with-pos-p s) (bare-symbol s) (symbol-with-pos-pos s))))"##,
        expect,
    );
}

#[test]
fn with_demoted_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (before)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((log nil))
  (with-demoted-errors "demoted: %S" (push 'before log) (error "boom") (push 'after log))
  (nreverse log))"##,
        expect,
    );
}

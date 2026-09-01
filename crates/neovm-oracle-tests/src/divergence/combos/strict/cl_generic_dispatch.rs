//! Strict combo oracle probes, batch 29: cl-generic CLOS dispatch —
//! defgeneric/defmethod primary dispatch, :before/:after qualifiers,
//! multiple-argument dispatch, call-next-method chaining, and (eql X)
//! specializers.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_g4_cl_generic_basic_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 \"AB\" (fallback (1)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defgeneric probe-gen-1 (x))
  (cl-defmethod probe-gen-1 ((x number)) (* x 2))
  (cl-defmethod probe-gen-1 ((x string)) (upcase x))
  (cl-defmethod probe-gen-1 (x) (list 'fallback x))
  (list (probe-gen-1 5)
        (probe-gen-1 "ab")
        (probe-gen-1 '(1))))
"##,
        expect,
    );
}

#[test]
fn div_g4_cl_generic_qualifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((primary after) (before primary after))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((log nil))
  (cl-defgeneric probe-gen-2 (x))
  (cl-defmethod probe-gen-2 :before ((x number)) (push 'before log))
  (cl-defmethod probe-gen-2 :after ((x number)) (push 'after log))
  (cl-defmethod probe-gen-2 ((x number)) (push 'primary log))
  (list (probe-gen-2 5) (nreverse log)))
"##,
        expect,
    );
}

#[test]
fn div_g4_cl_generic_call_next_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function call-next-method)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defgeneric probe-gen-3 (x))
  (cl-defmethod probe-gen-3 ((x integer))
    (if (> x 10) (call-next-method) 'small))
  (cl-defmethod probe-gen-3 ((x number)) 'big-number)
  (list (probe-gen-3 5) (probe-gen-3 100)))
"##,
        expect,
    );
}

#[test]
fn div_g4_cl_generic_eql_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (matched-special matched-42 fallback fallback)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defgeneric probe-gen-4 (x))
  (cl-defmethod probe-gen-4 ((x (eql :special))) 'matched-special)
  (cl-defmethod probe-gen-4 ((x (eql 42))) 'matched-42)
  (cl-defmethod probe-gen-4 (x) 'fallback)
  (list (probe-gen-4 :special)
        (probe-gen-4 42)
        (probe-gen-4 :other)
        (probe-gen-4 7)))
"##,
        expect,
    );
}

#[test]
fn div_g4_cl_generic_multiple_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (num-num str-str fallback)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defgeneric probe-gen-5 (a b))
  (cl-defmethod probe-gen-5 ((a number) (b number)) 'num-num)
  (cl-defmethod probe-gen-5 ((a string) (b string)) 'str-str)
  (cl-defmethod probe-gen-5 (a b) 'fallback)
  (list (probe-gen-5 1 2)
        (probe-gen-5 "a" "b")
        (probe-gen-5 1 "b")))
"##,
        expect,
    );
}

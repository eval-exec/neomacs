//! Strict combo oracle probes, batch 173: cl-generic multiple dispatch.
//! defgeneric + defmethod on combinations of specialized args, precedence
//! when multiple methods match, :eql specializer, and cl-call-next-method
//! chaining.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_generic_multi_dispatch_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-generic)
(cl-defgeneric probe-md (a b))
(cl-defmethod probe-md ((a number) (b number)) 'num-num)
(cl-defmethod probe-md ((a string) (b string)) 'str-str)
(cl-defmethod probe-md ((a number) b) 'num-any)
(cl-defmethod probe-md (a (b string)) 'any-str)
(cl-defmethod probe-md (a b) 'any-any)
(list (probe-md 1 2)
      (probe-md "x" "y")
      (probe-md 1 "y")
      (probe-md 'sym 'sym2)
      (probe-md [1 2] 5)
      (probe-md 5 [1 2]))
"##;
    let expect =
        expect_test::expect![[r#""OK (num-num str-str num-any any-any any-any num-any)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_generic_eql_specializer_call_next_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-generic)
(cl-defgeneric probe-eq (x))
(cl-defmethod probe-eq (x) 'default)
(cl-defmethod probe-eq ((x (eql special))) 'is-special)
(cl-defmethod probe-eq ((x (eql 42))) 'is-forty-two)
(cl-defgeneric probe-chain (x))
(cl-defmethod probe-chain (x) 'base)
(cl-defmethod probe-chain ((x number)) (cons 'num-pre (cl-call-next-method)))
(list (probe-eq 'special)
      (probe-eq 42)
      (probe-eq 'other)
      (probe-eq 'anything)
      (probe-chain 5)
      (probe-chain "str")
      (probe-chain [1 2]))
"##;
    let expect = expect_test::expect![[
        r#""OK (is-special is-forty-two default default (num-pre . base) base base)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_generic_hierarchy_overrides_no_applicable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-generic)
(cl-defgeneric probe-h (x))
(cl-defmethod probe-h ((x integer)) 'int)
(cl-defmethod probe-h ((x float)) 'float)
(cl-defmethod probe-h ((x string)) 'str)
(list (probe-h 5)
      (probe-h 5.0)
      (probe-h "hi")
      (condition-case err (probe-h [1 2]) (cl-no-applicable-method 'no-method))
      (condition-case err (probe-h 'sym) (cl-no-applicable-method 'no-method-sym)))
"##;
    let expect = expect_test::expect![[r#""OK (int float str no-method no-method-sym)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

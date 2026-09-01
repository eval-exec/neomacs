//! Strict combo oracle probes, batch 335: cl-call-next-method :around chaining.
//! Multi-level :around methods calling cl-call-next-method, :before/:after with
//! :around, and cl-next-method-p predicate.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_call_next_method_around_multi_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-generic)
(cl-defgeneric probe-cnm-chain (x))
(cl-defmethod probe-cnm-chain ((x integer)) 'base-int)
(cl-defmethod probe-cnm-chain :around ((x integer))
  (cons 'around-1 (cl-call-next-method)))
(cl-defmethod probe-cnm-chain :around ((x (eql 42)))
  (cons 'around-2 (cl-call-next-method)))
(list (probe-cnm-chain 5)
      (probe-cnm-chain 42))
"##;
    let expect =
        expect_test::expect![[r#""OK ((around-1 . base-int) (around-2 around-1 . base-int))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_next_method_p_before_after_with_around() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-generic)
(defvar probe-cnm-log nil)
(cl-defgeneric probe-combo (x))
(cl-defmethod probe-combo ((x integer))
  (push 'primary probe-cnm-log)
  'primary-result)
(cl-defmethod probe-combo :before ((x integer))
  (push 'before probe-cnm-log))
(cl-defmethod probe-combo :after ((x integer))
  (push 'after probe-cnm-log))
(cl-defmethod probe-combo :around ((x integer))
  (push 'around-enter probe-cnm-log)
  (let ((r (cl-call-next-method)))
    (push 'around-exit probe-cnm-log)
    r))
(let ((result (probe-combo 5)))
  (list result (nreverse probe-cnm-log)))
"##;
    let expect = expect_test::expect![[
        r#""OK (primary-result (around-enter before primary after around-exit))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_no_next_method_predicate_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-generic)
(cl-defgeneric probe-nnm (x))
(cl-defmethod probe-nnm ((x integer))
  (list (cl-next-method-p)
        (condition-case err
            (cl-call-next-method)
          (cl-no-next-method 'caught-no-next))))
(list (probe-nnm 5))
"##;
    let expect = expect_test::expect![[r#""OK ((nil caught-no-next))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

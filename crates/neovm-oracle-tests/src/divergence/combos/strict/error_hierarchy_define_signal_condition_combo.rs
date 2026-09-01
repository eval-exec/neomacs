//! Strict combo oracle probes, batch 203: custom error hierarchy. define-error
//! to build a hierarchy, signal custom errors, condition-case handler matching
//! by inheritance, and error-conditions / error-message metadata.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_define_error_hierarchy_inheritance_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (define-error 'probe-base-err "base error")
  (define-error 'probe-specific-err "specific error" 'probe-base-err)
  (list (condition-case err (signal 'probe-specific-err '("detail"))
           (probe-specific-err (cons 'caught-specific (cdr err)))
           (probe-base-err (cons 'caught-base (cdr err)))
           (error (cons 'caught-generic (cdr err))))
        (condition-case err (signal 'probe-specific-err '("d2"))
           (probe-base-err (cons 'caught-base (cdr err))))
        (get 'probe-specific-err 'error-conditions)
        (get 'probe-specific-err 'error-message)
        (get 'probe-base-err 'error-conditions)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((caught-specific \"detail\") (caught-base \"d2\") (probe-specific-err probe-base-err error) \"specific error\" (probe-base-err error))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_signal_data_formats_error_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (condition-case err (signal 'wrong-type-argument '(integerp stringp))
         (wrong-type-argument (cdr err)))
      (condition-case err (signal 'error '("plain message"))
         (error (cdr err)))
      (condition-case err (signal 'error '("multi" "part"))
         (error (cdr err)))
      (condition-case err (signal 'error 'no-list-data)
         (error (cdr err)))
      (condition-case err (error "convenience")
         (error (cdr err)))
      (condition-case err (signal 'arith-error nil)
         (arith-error (cdr err))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((integerp stringp) (\"plain message\") (\"multi\" \"part\") no-list-data (\"convenience\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_error_message_string_hierarchy_walk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (define-error 'probe-msg-l1 "level one")
  (define-error 'probe-msg-l2 "level two" 'probe-msg-l1)
  (define-error 'probe-msg-l3 "level three" 'probe-msg-l2)
  (list (condition-case err (signal 'probe-msg-l3 '("deep"))
           (probe-msg-l1 (cons 'caught-at-l1 (cdr err))))
        (error-message-string '(probe-msg-l3 "deep"))
        (error-message-string '(wrong-type-argument integerp stringp))
        (error-message-string '(error "x"))
        (member 'probe-msg-l1 (get 'probe-msg-l3 'error-conditions))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((caught-at-l1 \"deep\") \"level three: \\\"deep\\\"\" \"Wrong type argument: integerp, stringp\" \"x\" (probe-msg-l1 error))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

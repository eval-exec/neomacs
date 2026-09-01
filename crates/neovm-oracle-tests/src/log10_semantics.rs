//! Oracle parity tests for GNU `subr.el` `log10`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_log10_delegates_to_log_base_10() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:log10 is an obsolete wrapper whose behavior is exactly
    // `(log X 10)`, including float results and inherited type/domain errors.
    let form = r#"
(let ((inputs '(1 10 100 1000 0.1 0.01 2 2.5)))
  (list
   (mapcar (lambda (x)
             (let ((a (log10 x))
                   (b (log x 10)))
               (list (floatp a) (= a b) a)))
           inputs)
   (condition-case err
       (log10 "x")
     (error (car err)))
   (condition-case err
       (log10)
     (error (car err)))
   (condition-case err
       (log10 1 2)
     (error (car err)))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((t t 0.0) (t t 1.0) (t t 2.0) (t t 3.0) (t t -1.0) (t t -2.0) (t t 0.3010299956639812) (t t 0.3979400086720376)) wrong-type-argument wrong-number-of-arguments wrong-number-of-arguments)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

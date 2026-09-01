//! Oracle parity tests for GNU `subr.el` `apply-partially`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_apply_partially_fixed_args_and_late_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:apply-partially returns a closure that applies FUN to
    // (append fixed-args later-args).  Fixed argument forms are evaluated at
    // closure creation, not at each later call.
    let form = r#"(let* ((trace nil)
        (f (lambda (&rest xs)
             (push xs trace)
             (copy-sequence xs)))
        (p (apply-partially f
                            (progn (push 'fixed-eval trace)
                                   (list 'fixed))
                            'b)))
   (list
    (funcall p 'c 'd)
    (funcall p)
    (nreverse trace)
    (functionp p)
    (condition-case e
        (funcall (apply-partially (lambda (a) a) 1 2))
      (error (list 'error (car e))))))"#;
    let expect = expect_test::expect![
        r#""OK (((fixed) b c d) ((fixed) b) (fixed-eval ((fixed) b c d) ((fixed) b)) t (error wrong-number-of-arguments))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

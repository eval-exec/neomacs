use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_gdl_random_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (condition-case e (random (- (expt 2 100))) (error e)) (mapcar (lambda (lim) (let ((bad nil)) (dotimes (_ 128) (let ((r (random lim))) (unless (and (integerp r) (>= r 0) (< r lim)) (setq bad t)))) bad)) (list (expt 2 61) (expt 2 100) (* 3 (expt 2 64)))))"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((args-out-of-range -1267650600228229401496703205376) (nil nil nil))""#
        ]],
    );
}

#[test]
fn oracle_gdl_random_bignum_seeded_limb_sampling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (random "gdl-random")
  (mapcar (lambda (lim) (mapcar #'number-to-string (list (random lim) (random lim) (random lim) (random lim))))
          (list (expt 2 61) (expt 2 100) (* 3 (expt 2 64)) (1- (expt 2 128)))))
"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((0 0 0 0) (851761947114617447083013513859 224482528918756716904484870998 907658301231368906912288302539 126245061500818130435122957295) (20260365759028427471 54495161855190674693 30200319500665590818 12634134784309366070) (247158623233273134927031080400287539005 97807751809870584888633930788066213863 151307448816625400767657522736519148629 151214638439053400542179782939780817596))""#
        ]],
    );
}

//! Strict combo oracle probes, batch 226: calc math evaluation. calc-eval over
//! arithmetic, powers, roots, modulo, rationals, gcd, and algebraic functions.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_calc_eval_arithmetic_modulo_rational() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'calc)
(list (calc-eval "1+2")
      (calc-eval "2^10")
      (calc-eval "17 mod 5")
      (calc-eval "1/2 + 1/3")
      (calc-eval "7 * 8 - 3")
      (calc-eval "2^64")
      (calc-eval "3!")
      (calc-eval "100 / 7"))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"3\" \"1024\" \"2 mod 5\" \"0.833333333333\" \"53\" \"18446744073709551616\" \"6\" \"14.2857142857\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_calc_eval_sqrt_abs_gcd_algebraic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'calc)
(list (calc-eval "sqrt(16)")
      (calc-eval "sqrt(2)")
      (calc-eval "abs(-5)")
      (calc-eval "gcd(12, 18)")
      (calc-eval "lcm(4, 6)")
      (calc-eval "max(3, 7, 2)")
      (calc-eval "min(3, 7, 2)")
      (calc-eval "exp(0)"))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"4\" \"1.41421356237\" \"5\" \"6\" \"12\" \"7\" \"2\" \"1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_calc_eval_trig_and_constants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'calc)
(list (calc-eval "sin(0)")
      (calc-eval "cos(0)")
      (calc-eval "2 * pi")
      (calc-eval "log10(1000)")
      (calc-eval "ln(1)")
      (calc-eval "10^3")
      (calc-eval "log(8, 2)"))
"##;
    let expect =
        expect_test::expect![[r#""OK (\"0\" \"1\" \"2 pi\" \"3\" \"0\" \"1000\" \"3\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

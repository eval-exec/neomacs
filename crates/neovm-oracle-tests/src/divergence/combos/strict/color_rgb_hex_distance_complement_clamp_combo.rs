//! Strict combo oracle probes, batch 310: color math. color-rgb-to-hex,
//! color-name-to-rgb, color-distance, color-complement, color-clamp,
//! color-srgb-to-xyz, and color-supported-p.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_color_rgb_hex_name_distance_complement() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (color-rgb-to-hex 1.0 0.0 0.0 2)
      (color-rgb-to-hex 0.5 0.25 0.75)
      (car (color-name-to-rgb "red"))
      (color-distance "red" "blue")
      (color-distance "white" "black")
      (color-complement "red")
      (color-supported-p "red")
      (color-supported-p "notacolor"))
"##;
    let expect = expect_test::expect![[
        r##""OK (\"#ff0000\" \"#7fff3fffbfff\" 1.0 327669 589805 (0.0 1.0 1.0) t nil)""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_color_clamp_srgb_to_lab_hsl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (color-clamp 1.5)
      (color-clamp -0.5)
      (color-clamp 0.5)
      (color-srgb-to-xyz 1.0 1.0 1.0)
      (color-srgb-to-lab 1.0 1.0 1.0)
      (length (color-rgb-to-hsl 1.0 0.0 0.0))
      (car (color-rgb-to-hsl 1.0 0.0 0.0)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function color-clamp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_color_defined_values_gray_p_nearest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (color-defined-p "red")
      (color-defined-p "notacolor")
      (consp (color-values "red"))
      (color-gray-p "black")
      (color-gray-p "red")
      (integerp (length (defined-colors)))
      (memq "red" (defined-colors)))
"##;
    let expect = expect_test::expect![[r#""OK (t nil t t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Strict combo oracle probes, batch 91: font attribute canonicalization deep
//! dive — weight/slant/width via font-face-attributes for bold/light/heavy/
//! italic/oblique/condensed/expanded. The 'normal→'regular weight gap was
//! surfaced in batch 90; these probe whether other attributes also diverge.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_q5_font_weight_canonicalization_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:weight bold) (:weight light) (:weight black) (:weight extra-bold) (:weight semi-light) (:weight medium))""#
    ]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK ((:weight bold) (:weight light) (:weight black) (:weight extra-bold) (:weight semi-light) (:weight medium))
    // Neomacs:   OK ((:weight bold) (:weight light) (:weight heavy) (:weight ultra-bold) (:weight semi-light) (:weight medium))
    // font-face-attributes weight canonicalization diverges: GNU maps 'heavy to
    // 'black and 'ultra-bold to 'extra-bold (canonical names), Neomacs keeps
    // the aliases. (Same root cause as 'normal→'regular in batch 90.)
    // bold/light/semi-light/medium match.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (font-face-attributes (font-spec :weight 'bold))
      (font-face-attributes (font-spec :weight 'light))
      (font-face-attributes (font-spec :weight 'heavy))
      (font-face-attributes (font-spec :weight 'ultra-bold))
      (font-face-attributes (font-spec :weight 'semi-light))
      (font-face-attributes (font-spec :weight 'medium)))
"##,
        expect,
    );
}

#[test]
fn div_q5_font_slant_and_width_canonicalization() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:slant italic) (:slant oblique) (:slant normal) (:width condensed) (:width expanded) (:width normal))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (font-face-attributes (font-spec :slant 'italic))
      (font-face-attributes (font-spec :slant 'oblique))
      (font-face-attributes (font-spec :slant 'normal))
      (font-face-attributes (font-spec :width 'condensed))
      (font-face-attributes (font-spec :width 'expanded))
      (font-face-attributes (font-spec :width 'normal)))
"##,
        expect,
    );
}

#[test]
fn div_q5_font_numeric_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"invalid font property\" (:weight . 100))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (font-face-attributes (font-spec :weight 100))
      (font-face-attributes (font-spec :weight 400))
      (font-face-attributes (font-spec :weight 700))
      (font-face-attributes (font-spec :weight 900))
      (font-face-attributes (font-spec :slant 0))
      (font-face-attributes (font-spec :slant 200)))
"##,
        expect,
    );
}

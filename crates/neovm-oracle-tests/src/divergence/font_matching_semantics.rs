//! Font selection inputs that are observable without a graphical display.
//!
//! Native enumeration, fallback, and frame realization are exercised by the
//! graphical font fixture. These probes keep absent style constraints distinct
//! from explicit styles and pin GNU's canonical weight names.

use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};
use expect_test::expect;

#[test]
fn font_matching_semantics_unspecified_and_explicit_styles() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity_expect(
        r#"(mapcar (lambda (s)
  (mapcar (lambda (p) (font-get s p)) '(:weight :slant :width)))
 (list (font-spec :family "Mplus 1 code")
       (font-spec :family "Mplus 1 code" :weight 'thin)
       (font-spec :family "Mplus 1 code" :weight 'normal)
       (font-spec :family "Mplus 1 code" :weight 'bold)))"#,
        expect![[r#""OK ((nil nil nil) (thin nil nil) (normal nil nil) (bold nil nil))""#]],
    );
}

#[test]
fn font_matching_semantics_canonical_weight_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity_expect(
        r#"(mapcar (lambda (w) (font-face-attributes (font-spec :weight w)))
 '(thin ultra-light extra-light light semi-light normal regular medium bold heavy))"#,
        expect![[
            r#""OK ((:weight thin) (:weight ultra-light) (:weight ultra-light) (:weight light) (:weight semi-light) (:weight regular) (:weight regular) (:weight medium) (:weight bold) (:weight black))""#
        ]],
    );
}

#[test]
fn font_matching_semantics_named_style_constraints() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity_expect(
        r#"(mapcar (lambda (entry)
  (let ((s (font-spec :name (car entry))))
    (list (equal (symbol-name (font-get s :family)) (cadr entry))
          (= (font-get s :size) 14)
          (font-get s :weight)
          (font-get s :slant)
          (font-get s :width))))
 '(("Mplus 1 code-14" "Mplus 1 code")
   ("Mplus 1 code-14:weight=thin" "Mplus 1 code")
   ("DejaVu Sans Mono-14:weight=bold" "DejaVu Sans Mono")))"#,
        expect![[r#""OK ((t t nil nil nil) (t t thin nil nil) (t t bold nil nil))""#]],
    );
}

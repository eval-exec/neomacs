//! Strict combo oracle probes, batch 208: window body geometry across margin
//! configurations, characterizing the window-body-width margin divergence
//! from batch 205. window-body-width with zero / left-only / both margins,
//! window-body-height with margins, and the 'pixel variant.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_body_width_across_margin_configs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (set-window-margins w 0 0)
  (let ((zero (window-body-width w)))
    (set-window-margins w 3 0)
    (let ((left-only (window-body-width w)))
      (set-window-margins w 3 2)
      (let ((both (window-body-width w))
            (pixel (window-body-width w 'pixel)))
        (set-window-margins w 0 0)
        (list zero left-only both pixel)))))
"##;
    let expect = expect_test::expect![[r#""OK (80 77 75 75)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_body_height_margins_total_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (let ((total-h (window-total-height w))
        (body-h (window-body-height w))
        (total-w (window-total-width w)))
    (set-window-margins w 2 2)
    (let ((body-h-after-margin (window-body-height w)))
      (set-window-margins w 0 0)
      (list total-h body-h total-w body-h-after-margin))))
"##;
    let expect = expect_test::expect![[r#""OK (24 23 80 23)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_pixel_width_text_pixel_size_count_screen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (let ((pw (window-pixel-width w))
        (ph (window-pixel-height w)))
    (set-window-margins w 5 5)
    (let ((pw-after (window-pixel-width w)))
      (set-window-margins w 0 0)
      (list pw ph pw-after (window-body-width w) (eq pw pw-after)))))
"##;
    let expect = expect_test::expect![[r#""OK (80 24 80 80 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

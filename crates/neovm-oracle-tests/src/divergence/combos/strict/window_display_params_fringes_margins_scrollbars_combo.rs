//! Strict combo oracle probes, batch 205: window display parameters.
//! set-window-fringes / window-fringes, set-window-margins / window-margins,
//! and set-window-scroll-bars / window-scroll-bars round-trips.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_fringes_margins_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (set-window-fringes w 8 8 nil)
  (set-window-margins w 4 2)
  (list (window-fringes w)
        (window-margins w)
        (window-pixel-width w)
        (window-body-width w)
        (progn (set-window-margins w 0) (window-margins w))
        (progn (set-window-fringes w 0 0) (window-fringes w))))
"##;
    let expect =
        expect_test::expect![[r#""OK ((0 0 nil nil) (4 . 2) 80 74 (nil) (0 0 nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_scroll_bars_hscroll_vscroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (set-window-hscroll w 3)
  (set-window-vscroll w 2)
  (list (window-hscroll w)
        (window-vscroll w)
        (window-scroll-bars w)
        (progn (set-window-hscroll w 0) (window-hscroll w))
        (progn (set-window-vscroll w 0) (window-vscroll w))))
"##;
    let expect = expect_test::expect![[r#""OK (3 0 (nil 0 t nil 0 t nil) 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_edges_body_pixel_geometry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (list (window-edges w)
        (window-body-edges w)
        (window-inside-edges w)
        (window-pixel-edges w)
        (window-safe-p w)
        (window-total-size w)
        (window-body-height w 'pixel)
        (window-dedicated-p w)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function window-safe-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

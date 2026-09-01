//! Strict combo oracle probes, batch 368: window scroll-bar + fringe
//! behavioral. set-window-scroll-bars/window-scroll-bars,
//! set-window-fringes/window-fringes round-trips.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_scroll_bar_fringe_set_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (set-window-fringes w 8 8 nil)
  (set-window-scroll-bars w 'right nil)
  (let ((fr (window-fringes w))
        (sb (window-scroll-bars w)))
    (set-window-fringes w 0 0)
    (set-window-scroll-bars w nil 0)
    (list fr sb (window-fringes w) (window-scroll-bars w))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((0 0 nil nil) (nil 0 t nil 0 t nil) (0 0 nil nil) (nil 0 t nil 0 t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_margins_asymmetric_set_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (set-window-margins w 5 3)
  (let ((m1 (window-margins w)))
    (set-window-margins w 0)
    (list m1 (window-margins w))))
"##;
    let expect = expect_test::expect![[r#""OK ((5 . 3) (nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_hscroll_vscroll_set_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (set-window-hscroll w 5)
  (set-window-vscroll w 2 nil)
  (let ((hs (window-hscroll w))
        (vs (window-vscroll w)))
    (set-window-hscroll w 0)
    (set-window-vscroll w 0)
    (list hs vs (window-hscroll w) (window-vscroll w))))
"##;
    let expect = expect_test::expect![[r#""OK (5 0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

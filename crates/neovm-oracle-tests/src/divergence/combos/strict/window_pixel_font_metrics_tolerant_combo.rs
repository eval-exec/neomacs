//! Strict combo oracle probes, batch 314: window pixel / font metrics. We assert
//! shape (integerp / >0 / consistency) rather than exact pixel values, since
//! font/frame pixel sizes are rendering-dependent between the two builds.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_pixel_width_height_font_metrics_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (list (integerp (window-pixel-width w))
        (integerp (window-pixel-height w))
        (integerp (window-body-height w 'pixel))
        (integerp (window-body-width w 'pixel))
        (> (window-pixel-width w) 0)
        (> (window-pixel-height w) 0)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_default_font_width_line_pixel_height_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (integerp (default-font-width))
      (integerp (default-font-height))
      (integerp (default-line-height))
      (> (default-font-width) 0)
      (> (default-font-height) 0)
      (> (default-line-height) 0)
      (= (default-line-height) (default-font-height)))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_text_pixel_size_count_screen_lines_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-metric*")))
  (unwind-protect
      (with-current-buffer b
        (erase-buffer)
        (insert "line one\nline two\nline three\n")
        (let ((w (selected-window)))
          (set-window-buffer w b)
          (list (integerp (count-screen-lines (point-min) (point-max)))
                (> (count-screen-lines (point-min) (point-max)) 0)
                (integerp (window-text-pixel-size w (window-start w) (window-end w) 200))
                (integerp (line-pixel-height)))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

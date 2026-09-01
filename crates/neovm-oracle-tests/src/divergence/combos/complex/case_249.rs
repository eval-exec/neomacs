//! Complex combo batch 249 — `window` display engine deep:
//! `window-text-width`/`window-text-height` consistency,
//! `window-resize` availability, `window-pixel-edges` precision,
//! `window-body-width`/`window-body-height` in pixels vs chars,
//! `set-window-buffer` preserving point, `window-point` persistence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx249_window_text_width_height_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (integerp (window-text-width win))
        (integerp (window-text-height win))
        (integerp (window-body-width win))
        (integerp (window-body-height win))
        (>= (window-text-width win) 0)
        (>= (window-text-height win) 0)))
"##,
        expect,
    )
}

#[test]
fn div_cx249_window_pixel_edges_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (let ((pe (window-pixel-edges win))
        (ipe (window-inside-pixel-edges win))
        (ape (window-absolute-pixel-edges win)))
    (list (consp pe) (= (length pe) 4)
          (consp ipe) (= (length ipe) 4)
          (consp ape) (= (length ape) 4))))
"##,
        expect,
    )
}

#[test]
fn div_cx249_window_body_width_pixels_vs_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (let ((cw (window-body-width win))
        (pw (window-body-width win 'pixels))
        (ch (window-body-height win))
        (ph (window-body-height win 'pixels)))
    (list (integerp cw) (integerp pw)
          (integerp ch) (integerp ph)
          (> pw (* cw 2))
          (> ph (* ch 2)))))
"##,
        expect,
    )
}

#[test]
fn div_cx249_window_resize_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'window-resize)
      (fboundp 'window-resize-no-error)
      (fboundp 'adjust-window-trailing-edge)
      (fboundp 'fit-window-to-buffer)
      (fboundp 'shrink-window-if-larger-than-buffer))
"##,
        expect,
    )
}

#[test]
fn div_cx249_set_window_buffer_preserving_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx249-a*"))
      (buf-b (get-buffer-create " *neo-cx249-b*"))
      (win (selected-window)))
  (with-current-buffer buf-a (insert "AAAA") (goto-char 3))
  (with-current-buffer buf-b (insert "BBBBBBBB") (goto-char 5))
  (set-window-buffer win buf-a)
  (let ((pt-a (window-point win)))
    (set-window-buffer win buf-b)
    (let ((pt-b (window-point win)))
      (set-window-buffer win buf-a)
      (let ((pt-a-again (window-point win)))
        (kill-buffer buf-a)
        (kill-buffer buf-b)
        (list pt-a pt-b pt-a-again (eq pt-a pt-a-again)))))
"##,
        expect,
    )
}

#[test]
fn div_cx249_window_point_vs_buffer_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx249-wp*")))
  (with-current-buffer buf (insert "0123456789ABCDEF"))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 10)
  (let ((wp (window-point (selected-window)))
        (bp (with-current-buffer buf (point))))
    (kill-buffer buf)
    (list wp bp)))
"##,
        expect,
    )
}

#[test]
fn div_cx249_window_start_set_and_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx249-ws*")))
  (with-current-buffer buf
    (insert (mapconcat #'identity (make-list 50 "content line") "\n")))
  (set-window-buffer (selected-window) buf)
  (set-window-start (selected-window) 100)
  (let ((s1 (window-start)))
    (set-window-start (selected-window) 200)
    (let ((s2 (window-start)))
      (kill-buffer buf)
      (list s1 s2 (> s2 s1))))
"##,
        expect,
    )
}

#[test]
fn div_cx249_window_fringes_margins_scrollbar_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (let ((fringes (window-fringes win))
        (margins (window-margins win))
        (sb (window-scroll-bar-width win)))
    (list (consp fringes) (consp margins)
          (integerp sb) (>= sb 0))))
"##,
        expect,
    )
}

#[test]
fn div_cx249_window_max_chars_per_line_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((mc (window-max-chars-per-line))
      (tw (window-text-width)))
  (list (integerp mc) (> mc 0)
        (integerp tw) (> tw 0)
        (>= mc tw)))
"##,
        expect,
    )
}

#[test]
fn div_cx249_window_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Window display mega test buffer content here for testing")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 15))
          (ov (make-overlay 5 25)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 35)
      (let ((state (list (window-text-width win)
                         (window-text-height win)
                         (window-body-width win 'pixels)
                         (window-body-height win 'pixels)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))
"##,
        expect,
    )
}

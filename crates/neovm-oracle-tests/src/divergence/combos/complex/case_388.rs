//! Complex combo batch 388 — `window`/`frame`/`display` pixel ultimate:
//! window-text-pixel-width/height, window-body-width/height pixels,
//! window-pixel-edges, frame-pixel-width/height, frame-char-width/height,
//! display-pixel-dimensions, window-max-chars-per-line, posn-at-point.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx388_window_text_pixel_dimensions_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-text-pixel-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (integerp (window-text-pixel-width win))
        (integerp (window-text-pixel-height win))
        (integerp (window-body-width win))
        (integerp (window-body-height win))
        (integerp (window-body-width win 'pixels))
        (integerp (window-body-height win 'pixels))
        (integerp (window-max-chars-per-line))))
"##,
        expect,
    )
}

#[test]
fn div_cx388_window_pixel_edges_full() {
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
fn div_cx388_frame_pixel_dimensions_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t (0 0 80 24) (0 0 80 23))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (integerp (frame-pixel-width frame))
        (integerp (frame-pixel-height frame))
        (integerp (frame-char-width frame))
        (integerp (frame-char-height frame))
        (integerp (frame-text-width frame))
        (integerp (frame-text-height frame))
        (window-edges (selected-window))
        (window-inside-edges (selected-window))))
"##,
        expect,
    )
}

#[test]
fn div_cx388_display_info_full_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil t t 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (integerp (display-pixel-width))
        (integerp (display-pixel-height))
        (integerp (display-mm-width))
        (integerp (display-mm-height))
        (integerp (display-color-cells))
        (integerp (display-planes))
        (display-screens)
        (display-graphic-p)))
"##,
        expect,
    )
}

#[test]
fn div_cx388_window_margins_fringes_scrollbar() {
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
fn div_cx388_set_window_margins_fringes_hscroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((nil) (0 0 nil nil) (12 . 4) (0 0 nil nil) 5 (nil) (0 0 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (let ((m0 (window-margins win))
        (f0 (window-fringes win)))
    (set-window-margins win 12 4)
    (set-window-fringes win 16 8 t)
    (set-window-hscroll win 5)
    (let ((m1 (window-margins win))
          (f1 (window-fringes win))
          (h1 (window-hscroll win)))
      (set-window-margins win (car m0) (cdr m0))
      (set-window-fringes win (car f0) (cadr f0) (car (cddr f0)))
      (set-window-hscroll win 0)
      (list m0 f0 m1 f1 h1
            (window-margins win)
            (window-fringes win)))))
"##,
        expect,
    )
}

#[test]
fn div_cx388_window_start_end_set_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx388-ws*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert (mapconcat #'identity (make-list 50 "line of content") "\n")))
  (set-window-buffer (selected-window) buf)
  (set-window-start (selected-window) 100)
  (let ((start (window-start))
        (end (window-end nil t)))
    (set-window-start (selected-window) 200)
    (let ((start2 (window-start)))
      (kill-buffer buf)
      (list (integerp start) (integerp end) (> end start) (> start2 start))))
"##,
        expect,
    )
}

#[test]
fn div_cx388_posn_at_point_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil (0 . 0) nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "first line\nsecond line\nthird line\n")
  (goto-char 15)
  (let ((posn (posn-at-point (point))))
    (list (posn-point posn)
          (posn-col-row posn)
          (posn-actual-col-row posn)
          (posn-window posn)
          (posn-area posn)
          (posn-object posn))))
"##,
        expect,
    )
}

#[test]
fn div_cx388_window_dedicated_and_vscroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (set-window-vscroll win 10 t)
  (let ((v1 (window-vscroll win t)))
    (set-window-vscroll win 0 t)
    (let ((v2 (window-vscroll win t)))
      (let ((ded-before (window-dedicated-p win)))
        (set-window-dedicated-p win t)
        (let ((ded-true (window-dedicated-p win)))
          (set-window-dedicated-p win nil)
          (let ((ded-false (window-dedicated-p win)))
            (list v1 v2 ded-before ded-true ded-false)))))))
"##,
        expect,
    )
}

#[test]
fn div_cx388_window_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-text-pixel-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (set-window-parameter win 'neo-cx388-mega :win-val)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Window pixel ultimate mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 15))
          (ov (make-overlay 5 25)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (set-window-start (selected-window) 1)
      (narrow-to-region 2 40)
      (let ((state (list (window-parameter win 'neo-cx388-mega)
                         (window-text-pixel-width win)
                         (window-text-pixel-height win)
                         (window-body-width win 'pixels)
                         (window-body-height win 'pixels)
                         (window-hscroll)
                         (window-start)
                         (window-dedicated-p win)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (set-window-parameter win 'neo-cx388-mega nil)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}

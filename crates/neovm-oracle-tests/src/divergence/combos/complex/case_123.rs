//! Complex combo batch 123 — `frame` / `window` parameter persistence,
//! frame fonts, display info, monitor attributes, and multi-monitor
//! geometry queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx123_frame_parameters_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"F1\" nil nil 80 25 dark mono)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (frame-parameter frame 'name)
        (frame-parameter frame 'left)
        (frame-parameter frame 'top)
        (frame-parameter frame 'width)
        (frame-parameter frame 'height)
        (frame-parameter frame 'background-mode)
        (frame-parameter frame 'display-type)))
"##,
        expect,
    );
}

#[test]
fn div_cx123_frame_total_size_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#<window 1 on *scratch*> #<window 1 on *scratch*> #<window 1 on *scratch*> t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (frame-root-window frame)
        (frame-selected-window frame)
        (frame-first-window frame)
        (frame-parameter frame 'minibuffer)
        (eq frame (window-frame (selected-window)))))
"##,
        expect,
    );
}

#[test]
fn div_cx123_display_color_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil 80 25 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (display-color-cells frame)
        (display-grayscale-p frame)
        (display-pixel-width frame)
        (display-pixel-height frame)
        (display-mm-width frame)
        (display-mm-height frame)))
"##,
        expect,
    );
}

#[test]
fn div_cx123_monitor_attributes_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil (mm-size nil nil) (workarea 0 0 80 25))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((monitors (display-monitor-attributes-list)))
      (list (consp monitors)
            (assq 'geom (car monitors))
            (assq 'mm-size (car monitors))
            (assq 'workarea (car monitors))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx123_frame_live_and_visible_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (frame-live-p frame)
        (frame-visible-p frame)
        (eq frame (selected-frame))))
"##,
        expect,
    );
}

#[test]
fn div_cx123_modify_frame_parameters_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (let ((before (frame-parameter frame 'neo-cx123-custom)))
    (modify-frame-parameters frame '((neo-cx123-custom . "value-1")))
    (let ((v1 (frame-parameter frame 'neo-cx123-custom)))
      (modify-frame-parameters frame '((neo-cx123-custom . "value-2")))
      (let ((v2 (frame-parameter frame 'neo-cx123-custom)))
        (modify-frame-parameters frame '((neo-cx123-custom)))  ; remove
        (list before v1 v2 (frame-parameter frame 'neo-cx123-custom)))))
"##,
        expect,
    );
}

#[test]
fn div_cx123_frame_pixel_size_vs_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 80 25 80 25)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (frame-parameter frame 'width)
        (frame-text-width frame)
        (frame-text-height frame)
        (frame-pixel-width frame)
        (frame-pixel-height frame)))
"##,
        expect,
    );
}

#[test]
fn div_cx123_window_buffer_pixel_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-text-pixel-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (window-pixel-width win)
        (window-pixel-height win)
        (window-text-pixel-width win)
        (window-text-pixel-height win)
        (window-text-width win)
        (window-text-height win)))
"##,
        expect,
    );
}

#[test]
fn div_cx123_frame_char_width_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function frame-column-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (frame-char-width frame)
        (frame-char-height frame)
        (frame-column-width frame)
        (frame-line-height frame)))
"##,
        expect,
    );
}

#[test]
fn div_cx123_frame_font_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"tty\" nil 0 (font . \"tty\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (frame-parameter frame 'font)
        (frame-parameter frame 'font-backend)
        (frame-parameter frame 'line-spacing)
        (assq 'font (frame-parameters frame))))
"##,
        expect,
    );
}

#[test]
fn div_cx123_window_total_size_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 24 80 23 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (window-total-width win)
        (window-total-height win)
        (window-body-width win)
        (window-body-height win)
        (window-new-total win)
        (window-new-pixel win)))
"##,
        expect,
    );
}

#[test]
fn div_cx123_frame_window_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (modify-frame-parameters frame '((neo-cx123-mega . "value-1")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Frame/window mega test buffer content")
    (put-text-property 1 7 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (frame-parameter frame 'neo-cx123-mega)
                         (window-pixel-width (selected-window))
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (modify-frame-parameters frame '((neo-cx123-mega)))
        (list state (frame-parameter frame 'neo-cx123-mega)
              (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

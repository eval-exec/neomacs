//! Complex combo batch 182 — `frame` / `display` / `terminal` /
//! `tty-type` / `display-color-cells` / `display-planes` / `display-screens`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx182_display_info_matrix() {
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
    );
}

#[test]
fn div_cx182_terminal_info_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function terminalp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame))
      (terminal (frame-terminal (selected-frame))))
  (list (terminalp terminal)
        (terminal-live-p terminal)
        (stringp (terminal-name terminal))
        (eq (frame-terminal frame) terminal)))
"##,
        expect,
    );
}

#[test]
fn div_cx182_frame_live_and_visible_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (frame-live-p frame)
        (frame-visible-p frame)
        (eq (selected-frame) frame)))
"##,
        expect,
    );
}

#[test]
fn div_cx182_frame_pixel_size_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (integerp (frame-pixel-width frame))
        (integerp (frame-pixel-height frame))
        (integerp (frame-char-width frame))
        (integerp (frame-char-height frame))
        (integerp (frame-text-width frame))
        (integerp (frame-text-height frame))))
"##,
        expect,
    );
}

#[test]
fn div_cx182_modify_frame_parameters_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (let ((before (frame-parameter frame 'neo-cx182-param)))
    (modify-frame-parameters frame '((neo-cx182-param . "value-1")))
    (let ((v1 (frame-parameter frame 'neo-cx182-param)))
      (modify-frame-parameters frame '((neo-cx182-param . "value-2")))
      (let ((v2 (frame-parameter frame 'neo-cx182-param)))
        (modify-frame-parameters frame '((neo-cx182-param)))
        (list before v1 v2 (frame-parameter frame 'neo-cx182-param)))))
"##,
        expect,
    );
}

#[test]
fn div_cx182_display_grayscale_p_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (fboundp 'display-grayscale-p)
        (when (fboundp 'display-grayscale-p) (display-grayscale-p frame))))
"##,
        expect,
    );
}

#[test]
fn div_cx182_display_supports_p_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((frame (selected-frame)))
      (list (display-supports-face-attributes-p '(:foreground "red") frame)
            (display-supports-face-attributes-p '(:underline t) frame)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx182_frame_focus_state_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (fboundp 'select-frame-set-input-focus)
        (fboundp 'frame-focus)
        (eq (window-frame (selected-window)) frame)))
"##,
        expect,
    );
}

#[test]
fn div_cx182_tty_display_dimensions_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'tty-display-color-p)
          (fboundp 'tty-display-dimensional-p)
          (fboundp 'tty-no-underline))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx182_frame_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame))
      (terminal (frame-terminal (selected-frame))))
  (modify-frame-parameters frame '((neo-cx182-mega . "value")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Frame/display mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (frame-parameter frame 'neo-cx182-mega)
                         (terminal-name terminal)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (modify-frame-parameters frame '((neo-cx182-mega)))
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

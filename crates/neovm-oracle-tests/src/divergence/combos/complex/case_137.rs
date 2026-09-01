//! Complex combo batch 137 — `ansi-color` / `colors` / `rgb` / `color-name`
//! / `xterm-color` parsing and conversion.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx137_ansi_color_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ansi-color)
      (list (fboundp 'ansi-color-apply)
            (fboundp 'ansi-color-filter-apply)
            (boundp 'ansi-color-regexp)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_ansi_color_apply_basic_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((input "Hello \x1b[31mWorld\x1b[0m end"))
      (let ((result (ansi-color-apply input)))
        (list (stringp result)
              (get-text-property 6 'face result)
              (get-text-property 0 'face result))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_color_name_to_rgb() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((65535 0 0) (0 65535 0) (0 0 65535) (0 0 0) (65535 65535 65535) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (color-values "red")
          (color-values "green")
          (color-values "blue")
          (color-values "black")
          (color-values "white")
          (color-values "invalidcolor"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_color_rgb_to_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#feff010000\" \"#00feff0100\" \"#0000feff01\" \"#7f80007f800000\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (color-rgb-to-hex 65535 0 0 2)
          (color-rgb-to-hex 0 65535 0 2)
          (color-rgb-to-hex 0 0 65535 2)
          (color-rgb-to-hex 32768 32768 0 2))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_color_complement() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (color-complement "red")
          (color-complement "white")
          (color-complement "#00ff00"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_color_gradient() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((grad (color-gradient "red" "blue" 5)))
      (list (consp grad)
            (= (length grad) 5)
            (car grad)
            (car (last grad))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_color_distance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (327669 0 589805 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (color-distance "red" "blue")
          (color-distance "red" "red")
          (color-distance "white" "black")
          (color-distance "#808080" "#c0c0c0"))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_color_rgb_to_hsl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (color-rgb-to-hsl 1.0 0.0 0.0)
          (color-rgb-to-hsl 0.0 1.0 0.0)
          (color-rgb-to-hsl 0.5 0.5 0.5)
          (color-rgb-to-hsl 0.0 0.0 0.0))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_ansi_color_strip_codes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((input "Hello \x1b[1;31mBold Red\x1b[0m end"))
      (let ((stripped (ansi-color-filter-apply input)))
        (list stripped
              (length stripped)
              (not (string-match "\x1b" stripped)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_xterm_color_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'xterm-color)
          (fboundp 'xterm-color-colorize-buffer))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_color_supported_p_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'color-supported-p)
          (color-supported-p "red" nil t)
          (color-supported-p "invalidcolor" nil t))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx137_color_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((rgb (color-values "blue")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Color mega test buffer RGB: %S" rgb))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list rgb
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

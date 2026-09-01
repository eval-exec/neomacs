//! Complex combo batch 258 — `so-long` / `zone` / `display-time` /
//! `display-battery` / `which-key` / `emoji` / `animate` /
//! `life` / `doctor` / `yow` / `spook` toy/game availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx258_so_long_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'so-long)
      (list (fboundp 'so-long)
            (boundp 'so-long-threshold)
            (boundp 'so-long-max-lines)
            (boundp 'so-long-variable-values)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx258_zone_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'zone)
      (list (fboundp 'zone)
            (fboundp 'zone-when-idle)
            (boundp 'zone-programs)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx258_display_time_battery_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'time)
      (list (fboundp 'display-time-mode)
            (fboundp 'display-battery-mode)
            (boundp 'display-time-format)
            (boundp 'display-time-interval)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx258_which_key_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'which-key)
          (fboundp 'which-key-mode)
          (boundp 'which-key-idle-delay)
          (boundp 'which-key-max-description-length))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx258_emoji_insert_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'emoji)
          (fboundp 'emoji-insert)
          (fboundp 'emoji-search)
          (fboundp 'emoji-list)
          (boundp 'emoji--font-set))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx258_animate_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'animate)
      (list (fboundp 'animate-sequence)
            (fboundp 'animate)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx258_life_gomoku_games_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'life)
          (fboundp 'life)
          (featurep 'gomoku)
          (fboundp 'gomoku))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx258_doctor_yow_spook_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'doctor)
          (fboundp 'yow)
          (fboundp 'spook)
          (fboundp 'cookie))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx258_dunnet_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'dunnet)
          (boundp 'dunnet-mode))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx258_toy_games_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((avail (list (fboundp 'doctor)
                   (fboundp 'yow)
                   (featurep 'which-key)
                   (featurep 'emoji)
                   (boundp 'display-time-mode))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Toy/game mega test buffer content")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list avail
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
    )
}

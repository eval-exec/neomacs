//! Complex combo batch 266 — `benchmark-run` / `benchmark-proces` /
//! `face` underline/box/stipple/inverse-video attribute variants deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx266_benchmark_run_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'benchmark)
      (list (fboundp 'benchmark-run)
            (fboundp 'benchmark-proces)
            (fboundp 'benchmark)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx266_benchmark_run_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((result (benchmark-run 10 (+ 1 2 3))))
      (list (consp result)
            (floatp (car result))
            (integerp (cadr result))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx266_face_underline_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (t (:color \"red\" :style wave) (:color \"blue\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx266-ul1 '((t :underline t)) "ul1")
      (defface neo-cx266-ul2 '((t :underline (:color "red" :style wave))) "ul2")
      (defface neo-cx266-ul3 '((t :underline (:color "blue"))) "ul3")
      (list (face-attribute 'neo-cx266-ul1 :underline)
            (face-attribute 'neo-cx266-ul2 :underline)
            (face-attribute 'neo-cx266-ul3 :underline)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx266_face_box_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1 (:color \"red\" :line-width 2) (:color \"blue\" :line-width (2 . 2) :style released-button))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx266-box1 '((t :box t)) "box1")
      (defface neo-cx266-box2 '((t :box (:color "red" :line-width 2))) "box2")
      (defface neo-cx266-box3 '((t :box (:color "blue" :line-width (2 . 2) :style released-button))) "box3")
      (list (face-attribute 'neo-cx266-box1 :box)
            (face-attribute 'neo-cx266-box2 :box)
            (face-attribute 'neo-cx266-box3 :box)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx266_face_inverse_video_and_stipple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx266-inv '((t :inverse-video t)) "inv")
      (list (face-attribute 'neo-cx266-inv :inverse-video)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx266_face_inherit_nil_explicit_disable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx266-parent '((t :foreground "red" :weight bold)) "parent")
      (defface neo-cx266-no-inherit '((t :foreground "blue" :inherit nil)) "no-inherit")
      (list (face-attribute 'neo-cx266-parent :weight)
            (face-attribute 'neo-cx266-no-inherit :weight nil nil nil)
            (face-attribute 'neo-cx266-no-inherit :foreground)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx266_face_all_attributes_complete_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . \"default\") (:height . 1) (:weight . normal) (:slant . normal) (:underline) (:inverse-video) (:stipple))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((attrs (face-all-attributes 'default (selected-frame))))
  (list (assq :family attrs)
        (assq :height attrs)
        (assq :weight attrs)
        (assq :slant attrs)
        (assq :underline attrs)
        (assq :inverse-video attrs)
        (assq :stipple attrs)))
"##,
        expect,
    )
}

#[test]
fn div_cx266_face_foreground_background_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"unspecified-fg\" \"unspecified-bg\" unspecified unspecified)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (face-attribute 'default :foreground frame)
        (face-attribute 'default :background frame)
        (face-attribute 'font-lock-keyword-face :foreground frame)
        (face-attribute 'font-lock-string-face :foreground frame)))
"##,
        expect,
    )
}

#[test]
fn div_cx266_face_id_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx266-id1 '((t)) "id1")
      (defface neo-cx266-id2 '((t)) "id2")
      (list (integerp (face-id 'neo-cx266-id1))
            (integerp (face-id 'neo-cx266-id2))
            (not (= (face-id 'neo-cx266-id1) (face-id 'neo-cx266-id2)))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx266_face_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx266-mega '((t :foreground "purple" :weight bold :underline t)) "mega")
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Face attribute mega test buffer content")
        (put-text-property 1 6 'face 'neo-cx266-mega)
        (put-text-property 8 14 'face 'bold)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'neo-cx266-mega)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 22)
          (let ((state (list (face-attribute 'neo-cx266-mega :weight)
                             (face-attribute 'neo-cx266-mega :foreground)
                             (face-attribute 'neo-cx266-mega :underline)
                             (get-char-property 3 'face)
                             (get-char-property 8 'face)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

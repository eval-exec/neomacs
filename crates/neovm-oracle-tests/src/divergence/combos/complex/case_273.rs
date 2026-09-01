//! Complex combo batch 273 — `isearch-lazy-highlight` / `isearch` face
//! customization / `occur` edit mode / `multi-occur` / `color-comp` /
//! `comint` / `ielm` actual REPL availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx273_isearch_lazy_highlight_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'lazy-highlight-cleanup)
      (boundp 'lazy-highlight-initial-delay)
      (boundp 'lazy-highlight-interval)
      (boundp 'isearch-lazy-highlight)
      (boundp 'search-highlight))
"##,
        expect,
    )
}

#[test]
fn div_cx273_isearch_face_customization() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] unspecified)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (facep 'isearch)
      (facep 'lazy-highlight)
      (facep 'isearch-fail)
      (facep 'match)
      (face-attribute 'isearch :foreground))
"##,
        expect,
    )
}

#[test]
fn div_cx273_occur_edit_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'occur)
      (list (fboundp 'occur-edit-mode)
            (fboundp 'occur-mode-display-occurrence)
            (boundp 'occur-mode-hook)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx273_multi_occur_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'misearch)
      (list (fboundp 'multi-occur)
            (fboundp 'multi-occur-in-matching-buffers)
            (boundp 'multi-occur-buffer-list)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx273_comint_availability_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'comint)
      (list (fboundp 'make-comint)
            (fboundp 'comint-run)
            (boundp 'comint-prompt-read-only)
            (boundp 'comint-buffer-maximum-size)
            (boundp 'comint-scroll-show-maximum-output)
            (boundp 'comint-input-ring-size)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx273_ielm_repl_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ielm)
      (list (fboundp 'ielm)
            (fboundp 'ielm-change-working-buffer)
            (boundp 'ielm-dynamic-return)
            (boundp 'ielm-noisy)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx273_color_component_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'color-rgb-to-hsl)
          (fboundp 'color-hsl-to-rgb)
          (fboundp 'color-complement)
          (fboundp 'color-gradient)
          (fboundp 'color-distance)
          (fboundp 'color-name-to-rgb)
          (fboundp 'color-supported-p))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx273_color_hsl_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (color-rgb-to-hsl 1.0 0.0 0.0)
          (color-rgb-to-hsl 0.0 1.0 0.0)
          (color-rgb-to-hsl 0.0 0.0 1.0)
          (color-rgb-to-hsl 0.5 0.5 0.5)
          (color-rgb-to-hsl 0.0 0.0 0.0))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx273_search_ring_and_regexp_search_ring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'search-ring)
      (consp search-ring)
      (boundp 'regexp-search-ring)
      (consp regexp-search-ring)
      (boundp 'search-ring-max)
      (integerp search-ring-max))
"##,
        expect,
    )
}

#[test]
fn div_cx273_isearch_comint_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((avail (list (boundp 'lazy-highlight-cleanup)
                   (facep 'isearch)
                   (fboundp 'multi-occur)
                   (fboundp 'make-comint)
                   (fboundp 'ielm)
                   (fboundp 'color-rgb-to-hsl))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Isearch/comint/color mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list avail
                         (facep 'isearch)
                         (boundp 'search-ring)
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

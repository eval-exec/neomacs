//! Complex combo batch 263 — `undo-tree` / `undo-fu` / `page-break-lines` /
//! `visual-fill-column` / `form-feed` / `origami` folding availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx263_undo_tree_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'undo-tree)
          (fboundp 'undo-tree-mode)
          (fboundp 'undo-tree-visualize)
          (boundp 'undo-tree-visualizer-timestamps))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx263_undo_fu_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'undo-fu)
          (fboundp 'undo-fu-only-undo)
          (fboundp 'undo-fu-only-redo))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx263_page_break_lines_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'page-break-lines)
          (fboundp 'page-break-lines-mode))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx263_visual_fill_column_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'visual-fill-column)
          (fboundp 'visual-fill-column-mode)
          (boundp 'visual-fill-column-width))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx263_form_feed_display_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'form-feed)
          (fboundp 'form-feed-mode))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx263_origami_folding_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'origami)
          (fboundp 'origami-mode))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx263_consult_vertico_orderless_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (featurep 'consult)
      (fboundp 'consult-line)
      (featurep 'vertico)
      (fboundp 'vertico-mode)
      (featurep 'orderless)
      (boundp 'orderless-style-dispatchers))
"##,
        expect,
    )
}

#[test]
fn div_cx263_marginalia_embark_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (featurep 'marginalia)
      (fboundp 'marginalia-mode)
      (featurep 'embark)
      (fboundp 'embark-act)
      (boundp 'embark-action-indicator))
"##,
        expect,
    )
}

#[test]
fn div_cx263_selectrum_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'selectrum)
          (fboundp 'selectrum-mode))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx263_folding_completion_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((avail (list (featurep 'undo-tree)
                   (featurep 'visual-fill-column)
                   (featurep 'consult)
                   (featurep 'vertico)
                   (featurep 'orderless)
                   (featurep 'marginalia)
                   (featurep 'embark))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Folding/completion mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list avail
                         (boundp 'fill-column)
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

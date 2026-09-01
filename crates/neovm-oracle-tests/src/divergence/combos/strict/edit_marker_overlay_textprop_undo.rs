//! Strict combo oracle probes, batch 319: multi-subsystem combo -- buffer edit
//! + marker tracking + overlay + text-property + undo, all interacting.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_combo_edit_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Hello World")
  (let ((m (set-marker (make-marker) 6))
        (o (make-overlay 1 6)))
    (overlay-put o 'face 'bold)
    (add-text-properties 1 6 '(weight heavy))
    (goto-char 1)
    (insert "PREFIX")
    (undo-boundary)
    (delete-region 1 3)
    (let ((s1 (buffer-string))
          (m1 (marker-position m))
          (o-start (overlay-start o)))
      (undo)
      (list s1 m1 o-start
            (buffer-string)
            (marker-position m)
            (overlay-start o)
            (overlay-end o)
            (get-text-property 1 'weight)))))
"##;
    let expect = expect_test::expect![[
        r#""OK (#(\"EFIXHello World\" 4 9 (weight heavy)) 10 1 \"\" 1 1 1 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_combo_search_replace_match_data_marker_preserve() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "The QUICK brown FOX")
  (let ((m (set-marker (make-marker) 5)))
    (goto-char 1)
    (while (re-search-forward "[A-Z]+" nil t)
      (replace-match (downcase (match-string 0))))
    (list (buffer-string)
          (marker-position m)
          (save-match-data
            (string-match "brown" (buffer-string))
            (match-data)))))
"##;
    let expect = expect_test::expect![[r#""OK (\"The QUICK brown FOX\" 5 (10 15))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_combo_narrow_overlay_marker_textprop_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAAABBBBCCCCDDDD")
  (let ((m (set-marker (make-marker) 8))
        (o (make-overlay 5 12)))
    (overlay-put o 'face 'italic)
    (add-text-properties 5 9 '(face bold))
    (narrow-to-region 5 13)
    (list (point-min)
          (point-max)
          (marker-position m)
          (overlay-start o)
          (overlay-end o)
          (get-text-property (point-min) 'face)
          (buffer-substring (point-min) (point-max)))))
"##;
    let expect =
        expect_test::expect![[r#""OK (5 13 8 5 12 bold #(\"BBBBCCCC\" 0 4 (face bold)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Strict combo oracle probes, batch 138: save-match-data deep combo,
//! overlay categories through delete+undo, kill-region/yank with
//! text-properties, and window-configuration-to-register roundtrip.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v2_save_match_data_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let (outer-match)
  (with-temp-buffer
    (insert "outer inner outer")
    (string-match "outer" "outer inner outer")
    (setq outer-match (match-data t))
    (save-match-data
      (string-match "inner" "outer inner outer")
      (list (match-data t)
            (match-beginning 0)
            (match-end 0)))
    (list outer-match
          (match-data t)
          (match-beginning 0)
          (match-end 0))))
"##;
    let expect = expect_test::expect![[r#""OK ((0 5) (0 5) 0 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v2_overlay_categories_through_delete_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "abcdefghij")
    (let ((o (make-overlay 3 7)))
      (overlay-put o 'category 'probe-cat)
      (overlay-put o 'face 'bold)
      (push (list 'before (overlay-start o) (overlay-end o) (length (overlays-in 1 10))) log)
      (undo-boundary)
      (delete-region 4 6)
      (push (list 'after-delete (overlay-start o) (overlay-end o) (length (overlays-in 1 10))) log)
      (undo)
      (push (list 'after-undo (overlay-start o) (overlay-end o) (overlay-get o 'face) (length (overlays-in 1 10))) log))
    (nreverse log)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((before 3 7 1) (after-delete 3 5 1) (after-undo 1 1 bold 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v2_kill_yank_with_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((kill-ring nil))
  (with-temp-buffer
    (insert "plain ")
    (insert (propertize "bold" 'face 'bold))
    (insert " plain")
    (goto-char 7)
    (kill-word 1)
    (let ((after-kill (buffer-string)))
      (end-of-line)
      (yank)
      (list after-kill
            (buffer-string)
            (get-text-property 6 'face)
            (get-text-property (1- (point)) 'face)
            (car kill-ring))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v2_window_config_register_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((register-alist nil)
      (b1 (get-buffer-create " *probe-wcr-a*"))
      (b2 (get-buffer-create " *probe-wcr-b*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b1)
        (window-configuration-to-register ?w)
        (let ((w2 (split-window nil nil 'right)))
          (set-window-buffer w2 b2)
          (select-window w2))
        (jump-to-register ?w)
        (list (count-windows)
              (eq (selected-window) (frame-selected-window))
              (buffer-name (window-buffer (selected-window)))
              (buffer-live-p b1)
              (buffer-live-p b2)
              (assq ?w register-alist)))
    (kill-buffer b1)
    (kill-buffer b2)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[
        r#""OK (1 t \" *probe-wcr-a*\" t t (119 #<window-configuration> #<marker in no buffer>))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v2_buffer_swap_text_with_overlays_and_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((a (generate-new-buffer " *probe-bst-a*"))
      (b (generate-new-buffer " *probe-bst-b*"))
      (ma nil) (mb nil) (oa nil) (ob nil))
  (unwind-protect
      (progn
        (with-current-buffer a
          (insert "AAAA-content")
          (setq ma (copy-marker 3))
          (setq oa (make-overlay 2 5))
          (overlay-put oa 'face 'bold))
        (with-current-buffer b
          (insert "BBBB-content")
          (setq mb (copy-marker 3))
          (setq ob (make-overlay 2 5))
          (overlay-put ob 'face 'italic))
        (with-current-buffer a (buffer-swap-text b))
        (list (with-current-buffer a (buffer-string))
              (with-current-buffer b (buffer-string))
              (marker-position ma)
              (marker-position mb)
              (with-current-buffer a (overlay-start oa))
              (with-current-buffer b (overlay-start ob))
              (with-current-buffer a (overlay-get oa 'face))
              (with-current-buffer b (overlay-get ob 'face))))
    (kill-buffer a)
    (kill-buffer b)))
"##;
    let expect =
        expect_test::expect![[r#""OK (\"BBBB-content\" \"AAAA-content\" 3 3 2 2 bold italic)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

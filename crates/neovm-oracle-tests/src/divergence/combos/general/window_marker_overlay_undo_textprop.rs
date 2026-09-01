//! Deep combo: window × split-window × delete-window × select-window ×
//! with-selected-window × marker × overlay × text-prop × undo ×
//! buffer-local × narrow × point-marker × window-point.
//!
//! Stresses window interaction with buffer state: window point tracking,
//! window-specific overlays, and buffer-local state across windows.
//! Windows are tricky in a Rust rewrite because each window maintains
//! its own point position that must track markers correctly.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_split_window_point_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 11 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-sw")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (goto-char 12)
        (let* ((win1 (selected-window))
               (win2 (split-window win1 nil 'below)))
          (unwind-protect
              (progn
                (select-window win2)
                (goto-char 20)
                (let ((after (list (buffer-string)
                                   (marker-position m1)
                                   (marker-position m2)
                                   (overlay-start ov) (overlay-end ov)
                                   (window-point win1)
                                   (window-point win2)
                                   (get-text-property 1 'sect)
                                   (get-text-property 11 'sect)
                                   (get-text-property 21 'sect))))
                  (list after)))
            (delete-window win2)
            (kill-buffer buf))))))) "#,
        expect,
    );
}

#[test]
fn combo_with_selected_window_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 6 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-wsw")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let* ((win1 (selected-window))
               (win2 (split-window win1 nil 'below)))
          (unwind-protect
              (with-selected-window win2
                (goto-char 6)
                (insert "XX-")
                (let ((after (list (buffer-string)
                                   (marker-position m1)
                                   (marker-position m2)
                                   (overlay-start ov) (overlay-end ov)
                                   (get-text-property 1 'zone)
                                   (get-text-property 6 'zone))))
                  (primitive-undo 1 buffer-undo-list)
                  (let ((restored (list (buffer-string)
                                        (marker-position m1)
                                        (marker-position m2)
                                        (overlay-start ov) (overlay-end ov)
                                        (get-text-property 1 'zone)
                                        (get-text-property 6 'zone)
                                        (get-text-property 11 'zone))))
                    (list after restored))))
            (delete-window win2)
            (kill-buffer buf))))))) "#,
        expect,
    );
}

#[test]
fn combo_window_buffer_local_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable win-local)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-wbl")))
    (with-current-buffer buf
      (make-local-variable 'win-local)
      (setq win-local 'buffer-specific)
      (insert "HELLO-WORLD")
      (put-text-property 1 6 'word 'hello)
      (put-text-property 7 12 'word 'world)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 6 t))
            (ov (make-overlay 1 12)))
        (overlay-put ov 'scope 'all)
        (let* ((win1 (selected-window))
               (win2 (split-window win1 nil 'below)))
          (unwind-protect
              (progn
                (select-window win2)
                (goto-char 7)
                (let ((after (list win-local
                                   (marker-position m1)
                                   (marker-position m2)
                                   (overlay-start ov) (overlay-end ov)
                                   (window-point win1)
                                   (window-point win2)
                                   (get-text-property 1 'word)
                                   (get-text-property 7 'word))))
                  (list after)))
            (delete-window win2)
            (kill-buffer buf))))))) "#,
        expect,
    );
}

#[test]
fn combo_window_point_tracking_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 6 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-wpt")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (let* ((win1 (selected-window))
               (win2 (split-window win1 nil 'below)))
          (unwind-protect
              (progn
                (goto-char 8)
                (select-window win2)
                (goto-char 18)
                (let ((w1-pt (window-point win1))
                      (w2-pt (window-point win2)))
                  (select-window win1)
                  (goto-char 3)
                  (let ((after (list (marker-position m1)
                                     (marker-position m2)
                                     (overlay-start ov) (overlay-end ov)
                                     w1-pt w2-pt
                                     (window-point win1)
                                     (window-point win2)
                                     (get-text-property 1 'grp)
                                     (get-text-property 6 'grp)
                                     (get-text-property 11 'grp)
                                     (get-text-property 16 'grp))))
                    (list after))))
            (delete-window win2)
            (kill-buffer buf))))))) "#,
        expect,
    );
}

#[test]
fn combo_window_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 6 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-wnar")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 6 20)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (let* ((win1 (selected-window))
               (win2 (split-window win1 nil 'below)))
          (unwind-protect
              (with-selected-window win2
                (narrow-to-region 6 20)
                (goto-char (point-min))
                (insert "XX-")
                (widen)
                (let ((after (list (buffer-string)
                                   (marker-position m1)
                                   (marker-position m2)
                                   (overlay-start ov) (overlay-end ov)
                                   (get-text-property 1 'sect)
                                   (get-text-property 6 'sect)
                                   (get-text-property 16 'sect)
                                   (get-text-property 21 'sect))))
                  (primitive-undo 1 buffer-undo-list)
                  (let ((restored (list (buffer-string)
                                        (marker-position m1)
                                        (marker-position m2)
                                        (overlay-start ov) (overlay-end ov)
                                        (get-text-property 1 'sect)
                                        (get-text-property 6 'sect)
                                        (get-text-property 11 'sect)
                                        (get-text-property 16 'sect)
                                        (get-text-property 21 'sect))))
                    (list after restored))))
            (delete-window win2)
            (kill-buffer buf))))))) "#,
        expect,
    );
}

//! Deep combo: beginning-of-buffer × end-of-buffer ×
//! marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses buffer boundary movement with buffer state: moving to
//! beginning/end of buffer while preserving markers, overlays,
//! text properties, and undo state. Note: recenter/scroll commands
//! are excluded because they require a window displaying the buffer,
//! which is not available in batch mode.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_beginning_end_buffer_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-beb")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (put-text-property 21 25 'grp 'e)
      (put-text-property 26 30 'grp 'f)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t))
            (m3 (copy-marker 25 nil))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char (point-max))
        (let ((pos-end (point)))
          (beginning-of-buffer)
          (let ((pos-beg (point)))
            (end-of-buffer)
            (insert "-TAIL")
            (let ((after (list (buffer-string)
                               pos-beg pos-end
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'grp)
                               (get-text-property 11 'grp)
                               (get-text-property 21 'grp))))
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (marker-position m3)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'grp)
                                    (get-text-property 11 'grp)
                                    (get-text-property 21 'grp)
                                    (get-text-property 26 'grp))))
                (kill-buffer buf)
                (list after restored))))))))) "#,
        expect,
    );
}

#[test]
fn combo_beginning_end_buffer_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-bebn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (put-text-property 26 30 'sect 'f)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t))
            (ov (make-overlay 6 25)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 6 25)
        (goto-char (point-min))
        (let ((pmin (point)))
          (end-of-buffer)
          (let ((pmax (point)))
            (beginning-of-buffer)
            (insert "XX-")
            (widen)
            (let ((after (list (buffer-string)
                               pmin pmax
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'sect)
                               (get-text-property 6 'sect)
                               (get-text-property 16 'sect)
                               (get-text-property 21 'sect)
                               (get-text-property 26 'sect))))
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'sect)
                                    (get-text-property 6 'sect)
                                    (get-text-property 11 'sect)
                                    (get-text-property 16 'sect)
                                    (get-text-property 21 'sect)
                                    (get-text-property 26 'sect))))
                (kill-buffer buf)
                (list after restored)))))))))) "#,
        expect,
    );
}

#[test]
fn combo_beginning_end_buffer_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-bebl")))
    (with-current-buffer buf
      (make-local-variable 'beb-local)
      (setq beb-local 'buffer-specific)
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (put-text-property 21 25 'grp 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (end-of-buffer)
        (let ((pos-end (point)))
          (beginning-of-buffer)
          (let ((pos-beg (point)))
            (goto-char 10)
            (insert "XX")
            (let ((after (list (buffer-string)
                               pos-beg pos-end
                               beb-local
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'grp)
                               (get-text-property 6 'grp)
                               (get-text-property 12 'grp)
                               (get-text-property 18 'grp)
                               (get-text-property 22 'grp))))
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    beb-local
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'grp)
                                    (get-text-property 6 'grp)
                                    (get-text-property 11 'grp)
                                    (get-text-property 16 'grp)
                                    (get-text-property 21 'grp))))
                (kill-buffer buf)
                (list after restored)))))))))) "#,
        expect,
    );
}

#[test]
fn combo_beginning_end_buffer_marker_ring_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-bemr")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (put-text-property 21 25 'grp 'e)
      (put-text-property 26 30 'grp 'f)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t))
            (m3 (copy-marker 25 nil))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((ring (list (copy-marker 1) (copy-marker 11) (copy-marker 21))))
          (end-of-buffer)
          (let ((pos-end (point)))
            (beginning-of-buffer)
            (let ((pos-beg (point)))
              (goto-char 10)
              (insert "XX")
              (let ((after (list (buffer-string)
                                 pos-beg pos-end
                                 (mapcar 'marker-position ring)
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (overlay-start ov) (overlay-end ov)
                                 (get-text-property 1 'grp)
                                 (get-text-property 6 'grp)
                                 (get-text-property 12 'grp)
                                 (get-text-property 18 'grp)
                                 (get-text-property 22 'grp)
                                 (get-text-property 26 'grp))))
                (primitive-undo 1 buffer-undo-list)
                (let ((restored (list (buffer-string)
                                      (mapcar 'marker-position ring)
                                      (marker-position m1)
                                      (marker-position m2)
                                      (marker-position m3)
                                      (overlay-start ov) (overlay-end ov)
                                      (get-text-property 1 'grp)
                                      (get-text-property 6 'grp)
                                      (get-text-property 11 'grp)
                                      (get-text-property 16 'grp)
                                      (get-text-property 21 'grp)
                                      (get-text-property 26 'grp))))
                  (kill-buffer buf)
                  (list after restored)))))))))) "#,
        expect,
    );
}

#[test]
fn combo_beginning_end_buffer_multi_edit_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-beme")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (put-text-property 21 25 'grp 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (m3 (copy-marker 15 nil))
            (ov (make-overlay 1 25)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        ;; Move to end, insert
        (end-of-buffer)
        (insert "-TAIL")
        ;; Move to beginning, insert
        (beginning-of-buffer)
        (insert "HEAD-")
        ;; Move to middle, insert
        (goto-char 15)
        (insert "MID")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp)
                           (get-text-property 11 'grp)
                           (get-text-property 16 'grp)
                           (get-text-property 21 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'grp)
                                (get-text-property 6 'grp)
                                (get-text-property 11 'grp)
                                (get-text-property 16 'grp)
                                (get-text-property 21 'grp))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

//! Deep combo: with-temp-buffer × generate-new-buffer × get-buffer-create ×
//! switch-to-buffer × set-buffer × marker × overlay × textprop × undo ×
//! buffer-local × narrow.
//!
//! Stresses buffer creation and switching with buffer state: creating
//! temporary buffers, switching between buffers, and buffer-local
//! isolation. Buffer creation is tricky because each buffer has its
//! own independent state (markers, overlays, text properties, undo list)
//! and switching must correctly preserve all of it.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_with_temp_buffer_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((results nil))
    (with-temp-buffer
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (insert "XX")
        (push (list (buffer-string)
                    (marker-position m1)
                    (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'zone)
                    (get-text-property 6 'zone))
              results)
        (primitive-undo 1 buffer-undo-list)
        (push (list (buffer-string)
                    (marker-position m1)
                    (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'zone)
                    (get-text-property 6 'zone)
                    (get-text-property 11 'zone))
              results)))
    (list (nreverse results)))) "#,
        expect,
    );
}

#[test]
fn combo_generate_new_buffer_switch_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (generate-new-buffer " combo-gnb1"))
        (buf2 (generate-new-buffer " combo-gnb2"))
        (results nil))
    (with-current-buffer buf1
      (make-local-variable 'gnb-local)
      (setq gnb-local 'buf1)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c))
    (with-current-buffer buf2
      (make-local-variable 'gnb-local)
      (setq gnb-local 'buf2)
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'd)
      (put-text-property 6 10 'zone 'e)
      (put-text-property 11 15 'zone 'f))
    ;; Switch between buffers and edit each
    (with-current-buffer buf1
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (insert "XX")
        (push (list (buffer-string) gnb-local
                    (marker-position m1) (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'zone)
                    (get-text-property 6 'zone))
              results)
        (primitive-undo 1 buffer-undo-list)
        (push (list (buffer-string) gnb-local
                    (marker-position m1) (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'zone)
                    (get-text-property 6 'zone)
                    (get-text-property 11 'zone))
              results)))
    (with-current-buffer buf2
      (push (list (buffer-string) gnb-local
                  (get-text-property 1 'zone)
                  (get-text-property 6 'zone))
            results))
    (kill-buffer buf1)
    (kill-buffer buf2)
    (list (nreverse results)))) "#,
        expect,
    );
}

#[test]
fn combo_get_buffer_create_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (get-buffer-create " combo-gbc"))
        (results nil))
    (with-current-buffer buf
      (erase-buffer)
      (make-local-variable 'gbc-local)
      (setq gbc-local 'created)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (insert "XX")
        (push (list (buffer-string) gbc-local
                    (marker-position m1) (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'zone)
                    (get-text-property 6 'zone))
              results)
        (primitive-undo 1 buffer-undo-list)
        (push (list (buffer-string) gbc-local
                    (marker-position m1) (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'zone)
                    (get-text-property 6 'zone)
                    (get-text-property 11 'zone))
              results)))
    (kill-buffer buf)
    (list (nreverse results)))) "#,
        expect,
    );
}

#[test]
fn combo_multi_buffer_isolation_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq multi-iso 'global)
  (let ((b1 (generate-new-buffer " combo-iso1"))
        (b2 (generate-new-buffer " combo-iso2"))
        (b3 (generate-new-buffer " combo-iso3"))
        (results nil))
    (with-current-buffer b1
      (make-local-variable 'multi-iso)
      (setq multi-iso 'b1)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c))
    (with-current-buffer b2
      (make-local-variable 'multi-iso)
      (setq multi-iso 'b2)
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'd)
      (put-text-property 6 10 'zone 'e)
      (put-text-property 11 15 'zone 'f))
    (with-current-buffer b3
      (make-local-variable 'multi-iso)
      (setq multi-iso 'b3)
      (insert "GGGG-HHHH-IIII")
      (put-text-property 1 5 'zone 'g)
      (put-text-property 6 10 'zone 'h)
      (put-text-property 11 15 'zone 'i))
    ;; Edit each buffer independently
    (dolist (pair (list (cons b1 "XX") (cons b2 "YY") (cons b3 "ZZ")))
      (with-current-buffer (car pair)
        (let ((m (copy-marker 5 nil))
              (ov (make-overlay 1 15)))
          (overlay-put ov 'scope 'all)
          (undo-boundary)
          (goto-char 5)
          (insert (cdr pair))
          (push (list (buffer-string) multi-iso
                      (marker-position m)
                      (overlay-start ov) (overlay-end ov))
                results)
          (primitive-undo 1 buffer-undo-list)
          (push (list (buffer-string) multi-iso
                      (marker-position m)
                      (overlay-start ov) (overlay-end ov))
                results))))
    (kill-buffer b1)
    (kill-buffer b2)
    (kill-buffer b3)
    (list (nreverse results) multi-iso))) "#,
        expect,
    );
}

#[test]
fn combo_set_buffer_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-sbn"))
        (results nil))
    (with-current-buffer buf
      (make-local-variable 'sbn-local)
      (setq sbn-local 'isolated)
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
        (narrow-to-region 6 20)
        (let ((saved-buf (current-buffer)))
          (set-buffer (generate-new-buffer " combo-sbn-tmp"))
          (insert "TEMP")
          (set-buffer saved-buf))
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        (push (list (buffer-string) sbn-local
                    (marker-position m1) (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'sect)
                    (get-text-property 6 'sect)
                    (get-text-property 16 'sect)
                    (get-text-property 21 'sect))
              results)
        (primitive-undo 1 buffer-undo-list)
        (push (list (buffer-string) sbn-local
                    (marker-position m1) (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'sect)
                    (get-text-property 6 'sect)
                    (get-text-property 11 'sect)
                    (get-text-property 16 'sect)
                    (get-text-property 21 'sect))
              results)))
    (kill-buffer buf)
    (when (get-buffer " combo-sbn-tmp")
      (kill-buffer " combo-sbn-tmp"))
    (list (nreverse results)))) "#,
        expect,
    );
}

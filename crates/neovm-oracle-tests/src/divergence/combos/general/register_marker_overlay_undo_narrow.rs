//! Deep combo: register × marker × overlay × undo × text-prop ×
//! buffer-local × narrow × copy-to-register × insert-register ×
//! point-to-register × jump-to-register.
//!
//! Stresses register operations with buffer state: saving/restoring
//! positions and text regions via registers, with markers and overlays
//! tracking through register operations. Registers are tricky because
//! they store positions (markers) and strings (text with properties).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_register_point_jump_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Save point to register, insert text, jump back; markers track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-reg")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (m3 (copy-marker 15 nil))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (point-to-register ?a)
        (undo-boundary)
        (goto-char 10)
        (insert "INSERTED-")
        (jump-to-register ?a)
        (let ((after-insert (list (buffer-string)
                                  (point)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (marker-position m3)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (marker-position m3)
                                  (get-text-property 1 'zone)
                                  (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after-insert after-undo))))))) "#,
        expect,
    );
}

#[test]
fn combo_register_copy_region_insert_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 20 26)""#]];
    // Copy region to register, insert at different position; undo.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-regcopy")))
    (with-current-buffer buf
      (insert "ALPHA-BETA-GAMMA-DELTA")
      (put-text-property 1 6 'word 'alpha)
      (put-text-property 7 12 'word 'beta)
      (put-text-property 13 19 'word 'gamma)
      (put-text-property 20 26 'word 'delta)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 26)))
        (overlay-put ov 'scope 'all)
        (copy-to-register ?r 7 12)
        (undo-boundary)
        (goto-char 19)
        (insert-register ?r)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 7 'word)
                           (get-text-property 19 'word))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (get-text-property 7 'word)
                                (get-text-property 13 'word))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_register_narrow_copy_insert_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Narrow, copy to register, widen, insert; undo restores.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-regnar")))
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
        (narrow-to-region 6 20)
        (copy-to-register ?z (point-min) (point-max))
        (widen)
        (goto-char 25)
        (insert-register ?z)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 6 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 6 'sect)
                                (get-text-property 16 'sect))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_register_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Buffer-local register usage with markers/overlays.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-regbl")))
    (with-current-buffer buf
      (make-local-variable 'reg-local)
      (setq reg-local 'buffer-specific)
      (insert "START-MIDDLE-END")
      (put-text-property 1 6 'part 'start)
      (put-text-property 7 13 'part 'middle)
      (put-text-property 14 17 'part 'end)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 13 t))
            (ov (make-overlay 1 17)))
        (overlay-put ov 'scope 'all)
        (point-to-register ?b)
        (undo-boundary)
        (goto-char 7)
        (insert "NEW-")
        (jump-to-register ?b)
        (let ((after (list (buffer-string)
                           reg-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'part)
                           (get-text-property 7 'part))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                reg-local
                                (marker-position m1)
                                (marker-position m2)
                                (get-text-property 1 'part)
                                (get-text-property 7 'part))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_register_multiple_registers_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Multiple registers with different content; undo chain.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-regmult")))
    (with-current-buffer buf
      (insert "AAA BBB CCC DDD EEE")
      (put-text-property 1 4 'grp 'a)
      (put-text-property 5 8 'grp 'b)
      (put-text-property 9 12 'grp 'c)
      (put-text-property 13 16 'grp 'd)
      (put-text-property 17 20 'grp 'e)
      (let ((m1 (copy-marker 4 nil))
            (m2 (copy-marker 8 t))
            (m3 (copy-marker 12 nil))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (copy-to-register ?x 1 4)
        (copy-to-register ?y 5 8)
        (copy-to-register ?z 9 12)
        (undo-boundary)
        (goto-char 20)
        (insert "-")
        (insert-register ?x)
        (insert "-")
        (insert-register ?y)
        (insert "-")
        (insert-register ?z)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 5 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (get-text-property 1 'grp)
                                (get-text-property 9 'grp)
                                (get-text-property 13 'grp))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

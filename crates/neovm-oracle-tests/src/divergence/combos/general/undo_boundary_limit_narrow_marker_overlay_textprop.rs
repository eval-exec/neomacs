//! Deep combo: undo-boundary × marker × overlay × text-prop ×
//! buffer-local × narrow × undo-limit × primitive-undo × undo-auto.
//!
//! Stresses undo system edge cases: undo boundaries, undo limits,
//! undo in narrowed buffers, undo with buffer-local undo-list,
//! and undo-auto markers. The undo system is particularly tricky
//! in a Rust rewrite because it involves complex list manipulation
//! and position tracking across edits.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_undo_boundary_chain_marker_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Multiple undo boundaries with markers/overlays/text-properties.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ub")))
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
        (undo-boundary)
        (goto-char 5)
        (insert "-X-")
        (undo-boundary)
        (goto-char 13)
        (insert "-Y-")
        (undo-boundary)
        (goto-char 20)
        (insert "-Z-")
        (let ((state-3 (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'zone)
                             (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((state-2 (list (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (overlay-start ov) (overlay-end ov))))
            (primitive-undo 1 buffer-undo-list)
            (let ((state-1 (list (buffer-string)
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (overlay-start ov) (overlay-end ov)
                                 (get-text-property 1 'zone)
                                 (get-text-property 11 'zone))))
              (primitive-undo 1 buffer-undo-list)
              (let ((state-0 (list (buffer-string)
                                   (marker-position m1)
                                   (marker-position m2)
                                   (marker-position m3)
                                   (overlay-start ov) (overlay-end ov)
                                   (get-text-property 1 'zone)
                                   (get-text-property 6 'zone)
                                   (get-text-property 11 'zone)
                                   (get-text-property 16 'zone))))
                (kill-buffer buf)
                (list state-3 state-2 state-1 state-0))))))))) "#,
        expect,
    );
}

#[test]
fn combo_undo_narrow_marker_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Undo in narrowed buffer; markers/overlays must track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-un")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (m3 (copy-marker 15 nil))
            (ov (make-overlay 6 20)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (insert "XX-")
        (undo-boundary)
        (goto-char (point-max))
        (insert "-YY")
        (let ((narrowed (buffer-string))
              (m1-narrow (marker-position m1))
              (m2-narrow (marker-position m2))
              (m3-narrow (marker-position m3)))
          (widen)
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo-1 (list (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (marker-position m3)
                                    (overlay-start ov) (overlay-end ov))))
            (primitive-undo 1 buffer-undo-list)
            (let ((after-undo-2 (list (buffer-string)
                                      (marker-position m1)
                                      (marker-position m2)
                                      (marker-position m3)
                                      (overlay-start ov) (overlay-end ov)
                                      (get-text-property 1 'sect)
                                      (get-text-property 6 'sect)
                                      (get-text-property 11 'sect)
                                      (get-text-property 16 'sect)
                                      (get-text-property 21 'sect))))
              (kill-buffer buf)
              (list narrowed m1-narrow m2-narrow m3-narrow
                    after-undo-1 after-undo-2))))))))) "#,
        expect,
    );
}

#[test]
fn combo_undo_buffer_local_undo_list_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((#(\"HELLO-BEAUTIFUL--WORLD\" 0 5 (word hello) 17 22 (word world)) 6 17 1 23 hello nil (nil (6 . 17) 12 nil (nil word nil 7 . 12) (nil word nil 1 . 6) (1 . 12) (t . 0))) (#(\"HELLO-BEAUTIFUL--WORLD\" 0 5 (word hello) 17 22 (word world)) 6 17 1 23 hello nil))""#
    ]];
    // Buffer-local undo-list with markers/overlays.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ubl")))
    (with-current-buffer buf
      (make-local-variable 'buffer-undo-list)
      (setq buffer-undo-list nil)
      (insert "HELLO-WORLD")
      (put-text-property 1 6 'word 'hello)
      (put-text-property 7 12 'word 'world)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 6 t))
            (ov (make-overlay 1 12)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 6)
        (insert "-BEAUTIFUL-")
        (undo-boundary)
        (let ((after-insert (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'word)
                                  (get-text-property 7 'word)
                                  buffer-undo-list)))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'word)
                                  (get-text-property 7 'word))))
            (kill-buffer buf)
            (list after-insert after-undo))))))) "#,
        expect,
    );
}

#[test]
fn combo_undo_limit_marker_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // undo-limit affects undo behavior; markers/overlays must track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ul")))
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
        (goto-char 5)
        (insert "-X-")
        (undo-boundary)
        (goto-char 13)
        (insert "-Y-")
        (undo-boundary)
        (let ((undo-limit-saved undo-limit))
          (setq undo-limit 100)
          (let ((state-before (list (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'zone))))
            (primitive-undo 1 buffer-undo-list)
            (let ((state-after (list (buffer-string)
                                     (marker-position m1)
                                     (marker-position m2)
                                     (overlay-start ov) (overlay-end ov)
                                     (get-text-property 1 'zone)
                                     (get-text-property 6 'zone))))
              (setq undo-limit undo-limit-saved)
              (kill-buffer buf)
              (list state-before state-after)))))))) "#,
        expect,
    );
}

#[test]
fn combo_undo_auto_save_marker_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // undo-auto markers with save-excursion; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-uas")))
    (with-current-buffer buf
      (insert "PREFIX-CORE-SUFFIX")
      (put-text-property 1 7 'part 'prefix)
      (put-text-property 8 12 'part 'core)
      (put-text-property 13 19 'part 'suffix)
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 12 t))
            (ov (make-overlay 1 19)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (save-excursion
          (goto-char 8)
          (insert "NEW-"))
        (undo-boundary)
        (save-excursion
          (goto-char (point-max))
          (insert "-END"))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'part)
                           (get-text-property 8 'part))))
          (primitive-undo 1 buffer-undo-list)
          (let ((after-undo (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'part)
                                  (get-text-property 8 'part)
                                  (get-text-property 13 'part))))
            (kill-buffer buf)
            (list after after-undo))))))) "#,
        expect,
    );
}

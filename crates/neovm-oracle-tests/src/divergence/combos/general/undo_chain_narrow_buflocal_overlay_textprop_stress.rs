//! Deep combo: undo chain stress × narrow/widen × buffer-local × overlay ×
//! textprop × marker × regex replace × save-restriction.
//!
//! Stresses multi-step undo chains where each step involves narrowing,
//! buffer-local variables, overlays, text properties, and markers.
//! This targets the interaction between the undo system and narrowing,
//! which is complex in a Rust rewrite because undo must correctly
//! restore narrowing state alongside buffer content.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_undo_chain_narrow_buflocal_overlay_textprop_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ucnb")))
    (with-current-buffer buf
      (make-local-variable 'chain-local)
      (setq chain-local 'initial)
      (insert "alpha:100 beta:200 gamma:300 delta:400 epsilon:500")
      (put-text-property 1 10 'grp 'g1)
      (put-text-property 11 20 'grp 'g2)
      (put-text-property 21 30 'grp 'g3)
      (put-text-property 31 40 'grp 'g4)
      (put-text-property 41 51 'grp 'g5)
      (let ((m1 (copy-marker 10 nil))
            (m2 (copy-marker 20 t))
            (m3 (copy-marker 30 nil))
            (m4 (copy-marker 40 t))
            (ov1 (make-overlay 1 30))
            (ov2 (make-overlay 21 51)))
        (overlay-put ov1 'zone 'left)
        (overlay-put ov1 'priority 1)
        (overlay-put ov2 'zone 'right)
        (overlay-put ov2 'priority 2)
        ;; Step 1: narrow + insert
        (undo-boundary)
        (setq chain-local 'step1)
        (narrow-to-region 11 40)
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        ;; Step 2: regex replace
        (undo-boundary)
        (setq chain-local 'step2)
        (goto-char 1)
        (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
          (replace-match "\\1=\\2" t))
        ;; Step 3: narrow + delete
        (undo-boundary)
        (setq chain-local 'step3)
        (narrow-to-region 21 51)
        (delete-region (point-min) (+ (point-min) 5))
        (widen)
        ;; Record state after 3 steps
        (let ((state-3 (list (buffer-string)
                             chain-local
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (marker-position m4)
                             (overlay-start ov1) (overlay-end ov1)
                             (overlay-start ov2) (overlay-end ov2)
                             (overlay-get (car (overlays-at 1)) 'zone)
                             (overlay-get (car (overlays-at 25)) 'zone)
                             (get-text-property 1 'grp)
                             (get-text-property 11 'grp)
                             (get-text-property 21 'grp)
                             (get-text-property 31 'grp))))
          ;; Undo step 3
          (primitive-undo 1 buffer-undo-list)
          (let ((state-2 (list (buffer-string)
                               chain-local
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (marker-position m4)
                               (overlay-start ov1) (overlay-end ov1)
                               (overlay-start ov2) (overlay-end ov2)
                               (get-text-property 1 'grp)
                               (get-text-property 11 'grp)
                               (get-text-property 21 'grp))))
            ;; Undo step 2
            (primitive-undo 1 buffer-undo-list)
            (let ((state-1 (list (buffer-string)
                                 chain-local
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (marker-position m4)
                                 (overlay-start ov1) (overlay-end ov1)
                                 (overlay-start ov2) (overlay-end ov2)
                                 (get-text-property 1 'grp)
                                 (get-text-property 11 'grp)
                                 (get-text-property 21 'grp)
                                 (get-text-property 31 'grp)
                                 (get-text-property 41 'grp))))
              ;; Undo step 1
              (primitive-undo 1 buffer-undo-list)
              (let ((state-0 (list (buffer-string)
                                   chain-local
                                   (marker-position m1)
                                   (marker-position m2)
                                   (marker-position m3)
                                   (marker-position m4)
                                   (overlay-start ov1) (overlay-end ov1)
                                   (overlay-start ov2) (overlay-end ov2)
                                   (overlay-get (car (overlays-at 1)) 'zone)
                                   (overlay-get (car (overlays-at 25)) 'zone)
                                   (get-text-property 1 'grp)
                                   (get-text-property 11 'grp)
                                   (get-text-property 21 'grp)
                                   (get-text-property 31 'grp)
                                   (get-text-property 41 'grp))))
                (kill-buffer buf)
                (list state-3 state-2 state-1 state-0)))))))))) "#,
        expect,
    );
}

#[test]
fn combo_undo_chain_nested_narrow_evaporate_textprop_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ucnn")))
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
            (m3 (copy-marker 25 nil))
            (ov1 (make-overlay 1 15))
            (ov2 (make-overlay 11 25))
            (ov3 (make-overlay 6 30)))
        (overlay-put ov1 'zone 'left)
        (overlay-put ov1 'priority 1)
        (overlay-put ov2 'zone 'middle)
        (overlay-put ov2 'priority 2)
        (overlay-put ov2 'evaporate t)
        (overlay-put ov3 'zone 'outer)
        (overlay-put ov3 'priority 0)
        ;; Step 1: narrow to middle + insert
        (undo-boundary)
        (narrow-to-region 6 25)
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        ;; Step 2: narrow to inner + delete (should evaporate ov2)
        (undo-boundary)
        (narrow-to-region 11 25)
        (delete-region (point-min) (+ (point-min) 4))
        (widen)
        ;; Record states
        (let ((state-2 (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (and (overlay-start ov1) (overlay-end ov1))
                             (and (overlay-start ov2) (overlay-end ov2))
                             (and (overlay-start ov3) (overlay-end ov3))
                             (overlay-get (car (overlays-at 1)) 'zone)
                             (overlay-get (car (overlays-at 20)) 'zone)
                             (get-text-property 1 'sect)
                             (get-text-property 6 'sect)
                             (get-text-property 16 'sect)
                             (get-text-property 26 'sect))))
          ;; Undo step 2
          (primitive-undo 1 buffer-undo-list)
          (let ((state-1 (list (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (and (overlay-start ov2) (overlay-end ov2))
                               (overlay-get (car (overlays-at 1)) 'zone)
                               (overlay-get (car (overlays-at 12)) 'zone)
                               (get-text-property 1 'sect)
                               (get-text-property 6 'sect)
                               (get-text-property 16 'sect)
                               (get-text-property 26 'sect))))
            ;; Undo step 1
            (primitive-undo 1 buffer-undo-list)
            (let ((state-0 (list (buffer-string)
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (and (overlay-start ov1) (overlay-end ov1))
                                 (and (overlay-start ov2) (overlay-end ov2))
                                 (and (overlay-start ov3) (overlay-end ov3))
                                 (overlay-get (car (overlays-at 1)) 'zone)
                                 (overlay-get (car (overlays-at 12)) 'zone)
                                 (overlay-get (car (overlays-at 20)) 'zone)
                                 (get-text-property 1 'sect)
                                 (get-text-property 6 'sect)
                                 (get-text-property 11 'sect)
                                 (get-text-property 16 'sect)
                                 (get-text-property 21 'sect)
                                 (get-text-property 26 'sect))))
              (kill-buffer buf)
              (list state-2 state-1 state-0)))))))) "#,
        expect,
    );
}

#[test]
fn combo_undo_chain_buflocal_undo_list_narrow_overlay_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ucbl")))
    (with-current-buffer buf
      (make-local-variable 'undo-local)
      (setq undo-local 'chain)
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (put-text-property 21 25 'grp 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (m3 (copy-marker 15 nil))
            (ov1 (make-overlay 1 10))
            (ov2 (make-overlay 6 20))
            (ov3 (make-overlay 11 25)))
        (overlay-put ov1 'prio 1)
        (overlay-put ov2 'prio 2)
        (overlay-put ov3 'prio 3)
        ;; Step 1: narrow + insert
        (undo-boundary)
        (setq undo-local 'step1)
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (insert "INNER-")
        (widen)
        ;; Step 2: insert at point-max
        (undo-boundary)
        (setq undo-local 'step2)
        (goto-char (point-max))
        (insert "-TAIL")
        ;; Record states
        (let ((state-2 (list (buffer-string)
                             undo-local
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (overlay-get (car (overlays-at 1)) 'prio)
                             (overlay-get (car (overlays-at 7)) 'prio)
                             (overlay-get (car (overlays-at 15)) 'prio)
                             (get-text-property 1 'grp)
                             (get-text-property 6 'grp)
                             (get-text-property 11 'grp)
                             (get-text-property 16 'grp)
                             (get-text-property 21 'grp))))
          ;; Undo step 2
          (primitive-undo 1 buffer-undo-list)
          (let ((state-1 (list (buffer-string)
                               undo-local
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (overlay-get (car (overlays-at 1)) 'prio)
                               (overlay-get (car (overlays-at 7)) 'prio)
                               (overlay-get (car (overlays-at 15)) 'prio)
                               (get-text-property 1 'grp)
                               (get-text-property 6 'grp)
                               (get-text-property 11 'grp)
                               (get-text-property 16 'grp))))
            ;; Undo step 1
            (primitive-undo 1 buffer-undo-list)
            (let ((state-0 (list (buffer-string)
                                 undo-local
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (overlay-get (car (overlays-at 1)) 'prio)
                                 (overlay-get (car (overlays-at 7)) 'prio)
                                 (overlay-get (car (overlays-at 15)) 'prio)
                                 (overlay-get (car (overlays-at 22)) 'prio)
                                 (get-text-property 1 'grp)
                                 (get-text-property 6 'grp)
                                 (get-text-property 11 'grp)
                                 (get-text-property 16 'grp)
                                 (get-text-property 21 'grp))))
              (kill-buffer buf)
              (list state-2 state-1 state-0)))))))) "#,
        expect,
    );
}

#[test]
fn combo_undo_chain_replace_match_narrow_marker_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ucrm")))
    (with-current-buffer buf
      (insert "item-1:aaa item-2:bbb item-3:ccc item-4:ddd")
      (put-text-property 1 11 'item 'one)
      (put-text-property 12 22 'item 'two)
      (put-text-property 23 33 'item 'three)
      (put-text-property 34 44 'item 'four)
      (let ((m1 (copy-marker 11 nil))
            (m2 (copy-marker 22 t))
            (m3 (copy-marker 33 nil))
            (ov (make-overlay 1 44)))
        (overlay-put ov 'scope 'all)
        ;; Step 1: narrow + regex replace
        (undo-boundary)
        (narrow-to-region 12 33)
        (goto-char (point-min))
        (while (re-search-forward "item-\\([0-9]+\\)" nil t)
          (replace-match "ENTRY-\\1" t))
        (widen)
        ;; Step 2: narrow + insert
        (undo-boundary)
        (narrow-to-region 1 22)
        (goto-char (point-min))
        (insert "PREFIX-")
        (widen)
        ;; Record states
        (let ((state-2 (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'item)
                             (get-text-property 12 'item)
                             (get-text-property 23 'item)
                             (get-text-property 34 'item))))
          ;; Undo step 2
          (primitive-undo 1 buffer-undo-list)
          (let ((state-1 (list (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'item)
                               (get-text-property 12 'item)
                               (get-text-property 23 'item)
                               (get-text-property 34 'item))))
            ;; Undo step 1
            (primitive-undo 1 buffer-undo-list)
            (let ((state-0 (list (buffer-string)
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (overlay-start ov) (overlay-end ov)
                                 (get-text-property 1 'item)
                                 (get-text-property 12 'item)
                                 (get-text-property 23 'item)
                                 (get-text-property 34 'item))))
              (kill-buffer buf)
              (list state-2 state-1 state-0)))))))) "#,
        expect,
    );
}

#[test]
fn combo_undo_chain_save_restriction_narrow_buflocal_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-ucsr")))
    (with-current-buffer buf
      (make-local-variable 'sr-local)
      (setq sr-local 'initial)
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (put-text-property 26 30 'sect 'f)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 15 t))
            (m3 (copy-marker 25 nil))
            (ov (make-overlay 1 30)))
        (overlay-put ov 'scope 'all)
        ;; Step 1: save-restriction + narrow + insert
        (undo-boundary)
        (setq sr-local 'step1)
        (save-restriction
          (narrow-to-region 6 25)
          (goto-char (point-min))
          (insert "XX-"))
        ;; Step 2: save-restriction + nested narrow + insert
        (undo-boundary)
        (setq sr-local 'step2)
        (save-restriction
          (narrow-to-region 11 20)
          (save-restriction
            (narrow-to-region 2 5)
            (goto-char (point-min))
            (insert "YY")))
        ;; Record states
        (let ((state-2 (list (buffer-string)
                             sr-local
                             (marker-position m1)
                             (marker-position m2)
                             (marker-position m3)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'sect)
                             (get-text-property 6 'sect)
                             (get-text-property 11 'sect)
                             (get-text-property 16 'sect)
                             (get-text-property 21 'sect)
                             (get-text-property 26 'sect))))
          ;; Undo step 2
          (primitive-undo 1 buffer-undo-list)
          (let ((state-1 (list (buffer-string)
                               sr-local
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'sect)
                               (get-text-property 6 'sect)
                               (get-text-property 11 'sect)
                               (get-text-property 16 'sect)
                               (get-text-property 21 'sect)
                               (get-text-property 26 'sect))))
            ;; Undo step 1
            (primitive-undo 1 buffer-undo-list)
            (let ((state-0 (list (buffer-string)
                                 sr-local
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (overlay-start ov) (overlay-end ov)
                                 (get-text-property 1 'sect)
                                 (get-text-property 6 'sect)
                                 (get-text-property 11 'sect)
                                 (get-text-property 16 'sect)
                                 (get-text-property 21 'sect)
                                 (get-text-property 26 'sect))))
              (kill-buffer buf)
              (list state-2 state-1 state-0)))))))) "#,
        expect,
    );
}

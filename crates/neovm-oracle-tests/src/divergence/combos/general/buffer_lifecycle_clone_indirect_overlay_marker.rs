//! Deep combo: buffer lifecycle × clone-buffer × rename-buffer ×
//! make-indirect-buffer × kill-buffer × buffer-local × overlay ×
//! marker × textprop × undo.
//!
//! Stresses buffer lifecycle operations: cloning, renaming, creating
//! indirect buffers, and killing buffers while preserving overlays,
//! markers, text properties, and buffer-local variables. Buffer
//! lifecycle is tricky in a Rust rewrite because each operation must
//! correctly share or copy all buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_clone_buffer_overlay_marker_textprop_buflocal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-clone")))
    (with-current-buffer buf
      (make-local-variable 'clone-local)
      (setq clone-local 'original)
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (insert "XX")
        (let* ((clone-name " combo-cloned")
               (clone (clone-buffer clone-name)))
          (unwind-protect
              (let ((after (list (buffer-string)
                                 clone-local
                                 (marker-position m1)
                                 (marker-position m2)
                                 (overlay-start ov) (overlay-end ov)
                                 (get-text-property 1 'grp)
                                 (get-text-property 6 'grp)
                                 (with-current-buffer clone (buffer-string))
                                 (with-current-buffer clone
                                   (get-text-property 1 'grp))
                                 (with-current-buffer clone
                                   (get-text-property 6 'grp)))))
                (primitive-undo 1 buffer-undo-list)
                (let ((restored (list (buffer-string)
                                      clone-local
                                      (marker-position m1)
                                      (marker-position m2)
                                      (overlay-start ov) (overlay-end ov)
                                      (get-text-property 1 'grp)
                                      (get-text-property 6 'grp)
                                      (get-text-property 11 'grp))))
                  (kill-buffer clone)
                  (kill-buffer buf)
                  (list after restored))))))))) "#,
        expect,
    );
}

#[test]
fn combo_rename_buffer_overlay_marker_textprop_buflocal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-rename")))
    (with-current-buffer buf
      (make-local-variable 'rename-local)
      (setq rename-local 'original)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (rename-buffer " combo-renamed")
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (buffer-name)
                           rename-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (buffer-name)
                                rename-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_indirect_buffer_shared_overlay_marker_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((base (generate-new-buffer " combo-indbase")))
    (with-current-buffer base
      (make-local-variable 'ind-local)
      (setq ind-local 'base-val)
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (let ((ind (make-indirect-buffer base " combo-indirect")))
          (unwind-protect
              (progn
                ;; Edit via indirect buffer
                (with-current-buffer ind
                  (undo-boundary)
                  (make-local-variable 'ind-local)
                  (setq ind-local 'ind-val)
                  (goto-char 5)
                  (insert "XX"))
                ;; Check base sees the edit
                (let ((after (list (with-current-buffer base (buffer-string))
                                   (with-current-buffer ind (buffer-string))
                                   (with-current-buffer base ind-local)
                                   (with-current-buffer ind ind-local)
                                   (marker-position m1)
                                   (marker-position m2)
                                   (overlay-start ov) (overlay-end ov)
                                   (with-current-buffer base
                                     (get-text-property 1 'grp))
                                   (with-current-buffer base
                                     (get-text-property 6 'grp)))))
                  ;; Undo in base
                  (with-current-buffer base
                    (primitive-undo 1 buffer-undo-list))
                  (let ((restored (list (with-current-buffer base (buffer-string))
                                        (with-current-buffer ind (buffer-string))
                                        (with-current-buffer base ind-local)
                                        (marker-position m1)
                                        (marker-position m2)
                                        (overlay-start ov) (overlay-end ov)
                                        (with-current-buffer base
                                          (get-text-property 1 'grp))
                                        (with-current-buffer base
                                          (get-text-property 6 'grp))
                                        (with-current-buffer base
                                          (get-text-property 11 'grp)))))
                    (kill-buffer ind)
                    (kill-buffer base)
                    (list after restored))))))))) "#,
        expect,
    );
}

#[test]
fn combo_kill_buffer_local_overlay_marker_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq kill-test-global 'original)
  (let ((buf (generate-new-buffer " combo-kill")))
    (with-current-buffer buf
      (make-local-variable 'kill-test-global)
      (setq kill-test-global 'buf-local)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (overlay-put ov 'kind 'test)
        (let ((pre-kill (list (buffer-string)
                              kill-test-global
                              (marker-position m1)
                              (marker-position m2)
                              (overlay-get ov 'scope)
                              (overlay-get ov 'kind)
                              (get-text-property 1 'zone)
                              (get-text-property 6 'zone)
                              (get-text-property 11 'zone))))
          (kill-buffer buf)
          (list pre-kill kill-test-global))))) "#,
        expect,
    );
}

#[test]
fn combo_buffer_lifecycle_chain_overlay_marker_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-lc")))
    (with-current-buffer buf
      (make-local-variable 'lc-local)
      (setq lc-local 'created)
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        ;; Step 1: rename
        (undo-boundary)
        (rename-buffer " combo-lc-renamed")
        (setq lc-local 'renamed)
        ;; Step 2: clone
        (let ((clone (clone-buffer " combo-lc-clone")))
          (unwind-protect
              (progn
                ;; Step 3: edit original
                (undo-boundary)
                (goto-char 5)
                (insert "XX")
                (setq lc-local 'edited)
                (let ((after-edit (list (buffer-string)
                                        (buffer-name)
                                        lc-local
                                        (marker-position m1)
                                        (marker-position m2)
                                        (overlay-start ov) (overlay-end ov)
                                        (get-text-property 1 'grp)
                                        (get-text-property 6 'grp)
                                        (with-current-buffer clone (buffer-string))
                                        (with-current-buffer clone lc-local))))
                  ;; Undo edit
                  (primitive-undo 1 buffer-undo-list)
                  (let ((after-undo (list (buffer-string)
                                          (buffer-name)
                                          lc-local
                                          (marker-position m1)
                                          (marker-position m2)
                                          (overlay-start ov) (overlay-end ov)
                                          (get-text-property 1 'grp)
                                          (get-text-property 6 'grp)
                                          (get-text-property 11 'grp))))
                    (kill-buffer clone)
                    (kill-buffer buf)
                    (list after-edit after-undo))))
            (when (buffer-live-p clone)
              (kill-buffer clone)))))))) "#,
        expect,
    );
}

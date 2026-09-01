//! Deep combo: buffer-local × setq × set × let × let* × default-value ×
//! set-default × marker × overlay × text-prop × undo × narrow ×
//! kill-buffer × with-current-buffer × indirect-buffer.
//!
//! Stresses buffer-local variable interactions: setq vs set with
//! buffer-local variables, let-binding restoration, default-value
//! interactions, and buffer-local across buffer switches. These are
//! tricky because buffer-local variables have complex scoping rules
//! that must interact correctly with the edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_buflocal_setq_set_let_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq bl-test 'global)
  (let ((buf (generate-new-buffer " combo-blssl")))
    (with-current-buffer buf
      (make-local-variable 'bl-test)
      (setq bl-test 'buf-local)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((bl-test 'let-bound))
          (setq bl-test 'setq-in-let)
          (goto-char 5)
          (insert "XX")
          (let ((in-let (list bl-test
                              (marker-position m1)
                              (marker-position m2)
                              (overlay-start ov) (overlay-end ov)
                              (get-text-property 1 'zone)
                              (get-text-property 6 'zone))))
            (primitive-undo 1 buffer-undo-list)
            (let ((after-let (list bl-test
                                   (buffer-string)
                                   (marker-position m1)
                                   (marker-position m2)
                                   (overlay-start ov) (overlay-end ov)
                                   (get-text-property 1 'zone)
                                   (get-text-property 6 'zone)
                                   (get-text-property 11 'zone))))
              (kill-buffer buf)
              (list in-let after-let bl-test)))))))) "#,
        expect,
    );
}

#[test]
fn combo_buflocal_let_star_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq bls-a 'global-a)
  (setq bls-b 'global-b)
  (let ((buf (generate-new-buffer " combo-blls")))
    (with-current-buffer buf
      (make-local-variable 'bls-a)
      (make-local-variable 'bls-b)
      (setq bls-a 'local-a)
      (setq bls-b 'local-b)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let* ((bls-a 'let-star-a)
               (bls-b (list bls-a 'derived)))
          (goto-char 5)
          (insert "XX")
          (let ((in-let* (list bls-a bls-b
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'zone)
                               (get-text-property 6 'zone))))
            (primitive-undo 1 buffer-undo-list)
            (let ((after (list bls-a bls-b
                               (buffer-string)
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'zone)
                               (get-text-property 6 'zone)
                               (get-text-property 11 'zone))))
              (kill-buffer buf)
              (list in-let* after)))))))) "#,
        expect,
    );
}

#[test]
fn combo_buflocal_default_value_set_default_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq bdv-test 'original)
  (let ((buf (generate-new-buffer " combo-bldv")))
    (with-current-buffer buf
      (make-local-variable 'bdv-test)
      (setq bdv-test 'buf-local)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (set-default 'bdv-test 'new-default)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           bdv-test
                           (default-value 'bdv-test)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                bdv-test
                                (default-value 'bdv-test)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_buflocal_with_current_buffer_switch_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq bwcb-test 'global)
  (let ((buf1 (generate-new-buffer " combo-bwcb1"))
        (buf2 (generate-new-buffer " combo-bwcb2")))
    (with-current-buffer buf1
      (make-local-variable 'bwcb-test)
      (setq bwcb-test 'buf1-local)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c))
    (with-current-buffer buf2
      (make-local-variable 'bwcb-test)
      (setq bwcb-test 'buf2-local)
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'd)
      (put-text-property 6 10 'zone 'e)
      (put-text-property 11 15 'zone 'f))
    (let ((results nil))
      (with-current-buffer buf1
        (let ((m1 (copy-marker 5 nil))
              (m2 (copy-marker 10 t))
              (ov (make-overlay 1 15)))
          (overlay-put ov 'scope 'all)
          (undo-boundary)
          (goto-char 5)
          (insert "XX")
          (push (list (buffer-string)
                      bwcb-test
                      (marker-position m1)
                      (marker-position m2)
                      (overlay-start ov) (overlay-end ov)
                      (get-text-property 1 'zone)
                      (get-text-property 6 'zone))
                results)
          (primitive-undo 1 buffer-undo-list)
          (push (list (buffer-string)
                      bwcb-test
                      (marker-position m1)
                      (marker-position m2)
                      (overlay-start ov) (overlay-end ov)
                      (get-text-property 1 'zone)
                      (get-text-property 6 'zone)
                      (get-text-property 11 'zone))
                results)))
      (with-current-buffer buf2
        (push (list (buffer-string) bwcb-test) results))
      (kill-buffer buf1)
      (kill-buffer buf2)
      (list (nreverse results) bwcb-test)))) "#,
        expect,
    );
}

#[test]
fn combo_buflocal_indirect_buffer_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq bind-test 'global)
  (let ((base (generate-new-buffer " combo-blbase")))
    (with-current-buffer base
      (make-local-variable 'bind-test)
      (setq bind-test 'base-local)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((ind (make-indirect-buffer base " combo-blind")))
        (with-current-buffer ind
          (let ((m1 (copy-marker 5 nil))
                (m2 (copy-marker 10 t))
                (ov (make-overlay 1 15)))
            (overlay-put ov 'scope 'all)
            (undo-boundary)
            (goto-char 5)
            (insert "XX")
            (let ((after (list (buffer-string)
                               bind-test
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'zone)
                               (get-text-property 6 'zone)
                               (with-current-buffer base (buffer-string)))))
              (primitive-undo 1 buffer-undo-list)
              (let ((restored (list (buffer-string)
                                    bind-test
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'zone)
                                    (get-text-property 6 'zone)
                                    (get-text-property 11 'zone)
                                    (with-current-buffer base (buffer-string)))))
                (kill-buffer ind)
                (kill-buffer base)
                (list after restored))))))))) "#,
        expect,
    );
}

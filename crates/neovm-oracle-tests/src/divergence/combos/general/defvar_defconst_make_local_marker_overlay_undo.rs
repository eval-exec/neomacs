//! Deep combo: defvar × defconst × defcustom × make-variable-buffer-local ×
//! setq × set × marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses variable definition and buffer-local interactions: defvar,
//! defconst, defcustom with make-variable-buffer-local, and how these
//! interact with markers, overlays, text properties, and undo. Variable
//! definition is tricky because it involves global state that must
//! interact correctly with buffer-local bindings.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_defvar_make_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dml-test-var 'global-default)
  (let ((buf (generate-new-buffer " combo-dml")))
    (with-current-buffer buf
      (make-local-variable 'dml-test-var)
      (setq dml-test-var 'buffer-local)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (setq dml-test-var 'modified)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           dml-test-var
                           (default-value 'dml-test-var)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                dml-test-var
                                (default-value 'dml-test-var)
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
fn combo_defconst_setq_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defconst dsc-test-const 42 "Test constant")
  (let ((buf (generate-new-buffer " combo-dsc")))
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
        (insert (format "-<%d>-" dsc-test-const))
        (let ((after (list (buffer-string)
                           dsc-test-const
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                dsc-test-const
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
fn combo_make_variable_buffer_local_setq_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar mvbl-test 'global)
  (make-variable-buffer-local 'mvbl-test)
  (let ((buf (generate-new-buffer " combo-mvbl")))
    (with-current-buffer buf
      (setq mvbl-test 'buffer-set)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (setq mvbl-test 'modified)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           mvbl-test
                           (default-value 'mvbl-test)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                mvbl-test
                                (default-value 'mvbl-test)
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
fn combo_defvar_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dnar-test 'global)
  (let ((buf (generate-new-buffer " combo-dnar")))
    (with-current-buffer buf
      (make-local-variable 'dnar-test)
      (setq dnar-test 'buffer-local)
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
        (setq dnar-test 'narrowed)
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        (let ((after (list (buffer-string)
                           dnar-test
                           (default-value 'dnar-test)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                dnar-test
                                (default-value 'dnar-test)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_defvar_buffer_local_overlay_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvar-multi 'global)
  (let ((buf1 (generate-new-buffer " combo-dv1"))
        (buf2 (generate-new-buffer " combo-dv2")))
    (with-current-buffer buf1
      (make-local-variable 'dvar-multi)
      (setq dvar-multi 'buf1-val)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c))
    (with-current-buffer buf2
      (make-local-variable 'dvar-multi)
      (setq dvar-multi 'buf2-val)
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
          (setq dvar-multi 'buf1-modified)
          (goto-char 5)
          (insert "XX")
          (push (list (buffer-string)
                      dvar-multi
                      (marker-position m1)
                      (marker-position m2)
                      (overlay-start ov) (overlay-end ov)
                      (get-text-property 1 'zone)
                      (get-text-property 6 'zone))
                results)
          (primitive-undo 1 buffer-undo-list)
          (push (list (buffer-string)
                      dvar-multi
                      (marker-position m1)
                      (marker-position m2)
                      (overlay-start ov) (overlay-end ov)
                      (get-text-property 1 'zone)
                      (get-text-property 6 'zone)
                      (get-text-property 11 'zone))
                results)))
      (with-current-buffer buf2
        (push (list (buffer-string) dvar-multi) results))
      (kill-buffer buf1)
      (kill-buffer buf2)
      (list (nreverse results) dvar-multi)))) "#,
        expect,
    );
}

//! Deep combo: defvar × defconst × defcustom × make-variable-buffer-local ×
//! setq × set × default-value × set-default × marker × overlay × textprop ×
//! undo × buffer-local × narrow × kill-buffer × with-current-buffer.
//!
//! Stresses variable definition and buffer-local interactions: defvar,
//! defconst, defcustom with make-variable-buffer-local, and how these
//! interact with markers, overlays, text properties, and undo. Variable
//! definition is tricky because it involves global state that must
//! interact correctly with buffer-local bindings across multiple buffers.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_defvar_make_local_multi_buffer_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dmml-var 'global)
  (let ((b1 (generate-new-buffer " combo-dmml1"))
        (b2 (generate-new-buffer " combo-dmml2"))
        (results nil))
    (with-current-buffer b1
      (make-local-variable 'dmml-var)
      (setq dmml-var 'buf1)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c))
    (with-current-buffer b2
      (make-local-variable 'dmml-var)
      (setq dmml-var 'buf2)
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'd)
      (put-text-property 6 10 'zone 'e)
      (put-text-property 11 15 'zone 'f))
    ;; Edit b1
    (with-current-buffer b1
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (setq dmml-var 'modified)
        (goto-char 5)
        (insert "XX")
        (push (list (buffer-string) dmml-var
                    (default-value 'dmml-var)
                    (marker-position m1) (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'zone)
                    (get-text-property 6 'zone))
              results)
        (primitive-undo 1 buffer-undo-list)
        (push (list (buffer-string) dmml-var
                    (default-value 'dmml-var)
                    (marker-position m1) (marker-position m2)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'zone)
                    (get-text-property 6 'zone)
                    (get-text-property 11 'zone))
              results)))
    ;; Check b2 unaffected
    (with-current-buffer b2
      (push (list (buffer-string) dmml-var) results))
    (kill-buffer b1)
    (kill-buffer b2)
    (list (nreverse results) (default-value 'dmml-var)))) "#,
        expect,
    );
}

#[test]
fn combo_set_default_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar sd-var 'initial)
  (let ((buf (generate-new-buffer " combo-sd")))
    (with-current-buffer buf
      (make-local-variable 'sd-var)
      (setq sd-var 'local)
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (m3 (copy-marker 15 nil))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (set-default 'sd-var 'new-default)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           sd-var
                           (default-value 'sd-var)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp)
                           (get-text-property 12 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                sd-var
                                (default-value 'sd-var)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'grp)
                                (get-text-property 6 'grp)
                                (get-text-property 11 'grp)
                                (get-text-property 16 'grp))))
            (set-default 'sd-var 'initial)
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_make_variable_buffer_local_kill_buffer_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((#(\"AAAAXX-BBBB-CCCC\" 0 4 (zone a) 7 11 (zone b) 12 16 (zone c)) b1-val 5 1 17 a nil) (#(\"DDDD-EEEE-FFFF\" 0 4 (zone d) 5 9 (zone e) 10 14 (zone f)) b2-val d e)) global)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar mvbl-kill 'global)
  (make-variable-buffer-local 'mvbl-kill)
  (let ((b1 (generate-new-buffer " combo-mvbl1"))
        (b2 (generate-new-buffer " combo-mvbl2"))
        (results nil))
    (with-current-buffer b1
      (setq mvbl-kill 'b1-val)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c))
    (with-current-buffer b2
      (setq mvbl-kill 'b2-val)
      (insert "DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'd)
      (put-text-property 6 10 'zone 'e)
      (put-text-property 11 15 'zone 'f))
    ;; Edit b1
    (with-current-buffer b1
      (let ((m (copy-marker 5 nil))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (goto-char 5)
        (insert "XX")
        (push (list (buffer-string) mvbl-kill
                    (marker-position m)
                    (overlay-start ov) (overlay-end ov)
                    (get-text-property 1 'zone)
                    (get-text-property 6 'zone))
              results)))
    ;; Kill b1, b2 should be unaffected
    (kill-buffer b1)
    (with-current-buffer b2
      (push (list (buffer-string) mvbl-kill
                  (get-text-property 1 'zone)
                  (get-text-property 6 'zone))
            results))
    (kill-buffer b2)
    (list (nreverse results) mvbl-kill))) "#,
        expect,
    );
}

#[test]
fn combo_defvar_let_binding_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dlet-var 'global)
  (let ((buf (generate-new-buffer " combo-dlet")))
    (with-current-buffer buf
      (make-local-variable 'dlet-var)
      (setq dlet-var 'local)
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
        ;; let-binding should restore correctly
        (let ((dlet-var 'let-bound))
          (setq dlet-var 'modified-in-let)
          (goto-char 5)
          (insert (format "-<%s>-" dlet-var)))
        (let ((after (list (buffer-string)
                           dlet-var
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp)
                           (get-text-property 12 'grp)
                           (get-text-property 18 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                dlet-var
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'grp)
                                (get-text-property 6 'grp)
                                (get-text-property 11 'grp)
                                (get-text-property 16 'grp))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_defvar_narrow_set_default_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dnsd-var 'original)
  (let ((buf (generate-new-buffer " combo-dnsd")))
    (with-current-buffer buf
      (make-local-variable 'dnsd-var)
      (setq dnsd-var 'local)
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
        (set-default 'dnsd-var 'new-default)
        (setq dnsd-var 'narrowed)
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        (let ((after (list (buffer-string)
                           dnsd-var
                           (default-value 'dnsd-var)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                dnsd-var
                                (default-value 'dnsd-var)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect))))
            (set-default 'dnsd-var 'original)
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

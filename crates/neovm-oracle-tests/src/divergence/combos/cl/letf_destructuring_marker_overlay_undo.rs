//! Deep combo: cl-letf × cl-destructuring-bind × cl-multiple-value-bind ×
//! marker × overlay × text-prop × undo × buffer-local × narrow.
//!
//! Stresses cl-lib macro interaction with buffer state: cl-letf for
//! function/variable rebinding, cl-destructuring-bind for destructuring,
//! and cl-multiple-value-bind for multiple values. These are tricky in
//! a Rust rewrite because they involve complex macro expansion and
//! temporary bindings that must interact correctly with the edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_cl_letf_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // cl-letf with function rebinding; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'cl-lib)
  (let ((buf (generate-new-buffer " combo-clletf")))
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
        (cl-letf (((symbol-function 'my-insert)
                   (lambda (text) (insert text))))
          (goto-char 5)
          (funcall 'my-insert "-XX-"))
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
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_cl_letf_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // cl-letf with buffer-local variable rebinding.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'cl-lib)
  (setq cl-letf-global 'original)
  (let ((buf (generate-new-buffer " combo-clletfbl")))
    (with-current-buffer buf
      (make-local-variable 'cl-letf-global)
      (setq cl-letf-global 'buffer-local)
      (insert "HELLO-WORLD")
      (put-text-property 1 6 'word 'hello)
      (put-text-property 7 12 'word 'world)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 6 t))
            (ov (make-overlay 1 12)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (cl-letf ((cl-letf-global 'rebound))
          (goto-char 6)
          (insert "-BEAUTIFUL-")
          (let ((in-letf (list cl-letf-global
                               (marker-position m1)
                               (marker-position m2)
                               (overlay-start ov) (overlay-end ov)
                               (get-text-property 1 'word)
                               (get-text-property 7 'word))))
            (primitive-undo 1 buffer-undo-list)
            (let ((after-undo (list cl-letf-global
                                    (buffer-string)
                                    (marker-position m1)
                                    (marker-position m2)
                                    (overlay-start ov) (overlay-end ov)
                                    (get-text-property 1 'word)
                                    (get-text-property 7 'word))))
              (kill-buffer buf)
              (list in-letf after-undo)))))))) "#,
        expect,
    );
}

#[test]
fn combo_cl_destructuring_bind_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // cl-destructuring-bind with edit inside.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'cl-lib)
  (let ((buf (generate-new-buffer " combo-cldb")))
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
        (undo-boundary)
        (cl-destructuring-bind (a b c d) '(1 2 3 4)
          (goto-char 5)
          (insert (format "-<%d>-" (+ a b)))
          (goto-char 15)
          (insert (format "-<%d>-" (+ c d))))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
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
fn combo_cl_multiple_value_bind_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function values)""#]];
    // cl-multiple-value-bind with edit inside.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'cl-lib)
  (let ((buf (generate-new-buffer " combo-clmvb")))
    (with-current-buffer buf
      (insert "PREFIX-MIDDLE-SUFFIX")
      (put-text-property 1 7 'part 'prefix)
      (put-text-property 8 14 'part 'middle)
      (put-text-property 15 21 'part 'suffix)
      (let ((m1 (copy-marker 7 nil))
            (m2 (copy-marker 14 t))
            (ov (make-overlay 1 21)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (cl-multiple-value-bind (a b c)
            (values 10 20 30)
          (goto-char 7)
          (insert (format "-<%d>-<%d>-" a b))
          (goto-char (point-max))
          (insert (format "-<%d>-" c)))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'part)
                           (get-text-property 8 'part))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'part)
                                (get-text-property 8 'part)
                                (get-text-property 15 'part))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_cl_letf_nested_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // Nested cl-letf with edits.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (require 'cl-lib)
  (let ((buf (generate-new-buffer " combo-clletfn")))
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
        (cl-letf (((symbol-function 'outer-fn)
                   (lambda ()
                     (cl-letf (((symbol-function 'inner-fn)
                                (lambda () (insert "-INNER-"))))
                       (funcall 'inner-fn)))))
          (goto-char 5)
          (funcall 'outer-fn)
          (goto-char (point-max))
          (insert "-OUTER"))
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
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

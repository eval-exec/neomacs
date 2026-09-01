//! Deep combo: obarray × intern × unintern × mapatoms × symbol-name ×
//! symbol-plist × symbol-function × marker × overlay × textprop ×
//! undo × buffer-local × narrow.
//!
//! Stresses obarray and symbol operations with buffer state: interning
//! symbols, mapping over obarrays, and symbol property manipulation
//! during edits. Obarray operations are tricky because they involve
//! global state that must interact correctly with the buffer's edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_obarray_intern_mapatoms_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-oam"))
        (my-obarray (make-vector 7 0)))
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
        ;; Intern symbols
        (intern "my-sym-a" my-obarray)
        (intern "my-sym-b" my-obarray)
        (intern "my-sym-c" my-obarray)
        ;; Set properties
        (put (intern-soft "my-sym-a" my-obarray) 'data '(1 2 3))
        (put (intern-soft "my-sym-b" my-obarray) 'data '(4 5 6))
        ;; Map and collect
        (let ((syms nil))
          (mapatoms (lambda (s) (push (symbol-name s) syms)) my-obarray)
          (setq syms (sort syms 'string<)))
        ;; Edit buffer
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone)
                           (get (intern-soft "my-sym-a" my-obarray) 'data)
                           (get (intern-soft "my-sym-b" my-obarray) 'data)
                           (intern-soft "my-sym-c" my-obarray))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone)
                                (get (intern-soft "my-sym-a" my-obarray) 'data)
                                (get (intern-soft "my-sym-b" my-obarray) 'data))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_obarray_unintern_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-oun"))
        (my-obarray (make-vector 7 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (intern "keep-sym" my-obarray)
        (intern "drop-sym" my-obarray)
        (put (intern-soft "keep-sym" my-obarray) 'data 'kept)
        (put (intern-soft "drop-sym" my-obarray) 'data 'dropped)
        (undo-boundary)
        (unintern "drop-sym" my-obarray)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone)
                           (intern-soft "keep-sym" my-obarray)
                           (intern-soft "drop-sym" my-obarray)
                           (get (intern-soft "keep-sym" my-obarray) 'data))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone)
                                (intern-soft "keep-sym" my-obarray)
                                (intern-soft "drop-sym" my-obarray)
                                (get (intern-soft "keep-sym" my-obarray) 'data)
                                (get (intern-soft "drop-sym" my-obarray) 'data))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_obarray_symbol_plist_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-osym"))
        (my-obarray (make-vector 7 0)))
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
        ;; Intern and manipulate symbol plists
        (let ((sym (intern "test-sym" my-obarray)))
          (put sym 'face 'bold)
          (put sym 'data '(x y z))
          (fset sym (lambda () "test"))
          ;; Edit buffer
          (goto-char 5)
          (insert "XX")
          (goto-char 15)
          (insert "YY")
          (let ((after (list (buffer-string)
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get sym 'face)
                             (get sym 'data)
                             (symbol-function sym)
                             (get-text-property 1 'grp)
                             (get-text-property 6 'grp)
                             (get-text-property 12 'grp)
                             (get-text-property 18 'grp))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get sym 'face)
                                  (get sym 'data)
                                  (symbol-function sym)
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
fn combo_obarray_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-onar"))
        (my-obarray (make-vector 7 0)))
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
        ;; Intern symbols
        (intern "nar-sym-a" my-obarray)
        (intern "nar-sym-b" my-obarray)
        (put (intern-soft "nar-sym-a" my-obarray) 'val 100)
        (put (intern-soft "nar-sym-b" my-obarray) 'val 200)
        ;; Narrow and edit
        (narrow-to-region 6 20)
        (goto-char (point-min))
        (insert "XX-")
        (widen)
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get (intern-soft "nar-sym-a" my-obarray) 'val)
                           (get (intern-soft "nar-sym-b" my-obarray) 'val)
                           (get-text-property 1 'sect)
                           (get-text-property 6 'sect)
                           (get-text-property 16 'sect)
                           (get-text-property 21 'sect))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get (intern-soft "nar-sym-a" my-obarray) 'val)
                                (get (intern-soft "nar-sym-b" my-obarray) 'val)
                                (get-text-property 1 'sect)
                                (get-text-property 6 'sect)
                                (get-text-property 11 'sect)
                                (get-text-property 16 'sect)
                                (get-text-property 21 'sect))))
            (kill-buffer buf)
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_obarray_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-oblv"))
        (my-obarray (make-vector 7 0)))
    (with-current-buffer buf
      (make-local-variable 'oa-local)
      (setq oa-local 'buffer-specific)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        ;; Intern and manipulate
        (let ((sym (intern "bl-sym" my-obarray)))
          (put sym 'local oa-local)
          (fset sym (lambda () oa-local))
          ;; Edit buffer
          (goto-char 5)
          (insert "XX")
          (let ((after (list (buffer-string)
                             oa-local
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get sym 'local)
                             (funcall (symbol-function sym))
                             (get-text-property 1 'zone)
                             (get-text-property 6 'zone))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  oa-local
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get sym 'local)
                                  (funcall (symbol-function sym))
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone)
                                  (get-text-property 11 'zone))))
              (kill-buffer buf)
              (list after restored))))))) "#,
        expect,
    );
}

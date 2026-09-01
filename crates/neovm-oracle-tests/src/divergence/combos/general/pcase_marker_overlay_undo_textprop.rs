//! Deep combo: pcase × pcase-let × pcase-dolist × marker × overlay ×
//! text-prop × undo × buffer-local × narrow.
//!
//! Stresses pcase pattern matching interaction with buffer state:
//! pcase-exhaustive, pcase-let, pcase-dolist with markers, overlays,
//! text properties, and undo. pcase is tricky in a Rust rewrite
//! because it involves complex macro expansion and pattern matching.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_pcase_let_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // pcase-let with edit inside; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pclet")))
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
        (pcase-let ((`(,a ,b ,c) '(10 20 30)))
          (goto-char 5)
          (insert (format "-<%d>-" a))
          (goto-char 13)
          (insert (format "-<%d>-" b))
          (goto-char (point-max))
          (insert (format "-<%d>-" c)))
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
fn combo_pcase_match_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // pcase match with edit inside; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pcmatch")))
    (with-current-buffer buf
      (insert "HELLO-WORLD")
      (put-text-property 1 6 'word 'hello)
      (put-text-property 7 12 'word 'world)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 6 t))
            (ov (make-overlay 1 12)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (pcase 'hello
          ('hello
           (goto-char 6)
           (insert "-BEAUTIFUL-"))
          ('world
           (goto-char 12)
           (insert "-GOODBYE-")))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'word)
                           (get-text-property 7 'word))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'word)
                                (get-text-property 7 'word))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_pcase_dolist_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // pcase-dolist with edit inside; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pcdol")))
    (with-current-buffer buf
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
        (pcase-dolist (`(,pos ,text) '((5 "-X-") (10 "-Y-") (15 "-Z-")))
          (goto-char pos)
          (insert text))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (marker-position m3)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (marker-position m3)
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
fn combo_pcase_exhaustive_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // pcase-exhaustive with edit inside; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pcexh")))
    (with-current-buffer buf
      (insert "START-MIDDLE-END")
      (put-text-property 1 6 'part 'start)
      (put-text-property 7 13 'part 'middle)
      (put-text-property 14 17 'part 'end)
      (let ((m1 (copy-marker 6 nil))
            (m2 (copy-marker 13 t))
            (ov (make-overlay 1 17)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (pcase-exhaustive 'middle
          ('start
           (goto-char 1)
           (insert "BEFORE-"))
          ('middle
           (goto-char 7)
           (insert "NEW-"))
          ('end
           (goto-char (point-max))
           (insert "-AFTER")))
        (let ((after (list (buffer-string)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'part)
                           (get-text-property 7 'part))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'part)
                                (get-text-property 7 'part)
                                (get-text-property 14 'part))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_pcase_lambda_marker_overlay_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    // pcase in lambda with edit inside; markers/overlays track.
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-pclam")))
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
        (funcall
         (lambda (val)
           (pcase val
             ('a (goto-char 5) (insert "-X-"))
             ('b (goto-char 10) (insert "-Y-"))
             ('c (goto-char 15) (insert "-Z-"))))
         'b)
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

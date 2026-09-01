//! Combo: condition-case + markers + overlays + undo + narrow.
//! Tests error handling with buffer state preservation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_condition_case_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ccm")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (condition-case err
            (progn
              (goto-char 6)
              (insert "XX-")
              (error "intentional error"))
          (error
           (let ((msg (error-message-string err)))
             (list msg
                   (marker-position m)
                   (overlay-start ov)
                   (overlay-end ov)
                   (buffer-string)
                   (get-text-property 1 'seg)))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_condition_case_narrow_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ccn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 10)
        (undo-boundary)
        (condition-case err
            (progn
              (goto-char (point-min))
              (insert "XX")
              (signal 'error '("test")))
          (error
           (let ((mp (marker-position m))
                 (os (overlay-start ov))
                 (oe (overlay-end ov))
                 (k (get-text-property (point-min) 'z))
                 (bs (buffer-substring (point-min) (point-max))))
             (primitive-undo 1 buffer-undo-list)
             (widen)
             (list mp os oe k bs
                   (marker-position m)
                   (buffer-string)))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_condition_case_clone_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ccc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "ccc-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (condition-case err
              (progn
                (goto-char 6)
                (insert "XX-")
                (signal 'error '("test")))
            (error
             (let ((mp (marker-position m))
                   (os (overlay-start ov))
                   (oe (overlay-end ov))
                   (s (buffer-string)))
               (primitive-undo 1 buffer-undo-list)
               (list mp os oe s
                     (marker-position m)
                     (buffer-string)))))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_condition_case_textprop_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "cct")))
    (with-current-buffer buf
      (insert "one-two-three")
      (put-text-property 1 4 'w 'one)
      (put-text-property 5 8 'w 'two)
      (put-text-property 9 14 'w 'three)
      (let* ((ov (make-overlay 5 8))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 6)))
        (undo-boundary)
        (condition-case err
            (progn
              (goto-char 5)
              (insert "XX-")
              (error "boom"))
          (error
           (let ((mp (marker-position m))
                 (os (overlay-start ov))
                 (oe (overlay-end ov))
                 (k (get-text-property 5 'w))
                 (s (buffer-string)))
             (list mp os oe k s
                   (marker-position m)))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_condition_case_multi_error_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "cce")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (let ((results nil))
          (condition-case err
              (progn
                (goto-char 6)
                (insert "XX")
                (push (list (marker-position m)
                            (overlay-start ov1)
                            (overlay-end ov1))
                      results)
                (signal 'error '("first")))
            (error
             (push (list (marker-position m)
                         (overlay-start ov1)
                         (buffer-string))
                   results)))
          (condition-case err
              (progn
                (goto-char 12)
                (insert "YY")
                (push (list (marker-position m)
                            (overlay-start ov2)
                            (overlay-end ov2))
                      results)
                (signal 'error '("second")))
            (error
             (push (list (marker-position m)
                         (overlay-start ov2)
                         (buffer-string))
                   results)))
          (nreverse results))))
    (kill-buffer buf)))"#,
        expect,
    );
}

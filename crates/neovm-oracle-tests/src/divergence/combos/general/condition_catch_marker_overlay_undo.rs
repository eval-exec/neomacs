//! Combo: condition-case + catch/throw combined + markers + overlays + undo.
//! Tests combined error handling with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_condition_catch_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ccx")))
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
            (catch 'done
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
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_condition_catch_narrow_marker() {
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
            (catch 'exit
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
fn combo_condition_catch_clone_overlay() {
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
             (clone (clone-buffer "ccx-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (condition-case err
              (catch 'done
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
fn combo_catch_condition_multi_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "cmx")))
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
        (let ((r1 (condition-case err
                      (catch 'a
                        (goto-char 6)
                        (insert "XX")
                        (signal 'error '("first")))
                    (error
                     (list (marker-position m)
                           (overlay-start ov1)
                           (overlay-end ov1)
                           (buffer-string)))))
              (r2 (condition-case err
                      (catch 'b
                        (goto-char 12)
                        (insert "YY")
                        (signal 'error '("second")))
                    (error
                     (list (marker-position m)
                           (overlay-start ov2)
                           (overlay-end ov2)
                           (buffer-string))))))
          (list r1 r2))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_condition_catch_textprop_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "cct")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 1 15)
        (undo-boundary)
        (condition-case err
            (catch 'done
              (goto-char 6)
              (insert "XX")
              (signal 'error '("test")))
          (error
           (let ((mp (marker-position m))
                 (os (overlay-start ov))
                 (oe (overlay-end ov))
                 (k (get-text-property 6 'z))
                 (bs (buffer-string)))
             (primitive-undo 1 buffer-undo-list)
             (widen)
             (list mp os oe k bs
                   (marker-position m)
                   (buffer-string)))))))
    (kill-buffer buf)))"#,
        expect,
    );
}

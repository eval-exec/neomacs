//! Combo: overlay + marker + textprop + undo + evaporate + clone.
//! Tests complex evaporate+clone interactions with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_evaporate_clone_marker_textprop_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ecm")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "ecm-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (delete-region 6 10)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (ev (overlay-get ov 'evaporate))
                (k (get-text-property 1 'kind))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe ev k s
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (get-text-property 6 'kind)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_evaporate_clone_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ecn")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "ecn-clone")))
        (with-current-buffer clone
          (narrow-to-region 6 10)
          (undo-boundary)
          (delete-region (point-min) (point-max))
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (ev (overlay-get ov 'evaporate))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list mp os oe ev bs
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_evaporate_clone_multi_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "eco")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (put-text-property 16 20 'z 'd)
      (let* ((ov1 (make-overlay 1 10))
             (ov2 (make-overlay 11 20))
             (_ (overlay-put ov1 'evaporate t))
             (_ (overlay-put ov2 'evaporate t))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "eco-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (delete-region 6 10)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os1 (overlay-start ov1))
                (oe1 (overlay-end ov1))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os1 oe1 s
                  (marker-position m)
                  (overlay-start ov1)
                  (overlay-end ov1)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_evaporate_clone_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ecr")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "ecr-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (goto-char 6)
          (re-search-forward "BBBB")
          (replace-match "XX")
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (ev (overlay-get ov 'evaporate))
                (k (get-text-property 1 'kind))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe ev k s
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_evaporate_clone_buflocal_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "ecb")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local my-var 'base)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (clone (clone-buffer "ecb-clone")))
        (with-current-buffer clone
          (setq-local my-var 'cloned)
          (undo-boundary)
          (delete-region 6 10)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (ev (overlay-get ov 'evaporate))
                (v my-var)
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe ev v s
                  (marker-position m)
                  (overlay-start ov)
                  (overlay-end ov)
                  my-var
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

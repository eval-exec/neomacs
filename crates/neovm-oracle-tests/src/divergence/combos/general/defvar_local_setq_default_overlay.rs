//! Combo: defvar-local + setq-default + markers + overlays + undo + narrow.
//! Tests default vs buffer-local variable interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_defvarlocal_setqdefault_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-test-var 'default-val)
  (let ((buf (generate-new-buffer "dvl")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (setq-default dvl-test-var 'default-changed)
        (setq-local dvl-test-var 'local-val)
        (undo-boundary)
        (goto-char 6)
        (insert "XX-")
        (undo-boundary)
        (let ((lv dvl-test-var)
              (dv (default-value 'dvl-test-var))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property 1 'seg))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list lv dv mp os oe k s
                dvl-test-var
                (default-value 'dvl-test-var)
                (marker-position m)
                (buffer-string)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_defvarlocal_narrow_clone_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-clone-var 'base)
  (let ((buf (generate-new-buffer "dvc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (put-text-property 11 15 'z 'c)
      (setq-local dvl-clone-var 'buf-local)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 6))
             (clone (clone-buffer "dvc-clone")))
        (with-current-buffer clone
          (narrow-to-region 6 10)
          (undo-boundary)
          (goto-char (point-min))
          (insert "XX")
          (undo-boundary)
          (let ((v dvl-clone-var)
                (dv (default-value 'dvl-clone-var))
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'z))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v dv mp os oe k bs
                  dvl-clone-var
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_defvarlocal_multi_buffer_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-multi-var 'shared)
  (let ((b1 (generate-new-buffer "dvm1"))
        (b2 (generate-new-buffer "dvm2")))
    (with-current-buffer b1
      (insert "AAAA-BBBB")
      (put-text-property 1 5 'z 'a)
      (put-text-property 6 10 'z 'b)
      (setq-local dvl-multi-var 'b1-val)
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 6)))
        (undo-boundary)
        (goto-char 1)
        (insert "XX-")
        (undo-boundary)
        (let ((v dvl-multi-var)
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list v mp os oe s
                dvl-multi-var
                (marker-position m)
                (buffer-string)))))
    (with-current-buffer b2
      (insert "CCCC-DDDD")
      (put-text-property 1 5 'z 'c)
      (put-text-property 6 10 'z 'd)
      (setq-local dvl-multi-var 'b2-val)
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'face 'italic))
             (m (make-marker))
             (_ (set-marker m 6)))
        (let ((v dvl-multi-var)
              (mp (marker-position m))
              (s (buffer-string)))
          (list v mp s))))
    (kill-buffer b1)
    (kill-buffer b2)))"#,
        expect,
    );
}

#[test]
fn combo_defvarlocal_setq_default_local_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-sdl-var 'init)
  (let ((buf (generate-new-buffer "dsl")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 8)))
        (setq-default dvl-sdl-var 'def-val)
        (setq-local dvl-sdl-var 'loc-val)
        (undo-boundary)
        (goto-char 6)
        (insert "XX")
        (undo-boundary)
        (let ((lv dvl-sdl-var)
              (dv (default-value 'dvl-sdl-var))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (s (buffer-string)))
          (set-default 'dvl-sdl-var 'new-def)
          (list lv dv mp os oe s
                dvl-sdl-var
                (default-value 'dvl-sdl-var)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_defvarlocal_overlay_narrow_undo_clone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar dvl-onuc-var 'base)
  (let ((buf (generate-new-buffer "don")))
    (with-current-buffer buf
      (insert "line1-line2-line3")
      (put-text-property 1 6 'ln 1)
      (put-text-property 7 12 'ln 2)
      (put-text-property 13 18 'ln 3)
      (setq-local dvl-onuc-var 'local)
      (let* ((ov (make-overlay 7 12))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 9))
             (clone (clone-buffer "don-clone")))
        (with-current-buffer clone
          (narrow-to-region 7 12)
          (undo-boundary)
          (goto-char (point-min))
          (insert "XX-")
          (undo-boundary)
          (let ((v dvl-onuc-var)
                (mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (k (get-text-property (point-min) 'ln))
                (bs (buffer-substring (point-min) (point-max))))
            (primitive-undo 1 buffer-undo-list)
            (widen)
            (list v mp os oe k bs
                  dvl-onuc-var
                  (marker-position m)
                  (buffer-string)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

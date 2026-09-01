//! Combo: cl-eieio auto-fill-mode + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests auto-fill interaction with EIEIO objects, text properties, and editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_auto_fill_insert_long_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fill-edit-snap ()
    ((step :initarg :step :accessor fes-step :initform "")
     (line-count :initarg :lines :accessor fes-lines :initform 0)
     (m-pos :initarg :m-pos :accessor fes-mp :initform 0)
     (buf-string :initarg :buf-string :accessor fes-bs :initform "")))
  (let* ((buf (generate-new-buffer "af1"))
         (snaps nil))
    (with-current-buffer buf
      (setq-local auto-fill-function 'do-auto-fill
                  fill-column 20
                  left-margin 0)
      (insert "Short.")
      (put-text-property 1 7 'zone 'start)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 1 7))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 4))
             (results nil))
        (undo-boundary)
        (push (fill-edit-snap :step "init"
                             :lines (count-lines (point-min) (point-max))
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (goto-char (point-max))
        (insert " This is a much longer line that should definitely trigger the auto fill mechanism.")
        (do-auto-fill)
        (push (fill-edit-snap :step "after-fill"
                             :lines (count-lines (point-min) (point-max))
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fes-step s) (fes-lines s) (fes-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fes-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_auto_fill_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fill-prop-snap ()
    ((step :initarg :step :accessor fps-step :initform "")
     (line-count :initarg :lines :accessor fps-lines :initform 0)
     (prop-at-1 :initarg :prop1 :accessor fps-p1 :initform nil)
     (buf-string :initarg :buf-string :accessor fps-bs :initform "")))
  (let* ((buf (generate-new-buffer "af2"))
         (snaps nil))
    (with-current-buffer buf
      (setq-local auto-fill-function 'do-auto-fill
                  fill-column 15)
      (insert "AAAAAAAAAAAAAA BBBBBBBBBBBBBBB CCCCCCCCCCCCCCC")
      (put-text-property 1 15 'face 'bold)
      (put-text-property 16 31 'face 'italic)
      (put-text-property 32 47 'face 'underline)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 10 35))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 20))
             (results nil))
        (undo-boundary)
        (push (fill-prop-snap :step "init"
                             :lines (count-lines (point-min) (point-max))
                             :prop1 (get-text-property 1 'face)
                             :buf-string (buffer-string)) snaps)
        (fill-region (point-min) (point-max))
        (push (fill-prop-snap :step "after-fill"
                             :lines (count-lines (point-min) (point-max))
                             :prop1 (get-text-property 1 'face)
                             :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fps-step s) (fps-lines s) (fps-p1 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fps-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_auto_fill_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fill-narrow-snap ()
    ((step :initarg :step :accessor fns-step :initform "")
     (narrow-bounds :initarg :narrow :accessor fns-narrow :initform nil)
     (line-count :initarg :lines :accessor fns-lines :initform 0)
     (buf-string :initarg :buf-string :accessor fns-bs :initform "")))
  (let* ((buf (generate-new-buffer "af3"))
         (snaps nil))
    (with-current-buffer buf
      (setq-local auto-fill-function 'do-auto-fill
                  fill-column 15)
      (insert "Line-one-short\nLine-two-is-very-long-and-should-break\nLine-three")
      (put-text-property 1 16 'zone 'a)
      (put-text-property 17 52 'zone 'b)
      (put-text-property 53 63 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 17 52))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 25))
             (results nil))
        (undo-boundary)
        (push (fill-narrow-snap :step "init"
                               :narrow (list (point-min) (point-max))
                               :lines (count-lines (point-min) (point-max))
                               :buf-string (buffer-string)) snaps)
        (save-restriction
          (narrow-to-region 17 52)
          (fill-region (point-min) (point-max))
          (push (fill-narrow-snap :step "narrow-fill"
                                 :narrow (list (point-min) (point-max))
                                 :lines (count-lines (point-min) (point-max))
                                 :buf-string (buffer-string)) snaps))
        (push (fill-narrow-snap :step "after-widen"
                               :narrow (list (point-min) (point-max))
                               :lines (count-lines (point-min) (point-max))
                               :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fns-step s) (fns-lines s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fns-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_auto_fill_marker_relocation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fill-marker-snap ()
    ((step :initarg :step :accessor fms-step :initform "")
     (m1-pos :initarg :m1 :accessor fms-m1 :initform 0)
     (m2-pos :initarg :m2 :accessor fms-m2 :initform 0)
     (buf-string :initarg :buf-string :accessor fms-bs :initform "")))
  (let* ((buf (generate-new-buffer "af4"))
         (snaps nil))
    (with-current-buffer buf
      (setq-local auto-fill-function 'do-auto-fill
                  fill-column 12)
      (insert "AAAAAAAAAAAA-BBBBBBBBBBBB-CCCCCCCCCCCC-DDDDDDDDDDDD")
      (put-text-property 1 13 'zone 'a)
      (put-text-property 14 26 'zone 'b)
      (put-text-property 27 39 'zone 'c)
      (put-text-property 40 52 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 14 39))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 14))
             (_ (set-marker m2 39))
             (results nil))
        (undo-boundary)
        (push (fill-marker-snap :step "init"
                               :m1 (marker-position m1)
                               :m2 (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (fill-region 1 52)
        (push (fill-marker-snap :step "after-fill"
                               :m1 (marker-position m1)
                               :m2 (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (fill-marker-snap :step "after-insert"
                               :m1 (marker-position m1)
                               :m2 (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fms-step s) (fms-m1 s) (fms-m2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m1=%d m2=%d ov=[%d,%d]"
                       results (marker-position m1) (marker-position m2)
                       (overlay-start ov) (overlay-end ov)))
        (put-text-property (1- (point-max)) (point-max) 'fms-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m1) (marker-position m2)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_auto_fill_undo_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fill-undo-snap ()
    ((step :initarg :step :accessor fus-step :initform "")
     (line-count :initarg :lines :accessor fus-lines :initform 0)
     (m-pos :initarg :m-pos :accessor fus-mp :initform 0)
     (buf-string :initarg :buf-string :accessor fus-bs :initform "")))
  (let* ((buf (generate-new-buffer "af5"))
         (snaps nil))
    (with-current-buffer buf
      (setq-local auto-fill-function 'do-auto-fill
                  fill-column 20)
      (insert "This is a very long line that should be filled by the fill-region function call below.")
      (put-text-property 1 10 'face 'bold)
      (put-text-property 11 30 'face 'italic)
      (put-text-property 31 50 'face 'underline)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 10 50))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (fill-undo-snap :step "init"
                             :lines (count-lines (point-min) (point-max))
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (fill-region (point-min) (point-max))
        (undo-boundary)
        (push (fill-undo-snap :step "after-fill"
                             :lines (count-lines (point-min) (point-max))
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (fill-undo-snap :step "after-undo"
                             :lines (count-lines (point-min) (point-max))
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fus-step s) (fus-lines s) (fus-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fus-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

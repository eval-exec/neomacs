//! Combo: cl-eieio jit-lock / font-lock interaction + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests font-lock and jit-lock behavior with EIEIO objects and editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_jit_lock_basic_face_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fl-snap ()
    ((step :initarg :step :accessor fs-step :initform "")
     (face-at-5 :initarg :face :accessor fs-face :initform nil)
     (m-pos :initarg :m-pos :accessor fs-mp :initform 0)
     (buf-string :initarg :buf-string :accessor fs-bs :initform "")))
  (let* ((buf (generate-new-buffer "fl1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "(defun foo ()\n  (let ((x 42))\n    (+ x 1)))")
      (setq-local my-snaps snaps)
      (let* ((m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (fl-snap :step "init"
                       :face (get-text-property 5 'face)
                       :m-pos (marker-position m)
                       :buf-string (buffer-string)) snaps)
        (font-lock-mode 1)
        (push (fl-snap :step "font-lock-on"
                       :face (get-text-property 5 'face)
                       :m-pos (marker-position m)
                       :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "ZZ")
        (push (fl-snap :step "after-edit"
                       :face (get-text-property 5 'face)
                       :m-pos (marker-position m)
                       :buf-string (buffer-string)) snaps)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (push (fl-snap :step "after-undo"
                         :face (get-text-property 5 'face)
                         :m-pos (marker-position m)
                         :buf-string (buffer-string)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fs-step s) (fs-face s) (fs-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (put-text-property (1- (point-max)) (point-max) 'fs-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              font-lock-mode)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_jit_lock_overlay_priority_faces() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fl-ov-snap ()
    ((step :initarg :step :accessor fos-step :initform "")
     (ov-face :initarg :ov-face :accessor fos-ovface :initform nil)
     (tp-face :initarg :tp-face :accessor fos-tpface :initform nil)
     (m-pos :initarg :m-pos :accessor fos-mp :initform 0)))
  (let* ((buf (generate-new-buffer "fl2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "(defun bar ()\n  (message \"hello\"))")
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 8 18))
             (_ (overlay-put ov 'face 'underline))
             (_ (overlay-put ov 'priority 10))
             (m (make-marker))
             (_ (set-marker m 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (fl-ov-snap :step "init"
                         :ov-face (overlay-get ov 'face)
                         :tp-face (get-text-property 10 'face)
                         :m-pos (marker-position m)) snaps)
        (font-lock-mode 1)
        (push (fl-ov-snap :step "font-lock"
                         :ov-face (overlay-get ov 'face)
                         :tp-face (get-text-property 10 'face)
                         :m-pos (marker-position m)) snaps)
        (put-text-property 8 18 'face 'bold)
        (push (fl-ov-snap :step "tp-override"
                         :ov-face (overlay-get ov 'face)
                         :tp-face (get-text-property 10 'face)
                         :m-pos (marker-position m)) snaps)
        (goto-char 5)
        (insert "QQ")
        (push (fl-ov-snap :step "edit"
                         :ov-face (overlay-get ov 'face)
                         :tp-face (get-text-property 10 'face)
                         :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fos-step s) (fos-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d ov-start=%d ov-end=%d"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fos-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                font-lock-mode))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_jit_lock_narrow_refontify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fl-narrow-snap ()
    ((step :initarg :step :accessor fns-step :initform "")
     (narrow-bounds :initarg :narrow :accessor fns-narrow :initform nil)
     (face-at-3 :initarg :face :accessor fns-face :initform nil)
     (m-pos :initarg :m-pos :accessor fns-mp :initform 0)))
  (let* ((buf (generate-new-buffer "fl3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "(defun baz ()\n  (+ 1 2 3)\n  (* 4 5))")
      (setq-local my-snaps snaps)
      (let* ((m (make-marker))
             (_ (set-marker m 15))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (font-lock-mode 1)
        (push (fl-narrow-snap :step "init"
                             :narrow (list (point-min) (point-max))
                             :face (get-text-property 3 'face)
                             :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 10 20)
          (push (fl-narrow-snap :step "narrow"
                               :narrow (list (point-min) (point-max))
                               :face (get-text-property 3 'face)
                               :m-pos (marker-position m)) snaps)
          (goto-char (point-min))
          (insert "WW")
          (push (fl-narrow-snap :step "edit-in-narrow"
                               :narrow (list (point-min) (point-max))
                               :face (get-text-property 3 'face)
                               :m-pos (marker-position m)) snaps))
        (push (fl-narrow-snap :step "widen"
                             :narrow (list (point-min) (point-max))
                             :face (get-text-property 3 'face)
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fns-step s) (fns-mp s))) snaps))
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
                font-lock-mode))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_jit_lock_buflocal_keywords() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fl-kw-snap ()
    ((step :initarg :step :accessor fks-step :initform "")
     (kw-len :initarg :kw-len :accessor fks-kwlen :initform 0)
     (face-at-3 :initarg :face :accessor fks-face :initform nil)
     (m-pos :initarg :m-pos :accessor fks-mp :initform 0)))
  (let* ((buf (generate-new-buffer "fl4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "(my-func arg1 arg2)")
      (setq-local my-snaps snaps)
      (let* ((m (make-marker))
             (_ (set-marker m 8))
             (results nil)
             (custom-kw
              (list (cons 'my-func 'font-lock-function-name-face))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (fl-kw-snap :step "init"
                         :kw-len (length (if (listp font-lock-keywords)
                                            font-lock-keywords 0))
                         :face (get-text-property 2 'face)
                         :m-pos (marker-position m)) snaps)
        (font-lock-mode 1)
        (setq-local font-lock-keywords
                    (append font-lock-keywords custom-kw))
        (font-lock-fontify-buffer)
        (push (fl-kw-snap :step "custom-kw"
                         :kw-len (length (if (listp font-lock-keywords)
                                            font-lock-keywords 0))
                         :face (get-text-property 2 'face)
                         :m-pos (marker-position m)) snaps)
        (goto-char 5)
        (insert "RR")
        (push (fl-kw-snap :step "after-edit"
                         :kw-len (length (if (listp font-lock-keywords)
                                            font-lock-keywords 0))
                         :face (get-text-property 2 'face)
                         :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fks-step s) (fks-kwlen s) (fks-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fks-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              (length (if (listp font-lock-keywords) font-lock-keywords 0))
              font-lock-mode)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_jit_lock_overlay_evaporate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fl-evap-snap ()
    ((step :initarg :step :accessor fes-step :initform "")
     (ov-alive :initarg :ov-alive :accessor fes-alive :initform nil)
     (face-at-10 :initarg :face :accessor fes-face :initform nil)
     (m-pos :initarg :m-pos :accessor fes-mp :initform 0)))
  (let* ((buf (generate-new-buffer "fl5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "(defun qux ()\n  (list 1 2 3))")
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 8 18))
             (_ (overlay-put ov 'face 'highlight))
             (_ (overlay-put ov 'evaporate t))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (font-lock-mode 1)
        (push (fl-evap-snap :step "init"
                           :ov-alive (overlay-live-p ov)
                           :face (get-text-property 10 'face)
                           :m-pos (marker-position m)) snaps)
        (delete-region 8 18)
        (push (fl-evap-snap :step "delete-region"
                           :ov-alive (overlay-live-p ov)
                           :face (get-text-property 10 'face)
                           :m-pos (marker-position m)) snaps)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (push (fl-evap-snap :step "after-undo"
                             :ov-alive (overlay-live-p ov)
                             :face (get-text-property 10 'face)
                             :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fes-step s) (fes-alive s) (fes-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (put-text-property (1- (point-max)) (point-max) 'fes-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              (if (overlay-live-p ov) (overlay-start ov) -1)
              font-lock-mode)))
    (kill-buffer buf)))"#,
        expect,
    );
}

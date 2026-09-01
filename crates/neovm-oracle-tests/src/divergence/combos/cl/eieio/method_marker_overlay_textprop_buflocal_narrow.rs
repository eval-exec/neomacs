//! Combo: cl-eieio method combination (call-next-method, :around, :before,
//! :after, :static) + overlays + markers + textprop + buflocal + narrow.
//! Tests complex method combination chains with editing operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_method_around_before_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass method-ctx ()
    ((call-log :initarg :log :accessor mc-log :initform nil)
     (buf-name :initarg :buf :accessor mc-buf :initform "")))
  (defclass method-ctx-child (method-ctx)
    ((extra :initarg :extra :accessor mce-extra :initform nil)))
  (defmethod mc-do-edit :before ((ctx method-ctx) at str)
    (push (format "before-base@%d" at) (mc-log ctx)))
  (defmethod mc-do-edit :after ((ctx method-ctx) at str)
    (push (format "after-base@%d" at) (mc-log ctx)))
  (defmethod mc-do-edit ((ctx method-ctx) at str)
    (with-current-buffer (mc-buf ctx)
      (goto-char at)
      (insert str))
    (push (format "primary-base@%d" at) (mc-log ctx)))
  (defmethod mc-do-edit :before ((ctx method-ctx-child) at str)
    (push (format "before-child@%d" at) (mc-log ctx)))
  (defmethod mc-do-edit :after ((ctx method-ctx-child) at str)
    (push (format "after-child@%d" at) (mc-log ctx)))
  (defmethod mc-do-edit ((ctx method-ctx-child) at str)
    (push (format "primary-child@%d" at) (mc-log ctx))
    (cl-call-next-method)
    (push (format "after-call-next@%d" at) (mc-log ctx)))
  (let* ((buf (generate-new-buffer "mc1"))
         (snaps nil)
         (ctx (method-ctx-child :log nil :buf (buffer-name buf) :extra nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-mc-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (mc-do-edit ctx 8 "XXX")
        (push (list "edit1" (mc-log ctx) (marker-position m)) results)
        (mc-do-edit ctx 15 "YYY")
        (push (list "edit2" (mc-log ctx) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S"
                       results))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mc-log t)
        (list (buffer-string)
              (length (mc-log ctx))
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (mc-log ctx))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_method_around_wrapper_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass around-ctx ()
    ((log :initarg :log :accessor ac-log :initform nil)
     (buf-name :initarg :buf :accessor ac-buf :initform "")
     (wrap-count :initarg :wrap :accessor ac-wrap :initform 0)))
  (defclass around-ctx-sub (around-ctx)
    ((tag :initarg :tag :accessor acs-tag :initform "")))
  (defmethod ac-do-edit :around ((ctx around-ctx-sub) at str)
    (push (format "around-start@%d" at) (ac-log ctx))
    (setf (ac-wrap ctx) (1+ (ac-wrap ctx)))
    (cl-call-next-method)
    (setf (ac-wrap ctx) (1+ (ac-wrap ctx)))
    (push (format "around-end@%d" at) (ac-log ctx)))
  (defmethod ac-do-edit ((ctx around-ctx) at str)
    (with-current-buffer (ac-buf ctx)
      (goto-char at)
      (insert str))
    (push (format "primary@%d" at) (ac-log ctx)))
  (let* ((buf (generate-new-buffer "mc2"))
         (snaps nil)
         (ctx (around-ctx-sub :log nil :buf (buffer-name buf) :wrap 0 :tag "sub")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-ac-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (ac-do-edit ctx 8 "XXX")
        (push (list "edit1" (ac-wrap ctx) (marker-position m)) results)
        (ac-do-edit ctx 15 "YYY")
        (push (list "edit2" (ac-wrap ctx) (marker-position m)) results)
        (save-restriction
          (narrow-to-region 5 25)
          (ac-do-edit ctx 10 "ZZ")
          (push (list "narrow-edit" (ac-wrap ctx) (marker-position m)) results))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S ac-log=%S"
                       results (reverse (ac-log ctx))))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ac-log t)
        (list (buffer-string)
              (ac-wrap ctx)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (ac-log ctx))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_method_dispatch_multilevel() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass base-obj () ((val :initarg :val :accessor bo-val :initform 0)))
  (defclass mid-obj (base-obj) ((mid :initarg :mid :accessor mo-mid :initform "")))
  (defclass leaf-obj (mid-obj) ((leaf :initarg :leaf :accessor lo-leaf :initform "")))
  (defmethod obj-edit ((obj base-obj) buf at str)
    (with-current-buffer buf
      (goto-char at)
      (insert str)
      (push (format "base@%d" at) (bo-val obj))))
  (defmethod obj-edit ((obj mid-obj) buf at str)
    (cl-call-next-method)
    (push (format "mid@%d" at) (bo-val obj)))
  (defmethod obj-edit ((obj leaf-obj) buf at str)
    (cl-call-next-method)
    (push (format "leaf@%d" at) (bo-val obj)))
  (let* ((buf (generate-new-buffer "mc3"))
         (snaps nil)
         (obj (leaf-obj :val nil :mid "M" :leaf "L")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-disp-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (obj-edit obj (current-buffer) 8 "XXX")
        (push (list "edit1" (bo-val obj) (marker-position m)) results)
        (obj-edit obj (current-buffer) 15 "YYY")
        (push (list "edit2" (bo-val obj) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S disp-log=%S"
                       results (bo-val obj)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'disp-log t)
        (list (buffer-string)
              (length (bo-val obj))
              (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_method_no_applicable_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass target-obj () ((name :initarg :name :accessor to-name :initform "")))
  (defclass wrong-obj () ((tag :initarg :tag :accessor wo-tag :initform "")))
  (defmethod no-applicable-method (generic &rest args)
    (list 'no-applicable-method (mapcar (lambda (a) (format "%S" a)) args)))
  (defmethod target-edit ((obj target-obj) buf at str)
    (with-current-buffer buf
      (goto-char at)
      (insert str)))
  (let* ((buf (generate-new-buffer "mc4"))
         (snaps nil)
         (good (target-obj :name "good"))
         (bad (wrong-obj :tag "bad"))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-nam-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (target-edit good (current-buffer) 8 "XXX")
        (push (list "good-edit" (buffer-string) (marker-position m)) results)
        (let ((no-method (condition-case err
                            (target-edit bad (current-buffer) 12 "YYY")
                          (error err))))
          (push (list "bad-edit" no-method (marker-position m)) results)
          (setq my-nam-log (cons "no-method-called" my-nam-log)))
        (target-edit good (current-buffer) 15 "ZZZ")
        (push (list "good-edit2" (buffer-string) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S nam-log=%S"
                       results (reverse my-nam-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'nam-log t)
        (list (buffer-string)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-nam-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_method_dispatch_with_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dispatch-ctx ()
    ((log :initarg :log :accessor dc-log :initform nil)
     (buf-name :initarg :buf :accessor dc-buf :initform "")))
  (defclass dispatch-ctx-a (dispatch-ctx)
    ((kind :initarg :kind :accessor dca-kind :initform "A")))
  (defclass dispatch-ctx-b (dispatch-ctx)
    ((kind :initarg :kind :accessor dcb-kind :initform "B")))
  (defmethod dc-edit ((ctx dispatch-ctx-a) at str)
    (with-current-buffer (dc-buf ctx)
      (goto-char at)
      (insert str)
      (push (format "A@%d:%S" at str) (dc-log ctx))))
  (defmethod dc-edit ((ctx dispatch-ctx-b) at str)
    (with-current-buffer (dc-buf ctx)
      (goto-char at)
      (insert str)
      (push (format "B@%d:%S" at str) (dc-log ctx))))
  (let* ((buf (generate-new-buffer "mc5"))
         (snaps nil)
         (ctx-a (dispatch-ctx-a :log nil :buf (buffer-name buf) :kind "A"))
         (ctx-b (dispatch-ctx-b :log nil :buf (buffer-name buf) :kind "B"))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-dc-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (dc-edit ctx-a 8 "XXX")
        (push (list "A-edit" (dc-log ctx-a) (marker-position m)) results)
        (dc-edit ctx-b 15 "YYY")
        (push (list "B-edit" (dc-log ctx-b) (marker-position m)) results)
        (dc-edit ctx-a 20 "ZZZ")
        (push (list "A-edit2" (dc-log ctx-a) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S dc-log-a=%S dc-log-b=%S"
                       results (dc-log ctx-a) (dc-log ctx-b)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'dc-log t)
        (list (buffer-string)
              (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

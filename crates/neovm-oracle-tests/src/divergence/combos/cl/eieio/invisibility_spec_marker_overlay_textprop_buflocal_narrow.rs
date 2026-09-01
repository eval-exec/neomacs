//! Combo: cl-eieio buffer-invisibility-spec + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests buffer-invisibility-spec manipulation with EIEIO objects and editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_invis_spec_add_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass invis-spec-snap ()
    ((step :initarg :step :accessor iss-step :initform "")
     (spec :initarg :spec :accessor iss-spec :initform nil)
     (buf-string :initarg :buf-string :accessor iss-bs :initform "")
     (m-pos :initarg :m-pos :accessor iss-mp :initform 0)))
  (let* ((buf (generate-new-buffer "is1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov1 (make-overlay 6 10))
             (ov2 (make-overlay 11 15))
             (_ (overlay-put ov1 'invisible 'hide-b))
             (_ (overlay-put ov1 'priority 1))
             (_ (overlay-put ov2 'invisible 'hide-c))
             (_ (overlay-put ov2 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (invis-spec-snap :step "init"
                              :spec buffer-invisibility-spec
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (add-to-invisibility-spec 'hide-b)
        (push (invis-spec-snap :step "hide-b"
                              :spec buffer-invisibility-spec
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (add-to-invisibility-spec 'hide-c)
        (push (invis-spec-snap :step "hide-c"
                              :spec buffer-invisibility-spec
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (remove-from-invisibility-spec 'hide-b)
        (push (invis-spec-snap :step "show-b"
                              :spec buffer-invisibility-spec
                              :buf-string (buffer-string)
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (iss-step s) (iss-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'iss-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov1) (overlay-end ov1)
                (overlay-start ov2) (overlay-end ov2)
                buffer-invisibility-spec))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_invis_spec_ellipso_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass ellipsis-snap ()
    ((step :initarg :step :accessor es-step :initform "")
     (spec :initarg :spec :accessor es-spec :initform nil)
     (buf-string :initarg :buf-string :accessor es-bs :initform "")
     (m-pos :initarg :m-pos :accessor es-mp :initform 0)))
  (let* ((buf (generate-new-buffer "is2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'invisible t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (ellipsis-snap :step "init"
                            :spec buffer-invisibility-spec
                            :buf-string (buffer-string)
                            :m-pos (marker-position m)) snaps)
        (add-to-invisibility-spec '(t . "..."))
        (push (ellipsis-snap :step "ellipsis"
                            :spec buffer-invisibility-spec
                            :buf-string (buffer-string)
                            :m-pos (marker-position m)) snaps)
        (remove-from-invisibility-spec '(t . "..."))
        (push (ellipsis-snap :step "no-ellipsis"
                            :spec buffer-invisibility-spec
                            :buf-string (buffer-string)
                            :m-pos (marker-position m)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (ellipsis-snap :step "after-edit"
                            :spec buffer-invisibility-spec
                            :buf-string (buffer-string)
                            :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (es-step s) (es-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'es-log t)
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
fn combo_eieio_invis_spec_narrow_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass invis-narrow-snap ()
    ((step :initarg :step :accessor ins-step :initform "")
     (narrow-bounds :initarg :narrow :accessor ins-narrow :initform nil)
     (spec :initarg :spec :accessor ins-spec :initform nil)
     (buf-string :initarg :buf-string :accessor ins-bs :initform "")))
  (let* ((buf (generate-new-buffer "is3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'invisible 'fold))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (invis-narrow-snap :step "init"
                                :narrow (list (point-min) (point-max))
                                :spec buffer-invisibility-spec
                                :buf-string (buffer-string)) snaps)
        (add-to-invisibility-spec 'fold)
        (push (invis-narrow-snap :step "hide-fold"
                                :narrow (list (point-min) (point-max))
                                :spec buffer-invisibility-spec
                                :buf-string (buffer-string)) snaps)
        (save-restriction
          (narrow-to-region 6 15)
          (push (invis-narrow-snap :step "narrow"
                                  :narrow (list (point-min) (point-max))
                                  :spec buffer-invisibility-spec
                                  :buf-string (buffer-string)) snaps))
        (push (invis-narrow-snap :step "widen"
                                :narrow (list (point-min) (point-max))
                                :spec buffer-invisibility-spec
                                :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ins-step s) (ins-spec s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ins-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                buffer-invisibility-spec))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_invis_spec_overlay_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass invis-props-snap ()
    ((step :initarg :step :accessor ips-step :initform "")
     (ov-alive :initarg :ov-alive :accessor ips-alive :initform nil)
     (ov-invis :initarg :ov-invis :accessor ips-invis :initform nil)
     (buf-string :initarg :buf-string :accessor ips-bs :initform "")))
  (let* ((buf (generate-new-buffer "is4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'invisible 'my-hide))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (invis-props-snap :step "init"
                              :ov-alive (overlay-live-p ov)
                              :ov-invis (overlay-get ov 'invisible)
                              :buf-string (buffer-string)) snaps)
        (add-to-invisibility-spec 'my-hide)
        (push (invis-props-snap :step "hidden"
                              :ov-alive (overlay-live-p ov)
                              :ov-invis (overlay-get ov 'invisible)
                              :buf-string (buffer-string)) snaps)
        (overlay-put ov 'invisible nil)
        (push (invis-props-snap :step "cleared"
                              :ov-alive (overlay-live-p ov)
                              :ov-invis (overlay-get ov 'invisible)
                              :buf-string (buffer-string)) snaps)
        (overlay-put ov 'invisible 'my-hide)
        (goto-char 3)
        (insert "XX")
        (push (invis-props-snap :step "after-edit"
                              :ov-alive (overlay-live-p ov)
                              :ov-invis (overlay-get ov 'invisible)
                              :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (goto-char (point-max))
        (insert (format " | snaps=%S m=%d"
                       (mapcar (lambda (s) (list (ips-step s) (ips-alive s) (ips-invis s))) snaps)
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ips-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (overlay-get ov 'face)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_invis_spec_undo_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass invis-undo-snap ()
    ((step :initarg :step :accessor ius-step :initform "")
     (spec-len :initarg :spec-len :accessor ius-slen :initform 0)
     (m-pos :initarg :m-pos :accessor ius-mp :initform 0)
     (buf-string :initarg :buf-string :accessor ius-bs :initform "")))
  (let* ((buf (generate-new-buffer "is5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'invisible 'fold))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (invis-undo-snap :step "init"
                              :spec-len (length buffer-invisibility-spec)
                              :m-pos (marker-position m)
                              :buf-string (buffer-string)) snaps)
        (add-to-invisibility-spec 'fold)
        (goto-char 3)
        (insert "XX")
        (undo-boundary)
        (push (invis-undo-snap :step "edit+spec"
                              :spec-len (length buffer-invisibility-spec)
                              :m-pos (marker-position m)
                              :buf-string (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (invis-undo-snap :step "after-undo"
                              :spec-len (length buffer-invisibility-spec)
                              :m-pos (marker-position m)
                              :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ius-step s) (ius-slen s) (ius-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (put-text-property (1- (point-max)) (point-max) 'ius-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (length buffer-invisibility-spec))))
    (kill-buffer buf)))"#,
        expect,
    );
}

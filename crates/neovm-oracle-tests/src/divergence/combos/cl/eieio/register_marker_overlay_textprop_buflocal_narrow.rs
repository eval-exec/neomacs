//! Combo: cl-eieio register/insert-register + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests register operations with EIEIO objects, markers, and text property preservation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_copy_to_register_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer rg1> 6 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass register-snap ()
    ((step :initarg :step :accessor rgs-step :initform "")
     (reg-content :initarg :content :accessor rgs-content :initform "")
     (has-props :initarg :has-props :accessor rgs-props :initform nil)))
  (let* ((buf (generate-new-buffer "rg1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'shadow)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (copy-to-register ?a 6 15 t)
        (push (register-snap :step "copy"
                            :content (get-register ?a)
                            :has-props t) snaps)
        (delete-region 6 15)
        (push (register-snap :step "after-delete"
                            :content (get-register ?a)
                            :has-props t) snaps)
        (goto-char 6)
        (insert-register ?a t)
        (push (register-snap :step "after-insert"
                            :content (get-register ?a)
                            :has-props (get-text-property 6 'face)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rgs-step s) (length (rgs-content rgs s)) (rgs-props rgs s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rgs-log t)
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
fn combo_eieio_register_rectangle_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass reg-rect-snap ()
    ((step :initarg :step :accessor rrs-step :initform "")
     (m1-pos :initarg :m1 :accessor rrs-m1 :initform 0)
     (m2-pos :initarg :m2 :accessor rrs-m2 :initform 0)
     (buf-string :initarg :buf-string :accessor rrs-bs :initform "")))
  (let* ((buf (generate-new-buffer "rg2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB\nCCCC-DDDD\nEEEE-FFFF\nGGGG-HHHH")
      (put-text-property 1 10 'zone 'a)
      (put-text-property 11 20 'zone 'b)
      (put-text-property 21 30 'zone 'c)
      (put-text-property 31 40 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 11 30))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 15))
             (results nil))
        (undo-boundary)
        (push (reg-rect-snap :step "init"
                            :m1 (marker-position m1)
                            :m2 (marker-position m2)
                            :buf-string (buffer-string)) snaps)
        (copy-rectangle-to-register ?r 5 15)
        (push (reg-rect-snap :step "copy-rect"
                            :m1 (marker-position m1)
                            :m2 (marker-position m2)
                            :buf-string (buffer-string)) snaps)
        (delete-region 5 15)
        (push (reg-rect-snap :step "after-delete"
                            :m1 (marker-position m1)
                            :m2 (marker-position m2)
                            :buf-string (buffer-string)) snaps)
        (goto-char 5)
        (insert-register ?r t)
        (push (reg-rect-snap :step "after-insert"
                            :m1 (marker-position m1)
                            :m2 (marker-position m2)
                            :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rrs-step s) (rrs-m1 s) (rrs-m2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m1=%d m2=%d"
                       results (marker-position m1) (marker-position m2)))
        (put-text-property (1- (point-max)) (point-max) 'rrs-log t)
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
fn combo_eieio_register_narrow_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass reg-narrow-snap ()
    ((step :initarg :step :accessor rns-step :initform "")
     (narrow-bounds :initarg :narrow :accessor rns-narrow :initform nil)
     (m-pos :initarg :m-pos :accessor rns-mp :initform 0)
     (buf-string :initarg :buf-string :accessor rns-bs :initform "")))
  (let* ((buf (generate-new-buffer "rg3"))
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
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (copy-to-register ?x 6 15 t)
        (push (reg-narrow-snap :step "copy"
                              :narrow (list (point-min) (point-max))
                              :m-pos (marker-position m)
                              :buf-string (buffer-string)) snaps)
        (save-restriction
          (narrow-to-region 6 15)
          (goto-char 8)
          (insert "YY")
          (push (reg-narrow-snap :step "narrow-edit"
                                :narrow (list (point-min) (point-max))
                                :m-pos (marker-position m)
                                :buf-string (buffer-string)) snaps))
        (push (reg-narrow-snap :step "after-widen"
                              :narrow (list (point-min) (point-max))
                              :m-pos (marker-position m)
                              :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert-register ?x t)
        (push (reg-narrow-snap :step "insert-reg"
                              :narrow (list (point-min) (point-max))
                              :m-pos (marker-position m)
                              :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rns-step s) (rns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rns-log t)
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
fn combo_eieio_register_marker_point_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass reg-point-snap ()
    ((step :initarg :step :accessor rp-step :initform "")
     (point :initarg :point :accessor rp-point :initform 0)
     (m-pos :initarg :m-pos :accessor rp-mp :initform 0)
     (buf-string :initarg :buf-string :accessor rp-bs :initform "")))
  (let* ((buf (generate-new-buffer "rg4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (point-to-register ?p)
        (push (reg-point-snap :step "save-point"
                             :point (point)
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (reg-point-snap :step "after-edit"
                             :point (point)
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (delete-region 3 5)
        (push (reg-point-snap :step "after-delete"
                             :point (point)
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rp-step s) (rp-point s) (rp-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rp-log t)
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
fn combo_eieio_register_window_config_save_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer rg5> 3 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass reg-wconf-snap ()
    ((step :initarg :step :accessor rw-step :initform "")
     (buf-string :initarg :buf-string :accessor rw-bs :initform "")
     (m-pos :initarg :m-pos :accessor rw-mp :initform 0)
     (ov-bounds :initarg :ov-bounds :accessor rw-ov :initform nil)))
  (let* ((buf (generate-new-buffer "rg5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (reg-wconf-snap :step "init"
                             :buf-string (buffer-string)
                             :m-pos (marker-position m)
                             :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps)
        (copy-to-register ?w 1 20 t)
        (goto-char 3)
        (insert "XX")
        (push (reg-wconf-snap :step "after-edit"
                             :buf-string (buffer-string)
                             :m-pos (marker-position m)
                             :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps)
        (delete-region 3 5)
        (push (reg-wconf-snap :step "after-delete"
                             :buf-string (buffer-string)
                             :m-pos (marker-position m)
                             :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rw-step s) (rw-mp s) (rw-ov s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rw-log t)
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

//! Combo: cl-eieio buffer-local variables + overlays + markers + textprop + narrow + undo.
//! Tests buffer-local variable interactions with EIEIO objects and editing operations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_buflocal_set_make_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buflocal-snap ()
    ((step :initarg :step :accessor bls-step :initform "")
     (fill-col :initarg :fill-col :accessor bls-fc :initform 0)
     (tab-width :initarg :tab-width :accessor bls-tw :initform 0)
     (buf-string :initarg :buf-string :accessor bls-bs :initform "")))
  (let* ((buf (generate-new-buffer "bl1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 9 'zone 'b)
      (put-text-property 10 13 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil))
        (undo-boundary)
        (push (buflocal-snap :step "init"
                            :fill-col fill-column
                            :tab-width tab-width
                            :buf-string (buffer-string)) snaps)
        (setq-local fill-column 40)
        (setq-local tab-width 8)
        (push (buflocal-snap :step "after-set"
                            :fill-col fill-column
                            :tab-width tab-width
                            :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (buflocal-snap :step "after-insert"
                            :fill-col fill-column
                            :tab-width tab-width
                            :buf-string (buffer-string)) snaps)
        (kill-local-variable 'fill-column)
        (push (buflocal-snap :step "after-kill-local"
                            :fill-col fill-column
                            :tab-width tab-width
                            :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (bls-step s) (bls-fc s) (bls-tw s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bls-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                fill-column tab-width))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_buflocal_default_value_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass default-snap ()
    ((step :initarg :step :accessor ds-step :initform "")
     (local-val :initarg :local :accessor ds-local :initform 0)
     (default-val :initarg :default :accessor ds-default :initform 0)
     (buf-string :initarg :buf-string :accessor ds-bs :initform "")))
  (let* ((buf (generate-new-buffer "bl2"))
         (snaps nil)
         (orig-fill-col (default-value 'fill-column)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 9 'zone 'b)
      (put-text-property 10 13 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 3 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (results nil))
        (undo-boundary)
        (push (default-snap :step "init"
                           :local fill-column
                           :default (default-value 'fill-column)
                           :buf-string (buffer-string)) snaps)
        (setq-local fill-column 50)
        (push (default-snap :step "local-set"
                           :local fill-column
                           :default (default-value 'fill-column)
                           :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (default-snap :step "after-edit"
                           :local fill-column
                           :default (default-value 'fill-column)
                           :buf-string (buffer-string)) snaps)
        (setq-default fill-column 100)
        (push (default-snap :step "after-default-set"
                           :local fill-column
                           :default (default-value 'fill-column)
                           :buf-string (buffer-string)) snaps)
        (setq-default fill-column orig-fill-col)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ds-step s) (ds-local s) (ds-default s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ds-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                fill-column (default-value 'fill-column)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_buflocal_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-buflocal-snap ()
    ((step :initarg :step :accessor nbs-step :initform "")
     (narrow-bounds :initarg :narrow :accessor nbs-narrow :initform nil)
     (fill-col :initarg :fill-col :accessor nbs-fc :initform 0)
     (buf-string :initarg :buf-string :accessor nbs-bs :initform "")))
  (let* ((buf (generate-new-buffer "bl3"))
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
        (push (narrow-buflocal-snap :step "init"
                                   :narrow (list (point-min) (point-max))
                                   :fill-col fill-column
                                   :buf-string (buffer-string)) snaps)
        (setq-local fill-column 50)
        (save-restriction
          (narrow-to-region 6 15)
          (push (narrow-buflocal-snap :step "narrow"
                                     :narrow (list (point-min) (point-max))
                                     :fill-col fill-column
                                     :buf-string (buffer-string)) snaps)
          (goto-char 8)
          (insert "XX")
          (push (narrow-buflocal-snap :step "narrow-edit"
                                     :narrow (list (point-min) (point-max))
                                     :fill-col fill-column
                                     :buf-string (buffer-string)) snaps))
        (push (narrow-buflocal-snap :step "widen"
                                   :narrow (list (point-min) (point-max))
                                   :fill-col fill-column
                                   :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (nbs-step s) (nbs-fc s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'nbs-log t)
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
fn combo_eieio_buflocal_eieio_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buf-config ()
    ((buf-name :initarg :buf-name :accessor bc-name :initform "")
     (threshold :initarg :threshold :accessor bc-thresh :initform 0)
     (enabled :initarg :enabled :accessor bc-enabled :initform t)))
  (let* ((buf (generate-new-buffer "bl4"))
         (cfg (buf-config :buf-name "bl4" :threshold 10 :enabled t))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-cfg cfg
                  my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (list 'init (bc-thresh cfg) (bc-enabled cfg)) results)
        (setf (bc-thresh cfg) 20)
        (setf (bc-enabled cfg) nil)
        (push (list 'after-set (bc-thresh cfg) (bc-enabled cfg)) results)
        (goto-char 3)
        (insert "XX")
        (push (list 'after-edit (bc-thresh cfg) (bc-enabled cfg) (marker-position m)) results)
        (when (< (buffer-size) (bc-thresh cfg))
          (setf (bc-enabled cfg) t))
        (push (list 'after-check (bc-thresh cfg) (bc-enabled cfg)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bc-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                results
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (bc-thresh my-cfg)
                (bc-enabled my-cfg)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_buflocal_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass buflocal-undo-snap ()
    ((step :initarg :step :accessor bus-step :initform "")
     (fill-col :initarg :fill-col :accessor bus-fc :initform 0)
     (case-fold :initarg :case-fold :accessor bus-cf :initform nil)
     (buf-string :initarg :buf-string :accessor bus-bs :initform "")))
  (let* ((buf (generate-new-buffer "bl5"))
         (snaps nil)
         (orig-case-fold case-fold-search))
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
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (buflocal-undo-snap :step "init"
                                 :fill-col fill-column
                                 :case-fold case-fold-search
                                 :buf-string (buffer-string)) snaps)
        (setq-local fill-column 80)
        (setq-local case-fold-search nil)
        (goto-char 3)
        (insert "XX")
        (undo-boundary)
        (push (buflocal-undo-snap :step "edit+buflocal"
                                 :fill-col fill-column
                                 :case-fold case-fold-search
                                 :buf-string (buffer-string)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (buflocal-undo-snap :step "after-undo"
                                 :fill-col fill-column
                                 :case-fold case-fold-search
                                 :buf-string (buffer-string)) snaps)
        (setq-local case-fold-search orig-case-fold)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (bus-step s) (bus-fc s) (bus-cf s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'bus-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              fill-column case-fold-search)))
    (kill-buffer buf)))"#,
        expect,
    );
}

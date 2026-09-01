//! Combo: cl-eieio slot lifecycle (slot-boundp, slot-makeunbound, slot-missing)
//! + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests slot binding state tracking through editing operations with EIEIO.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_slot_boundp_makeunbound_with_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass slot-tracker ()
    ((name :initarg :name :accessor st-name :initform "")
     (val :initarg :val :accessor st-val :initform 0)
     (extra :initarg :extra :accessor st-extra :initform nil)))
  (let* ((buf (generate-new-buffer "sl1"))
         (snaps nil)
         (obj (slot-tracker :name "test" :val 10 :extra (list 1 2))))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-sl-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil)
             (snap-slots
              (lambda ()
                (list (slot-boundp obj 'name)
                      (slot-boundp obj 'val)
                      (slot-boundp obj 'extra)
                      (st-val obj)))))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (funcall snap-slots)) results)
        (slot-makeunbound obj 'extra)
        (setq my-sl-log (cons "unbound-extra" my-sl-log))
        (push (list "unbound-extra"
                    (slot-boundp obj 'name)
                    (slot-boundp obj 'val)
                    (slot-boundp obj 'extra)
                    (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-sl-log (cons "ins@8" my-sl-log))
        (setf (st-val obj) (marker-position m))
        (push (list "edit"
                    (slot-boundp obj 'name)
                    (slot-boundp obj 'val)
                    (slot-boundp obj 'extra)
                    (st-val obj)
                    (marker-position m)) results)
        (setf (st-extra obj) (list 3 4))
        (setq my-sl-log (cons "rebind-extra" my-sl-log))
        (push (list "rebind"
                    (slot-boundp obj 'extra)
                    (st-extra obj)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S sl-log=%S"
                       results (reverse my-sl-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sl-log t)
        (list (buffer-string)
              (slot-boundp obj 'name)
              (slot-boundp obj 'val)
              (slot-boundp obj 'extra)
              (st-val obj)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-sl-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_slot_boundp_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass slot-narrow-obj ()
    ((tag :initarg :tag :accessor sno-tag :initform "")
     (data :initarg :data :accessor sno-data :initform nil)
     (count :initarg :count :accessor sno-count :initform 0)))
  (let* ((buf (generate-new-buffer "sl2"))
         (snaps nil)
         (obj (slot-narrow-obj :tag "narrow-test" :data (list 'a 'b) :count 0)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF-GGGG-HHHH")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (put-text-property 31 35 'zone 'g)
      (put-text-property 36 40 'zone 'h)
      (setq-local my-sno-log nil)
      (let* ((ov (make-overlay 10 30))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 18))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (slot-boundp obj 'tag) (slot-boundp obj 'data)
                    (sno-count obj) (marker-position m)) results)
        (slot-makeunbound obj 'data)
        (setq my-sno-log (cons "unbound-data" my-sno-log))
        (save-restriction
          (narrow-to-region 8 28)
          (push (list "narrow" (slot-boundp obj 'data)
                      (sno-count obj) (marker-position m)) results)
          (goto-char 10)
          (insert "XXX")
          (setf (sno-count obj) (1+ (sno-count obj)))
          (setq my-sno-log (cons "ins-narrow@10" my-sno-log))
          (push (list "narrow-edit" (slot-boundp obj 'data)
                      (sno-count obj) (marker-position m)) results))
        (setf (sno-data obj) (list 'x 'y 'z))
        (setq my-sno-log (cons "rebind-data" my-sno-log))
        (push (list "rebind" (slot-boundp obj 'data)
                    (sno-data obj) (sno-count obj)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S sno-log=%S"
                       results (reverse my-sno-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sno-log t)
        (list (buffer-string)
              (slot-boundp obj 'tag)
              (slot-boundp obj 'data)
              (sno-count obj)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-sno-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_slot_missing_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass missing-obj ()
    ((name :initarg :name :accessor mo-name :initform "")))
  (defmethod slot-missing ((class (eql 'missing-obj)) obj slot-name op &optional new-val)
    (list 'slot-missing slot-name op new-val))
  (let* ((buf (generate-new-buffer "sl3"))
         (snaps nil)
         (obj (missing-obj :name "test")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (setq-local my-sm-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (mo-name obj)
                    (slot-boundp obj 'name)) results)
        (let ((missing-read (slot-value obj 'nonexistent)))
          (push (list "missing-read" missing-read) results))
        (setq my-sm-log (cons "read-nonexistent" my-sm-log))
        (goto-char 8)
        (insert "XXX")
        (setq my-sm-log (cons "ins@8" my-sm-log))
        (setf (mo-name obj) (format "edited-%d" (marker-position m)))
        (push (list "edit" (mo-name obj)
                    (marker-position m)) results)
        (let ((missing-set (setf (slot-value obj 'another-bad) 42)))
          (push (list "missing-set" missing-set) results))
        (setq my-sm-log (cons "set-nonexistent" my-sm-log))
        (push (list "after-missing" (mo-name obj)
                    (slot-boundp obj 'name)
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S sm-log=%S"
                       results (reverse my-sm-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sm-log t)
        (list (buffer-string)
              (mo-name obj)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-sm-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_slot_boundp_marker_in_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass marker-slot-obj ()
    ((label :initarg :label :accessor mso-label :initform "")
     (stored-marker :initarg :marker :accessor mso-marker :initform nil)))
  (let* ((buf (generate-new-buffer "sl4"))
         (snaps nil)
         (obj (marker-slot-obj :label "main" :marker nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (put-text-property 26 30 'zone 'f)
      (setq-local my-mso-log nil)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'face 'bold))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 12))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (slot-boundp obj 'stored-marker)
                    (mso-marker obj) (marker-position m)) results)
        (setf (mso-marker obj) m)
        (push (list "store-marker" (slot-boundp obj 'stored-marker)
                    (marker-position (mso-marker obj))
                    (marker-position m)) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-mso-log (cons "ins@8" my-mso-log))
        (push (list "edit" (marker-position (mso-marker obj))
                    (marker-position m)) results)
        (slot-makeunbound obj 'stored-marker)
        (setq my-mso-log (cons "unbound-marker" my-mso-log))
        (push (list "unbound" (slot-boundp obj 'stored-marker)
                    (marker-position m)) results)
        (setf (mso-marker obj) m)
        (push (list "rebind" (slot-boundp obj 'stored-marker)
                    (marker-position (mso-marker obj))
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S mso-log=%S"
                       results (reverse my-mso-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mso-log t)
        (list (buffer-string)
              (slot-boundp obj 'stored-marker)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              my-mso-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_slot_boundp_overlay_in_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass overlay-slot-obj ()
    ((ident :initarg :ident :accessor oso-ident :initform "")
     (stored-overlay :initarg :overlay :accessor oso-overlay :initform nil)
     (log :initarg :log :accessor oso-log :initform nil)))
  (let* ((buf (generate-new-buffer "sl5"))
         (snaps nil)
         (obj (overlay-slot-obj :ident "main" :overlay nil :log nil)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (put-text-property 16 20 'face 'default)
      (put-text-property 21 25 'face 'highlight)
      (setq-local my-oso-log nil)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'face 'shadow))
             (_ (overlay-put ov 'priority 5))
             (m (set-marker (make-marker) 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (list "init" (slot-boundp obj 'stored-overlay)
                    (overlay-live-p ov) (marker-position m)) results)
        (setf (oso-overlay obj) ov)
        (push (list "store-ov" (slot-boundp obj 'stored-overlay)
                    (overlay-live-p (oso-overlay obj))
                    (overlay-start (oso-overlay obj))) results)
        (goto-char 8)
        (insert "XXX")
        (setq my-oso-log (cons "ins@8" my-oso-log))
        (push (list "edit" (overlay-start (oso-overlay obj))
                    (overlay-end (oso-overlay obj))
                    (marker-position m)) results)
        (slot-makeunbound obj 'stored-overlay)
        (setq my-oso-log (cons "unbound-ov" my-oso-log))
        (push (list "unbound" (slot-boundp obj 'stored-overlay)
                    (overlay-live-p ov)) results)
        (delete-overlay ov)
        (setf (oso-overlay obj) ov)
        (push (list "store-dead" (slot-boundp obj 'stored-overlay)
                    (overlay-live-p (oso-overlay obj))
                    (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S oso-log=%S"
                       results (reverse my-oso-log)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'oso-log t)
        (list (buffer-string)
              (slot-boundp obj 'stored-overlay)
              (overlay-live-p ov)
              (marker-position m)
              my-oso-log)))
    (kill-buffer buf)))"#,
        expect,
    );
}

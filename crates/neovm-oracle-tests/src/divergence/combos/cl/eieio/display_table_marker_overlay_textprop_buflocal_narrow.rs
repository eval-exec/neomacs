//! Combo: cl-eieio buffer-display-table + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests buffer-display-table interactions with EIEIO objects and buffer editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_display_table_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass display-snap ()
    ((step :initarg :step :accessor dsp-step :initform "")
     (has-table :initarg :has-table :accessor dsp-has :initform nil)
     (border-glyph :initarg :border :accessor dsp-border :initform nil)
     (wrap-glyph :initarg :wrap :accessor dsp-wrap :initform nil)))
  (let* ((buf (generate-new-buffer "dt1"))
         (dt (make-display-table))
         (snaps nil))
    (with-current-buffer buf
      (aset dt 0 (list ?> ?>))
      (setq buffer-display-table dt)
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 15 'zone 'z3)
      (put-text-property 16 20 'zone 'z4)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (display-snap :step "init"
                           :has-table (if buffer-display-table t nil)
                           :border (aref dt 0)
                           :wrap (aref (or buffer-display-table (make-display-table)) 0))
              snaps)
        (goto-char 3)
        (insert "XX")
        (push (display-snap :step "after-insert"
                           :has-table (if buffer-display-table t nil)
                           :border (aref (or buffer-display-table (make-display-table)) 0)
                           :wrap nil) snaps)
        (delete-region 3 5)
        (push (display-snap :step "after-delete"
                           :has-table (if buffer-display-table t nil)
                           :border nil
                           :wrap nil) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (dsp-step s) (dsp-has s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'dsp-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (if buffer-display-table t nil)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_display_table_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dt-narrow-snap ()
    ((narrow-bounds :initarg :narrow :accessor dns-narrow :initform nil)
     (display-table :initarg :dt :accessor dns-dt :initform nil)
     (visible-string :initarg :visible :accessor dns-visible :initform "")))
  (let* ((buf (generate-new-buffer "dt2"))
         (dt (make-display-table))
         (snaps nil))
    (with-current-buffer buf
      (aset dt 0 (list ?#))
      (setq buffer-display-table dt)
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'section 's1)
      (put-text-property 6 10 'section 's2)
      (put-text-property 11 15 'section 's3)
      (put-text-property 16 20 'section 's4)
      (put-text-property 21 25 'section 's5)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (dt-narrow-snap :narrow (list (point-min) (point-max))
                             :dt (if buffer-display-table t nil)
                             :visible (buffer-substring-no-properties 1 20)) snaps)
        (save-restriction
          (narrow-to-region 6 15)
          (push (dt-narrow-snap :narrow (list (point-min) (point-max))
                               :dt (if buffer-display-table t nil)
                               :visible (buffer-substring-no-properties
                                        (point-min) (point-max))) snaps)
          (goto-char 8)
          (insert "XX"))
        (push (dt-narrow-snap :narrow (list (point-min) (point-max))
                             :dt (if buffer-display-table t nil)
                             :visible (buffer-substring-no-properties 1 22)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (dns-narrow s) (dns-dt s) (length (dns-visible s)))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'dns-log t)
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
fn combo_eieio_display_table_overlay_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function overlay-live-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dt-invis-snap ()
    ((step :initarg :step :accessor dis-step :initform "")
     (buf-string :initarg :buf-string :accessor dis-bs :initform "")
     (has-dt :initarg :has-dt :accessor dis-dt :initform nil)
     (ov-alive :initarg :ov-alive :accessor dis-alive :initform nil)))
  (let* ((buf (generate-new-buffer "dt3"))
         (dt (make-display-table))
         (snaps nil))
    (with-current-buffer buf
      (aset dt 0 (list ?@))
      (setq buffer-display-table dt)
      (insert "AAAAAAAAAAAAAAAAAAAA")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 12 'zone 'b)
      (put-text-property 13 20 'zone 'c)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 7 12))
             (_ (overlay-put ov 'invisible t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil))
        (undo-boundary)
        (push (dt-invis-snap :step "init"
                            :buf-string (buffer-string)
                            :has-dt (if buffer-display-table t nil)
                            :ov-alive (overlay-live-p ov)) snaps)
        (goto-char 3)
        (insert "PP")
        (push (dt-invis-snap :step "insert"
                            :buf-string (buffer-string)
                            :has-dt (if buffer-display-table t nil)
                            :ov-alive (overlay-live-p ov)) snaps)
        (delete-region 3 5)
        (push (dt-invis-snap :step "delete"
                            :buf-string (buffer-string)
                            :has-dt (if buffer-display-table t nil)
                            :ov-alive (overlay-live-p ov)) snaps)
        (setq buffer-display-table nil)
        (push (dt-invis-snap :step "no-dt"
                            :buf-string (buffer-string)
                            :has-dt (if buffer-display-table t nil)
                            :ov-alive (overlay-live-p ov)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (dis-step s) (dis-dt s) (dis-alive s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'dis-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (if buffer-display-table t nil)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_display_table_multibyte_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dt-multi-snap ()
    ((step :initarg :step :accessor dms-step :initform "")
     (buf-string :initarg :buf-string :accessor dms-bs :initform "")
     (buf-len :initarg :buf-len :accessor dms-len :initform 0)
     (has-dt :initarg :has-dt :accessor dms-dt :initform nil)))
  (let* ((buf (generate-new-buffer "dt4"))
         (dt (make-display-table))
         (snaps nil))
    (with-current-buffer buf
      (aset dt 0 (list ?~))
      (setq buffer-display-table dt)
      (insert "AAAA--BBBB--CCCC")
      (put-text-property 1 5 'face 'bold)
      (put-text-property 6 10 'face 'italic)
      (put-text-property 11 15 'face 'underline)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 5 11))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (undo-boundary)
        (push (dt-multi-snap :step "init"
                            :buf-string (buffer-string)
                            :buf-len (buffer-size)
                            :has-dt (if buffer-display-table t nil)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (dt-multi-snap :step "insert-ascii"
                            :buf-string (buffer-string)
                            :buf-len (buffer-size)
                            :has-dt (if buffer-display-table t nil)) snaps)
        (goto-char 8)
        (insert "\n\n")
        (push (dt-multi-snap :step "insert-newlines"
                            :buf-string (buffer-string)
                            :buf-len (buffer-size)
                            :has-dt (if buffer-display-table t nil)) snaps)
        (delete-region 3 5)
        (push (dt-multi-snap :step "delete"
                            :buf-string (buffer-string)
                            :buf-len (buffer-size)
                            :has-dt (if buffer-display-table t nil)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (dms-step s) (dms-len s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'dms-log t)
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
fn combo_eieio_display_table_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dt-undo-snap ()
    ((step :initarg :step :accessor dus-step :initform "")
     (dt-present :initarg :dt-present :accessor dus-dt :initform nil)
     (buf-string :initarg :buf-string :accessor dus-bs :initform "")))
  (let* ((buf (generate-new-buffer "dt5"))
         (dt (make-display-table))
         (snaps nil))
    (with-current-buffer buf
      (aset dt 0 (list ?*))
      (setq buffer-display-table dt)
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
        (push (dt-undo-snap :step "init"
                           :dt-present (if buffer-display-table t nil)
                           :buf-string (buffer-string)) snaps)
        (goto-char 3)
        (insert "XX")
        (push (dt-undo-snap :step "insert"
                           :dt-present (if buffer-display-table t nil)
                           :buf-string (buffer-string)) snaps)
        (setq buffer-display-table nil)
        (push (dt-undo-snap :step "clear-dt"
                           :dt-present (if buffer-display-table t nil)
                           :buf-string (buffer-string)) snaps)
        (undo-boundary)
        (primitive-undo 1 buffer-undo-list)
        (push (dt-undo-snap :step "after-undo"
                           :dt-present (if buffer-display-table t nil)
                           :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (dus-step s) (dus-dt s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'dus-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (if buffer-display-table t nil))))
    (kill-buffer buf)))"#,
        expect,
    );
}

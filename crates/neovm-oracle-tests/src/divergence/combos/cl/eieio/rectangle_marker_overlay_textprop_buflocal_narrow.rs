//! Combo: cl-eieio rectangle operations + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests rectangle insert/extract/delete/kill with EIEIO objects tracking.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_extract_rectangle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass rect-extract ()
    ((start-col :initarg :start-col :accessor re-start :initform 0)
     (end-col :initarg :end-col :accessor re-end :initform 0)
     (lines :initarg :lines :accessor re-lines :initform 0)
     (rect-string :initarg :rect :accessor re-rect :initform nil)))
  (let* ((buf (generate-new-buffer "rc1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAAAA-BBBB-CCCC\nDDDDDD-EEEE-FFFF\nGGGGGG-HHHH-IIII\nJJJJJJ-KKKK-LLLL")
      (put-text-property 1 15 'zone 'z1)
      (put-text-property 16 31 'zone 'z2)
      (put-text-property 32 47 'zone 'z3)
      (put-text-property 48 63 'zone 'z4)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 8 40))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (undo-boundary)
        (let ((rect (extract-rectangle 8 27)))
          (push (rect-extract :start-col 7 :end-col 11
                             :lines (length rect)
                             :rect (mapcar (lambda (s) (length s)) rect)) snaps))
        (let ((rect (extract-rectangle 1 47)))
          (push (rect-extract :start-col 0 :end-col 11
                             :lines (length rect)
                             :rect (mapcar (lambda (s) (length s)) rect)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (re-start s) (re-end s) (re-lines s) (re-rect s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 5)
        (put-text-property (1- (point-max)) (point-max) 're-log t)
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
fn combo_eieio_insert_rectangle_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 21 27)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass rect-insert-snap ()
    ((step :initarg :step :accessor ris-step :initform "")
     (m-pos :initarg :m-pos :accessor ris-mpos :initform 0)
     (m2-pos :initarg :m2-pos :accessor ris-m2pos :initform 0)
     (buf-string :initarg :buf-string :accessor ris-bs :initform "")))
  (let* ((buf (generate-new-buffer "rc2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 13 'zone 'b)
      (put-text-property 14 20 'zone 'c)
      (put-text-property 21 27 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 7 20))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 7))
             (_ (set-marker m2 14))
             (results nil))
        (undo-boundary)
        (push (rect-insert-snap :step "init" :m-pos (marker-position m1) :m2-pos (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (goto-char 4)
        (insert-rectangle (list "XX" "YY" "ZZ" "WW"))
        (push (rect-insert-snap :step "after-insert" :m-pos (marker-position m1) :m2-pos (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (delete-region 4 8)
        (push (rect-insert-snap :step "after-delete" :m-pos (marker-position m1) :m2-pos (marker-position m2)
                               :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ris-step s) (ris-mpos s) (ris-m2pos s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m1=%d m2=%d ov=[%d,%d]"
                       results (marker-position m1) (marker-position m2)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m1 3)
        (put-text-property (1- (point-max)) (point-max) 'ris-log t)
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
fn combo_eieio_delete_rectangle_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass rect-del-snap ()
    ((step :initarg :step :accessor rds-step :initform "")
     (m-pos :initarg :m-pos :accessor rds-mpos :initform 0)
     (line-count :initarg :lines :accessor rds-lines :initform 0)
     (buf-string :initarg :buf-string :accessor rds-bs :initform "")))
  (let* ((buf (generate-new-buffer "rc3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC\nDDDD-EEEE-FFFF\nGGGG-HHHH-IIII\nJJJJ-KKKK-LLLL")
      (put-text-property 1 14 'zone 'a)
      (put-text-property 15 29 'zone 'b)
      (put-text-property 30 44 'zone 'c)
      (put-text-property 45 59 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (undo-boundary)
        (push (rect-del-snap :step "init"
                            :m-pos (marker-position m)
                            :lines (count-lines (point-min) (point-max))
                            :buf-string (buffer-string)) snaps)
        (delete-rectangle 6 22)
        (push (rect-del-snap :step "after-delete"
                            :m-pos (marker-position m)
                            :lines (count-lines (point-min) (point-max))
                            :buf-string (buffer-string)) snaps)
        (delete-rectangle 1 8)
        (push (rect-del-snap :step "after-delete2"
                            :m-pos (marker-position m)
                            :lines (count-lines (point-min) (point-max))
                            :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rds-step s) (rds-mpos s) (rds-lines s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rds-log t)
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
fn combo_eieio_rect_narrow_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass rect-narrow-snap ()
    ((narrow-bounds :initarg :narrow :accessor rn-bounds :initform nil)
     (rect-contents :initarg :rect :accessor rn-rect :initform nil)
     (m-pos :initarg :m-pos :accessor rn-mpos :initform 0)))
  (let* ((buf (generate-new-buffer "rc4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC\nDDDD-EEEE-FFFF\nGGGG-HHHH-IIII\nJJJJ-KKKK-LLLL")
      (put-text-property 1 14 'zone 'a)
      (put-text-property 15 29 'zone 'b)
      (put-text-property 30 44 'zone 'c)
      (put-text-property 45 59 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 15 44))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 20))
             (results nil))
        (undo-boundary)
        (let ((rect (extract-rectangle 6 22)))
          (push (rect-narrow-snap :narrow (list (point-min) (point-max))
                                 :rect (mapcar (lambda (s) (length s)) rect)
                                 :m-pos (marker-position m)) snaps))
        (save-restriction
          (narrow-to-region 15 44)
          (goto-char 20)
          (insert "PP")
          (push (rect-narrow-snap :narrow (list (point-min) (point-max))
                                 :rect nil
                                 :m-pos (marker-position m)) snaps))
        (let ((rect (extract-rectangle 6 22)))
          (push (rect-narrow-snap :narrow (list (point-min) (point-max))
                                 :rect (mapcar (lambda (s) (length s)) rect)
                                 :m-pos (marker-position m)) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rn-bounds s) (rn-rect s) (rn-mpos s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rn-log t)
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
fn combo_eieio_rect_kill_yank_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass rect-kill-snap ()
    ((step :initarg :step :accessor rks-step :initform "")
     (rect-yanked-lines :initarg :lines :accessor rks-lines :initform 0)
     (m-pos :initarg :m-pos :accessor rks-mpos :initform 0)
     (buf-string :initarg :buf-string :accessor rks-bs :initform "")))
  (let* ((buf (generate-new-buffer "rc5"))
         (snaps nil)
         (killed-rect nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC\nDDDD-EEEE-FFFF\nGGGG-HHHH-IIII\nJJJJ-KKKK-LLLL")
      (put-text-property 1 14 'zone 'a)
      (put-text-property 15 29 'zone 'b)
      (put-text-property 30 44 'zone 'c)
      (put-text-property 45 59 'zone 'd)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 40))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (undo-boundary)
        (push (rect-kill-snap :step "init"
                             :lines 0
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (setq killed-rect (extract-rectangle 6 22))
        (delete-rectangle 6 22)
        (push (rect-kill-snap :step "after-kill"
                             :lines (length killed-rect)
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (goto-char (point-max))
        (open-line 1)
        (forward-line 1)
        (insert-rectangle killed-rect)
        (push (rect-kill-snap :step "after-yank"
                             :lines (length killed-rect)
                             :m-pos (marker-position m)
                             :buf-string (buffer-string)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (rks-step s) (rks-lines s) (rks-mpos s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'rks-log t)
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

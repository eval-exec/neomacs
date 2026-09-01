//! Combo: cl-eieio line-move/forward-line + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests line navigation with EIEIO objects, overlays, invisible text, and editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_forward_line_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 28 33)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass line-nav-snap ()
    ((step :initarg :step :accessor lns-step :initform "")
     (line-num :initarg :line :accessor lns-line :initform 0)
     (col :initarg :col :accessor lns-col :initform 0)
     (m-pos :initarg :m-pos :accessor lns-mp :initform 0)))
  (let* ((buf (generate-new-buffer "lm1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 13 'zone 'b)
      (put-text-property 14 20 'zone 'c)
      (put-text-property 21 27 'zone 'd)
      (put-text-property 28 33 'zone 'e)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 7 20))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (undo-boundary)
        (goto-char 1)
        (push (line-nav-snap :step "at-1"
                            :line (line-number-at-pos)
                            :col (current-column)
                            :m-pos (marker-position m)) snaps)
        (forward-line 1)
        (push (line-nav-snap :step "fwd-1"
                            :line (line-number-at-pos)
                            :col (current-column)
                            :m-pos (marker-position m)) snaps)
        (forward-line 2)
        (push (line-nav-snap :step "fwd-3"
                            :line (line-number-at-pos)
                            :col (current-column)
                            :m-pos (marker-position m)) snaps)
        (forward-line -1)
        (push (line-nav-snap :step "back-1"
                            :line (line-number-at-pos)
                            :col (current-column)
                            :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (lns-step s) (lns-line s) (lns-col s) (lns-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'lns-log t)
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
fn combo_eieio_line_move_invisible_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 38 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass invis-line-snap ()
    ((step :initarg :step :accessor ils-step :initform "")
     (line-num :initarg :line :accessor ils-line :initform 0)
     (buf-line-count :initarg :total-lines :accessor ils-total :initform 0)
     (m-pos :initarg :m-pos :accessor ils-mp :initform 0)))
  (let* ((buf (generate-new-buffer "lm2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "visible1\nhidden2\nhidden3\nvisible4\nvisible5")
      (put-text-property 1 9 'zone 'a)
      (put-text-property 10 18 'zone 'b)
      (put-text-property 19 27 'zone 'c)
      (put-text-property 28 37 'zone 'd)
      (put-text-property 38 46 'zone 'e)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 10 27))
             (_ (overlay-put ov 'invisible t))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (undo-boundary)
        (goto-char 1)
        (push (invis-line-snap :step "line-1"
                              :line (line-number-at-pos)
                              :total-lines (count-lines (point-min) (point-max))
                              :m-pos (marker-position m)) snaps)
        (forward-line 1)
        (push (invis-line-snap :step "line-2"
                              :line (line-number-at-pos)
                              :total-lines (count-lines (point-min) (point-max))
                              :m-pos (marker-position m)) snaps)
        (forward-line 1)
        (push (invis-line-snap :step "line-3"
                              :line (line-number-at-pos)
                              :total-lines (count-lines (point-min) (point-max))
                              :m-pos (marker-position m)) snaps)
        (overlay-put ov 'invisible nil)
        (goto-char 1)
        (forward-line 2)
        (push (invis-line-snap :step "visible-line-3"
                              :line (line-number-at-pos)
                              :total-lines (count-lines (point-min) (point-max))
                              :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ils-step s) (ils-line s) (ils-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ils-log t)
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
fn combo_eieio_line_move_narrow_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 34 39)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-line-snap ()
    ((step :initarg :step :accessor nls-step :initform "")
     (narrow-bounds :initarg :narrow :accessor nls-narrow :initform nil)
     (line-at-point :initarg :line :accessor nls-line :initform 0)
     (m-pos :initarg :m-pos :accessor nls-mp :initform 0)))
  (let* ((buf (generate-new-buffer "lm3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5\nline6")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 13 'zone 'b)
      (put-text-property 14 20 'zone 'c)
      (put-text-property 21 27 'zone 'd)
      (put-text-property 28 33 'zone 'e)
      (put-text-property 34 39 'zone 'f)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 7 27))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 15))
             (results nil))
        (undo-boundary)
        (push (narrow-line-snap :step "init"
                               :narrow (list (point-min) (point-max))
                               :line (line-number-at-pos)
                               :m-pos (marker-position m)) snaps)
        (save-restriction
          (narrow-to-region 7 27)
          (goto-char (point-min))
          (forward-line 1)
          (push (narrow-line-snap :step "narrow-line-2"
                                 :narrow (list (point-min) (point-max))
                                 :line (line-number-at-pos)
                                 :m-pos (marker-position m)) snaps)
          (goto-char 5)
          (insert "XXXX"))
        (push (narrow-line-snap :step "after-widen"
                               :narrow (list (point-min) (point-max))
                               :line (line-number-at-pos)
                               :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (nls-step s) (nls-line s) (nls-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'nls-log t)
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
fn combo_eieio_line_move_end_begin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass line-end-snap ()
    ((step :initarg :step :accessor les-step :initform "")
     (bol :initarg :bol :accessor les-bol :initform 0)
     (eol :initarg :eol :accessor les-eol :initform 0)
     (m-pos :initarg :m-pos :accessor les-mp :initform 0)))
  (let* ((buf (generate-new-buffer "lm4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB\nCCCC-DDDD\nEEEE-FFFF")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-snaps snaps)
      (let* ((ov (make-overlay 6 20))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (goto-char 3)
        (push (line-end-snap :step "line1"
                            :bol (line-beginning-position)
                            :eol (line-end-position)
                            :m-pos (marker-position m)) snaps)
        (end-of-line)
        (push (line-end-snap :step "eol-1"
                            :bol (line-beginning-position)
                            :eol (line-end-position)
                            :m-pos (marker-position m)) snaps)
        (forward-line 1)
        (beginning-of-line)
        (push (line-end-snap :step "bol-2"
                            :bol (line-beginning-position)
                            :eol (line-end-position)
                            :m-pos (marker-position m)) snaps)
        (end-of-line 1)
        (push (line-end-snap :step "eol-2"
                            :bol (line-beginning-position)
                            :eol (line-end-position)
                            :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (les-step s) (les-bol s) (les-eol s) (les-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'les-log t)
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
fn combo_eieio_line_move_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 21 27)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass line-undo-snap ()
    ((step :initarg :step :accessor lus-step :initform "")
     (line-num :initarg :line :accessor lus-line :initform 0)
     (buf-string :initarg :buf-string :accessor lus-bs :initform "")
     (m-pos :initarg :m-pos :accessor lus-mp :initform 0)))
  (let* ((buf (generate-new-buffer "lm5"))
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
             (m (make-marker))
             (_ (set-marker m 10))
             (results nil))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (push (line-undo-snap :step "init"
                             :line (line-number-at-pos (point))
                             :buf-string (buffer-string)
                             :m-pos (marker-position m)) snaps)
        (goto-char 3)
        (insert "XX\n")
        (undo-boundary)
        (push (line-undo-snap :step "after-insert"
                             :line (line-number-at-pos (point))
                             :buf-string (buffer-string)
                             :m-pos (marker-position m)) snaps)
        (forward-line 2)
        (push (line-undo-snap :step "fwd-2"
                             :line (line-number-at-pos (point))
                             :buf-string (buffer-string)
                             :m-pos (marker-position m)) snaps)
        (primitive-undo 1 buffer-undo-list)
        (push (line-undo-snap :step "after-undo"
                             :line (line-number-at-pos (point))
                             :buf-string (buffer-string)
                             :m-pos (marker-position m)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (lus-step s) (lus-line s) (lus-mp s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (put-text-property (1- (point-max)) (point-max) 'lus-log t)
        (list (buffer-string)
              (length snaps)
              (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

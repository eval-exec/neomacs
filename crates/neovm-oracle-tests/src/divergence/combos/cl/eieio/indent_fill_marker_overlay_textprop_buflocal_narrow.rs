//! Combo: cl-eieio indentation fill + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests indentation, fill-region, and line operations with EIEIO objects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_indent_region_with_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 28 33)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass indent-snap ()
    ((line-num :initarg :line :accessor is-line :initform 0)
     (indent :initarg :indent :accessor is-indent :initform 0)
     (content :initarg :content :accessor is-content :initform "")))
  (let* ((buf (generate-new-buffer "fl1"))
         (snaps nil))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 13 'zone 'b)
      (put-text-property 14 20 'zone 'c)
      (put-text-property 21 27 'zone 'd)
      (put-text-property 28 33 'zone 'e)
      (setq-local my-snaps snaps
                  indent-tabs-mode nil
                  tab-width 4)
      (let* ((ov (make-overlay 7 20))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (indent-rigidly 1 33 4)
        (dotimes (i 5)
          (goto-char (point-min))
          (forward-line i)
          (push (indent-snap :line (1+ i)
                            :indent (current-indentation)
                            :content (buffer-substring-no-properties
                                     (line-beginning-position) (line-end-position)))
                snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (is-line s) (is-indent s) (is-content s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'is-log t)
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
fn combo_eieio_fill_region_with_textprops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fill-snap ()
    ((step :initarg :step :accessor fs-step :initform "")
     (line-count :initarg :lines :accessor fs-lines :initform 0)
     (buf-string :initarg :buf-string :accessor fs-bs :initform "")
     (prop-at-1 :initarg :prop :accessor fs-prop :initform nil)))
  (let* ((buf (generate-new-buffer "fl2"))
         (snaps nil))
    (with-current-buffer buf
      (insert "This is a very long line that should be wrapped by the fill-region function when the fill-column is set to a small value like twenty.")
      (put-text-property 1 10 'face 'bold)
      (put-text-property 11 30 'face 'italic)
      (put-text-property 31 50 'face 'underline)
      (setq-local my-snaps snaps
                  fill-column 20
                  left-margin 0)
      (let* ((ov (make-overlay 11 50))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 15))
             (results nil))
        (undo-boundary)
        (push (fill-snap :step "before"
                        :lines (count-lines (point-min) (point-max))
                        :buf-string (buffer-string)
                        :prop (get-text-property 1 'face)) snaps)
        (fill-region (point-min) (point-max))
        (push (fill-snap :step "after-fill"
                        :lines (count-lines (point-min) (point-max))
                        :buf-string (buffer-string)
                        :prop (get-text-property 1 'face)) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fs-step s) (fs-lines s) (fs-prop s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'fs-log t)
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
fn combo_eieio_indent_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 28 33)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass indent-narrow-snap ()
    ((narrow-bounds :initarg :narrow :accessor ins-narrow :initform nil)
     (visible-string :initarg :visible :accessor ins-visible :initform "")
     (indent-at-line2 :initarg :indent :accessor ins-indent :initform 0)))
  (let* ((buf (generate-new-buffer "fl3"))
         (snaps nil))
    (with-current-buffer buf
      (insert "line1\nline2\nline3\nline4\nline5")
      (put-text-property 1 6 'zone 'a)
      (put-text-property 7 13 'zone 'b)
      (put-text-property 14 20 'zone 'c)
      (put-text-property 21 27 'zone 'd)
      (put-text-property 28 33 'zone 'e)
      (setq-local my-snaps snaps
                  indent-tabs-mode nil
                  tab-width 2)
      (let* ((ov (make-overlay 7 20))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil))
        (undo-boundary)
        (push (indent-narrow-snap :narrow (list (point-min) (point-max))
                                 :visible (buffer-substring-no-properties 1 33)
                                 :indent 0) snaps)
        (save-restriction
          (narrow-to-region 7 20)
          (push (indent-narrow-snap :narrow (list (point-min) (point-max))
                                   :visible (buffer-string)
                                   :indent 0) snaps)
          (indent-rigidly (point-min) (point-max) 6)
          (goto-char (point-min))
          (forward-line 1)
          (push (indent-narrow-snap :narrow (list (point-min) (point-max))
                                   :visible (buffer-string)
                                   :indent (current-indentation)) snaps))
        (push (indent-narrow-snap :narrow (list (point-min) (point-max))
                                 :visible (buffer-substring-no-properties 1 39)
                                 :indent (progn (goto-char 8) (current-indentation))) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ins-narrow s) (ins-indent s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d ov=[%d,%d]"
                       results (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ins-log t)
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
fn combo_eieio_newline_indent_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass newline-snap ()
    ((step :initarg :step :accessor ns-step :initform "")
     (m1-pos :initarg :m1 :accessor ns-m1 :initform 0)
     (m2-pos :initarg :m2 :accessor ns-m2 :initform 0)
     (buf-string :initarg :buf-string :accessor ns-bs :initform "")))
  (let* ((buf (generate-new-buffer "fl4"))
         (snaps nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-snaps snaps
                  indent-tabs-mode nil
                  tab-width 4)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m1 (make-marker))
             (m2 (make-marker))
             (_ (set-marker m1 5))
             (_ (set-marker m2 15))
             (_ (set-marker-insertion-type m1 t))
             (results nil))
        (add-hook 'post-command-hook
                  (lambda ()
                    (push (newline-snap :step "post-cmd"
                                       :m1 (marker-position m1)
                                       :m2 (marker-position m2)
                                       :buf-string (buffer-string)) snaps))
                  nil t)
        (undo-boundary)
        (goto-char 5)
        (newline-and-indent)
        (goto-char 10)
        (insert "    ")
        (goto-char 20)
        (newline 2)
        (remove-hook 'post-command-hook (car (default-value 'post-command-hook)) t)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (ns-m1 s) (ns-m2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%d m1=%d m2=%d ov=[%d,%d]"
                       (length results) (marker-position m1) (marker-position m2)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m1 3)
        (put-text-property (1- (point-max)) (point-max) 'ns-log t)
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
fn combo_eieio_fill_narrow_overlay_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 116 120)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass fill-narrow-snap ()
    ((step :initarg :step :accessor fns-step :initform "")
     (line-count :initarg :lines :accessor fns-lines :initform 0)
     (m-pos :initarg :m-pos :accessor fns-mpos :initform 0)
     (ov-bounds :initarg :ov-bounds :accessor fns-ov :initform nil)))
  (let* ((buf (generate-new-buffer "fl5"))
         (snaps nil))
    (with-current-buffer buf
      (insert "Short line.\nThis is a somewhat longer line that will be wrapped.\nAnother line here for testing fill behavior.\nEnd.")
      (put-text-property 1 12 'zone 'a)
      (put-text-property 13 70 'zone 'b)
      (put-text-property 71 115 'zone 'c)
      (put-text-property 116 120 'zone 'd)
      (setq-local my-snaps snaps
                  fill-column 30)
      (let* ((ov (make-overlay 13 70))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 20))
             (results nil))
        (undo-boundary)
        (push (fill-narrow-snap :step "init"
                               :lines (count-lines (point-min) (point-max))
                               :m-pos (marker-position m)
                               :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps)
        (fill-region 13 70)
        (push (fill-narrow-snap :step "after-fill"
                               :lines (count-lines (point-min) (point-max))
                               :m-pos (marker-position m)
                               :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps)
        (save-restriction
          (narrow-to-region 1 30)
          (fill-region (point-min) (point-max))
          (push (fill-narrow-snap :step "narrow-fill"
                                 :lines (count-lines (point-min) (point-max))
                                 :m-pos (marker-position m)
                                 :ov-bounds (list (overlay-start ov) (overlay-end ov))) snaps))
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (fns-step s) (fns-lines s) (fns-mpos s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%s m=%d"
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

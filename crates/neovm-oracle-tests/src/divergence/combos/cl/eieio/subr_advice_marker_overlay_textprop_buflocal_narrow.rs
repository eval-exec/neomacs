//! Combo: cl-eieio subr advice around buffer ops + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests advising subrs (insert, delete-region, goto-char) with EIEIO objects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_advice_insert_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass insert-advice-log ()
    ((call-count :initarg :call-count :accessor ial-count :initform 0)
     (args :initarg :args :accessor ial-args :initform nil)
     (buf-len-at-call :initarg :buf-len :accessor ial-blen :initform 0)))
  (let* ((buf (generate-new-buffer "ad1"))
         (call-count 0)
         (log-entries nil)
         (advice-obj (insert-advice-log)))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 14 'zone 'z3)
      (setq-local my-call-count call-count
                  my-log log-entries)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil)
             (my-advice (lambda (orig-fn &rest args)
                         (setq call-count (1+ call-count))
                         (push (insert-advice-log :call-count call-count
                                                 :args args
                                                 :buf-len (buffer-size))
                               log-entries)
                         (apply orig-fn args))))
        (advice-add 'insert :around my-advice)
        (undo-boundary)
        (goto-char 3)
        (insert "XX")
        (goto-char 10)
        (insert "YY")
        (push (list 'advised-inserts call-count (marker-position m)) results)
        (advice-remove 'insert my-advice)
        (goto-char 14)
        (insert "ZZ")
        (push (list 'unadvised-inserts call-count (marker-position m)) results)
        (setq log-entries (reverse log-entries))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s log=%d m=%d ov=[%d,%d]"
                       results (length log-entries) (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ial-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length log-entries)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_advice_delete_region_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass delete-advice-log ()
    ((call-count :initarg :call-count :accessor dal-count :initform 0)
     (range :initarg :range :accessor dal-range :initform nil)
     (deleted-text :initarg :deleted-text :accessor dal-text :initform "")))
  (let* ((buf (generate-new-buffer "ad2"))
         (call-count 0)
         (log-entries nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-call-count call-count
                  my-log log-entries)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil)
             (my-advice (lambda (orig-fn start end)
                         (setq call-count (1+ call-count))
                         (push (delete-advice-log :call-count call-count
                                                 :range (list start end)
                                                 :deleted-text (buffer-substring start end))
                               log-entries)
                         (funcall orig-fn start end))))
        (advice-add 'delete-region :around my-advice)
        (undo-boundary)
        (delete-region 3 6)
        (push (list 'delete-1 call-count (marker-position m)) results)
        (delete-region 5 8)
        (push (list 'delete-2 call-count (marker-position m)) results)
        (advice-remove 'delete-region my-advice)
        (delete-region 3 5)
        (push (list 'delete-3-unadvised call-count (marker-position m)) results)
        (setq log-entries (reverse log-entries))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s log=%d m=%d ov=[%d,%d]"
                       results (length log-entries) (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 2)
        (put-text-property (1- (point-max)) (point-max) 'dal-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length log-entries)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_advice_goto_char_with_fields() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass goto-advice-log ()
    ((call-count :initarg :call-count :accessor gal-count :initform 0)
     (target :initarg :target :accessor gal-target :initform 0)
     (actual-pos :initarg :actual :accessor gal-actual :initform 0)
     (field-at-pos :initarg :field :accessor gal-field :initform nil)))
  (let* ((buf (generate-new-buffer "ad3"))
         (call-count 0)
         (log-entries nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'field 'fa)
      (put-text-property 6 10 'field 'fb)
      (put-text-property 11 15 'field 'fc)
      (put-text-property 16 20 'field 'fd)
      (setq-local my-call-count call-count
                  my-log log-entries)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil)
             (my-advice (lambda (orig-fn pos)
                         (setq call-count (1+ call-count))
                         (funcall orig-fn pos)
                         (push (goto-advice-log :call-count call-count
                                               :target pos
                                               :actual (point)
                                               :field (get-text-property (point) 'field))
                               log-entries))))
        (advice-add 'goto-char :around my-advice)
        (undo-boundary)
        (goto-char 3)
        (goto-char 8)
        (goto-char 13)
        (goto-char 18)
        (push (list 'goto-count call-count) results)
        (advice-remove 'goto-char my-advice)
        (goto-char 1)
        (push (list 'after-remove call-count) results)
        (setq log-entries (reverse log-entries))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s log=%d m=%d"
                       results (length log-entries) (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'gal-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length log-entries)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_advice_narrow_to_region_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-advice-log ()
    ((call-count :initarg :call-count :accessor nal-count :initform 0)
     (range :initarg :range :accessor nal-range :initform nil)
     (visible-text :initarg :visible :accessor nal-visible :initform "")))
  (let* ((buf (generate-new-buffer "ad4"))
         (call-count 0)
         (log-entries nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'section 's1)
      (put-text-property 6 10 'section 's2)
      (put-text-property 11 15 'section 's3)
      (put-text-property 16 20 'section 's4)
      (put-text-property 21 25 'section 's5)
      (setq-local my-call-count call-count
                  my-log log-entries)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil)
             (my-advice (lambda (orig-fn start end)
                         (setq call-count (1+ call-count))
                         (funcall orig-fn start end)
                         (push (narrow-advice-log :call-count call-count
                                                 :range (list start end)
                                                 :visible (buffer-substring-no-properties
                                                          (point-min) (point-max)))
                               log-entries))))
        (advice-add 'narrow-to-region :around my-advice)
        (undo-boundary)
        (save-restriction
          (narrow-to-region 6 15)
          (push (list 'narrow1 call-count (buffer-string) (marker-position m)) results)
          (goto-char 8)
          (insert "XX")
          (push (list 'narrow-insert call-count (buffer-string) (marker-position m)) results))
        (push (list 'widen call-count (buffer-string) (marker-position m)) results)
        (advice-remove 'narrow-to-region my-advice)
        (save-restriction
          (narrow-to-region 1 10)
          (push (list 'narrow2-unadvised call-count (buffer-string)) results))
        (setq log-entries (reverse log-entries))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s log=%d m=%d"
                       results (length log-entries) (marker-position m)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'nal-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length log-entries)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_advice_buffer_substring_overlay_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass substring-advice-log ()
    ((call-count :initarg :call-count :accessor sal-count :initform 0)
     (range :initarg :range :accessor sal-range :initform nil)
     (result-len :initarg :result-len :accessor sal-rlen :initform 0)
     (has-props :initarg :has-props :accessor sal-props :initform nil)))
  (let* ((buf (generate-new-buffer "ad5"))
         (call-count 0)
         (log-entries nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'z1)
      (put-text-property 6 10 'zone 'z2)
      (put-text-property 11 15 'zone 'z3)
      (put-text-property 16 20 'zone 'z4)
      (setq-local my-call-count call-count
                  my-log log-entries)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil)
             (my-advice (lambda (orig-fn start end)
                         (setq call-count (1+ call-count))
                         (let ((result (funcall orig-fn start end)))
                           (push (substring-advice-log :call-count call-count
                                                      :range (list start end)
                                                      :result-len (length result)
                                                      :has-props (text-property-any 0 (length result) 'zone nil result))
                                 log-entries)
                           result))))
        (advice-add 'buffer-substring :around my-advice)
        (undo-boundary)
        (let ((s1 (buffer-substring 1 10)))
          (push (list 'sub1 (length s1) call-count) results))
        (let ((s2 (buffer-substring 6 20)))
          (push (list 'sub2 (length s2) call-count) results))
        (let ((s3 (buffer-substring-no-properties 1 20)))
          (push (list 'sub3-noprops (length s3) call-count) results))
        (advice-remove 'buffer-substring my-advice)
        (let ((s4 (buffer-substring 1 5)))
          (push (list 'sub4-unadvised (length s4) call-count) results))
        (setq log-entries (reverse log-entries))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%s log=%d m=%d ov=[%d,%d]"
                       results (length log-entries) (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'sal-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length log-entries)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

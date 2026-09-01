//! Combo: cl-eieio before/after-change hooks + markers + overlays + textprop + buflocal + narrow + undo.
//! Tests change hooks with EIEIO objects tracking buffer modifications.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_before_after_change_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass change-log ()
    ((change-type :initarg :change-type :accessor cl-type :initform "")
     (beg :initarg :beg :accessor cl-beg :initform 0)
     (end :initarg :end :accessor cl-end :initform 0)
     (len :initarg :len :accessor cl-len :initform 0)))
  (let* ((buf (generate-new-buffer "hk1"))
         (before-changes nil)
         (after-changes nil)
         (tracking-obj nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 14 'zone 'c)
      (setq-local my-before before-changes
                  my-after after-changes)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6)))
        (setq tracking-obj (list ov m))
        (add-hook 'before-change-functions
                  (lambda (beg end)
                    (push (change-log :change-type "before"
                                     :beg beg :end end :len (- end beg))
                          before-changes))
                  nil t)
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (change-log :change-type "after"
                                     :beg beg :end end :len len)
                          after-changes))
                  nil t)
        (undo-boundary)
        (goto-char 3)
        (insert "XXX")
        (goto-char 10)
        (insert "YYY")
        (setq before-changes (reverse before-changes))
        (setq after-changes (reverse after-changes))
        (let ((before-data (mapcar (lambda (c) (list (cl-type c) (cl-beg c) (cl-end c)))
                                   before-changes))
              (after-data (mapcar (lambda (c) (list (cl-type c) (cl-beg c) (cl-end c) (cl-len c)))
                                  after-changes)))
          (goto-char (point-max))
          (insert (format " | before=%s after=%s m=%d ov=[%d,%d]"
                         before-data after-data
                         (marker-position m) (overlay-start ov) (overlay-end ov)))
          (set-marker m 3)
          (put-text-property (1- (point-max)) (point-max) 'hook-log t))
        (remove-hook 'before-change-functions (car (default-value 'before-change-functions)) t)
        (remove-hook 'after-change-functions (car (default-value 'after-change-functions)) t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                tracking-obj))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_change_hooks_delete_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass delete-tracker ()
    ((pos :initarg :pos :accessor dt-pos :initform 0)
     (deleted-len :initarg :deleted-len :accessor dt-len :initform 0)
     (before-text :initarg :before-text :accessor dt-before :initform "")))
  (let* ((buf (generate-new-buffer "hk2"))
         (deletions nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 1)
      (put-text-property 6 10 'zone 2)
      (put-text-property 11 15 'zone 3)
      (put-text-property 16 20 'zone 4)
      (setq-local my-deletions deletions)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 11)))
        (add-hook 'before-change-functions
                  (lambda (beg end)
                    (when (> (- end beg) 0)
                      (push (delete-tracker :pos beg
                                           :deleted-len (- end beg)
                                           :before-text (buffer-substring beg end))
                            deletions)))
                  nil t)
        (undo-boundary)
        (delete-region 6 10)
        (delete-region 8 12)
        (setq deletions (reverse deletions))
        (let ((del-data (mapcar (lambda (d) (list (dt-pos d) (dt-len d) (dt-before d)))
                               deletions)))
          (goto-char (point-max))
          (insert (format " | deletions=%s m=%d ov=[%d,%d]"
                         del-data
                         (marker-position m) (overlay-start ov) (overlay-end ov)))
          (set-marker m 5)
          (put-text-property (1- (point-max)) (point-max) 'del-log t))
        (remove-hook 'before-change-functions (car (default-value 'before-change-functions)) t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-deletions))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_change_hooks_replace_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (cl-no-applicable-method re-op (propchange yellow))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass replace-event ()
    ((op :initarg :op :accessor re-op :initform "")
     (range :initarg :range :accessor re-range :initform nil)))
  (let* ((buf (generate-new-buffer "hk3"))
         (events nil))
    (with-current-buffer buf
      (insert "XXXX-YYYY-ZZZZ")
      (put-text-property 1 5 'color 'red)
      (put-text-property 6 9 'color 'blue)
      (put-text-property 10 13 'color 'green)
      (setq-local my-events events)
      (let* ((ov (make-overlay 1 9))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6)))
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (replace-event :op "after"
                                        :range (list beg end len))
                          events))
                  nil t)
        (undo-boundary)
        (goto-char 3)
        (insert "AA")
        (push (list 'insert1 (get-text-property 3 'color)) events)
        (delete-region 3 5)
        (push (list 'delete1 (get-text-property 3 'color)) events)
        (put-text-property 3 7 'color 'yellow)
        (push (list 'propchange (get-text-property 3 'color)) events)
        (setq events (reverse events))
        (goto-char (point-max))
        (insert (format " | events=%s m=%d"
                       (mapcar (lambda (e) (list (re-op e) (re-range e)))
                               (reverse events))
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'replace-log t)
        (remove-hook 'after-change-functions (car (default-value 'after-change-functions)) t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-events))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_change_hooks_narrow_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-change ()
    ((narrow-min :initarg :narrow-min :accessor nc-min :initform 1)
     (narrow-max :initarg :narrow-max :accessor nc-max :initform 1)
     (change-beg :initarg :change-beg :accessor nc-beg :initform 0)))
  (let* ((buf (generate-new-buffer "hk4"))
         (ncs nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'section 1)
      (put-text-property 6 10 'section 2)
      (put-text-property 11 15 'section 3)
      (put-text-property 16 20 'section 4)
      (setq-local my-ncs ncs)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 6))
             (results nil))
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (narrow-change :narrow-min (point-min)
                                        :narrow-max (point-max)
                                        :change-beg beg)
                          ncs))
                  nil t)
        (undo-boundary)
        (save-restriction
          (narrow-to-region 6 10)
          (goto-char 7)
          (insert "XX")
          (push (list 'narrow-insert (buffer-string) (marker-position m)) results))
        (push (list 'after-widen (buffer-string) (marker-position m)) results)
        (goto-char 13)
        (insert "YY")
        (push (list 'wide-insert (buffer-string) (marker-position m)) results)
        (setq ncs (reverse ncs))
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | ncs=%d results=%s m=%d"
                       (length ncs) results (marker-position m)))
        (set-marker m 4)
        (put-text-property (1- (point-max)) (point-max) 'nc-log t)
        (remove-hook 'after-change-functions (car (default-value 'after-change-functions)) t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-ncs))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_change_hooks_overlay_modification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (cl-no-applicable-method oe-beg (after-delete 5 14 6))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass overlay-event ()
    ((ov-start :initarg :ov-start :accessor oe-start :initform 0)
     (ov-end :initarg :ov-end :accessor oe-end :initform 0)
     (change-beg :initarg :change-beg :accessor oe-beg :initform 0)
     (change-end :initarg :change-end :accessor oe-end2 :initform 0)))
  (let* ((buf (generate-new-buffer "hk5"))
         (events nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'layer 'a)
      (put-text-property 6 9 'layer 'b)
      (put-text-property 10 13 'layer 'c)
      (setq-local my-events events)
      (let* ((ov (make-overlay 3 11))
             (_ (overlay-put ov 'priority 2))
             (m (make-marker))
             (_ (set-marker m 6)))
        (add-hook 'after-change-functions
                  (lambda (beg end len)
                    (push (overlay-event :ov-start (overlay-start ov)
                                        :ov-end (overlay-end ov)
                                        :change-beg beg
                                        :change-end end)
                          events))
                  nil t)
        (undo-boundary)
        (goto-char 1)
        (insert "PRE")
        (push (list 'pre-insert (overlay-start ov) (overlay-end ov) (marker-position m)) events)
        (goto-char 12)
        (insert "MID")
        (push (list 'mid-insert (overlay-start ov) (overlay-end ov) (marker-position m)) events)
        (goto-char (point-max))
        (insert "POST")
        (push (list 'post-insert (overlay-start ov) (overlay-end ov) (marker-position m)) events)
        (delete-region 5 8)
        (push (list 'after-delete (overlay-start ov) (overlay-end ov) (marker-position m)) events)
        (setq events (reverse events))
        (goto-char (point-max))
        (insert (format " | events=%d first=%s last=%s m=%d"
                       (length events)
                       (let ((e (car events))) (list (oe-beg e) (oe-end2 e)))
                       (let ((e (car (last events)))) (list (oe-beg e) (oe-end2 e)))
                       (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'oe-log t)
        (remove-hook 'after-change-functions (car (default-value 'after-change-functions)) t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs
                (marker-position m)
                (buffer-string)
                my-events))))
    (kill-buffer buf)))"#,
        expect,
    );
}

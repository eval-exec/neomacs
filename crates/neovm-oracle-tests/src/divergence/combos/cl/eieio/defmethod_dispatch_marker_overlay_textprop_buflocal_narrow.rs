//! Combo: cl-defmethod dispatch with EIEIO + overlays + markers + textprop + buflocal + narrow + undo.
//! Tests polymorphic dispatch on EIEIO objects stored in buffer-local vars with editing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_defmethod_dispatch_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass base-handler ()
    ((name :initarg :name :accessor bh-name :initform "")))
  (defclass text-handler (base-handler)
    ((transform :initarg :transform :accessor th-xform :initform nil)))
  (defclass overlay-handler (base-handler)
    ((priority :initarg :priority :accessor oh-pri :initform 0)))
  (cl-defgeneric process-edit (handler beg end &optional action)
    "Process an edit event.")
  (cl-defmethod process-edit ((h text-handler) beg end &optional action)
    (list 'text (bh-name h) beg end action))
  (cl-defmethod process-edit ((h overlay-handler) beg end &optional action)
    (list 'overlay (bh-name h) (oh-pri h) beg end action))
  (let* ((buf (generate-new-buffer "dm1"))
         (th (text-handler :name "txt" :transform 'upcase))
         (oh (overlay-handler :name "ov" :priority 5))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 9 'zone 'b)
      (put-text-property 10 13 'zone 'c)
      (setq-local my-th th my-oh oh)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5)))
        (undo-boundary)
        (push (process-edit th 1 5 'insert) results)
        (push (process-edit oh 6 10 'delete) results)
        (goto-char 3)
        (insert "XX")
        (push (process-edit th 3 5 'replace) results)
        (delete-region 3 5)
        (push (process-edit oh 3 5 'replace) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'dm-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length results)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (process-edit my-th 1 5 'check)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defmethod_dispatch_change_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass marker-policy ()
    ((name :initarg :name :accessor mp-name :initform "")))
  (defclass stick-policy (marker-policy) nil)
  (defclass advance-policy (marker-policy) nil)
  (cl-defgeneric apply-policy (policy m pos)
    "Apply marker policy.")
  (cl-defmethod apply-policy ((p stick-policy) m pos)
    (set-marker m pos)
    (list 'stick (mp-name p) (marker-position m)))
  (cl-defmethod apply-policy ((p advance-policy) m pos)
    (set-marker-insertion-type m t)
    (set-marker m pos)
    (list 'advance (mp-name p) (marker-position m) (marker-insertion-type m)))
  (let* ((buf (generate-new-buffer "dm2"))
         (policy (stick-policy :name "stick1"))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-policy policy)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5))
             (snaps nil))
        (undo-boundary)
        (push (apply-policy policy m 8) snaps)
        (goto-char 3)
        (insert "XX")
        (push (apply-policy policy m 10) snaps)
        (change-class policy 'advance-policy :name "advance1")
        (delete-region 3 5)
        (push (apply-policy policy m 3) snaps)
        (setq snaps (reverse snaps))
        (setq results (mapcar (lambda (s) (list (car s) (cadr s) (nth 2 s))) snaps))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d policy=%s"
                       results (marker-position m) (mp-name my-policy)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'mp-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length snaps)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (marker-insertion-type m)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defmethod_multi_dispatch_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"Unknown specializer base-handler\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass edit-action ()
    ((action-type :initarg :type :accessor ea-type :initform "")))
  (defclass insert-action (edit-action) nil)
  (defclass delete-action (edit-action) nil)
  (cl-defgeneric log-action (action pos len handler)
    "Log an action with handler.")
  (cl-defmethod log-action ((a insert-action) pos len (h base-handler))
    (list 'insert (ea-type a) pos len (bh-name h)))
  (cl-defmethod log-action ((a delete-action) pos len (h base-handler))
    (list 'delete (ea-type a) pos len (bh-name h)))
  (defclass base-handler ()
    ((name :initarg :name :accessor bh-name :initform "")))
  (let* ((buf (generate-new-buffer "dm3"))
         (handler (base-handler :name "main"))
         (ia (insert-action :type "char"))
         (da (delete-action :type "region"))
         (logs nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (setq-local my-handler handler my-logs logs)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (goto-char 3)
        (insert "XX")
        (push (log-action ia 3 2 handler) logs)
        (delete-region 5 8)
        (push (log-action da 5 3 handler) logs)
        (goto-char 10)
        (insert "YY")
        (push (log-action ia 10 2 handler) logs)
        (setq logs (reverse logs))
        (goto-char (point-max))
        (insert (format " | logs=%S m=%d ov=[%d,%d]"
                       logs (marker-position m)
                       (overlay-start ov) (overlay-end ov)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ea-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length logs)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (log-action ia 1 1 my-handler))))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defmethod_narrow_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 54 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass narrow-strategy ()
    ((label :initarg :label :accessor ns-label :initform "")))
  (defclass full-scope (narrow-strategy) nil)
  (defclass narrow-scope (narrow-strategy) nil)
  (cl-defgeneric get-scope-text (strategy buf ov m)
    "Get text according to strategy.")
  (cl-defmethod get-scope-text ((s full-scope) buf ov m)
    (with-current-buffer buf
      (list 'full (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov))))
  (cl-defmethod get-scope-text ((s narrow-scope) buf ov m)
    (with-current-buffer buf
      (save-restriction
        (narrow-to-region (overlay-start ov) (overlay-end ov))
        (list 'narrow (buffer-string) (marker-position m)
              (point-min) (point-max))))))
  (let* ((buf (generate-new-buffer "dm4"))
         (full-s (full-scope :label "full"))
         (narrow-s (narrow-scope :label "narrow"))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (put-text-property 16 20 'zone 'd)
      (put-text-property 21 25 'zone 'e)
      (setq-local my-full full-s my-narrow narrow-s)
      (let* ((ov (make-overlay 6 15))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (push (get-scope-text full-s buf ov m) results)
        (push (get-scope-text narrow-s buf ov m) results)
        (goto-char 3)
        (insert "XX")
        (push (get-scope-text full-s buf ov m) results)
        (push (get-scope-text narrow-s buf ov m) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (set-marker m 3)
        (put-text-property (1- (point-max)) (point-max) 'ns-log t)
        (undo-boundary)
        (let ((bs (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list bs (buffer-string)
                (length results)
                (marker-position m)
                (overlay-start ov) (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defmethod_undo_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 54 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass undo-strategy ()
    ((steps :initarg :steps :accessor us-steps :initform 1)))
  (defclass single-undo (undo-strategy) nil)
  (defclass multi-undo (undo-strategy) nil)
  (cl-defgeneric perform-undo (strategy buf)
    "Perform undo according to strategy.")
  (cl-defmethod perform-undo ((s single-undo) buf)
    (with-current-buffer buf
      (primitive-undo 1 buffer-undo-list)
      (list 'single (buffer-string) (us-steps s))))
  (cl-defmethod perform-undo ((s multi-undo) buf)
    (with-current-buffer buf
      (primitive-undo (us-steps s) buffer-undo-list)
      (list 'multi (buffer-string) (us-steps s)))))
  (let* ((buf (generate-new-buffer "dm5"))
         (single-s (single-undo :steps 1))
         (multi-s (multi-undo :steps 2))
         (results nil))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 9 'zone 'b)
      (put-text-property 10 13 'zone 'c)
      (setq-local my-single single-s my-multi multi-s)
      (let* ((ov (make-overlay 3 10))
             (_ (overlay-put ov 'priority 1))
             (m (make-marker))
             (_ (set-marker m 5)))
        (setq buffer-undo-list nil)
        (undo-boundary)
        (goto-char 3)
        (insert "XX")
        (undo-boundary)
        (delete-region 3 5)
        (undo-boundary)
        (goto-char 6)
        (insert "YY")
        (undo-boundary)
        (push (list 'before (buffer-string) (marker-position m)) results)
        (push (perform-undo single-s buf) results)
        (push (list 'after-single (buffer-string) (marker-position m)) results)
        (push (perform-undo multi-s buf) results)
        (push (list 'after-multi (buffer-string) (marker-position m)) results)
        (setq results (reverse results))
        (goto-char (point-max))
        (insert (format " | results=%S m=%d"
                       results (marker-position m)))
        (put-text-property (1- (point-max)) (point-max) 'us-log t)
        (list (buffer-string)
              (length results)
              (marker-position m)
              (overlay-start ov) (overlay-end ov))))
    (kill-buffer buf)))"#,
        expect,
    );
}

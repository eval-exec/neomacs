//! Combo: cl-eieio unwind-protect + catch/throw + marker + overlay + textprop + buflocal + undo.
//! Tests non-local exits through EIEIO method dispatch with buffer cleanup verification.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_unwind_protect_method_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass transaction ()
    ((id :initarg :id :accessor txn-id :initform 0)
     (state :initarg :state :accessor txn-state :initform 'open)
     (log :initarg :log :accessor txn-log :initform nil)))
  (defgeneric txn-execute (txn op)
    (:documentation "Execute a transaction operation."))
  (defmethod txn-execute ((txn transaction) op)
    (unwind-protect
        (progn
          (push (format "exec:%s" op) (txn-log txn))
          (setf (txn-state txn) 'running)
          (when (eq op 'fail)
            (error "txn-failed"))
          (setf (txn-state txn) 'committed)
          (format "ok:%s" op))
      (unless (eq (txn-state txn) 'committed)
        (setf (txn-state txn) 'rolled-back)
        (push "rolled-back" (txn-log txn)))))
  (let* ((buf (generate-new-buffer "up1"))
         (t1 (transaction :id 1))
         (t2 (transaction :id 2))
         (results nil)
         (errors nil))
    (with-current-buffer buf
      (insert "TXN:1:open-TXN:2:open")
      (put-text-property 1 4 'field 'type)
      (put-text-property 5 6 'field 'id1)
      (put-text-property 7 11 'field 'state1)
      (put-text-property 12 15 'field 'type)
      (put-text-property 16 17 'field 'id2)
      (put-text-property 18 22 'field 'state2)
      (setq-local txns (list t1 t2))
      (let* ((ov (make-overlay 7 22))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 9)))
        (undo-boundary)
        (condition-case err
            (push (txn-execute t1 'write) results)
          (error (push (format "err1:%s" err) errors)))
        (condition-case err
            (push (txn-execute t2 'fail) results)
          (error (push (format "err2:%s" (car (cdr err))) errors)))
        (condition-case err
            (push (txn-execute t2 'write) results)
          (error (push (format "err3:%s" err) errors)))
        (goto-char 7)
        (insert (format "%s:%s"
                       (txn-state t1) (txn-state t2)))
        (setf (marker-position m) 12)
        (put-text-property 7 (+ 7 (length (format "%s:%s" (txn-state t1) (txn-state t2))))
                          'txn-result t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (s1 (txn-state (car txns)))
              (s2 (txn-state (cadr txns)))
              (l1 (reverse (txn-log (car txns))))
              (l2 (reverse (txn-log (cadr txns)))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs s1 s2 l1 l2
                (reverse results) (reverse errors)
                (marker-position m)
                (buffer-string)
                txns)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_catch_throw_through_generics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass worker ()
    ((name :initarg :name :accessor worker-name :initform "")
     (status :initarg :status :accessor worker-status :initform 'idle)
     (tasks-done :initarg :tasks-done :accessor worker-tasks :initform 0)))
  (defgeneric worker-do-task (worker task)
    (:documentation "Execute a task."))
  (defmethod worker-do-task ((w worker) task)
    (incf (worker-tasks w))
    (setf (worker-status w) 'busy)
    (when (eq task 'abort)
      (throw 'worker-abort (format "aborted:%s" (worker-name w))))
    (when (eq task 'panic)
      (error "panic in %s" (worker-name w)))
    (setf (worker-status w) 'idle)
    (format "done:%s[#%d]" task (worker-tasks w)))
  (let* ((buf (generate-new-buffer "up2"))
         (w1 (worker :name "alpha" :tasks-done 0))
         (w2 (worker :name "beta" :tasks-done 0))
         (results nil)
         (catches nil))
    (with-current-buffer buf
      (insert "WORKER:alpha:idle-WORKER:beta:idle")
      (put-text-property 1 7 'field 'type)
      (put-text-property 8 13 'field 'name1)
      (put-text-property 14 18 'field 'status1)
      (put-text-property 19 25 'field 'type)
      (put-text-property 26 30 'field 'name2)
      (put-text-property 31 35 'field 'status2)
      (setq-local workers (list w1 w2))
      (let* ((ov (make-overlay 8 18))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (push (catch 'worker-abort
                (push (worker-do-task w1 'compile) results)
                (push (worker-do-task w1 'abort) results)
                (push (worker-do-task w1 'test) results)
                'no-abort)
              catches)
        (push (catch 'worker-abort
                (push (worker-do-task w2 'build) results)
                (push (worker-do-task w2 'deploy) results)
                'no-abort)
              catches)
        (goto-char 14)
        (insert (format "%s:%s" (worker-status w1) (worker-status w2)))
        (setf (marker-position m) 16)
        (put-text-property 14 (+ 14 (length (format "%s:%s" (worker-status w1) (worker-status w2))))
                          'worker-result t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (s1 (worker-status (car workers)))
              (s2 (worker-status (cadr workers)))
              (t1 (worker-tasks (car workers)))
              (t2 (worker-tasks (cadr workers))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs s1 s2 t1 t2
                (reverse results) (reverse catches)
                (marker-position m)
                (buffer-string)
                workers)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_unwind_nested_protect_generics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass resource-pool ()
    ((name :initarg :name :accessor pool-name :initform "")
     (acquired :initarg :acquired :accessor pool-acquired :initform 0)
     (released :initarg :released :accessor pool-released :initform 0)))
  (defgeneric pool-acquire (pool)
    (:documentation "Acquire from pool."))
  (defgeneric pool-release (pool)
    (:documentation "Release to pool."))
  (defmethod pool-acquire ((p resource-pool))
    (incf (pool-acquired p))
    (format "acquired[#%d]" (pool-acquired p)))
  (defmethod pool-release ((p resource-pool))
    (incf (pool-released p))
    (format "released[a=%d,r=%d]" (pool-acquired p) (pool-released p)))
  (let* ((buf (generate-new-buffer "up3"))
         (pool (resource-pool :name "main"))
         (ops nil))
    (with-current-buffer buf
      (insert "POOL:main:a=0:r=0")
      (put-text-property 1 5 'field 'type)
      (put-text-property 6 10 'field 'name)
      (put-text-property 11 14 'field 'acq)
      (put-text-property 15 18 'field 'rel)
      (setq-local my-pool pool)
      (let* ((ov (make-overlay 11 18))
             (_ (overlay-put ov 'face 'underline))
             (m (make-marker))
             (_ (set-marker m 13)))
        (undo-boundary)
        (unwind-protect
            (progn
              (push (pool-acquire pool) ops)
              (unwind-protect
                  (progn
                    (push (pool-acquire pool) ops)
                    (error "inner-failure"))
                (push (pool-release pool) ops)))
          (push (pool-release pool) ops))
        (unwind-protect
            (progn
              (push (pool-acquire pool) ops)
              (push (pool-acquire pool) ops))
          (push (pool-release pool) ops))
        (goto-char 11)
        (insert (format "a=%d:r=%d" (pool-acquired pool) (pool-released pool)))
        (setf (marker-position m) 15)
        (put-text-property 11 (+ 11 (length (format "a=%d:r=%d" (pool-acquired pool) (pool-released pool))))
                          'pool-result t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (a (pool-acquired my-pool))
              (r (pool-released my-pool)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs a r (reverse ops)
                (marker-position m)
                (buffer-string)
                my-pool)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_throw_through_method_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass step-result ()
    ((name :initarg :name :accessor step-name :initform "")
     (value :initarg :value :accessor step-value :initform 0)
     (error-count :initarg :error-count :accessor step-errors :initform 0)))
  (defgeneric process-step (sr)
    (:documentation "Process a step."))
  (defmethod process-step :before ((sr step-result))
    (when (< (step-value sr) 0)
      (throw 'negative-value (step-name sr))))
  (defmethod process-step ((sr step-result))
    (incf (step-value sr))
    (format "step[%s]=%d" (step-name sr) (step-value sr)))
  (defmethod process-step :after ((sr step-result))
    (when (> (step-value sr) 100)
      (incf (step-errors sr))))
  (let* ((buf (generate-new-buffer "up4"))
         (s1 (step-result :name "alpha" :value 5))
         (s2 (step-result :name "beta" :value -1))
         (s3 (step-result :name "gamma" :value 50))
         (results nil)
         (catches nil))
    (with-current-buffer buf
      (insert "STEPS:alpha=5:beta=-1:gamma=50")
      (put-text-property 1 6 'field 'header)
      (put-text-property 7 12 'field 's1name)
      (put-text-property 13 14 'field 's1val)
      (put-text-property 15 19 'field 's2name)
      (put-text-property 20 22 'field 's2val)
      (put-text-property 23 28 'field 's3name)
      (put-text-property 29 31 'field 's3val)
      (setq-local steps (list s1 s2 s3))
      (let* ((ov (make-overlay 7 28))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (push (catch 'negative-value
                (push (process-step s1) results)
                (push (process-step s2) results)
                'no-throw)
              catches)
        (push (catch 'negative-value
                (push (process-step s3) results)
                'no-throw)
              catches)
        (dotimes (_ 98)
          (process-step s3))
        (goto-char 13)
        (insert (format "v=%d:e=%d|v=%d:e=%d|v=%d:e=%d"
                       (step-value s1) (step-errors s1)
                       (step-value s2) (step-errors s2)
                       (step-value s3) (step-errors s3)))
        (setf (marker-position m) 18)
        (put-text-property 13 (+ 13 (length (format "v=%d:e=%d|v=%d:e=%d|v=%d:e=%d"
                                                      (step-value s1) (step-errors s1)
                                                      (step-value s2) (step-errors s2)
                                                      (step-value s3) (step-errors s3))))
                          'step-result t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (v1 (step-value (car steps)))
              (v2 (step-value (cadr steps)))
              (v3 (step-value (caddr steps)))
              (e3 (step-errors (caddr steps))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs v1 v2 v3 e3
                (reverse results) (reverse catches)
                (marker-position m)
                (buffer-string)
                steps)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_unwind_protect_buffer_state_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass guarded-buffer-op ()
    ((op-name :initarg :op-name :accessor gbo-name :initform "")
     (succeeded :initarg :succeeded :accessor gbo-succeeded :initform 0)
     (failed :initarg :failed :accessor gbo-failed :initform 0)))
  (defgeneric perform-op (gbo buf op)
    (:documentation "Perform a guarded buffer operation."))
  (defmethod perform-op ((gbo guarded-buffer-op) buf op)
    (with-current-buffer buf
      (unwind-protect
          (progn
            (let ((orig-point (point)))
              (goto-char (point-min))
              (cond
               ((eq op 'insert-front)
                (insert "FRONT:")
                (incf (gbo-succeeded gbo)))
               ((eq op 'insert-fail)
                (error "deliberate-fail"))
               ((eq op 'replace-all)
                (delete-region (point-min) (point-max))
                (insert "REPLACED")
                (incf (gbo-succeeded gbo)))
               ((eq op 'prop-set)
                (put-text-property (point-min) (point-max) 'guarded t)
                (incf (gbo-succeeded gbo))))
              (format "ok:%s" op))
            (incf (gbo-succeeded gbo)))
        (when (= (gbo-succeeded gbo) 0)
          (incf (gbo-failed gbo))))))
  (let* ((buf (generate-new-buffer "up5"))
         (gbo (guarded-buffer-op :op-name "test"))
         (results nil)
         (errors nil))
    (with-current-buffer buf
      (insert "INITIAL-CONTENT-HERE")
      (put-text-property 1 7 'field 'start)
      (put-text-property 8 15 'field 'content)
      (put-text-property 16 20 'field 'end)
      (setq-local op-handler gbo)
      (let* ((ov (make-overlay 8 15))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 10))
             (saved-text (buffer-string)))
        (undo-boundary)
        (condition-case err
            (push (perform-op gbo buf 'insert-front) results)
          (error (push (format "err1:%s" err) errors)))
        (condition-case err
            (push (perform-op gbo buf 'insert-fail) results)
          (error (push (format "err2:%s" (car (cdr err))) errors)))
        (condition-case err
            (push (perform-op gbo buf 'prop-set) results)
          (error (push (format "err3:%s" err) errors)))
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (succ (gbo-succeeded op-handler))
              (fail (gbo-failed op-handler))
              (tp (get-text-property 1 'guarded)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs succ fail tp saved-text
                (reverse results) (reverse errors)
                (marker-position m)
                (buffer-string)
                op-handler)))
      (kill-buffer buf))))"#,
        expect,
    );
}

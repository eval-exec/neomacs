//! Combo: cl-eieio advice on generic functions + marker + overlay + textprop + buflocal + undo.
//! Tests advice-add/advice-remove on EIEIO generic functions with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_advice_before_after_generic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass event-handler ()
    ((name :initarg :name :accessor handler-name :initform "")
     (count :initarg :count :accessor handler-count :initform 0)))
  (defgeneric handle-event (handler event-type data)
    (:documentation "Handle an event."))
  (defmethod handle-event ((handler event-handler) event-type data)
    (incf (handler-count handler))
    (format "handled[%s:%s:#%d]" event-type data (handler-count handler)))
  (let* ((buf (generate-new-buffer "ad1"))
         (h (event-handler :name "main" :count 0))
         (advice-log nil))
    (with-current-buffer buf
      (insert "HANDLER:main:count=0")
      (put-text-property 1 8 'field 'type)
      (put-text-property 9 13 'field 'name)
      (put-text-property 14 21 'field 'count)
      (setq-local handler h)
      (let* ((ov (make-overlay 9 13))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 11)))
        (undo-boundary)
        (let ((r1 (handle-event h 'click "button-1")))
          (push r1 advice-log))
        (let ((before-advice (lambda (handler &rest args)
                               (push (format "before:%s:%s" (handler-name handler) (car args)) advice-log))))
          (advice-add 'handle-event :before before-advice)
          (let ((r2 (handle-event h 'key "enter")))
            (push r2 advice-log))
          (let ((after-advice (lambda (handler &rest args)
                                (push (format "after:count=%d" (handler-count handler)) advice-log))))
            (advice-add 'handle-event :after after-advice)
            (let ((r3 (handle-event h 'mouse "move")))
              (push r3 advice-log))
            (advice-remove 'handle-event after-advice))
          (let ((r4 (handle-event h 'scroll "down")))
            (push r4 advice-log))
          (advice-remove 'handle-event before-advice))
        (let ((r5 (handle-event h 'resize "window")))
          (push r5 advice-log))
        (goto-char 14)
        (insert (format "count=%d[%s]" (handler-count h) (mapconcat #'identity (reverse advice-log) "|")))
        (setf (marker-position m) 20)
        (put-text-property 14 (+ 14 (length (format "count=%d[%s]"
                                                      (handler-count h) (mapconcat #'identity (reverse advice-log) "|"))))
                          'advice-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (final-count (handler-count handler)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs final-count
                (marker-position m)
                (buffer-string)
                handler)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_advice_around_generic_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass processor ()
    ((label :initarg :label :accessor proc-label :initform "")
     (calls :initarg :calls :accessor proc-calls :initform 0)))
  (defgeneric process-item (proc item)
    (:documentation "Process an item."))
  (defmethod process-item ((p processor) item)
    (incf (proc-calls p))
    (format "proc[%s:%s:#%d]" (proc-label p) item (proc-calls p)))
  (let* ((buf (generate-new-buffer "ad2"))
         (p (processor :label "worker" :calls 0))
         (around-count 0))
    (with-current-buffer buf
      (insert "PROC:worker:calls=0")
      (put-text-property 1 5 'field 'type)
      (put-text-property 6 12 'field 'label)
      (put-text-property 13 20 'field 'calls)
      (setq-local my-proc p)
      (let* ((ov (make-overlay 6 12))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 9))
             (results nil))
        (undo-boundary)
        (push (process-item p "alpha") results)
        (let ((around-fn (lambda (fn proc item)
                           (setq around-count (1+ around-count))
                           (let ((result (funcall fn proc item)))
                             (format "wrapped(%d)[%s]" around-count result)))))
          (advice-add 'process-item :around around-fn)
          (push (process-item p "beta") results)
          (push (process-item p "gamma") results)
          (advice-remove 'process-item around-fn))
        (push (process-item p "delta") results)
        (setf (proc-label p) "worker-v2")
        (goto-char 13)
        (insert (format "ac=%d[%s]" around-count (mapconcat #'identity (reverse results) ",")))
        (setf (marker-position m) 18)
        (put-text-property 13 (+ 13 (length (format "ac=%d[%s]"
                                                      around-count (mapconcat #'identity (reverse results) ","))))
                          'around-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (calls (proc-calls my-proc))
              (label (proc-label my-proc)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs calls label
                (marker-position m)
                (buffer-string)
                my-proc)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_advice_filter_generic_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass validator ()
    ((rules :initarg :rules :accessor val-rules :initform nil)
     (passes :initarg :passes :accessor val-passes :initform 0)
     (failures :initarg :failures :accessor val-failures :initform 0)))
  (defgeneric validate (v data)
    (:documentation "Validate data."))
  (defmethod validate ((v validator) data)
    (incf (val-passes v))
    (format "valid:%s" data))
  (let* ((buf (generate-new-buffer "ad3"))
         (v (validator :rules '(positivep) :passes 0 :failures 0)))
    (with-current-buffer buf
      (insert "VALID:rules:pass=0:fail=0")
      (put-text-property 1 6 'field 'type)
      (put-text-property 7 12 'field 'rules)
      (put-text-property 13 19 'field 'pass)
      (put-text-property 20 26 'field 'fail)
      (setq-local my-validator v)
      (let* ((ov (make-overlay 13 26))
             (_ (overlay-put ov 'face 'italic))
             (m (make-marker))
             (_ (set-marker m 16))
             (filter-fn (lambda (v data)
                          (if (and (numberp data) (> data 0))
                              data
                            (progn (incf (val-failures v)) nil))))
             (results nil))
        (advice-add 'validate :filter-args filter-fn)
        (undo-boundary)
        (narrow-to-region 7 26)
        (push (validate v 5) results)
        (push (validate v -3) results)
        (push (validate v 10) results)
        (push (validate v 0) results)
        (push (validate v 42) results)
        (goto-char (point-min))
        (insert (format "p=%d:f=%d" (val-passes v) (val-failures v)))
        (setf (marker-position m) (+ (point-min) 5))
        (put-text-property (point-min) (+ (point-min) 10) 'val-result t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-substring (point-min) (point-max)))
              (passes (val-passes my-validator))
              (failures (val-failures my-validator)))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (advice-remove 'validate filter-fn)
          (list mp os oe bs passes failures (reverse results)
                (marker-position m)
                (buffer-string)
                my-validator)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_advice_override_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass logger ()
    ((entries :initarg :entries :accessor log-entries :initform nil)
     (level :initarg :level :accessor log-level :initform 'info)))
  (defgeneric log-msg (lg msg)
    (:documentation "Log a message."))
  (defmethod log-msg ((lg logger) msg)
    (push (format "[%s] %s" (log-level lg) msg) (log-entries lg))
    (format "logged[%d]" (length (log-entries lg))))
  (let* ((buf (generate-new-buffer "ad4"))
         (lg (logger :level 'debug)))
    (with-current-buffer buf
      (insert "LOG:debug:entries=0")
      (put-text-property 1 4 'field 'type)
      (put-text-property 5 10 'field 'level)
      (put-text-property 11 20 'field 'count)
      (setq-local my-logger lg)
      (let* ((ov (make-overlay 5 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 7))
             (override-fn (lambda (lg msg)
                            (if (eq (log-level lg) 'silent)
                                "suppressed"
                              (cl-letf (((log-level lg) 'override))
                                (format "override:%s" (cl-call-next-method lg (format "OVERRIDE:%s" msg))))))))
        (advice-add 'log-msg :around override-fn)
        (undo-boundary)
        (let ((r1 (log-msg lg "test1"))
              (r2 (log-msg lg "test2")))
          (setf (log-level lg) 'silent)
          (let ((r3 (log-msg lg "test3")))
            (setf (log-level lg) 'debug)
            (let ((r4 (log-msg lg "test4")))
              (goto-char 11)
              (insert (format "%s,%s,%s,%s" r1 r2 r3 r4))
              (setf (marker-position m) 15)
              (put-text-property 11 (+ 11 (length (format "%s,%s,%s,%s" r1 r2 r3 r4)))
                                'log-results t))))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (entries (log-entries my-logger))
              (level (log-level my-logger)))
          (primitive-undo 1 buffer-undo-list)
          (advice-remove 'log-msg override-fn)
          (list mp os oe bs entries level
                (marker-position m)
                (buffer-string)
                my-logger)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_advice_multi_generic_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass cache-entry ()
    ((key :initarg :key :accessor cache-key :initform "")
     (value :initarg :value :accessor cache-value :initform nil)
     (hits :initarg :hits :accessor cache-hits :initform 0)))
  (defgeneric cache-lookup (entry)
    (:documentation "Lookup cache entry."))
  (defmethod cache-lookup ((e cache-entry))
    (incf (cache-hits e))
    (cache-value e))
  (defgeneric cache-update (entry new-value)
    (:documentation "Update cache entry."))
  (defmethod cache-update ((e cache-entry) new-value)
    (setf (cache-value e) new-value)
    (format "updated[%s->%s]" (cache-key e) new-value))
  (let* ((buf (generate-new-buffer "ad5"))
         (e (cache-entry :key "user:1" :value '(name "alice") :hits 0))
         (lookup-log nil))
    (with-current-buffer buf
      (insert "CACHE:user:1:hits=0")
      (put-text-property 1 6 'field 'type)
      (put-text-property 7 14 'field 'key)
      (put-text-property 15 21 'field 'hits)
      (setq-local my-entry e)
      (let* ((ov (make-overlay 7 14))
             (_ (overlay-put ov 'face 'underline))
             (m (make-marker))
             (_ (set-marker m 10))
             (lookup-advice (lambda (entry)
                              (push (format "lookup:%s:hits=%d" (cache-key entry) (cache-hits entry)) lookup-log)
                              (cl-call-next-method entry))))
        (advice-add 'cache-lookup :around lookup-advice)
        (undo-boundary)
        (let ((v1 (cache-lookup e))
              (v2 (cache-lookup e)))
          (cache-update e '(name "bob"))
          (let ((v3 (cache-lookup e)))
            (goto-char 15)
            (insert (format "h=%d[%s]" (cache-hits e)
                           (mapconcat (lambda (v) (format "%s" v)) (list v1 v2 v3) ",")))
            (setf (marker-position m) 18)
            (put-text-property 15 (+ 15 (length (format "h=%d[%s]"
                                                          (cache-hits e)
                                                          (mapconcat (lambda (v) (format "%s" v))
                                                                     (list v1 v2 v3) ","))))
                              'cache-log t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (hits (cache-hits my-entry))
              (value (cache-value my-entry))
              (key (cache-key my-entry)))
          (primitive-undo 1 buffer-undo-list)
          (advice-remove 'cache-lookup lookup-advice)
          (list mp os oe bs hits value key (reverse lookup-log)
                (marker-position m)
                (buffer-string)
                my-entry)))
      (kill-buffer buf))))"#,
        expect,
    );
}

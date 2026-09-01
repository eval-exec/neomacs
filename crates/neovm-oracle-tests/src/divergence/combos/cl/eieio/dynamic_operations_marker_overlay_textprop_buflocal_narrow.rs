//! Combo: cl-eieio dynamic class operations + marker + overlay + textprop + buflocal + undo.
//! Tests runtime class introspection, slot manipulation via symbols, dynamic generic function dispatch.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_dynamic_slot_value_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 14 22)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dyn-slots ()
    ((alpha :initarg :alpha :accessor ds-alpha :initform 0)
     (beta :initarg :beta :accessor ds-beta :initform "")
     (gamma :initarg :gamma :accessor ds-gamma :initform nil)))
  (let* ((buf (generate-new-buffer "dc1"))
         (obj (dyn-slots :alpha 42 :beta "hello" :gamma '(1 2 3)))
         (slot-names '(alpha beta gamma)))
    (with-current-buffer buf
      (insert "DYN:42:hello:(1,2,3)")
      (put-text-property 1 4 'field 'header)
      (put-text-property 5 7 'field 'alpha)
      (put-text-property 8 13 'field 'beta)
      (put-text-property 14 22 'field 'gamma)
      (setq-local my-obj obj)
      (let* ((ov (make-overlay 5 22))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 7))
             (values-before (mapcar (lambda (s) (slot-value obj s)) slot-names)))
        (undo-boundary)
        (dolist (s slot-names)
          (let ((v (slot-value obj s)))
            (cond ((numberp v) (setf (slot-value obj s) (* v 10)))
                  ((stringp v) (setf (slot-value obj s) (concat v "-mod")))
                  ((consp v) (setf (slot-value obj s) (append v '(4 5)))))))
        (let ((values-after (mapcar (lambda (s) (slot-value obj s)) slot-names)))
          (goto-char 5)
          (insert (format "%s->%s" values-before values-after))
          (setf (marker-position m) 10)
          (put-text-property 5 (+ 5 (length (format "%s->%s" values-before values-after)))
                            'dyn-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (a (ds-alpha my-obj))
              (b (ds-beta my-obj))
              (g (ds-gamma my-obj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs a b g
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_class_slots_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 18 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass introspected ()
    ((x :initarg :x :accessor it-x :initform 0 :type number :documentation "X coordinate")
     (y :initarg :y :accessor it-y :initform 0 :type number :documentation "Y coordinate")
     (label :initarg :label :accessor it-label :initform "" :type string)))
  (let* ((buf (generate-new-buffer "dc2"))
         (obj (introspected :x 10 :y 20 :label "origin"))
         (class (eieio-object-class obj)))
    (with-current-buffer buf
      (insert "INTRO:x=10:y=20:label=origin")
      (put-text-property 1 7 'field 'header)
      (put-text-property 8 12 'field 'x)
      (put-text-property 13 17 'field 'y)
      (put-text-property 18 30 'field 'label)
      (setq-local my-obj obj)
      (let* ((ov (make-overlay 8 17))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 10))
             (slot-names (mapcar (lambda (s) (cl--slot-descriptor-name s))
                                 (eieio-class-slots class)))
             (slot-count (length slot-names))
             (cpl (eieio-class-precedence-list class))
             (cpl-names (mapcar (lambda (c) (eieio-class-name c)) cpl)))
        (undo-boundary)
        (setf (it-x obj) 100
              (it-y obj) 200
              (it-label obj) "updated")
        (let ((slot-values (mapcar (lambda (s) (slot-value obj s)) slot-names))
              (child-p (child-of-class-p class 'eieio-default-superclass))
              (ancestor-p (class-ancestor-p 'eieio-default-superclass class)))
          (goto-char 8)
          (insert (format "slots=%s:cpl=%s:child=%s"
                         slot-values cpl-names child-p))
          (setf (marker-position m) 15)
          (put-text-property 8 (+ 8 (length (format "slots=%s:cpl=%s:child=%s"
                                                      slot-values cpl-names child-p)))
                            'intro-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (x (it-x my-obj))
              (y (it-y my-obj))
              (label (it-label my-obj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs x y label slot-names slot-count
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_apply_funcall_generic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass callable-entity ()
    ((name :initarg :name :accessor ce-name :initform "")
     (invocations :initarg :invocations :accessor ce-invocations :initform 0)))
  (defgeneric ce-process (entity &rest args)
    (:documentation "Process with entity."))
  (defmethod ce-process ((entity callable-entity) &rest args)
    (incf (ce-invocations entity))
    (format "process[%s:#%d:args=%s]" (ce-name entity) (ce-invocations entity) args))
  (let* ((buf (generate-new-buffer "dc3"))
         (e (callable-entity :name "handler")))
    (with-current-buffer buf
      (insert "CALL:handler:inv=0")
      (put-text-property 1 5 'field 'type)
      (put-text-property 6 13 'field 'name)
      (put-text-property 14 19 'field 'inv)
      (setq-local entity e)
      (let* ((ov (make-overlay 6 13))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 9))
             (r1 (funcall 'ce-process e "arg1"))
             (r2 (apply 'ce-process e '("arg2" "arg3")))
             (r3 (funcall (symbol-function 'ce-process) e "arg4")))
        (undo-boundary)
        (setf (ce-name e) "handler-v2")
        (let ((r4 (apply 'ce-process e (list "batch" "mode")))
              (r5 (funcall 'ce-process e "final")))
          (goto-char 14)
          (insert (format "inv=%d[%s,%s,%s,%s,%s]"
                         (ce-invocations e) r1 r2 r3 r4 r5))
          (setf (marker-position m) 18)
          (put-text-property 14 (+ 14 (length (format "inv=%d[%s,%s,%s,%s,%s]"
                                                        (ce-invocations e) r1 r2 r3 r4 r5)))
                            'call-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (name (ce-name entity))
              (inv (ce-invocations entity)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs name inv
                (marker-position m)
                (buffer-string)
                entity)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_setf_slot_via_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 22 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass mutable-entity ()
    ((a :initarg :a :accessor me-a :initform "a")
     (b :initarg :b :accessor me-b :initform "b")
     (c :initarg :c :accessor me-c :initform "c")))
  (let* ((buf (generate-new-buffer "dc4"))
         (obj (mutable-entity :a "alpha" :b "beta" :c "gamma")))
    (with-current-buffer buf
      (insert "MUT:a=alpha:b=beta:c=gamma")
      (put-text-property 1 4 'field 'header)
      (put-text-property 5 13 'field 'a)
      (put-text-property 14 21 'field 'b)
      (put-text-property 22 30 'field 'c)
      (setq-local my-obj obj)
      (let* ((ov (make-overlay 5 21))
             (_ (overlay-put ov 'priority 3))
             (m (make-marker))
             (_ (set-marker m 7))
             (slots '(a b c))
             (before (mapcar (lambda (s) (cons s (slot-value obj s))) slots)))
        (undo-boundary)
        (dolist (s slots)
          (setf (slot-value obj s) (upcase (slot-value obj s))))
        (let ((after (mapcar (lambda (s) (cons s (slot-value obj s))) slots)))
          (goto-char 5)
          (insert (format "%s->%s" before after))
          (setf (marker-position m) 10)
          (put-text-property 5 (+ 5 (length (format "%s->%s" before after)))
                            'mut-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (a (me-a my-obj))
              (b (me-b my-obj))
              (c (me-c my-obj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs a b c
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_dynamic_dispatch_apply_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass dispatched ()
    ((kind :initarg :kind :accessor disp-kind :initform "")
     (payload :initarg :payload :accessor disp-payload :initform nil)))
  (defgeneric dispatch (obj action)
    (:documentation "Dispatch an action."))
  (defmethod dispatch ((obj dispatched) action)
    (pcase action
      ('increment (setf (disp-payload obj) (1+ (or (disp-payload obj) 0))))
      ('double (setf (disp-payload obj) (* 2 (or (disp-payload obj) 0))))
      ('reset (setf (disp-payload obj) 0))
      (_ (format "unknown:%s" action)))
    (format "%s:%s=%s" (disp-kind obj) action (disp-payload obj)))
  (let* ((buf (generate-new-buffer "dc5"))
         (obj (dispatched :kind "counter" :payload 0))
         (actions '(increment increment double increment reset increment double double))
         (results nil))
    (with-current-buffer buf
      (insert "DISPATCH:counter:p=0")
      (put-text-property 1 9 'field 'header)
      (put-text-property 10 17 'field 'kind)
      (put-text-property 18 20 'field 'payload)
      (setq-local my-obj obj)
      (let* ((ov (make-overlay 10 17))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 13)))
        (undo-boundary)
        (dolist (action actions)
          (push (dispatch obj action) results))
        (setf (disp-kind obj) "counter-v2")
        (goto-char 18)
        (insert (format "p=%d[%s]"
                       (disp-payload obj)
                       (mapconcat #'identity (reverse results) ",")))
        (setf (marker-position m) 20)
        (put-text-property 18 (+ 18 (length (format "p=%d[%s]"
                                                      (disp-payload obj)
                                                      (mapconcat #'identity (reverse results) ","))))
                          'dispatch-result t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (kind (disp-kind my-obj))
              (payload (disp-payload my-obj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs kind payload
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

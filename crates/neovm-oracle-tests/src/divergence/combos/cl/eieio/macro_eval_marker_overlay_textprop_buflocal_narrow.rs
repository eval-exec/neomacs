//! Combo: cl-eieio macro/eval interaction + marker + overlay + textprop + buflocal + undo.
//! Tests eval of defclass/defmethod forms, macro expansion generating EIEIO code, with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_eval_defclass_defmethod() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (eval '(defclass eval-class ()
           ((name :initarg :name :accessor ec-name :initform "")
            (count :initarg :count :accessor ec-count :initform 0)))
         t)
  (eval '(defmethod ec-increment ((obj eval-class))
           (incf (ec-count obj))
           (ec-count obj))
         t)
  (let* ((buf (generate-new-buffer "ev1"))
         (obj (eval-class :name "dynamic" :count 0)))
    (with-current-buffer buf
      (insert "EVAL:dynamic:count=0")
      (put-text-property 1 5 'field 'type)
      (put-text-property 6 14 'field 'name)
      (put-text-property 15 22 'field 'count)
      (setq-local my-obj obj)
      (let* ((ov (make-overlay 6 14))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 10))
             (class-sym (intern-soft "eval-class"))
             (method-sym (intern-soft "ec-increment"))
             (class-fbound (and class-sym (fboundp class-sym)))
             (method-fbound (and method-sym (fboundp method-sym))))
        (undo-boundary)
        (let ((c1 (ec-increment obj))
              (c2 (ec-increment obj))
              (c3 (ec-increment obj)))
          (setf (ec-name obj) "updated")
          (goto-char 15)
          (insert (format "count=%d[%d,%d,%d]" (ec-count obj) c1 c2 c3))
          (setf (marker-position m) 18)
          (put-text-property 15 (+ 15 (length (format "count=%d[%d,%d,%d]"
                                                        (ec-count obj) c1 c2 c3)))
                            'eval-result t))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (name (ec-name my-obj))
              (count (ec-count my-obj))
              (cls (intern-soft "eval-class")))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs name count class-fbound method-fbound
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_defmacro_generates_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defmethod)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro define-counter (name &optional default)
    (let ((class-name (intern (format "cnt-%s" name)))
          (accessor-name (intern (format "cnt-%s-val" name))))
      `(progn
         (defclass ,class-name ()
           ((val :initarg :val :accessor ,accessor-name :initform ,(or default 0))))
         (defmethod increment-counter ((obj ,class-name))
           (,accessor-name obj (1+ (,accessor-name obj)))
           (,accessor-name obj))
         (list ',class-name ',accessor-name))))
  (let* ((buf (generate-new-buffer "ev2"))
         (r1 (define-counter hits 0))
         (r2 (define-counter misses 0)))
    (let* ((hits-class (car r1))
           (hits-acc (cadr r1))
           (misses-class (car r2))
           (h1 (funcall (or (get hits-class 'eieio-constructor) hits-class) :val 5))
           (m1 (funcall (or (get misses-class 'eieio-constructor) misses-class) :val 2)))
      (with-current-buffer buf
        (insert "MACRO:hits=5:misses=2")
        (put-text-property 1 6 'field 'header)
        (put-text-property 7 11 'field 'hits)
        (put-text-property 12 16 'field 'hval)
        (put-text-property 17 24 'field 'misses)
        (put-text-property 25 26 'field 'mval)
        (setq-local hits-obj h1)
        (setq-local misses-obj m1)
        (let* ((ov (make-overlay 12 26))
               (_ (overlay-put ov 'face 'bold))
               (m (make-marker))
               (_ (set-marker m 14))
               (h-accessor (intern-soft "cnt-hits-val"))
               (m-accessor (intern-soft "cnt-misses-val"))
               (h-bound (and h-accessor (fboundp h-accessor)))
               (m-bound (and m-accessor (fboundp m-accessor))))
          (undo-boundary)
          (increment-counter h1)
          (increment-counter h1)
          (increment-counter m1)
          (let ((hv (cnt-hits-val h1))
                (mv (cnt-misses-val m1)))
            (goto-char 12)
            (insert (format "%d:misses=%d" hv mv))
            (setf (marker-position m) 16)
            (put-text-property 12 (+ 12 (length (format "%d:misses=%d" hv mv)))
                              'macro-result t))
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (bs (buffer-string))
                (hv (cnt-hits-val hits-obj))
                (mv (cnt-misses-val misses-obj)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe bs hv mv h-bound m-bound
                  (marker-position m)
                  (buffer-string)
                  hits-obj misses-obj)))
        (kill-buffer buf))))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_eval_string_with_instances() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable obj)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass runtime-checked ()
    ((field-a :initarg :field-a :accessor rc-field-a :initform "default")
     (field-b :initarg :field-b :accessor rc-field-b :initform 0)))
  (let* ((buf (generate-new-buffer "ev3"))
         (obj (runtime-checked :field-a "init" :field-b 1)))
    (with-current-buffer buf
      (insert "RUNTIME:init:1")
      (put-text-property 1 8 'field 'header)
      (put-text-property 9 13 'field 'a)
      (put-text-property 14 15 'field 'b)
      (setq-local my-obj obj)
      (let* ((ov (make-overlay 9 13))
             (_ (overlay-put ov 'face 'underline))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (let ((r1 (eval '(rc-field-a obj) t))
              (r2 (eval '(rc-field-b obj) t)))
          (eval '(setf (rc-field-a obj) "evaluated") t)
          (eval '(setf (rc-field-b obj) 99) t)
          (let ((r3 (eval '(rc-field-a obj) t))
                (r4 (eval '(rc-field-b obj) t))
                (r5 (eval '(type-of obj) t)))
            (goto-char 9)
            (insert (format "%s->%s|%d->%d[%s]"
                           r1 r3 r2 r4 r5))
            (setf (marker-position m) 14)
            (put-text-property 9 (+ 9 (length (format "%s->%s|%d->%d[%s]"
                                                        r1 r3 r2 r4 r5)))
                              'eval-result t)))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (a (rc-field-a my-obj))
              (b (rc-field-b my-obj)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs a b
                (marker-position m)
                (buffer-string)
                my-obj)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_macrolet_with_generics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass measurable ()
    ((value :initarg :value :accessor ms-value :initform 0)
     (unit :initarg :unit :accessor ms-unit :initform "")))
  (defgeneric measure (obj)
    (:documentation "Get measurement."))
  (defmethod measure ((obj measurable))
    (format "%d%s" (ms-value obj) (ms-unit obj)))
  (let* ((buf (generate-new-buffer "ev4"))
         (m1 (measurable :value 100 :unit "kg"))
         (m2 (measurable :value 200 :unit "m")))
    (with-current-buffer buf
      (insert "MEAS:100kg:200m")
      (put-text-property 1 5 'field 'header)
      (put-text-property 6 11 'field 'm1)
      (put-text-property 12 16 'field 'm2)
      (setq-local meas (list m1 m2))
      (let* ((ov (make-overlay 6 11))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (cl-macrolet ((double-measure (obj)
                        `(setf (ms-value ,obj) (* (ms-value ,obj) 2))))
          (let ((before1 (measure m1))
                (before2 (measure m2)))
            (double-measure m1)
            (double-measure m2)
            (let ((after1 (measure m1))
                  (after2 (measure m2)))
              (goto-char 6)
              (insert (format "%s->%s|%s->%s" before1 after1 before2 after2))
              (setf (marker-position m) 12)
              (put-text-property 6 (+ 6 (length (format "%s->%s|%s->%s"
                                                          before1 after1 before2 after2)))
                                'meas-result t))))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (v1 (ms-value (car meas)))
              (v2 (ms-value (cadr meas))))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs v1 v2
                (marker-position m)
                (buffer-string)
                meas)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_symbol_macrolet_slot_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 27 35)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass config-store ()
    ((host :initarg :host :accessor cs-host :initform "localhost")
     (port :initarg :port :accessor cs-port :initform 8080)
     (debug :initarg :debug :accessor cs-debug :initform nil)))
  (let* ((buf (generate-new-buffer "ev5"))
         (cfg (config-store :host "prod.example.com" :port 443 :debug t)))
    (with-current-buffer buf
      (insert "CFG:prod.example.com:443:debug=t")
      (put-text-property 1 4 'field 'header)
      (put-text-property 5 22 'field 'host)
      (put-text-property 23 26 'field 'port)
      (put-text-property 27 35 'field 'debug)
      (setq-local my-cfg cfg)
      (let* ((ov (make-overlay 5 26))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 10)))
        (undo-boundary)
        (cl-symbol-macrolet
            ((current-host (cs-host cfg))
             (current-port (cs-port cfg))
             (current-debug (cs-debug cfg)))
          (let ((h1 current-host)
                (p1 current-port)
                (d1 current-debug))
            (setq current-host "staging.example.com")
            (setq current-port 8443)
            (setq current-debug nil)
            (let ((h2 current-host)
                  (p2 current-port)
                  (d2 current-debug))
              (goto-char 5)
              (insert (format "%s:%d:%s->%s:%d:%s"
                             h1 p1 d1 h2 p2 d2))
              (setf (marker-position m) 14)
              (put-text-property 5 (+ 5 (length (format "%s:%d:%s->%s:%d:%s"
                                                          h1 p1 d1 h2 p2 d2)))
                                'symacro-result t))))
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (host (cs-host my-cfg))
              (port (cs-port my-cfg))
              (debug (cs-debug my-cfg)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs host port debug
                (marker-position m)
                (buffer-string)
                my-cfg)))
      (kill-buffer buf))))"#,
        expect,
    );
}

//! Combo: cl-eieio error recovery + condition-case + marker + overlay + textprop + buflocal + undo.
//! Tests EIEIO error conditions (unbound slots, missing methods, invalid initargs) with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_eieio_slot_unbound_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function slot-makunbound)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass guarded-resource ()
    ((name :initarg :name :accessor gr-name :initform "default")
     (secret :initarg :secret :accessor gr-secret)
     (state :initarg :state :accessor gr-state :initform 'idle)))
  (let* ((buf (generate-new-buffer "er1"))
         (r1 (guarded-resource :name "alpha" :secret "s1"))
         (r2 (guarded-resource :name "beta")))
    (with-current-buffer buf
      (insert "RES:alpha:IDLE-RES:beta:IDLE")
      (put-text-property 1 4 'field 'type)
      (put-text-property 5 10 'field 'r1name)
      (put-text-property 11 15 'field 'r1state)
      (put-text-property 16 19 'field 'type)
      (put-text-property 20 24 'field 'r2name)
      (put-text-property 25 29 'field 'r2state)
      (setq-local resources (list r1 r2))
      (let* ((ov (make-overlay 5 15))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8))
             (results nil)
             (errors nil))
        (undo-boundary)
        (condition-case err
            (push (format "r1-secret=%s" (gr-secret r1)) results)
          (error (push (format "r1-err:%s" (car (cdr err))) errors)))
        (condition-case err
            (push (format "r2-secret=%s" (gr-secret r2)) results)
          (void-variable (push "r2-void-slot" errors))
          (error (push (format "r2-err:%s" (car (cdr err))) errors)))
        (setf (gr-state r1) 'active)
        (setf (gr-state r2) 'active)
        (slot-makunbound r1 'secret)
        (condition-case err
            (push (format "r1-secret-after=%s" (gr-secret r1)) results)
          (error (push (format "r1-after:%s" (car (cdr err))) errors)))
        (goto-char 11)
        (insert (format "%s" (mapconcat #'identity (reverse results) "|")))
        (setf (marker-position m) 15)
        (put-text-property 11 (+ 11 (length (format "%s" (mapconcat #'identity (reverse results) "|"))))
                          'results t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (r1-state (gr-state r1))
              (r2-state (gr-state r2))
              (r1-name (gr-name r1))
              (r2-name (gr-name r2)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs r1-state r2-state r1-name r2-name
                (reverse results) (reverse errors)
                (marker-position m)
                (buffer-string)
                resources)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_no_method_error_with_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass data-source ()
    ((label :initarg :label :accessor src-label :initform "")))
  (defclass file-source (data-source)
    ((path :initarg :path :accessor src-path :initform "/tmp")))
  (defclass net-source (data-source)
    ((url :initarg :url :accessor src-url :initform "http://localhost")))
  (defgeneric fetch-data (src)
    (:documentation "Fetch data from source."))
  (defmethod fetch-data ((src file-source))
    (format "file:%s:%s" (src-label src) (src-path src)))
  (let* ((buf (generate-new-buffer "er2"))
         (fs (file-source :label "config" :path "/etc/config"))
         (ns (net-source :label "api" :url "http://api.example.com")))
    (with-current-buffer buf
      (insert "FILE:config:/etc-NET:api:http://api")
      (put-text-property 1 5 'src 'file)
      (put-text-property 6 12 'field 'label)
      (put-text-property 13 17 'field 'path)
      (put-text-property 18 21 'src 'net)
      (put-text-property 22 25 'field 'nlabel)
      (put-text-property 26 39 'field 'url)
      (setq-local sources (list fs ns))
      (let* ((ov (make-overlay 1 17))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 7))
             (results nil)
             (errors nil))
        (undo-boundary)
        (condition-case err
            (push (fetch-data fs) results)
          (error (push (format "file-err:%s" err) errors)))
        (condition-case err
            (push (fetch-data ns) results)
          (cl-no-applicable-method
           (push (format "no-method-for:%s" (eieio-class-name (eieio-object-class ns))) errors))
          (error (push (format "net-err:%s" err) errors)))
        (setf (src-path fs) "/var/data")
        (condition-case err
            (push (fetch-data fs) results)
          (error (push (format "file2-err:%s" err) errors)))
        (goto-char 13)
        (insert (format "[%s]" (mapconcat #'identity (reverse results) ",")))
        (setf (marker-position m) 20)
        (put-text-property 13 (+ 13 (length (format "[%s]" (mapconcat #'identity (reverse results) ","))))
                          'fetched t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (fs-path (src-path fs))
              (ns-url (src-url ns)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs fs-path ns-url
                (reverse results) (reverse errors)
                (marker-position m)
                (buffer-string)
                sources)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_invalid_initarg_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass strict-config ()
    ((host :initarg :host :accessor cfg-host :initform "localhost")
     (port :initarg :port :accessor cfg-port :initform 8080)))
  (let* ((buf (generate-new-buffer "er3"))
         (c1 (strict-config :host "db1" :port 3306))
         (results nil)
         (errors nil))
    (with-current-buffer buf
      (insert "CFG:db1:3306-CFG2:pending")
      (put-text-property 1 4 'field 'type)
      (put-text-property 5 8 'field 'host)
      (put-text-property 9 13 'field 'port)
      (put-text-property 14 18 'field 'type2)
      (put-text-property 19 26 'field 'pending)
      (setq-local config c1)
      (let* ((ov (make-overlay 5 13))
             (_ (overlay-put ov 'face 'underline))
             (m (make-marker))
             (_ (set-marker m 7)))
        (undo-boundary)
        (condition-case err
            (let ((c2 (strict-config :host "db2" :port 5432 :invalid-arg "oops")))
              (push (format "c2-ok:%s:%d" (cfg-host c2) (cfg-port c2)) results))
          (error (push (format "c2-err:%s" (car (cdr err))) errors)))
        (condition-case err
            (let ((c3 (strict-config :host "db3" :port 6379)))
              (push (format "c3-ok:%s:%d" (cfg-host c3) (cfg-port c3)) results))
          (error (push (format "c3-err:%s" (car (cdr err))) errors)))
        (setf (cfg-port c1) 3307)
        (goto-char 9)
        (insert (format "%s" (mapconcat #'identity (reverse results) "|")))
        (setf (marker-position m) 12)
        (put-text-property 9 (+ 9 (length (format "%s" (mapconcat #'identity (reverse results) "|"))))
                          'init-results t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (c1-port (cfg-port config))
              (c1-host (cfg-host config)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs c1-port c1-host
                (reverse results) (reverse errors)
                (marker-position m)
                (buffer-string)
                config)))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn combo_eieio_slot_missing_error_multi_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass base-component ()
    ((id :initarg :id :accessor comp-id :initform 0)
     (active :initarg :active :accessor comp-active :initform t)))
  (defclass render-component (base-component)
    ((color :initarg :color :accessor comp-color :initform "white")))
  (let* ((buf1 (generate-new-buffer "er4a"))
         (buf2 (generate-new-buffer "er4b"))
         (c1 (render-component :id 1 :color "red"))
         (c2 (render-component :id 2 :color "blue"))
         (results nil)
         (errors nil))
    (with-current-buffer buf1
      (insert "COMP1:1:red:active")
      (put-text-property 1 6 'comp 1)
      (put-text-property 7 8 'comp-id 1)
      (put-text-property 9 12 'comp-color "red")
      (put-text-property 13 19 'comp-state 'active)
      (setq-local comp c1))
    (with-current-buffer buf2
      (insert "COMP2:2:blue:active")
      (put-text-property 1 6 'comp 2)
      (put-text-property 7 8 'comp-id 2)
      (put-text-property 9 13 'comp-color "blue")
      (put-text-property 14 20 'comp-state 'active)
      (setq-local comp c2))
    (let* ((ov1 (with-current-buffer buf1 (let ((ov (make-overlay 9 12)))
                                            (overlay-put ov 'priority 1) ov)))
           (ov2 (with-current-buffer buf2 (let ((ov (make-overlay 9 13)))
                                            (overlay-put ov 'priority 2) ov)))
           (m1 (with-current-buffer buf1 (let ((m (make-marker))) (set-marker m 10) m)))
           (m2 (with-current-buffer buf2 (let ((m (make-marker))) (set-marker m 10) m))))
      (with-current-buffer buf1
        (condition-case err
            (progn
              (slot-value c1 'nonexistent)
              (push "c1-no-err" results))
          (error (push (format "c1-missing:%s" (car (cdr err))) errors))))
      (with-current-buffer buf2
        (condition-case err
            (progn
              (setf (comp-color c2) "green")
              (push (format "c2-color:%s" (comp-color c2)) results))
          (error (push (format "c2-err:%s" err) errors))))
      (with-current-buffer buf1
        (condition-case err
            (progn
              (setf (comp-active c1) nil)
              (push (format "c1-active:%s" (comp-active c1)) results))
          (error (push (format "c1-active-err:%s" err) errors))))
      (with-current-buffer buf1
        (goto-char 9)
        (insert (format "[%s]" (mapconcat #'identity (reverse results) ",")))
        (setf (marker-position m1) 15))
      (with-current-buffer buf2
        (goto-char 9)
        (insert (format "[%s]" (mapconcat #'identity (reverse errors) ",")))
        (setf (marker-position m2) 15))
      (let ((mp1 (marker-position m1))
            (mp2 (marker-position m2))
            (os1 (overlay-start ov1))
            (oe1 (overlay-end ov1))
            (os2 (overlay-start ov2))
            (oe2 (overlay-end ov2))
            (bs1 (with-current-buffer buf1 (buffer-string)))
            (bs2 (with-current-buffer buf2 (buffer-string)))
            (c1-active (with-current-buffer buf1 (comp-active comp)))
            (c2-color (with-current-buffer buf2 (comp-color comp))))
        (list mp1 mp2 os1 oe1 os2 oe2 bs1 bs2 c1-active c2-color
              (reverse results) (reverse errors))))
    (kill-buffer buf1)
    (kill-buffer buf2)))"#,
        expect,
    );
}

#[test]
fn combo_eieio_nested_condition_case_generics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defgeneric)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass pipeline-stage ()
    ((name :initarg :name :accessor stage-name :initform "")
     (enabled :initarg :enabled :accessor stage-enabled :initform t)))
  (defclass filter-stage (pipeline-stage)
    ((predicate :initarg :predicate :accessor stage-predicate :initform nil)))
  (defclass transform-stage (pipeline-stage)
    ((fn :initarg :fn :accessor stage-fn :initform nil)))
  (defgeneric run-stage (stage input)
    (:documentation "Run a pipeline stage."))
  (defmethod run-stage ((stage filter-stage) input)
    (if (and (stage-predicate stage) (funcall (stage-predicate stage) input))
        input
      (signal 'filter-rejected (list (stage-name stage) input))))
  (defmethod run-stage ((stage transform-stage) input)
    (if (stage-fn stage)
        (funcall (stage-fn stage) input)
      (signal 'transform-failed (list (stage-name stage)))))
  (defmethod run-stage ((stage pipeline-stage) input)
    input)
  (let* ((buf (generate-new-buffer "er5"))
         (f1 (filter-stage :name "even?" :predicate (lambda (x) (= (% x 2) 0))))
         (t1 (transform-stage :name "double" :fn (lambda (x) (* x 2))))
         (f2 (filter-stage :name "positive?" :predicate (lambda (x) (> x 0))))
         (t2 (transform-stage :name "no-fn" :fn nil))
         (pipeline (list f1 t1 f2))
         (results nil)
         (errors nil))
    (with-current-buffer buf
      (insert "PIPE:even->double->positive")
      (put-text-property 1 5 'stage 'pipe)
      (put-text-property 6 10 'stage 'filter)
      (put-text-property 11 17 'stage 'transform)
      (put-text-property 18 27 'stage 'filter2)
      (setq-local stages pipeline)
      (let* ((ov (make-overlay 6 17))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (dolist (input '(4 3 6 -2 8))
          (condition-case err
              (let ((val input))
                (dolist (stage pipeline)
                  (setq val (run-stage stage val)))
                (push (format "%d->%s" input val) results))
            (filter-rejected
             (push (format "%d-rejected-by-%s" input (cadr err)) errors))
            (transform-failed
             (push (format "%d-transform-fail:%s" input (cadr err)) errors))
            (error
             (push (format "%d-err:%s" input (car (cdr err))) errors))))
        (setf (stage-enabled f1) nil)
        (setf (stage-enabled t1) t)
        (goto-char (point-max))
        (insert (format " | %s" (mapconcat #'identity (reverse results) ",")))
        (setf (marker-position m) 15)
        (put-text-property 1 (+ 1 (length "PIPE:even->double->positive | ")) 'pipeline-log t)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (bs (buffer-string))
              (f1-en (stage-enabled f1))
              (t1-name (stage-name t1)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe bs f1-en t1-name
                (reverse results) (reverse errors)
                (marker-position m)
                (buffer-string)
                stages)))
      (kill-buffer buf))))"#,
        expect,
    );
}

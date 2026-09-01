//! Divergence tests: EIEIO + error recovery + dynamic binding deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eieio_method_error_after_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (6 ((before 3 4) completed (after 4) (before 4 5) completed (after 5) (before 5 6)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-eieio-log-xxx nil)
  (defclass test-state-xxx () ((val :initarg :val :initform 0)))
  (cl-defmethod test-inc-and-check-xxx ((obj test-state-xxx))
    (let ((old (slot-value obj 'val)))
      (oset obj val (1+ old))
      (push (list 'before old (slot-value obj 'val)) test-eieio-log-xxx)
      (when (> (slot-value obj 'val) 5)
        (error "too high: %d" (slot-value obj 'val)))
      (push 'completed test-eieio-log-xxx)))
  (cl-defmethod test-inc-and-check-xxx :after ((obj test-state-xxx))
    (push (list 'after (slot-value obj 'val)) test-eieio-log-xxx))
  (let ((o (test-state-xxx "o" :val 3)))
    (ignore-errors (test-inc-and-check-xxx o))
    (ignore-errors (test-inc-and-check-xxx o))
    (ignore-errors (test-inc-and-check-xxx o))
    (list (slot-value o 'val)
          (nreverse test-eieio-log-xxx)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_initform_dynamic_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 test-eieio-dyn-xxx 105 test-eieio-dyn-xxx)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-eieio-dyn-xxx 10)
  (defclass test-dyn-init-xxx ()
    ((derived :initform (+ test-eieio-dyn-xxx 5))
     (captured :initform test-eieio-dyn-xxx)))
  (let ((o1 (test-dyn-init-xxx "o1")))
    (let ((test-eieio-dyn-xxx 100))
      (let ((o2 (test-dyn-init-xxx "o2")))
        (list (slot-value o1 'derived) (slot-value o1 'captured)
              (slot-value o2 'derived) (slot-value o2 'captured)))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_labels_recursive_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((visited nil))
  (cl-labels ((visit (n depth)
                 (push (list n depth) visited)
                 (condition-case err
                     (when (< depth 3)
                       (visit (* n 2) (1+ depth))
                       (visit (1+ (* n 2)) (1+ depth)))
                   (error (push (list 'caught n) visited)))))
    (visit 1 0))
  (nreverse visited)) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_defclass_with_validation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (50 0 100 test-valid-xxx)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-valid-xxx ()
    ((value :initarg :value :initform 0
            :type (integer 0 100)
            :custom (integer 0 100)
            :documentation "A value 0-100."))
    "A validated class.")
  (let ((o1 (test-valid-xxx "o1" :value 50))
        (o2 (test-valid-xxx "o2" :value 0))
        (o3 (test-valid-xxx "o3" :value 100)))
    (list (slot-value o1 'value)
          (slot-value o2 'value)
          (slot-value o3 'value)
          (eieio-object-class-name o1)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_clone_after_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (99 10 20 20 nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-pair-xxx ()
    ((a :initarg :a :accessor test-pair-a-xxx)
     (b :initarg :b :accessor test-pair-b-xxx)))
  (let* ((orig (test-pair-xxx "o" :a 10 :b 20))
         (cloned (clone orig)))
    (setf (test-pair-a-xxx orig) 99)
    (list (test-pair-a-xxx orig) (test-pair-a-xxx cloned)
          (test-pair-b-xxx orig) (test-pair-b-xxx cloned)
          (eq orig cloned)
          (equal (eieio-object-class-name orig)
                 (eieio-object-class-name cloned))))) "#,
        expect,
    );
}

#[test]
fn divergence_advice_on_generic_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (150 50 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-adv-eieio-xxx () ((v :initarg :v :initform 1)))
  (cl-defgeneric test-compute-xxx (obj) "Compute.")
  (cl-defmethod test-compute-xxx ((obj test-adv-eieio-xxx))
    (* (slot-value obj 'v) 10))
  (advice-add 'test-compute-xxx :filter-return
               (lambda (r) (+ r 100)))
  (let ((o (test-adv-eieio-xxx "o" :v 5)))
    (let ((result (test-compute-xxx o)))
      (advice-remove 'test-compute-xxx
                      (lambda (r) (+ r 100)))
      (list result
            (test-compute-xxx o)
            (= result 150)
            (= (test-compute-xxx o) 50))))) "#,
        expect,
    );
}

#[test]
fn divergence_unwind_protect_in_constructor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (error \"test error in ctor\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-ctor-log-xxx nil)
  (defclass test-ctor-xxx ()
    ((val :initform (progn
                      (push 'initform test-ctor-log-xxx)
                      42))))
  (unwind-protect
      (let ((o (test-ctor-xxx "o")))
        (push (list 'created (slot-value o 'val)) test-ctor-log-xxx)
        (error "test error in ctor"))
    (push 'cleanup test-ctor-log-xxx))
  (nreverse test-ctor-log-xxx)) "#,
        expect,
    );
}

#[test]
fn divergence_dynamic_var_in_method_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((a default) (b default) ((a overridden) (b overridden)))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-dispatch-ctx-xxx 'default)
  (defclass test-ctx-xxx () ())
  (defclass test-ctx-a-xxx (test-ctx-xxx) ())
  (defclass test-ctx-b-xxx (test-ctx-xxx) ())
  (cl-defgeneric test-handle-xxx (obj) "Handle.")
  (cl-defmethod test-handle-xxx ((obj test-ctx-a-xxx))
    (list 'a test-dispatch-ctx-xxx))
  (cl-defmethod test-handle-xxx ((obj test-ctx-b-xxx))
    (list 'b test-dispatch-ctx-xxx))
  (let ((a (test-ctx-a-xxx "a"))
        (b (test-ctx-b-xxx "b")))
    (list (test-handle-xxx a)
          (test-handle-xxx b)
          (let ((test-dispatch-ctx-xxx 'overridden))
            (list (test-handle-xxx a) (test-handle-xxx b)))))) "#,
        expect,
    );
}

#[test]
fn divergence_cl_defmethod_eql_specializer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((the-answer 42) (special :special) (default 99) (default anything))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defgeneric test-eql-dispatch-xxx (x) "EQL dispatch.")
  (cl-defmethod test-eql-dispatch-xxx ((x (eql 42)))
    (list 'the-answer x))
  (cl-defmethod test-eql-dispatch-xxx ((x (eql :special)))
    (list 'special x))
  (cl-defmethod test-eql-dispatch-xxx (x)
    (list 'default x))
  (list (test-eql-dispatch-xxx 42)
        (test-eql-dispatch-xxx :special)
        (test-eql-dispatch-xxx 99)
        (test-eql-dispatch-xxx 'anything))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_object_with_overridden_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (1 2 nil t \"#s(test-eq-obj-xxx 1 (x y z))\" \"#s(test-eq-obj-xxx 2 (x y z))\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-eq-obj-xxx ()
    ((id :initarg :id)
     (data :initarg :data :initform nil)))
  (cl-defmethod cl-print-object ((obj test-eq-obj-xxx) stream)
    (princ (format \"#<obj-%d>\" (slot-value obj 'id)) stream)
    obj)
  (let ((a (test-eq-obj-xxx \"a\" :id 1 :data '(x y z))))
    (let ((b (clone a)))
      (setf (slot-value b 'id) 2)
      (list (slot-value a 'id) (slot-value b 'id)
            (eq a b)
            (equal (slot-value a 'data) (slot-value b 'data))
            (format \"%s\" a)
            (format \"%s\" b))))) ",
        expect,
    );
}

//! Divergence tests: EIEIO multiple inheritance + method dispatch + advice combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eieio_multiple_inheritance_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (args-out-of-range #s(test-child-ab-xxx 5 7 100) 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-base-a-xxx () ((a :initarg :a :initform 1)))
  (defclass test-base-b-xxx () ((b :initarg :b :initform 2)))
  (defclass test-child-ab-xxx (test-base-a-xxx test-base-b-xxx)
    ((c :initarg :c :initform 3)))
  (cl-defgeneric test-compute-xxx (obj) "Compute.")
  (cl-defmethod test-compute-xxx ((obj test-base-a-xxx))
    (* (slot-value obj 'a) 10))
  (cl-defmethod test-compute-xxx ((obj test-base-b-xxx))
    (* (slot-value obj 'b) 20))
  (cl-defmethod test-compute-xxx ((obj test-child-ab-xxx))
    (+ (cl-call-next-method)
       (slot-value obj 'c)))
  (let ((a (test-base-a-xxx "a"))
        (b (test-base-b-xxx "b"))
        (ab (test-child-ab-xxx "ab" :a 5 :b 7 :c 100)))
    (list (test-compute-xxx a)
          (= (test-compute-xxx a) 10)
          (test-compute-xxx b)
          (= (test-compute-xxx b) 40)
          (test-compute-xxx ab)
          (>= (test-compute-xxx ab) 100)
          (child-of-class-p ab 'test-child-ab-xxx)
          (child-of-class-p ab 'test-base-a-xxx)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_before_after_around_methods() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (1 t (around-before before after around-after) nil nil nil (around-after))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-baa-log-xxx nil)
  (defclass test-baa-xxx () ((v :initarg :v :initform 0)))
  (cl-defmethod test-baa-incr-xxx ((obj test-baa-xxx))
    (oset obj v (1+ (slot-value obj 'v))))
  (cl-defmethod test-baa-incr-xxx :before ((obj test-baa-xxx))
    (push 'before test-baa-log-xxx))
  (cl-defmethod test-baa-incr-xxx :after ((obj test-baa-xxx))
    (push 'after test-baa-log-xxx))
  (cl-defmethod test-baa-incr-xxx :around ((obj test-baa-xxx))
    (push 'around-before test-baa-log-xxx)
    (cl-call-next-method)
    (push 'around-after test-baa-log-xxx))
  (let ((o (test-baa-xxx "o" :v 0)))
    (test-baa-incr-xxx o)
    (list (slot-value o 'v)
          (= (slot-value o 'v) 1)
          (nreverse test-baa-log-xxx)
          (member 'around-before (nreverse test-baa-log-xxx))
          (member 'before (nreverse test-baa-log-xxx))
          (member 'after (nreverse test-baa-log-xxx))
          (member 'around-after (nreverse test-baa-log-xxx))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_method_with_advice_and_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 t nil 26 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-mac-xxx () ((val :initarg :val :initform 0)))
  (cl-defmethod test-mac-add-xxx ((obj test-mac-xxx) n)
    (oset obj val (+ (slot-value obj 'val) n)))
  (let ((call-count 0))
    (advice-add 'test-mac-add-xxx :filter-args
                 (lambda (args)
                   (cl-incf call-count)
                   (list (car args) (* (cadr args) 2)))))
  (let ((o (test-mac-xxx "o" :val 10)))
    (test-mac-add-xxx o 5)
    (list (slot-value o 'val)
          (= (slot-value o 'val) 20)
          (advice-remove 'test-mac-add-xxx
                          (lambda (args)
                            (cl-incf call-count)
                            (list (car args) (* (cadr args) 2))))
          (test-mac-add-xxx o 3)
          (= (slot-value o 'val) 23)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_slot_accessors_with_setf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (99 t 99 t 99 t 100 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-sa-xxx ()
    ((x :initarg :x :accessor test-sa-x-xxx :initform 0)
     (y :initarg :y :accessor test-sa-y-xxx :initform 0)))
  (let ((o (test-sa-xxx "o" :x 10 :y 20)))
    (setf (test-sa-x-xxx o) 99)
    (setf (test-sa-y-xxx o) (test-sa-x-xxx o))
    (list (test-sa-x-xxx o)
          (= (test-sa-x-xxx o) 99)
          (test-sa-y-xxx o)
          (= (test-sa-y-xxx o) 99)
          (slot-value o 'x)
          (= (slot-value o 'x) 99)
          (setf (test-sa-x-xxx o) (+ (test-sa-y-xxx o) 1))
          (= (test-sa-x-xxx o) 100)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_deep_inheritance_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (d1 t d1 t d3 t d3 t t 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-d1-xxx () ((v :initarg :v :initform 1)))
  (defclass test-d2-xxx (test-d1-xxx) ())
  (defclass test-d3-xxx (test-d2-xxx) ())
  (defclass test-d4-xxx (test-d3-xxx) ())
  (cl-defmethod test-dval-xxx ((obj test-d1-xxx)) 'd1)
  (cl-defmethod test-dval-xxx ((obj test-d3-xxx)) 'd3)
  (let ((o1 (test-d1-xxx "o1"))
        (o2 (test-d2-xxx "o2"))
        (o3 (test-d3-xxx "o3"))
        (o4 (test-d4-xxx "o4")))
    (list (test-dval-xxx o1)
          (eq (test-dval-xxx o1) 'd1)
          (test-dval-xxx o2)
          (eq (test-dval-xxx o2) 'd1)
          (test-dval-xxx o3)
          (eq (test-dval-xxx o3) 'd3)
          (test-dval-xxx o4)
          (eq (test-dval-xxx o4) 'd3)
          (eieio-object-p o4)
          (slot-value o4 'v)
          (= (slot-value o4 'v) 1)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_static_methods() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"created\" t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-sm-xxx ()
    ((name :initarg :name :initform "unnamed")))
  (cl-defmethod test-sm-make-xxx ((class (subclass test-sm-xxx)) name)
    (test-sm-xxx "o" :name name))
  (let ((obj (test-sm-make-xxx 'test-sm-xxx "created")))
    (list (slot-value obj 'name)
          (string= (slot-value obj 'name) "created")
          (eieio-object-p obj)
          (object-of-class-p obj 'test-sm-xxx)
          (class-children 'test-sm-xxx)
          (listp (class-children 'test-sm-xxx))))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_initializers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 t 20 t 30 t (initialized))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-init-log-xxx nil)
  (defclass test-init-xxx ()
    ((a :initarg :a :initform 0)
     (b :initarg :b :initform 0)
     (computed :initform nil)))
  (cl-defmethod initialize-instance :after ((obj test-init-xxx) &rest _)
    (oset obj computed (+ (slot-value obj 'a) (slot-value obj 'b)))
    (push 'initialized test-init-log-xxx))
  (let ((o (test-init-xxx "o" :a 10 :b 20)))
    (list (slot-value o 'a)
          (= (slot-value o 'a) 10)
          (slot-value o 'b)
          (= (slot-value o 'b) 20)
          (slot-value o 'computed)
          (= (slot-value o 'computed) 30)
          (member 'initialized test-init-log-xxx)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_generic_with_multiple_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (30 t \"helloworld\" t \"10-hello\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-num-xxx () ((val :initarg :val)))
  (defclass test-str-xxx () ((val :initarg :val)))
  (cl-defgeneric test-merge-xxx (a b) "Merge.")
  (cl-defmethod test-merge-xxx ((a test-num-xxx) (b test-num-xxx))
    (+ (slot-value a 'val) (slot-value b 'val)))
  (cl-defmethod test-merge-xxx ((a test-str-xxx) (b test-str-xxx))
    (concat (slot-value a 'val) (slot-value b 'val)))
  (cl-defmethod test-merge-xxx ((a test-num-xxx) (b test-str-xxx))
    (format "%d-%s" (slot-value a 'val) (slot-value b 'val)))
  (let ((n1 (test-num-xxx "n1" :val 10))
        (n2 (test-num-xxx "n2" :val 20))
        (s1 (test-str-xxx "s1" :val "hello"))
        (s2 (test-str-xxx "s2" :val "world")))
    (list (test-merge-xxx n1 n2)
          (= (test-merge-xxx n1 n2) 30)
          (test-merge-xxx s1 s2)
          (string= (test-merge-xxx s1 s2) "helloworld")
          (test-merge-xxx n1 s1)
          (string= (test-merge-xxx n1 s1) "10-hello")))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_slot_boundp_oset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbol 'a slot)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-sbp-xxx ()
    ((a :initarg :a :initform nil)
     (b :initarg :b)))
  (let ((o1 (test-sbp-xxx "o1" :a 42))
        (o2 (test-sbp-xxx "o2" :b 99)))
    (list (slot-value o1 'a)
          (= (slot-value o1 'a) 42)
          (slot-boundp o1 'a)
          (slot-boundp o2 'b)
          (= (slot-value o2 'b) 99)
          (slot-boundp o2 'a)
          (oset o1 'a 100)
          (slot-value o1 'a)
          (= (slot-value o1 'a) 100)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_with_defstruct_interop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct test-dsi-xxx name value)
  (defclass test-ei-xxx ()
    ((data :initarg :data :initform nil)))
  (cl-defmethod test-ei-store-xxx ((obj test-ei-xxx) item)
    (push item (slot-value obj 'data)))
  (let ((eobj (test-ei-xxx "e"))
        (d1 (make-test-dsi-xxx :name "first" :value 10))
        (d2 (make-test-dsi-xxx :name "second" :value 20)))
    (test-ei-store-xxx eobj d1)
    (test-ei-store-xxx eobj d2)
    (let ((stored (nreverse (slot-value eobj 'data))))
      (list (length stored)
            (= (length stored) 2)
            (test-dsi-xxx-name (car stored))
            (string= (test-dsi-xxx-name (car stored)) "first")
            (test-dsi-xxx-value (cadr stored))
            (= (test-dsi-xxx-value (cadr stored)) 20)
            (eieio-object-p eobj)
            (null (eieio-object-p d1)))))) "#,
        expect,
    );
}

//! Divergence tests: defstruct + EIEIO interop + print + equal combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eieio_with_defstruct_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct test-ds-slot-xxx name value)
  (defclass test-eio-ds-xxx ()
    ((data :initarg :data :initform nil)
     (computed :initform nil)))
  (cl-defmethod test-eio-compute-xxx ((obj test-eio-ds-xxx))
    (let ((sum 0))
      (dolist (item (slot-value obj 'data))
        (cl-incf sum (test-ds-slot-xxx-value item)))
      (oset obj computed sum)))
  (let ((o (test-eio-ds-xxx "o")))
    (setf (slot-value o 'data)
          (list (make-test-ds-slot-xxx :name "a" :value 10)
                (make-test-ds-slot-xxx :name "b" :value 20)
                (make-test-ds-slot-xxx :name "c" :value 30)))
    (test-eio-compute-xxx o)
    (list (slot-value o 'computed)
          (= (slot-value o 'computed) 60)
          (length (slot-value o 'data))
          (= (length (slot-value o 'data)) 3)
          (test-ds-slot-xxx-name (car (slot-value o 'data)))
          (string= (test-ds-slot-xxx-name (car (slot-value o 'data))) "a")
          (test-ds-slot-xxx-value (cadr (slot-value o 'data)))
          (= (test-ds-slot-xxx-value (cadr (slot-value o 'data))) 20)))) #"#,
        expect,
    );
}

#[test]
fn divergence_eieio_equal_nested_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 15 52)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-eq-nested-xxx ()
    ((val :initarg :val :initform 0)
     (child :initarg :child :initform nil)))
  (let ((leaf1 (make-instance 'test-eq-nested-xxx :val 42))
        (leaf2 (make-instance 'test-eq-nested-xxx :val 42))
        (parent (make-instance 'test-eq-nested-xxx :val 1)))
    (setf (slot-value parent 'child) leaf1)
    (list (eq leaf1 leaf2)
          (null (eq leaf1 leaf2))
          (equal leaf1 leaf2)
          (test-eq-nested-xxx-p parent)
          (slot-value (slot-value parent 'child) 'val)
          (= (slot-value (slot-value parent 'child) 'val) 42)
          (eq (slot-value parent 'child) leaf1)))) #"#,
        expect,
    );
}

#[test]
fn divergence_eieio_print_read_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function test-pr-obj-xxx-a)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-pr-obj-xxx ()
    ((a :initarg :a :initform 0)
     (b :initarg :b :initform nil)))
  (let ((obj (make-instance 'test-pr-obj-xxx :a 42 :b '(x y z))))
    (let ((printed (prin1-to-string obj)))
      (list (stringp printed)
            (> (length printed) 0)
            (string-match "test-pr-obj" printed)
            (string-match "42" printed)
            (test-pr-obj-xxx-a obj)
            (= (test-pr-obj-xxx-a obj) 42)
            (test-pr-obj-xxx-b obj)
            (equal (test-pr-obj-xxx-b obj) '(x y z)))))) #"#,
        expect,
    );
}

#[test]
fn divergence_defstruct_in_eieio_method_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct test-dm-item-xxx name weight)
  (defclass test-dm-container-xxx ()
    ((items :initarg :items :initform nil)))
  (cl-defmethod test-dm-total-xxx ((obj test-dm-container-xxx))
    (cl-loop for item in (slot-value obj 'items)
             sum (test-dm-item-xxx-weight item)))
  (cl-defmethod test-dm-add-xxx ((obj test-dm-container-xxx) name weight)
    (push (make-test-dm-item-xxx :name name :weight weight)
          (slot-value obj 'items)))
  (let ((c (make-instance 'test-dm-container-xxx)))
    (test-dm-add-xxx c "a" 10)
    (test-dm-add-xxx c "b" 20)
    (test-dm-add-xxx c "c" 30)
    (list (test-dm-total-xxx c)
          (= (test-dm-total-xxx c) 60)
          (length (slot-value c 'items))
          (= (length (slot-value c 'items)) 3)
          (test-dm-item-xxx-name (car (slot-value c 'items)))
          (string= (test-dm-item-xxx-name (car (slot-value c 'items))) "c")))) #"#,
        expect,
    );
}

#[test]
fn divergence_eieio_with_closure_factory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 17 38)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-cf-xxx ()
    ((val :initarg :val :initform 0)))
  (defun test-make-counter-xxx (start)
    (let ((obj (make-instance 'test-cf-xxx :val start)))
      (list (lambda () (slot-value obj 'val))
            (lambda () (cl-incf (slot-value obj 'val))))))
  (let* ((fns (test-make-counter-xxx 10))
         (reader (car fns))
         (incrementer (cadr fns)))
    (list (funcall reader)
          (= (funcall reader) 10)
          (funcall incrementer)
          (= (funcall reader) 11)
          (funcall incrementer)
          (funcall incrementer)
          (= (funcall reader) 13)))) #"#,
        expect,
    );
}

#[test]
fn divergence_eieio_cl_print_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 3 nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
  (defclass test-po-xxx ()\n\
    ((x :initarg :x :initform 0)\n\
     (y :initarg :y :initform 0)))\n\
  (cl-defmethod cl-print-object ((obj test-po-xxx) stream)\n\
    (princ (format \"#<test-po x=%d y=%d>\" (slot-value obj 'x) (slot-value obj 'y))\n\
           stream))\n\
  (let ((obj (make-instance 'test-po-xxx :x 5 :y 10)))\n\
    (let ((printed (prin1-to-string obj)))\n\
      (list (stringp printed)\n\
            (> (length printed) 0)\n\
            (string-match \"test-po\" printed)\n\
            (string-match \"x=5\" printed)\n\
            (string-match \"y=10\" printed)\n\
            (string-match \"<\" printed))))) ",
        expect,
    );
}

#[test]
fn divergence_eieio_class_hierarchy_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 17 77)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-ch-root-xxx () ((id :initarg :id)))
  (defclass test-ch-mid-xxx (test-ch-root-xxx) ((mid :initarg :mid)))
  (defclass test-ch-leaf-xxx (test-ch-mid-xxx) ((leaf :initarg :leaf)))
  (let ((root (make-instance 'test-ch-root-xxx :id 1))
        (mid (make-instance 'test-ch-mid-xxx :id 2 :mid 'm))
        (leaf (make-instance 'test-ch-leaf-xxx :id 3 :mid 'm :leaf 'l)))
    (list (test-ch-root-xxx-p root)
          (test-ch-root-xxx-p mid)
          (test-ch-root-xxx-p leaf)
          (null (test-ch-mid-xxx-p root))
          (test-ch-mid-xxx-p mid)
          (test-ch-mid-xxx-p leaf)
          (null (test-ch-leaf-xxx-p root))
          (null (test-ch-leaf-xxx-p mid))
          (test-ch-leaf-xxx-p leaf)
          (child-of-class-p (eieio-object-class leaf) 'test-ch-root-xxx)))) #"#,
        expect,
    );
}

#[test]
fn deficiency_defstruct_print_read_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct test-prr-xxx (x 0) (y 0))
  (let ((obj (make-test-prr-xxx :x 42 :y 99)))
    (let ((printed (prin1-to-string obj))
          (re-read (read (prin1-to-string obj))))
      (list printed
            (stringp printed)
            (> (length printed) 0)
            (string-match "test-prr" printed)
            (test-prr-xxx-x obj)
            (= (test-prr-xxx-x obj) 42)
            (test-prr-xxx-y obj)
            (= (test-prr-xxx-y obj) 99))))) #"#,
        expect,
    );
}

#[test]
fn divergence_eieio_object_with_hash_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 16 39)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-hk-xxx ()
    ((name :initarg :name :initform "")))
  (let ((ht (make-hash-table :test 'eql))
        (o1 (make-instance 'test-hk-xxx :name "first"))
        (o2 (make-instance 'test-hk-xxx :name "second")))
    (puthash o1 100 ht)
    (puthash o2 200 ht)
    (list (gethash o1 ht)
          (= (gethash o1 ht) 100)
          (gethash o2 ht)
          (= (gethash o2 ht) 200)
          (hash-table-count ht)
          (= (hash-table-count ht) 2)
          (eieio-object-p o1)
          (eq (gethash o1 ht) 100)))) #"#,
        expect,
    );
}

#[test]
fn divergence_eieio_slot_type_checking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (invalid-slot-type test-tc-xxx num number \"not-a-number\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-tc-xxx ()
    ((num :initarg :num :type number :initform 0)
     (str :initarg :str :type string :initform "")))
  (let ((o (make-instance 'test-tc-xxx :num 42 :str "hello")))
    (list (slot-value o 'num)
          (= (slot-value o 'num) 42)
          (slot-value o 'str)
          (string= (slot-value o 'str) "hello")
          (condition-case e
              (progn (oset o num "not-a-number") 'no-error)
            (wrong-type-argument 'type-error))
          (slot-value o 'num)
          (numberp (slot-value o 'num))))) #"#,
        expect,
    );
}

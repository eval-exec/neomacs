//! Divergence tests: advice + EIEIO + keymap triple combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_advice_on_generic_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 27)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-adv-obj-xxx () ((v :initarg :v :initform 1)))
  (cl-defgeneric test-adv-compute-xxx (obj) "Compute.")
  (cl-defmethod test-adv-compute-xxx ((obj test-adv-obj-xxx))
    (* (slot-value obj 'v) 10))
  (advice-add 'test-adv-compute-xxx :filter-return
               (lambda (r) (+ r 100)))
  (let ((o (test-adv-obj-xxx "o" :v 5)))
    (let ((r1 (test-adv-compute-xxx o)))
      (advice-remove 'test-adv-compute-xxx
                      (lambda (r) (+ r 100)))
      (list r1 (test-adv-compute-xxx o)
            (= r1 150)))))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_binding_eieio_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((closure ((obj . #s(test-km-obj-xxx \"World\"))) nil (test-km-greet-xxx obj)) t \"Hello, World\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-km-obj-xxx () ((name :initarg :name)))
  (cl-defmethod test-km-greet-xxx ((obj test-km-obj-xxx))
    (format "Hello, %s" (slot-value obj 'name)))
  (let ((map (make-sparse-keymap))
        (obj (test-km-obj-xxx "o" :name "World")))
    (define-key map "g"
      (lambda () (interactive) (test-km-greet-xxx obj)))
    (list (lookup-key map "g")
          (commandp (lookup-key map "g"))
          (funcall (lookup-key map "g"))))) "#,
        expect,
    );
}

#[test]
fn divergence_advice_on_defun_creating_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (50 t t nil 70 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-factory-xxx () ((id :initarg :id)))
  (defun test-make-factory-xxx (id)
    (test-factory-xxx "f" :id id))
  (advice-add 'test-make-factory-xxx :filter-return
               (lambda (obj)
                 (setf (slot-value obj 'id) (* (slot-value obj 'id) 10))
                 obj))
  (let ((o (test-make-factory-xxx 5)))
    (list (slot-value o 'id)
          (= (slot-value o 'id) 50)
          (eieio-object-p o)
          (advice-remove 'test-make-factory-xxx
                          (lambda (obj)
                            (setf (slot-value obj 'id) (* (slot-value obj 'id) 10))
                            obj))
          (slot-value (test-make-factory-xxx 7) 'id)
          (= (slot-value (test-make-factory-xxx 7) 'id) 7)))) "#,
        expect,
    );
}

#[test]
fn divergence_eieio_accessor_advice_setf() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 99 99 (accessed accessed) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-acc-xxx ()
    ((val :initarg :val :accessor test-acc-val-xxx :initform 0)))
  (let ((log nil))
    (advice-add 'test-acc-val-xxx :after
                 (lambda (&rest _) (push 'accessed log)))
    (let ((o (test-acc-xxx "o" :val 10)))
      (list (test-acc-val-xxx o)
            (setf (test-acc-val-xxx o) 99)
            (test-acc-val-xxx o)
            (nreverse log)
            (>= (length log) 3)
            (advice-remove 'test-acc-val-xxx
                            (lambda (&rest _) (push 'accessed log))))))) "#,
        expect,
    );
}

#[test]
fn divergence_method_inheritance_advice_child() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((advised parent) (advised child) t t nil child t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-parent-xxx () ())
  (defclass test-child-xxx (test-parent-xxx) ())
  (cl-defgeneric test-hierarchy-xxx (obj) "Dispatch.")
  (cl-defmethod test-hierarchy-xxx ((obj test-parent-xxx)) 'parent)
  (cl-defmethod test-hierarchy-xxx ((obj test-child-xxx)) 'child)
  (advice-add 'test-hierarchy-xxx :filter-return
               (lambda (r) (list 'advised r)))
  (let ((p (test-parent-xxx "p"))
        (c (test-child-xxx "c")))
    (list (test-hierarchy-xxx p)
          (test-hierarchy-xxx c)
          (equal (test-hierarchy-xxx p) '(advised parent))
          (equal (test-hierarchy-xxx c) '(advised child))
          (advice-remove 'test-hierarchy-xxx
                          (lambda (r) (list 'advised r)))
          (test-hierarchy-xxx c)
          (eq (test-hierarchy-xxx c) 'child)))) "#,
        expect,
    );
}

#[test]
fn divergence_defclass_accessor_keymap_describe() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t test-desc-data-xxx t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-desc-xxx ()
    ((data :initarg :data :accessor test-desc-data-xxx :initform nil)))
  (let ((map (make-sparse-keymap)))
    (define-key map "d" 'test-desc-data-xxx)
    (list (commandp 'test-desc-data-xxx)
          (fboundp 'test-desc-data-xxx)
          (lookup-key map "d")
          (eq (lookup-key map "d") 'test-desc-data-xxx)))) "#,
        expect,
    );
}

#[test]
fn divergence_defgeneric_filter_args_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 t nil 30 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-fa-xxx () ((v :initarg :v :initform 0)))
  (cl-defgeneric test-fa-add-xxx (obj n) "Add n to v.")
  (cl-defmethod test-fa-add-xxx ((obj test-fa-xxx) n)
    (oset obj v (+ (slot-value obj 'v) n)))
  (advice-add 'test-fa-add-xxx :filter-args
               (lambda (args) (list (car args) (* (cadr args) 2))))
  (let ((o (test-fa-xxx "o" :v 10)))
    (test-fa-add-xxx o 5)
    (list (slot-value o 'v)
          (= (slot-value o 'v) 20)
          (advice-remove 'test-fa-add-xxx
                          (lambda (args) (list (car args) (* (cadr args) 2))))
          (test-fa-add-xxx o 5)
          (= (slot-value o 'v) 25)))) "#,
        expect,
    );
}

#[test]
fn divergence_method_error_after_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 (2 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-err-method-log-xxx nil)
  (defclass test-err-xxx () ((v :initarg :v)))
  (cl-defmethod test-err-incr-xxx ((obj test-err-xxx))
    (let ((old (slot-value obj 'v)))
      (oset obj v (1+ old))
      (when (> (slot-value obj 'v) 3)
        (error "overflow: %d" (slot-value obj 'v)))))
  (cl-defmethod test-err-incr-xxx :after ((obj test-err-xxx))
    (push (slot-value obj 'v) test-err-method-log-xxx))
  (let ((o (test-err-xxx "o" :v 1)))
    (ignore-errors (test-err-incr-xxx o))
    (ignore-errors (test-err-incr-xxx o))
    (ignore-errors (test-err-incr-xxx o))
    (list (slot-value o 'v)
          (nreverse test-err-method-log-xxx)))) "#,
        expect,
    );
}

#[test]
fn divergence_advice_on_make_instance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (99 t nil 99 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-mi-xxx () ((val :initarg :val :initform 0)))
  (advice-add 'make-instance :filter-args
               (lambda (args)
                 (if (eq (car args) 'test-mi-xxx)
                     (list (car args) (cadr args) :val 99)
                   args)))
  (let ((o (test-mi-xxx "o" :val 1)))
    (list (slot-value o 'val)
          (= (slot-value o 'val) 99)
          (advice-remove 'make-instance
                          (lambda (args)
                            (if (eq (car args) 'test-mi-xxx)
                                (list (car args) (cadr args) :val 99)
                              args)))
          (slot-value (test-mi-xxx "o2" :val 5) 'val)
          (= (slot-value (test-mi-xxx "o2" :val 5) 'val) 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_keymap_parent_chain_with_eieio() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((closure (t) nil 'parent-x) (closure (t) nil 'child-y) nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defclass test-km-disp-xxx () ((mode :initarg :mode)))
  (cl-defmethod test-km-execute-xxx ((obj test-km-disp-xxx))
    (slot-value obj 'mode))
  (let ((parent-map (make-sparse-keymap))
        (child-map (make-sparse-keymap)))
    (define-key parent-map "x" (lambda () (interactive) 'parent-x))
    (set-keymap-parent child-map parent-map)
    (define-key child-map "y" (lambda () (interactive) 'child-y))
    (list (lookup-key child-map "x")
        (lookup-key child-map "y")
        (lookup-key child-map "z")
        (commandp (lookup-key child-map "x"))
        (commandp (lookup-key child-map "y"))))) "#,
        expect,
    );
}

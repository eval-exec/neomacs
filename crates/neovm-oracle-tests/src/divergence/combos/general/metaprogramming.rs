//! Divergence tests: deep Elisp metaprogramming combinations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eval_and_defun_macro_generate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defmacro test-define-accessors-xxx (class slots)
    \\`(progn
       ,@(mapcar (lambda (s)
                   \\`(defun ,(intern (format \"test-%s-%s-xxx\" (symbol-name class) (symbol-name s))) (obj)
                      (slot-value obj ',s)))
                 slots)))
  (defclass test-item-xxx () ((name :initarg :name) (value :initarg :value)))
  (test-define-accessors-xxx item (name value))
  (let ((obj (test-item-xxx \"o\" :name \"test\" :value 42)))
    (list (test-item-name-xxx obj)
          (test-item-value-xxx obj)
          (fboundp 'test-item-name-xxx)))) ", expect);
}

#[test]
fn divergence_advice_around_with_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1050 1030 2 t t 1070)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-around-count-xxx 0)
  (defun test-around-fn-xxx (x) (* x 10))
  (advice-add 'test-around-fn-xxx :around
               (lambda (fn &rest args)
                 (cl-incf test-around-count-xxx)
                 (let ((result (apply fn args)))
                   (+ result 1000))))
  (let ((r1 (test-around-fn-xxx 5))
        (r2 (test-around-fn-xxx 3)))
    (advice-remove 'test-around-fn-xxx
                    (lambda (fn &rest args)
                      (cl-incf test-around-count-xxx)
                      (let ((result (apply fn args)))
                        (+ result 1000))))
    (list r1 r2 test-around-count-xxx
          (= r1 1050)
          (= r2 1030)
          (test-around-fn-xxx 7)))) ",
        expect,
    );
}

#[test]
fn divergence_compiler_macro_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function inline-leteval)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (define-inline test-inline-square-xxx (x)
    (inline-leteval (x)
      (inline-quote (* ,x ,x))))
  (defun test-use-square-xxx (n)
    (+ (test-inline-square-xxx n) 1))
  (list (test-use-square-xxx 3)
        (test-use-square-xxx 0)
        (test-use-square-xxx -5)
        (= (test-use-square-xxx 3) 10)
        (= (test-use-square-xxx 0) 1))) ",
        expect,
    );
}

#[test]
fn deficiency_obarray_intern_with_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 t 150 150 t test-ob-set-xxx t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((sym (intern \"test-ob-set-xxx\" obarray)))
  (set sym 100)
  (list (symbol-value sym)
        (boundp sym)
        (set sym (+ (symbol-value sym) 50))
        (symbol-value sym)
        (= (symbol-value sym) 150)
        (makunbound sym)
        (not (boundp sym)))) ",
        expect,
    );
}

#[test]
fn divergence_nested_macro_with_gensym() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defmacro test-alet-xxx (bindings &rest body)
    (let ((var (make-symbol \"result\")))
      \\`(let ((,var nil))
         (dolist (b (list ,@bindings))
           (setq ,var (cons b ,var)))
         (let ((result (nreverse ,var)))
           ,@body))))
  (test-alet-xxx (1 2 3 4 5) result)) ",
        expect,
    );
}

#[test]
fn divergence_cl_defgeneric_method_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-shape-xxx () ())
  (defclass test-circle-xxx (test-shape-xxx) ((r :initarg :r)))
  (defclass test-square-xxx (test-shape-xxx) ((s :initarg :s)))
  (cl-defgeneric test-area-xxx (obj) \"Area\")
  (cl-defmethod test-area-xxx ((obj test-circle-xxx))
    (* float-pi (expt (slot-value obj 'r) 2)))
  (cl-defmethod test-area-xxx ((obj test-square-xxx))
    (expt (slot-value obj 's) 2))
  (cl-defgeneric test-perimeter-xxx (obj) \"Perimeter\")
  (cl-defmethod test-perimeter-xxx ((obj test-circle-xxx))
    (* 2 float-pi (slot-value obj 'r)))
  (cl-defmethod test-perimeter-xxx ((obj test-square-xxx))
    (* 4 (slot-value obj 's)))
  (let ((c (test-circle-xxx \"c\" :r 1))
        (s (test-square-xxx \"s\" :s 2)))
    (list (> (test-area-xxx c) 3.0)
          (= (test-area-xxx s) 4)
          (> (test-perimeter-xxx c) 6.0)
          (= (test-perimeter-xxx s) 8)))) ",
        expect,
    );
}

#[test]
fn divergence_eval_region_with_defuns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 15 25)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (let ((code \"
    (defun test-eval-fn1-xxx (x) (+ x 10))
    (defun test-eval-fn2-xxx (x) (* x 3))
    (defun test-eval-fn3-xxx (x) (test-eval-fn1-xxx (test-eval-fn2-xxx x)))
  \"))
    (eval (car (read-from-string code)))
    (eval (car (read-from-string
                (substring code (cdr (read-from-string code))))))
    (eval (car (read-from-string
                (substring code
                           (+ (cdr (read-from-string code))
                              (cdr (read-from-string
                                    (substring code (cdr (read-from-string code))))))))))
    (list (test-eval-fn1-xxx 5)
          (test-eval-fn2-xxx 5)
          (test-eval-fn3-xxx 5)))) ",
        expect,
    );
}

#[test]
fn divergence_cl_print_object_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#s(test-doc-xxx Hello)\" \"#s(test-num-xxx 42)\" 8 16)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defclass test-doc-xxx () ((title :initarg :title)))
  (cl-defmethod cl-print-object ((obj test-doc-xxx) stream)
    (princ (format \"#<doc: %s>\" (slot-value obj 'title)) stream)
    obj)
  (defclass test-num-xxx () ((val :initarg :val)))
  (cl-defmethod cl-print-object ((obj test-num-xxx) stream)
    (princ (format \"#<num: %d>\" (slot-value obj 'val)) stream)
    obj)
  (let ((d (test-doc-xxx \"d\" :title \"Hello\"))
        (n (test-num-xxx \"n\" :val 42)))
    (list (format \"%s\" d)
          (format \"%s\" n)
          (string-match \"doc\" (format \"%s\" d))
          (string-match \"42\" (format \"%s\" n))))) ",
        expect,
    );
}

#[test]
fn divergence_function_documentation_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Double X.\" \"Double X.\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-doc-fn-xxx (x) \"Double X.\" (* x 2))
  (defalias 'test-doc-alias-xxx 'test-doc-fn-xxx)
  (list (documentation 'test-doc-fn-xxx)
        (documentation 'test-doc-alias-xxx)
        (string= (documentation 'test-doc-fn-xxx) \"Double X.\")
        (string= (documentation 'test-doc-alias-xxx) \"Double X.\"))) ",
        expect,
    );
}

#[test]
fn divergence_setf_with_custom_places() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-getval-xxx (alist key)
    (cdr (assoc key alist)))
  (gv-define-setter test-getval-xxx (val alist key)
    \\`(setcdr (or (assoc ,key ,alist)
                  (car (push (cons ,key nil) ,alist)))
              ,val))
  (let ((data '((a . 1) (b . 2))))
    (setf (test-getval-xxx data 'a) 99)
    (setf (test-getval-xxx data 'c) 77)
    (list data
          (test-getval-xxx data 'a)
          (test-getval-xxx data 'c)
          (length data)))) ",
        expect,
    );
}

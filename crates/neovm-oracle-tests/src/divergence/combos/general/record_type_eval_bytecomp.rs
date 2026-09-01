//! Divergence tests: cl-record + type + bytecomp + eval combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_record_type_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-rec-xxx (:type list)) a b c)
  (let ((r (make-test-rec-xxx :a 1 :b 2 :c 3)))
    (list (test-rec-xxx-a r)
          (= (test-rec-xxx-a r) 1)
          (test-rec-xxx-b r)
          (= (test-rec-xxx-b r) 2)
          (test-rec-xxx-c r)
          (= (test-rec-xxx-c r) 3)
          (listp r)
          (setf (test-rec-xxx-a r) 99)
          (= (test-rec-xxx-a r) 99)
          (test-rec-xxx-b r)
          (= (test-rec-xxx-b r) 2)))) #"#,
        expect,
    );
}

#[test]
fn divergence_eval_defun_with_closure_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 t 200 nil 200 nil 200 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-edc-state-xxx 0)
  (let ((test-edc-state-xxx 100))
    (eval '(defun test-edc-get-xxx () test-edc-state-xxx)))
  (let ((v1 (test-edc-get-xxx)))
    (let ((test-edc-state-xxx 200))
      (list v1
            (= v1 0)
            (test-edc-get-xxx)
            (= (test-edc-get-xxx) 0)
            test-edc-state-xxx
            (= test-edc-state-xxx 0)
            (symbol-value 'test-edc-state-xxx)
            (= (symbol-value 'test-edc-state-xxx) 0))))) "#,
        expect,
    );
}

#[test]
fn divergence_macro_expansion_nested_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p (1 2 3 4 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defmacro test-nme-xxx (op &rest args)
    (list op (cons 'list args)))
  (list (test-nme-xxx + 1 2 3 4 5)
        (= (test-nme-xxx + 1 2 3 4 5) 15)
        (test-nme-xxx * 2 3 4)
        (= (test-nme-xxx * 2 3 4) 24)
        (macroexpand '(test-nme-xxx + 10 20))
        (equal (macroexpand '(test-nme-xxx + 10 20))
               '(+ (list 10 20)))
        (eval (macroexpand '(test-nme-xxx + 10 20)))
        (= (eval (macroexpand '(test-nme-xxx + 10 20))) 30))) #"#,
        expect,
    );
}

#[test]
fn divergence_lambda_in_eval_with_dynvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 14 39)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-led-xxx 'outer)
  (let ((fn (eval '(lambda () test-led-xxx))))
    (list (funcall fn)
          (eq (funcall fn) 'outer)
          (let ((test-led-xxx 'inner))
            (funcall fn))
          (eq (let ((test-led-xxx 'inner))
                (funcall fn))
              'outer)
          (funcall fn)
          (eq (funcall fn) 'outer)
          test-led-xxx
          (eq test-led-xxx 'outer)))) #"#,
        expect,
    );
}

#[test]
fn divergence_closure_let_binding_evaluation_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 13 50)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-lbe-log-xxx nil)
  (let ((fns nil))
    (dolist (i '(1 2 3 4 5))
      (push (let ((x i))
              (lambda () (push x test-lbe-log-xxx)))
            fns))
    (mapc 'funcall (nreverse fns))
    (list (nreverse test-lbe-log-xxx)
          (equal (nreverse test-lbe-log-xxx) '(1 2 3 4 5))
          (= (length test-lbe-log-xxx) 5)
          (= (car test-lbe-log-xxx) 5)
          (= (car (last test-lbe-log-xxx)) 1)))) #"#,
        expect,
    );
}

#[test]
fn divergence_record_vector_accessor_compatibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-defstruct (test-rva-xxx (:type vector)) x y z)
  (let ((r (make-test-rva-xxx :x 10 :y 20 :z 30)))
    (list (vectorp r)
          (aref r 0) (= (aref r 0) 10)
          (aref r 1) (= (aref r 1) 20)
          (aref r 2) (= (aref r 2) 30)
          (length r) (= (length r) 3)
          (test-rva-xxx-x r) (= (test-rva-xxx-x r) 10)
          (aset r 1 99)
          (aref r 1) (= (aref r 1) 99)
          (test-rva-xxx-y r) (= (test-rva-xxx-y r) 99)))) #"#,
        expect,
    );
}

#[test]
fn divergence_defalias_and_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 12 42)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-da-original-xxx (x) (+ x 1))
  (defalias 'test-da-alias-xxx 'test-da-original-xxx)
  (list (funcall 'test-da-original-xxx 5)
        (= (funcall 'test-da-original-xxx 5) 6)
        (funcall 'test-da-alias-xxx 10)
        (= (funcall 'test-da-alias-xxx 10) 11)
        (eq (symbol-function 'test-da-alias-xxx)
            (symbol-function 'test-da-original-xxx))
        (fboundp 'test-da-alias-xxx)
        (fboundp 'test-da-original-xxx)
        (functionp 'test-da-alias-xxx))) #"#,
        expect,
    );
}

#[test]
fn divergence_advised_closure_in_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 15 39)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-acie-xxx (x) (* x 2))
  (advice-add 'test-acie-xxx :filter-return
               (lambda (r) (+ r 100)))
  (let ((result (eval '(test-acie-xxx 5))))
    (list result
          (= result 110)
          (funcall 'test-acie-xxx 10)
          (= (funcall 'test-acie-xxx 10) 120)
          (apply 'test-acie-xxx '(20))
          (= (apply 'test-acie-xxx '(20)) 140)
          (advice-remove 'test-acie-xxx
                          (lambda (r) (+ r 100)))
          (test-acie-xxx 5)
          (= (test-acie-xxx 5) 10)))) #"#,
        expect,
    );
}

#[test]
fn divergence_defvar_vs_defconst_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function constantp)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-dvc-var-xxx 10)
  (defconst test-dvc-const-xxx 20)
  (list test-dvc-var-xxx
        (= test-dvc-var-xxx 10)
        test-dvc-const-xxx
        (= test-dvc-const-xxx 20)
        (setq test-dvc-var-xxx 99)
        (= test-dvc-var-xxx 99)
        (not (constantp 'test-dvc-var-xxx))
        (constantp 'test-dvc-const-xxx)
        (special-variable-p 'test-dvc-var-xxx)
        (special-variable-p 'test-dvc-const-xxx))) #"#,
        expect,
    );
}

#[test]
fn divergence_function_interactive_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 8 45)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-fis-xxx (a b) (interactive "nA: \nnB: ") (+ a b))
  (list (commandp 'test-fis-xxx)
        (fboundp 'test-fis-xxx)
        (interactive-form 'test-fis-xxx)
        (equal (interactive-form 'test-fis-xxx) '(interactive "nA: \nnB: "))
        (funcall 'test-fis-xxx 3 4)
        (= (funcall 'test-fis-xxx 3 4) 7))) #"#,
        expect,
    );
}

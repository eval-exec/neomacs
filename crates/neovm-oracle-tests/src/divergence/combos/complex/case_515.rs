/// Batch 515: function/macro/alias combined deep characterization.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx515_function_cell_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (make-symbol "cx515-fcm")))
  (fset s (lambda (x) (* x 2)))
  (list (fboundp s) (funcall s 5)))
"##,
        expect,
    );
}

#[test]
fn div_cx515_defalias_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((a (make-symbol "cx515-a"))
      (b (make-symbol "cx515-b")))
  (defalias a 'car)
  (defalias b a)
  (eq (indirect-function b) (symbol-function 'car)))
"##,
        expect,
    );
}

#[test]
fn div_cx515_function_get_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK test-val""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (make-symbol "cx515-fgp")))
  (defalias s 'cdr)
  (function-put s 'test-attr 'test-val)
  (function-get s 'test-attr))
"##,
        expect,
    );
}

#[test]
fn div_cx515_interactive_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t (interactive \"p\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f1 (lambda (x) (interactive "p") x))
      (f2 (lambda () (interactive) 42)))
  (list (commandp f1) (commandp f2) (interactive-form f1)))
"##,
        expect,
    );
}

#[test]
fn div_cx515_command_modes_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (text-mode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (lambda () (interactive))))
  (put 'cx515-cmd 'command-modes '(text-mode))
  (defalias 'cx515-cmd f)
  (command-modes 'cx515-cmd))
"##,
        expect,
    );
}

#[test]
fn div_cx515_subr_arity_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument subrp car)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(mapcar #'subr-arity '(car cdr cons + - * / concat list vector))
"##,
        expect,
    );
}

#[test]
fn div_cx515_subr_name_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument subrp car)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(mapcar #'subr-name '(car cdr cons +))
"##,
        expect,
    );
}

#[test]
fn div_cx515_help_function_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((arg1) (&rest rest) (arg1 arg2 &rest rest))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (help-function-arglist 'car)
      (help-function-arglist 'concat)
      (help-function-arglist 'if))
"##,
        expect,
    );
}

#[test]
fn div_cx515_documentation_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((doc (documentation 'car)))
  (and (stringp doc) (> (length doc) 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx515_indirect_function_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK cyclic-function-indirection""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((a (make-symbol "cx515-ifa"))
          (b (make-symbol "cx515-ifb")))
      (defalias a b)
      (defalias b a)
      (indirect-function a))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx515_called_interactively() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (lambda ()
             (interactive)
             (called-interactively-p 'any))))
  (list (commandp f)))
"##,
        expect,
    );
}

#[test]
fn div_cx515_function_alias_p_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (make-symbol "cx515-fap")))
  (defalias s 'forward-char)
  (fboundp s))
"##,
        expect,
    );
}

#[test]
fn div_cx515_autoload_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'autoload)
  (list (boundp 'autoload-modified-buffers) (fboundp 'autoload-rubric)))
"##,
        expect,
    );
}

#[test]
fn div_cx515_byte_code_function_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 21)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (byte-compile (lambda (x) (* x 3)))))
  (list (byte-code-function-p f) (funcall f 7)))
"##,
        expect,
    );
}

#[test]
fn div_cx515_closurep_interp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (lambda (x) (+ x 1))))
  (list (functionp f) (closurep f) (subrp f)))
"##,
        expect,
    );
}

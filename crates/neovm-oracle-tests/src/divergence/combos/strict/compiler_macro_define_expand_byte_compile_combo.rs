//! Strict combo oracle probes, batch 289: compiler-macro define/expand
//! byte-compile combo. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_define_compiler_macro_function_get_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (define-compiler-macro my-cm-test (a)
    `(+ ,a 1))
  (list (functionp (function-get 'my-cm-test 'compiler-macro))
        (not (null (function-get 'my-cm-test 'compiler-macro)))
        (funcall (function-get 'my-cm-test 'compiler-macro)
                 '(my-cm-test 41) nil)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function define-compiler-macro)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_compiler_macro_byte_compile_replacement_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defun my-cm-target (n) (* n 2))
  (define-compiler-macro my-cm-target (n)
    `(* ,n 2))
  (let ((byte-compile-warnings nil))
    (byte-compile
     (lambda (x)
       (list (my-cm-target x)
             (my-cm-target 10))))
    (list (function-get 'my-cm-target 'compiler-macro)
          (compiled-function-p
           (byte-compile
            (lambda (x) (my-cm-target x)))))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function define-compiler-macro)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_compiler_macro_whole_form_and_docstring_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (define-compiler-macro my-cm-whole (&whole form a b)
    (list 'quote (list form a b)))
  (list (funcall (function-get 'my-cm-whole 'compiler-macro)
                 '(my-cm-whole 1 2) nil)
        (documentation (function-get 'my-cm-whole 'compiler-macro) t)
        (not (null (get 'my-cm-whole 'compiler-macro)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function define-compiler-macro)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

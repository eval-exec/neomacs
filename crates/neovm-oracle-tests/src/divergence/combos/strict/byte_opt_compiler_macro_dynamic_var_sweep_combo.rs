//! Strict combo oracle probes, batch 291: byte-opt / compiler-macro / dynamic
//! CORE variable sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_byte_compile_warnings_dynamic_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'byte-compile-warnings)
      (boundp 'byte-compile-current-buffer)
      (boundp 'byte-compile--out-buffer)
      (boundp 'byte-compile-delete-errors)
      (boundp 'byte-compile-dynamic)
      (boundp 'byte-compile-dynamic-docstrings)
      (boundp 'byte-compile-generate-call-tree)
      (boundp 'byte-compile-verbose)
      (boundp 'byte-compile-dest-file-function)
      (boundp 'byte-compile-root-functions)
      (boundp 'emacs-lisp-file-regexp)
      (boundp 'lexical-binding))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil nil nil nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_compiler_macro_defalias_advice_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'macro-declarations-alist)
      (boundp 'function-put)
      (boundp 'compiler-macro-file)
      (boundp 'ad-redefinition-action)
      (boundp 'load-prefer-newer)
      (boundp 'find-function-C-source-directory)
      (boundp 'find-function-regexp)
      (boundp 'find-variable-regexp)
      (boundp 'generated-autoload-file)
      (boundp 'autoload-compute-prefixes)
      (boundp 'make-autoload)
      (boundp 'byte-compile-defm))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil t t nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_lexical_dynbind_eval_module_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'lexical-binding)
      (boundp 'internal-interpreter-environment)
      (boundp 'eval-expression-print-format)
      (boundp 'eval-expression-debug-on-error)
      (boundp 'load-read-function)
      (boundp 'read-circle)
      (boundp 'read-with-symbol-positions)
      (boundp 'module-file-suffix)
      (boundp 'module-functions)
      (boundp 'module-data-function)
      (boundp 'module-env-type)
      (boundp 'module-assertions))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil t t t nil t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

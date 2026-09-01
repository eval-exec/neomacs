//! Strict combo oracle probes, batch 274: lread / load / eval-expression CORE
//! variable sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_load_path_suffixes_after_load_alist_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'load-path)
      (boundp 'load-file-name)
      (boundp 'load-suffixes)
      (boundp 'load-file-rep-suffixes)
      (boundp 'load-in-progress)
      (boundp 'load-read-function)
      (boundp 'load-source-file-function)
      (boundp 'after-load-alist)
      (boundp 'load-history)
      (boundp 'load-dangerous-libraries)
      (boundp 'read-circle)
      (boundp 'read-with-symbol-positions))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_eval_expression_print_format_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'eval-expression-print-format)
      (boundp 'eval-expression-print-level)
      (boundp 'eval-expression-print-length)
      (boundp 'eval-expression-debug-on-error)
      (boundp 'read-quoted-char-radix)
      (boundp 'read-symbol-positions-list)
      (boundp 'lread)
      (boundp 'read-buffer)
      (boundp 'define-symbol-props)
      (boundp 'byte-compile-current-buffer)
      (boundp 'byte-compile-warnings)
      (boundp 'byte-optimize))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t t t nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_features_provided_required_alist_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'features)
      (boundp 'load-file-rep-suffixes)
      (boundp 'package-alist)
      (boundp 'package-activated-list)
      (boundp 'package-archive-base)
      (boundp 'package-user-dir)
      (boundp 'package-directory-list)
      (boundp 'package-load-list)
      (boundp 'list-load-path-shadows)
      (boundp 'find-function-C-source-directory)
      (boundp 'source-directory)
      (boundp 'emacs-version))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil t nil t t nil nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

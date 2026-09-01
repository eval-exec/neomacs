//! Strict combo oracle probes, batch 258: font-lock / jit-lock / syntax-
//! highlighting variable existence sweep. Any nil-in-Neomacs/t-in-GNU is a
//! missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_font_lock_maximum_decoration_support_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'font-lock-maximum-size)
      (boundp 'font-lock-maximum-decoration)
      (boundp 'font-lock-verbose)
      (boundp 'font-lock-support-mode)
      (boundp 'font-lock-beginning-of-syntax-function)
      (boundp 'font-lock-mark-block-function)
      (boundp 'font-lock-syntactic-face-function)
      (boundp 'font-lock-keywords-only)
      (boundp 'font-lock-keywords-case-fold-search)
      (boundp 'font-lock-multiline)
      (boundp 'font-lock-default-fontify-buffer-function)
      (boundp 'font-lock-fontify-buffer-function)
      (boundp 'font-lock-fontify-region-function)
      (boundp 'font-lock-defaults)
      (boundp 'font-lock-mode))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t t nil t t t t t nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_jit_lock_context_stealth_defer_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'jit-lock-contextually)
      (boundp 'jit-lock-context-time)
      (boundp 'jit-lock-stealth-time)
      (boundp 'jit-lock-stealth-load)
      (boundp 'jit-lock-stealth-nice)
      (boundp 'jit-lock-defer-time)
      (boundp 'jit-lock-defer-contextually)
      (boundp 'fontification-functions)
      (boundp 'jit-lock-mode)
      (boundp 'font-lock-extend-after-change-region-function)
      (boundp 'syntax-beginning-function))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_syntax_comment_parse_sexp_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'parse-sexp-ignore-comments)
      (boundp 'parse-sexp-lookup-properties)
      (boundp 'comment-start)
      (boundp 'comment-end)
      (boundp 'comment-start-skip)
      (boundp 'comment-end-skip)
      (boundp 'comment-column)
      (boundp 'comment-indent-function)
      (boundp 'comment-multi-line)
      (boundp 'comment-line-break-function)
      (boundp 'syntax-propertize-function)
      (boundp 'syntax-propertize-extend-region-functions)
      (boundp 'forward-sexp-function)
      (boundp 'multibyte-syntax-as-symbol))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

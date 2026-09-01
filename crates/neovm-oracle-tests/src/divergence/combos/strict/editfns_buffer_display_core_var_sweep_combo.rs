//! Strict combo oracle probes, batch 262: editfns / buffer-display CORE
//! variable existence sweep (vars from loadup-loaded files). Any nil-in-
//! Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_transient_mark_word_wrap_truncate_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'transient-mark-mode)
      (boundp 'inhibit-read-only)
      (boundp 'buffer-read-only)
      (boundp 'word-wrap)
      (boundp 'truncate-lines)
      (boundp 'truncate-partial-width-windows)
      (boundp 'line-move-visual)
      (boundp 'goal-column)
      (boundp 'fill-column)
      (boundp 'left-margin)
      (boundp 'auto-fill-function)
      (boundp 'use-hard-newlines)
      (boundp 'default-justification))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_selective_display_paragraph_start_separate_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'selective-display)
      (boundp 'selective-display-ellipses)
      (boundp 'paragraph-start)
      (boundp 'paragraph-separate)
      (boundp 'paragraph-ignore-fill-prefix)
      (boundp 'sentence-end)
      (boundp 'sentence-end-double-space)
      (boundp 'sentence-end-without-period)
      (boundp 'sentence-end-base)
      (boundp 'page-delimiter)
      (boundp 'adaptive-fill-mode)
      (boundp 'adaptive-fill-first-line-regexp)
      (boundp 'adaptive-fill-regexp))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_mode_line_format_header_line_tab_line_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'mode-line-format)
      (boundp 'header-line-format)
      (boundp 'tab-line-format)
      (boundp 'mode-name)
      (boundp 'mode-line-process)
      (boundp 'mode-line-modes)
      (boundp 'mode-line-mule-info)
      (boundp 'mode-line-client)
      (boundp 'mode-line-modified)
      (boundp 'mode-line-front-space)
      (boundp 'mode-line-end-spaces)
      (boundp 'mode-line-position)
      (boundp 'mode-line-remote)
      (boundp 'mode-line-frame-identification))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Strict combo oracle probes, batch 256: minibuffer / completion / history
//! variable existence sweep. boundp over standard minibuffer+completion
//! defcustoms. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_minibuffer_completion_styles_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'minibuffer-follows-selected-frame)
      (boundp 'read-buffer-function)
      (boundp 'completions-format)
      (boundp 'completion-auto-help)
      (boundp 'completion-auto-select)
      (boundp 'completion-category-overrides)
      (boundp 'completion-styles)
      (boundp 'completion-cycle-threshold)
      (boundp 'completion-flex-nospace)
      (boundp 'completion-pcm-complete-word-inserts-delimiters)
      (boundp 'completion-show-help)
      (boundp 'completions-detailed)
      (boundp 'completions-sort)
      (boundp 'enable-recursive-minibuffers))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_history_case_ignore_read_vars_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'history-length)
      (boundp 'history-delete-duplicates)
      (boundp 'minibuffer-history-case-insensitive-variables)
      (boundp 'read-file-name-completion-ignore-case)
      (boundp 'read-buffer-completion-ignore-case)
      (boundp 'completion-ignore-case)
      (boundp 'read-mail-command)
      (boundp 'minibuffer-eldef-shorten-default)
      (boundp 'minibuffer-electric-default-map)
      (boundp 'minibuffer-local-filename-completion-map)
      (boundp 'read-expression-map))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_completion_style_table_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'completion--styles)
      (boundp 'completion-category-defaults)
      (boundp 'completion-extra-properties)
      (boundp 'completion-all-sorted-completions)
      (boundp 'completion-common-substring)
      (boundp 'completions-max-height)
      (boundp 'completions-header-format)
      (boundp 'completions-highlight-face)
      (boundp 'completion-list-compare)
      (boundp 'tmm--completion-table-cache))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t t nil t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

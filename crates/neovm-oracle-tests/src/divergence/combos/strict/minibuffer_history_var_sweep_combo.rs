//! Strict combo oracle probes, batch 268: minibuffer history variable sweep.
//! boundp over the standard *-history vars that core read-* functions define.
//! Any nil-in-Neomacs/t-in-GNU is a missing-variable bug (same class as the
//! buffer-name-history void divergence from batch 266).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_buffer_file_query_replace_history_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'buffer-name-history)
      (boundp 'file-name-history)
      (boundp 'minibuffer-history)
      (boundp 'query-replace-history)
      (boundp 'regexp-history)
      (boundp 'string-rectangle-history)
      (boundp 'minibuffer-history-variable)
      (boundp 'minibuffer-history-case-insensitive-variables)
      (boundp 'read-expression-history)
      (boundp 'extended-command-history)
      (boundp 'shell-command-history)
      (boundp 'read-number-history))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t nil t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_misc_command_history_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'command-history)
      (boundp 'compile-history)
      (boundp 'compile-command)
      (boundp 'grep-history)
      (boundp 'grep-find-history)
      (boundp 'occur-collect-history)
      (boundp 'dired-regexp-history)
      (boundp 'read-char-history)
      (boundp 'read-char-by-name-history)
      (boundp 'read-coding-system-history)
      (boundp 'read-input-method-function-history)
      (boundp 'color-history))
"##;
    let expect = expect_test::expect![[r#""OK (t nil t t t nil nil t nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_history_length_delete_duplicates_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'history-length)
      (boundp 'history-delete-duplicates)
      (boundp 'history-add-new-input)
      (boundp 'minibuffer-history-variable)
      (boundp 'minibuffer-history-sexp-flag)
      (boundp 'read-buffer-function)
      (boundp 'read-file-name-function)
      (boundp 'minibuffer-completing-file-name)
      (boundp 'minibuffer-completion-table)
      (boundp 'minibuffer-completion-predicate)
      (boundp 'completion-cycle-threshold))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

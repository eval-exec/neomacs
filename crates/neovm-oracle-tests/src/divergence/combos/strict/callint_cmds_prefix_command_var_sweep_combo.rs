//! Strict combo oracle probes, batch 277: callint / cmds CORE variable sweep
//! (this/last-command, prefix-arg, command-history). Any nil-in-Neomacs/t-in-GNU
//! is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_this_last_command_prefix_arg_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'this-command)
      (boundp 'last-command)
      (boundp 'real-this-command)
      (boundp 'real-last-command)
      (boundp 'this-original-command)
      (boundp 'last-repeatable-command)
      (boundp 'prefix-arg)
      (boundp 'current-prefix-arg)
      (boundp 'command-history)
      (boundp 'command-debug-status)
      (boundp 'minibuffer-history-variable)
      (boundp 'extended-command-history))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_disable_command_interprogram_clipboard_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'enable-recursive-minibuffers)
      (boundp 'enable-dir-local-variables)
      (boundp 'enable-local-variables)
      (boundp 'enable-local-eval)
      (boundp 'disabled-command-function)
      (boundp 'disabled-command-hook)
      (boundp 'suggest-key-bindings)
      (boundp 'interprogram-cut-function)
      (boundp 'interprogram-paste-function)
      (boundp 'save-interprogram-paste-before-kill)
      (boundp 'x-select-enable-clipboard)
      (boundp 'x-select-enable-primary))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t nil t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_interactive_history_saved_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'history-length)
      (boundp 'history-delete-duplicates)
      (boundp 'savehist-mode)
      (boundp 'savehist-file)
      (boundp 'savehist-additional-variables)
      (boundp 'savehist-ignored-variables)
      (boundp 'savehist-autosave-interval)
      (boundp 'save-place-mode)
      (boundp 'save-place-file)
      (boundp 'save-place-limit)
      (boundp 'recentf-mode)
      (boundp 'recentf-max-saved-items))
"##;
    let expect = expect_test::expect![[r#""OK (t t t nil nil nil nil t nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

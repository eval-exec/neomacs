//! Strict combo oracle probes, batch 260: input-method / keyboard / key-
//! translation variable existence sweep. Any nil-in-Neomacs/t-in-GNU is a
//! missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_input_method_current_default_verbose_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'current-input-method)
      (boundp 'current-input-method-title)
      (boundp 'default-input-method)
      (boundp 'input-method-verbose-flag)
      (boundp 'input-method-highlight-flag)
      (boundp 'input-method-exit-on-first-char)
      (boundp 'input-method-use-structure-point)
      (boundp 'input-method-function)
      (boundp 'deactivate-current-input-method-function)
      (boundp 'describe-current-input-method-function))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_key_translation_function_key_map_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'key-translation-map)
      (boundp 'function-key-map)
      (boundp 'esc-map)
      (boundp 'input-decode-map)
      (boundp 'local-function-key-map)
      (boundp 'minor-mode-map-alist)
      (boundp 'minor-mode-overriding-map-alist)
      (boundp 'emulation-mode-map-alists)
      (boundp 'where-is-preferred-modifier)
      (boundp 'modifier-key-1)
      (boundp 'character-translations))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_key_binding_overscroll_event_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'overriding-local-map)
      (boundp 'overriding-terminal-local-map)
      (boundp 'pre-command-hook)
      (boundp 'post-command-hook)
      (boundp 'prefix-arg)
      (boundp 'current-prefix-arg)
      (boundp 'last-command)
      (boundp 'this-command)
      (boundp 'real-this-command)
      (boundp 'last-command-event)
      (boundp 'last-input-char)
      (boundp 'menu-bar-final-items))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

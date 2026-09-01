//! Strict combo oracle probes, batch 333: format-message + substitute-command-
//! keys deep. format-message, substitute-command-keys with various key specs,
//! and format-message with %s/%d.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_format_message_substitute_keys_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format-message "Use `%s' to proceed" "M-x")
      (format-message "Type %s now" "C-c")
      (substitute-command-keys "Press \\[forward-char] to move")
      (substitute-command-keys "Use \\[keyboard-quit] to abort")
      (substitute-command-keys "\\[foo] is undefined")
      (substitute-command-keys "Plain text with no keys")
      (format-message "Count: %d items" 42))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"Use ‘M-x’ to proceed\" \"Type C-c now\" #(\"Press C-f to move\" 6 9 (font-lock-face help-key-binding face help-key-binding)) #(\"Use C-g to abort\" 4 7 (font-lock-face help-key-binding face help-key-binding)) #(\"M-x foo is undefined\" 0 7 (font-lock-face help-key-binding face help-key-binding)) \"Plain text with no keys\" \"Count: 42 items\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_substitute_command_keys_literal_faced() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (substitute-command-keys "\\=\\[literal] preserved")
      (substitute-command-keys "Multiple \\[forward-char] and \\[backward-char] keys")
      (length (substitute-command-keys "\\[forward-char]"))
      (> (length (substitute-command-keys "\\[forward-char]")) 0)
      (format-message "Done"))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"\\\\[literal] preserved\" #(\"Multiple C-f and C-b keys\" 9 12 (font-lock-face help-key-binding face help-key-binding) 17 20 (font-lock-face help-key-binding face help-key-binding)) 3 t \"Done\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_message_help_echo_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format-message "Hello %s" "world")
      (format-message "Value: %d, Float: %.2f" 42 3.14)
      (format-message "Mixed %s and %d" "str" 7)
      (substitute-command-keys "\\`backtick test")
      (stringp (documentation 'car))
      (stringp (documentation-property 'car 'function-documentation)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"Hello world\" \"Value: 42, Float: 3.14\" \"Mixed str and 7\" \"\\\\‘backtick test\" t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

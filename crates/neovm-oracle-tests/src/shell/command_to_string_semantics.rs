//! Oracle parity tests for GNU `simple.el` `shell-command-to-string`.
//!
//! GNU implements this helper by binding `standard-output` to a temp buffer
//! and calling `shell-command`, so it must honor Lisp-level `shell-file-name`
//! and `shell-command-switch` dynamic bindings.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_shell_command_to_string_basic_output_and_status() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (shell-command-to-string "printf 'alpha\nbeta\n'")
 (shell-command-to-string "printf 'kept'; exit 7")
 (condition-case err
     (shell-command-to-string 42)
   (error err)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"alpha\\nbeta\\n\" \"kept\" (wrong-type-argument stringp 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_shell_command_to_string_respects_shell_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((shell-file-name "printf")
      (shell-command-switch "neo:%s"))
  (shell-command-to-string "abc"))
"#;

    let expect = expect_test::expect![[r#""OK \"neo:abc\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_shell_command_to_string_uses_current_default_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((default-directory temporary-file-directory))
  (list
   (file-name-as-directory (expand-file-name temporary-file-directory))
   (shell-command-to-string "pwd")))
"#;

    let expect = expect_test::expect![[r#""OK (\"[SESSION-TMPDIR]/\" \"[SESSION-TMPDIR]\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

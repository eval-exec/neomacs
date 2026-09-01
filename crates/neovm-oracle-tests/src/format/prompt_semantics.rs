//! Oracle parity tests for GNU `minibuffer.el` `format-prompt` semantics.
//!
//! `format-prompt` composes prompt text, optional format arguments, list
//! defaults, empty defaults, and `minibuffer-default-prompt-format`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_format_prompt_default_presence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((minibuffer-default-prompt-format " (default %s)"))
  (list
   (format-prompt "Name" "alice")
   (format-prompt "Name" nil)
   (format-prompt "Name" "")
   (format-prompt "Name" 42)
   (format-prompt "Name" '(first second third))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"Name (default alice): \" \"Name: \" \"Name: \" \"Name (default 42): \" \"Name (default first): \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_format_prompt_prompt_format_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((minibuffer-default-prompt-format " [%s]"))
  (list
   (format-prompt "Open %s" "README.md" "file")
   (format-prompt "Replace %s with %s" "new" "old" "new")
   (format-prompt "%s/%s" '("main" "ignored") "branch" "remote")))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"Open file [README.md]: \" \"Replace old with new [new]: \" \"branch/remote [main]: \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_format_prompt_custom_default_prompt_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((minibuffer-default-prompt-format " {%s}"))
   (format-prompt "Project" "neomacs"))
 (let ((minibuffer-default-prompt-format " default=%S"))
   (format-prompt "Value" '(alpha beta)))
 (let ((minibuffer-default-prompt-format ""))
   (format-prompt "No suffix" "x")))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"Project {neomacs}: \" \"Value default=alpha: \" \"No suffix: \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_format_prompt_substitute_command_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((minibuffer-default-prompt-format " (default `%s')"))
  (list
   (format-prompt "Use \\[find-file]" "path")
   (format-prompt "Command `%s'" "M-x" "compile")))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"Use C-x C-f (default ‘path’): \" 4 11 (face help-key-binding font-lock-face help-key-binding)) \"Command ‘compile’ (default ‘M-x’): \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

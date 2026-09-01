//! Oracle parity tests for GNU synchronous shell process wrappers.
//!
//! GNU implements `call-process-shell-command` and
//! `process-file-shell-command` in `lisp/subr.el` by dynamically reading
//! `shell-file-name` and `shell-command-switch`, then passing the command plus
//! legacy rest args joined with spaces to `call-process` / `process-file`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_call_process_shell_command_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((shell-file-name "printf")
      (shell-command-switch "cmd:%s"))
  (with-temp-buffer
    (insert "AA")
    (goto-char 2)
    (list
     (call-process-shell-command "one" nil t nil "two" "three")
     (buffer-string)
     (point))))
"#;

    let expect = expect_test::expect![[r#""OK (0 \"Acmd:one two threeA\" 19)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_process_file_shell_command_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((shell-file-name "printf")
      (shell-command-switch "pf:%s"))
  (with-temp-buffer
    (list
     (process-file-shell-command "alpha" nil t nil "beta")
     (buffer-string)
     (point))))
"#;

    let expect = expect_test::expect![[r#""OK (0 \"pf:alpha beta\" 14)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_call_process_shell_command_mixes_stderr_when_requested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (list
   (call-process-shell-command "printf out; printf err >&2" nil (list t t))
   (buffer-string)))
"#;

    let expect = expect_test::expect![[r#""OK (0 \"outerr\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

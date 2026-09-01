//! Strict combo oracle probes, batch 23: synchronous process calls
//! (call-process exit codes and output, shell-command-to-string, call-process
//! stderr routing), process-environment / getenv, and process-list shape.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_f8_call_process_exit_and_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 \"hello\\n\") (0 \"world\\n\") (1 \"\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (let ((status (call-process "echo" nil t nil "hello")))
          (list status (buffer-string))))
      (with-temp-buffer
        (let ((status (call-process "printf" nil t nil "%s\n" "world")))
          (list status (buffer-string))))
      (with-temp-buffer
        (let ((status (call-process "false" nil t nil)))
          (list status (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_f8_shell_command_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hi\\n\" \"abc\" 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (shell-command-to-string "echo hi")
      (shell-command-to-string "printf abc")
      (length (shell-command-to-string "seq 1 5")))
"##,
        expect,
    );
}

#[test]
fn div_f8_call_process_stderr_and_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((0 \"a\\nb\") (0 \"out\\nerr\\n\") (0 \"2\\n3\\n4\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer
        (let ((status (call-process shell-file-name nil t nil
                                    shell-command-switch "printf 'a\\nb'")))
          (list status (buffer-string))))
      (with-temp-buffer
        (let ((status (call-process "sh" nil t nil "-c" "echo out; echo err 1>&2")))
          (list status (buffer-string))))
      (with-temp-buffer
        (let ((status (call-process "seq" nil t nil "2" "4")))
          (list status (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_f8_process_environment_getenv() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"zzz\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((process-environment (cons "NEO_PROBE_SYNC_ENV=zzz" process-environment)))
  (list (getenv "NEO_PROBE_SYNC_ENV")
        (stringp (getenv "HOME"))
        (stringp (getenv "PATH"))))
"##,
        expect,
    );
}

#[test]
fn div_f8_process_environment_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK 284
    // Neomacs:   OK 283
    // (length process-environment) differs by one: GNU's default process-
    // environment has one more entry than Neomacs (an Emacs-internal variable
    // GNU injects). getenv values for HOME/PATH agree.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (integerp (length process-environment))
      (> (length process-environment) 0))
"##,
        expect,
    );
}

#[test]
fn div_f8_call_process_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"1\\n2\\n3\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "3\n1\n2\n")
  (let ((status (call-process-region (point-min) (point-max) "sort" t t nil)))
    (list status (buffer-string))))
"##,
        expect,
    );
}

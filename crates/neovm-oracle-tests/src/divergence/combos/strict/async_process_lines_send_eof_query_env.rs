//! Strict combo oracle probes, batch 52: more process machinery — process-lines
//! (sync line output), process-send-string/send-eof, process-query-on-exit-flag
//! and delete-process, and the make-process :environment option. Commands use
//! shell-file-name for portability.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_j2_process_lines_sync() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(process-lines shell-file-name shell-command-switch "printf 'a\nb\nc'")
"##,
        expect,
    );
}

#[test]
fn div_j2_process_lines_with_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"single\") 5 (error t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (process-lines shell-file-name shell-command-switch "echo single")
      (length (process-lines shell-file-name shell-command-switch "seq 1 5"))
      (condition-case err
          (process-lines shell-file-name shell-command-switch "exit 7")
        (error
         (list (car err)
               (and (string-match-p "exited with status 7\\'"
                                    (error-message-string err))
                    t)))))
"##,
        expect,
    );
}

#[test]
fn div_j2_process_send_string_and_eof() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable proc)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (generate-new-buffer " *probe-send*"))
       (proc (make-process :name "probe-psend"
                           :command (list shell-file-name shell-command-switch "cat")
                           :buffer buf
                           :sentinel (lambda (&rest _) nil)))))
  (set-process-query-on-exit-flag proc nil)
  (process-send-string proc "hello\n")
  (process-send-eof proc)
  (accept-process-output proc 1)
  (list (with-current-buffer buf (buffer-string))
        (process-status proc)))
"##,
        expect,
    );
}

#[test]
fn div_j2_process_query_on_exit_and_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t signal)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "probe-pqoe"
                          :command (list shell-file-name shell-command-switch "sleep 1"))))
  (list (process-query-on-exit-flag proc)
        (progn (set-process-query-on-exit-flag proc t) (process-query-on-exit-flag proc))
        (progn (delete-process proc) (process-status proc))))
"##,
        expect,
    );
}

#[test]
fn div_j2_process_environment_option() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (generate-new-buffer " *probe-env-out*")))
  (let ((proc (make-process :name "probe-penv"
                            :command (list shell-file-name shell-command-switch "echo $PROBE_ENV_VAR")
                            :buffer buf
                            :environment '("PROBE_ENV_VAR=testvalue")
                            :sentinel #'ignore)))
    (set-process-query-on-exit-flag proc nil)
    (accept-process-output proc 1))
  (with-current-buffer buf (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_j2_process_send_string_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable proc)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (generate-new-buffer " *probe-sr*"))
       (proc (make-process :name "probe-psr"
                           :command (list shell-file-name shell-command-switch "cat")
                           :buffer buf
                           :sentinel (lambda (&rest _) nil)))))
  (set-process-query-on-exit-flag proc nil)
  (list (process-send-string proc "x\n")
        (process-send-eof proc)
        (accept-process-output proc 1)
        (with-current-buffer buf (buffer-string))))
"##,
        expect,
    );
}

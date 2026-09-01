//! Strict combo oracle probes, batch 383: process stderr/stdout separation +
//! call-process-region with coding. call-process with separate stderr,
//! call-process-region pipeline, and process-coding-system on async process.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_call_process_stderr_stdout_split_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((out (generate-new-buffer " *probe-cp-out*"))
      (err (generate-new-buffer " *probe-cp-err*")))
  (unwind-protect
      (let ((code (call-process shell-file-name nil (list out err) nil
                                shell-command-switch
                                "echo to-stdout; echo to-stderr 1>&2")))
        (list code
              (with-current-buffer out (buffer-string))
              (with-current-buffer err (buffer-string))))
    (kill-buffer out)
    (kill-buffer err)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp #<killed buffer>)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_call_process_region_sort_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((out (generate-new-buffer " *probe-cpr-out*")))
  (unwind-protect
      (with-temp-buffer
        (insert "5\n3\n1\n4\n2\n")
        (let ((code (call-process-region (point-min) (point-max)
                                         shell-file-name nil out nil
                                         shell-command-switch "sort -n")))
          (list code
                (with-current-buffer out (buffer-string)))))
    (kill-buffer out)))
"##;
    let expect = expect_test::expect![[r#""OK (0 \"1\\n2\\n3\\n4\\n5\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_process_environment_inherit_env_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-env*"))
       (proc (make-process :name "probe-env"
                           :command (list shell-file-name shell-command-switch "echo $PROBE_ENV_VAR")
                           :buffer buf
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (setenv "PROBE_ENV_VAR" "test-value-42")
  (accept-process-output proc 1)
  (accept-process-output proc 1)
  (let ((result (with-current-buffer buf (buffer-string))))
    (setenv "PROBE_ENV_VAR" nil)
    (kill-buffer buf)
    (list (> (length result) 0)
          (or (string-match "test-value-42" result) result)
          result)))
"##;
    let expect = expect_test::expect![[r#""OK (t \"\\n\" \"\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

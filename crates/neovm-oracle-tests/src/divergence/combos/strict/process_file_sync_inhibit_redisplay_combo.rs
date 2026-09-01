//! Strict combo oracle probes, batch 344: process-file synchronous + inhibit.
//! process-file with output to file, process-file with t (current buffer),
//! and process-file-side-effects query. Shared tempdir.
//! Uses assert_oracle_parity_with_shared_tempdir_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_process_file_output_to_file_sync() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (out (expand-file-name "probe-pf-out.txt" dir)))
  (when (file-exists-p out) (delete-file out))
  (let ((code (process-file shell-file-name nil out nil
                            shell-command-switch "echo pf-sync-result")))
    (prog1
        (list code
              (file-exists-p out)
              (with-temp-buffer
                (insert-file-contents out)
                (buffer-string)))
      (when (file-exists-p out) (delete-file out))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(form, expect);
}

#[test]
fn div_v8_process_file_output_to_buffer_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (generate-new-buffer " *probe-pf-buf*")
  (let ((code (process-file shell-file-name nil t nil
                            shell-command-switch "printf 'buf-out'")))
    (prog1
        (list code
              (buffer-string)
              (= (buffer-size) 7))
      (kill-buffer (current-buffer)))))
"##;
    let expect = expect_test::expect![[r#""OK (0 \"buf-out\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_process_file_exit_code_nonzero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((code1 (process-file shell-file-name nil nil nil
                           shell-command-switch "exit 0"))
      (code2 (process-file shell-file-name nil nil nil
                           shell-command-switch "exit 42")))
  (list code1
        code2
        (or (null code2) (and (integerp code2) (/= code2 0)))))
"##;
    let expect = expect_test::expect![[r#""OK (0 42 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Strict combo oracle probes, batch 160: process machinery. synchronous
//! call-process exit-code + stdout-to-buffer, make-process with custom filter
//! accumulation and suppressed sentinel, process-status/name/buffer identity
//! after output, and process-list membership filtering.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_call_process_synchronous_exit_stdout() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (let ((code (call-process "printf" nil t nil "line1\nline2\nline3\n")))
    (list code
          (buffer-string)
          (count-lines (point-min) (point-max))
          (goto-char (point-min))
          (buffer-substring (line-beginning-position) (line-end-position)))))
"##;
    let expect = expect_test::expect![[r#""OK (0 \"line1\\nline2\\nline3\\n\" 3 1 \"line1\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_make_process_filter_accumulate_status() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-mpf*"))
       (acc (list))
       (proc (make-process :name "probe-mpf"
                           :command (list shell-file-name shell-command-switch "printf 'a\\nb\\nc\\n'")
                           :buffer buf
                           :filter (lambda (p s) (push s acc) (with-current-buffer (process-buffer p) (insert s)))
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (accept-process-output proc 1)
  (accept-process-output proc 1)
  (list (eq (process-buffer proc) buf)
        (process-name proc)
        (processp proc)
        (memq proc (process-list))
        (> (length (with-current-buffer buf (buffer-string))) 0)
        (> (process-id proc) 0)
        (memql (process-status proc) '(run exit))))
"##;
    let expect = expect_test::expect![[r#""OK (t \"probe-mpf\" t nil t t (exit))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_call_process_error_exit_stderr_separate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((stdout (generate-new-buffer " *probe-cpe-out*"))
      (stderr (generate-new-buffer " *probe-cpe-err*")))
  (unwind-protect
      (let ((code (call-process shell-file-name nil (list stdout stderr) nil
                                shell-command-switch "echo to-out; echo to-err 1>&2")))
        (list code
              (with-current-buffer stdout (buffer-string))
              (with-current-buffer stderr (buffer-string))))
    (kill-buffer stdout)
    (kill-buffer stderr)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp #<killed buffer>)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

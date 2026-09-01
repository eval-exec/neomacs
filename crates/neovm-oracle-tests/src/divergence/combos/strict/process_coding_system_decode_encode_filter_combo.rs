//! Strict combo oracle probes, batch 313: process coding-system + filter/
//! sentinel reset deep. set-process-coding-system, process-coding-system,
//! process-decode/encode-coding-system, and set-process-filter/sentinel reset.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_process_coding_system_set_decode_encode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-pcs*"))
       (proc (make-process :name "probe-pcs"
                           :command (list shell-file-name shell-command-switch "echo coded")
                           :buffer buf
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (set-process-coding-system proc 'utf-8 'utf-8)
  (accept-process-output proc 1)
  (list (eq (car (process-coding-system proc)) 'utf-8)
        (process-coding-system proc)
        (process-decode-coding-system proc)
        (process-encode-coding-system proc)
        (kill-buffer buf)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function process-decode-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_process_filter_sentinel_reset_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-pfsr*"))
       (filt (lambda (p s) nil))
       (sent (lambda (&rest _) nil))
       (proc (make-process :name "probe-pfsr"
                           :command (list shell-file-name shell-command-switch "echo reset")
                           :buffer buf
                           :filter filt
                           :sentinel sent)))
  (set-process-query-on-exit-flag proc nil)
  (let ((f1 (eq (process-filter proc) filt))
        (s1 (eq (process-sentinel proc) sent)))
    (set-process-filter proc nil)
    (set-process-sentinel proc nil)
    (list f1 s1
          (null (process-filter proc))
          (null (process-sentinel proc))
          (kill-buffer buf))))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_process_send_string_region_query_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-send*"))
       (proc (make-process :name "probe-send"
                           :command (list shell-file-name shell-command-switch "cat")
                           :buffer buf
                           :connection-type 'pipe
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (process-send-string proc "line1\n")
  (process-send-string proc "line2\n")
  (accept-process-output proc 1)
  (accept-process-output proc 1)
  (let ((out (with-current-buffer buf (buffer-string))))
    (list (> (length out) 0)
          (memql (process-status proc) '(run exit))
          (integerp (process-id proc))
          (kill-buffer buf))))
"##;
    let expect = expect_test::expect![[r#""OK (t (run exit) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

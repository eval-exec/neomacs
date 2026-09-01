//! Strict combo oracle probes, batch 215: process deep. process-plist /
//! set-process-plist, process-get/process-put, process-query-on-exit-flag,
//! process-contact, and process attributes after a short-lived echo command.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_process_plist_get_put_query_on_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-pdp*"))
       (proc (make-process :name "probe-pdp"
                           :command (list shell-file-name shell-command-switch "echo hi")
                           :buffer buf
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (set-process-plist proc '(orig-key orig-val))
  (process-put proc 'new-key 'new-val)
  (accept-process-output proc 1)
  (list (processp proc)
        (eq (process-buffer proc) buf)
        (process-name proc)
        (process-get proc 'orig-key)
        (process-get proc 'new-key)
        (process-query-on-exit-flag proc)
        (plist-get (process-plist proc) 'new-key)
        (assq 'orig-key (process-plist proc))))
"##;
    let expect =
        expect_test::expect![[r#""OK (t t \"probe-pdp\" orig-val new-val nil new-val nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_process_attributes_status_type_tty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-pat*"))
       (proc (make-process :name "probe-pat"
                           :command (list shell-file-name shell-command-switch "echo hello")
                           :buffer buf
                           :connection-type 'pipe
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (let ((status (process-status proc))
        (plist (process-list)))
    (list (processp proc)
          (integerp (process-id proc))
          (eq (process-buffer proc) buf)
          (process-type proc)
          (eq (process-contact proc :local) nil)
          (not (null (memq status '(run exit signal))))
          (if (memq status '(exit signal))
              (not (memq proc plist))
            (not (null (memq proc plist)))))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t real nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_process_kill_then_check_buffer_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-pkl*"))
       (proc (make-process :name "probe-pkl"
                           :command (list shell-file-name shell-command-switch "printf 'line\\n'")
                           :buffer buf
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (accept-process-output proc 1)
  (let ((output (with-current-buffer buf (buffer-string))))
    (delete-process proc)
    (list (> (length output) 0)
          output
          (process-live-p proc)
          (eq (process-buffer proc) buf))))
"##;
    let expect = expect_test::expect![[r#""OK (t \"line\\n\" nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

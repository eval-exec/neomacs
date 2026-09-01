//! Strict combo oracle probes, batch 302: process-attributes. process-
//! attributes returns an alist (comm, ppid, user, etc.) for a process; we
//! compare the deterministic shape (keys present) rather than PID-dependent
//! values. process-list filtering too.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_process_attributes_keys_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-attr*"))
       (proc (make-process :name "probe-attr"
                           :command (list shell-file-name shell-command-switch "echo attr-done")
                           :buffer buf
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (let ((attrs (process-attributes proc)))
    (list (consp attrs)
          (assq 'comm attrs)
          (assq 'ppid attrs)
          (assq 'user attrs)
          (assq 'pid attrs)
          (assq 'etime attrs)
          (kill-buffer buf))))
"##;
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument numberp #<process probe-attr>)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_process_list_filter_live_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-plist*"))
       (proc (make-process :name "probe-plist"
                           :command (list shell-file-name shell-command-switch "echo x")
                           :buffer buf
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (let ((i 0))
    (while (and (< i 20) (memq proc (process-list)))
      (accept-process-output proc 0.05)
      (setq i (1+ i))))
  (let ((in-list (memq proc (process-list))))
    (list (consp in-list)
          (processp proc)
          (integerp (process-id proc))
          (kill-buffer buf))))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_process_status_type_mark_tty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-pstat*"))
       (proc (make-process :name "probe-pstat"
                           :command (list shell-file-name shell-command-switch "printf done")
                           :buffer buf
                           :connection-type 'pipe
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (accept-process-output proc 1)
  (let ((result (list (process-type proc)
                      (eq (process-mark proc) (process-mark proc))
                      (markerp (process-mark proc))
                      (integerp (marker-position (process-mark proc))))))
    (kill-buffer buf)
    result))
"##;
    let expect = expect_test::expect![[r#""OK (real t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

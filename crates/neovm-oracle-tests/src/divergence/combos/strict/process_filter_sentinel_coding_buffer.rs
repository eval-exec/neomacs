//! Strict combo oracle probes, batch 321: multi-subsystem combo -- process +
//! filter + sentinel + coding-system + buffer-local, all interacting.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_combo_process_filter_coding_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-combo-proc*"))
       (acc nil)
       (sent-fired 0)
       (proc (make-process :name "combo-proc"
                           :command (list shell-file-name shell-command-switch "echo combo-data-123")
                           :buffer buf
                           :filter (lambda (p s)
                                     (push s acc)
                                     (with-current-buffer (process-buffer p) (insert s)))
                           :sentinel (lambda (&rest _) (setq sent-fired (1+ sent-fired))))))
  (set-process-query-on-exit-flag proc nil)
  (set-process-coding-system proc 'utf-8 'utf-8)
  (let ((coding-before (eq (car (process-coding-system proc)) 'utf-8)))
    (with-current-buffer buf
      (setq-local probe-combo-proc-var 'buf-local))
    (accept-process-output proc 1)
    (accept-process-output proc 1)
    (list (processp proc)
          (> (length acc) 0)
          coding-before
          (with-current-buffer buf (> (buffer-size) 0))
          (with-current-buffer buf probe-combo-proc-var)
          (buffer-live-p buf)
          (kill-buffer buf))))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t buf-local t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_combo_call_process_region_call_process_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((out-buf (generate-new-buffer " *probe-combo-call-out*"))
       (region-text "5\n3\n1\n4\n2"))
  (with-temp-buffer
    (insert region-text)
    (let ((code (call-process-region (point-min) (point-max)
                                     shell-file-name nil out-buf nil
                                     shell-command-switch "sort")))
      (let ((sorted (with-current-buffer out-buf (buffer-string))))
        (prog1
            (list code
                  sorted
                  (with-current-buffer out-buf (count-lines (point-min) (point-max))))
          (kill-buffer out-buf)))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_combo_async_process_kill_buffer_marker_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((buf (generate-new-buffer " *probe-combo-kill*"))
       (proc (make-process :name "combo-kill"
                           :command (list shell-file-name shell-command-switch "sleep 0.05; echo done")
                           :buffer buf
                           :sentinel (lambda (&rest _) nil))))
  (set-process-query-on-exit-flag proc nil)
  (let ((m (with-current-buffer buf (set-marker (make-marker) 1))))
    (accept-process-output proc 1)
    (let ((live-before (buffer-live-p buf))
          (proc-live (process-live-p proc)))
      (kill-buffer buf)
      (list live-before proc-live
            (buffer-live-p buf)
            (eq (process-buffer proc) buf)
            (process-live-p proc)))))
"##;
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm(form);
    let exited_before_kill = "OK (t nil nil t nil)";
    let live_through_kill =
        "OK (t (run open listen connect stop) nil t (run open listen connect stop))";
    let live_before_kill = "OK (t (run open listen connect stop) nil t nil)";
    // GNU can observe the short subprocess status before or after the single
    // `accept-process-output' call returns.  The stable semantics here are
    // that the buffer starts live, `kill-buffer' kills it, and the process
    // object still records the killed buffer as its process buffer.
    for (label, value) in [("GNU", oracle.as_str()), ("Neomacs", neovm.as_str())] {
        assert!(
            value == exited_before_kill || value == live_through_kill || value == live_before_kill,
            "{label} returned unexpected process/buffer state: {value:?}"
        );
    }
}

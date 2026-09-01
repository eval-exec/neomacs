//! Strict combo oracle probes, batch 50: async process machinery — process
//! output via filter, process-plist/get/put, process-buffer/mark, connection
//! type (pipe/pty) and process-contact, and stderr routing to a separate
//! buffer. Commands use shell-file-name + shell-command-switch (builtins) so
//! they are portable across systems where /bin/echo and /bin/true do not exist
//! (e.g. NixOS).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_j0_process_output_via_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello world\\n\" run 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (collected)
  (let ((proc (make-process :name "probe-proc-out"
                            :command (list shell-file-name shell-command-switch
                                           "printf '%s\\n' 'hello world'; sleep 0.2")
                            :connection-type 'pipe
                            :filter (lambda (_p s) (setq collected (concat collected s))))))
    (set-process-query-on-exit-flag proc nil)
    (accept-process-output proc 1)
    (prog1 (list collected
                 (process-status proc)
                 (process-exit-status proc)
                 (process-buffer proc))
      (delete-process proc))))
"##,
        expect,
    );
}

#[test]
fn div_j0_process_plist_get_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 nil 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "probe-proc-pl"
                          :command (list shell-file-name shell-command-switch "true"))))
  (set-process-query-on-exit-flag proc nil)
  (process-put proc 'probe-prop 42)
  (list (process-get proc 'probe-prop)
        (process-get proc 'missing)
        (plist-get (process-plist proc) 'probe-prop)))
"##,
        expect,
    );
}

#[test]
fn div_j0_process_buffer_and_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 4 \" *probe-proc-buf*\" \"hi\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (generate-new-buffer " *probe-proc-buf*")))
  (let ((proc (make-process :name "probe-proc-bm"
                            :command (list shell-file-name shell-command-switch
                                           "printf '%s\\n' hi; sleep 0.2")
                            :buffer buf
                            :connection-type 'pipe)))
    (set-process-query-on-exit-flag proc nil)
    (accept-process-output proc 1)
    (prog1 (list (eq (process-buffer proc) buf)
                 (marker-position (process-mark proc))
                 (buffer-name (process-buffer proc))
                 (with-current-buffer buf (buffer-string)))
      (delete-process proc)
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_j0_process_connection_type_and_contact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p1 (make-process :name "probe-pipe"
                        :command (list shell-file-name shell-command-switch "true")
                        :connection-type 'pipe))
      (p2 (make-process :name "probe-pty"
                        :command (list shell-file-name shell-command-switch "true")
                        :connection-type 'pty)))
  (set-process-query-on-exit-flag p1 nil)
  (set-process-query-on-exit-flag p2 nil)
  (list (process-type p1)
        (process-type p2)
        (car (process-contact p1))
        (car (process-contact p2))))
"##,
        expect,
    );
}

#[test]
fn div_j0_process_stderr_separate_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"out\\n\" \"err\\n\")""#]];
    // stdout and stderr are separate process objects. Wait for both and ignore
    // their sentinels so buffer contents do not depend on pipe scheduling.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((outbuf (generate-new-buffer " *probe-stderr-out*"))
      (errbuf (generate-new-buffer " *probe-stderr-err*")))
  (let ((proc (make-process :name "probe-stderr"
                            :command (list shell-file-name shell-command-switch "echo out; echo err 1>&2")
                            :buffer outbuf
                            :stderr errbuf)))
    (set-process-query-on-exit-flag proc nil)
    (set-process-sentinel proc #'ignore)
    (let ((stderr-proc (get-buffer-process errbuf)))
      (when stderr-proc
        (set-process-query-on-exit-flag stderr-proc nil)
        (set-process-sentinel stderr-proc #'ignore))
      (let ((attempts 0))
        (while (and (or (process-live-p proc)
                        (and stderr-proc (process-live-p stderr-proc)))
                    (< attempts 100))
          (accept-process-output nil 0.05)
          (setq attempts (1+ attempts)))
        (when (or (process-live-p proc)
                  (and stderr-proc (process-live-p stderr-proc)))
          (error "process did not exit")))
      (let ((drains 0))
        (while (and (< drains 100)
                    (accept-process-output nil 0.01))
          (setq drains (1+ drains)))))
    (list (with-current-buffer outbuf (buffer-string))
          (with-current-buffer errbuf (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_j0_process_name_pid_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"probe-proc-np\" t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "probe-proc-np"
                          :command (list shell-file-name shell-command-switch "true"))))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (list (process-name proc)
        (integerp (process-id proc))
        (> (length (process-list)) 0)
        (memq proc (process-list))))
"##,
        expect,
    );
}

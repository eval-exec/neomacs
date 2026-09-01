//! Strict combo oracle probes, batch 51: more async-process machinery —
//! set/get process-filter and process-sentinel, process-coding-system,
//! make-pipe-process + process-send-string, and process-multibyte-p.
//! Commands use shell-file-name for portability.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_j1_process_filter_set_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable f)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((called 0)
      (f (lambda (_p _s) (setq called (1+ called))))
      (proc (make-process :name "probe-pf"
                          :command (list shell-file-name shell-command-switch "echo hi")
                          :filter f)))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (list (eq (process-filter proc) f)
        (functionp (process-filter proc))
        called))
"##,
        expect,
    );
}

#[test]
fn div_j1_process_sentinel_set_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    // process-sentinel accessor only (set-process-sentinel + process-sentinel
    // eq/functionp); the async sentinel FIRING timing is not asserted here to
    // avoid flakiness.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((sent (lambda (&rest _) nil))
      (proc (make-process :name "probe-ps"
                          :command (list shell-file-name shell-command-switch "true"))))
  (set-process-sentinel proc sent)
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (list (eq (process-sentinel proc) sent)
        (functionp (process-sentinel proc))))
"##,
        expect,
    );
}

#[test]
fn div_j1_process_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((utf-8-unix . utf-8-unix))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "probe-pc"
                          :command (list shell-file-name shell-command-switch "true"))))
  (set-process-coding-system proc 'utf-8-unix 'utf-8-unix)
  (list (process-coding-system proc)))
"##,
        expect,
    );
}

#[test]
fn div_j1_make_pipe_process_send() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (open (open listen connect stop) \"\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((buf (generate-new-buffer " *probe-pipe*"))
       (pipe (make-pipe-process :name "probe-pipe" :buffer buf)))
  (process-send-string pipe "data line\n")
  (list (process-status pipe)
        (process-live-p pipe)
        (with-current-buffer buf (buffer-string))
        (eq (process-buffer pipe) buf)))
"##,
        expect,
    );
}

#[test]
fn div_j1_process_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function process-multibyte-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "probe-pm"
                          :command (list shell-file-name shell-command-switch "true"))))
  (list (process-multibyte-p proc)
        (progn (set-process-multibyte proc t) (process-multibyte-p proc))))
"##,
        expect,
    );
}

#[test]
fn div_j1_process_status_and_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Process probe-prc is not active\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "probe-prc"
                          :command (list shell-file-name shell-command-switch "sleep 0.2"))))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 1)
  (list (process-type proc)
        (process-running-child-p proc)
        (process-status proc)))
"##,
        expect,
    );
}

#[test]
fn div_j1_accept_process_output_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil exit)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (nil exit)
    // Neomacs:   OK (t exit)
    // accept-process-output on a process that produced NO output returns nil
    // in GNU Emacs (no output read) but t in Neomacs. (Timing-sensitive, but
    // reproducible for a no-output "sleep" command.)
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((proc (make-process :name "probe-apo"
                          :command (list shell-file-name shell-command-switch "sleep 0.2"))))
  (set-process-query-on-exit-flag proc nil)
  (list (accept-process-output proc 1)
        (process-status proc)))
"##,
        expect,
    );
}

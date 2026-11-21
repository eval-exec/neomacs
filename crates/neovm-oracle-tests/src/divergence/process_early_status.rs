//! Early child status after a pure spin.
//!
//! GNU's SIGCHLD handler reaps the child and records its status, so
//! `process-status` reports `exit` with nobody having waited. This port
//! records the status only at a wait, so without an observation-side reap it
//! reports `run` until the first `accept-process-output`.
//!
//! The spin uses no waiting primitive on purpose: `sleep-for`/`sit-for` run
//! the wait and read process output, which defeats the probe.

use crate::common::assert_oracle_parity_expect;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn process_status_after_a_pure_spin_reaches_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"early=exit late=exit buffer=\\\"hello-token<NL><NL>Process neo-pty-probe2 finished<NL>\\\"\"""#
    ]];
    assert_oracle_parity_expect(
        r##"(let* ((buf (get-buffer-create " *neo-pty-probe2*"))
       (proc (make-process :name "neo-pty-probe2" :buffer buf
                           :connection-type 'pty
                           :command (list "echo" "hello-token"))))
  (unwind-protect
      (progn
        (set-process-query-on-exit-flag proc nil)
        (let ((end (+ (float-time) 0.3)))
          (while (< (float-time) end)))
        (format "early=%S late=%S buffer=%S"
                (process-status proc)
                (progn (accept-process-output proc 1)
                       (accept-process-output nil 0.3)
                       (process-status proc))
                (replace-regexp-in-string "\r?\n" "<NL>"
                                          (with-current-buffer buf (buffer-string)))))
    (when (buffer-live-p buf) (kill-buffer buf))))"##,
        expect,
    );
}

#[test]
fn process_status_change_during_a_wait_is_notified_before_it_returns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"status=exit sentinel-inside=t\""#]];
    assert_oracle_parity_expect(
        r##"(let ((observed 'never))
  (let ((p (make-process :name "pwdet" :noquery t :buffer nil
                         :connection-type 'pipe
                         :command '("sh" "-c" "sleep 0.05; exit 0")
                         :sentinel (lambda (_p _m) (setq observed last-command)))))
    (let ((last-command 'pw-inside-the-let))
      (accept-process-output p 2)
      (format "status=%S sentinel-inside=%S" (process-status p)
              (eq observed 'pw-inside-the-let)))))"##,
        expect,
    );
}

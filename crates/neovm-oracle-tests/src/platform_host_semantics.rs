//! Platform-defined answers that GNU propagates from the host C library and
//! process API, checked against GNU's answer on whichever host runs the suite.
//!
//! Snapshot mode selects the compile-time host's record; live modes select the
//! attested GNU reference's record through the typed `host_expect!` helper.
//! The answers must not be pinned unconditionally in the engine.  See
//! `arithmetic::builtin_div` (GNU `Fquo`, src/data.c) and
//! `process::builtins::process_tty_foreground_group` (GNU
//! `emacs_get_tty_pgrp`, src/process.c:7003).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// GNU divides in C and prints the sign bit of the NaN the hardware produced.
/// x86's invalid-operation NaN is negative (`-0.0e+NaN`); arm64's is positive
/// (`0.0e+NaN`).  A NaN *input* keeps its own sign on both.
#[test]
fn oracle_float_invalid_nan_sign_follows_the_host_hardware() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = crate::common::host_expect!(
        LinuxGnu => r#""OK (\"-0.0e+NaN\" \"1.0e+INF\" \"-1.0e+INF\" \"0.0e+NaN\" \"0.0e+NaN\")""#,
        MacosDarwin => r#""OK (\"0.0e+NaN\" \"1.0e+INF\" \"-1.0e+INF\" \"0.0e+NaN\" \"0.0e+NaN\")""#,
    );
    crate::common::assert_oracle_parity_expect(
        r#"(list (number-to-string (/ 0.0 0.0))
                 (number-to-string (/ 10.0 0.0))
                 (number-to-string (/ -10.0 0.0))
                 (number-to-string (/ 0.0e+NaN 2.0))
                 (number-to-string (/ 2.0 0.0e+NaN)))"#,
        expect,
    );
}

/// GNU's `process-running-child-p` probes `p->infd` with `TIOCGPGRP`.  On a
/// pipe the platform decides the answer: Linux fails with ENOTTY and GNU
/// returns `t`; Darwin succeeds with process group 0 and GNU returns `0`.
#[test]
fn oracle_process_running_child_p_matches_the_host_pipe_probe() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = crate::common::host_expect!(
        LinuxGnu => r#""OK (t t)""#,
        MacosDarwin => r#""OK (t 0)""#,
    );
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((ready nil)
                  (proc (make-process
                         :name "oracle-running-child-probe"
                         :command '("/bin/sh" "-c" "echo ready; read line")
                         :connection-type 'pipe
                         :filter (lambda (_proc out)
                                   (when (string-match-p "ready" out)
                                     (setq ready t))))))
             (unwind-protect
                 (let ((deadline (+ (float-time) 10)))
                   (while (and (not ready) (< (float-time) deadline))
                     (accept-process-output proc 0.05))
                   (list ready (process-running-child-p proc)))
               (when (process-live-p proc)
                 (delete-process proc))))"#,
        expect,
    );
}

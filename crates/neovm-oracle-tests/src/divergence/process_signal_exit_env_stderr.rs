//! Process/subprocess signal, exit-status, environment, and stderr oracle
//! parity coverage.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_signal_process_symbolic_sigkill() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (signal 9 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((msg nil) (proc (start-process "neo-ip-xxx" nil "sleep" "30")))
  (set-process-query-on-exit-flag proc nil)
  (set-process-sentinel proc (lambda (_p e) (setq msg e)))
  (signal-process proc 'SIGKILL)
  (while (process-live-p proc) (accept-process-output proc 0.1))
  (while (null msg) (accept-process-output proc 0.05))
  (list (process-status proc) (process-exit-status proc) (string-match "killed" msg)))"##,
        expect,
    );
}

#[test]
fn divergence_signal_process_symbolic_sigterm() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (signal 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-ses-xxx" nil "sleep" "30")))
  (set-process-query-on-exit-flag proc nil)
  (signal-process proc 'SIGTERM)
  (while (process-live-p proc) (accept-process-output proc 0.1))
  (list (process-status proc) (process-exit-status proc)))"##,
        expect,
    );
}

#[test]
fn divergence_process_exit_status_nonzero_collapses() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-es-xxx" nil "sh" "-c" "exit 42")))
  (set-process-query-on-exit-flag proc nil)
  (while (process-live-p proc) (accept-process-output proc 1))
  (process-exit-status proc))"##,
        expect,
    );
}

#[test]
fn divergence_setenv_not_exported_to_subprocess() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"value42\" \"value42\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((process-environment (copy-sequence process-environment)))
  (setenv "NEO_TEST_VAR_XYZ" "value42")
  (list (getenv "NEO_TEST_VAR_XYZ")
        (shell-command-to-string "printf %s \"$NEO_TEST_VAR_XYZ\"")))"##,
        expect,
    );
}

#[test]
fn divergence_make_process_stderr_buffer_ignored() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"OUT\\n\\nProcess neo-se2-xxx finished\\n\" \"ERR\\n\\nProcess neo-se2-xxx stderr finished\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((obuf (generate-new-buffer " neo-o2-xxx")) (ebuf (generate-new-buffer " neo-e2-xxx")))
  (let ((p (make-process :name "neo-se2-xxx"
            :command '("sh" "-c" "echo OUT; echo ERR 1>&2")
            :buffer obuf :stderr ebuf :noquery t)))
    (while (process-live-p p) (accept-process-output p 0.1))
    (sit-for 0.2)
    (list (with-current-buffer obuf (buffer-string))
          (with-current-buffer ebuf (buffer-string)))))"##,
        expect,
    );
}

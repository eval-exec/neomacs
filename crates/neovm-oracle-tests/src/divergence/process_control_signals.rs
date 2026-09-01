//! Process control / signal parity: kill/interrupt/signal-process,
//! process-running-child-p, list-system-processes, process-attributes,
//! plus the signal-0 no-op divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn proc_kill_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (signal (9 15))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-kp-xxx" nil "sleep" "30")))
  (set-process-query-on-exit-flag proc nil)
  (kill-process proc)
  (while (process-live-p proc) (accept-process-output proc 0.1))
  (list (process-status proc) (memq (process-exit-status proc) '(9 15))))"##,
        expect,
    );
}

#[test]
fn proc_signal_process_numeric() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (signal 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-sn9-xxx" nil "sleep" "30")))
  (set-process-query-on-exit-flag proc nil)
  (signal-process proc 9)
  (while (process-live-p proc) (accept-process-output proc 0.1))
  (list (process-status proc) (process-exit-status proc)))"##,
        expect,
    );
}

#[test]
fn proc_running_child_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ok""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-rc-xxx" nil "sleep" "30")))
  (set-process-query-on-exit-flag proc nil)
  (prog1 (condition-case e (progn (process-running-child-p proc) 'ok) (error (car e)))
    (delete-process proc)))"##,
        expect,
    );
}

#[test]
fn proc_list_system_processes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ps (list-system-processes)))
  (list (listp ps)
        (> (length ps) 0)
        (let ((all-integers t))
          (dolist (pid ps all-integers)
            (unless (integerp pid)
              (setq all-integers nil))))))"##,
        expect,
    );
}

#[test]
fn proc_attributes_self() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((attrs (process-attributes (emacs-pid))))
  (list (listp attrs) (stringp (cdr (assq 'comm attrs))) (integerp (cdr (assq 'ppid attrs)))))"##,
        expect,
    );
}

#[test]
fn divergence_signal_process_signal0_noop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (before (run open listen connect stop) ret 0 after (run open listen connect stop) status run)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-s0-xxx" nil "sleep" "30")))
  (set-process-query-on-exit-flag proc nil)
  (sit-for 0.1)
  (let ((before (process-live-p proc))
        (ret (signal-process proc 0))
        (after (process-live-p proc))
        (status (process-status proc)))
    (delete-process proc)
    (list 'before before 'ret ret 'after after 'status status)))"##,
        expect,
    );
}

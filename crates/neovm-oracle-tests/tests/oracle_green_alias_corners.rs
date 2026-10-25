//! GNU 31.1 oracles for alias target corners in Green mode.
//! Refresh only via NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1 in the PID sandbox.

use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;
const GREEN: &[(&str, &str)] = &[("NEOVM_THREADS", "green")];

#[test]
fn forwarded_integer_alias_accepts_a_valid_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"
(progn
 (defvar ac-source 'source-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (worker nil)
        (old gc-cons-threshold))
   (setq gc-cons-threshold 800000)
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (let ((ac-source 987654))
              (mutex-lock m)
              (setq ready t) (condition-notify cv)
              (while (not go) (condition-wait cv))
              (mutex-unlock m)
              (list ac-source gc-cons-threshold)))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 'gc-cons-threshold)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (prog1 (list (thread-join worker) gc-cons-threshold (thread-last-error t))
   (setq gc-cons-threshold old))))
"##,
        &[GREEN],
        expect_test::expect![[r#""OK ((987654 987654) 800000 nil)""#]],
    );
}

#[test]
fn forwarded_boolean_alias_normalizes_a_truthy_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"
(progn
 (defvar ac-source 'source-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (worker nil)
        (old auto-save-no-message))
   (setq auto-save-no-message nil)
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (let ((ac-source 'truthy))
              (mutex-lock m)
              (setq ready t) (condition-notify cv)
              (while (not go) (condition-wait cv))
              (mutex-unlock m)
              (list ac-source auto-save-no-message)))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 'auto-save-no-message)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (prog1 (list (thread-join worker) auto-save-no-message (thread-last-error t))
   (setq auto-save-no-message old))))
"##,
        &[GREEN],
        expect_test::expect![[r#""OK ((t t) nil nil)""#]],
    );
}

#[test]
fn forwarded_alias_reports_its_native_type_error_on_resume() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"
(progn
 (defvar ac-source 'source-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (worker nil)
        (old gc-cons-threshold))
   (setq gc-cons-threshold 800000)
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (let ((ac-source 'not-an-integer))
              (mutex-lock m)
              (setq ready t) (condition-notify cv)
              (while (not go) (condition-wait cv))
              (mutex-unlock m)
              (list ac-source gc-cons-threshold)))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 'gc-cons-threshold)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (prog1 (list (thread-join worker) gc-cons-threshold (thread-last-error t))
   (setq gc-cons-threshold old))))
"##,
        &[GREEN],
        expect_test::expect![[
            r#""OK (nil 800000 (wrong-type-argument integerp not-an-integer))""#
        ]],
    );
}

#[test]
fn an_alias_retargeted_by_its_running_owner_keeps_gnu_saved_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"
(progn
 (defvar ac-source 'source-global)
 (defvar ac-first 'first-global)
 (defvar ac-second 'second-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (worker nil))
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (let ((ac-source 'bound))
              (mutex-lock m)
              (setq ready t) (condition-notify cv)
              (while (not go) (condition-wait cv))
              (mutex-unlock m)
              (let ((before (list ac-source ac-first ac-second)))
                (defvaralias 'ac-first 'ac-second)
                (list before (list ac-source ac-first ac-second)
                      (thread-join (make-thread (lambda () (list ac-source ac-first ac-second))))
                      (list ac-source ac-first ac-second)))))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 'ac-first)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (list (thread-join worker) ac-source ac-first ac-second)))
"##,
        &[GREEN],
        expect_test::expect![[
            r#""OK (((bound bound second-global) (second-global second-global second-global) (first-global first-global first-global) (second-global second-global second-global)) first-global first-global first-global)""#
        ]],
    );
}

// GNU reacquires without re-exchanging when no different Lisp thread ran.
// A variant joining a newly created peer after catching the first constant
// error crashes the pinned GNU31.1 process (three sandbox runs, exit139),
// so it supplies no normal Lisp answer to assert. Its bounded reproducer
// and crash evidence are recorded in tmp/3e-oracles-turn2/results.json.
#[test]
fn a_caught_constant_alias_error_allows_repeated_yields_without_a_peer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"
(progn
 (defvar ac-source 'source-global)
 (let* ((m (make-mutex)) (cv (make-condition-variable m))
        (ready nil) (go nil) (log nil) (worker nil))
   (mutex-lock m)
   (setq worker
         (make-thread
          (lambda ()
            (condition-case outer
                (let ((ac-source 'bound))
                  (mutex-lock m)
                  (setq ready t) (condition-notify cv)
                  (condition-case first
                      (progn (while (not go) (condition-wait cv)) (push 'no-first log))
                    (error (push (list 'first first) log)))
                  (mutex-unlock m)
                  (condition-case second
                      (dotimes (_ 3) (push (list 'yielded (thread-yield)) log))
                    (error (push (list 'second second) log)))
                  (push 'after-second log))
              (error (push (list 'outer outer) log)))
            (nreverse log))))
   (while (not ready) (condition-wait cv))
   (defvaralias 'ac-source 't)
   (setq go t) (condition-notify cv) (mutex-unlock m)
   (list (thread-join worker) (thread-last-error t))))
"##,
        &[GREEN],
        expect_test::expect![[
            r#""OK (((first (setting-constant ac-source)) (yielded nil) (yielded nil) (yielded nil) after-second (outer (setting-constant ac-source))) nil)""#
        ]],
    );
}

#[test]
fn a_caught_constant_alias_failure_repeats_across_real_handoffs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"
(progn
 (defvar ac-source 'source-global)
 (progn
   (defvar ac-m) (defvar ac-cv) (defvar ac-sm) (defvar ac-scv)
   (defvar ac-scout-ready) (defvar ac-scout-go) (defvar ac-scout)
   (defvar ac-ready) (defvar ac-go) (defvar ac-log) (defvar ac-worker)
   (setq ac-m (make-mutex) ac-cv (make-condition-variable ac-m)
         ac-sm (make-mutex) ac-scv (make-condition-variable ac-sm)
         ac-scout-ready nil ac-scout-go nil ac-scout nil
         ac-ready nil ac-go nil ac-log nil ac-worker nil)
   (mutex-lock ac-sm)
   (setq ac-scout (make-thread
                (lambda ()
                  (condition-case caught
                      (progn
                        (mutex-lock ac-sm)
                        (setq ac-scout-ready t) (condition-notify ac-scv)
                        (while (not ac-scout-go) (condition-wait ac-scv))
                        (mutex-unlock ac-sm)
                        'scout-normal)
                    (error (list 'ac-scout caught
                                 (condition-case unlock (mutex-unlock ac-sm) (error unlock))))))))
   (while (not ac-scout-ready) (condition-wait ac-scv))
   (mutex-unlock ac-sm)
   (mutex-lock ac-m)
   (setq ac-worker
         (make-thread
          (lambda ()
            (condition-case outer
                (let ((ac-source 'bound))
                  (mutex-lock ac-m)
                  (setq ac-ready t) (condition-notify ac-cv)
                  (condition-case first
                      (progn (while (not ac-go) (condition-wait ac-cv)) (push 'no-first ac-log))
                    (error (push (list 'first first) ac-log)))
                  (mutex-unlock ac-m)
                  (mutex-lock ac-sm) (setq ac-scout-go t) (condition-notify ac-scv) (mutex-unlock ac-sm)
                  (condition-case second
                      (progn (while (thread-live-p ac-scout) (thread-yield))
                             (push 'yielded ac-log))
                    (error (push (list 'second second) ac-log)))
                  (push 'after-second ac-log))
              (error (push (list 'outer outer) ac-log)))
            (list (nreverse ac-log) (thread-join ac-scout)))))
   (while (not ac-ready) (condition-wait ac-cv))
   (defvaralias 'ac-source 't)
   (setq ac-go t) (condition-notify ac-cv) (mutex-unlock ac-m)
   (list (thread-join ac-worker) (thread-last-error t))))
"##,
        &[GREEN],
        expect_test::expect![[
            r#""OK ((((first (setting-constant ac-source)) (second (setting-constant ac-source)) after-second (outer (setting-constant ac-source))) (ac-scout (setting-constant ac-source) nil)) nil)""#
        ]],
    );
}

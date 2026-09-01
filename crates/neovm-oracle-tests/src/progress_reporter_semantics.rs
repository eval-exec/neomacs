//! Oracle parity tests for GNU `subr.el` progress reporter semantics.
//!
//! The reporter object is intentionally a cons plus parameter vector in GNU
//! Emacs.  These tests observe the public hook states and accessors without
//! depending on echo-area display.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_progress_reporter_numeric_updates_and_throttling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (events)
  (let* ((progress-reporter-update-functions
          (list (lambda (reporter state)
                  (push (list state
                              (progress-reporter-text reporter)
                              (progress-reporter-context reporter)
                              (car reporter)
                              (aref (cdr reporter) 6))
                        events))))
         (reporter (make-progress-reporter "Work" 0 10 nil 10 0 'async)))
    (progress-reporter-update reporter 1 " one")
    (progress-reporter-update reporter 1 " duplicate")
    (progress-reporter-update reporter 2)
    (progress-reporter-force-update reporter 5 "Changed..." " forced")
    (progress-reporter-done reporter)
    (nreverse events)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((0.0 \"Work...\" async 1 nil) (0.1 \"Work...\" async 2 \" one\") (0.2 \"Work...\" async 3 \" one\") (0.5 \"Changed...\" async 6 \" forced\") (done \"Changed...\" async 6 \" forced\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_progress_reporter_pulse_updates_and_suffix_memory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (events)
  (let* ((progress-reporter-update-functions
          (list (lambda (reporter state)
                  (push (list state
                              (progress-reporter-text reporter)
                              (car reporter)
                              (aref (cdr reporter) 6))
                        events))))
         (reporter (make-progress-reporter "Pulse" nil nil nil nil 0)))
    (progress-reporter-update reporter nil " a")
    (progress-reporter-update reporter " legacy-value")
    (progress-reporter-update reporter)
    (progress-reporter-force-update reporter nil "Pulse changed" " forced")
    (progress-reporter-done reporter)
    (nreverse events)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((1 \"Pulse...\" 1 nil) (2 \"Pulse...\" 2 \" a\") (3 \"Pulse...\" 3 \" legacy-value\") (0 \"Pulse...\" 0 \" legacy-value\") (1 \"Pulse changed\" 1 \" forced\") (done \"Pulse changed\" 1 \" forced\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_progress_reporter_message_and_alias_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (events)
  (let* ((progress-reporter-update-functions
          (list (lambda (reporter state)
                  (push (list state
                              (progress-reporter-text reporter)
                              (progress-reporter-context reporter))
                        events))))
         (r1 (make-progress-reporter "Compile" 0 100 25 1 0))
         (r2 (progress-reporter-make "Already..." 0 1 0 nil 0 'async)))
    (list
     (eq (symbol-function 'progress-reporter-make)
         (symbol-function 'make-progress-reporter))
     (progress-reporter-text r1)
     (progress-reporter-text r2)
     (progress-reporter-context r1)
     (progress-reporter-context r2)
     (nreverse events))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil \"Compile...\" \"Already...\" nil async ((0.25 \"Compile...\" nil) (0.0 \"Already...\" async)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_progress_reporter_loop_macros_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (events)
  (let ((progress-reporter-update-functions
         (list (lambda (reporter state)
                 (push (list state (progress-reporter-text reporter))
                       events)))))
    (list
     (dotimes-with-progress-reporter (i 3 'dotimes-result) "Loop"
       i)
     (let ((sum 0))
       (dolist-with-progress-reporter (x '(1 2 3) sum) "List loop"
         (setq sum (+ sum x))))
     (nreverse events))))
"#;

    let expect = expect_test::expect![[
        r#""OK (dotimes-result 6 ((0.0 \"Loop...\") (done \"Loop...\") (0.0 \"List loop...\") (done \"List loop...\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

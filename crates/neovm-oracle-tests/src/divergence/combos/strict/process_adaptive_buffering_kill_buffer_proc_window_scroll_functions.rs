//! Strict combo oracle probes, batch 131: process adaptive read buffering,
//! kill-buffer-query-functions interaction, window-scroll-functions hook,
//! before/after-make-frame-hooks, and timer-list ordering after multiple
//! create/cancel cycles.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u5_process_adaptive_read_buffering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((proc (make-process :name "probe-adaptive"
                          :command (list shell-file-name shell-command-switch "true"))))
  (set-process-query-on-exit-flag proc nil)
  (accept-process-output proc 0.1)
  (list (process-adaptive-read-buffering proc)
        (progn (set-process-adaptive-read-buffering proc nil)
               (process-adaptive-read-buffering proc))
        (progn (set-process-adaptive-read-buffering proc t)
               (process-adaptive-read-buffering proc))
        (process-type proc)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function process-adaptive-read-buffering)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u5_kill_buffer_query_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (generate-new-buffer " *probe-kbq*"))
      (log nil))
  (with-current-buffer b
    (setq-local kill-buffer-query-functions
                (list (lambda () (push 'query log) t))))
  (setq-local kill-buffer-hook
              (list (lambda () (push 'hook log))))
  (list (kill-buffer b)
        (buffer-live-p b)
        (nreverse log)))
"##;
    let expect = expect_test::expect![[r#""OK (t nil (query))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u5_window_scroll_functions_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wsf*"))
      (log nil))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b
          (dotimes (i 80) (insert (format "line%02d\n" i))))
        (setq window-scroll-functions nil)
        (add-hook 'window-scroll-functions
                  (lambda (win start) (push (list (windowp win) start) log)))
        (condition-case err (scroll-up 3) (error nil))
        (condition-case err (scroll-down 1) (error nil))
        (list (length log)
              (windowp (selected-window))
              (window-start)
              (window-end nil t)))
    (when (buffer-live-p b) (kill-buffer b))
    (setq window-scroll-functions nil)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[r#""OK (0 t 477 561)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u5_timer_create_cancel_reorder() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Printing raw `memq` tails would embed live timer vectors whose
    // HIGH/LOW/USEC/PSEC fields are wall-clock readings and can never match
    // across two processes; normalize membership to booleans and check the
    // timer time structure (four integer time fields, timer.el layout)
    // instead. The PSEC field being an integer (not nil) is the regression
    // guard for `time-add nil` dropping sub-microsecond precision.
    let form = r##"
(let ((t1 (run-at-time 100 nil (lambda () nil)))
      (t2 (run-at-time 200 nil (lambda () nil)))
      (t3 (run-at-time 50 nil (lambda () nil))))
  (list (length timer-list)
        (timer--repeat-delay t1)
        (timer--repeat-delay t2)
        (timer--repeat-delay t3)
        (mapcar (lambda (tm)
                  (list (integerp (timer--high-seconds tm))
                        (integerp (timer--low-seconds tm))
                        (integerp (timer--usecs tm))
                        (integerp (timer--psecs tm))
                        (timer--triggered tm)))
                (list t1 t2 t3))
        (progn (cancel-timer t2)
               (length timer-list))
        (and (memq t1 timer-list) t)
        (and (memq t2 timer-list) t)
        (and (memq t3 timer-list) t)
        (progn (cancel-timer t1) (cancel-timer t3)
               (length timer-list))))
"##;
    let expect = expect_test::expect![[
        r#""OK (3 nil nil nil ((t t t t nil) (t t t t nil) (t t t t nil)) 2 t nil t 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u5_condition_case_with_unwind_protect_cleanup_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (catch 'done
    (condition-case err
        (unwind-protect
            (progn
              (push 'body-start log)
              (signal 'error '("inner"))
              (push 'body-after-signal log))
          (push 'cleanup-1 log)
          (push 'cleanup-2 log))
      (error
       (push (cons 'caught (cdr err)) log)
       (throw 'done 'caught)))
  (nreverse log))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

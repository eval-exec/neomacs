//! Complex combo batch 251 — `profiler` CPU/memory actual profiling /
//! `trace-function` actual tracing / `backtrace-frame` extraction /
//! `mapbacktrace` frame iteration in signal handlers.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx251_profiler_cpu_start_stop_sample() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'profiler)
      (profiler-start 'cpu)
      (let ((sum (cl-loop for i from 1 to 1000 sum i)))
        (profiler-stop)
        (list (> sum 0)
              (fboundp 'profiler-report)
              (fboundp 'profiler-find-profile))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx251_profiler_memory_start_stop_sample() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'profiler)
      (profiler-start 'memory)
      (let ((data (make-list 500 :x)))
        (profiler-stop)
        (list (> (length data) 0)
              (fboundp 'profiler-report))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx251_trace_function_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'trace)
      (list (fboundp 'trace-function)
            (fboundp 'trace-function-background)
            (fboundp 'untrace-function)
            (fboundp 'untrace-all)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx251_backtrace_frame_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:outer-enter 42) (:mid-enter 42) (:inner 42) (:mid-exit 42) (:outer-exit 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (frames)
  (let* ((inner (lambda (x) (push (list :inner x) frames)))
         (mid (lambda (x) (push (list :mid-enter x) frames) (funcall inner x) (push (list :mid-exit x) frames)))
         (outer (lambda (x) (push (list :outer-enter x) frames) (funcall mid x) (push (list :outer-exit x) frames))))
    (funcall outer 42))
  (nreverse frames))
"##,
        expect,
    )
}

#[test]
fn div_cx251_mapbacktrace_in_signal_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (bt-frames)
  (condition-case e
      (signal 'wrong-type-argument '(integerp "x"))
    (error
     (mapbacktrace (lambda (evald func args flags)
                      (push (list (if (symbolp func) func :lambda)
                                  (length args))
                            bt-frames))
                   t)))
  (list (consp bt-frames)
        (> (length bt-frames) 0)))
"##,
        expect,
    )
}

#[test]
fn div_cx251_debug_on_entry_setup_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :ran""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((debug-on-error nil))
      (defun neo-cx251-debug-test () :result)
      (debug-on-entry 'neo-cx251-debug-test)
      (cancel-debug-on-entry 'neo-cx251-debug-test)
      :ran)
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx251_backtrace_buffer_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fboundp 'backtrace)
      (fboundp 'debug)
      (boundp 'debug-on-error)
      (boundp 'debug-on-quit)
      (boundp 'debug-ignored-errors))
"##,
        expect,
    )
}

#[test]
fn div_cx251_profiler_log_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'profiler)
      (list (fboundp 'profiler-start)
            (fboundp 'profiler-stop)
            (fboundp 'profiler-reset)
            (boundp 'profiler-cpu-log)
            (boundp 'profiler-memory-log)))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx251_ert_should_error_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((error \"boom\") (wrong-type-argument integerp \"x\") :no-error)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ert)
      (list (should-error (error "boom"))
            (should-error (signal 'wrong-type-argument '(integerp "x")))
            (condition-case err (should-error (+ 1 1)) (error :no-error))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx251_profiler_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((bt-frames nil))
      (condition-case err
          (signal 'wrong-type-argument '(integerp "x"))
        (error
         (mapbacktrace (lambda (evald func args flags)
                          (push (when (symbolp func) func) bt-frames))
                       t)))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert (format "Profiler/backtrace mega: %d frames" (length bt-frames)))
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (let ((state (list (consp bt-frames)
                             (boundp 'debug-on-error)
                             (fboundp 'profiler-start)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

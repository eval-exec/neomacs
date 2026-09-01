//! Complex combo batch 103 — edebug / trace / backtrace / mapbacktrace /
//! profiler-cpu / profiler-memory / debug-on-error availability and
//! behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx103_debug_on_error_handler_runs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:caught wrong-type-argument number-or-marker-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((debug-on-error t))
  (condition-case err
      (progn
        (+ 1 "x")
        :never)
    (error (list :caught (car err) (cadr err)))))
"##,
        expect,
    );
}

#[test]
fn div_cx103_debug_on_quit_handler_runs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid condition handler: :never\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((debug-on-quit t))
  (condition-case err
      (signal 'quit nil)
      :never)
    (quit (list :caught-quit))
    (error (list :caught-error))))
"##,
        expect,
    );
}

#[test]
fn div_cx103_backtrace_frame_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:outer-enter :inner :outer-exit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (frames)
  (let* ((inner (lambda () (push :inner frames)))
         (outer (lambda () (push :outer-enter frames) (funcall inner) (push :outer-exit frames))))
    (funcall outer))
  (nreverse frames))
"##,
        expect,
    );
}

#[test]
fn div_cx103_profiler_cpu_start_stop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5050 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'profiler)
      (profiler-start 'cpu)
      (let ((sum (cl-loop for i from 1 to 100 sum i)))
        (profiler-stop)
        (list sum
              (fboundp 'profiler-report)
              (fboundp 'profiler-find-profile))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx103_profiler_memory_start_stop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'profiler)
      (profiler-start 'memory)
      (let ((data (make-list 100 :x)))
        (profiler-stop)
        (list (> (length data) 0)
              (fboundp 'profiler-report))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx103_trace_function_availability() {
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
fn div_cx103_debugger_break_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'debug)
          (fboundp 'cancel-debug-on-entry)
          (fboundp 'debug-on-entry)
          (boundp 'debug-on-error)
          (boundp 'debug-ignored-errors))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx103_edebug_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'edebug)
      (list (fboundp 'edebug-defun)
            (fboundp 'edebug-step-through)
            (boundp 'edebug-initial-mode)
            (boundp 'edebug-all-defs)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx103_ert_basic_assertion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'ert)
      (list (fboundp 'ert-deftest)
            (fboundp 'ert-run-tests-interactively)
            (fboundp 'should)
            (fboundp 'should-not)
            (fboundp 'should-error)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx103_ert_should_error_with_form() {
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
    );
}

#[test]
fn div_cx103_set_debug_on_entry_for_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :ran""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((debug-on-error nil))
      (debug-on-entry 'neo-cx103-test-fn)
      (defun neo-cx103-test-fn () :result)
      (cancel-debug-on-entry 'neo-cx103-test-fn)
      :ran)
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx103_compiler_macro_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'byte-compile)
          (fboundp 'byte-compile-file)
          (boundp 'byte-compile-warnings)
          (fboundp 'native-compile)
          (boundp 'native-comp-jit-compilation))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx103_debug_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((debug-on-error nil))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Debug test buffer content here")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state-1 (list (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
        (condition-case err
            (+ 1 "x")
          (error (list :caught err)))
        (undo)
        (widen)
        (list state-1 (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

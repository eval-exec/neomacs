//! Strict combo oracle probes, batch 128: areas adjacent to recent remote
//! fixes — color-rgb-to-hsl/hsl-to-rgb, timer--time formatting, process
//! stderr to pipe-process, font-spec height modes, and nil-argument edge
//! cases across arithmetic functions.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u2_color_rgb_hsl_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (color-rgb-to-hsl 65535 0 0)
      (color-rgb-to-hsl 0 65535 0)
      (color-rgb-to-hsl 0 0 65535)
      (color-rgb-to-hsl 0 0 0)
      (color-rgb-to-hsl 32768 32768 32768)
      (color-values "red")
      (color-values "blue")
      (color-values "black")
      (color-values "white")
      (color-values "gray50")
      (color-distance "red" "blue")
      (color-distance "black" "white")
      (color-complement "red"))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function color-rgb-to-hsl)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u2_timer_time_format_and_cancel_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let (timers)
  (dotimes (i 3)
    (push (run-at-time 100 nil (lambda () nil)) timers))
  (list (length timer-list)
        (length timers)
        (timerp (car timers))
        (timer--repeat-delay (car timers))
        (integerp (car (timer--time (car timers))))
        (integerp (nth 3 (timer--time (car timers))))
        (progn (dolist (tm timers) (cancel-timer tm))
               (length timer-list))
        (timerp (car timers))))
"##;
    let expect = expect_test::expect![[r#""OK (3 3 t nil t t 0 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u2_process_stderr_pipe_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((stderr-pipe (make-pipe-process :name "probe-stderr-pipe"
                                       :buffer (generate-new-buffer " *probe-sp*")
                                       :sentinel (lambda (&rest _) nil))))
  (let ((proc (make-process :name "probe-sp-main"
                            :command (list shell-file-name shell-command-switch
                                           "echo out; echo err 1>&2")
                            :buffer (generate-new-buffer " *probe-sp-out*")
                            :stderr stderr-pipe
                            :sentinel (lambda (&rest _) nil))))
    (set-process-query-on-exit-flag proc nil)
    (set-process-query-on-exit-flag stderr-pipe nil)
    (accept-process-output proc 1)
    (accept-process-output stderr-pipe 1)
    (let ((out (with-current-buffer (process-buffer proc) (buffer-string)))
          (err (with-current-buffer (process-buffer stderr-pipe) (buffer-string))))
      (kill-buffer (process-buffer proc))
      (kill-buffer (process-buffer stderr-pipe))
      (list (string-trim out)
            (string-trim err)
            (string= (string-trim out) "out")
            (string= (string-trim err) "err"))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u2_font_spec_height_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((fs1 (font-spec :family "Monospace" :height 100))
      (fs2 (font-spec :family "Monospace" :height 10.0))
      (fs3 (font-spec :family "Monospace" :height 120))
      (fs4 (font-spec :family "Monospace")))
  (list (font-get fs1 :height)
        (font-get fs2 :height)
        (font-get fs3 :height)
        (font-get fs4 :height)
        (font-spec-p fs1)
        (font-spec-p fs2)
        (> (font-get fs3 :height) (font-get fs1 :height))
        (integerp (font-get fs1 :height))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function font-spec-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u2_nil_argument_arithmetic_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (condition-case err (+ 1 nil) (wrong-type-argument (car err)))
      (condition-case err (- 1 nil) (wrong-type-argument (car err)))
      (condition-case err (* 1 nil) (wrong-type-argument (car err)))
      (condition-case err (/ 1 nil) (wrong-type-argument (car err)))
      (condition-case err (max 1 nil 2) (wrong-type-argument (car err)))
      (condition-case err (min 1 nil 2) (wrong-type-argument (car err)))
      (condition-case err (mod 10 nil) (wrong-type-argument (car err)))
      (condition-case err (ash 1 nil) (wrong-type-argument (car err)))
      (condition-case err (logand 1 nil) (wrong-type-argument (car err)))
      (condition-case err (expt 2 nil) (wrong-type-argument (car err))))
"##;
    let expect = expect_test::expect![[
        r#""OK (wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Complex combo batch 108 — message / format / display-time / sit-for /
//! redisplay / current-message / with-temp-message semantics.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx108_message_basic_and_current_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (message "hello %s" "world")
  (let ((m (current-message)))
    (prog1 m
      (message nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_message_clears_with_nil_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (message "first")
  (let ((first (current-message)))
    (message nil)
    (let ((after-nil (current-message)))
      (list first after-nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_with_temp_message_restores_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((nil :inside) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (message "outer")
  (let ((result
         (with-temp-message "inner"
           (list (current-message)
                 :inside))))
    (list result (current-message))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_format_message_with_backtick_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"plain\" \"with ‘quotes’ here\" \"with x substitution\" \"with ‘nested y inside’\" \"z and `backtick' literal\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (format-message "plain")
 (format-message "with `quotes' here")
 (format-message "with %s substitution" "x")
 (format-message "with `nested %s inside'" "y")
 (format "%s and `backtick' literal" "z"))
"##,
        expect,
    );
}

#[test]
fn div_cx108_message_or_box_or_neutral() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'message-or-box)
          (fboundp 'message-box)
          (fboundp 'message-with-echo-area))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_sit_for_returns_t_on_idle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((r1 (sit-for 0))
          (r2 (sit-for 0.001)))
      (list r1 r2))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_sleep_for_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (sleep-for 0)
          (sleep-for 0.001)
          (sleep-for 0 100))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_redisplay_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((r1 (redisplay))
          (r2 (redisplay t))
          (r3 (redisplay 'force)))
      (list r1 r2 r3))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_force_mode_line_update() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'force-mode-line-update)
          (fboundp 'force-window-update)
          (fboundp 'window-width)
          (fboundp 'window-height))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_progress_reporter_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((reporter (make-progress-reporter "Doing work..." 0 100)))
      (progress-reporter-update reporter 25)
      (progress-reporter-update reporter 50)
      (progress-reporter-update reporter 75)
      (progress-reporter-done reporter)
      (current-message))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_format_propertized_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"ALPHA 42 OMEGA\" 0 5 (face bold) 9 14 (face italic)) (face bold) nil (face italic))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((formatted (format "%s %d %s"
                              (propertize "ALPHA" 'face 'bold)
                              42
                              (propertize "OMEGA" 'face 'italic)))
           (props-1 (text-properties-at 0 formatted))
           (props-7 (text-properties-at 6 formatted))
           (props-10 (text-properties-at 9 formatted)))
      (list formatted props-1 props-7 props-10))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_minibuffer_message_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'minibuffer-message)
          (fboundp 'minibuffer-prompt)
          (fboundp 'read-from-minibuffer)
          (fboundp 'read-string))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx108_message_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((outer-msg (progn (message "outer") (current-message))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Message mega test buffer content")
    (put-text-property 1 7 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (message "inner %d" 42)
      (let ((inner-msg (current-message)))
        (let ((state (list outer-msg inner-msg
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1))))))
"##,
        expect,
    );
}

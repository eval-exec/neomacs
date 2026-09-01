//! Strict combo oracle probes, batch 35: heavier loaded-library coverage via
//! assert_oracle_parity_with_load — time-date.el (date/time conversions),
//! ansi-color.el (ANSI escape application), wid-edit.el (widget create/get),
//! and parse-time.el (parse-time-string).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_h2_time_date_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (737424 1577836800.0 (972189 55296) (24076 10064) 1577836800.0 719162)""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((t0 (encode-time 0 0 0 1 1 2020 0)))
  (list (time-to-days t0)
        (time-to-seconds t0)
        (days-to-time (time-to-days t0))
        (date-to-time "2020-01-01 00:00:00")
        (float-time t0)
        (time-to-days (encode-time 0 0 0 1 1 1970 0))))
"##,
        &["calendar/time-date.el"],
        expect,
    );
}

#[test]
fn div_h2_ansi_color_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"red and bold green\" 0 3 (font-lock-face (:foreground \"red3\")) 8 18 (font-lock-face (ansi-color-bold (:foreground \"green3\")))) 18 \"red and bold green\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((s "\033[31mred\033[0m and \033[1;32mbold green\033[0m"))
  (list (ansi-color-apply s)
        (length (ansi-color-apply s))
        (ansi-color-filter-apply s)))
"##,
        &["ansi-color.el"],
        expect,
    );
}

#[test]
fn div_h2_widget_create_and_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((default :convert-widget widget-value-convert-widget :keymap (keymap (5 . widget-end-of-line) (11 . widget-kill-line) (13 . widget-field-activate) (touchscreen-begin . widget-button-click) (down-mouse-1 . widget-button-click) (down-mouse-2 . widget-button-click) (backtab . widget-backward) (S-tab . widget-backward) (27 keymap (9 . widget-complete)) (9 . widget-forward)) :format \"%v\" :help-echo \"M-TAB: complete field; RET: enter value\" :value \"\" :prompt-internal widget-field-prompt-internal :prompt-history widget-field-history :prompt-value widget-field-prompt-value :action widget-field-action :validate widget-field-validate :valid-regexp \"\" :error \"Field's value doesn't match allowed forms\" :value-create widget-field-value-create :value-set widget-field-value-set :value-delete widget-field-value-delete :value-get widget-field-value-get :match widget-field-match) editable-field \"default text\" (toggle :button-suffix \"\" :button-prefix \"\" :format \"%[%v%]\" :on \"[X]\" :on-glyph \"checked\" :off \"[ ]\" :off-glyph \"unchecked\" :help-echo \"Toggle this item.\" :action widget-checkbox-action) checkbox \"default text\")""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (let ((w (widget-create 'editable-field :format "%v" "default text"))
        (w2 (widget-create 'checkbox)))
    (list (widgetp w)
          (widget-type w)
          (widget-get w :value)
          (widgetp w2)
          (widget-type w2)
          (widget-apply w :value-get))))
"##,
        &["wid-edit.el"],
        expect,
    );
}

#[test]
fn div_h2_parse_time_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((45 30 12 15 6 2020 nil -1 nil) (45 30 12 15 6 2020 1 -1 0) (nil nil nil nil nil nil nil -1 nil))""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (parse-time-string "2020-06-15 12:30:45")
      (parse-time-string "Mon, 15 Jun 2020 12:30:45 +0000")
      (parse-time-string "invalid junk"))
"##,
        &["calendar/parse-time.el"],
        expect,
    );
}

#[test]
fn div_h2_time_date_arithmetic_fixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1580428830 1577833230 t t 86400.0)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((t0 (encode-time 30 0 0 1 1 2020 0)))
  (list (time-add t0 (days-to-time 30))
        (time-subtract t0 3600)
        (time-less-p t0 (time-add t0 1))
        (time-equal-p t0 t0)
        (float-time (days-to-time 1))))
"##,
        &["calendar/time-date.el"],
        expect,
    );
}

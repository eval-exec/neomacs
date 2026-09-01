//! Strict combo oracle probes, batch 365: window-parameters behavioral.
//! set-window-parameter, window-parameter, window-parameters listing,
//! and window-parameter deletion via set-nil.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_window_parameter_set_get_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (set-window-parameter w 'probe-param-1 'val-1)
  (set-window-parameter w 'probe-param-2 42)
  (set-window-parameter w 'probe-param-3 '(nested list))
  (list (window-parameter w 'probe-param-1)
        (window-parameter w 'probe-param-2)
        (window-parameter w 'probe-param-3)
        (window-parameter w 'nonexistent)
        (consp (window-parameters w))
        (assq 'probe-param-1 (window-parameters w))))
"##;
    let expect =
        expect_test::expect![[r#""OK (val-1 42 (nested list) nil t (probe-param-1 . val-1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_parameter_override_delete_survive_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (set-window-parameter w 'probe-survive 'survived)
  (let ((cfg (current-window-configuration)))
    (split-window nil nil 'right)
    (set-window-configuration cfg)
    (list (window-parameter w 'probe-survive)
          (window-parameter w 'probe-new)
          (eq w (selected-window)))))
"##;
    let expect = expect_test::expect![[r#""OK (survived nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_window_parameters_in_window_state_get_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((w (selected-window)))
  (delete-other-windows)
  (set-window-parameter w 'probe-state 'value-x)
  (let ((state (window-state-get w nil)))
    (delete-other-windows)
    (let ((w2 (selected-window)))
      (window-state-put state w2 nil)
      (list (window-parameter w2 'probe-state)
            (windowp w2)
            (eq w w2)))))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

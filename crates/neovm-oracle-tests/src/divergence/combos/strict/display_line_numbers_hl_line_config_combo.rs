//! Strict combo oracle probes, batch 224: line-number / highlight display
//! config. display-line-numbers-mode + display-line-numbers, linum-mode,
//! hl-line-mode toggles, and the buffer-local variables they set.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_display_line_numbers_mode_toggle_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-dln*")
  (insert "a\nb\nc")
  (display-line-numbers-mode 1)
  (let ((result (list (boundp 'display-line-numbers)
                      display-line-numbers
                      (default-value 'display-line-numbers)
                      (progn (display-line-numbers-mode -1) display-line-numbers))))
    (kill-buffer (current-buffer))
    result))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_hl_line_mode_toggle_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-hl*")
  (insert "line")
  (let ((before hl-line-mode))
    (hl-line-mode 1)
    (let ((after-on hl-line-mode))
      (hl-line-mode -1)
      (let ((after-off hl-line-mode))
        (kill-buffer (current-buffer))
        (list before after-on after-off)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable hl-line-mode)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_display_line_numbers_width_format_config() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-current-buffer (get-buffer-create " *probe-dlw*")
  (insert "x")
  (display-line-numbers-mode 1)
  (setq display-line-numbers-width 4)
  (setq display-line-numbers 'relative)
  (let ((result (list display-line-numbers
                      display-line-numbers-width
                      (progn (setq display-line-numbers 'visual) display-line-numbers)
                      (progn (setq display-line-numbers t) display-line-numbers))))
    (kill-buffer (current-buffer))
    result))
"##;
    let expect = expect_test::expect![[r#""OK (relative 4 visual t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

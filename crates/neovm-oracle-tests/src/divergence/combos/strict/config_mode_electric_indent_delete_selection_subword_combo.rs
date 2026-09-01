//! Strict combo oracle probes, batch 228: minor config-mode toggles.
//! electric-indent-mode, delete-selection-mode, subword-mode, and show-paren-mode
//! toggle state round-trips and buffer-local behavior.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_electric_indent_delete_selection_global_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((ei electric-indent-mode)
      (ds delete-selection-mode))
  (unwind-protect
      (progn
        (electric-indent-mode 1)
        (delete-selection-mode 1)
        (let ((ei-on electric-indent-mode)
              (ds-on delete-selection-mode))
          (electric-indent-mode -1)
          (delete-selection-mode -1)
          (list ei ds ei-on ds-on electric-indent-mode delete-selection-mode)))
    (when (null electric-indent-mode) (electric-indent-mode (if ei 1 -1)))
    (when (null delete-selection-mode) (delete-selection-mode (if ds 1 -1)))))
"##;
    let expect = expect_test::expect![[r#""OK (t nil t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_subword_show_paren_buffer_local_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((sp show-paren-mode))
  (unwind-protect
      (with-current-buffer (get-buffer-create " *probe-cfg*")
        (subword-mode 1)
        (show-paren-mode 1)
        (let ((sw subword-mode)
              (sp-on show-paren-mode))
          (subword-mode -1)
          (show-paren-mode -1)
          (let ((result (list sw sp-on subword-mode show-paren-mode)))
            (kill-buffer (current-buffer))
            result)))
    (when (null show-paren-mode) (show-paren-mode (if sp 1 -1)))))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_global_mode_state_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (booleanp electric-indent-mode)
      (booleanp delete-selection-mode)
      (booleanp subword-mode)
      (booleanp show-paren-mode)
      (booleanp hl-line-mode)
      (booleanp line-number-mode)
      (booleanp column-number-mode)
      (boundp 'global-hl-line-mode))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable subword-mode)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

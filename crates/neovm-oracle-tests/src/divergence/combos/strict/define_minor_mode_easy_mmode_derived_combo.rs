//! Strict combo oracle probes, batch 212: mode definition macros. define-
//! minor-mode (variable + toggle function + lighter + keymap), define-derived-
//! mode parent/body, and easy-mmode define-minor-mode keymap body.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_define_minor_mode_toggle_lighter_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (define-minor-mode probe-mmr-mode
    "Probe minor mode."
    :init-value nil
    :lighter " Probe"
    :keymap '((("C-c p" . probe-mmr-act)))
    (setq probe-mmr-side-effect 'toggled))
  (list (fboundp 'probe-mmr-mode)
        (boundp 'probe-mmr-mode)
        (default-value 'probe-mmr-mode)
        (progn (probe-mmr-mode 1) (default-value 'probe-mmr-mode))
        (progn (probe-mmr-mode -1) (default-value 'probe-mmr-mode))
        (assq 'probe-mmr-mode minor-mode-alist)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp probe-mmr-act)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_define_global_minor_mode_globalized() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (define-minor-mode probe-gmm-mode
    "Probe global minor mode."
    :global t
    :init-value nil
    (setq probe-gmm-ran t))
  (list (fboundp 'probe-gmm-mode)
        (boundp 'probe-gmm-mode)
        (default-value 'probe-gmm-mode)
        (progn (probe-gmm-mode 1) (default-value 'probe-gmm-mode))
        probe-gmm-ran))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_define_derived_mode_parent_syntax_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (define-derived-mode probe-derived-mode prog-mode "ProbeDer"
    "A probe derived mode."
    (setq-local probe-derived-ran t))
  (with-temp-buffer
    (probe-derived-mode)
    (list (eq major-mode 'probe-derived-mode)
          (eq (derived-mode-p 'prog-mode) t)
          (derived-mode-p 'text-mode)
          probe-derived-ran
          (stringp (format-mode-line mode-name)))))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

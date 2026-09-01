//! Strict combo oracle probes, batch 231: feature / loadhist introspection.
//! featurep / provide / require, feature-file, feature-symbols subset, and
//! load-history queries.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_featurep_provide_require_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (provide 'probe-feat-a)
  (list (featurep 'probe-feat-a)
        (featurep 'subr)
        (featurep 'probe-feat-missing)
        (consp (memq 'probe-feat-a features))
        (provided-mode-derived-p 'prog-mode 'fundamental-mode)))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_feature_file_and_load_history_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'loadhist)
(list (feature-file 'subr)
      (stringp (feature-file 'subr))
      (assoc (feature-file 'subr) load-history)
      (consp load-history)
      (> (length load-history) 0))
"##;
    let expect =
        expect_test::expect![[r#""ERR (error \"subr is not a currently loaded feature\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_autoload_p_function_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (autoloadp (symbol-function 'find-file))
      (autoloadp (symbol-function 'car))
      (subrp (symbol-function 'car))
      (byte-code-function-p (symbol-function 'car))
      (subrp (symbol-function 'cons))
      (let ((sfn (symbol-function 'car)))
        (or (subrp sfn) (byte-code-function-p sfn)))
      (commandp 'forward-char))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

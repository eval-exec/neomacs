//! Strict combo oracle probes, batch 345: load/require/autoload deep.
//! load with NOERROR/NOSUFFIX, require with MINIMUM-VERSION, autoload-do-load,
//! after-load-functions, and load-suffixes.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_load_noerror_nosuffix_require_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (load "nonexistent-file-xyz" t t t)
      (featurep 'subr)
      (fboundp 'car)
      (null (load "nonexistent-2" t)))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_autoload_do_load_function_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (autoloadp (symbol-function 'find-file))
      (functionp (symbol-function 'find-file))
      (fboundp 'find-file)
      (let ((sfn (symbol-function 'find-file)))
        (or (autoloadp sfn) (functionp sfn)))
      (load-suffixes)
      (load-file-rep-suffixes))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function load-suffixes)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_after_load_alist_eval_after_load_provide() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((eval-marker nil))
  (eval-after-load 'probe-ala '(setq eval-marker 'fired))
  (provide 'probe-ala)
  (list eval-marker
        (featurep 'probe-ala)
        (consp (assq 'probe-ala after-load-alist))))
"##;
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Strict combo oracle probes, batch 289: abbrev CORE variable sweep. Any
//! nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_abbrev_mode_table_name_list_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'abbrev-mode)
      (boundp 'global-abbrev-table)
      (boundp 'abbrev-all-caps)
      (boundp 'abbrev-file-name)
      (boundp 'abbrevs-changed)
      (boundp 'abbrev-table-name-list)
      (boundp 'save-abbrevs)
      (boundp 'only-global-abbrevs)
      (boundp 'pre-abbrev-expand-hook)
      (boundp 'abbrev-expand-function)
      (boundp 'abbrev-start-location)
      (boundp 'abbrev-start-location-buffer))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_abbrev_local_table_editing_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'local-abbrev-table)
      (boundp 'abbrev-minor-mode-table-alist)
      (boundp 'abbrev-edit-functions)
      (boundp 'abbrev-expand-functions)
      (boundp 'abbrev-suggest)
      (boundp 'abbrev-suggest-show-usage-message)
      (boundp 'abbrev-table-get)
      (boundp 'abbrev-table-p)
      (boundp 'define-abbrev-table)
      (boundp 'define-global-abbrev)
      (boundp 'define-mode-abbrev)
      (boundp 'quietly-read-abbrev-file))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil t t nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Strict combo oracle probes, batch 70: org-table operations, text/enriched
//! encode/decode, and CCL (Code Conversion Language) program compilation.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_o4_org_table_align_and_analyze() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | b |\\n|---+---|\\n| 1 | 2 |\\n\" 0 9 (face org-table) 9 10 (face org-table-row) 10 19 (face org-table) 19 20 (face org-table-row) 20 29 (face org-table) 29 30 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |\n")
  (org-table-align)
  (buffer-string))
"##,
        &["org/org.el", "org/org-table.el"],
        expect,
    );
}

#[test]
fn div_o4_org_table_get_and_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Unknown field: value\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (org-mode)
  (insert "| name | value |\n|---+---|\n| x | 10 |\n| y | 20 |\n")
  (org-table-goto-field "value")
  (list (org-table-get-field "name" 1)
        (org-table-get-field "value" 1)
        (org-table-get-field "value" 2)))
"##,
        &["org/org.el", "org/org-table.el"],
        expect,
    );
}

#[test]
fn div_o4_enriched_encode_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 67)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let ((s1 #("bold text" 0 4 (face bold))))
  (list (stringp s1)
        (with-temp-buffer
          (insert s1)
          (let ((str (enriched-encode (point-min) (point-max) nil)))
            str))))
"##,
        &["textmodes/enriched.el"],
        expect,
    );
}

#[test]
fn div_o4_ccl_program_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (err . error)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(condition-case err
    (let ((prog (ccl-compile
                  '((read r0)
                    (loop
                      (write-read-repeat r0))))))
      (list (vectorp prog)
            (length prog)))
  (error (cons 'err (car err))))
"##,
        &["international/ccl.el"],
        expect,
    );
}

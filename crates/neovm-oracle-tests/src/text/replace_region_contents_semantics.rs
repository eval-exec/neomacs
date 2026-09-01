//! Oracle coverage for GNU's non-destructive region replacement contract.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_replace_region_contents_relocates_point_across_change_runs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/editfns.c walks the diff back-to-front, while src/insdel.c
    // relocates point for every replace_range call as an insert-before marker.
    // Use all positions in a compact JSON object because pretty-printing it
    // produces multiple disjoint insertions and exposes accidental use of point
    // as the implementation's edit cursor.
    let form = r#"
(mapcar
 (lambda (position)
   (with-temp-buffer
     (insert "{\"x\":1}")
     (goto-char position)
     (replace-region-contents
      (point-min) (point-max) "{\n  \"x\": 1\n}")
     (point)))
 '(1 2 3 4 5 6 7 8))
"#;

    let expect = expect_test::expect![[r#""OK (1 5 6 7 8 10 12 13)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

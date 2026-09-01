//! Strict combo oracle probes, batch 181: buffer-substring and filters.
//! buffer-substring (with/without properties), buffer-substring-no-properties,
//! filter-buffer-substring over a propertized range, buffer-substring-filters
//! membership, and substring extraction edge cases (empty, full, mid).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_buffer_substring_with_without_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "Hello World Foo Bar")
  (add-text-properties 1 6 '(face bold))
  (add-text-properties 7 13 '(face italic color red))
  (list (buffer-substring 1 6)
        (buffer-substring-no-properties 1 21)
        (buffer-substring 7 13)
        (buffer-substring 14 21)
        (format "%s" (buffer-substring 1 5))
        (text-properties-at 1 (buffer-substring 1 6))
        (get-text-property 2 'face (buffer-substring 1 6))))
"##;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 1 21)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_filter_buffer_substring_filters_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "visible HIDDEN visible")
  (add-text-properties 9 15 '(invisible t))
  (list (buffer-substring 1 22)
        (filter-buffer-substring 1 22 nil)
        (filter-buffer-substring 1 22 t)
        (consp buffer-substring-filters)
        (filter-buffer-substring 1 8 nil)
        (buffer-substring-no-properties 1 22)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable buffer-substring-filters)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_buffer_substring_edge_empty_full_mid() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "ABCDE")
  (list (buffer-substring 3 3)
        (buffer-substring 1 6)
        (buffer-substring 2 4)
        (buffer-substring 1 1)
        (buffer-substring 6 6)
        (length (buffer-substring-no-properties 1 6))
        (buffer-substring 5 6)
        (eq (buffer-substring 1 1) "")))
"##;
    let expect = expect_test::expect![[r#""OK (\"\" \"ABCDE\" \"BC\" \"\" \"\" 5 \"E\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

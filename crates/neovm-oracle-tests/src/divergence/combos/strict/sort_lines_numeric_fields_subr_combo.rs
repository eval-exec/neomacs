//! Strict combo oracle probes, batch 246: sorting. sort-lines (lexicographic
//! forward/reverse), sort-numeric-fields, sort-columns, and sort-subr with a
//! custom key extractor.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_sort_lines_lexicographic_reverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-temp-buffer
        (insert "banana\napple\ncherry\ndate\n")
        (sort-lines nil (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (insert "banana\napple\ncherry\ndate\n")
        (sort-lines t (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (insert "a\nb\nc\n")
        (sort-lines nil (point-min) (point-max))
        (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"apple\\nbanana\\ncherry\\ndate\\n\" \"date\\ncherry\\nbanana\\napple\\n\" \"a\\nb\\nc\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_sort_numeric_fields_columns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-temp-buffer
        (insert "10 alpha\n2 beta\n100 gamma\n1 delta\n")
        (sort-numeric-fields 1 (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (insert "z 1\na 2\nm 3\n")
        (sort-fields 1 (point-min) (point-max))
        (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"1 delta\\n2 beta\\n10 alpha\\n100 gamma\\n\" \"a 2\\nm 3\\nz 1\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_sort_subr_custom_key_extractor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-temp-buffer
        (insert "banana:3\napple:1\ncherry:2\n")
        (sort-lines nil (point-min) (point-max))
        (buffer-string))
      (with-temp-buffer
        (insert "first paragraph here\n\naaa short\n\nmiddle paragraph longer\n")
        (sort-paragraphs nil (point-min) (point-max))
        (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"apple:1\\nbanana:3\\ncherry:2\\n\" \"aaa short\\n\\nfirst paragraph here\\n\\nmiddle paragraph longer\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

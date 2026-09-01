//! Strict combo oracle probes, batch 193: case manipulation. upcase/downcase/
//! capitalize strings + regions, upcase-initials, multibyte case folding
//! (accented latin), and case-region variants.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_case_upcase_downcase_capitalize_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (upcase "hello world")
      (downcase "HELLO WORLD")
      (capitalize "hello world foo")
      (upcase-initials "hello world foo")
      (upcase-initials "the QUICK brown")
      (capitalize "aBC dEF")
      (downcase "ABCdef123GHI")
      (upcase "abc123def"))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"HELLO WORLD\" \"hello world\" \"Hello World Foo\" \"Hello World Foo\" \"The QUICK Brown\" \"Abc Def\" \"abcdef123ghi\" \"ABC123DEF\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_case_region_upcase_downcase_capitalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-temp-buffer
        (insert "hello world")
        (upcase-region 1 6)
        (buffer-string))
      (with-temp-buffer
        (insert "HELLO WORLD")
        (downcase-region 1 6)
        (buffer-string))
      (with-temp-buffer
        (insert "hello world foo")
        (capitalize-region 1 12)
        (buffer-string))
      (with-temp-buffer
        (insert "hello world foo")
        (upcase-initials-region 1 16)
        (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"HELLO world\" \"hello WORLD\" \"Hello World foo\" \"Hello World Foo\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_case_multibyte_accented_and_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (upcase "héllo wörld")
      (downcase "HÉLLO WÖRLD")
      (capitalize "naïve café résumé")
      (upcase-initials "ñandú über")
      (with-temp-buffer
        (insert "café résumé")
        (upcase-region 1 12)
        (buffer-string))
      (with-temp-buffer
        (insert "ÖSTERREICH")
        (downcase-region 1 10)
        (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"HÉLLO WÖRLD\" \"héllo wörld\" \"Naïve Café Résumé\" \"Ñandú Über\" \"CAFÉ RÉSUMÉ\" \"österreicH\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

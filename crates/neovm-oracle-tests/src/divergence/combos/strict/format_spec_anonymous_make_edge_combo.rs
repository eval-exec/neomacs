//! Strict combo oracle probes, batch 354: format-spec edge cases.
//! format-spec with %N (number), empty spec list, nested %%, format-spec-make
//! with special chars, and format-spec error handling.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_format_spec_edge_multibyte_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((spec '((?n . "café") (?j . "日本語") (?e . ""))))
  (list (format-spec "%n %j" spec)
        (format-spec "%e" spec)
        (format-spec "%n-%j-%e" spec)
        (format-spec "no specs" spec)
        (format-spec "%%literal %n" spec)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"café 日本語\" \"\" \"café-日本語-\" \"no specs\" \"%literal café\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_spec_make_from_string_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((s1 (format-spec-make "a=1 b=2 c=3"))
      (s2 (format-spec-make "single=val"))
      (s3 (format-spec-make "")))
  (list (length s1)
        (format-spec "a=%a b=%b c=%c" s1)
        (format-spec "s=%s" s2)
        (format-spec "no match" s3)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function format-spec-make)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_spec_with_predicate_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((spec '((?a . "apple") (?b . "banana") (?c . "cherry"))))
  (list (format-spec "%a %b %c" spec)
        (format-spec "%a-%b-%c-%a" spec)
        (length (format-spec "%a%b%c" spec))
        (format-spec "x%ay%bz%cw" spec)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"apple banana cherry\" \"apple-banana-cherry-apple\" 17 \"xappleybananazcherryw\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

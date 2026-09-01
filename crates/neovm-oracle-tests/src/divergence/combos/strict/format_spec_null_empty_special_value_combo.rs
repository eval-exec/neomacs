//! Strict combo oracle probes, batch 370: format-spec with null/empty/special
//! values. Spec entries with nil, empty-string, number, list, and symbol values.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_format_spec_null_empty_number_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((spec '((?n . nil) (?e . "") (?d . 42) (?s . 'symbol) (?l . (a b c)))))
  (list (format-spec "n=%n e=%e d=%d" spec)
        (format-spec "s=%s l=%l" spec)
        (format-spec "%n" spec)
        (format-spec "%e" spec)
        (format-spec "%d" spec)))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Invalid format character: ‘%n’\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_spec_make_with_equals_and_spaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format-spec-make "key=value")
      (format-spec-make "key1=v1 key2=v2")
      (format-spec-make "with space=v")
      (length (format-spec-make "a=1 b=2 c=3")))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function format-spec-make)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_spec_unknown_spec_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((spec '((?a . "val"))))
  (list (format-spec "known %a unknown %z" spec)
        (format-spec "%a%z%a" spec)
        (format-spec "no specs at all" spec)
        (format-spec "" spec)))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Invalid format character: ‘%z’\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

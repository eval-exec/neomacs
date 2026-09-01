//! Strict combo oracle probes, batch 161: format-spec. %SPEC expansion from an
//! alist, %% literal percent, missing-spec handling, format-spec-make from a
//! "key=value" string, and padding/flag forms (%-10s width).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_format_spec_expand_alist_percent_missing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((spec '((?u . "alice") (?h . "host") (?p . "/tmp/file"))))
  (list (format-spec "user=%u host=%h path=%p" spec)
        (format-spec "only %u" spec)
        (format-spec "100%%percent done" spec)
        (format-spec "missing %z end" spec)
        (format-spec "no specs at all" spec)
        (format-spec "%u-%u-%h" spec)))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Invalid format character: ‘%z’\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_spec_make_and_padding_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format-spec-make "user=alice")
      (format-spec-make "user=alice host=box")
      (let ((s (format-spec-make "n=10 m=20")))
        (format-spec "n=%n m=%m" s))
      (format-spec "pad %-10s|" '((?s . "hi")))
      (format-spec "pad %10s|" '((?s . "hi")))
      (format-spec "trunc %5s|" '((?s . "toolong")))
      (format-spec "key=%( g=%)" '((?u . "v"))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function format-spec-make)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_spec_multibyte_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((spec '((?n . "日本語") (?e . "café"))))
  (list (format-spec "name=%n drink=%e" spec)
        (length (format-spec "%n" spec))
        (format-spec "%e-%e" spec)
        (format-spec "tab\there" spec)
        (format-spec "newline\nhere" spec)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"name=日本語 drink=café\" 3 \"café-café\" \"tab\there\" \"newline\\nhere\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

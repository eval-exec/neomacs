//! Strict combo oracle probes, batch 377: regexp named-group + shy + backref
//! edge cases. Named groups (?<name>...), shy groups, nested backrefs, and
//! match-data extraction.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_regexp_named_group_backref_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (string-match "\\(?<key>[a-z]+\\)=\\(?<val>[0-9]+\\)" "abc=42")
      (match-string 1 "abc=42")
      (match-string 2 "abc=42")
      (string-match "\\(?1:foo\\) \\1" "foo foo")
      (match-string 1 "foo foo")
      (string-match "\\(?<word>\\w+\\) \\k<word>" "hello hello")
      (match-string 1 "hello hello"))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-regexp \"Invalid regular expression\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_regexp_shy_alternation_backref_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (string-match "\\(?:ab\\|cd\\)+" "abcdab")
      (match-string 0 "abcdab")
      (string-match "\\(?:foo\\)\\(?:bar\\)" "foobar")
      (match-beginning 0)
      (string-match "\\(.\\)\\(.\\)\\1\\2" "abab")
      (match-string 0 "abab")
      (string-match "\\(.\\)\\1\\(.\\)\\2" "aabb")
      (match-string 0 "aabb"))
"##;
    let expect = expect_test::expect![[r#""OK (0 \"abcdab\" 0 0 0 \"abab\" 0 \"aabb\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_regexp_w_max_minimal_paren_star() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (string-match "\\w*" "  hello  ")
      (string-match "\\w+" "abc123def")
      (match-string 0 "abc123def")
      (string-match "a\\W*b" "a  !  b")
      (match-string 0 "a  !  b")
      (string-match "\\<word\\>" "a word here")
      (match-string 0 "a word here"))
"##;
    let expect = expect_test::expect![[r#""OK (0 0 \"abc123def\" 0 \"a  !  b\" 2 \"word\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

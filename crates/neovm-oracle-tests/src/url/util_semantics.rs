//! Oracle parity tests for GNU `url/url-util.el` URL utility semantics.
//!
//! GNU `url-util.el` implements percent encoding/decoding, query parsing, and
//! query construction in Elisp.  These tests pin exact public behavior around
//! newline handling, key normalization, empty values, and per-URI-component
//! allowed character masks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_url_unhex_string_newlines_plus_and_invalid_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'url-util)
  (list
   (url-unhex-string nil)
   (url-unhex-string "a%20b%2Bc")
   (url-unhex-string "line%0Afeed%0Dcarriage")
   (url-unhex-string "line%0Afeed%0Dcarriage" t)
   (url-unhex-string "plus+is+literal")
   (url-unhex-string "%zz%4G%")))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\" \"a b+c\" \"line feed carriage\" \"line\\nfeed\\rcarriage\" \"plus+is+literal\" \"%zz%4G%\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_url_hexify_string_default_utf8_and_allowed_masks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'url-util)
  (list
   (url-hexify-string "AZaz09-_.~")
   (url-hexify-string "a b+c&d=e")
   (url-hexify-string "snowman ☃")
   (url-hexify-string "a/b?c=d&e" url-path-allowed-chars)
   (url-hexify-string "a/b?c=d&e" url-query-allowed-chars)
   (url-hexify-string "a/b?c=d&e" url-query-key-value-allowed-chars)
   (url-hexify-string "%already" url--query-key-value-preserved-chars)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"AZaz09-_.~\" \"a%20b%2Bc%26d%3De\" \"snowman%20%E2%98%83\" \"a/b%3Fc=d&e\" \"a/b?c=d&e\" \"a/b?c%3Dd%26e\" \"%25already\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_url_parse_query_string_grouping_and_downcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'url-util)
  (list
   (url-parse-query-string "a=1&b=two;c=3")
   (url-parse-query-string "A=1&a=2" t)
   (url-parse-query-string "empty=&missing&repeat=one&repeat=two")
   (url-parse-query-string "plus=a+b&space=a%20b")
   (url-parse-query-string "line=x%0Ay" nil nil)
   (url-parse-query-string "line=x%0Ay" nil t)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((\"c\" \"3\") (\"b\" \"two\") (\"a\" \"1\")) ((\"a\" \"2\" \"1\")) ((\"repeat\" \"two\" \"one\") (\"missing\" \"\") (\"empty\" \"\")) ((\"space\" \"a b\") (\"plus\" \"a+b\")) ((\"line\" \"x y\")) ((\"line\" \"x\\ny\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_url_build_query_string_empty_values_and_separators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'url-util)
  (list
   (url-build-query-string '((key1 val1)
                             (key2 "two words")
                             (key3 "a&b" "c=d")
                             (key4)
                             (key5 "")))
   (url-build-query-string '((key1 val1)
                             (key2 "two words")
                             (key3 "a&b" "c=d")
                             (key4)
                             (key5 "")) t)
   (url-build-query-string '((key4) (key5 "")) nil t)
   (url-build-query-string '((:keyword value) ("string key" "string value")))
   (url-build-query-string '((percent "%already") (slash "a/b")))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"key1=val1&key2=two%20words&key3=a%26b&key3=c%3Dd&key4&key5\" \"key1=val1;key2=two%20words;key3=a%26b;key3=c%3Dd;key4;key5\" \"key4=&key5=\" \":keyword=value&string%20key=string%20value\" \"percent=%25already&slash=a/b\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

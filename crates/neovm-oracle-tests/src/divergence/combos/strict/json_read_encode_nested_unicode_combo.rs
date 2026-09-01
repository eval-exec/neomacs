//! Strict combo oracle probes, batch 218: JSON parsing + encoding deep.
//! json-read-from-string over nested objects/arrays/scalars/unicode, and
//! json-encode of alists/plists/arrays. json-object-type bound to alist for
//! deterministic output.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_json_read_nested_arrays_objects_scalars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'json)
(let ((json-object-type 'alist)
      (json-array-type 'list)
      (json-false :json-false)
      (json-null nil))
  (list (json-read-from-string "{\"a\": 1, \"b\": [2, 3, 4], \"c\": {\"d\": true, \"e\": false}}")
        (json-read-from-string "[1, 2, 3]")
        (json-read-from-string "\"hello\"")
        (json-read-from-string "42")
        (json-read-from-string "3.14")
        (json-read-from-string "null")
        (json-read-from-string "true")
        (json-read-from-string "[{\"k\": \"v\"}, null, [1]]")))
"##;
    let expect = expect_test::expect![[
        r#""OK (((a . 1) (b 2 3 4) (c (d . t) (e . :json-false))) (1 2 3) \"hello\" 42 3.14 nil t (((k . \"v\")) nil (1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_json_encode_alist_plist_arrays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'json)
(let ((json-object-type 'alist))
  (list (json-encode-alist '((a . 1) (b . 2) (c . "str")))
        (json-encode-array '(1 2 3))
        (json-encode '((a . 1) (b . (2 3))))
        (json-encode-string "with \"quotes\" and \\backslash")
        (json-encode-string "unicode: 日本語 café")
        (json-encode-number 42)
        (json-encode-number 3.14)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"{\\\"a\\\":1,\\\"b\\\":2,\\\"c\\\":\\\"str\\\"}\" \"[1,2,3]\" \"{\\\"a\\\":1,\\\"b\\\":[2,3]}\" \"\\\"with \\\\\\\"quotes\\\\\\\" and \\\\\\\\backslash\\\"\" \"\\\"unicode: 日本語 café\\\"\" \"42\" \"3.14\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_json_roundtrip_unicode_escape_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'json)
(let ((json-object-type 'alist))
  (list (json-read-from-string "\"\\u00e9\"")
        (json-read-from-string "\"line\\nbreak\\ttab\"")
        (json-read-from-string "\"escaped \\\\u s l a s h\"")
        (json-encode (json-read-from-string "{\"x\": [1, {\"y\": 2}], \"z\": \"café\"}"))
        (json-read-from-string "  {  \"spaces\"  :  1  }  ")
        (condition-case err (json-read-from-string "{bad json")
          (json-error-format 'caught)
          (error (cons 'other-err (car err))))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"é\" \"line\\nbreak\ttab\" \"escaped \\\\u s l a s h\" \"{\\\"x\\\":[1,{\\\"y\\\":2}],\\\"z\\\":\\\"café\\\"}\" ((spaces . 1)) (other-err . json-end-of-file))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

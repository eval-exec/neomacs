//! json-serialize / json-parse-string parity: objects/arrays, object-type
//! alist/plist/hash, special values, numbers, roundtrips, plus the unibyte
//! vs multibyte result-string divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn json_alist_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"{\\\"a\\\":1}\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(json-serialize '((a . 1)) :false-object :false :null-object :null)"##,
        expect,
    );
}

#[test]
fn json_numbers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (3.14 42 -17 1000.0 \"{\\\"x\\\":3.5,\\\"y\\\":100}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (json-parse-string "3.14") (json-parse-string "42")
        (json-parse-string "-17") (json-parse-string "1e3")
        (json-serialize '((x . 3.5) (y . 100))))"##,
        expect,
    );
}

#[test]
fn json_parse_object_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#s(hash-table test equal data (\"a\" 1 \"b\" [2 3])) ((a . 1)) (:a 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (json-parse-string "{\"a\":1,\"b\":[2,3]}")
        (json-parse-string "{\"a\":1}" :object-type 'alist)
        (json-parse-string "{\"a\":1}" :object-type 'plist))"##,
        expect,
    );
}

#[test]
fn json_parse_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (t :false :null NIL [] #s(hash-table test equal))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (json-parse-string "true") (json-parse-string "false")
        (json-parse-string "null") (json-parse-string "null" :null-object 'NIL)
        (json-parse-string "[]") (json-parse-string "{}"))"##,
        expect,
    );
}

#[test]
fn json_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "{\"k\":[1,2,{\"n\":true}],\"s\":\"v\"}"))
  (string= s (json-serialize (json-parse-string s))))"##,
        expect,
    );
}

#[test]
fn json_serialize_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"{\\\"a\\\":1,\\\"b\\\":\\\"x\\\",\\\"c\\\":true,\\\"d\\\":false,\\\"e\\\":null}\" \"[1,2,3]\" \"{}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (json-serialize '((a . 1) (b . "x") (c . t) (d . :false) (e . :null)))
        (json-serialize [1 2 3])
        (json-serialize (make-hash-table)))"##,
        expect,
    );
}

#[test]
fn json_serialize_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"{\\\"name\\\":\\\"test\\\",\\\"nums\\\":[1,2,3],\\\"obj\\\":{\\\"k\\\":\\\"v\\\"}}\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(json-serialize '((name . "test") (nums . [1 2 3]) (obj . ((k . "v")))))"##,
        expect,
    );
}

#[test]
fn divergence_json_serialize_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 12 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (json-serialize ["é" "⚡"])))
  (list (multibyte-string-p s)
        (string-bytes s)
        (length s)))"##,
        expect,
    );
}

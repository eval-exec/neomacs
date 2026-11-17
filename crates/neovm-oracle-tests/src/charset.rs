//! Oracle parity tests for charset primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_charset_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list (char-charset ?A) (charsetp (char-charset ?A)) (encode-char ?A 'ucs) (decode-char 'ucs #x41) (encode-char ?😀 'ucs) (decode-char 'ucs #x1F600))";
    let expect = expect_test::expect![[r#""OK (ascii t 65 65 128512 128512)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(ascii t 65 65 128512 128512)", &oracle, &neovm);
}

#[test]
fn oracle_prop_char_charset_classification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form =
        "(list (char-charset ?A) (char-charset ?é) (char-charset ?😀) (char-charset ?\\x80))";
    let expect = expect_test::expect![[r#""OK (ascii unicode-bmp unicode unicode-bmp)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(ascii unicode-bmp unicode unicode-bmp)", &oracle, &neovm);
}

#[test]
fn oracle_prop_encode_char_unknown_charset_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument charsetp neovm-no-such-charset)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(encode-char ?A 'neovm-no-such-charset)",
        expect,
    );
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_decode_char_out_of_range_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Not an in-range integer, integral float, or cons of integers\")""#
    ]];
    crate::common::assert_oracle_parity_expect("(decode-char 'ucs -1)", expect);
}

#[test]
fn oracle_prop_iso_charset_designations_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU `Fiso_charset` (charset.c:2089): lookup in `iso_charset_table`,
    // where a later definition wins (`thai-tis620` over `thai-iso8859-11`),
    // plus the `check_iso_charset_parameter` errors.
    let form = "(list (iso-charset 1 94 ?B) (iso-charset 2 94 ?B) (iso-charset 1 96 ?A) \
        (iso-charset 2 94 ?A) (iso-charset 1 94 ?Z) (iso-charset 1 96 ?T) \
        (condition-case e (iso-charset 0 94 ?B) (error e)) \
        (condition-case e (iso-charset 1 1 ?A) (error e)) \
        (condition-case e (iso-charset 1 94 ?/) (error e)) \
        (condition-case e (iso-charset nil 94 ?B) (error e)) \
        (condition-case e (iso-charset 1 94 -1) (error e)))";
    let expect = expect_test::expect![[
        r#""OK (ascii japanese-jisx0208 latin-iso8859-1 chinese-gb2312 nil thai-tis620 (error \"Invalid DIMENSION 0, it should be 1, 2, or 3\") (error \"Invalid CHARS 1, it should be 94 or 96\") (error \"Invalid FINAL-CHAR ‘/’, it should be ‘0’..‘~’\") (wrong-type-argument fixnump nil) (wrong-type-argument characterp -1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_declare_equiv_charset_and_alias_designations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU `Fdeclare_equiv_charset` (charset.c:1428) stores CHARSET's id in
    // `iso_charset_table`, so an alias stands for its target. GNU
    // `Fdefine_charset_alias` (charset.c:1289) only rebinds a name, so a
    // charset whose name becomes an alias keeps its designation. CHARSET is
    // checked before the ISO parameters.
    let form = "(list (progn (declare-equiv-charset 1 94 ?B 'unicode) (iso-charset 1 94 ?B)) \
        (progn (define-charset-alias 'oracle-equiv-alias 'latin-iso8859-1) \
        (declare-equiv-charset 1 94 ?B 'oracle-equiv-alias) (iso-charset 1 94 ?B)) \
        (progn (define-charset-alias 'latin-iso8859-1 'ascii) (iso-charset 1 96 ?A)) \
        (condition-case e (declare-equiv-charset 0 94 ?B 'nope) (error e)) \
        (condition-case e (declare-equiv-charset 0 94 ?B 'ascii) (error e)))";
    let expect = expect_test::expect![[
        r#""OK (unicode latin-iso8859-1 latin-iso8859-1 (wrong-type-argument charsetp nope) (error \"Invalid DIMENSION 0, it should be 1, 2, or 3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

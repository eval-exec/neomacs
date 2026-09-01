/// Batch 522: further print/read edge cases - read syntax errors, prin1 edge.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx522_read_bad_character() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#\\\\\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (read-from-string "#\\zZzZz") (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_cx522_read_unmatched_paren() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (read-from-string "(1 2 3") (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_cx522_read_unmatched_bracket() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (read-from-string "[1 2 3") (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_cx522_read_unmatched_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (read-from-string "\"abc") (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_cx522_read_invalid_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"z\" . 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (read-from-string "\"\\z\"" ) (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_cx522_print_char_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\\"hello\\\\nworld\\\\351\\\"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((print-escape-newlines t) (print-escape-nonascii t))
  (prin1-to-string "hello\nworld\xe9"))
"##,
        expect,
    );
}

#[test]
fn div_cx522_print_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"[1 2 3]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(prin1-to-string [1 2 3])
"##,
        expect,
    );
}

#[test]
fn div_cx522_print_bool_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#&3\\\"\u{5}\\\"\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(prin1-to-string (bool-vector t nil t))
"##,
        expect,
    );
}

#[test]
fn div_cx522_print_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#^[119 nil syntax-table 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119 119]\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ct (make-char-table 'syntax-table ?w)))
  (prin1-to-string ct))
"##,
        expect,
    );
}

#[test]
fn div_cx522_read_propertized_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK #(\"hello\" 0 5 (face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(car (read-from-string "#(\"hello\" 0 5 (face bold))"))
"##,
        expect,
    );
}

#[test]
fn div_cx522_read_bad_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1.0.0 . 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (read-from-string "1.0.0") (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_cx522_read_bad_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (12a . 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (read-from-string "12a") (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_cx522_print_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"HELLO\" \"Hello\" \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (prin1-to-string 'HELLO) (prin1-to-string 'Hello) (prin1-to-string 'hello))
"##,
        expect,
    );
}

#[test]
fn div_cx522_print_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \":test-keyword\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(prin1-to-string :test-keyword)
"##,
        expect,
    );
}

#[test]
fn div_cx522_read_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(car (read-from-string "nil"))
"##,
        expect,
    );
}

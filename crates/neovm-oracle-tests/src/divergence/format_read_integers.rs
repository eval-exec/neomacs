//! Divergence tests: print integers, read integers, format integers.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_format_hex_octal_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 6 20)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list\n  (format \"%x\" 255)\n  (format \"%X\" 255)\n  (format \"%o\" 8)\n  (format \"%#x\" 255)\n  (format \"%#o\" 8)))",
        expect,
    );
}

#[test]
fn divergence_read_hex_octal_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (255 63 10 0 42)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list\n  (string-to-number \"ff\" 16)\n  (string-to-number \"77\" 8)\n  (string-to-number \"1010\" 2)\n  (string-to-number \"0xff\" 16)\n  (string-to-number \"42\" 10))",
        expect,
    );
}

#[test]
fn divergence_prin1_large_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"18446744073709551616\" 18446744073709551616 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((big (expt 2 64)))
  (list (prin1-to-string big)
        (string-to-number (prin1-to-string big))
        (= (string-to-number (prin1-to-string big)) big)))"#,
        expect,
    );
}

#[test]
fn divergence_read_negative_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((-42 . 3) ((- 42) . 6) -42)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "-42")
  (read-from-string "(- 42)")
  (car (read-from-string "-42"))) "#,
        expect,
    );
}

#[test]
fn divergence_format_escaped_percent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"100%\" \"42%\" \"99.9%\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "100%%")
  (format "%d%%" 42)
  (format "%.1f%%" 99.9))"#,
        expect,
    );
}

#[test]
fn divergence_format_zero_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"00042\" \"00000000\" \"000ff\" \"0010\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%05d" 42)
  (format "%08d" 0)
  (format "%05x" 255)
  (format "%04o" 8))"#,
        expect,
    );
}

#[test]
fn divergence_print_symbol_with_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"foo-bar\" \"foo_bar\" \"foo::bar\" \"foo-bar\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (prin1-to-string 'foo-bar)
  (prin1-to-string 'foo_bar)
  (prin1-to-string 'foo::bar)
  (symbol-name 'foo-bar))"#,
        expect,
    );
}

#[test]
fn divergence_read_string_escape_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\\\"\" 4 43)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (read-from-string "\"hello\\nworld\"")
  (read-from-string "\"tab\\there\"")
  (read-from-string "\"back\\\\slash\""))#" ,
    );
}

#[test]
fn divergence_print_cons_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    assert_oracle_parity(
        r#"(list
  (prin1-to-string '(a . b))
  (prin1-to-string '(a b c))
  (prin1-to-string '(a b . c)))"#,
        expect,
    );
}

#[test]
fn divergence_print_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"[1 2 3]\" \"[]\" \"[a \\\"b\\\" (c d)]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (prin1-to-string [1 2 3])
  (prin1-to-string [])
  (prin1-to-string [a "b" (c d)]))"#,
        expect,
    );
}

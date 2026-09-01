//! Divergence tests: format specifiers, width, precision, padding.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_format_integers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"42\" \"-42\" \"0\" \"+42\" \" 42\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%d" 42)
  (format "%d" -42)
  (format "%d" 0)
  (format "%+d" 42)
  (format "% d" 42)) "#,
        expect,
    );
}

#[test]
fn divergence_format_hex_octal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ff\" \"FF\" \"0xff\" \"10\" \"010\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%x" 255)
  (format "%X" 255)
  (format "%#x" 255)
  (format "%o" 8)
  (format "%#o" 8)) "#,
        expect,
    );
}

#[test]
fn divergence_format_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"   42\" \"42   \" \"00042\" \"        hi\" \"hi        |\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%5d" 42)
  (format "%-5d" 42)
  (format "%05d" 42)
  (format "%10s" "hi")
  (format "%-10s|" "hi")) "#,
        expect,
    );
}

#[test]
fn divergence_format_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"3.140000\" \"3.14\" \"3.140000e+00\" \"3.14\" \"3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%f" 3.14)
  (format "%.2f" 3.14159)
  (format "%e" 3.14)
  (format "%g" 3.14)
  (format "%.0f" 3.14)) "#,
        expect,
    );
}

#[test]
fn divergence_format_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Hello World\" \"\\\"hello\\\"\" \"%\" \"A\" \"<<nil>>\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "Hello %s" "World")
  (format "%S" "hello")
  (format "%%")
  (format "%c" 65)
  (format "<<%s>>" nil)) "#,
        expect,
    );
}

#[test]
fn divergence_format_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Alice is 30 years old\" \"1 + 2 = 3\" \"list: (1 2 3)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%s is %d years old" "Alice" 30)
  (format "%d + %d = %d" 1 2 3)
  (format "list: %S" '(1 2 3))) "#,
        expect,
    );
}

#[test]
fn divergence_format_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'format-time-string)
  (stringp (format-time-string "%Y-%m-%d"))
  (stringp (format-time-string "%H:%M:%S"))
  (stringp (format-time-string "%A, %B %d, %Y"))) "#,
        expect,
    );
}

#[test]
fn divergence_format_seconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'format-seconds)
  (fboundp 'seconds-to-string)
  (stringp (seconds-to-string 90))
  (stringp (seconds-to-string 3661))) "#,
        expect,
    );
}

#[test]
fn divergence_format_spec_padding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"     3.140\" \"3.140     \" \"+3.140\" \" 3.140\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format "%10.3f" 3.14)
  (format "%-10.3f" 3.14)
  (format "%+.3f" 3.14)
  (format "% .3f" 3.14)) "#,
        expect,
    );
}

#[test]
fn divergence_format_propertized() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'format-propertized)
  (fboundp 'format-message)
  (stringp (format-message "`foo' and `bar'"))
  (stringp (format-message "this is `foo'"))) "#,
        expect,
    );
}

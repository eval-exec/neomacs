//! Oracle parity tests for advanced `format` patterns:
//! all format specs (`%d`, `%x`, `%o`, `%e`, `%f`, `%g`, `%s`, `%S`,
//! `%c`), width/padding/precision, and complex formatting pipelines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Integer format specs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_integer_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (format "%d" 42)
                        (format "%d" -42)
                        (format "%d" 0)
                        (format "%x" 255)
                        (format "%X" 255)
                        (format "%o" 8)
                        (format "%o" 255))"#;
    let expect =
        expect_test::expect![[r#""OK (\"42\" \"-42\" \"0\" \"ff\" \"FF\" \"10\" \"377\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Width and padding
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_width_padding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (format "%10d" 42)
                        (format "%-10d" 42)
                        (format "%010d" 42)
                        (format "%10s" "hello")
                        (format "%-10s" "hello")
                        (format "%5d" 123456))"#;
    let expect = expect_test::expect![[
        r#""OK (\"        42\" \"42        \" \"0000000042\" \"     hello\" \"hello     \" \"123456\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Float format specs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_float_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (format "%f" 3.14159)
                        (format "%.2f" 3.14159)
                        (format "%.0f" 3.14159)
                        (format "%e" 12345.6789)
                        (format "%.2e" 12345.6789)
                        (format "%g" 0.00001)
                        (format "%g" 12345.0))"#;
    let expect = expect_test::expect![[
        r#""OK (\"3.141590\" \"3.14\" \"3\" \"1.234568e+04\" \"1.23e+04\" \"1e-05\" \"12345\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_format_float_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (format "%10.2f" 3.14)
                        (format "%-10.2f" 3.14)
                        (format "%010.2f" 3.14))"#;
    let expect = expect_test::expect![[r#""OK (\"      3.14\" \"3.14      \" \"0000003.14\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Character format
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (format "%c" 65)
                        (format "%c" ?a)
                        (format "%c" ?Z)
                        (format "%c%c%c" ?H ?i ?!))"#;
    let expect = expect_test::expect![[r#""OK (\"A\" \"a\" \"Z\" \"Hi!\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// %s vs %S (prin1 vs princ)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_s_vs_S() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (format "%s" "hello")
                        (format "%S" "hello")
                        (format "%s" 42)
                        (format "%S" 42)
                        (format "%s" '(a b c))
                        (format "%S" '(a b c))
                        (format "%s" nil)
                        (format "%S" nil))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"\\\"hello\\\"\" \"42\" \"42\" \"(a b c)\" \"(a b c)\" \"nil\" \"nil\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_multiple_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    (format "%s is %d years old" "Alice" 30)
                    (format "[%5d] %-20s %6.2f" 1 "item" 9.99)
                    (format "%s + %s = %s" 1 2 (+ 1 2))
                    (format "0x%04X = %d = 0%o" 255 255 255))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice is 30 years old\" \"[    1] item                   9.99\" \"1 + 2 = 3\" \"0x00FF = 255 = 0377\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// %% literal percent
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_literal_percent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list (format "100%%")
                        (format "%d%%" 42)
                        (format "%%s is not a format"))"#;
    let expect = expect_test::expect![[r#""OK (\"100%\" \"42%\" \"%s is not a format\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: table formatter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a formatted table
    let form = r#"(let ((header (format "%-12s %6s %10s" "Name" "Age" "Score"))
                        (sep (make-string 30 ?-))
                        (rows '(("Alice" 30 95.5)
                                ("Bob" 25 87.2)
                                ("Carol" 35 92.8))))
                    (let ((formatted-rows
                           (mapcar
                            (lambda (row)
                              (format "%-12s %6d %10.1f"
                                      (nth 0 row)
                                      (nth 1 row)
                                      (nth 2 row)))
                            rows)))
                      (mapconcat #'identity
                                 (append (list header sep)
                                         formatted-rows)
                                 "\n")))"#;
    let expect = expect_test::expect![[
        r#""OK \"Name            Age      Score\\n------------------------------\\nAlice            30       95.5\\nBob              25       87.2\\nCarol            35       92.8\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: sprintf-like number formatter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_number_formatter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Format numbers in various human-readable ways
    let form = r#"(let ((format-bytes
                         (lambda (n)
                           (cond
                            ((>= n (* 1024 1024 1024))
                             (format "%.1f GB" (/ (float n) 1024 1024 1024)))
                            ((>= n (* 1024 1024))
                             (format "%.1f MB" (/ (float n) 1024 1024)))
                            ((>= n 1024)
                             (format "%.1f KB" (/ (float n) 1024)))
                            (t (format "%d B" n))))))
                    (mapcar format-bytes
                            '(42 1024 1536 1048576 1073741824
                              5368709120)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"42 B\" \"1.0 KB\" \"1.5 KB\" \"1.0 MB\" \"1.0 GB\" \"5.0 GB\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

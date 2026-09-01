//! Oracle parity tests for advanced format string edge cases:
//! all format directives with parameter variations, field width, padding,
//! precision, left-justify flags, nested format calls, format with special
//! chars/unicode, %S vs %s differences, float precision edge cases,
//! and format producing very long strings.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// %s (princ) vs %S (prin1) differences with various types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_s_vs_S_differences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    // String: %s strips quotes, %S keeps them
    crate::common::assert_oracle_parity_expect(r#"(format "%s" "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\\\"hello\\\"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" "hello")"#, expect);

    let expect = expect_test::expect![[r#""OK \"line1\\nline2\"""#]];
    // String with special chars
    crate::common::assert_oracle_parity_expect(r#"(format "%s" "line1\nline2")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\\\"line1\\nline2\\\"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" "line1\nline2")"#, expect);

    let expect = expect_test::expect![[r#""OK \"nil\"""#]];
    // nil
    crate::common::assert_oracle_parity_expect(r#"(format "%s" nil)"#, expect);
    let expect = expect_test::expect![[r#""OK \"nil\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" nil)"#, expect);

    let expect = expect_test::expect![[r#""OK \"t\"""#]];
    // t
    crate::common::assert_oracle_parity_expect(r#"(format "%s" t)"#, expect);
    let expect = expect_test::expect![[r#""OK \"t\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" t)"#, expect);

    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    // Symbols
    crate::common::assert_oracle_parity_expect(r#"(format "%s" 'hello)"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" 'hello)"#, expect);

    let expect = expect_test::expect![[r#""OK \"(1 2 3)\"""#]];
    // Lists
    crate::common::assert_oracle_parity_expect(r#"(format "%s" '(1 2 3))"#, expect);
    let expect = expect_test::expect![[r#""OK \"(1 2 3)\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" '(1 2 3))"#, expect);

    let expect = expect_test::expect![[r#""OK \"(a (b c) (d (e f)))\"""#]];
    // Nested lists
    crate::common::assert_oracle_parity_expect(r#"(format "%s" '(a (b c) (d (e f))))"#, expect);
    let expect = expect_test::expect![[r#""OK \"(a (b c) (d (e f)))\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" '(a (b c) (d (e f))))"#, expect);

    let expect = expect_test::expect![[r#""OK \"[1 2 3]\"""#]];
    // Vectors
    crate::common::assert_oracle_parity_expect(r#"(format "%s" [1 2 3])"#, expect);
    let expect = expect_test::expect![[r#""OK \"[1 2 3]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" [1 2 3])"#, expect);

    let expect = expect_test::expect![[r#""OK \"(a . b)\"""#]];
    // Cons pairs (dotted)
    crate::common::assert_oracle_parity_expect(r#"(format "%s" '(a . b))"#, expect);
    let expect = expect_test::expect![[r#""OK \"(a . b)\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" '(a . b))"#, expect);

    let expect = expect_test::expect![[r#""OK \"\\\"he said \\\\\\\"hi\\\\\\\"\\\"\"""#]];
    // String with embedded quotes
    crate::common::assert_oracle_parity_expect(r#"(format "%S" "he said \"hi\"")"#, expect);

    let expect = expect_test::expect![[r#""OK \"65\"""#]];
    // Characters
    crate::common::assert_oracle_parity_expect(r#"(format "%s" ?A)"#, expect);
    let expect = expect_test::expect![[r#""OK \"65\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" ?A)"#, expect);
}

// ---------------------------------------------------------------------------
// %c with various character values including unicode
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_c_characters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"A\"""#]];
    // ASCII printable
    crate::common::assert_oracle_parity_expect(r#"(format "%c" 65)"#, expect);
    let expect = expect_test::expect![[r#""OK \"z\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%c" 122)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%c" 48)"#, expect);

    let expect = expect_test::expect![[r#""OK \" \"""#]];
    // Space and special ASCII
    crate::common::assert_oracle_parity_expect(r#"(format "%c" 32)"#, expect);
    let expect = expect_test::expect![[r#""OK \"~\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%c" 126)"#, expect);

    let expect = expect_test::expect![[r#""OK \"α\"""#]];
    // Unicode characters
    crate::common::assert_oracle_parity_expect(r#"(format "%c" #x03B1)"#, expect);
    let expect = expect_test::expect![[r#""OK \"β\"""#]];
    // alpha
    crate::common::assert_oracle_parity_expect(r#"(format "%c" #x03B2)"#, expect);
    let expect = expect_test::expect![[r#""OK \"世\"""#]];
    // beta
    crate::common::assert_oracle_parity_expect(r#"(format "%c" #x4e16)"#, expect);
    let expect = expect_test::expect![[r#""OK \"Hello\"""#]];
    // CJK char

    // Multiple %c in one format
    crate::common::assert_oracle_parity_expect(
        r#"(format "%c%c%c%c%c" 72 101 108 108 111)"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"[    A]\"""#]];
    // %c with width
    crate::common::assert_oracle_parity_expect(r#"(format "[%5c]" 65)"#, expect);
    let expect = expect_test::expect![[r#""OK \"[A    ]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-5c]" 65)"#, expect);
}

// ---------------------------------------------------------------------------
// %d with all flag combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_d_all_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    // Basic
    crate::common::assert_oracle_parity_expect(r#"(format "%d" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"-42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%d" -42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%d" 0)"#, expect);

    let expect = expect_test::expect![[r#""OK \"        42\"""#]];
    // Width
    crate::common::assert_oracle_parity_expect(r#"(format "%10d" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"       -42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%10d" -42)"#, expect);
    let expect = expect_test::expect![[r#""OK \" 42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%3d" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%1d" 42)"#, expect);

    let expect = expect_test::expect![[r#""OK \"0000000042\"""#]];
    // Zero-padding
    crate::common::assert_oracle_parity_expect(r#"(format "%010d" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"-000000042\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%010d" -42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"00000\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%05d" 0)"#, expect);

    let expect = expect_test::expect![[r#""OK \"[42        ]\"""#]];
    // Left-justify
    crate::common::assert_oracle_parity_expect(r#"(format "[%-10d]" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"[-42       ]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-10d]" -42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"[42 ]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-3d]" 42)"#, expect);

    let expect = expect_test::expect![[r#""OK \"+42\"""#]];
    // Plus sign
    crate::common::assert_oracle_parity_expect(r#"(format "%+d" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"-42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%+d" -42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"+0\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%+d" 0)"#, expect);

    let expect = expect_test::expect![[r#""OK \"[       +42]\"""#]];
    // Combined flags
    crate::common::assert_oracle_parity_expect(r#"(format "[%+10d]" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"[+42       ]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-+10d]" 42)"#, expect);

    let expect = expect_test::expect![[r#""OK \"1000000000\"""#]];
    // Large numbers
    crate::common::assert_oracle_parity_expect(r#"(format "%d" 1000000000)"#, expect);
    let expect = expect_test::expect![[r#""OK \"-1000000000\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%d" -1000000000)"#, expect);
}

// ---------------------------------------------------------------------------
// %o (octal) and %x/%X (hex) with flags
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_octal_hex_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"10\"""#]];
    // Basic octal
    crate::common::assert_oracle_parity_expect(r#"(format "%o" 8)"#, expect);
    let expect = expect_test::expect![[r#""OK \"377\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%o" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%o" 0)"#, expect);
    let expect = expect_test::expect![[r#""OK \"777\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%o" 511)"#, expect);

    let expect = expect_test::expect![[r#""OK \"       377\"""#]];
    // Octal with width
    crate::common::assert_oracle_parity_expect(r#"(format "%10o" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0000000377\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%010o" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"[377       ]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-10o]" 255)"#, expect);

    let expect = expect_test::expect![[r#""OK \"ff\"""#]];
    // Hex lowercase
    crate::common::assert_oracle_parity_expect(r#"(format "%x" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"1000\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%x" 4096)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%x" 0)"#, expect);

    let expect = expect_test::expect![[r#""OK \"FF\"""#]];
    // Hex uppercase
    crate::common::assert_oracle_parity_expect(r#"(format "%X" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"1000\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%X" 4096)"#, expect);
    let expect = expect_test::expect![[r#""OK \"BEEF\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%X" 48879)"#, expect);
    let expect = expect_test::expect![[r#""OK \"000000ff\"""#]];
    // 0xBEEF

    // Hex with width and zero-pad
    crate::common::assert_oracle_parity_expect(r#"(format "%08x" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"000000FF\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%08X" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"[ff        ]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-10x]" 255)"#, expect);
    let expect = expect_test::expect![[r#""OK \"[FF        ]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-10X]" 255)"#, expect);

    let expect = expect_test::expect![[r#""OK \"d=42 o=52 x=2a X=2A\"""#]];
    // All integer formats in one call
    crate::common::assert_oracle_parity_expect(
        r#"(format "d=%d o=%o x=%x X=%X" 42 42 42 42)"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// %e, %f, %g with precision variations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_float_precision_extensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"3\"""#]];
    // %f with various precisions
    crate::common::assert_oracle_parity_expect(r#"(format "%.0f" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.1\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.1f" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.14\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.2f" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.14159\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.5f" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.1415900000\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.10f" 3.14159)"#, expect);

    let expect = expect_test::expect![[r#""OK \"      3.14\"""#]];
    // %f with width and precision
    crate::common::assert_oracle_parity_expect(r#"(format "%10.2f" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"     -3.14\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%10.2f" -3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0000003.14\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%010.2f" 3.14)"#, expect);
    let expect = expect_test::expect![[r#""OK \"[3.14      ]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-10.2f]" 3.14)"#, expect);

    let expect = expect_test::expect![[r#""OK \"3e+00\"""#]];
    // %e with precisions
    crate::common::assert_oracle_parity_expect(r#"(format "%.0e" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.14e+00\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.2e" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.14159e+00\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.5e" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"1.00e-03\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.2e" 0.001)"#, expect);
    let expect = expect_test::expect![[r#""OK \"1.23e+05\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.2e" 123456.789)"#, expect);

    let expect = expect_test::expect![[r#""OK \"      3.142e+00\"""#]];
    // %e with width
    crate::common::assert_oracle_parity_expect(r#"(format "%15.3e" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"[3.142e+00      ]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-15.3e]" 3.14159)"#, expect);

    let expect = expect_test::expect![[r#""OK \"3.14159\"""#]];
    // %g chooses between %f and %e style
    crate::common::assert_oracle_parity_expect(r#"(format "%g" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"100000\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%g" 100000.0)"#, expect);
    let expect = expect_test::expect![[r#""OK \"0.0001\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%g" 0.0001)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.1\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.2g" 3.14159)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.14159\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.10g" 3.14159)"#, expect);

    let expect = expect_test::expect![[r#""OK \"-0.000\"""#]];
    // Negative floats
    crate::common::assert_oracle_parity_expect(r#"(format "%.3f" -0.0)"#, expect);
    let expect = expect_test::expect![[r#""OK \"-1.500e+00\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.3e" -1.5)"#, expect);
    let expect = expect_test::expect![[r#""OK \"+3.14\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%+.2f" 3.14)"#, expect);

    let expect = expect_test::expect![[r#""OK \"1.000000e-15\"""#]];
    // Very small / very large
    crate::common::assert_oracle_parity_expect(r#"(format "%e" 1e-15)"#, expect);
    let expect = expect_test::expect![[r#""OK \"1.000000e+15\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%e" 1e15)"#, expect);
    let expect = expect_test::expect![[r#""OK \"10000000000.000000\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%f" 1e10)"#, expect);
}

// ---------------------------------------------------------------------------
// Nested format calls
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_nested_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"00042\"""#]];
    // Format producing a format string that is then used
    crate::common::assert_oracle_parity_expect(r#"(format (format "%%0%dd" 5) 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello     \"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format (format "%%-%ds" 10) "hello")"#, expect);

    let expect = expect_test::expect![[r#""OK \"result: 3 + 4 = 7\"""#]];
    // Nested format as argument
    crate::common::assert_oracle_parity_expect(
        r#"(format "result: %s" (format "%d + %d = %d" 3 4 7))"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK \"[00042] [hi        ]\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(format "[%s] [%s]"
                                    (format "%05d" 42)
                                    (format "%-10s" "hi"))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"outer(mid(inner(42)))\"""#]];
    // Triple nesting
    crate::common::assert_oracle_parity_expect(
        r#"(format "outer(%s)"
                                    (format "mid(%s)"
                                            (format "inner(%d)" 42)))"#,
        expect,
    );

    // Format in a loop building a string
    let form = r#"(let ((parts nil) (i 0))
  (while (< i 5)
    (setq parts (cons (format "[%02d:%s]" i (make-string (1+ i) ?*)) parts))
    (setq i (1+ i)))
  (mapconcat #'identity (nreverse parts) "-"))"#;
    let expect = expect_test::expect![[r#""OK \"[00:*]-[01:**]-[02:***]-[03:****]-[04:*****]\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Format with nil, t, symbols, lists, vectors as arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_diverse_arg_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"nil t nil t\"""#]];
    // nil and t
    crate::common::assert_oracle_parity_expect(r#"(format "%s %s %S %S" nil t nil t)"#, expect);

    let expect = expect_test::expect![[r#""OK \"sym: hello hello\"""#]];
    // Symbols
    crate::common::assert_oracle_parity_expect(r#"(format "sym: %s %S" 'hello 'hello)"#, expect);
    let expect = expect_test::expect![[r#""OK \"with-special-chars\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" 'with-special-chars)"#, expect);
    let expect = expect_test::expect![[r#""OK \"with-special-chars\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" 'with-special-chars)"#, expect);

    let expect = expect_test::expect![[r#""OK \"(1 2 3)\"""#]];
    // Lists
    crate::common::assert_oracle_parity_expect(r#"(format "%s" '(1 2 3))"#, expect);
    let expect = expect_test::expect![[r#""OK \"(1 \\\"two\\\" three)\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" '(1 "two" three))"#, expect);
    let expect = expect_test::expect![[r#""OK \"(a . b)\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" '(a . b))"#, expect);
    let expect = expect_test::expect![[r#""OK \"(a b . c)\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" '(a b . c))"#, expect);

    let expect = expect_test::expect![[r#""OK \"[1 2 3]\"""#]];
    // Vectors
    crate::common::assert_oracle_parity_expect(r#"(format "%s" [1 2 3])"#, expect);
    let expect = expect_test::expect![[r#""OK \"[1 \\\"two\\\" three]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" [1 "two" three])"#, expect);
    let expect = expect_test::expect![[r#""OK \"[]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" [])"#, expect);
    let expect = expect_test::expect![[r#""OK \"[]\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" [])"#, expect);

    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    // Integers as %s
    crate::common::assert_oracle_parity_expect(r#"(format "%s" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" 42)"#, expect);

    let expect = expect_test::expect![[r#""OK \"3.14\"""#]];
    // Floats as %s
    crate::common::assert_oracle_parity_expect(r#"(format "%s" 3.14)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3.14\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" 3.14)"#, expect);

    let expect = expect_test::expect![[r#""OK \"nil|(a b)|[1 2]|\\\"text\\\"|42\"""#]];
    // Mixed in one call
    crate::common::assert_oracle_parity_expect(
        r#"(format "%s|%S|%s|%S|%s"
                                    nil '(a b) [1 2] "text" 42)"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Format with %% literal percent
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_literal_percent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"100%\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "100%%")"#, expect);
    let expect = expect_test::expect![[r#""OK \"50%\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%d%%" 50)"#, expect);
    let expect = expect_test::expect![[r#""OK \"%d = 42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%%d = %d" 42)"#, expect);
    let expect = expect_test::expect![[r#""OK \"%%\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%%%%")"#, expect);
    let expect = expect_test::expect![[r#""OK \"task is 75% complete\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(format "%s is %d%% complete" "task" 75)"#,
        expect,
    );
}

// ---------------------------------------------------------------------------
// Format with unicode strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_unicode_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"cafe\u{301}\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s" "cafe\u0301")"#, expect);
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s %s" "hello" "world")"#, expect);

    let expect = expect_test::expect![[r#""OK \"αβγ\"""#]];
    // Width with unicode (interesting because char width != byte width)
    crate::common::assert_oracle_parity_expect(r#"(format "%s" "\u03b1\u03b2\u03b3")"#, expect);

    let expect = expect_test::expect![[r#""OK \"Greek: π, Number: 314\"""#]];
    // Mixed ASCII and unicode
    crate::common::assert_oracle_parity_expect(
        r#"(format "Greek: %s, Number: %d" "\u03c0" 314)"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"世界\"""#]];
    // CJK characters
    crate::common::assert_oracle_parity_expect(r#"(format "%s" "\u4e16\u754c")"#, expect);
}

// ---------------------------------------------------------------------------
// Format producing very long strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_long_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"                                                                                                   x\"""#
    ]];
    // Wide field with short content
    crate::common::assert_oracle_parity_expect(r#"(format "%100s" "x")"#, expect);
    let expect = expect_test::expect![[
        r#""OK \"[x                                                                                                   ]\"""#
    ]];
    crate::common::assert_oracle_parity_expect(r#"(format "[%-100s]" "x")"#, expect);

    let expect = expect_test::expect![[r#""OK 150""#]];
    // Multiple wide fields
    crate::common::assert_oracle_parity_expect(
        r#"(length (format "%50s%50s%50s" "a" "b" "c"))"#,
        expect,
    );

    // Repeated format in accumulation
    let form = r#"(let ((parts nil) (i 0))
  (while (< i 20)
    (setq parts (cons (format "%05d" (* i i)) parts))
    (setq i (1+ i)))
  (mapconcat #'identity (nreverse parts) ","))"#;
    let expect = expect_test::expect![[
        r#""OK \"00000,00001,00004,00009,00016,00025,00036,00049,00064,00081,00100,00121,00144,00169,00196,00225,00256,00289,00324,00361\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Format edge cases: empty string, no args, excess args
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    // No format specs
    crate::common::assert_oracle_parity_expect(r#"(format "hello world")"#, expect);
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "")"#, expect);

    let expect = expect_test::expect![[r#""OK \"42\"""#]];
    // Width of 0
    crate::common::assert_oracle_parity_expect(r#"(format "%0d" 42)"#, expect);

    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    // String arg wider than field
    crate::common::assert_oracle_parity_expect(r#"(format "%3s" "hello world")"#, expect);

    let expect = expect_test::expect![[r#""OK \"123456789\"""#]];
    // Integer arg wider than field
    crate::common::assert_oracle_parity_expect(r#"(format "%1d" 123456789)"#, expect);

    let expect = expect_test::expect![[r#""OK \"4\"""#]];
    // Float with precision 0
    crate::common::assert_oracle_parity_expect(r#"(format "%.0f" 3.5)"#, expect);
    let expect = expect_test::expect![[r#""OK \"3\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.0f" 3.4)"#, expect);
    let expect = expect_test::expect![[r#""OK \"-2\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%.0f" -2.5)"#, expect);

    let expect = expect_test::expect![[r#""OK \"1.000000000000000\"""#]];
    // Very large precision
    crate::common::assert_oracle_parity_expect(r#"(format "%.15f" 1.0)"#, expect);

    let expect = expect_test::expect![[r#""OK \"tab:\there\"""#]];
    // Format with special chars in literal parts
    crate::common::assert_oracle_parity_expect(r#"(format "tab:\there" )"#, expect);
    let expect = expect_test::expect![[r#""OK \"newline:\\n42\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "newline:\n%d" 42)"#, expect);

    let expect = expect_test::expect![[r#""OK \"1 2 3 4 5\"""#]];
    // Multiple format specs with same value types
    crate::common::assert_oracle_parity_expect(r#"(format "%d %d %d %d %d" 1 2 3 4 5)"#, expect);
    let expect = expect_test::expect![[r#""OK \"a b c\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%s %s %s" "a" "b" "c")"#, expect);
}

// ---------------------------------------------------------------------------
// Format table-building patterns (aligned columns)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_table_alignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a formatted table
    let form = r#"(let ((header (format "%-12s %8s %8s %10s" "Name" "Age" "Score" "Status"))
      (rows '(("Alice" 30 95 "pass")
              ("Bob" 25 67 "fail")
              ("Carol" 35 88 "pass")
              ("Dave" 28 72 "pass")))
      (lines nil))
  (setq lines (list header))
  (setq lines (cons (make-string (length header) ?-) lines))
  (dolist (row rows)
    (setq lines
          (cons (format "%-12s %8d %8d %10s"
                        (nth 0 row) (nth 1 row) (nth 2 row) (nth 3 row))
                lines)))
  (mapconcat #'identity (nreverse lines) "\n"))"#;
    let expect = expect_test::expect![[
        r#""OK \"Name              Age    Score     Status\\n-----------------------------------------\\nAlice              30       95       pass\\nBob                25       67       fail\\nCarol              35       88       pass\\nDave               28       72       pass\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Format with %s on list structures of varying depth
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_nested_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"((1 2) (3 4) (5 6))\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" '((1 2) (3 4) (5 6)))"#, expect);
    let expect = expect_test::expect![[r#""OK \"((a . 1) (b . 2) (c . 3))\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(format "%S" '((a . 1) (b . 2) (c . 3)))"#,
        expect,
    );
    let expect = expect_test::expect![[r#""OK \"(lambda (x) (* x x))\"""#]];
    crate::common::assert_oracle_parity_expect(r#"(format "%S" '(lambda (x) (* x x)))"#, expect);
    let expect = expect_test::expect![[r#""OK \"'(1 2 3)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(format "%S" (list 'quote (list 1 2 3)))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"(a (b (c (d (e)))))\"""#]];
    // Deep nesting
    crate::common::assert_oracle_parity_expect(r#"(format "%S" '(a (b (c (d (e))))))"#, expect);

    let expect = expect_test::expect![[r#""OK \"(1 \\\"two\\\" three [4 5] (6 . 7))\"""#]];
    // Mixed types in deep structure
    crate::common::assert_oracle_parity_expect(
        r#"(format "%S" (list 1 "two" 'three [4 5] '(6 . 7)))"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"nil nil []\"""#]];
    // Empty collections
    crate::common::assert_oracle_parity_expect(r#"(format "%S %S %S" nil '() [])"#, expect);
}

// ---------------------------------------------------------------------------
// Format with min-width and various directives combined
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_combined_directives() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK \"s=hi S=sym d=42 o=52 x=2a X=2A c=A f=3.14 e=3.14e+00 g=0.0042 %\"""#
    ]];
    // All directives in one format string
    crate::common::assert_oracle_parity_expect(
        r#"(format "s=%s S=%S d=%d o=%o x=%x X=%X c=%c f=%.2f e=%.2e g=%g %%"
                  "hi" 'sym 42 42 42 42 65 3.14 3.14 0.0042)"#,
        expect,
    );

    let expect = expect_test::expect![[r#""OK \"1+2+3=6\"""#]];
    // Repeated same directive
    crate::common::assert_oracle_parity_expect(r#"(format "%d+%d+%d=%d" 1 2 3 6)"#, expect);

    // Width and precision with every numeric type
    let form = r#"(format "%8d %8o %8x %8X %10.3f %12.3e %10g" 255 255 255 255 3.14 3.14 3.14)"#;
    let expect = expect_test::expect![[
        r#""OK \"     255      377       ff       FF      3.140    3.140e+00       3.14\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);

    let expect = expect_test::expect![[r#""OK \"00000042 00000052 0000002a 0000002A\"""#]];
    // Zero-pad with every integer type
    crate::common::assert_oracle_parity_expect(
        r#"(format "%08d %08o %08x %08X" 42 42 42 42)"#,
        expect,
    );

    let expect =
        expect_test::expect![[r#""OK \"[42      ][52      ][2a      ][hi      ][3.14    ]\"""#]];
    // Left-justify with every type
    crate::common::assert_oracle_parity_expect(
        r#"(format "[%-8d][%-8o][%-8x][%-8s][%-8.2f]" 42 42 42 "hi" 3.14)"#,
        expect,
    );
}

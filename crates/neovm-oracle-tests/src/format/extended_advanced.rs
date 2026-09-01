//! Advanced oracle parity tests for `format` beyond basics:
//! %s %d %f %c %o %x %X %e %g, field width, padding, precision,
//! multiple args, format with special chars, format creating elisp
//! code strings, complex nested format calls.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Comprehensive precision and width combinations for %f %e %g
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_ext_float_precision_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test all combinations of width, precision, and float specifiers
    let form = r#"(list
                    ;; %f with varying precision
                    (format "%.0f" 3.14159)
                    (format "%.1f" 3.14159)
                    (format "%.3f" 3.14159)
                    (format "%.6f" 3.14159)
                    (format "%.10f" 3.14159)
                    ;; %f with width + precision
                    (format "%10.2f" 3.14)
                    (format "%10.2f" -3.14)
                    (format "%-10.2f" 3.14)
                    (format "%010.2f" 3.14)
                    (format "%+10.2f" 3.14)
                    (format "%+10.2f" -3.14)
                    ;; %e with varying precision
                    (format "%e" 12345.6789)
                    (format "%.2e" 12345.6789)
                    (format "%.0e" 12345.6789)
                    (format "%15.4e" 0.000123)
                    (format "%-15.4e" 0.000123)
                    ;; %g switching between f and e style
                    (format "%g" 100.0)
                    (format "%g" 100000.0)
                    (format "%g" 0.0001)
                    (format "%g" 0.00001)
                    (format "%.2g" 3.14159)
                    (format "%.10g" 3.14159))"#;
    let expect = expect_test::expect![[
        r#""OK (\"3\" \"3.1\" \"3.142\" \"3.141590\" \"3.1415900000\" \"      3.14\" \"     -3.14\" \"3.14      \" \"0000003.14\" \"     +3.14\" \"     -3.14\" \"1.234568e+04\" \"1.23e+04\" \"1e+04\" \"     1.2300e-04\" \"1.2300e-04     \" \"100\" \"100000\" \"0.0001\" \"1e-05\" \"3.1\" \"3.14159\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Hex/octal with width, zero-padding, and prefix
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_ext_hex_octal_variations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; Basic hex and octal
                    (format "%x" 0)
                    (format "%x" 15)
                    (format "%x" 255)
                    (format "%x" 65535)
                    (format "%X" 255)
                    (format "%X" 48879)
                    (format "%o" 0)
                    (format "%o" 7)
                    (format "%o" 8)
                    (format "%o" 511)
                    ;; With width and zero-padding
                    (format "%04x" 15)
                    (format "%08x" 255)
                    (format "%08X" 255)
                    (format "%06o" 8)
                    ;; With # prefix
                    (format "%#x" 255)
                    (format "%#X" 255)
                    (format "%#o" 8)
                    (format "%#10x" 255)
                    ;; Left-aligned
                    (format "%-10x" 255)
                    (format "%-10o" 255)
                    ;; Edge: 0 with prefix
                    (format "%#x" 0)
                    (format "%#o" 0))"#;
    let expect = expect_test::expect![[
        r#""OK (\"0\" \"f\" \"ff\" \"ffff\" \"FF\" \"BEEF\" \"0\" \"7\" \"10\" \"777\" \"000f\" \"000000ff\" \"000000FF\" \"000010\" \"0xff\" \"0XFF\" \"010\" \"      0xff\" \"ff        \" \"377       \" \"0\" \"0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// %c with various character values and combined with other specs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_ext_char_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; ASCII letters
                    (format "%c" ?A)
                    (format "%c" ?z)
                    ;; Digits as characters
                    (format "%c" ?0)
                    (format "%c" ?9)
                    ;; Punctuation
                    (format "%c" ?!)
                    (format "%c" ?@)
                    (format "%c" ?~)
                    ;; Space and special ASCII
                    (format "%c" 32)
                    (format "%c" 10)
                    ;; Build string from chars
                    (format "%c%c%c%c%c" ?H ?e ?l ?l ?o)
                    ;; Mix %c with other specifiers
                    (format "char=%c code=%d hex=%x" ?A ?A ?A)
                    (format "%c%c %d+%d=%d" ?( ?) 2 3 5)
                    ;; Wider char codes
                    (format "%c" 955)
                    (format "%c" 8364))"#;
    let expect = expect_test::expect![[
        r#""OK (\"A\" \"z\" \"0\" \"9\" \"!\" \"@\" \"~\" \" \" \"\\n\" \"Hello\" \"char=A code=65 hex=41\" \"() 2+3=5\" \"λ\" \"€\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Format with special characters in template and arguments
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_ext_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    ;; Newlines and tabs in template
                    (format "line1\nline2\nline3")
                    (format "col1\tcol2\tcol3")
                    (format "a\nb\tc\n")
                    ;; Backslash
                    (format "path: C:\\Users\\%s" "test")
                    ;; Quotes within format
                    (format "He said \"%s\"" "hello")
                    ;; Multiple %% (literal percent)
                    (format "%d%% of %d%% = %.2f%%" 50 80 (* 0.5 0.8 100))
                    ;; Empty string arg
                    (format "[%s]" "")
                    (format "[%10s]" "")
                    (format "[%-10s]" "")
                    ;; Format with nil, t
                    (format "%s/%s/%s" nil t 'symbol)
                    ;; Very long format string
                    (format "%s-%s-%s-%s-%s-%s-%s-%s"
                            "a" "b" "c" "d" "e" "f" "g" "h"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"line1\\nline2\\nline3\" \"col1\tcol2\tcol3\" \"a\\nb\tc\\n\" \"path: C:\\\\Users\\\\test\" \"He said \\\"hello\\\"\" \"50% of 80% = 40.00%\" \"[]\" \"[          ]\" \"[          ]\" \"nil/t/symbol\" \"a-b-c-d-e-f-g-h\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested format calls building complex strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_ext_nested_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use format results as arguments to outer format calls
    let form = r#"(list
                    ;; Simple nesting
                    (format "outer(%s)" (format "inner(%d)" 42))
                    ;; Double nesting
                    (format "[%s]"
                            (format "<%s>"
                                    (format "{%d}" 99)))
                    ;; Format generating format strings (meta)
                    (let ((spec (format "%%0%dd" 5)))
                      (format spec 42))
                    ;; Build a CSV line via nested format
                    (let ((fields '("Alice" 30 95.5)))
                      (format "%s,%s,%s"
                              (format "%s" (nth 0 fields))
                              (format "%d" (nth 1 fields))
                              (format "%.1f" (nth 2 fields))))
                    ;; Build key=value pairs
                    (mapconcat
                     (lambda (pair)
                       (format "%s=%s" (car pair) (cdr pair)))
                     '(("host" . "localhost")
                       ("port" . "8080")
                       ("debug" . "true"))
                     "&")
                    ;; Nested format with conditional
                    (let ((values '(1 -2 3 -4 5)))
                      (mapconcat
                       (lambda (v)
                         (if (< v 0)
                             (format "(%d)" (abs v))
                           (format " %d " v)))
                       values
                       ",")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"outer(inner(42))\" \"[<{99}>]\" \"00042\" \"Alice,30,95.5\" \"host=localhost&port=8080&debug=true\" \" 1 ,(2), 3 ,(4), 5 \")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Format creating elisp code strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_ext_elisp_code_generation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use format to construct elisp expressions as strings, then read them
    let form = r#"(list
                    ;; Generate a setq form
                    (format "(setq %s %d)" "my-var" 42)
                    ;; Generate a defun
                    (format "(defun %s (%s)\n  %s)"
                            "my-add" "a b"
                            "(+ a b)")
                    ;; Generate a let form with bindings
                    (let ((bindings '((x . 1) (y . 2) (z . 3))))
                      (format "(let (%s)\n  (+ x y z))"
                              (mapconcat
                               (lambda (b)
                                 (format "(%s %d)" (car b) (cdr b)))
                               bindings
                               " ")))
                    ;; Generate a list literal
                    (format "'(%s)"
                            (mapconcat
                             (lambda (x) (format "%S" x))
                             '("hello" world 42 nil)
                             " "))
                    ;; Read back a generated form and verify
                    (let ((code (format "(+ %d %d)" 10 20)))
                      (car (read-from-string code)))
                    ;; Generate and read a more complex form
                    (let ((code (format "(list %d %d %d)" 1 2 3)))
                      (eval (car (read-from-string code)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"(setq my-var 42)\" \"(defun my-add (a b)\\n  (+ a b))\" \"(let ((x 1) (y 2) (z 3))\\n  (+ x y z))\" \"'(\\\"hello\\\" world 42 nil)\" (+ 10 20) (1 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: format-based table builder with alignment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_ext_aligned_table_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a properly aligned table with computed column widths,
    // header, separator, data rows, and a summary row.
    let form = r#"(let* ((data '(("Widget A" 150 24.99)
                                  ("Gadget Pro" 42 149.95)
                                  ("Nano Thing" 1000 3.50)
                                  ("Mega Device" 7 999.99)))
                         ;; Compute column widths
                         (name-width (apply 'max
                                           (length "Product")
                                           (mapcar (lambda (r) (length (nth 0 r))) data)))
                         ;; Build header
                         (header (format "%-12s %6s %10s %12s"
                                         "Product" "Qty" "Price" "Total"))
                         (sep (make-string (length header) ?=))
                         ;; Build data rows with computed totals
                         (rows (mapcar
                                (lambda (r)
                                  (let ((total (* (nth 1 r) (nth 2 r))))
                                    (format "%-12s %6d %10.2f %12.2f"
                                            (nth 0 r) (nth 1 r) (nth 2 r) total)))
                                data))
                         ;; Summary row
                         (grand-total (apply '+ (mapcar
                                                 (lambda (r) (* (nth 1 r) (nth 2 r)))
                                                 data)))
                         (total-qty (apply '+ (mapcar (lambda (r) (nth 1 r)) data)))
                         (summary (format "%-12s %6d %10s %12.2f"
                                          "TOTAL" total-qty "" grand-total)))
                    (list
                     header
                     sep
                     rows
                     summary
                     ;; Verify computed values
                     total-qty
                     (format "%.2f" grand-total)
                     ;; Row count
                     (length rows)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Product         Qty      Price        Total\" \"===========================================\" (\"Widget A        150      24.99      3748.50\" \"Gadget Pro       42     149.95      6297.90\" \"Nano Thing     1000       3.50      3500.00\" \"Mega Device       7     999.99      6999.93\") \"TOTAL          1199                20546.33\" 1199 \"20546.33\" 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Format with many mixed specifiers in one call
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_ext_many_mixed_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Stress test with many different specifiers in a single format call
    let form = r#"(list
                    ;; All specifier types in one call
                    (format "d=%d x=%x X=%X o=%o f=%.2f e=%.1e g=%g c=%c s=%s S=%S %%"
                            42 255 255 8 3.14 12345.0 0.00001 ?A "hi" '(1 2))
                    ;; Width and alignment variety
                    (format "|%5d|%-5d|%05d|%+5d|%+5d|"
                            42 42 42 42 -42)
                    ;; String width variations
                    (format "|%10s|%-10s|%.3s|%10.3s|%-10.3s|"
                            "hello" "hello" "hello" "hello" "hello")
                    ;; Float edge cases
                    (format "%.2f %.2f %.2f %.2f"
                            0.0 -0.0 1.005 99.995)
                    ;; Very large and small numbers
                    (format "%d %d %e %e"
                            1000000 -1000000 1e20 1e-20)
                    ;; Format with computed args
                    (let ((x 7) (y 3))
                      (format "%d + %d = %d, %d * %d = %d, %d / %d = %d"
                              x y (+ x y) x y (* x y) x y (/ x y))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"d=42 x=ff X=FF o=10 f=3.14 e=1.2e+04 g=1e-05 c=A s=hi S=(1 2) %\" \"|   42|42   |00042|  +42|  -42|\" \"|     hello|hello     |hel|       hel|hel       |\" \"0.00 -0.00 1.00 100.00\" \"1000000 -1000000 1.000000e+20 1.000000e-20\" \"7 + 3 = 10, 7 * 3 = 21, 7 / 3 = 2\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

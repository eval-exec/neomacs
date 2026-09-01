//! Comprehensive oracle parity tests for `format` with all format specs,
//! width/precision/padding combinations, edge cases with argument count
//! mismatches, nested format calls, and complex multi-spec strings.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// %s with various types: string, number, symbol, list, vector, nil, t, cons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_percent_s_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (format "%s" "hello world")
      (format "%s" "")
      (format "%s" 0)
      (format "%s" -999)
      (format "%s" 3.14)
      (format "%s" 'my-symbol)
      (format "%s" '(1 2 3))
      (format "%s" '(a . b))
      (format "%s" [1 2 3])
      (format "%s" nil)
      (format "%s" t)
      (format "%s" '(nil t nil))
      (format "%s" (make-vector 0 0))
      (format "%s" '((a . 1) (b . 2)))
      (format "%s" (make-bool-vector 5 t))
      (format "%s" :keyword))"#;
    let expect = expect_test::expect![[
        r##""OK (\"hello world\" \"\" \"0\" \"-999\" \"3.14\" \"my-symbol\" \"(1 2 3)\" \"(a . b)\" \"[1 2 3]\" \"nil\" \"t\" \"(nil t nil)\" \"[]\" \"((a . 1) (b . 2))\" \"#&5\\\"\u{1f}\\\"\" \":keyword\")""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// %d with integers: zero, negative, positive, large numbers, boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_percent_d() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (format "%d" 0)
      (format "%d" 1)
      (format "%d" -1)
      (format "%d" 42)
      (format "%d" -42)
      (format "%d" 999999)
      (format "%d" -999999)
      (format "%d" 2147483647)
      (format "%d" -2147483648)
      (format "%d" 100000000)
      ;; Float truncated to int by %d
      (format "%d" 3.7)
      (format "%d" -2.9)
      (format "%d" 0.0))"#;
    let expect = expect_test::expect![[
        r#""OK (\"0\" \"1\" \"-1\" \"42\" \"-42\" \"999999\" \"-999999\" \"2147483647\" \"-2147483648\" \"100000000\" \"3\" \"-2\" \"0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// %o (octal), %x (hex lower), %X (hex upper) with various values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_octal_hex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Octal
      (format "%o" 0)
      (format "%o" 7)
      (format "%o" 8)
      (format "%o" 255)
      (format "%o" 4096)
      (format "%o" 65535)
      ;; Hex lowercase
      (format "%x" 0)
      (format "%x" 15)
      (format "%x" 16)
      (format "%x" 255)
      (format "%x" 256)
      (format "%x" 65535)
      (format "%x" 1048576)
      ;; Hex uppercase
      (format "%X" 0)
      (format "%X" 10)
      (format "%X" 11)
      (format "%X" 255)
      (format "%X" 48879)
      (format "%X" 16777215)
      ;; # flag for alternate form
      (format "%#o" 8)
      (format "%#x" 255)
      (format "%#X" 255)
      (format "%#o" 0)
      (format "%#x" 0))"#;
    let expect = expect_test::expect![[
        r#""OK (\"0\" \"7\" \"10\" \"377\" \"10000\" \"177777\" \"0\" \"f\" \"10\" \"ff\" \"100\" \"ffff\" \"100000\" \"0\" \"A\" \"B\" \"FF\" \"BEEF\" \"FFFFFF\" \"010\" \"0xff\" \"0XFF\" \"0\" \"0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// %f, %e, %g float format specs with precision and special values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_float_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; %f basic
      (format "%f" 0.0)
      (format "%f" 1.0)
      (format "%f" -1.0)
      (format "%f" 3.14159265)
      (format "%f" 0.001)
      (format "%f" 123456.789)
      ;; %f with precision
      (format "%.0f" 3.14159)
      (format "%.1f" 3.14159)
      (format "%.2f" 3.14159)
      (format "%.5f" 3.14159)
      (format "%.10f" 3.14159)
      (format "%.0f" 0.5)
      (format "%.0f" 1.5)
      ;; %e scientific
      (format "%e" 0.0)
      (format "%e" 1.0)
      (format "%e" 12345.6789)
      (format "%e" 0.00001)
      (format "%e" -99.99)
      (format "%.2e" 12345.6789)
      (format "%.0e" 12345.6789)
      ;; %g general float
      (format "%g" 0.0)
      (format "%g" 1.0)
      (format "%g" 0.00001)
      (format "%g" 12345.0)
      (format "%g" 100000.0)
      (format "%g" 0.1234567890)
      (format "%.2g" 3.14159)
      (format "%.10g" 3.14159))"#;
    let expect = expect_test::expect![[
        r#""OK (\"0.000000\" \"1.000000\" \"-1.000000\" \"3.141593\" \"0.001000\" \"123456.789000\" \"3\" \"3.1\" \"3.14\" \"3.14159\" \"3.1415900000\" \"0\" \"2\" \"0.000000e+00\" \"1.000000e+00\" \"1.234568e+04\" \"1.000000e-05\" \"-9.999000e+01\" \"1.23e+04\" \"1e+04\" \"0\" \"1\" \"1e-05\" \"12345\" \"100000\" \"0.123457\" \"3.1\" \"3.14159\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// %c character format with various char values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_percent_c() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (format "%c" 65)
      (format "%c" 90)
      (format "%c" 97)
      (format "%c" 122)
      (format "%c" 48)
      (format "%c" 57)
      (format "%c" 32)
      (format "%c" 33)
      (format "%c" ?A)
      (format "%c" ?z)
      (format "%c" ?0)
      (format "%c" ?!)
      (format "%c" ?@)
      (format "%c" ?~)
      ;; Multiple %c in one format
      (format "%c%c%c%c%c" ?H ?e ?l ?l ?o)
      (format "%c %c %c" 65 66 67)
      ;; Unicode characters
      (format "%c" 955)
      (format "%c" 8364))"#;
    let expect = expect_test::expect![[
        r#""OK (\"A\" \"Z\" \"a\" \"z\" \"0\" \"9\" \" \" \"!\" \"A\" \"z\" \"0\" \"!\" \"@\" \"~\" \"Hello\" \"A B C\" \"λ\" \"€\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Width and precision specifiers: right/left align, zero-pad, combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_width_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Right-aligned integers with width
      (format "%5d" 42)
      (format "%10d" 42)
      (format "%3d" 42)
      (format "%1d" 42)
      (format "%5d" -42)
      ;; Left-aligned integers
      (format "%-5d|" 42)
      (format "%-10d|" 42)
      (format "%-5d|" -42)
      ;; Zero-padded integers
      (format "%05d" 42)
      (format "%010d" 42)
      (format "%05d" -42)
      (format "%08d" 0)
      ;; Right-aligned strings
      (format "%10s" "hi")
      (format "%10s" "hello world")
      (format "%3s" "hello")
      ;; Left-aligned strings
      (format "%-10s|" "hi")
      (format "%-10s|" "hello world")
      ;; Float width + precision
      (format "%10.2f" 3.14)
      (format "%-10.2f|" 3.14)
      (format "%010.2f" 3.14)
      (format "%10.5f" 3.14)
      (format "%15.3e" 12345.6789)
      ;; Hex with width
      (format "%08x" 255)
      (format "%08X" 255)
      (format "%-8x|" 255)
      ;; Octal with width
      (format "%08o" 255)
      ;; Precision on strings (truncation)
      (format "%.3s" "hello")
      (format "%.10s" "hello")
      (format "%.0s" "hello")
      (format "%10.3s" "hello")
      (format "%-10.3s|" "hello"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"   42\" \"        42\" \" 42\" \"42\" \"  -42\" \"42   |\" \"42        |\" \"-42  |\" \"00042\" \"0000000042\" \"-0042\" \"00000000\" \"        hi\" \"hello world\" \"hello\" \"hi        |\" \"hello world|\" \"      3.14\" \"3.14      |\" \"0000003.14\" \"   3.14000\" \"      1.235e+04\" \"000000ff\" \"000000FF\" \"ff      |\" \"00000377\" \"hel\" \"hello\" \"\" \"       hel\" \"hel       |\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple format specs in one string, mixed types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_multi_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (format "%s is %d years old and scores %.1f%%" "Alice" 30 95.5)
      (format "0x%04X = %d = 0%o" 255 255 255)
      (format "[%c] %10s: %+8.2f" ?* "total" -42.5)
      (format "%s%s%s" "a" "b" "c")
      (format "%d%d%d%d" 1 2 3 4)
      (format "(%s . %s)" "key" "value")
      (format "%05d-%02d-%02d" 2026 3 2)
      (format "%s: %x (%o) [%d]" "val" 42 42 42)
      (format "%.2f + %.2f = %.2f" 1.1 2.2 3.3)
      (format "%20s | %-20s" "right" "left")
      ;; All spec types in one
      (format "%s %d %o %x %X %f %e %g %c %%" "str" 42 42 42 42 3.14 3.14 3.14 65))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alice is 30 years old and scores 95.5%\" \"0x00FF = 255 = 0377\" \"[*]      total:   -42.50\" \"abc\" \"1234\" \"(key . value)\" \"02026-03-02\" \"val: 2a (52) [42]\" \"1.10 + 2.20 = 3.30\" \"               right | left                \" \"str 42 52 2a 2A 3.140000 3.140000e+00 3.14 A %\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// %% literal percent in various positions and combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_literal_percent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (format "100%%")
      (format "%d%%" 42)
      (format "%%")
      (format "%%%%")
      (format "%%s is not a format")
      (format "start%% middle%% end%%")
      (format "%d%% of %d is %.2f" 50 200 100.0)
      (format "100%% complete: %s" "done")
      (format "%% %s %%" "middle"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"100%\" \"42%\" \"%\" \"%%\" \"%s is not a format\" \"start% middle% end%\" \"50% of 200 is 100.00\" \"100% complete: done\" \"% middle %\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: extra args beyond format specs, format with no specs
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_arg_count_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; No format specs, no extra args
      (format "plain text")
      (format "")
      ;; More args than specs (extra args ignored in Emacs)
      (format "%d" 1 2 3)
      (format "%s" "only-one" "extra1" "extra2")
      ;; Complex string with no specs
      (format "hello world, no format specs here!")
      ;; Only %% (no real specs)
      (format "100%% done" )
      ;; Nested parens in format string
      (format "(%s, %s)" "a" "b")
      ;; Newlines and tabs in format string
      (format "line1\nline2\ttab")
      ;; Backslash in format string
      (format "path\\to\\file"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"plain text\" \"\" \"1\" \"only-one\" \"hello world, no format specs here!\" \"100% done\" \"(a, b)\" \"line1\\nline2\ttab\" \"path\\\\to\\\\file\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: format used in algorithmic context (table generation)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_algorithmic_usage() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Build a multiplication table using format
  (fset 'neovm--fmt-mul-table
    (lambda (n)
      (let ((rows nil))
        ;; Header row
        (let ((hdr (format "%3s" "")))
          (let ((i 1))
            (while (<= i n)
              (setq hdr (concat hdr (format " %4d" i)))
              (setq i (1+ i))))
          (push hdr rows))
        ;; Separator
        (push (make-string (+ 3 (* 5 n)) ?-) rows)
        ;; Data rows
        (let ((i 1))
          (while (<= i n)
            (let ((row (format "%3d" i))
                  (j 1))
              (while (<= j n)
                (setq row (concat row (format " %4d" (* i j))))
                (setq j (1+ j)))
              (push row rows))
            (setq i (1+ i))))
        (mapconcat #'identity (nreverse rows) "\n"))))

  ;; Number base converter using format
  (fset 'neovm--fmt-bases
    (lambda (n)
      (format "dec=%d hex=%x oct=%o bin=%s"
              n n n
              (let ((s "") (v n))
                (if (= v 0) "0"
                  (while (> v 0)
                    (setq s (concat (if (= (% v 2) 1) "1" "0") s))
                    (setq v (/ v 2)))
                  s)))))

  (unwind-protect
      (list
       (funcall 'neovm--fmt-mul-table 4)
       (funcall 'neovm--fmt-bases 0)
       (funcall 'neovm--fmt-bases 1)
       (funcall 'neovm--fmt-bases 42)
       (funcall 'neovm--fmt-bases 255)
       (funcall 'neovm--fmt-bases 1024))
    (fmakunbound 'neovm--fmt-mul-table)
    (fmakunbound 'neovm--fmt-bases)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"       1    2    3    4\\n-----------------------\\n  1    1    2    3    4\\n  2    2    4    6    8\\n  3    3    6    9   12\\n  4    4    8   12   16\" \"dec=0 hex=0 oct=0 bin=0\" \"dec=1 hex=1 oct=1 bin=1\" \"dec=42 hex=2a oct=52 bin=101010\" \"dec=255 hex=ff oct=377 bin=11111111\" \"dec=1024 hex=400 oct=2000 bin=10000000000\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// %S (prin1) vs %s (princ) with complex nested structures
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_format_comprehensive_S_vs_s_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      ;; Strings: %s strips quotes, %S keeps them
      (format "%s" "hello")
      (format "%S" "hello")
      ;; Strings with special chars
      (format "%s" "line1\nline2")
      (format "%S" "line1\nline2")
      ;; Nested lists
      (format "%s" '((1 2) (3 4) (5 6)))
      (format "%S" '((1 2) (3 4) (5 6)))
      ;; Vectors
      (format "%s" [a b c])
      (format "%S" [a b c])
      ;; Mixed types in list
      (format "%s" '(1 "two" three 4.0 nil t))
      (format "%S" '(1 "two" three 4.0 nil t))
      ;; Dotted pair
      (format "%s" '(a . b))
      (format "%S" '(a . b))
      ;; Keyword
      (format "%s" :test)
      (format "%S" :test)
      ;; Character
      (format "%s" ?A)
      (format "%S" ?A))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"\\\"hello\\\"\" \"line1\\nline2\" \"\\\"line1\\nline2\\\"\" \"((1 2) (3 4) (5 6))\" \"((1 2) (3 4) (5 6))\" \"[a b c]\" \"[a b c]\" \"(1 two three 4.0 nil t)\" \"(1 \\\"two\\\" three 4.0 nil t)\" \"(a . b)\" \"(a . b)\" \":test\" \":test\" \"65\" \"65\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

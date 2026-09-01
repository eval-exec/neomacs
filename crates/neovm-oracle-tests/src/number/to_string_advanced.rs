//! Advanced oracle parity tests for number/string conversions:
//! number-to-string on edge cases, string-to-number with bases,
//! format directives for number formatting, base converters,
//! thousands separators, and floating-point precision analysis.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// number-to-string: integers (positive, negative, zero, large, most-positive)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nts_adv_integers_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (number-to-string 0)
  (number-to-string 1)
  (number-to-string -1)
  (number-to-string 2147483647)
  (number-to-string -2147483648)
  (number-to-string 9999999999)
  (number-to-string -9999999999)
  (number-to-string most-positive-fixnum)
  (number-to-string most-negative-fixnum))"#;
    let expect = expect_test::expect![[
        r#""OK (\"0\" \"1\" \"-1\" \"2147483647\" \"-2147483648\" \"9999999999\" \"-9999999999\" \"2305843009213693951\" \"-2305843009213693952\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// number-to-string: floats (normal, scientific, infinity, NaN, denorms)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nts_adv_floats_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (number-to-string 3.14159265358979)
  (number-to-string 0.0)
  (number-to-string -0.0)
  (number-to-string 1.0e308)
  (number-to-string -1.0e308)
  (number-to-string 1.0e-300)
  (number-to-string 5.0e-324)
  (number-to-string 1.7976931348623157e+308)
  (number-to-string 1.0e10)
  (number-to-string 1.0e-10)
  (number-to-string 1.0e100)
  (number-to-string 0.1)
  (number-to-string 0.2)
  (number-to-string (+ 0.1 0.2)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"3.14159265358979\" \"0.0\" \"-0.0\" \"1e+308\" \"-1e+308\" \"1e-300\" \"5e-324\" \"1.7976931348623157e+308\" \"10000000000.0\" \"1e-10\" \"1e+100\" \"0.1\" \"0.2\" \"0.30000000000000004\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-to-number: all bases (10, 16, 8, 2) including edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nts_adv_string_to_number_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Base 10 (default)
  (string-to-number "42")
  (string-to-number "-100")
  (string-to-number "0")
  (string-to-number "999999999")
  ;; Base 16 (hex)
  (string-to-number "FF" 16)
  (string-to-number "ff" 16)
  (string-to-number "DEADBEEF" 16)
  (string-to-number "0" 16)
  (string-to-number "1A2B" 16)
  ;; Base 8 (octal)
  (string-to-number "77" 8)
  (string-to-number "0" 8)
  (string-to-number "777" 8)
  (string-to-number "12345" 8)
  ;; Base 2 (binary)
  (string-to-number "1010" 2)
  (string-to-number "11111111" 2)
  (string-to-number "0" 2)
  (string-to-number "1" 2)
  (string-to-number "10000000" 2))"#;
    let expect = expect_test::expect![[
        r#""OK (42 -100 0 999999999 255 255 3735928559 0 6699 63 0 511 5349 10 255 0 1 128)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-to-number: leading/trailing whitespace and invalid input
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nts_adv_string_to_number_whitespace_invalid() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Leading whitespace
  (string-to-number "  42")
  (string-to-number "   -7")
  (string-to-number "\t100")
  ;; Trailing non-numeric characters
  (string-to-number "42abc")
  (string-to-number "3.14xyz")
  (string-to-number "100 200")
  ;; Completely invalid
  (string-to-number "hello")
  (string-to-number "")
  (string-to-number " ")
  ;; Float parsing
  (string-to-number "3.14")
  (string-to-number "-2.718")
  (string-to-number "1e10")
  (string-to-number "1.5e3")
  ;; Mixed valid/invalid in different bases
  (string-to-number "1G" 16)
  (string-to-number "29" 8)
  (string-to-number "12" 2))"#;
    let expect = expect_test::expect![[
        r#""OK (42 -7 100 42 3.14 100 0 0 0 3.14 -2.718 10000000000.0 1500.0 1 2 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format %d/%x/%o for integer formatting comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nts_adv_format_integer_directives() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; %d decimal
  (format "%d" 42)
  (format "%d" -42)
  (format "%d" 0)
  (format "%d" 255)
  (format "%d" 65536)
  ;; %x hexadecimal
  (format "%x" 255)
  (format "%x" 0)
  (format "%x" 65535)
  (format "%x" 256)
  (format "%X" 255)
  (format "%X" 48879)
  ;; %o octal
  (format "%o" 8)
  (format "%o" 255)
  (format "%o" 0)
  (format "%o" 511)
  ;; Padding and width
  (format "%05d" 42)
  (format "%10d" 42)
  (format "%-10d" 42)
  (format "%08x" 255)
  ;; Multiple format in one string
  (format "dec=%d hex=%x oct=%o" 255 255 255)
  (format "%d+%d=%d" 3 4 7))"#;
    let expect = expect_test::expect![[
        r#""OK (\"42\" \"-42\" \"0\" \"255\" \"65536\" \"ff\" \"0\" \"ffff\" \"100\" \"FF\" \"BEEF\" \"10\" \"377\" \"0\" \"777\" \"00042\" \"        42\" \"42        \" \"000000ff\" \"dec=255 hex=ff oct=377\" \"3+4=7\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: number base converter (any base 2-36)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nts_adv_base_converter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a general base converter: convert integer to string in any base 2-36
    // and back, verifying roundtrip
    let form = r#"(let ((int-to-base
         (lambda (n base)
           (if (= n 0) "0"
             (let ((digits "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ")
                   (result nil)
                   (neg (< n 0))
                   (num (abs n)))
               (while (> num 0)
                 (setq result (cons (aref digits (% num base)) result))
                 (setq num (/ num base)))
               (when neg
                 (setq result (cons ?- result)))
               (apply #'string result)))))
        (base-to-int
         (lambda (s base)
           (let ((digits "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ")
                 (result 0)
                 (i 0)
                 (neg (and (> (length s) 0) (= (aref s 0) ?-)))
                 (start (if (and (> (length s) 0) (= (aref s 0) ?-)) 1 0)))
             (setq i start)
             (while (< i (length s))
               (let* ((ch (upcase (aref s i)))
                      (val (- ch (if (>= ch ?A) (- ?A 10) ?0))))
                 (setq result (+ (* result base) val))
                 (setq i (1+ i))))
             (if neg (- result) result)))))
  (list
   ;; Binary
   (funcall int-to-base 42 2)
   (funcall base-to-int "101010" 2)
   ;; Octal
   (funcall int-to-base 255 8)
   (funcall base-to-int "377" 8)
   ;; Hex
   (funcall int-to-base 48879 16)
   (funcall base-to-int "BEEF" 16)
   ;; Base 36
   (funcall int-to-base 1295 36)
   (funcall base-to-int "ZZ" 36)
   ;; Roundtrip tests
   (= 255 (funcall base-to-int (funcall int-to-base 255 16) 16))
   (= 1000 (funcall base-to-int (funcall int-to-base 1000 7) 7))
   (= 12345 (funcall base-to-int (funcall int-to-base 12345 36) 36))
   ;; Negative
   (funcall int-to-base -42 10)
   (funcall base-to-int "-42" 10)
   ;; Zero
   (funcall int-to-base 0 16)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"101010\" 42 \"377\" 255 \"BEEF\" 48879 \"ZZ\" 1295 t t t \"-42\" -42 \"0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: number formatting with thousands separator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nts_adv_thousands_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement thousands separator formatting in pure Elisp
    let form = r#"(let ((format-thousands
         (lambda (n sep)
           (let* ((neg (< n 0))
                  (s (number-to-string (abs n)))
                  (len (length s))
                  (result nil)
                  (count 0)
                  (i (1- len)))
             (while (>= i 0)
               (when (and (> count 0) (= (% count 3) 0))
                 (setq result (cons sep result)))
               (setq result (cons (aref s i) result))
               (setq count (1+ count))
               (setq i (1- i)))
             (let ((formatted (apply #'string result)))
               (if neg (concat "-" formatted) formatted))))))
  (list
   (funcall format-thousands 0 ?,)
   (funcall format-thousands 1 ?,)
   (funcall format-thousands 100 ?,)
   (funcall format-thousands 1000 ?,)
   (funcall format-thousands 1000000 ?,)
   (funcall format-thousands 1234567890 ?,)
   (funcall format-thousands -9876543 ?,)
   (funcall format-thousands 999 ?.)
   (funcall format-thousands 1000 ?.)
   (funcall format-thousands 100000 ? )
   ;; Verify specific known values
   (string= (funcall format-thousands 1234567 ?,) "1,234,567")
   (string= (funcall format-thousands 1000000000 ?,) "1,000,000,000")
   (string= (funcall format-thousands -42000 ?,) "-42,000")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"0\" \"1\" \"100\" \"1,000\" \"1,000,000\" \"1,234,567,890\" \"-9,876,543\" \"999\" \"1.000\" \"100 000\" t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: floating-point precision analysis
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nts_adv_float_precision_analysis() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Analyze floating-point precision: compute relative errors,
    // test catastrophic cancellation, Kahan summation vs naive sum
    let form = r#"(let ((rel-error
         (lambda (approx exact)
           (if (= exact 0.0) (abs approx)
             (abs (/ (- approx exact) exact)))))
        (kahan-sum
         (lambda (nums)
           (let ((sum 0.0) (c 0.0))
             (dolist (x nums)
               (let* ((y (- x c))
                      (t-val (+ sum y)))
                 (setq c (- (- t-val sum) y))
                 (setq sum t-val)))
             sum)))
        (naive-sum
         (lambda (nums)
           (let ((s 0.0))
             (dolist (x nums) (setq s (+ s x)))
             s))))
  (let* (;; Classic: 0.1 + 0.2 != 0.3
         (sum-01-02 (+ 0.1 0.2))
         (exact-03 0.3)
         (err1 (funcall rel-error sum-01-02 exact-03))
         ;; Catastrophic cancellation
         (big (+ 1.0e15 3.14159))
         (cancelled (- big 1.0e15))
         ;; Kahan vs naive for many small values
         (small-vals (let ((lst nil) (i 0))
                       (while (< i 1000)
                         (setq lst (cons 0.001 lst))
                         (setq i (1+ i)))
                       lst))
         (naive-result (funcall naive-sum small-vals))
         (kahan-result (funcall kahan-sum small-vals))
         ;; Integer arithmetic precision
         (big-product (* 123456789 987654321))
         (big-sum (+ most-positive-fixnum 0))
         ;; Float->int->float roundtrip
         (fval 42.0)
         (ival (truncate fval))
         (fval2 (float ival))
         (roundtrip-ok (= fval fval2)))
    (list
     sum-01-02
     (> err1 0.0)
     cancelled
     naive-result
     kahan-result
     (< (abs (- kahan-result 1.0)) (abs (- naive-result 1.0)))
     big-product
     big-sum
     roundtrip-ok
     ;; Powers of 2 are exactly representable
     (= (expt 2.0 52) 4503599627370496.0)
     ;; Float equality pitfalls
     (= 1.0 (+ 0.5 0.5))
     (= 1.0 (+ 0.1 0.1 0.1 0.1 0.1 0.1 0.1 0.1 0.1 0.1)))))"#;
    let expect = expect_test::expect![[
        r#""OK (0.30000000000000004 t 3.125 1.0000000000000007 1.0 t 0 0 t t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

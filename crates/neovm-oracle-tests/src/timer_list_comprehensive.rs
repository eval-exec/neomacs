//! Oracle parity tests for timer and time operations:
//! `current-time`, `float-time`, `time-add`, `time-subtract`,
//! `time-less-p`, `time-equal-p`, `format-time-string` with many format
//! specifiers, `decode-time`, `encode-time`, `current-time-string`,
//! `time-convert`. Tests arithmetic, comparison, formatting, and
//! round-trip encoding/decoding.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// time-add and time-subtract with various representations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_add_subtract_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (let* ((base '(0 100 0 0))
         (delta '(0 50 0 0))
         (sum (time-add base delta))
         (diff (time-subtract base delta)))
    (list
     ;; time-add with integer seconds
     (float-time (time-add '(0 10 0 0) '(0 20 0 0)))
     ;; time-subtract gives positive result
     (float-time (time-subtract '(0 100 0 0) '(0 30 0 0)))
     ;; time-add with float argument
     (let ((r (float-time (time-add 1.5 2.5))))
       (and (>= r 3.99) (<= r 4.01)))
     ;; Adding zero
     (float-time (time-add '(0 42 0 0) '(0 0 0 0)))
     ;; Subtracting from self gives zero
     (let ((t1 '(0 1000 0 0)))
       (float-time (time-subtract t1 t1)))
     ;; Associativity: (a + b) + c = a + (b + c)
     (let ((a '(0 10 0 0)) (b '(0 20 0 0)) (c '(0 30 0 0)))
       (time-equal-p (time-add (time-add a b) c)
                     (time-add a (time-add b c))))
     ;; time-add with negative float
     (let ((r (float-time (time-add 100.0 -30.0))))
       (and (>= r 69.99) (<= r 70.01))))))"#;
    let expect = expect_test::expect![[r#""OK (30.0 70.0 t 42.0 0.0 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// time-less-p comparisons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_less_p_comparisons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (list
   ;; Basic comparisons with integer seconds
   (time-less-p '(0 10 0 0) '(0 20 0 0))
   (time-less-p '(0 20 0 0) '(0 10 0 0))
   ;; Equal times are not less
   (time-less-p '(0 50 0 0) '(0 50 0 0))
   ;; Float comparisons
   (time-less-p 1.0 2.0)
   (time-less-p 2.0 1.0)
   (time-less-p 1.0 1.0)
   ;; Mixed: float and list
   (time-less-p 0.0 '(0 1 0 0))
   ;; Microsecond precision matters
   (time-less-p '(0 1 0 0) '(0 1 1 0))
   ;; Picosecond precision
   (time-less-p '(0 1 0 0) '(0 1 0 1))
   ;; Large values
   (time-less-p '(1000 0 0 0) '(1001 0 0 0))))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t nil nil t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// time-equal-p with different representations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_equal_p_representations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (list
   ;; Same list representation
   (time-equal-p '(0 10 0 0) '(0 10 0 0))
   ;; Different but equal
   (time-equal-p '(0 0 0 0) 0)
   ;; Float vs list
   (time-equal-p 0.0 '(0 0 0 0))
   ;; Not equal
   (time-equal-p '(0 1 0 0) '(0 2 0 0))
   ;; Reflexivity
   (let ((t1 '(100 200 300 0)))
     (time-equal-p t1 t1))
   ;; Symmetry
   (let ((a '(0 42 0 0)) (b '(0 42 0 0)))
     (and (time-equal-p a b) (time-equal-p b a)))
   ;; Integer representation
   (time-equal-p 100 100)
   (time-equal-p 100 200)))"#;
    let expect = expect_test::expect![[r#""OK (t t t nil t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format-time-string with various format specifiers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_format_time_string_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Use a fixed known time: 2000-01-15 10:30:45 UTC (epoch 947933445)
    let form = r#"(progn
  (let ((fixed-time (encode-time '(45 30 10 15 1 2000 nil nil t))))
    (list
     ;; Year
     (format-time-string "%Y" fixed-time t)
     ;; Month (zero-padded)
     (format-time-string "%m" fixed-time t)
     ;; Day
     (format-time-string "%d" fixed-time t)
     ;; Hour (24h)
     (format-time-string "%H" fixed-time t)
     ;; Minute
     (format-time-string "%M" fixed-time t)
     ;; Second
     (format-time-string "%S" fixed-time t)
     ;; Combined ISO-ish
     (format-time-string "%Y-%m-%d %H:%M:%S" fixed-time t)
     ;; Abbreviated weekday name
     (format-time-string "%a" fixed-time t)
     ;; Full weekday name
     (format-time-string "%A" fixed-time t)
     ;; Abbreviated month name
     (format-time-string "%b" fixed-time t)
     ;; Full month name
     (format-time-string "%B" fixed-time t)
     ;; Timezone offset
     (format-time-string "%z" fixed-time t)
     ;; Literal percent
     (format-time-string "%%" fixed-time t))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"2000\" \"01\" \"15\" \"10\" \"30\" \"45\" \"2000-01-15 10:30:45\" \"Sat\" \"Saturday\" \"Jan\" \"January\" \"+0000\" \"%\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// decode-time and encode-time round-trip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_decode_encode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (let* ((original-time (encode-time '(0 0 12 25 6 2020 nil nil t)))
         (decoded (decode-time original-time t))
         ;; decoded is (SEC MIN HOUR DAY MON YEAR DOW DST UTCOFF)
         (sec (nth 0 decoded))
         (min (nth 1 decoded))
         (hour (nth 2 decoded))
         (day (nth 3 decoded))
         (mon (nth 4 decoded))
         (year (nth 5 decoded))
         (dow (nth 6 decoded))
         ;; Re-encode
         (reencoded (encode-time (list sec min hour day mon year nil nil t))))
    (list
     ;; Decoded components are correct
     sec min hour day mon year
     ;; Day of week (Thursday = 4 for 2020-06-25)
     dow
     ;; Round-trip: re-encoded equals original
     (time-equal-p original-time reencoded)
     ;; Decode another known date: 2000-01-01 00:00:00 UTC
     (let* ((y2k (encode-time '(0 0 0 1 1 2000 nil nil t)))
            (d (decode-time y2k t)))
       (list (nth 3 d) (nth 4 d) (nth 5 d) (nth 6 d))))))"#;
    let expect = expect_test::expect![[r#""OK (0 0 12 25 6 2020 4 t (1 1 2000 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// time-convert between representations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_convert_representations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (list
   ;; Convert integer to list representation
   (let ((t1 (time-convert 1000 'list)))
     (and (listp t1) (integerp (car t1))))
   ;; Convert to integer (truncates)
   (time-convert 100 'integer)
   ;; Convert float-time to integer
   (time-convert 42.7 'integer)
   ;; time-convert with nil (default output)
   (let ((r (time-convert 100 nil)))
     (not (null r)))
   ;; Round-trip: int -> list -> int
   (let* ((orig 500)
          (as-list (time-convert orig 'list))
          (back (time-convert as-list 'integer)))
     (= orig back))
   ;; Zero time
   (time-convert 0 'integer)
   ;; Large value
   (time-convert 1000000 'integer)))"#;
    let expect = expect_test::expect![[r#""OK (t 100 42 t t 0 1000000)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// current-time-string format
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_current_time_string_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Use a fixed time to get deterministic output
    let form = r#"(progn
  (let* ((fixed-time (encode-time '(30 15 9 4 7 2023 nil nil t)))
         (str (current-time-string fixed-time t)))
    (list
     ;; Result is a string
     (stringp str)
     ;; Has expected length (like "Tue Jul  4 09:15:30 2023")
     (> (length str) 20)
     ;; Contains year
     (not (null (string-match "2023" str)))
     ;; Contains month
     (not (null (string-match "Jul" str)))
     ;; Contains time
     (not (null (string-match "09:15:30" str))))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// float-time and arithmetic consistency
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_float_time_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (list
   ;; float-time of integer
   (float-time 0)
   (float-time 100)
   ;; float-time of list
   (float-time '(0 42 0 0))
   ;; float-time preserves microseconds approximately
   (let ((r (float-time '(0 1 500000 0))))
     (and (> r 1.4) (< r 1.6)))
   ;; Arithmetic consistency: float-time(a + b) ~ float-time(a) + float-time(b)
   (let* ((a '(0 100 0 0))
          (b '(0 200 0 0))
          (sum-ft (float-time (time-add a b)))
          (ft-sum (+ (float-time a) (float-time b))))
     (< (abs (- sum-ft ft-sum)) 0.001))
   ;; float-time of float is identity
   (let ((r (float-time 3.14)))
     (and (> r 3.13) (< r 3.15)))
   ;; Negative time
   (< (float-time (time-subtract '(0 10 0 0) '(0 20 0 0))) 0)))"#;
    let expect = expect_test::expect![[r#""OK (0.0 100.0 42.0 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// format-time-string with composite and padding specifiers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_format_composite_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Use a fixed time: 2023-03-05 08:05:03 UTC
    let form = r#"(progn
  (let ((t1 (encode-time '(3 5 8 5 3 2023 nil nil t))))
    (list
     ;; 12-hour clock
     (format-time-string "%I" t1 t)
     ;; AM/PM
     (format-time-string "%p" t1 t)
     ;; Day of year
     (format-time-string "%j" t1 t)
     ;; Week number (Sunday start)
     (format-time-string "%U" t1 t)
     ;; Week number (Monday start)
     (format-time-string "%W" t1 t)
     ;; Full date and time
     (format-time-string "%c" t1 t)
     ;; Just date
     (format-time-string "%x" t1 t)
     ;; Just time
     (format-time-string "%X" t1 t)
     ;; Tab and newline escapes
     (format-time-string "%Y%t%m%n%d" t1 t)
     ;; Century
     (format-time-string "%C" t1 t))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"08\" \"AM\" \"064\" \"10\" \"09\" \"Sun Mar  5 08:05:03 2023\" \"03/05/23\" \"08:05:03\" \"2023\t03\\n05\" \"20\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// encode-time with edge cases and boundary dates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_time_encode_time_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (list
   ;; Epoch time
   (float-time (encode-time '(0 0 0 1 1 1970 nil nil t)))
   ;; End of day
   (let* ((t1 (encode-time '(59 59 23 31 12 1999 nil nil t)))
          (d (decode-time t1 t)))
     (list (nth 0 d) (nth 1 d) (nth 2 d)))
   ;; Leap year date: Feb 29, 2000
   (let* ((t1 (encode-time '(0 0 12 29 2 2000 nil nil t)))
          (d (decode-time t1 t)))
     (list (nth 3 d) (nth 4 d) (nth 5 d)))
   ;; Non-leap year: encode Feb 29, 2001 — Emacs normalizes to Mar 1
   (let* ((t1 (encode-time '(0 0 12 29 2 2001 nil nil t)))
          (d (decode-time t1 t)))
     (list (nth 3 d) (nth 4 d) (nth 5 d)))
   ;; Overflow seconds: 90 seconds = 1 minute 30 seconds
   (let* ((t1 (encode-time '(90 0 0 1 1 2000 nil nil t)))
          (d (decode-time t1 t)))
     (list (nth 0 d) (nth 1 d)))
   ;; Year 2038 (32-bit overflow test area)
   (let* ((t1 (encode-time '(0 0 0 1 1 2038 nil nil t)))
          (d (decode-time t1 t)))
     (nth 5 d))))"#;
    let expect =
        expect_test::expect![[r#""OK (0.0 (59 59 23) (29 2 2000) (1 3 2001) (30 1) 2038)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

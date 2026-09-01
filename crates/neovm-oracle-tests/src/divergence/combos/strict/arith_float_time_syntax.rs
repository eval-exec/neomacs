//! Strict combo oracle probes: integer/float arithmetic edge cases, time
//! formatting with explicit zones, syntax-table parsing, read syntax bases,
//! char-tables, records, and error-condition machinery.
//!
//! These target reimplementation weak spots: % / mod sign semantics, the
//! 2-argument floor/ceiling/round/truncate (quotient . remainder) pairs,
//! bignum arithmetic, IEEE inf/nan and frexp/ldexp/logb/copysign,
//! format-time-string padding flags (%_H, %-H, %0H, %^a) and ISO week with
//! an explicit UTC zone, encode/decode-time roundtrips, parse-partial-sexp,
//! scan-lists with custom paren delimiters, #x/#o/#b/#Nr read syntax,
//! char-table subtype/aref, records/type-of, and define-error hierarchies.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- integer %/mod sign semantics and 2-arg rounding pairs ----------------

#[test]
fn div_afs_integer_mod_and_2arg_rounding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 -1 1 -1 1 2 -2 -1 2 3 2 2 -3 -2 -3 -2 -2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (% 7 3)
      (% -7 3)
      (% 7 -3)
      (% -7 -3)
      (mod 7 3)
      (mod -7 3)
      (mod 7 -3)
      (mod -7 -3)
      (floor 7 3)
      (ceiling 7 3)
      (round 7 3)
      (truncate 7 3)
      (floor -7 3)
      (ceiling -7 3)
      (floor 7 -3)
      (ceiling 7 -3)
      (round -7 3))
"##,
        expect,
    );
}

#[test]
fn div_afs_bignum_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function gcd)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (expt 2 64)
      (expt 2 128)
      (expt 3 50)
      (* 1000000000000 1000000000000)
      (gcd 48 180)
      (lcm 4 6)
      (gcd (expt 2 40) (expt 2 30))
      (logcount 15)
      (logcount (expt 2 100))
      (ash 1 200)
      (ash -1 5)
      (mod (expt 2 100) 7)
      (floor (expt 2 100) 13)
      (% (expt 2 100) 13))
"##,
        expect,
    );
}

// --- IEEE float inf/nan and frexp/ldexp/logb/copysign ----------------------

#[test]
fn div_afs_float_inf_nan_decomp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1.0e+INF -1.0e+INF -0.0e+NaN t nil nil -1.0 -0.0 (0.875 . 2) (0.0 . 0) 3.5 3 1.0e+INF -0.0 -1.0e+INF 1.0e+INF)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (/ 1.0 0.0)
      (/ -1.0 0.0)
      (/ 0.0 0.0)
      (isnan (/ 0.0 0.0))
      (isnan 1.0)
      (isnan (/ 1.0 0.0))
      (copysign 1.0 -1.0)
      (copysign 0.0 -1.0)
      (frexp 3.5)
      (frexp 0.0)
      (ldexp 0.875 2)
      (logb 8.0)
      (string-to-number "1e400")
      (string-to-number "-0.0")
      (* 1.0e+INF -1.0)
      (+ 1.0e+INF 1.0e+INF))
"##,
        expect,
    );
}

#[test]
fn div_afs_float_ffloor_fceiling_fround_ftruncate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3.0 -4.0 4.0 -3.0 2.0 4.0 3.0 -3.0 0.0 -0.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (ffloor 3.7)
      (ffloor -3.7)
      (fceiling 3.2)
      (fceiling -3.2)
      (fround 2.5)
      (fround 3.5)
      (ftruncate 3.9)
      (ftruncate -3.9)
      (fceiling 0.0)
      (fround -0.5))
"##,
        expect,
    );
}

#[test]
fn div_afs_float_transcendental_and_constants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function float-e)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (sqrt 2.0)
      (expt 2.0 0.5)
      (exp 1.0)
      (log 1000.0)
      (log 8.0 2.0)
      (sin 0.0)
      (cos 0.0)
      (atan 1.0)
      (float-e)
      (float-pi)
      (abs -3.5)
      (abs -7)
      (abs 0))
"##,
        expect,
    );
}

// --- format-time-string padding flags + ISO week, explicit UTC zone --------

#[test]
fn div_afs_format_time_padding_zone() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"09:30:05\" \"2025-07-04\" \"09:30:05 +0000\" \"09:30 AM\" \"Fri Friday\" \"Jul July\" \"185\" \"26\" \"26\" \"2025-W27-5\" \" 9:30:05\" \"FRI JUL\" \"11:30:05\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((time (encode-time 5 30 9 4 7 2025 0)))
  (list (format-time-string "%H:%M:%S" time 0)
        (format-time-string "%Y-%m-%d" time 0)
        (format-time-string "%H:%M:%S %z" time 0)
        (format-time-string "%I:%M %p" time 0)
        (format-time-string "%a %A" time 0)
        (format-time-string "%b %B" time 0)
        (format-time-string "%j" time 0)
        (format-time-string "%U" time 0)
        (format-time-string "%W" time 0)
        (format-time-string "%G-W%V-%u" time 0)
        (format-time-string "%_H:%-M:%0S" time 0)
        (format-time-string "%^a %^b" time 0)
        (format-time-string "%H:%M:%S" time 7200)))
"##,
        expect,
    );
}

#[test]
fn div_afs_encode_decode_time_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((14041 15794) (30 59 12 28 2 1999 0 nil 0) (0 \"GMT\") 920206770.0 (-41 8576) (0 0 0 1 12 1969 1 nil 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((time (encode-time 30 59 12 28 2 1999 0)))
  (list time
        (decode-time time 0)
        (current-time-zone time 0)
        (float-time time)
        (encode-time 0 0 0 1 0 1970 0)
        (decode-time (encode-time 0 0 0 1 0 1970 0) 0)))
"##,
        expect,
    );
}

// --- syntax-table parsing: scan-lists, scan-sexps, parse-partial-sexp ------

#[test]
fn div_afs_parse_partial_sexp_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (16 16 2 (0 nil 1 nil nil nil 0 nil nil nil nil) 1 nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(foo (bar) baz) ;; comment
(gone)")
  (goto-char 1)
  (list (scan-sexps 1 1)
        (scan-lists 1 1 0)
        (scan-lists 1 1 -1)
        (parse-partial-sexp 1 16)
        (nth 0 (parse-partial-sexp 1 5))
        (nth 3 (parse-partial-sexp 1 6))
        (condition-case err (scan-sexps 1 5) (scan-error (car err)))))
"##,
        expect,
    );
}

#[test]
fn div_afs_custom_paren_delimiter_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 15 nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (modify-syntax-entry ?< "(>")
  (modify-syntax-entry ?> ")<")
  (insert "<nested <a> b>")
  (goto-char 1)
  (list (scan-lists 1 1 0)
        (scan-sexps 1 1)
        (forward-comment 14)
        (nth 0 (parse-partial-sexp 1 7))))
"##,
        expect,
    );
}

// --- read syntax: bases and character escapes -----------------------------

#[test]
fn div_afs_read_number_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (511 511 11 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (read "#x1ff")
      (read "#o777")
      (read "#b1011")
      (read "#24rn"))
"##,
        expect,
    );
}

#[test]
fn div_afs_read_char_escapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 134217825 134217729 233 233)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (read "?\\C-a")
      (read "?\\M-a")
      (read "?\\C-\\M-a")
      (read "?\\u00e9")
      (read "?\\N{LATIN SMALL LETTER E WITH ACUTE}"))
"##,
        expect,
    );
}

#[test]
fn div_afs_read_from_string_and_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 (97 98 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (car (read-from-string "42 extra"))
      (read "(?a ?b ?\\C-c)"))
"##,
        expect,
    );
}

#[test]
fn div_afs_read_hash_paren_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\")""##]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: ERR (invalid-read-syntax "#")
    // Neomacs:   ERR (invalid-read-syntax "#(: first element must be a string")
    // GNU rejects the "#(" digraph outright; Neomacs' reader treats "#(" as a
    // record/char-table-like prefix and only fails later with a divergent
    // error message.
    crate::common::assert_oracle_parity_expect(
        r##"
(read "#(1 2 3)")
"##,
        expect,
    );
}

#[test]
fn div_afs_read_bool_vector_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#&...\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(read "#&3\"abc\"")
"##,
        expect,
    );
}

// --- char-table basics ----------------------------------------------------

#[test]
fn div_afs_char_table_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t char-table syntax-table 4 nil (1) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'syntax-table nil)))
  (aset ct ?\( 4)
  (aset ct ?a '(1))
  (list (char-table-p ct)
        (type-of ct)
        (char-table-subtype ct)
        (aref ct ?\()
        (aref ct ?A)
        (aref ct ?a)
        (char-table-range ct 0)))
"##,
        expect,
    );
}

// --- records and type-of --------------------------------------------------

#[test]
fn div_afs_record_and_type_of() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function vrecordp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((r (record 'foo 1 2 3)))
  (list (recordp r)
        (type-of r)
        (length r)
        (aref r 0)
        (aref r 3)
        (record 'a 'b 'c)
        (vrecordp r)))
"##,
        expect,
    );
}

// --- define-error / error-conditions hierarchy ----------------------------

#[test]
fn div_afs_error_conditions_define_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Unknown signal ‘my-parent’\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (define-error 'probe-cust-err "a probe error" '(my-parent error))
  (define-error 'probe-child-err "child" '(probe-cust-err))
  (list (get 'probe-cust-err 'error-conditions)
        (get 'probe-cust-err 'error-message)
        (get 'probe-child-err 'error-conditions)
        (condition-case err
            (signal 'probe-child-err '(42 43))
          (probe-cust-err (list 'caught (cadr err)))
          (error (list 'other (cdr err))))
        (condition-case err
            (signal 'probe-child-err nil)
          (my-parent 'caught-parent))))
"##,
        expect,
    );
}

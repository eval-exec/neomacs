//! Complex combo batch 311 — `number` operations ultimate: format
//! specifier combos, `number-to-string`/`string-to-number` edge cases,
//! `expt`/`log`/`sqrt` matrix, `floor`/`ceiling`/`round`/`truncate`
//! with all sign/divisor combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx311_format_all_specifiers_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Format string ends in middle of format specifier\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%d" 42)
      (format "%i" 42)
      (format "%5d" 42)
      (format "%-5d|" 42)
      (format "%05d" 42)
      (format "%+d" 42)
      (format "%o" 64)
      (format "%x" 255)
      (format "%X" 255)
      (format "%b" 10)
      (format "%c" 65)
      (format "%e" 12345.6789)
      (format "%f" 12345.6789)
      (format "%g" 0.00001)
      (format "%.2f" 3.14159)
      (format "%10.3f" 3.14159)
      (format "%s" "hello")
      (format "%S" '(1 "two" 3))
      (format "%%")
      (format "%3$" 1 2 3))
"##,
        expect,
    )
}

#[test]
fn div_cx311_number_to_string_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (number-to-string 42)
      (number-to-string -42)
      (number-to-string 3.14)
      (number-to-string 1/3)
      (number-to-string (expt 2 64))
      (number-to-string -1/7)
      (number-to-string 0)
      (number-to-string -0.0))
"##,
        expect,
    )
}

#[test]
fn div_cx311_string_to_number_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 3.14 1 0 0 0 0 42 0 -314.0 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-to-number "42")
      (string-to-number "3.14")
      (string-to-number "1/3")
      (string-to-number "0x1A")
      (string-to-number "0o17")
      (string-to-number "0b1010")
      (string-to-number "not-a-number")
      (string-to-number "42abc")
      (string-to-number "")
      (string-to-number "-3.14e2")
      (string-to-number "  42  "))
"##,
        expect,
    )
}

#[test]
fn div_cx311_expt_log_sqrt_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable exp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (expt 2 10)
      (expt 2 0)
      (expt 2 -1)
      (expt 2.0 0.5)
      (expt 10 20)
      (expt 0 0)
      (expt 1 100)
      (log 100)
      (log 100 10)
      (log exp)
      (log 1)
      (sqrt 16)
      (sqrt 2)
      (expt 8 1/3))
"##,
        expect,
    )
}

#[test]
fn div_cx311_floor_ceiling_round_truncate_all_combos() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 -3 3 -2 2 2 3 4 -4 2 -2 2.0 3.0 2.0 2.0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (floor 2.7)
      (floor -2.7)
      (ceiling 2.3)
      (ceiling -2.3)
      (round 2.5)
      (round 2.4)
      (round 2.6)
      (round 3.5)
      (round -3.5)
      (truncate 2.7)
      (truncate -2.7)
      (ffloor 2.7)
      (fceiling 2.3)
      (fround 2.5)
      (ftruncate 2.7))
"##,
        expect,
    )
}

#[test]
fn div_cx311_mod_remainder_negative_divisor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 -1 1 -1 1 2 -2 -1 1.5 1.5)""#]];
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
      (mod 7.5 3)
      (mod -7.5 3))
"##,
        expect,
    )
}

#[test]
fn div_cx311_ash_lsh_logand_logior_logxor_bignum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (1 1024 4294967296 18446744073709551616 340282366920938463463374607431768211456 128 -1 1298074214633706907132624082305024 1237940039285380274899124224 15 255 240 -1 8 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 100)))
  (list (ash 1 0)
        (ash 1 10)
        (ash 1 32)
        (ash 1 64)
        (ash 1 128)
        (ash 256 -1)
        (ash -1 -1)
        (ash big 10)
        (ash big -10)
        (logand #xff #x0f)
        (logior #xf0 #x0f)
        (logxor #xff #x0f)
        (lognot #x00)
        (logcount 255)
        (logcount -1)))
"##,
        expect,
    )
}

#[test]
fn div_cx311_format_positional_args_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Not enough arguments for format string\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%2$s %1$s" "world" "hello")
      (format "%1$d + %2$d = %3$d" 2 3 5)
      (format "%s = %2$d (or %d)" "x" 99)
      (format "%3$-10s|" "a" "b" "c"))
"##,
        expect,
    )
}

#[test]
fn div_cx311_nan_inf_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((inf (/ 1.0 0.0))
      (neginf (/ -1.0 0.0))
      (nan (/ 0.0 0.0)))
  (list (numberp inf)
        (floatp inf)
        (= inf inf)
        (eq inf inf)
        (< neginf inf)
        (numberp nan)
        (= nan nan)
        (< nan 0)))
"##,
        expect,
    )
}

#[test]
fn div_cx311_number_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 355/113)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((big (expt 2 128))
      (ratio 355/113)
      (flt 3.141592653589793))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "big=%s ratio=%s pi=%.15f" big ratio flt))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list (integerp big)
                        (> big most-positive-fixnum)
                        (format "%d" big)
                        (format "%x" big)
                        (number-to-string ratio)
                        (format "%.10f" ratio)
                        (buffer-string)
                        (marker-position m)
                        (overlay-start ov) (overlay-end ov)
                        (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    )
}

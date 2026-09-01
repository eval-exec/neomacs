//! Strict combo oracle probes, batch 10: extended # read syntax, number
//! parsing edges, bignum/negative bitwise ops, extended format-time flags,
//! 1-arg vs 2-arg floor/ceiling/round, min/max/abs over mixed types, and
//! sxhash determinism.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_e5_read_hash_syntax_more() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#&...\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (read "#s(foo 1 2 3)")
      (read "#&3\"abc\"")
      (type-of (read "#s(foo 1 2 3)"))
      (aref (read "#s(foo 1 2 3)") 0)
      (condition-case err (read "#?x") (invalid-read-syntax (car err)))
      (condition-case err (read "#0a") (invalid-read-syntax (car err)))
      (condition-case err (read "#z1") (invalid-read-syntax (car err))))
"##,
        expect,
    );
}

#[test]
fn div_e5_number_parsing_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 5 0 0.5 1.0e+INF 42 3.14 0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-to-number "0x1f")
      (string-to-number "1_000")
      (string-to-number "+5")
      (string-to-number "-0")
      (string-to-number ".5")
      (string-to-number "1e1000")
      (string-to-number "  42  ")
      (string-to-number "3.14abc")
      (string-to-number "")
      (string-to-number "inf")
      (string-to-number "0b101"))
"##,
        expect,
    );
}

#[test]
fn div_e5_bitwise_bignum_negatives() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (255 7 6 0 -1 -6 -1 0 -4 -1267650600228229401496703205376 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (logand -1 255)
      (logior 5 3)
      (logxor 5 3)
      (logand (expt 2 64) (1- (expt 2 64)))
      (lognot 0)
      (lognot 5)
      (logand -1)
      (logior)
      (ash -8 -1)
      (ash -1 100)
      (logcount (lognot 0)))
"##,
        expect,
    );
}

#[test]
fn div_e5_format_time_zone_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"+0000\" \"000000000\" \"1749988800\" \"24\" \"2025\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 12 15 6 2025 0)))
  (list (format-time-string "%z" t0 0)
        (format-time-string "%N" t0 0)
        (format-time-string "%s" t0 0)
        (format-time-string "%V" t0 0)
        (format-time-string "%G" t0 0)))
"##,
        expect,
    );
}

#[test]
fn div_e5_format_time_colon_zone_modifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"+00:00\" \"+00:00:00\" \"+00\")""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs expands the ":" modifier on %z: %:z -> "+00:00",
    // %::z -> "+00:00:00", %:::z -> "+00".  Neomacs does not recognize the
    // ":" modifier and emits the spec literally.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t0 (encode-time 0 0 12 15 6 2025 0)))
  (list (format-time-string "%:z" t0 0)
        (format-time-string "%::z" t0 0)
        (format-time-string "%:::z" t0 0)))
"##,
        expect,
    );
}

#[test]
fn div_e5_floor_ceiling_1arg_and_2arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 2 -3 1 2 2 -2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (floor 3.7)
      (ceiling 3.2)
      (round 2.5)
      (truncate -3.7)
      (floor 3.7 2)
      (ceiling 3.2 2)
      (floor 7 3)
      (round -2.5)
      (floor most-positive-fixnum 3))
"##,
        expect,
    );
}

#[test]
fn div_e5_min_max_abs_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 10 37)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (max 1 2 3)
      (min 3 1 2)
      (max -1 -2 -3)
      (abs -5)
      (abs -5.5)
      (max 1 2.0 3)
      (min 1 1.0)
      (apply #'max '(1 2 3 4))
      (max (expt 2 40) (expt 2 39))))
"##,
        expect,
    );
}

#[test]
fn div_e5_sxhash_determinism() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8059383 8059383 0 0 363 15723 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (sxhash "abc")
      (sxhash "abc")
      (sxhash-eq 'sym)
      (sxhash-eq 'sym)
      (sxhash '(1 2 3))
      (sxhash [1 2 3])
      (integerp (sxhash 42))
      (integerp (sxhash-eq 42)))
"##,
        expect,
    );
}

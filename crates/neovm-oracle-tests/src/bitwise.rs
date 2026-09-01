//! Oracle parity tests for bitwise operations: `logand`, `logior`,
//! `logxor`, `lognot`, `ash`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// logand
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logand_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 15""#];
    crate::common::assert_oracle_parity_expect("(logand #xff #x0f)", expect);
    let expect = expect_test::expect![r#""OK 8""#];
    crate::common::assert_oracle_parity_expect("(logand #b1010 #b1100)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(logand 255 0)", expect);
    let expect = expect_test::expect![r#""OK 42""#];
    crate::common::assert_oracle_parity_expect("(logand -1 42)", expect);
}

#[test]
fn oracle_prop_logand_multiple_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 3""#];
    crate::common::assert_oracle_parity_expect("(logand #xff #x0f #x03)", expect);
    let expect = expect_test::expect![r#""OK 31""#];
    crate::common::assert_oracle_parity_expect("(logand 255 127 63 31)", expect);
    let expect = expect_test::expect![r#""OK -1""#];
    crate::common::assert_oracle_parity_expect("(logand)", expect);
    let expect = expect_test::expect![r#""OK 42""#];
    crate::common::assert_oracle_parity_expect("(logand 42)", expect);
}

#[test]
fn oracle_prop_logand_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK -1""#];
    crate::common::assert_oracle_parity_expect("(logand -1 -1)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(logand -256 255)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(logand -128 127)", expect);
}

// ---------------------------------------------------------------------------
// logior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logior_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 255""#];
    crate::common::assert_oracle_parity_expect("(logior #x0f #xf0)", expect);
    let expect = expect_test::expect![r#""OK 15""#];
    crate::common::assert_oracle_parity_expect("(logior #b1010 #b0101)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(logior 0 0)", expect);
}

#[test]
fn oracle_prop_logior_multiple_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 31""#];
    crate::common::assert_oracle_parity_expect("(logior 1 2 4 8 16)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(logior)", expect);
    let expect = expect_test::expect![r#""OK 42""#];
    crate::common::assert_oracle_parity_expect("(logior 42)", expect);
}

#[test]
fn oracle_prop_logior_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK -1""#];
    crate::common::assert_oracle_parity_expect("(logior -1 0)", expect);
    let expect = expect_test::expect![r#""OK -64""#];
    crate::common::assert_oracle_parity_expect("(logior -128 64)", expect);
}

// ---------------------------------------------------------------------------
// logxor
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_logxor_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 240""#];
    crate::common::assert_oracle_parity_expect("(logxor #xff #x0f)", expect);
    let expect = expect_test::expect![r#""OK 6""#];
    crate::common::assert_oracle_parity_expect("(logxor #b1010 #b1100)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(logxor 42 42)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(logxor 0 0)", expect);
}

#[test]
fn oracle_prop_logxor_multiple_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 7""#];
    crate::common::assert_oracle_parity_expect("(logxor 1 2 4)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(logxor)", expect);
    let expect = expect_test::expect![r#""OK 42""#];
    crate::common::assert_oracle_parity_expect("(logxor 42)", expect);
}

#[test]
fn oracle_prop_logxor_self_inverse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // XOR with itself is always 0
    let form = "(let ((x 12345))
                  (logxor (logxor x 99999) 99999))";
    let expect = expect_test::expect![[r#""OK 12345""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("12345", &o, &n);
}

// ---------------------------------------------------------------------------
// lognot
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_lognot_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK -1""#];
    crate::common::assert_oracle_parity_expect("(lognot 0)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(lognot -1)", expect);
    let expect = expect_test::expect![r#""OK -2""#];
    crate::common::assert_oracle_parity_expect("(lognot 1)", expect);
    let expect = expect_test::expect![r#""OK -256""#];
    crate::common::assert_oracle_parity_expect("(lognot 255)", expect);
}

#[test]
fn oracle_prop_lognot_double_negation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 42""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect("(lognot (lognot 42))", expect);
    assert_ok_eq("42", &o, &n);
}

#[test]
fn oracle_prop_logcount_accepts_bignum_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU data.c uses CHECK_INTEGER and has a BIGNUMP path for logcount.  For
    // negative integers it counts zero bits by complementing the value first.
    let form = r#"(let ((big (ash 1 100))
      (mask (1- (ash 1 100))))
  (list (logcount big)
        (logcount mask)
        (logcount (- big))
        (logcount (- -1 mask))))"#;
    let expect = expect_test::expect![r#""OK (1 100 100 100)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// ash (arithmetic shift)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_ash_left_shift() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 1""#];
    crate::common::assert_oracle_parity_expect("(ash 1 0)", expect);
    let expect = expect_test::expect![r#""OK 2""#];
    crate::common::assert_oracle_parity_expect("(ash 1 1)", expect);
    let expect = expect_test::expect![r#""OK 256""#];
    crate::common::assert_oracle_parity_expect("(ash 1 8)", expect);
    let expect = expect_test::expect![r#""OK 65536""#];
    crate::common::assert_oracle_parity_expect("(ash 1 16)", expect);
    let expect = expect_test::expect![r#""OK 40""#];
    crate::common::assert_oracle_parity_expect("(ash 5 3)", expect);
}

#[test]
fn oracle_prop_ash_right_shift() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK 128""#];
    crate::common::assert_oracle_parity_expect("(ash 256 -1)", expect);
    let expect = expect_test::expect![r#""OK 16""#];
    crate::common::assert_oracle_parity_expect("(ash 256 -4)", expect);
    let expect = expect_test::expect![r#""OK 1""#];
    crate::common::assert_oracle_parity_expect("(ash 256 -8)", expect);
    let expect = expect_test::expect![r#""OK 15""#];
    crate::common::assert_oracle_parity_expect("(ash 255 -4)", expect);
    let expect = expect_test::expect![r#""OK 0""#];
    crate::common::assert_oracle_parity_expect("(ash 1 -1)", expect);
}

#[test]
fn oracle_prop_ash_negative_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![r#""OK -2""#];
    // Arithmetic shift preserves sign
    crate::common::assert_oracle_parity_expect("(ash -1 1)", expect);
    let expect = expect_test::expect![r#""OK -16""#];
    crate::common::assert_oracle_parity_expect("(ash -256 -4)", expect);
    let expect = expect_test::expect![r#""OK -64""#];
    crate::common::assert_oracle_parity_expect("(ash -128 -1)", expect);
}

// ---------------------------------------------------------------------------
// Complex: bit manipulation patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_bitwise_flag_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set/clear/test flags pattern
    let form = "(let ((flags 0)
                      (read-flag 1)
                      (write-flag 2)
                      (exec-flag 4))
                  ;; Set read and write
                  (setq flags (logior flags read-flag write-flag))
                  (let ((has-read (not (= 0 (logand flags read-flag))))
                        (has-write (not (= 0 (logand flags write-flag))))
                        (has-exec (not (= 0 (logand flags exec-flag)))))
                    ;; Clear write flag
                    (setq flags (logand flags (lognot write-flag)))
                    (let ((after-clear-write
                           (not (= 0 (logand flags write-flag)))))
                      ;; Toggle exec flag
                      (setq flags (logxor flags exec-flag))
                      (let ((has-exec-now
                             (not (= 0 (logand flags exec-flag)))))
                        (list has-read has-write has-exec
                              after-clear-write has-exec-now
                              flags)))))";
    let expect = expect_test::expect![r#""OK (t t nil nil t 5)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bitwise_mask_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract bit fields from packed integer
    let form = "(let ((packed (logior (ash 5 8) (ash 3 4) 7)))
                  (let ((high (logand (ash packed -8) #xff))
                        (mid (logand (ash packed -4) #xf))
                        (low (logand packed #xf)))
                    (list high mid low)))";
    let expect = expect_test::expect![r#""OK (5 3 7)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_bitwise_population_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Count number of set bits
    let form = "(let ((popcount (lambda (n)
                    (let ((count 0) (val n))
                      (while (> val 0)
                        (when (= 1 (logand val 1))
                          (setq count (1+ count)))
                        (setq val (ash val -1)))
                      count))))
                  (list (funcall popcount 0)
                        (funcall popcount 1)
                        (funcall popcount 7)
                        (funcall popcount 255)
                        (funcall popcount 256)))";
    let expect = expect_test::expect![r#""OK (0 1 3 8 1)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// proptest: logand/logior/logxor identities
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_bitwise_demorgan(
        a in 0i64..65536,
        b in 0i64..65536,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        // De Morgan's law: ~(a & b) == (~a | ~b) — for positive range
        let form = format!(
            "(= (lognot (logand {} {}))
                (logior (lognot {}) (lognot {})))",
            a, b, a, b
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(neovm.as_str(), "OK t");
        prop_assert_eq!(oracle.as_str(), "OK t");
    }
}

//! Per-char char-to-string matrix over the sentinel-collision PUA ranges.
//!
//! Neomacs internal raw-byte sentinels (U+E080-E0FF) and unibyte sentinels
//! (U+E300-E3FF) collide with real Private Use Area chars; char-to-string /
//! format "%c" / princ corrupt those PUA chars into eight-bit sentinels.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_fpm_E080() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57472""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe080) 0)", expect);
}

#[test]
fn div_fpm_E081() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57473""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe081) 0)", expect);
}

#[test]
fn div_fpm_E082() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57474""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe082) 0)", expect);
}

#[test]
fn div_fpm_E083() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57475""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe083) 0)", expect);
}

#[test]
fn div_fpm_E084() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57476""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe084) 0)", expect);
}

#[test]
fn div_fpm_E085() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57477""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe085) 0)", expect);
}

#[test]
fn div_fpm_E086() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57478""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe086) 0)", expect);
}

#[test]
fn div_fpm_E087() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57479""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe087) 0)", expect);
}

#[test]
fn div_fpm_E088() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57480""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe088) 0)", expect);
}

#[test]
fn div_fpm_E089() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57481""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe089) 0)", expect);
}

#[test]
fn div_fpm_E08A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57482""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe08a) 0)", expect);
}

#[test]
fn div_fpm_E08B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57483""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe08b) 0)", expect);
}

#[test]
fn div_fpm_E08C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57484""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe08c) 0)", expect);
}

#[test]
fn div_fpm_E08D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57485""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe08d) 0)", expect);
}

#[test]
fn div_fpm_E08E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57486""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe08e) 0)", expect);
}

#[test]
fn div_fpm_E08F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57487""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe08f) 0)", expect);
}

#[test]
fn div_fpm_E090() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57488""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe090) 0)", expect);
}

#[test]
fn div_fpm_E091() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57489""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe091) 0)", expect);
}

#[test]
fn div_fpm_E092() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57490""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe092) 0)", expect);
}

#[test]
fn div_fpm_E093() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57491""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe093) 0)", expect);
}

#[test]
fn div_fpm_E094() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57492""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe094) 0)", expect);
}

#[test]
fn div_fpm_E095() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57493""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe095) 0)", expect);
}

#[test]
fn div_fpm_E096() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57494""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe096) 0)", expect);
}

#[test]
fn div_fpm_E097() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57495""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe097) 0)", expect);
}

#[test]
fn div_fpm_E098() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57496""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe098) 0)", expect);
}

#[test]
fn div_fpm_E099() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57497""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe099) 0)", expect);
}

#[test]
fn div_fpm_E09A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57498""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe09a) 0)", expect);
}

#[test]
fn div_fpm_E09B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57499""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe09b) 0)", expect);
}

#[test]
fn div_fpm_E09C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57500""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe09c) 0)", expect);
}

#[test]
fn div_fpm_E09D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57501""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe09d) 0)", expect);
}

#[test]
fn div_fpm_E09E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57502""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe09e) 0)", expect);
}

#[test]
fn div_fpm_E09F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57503""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe09f) 0)", expect);
}

#[test]
fn div_fpm_E0A0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57504""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a0) 0)", expect);
}

#[test]
fn div_fpm_E0A1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57505""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a1) 0)", expect);
}

#[test]
fn div_fpm_E0A2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57506""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a2) 0)", expect);
}

#[test]
fn div_fpm_E0A3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57507""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a3) 0)", expect);
}

#[test]
fn div_fpm_E0A4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57508""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a4) 0)", expect);
}

#[test]
fn div_fpm_E0A5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57509""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a5) 0)", expect);
}

#[test]
fn div_fpm_E0A6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57510""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a6) 0)", expect);
}

#[test]
fn div_fpm_E0A7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57511""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a7) 0)", expect);
}

#[test]
fn div_fpm_E0A8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57512""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a8) 0)", expect);
}

#[test]
fn div_fpm_E0A9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57513""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0a9) 0)", expect);
}

#[test]
fn div_fpm_E0AA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57514""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0aa) 0)", expect);
}

#[test]
fn div_fpm_E0AB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57515""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ab) 0)", expect);
}

#[test]
fn div_fpm_E0AC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57516""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ac) 0)", expect);
}

#[test]
fn div_fpm_E0AD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57517""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ad) 0)", expect);
}

#[test]
fn div_fpm_E0AE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57518""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ae) 0)", expect);
}

#[test]
fn div_fpm_E0AF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57519""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0af) 0)", expect);
}

#[test]
fn div_fpm_E0B0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57520""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b0) 0)", expect);
}

#[test]
fn div_fpm_E0B1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57521""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b1) 0)", expect);
}

#[test]
fn div_fpm_E0B2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57522""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b2) 0)", expect);
}

#[test]
fn div_fpm_E0B3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57523""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b3) 0)", expect);
}

#[test]
fn div_fpm_E0B4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57524""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b4) 0)", expect);
}

#[test]
fn div_fpm_E0B5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57525""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b5) 0)", expect);
}

#[test]
fn div_fpm_E0B6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57526""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b6) 0)", expect);
}

#[test]
fn div_fpm_E0B7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57527""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b7) 0)", expect);
}

#[test]
fn div_fpm_E0B8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57528""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b8) 0)", expect);
}

#[test]
fn div_fpm_E0B9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57529""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0b9) 0)", expect);
}

#[test]
fn div_fpm_E0BA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57530""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ba) 0)", expect);
}

#[test]
fn div_fpm_E0BB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57531""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0bb) 0)", expect);
}

#[test]
fn div_fpm_E0BC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57532""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0bc) 0)", expect);
}

#[test]
fn div_fpm_E0BD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57533""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0bd) 0)", expect);
}

#[test]
fn div_fpm_E0BE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57534""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0be) 0)", expect);
}

#[test]
fn div_fpm_E0BF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57535""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0bf) 0)", expect);
}

#[test]
fn div_fpm_E0C0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57536""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c0) 0)", expect);
}

#[test]
fn div_fpm_E0C1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57537""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c1) 0)", expect);
}

#[test]
fn div_fpm_E0C2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57538""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c2) 0)", expect);
}

#[test]
fn div_fpm_E0C3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57539""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c3) 0)", expect);
}

#[test]
fn div_fpm_E0C4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57540""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c4) 0)", expect);
}

#[test]
fn div_fpm_E0C5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57541""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c5) 0)", expect);
}

#[test]
fn div_fpm_E0C6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57542""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c6) 0)", expect);
}

#[test]
fn div_fpm_E0C7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57543""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c7) 0)", expect);
}

#[test]
fn div_fpm_E0C8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57544""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c8) 0)", expect);
}

#[test]
fn div_fpm_E0C9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57545""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0c9) 0)", expect);
}

#[test]
fn div_fpm_E0CA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57546""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ca) 0)", expect);
}

#[test]
fn div_fpm_E0CB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57547""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0cb) 0)", expect);
}

#[test]
fn div_fpm_E0CC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57548""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0cc) 0)", expect);
}

#[test]
fn div_fpm_E0CD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57549""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0cd) 0)", expect);
}

#[test]
fn div_fpm_E0CE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57550""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ce) 0)", expect);
}

#[test]
fn div_fpm_E0CF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57551""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0cf) 0)", expect);
}

#[test]
fn div_fpm_E0D0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57552""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d0) 0)", expect);
}

#[test]
fn div_fpm_E0D1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57553""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d1) 0)", expect);
}

#[test]
fn div_fpm_E0D2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57554""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d2) 0)", expect);
}

#[test]
fn div_fpm_E0D3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57555""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d3) 0)", expect);
}

#[test]
fn div_fpm_E0D4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57556""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d4) 0)", expect);
}

#[test]
fn div_fpm_E0D5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57557""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d5) 0)", expect);
}

#[test]
fn div_fpm_E0D6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57558""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d6) 0)", expect);
}

#[test]
fn div_fpm_E0D7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57559""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d7) 0)", expect);
}

#[test]
fn div_fpm_E0D8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57560""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d8) 0)", expect);
}

#[test]
fn div_fpm_E0D9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57561""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0d9) 0)", expect);
}

#[test]
fn div_fpm_E0DA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57562""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0da) 0)", expect);
}

#[test]
fn div_fpm_E0DB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57563""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0db) 0)", expect);
}

#[test]
fn div_fpm_E0DC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57564""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0dc) 0)", expect);
}

#[test]
fn div_fpm_E0DD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57565""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0dd) 0)", expect);
}

#[test]
fn div_fpm_E0DE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57566""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0de) 0)", expect);
}

#[test]
fn div_fpm_E0DF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57567""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0df) 0)", expect);
}

#[test]
fn div_fpm_E0E0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57568""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e0) 0)", expect);
}

#[test]
fn div_fpm_E0E1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57569""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e1) 0)", expect);
}

#[test]
fn div_fpm_E0E2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57570""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e2) 0)", expect);
}

#[test]
fn div_fpm_E0E3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57571""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e3) 0)", expect);
}

#[test]
fn div_fpm_E0E4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57572""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e4) 0)", expect);
}

#[test]
fn div_fpm_E0E5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57573""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e5) 0)", expect);
}

#[test]
fn div_fpm_E0E6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57574""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e6) 0)", expect);
}

#[test]
fn div_fpm_E0E7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57575""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e7) 0)", expect);
}

#[test]
fn div_fpm_E0E8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57576""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e8) 0)", expect);
}

#[test]
fn div_fpm_E0E9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57577""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0e9) 0)", expect);
}

#[test]
fn div_fpm_E0EA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57578""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ea) 0)", expect);
}

#[test]
fn div_fpm_E0EB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57579""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0eb) 0)", expect);
}

#[test]
fn div_fpm_E0EC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57580""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ec) 0)", expect);
}

#[test]
fn div_fpm_E0ED() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57581""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ed) 0)", expect);
}

#[test]
fn div_fpm_E0EE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57582""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ee) 0)", expect);
}

#[test]
fn div_fpm_E0EF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57583""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ef) 0)", expect);
}

#[test]
fn div_fpm_E0F0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57584""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f0) 0)", expect);
}

#[test]
fn div_fpm_E0F1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57585""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f1) 0)", expect);
}

#[test]
fn div_fpm_E0F2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57586""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f2) 0)", expect);
}

#[test]
fn div_fpm_E0F3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57587""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f3) 0)", expect);
}

#[test]
fn div_fpm_E0F4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57588""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f4) 0)", expect);
}

#[test]
fn div_fpm_E0F5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57589""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f5) 0)", expect);
}

#[test]
fn div_fpm_E0F6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57590""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f6) 0)", expect);
}

#[test]
fn div_fpm_E0F7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57591""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f7) 0)", expect);
}

#[test]
fn div_fpm_E0F8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57592""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f8) 0)", expect);
}

#[test]
fn div_fpm_E0F9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57593""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0f9) 0)", expect);
}

#[test]
fn div_fpm_E0FA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57594""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0fa) 0)", expect);
}

#[test]
fn div_fpm_E0FB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57595""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0fb) 0)", expect);
}

#[test]
fn div_fpm_E0FC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57596""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0fc) 0)", expect);
}

#[test]
fn div_fpm_E0FD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57597""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0fd) 0)", expect);
}

#[test]
fn div_fpm_E0FE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57598""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0fe) 0)", expect);
}

#[test]
fn div_fpm_E0FF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57599""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe0ff) 0)", expect);
}

#[test]
fn div_fpm_E300() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58112""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe300) 0)", expect);
}

#[test]
fn div_fpm_E301() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58113""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe301) 0)", expect);
}

#[test]
fn div_fpm_E302() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58114""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe302) 0)", expect);
}

#[test]
fn div_fpm_E303() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58115""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe303) 0)", expect);
}

#[test]
fn div_fpm_E304() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58116""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe304) 0)", expect);
}

#[test]
fn div_fpm_E305() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58117""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe305) 0)", expect);
}

#[test]
fn div_fpm_E306() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58118""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe306) 0)", expect);
}

#[test]
fn div_fpm_E307() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58119""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe307) 0)", expect);
}

#[test]
fn div_fpm_E308() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58120""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe308) 0)", expect);
}

#[test]
fn div_fpm_E309() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58121""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe309) 0)", expect);
}

#[test]
fn div_fpm_E30A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58122""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe30a) 0)", expect);
}

#[test]
fn div_fpm_E30B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58123""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe30b) 0)", expect);
}

#[test]
fn div_fpm_E30C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58124""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe30c) 0)", expect);
}

#[test]
fn div_fpm_E30D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58125""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe30d) 0)", expect);
}

#[test]
fn div_fpm_E30E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58126""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe30e) 0)", expect);
}

#[test]
fn div_fpm_E30F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58127""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe30f) 0)", expect);
}

#[test]
fn div_fpm_E310() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58128""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe310) 0)", expect);
}

#[test]
fn div_fpm_E311() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58129""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe311) 0)", expect);
}

#[test]
fn div_fpm_E312() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58130""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe312) 0)", expect);
}

#[test]
fn div_fpm_E313() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58131""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe313) 0)", expect);
}

#[test]
fn div_fpm_E314() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58132""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe314) 0)", expect);
}

#[test]
fn div_fpm_E315() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58133""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe315) 0)", expect);
}

#[test]
fn div_fpm_E316() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58134""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe316) 0)", expect);
}

#[test]
fn div_fpm_E317() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58135""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe317) 0)", expect);
}

#[test]
fn div_fpm_E318() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58136""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe318) 0)", expect);
}

#[test]
fn div_fpm_E319() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58137""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe319) 0)", expect);
}

#[test]
fn div_fpm_E31A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58138""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe31a) 0)", expect);
}

#[test]
fn div_fpm_E31B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58139""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe31b) 0)", expect);
}

#[test]
fn div_fpm_E31C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58140""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe31c) 0)", expect);
}

#[test]
fn div_fpm_E31D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58141""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe31d) 0)", expect);
}

#[test]
fn div_fpm_E31E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58142""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe31e) 0)", expect);
}

#[test]
fn div_fpm_E31F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58143""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe31f) 0)", expect);
}

#[test]
fn div_fpm_E320() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58144""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe320) 0)", expect);
}

#[test]
fn div_fpm_E321() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58145""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe321) 0)", expect);
}

#[test]
fn div_fpm_E322() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58146""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe322) 0)", expect);
}

#[test]
fn div_fpm_E323() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58147""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe323) 0)", expect);
}

#[test]
fn div_fpm_E324() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58148""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe324) 0)", expect);
}

#[test]
fn div_fpm_E325() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58149""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe325) 0)", expect);
}

#[test]
fn div_fpm_E326() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58150""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe326) 0)", expect);
}

#[test]
fn div_fpm_E327() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58151""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe327) 0)", expect);
}

#[test]
fn div_fpm_E328() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58152""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe328) 0)", expect);
}

#[test]
fn div_fpm_E329() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58153""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe329) 0)", expect);
}

#[test]
fn div_fpm_E32A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58154""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe32a) 0)", expect);
}

#[test]
fn div_fpm_E32B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58155""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe32b) 0)", expect);
}

#[test]
fn div_fpm_E32C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58156""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe32c) 0)", expect);
}

#[test]
fn div_fpm_E32D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58157""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe32d) 0)", expect);
}

#[test]
fn div_fpm_E32E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58158""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe32e) 0)", expect);
}

#[test]
fn div_fpm_E32F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58159""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe32f) 0)", expect);
}

#[test]
fn div_fpm_E330() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58160""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe330) 0)", expect);
}

#[test]
fn div_fpm_E331() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58161""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe331) 0)", expect);
}

#[test]
fn div_fpm_E332() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58162""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe332) 0)", expect);
}

#[test]
fn div_fpm_E333() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58163""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe333) 0)", expect);
}

#[test]
fn div_fpm_E334() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58164""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe334) 0)", expect);
}

#[test]
fn div_fpm_E335() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58165""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe335) 0)", expect);
}

#[test]
fn div_fpm_E336() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58166""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe336) 0)", expect);
}

#[test]
fn div_fpm_E337() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58167""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe337) 0)", expect);
}

#[test]
fn div_fpm_E338() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58168""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe338) 0)", expect);
}

#[test]
fn div_fpm_E339() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58169""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe339) 0)", expect);
}

#[test]
fn div_fpm_E33A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58170""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe33a) 0)", expect);
}

#[test]
fn div_fpm_E33B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58171""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe33b) 0)", expect);
}

#[test]
fn div_fpm_E33C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58172""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe33c) 0)", expect);
}

#[test]
fn div_fpm_E33D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58173""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe33d) 0)", expect);
}

#[test]
fn div_fpm_E33E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58174""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe33e) 0)", expect);
}

#[test]
fn div_fpm_E33F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58175""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe33f) 0)", expect);
}

#[test]
fn div_fpm_E340() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58176""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe340) 0)", expect);
}

#[test]
fn div_fpm_E341() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58177""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe341) 0)", expect);
}

#[test]
fn div_fpm_E342() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58178""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe342) 0)", expect);
}

#[test]
fn div_fpm_E343() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58179""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe343) 0)", expect);
}

#[test]
fn div_fpm_E344() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58180""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe344) 0)", expect);
}

#[test]
fn div_fpm_E345() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58181""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe345) 0)", expect);
}

#[test]
fn div_fpm_E346() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58182""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe346) 0)", expect);
}

#[test]
fn div_fpm_E347() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58183""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe347) 0)", expect);
}

#[test]
fn div_fpm_E348() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58184""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe348) 0)", expect);
}

#[test]
fn div_fpm_E349() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58185""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe349) 0)", expect);
}

#[test]
fn div_fpm_E34A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58186""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe34a) 0)", expect);
}

#[test]
fn div_fpm_E34B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58187""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe34b) 0)", expect);
}

#[test]
fn div_fpm_E34C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58188""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe34c) 0)", expect);
}

#[test]
fn div_fpm_E34D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58189""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe34d) 0)", expect);
}

#[test]
fn div_fpm_E34E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58190""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe34e) 0)", expect);
}

#[test]
fn div_fpm_E34F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58191""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe34f) 0)", expect);
}

#[test]
fn div_fpm_E350() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58192""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe350) 0)", expect);
}

#[test]
fn div_fpm_E351() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58193""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe351) 0)", expect);
}

#[test]
fn div_fpm_E352() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58194""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe352) 0)", expect);
}

#[test]
fn div_fpm_E353() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58195""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe353) 0)", expect);
}

#[test]
fn div_fpm_E354() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58196""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe354) 0)", expect);
}

#[test]
fn div_fpm_E355() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58197""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe355) 0)", expect);
}

#[test]
fn div_fpm_E356() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58198""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe356) 0)", expect);
}

#[test]
fn div_fpm_E357() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58199""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe357) 0)", expect);
}

#[test]
fn div_fpm_E358() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58200""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe358) 0)", expect);
}

#[test]
fn div_fpm_E359() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58201""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe359) 0)", expect);
}

#[test]
fn div_fpm_E35A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58202""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe35a) 0)", expect);
}

#[test]
fn div_fpm_E35B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58203""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe35b) 0)", expect);
}

#[test]
fn div_fpm_E35C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58204""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe35c) 0)", expect);
}

#[test]
fn div_fpm_E35D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58205""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe35d) 0)", expect);
}

#[test]
fn div_fpm_E35E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58206""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe35e) 0)", expect);
}

#[test]
fn div_fpm_E35F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58207""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe35f) 0)", expect);
}

#[test]
fn div_fpm_E360() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58208""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe360) 0)", expect);
}

#[test]
fn div_fpm_E361() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58209""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe361) 0)", expect);
}

#[test]
fn div_fpm_E362() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58210""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe362) 0)", expect);
}

#[test]
fn div_fpm_E363() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58211""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe363) 0)", expect);
}

#[test]
fn div_fpm_E364() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58212""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe364) 0)", expect);
}

#[test]
fn div_fpm_E365() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58213""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe365) 0)", expect);
}

#[test]
fn div_fpm_E366() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58214""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe366) 0)", expect);
}

#[test]
fn div_fpm_E367() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58215""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe367) 0)", expect);
}

#[test]
fn div_fpm_E368() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58216""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe368) 0)", expect);
}

#[test]
fn div_fpm_E369() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58217""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe369) 0)", expect);
}

#[test]
fn div_fpm_E36A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58218""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe36a) 0)", expect);
}

#[test]
fn div_fpm_E36B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58219""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe36b) 0)", expect);
}

#[test]
fn div_fpm_E36C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58220""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe36c) 0)", expect);
}

#[test]
fn div_fpm_E36D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58221""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe36d) 0)", expect);
}

#[test]
fn div_fpm_E36E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58222""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe36e) 0)", expect);
}

#[test]
fn div_fpm_E36F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58223""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe36f) 0)", expect);
}

#[test]
fn div_fpm_E370() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58224""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe370) 0)", expect);
}

#[test]
fn div_fpm_E371() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58225""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe371) 0)", expect);
}

#[test]
fn div_fpm_E372() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58226""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe372) 0)", expect);
}

#[test]
fn div_fpm_E373() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58227""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe373) 0)", expect);
}

#[test]
fn div_fpm_E374() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58228""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe374) 0)", expect);
}

#[test]
fn div_fpm_E375() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58229""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe375) 0)", expect);
}

#[test]
fn div_fpm_E376() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58230""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe376) 0)", expect);
}

#[test]
fn div_fpm_E377() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58231""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe377) 0)", expect);
}

#[test]
fn div_fpm_E378() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58232""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe378) 0)", expect);
}

#[test]
fn div_fpm_E379() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58233""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe379) 0)", expect);
}

#[test]
fn div_fpm_E37A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58234""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe37a) 0)", expect);
}

#[test]
fn div_fpm_E37B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58235""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe37b) 0)", expect);
}

#[test]
fn div_fpm_E37C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58236""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe37c) 0)", expect);
}

#[test]
fn div_fpm_E37D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58237""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe37d) 0)", expect);
}

#[test]
fn div_fpm_E37E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58238""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe37e) 0)", expect);
}

#[test]
fn div_fpm_E37F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58239""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe37f) 0)", expect);
}

#[test]
fn div_fpm_E380() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58240""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe380) 0)", expect);
}

#[test]
fn div_fpm_E381() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58241""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe381) 0)", expect);
}

#[test]
fn div_fpm_E382() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58242""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe382) 0)", expect);
}

#[test]
fn div_fpm_E383() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58243""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe383) 0)", expect);
}

#[test]
fn div_fpm_E384() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58244""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe384) 0)", expect);
}

#[test]
fn div_fpm_E385() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58245""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe385) 0)", expect);
}

#[test]
fn div_fpm_E386() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58246""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe386) 0)", expect);
}

#[test]
fn div_fpm_E387() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58247""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe387) 0)", expect);
}

#[test]
fn div_fpm_E388() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58248""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe388) 0)", expect);
}

#[test]
fn div_fpm_E389() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58249""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe389) 0)", expect);
}

#[test]
fn div_fpm_E38A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58250""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe38a) 0)", expect);
}

#[test]
fn div_fpm_E38B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58251""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe38b) 0)", expect);
}

#[test]
fn div_fpm_E38C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58252""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe38c) 0)", expect);
}

#[test]
fn div_fpm_E38D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58253""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe38d) 0)", expect);
}

#[test]
fn div_fpm_E38E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58254""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe38e) 0)", expect);
}

#[test]
fn div_fpm_E38F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58255""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe38f) 0)", expect);
}

#[test]
fn div_fpm_E390() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58256""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe390) 0)", expect);
}

#[test]
fn div_fpm_E391() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58257""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe391) 0)", expect);
}

#[test]
fn div_fpm_E392() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58258""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe392) 0)", expect);
}

#[test]
fn div_fpm_E393() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58259""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe393) 0)", expect);
}

#[test]
fn div_fpm_E394() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58260""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe394) 0)", expect);
}

#[test]
fn div_fpm_E395() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58261""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe395) 0)", expect);
}

#[test]
fn div_fpm_E396() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58262""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe396) 0)", expect);
}

#[test]
fn div_fpm_E397() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58263""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe397) 0)", expect);
}

#[test]
fn div_fpm_E398() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58264""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe398) 0)", expect);
}

#[test]
fn div_fpm_E399() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58265""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe399) 0)", expect);
}

#[test]
fn div_fpm_E39A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58266""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe39a) 0)", expect);
}

#[test]
fn div_fpm_E39B() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58267""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe39b) 0)", expect);
}

#[test]
fn div_fpm_E39C() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58268""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe39c) 0)", expect);
}

#[test]
fn div_fpm_E39D() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58269""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe39d) 0)", expect);
}

#[test]
fn div_fpm_E39E() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58270""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe39e) 0)", expect);
}

#[test]
fn div_fpm_E39F() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58271""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe39f) 0)", expect);
}

#[test]
fn div_fpm_E3A0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58272""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a0) 0)", expect);
}

#[test]
fn div_fpm_E3A1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58273""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a1) 0)", expect);
}

#[test]
fn div_fpm_E3A2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58274""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a2) 0)", expect);
}

#[test]
fn div_fpm_E3A3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58275""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a3) 0)", expect);
}

#[test]
fn div_fpm_E3A4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58276""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a4) 0)", expect);
}

#[test]
fn div_fpm_E3A5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58277""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a5) 0)", expect);
}

#[test]
fn div_fpm_E3A6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58278""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a6) 0)", expect);
}

#[test]
fn div_fpm_E3A7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58279""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a7) 0)", expect);
}

#[test]
fn div_fpm_E3A8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58280""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a8) 0)", expect);
}

#[test]
fn div_fpm_E3A9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58281""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3a9) 0)", expect);
}

#[test]
fn div_fpm_E3AA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58282""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3aa) 0)", expect);
}

#[test]
fn div_fpm_E3AB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58283""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ab) 0)", expect);
}

#[test]
fn div_fpm_E3AC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58284""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ac) 0)", expect);
}

#[test]
fn div_fpm_E3AD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58285""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ad) 0)", expect);
}

#[test]
fn div_fpm_E3AE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58286""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ae) 0)", expect);
}

#[test]
fn div_fpm_E3AF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58287""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3af) 0)", expect);
}

#[test]
fn div_fpm_E3B0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58288""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b0) 0)", expect);
}

#[test]
fn div_fpm_E3B1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58289""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b1) 0)", expect);
}

#[test]
fn div_fpm_E3B2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58290""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b2) 0)", expect);
}

#[test]
fn div_fpm_E3B3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58291""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b3) 0)", expect);
}

#[test]
fn div_fpm_E3B4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58292""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b4) 0)", expect);
}

#[test]
fn div_fpm_E3B5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58293""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b5) 0)", expect);
}

#[test]
fn div_fpm_E3B6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58294""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b6) 0)", expect);
}

#[test]
fn div_fpm_E3B7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58295""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b7) 0)", expect);
}

#[test]
fn div_fpm_E3B8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58296""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b8) 0)", expect);
}

#[test]
fn div_fpm_E3B9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58297""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3b9) 0)", expect);
}

#[test]
fn div_fpm_E3BA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58298""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ba) 0)", expect);
}

#[test]
fn div_fpm_E3BB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58299""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3bb) 0)", expect);
}

#[test]
fn div_fpm_E3BC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58300""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3bc) 0)", expect);
}

#[test]
fn div_fpm_E3BD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58301""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3bd) 0)", expect);
}

#[test]
fn div_fpm_E3BE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58302""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3be) 0)", expect);
}

#[test]
fn div_fpm_E3BF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58303""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3bf) 0)", expect);
}

#[test]
fn div_fpm_E3C0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58304""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c0) 0)", expect);
}

#[test]
fn div_fpm_E3C1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58305""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c1) 0)", expect);
}

#[test]
fn div_fpm_E3C2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58306""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c2) 0)", expect);
}

#[test]
fn div_fpm_E3C3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58307""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c3) 0)", expect);
}

#[test]
fn div_fpm_E3C4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58308""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c4) 0)", expect);
}

#[test]
fn div_fpm_E3C5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58309""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c5) 0)", expect);
}

#[test]
fn div_fpm_E3C6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58310""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c6) 0)", expect);
}

#[test]
fn div_fpm_E3C7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58311""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c7) 0)", expect);
}

#[test]
fn div_fpm_E3C8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58312""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c8) 0)", expect);
}

#[test]
fn div_fpm_E3C9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58313""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3c9) 0)", expect);
}

#[test]
fn div_fpm_E3CA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58314""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ca) 0)", expect);
}

#[test]
fn div_fpm_E3CB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58315""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3cb) 0)", expect);
}

#[test]
fn div_fpm_E3CC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58316""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3cc) 0)", expect);
}

#[test]
fn div_fpm_E3CD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58317""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3cd) 0)", expect);
}

#[test]
fn div_fpm_E3CE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58318""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ce) 0)", expect);
}

#[test]
fn div_fpm_E3CF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58319""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3cf) 0)", expect);
}

#[test]
fn div_fpm_E3D0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58320""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d0) 0)", expect);
}

#[test]
fn div_fpm_E3D1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58321""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d1) 0)", expect);
}

#[test]
fn div_fpm_E3D2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58322""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d2) 0)", expect);
}

#[test]
fn div_fpm_E3D3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58323""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d3) 0)", expect);
}

#[test]
fn div_fpm_E3D4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58324""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d4) 0)", expect);
}

#[test]
fn div_fpm_E3D5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58325""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d5) 0)", expect);
}

#[test]
fn div_fpm_E3D6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58326""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d6) 0)", expect);
}

#[test]
fn div_fpm_E3D7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58327""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d7) 0)", expect);
}

#[test]
fn div_fpm_E3D8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58328""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d8) 0)", expect);
}

#[test]
fn div_fpm_E3D9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58329""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3d9) 0)", expect);
}

#[test]
fn div_fpm_E3DA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58330""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3da) 0)", expect);
}

#[test]
fn div_fpm_E3DB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58331""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3db) 0)", expect);
}

#[test]
fn div_fpm_E3DC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58332""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3dc) 0)", expect);
}

#[test]
fn div_fpm_E3DD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58333""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3dd) 0)", expect);
}

#[test]
fn div_fpm_E3DE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58334""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3de) 0)", expect);
}

#[test]
fn div_fpm_E3DF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58335""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3df) 0)", expect);
}

#[test]
fn div_fpm_E3E0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58336""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e0) 0)", expect);
}

#[test]
fn div_fpm_E3E1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58337""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e1) 0)", expect);
}

#[test]
fn div_fpm_E3E2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58338""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e2) 0)", expect);
}

#[test]
fn div_fpm_E3E3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58339""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e3) 0)", expect);
}

#[test]
fn div_fpm_E3E4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58340""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e4) 0)", expect);
}

#[test]
fn div_fpm_E3E5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58341""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e5) 0)", expect);
}

#[test]
fn div_fpm_E3E6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58342""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e6) 0)", expect);
}

#[test]
fn div_fpm_E3E7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58343""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e7) 0)", expect);
}

#[test]
fn div_fpm_E3E8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58344""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e8) 0)", expect);
}

#[test]
fn div_fpm_E3E9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58345""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3e9) 0)", expect);
}

#[test]
fn div_fpm_E3EA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58346""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ea) 0)", expect);
}

#[test]
fn div_fpm_E3EB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58347""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3eb) 0)", expect);
}

#[test]
fn div_fpm_E3EC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58348""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ec) 0)", expect);
}

#[test]
fn div_fpm_E3ED() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58349""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ed) 0)", expect);
}

#[test]
fn div_fpm_E3EE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58350""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ee) 0)", expect);
}

#[test]
fn div_fpm_E3EF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58351""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ef) 0)", expect);
}

#[test]
fn div_fpm_E3F0() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58352""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f0) 0)", expect);
}

#[test]
fn div_fpm_E3F1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58353""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f1) 0)", expect);
}

#[test]
fn div_fpm_E3F2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58354""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f2) 0)", expect);
}

#[test]
fn div_fpm_E3F3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58355""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f3) 0)", expect);
}

#[test]
fn div_fpm_E3F4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58356""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f4) 0)", expect);
}

#[test]
fn div_fpm_E3F5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58357""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f5) 0)", expect);
}

#[test]
fn div_fpm_E3F6() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58358""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f6) 0)", expect);
}

#[test]
fn div_fpm_E3F7() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58359""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f7) 0)", expect);
}

#[test]
fn div_fpm_E3F8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58360""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f8) 0)", expect);
}

#[test]
fn div_fpm_E3F9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58361""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3f9) 0)", expect);
}

#[test]
fn div_fpm_E3FA() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58362""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3fa) 0)", expect);
}

#[test]
fn div_fpm_E3FB() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58363""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3fb) 0)", expect);
}

#[test]
fn div_fpm_E3FC() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58364""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3fc) 0)", expect);
}

#[test]
fn div_fpm_E3FD() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58365""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3fd) 0)", expect);
}

#[test]
fn div_fpm_E3FE() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58366""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3fe) 0)", expect);
}

#[test]
fn div_fpm_E3FF() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58367""#]];
    crate::common::assert_oracle_parity_expect("(aref (char-to-string #xe3ff) 0)", expect);
}

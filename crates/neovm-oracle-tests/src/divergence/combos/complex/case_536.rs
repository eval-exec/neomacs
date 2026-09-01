/// Batch 536: rx, pcase, string-case, char-fold-to-regexp deep probes.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx536_rx_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match (rx "hello") "hello world")
"##,
        expect,
    );
}

#[test]
fn div_cx536_rx_or() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match (rx (or "cat" "dog")) "doghouse")
"##,
        expect,
    );
}

#[test]
fn div_cx536_rx_and() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match (rx (and "abc" "def")) "abcdef")
"##,
        expect,
    );
}

#[test]
fn div_cx536_rx_char_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match (rx (any "a-z")) "5")
"##,
        expect,
    );
}

#[test]
fn div_cx536_rx_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match (rx (+ (in "0-9"))) "abc123def")
"##,
        expect,
    );
}

#[test]
fn div_cx536_rx_opt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match (rx "ab" (? "c") "d") "abd")
"##,
        expect,
    );
}

#[test]
fn div_cx536_rx_minimal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match (rx (minimal-match (one-or-more any)) "b") "aabb")
"##,
        expect,
    );
}

#[test]
fn div_cx536_rx_maximal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(string-match (rx (maximal-match (one-or-more any)) "b") "aabb")
"##,
        expect,
    );
}

#[test]
fn div_cx536_char_fold_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:a[\u{300}-\u{304}\u{306}-\u{30a}\u{30c}\u{30f}\u{311}\u{323}\u{325}\u{328}]\\\\|[aªà-åāăąǎǟǡǻȁȃȧᵃḁạảấầẩẫậắằẳẵặₐⓐａ𝐚𝑎𝒂𝒶𝓪𝔞𝕒𝖆𝖺𝗮𝘢𝙖𝚊]\\\\)\" \"\\\\(?:e[\u{300}-\u{304}\u{306}-\u{309}\u{30c}\u{30f}\u{311}\u{323}\u{327}\u{328}\u{32d}\u{330}]\\\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\\\)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-fold-to-regexp "a") (char-fold-to-regexp "e"))
"##,
        expect,
    );
}

#[test]
fn div_cx536_char_fold_accent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (string-match (char-fold-to-regexp "cafe") "café"))
"##,
        expect,
    );
}

#[test]
fn div_cx536_char_fold_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t))
  (string-match (char-fold-to-regexp "αβγ") "αβγδε"))
"##,
        expect,
    );
}

#[test]
fn div_cx536_pcase_let_star() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase-let* ((`(,a ,b) '(1 2))
                (c (+ a b)))
  c)
"##,
        expect,
    );
}

#[test]
fn div_cx536_pcase_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 (2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase-let ((`(,a . ,b) '(1 2 3 4)))
  (list a b))
"##,
        expect,
    );
}

#[test]
fn div_cx536_pcase_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase-exhaustive '(1 2 3)
  (`(,a ,b ,c) (+ a b c)))
"##,
        expect,
    );
}

#[test]
fn div_cx536_pcase_dolist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 7 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let (result)
  (pcase-dolist (`(,a ,b) '((1 2) (3 4) (5 6)))
    (push (+ a b) result))
  (nreverse result))
"##,
        expect,
    );
}

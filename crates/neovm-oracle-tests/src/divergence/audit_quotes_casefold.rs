//! Quote-style (format-message) + asymmetric case-fold divergences.
//!
//! Two source-audit veins:
//!  (a) `format-message` converts ` → ‘ and ' → ’; many error/warn paths use it,
//!      so GNU emits curly quotes where Neomacs (missing the conversion) emits
//!      straight quotes — affects every error-message comparison.
//!  (b) asymmetric case-fold: `string-match` lower→upper folding is one-directional
//!      in Neomacs (σ fails to match Σ, though Σ→σ works).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- format-message quote conversion ----------------------------------------

#[test]
fn div_aq_format_message_backtick_curly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a ‘b’ c\"""#]];
    crate::common::assert_oracle_parity_expect(r##"(format-message "a `b' c")"##, expect);
}

#[test]
fn div_aq_format_message_apostrophe_curly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"don’t ‘do’ it\"""#]];
    crate::common::assert_oracle_parity_expect(r##"(format-message "don't `do' it")"##, expect);
}

#[test]
fn div_aq_error_message_quote_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Invalid use of ‘\\\\’ in replacement text\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e (replace-regexp-in-string "x" "\\z" "x") (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_aq_user_error_quote_style() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"bad ‘foo’\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e (user-error "bad `%s'" 'foo) (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_aq_signal_message_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK stringp""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e (signal 'wrong-type-argument (list 'stringp 5)) (error (cadr e)))
"##,
        expect,
    );
}

#[test]
fn div_aq_message_with_format_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a `b'\" \"a ‘b’\")""#]];
    // message uses format (straight); format-message uses curly.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "a `%s'" 'b) (format-message "a `%s'" 'b))
"##,
        expect,
    );
}

// --- asymmetric case-fold (lower -> upper) ----------------------------------

#[test]
fn div_acf_sigma_lower_to_upper() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t)) (string-match "σ" "Σ"))"##,
        expect,
    );
}

#[test]
fn div_acf_sigma_upper_to_lower() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t)) (string-match "Σ" "σ"))"##,
        expect,
    );
}

#[test]
fn div_acf_alpha_lower_to_upper() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t)) (string-match "α" "Α"))"##,
        expect,
    );
}

#[test]
fn div_acf_omega_lower_to_upper() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((case-fold-search t)) (string-match "ω" "Ω"))"##,
        expect,
    );
}

#[test]
fn div_acf_greek_lowercase_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    // Probe several Greek lowercase -> uppercase case-fold matches.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (mapcar (lambda (p) (if (string-match (char-to-string (car p))
                                         (char-to-string (cdr p)))
                          t nil))
          '((945 . 913) (946 . 914) (956 . 924) (969 . 937))))
"##,
        expect,
    );
}

#[test]
fn div_acf_cyrillic_lower_to_upper() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "б" "Б") (string-match "я" "Я")))
"##,
        expect,
    );
}

#[test]
fn div_acf_ascii_case_fold_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    // ASCII case-fold should work in both directions.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match "a" "A") (string-match "A" "a")))
"##,
        expect,
    );
}

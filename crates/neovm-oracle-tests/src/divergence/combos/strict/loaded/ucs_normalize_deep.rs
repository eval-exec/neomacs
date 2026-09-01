//! Strict combo oracle probes, batch 57: DEEP characterization of the
//! ucs-normalize decomposition bug. Tests NFC composition (decomposed ->
//! precomposed), Hangul syllable algorithmic composition/decomposition, and an
//! NFD sweep over Latin/Greek/Cyrillic precomposed chars to determine whether
//! decomposition is totally broken or partial.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_l1_nfc_composition_from_decomposed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"è\" \"ñ\" \"ö\" \"ü\")""#]];
    // Characterization (parity): NFC composition (decomposed -> precomposed)
    // WORKS in Neomacs — the composition table/path is functional.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (ucs-normalize-NFC-string "è")
      (ucs-normalize-NFC-string "ñ")
      (ucs-normalize-NFC-string "ö")
      (ucs-normalize-NFC-string "ü"))
"##,
        &["international/ucs-normalize.el"],
        expect,
    );
}

#[test]
fn div_l1_hangul_compose_decompose() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"가\" \"가\" 2 \"각\" 3)""#]];
    // Characterization (parity): Hangul algorithmic NFC/NFD composition AND
    // decomposition WORK in Neomacs — the special-case algorithm is functional.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (ucs-normalize-NFC-string "가")
      (ucs-normalize-NFD-string "가")
      (length (ucs-normalize-NFD-string "가"))
      (ucs-normalize-NFC-string "각")
      (length (ucs-normalize-NFD-string "각")))
"##,
        &["international/ucs-normalize.el"],
        expect,
    );
}

#[test]
fn div_l1_nfd_sweep_precomposed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 2 2 1 1 1)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (2 2 2 2 1 1 1)
    // Neomacs:   OK (1 1 1 1 1 1 1)
    // NFD table-based canonical decomposition is broken: Latin precomposed
    // chars (a u n o) stay 1 char in Neomacs but decompose to 2 in GNU. Chars
    // without a decomposition (Omega, Cyrillic ya, o-slash) agree at 1.
    // (Hangul algorithmic decomposition and NFC composition DO work.)
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (length (ucs-normalize-NFD-string "à"))
      (length (ucs-normalize-NFD-string "ü"))
      (length (ucs-normalize-NFD-string "ñ"))
      (length (ucs-normalize-NFD-string "ö"))
      (length (ucs-normalize-NFD-string "Ω"))
      (length (ucs-normalize-NFD-string "я"))
      (length (ucs-normalize-NFD-string "ø")))
"##,
        &["international/ucs-normalize.el"],
        expect,
    );
}

#[test]
fn div_l1_nfc_nfd_idempotent_precomposed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"é\" \"café\" \"日本\" \"Ω\" \"Æ\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (ucs-normalize-NFC-string "é")
      (ucs-normalize-NFC-string "café")
      (ucs-normalize-NFC-string "日本")
      (ucs-normalize-NFC-string "Ω")
      (ucs-normalize-NFC-string "Æ"))
"##,
        &["international/ucs-normalize.el"],
        expect,
    );
}

#[test]
fn div_l1_nfkc_canonical_and_compat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Ω\" \"(株)\" \"°C\" \"fi\" \"TM\")""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK ("Ω" "(株)" "°C" "fi" "TM")
    // Neomacs:   OK ("Ω" "(株)" "°C" "ﬁ" "™")
    // NFKD/NFKC compatibility decomposition is broken: the ligature ﬁ stays ﬁ
    // (GNU -> fi) and trademark ™ stays ™ (GNU -> TM) in Neomacs. The
    // ideographic (株) and ℃ decompositions agree.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (ucs-normalize-NFKC-string "Ω")
      (ucs-normalize-NFKC-string "㈱")
      (ucs-normalize-NFKC-string "℃")
      (ucs-normalize-NFKD-string "ﬁ")
      (ucs-normalize-NFKD-string "™"))
"##,
        &["international/ucs-normalize.el"],
        expect,
    );
}

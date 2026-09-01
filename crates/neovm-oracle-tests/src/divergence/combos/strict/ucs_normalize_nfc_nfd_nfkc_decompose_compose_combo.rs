//! Strict combo oracle probes, batch 179: Unicode normalization (ucs-normalize).
//! NFC/NFD/NFKC of latin+diacritic, compatibility decomposables (ligatures,
//! circled numerals, parenthesized), and Hangul syllable decomposition. This
//! is a KNOWN divergence area; tests are committed failing per the project's
//! divergences convention.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_ucs_normalize_nfc_nfd_combining() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'ucs-normalize)
(list (ucs-normalize-string "café" 'NFC)
      (ucs-normalize-string "café" 'NFD)
      (ucs-normalize-string (string ?a ?̈) 'NFC)
      (ucs-normalize-string (string ?a ?̈) 'NFD)
      (ucs-normalize-string "Ω²" 'NFC)
      (ucs-normalize-string (string ?o ?̈ ?̄) 'NFC))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_ucs_normalize_nfkc_compatibility_ligatures() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'ucs-normalize)
(list (ucs-normalize-string "ﬁ" 'NFKC)
      (ucs-normalize-string "ﬃ" 'NFKC)
      (ucs-normalize-string "①" 'NFKC)
      (ucs-normalize-string "㈎" 'NFKC)
      (ucs-normalize-string "½" 'NFKC)
      (ucs-normalize-string "ℌ" 'NFKC)
      (ucs-normalize-string "TEST" 'NFKC)
      (ucs-normalize-string "café" 'NFKC)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_ucs_normalize_hangul_region_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'ucs-normalize)
(with-temp-buffer
  (insert "한국어 안녕")
  (ucs-normalize-region (point-min) (point-max) 'NFC)
  (let ((nfc-result (buffer-string)))
    (erase-buffer)
    (insert "한국어 안녕")
    (ucs-normalize-region (point-min) (point-max) 'NFD)
    (list nfc-result
          (buffer-string)
          (ucs-normalize-string "가나다" 'NFC)
          (ucs-normalize-string "가나다" 'NFD)
          (length (ucs-normalize-string "가" 'NFD)))))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (5 . 5) 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

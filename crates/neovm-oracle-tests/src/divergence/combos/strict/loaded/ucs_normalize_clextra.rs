//! Strict combo oracle probes, batch 40: Unicode normalization
//! (international/ucs-normalize.el — NFC/NFD/NFKC/NFKD of combining sequences
//! and compatibility decompositions) and cl-extra deep edges, via
//! assert_oracle_parity_with_load. Unicode normalization is a complex
//! table-driven algorithm with high divergence potential.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_h7_ucs_normalize_nfc_nfd() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"é\" \"é\" \"e\u{301}\" 2 \"ö\" \"o\u{308}\")""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK ("é" "é" "é" 2 "ö" "ö")
    // Neomacs:   OK ("é" "é" "é" 1 "ö" "ö")
    // ucs-normalize-NFD-string does NOT decompose precomposed characters:
    // NFD("é") is 2 chars in GNU (e + combining acute) but 1 in Neomacs
    // (unchanged). NFC composition agrees.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (ucs-normalize-NFC-string "é")
      (ucs-normalize-NFC-string "é")
      (ucs-normalize-NFD-string "é")
      (length (ucs-normalize-NFD-string "é"))
      (ucs-normalize-NFC-string "ö")
      (ucs-normalize-NFD-string "ö"))
"##,
        &["international/ucs-normalize.el"],
        expect,
    );
}

#[test]
fn div_h7_ucs_normalize_nfkc_compat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"fi\" \"ffi\" \"1⁄2\" \"°C\" \"(株)\")""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK ("fi" "ffi" "1⁄2" "°C" "(株)")
    // Neomacs:   OK ("fi" "ffi" "½" "°C" "(株)")
    // ucs-normalize-NFKD-string does not decompose the vulgar fraction ½
    // (GNU -> "1⁄2", Neomacs -> "½"). Other compatibility decompositions
    // (ﬁ -> fi, ﬃ -> ffi, ℃ -> °C, ㈱ -> (株)) agree.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (ucs-normalize-NFKC-string "ﬁ")
      (ucs-normalize-NFKC-string "ﬃ")
      (ucs-normalize-NFKD-string "½")
      (ucs-normalize-NFKC-string "℃")
      (ucs-normalize-NFKC-string "㈱"))
"##,
        &["international/ucs-normalize.el"],
        expect,
    );
}

#[test]
fn div_h7_ucs_normalize_canonical() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ﾊ\u{ff9e}\" \"カ\u{3099}\" 2)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK ("ﾊﾞ" "ガ" 2)
    // Neomacs:   OK ("ﾊﾞ" "ガ" 1)
    // ucs-normalize-HFS-NFD-string does not decompose ガ (2 chars
    // カ+voiced-mark in GNU, 1 in Neomacs).
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (ucs-normalize-HFS-NFC-string "ﾊﾞ")
      (ucs-normalize-HFS-NFD-string "ガ")
      (length (ucs-normalize-HFS-NFD-string "ガ")))
"##,
        &["international/ucs-normalize.el"],
        expect,
    );
}

#[test]
fn div_h7_cl_extra_floor_ceiling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((1 1.7000000000000002) (2 -0.7999999999999998) (2 0.5) (4 -0.5) (2 1) (2 1))""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (cl-floor 3.7 2)
      (cl-ceiling 3.2 2)
      (cl-round 2.5)
      (cl-round 3.5)
      (cl-truncate 7 3)
      (cl-floor 7 3))
"##,
        &["emacs-lisp/cl-extra.el"],
        expect,
    );
}

#[test]
fn div_h7_ucs_normalize_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 \"é\" t \"À\")""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (2 "é" t "À")
    // Neomacs:   OK (1 "é" t "À")
    // length(NFD("é")) is 1 in Neomacs vs 2 in GNU — same root cause as
    // div_h7_ucs_normalize_nfc_nfd: decomposition is a no-op. Recomposition
    // (NFC of the decomposed form) and NFC("À") agree.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(let* ((decomp (ucs-normalize-NFD-string "é"))
       (recomp (ucs-normalize-NFC-string decomp)))
  (list (length decomp)
        recomp
        (= (length recomp) 1)
        (ucs-normalize-NFC-string "À")))
"##,
        &["international/ucs-normalize.el"],
        expect,
    );
}

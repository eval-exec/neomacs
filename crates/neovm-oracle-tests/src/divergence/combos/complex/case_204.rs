//! Complex combo batch 204 — `ucs-normalize` (NFC/NFD/NFKC/NFKD) across
//! Hangul, Latin, Greek with decomposition composition round trips.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx204_ucs_normalize_nfc_nfd_roundtrip_latin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((nfc "café naïve résumé")
       (nfd (ucs-normalize-string nfc 'nfd))
       (nfc-back (ucs-normalize-string nfd 'nfc)))
  (list (string= nfc nfc-back)
        (string= nfc nfd)
        (length nfc) (length nfd)
        (string-bytes nfc) (string-bytes nfd)))
"##,
        expect,
    );
}

#[test]
fn div_cx204_ucs_normalize_nfkc_nfkd_compatibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((orig "ﬁℌ℃")  ; ligature fi, script H, degrees Celsius
       (nfkc (ucs-normalize-string orig 'nfkc))
       (nfkd (ucs-normalize-string orig 'nfkd)))
  (list orig nfkc nfkd
        (length orig) (length nfkc) (length nfkd)
        (string= nfkc nfkd)))
"##,
        expect,
    );
}

#[test]
fn div_cx204_ucs_normalize_hangul_composition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((composed "한글")
       (decomposed (ucs-normalize-string composed 'nfd))
       (recomposed (ucs-normalize-string decomposed 'nfc)))
  (list (string= composed recomposed)
        (string= composed decomposed)
        (length composed) (length decomposed)))
"##,
        expect,
    );
}

#[test]
fn div_cx204_ucs_normalize_region_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "café naïve")
      (ucs-normalize-region 1 (point-max) 'nfd)
      (let ((nfd-len (buffer-size)))
        (ucs-normalize-region 1 (point-max) 'nfc)
        (list nfd-len (buffer-size) (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx204_ucs_normalize_greek_diacritics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((nfc "άέήίόύώ")
       (nfd (ucs-normalize-string nfc 'nfd))
       (nfc-back (ucs-normalize-string nfd 'nfc)))
  (list (string= nfc nfc-back)
        (string= nfc nfd)
        (length nfc) (length nfd)))
"##,
        expect,
    );
}

#[test]
fn div_cx204_ucs_normalize_idempotent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café 한글 ﬁℌ"))
  (list (string= s (ucs-normalize-string (ucs-normalize-string s 'nfc) 'nfc))
        (string= s (ucs-normalize-string (ucs-normalize-string s 'nfd) 'nfd))))
"##,
        expect,
    );
}

#[test]
fn div_cx204_ucs_normalize_already_normalized() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ascii "Hello World 123"))
  (list (string= ascii (ucs-normalize-string ascii 'nfc))
        (string= ascii (ucs-normalize-string ascii 'nfd))
        (string= ascii (ucs-normalize-string ascii 'nfkc))
        (string= ascii (ucs-normalize-string ascii 'nfkd))))
"##,
        expect,
    );
}

#[test]
fn div_cx204_ucs_normalize_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (ucs-normalize-string "" 'nfc)
      (ucs-normalize-string "" 'nfd)
      (ucs-normalize-string "" 'nfkc)
      (ucs-normalize-string "" 'nfkd)
      (length (ucs-normalize-string "" 'nfc)))
"##,
        expect,
    );
}

#[test]
fn div_cx204_ucs_normalize_combined_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((precomposed "é")
       (decomposed (string ?e (decode-char 'unicode #x0301)))
       (nfc-pre (ucs-normalize-string precomposed 'nfc))
       (nfc-decomp (ucs-normalize-string decomposed 'nfc)))
  (list (string= nfc-pre nfc-decomp)
        (length nfc-pre) (length nfc-decomp)
        (length precomposed) (length decomposed)))
"##,
        expect,
    );
}

#[test]
fn div_cx204_ucs_normalize_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function ucs-normalize-string)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((nfc "café 한글 naïve")
       (nfd (ucs-normalize-string nfc 'nfd))
       (hash-nfd (secure-hash 'sha256 nfd)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert nfd)
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (string= nfc (ucs-normalize-string nfd 'nfc))
                         hash-nfd
                         (length nfc) (length nfd)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}

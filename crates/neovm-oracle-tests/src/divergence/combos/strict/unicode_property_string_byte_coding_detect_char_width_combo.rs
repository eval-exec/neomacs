//! Strict combo oracle probes, batch 126: Unicode property access
//! (general-category, canonical-combining-class, bidi-class, script),
//! string byte-level operations, coding detection, and combining char width.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u0_unicode_property_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (get-char-code-property ?a 'general-category)
      (get-char-code-property ?1 'general-category)
      (get-char-code-property ?  'general-category)
      (get-char-code-property ?\n 'general-category)
      (get-char-code-property 769 'canonical-combining-class)
      (get-char-code-property 768 'canonical-combining-class)
      (get-char-code-property ?a 'bidi-class)
      (get-char-code-property ?א 'bidi-class)
      (get-char-code-property ?a 'script)
      (get-char-code-property ?日 'script)
      (get-char-code-property ?α 'script))
"##;
    let expect = expect_test::expect![[r#""OK (Ll Nd Zs Cc 230 230 L R nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u0_string_byte_level_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((uni "café")
      (raw (string-make-unibyte "café"))
      (mixed (string 65 233 66)))
  (list (length uni)
        (string-bytes uni)
        (length raw)
        (string-bytes raw)
        (multibyte-string-p uni)
        (multibyte-string-p raw)
        (length mixed)
        (string-bytes mixed)
        (length (string-as-unibyte uni))
        (length (string-as-multibyte raw))
      (string= (string-as-unibyte uni) raw)
      (aref uni 3)
      (aref raw 3)
      (string-make-multibyte "\200")
      (length (string-make-multibyte "\200"))))
"##;
    let expect = expect_test::expect![[r#""OK (4 5 4 4 t nil 3 4 5 4 nil 233 233 \"\\200\" 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u0_detect_coding_region_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((utf8 (encode-coding-string "café" 'utf-8))
      (sjis (encode-coding-string "日本" 'shift_jis)))
  (with-temp-buffer
    (insert utf8)
    (let ((d1 (detect-coding-region (point-min) (point-max))))
      (erase-buffer)
      (insert sjis)
      (let ((d2 (detect-coding-region (point-min) (point-max))))
        (erase-buffer)
        (insert "plain ASCII")
        (let ((d3 (detect-coding-region (point-min) (point-max))))
        (list (car d1)
              (car d2))))))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u0_combining_char_width_and_string_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (char-width 768)
      (char-width 769)
      (char-width 771)
      (char-width 840)
      (string-width "é")
      (string-width "ñ")
      (string-width "ǟ")
      (string-width (string 97 768 98))
      (length "é")
      (string-bytes (encode-coding-string "é" 'utf-8)))
"##;
    let expect = expect_test::expect![[r#""OK (0 0 0 0 1 1 1 2 2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u0_arithmetic_bignum_overflow_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (* most-positive-fixnum 2)
      (* most-positive-fixnum most-positive-fixnum)
      (+ most-positive-fixnum most-positive-fixnum most-positive-fixnum)
      (expt most-positive-fixnum 2)
      (expt 2 128)
      (expt 2 256)
      (expt 2 512)
      (* (expt 10 50) (expt 10 50))
      (mod (expt 2 100) (expt 2 50))
      (gcd (expt 2 40) (expt 2 60))
      (/ (expt 2 100) (expt 2 50))
      (ash (expt 2 64) -32)
      (ash 1 -1)
      (logand (1- (expt 2 32)) (expt 2 16))
      (logior (expt 2 32) (expt 2 33)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function gcd)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

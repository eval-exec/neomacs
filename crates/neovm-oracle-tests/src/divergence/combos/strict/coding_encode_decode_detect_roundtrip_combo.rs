//! Strict combo oracle probes, batch 165: coding systems. encode/decode
//! round-trips for utf-8/latin-1/utf-16/ascii, encode of CJK under utf-8 and
//! east-asian 8-bit codings, coding-system-p / check-coding-systems /
//! coding-system-charset-list predicates, and detect-coding-region over a
//! byte-encoded buffer.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_coding_encode_decode_roundtrip_common() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (decode-coding-string (encode-coding-string "héllo wörld" 'utf-8) 'utf-8)
      (decode-coding-string (encode-coding-string "café" 'latin-1) 'latin-1)
      (decode-coding-string (encode-coding-string "ABCabc" 'utf-16) 'utf-16)
      (decode-coding-string (encode-coding-string "plain ascii" 'us-ascii) 'us-ascii)
      (encode-coding-string "日本語" 'utf-8)
      (length (encode-coding-string "日本語" 'utf-8))
      (string-to-multibyte "abc")
      (string-make-unibyte "abc")
      (encode-coding-string "a" 'utf-8))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"héllo wörld\" #(\"café\" 0 4 (charset iso-8859-1)) \"ABCabc\" \"plain ascii\" \"日本語\" 9 \"abc\" \"abc\" \"a\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_coding_system_predicates_check_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (coding-system-p 'utf-8)
      (coding-system-p 'latin-1)
      (coding-system-p 'made-up-coding)
      (coding-system-p 42)
      (check-coding-systems "abc" 'utf-8 nil)
      (coding-system-type 'utf-8)
      (coding-system-base 'utf-8-with-signature)
      (eq (coding-system-base 'utf-8-with-signature) 'utf-8)
      (coding-system-charset-list 'us-ascii)
      (car (coding-system-priority-list))
      (memq 'utf-8 (coding-system-priority-list)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function check-coding-systems)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_coding_east_asian_detect_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((enc (encode-coding-string "日本" 'utf-8)))
  (list (length enc)
        (eq (encode-coding-string "a" 'shift_jis) (encode-coding-string "a" 'shift_jis))
        (decode-coding-string enc 'utf-8)
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert enc)
          (let ((detected (detect-coding-region (point-min) (point-max))))
            (car detected)))
        (condition-case err
            (decode-coding-string (string 0x82 0xa0) 'shift_jis)
          (coding-system-error 'caught))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable 0x82)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Strict combo oracle probes, batch 348: format-encode/decode enriched text.
//! format-encode-string / format-decode-string with text/enriched, and
//! enriched text face property round-trip.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_format_encode_decode_enriched_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'enriched)
(require 'format)
(let* ((text "Hello enriched world\nSecond line\n")
       (encoded (format-encode-string text '(text/enriched))))
  (list (stringp encoded)
        (> (length encoded) 0)
        (let ((decoded (format-decode-string encoded (length encoded) '(text/enriched))))
          (or (string-match "Hello" decoded) decoded))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function format-encode-string)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_enriched_face_property_encode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'enriched)
(require 'format)
(with-temp-buffer
  (insert "bold text here")
  (add-text-properties 1 5 '(face bold))
  (let ((encoded (format-encode-string (buffer-string) '(text/enriched))))
    (list (stringp encoded)
          (string-match "bold" encoded))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function format-encode-string)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_decode_detect_enriched() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'enriched)
(require 'format)
(let* ((sample "Content-Type: text/enriched\nText-Width: 70\n\nThis is <bold>bold</bold> text.\n")
       (decoded (format-decode-string sample (length sample))))
  (list (stringp decoded)
        (or (string-match "bold" decoded) decoded)
        (>= (length decoded) 0)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function format-decode-string)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

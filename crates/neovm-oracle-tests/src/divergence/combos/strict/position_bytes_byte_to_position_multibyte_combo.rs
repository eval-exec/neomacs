//! Strict combo oracle probes, batch 186: byte <-> position mapping over
//! multibyte text. position-bytes (char->byte) and byte-to-position (byte->
//! char) across ASCII + CJK boundaries, which is a historically divergence-
//! prone surface (per the UTF-8 divergence notes).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_position_bytes_byte_to_position_ascii_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "abc日本語def")
  (list (position-bytes 1)
        (position-bytes 4)
        (position-bytes 5)
        (position-bytes 6)
        (position-bytes 7)
        (position-bytes 8)
        (byte-to-position 1)
        (byte-to-position 4)
        (byte-to-position 7)
        (byte-to-position 10)
        (byte-to-position 13)
        (char-after 1)
        (char-after 4)
        (char-after 7)))
"##;
    let expect = expect_test::expect![[r#""OK (1 4 7 10 13 14 1 4 5 6 7 97 26085 100)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_position_bytes_combining_marks_emoji() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "xÄy您好z")
  (list (length (buffer-string))
        (position-bytes 1)
        (position-bytes 2)
        (position-bytes 3)
        (position-bytes 4)
        (position-bytes 5)
        (position-bytes 6)
        (position-bytes 7)
        (byte-to-position 1)
        (byte-to-position 3)
        (byte-to-position 4)
        (byte-to-position 7)
        (byte-to-position 10)))
"##;
    let expect = expect_test::expect![[r#""OK (6 1 2 4 5 8 11 12 1 2 3 4 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_position_bytes_empty_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "")
  (list (position-bytes 1)
        (byte-to-position 1)))
"##;
    let expect = expect_test::expect![[r#""OK (1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

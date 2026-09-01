//! UTF-8 / multibyte *buffer multibyte toggling* divergence probes.
//!
//! Characterizes the data-corruption bug found in `set-buffer-multibyte t`
//! (raw-byte promotion): toggling a unibyte buffer holding raw bytes back to
//! multibyte can drop trailing ASCII bytes. These probes vary the byte pattern
//! to pin exactly when corruption occurs, for a tight teammate reproduction.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_utf8_toggle_unibyte_to_multibyte_trailing_ascii_dropped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 (4194248 4194249 65) t)""#]];
    // Original corruption case: bytes (200 201 65) -> NEO drops trailing 65.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65))
  (set-buffer-multibyte t)
  (list (length (buffer-string)) (append (buffer-string) nil)
        (multibyte-string-p (buffer-string))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_toggle_unibyte_to_multibyte_raw_bytes_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 (4194248 4194249 4194303))""#]];
    // No trailing ASCII — does corruption still occur?
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 255))
  (set-buffer-multibyte t)
  (list (length (buffer-string)) (append (buffer-string) nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_toggle_unibyte_to_multibyte_leading_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 (65 66 4194248 4194249))""#]];
    // ASCII first, then raw bytes.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 65 66 200 201))
  (set-buffer-multibyte t)
  (list (length (buffer-string)) (append (buffer-string) nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_toggle_unibyte_to_multibyte_interleaved() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 (65 4194248 66 4194249 67 4194303))""#]];
    // ASCII and raw bytes interleaved.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 65 200 66 201 67 255))
  (set-buffer-multibyte t)
  (list (length (buffer-string)) (append (buffer-string) nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_toggle_unibyte_to_multibyte_single_raw_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 (4194248) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200))
  (set-buffer-multibyte t)
  (list (length (buffer-string)) (append (buffer-string) nil)
        (multibyte-string-p (buffer-string))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_toggle_double_roundtrip_multibyte_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café世界\" t 6 (99 97 102 233 19990 30028))""#]];
    // multibyte -> unibyte -> multibyte round trip stability.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert "café世界")
  (let ((original (buffer-string)))
    (set-buffer-multibyte nil)
    (set-buffer-multibyte t)
    (list (buffer-string) (equal original (buffer-string))
          (length (buffer-string)) (append (buffer-string) nil))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_toggle_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Changing multibyteness in a narrowed buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 65 200 201 66))
  (narrow-to-region 2 4)
  (set-buffer-multibyte t)
  (list (point-min) (point-max) (append (buffer-string) nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_toggle_unibyte_to_multibyte_preserves_point_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 4 (4194248 4194249 65 66))""#]];
    // point-max before vs after the toggle.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65 66))
  (let ((before (point-max)))
    (set-buffer-multibyte t)
    (list before (point-max) (length (buffer-string)) (append (buffer-string) nil))))
"#,
        expect,
    );
}

//! UTF-8 / multibyte *buffer charset properties & eight-bit buffer positions*.
//!
//! Extends two themes:
//!  (a) The charset text-property divergence (#1) into more file-read paths
//!      (utf-8 vs latin-1, multibyte vs unibyte buffer) to characterize when
//!      Neomacs attaches `charset` properties that GNU does not.
//!  (b) The eight-bit byte-width inconsistency into the BUFFER layer:
//!      `position-bytes` of a buffer holding decode-recovered eight-bit chars.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- charset text properties on file read -----------------------------------

#[test]
fn div_utf8_fileread_utf8_no_charset_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil \"café世界\" 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((tmp (make-temp-file "u8p-")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'utf-8-unix))
          (write-region "café世界" nil tmp nil 'silent))
        (with-temp-buffer
          (let ((coding-system-for-read 'utf-8-unix))
            (insert-file-contents tmp))
          (list (text-properties-at 0 (buffer-string))
                (text-properties-at 1 (buffer-string))
                (buffer-string) (point-max))))
    (delete-file tmp)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_fileread_latin1_multibyte_buffer_charset_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((charset iso-8859-1) (charset iso-8859-1) #(\"café\" 0 4 (charset iso-8859-1)) 5)""#
    ]];
    // Reading latin-1 into a *multibyte* (default) buffer — does Neomacs
    // attach charset properties that GNU does not?
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((tmp (make-temp-file "l1p-")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'latin-1-unix))
          (write-region "café" nil tmp nil 'silent))
        (with-temp-buffer
          (let ((coding-system-for-read 'latin-1-unix))
            (insert-file-contents tmp))
          (list (text-properties-at 0 (buffer-string))
                (text-properties-at 3 (buffer-string))
                (buffer-string) (point-max))))
    (delete-file tmp)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_fileread_latin1_unibyte_buffer_charset_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil \"caf�\" 5 (99 97 102 233))""#]];
    // The original #1 case: latin-1 into a unibyte buffer.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((tmp (make-temp-file "l1u-")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'latin-1-unix))
          (write-region "café" nil tmp nil 'silent))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (let ((coding-system-for-read 'latin-1-unix))
            (insert-file-contents tmp))
          (list (text-properties-at 0 (buffer-string))
                (buffer-string)
                (point-max)
                (append (buffer-string) nil))))
    (delete-file tmp)))
"#,
        expect,
    );
}

// --- eight-bit chars in buffer byte positions -------------------------------

#[test]
fn div_utf8_position_bytes_buffer_recovered_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 6 3 5 2 4194248 4194249)""#]];
    // Decode-recovered eight-bit chars (3 bytes in Neomacs) in a buffer.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert (decode-coding-string (unibyte-string 200 201 65) 'utf-8))
  (list (point-max)
        (position-bytes (point-max))
        (position-bytes 2)
        (position-bytes 3)
        (byte-to-position 4)
        (char-after 1) (char-after 2)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_position_bytes_buffer_constructed_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 6 3 5)""#]];
    // CONTROL: constructed eight-bit chars (2 bytes in both) should agree.
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert (string-make-multibyte (unibyte-string 200 201 65)))
  (list (point-max)
        (position-bytes (point-max))
        (position-bytes 2)
        (position-bytes 3)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_buffer_narrow_recovered_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 5 \"B\\310\\311\" 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(with-temp-buffer
  (insert (concat "AB" (decode-coding-string (unibyte-string 200 201) 'utf-8) "CD"))
  (narrow-to-region 2 5)
  (list (point-min) (point-max) (buffer-string)
        (position-bytes (point-max))))
"#,
        expect,
    );
}

// --- raw-byte file round trip (no-conversion) -------------------------------

#[test]
fn div_utf8_raw_byte_file_no_conversion_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (200 201 255)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((tmp (make-temp-file "8bit-"))
      (s (decode-coding-string (unibyte-string 200 201 255) 'utf-8)))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'no-conversion))
          (write-region s nil tmp nil 'silent))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents tmp)
          (append (buffer-string) nil)))
    (delete-file tmp)))
"#,
        expect,
    );
}

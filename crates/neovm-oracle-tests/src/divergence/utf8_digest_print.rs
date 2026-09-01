//! UTF-8 / multibyte *byte-representation-sensitive* divergence probes.
//!
//! Operations whose result depends on the internal byte layout of a string:
//! `md5`, `secure-hash`, `base64-encode-string`, `prin1-to-string`/`read`
//! round-trips, and `%S` printing.  These expose the eight-bit byte-width
//! divergence (and any print/read asymmetry) through new surfaces.
//!
//! Also pins a Neomacs internal inconsistency: `decode-coding-string` recovery
//! vs `string-make-multibyte` produce eight-bit chars with different internal
//! byte widths.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- hashes (byte-representation sensitive) ---------------------------------

#[test]
fn div_utf8_md5_ascii_and_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"900150983cd24fb0d6963f7d28e17f72\" \"07117fe4a1ebd544965dc19573183da2\" \"c086b3008aca0efa8f2ded065d6afb50\" \"be50e8478cf24ff3595bc7307fb91b50\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (md5 "abc")
      (md5 "café")
      (md5 "世界")
      (md5 "héllo"))
"#,
        expect,
    );
}

#[test]
fn div_utf8_md5_of_recovered_eightbit_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1fab6b63ef2756a9df4f07be5bd5a122\" \"1fab6b63ef2756a9df4f07be5bd5a122\")""#
    ]];
    // md5 hashes the internal bytes; eight-bit byte width (2 vs 3) diverges.
    crate::common::assert_oracle_parity_expect(
        r#"
(list (md5 (decode-coding-string (unibyte-string 200 201 255) 'utf-8))
      (md5 (string-make-multibyte (unibyte-string 200 201 255))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_secure_hash_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"850f7dc43910ff890f8879c0ed26fe697c93a067ad93a7d50f466a7028a9bf4e\" \"cf1656101ed511a094d1e4e515bbf8d32b266090\" \"be50e8478cf24ff3595bc7307fb91b50\" \"fc66507e73bf39f458cab64f2054b8007defe1c13fdaee21481cd6546b9eb056fa295ee7485c70f6f78f0cbbe797b1dda1483f922ed9687cda97ca1aca886d5f\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (secure-hash 'sha256 "café")
      (secure-hash 'sha1 "世界")
      (secure-hash 'md5 "héllo")
      (secure-hash 'sha512 "a😀b"))
"#,
        expect,
    );
}

// --- base64 (byte-representation sensitive) ---------------------------------

#[test]
fn div_utf8_base64_multibyte_and_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Multibyte character in data for base64 encoding\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (base64-encode-string "abc")
      (base64-encode-string "café")
      (base64-encode-string "世界")
      (base64-encode-string (decode-coding-string (unibyte-string 200 255) 'utf-8))
      (base64-encode-string (string-make-multibyte (unibyte-string 200 255))))
"#,
        expect,
    );
}

// --- prin1 / read round-trip ------------------------------------------------

#[test]
fn div_utf8_prin1_multibyte_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\\"café世界\\\"\" t 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((s "café世界")
       (p (prin1-to-string s))
       (back (car (read-from-string p))))
  (list p (equal s back) (length p)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_prin1_eightbit_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\\\"\\\\310\\\\311\\\\377\\\"\" nil (200 201 255))""#]];
    // How eight-bit chars are printed (octal \NNN vs \xNN) and whether they
    // round-trip through read.
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((raw (decode-coding-string (unibyte-string 200 201 255) 'utf-8))
       (p (prin1-to-string raw))
       (back (car (read-from-string p))))
  (list p (equal raw back) (append back nil)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_format_S_multibyte_and_eightbit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\"café\\\"\" \"\\\"世界\\\"\" \"\\\"\\\\310\\\"\" \"\\\"\\\\310\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (format "%S" "café")
      (format "%S" "世界")
      (format "%S" (decode-coding-string (unibyte-string 200) 'utf-8))
      (format "%S" (string-make-multibyte (unibyte-string 200))))
"#,
        expect,
    );
}

// --- the pinned inconsistency -----------------------------------------------

#[test]
fn div_utf8_pinned_decode_vs_make_eightbit_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 t (4194248) (4194248))""#]];
    // GNU: both construction paths yield identical 2-byte eight-bit chars.
    // Neomacs: decode-coding-string recovery yields 3-byte storage while
    // string-make-multibyte yields 2-byte storage -> internal inconsistency.
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((d (decode-coding-string (unibyte-string 200) 'utf-8))
      (m (string (unibyte-char-to-multibyte 200))))
  (list (string-bytes d) (string-bytes m)
        (equal d m)
        (append d nil) (append m nil)))
"#,
        expect,
    );
}

// --- aref on unibyte vs multibyte raw bytes ---------------------------------

#[test]
fn div_utf8_aref_unibyte_vs_multibyte_indexing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (200 201 4194248 4194249)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((u (unibyte-string 200 201))
      (m (string-make-multibyte (unibyte-string 200 201))))
  (list (aref u 0) (aref u 1)
        (aref m 0) (aref m 1)))
"#,
        expect,
    );
}

//! Strict combo oracle probes, batch 37: MIME/mail encoding loaded libraries
//! via assert_oracle_parity_with_load — qp.el (quoted-printable encode/decode
//! of multibyte, the same =XX machinery as rfc2047), mail-extr.el (address
//! component extraction), and ietf-drums.el (RFC822 address/header parsing).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_h4_quoted_printable_ascii_encode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"plain\" \"Hello, World!\" 5)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (quoted-printable-encode-string "plain")
      (quoted-printable-encode-string "Hello, World!")
      (length (quoted-printable-encode-string "plain")))
"##,
        &["mail/qp.el"],
        expect,
    );
}

#[test]
fn div_h4_quoted_printable_multibyte_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Multibyte character in QP encoding region\")""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: ERR (error "Multibyte character in QP encoding region")
    // Neomacs:   ERR (error "Not an ASCII nor an 8-bit character: 233")
    // quoted-printable-encode-string of a multibyte string errors in both
    // engines but with divergent messages (Neomacs' underlying char-class
    // check produces a different diagnostic). ASCII encoding/decoding agree.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(quoted-printable-encode-string "café")
"##,
        &["mail/qp.el"],
        expect,
    );
}

#[test]
fn div_h4_quoted_printable_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"café 日本\" \"no encoding here\" \"softbreak\")""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (quoted-printable-decode-string "caf=C3=A9 =E6=97=A5=E6=9C=AC")
      (quoted-printable-decode-string "no encoding here")
      (quoted-printable-decode-string "soft=\nbreak"))
"##,
        &["mail/qp.el"],
        expect,
    );
}

#[test]
fn div_h4_mail_extract_address_components() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"John Doe\" \"john@example.com\") (nil \"john@example.com\") (nil \"john@example.com\"))""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (mail-extract-address-components "John Doe <john@example.com>")
      (mail-extract-address-components "john@example.com")
      (mail-extract-address-components "John <john@example.com> (the boss)"))
"##,
        &["mail/mail-extr.el"],
        expect,
    );
}

#[test]
fn div_h4_ietf_drums_parse_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"john@example.com\" . \"John\") (\"john@example.com\"))""#
    ]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (ietf-drums-parse-address "John <john@example.com>")
      (ietf-drums-parse-address "john@example.com"))
"##,
        &["mail/ietf-drums.el"],
        expect,
    );
}

#[test]
fn div_h4_ietf_drums_group_address_comma() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"group:a@b.com,c@d.com;\")""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK ("group:a@b.com,c@d.com;")
    // Neomacs:   OK ("group:a@b.comc@d.com;")
    // ietf-drums-parse-address of a group address list DROPS the comma
    // separating addresses in Neomacs (functional parsing bug). Simple
    // addresses parse correctly.
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(ietf-drums-parse-address "group: a@b.com, c@d.com;")
"##,
        &["mail/ietf-drums.el"],
        expect,
    );
}

//! Divergence tests: auth-source, gnus, message, mail deep stubs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_auth_source() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'auth-source-search)
  (fboundp 'auth-source-forget)
  (featurep 'auth-source))"#,
        expect,
    );
}

#[test]
fn divergence_gnus_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'gnus)
  (fboundp 'gnus-group-list)
  (featurep 'gnus)
  (featurep 'gnus-group))"#,
        expect,
    );
}

#[test]
fn divergence_message_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'message-mail)
  (fboundp 'message-reply)
  (featurep 'message))"#,
        expect,
    );
}

#[test]
fn divergence_sendmail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'sendmail-send-it)
  (boundp 'send-mail-function)
  (featurep 'sendmail))"#,
        expect,
    );
}

#[test]
fn divergence_smtpmail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'smtpmail-send-it)
  (featurep 'smtpmail))"#,
        expect,
    );
}

#[test]
fn divergence_epa_gpg() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'epa-encrypt-file)
  (fboundp 'epa-decrypt-file)
  (fboundp 'epa-sign-file)
  (featurep 'epa)
  (featurep 'epg))"#,
        expect,
    );
}

#[test]
fn divergence_erc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'erc)
  (fboundp 'erc-select)
  (featurep 'erc))"#,
        expect,
    );
}

#[test]
fn divergence_rcirc() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'rcirc)
  (featurep 'rcirc))"#,
        expect,
    );
}

#[test]
fn divergence_eww_shr() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'eww)
  (fboundp 'eww-open-file)
  (featurep 'eww)
  (featurep 'shr))"#,
        expect,
    );
}

#[test]
fn divergence_url_library() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'url-retrieve)
  (fboundp 'url-retrieve-synchronously)
  (featurep 'url))"#,
        expect,
    );
}

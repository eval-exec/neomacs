/// Batch 471: auth-source deep, password-cache, plstore, gnus, nnheader, imap.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx471_auth_source_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'auth-source)
  (list (fboundp 'auth-source-search)
        (boundp 'auth-sources)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_password_cache_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'password-cache)
  (password-cache-add "test" "secret")
  (stringp (password-cache-remove "test")))
"##,
        expect,
    );
}

#[test]
fn div_cx471_plstore_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'plstore)
  (list (fboundp 'plstore-open) (boundp 'plstore-encoded)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_gnus_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'gnus)
  (list (boundp 'gnus-version) (fboundp 'gnus)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_nnheader_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'nnheader)
  (list (boundp 'nnheader-version) (fboundp 'nnheader-find-etc-directory)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_imap_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'imap)
  (list (fboundp 'imap-open) (boundp 'imap-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_pop3_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'pop3)
  (list (fboundp 'pop3-open) (boundp 'pop3-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_sieve_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'sieve)
  (list (fboundp 'sieve-edit-script) (fboundp 'sieve-upload)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_ldap_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ldap)
  (list (fboundp 'ldap-search) (boundp 'ldap-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_nndiary_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'nndiary)
  (list (fboundp 'nndiary-request-accept-article)
        (fboundp 'nndiary-request-post)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_nnfolder_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'nnfolder)
  (list (fboundp 'nnfolder-generate-active-file)
        (fboundp 'nnfolder-request-create-group)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_nnimap_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'nnimap)
  (list (fboundp 'nnimap-open-server)
        (boundp 'nnimap-version)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_nnmail_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'nnmail)
  (list (fboundp 'nnmail-split-fancy)
        (fboundp 'nnmail-expired-article-p)))
"##,
        expect,
    );
}

#[test]
fn div_cx471_message_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (message-mode message-mode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'message)
  (with-temp-buffer
    (message-mode)
    (list major-mode (derived-mode-p 'message-mode))))
"##,
        expect,
    );
}

#[test]
fn div_cx471_sendmail_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'sendmail)
  (list (boundp 'sendmail-program) (fboundp 'sendmail-send-it)))
"##,
        expect,
    );
}

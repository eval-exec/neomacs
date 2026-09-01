//! Divergence tests: network, socket, TLS, URL deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_network_options() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'network-interface-list)
  (fboundp 'network-interface-info)
  (fboundp 'format-network-address)) "#,
        expect,
    );
}

#[test]
fn divergence_socket_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'open-network-stream)
  (fboundp 'gnutls-available-p)
  (fboundp 'open-gnutls-stream)
  (featurep 'gnutls)) "#,
        expect,
    );
}

#[test]
fn divergence_tls_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable gnutls-trustfiles)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'gnutls-trustfiles)
  (listp gnutls-trustfiles)
  (boundp 'gnutls-verify-error)
  (boundp 'gnutls-min-prime-bits)
  (integerp gnutls-min-prime-bits)) "#,
        expect,
    );
}

#[test]
fn divergence_url_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'url-configuration-directory)
  (boundp 'url-cookie-file)
  (boundp 'url-history-file)
  (fboundp 'url-insert-file-contents)) "#,
        expect,
    );
}

#[test]
fn divergence_dns_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 5 26)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'dns-query)
  (fboundp 'dns-lookup-host)
  (fboundp 'network-lookup-address-info)
  (fboundp 'lookup-host)))) "#,
        expect,
    );
}

#[test]
fn divergence_network_lookup_numeric_hint() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (([127 0 0 1 0]) nil (error error \"Unsupported hints value\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (network-lookup-address-info "127.0.0.1" 'ipv4 'numeric)
  (network-lookup-address-info "localhost" 'ipv4 'numeric)
  (condition-case err
      (network-lookup-address-info "127.0.0.1" 'ipv4 'canonical)
    (error (list 'error (car err) (cadr err))))) "#,
        expect,
    );
}

#[test]
fn divergence_http_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'url-http-file-exists-p)
  (fboundp 'url-file-exists-p)
  (fboundp 'url-file-directory-p)
  (fboundp 'url-expand-file-name)) "#,
        expect,
    );
}

#[test]
fn divergence_ldap_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'ldap-open)
  (fboundp 'ldap-close)
  (fboundp 'ldap-search)
  (featurep 'ldap)) "#,
        expect,
    );
}

#[test]
fn divergence_mime_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'mime-edit)
  (fboundp 'mailcap-parse-mailcaps)
  (fboundp 'mailcap-mime-info)
  (featurep 'mailcap)) "#,
        expect,
    );
}

#[test]
fn divergence_mail_utils() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'mail-strip-quoted-names)
  (fboundp 'rfc822-addresses)
  (fboundp 'mail-header-parse-address)
  (featurep 'mail-utils)) "#,
        expect,
    );
}

#[test]
fn divergence_news_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'gnus)
  (fboundp 'gnus-group-read-news)
  (fboundp 'gnus-msg-mail)
  (featurep 'gnus)) "#,
        expect,
    );
}

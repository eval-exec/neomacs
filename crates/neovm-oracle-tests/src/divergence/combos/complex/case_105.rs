//! Complex combo batch 105 — mail / smtp / message / gnus / auth-source /
//! network-security availability and metadata.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx105_message_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'message)
      (list (fboundp 'message-mode)
            (fboundp 'message-mail)
            (boundp 'message-send-mail-function)
            (boundp 'message-from-style)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_sendmail_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'sendmail)
      (list (fboundp 'mail)
            (fboundp 'sendmail-send-it)
            (boundp 'send-mail-function)
            (boundp 'mail-header-separator)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_smtpmail_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'smtpmail)
      (list (fboundp 'smtpmail-send-it)
            (boundp 'smtpmail-smtp-server)
            (boundp 'smtpmail-smtp-service)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_gnus_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'gnus)
      (list (fboundp 'gnus)
            (boundp 'gnus-home-directory)
            (boundp 'gnus-startup-file)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_auth_source_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'auth-source)
      (list (fboundp 'auth-source-search)
            (fboundp 'auth-source-forget)
            (boundp 'auth-sources)
            (boundp 'auth-source-do-cache)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_epa_epg_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'epa)
      (require 'epg)
      (list (fboundp 'epa-encrypt-file)
            (fboundp 'epa-decrypt-file)
            (fboundp 'epg-make-context)
            (boundp 'epg-gpg-program)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_network_security_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'nsm)
      (list (fboundp 'network-connection-status)
            (boundp 'network-security-level)
            (boundp 'nsm-trustable-pem-file)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_gnutls_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'gnutls-available-p)
          (fboundp 'open-gnutls-stream)
          (boundp 'gnutls-min-prime-bits)
          (boundp 'gnutls-trustfiles))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_url_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'url)
      (list (fboundp 'url-retrieve)
            (fboundp 'url-retrieve-synchronously)
            (boundp 'url-user-agent)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_json_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'json)
      (list (fboundp 'json-encode)
            (fboundp 'json-read)
            (fboundp 'json-read-from-string)
            (fboundp 'json-encode-key)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_json_roundtrip_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"{\\\"name\\\":\\\"alpha\\\",\\\"value\\\":42,\\\"tags\\\":[\\\"x\\\",\\\"y\\\",\\\"z\\\"],\\\"nested\\\":{\\\"deep\\\":\\\"val\\\"}}\" 72 \"alpha\" 42 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'json)
      (let* ((data '((name . "alpha")
                     (value . 42)
                     (tags . ("x" "y" "z"))
                     (nested . ((deep . :val)))))
             (encoded (json-encode data))
             (decoded (json-read-from-string encoded)))
        (list encoded
              (length encoded)
              (cdr (assq 'name decoded))
              (cdr (assq 'value decoded))
              (length (cdr (assq 'tags decoded))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_xml_dom_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'xml)
      (require 'dom)
      (list (fboundp 'xml-parse-region)
            (fboundp 'xml-parse-string)
            (fboundp 'dom-node)
            (fboundp 'dom-by-tag)
            (fboundp 'dom-attr)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_xml_parse_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((xml "<root><child attr=\"value\">text</child><empty/></root>")
           (parsed (with-temp-buffer
                     (insert xml)
                     (xml-parse-region (point-min) (point-max)))))
      (list (car (car parsed))
            (dom-by-tag (car parsed) 'child)
            (dom-attr (car (dom-by-tag (car parsed) 'child)) 'attr)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx105_message_encode_decode_headers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"plain\" \"=?utf-8?B?Y2Fmw6kg5LiW55WM?=\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'rfc2047)
      (list (rfc2047-encode-string "plain")
            (condition-case err (rfc2047-encode-string "café 世界") (error :err))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

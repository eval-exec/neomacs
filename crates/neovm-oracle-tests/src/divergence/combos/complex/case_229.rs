//! Complex combo batch 229 — `server` / `server-start` / `emacsclient` /
//! `mailcap` / `mime-types` / `mm-decode` / `mm-encode` availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx229_server_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'server)
      (list (fboundp 'server-start)
            (fboundp 'server-running-p)
            (boundp 'server-name)
            (boundp 'server-socket-dir)
            (boundp 'server-window)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx229_mailcap_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'mailcap)
      (list (fboundp 'mailcap-parse-mailcaps)
            (fboundp 'mailcap-mime-info)
            (boundp 'mailcap-mime-data)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx229_mime_type_to_extension() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'mailcap-extension-to-mime)
          (fboundp 'mailcap-mime-type-to-extension)
          (boundp 'mailcap-mime-extensions))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx229_mm_decode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'mm-decode)
      (list (fboundp 'mm-decode-string)
            (fboundp 'mm-encode-string)
            (fboundp 'mm-dissect-buffer)
            (boundp 'mm-text-html-renderer)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx229_mm_encode_decode_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'mm-encode)
      (require 'mm-decode)
      (let* ((text "Hello café 世界")
             (enc (mm-encode-string text 'base64))
             (dec (mm-decode-string enc 'base64)))
        (list enc dec (string= text dec))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx229_message_rfc822_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'rfc822)
      (list (fboundp 'rfc822-addresses)
            (fboundp 'rfc822-parse-string)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx229_flatten_rfc822_addresses() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'mail-utils)
      (list (fboundp 'mail-string-delete)
            (fboundp 'mail-strip-quoted-names)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx229_rfc2047_encode_decode_headers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"plain\" \"=?utf-8?B?Y2Fmw6kg5LiW55WM?=\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'rfc2047)
      (list (rfc2047-encode-string "plain")
            (condition-case err (rfc2047-encode-string "café 世界") (error :err))
            (fboundp 'rfc2047-decode-string)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx229_server_running_p_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (server-running-p))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx229_server_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'server)
      (require 'mm-decode)
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Server/MIME mega test buffer content")
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (fboundp 'server-start)
                             (fboundp 'mm-decode-string)
                             (boundp 'mm-text-html-renderer)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

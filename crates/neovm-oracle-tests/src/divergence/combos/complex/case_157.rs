//! Complex combo batch 157 — `erc` / `rcirc` / `tracking` /
//! `notifications` / `notifications-notify` availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx157_erc_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'erc)
      (list (fboundp 'erc)
            (boundp 'erc-server)
            (boundp 'erc-port)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_rcirc_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'rcirc)
      (list (fboundp 'rcirc)
            (boundp 'rcirc-server-alist)
            (boundp 'rcirc-default-nick)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_notifications_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'notifications)
      (list (fboundp 'notifications-notify)
            (fboundp 'notifications-get-capabilities)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_tracking_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'tracking)
      (list (fboundp 'tracking-mode)
            (boundp 'tracking-buffers)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_erc_fill_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'erc-fill)
      (list (fboundp 'erc-fill-mode)
            (boundp 'erc-fill-column)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_erc_track_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'erc-track)
      (list (fboundp 'erc-track-mode)
            (boundp 'erc-track-exclude)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_erc_autojoin_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'erc-join)
      (list (fboundp 'erc-autojoin-mode)
            (boundp 'erc-autojoin-channels-alist)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_erc_log_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'erc-log)
      (list (fboundp 'erc-log-mode)
            (boundp 'erc-log-channels-directory)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_erc_sasl_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (featurep 'erc-sasl)
          (fboundp 'erc-sasl-mode))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_erc_services_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'erc-services)
      (list (fboundp 'erc-services-mode)
            (boundp 'erc-nickserv-passwords)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx157_erc_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (insert "ERC mega test buffer content")
      (put-text-property 1 5 'face 'bold)
      (let ((m (set-marker (make-marker) 8))
            (ov (make-overlay 4 14)))
        (overlay-put ov 'face 'italic)
        (overlay-put ov 'evaporate t)
        (narrow-to-region 2 18)
        (let ((state (list (fboundp 'erc)
                           (boundp 'erc-server)
                           (buffer-string)
                           (marker-position m)
                           (overlay-start ov) (overlay-end ov)
                           (text-properties-at 1))))
          (undo)
          (widen)
          (list state (buffer-string) (marker-position m)
                (overlay-start ov) (overlay-end ov)
                (text-properties-at 1)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

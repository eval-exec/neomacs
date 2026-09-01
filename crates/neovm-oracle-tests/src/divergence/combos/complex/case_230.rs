//! Complex combo batch 230 — `network` process / `make-network-process` /
//! TCP/UDP / `process-contact` / `process-datagram-address` queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx230_make_network_process_ipv4() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored file-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((p (make-network-process :name "neo-cx230-net"
                                    :host "127.0.0.1"
                                    :service 80
                                    :family 'ipv4)))
      (prog1 (list (processp p)
                   (process-name p)
                   (process-status p)
                   (process-contact p))
        (delete-process p)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx230_network_process_family_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'make-network-process)
          (fboundp 'set-network-process-option)
          (fboundp 'network-interface-list)
          (fboundp 'network-interface-info))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx230_network_interface_list_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ifaces (network-interface-list)))
      (list (or (null ifaces) (consp ifaces))
            (when ifaces (> (length ifaces) 0))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx230_network_interface_info_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((ifaces (network-interface-list))
           (first-iface (car ifaces)))
      (when first-iface
        (let ((info (network-interface-info first-iface)))
          (list (or (null info) (consp info))
                first-iface))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx230_datagram_process_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'make-network-process)
          (fboundp 'process-datagram-address))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx230_x_clipboard_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'x-set-selection)
          (fboundp 'x-get-selection)
          (fboundp 'gui-set-selection)
          (fboundp 'gui-get-selection)
          (boundp 'selection-coding-system))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx230_clipboard_manager_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'clipboard-kill-region)
          (fboundp 'clipboard-yank)
          (fboundp 'gui-select-text)
          (fboundp 'gui-backend-select-text))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx230_x_get_selection_owns_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'x-selection-owner-p)
          (fboundp 'gui-selection-owner-p))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx230_interprogram_cut_paste_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'interprogram-cut-function)
      (boundp 'interprogram-paste-function)
      (functionp interprogram-cut-function)
      (functionp interprogram-paste-function))
"##,
        expect,
    );
}

#[test]
fn div_cx230_network_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ifaces (network-interface-list)))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert (format "Network mega: %d interfaces" (length ifaces)))
        (put-text-property 1 6 'face 'bold)
        (let ((m (set-marker (make-marker) 10))
              (ov (make-overlay 4 18)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 25)
          (let ((state (list (fboundp 'make-network-process)
                             (boundp 'interprogram-cut-function)
                             ifaces
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

//! Network-process coverage (thin + fragile area).
//!
//! Deterministic, non-racy network ops on loopback: server/client creation
//! (status listen/open), process-contact info, process-type, set-process-filter/
//! sentinel (no error), process-list membership, delete-process→closed,
//! network-interface-list/info. Avoids comparing the OS-assigned ephemeral port
//! NUMBER (random per run) and data-exchange timing (racy).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_net_server_create_listen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (make-network-process :name "ns" :server t :service 0 :host "127.0.0.1")))
  (prog1 (list (processp s)
               (eq (process-status s) 'listen)
               (integerp (process-contact s :service)))
    (delete-process s)))
"##,
        expect,
    );
}

#[test]
fn div_net_client_connect_service_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (make-network-process :name "ns" :server t :service 0 :host "127.0.0.1")))
  (let ((port (process-contact s :service)))
    (let ((c (make-network-process :name "nc" :service port :host "127.0.0.1")))
      (accept-process-output c 0.05)
      (prog1 (list (processp c)
                   (eq (process-status c) 'open)
                   (eq (process-contact c :service) port))
        (delete-process c))))
  (delete-process s))
"##,
        expect,
    );
}

#[test]
fn div_net_process_type_and_contact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (network nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (make-network-process :name "ns" :server t :service 0 :host "127.0.0.1")))
  (prog1 (list (process-type s)
               (eq (process-contact s :host) '127.0.0.1)
               (memq (process-contact s :family) '(ipv4 local)))
    (delete-process s)))
"##,
        expect,
    );
}

#[test]
fn div_net_network_interface_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(consp (network-interface-list))
"##,
        expect,
    );
}

#[test]
fn div_net_network_interface_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ifs (network-interface-list)))
  (if ifs (consp (condition-case e (network-interface-info (caar ifs)) (error nil))) :none))
"##,
        expect,
    );
}

#[test]
fn div_net_delete_process_status() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK closed""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (make-network-process :name "ns" :server t :service 0 :host "127.0.0.1")))
  (delete-process s)
  (process-status s))
"##,
        expect,
    );
}

#[test]
fn div_net_set_process_filter_sentinel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((closure (t) (p m) nil) (closure (t) (p e) nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (make-network-process :name "ns" :server t :service 0 :host "127.0.0.1")))
  (prog1 (list (set-process-filter s (lambda (p m) nil))
               (set-process-sentinel s (lambda (p e) nil)))
    (delete-process s)))
"##,
        expect,
    );
}

#[test]
fn div_net_process_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (make-network-process :name "ns" :server t :service 0 :host "127.0.0.1")))
  (prog1 (listp (process-plist s))
    (delete-process s)))
"##,
        expect,
    );
}

#[test]
fn div_net_process_list_membership() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (make-network-process :name "ns" :server t :service 0 :host "127.0.0.1")))
  (prog1 (if (memq s (process-list)) t nil)
    (delete-process s)))
"##,
        expect,
    );
}

#[test]
fn div_net_process_name_and_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"ns\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (make-network-process :name "ns" :server t :service 0 :host "127.0.0.1")))
  (prog1 (list (process-name s) (eq (process-buffer s) nil))
    (delete-process s)))
"##,
        expect,
    );
}

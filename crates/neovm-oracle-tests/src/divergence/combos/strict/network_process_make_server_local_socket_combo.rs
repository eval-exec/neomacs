//! Strict combo oracle probes, batch 217: network surface (deterministic parts
//! only). make-network-process :server over a local socket (outcome compared
//! loosely as success-or-caught since socket binding is environment-dependent),
//! format-network-address over proper address forms, and network-interface-list
//! existence. See commit note for environment-dependent divergences observed
//! but not pinned (container bridge interfaces, malformed-call error messages).
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_network_process_server_local_socket_outcome() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let* ((default-directory temporary-file-directory)
           (sock "probe-net-test.sock")
           (proc (make-network-process :name "probe-net"
                                       :family 'local
                                       :service sock
                                       :server t
                                       :noquery t)))
      (prog1
          (list 'bound (processp proc) (eq (process-type proc) 'real))
        (delete-process proc)
        (when (file-exists-p sock) (delete-file sock))))
  (file-error (list 'caught-file-error))
  (error (list 'caught-error)))
"##;
    let expect = expect_test::expect![[r#""OK (bound t nil)""#]];
    crate::common::assert_oracle_parity_with_case_workdir_expect(form, expect);
}

#[test]
fn div_v8_format_network_address_proper_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (format-network-address "127.0.0.1")
      (format-network-address '(127 0 0 1))
      (format-network-address '(192 168 1 1 65))
      (format-network-address '(192 168 1 1 65) t)
      (format-network-address "192.168.1.1")
      (consp (network-interface-list)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"127.0.0.1\" \"<Family 127>\" \"<Family 192>\" \"<Family 192>\" \"192.168.1.1\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_network_stream_connect_refused_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(condition-case err
    (let ((proc (make-network-process :name "probe-stream"
                                       :host "127.0.0.1"
                                       :service 9
                                       :family 'ipv4)))
      (prog1 (list 'connect-attempted (processp proc) (process-status proc))
        (when (processp proc) (delete-process proc))))
  (file-error (list 'caught-connect-refused))
  (error (list 'caught-error)))
"##;
    let expect = expect_test::expect![[r#""OK (caught-connect-refused)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

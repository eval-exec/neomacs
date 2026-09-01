//! Extra process/network parity: datagram (UDP) loopback, binary send/recv,
//! query-on-exit flag, filter/sentinel getters, set-process-buffer,
//! waiting-for-user-input-p, accept-process-output after exit, send-after-eof,
//! and binary-coding filter string shape.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn accept_output_just_exited() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil exit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-aje-xxx" nil "true")))
  (set-process-query-on-exit-flag proc nil)
  (while (process-live-p proc) (accept-process-output proc 1))
  (list (accept-process-output proc 0.1) (process-status proc)))"##,
        expect,
    );
}

#[test]
fn datagram_udp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"ping-udp\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((srv nil) (recv nil))
  (setq srv (make-network-process :name "neo-udp-xxx" :type 'datagram :server t
             :host 'local :service t :family 'ipv4 :noquery t
             :filter (lambda (_p s) (setq recv s))))
  (let* ((local (process-contact srv :local)) (port (aref local (1- (length local)))))
    (let ((cli (make-network-process :name "neo-udpc-xxx" :type 'datagram :host 'local
                :service port :family 'ipv4 :noquery t)))
      (process-send-string cli "ping-udp")
      (let ((k 0)) (while (and (null recv) (< k 100)) (accept-process-output nil 0.02) (setq k (1+ k))))
      (delete-process cli) (delete-process srv)
      recv)))"##,
        expect,
    );
}

#[cfg(unix)]
#[test]
fn local_seqpacket_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (listen open t \"ping-seqpacket\")""#]];
    crate::common::assert_oracle_parity_with_case_workdir_expect(
        r##"(let* ((dir (or (getenv "NEOVM_ORACLE_TEST_TMPDIR") temporary-file-directory))
       (default-directory (file-name-as-directory dir))
       (path "neo-seqpacket.sock")
       (srv nil)
       (cli nil)
       (accepted nil)
       (recv nil))
  (when (file-exists-p path)
    (delete-file path))
  (unwind-protect
      (progn
        (setq srv
              (make-network-process
               :name "neo-seqp-srv"
               :family 'local
               :service path
               :server t
               :type 'seqpacket
               :noquery t
               :log (lambda (_server client _message)
                      (setq accepted client)
                      (set-process-query-on-exit-flag client nil)
                      (set-process-filter client
                                          (lambda (_proc string)
                                            (setq recv string))))))
        (setq cli
              (make-network-process
               :name "neo-seqp-cli"
               :family 'local
               :service path
               :type 'seqpacket
               :noquery t))
        (let ((i 0))
          (while (and (null accepted) (< i 100))
            (accept-process-output nil 0.02)
            (setq i (1+ i))))
        (process-send-string cli "ping-seqpacket")
        (let ((i 0))
          (while (and (null recv) (< i 100))
            (accept-process-output nil 0.02)
            (setq i (1+ i))))
        (list (process-status srv)
              (process-status cli)
              (processp accepted)
              recv))
    (when (processp cli)
      (delete-process cli))
    (when (processp accepted)
      (delete-process accepted))
    (when (processp srv)
      (delete-process srv))
    (when (file-exists-p path)
      (delete-file path))))"##,
        expect,
    );
}

#[test]
fn proc_buffer_reassign() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((b1 (generate-new-buffer " neo-pb1-xxx")) (b2 (generate-new-buffer " neo-pb2-xxx")))
  (let ((proc (start-process "neo-pbr-xxx" b1 "sleep" "5")))
    (set-process-query-on-exit-flag proc nil)
    (set-process-buffer proc b2)
    (prog1 (list (eq (process-buffer proc) b2))
      (delete-process proc) (kill-buffer b1) (kill-buffer b2))))"##,
        expect,
    );
}

#[test]
fn proc_filter_sentinel_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (make-process :name "neo-fsg-xxx" :command '("cat") :connection-type 'pipe :noquery t)))
  (set-process-filter proc 'ignore)
  (set-process-sentinel proc 'ignore)
  (prog1 (list (eq (process-filter proc) 'ignore) (eq (process-sentinel proc) 'ignore))
    (delete-process proc)))"##,
        expect,
    );
}

#[test]
fn proc_query_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((proc (start-process "neo-qf-xxx" nil "sleep" "5")))
  (set-process-query-on-exit-flag proc t)
  (prog1 (list (process-query-on-exit-flag proc)
               (progn (set-process-query-on-exit-flag proc nil)
                      (process-query-on-exit-flag proc)))
    (delete-process proc)))"##,
        expect,
    );
}

#[test]
fn proc_send_after_eof() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK sent""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (let ((proc (make-process :name "neo-sae-xxx" :command '("cat") :connection-type 'pipe :noquery t)))
  (process-send-eof proc)
  (process-send-string proc "after-eof")
  (delete-process proc) 'sent) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn divergence_proc_binary_filter_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 nil (0 1 2 255 254 10))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((acc ""))
  (let ((proc (make-process :name "neo-bin-xxx" :command '("cat")
               :connection-type 'pipe :coding 'binary
               :filter (lambda (_p s) (setq acc (concat acc s))))))
    (set-process-query-on-exit-flag proc nil)
    (process-send-string proc (unibyte-string 0 1 2 255 254 10))
    (process-send-eof proc)
    (while (process-live-p proc) (accept-process-output proc 1))
    (list (length acc) (multibyte-string-p acc) (append (string-to-unibyte acc) nil))))"##,
        expect,
    );
}

#[test]
fn proc_waiting_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (booleanp (waiting-for-user-input-p))
        (processp (get-buffer-process (current-buffer))))"##,
        expect,
    );
}

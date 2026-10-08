#[path = "../src/common.rs"]
mod common;
#[path = "../../../test/support/tls_loopback.rs"]
mod loopback;
use common::return_if_neovm_enable_oracle_proptest_not_set;
use std::time::Duration;

#[test]
fn trusted_tls_encrypted_io_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let peer = loopback::TlsPeer::spawn(2, Duration::ZERO, true);
    let expression = format!(
        r#"(let ((client (make-network-process :name "tls-oracle" :host "127.0.0.1" :service {} :buffer (generate-new-buffer " *tls*"))))
      (unwind-protect
          (progn
            (gnutls-boot client 'gnutls-x509pki '(:hostname "localhost" :trustfiles ("{}") :verify-error t :complete-negotiation t))
            (process-send-string client "encrypted oracle data\n")
            (accept-process-output client 1)
            (with-current-buffer (process-buffer client) (string-prefix-p "encrypted oracle data\n" (buffer-string))))
        (delete-process client)
        (kill-buffer (process-buffer client))))"#,
        peer.port,
        loopback::resources().join("ca.pem").display()
    );
    common::assert_oracle_parity_expect(&expression, expect_test::expect![[r#""OK t""#]]);
}

#[test]
fn tls_hostname_and_ca_verification_match_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    for (hostname, roots) in [
        (
            "wrong.example",
            format!(
                ":trustfiles (\"{}\")",
                loopback::resources().join("ca.pem").display()
            ),
        ),
        ("localhost", String::new()),
    ] {
        let peer = loopback::TlsPeer::spawn(2, Duration::ZERO, false);
        let expression = format!(
            r#"(let ((client (make-network-process :name "tls-rejected" :host "127.0.0.1" :service {})))
          (unwind-protect
              (condition-case nil
                  (progn (gnutls-boot client 'gnutls-x509pki '(:hostname "{}" {} :verify-error t :complete-negotiation t)) 'accepted)
                (error 'rejected))
            (delete-process client)))"#,
            peer.port, hostname, roots
        );
        common::assert_oracle_parity_expect(
            &expression,
            expect_test::expect![[r#""OK rejected""#]],
        );
    }
}

#[test]
fn timer_quit_propagates_to_the_wait_caller_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn (require 'timer)
              (let* ((inhibit-quit nil)
                     (timer (run-with-timer 0.025 nil (lambda () (setq quit-flag t)))))
                (unwind-protect
                    (condition-case nil (progn (sleep-for 0.2) 'returned) (quit 'quit))
                  (setq quit-flag nil) (cancel-timer timer))))"#,
        expect_test::expect![[r#""OK quit""#]],
    );
}

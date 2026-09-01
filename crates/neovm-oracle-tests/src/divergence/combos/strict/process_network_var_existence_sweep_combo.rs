//! Strict combo oracle probes, batch 254: process/network/shell variable
//! existence sweep. boundp over standard process/shell/network defcustoms.
//! Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_process_shell_exec_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'read-process-output-max)
      (boundp 'process-adaptive-read-buffering)
      (boundp 'process-file-side-effects)
      (boundp 'process-connection-type)
      (boundp 'delete-exited-processes)
      (boundp 'exec-path)
      (boundp 'exec-suffixes)
      (boundp 'exec-directory)
      (boundp 'shell-file-name)
      (boundp 'shell-command-switch)
      (boundp 'null-device)
      (boundp 'process-coding-system-alist)
      (boundp 'file-name-handler-alist)
      (boundp 'coding-system-for-read)
      (boundp 'coding-system-for-write))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_network_socket_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'network-security-level)
      (boundp 'network-security-protocol-checks)
      (boundp 'gnutls-log-level)
      (boundp 'gnutls-min-prime-bits)
      (boundp 'tls-program)
      (boundp 'tls-success-message)
      (boundp 'tls-checktrust)
      (boundp 'socks-server)
      (boundp 'socks-timeout)
      (boundp 'url-user-agent))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil t nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_signal_alarm_async_event_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'timer-list)
      (boundp 'timer-idle-list)
      (boundp 'timer-max-repeats)
      (boundp 'timer-debug)
      (boundp 'unread-command-events)
      (boundp 'unread-post-input-method-events)
      (boundp 'last-input-event)
      (boundp 'last-event-frame)
      (boundp 'last-nonmenu-event)
      (boundp 'track-mouse)
      (boundp 'double-click-time)
      (boundp 'mouse-1-click-follows-link))
"##;
    let expect = expect_test::expect![[r#""OK (t t t nil t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

//! Re-homed (option A) native deleted/dropped-PTY deferred-reap tests from the
//! system process tests. The deferred registry must outlive the last
//! evaluator's wait loop and reap a deleted child whose PTY still has unread
//! output.

use neovm_system::emacs_core::process::test_support::{
    DeferredReapFixtureForTest, DeferredWatcherTestScopeForTest,
};
use neovm_system::emacs_core::process::*;
use neovm_value::value::Value;
use std::os::fd::AsRawFd;

/// No output read is allowed before deletion. A readable master while the
/// slave waits for input establishes queued output without waiting for exit.
fn pty_with_unread_output() -> (ProcessManager, ProcessId, u32) {
    let mut manager = ProcessManager::new();
    let id = manager.create_process(
        "unread-pty-deferred-reap".into(),
        Value::NIL,
        "/bin/sh".into(),
        vec![
            "-c".into(),
            "printf 'process-output\\n'; read response".into(),
        ],
        ProcessCodingSystems::gnu_make_process_initial(),
    );
    manager.spawn_child(id, true).expect("own PTY child");
    let process = manager.get(id).expect("spawned process");
    let pid = process.os_pid.expect("own child pid");
    let fd = process
        .live_io
        .pty_master
        .as_ref()
        .and_then(|master| master.as_raw_fd())
        .expect("native master fd");
    let mut event = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        // SAFETY: one initialized pollfd borrowing the still-owned master.
        let ready = unsafe { libc::poll(&mut event, 1, 10_000) };
        if ready < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
            continue;
        }
        assert_eq!(ready, 1, "child must queue output before teardown");
        assert_ne!(event.revents & libc::POLLIN, 0, "unread master data");
        break;
    }
    (manager, id, pid)
}

#[test]
fn deleted_pty_with_unread_output_reaps_without_another_context_wait() {
    crate::test_utils::init_test_tracing();
    let _ctx = crate::test_utils::new_context();
    let watchers = DeferredWatcherTestScopeForTest::new();
    let (mut manager, id, pid) = pty_with_unread_output();
    assert!(manager.delete_process(id));
    assert!(manager.get(id).is_none());
    watchers.finish();
    assert!(!DeferredReapFixtureForTest::is_owned(pid));
}

#[test]
fn dropped_pty_with_unread_output_reaps_without_another_context_wait() {
    crate::test_utils::init_test_tracing();
    let _ctx = crate::test_utils::new_context();
    let watchers = DeferredWatcherTestScopeForTest::new();
    let (manager, _, pid) = pty_with_unread_output();
    drop(manager);
    // The native watcher, rather than another process wait, performs cleanup.
    watchers.finish();
    assert!(!DeferredReapFixtureForTest::is_owned(pid));
}

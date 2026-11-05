//! Native exit collection must outlive the last evaluator's wait loop.

use super::*;

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
    let watchers = super::reap::DeferredWatcherTestScope::new();
    let (mut manager, id, pid) = pty_with_unread_output();
    assert!(manager.delete_process(id));
    assert!(manager.get(id).is_none());
    watchers.finish();
    assert!(!super::reap::deferred_child_is_owned_for_test(pid));
}

#[test]
fn dropped_pty_with_unread_output_reaps_without_another_context_wait() {
    let watchers = super::reap::DeferredWatcherTestScope::new();
    let (manager, _, pid) = pty_with_unread_output();
    drop(manager);
    // The native watcher, rather than another process wait, performs cleanup.
    watchers.finish();
    assert!(!super::reap::deferred_child_is_owned_for_test(pid));
}

/// Keep every owned child blocked on its private stdin until every registration
/// exists. No exit race can reduce the number of pending native watchers.
#[test]
fn many_deferred_pipe_children_share_one_native_reaper_and_join() {
    use std::io::Read;
    use std::process::{Command, Stdio};

    let watchers = super::reap::DeferredWatcherTestScope::new();
    let mut inputs = Vec::new();
    let mut pids = Vec::new();
    for _ in 0..24 {
        let mut child = Command::new("/bin/sh")
            .args(["-c", "printf r; read response"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("own pipe child");
        inputs.push(child.stdin.take().expect("own child stdin"));
        let mut ready = [0];
        child
            .stdout
            .take()
            .expect("own child stdout")
            .read_exact(&mut ready)
            .expect("child is blocked waiting for private stdin");
        assert_eq!(ready, [b'r']);
        pids.push(child.id());
        let mut owner = super::reap::ChildOwnership::of_pipe_child(
            crate::emacs_core::callproc::SpawnedChild::from_std(child),
        );
        owner.defer_reaping();
        assert!(matches!(owner, super::reap::ChildOwnership::Deferred));
    }
    for &pid in &pids {
        assert!(super::reap::deferred_child_is_owned_for_test(pid));
    }
    assert_eq!(
        watchers.worker_count_for_test(),
        1,
        "all pending children must share one native reaper thread"
    );
    drop(inputs);
    // Neither a Lisp safe point nor a registry sweep on this test thread can
    // collect the children. The shared native collector completes and joins.
    watchers.finish();
    for pid in pids {
        assert!(!super::reap::deferred_child_is_owned_for_test(pid));
    }
}

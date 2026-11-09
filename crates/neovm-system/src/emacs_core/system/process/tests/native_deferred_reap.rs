//! Native exit collection must outlive the last evaluator's wait loop.

use super::*;

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
        let mut owner =
            neovm_eval::emacs_core::eval::data::process_state::ChildOwnership::of_pipe_child(
                crate::emacs_core::callproc::SpawnedChild::from_std(child),
            );
        owner.defer_reaping();
        assert!(matches!(
            owner,
            neovm_eval::emacs_core::eval::data::process_state::ChildOwnership::Deferred
        ));
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

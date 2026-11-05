//! Deleted children must never block the evaluator while their PTY drains.

use super::*;
use std::io;
use std::sync::atomic::AtomicUsize;

#[derive(Debug, Default)]
struct MockState {
    ready: AtomicBool,
    interrupt_next: AtomicBool,
    probes: AtomicUsize,
    drops: AtomicUsize,
}

#[derive(Debug)]
struct NonblockingOnlyChild(Arc<MockState>);

impl portable_pty::ChildKiller for NonblockingOnlyChild {
    fn kill(&mut self) -> io::Result<()> {
        panic!("the deferred registry must never signal a child")
    }

    fn clone_killer(&self) -> Box<dyn portable_pty::ChildKiller + Send + Sync> {
        panic!("deferred ownership must never duplicate signal authority")
    }
}

impl portable_pty::Child for NonblockingOnlyChild {
    fn try_wait(&mut self) -> io::Result<Option<portable_pty::ExitStatus>> {
        self.0.probes.fetch_add(1, Ordering::Relaxed);
        if self.0.interrupt_next.swap(false, Ordering::AcqRel) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        Ok(self
            .0
            .ready
            .load(Ordering::Acquire)
            .then(|| portable_pty::ExitStatus::with_exit_code(0)))
    }

    fn wait(&mut self) -> io::Result<portable_pty::ExitStatus> {
        panic!("asynchronous delete/drop must never perform a blocking child wait")
    }

    fn process_id(&self) -> Option<u32> {
        None
    }

    #[cfg(windows)]
    fn as_raw_handle(&self) -> Option<std::os::windows::io::RawHandle> {
        None
    }
}

impl Drop for NonblockingOnlyChild {
    fn drop(&mut self) {
        self.0.drops.fetch_add(1, Ordering::Release);
    }
}

fn mock_owner(state: &Arc<MockState>) -> ChildOwnership {
    // The mock has no OS child. Its pid metadata is never handed to an OS
    // syscall: the test helper only transfers ownership, and collection delegates
    // exclusively to the mock's nonblocking try_wait. Never invoke a signal
    // operation on this fixture.
    ChildOwnership::Unreaped(ReapableChild {
        pid: std::process::id(),
        handle: ChildHandle::Pty(Box::new(NonblockingOnlyChild(Arc::clone(state)))),
    })
}

fn sweep() {
    // The deleted-pid sweep must run even with no live process-table entries.
    let mut manager = super::super::ProcessManager::new();
    manager.record_child_status_changes(
        crate::emacs_core::wait::WaitStatusNotifySite::for_a_unit_test_of_the_walk_itself(),
    );
}

#[test]
fn deferred_child_reaping_transfers_ownership_without_blocking_wait() {
    let state = Arc::new(MockState::default());
    let mut owner = mock_owner(&state);
    owner.defer_reaping_for_test();
    assert!(matches!(owner, ChildOwnership::Deferred));
    assert_eq!(owner.pid_if_unreaped(), None);
    assert_eq!(state.drops.load(Ordering::Acquire), 0);

    // Exercise the aggregate Drop with authority already transferred: it must
    // neither wait nor signal the registry's child a second time.
    let mut io = super::super::LiveProcessIo::default();
    io.child = owner;
    drop(io);
    sweep();
    assert!(state.probes.load(Ordering::Relaxed) >= 1);
    assert_eq!(state.drops.load(Ordering::Acquire), 0);

    state.ready.store(true, Ordering::Release);
    sweep();
    assert_eq!(state.drops.load(Ordering::Acquire), 1);
    sweep();
    assert_eq!(state.drops.load(Ordering::Acquire), 1);
}

#[test]
fn deferred_child_reaping_retains_ownership_after_interrupted_probe() {
    let state = Arc::new(MockState::default());
    let mut owner = mock_owner(&state);
    owner.defer_reaping_for_test();
    state.interrupt_next.store(true, Ordering::Release);
    sweep();
    assert_eq!(state.drops.load(Ordering::Acquire), 0);
    state.ready.store(true, Ordering::Release);
    sweep();
    assert_eq!(state.drops.load(Ordering::Acquire), 1);
}

#[test]
fn native_deferred_child_is_not_probed_by_ordinary_wait_polls() {
    let state = Arc::new(MockState::default());
    let mut owner = mock_owner(&state);
    let child = owner
        .take_unreaped_for_transfer()
        .expect("mock native owner");
    let completed = publish_deferred_child(child, DeferredReapDriver::NativeExit);
    // No actual native watcher is registered against this synthetic handle.
    // Force the native-driven state, then exercise ordinary wait safe points.
    state.ready.store(true, Ordering::Release);
    for _ in 0..1000 {
        sweep();
    }
    assert_eq!(state.probes.load(Ordering::Relaxed), 0);
    assert_eq!(state.drops.load(Ordering::Acquire), 0);
    assert!(!completed.load(Ordering::Acquire));

    collect_deferred_children(DeferredReapSweep::NativeExit);
    assert!(completed.load(Ordering::Acquire));
    assert_eq!(state.drops.load(Ordering::Acquire), 1);
}

#[test]
fn native_reaper_collects_only_ready_entries_and_keeps_completion_identity() {
    let mut registry = DeferredRegistry::new();
    let mut fixtures = Vec::new();
    for _ in 0..64 {
        let state = Arc::new(MockState::default());
        let owner = mock_owner(&state)
            .take_unreaped_for_transfer()
            .expect("synthetic owned child");
        let (completed, id) = registry.publish(owner, DeferredReapDriver::NativeExit);
        fixtures.push((state, completed, id.expect("native identity")));
    }
    let (ready, completed, ready_id) = &fixtures[17];
    ready.ready.store(true, Ordering::Release);
    collect_native_notifications(
        &mut registry,
        vec![super::super::sys::ChildExitNotification::Exited(
            ready_id.as_native(),
        )],
    );
    assert!(completed.load(Ordering::Acquire));
    assert_eq!(ready.probes.load(Ordering::Relaxed), 1);
    for (index, (state, completed, _)) in fixtures.iter().enumerate() {
        if index != 17 {
            assert_eq!(
                state.probes.load(Ordering::Relaxed),
                0,
                "unready child must not be swept"
            );
            assert!(!completed.load(Ordering::Acquire));
        }
    }
    // Reuse the same synthetic PID with a new completion identity, then deliver
    // a stale old native event. It must not probe or complete the replacement.
    let replacement = Arc::new(MockState::default());
    replacement.ready.store(true, Ordering::Release);
    let child = mock_owner(&replacement)
        .take_unreaped_for_transfer()
        .expect("replacement owner");
    let (replacement_completed, replacement_id) =
        registry.publish(child, DeferredReapDriver::NativeExit);
    assert_ne!(replacement_id, Some(*ready_id));
    collect_native_notifications(
        &mut registry,
        vec![super::super::sys::ChildExitNotification::Exited(
            ready_id.as_native(),
        )],
    );
    assert!(!replacement_completed.load(Ordering::Acquire));
    assert_eq!(replacement.probes.load(Ordering::Relaxed), 0);
}

#[test]
fn native_reaper_sigchld_retries_only_note_exit_children_before_zombie() {
    let mut registry = DeferredRegistry::new();
    let exiting = Arc::new(MockState::default());
    let running = Arc::new(MockState::default());
    let child = mock_owner(&exiting)
        .take_unreaped_for_transfer()
        .expect("exiting owner");
    let (completed, id) = registry.publish(child, DeferredReapDriver::NativeExit);
    let id = id.expect("native identity");
    let child = mock_owner(&running)
        .take_unreaped_for_transfer()
        .expect("running owner");
    let (running_completed, running_id) = registry.publish(child, DeferredReapDriver::NativeExit);
    // A registration wake may inspect a still-running child once, but must not
    // put it in the NOTE_EXIT-before-zombie retry population.
    registry
        .newly_registered
        .push(running_id.expect("running identity"));
    collect_native_notifications(
        &mut registry,
        vec![super::super::sys::ChildExitNotification::Exited(
            id.as_native(),
        )],
    );
    assert!(!completed.load(Ordering::Acquire));
    assert_eq!(exiting.probes.load(Ordering::Relaxed), 1);
    assert_eq!(running.probes.load(Ordering::Relaxed), 1);
    exiting.ready.store(true, Ordering::Release);
    collect_native_notifications(
        &mut registry,
        vec![super::super::sys::ChildExitNotification::ChildSignal],
    );
    assert!(completed.load(Ordering::Acquire));
    assert_eq!(exiting.probes.load(Ordering::Relaxed), 2);
    assert_eq!(
        running.probes.load(Ordering::Relaxed),
        1,
        "SIGCHLD must not sweep running registrations"
    );
    assert!(!running_completed.load(Ordering::Acquire));
}

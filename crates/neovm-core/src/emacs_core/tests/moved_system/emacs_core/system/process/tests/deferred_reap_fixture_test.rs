//! Re-homed (option A) deferred-reap mock-path unit tests from the system
//! process tests: ownership transfer without a blocking wait, retention after
//! an interrupted probe, and the ordinary-wait non-probe guarantee. The
//! registry internals stay private to `neovm_system::...::process::reap`; this
//! file drives the same paths through the signed test-support facade.

use neovm_eval::emacs_core::eval::data::process_state::{
    ChildHandle, ChildOwnership, LiveProcessIo, ReapableChild,
};
use neovm_system::emacs_core::process::reap::ChildOwnershipIo;
use neovm_system::emacs_core::process::test_support::DeferredReapFixtureForTest;
use neovm_system::emacs_core::process::*;
use neovm_system::emacs_core::wait::WaitStatusNotifySite;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
    // syscall: the test helper only transfers ownership, and collection
    // delegates exclusively to the mock's nonblocking try_wait. Never invoke a
    // signal operation on this fixture.
    ChildOwnership::Unreaped(ReapableChild {
        pid: std::process::id(),
        handle: ChildHandle::Pty(Box::new(NonblockingOnlyChild(Arc::clone(state)))),
    })
}

/// The deleted-pid sweep must run even with no live process-table entries.
fn sweep() {
    let mut manager = ProcessManager::new();
    manager.record_child_status_changes(WaitStatusNotifySite::for_a_unit_test_of_the_walk_itself());
}

#[test]
fn deferred_child_reaping_transfers_ownership_without_blocking_wait() {
    crate::test_utils::init_test_tracing();
    let _ctx = crate::test_utils::new_context();
    let state = Arc::new(MockState::default());
    let mut owner = mock_owner(&state);
    owner.defer_reaping_for_test();
    assert!(matches!(owner, ChildOwnership::Deferred));
    assert_eq!(owner.pid_if_unreaped(), None);
    assert_eq!(state.drops.load(Ordering::Acquire), 0);

    // Exercise the aggregate Drop with authority already transferred: it must
    // neither wait nor signal the registry's child a second time.
    let mut io = LiveProcessIo::default();
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
    crate::test_utils::init_test_tracing();
    let _ctx = crate::test_utils::new_context();
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
    crate::test_utils::init_test_tracing();
    let _ctx = crate::test_utils::new_context();
    let state = Arc::new(MockState::default());
    let mut owner = mock_owner(&state);
    let child = owner
        .take_unreaped_for_transfer()
        .expect("mock native owner");
    let completed = DeferredReapFixtureForTest::publish_native(child);
    // No actual native watcher is registered against this synthetic handle.
    // Force the native-driven state, then exercise ordinary wait safe points.
    state.ready.store(true, Ordering::Release);
    for _ in 0..1000 {
        sweep();
    }
    assert_eq!(state.probes.load(Ordering::Relaxed), 0);
    assert_eq!(state.drops.load(Ordering::Acquire), 0);
    assert!(!completed.load(Ordering::Acquire));

    DeferredReapFixtureForTest::collect_native();
    assert!(completed.load(Ordering::Acquire));
    assert_eq!(state.drops.load(Ordering::Acquire), 1);
}

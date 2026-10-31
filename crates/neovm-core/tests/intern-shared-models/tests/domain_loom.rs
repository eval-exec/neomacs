//! The real process-domain algorithm, with Loom atomics at its backend seam.
#![allow(dead_code)]
mod sync {
    pub use loom::sync::atomic::{AtomicU64, Ordering};
}
#[path = "../../../src/emacs_core/runtime/intern/shared/domain.rs"]
mod domain;
#[path = "../../nmjit-publication-loom/tests/support/mod.rs"]
mod support;
use domain::*;
use loom::sync::{Arc, Mutex, mpsc};
use loom::thread;

struct Protected {
    domain: InternReadDomain,
    cell: loom::cell::UnsafeCell<u64>,
}
// SAFETY: every modeled cell access below holds this same domain's counted
// read scope or exclusive zero-reader grace. There are no untracked accesses.
unsafe impl Sync for Protected {}

#[test]
fn real_reader_entry_and_grace_release_order_cell_accesses() {
    support::exhaustive(2, || {
        let state = Arc::new(Protected {
            domain: InternReadDomain::new(),
            cell: loom::cell::UnsafeCell::new(0),
        });
        let reader = Arc::clone(&state);
        let thread = thread::spawn(move || {
            match reader.domain.try_scope() {
                Ok(scope) => {
                    let view = scope.borrow(&reader.cell);
                    view.get().with(|ptr| {
                        // SAFETY: this read is within the real domain scope.
                        let value = unsafe { *ptr };
                        assert!(matches!(value, 0 | 7));
                    });
                }
                Err(ReadRefusal::Busy | ReadRefusal::Draining) => {}
                Err(ReadRefusal::Saturated | ReadRefusal::ReservedState) => {
                    panic!("unexpected domain state")
                }
            }
        });
        match state.domain.try_grace() {
            Ok(grace) => {
                grace.verify_domain(&state.domain).unwrap();
                state.cell.with_mut(|ptr| {
                    // SAFETY: real exclusive grace excludes every cell reader.
                    unsafe {
                        *ptr = 7;
                    }
                });
            }
            Err(ReadRefusal::Busy | ReadRefusal::Draining) => {}
            Err(ReadRefusal::Saturated | ReadRefusal::ReservedState) => {
                panic!("unexpected domain state")
            }
        }
        support::join(thread);
        assert!(state.domain.try_grace().is_ok());
    });
}

#[test]
fn real_scopes_from_multiple_owners_block_process_grace() {
    support::exhaustive(2, || {
        let domain = Arc::new(InternReadDomain::new());
        let local = domain.try_scope().unwrap();
        let peer = Arc::clone(&domain);
        let reader = thread::spawn(move || {
            let scope = peer.try_scope().unwrap();
            assert!(matches!(peer.try_grace(), Err(ReadRefusal::Busy)));
            drop(scope);
        });
        assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
        support::join(reader);
        assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
        drop(local);
        assert!(domain.try_grace().is_ok());
    });
}

#[derive(Clone, Copy)]
enum WaitPolicy {
    DeferOutsideLocks,
    WaitUnderWorldWriter,
    RetryWithRegistryHeld,
}

/// Only this test adapter blocks. A held real scope must reach registry.lock()
/// before its exit can wake the writer. Each forbidden policy literally holds
/// that registry while waiting for the exit event, closing the wait-for cycle.
fn registry_history(policy: WaitPolicy) {
    let domain = Arc::new(InternReadDomain::new());
    let registry = Arc::new(Mutex::new(()));
    let world = Mutex::new(());
    let (ready_tx, ready_rx) = mpsc::channel();
    let (probe_tx, probe_rx) = mpsc::channel();
    let (exited_tx, exited_rx) = mpsc::channel();
    let (reader_domain, reader_registry) = (Arc::clone(&domain), Arc::clone(&registry));
    let reader = thread::spawn(move || {
        let scope = reader_domain.try_scope().unwrap();
        ready_tx.send(()).unwrap();
        probe_rx.recv().unwrap();
        {
            let _registry = reader_registry.lock().unwrap();
        }
        drop(scope);
        exited_tx.send(()).unwrap();
    });
    ready_rx.recv().unwrap();
    match policy {
        WaitPolicy::DeferOutsideLocks => {
            {
                let _world_writer = world.lock().unwrap();
                let _registry_writer = registry.lock().unwrap();
                // Publication preparation only: no grace attempt under locks.
            }
            assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
            probe_tx.send(()).unwrap();
            exited_rx.recv().unwrap();
        }
        WaitPolicy::WaitUnderWorldWriter => {
            let _world_writer = world.lock().unwrap();
            let _registry_writer = registry.lock().unwrap();
            assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
            probe_tx.send(()).unwrap();
            // Illegal blocking adapter: wait for the scoped reader while still
            // holding WRITER and its registry, then retry grace on wakeup.
            exited_rx.recv().unwrap();
            assert!(domain.try_grace().is_ok());
        }
        WaitPolicy::RetryWithRegistryHeld => {
            let _registry_writer = registry.lock().unwrap();
            assert!(matches!(domain.try_grace(), Err(ReadRefusal::Busy)));
            probe_tx.send(()).unwrap();
            // Illegal cold retry retains registry while waiting for grace.
            exited_rx.recv().unwrap();
            assert!(domain.try_grace().is_ok());
        }
    }
    support::join(reader);
    assert!(domain.try_grace().is_ok());
}

/// Loom 0.7.2 detects these cycles, but unwinding a still-blocked actor can
/// panic again in its runtime destructor. Capture the scheduler's original
/// panic in a child process before unwinding; the parent requires both its
/// dedicated exit status and the actual blocked-thread payload. Neither a
/// successful history nor an unrelated panic can satisfy this control.
fn negative(policy: WaitPolicy) {
    const WITNESS_EXIT: i32 = 73;
    const CHILD: &str = "INTERN_SHARED_DEADLOCK_CHILD";
    const HEADING: &str = "deadlock; threads = ";
    const RECEIPT: &str = "LOOM_DEADLOCK_WITNESS ";
    let current = std::thread::current();
    let name = current.name().expect("named libtest actor");
    if std::env::var(CHILD).ok().as_deref() == Some(name) {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let message = info
                .payload()
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| info.payload().downcast_ref::<&str>().copied());
            if let Some(message) = message.filter(|text| text.starts_with(HEADING)) {
                eprintln!("{RECEIPT}{message}");
                // Only Loom's observed deadlock panic produces this receipt;
                // no modeled actor chooses or manufactures a witness.
                std::process::exit(WITNESS_EXIT);
            }
            previous(info);
        }));
        support::exhaustive(2, move || registry_history(policy));
        panic!("forbidden wait unexpectedly completed");
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture", "--test-threads=1"])
        .env(CHILD, name)
        .output()
        .expect("run isolated deadlock control");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 Loom diagnostic");
    assert_eq!(output.status.code(), Some(WITNESS_EXIT), "{stderr}");
    let witness = stderr
        .lines()
        .find_map(|line| line.strip_prefix(RECEIPT))
        .expect("child must capture Loom's original deadlock panic");
    assert!(witness.starts_with(HEADING));
    assert_eq!(witness.matches("Blocked(").count(), 2, "{witness}");
    eprintln!("{RECEIPT}{witness}");
}

#[test]
fn writer_defers_grace_and_unwinds_registry_before_reader_slow_path() {
    support::exhaustive(2, || registry_history(WaitPolicy::DeferOutsideLocks));
}

#[test]
fn negative_writer_waits_for_read_domain_under_world_writer() {
    negative(WaitPolicy::WaitUnderWorldWriter);
}

#[test]
fn negative_cold_retry_holds_registry_while_reader_needs_registry() {
    negative(WaitPolicy::RetryWithRegistryHeld);
}

#[test]
fn draining_reaches_grace_after_at_most_two_preexisting_reader_exits() {
    support::exhaustive(3, || {
        let domain = Arc::new(InternReadDomain::new());
        let (ready_tx, ready_rx) = mpsc::channel();
        let (exit_tx, exit_rx) = mpsc::channel();
        let mut readers = Vec::new();
        let mut resume = Vec::new();
        for _ in 0..2 {
            let domain = Arc::clone(&domain);
            let ready = ready_tx.clone();
            let exited = exit_tx.clone();
            let (resume_tx, resume_rx) = mpsc::channel();
            resume.push(resume_tx);
            readers.push(thread::spawn(move || {
                let old = domain.try_scope().unwrap();
                ready.send(()).unwrap();
                resume_rx.recv().unwrap();
                // Churn cannot replace either of the two pre-drain scopes.
                for _ in 0..3 {
                    assert!(matches!(domain.try_scope(), Err(ReadRefusal::Draining)));
                    thread::yield_now();
                }
                drop(old);
                exited.send(()).unwrap();
            }));
            // Establish each old scope before starting the next actor. Entry
            // is try-once: concurrent admission may legitimately lose its CAS.
            ready_rx.recv().unwrap();
        }
        // Stable preexisting scopes make this boundary's single CAS succeed.
        let drain = domain.request_drain().unwrap();
        let drain = match drain.try_grace() {
            Err(pending) => pending,
            Ok(_) => panic!("two scopes still held"),
        };
        for sender in resume {
            sender.send(()).unwrap();
        }
        // These events model the later publication boundary, outside all
        // WRITER/registry/ledger locks. The production writer never waits.
        exit_rx.recv().unwrap();
        exit_rx.recv().unwrap();
        let grace = drain
            .try_grace()
            .unwrap_or_else(|_| panic!("two exits suffice"));
        grace.verify_domain(&domain).unwrap();
        drop(grace);
        for reader in readers {
            support::join(reader);
        }
        assert!(domain.try_scope().is_ok());
    });
}

#[test]
fn concurrent_drain_request_cannot_be_lost_by_grace_exit() {
    support::exhaustive(2, || {
        let domain = Arc::new(InternReadDomain::new());
        let grace = domain.try_grace().unwrap();
        let writer_domain = Arc::clone(&domain);
        let (requested_tx, requested_rx) = mpsc::channel();
        let (retired_tx, retired_rx) = mpsc::channel();
        let writer = thread::spawn(move || {
            let drain = writer_domain.request_drain().unwrap();
            requested_tx.send(()).unwrap();
            retired_rx.recv().unwrap();
            assert!(matches!(
                writer_domain.try_scope(),
                Err(ReadRefusal::Draining)
            ));
            let grace = drain
                .try_grace()
                .unwrap_or_else(|_| panic!("previous grace retired"));
            drop(grace);
        });
        // Race the bit-set directly against grace's Release clear.
        drop(grace);
        requested_rx.recv().unwrap();
        retired_tx.send(()).unwrap();
        support::join(writer);
        assert!(domain.try_scope().is_ok());
    });
}

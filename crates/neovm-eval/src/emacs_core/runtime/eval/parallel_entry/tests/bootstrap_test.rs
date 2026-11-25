use super::super::{SourceWithdrawalFailure, SourceWithdrawalStep};
use super::PreparedEraStart;
use super::era_fixture::{close, finish_fixture, ready};
use neovm_value::tagged::gc::heap_transfer::TransferError;
use neovm_value::value::Value;

#[test]
fn pentry_e2b_bootstrap_actual_source_send_and_verified_return() {
    let mut source = ready();
    let private = Value::string("actual-boxed-source-private-root");
    source.lexenv = private;
    let alive = source.world_lifetime_probe_for_test();
    let original = std::ptr::from_ref(&*source);
    let (shell, start) = close(source).prepare_era().unwrap();
    assert_eq!(std::ptr::from_ref(&**shell.frame.source), original);
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let end = std::thread::spawn(move || {
        let transfer = start
            .into_heap_transfer()
            .source_roundtrip_for_test()
            .expect("destination-local GEN1 loan");
        PreparedEraStart { transfer }.return_to_main().unwrap()
    })
    .join()
    .expect("service thread");
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
    assert!(
        !alive(),
        "the fixture joins its actual GC service after the matched return"
    );
}

#[test]
fn pentry_e2b_bootstrap_source_drop_poisons_the_prepared_right() {
    let source = ready();
    let alive = source.world_lifetime_probe_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();
    drop(shell);
    assert!(start.return_to_main().is_err());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_bootstrap_prepared_drop_and_source_forget_retain_backing() {
    let source = ready();
    let alive = source.world_lifetime_probe_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();
    drop(start);
    std::mem::forget(shell);
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_bootstrap_catch_unwind_does_not_restore_source_execution() {
    let source = ready();
    let alive = source.world_lifetime_probe_for_test();
    let pair = close(source).prepare_era().unwrap();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _pair = pair;
        panic!("SUSPENDED_MAIN_UNWIND");
    }));
    assert!(caught.is_err());
    assert!(alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_e2b_bootstrap_foreign_tls_preparation_failure_retains_actual_shell() {
    let source = ready();
    let alive = source.world_lifetime_probe_for_test();
    let closed = close(source);
    let mut foreign = Box::new(super::super::super::super::Context::new_minimal_vm_harness(
        crate::test_utils::unit_harness(),
    ));
    foreign.setup_thread_locals();
    let failure = match closed.prepare_era() {
        Err(failure) => failure,
        Ok(_) => panic!("foreign TLS was accepted"),
    };
    assert!(format!("{failure:?}").contains("InstalledTls"));
    match &failure {
        SourceWithdrawalFailure::Retained(retained) => {
            assert_eq!(retained.step, SourceWithdrawalStep::HeapTransfer);
        }
        SourceWithdrawalFailure::BeforeEffects(_) => panic!("source had already closed"),
    }
    drop(failure);
    assert!(neovm_value::tagged::gc::tagged_heap_is_current(
        &foreign.tagged_heap
    ));
    // SAFETY: `foreign` is about to drop at the end of this test.
    unsafe { neovm_value::tagged::gc::clear_tagged_heap_if_installed(&foreign.tagged_heap) };
    assert!(alive());
}

#[test]
fn pentry_e2b_bootstrap_foreign_return_cannot_reactivate_either_source() {
    let first = ready();
    let first_alive = first.world_lifetime_probe_for_test();
    let (first, first_start) = close(first).prepare_era().unwrap();
    let second = ready();
    let second_alive = second.world_lifetime_probe_for_test();
    let (second, second_start) = close(second).prepare_era().unwrap();
    let end = second_start.return_to_main().unwrap();
    let failure = match first.accept_return(end) {
        Err(failure) => failure,
        Ok(_) => panic!("foreign return was accepted"),
    };
    assert_eq!(failure.failure.reason(), TransferError::WrongReturn);
    drop(failure);
    drop(first_start);
    drop(second);
    assert!(first_alive() && second_alive());
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn pentry_l61_keeper_service_admits_and_finishes_its_own_era() {
    use super::super::keeper_service::keeper_start;
    let mut source = ready();
    let private = Value::string("l61-keeper-service-private-root");
    source.lexenv = private;
    let (shell, start) = close(source).prepare_era().unwrap();
    let main_id = std::thread::current().id();
    let (tx, rx) = std::sync::mpsc::channel();
    let service = std::thread::spawn(move || {
        let service_id = std::thread::current().id();
        match keeper_start(start) {
            Ok(end) => {
                tx.send((service_id, Ok(end))).expect("send keeper end");
            }
            Err(failure) => {
                tx.send((service_id, Err(failure.notice())))
                    .expect("send keeper notice");
            }
        }
    });
    let (service_id, outcome) = rx.recv().expect("keeper outcome");
    service.join().expect("keeper service thread");
    assert_ne!(
        service_id, main_id,
        "keeper_start runs on a dedicated service OS thread, not main"
    );
    let end = outcome.unwrap_or_else(|notice| panic!("keeper start refused: {notice:?}"));
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
}

#[test]
fn pentry_l62_real_thread_stop_and_resume_on_the_service_thread() {
    use super::super::keeper_service::keeper_serve;
    let mut source = ready();
    let private = Value::string("l62-keeper-stop-private-root");
    source.lexenv = private;
    let (shell, start) = close(source).prepare_era().unwrap();
    let main_id = std::thread::current().id();
    let (tx, rx) = std::sync::mpsc::channel();
    let service = std::thread::spawn(move || {
        let service_id = std::thread::current().id();
        let outcome = keeper_serve(start, |keeper| {
            // No other owner is registered. The keeper's stop returns as soon
            // as it requests the epoch; a Busy retry cannot clear that.
            let mut stopped = keeper.stop().expect("keeper stop");
            assert!(stopped.epoch().get() > 0, "the real stop has an epoch");
            // A stopped epoch completes only after its trace callback; this
            // control-only slice traces nothing (L6.5 runs the collection
            // here).
            stopped
                .trace(|_stopped, _proof| Ok(()))
                .expect("control trace");
            stopped.complete().expect("resume the stopped epoch");
        });
        match outcome {
            Ok((end, ())) => tx.send((service_id, Ok(end))).expect("send keeper end"),
            Err(failure) => tx
                .send((service_id, Err(failure.notice())))
                .expect("send keeper notice"),
        }
    });
    let (service_id, outcome) = rx.recv().expect("keeper outcome");
    service.join().expect("keeper service thread");
    assert_ne!(
        service_id, main_id,
        "the stop and resume run on the dedicated service OS thread"
    );
    let end = outcome.unwrap_or_else(|notice| panic!("keeper stop refused: {notice:?}"));
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
}

#[test]
fn pentry_l62_keeper_era_control_persists_across_two_stops() {
    use super::super::keeper_service::keeper_serve;
    let mut source = ready();
    let private = Value::string("l62-keeper-two-stops-private-root");
    source.lexenv = private;
    let (shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |keeper| {
            let mut last_epoch = 0;
            for _round in 0..2 {
                let mut stopped = keeper.stop().expect("keeper stop");
                let epoch = stopped.epoch().get();
                assert!(
                    epoch > last_epoch,
                    "each stop has its own later epoch ({epoch} after {last_epoch})"
                );
                last_epoch = epoch;
                stopped
                    .trace(|_stopped, _proof| Ok(()))
                    .expect("control trace");
                stopped.complete().expect("resume the stopped epoch");
            }
        });
        match outcome {
            Ok((end, ())) => tx.send(Ok(end)).expect("send keeper end"),
            Err(failure) => tx.send(Err(failure.notice())).expect("send keeper notice"),
        }
    });
    let outcome = rx.recv().expect("keeper outcome");
    service.join().expect("keeper service thread");
    let end = outcome.unwrap_or_else(|notice| panic!("keeper stops refused: {notice:?}"));
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
}

/// Live L6.2 stop/export/lend on a real `PublishedOwner`.
///
/// The current owner waits on the product condvar, then publishes native.
/// The keeper's single `stop` cannot return until that publish, and the two
/// epochs match. A serial request queued before the stop is still the head
/// and is not granted. A retired receipt is stale (`Publication`). A
/// superseded receipt is the wrong generation (`Generation`). One lend
/// consumes the stopped keeper. `SuspendedKeeper` has no lend, so a second
/// stop while it is held is `Busy`. `ExportReservation` stays inert: this
/// row never collects or loans the heap through it. Completing the
/// already-traced epoch is the keeper control finish, the queued request is
/// still there, and the same `KeeperEra` then stops again.
#[test]
fn pentry_l62_live_lend_refuses_a_stale_owner_a_wrong_generation_and_a_second_stop() {
    use super::super::super::keeper_custody::StoppedKeeper;
    use super::era_fixture::{KeeperPanicGuard, activate, keeper_serve, retire, retry_native};
    use neovm_value::tagged::gc::RendezvousError;
    use neovm_value::tagged::gc::heap_transfer::with_stop_scope;
    use std::num::NonZeroU64;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    const STALE_OWNER: u64 = 6201;
    const CURRENT_OWNER: u64 = 6202;
    // Recorded by the test probe. Stopping refuses the grant before any
    // publication check, so this is not a product ring generation.
    const SERIAL_GENERATION: NonZeroU64 = NonZeroU64::MIN;

    let mut source = ready();
    let private = Value::string("l62-live-lend-private-root");
    source.lexenv = private;
    let (shell, start) = close(source).prepare_era().unwrap();
    let (tx, rx) = mpsc::channel();
    let service = std::thread::spawn(move || {
        let outcome = keeper_serve(start, |keeper| {
            let heap = keeper.heap_identity();
            let stale_permit = keeper
                .register(STALE_OWNER, 0)
                .expect("register the stale owner");
            let current_permit = keeper
                .register(CURRENT_OWNER, 0)
                .expect("register the current owner");
            let current_key = current_permit.key();
            let (stale_tx, stale_rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel::<()>();
            let (ack_tx, ack_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let (pre_tx, pre_rx) = mpsc::channel::<()>();
            let (go_tx, go_rx) = mpsc::channel::<()>();
            // Set only after `stop` returns. The checker must observe it
            // still false between the product wait and the native publish.
            let stop_returned = Arc::new(AtomicBool::new(false));
            let checker_flag = Arc::clone(&stop_returned);
            std::thread::scope(|scope| {
                // Inside the scope: its destructor runs before `thread::scope`
                // joins. A guard outside the scope would poison only after
                // the join, which is the hang.
                let _panic_guard = KeeperPanicGuard { keeper };
                scope.spawn(move || {
                    let mut owner = activate(stale_permit, heap, STALE_OWNER);
                    let stale = retry_native("publish stale", || owner.publish_native_owner(&[]));
                    retire(owner);
                    stale_tx.send(stale).expect("send the stale receipt");
                });
                scope.spawn(move || {
                    let mut owner = activate(current_permit, heap, CURRENT_OWNER);
                    let superseded = retry_native("publish first generation", || {
                        owner.publish_native_owner(&[])
                    });
                    retry_native("leave native", || owner.leave_native().map(|_| ()));
                    ready_tx
                        .send(())
                        .expect("the owner is running and about to wait");
                    let waited = std::time::Instant::now();
                    let epoch = match owner.wait_request_for(Duration::from_secs(30)) {
                        Ok(epoch) => epoch,
                        Err(RendezvousError::Busy)
                            if waited.elapsed() >= Duration::from_secs(30) =>
                        {
                            panic!("owner wait exceeded 30s");
                        }
                        Err(reason) => {
                            panic!("the product wait observes the keeper stop: {reason:?}")
                        }
                    };
                    pre_tx
                        .send(())
                        .expect("the worker is between the wait and the publish");
                    go_rx.recv().expect("the checker released the publish");
                    let current =
                        retry_native("publish the stop ack", || owner.publish_native_owner(&[]));
                    ack_tx
                        .send((epoch, superseded, current))
                        .expect("send the stop ack");
                    release_rx
                        .recv_timeout(Duration::from_secs(30))
                        .expect("the keeper releases the owner");
                    retire(owner);
                });
                scope.spawn(move || {
                    pre_rx
                        .recv()
                        .expect("the worker reached the pre-publish check");
                    assert!(
                        !checker_flag.load(Ordering::Acquire),
                        "stop returned before the native publish"
                    );
                    go_tx.send(()).expect("release the publish");
                });
                let stale = stale_rx.recv().expect("stale owner published");
                ready_rx.recv().expect("current owner is waiting");
                let queued = retry_native("queue one serial request ahead of the stop", || {
                    keeper.enqueue_serial_for_test(current_key, SERIAL_GENERATION)
                });
                let mut stopped = keeper
                    .stop()
                    .expect("the keeper stop waits for the native publish");
                stop_returned.store(true, Ordering::Release);
                let (epoch, superseded, current) = ack_rx.recv().expect("stop ack");
                let acked = stopped.epoch();
                assert_eq!(
                    epoch, acked,
                    "the worker's wait and the keeper stop share one epoch"
                );
                // Only the keeper's own grant call issues a turn, inside its
                // traced stop (L6.7 K3); opening the stop grants nothing.
                let (len, head) = keeper
                    .serial_ring_head_for_test()
                    .expect("read the ring while the stop is open");
                assert_eq!((len, head), (1, Some(queued)), "the stop keeps the head");
                stopped
                    .trace(|_stopped, _proof| Ok(()))
                    .expect("control trace");
                with_stop_scope(|scope| {
                    let bound = StoppedKeeper::from_scope(scope, stopped).unwrap_or_else(
                        |(_scope, stopped, reason)| {
                            drop(stopped);
                            panic!("bind the real stop: {reason:?}");
                        },
                    );
                    match bound.ready_request(stale) {
                        Err(RendezvousError::Publication) => {}
                        Err(reason) => panic!("stale owner refused with {reason:?}"),
                        Ok(_) => panic!("a retired receipt was accepted"),
                    }
                    match bound.ready_request(superseded) {
                        Err(RendezvousError::Generation) => {}
                        Err(reason) => panic!("wrong generation refused with {reason:?}"),
                        Ok(_) => panic!("a superseded receipt was accepted"),
                    }
                    let request = match bound.ready_request(current) {
                        Ok(request) => request,
                        Err(reason) => panic!("current owner refused: {reason:?}"),
                    };
                    let suspended = match bound.lend_serial(request) {
                        Ok(suspended) => suspended,
                        Err(failure) => {
                            let _ = failure.complete_traced();
                            panic!("lend the current owner");
                        }
                    };
                    let (ticket, held) =
                        super::super::super::serial_ticket::StoppedSerialTicket::from_lend(
                            suspended,
                        );
                    let cut = ticket.identity();
                    assert_eq!(cut.owner, CURRENT_OWNER);
                    assert_eq!(cut.heap, heap);
                    assert_eq!(cut.epoch, acked, "the ticket carries this stop's epoch");
                    let mut wrong = cut;
                    wrong.epoch = std::num::NonZeroU64::new(cut.epoch.get().saturating_add(1))
                        .expect("a later epoch");
                    let (held, wrong_return) = match held.accept_return(
                        super::super::super::keeper_custody::ReturnedSerialTicket::for_test(wrong),
                    ) {
                        Err(refused) => refused,
                        Ok(_) => panic!("a different epoch is not the ticket this lend cut"),
                    };
                    assert_eq!(wrong_return.identity(), wrong);
                    let mut seat = super::super::super::serial_ticket::SerialDestination::new(cut);
                    let returned = seat
                        .receive_serial(ticket)
                        .expect("receive the cut ticket")
                        .guard()
                        .finish(&mut seat)
                        .expect("end the turn")
                        .return_ticket();
                    // The export reservation is held and inert. Cutting the
                    // ticket consumed the lend typestate and did not drop the
                    // stop. A second stop cannot mint another lend.
                    assert!(
                        matches!(keeper.stop(), Err(RendezvousError::Busy)),
                        "a second stop is refused while the lend is outstanding"
                    );
                    let restored = match held.accept_return(returned) {
                        Ok(restored) => restored,
                        Err((_held, rejected)) => {
                            panic!(
                                "the cut ticket was refused: epoch {}",
                                rejected.identity().epoch
                            );
                        }
                    };
                    restored
                        .complete_traced()
                        .expect("complete the traced epoch after the ticket returned");
                });
                // The lend above never granted the probe request: a turn is
                // granted only by the keeper's grant call inside its stop
                // (L6.7 K3; the Idle grant was deleted at L6.7b b2). It is
                // still queued after the epoch, and retires once.
                let (len, head) = keeper
                    .serial_ring_head_for_test()
                    .expect("read the ring after the epoch");
                assert_eq!((len, head), (1, Some(queued)), "the probe is still queued");
                keeper
                    .retire_serial_head_for_test(queued)
                    .expect("retire the queued probe request");
                let mut reopened = keeper
                    .stop()
                    .expect("the same keeper stops again after the lend");
                assert!(
                    reopened.epoch() > acked,
                    "the reopen has its own later epoch"
                );
                reopened
                    .trace(|_stopped, _proof| Ok(()))
                    .expect("reopen trace");
                reopened.complete().expect("reopen complete");
                release_tx.send(()).expect("release the owner");
            });
        });
        match outcome {
            Ok((end, ())) => tx.send(Ok(end)).expect("send keeper end"),
            Err(failure) => tx.send(Err(failure.notice())).expect("send keeper notice"),
        }
    });
    let outcome = rx.recv().expect("keeper outcome");
    service.join().expect("keeper service thread");
    let end = outcome.unwrap_or_else(|notice| panic!("live lend refused: {notice:?}"));
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
    let returned = shell.accept_return(end).unwrap();
    finish_fixture(returned, private);
}

#[test]
fn pentry_l61_keeper_service_refuses_a_poisoned_start_with_a_notice() {
    use super::super::keeper_service::{KeeperStartNotice, keeper_start};
    use neovm_value::tagged::gc::heap_transfer::ScopedEraError;
    let source = ready();
    let alive = source.world_lifetime_probe_for_test();
    let (shell, start) = close(source).prepare_era().unwrap();
    drop(shell);
    let (tx, rx) = std::sync::mpsc::channel();
    let service = std::thread::spawn(move || {
        let notice = match keeper_start(start) {
            Ok(_) => None,
            Err(failure) => Some(failure.notice()),
        };
        tx.send(notice).expect("send keeper notice");
    });
    let notice = rx
        .recv()
        .expect("notice")
        .expect("a poisoned start is refused");
    service.join().expect("keeper service thread");
    assert_eq!(
        notice,
        KeeperStartNotice::Scoped(ScopedEraError::Transfer(TransferError::Phase)),
        "only a send notice crosses; the confined failure stays on the thread"
    );
    assert!(alive(), "the poisoned start retains its backing");
    assert!(!neovm_value::tagged::gc::tagged_heap_is_installed());
}

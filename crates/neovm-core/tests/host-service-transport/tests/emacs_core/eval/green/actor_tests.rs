//! Synchronization orders actor progress; wall-clock sleeps are never oracles.
use super::HostRequestKey;
use super::transport::*;
use crate::host_effect::contracts::OwnedNativeData;
use std::num::NonZeroU64;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::Duration;

const SAFETY_BOUND: Duration = Duration::from_secs(30);

fn key(value: u64) -> HostRequestKey {
    HostRequestKey::for_test(NonZeroU64::new(value).expect("nonzero test identity"))
}

fn returned() -> NativeOutcome {
    NativeOutcome::Reply(NativeReply::Returned(OwnedNativeData::Text(
        "committed".into(),
    )))
}

fn publish(client: &HostClient, request: HostRequestKey) -> HostWaiter {
    match client.publish(request, OwnedNativeData::Text("request".into())) {
        Publication::Delivered(waiter) => waiter,
        Publication::ReceiverClosed { .. } => panic!("live test service rejected publication"),
    }
}

/// Only this native service actor consumes completion and terminal receipts.
/// These are transport-metadata acknowledgements, not heap/root proofs.
fn consume_and_acknowledge(
    actor: &mut ServiceTransport,
    request: HostRequestKey,
) -> NativeSettlement {
    let (receipt, settlement) = match actor.take_terminal(request) {
        TerminalRead::Ready {
            receipt,
            settlement,
        } => (receipt, settlement),
        TerminalRead::Pending | TerminalRead::AlreadyObserved | TerminalRead::UnknownRequest => {
            panic!("expected one durable terminal")
        }
    };
    assert!(matches!(
        actor.take_terminal(request),
        TerminalRead::AlreadyObserved
    ));
    match actor.acknowledge(receipt) {
        Acknowledgement::Retired(retired) => assert_eq!(retired, request),
        Acknowledgement::Rejected(_) => panic!("matching observed receipt rejected"),
    }
    assert!(matches!(
        actor.take_terminal(request),
        TerminalRead::UnknownRequest
    ));
    settlement
}

#[test]
fn separate_os_actors_race_cancel_and_close_after_dispatch_without_replaying_effect() {
    let (client, mut actor) = channel();
    let request = key(1);
    let committed = Arc::new(AtomicUsize::new(0));
    let native_and_controls = Arc::new(Barrier::new(3));
    let (published_tx, published_rx) = mpsc::channel();
    let (observed_tx, observed_rx) = mpsc::channel();
    let (owner_finish_tx, owner_finish_rx) = mpsc::channel();
    let (native_tx, native_rx) = mpsc::channel::<NativeDispatch>();
    let (completion_tx, completion_rx) = mpsc::channel();
    let (control_tx, control_rx) = mpsc::channel();

    let publisher_client = client.clone();
    let publisher = thread::spawn(move || {
        let waiter = publish(&publisher_client, request);
        published_tx.send(()).unwrap();
        assert_eq!(
            waiter.wait(),
            WaitObservation::Notified(SettlementNotification::Settled(request))
        );
        observed_tx.send(()).unwrap();
        owner_finish_rx.recv_timeout(SAFETY_BOUND).unwrap();
        waiter.finish();
    });

    let native_barrier = Arc::clone(&native_and_controls);
    let native_committed = Arc::clone(&committed);
    let native = thread::spawn(move || {
        let dispatch = native_rx.recv_timeout(SAFETY_BOUND).unwrap();
        assert_eq!(dispatch.key(), request);
        let (ticket, payload) = dispatch.into_parts();
        assert_eq!(payload, OwnedNativeData::Text("request".into()));
        native_barrier.wait();
        assert_eq!(native_committed.fetch_add(1, Ordering::SeqCst), 0);
        completion_tx.send(ticket.settle(returned())).unwrap();
    });

    let cancel_barrier = Arc::clone(&native_and_controls);
    let cancel_client = client.clone();
    let cancel_done = control_tx.clone();
    let cancel = thread::spawn(move || {
        cancel_barrier.wait();
        assert_eq!(
            cancel_client.cancel(request, CancelReason::TargetSignal),
            DeliveryStatus::Delivered
        );
        cancel_done.send(()).unwrap();
    });

    let close_barrier = Arc::clone(&native_and_controls);
    let close_client = client.clone();
    let close = thread::spawn(move || {
        close_barrier.wait();
        assert_eq!(close_client.close(), DeliveryStatus::Delivered);
        control_tx.send(()).unwrap();
    });

    let service = thread::spawn(move || {
        published_rx.recv_timeout(SAFETY_BOUND).unwrap();
        let dispatch = match actor.dispatch_next() {
            DispatchPoll::Ready(dispatch) => dispatch,
            DispatchPoll::Idle => panic!("published request was not dispatched"),
        };
        native_tx.send(dispatch).unwrap();
        // Both control sends are committed before the sole actor completes.
        // Their relative order intentionally races on the actual FIFO.
        control_rx.recv_timeout(SAFETY_BOUND).unwrap();
        control_rx.recv_timeout(SAFETY_BOUND).unwrap();
        let completion = completion_rx.recv_timeout(SAFETY_BOUND).unwrap();
        match actor.complete(completion) {
            CompletionDisposition::Settled(settled) => assert_eq!(settled, request),
            CompletionDisposition::Rejected { .. } => panic!("single native completion rejected"),
        }
        observed_rx.recv_timeout(SAFETY_BOUND).unwrap();
        let settlement = consume_and_acknowledge(&mut actor, request);
        match settlement {
            NativeSettlement::CompletedAfterCancellation {
                outcome,
                cancellation,
            } => {
                assert_eq!(outcome, returned());
                match cancellation {
                    CancelReason::TargetSignal | CancelReason::SessionClosing => {}
                    CancelReason::TargetQuit | CancelReason::OwnerEnded => {
                        panic!("foreign cancellation cause")
                    }
                }
            }
            NativeSettlement::Completed(_) => panic!("both controls preceded completion"),
        }
        assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
        assert_eq!(actor.close_and_drain(), CloseProgress::Drained);
        owner_finish_tx.send(()).unwrap();
    });

    service.join().unwrap();
    publisher.join().unwrap();
    native.join().unwrap();
    cancel.join().unwrap();
    close.join().unwrap();
    assert_eq!(committed.load(Ordering::SeqCst), 1);
}

#[test]
fn separate_actors_cancel_before_dispatch_and_close_without_native_execution() {
    let (client, mut actor) = channel();
    let request = key(2);
    let (published_tx, published_rx) = mpsc::channel();
    let (cancelled_tx, cancelled_rx) = mpsc::channel();
    let (closed_tx, closed_rx) = mpsc::channel();
    let (observed_tx, observed_rx) = mpsc::channel();
    let (retired_tx, retired_rx) = mpsc::channel();

    let publisher_client = client.clone();
    let publisher = thread::spawn(move || {
        let waiter = publish(&publisher_client, request);
        published_tx.send(()).unwrap();
        assert_eq!(
            waiter.wait(),
            WaitObservation::Notified(SettlementNotification::Settled(request))
        );
        observed_tx.send(()).unwrap();
        retired_rx.recv_timeout(SAFETY_BOUND).unwrap();
        waiter.finish();
    });

    let cancel_client = client.clone();
    let cancel = thread::spawn(move || {
        published_rx.recv_timeout(SAFETY_BOUND).unwrap();
        assert_eq!(
            cancel_client.cancel(request, CancelReason::TargetQuit),
            DeliveryStatus::Delivered
        );
        cancelled_tx.send(()).unwrap();
    });
    let close = thread::spawn(move || {
        cancelled_rx.recv_timeout(SAFETY_BOUND).unwrap();
        assert_eq!(client.close(), DeliveryStatus::Delivered);
        closed_tx.send(()).unwrap();
    });
    let service = thread::spawn(move || {
        closed_rx.recv_timeout(SAFETY_BOUND).unwrap();
        assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
        observed_rx.recv_timeout(SAFETY_BOUND).unwrap();
        let settlement = consume_and_acknowledge(&mut actor, request);
        assert_eq!(
            settlement,
            NativeSettlement::Completed(NativeOutcome::Interrupted(CancelReason::TargetQuit))
        );
        assert_eq!(actor.close_and_drain(), CloseProgress::Drained);
        retired_tx.send(()).unwrap();
    });

    service.join().unwrap();
    publisher.join().unwrap();
    cancel.join().unwrap();
    close.join().unwrap();
}

use super::*;
use std::num::NonZeroU64;

fn key(request: u64) -> HostRequestKey {
    HostRequestKey::for_test(NonZeroU64::new(request).expect("nonzero fixture request"))
}

fn publish(client: &HostClient, request: HostRequestKey) -> HostWaiter {
    match client.publish(request, OwnedNativeData::Text("request".into())) {
        Publication::Delivered(waiter) => waiter,
        Publication::ReceiverClosed { .. } => panic!("fixture actor is alive"),
    }
}

fn dispatch(actor: &mut ServiceTransport) -> NativeDispatch {
    match actor.dispatch_next() {
        DispatchPoll::Ready(dispatch) => dispatch,
        DispatchPoll::Idle => panic!("fixture has queued native work"),
    }
}

fn returned() -> NativeOutcome {
    NativeOutcome::Reply(NativeReply::Returned(OwnedNativeData::Text("reply".into())))
}

fn finish(actor: &mut ServiceTransport, dispatch: NativeDispatch, outcome: NativeOutcome) {
    let (ticket, _) = dispatch.into_parts();
    assert!(matches!(
        actor.complete(ticket.settle(outcome)),
        CompletionDisposition::Settled(_)
    ));
}

fn terminal(
    actor: &mut ServiceTransport,
    key: HostRequestKey,
) -> (TerminalReceipt, NativeSettlement) {
    match actor.take_terminal(key) {
        TerminalRead::Ready {
            receipt,
            settlement,
        } => (receipt, settlement),
        TerminalRead::Pending | TerminalRead::AlreadyObserved | TerminalRead::UnknownRequest => {
            panic!("fixture expects a previously unobserved terminal reply")
        }
    }
}

#[test]
fn native_dispatch_is_fifo_and_at_most_once() {
    let (client, mut actor) = channel();
    let first = publish(&client, key(1));
    let second = publish(&client, key(2));
    let first_dispatch = dispatch(&mut actor);
    assert_eq!(first_dispatch.key(), first.key());
    let second_dispatch = dispatch(&mut actor);
    assert_eq!(second_dispatch.key(), second.key());
    assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
    finish(&mut actor, second_dispatch, returned());
    finish(&mut actor, first_dispatch, returned());
    assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
    assert_eq!(
        first.observe(),
        WaitObservation::Notified(SettlementNotification::Settled(key(1)))
    );
    assert_eq!(
        second.observe(),
        WaitObservation::Notified(SettlementNotification::Settled(key(2)))
    );
}

#[test]
fn durable_terminal_precedes_notification_and_survives_observation_until_ack() {
    let (client, mut actor) = channel();
    let waiter = publish(&client, key(1));
    assert_eq!(waiter.observe(), WaitObservation::Pending);
    let work = dispatch(&mut actor);
    finish(&mut actor, work, returned());
    assert_eq!(
        waiter.wait(),
        WaitObservation::Notified(SettlementNotification::Settled(key(1)))
    );
    let (receipt, settlement) = terminal(&mut actor, key(1));
    assert_eq!(receipt.key(), key(1));
    assert_eq!(settlement, NativeSettlement::Completed(returned()));
    assert!(matches!(
        actor.take_terminal(key(1)),
        TerminalRead::AlreadyObserved
    ));
    // A consumed reply still blocks duplicate admission until registry retirement.
    let duplicate = publish(&client, key(1));
    assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
    assert_eq!(
        duplicate.observe(),
        WaitObservation::Notified(SettlementNotification::Rejected(
            key(1),
            PublicationRejection::DuplicateKey
        ))
    );
    assert!(
        matches!(actor.acknowledge(receipt), Acknowledgement::Retired(value) if value == key(1))
    );
    assert!(matches!(
        actor.take_terminal(key(1)),
        TerminalRead::UnknownRequest
    ));
    waiter.finish();
}

#[test]
fn cancellation_before_dispatch_suppresses_the_native_effect() {
    let (client, mut actor) = channel();
    let waiter = publish(&client, key(1));
    assert_eq!(
        client.cancel(key(1), CancelReason::TargetQuit),
        DeliveryStatus::Delivered
    );
    assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
    assert!(matches!(
        actor.next_observation(),
        Some(ControlObservation::Cancellation {
            key: value,
            reason: CancelReason::TargetQuit,
            resolution: CancelResolution::SuppressedBeforeDispatch,
        }) if value == key(1)
    ));
    let (_, settlement) = terminal(&mut actor, key(1));
    assert_eq!(
        settlement,
        NativeSettlement::Completed(NativeOutcome::Interrupted(CancelReason::TargetQuit))
    );
    assert_eq!(
        waiter.wait(),
        WaitObservation::Notified(SettlementNotification::Settled(key(1)))
    );
}

#[test]
fn cancellation_after_dispatch_waits_for_native_settlement_and_preserves_effect_result() {
    let (client, mut actor) = channel();
    let waiter = publish(&client, key(1));
    let work = dispatch(&mut actor);
    assert_eq!(
        client.cancel(key(1), CancelReason::TargetSignal),
        DeliveryStatus::Delivered
    );
    actor.drain_commands();
    assert!(matches!(actor.take_terminal(key(1)), TerminalRead::Pending));
    assert_eq!(waiter.observe(), WaitObservation::Pending);
    assert!(matches!(
        actor.next_observation(),
        Some(ControlObservation::Cancellation {
            resolution: CancelResolution::AwaitNativeSettlement,
            ..
        })
    ));
    finish(&mut actor, work, returned());
    let (_, settlement) = terminal(&mut actor, key(1));
    assert_eq!(
        settlement,
        NativeSettlement::CompletedAfterCancellation {
            outcome: returned(),
            cancellation: CancelReason::TargetSignal,
        }
    );
    assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
}

#[test]
fn cancellation_and_completion_linearize_once_in_either_order() {
    for order in [CancelOrder::BeforeCompletion, CancelOrder::AfterCompletion] {
        let (client, mut actor) = channel();
        let _waiter = publish(&client, key(1));
        let work = dispatch(&mut actor);
        match order {
            CancelOrder::BeforeCompletion => {
                client.cancel(key(1), CancelReason::TargetQuit);
                finish(&mut actor, work, returned());
            }
            CancelOrder::AfterCompletion => {
                finish(&mut actor, work, returned());
                client.cancel(key(1), CancelReason::TargetQuit);
                actor.drain_commands();
            }
        }
        let expected = match order {
            CancelOrder::BeforeCompletion => CancelResolution::AwaitNativeSettlement,
            CancelOrder::AfterCompletion => CancelResolution::AlreadyTerminal,
        };
        assert!(
            matches!(actor.next_observation(), Some(ControlObservation::Cancellation {
            resolution,
            ..
        }) if resolution == expected)
        );
        let _ = terminal(&mut actor, key(1));
        assert!(matches!(
            actor.take_terminal(key(1)),
            TerminalRead::AlreadyObserved
        ));
        assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
    }
}

enum CancelOrder {
    BeforeCompletion,
    AfterCompletion,
}

#[test]
fn repeated_cancellation_keeps_the_first_pending_target_reason() {
    let (client, mut actor) = channel();
    let _waiter = publish(&client, key(1));
    let work = dispatch(&mut actor);
    client.cancel(key(1), CancelReason::TargetSignal);
    client.cancel(key(1), CancelReason::TargetQuit);
    finish(&mut actor, work, returned());
    let (_, settlement) = terminal(&mut actor, key(1));
    assert_eq!(
        settlement,
        NativeSettlement::CompletedAfterCancellation {
            outcome: returned(),
            cancellation: CancelReason::TargetSignal,
        }
    );
}

#[test]
fn dropped_waiter_posts_metadata_cancellation_without_waiting() {
    let (client, mut actor) = channel();
    let waiter = publish(&client, key(1));
    drop(waiter);
    assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
    assert!(matches!(
        actor.next_observation(),
        Some(ControlObservation::Cancellation {
            reason: CancelReason::OwnerEnded,
            resolution: CancelResolution::SuppressedBeforeDispatch,
            ..
        })
    ));
    let (_, settlement) = terminal(&mut actor, key(1));
    assert_eq!(
        settlement,
        NativeSettlement::Completed(NativeOutcome::Interrupted(CancelReason::OwnerEnded))
    );
}

#[test]
fn dropping_a_rejected_duplicate_does_not_cancel_the_original_waiter() {
    let (client, mut actor) = channel();
    let _original = publish(&client, key(1));
    let work = dispatch(&mut actor);
    let rejected = publish(&client, key(1));
    actor.drain_commands();
    assert!(matches!(
        actor.next_observation(),
        Some(ControlObservation::RejectedPublication { .. })
    ));
    drop(rejected);
    actor.drain_commands();
    assert!(matches!(
        actor.next_observation(),
        Some(ControlObservation::Cancellation {
            reason: CancelReason::OwnerEnded,
            resolution: CancelResolution::UnknownRequest,
            ..
        })
    ));
    finish(&mut actor, work, returned());
    let (_, settlement) = terminal(&mut actor, key(1));
    assert_eq!(settlement, NativeSettlement::Completed(returned()));
}

#[test]
fn closing_suppresses_queued_work_but_retains_inflight_work_until_settlement() {
    let (client, mut actor) = channel();
    let running_waiter = publish(&client, key(1));
    let work = dispatch(&mut actor);
    let queued_waiter = publish(&client, key(2));
    assert_eq!(client.close(), DeliveryStatus::Delivered);
    assert_eq!(
        actor.close_and_drain(),
        CloseProgress::AwaitingNativeSettlement(vec![key(1)])
    );
    assert_eq!(running_waiter.observe(), WaitObservation::Pending);
    assert_eq!(
        queued_waiter.wait(),
        WaitObservation::Notified(SettlementNotification::Settled(key(2)))
    );
    let (_, queued) = terminal(&mut actor, key(2));
    assert_eq!(
        queued,
        NativeSettlement::Completed(NativeOutcome::HostClosed)
    );
    finish(&mut actor, work, returned());
    assert_eq!(actor.close_and_drain(), CloseProgress::Drained);
    let (_, running) = terminal(&mut actor, key(1));
    assert_eq!(
        running,
        NativeSettlement::CompletedAfterCancellation {
            outcome: returned(),
            cancellation: CancelReason::SessionClosing,
        }
    );
    assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
}

#[test]
fn publication_after_close_is_durably_closed_and_never_dispatched() {
    let (client, mut actor) = channel();
    client.close();
    let waiter = publish(&client, key(1));
    assert!(matches!(actor.dispatch_next(), DispatchPoll::Idle));
    assert_eq!(
        waiter.wait(),
        WaitObservation::Notified(SettlementNotification::Settled(key(1)))
    );
    let (_, settlement) = terminal(&mut actor, key(1));
    assert_eq!(
        settlement,
        NativeSettlement::Completed(NativeOutcome::HostClosed)
    );
}

#[test]
fn disconnected_service_returns_receiver_closed_without_an_enqueue_retry() {
    let (client, actor) = channel();
    drop(actor);
    assert!(
        matches!(client.publish(key(1), OwnedNativeData::None), Publication::ReceiverClosed { key: value } if value == key(1))
    );
    assert_eq!(
        client.cancel(key(1), CancelReason::OwnerEnded),
        DeliveryStatus::ReceiverClosed
    );
    assert_eq!(client.close(), DeliveryStatus::ReceiverClosed);
}

#[test]
fn accepted_waiter_observes_service_death_without_self_retaining_a_notifier() {
    let (client, mut actor) = channel();
    let waiter = publish(&client, key(1));
    actor.drain_commands();
    drop(actor);
    assert_eq!(waiter.observe(), WaitObservation::ChannelClosed);
    assert_eq!(waiter.wait(), WaitObservation::ChannelClosed);
}

#[test]
fn unknown_cancellation_and_duplicate_completion_do_not_create_terminal_records() {
    let (client, mut actor) = channel();
    client.cancel(key(1), CancelReason::OwnerEnded);
    actor.drain_commands();
    assert!(matches!(
        actor.next_observation(),
        Some(ControlObservation::Cancellation {
            resolution: CancelResolution::UnknownRequest,
            ..
        })
    ));
    assert!(matches!(
        actor.take_terminal(key(1)),
        TerminalRead::UnknownRequest
    ));
    let _waiter = publish(&client, key(1));
    let work = dispatch(&mut actor);
    finish(&mut actor, work, returned());
    // Malicious duplicate-completion mutation: production cannot mint this.
    let duplicate = NativeCompletion {
        key: key(1),
        outcome: NativeOutcome::HostClosed,
    };
    assert!(matches!(
        actor.complete(duplicate),
        CompletionDisposition::Rejected {
            reason: CompletionRejection::AlreadyTerminal,
            ..
        }
    ));
    let (_, settlement) = terminal(&mut actor, key(1));
    assert_eq!(settlement, NativeSettlement::Completed(returned()));
}

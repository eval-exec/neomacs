use super::*;
use crate::host_effect::contracts::{OwnedNativeData, ReadDeadline};
use crate::host_effect::native_outcome::NativeReply;
use crate::host_effect::wake::{ControlWake, ControlWakeOutcome};
use std::num::NonZeroU64;
use std::time::Duration;

fn key(value: u64) -> HostRequestKey {
    HostRequestKey::for_test(NonZeroU64::new(value).unwrap())
}

/// The ordinary control source and independent paired settlement source are
/// both retained by this immutable route. Tests may keep either owner to arm it.
fn route(primary: ControlNudgePublisher) -> NativeCompletionRoute {
    let (_settlement_owner, settlement) = ControlWake::paired();
    NativeCompletionRoute::new(primary, settlement)
}

fn returned(text: &str) -> NativeOutcome {
    NativeOutcome::Reply(NativeReply::Returned(OwnedNativeData::Text(text.into())))
}

fn complete(operation: &mut PendingNativeOperation) -> (HostRequestKey, NativeOutcome) {
    match operation.observe() {
        NativePendingObservation::Complete(completion) => completion.into_parts(),
        other => panic!("expected owned native completion: {other:?}"),
    }
}

#[test]
fn native_completion_is_committed_before_value_free_wake() {
    let (wake, publisher) = ControlWake::paired();
    let (mut operation, native) = pair(key(1), ReadDeadline::Unbounded, route(publisher));
    let armed = wake.arm(ControlDestination::NativeMain).unwrap();
    assert!(matches!(
        native.complete(returned("ready")),
        NativePublication::Committed(_)
    ));
    assert_eq!(
        armed.poll(),
        Some(ControlWakeOutcome::ControlReady(
            ControlNudge::HostServiceReady
        ))
    );
    assert_eq!(complete(&mut operation), (key(1), returned("ready")));
    assert!(matches!(
        operation.observe(),
        NativePendingObservation::AlreadyObserved
    ));
}

#[test]
fn out_of_order_completions_keep_their_original_request_keys() {
    let (_wake, publisher) = ControlWake::paired();
    let (mut first, first_native) = pair(key(1), ReadDeadline::Unbounded, route(publisher.clone()));
    let (mut second, second_native) = pair(key(2), ReadDeadline::Unbounded, route(publisher));
    second_native.complete(returned("B"));
    assert!(matches!(first.observe(), NativePendingObservation::Pending));
    assert_eq!(complete(&mut second), (key(2), returned("B")));
    first_native.complete(returned("A"));
    assert_eq!(complete(&mut first), (key(1), returned("A")));
}

#[test]
fn cancellation_requires_actual_native_acknowledgement() {
    let (_wake, publisher) = ControlWake::paired();
    let deadline = ReadDeadline::At(Instant::now() + Duration::from_secs(60));
    let (mut operation, native) = pair(key(1), deadline, route(publisher));
    assert_eq!(
        operation.request_cancel(CancelReason::TargetSignal),
        NativeCancelProgress::Delivered
    );
    assert_eq!(operation.deadline(), deadline);
    assert_eq!(operation.next_wait_deadline(), ReadDeadline::Unbounded);
    assert!(matches!(
        operation.observe(),
        NativePendingObservation::Pending
    ));
    assert_eq!(
        native.cancellation(),
        NativeCancellationObservation::Requested(NativeCancellationRequest::Owner(
            CancelReason::TargetSignal
        ))
    );
    assert_eq!(
        operation.request_cancel(CancelReason::TargetQuit),
        NativeCancelProgress::AlreadyAwaitingSettlement
    );
    native.complete(NativeOutcome::Interrupted(CancelReason::TargetSignal));
    assert_eq!(
        complete(&mut operation),
        (
            key(1),
            NativeOutcome::Interrupted(CancelReason::TargetSignal)
        )
    );
}

#[test]
fn expired_absolute_deadline_requests_once_then_waits_for_native_settlement() {
    let (_wake, publisher) = ControlWake::paired();
    let start = Instant::now();
    let end = start + Duration::from_secs(60);
    let (mut operation, native) = pair(key(1), ReadDeadline::At(end), route(publisher));
    assert_eq!(
        operation.expire_deadline(start),
        NativeDeadlineProgress::NotElapsed
    );
    assert_eq!(operation.next_wait_deadline(), ReadDeadline::At(end));
    assert_eq!(
        operation.expire_deadline(end),
        NativeDeadlineProgress::Cancellation(NativeCancelProgress::Delivered)
    );
    assert_eq!(operation.next_wait_deadline(), ReadDeadline::Unbounded);
    assert_eq!(operation.deadline(), ReadDeadline::At(end));
    assert_eq!(
        native.cancellation(),
        NativeCancellationObservation::Requested(NativeCancellationRequest::DeadlineElapsed)
    );
    assert!(matches!(
        operation.observe(),
        NativePendingObservation::Pending
    ));
    assert_eq!(
        operation.expire_deadline(end),
        NativeDeadlineProgress::Cancellation(NativeCancelProgress::AlreadyAwaitingSettlement)
    );
    // Deadline/cancellation is not a fabricated native terminal. Only dropping
    // the real producer makes this next observation Closed.
    drop(native);
    assert!(
        matches!(operation.observe(), NativePendingObservation::Closed(closed) if closed.key() == key(1))
    );
}

#[test]
fn producer_disconnect_is_durable_before_its_drop_nudge() {
    let (wake, publisher) = ControlWake::paired();
    let (mut operation, native) = pair(key(1), ReadDeadline::Unbounded, route(publisher));
    let armed = wake.arm(ControlDestination::NativeMain).unwrap();
    drop(native);
    assert!(armed.poll().is_some());
    assert!(
        matches!(operation.observe(), NativePendingObservation::Closed(closed) if closed.key() == key(1))
    );
    assert!(matches!(
        operation.observe(),
        NativePendingObservation::AlreadyObserved
    ));
}

#[test]
fn cancellation_after_native_commit_preserves_the_committed_outcome() {
    let (_wake, publisher) = ControlWake::paired();
    let (mut operation, native) = pair(key(1), ReadDeadline::Unbounded, route(publisher));
    native.complete(returned("already committed"));
    assert_eq!(
        operation.request_cancel(CancelReason::SessionClosing),
        NativeCancelProgress::ProducerClosed
    );
    assert_eq!(
        complete(&mut operation),
        (key(1), returned("already committed"))
    );
}

#[test]
fn receiver_closure_returns_unconsumed_completion_without_replay_authority() {
    let (_wake, publisher) = ControlWake::paired();
    let (operation, native) = pair(key(1), ReadDeadline::Unbounded, route(publisher));
    drop(operation);
    match native.complete(returned("not enqueued")) {
        NativePublication::ReceiverClosed { completion, .. } => {
            assert_eq!(completion.into_parts(), (key(1), returned("not enqueued")))
        }
        NativePublication::Committed(_) => panic!("closed receiver accepted completion"),
    }
}

#[test]
fn primary_close_cannot_erase_settlement_completion_readiness() {
    let (primary, primary_publisher) = ControlWake::paired();
    let (settlement, settlement_publisher) = ControlWake::paired();
    let route = NativeCompletionRoute::new(primary_publisher, settlement_publisher);
    let (mut operation, native) = pair(key(1), ReadDeadline::Unbounded, route);
    let waiting = settlement.arm(ControlDestination::NativeMain).unwrap();
    primary.retire();
    match native.complete(returned("settled after primary close")) {
        NativePublication::Committed(NativeCompletionNotification::Published {
            primary: ControlNudgeOutcome::RetiredSession,
            settlement: ControlNudgeOutcome::Notified,
        }) => {}
        other => panic!("completion must notify the independent settlement route: {other:?}"),
    }
    assert_eq!(
        waiting.wait(ReadDeadline::Immediate),
        ControlWakeOutcome::ControlReady(ControlNudge::HostServiceReady)
    );
    assert_eq!(
        complete(&mut operation),
        (key(1), returned("settled after primary close"))
    );
}

#[test]
fn primary_close_cannot_erase_settlement_producer_disconnect_readiness() {
    let (primary, primary_publisher) = ControlWake::paired();
    let (settlement, settlement_publisher) = ControlWake::paired();
    let route = NativeCompletionRoute::new(primary_publisher, settlement_publisher);
    let (mut operation, native) = pair(key(1), ReadDeadline::Unbounded, route);
    let waiting = settlement.arm(ControlDestination::NativeMain).unwrap();
    primary.retire();
    drop(native);
    assert_eq!(
        waiting.wait(ReadDeadline::Immediate),
        ControlWakeOutcome::ControlReady(ControlNudge::HostServiceReady)
    );
    assert!(
        matches!(operation.observe(), NativePendingObservation::Closed(closed) if closed.key() == key(1))
    );
}

#[test]
fn native_producer_waits_for_the_actual_cancellation_request_before_acknowledging() {
    let (_wake, publisher) = ControlWake::paired();
    let (mut operation, native) = pair(key(1), ReadDeadline::Unbounded, route(publisher));
    let producer = std::thread::spawn(move || {
        assert_eq!(
            native.wait_cancellation(),
            NativeCancellationObservation::Requested(NativeCancellationRequest::Owner(
                CancelReason::SessionClosing
            ))
        );
        native.complete(returned("actual cancellation acknowledgment"))
    });
    assert_eq!(
        operation.request_cancel(CancelReason::SessionClosing),
        NativeCancelProgress::Delivered
    );
    assert!(matches!(
        producer.join().unwrap(),
        NativePublication::Committed(_)
    ));
    assert_eq!(
        complete(&mut operation),
        (key(1), returned("actual cancellation acknowledgment"))
    );
}

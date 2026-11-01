use super::super::transport::{
    self, Acknowledgement, CloseProgress, CompletionDisposition, DispatchPoll, DispatchTicket,
    NativeSettlement, Publication, TerminalRead,
};
use super::*;
use crate::host_effect::contracts::OwnedNativeData;
use crate::host_effect::native_outcome::NativeReply;
use crate::host_effect::native_pending::{
    self, NativeCancellationObservation, NativeCancellationRequest, NativeCompletionRoute,
    NativePublication,
};
use crate::host_effect::wake::ControlWake;
use std::num::NonZeroU64;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;

struct Reservation {
    key: HostRequestKey,
    ticket: DispatchTicket,
}
impl PendingReservation for Reservation {
    fn key(&self) -> HostRequestKey {
        self.key
    }
}

fn key(value: u64) -> HostRequestKey {
    HostRequestKey::for_test(NonZeroU64::new(value).unwrap())
}
fn returned(value: &str) -> NativeOutcome {
    NativeOutcome::Reply(NativeReply::Returned(OwnedNativeData::Text(value.into())))
}
fn publish(
    client: &transport::HostClient,
    actor: &mut transport::ServiceTransport,
    key: HostRequestKey,
) -> (transport::HostWaiter, Reservation) {
    let waiter = match client.publish(key, OwnedNativeData::None) {
        Publication::Delivered(waiter) => waiter,
        Publication::ReceiverClosed { .. } => panic!("live native actor"),
    };
    let dispatch = match actor.dispatch_next() {
        DispatchPoll::Ready(dispatch) => dispatch,
        DispatchPoll::Idle => panic!("published dispatch"),
    };
    let (ticket, _) = dispatch.into_parts();
    (waiter, Reservation { key, ticket })
}
fn settle(
    actor: &mut transport::ServiceTransport,
    ready: ReadyPending<Reservation>,
) -> NativeSettlement {
    let (key, outcome, reservation) = ready.into_parts();
    assert_eq!(key, reservation.key);
    let NativeReadyOutcome::Completed(outcome) = outcome else {
        panic!("actual native reply")
    };
    assert!(
        matches!(actor.complete(reservation.ticket.settle(outcome)), CompletionDisposition::Settled(k) if k == key)
    );
    let (receipt, settlement) = match actor.take_terminal(key) {
        TerminalRead::Ready {
            receipt,
            settlement,
        } => (receipt, settlement),
        _ => panic!("one durable terminal"),
    };
    assert!(matches!(actor.acknowledge(receipt), Acknowledgement::Retired(k) if k == key));
    settlement
}

#[test]
fn out_of_order_native_results_preserve_linear_actor_reservations() {
    let (client, mut actor) = transport::channel();
    let (_wake, nudge) = ControlWake::paired();
    let nudge = NativeCompletionRoute::new(nudge, ControlWake::paired().1);
    let mut pending = NativePendingCalls::new();
    let (wa, a) = publish(&client, &mut actor, key(1));
    let (wb, b) = publish(&client, &mut actor, key(2));
    let (oa, pa) = native_pending::pair(key(1), ReadDeadline::Unbounded, nudge.clone());
    let (ob, pb) = native_pending::pair(key(2), ReadDeadline::Unbounded, nudge);
    assert!(pending.insert(oa, a).is_ok());
    assert!(pending.insert(ob, b).is_ok());
    pb.complete(returned("B"));
    let mut ready = pending.observe_ready();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].key(), key(2));
    assert_eq!(
        settle(&mut actor, ready.pop().unwrap()),
        NativeSettlement::Completed(returned("B"))
    );
    assert!(matches!(
        pending.drain_progress(),
        NativeDrainProgress::AwaitingNativeSettlement(_)
    ));
    pa.complete(returned("A"));
    assert_eq!(
        settle(&mut actor, pending.observe_ready().pop().unwrap()),
        NativeSettlement::Completed(returned("A"))
    );
    assert_eq!(pending.drain_progress(), NativeDrainProgress::Drained);
    wa.finish();
    wb.finish();
}

#[test]
fn b_native_work_unblocks_a_without_lifo_ownership_or_a_second_lease() {
    let (client, mut actor) = transport::channel();
    let (_wake, nudge) = ControlWake::paired();
    let nudge = NativeCompletionRoute::new(nudge, ControlWake::paired().1);
    let mut pending = NativePendingCalls::new();
    let (wa, a) = publish(&client, &mut actor, key(1));
    let (wb, b) = publish(&client, &mut actor, key(2));
    let (oa, pa) = native_pending::pair(key(1), ReadDeadline::Unbounded, nudge.clone());
    let (ob, pb) = native_pending::pair(key(2), ReadDeadline::Unbounded, nudge);
    assert!(pending.insert(oa, a).is_ok());
    assert!(pending.insert(ob, b).is_ok());
    assert!(pending.observe_ready().is_empty());
    // B's native action sends A's completion; B itself remains pending. This
    // rejects a stack-shaped rule that requires B to finish before A can resume.
    let b_native = thread::spawn(move || pa.complete(returned("A from B")));
    assert!(matches!(
        b_native.join().unwrap(),
        NativePublication::Committed(_)
    ));
    let mut ready = pending.observe_ready();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].key(), key(1));
    assert_eq!(
        settle(&mut actor, ready.pop().unwrap()),
        NativeSettlement::Completed(returned("A from B"))
    );
    assert!(pending.observe_ready().is_empty());
    pb.complete(returned("B later"));
    settle(&mut actor, pending.observe_ready().pop().unwrap());
    assert_eq!(pending.drain_progress(), NativeDrainProgress::Drained);
    wa.finish();
    wb.finish();
}

#[test]
fn deadlines_request_native_cancellation_without_restarting_or_fabricating_a_terminal() {
    let (client, mut actor) = transport::channel();
    let (_wake, nudge) = ControlWake::paired();
    let nudge = NativeCompletionRoute::new(nudge, ControlWake::paired().1);
    let (waiter, reservation) = publish(&client, &mut actor, key(1));
    let end = Instant::now() + Duration::from_secs(60);
    let (operation, producer) = native_pending::pair(key(1), ReadDeadline::At(end), nudge);
    let mut pending = NativePendingCalls::new();
    assert!(pending.insert(operation, reservation).is_ok());
    assert_eq!(pending.next_deadline(), ReadDeadline::At(end));
    assert_eq!(
        pending.expire_deadlines(end),
        [(
            key(1),
            NativeDeadlineProgress::Cancellation(NativeCancelProgress::Delivered)
        )]
    );
    assert_eq!(pending.next_deadline(), ReadDeadline::Unbounded);
    assert!(pending.observe_ready().is_empty());
    assert_eq!(
        producer.cancellation(),
        NativeCancellationObservation::Requested(NativeCancellationRequest::DeadlineElapsed)
    );
    assert!(matches!(
        pending.drain_progress(),
        NativeDrainProgress::AwaitingNativeSettlement(_)
    ));
    producer.complete(returned("native deadline acknowledgment"));
    settle(&mut actor, pending.observe_ready().pop().unwrap());
    waiter.finish();
}

#[test]
fn close_retains_both_native_obligations_until_their_actual_settlement() {
    let (client, mut actor) = transport::channel();
    let (_wake, nudge) = ControlWake::paired();
    let nudge = NativeCompletionRoute::new(nudge, ControlWake::paired().1);
    let (wa, a) = publish(&client, &mut actor, key(1));
    let (wb, b) = publish(&client, &mut actor, key(2));
    let (oa, pa) = native_pending::pair(key(1), ReadDeadline::Unbounded, nudge.clone());
    let (ob, pb) = native_pending::pair(key(2), ReadDeadline::Unbounded, nudge);
    let mut pending = NativePendingCalls::new();
    assert!(pending.insert(oa, a).is_ok());
    assert!(pending.insert(ob, b).is_ok());
    assert!(
        matches!(pending.close(), NativeDrainProgress::AwaitingNativeSettlement(count) if count.get() == 2)
    );
    assert!(
        matches!(actor.close_and_drain(), CloseProgress::AwaitingNativeSettlement(keys) if keys.len() == 2)
    );
    assert!(pending.observe_ready().is_empty());
    for producer in [&pa, &pb] {
        assert_eq!(
            producer.cancellation(),
            NativeCancellationObservation::Requested(NativeCancellationRequest::Owner(
                CancelReason::SessionClosing
            ))
        );
    }
    pb.complete(returned("committed B"));
    assert_eq!(
        settle(&mut actor, pending.observe_ready().pop().unwrap()),
        NativeSettlement::CompletedAfterCancellation {
            outcome: returned("committed B"),
            cancellation: CancelReason::SessionClosing
        }
    );
    assert!(
        matches!(pending.drain_progress(), NativeDrainProgress::AwaitingNativeSettlement(count) if count.get() == 1)
    );
    pa.complete(returned("committed A"));
    settle(&mut actor, pending.observe_ready().pop().unwrap());
    assert_eq!(pending.drain_progress(), NativeDrainProgress::Drained);
    assert_eq!(actor.close_and_drain(), CloseProgress::Drained);
    wa.finish();
    wb.finish();
}

#[test]
fn separate_native_and_cancel_actors_preserve_committed_results_during_close() {
    let (client, mut actor) = transport::channel();
    let (_wake, nudge) = ControlWake::paired();
    let nudge = NativeCompletionRoute::new(nudge, ControlWake::paired().1);
    let (waiter, reservation) = publish(&client, &mut actor, key(1));
    let (operation, producer) = native_pending::pair(key(1), ReadDeadline::Unbounded, nudge);
    let mut pending = NativePendingCalls::new();
    assert!(pending.insert(operation, reservation).is_ok());
    let race = Arc::new(Barrier::new(3));
    let native_gate = Arc::clone(&race);
    let native = thread::spawn(move || {
        native_gate.wait();
        producer.complete(returned("irreversible native commit"))
    });
    let cancel_gate = Arc::clone(&race);
    let cancel = thread::spawn(move || {
        cancel_gate.wait();
        client.cancel(key(1), CancelReason::TargetSignal);
        client.close();
    });
    race.wait();
    native.join().unwrap();
    cancel.join().unwrap();
    pending.request_cancel(key(1), CancelReason::TargetSignal);
    assert_eq!(
        settle(&mut actor, pending.observe_ready().pop().unwrap()),
        NativeSettlement::CompletedAfterCancellation {
            outcome: returned("irreversible native commit"),
            cancellation: CancelReason::TargetSignal
        }
    );
    assert_eq!(pending.drain_progress(), NativeDrainProgress::Drained);
    waiter.finish();
}

#[test]
fn mismatched_insert_returns_the_operation_and_reservation_without_consumption() {
    let (client, mut actor) = transport::channel();
    let (_wake, nudge) = ControlWake::paired();
    let nudge = NativeCompletionRoute::new(nudge, ControlWake::paired().1);
    let (waiter, reservation) = publish(&client, &mut actor, key(2));
    let (operation, producer) = native_pending::pair(key(1), ReadDeadline::Unbounded, nudge);
    let mut pending = NativePendingCalls::new();
    let rejected = match pending.insert(operation, reservation) {
        Err(rejected) => rejected,
        Ok(()) => panic!("mismatched keys accepted"),
    };
    let (mut operation, reservation, reason) = rejected.into_parts();
    assert_eq!(reason, PendingInsertRefusal::WrongKey);
    assert_eq!(operation.key(), key(1));
    assert_eq!(reservation.key(), key(2));
    producer.complete(returned("preserved operation"));
    assert!(matches!(
        operation.observe(),
        NativePendingObservation::Complete(_)
    ));
    assert!(matches!(
        actor.complete(reservation.ticket.settle(returned("preserved ticket"))),
        CompletionDisposition::Settled(retired) if retired == key(2)
    ));
    waiter.finish();
}

struct MetadataReservation(HostRequestKey);
impl PendingReservation for MetadataReservation {
    fn key(&self) -> HostRequestKey {
        self.0
    }
}

#[test]
fn duplicate_insert_returns_both_obligations_without_replacing_the_first() {
    let (_wake, nudge) = ControlWake::paired();
    let nudge = NativeCompletionRoute::new(nudge, ControlWake::paired().1);
    let (first, first_producer) =
        native_pending::pair(key(1), ReadDeadline::Unbounded, nudge.clone());
    let (duplicate, duplicate_producer) =
        native_pending::pair(key(1), ReadDeadline::Unbounded, nudge);
    let mut pending = NativePendingCalls::new();
    assert!(pending.insert(first, MetadataReservation(key(1))).is_ok());
    let rejected = match pending.insert(duplicate, MetadataReservation(key(1))) {
        Err(rejected) => rejected,
        Ok(()) => panic!("duplicate key replaced a linear native obligation"),
    };
    let (mut duplicate, reservation, reason) = rejected.into_parts();
    assert_eq!(reason, PendingInsertRefusal::DuplicateKey);
    assert_eq!(reservation.key(), key(1));
    duplicate_producer.complete(returned("rejected duplicate"));
    assert!(matches!(
        duplicate.observe(),
        NativePendingObservation::Complete(_)
    ));
    assert!(pending.observe_ready().is_empty());
    first_producer.complete(returned("original"));
    let (_, outcome, _) = pending.observe_ready().pop().unwrap().into_parts();
    assert_eq!(outcome, NativeReadyOutcome::Completed(returned("original")));
}

#[test]
fn closed_registry_rejects_new_work_and_only_real_producer_death_drains_old_work() {
    let (_wake, nudge) = ControlWake::paired();
    let nudge = NativeCompletionRoute::new(nudge, ControlWake::paired().1);
    let (first, first_producer) =
        native_pending::pair(key(1), ReadDeadline::Unbounded, nudge.clone());
    let mut pending = NativePendingCalls::new();
    assert!(pending.insert(first, MetadataReservation(key(1))).is_ok());
    assert!(matches!(
        pending.close(),
        NativeDrainProgress::AwaitingNativeSettlement(_)
    ));
    assert!(pending.observe_ready().is_empty());
    let (new_work, new_producer) = native_pending::pair(key(2), ReadDeadline::Unbounded, nudge);
    let rejected = match pending.insert(new_work, MetadataReservation(key(2))) {
        Err(rejected) => rejected,
        Ok(()) => panic!("closed native registry admitted new work"),
    };
    let (new_work, reservation, reason) = rejected.into_parts();
    assert_eq!(reason, PendingInsertRefusal::Closed);
    assert_eq!(new_work.key(), key(2));
    assert_eq!(reservation.key(), key(2));
    drop(new_work);
    drop(new_producer);
    assert!(pending.observe_ready().is_empty());
    drop(first_producer);
    let (ready_key, outcome, _) = pending.observe_ready().pop().unwrap().into_parts();
    assert_eq!(ready_key, key(1));
    assert_eq!(outcome, NativeReadyOutcome::ProducerClosed);
    assert_eq!(pending.drain_progress(), NativeDrainProgress::Drained);
}

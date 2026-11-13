use super::*;

#[test]
fn a_full_delivery_queue_latches_one_rescan_request() {
    let (sender, receiver) = channel_with_capacity::<_, ()>(1, None);

    assert_eq!(sender.publish(1), PublishOutcome::Published);
    assert_eq!(sender.publish(2), PublishOutcome::Overflowed);
    let first = receiver.drain_consistent();
    assert_eq!(first.records, [DeliveryRecord::Event(1)]);
    assert!(first.overflowed);
    assert!(
        !receiver.drain_consistent().overflowed,
        "overflow is coalesced"
    );
}

#[test]
fn control_records_do_not_compete_with_a_full_event_queue() {
    let (sender, receiver) = channel_with_capacity(1, None);

    assert_eq!(sender.publish(1), PublishOutcome::Published);
    let event_sender = sender.clone();
    sender.finish_with("native worker failed".to_owned(), || {});
    assert_eq!(event_sender.publish(2), PublishOutcome::Overflowed);

    let batch = receiver.drain_consistent();
    assert_eq!(
        batch.records,
        [
            DeliveryRecord::Event(1),
            DeliveryRecord::Control("native worker failed".to_owned()),
        ]
    );
}

#[test]
fn control_publication_commits_lifecycle_before_returning() {
    let (sender, receiver) = channel_with_capacity::<(), _>(1, None);
    let committed = AtomicBool::new(false);

    assert_eq!(
        sender.publish_control(7, || committed.store(true, Ordering::Release)),
        PublishOutcome::Published
    );

    assert!(committed.load(Ordering::Acquire));
    assert_eq!(
        receiver.drain_consistent().records,
        [DeliveryRecord::Control(7)]
    );
}

#[test]
fn failure_plane_preserves_every_worker_failure() {
    let (first, receiver) = channel_with_capacity::<(), _>(1, None);
    let second = first.clone();

    first.finish_with("first worker".to_owned(), || {});
    second.finish_with("second worker".to_owned(), || {});

    assert_eq!(
        receiver.drain_consistent().records,
        [
            DeliveryRecord::Control("first worker".to_owned()),
            DeliveryRecord::Control("second worker".to_owned()),
        ]
    );
}

#[test]
fn control_barrier_redrains_data_published_during_the_initial_poll() {
    let (sender, receiver) = channel_with_capacity(2, None);

    let batch = receiver.drain_consistent_with(|| {
        assert_eq!(sender.publish(9), PublishOutcome::Published);
        assert_eq!(
            sender.publish_control("terminal", || {}),
            PublishOutcome::Published
        );
    });

    assert_eq!(
        batch.records,
        [
            DeliveryRecord::Event(9),
            DeliveryRecord::Control("terminal")
        ]
    );
}

#[test]
fn separate_delivery_planes_preserve_publication_order() {
    let (sender, receiver) = channel_with_capacity(2, None);

    assert_eq!(
        sender.publish_control("terminal", || {}),
        PublishOutcome::Published
    );
    assert_eq!(sender.publish(9), PublishOutcome::Published);

    assert_eq!(
        receiver.drain_consistent().records,
        [
            DeliveryRecord::Control("terminal"),
            DeliveryRecord::Event(9)
        ]
    );
}

#[test]
fn each_consistent_drain_has_a_fixed_data_work_budget() {
    let (sender, receiver) = channel_with_capacity::<_, ()>(2, None);
    assert_eq!(sender.publish(1), PublishOutcome::Published);
    assert_eq!(sender.publish(2), PublishOutcome::Published);

    let batch = receiver.drain_consistent_with(|| {
        assert_eq!(sender.publish(3), PublishOutcome::Published);
        assert_eq!(sender.publish(4), PublishOutcome::Published);
    });

    assert_eq!(
        batch.records,
        [
            DeliveryRecord::Event(1),
            DeliveryRecord::Event(2),
            DeliveryRecord::Event(3),
            DeliveryRecord::Event(4),
        ]
    );
    assert!(
        batch.records.len() <= 3 * receiver.event_capacity,
        "continuous producers cannot make one evaluator drain unbounded"
    );
}

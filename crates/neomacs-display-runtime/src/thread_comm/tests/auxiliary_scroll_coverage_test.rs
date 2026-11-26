use super::*;
use neomacs_display_protocol::scroll_coverage::AuxiliaryScrollCoverage;
use neomacs_display_protocol::{DisplayFrameId, PresentationId};
use std::collections::HashMap;
use std::num::NonZeroUsize;

fn update(frame: u64, base: u64) -> AuxiliaryScrollCoverage {
    AuxiliaryScrollCoverage {
        frame: DisplayFrameId::new(frame),
        base: PresentationId::new(base),
        surfaces: Vec::new(),
    }
}

#[test]
fn auxiliary_snapshot_replacement_coalesces_one_wake_per_frame() {
    let (emacs, render) = ThreadComms::new().split();
    let publish = |payload| {
        emacs
            .cmd_tx
            .publish_scroll_coverage(&emacs.auxiliary_scroll_coverage, payload)
    };
    assert!(publish(update(7, 10)).unwrap());
    assert!(!publish(update(7, 11)).unwrap());
    assert!(publish(update(8, 12)).unwrap());
    for expected in [7, 8] {
        assert!(matches!(render.cmd_rx.try_recv().unwrap(),
            RenderCommand::Window(WindowCommand::PublishScrollCoverage { frame })
                if frame == DisplayFrameId::new(expected)));
    }
    assert!(render.cmd_rx.try_recv().is_err());
    let latest = render.take_scroll_coverage(DisplayFrameId::new(7)).unwrap();
    assert_eq!(latest.base, PresentationId::new(11));
    assert!(
        render
            .take_scroll_coverage(DisplayFrameId::new(7))
            .is_none()
    );
    assert_eq!(
        render
            .take_scroll_coverage(DisplayFrameId::new(8))
            .unwrap()
            .base,
        PresentationId::new(12)
    );
    // A producer that races after the take sees an empty slot and queues its
    // own next wake; a producer before it would replace the payload above.
    assert!(publish(update(7, 13)).unwrap());
    assert!(matches!(render.cmd_rx.try_recv().unwrap(),
        RenderCommand::Window(WindowCommand::PublishScrollCoverage { frame })
            if frame == DisplayFrameId::new(7)));
    assert_eq!(
        render
            .take_scroll_coverage(DisplayFrameId::new(7))
            .unwrap()
            .base,
        PresentationId::new(13)
    );
}

#[test]
fn auxiliary_full_queue_returns_payload_without_an_unwoken_slot() {
    let (sender, receiver) = RenderCommandSender::bounded(NonZeroUsize::new(1).unwrap());
    sender
        .try_send(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
        .unwrap();
    let slots = Mutex::new(HashMap::new());
    let returned = match sender.publish_scroll_coverage(&slots, update(7, 10)) {
        Err(TrySendError::Full(payload)) => payload,
        result => panic!("expected full auxiliary queue, got {result:?}"),
    };
    assert!(slots.lock().unwrap().is_empty());
    assert_eq!(returned.base, PresentationId::new(10));
    receiver.try_recv().unwrap();
    assert!(sender.publish_scroll_coverage(&slots, returned).unwrap());
    assert_eq!(slots.lock().unwrap().len(), 1);
    assert!(matches!(
        receiver.try_recv().unwrap(),
        RenderCommand::Window(WindowCommand::PublishScrollCoverage { .. })
    ));
}

#[test]
fn auxiliary_disconnected_queue_returns_payload_without_installing_a_slot() {
    let (sender, receiver) = RenderCommandSender::bounded(NonZeroUsize::new(1).unwrap());
    drop(receiver);
    let slots = Mutex::new(HashMap::new());
    let returned = match sender.publish_scroll_coverage(&slots, update(7, 10)) {
        Err(TrySendError::Disconnected(payload)) => payload,
        result => panic!("expected disconnected auxiliary queue, got {result:?}"),
    };
    assert!(slots.lock().unwrap().is_empty());
    assert_eq!(returned.base, PresentationId::new(10));
}

#[test]
fn auxiliary_concurrent_take_and_replace_do_not_lose_the_latest_snapshot() {
    let (sender, receiver) = RenderCommandSender::bounded(NonZeroUsize::new(1).unwrap());
    let slots = Arc::new(Mutex::new(HashMap::new()));
    let producer_slots = Arc::clone(&slots);
    let producer = std::thread::spawn(move || {
        for base in 1..=100 {
            sender
                .publish_scroll_coverage(&producer_slots, update(7, base))
                .unwrap();
            std::thread::yield_now();
        }
    });
    let mut latest = 0;
    while latest < 100 {
        let command = receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        let frame = match command {
            RenderCommand::Window(WindowCommand::PublishScrollCoverage { frame }) => frame,
            other => panic!("unexpected auxiliary wake: {other:?}"),
        };
        let payload = slots
            .lock()
            .unwrap()
            .remove(&frame)
            .expect("every wake has a payload");
        assert!(payload.base.get() > latest);
        latest = payload.base.get();
    }
    producer.join().unwrap();
    assert!(slots.lock().unwrap().is_empty());
    assert!(receiver.try_recv().is_err());
}

use super::*;

#[test]
fn preview_observation_belongs_only_to_innermost_command() {
    let stream = InputStream::default();
    let outer_input = stream.issue().unwrap();
    let receipt = outer_input.receipt();
    let mut progress = InputProgress::default();
    progress.consumed(outer_input);
    assert!(progress.current_command_receipts().is_empty());
    let outer = progress.begin_command();
    assert!(progress.current_command_receipts()[0].same_input(&receipt));
    let inner = progress.begin_command();
    assert!(progress.current_command_receipts().is_empty());
    drop(inner);
    assert_eq!(progress.current_command_receipts().len(), 1);
    drop(outer);
    assert!(progress.current_command_receipts().is_empty());
    assert!(receipt.acknowledged_by(&progress.checkpoint()));
}

#[test]
fn old_frame_cannot_acknowledge_a_later_command_completion() {
    let stream = InputStream::default();
    let receipt = stream.issue().unwrap();
    let mut progress = InputProgress::default();
    progress.consumed(receipt.clone());
    let command = progress.begin_command();
    let old_frame = progress.checkpoint();
    assert!(!receipt.acknowledged_by(&old_frame));
    drop(command);
    assert!(!receipt.acknowledged_by(&old_frame));
    assert!(receipt.acknowledged_by(&progress.checkpoint()));
}

#[test]
fn nested_completion_waits_for_the_outer_command() {
    let stream = InputStream::default();
    let outer = stream.issue().unwrap();
    let inner = stream.issue().unwrap();
    let mut progress = InputProgress::default();
    progress.consumed(outer.clone());
    let outer_command = progress.begin_command();
    let inner_command = progress.begin_command();
    progress.consumed(inner.clone());
    drop(inner_command);
    assert!(!inner.acknowledged_by(&progress.checkpoint()));
    drop(outer_command);
    assert!(inner.acknowledged_by(&progress.checkpoint()));
}

#[test]
fn completion_on_unwind_and_stream_identity_are_independent() {
    let stream = InputStream::default();
    let receipt = stream.issue().unwrap();
    let foreign = InputStream::default().issue().unwrap();
    let mut progress = InputProgress::default();
    progress.consumed(receipt.clone());
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _command = progress.begin_command();
        panic!("command aborted");
    }));
    assert!(receipt.acknowledged_by(&progress.checkpoint()));
    assert!(!foreign.acknowledged_by(&progress.checkpoint()));
}

#[test]
fn discarding_delivery_cancels_even_while_the_compositor_keeps_a_receipt() {
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let observer = delivery.receipt();
    let queued_copy = delivery.clone();
    drop(delivery);
    assert!(!observer.cancelled());
    drop(queued_copy);
    assert!(observer.cancelled());
    assert!(observer.acknowledged_by(&[stream.checkpoint()]));
}

#[test]
fn completed_delivery_does_not_later_turn_into_cancellation() {
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let observer = delivery.receipt();
    let mut progress = InputProgress::default();
    let command = progress.begin_command();
    progress.consumed(delivery);
    assert!(!observer.acknowledged_by(&progress.checkpoint()));
    drop(command);
    assert!(observer.acknowledged_by(&progress.checkpoint()));
    assert!(!observer.cancelled());
}

#[test]
fn pending_receipts_are_bounded_and_capacity_returns_after_completion() {
    let stream = InputStream::default();
    let receipts: Vec<_> = (0..MAX_PENDING).map(|_| stream.issue().unwrap()).collect();
    assert!(stream.issue().is_none());
    for receipt in receipts {
        receipt.complete();
    }
    assert!(stream.issue().is_some());
}

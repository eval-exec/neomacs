use super::*;

#[test]
fn repeat_backpressure_releases_on_read_not_command_completion() {
    let stream = InputStream::default();
    let delivery = stream.issue().unwrap();
    let receipt = delivery.receipt();
    let bridge_copy = delivery.clone();
    let mut progress = InputProgress::default();
    let command = progress.begin_command();
    assert!(!receipt.consumed_or_cancelled());
    progress.consumed(delivery);
    assert!(receipt.consumed_or_cancelled());
    assert!(!receipt.acknowledged_by(&progress.checkpoint()));
    drop(bridge_copy);
    assert!(!receipt.cancelled());
    drop(command);
    assert!(receipt.acknowledged_by(&progress.checkpoint()));
}

#[test]
fn dropped_repeat_delivery_releases_backpressure() {
    let delivery = InputDelivery::for_read();
    let receipt = delivery.receipt();
    assert!(!receipt.consumed_or_cancelled());
    drop(delivery);
    assert!(receipt.consumed_or_cancelled());
    assert!(receipt.cancelled());
}

#[test]
fn read_receipt_identity_is_independent_of_completion_checkpoints() {
    let read = InputDelivery::for_read();
    let receipt = read.receipt();
    assert!(receipt.same_input(&read.receipt()));
    assert!(!receipt.same_input(&InputDelivery::for_read().receipt()));
    let stream = InputStream::default();
    let completion = stream.issue().unwrap();
    assert!(!receipt.same_input(&completion.receipt()));
    let mut progress = InputProgress::default();
    let command = progress.begin_command();
    progress.consumed(completion);
    progress.consumed(read);
    drop(command);
    assert!(receipt.consumed_or_cancelled());
    assert!(!receipt.cancelled());
    assert!(!receipt.acknowledged_by(&progress.checkpoint()));
}

use super::*;

#[test]
fn snapshot_validates_unicode_selection_offsets() {
    let snapshot = ImeTextSnapshot::new(ImeSnapshotId(7), "A😀Z".into(), 5, 1).unwrap();
    assert_eq!(snapshot.id(), ImeSnapshotId(7));
    assert_eq!(snapshot.text(), "A😀Z");
    assert_eq!((snapshot.cursor(), snapshot.anchor()), (5, 1));
    for invalid in [2, 3, 4, 7, usize::MAX] {
        assert!(ImeTextSnapshot::new(ImeSnapshotId(7), "A😀Z".into(), invalid, 1).is_none());
        assert!(ImeTextSnapshot::new(ImeSnapshotId(7), "A😀Z".into(), 1, invalid).is_none());
    }
}

#[test]
fn snapshot_enforces_transport_size_limit() {
    assert!(ImeTextSnapshot::new(ImeSnapshotId(1), "a".repeat(3999), 3999, 0).is_some());
    assert!(ImeTextSnapshot::new(ImeSnapshotId(1), "a".repeat(4000), 0, 0).is_none());
}

#[test]
fn protocol_values_can_cross_threads_without_vm_state() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ImeTextSnapshot>();
    assert_send_sync::<ImeSelection>();
    assert_send_sync::<ImeSelectionOutcome>();
    assert_send_sync::<ImeOperation>();
}

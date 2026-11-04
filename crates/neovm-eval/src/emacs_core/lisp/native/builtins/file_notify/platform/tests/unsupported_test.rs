use super::*;

#[test]
fn unsupported_backend_keeps_common_lifecycle_inert() {
    let mut backend = UnsupportedBackend;
    let watch_id = WatchId::new(0, 0);

    assert!(!backend.has_watches());
    assert!(!backend.valid_p(&watch_id));
    assert!(matches!(
        backend.remove_watch(&watch_id),
        RemoveWatchOutcome::NotFound
    ));
    let batch = backend.drain_events().expect("drain inert backend");
    assert!(batch.events.is_empty());
    assert!(batch.terminated.is_empty());
    assert!(batch.failure.is_none());
}

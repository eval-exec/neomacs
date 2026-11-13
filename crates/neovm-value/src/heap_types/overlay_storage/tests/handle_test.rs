use super::*;

#[test]
fn overlay_position_handle_reads_live_store_and_does_not_retain_it() {
    let overlay = Value::NIL;
    let initial = EmacsByteRange::from_usize(2, 5);
    let moved = EmacsByteRange::from_usize(4, 9);
    let store = Arc::new(RwLock::new(IntervalBPlusTree::new()));
    assert!(store.write().insert(overlay, initial));
    let handle = OverlayPositionHandle::new(overlay, &store);
    assert_eq!(handle.current_range(), Some(initial));
    store.write().take(overlay);
    assert!(store.write().insert(overlay, moved));
    assert_eq!(handle.current_range(), Some(moved));
    drop(store);
    assert_eq!(handle.current_range(), None);
}

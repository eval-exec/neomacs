use super::*;
use neomacs_app::frontend_event::ImeSessionId;

#[test]
fn replacing_connection_rejects_old_identity_even_in_the_same_session() {
    let mut connections = Connections::default();
    let old = connections.open(ImeSessionId(1)).unwrap();
    let current = connections.open(ImeSessionId(1)).unwrap();
    assert_ne!(old, current);
    assert_eq!(connections.validate(old), Err(ConnectionError::Retired));
    assert_eq!(connections.validate(current), Ok(()));
    assert!(!connections.close(old));
    assert_eq!(connections.validate(current), Ok(()));
}

#[test]
fn batches_nest_and_old_end_cannot_finish_new_connections_batch() {
    let mut connections = Connections::default();
    let old = connections.open(ImeSessionId(1)).unwrap();
    connections.begin_batch(old).unwrap();
    let current = connections.open(ImeSessionId(2)).unwrap();
    connections.begin_batch(current).unwrap();
    connections.begin_batch(current).unwrap();
    assert_eq!(connections.end_batch(old), Err(ConnectionError::Retired));
    assert_eq!(connections.end_batch(current), Ok(BatchState::InProgress));
    assert_eq!(connections.end_batch(current), Ok(BatchState::Idle));
    assert_eq!(connections.end_batch(current), Ok(BatchState::Idle));
    assert!(connections.close(current));
    assert!(!connections.close(current));
    assert_eq!(
        connections.begin_batch(current),
        Err(ConnectionError::Retired)
    );
}

#[test]
fn retired_connection_cannot_decode_selection_against_a_new_snapshot() {
    use neovm_host_abi::ime::{ImeSelection, ImeSnapshotId, ImeTextSnapshot};
    let mut connections = Connections::default();
    let old = connections.open(ImeSessionId(1)).unwrap();
    let current = connections.open(ImeSessionId(2)).unwrap();
    let snapshot = ImeTextSnapshot::new(ImeSnapshotId(99), "A😀Z".into(), 0, 0).unwrap();
    assert_eq!(
        connections.selection(old, &snapshot, 1, 3),
        Err(SelectionError::Connection(ConnectionError::Retired))
    );
    assert_eq!(
        connections.selection(current, &snapshot, 1, 3),
        Ok(ImeSelection {
            snapshot: ImeSnapshotId(99),
            cursor: 1,
            anchor: 5,
        })
    );
}

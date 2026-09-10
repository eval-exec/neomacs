use super::*;

#[test]
fn utf16_selection_preserves_snapshot_and_reversed_selection() {
    let snapshot = ImeTextSnapshot::new(ImeSnapshotId(42), "A😀Z".into(), 5, 1).unwrap();
    let selection = snapshot
        .selection_from_utf16(
            ImeUtf16Offset::new(1).unwrap(),
            ImeUtf16Offset::new(3).unwrap(),
        )
        .unwrap();
    assert_eq!(
        selection,
        ImeSelection {
            snapshot: ImeSnapshotId(42),
            cursor: 1,
            anchor: 5
        }
    );
}

#[test]
fn utf16_selection_rejects_surrogate_interiors_and_out_of_bounds() {
    let snapshot = ImeTextSnapshot::new(ImeSnapshotId(9), "A😀Z".into(), 0, 0).unwrap();
    let start = ImeUtf16Offset::new(0).unwrap();
    for invalid in [2, 5, i32::MAX] {
        let offset = ImeUtf16Offset::new(invalid).unwrap();
        assert!(snapshot.selection_from_utf16(offset, start).is_none());
        assert!(snapshot.selection_from_utf16(start, offset).is_none());
    }
    assert!(ImeUtf16Offset::new(-1).is_none());
    assert!(ImeUtf16Offset::new(i32::MIN).is_none());
}

#[test]
fn utf16_selection_handles_endpoints_empty_text_and_combining_characters() {
    // Combining marks remain individually addressable, unlike surrogate halves.
    let snapshot = ImeTextSnapshot::new(ImeSnapshotId(10), "é\u{301}😀".into(), 0, 0).unwrap();
    for (units, bytes) in [(0, 0), (1, 2), (2, 4), (4, 8)] {
        let offset = ImeUtf16Offset::new(units).unwrap();
        assert_eq!(
            snapshot.selection_from_utf16(offset, offset),
            Some(ImeSelection {
                snapshot: ImeSnapshotId(10),
                cursor: bytes,
                anchor: bytes,
            })
        );
    }
    let empty = ImeTextSnapshot::new(ImeSnapshotId(11), String::new(), 0, 0).unwrap();
    let zero = ImeUtf16Offset::new(0).unwrap();
    assert_eq!(
        empty.selection_from_utf16(zero, zero),
        Some(ImeSelection {
            snapshot: ImeSnapshotId(11),
            cursor: 0,
            anchor: 0,
        })
    );
    assert!(
        empty
            .selection_from_utf16(ImeUtf16Offset::new(1).unwrap(), zero)
            .is_none()
    );
}

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

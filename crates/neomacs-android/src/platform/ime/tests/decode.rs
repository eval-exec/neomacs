use super::*;
use neovm_host_abi::ime::{ImeSelection, ImeSnapshotId, ImeTextSnapshot};

#[test]
fn android_selection_preserves_gnu_point_and_mark_mapping() {
    let snapshot = ImeTextSnapshot::new(ImeSnapshotId(7), "A😀Z".into(), 0, 0).unwrap();
    assert_eq!(
        selection(&snapshot, 1, 3),
        Ok(ImeSelection {
            snapshot: ImeSnapshotId(7),
            cursor: 1,
            anchor: 5,
        })
    );
    assert_eq!(
        selection(&snapshot, 3, 1),
        Ok(ImeSelection {
            snapshot: ImeSnapshotId(7),
            cursor: 5,
            anchor: 1,
        })
    );
}

#[test]
fn android_selection_rejects_negative_split_surrogate_and_external_offsets() {
    let snapshot = ImeTextSnapshot::new(ImeSnapshotId(8), "A😀Z".into(), 0, 0).unwrap();
    for invalid in [-1, i32::MIN, 2, 5, i32::MAX] {
        assert_eq!(
            selection(&snapshot, invalid, 0),
            Err(DecodeError::InvalidSelection)
        );
        assert_eq!(
            selection(&snapshot, 0, invalid),
            Err(DecodeError::InvalidSelection)
        );
    }
}

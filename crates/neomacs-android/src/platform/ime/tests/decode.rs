use super::*;
use neovm_host_abi::ime::{ImeSelection, ImeSnapshotId, ImeTextSnapshot};

#[test]
fn commit_and_composition_preserve_signed_cursor_semantics() {
    for (raw, cursor) in [
        (1, CursorPlacement::AfterEnd(0)),
        (2, CursorPlacement::AfterEnd(1)),
        (0, CursorPlacement::BeforeStart(0)),
        (-1, CursorPlacement::BeforeStart(1)),
        (i32::MIN, CursorPlacement::BeforeStart(2147483648)),
        (i32::MAX, CursorPlacement::AfterEnd(2147483646)),
    ] {
        for kind in [TextEditKind::Commit, TextEditKind::Compose] {
            assert_eq!(
                text_edit(kind, "😀".into(), raw),
                TextEdit {
                    kind,
                    text: "😀".into(),
                    cursor,
                }
            );
        }
    }
}

#[test]
fn deletion_counts_keep_their_units_and_reject_negative_lengths() {
    assert_eq!(
        deletion(2, 0, DeletionUnit::Utf16),
        Ok(Deletion {
            before: 2,
            after: 0,
            unit: DeletionUnit::Utf16
        })
    );
    assert_eq!(
        deletion(1, 0, DeletionUnit::CodePoint),
        Ok(Deletion {
            before: 1,
            after: 0,
            unit: DeletionUnit::CodePoint
        })
    );
    assert_eq!(
        deletion(-1, 0, DeletionUnit::CodePoint),
        Err(DecodeError::InvalidDeletion)
    );
    assert_eq!(
        deletion(0, -1, DeletionUnit::Utf16),
        Err(DecodeError::InvalidDeletion)
    );
}

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

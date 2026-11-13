use super::*;

/// The link is a live dereference, not a snapshot: GNU reads
/// `base_buffer->modtime` at the moment of the change (`src/undo.c:221`),
/// so a `save-buffer` in the base between `make-indirect-buffer` and the
/// first change must be visible.
#[test]
fn an_attached_slot_reads_the_bases_current_modtime() {
    let base = VisitedFileModtimeSlot::new(VisitedFileModtime::Known { sec: 1, nsec: 2 });
    let mut indirect = VisitedFileModtimeSlot::default();
    indirect.reset_as_indirect_of(base.share_own());

    assert_eq!(
        indirect.for_first_change(),
        base.for_first_change(),
        "an indirect buffer's first change records its base's modtime"
    );
    assert_eq!(
        indirect.own(),
        VisitedFileModtime::Unknown,
        "the indirect buffer visits no file of its own"
    );

    base.set_own(VisitedFileModtime::Known { sec: 9, nsec: 8 });
    assert_eq!(
        indirect.for_first_change(),
        FirstChangeModtime(VisitedFileModtime::Known { sec: 9, nsec: 8 }),
        "the base's later modtime must be visible through the link"
    );
}

/// Setting an indirect buffer's own modtime does not change what its first
/// change records -- GNU's redirect is unconditional (`src/undo.c:213-214`),
/// confirmed against GNU 31.0.90 with `(set-visited-file-modtime '(1 2 3 4))`
/// in the indirect buffer.
#[test]
fn an_own_modtime_does_not_displace_the_bases_for_first_change() {
    let base = VisitedFileModtimeSlot::new(VisitedFileModtime::Known { sec: 7, nsec: 0 });
    let mut indirect = VisitedFileModtimeSlot::default();
    indirect.reset_as_indirect_of(base.share_own());
    indirect.set_own(VisitedFileModtime::Known { sec: 1, nsec: 2 });

    assert_eq!(
        indirect.own(),
        VisitedFileModtime::Known { sec: 1, nsec: 2 }
    );
    assert_eq!(
        indirect.for_first_change(),
        FirstChangeModtime(VisitedFileModtime::Known { sec: 7, nsec: 0 })
    );
}

/// A cloned buffer is a different buffer: writing its modtime must not
/// write the original's.
#[test]
fn cloning_a_slot_copies_the_value_and_not_the_cell() {
    let original = VisitedFileModtimeSlot::new(VisitedFileModtime::Known { sec: 5, nsec: 6 });
    let clone = original.clone();
    assert_eq!(clone.own(), original.own());

    clone.set_own(VisitedFileModtime::Unknown);
    assert_eq!(
        original.own(),
        VisitedFileModtime::Known { sec: 5, nsec: 6 },
        "the clone must not share the original's storage"
    );
}

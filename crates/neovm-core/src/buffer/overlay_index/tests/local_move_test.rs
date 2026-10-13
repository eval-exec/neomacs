use super::*;

fn force_local_move_knob_before_runtime_initialization() {
    // SAFETY: this repository runs tests through nextest, which gives each
    // test its own process. Set non-Lisp configuration before tracing/runtime
    // initialization can start workers or read the once-per-process knob.
    unsafe {
        std::env::set_var("NEOVM_OVERLAY_LOCAL_MOVE", "on");
    }
}

#[test]
fn cl2_local_start_moves_do_not_materialize_positions_during_mirror_descent() {
    force_local_move_knob_before_runtime_initialization();
    crate::test_utils::init_test_tracing();

    let mut index = OverlayIndex::new();
    let entries: Vec<_> = (0..5_000)
        .map(|entry| {
            let start = 1 + entry * 4;
            (overlay(start, start + 2), range(start, start + 2))
        })
        .collect();
    assert!(index.attach_batch(&entries, OverlayBatchOrder::AttachmentSequence));
    let (moving, original) = entries[2_500];
    index
        .intervals
        .read()
        .records
        .reset_identity_position_resolution_count();

    let mut previous = original;
    let moves = 64;
    for step in 0..moves {
        let displacement = usize::from(step % 2 == 0);
        let next = range(
            original.start().get() + displacement,
            original.end().get() + displacement,
        );
        assert_eq!(index.move_to(moving, next), Some(previous));
        previous = next;
    }

    let resolutions = index
        .intervals
        .read()
        .records
        .identity_position_resolution_count();
    assert!(
        resolutions <= moves * 2,
        "real interior local moves need their old range and at most one \
         structural predecessor guard, rather than a coordinate lookup per \
         GNU mirror descent: {resolutions} resolutions for {moves} moves"
    );
    assert_eq!(index.range(moving), Some(original));
    index.assert_invariants();
}

#[test]
fn cl2_local_move_preserves_gnu_topology_after_order_and_shift_changes() {
    force_local_move_knob_before_runtime_initialization();
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let overlays: Vec<_> = (0..257)
        .map(|entry| {
            let start = 10 + (entry * 73 % 41) * 4;
            let value = overlay(start, start + 3 + entry % 7);
            assert!(index.attach(value, range(start, start + 3 + entry % 7)));
            value
        })
        .collect();
    index.adjust_for_text_edit(OverlayTextEdit::Insert {
        position: EmacsBytePos::ZERO,
        length: EmacsByteLen::new(7),
        before_markers: false,
    });

    let identities: Vec<_> = overlays.iter().copied().map(OverlayIdentity::of).collect();
    for (step, moving) in overlays.iter().copied().step_by(7).enumerate() {
        let original = index.range(moving).unwrap();
        let next = if step % 3 == 0 {
            range(original.start().get(), original.end().get() + 1)
        } else if step % 3 == 1 {
            range(original.start().get() + 1, original.end().get() + 1)
        } else {
            range(40, 55)
        };
        let mut reference = index.gnu_order.clone();
        assert_eq!(index.move_to(moving, next), Some(original));
        if original.start() != next.start() {
            let identity = OverlayIdentity::of(moving);
            assert!(reference.remove(identity).is_ok());
            assert!(
                reference
                    .insert_by(identity, |existing| {
                        next.start().cmp(
                            &index
                                .intervals
                                .read()
                                .range_by_identity(existing)
                                .unwrap()
                                .start(),
                        )
                    })
                    .is_ok()
            );
        }
        assert_eq!(
            index.gnu_order.subset_in_preorder(&identities),
            reference.subset_in_preorder(&identities),
            "a start-changing move must retain GNU's exact topology at step {step}"
        );
        index.assert_invariants();
    }
}

#[test]
fn cl2_local_move_keeps_published_endpoints_after_lazy_shifts() {
    force_local_move_knob_before_runtime_initialization();
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let entries: Vec<_> = (0..129)
        .map(|entry| {
            let start = 10 + entry * 4;
            (overlay(start, start + 2), range(start, start + 2))
        })
        .collect();
    assert!(index.attach_batch(&entries, OverlayBatchOrder::AttachmentSequence));
    assert_eq!(
        index.next_boundary_after(EmacsBytePos::ZERO, EmacsBytePos::new(1_000)),
        Some(EmacsBytePos::new(10))
    );
    index.adjust_for_text_edit(OverlayTextEdit::Insert {
        position: EmacsBytePos::ZERO,
        length: EmacsByteLen::new(7),
        before_markers: false,
    });

    for entry in [0, 31, 64, 127, 128] {
        let moving = entries[entry].0;
        let original = index.range(moving).unwrap();
        let next = range(original.start().get() + 1, original.end().get() + 1);
        assert_eq!(index.move_to(moving, next), Some(original));
        assert_eq!(index.range(moving), Some(next));
        assert_eq!(index.overlays_at(next.start()), vec![moving]);
        assert!(index.overlays_at(original.start()).is_empty());
        assert_eq!(
            index.next_boundary_after(original.start(), EmacsBytePos::new(1_000)),
            Some(next.start())
        );
        assert_eq!(
            index.previous_boundary_before(next.end(), EmacsBytePos::ZERO),
            Some(next.start())
        );
        index.assert_invariants();
    }
}

#[test]
fn cl2_local_moves_update_ordered_record_without_membership_mutations() {
    force_local_move_knob_before_runtime_initialization();
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let entries: Vec<_> = (0..5_000)
        .map(|entry| {
            let start = 10 + entry * 4;
            (overlay(start, start + 2), range(start, start + 2))
        })
        .collect();
    assert!(index.attach_batch(&entries, OverlayBatchOrder::AttachmentSequence));
    // Exercise both a root lazy tag and tags limited to a suffix. The local
    // replacement must normalize the owner's path before comparing keys.
    for (position, length) in [(0, 7), (5_000, 3)] {
        index.adjust_for_text_edit(OverlayTextEdit::Insert {
            position: EmacsBytePos::new(position),
            length: EmacsByteLen::new(length),
            before_markers: false,
        });
    }
    let moving = entries[2_500].0;
    let original = index.range(moving).unwrap();
    let initial_attachment = index.intervals.read().next_attachment_order;
    index
        .intervals
        .read()
        .records
        .reset_membership_mutation_count();

    let moves = 64;
    let mut previous = original;
    for step in 0..moves {
        let displacement = usize::from(step % 2 == 0);
        // Expanding across later intervals also exercises max-end and
        // non-overlap augmentation; this changes real endpoints every time.
        let extension = if displacement == 1 { 8 } else { 0 };
        let next = range(
            original.start().get() + displacement,
            original.end().get() + displacement + extension,
        );
        assert_eq!(index.move_to(moving, next), Some(previous));
        assert_eq!(index.range(moving), Some(next));
        assert!(
            index
                .overlays_at(EmacsBytePos::new(next.end().get() - 1))
                .iter()
                .any(|candidate| candidate.bits() == moving.bits())
        );
        index.assert_invariants();
        previous = next;
    }
    let intervals = index.intervals.read();
    assert_eq!(intervals.next_attachment_order, initial_attachment + moves);
    assert_eq!(
        intervals.records.membership_mutation_count(),
        0,
        "genuine neighbor-preserving moves should update the occupied record \
         rather than remove/reinsert B+ membership"
    );
}

#[test]
fn cl2_local_move_attempts_consume_one_attachment_order_and_keep_equal_start_order() {
    force_local_move_knob_before_runtime_initialization();
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let entries: Vec<_> = (0..97)
        .map(|entry| {
            let start = 10 + entry * 4;
            (overlay(start, start + 2), range(start, start + 2))
        })
        .collect();
    assert!(index.attach_batch(&entries, OverlayBatchOrder::AttachmentSequence));
    let (moving, original) = entries[48];
    let identities: Vec<_> = entries
        .iter()
        .map(|(value, _)| OverlayIdentity::of(*value))
        .collect();
    for next in [
        range(original.start().get() + 1, original.end().get() + 1),
        range(16, 19),
        range(original.start().get() + 1, original.end().get() + 1),
        entries[47].1,
        range(original.start().get() + 5, original.end().get() + 5),
        original,
        range(original.start().get(), original.end().get() + 9),
        range(original.start().get(), original.end().get() + 9),
    ] {
        let previous = index.range(moving).unwrap();
        let previous_record = index
            .intervals
            .read()
            .records
            .record(OverlayIdentity::of(moving))
            .unwrap();
        let next_attachment = index.intervals.read().next_attachment_order;
        let mut reference = index.gnu_order.clone();
        assert_eq!(index.move_to(moving, next), Some(previous));
        let record = index
            .intervals
            .read()
            .records
            .record(OverlayIdentity::of(moving))
            .unwrap();
        if previous.start() != next.start() {
            assert_eq!(record.key.attachment_order, next_attachment);
            assert_eq!(
                index.intervals.read().next_attachment_order,
                next_attachment + 1,
                "a rejected local attempt must not consume a second attachment"
            );
            let identity = OverlayIdentity::of(moving);
            assert!(reference.remove(identity).is_ok());
            assert!(
                reference
                    .insert_by(identity, |existing| {
                        next.start().cmp(
                            &index
                                .intervals
                                .read()
                                .range_by_identity(existing)
                                .unwrap()
                                .start(),
                        )
                    })
                    .is_ok()
            );
        } else {
            assert_eq!(record.key, previous_record.key);
            assert_eq!(
                index.intervals.read().next_attachment_order,
                next_attachment
            );
        }
        assert_eq!(
            index.gnu_order.subset_in_preorder(&identities),
            reference.subset_in_preorder(&identities)
        );
        if next.start() == entries[47].1.start() {
            assert_eq!(index.overlays_at(next.start())[0].bits(), moving.bits());
        }
        index.assert_invariants();
    }
}

#[test]
fn cl2_local_moves_keep_leaf_edges_and_reject_crossing_neighbors() {
    force_local_move_knob_before_runtime_initialization();
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let entries: Vec<_> = (0..97)
        .map(|entry| {
            let start = 10 + entry * 4;
            (overlay(start, start + 2), range(start, start + 2))
        })
        .collect();
    assert!(index.attach_batch(&entries, OverlayBatchOrder::AttachmentSequence));
    for (position, length) in [(0, 7), (100, 3)] {
        index.adjust_for_text_edit(OverlayTextEdit::Insert {
            position: EmacsBytePos::new(position),
            length: EmacsByteLen::new(length),
            before_markers: false,
        });
    }
    index
        .intervals
        .read()
        .records
        .reset_membership_mutation_count();

    // Balanced construction puts 25 records in the first leaf and 24 in
    // each remaining leaf. Include both sides of every leaf boundary and
    // the ends of the whole tree, after unequal lazy shifts.
    for entry in [0, 24, 25, 48, 49, 72, 73, 96] {
        let moving = entries[entry].0;
        let original = index.range(moving).unwrap();
        let next = range(original.start().get() + 1, original.end().get() + 1);
        assert_eq!(index.move_to(moving, next), Some(original));
        assert_eq!(index.move_to(moving, original), Some(next));
        index.assert_invariants();
    }
    assert_eq!(
        index.intervals.read().records.membership_mutation_count(),
        0
    );

    // A fresh attachment may become the newest equal-start record without
    // crossing a full key. Its GNU mirror still must be removed/reinserted.
    let moving = entries[24].0;
    let previous = index.range(moving).unwrap();
    let equal = index.range(entries[25].0).unwrap();
    assert_eq!(index.move_to(moving, equal), Some(previous));
    assert_eq!(index.overlays_at(equal.start())[0].bits(), moving.bits());
    assert_eq!(
        index.intervals.read().records.membership_mutation_count(),
        0
    );
    index.assert_invariants();

    // Crossing the next full key must use the generic membership path.
    let next = index.range(entries[26].0).unwrap();
    let crossing = range(next.start().get() + 1, next.end().get() + 1);
    assert_eq!(index.move_to(moving, crossing), Some(equal));
    assert_eq!(
        index.intervals.read().records.membership_mutation_count(),
        2
    );
    index.assert_invariants();
}

#[test]
fn cl2_local_moves_preserve_gnu_topology_after_deletion_collapses_distinct_starts() {
    force_local_move_knob_before_runtime_initialization();
    crate::test_utils::init_test_tracing();
    let mut index = OverlayIndex::new();
    let a = overlay(10, 30);
    let b = overlay(20, 30);
    let moving = overlay(40, 45);
    for (value, bounds) in [
        (a, range(10, 30)),
        (b, range(20, 30)),
        (moving, range(40, 45)),
    ] {
        assert!(index.attach(value, bounds));
    }
    index.adjust_for_text_edit(OverlayTextEdit::Delete {
        range: range(5, 25),
    });
    assert_eq!(index.range(a), Some(range(5, 10)));
    assert_eq!(index.range(b), Some(range(5, 10)));
    // GNU contracts begin coordinates in place. The topology still keeps A
    // before B even though B has a later attachment serial in the B+ keys.
    let identities = [a, b, moving].map(OverlayIdentity::of);
    for next in [
        range(3, 10),
        range(5, 10),
        range(4, 10),
        range(5, 10),
        range(8, 12),
    ] {
        let previous = index.range(moving).unwrap();
        let mut reference = index.gnu_order.clone();
        assert!(reference.remove(OverlayIdentity::of(moving)).is_ok());
        assert!(
            reference
                .insert_by(OverlayIdentity::of(moving), |existing| {
                    next.start().cmp(
                        &index
                            .intervals
                            .read()
                            .range_by_identity(existing)
                            .unwrap()
                            .start(),
                    )
                })
                .is_ok()
        );
        assert_eq!(index.move_to(moving, next), Some(previous));
        assert_eq!(
            index.gnu_order.subset_in_preorder(&identities),
            reference.subset_in_preorder(&identities),
            "a B+ successor must not substitute attachment order for GNU's \
             preserved equal-start topology after deletion, new range {next:?}"
        );
        index.assert_invariants();
    }
}

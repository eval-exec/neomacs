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
        resolutions <= moves * 4,
        "real local moves should resolve coordinates a bounded number of times, \
         rather than once per GNU mirror descent: {resolutions} resolutions for {moves} moves"
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
            assert!(reference.remove(identity));
            assert!(reference.insert_by(identity, |existing| {
                next.start().cmp(
                    &index
                        .intervals
                        .read()
                        .range_by_identity(existing)
                        .unwrap()
                        .start(),
                )
            }));
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

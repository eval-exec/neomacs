use super::*;
use neovm_core::window::{
    CompletedEobOverlayMotionExtent, DisplayRowEndSource, EobOverlayMotionCoverage,
};

fn emitter() -> (Context, WindowOutputEmitter) {
    let mut eval = Context::new();
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("abc");
    let frame = eval
        .frame_manager_mut()
        .create_frame("eob-motion", 400, 240, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    (
        eval,
        WindowOutputEmitter::new_speculative(frame, window, 0, 0.0, 0.0),
    )
}

fn finish(eval: &mut Context, emitter: WindowOutputEmitter) -> WindowDisplaySnapshot {
    emitter.finish_snapshot_with_geometry(
        eval,
        CellOrigin::default(),
        PresentedWindowRegions::default(),
        0,
        0,
        0,
    )
}

#[test]
fn eob_overlay_observation_without_completion_remains_incomplete() {
    let (mut eval, mut emitter) = emitter();
    let _started = emitter.begin_eob_overlay_motion(LispCharPos1::new(4), 7);
    let snapshot = finish(&mut eval, emitter);
    assert_eq!(
        snapshot.eob_overlay_motion,
        EobOverlayMotionCoverage::Incomplete {
            anchor: LispCharPos1::new(4),
        }
    );
}

#[test]
fn eob_overlay_completion_keeps_an_unpublished_empty_row_without_source_positions() {
    let (mut eval, mut emitter) = emitter();
    let started = emitter.begin_eob_overlay_motion(LispCharPos1::new(4), 7);
    emitter.complete_eob_overlay_motion(started, 9);
    let snapshot = finish(&mut eval, emitter);
    assert_eq!(
        snapshot.eob_overlay_motion,
        EobOverlayMotionCoverage::Complete(
            CompletedEobOverlayMotionExtent::from_physical_rows(LispCharPos1::new(4), 0, 2)
                .unwrap(),
        )
    );
    assert!(snapshot.rows.is_empty());
    assert!(snapshot.iter_points().next().is_none());
}

#[test]
fn eob_overlay_completion_refuses_a_reversed_interval() {
    let (mut eval, mut emitter) = emitter();
    let started = emitter.begin_eob_overlay_motion(LispCharPos1::new(4), 7);
    emitter.complete_eob_overlay_motion(started, 6);
    assert!(matches!(
        finish(&mut eval, emitter).eob_overlay_motion,
        EobOverlayMotionCoverage::Incomplete { .. }
    ));
}

#[test]
fn retained_eob_overlay_rows_do_not_manufacture_completion_evidence() {
    let (mut eval, mut emitter) = emitter();
    emitter.seed_cursor_only_body(
        vec![DisplayRowSnapshot {
            start_buffer_pos: Some(LispCharPos1::ONE),
            end_buffer_pos: Some(LispCharPos1::new(4)),
            end_source: DisplayRowEndSource::OverlayAfterString,
            ..Default::default()
        }],
        Vec::new(),
        None,
    );
    assert_eq!(
        finish(&mut eval, emitter).eob_overlay_motion,
        EobOverlayMotionCoverage::Incomplete {
            anchor: LispCharPos1::new(4),
        }
    );
}

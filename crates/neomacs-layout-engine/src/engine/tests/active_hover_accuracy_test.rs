//! Prepared output accuracy cannot authorize a different renderer-active hit.
//! Numeric accepted-output/input integration with the canonical core runtime;
//! the complete row producer is exercised by the separate #832 engine rows.
use neovm_core::buffer::{BufferId, EmacsBytePos, LispCharPos1};
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::eval::RedisplayHookPolicyGuard;
use neovm_core::window::display::WindowDisplayContextExt;
use neovm_core::window::geometry::PresentationId;
use neovm_core::window::{
    DisplayFrameExt, FrameId, MatrixRow0, WindowDisplaySnapshot, WindowEndRecord, WindowId,
    WindowPresentationSnapshot,
};

// These helpers use the real prepare/activate lifecycle and internal input
// queue, but supply numeric snapshots/hits rather than running a renderer.
fn prepare_hover_presentation(
    eval: &mut Context,
    frame: FrameId,
    window: WindowId,
    buffer: BufferId,
    generation: PresentationId,
) {
    let accuracy = eval.gnu_window_accuracy(frame, window, buffer).unwrap();
    let snapshot = WindowDisplaySnapshot {
        window_id: window,
        buffer_modiff: Some(accuracy.modified_tick),
        layout_freshness: eval.window_display_snapshot_freshness(frame, window, buffer),
        window_end_record: Some(WindowEndRecord::from_positions(
            LispCharPos1::new(4),
            EmacsBytePos::new(3),
            LispCharPos1::new(4),
            EmacsBytePos::new(3),
            MatrixRow0::ZERO,
        )),
        ..WindowDisplaySnapshot::default()
    };
    let state = eval.frame_manager_mut().get_mut(frame).unwrap();
    state.set_window_system(Some(neovm_value::value::Value::symbol("neo")));
    state
        .prepare_display_presentation(generation, vec![WindowPresentationSnapshot::live(snapshot)])
        .unwrap();
    assert!(
        state
            .find_window_mut(window)
            .unwrap()
            .set_gnu_redisplay_accuracy(generation, accuracy)
    );
    eval.buffer_manager()
        .get(buffer)
        .unwrap()
        .acknowledge_mode_line_clip();
}

fn observe_exact_hover(
    eval: &mut Context,
    frame: FrameId,
    window: WindowId,
    presentation: PresentationId,
) -> Option<neovm_value::value::Value> {
    use neomacs_display_protocol::{
        DisplayWindowId, FrameRect, GeometryPoint, InteractionProjection, LogicalPixels,
        PresentedHitIndex, PresentedHitQuery, PresentedHitRegion, PresentedRegionKind,
        PresentedTextPosition, RootSurfaceSpace,
    };
    use neovm_eval::emacs_core::eval::keyboard::InputEvent;
    let window = DisplayWindowId::new(window.0 as i64);
    let render_presentation = neomacs_display_protocol::PresentationId::new(presentation.get());
    let index = PresentedHitIndex::from_parts(
        render_presentation,
        vec![PresentedHitRegion::new(
            Some(window),
            PresentedRegionKind::TextBody,
            FrameRect::new(20.0, 10.0, 100.0, 40.0).unwrap(),
            0,
        )],
        vec![PresentedTextPosition::new(
            window,
            FrameRect::new(20.0, 10.0, 8.0, 16.0).unwrap(),
            1,
            0,
            0,
        )],
    )
    .unwrap();
    let point = InteractionProjection::settled(render_presentation)
        .map(GeometryPoint::<RootSurfaceSpace, LogicalPixels>::from_px(22.0, 12.0).unwrap())
        .unwrap();
    let hit = index
        .resolve(PresentedHitQuery::new(point))
        .unwrap()
        .unwrap();
    eval.command_loop_mut()
        .keyboard
        .pending_input_events
        .push_back(InputEvent::PresentedRegion {
            presentation: presentation.get(),
            hit: Some(hit),
            x: 22.0,
            y: 12.0,
            target_frame_id: frame.0,
        });
    eval.read_char_with_timeout(Some(std::time::Duration::ZERO))
        .unwrap();
    assert_eq!(
        eval.command_loop()
            .keyboard
            .kboard
            .presented_mouse_observation
            .unwrap()
            .presentation,
        presentation.get(),
        "the actual internal input route accepted the exact active observation"
    );
    eval.resolve_text_area_help_echo_event(frame, 22, 12)
}

#[test]
fn prepared_accuracy_cannot_authorize_an_older_active_text_hover_hit() {
    use neovm_value::value::list_to_vec;
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, frame, buffer, window) = super::incr_editing_frame("abc", 800, 240);
    super::realize_test_gui_frame(&mut eval, frame);
    eval.eval_str("(put-text-property 1 2 'help-echo \"A-tip\")")
        .unwrap();
    let a = PresentationId::new(1);
    let b = PresentationId::new(2);
    prepare_hover_presentation(&mut eval, frame, window, buffer, a);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .activate_display_presentation(a)
        .unwrap();
    let initial =
        observe_exact_hover(&mut eval, frame, window, a).expect("active A hover positive");
    assert_eq!(
        list_to_vec(&initial).unwrap()[2].as_str_owned().as_deref(),
        Some("A-tip")
    );

    eval.eval_str("(put-text-property 1 2 'help-echo \"B-tip\")")
        .unwrap();
    prepare_hover_presentation(&mut eval, frame, window, buffer, b);
    let state = eval.frame_manager().get(frame).unwrap();
    assert_eq!(state.active_presentation(), Some(a));
    assert_eq!(
        state
            .find_window(window)
            .unwrap()
            .display()
            .unwrap()
            .redisplay_output()
            .unwrap()
            .generation,
        b
    );
    assert_eq!(
        state
            .find_window(window)
            .unwrap()
            .gnu_text_hover_is_current(eval.buffer_manager()),
        Some(true),
        "B's source stamp alone would incorrectly admit the old A hit"
    );
    assert!(
        observe_exact_hover(&mut eval, frame, window, a).is_none(),
        "newly prepared B accuracy must not validate still-visible A geometry"
    );

    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .activate_display_presentation(b)
        .unwrap();
    let current =
        observe_exact_hover(&mut eval, frame, window, b).expect("active B hover positive");
    assert_eq!(
        list_to_vec(&current).unwrap()[2].as_str_owned().as_deref(),
        Some("B-tip")
    );
}

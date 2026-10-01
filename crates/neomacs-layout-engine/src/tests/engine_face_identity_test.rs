//! Real frame-output regressions for buffer-local face realization.
use super::*;

#[test]
fn scaled_buffer_synthetic_glyphs_have_their_own_realized_default_face() {
    let (mut eval, frame_id, buffer_id, window_id) = incr_editing_frame("abcdef\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame_id);
    eval.buffer_manager_mut()
        .get_mut(buffer_id)
        .unwrap()
        .set_buffer_local(
            "face-remapping-alist",
            Value::list(vec![Value::list(vec![
                Value::symbol("default"),
                Value::list(vec![Value::keyword("height"), Value::make_float(0.75)]),
                Value::symbol("default"),
            ])]),
        );
    let neovm_core::window::Window::Leaf { hscroll, .. } = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .unwrap()
        .find_window_mut(window_id)
        .unwrap()
    else {
        panic!("text window");
    };
    *hscroll = 3;

    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("frame output");
    let default = state.faces.get(&FaceId::new(0)).expect("frame default");
    let (marker, body) = marker_and_body(state, window_id);
    assert_ne!(
        marker.face_id,
        FaceId::new(0),
        "a remapped marker must not overwrite the canonical default face"
    );
    assert_eq!(
        marker.face_id, body.face_id,
        "GNU produces the marker with the buffer's remapped default face"
    );
    let marker_face = state
        .faces
        .get(&marker.face_id)
        .expect("published marker face");
    assert!((marker_face.font_size - default.font_size * 0.75).abs() < 0.001);
}

#[test]
fn hscroll_faces_remain_distinct_across_scaled_and_unscaled_windows() {
    let (mut eval, frame_id, _buffer_id, main) = incr_editing_frame("abcdef\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame_id);
    let side_buffer = eval.buffer_manager_mut().create_buffer("*treemacs-scale*");
    let buffer = eval.buffer_manager_mut().get_mut(side_buffer).unwrap();
    buffer.insert("abcdef\n");
    // Treemacs' configured text-scale amount -1.5, using the standard 1.2 step.
    buffer.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("default"),
            Value::list(vec![
                Value::keyword("height"),
                Value::make_float(0.7607257743127308),
            ]),
            Value::symbol("default"),
        ])]),
    );
    let side = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            main,
            neovm_core::window::SplitDirection::Horizontal,
            side_buffer,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .unwrap();
    for window_id in [main, side] {
        let neovm_core::window::Window::Leaf { hscroll, .. } = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .unwrap()
            .find_window_mut(window_id)
            .unwrap()
        else {
            panic!("text window");
        };
        *hscroll = 3;
    }
    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    for _ in 0..2 {
        engine.layout_frame_rust(&mut eval, frame_id);
        let state = engine.last_frame_display_state.as_ref().unwrap();
        let (main_marker, main_body) = marker_and_body(state, main);
        let (side_marker, side_body) = marker_and_body(state, side);
        assert_eq!(main_marker.face_id, FaceId::new(0));
        assert_eq!(
            state.faces[&main_marker.face_id].font_size,
            state.faces[&main_body.face_id].font_size
        );
        assert_ne!(side_marker.face_id, main_marker.face_id);
        assert_eq!(
            state.faces[&side_marker.face_id].font_size,
            state.faces[&side_body.face_id].font_size
        );
        assert!(
            (state.faces[&side_marker.face_id].font_size
                / state.faces[&main_marker.face_id].font_size
                - 0.7607258)
                .abs()
                < 0.001
        );
        activate_last_engine_presentation(&mut eval, &engine, frame_id);
    }
}

fn marker_and_body(
    state: &neomacs_display_protocol::glyph_matrix::FrameDisplayState,
    window_id: neovm_core::window::WindowId,
) -> (&Glyph, &Glyph) {
    let window = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window_id.0 as i64)
        .expect("scaled window output");
    let glyphs = || {
        window
            .matrix
            .rows
            .iter()
            .filter(|row| row.enabled && row.role == GlyphRowRole::Text)
            .flat_map(|row| row.glyphs[GlyphArea::Text.index()].iter())
    };
    let marker = glyphs()
        .find(|glyph| glyph.provenance == GlyphProvenance::LeftTruncation)
        .expect("horizontal-scroll marker must be present");
    let body = glyphs()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: 'e' }))
        .expect("visible source text");
    (marker, body)
}

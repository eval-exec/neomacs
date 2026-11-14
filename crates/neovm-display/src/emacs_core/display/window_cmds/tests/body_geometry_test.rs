use super::eval_with_gui_frame;

#[test]
fn chrome_height_queries_agree_with_absent_pre_redisplay_chrome() {
    let results = eval_with_gui_frame(
        r#"
        (setq mode-line-format nil header-line-format nil tab-line-format nil)
        (list (window-mode-line-height) (window-header-line-height) (window-tab-line-height))
    "#,
    );
    assert_eq!(results[1], "OK (0 0 0)");
}

#[test]
fn matrix_adjustment_uses_committed_body_instead_of_previous_presented_regions() {
    use super::*;
    use neomacs_display_protocol::glyph_matrix_capacity::required_gui_glyph_matrix_rows;
    use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};

    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let buffer = ctx.edit.buffers.create_buffer("*matrix-geometry*");
    ctx.edit.buffers.set_current(buffer);
    for name in ["mode-line-format", "header-line-format", "tab-line-format"] {
        ctx.edit
            .buffers
            .get_mut(buffer)
            .unwrap()
            .set_buffer_local(name, Value::NIL);
    }
    let frame_id = ctx
        .frame_manager_mut()
        .create_frame("matrix-geometry", 800, 600, buffer);
    let identity = GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::X11);
    identity.observe_opened_font_metrics(15, 5);
    let window_id = {
        let frame = ctx.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_display_identity(crate::window::FrameDisplayIdentity::Graphical(identity));
        frame.set_known_parameter(FrameParam::HorizontalScrollBars, Value::NIL);
        frame.selected_window
    };
    let (old_body, header_height) = {
        let frame = ctx.frame_manager().get(frame_id).unwrap();
        (
            frame.find_window(window_id).unwrap().bounds().height as i64,
            frame.char_height.max(1.0) as i64,
        )
    };
    publish_selected_gui_window_regions(
        &mut ctx,
        frame_id,
        1,
        crate::window::PresentedWindowRegions {
            outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, old_body as f32),
            text_body: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, old_body as f32),
            ..Default::default()
        },
    );
    crate::emacs_core::window_cmds::adjust_frame_glyph_matrices(&ctx, frame_id);
    let before_rows = ctx
        .frame_manager()
        .get(frame_id)
        .unwrap()
        .find_window(window_id)
        .unwrap()
        .display()
        .unwrap()
        .glyph_matrix_row_scale
        .allocated_rows();
    assert_eq!(
        before_rows,
        Some(required_gui_glyph_matrix_rows(
            old_body,
            0,
            20,
            std::num::NonZeroU32::MIN,
        ))
    );

    // A native frame resize clears its redisplay cache via minibuffer
    // repositioning. A buffer-local chrome change does not: it is the actual
    // persistent-cache case that the following GNU margin adjustment consumes.
    ctx.edit
        .buffers
        .get_mut(buffer)
        .unwrap()
        .set_buffer_local("header-line-format", Value::string("header"));
    let current_body = old_body - header_height;
    assert_ne!(current_body, old_body);
    assert_eq!(
        ctx.frame_manager()
            .get(frame_id)
            .unwrap()
            .redisplay_snapshot(window_id)
            .expect("completed redisplay remains installed after the buffer-local change")
            .regions
            .text_body
            .height,
        old_body as f32,
        "the fixture must retain the old completed body rectangle"
    );
    assert_eq!(
        crate::emacs_core::window_cmds::builtin_window_body_height(
            &mut ctx,
            &[Value::make_window(window_id.0), Value::T]
        )
        .unwrap(),
        Value::fixnum(old_body),
        "the ordinary query must still prefer the completed presentation"
    );
    let (_, adjustment_body) = crate::emacs_core::window_cmds::hook_window_body_dimensions(
        ctx.frame_manager(),
        &ctx.edit.buffers,
        frame_id,
        window_id,
    )
    .unwrap();
    assert_eq!(adjustment_body, current_body);

    // GNU apply_window_adjustment adjusts the complete frame even for a
    // horizontal margin change, consuming the current chrome geometry.
    assert_eq!(
        crate::emacs_core::window_cmds::builtin_set_window_margins(
            &mut ctx,
            vec![
                Value::make_window(window_id.0),
                Value::fixnum(1),
                Value::NIL
            ],
        )
        .unwrap(),
        Value::T,
    );
    let frame = ctx.frame_manager().get(frame_id).unwrap();
    let owner = &frame
        .find_window(window_id)
        .unwrap()
        .display()
        .unwrap()
        .glyph_matrix_row_scale;
    assert_eq!(
        owner.allocated_rows(),
        Some(required_gui_glyph_matrix_rows(
            current_body,
            0,
            20,
            owner.current()
        ))
    );
    assert_ne!(owner.allocated_rows(), before_rows);
}

use neomacs_app::initial_surface::{
    InitialBackgroundMode, InitialDisplayType, InitialEditorSurfaceSpec, InitialFrameFont,
    InitialFrameMetrics, prepare_initial_editor_surface,
};
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::eval::Context;
use neovm_core::window::FrameDisplayIdentity;

#[test]
fn gui_surface_reuses_gnu_startup_buffers_and_selects_one_visible_frame() {
    let mut evaluator = Context::new();
    let metrics = InitialFrameMetrics::new(800, 600, 8.0, 16.0, 16.0).expect("valid GUI metrics");
    let spec = InitialEditorSurfaceSpec::gui(
        metrics,
        FrameDisplayIdentity::default(),
        InitialDisplayType::Color,
        InitialBackgroundMode::Light,
        InitialFrameFont::new(Value::string("test-font"), Value::string("test-font")),
    );

    let surface = prepare_initial_editor_surface(&mut evaluator, spec);

    assert_eq!(
        evaluator.buffer_manager().current_buffer_id(),
        Some(surface.scratch_buffer())
    );
    assert_eq!(
        evaluator
            .buffer_manager()
            .find_buffer_by_name(" *Minibuf-0*"),
        Some(surface.minibuffer())
    );
    let frame = evaluator
        .frame_manager()
        .selected_frame()
        .expect("initial frame selected");
    assert_eq!(frame.id, surface.frame());
    assert_eq!((frame.width, frame.height), (800, 600));
    assert!(frame.visible);
    assert_eq!(frame.effective_window_system(), Some(Value::symbol("neo")));
}

#[test]
fn initial_frame_metrics_reject_non_renderable_geometry() {
    assert!(InitialFrameMetrics::new(0, 600, 8.0, 16.0, 16.0).is_err());
    assert!(InitialFrameMetrics::new(800, 600, f32::NAN, 16.0, 16.0).is_err());
    assert!(InitialFrameMetrics::new(800, 600, 8.0, -1.0, 16.0).is_err());
}

#[test]
fn named_initial_font_keeps_parameter_and_public_name_identical() {
    let mut evaluator = Context::new();
    let metrics = InitialFrameMetrics::new(320, 240, 8.0, 16.0, 16.0).unwrap();
    let spec = InitialEditorSurfaceSpec::gui(
        metrics,
        FrameDisplayIdentity::default(),
        InitialDisplayType::Color,
        InitialBackgroundMode::Light,
        InitialFrameFont::named("Monospace"),
    );

    prepare_initial_editor_surface(&mut evaluator, spec);

    let frame = evaluator.frame_manager().selected_frame().unwrap();
    let expected = Value::string("Monospace");
    assert_eq!(frame.parameter("font-parameter"), Some(expected));
    assert_eq!(frame.parameter("font"), Some(expected));
}

use neomacs_app::frontend_event::FrontendScaleFactor;
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
        FrontendScaleFactor::new(1.75).unwrap(),
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
    assert_eq!(frame.device_scale_factor, 1.75);
    assert!(frame.visibility.is_visible());
    assert_eq!(frame.effective_window_system(), Some(Value::symbol("neo")));
}

#[test]
fn initial_frame_metrics_reject_non_renderable_geometry() {
    assert!(InitialFrameMetrics::new(0, 600, 8.0, 16.0, 16.0).is_err());
    assert!(InitialFrameMetrics::new(800, 600, f32::NAN, 16.0, 16.0).is_err());
    assert!(InitialFrameMetrics::new(800, 600, 8.0, -1.0, 16.0).is_err());
}

#[test]
fn gui_surface_reserves_enabled_frame_bars_before_first_input() {
    let mut evaluator = Context::new();
    prepare_initial_editor_surface(
        &mut evaluator,
        InitialEditorSurfaceSpec::gui(
            InitialFrameMetrics::new(800, 600, 8.0, 16.0, 16.0).unwrap(),
            FrontendScaleFactor::new(1.0).unwrap(),
            FrameDisplayIdentity::default(),
            InitialDisplayType::Color,
            InitialBackgroundMode::Dark,
            InitialFrameFont::named("Monospace"),
        ),
    );
    let frame = evaluator.frame_manager_mut().selected_frame_mut().unwrap();
    // GUI toolbar buttons include image margins/relief: 39 px at this font.
    for (menu, tool, tab, expected_top) in [(0, 0, 1, 16.0), (1, 1, 1, 71.0), (0, 0, 0, 0.0)] {
        for (name, lines) in [
            ("menu-bar-lines", menu),
            ("tool-bar-lines", tool),
            ("tab-bar-lines", tab),
        ] {
            frame.set_parameter(Value::symbol(name), Value::fixnum(lines));
        }
        frame.sync_menu_bar_height_from_parameters();
        frame.sync_tool_bar_height_from_parameters();
        frame.sync_tab_bar_height_from_parameters();
        assert_eq!(frame.root_window().bounds().y, expected_top);
    }
}

#[test]
fn initial_tty_surface_reserves_chrome_only_for_interactive_sessions() {
    for (batch, expected_top) in [(true, 0.0), (false, 1.0)] {
        let mut evaluator = Context::new();
        prepare_initial_editor_surface(
            &mut evaluator,
            InitialEditorSurfaceSpec::tty(
                InitialFrameMetrics::new(80, 25, 1.0, 1.0, 1.0).unwrap(),
                batch,
            ),
        );
        let frame = evaluator.frame_manager_mut().selected_frame_mut().unwrap();
        assert_eq!(frame.root_window().bounds().y, expected_top);
        // Visibility is independent of whether bars participate in layout.
        frame.visibility = neovm_core::window::FrameVisibility::Iconified;
        frame.sync_window_area_bounds();
        assert_eq!(frame.root_window().bounds().y, expected_top);
    }
}

#[test]
fn named_initial_font_keeps_parameter_and_public_name_identical() {
    let mut evaluator = Context::new();
    let metrics = InitialFrameMetrics::new(320, 240, 8.0, 16.0, 16.0).unwrap();
    let spec = InitialEditorSurfaceSpec::gui(
        metrics,
        FrontendScaleFactor::new(1.0).unwrap(),
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

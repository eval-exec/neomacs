use std::path::Path;

use neomacs_app::frontend_event::FrontendScaleFactor;
use neomacs_app::initial_surface::{
    InitialBackgroundMode, InitialDisplayType, InitialEditorSurfaceSpec, InitialFrameFont,
    InitialFrameMetrics, prepare_initial_editor_surface,
};
use neomacs_app::startup::{InteractiveGuiStartup, configure_interactive_gui_startup};
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::value::list_to_vec;

#[test]
fn interactive_gui_startup_materializes_host_identity_and_gnu_command_line_state() {
    let mut evaluator = Context::new();
    let metrics = InitialFrameMetrics::new(320, 240, 8.0, 16.0, 16.0).unwrap();
    let surface = prepare_initial_editor_surface(
        &mut evaluator,
        InitialEditorSurfaceSpec::gui(
            metrics,
            FrontendScaleFactor::ONE,
            Default::default(),
            InitialDisplayType::Color,
            InitialBackgroundMode::Light,
            InitialFrameFont::named("Monospace"),
        ),
    );
    let startup = InteractiveGuiStartup::new(
        "neomacs-android",
        Path::new("/data/app/lib"),
        Path::new("/data/user/0/neomacs/files"),
    )
    .with_arguments(["--no-splash"]);

    configure_interactive_gui_startup(&mut evaluator, surface, &startup).unwrap();

    assert_eq!(
        evaluator.obarray().symbol_value("command-line-processed"),
        Some(&Value::NIL),
    );
    assert_eq!(
        evaluator.obarray().symbol_value("noninteractive"),
        Some(&Value::NIL),
    );
    assert_eq!(
        evaluator
            .obarray()
            .symbol_value("invocation-name")
            .and_then(|value| value.as_utf8_str()),
        Some("neomacs-android"),
    );
    assert_eq!(
        evaluator
            .obarray()
            .symbol_value("invocation-directory")
            .and_then(|value| value.as_utf8_str()),
        Some("/data/app/lib/"),
    );
    assert_eq!(
        evaluator
            .eval_str("default-directory")
            .unwrap()
            .as_str_owned()
            .as_deref(),
        Some("/data/user/0/neomacs/files/"),
    );
    assert_eq!(
        evaluator.obarray().symbol_value("frame-initial-frame"),
        Some(&Value::make_frame(surface.frame().0)),
    );
    let terminal_frame = evaluator
        .obarray()
        .symbol_value("terminal-frame")
        .and_then(|value| value.as_frame_id())
        .expect("interactive GUI startup has GNU's temporary terminal frame");
    assert_ne!(terminal_frame, surface.frame().0);
    let terminal_frame = evaluator
        .frame_manager()
        .get(neovm_core::window::FrameId(terminal_frame))
        .unwrap();
    assert!(!terminal_frame.visible);
    assert!(terminal_frame.effective_window_system().is_none());
    assert_eq!(
        list_to_vec(&evaluator.eval_str("command-line-args").unwrap()).unwrap(),
        vec![
            Value::string("neomacs-android"),
            Value::string("--no-splash"),
        ],
    );
}

use neomacs_app::frontend_event::FrontendScaleFactor;
use neomacs_app::initial_surface::{
    InitialBackgroundMode, InitialDisplayType, InitialEditorSurfaceSpec, InitialFrameFont,
    InitialFrameMetrics, prepare_initial_editor_surface,
};
use neomacs_app::presentation::{EditorPresentationRuntime, PresentationMetrics};
use neomacs_layout_engine::font::sizing::FontSizing;
use neovm_core::emacs_core::{Value, eval::Context};

fn scalable_session() -> (Context, EditorPresentationRuntime) {
    let mut eval = Context::new();
    prepare_initial_editor_surface(
        &mut eval,
        InitialEditorSurfaceSpec::gui(
            InitialFrameMetrics::new(800, 600, 8.0, 16.0, 16.0).unwrap(),
            FrontendScaleFactor::ONE,
            Default::default(),
            InitialDisplayType::Color,
            InitialBackgroundMode::Dark,
            InitialFrameFont::named("Monospace"),
        ),
    );
    let runtime =
        EditorPresentationRuntime::new(PresentationMetrics::Scalable(FontSizing::logical()));
    runtime.install_evaluator_query_hooks(&mut eval);
    // Context::new has not loaded faces.el. Seed the same default-face
    // attributes that normal startup Lisp supplies.
    eval.eval_str(
        r##"
        (internal-set-lisp-face-attribute 'default :family "Monospace" nil)
        (internal-set-lisp-face-attribute 'default :height 120 nil)
    "##,
    )
    .unwrap();
    (eval, runtime)
}

#[test]
fn scalable_session_exposes_its_default_font_to_lisp() {
    let (mut eval, _runtime) = scalable_session();
    assert_eq!(
        eval.eval_str("(stringp (face-font 'default))").unwrap(),
        Value::T
    );
    assert_eq!(
        eval.eval_str(
            r##"
        (let ((info (font-info (face-font 'default))))
          (and (vectorp info) (> (aref info 3) 0)))
    "##
        )
        .unwrap(),
        Value::T
    );
}

#[test]
fn fonts_do_not_claim_window_or_shader_ownership() {
    let (mut eval, _runtime) = scalable_session();
    assert!(eval.display_host.is_none());
    assert_eq!(
        eval.eval_str("(neomacs-surface-available-p)").unwrap(),
        Value::NIL
    );
    // Font queries must not redirect the existing synchronous no-window-host
    // geometry path into an unacknowledged native resize request.
    eval.eval_str("(set-frame-size nil 640 480 t)").unwrap();
    assert_eq!(
        eval.eval_str("(frame-text-width)").unwrap(),
        Value::fixnum(640)
    );
}

#[test]
fn lisp_font_queries_are_reentrant_during_redisplay() {
    let (mut eval, runtime) = scalable_session();
    eval.eval_str(
        r##"
      (setq font-query-during-layout nil)
      (setq mode-line-format
        '((:eval (progn
                   (setq font-query-during-layout (font-info (face-font 'default)))
                   "font-query"))))
    "##,
    )
    .unwrap();
    let frame = eval.frame_manager().selected_frame().unwrap().id;
    assert!(
        runtime
            .prepare_frame(
                &mut eval,
                frame,
                neomacs_app::presentation::FrameLayoutPurpose::Snapshot
            )
            .is_some()
    );
    assert_eq!(
        eval.eval_str("(vectorp font-query-during-layout)").unwrap(),
        Value::T
    );
}

#[test]
fn cell_grid_session_does_not_claim_graphical_fonts() {
    let mut eval = Context::new();
    let runtime = EditorPresentationRuntime::new(PresentationMetrics::CellGrid);
    runtime.install_evaluator_query_hooks(&mut eval);
    assert!(eval.display_host.is_none());
}

#[test]
fn native_display_retains_its_font_provider() {
    use neovm_core::emacs_core::display_host::{AvailableFontFamilyName, DisplayHost};
    use neovm_core::emacs_core::eval::GuiFrameHostRequest;
    use neovm_core::window::FrameId;
    struct NativeDisplay;
    impl DisplayHost for NativeDisplay {
        fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn list_font_families(
            &mut self,
            _: FrameId,
        ) -> Result<Vec<AvailableFontFamilyName>, String> {
            Ok(vec![
                AvailableFontFamilyName::from_utf8("Native Display Font").unwrap(),
            ])
        }
    }
    let mut eval = Context::new();
    eval.display_host = Some(Box::new(NativeDisplay));
    let runtime =
        EditorPresentationRuntime::new(PresentationMetrics::Scalable(FontSizing::logical()));
    runtime.install_evaluator_query_hooks(&mut eval);
    assert_eq!(
        eval.eval_str(r##"(equal (font-family-list) '("Native Display Font"))"##)
            .unwrap(),
        Value::T
    );
}

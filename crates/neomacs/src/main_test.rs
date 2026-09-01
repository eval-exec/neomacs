use super::frame_layout::{
    REDISPLAY_RUNTIME, current_layout_frame_id,
    install_tty_redisplay_callback as maybe_install_tty_redisplay_callback,
};
use super::image_catalog::{AsyncImageCatalog, wait_for_image_metadata};
use super::tty_frontend::{TtyPopupDisplayHost, TtyTerminalHost};
use super::tty_init::{
    default_controlling_tty_name, detect_tty_background_mode, should_enable_live_tty_io,
};
use super::{
    BOOTSTRAP_CORE_FEATURES, BootstrapDisplayConfig, DumpImageKind, EarlyCliAction, FontSizing,
    FrontendKind, Interactivity, PrimaryWindowDisplayHost, PrimaryWindowSize, RuntimeMode,
    StartupOptions, adopt_existing_primary_gui_frame, bootstrap_buffers,
    bootstrap_default_font_name, bootstrap_frame_metrics, bootstrap_frame_metrics_for_font_sizing,
    bootstrap_frame_metrics_for_frontend, bootstrap_gui_display_config,
    bootstrap_tty_display_config, classify_early_cli_action, configure_gnu_startup_state,
    gui_display_identity, gui_frame_scale_from_observation, load_neomacs_gui_term_layer,
    parse_startup_options, publish_gui_frame, raw_dump_loadup_invocation, raw_loadup_command_line,
    render_fingerprint_text, render_help_text, render_startup_image_error, render_version_text,
    run_gnu_startup, runtime_mode_from_program_name, source_bootstrap_loadup_invocation,
    startup_dimensions, sync_live_gui_frame_titles, sync_selected_gui_chrome_state,
};
use neomacs_display_protocol::WebViewId;
use neomacs_display_runtime::render_thread::{
    ImageDecodeTerminal, ImageRenderState, SharedImageRenderState,
};
use neomacs_display_runtime::thread_comm::{
    AssetCommand, ClipboardCommand, ClipboardSelection, ConfigCommand, FrameRef,
    FrameShaderAvailability, LifecycleCommand, MediaSource, RenderCommand,
    SharedRenderCapabilities, UiCommand, WindowCommand, WindowFullscreenMode,
};
#[cfg(feature = "neo-term")]
use neomacs_display_runtime::{
    terminal::{TerminalDisplayTarget, new_shared_terminals},
    thread_comm::TerminalCommand,
};
use neomacs_layout_engine::font::metrics::FontMetricsService;
use neomacs_layout_engine::font::sizing::face_height_to_pixels;
use neomacs_webview::{NavigationTarget, WebViewCommand};
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::GuiFrameHostRequest;
use neovm_core::emacs_core::Value;
#[cfg(feature = "neo-term")]
use neovm_core::emacs_core::display_host::{
    TerminalCreateRequest, TerminalDisplayTarget as CoreTerminalDisplayTarget,
    TerminalFloatPlacement, TerminalGridSize,
};
use neovm_core::emacs_core::error::EvalError;
use neovm_core::emacs_core::eval::{
    PopupMenuEntry, PopupMenuRequest, ShaderSurfaceLanguage, ShaderSurfaceUniformInit,
    VideoResolveRequest, VideoResolveSource, WebKitResolveRequest, WebKitResolveSource,
};
use neovm_core::emacs_core::image_catalog::{AxisSize, ImageRotation, ImageSizeSpec};
use neovm_core::emacs_core::image_catalog::{
    ImageAnimationInvalidation, ImageCatalog, ImageColorContext, ImageDataSource, ImageFrameIndex,
    ImageId, ImageLoadAttempt, ImageLoadToken, ImageLookup, ImageResolveRequest,
    ImageResolveSource, ImageSpecIdentity, ResolvedImageMetadata,
};
use neovm_core::emacs_core::intern::intern;
use neovm_core::emacs_core::load::{
    LoadupDumpMode, LoadupInvocation, create_bootstrap_evaluator_cached_with_features,
    create_bootstrap_evaluator_with_features,
};
use neovm_core::emacs_core::print_value_with_eval;
use neovm_core::emacs_core::terminal::pure::TerminalHost;
use neovm_core::emacs_core::terminal::pure::{TerminalRuntimeConfig, configure_terminal_runtime};
use neovm_core::emacs_core::value::list_to_vec;
use neovm_core::face::FaceHeight;
use neovm_core::heap_types::LispString;
use neovm_core::window::{
    FrameFullscreen, FrameId, FrameParam, GuiFrameGeometryHints, WindowId,
    default_gui_tool_bar_line_height,
};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn gui_display() -> BootstrapDisplayConfig {
    let observation = neomacs_display_protocol::DisplayObservation::X11(
        neomacs_display_protocol::X11DisplayObservation::new(
            neomacs_display_protocol::XServerKind::Unknown,
            None,
            None,
            neomacs_display_protocol::DeviceScale::ONE,
        ),
    );
    bootstrap_gui_display_config(
        Interactivity::Interactive,
        gui_frame_scale_from_observation(observation),
    )
}

fn test_image_load(image: u32, attempt: u64) -> ImageLoadToken {
    ImageLoadToken::new(
        ImageId::new(image),
        ImageLoadAttempt::new(attempt).expect("non-zero test image load attempt"),
    )
}

#[test]
fn gui_display_identity_records_the_native_backend() {
    let wayland = gui_display_identity(Some("wayland-7"), Some(":42"));
    assert_eq!(wayland.native_display(), Some("wayland-7"));
    assert_eq!(wayland.x_display(), None);

    let x11 = gui_display_identity(None, Some(":42"));
    assert_eq!(x11.native_display(), Some(":42"));
    assert_eq!(x11.x_display(), Some(":42"));
}

fn shared_primary_window_size(width: u32, height: u32) -> Arc<Mutex<PrimaryWindowSize>> {
    Arc::new(Mutex::new(PrimaryWindowSize { width, height }))
}

thread_local! {
    static IMAGE_SPEC_TEST_CONTEXT: Context = Context::new();
}

fn test_image_spec_identity(label: &str) -> ImageSpecIdentity {
    let spec = IMAGE_SPEC_TEST_CONTEXT.with(|_| {
        Value::list(vec![
            Value::symbol("image"),
            Value::keyword(":type"),
            Value::symbol("png"),
            Value::keyword(":file"),
            Value::string(label),
        ])
    });
    ImageSpecIdentity::from_lisp_spec(&spec).expect("test image spec")
}

#[test]
fn layout_purpose_is_the_only_owner_of_pending_scroll_consumption() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.buffer_manager_mut()
        .get_mut(buffer_id)
        .expect("buffer")
        .insert(&"scrollable line\n".repeat(40));
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("layout-purpose-scroll", 800, 320, buffer_id);
    eval.accumulate_pending_pixel_scroll(frame_id, 3.5);

    let snapshot = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Snapshot,
    )
    .expect("snapshot layout");
    let _ = snapshot.discard(&mut eval);
    assert_eq!(
        eval.pending_pixel_scroll_for_frame(frame_id),
        Some(3.5),
        "snapshot and logical-query layout must preserve user input for redisplay"
    );

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("redisplay layout");
    let _ = redisplay.discard(&mut eval);
    assert_eq!(eval.pending_pixel_scroll_for_frame(frame_id), None);
}

fn initialized_redisplay_test_frame(
    frame_name: &str,
    width: u32,
    height: u32,
    line: &str,
    line_count: usize,
) -> (Context, neovm_core::buffer::BufferId, FrameId, WindowId) {
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.buffer_manager_mut()
        .get_mut(buffer_id)
        .expect("buffer")
        .insert(&line.repeat(line_count));
    let frame_id = eval
        .frame_manager_mut()
        .create_frame(frame_name, width, height, buffer_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .initial = false;

    super::frame_layout::install_window_layout_query_fn(&mut eval);
    let initial = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("initial layout");
    let _ = initial.discard(&mut eval);
    (eval, buffer_id, frame_id, window_id)
}

/// Issue #292: redisplay owns the layout engine while GNU's scroll hook runs,
/// but the hook is allowed to ask a synchronous display question.  Exercise
/// the production redisplay/query seam rather than LayoutEngine directly so a
/// nested borrow of the frontend-owned engine is observable.
#[test]
fn scroll_hook_can_query_window_end_through_frontend_layout_seam() {
    let (mut eval, buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "scroll-hook-query",
        800,
        320,
        "scroll-hook layout fixture\n",
        200,
    );
    let (buffer_z_char, buffer_z_byte) = {
        let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
        (
            buffer.point_max_char_pos().to_lisp(),
            buffer.point_max_emacs_byte_pos(),
        )
    };
    let window = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .and_then(|frame| frame.find_window_mut(window_id))
        .expect("window");
    window.set_window_end_from_positions(
        buffer_z_char,
        buffer_z_byte,
        neovm_core::buffer::LispCharPos1::new(2),
        neovm_core::buffer::EmacsBytePos::new(1),
        0,
    );
    window.invalidate_window_end();

    eval.eval_str(
        "(progn
           (setq neomacs-issue-292-result nil)
           (setq window-scroll-functions
                 (list (lambda (window start)
                         (setq neomacs-issue-292-result
                               (list start
                                     (window-end window t)
                                     (window-end window nil))))))
           (goto-char 400))",
    )
    .expect("install scroll hook");
    eval.eval_form(Value::list(vec![
        Value::symbol("set-window-start"),
        Value::make_window(window_id.0),
        Value::fixnum(400),
    ]))
    .expect("force a redisplay start");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("reentrant scroll-hook layout");
    let _ = redisplay.discard(&mut eval);

    let result = eval
        .eval_str("neomacs-issue-292-result")
        .expect("inspect hook result");
    let fields = list_to_vec(&result).expect("hook result list");
    assert_eq!(
        fields.first().and_then(|value| value.as_fixnum()),
        Some(400)
    );
    assert!(
        fields
            .get(1)
            .and_then(|value| value.as_fixnum())
            .is_some_and(|end| end >= 400),
        "window-end UPDATE must be available from window-scroll-functions: {result}"
    );
    assert_eq!(
        fields.get(2).and_then(|value| value.as_fixnum()),
        Some(2),
        "GNU's stack-local Fwindow_end iterator must not publish a discarded redisplay end"
    );
}

#[test]
fn scroll_hook_changed_start_is_final_without_running_the_hook_twice() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "scroll-hook-start",
        80,
        24,
        "scroll-hook start fixture\n",
        500,
    );

    eval.eval_str(
        "(progn
           (setq neomacs-scroll-hook-count 0)
           (setq neomacs-scroll-hook-inside-start nil)
           (setq window-scroll-functions
                 (list (lambda (window _start)
                         (setq neomacs-scroll-hook-count
                               (1+ neomacs-scroll-hook-count))
                         (if (= neomacs-scroll-hook-count 1)
                             (progn
                               ;; NOFORCE is the adversarial case: the resume
                               ;; must still consume this exact hook-reread
                               ;; start before automatic viewport policy runs.
                               (set-window-start window 500 t)
                               (setq neomacs-scroll-hook-inside-start
                                     (window-start window)))))))
           (goto-char 700))",
    )
    .expect("install start-changing scroll hook");
    eval.eval_form(Value::list(vec![
        Value::symbol("set-window-start"),
        Value::make_window(window_id.0),
        Value::fixnum(400),
    ]))
    .expect("force the first start");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout after hook changed its start");
    let _ = redisplay.discard(&mut eval);

    assert_eq!(
        eval.eval_str("neomacs-scroll-hook-count")
            .expect("hook count")
            .as_fixnum(),
        Some(1),
        "GNU rereads the hook-updated start and continues; it does not enter a second scroll-hook site"
    );
    assert_eq!(
        eval.eval_str("neomacs-scroll-hook-inside-start")
            .expect("start observed inside hook")
            .as_fixnum(),
        Some(500),
        "set-window-start must update the live marker before the hook returns"
    );
    assert_eq!(
        eval.eval_form(Value::list(vec![
            Value::symbol("window-start"),
            Value::make_window(window_id.0),
        ]))
        .expect("final window start")
        .as_fixnum(),
        Some(500),
        "the hook-updated start must own the final presentation"
    );
}

#[test]
fn scroll_hook_resume_survives_an_earlier_windows_intervening_hook() {
    let (mut eval, buffer_id, frame_id, earlier_window) = initialized_redisplay_test_frame(
        "scroll-hook-multiple-windows",
        800,
        320,
        "multiple window scroll hook fixture\n",
        300,
    );
    let later_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            earlier_window,
            neovm_core::window::SplitDirection::Horizontal,
            buffer_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");
    let split_layout = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout split frame");
    let _ = split_layout.discard(&mut eval);

    eval.set_variable(
        "neomacs-earlier-scroll-window",
        Value::make_window(earlier_window.0),
    );
    eval.set_variable(
        "neomacs-later-scroll-window",
        Value::make_window(later_window.0),
    );
    eval.eval_str(
        "(progn
           (setq neomacs-earlier-scroll-count 0)
           (setq neomacs-later-scroll-count 0)
           (setq neomacs-earlier-scroll-start-seen nil)
           (setq neomacs-multi-scroll-log nil)
           (setq window-scroll-functions
                 (list
                  (lambda (window start)
                    (setq neomacs-multi-scroll-log
                          (cons (list window start)
                                neomacs-multi-scroll-log))
                    (cond
                     ((eq window neomacs-later-scroll-window)
                      (setq neomacs-later-scroll-count
                            (1+ neomacs-later-scroll-count))
                      (if (= neomacs-later-scroll-count 1)
                          (progn
                            (set-window-start neomacs-earlier-scroll-window 60)
                            (setq neomacs-earlier-scroll-start-seen
                                  (window-start neomacs-earlier-scroll-window))
                            (set-window-start neomacs-later-scroll-window 120))))
                     ((eq window neomacs-earlier-scroll-window)
                      (setq neomacs-earlier-scroll-count
                            (1+ neomacs-earlier-scroll-count)))))))
           ;; Keep the earlier leaf's EOB viewport stable.  The scenario under
           ;; test starts when the later leaf mutates an already-completed
           ;; sibling, not when changing selected-window point first scrolls
           ;; that sibling on its own.
           (select-window neomacs-later-scroll-window)
           (goto-char 200))",
    )
    .expect("install multi-window scroll hooks");
    eval.eval_form(Value::list(vec![
        Value::symbol("set-window-start"),
        Value::make_window(later_window.0),
        Value::fixnum(80),
    ]))
    .expect("force the later window start");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("multi-window resumed layout");
    let _ = redisplay.discard(&mut eval);
    let hook_log = eval
        .eval_str("(format \"%S\" (nreverse neomacs-multi-scroll-log))")
        .expect("multi-window hook log")
        .as_str_owned()
        .expect("hook log string");
    let earlier_start_seen = eval
        .eval_str("neomacs-earlier-scroll-start-seen")
        .expect("earlier start observed by later hook");

    assert_eq!(
        eval.eval_str("neomacs-later-scroll-count")
            .expect("later hook count")
            .as_fixnum(),
        Some(1),
        "an earlier window's intervening suspension must not discard the later window's exact acknowledgement; log={hook_log}"
    );
    assert_eq!(
        eval.eval_str("neomacs-earlier-scroll-count")
            .expect("earlier hook count")
            .as_fixnum(),
        Some(0),
        "GNU does not revisit an earlier leaf after a later leaf's hook mutates it; log={hook_log}, earlier_start_seen={earlier_start_seen}"
    );

    let followup = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("follow-up redisplay for the earlier invalidated leaf");
    let _ = followup.discard(&mut eval);
    assert_eq!(
        eval.eval_str("neomacs-earlier-scroll-count")
            .expect("earlier follow-up hook count")
            .as_fixnum(),
        Some(1),
        "the mutation remains live and is consumed when the next redisplay reaches that earlier leaf"
    );
}

#[test]
fn scroll_hook_window_tree_mutation_restarts_the_frame_plan() {
    let (mut eval, buffer_id, frame_id, earlier_window) = initialized_redisplay_test_frame(
        "scroll-hook-window-tree",
        800,
        320,
        "scroll hook topology fixture\n",
        300,
    );
    let later_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            earlier_window,
            neovm_core::window::SplitDirection::Horizontal,
            buffer_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");
    let split_layout = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout split frame");
    let _ = split_layout.discard(&mut eval);

    eval.set_variable(
        "neomacs-topology-delete-window",
        Value::make_window(earlier_window.0),
    );
    eval.set_variable(
        "neomacs-topology-hook-window",
        Value::make_window(later_window.0),
    );
    eval.eval_str(
        "(progn
           (setq neomacs-topology-hook-count 0)
           (setq neomacs-topology-resume-start (- (point-max) 5))
           (setq window-scroll-functions
                 (list (lambda (window _start)
                         (if (eq window neomacs-topology-hook-window)
                             (progn
                               (setq neomacs-topology-hook-count
                                     (1+ neomacs-topology-hook-count))
                               (set-window-start
                                neomacs-topology-hook-window
                                neomacs-topology-resume-start t)
                               (delete-window-internal
                                neomacs-topology-delete-window))))))
           (select-window neomacs-topology-hook-window)
           (goto-char neomacs-topology-resume-start)
           (set-window-start neomacs-topology-hook-window 120))",
    )
    .expect("install topology-changing scroll hook");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout after scroll hook changed the window tree");
    assert_eq!(
        eval.eval_str("neomacs-topology-hook-count")
            .expect("hook count")
            .as_fixnum(),
        Some(1)
    );
    let expected_start = eval
        .eval_str("neomacs-topology-resume-start")
        .expect("hook-selected resume start")
        .as_fixnum();
    assert_eq!(
        eval.eval_form(Value::list(vec![
            Value::symbol("window-start"),
            Value::make_window(later_window.0),
        ]))
        .expect("post-hook surviving window start")
        .as_fixnum(),
        expected_start,
        "GNU rereads the hook-updated start before reacting to the topology mutation"
    );
    assert!(
        redisplay
            .window_infos
            .iter()
            .all(|info| info.window_id.get() != earlier_window.0 as i64),
        "a discarded pre-hook leaf must not survive in the accepted presentation"
    );
    assert!(
        redisplay
            .window_infos
            .iter()
            .any(|info| info.window_id.get() == later_window.0 as i64)
    );
    let _ = redisplay.discard(&mut eval);
}

#[test]
fn gui_chrome_lisp_runs_once_before_window_scroll_hooks_across_physical_retry() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "gui-tool-bar-before-window-rows",
        800,
        320,
        "tool bar order fixture\n",
        300,
    );
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        // Deliberately underestimate the tab bar so the first physical pass
        // measures a different height and retries. Logical tab/tool Lisp must
        // still run exactly once, before any leaf scroll hook.
        frame.tab_bar_height = 1;
        frame.tool_bar_height = 0;
    }
    eval.set_variable(
        "neomacs-tool-bar-order-window",
        Value::make_window(window_id.0),
    );
    eval.eval_str(
        "(progn
           (setq neomacs-redisplay-order nil)
           (fset 'tab-bar-make-keymap-1
                 (lambda ()
                   (setq neomacs-redisplay-order
                         (append neomacs-redisplay-order '(tab-bar)))
                   (modify-frame-parameters nil '((tool-bar-lines . 1)))
                   nil))
           (setq tool-bar-map (make-sparse-keymap))
           (define-key tool-bar-map [neomacs-order]
             '(menu-item \"Order\" ignore
                         :visible
                         (progn
                           (setq neomacs-redisplay-order
                                 (append neomacs-redisplay-order '(tool-bar)))
                           t)))
           (setq window-scroll-functions
                 (list (lambda (window _start)
                         (if (eq window neomacs-tool-bar-order-window)
                             (setq neomacs-redisplay-order
                                   (append neomacs-redisplay-order
                                           '(scroll-hook)))))))
           (select-window neomacs-tool-bar-order-window)
           (goto-char 200)
           (set-window-start neomacs-tool-bar-order-window 120))",
    )
    .expect("install GUI chrome and scroll-hook order probes");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout GUI frame");
    assert_eq!(
        eval.eval_str("(prin1-to-string neomacs-redisplay-order)")
            .expect("redisplay callback order")
            .as_runtime_string_owned()
            .expect("printed callback order"),
        "(tab-bar tool-bar scroll-hook)",
        "GNU prepare_menu_bars updates tab/tool bars before redisplay_windows fills any window rows"
    );
    let _ = redisplay.discard(&mut eval);
}

#[test]
fn scroll_hook_runs_before_status_line_lisp_and_status_line_runs_once() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "scroll-hook-order",
        800,
        320,
        "scroll-hook ordering fixture\n",
        200,
    );
    eval.eval_str(
        "(progn
           (setq neomacs-scroll-order nil)
           (set (make-local-variable 'mode-line-format)
                '((:eval
                   (progn
                     (setq neomacs-scroll-order
                           (cons 'mode-line neomacs-scroll-order))
                     \"ORDER\"))))
           (setq window-scroll-functions
                 (list (lambda (_window _start)
                         (setq neomacs-scroll-order
                               (cons 'scroll-hook neomacs-scroll-order)))))
           (goto-char 400))",
    )
    .expect("install ordering probes");
    eval.eval_form(Value::list(vec![
        Value::symbol("set-window-start"),
        Value::make_window(window_id.0),
        Value::fixnum(400),
    ]))
    .expect("force a redisplay start");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("ordered scroll-hook layout");
    let _ = redisplay.discard(&mut eval);

    assert_eq!(
        eval.eval_str("(format \"%S\" neomacs-scroll-order)")
            .expect("ordering log")
            .as_str_owned()
            .expect("ordering string"),
        "(mode-line scroll-hook)",
        "GNU runs the scroll hook before try_window/status-line evaluation, and the rejected physical attempt must not duplicate status-line Lisp"
    );
}

#[test]
fn status_line_layout_mutation_rejects_rows_built_from_the_old_geometry() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "status-line-live-inputs",
        800,
        320,
        "status line freshness fixture\n",
        200,
    );
    eval.eval_str(
        "(progn
           (setq neomacs-status-line-layout-count 0)
           (set (make-local-variable 'mode-line-format)
                '((:eval
                   (progn
                     (setq neomacs-status-line-layout-count
                           (1+ neomacs-status-line-layout-count))
                     (if (= neomacs-status-line-layout-count 1)
                         (set-window-margins (selected-window) 5 2))
                     \"FRESH\")))))",
    )
    .expect("install layout-mutating status line");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout must retry from status-line-mutated inputs");
    let info = redisplay
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == window_id.0 as i64)
        .expect("selected window output");
    let neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Complete {
        regions, ..
    } = info.geometry
    else {
        panic!("status-line window must have complete presented geometry")
    };
    assert_eq!(
        (regions.left_margin_columns, regions.right_margin_columns),
        (5, 2),
        "rows built before the mutation must not be tagged with post-mutation freshness"
    );
    assert!(
        eval.eval_str("neomacs-status-line-layout-count")
            .expect("status-line count")
            .as_fixnum()
            .is_some_and(|count| count >= 2),
        "one rejected physical attempt and one accepted retry must evaluate chrome"
    );
    let _ = redisplay.discard(&mut eval);
}

#[test]
fn rejected_chrome_convergence_restores_the_last_accepted_window_end() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "status-line-window-end-rollback",
        800,
        320,
        "speculative end rollback fixture\n",
        500,
    );
    let accepted_end = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(window_id.0),
            Value::NIL,
        ]))
        .expect("accepted window end")
        .as_fixnum()
        .expect("accepted end position");
    eval.eval_str(
        "(progn
           (setq neomacs-rejected-end-attempt 0)
           (set (make-local-variable 'mode-line-format)
                '((:eval
                   (progn
                     (setq neomacs-rejected-end-attempt
                           (1+ neomacs-rejected-end-attempt))
                     (set-window-margins
                      (selected-window) neomacs-rejected-end-attempt 0)
                     \"RETRY\"))))
           (set-window-start (selected-window) 300))",
    )
    .expect("install non-converging status line");

    let rejected = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    );
    let rejected_attempts = eval
        .eval_str("neomacs-rejected-end-attempt")
        .expect("rejected attempt count");
    assert!(
        rejected.is_none(),
        "continuously changing layout inputs must exhaust the bounded coordinator; attempts={rejected_attempts}"
    );
    let after = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(window_id.0),
            Value::NIL,
        ]))
        .expect("window end after rejected attempts")
        .as_fixnum()
        .expect("stored end position");
    assert_eq!(
        after, accepted_end,
        "a rejected body's provisional end is visible to its chrome, but must never become previous accepted viewport evidence"
    );
}

#[test]
fn later_sibling_failure_restores_every_earlier_provisional_window_end() {
    let (mut eval, buffer_id, frame_id, earlier_window) = initialized_redisplay_test_frame(
        "frame-window-end-rollback",
        800,
        320,
        "frame-wide provisional end fixture\n",
        500,
    );
    let later_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            earlier_window,
            neovm_core::window::SplitDirection::Horizontal,
            buffer_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split frame");
    let accepted = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("establish both accepted window ends");
    let _ = accepted.discard(&mut eval);
    let accepted_earlier_end = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(earlier_window.0),
            Value::NIL,
        ]))
        .expect("accepted earlier end")
        .as_fixnum()
        .expect("accepted earlier end position");

    eval.set_variable(
        "neomacs-failing-later-window",
        Value::make_window(later_window.0),
    );
    eval.eval_str(
        "(progn
           (setq neomacs-later-sibling-attempt 0)
           (set-window-parameter
            neomacs-failing-later-window 'mode-line-format
            '((:eval
               (progn
                 (setq neomacs-later-sibling-attempt
                       (1+ neomacs-later-sibling-attempt))
                 (set-window-margins
                  neomacs-failing-later-window
                  neomacs-later-sibling-attempt 0)
                 \"RETRY\"))))
           (set-window-start (selected-window) 300))",
    )
    .expect("install later-sibling convergence failure");

    assert!(
        super::frame_layout::layout_frame_display_state(
            &mut eval,
            frame_id,
            super::frame_layout::FrameLayoutPurpose::Redisplay,
        )
        .is_none(),
        "the later sibling must exhaust the bounded frame coordinator"
    );
    assert_eq!(
        eval.eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(earlier_window.0),
            Value::NIL,
        ]))
        .expect("earlier end after rejected frame")
        .as_fixnum(),
        Some(accepted_earlier_end),
        "leaf window ends become accepted evidence only when the entire frame converges"
    );
}

#[test]
fn scroll_hooks_preserve_gnu_leaf_order_across_windows() {
    let (mut eval, buffer_id, frame_id, earlier_window) = initialized_redisplay_test_frame(
        "scroll-hook-leaf-order",
        800,
        320,
        "scroll hook leaf ordering fixture\n",
        300,
    );
    let later_window = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            earlier_window,
            neovm_core::window::SplitDirection::Horizontal,
            buffer_id,
            None,
            neovm_core::window::SplitPlacement::AfterTarget,
        )
        .expect("split window");
    let initial = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout split frame");
    let _ = initial.discard(&mut eval);

    eval.set_variable(
        "neomacs-earlier-scroll-window",
        Value::make_window(earlier_window.0),
    );
    eval.set_variable(
        "neomacs-later-scroll-window",
        Value::make_window(later_window.0),
    );
    eval.eval_str(
        "(progn
           (setq neomacs-scroll-leaf-order nil)
           (set-window-parameter
            neomacs-earlier-scroll-window 'mode-line-format
            '((:eval
               (progn
                 (setq neomacs-scroll-leaf-order
                       (cons 'earlier-mode-line neomacs-scroll-leaf-order))
                 \"EARLIER\"))))
           (set-window-parameter
            neomacs-later-scroll-window 'mode-line-format
            '((:eval
               (progn
                 (setq neomacs-scroll-leaf-order
                       (cons 'later-mode-line neomacs-scroll-leaf-order))
                 \"LATER\"))))
           (setq window-scroll-functions
                 (list
                  (lambda (window _start)
                    (setq neomacs-scroll-leaf-order
                          (cons (if (eq window neomacs-earlier-scroll-window)
                                    'earlier-scroll-hook
                                  'later-scroll-hook)
                                neomacs-scroll-leaf-order)))))
           (set-window-start neomacs-earlier-scroll-window 80)
           (set-window-start neomacs-later-scroll-window 120))",
    )
    .expect("install leaf-order probes");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("ordered multi-window layout");
    let _ = redisplay.discard(&mut eval);

    assert_eq!(
        eval.eval_str("(format \"%S\" (nreverse neomacs-scroll-leaf-order))")
            .expect("leaf order")
            .as_str_owned()
            .expect("leaf order string"),
        "(earlier-scroll-hook earlier-mode-line later-scroll-hook later-mode-line)",
        "GNU redisplay_window completes one leaf's hook/body/chrome before advancing to the next leaf"
    );
}

#[test]
fn scroll_hook_layout_mutations_replace_the_phase_a_window_snapshot() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "scroll-hook-live-inputs",
        800,
        320,
        "scroll hook live input fixture\n",
        200,
    );
    eval.eval_str(
        "(progn
           (setq window-scroll-functions
                 (list (lambda (window _start)
                         (set-window-margins window 7 3))))
           (set-window-start (selected-window) 120))",
    )
    .expect("install margin-changing scroll hook");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout after hook changed canonical window inputs");
    let info = redisplay
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == window_id.0 as i64)
        .expect("selected window output");
    let neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Complete {
        regions, ..
    } = info.geometry
    else {
        panic!("scroll-hook window must have complete presented geometry")
    };
    assert_eq!(regions.left_margin_columns, 7);
    assert_eq!(regions.right_margin_columns, 3);
    let _ = redisplay.discard(&mut eval);
}

#[test]
fn fontification_layout_mutations_discard_the_speculative_frame() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "fontification-live-inputs",
        800,
        320,
        "fontification live input fixture\n",
        200,
    );
    eval.eval_str(
        "(progn
           (setq neomacs-fontification-layout-count 0)
           (setq fontification-functions
                 (list (lambda (start)
                         (setq neomacs-fontification-layout-count
                               (1+ neomacs-fontification-layout-count))
                         (set-window-margins (selected-window) 6 2)
                         (put-text-property
                          start (min (point-max) (+ start 10000))
                          'fontified t)))))",
    )
    .expect("install layout-mutating fontification hook");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout after fontification changed canonical window inputs");
    let info = redisplay
        .window_infos
        .iter()
        .find(|info| info.window_id.get() == window_id.0 as i64)
        .expect("selected window output");
    let neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Complete {
        regions, ..
    } = info.geometry
    else {
        panic!("fontified window must have complete presented geometry")
    };
    assert_eq!(
        (regions.left_margin_columns, regions.right_margin_columns),
        (6, 2)
    );
    assert!(
        eval.eval_str("neomacs-fontification-layout-count")
            .expect("fontification count")
            .as_fixnum()
            .is_some_and(|count| count >= 1),
        "the regression must exercise the Lisp fontification boundary"
    );
    let _ = redisplay.discard(&mut eval);
}

#[test]
fn scoped_display_binding_during_chrome_layout_does_not_prevent_convergence() {
    let (mut eval, _buffer_id, frame_id, _window_id) = initialized_redisplay_test_frame(
        "scoped-display-binding-convergence",
        800,
        320,
        "stable live inputs after a scoped display binding\n",
        80,
    );
    eval.eval_str(
        "(progn
           (setq neomacs-scoped-display-binding-count 0)
           (set (make-local-variable 'mode-line-format)
                '((:eval
                   (let ((truncate-lines truncate-lines))
                     (setq neomacs-scoped-display-binding-count
                           (1+ neomacs-scoped-display-binding-count))
                     \"STABLE\")))))",
    )
    .expect("install a chrome callback with a restored display binding");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    );

    assert!(
        redisplay.is_some(),
        "a scoped callback binding that restores the same live display inputs must converge"
    );
    let _ = redisplay.expect("accepted presentation").discard(&mut eval);
}

#[test]
fn layout_attempt_freshness_distinguishes_a_persistent_display_change() {
    let (mut eval, buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "persistent-display-change-retry",
        800,
        320,
        "persistent display input change during chrome layout\n",
        80,
    );

    let before = eval
        .window_layout_attempt_freshness(frame_id, window_id, buffer_id)
        .expect("initial logical input projection");
    eval.eval_str("(set (make-local-variable 'truncate-lines) t)")
        .expect("persistently change an effective layout variable");
    let after = eval
        .window_layout_attempt_freshness(frame_id, window_id, buffer_id)
        .expect("changed logical input projection");

    assert_ne!(
        after, before,
        "a persistent effective display-variable change must stale speculative rows"
    );
}

#[test]
fn fontification_window_start_supersedes_the_exact_scroll_hook_resume() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "fontification-window-start",
        800,
        320,
        "fontification start ownership fixture\n",
        300,
    );
    eval.eval_str(
        "(progn
           (setq neomacs-start-hook-count 0)
           (setq neomacs-start-fontify-count 0)
           (setq window-scroll-functions
                 (list
                  (lambda (window _start)
                    (setq neomacs-start-hook-count
                          (1+ neomacs-start-hook-count))
                    (if (= neomacs-start-hook-count 1)
                        (set-window-start window 120)))))
           (setq fontification-functions
                 (list
                  (lambda (start)
                    (setq neomacs-start-fontify-count
                          (1+ neomacs-start-fontify-count))
                    (if (= neomacs-start-fontify-count 1)
                        (set-window-start (selected-window) 200))
                    (put-text-property
                     start (min (point-max) (+ start 10000))
                     'fontified t))))
           (set-window-start (selected-window) 80))",
    )
    .expect("install ordered hook/fontification start mutations");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout after fontification superseded the hook continuation");
    let _ = redisplay.discard(&mut eval);

    assert_eq!(
        eval.eval_form(Value::list(vec![
            Value::symbol("window-start"),
            Value::make_window(window_id.0),
        ]))
        .expect("final window start")
        .as_fixnum(),
        Some(200),
        "an exact post-hook continuation is valid only until later Lisp writes a newer canonical start"
    );
    assert_eq!(
        eval.eval_str("neomacs-start-hook-count")
            .expect("hook count")
            .as_fixnum(),
        Some(2),
        "the logical retry must enter the new canonical hook site instead of replaying the old continuation"
    );
}

#[test]
fn scroll_hook_does_not_observe_speculative_window_end() {
    let (mut eval, buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "scroll-hook-end",
        80,
        24,
        "scroll-hook end fixture\n",
        500,
    );
    let (buffer_z_char, buffer_z_byte) = {
        let buffer = eval.buffer_manager().get(buffer_id).expect("buffer");
        (
            buffer.point_max_char_pos().to_lisp(),
            buffer.point_max_emacs_byte_pos(),
        )
    };
    let window = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .and_then(|frame| frame.find_window_mut(window_id))
        .expect("window");
    window.set_window_end_from_positions(
        buffer_z_char,
        buffer_z_byte,
        neovm_core::buffer::LispCharPos1::new(123),
        neovm_core::buffer::EmacsBytePos::new(122),
        0,
    );
    window.invalidate_window_end();
    let previous_end = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(window_id.0),
            Value::NIL,
        ]))
        .expect("previous recorded end")
        .as_fixnum()
        .expect("recorded end position");

    eval.eval_str(
        "(progn
           (setq neomacs-scroll-hook-inside-end nil)
           (setq window-scroll-functions
                 (list (lambda (window _start)
                         (setq neomacs-scroll-hook-inside-end
                               (window-end window nil)))))
           (goto-char 400))",
    )
    .expect("install end-observing scroll hook");
    eval.eval_form(Value::list(vec![
        Value::symbol("set-window-start"),
        Value::make_window(window_id.0),
        Value::fixnum(400),
    ]))
    .expect("force a new start");

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout after observing the pre-final end");
    let _ = redisplay.discard(&mut eval);
    let final_end = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(window_id.0),
            Value::NIL,
        ]))
        .expect("final recorded end")
        .as_fixnum()
        .expect("final end position");

    assert_eq!(
        eval.eval_str("neomacs-scroll-hook-inside-end")
            .expect("end observed inside hook")
            .as_fixnum(),
        Some(previous_end),
        "GNU invalidates window_end before the scroll hook and records the new end only after try_window finishes"
    );
    assert_ne!(
        final_end, previous_end,
        "final layout must replace the fixture's stale end"
    );
}

#[test]
fn nested_window_query_from_mode_line_uses_the_renderer_inert_query_engine() {
    let (mut eval, _buffer_id, frame_id, _window_id) = initialized_redisplay_test_frame(
        "nested-layout-query",
        800,
        320,
        "mode-line query fixture\n",
        80,
    );

    eval.eval_str(
        "(progn
           (setq neomacs-nested-layout-query-result nil)
           (set (make-local-variable 'mode-line-format)
                '((:eval
                   (condition-case err
                       (progn
                         (setq neomacs-nested-layout-query-result
                               (window-end nil t))
                         \"QUERY\")
                     (error
                      (setq neomacs-nested-layout-query-result (car err))
                      \"BUSY\"))))))",
    )
    .expect("install reentrant mode-line query");

    let display = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("layout must complete without a recursive RefCell panic");
    let _ = display.discard(&mut eval);
    assert!(
        eval.eval_str("neomacs-nested-layout-query-result")
            .expect("query result")
            .as_fixnum()
            .is_some(),
        "a nested display query must use a fresh stack-local row walk, never stale retained geometry or a recursive RefCell borrow"
    );
}

#[test]
fn synchronous_window_end_query_does_not_evaluate_status_line_lisp() {
    let (mut eval, _buffer_id, _frame_id, window_id) = initialized_redisplay_test_frame(
        "window-end-no-chrome",
        800,
        320,
        "window-end query fixture\n",
        80,
    );

    eval.eval_str(
        "(progn
           (setq neomacs-window-end-mode-line-count 0)
           (set (make-local-variable 'mode-line-format)
                '((:eval
                   (progn
                     (setq neomacs-window-end-mode-line-count
                           (1+ neomacs-window-end-mode-line-count))
                     \"QUERY\")))))",
    )
    .expect("install mode-line side-effect probe");

    let end = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(window_id.0),
            Value::symbol("t"),
        ]))
        .expect("exact window-end query");

    assert!(
        end.as_fixnum().is_some(),
        "window-end must return a position"
    );
    assert_eq!(
        eval.eval_str("neomacs-window-end-mode-line-count")
            .expect("mode-line counter")
            .as_fixnum(),
        Some(0),
        "GNU's stack-local Fwindow_end iterator does not evaluate window chrome"
    );
}

#[test]
fn current_fresh_window_end_does_not_reenter_the_layout_adapter() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "window-end-current-fast-path",
        800,
        320,
        "current window end fixture\n",
        80,
    );
    let activated = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("prepare current presentation")
    .activate(&mut eval)
    .expect("activate current presentation");
    drop(activated);
    let expected = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(window_id.0),
            Value::NIL,
        ]))
        .expect("accepted end");

    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed_calls = std::rc::Rc::clone(&calls);
    eval.install_window_layout_query(move |_eval, _frame_id, _window_id| {
        observed_calls.set(observed_calls.get() + 1);
        neovm_core::window::WindowLayoutQueryOutcome::Failed(
            neovm_core::window::WindowLayoutQueryFailure::DidNotConverge,
        )
    });
    let updated = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(window_id.0),
            Value::T,
        ]))
        .expect("fresh UPDATE query");

    assert_eq!(updated, expected);
    assert_eq!(
        calls.get(),
        0,
        "GNU Fwindow_end reuses an accepted current end instead of constructing another iterator"
    );
}

#[test]
fn zero_area_window_end_update_returns_the_exact_live_start() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "window-end-zero-area",
        800,
        320,
        "zero area window-end fixture\n",
        80,
    );
    eval.eval_form(Value::list(vec![
        Value::symbol("set-window-start"),
        Value::make_window(window_id.0),
        Value::fixnum(25),
    ]))
    .expect("set exact live start");
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .find_window_mut(window_id)
        .expect("window")
        .set_bounds(neovm_core::window::Rect::new(0.0, 0.0, 0.0, 0.0));

    assert_eq!(
        eval.eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(window_id.0),
            Value::T,
        ]))
        .expect("zero-area exact query")
        .as_fixnum(),
        Some(25),
        "a coherent Ready query always owns an end, even when no matrix row can be produced"
    );
}

#[test]
fn inactive_echo_layout_preserves_live_minibuffer_positions_and_query_source() {
    let (mut eval, _buffer_id, frame_id, _window_id) = initialized_redisplay_test_frame(
        "inactive-echo-source",
        800,
        320,
        "inactive echo source fixture\n",
        80,
    );
    let minibuffer_window = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.minibuffer_window)
        .expect("minibuffer window");
    // The low-level frame fixture initially gives its ordinary and minibuffer
    // windows the same buffer.  Production startup installs a distinct
    // minibuffer buffer; reproduce that ownership boundary here so the local
    // hook below cannot be observed by the ordinary root window.
    let minibuffer_buffer = eval
        .buffer_manager_mut()
        .create_buffer(" *inactive echo live minibuffer*");
    eval.eval_form(Value::list(vec![
        Value::symbol("set-window-buffer"),
        Value::make_window(minibuffer_window.0),
        Value::make_buffer(minibuffer_buffer),
    ]))
    .expect("install distinct live minibuffer buffer");
    eval.buffer_manager_mut()
        .get_mut(minibuffer_buffer)
        .expect("live minibuffer buffer")
        .insert(&"live minibuffer text\n".repeat(20));
    eval.ensure_echo_area_buffers();
    let echo_buffer = eval
        .buffer_manager()
        .find_buffer_by_name(" *Echo Area 0*")
        .expect("echo buffer");
    eval.buffer_manager_mut()
        .get_mut(echo_buffer)
        .expect("echo buffer")
        .insert("x\n");

    eval.set_variable(
        "neomacs-inactive-minibuffer-window",
        Value::make_window(minibuffer_window.0),
    );
    eval.eval_str(
        "(progn
           (setq neomacs-inactive-echo-hook-count 0)
           (setq neomacs-inactive-echo-original-buffer (current-buffer))
           (set-buffer (window-buffer neomacs-inactive-minibuffer-window))
           (set (make-local-variable 'window-scroll-functions)
                (list
                 (lambda (_window _start)
                   (setq neomacs-inactive-echo-hook-count
                         (1+ neomacs-inactive-echo-hook-count)))))
           (set-buffer neomacs-inactive-echo-original-buffer))",
    )
    .expect("install inactive-minibuffer hook probe");
    let _ = eval.publish_redisplay_window_start(
        frame_id,
        minibuffer_window,
        neovm_core::buffer::LispCharPos1::new(20),
    );
    assert_eq!(
        eval.eval_form(Value::list(vec![
            Value::symbol("window-start"),
            Value::make_window(minibuffer_window.0),
        ]))
        .expect("initial live minibuffer start")
        .as_fixnum(),
        Some(20),
        "the test must establish a non-echo live start before redisplay"
    );

    let redisplay = super::frame_layout::layout_frame_display_state(
        &mut eval,
        frame_id,
        super::frame_layout::FrameLayoutPurpose::Redisplay,
    )
    .expect("render inactive echo source");
    let _ = redisplay.discard(&mut eval);
    assert_eq!(
        eval.eval_form(Value::list(vec![
            Value::symbol("window-start"),
            Value::make_window(minibuffer_window.0),
        ]))
        .expect("live minibuffer start")
        .as_fixnum(),
        Some(20),
        "GNU with_echo_area_buffer restores the live minibuffer start"
    );
    assert_eq!(
        eval.eval_str("neomacs-inactive-echo-hook-count")
            .expect("inactive echo hook count")
            .as_fixnum(),
        Some(0),
        "display_echo_area_1 does not enter redisplay_window's scroll hook"
    );
    assert!(
        eval.eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(minibuffer_window.0),
            Value::T,
        ]))
        .expect("live minibuffer query")
        .as_fixnum()
        .is_some_and(|end| end >= 20),
        "Fwindow_end must walk the live minibuffer buffer, not the temporary echo source"
    );
}

#[test]
fn synchronous_window_end_uses_the_final_source_buffer_identity() {
    let (mut eval, _buffer_id, frame_id, window_id) = initialized_redisplay_test_frame(
        "window-end-buffer-switch",
        800,
        320,
        "old source buffer has many rows\n",
        300,
    );
    eval.eval_str(
        "(progn
           (setq neomacs-window-end-fontify-count 0)
           (setq neomacs-window-end-target-buffer
                 (get-buffer-create \" *window-end-new-source*\"))
           (setq neomacs-window-end-original-buffer (current-buffer))
           (set-buffer neomacs-window-end-target-buffer)
           (erase-buffer)
           (insert \"x\\n\")
           (set-buffer neomacs-window-end-original-buffer)
           (setq neomacs-window-end-switch-fontifier
                 (lambda (start)
                   (setq neomacs-window-end-fontify-count
                         (1+ neomacs-window-end-fontify-count))
                   (if (= neomacs-window-end-fontify-count 1)
                       (set-window-buffer
                        (selected-window)
                        neomacs-window-end-target-buffer))
                   (put-text-property
                    start (min (point-max) (+ start 10000)) 'fontified t)))
           (setq fontification-functions
                 (list neomacs-window-end-switch-fontifier))
           (set-buffer neomacs-window-end-target-buffer)
           (setq fontification-functions
                 (list neomacs-window-end-switch-fontifier))
           (set-buffer neomacs-window-end-original-buffer))",
    )
    .expect("install source-switching fontifier");
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .and_then(|frame| frame.find_window_mut(window_id))
        .expect("query window")
        .invalidate_window_end();

    let end = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-end"),
            Value::make_window(window_id.0),
            Value::T,
        ]))
        .expect("window-end after source-buffer switch")
        .as_fixnum()
        .expect("absolute end position");
    assert!(
        (1..=3).contains(&end),
        "the query end must be decoded against the final two-character buffer, not the old buffer Z: {end}"
    );
    assert!(
        eval.eval_str("neomacs-window-end-fontify-count")
            .expect("fontification count")
            .as_fixnum()
            .is_some_and(|count| count >= 2),
        "the query must reject the old source and rerun against the replacement"
    );
}

fn test_image_catalog(
    cmd_tx: &crossbeam_channel::Sender<RenderCommand>,
    image_metadata: SharedImageRenderState,
) -> Rc<AsyncImageCatalog> {
    Rc::new(AsyncImageCatalog::new(cmd_tx.clone(), None, image_metadata))
}

#[test]
fn image_catalog_reports_renderer_owned_cache_bytes() {
    let (cmd_tx, _cmd_rx) = crossbeam_channel::unbounded();
    let image_state = Arc::new(neomacs_display_runtime::render_thread::ImageRenderState::default());
    let catalog = test_image_catalog(&cmd_tx, Arc::clone(&image_state));

    image_state.publish_cache_usage(neomacs_display_protocol::ImageCacheUsage::new(4_096, 768));

    assert_eq!(catalog.cached_size_bytes(), 4_864);
}

#[test]
fn runtime_mode_binary_names_match_gnu_shaped_roles() {
    assert_eq!(RuntimeMode::Raw.binary_name(), "neomacs-temacs");
    assert_eq!(RuntimeMode::BootstrapUse.binary_name(), "bootstrap-neomacs");
    assert_eq!(RuntimeMode::FinalRun.binary_name(), "neomacs");
}

#[test]
fn runtime_mode_comes_from_invoked_program_name() {
    assert_eq!(
        runtime_mode_from_program_name("/tmp/neomacs-temacs"),
        RuntimeMode::Raw
    );
    assert_eq!(
        runtime_mode_from_program_name("target/debug/bootstrap-neomacs"),
        RuntimeMode::BootstrapUse
    );
    assert_eq!(
        runtime_mode_from_program_name("target/debug/neomacs"),
        RuntimeMode::FinalRun
    );
    assert_eq!(
        runtime_mode_from_program_name("target/debug/neomacs.exe"),
        RuntimeMode::FinalRun
    );
}

#[test]
fn runtime_mode_dump_image_kinds_match_pipeline_roles() {
    assert_eq!(RuntimeMode::Raw.dump_image_kind(), None);
    assert_eq!(
        RuntimeMode::BootstrapUse.dump_image_kind(),
        Some(DumpImageKind::Bootstrap)
    );
    assert_eq!(
        RuntimeMode::FinalRun.dump_image_kind(),
        Some(DumpImageKind::Final)
    );
}

#[test]
fn bootstrap_gui_display_defaults_to_gnu_light_background_mode() {
    assert_eq!(gui_display().background_mode, "light");
}

#[test]
fn bootstrap_gui_frame_uses_gnu_cursor_and_pointer_color_defaults() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let frame_id = bootstrap_runtime_gui_startup(&mut eval);
    let frame = eval.frame_manager().get(frame_id).expect("live frame");

    assert_eq!(
        frame.known_parameter(FrameParam::ForegroundColor),
        Some(Value::string("black"))
    );
    assert_eq!(
        frame.known_parameter(FrameParam::BackgroundColor),
        Some(Value::string("white"))
    );
    assert_eq!(
        frame.known_parameter(FrameParam::CursorColor),
        Some(Value::string("black"))
    );
    assert_eq!(
        frame.known_parameter(FrameParam::MouseColor),
        Some(Value::string("black"))
    );
    assert_eq!(
        frame.known_parameter(FrameParam::BorderColor),
        Some(Value::string("black"))
    );
    assert_eq!(
        frame.known_parameter(FrameParam::CursorType),
        Some(Value::symbol("box"))
    );
}

fn gui_startup() -> StartupOptions {
    StartupOptions {
        frontend: FrontendKind::Gui,
        forwarded_args: vec!["neomacs".to_string(), "-Q".to_string()],
        terminal_device: None,
        noninteractive: false,
        temacs_mode: None,
        dump_file_override: None,
        no_site_lisp: true,
        no_loadup: false,
        no_build_details: false,
    }
}

fn gui_startup_with_args(args: &[&str]) -> StartupOptions {
    let mut forwarded_args = vec!["neomacs".to_string()];
    forwarded_args.extend(args.iter().map(|arg| (*arg).to_string()));
    StartupOptions {
        frontend: FrontendKind::Gui,
        forwarded_args,
        terminal_device: None,
        noninteractive: false,
        temacs_mode: None,
        dump_file_override: None,
        no_site_lisp: false,
        no_loadup: false,
        no_build_details: false,
    }
}

fn tty_batch_startup_with_args(args: &[&str]) -> StartupOptions {
    let mut forwarded_args = vec!["neomacs".to_string()];
    forwarded_args.extend(args.iter().map(|arg| (*arg).to_string()));
    StartupOptions {
        frontend: FrontendKind::Tty,
        forwarded_args,
        terminal_device: None,
        noninteractive: true,
        temacs_mode: None,
        dump_file_override: None,
        no_site_lisp: false,
        no_loadup: false,
        no_build_details: false,
    }
}

#[test]
fn parse_startup_options_accepts_gnu_temacs_modes() {
    let startup = parse_startup_options([
        "neomacs-temacs".to_string(),
        "--temacs=pbootstrap".to_string(),
        "--batch".to_string(),
    ])
    .expect("startup options should parse");
    assert_eq!(startup.temacs_mode, Some(LoadupDumpMode::Pbootstrap));

    let startup = parse_startup_options([
        "neomacs-temacs".to_string(),
        "--temacs".to_string(),
        "pdump".to_string(),
    ])
    .expect("startup options should parse");
    assert_eq!(startup.temacs_mode, Some(LoadupDumpMode::Pdump));
}

#[test]
fn parse_startup_options_accepts_dump_file_override() {
    let startup = parse_startup_options([
        "neomacs".to_string(),
        "--dump-file=/tmp/custom.pdump".to_string(),
    ])
    .expect("startup options should parse");
    assert_eq!(
        startup.dump_file_override,
        Some(std::path::PathBuf::from("/tmp/custom.pdump"))
    );
}

#[test]
fn parse_startup_options_consumes_chdir_flag_and_changes_cwd() {
    // GNU emacs.c:1538-1561 — `--chdir DIR` calls chdir(DIR) before
    // any later parsing or file resolution. The flag is consumed (not
    // forwarded) and a chdir failure aborts startup.
    //
    // nextest runs each #[test] in its own process so the cwd mutation
    // does not leak into sibling tests.
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let canonical = std::fs::canonicalize(tmp.path()).expect("canonicalize tempdir");

    let startup = parse_startup_options([
        "neomacs".to_string(),
        "--chdir".to_string(),
        canonical.to_string_lossy().into_owned(),
    ])
    .expect("startup options should parse");

    let cwd = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
    assert_eq!(cwd, canonical);
    // The flag must NOT appear in forwarded_args — GNU consumes it.
    assert!(
        !startup
            .forwarded_args
            .iter()
            .any(|a| a == "--chdir" || a == "-chdir"),
        "--chdir should be consumed, not forwarded: {:?}",
        startup.forwarded_args
    );
}

#[test]
fn parse_startup_options_chdir_inline_value_form_works() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let canonical = std::fs::canonicalize(tmp.path()).expect("canonicalize tempdir");

    let startup = parse_startup_options([
        "neomacs".to_string(),
        format!("--chdir={}", canonical.display()),
    ])
    .expect("startup options should parse");

    let cwd = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
    assert_eq!(cwd, canonical);
    assert!(
        !startup
            .forwarded_args
            .iter()
            .any(|a| a.starts_with("--chdir")),
        "--chdir=… should be consumed, not forwarded: {:?}",
        startup.forwarded_args
    );
}

#[test]
fn parse_startup_options_chdir_to_nonexistent_dir_errors() {
    // GNU emacs.c:1551 — `Can't chdir to %s: %s`. We match the prefix
    // but use Rust's std::io::Error message for the suffix.
    let err = parse_startup_options([
        "neomacs".to_string(),
        "--chdir".to_string(),
        "/this/path/cannot/possibly/exist".to_string(),
    ])
    .expect_err("chdir to nonexistent should fail");
    assert!(
        err.starts_with("neomacs: Can't chdir to /this/path/cannot/possibly/exist"),
        "unexpected error message: {err}"
    );
}

#[test]
fn parse_startup_options_chdir_missing_value_errors() {
    let err = parse_startup_options(["neomacs".to_string(), "--chdir".to_string()])
        .expect_err("chdir without value should fail");
    assert!(
        err.contains("requires an argument"),
        "expected requires-argument error, got: {err}"
    );
}

#[test]
fn parse_startup_options_consumes_script_flag_with_rewrite() {
    // GNU emacs.c:1708-1717: --script FILE sets noninteractive and
    // rewrites the matched flag to -scriptload (an internal flag that
    // lisp/startup.el:2841 understands). The user's FILE follows
    // -scriptload in argv.
    let startup = parse_startup_options([
        "neomacs".to_string(),
        "--script".to_string(),
        "/tmp/foo.el".to_string(),
    ])
    .expect("startup options should parse");

    assert!(startup.noninteractive, "--script must imply noninteractive");
    assert_eq!(startup.frontend, FrontendKind::Tty);
    // The original --script flag must NOT appear in forwarded_args.
    assert!(
        !startup
            .forwarded_args
            .iter()
            .any(|a| a == "--script" || a == "-script"),
        "--script should be rewritten away: {:?}",
        startup.forwarded_args
    );
    // -scriptload FILE must be present in the right order.
    let pos = startup
        .forwarded_args
        .iter()
        .position(|a| a == "-scriptload")
        .expect("-scriptload should be in forwarded_args");
    assert_eq!(
        startup.forwarded_args.get(pos + 1).map(String::as_str),
        Some("/tmp/foo.el"),
        "FILE should follow -scriptload"
    );
}

#[test]
fn parse_startup_options_script_missing_value_errors() {
    let err = parse_startup_options(["neomacs".to_string(), "--script".to_string()])
        .expect_err("--script with no value should fail");
    assert!(
        err.contains("requires an argument"),
        "expected requires-argument error, got: {err}"
    );
}

#[test]
fn parse_startup_options_consumes_dash_x_with_scripteval_rewrite() {
    // GNU emacs.c:2132-2140: -x sets noninteractive AND no_site_lisp,
    // and rewrites the matched flag to -scripteval (internal flag for
    // shebang-style #!/usr/bin/neomacs -x scripts).
    let startup = parse_startup_options(["neomacs".to_string(), "-x".to_string()])
        .expect("startup options should parse");

    assert!(startup.noninteractive, "-x must imply noninteractive");
    assert!(startup.no_site_lisp, "-x must imply no-site-lisp");
    assert_eq!(startup.frontend, FrontendKind::Tty);
    assert!(
        !startup.forwarded_args.iter().any(|a| a == "-x"),
        "-x should be rewritten away: {:?}",
        startup.forwarded_args
    );
    assert!(
        startup.forwarded_args.iter().any(|a| a == "-scripteval"),
        "-scripteval should be in forwarded_args: {:?}",
        startup.forwarded_args
    );
}

#[test]
fn parse_startup_options_consumes_no_loadup_flag() {
    // GNU emacs.c:2031-2032: --no-loadup sets no_loadup, which gates the
    // -l loadup splice in main(). Consumed entirely; not forwarded.
    let startup = parse_startup_options(["neomacs".to_string(), "--no-loadup".to_string()])
        .expect("startup options should parse");
    assert!(startup.no_loadup);
    assert!(
        !startup
            .forwarded_args
            .iter()
            .any(|a| a == "--no-loadup" || a == "-nl"),
        "--no-loadup should be consumed"
    );
}

#[test]
fn parse_startup_options_consumes_short_nl_flag() {
    let startup = parse_startup_options(["neomacs".to_string(), "-nl".to_string()])
        .expect("startup options should parse");
    assert!(startup.no_loadup);
}

#[test]
fn raw_loadup_command_line_skips_loadup_splice_when_no_loadup_set() {
    // The user-visible effect of --no-loadup at RuntimeMode::Raw: the
    // synthetic `-l loadup` splice is omitted, mirroring GNU
    // emacs.c:2578 `if (!no_loadup) ... loadup.el`.
    let startup = parse_startup_options([
        "neomacs-temacs".to_string(),
        "--no-loadup".to_string(),
        "--temacs=pdump".to_string(),
    ])
    .expect("startup options should parse");
    let argv = raw_loadup_command_line(&startup, LoadupDumpMode::Pdump);
    assert!(
        !argv.windows(2).any(|w| w[0] == "-l" && w[1] == "loadup"),
        "loadup splice should be skipped: {argv:?}"
    );
}

#[test]
fn parse_startup_options_consumes_no_site_lisp_flag() {
    // GNU emacs.c:2034-2035: --no-site-lisp sets no_site_lisp.
    let startup = parse_startup_options(["neomacs".to_string(), "--no-site-lisp".to_string()])
        .expect("startup options should parse");
    assert!(startup.no_site_lisp);
    assert!(
        !startup
            .forwarded_args
            .iter()
            .any(|a| a == "--no-site-lisp" || a == "-nsl"),
        "--no-site-lisp should be consumed"
    );
}

#[test]
fn parse_startup_options_consumes_short_nsl_flag() {
    let startup = parse_startup_options(["neomacs".to_string(), "-nsl".to_string()])
        .expect("startup options should parse");
    assert!(startup.no_site_lisp);
}

#[test]
fn parse_startup_options_consumes_no_build_details_flag() {
    // GNU emacs.c:2037-2038: --no-build-details inverts build_details.
    let startup = parse_startup_options(["neomacs".to_string(), "--no-build-details".to_string()])
        .expect("startup options should parse");
    assert!(startup.no_build_details);
    assert!(
        !startup
            .forwarded_args
            .iter()
            .any(|a| a == "--no-build-details" || a == "-no-build-details"),
        "--no-build-details should be consumed"
    );
}

#[test]
fn parse_startup_options_peeks_q_to_set_no_site_lisp() {
    // GNU emacs.c:2126-2129 — `-Q` is peeked: it sets no_site_lisp=1
    // AND remains in argv so lisp/startup.el's command-line at
    // lisp/startup.el:1404 can also process it. We mirror both halves.
    let startup = parse_startup_options(["neomacs".to_string(), "-Q".to_string()])
        .expect("startup options should parse");
    assert!(startup.no_site_lisp, "-Q peek should set no_site_lisp");
    assert!(
        startup.forwarded_args.iter().any(|a| a == "-Q"),
        "-Q must remain in forwarded_args after peek: {:?}",
        startup.forwarded_args
    );
}

#[test]
fn parse_startup_options_peeks_long_quick_alias() {
    // GNU emacs.c:2126-2127 — `--quick` and `-quick` are equivalent
    // peek aliases for -Q. The -quick spelling matches the same
    // STANDARD_ARGS row that `-Q` does (priority 55).
    for spelling in &["--quick", "-quick"] {
        let startup = parse_startup_options(["neomacs".to_string(), (*spelling).to_string()])
            .expect("startup options should parse");
        assert!(
            startup.no_site_lisp,
            "{spelling} peek should set no_site_lisp"
        );
        assert!(
            startup.forwarded_args.iter().any(|a| a == spelling),
            "{spelling} must remain in forwarded_args after peek: {:?}",
            startup.forwarded_args
        );
    }
}

#[test]
fn parse_startup_options_q_peek_redundant_when_nsl_already_set() {
    // GNU emacs.c:2123 has an `if (! no_site_lisp)` guard around the
    // peek block. Once -nsl has set the flag, peeking -Q is a no-op
    // for state but the -Q token still remains in forwarded_args.
    let startup = parse_startup_options([
        "neomacs".to_string(),
        "--no-site-lisp".to_string(),
        "-Q".to_string(),
    ])
    .expect("startup options should parse");
    assert!(startup.no_site_lisp);
    assert!(startup.forwarded_args.iter().any(|a| a == "-Q"));
    // --no-site-lisp itself was consumed (Phase 3c).
    assert!(!startup.forwarded_args.iter().any(|a| a == "--no-site-lisp"));
}

#[test]
fn parse_startup_options_normalizes_display_args_to_gnu_form() {
    // GNU emacs.c:2110-2120 rewrites `--display=NAME` into the
    // equivalent `-d NAME` two-token form before passing argv on to
    // `lisp/startup.el`. We mirror that normalization in `parse_startup_options`
    // so the Lisp side observes the same shape under both implementations.
    // Other flags like `-Q` flow through unchanged.
    let startup = parse_startup_options([
        "neomacs".to_string(),
        "--display=:1".to_string(),
        "-Q".to_string(),
    ])
    .expect("startup options should parse");

    assert_eq!(
        startup.forwarded_args,
        vec![
            "neomacs".to_string(),
            "-d".to_string(),
            ":1".to_string(),
            "-Q".to_string()
        ]
    );
}

#[test]
fn raw_loadup_command_line_inserts_internal_loadup_marker() {
    // Phase 2 added sort_args to parse_startup_options, so flags now
    // appear in GNU's standard_args[] priority order regardless of how
    // they were typed. -Q (priority 55) sits ahead of --temacs / --dump-file
    // (priority 1). The -l loadup splice from raw_loadup_command_line
    // is then prepended.
    let startup = parse_startup_options([
        "neomacs-temacs".to_string(),
        "--temacs=pdump".to_string(),
        "--dump-file=/tmp/custom.pdump".to_string(),
        "-Q".to_string(),
    ])
    .expect("startup options should parse");

    assert_eq!(
        raw_loadup_command_line(&startup, LoadupDumpMode::Pdump),
        vec![
            "neomacs-temacs".to_string(),
            "-l".to_string(),
            "loadup".to_string(),
            "-Q".to_string(),
            "--temacs=pdump".to_string(),
            "--dump-file=/tmp/custom.pdump".to_string(),
        ]
    );
}

#[test]
fn raw_dump_loadup_invocation_owns_mode_and_build_argv() {
    let startup = parse_startup_options([
        "neomacs-temacs".to_string(),
        "--temacs=pbootstrap".to_string(),
    ])
    .expect("startup options should parse");

    let invocation = raw_dump_loadup_invocation(&startup, LoadupDumpMode::Pbootstrap);
    let LoadupInvocation::Dump(dump) = invocation else {
        panic!("raw dump should produce a typed dump invocation");
    };
    assert_eq!(dump.mode(), LoadupDumpMode::Pbootstrap);
    assert_eq!(
        dump.command_line_args(),
        vec![
            "neomacs-temacs".to_string(),
            "-l".to_string(),
            "loadup".to_string(),
            "--temacs=pbootstrap".to_string(),
        ]
    );
}

fn bootstrap_runtime_gui_frame(eval: &mut Context) -> FrameId {
    load_neomacs_gui_term_layer(eval);
    let _bootstrap = bootstrap_buffers(eval, 960, 640, gui_display());
    eval.frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id
}

fn bootstrap_runtime_gui_startup(eval: &mut Context) -> FrameId {
    let frame_id = bootstrap_runtime_gui_frame(eval);
    configure_gnu_startup_state(eval, frame_id, &gui_startup());
    frame_id
}

fn eval_after_gnu_gui_startup(source: &str) -> String {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);
    run_gnu_startup(&mut eval);

    let result = eval.eval_str(source).expect("probe should evaluate");
    print_value_with_eval(&mut eval, &result)
}

#[test]
fn bootstrap_buffers_realize_default_face_from_frame_font_parameter() {
    let mut eval = create_bootstrap_evaluator_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let expected_family_value = eval
        .eval_str("(font-get (frame-parameter nil 'font-parameter) :family)")
        .expect("bootstrap frame font family");
    let expected_family = expected_family_value
        .as_utf8_str()
        .or_else(|| expected_family_value.as_symbol_name())
        .map(str::to_owned)
        .expect("font family name");
    let default = eval.face_table().get("default").expect("default face");
    assert_eq!(
        default.family_runtime_string_owned().as_deref(),
        Some(expected_family.as_str()),
    );
    assert_eq!(default.weight.map(|weight| weight.css_weight()), Some(400));
    assert_eq!(default.height, Some(FaceHeight::Absolute(100)));
}

fn assert_selected_frame_matches_materialized_default_metrics(eval: &Context) {
    let frame = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap");
    let face = eval.face_table().resolve("default");
    let family = face
        .family_runtime_string_owned()
        .unwrap_or_else(|| "Monospace".to_string());
    let weight = face.weight.map(|weight| weight.css_weight()).unwrap_or(400);
    let italic = face.slant.is_some_and(|slant| slant.is_italic());
    let mut service = FontMetricsService::new();
    let expected = service.font_metrics(&family, weight, italic, frame.font_pixel_size);
    assert_eq!(frame.char_width, expected.char_width.max(1.0));
    assert_eq!(frame.char_height, expected.line_height.max(1.0));
}

#[test]
fn opening_gui_frame_adoption_does_not_push_stale_window_size() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let mut host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };

    neovm_core::emacs_core::DisplayHost::realize_gui_frame(
        &mut host,
        GuiFrameHostRequest {
            frame_id: FrameId(0x100000001),
            width: 960,
            height: 640,
            title: LispString::from_utf8("Neomacs"),
            geometry_hints: GuiFrameGeometryHints {
                base_width: 24,
                base_height: 16,
                min_width: 24,
                min_height: 16,
                width_inc: 8,
                height_inc: 16,
            },
            fullscreen: None,
        },
    )
    .expect("adopt opening gui frame");

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert_eq!(commands.len(), 3);
    assert!(
        commands.iter().any(
            |cmd| matches!(cmd, RenderCommand::Window(WindowCommand::SetWindowTitle { title }) if title == "Neomacs")
        )
    );
    assert!(commands.iter().any(|cmd| matches!(
        cmd,
        RenderCommand::Window(WindowCommand::SetFrameGeometryHints {
            frame: FrameRef::Primary,
            geometry_hints,
        }) if *geometry_hints
            == GuiFrameGeometryHints {
                base_width: 24,
                base_height: 16,
                min_width: 24,
                min_height: 16,
                width_inc: 8,
                height_inc: 16,
            }
    )));
    assert!(commands.iter().any(|cmd| matches!(
        cmd,
        RenderCommand::Window(WindowCommand::AdoptPrimaryFrame {
            frame: FrameRef::Frame(0x100000001),
        })
    )));
    assert!(host.primary_window_adopted);
    assert_eq!(host.primary_frame_id, Some(FrameId(0x100000001)));
}

#[test]
fn opening_gui_frame_adoption_applies_fullscreen_mode() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let mut host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };

    neovm_core::emacs_core::DisplayHost::realize_gui_frame(
        &mut host,
        GuiFrameHostRequest {
            frame_id: FrameId(0x100000001),
            width: 960,
            height: 640,
            title: LispString::from_utf8("Neomacs"),
            geometry_hints: GuiFrameGeometryHints {
                base_width: 24,
                base_height: 16,
                min_width: 24,
                min_height: 16,
                width_inc: 8,
                height_inc: 16,
            },
            fullscreen: Some(FrameFullscreen::Maximized),
        },
    )
    .expect("adopt opening gui frame");

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert!(commands.iter().any(|cmd| matches!(
        cmd,
        RenderCommand::Window(WindowCommand::SetWindowFullscreen {
            frame: FrameRef::Primary,
            mode: WindowFullscreenMode::Maximized,
        })
    )));
}

#[test]
fn primary_display_host_destroy_gui_frame_routes_primary_and_secondary_windows() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let mut host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: true,
        primary_frame_id: Some(FrameId(0x100000001)),
        last_window_titles: Mutex::new(std::collections::HashMap::from([
            (FrameId(0x100000001), LispString::from_utf8("primary")),
            (FrameId(0x100000002), LispString::from_utf8("secondary")),
        ])),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };

    neovm_core::emacs_core::DisplayHost::destroy_gui_frame(&mut host, FrameId(0x100000002))
        .expect("destroy secondary frame");
    neovm_core::emacs_core::DisplayHost::destroy_gui_frame(&mut host, FrameId(0x100000001))
        .expect("destroy primary frame");

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert_eq!(commands.len(), 2);
    assert!(matches!(
        commands[0],
        RenderCommand::Window(WindowCommand::DestroyWindow {
            frame: FrameRef::Frame(0x100000002),
        })
    ));
    assert!(matches!(
        commands[1],
        RenderCommand::Window(WindowCommand::DestroyWindow {
            frame: FrameRef::Primary
        })
    ));
    assert_eq!(host.primary_frame_id, None);
    let cached_titles = host.last_window_titles.lock().expect("title cache");
    assert!(cached_titles.is_empty());
}

#[test]
fn primary_display_host_popup_menu_routes_primary_and_secondary_frames() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let mut host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: true,
        primary_frame_id: Some(FrameId(0x100000001)),
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };

    let entry = PopupMenuEntry {
        label: "Open".to_string(),
        shortcut: "C-x C-f".to_string(),
        enabled: true,
        separator: false,
        submenu: false,
        depth: 0,
    };
    for frame_id in [FrameId(0x100000001), FrameId(0x100000002)] {
        neovm_core::emacs_core::DisplayHost::show_popup_menu(
            &mut host,
            PopupMenuRequest {
                frame_id,
                placement: neomacs_display_protocol::PopupPlacement::at(
                    neomacs_display_protocol::Point::new(10.0, 20.0),
                ),
                title: Some("File".to_string()),
                entries: vec![entry.clone()],
                selected: 0,
            },
        )
        .expect("show popup menu");
    }

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert_eq!(commands.len(), 2);
    assert!(matches!(
        commands[0],
        RenderCommand::Ui(UiCommand::ShowPopupMenu {
            frame: FrameRef::Primary,
            ..
        })
    ));
    assert!(matches!(
        commands[1],
        RenderCommand::Ui(UiCommand::ShowPopupMenu {
            frame: FrameRef::Frame(0x100000002),
            ..
        })
    ));
}

#[test]
fn primary_image_catalog_lookup_returns_pending_without_waiting_for_render_thread() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let image_metadata = Arc::new(ImageRenderState::default());
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::clone(&image_metadata)),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };
    let repo_root = Path::new(env!("CARGO_WORKSPACE_DIR"));
    let image_path = repo_root.join("test/data/image/blank-100x200.png");
    let request = ImageResolveRequest {
        spec: test_image_spec_identity(image_path.to_str().expect("utf8 path")),
        source: ImageResolveSource::File(LispString::from_utf8(
            image_path.to_str().expect("utf8 path"),
        )),
        size: ImageSizeSpec::new(AxisSize::AtMost(50), AxisSize::AtMost(50)),
        rotation: ImageRotation::None,
        colors: ImageColorContext::default(),
        mask: Default::default(),
        frame: ImageFrameIndex::new(3),
        realization: Default::default(),
    };

    let started = Instant::now();
    let lookup = host.image_catalog.lookup(request.clone());

    assert!(
        started.elapsed() < Duration::from_millis(100),
        "image lookup should not wait for render-thread dimensions"
    );
    let ImageLookup::Pending(image) = lookup else {
        panic!("new image lookup should be pending");
    };
    assert_eq!(image.placement().width(), 50);
    assert_eq!(image.placement().height(), 50);
    match cmd_rx.try_recv().expect("queued image load") {
        RenderCommand::Asset(AssetCommand::ImageLoadFile { size, frame, .. }) => {
            assert_eq!(
                size,
                ImageSizeSpec::new(AxisSize::AtMost(50), AxisSize::AtMost(50))
            );
            assert_eq!(frame, ImageFrameIndex::new(3));
        }
        other => panic!("expected ImageLoadFile, got {other:?}"),
    }
    assert_eq!(
        host.image_catalog.lookup(request.clone()),
        ImageLookup::Pending(image.clone()),
        "duplicate lookup should reuse the same pending image"
    );
    assert!(
        cmd_rx.try_recv().is_err(),
        "duplicate lookup must not enqueue a second decode"
    );

    image_metadata.publish_terminal(
        image.load(),
        ImageDecodeTerminal::Ready(ResolvedImageMetadata::layout_is_image_pixels(
            25,
            50,
            0x12_34_56,
            false,
            Default::default(),
        )),
    );

    let ImageLookup::Ready(image) = host.image_catalog.lookup(request) else {
        panic!("decoded image lookup should be ready");
    };
    assert_eq!(image.metadata.layout.dimensions(), (25, 50));
    assert_eq!(
        image.metadata,
        ResolvedImageMetadata::layout_is_image_pixels(
            25,
            50,
            0x12_34_56,
            false,
            Default::default(),
        )
    );
}

#[test]
fn animation_frames_share_sequence_identity_and_retirement_advances_generation() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let catalog = AsyncImageCatalog::new(cmd_tx, None, Arc::new(ImageRenderState::default()));
    let source = ImageResolveSource::Data(ImageDataSource::Isolated(vec![b'G', b'I', b'F']));
    let mut request = ImageResolveRequest {
        spec: test_image_spec_identity("animated.gif"),
        source: source.clone(),
        size: ImageSizeSpec::default(),
        rotation: ImageRotation::None,
        colors: ImageColorContext::default(),
        mask: Default::default(),
        frame: ImageFrameIndex::new(0),
        realization: Default::default(),
    };

    catalog.lookup(request.clone());
    request.frame = ImageFrameIndex::new(1);
    catalog.lookup(request.clone());
    let sequence_for = |command| match command {
        RenderCommand::Asset(AssetCommand::ImageLoadData { sequence, .. }) => sequence,
        other => panic!("expected image data load, got {other:?}"),
    };
    let first = sequence_for(cmd_rx.try_recv().expect("first frame load"));
    let second = sequence_for(cmd_rx.try_recv().expect("second frame load"));
    assert_eq!(
        first, second,
        "frame index must not partition decoder state"
    );

    catalog.invalidate_animation(ImageAnimationInvalidation::Source(source));
    assert!(matches!(
        cmd_rx.try_recv().expect("sequence retirement"),
        RenderCommand::Asset(AssetCommand::ImageSequenceRetire {
            retirement: neomacs_display_protocol::ImageSequenceRetirement::One(sequence),
        }) if sequence == first
    ));

    request.frame = ImageFrameIndex::new(2);
    catalog.lookup(request);
    let replacement = sequence_for(cmd_rx.try_recv().expect("replacement frame load"));
    assert_ne!(replacement, first);
}

#[test]
fn primary_image_catalog_does_not_block_on_render_command_backpressure() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::bounded(1);
    cmd_tx
        .send(RenderCommand::Asset(AssetCommand::ImageRetire {
            image: ImageId::new(1),
        }))
        .expect("fill command queue");
    let request = ImageResolveRequest {
        spec: test_image_spec_identity("backpressure.png"),
        source: ImageResolveSource::Data(ImageDataSource::Isolated(vec![0x89, b'P', b'N', b'G'])),
        size: ImageSizeSpec::new(AxisSize::AtMost(24), AxisSize::AtMost(24)),
        rotation: ImageRotation::None,
        colors: ImageColorContext::default(),
        mask: Default::default(),
        frame: Default::default(),
        realization: Default::default(),
    };
    let (done_tx, done_rx) = crossbeam_channel::bounded(1);
    let worker_cmd_tx = cmd_tx.clone();
    let worker = std::thread::spawn(move || {
        let host = PrimaryWindowDisplayHost {
            cmd_tx: worker_cmd_tx.clone(),
            render_waker: None,
            font_sizing: FontSizing::xft(),
            primary_window_adopted: false,
            primary_frame_id: None,
            last_window_titles: Mutex::new(std::collections::HashMap::new()),
            font_metrics: None,
            primary_window_size: shared_primary_window_size(1600, 1800),
            image_catalog: test_image_catalog(
                &worker_cmd_tx,
                Arc::new(ImageRenderState::default()),
            ),
            resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
            resolved_webkits: Mutex::new(std::collections::HashMap::new()),
            resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
            render_capabilities: Arc::new(SharedRenderCapabilities::default()),
            requested_frame_shader: Mutex::new(None),
            #[cfg(feature = "neo-term")]
            terminal_state: super::TerminalHostState::new(new_shared_terminals()),
        };
        let lookup = host.image_catalog.lookup(request.clone());
        let duplicate_lookup = host.image_catalog.lookup(request);
        done_tx
            .send((lookup, duplicate_lookup))
            .expect("publish lookup results");
    });

    let first_result = done_rx.recv_timeout(Duration::from_millis(100));
    cmd_rx.try_recv().expect("drain backpressuring command");
    let (lookup, duplicate_lookup) = match first_result {
        Ok(result) => result,
        Err(error) => {
            let _result = done_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("blocked lookup should finish after draining queue");
            worker.join().expect("lookup worker");
            panic!("image catalog blocked on render command backpressure: {error}");
        }
    };
    worker.join().expect("lookup worker");

    let ImageLookup::Pending(image) = lookup else {
        panic!("backpressured lookup should remain pending");
    };
    assert_eq!(duplicate_lookup, ImageLookup::Pending(image.clone()));
    assert!(matches!(
        cmd_rx.recv_timeout(Duration::from_secs(1)),
        Ok(RenderCommand::Asset(AssetCommand::ImageLoadData { load, .. }))
            if load == image.load()
    ));
    assert!(cmd_rx.try_recv().is_err(), "retry must not enqueue twice");
}

#[test]
fn primary_image_catalog_does_not_wait_for_renderer_metadata_lock() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let image_metadata = Arc::new(ImageRenderState::default());
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::clone(&image_metadata)),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };
    let request = ImageResolveRequest {
        spec: test_image_spec_identity("metadata-lock.png"),
        source: ImageResolveSource::Data(ImageDataSource::Isolated(vec![0x89, b'P', b'N', b'G'])),
        size: ImageSizeSpec::new(AxisSize::AtMost(18), AxisSize::AtMost(18)),
        rotation: ImageRotation::None,
        colors: ImageColorContext::default(),
        mask: Default::default(),
        frame: Default::default(),
        realization: Default::default(),
    };
    let ImageLookup::Pending(expected) = host.image_catalog.lookup(request.clone()) else {
        panic!("new image should be pending");
    };
    cmd_rx.try_recv().expect("queued image command");

    let locked_metadata = Arc::clone(&image_metadata);
    let (locked_tx, locked_rx) = crossbeam_channel::bounded(1);
    let (release_tx, release_rx) = crossbeam_channel::bounded::<()>(1);
    let locker = std::thread::spawn(move || {
        let _publication = locked_metadata.begin_terminal_publication();
        locked_tx.send(()).expect("publish locked state");
        let _ = release_rx.recv_timeout(Duration::from_millis(250));
    });
    locked_rx.recv().expect("renderer metadata lock acquired");

    let started = Instant::now();
    let lookup = host.image_catalog.lookup(request);
    let elapsed = started.elapsed();
    drop(release_tx);
    locker.join().expect("metadata locker");
    assert!(
        elapsed < Duration::from_millis(100),
        "image catalog waited {elapsed:?} for renderer metadata lock"
    );
    assert_eq!(lookup, ImageLookup::Pending(expected));
}

#[test]
fn primary_display_host_expands_tilde_in_image_file_before_render_command() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };
    let request = ImageResolveRequest {
        spec: test_image_spec_identity("~/Pictures/Pik.png"),
        source: ImageResolveSource::File(LispString::from_utf8("~/Pictures/Pik.png")),
        size: ImageSizeSpec::new(AxisSize::AtMost(0), AxisSize::AtMost(24)),
        rotation: ImageRotation::None,
        colors: ImageColorContext::default(),
        mask: Default::default(),
        frame: Default::default(),
        realization: Default::default(),
    };

    assert!(matches!(
        host.image_catalog.lookup(request),
        ImageLookup::Pending(_)
    ));

    let home = std::env::var("HOME").expect("HOME for GNU tilde expansion");
    let expected = Path::new(&home)
        .join("Pictures/Pik.png")
        .to_string_lossy()
        .into_owned();
    assert!(matches!(
        cmd_rx.try_recv().expect("queued image load"),
        RenderCommand::Asset(AssetCommand::ImageLoadFile { path, .. })
            if path == expected
    ));
}

#[test]
fn failed_image_decode_wakes_waiter_and_is_negative_cached() {
    let shared: SharedImageRenderState = Arc::new(ImageRenderState::default());
    let load = test_image_load(77, 1);
    let publisher = Arc::clone(&shared);
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(10));
        publisher.publish_terminal(load, ImageDecodeTerminal::Failed("bad image".to_owned()));
    });

    let started = Instant::now();
    assert_eq!(
        wait_for_image_metadata(&shared, load, Duration::from_secs(1)),
        Some(ImageDecodeTerminal::Failed("bad image".to_owned()))
    );
    assert!(
        started.elapsed() < Duration::from_millis(250),
        "failure should wake the waiter instead of consuming the timeout"
    );
    worker.join().expect("publisher thread");

    let cached = Instant::now();
    assert_eq!(
        wait_for_image_metadata(&shared, load, Duration::from_secs(1)),
        Some(ImageDecodeTerminal::Failed("bad image".to_owned()))
    );
    assert!(
        cached.elapsed() < Duration::from_millis(250),
        "a terminal failure should be returned from the negative cache"
    );
}

#[test]
fn primary_display_host_resolve_image_sync_returns_cached_decode_failure_promptly() {
    let (cmd_tx, _cmd_rx) = crossbeam_channel::unbounded();
    let image_metadata: SharedImageRenderState = Arc::new(ImageRenderState::default());
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::clone(&image_metadata)),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };
    let request = ImageResolveRequest {
        spec: test_image_spec_identity("failed-decode.png"),
        source: ImageResolveSource::Data(ImageDataSource::Isolated(vec![0xde, 0xad])),
        size: ImageSizeSpec::new(AxisSize::AtMost(0), AxisSize::AtMost(0)),
        rotation: ImageRotation::None,
        colors: ImageColorContext::default(),
        mask: Default::default(),
        frame: Default::default(),
        realization: Default::default(),
    };
    let ImageLookup::Pending(image) = host.image_catalog.lookup(request.clone()) else {
        panic!("new image should be pending");
    };
    image_metadata.publish_terminal(
        image.load(),
        ImageDecodeTerminal::Failed("image decode failed".to_owned()),
    );

    for _ in 0..2 {
        let started = Instant::now();
        let ImageLookup::Failed(failed) = host.image_catalog.lookup(request.clone()) else {
            panic!("failed decode should be negative-cached");
        };
        assert_eq!(failed.placement(), image.placement());
        assert_eq!(failed.error, "image decode failed");
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "failed catalog lookup should return from the negative cache"
        );
    }

    for _ in 0..2 {
        let started = Instant::now();
        assert_eq!(
            neovm_core::emacs_core::DisplayHost::resolve_image_sync(&host, request.clone()),
            Err("image decode failed".to_owned())
        );
        assert!(
            started.elapsed() < Duration::from_millis(250),
            "cached decode failure should not wait for the one-second timeout"
        );
    }
}

#[test]
fn primary_display_host_request_video_queues_create_once_with_stable_id() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };
    let request = VideoResolveRequest {
        source: VideoResolveSource::File(LispString::from_utf8("/tmp/demo.mp4")),
        loop_count: -1,
        autoplay: true,
    };

    let first = neovm_core::emacs_core::DisplayHost::request_video(&host, request.clone())
        .expect("request video")
        .expect("video handle");
    let second = neovm_core::emacs_core::DisplayHost::request_video(&host, request)
        .expect("request cached video")
        .expect("video handle");

    assert_eq!(first, second);
    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        RenderCommand::Asset(AssetCommand::VideoCreate {
            id,
            source,
            loop_count,
            autoplay,
        }) if *id == first.video_id
            && matches!(source, MediaSource::File(path) if path == "/tmp/demo.mp4")
            && *loop_count == -1
            && *autoplay
    ));
}

#[test]
fn resolved_video_registry_never_evicts_a_still_referenceable_identity() {
    let mut registry = super::ResolvedVideoRegistry::default();
    for index in 0..80 {
        registry.insert(
            VideoResolveRequest {
                source: VideoResolveSource::Uri(LispString::from_utf8(&format!(
                    "https://example.com/{index}.mp4"
                ))),
                loop_count: 0,
                autoplay: false,
            },
            super::ResolvedVideo {
                video_id: index as u32 + 1,
            },
        );
        assert_eq!(
            registry
                .get(&VideoResolveRequest {
                    source: VideoResolveSource::Uri(LispString::from_utf8(&format!(
                        "https://example.com/{index}.mp4"
                    ))),
                    loop_count: 0,
                    autoplay: false,
                })
                .map(|video| video.video_id),
            Some(index as u32 + 1)
        );
    }
    assert_eq!(registry.entries.len(), 80);
}

#[test]
fn primary_display_host_request_video_preserves_uri_source() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };
    let request = VideoResolveRequest {
        source: VideoResolveSource::Uri(LispString::from_utf8("https://example.com/video.mp4")),
        loop_count: 0,
        autoplay: false,
    };

    let resolved = neovm_core::emacs_core::DisplayHost::request_video(&host, request)
        .expect("request video")
        .expect("video handle");

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        RenderCommand::Asset(AssetCommand::VideoCreate {
            id,
            source,
            loop_count,
            autoplay,
        }) if *id == resolved.video_id
            && matches!(source, MediaSource::Uri(uri) if uri == "https://example.com/video.mp4")
            && *loop_count == 0
            && !*autoplay
    ));
}

#[test]
fn primary_display_host_request_webkit_queues_create_and_load_once_with_stable_id() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };
    let request = WebKitResolveRequest {
        source: WebKitResolveSource::Uri(LispString::from_utf8("https://example.com")),
        width: 400,
        height: 300,
    };

    let first = neovm_core::emacs_core::DisplayHost::request_webkit(&host, request.clone())
        .expect("request webkit")
        .expect("webkit handle");
    let second = neovm_core::emacs_core::DisplayHost::request_webkit(&host, request)
        .expect("request cached webkit")
        .expect("webkit handle");

    assert_eq!(first, second);
    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Create(create)))
            if create.id == first.webview_id
                && create.initial_size.width() == 400
                && create.initial_size.height() == 300
                && matches!(
                    &create.initial_navigation,
                    Some(NavigationTarget::Uri(url)) if url == "https://example.com"
                )
    ));
}

#[test]
fn primary_display_host_preserves_file_navigation_as_a_typed_path() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };
    let path = std::path::PathBuf::from("/tmp/neomacs web#view.html");
    let request = WebKitResolveRequest {
        source: WebKitResolveSource::File(LispString::from_utf8(path.to_string_lossy().as_ref())),
        width: 400,
        height: 300,
    };

    neovm_core::emacs_core::DisplayHost::request_webkit(&host, request)
        .expect("request file webkit")
        .expect("webkit handle");

    let command = cmd_rx.try_recv().expect("queued WebView create");
    assert!(matches!(
        command,
        RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Create(create)))
            if create.initial_navigation == Some(NavigationTarget::File(path))
    ));
}

#[test]
fn primary_display_host_xwidget_lifecycle_uses_explicit_xwidget_id() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 1800),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };

    let id = WebViewId::new(42);
    neovm_core::emacs_core::DisplayHost::create_webkit_xwidget(&host, id, 400, 300)
        .expect("create xwidget");
    neovm_core::emacs_core::DisplayHost::load_webkit_xwidget_uri(
        &host,
        id,
        LispString::from_utf8("https://example.com"),
    )
    .expect("load xwidget");
    neovm_core::emacs_core::DisplayHost::resize_webkit_xwidget(&host, id, 320, 240)
        .expect("resize xwidget");
    neovm_core::emacs_core::DisplayHost::destroy_webkit_xwidget(&host, id)
        .expect("destroy xwidget");

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert_eq!(commands.len(), 4);
    assert!(matches!(
        &commands[0],
        RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Create(create)))
            if create.id == id
                && create.initial_size.width() == 400
                && create.initial_size.height() == 300
    ));
    assert!(matches!(
        &commands[1],
        RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Navigate {
            id: command_id,
            target: NavigationTarget::Uri(url),
        })) if *command_id == id && url == "https://example.com"
    ));
    assert!(matches!(
        &commands[2],
        RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::SetModelSize {
            id: command_id,
            size,
        })) if *command_id == id && size.width() == 320 && size.height() == 240
    ));
    assert!(matches!(
        &commands[3],
        RenderCommand::Asset(AssetCommand::WebView(WebViewCommand::Close { id: command_id }))
            if *command_id == id
    ));
}

#[test]
fn bootstrap_gui_frame_adoption_routes_future_resizes_to_primary_window() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 843, 489, gui_display());
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();

    eval.set_display_host(Box::new(PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(843, 489),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    }));

    adopt_existing_primary_gui_frame(&mut eval).expect("bootstrap GUI frame should adopt");
    eval.eval_str("(set-frame-size (selected-frame) 132 42)")
        .expect("set-frame-size should succeed");

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert!(
        commands.iter().any(|cmd| matches!(
            cmd,
            RenderCommand::Window(WindowCommand::SetWindowTitle { .. })
        )),
        "expected bootstrap adoption to set the primary window title, got {commands:?}"
    );
    assert!(
        commands.iter().any(|cmd| matches!(
            cmd,
            RenderCommand::Window(WindowCommand::SetFrameGeometryHints {
                frame: FrameRef::Primary,
                ..
            })
        )),
        "expected bootstrap adoption to publish primary window geometry hints, got {commands:?}"
    );
    assert!(
        commands.iter().any(|cmd| matches!(
            cmd,
            RenderCommand::Window(WindowCommand::AdoptPrimaryFrame { .. })
        )),
        "expected bootstrap adoption to publish the primary frame identity, got {commands:?}"
    );
    assert!(
        commands.iter().any(|cmd| matches!(
            cmd,
            RenderCommand::Window(WindowCommand::ResizeWindow {
                frame: FrameRef::Primary,
                ..
            })
        )),
        "expected bootstrap resize to target the adopted primary window, got {commands:?}"
    );
}

#[test]
fn primary_window_resize_does_not_wait_for_host_acknowledgement() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let shared = shared_primary_window_size(843, 489);
    let mut host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: true,
        primary_frame_id: Some(FrameId(0x100000001)),
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: Arc::clone(&shared),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };

    let started = Instant::now();
    neovm_core::emacs_core::DisplayHost::resize_gui_frame(
        &mut host,
        GuiFrameHostRequest {
            frame_id: FrameId(0x100000001),
            width: 1068,
            height: 1386,
            title: LispString::from_utf8("Neomacs"),
            geometry_hints: GuiFrameGeometryHints {
                base_width: 29,
                base_height: 31,
                min_width: 29,
                min_height: 31,
                width_inc: 13,
                height_inc: 31,
            },
            fullscreen: None,
        },
    )
    .expect("primary resize should succeed");

    assert!(
        started.elapsed() < Duration::from_millis(10),
        "primary resize should stay asynchronous; geometry queries do the waiting"
    );

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert!(
        commands.iter().any(|cmd| matches!(
            cmd,
            RenderCommand::Window(WindowCommand::ResizeWindow {
                frame: FrameRef::Primary,
                width: 1068,
                height: 1386,
                ..
            })
        )),
        "expected primary resize command, got {commands:?}"
    );
}

#[test]
fn primary_window_display_host_forwards_visual_config_to_renderer() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let mut host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: true,
        primary_frame_id: Some(FrameId(0x100000001)),
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(843, 489),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };

    let mut config = neomacs_display_protocol::VisualConfig::default();
    config.cursor_blink.enabled = false;
    config.cursor_blink.interval = std::time::Duration::from_millis(250);
    neovm_core::emacs_core::DisplayHost::set_visual_config(&mut host, config)
        .expect("visual config command should forward");

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert_eq!(commands.len(), 1);
    assert!(matches!(
        &commands[0],
        RenderCommand::Config(ConfigCommand::SetVisualConfig(config))
            if !config.cursor_blink.enabled
                && config.cursor_blink.interval == std::time::Duration::from_millis(250)
    ));
}

#[test]
fn primary_window_display_host_round_trips_clipboard_requests_through_renderer() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let worker = std::thread::spawn(move || {
        let RenderCommand::Clipboard(ClipboardCommand::SetText {
            selection,
            text,
            reply,
            ..
        }) = cmd_rx.recv().unwrap()
        else {
            panic!("expected clipboard set command");
        };
        assert_eq!(selection, ClipboardSelection::Clipboard);
        assert_eq!(text.as_deref(), Some("copied"));
        reply.send(Ok(())).unwrap();

        let RenderCommand::Clipboard(ClipboardCommand::GetText {
            selection, reply, ..
        }) = cmd_rx.recv().unwrap()
        else {
            panic!("expected PRIMARY get command");
        };
        assert_eq!(selection, ClipboardSelection::Primary);
        reply.send(Ok(Some("selected".to_owned()))).unwrap();
    });
    let mut host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: true,
        primary_frame_id: Some(FrameId(0x100000001)),
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(843, 489),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };

    neovm_core::emacs_core::DisplayHost::set_clipboard_text(&mut host, Some("copied"))
        .expect("clipboard set should round-trip through renderer");
    assert_eq!(
        neovm_core::emacs_core::DisplayHost::primary_selection_text(&mut host).unwrap(),
        Some("selected".to_owned())
    );
    worker.join().unwrap();
}

#[test]
fn tty_display_host_reports_clipboard_as_unsupported() {
    let mut host = TtyPopupDisplayHost::new(Arc::new(AtomicBool::new(false)));

    assert_eq!(
        neovm_core::emacs_core::DisplayHost::set_clipboard_text(&mut host, Some("copied")),
        Err("system clipboard is unavailable in TTY mode".to_owned())
    );
    assert_eq!(
        neovm_core::emacs_core::DisplayHost::clipboard_text(&mut host),
        Err("system clipboard is unavailable in TTY mode".to_owned())
    );
    assert_eq!(
        neovm_core::emacs_core::DisplayHost::set_primary_selection_text(
            &mut host,
            Some("selected")
        ),
        Err("PRIMARY selection is unavailable in TTY mode".to_owned())
    );
    assert_eq!(
        neovm_core::emacs_core::DisplayHost::primary_selection_text(&mut host),
        Err("PRIMARY selection is unavailable in TTY mode".to_owned())
    );
}

#[test]
fn redisplay_title_sync_formats_frame_title_format_for_primary_window() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 843, 489, gui_display());
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();

    eval.set_display_host(Box::new(PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(843, 489),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    }));

    adopt_existing_primary_gui_frame(&mut eval).expect("bootstrap GUI frame should adopt");
    let _ = cmd_rx.try_iter().collect::<Vec<_>>();

    eval.eval_str(r#"(setq frame-title-format "oracle-title")"#)
        .expect("frame-title-format should set");
    sync_live_gui_frame_titles(&mut eval);

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert!(
        commands.iter().any(|cmd| matches!(
            cmd,
            RenderCommand::Window(WindowCommand::SetFrameWindowTitle {
                frame: FrameRef::Primary,
                title
            }) if title == "oracle-title"
        )),
        "expected redisplay title sync to publish the formatted primary title, got {commands:?}"
    );
}

#[test]
fn frame_host_title_formats_the_restored_runtime_system_name() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 843, 489, gui_display());
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    eval.set_display_host(Box::new(PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(843, 489),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    }));
    let expected_system_name: String = hostname::get()
        .expect("OS hostname")
        .to_string_lossy()
        .chars()
        .map(|character| match character {
            ' ' | '\t' => '-',
            other => other,
        })
        .collect();

    adopt_existing_primary_gui_frame(&mut eval).expect("bootstrap GUI frame should adopt");
    let _ = cmd_rx.try_iter().collect::<Vec<_>>();
    eval.eval_str(r#"(setq frame-title-format '("host:" system-name))"#)
        .expect("frame-title-format should set");
    sync_live_gui_frame_titles(&mut eval);

    let expected_title = format!("host:{expected_system_name}");
    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert!(
        commands.iter().any(|command| matches!(
            command,
            RenderCommand::Window(WindowCommand::SetFrameWindowTitle {
                frame: FrameRef::Primary,
                title,
            }) if title == &expected_title
        )),
        "restored runtime hostname must reach the native title command; got {commands:?}"
    );
}

#[test]
fn tty_terminal_host_delete_terminal_sends_shutdown() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let mut host = TtyTerminalHost { cmd_tx };

    host.delete_terminal()
        .expect("delete terminal should succeed");

    match cmd_rx
        .try_recv()
        .expect("shutdown command should be queued")
    {
        RenderCommand::Lifecycle(LifecycleCommand::Shutdown) => {}
        other => panic!("expected Shutdown, got {other:?}"),
    }
}

#[test]
fn current_layout_frame_follows_selected_frame() {
    let mut eval = Context::new();
    let b1 = eval.buffer_manager_mut().create_buffer("*one*");
    let b2 = eval.buffer_manager_mut().create_buffer("*two*");
    let f1 = eval.frame_manager_mut().create_frame("F1", 80, 24, b1);
    let f2 = eval.frame_manager_mut().create_frame("F2", 80, 24, b2);

    assert_eq!(current_layout_frame_id(&eval), Some(f1));
    assert!(eval.frame_manager_mut().select_frame(f2));
    assert_eq!(current_layout_frame_id(&eval), Some(f2));
}

#[test]
fn current_layout_frame_tracks_surrogate_after_bootstrap_frame_deletion() {
    let mut eval = Context::new();
    let b1 = eval.buffer_manager_mut().create_buffer("*one*");
    let b2 = eval.buffer_manager_mut().create_buffer("*two*");
    let f1 = eval.frame_manager_mut().create_frame("F1", 80, 24, b1);
    let f2 = eval.frame_manager_mut().create_frame("F2", 80, 24, b2);

    assert_eq!(current_layout_frame_id(&eval), Some(f1));
    assert!(eval.frame_manager_mut().delete_frame(f1));
    assert_eq!(current_layout_frame_id(&eval), Some(f2));
}

#[test]
fn publish_gui_frame_sends_opening_frame_before_startup_lisp() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    REDISPLAY_RUNTIME.with(|runtime| runtime.enable_cosmic_metrics());
    let (frame_tx, frame_rx) = crossbeam_channel::unbounded();
    let active_before = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.active_presentation());

    publish_gui_frame(&mut eval, &frame_tx, None);

    let display_state = frame_rx
        .try_recv()
        .expect("opening GUI frame should be published");
    assert_eq!(current_layout_frame_id(&eval), Some(frame_id));
    assert_eq!(display_state.frame_placement.parent(), None);
    assert!(display_state.frame_cols > 0);
    assert!(display_state.frame_rows > 0);
    assert!(
        !display_state.window_matrices.is_empty(),
        "opening GUI frame should carry at least one window matrix"
    );
    let presentation =
        neovm_core::window::geometry::PresentationId::new(display_state.presentation_id.get());
    let frame = eval.frame_manager().get(frame_id).expect("published frame");
    assert_eq!(frame.active_presentation(), active_before);
    assert!(frame.is_display_presentation_prepared(presentation));
}

#[test]
fn publish_gui_frame_sends_every_visible_top_level_frame_tree() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let selected = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, selected, &gui_startup());

    let second_buffer = eval.buffer_manager_mut().create_buffer("*second-frame*");
    let second = eval
        .frame_manager_mut()
        .create_frame("second", 960, 640, second_buffer);
    eval.frame_manager_mut()
        .get_mut(second)
        .expect("second frame")
        .set_window_system(Some(Value::symbol("neo")));
    assert_eq!(
        eval.frame_manager().selected_frame().map(|frame| frame.id),
        Some(selected),
        "creating another top-level frame must not make it selected"
    );

    REDISPLAY_RUNTIME.with(|runtime| runtime.enable_cosmic_metrics());
    let (frame_tx, frame_rx) = crossbeam_channel::unbounded();

    publish_gui_frame(&mut eval, &frame_tx, None);

    let mut published = frame_rx
        .try_iter()
        .map(|state| FrameId(state.frame_placement.frame().get()))
        .collect::<Vec<_>>();
    published.sort_by_key(|frame_id| frame_id.0);
    assert_eq!(published, vec![selected, second]);
}

#[test]
fn rejected_gui_frame_is_discarded_instead_of_becoming_active() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    REDISPLAY_RUNTIME.with(|runtime| runtime.enable_cosmic_metrics());
    let (frame_tx, frame_rx) = crossbeam_channel::unbounded();
    drop(frame_rx);

    publish_gui_frame(&mut eval, &frame_tx, None);

    let frame = eval.frame_manager().get(frame_id).expect("rejected frame");
    assert_eq!(frame.active_presentation(), None);
    assert!(!frame.has_prepared_display_presentations());
}

#[test]
fn early_cli_handles_gnu_c_owned_help_and_version_options() {
    assert_eq!(
        classify_early_cli_action(
            ["./target/release/neomacs", "--help"]
                .into_iter()
                .map(str::to_string)
        ),
        Some(EarlyCliAction::PrintHelp {
            program: "./target/release/neomacs".to_string()
        })
    );
    assert_eq!(
        classify_early_cli_action(
            ["./target/release/neomacs", "-version"]
                .into_iter()
                .map(str::to_string)
        ),
        Some(EarlyCliAction::PrintVersion)
    );
    assert_eq!(
        classify_early_cli_action(
            ["./target/release/neomacs", "--fingerprint"]
                .into_iter()
                .map(str::to_string)
        ),
        Some(EarlyCliAction::PrintFingerprint)
    );
    assert_eq!(
        classify_early_cli_action(
            ["./target/release/neomacs", "--", "--help"]
                .into_iter()
                .map(str::to_string)
        ),
        None
    );
}

#[test]
fn early_cli_help_uses_invoked_program_name_and_gnu_style_usage() {
    let help = render_help_text("/tmp/neomacs");
    assert!(help.starts_with("Usage: /tmp/neomacs [OPTION-OR-FILENAME]...\n\n"));
    assert!(help.contains("--help                          display this help and exit"));
    assert!(help.contains("--fingerprint                   output fingerprint and exit"));
    assert!(help.contains("--quick, -Q                 equivalent to:"));
}

#[test]
fn early_cli_version_reports_neomacs_identity() {
    let version = render_version_text();
    assert!(version.starts_with(&format!(
        "Neomacs {}\nGit commit: ",
        neomacs_display_runtime::VERSION
    )));
    let revision = version
        .lines()
        .find_map(|line| line.strip_prefix("Git commit: "))
        .expect("Git revision line");
    let revision = revision
        .split_once(' ')
        .map_or(revision, |(revision, _)| revision);
    assert!(
        revision == "unknown"
            || ((revision.len() == 40 || revision.len() == 64)
                && revision.bytes().all(|byte| byte.is_ascii_hexdigit())),
        "Git revision should be a complete hexadecimal object ID or an explicit fallback: {revision}"
    );
    assert!(version.contains("Source date: "));
    assert!(version.contains("Build: "));
    assert!(version.contains(" with rustc "));
    assert!(version.contains("Standalone Rust binary for Neomacs"));
}

#[test]
fn early_cli_fingerprint_reports_shared_pdump_fingerprint() {
    assert_eq!(
        render_fingerprint_text(),
        format!("{}\n", neovm_core::emacs_core::pdump::fingerprint_hex())
    );
}

#[test]
fn startup_image_error_renderer_surfaces_heapless_payload() {
    let payload = Value::symbol(intern(
        "failed to load final image /tmp/neomacs.pdump: boom",
    ));
    let err = EvalError::signal(intern("error"), vec![payload], Some(payload));

    assert_eq!(
        render_startup_image_error(&err),
        "failed to load final image /tmp/neomacs.pdump: boom"
    );
}

#[test]
fn startup_option_parser_promotes_nw_and_strips_c_owned_display_flags() {
    let parsed = parse_startup_options(
        [
            "neomacs",
            "-nw",
            "--display",
            ":1",
            "--terminal=/dev/pts/7",
            "README.md",
        ]
        .into_iter()
        .map(str::to_string),
    )
    .expect("startup options should parse");

    assert_eq!(parsed.frontend, FrontendKind::Tty);
    assert!(!parsed.noninteractive);
    assert_eq!(parsed.terminal_device.as_deref(), Some("/dev/pts/7"));
    assert_eq!(
        parsed.forwarded_args,
        vec!["neomacs".to_string(), "README.md".to_string()]
    );
}

#[test]
fn startup_option_parser_promotes_batch_to_noninteractive_and_strips_batch_flag() {
    let parsed = parse_startup_options(
        ["neomacs", "--batch", "-Q", "--eval", "(princ 1)"]
            .into_iter()
            .map(str::to_string),
    )
    .expect("startup options should parse");

    assert_eq!(parsed.frontend, FrontendKind::Tty);
    assert!(parsed.noninteractive);
    assert_eq!(
        parsed.forwarded_args,
        vec![
            "neomacs".to_string(),
            "-Q".to_string(),
            "--eval".to_string(),
            "(princ 1)".to_string()
        ]
    );
}

#[test]
fn batch_tty_mode_does_not_spawn_input_reader() {
    let batch = tty_batch_startup_with_args(&["-Q"]);
    assert!(!should_enable_live_tty_io(&batch));

    let interactive = StartupOptions {
        frontend: FrontendKind::Tty,
        noninteractive: false,
        ..batch
    };
    assert!(should_enable_live_tty_io(&interactive));
}

#[test]
fn batch_tty_mode_leaves_redisplay_callback_unset() {
    let batch = tty_batch_startup_with_args(&["-Q"]);
    let mut eval = Context::new();
    maybe_install_tty_redisplay_callback(&mut eval, &batch);
    assert!(
        eval.redisplay_fn.is_none(),
        "batch tty should not install a live redisplay callback"
    );

    let interactive = StartupOptions {
        frontend: FrontendKind::Tty,
        noninteractive: false,
        ..batch
    };
    let mut interactive_eval = Context::new();
    maybe_install_tty_redisplay_callback(&mut interactive_eval, &interactive);
    assert!(
        interactive_eval.redisplay_fn.is_some(),
        "interactive tty should install a redisplay callback"
    );
}

#[test]
fn configure_gnu_startup_state_marks_bootstrap_gui_frame_as_initial_frame() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let terminal_frame = *eval
        .obarray()
        .symbol_value("terminal-frame")
        .expect("terminal-frame");
    let Some(terminal_frame_id) = terminal_frame.as_frame_id() else {
        panic!("GUI startup should seed a hidden terminal frame, got {terminal_frame:?}");
    };
    let terminal_frame_id = FrameId(terminal_frame_id);
    let terminal_frame = eval
        .frame_manager()
        .get(terminal_frame_id)
        .expect("hidden terminal frame");

    assert_eq!(
        terminal_frame.visible, false,
        "GNU frame-initialize should delete a hidden terminal frame, not the opening GUI frame"
    );
    assert!(
        terminal_frame.effective_window_system().is_none(),
        "hidden startup terminal frame must stay non-GUI"
    );
    assert_eq!(
        terminal_frame.parameter("display-type"),
        Some(Value::symbol("color")),
        "hidden startup terminal frame should use TTY face classification"
    );
    assert_eq!(
        terminal_frame.parameter("background-mode"),
        Some(Value::symbol(detect_tty_background_mode())),
        "hidden startup terminal frame should use TTY background classification"
    );
    assert_eq!(
        eval.obarray()
            .symbol_value("frame-initial-frame")
            .and_then(|value| value.as_frame_id()),
        Some(frame_id.0)
    );
    assert_eq!(
        eval.obarray().symbol_value("frame-initial-frame-alist"),
        Some(&Value::list(vec![Value::cons(
            Value::symbol("window-system"),
            Value::symbol("neo"),
        )]))
    );
    assert_eq!(
        eval.obarray()
            .symbol_value("default-minibuffer-frame")
            .and_then(|value| value.as_frame_id()),
        Some(frame_id.0)
    );
}

#[test]
fn configure_gnu_startup_state_reports_neo_window_system_for_gui_boots() {
    let mut eval = Context::new();
    configure_gnu_startup_state(&mut eval, FrameId(42), &gui_startup());

    assert_eq!(
        eval.obarray().symbol_value("window-system"),
        Some(&Value::symbol("neo"))
    );
    assert_eq!(
        eval.obarray().symbol_value("initial-window-system"),
        Some(&Value::symbol("neo"))
    );
}

#[test]
fn gui_startup_terminal_frame_uses_separate_terminal_owner() {
    let mut eval = Context::new();
    let buffer_id = eval.buffer_manager_mut().create_buffer("*scratch*");
    let gui_frame_id = eval
        .frame_manager_mut()
        .create_frame("F1", 960, 640, buffer_id);
    eval.frame_manager_mut()
        .get_mut(gui_frame_id)
        .expect("GUI frame")
        .set_window_system(Some(Value::symbol("neo")));
    let _ = eval.frame_manager_mut().select_frame(gui_frame_id);

    configure_gnu_startup_state(&mut eval, gui_frame_id, &gui_startup());

    let gui_terminal_id = eval
        .frame_manager()
        .get(gui_frame_id)
        .expect("GUI frame")
        .terminal_id;
    let startup_terminal_frame = eval
        .frame_manager()
        .frame_list()
        .into_iter()
        .filter(|frame_id| *frame_id != gui_frame_id)
        .find_map(|frame_id| eval.frame_manager().get(frame_id))
        .expect("hidden startup terminal frame");

    assert!(startup_terminal_frame.effective_window_system().is_none());
    assert_ne!(startup_terminal_frame.terminal_id, gui_terminal_id);
    let terminal_types = eval
        .eval_str(
            "(list (terminal-live-p (frame-terminal frame-initial-frame))
                   (terminal-live-p (frame-terminal terminal-frame)))",
        )
        .expect("terminal-live-p should evaluate for startup frames");
    assert_eq!(
        print_value_with_eval(&mut eval, &terminal_types),
        "(neo t)",
        "hidden startup terminal must stay tty-typed even while GUI frame is selected"
    );
}

#[test]
fn gui_startup_hidden_terminal_frame_matches_tty_face_specs() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"
        (condition-case err
            (list
             (terminal-live-p (frame-terminal frame-initial-frame))
             (terminal-live-p (frame-terminal terminal-frame))
             (framep-on-display terminal-frame)
             (window-system terminal-frame)
             (display-color-cells terminal-frame)
             (face-spec-choose
              '((((class color grayscale) (min-colors 88)) :foreground "red")
                (((type tty)) :foreground "blue"))
              terminal-frame
              'no-match)
             (condition-case err
                 (x-display-color-cells terminal-frame)
               (error (error-message-string err))))
          (error (list 'error (error-message-string err))))
        "#,
        )
        .expect("hidden terminal face probe should evaluate");

    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(neo t t nil 0 (:foreground \"blue\") \"Window system frame should be used\")",
        "hidden startup terminal frame must use tty display queries without weakening explicit X errors"
    );
}

#[test]
fn gui_startup_ediff_window_parameters_use_live_display_pixels() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"
        (condition-case err
            (progn
              (require 'ediff-wind)
              (list
               (display-pixel-height)
               (display-pixel-width)
               (cdr (assq 'top ediff-control-frame-parameters))
               (cdr (assq 'left ediff-control-frame-parameters))))
          (error (list 'error (error-message-string err))))
        "#,
        )
        .expect("ediff window probe should evaluate");

    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(25 80 26 81)",
        "ediff-wind should load with GUI display pixel queries active"
    );
}

#[test]
fn gui_startup_display_mm_width_accepts_selected_frame() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let result = eval
        .eval_str("(display-mm-width (selected-frame))")
        .expect("display-mm-width should accept an explicit live GUI frame");

    assert!(
        result.is_nil() || result.as_fixnum().is_some(),
        "GNU returns either the display width in millimeters or nil when it is unknown"
    );
}

#[test]
fn cl_generic_context_dispatch_uses_neo_window_system_method() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let rendered = eval
        .eval_str(
            r#"
        (progn
          (cl-defgeneric neomacs--ctx-probe ())
          (cl-defmethod neomacs--ctx-probe (&context (window-system nil)) 'tty)
          (cl-defmethod neomacs--ctx-probe (&context (window-system neo)) 'neo)
          (let ((window-system 'neo))
            (neomacs--ctx-probe)))
        "#,
        )
        .map(|value| print_value_with_eval(&mut eval, &value))
        .unwrap_or_else(|err| format!("{err:?}"));
    assert_eq!(rendered, "neo");
}

#[test]
fn pdump_preserves_neo_term_generic_methods() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    load_neomacs_gui_term_layer(&mut eval);

    let pre = eval
        .eval_str(
            r#"
        (let ((window-system 'neo))
          (window-system-initialization)
          neomacs-initialized)
        "#,
        )
        .map(|value| print_value_with_eval(&mut eval, &value))
        .unwrap_or_else(|err| format!("{err:?}"));

    let post = eval
        .eval_str(
            r#"
        (progn
          (load "term/neo-win" nil t)
          (setq neomacs-initialized nil)
          (let ((window-system 'neo))
            (window-system-initialization)
            neomacs-initialized))
        "#,
        )
        .map(|value| print_value_with_eval(&mut eval, &value))
        .unwrap_or_else(|err| format!("{err:?}"));

    assert_eq!(
        pre, "t",
        "runtime pdump lost neo generic methods before reload"
    );
    assert_eq!(
        post, "t",
        "reloading term/neo-win should keep neo init working"
    );
}

#[test]
fn neo_win_registers_neo_display_format() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");

    let rendered = eval
        .eval_str(
            r#"
        (progn
          (load "term/neo-win.el" nil t)
          (list
           (window-system-for-display ":0")
           (window-system-for-display "neo")
           (cdr (car display-format-alist))))
        "#,
        )
        .map(|value| print_value_with_eval(&mut eval, &value))
        .unwrap_or_else(|err| format!("{err:?}"));

    assert_eq!(rendered, "(neo neo neo)");
}

#[test]
fn neo_window_system_initialization_preserves_gnu_clipboard_policy_and_user_customization() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");

    let rendered = eval
        .eval_str(
            r#"
        (progn
          (require 'cl-lib)
          (load "term/neo-win" nil t)
          (let ((gnu-defaults
                 (and (eq interprogram-cut-function #'gui-select-text)
                      (eq interprogram-paste-function #'gui-selection-value))))
            (setq neomacs-initialized nil
                  command-line-args '("neomacs" "-Q" "--")
                  window-setup-hook nil
                  interprogram-cut-function #'ignore
                  interprogram-paste-function #'identity)
            (let ((window-system 'neo))
              (window-system-initialization))
            (and gnu-defaults
                 (equal command-line-args '("neomacs" "-Q" "--"))
                 (memq #'neomacs--window-setup window-setup-hook)
                 (eq interprogram-cut-function #'ignore)
                 (eq interprogram-paste-function #'identity)
                 (progn
                   (run-hooks 'window-setup-hook)
                   (eq interprogram-cut-function #'ignore)
                   (eq interprogram-paste-function #'identity)
                   (not (memq #'neomacs--window-setup window-setup-hook)))
                 (let ((window-system 'neo)
                       (select-enable-clipboard t)
                       (select-enable-primary t)
                       calls)
                   (cl-letf (((symbol-function 'neomacs-clipboard-set)
                              (lambda (value) (push (list 'clipboard value) calls)))
                             ((symbol-function 'neomacs-primary-selection-set)
                              (lambda (value) (push (list 'primary value) calls))))
                     (gui-select-text "copied")
                     (equal (nreverse calls)
                            '((primary "copied") (clipboard "copied"))))))))
        "#,
        )
        .map(|value| print_value_with_eval(&mut eval, &value))
        .unwrap_or_else(|err| format!("{err:?}"));

    assert_eq!(rendered, "t");
}

#[test]
fn neo_selection_backend_forwards_nil_to_disown_clipboard_and_primary() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");

    let rendered = eval
        .eval_str(
            r#"
        (progn
          (require 'cl-lib)
          (load "term/neo-win.el" nil t)
          (let ((window-system 'neo)
                (calls nil))
            (cl-letf (((symbol-function 'neomacs-clipboard-set)
                       (lambda (value) (push (list 'clipboard value) calls)))
                      ((symbol-function 'neomacs-primary-selection-set)
                       (lambda (value) (push (list 'primary value) calls))))
              (gui-backend-set-selection 'CLIPBOARD nil)
              (gui-backend-set-selection 'PRIMARY nil))
            (nreverse calls)))
        "#,
        )
        .map(|value| print_value_with_eval(&mut eval, &value))
        .unwrap_or_else(|err| format!("{err:?}"));

    assert_eq!(rendered, "((clipboard nil) (primary nil))");
}

#[test]
fn configure_gnu_startup_state_clears_window_system_for_tty_boots() {
    let mut eval = Context::new();
    let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
    let frame_id = eval.frame_manager_mut().create_frame("F1", 80, 25, scratch);
    let startup = StartupOptions {
        frontend: FrontendKind::Tty,
        forwarded_args: vec!["neomacs".to_string(), "-q".to_string()],
        terminal_device: Some("/dev/tty".to_string()),
        noninteractive: false,
        temacs_mode: None,
        dump_file_override: None,
        no_site_lisp: false,
        no_loadup: false,
        no_build_details: false,
    };
    configure_gnu_startup_state(&mut eval, frame_id, &startup);

    assert_eq!(
        eval.obarray().symbol_value("window-system"),
        Some(&Value::NIL)
    );
    assert_eq!(
        eval.obarray().symbol_value("initial-window-system"),
        Some(&Value::NIL)
    );
    assert_eq!(
        eval.obarray().symbol_value("command-line-args"),
        Some(&Value::list(vec![
            Value::string("neomacs"),
            Value::string("-q")
        ]))
    );
    assert_eq!(
        eval.obarray().symbol_value("command-line-args-left"),
        Some(&Value::list(vec![Value::string("-q")]))
    );
    let frame = eval
        .frame_manager()
        .get(frame_id)
        .expect("selected TTY frame should exist");
    // NEITHER display-derived parameter may be invented in Rust.  GNU's
    // `make_initial_frame' (src/frame.c:1423) sets neither, and both are
    // computed by `frame-set-background-mode' (lisp/frame.el:1526), which this
    // bare `Context::new()' -- GNU before loadup -- has not loaded.  Seeding
    // them here is DIVERGENCES.md 157's bug, and this row is the guard.
    assert_eq!(
        frame.parameter("display-type"),
        None,
        "display-type is DERIVED by frame-set-background-mode (lisp/frame.el:1526); \
         Rust must not seed it"
    );
    assert_eq!(
        frame.parameter("background-mode"),
        None,
        "background-mode is DERIVED by frame-set-background-mode (lisp/frame.el:1526); \
         Rust must not seed it"
    );
    // Nor may the TERMINAL parameter be invented, and that is the half
    // DIVERGENCES.md 157 handed on.  `frame-terminal-default-bg-mode'
    // (lisp/frame.el:1588-1599) is the FIRST clause of
    // `frame--current-background-mode' (lisp/frame.el:1505-1524), so a non-nil
    // value there WINS over the colour and the tty type -- permanently, for the
    // life of the terminal.  GNU's only writer is `xterm--set-background-mode'
    // (lisp/term/xterm.el:1309-1316), from a real OSC-11 reply; with no reply
    // the slot stays nil and the mode is derived from the background colour.
    assert_eq!(
        eval.eval_str("(terminal-parameter nil 'background-mode)")
            .expect("terminal-parameter probe"),
        Value::NIL,
        "background-mode on the TERMINAL is GNU's OSC-11 reply slot \
         (lisp/term/xterm.el:1309-1316); Rust must not seed it from a guess"
    );
    let frame = eval
        .frame_manager()
        .get(frame_id)
        .expect("selected TTY frame should exist");
    assert_eq!(
        frame.parameter("tty"),
        Some(Value::string(default_controlling_tty_name()))
    );
    assert_eq!(
        frame.parameter("tty-type"),
        std::env::var("TERM")
            .ok()
            .filter(|value| !value.is_empty())
            .map(Value::string)
    );
}

#[test]
fn live_tty_defface_keeps_dark_color_parent_attributes_through_inverse_video() {
    let mut eval = create_bootstrap_evaluator_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("screen-256color".to_string()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let display = bootstrap_tty_display_config(Interactivity::Interactive);
    let _bootstrap = bootstrap_buffers(&mut eval, 160, 50, display);
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected TTY frame")
        .id;
    let startup = StartupOptions {
        frontend: FrontendKind::Tty,
        forwarded_args: vec!["neomacs".to_string(), "-Q".to_string()],
        terminal_device: None,
        noninteractive: false,
        temacs_mode: None,
        dump_file_override: None,
        no_site_lisp: true,
        no_loadup: false,
        no_build_details: false,
    };
    configure_gnu_startup_state(&mut eval, frame_id, &startup);

    eval.eval_str(
        r#"(progn
             (defface neomacs-test-branch-local
               '((((class color) (background light)) :foreground "SkyBlue4")
                 (((class color) (background dark)) :foreground "LightSkyBlue1"))
               "Parent face.")
             (defface neomacs-test-branch-current
               '((((supports (:box t)))
                  :inherit neomacs-test-branch-local :box t)
                 (t :inherit neomacs-test-branch-local :inverse-video t))
               "Current face."))"#,
    )
    .expect("define Magit-shaped TTY faces");
    eval.sync_runtime_faces_for_frame(frame_id);

    let resolver = neomacs_layout_engine::neovm_bridge::FaceResolver::new(
        eval.face_table(),
        0x00e5e5e5,
        0x00333333,
        1.0,
        None,
    );
    let current = resolver.resolve_named_face("neomacs-test-branch-current");
    assert_eq!(
        current.fg, 0x00333333,
        "GNU realizes the inherited terminal-default background before inverse-video swaps it into the foreground slot"
    );
    assert!(
        !current.use_default_foreground,
        "a terminal-default background moved into the foreground slot is the concrete frame background, not terminal-default foreground"
    );
    assert_eq!(
        current.bg, 0x00b0e2ff,
        "the inverse face background must retain its inherited LightSkyBlue1 foreground"
    );
    assert!(!current.use_default_background);
    assert!(
        !current.terminal_inverse_video,
        "one concrete inherited color must not collapse into default/default inverse video"
    );

    let composed = resolver
        .resolve_face_value_over(
            resolver.default_face(),
            &Value::symbol("neomacs-test-branch-current"),
        )
        .expect("compose current-branch text-property face over the buffer base face");
    assert_eq!(
        composed.fg, 0x00333333,
        "the text-property composition path must realize inverse-video with the inherited frame background"
    );
    assert!(!composed.use_default_foreground);
    assert_eq!(
        composed.bg, 0x00b0e2ff,
        "the text-property composition path must retain the inherited LightSkyBlue1 foreground as its background"
    );
    assert!(!composed.use_default_background);
    assert!(!composed.terminal_inverse_video);
}

#[test]
fn configure_gnu_startup_state_marks_batch_mode_noninteractive() {
    let mut eval = Context::new();
    let startup = StartupOptions {
        frontend: FrontendKind::Tty,
        forwarded_args: vec![
            "neomacs".to_string(),
            "-Q".to_string(),
            "--eval".to_string(),
            "(princ 1)".to_string(),
        ],
        terminal_device: None,
        noninteractive: true,
        temacs_mode: None,
        dump_file_override: None,
        no_site_lisp: false,
        no_loadup: false,
        no_build_details: false,
    };
    configure_gnu_startup_state(&mut eval, FrameId(9), &startup);

    assert_eq!(
        eval.obarray().symbol_value("noninteractive"),
        Some(&Value::T)
    );
    assert_eq!(
        eval.obarray().symbol_value("gc-cons-percentage"),
        Some(&Value::make_float(1.0))
    );
    // A noninteractive startup never activates the startup GC ceiling: the
    // batch script runs inside `normal-top-level` and the settling timer that
    // releases the ceiling cannot fire, so GNU semantics (no ceiling) apply.
    assert_eq!(
        eval.obarray()
            .symbol_value("neomacs--startup-gc-ceiling-active"),
        Some(&Value::NIL)
    );
    assert_eq!(
        eval.obarray().symbol_value("command-line-args"),
        Some(&Value::list(vec![
            Value::string("neomacs"),
            Value::string("-Q"),
            Value::string("--eval"),
            Value::string("(princ 1)"),
        ]))
    );
}

#[test]
fn configure_gnu_startup_state_seeds_command_line_args_left_for_gnu_startup() {
    let mut eval = Context::new();
    let startup = gui_startup_with_args(&["-Q", "-l", "/tmp/demo.el"]);
    configure_gnu_startup_state(&mut eval, FrameId(42), &startup);

    assert_eq!(
        eval.obarray().symbol_value("command-line-args-left"),
        Some(&Value::list(vec![
            Value::string("-Q"),
            Value::string("-l"),
            Value::string("/tmp/demo.el")
        ]))
    );
}

#[test]
fn bootstrap_buffers_seed_frame_with_renderer_metrics() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    assert_selected_frame_matches_materialized_default_metrics(&eval);
    let expected_pixel_size = eval
        .eval_str("(font-get (frame-parameter nil 'font-parameter) :size)")
        .expect("bootstrap font pixel size")
        .as_int()
        .expect("integer bootstrap font pixel size") as f32;
    let frame = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap");
    assert_eq!(frame.font_pixel_size, expected_pixel_size);
    let font_param = frame
        .parameter("font")
        .expect("bootstrap GUI frame should seed a font frame parameter");
    assert!(font_param.is_string());
    let minibuffer_height = frame
        .minibuffer_leaf
        .as_ref()
        .expect("minibuffer leaf")
        .bounds()
        .height;
    assert_eq!(minibuffer_height, frame.char_height);
}

#[test]
fn startup_dimensions_gui_matches_gnu_default_text_grid() {
    let metrics = bootstrap_frame_metrics();
    let (width, height) = startup_dimensions(FrontendKind::Gui, metrics, false);
    // GNU's default GUI frame is 80 text columns wide and 35 counted text rows
    // tall (its frame-height is deterministically one less than the nominal 36-row
    // geometry). The requested window reserves the scroll bar + both 8px fringes on
    // the sides and the menu + tool bars on top, so the text grid lands on 80x35
    // instead of the chrome eating into it (78x33).
    let side_chrome = metrics.char_width + 2.0 * 8.0;
    let top_chrome = metrics.char_height
        + neovm_core::window::default_gui_tool_bar_line_height(metrics.font_pixel_size) as f32;
    assert_eq!(
        width,
        (80.0 * metrics.char_width + side_chrome).round() as u32
    );
    assert_eq!(
        height,
        (35.0 * metrics.char_height + top_chrome).round() as u32
    );
}

#[test]
fn startup_dimensions_noninteractive_tty_matches_gnu_initial_frame() {
    let metrics = bootstrap_frame_metrics_for_frontend(FrontendKind::Tty);
    assert_eq!(
        startup_dimensions(FrontendKind::Tty, metrics, true),
        (80, 25)
    );
}

#[test]
fn bootstrap_frame_metrics_uses_default_face_height_pixels() {
    let metrics = bootstrap_frame_metrics();
    assert_eq!(metrics.font_pixel_size, face_height_to_pixels(100));
}

#[test]
fn wayland_font_policy_uses_logical_dpi_not_xft_dpi() {
    assert_eq!(
        FontSizing::logical().face_height_to_layout_pixels(100),
        13.0
    );
}

#[test]
fn wayland_bootstrap_frame_metrics_use_logical_default_font_size() {
    let metrics = bootstrap_frame_metrics_for_font_sizing(FontSizing::logical());
    assert_eq!(metrics.font_pixel_size, 13.0);
}

#[test]
fn gui_font_policy_uses_the_observed_xwayland_backend() {
    let observation = neomacs_display_protocol::DisplayObservation::X11(
        neomacs_display_protocol::X11DisplayObservation::new(
            neomacs_display_protocol::XServerKind::Xwayland,
            None,
            Some(
                neomacs_display_protocol::DisplayHeightGeometry::new(1080, 800)
                    .expect("valid test geometry"),
            ),
            neomacs_display_protocol::DeviceScale::ONE,
        ),
    );

    assert_eq!(
        gui_frame_scale_from_observation(observation)
            .font_sizing()
            .layout_dpi(),
        96.0
    );
}

#[test]
fn bootstrap_display_keeps_the_resolved_gui_scale_profile_atomic() {
    let device_scale = neomacs_display_protocol::DeviceScale::new(2.0).expect("valid scale");
    let observation = neomacs_display_protocol::DisplayObservation::Wayland { device_scale };
    let resolved = gui_frame_scale_from_observation(observation);
    let display = bootstrap_gui_display_config(Interactivity::Interactive, resolved);

    assert_eq!(display.frame_scale_profile(), Some(resolved));
    assert_eq!(display.font_sizing(), resolved.font_sizing());
    assert_eq!(
        display
            .frame_scale_profile()
            .expect("GUI scale profile")
            .device_scale(),
        device_scale
    );
}

#[test]
fn x11_font_policy_keeps_xft_dpi_conversion() {
    assert_eq!(
        FontSizing::xft().face_height_to_layout_pixels(100),
        face_height_to_pixels(100)
    );
}

#[test]
fn relative_face_height_uses_policy_default_font_size() {
    let face = neovm_core::face::Face {
        height: Some(FaceHeight::Relative(2.0)),
        ..Default::default()
    };
    assert_eq!(FontSizing::logical().font_size_px_for_face(&face), 26.0);
}

#[test]
fn bootstrap_default_font_name_uses_pixel_size_field() {
    let mut eval = Context::new();
    let font_pixel_size = face_height_to_pixels(100);
    let font_name = bootstrap_default_font_name(font_pixel_size);
    let rendered = print_value_with_eval(&mut eval, &font_name);
    assert!(rendered.contains(&format!("-*-{}-", font_pixel_size.round() as i64)));
    assert!(rendered.contains("-regular-"));
}

#[test]
fn bootstrap_buffers_reuses_selected_startup_frame_when_one_already_exists() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let old_buffer = eval.buffer_manager_mut().create_buffer("*old*");
    let old_frame = eval
        .frame_manager_mut()
        .create_frame("old", 320, 200, old_buffer);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(old_frame)
            .expect("old frame should exist");
        frame.set_title_value(Value::string("old"));
    }

    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    assert_selected_frame_matches_materialized_default_metrics(&eval);

    assert_eq!(eval.frame_manager().frame_list().len(), 1);
    let selected = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap");
    assert_eq!(selected.id, old_frame);
    assert_eq!(selected.width, 960);
    assert_eq!(selected.height, 640);
    assert_eq!(
        selected.effective_window_system(),
        Some(Value::symbol("neo"))
    );
    assert_eq!(selected.name_runtime_string_owned(), "F1");
    assert_eq!(selected.title_runtime_string_owned(), None);
    let minibuffer_height = selected
        .minibuffer_leaf
        .as_ref()
        .expect("minibuffer leaf")
        .bounds()
        .height;
    assert_eq!(minibuffer_height, selected.char_height);
}

#[test]
fn bootstrap_buffers_reuses_cached_surrogate_frame_when_it_is_the_only_selected_frame() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let old_buffer = eval.buffer_manager_mut().create_buffer("*old*");
    let surrogate = eval
        .frame_manager_mut()
        .create_frame("F1", 80, 25, old_buffer);

    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    assert_selected_frame_matches_materialized_default_metrics(&eval);

    assert_eq!(eval.frame_manager().frame_list().len(), 1);
    let selected = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap");
    assert_eq!(selected.id, surrogate);
    assert_eq!(selected.width, 960);
    assert_eq!(selected.height, 640);
    assert_eq!(
        selected.effective_window_system(),
        Some(Value::symbol("neo"))
    );
}

/// GNU `make_initial_frame` assigns the user-visible name `F1` from its
/// dedicated `tty_frame_count`; object allocation history is irrelevant.  A
/// pdump surrogate can therefore have a later internal frame id and still
/// become the initial live TTY frame named `F1`.
#[test]
fn bootstrap_buffers_names_reused_initial_tty_frame_f1_after_surrogate_allocation() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("cached bootstrap evaluator");
    let old_buffer = eval.buffer_manager_mut().create_buffer("*old*");
    let dump_only = eval
        .frame_manager_mut()
        .create_frame("dump-only", 80, 25, old_buffer);
    let surrogate = eval
        .frame_manager_mut()
        .create_frame("surrogate", 80, 25, old_buffer);
    assert!(eval.frame_manager_mut().select_frame(surrogate));
    assert!(eval.frame_manager_mut().delete_frame(dump_only));

    let _bootstrap = bootstrap_buffers(
        &mut eval,
        160,
        50,
        bootstrap_tty_display_config(Interactivity::Interactive),
    );

    let selected = eval
        .frame_manager()
        .selected_frame()
        .expect("selected initial TTY frame after bootstrap");
    assert_eq!(selected.id, surrogate);
    assert_eq!(selected.name_runtime_string_owned(), "F1");
}

#[test]
fn bootstrap_buffers_reuses_existing_named_buffers_in_cached_bootstrap() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let original_scratch = eval
        .buffer_manager()
        .find_buffer_by_name("*scratch*")
        .expect("bootstrap scratch");

    let bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());

    assert_eq!(bootstrap.scratch_id, original_scratch);
    let scratch_count = eval
        .buffer_manager()
        .buffer_list()
        .into_iter()
        .filter(|id| {
            eval.buffer_manager()
                .get(*id)
                .is_some_and(|buffer| buffer.name_runtime_string_owned() == "*scratch*")
        })
        .count();
    assert_eq!(scratch_count, 1);
}

#[test]
fn bootstrap_buffers_clears_predump_messages_buffer_contents() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let messages_id = eval
        .buffer_manager()
        .find_buffer_by_name("*Messages*")
        .expect("bootstrap messages");
    {
        let messages = eval
            .buffer_manager_mut()
            .get_mut(messages_id)
            .expect("live messages buffer");
        messages.widen();
        messages.goto_emacs_byte_pos(messages.point_max_emacs_byte_pos());
        messages.insert("stale predump log\n");
    }

    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());

    let messages = eval
        .buffer_manager()
        .get(messages_id)
        .expect("messages buffer after bootstrap");
    assert_eq!(messages.buffer_string(), "");
    assert_eq!(messages.point_emacs_byte_pos().get(), 0);
}

#[test]
fn gnu_startup_keeps_scratch_selected_under_q_startup() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    run_gnu_startup(&mut eval);

    let current = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer after startup");
    assert_eq!(current.name_runtime_string_owned(), "*scratch*");
}

#[test]
fn gnu_startup_reused_gui_frame_installs_common_window_key_translations() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"
        (list
         (lookup-key local-function-key-map [M-backspace])
         (lookup-key local-function-key-map [M-delete])
         (terminal-parameter nil 'x-setup-function-keys)
         (key-binding [M-backspace])
         (key-binding [M-delete])
         (key-binding [?\M-\d]))
        "#,
        )
        .expect("key translation probe should evaluate");
    let rendered = print_value_with_eval(&mut eval, &result);
    assert_eq!(
        rendered,
        "([134217855] [134217855] t nil nil backward-kill-word)"
    );
}

#[test]
fn gnu_startup_keeps_bootstrap_gui_frame_instead_of_creating_replacement_frame() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let frame_id = bootstrap_runtime_gui_startup(&mut eval);

    eval.eval_str(
        r#"
        (progn
          (setq neomacs--probe-handle-args-called nil)
          (setq neomacs--probe-window-system-init-called nil)
          (setq neomacs--probe-frame-initialize-called nil)
          (setq neomacs--probe-normal-top-level-called nil)
          (setq neomacs--probe-command-line-called nil)
          (setq neomacs--probe-top-level-before top-level)
          (setq neomacs--orig-handle-args-function
                (symbol-function 'handle-args-function))
          (setq neomacs--orig-window-system-initialization
                (symbol-function 'window-system-initialization))
          (setq neomacs--orig-frame-initialize
                (symbol-function 'frame-initialize))
          (setq neomacs--orig-normal-top-level
                (symbol-function 'normal-top-level))
          (setq neomacs--orig-command-line
                (symbol-function 'command-line))
          (fset 'handle-args-function
                (lambda (args)
                  (setq neomacs--probe-handle-args-called t)
                  (funcall neomacs--orig-handle-args-function args)))
          (fset 'window-system-initialization
                (lambda (&optional display)
                  (setq neomacs--probe-window-system-init-called t)
                  (funcall neomacs--orig-window-system-initialization display)))
          (fset 'frame-initialize
                (lambda ()
                  (setq neomacs--probe-frame-initialize-called t)
                  (funcall neomacs--orig-frame-initialize)))
          (fset 'normal-top-level
                (lambda ()
                  (setq neomacs--probe-normal-top-level-called t)
                  (funcall neomacs--orig-normal-top-level)))
          (fset 'command-line
                (lambda (&rest args)
                  (setq neomacs--probe-command-line-called t)
                  (apply neomacs--orig-command-line args))))
        "#,
    )
    .expect("startup hook probe should install");

    run_gnu_startup(&mut eval);

    let startup_probe = eval
        .eval_str(
            r#"
         (list
         (current-message)
         noninteractive
         window-system
         initial-window-system
         (featurep 'neo-win)
         (featurep 'term/neo-win)
         (featurep 'x-win)
         (daemonp)
         command-line-processed
         neomacs--probe-top-level-before
         neomacs--probe-normal-top-level-called
         neomacs--probe-command-line-called
         neomacs--probe-handle-args-called
         neomacs--probe-window-system-init-called
         neomacs--probe-frame-initialize-called
         neomacs-initialized
         (get 'neo 'window-system-initialized)
         frame-initial-frame
         (and (boundp 'neomacs--startup-last-phase)
              neomacs--startup-last-phase)
         (and (boundp 'neomacs--startup-last-call)
              neomacs--startup-last-call)
         terminal-frame
         (mapcar
          (lambda (frame)
            (list frame
                  (frame-parameter frame 'window-system)
                  (frame-parameter frame 'display-type)
                  (frame-parameter frame 'background-mode)
                  (frame-visible-p frame)
                  (eq frame terminal-frame)
                  (eq frame frame-initial-frame)
                  (eq frame (selected-frame))
                  (eq frame (window-frame (minibuffer-window frame)))))
          (frame-list)))
        "#,
        )
        .expect("startup probe should evaluate");
    let shutdown_request = eval.shutdown_request();
    let frame_ids: Vec<_> = eval.frame_manager().frame_list().into_iter().collect();
    let selected_frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after startup")
        .id;
    assert_eq!(
        frame_ids,
        vec![frame_id],
        "startup probe={} shutdown_request={shutdown_request:?}",
        print_value_with_eval(&mut eval, &startup_probe),
    );
    assert_eq!(
        selected_frame_id,
        frame_id,
        "startup probe={} shutdown_request={shutdown_request:?}",
        print_value_with_eval(&mut eval, &startup_probe),
    );
}

#[test]
fn bootstrap_gui_state_allows_gnu_frame_initialize_to_delete_terminal_frame() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let frame_id = bootstrap_runtime_gui_startup(&mut eval);

    eval.eval_str("(frame-initialize)")
        .expect("frame-initialize should succeed on bootstrap gui state");

    let frame_ids: Vec<_> = eval.frame_manager().frame_list().into_iter().collect();
    assert_eq!(frame_ids, vec![frame_id]);
    assert_eq!(
        eval.obarray().symbol_value("terminal-frame"),
        Some(&Value::NIL)
    );
}

#[test]
fn gnu_startup_keeps_scratch_text_accessible_under_q_startup() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"(with-current-buffer (current-buffer)
                 (list (buffer-name)
                       major-mode
                       (> (point-max) 1)
                       (> (buffer-size) 0)
                       (> (length
                           (buffer-substring-no-properties
                            (point-min)
                            (min (point-max) (+ (point-min) 16))))
                          0)))"#,
        )
        .expect("scratch accessibility probe should evaluate");
    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(\"*scratch*\" lisp-interaction-mode t t t)"
    );
}

#[test]
fn gnu_startup_preserves_default_fontset_alias() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str("(query-fontset \"fontset-default\")")
        .expect("fontset query should evaluate");
    assert_eq!(
        result,
        Value::string("-*-*-*-*-*-*-*-*-*-*-*-*-fontset-default")
    );
}

#[test]
fn gnu_startup_posts_echo_area_message() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let frame_id = bootstrap_runtime_gui_frame(&mut eval);
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"(list (current-message)
                   (substring-no-properties (startup-echo-area-message)))"#,
        )
        .expect("startup echo probe should evaluate");
    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(#(\"For information about GNU Emacs and the GNU system, type C-h C-a.\" 57 64 (font-lock-face help-key-binding face help-key-binding)) \"For information about GNU Emacs and the GNU system, type C-h C-a.\")"
    );
}

#[test]
fn gnu_startup_keeps_single_row_minibuffer() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str("(window-total-height (minibuffer-window))")
        .expect("minibuffer height probe should evaluate");
    assert_eq!(result, Value::fixnum(1));
}

#[test]
fn bootstrap_gui_frame_seeds_live_menu_and_tool_bar_rows() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap");

    assert_eq!(frame.frame_parameter_int("menu-bar-lines"), Some(1));
    assert_eq!(frame.frame_parameter_int("tool-bar-lines"), Some(1));
    assert_eq!(frame.menu_bar_height, frame.char_height.round() as u32);
    assert_eq!(
        frame.tool_bar_height,
        default_gui_tool_bar_line_height(frame.font_pixel_size)
    );
}

#[test]
fn sync_selected_gui_chrome_state_tracks_gnu_window_system_defaults() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    sync_selected_gui_chrome_state(&mut eval);

    let frame = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after chrome sync");
    assert_eq!(frame.frame_parameter_int("menu-bar-lines"), Some(1));
    assert_eq!(frame.frame_parameter_int("tool-bar-lines"), Some(1));
}

#[test]
fn sync_selected_gui_chrome_state_defers_lisp_setup_during_throw_on_input() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());
    eval.eval_str(
        r#"(progn
             (setq neo-tool-bar-setup-count 0)
             (setq-default tool-bar-map '(keymap))
             (fset 'tool-bar-setup
                   (lambda ()
                     (setq neo-tool-bar-setup-count
                           (1+ neo-tool-bar-setup-count))))
             (setq throw-on-input 'neo-while-no-input-tag))"#,
    )
    .expect("install deferred tool-bar setup probe");

    sync_selected_gui_chrome_state(&mut eval);
    assert_eq!(
        eval.obarray()
            .symbol_value("neo-tool-bar-setup-count")
            .copied()
            .expect("setup count while throw-on-input is active"),
        Value::fixnum(0)
    );

    eval.set_variable("throw-on-input", Value::NIL);
    sync_selected_gui_chrome_state(&mut eval);
    assert_eq!(
        eval.obarray()
            .symbol_value("neo-tool-bar-setup-count")
            .copied()
            .expect("setup count after throw-on-input"),
        Value::fixnum(1)
    );
}

#[test]
fn sync_selected_gui_chrome_state_uses_compact_bar_as_separate_gui_chrome() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());
    eval.set_variable("compact-bar-mode", Value::T);

    sync_selected_gui_chrome_state(&mut eval);

    let frame = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after compact chrome sync");
    assert_eq!(frame.frame_parameter_int("menu-bar-lines"), Some(0));
    assert_eq!(frame.frame_parameter_int("tool-bar-lines"), Some(0));
    assert_eq!(frame.frame_parameter_int("compact-bar-lines"), Some(1));
    assert_eq!(frame.menu_bar_height, 0);
    assert_eq!(frame.tool_bar_height, 0);
    assert_eq!(
        frame.compact_bar_height,
        default_gui_tool_bar_line_height(frame.font_pixel_size)
    );
}

#[test]
fn gnu_startup_runtime_load_path_finds_mail_rfc6068() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str("(locate-library \"rfc6068\")")
        .expect("locate-library startup probe should evaluate");
    let path = result
        .as_runtime_string_owned()
        .expect("locate-library should return a resolved path string after startup");
    assert!(
        path.ends_with("/mail/rfc6068.elc"),
        "expected GNU mail runtime path, got {path}"
    );
}

#[test]
fn gnu_startup_where_is_internal_finds_about_emacs_on_help_prefix() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"(list
               (lookup-key help-map [1])
               (lookup-key (symbol-function 'help-command) [1])
               (lookup-key (current-global-map) [8])
               (lookup-key (current-global-map) [8 1]))"#,
        )
        .expect("startup help-prefix probe should evaluate");
    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(about-emacs about-emacs help-command about-emacs)"
    );
}

#[test]
#[ignore = "startup echo helper blocks in this harness; message redisplay is covered in neovm-core"]
fn gnu_startup_requests_redisplay_for_echo_area_message() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    let redisplay_rows = Arc::new(Mutex::new(Vec::<String>::new()));
    let redisplay_rows_capture = Arc::clone(&redisplay_rows);
    eval.redisplay_fn = Some(Box::new(move |eval: &mut Context| {
        redisplay_rows_capture
            .lock()
            .expect("redisplay row buffer")
            .push(eval.current_message_text().unwrap_or_default().to_string());
    }));

    let result = eval
        .eval_str("(display-startup-echo-area-message)")
        .expect("display-startup-echo-area-message should evaluate");
    assert_eq!(
        result,
        Value::string("For information about GNU Emacs and the GNU system, type C-h C-a.")
    );

    let rendered_rows = redisplay_rows.lock().expect("captured redisplay rows");

    assert!(
        rendered_rows
            .iter()
            .any(|row| row.contains("For information about GNU Emacs and the GNU system")),
        "expected startup echo message during redisplay, got: {rendered_rows:?}"
    );
}

#[test]
fn gnu_startup_restores_meta_and_ctl_x_bindings() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"(list
                 (key-binding (kbd "M-x"))
                 (lookup-key (current-global-map) (kbd "M-x"))
                 (key-binding (kbd "C-x 2"))
                 (lookup-key (current-global-map) (kbd "C-x 2"))
                 (key-binding (kbd "C-x 3"))
                 (lookup-key (current-global-map) (kbd "C-x 3")))"#,
        )
        .expect("startup keybinding probe should evaluate");
    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(execute-extended-command execute-extended-command split-window-below split-window-below split-window-right split-window-right)"
    );
}

#[test]
fn gnu_startup_formats_mode_line_for_target_window_buffer() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"(let* ((w (selected-window))
                      (buf (window-buffer w))
                      (mini (minibuffer-window)))
                 (with-current-buffer (window-buffer mini)
                   (format-mode-line "%b" nil w buf)))"#,
        )
        .expect("startup mode-line probe should evaluate");
    assert_eq!(result, Value::string("*scratch*"));
}

#[test]
fn gnu_startup_split_window_right_succeeds_on_opening_frame() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let (expected_width, expected_height) = {
        let frame = eval
            .frame_manager()
            .selected_frame()
            .expect("selected frame after startup");
        let selected = frame
            .selected_window()
            .expect("selected window after startup");
        let bounds = selected.bounds();
        (
            (bounds.width / frame.char_width) as i64,
            (bounds.height / frame.char_height) as i64,
        )
    };

    let result = eval
        .eval_str(
            r#"(list
                 (window-total-width)
                 (window-total-height)
                 (window-min-size nil t)
                 (window-min-size nil nil)
                 (window-size-fixed-p (selected-window))
                 (window-size-fixed-p (selected-window) t)
                 (condition-case err
                     (progn (split-window-right) 'ok)
                   (error (list 'error (error-message-string err)))))"#,
        )
        .expect("startup split-window probe should evaluate");
    let items = list_to_vec(&result).expect("split-window result list");
    assert_eq!(items[0], Value::fixnum(expected_width));
    assert_eq!(items[1], Value::fixnum(expected_height));
    assert_eq!(items[2], Value::fixnum(10));
    assert_eq!(items[3], Value::fixnum(4));
    assert!(items[4].is_nil());
    assert!(items[5].is_nil());
    assert_eq!(items[6], Value::symbol("ok"));
}

#[test]
fn gnu_startup_split_window_below_succeeds_on_opening_frame() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let (expected_width, expected_height) = {
        let frame = eval
            .frame_manager()
            .selected_frame()
            .expect("selected frame after startup");
        let selected = frame
            .selected_window()
            .expect("selected window after startup");
        let bounds = selected.bounds();
        (
            (bounds.width / frame.char_width) as i64,
            (bounds.height / frame.char_height) as i64,
        )
    };

    let result = eval
        .eval_str(
            r#"(list
                 (window-total-width)
                 (window-total-height)
                 (window-min-size nil t)
                 (window-min-size nil nil)
                 (window-size-fixed-p (selected-window))
                 (window-size-fixed-p (selected-window) t)
                 (condition-case err
                     (progn (split-window-below) 'ok)
                   (error (list 'error (error-message-string err)))))"#,
        )
        .expect("startup split-window probe should evaluate");
    let items = list_to_vec(&result).expect("split-window result list");
    assert_eq!(items[0], Value::fixnum(expected_width));
    assert_eq!(items[1], Value::fixnum(expected_height));
    assert_eq!(items[2], Value::fixnum(10));
    assert_eq!(items[3], Value::fixnum(4));
    assert!(items[4].is_nil());
    assert!(items[5].is_nil());
    assert_eq!(items[6], Value::symbol("ok"));
}

#[test]
fn gnu_startup_window_pixel_queries_use_live_frame_pixels() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"(list
                 (window-pixel-width)
                 (window-pixel-height)
                 (window-body-width nil t)
                 (window-body-height nil t)
                 (window-text-width nil t)
                 (window-text-height nil t)
                 (window-fringes)
                 (window-scroll-bar-width)
                 (window-edges nil nil nil t)
                 (window-edges nil t nil t))"#,
        )
        .expect("startup pixel probe should evaluate");
    let items = list_to_vec(&result).expect("pixel query result list");
    let pixel_width = items[0].as_int().expect("window-pixel-width");
    let pixel_height = items[1].as_int().expect("window-pixel-height");
    let body_width = items[2].as_int().expect("window-body-width");
    let body_height = items[3].as_int().expect("window-body-height");
    let text_width = items[4].as_int().expect("window-text-width");
    let text_height = items[5].as_int().expect("window-text-height");
    let fringes = list_to_vec(&items[6]).expect("window fringes");
    let scroll_bar_width = items[7].as_int().expect("window-scroll-bar-width");
    let outer_edges = list_to_vec(&items[8]).expect("outer window edges");
    let inner_edges = list_to_vec(&items[9]).expect("inner window edges");
    let left_fringe = fringes[0].as_int().expect("left fringe");
    let right_fringe = fringes[1].as_int().expect("right fringe");
    let outer_top = outer_edges[1].as_int().expect("outer top edge");

    assert_eq!(pixel_width, 960);
    assert!(pixel_height > 0);
    assert_eq!(
        body_width,
        pixel_width - left_fringe - right_fringe - scroll_bar_width
    );
    assert_eq!(text_width, body_width);
    assert_eq!(body_height, text_height);
    assert!(pixel_height >= body_height);
    assert_eq!(
        outer_edges,
        vec![
            Value::fixnum(0),
            Value::fixnum(outer_top),
            Value::fixnum(pixel_width),
            Value::fixnum(outer_top + pixel_height)
        ]
    );
    assert_eq!(
        inner_edges,
        vec![
            Value::fixnum(left_fringe),
            Value::fixnum(outer_top),
            Value::fixnum(pixel_width - right_fringe - scroll_bar_width),
            Value::fixnum(outer_top + body_height)
        ]
    );
}

#[test]
fn gnu_startup_processes_load_option_from_forwarded_args() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let frame_id = bootstrap_runtime_gui_frame(&mut eval);
    let repo_root = Path::new(env!("CARGO_WORKSPACE_DIR"));
    let face_test = repo_root.join("test/neomacs/neomacs-face-test.el");
    let startup = gui_startup_with_args(&[
        "-Q",
        "-l",
        face_test
            .to_str()
            .expect("face test path must be valid utf-8"),
    ]);
    configure_gnu_startup_state(&mut eval, frame_id, &startup);

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"(list
                 (fboundp 'neomacs-face-test-write-matrix-report)
                 (buffer-live-p (get-buffer "*Neomacs Face Test*"))
                 (buffer-name (window-buffer (selected-window))))"#,
        )
        .expect("startup load-option probe should evaluate");
    let items = list_to_vec(&result).expect("load-option result list");
    assert_eq!(items[0], Value::T);
    assert_eq!(items[1], Value::T);
    assert_eq!(
        print_value_with_eval(&mut eval, &items[2]),
        "\"*Neomacs Face Test*\""
    );
}

#[test]
fn recursive_edit_processes_load_option_from_forwarded_args_before_first_input() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let frame_id = bootstrap_runtime_gui_frame(&mut eval);
    let repo_root = Path::new(env!("CARGO_WORKSPACE_DIR"));
    let face_test = repo_root.join("test/neomacs/neomacs-face-test.el");
    let startup = gui_startup_with_args(&[
        "-Q",
        "-l",
        face_test
            .to_str()
            .expect("face test path must be valid utf-8"),
    ]);
    configure_gnu_startup_state(&mut eval, frame_id, &startup);

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(neovm_core::keyboard::InputEvent::WindowClose {
        emacs_frame_id: u64::MAX,
    })
    .expect("queue close request");
    drop(tx);
    eval.init_input_system(rx);

    let result = eval.recursive_edit();
    result.expect("close request should let the outer recursive edit exit cleanly");

    let result = eval
        .eval_str(
            r#"(list
                 (fboundp 'neomacs-face-test-write-matrix-report)
                 (buffer-live-p (get-buffer "*Neomacs Face Test*"))
                 (buffer-name (window-buffer (selected-window))))"#,
        )
        .expect("recursive-edit load-option probe should evaluate");
    let items = list_to_vec(&result).expect("recursive-edit result list");
    assert_eq!(items[0], Value::T);
    assert_eq!(items[1], Value::T);
    assert_eq!(
        print_value_with_eval(&mut eval, &items[2]),
        "\"*Neomacs Face Test*\""
    );
}

#[test]
fn bootstrap_batch_eval_exits_outer_command_loop_like_gnu() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(
        &mut eval,
        80,
        24,
        bootstrap_tty_display_config(Interactivity::Batch),
    );
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    let startup = tty_batch_startup_with_args(&["-Q", "--eval", "(setq neomacs--batch-probe 42)"]);
    configure_gnu_startup_state(&mut eval, frame_id, &startup);

    let (_tx, rx) = crossbeam_channel::unbounded();
    eval.init_input_system(rx);

    let result = eval.recursive_edit();
    result.expect("batch recursive edit should exit cleanly");

    assert_eq!(
        eval.shutdown_request(),
        Some(neovm_core::emacs_core::eval::ShutdownRequest {
            exit_code: 0,
            restart: false,
        })
    );
    assert_eq!(
        eval.obarray().symbol_value("neomacs--batch-probe"),
        Some(&Value::fixnum(42))
    );
    assert_eq!(
        eval.obarray().symbol_value("command-line-processed"),
        Some(&Value::T)
    );
}

#[test]
fn bootstrap_batch_kill_emacs_is_silent_shutdown() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(
        &mut eval,
        80,
        24,
        bootstrap_tty_display_config(Interactivity::Batch),
    );
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    let startup = tty_batch_startup_with_args(&["-Q", "--eval", "(kill-emacs)"]);
    configure_gnu_startup_state(&mut eval, frame_id, &startup);

    let (_tx, rx) = crossbeam_channel::unbounded();
    eval.init_input_system(rx);

    let result = eval.recursive_edit();
    result.expect("batch kill-emacs should exit cleanly");

    assert_eq!(
        eval.shutdown_request(),
        Some(neovm_core::emacs_core::eval::ShutdownRequest {
            exit_code: 0,
            restart: false,
        })
    );
    assert_ne!(eval.current_message_text().as_deref(), Some("kill-emacs"));
}

#[test]
fn bootstrap_batch_startup_error_exits_nonzero_like_gnu() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(
        &mut eval,
        80,
        24,
        bootstrap_tty_display_config(Interactivity::Batch),
    );
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    let startup = tty_batch_startup_with_args(&["-Q", "--eval", "(error \"boom\")"]);
    configure_gnu_startup_state(&mut eval, frame_id, &startup);

    let (_tx, rx) = crossbeam_channel::unbounded();
    eval.init_input_system(rx);

    let result = eval.recursive_edit();
    result.expect("batch startup error should leave through kill-emacs");

    assert_eq!(
        eval.shutdown_request(),
        Some(neovm_core::emacs_core::eval::ShutdownRequest {
            exit_code: -1,
            restart: false,
        })
    );
    assert_eq!(
        eval.current_message_text(),
        None,
        "GNU noninteractive message writes to stderr/*Messages*, not the echo area"
    );
    let messages = eval
        .buffer_manager()
        .find_buffer_by_name("*Messages*")
        .and_then(|id| eval.buffer_manager().get(id))
        .map(|buffer| buffer.buffer_string())
        .unwrap_or_default();
    assert!(
        messages.lines().any(|line| line == "boom"),
        "GNU prints `(error STRING)` as STRING before nonzero shutdown; messages={messages:?}"
    );
}

#[test]
fn gui_bootstrap_accepts_iso_8859_15_coding_primitives() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let probes = [
        ("known", "(coding-system-p 'iso-8859-15)", Some(Value::T)),
        (
            "type",
            "(coding-system-type 'iso-8859-15)",
            Some(Value::symbol("charset")),
        ),
        (
            "eol",
            "(coding-system-change-eol-conversion 'iso-8859-15 0)",
            Some(Value::symbol("iso-latin-9-unix")),
        ),
        (
            "terminal-set",
            "(progn (set-terminal-coding-system 'iso-8859-15 nil t) 'ok)",
            Some(Value::symbol("ok")),
        ),
        (
            "keyboard-set",
            "(progn (set-keyboard-coding-system 'iso-8859-15 nil) 'ok)",
            Some(Value::symbol("ok")),
        ),
        (
            "keyboard-var",
            "keyboard-coding-system",
            Some(Value::symbol("iso-latin-9-unix")),
        ),
        (
            "terminal-var",
            "(terminal-coding-system)",
            Some(Value::symbol("iso-8859-15")),
        ),
    ];

    for (label, source, expected) in probes {
        let result = eval.eval_str(source);
        let value = result.unwrap_or_else(|_| panic!("coding probe {label} should evaluate"));
        if let Some(expected_value) = expected {
            let actual_name = value
                .as_symbol_name()
                .map(|name| name.to_string())
                .unwrap_or_else(|| format!("{value:?}"));
            let expected_name = expected_value
                .as_symbol_name()
                .map(|name| name.to_string())
                .unwrap_or_else(|| format!("{expected_value:?}"));
            assert_eq!(
                value, expected_value,
                "coding probe {label} should match GNU bootstrap semantics (actual={actual_name}, expected={expected_name})"
            );
        }
    }
}

#[test]
fn gnu_startup_next_line_moves_point_on_live_gui_frame() {
    let mut eval = create_bootstrap_evaluator_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());

    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"(progn
             (switch-to-buffer "*scratch*")
             (erase-buffer)
             (insert "abc\ndef\nghi")
             (goto-char 1)
             (command-execute 'next-line)
             (point))"#,
        )
        .expect("startup next-line should evaluate");
    assert_eq!(result, Value::fixnum(5));
}

#[test]
fn frame_set_background_mode_uses_live_gui_window_system_after_startup_clears_initial_flag() {
    let mut eval = create_bootstrap_evaluator_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());
    eval.set_variable("initial-window-system", Value::NIL);

    let result = eval
        .eval_str(
            r#"(condition-case err
                  (progn
                    (frame-set-background-mode (selected-frame))
                    'ok)
                (error (list 'error (error-message-string err))))"#,
        )
        .expect("frame-set-background-mode probe should evaluate");
    assert_eq!(result, Value::symbol("ok"));
}

#[test]
fn modify_frame_parameters_updates_live_default_face_colors_for_gui_frames() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let result = eval
        .eval_str(
            r##"(progn
             (modify-frame-parameters
              (selected-frame)
              '((foreground-color . "white")
                (background-color . "#000000")))
             (list
              (frame-parameter nil 'background-mode)
              (frame-parameter nil 'foreground-color)
              (frame-parameter nil 'background-color)
              (face-foreground 'default nil t)
              (face-background 'default nil t)))"##,
        )
        .expect("modify-frame-parameters face probe should evaluate");
    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(dark \"white\" \"#000000\" \"white\" \"#000000\")"
    );
}

#[test]
fn modify_frame_parameters_background_color_only_completes_for_gui_frames() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let result = eval
        .eval_str(
            r##"(progn
             (modify-frame-parameters
              (selected-frame)
              '((background-color . "#000000")))
             (list
              'after-modify
              (frame-parameter nil 'background-mode)
              (frame-parameter nil 'background-color)))"##,
        )
        .expect("background-only modify-frame-parameters should evaluate");
    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(after-modify dark \"#000000\")"
    );
}

#[test]
fn frame_set_background_mode_keep_face_specs_completes_after_dark_background_change() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let frame_id = bootstrap_runtime_gui_startup(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("live frame");
        frame.set_parameter(Value::symbol("background-color"), Value::string("#000000"));
    }

    let result = eval
        .eval_str(
            r#"(progn
             (frame-set-background-mode (selected-frame) t)
             (list
              'after-frame-set-background-mode
              (frame-parameter nil 'background-mode)
              (frame-parameter nil 'display-type)))"#,
        )
        .expect("frame-set-background-mode keep-face-specs should evaluate");
    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(after-frame-set-background-mode dark color)"
    );
}

fn seed_selected_frame_background_color(eval: &mut Context, color: &str) {
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame")
        .id;
    let frame = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .expect("live frame");
    frame.set_parameter(Value::symbol("background-color"), Value::string(color));
}

#[test]
fn dark_gui_background_color_values_match_gnu_shape() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);
    seed_selected_frame_background_color(&mut eval, "#000000");

    let result = eval
        .eval_str(r##"(color-values "#000000" (selected-frame))"##)
        .expect("color-values probe should evaluate");
    assert_eq!(print_value_with_eval(&mut eval, &result), "(0 0 0)");
}

#[test]
fn dark_gui_background_color_dark_predicate_completes() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);
    seed_selected_frame_background_color(&mut eval, "#000000");

    let result = eval
        .eval_str(
            r##"(color-dark-p
             (mapcar (lambda (c) (/ c 65535.0))
                     (color-values "#000000" (selected-frame))))"##,
        )
        .expect("color-dark-p probe should evaluate");
    assert_eq!(result, Value::T);
}

#[test]
fn dark_gui_frame_current_background_mode_completes() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);
    seed_selected_frame_background_color(&mut eval, "#000000");

    let result = eval
        .eval_str(r#"(frame--current-background-mode (selected-frame))"#)
        .expect("current background mode probe should evaluate");
    let debug_result = eval
        .eval_str(
            r##"(list
            (frame-parameter nil 'background-color)
            (frame-parameter nil 'background-mode)
            frame-background-mode
            (terminal-parameter nil 'background-mode)
            (window-system (selected-frame))
            (tty-type (selected-frame))
            (color-values "#000000" (selected-frame))
            (frame--current-background-mode (selected-frame)))"##,
        )
        .expect("current background mode debug probe should evaluate");
    assert_eq!(
        print_value_with_eval(&mut eval, &debug_result),
        "(\"#000000\" light nil nil neo nil (0 0 0) dark)"
    );
    assert_eq!(result, Value::symbol("dark"));
}

#[test]
fn modify_frame_parameters_prefers_first_duplicate_frame_parameter_like_gnu() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let result = eval
        .eval_str(
            r##"(progn
             (modify-frame-parameters
              (selected-frame)
              '((background-color . "#000000")
                (background-color . "white")))
             (list
              (frame-parameter nil 'background-color)
              (face-background 'default nil t)
              (frame-parameter nil 'background-mode)))"##,
        )
        .expect("duplicate frame parameter probe should evaluate");
    assert_eq!(
        print_value_with_eval(&mut eval, &result),
        "(\"#000000\" \"#000000\" dark)"
    );
}

#[test]
fn gnu_startup_seeds_light_gui_chrome_faces_from_faces_el() {
    let mut eval = create_bootstrap_evaluator_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let _frame_id = bootstrap_runtime_gui_startup(&mut eval);
    run_gnu_startup(&mut eval);

    let result = eval
        .eval_str(
            r#"(list
             (window-system)
             (frame-parameter nil 'window-system)
             (display-graphic-p)
             (display-graphic-p (selected-frame))
             (xw-display-color-p (selected-frame))
             (frame-parameter nil 'display-type)
             (frame-parameter nil 'background-mode)
             (display-color-p)
             (display-color-cells)
             (face-default-spec 'mode-line)
             (face-spec-choose (face-default-spec 'mode-line) (selected-frame) 'no-match)
             (face-default-spec 'header-line)
             (face-spec-choose (face-default-spec 'header-line) (selected-frame) 'no-match)
             (condition-case err
                 (progn
                   (face-set-after-frame-default
                    (selected-frame)
                    (frame-parameters (selected-frame)))
                   (list
                    (face-background 'mode-line nil t)
                    (face-background 'mode-line-inactive nil t)
                    (face-background 'header-line nil t)
                    (face-background 'tab-bar nil t)
                    (face-background 'tab-line nil t)))
               (error (list 'error (error-message-string err))))
             (list
              (face-background 'mode-line nil t)
              (face-background 'mode-line-inactive nil t)
              (face-background 'header-line nil t)
              (face-background 'tab-bar nil t)
              (face-background 'tab-line nil t))
             (progn
               (set-face-attribute 'mode-line (selected-frame)
                                   :background "grey75"
                                   :foreground "black")
               (list
                (face-background 'mode-line nil t)
                (face-foreground 'mode-line nil t)))
             (face-background 'mode-line nil t)
             (face-foreground 'mode-line nil t)
             (face-background 'mode-line-inactive nil t)
             (face-background 'header-line nil t)
             (face-background 'tab-bar nil t)
             (face-background 'tab-line nil t))"#,
        )
        .expect("chrome face probe should evaluate");
    let values = list_to_vec(&result).expect("chrome face probe should return a list");
    assert_eq!(values.len(), 22);
    let rendered: Vec<String> = values
        .iter()
        .map(|value| print_value_with_eval(&mut eval, value))
        .collect();
    assert_eq!(
        rendered[0], "neo",
        "chrome probe should still report a GUI backend: {rendered:?}"
    );
    assert_eq!(
        rendered[1], "neo",
        "frame backend should stay on neo during startup: {rendered:?}"
    );
    assert_eq!(
        rendered[2], "t",
        "display-graphic-p should stay true during startup: {rendered:?}"
    );
    assert_eq!(
        rendered[3], "t",
        "display-graphic-p should stay true for the selected frame: {rendered:?}"
    );
    assert_eq!(
        rendered[4], "t",
        "xw-display-color-p should stay true for the selected frame: {rendered:?}"
    );
    assert_eq!(
        rendered[5], "color",
        "display-type should stay color during startup: {rendered:?}"
    );
    assert_eq!(
        rendered[6], "light",
        "background-mode should stay light during startup: {rendered:?}"
    );
    assert_eq!(
        rendered[7], "t",
        "display-color-p should stay true during startup: {rendered:?}"
    );
    assert_eq!(
        rendered[8], "16777216",
        "display-color-cells should stay high-color during startup: {rendered:?}"
    );
    assert_eq!(
        rendered[9],
        "((((class color grayscale) (min-colors 88) (background light)) :box (:line-width -1 :style released-button) :background \"grey75\" :foreground \"black\") (((class color grayscale) (min-colors 88) (background dark)) :box (:line-width -1 :style released-button) :background \"grey20\" :foreground \"white\") (t :inverse-video t))",
        "mode-line defface spec should be present: {rendered:?}"
    );
    assert_eq!(
        rendered[10],
        "(:box (:line-width -1 :style released-button) :background \"grey75\" :foreground \"black\")",
        "mode-line defface should match the live neo frame: {rendered:?}"
    );
    assert_eq!(
        rendered[11],
        "((default :inherit mode-line) (((type tty)) :inverse-video nil :underline t) (((class color grayscale) (background light)) :background \"grey90\" :foreground \"grey20\" :box nil) (((class color grayscale) (background dark)) :background \"grey20\" :foreground \"grey90\" :box nil) (((class mono) (background light)) :background \"white\" :foreground \"black\" :inverse-video nil :box nil :underline t) (((class mono) (background dark)) :background \"black\" :foreground \"white\" :inverse-video nil :box nil :underline t))",
        "header-line defface spec should be present: {rendered:?}"
    );
    assert_eq!(
        rendered[12], "(:inherit mode-line :background \"grey90\" :foreground \"grey20\" :box nil)",
        "header-line defface should match the live neo frame: {rendered:?}"
    );
    assert_eq!(
        rendered[13], "(\"grey75\" \"grey90\" \"grey90\" \"grey85\" \"grey85\")",
        "face-set-after-frame-default probe = {rendered:?}"
    );
    assert_eq!(
        rendered[14], "(\"grey75\" \"grey90\" \"grey90\" \"grey85\" \"grey85\")",
        "chrome probe = {rendered:?}"
    );
    assert_eq!(
        rendered[15], "(\"grey75\" \"black\")",
        "manual set-face-attribute should still work on the live GUI frame: {rendered:?}"
    );
    assert_eq!(rendered[16], "\"grey75\"", "chrome probe = {rendered:?}");
    assert_eq!(rendered[17], "\"black\"", "chrome probe = {rendered:?}");
    assert_eq!(rendered[18], "\"grey90\"", "chrome probe = {rendered:?}");
    assert_eq!(rendered[19], "\"grey90\"", "chrome probe = {rendered:?}");
    assert_eq!(rendered[20], "\"grey85\"", "chrome probe = {rendered:?}");
    assert_eq!(rendered[21], "\"grey85\"", "chrome probe = {rendered:?}");
}

#[test]
fn gnu_startup_clears_terminal_frame_without_deselecting_opening_gui_frame() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(BOOTSTRAP_CORE_FEATURES)
        .expect("bootstrap evaluator");
    let frame_id = bootstrap_runtime_gui_startup(&mut eval);

    let (tx, rx) = crossbeam_channel::unbounded();
    tx.send(neovm_core::keyboard::InputEvent::WindowClose {
        emacs_frame_id: u64::MAX,
    })
    .expect("queue close request");
    drop(tx);
    eval.init_input_system(rx);
    let result = eval.recursive_edit();
    result.expect("close request should let recursive edit exit cleanly");

    assert_eq!(
        eval.frame_manager().selected_frame().map(|frame| frame.id),
        Some(frame_id),
        "GUI startup should keep the opening frame selected through the first recursive edit"
    );
    assert_eq!(
        eval.obarray().symbol_value("terminal-frame"),
        Some(&Value::NIL),
        "GUI startup should clear terminal-frame after the first recursive edit enters the command loop"
    );
}

#[test]
fn gnu_startup_set_face_attribute_returns_on_live_gui_frame() {
    assert_eq!(
        eval_after_gnu_gui_startup(
            r#"(condition-case err
                  (progn
                    (set-face-attribute 'mode-line (selected-frame)
                                        :background "grey75"
                                        :foreground "black")
                    (list
                     (face-background 'mode-line nil t)
                     (face-foreground 'mode-line nil t)))
                (error (list 'error (error-message-string err))))"#,
        ),
        "(\"grey75\" \"black\")"
    );
}

#[test]
fn gnu_startup_face_set_after_frame_default_materializes_mode_line() {
    assert_eq!(
        eval_after_gnu_gui_startup(
            r#"(condition-case err
                  (progn
                    (face-set-after-frame-default
                     (selected-frame)
                     (frame-parameters (selected-frame)))
                    (list
                     (face-background 'mode-line nil t)
                     (face-foreground 'mode-line nil t)))
                (error (list 'error (error-message-string err))))"#,
        ),
        "(\"grey75\" \"black\")"
    );
}

#[test]
fn gnu_startup_face_recalc_loop_materializes_gui_chrome_faces_progressively() {
    assert_eq!(
        eval_after_gnu_gui_startup(
            r#"(let ((last :unset)
                     changes
                     errors)
                 (dolist (face (nreverse (face-list)))
                   (condition-case err
                       (progn
                         (face-spec-recalc face (selected-frame))
                         (internal-merge-in-global-face face (selected-frame)))
                     (error
                      (push (list face (error-message-string err)) errors)))
                   (let ((current (list (face-background 'mode-line nil t)
                                        (face-foreground 'mode-line nil t)
                                        (face-background 'mode-line-inactive nil t)
                                        (face-background 'header-line nil t)
                                        (face-background 'tab-bar nil t)
                                        (face-background 'tab-line nil t))))
                     (unless (equal current last)
                       (push (list face current) changes)
                       (setq last current))))
                 (list (nreverse changes) (nreverse errors)))"#,
        ),
        "(((default (\"grey75\" \"black\" \"grey90\" \"grey90\" \"grey85\" \"grey85\"))) nil)"
    );
}

#[test]
fn gnu_startup_face_spec_recalc_materializes_mode_line() {
    assert_eq!(
        eval_after_gnu_gui_startup(
            r#"(condition-case err
                  (progn
                    (face-spec-recalc 'mode-line (selected-frame))
                    (list
                     (face-background 'mode-line nil t)
                     (face-foreground 'mode-line nil t)))
                (error (list 'error (error-message-string err))))"#
        ),
        "(\"grey75\" \"black\")"
    );
}

#[test]
fn gnu_startup_internal_merge_in_global_face_preserves_mode_line_after_recalc() {
    assert_eq!(
        eval_after_gnu_gui_startup(
            r#"(condition-case err
                  (progn
                    (face-spec-recalc 'mode-line (selected-frame))
                    (internal-merge-in-global-face 'mode-line (selected-frame))
                    (list
                     (face-background 'mode-line nil t)
                     (face-foreground 'mode-line nil t)))
                (error (list 'error (error-message-string err))))"#
        ),
        "(\"grey75\" \"black\")"
    );
}

#[test]
fn gnu_startup_face_background_getter_returns_on_live_gui_frame() {
    assert_eq!(
        eval_after_gnu_gui_startup(
            r#"(condition-case err
                  (face-background 'mode-line nil t)
                (error (list 'error (error-message-string err))))"#
        ),
        "\"grey75\""
    );
}

#[test]
fn gnu_startup_internal_set_lisp_face_attribute_returns_on_live_gui_frame() {
    assert_eq!(
        eval_after_gnu_gui_startup(
            r#"(condition-case err
                  (progn
                    (internal-set-lisp-face-attribute
                     'mode-line :background "grey75" (selected-frame))
                    (face-background 'mode-line nil t))
                (error (list 'error (error-message-string err))))"#
        ),
        "\"grey75\""
    );
}

#[test]
fn gnu_startup_internal_set_lisp_face_attribute_without_getter_returns_on_live_gui_frame() {
    assert_eq!(
        eval_after_gnu_gui_startup(
            r#"(condition-case err
                  (progn
                    (internal-set-lisp-face-attribute
                     'mode-line :background "grey75" (selected-frame))
                    'ok)
                (error (list 'error (error-message-string err))))"#
        ),
        "ok"
    );
}

/// End-to-end frame snapshot: bootstrap a GUI evaluator, install the
/// production hook, and drive it through the real subr. The JSON must be
/// the serde form of FrameDisplayState ({"frames":[...]}) and the text form
/// must carry the frame header and the scratch buffer name.
#[test]
fn frame_snapshot_subr_end_to_end_json_and_text() {
    let mut eval = create_bootstrap_evaluator_cached_with_features(&["neomacs"])
        .expect("cached bootstrap evaluator");
    let _bootstrap = bootstrap_buffers(&mut eval, 960, 640, gui_display());
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame after bootstrap")
        .id;
    configure_gnu_startup_state(&mut eval, frame_id, &gui_startup());
    REDISPLAY_RUNTIME.with(|runtime| runtime.enable_cosmic_metrics());
    super::frame_layout::install_frame_snapshot_fn(&mut eval);

    let json_value = eval
        .eval_str("(neomacs--frame-snapshot t 'json)")
        .expect("all-frames JSON snapshot");
    let json = json_value.as_str_owned().expect("string result");
    let doc: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let frames = doc["frames"].as_array().expect("frames array");
    assert!(
        !frames.is_empty(),
        "at least the selected frame: {json:.200}"
    );
    let window_infos = frames[0]["window_infos"]
        .as_array()
        .expect("window_infos serialized");
    assert!(
        window_infos
            .iter()
            .any(|info| info["buffer_name"].as_str().is_some_and(|n| !n.is_empty())),
        "window_infos carry buffer names: {window_infos:?}"
    );

    let text_value = eval
        .eval_str("(neomacs--frame-snapshot)")
        .expect("selected-frame text snapshot");
    let text = text_value.as_str_owned().expect("string result");
    assert!(text.starts_with("=== frame "), "frame header:\n{text}");
    assert!(
        text.contains("*scratch*"),
        "scratch window visible:\n{text}"
    );

    let faces_value = eval
        .eval_str("(neomacs--frame-snapshot nil 'text-faces)")
        .expect("face-annotated snapshot");
    let faces_text = faces_value.as_str_owned().expect("string result");
    assert!(
        faces_text.contains(": run ") && faces_text.contains("fg=#"),
        "face runs with colors:\n{faces_text:.400}"
    );
}

#[test]
fn primary_display_host_reports_quality_policy_frame_shader_suppression() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let mut host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 900),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::new(
            FrameShaderAvailability::SuppressedByQualityPolicy,
        )),
        requested_frame_shader: Mutex::new(None),
        #[cfg(feature = "neo-term")]
        terminal_state: super::TerminalHostState::new(new_shared_terminals()),
    };

    let error = neovm_core::emacs_core::DisplayHost::set_frame_shader(
        &host,
        Some((
            "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0); }".to_owned(),
            ShaderSurfaceLanguage::Wgsl,
            Vec::new(),
        )),
    )
    .expect_err("suppressed frame shader must be observable to Lisp");

    assert!(error.contains("render-quality policy"), "{error}");
    assert!(cmd_rx.try_recv().is_err(), "suppressed shader was queued");

    host.render_capabilities = Arc::new(SharedRenderCapabilities::new(
        FrameShaderAvailability::Available,
    ));
    neovm_core::emacs_core::DisplayHost::set_frame_shader(
        &host,
        Some((
            "fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> { return vec4<f32>(u_gain()); }"
                .to_owned(),
            ShaderSurfaceLanguage::Wgsl,
            vec![ShaderSurfaceUniformInit {
                name: "gain".to_owned(),
                value: [0.25, 0.0, 0.0, 0.0],
                components: 1,
            }],
        )),
    )
    .expect("hardware policy accepts a frame shader");
    let _installation = cmd_rx.recv().expect("installation command");

    neovm_core::emacs_core::DisplayHost::set_frame_shader_uniform(
        &host,
        "gain",
        [0.75, 0.0, 0.0, 0.0],
    )
    .expect("live uniform update");
    let _uniform_update = cmd_rx.recv().expect("uniform command");

    neovm_core::emacs_core::DisplayHost::display_reset(&host);
    let RenderCommand::Asset(AssetCommand::FrameShaderSet {
        request: _,
        composed: Some((_, _, uniforms)),
    }) = cmd_rx.recv().expect("recovery command")
    else {
        panic!("display reset did not restore the frame shader");
    };
    assert_eq!(uniforms[0].value, [0.75, 0.0, 0.0, 0.0]);
}

#[cfg(feature = "neo-term")]
#[test]
fn primary_display_host_routes_typed_terminal_requests_to_the_renderer() {
    let (cmd_tx, cmd_rx) = crossbeam_channel::unbounded();
    let shared_terminals = new_shared_terminals();
    let host = PrimaryWindowDisplayHost {
        cmd_tx: cmd_tx.clone(),
        render_waker: None,
        font_sizing: FontSizing::xft(),
        primary_window_adopted: false,
        primary_frame_id: None,
        last_window_titles: Mutex::new(std::collections::HashMap::new()),
        font_metrics: None,
        primary_window_size: shared_primary_window_size(1600, 900),
        image_catalog: test_image_catalog(&cmd_tx, Arc::new(ImageRenderState::default())),
        resolved_videos: Mutex::new(super::ResolvedVideoRegistry::default()),
        resolved_webkits: Mutex::new(std::collections::HashMap::new()),
        resolved_surfaces: Mutex::new(super::ResolvedSurfaceMemo::default()),
        render_capabilities: Arc::new(SharedRenderCapabilities::default()),
        requested_frame_shader: Mutex::new(None),
        terminal_state: super::TerminalHostState::new(shared_terminals),
    };

    let id = neovm_core::emacs_core::DisplayHost::create_terminal(
        &host,
        TerminalCreateRequest {
            size: TerminalGridSize {
                cols: std::num::NonZeroU16::new(96).unwrap(),
                rows: std::num::NonZeroU16::new(31).unwrap(),
            },
            target: CoreTerminalDisplayTarget::Floating,
            shell: Some("/bin/sh".to_owned()),
        },
    )
    .expect("terminal create should queue");
    neovm_core::emacs_core::DisplayHost::write_terminal(&host, id, b"echo typed\r".to_vec())
        .expect("terminal input should queue");
    neovm_core::emacs_core::DisplayHost::resize_terminal(
        &host,
        id,
        TerminalGridSize {
            cols: std::num::NonZeroU16::new(120).unwrap(),
            rows: std::num::NonZeroU16::new(40).unwrap(),
        },
    )
    .expect("terminal resize should queue");
    neovm_core::emacs_core::DisplayHost::set_floating_terminal(
        &host,
        id,
        TerminalFloatPlacement::new(12.5, 24.0, 0.9).unwrap(),
    )
    .expect("terminal placement should queue");
    assert_eq!(
        neovm_core::emacs_core::DisplayHost::terminal_text(&host, id)
            .expect("terminal text lookup should succeed"),
        None
    );
    neovm_core::emacs_core::DisplayHost::destroy_terminal(&host, id)
        .expect("terminal destroy should queue");
    assert_eq!(
        neovm_core::emacs_core::DisplayHost::terminal_text(&host, id)
            .expect("destroyed terminal observation should settle"),
        None
    );
    assert!(
        neovm_core::emacs_core::DisplayHost::write_terminal(&host, id, b"stale".to_vec())
            .unwrap_err()
            .contains("being destroyed")
    );
    let arbitrary_id = neovm_core::emacs_core::display_host::TerminalId::new(999).unwrap();
    assert!(
        neovm_core::emacs_core::DisplayHost::terminal_text(&host, arbitrary_id)
            .unwrap_err()
            .contains("unknown neo-term terminal id")
    );

    let commands: Vec<_> = cmd_rx.try_iter().collect();
    assert_eq!(commands.len(), 5);
    assert!(matches!(
        &commands[0],
        RenderCommand::Terminal(TerminalCommand::TerminalCreate {
            id: command_id,
            size,
            target: TerminalDisplayTarget::Floating,
            shell: Some(shell),
        }) if *command_id == id
            && *size == TerminalGridSize::new(96, 31).unwrap()
            && shell == "/bin/sh"
    ));
    assert!(matches!(
        &commands[1],
        RenderCommand::Terminal(TerminalCommand::TerminalWrite { id: command_id, data })
            if *command_id == id && data == b"echo typed\r"
    ));
    assert!(matches!(
        &commands[2],
        RenderCommand::Terminal(TerminalCommand::TerminalResize {
            id: command_id,
            size,
        }) if *command_id == id && *size == TerminalGridSize::new(120, 40).unwrap()
    ));
    assert!(matches!(
        &commands[3],
        RenderCommand::Terminal(TerminalCommand::TerminalSetFloat {
            id: command_id,
            placement,
        }) if *command_id == id
            && *placement == TerminalFloatPlacement::new(12.5, 24.0, 0.9).unwrap()
    ));
    assert!(matches!(
        &commands[4],
        RenderCommand::Terminal(TerminalCommand::TerminalDestroy { id: command_id })
            if *command_id == id
    ));
}

#[cfg(feature = "neo-term")]
#[test]
fn terminal_id_allocation_is_isolated_per_editor_host() {
    let first = super::TerminalHostState::new(new_shared_terminals());
    let second = super::TerminalHostState::new(new_shared_terminals());

    assert_eq!(first.allocate().unwrap(), second.allocate().unwrap());
    assert_eq!(
        first.allocate().unwrap().get(),
        super::HOST_TERMINAL_ID_START + 1
    );
    assert_eq!(
        second.allocate().unwrap().get(),
        super::HOST_TERMINAL_ID_START + 1
    );
}

#[test]
fn source_bootstrap_does_not_forward_init_directory_into_loadup_top_level() {
    // Issue #316: imageless startup previously filtered a user session argv
    // with a blacklist, which let --init-directory reach loadup.el's
    // disposable top-level pass.  The preload-only variant deliberately has
    // no argv field, so that state is unrepresentable rather than filtered.
    assert!(matches!(
        source_bootstrap_loadup_invocation(),
        LoadupInvocation::PreloadOnly
    ));
}

/// **The stale-bytecode refusal covers this crate's in-process tests.**
///
/// It did not.  Ledger 202 gated the refusal on `cfg!(test)`, which Rust sets
/// only for the crate being compiled as a test -- so it was live for
/// `neovm-core`'s own 482 in-process tests and DARK for the 62 here and the 13
/// in `neomacs-layout-engine`, which link `neovm-core` as an ordinary
/// dependency.  202 recorded that as residual 1; ledger 206 reproduced it.
///
/// The reproduction, on one deliberately staled tree carrying a single stale
/// `lisp/international/emoji-zwj.elc`:
///
/// ```text
/// neovm-core  the_gui_terminal_layer_adds_documentation_and_never_rewrites_it
///             REFUSED in 2.0s, naming the file and both mtimes
/// neomacs     startup::tests::bootstrap_gui_frame_uses_gnu_cursor_and_pointer_color_defaults
///             1 passed in 9.4s, silently
/// ```
///
/// RED before ledger 206: `for_this_process` did not exist, and the policy this
/// process got was `Warn`.  It is now `Refuse` by default in every process that
/// has not announced itself a shipped editor -- and the only one that does is
/// `neomacs`'s own `main`, a few lines up in this same file's parent module,
/// which is a different program from this test binary.
///
/// One honest caveat: with `NEOVM_ALLOW_STALE_BYTECODE` set, both arms are
/// `Warn` and this check cannot tell them apart -- which is what that variable
/// is FOR, and why the red above was produced with it unset.  A gate run that
/// exported it globally would make this guard vacuous.
#[test]
fn the_stale_bytecode_refusal_covers_this_crates_tests() {
    use neovm_core::emacs_core::load::{ALLOW_STALE_BYTECODE_ENV, StaleBytecodePolicy};

    let expected = match std::env::var_os(ALLOW_STALE_BYTECODE_ENV) {
        Some(value) if !value.is_empty() => StaleBytecodePolicy::Warn,
        _ => StaleBytecodePolicy::Refuse,
    };
    assert_eq!(
        StaleBytecodePolicy::for_this_process(),
        expected,
        "this crate's tests boot an image in-process, so they must not be \
         allowed to read bytecode that does not implement the checked-out \
         source; `main' announcing itself a shipped editor is a different \
         process from this one"
    );
}

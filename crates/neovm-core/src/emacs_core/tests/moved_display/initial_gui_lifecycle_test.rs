//! Re-homed from display::window_cmds::tests::initial_gui_lifecycle_test
//! (P1 B1, oracleb).
//!
//! The three tests are blocked in display's cfg(test) unit runtime by
//! core-owned pieces: `frame-after-make-frame` and the
//! `window-change` redisplay hook slot. Core's boot facade has every layer
//! real, including the canonical hook set installed by the one constructor.
//! The bodies are the display originals; the adaptations are the signed API
//! drift (`crate::test_utils::new_context()` for
//! `Context::new(unit_runtime())`, explicit trait imports, and the
//! `neovm_eval::emacs_core::eval::protocol` path for
//! `InitialGuiFrameDefaults`).

use crate::emacs_core::eval::RedisplayHookPolicyGuard;
use crate::window::{DisplayFrameExt, DisplayFrameManagerExt};
use crate::window::{FrameId, FrameVisibility};
use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};
use neovm_eval::emacs_core::eval::protocol::InitialGuiFrameDefaults;
use neovm_eval::emacs_core::eval::{FontPxProbeResult, GuiFrameHostSize};
use neovm_eval::emacs_core::print::format_eval_result;
use neovm_value::value::Value;

fn initial_host() -> neovm_display::test_utils::window_cmds_host::RecordingDisplayHost {
    use neovm_display::test_utils::window_cmds_host::{RecordingDisplayHost, resolved_frame_font};
    // Assign the public field after Default. A struct update would name
    // fail_resize and font_requests, which are pub(crate) in neovm-display
    // (peer 5543) and invisible to this crate.
    let mut host = RecordingDisplayHost::default();
    host.initial_gui_defaults = Some(InitialGuiFrameDefaults {
        display: GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::X11),
        font: resolved_frame_font(
            "Initial Mono",
            "InitialMono-Regular",
            130,
            FontPxProbeResult {
                pixel_size: 17,
                height: 18,
                ascent: 14,
                descent: 4,
                max_width: 9,
                space_width: 9,
                average_width: 9,
            },
        ),
        size: GuiFrameHostSize {
            width: 745,
            height: 680,
        },
    });
    host
}

fn terminal_startup_context() -> neovm_eval::emacs_core::eval::Context {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.edit.buffers.create_buffer("*scratch*");
    eval.edit.buffers.set_current(buffer);
    let terminal = eval.frame_manager_mut().create_frame("F1", 80, 25, buffer);
    let frame = eval.frame_manager_mut().get_mut(terminal).unwrap();
    frame.set_window_system(None);
    frame.char_width = 1.0;
    frame.char_height = 1.0;
    frame.displays_chrome = false;
    eval
}

#[test]
fn frame_creation_finish_observes_final_lisp_dimensions() {
    crate::test_utils::init_test_tracing();
    let mut eval = terminal_startup_context();
    let host = initial_host();
    let finished = host.finished.clone();
    eval.set_display_host(Box::new(host));
    let value = eval
        .eval_str("(x-create-frame '((name . \"Initial\") (visibility . nil)))")
        .unwrap();
    eval.set_variable("issue417-frame", value);
    assert!(finished.borrow().is_empty());

    eval.eval_str("(set-frame-size issue417-frame 81 27)")
        .unwrap();
    eval.eval_str("(frame-after-make-frame issue417-frame t)")
        .unwrap();
    let frame = eval
        .frame_manager()
        .get(FrameId(value.as_frame_id().unwrap()))
        .unwrap();
    let finished = finished.borrow();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].frame_id, frame.id);
    assert_eq!(
        (finished[0].width, finished[0].height),
        (frame.width, frame.height)
    );
    assert_eq!(finished[0].geometry_hints, frame.gui_geometry_hints());
    // Geometry completion must retain the original invisible request; it
    // does not authorize the native backend to expose this logical frame.
    assert_eq!(finished[0].visibility, FrameVisibility::Invisible);
    assert!(finished[0].decorated);
}

fn check_old_selected_window_after_gui_startup() {
    let mut eval = terminal_startup_context();
    eval.set_variable("noninteractive", Value::NIL);
    eval.set_display_host(Box::new(initial_host()));
    // Supply the frontend callback so public redisplay enters its ordinary
    // window-change pass. This test checks Lisp observations, not hook records.
    eval.redisplay_fn = Some(Box::new(|_| {}));

    let before = eval.eval_str(
        r#"(progn
           (setq startup-terminal-frame (selected-frame)
                 startup-terminal-window (selected-window)
                 startup-gui-frame (x-create-frame '((name . "Initial GUI"))))
           (select-frame startup-gui-frame)
           (delete-frame startup-terminal-frame)
           (list (window-live-p startup-terminal-window)
                 (eq (selected-window) startup-terminal-window)
                 (eq (old-selected-window) startup-terminal-window)))"#,
    );
    assert_eq!(format_eval_result(&before), "OK (nil nil t)");

    // GNU window.c:496-503 defines old-selected-window as the selection at the
    // last window-change pass; window_change_record:3993 advances it. Creating
    // the first graphical frame must not preserve the dead terminal forever.
    eval.eval_str("(redisplay t)").expect("public redisplay");
    assert_eq!(
        format_eval_result(&eval.eval_str(
            "(list (eq (old-selected-window) (selected-window))
                   (window-live-p (old-selected-window)))",
        )),
        "OK (t t)"
    );
}

#[test]
fn old_selected_window_follows_gui_startup_after_redisplay_gnu() {
    crate::test_utils::init_test_tracing();
    let _policy = RedisplayHookPolicyGuard::gnu();
    check_old_selected_window_after_gui_startup();
}

#[test]
fn old_selected_window_follows_gui_startup_after_redisplay_legacy() {
    crate::test_utils::init_test_tracing();
    let _policy = RedisplayHookPolicyGuard::legacy();
    check_old_selected_window_after_gui_startup();
}

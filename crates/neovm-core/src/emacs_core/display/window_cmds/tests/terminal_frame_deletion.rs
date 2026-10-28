use crate::emacs_core::{Context, Value, format_eval_result};
use crate::window::FrameVisibility;

fn context_with_hidden_gui_peer(primary_window_system: Option<&str>) -> Context {
    let mut eval = Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(scratch);
    let primary = eval.frames.create_frame("target", 800, 600, scratch);
    let frame = eval.frames.get_mut(primary).expect("target frame");
    frame.set_window_system(primary_window_system.map(Value::symbol));
    frame.visibility = FrameVisibility::Visible;
    let peer = eval.frames.create_frame("hidden GUI", 800, 600, scratch);
    let frame = eval.frames.get_mut(peer).expect("hidden GUI frame");
    frame.set_window_system(Some(Value::symbol("neo")));
    frame.visibility = FrameVisibility::Invisible;
    eval.set_variable("tested-frame", Value::make_frame(primary.0));
    eval.set_variable("hidden-peer", Value::make_frame(peer.0));
    eval
}

#[test]
fn terminal_frame_can_be_deleted_when_only_an_invisible_gui_frame_remains() {
    crate::test_utils::init_test_tracing();
    let mut eval = context_with_hidden_gui_peer(None);
    // GNU frame-initialize retires its terminal even when initial-frame-alist
    // requests an invisible graphical frame. No force argument is required.
    let result = eval.eval_str(
        "(progn
           (delete-frame tested-frame)
           (list (frame-live-p tested-frame)
                 (and (frame-live-p hidden-peer) t)
                 (frame-visible-p hidden-peer)
                 (length (frame-list))))",
    );
    assert_eq!(format_eval_result(&result), "OK (nil t nil 1)");
}

#[test]
fn terminal_frame_cannot_be_hidden_when_only_an_invisible_gui_frame_remains() {
    crate::test_utils::init_test_tracing();
    let mut eval = context_with_hidden_gui_peer(None);
    let result = eval.eval_str(
        "(condition-case err
             (make-frame-invisible tested-frame)
           (error err))",
    );
    assert_eq!(
        format_eval_result(&result),
        "OK (error \"Attempt to make invisible the sole visible or iconified frame\")",
    );
    let state = eval.eval_str("(list (frame-visible-p tested-frame) (length (frame-list)))");
    assert_eq!(format_eval_result(&state), "OK (t 2)");
}

#[test]
fn graphical_frame_cannot_be_deleted_when_only_an_invisible_gui_frame_remains() {
    crate::test_utils::init_test_tracing();
    let mut eval = context_with_hidden_gui_peer(Some("neo"));
    let result = eval.eval_str(
        "(condition-case err
             (delete-frame tested-frame)
           (error err))",
    );
    assert_eq!(
        format_eval_result(&result),
        "OK (error \"Attempt to delete the sole visible or iconified frame\")",
    );
    let state = eval.eval_str("(list (frame-visible-p tested-frame) (length (frame-list)))");
    assert_eq!(format_eval_result(&state), "OK (t 2)");
}

#[test]
fn last_graphical_frame_deletion_retains_connection_for_replacement_frame() {
    use crate::emacs_core::terminal::pure::{TerminalRuntimeConfig, ensure_terminal_runtime_owner};
    use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};

    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    ensure_terminal_runtime_owner(
        0,
        "graphical connection",
        TerminalRuntimeConfig::window_system(
            GraphicalDisplayIdentity::named(GraphicalBackend::X11, ":417").unwrap(),
        ),
    );
    ensure_terminal_runtime_owner(1, "initial_terminal", TerminalRuntimeConfig::inactive());
    let scratch = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(scratch);
    let terminal_frame =
        eval.frames
            .create_frame_on_terminal("initial terminal", 1, 80, 25, scratch);
    eval.frames.get_mut(terminal_frame).unwrap().initial = true;
    let graphical_frame = eval
        .frames
        .create_frame_on_terminal("opening GUI", 0, 800, 600, scratch);
    eval.frames
        .get_mut(graphical_frame)
        .unwrap()
        .set_window_system(Some(Value::symbol(
            crate::emacs_core::display::gui_window_system_symbol(),
        )));
    eval.set_variable("tested-frame", Value::make_frame(graphical_frame.0));
    eval.set_variable(
        "initial-terminal-frame",
        Value::make_frame(terminal_frame.0),
    );

    // GNU toolkit backends retain an empty graphical connection. Retiring an
    // opening GUI frame must not discard connection parameters needed by its
    // replacement, even while the initial terminal frame remains alive.
    let retired = eval.eval_str(
        "(progn
           (setq tested-terminal (frame-terminal tested-frame))
           (set-terminal-parameter tested-terminal 'connection-marker 'retained)
           (delete-frame tested-frame)
           (list (frame-live-p tested-frame)
                 (and (terminal-live-p tested-terminal) t)
                 (and (terminal-live-p tested-terminal)
                      (terminal-parameter tested-terminal 'connection-marker))
                 (and (memq tested-terminal (terminal-list)) t)
                 (and (frame-live-p initial-terminal-frame) t)))",
    );
    assert_eq!(format_eval_result(&retired), "OK (nil t retained t t)");

    let replacement = eval.eval_str(
        "(let ((replacement (x-create-frame '((visibility . nil)))))
           (list (and (frame-live-p replacement) t)
                 (eq (frame-terminal replacement) tested-terminal)
                 (terminal-parameter (frame-terminal replacement) 'connection-marker)
                 (terminal-name (frame-terminal replacement))
                 (and (frame-live-p initial-terminal-frame) t)))",
    );
    assert_eq!(
        format_eval_result(&replacement),
        r#"OK (t t retained ":417" t)"#,
    );
    let explicit_close = eval.eval_str(
        "(let ((replacement (car (delq initial-terminal-frame (frame-list)))))
           (delete-terminal tested-terminal t)
           (list (terminal-live-p tested-terminal)
                 (frame-live-p replacement)
                 (and (memq tested-terminal (terminal-list)) t)
                 (and (frame-live-p initial-terminal-frame) t)
                 (length (frame-list))))",
    );
    assert_eq!(format_eval_result(&explicit_close), "OK (nil nil nil t 1)");
}

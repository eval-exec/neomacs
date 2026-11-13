//! Re-homed from display::window_cmds::tests::terminal_frame_deletion (P1 B1).
//!
//! The test is blocked in display's cfg(test) unit runtime by the core-owned
//! `terminal-list` subr, which the absent-layer runtime cannot supply. Core's
//! boot facade has every layer real. The body is the display original; the
//! only adaptations are the signed API drift
//! (`crate::test_utils::new_context()` for `Context::new(unit_runtime())` and
//! the public `buffer_manager_mut()` accessor).

use crate::window::DisplayFrameManagerExt;
use crate::window::FrameVisibility;
use neovm_eval::emacs_core::print::format_eval_result;
use neovm_value::value::Value;

#[test]
fn last_graphical_frame_deletion_retains_connection_for_replacement_frame() {
    use crate::emacs_core::terminal::pure::{TerminalRuntimeConfig, ensure_terminal_runtime_owner};
    use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};

    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    ensure_terminal_runtime_owner(
        0,
        "graphical connection",
        TerminalRuntimeConfig::window_system(
            GraphicalDisplayIdentity::named(GraphicalBackend::X11, ":417").unwrap(),
        ),
    );
    ensure_terminal_runtime_owner(1, "initial_terminal", TerminalRuntimeConfig::inactive());
    let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
    eval.buffer_manager_mut().set_current(scratch);
    let terminal_frame =
        eval.frame_manager_mut()
            .create_frame_on_terminal("initial terminal", 1, 80, 25, scratch);
    eval.frame_manager_mut()
        .get_mut(terminal_frame)
        .unwrap()
        .initial = true;
    let graphical_frame =
        eval.frame_manager_mut()
            .create_frame_on_terminal("opening GUI", 0, 800, 600, scratch);
    eval.frame_manager_mut()
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

use super::pure::*;
use crate::emacs_core::error::{FlowKind, FlowResultExt as _};
use crate::emacs_core::value::Value;

#[cfg(test)]
mod idle_redraw;

#[test]
fn redraw_frame_nil_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_redraw_frame(&mut ctx, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn redraw_frame_rejects_non_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_redraw_frame(&mut ctx, vec![Value::string("not-a-frame")]);
    assert!(result.is_err());
}

#[test]
fn redraw_display_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::eval::Context::new();
    let result = builtin_redraw_display(&mut ctx, vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn ding_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_ding(vec![]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn ding_with_arg_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_ding(vec![Value::T]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn open_termscript_signals_tty_error() {
    crate::test_utils::init_test_tracing();
    let result = builtin_open_termscript(vec![Value::NIL]);
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Current frame is not on a tty device")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn send_string_to_terminal_rejects_non_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_send_string_to_terminal(&mut eval, vec![Value::fixnum(42)]);
    assert!(result.is_err());
}

#[test]
fn send_string_to_terminal_accepts_string() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_send_string_to_terminal(&mut eval, vec![Value::string("hello")]).unwrap();
    assert!(result.is_nil());
}

// Issue #453: GNU's `Fsend_string_to_terminal' writes the string to the
// terminal (`src/dispnew.c:6808').  A stub that validates arguments and drops
// the bytes silently breaks every Lisp-side terminal negotiation -- `kkp.el'
// enables the Kitty Keyboard Protocol and queries capabilities exclusively
// through this primitive, so nothing it sends can reach the tty.

struct ByteRecordingHost {
    writes: std::rc::Rc<std::cell::RefCell<Vec<Vec<u8>>>>,
}

impl crate::emacs_core::terminal::pure::TerminalHost for ByteRecordingHost {
    fn suspend_tty(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn resume_tty(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writes.borrow_mut().push(bytes.to_vec());
        Ok(())
    }
}

fn configure_interactive_tty_terminal() {
    use crate::emacs_core::terminal::pure::TerminalRuntimeConfig;
    use crate::emacs_core::terminal::pure::configure_terminal_runtime;
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_string()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
}

fn reset_terminal_to_bootstrap() {
    use crate::emacs_core::terminal::pure::{reset_terminal_host, reset_terminal_runtime};
    reset_terminal_host();
    reset_terminal_runtime();
}

fn signal_message(result: crate::emacs_core::error::EvalResult) -> String {
    match result.kinded() {
        Err(FlowKind::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            sig.data
                .first()
                .and_then(Value::as_lisp_string)
                .map(|message| String::from_utf8_lossy(message.as_bytes()).into_owned())
                .unwrap_or_default()
        }
        other => panic!("expected an error signal, got {other:?}"),
    }
}

#[test]
fn send_string_to_terminal_delivers_bytes_to_the_termcap_host() {
    crate::test_utils::init_test_tracing();
    configure_interactive_tty_terminal();
    let writes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    crate::emacs_core::terminal::pure::set_terminal_host(Box::new(ByteRecordingHost {
        writes: std::rc::Rc::clone(&writes),
    }));
    let mut eval = crate::emacs_core::eval::Context::new();
    builtin_send_string_to_terminal(&mut eval, vec![Value::string("PROBE-BYTES")]).unwrap();
    assert_eq!(
        writes.borrow().as_slice(),
        [b"PROBE-BYTES".to_vec()],
        "send-string-to-terminal must deliver its bytes to the terminal"
    );
}

#[test]
fn send_string_to_terminal_writes_to_stdout_for_the_initial_terminal() {
    crate::test_utils::init_test_tracing();
    reset_terminal_to_bootstrap();
    // GNU: `out = stdout' for `output_initial' -- what batch, daemon and
    // pre-tty sessions run on (src/dispnew.c:6823-6824).  nextest runs every
    // test in its own process, so borrowing fd 1 for this assertion cannot
    // leak into sibling tests.
    let mut capture = tempfile::tempfile().unwrap();
    use std::io::{Read as _, Seek as _, Write as _};
    use std::os::fd::AsRawFd as _;
    capture.flush().unwrap();
    let saved = unsafe { libc::dup(1) };
    assert!(saved >= 0);
    unsafe { libc::dup2(capture.as_raw_fd(), 1) };
    let result = builtin_send_string_to_terminal(
        &mut crate::emacs_core::eval::Context::new(),
        vec![Value::string("PROBE-INITIAL-STDOUT")],
    );
    unsafe {
        libc::dup2(saved, 1);
        libc::close(saved);
    }
    result.unwrap();
    capture.seek(std::io::SeekFrom::Start(0)).unwrap();
    let mut captured = Vec::new();
    capture.read_to_end(&mut captured).unwrap();
    assert_eq!(
        captured, b"PROBE-INITIAL-STDOUT",
        "the initial terminal's output is stdout"
    );
}

#[test]
fn send_string_to_terminal_rejects_a_window_system_terminal() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::terminal::pure::TerminalRuntimeConfig;
    use crate::emacs_core::terminal::pure::configure_terminal_runtime;
    configure_terminal_runtime(TerminalRuntimeConfig::window_system(
        neomacs_display_protocol::GraphicalDisplayIdentity::named(
            neomacs_display_protocol::GraphicalBackend::X11,
            ":0",
        )
        .unwrap(),
    ));
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_send_string_to_terminal(&mut eval, vec![Value::string("x")]);
    assert_eq!(
        signal_message(result),
        "Device 0 is not a termcap terminal device",
        "GNU's error for a non-termcap device (src/dispnew.c:6827)"
    );
}

#[test]
fn send_string_to_terminal_rejects_a_suspended_terminal() {
    crate::test_utils::init_test_tracing();
    configure_interactive_tty_terminal();
    crate::emacs_core::terminal::pure::set_terminal_host(Box::new(ByteRecordingHost {
        writes: std::rc::Rc::default(),
    }));
    let mut eval = crate::emacs_core::eval::Context::new();
    crate::emacs_core::terminal::pure::builtin_suspend_tty(&mut eval, vec![]).unwrap();
    let result = builtin_send_string_to_terminal(&mut eval, vec![Value::string("x")]);
    assert_eq!(
        signal_message(result),
        "Terminal is currently suspended",
        "GNU's error for a tty without `tty->output' (src/dispnew.c:6835)"
    );
}

#[test]
fn internal_show_cursor_tracks_visibility() {
    crate::test_utils::init_test_tracing();
    reset_dispnew_thread_locals();
    let mut eval = crate::emacs_core::eval::Context::new();
    let fid = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let wid = eval.frames.get(fid).expect("frame").selected_window;
    let default_visible = builtin_internal_show_cursor_p(&mut eval, vec![]).unwrap();
    assert_eq!(default_visible, Value::T);
    assert!(
        eval.frames
            .get(fid)
            .and_then(|frame| frame.find_window(wid))
            .and_then(|window| window.display())
            .is_some_and(|display| !display.cursor_off_p)
    );

    builtin_internal_show_cursor(&mut eval, vec![Value::NIL, Value::NIL]).unwrap();
    let hidden = builtin_internal_show_cursor_p(&mut eval, vec![]).unwrap();
    assert!(hidden.is_nil());
    assert!(
        eval.frames
            .get(fid)
            .and_then(|frame| frame.find_window(wid))
            .and_then(|window| window.display())
            .is_some_and(|display| display.cursor_off_p && !display.last_cursor_off_p)
    );

    builtin_internal_show_cursor(&mut eval, vec![Value::NIL, Value::T]).unwrap();
    let visible = builtin_internal_show_cursor_p(&mut eval, vec![]).unwrap();
    assert_eq!(visible, Value::T);
    assert!(
        eval.frames
            .get(fid)
            .and_then(|frame| frame.find_window(wid))
            .and_then(|window| window.display())
            .is_some_and(|display| !display.cursor_off_p && !display.last_cursor_off_p)
    );
}

#[test]
fn internal_show_cursor_rejects_non_window() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    let result = builtin_internal_show_cursor(&mut eval, vec![Value::fixnum(1), Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn force_window_update_no_arg_returns_t() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result =
        crate::emacs_core::window_cmds::builtin_force_window_update(&mut eval, vec![]).unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn force_window_update_non_window_arg_returns_nil() {
    // A non-window, non-displayed-buffer OBJECT yields nil (GNU
    // `Fforce_window_update' returns t only for nil / a live window / a
    // buffer shown in some window).
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result =
        crate::emacs_core::window_cmds::builtin_force_window_update(&mut eval, vec![Value::T])
            .unwrap();
    assert!(result.is_nil());
}

#[test]
fn force_window_update_nil_arg_returns_t() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result =
        crate::emacs_core::window_cmds::builtin_force_window_update(&mut eval, vec![Value::NIL])
            .unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn force_window_update_live_window_returns_t() {
    // GNU returns t for a live window OBJECT (oracle test cx409); the prior
    // stub wrongly returned nil for any non-nil OBJECT.
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let selected =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();
    let result =
        crate::emacs_core::window_cmds::builtin_force_window_update(&mut eval, vec![selected])
            .unwrap();
    assert_eq!(result, Value::T);
}

#[test]
fn eval_internal_show_cursor_per_window_state() {
    crate::test_utils::init_test_tracing();
    reset_dispnew_thread_locals();
    let mut eval = crate::emacs_core::Context::new();
    let _ = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let selected =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();
    let other = crate::emacs_core::builtins::dispatch_builtin(
        &mut eval,
        "split-window-internal",
        // PIXEL-SIZE is required; GNU's `split-window` computes one.
        vec![Value::NIL, Value::fixnum(12), Value::NIL, Value::NIL],
    )
    .unwrap()
    .unwrap();

    // Both start visible
    assert_eq!(
        builtin_internal_show_cursor_p(&mut eval, vec![selected]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_internal_show_cursor_p(&mut eval, vec![other]).unwrap(),
        Value::T
    );

    // Hide selected window cursor
    builtin_internal_show_cursor(&mut eval, vec![Value::NIL, Value::NIL]).unwrap();
    assert!(
        builtin_internal_show_cursor_p(&mut eval, vec![selected])
            .unwrap()
            .is_nil()
    );
    assert_eq!(
        builtin_internal_show_cursor_p(&mut eval, vec![other]).unwrap(),
        Value::T
    );
}

#[test]
fn frame_z_order_lessp_returns_nil() {
    crate::test_utils::init_test_tracing();
    let result = builtin_frame_z_order_lessp(vec![Value::NIL, Value::NIL]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn frame_z_order_lessp_requires_two_args() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_frame_z_order_lessp(vec![]).is_err());
    assert!(builtin_frame_z_order_lessp(vec![Value::NIL]).is_err());
}

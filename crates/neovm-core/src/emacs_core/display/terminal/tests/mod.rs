use super::pure::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use std::cell::RefCell;
use std::rc::Rc;

struct RecordingTerminalHost {
    log: Rc<RefCell<Vec<&'static str>>>,
}

struct FailingDeleteTerminalHost;

struct RecordingTtyFrameHostFactory {
    requests: Rc<RefCell<Vec<TtyFrameOpenRequest>>>,
    lifecycle: Rc<RefCell<Vec<&'static str>>>,
}

struct FailingTtyFrameHostFactory;

impl TtyFrameHostFactory for RecordingTtyFrameHostFactory {
    fn open_tty(&mut self, request: TtyFrameOpenRequest) -> Result<OpenedTtyFrameHost, String> {
        self.requests.borrow_mut().push(request);
        Ok(OpenedTtyFrameHost::new(
            TtyFrameSize::new(132, 43).expect("non-zero test dimensions"),
            neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256),
            Box::new(RecordingTerminalHost {
                log: Rc::clone(&self.lifecycle),
            }),
        ))
    }
}

impl TtyFrameHostFactory for FailingTtyFrameHostFactory {
    fn open_tty(&mut self, _request: TtyFrameOpenRequest) -> Result<OpenedTtyFrameHost, String> {
        Err("test TTY open failure".to_string())
    }
}

impl TerminalHost for RecordingTerminalHost {
    fn suspend_tty(&mut self) -> Result<(), String> {
        self.log.borrow_mut().push("suspend");
        Ok(())
    }

    fn resume_tty(&mut self) -> Result<(), String> {
        self.log.borrow_mut().push("resume");
        Ok(())
    }

    fn delete_terminal(&mut self) -> Result<(), String> {
        self.log.borrow_mut().push("delete");
        Ok(())
    }
}

impl TerminalHost for FailingDeleteTerminalHost {
    fn suspend_tty(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn resume_tty(&mut self) -> Result<(), String> {
        Ok(())
    }

    fn delete_terminal(&mut self) -> Result<(), String> {
        Err("terminal already disappeared".to_string())
    }
}

#[test]
fn terminal_name_returns_string() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let result = builtin_terminal_name(&mut eval, vec![]).unwrap();
    assert_eq!(result, Value::string(TERMINAL_NAME));
}

#[test]
fn terminal_name_accepts_nil() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let result = builtin_terminal_name(&mut eval, vec![Value::NIL]).unwrap();
    assert_eq!(result, Value::string(TERMINAL_NAME));
}

#[test]
fn terminal_list_returns_singleton_list() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let result = builtin_terminal_list(vec![]).unwrap();
    let items = crate::emacs_core::value::list_to_vec(&result).unwrap();
    assert_eq!(items.len(), 1);
    let live = builtin_terminal_live_p(&mut eval, vec![items[0]]).unwrap();
    assert_eq!(live, Value::T);
}

#[test]
fn terminal_live_p_nil_is_live() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    assert_eq!(
        builtin_terminal_live_p(&mut eval, vec![Value::NIL]).unwrap(),
        Value::T
    );
}

#[test]
fn terminal_live_p_reports_frame_terminal_type_not_selected_global_type() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("*scratch*");
    let gui_frame = eval.frame_manager_mut().create_frame("F1", 80, 25, buffer);
    eval.frame_manager_mut()
        .get_mut(gui_frame)
        .expect("GUI frame")
        .set_window_system(Some(Value::symbol("neo")));
    let _ = eval.frame_manager_mut().select_frame(gui_frame);
    eval.set_variable("window-system", Value::symbol("neo"));
    eval.set_variable("initial-window-system", Value::symbol("neo"));

    let hidden_terminal =
        ensure_terminal_runtime_owner(1, "startup_terminal", TerminalRuntimeConfig::inactive());
    let hidden_frame =
        eval.frame_manager_mut()
            .create_frame_on_terminal("Fstartup-tty", 1, 80, 25, buffer);
    eval.frame_manager_mut()
        .get_mut(hidden_frame)
        .expect("hidden terminal frame")
        .set_window_system(None);

    let gui_terminal =
        builtin_frame_terminal(&mut eval, vec![Value::make_frame(gui_frame.0)]).unwrap();

    assert_eq!(
        builtin_terminal_live_p(&mut eval, vec![gui_terminal]).unwrap(),
        Value::symbol("neo")
    );
    assert_eq!(
        builtin_terminal_live_p(&mut eval, vec![hidden_terminal]).unwrap(),
        Value::T
    );
}

#[test]
fn terminal_live_p_int_is_not_live() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let result = builtin_terminal_live_p(&mut eval, vec![Value::fixnum(42)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn terminal_parameter_roundtrip() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let prev = builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::symbol("test-param"), Value::fixnum(99)],
    )
    .unwrap();
    assert!(prev.is_nil());

    let val = builtin_terminal_parameter(&mut eval, vec![Value::NIL, Value::symbol("test-param")])
        .unwrap();
    assert_eq!(val, Value::fixnum(99));
}

#[test]
fn terminal_parameter_defaults() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    // GNU has no terminal-parameter defaults; normal-erase-is-backspace in
    // particular must start nil so the (unless (terminal-parameter ...))
    // guard in normal-erase-is-backspace-setup-frame lets the real decision
    // run during command-line (DIVERGENCES.md entry 67). A fabricated 0 here
    // permanently latched the mode off on ^H-erase terminals.
    let normal = builtin_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::symbol("normal-erase-is-backspace")],
    )
    .unwrap();
    assert!(normal.is_nil());
}

#[test]
fn tty_type_returns_nil() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    assert!(builtin_tty_type(&mut eval, vec![]).unwrap().is_nil());
}

#[test]
fn tty_runtime_can_report_terminal_type_and_color_capability() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_string()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));

    let mut eval = Context::new();
    assert_eq!(
        builtin_tty_type(&mut eval, vec![]).unwrap(),
        Value::string("xterm-256color")
    );
    assert_eq!(
        builtin_tty_display_color_p(&mut eval, vec![]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_tty_display_color_cells(&mut eval, vec![]).unwrap(),
        Value::fixnum(256)
    );
    assert_eq!(
        builtin_controlling_tty_p(&mut eval, vec![]).unwrap(),
        Value::T
    );
}

#[test]
fn tty_runtime_can_name_the_live_tty_terminal() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(
        TerminalRuntimeConfig::interactive(Some("xterm-256color".to_string()), neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256))
            .with_name("/dev/tty"),
    );

    let mut eval = Context::new();
    assert_eq!(
        builtin_terminal_name(&mut eval, vec![]).unwrap(),
        Value::string("/dev/tty")
    );
}

#[test]
fn graphical_terminal_adopts_its_display_name() {
    // GNU names a window-system terminal after its display connection, so
    // `(terminal-name)` distinguishes a real GUI display from the display-less
    // bootstrap "initial_terminal". indent-bars' theme-reset guard relies on it.
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    reset_terminal_runtime();
    let mut eval = Context::new();
    assert_eq!(
        builtin_terminal_name(&mut eval, vec![]).unwrap(),
        Value::string("initial_terminal")
    );
    configure_terminal_runtime(TerminalRuntimeConfig::window_system().with_name(":0"));
    assert_eq!(
        builtin_terminal_name(&mut eval, vec![]).unwrap(),
        Value::string(":0")
    );
}

/// A display connection with no name to adopt keeps `"initial_terminal"` as its
/// name -- and still must not be mistaken for the initial terminal, because the
/// name was never the thing GNU asks about.
#[test]
fn nameless_graphical_terminal_is_still_not_the_initial_terminal() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    reset_terminal_runtime();
    configure_terminal_runtime(TerminalRuntimeConfig::window_system());
    let mut eval = Context::new();
    assert_eq!(
        builtin_terminal_name(&mut eval, vec![]).unwrap(),
        Value::string("initial_terminal")
    );
    let terminal = builtin_frame_terminal(&mut eval, vec![Value::NIL]).unwrap();
    assert_eq!(frame_initial_p(&mut eval, terminal), Ok(Value::NIL));
}

/// The GUI startup keeps a SECOND, display-less terminal for the hidden
/// bootstrap frame (`ensure_terminal_runtime_owner(GUI_STARTUP_TERMINAL_ID,
/// "startup_terminal", …)` in neomacs-bin).  That one is GNU's initial terminal,
/// and GNU's doc string is about exactly it: "If FRAME is a terminal object,
/// return non-nil if it holds the initial frame."
#[test]
fn frame_initial_p_separates_the_display_terminal_from_the_startup_terminal() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(TerminalRuntimeConfig::window_system().with_name(":0"));
    let mut eval = Context::new();
    // After the Context, which re-mints terminal handles.
    let startup_terminal =
        ensure_terminal_runtime_owner(1, "startup_terminal", TerminalRuntimeConfig::inactive());
    let display_terminal = builtin_frame_terminal(&mut eval, vec![Value::NIL]).unwrap();
    assert_eq!(frame_initial_p(&mut eval, display_terminal), Ok(Value::NIL));
    assert_eq!(frame_initial_p(&mut eval, startup_terminal), Ok(Value::T));
}

#[test]
fn tty_display_color_cells_returns_zero() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    assert_eq!(
        builtin_tty_display_color_cells(&mut eval, vec![]).unwrap(),
        Value::fixnum(0)
    );
}

#[test]
fn tty_top_frame_tracks_selected_frame_when_tty_runtime_is_active() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_string()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));

    let mut eval = Context::new();
    let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
    let frame_id = eval.frame_manager_mut().create_frame("F1", 80, 25, scratch);

    assert_eq!(
        builtin_tty_top_frame(&mut eval, vec![]).unwrap(),
        Value::make_frame(frame_id.0)
    );
}

#[test]
fn suspend_tty_signals_error() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    match builtin_suspend_tty(&mut eval, vec![]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn resume_tty_signals_error() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    match builtin_resume_tty(&mut eval, vec![]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn suspend_tty_runs_hook_and_invokes_terminal_host() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_string()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let log = Rc::new(RefCell::new(Vec::new()));
    set_terminal_host(Box::new(RecordingTerminalHost {
        log: Rc::clone(&log),
    }));
    eval.eval_str(
        r#"
(setq suspend-log nil)
(setq suspend-tty-functions
      (list (lambda (term) (setq suspend-log term))))
"#,
    )
    .expect("install suspend hook setup");

    assert_eq!(builtin_suspend_tty(&mut eval, vec![]).unwrap(), Value::NIL);
    assert_eq!(log.borrow().as_slice(), &["suspend"]);
    assert_eq!(
        eval.eval_str("suspend-log").expect("suspend-log value"),
        terminal_handle_value()
    );
}

#[test]
fn resume_tty_runs_hook_after_terminal_host_resume() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_string()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let log = Rc::new(RefCell::new(Vec::new()));
    set_terminal_host(Box::new(RecordingTerminalHost {
        log: Rc::clone(&log),
    }));
    builtin_suspend_tty(&mut eval, vec![]).expect("suspend tty");
    eval.eval_str(
        r#"
(setq resume-log nil)
(setq resume-tty-functions
      (list (lambda (term) (setq resume-log term))))
"#,
    )
    .expect("install resume hook setup");

    assert_eq!(builtin_resume_tty(&mut eval, vec![]).unwrap(), Value::NIL);
    assert_eq!(log.borrow().as_slice(), &["suspend", "resume"]);
    assert_eq!(
        eval.eval_str("resume-log").expect("resume-log value"),
        terminal_handle_value()
    );
}

#[test]
fn delete_terminal_nil_signals_sole_terminal_error() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    match builtin_delete_terminal(&mut eval, vec![]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Attempt to delete the sole active display terminal"
                )]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn delete_terminal_force_marks_terminal_dead_and_clears_terminal_list() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let handle = terminal_handle_value();

    assert_eq!(
        builtin_delete_terminal(&mut eval, vec![Value::NIL, Value::T]).unwrap(),
        Value::NIL
    );
    assert!(
        builtin_terminal_live_p(&mut eval, vec![handle])
            .unwrap()
            .is_nil(),
        "deleted terminal should no longer be live"
    );
    let terminals = builtin_terminal_list(vec![]).unwrap();
    assert!(
        crate::emacs_core::value::list_to_vec(&terminals)
            .expect("terminal-list result")
            .is_empty(),
        "deleted terminal should be removed from terminal-list"
    );
}

#[test]
fn delete_terminal_force_runs_hook_and_deletes_frames_on_terminal() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
    let _ = eval
        .frame_manager_mut()
        .create_frame_on_terminal("F1", TERMINAL_ID, 80, 25, scratch);
    let handle = terminal_handle_value();
    eval.eval_str(
        r#"
(setq deleted-terminal-log nil)
(setq delete-terminal-functions
      (list (lambda (term) (setq deleted-terminal-log term))))
"#,
    )
    .expect("install hook setup");

    assert_eq!(
        builtin_delete_terminal(&mut eval, vec![Value::NIL, Value::T]).unwrap(),
        Value::NIL
    );
    assert!(
        eval.frame_manager().frame_list().is_empty(),
        "delete-terminal should remove frames on the terminal"
    );
    assert_eq!(
        eval.eval_str("deleted-terminal-log")
            .expect("deleted-terminal-log value"),
        handle
    );
}

#[test]
fn delete_terminal_force_defers_frame_hooks_until_pending_safe_funcalls_flush() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
    let _keep =
        eval.frame_manager_mut()
            .create_frame_on_terminal("F1", TERMINAL_ID, 80, 25, scratch);
    let terminal = ensure_terminal_runtime_owner(1, "secondary", TerminalRuntimeConfig::inactive());
    let doomed = eval
        .frame_manager_mut()
        .create_frame_on_terminal("F2", 1, 80, 25, scratch);
    eval.eval_str(
        r#"
(setq hook-log nil)
(setq delete-terminal-functions
      (list (lambda (term)
              (setq hook-log
                    (cons (list 'terminal (terminal-live-p term)) hook-log)))))
(setq delete-frame-functions
      (list (lambda (frame)
              (setq hook-log
                    (cons (list 'before (frame-live-p frame)) hook-log)))))
(setq after-delete-frame-functions
      (list (lambda (frame)
              (setq hook-log
                    (cons (list 'after (frame-live-p frame)) hook-log)))))
"#,
    )
    .expect("install hook setup");

    assert_eq!(
        builtin_delete_terminal(&mut eval, vec![terminal, Value::T]).unwrap(),
        Value::NIL
    );
    assert!(
        eval.frames.get(doomed).is_none(),
        "delete-terminal should remove frames on that terminal immediately"
    );
    assert_eq!(
        eval.eval_str("hook-log")
            .expect("hook-log after delete-terminal"),
        Value::list(vec![Value::list(vec![Value::symbol("terminal"), Value::T])])
    );

    eval.flush_pending_safe_funcalls();

    let post_flush = eval
        .eval_str("(nreverse hook-log)")
        .expect("hook-log after flush");
    assert_eq!(
        format!("{}", post_flush),
        "((terminal t) (after nil) (before nil))"
    );
}

#[test]
fn delete_terminal_force_invokes_terminal_host_delete_hook() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_string()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let log = Rc::new(RefCell::new(Vec::new()));
    set_terminal_host(Box::new(RecordingTerminalHost {
        log: Rc::clone(&log),
    }));

    let mut eval = Context::new();
    assert_eq!(
        builtin_delete_terminal(&mut eval, vec![Value::NIL, Value::T]).unwrap(),
        Value::NIL
    );
    assert_eq!(log.borrow().as_slice(), &["delete"]);
}

#[test]
fn delete_terminal_noelisp_bypasses_sole_terminal_check_and_defers_hooks() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
    eval.buffer_manager_mut().set_current(scratch);
    let _frame =
        eval.frame_manager_mut()
            .create_frame_on_terminal("F1", TERMINAL_ID, 80, 25, scratch);
    eval.eval_str(
        r#"
(setq hook-log nil)
(setq delete-terminal-functions
      (list (lambda (term)
              (setq hook-log
                    (cons (list 'terminal (terminal-live-p term)) hook-log)))))
(setq delete-frame-functions
      (list (lambda (frame)
              (setq hook-log
                    (cons (list 'before (frame-live-p frame)) hook-log)))))
(setq after-delete-frame-functions
      (list (lambda (frame)
              (setq hook-log
                    (cons (list 'after (frame-live-p frame)) hook-log)))))
"#,
    )
    .expect("install hook setup");

    assert_eq!(
        delete_terminal_noelisp_owned(&mut eval, TERMINAL_ID).unwrap(),
        Value::NIL
    );
    assert!(eval.frame_manager().frame_list().is_empty());
    assert!(
        builtin_terminal_live_p(&mut eval, vec![terminal_handle_value()])
            .unwrap()
            .is_nil(),
        "noelisp delete should mark the terminal dead even when it is the sole terminal"
    );
    assert_eq!(
        eval.eval_str("hook-log").expect("hook-log before flush"),
        Value::NIL
    );

    eval.flush_pending_safe_funcalls();

    let post_flush = eval
        .eval_str("(nreverse hook-log)")
        .expect("hook-log after flush");
    assert_eq!(
        format!("{}", post_flush),
        "((after nil) (before nil) (terminal nil))"
    );
}

#[test]
fn delete_terminal_noelisp_ignores_host_delete_failures() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_string()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    set_terminal_host(Box::new(FailingDeleteTerminalHost));

    let mut eval = Context::new();
    assert_eq!(
        delete_terminal_noelisp_owned(&mut eval, TERMINAL_ID).unwrap(),
        Value::NIL
    );
    assert!(
        builtin_terminal_live_p(&mut eval, vec![terminal_handle_value()])
            .unwrap()
            .is_nil(),
        "noelisp delete should finish even if the host is already gone"
    );
}

#[test]
fn make_terminal_frame_is_eval_backed_frame_creation() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(scratch);
    super::pure::mark_selected_terminal_usable_for_test(&eval);

    let frame = crate::emacs_core::frame::builtin_make_terminal_frame(&mut eval, vec![Value::NIL])
        .expect("make-terminal-frame");

    assert!(frame.as_frame_id().is_some());
    assert_eq!(eval.frames.frame_list().len(), 1);
}

#[test]
fn make_terminal_frame_opens_and_owns_an_explicit_secondary_tty() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(TerminalRuntimeConfig::window_system().with_name(":0"));
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    let scratch = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(scratch);
    let requests = Rc::new(RefCell::new(Vec::new()));
    let lifecycle = Rc::new(RefCell::new(Vec::new()));
    eval.set_tty_frame_host_factory(Box::new(RecordingTtyFrameHostFactory {
        requests: Rc::clone(&requests),
        lifecycle: Rc::clone(&lifecycle),
    }));

    let params = Value::list(vec![
        Value::cons(Value::symbol("tty"), Value::string("/dev/pts/42")),
        Value::cons(Value::symbol("tty-type"), Value::string("xterm-256color")),
    ]);
    let frame = crate::emacs_core::frame::builtin_make_terminal_frame(&mut eval, vec![params])
        .expect("make-terminal-frame should open the requested tty");
    let frame_id = crate::window::FrameId(frame.as_frame_id().expect("frame id"));
    let terminal_id = eval.frames.get(frame_id).expect("frame").terminal_id;

    assert_ne!(
        terminal_id, TERMINAL_ID,
        "the GUI terminal must remain distinct"
    );
    let opened_frame = eval.frames.get(frame_id).expect("frame");
    assert_eq!(opened_frame.width, 132);
    assert_eq!(opened_frame.height, 43);
    assert_eq!(opened_frame.char_width, 1.0);
    assert_eq!(opened_frame.char_height, 1.0);
    assert!(opened_frame.displays_chrome);
    assert_eq!(
        opened_frame
            .minibuffer_leaf
            .as_ref()
            .expect("terminal minibuffer")
            .bounds()
            .height,
        1.0
    );
    let request = requests.borrow();
    assert_eq!(request.len(), 1);
    assert_eq!(request[0].terminal_id(), terminal_id);
    assert_eq!(request[0].frame_id(), frame_id);
    assert_eq!(request[0].device(), "/dev/pts/42");
    assert_eq!(request[0].terminal_type(), "xterm-256color");

    let terminal = builtin_frame_terminal(&mut eval, vec![frame]).expect("frame-terminal");
    assert_eq!(
        builtin_terminal_name(&mut eval, vec![terminal]).expect("terminal-name"),
        Value::string("/dev/pts/42")
    );
    assert_eq!(
        builtin_tty_type(&mut eval, vec![terminal]).expect("tty-type"),
        Value::string("xterm-256color")
    );
    builtin_suspend_tty(&mut eval, vec![terminal]).expect("suspend secondary tty");
    builtin_resume_tty(&mut eval, vec![terminal]).expect("resume secondary tty");
    builtin_delete_terminal(&mut eval, vec![terminal, Value::T]).expect("delete secondary tty");
    assert_eq!(&*lifecycle.borrow(), &["suspend", "resume", "delete"]);
}

#[test]
fn make_terminal_frame_without_a_device_reuses_the_selected_secondary_terminal() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(TerminalRuntimeConfig::window_system().with_name(":0"));
    let mut eval = Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(scratch);
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_tty_frame_host_factory(Box::new(RecordingTtyFrameHostFactory {
        requests: Rc::clone(&requests),
        lifecycle: Rc::new(RefCell::new(Vec::new())),
    }));
    let params = Value::list(vec![
        Value::cons(Value::symbol("tty"), Value::string("/dev/pts/42")),
        Value::cons(Value::symbol("tty-type"), Value::string("xterm-256color")),
    ]);
    let first = crate::emacs_core::frame::builtin_make_terminal_frame(&mut eval, vec![params])
        .expect("open secondary terminal");
    let first_id = crate::window::FrameId(first.as_frame_id().expect("first frame id"));
    let terminal_id = eval.frames.get(first_id).expect("first frame").terminal_id;

    let second = crate::emacs_core::frame::builtin_make_terminal_frame(&mut eval, vec![Value::NIL])
        .expect("reuse selected secondary terminal");
    let second_id = crate::window::FrameId(second.as_frame_id().expect("second frame id"));
    let second_frame = eval.frames.get(second_id).expect("second frame");
    assert_eq!(second_frame.terminal_id, terminal_id);
    assert_eq!((second_frame.width, second_frame.height), (132, 43));
    assert_eq!(
        requests.borrow().len(),
        1,
        "reusing a terminal must not acquire a second OS host"
    );
}

#[test]
fn make_terminal_frame_reuses_an_active_tty_already_open_on_the_named_device() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(TerminalRuntimeConfig::window_system().with_name(":0"));
    let mut eval = Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(scratch);
    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_tty_frame_host_factory(Box::new(RecordingTtyFrameHostFactory {
        requests: Rc::clone(&requests),
        lifecycle: Rc::new(RefCell::new(Vec::new())),
    }));
    let params = || {
        Value::list(vec![
            Value::cons(Value::symbol("tty"), Value::string("/dev/pts/42")),
            Value::cons(Value::symbol("tty-type"), Value::string("xterm-256color")),
        ])
    };
    let first = crate::emacs_core::frame::builtin_make_terminal_frame(&mut eval, vec![params()])
        .expect("open named terminal");
    let second = crate::emacs_core::frame::builtin_make_terminal_frame(&mut eval, vec![params()])
        .expect("reuse named terminal");
    let first_id = crate::window::FrameId(first.as_frame_id().expect("first frame id"));
    let second_id = crate::window::FrameId(second.as_frame_id().expect("second frame id"));

    assert_eq!(
        eval.frames.get(first_id).expect("first frame").terminal_id,
        eval.frames
            .get(second_id)
            .expect("second frame")
            .terminal_id
    );
    assert_eq!(
        requests.borrow().len(),
        1,
        "GNU init_tty reuses an active terminal with the same device name"
    );
}

#[test]
fn make_terminal_frame_rolls_back_the_provisional_frame_when_tty_open_fails() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(TerminalRuntimeConfig::window_system().with_name(":0"));
    let mut eval = Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(scratch);
    let original_frame = crate::emacs_core::window_cmds::make_frame_plain(
        &mut eval.frames,
        &mut eval.buffers,
        vec![Value::NIL],
    )
    .expect("initial frame");
    let original_frame_id = original_frame.as_frame_id().expect("frame id");
    eval.set_tty_frame_host_factory(Box::new(FailingTtyFrameHostFactory));

    let params = Value::list(vec![
        Value::cons(Value::symbol("tty"), Value::string("/dev/pts/404")),
        Value::cons(Value::symbol("tty-type"), Value::string("xterm-256color")),
    ]);
    let error = crate::emacs_core::frame::builtin_make_terminal_frame(&mut eval, vec![params])
        .expect_err("failed device open must fail frame creation");

    match error {
        Flow::Signal(signal) => {
            assert_eq!(signal.symbol_name(), "error");
            assert_eq!(signal.data, vec![Value::string("test TTY open failure")]);
        }
        other => panic!("expected TTY open error signal, got {other:?}"),
    }
    assert_eq!(
        eval.frames.frame_list(),
        vec![crate::window::FrameId(original_frame_id)],
        "a failed host acquisition must not leak the provisional frame"
    );
    assert_eq!(
        eval.frames.selected_frame().map(|frame| frame.id),
        Some(crate::window::FrameId(original_frame_id))
    );
    assert_eq!(
        crate::emacs_core::value::list_to_vec(
            &builtin_terminal_list(vec![]).expect("terminal-list")
        )
        .expect("terminal list")
        .len(),
        1,
        "a failed host acquisition must not publish a terminal"
    );
}

#[test]
fn selected_terminal_returns_live_handle() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let handle = builtin_selected_terminal(vec![]).unwrap();
    let live = builtin_terminal_live_p(&mut eval, vec![handle]).unwrap();
    assert_eq!(live, Value::T);
}

#[test]
fn frame_terminal_returns_live_handle() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let handle = builtin_frame_terminal(&mut eval, vec![Value::NIL]).unwrap();
    let live = builtin_terminal_live_p(&mut eval, vec![handle]).unwrap();
    assert_eq!(live, Value::T);
}

// ---------------------------------------------------------------------------
// `frame-initial-p` takes a TERMINAL as well as a FRAME (ledger 160)
// ---------------------------------------------------------------------------
//
// GNU's `Fframe_initial_p` (src/terminal.c:482-500) admits either shape:
//
//     if (NILP (frame))    frame = selected_frame;
//     if (FRAMEP (frame))  return FRAME_LIVE_P (f) && FRAME_INITIAL_P (f);
//     struct terminal *t = decode_terminal (frame);
//     return t && t->type == output_initial ? Qt : Qnil;
//
// and `decode_terminal` (src/terminal.c:223-233) never signals: it answers NULL
// for anything that is neither a terminal nor a frame, and for a terminal whose
// `name` has been freed by `delete_terminal`, so the subr answers nil for junk.
//
// The terminal branch is not decoration.  `turn-on-xterm-mouse-tracking-on-terminal`
// (lisp/xt-mouse.el:508-512) passes a TERMINAL deliberately, to skip "the
// initial terminal which is not a termcap device", and `xterm--init`
// (lisp/term/xterm.el:1035-1044) calls `(xterm-mouse-mode 1)` on every terminal
// whose TERM matches `xterm--auto-xt-mouse-allowed-types` -- alacritty, contour.
// A signal there aborts `command-line-1`, so `-l` and `--eval` never run.

/// Measured in GNU 31.0.90 `--batch -Q`:
/// `(frame-initial-p (car (terminal-list))) => t`.
#[test]
fn frame_initial_p_answers_t_for_the_bootstrap_terminal() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let terminal = builtin_frame_terminal(&mut eval, vec![Value::NIL]).unwrap();
    assert_eq!(frame_initial_p(&mut eval, terminal), Ok(Value::T));
}

/// Measured in GNU 31.0.90, `emacs -nw -Q` on a pty with TERM=alacritty:
/// `(frame-initial-p (frame-terminal)) => nil`.  GNU is answering about an
/// `output_termcap` terminal `init_tty` created; its initial terminal is gone by
/// then.  We reuse terminal record 0 for the tty rather than allocating a second
/// one, so the answer has to come from the terminal's OUTPUT METHOD -- not from
/// its id (ours is 0, GNU's is 1) and not from its name.
#[test]
fn frame_initial_p_answers_nil_for_a_live_tty_terminal() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    configure_terminal_runtime(
        TerminalRuntimeConfig::interactive(Some("alacritty".to_string()), neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256))
            .with_name("/dev/tty"),
    );
    let mut eval = Context::new();
    let terminal = builtin_frame_terminal(&mut eval, vec![Value::NIL]).unwrap();
    assert_eq!(frame_initial_p(&mut eval, terminal), Ok(Value::NIL));
}

/// `decode_terminal` answers NULL, so GNU answers nil.  Measured in GNU 31.0.90
/// `--batch -Q`: `"junk"`, `'sym` and `42` each answer nil, none signal.
#[test]
fn frame_initial_p_answers_nil_for_a_non_designator() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    for junk in [
        Value::string("junk"),
        Value::symbol("sym"),
        Value::fixnum(42),
    ] {
        assert_eq!(
            frame_initial_p(&mut eval, junk),
            Ok(Value::NIL),
            "frame-initial-p should answer nil for {junk:?}, as GNU's decode_terminal does"
        );
    }
}

/// A deleted terminal has no `name` in GNU, so `decode_terminal` answers NULL
/// and `frame-initial-p` answers nil instead of t.
#[test]
fn frame_initial_p_answers_nil_for_a_deleted_terminal() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    // After the Context, which re-mints terminal handles.
    let secondary =
        ensure_terminal_runtime_owner(1, "secondary", TerminalRuntimeConfig::inactive());
    assert_eq!(frame_initial_p(&mut eval, secondary), Ok(Value::T));
    builtin_delete_terminal(&mut eval, vec![secondary, Value::T]).expect("delete-terminal");
    assert_eq!(frame_initial_p(&mut eval, secondary), Ok(Value::NIL));
}

/// The FRAME branch keeps GNU's `FRAME_LIVE_P (f) && FRAME_INITIAL_P (f)`.
#[test]
fn frame_initial_p_still_answers_for_frames() {
    crate::test_utils::init_test_tracing();
    reset_terminal_thread_locals();
    let mut eval = Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("*scratch*");
    let real = eval.frame_manager_mut().create_frame("F1", 80, 25, buffer);
    assert_eq!(
        frame_initial_p(&mut eval, Value::make_frame(real.0)),
        Ok(Value::NIL)
    );
    eval.frame_manager_mut()
        .get_mut(real)
        .expect("frame")
        .initial = true;
    assert_eq!(
        frame_initial_p(&mut eval, Value::make_frame(real.0)),
        Ok(Value::T)
    );
}

/// `(frame-initial-p DESIGNATOR)`, with a signal rendered as text so a test can
/// state the ANSWER it expects and report the raise it got instead.
fn frame_initial_p(eval: &mut Context, designator: Value) -> Result<Value, String> {
    builtin_frame_initial_p(eval, vec![designator]).map_err(|flow| format!("{flow:?}"))
}

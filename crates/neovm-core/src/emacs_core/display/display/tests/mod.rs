use super::*;
use crate::EvalError;
use crate::emacs_core::dispnew::pure::{
    builtin_internal_show_cursor, builtin_internal_show_cursor_p, builtin_open_termscript,
    builtin_redraw_frame, builtin_send_string_to_terminal, reset_dispnew_thread_locals,
};
use crate::emacs_core::eval::{DisplayHost, GuiFrameHostRequest, PopupMenuRequest};
use crate::emacs_core::intern::resolve_sym;
use crate::emacs_core::terminal::pure::{
    builtin_controlling_tty_p, builtin_frame_terminal, builtin_resume_tty,
    builtin_selected_terminal, builtin_set_terminal_parameter, builtin_suspend_tty,
    builtin_terminal_list, builtin_terminal_live_p, builtin_terminal_name,
    builtin_terminal_parameter, builtin_terminal_parameters, builtin_tty_top_frame,
    builtin_tty_type, reset_terminal_thread_locals, terminal_handle_value,
};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

fn builtin_x_popup_menu(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    super::builtin_x_popup_menu_batch(eval, args)
}

struct ImageCapableDisplayHost;

impl DisplayHost for ImageCapableDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }
}

struct ItalicCapableDisplayHost;

impl DisplayHost for ItalicCapableDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_frame_font(
        &mut self,
        _frame_id: crate::window::FrameId,
        request: crate::emacs_core::display_host::FrameFontRequest,
    ) -> Result<Option<crate::emacs_core::eval::ResolvedFrameFont>, String> {
        let face = request.face();
        let metrics = crate::emacs_core::eval::FontPxProbeResult {
            pixel_size: 14,
            height: 16,
            ascent: 12,
            descent: 4,
            max_width: 8,
            space_width: 8,
            average_width: 8,
        };
        Ok(Some(crate::emacs_core::eval::ResolvedFrameFont {
            font: crate::emacs_core::eval::test_resolved_opened_font(
                "Noto Sans",
                None,
                None,
                face.weight.unwrap_or(crate::face::FontWeight::NORMAL),
                face.slant.unwrap_or(crate::face::FontSlant::Normal),
                face.width.unwrap_or(crate::face::FontWidth::Normal),
                None,
                metrics,
                None,
            ),
            height_tenths: 100,
        }))
    }
}

struct FailingClipboardDisplayHost;

impl DisplayHost for FailingClipboardDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn set_clipboard_text(&mut self, _text: Option<&str>) -> Result<(), String> {
        Err("system clipboard unavailable".to_owned())
    }

    fn clipboard_text(&mut self) -> Result<Option<String>, String> {
        Err("system clipboard unavailable".to_owned())
    }

    fn set_primary_selection_text(&mut self, _text: Option<&str>) -> Result<(), String> {
        Err("system clipboard unavailable".to_owned())
    }

    fn primary_selection_text(&mut self) -> Result<Option<String>, String> {
        Err("system clipboard unavailable".to_owned())
    }
}

#[derive(Clone, Default)]
struct RecordingPopupHost {
    shown: Arc<Mutex<Vec<PopupMenuRequest>>>,
    hidden: Arc<Mutex<usize>>,
}

impl DisplayHost for RecordingPopupHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn show_popup_menu(&mut self, menu: PopupMenuRequest) -> Result<(), String> {
        self.shown.lock().unwrap().push(menu);
        Ok(())
    }

    fn hide_popup_menu(&mut self) -> Result<(), String> {
        *self.hidden.lock().unwrap() += 1;
        Ok(())
    }
}

fn clear_terminal_parameters() {
    reset_terminal_thread_locals();
}

#[test]
fn raw_context_does_not_prebind_x_selection_aliases() {
    crate::test_utils::init_test_tracing();
    let eval = crate::emacs_core::Context::new();
    for name in [
        "x-select-text",
        "x-selection-value",
        "x-get-selection-value",
        "x-get-selection",
        "x-set-selection",
    ] {
        assert!(
            eval.obarray.symbol_function(name).is_none(),
            "{name} should come from GNU select.el, not Context::new",
        );
    }
}

#[test]
fn gui_clipboard_errors_are_visible_instead_of_returning_cached_text() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    eval.eval_str(r#"(neomacs-clipboard-set "stale")"#)
        .expect("headless clipboard cache should remain available");
    eval.set_display_host(Box::new(FailingClipboardDisplayHost));

    for form in [
        r#"(neomacs-clipboard-set "new")"#,
        "(neomacs-clipboard-get)",
        r#"(neomacs-primary-selection-set "new")"#,
        "(neomacs-primary-selection-get)",
    ] {
        let err = eval
            .eval_str(form)
            .expect_err("GUI clipboard backend failures must reach Lisp");
        let EvalError::Signal { symbol, data, .. } = err else {
            panic!("clipboard backend failure should signal an error");
        };
        assert_eq!(resolve_sym(symbol), "error");
        assert_eq!(data, vec![Value::string("system clipboard unavailable")]);
    }
}

#[test]
fn gnu_select_el_defines_x_selection_aliases() {
    crate::test_utils::init_test_tracing();
    let source =
        fs::read_to_string(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("lisp/select.el"))
            .expect("read select.el");
    assert!(
        source
            .contains("(define-obsolete-function-alias 'x-select-text 'gui-select-text \"25.1\")"),
        "GNU select.el should own the x-select-text alias",
    );
    assert!(
        source.contains(
            "(define-obsolete-function-alias 'x-selection-value 'gui-selection-value \"25.1\")",
        ),
        "GNU select.el should own the x-selection-value alias",
    );
    assert!(
        source.contains(
            "(define-obsolete-function-alias 'x-get-selection-value\n  'gui-get-primary-selection \"25.1\")",
        ),
        "GNU select.el should own the x-get-selection-value alias",
    );
    assert!(
        source.contains(
            "(define-obsolete-function-alias 'x-get-selection 'gui-get-selection \"25.1\")"
        ),
        "GNU select.el should own the x-get-selection alias",
    );
    assert!(
        source.contains(
            "(define-obsolete-function-alias 'x-set-selection 'gui-set-selection \"25.1\")"
        ),
        "GNU select.el should own the x-set-selection alias",
    );
}

#[test]
fn x_window_system_active_falls_back_to_window_system_when_initial_is_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    eval.set_variable("initial-window-system", Value::NIL);
    eval.set_variable("window-system", Value::symbol(gui_window_system_symbol()));

    assert!(x_window_system_active(&eval));
    assert!(x_window_system_active_in_state(&eval.obarray, &[]));
}

#[test]
fn terminal_parameter_exposes_oracle_defaults() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    // normal-erase-is-backspace has NO default: GNU leaves it unset until
    // normal-erase-is-backspace-setup-frame stores 0/1 during command-line,
    // and a fabricated 0 vetoed that decision (DIVERGENCES.md entry 67).
    let normal = builtin_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::symbol("normal-erase-is-backspace")],
    )
    .unwrap();
    assert!(normal.is_nil());

    let keyboard = builtin_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::symbol("keyboard-coding-saved-meta-mode")],
    )
    .unwrap();
    assert_eq!(keyboard, Value::list(vec![Value::T]));

    let missing =
        builtin_terminal_parameter(&mut eval, vec![Value::NIL, Value::symbol("neovm-param")])
            .unwrap();
    assert!(missing.is_nil());
}

#[test]
fn terminal_parameter_round_trips() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    let set_result = builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::symbol("neovm-param"), Value::fixnum(42)],
    )
    .unwrap();
    assert!(set_result.is_nil());

    let get_result =
        builtin_terminal_parameter(&mut eval, vec![Value::NIL, Value::symbol("neovm-param")])
            .unwrap();
    assert_eq!(get_result, Value::fixnum(42));
}

#[test]
fn set_terminal_parameter_returns_previous_default_values() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    // No fabricated default for normal-erase-is-backspace: the previous
    // value of a never-set parameter is nil, as in GNU (entry 67).
    let previous_normal = builtin_set_terminal_parameter(
        &mut eval,
        vec![
            Value::NIL,
            Value::symbol("normal-erase-is-backspace"),
            Value::fixnum(9),
        ],
    )
    .unwrap();
    assert!(previous_normal.is_nil());

    let previous_keyboard = builtin_set_terminal_parameter(
        &mut eval,
        vec![
            Value::NIL,
            Value::symbol("keyboard-coding-saved-meta-mode"),
            Value::NIL,
        ],
    )
    .unwrap();
    assert_eq!(previous_keyboard, Value::list(vec![Value::T]));
}

#[test]
fn terminal_parameter_distinct_keys_do_not_alias() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::symbol("k1"), Value::fixnum(1)],
    )
    .unwrap();
    builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::symbol("k2"), Value::fixnum(2)],
    )
    .unwrap();

    let first =
        builtin_terminal_parameter(&mut eval, vec![Value::NIL, Value::symbol("k1")]).unwrap();
    let second =
        builtin_terminal_parameter(&mut eval, vec![Value::NIL, Value::symbol("k2")]).unwrap();
    assert_eq!(first, Value::fixnum(1));
    assert_eq!(second, Value::fixnum(2));
}

#[test]
fn terminal_parameter_rejects_non_symbol_key() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_terminal_parameter(&mut eval, vec![Value::NIL, Value::string("k")]);
    assert!(result.is_err());
}

#[test]
fn set_terminal_parameter_ignores_non_symbol_key() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    let set_result = builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::string("k"), Value::fixnum(9)],
    )
    .unwrap();
    assert!(set_result.is_nil());

    let second_result = builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::string("k"), Value::fixnum(1)],
    )
    .unwrap();
    assert!(second_result.is_nil());

    let get_result =
        builtin_terminal_parameter(&mut eval, vec![Value::NIL, Value::symbol("k")]).unwrap();
    assert!(get_result.is_nil());
}

#[test]
fn set_terminal_parameter_returns_previous_for_repeat_non_symbol_key() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    let first = builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::fixnum(1), Value::fixnum(9)],
    )
    .unwrap();
    assert!(first.is_nil());

    let second = builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::fixnum(1), Value::fixnum(1)],
    )
    .unwrap();
    assert_eq!(second, Value::fixnum(9));
}

#[test]
fn terminal_parameter_rejects_non_terminal_designator() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_terminal_parameter(&mut eval, vec![Value::fixnum(1), Value::symbol("k")]);
    assert!(result.is_err());
}

#[test]
fn terminal_parameters_lists_mutated_symbol_entries() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    let _ = builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::symbol("k1"), Value::fixnum(1)],
    )
    .unwrap();
    let _ = builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::NIL, Value::symbol("k2"), Value::fixnum(2)],
    )
    .unwrap();

    let params = builtin_terminal_parameters(&mut eval, vec![Value::NIL]).unwrap();
    let entries = list_to_vec(&params).expect("parameter alist");
    assert!(entries.len() >= 3);
    // normal-erase-is-backspace must NOT be listed until Lisp stores it
    // (entry 67): GNU's alist has no entry for a never-set parameter.
    assert!(
        !entries.iter().any(|entry| entry.is_cons()
            && entry.cons_car() == Value::symbol("normal-erase-is-backspace"))
    );
    assert!(entries.iter().any(|entry| entry.is_cons() && {
        entry.cons_car() == Value::symbol("keyboard-coding-saved-meta-mode")
            && entry.cons_cdr() == Value::list(vec![Value::T])
    }));
    assert!(entries.iter().any(|entry| entry.is_cons() && {
        entry.cons_car() == Value::symbol("k1") && entry.cons_cdr() == Value::fixnum(1)
    }));
    assert!(entries.iter().any(|entry| entry.is_cons() && {
        entry.cons_car() == Value::symbol("k2") && entry.cons_cdr() == Value::fixnum(2)
    }));

    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    let via_frame = builtin_terminal_parameters(&mut eval, vec![Value::make_frame(frame_id)])
        .expect("eval terminal-parameters");
    let eval_entries = list_to_vec(&via_frame).expect("parameter alist");
    assert!(eval_entries.len() >= 3);
}

#[test]
fn set_terminal_parameter_rejects_non_terminal_designator() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_set_terminal_parameter(
        &mut eval,
        vec![Value::fixnum(1), Value::symbol("k"), Value::fixnum(1)],
    );
    assert!(result.is_err());
}

#[test]
fn eval_terminal_parameter_accepts_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    clear_terminal_parameters();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    builtin_set_terminal_parameter(
        &mut eval,
        vec![
            Value::make_frame(frame_id),
            Value::symbol("neovm-frame-param"),
            Value::fixnum(7),
        ],
    )
    .unwrap();
    let value = builtin_terminal_parameter(
        &mut eval,
        vec![
            Value::make_frame(frame_id),
            Value::symbol("neovm-frame-param"),
        ],
    )
    .unwrap();
    assert_eq!(value, Value::fixnum(7));
}

#[test]
fn terminal_live_p_reflects_designator_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let live_nil = builtin_terminal_live_p(&mut eval, vec![Value::NIL]).unwrap();
    let live_handle = builtin_terminal_live_p(&mut eval, vec![terminal_handle_value()]).unwrap();
    let live_string =
        builtin_terminal_live_p(&mut eval, vec![Value::string("initial_terminal")]).unwrap();
    let live_int = builtin_terminal_live_p(&mut eval, vec![Value::fixnum(1)]).unwrap();
    assert_eq!(live_nil, Value::T);
    assert_eq!(live_handle, Value::T);
    assert!(live_string.is_nil());
    assert!(live_int.is_nil());
}

#[test]
fn eval_terminal_live_p_accepts_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    let live = builtin_terminal_live_p(&mut eval, vec![Value::make_frame(frame_id)]).unwrap();
    assert_eq!(live, Value::T);

    let stale = builtin_terminal_live_p(&mut eval, vec![Value::fixnum(999_999)]).unwrap();
    assert!(stale.is_nil());
}

#[test]
fn terminal_name_rejects_invalid_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_terminal_name(&mut eval, vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn eval_terminal_name_accepts_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    let result = builtin_terminal_name(&mut eval, vec![Value::make_frame(frame_id)]).unwrap();
    assert_eq!(result, Value::string("initial_terminal"));
}

#[test]
fn frame_terminal_rejects_non_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_frame_terminal(&mut eval, vec![Value::string("not-a-frame")]);
    assert!(result.is_err());
}

#[test]
fn frame_terminal_rejects_non_frame_arg_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU `frame-terminal` only accepts nil or a live frame; integer
    // arguments signal `wrong-type-argument frame-live-p`. Mirror that.
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_frame_terminal(&mut eval, vec![Value::fixnum(1)]);
    match result {
        Err(crate::emacs_core::error::Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data[0], Value::symbol("frame-live-p"));
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn frame_terminal_returns_live_terminal_handle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let handle = builtin_frame_terminal(&mut eval, vec![Value::NIL]).unwrap();
    let live = builtin_terminal_live_p(&mut eval, vec![handle]).unwrap();
    assert_eq!(live, Value::T);
    assert_eq!(handle.type_name(), "terminal");
}

#[test]
fn selected_terminal_returns_live_terminal_handle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let handle = builtin_selected_terminal(vec![]).unwrap();
    let live = builtin_terminal_live_p(&mut eval, vec![handle]).unwrap();
    assert_eq!(live, Value::T);
}

#[test]
fn frame_terminal_and_terminal_list_are_terminal_objects() {
    crate::test_utils::init_test_tracing();
    crate::emacs_core::terminal::pure::reset_terminal_thread_locals();
    let mut eval = crate::emacs_core::Context::new();

    let terminal = builtin_frame_terminal(&mut eval, vec![Value::NIL]).unwrap();
    let terminals = builtin_terminal_list(vec![]).unwrap();
    let listed = crate::emacs_core::value::list_to_vec(&terminals).unwrap();

    assert_eq!(terminal.type_name(), "terminal");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].type_name(), "terminal");
    assert!(crate::emacs_core::value::eq_value(&terminal, &listed[0]));
}

#[test]
fn selected_terminal_arity() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_selected_terminal(vec![Value::NIL]).is_err());
}

#[test]
fn eval_frame_terminal_accepts_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    let handle = builtin_frame_terminal(&mut eval, vec![Value::make_frame(frame_id)]).unwrap();
    let live = builtin_terminal_live_p(&mut eval, vec![handle]).unwrap();
    assert_eq!(live, Value::T);
}

#[test]
fn redraw_frame_rejects_non_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::emacs_core::Context::new();
    let result = builtin_redraw_frame(&mut ctx, vec![Value::string("not-a-frame")]);
    assert!(result.is_err());
}

#[test]
fn eval_redraw_frame_accepts_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    let result = builtin_redraw_frame(&mut eval, vec![Value::make_frame(frame_id)]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn frame_edges_string_designator_uses_unquoted_live_frame_error_message() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_frame_edges(&mut eval, vec![Value::string("x")]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("x is not a live frame")]);
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn eval_frame_edges_numeric_designator_reports_numeric_message() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_frame_edges(&mut eval, vec![Value::fixnum(999_999)]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("999999 is not a live frame")]);
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn eval_frame_edges_live_window_designator_includes_buffer_context() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let _ = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let window =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();
    let result = builtin_frame_edges(&mut eval, vec![window]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            let message = match sig.data.as_slice() {
                [val] => val
                    .as_utf8_str()
                    .expect("expected string payload")
                    .to_string(),
                other => panic!("expected single error message payload, got {other:?}"),
            };
            assert!(message.starts_with("#<window "));
            assert!(message.contains(" on "));
            assert!(message.ends_with(" is not a live frame"));
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn open_termscript_uses_batch_tty_error_payload() {
    crate::test_utils::init_test_tracing();
    let result = builtin_open_termscript(vec![Value::NIL]);
    match result {
        Err(Flow::Signal(sig)) => {
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
fn send_string_to_terminal_rejects_invalid_terminal_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result =
        builtin_send_string_to_terminal(&mut eval, vec![Value::string(""), Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn send_string_to_terminal_accepts_live_terminal_handle() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let handle = terminal_handle_value();
    let result =
        builtin_send_string_to_terminal(&mut eval, vec![Value::string(""), handle]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn eval_send_string_to_terminal_accepts_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    let result = builtin_send_string_to_terminal(
        &mut eval,
        vec![Value::string(""), Value::make_frame(frame_id)],
    )
    .unwrap();
    assert!(result.is_nil());
}

#[test]
fn internal_show_cursor_tracks_visibility_state() {
    crate::test_utils::init_test_tracing();
    reset_dispnew_thread_locals();
    let mut eval = crate::emacs_core::Context::new();
    let default_visible = builtin_internal_show_cursor_p(&mut eval, vec![]).unwrap();
    assert_eq!(default_visible, Value::T);

    builtin_internal_show_cursor(&mut eval, vec![Value::NIL, Value::NIL]).unwrap();
    let hidden = builtin_internal_show_cursor_p(&mut eval, vec![]).unwrap();
    assert!(hidden.is_nil());

    builtin_internal_show_cursor(&mut eval, vec![Value::NIL, Value::T]).unwrap();
    let visible = builtin_internal_show_cursor_p(&mut eval, vec![]).unwrap();
    assert_eq!(visible, Value::T);
}

#[test]
fn internal_show_cursor_rejects_non_window_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let result = builtin_internal_show_cursor(&mut eval, vec![Value::fixnum(1), Value::NIL]);
    assert!(result.is_err());
}

#[test]
fn eval_internal_show_cursor_accepts_live_window_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let _ = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let window =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();
    let result = builtin_internal_show_cursor(&mut eval, vec![window, Value::T]).unwrap();
    assert!(result.is_nil());
}

#[test]
fn eval_internal_show_cursor_p_accepts_live_window_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let _ = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let window =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();
    let result = builtin_internal_show_cursor_p(&mut eval, vec![window]).unwrap();
    assert!((result.is_t() || result.is_nil()));
}

#[test]
fn eval_internal_show_cursor_tracks_per_window_state() {
    crate::test_utils::init_test_tracing();
    reset_dispnew_thread_locals();
    let mut eval = crate::emacs_core::Context::new();
    let _ = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let selected =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();
    let other = crate::emacs_core::builtins::dispatch_builtin(
        &mut eval,
        "split-window-internal",
        vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL],
    )
    .unwrap()
    .unwrap();

    assert_eq!(
        builtin_internal_show_cursor_p(&mut eval, vec![selected]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_internal_show_cursor_p(&mut eval, vec![other]).unwrap(),
        Value::T
    );

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
    assert!(
        builtin_internal_show_cursor_p(&mut eval, vec![])
            .unwrap()
            .is_nil()
    );

    builtin_internal_show_cursor(&mut eval, vec![other, Value::T]).unwrap();
    assert!(
        builtin_internal_show_cursor_p(&mut eval, vec![selected])
            .unwrap()
            .is_nil()
    );
    assert_eq!(
        builtin_internal_show_cursor_p(&mut eval, vec![other]).unwrap(),
        Value::T
    );
    assert!(
        builtin_internal_show_cursor_p(&mut eval, vec![])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn tty_queries_reject_invalid_terminal_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let tty_type = builtin_tty_type(&mut eval, vec![Value::fixnum(1)]);
    let tty_top_frame = builtin_tty_top_frame(&mut eval, vec![Value::fixnum(1)]);
    let controlling = builtin_controlling_tty_p(&mut eval, vec![Value::fixnum(1)]);
    assert!(tty_type.is_err());
    assert!(tty_top_frame.is_err());
    assert!(controlling.is_err());
}

#[test]
fn eval_tty_queries_accept_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    assert!(
        builtin_tty_type(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_tty_top_frame(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_controlling_tty_p(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn suspend_tty_signals_non_text_terminal_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    for args in [vec![], vec![Value::NIL], vec![terminal_handle_value()]] {
        let result = builtin_suspend_tty(&mut eval, args);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string(
                        "Attempt to suspend a non-text terminal device"
                    )]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }
}

#[test]
fn eval_suspend_resume_accept_live_frame_and_signal_non_text_terminal_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    let suspend = builtin_suspend_tty(&mut eval, vec![Value::make_frame(frame_id)]);
    let resume = builtin_resume_tty(&mut eval, vec![Value::make_frame(frame_id)]);
    assert!(suspend.is_err());
    assert!(resume.is_err());
}

#[test]
fn resume_tty_signals_non_text_terminal_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    for args in [vec![], vec![Value::NIL], vec![terminal_handle_value()]] {
        let result = builtin_resume_tty(&mut eval, args);
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string(
                        "Attempt to resume a non-text terminal device"
                    )]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }
}

#[test]
fn x_open_connection_requires_string_display_arg() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let bad = builtin_x_open_connection(&mut eval, vec![Value::NIL]);
    assert!(bad.is_err());
}

#[test]
fn x_open_connection_eval_accepts_x_host_startup() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    eval.set_variable("initial-window-system", Value::symbol("x"));
    assert!(
        builtin_x_open_connection(&mut eval, vec![Value::NIL])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn x_window_system_resource_queries_return_nil() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    eval.set_variable("initial-window-system", Value::symbol("x"));

    assert!(
        builtin_x_get_resource(
            &mut eval,
            vec![Value::string("geometry"), Value::string("Geometry")]
        )
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_x_list_fonts(&mut eval, vec![Value::string("*")])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn x_open_connection_arity_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let x_open_none = builtin_x_open_connection(&mut eval, vec![]);
    let x_open_four = builtin_x_open_connection(
        &mut eval,
        vec![
            Value::string("foo"),
            Value::string("xrm"),
            Value::T,
            Value::NIL,
        ],
    );
    assert!(x_open_none.is_err());
    assert!(x_open_four.is_err());
    match x_open_none {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match x_open_four {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn x_close_connection_argument_shape_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let x_nil = builtin_x_close_connection(&mut eval, vec![Value::NIL]);
    let x_int = builtin_x_close_connection(&mut eval, vec![Value::fixnum(1)]);
    let x_str = builtin_x_close_connection(&mut eval, vec![Value::string("")]);
    let x_raw = builtin_x_close_connection(
        &mut eval,
        vec![Value::heap_string(
            crate::heap_types::LispString::from_unibyte(vec![0xFF]),
        )],
    );
    let x_term = builtin_x_close_connection(&mut eval, vec![terminal_handle_value()]);
    let x_close_none = builtin_x_close_connection(&mut eval, vec![]);
    let x_close_two = builtin_x_close_connection(&mut eval, vec![Value::string("foo"), Value::NIL]);
    assert!(x_nil.is_err());
    assert!(x_int.is_err());
    assert!(x_str.is_err());
    assert!(x_raw.is_err());
    assert!(x_close_none.is_err());
    assert!(x_close_two.is_err());
    match x_close_none {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match x_close_two {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match x_term {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Terminal 0 is not an X display")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn eval_x_close_connection_live_frame_uses_window_system_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;

    let result = builtin_x_close_connection(&mut eval, vec![Value::make_frame(frame_id)]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn tty_menu_navigation_command_domain_matches_gnu_symbols() {
    assert_eq!(
        TtyMenuNavigationCommand::from_symbol_name("tty-menu-next-item"),
        Some(TtyMenuNavigationCommand::TtyMenuNextItem)
    );
    assert_eq!(
        TtyMenuNavigationCommand::from_symbol_name("tty-menu-prev-item"),
        Some(TtyMenuNavigationCommand::TtyMenuPrevItem)
    );
    assert_eq!(
        TtyMenuNavigationCommand::from_symbol_name("tty-menu-next-menu"),
        Some(TtyMenuNavigationCommand::TtyMenuNextMenu)
    );
    assert_eq!(
        TtyMenuNavigationCommand::from_symbol_name("tty-menu-prev-menu"),
        Some(TtyMenuNavigationCommand::TtyMenuPrevMenu)
    );
    assert_eq!(
        TtyMenuNavigationCommand::from_symbol_name("tty-menu-select"),
        Some(TtyMenuNavigationCommand::TtyMenuSelect)
    );
    assert_eq!(
        TtyMenuNavigationCommand::from_symbol_name("tty-menu-exit"),
        Some(TtyMenuNavigationCommand::TtyMenuExit)
    );
    assert_eq!(
        TtyMenuNavigationCommand::from_symbol_name("keyboard-quit"),
        Some(TtyMenuNavigationCommand::KeyboardQuit)
    );
    assert_eq!(
        TtyMenuNavigationCommand::from_symbol_name("keyboard-escape-quit"),
        Some(TtyMenuNavigationCommand::KeyboardEscapeQuit)
    );
    assert_eq!(
        TtyMenuNavigationCommand::from_symbol_name("menu-bar-open"),
        None
    );
    assert_eq!(
        <&'static str>::from(TtyMenuNavigationCommand::TtyMenuNextItem),
        "tty-menu-next-item"
    );
}

#[test]
fn window_system_kind_domain_matches_gnu_and_neomacs_symbols() {
    assert_eq!(
        WindowSystemKind::from_symbol_name("x"),
        Some(WindowSystemKind::X)
    );
    assert_eq!(
        WindowSystemKind::from_symbol_name("w32"),
        Some(WindowSystemKind::W32)
    );
    assert_eq!(
        WindowSystemKind::from_symbol_name("pc"),
        Some(WindowSystemKind::Pc)
    );
    assert_eq!(
        WindowSystemKind::from_symbol_name("ns"),
        Some(WindowSystemKind::Ns)
    );
    assert_eq!(
        WindowSystemKind::from_symbol_name("pgtk"),
        Some(WindowSystemKind::Pgtk)
    );
    assert_eq!(
        WindowSystemKind::from_symbol_name("haiku"),
        Some(WindowSystemKind::Haiku)
    );
    assert_eq!(
        WindowSystemKind::from_symbol_name("android"),
        Some(WindowSystemKind::Android)
    );
    assert_eq!(
        WindowSystemKind::from_symbol_name(gui_window_system_symbol()),
        Some(WindowSystemKind::Neo)
    );
    assert_eq!(WindowSystemKind::from_symbol_name("tty"), None);
    assert_eq!(<&'static str>::from(WindowSystemKind::Pgtk), "pgtk");
    assert!(WindowSystemKind::Neo.is_neomacs_gui_compatible());
    assert!(WindowSystemKind::X.is_neomacs_gui_compatible());
    assert!(!WindowSystemKind::W32.is_neomacs_gui_compatible());
    assert!(WindowSystemKind::Android.supports_selections());
    assert!(!WindowSystemKind::Pc.supports_selections());
}

#[test]
fn x_display_pixel_size_errors_match_batch_shapes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let width_none = builtin_x_display_pixel_width(&mut eval, vec![]);
    let width_int = builtin_x_display_pixel_width(&mut eval, vec![Value::fixnum(1)]);
    let width_str = builtin_x_display_pixel_width(&mut eval, vec![Value::string("")]);
    let width_term = builtin_x_display_pixel_width(&mut eval, vec![terminal_handle_value()]);
    let height_none = builtin_x_display_pixel_height(&mut eval, vec![]);
    let height_int = builtin_x_display_pixel_height(&mut eval, vec![Value::fixnum(1)]);
    let height_str = builtin_x_display_pixel_height(&mut eval, vec![Value::string("")]);
    let height_term = builtin_x_display_pixel_height(&mut eval, vec![terminal_handle_value()]);
    assert!(width_none.is_err());
    assert!(width_int.is_err());
    assert!(width_str.is_err());
    assert!(height_none.is_err());
    assert!(height_int.is_err());
    assert!(height_str.is_err());
    match width_term {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Terminal 0 is not an X display")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match height_term {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Terminal 0 is not an X display")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn x_missing_optional_display_queries_match_batch_no_x_shapes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let term = terminal_handle_value();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;

    type EvalXQuery = fn(&mut crate::emacs_core::eval::Context, Vec<Value>) -> EvalResult;
    for eval_query in [
        builtin_x_display_backing_store as EvalXQuery,
        builtin_x_display_color_cells,
        builtin_x_display_mm_height,
        builtin_x_display_mm_width,
        builtin_x_display_monitor_attributes_list,
        builtin_x_display_planes,
        builtin_x_display_save_under,
        builtin_x_display_screens,
        builtin_x_display_visual_class,
        builtin_x_server_input_extension_version,
        builtin_x_server_vendor,
    ] {
        match eval_query(&mut eval, vec![]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("X windows are not in use or not initialized")]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }

        match eval_query(&mut eval, vec![term]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                // Terminal ID may vary; just check the message pattern.
                let msg = sig.data[0].as_utf8_str().unwrap_or_default();
                assert!(
                    msg.contains("is not an X display") || msg.contains("X windows are not in use"),
                    "expected terminal error, got: {msg}"
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }

        match eval_query(&mut eval, vec![Value::string("x")]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                let actual_msg = sig.data[0].as_utf8_str().map(String::from);
                assert_eq!(
                    actual_msg.as_deref(),
                    Some("Display x can\u{2019}t be opened"),
                    "full data={:?}",
                    sig.data
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }

        match eval_query(&mut eval, vec![Value::fixnum(1)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
                );
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }

        match eval_query(&mut eval, vec![Value::make_frame(frame_id)]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Window system frame should be used")]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }
}

#[test]
fn x_gui_display_queries_accept_nil_and_live_frames_when_x_is_active() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = Value::fixnum(frame_id.0 as i64);
    eval.set_variable("initial-window-system", Value::NIL);
    eval.set_variable("window-system", Value::symbol(gui_window_system_symbol()));
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));

    assert_eq!(
        builtin_x_display_grayscale_p(&mut eval, vec![]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_x_display_grayscale_p(&mut eval, vec![frame]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_x_display_color_cells(&mut eval, vec![Value::NIL]).unwrap(),
        Value::fixnum(16_777_216)
    );
    assert_eq!(
        builtin_x_display_color_cells(&mut eval, vec![frame]).unwrap(),
        Value::fixnum(16_777_216)
    );
    assert_eq!(
        builtin_x_display_planes(&mut eval, vec![Value::NIL]).unwrap(),
        Value::fixnum(24)
    );
    assert_eq!(
        builtin_x_display_planes(&mut eval, vec![frame]).unwrap(),
        Value::fixnum(24)
    );
    assert_eq!(
        builtin_x_display_visual_class(&mut eval, vec![Value::NIL]).unwrap(),
        Value::symbol("true-color")
    );
    assert_eq!(
        builtin_x_display_visual_class(&mut eval, vec![frame]).unwrap(),
        Value::symbol("true-color")
    );
}

#[test]
fn display_queries_default_to_selected_frame_window_system_surface() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = Value::fixnum(frame_id.0 as i64);

    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));
    eval.set_variable("initial-window-system", Value::NIL);
    eval.set_variable("window-system", Value::NIL);

    assert_eq!(
        builtin_display_graphic_p(&mut eval, vec![]).unwrap(),
        Value::T
    );
    // `display-color-cells' is lisp/frame.el:2966, not a subr (DIVERGENCES.md
    // 157).  Its body dispatches on `framep-on-display' and, for a `neo'
    // frame, calls the C `x-display-color-cells' (src/xfns.c:5714) -- which is
    // what this bare evaluator can ask.
    assert_eq!(
        builtin_x_display_color_cells(&mut eval, vec![]).unwrap(),
        Value::fixnum(16_777_216)
    );
    assert_eq!(
        builtin_x_display_color_cells(&mut eval, vec![frame]).unwrap(),
        Value::fixnum(16_777_216)
    );
    assert_eq!(
        crate::emacs_core::builtins::symbols::builtin_xw_display_color_p_ctx(
            &eval,
            vec![Value::NIL],
        )
        .unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::builtins::symbols::builtin_xw_display_color_p_ctx(&eval, vec![frame],)
            .unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_display_planes(&mut eval, vec![]).unwrap(),
        Value::fixnum(24)
    );
    assert_eq!(
        builtin_display_visual_class(&mut eval, vec![]).unwrap(),
        Value::symbol("true-color")
    );
    assert_eq!(
        builtin_x_display_color_cells(&mut eval, vec![Value::NIL]).unwrap(),
        Value::fixnum(16_777_216)
    );
    assert_eq!(
        builtin_x_display_visual_class(&mut eval, vec![frame]).unwrap(),
        Value::symbol("true-color")
    );
}

#[test]
fn x_display_set_last_user_time_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();

    match builtin_x_display_set_last_user_time(&mut eval, vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("X windows are not in use or not initialized")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    match builtin_x_display_set_last_user_time(&mut eval, vec![Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("X windows are not in use or not initialized")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    match builtin_x_display_set_last_user_time(&mut eval, vec![Value::string("x"), Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("X windows are not in use or not initialized")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    match builtin_x_display_set_last_user_time(&mut eval, vec![Value::NIL, Value::string("x")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Display x can’t be opened")]);
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    match builtin_x_display_set_last_user_time(&mut eval, vec![Value::NIL, terminal_handle_value()])
    {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Terminal 0 is not an X display")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    match builtin_x_display_set_last_user_time(&mut eval, vec![Value::NIL, Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    match builtin_x_display_set_last_user_time(&mut eval, vec![]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    match builtin_x_display_set_last_user_time(
        &mut eval,
        vec![Value::NIL, Value::fixnum(1), Value::NIL],
    ) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn x_display_set_last_user_time_eval_uses_user_time_designator_payloads() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let term = terminal_handle_value();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;

    for display in [
        Value::NIL,
        Value::string("display"),
        Value::fixnum(1),
        Value::symbol("foo"),
        Value::make_frame(frame_id),
        term,
    ] {
        match builtin_x_display_set_last_user_time(&mut eval, vec![display, Value::string("x")]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(sig.data, vec![Value::string("Display x can’t be opened")]);
            }
            other => panic!("expected error signal, got {other:?}"),
        }

        match builtin_x_display_set_last_user_time(
            &mut eval,
            vec![display, Value::make_frame(frame_id)],
        ) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Window system frame should be used")]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }

        match builtin_x_display_set_last_user_time(&mut eval, vec![display, term]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Terminal 0 is not an X display")]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }
}

#[test]
fn x_selection_queries_and_old_gtk_dialog_batch_semantics() {
    crate::test_utils::init_test_tracing();
    assert!(builtin_x_selection_exists_p(vec![]).unwrap().is_nil());
    assert!(builtin_x_selection_owner_p(vec![]).unwrap().is_nil());
    assert!(
        builtin_x_selection_exists_p(vec![Value::symbol("PRIMARY"), Value::symbol("STRING")])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_x_selection_owner_p(vec![Value::symbol("PRIMARY"), Value::fixnum(1)])
            .unwrap()
            .is_nil()
    );
    match builtin_x_selection_exists_p(vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_selection_owner_p(vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("symbolp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    assert!(builtin_x_uses_old_gtk_dialog(vec![]).unwrap().is_nil());
    match builtin_x_uses_old_gtk_dialog(vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn x_geometry_fonts_and_resource_batch_semantics() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        builtin_x_parse_geometry(vec![Value::string("80x24+10+20")]).unwrap(),
        Value::list(vec![
            Value::cons(Value::symbol("height"), Value::fixnum(24)),
            Value::cons(Value::symbol("width"), Value::fixnum(80)),
            Value::cons(Value::symbol("top"), Value::fixnum(20)),
            Value::cons(Value::symbol("left"), Value::fixnum(10)),
        ])
    );
    assert_eq!(
        builtin_x_parse_geometry(vec![Value::string("80x24")]).unwrap(),
        Value::list(vec![
            Value::cons(Value::symbol("height"), Value::fixnum(24)),
            Value::cons(Value::symbol("width"), Value::fixnum(80)),
        ])
    );
    assert!(
        builtin_x_parse_geometry(vec![Value::string("x")])
            .unwrap()
            .is_nil()
    );
    match builtin_x_parse_geometry(vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    assert!(builtin_x_family_fonts(vec![]).unwrap().is_nil());
    assert!(
        builtin_x_family_fonts(vec![Value::string("abc"), Value::NIL])
            .unwrap()
            .is_nil()
    );
    match builtin_x_family_fonts(vec![Value::fixnum(1), Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_family_fonts(vec![Value::fixnum(1), Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::fixnum(1)]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    let mut eval = crate::emacs_core::Context::new();

    match builtin_x_list_fonts(&mut eval, vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Window system is not in use or not initialized"
                )]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    match builtin_x_get_resource(&mut eval, vec![Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string(
                    "Window system is not in use or not initialized"
                )]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_get_resource(&mut eval, vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn x_property_and_frame_arg_batch_semantics() {
    crate::test_utils::init_test_tracing();
    for args in [vec![], vec![Value::NIL], vec![Value::make_frame(1)]] {
        match builtin_x_backspace_delete_keys_p(args) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Window system frame should be used")]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }
    match builtin_x_backspace_delete_keys_p(vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    match builtin_x_get_atom_name(vec![Value::symbol("WM_CLASS")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_get_atom_name(vec![Value::symbol("WM_CLASS"), Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }

    match builtin_x_window_property(vec![Value::string("WM_NAME")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_window_property(vec![Value::string("WM_NAME"), Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_window_property(vec![
        Value::string("WM_NAME"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    match builtin_x_window_property_attributes(vec![Value::string("WM_NAME")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_window_property_attributes(vec![Value::string("WM_NAME"), Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_window_property_attributes(vec![
        Value::string("WM_NAME"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn x_coordinate_sync_and_message_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let term = terminal_handle_value();

    for args in [
        vec![Value::NIL],
        vec![Value::NIL, Value::NIL],
        vec![Value::make_frame(1)],
        vec![Value::fixnum(1), Value::NIL],
        vec![Value::string("x"), Value::NIL],
        vec![term, Value::NIL],
    ] {
        match builtin_x_synchronize(args) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("X windows are not in use or not initialized")]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }
    match builtin_x_synchronize(vec![]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    match builtin_x_translate_coordinates(vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("X windows are not in use or not initialized")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_translate_coordinates(vec![Value::make_frame(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_translate_coordinates(vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_translate_coordinates(vec![Value::string("x")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Display x can’t be opened")]);
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_translate_coordinates(vec![term]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Terminal 0 is not an X display")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_translate_coordinates(vec![]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match builtin_x_translate_coordinates(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    match builtin_x_frame_list_z_order(vec![]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("X windows are not in use or not initialized")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_frame_list_z_order(vec![Value::make_frame(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_frame_list_z_order(vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_frame_list_z_order(vec![Value::string("x")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Display x can’t be opened")]);
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_frame_list_z_order(vec![term]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Terminal 0 is not an X display")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_frame_list_z_order(vec![Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    match builtin_x_send_client_message(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("X windows are not in use or not initialized")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_send_client_message(vec![
        Value::make_frame(1),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_send_client_message(vec![
        Value::fixnum(1),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_send_client_message(vec![
        Value::string("x"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("Display x can’t be opened")]);
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_send_client_message(vec![
        term,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Terminal 0 is not an X display")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_send_client_message(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn x_popup_dialog_and_menu_batch_semantics() {
    crate::test_utils::init_test_tracing();
    // One Context for the whole test: constructing a Context resets the
    // tagged heap, so every Value below must be allocated after it exists.
    let mut eval = Context::new();
    let term = terminal_handle_value();

    match builtin_x_popup_dialog_batch(vec![Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("windowp"), Value::NIL]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_popup_dialog_batch(vec![Value::make_frame(1), Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("stringp"), Value::NIL]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_popup_dialog_batch(vec![
        Value::make_frame(1),
        Value::list(vec![Value::string("A")]),
    ]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("consp"), Value::NIL]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    assert!(
        builtin_x_popup_dialog_batch(vec![
            Value::make_frame(1),
            Value::list(vec![
                Value::string("Title"),
                Value::cons(Value::string("Yes"), Value::T),
            ]),
        ])
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_x_popup_dialog_batch(vec![
            Value::T,
            Value::list(vec![
                Value::string("Title"),
                Value::cons(Value::string("Yes"), Value::T),
            ]),
        ])
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_x_popup_dialog_batch(vec![
            Value::make_frame(1),
            Value::list(vec![Value::string("A"), Value::fixnum(1)]),
        ])
        .unwrap()
        .is_nil()
    );
    for arg in [Value::string("x"), Value::fixnum(1), term] {
        match builtin_x_popup_dialog_batch(vec![arg, Value::NIL]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("windowp"), Value::NIL]);
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }
    match builtin_x_popup_dialog_batch(vec![]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match builtin_x_popup_dialog_batch(vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match builtin_x_popup_dialog_batch(vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    let assert_wta = |result: EvalResult, pred: &str, arg: Value| match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol(pred), arg]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    };
    let basic_menu = Value::list(vec![
        Value::string("A"),
        Value::cons(Value::string("Yes"), Value::T),
    ]);

    assert!(
        builtin_x_popup_menu(&mut eval, vec![Value::NIL, Value::NIL])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_x_popup_menu(&mut eval, vec![Value::NIL, basic_menu])
            .unwrap()
            .is_nil()
    );
    for pos in [
        Value::make_frame(1),
        Value::string("x"),
        Value::fixnum(1),
        term,
    ] {
        assert_wta(
            builtin_x_popup_menu(&mut eval, vec![pos, Value::NIL]),
            "listp",
            pos,
        );
    }

    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::fixnum(0), Value::fixnum(0)]),
                Value::NIL,
            ],
        ),
        "listp",
        Value::fixnum(0),
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::fixnum(0), Value::fixnum(0)]),
                basic_menu,
            ],
        ),
        "listp",
        Value::fixnum(0),
    );
    assert_wta(
        builtin_x_popup_menu(&mut eval, vec![Value::list(vec![Value::NIL]), Value::NIL]),
        "stringp",
        Value::NIL,
    );
    assert_wta(
        builtin_x_popup_menu(&mut eval, vec![Value::list(vec![Value::NIL]), basic_menu]),
        "consp",
        Value::T,
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![Value::list(vec![Value::symbol("menu-bar")]), Value::NIL],
        ),
        "stringp",
        Value::NIL,
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![Value::list(vec![Value::symbol("menu-bar")]), basic_menu],
        ),
        "consp",
        Value::T,
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![Value::list(vec![Value::symbol("mouse-1")]), Value::NIL],
        ),
        "stringp",
        Value::NIL,
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![Value::list(vec![Value::symbol("mouse-1")]), basic_menu],
        ),
        "consp",
        Value::T,
    );

    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![Value::list(vec![Value::NIL, Value::NIL]), Value::NIL],
        ),
        "stringp",
        Value::NIL,
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![Value::list(vec![Value::NIL, Value::NIL]), basic_menu],
        ),
        "consp",
        Value::T,
    );
    assert!(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![Value::string("A")]),
            ]
        )
        .unwrap()
        .is_nil()
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![Value::string("A"), Value::fixnum(1)]),
            ],
        ),
        "listp",
        Value::fixnum(1),
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![
                    Value::fixnum(1),
                    Value::cons(Value::string("Yes"), Value::T),
                ]),
            ],
        ),
        "stringp",
        Value::fixnum(1),
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![Value::cons(Value::string("A"), Value::T)]),
            ],
        ),
        "stringp",
        Value::cons(Value::string("A"), Value::T),
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![Value::list(vec![Value::NIL, Value::NIL]), Value::fixnum(1)],
        ),
        "listp",
        Value::fixnum(1),
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::string("x"),
            ],
        ),
        "listp",
        Value::string("x"),
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![Value::string("A"), Value::NIL]),
            ],
        ),
        "stringp",
        Value::NIL,
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![
                    Value::string("A"),
                    Value::list(vec![Value::string("Pane")]),
                ]),
            ],
        ),
        "consp",
        Value::NIL,
    );
    assert!(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![
                    Value::string("A"),
                    Value::list(vec![Value::string("Pane"), Value::NIL]),
                ]),
            ]
        )
        .unwrap()
        .is_nil()
    );
    assert!(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![
                    Value::string("A"),
                    Value::list(vec![
                        Value::string("Pane"),
                        Value::cons(Value::string("Y"), Value::T),
                    ]),
                ]),
            ]
        )
        .unwrap()
        .is_nil()
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![
                    Value::string("A"),
                    Value::cons(Value::string("Pane"), Value::fixnum(1)),
                ]),
            ],
        ),
        "consp",
        Value::fixnum(1),
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::NIL, Value::NIL]),
                Value::list(vec![
                    Value::string("A"),
                    Value::cons(Value::fixnum(1), Value::fixnum(2)),
                ]),
            ],
        ),
        "stringp",
        Value::fixnum(1),
    );

    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::list(vec![Value::fixnum(0), Value::fixnum(0)])]),
                Value::NIL,
            ],
        ),
        "windowp",
        Value::NIL,
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![Value::list(vec![Value::fixnum(0), Value::fixnum(0)])]),
                basic_menu,
            ],
        ),
        "windowp",
        Value::NIL,
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![
                    Value::list(vec![Value::fixnum(0), Value::fixnum(0)]),
                    Value::fixnum(1),
                ]),
                Value::NIL,
            ],
        ),
        "windowp",
        Value::fixnum(1),
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::list(vec![
                    Value::list(vec![Value::fixnum(0), Value::fixnum(0)]),
                    Value::fixnum(1),
                ]),
                basic_menu,
            ],
        ),
        "windowp",
        Value::fixnum(1),
    );
    assert_wta(
        builtin_x_popup_menu(
            &mut eval,
            vec![
                Value::cons(
                    Value::list(vec![Value::fixnum(0), Value::fixnum(0)]),
                    Value::fixnum(0),
                ),
                Value::NIL,
            ],
        ),
        "listp",
        Value::fixnum(0),
    );
    match builtin_x_popup_menu(&mut eval, vec![]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match builtin_x_popup_menu(&mut eval, vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match builtin_x_popup_menu(&mut eval, vec![Value::NIL, Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn x_popup_menu_accepts_current_mouse_position_sentinel_in_batch() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(r#"(x-popup-menu t '("Title" ("Pane" ("Item" . value))))"#)
        .expect("documented current-mouse POSITION should be accepted");

    assert!(result.is_nil());
}

/// GNU `x_popup_menu_1` (`src/menu.c:1239-1269`) decodes the `(XY WINDOW)`
/// POSITION into a place to put the menu, and that decode accepts a FRAME:
///
/// ```c
///     if (FRAMEP (window)) { f = XFRAME (window); xpos = 0; ypos = 0; }
///     else if (WINDOWP (window)) { CHECK_LIVE_WINDOW (window); ... }
///     else
///       /* ??? Not really clean; should be Qwindow_or_framep ... */
///       wrong_type_argument (Qwindowp, window);
/// ```
///
/// A frame is what `popup-menu` passes for a nil POSITION: it normalizes via
/// `(mouse-pixel-position)`, whose car is the frame (`lisp/menu-bar.el:2786`),
/// which is the shape `imenu`'s mouse path reaches with `last-nonmenu-event`
/// nil.  Neomacs' batch `x-popup-menu` rejected the second element outright
/// with `windowp` whenever the first was non-nil, so that whole path signalled
/// where GNU returns nil.
///
/// GNU also never looks at the designator when BOTH coordinates are nil: that
/// sets `get_current_pos_p` (`src/menu.c:1182-1184`) and replaces WINDOW with
/// the selected frame, so even a nonsense designator is accepted there.
///
/// Every expectation below was read from GNU Emacs 31.0.90 `-Q --batch`.
#[test]
fn x_popup_menu_position_window_slot_accepts_a_frame_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let menu = r#"'("Title" ("Pane" ("item" . 1)))"#;
    let probe = |eval: &mut Context, position: &str| -> String {
        let form = format!(
            r#"(condition-case e (x-popup-menu {position} {menu})
                 (error (list (car e) (cdr e))))"#
        );
        crate::emacs_core::print::print_value(&eval.eval_str(&form).expect("probe should evaluate"))
    };

    // A frame in the window slot: GNU puts the menu at the frame origin.
    assert_eq!(
        probe(&mut eval, "(list (list 0 0) (selected-frame))"),
        "nil"
    );
    assert_eq!(
        probe(&mut eval, "(list (list 5 7) (selected-frame))"),
        "nil"
    );
    // A live window is equally acceptable.
    assert_eq!(
        probe(&mut eval, "(list (list 5 7) (selected-window))"),
        "nil"
    );
    // Both coordinates nil: GNU never inspects the designator at all.
    assert_eq!(
        probe(&mut eval, "(list (list nil nil) 'not-a-window)"),
        "nil"
    );
    assert_eq!(probe(&mut eval, "(list (list nil nil))"), "nil");
    // Anything that is neither a frame nor a window is still `windowp`.
    assert_eq!(
        probe(&mut eval, "(list (list 0 0) 'not-a-window)"),
        "(wrong-type-argument (windowp not-a-window))"
    );
    assert_eq!(
        probe(&mut eval, "(list (list 0 0))"),
        "(wrong-type-argument (windowp nil))"
    );
    // A valid but non-live (internal) window is `window-live-p`, not `windowp`:
    // GNU reaches it through `CHECK_LIVE_WINDOW`.
    let internal = probe(
        &mut eval,
        "(list (list 0 0) (window-parent (split-window-internal nil nil nil nil)))",
    );
    assert!(
        internal.starts_with("(wrong-type-argument (window-live-p "),
        "an internal window must fail the liveness check, not the type check: {internal}"
    );
}

/// GNU `x_popup_menu_1` decodes MENU in three branches
/// (`src/menu.c:1294-1364`) and only the last one -- the "old-fashioned menu"
/// -- runs `CHECK_STRING (title)`.  A keymap becomes panes through
/// `keymap_panes` and takes its title from the keymap prompt; a list of
/// keymaps does the same after resolving every element with `get_keymap (...,
/// 1, 0)`, which is why a bad element there is `keymapp`, not `stringp`.
///
/// `imenu` reaches the first branch: `imenu--mouse-menu` builds a keymap and
/// `popup-menu` passes `(indirect-function map)` to `x-popup-menu`.  Neomacs
/// validated every MENU as the old-fashioned shape and answered
/// `(wrong-type-argument stringp keymap)`.
///
/// Every expectation below was read from GNU Emacs 31.0.90 `-Q --batch`.
#[test]
fn x_popup_menu_accepts_keymap_menus_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let probe = |eval: &mut Context, position: &str, menu: &str| -> String {
        let form = format!(
            r#"(let ((map (make-sparse-keymap "Index")))
                 (define-key map [item] '(menu-item "Item" ignore))
                 (fset 'p109-map map)
                 (condition-case e (x-popup-menu {position} {menu})
                   (error (list (car e) (cdr e)))))"#
        );
        crate::emacs_core::print::print_value(&eval.eval_str(&form).expect("probe should evaluate"))
    };
    let at_frame = "(list (list 0 0) (selected-frame))";

    assert_eq!(probe(&mut eval, at_frame, "map"), "nil");
    assert_eq!(probe(&mut eval, "t", "map"), "nil");
    assert_eq!(probe(&mut eval, at_frame, "(list map)"), "nil");
    assert_eq!(probe(&mut eval, at_frame, "'p109-map"), "nil");
    assert_eq!(probe(&mut eval, at_frame, "(make-sparse-keymap)"), "nil");
    // A list that starts with a keymap resolves each element with GNU's
    // erroring `get_keymap`.
    assert_eq!(
        probe(&mut eval, at_frame, "(list map 42)"),
        "(wrong-type-argument (keymapp 42))"
    );
    assert_eq!(
        probe(&mut eval, at_frame, "(list map nil)"),
        "(wrong-type-argument (keymapp nil))"
    );
    // The old-fashioned branch still checks its title.
    assert_eq!(
        probe(&mut eval, at_frame, r#"'(1 ("Pane" ("i" . 1)))"#),
        "(wrong-type-argument (stringp 1))"
    );
}

#[test]
fn x_clipboard_input_context_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let term = terminal_handle_value();
    let frame = Value::make_frame(1);

    let assert_wrong_type = |result: EvalResult, pred: &str, arg: Value| match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol(pred), arg]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    };
    let assert_error = |result: EvalResult, msg: &str| match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string(msg)]);
        }
        other => panic!("expected error signal, got {other:?}"),
    };
    let assert_wrong_number = |result: EvalResult| match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    };

    assert!(builtin_x_get_clipboard(vec![]).unwrap().is_nil());
    assert_wrong_number(builtin_x_get_clipboard(vec![Value::NIL]));

    assert_error(
        builtin_x_get_modifier_masks(vec![]),
        "X windows are not in use or not initialized",
    );
    assert_error(
        builtin_x_get_modifier_masks(vec![Value::NIL]),
        "X windows are not in use or not initialized",
    );
    assert_error(
        builtin_x_get_modifier_masks(vec![term]),
        "Terminal 0 is not an X display",
    );
    assert_wrong_type(
        builtin_x_get_modifier_masks(vec![Value::fixnum(1)]),
        "frame-live-p",
        Value::fixnum(1),
    );
    assert_error(
        builtin_x_get_modifier_masks(vec![Value::string("x")]),
        "Display x can’t be opened",
    );
    assert_error(
        builtin_x_get_modifier_masks(vec![frame]),
        "Window system frame should be used",
    );
    assert_wrong_number(builtin_x_get_modifier_masks(vec![Value::NIL, Value::NIL]));

    assert!(builtin_x_hide_tip(vec![]).unwrap().is_nil());
    assert_wrong_number(builtin_x_hide_tip(vec![Value::NIL]));

    for arg in [
        Value::NIL,
        term,
        Value::fixnum(1),
        Value::string("x"),
        frame,
    ] {
        assert!(
            builtin_x_internal_focus_input_context(vec![arg])
                .unwrap()
                .is_nil()
        );
    }
    assert_wrong_number(builtin_x_internal_focus_input_context(vec![]));
    assert_wrong_number(builtin_x_internal_focus_input_context(vec![
        Value::NIL,
        Value::NIL,
    ]));

    assert_error(
        builtin_x_wm_set_size_hint(vec![]),
        "Window system frame should be used",
    );
    assert_error(
        builtin_x_wm_set_size_hint(vec![Value::NIL]),
        "Window system frame should be used",
    );
    assert_wrong_type(
        builtin_x_wm_set_size_hint(vec![term]),
        "frame-live-p",
        terminal_handle_value(),
    );
    assert_wrong_type(
        builtin_x_wm_set_size_hint(vec![Value::fixnum(1)]),
        "frame-live-p",
        Value::fixnum(1),
    );
    assert_wrong_type(
        builtin_x_wm_set_size_hint(vec![Value::string("x")]),
        "frame-live-p",
        Value::string("x"),
    );
    assert_error(
        builtin_x_wm_set_size_hint(vec![frame]),
        "Window system frame should be used",
    );
    assert_wrong_number(builtin_x_wm_set_size_hint(vec![Value::NIL, Value::NIL]));
}

#[test]
fn x_selection_property_tip_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let assert_wrong_type = |result: EvalResult, pred: &str, arg: Value| match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol(pred), arg]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    };
    let assert_error = |result: EvalResult, msg: &str| match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string(msg)]);
        }
        other => panic!("expected error signal, got {other:?}"),
    };
    let assert_wrong_number = |result: EvalResult| match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    };

    assert_error(
        builtin_x_change_window_property(vec![Value::string("P"), Value::string("V")]),
        "Window system frame should be used",
    );
    assert_error(
        builtin_x_change_window_property(vec![Value::string("P"), Value::string("V"), Value::NIL]),
        "Window system frame should be used",
    );
    assert_error(
        builtin_x_change_window_property(vec![
            Value::string("P"),
            Value::string("V"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ]),
        "Window system frame should be used",
    );
    assert_wrong_number(builtin_x_change_window_property(vec![
        Value::string("P"),
        Value::string("V"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]));

    assert_error(
        builtin_x_delete_window_property(vec![Value::string("P")]),
        "Window system frame should be used",
    );
    assert_error(
        builtin_x_delete_window_property(vec![Value::string("P"), Value::NIL]),
        "Window system frame should be used",
    );
    assert_error(
        builtin_x_delete_window_property(vec![Value::string("P"), Value::NIL, Value::NIL]),
        "Window system frame should be used",
    );
    assert_wrong_number(builtin_x_delete_window_property(vec![
        Value::string("P"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]));

    assert!(
        builtin_x_disown_selection_internal(vec![Value::NIL])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_x_disown_selection_internal(vec![Value::NIL, Value::NIL])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_x_disown_selection_internal(vec![Value::NIL, Value::NIL, Value::NIL])
            .unwrap()
            .is_nil()
    );
    assert_wrong_number(builtin_x_disown_selection_internal(vec![]));
    assert_wrong_number(builtin_x_disown_selection_internal(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]));

    assert_wrong_type(builtin_x_get_local_selection(vec![]), "consp", Value::NIL);
    assert_wrong_type(
        builtin_x_get_local_selection(vec![Value::NIL]),
        "consp",
        Value::NIL,
    );
    assert_wrong_type(
        builtin_x_get_local_selection(vec![Value::NIL, Value::NIL]),
        "consp",
        Value::NIL,
    );
    assert_wrong_number(builtin_x_get_local_selection(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]));

    assert_error(
        builtin_x_get_selection_internal(vec![Value::NIL, Value::NIL]),
        "X selection unavailable for this frame",
    );
    assert_error(
        builtin_x_get_selection_internal(vec![Value::NIL, Value::NIL, Value::NIL]),
        "X selection unavailable for this frame",
    );
    assert_error(
        builtin_x_get_selection_internal(vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL]),
        "X selection unavailable for this frame",
    );
    assert_wrong_number(builtin_x_get_selection_internal(vec![]));
    assert_wrong_number(builtin_x_get_selection_internal(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]));

    assert_error(
        builtin_x_own_selection_internal(vec![Value::NIL, Value::NIL]),
        "X selection unavailable for this frame",
    );
    assert_error(
        builtin_x_own_selection_internal(vec![Value::NIL, Value::NIL, Value::NIL]),
        "X selection unavailable for this frame",
    );
    assert_wrong_number(builtin_x_own_selection_internal(vec![Value::NIL]));
    assert_wrong_number(builtin_x_own_selection_internal(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]));

    assert_error(
        builtin_x_show_tip(vec![Value::string("m")]),
        "Window system frame should be used",
    );
    assert_wrong_type(
        builtin_x_show_tip(vec![Value::fixnum(1)]),
        "stringp",
        Value::fixnum(1),
    );
    assert_error(
        builtin_x_show_tip(vec![
            Value::string("m"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ]),
        "Window system frame should be used",
    );
    assert_wrong_number(builtin_x_show_tip(vec![]));
    assert_wrong_number(builtin_x_show_tip(vec![
        Value::string("m"),
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]));
}

#[test]
fn gui_selection_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let assert_error = |result: EvalResult, msg: &str| match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string(msg)]);
        }
        other => panic!("expected error signal, got {other:?}"),
    };
    let assert_wrong_number = |result: EvalResult| match result {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    };

    assert!(builtin_gui_get_selection(vec![]).unwrap().is_nil());
    assert!(
        builtin_gui_get_selection(vec![Value::NIL])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_gui_get_selection(vec![Value::NIL, Value::NIL])
            .unwrap()
            .is_nil()
    );
    assert_wrong_number(builtin_gui_get_selection(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]));

    assert_error(
        builtin_gui_get_primary_selection(vec![]),
        "No selection is available",
    );
    assert_wrong_number(builtin_gui_get_primary_selection(vec![Value::NIL]));

    assert!(
        builtin_gui_select_text(vec![Value::string("a")])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_gui_select_text(vec![Value::fixnum(1)])
            .unwrap()
            .is_nil()
    );
    assert_wrong_number(builtin_gui_select_text(vec![
        Value::string("a"),
        Value::NIL,
    ]));

    assert!(builtin_gui_selection_value(vec![]).unwrap().is_nil());
    assert_wrong_number(builtin_gui_selection_value(vec![Value::NIL]));

    assert!(
        builtin_gui_set_selection(vec![Value::NIL, Value::NIL])
            .unwrap()
            .is_nil()
    );
    assert_wrong_number(builtin_gui_set_selection(vec![
        Value::NIL,
        Value::NIL,
        Value::NIL,
    ]));
}

#[test]
fn x_frame_restack_safe_arity_surface() {
    crate::test_utils::init_test_tracing();
    match builtin_x_frame_restack(vec![Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_frame_restack(vec![Value::NIL, Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
    match builtin_x_frame_restack(vec![]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match builtin_x_frame_restack(vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match builtin_x_frame_restack(vec![Value::NIL, Value::NIL, Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn x_frame_mouse_and_dnd_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let term = terminal_handle_value();

    for args in [
        vec![],
        vec![Value::NIL],
        vec![Value::make_frame(1)],
        vec![Value::NIL, Value::NIL],
    ] {
        match builtin_x_export_frames(args) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Window system frame should be used")]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }
    for arg in [Value::fixnum(1), Value::string("x"), term] {
        match builtin_x_export_frames(vec![arg]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("frame-live-p"), arg]);
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }
    match builtin_x_export_frames(vec![Value::NIL, Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    let mut focus_eval = crate::emacs_core::Context::new();
    let focus_frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut focus_eval);
    for args in [
        vec![Value::NIL],
        vec![Value::make_frame(focus_frame_id.0)],
        vec![Value::NIL, Value::NIL],
    ] {
        match builtin_x_focus_frame(&mut focus_eval, args) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Window system frame should be used")]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }
    for arg in [Value::fixnum(999999), Value::string("x"), term] {
        match builtin_x_focus_frame(&mut focus_eval, vec![arg]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("frame-live-p"), arg]);
            }
            other => panic!("expected wrong-type-argument signal, got {other:?}"),
        }
    }
    match builtin_x_focus_frame(&mut focus_eval, vec![]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    assert!(builtin_x_frame_edges(vec![]).unwrap().is_nil());
    assert!(builtin_x_frame_edges(vec![Value::NIL]).unwrap().is_nil());
    assert!(
        builtin_x_frame_edges(vec![Value::make_frame(1)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_x_frame_edges(vec![Value::NIL, Value::NIL])
            .unwrap()
            .is_nil()
    );
    match builtin_x_frame_edges(vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_frame_edges(vec![Value::string("x")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::string("x")]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_frame_edges(vec![Value::NIL, Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    assert!(builtin_x_frame_geometry(vec![]).unwrap().is_nil());
    assert!(builtin_x_frame_geometry(vec![Value::NIL]).unwrap().is_nil());
    assert!(
        builtin_x_frame_geometry(vec![Value::make_frame(1)])
            .unwrap()
            .is_nil()
    );
    match builtin_x_frame_geometry(vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_frame_geometry(vec![Value::string("x")]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::string("x")]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
    match builtin_x_frame_geometry(vec![Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    assert!(
        builtin_x_mouse_absolute_pixel_position(vec![])
            .unwrap()
            .is_nil()
    );
    match builtin_x_mouse_absolute_pixel_position(vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    assert!(
        builtin_x_set_mouse_absolute_pixel_position(vec![Value::NIL, Value::NIL])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_x_set_mouse_absolute_pixel_position(vec![Value::fixnum(1), Value::fixnum(2)])
            .unwrap()
            .is_nil()
    );
    match builtin_x_set_mouse_absolute_pixel_position(vec![Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match builtin_x_set_mouse_absolute_pixel_position(vec![Value::NIL, Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }

    for args in [
        vec![Value::NIL],
        vec![Value::make_frame(1)],
        vec![Value::fixnum(1)],
        vec![terminal_handle_value()],
        vec![Value::NIL, Value::NIL],
    ] {
        match builtin_x_register_dnd_atom(args) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("Window system frame should be used")]
                );
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }
    match builtin_x_register_dnd_atom(vec![]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
    match builtin_x_register_dnd_atom(vec![Value::NIL, Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

#[test]
fn eval_x_display_queries_accept_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;

    let width = builtin_x_display_pixel_width(&mut eval, vec![Value::make_frame(frame_id)]);
    let height = builtin_x_display_pixel_height(&mut eval, vec![Value::make_frame(frame_id)]);
    assert!(width.is_err());
    assert!(height.is_err());
}

#[test]
fn eval_x_display_pixel_queries_use_selected_gui_display() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));

    assert_eq!(
        builtin_x_display_pixel_width(&mut eval, vec![]).unwrap(),
        Value::fixnum(80)
    );
    assert_eq!(
        builtin_x_display_pixel_height(&mut eval, vec![]).unwrap(),
        Value::fixnum(25)
    );
    assert_eq!(
        builtin_x_display_pixel_width(&mut eval, vec![Value::make_frame(frame_id.0)]).unwrap(),
        Value::fixnum(80)
    );
    assert_eq!(
        builtin_x_display_pixel_height(&mut eval, vec![Value::make_frame(frame_id.0)]).unwrap(),
        Value::fixnum(25)
    );
}

#[test]
fn eval_x_display_mm_queries_accept_selected_gui_display() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));

    for result in [
        builtin_x_display_mm_width(&mut eval, vec![]),
        builtin_x_display_mm_height(&mut eval, vec![]),
        builtin_x_display_mm_width(&mut eval, vec![Value::make_frame(frame_id.0)]),
        builtin_x_display_mm_height(&mut eval, vec![Value::make_frame(frame_id.0)]),
    ] {
        let dimension = result.expect("a live GUI display is a valid millimeter query target");
        assert!(
            dimension.is_nil() || dimension.as_fixnum().is_some(),
            "an unavailable physical dimension is nil; an available one is an integer"
        );
    }
}

#[test]
fn x_focus_frame_accepts_live_neomacs_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));

    assert_eq!(
        builtin_x_focus_frame(&mut eval, vec![Value::make_frame(frame_id.0)]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_x_focus_frame(&mut eval, vec![Value::NIL]).unwrap(),
        Value::NIL
    );
}

#[test]
fn x_focus_frame_rejects_live_tty_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(None);

    let err = builtin_x_focus_frame(&mut eval, vec![Value::make_frame(frame_id.0)]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Window system frame should be used")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }
}

#[test]
fn eval_monitor_attributes_include_bootstrapped_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let list = builtin_display_monitor_attributes_list(&mut eval, vec![]).unwrap();
    let monitors = list_to_vec(&list).expect("monitor list");
    let attrs = list_to_vec(monitors.first().expect("first monitor")).expect("monitor attrs");

    let mut frames_value = Value::NIL;
    for attr in attrs {
        if attr.is_cons() {
            let pair_car = attr.cons_car();
            let pair_cdr = attr.cons_cdr();
            if pair_car.is_symbol_named("frames") {
                frames_value = pair_cdr;
                break;
            }
        }
    }

    let frames = list_to_vec(&frames_value).expect("frames list");
    assert_eq!(frames.len(), 1);
    assert!(frames.first().map_or(false, |v| v.is_frame()));
    assert!(!frames[0].is_integer());
    assert_eq!(
        crate::emacs_core::frame::builtin_framep(&mut eval, vec![frames[0]]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_live_p(&mut eval, vec![frames[0]]).unwrap(),
        Value::T
    );
}

#[test]
fn eval_monitor_queries_accept_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;

    let list =
        builtin_display_monitor_attributes_list(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap();
    let monitors = list_to_vec(&list).expect("monitor list");
    assert_eq!(monitors.len(), 1);

    let attrs =
        builtin_frame_monitor_attributes(&mut eval, vec![Value::make_frame(frame_id)]).unwrap();
    let attr_list = list_to_vec(&attrs).expect("monitor attrs");
    assert!(!attr_list.is_empty());
}

#[test]
fn eval_monitor_queries_accept_frame_handle_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let list = builtin_display_monitor_attributes_list(&mut eval, vec![]).unwrap();
    let monitors = list_to_vec(&list).expect("monitor list");
    let attrs = list_to_vec(monitors.first().expect("first monitor")).expect("monitor attrs");

    let mut frame = Value::NIL;
    for attr in attrs {
        if attr.is_cons() {
            let pair_car = attr.cons_car();
            let pair_cdr = attr.cons_cdr();
            if pair_car.is_symbol_named("frames") {
                let frames = list_to_vec(&pair_cdr).expect("frames list");
                frame = frames.first().cloned().expect("first frame");
                break;
            }
        }
    }
    assert!(frame.is_frame());

    let by_display = builtin_display_monitor_attributes_list(&mut eval, vec![frame]).unwrap();
    let display_list = list_to_vec(&by_display).expect("monitor list");
    assert_eq!(display_list.len(), 1);

    let by_frame = builtin_frame_monitor_attributes(&mut eval, vec![frame]).unwrap();
    let frame_attrs = list_to_vec(&by_frame).expect("monitor attrs");
    assert!(!frame_attrs.is_empty());
}

#[test]
fn eval_display_queries_accept_live_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;

    assert!(
        builtin_display_graphic_p(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert_eq!(
        builtin_display_pixel_width(&mut eval, vec![Value::make_frame(frame_id)]).unwrap(),
        Value::fixnum(80)
    );
    assert_eq!(
        builtin_display_pixel_height(&mut eval, vec![Value::make_frame(frame_id)]).unwrap(),
        Value::fixnum(25)
    );
    assert!(
        builtin_display_mm_width(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_mm_height(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert_eq!(
        builtin_display_screens(&mut eval, vec![Value::make_frame(frame_id)]).unwrap(),
        Value::fixnum(1)
    );
    // No window system, so `display-color-cells' (lisp/frame.el:2966) falls to
    // the `t' arm of its `cond' and calls the C `tty-display-color-cells'
    // (src/term.c:2226).  DIVERGENCES.md 157 deleted the Rust subr for the
    // Lisp name; the primitive beneath it is what a bare evaluator asks.
    assert_eq!(
        crate::emacs_core::terminal::pure::builtin_tty_display_color_cells(
            &mut eval,
            vec![Value::make_frame(frame_id)]
        )
        .unwrap(),
        Value::fixnum(0)
    );
    assert_eq!(
        builtin_display_planes(&mut eval, vec![Value::make_frame(frame_id)]).unwrap(),
        Value::fixnum(3)
    );
    assert_eq!(
        builtin_display_visual_class(&mut eval, vec![Value::make_frame(frame_id)]).unwrap(),
        Value::symbol("static-gray")
    );
    assert_eq!(
        builtin_display_backing_store(&mut eval, vec![Value::make_frame(frame_id)]).unwrap(),
        Value::symbol("not-useful")
    );
    assert_eq!(
        builtin_display_save_under(&mut eval, vec![Value::make_frame(frame_id)]).unwrap(),
        Value::symbol("not-useful")
    );
    assert!(
        builtin_display_selections_p(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_images_p(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_supports_face_attributes_p(
            &mut eval,
            vec![Value::list(vec![
                Value::symbol(":weight"),
                Value::symbol("bold")
            ])]
        )
        .unwrap()
        .is_nil()
    );
}

#[test]
fn window_system_prefers_selected_frame_then_global_fallback() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();

    assert_eq!(
        builtin_window_system(&mut eval, vec![]).unwrap(),
        Value::NIL
    );
    assert!(
        eval.frames.frame_list().is_empty(),
        "window-system should not synthesize a frame when no frame exists"
    );

    eval.set_variable("window-system", Value::symbol("tty"));
    assert_eq!(
        builtin_window_system(&mut eval, vec![]).unwrap(),
        Value::symbol("tty")
    );
    assert!(
        eval.frames.frame_list().is_empty(),
        "window-system should use the global fallback without synthesizing a frame"
    );

    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_parameter(Value::symbol("window-system"), Value::symbol("x"));

    assert_eq!(
        builtin_window_system(&mut eval, vec![]).unwrap(),
        Value::symbol("x")
    );
    assert_eq!(
        builtin_window_system(&mut eval, vec![Value::fixnum(frame_id.0 as i64)]).unwrap(),
        Value::symbol("x")
    );
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(None);
    assert_eq!(
        builtin_window_system(&mut eval, vec![Value::fixnum(frame_id.0 as i64)]).unwrap(),
        Value::NIL,
        "an explicit non-window-system frame must not fall back to global window-system"
    );

    let err = builtin_window_system(&mut eval, vec![Value::string("x")]).unwrap_err();
    match err {
        Flow::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("framep"), Value::string("x")]);
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }
}

#[test]
fn display_graphic_p_uses_global_window_system_without_live_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    eval.set_variable("initial-window-system", Value::symbol("neo"));

    assert_eq!(
        builtin_display_graphic_p(&mut eval, vec![]).unwrap(),
        Value::T
    );
    assert!(
        eval.frames.frame_list().is_empty(),
        "display-graphic-p should not synthesize a frame when only the global backend is known"
    );
}

#[test]
fn eval_display_queries_reject_invalid_frame_designator() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let _ = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let result = builtin_display_pixel_width(&mut eval, vec![Value::fixnum(999_999)]);
    assert!(result.is_err());
}

#[test]
fn eval_display_queries_string_designator_reports_missing_display() {
    crate::test_utils::init_test_tracing();
    fn assert_missing_display(result: EvalResult) {
        match result {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(sig.data, vec![Value::string("Display x does not exist")]);
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }

    let mut eval = crate::emacs_core::Context::new();
    assert_missing_display(builtin_display_graphic_p(
        &mut eval,
        vec![Value::string("x")],
    ));
    assert_missing_display(builtin_display_pixel_width(
        &mut eval,
        vec![Value::string("x")],
    ));
    assert_missing_display(builtin_display_pixel_height(
        &mut eval,
        vec![Value::string("x")],
    ));
    assert_missing_display(builtin_display_mm_width(
        &mut eval,
        vec![Value::string("x")],
    ));
    assert_missing_display(builtin_display_mm_height(
        &mut eval,
        vec![Value::string("x")],
    ));
    assert_missing_display(builtin_display_screens(&mut eval, vec![Value::string("x")]));
    // No `display-color-cells' row: it is lisp/frame.el:2966 and has no Rust
    // subr any more (DIVERGENCES.md 157).  Nor does its C neighbour stand in
    // for it here -- the two answer DIFFERENT errors, measured on GNU 31.0.90
    // `-Q --batch':
    //
    //     (display-color-cells "x")   => (error "Display x does not exist")
    //     (x-display-color-cells "x") => (error "Display x can't be opened")
    //
    // GNU's message in the first line is raised by `framep-on-display', the
    // Lisp its body opens with, not by the primitive its `memq' arm reaches --
    // so repointing this row at `x-display-color-cells' would assert the wrong
    // string.  Ours already answers GNU's second line.
    assert_missing_display(builtin_display_planes(&mut eval, vec![Value::string("x")]));
    assert_missing_display(builtin_display_visual_class(
        &mut eval,
        vec![Value::string("x")],
    ));
    assert_missing_display(builtin_display_backing_store(
        &mut eval,
        vec![Value::string("x")],
    ));
    assert_missing_display(builtin_display_save_under(
        &mut eval,
        vec![Value::string("x")],
    ));
    assert_missing_display(builtin_display_selections_p(
        &mut eval,
        vec![Value::string("x")],
    ));
    assert_missing_display(builtin_display_images_p(
        &mut eval,
        vec![Value::string("x")],
    ));
}

#[test]
fn eval_display_monitor_errors_render_window_designators() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let _ = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let window =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();

    let list_err = builtin_display_monitor_attributes_list(&mut eval, vec![window])
        .expect_err("window designator should be rejected");
    let frame_err = builtin_frame_monitor_attributes(&mut eval, vec![window])
        .expect_err("window designator should be rejected");

    for err in [list_err, frame_err] {
        match err {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol_name(), "error");
                match sig.data.as_slice() {
                    [val] => {
                        let msg = val
                            .as_utf8_str()
                            .expect("expected string payload")
                            .to_string();
                        assert!(msg.contains("get-device-terminal"));
                        assert!(msg.contains("#<window"));
                        assert!(msg.contains("*scratch*"));
                    }
                    other => panic!("unexpected signal payload: {other:?}"),
                }
            }
            other => panic!("expected signal, got {other:?}"),
        }
    }
}

#[test]
fn get_device_terminal_formatter_keeps_integer_literals() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let _ = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let window =
        crate::emacs_core::window_cmds::builtin_selected_window(&mut eval, vec![]).unwrap();

    let rendered_window = format_get_device_terminal_arg_eval(&eval, &window);
    assert!(rendered_window.contains("#<window"));
    assert!(rendered_window.contains("*scratch*"));

    let rendered_integer = format_get_device_terminal_arg_eval(&eval, &Value::fixnum(1));
    assert_eq!(rendered_integer, "1");
}

#[test]
fn display_images_p_shapes_and_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    assert!(
        builtin_display_images_p(&mut eval, vec![])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_images_p(&mut eval, vec![Value::NIL])
            .unwrap()
            .is_nil()
    );
    eval.set_display_host(Box::new(ImageCapableDisplayHost));
    assert_eq!(
        builtin_display_images_p(&mut eval, vec![]).unwrap(),
        Value::T
    );

    match builtin_display_images_p(&mut eval, vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Invalid argument 1 in ‘get-device-terminal’")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    match builtin_display_images_p(&mut eval, vec![Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments, got {other:?}"),
    }
}

#[test]
fn display_save_under_and_display_selections_p_shapes_and_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();

    assert_eq!(
        builtin_display_save_under(&mut eval, vec![]).unwrap(),
        Value::symbol("not-useful")
    );
    assert_eq!(
        builtin_display_save_under(&mut eval, vec![Value::NIL]).unwrap(),
        Value::symbol("not-useful")
    );
    assert!(
        builtin_display_selections_p(&mut eval, vec![])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_selections_p(&mut eval, vec![Value::NIL])
            .unwrap()
            .is_nil()
    );

    match builtin_display_save_under(&mut eval, vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Invalid argument 1 in ‘get-device-terminal’")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    match builtin_display_selections_p(&mut eval, vec![Value::fixnum(1)]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(
                sig.data,
                vec![Value::string("Invalid argument 1 in ‘get-device-terminal’")]
            );
        }
        other => panic!("expected error signal, got {other:?}"),
    }

    match builtin_display_save_under(&mut eval, vec![Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments, got {other:?}"),
    }

    match builtin_display_selections_p(&mut eval, vec![Value::NIL, Value::NIL]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments, got {other:?}"),
    }
}

#[test]
fn display_optional_capability_queries_match_color_shapes() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();

    for query in [
        builtin_display_grayscale_p
            as fn(&mut crate::emacs_core::eval::Context, Vec<Value>) -> EvalResult,
        builtin_display_mouse_p,
        builtin_display_popup_menus_p,
        builtin_display_symbol_keys_p,
    ] {
        assert!(query(&mut eval, vec![]).unwrap().is_nil());
        assert!(query(&mut eval, vec![Value::NIL]).unwrap().is_nil());
        assert!(
            query(&mut eval, vec![terminal_handle_value()])
                .unwrap()
                .is_nil()
        );

        match query(&mut eval, vec![Value::fixnum(1)]) {
            Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "error"),
            other => panic!("expected error signal, got {other:?}"),
        }

        match query(&mut eval, vec![Value::string("x")]) {
            Err(Flow::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "error");
                assert_eq!(sig.data, vec![Value::string("Display x does not exist")]);
            }
            other => panic!("expected error signal, got {other:?}"),
        }
    }

    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval).0;
    assert!(
        builtin_display_grayscale_p(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_mouse_p(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_popup_menus_p(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_symbol_keys_p(&mut eval, vec![Value::make_frame(frame_id)])
            .unwrap()
            .is_nil()
    );
}

#[test]
fn display_supports_face_attributes_p_arity_and_nil_result() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let attrs = Value::list(vec![Value::symbol(":weight"), Value::symbol("bold")]);
    assert!(
        builtin_display_supports_face_attributes_p(&mut eval, vec![attrs])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_supports_face_attributes_p(&mut eval, vec![attrs, Value::fixnum(999_999)])
            .unwrap()
            .is_nil()
    );
    assert!(
        builtin_display_supports_face_attributes_p(&mut eval, vec![Value::fixnum(1)])
            .unwrap()
            .is_nil()
    );

    match builtin_display_supports_face_attributes_p(&mut eval, vec![]) {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments, got {other:?}"),
    }
    match builtin_display_supports_face_attributes_p(&mut eval, vec![attrs, Value::NIL, Value::NIL])
    {
        Err(Flow::Signal(sig)) => assert_eq!(sig.symbol_name(), "wrong-number-of-arguments"),
        other => panic!("expected wrong-number-of-arguments, got {other:?}"),
    }
}

#[test]
fn display_supports_face_attributes_p_uses_live_gui_font_capabilities() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    let frame_id = eval
        .frames
        .create_frame("gui-face-support", 800, 600, scratch);
    eval.frames.select_frame(frame_id);
    eval.set_display_host(Box::new(ItalicCapableDisplayHost));

    let attrs = Value::list(vec![Value::keyword("slant"), Value::symbol("italic")]);
    assert_eq!(
        builtin_display_supports_face_attributes_p(&mut eval, vec![attrs]).unwrap(),
        Value::T,
        "a GUI host that realizes the requested italic font must report the face spec as supported",
    );
}

#[test]
fn x_popup_menu_interactive_keymap_returns_selected_event() {
    let mut eval = crate::emacs_core::Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    let frame_id = eval.frames.create_frame("popup-owner", 800, 600, scratch);
    eval.frames.select_frame(frame_id);
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    let hidden = Arc::clone(&host.hidden);
    eval.set_display_host(Box::new(host));

    let menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
    );
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: 0 })
        .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    let events = crate::emacs_core::value::list_to_vec(&result).expect("event list");
    assert_eq!(events, vec![Value::symbol("open")]);
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].frame_id, frame_id);
    assert_eq!(shown[0].entries.len(), 1);
    assert_eq!(shown[0].entries[0].label, "Open");
    assert_eq!(*hidden.lock().unwrap(), 1);
}

#[test]
fn x_popup_dialog_interactive_returns_selected_value() {
    let mut eval = crate::emacs_core::Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    let frame_id = eval.frames.create_frame("dialog-owner", 800, 600, scratch);
    eval.frames.select_frame(frame_id);
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    let hidden = Arc::clone(&host.hidden);
    eval.set_display_host(Box::new(host));

    let contents = Value::list(vec![
        Value::string("Confirm?"),
        Value::cons(Value::string("Yes"), Value::T),
        Value::cons(Value::string("No"), Value::symbol("declined")),
    ]);
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: 1 })
        .unwrap();

    let result =
        super::builtin_x_popup_dialog(&mut eval, vec![Value::T, contents, Value::NIL]).unwrap();

    assert_eq!(result, Value::symbol("declined"));
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].frame_id, frame_id);
    assert_eq!(
        shown[0].placement,
        neomacs_display_protocol::PopupPlacement::at(neomacs_display_protocol::Point::new(
            400.0, 300.0
        ),)
    );
    assert_eq!(shown[0].title.as_deref(), Some("Confirm?"));
    assert_eq!(shown[0].entries.len(), 2);
    assert_eq!(shown[0].entries[0].label, "Yes");
    assert_eq!(shown[0].entries[1].label, "No");
    assert_eq!(*hidden.lock().unwrap(), 1);
}

#[test]
fn x_popup_menu_interactive_keymap_collapses_submenu_on_tty() {
    // A frame with no window system is a TTY frame (effective_window_system =
    // None). GNU `single_menu_item` (src/menu.c:407-433) renders a submenu on a
    // TTY frame as ONE collapsed line whose label gains the `" >"` suffix; the
    // child-recursion (`single_keymap_panes`) is inside the
    // `#if USE_X_TOOLKIT || USE_GTK || ...` block and is compiled out for TTY,
    // so the children are NOT inlined into the parent pane (they are opened on
    // demand by `tty_menu_activate`). This test previously asserted the GUI
    // toolkit behavior (children flattened into the same pane), which is wrong
    // for a TTY frame and ballooned panes like Help -> Describe past the
    // screen height.
    let mut eval = crate::emacs_core::Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    let frame_id = eval.frames.create_frame("popup-owner", 800, 600, scratch);
    eval.frames.select_frame(frame_id);
    assert!(
        eval.frames
            .get(frame_id)
            .and_then(|f| f.effective_window_system())
            .is_none(),
        "test fixture frame should be a TTY frame (no window system)"
    );
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    eval.set_display_host(Box::new(host));

    let submenu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        submenu,
        Value::symbol("visual-line-mode"),
        Value::cons(Value::string("Visual Line Mode"), Value::T),
    );

    let menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        menu,
        Value::symbol("line-wrapping"),
        Value::cons(Value::string("Line Wrapping in this Buffer"), submenu),
    );
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: -1 })
        .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    assert!(result.is_nil());
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    // Only the collapsed submenu header is pushed — its children are NOT
    // inlined — and the label gains the GNU `" >"` suffix.
    assert_eq!(shown[0].entries.len(), 1);
    assert_eq!(shown[0].entries[0].label, "Line Wrapping in this Buffer >");
    assert!(shown[0].entries[0].submenu);
    assert_eq!(shown[0].entries[0].depth, 0);
}

#[test]
fn x_popup_menu_interactive_submenu_selection_returns_full_event_path() {
    let mut eval = crate::emacs_core::Context::new();
    // On a window-system (GUI) frame the toolkit renders nested submenu panes,
    // so GNU recurses into the submenu (`single_keymap_panes`, the
    // `#if USE_X_TOOLKIT || ...` branch in src/menu.c:422-433) and the child
    // entry is reachable. Selecting it returns the full nested event path.
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let submenu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        submenu,
        Value::symbol("visual-line-mode"),
        Value::cons(Value::string("Visual Line Mode"), Value::T),
    );

    let menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        menu,
        Value::symbol("line-wrapping"),
        Value::cons(Value::string("Line Wrapping in this Buffer"), submenu),
    );
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: 1 })
        .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    let events = crate::emacs_core::value::list_to_vec(&result).expect("event list");
    assert_eq!(
        events,
        vec![
            Value::symbol("line-wrapping"),
            Value::symbol("visual-line-mode")
        ]
    );
}

#[test]
fn x_popup_menu_interactive_ignores_tty_mouse_navigation() {
    let mut eval = crate::emacs_core::Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let tty_menu_navigation_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        tty_menu_navigation_map,
        Value::symbol("mouse-movement"),
        Value::symbol("tty-menu-select"),
    );
    eval.set_variable("tty-menu-navigation-map", tty_menu_navigation_map);
    eval.set_variable("track-mouse", Value::T);

    let menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        menu,
        Value::symbol("first"),
        Value::cons(Value::string("First"), Value::T),
    );
    tx.send(crate::keyboard::InputEvent::MouseMove {
        x: 8.0,
        y: 18.0,
        modifiers: crate::keyboard::Modifiers::none(),
        target_frame_id: 0,
    })
    .unwrap();
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: -1 })
        .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    assert!(
        result.is_nil(),
        "native popup hover must not run tty-menu-select"
    );
}

#[test]
fn x_popup_menu_interactive_keyboard_select_roots_selected_event() {
    let mut eval = crate::emacs_core::Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        menu,
        Value::symbol("second"),
        Value::cons(Value::string("Second"), Value::T),
    );
    crate::emacs_core::keymap::list_keymap_define(
        menu,
        Value::symbol("first"),
        Value::cons(Value::string("First"), Value::T),
    );
    tx.send(crate::keyboard::InputEvent::KeyPress {
        key: crate::keyboard::KeyEvent::named(crate::keyboard::NamedKey::Down),
        emacs_frame_id: 0,
    })
    .unwrap();
    tx.send(crate::keyboard::InputEvent::KeyPress {
        key: crate::keyboard::KeyEvent::named(crate::keyboard::NamedKey::Return),
        emacs_frame_id: 0,
    })
    .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    let events = crate::emacs_core::value::list_to_vec(&result).expect("event list");
    assert_eq!(events, vec![Value::symbol("second")]);
}

#[test]
fn x_popup_menu_interactive_cancel_returns_nil() {
    let mut eval = crate::emacs_core::Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
    );
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: -1 })
        .unwrap();

    assert!(
        super::builtin_x_popup_menu(
            &mut eval,
            vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
        )
        .unwrap()
        .is_nil()
    );
}

#[test]
fn x_popup_menu_interactive_menu_bar_position_anchors_below_menu_bar() {
    let mut eval = crate::emacs_core::Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    let frame_id = eval.frames.create_frame("popup-owner", 800, 600, scratch);
    eval.frames
        .get_mut(frame_id)
        .expect("frame")
        .menu_bar_height = 24;
    eval.frames.select_frame(frame_id);
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    eval.set_display_host(Box::new(host));

    let menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
    );
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: -1 })
        .unwrap();

    let position = Value::list(vec![
        Value::symbol("file"),
        Value::list(vec![
            Value::make_frame(frame_id.0),
            Value::list(vec![Value::symbol("menu-bar")]),
            Value::list(vec![Value::fixnum(5), Value::fixnum(0)]),
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::list(vec![Value::fixnum(96), Value::fixnum(4)]),
            Value::cons(Value::fixnum(64), Value::fixnum(24)),
        ]),
    ]);
    let result = super::builtin_x_popup_menu(&mut eval, vec![position, menu]).unwrap();

    assert!(result.is_nil());
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0].placement,
        neomacs_display_protocol::PopupPlacement::new(
            neomacs_display_protocol::Rect::new(96.0, 0.0, 64.0, 24.0),
            neomacs_display_protocol::PopupPreferredSide::Below,
            neomacs_display_protocol::Point::ZERO,
            neomacs_display_protocol::PopupConstraintPolicy::FlipAndShift { padding: 4.0 },
        )
    );
}

#[test]
fn x_popup_menu_interactive_menu_bar_position_uses_pending_native_anchor() {
    let mut eval = crate::emacs_core::Context::new();
    let scratch = eval.buffers.create_buffer("*scratch*");
    let frame_id = eval.frames.create_frame("popup-owner", 800, 600, scratch);
    eval.frames
        .get_mut(frame_id)
        .expect("frame")
        .menu_bar_height = 24;
    eval.frames.select_frame(frame_id);
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    eval.set_display_host(Box::new(host));
    eval.pending_menu_bar_popup_anchor = Some(crate::emacs_core::MenuBarPopupAnchor {
        frame_id,
        menu_key: Some("tools".to_string()),
        menu_x: 26,
        x: 244,
        y: 0,
        width: 55,
        height: 18,
    });

    let menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
    );
    tx.send(crate::keyboard::InputEvent::MenuSelection { index: -1 })
        .unwrap();

    let position = Value::list(vec![
        Value::symbol("tools"),
        Value::list(vec![
            Value::make_frame(frame_id.0),
            Value::list(vec![Value::symbol("menu-bar")]),
            Value::list(vec![Value::fixnum(16), Value::fixnum(0)]),
            Value::fixnum(0),
        ]),
    ]);
    let result = super::builtin_x_popup_menu(&mut eval, vec![position, menu]).unwrap();

    assert!(result.is_nil());
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0].placement,
        neomacs_display_protocol::PopupPlacement::new(
            neomacs_display_protocol::Rect::new(244.0, 0.0, 55.0, 18.0),
            neomacs_display_protocol::PopupPreferredSide::Below,
            neomacs_display_protocol::Point::ZERO,
            neomacs_display_protocol::PopupConstraintPolicy::FlipAndShift { padding: 4.0 },
        )
    );
}

#[test]
fn x_popup_menu_interactive_menu_bar_right_returns_next_menu_position() {
    let mut eval = crate::emacs_core::Context::new();
    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let menu_bar = crate::emacs_core::keymap::make_sparse_list_keymap();
    let file_menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    let help_menu = crate::emacs_core::keymap::make_sparse_list_keymap();
    let global_map = crate::emacs_core::keymap::make_sparse_list_keymap();
    crate::emacs_core::keymap::list_keymap_define(
        menu_bar,
        Value::symbol("help-menu"),
        Value::cons(Value::string("Help"), help_menu),
    );
    crate::emacs_core::keymap::list_keymap_define(
        menu_bar,
        Value::symbol("file"),
        Value::cons(Value::string("File"), file_menu),
    );
    crate::emacs_core::keymap::list_keymap_define(global_map, Value::symbol("menu-bar"), menu_bar);
    eval.set_variable("global-map", global_map);
    eval.select_global_map(global_map);
    eval.set_variable(
        "menu-bar-final-items",
        Value::list(vec![Value::symbol("help-menu")]),
    );

    crate::emacs_core::keymap::list_keymap_define(
        file_menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
    );
    tx.send(crate::keyboard::InputEvent::KeyPress {
        key: crate::keyboard::KeyEvent::named(crate::keyboard::NamedKey::Right),
        emacs_frame_id: 0,
    })
    .unwrap();

    let position = Value::list(vec![
        Value::symbol("file"),
        Value::list(vec![
            Value::NIL,
            Value::list(vec![Value::symbol("menu-bar")]),
            Value::list(vec![Value::fixnum(0), Value::fixnum(0)]),
            Value::fixnum(0),
        ]),
    ]);
    let result = super::builtin_x_popup_menu(&mut eval, vec![position, file_menu]).unwrap();

    assert!(
        result.is_cons(),
        "right from File should return menu-bar coordinates"
    );
    assert_eq!(result.cons_car(), Value::fixnum(5));
    assert_eq!(result.cons_cdr(), Value::fixnum(0));
}

// ---------------------------------------------------------------------------
// display-supports-face-attributes-p on a TTY frame
// ---------------------------------------------------------------------------

/// GNU `tty_supports_face_attributes_p` (src/xfaces.c) answers this predicate on
/// a terminal frame from the terminal's own capabilities (`tty_capable_p`), so a
/// package asking whether it may use bold or underline in `-nw` gets the same
/// answer GNU gives. neomacs implemented only the GUI branch and returned nil for
/// everything on a tty -- verified against GNU 31 under TERM=screen-256color,
/// where GNU reports bold and underline supported and italic not.
#[test]
fn tty_frame_supports_the_attributes_the_terminal_can_render() {
    use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;

    let mut eval = crate::emacs_core::Context::new();
    // A terminal frame must exist, as it always does in a real session.
    let buffer = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame = eval.frames.create_frame("F1", 80, 25, buffer);
    eval.frames.select_frame(frame);
    // screen-256color: bold, dim, underline and standout, but no `sitm' and no
    // `smxx'.
    crate::emacs_core::terminal::pure::configure_terminal_runtime(
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::interactive(Some("screen-256color".to_string()), neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256))
        .with_attribute_capabilities(TtyAttributeCapabilities {
            italic_sequence: None,
            strike_through_sequence: None,
            styled_underline: None,
            ..TtyAttributeCapabilities::full()
        }),
    );

    let supported = |eval: &mut crate::emacs_core::Context, attrs: Vec<Value>| -> bool {
        builtin_display_supports_face_attributes_p(eval, vec![Value::list(attrs)])
            .expect("predicate should not signal")
            .is_truthy()
    };

    assert!(
        supported(
            &mut eval,
            vec![Value::keyword("weight"), Value::symbol("bold")]
        ),
        "bold is supported: the terminal has `md'"
    );
    assert!(
        supported(&mut eval, vec![Value::keyword("underline"), Value::T]),
        "underline is supported: the terminal has `us'"
    );
    assert!(
        !supported(
            &mut eval,
            vec![Value::keyword("slant"), Value::symbol("italic")]
        ),
        "italic is NOT supported: the terminal has no `sitm'"
    );
    assert!(
        !supported(&mut eval, vec![Value::keyword("strike-through"), Value::T]),
        "strike-through is NOT supported: the terminal has no `smxx'"
    );
    // GNU rejects these outright on a tty, whatever the capabilities say.
    assert!(
        !supported(&mut eval, vec![Value::keyword("height"), Value::fixnum(2)]),
        ":height is meaningless on a tty"
    );
    assert!(
        !supported(
            &mut eval,
            vec![Value::keyword("family"), Value::string("Monospace")]
        ),
        ":family is meaningless on a tty"
    );
    // "Same as the default face" is not support, per GNU's early returns.
    assert!(
        !supported(
            &mut eval,
            vec![Value::keyword("weight"), Value::symbol("normal")]
        ),
        "the default weight is not a supported difference"
    );

    crate::emacs_core::terminal::pure::reset_terminal_runtime();
}

#[test]
fn a_nil_message_prints_an_empty_line_in_batch_unless_the_cursor_is_in_the_echo_area() {
    // GNU `message_to_stderr' (src/xdisp.c:12579-12602) writes the message text
    // only when it is a string, then emits the trailing newline
    // `if (STRINGP (m) || !cursor_in_echo_area)'.  Its own comment says the
    // consequence: "Log the message M to stderr.  Log an empty line if M is not
    // a string."
    //
    // `(message nil)' reaches it through Fmessage's nil arm -> message1 (0) ->
    // message3 (Qnil) -> message3_nolog -> message_to_stderr, so an echo-area
    // clear is NOT silent in batch.  Measured under GNU 31.0.90
    // (tmp/coord-echo-probe2.el): `(message nil)' emits one blank line, and a
    // keyboard macro emits one per keystroke because the command loop clears
    // the echo area per iteration.
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::builtins::misc_pure::stderr_message_ends_with_newline;

    // A string message always ends with a newline, whatever the cursor does.
    assert!(stderr_message_ends_with_newline(true, false));
    assert!(stderr_message_ends_with_newline(true, true));
    // A nil message prints the bare newline only when the cursor is not in the
    // echo area -- this is the arm that was missing entirely.
    assert!(stderr_message_ends_with_newline(false, false));
    assert!(!stderr_message_ends_with_newline(false, true));
}

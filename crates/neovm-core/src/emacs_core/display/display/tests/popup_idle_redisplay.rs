//! Popup fixtures belong to one test's evaluator and run on its owning thread.
//! Only the TTY redraw request crosses the host/callback boundary, atomically.

use super::*;
use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Models the TTY host's out-of-band painting and deferred full redraw.
/// Each test owns its context; the host and redisplay callback communicate
/// through the same atomic publication used by the real TTY frontend.
struct DeferredTtyPopupHost {
    recording: RecordingPopupHost,
    force_full_redraw: Arc<AtomicBool>,
}

impl DisplayHost for DeferredTtyPopupHost {
    fn realize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        self.recording.realize_gui_frame(request)
    }

    fn resize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        self.recording.resize_gui_frame(request)
    }

    fn show_popup_menu(&mut self, menu: PopupMenuRequest) -> Result<(), String> {
        self.recording.show_popup_menu(menu)
    }

    fn hide_popup_menu(
        &mut self,
        token: neomacs_display_protocol::menu::MenuToken,
    ) -> Result<(), String> {
        self.force_full_redraw.store(true, Ordering::Release);
        self.recording.hide_popup_menu(token)
    }
}

/// Resets the existing per-test-thread knob override even if an assertion fails.
struct IdleSkipOn;

impl IdleSkipOn {
    fn new() -> Self {
        crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(Some(true));
        Self
    }
}

impl Drop for IdleSkipOn {
    fn drop(&mut self) {
        crate::emacs_core::xdisp::set_redisplay_idle_skip_for_test(None);
    }
}

enum PopupExit {
    KeyboardEscapeQuit,
    Select,
    DialogCancel,
}

fn assert_tty_popup_teardown_repaints(exit: PopupExit) {
    let _idle_skip = IdleSkipOn::new();
    let mut eval = Context::new();
    let buffer = eval.buffers.current_buffer_id().unwrap();
    let frame = eval.frames.create_frame("tty-popup", 80, 24, buffer);
    eval.frames.select_frame(frame);
    assert!(
        eval.frames
            .get(frame)
            .unwrap()
            .effective_window_system()
            .is_none()
    );
    eval.eval_str(
        r#"(progn
             (setq global-map (make-sparse-keymap))
             (use-global-map global-map)
             (define-key global-map [27 27 27] 'keyboard-escape-quit))"#,
    )
    .unwrap();
    let menu = eval
        .eval_str(
            "(let ((map (make-sparse-keymap))) (define-key map [a] '(menu-item \"A\" ignore)) map)",
        )
        .unwrap();
    let recording = RecordingPopupHost::default();
    let shown = recording.shown.clone();
    let hidden = recording.hidden.clone();
    let force_full_redraw = Arc::new(AtomicBool::new(false));
    eval.set_display_host(Box::new(DeferredTtyPopupHost {
        recording,
        force_full_redraw: force_full_redraw.clone(),
    }));
    let layouts = Rc::new(Cell::new(0));
    let counter = layouts.clone();
    let full_redraws = Rc::new(Cell::new(0));
    let redraw_counter = full_redraws.clone();
    let redraw_flag = force_full_redraw.clone();
    eval.redisplay_fn = Some(Box::new(move |ctx| {
        counter.set(counter.get() + 1);
        if redraw_flag.swap(false, Ordering::AcqRel) {
            redraw_counter.set(redraw_counter.get() + 1);
        }
        ctx.buffers
            .current_buffer()
            .unwrap()
            .reset_unchanged_region();
        crate::test_utils::mock_redisplay::accept_all_frames(ctx);
    }));
    eval.eval_str("(redisplay t)").unwrap();
    let before = layouts.get();
    eval.eval_str("(redisplay t)").unwrap();
    assert_eq!(
        layouts.get(),
        before,
        "fixture must skip unchanged forced redisplay"
    );
    assert!(eval.current_message.is_none());

    let (tx, rx) = crossbeam_channel::unbounded();
    eval.input_rx = Some(rx);
    match exit {
        PopupExit::KeyboardEscapeQuit => {
            for _ in 0..3 {
                tx.send(crate::keyboard::InputEvent::KeyPress {
                    key: crate::keyboard::KeyEvent::char('\u{1b}'),
                    emacs_frame_id: frame.0,
                })
                .unwrap();
            }
        }
        PopupExit::Select => {
            tx.send(crate::keyboard::InputEvent::KeyPress {
                key: crate::keyboard::KeyEvent::char('\r'),
                emacs_frame_id: frame.0,
            })
            .unwrap();
        }
        PopupExit::DialogCancel => {
            tx.send(crate::keyboard::InputEvent::MenuSelection {
                index: -1,
                token: None,
            })
            .unwrap();
        }
    }
    let result = match exit {
        PopupExit::DialogCancel => {
            let contents = eval.eval_str("'(\"Dialog\" (\"A\" . a))").unwrap();
            super::super::builtin_x_popup_dialog(&mut eval, vec![Value::T, contents, Value::NIL])
        }
        _ => super::super::builtin_x_popup_menu(&mut eval, vec![Value::T, menu]),
    }
    .expect("popup exits without a quit signal");
    if matches!(exit, PopupExit::Select) {
        assert_eq!(
            crate::emacs_core::value::list_to_vec(&result).unwrap(),
            vec![Value::symbol("a")]
        );
    } else {
        assert!(result.is_nil());
    }
    assert_eq!(*hidden.lock().unwrap(), 1);
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert!(shown[0].entries.iter().all(|entry| entry.help.is_none()));
    assert!(
        eval.current_message.is_none(),
        "help must not mask the stale-screen bug"
    );
    assert_eq!(
        layouts.get(),
        before + 1,
        "teardown must run redisplay for an unchanged TTY screen"
    );
    assert_eq!(
        full_redraws.get(),
        1,
        "teardown must consume the TTY host's redraw request"
    );
    assert!(!force_full_redraw.load(Ordering::Acquire));
    eval.eval_str("(redisplay)").unwrap();
    assert_eq!(
        layouts.get(),
        before + 1,
        "ordinary idle redisplay still skips"
    );
}

#[test]
fn tty_popup_keyboard_escape_quit_teardown_repaints_with_idle_skip() {
    assert_tty_popup_teardown_repaints(PopupExit::KeyboardEscapeQuit);
}

#[test]
fn tty_popup_no_help_selection_teardown_repaints_with_idle_skip() {
    assert_tty_popup_teardown_repaints(PopupExit::Select);
}

#[test]
fn tty_popup_dialog_teardown_repaints_with_idle_skip() {
    assert_tty_popup_teardown_repaints(PopupExit::DialogCancel);
}

//! VM tests whose fixture is the system host-input publisher or the
//! display window rect. Eval cannot name those crates. The recording
//! display host moves with them. Test names and assertions are unchanged.
//!
//! Frame and buffer managers are the public accessors. `HostInputPublisher`
//! is core's re-export of the system publisher. The bare harness is core's
//! `new_context`: eval's unit-runtime module stays `cfg(test)`.

use std::cell::RefCell;
use std::rc::Rc;

use neovm_display::window::DisplayFrameManagerExt;
use neovm_value::display_vocab::WindowSystemKind;
use neovm_value::value::Value;

use crate::emacs_core::eval::{Context, DisplayHost, GuiFrameHostRequest, GuiFrameHostSize};

fn vm_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::new_context();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[derive(Clone, Default)]
struct VmRecordingDisplayHost {
    realized: Rc<RefCell<Vec<GuiFrameHostRequest>>>,
    primary_size: Option<GuiFrameHostSize>,
}

impl VmRecordingDisplayHost {
    #[doc(hidden)]
    pub fn with_primary_size(width: u32, height: u32) -> Self {
        Self {
            realized: Rc::default(),
            primary_size: Some(GuiFrameHostSize { width, height }),
        }
    }
}

impl DisplayHost for VmRecordingDisplayHost {
    fn realize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        self.realized.borrow_mut().push(request);
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn current_primary_window_size(&self) -> Option<GuiFrameHostSize> {
        self.primary_size
    }
}

#[test]
fn vm_frame_native_metrics_sync_pending_resize_events() {
    crate::test_utils::init_test_tracing();
    let result = vm_eval_with_init_str(
        r#"(list (frame-native-width) (frame-native-height))"#,
        |eval| {
            let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
            eval.buffer_manager_mut().set_current(scratch);
            let fid = eval
                .frame_manager_mut()
                .create_frame("vm-frame", 960, 640, scratch);
            assert!(eval.frame_manager_mut().select_frame(fid), "selected frame");
            {
                let frame = eval
                    .frame_manager_mut()
                    .get_mut(fid)
                    .expect("frame should exist");
                frame.width = 960;
                frame.height = 640;
                frame.install_backend_less_gui_kind(WindowSystemKind::X);
            }

            let tx = crate::emacs_core::host_input::HostInputPublisher::install(eval);

            tx.send(crate::emacs_core::eval::keyboard::InputEvent::Resize {
                width: 1400,
                height: 1600,
                scale_factor: 1.0,
                emacs_frame_id: 0,
            })
            .expect("queue resize");
        },
    );

    assert_eq!(result, "OK (1400 1600)");
}

#[test]
fn vm_x_create_frame_syncs_pending_resize_before_adopting_opening_gui_frame() {
    crate::test_utils::init_test_tracing();
    let host = VmRecordingDisplayHost::default();
    let requests = host.realized.clone();
    let result = vm_eval_with_init_str(
        "(x-create-frame '((name . \"Neomacs\") (title . \"Neomacs\")))",
        |eval| {
            let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
            let fid = eval
                .frame_manager_mut()
                .create_frame("bootstrap", 960, 640, scratch);
            {
                let frame = eval
                    .frame_manager_mut()
                    .get_mut(fid)
                    .expect("bootstrap frame");
                frame.install_backend_less_gui_kind(WindowSystemKind::X);
                frame.char_width = 10.0;
                frame.char_height = 20.0;
                if let Some(mini_leaf) = frame.minibuffer_leaf.as_mut() {
                    mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
                }
            }
            eval.set_variable("terminal-frame", Value::make_frame(fid.0));
            let tx = crate::emacs_core::host_input::HostInputPublisher::install(eval);

            tx.send(crate::emacs_core::eval::keyboard::InputEvent::Focus {
                focused: true,
                emacs_frame_id: 0,
            })
            .expect("queue focus");
            tx.send(crate::emacs_core::eval::keyboard::InputEvent::Resize {
                width: 1500,
                height: 1900,
                scale_factor: 1.0,
                emacs_frame_id: 0,
            })
            .expect("queue resize");
            eval.set_display_host(Box::new(host.clone()));
        },
    );

    assert!(
        result.starts_with("OK #<frame "),
        "expected x-create-frame to succeed, got: {result}"
    );

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].width, 1500);
    assert_eq!(requests[0].height, 1900);
}

#[test]
fn vm_make_frame_uses_gui_creation_path_when_display_host_is_active() {
    crate::test_utils::init_test_tracing();
    let host = VmRecordingDisplayHost::default();
    let requests = host.realized.clone();
    let result = vm_eval_with_init_str(
        // GNU owns `make-frame' in Lisp (lisp/frame.el:1019) and it funcalls
        // `frame-creation-function', which for a window system reaches the C
        // primitive `x-create-frame' (src/xfns.c:4916).  DIVERGENCES.md 154
        // deleted the Rust `make-frame' subr, whose GUI arm was a call to
        // exactly this.
        "(x-create-frame '((name . \"GUI\") (width . 80) (height . 25)))",
        |eval| {
            let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
            let fid = eval
                .frame_manager_mut()
                .create_frame("bootstrap", 960, 640, scratch);
            {
                let frame = eval
                    .frame_manager_mut()
                    .get_mut(fid)
                    .expect("bootstrap frame");
                frame.set_window_system(Some(Value::symbol("x")));
                frame.char_width = 10.0;
                frame.char_height = 20.0;
                if let Some(mini_leaf) = frame.minibuffer_leaf.as_mut() {
                    mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
                }
            }
            eval.set_variable("terminal-frame", Value::make_frame(fid.0));
            eval.set_display_host(Box::new(host.clone()));
        },
    );

    assert!(
        result.starts_with("OK #<frame "),
        "expected make-frame to succeed, got: {result}"
    );

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].width, 800);
    assert_eq!(requests[0].height, 500);
}

#[test]
fn vm_x_create_frame_prefers_display_host_primary_window_size_when_available() {
    crate::test_utils::init_test_tracing();
    let host = VmRecordingDisplayHost::with_primary_size(1500, 1900);
    let requests = host.realized.clone();
    let result = vm_eval_with_init_str(
        "(x-create-frame '((name . \"Neomacs\") (title . \"Neomacs\")))",
        |eval| {
            let scratch = eval.buffer_manager_mut().create_buffer("*scratch*");
            let fid = eval
                .frame_manager_mut()
                .create_frame("bootstrap", 960, 640, scratch);
            {
                let frame = eval
                    .frame_manager_mut()
                    .get_mut(fid)
                    .expect("bootstrap frame");
                frame.install_backend_less_gui_kind(WindowSystemKind::X);
                frame.char_width = 10.0;
                frame.char_height = 20.0;
                if let Some(mini_leaf) = frame.minibuffer_leaf.as_mut() {
                    mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
                }
            }
            eval.set_variable("terminal-frame", Value::make_frame(fid.0));
            eval.set_display_host(Box::new(host.clone()));
        },
    );

    assert!(
        result.starts_with("OK #<frame "),
        "expected x-create-frame to succeed, got: {result}"
    );

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].width, 1500);
    assert_eq!(requests[0].height, 1900);
}

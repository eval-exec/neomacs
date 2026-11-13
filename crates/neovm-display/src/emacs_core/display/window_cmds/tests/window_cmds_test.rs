use crate::emacs_core::window_cmds::{FrameDomain, SplitWindowSide, WindowDomain};
use crate::face::{FontSlant, FontWeight, FontWidth};
pub use crate::test_utils::window_cmds_host::{
    RecordingDisplayHost, due_gnu_timer, remapped_mono_font_metrics, resolved_frame_font,
};
use crate::window::DisplayFrameTestExt;
use crate::window::display::WindowDisplayContextExt;
use crate::window::{DisplayFrameExt, DisplayFrameManagerExt};
use crate::window::{FrameFullscreen, FrameParam};
use neovm_edit::buffer::{EmacsBytePos, LispCharPos1};
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::DisplayHost;
use neovm_eval::emacs_core::eval::GuiFrameHostRequest;
use neovm_eval::emacs_core::eval::data::minibuffer_entry::MinibufferManagerExt;
use neovm_eval::emacs_core::eval::{FontPxProbeResult, GuiFrameHostSize, ResolvedFrameFont};
use neovm_eval::emacs_core::print::format_eval_result;
use neovm_value::display_vocab::WindowSystemKind;
use neovm_value::error::FlowResultExt as _;
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;
use std::cell::Cell;
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
#[path = "core_hook_defaults_test.rs"]
mod core_hook_defaults;

#[path = "body_geometry_test.rs"]
mod body_geometry_test;
#[path = "frame_position_test.rs"]
mod frame_position_test;
#[path = "frame_resize_test.rs"]
mod frame_resize_test;

#[path = "initial_gui_lifecycle_test.rs"]
mod initial_gui_lifecycle_test;

#[test]
fn gui_frame_position_requests_move_the_native_window() {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let root = ev.frame_manager_mut().create_frame("Root", 800, 600, buf);
    ev.frame_manager_mut()
        .get_mut(root)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    let host = RecordingDisplayHost::new();
    let positions = host.positions.clone();
    ev.set_display_host(Box::new(host));
    let results = ev.eval_str_each(
        "(set-frame-position nil 0 0) \
         (set-frame-size-and-position-pixelwise nil 400 300 120 80) \
         (modify-frame-parameters nil '((left . 10) (top . 20))) \
         (modify-frame-parameters nil '((left . 30))) \
         (modify-frame-parameters nil '((top . 50))) \
         (set-frame-position nil -10 40) \
         (modify-frame-parameters nil '((top . 60))) \
         (modify-frame-parameters nil '((left . (- 0)))) \
         (frame-position)",
    );
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    // Only fully absolute requests reach the host.  A lone edge would take
    // the other edge from the stored position, which is not read back from
    // the native window and here is the unresolved far-edge -10; a far-edge
    // offset needs the window's outer size and the workarea.
    assert_eq!(
        *positions.borrow(),
        vec![(root, 0, 0), (root, 120, 80), (root, 10, 20)]
    );
}

#[test]
fn alpha_lower_limit_variable_projects_without_replaying_alpha_policy() {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let host = RecordingDisplayHost::new();
    let limits = host.alpha_limits.clone();
    let operations = host.alpha_operations.clone();
    ev.set_display_host(Box::new(host));
    let results = ev.eval_str_each("(setq frame-alpha-lower-limit 20) (let ((frame-alpha-lower-limit 60)) (setq frame-alpha-lower-limit 70)) (defvaralias 'alpha-limit-alias 'frame-alpha-lower-limit) (set 'alpha-limit-alias 40)");
    assert!(results.iter().all(Result::is_ok), "{results:?}");
    assert_eq!(*limits.borrow(), vec![0.2, 0.6, 0.7, 0.2, 0.2, 0.4]);
    assert!(operations.borrow().is_empty());
}

#[test]
fn accepted_root_and_child_alpha_operations_survive_numeric_then_nil_without_layout() {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let root = ev.frame_manager_mut().create_frame("Root", 800, 600, buf);
    ev.frame_manager_mut()
        .get_mut(root)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    let host = RecordingDisplayHost::new();
    let operations = host.alpha_operations.clone();
    ev.set_display_host(Box::new(host));
    let values = ev.eval_str_each("(let* ((root (selected-frame)) (child (x-create-frame (list (cons 'parent-frame root))))) (modify-frame-parameters root '((alpha . 50))) (modify-frame-parameters root '((alpha . nil))) (modify-frame-parameters child '((alpha . 50))) (modify-frame-parameters child '((alpha . nil))) (list (frame-parameter root 'alpha) (frame-parameter child 'alpha)))");
    assert_eq!(format_eval_result(&values[0]), "OK (nil nil)");
    let operations = operations.borrow();
    assert_eq!(operations.len(), 5); // child creation, then both accepted setters per frame
    assert_eq!(operations[1].0, root);
    assert_eq!(operations[1].1, [0.5; 2]);
    assert_eq!(operations[2].1, [-1.0; 2]);
    assert_ne!(operations[3].0, root);
    assert_eq!(operations[3].1, [0.5; 2]);
    assert_eq!(operations[4].1, [-1.0; 2]);
}

#[test]
fn x_create_frame_returns_the_realized_frame_when_alpha_sync_fails() {
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let host = RecordingDisplayHost {
        fail_alpha_sync: true,
        ..RecordingDisplayHost::new()
    };
    let realized = host.realized.clone();
    let operations = host.alpha_operations.clone();
    ev.set_display_host(Box::new(host));
    let result = ev.eval_str_each(
        "(let ((frame (x-create-frame '((alpha . 50))))) (list (and (framep frame) t) (and (frame-live-p frame) t) (and (memq frame (frame-list)) t)))",
    );
    assert_eq!(format_eval_result(&result[0]), "OK (t t t)");
    assert_eq!(realized.borrow().len(), 1);
    assert_eq!(operations.borrow().len(), 1);
}

#[test]
fn x_create_frame_resolves_alpha_defaults_and_explicit_nil() {
    for (parameter, value, explicit, expected, background) in [
        ("alpha", "50", "nil", [0.5, 0.5], 1.0),
        ("alpha", "(80 . 40)", "nil", [0.8, 0.4], 1.0),
        ("alpha-background", "50", "nil", [-1.0, -1.0], 0.5),
        ("alpha", "50", "'((alpha . nil))", [-1.0, -1.0], 1.0),
        ("alpha", "invalid", "'((alpha . nil))", [-1.0, -1.0], 1.0),
        (
            "alpha-background",
            "invalid",
            "'((alpha-background . nil))",
            [-1.0, -1.0],
            1.0,
        ),
        (
            "alpha-background",
            "50",
            "'((alpha-background . nil))",
            [-1.0, -1.0],
            1.0,
        ),
    ] {
        let mut ev = Context::new(crate::test_utils::unit_runtime());
        let buf = ev.edit.buffers.create_buffer("*scratch*");
        ev.edit.buffers.set_current(buf);
        ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
        ev.set_display_host(Box::new(RecordingDisplayHost::new()));
        let src = format!(
            "(let ((default-frame-alist '(({parameter} . {value})))) (x-create-frame {explicit}))"
        );
        let result = ev.eval_str_each(&src);
        let frame_value = result[0].as_ref().expect("valid inherited/default alpha");
        let id = crate::window::FrameId(frame_value.as_frame_id().expect("created frame"));
        let frame = ev.frame_manager().get(id).unwrap();
        assert_eq!(frame.frame_alpha, expected, "{src}");
        assert_eq!(frame.background_alpha, background, "{src}");
    }
    for parameter in ["alpha", "alpha-background"] {
        let out = eval_with_gui_frame(&format!(
            "(let ((default-frame-alist '(({parameter} . invalid)))) (condition-case err (x-create-frame nil) (error (car err))))"
        ));
        assert_eq!(out, vec!["OK wrong-type-argument"], "{parameter}");
    }
}

#[test]
fn frame_scale_factor_reads_the_selected_frames_presented_device_scale() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let frame_id = super::ensure_selected_frame_id(&mut eval);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame")
        .device_scale_factor = 1.75;

    let value = crate::emacs_core::frame::builtin_frame_scale_factor(&mut eval, vec![])
        .expect("scale factor");

    assert_eq!(value, Value::make_float(1.75));
}

/// Evaluate all forms with a fresh evaluator that has a frame+window set up.
/// Harness-bound wrapper over the shared upper-layer helper (one home:
/// display test_utils); display tests keep their unit-runtime Context.
fn eval_with_frame(src: &str) -> Vec<String> {
    crate::test_utils::window_cmds_upper_layer_helpers::eval_with_frame(
        || Context::new(crate::test_utils::unit_runtime()),
        src,
    )
}

fn eval_one_with_frame(src: &str) -> String {
    crate::test_utils::window_cmds_upper_layer_helpers::eval_one_with_frame(
        || Context::new(crate::test_utils::unit_runtime()),
        src,
    )
}

fn eval_with_gui_frame(src: &str) -> Vec<String> {
    crate::test_utils::window_cmds_upper_layer_helpers::eval_with_gui_frame(
        || Context::new(crate::test_utils::unit_runtime()),
        src,
    )
}

fn publish_selected_gui_window_regions(
    ev: &mut Context,
    fid: crate::window::FrameId,
    presentation: u64,
    regions: crate::window::PresentedWindowRegions,
) {
    let window_id = ev.frame_manager().get(fid).expect("frame").selected_window;
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .prepare_and_activate_display_presentation_for_test(
            crate::window::geometry::PresentationId::new(presentation),
            vec![crate::window::WindowDisplaySnapshot {
                point_rows: None,
                window_id,
                regions,
                regions_materialized: true,
                ..Default::default()
            }],
        )
        .expect("presented GUI geometry");
}

#[test]
fn active_minibuffer_window_tracks_live_minibuffer_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.frame_manager_mut().create_frame("F1", 800, 600, buf);

    let (ef_frames, ef_buffers) = ev.frame_manager_and_buffers_mut();
    let fid = super::ensure_selected_frame_id_in_state(ef_frames, ef_buffers);
    let minibuffer_buffer_id = {
        let frame = ev.frame_manager().get(fid).expect("selected frame");
        let minibuffer_wid = frame.minibuffer_window.expect("minibuffer window");
        frame
            .find_window(minibuffer_wid)
            .and_then(|window| window.buffer_id())
            .expect("minibuffer buffer")
    };
    ev.minibuffers_mut()
        .read_from_minibuffer(minibuffer_buffer_id, "M-x ", None, None)
        .expect("active minibuffer state");

    let minibuffer_window = super::builtin_minibuffer_window(&mut ev, vec![]).unwrap();
    let active_minibuffer_window =
        super::builtin_active_minibuffer_window(&mut ev, vec![]).unwrap();
    assert_eq!(active_minibuffer_window, minibuffer_window);
    assert!(!active_minibuffer_window.is_nil());
}

// -- Window queries --

#[test]
fn selected_window_returns_window_handle() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(selected-window)");
    assert!(
        r.starts_with("OK #<window "),
        "expected window handle, got: {r}"
    );
}

#[test]
fn selected_window_bootstraps_initial_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each("(window-live-p (selected-window))")
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
}

#[test]
fn frame_selected_window_arity_and_designators() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(windowp (frame-selected-window))
         (windowp (frame-selected-window nil))
         (windowp (frame-selected-window (selected-frame)))
         (condition-case err (frame-selected-window \"x\") (error err))
         (condition-case err (frame-selected-window 999999) (error err))
         (condition-case err (frame-selected-window nil nil) (error (car err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK t");
    assert_eq!(out[2], "OK t");
    assert_eq!(out[3], "OK (wrong-type-argument frame-live-p \"x\")");
    assert_eq!(out[4], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[5], "OK wrong-number-of-arguments");
}

#[test]
fn minibuffer_window_frame_first_window_and_window_minibuffer_p_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
        "(window-minibuffer-p)
         (windowp (minibuffer-window))
         (windowp (minibuffer-window (selected-frame)))
         (window-minibuffer-p (minibuffer-window))
         (eq (frame-first-window) (selected-window))
         (eq (frame-first-window (selected-window)) (selected-window))
         (eq (frame-first-window (minibuffer-window)) (selected-window))
         (eq (minibuffer-window) (car (nthcdr (1- (length (window-list nil t))) (window-list nil t))))
         (condition-case err (minibuffer-window 999999) (error err))
         (condition-case err (window-minibuffer-p 999999) (error err))
         (condition-case err (frame-first-window 999999) (error err))
         (condition-case err (minibuffer-window (selected-window)) (error (car err)))
         (condition-case err (window-minibuffer-p nil nil) (error (car err)))
         (condition-case err (frame-first-window nil nil) (error (car err)))",
    )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK nil");
    assert_eq!(out[1], "OK t");
    assert_eq!(out[2], "OK t");
    assert_eq!(out[3], "OK t");
    assert_eq!(out[4], "OK t");
    assert_eq!(out[5], "OK t");
    assert_eq!(out[6], "OK t");
    assert_eq!(out[7], "OK t");
    assert_eq!(out[8], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[9], "OK (wrong-type-argument window-valid-p 999999)");
    assert_eq!(out[10], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[11], "OK wrong-type-argument");
    assert_eq!(out[12], "OK wrong-number-of-arguments");
    assert_eq!(out[13], "OK wrong-number-of-arguments");
}

#[test]
fn window_at_matches_batch_coordinate_and_error_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(windowp (window-at 0 0))
         (windowp (window-at 79 0))
         (null (window-at 80 0))
         (windowp (window-at 0 23))
         (let ((w (window-at 0 24))) (and w (window-minibuffer-p w)))
         (null (window-at 0 25))
         (null (window-at -1 0))
         (null (window-at 0 -1))
         (windowp (window-at 79.9 0))
         (null (window-at 80.0 0))
         (windowp (window-at 0 24.1))
         (condition-case err (window-at 'foo 0) (error err))
         (condition-case err (window-at 0 'foo) (error err))
         (condition-case err (window-at 0 0 999999) (error err))
         (condition-case err (window-at 0) (error (car err)))
         (condition-case err (window-at 0 0 nil nil) (error (car err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK t");
    assert_eq!(out[2], "OK t");
    assert_eq!(out[3], "OK t");
    assert_eq!(out[4], "OK t");
    assert_eq!(out[5], "OK t");
    assert_eq!(out[6], "OK t");
    assert_eq!(out[7], "OK t");
    assert_eq!(out[8], "OK t");
    assert_eq!(out[9], "OK t");
    assert_eq!(out[10], "OK t");
    assert_eq!(out[11], "OK (wrong-type-argument numberp foo)");
    assert_eq!(out[12], "OK (wrong-type-argument numberp foo)");
    assert_eq!(out[13], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[14], "OK wrong-number-of-arguments");
    assert_eq!(out[15], "OK wrong-number-of-arguments");
}

#[test]
fn window_frame_arity_and_designators() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(framep (window-frame))
         (framep (window-frame nil))
         (framep (window-frame (selected-window)))
         (condition-case err (window-frame \"x\") (error err))
         (condition-case err (window-frame 999999) (error err))
         (condition-case err (window-frame nil nil) (error (car err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK t");
    assert_eq!(out[2], "OK t");
    assert_eq!(out[3], "OK (wrong-type-argument window-valid-p \"x\")");
    assert_eq!(out[4], "OK (wrong-type-argument window-valid-p 999999)");
    assert_eq!(out[5], "OK wrong-number-of-arguments");
}

#[test]
fn window_designators_bootstrap_nil_and_validate_invalid_window_handles() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(window-start nil)
         (window-point nil)
         (window-buffer nil)
         (condition-case err (window-start 999999) (error err))
         (condition-case err (window-buffer 999999) (error err))
         (condition-case err (set-window-start nil 1) (error err))
         (condition-case err (set-window-point nil 1) (error err))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK 1");
    assert_eq!(out[1], "OK 1");
    assert!(
        out[2].starts_with("OK #<buffer "),
        "unexpected value: {}",
        out[2]
    );
    assert_eq!(out[3], "OK (wrong-type-argument window-live-p 999999)");
    assert_eq!(out[4], "OK (wrong-type-argument windowp 999999)");
    assert_eq!(out[5], "OK 1");
    assert_eq!(out[6], "OK 1");
}

#[test]
fn windowp_true() {
    crate::test_utils::init_test_tracing();
    let r = eval_with_frame("(windowp (selected-window))");
    assert_eq!(r[0], "OK t");
}

#[test]
fn windowp_false() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(windowp 999999)");
    assert_eq!(r, "OK nil");
}

#[test]
fn window_live_p_true() {
    crate::test_utils::init_test_tracing();
    let r = eval_with_frame("(window-live-p (selected-window))");
    assert_eq!(r[0], "OK t");
}

#[test]
fn window_live_p_false_for_non_window() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-live-p 999999)");
    assert_eq!(r, "OK nil");
}

#[test]
fn window_start_default() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-start)");
    assert_eq!(r, "OK 1");
}

#[test]
fn window_point_default() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-point)");
    assert_eq!(r, "OK 1");
}

#[test]
fn selected_window_sync_prefers_live_current_buffer_point_before_resync() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut ev);
    let selected_wid = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let buffer_id = ev
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(selected_wid))
        .and_then(|window| window.buffer_id())
        .expect("selected window buffer");

    ev.buffer_manager_mut()
        .replace_buffer_contents(buffer_id, "abc\ndef\n")
        .expect("replace selected buffer contents");
    ev.buffer_manager_mut()
        .goto_buffer_emacs_byte_pos(buffer_id, neovm_edit::buffer::EmacsBytePos::new(4))
        .expect("move selected buffer point");
    if let Some(crate::window::Window::Leaf { point, .. }) = ev
        .frame_manager_mut()
        .get_mut(frame_id)
        .and_then(|frame| frame.find_window_mut(selected_wid))
    {
        *point = LispCharPos1::ONE;
    }

    let pre_buffer_point = ev
        .buffer_manager()
        .get(buffer_id)
        .expect("selected buffer")
        .point_char_pos()
        .get()
        + 1;
    assert_eq!(pre_buffer_point, 5);

    let (rw_frames, rw_buffers) = ev.frame_manager_and_buffers_mut();
    crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
        rw_frames, rw_buffers, frame_id,
    );
    let (sw_frames, sw_buffers) = ev.frame_manager_read_and_buffers_mut();
    crate::emacs_core::window_cmds::sync_selected_window_buffer_in_state(
        sw_frames, sw_buffers, frame_id,
    );

    let selected_point = ev
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(selected_wid))
        .and_then(|window| match window {
            crate::window::Window::Leaf { point, .. } => Some(*point),
            crate::window::Window::Internal { .. } => None,
        })
        .expect("selected window point");
    let buffer_point = ev
        .buffer_manager()
        .get(buffer_id)
        .expect("selected buffer")
        .point_char_pos()
        .get()
        + 1;

    assert_eq!(selected_point, LispCharPos1::from_one_based_usize(5));
    assert_eq!(buffer_point, 5);
}

#[test]
fn window_height_positive() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-total-height)");
    assert!(r.starts_with("OK "));
    let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
    assert!(val > 0, "window-total-height should be positive, got {val}");
}

#[test]
fn window_width_positive() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-body-width)");
    assert!(r.starts_with("OK "));
    let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
    assert!(val > 0, "window-body-width should be positive, got {val}");
}

#[test]
fn window_body_height_pixelwise() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-body-height nil t)");
    assert!(r.starts_with("OK "));
    let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
    // PIXELWISE=t returns pixel height of the root window body.  GNU frames
    // reserve the minibuffer line outside the root window, then exclude the
    // mode-line from the root window's text area.
    assert_eq!(val, 568);
}

#[test]
fn window_body_width_pixelwise() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-body-width nil t)");
    assert!(r.starts_with("OK "));
    let val: i64 = r.strip_prefix("OK ").unwrap().trim().parse().unwrap();
    // PIXELWISE=t returns pixel width (frame 800).
    assert_eq!(val, 800);
}

#[test]
fn window_body_width_remap_without_face_remapping_returns_columns() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-body-width nil 'remap)");
    assert_eq!(r, "OK 100");
}

#[test]
fn window_body_remap_uses_the_current_buffer_default_face_metrics() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.edit.buffers.create_buffer("*remapped-window-body*");
    eval.edit.buffers.set_current(buffer);
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("F1", 800, 600, buffer);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));
    eval.set_display_host(Box::new(RecordingDisplayHost::with_resolved_frame_font(
        remapped_mono_font_metrics(),
    )));

    let value = eval
        .eval_str(
            "(progn
               (set (make-local-variable 'face-remapping-alist)
                    '((default (:height 2.0))))
               (list (window-body-width)
                     (window-body-width nil 'remap)
                     (window-body-height)
                     (window-body-height nil 'remap)))",
        )
        .expect("window body units");

    // Body = 800 px - 2x8 px fringes - the 16 px scroll-bar band, x_set_scroll_bar_default_width
    // (src/xfns.c:2393-2407), so 768 px: 96 columns at 8 px, 48 at the remapped 16 px.
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&value),
        "(96 48 35 17)"
    );
}

#[test]
fn window_body_remap_uses_the_current_buffers_face_remapping_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let displayed = eval.edit.buffers.create_buffer("*displayed-window-body*");
    eval.edit.buffers.set_current(displayed);
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("F1", 800, 600, displayed);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));

    // GNU's `window_body_width` reads the buffer-local
    // `Vface_remapping_alist` of the current buffer.  The WINDOW argument
    // supplies geometry; it does not switch buffers as a side effect.
    let query_buffer = eval.edit.buffers.create_buffer("*query-window-body*");
    eval.edit.buffers.set_current(query_buffer);
    eval.set_display_host(Box::new(RecordingDisplayHost::with_resolved_frame_font(
        remapped_mono_font_metrics(),
    )));

    let value = eval
        .eval_str(
            "(progn
               (set (make-local-variable 'face-remapping-alist)
                    '((default (:height 2.0))))
               (window-body-width nil 'remap))",
        )
        .expect("remapped width");

    assert_eq!(value, Value::fixnum(48));
}

#[test]
fn batch_window_body_width_excludes_margins() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame(
        "(progn
           (set-window-margins nil 3 2)
           (list (window-total-width) (window-body-width) (window-body-width nil t)))",
    );
    assert_eq!(r, "OK (100 95 760)");
}

#[test]
fn gui_window_body_geometry_excludes_fringes_and_margins() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }

    assert_eq!(
        super::builtin_set_window_margins(
            &mut ev,
            vec![Value::NIL, Value::fixnum(1), Value::fixnum(2)]
        )
        .expect("set-window-margins"),
        Value::T
    );
    assert_eq!(
        super::builtin_set_window_fringes(
            &mut ev,
            vec![Value::NIL, Value::fixnum(8), Value::fixnum(12)]
        )
        .expect("set-window-fringes"),
        Value::T
    );
    let window_id = ev.frame_manager().get(fid).expect("frame").selected_window;
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .prepare_and_activate_display_presentation_for_test(
            crate::window::geometry::PresentationId::new(1),
            vec![crate::window::WindowDisplaySnapshot {
                point_rows: None,
                window_id,
                regions: crate::window::PresentedWindowRegions {
                    outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
                    text_body: neomacs_display_protocol::types::Rect::new(16.0, 0.0, 748.0, 568.0),
                    left_margin_columns: 1,
                    right_margin_columns: 2,
                    left_margin: Some(neomacs_display_protocol::types::Rect::new(
                        0.0, 0.0, 8.0, 568.0,
                    )),
                    right_margin: Some(neomacs_display_protocol::types::Rect::new(
                        764.0, 0.0, 16.0, 568.0,
                    )),
                    left_fringe: Some(neomacs_display_protocol::types::Rect::new(
                        8.0, 0.0, 8.0, 568.0,
                    )),
                    right_fringe: Some(neomacs_display_protocol::types::Rect::new(
                        780.0, 0.0, 12.0, 568.0,
                    )),
                    right_scroll_bar: Some(neomacs_display_protocol::types::Rect::new(
                        792.0, 0.0, 8.0, 568.0,
                    )),
                    mode_line: Some(neomacs_display_protocol::types::Rect::new(
                        0.0, 568.0, 800.0, 16.0,
                    )),
                    ..Default::default()
                },
                regions_materialized: true,
                ..Default::default()
            }],
        )
        .expect("presented GUI geometry");
    assert_eq!(
        super::builtin_window_body_width(&mut ev, &vec![Value::NIL, Value::T])
            .expect("window-body-width"),
        // GNU `window-body-width` also subtracts the frame-default right
        // scroll-bar area when a GUI frame inherits `vertical-scroll-bar = t`.
        Value::fixnum(748)
    );
    assert_eq!(
        super::builtin_window_text_width(&mut ev, vec![Value::NIL, Value::T])
            .expect("window-text-width"),
        Value::fixnum(748)
    );
    // `window-edges' is lisp/window.el:3839 and has no Rust subr any more
    // (DIVERGENCES.md 154).  Its BODY/PIXELWISE right edge is
    // `left-body + (window-body-width W t)' and its bottom is
    // `top-body + (window-body-height W t)', so the two numbers it would have
    // added come from these C primitives; the width is asserted above.
    assert_eq!(
        super::builtin_window_body_height(&mut ev, &vec![Value::NIL, Value::T])
            .expect("window-body-height"),
        Value::fixnum(568)
    );
    assert_eq!(
        super::builtin_window_fringes(&mut ev, &vec![Value::NIL]).expect("window-fringes"),
        Value::list(vec![
            Value::fixnum(8),
            Value::fixnum(12),
            Value::NIL,
            Value::NIL,
        ])
    );
    assert_eq!(
        super::builtin_window_margins(&mut ev, &vec![Value::NIL]).expect("window-margins"),
        Value::cons(Value::fixnum(1), Value::fixnum(2))
    );
}

#[test]
fn gui_window_fringes_and_margins_read_new_values_before_redisplay() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));
    super::builtin_set_window_margins(
        &mut ev,
        vec![Value::NIL, Value::fixnum(8), Value::fixnum(8)],
    )
    .expect("initial margins");
    publish_selected_gui_window_regions(
        &mut ev,
        fid,
        1,
        crate::window::PresentedWindowRegions {
            outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
            text_body: neomacs_display_protocol::types::Rect::new(72.0, 0.0, 656.0, 568.0),
            left_margin_columns: 8,
            right_margin_columns: 8,
            left_fringe: Some(neomacs_display_protocol::types::Rect::new(
                0.0, 0.0, 8.0, 568.0,
            )),
            right_fringe: Some(neomacs_display_protocol::types::Rect::new(
                784.0, 0.0, 8.0, 568.0,
            )),
            ..Default::default()
        },
    );

    super::builtin_set_window_fringes(
        &mut ev,
        vec![Value::NIL, Value::fixnum(123), Value::fixnum(45), Value::T],
    )
    .expect("set fringes");
    super::builtin_set_window_margins(
        &mut ev,
        vec![Value::NIL, Value::fixnum(0), Value::fixnum(3)],
    )
    .expect("set margins");

    // GNU reads the window's own fields, so no redisplay is needed.
    assert_eq!(
        super::builtin_window_fringes(&mut ev, &vec![Value::NIL]).expect("window-fringes"),
        Value::list(vec![
            Value::fixnum(123),
            Value::fixnum(45),
            Value::T,
            Value::NIL,
        ])
    );
    assert_eq!(
        super::builtin_window_margins(&mut ev, &vec![Value::NIL]).expect("window-margins"),
        Value::cons(Value::NIL, Value::fixnum(3))
    );
}

#[test]
fn gui_window_fringes_default_to_frame_defaults_when_reset() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));

    let regions_with_fringes = |left_width: f32, right_width: f32| {
        let left_fringe = (left_width > 0.0)
            .then(|| neomacs_display_protocol::types::Rect::new(0.0, 0.0, left_width, 568.0));
        let right_fringe = (right_width > 0.0).then(|| {
            neomacs_display_protocol::types::Rect::new(792.0 - right_width, 0.0, right_width, 568.0)
        });
        crate::window::PresentedWindowRegions {
            outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
            text_body: neomacs_display_protocol::types::Rect::new(
                left_width,
                0.0,
                792.0 - left_width - right_width,
                568.0,
            ),
            left_fringe,
            right_fringe,
            right_scroll_bar: Some(neomacs_display_protocol::types::Rect::new(
                792.0, 0.0, 8.0, 568.0,
            )),
            mode_line: Some(neomacs_display_protocol::types::Rect::new(
                0.0, 568.0, 800.0, 16.0,
            )),
            ..Default::default()
        }
    };

    publish_selected_gui_window_regions(&mut ev, fid, 1, regions_with_fringes(8.0, 8.0));
    assert_eq!(
        super::builtin_window_fringes(&mut ev, &vec![Value::NIL]).expect("default fringes"),
        Value::list(vec![
            Value::fixnum(8),
            Value::fixnum(8),
            Value::NIL,
            Value::NIL,
        ])
    );

    super::builtin_set_window_fringes(
        &mut ev,
        vec![Value::NIL, Value::fixnum(0), Value::fixnum(4)],
    )
    .expect("set explicit fringes");
    publish_selected_gui_window_regions(&mut ev, fid, 2, regions_with_fringes(0.0, 4.0));
    assert_eq!(
        super::builtin_window_fringes(&mut ev, &vec![Value::NIL]).expect("explicit fringes"),
        Value::list(vec![
            Value::fixnum(0),
            Value::fixnum(4),
            Value::NIL,
            Value::NIL,
        ])
    );

    super::builtin_set_window_fringes(&mut ev, vec![Value::NIL, Value::NIL, Value::NIL])
        .expect("reset fringes");
    publish_selected_gui_window_regions(&mut ev, fid, 3, regions_with_fringes(8.0, 8.0));
    assert_eq!(
        super::builtin_window_fringes(&mut ev, &vec![Value::NIL]).expect("reset fringes"),
        Value::list(vec![
            Value::fixnum(8),
            Value::fixnum(8),
            Value::NIL,
            Value::NIL,
        ])
    );
}

#[test]
fn gui_window_scroll_bars_round_trip_explicit_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));

    assert_eq!(
        ev.eval_str("(window-scroll-bars)")
            .map(|value| neovm_eval::emacs_core::print::print_value(&value))
            .expect("default scroll bars"),
        // COLUMNS is FRAME_CONFIG_SCROLL_BAR_COLS = ceil(16 / 8) = 2 for the 16 px toolkit
        // default (src/xfns.c:2393-2407, src/window.h:934-939).
        "(nil 2 t nil 0 t nil)"
    );
    ev.eval_str("(set-window-scroll-bars nil 13 'left 9 'bottom t)")
        .expect("set scroll bars");
    publish_selected_gui_window_regions(
        &mut ev,
        fid,
        1,
        crate::window::PresentedWindowRegions {
            outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
            text_body: neomacs_display_protocol::types::Rect::new(13.0, 0.0, 787.0, 575.0),
            left_scroll_bar: Some(neomacs_display_protocol::types::Rect::new(
                0.0, 0.0, 13.0, 575.0,
            )),
            horizontal_scroll_bar: Some(neomacs_display_protocol::types::Rect::new(
                0.0, 575.0, 800.0, 9.0,
            )),
            mode_line: Some(neomacs_display_protocol::types::Rect::new(
                0.0, 584.0, 800.0, 16.0,
            )),
            ..Default::default()
        },
    );
    assert_eq!(
        ev.eval_str(
            "(list (window-scroll-bars)
                   (window-scroll-bar-width)
                   (window-scroll-bar-height))",
        )
        .map(|value| neovm_eval::emacs_core::print::print_value(&value))
        .expect("realized scroll bars"),
        "((13 2 left 9 1 bottom t) 13 9)"
    );
}

#[test]
fn gui_window_body_geometry_excludes_scroll_bar_area() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));
    ev.eval_str("(set-window-scroll-bars nil 13 'left)")
        .expect("set scroll bars");
    publish_selected_gui_window_regions(
        &mut ev,
        fid,
        1,
        crate::window::PresentedWindowRegions {
            outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
            text_body: neomacs_display_protocol::types::Rect::new(21.0, 0.0, 771.0, 568.0),
            left_fringe: Some(neomacs_display_protocol::types::Rect::new(
                13.0, 0.0, 8.0, 568.0,
            )),
            right_fringe: Some(neomacs_display_protocol::types::Rect::new(
                792.0, 0.0, 8.0, 568.0,
            )),
            left_scroll_bar: Some(neomacs_display_protocol::types::Rect::new(
                0.0, 0.0, 13.0, 568.0,
            )),
            mode_line: Some(neomacs_display_protocol::types::Rect::new(
                0.0, 568.0, 800.0, 16.0,
            )),
            ..Default::default()
        },
    );
    assert_eq!(
        ev.eval_str("(list (window-body-width nil t) (window-text-width nil t))")
            .map(|value| neovm_eval::emacs_core::print::print_value(&value))
            .expect("body geometry"),
        "(771 771)"
    );
}

#[test]
fn gui_set_window_buffer_applies_buffer_local_display_defaults() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(scratch);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, scratch);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    let buffer_name = " *gui-swb-display*";
    let buffer_id = ev.edit.buffers.create_buffer(buffer_name);
    ev.edit
        .buffers
        .set_buffer_local_property(buffer_id, "left-fringe-width", Value::fixnum(3))
        .expect("left fringe");
    ev.edit
        .buffers
        .set_buffer_local_property(buffer_id, "right-fringe-width", Value::fixnum(5))
        .expect("right fringe");
    ev.edit
        .buffers
        .set_buffer_local_property(buffer_id, "fringes-outside-margins", Value::T)
        .expect("outside margins");
    ev.edit
        .buffers
        .set_buffer_local_property(buffer_id, "scroll-bar-width", Value::fixnum(11))
        .expect("scroll bar width");
    ev.edit
        .buffers
        .set_buffer_local_property(buffer_id, "vertical-scroll-bar", Value::symbol("left"))
        .expect("vertical scroll bar");
    ev.edit
        .buffers
        .set_buffer_local_property(buffer_id, "scroll-bar-height", Value::fixnum(7))
        .expect("scroll bar height");
    ev.edit
        .buffers
        .set_buffer_local_property(buffer_id, "horizontal-scroll-bar", Value::symbol("bottom"))
        .expect("horizontal scroll bar");

    ev.eval_str("(set-window-buffer (selected-window) \" *gui-swb-display*\")")
        .expect("set-window-buffer");
    let window_id = ev.frame_manager().get(fid).expect("frame").selected_window;
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .prepare_and_activate_display_presentation_for_test(
            crate::window::geometry::PresentationId::new(1),
            vec![crate::window::WindowDisplaySnapshot {
                point_rows: None,
                window_id,
                regions: crate::window::PresentedWindowRegions {
                    outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
                    text_body: neomacs_display_protocol::types::Rect::new(14.0, 0.0, 781.0, 577.0),
                    left_fringe: Some(neomacs_display_protocol::types::Rect::new(
                        11.0, 0.0, 3.0, 577.0,
                    )),
                    right_fringe: Some(neomacs_display_protocol::types::Rect::new(
                        795.0, 0.0, 5.0, 577.0,
                    )),
                    left_scroll_bar: Some(neomacs_display_protocol::types::Rect::new(
                        0.0, 0.0, 11.0, 577.0,
                    )),
                    horizontal_scroll_bar: Some(neomacs_display_protocol::types::Rect::new(
                        0.0, 577.0, 800.0, 7.0,
                    )),
                    mode_line: Some(neomacs_display_protocol::types::Rect::new(
                        0.0, 584.0, 800.0, 16.0,
                    )),
                    ..Default::default()
                },
                regions_materialized: true,
                ..Default::default()
            }],
        )
        .expect("presented GUI geometry");
    let out = ev
        .eval_str(
            "(let ((w (selected-window)))
               (list (window-fringes w)
                     (window-scroll-bars w)
                     (window-scroll-bar-width w)
                     (window-scroll-bar-height w)))",
        )
        .map(|value| neovm_eval::emacs_core::print::print_value(&value))
        .expect("geometry query");
    assert_eq!(out, "((3 5 t nil) (11 2 left 7 1 bottom nil) 11 7)");
}

#[test]
fn window_total_size_queries_work() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(list (integerp (window-total-height))
               (integerp (window-total-width))
               (integerp (window-total-height nil t))
               (integerp (window-total-width nil t)))",
    );
    assert_eq!(results[0], "OK (t t t t)");
}

#[test]
fn get_buffer_window_finds_selected_window_for_current_buffer() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        "(let ((w (selected-window)))
           (eq w (get-buffer-window (window-buffer w))))",
    );
    assert_eq!(result, "OK t");
}

#[test]
fn get_buffer_window_list_includes_active_minibuffer_by_default() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let (sb_frames, sb_buffers) = ev.frame_manager_and_buffers_mut();
    let fid = super::seed_batch_startup_frame_in_state(sb_frames, sb_buffers);
    let minibuffer_buffer = {
        let frame = ev.frame_manager().get(fid).expect("frame should exist");
        let minibuffer_wid = frame.minibuffer_window.expect("minibuffer window");
        frame
            .find_window(minibuffer_wid)
            .and_then(|window| window.buffer_id())
            .expect("minibuffer buffer")
    };
    ev.minibuffers_mut()
        .read_from_minibuffer(minibuffer_buffer, "M-x ", None, None)
        .expect("active minibuffer state");
    ev.edit.buffers.set_current(minibuffer_buffer);

    let active_minibuffer =
        super::builtin_active_minibuffer_window(&mut ev, vec![]).expect("active minibuffer");
    let default_list =
        super::builtin_window_list_1(&mut ev, &vec![Value::NIL, Value::NIL, Value::NIL])
            .expect("window-list-1 with nil minibuf");
    let default_windows =
        neovm_value::value::list_to_vec(&default_list).expect("default window list");
    assert!(
        default_windows.contains(&active_minibuffer),
        "nil MINIBUF should include the active minibuffer"
    );

    let excluded_list =
        super::builtin_window_list_1(&mut ev, &vec![Value::NIL, Value::symbol("not"), Value::NIL])
            .expect("window-list-1 with non-nil non-t minibuf");
    let excluded_windows =
        neovm_value::value::list_to_vec(&excluded_list).expect("excluded window list");
    assert!(
        !excluded_windows.contains(&active_minibuffer),
        "non-t MINIBUF should exclude the active minibuffer"
    );
}

#[test]
fn applying_character_totals_preserves_pixel_geometry() {
    crate::test_utils::init_test_tracing();
    let result = eval_with_gui_frame(
        "(let* ((w (selected-window))
                (edges (list (window-pixel-width w) (window-pixel-height w))))
           (set-window-new-total w 3)
           (window-resize-apply-total nil nil)
           (list (equal edges (list (window-pixel-width w) (window-pixel-height w)))
                 (window-total-height w)))",
    );
    assert_eq!(result, vec!["OK (t 3)"]);
}

#[test]
fn rounded_window_totals_measure_pixels_independently_of_assigned_cells() {
    let result = eval_with_gui_frame(
        "(let* ((w (selected-window))
                (height (window-total-height w 'floor))
                (width (window-total-width w 'ceiling)))
           (set-window-new-total w 3)
           (window-resize-apply-total nil nil)
           (set-window-new-total w 4)
           (window-resize-apply-total nil t)
           (list (window-total-height w) (window-total-width w)
                 (= height (window-total-height w 'floor))
                 (= width (window-total-width w 'ceiling))))",
    );
    assert_eq!(result, vec!["OK (3 4 t t)"]);
}

#[test]
fn window_list_1_callable_paths_return_live_windows() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame(
        "(let* ((fn (indirect-function 'window-list-1))
                (a (funcall #'window-list-1 nil nil))
                (b (apply #'window-list-1 '(nil nil)))
                (c (funcall fn nil nil)))
           (list (listp a)
                 (consp a)
                 (equal a b)
                 (equal a c)
                 (null (memq nil (mapcar #'windowp a)))))",
    );
    assert_eq!(r, "OK (t t t t t)");
}

#[test]
fn window_list_returns_list() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(listp (window-list))");
    assert_eq!(r, "OK t");
}

#[test]
fn window_list_has_one_entry() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(length (window-list))");
    assert_eq!(r, "OK 1");
}

#[test]
fn window_list_matches_frame_minibuffer_and_all_frames_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(condition-case err (length (window-list)) (error err))
         (condition-case err (length (window-list (selected-frame))) (error err))
         (condition-case err (window-list 999999) (error err))
         (condition-case err (window-list 'foo) (error err))
         (condition-case err (window-list (selected-window)) (error err))
         (condition-case err (window-list 999999 nil t) (error err))
         (condition-case err (window-list nil nil t) (error err))
         (condition-case err (window-list nil nil 0) (error err))
         (length (window-list nil t))
         (length (window-list (selected-frame) t))
         (length (window-list nil nil (selected-window)))
         (length (window-list nil t (selected-window)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK 1");
    assert_eq!(out[1], "OK 1");
    assert_eq!(out[2], "OK (error \"Window is on a different frame\")");
    assert_eq!(out[3], "OK (error \"Window is on a different frame\")");
    assert_eq!(out[4], "OK (error \"Window is on a different frame\")");
    assert_eq!(out[5], "OK (wrong-type-argument windowp t)");
    assert_eq!(out[6], "OK (wrong-type-argument windowp t)");
    assert_eq!(out[7], "OK (wrong-type-argument windowp 0)");
    assert_eq!(out[8], "OK 2");
    assert_eq!(out[9], "OK 2");
    assert_eq!(out[10], "OK 1");
    assert_eq!(out[11], "OK 2");
}

#[test]
fn window_dedicated_p_default() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-dedicated-p)");
    assert_eq!(r, "OK nil");
}

#[test]
fn window_accessors_enforce_max_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(condition-case err (window-buffer nil nil) (error (car err)))
         (condition-case err (window-start nil nil) (error (car err)))
         (condition-case err (window-end nil nil nil) (error (car err)))
         (condition-case err (window-point nil nil) (error (car err)))
         (condition-case err (window-dedicated-p nil nil) (error (car err)))
         (condition-case err (set-window-start nil 1 nil nil) (error (car err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK wrong-number-of-arguments");
    assert_eq!(out[2], "OK wrong-number-of-arguments");
    assert_eq!(out[3], "OK wrong-number-of-arguments");
    assert_eq!(out[4], "OK wrong-number-of-arguments");
    assert_eq!(out[5], "OK wrong-number-of-arguments");
}

#[test]
fn set_window_dedicated_p() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(set-window-dedicated-p (selected-window) t)
         (window-dedicated-p)",
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK t");
}

#[test]
fn set_window_dedicated_p_bootstraps_nil_and_validates_designators() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(condition-case err (set-window-dedicated-p nil t) (error err))
         (window-dedicated-p nil)
         (condition-case err (set-window-dedicated-p 'foo t) (error err))
         (condition-case err (set-window-dedicated-p 999999 t) (error err))
         (condition-case err (set-window-dedicated-p nil nil) (error err))
         (window-dedicated-p nil)",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK t");
    assert_eq!(out[2], "OK (wrong-type-argument window-live-p foo)");
    assert_eq!(out[3], "OK (wrong-type-argument window-live-p 999999)");
    assert_eq!(out[4], "OK nil");
    assert_eq!(out[5], "OK nil");
}

// -- Window manipulation --

#[test]
fn split_window_side_domain_matches_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        SplitWindowSide::from_side_argument(&Value::NIL),
        SplitWindowSide::Below
    );
    assert_eq!(
        SplitWindowSide::from_side_argument(&Value::T),
        SplitWindowSide::Right
    );
    assert_eq!(
        SplitWindowSide::from_side_argument(&Value::symbol("above")),
        SplitWindowSide::Above
    );
    assert_eq!(
        SplitWindowSide::from_side_argument(&Value::symbol("below")),
        SplitWindowSide::Below
    );
    assert_eq!(
        SplitWindowSide::from_side_argument(&Value::symbol("left")),
        SplitWindowSide::Left
    );
    assert_eq!(
        SplitWindowSide::from_side_argument(&Value::symbol("right")),
        SplitWindowSide::Right
    );
    assert_eq!(SplitWindowSide::Right.name(), "right");
    assert!(SplitWindowSide::Right.is_horizontal());
    assert!(SplitWindowSide::Left.is_horizontal());
    assert!(!SplitWindowSide::Above.is_horizontal());
    assert!(!SplitWindowSide::Below.is_horizontal());
    // Unrecognised values are not an error, they are `below' -- GNU's
    // `horflag'/before tests are `EQ' against a closed set and reject nothing,
    // so a stray symbol, a fixnum and a string all split vertically.
    assert_eq!(
        SplitWindowSide::from_side_argument(&Value::symbol("other")),
        SplitWindowSide::Below
    );
    assert_eq!(
        SplitWindowSide::from_side_argument(&Value::fixnum(9)),
        SplitWindowSide::Below
    );
    assert_eq!(
        SplitWindowSide::from_side_argument(&Value::string("x")),
        SplitWindowSide::Below
    );
}

/// Which decoder GNU opens each single-argument WINDOW subr with, extracted
/// from `src/window.c`, `src/dispnew.c`, `src/xdisp.c` and `src/minibuf.c` by
/// splitting on `DEFUN (` and reading the `decode_*_window` / `CHECK_*_WINDOW`
/// in each body.
///
/// This table IS the contract.  Keeping it in the tree turns what used to be a
/// 336-process differential probe against a GNU binary into an ordinary unit
/// test -- one that runs on every commit, needs no GNU build, and cannot be
/// fooled by the three ways that probe produced false results (a
/// build-configuration `#ifdef`, mutating subrs contaminating each other inside
/// one process, and an argument-position assumption that hid
/// `buffer-text-pixel-size` entirely).

/// Which decoder GNU opens each FRAME-taking subr with, from `src/frame.c`,
/// `src/window.c`, `src/dispnew.c` and `src/xdisp.c`.
///
/// `decode_any_frame` is `CHECK_FRAME` -- a bare type test that a DELETED frame
/// still passes -- while `decode_live_frame` is `CHECK_LIVE_FRAME`, i.e.
/// `FRAME_LIVE_P`, testing `f->terminal` (`src/frame.c:98,107`).
///
/// Four subrs GNU decodes here are deliberately ABSENT: `frame-char-height`,
/// `frame-char-width`, `tool-bar-height` and `tool-bar-pixel-width` all decode
/// inside an `#ifdef`, so the reference binary's answer depends on how it was
/// configured rather than on the contract.  `tool-bar-height` is the one that
/// wasted an afternoon earlier in this campaign, looking for all the world like
/// a divergence; the extractor that built this table now detects the guard and
/// drops such subrs automatically, so that mistake cannot be repeated by hand.

/// The multi-argument half of the same contract.  WINDOW is not always the
/// first argument -- `buffer-text-pixel-size` and `window-line-height` take it
/// second, `format-mode-line` third -- and that is not a detail: the
/// differential probe this replaces only ever put its value in argument 0, and
/// `buffer-text-pixel-size` carried a real divergence it therefore could not
/// see.  The third field is the Lisp argument list, `{w}` marking the slot.

#[test]
fn select_window_validates_designators_and_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(condition-case err (select-window nil) (error err))
         (condition-case err (select-window 'foo) (error err))
         (condition-case err (select-window 999999) (error err))
         (windowp (select-window (selected-window)))
         (condition-case err (select-window (selected-window) nil nil) (error (car err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (wrong-type-argument window-live-p nil)");
    assert_eq!(out[1], "OK (wrong-type-argument window-live-p foo)");
    assert_eq!(out[2], "OK (wrong-type-argument window-live-p 999999)");
    assert_eq!(out[3], "OK t");
    assert_eq!(out[4], "OK wrong-number-of-arguments");
}

#[test]
fn selected_frame_bootstraps_initial_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let results = ev.eval_str_each("(list (framep (selected-frame)) (length (frame-list)))");
    assert_eq!(format_eval_result(&results[0]), "OK (t 1)");
}

#[test]
fn window_size_queries_bootstrap_initial_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let results = ev.eval_str_each(
        "(list (integerp (window-total-height))
               (integerp (window-total-width))
               (integerp (window-body-height))
               (integerp (window-body-width)))",
    );
    assert_eq!(format_eval_result(&results[0]), "OK (t t t t)");
}

#[test]
fn window_size_queries_match_batch_defaults_and_invalid_window_predicates() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(window-total-height nil)
         (window-total-width nil)
         (window-body-height nil)
         (window-body-width nil)
         (condition-case err (window-total-height 999999) (error err))
         (condition-case err (window-total-width 999999) (error err))
         (condition-case err (window-body-height 999999) (error err))
         (condition-case err (window-body-width 999999) (error err))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK 24");
    assert_eq!(out[1], "OK 80");
    assert_eq!(out[2], "OK 23");
    assert_eq!(out[3], "OK 80");
    assert_eq!(out[4], "OK (wrong-type-argument window-valid-p 999999)");
    assert_eq!(out[5], "OK (wrong-type-argument window-valid-p 999999)");
    assert_eq!(out[6], "OK (wrong-type-argument window-live-p 999999)");
    assert_eq!(out[7], "OK (wrong-type-argument window-live-p 999999)");
}

#[test]
fn window_geometry_helper_queries_match_batch_defaults_and_error_predicates() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(let* ((w (selected-window))
                (m (minibuffer-window)))
           (list (window-left-column w)
                 (window-left-column m)
                 (window-top-line w)
                 (window-top-line m)
                 (window-hscroll w)
                 (window-hscroll m)
                 (window-margins w)
                 (window-margins m)
                 (window-fringes w)
                 (window-fringes m)
                 (window-scroll-bars w)
                 (window-scroll-bars m)))
         (list (condition-case err (window-left-column 999999) (error err))
               (condition-case err (window-top-line 999999) (error err))
               (condition-case err (window-hscroll 999999) (error err))
               (condition-case err (window-margins 999999) (error err))
               (condition-case err (window-fringes 999999) (error err))
               (condition-case err (window-scroll-bars 999999) (error err))
               (condition-case err (window-left-column nil nil) (error err))
               (condition-case err (window-top-line nil nil) (error err))
               (condition-case err (window-hscroll nil nil) (error err))
               (condition-case err (window-margins nil nil) (error err))
               (condition-case err (window-fringes nil nil) (error err))
               (condition-case err (window-scroll-bars nil nil) (error err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        out[0],
        "OK (0 0 0 24 0 0 (nil) (nil) (0 0 nil nil) (0 0 nil nil) (nil 0 t nil 0 t nil) (nil 0 t nil 0 t nil))"
    );
    assert_eq!(
        out[1],
        "OK ((wrong-type-argument window-valid-p 999999) (wrong-type-argument window-valid-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-number-of-arguments window-left-column 2) (wrong-number-of-arguments window-top-line 2) (wrong-number-of-arguments window-hscroll 2) (wrong-number-of-arguments window-margins 2) (wrong-number-of-arguments window-fringes 2) (wrong-number-of-arguments window-scroll-bars 2))"
    );
}

#[test]
fn window_scroll_state_shared_state_smoke() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(let* ((w (selected-window))
                (m (minibuffer-window)))
           (list (window-vscroll w)
                 (window-vscroll m)
                 (window-vscroll w t)
                 (window-vscroll m t)
                 (set-window-vscroll w 1)
                 (set-window-vscroll w 2 t)
                 (set-window-vscroll w 3 t t)
                 (set-window-vscroll nil 1.5)
                 (window-vscroll w)
                 (window-vscroll w t)
                 (window-hscroll w)
                 (set-window-hscroll w 3)
                 (window-hscroll w)
                 (set-window-hscroll w -1)
                 (window-hscroll w)
                 (set-window-hscroll w ?a)
                 (window-hscroll w)
                 (window-margins w)
                 (set-window-margins w 1 2)
                 (window-margins w)
                 (set-window-margins w 1 2)
                 (set-window-margins w nil nil)
                 (window-margins w)
                 (set-window-margins w 3)
                 (window-margins w)
                 (set-window-margins w 3)
                 (window-fringes w)
                 (window-fringes m)
                 (set-window-fringes w 0 0)
                 (set-window-fringes w 1 2)
                 (set-window-fringes w nil nil)
                 (window-fringes w)
                 (window-scroll-bars w)
                 (window-scroll-bars m)
                 (set-window-scroll-bars w nil nil nil nil)
                 (set-window-scroll-bars w 'left)
                 (window-scroll-bars w)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        out[0],
        "OK (0 0 0 0 0 0 0 0 0 0 0 3 3 0 0 97 97 (nil) t (1 . 2) nil t (nil) t (3) nil (0 0 nil nil) (0 0 nil nil) nil nil nil (0 0 nil nil) (nil 0 t nil 0 t nil) (nil 0 t nil 0 t nil) nil nil (nil 0 t nil 0 t nil))"
    );
}

#[test]
fn window_parameter_helpers_match_batch_defaults_and_key_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(let* ((w (selected-window))
                (m (minibuffer-window)))
           (list (window-parameters w)
                 (window-parameters m)
                 (window-parameter w 'foo)
                 (window-parameter m 'foo)
                 (set-window-parameter w 'foo 'bar)
                 (window-parameter w 'foo)
                 (window-parameters w)
                 (set-window-parameter m 'foo 42)
                 (window-parameter m 'foo)
                 (window-parameters m)
                 (set-window-parameter w 'foo nil)
                 (window-parameter w 'foo)
                 (window-parameters w)
                 (set-window-parameter w 1 2)
                 (window-parameter w 1)
                 (window-parameters w)))
         (list (condition-case err (window-parameter 999999 'foo) (error err))
               (condition-case err (set-window-parameter 999999 'foo 'bar) (error err))
               (condition-case err (window-parameters 999999) (error err))
               (condition-case err (window-parameter nil) (error err))
               (condition-case err (window-parameter nil nil nil) (error err))
               (condition-case err (set-window-parameter nil nil) (error err))
               (condition-case err (set-window-parameter nil nil nil nil) (error err))
               (condition-case err (window-parameters nil nil) (error err))
               (condition-case err (window-parameter 'foo 'bar) (error err))
               (condition-case err (set-window-parameter 'foo 'bar 'baz) (error err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        out[0],
        "OK (nil nil nil nil bar bar ((foo . bar)) 42 42 ((foo . 42)) nil nil ((foo)) 2 2 ((1 . 2) (foo)))"
    );
    assert_eq!(
        out[1],
        "OK ((wrong-type-argument windowp 999999) (wrong-type-argument windowp 999999) (wrong-type-argument window-valid-p 999999) (wrong-number-of-arguments window-parameter 1) (wrong-number-of-arguments window-parameter 3) (wrong-number-of-arguments set-window-parameter 2) (wrong-number-of-arguments set-window-parameter 4) (wrong-number-of-arguments window-parameters 2) (wrong-type-argument windowp foo) (wrong-type-argument windowp foo))"
    );
}

// Cursor audit Finding 2: window-cursor-info returns nil in
// batch mode because GNU `phys_cursor_on_p` is false until a
// real redisplay has drawn the cursor.
//
// Mirrors GNU src/window.c:8671-8672:
//   if (!w->phys_cursor_on_p)
//     return Qnil;
//
// Verified against GNU Emacs 31.0.50:
//   $ emacs -Q --batch --eval '(princ (window-cursor-info))'
//   nil
//   $ emacs -Q --batch --eval '(progn
//       (set-window-cursor-type (selected-window) (quote bar))
//       (princ (window-cursor-info)))'
//   nil
//
// GNU returns nil in batch when no live redisplay cursor geometry exists.
// neomacs now mirrors that through the frame snapshot path: without a
// `WindowCursorSnapshot`, `window-cursor-info` still returns nil.
#[test]
fn window_cursor_info_returns_nil_in_batch_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(list (window-cursor-info (selected-window))
                   (window-cursor-info nil)
                   (progn
                     (set-window-cursor-type (selected-window) 'bar)
                     (window-cursor-info (selected-window)))
                   (progn
                     (set-window-cursor-type (selected-window) nil)
                     (window-cursor-info (selected-window)))
                   (progn
                     (set-window-cursor-type (selected-window) t)
                     (window-cursor-info (selected-window))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (nil nil nil nil nil)");
}

#[test]
fn window_cursor_info_returns_last_redisplay_cursor_geometry() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;

    ev.frame_manager_mut()
        .set_window_cursor_type(wid, Value::symbol("bar"));
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            point_rows: None,
            window_id: wid,
            phys_cursor: Some(crate::window::WindowCursorSnapshot {
                kind: crate::window::WindowCursorKind::Bar,
                x: 11,
                y: 29,
                width: 3,
                height: 16,
                ascent: 12,
                row: 1,
                col: 4,
            }),
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let out = super::builtin_window_cursor_info(&mut ev, vec![]).expect("window-cursor-info");
    let items = out.as_vector_data().expect("cursor-info vector");
    assert_eq!(items.len(), 6);
    assert_eq!(items[0], Value::symbol("bar"));
    assert_eq!(items[1], Value::fixnum(11));
    assert_eq!(items[2], Value::fixnum(29));
    assert_eq!(items[3], Value::fixnum(3));
    assert_eq!(items[4], Value::fixnum(16));
    assert_eq!(items[5], Value::fixnum(12));
}

#[test]
fn window_cursor_info_hides_and_restores_live_cursor_geometry() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;

    ev.frame_manager_mut()
        .set_window_cursor_type(wid, Value::symbol("bar"));
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            point_rows: None,
            window_id: wid,
            phys_cursor: Some(crate::window::WindowCursorSnapshot {
                kind: crate::window::WindowCursorKind::Bar,
                x: 11,
                y: 29,
                width: 3,
                height: 16,
                ascent: 12,
                row: 1,
                col: 4,
            }),
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    crate::emacs_core::dispnew::pure::builtin_internal_show_cursor(
        &mut ev,
        vec![Value::NIL, Value::NIL],
    )
    .expect("hide cursor");
    assert_eq!(
        super::builtin_window_cursor_info(&mut ev, vec![]).expect("window-cursor-info"),
        Value::NIL
    );

    crate::emacs_core::dispnew::pure::builtin_internal_show_cursor(
        &mut ev,
        vec![Value::NIL, Value::T],
    )
    .expect("show cursor");
    let out = super::builtin_window_cursor_info(&mut ev, vec![]).expect("window-cursor-info");
    let items = out.as_vector_data().expect("cursor-info vector");
    assert_eq!(items[0], Value::symbol("bar"));
    assert_eq!(items[1], Value::fixnum(11));
    assert_eq!(items[2], Value::fixnum(29));
    assert_eq!(items[3], Value::fixnum(3));
    assert_eq!(items[4], Value::fixnum(16));
    assert_eq!(items[5], Value::fixnum(12));
}

#[test]
fn window_cursor_info_validates_window_designator_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(list (condition-case err (window-cursor-info 'foo) (error (car err)))
                   (condition-case err (window-cursor-info 999999) (error (car err)))
                   (condition-case err (window-cursor-info nil nil) (error (car err))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        out[0],
        "OK (wrong-type-argument wrong-type-argument wrong-number-of-arguments)"
    );
}

// Cursor audit Finding 3: set-window-cursor-type validates TYPE.
//
// Mirrors GNU src/window.c:8616-8627: TYPE must be one of
//   nil | t | box | hollow | bar | hbar
//   (box . INTEGER) | (bar . INTEGER) | (hbar . INTEGER)
// otherwise GNU signals (error "Invalid cursor type"). Before this
// fix neomacs accepted any value silently.
#[test]
fn set_window_cursor_type_signals_error_on_invalid_type_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(list
               ;; Symbol that isn't a recognized shape.
               (condition-case err
                   (set-window-cursor-type (selected-window) 'tunafish)
                 (error err))
               ;; A bare integer.
               (condition-case err
                   (set-window-cursor-type (selected-window) 42)
                 (error err))
               ;; A string.
               (condition-case err
                   (set-window-cursor-type (selected-window) \"box\")
                 (error err))
               ;; (box . NON-INTEGER) is rejected.
               (condition-case err
                   (set-window-cursor-type (selected-window) '(box . foo))
                 (error err))
               ;; (foo . 3) head must be box/bar/hbar.
               (condition-case err
                   (set-window-cursor-type (selected-window) '(foo . 3))
                 (error err))
               ;; (box . 5) is the canonical valid cons form.
               (set-window-cursor-type (selected-window) '(box . 5))
               (window-cursor-type (selected-window))
               ;; Reset.
               (set-window-cursor-type (selected-window) t))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        out[0],
        "OK ((error \"Invalid cursor type\") \
            (error \"Invalid cursor type\") \
            (error \"Invalid cursor type\") \
            (error \"Invalid cursor type\") \
            (error \"Invalid cursor type\") \
            (box . 5) \
            (box . 5) \
            t)"
    );
}

#[test]
fn window_metadata_shared_state_smoke() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(let* ((w (selected-window))
                (m (minibuffer-window))
                (dt '(1 2 3)))
           (list (window-dedicated-p w)
                 (set-window-dedicated-p w t)
                 (window-dedicated-p w)
                 (set-window-dedicated-p w nil)
                 (window-dedicated-p w)
                 (null (window-parameters w))
                 (set-window-parameter w 'foo 'bar)
                 (window-parameter w 'foo)
                 (equal (window-parameters w) '((foo . bar)))
                 (set-window-parameter w 'foo nil)
                 (equal (window-parameters w) '((foo)))
                 (null (window-display-table w))
                 (let ((rv (set-window-display-table w dt))) (equal rv dt))
                 (equal (window-display-table w) dt)
                 (null (set-window-display-table w nil))
                 (null (window-display-table w))
                 (window-cursor-type w)
                 (set-window-cursor-type w 'bar)
                 (window-cursor-type w)
                 (set-window-cursor-type w t)
                 (window-cursor-type w)
                 (set-window-cursor-type m nil)
                 (window-cursor-type m)))
         (list (condition-case err (window-parameter 999999 'foo) (error err))
               (condition-case err (set-window-parameter 999999 'foo 'bar) (error err))
               (condition-case err (window-display-table 999999) (error err))
               (condition-case err (set-window-display-table 999999 nil) (error err))
               (condition-case err (window-cursor-type 999999) (error err))
               (condition-case err (set-window-cursor-type 999999 nil) (error err))
               (condition-case err (set-window-dedicated-p 999999 t) (error err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        out[0],
        "OK (nil t t nil nil t bar bar t nil t t t t t t t bar bar t t nil nil)"
    );
    assert_eq!(
        out[1],
        "OK ((wrong-type-argument windowp 999999) (wrong-type-argument windowp 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999))"
    );
}

#[test]
fn raw_context_does_not_prebind_window_inside_aliases() {
    crate::test_utils::init_test_tracing();
    let eval = neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    for name in ["window-inside-pixel-edges", "window-inside-edges"] {
        assert!(
            eval.obarray_view().symbol_function(name).is_none(),
            "{name} should come from GNU window.el, not Context::new"
        );
    }
}

#[test]
fn gnu_window_el_defines_window_inside_aliases() {
    crate::test_utils::init_test_tracing();
    let source = fs::read_to_string(crate::test_utils::workspace_root().join("lisp/window.el"))
        .expect("read window.el");
    assert!(
        source.contains("(defun window-body-edges (&optional window)"),
        "GNU window.el should define window-body-edges",
    );
    assert!(
        source.contains("(defalias 'window-inside-edges 'window-body-edges)"),
        "GNU window.el should own the window-inside-edges alias",
    );
    assert!(
        source.contains("(defun window-body-pixel-edges (&optional window)"),
        "GNU window.el should define window-body-pixel-edges",
    );
    assert!(
        source.contains("(defalias 'window-inside-pixel-edges 'window-body-pixel-edges)"),
        "GNU window.el should own the window-inside-pixel-edges alias",
    );
}

// -- Frame operations --

#[test]
fn frame_visible_p_enforces_arity_and_designators() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(condition-case err (frame-visible-p) (error (car err)))
         (condition-case err (frame-visible-p nil) (error err))
         (condition-case err (frame-visible-p 999999) (error err))
         (frame-visible-p (selected-frame))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK (wrong-type-argument frame-live-p nil)");
    assert_eq!(out[2], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[3], "OK t");
}

#[test]
fn frame_designator_errors_use_emacs_predicates() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            "(condition-case err (frame-parameter \"x\" 'name) (error err))
         (condition-case err (frame-parameter 999999 'name) (error err))
         (condition-case err (frame-parameters \"x\") (error err))
         (condition-case err (frame-parameters 999999) (error err))
         (condition-case err (modify-frame-parameters \"x\" nil) (error err))
         (condition-case err (modify-frame-parameters 999999 nil) (error err))
         (condition-case err (delete-frame \"x\") (error err))
         (condition-case err (delete-frame 999999) (error err))
         (frame-parameter nil 'name)
         (condition-case err (modify-frame-parameters nil nil) (error err))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (wrong-type-argument framep \"x\")");
    assert_eq!(out[1], "OK (wrong-type-argument framep 999999)");
    assert_eq!(out[2], "OK (wrong-type-argument framep \"x\")");
    assert_eq!(out[3], "OK (wrong-type-argument framep 999999)");
    assert_eq!(out[4], "OK (wrong-type-argument frame-live-p \"x\")");
    assert_eq!(out[5], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[6], "OK (wrong-type-argument framep \"x\")");
    assert_eq!(out[7], "OK (wrong-type-argument framep 999999)");
    assert_eq!(out[8], "OK \"F1\"");
    assert_eq!(out[9], "OK nil");
}

#[test]
fn frame_query_builtins_match_gnu_batch_startup_geometry() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let out = ev
        .eval_str_each(
            r#"(list (frame-char-height)
                 (frame-char-width)
                 (frame-native-height)
                 (frame-native-width)
                 (frame-text-cols)
                 (frame-text-lines)
                 (frame-text-width)
                 (frame-text-height)
                 (frame-total-cols)
                 (frame-total-lines)
                 (frame-position))"#,
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (1 1 25 80 80 25 80 25 80 25 (0 . 0))");
}

#[test]
fn frame_query_builtins_report_pixel_sizes_for_gui_frames() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev.frame_manager_mut().create_frame("gui", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("gui frame");
        frame.set_window_system(Some(Value::symbol("x")));
    }

    assert_eq!(
        crate::emacs_core::frame::builtin_frame_native_width(
            &mut ev,
            vec![Value::make_frame(fid.0)]
        )
        .unwrap(),
        Value::fixnum(800)
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_native_height(
            &mut ev,
            vec![Value::make_frame(fid.0)]
        )
        .unwrap(),
        Value::fixnum(600)
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_text_width(&mut ev, vec![Value::make_frame(fid.0)])
            .unwrap(),
        // FRAME_PIXEL_TO_TEXT_WIDTH (src/frame.h:1804-1808): 800 - 16 px scroll-bar area
        // (the toolkit default, src/xfns.c:2393-2407) - 2x8 px fringes.
        Value::fixnum(768)
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_text_height(
            &mut ev,
            vec![Value::make_frame(fid.0)]
        )
        .unwrap(),
        Value::fixnum(600)
    );
}

#[test]
fn frame_text_width_ignores_window_local_fringe_overrides_for_gui_frames() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev.frame_manager_mut().create_frame("gui", 800, 600, buf);
    let selected_window = {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("gui frame");
        frame.set_window_system(Some(Value::symbol("x")));
        frame.selected_window
    };

    assert!(
        ev.frame_manager_mut()
            .set_window_fringes(selected_window, Some(5), Some(5), false, false),
        "window-local fringes should change"
    );

    assert_eq!(
        crate::emacs_core::frame::builtin_frame_text_width(&mut ev, vec![Value::make_frame(fid.0)])
            .unwrap(),
        // FRAME_PIXEL_TO_TEXT_WIDTH (src/frame.h:1804-1808): 800 - 16 px scroll-bar area
        // (the toolkit default, src/xfns.c:2393-2407) - 2x8 px fringes.
        Value::fixnum(768)
    );
}

#[test]
fn frame_query_builtins_use_internal_window_system_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev.frame_manager_mut().create_frame("gui", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("gui frame");
        frame.set_window_system(Some(Value::symbol("x")));
        frame.remove_parameter(Value::symbol("window-system"));
    }

    assert_eq!(
        crate::emacs_core::frame::builtin_frame_native_width(
            &mut ev,
            vec![Value::make_frame(fid.0)]
        )
        .unwrap(),
        Value::fixnum(800)
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_native_height(
            &mut ev,
            vec![Value::make_frame(fid.0)]
        )
        .unwrap(),
        Value::fixnum(600)
    );
}

#[test]
fn frame_old_selected_window_direct_wrapper_matches_batch_nil_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let fid = super::ensure_selected_frame_id(&mut ev);

    assert_eq!(
        super::builtin_frame_old_selected_window(&mut ev, vec![]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        super::builtin_frame_old_selected_window(&mut ev, vec![Value::NIL]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        super::builtin_frame_old_selected_window(&mut ev, vec![Value::make_frame(fid.0)]).unwrap(),
        Value::NIL
    );

    let err = super::builtin_frame_old_selected_window(&mut ev, vec![Value::fixnum(999999)])
        .expect_err("invalid frame should signal");
    match err.into_kind() {
        neovm_value::error::FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("frame-live-p"), Value::fixnum(999999)]
            );
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }
}

#[test]
fn selected_frame_returns_frame_handle() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame(
        "(let ((f (selected-frame)))
           (list (framep f)
                 (frame-live-p f)
                 (integerp f)
                 (eq f (window-frame))))",
    );
    assert_eq!(r, "OK (t t nil t)");
}

#[test]
fn frame_list_has_one() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(length (frame-list))");
    assert_eq!(r, "OK 1");
}

#[test]
fn make_terminal_frame_creates_tty_child_frame_with_gnu_geometry_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(scratch);
    let root_id = ev.frame_manager_mut().create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frame_manager_mut().get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let params = Value::list(vec![
        Value::cons(Value::symbol("parent-frame"), root),
        Value::cons(Value::symbol("left"), Value::fixnum(4)),
        Value::cons(Value::symbol("top"), Value::fixnum(2)),
        Value::cons(Value::symbol("width"), Value::fixnum(6)),
        Value::cons(Value::symbol("height"), Value::fixnum(3)),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);
    let child = crate::emacs_core::frame::builtin_make_terminal_frame(&mut ev, vec![params])
        .expect("make-terminal-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));

    assert_eq!(
        crate::emacs_core::frame::builtin_frame_parent(&mut ev, vec![child]).expect("frame-parent"),
        root
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_ancestor_p(&mut ev, vec![root, child])
            .expect("frame-ancestor-p"),
        Value::T
    );

    let position = crate::emacs_core::frame::builtin_frame_position(&mut ev, vec![child])
        .expect("frame-position");
    assert_eq!(position.cons_car(), Value::fixnum(4));
    assert_eq!(position.cons_cdr(), Value::fixnum(2));

    let edges = super::builtin_tty_frame_edges(&mut ev, vec![child]).expect("tty-frame-edges");
    assert_eq!(
        neovm_value::value::list_to_vec(&edges).expect("edges list"),
        vec![
            Value::fixnum(4),
            Value::fixnum(2),
            Value::fixnum(10),
            Value::fixnum(5),
        ]
    );

    let root_minibuffer = ev
        .frame_manager()
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(child_frame.minibuffer_window, Some(root_minibuffer));
    assert!(child_frame.minibuffer_leaf.is_none());
    assert_eq!(
        child_frame.parameter("minibuffer"),
        Some(Value::make_window(root_minibuffer.0))
    );

    let hidden_z_order =
        super::builtin_tty_frame_list_z_order(&mut ev, vec![root]).expect("hidden z-order");
    assert_eq!(
        neovm_value::value::list_to_vec(&hidden_z_order).expect("hidden z-order list"),
        vec![root]
    );

    assert_eq!(
        crate::emacs_core::frame::builtin_make_frame_visible(&mut ev, vec![child])
            .expect("make-frame-visible"),
        child
    );

    let visible_z_order =
        super::builtin_tty_frame_list_z_order(&mut ev, vec![root]).expect("visible z-order");
    assert_eq!(
        neovm_value::value::list_to_vec(&visible_z_order).expect("visible z-order list"),
        vec![child, root]
    );

    let hit = super::builtin_tty_frame_at(&mut ev, vec![Value::fixnum(5), Value::fixnum(3)])
        .expect("tty-frame-at");
    assert_eq!(
        neovm_value::value::list_to_vec(&hit).expect("hit list"),
        vec![child, Value::fixnum(1), Value::fixnum(1)]
    );

    let border_hit = super::builtin_tty_frame_at(&mut ev, vec![Value::fixnum(4), Value::fixnum(1)])
        .expect("tty-frame-at border");
    assert_eq!(
        neovm_value::value::list_to_vec(&border_hit).expect("border hit list"),
        vec![child, Value::fixnum(0), Value::fixnum(-1)]
    );
}

#[test]
fn make_terminal_frame_accepts_tty_minibuffer_window_parameter() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(scratch);
    let root_id = ev.frame_manager_mut().create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frame_manager_mut().get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frame_manager()
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let root_minibuffer_value = Value::make_window(root_minibuffer.0);
    let params = Value::list(vec![
        Value::cons(Value::symbol("parent-frame"), root),
        Value::cons(Value::symbol("width"), Value::fixnum(6)),
        Value::cons(Value::symbol("height"), Value::fixnum(3)),
        Value::cons(Value::symbol("minibuffer"), root_minibuffer_value),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);

    let child = crate::emacs_core::frame::builtin_make_terminal_frame(&mut ev, vec![params])
        .expect("make-terminal-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));
    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(child_frame.minibuffer_window, Some(root_minibuffer));
    assert!(child_frame.minibuffer_leaf.is_none());
    assert_eq!(
        child_frame.parameter("minibuffer"),
        Some(root_minibuffer_value)
    );
}

#[test]
fn tty_child_frame_accepts_text_pixel_size_parameters() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(scratch);
    let root_id = ev.frame_manager_mut().create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frame_manager_mut().get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frame_manager()
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let root_minibuffer_value = Value::make_window(root_minibuffer.0);
    let text_pixels = |n| Value::cons(Value::symbol("text-pixels"), Value::fixnum(n));
    let params = Value::list(vec![
        Value::cons(Value::symbol("parent-frame"), root),
        Value::cons(Value::symbol("width"), text_pixels(17)),
        Value::cons(Value::symbol("height"), text_pixels(4)),
        Value::cons(Value::symbol("minibuffer"), root_minibuffer_value),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);

    let child = crate::emacs_core::frame::builtin_make_terminal_frame(&mut ev, vec![params])
        .expect("make-terminal-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));
    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(child_frame.width, 17);
    assert_eq!(child_frame.height, 4);
    assert_eq!(child_frame.parameter("width"), Some(Value::fixnum(17)));
    assert_eq!(child_frame.parameter("height"), Some(Value::fixnum(4)));
    assert_eq!(
        *child_frame.root_window().bounds(),
        crate::window::Rect::new(0.0, 0.0, 17.0, 4.0)
    );
}

#[test]
fn modify_frame_parameters_accepts_text_pixel_size_on_tty_child_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(scratch);
    let root_id = ev.frame_manager_mut().create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frame_manager_mut().get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frame_manager()
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let root_minibuffer_value = Value::make_window(root_minibuffer.0);
    let params = Value::list(vec![
        Value::cons(Value::symbol("parent-frame"), root),
        Value::cons(Value::symbol("width"), Value::fixnum(1)),
        Value::cons(Value::symbol("height"), Value::fixnum(1)),
        Value::cons(Value::symbol("minibuffer"), root_minibuffer_value),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);
    let child = crate::emacs_core::frame::builtin_make_terminal_frame(&mut ev, vec![params])
        .expect("make-terminal-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));
    let text_pixels = |n| Value::cons(Value::symbol("text-pixels"), Value::fixnum(n));
    let alist = Value::list(vec![
        Value::cons(Value::symbol("width"), text_pixels(91)),
        Value::cons(Value::symbol("height"), text_pixels(18)),
    ]);

    crate::emacs_core::frame::builtin_modify_frame_parameters(&mut ev, vec![child, alist])
        .expect("modify-frame-parameters");
    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(child_frame.width, 91);
    assert_eq!(child_frame.height, 18);
    assert_eq!(child_frame.parameter("width"), Some(Value::fixnum(91)));
    assert_eq!(child_frame.parameter("height"), Some(Value::fixnum(18)));
    assert_eq!(
        *child_frame.root_window().bounds(),
        crate::window::Rect::new(0.0, 0.0, 91.0, 18.0)
    );
}

#[test]
fn window_text_pixel_size_uses_supplied_child_window_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(scratch);
    let root_id = ev.frame_manager_mut().create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frame_manager_mut().get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frame_manager()
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let child_buffer = ev.edit.buffers.create_buffer("*child-measure*");
    ev.edit
        .buffers
        .insert_into_buffer(child_buffer, "abcd\nxy")
        .expect("insert child buffer");
    let params = Value::list(vec![
        Value::cons(Value::symbol("parent-frame"), root),
        Value::cons(Value::symbol("width"), Value::fixnum(20)),
        Value::cons(Value::symbol("height"), Value::fixnum(5)),
        Value::cons(
            Value::symbol("minibuffer"),
            Value::make_window(root_minibuffer.0),
        ),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);
    let child = crate::emacs_core::frame::builtin_make_terminal_frame(&mut ev, vec![params])
        .expect("make-terminal-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));
    let child_window = {
        let frame = ev
            .frame_manager_mut()
            .get_mut(child_id)
            .expect("child frame");
        let child_window = frame.root_window().id();
        frame
            .find_window_mut(child_window)
            .expect("child root window")
            .set_buffer(child_buffer);
        child_window
    };

    let size = crate::emacs_core::xdisp::builtin_window_text_pixel_size_ctx(
        &mut ev,
        vec![Value::make_window(child_window.0)],
    )
    .expect("window-text-pixel-size");
    assert_eq!(size.cons_car(), Value::fixnum(4));
    assert_eq!(size.cons_cdr(), Value::fixnum(2));
}

#[test]
fn shared_tty_child_minibuffer_window_apis_use_owner_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(scratch);
    let root_id = ev.frame_manager_mut().create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frame_manager_mut().get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frame_manager()
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let root_minibuffer_value = Value::make_window(root_minibuffer.0);
    let params = Value::list(vec![
        Value::cons(Value::symbol("parent-frame"), root),
        Value::cons(Value::symbol("width"), Value::fixnum(10)),
        Value::cons(Value::symbol("height"), Value::fixnum(4)),
        Value::cons(Value::symbol("minibuffer"), root_minibuffer_value),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);

    let child = crate::emacs_core::frame::builtin_make_terminal_frame(&mut ev, vec![params])
        .expect("make-terminal-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));
    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(child_frame.minibuffer_window, Some(root_minibuffer));
    assert!(child_frame.minibuffer_leaf.is_none());

    assert_eq!(
        ev.frame_manager().find_window_frame_id(root_minibuffer),
        Some(root_id)
    );
    assert_eq!(
        ev.frame_manager()
            .find_valid_window_frame_id(root_minibuffer),
        Some(root_id)
    );
    assert!(ev.frame_manager().lookup_window(root_minibuffer).is_some());
    assert_eq!(
        super::builtin_window_frame(&mut ev, &vec![root_minibuffer_value]).expect("window-frame"),
        root
    );
    assert!(
        super::builtin_window_buffer(&mut ev, &vec![root_minibuffer_value])
            .expect("window-buffer")
            .is_buffer()
    );
    let width =
        super::builtin_window_total_width(&mut ev, vec![root_minibuffer_value]).expect("width");
    assert!(width.as_int().is_some_and(|value| value > 0));
}

#[test]
fn x_create_frame_creates_live_frame_and_preserves_char_geometry_params() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev
        .frame_manager_mut()
        .create_frame("bootstrap", 800, 600, scratch);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("bootstrap frame")
        .install_backend_less_gui_kind(WindowSystemKind::X);

    let params = Value::list(vec![
        Value::cons(Value::symbol("name"), Value::string("GUI")),
        Value::cons(Value::symbol("width"), Value::fixnum(80)),
        Value::cons(Value::symbol("height"), Value::fixnum(25)),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");

    let created_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    assert_ne!(created_id, fid);
    let frame = ev.frame_manager().get(created_id).expect("created frame");
    assert_eq!(ev.frame_manager().frame_list().len(), 2);
    assert_eq!(frame.name_runtime_string_owned(), "GUI");
    assert_eq!(frame.parameter("width"), Some(Value::fixnum(80)));
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(25)));
    assert_eq!(frame.visibility, crate::window::FrameVisibility::Invisible);
    assert_eq!(frame.char_width, 8.0);
    assert_eq!(frame.char_height, 16.0);
}

#[test]
fn x_create_frame_creates_opening_frame_and_notifies_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev
        .frame_manager_mut()
        .create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev
            .frame_manager_mut()
            .get_mut(fid)
            .expect("bootstrap frame");
        frame.install_backend_less_gui_kind(WindowSystemKind::X);
        if let Some(mini_leaf) = frame.minibuffer_leaf.as_mut() {
            mini_leaf.set_bounds(crate::window::Rect::new(0.0, 608.0, 960.0, 32.0));
        }
    }
    ev.set_variable("terminal-frame", Value::make_frame(fid.0));
    let host = RecordingDisplayHost::new();
    let requests = host.realized.clone();
    ev.set_display_host(Box::new(host));

    let title = Value::heap_string(LispString::from_utf8("Neomacs"));
    let params = Value::list(vec![
        Value::cons(Value::symbol("name"), title),
        Value::cons(Value::symbol("title"), title),
        Value::cons(Value::symbol("width"), Value::fixnum(80)),
        Value::cons(Value::symbol("height"), Value::fixnum(25)),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");

    let created_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    assert_ne!(created_id, fid);
    assert_eq!(ev.frame_manager().frame_list().len(), 2);
    let frame = ev
        .frame_manager()
        .get(created_id)
        .expect("created opening frame");
    assert_eq!(frame.parameter("width"), Some(Value::fixnum(80)));
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(25)));
    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, created_id);
    assert_eq!(requests[0].title, LispString::from_utf8("Neomacs"));
    assert_eq!(requests[0].width, frame.width);
    assert_eq!(requests[0].height, frame.height);
    assert_eq!(
        ev.frame_manager()
            .selected_frame()
            .expect("selected frame")
            .id,
        created_id
    );
}

#[test]
fn x_create_frame_fullscreen_maximized_param_reaches_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev
        .frame_manager_mut()
        .create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev
            .frame_manager_mut()
            .get_mut(fid)
            .expect("bootstrap frame");
        frame.install_backend_less_gui_kind(WindowSystemKind::X);
    }
    ev.set_variable("terminal-frame", Value::make_frame(fid.0));
    let host = RecordingDisplayHost::new();
    let requests = host.realized.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(Value::symbol("name"), Value::string("Neomacs")),
        Value::cons(Value::symbol("fullscreen"), Value::symbol("maximized")),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");

    let created_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let frame = ev
        .frame_manager()
        .get(created_id)
        .expect("created opening frame");
    assert_eq!(
        frame.known_parameter(FrameParam::Fullscreen),
        Some(Value::symbol("maximized"))
    );

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, created_id);
    assert_eq!(requests[0].fullscreen, Some(FrameFullscreen::Maximized));
}

#[test]
fn x_create_frame_with_parent_frame_creates_gui_child_overlay_without_host_window() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.set_display_identity(crate::window::FrameDisplayIdentity::wayland("wayland-7"));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
        if let Some(mini_leaf) = parent.minibuffer_leaf.as_mut() {
            mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
        }
    }
    ev.frame_manager_mut().select_frame(parent_id);
    let host = RecordingDisplayHost::new();
    let requests = host.realized.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(Value::symbol("name"), Value::string("child")),
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("left"), Value::fixnum(30)),
        Value::cons(Value::symbol("top"), Value::fixnum(40)),
        Value::cons(Value::symbol("width"), Value::fixnum(20)),
        Value::cons(Value::symbol("height"), Value::fixnum(5)),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
        Value::cons(Value::symbol("undecorated"), Value::T),
        Value::cons(Value::symbol("no-accept-focus"), Value::T),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");

    let child_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let parent_minibuffer = ev
        .frame_manager()
        .get(parent_id)
        .and_then(|frame| frame.minibuffer_window)
        .expect("parent minibuffer");
    let child = ev.frame_manager().get(child_id).expect("child frame");

    assert_eq!(requests.borrow().len(), 0);
    assert_eq!(child.parent_frame, Value::make_frame(parent_id.0));
    assert_eq!(
        child.parameter("parent-frame"),
        Some(Value::make_frame(parent_id.0))
    );
    assert_eq!(child.left_pos, 30);
    assert_eq!(child.top_pos, 40);
    // 20 text columns x 10 px plus default fringes (8+8) and the 16 px toolkit scroll-bar
    // area (src/xfns.c:2393-2407; FRAME_TEXT_TO_PIXEL_WIDTH, src/frame.h:1790-1794).
    assert_eq!(child.width, 232);
    assert_eq!(child.height, 100);
    assert_eq!(child.char_width, 10.0);
    assert_eq!(child.char_height, 20.0);
    assert_eq!(child.parameter("display"), Some(Value::string("wayland-7")));
    assert_eq!(child.display_identity().x_display(), None);
    assert!(child.undecorated);
    assert!(child.no_accept_focus);
    assert_eq!(child.minibuffer_window, Some(parent_minibuffer));
    assert!(child.minibuffer_leaf.is_none());
}

#[test]
fn x_create_frame_root_window_positions_follow_buffer_edits() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let input = "M-x ifconfig";
    let scratch = ev.edit.buffers.create_buffer("*x-child-marker*");
    ev.edit.buffers.set_current(scratch);
    ev.edit.buffers.insert_into_buffer(scratch, input);
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
    }
    ev.frame_manager_mut().select_frame(parent_id);

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let old_end = LispCharPos1::from_one_based_usize(input.chars().count() + 1);
    {
        let (cf_frames, cf_buffers) = ev.frame_manager_and_buffers_mut();
        let child = cf_frames.get_mut(child_id).expect("child frame");
        crate::window::window_markers::set_window_point_with_marker(
            cf_buffers,
            child.root_window_mut(),
            old_end,
        );
    }

    // Backspace at EOB moves GNU's w->pointm marker with the deletion.  A
    // child frame must therefore observe the new EOB before its next
    // redisplay, without waiting for a later `set-window-point` call.
    ev.edit
        .buffers
        .get_mut(scratch)
        .expect("child buffer")
        .delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            input.len() - 1,
            input.len(),
        ));
    ev.sync_window_positions(scratch);

    let child = ev.frame_manager().get(child_id).expect("child frame");
    let crate::window::Window::Leaf {
        point,
        position_markers,
        ..
    } = &child.root_window()
    else {
        panic!("fresh frame root must be a live leaf window");
    };
    assert!(
        position_markers.is_attached(),
        "x-create-frame must atomically attach all GNU window-position markers"
    );
    assert_eq!(
        *point,
        LispCharPos1::from_one_based_usize(input.chars().count()),
        "the child window point must follow deletion of the character before EOB"
    );
}

#[test]
fn x_create_frame_root_window_position_markers_survive_garbage_collection() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let input = "M-x ifconfig";
    let minibuffer = ev.edit.buffers.create_buffer(" *Minibuf-vertico-gc*");
    ev.edit.buffers.set_current(minibuffer);
    ev.edit.buffers.insert_into_buffer(minibuffer, input);

    let parent_id = ev
        .frame_manager_mut()
        .create_frame("vertico-parent", 960, 640, minibuffer);
    let parent_minibuffer = ev
        .frame_manager()
        .get(parent_id)
        .expect("parent frame")
        .selected_window;
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.minibuffer_window = Some(parent_minibuffer);
    }
    ev.frame_manager_mut().select_frame(parent_id);

    // This is the shape used by posframe: a child frame with no minibuffer of
    // its own, sharing the parent's minibuffer window, whose ordinary root
    // window displays that same minibuffer buffer.
    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(
            Value::symbol("minibuffer"),
            Value::make_window(parent_minibuffer.0),
        ),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {created:?}")),
    );
    let child_window = ev
        .frame_manager()
        .get(child_id)
        .expect("child frame")
        .selected_window;
    super::builtin_set_window_buffer(
        &mut ev,
        vec![
            Value::make_window(child_window.0),
            Value::make_buffer(minibuffer),
        ],
    )
    .expect("set child root buffer");
    super::builtin_set_window_point(
        &mut ev,
        vec![
            Value::make_window(child_window.0),
            Value::fixnum(input.chars().count() as i64 + 1),
        ],
    )
    .expect("set child point to minibuffer EOB");

    // Completion allocates heavily between posframe creation and the next
    // edit.  GNU's live Window roots w->start/pointm/old_pointm, so a
    // collection cannot unlink them from the buffer marker chain.
    ev.eval_str("(garbage-collect)")
        .expect("collect while the child frame remains live");

    ev.edit
        .buffers
        .get_mut(minibuffer)
        .expect("minibuffer")
        .delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
            input.len() - 1,
            input.len(),
        ));
    ev.sync_window_positions(minibuffer);

    let child = ev.frame_manager().get(child_id).expect("child frame");
    let crate::window::Window::Leaf { point, .. } = &child.root_window() else {
        panic!("child root must remain a live leaf window");
    };
    assert_eq!(
        *point,
        LispCharPos1::from_one_based_usize(input.chars().count()),
        "a live child window must keep its point marker rooted across GC so a Backspace edit moves it to the new EOB"
    );
}

#[test]
fn x_create_frame_with_parent_frame_inherits_parent_font_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    let parent_font_name = Value::string("-*-Hack-regular-normal-*-*-102-*-*-*-m-0-iso10646-1");
    let mut parent_font_face = crate::face::Face::new("default");
    parent_font_face.family = Some(Value::string("Hack"));
    parent_font_face.weight = Some(FontWeight::NORMAL);
    parent_font_face.slant = Some(FontSlant::Normal);
    parent_font_face.width = Some(FontWidth::Normal);
    parent_font_face.height = Some(crate::face::FaceHeight::Absolute(102));
    let parent_font_object = crate::emacs_core::font::build_font_object(&parent_font_face);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.set_known_parameter(FrameParam::Font, parent_font_name);
        parent.set_parameter(Value::symbol("font-parameter"), parent_font_object);
        parent.char_width = 7.2;
        parent.char_height = 17.0;
        parent.font_pixel_size = 13.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    ev.set_display_host(Box::new(RecordingDisplayHost::new()));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let child = ev.frame_manager().get(child_id).expect("child frame");

    assert_eq!(
        child.known_parameter(FrameParam::Font),
        Some(parent_font_name)
    );
    assert_eq!(child.parameter("font-parameter"), Some(parent_font_object));
}

#[test]
fn x_create_frame_with_explicit_font_resolves_internal_font_parameter() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.set_known_parameter(FrameParam::Font, Value::string("JetBrains Mono-9"));
        parent.char_width = 7.2;
        parent.char_height = 17.0;
        parent.font_pixel_size = 13.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    ev.set_display_host(Box::new(RecordingDisplayHost::with_resolved_frame_font(
        resolved_frame_font(
            "JetBrains Mono",
            "JetBrainsMono-Regular",
            90,
            FontPxProbeResult {
                pixel_size: 13,
                height: 17,
                ascent: 13,
                descent: 4,
                max_width: 7,
                space_width: 7,
                average_width: 7,
            },
        ),
    )));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("font"), Value::string("JetBrains Mono-9")),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let child = ev.frame_manager().get(child_id).expect("child frame");

    assert!(
        child
            .parameter("font-parameter")
            .is_some_and(|value| value.is_font_object()),
        "explicit x-create-frame font must be resolved into internal font-parameter"
    );
    assert_eq!(child.char_width, 7.0);
    assert_eq!(child.char_height, 17.0);
    assert_eq!(child.font_pixel_size, 13.0);
}

fn opened_noto_host() -> RecordingDisplayHost {
    RecordingDisplayHost::with_resolved_frame_font(resolved_frame_font(
        "Noto Sans Mono",
        "NotoSansMono-Regular",
        160,
        FontPxProbeResult {
            pixel_size: 22,
            height: 31,
            ascent: 23,
            descent: 8,
            max_width: 13,
            space_width: 13,
            average_width: 13,
        },
    ))
}

fn gui_frame_for_font_change(ev: &mut Context) -> crate::window::FrameId {
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
        frame.font_pixel_size = 11.0;
    }
    fid
}

#[test]
fn modify_frame_parameters_font_reopens_the_initial_frame_font() {
    // GNU `frame-notice-user-settings' applies a `font' set in
    // `default-frame-alist' after the initial frame exists by calling
    // `modify-frame-parameters'. That must reopen the font like
    // `gui_set_font', not merely store the parameter (#639).
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let host = opened_noto_host();
    let resized = host.resized.clone();
    let font_requests = host.font_requests().clone();
    ev.set_display_host(Box::new(host));
    let fid = gui_frame_for_font_change(&mut ev);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .set_known_parameter(FrameParam::Font, Value::string("Initial-11"));
    ev.eval_str("(fset 'frame-windows-min-size (lambda (&rest _) 1))")
        .expect("stub the absent window-minimum subr");

    let out = ev.eval_str_each(
        r#"(modify-frame-parameters (selected-frame) '((font . "Noto Sans Mono-16")))"#,
    );
    assert!(out[0].is_ok(), "font update failed: {:?}", out[0]);

    let frame = ev.frame_manager().get(fid).expect("frame should exist");
    assert_eq!(frame.char_width, 13.0);
    assert_eq!(frame.char_height, 31.0);
    assert_eq!(frame.font_pixel_size, 22.0);
    let opened = frame
        .parameter("font-parameter")
        .expect("the opened font object is stored on the frame");
    assert!(
        crate::emacs_core::font::is_font_object(&opened),
        "font-parameter must hold the opened font object, got {opened:?}"
    );
    assert_eq!(
        frame.known_parameter(FrameParam::Font),
        Some(crate::emacs_core::font::public_frame_font_parameter_value(
            opened
        )),
        "the frame parameter must be the opened font's public name"
    );
    assert_eq!(
        resized.borrow().len(),
        1,
        "a live frame font change must request the implied resize"
    );

    let requests = font_requests.borrow();
    assert_eq!(requests.len(), 1, "one frame font resolution expected");
    assert_eq!(requests[0].0, fid);
    assert_eq!(
        requests[0].1.face().family,
        Some(Value::string("Noto Sans Mono")),
        "the requested family must be parsed from the parameter"
    );
    let size = requests[0].1.size();
    assert_eq!(
        size,
        crate::emacs_core::display_host::FrameFontSize::points(16.0).expect("16pt is a valid size")
    );

    let table =
        crate::emacs_core::xfaces::runtime_face_table_from_frame_lisp_faces(&ev, fid, false);
    let default_face = table.merge_faces(&["default"]);
    assert_eq!(
        default_face.family,
        Some(Value::string("Noto Sans Mono")),
        "redisplay's default face must follow the opened font"
    );
    assert_eq!(
        default_face.height,
        Some(crate::face::FaceHeight::Absolute(160)),
        "the default face must carry the opened font's height"
    );
}

#[test]
fn modify_frame_parameters_font_keeps_explicit_default_face_attributes() {
    // GNU `set_lface_from_font` only fills unspecified slots. A family set
    // explicitly on the frame-local default face must survive a frame font
    // change, while the Font slot still follows the opened font.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let host = opened_noto_host();
    ev.set_display_host(Box::new(host));
    let fid = gui_frame_for_font_change(&mut ev);
    ev.eval_str("(fset 'frame-windows-min-size (lambda (&rest _) 1))")
        .expect("stub the absent window-minimum subr");
    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("family"),
            Value::string("Explicit Family"),
            Value::make_frame(fid.0),
        ],
    )
    .expect("set an explicit default-face family");

    let out = ev.eval_str_each(
        r#"(modify-frame-parameters (selected-frame) '((font . "Noto Sans Mono-16")))"#,
    );
    assert!(out[0].is_ok(), "font update failed: {:?}", out[0]);

    let table =
        crate::emacs_core::xfaces::runtime_face_table_from_frame_lisp_faces(&ev, fid, false);
    let default_face = table.merge_faces(&["default"]);
    assert_eq!(
        default_face.family,
        Some(Value::string("Explicit Family")),
        "an explicit default-face family must survive the font change"
    );

    let opened = ev
        .frame_manager()
        .get(fid)
        .expect("frame")
        .parameter("font-parameter")
        .expect("opened font object");
    let font_attr = crate::emacs_core::xfaces::builtin_internal_get_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::make_frame(fid.0),
        ],
    )
    .expect("default face font");
    assert_eq!(
        font_attr, opened,
        "the Font slot must still follow the opened font"
    );

    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("family"),
            Value::symbol("unspecified"),
            Value::make_frame(fid.0),
        ],
    )
    .expect("clear the explicit family");
    let out = ev.eval_str_each(
        r#"(modify-frame-parameters (selected-frame) '((font . "Noto Sans Mono-17")))"#,
    );
    assert!(out[0].is_ok(), "second font update failed: {:?}", out[0]);
    let table =
        crate::emacs_core::xfaces::runtime_face_table_from_frame_lisp_faces(&ev, fid, false);
    let default_face = table.merge_faces(&["default"]);
    assert_eq!(
        default_face.family,
        Some(Value::string("Noto Sans Mono")),
        "an explicit unspecified family must not block the font-derived value"
    );
}

#[test]
fn modify_frame_parameters_font_refreshes_geometry_when_the_host_rejects_resize() {
    // A rejected implied resize leaves no acknowledgement to recompute the
    // window tree. The new character metrics must still be adopted locally
    // at the frame's current pixel allocation.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let host = opened_noto_host();
    let resized = host.resized.clone();
    host.fail_resize().set(true);
    ev.set_display_host(Box::new(host));
    let fid = gui_frame_for_font_change(&mut ev);
    ev.eval_str("(fset 'frame-windows-min-size (lambda (&rest _) 1))")
        .expect("stub the absent window-minimum subr");
    let before = *ev
        .frame_manager()
        .get(fid)
        .expect("frame")
        .root_window()
        .bounds();

    let out = ev.eval_str_each(
        r#"(modify-frame-parameters (selected-frame) '((font . "Noto Sans Mono-16")))"#,
    );
    assert!(
        out[0].is_ok(),
        "a rejected implied resize must not signal: {:?}",
        out[0]
    );

    let frame = ev.frame_manager().get(fid).expect("frame");
    assert_eq!(
        (frame.width, frame.height),
        (800, 600),
        "a rejected resize must keep the frame's pixel allocation"
    );
    assert_eq!(frame.char_width, 13.0);
    assert_eq!(frame.char_height, 31.0);
    assert_eq!(
        resized.borrow().len(),
        1,
        "the implied resize must have been attempted"
    );
    assert_ne!(
        *frame.root_window().bounds(),
        before,
        "the window tree must adopt the new metrics at the kept allocation"
    );
}

#[test]
fn modify_frame_parameters_font_signals_like_gui_set_font_on_failure() {
    // GNU `gui_set_font' signals `Font NAME is not defined' for a string
    // that cannot be opened and `Invalid font' for any other non-font value.
    // The previous parameter is restored in both cases.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_display_host(Box::new(RecordingDisplayHost::new()));
    let fid = gui_frame_for_font_change(&mut ev);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .set_known_parameter(FrameParam::Font, Value::string("Initial-11"));

    let out = ev.eval_str_each(
        r#"(list
            (condition-case err
                (progn (modify-frame-parameters nil '((font . "No Such Font"))) 'ok)
              (error (car (cdr err))))
            (condition-case err
                (progn (modify-frame-parameters nil '((font . monospace))) 'ok)
              (error (car (cdr err))))
            (frame-parameter nil 'font))"#,
    );
    assert_eq!(
        format_eval_result(&out[0]),
        r#"OK ("Font `No Such Font' is not defined" "Invalid font" "Initial-11")"#
    );
}

#[test]
fn modify_frame_parameters_font_rejects_a_symbol_before_resolution() {
    // GNU gui_set_font signals Invalid font for a symbol and includes the
    // value in the error data. A host that can open fonts must not be asked.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let host = opened_noto_host();
    let font_requests = host.font_requests().clone();
    ev.set_display_host(Box::new(host));
    let fid = gui_frame_for_font_change(&mut ev);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .set_known_parameter(FrameParam::Font, Value::string("Initial-11"));
    ev.eval_str("(fset 'frame-windows-min-size (lambda (&rest _) 1))")
        .expect("stub the absent window-minimum subr");

    let out = ev.eval_str_each(
        r#"(list
            (condition-case err
                (progn (modify-frame-parameters nil '((font . monospace))) 'ok)
              (error (cdr err)))
            (condition-case err
                (progn (modify-frame-parameters nil '((font . 42))) 'ok)
              (error (cdr err)))
            (frame-parameter nil 'font))"#,
    );
    assert_eq!(
        format_eval_result(&out[0]),
        r#"OK (("Invalid font" monospace) ("Invalid font" 42) "Initial-11")"#
    );
    assert!(
        font_requests.borrow().is_empty(),
        "a rejected selector must not be opened"
    );
}

#[test]
fn modify_frame_parameters_font_keeps_each_frames_explicit_face() {
    // set_lface_from_font reads the target frame's own lface slot. An
    // explicit family on another frame must not block this frame's font.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_display_host(Box::new(opened_noto_host()));
    let kept = gui_frame_for_font_change(&mut ev);
    let changed = gui_frame_for_font_change(&mut ev);
    ev.eval_str("(fset 'frame-windows-min-size (lambda (&rest _) 1))")
        .expect("stub the absent window-minimum subr");
    // Materialize the changed frame's face before the other frame's override
    // lands in the shared map.
    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("family"),
            Value::symbol("unspecified"),
            Value::make_frame(changed.0),
        ],
    )
    .expect("materialize the changed frame's family slot");
    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("family"),
            Value::string("Explicit Family"),
            Value::make_frame(kept.0),
        ],
    )
    .expect("set an explicit family on the other frame");
    ev.frame_manager_mut().select_frame(changed);

    let out = ev.eval_str_each(
        r#"(modify-frame-parameters (selected-frame) '((font . "Noto Sans Mono-16")))"#,
    );
    assert!(out[0].is_ok(), "font update failed: {:?}", out[0]);

    let family = |ev: &Context, fid: crate::window::FrameId| {
        crate::emacs_core::xfaces::runtime_face_table_from_frame_lisp_faces(ev, fid, false)
            .merge_faces(&["default"])
            .family
    };
    assert_eq!(
        family(&ev, changed),
        Some(Value::string("Noto Sans Mono")),
        "the changed frame's unspecified family must follow its opened font"
    );
    assert_eq!(
        family(&ev, kept),
        Some(Value::string("Explicit Family")),
        "the other frame's explicit family must stay"
    );
}

#[test]
fn modify_frame_parameters_font_updates_a_previously_seeded_family() {
    // A family written by opening a font is not a user setting. The next
    // font must replace it. An explicit height equal to the face base must
    // still survive, because that value was set on this frame.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_display_host(Box::new(opened_noto_host()));
    let fid = gui_frame_for_font_change(&mut ev);
    ev.eval_str("(fset 'frame-windows-min-size (lambda (&rest _) 1))")
        .expect("stub the absent window-minimum subr");
    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("height"),
            Value::fixnum(1),
            Value::make_frame(fid.0),
        ],
    )
    .expect("set an explicit height equal to the base value");

    let out = ev.eval_str_each(
        r#"(modify-frame-parameters (selected-frame) '((font . "Noto Sans Mono-16")))"#,
    );
    assert!(out[0].is_ok(), "first font update failed: {:?}", out[0]);

    let face = |ev: &Context| {
        crate::emacs_core::xfaces::runtime_face_table_from_frame_lisp_faces(ev, fid, false)
            .merge_faces(&["default"])
    };
    assert_eq!(
        face(&ev).family,
        Some(Value::string("Noto Sans Mono")),
        "the first opened font fills an unset family"
    );
    let explicit_height = face(&ev).height;
    assert_ne!(
        explicit_height,
        Some(crate::face::FaceHeight::Absolute(160)),
        "an explicit base height must not be replaced by the opened font"
    );

    ev.set_display_host(Box::new(RecordingDisplayHost::with_resolved_frame_font(
        resolved_frame_font(
            "Other Mono",
            "OtherMono-Regular",
            180,
            FontPxProbeResult {
                pixel_size: 24,
                height: 33,
                ascent: 24,
                descent: 9,
                max_width: 14,
                space_width: 14,
                average_width: 14,
            },
        ),
    )));
    let out = ev
        .eval_str_each(r#"(modify-frame-parameters (selected-frame) '((font . "Other Mono-18")))"#);
    assert!(out[0].is_ok(), "second font update failed: {:?}", out[0]);
    let face = face(&ev);
    assert_eq!(
        face.family,
        Some(Value::string("Other Mono")),
        "a family seeded by the previous font must follow the new font"
    );
    assert_eq!(
        face.height, explicit_height,
        "the explicit height must survive the second font"
    );
}

#[test]
fn modify_frame_parameters_font_honors_a_copied_default_face() {
    // `internal-copy-lisp-face` installs the source vector on this frame.
    // A concrete copied family must stick, and a copied `unspecified`
    // height must let the next font fill it. FRAME t copies only the
    // global definition, so it must leave this frame's mask alone.
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_display_host(Box::new(opened_noto_host()));
    let fid = gui_frame_for_font_change(&mut ev);
    let frame = Value::make_frame(fid.0);
    ev.eval_str("(fset 'frame-windows-min-size (lambda (&rest _) 1))")
        .expect("stub the absent window-minimum subr");
    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("source-face"),
            Value::keyword("family"),
            Value::string("Copied Family"),
            frame,
        ],
    )
    .expect("create the source face");
    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("height"),
            Value::fixnum(1),
            frame,
        ],
    )
    .expect("mark an explicit height on this frame");

    let out = ev.eval_str_each(
        r#"(modify-frame-parameters (selected-frame) '((font . "Noto Sans Mono-16")))"#,
    );
    assert!(out[0].is_ok(), "first font update failed: {:?}", out[0]);

    crate::emacs_core::xfaces::builtin_internal_copy_lisp_face(
        &mut ev,
        vec![
            Value::symbol("source-face"),
            Value::symbol("default"),
            Value::T,
            Value::NIL,
        ],
    )
    .expect("a global copy must not touch this frame");
    let face = |ev: &Context| {
        crate::emacs_core::xfaces::runtime_face_table_from_frame_lisp_faces(ev, fid, false)
            .merge_faces(&["default"])
    };
    let height_after_global_copy = face(&ev).height;
    assert_ne!(
        height_after_global_copy,
        Some(crate::face::FaceHeight::Absolute(160)),
        "FRAME t must not clear this frame's explicit height"
    );

    crate::emacs_core::xfaces::builtin_internal_copy_lisp_face(
        &mut ev,
        vec![
            Value::symbol("source-face"),
            Value::symbol("default"),
            frame,
            Value::NIL,
        ],
    )
    .expect("copy the source face onto this frame's default");
    assert_eq!(
        face(&ev).family,
        Some(Value::string("Copied Family")),
        "the copy installs the source family"
    );

    ev.set_display_host(Box::new(RecordingDisplayHost::with_resolved_frame_font(
        resolved_frame_font(
            "Other Mono",
            "OtherMono-Regular",
            180,
            FontPxProbeResult {
                pixel_size: 24,
                height: 33,
                ascent: 24,
                descent: 9,
                max_width: 14,
                space_width: 14,
                average_width: 14,
            },
        ),
    )));
    let out = ev
        .eval_str_each(r#"(modify-frame-parameters (selected-frame) '((font . "Other Mono-18")))"#);
    assert!(out[0].is_ok(), "second font update failed: {:?}", out[0]);
    let face = face(&ev);
    assert_eq!(
        face.family,
        Some(Value::string("Copied Family")),
        "a family installed by copy-face must survive the next font"
    );
    assert_eq!(
        face.height,
        Some(crate::face::FaceHeight::Absolute(180)),
        "a copied unspecified height must be filled by the next font"
    );
}

#[test]
fn x_create_frame_accepts_text_pixel_size_on_gui_child_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    let host = RecordingDisplayHost::new();
    let requests = host.realized.clone();
    ev.set_display_host(Box::new(host));

    let text_pixels = |n| Value::cons(Value::symbol("text-pixels"), Value::fixnum(n));
    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("width"), text_pixels(190)),
        Value::cons(Value::symbol("height"), text_pixels(78)),
        Value::cons(Value::symbol("child-frame-border-width"), Value::fixnum(2)),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(created.as_frame_id().expect("child frame"));

    let child = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(requests.borrow().len(), 0);
    // FRAME_TEXT_TO_PIXEL_WIDTH (src/frame.h:1790-1794): 190 text pixels + 2x8 px fringes +
    // the 16 px toolkit scroll-bar area (src/xfns.c:2393-2407) + two 2 px borders.
    assert_eq!(child.width, 226);
    assert_eq!(child.height, 82);
    assert_eq!(child.internal_border_width(), 2);
    assert_eq!(
        *child.root_window().bounds(),
        crate::window::Rect::new(2.0, 2.0, 222.0, 78.0)
    );
}

#[test]
fn modify_frame_parameters_resizes_gui_child_frame_text_pixels() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(scratch);
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    ev.set_display_host(Box::new(RecordingDisplayHost::new()));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("width"), Value::fixnum(1)),
        Value::cons(Value::symbol("height"), Value::fixnum(1)),
        Value::cons(Value::symbol("child-frame-border-width"), Value::fixnum(2)),
        Value::cons(Value::symbol("vertical-scroll-bars"), Value::NIL),
        Value::cons(Value::symbol("left-fringe"), Value::fixnum(0)),
        Value::cons(Value::symbol("right-fringe"), Value::fixnum(0)),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(created.as_frame_id().expect("child frame"));
    let text_pixels = |n| Value::cons(Value::symbol("text-pixels"), Value::fixnum(n));
    let alist = Value::list(vec![
        Value::cons(Value::symbol("width"), text_pixels(190)),
        Value::cons(Value::symbol("height"), text_pixels(78)),
    ]);

    crate::emacs_core::frame::builtin_modify_frame_parameters(&mut ev, vec![created, alist])
        .expect("modify-frame-parameters");

    let child = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(child.width, 194);
    assert_eq!(child.height, 82);
    assert_eq!(child.internal_border_width(), 2);
    assert_eq!(
        *child.root_window().bounds(),
        crate::window::Rect::new(2.0, 2.0, 190.0, 78.0)
    );
}

#[test]
fn modify_frame_parameters_resolves_child_fractional_size_before_position() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(scratch);
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 624, 648, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 8.0;
        parent.char_height = 16.0;
        parent.font_pixel_size = 16.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    ev.set_display_host(Box::new(RecordingDisplayHost::new()));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("width"), Value::fixnum(10)),
        Value::cons(Value::symbol("height"), Value::fixnum(1)),
        Value::cons(Value::symbol("child-frame-border-width"), Value::fixnum(3)),
        Value::cons(Value::symbol("left-fringe"), Value::fixnum(0)),
        Value::cons(Value::symbol("right-fringe"), Value::fixnum(0)),
        Value::cons(Value::symbol("vertical-scroll-bars"), Value::NIL),
        Value::cons(Value::symbol("horizontal-scroll-bars"), Value::NIL),
        Value::cons(Value::symbol("minibuffer"), Value::symbol("only")),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);
    let (xf_frames, xf_buffers, xf_host) = ev.frame_manager_buffers_and_display_host_mut();
    let child = super::x_create_frame_impl(xf_frames, xf_buffers, xf_host, vec![params])
        .expect("x-create-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));

    let alist = Value::list(vec![
        Value::cons(Value::symbol("width"), Value::make_float(0.5)),
        Value::cons(Value::symbol("height"), Value::fixnum(1)),
        Value::cons(Value::symbol("left"), Value::make_float(0.5)),
        Value::cons(Value::symbol("top"), Value::make_float(0.0)),
    ]);
    crate::emacs_core::frame::builtin_modify_frame_parameters(&mut ev, vec![child, alist])
        .expect("modify-frame-parameters");

    let child_native_width =
        crate::emacs_core::frame::builtin_frame_native_width(&mut ev, vec![child])
            .expect("frame-native-width");
    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(
        (
            child_native_width,
            child_frame.left_pos,
            child_frame.top_pos
        ),
        (Value::fixnum(312), 156, 0)
    );
}

#[test]
fn delete_frame_removes_gui_child_overlay_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("x")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    let host = RecordingDisplayHost::new();
    let removed_child_frames = host.removed_child_frames.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("width"), Value::fixnum(20)),
        Value::cons(Value::symbol("height"), Value::fixnum(5)),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(created.as_frame_id().expect("child frame id"));

    crate::emacs_core::frame::builtin_delete_frame(&mut ev, vec![created])
        .expect("delete child frame");

    assert!(ev.frame_manager().get(child_id).is_none());
    assert_eq!(*removed_child_frames.borrow(), vec![child_id]);
}

#[test]
fn make_frame_invisible_removes_gui_child_overlay_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    let host = RecordingDisplayHost::new();
    let removed_child_frames = host.removed_child_frames.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("width"), Value::fixnum(20)),
        Value::cons(Value::symbol("height"), Value::fixnum(5)),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(created.as_frame_id().expect("child frame id"));

    crate::emacs_core::frame::builtin_make_frame_invisible(&mut ev, vec![created])
        .expect("make child frame invisible");

    let child = ev
        .frame_manager()
        .get(child_id)
        .expect("child frame remains live");
    assert_eq!(child.visibility, crate::window::FrameVisibility::Invisible);
    assert_eq!(*removed_child_frames.borrow(), vec![child_id]);
}

#[test]
fn make_frame_visible_shows_gui_child_overlay_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    let host = RecordingDisplayHost::new();
    let shown_child_frames = host.shown_child_frames.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("width"), Value::fixnum(20)),
        Value::cons(Value::symbol("height"), Value::fixnum(5)),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(created.as_frame_id().expect("child frame id"));

    crate::emacs_core::frame::builtin_make_frame_invisible(&mut ev, vec![created])
        .expect("make child frame invisible");
    crate::emacs_core::frame::builtin_make_frame_visible(&mut ev, vec![created])
        .expect("make child frame visible");

    let child = ev
        .frame_manager()
        .get(child_id)
        .expect("child frame remains live");
    assert_eq!(child.visibility, crate::window::FrameVisibility::Visible);
    assert_eq!(*shown_child_frames.borrow(), vec![child_id]);
}

#[test]
fn delete_frame_destroys_top_level_gui_window_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let first_id = ev
        .frame_manager_mut()
        .create_frame("first", 960, 640, scratch);
    let second_id = ev
        .frame_manager_mut()
        .create_frame("second", 800, 500, scratch);
    for frame_id in [first_id, second_id] {
        ev.frame_manager_mut()
            .get_mut(frame_id)
            .expect("gui frame")
            .set_window_system(Some(Value::symbol("x")));
    }
    ev.frame_manager_mut().select_frame(first_id);
    let host = RecordingDisplayHost::new();
    let destroyed_gui_frames = host.destroyed_gui_frames.clone();
    let removed_child_frames = host.removed_child_frames.clone();
    ev.set_display_host(Box::new(host));

    crate::emacs_core::frame::builtin_delete_frame(&mut ev, vec![Value::make_frame(second_id.0)])
        .expect("delete top-level gui frame");

    assert!(ev.frame_manager().get(first_id).is_some());
    assert!(ev.frame_manager().get(second_id).is_none());
    assert_eq!(*destroyed_gui_frames.borrow(), vec![second_id]);
    assert!(removed_child_frames.borrow().is_empty());
}

#[test]
fn delete_parent_frame_cascades_to_gui_child_overlays() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    let other_id = ev
        .frame_manager_mut()
        .create_frame("other", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("x")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    let host = RecordingDisplayHost::new();
    let removed_child_frames = host.removed_child_frames.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("width"), Value::fixnum(20)),
        Value::cons(Value::symbol("height"), Value::fixnum(5)),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(created.as_frame_id().expect("child frame id"));

    crate::emacs_core::frame::builtin_delete_frame(&mut ev, vec![Value::make_frame(parent_id.0)])
        .expect("delete parent frame");

    assert!(ev.frame_manager().get(parent_id).is_none());
    assert!(ev.frame_manager().get(child_id).is_none());
    assert!(ev.frame_manager().get(other_id).is_some());
    assert_eq!(*removed_child_frames.borrow(), vec![child_id]);
}

#[test]
fn x_create_frame_reserves_tab_bar_space_above_root_window() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev
        .frame_manager_mut()
        .create_frame("bootstrap", 960, 640, scratch);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("bootstrap frame")
        .install_backend_less_gui_kind(WindowSystemKind::X);

    let params = Value::list(vec![
        Value::cons(Value::symbol("name"), Value::string("GUI")),
        Value::cons(Value::symbol("width"), Value::fixnum(80)),
        Value::cons(Value::symbol("height"), Value::fixnum(25)),
        // Isolate tab-bar space from the GUI menu/tool bars enabled by default.
        Value::cons(Value::symbol("menu-bar-lines"), Value::fixnum(0)),
        Value::cons(Value::symbol("tool-bar-lines"), Value::fixnum(0)),
        Value::cons(Value::symbol("tab-bar-lines"), Value::fixnum(1)),
        // Isolate the tab band from the mode-derived defaults installed at
        // frame creation; menu/tool bars have their own geometry tests.
        Value::cons(Value::symbol("menu-bar-lines"), Value::fixnum(0)),
        Value::cons(Value::symbol("tool-bar-lines"), Value::fixnum(0)),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");

    let created_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let frame = ev.frame_manager().get(created_id).expect("created frame");

    assert_eq!(frame.tab_bar_height, 16);
    assert_eq!(
        *frame.root_window().bounds(),
        crate::window::Rect::new(0.0, 16.0, 640.0, 368.0)
    );
    assert_eq!(
        *frame.minibuffer_leaf.as_ref().expect("minibuffer").bounds(),
        crate::window::Rect::new(0.0, 384.0, 640.0, 16.0)
    );
}

#[test]
fn x_create_frame_realizes_the_gui_frame_make_frame_delegates_to() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev
        .frame_manager_mut()
        .create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev
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
    ev.set_variable("terminal-frame", Value::make_frame(fid.0));

    let host = RecordingDisplayHost::new();
    let requests = host.realized.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(Value::symbol("name"), Value::string("GUI")),
        Value::cons(Value::symbol("width"), Value::fixnum(80)),
        Value::cons(Value::symbol("height"), Value::fixnum(25)),
    ]);
    // GNU owns `make-frame' in Lisp (lisp/frame.el:1019) and it funcalls
    // `frame-creation-function', which for a window system reaches the C
    // primitive `x-create-frame' (src/xfns.c).  DIVERGENCES.md 154 deleted the
    // Rust `make-frame' subr, whose GUI arm was one line -- a call to exactly
    // this -- so the test asks the primitive frame.el delegates to.
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");

    let created_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let frame = ev
        .frame_manager()
        .get(created_id)
        .expect("created opening frame");
    assert_eq!(frame.effective_window_system(), Some(Value::symbol("neo")));
    assert_eq!(frame.width, 800);
    assert_eq!(frame.height, 500);
    assert_eq!(
        ev.frame_manager()
            .selected_frame()
            .expect("selected frame")
            .id,
        created_id
    );

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, created_id);
    assert_eq!(requests[0].width, 800);
    assert_eq!(requests[0].height, 500);
}

#[test]
fn x_create_frame_prefers_display_host_primary_window_size_without_explicit_geometry() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev
        .frame_manager_mut()
        .create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev
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
    ev.set_variable("terminal-frame", Value::make_frame(fid.0));

    let host = RecordingDisplayHost::with_primary_size(1500, 1900);
    let requests = host.realized.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(Value::symbol("name"), Value::string("Neomacs")),
        Value::cons(Value::symbol("title"), Value::string("Neomacs")),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");

    let created_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let frame = ev
        .frame_manager()
        .get(created_id)
        .expect("created opening frame");
    assert_eq!(frame.width, 1500);
    assert_eq!(frame.height, 1900);

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].width, 1500);
    assert_eq!(requests[0].height, 1900);
    assert_eq!(
        ev.frame_manager()
            .selected_frame()
            .expect("selected frame")
            .id,
        created_id
    );
}

#[test]
fn delete_frame_uses_mru_only_for_the_mru_symbol() {
    // GNU 31.1 frame.c tests EQ(value, Qmru), not truthiness.  Its fallback
    // walks Vframe_list, where newly created frames are prepended.
    for (setting, expected) in [
        ("'mru", "OK t"),
        ("nil", "OK nil"),
        ("t", "OK nil"),
        ("'custom", "OK nil"),
    ] {
        let mut ev = Context::new(crate::test_utils::unit_runtime());
        let buffer = ev.edit.buffers.create_buffer("*scratch*");
        ev.edit.buffers.set_current(buffer);
        let recent = ev
            .frame_manager_mut()
            .create_frame("recent", 800, 600, buffer);
        ev.frame_manager_mut()
            .create_frame("fallback", 800, 600, buffer);
        let victim = ev
            .frame_manager_mut()
            .create_frame("victim", 800, 600, buffer);
        ev.obarray_mut()
            .set_symbol_value("test-recent-frame", Value::make_frame(recent.0));
        ev.obarray_mut()
            .set_symbol_value("test-victim-frame", Value::make_frame(victim.0));
        let form = format!(
            r##"
(progn
  ;; GNU do_switch_frame returns immediately for the selected frame. Leave
  ;; recent first so the next selection genuinely records its use time.
  (select-frame test-victim-frame)
  (select-frame test-recent-frame)
  (select-frame test-victim-frame)
  (let ((delete-frame-choose-selected {setting}))
    (delete-frame test-victim-frame))
  (eq (selected-frame) test-recent-frame))
"##
        );
        assert_eq!(
            format_eval_result(&ev.eval_str(&form)),
            expected,
            "setting {setting}"
        );
    }
}

#[test]
fn delete_frame_errors_on_sole_frame_without_force() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        "(condition-case err
             (delete-frame nil)
           (error err))",
    );
    assert_eq!(
        result,
        "OK (error \"Attempt to delete the sole visible or iconified frame\")"
    );
}

#[test]
fn delete_frame_force_errors_on_only_frame() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        "(condition-case err
             (delete-frame nil t)
           (error err))",
    );
    assert_eq!(result, "OK (error \"Attempt to delete the only frame\")");
}

#[test]
fn deleting_last_frame_on_terminal_deletes_terminal_too() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let _primary = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    crate::emacs_core::terminal::pure::ensure_terminal_runtime_owner(
        7,
        "tty-7",
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::interactive(
            Some("xterm-256color".to_string()),
            neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256),
        ),
    );
    let secondary = ev
        .frame_manager_mut()
        .create_frame_on_terminal("F2", 7, 800, 600, buf);
    let secondary_terminal =
        crate::emacs_core::terminal::pure::terminal_handle_value_for_id(7).expect("terminal 7");

    assert_eq!(
        crate::emacs_core::frame::builtin_delete_frame(
            &mut ev,
            vec![Value::make_frame(secondary.0)]
        )
        .unwrap(),
        Value::NIL
    );
    assert!(
        crate::emacs_core::terminal::pure::builtin_terminal_live_p(
            &mut ev,
            &vec![secondary_terminal]
        )
        .unwrap()
        .is_nil(),
        "deleting the last frame on a terminal should tear down that terminal"
    );
}

#[test]
fn framep_true() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(framep (selected-frame))");
    assert_eq!(r, "OK t");
}

#[test]
fn framep_returns_window_system_symbol_for_gui_frames() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let frame_id = super::ensure_selected_frame_id(&mut ev);
    ev.frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame")
        .install_backend_less_gui_kind(WindowSystemKind::X);

    let result =
        crate::emacs_core::frame::builtin_framep(&mut ev, vec![Value::make_frame(frame_id.0)])
            .unwrap();
    assert_eq!(result, Value::symbol("x"));
}

#[test]
fn framep_false() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(framep 999999)");
    assert_eq!(r, "OK nil");
}

#[test]
fn frame_live_p_true() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(frame-live-p (selected-frame))");
    assert_eq!(r, "OK t");
}

#[test]
fn frame_live_p_false() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(frame-live-p 999999)");
    assert_eq!(r, "OK nil");
}

#[test]
fn frame_builtins_accept_frame_handle_values() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let fid = super::ensure_selected_frame_id(&mut ev);
    let frame = Value::make_frame(fid.0);

    assert_eq!(
        crate::emacs_core::frame::builtin_framep(&mut ev, vec![frame]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_live_p(&mut ev, &vec![frame]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_visible_p(&mut ev, vec![frame]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_select_frame(&mut ev, vec![frame]).unwrap(),
        Value::make_frame(fid.0)
    );
    // `select-frame-set-input-focus' is lisp/frame.el:1262 and has no Rust
    // subr any more (DIVERGENCES.md 154).  Its body is `select-frame' --
    // asserted just above -- plus `raise-frame' and `x-focus-frame'.
    // `raise-frame' is the one of those two a NON-graphical frame handle must
    // still be accepted by; `x-focus-frame' signals unless the frame has a
    // window system, and is asserted in display_test.rs where one does.
    assert_eq!(
        neovm_eval::emacs_core::builtins::symbols::builtin_raise_frame(vec![frame]).unwrap(),
        Value::NIL
    );
}

#[test]
fn iconify_top_level_tty_frame_is_a_noop() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let fid = super::ensure_selected_frame_id(&mut ev);
    let frame = Value::make_frame(fid.0);

    assert_eq!(
        crate::emacs_core::frame::builtin_iconify_frame(&mut ev, vec![frame]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_visible_p(&mut ev, vec![frame]).unwrap(),
        Value::T
    );
}

#[test]
fn iconify_top_level_gui_frame_reports_iconified_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let fid = super::ensure_selected_frame_id(&mut ev);
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));
    let host = RecordingDisplayHost::new();
    let visibility_changes = host.visibility_changes.clone();
    ev.set_display_host(Box::new(host));
    let frame = Value::make_frame(fid.0);

    assert_eq!(
        crate::emacs_core::frame::builtin_iconify_frame(&mut ev, vec![frame]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_visible_p(&mut ev, vec![frame]).unwrap(),
        Value::symbol("icon")
    );
    assert_eq!(
        *visibility_changes.borrow(),
        vec![(fid, crate::window::FrameVisibility::Iconified)]
    );
}

#[test]
fn select_frame_switches_active_kboard_to_frame_terminal() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let primary = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    ev.command_loop_mut()
        .keyboard
        .set_input_decode_map(Value::symbol("primary-map"));

    crate::emacs_core::terminal::pure::ensure_terminal_runtime_owner(
        7,
        "tty-7",
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::interactive(
            Some("xterm-256color".to_string()),
            neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256),
        ),
    );
    let secondary = ev
        .frame_manager_mut()
        .create_frame_on_terminal("F2", 7, 800, 600, buf);

    assert_eq!(
        crate::emacs_core::frame::builtin_select_frame(
            &mut ev,
            vec![Value::make_frame(secondary.0)]
        )
        .expect("select secondary frame"),
        Value::make_frame(secondary.0)
    );
    assert_eq!(ev.command_loop().keyboard.active_terminal_id(), 7);
    assert_eq!(ev.command_loop().keyboard.input_decode_map(), Value::NIL);

    ev.command_loop_mut()
        .keyboard
        .set_input_decode_map(Value::symbol("secondary-map"));

    assert_eq!(
        crate::emacs_core::frame::builtin_select_frame(&mut ev, vec![Value::make_frame(primary.0)])
            .expect("reselect primary frame"),
        Value::make_frame(primary.0)
    );
    assert_eq!(
        ev.command_loop().keyboard.input_decode_map(),
        Value::symbol("primary-map")
    );

    crate::emacs_core::frame::builtin_select_frame(&mut ev, vec![Value::make_frame(secondary.0)])
        .expect("reselect secondary frame");
    assert_eq!(
        ev.command_loop().keyboard.input_decode_map(),
        Value::symbol("secondary-map")
    );
}

#[test]
fn frame_visible_p_requires_one_arg() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(condition-case err (frame-visible-p) (error (car err)))");
    assert_eq!(r, "OK wrong-number-of-arguments");
}

#[test]
fn frame_parameter_name() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(frame-parameter (selected-frame) 'name)");
    assert_eq!(r, r#"OK "F1""#);
}

#[test]
fn frame_parameter_explicit_name_defaults_to_nil() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(frame-parameter (selected-frame) 'explicit-name)");
    assert_eq!(r, "OK nil");
}

#[test]
fn frame_parameter_icon_name_defaults_to_nil() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(frame-parameter (selected-frame) 'icon-name)");
    assert_eq!(r, "OK nil");
}

#[test]
fn frame_parameter_minibuffer_defaults_to_t() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(frame-parameter (selected-frame) 'minibuffer)");
    assert_eq!(r, "OK t");
}

#[test]
fn frame_focus_defaults_to_nil() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(frame-focus)");
    assert_eq!(r, "OK nil");
}

#[test]
fn redirect_frame_focus_tracks_frame_state_and_selection_redirects() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    let primary = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let secondary = ev.frame_manager_mut().create_frame("F2", 800, 600, buf);

    assert_eq!(
        crate::emacs_core::frame::builtin_frame_focus(&mut ev, vec![Value::make_frame(primary.0)])
            .unwrap(),
        Value::NIL
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_redirect_frame_focus(
            &mut ev,
            vec![Value::make_frame(primary.0), Value::make_frame(primary.0)],
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_focus(&mut ev, vec![Value::make_frame(primary.0)])
            .unwrap(),
        Value::make_frame(primary.0)
    );

    crate::emacs_core::frame::builtin_select_frame(&mut ev, vec![Value::make_frame(secondary.0)])
        .expect("select secondary frame");
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_focus(&mut ev, vec![Value::make_frame(primary.0)])
            .unwrap(),
        Value::make_frame(secondary.0)
    );

    assert_eq!(
        crate::emacs_core::frame::builtin_redirect_frame_focus(
            &mut ev,
            vec![Value::make_frame(primary.0)]
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_focus(&mut ev, vec![Value::make_frame(primary.0)])
            .unwrap(),
        Value::NIL
    );
}

#[test]
fn frame_parameter_width() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(frame-parameter (selected-frame) 'width)");
    assert_eq!(r, "OK 100");
}

#[test]
fn frame_parameter_height() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(frame-parameter (selected-frame) 'height)");
    assert_eq!(r, "OK 37");
}

#[test]
fn frame_parameters_returns_alist() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(listp (frame-parameters))");
    assert_eq!(r, "OK t");
}

#[test]
fn tty_frame_font_and_cursor_parameters_match_gnu_defaults() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame(
        r#"(let ((f (selected-frame)))
             (list (frame-parameter f 'font)
                   (cdr (assq 'font (frame-parameters f)))
                   (frame-parameter f 'cursor-color)
                   (frame-parameter f 'foreground-color)
                   (frame-parameter f 'background-color)))"#,
    );
    assert_eq!(
        r,
        r#"OK ("tty" "tty" "white" "unspecified-fg" "unspecified-bg")"#
    );
}

#[test]
fn gui_frame_font_parameter_exposes_font_name_not_font_object() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let mut font_face = crate::face::Face::new("default");
    font_face.family = Some(Value::string("Hack"));
    font_face.weight = Some(FontWeight::NORMAL);
    font_face.slant = Some(FontSlant::Normal);
    font_face.height = Some(crate::face::FaceHeight::Absolute(27));
    let font_object = crate::emacs_core::font::build_font_object(&font_face);
    let public_font_name =
        crate::emacs_core::font::font_get(vec![font_object, Value::keyword("name")])
            .expect("opened font name");
    let public_font_name_text = public_font_name
        .as_utf8_str()
        .expect("opened font name string")
        .to_owned();
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_known_parameter(FrameParam::Font, font_object);
    }

    let frame_value = Value::make_frame(fid.0);
    let direct = crate::emacs_core::frame::builtin_frame_parameter(
        &mut ev,
        &vec![frame_value, FrameParam::Font.symbol()],
    )
    .expect("frame-parameter");
    assert_eq!(direct.as_utf8_str(), Some(public_font_name_text.as_str()));

    let via_lisp = ev
        .eval_str_each(
            r#"(list (frame-parameter (selected-frame) 'font)
                     (cdr (assq 'font (frame-parameters (selected-frame)))))"#,
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        via_lisp[0],
        format!("OK (\"{public_font_name_text}\" \"{public_font_name_text}\")")
    );
}

#[test]
fn modify_frame_parameters_name() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(modify-frame-parameters (selected-frame) '((name . \"NewName\")))
         (frame-parameter (selected-frame) 'name)
         (frame-parameter (selected-frame) 'explicit-name)
         (assq 'explicit-name (frame-parameters))",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], r#"OK "NewName""#);
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK nil");
}

#[test]
fn modify_frame_parameters_name_nil_restores_generated_name() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(modify-frame-parameters (selected-frame) '((name . \"NewName\")))
         (modify-frame-parameters (selected-frame) '((name . nil)))
         (frame-parameter (selected-frame) 'name)
         (frame-parameter (selected-frame) 'explicit-name)",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK nil");
    assert_eq!(results[2], r#"OK "F1""#);
    assert_eq!(results[3], "OK nil");
}

#[test]
fn modify_frame_parameters_top_level_tty_visibility_reports_live_state() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(modify-frame-parameters (selected-frame) '((visibility . nil)))
         (frame-visible-p (selected-frame))
         (frame-parameter nil 'visibility)
         (assq 'visibility (frame-parameters))",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK t");
    assert_eq!(results[3], "OK (visibility . t)");
}

#[test]
fn modify_frame_parameters_icon_name_tracks_frame_field() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(modify-frame-parameters (selected-frame) '((icon-name . \"frame-icon\")))
         (frame-parameter (selected-frame) 'icon-name)
         (modify-frame-parameters (selected-frame) '((icon-name . nil)))
         (frame-parameter (selected-frame) 'icon-name)",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], r#"OK "frame-icon""#);
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK nil");
}

#[test]
fn tty_divider_width_builtins_return_zero_but_keep_parameters_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(modify-frame-parameters
           (selected-frame)
           '((right-divider-width . 6) (bottom-divider-width . 4)))
         (list (frame-right-divider-width)
               (frame-bottom-divider-width)
               (cdr (assq 'right-divider-width (frame-parameters)))
               (cdr (assq 'bottom-divider-width (frame-parameters))))",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK (0 0 6 4)");
}

#[test]
fn gui_divider_width_builtins_read_effective_gnu_values() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_gui_frame(
        "(modify-frame-parameters
           (selected-frame)
           '((right-divider-width . 6) (bottom-divider-width . 4)))
         (list (frame-right-divider-width)
               (frame-bottom-divider-width))",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK (6 4)");
}

#[test]
fn tty_frame_border_width_builtins_return_zero_but_keep_parameters_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(modify-frame-parameters
           (selected-frame)
           '((internal-border-width . 4) (child-frame-border-width . 2)))
         (list (frame-internal-border-width)
               (frame-child-frame-border-width)
               (cdr (assq 'internal-border-width (frame-parameters)))
               (cdr (assq 'child-frame-border-width (frame-parameters))))",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK (0 0 4 2)");
}

#[test]
fn gui_frame_border_width_builtins_read_effective_gnu_values() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_gui_frame(
        "(modify-frame-parameters
           (selected-frame)
           '((internal-border-width . 4) (child-frame-border-width . 2)))
         (list (frame-internal-border-width)
               (frame-child-frame-border-width))",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK (4 2)");
}

#[test]
fn neomacs_frame_edges_return_numeric_gui_edges_like_gnu_toolkits() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_gui_frame(
        "(modify-frame-parameters (selected-frame) '((internal-border-width . 4)))
         (list (neomacs-frame-edges)
               (neomacs-frame-edges nil 'native-edges)
               (neomacs-frame-edges nil 'outer-edges)
               (neomacs-frame-edges nil 'inner-edges))",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(
        results[1],
        "OK ((0 0 800 600) (0 0 800 600) (0 0 800 600) (4 4 796 596))"
    );
}

#[test]
fn modify_frame_parameters_top_level_tty_reports_live_width_height() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let (initial_width, initial_height) = {
        let frame = ev.frame_manager().get(fid).expect("frame should exist");
        (
            Value::fixnum(frame.columns() as i64),
            Value::fixnum(frame.lines() as i64),
        )
    };
    let out = ev.eval_str_each(
        "(progn
           (modify-frame-parameters
            (selected-frame) '((width . 90) (height . 30) (left . 7) (top . 8)))
           (list (frame-parameter nil 'width)
                 (frame-parameter nil 'height)
                 (frame-parameter nil 'left)
                 (frame-parameter nil 'top)))",
    );
    assert!(
        out[0].is_ok(),
        "modify-frame-parameters failed: {:?}",
        out[0]
    );

    let value = out[0].as_ref().expect("eval result");
    let items = neovm_value::value::list_to_vec(value).expect("result list");
    assert_eq!(items[0], initial_width);
    assert_eq!(items[1], initial_height);
    assert_eq!(items[2], Value::fixnum(7));
    assert_eq!(items[3], Value::fixnum(8));
}

#[test]
fn modify_frame_parameters_width_height_resizes_live_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let host = RecordingDisplayHost::new();
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
    }

    let out = ev
        .eval_str_each("(modify-frame-parameters (selected-frame) '((width . 80) (height . 25)))");
    assert!(
        out[0].is_ok(),
        "modify-frame-parameters failed: {:?}",
        out[0]
    );

    let resize_requests = resized.borrow();
    assert_eq!(resize_requests.len(), 1);
    assert_eq!(resize_requests[0].frame_id, fid);
    // FRAME_TEXT_TO_PIXEL_WIDTH (src/frame.h:1790-1794): 80 columns x 8 px + 2x8 px
    // fringes + the 16 px toolkit scroll-bar area (src/xfns.c:2393-2407).
    assert_eq!(resize_requests[0].width, 672);
    assert_eq!(resize_requests[0].height, 400);
    assert_eq!(
        resize_requests[0].geometry_hints,
        ev.frame_manager()
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );

    let frame = ev.frame_manager().get(fid).expect("frame should exist");
    assert_eq!(frame.width, 672);
    assert_eq!(frame.height, 400);
    assert_eq!(frame.parameter("width"), Some(Value::fixnum(80)));
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(25)));
}

#[test]
fn modify_frame_parameters_fullscreen_maximized_updates_live_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let host = RecordingDisplayHost::new();
    let fullscreen_changes = host.fullscreen_changes.clone();
    ev.set_display_host(Box::new(host));
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }

    let out = ev.eval_str_each(
        "(progn
           (modify-frame-parameters
            (selected-frame) '((fullscreen . maximized)))
           (frame-parameter nil 'fullscreen))",
    );

    assert!(out[0].is_ok(), "fullscreen update failed: {:?}", out[0]);
    assert_eq!(
        out[0].as_ref().expect("result"),
        &Value::symbol("maximized")
    );
    assert_eq!(
        fullscreen_changes.borrow().as_slice(),
        &[(fid, FrameFullscreen::Maximized)]
    );
}

#[test]
fn gui_frame_metrics_default_minibuffer_is_one_line() {
    // Regression for the Doom "echo area is two lines" bug. A frame without its
    // own minibuffer_leaf (e.g. one sharing the parent's minibuffer) used to
    // default its minibuffer height to TWO text lines. Every GUI frame seeded
    // from those metrics then started with a two-line echo area, and grow-only
    // `resize-mini-windows` never shrinks an over-allocated mini-window, so it
    // stayed two lines forever. GNU `make-frame` defaults the minibuffer to a
    // single line.
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.char_height = 20.0;
        frame.minibuffer_leaf = None;
    }
    ev.frame_manager_mut().select_frame(fid);

    let metrics = super::current_gui_frame_metrics_in_state(&ev.frame_manager());
    assert_eq!(
        metrics.minibuffer_height, 20.0,
        "a frame's default minibuffer must be one text line (char_height), not two"
    );
}

#[test]
fn modify_frame_parameters_resize_ignores_window_local_fringes_for_gui_frames() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let host = RecordingDisplayHost::new();
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let selected_window = {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
        frame.selected_window
    };
    assert!(
        ev.frame_manager_mut()
            .set_window_fringes(selected_window, Some(5), Some(5), false, false),
        "window-local fringes should change"
    );

    let out = ev
        .eval_str_each("(modify-frame-parameters (selected-frame) '((width . 80) (height . 25)))");
    assert!(
        out[0].is_ok(),
        "modify-frame-parameters failed: {:?}",
        out[0]
    );

    let resize_requests = resized.borrow();
    assert_eq!(resize_requests.len(), 1);
    assert_eq!(resize_requests[0].frame_id, fid);
    // FRAME_TEXT_TO_PIXEL_WIDTH (src/frame.h:1790-1794): 80 columns x 8 px + 2x8 px
    // fringes + the 16 px toolkit scroll-bar area (src/xfns.c:2393-2407).
    assert_eq!(resize_requests[0].width, 672);
    assert_eq!(resize_requests[0].height, 400);
    assert_eq!(
        resize_requests[0].geometry_hints,
        ev.frame_manager()
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );
}

#[test]
fn modify_frame_parameters_tab_bar_lines_reflows_root_window_tree() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.char_width = 10.0;
        frame.char_height = 20.0;
        // GNU offsets the tab/tool-bar only on a displayed frame (commit
        // 1030f559b); mark this test frame displayed so the reflow reserves
        // the bar above the root window.
        frame.displays_chrome = true;
    }

    let out = ev.eval_str_each("(modify-frame-parameters (selected-frame) '((tab-bar-lines . 1)))");
    assert!(
        out[0].is_ok(),
        "modify-frame-parameters failed: {:?}",
        out[0]
    );

    let frame = ev.frame_manager().get(fid).expect("frame should exist");
    assert_eq!(frame.tab_bar_height, 20);
    assert_eq!(
        *frame.root_window().bounds(),
        crate::window::Rect::new(0.0, 20.0, 800.0, 564.0)
    );
    assert_eq!(
        *frame.minibuffer_leaf.as_ref().expect("minibuffer").bounds(),
        crate::window::Rect::new(0.0, 584.0, 800.0, 16.0)
    );
}

#[test]
fn modify_frame_parameters_tool_bar_lines_reflows_root_window_tree() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.char_width = 10.0;
        frame.char_height = 20.0;
        // GNU offsets the tab/tool-bar only on a displayed frame (commit
        // 1030f559b); mark this test frame displayed so the reflow reserves
        // the bar above the root window.
        frame.displays_chrome = true;
    }

    let out =
        ev.eval_str_each("(modify-frame-parameters (selected-frame) '((tool-bar-lines . 2)))");
    assert!(
        out[0].is_ok(),
        "modify-frame-parameters failed: {:?}",
        out[0]
    );

    let frame = ev.frame_manager().get(fid).expect("frame should exist");
    assert_eq!(frame.tool_bar_height, 40);
    assert_eq!(
        *frame.root_window().bounds(),
        crate::window::Rect::new(0.0, 40.0, 800.0, 544.0)
    );
    assert_eq!(
        *frame.minibuffer_leaf.as_ref().expect("minibuffer").bounds(),
        crate::window::Rect::new(0.0, 584.0, 800.0, 16.0)
    );
}

#[test]
fn set_frame_size_builtins_leave_top_level_tty_size_unchanged() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let out = ev.eval_str_each(
        "(progn
           (set-frame-width (selected-frame) 90)
           (set-frame-height (selected-frame) 30)
           (set-frame-size (selected-frame) 100 35))",
    );
    assert!(
        out[0].is_ok(),
        "set-frame-size builtins failed: {:?}",
        out[0]
    );

    let frame = ev.frame_manager().get(fid).expect("frame should exist");
    assert_eq!(frame.width, 800);
    assert_eq!(frame.height, 600);
    assert_eq!(frame.parameter("width"), None);
    assert_eq!(frame.parameter("height"), None);
    assert_eq!(frame.parameter("neovm--frame-text-lines"), None);
}

#[test]
fn set_frame_size_builtins_resize_live_gui_frames_and_notify_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev
            .frame_manager_mut()
            .get_mut(fid)
            .expect("frame should exist");
        frame.install_backend_less_gui_kind(WindowSystemKind::X);
    }
    let host = RecordingDisplayHost::new();
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));

    let out = ev.eval_str_each("(set-frame-size (selected-frame) 100 35)");
    assert!(
        out[0].is_ok(),
        "set-frame-size builtins failed: {:?}",
        out[0]
    );

    let frame = ev.frame_manager().get(fid).expect("frame should exist");
    assert_eq!(frame.width, 800);
    assert_eq!(frame.height, 600);
    assert_eq!(frame.parameter("width"), None);
    assert_eq!(frame.parameter("height"), None);
    assert_eq!(frame.parameter("neovm--frame-text-lines"), None);

    let requests = resized.borrow();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.frame_id, fid);
    // 100 columns x 8 px + 2x8 px fringes + the 16 px toolkit scroll-bar area
    // (FRAME_TEXT_TO_PIXEL_WIDTH, src/frame.h:1790-1794; src/xfns.c:2393-2407).
    assert_eq!(request.width, 832);
    assert_eq!(request.height, 560);
    assert_eq!(
        request.geometry_hints,
        ev.frame_manager()
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );

    drop(requests);

    ev.apply_resize_input_event(832, 560, 1.0, fid.0, false)
        .expect("resize redisplay");

    let frame = ev
        .frame_manager()
        .get(fid)
        .expect("frame should exist after host ack");
    assert_eq!(frame.width, 832);
    assert_eq!(frame.height, 560);
    assert_eq!(frame.parameter("width"), Some(Value::fixnum(100)));
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(35)));
    assert_eq!(
        frame.parameter("neovm--frame-text-lines"),
        Some(Value::fixnum(34))
    );
}

#[test]
fn x_create_frame_minibuffer_only_uses_root_as_minibuffer() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let root_id = ev.frame_manager_mut().create_frame("F1", 624, 648, buf);
    {
        let root = ev.frame_manager_mut().get_mut(root_id).expect("root frame");
        root.set_window_system(Some(Value::symbol("neo")));
        root.char_width = 7.2;
        root.char_height = 17.0;
        root.font_pixel_size = 12.0;
    }
    ev.set_display_host(Box::new(RecordingDisplayHost::new()));

    let params = Value::list(vec![
        Value::cons(Value::symbol("parent-frame"), Value::make_frame(root_id.0)),
        Value::cons(Value::symbol("minibuffer"), Value::symbol("only")),
        Value::cons(Value::symbol("height"), Value::fixnum(1)),
        Value::cons(Value::symbol("child-frame-border-width"), Value::fixnum(3)),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);
    let (xf_frames, xf_buffers, xf_host) = ev.frame_manager_buffers_and_display_host_mut();
    let child = super::x_create_frame_impl(xf_frames, xf_buffers, xf_host, vec![params])
        .expect("x-create-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));
    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    let root_window_id = child_frame.root_window().id();

    assert!(child_frame.minibuffer_leaf.is_none());
    assert_eq!(child_frame.minibuffer_window, Some(root_window_id));
    let crate::window::Window::Leaf { buffer_id, .. } = &child_frame.root_window() else {
        panic!("child frame root must be a leaf window");
    };
    assert!(
        ev.edit
            .buffers
            .get(*buffer_id)
            .expect("root buffer")
            .has_name(" *Minibuf-0*")
    );
    assert!(child_frame.no_split);
    assert_eq!(
        child_frame.parameter("minibuffer"),
        Some(Value::symbol("only"))
    );
}

#[test]
fn pixelwise_child_frame_resize_preserves_requested_text_pixels() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let root_id = ev.frame_manager_mut().create_frame("F1", 624, 648, buf);
    {
        let root = ev.frame_manager_mut().get_mut(root_id).expect("root frame");
        root.set_window_system(Some(Value::symbol("neo")));
        root.char_width = 7.2;
        root.char_height = 17.0;
        root.font_pixel_size = 12.0;
    }
    ev.set_display_host(Box::new(RecordingDisplayHost::new()));
    let root_minibuffer = ev
        .frame_manager()
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let params = Value::list(vec![
        Value::cons(Value::symbol("parent-frame"), Value::make_frame(root_id.0)),
        Value::cons(Value::symbol("width"), Value::fixnum(1)),
        Value::cons(Value::symbol("height"), Value::fixnum(1)),
        Value::cons(Value::symbol("child-frame-border-width"), Value::fixnum(1)),
        Value::cons(Value::symbol("left-fringe"), Value::fixnum(0)),
        Value::cons(Value::symbol("right-fringe"), Value::fixnum(0)),
        Value::cons(Value::symbol("vertical-scroll-bars"), Value::NIL),
        Value::cons(Value::symbol("horizontal-scroll-bars"), Value::NIL),
        Value::cons(
            Value::symbol("minibuffer"),
            Value::make_window(root_minibuffer.0),
        ),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);
    let (xf_frames, xf_buffers, xf_host) = ev.frame_manager_buffers_and_display_host_mut();
    let child = super::x_create_frame_impl(xf_frames, xf_buffers, xf_host, vec![params])
        .expect("x-create-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));

    crate::emacs_core::frame::builtin_set_frame_size(
        &mut ev,
        vec![child, Value::fixnum(525), Value::fixnum(374), Value::T],
    )
    .expect("first pixelwise set-frame-size");
    crate::emacs_core::frame::builtin_set_frame_size(
        &mut ev,
        vec![child, Value::fixnum(525), Value::fixnum(374), Value::T],
    )
    .expect("second pixelwise set-frame-size");

    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(
        super::frame_text_width_pixels_in_state(&ev.frame_manager(), child_id),
        525
    );
    assert_eq!(super::frame_text_height_pixels(child_frame), 374);
    assert_eq!(child_frame.width, 527);
    assert_eq!(child_frame.height, 376);
}

#[test]
fn set_frame_size_and_position_pixelwise_resizes_gui_child_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let root_id = ev.frame_manager_mut().create_frame("F1", 624, 648, buf);
    {
        let root = ev.frame_manager_mut().get_mut(root_id).expect("root frame");
        root.set_window_system(Some(Value::symbol("neo")));
        root.char_width = 7.2;
        root.char_height = 17.0;
        root.font_pixel_size = 12.0;
    }
    ev.set_display_host(Box::new(RecordingDisplayHost::new()));
    let root_minibuffer = ev
        .frame_manager()
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let params = Value::list(vec![
        Value::cons(Value::symbol("parent-frame"), Value::make_frame(root_id.0)),
        Value::cons(Value::symbol("width"), Value::fixnum(0)),
        Value::cons(Value::symbol("height"), Value::fixnum(0)),
        Value::cons(Value::symbol("child-frame-border-width"), Value::fixnum(1)),
        Value::cons(Value::symbol("left-fringe"), Value::fixnum(0)),
        Value::cons(Value::symbol("right-fringe"), Value::fixnum(0)),
        Value::cons(Value::symbol("vertical-scroll-bars"), Value::NIL),
        Value::cons(Value::symbol("horizontal-scroll-bars"), Value::NIL),
        Value::cons(
            Value::symbol("minibuffer"),
            Value::make_window(root_minibuffer.0),
        ),
        Value::cons(Value::symbol("visibility"), Value::NIL),
    ]);
    let (xf_frames, xf_buffers, xf_host) = ev.frame_manager_buffers_and_display_host_mut();
    let child = super::x_create_frame_impl(xf_frames, xf_buffers, xf_host, vec![params])
        .expect("x-create-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));

    crate::emacs_core::frame::builtin_set_frame_size_and_position_pixelwise(
        &mut ev,
        vec![
            child,
            Value::fixnum(525),
            Value::fixnum(374),
            Value::fixnum(21),
            Value::fixnum(31),
        ],
    )
    .expect("set-frame-size-and-position-pixelwise");

    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(
        super::frame_text_width_pixels_in_state(&ev.frame_manager(), child_id),
        525
    );
    assert_eq!(super::frame_text_height_pixels(child_frame), 374);
    assert_eq!(child_frame.width, 527);
    assert_eq!(child_frame.height, 376);
    assert_eq!(child_frame.left_pos, 21);
    assert_eq!(child_frame.top_pos, 31);
    assert_eq!(child_frame.parameter("left"), Some(Value::fixnum(21)));
    assert_eq!(child_frame.parameter("top"), Some(Value::fixnum(31)));
}

#[test]
fn window_end_reads_the_atomic_record_when_a_snapshot_disagrees() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    ev.edit
        .buffers
        .get_mut(buf)
        .expect("scratch buffer")
        .insert("hello\nworld\nmore\n");
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;
    let point_max = ev
        .edit
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_char_pos()
        .get();
    let buffer_z_byte = ev
        .edit
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_emacs_byte_pos();

    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        let Some(window) = frame.find_window_mut(wid) else {
            panic!("selected window should exist");
        };
        let crate::window::Window::Leaf { window_start, .. } = window else {
            panic!("selected window should be a leaf");
        };
        *window_start = LispCharPos1::ONE;
        window.set_window_end_from_positions(
            LispCharPos1::from_one_based_usize(point_max.saturating_add(1)),
            buffer_z_byte,
            LispCharPos1::new(8),
            EmacsBytePos::new(7),
            0,
        );

        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            point_rows: None,
            window_id: wid,
            rows: vec![crate::window::DisplayRowSnapshot {
                truncated_end_buffer_pos: None,
                row: 0,
                y: 0,
                height: 16,
                extra_line_spacing: 0,
                start_x: 0,
                start_col: 0,
                end_x: 0,
                end_col: 0,
                start_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(1)),
                end_buffer_pos: Some(neovm_edit::buffer::LispCharPos1::new(12)),
                end_source: Default::default(),
                fringe: Default::default(),
            }],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let result = super::builtin_window_end(&mut ev, vec![]).expect("window-end");
    assert_eq!(
        result,
        Value::fixnum(8),
        "the atomically published GNU window-end tuple is the sole authority"
    );
}

#[test]
fn window_end_record_at_eob_never_exceeds_point_max() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    ev.edit
        .buffers
        .get_mut(buf)
        .expect("scratch buffer")
        .insert("hello");
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;
    let point_max = ev
        .edit
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_char_pos()
        .get()
        .saturating_add(1);

    let buffer_z_byte = ev
        .edit
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_emacs_byte_pos();
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .find_window_mut(wid)
        .expect("selected window")
        .set_window_end_from_positions(
            LispCharPos1::from_one_based_usize(point_max),
            buffer_z_byte,
            LispCharPos1::from_one_based_usize(point_max),
            buffer_z_byte,
            0,
        );

    let result = super::builtin_window_end(&mut ev, vec![]).expect("window-end");
    assert_eq!(result, Value::fixnum(point_max as i64));
}

#[test]
fn window_end_update_in_batch_returns_stored_end_without_estimate() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_variable("noninteractive", Value::T);
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    ev.edit
        .buffers
        .get_mut(buf)
        .expect("scratch buffer")
        .insert(&vec!["line of content"; 50].join("\n"));
    let fid = ev.frame_manager_mut().create_frame("F1", 80, 25, buf);
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;
    let point_max = ev
        .edit
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_char_pos()
        .get();

    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        if let Some(crate::window::Window::Leaf { window_start, .. }) = frame.find_window_mut(wid) {
            *window_start = LispCharPos1::from_one_based_usize(50);
        } else {
            panic!("selected window should be a leaf");
        }
    }

    let result =
        super::builtin_window_end(&mut ev, vec![Value::NIL, Value::T]).expect("window-end");
    assert_eq!(result, Value::fixnum(point_max as i64 + 1));
}

#[test]
fn window_end_update_returns_the_query_record_without_publishing_it() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_variable("noninteractive", Value::NIL);
    let buf = ev.edit.buffers.create_buffer("*window-end-layout-query*");
    ev.edit.buffers.set_current(buf);
    ev.edit
        .buffers
        .get_mut(buf)
        .expect("window-end buffer")
        .insert("//! Unicode — repro\nsecond line\nthird line\n");
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;
    ev.frame_manager_mut().get_mut(fid).expect("frame").initial = false;
    let buffer_z_char = LispCharPos1::from_one_based_usize(
        ev.edit
            .buffers
            .get(buf)
            .expect("window-end buffer")
            .point_max_char_pos()
            .get()
            .saturating_add(1),
    );
    let buffer_z_byte = ev
        .edit
        .buffers
        .get(buf)
        .expect("window-end buffer")
        .point_max_emacs_byte_pos();
    ev.frame_manager_mut()
        .get_mut(fid)
        .expect("frame")
        .find_window_mut(wid)
        .expect("selected window")
        .set_window_end_from_positions(
            buffer_z_char,
            buffer_z_byte,
            LispCharPos1::new(5),
            EmacsBytePos::new(4),
            0,
        );

    let calls = Rc::new(Cell::new(0));
    let observed_calls = Rc::clone(&calls);
    ev.install_window_layout_query(move |_eval, frame_id, window_id, _scope| {
        assert_eq!(frame_id, fid);
        assert_eq!(window_id, wid);
        observed_calls.set(observed_calls.get() + 1);
        crate::window::WindowLayoutQueryOutcome::Ready(crate::window::WindowLayoutQuery::new(
            LispCharPos1::new(17),
            None,
        ))
    })
    .expect("install the layout-query handler");

    let result =
        super::builtin_window_end(&mut ev, vec![Value::NIL, Value::T]).expect("window-end");

    assert_eq!(
        result,
        Value::fixnum(17),
        "window-end UPDATE must return the stack-local query record"
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(
        super::builtin_window_end(&mut ev, vec![Value::NIL, Value::NIL])
            .expect("stored window-end"),
        Value::fixnum(5),
        "a synchronous query must not overwrite retained redisplay state"
    );
}

#[test]
fn window_end_update_signals_when_installed_layout_query_does_not_converge() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_variable("noninteractive", Value::NIL);
    let buf = ev.edit.buffers.create_buffer("*window-end-layout-failure*");
    ev.edit.buffers.set_current(buf);
    ev.edit
        .buffers
        .get_mut(buf)
        .expect("buffer")
        .insert("one\ntwo\n");
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    ev.frame_manager_mut().get_mut(fid).expect("frame").initial = false;
    ev.install_window_layout_query(|_eval, _frame_id, _window_id, _scope| {
        crate::window::WindowLayoutQueryOutcome::Failed(
            crate::window::WindowLayoutQueryFailure::DidNotConverge,
        )
    })
    .expect("install the layout-query handler");

    let error = super::builtin_window_end(&mut ev, vec![Value::NIL, Value::T])
        .expect_err("an installed adapter failure must not return stale retained state");
    let error = error.into_kind();
    let neovm_value::error::FlowKind::Signal(signal) = error else {
        panic!("expected a Lisp error signal, got {error:?}")
    };
    assert_eq!(signal.symbol_name(), "error");
    assert_eq!(
        signal
            .data
            .first()
            .copied()
            .and_then(Value::as_str_owned)
            .as_deref(),
        Some("Window layout query did not converge")
    );
}

#[test]
fn window_chrome_height_queries_prefer_last_redisplay_snapshot_when_available() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;

    // Recorded heights measure requested chrome; they do not create chrome
    // whose buffer format is nil (GNU WINDOW_*_LINE_HEIGHT predicates).
    let buffer = ev.edit.buffers.get_mut(buf).expect("buffer");
    buffer.set_buffer_local("header-line-format", Value::string("HEADER"));
    buffer.set_buffer_local("tab-line-format", Value::string("TAB"));

    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            point_rows: None,
            window_id: wid,
            mode_line_height: 35,
            header_line_height: 35,
            tab_line_height: 34,
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    assert_eq!(
        super::builtin_window_mode_line_height(&mut ev, vec![]).expect("mode-line height"),
        Value::fixnum(35)
    );
    assert_eq!(
        super::builtin_window_header_line_height(&mut ev, &vec![]).expect("header-line height"),
        Value::fixnum(35)
    );
    assert_eq!(
        super::builtin_window_tab_line_height(&mut ev, &vec![]).expect("tab-line height"),
        Value::fixnum(34)
    );
}

#[test]
fn window_body_pixel_edges_begin_below_rendered_header_and_tab_lines() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;

    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(1),
                vec![crate::window::WindowDisplaySnapshot {
                    point_rows: None,
                    window_id: wid,
                    regions: crate::window::PresentedWindowRegions {
                        outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
                        text_body: neomacs_display_protocol::types::Rect::new(
                            0.0, 22.0, 800.0, 562.0,
                        ),
                        tab_line: Some(neomacs_display_protocol::types::Rect::new(
                            0.0, 0.0, 800.0, 17.0,
                        )),
                        header_line: Some(neomacs_display_protocol::types::Rect::new(
                            0.0, 17.0, 800.0, 5.0,
                        )),
                        mode_line: Some(neomacs_display_protocol::types::Rect::new(
                            0.0, 584.0, 800.0, 16.0,
                        )),
                        ..Default::default()
                    },
                    regions_materialized: true,
                    header_line_height: 999,
                    tab_line_height: 999,
                    points: vec![crate::window::DisplayPointSnapshot {
                        role: crate::window::DisplayPointRole::Glyph,
                        buffer_pos: neovm_edit::buffer::LispCharPos1::new(1),
                        x: 54,
                        y: 999,
                        width: 7,
                        height: 17,
                        row: 17,
                        col: 0,
                    }],
                    body_rows: vec![crate::window::PresentedBodyRowSnapshot {
                        output_row: 17,
                        body_row: 17,
                        body_y: 291,
                    }],
                    ..crate::window::WindowDisplaySnapshot::default()
                }],
            )
            .expect("presented geometry");
    }

    // GNU's `window-edges' (lisp/window.el:3839) computes a body's TOP as
    //   (window-pixel-top W) + (frame-internal-border-width FRAME)
    //   + (window-header-line-height W) + (window-tab-line-height W)
    // and it is Lisp there and here -- DIVERGENCES.md 154 deleted the Rust
    // subr.  Ask the four C primitives the Lisp reads: the RENDERED tab line
    // (17) and header line (5) must beat the stale 999s in the snapshot's
    // scalar fields, which is what this test is about.
    let top_body = ev
        .eval_str(
            "(+ (window-pixel-top) (frame-internal-border-width)
                (window-header-line-height) (window-tab-line-height))",
        )
        .expect("body top from the C primitives `window-edges' reads");
    assert_eq!(top_body, Value::fixnum(22));

    let posn = crate::emacs_core::xdisp::builtin_posn_at_point(
        &mut ev,
        vec![Value::fixnum(1), Value::make_window(wid.0)],
    )
    .expect("posn-at-point");
    let posn = neovm_value::value::list_to_vec(&posn).expect("position list");
    assert!(posn[2].is_cons(), "position coordinates");
    assert!(posn[9].is_cons(), "position geometry");
    let glyph_y = posn[2].cons_cdr().as_fixnum().expect("glyph y");
    let glyph_height = posn[9].cons_cdr().as_fixnum().expect("glyph height");

    assert_eq!(
        top_body.as_fixnum().expect("body top") + glyph_y + glyph_height,
        330
    );
}

#[test]
fn gui_geometry_queries_distinguish_logical_layout_from_presented_output() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buffer = ev.edit.buffers.create_buffer("*geometry-invariant*");
    ev.edit.buffers.set_current(buffer);
    let frame_id = ev.frame_manager_mut().create_frame("gui", 800, 600, buffer);
    let window_id = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let frame = ev.frame_manager_mut().get_mut(frame_id).expect("frame");
    frame.set_window_system(Some(Value::symbol("neo")));
    frame
        .find_window_mut(window_id)
        .expect("window")
        .set_bounds(crate::window::Rect::new(901.0, 902.0, 903.0, 904.0));

    assert_eq!(
        super::builtin_window_pixel_width(&mut ev, &vec![])
            .expect("GNU exposes the synchronous logical pixel width")
            .as_int(),
        Some(903),
        "window-pixel-width reads the window's current layout before first presentation"
    );
    assert_eq!(
        super::builtin_window_pixel_height(&mut ev, &vec![])
            .expect("GNU exposes the synchronous logical pixel height")
            .as_int(),
        Some(904),
        "window-pixel-height reads the window's current layout before first presentation"
    );
    assert_eq!(
        super::builtin_window_pixel_left(&mut ev, &vec![])
            .expect("GNU exposes the synchronous logical pixel origin")
            .as_int(),
        Some(901)
    );
    assert_eq!(
        super::builtin_window_pixel_top(&mut ev, &vec![])
            .expect("GNU exposes the synchronous logical pixel origin")
            .as_int(),
        Some(902)
    );
    assert_eq!(
        super::builtin_window_body_height(&mut ev, &vec![Value::NIL, Value::T])
            .expect("logical body geometry exists before first presentation")
            .as_int(),
        Some(888),
        "pre-presentation body height derives from synchronous layout and chrome state"
    );
}

#[test]
fn graphical_logical_window_origin_is_available_before_first_presentation() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_with_gui_frame("(list (window-left-column) (window-top-line))"),
        ["OK (0 0)"],
        "GNU logical window edges exist before redisplay publishes pixel geometry"
    );
}

#[test]
fn presented_fringe_geometry_and_scrollbar_tuple_keep_live_gnu_configuration() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buffer = ev.edit.buffers.create_buffer("*geometry-tuples*");
    ev.edit.buffers.set_current(buffer);
    let frame_id = ev.frame_manager_mut().create_frame("gui", 800, 600, buffer);
    let window_id = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = ev.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    assert!(
        ev.frame_manager_mut()
            .set_window_fringes(window_id, Some(8), Some(0), true, true)
    );
    assert!(ev.frame_manager_mut().set_window_scroll_bars(
        window_id,
        Some(0),
        Value::symbol("left"),
        Some(0),
        Value::symbol("bottom"),
        true,
    ));
    // Exercise a valid published zero-width realization while retaining the
    // live Lisp configuration fields.  The geometry tuple must not infer
    // these non-geometric values from absence of a positive-area rectangle.
    {
        let display = ev
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .find_window_mut(window_id)
            .expect("window")
            .display_mut()
            .expect("leaf display state");
        display.vertical_scroll_bar_type = Value::symbol("left");
        display.horizontal_scroll_bar_type = Value::symbol("bottom");
    }
    ev.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .prepare_and_activate_display_presentation_for_test(
            crate::window::geometry::PresentationId::new(1),
            vec![crate::window::WindowDisplaySnapshot {
                point_rows: None,
                window_id,
                regions: crate::window::PresentedWindowRegions {
                    outer: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
                    text_body: neomacs_display_protocol::types::Rect::new(8.0, 0.0, 792.0, 584.0),
                    left_fringe: Some(neomacs_display_protocol::types::Rect::new(
                        0.0, 0.0, 8.0, 584.0,
                    )),
                    left_scroll_bar: Some(neomacs_display_protocol::types::Rect::new(
                        0.0, 0.0, 13.0, 577.0,
                    )),
                    horizontal_scroll_bar: Some(neomacs_display_protocol::types::Rect::new(
                        0.0, 577.0, 800.0, 7.0,
                    )),
                    ..Default::default()
                },
                regions_materialized: true,
                ..Default::default()
            }],
        )
        .expect("presented geometry");

    let fringes = super::builtin_window_fringes(&mut ev, &vec![]).expect("fringes");
    let scrollbars = super::builtin_window_scroll_bars(&mut ev, &vec![]).expect("scrollbars");
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&fringes),
        "(8 0 t t)"
    );
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&scrollbars),
        "(0 0 left 0 0 bottom t)"
    );

    {
        let display = ev
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .find_window_mut(window_id)
            .expect("window")
            .display_mut()
            .expect("leaf display state");
        display.scroll_bar_width = -1;
        display.vertical_scroll_bar_type = Value::T;
        display.scroll_bar_height = -1;
        display.horizontal_scroll_bar_type = Value::T;
        display.scroll_bars_persistent = false;
    }
    let inherited = super::builtin_window_scroll_bars(&mut ev, &vec![]).expect("scrollbars");
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&inherited),
        // Inherited COLUMNS = FRAME_CONFIG_SCROLL_BAR_COLS = ceil(16 / 8) = 2 (src/xfns.c:2393-2407,
        // src/window.h:934-939); the 13 px presented band must not leak into it.
        "(nil 2 t nil 0 t nil)",
        "realized rectangles must not replace GNU's raw inherited tuple fields"
    );
}

/// `recenter` walks backward `target_line` screen lines from point. Exercises
/// the multi-line backward walk and the begv clamp -- the paths the 0-line /
/// 1-line cases above miss.

/// `recenter` counts SCREEN lines, so invisible text does not consume one.
///
/// GNU's positive-ARG branch runs the display iterator --
/// `start_display`, `move_it_by_lines (&it, 0)` to the head of the screen line
/// holding point, then `move_it_by_lines (&it, -nlines)` (src/window.c:7395-7407)
/// -- the same machinery `vertical-motion` uses, which steps over invisible
/// text without counting it. Verified under GNU in a PTY: with line 10 of a
/// 23-line buffer hidden by an `invisible` overlay and point on line 20,
/// `(recenter 12)` puts window-start on line 7, exactly where
/// `(vertical-motion -12)` lands; a buffer with nothing hidden puts it on
/// line 8. Neomacs answered line 8 in BOTH cases, because `recenter` walked
/// raw buffer newlines instead of screen lines.
///
/// `helm-css-scss--recenter` is `(recenter (/ (window-height) 2))`, which is
/// how this reached the terminal parity suite.

/// Multiple wheel events can dispatch their `scroll-up` commands before the
/// asynchronous GUI redisplay publishes a fresh window-end.  GNU keeps using
/// the explicit window-start in that interval, so same-direction commands
/// must advance monotonically rather than recentering around point.

/// Reproduces the interactive `-nw` bug: `M-x view-hello-file`, `C-v` to the
/// end of the buffer, then `M-v` appears to do nothing. Root cause: after
/// scroll-down recomputes window-start, GNU pulls point UP into the window
/// when it ended up BELOW the last visible line (window.c
/// `window_scroll_line_based`: opoint at/after the bottom is moved to the
/// start of the last visible line); point left outside the window makes the
/// next redisplay recenter around eob, snapping the view back.
///
/// GNU Emacs 31.0.90 ground truth (batch, body-height pinned to match this
/// harness), 200 x "line %03d\n" buffer, point at point-max:
///   scroll-down => start = point-of-last-window-full, point = start of the
///   LAST fully-visible window line (exact line start, no +1).

/// A near-full-screen scroll is measured in displayed screen rows, not raw
/// buffer newlines.  This matters for folded Org subtrees: invisible logical
/// lines occupy no part of the window and therefore cannot consume a page.

/// Page-up near the end of a large buffer must not restart display scanning at
/// `point-min` for every requested screen row.  That made one `C-b` in a
/// 149-KiB Org journal execute roughly `page_height * buffer_size` display
/// iterator steps and block the command loop for about 25 seconds.
#[test]
fn scroll_down_page_scans_buffer_once_not_once_per_screen_row() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    let buffer_id = ev.edit.buffers.create_buffer("*scroll-complexity*");
    ev.edit.buffers.set_current(buffer_id);
    let frame_id = ev
        .frame_manager_mut()
        .create_frame("scroll-complexity", 800, 600, buffer_id);
    ev.edit
        .buffers
        .insert_into_buffer(buffer_id, &"some journal text on this line\n".repeat(400));
    let end = ev
        .edit
        .buffers
        .get(buffer_id)
        .expect("scroll buffer")
        .emacs_byte_pos_to_lisp_char_pos(
            ev.edit
                .buffers
                .get(buffer_id)
                .expect("scroll buffer")
                .accessible_emacs_byte_region()
                .end(),
        );
    let window_id = ev
        .frame_manager()
        .get(frame_id)
        .expect("scroll frame")
        .selected_window;
    let (wm_frames, wm_buffers) = ev.frame_manager_and_buffers_mut();
    let window = wm_frames
        .get_mut(frame_id)
        .expect("scroll frame")
        .find_window_mut(window_id)
        .expect("scroll window");
    crate::window::window_markers::set_window_point_with_marker(wm_buffers, window, end);
    crate::window::window_markers::set_window_start_with_marker(wm_buffers, window, end);

    neovm_edit::test_support::authority::reset_screen_line_scan_steps_for_test();
    neovm_edit::test_support::authority::reset_display_stop_recomputes_for_test();
    super::builtin_scroll_down(&mut ev, vec![Value::NIL]).expect("page up from end");
    let after_start = super::builtin_window_start(&mut ev, &vec![Value::make_window(window_id.0)])
        .expect("window-start after page up")
        .as_fixnum()
        .expect("window-start is an integer");
    let scan_steps = neovm_edit::emacs_core::indent::screen_line_scan_steps_for_test();
    let buffer_bytes = ev
        .edit
        .buffers
        .get(buffer_id)
        .expect("scroll buffer")
        .accessible_emacs_byte_region()
        .range()
        .len()
        .get();

    assert!(
        after_start < end.as_i64(),
        "page-up must move window-start backward: before={} after={after_start}",
        end.as_i64()
    );
    assert!(
        scan_steps < buffer_bytes.saturating_mul(2),
        "page-up rescanned the buffer per screen row: {scan_steps} display steps for \
         {buffer_bytes} buffer bytes"
    );
    // The DisplayStopCache must coalesce the invisible/display/composition
    // probes: over plain text they run once per stop (~one per screen line here,
    // since each line is far shorter than DISPLAY_STOP_CHAR_CAP), not once per
    // scanned character. Guard against regressing to per-char probing.
    let recomputes = neovm_edit::emacs_core::indent::display_stop_recomputes_for_test();
    assert!(
        recomputes.saturating_mul(4) < scan_steps,
        "display-stop cache did not coalesce property probes: {recomputes} stop recomputes \
         for {scan_steps} per-char scan steps (expected far fewer recomputes)"
    );
}

// ---------------------------------------------------------------------------
// `window-combination-limit` / side-window placement parity with GNU.
//
// GNU `Fsplit_window_internal` (src/window.c:5423-5431) decides whether a
// split wraps the target in a fresh parent from THREE inputs:
//
//     combination_limit = (Vwindow_combination_limit == Qt)   // dynamic var
//                        || NILP (o->parent)                  // splitting root
//                        || parent is ortho-combined;
//
// The per-window stored slot `w->combination_limit` is NOT consulted here; it
// only guards `recombine_windows` on the delete path (src/window.c:2616).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// `delete-window` space reclamation.
//
// `lisp/window.el`'s `delete-window` picks ONE sibling to absorb the deleted
// window's space -- `(or (window-left window) (window-right window))`, i.e. the
// previous sibling when there is one, else the next -- and stages the new size
// with `window--resize-this-window`.  GNU's `Fdelete_window_internal` then
// commits the staged `new_pixel` values with `window_resize_apply`.
//
// neomacs stages the same values (verified identical to GNU via an advice
// probe on `delete-window-internal`), so the primitive must apply them rather
// than invent a layout of its own.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// window-lines-pixel-dimensions
//
// Expected values are GNU 31.1's, recorded from the real binary on a 9x20 cell
// (`tmp/textsize2/gnu-wlpd.txt`).  The fixture below uses a 10x20 cell, so a
// pure-text width is GNU's scaled by 10/9.
// ---------------------------------------------------------------------------

/// A window holding `ab\n`, `abcd\n`, `abcdef\n` plus the empty row the final
/// newline opens, and the mode line below it -- the buffer GNU's `basic` run
/// measured.
///
/// Returns the evaluator, the window id, and the window's body width in pixels
/// (GNU's `window_width` under BODY, and the width an INVERSE answer measures
/// back from).
fn window_lines_pixel_dimensions_fixture() -> (Context, crate::window::WindowId, i64) {
    // `noninteractive' is what GNU's first bail tests, so the fixture has to
    // look like an interactive session for the matrix to be readable at all.
    let mut ev = Context::new(crate::test_utils::unit_runtime());
    ev.set_variable("noninteractive", Value::NIL);
    let buf = ev.edit.buffers.create_buffer("*wlpd*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("wlpd", 745, 422, buf);
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.char_width = 10.0;
        frame.char_height = 20.0;
        frame.font_pixel_size = 20.0;
        frame.font_ascent = 15.0;
        frame.set_window_system(Some(Value::symbol("x")));
        frame
            .root_window_mut()
            .set_bounds(crate::window::Rect::new(0.0, 0.0, 800.0, 442.0));
        // The rows the walk would have produced: `ab`, `abcd`, `abcdef`, the
        // empty last line, and the mode line at the window's foot.
        let row = |row: i64, y: i64, used: i64| crate::window::DisplayRowSnapshot {
            row,
            y,
            height: 20,
            start_x: 0,
            end_x: used,
            ..crate::window::DisplayRowSnapshot::default()
        };
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
            point_rows: None,
            window_id: wid,
            mode_line_height: 20,
            rows: vec![
                row(0, 0, 20),
                row(1, 20, 40),
                row(2, 40, 60),
                row(3, 60, 0),
                crate::window::DisplayRowSnapshot {
                    row: 21,
                    y: 422,
                    height: 20,
                    ..crate::window::DisplayRowSnapshot::default()
                },
            ],
            points: vec![
                crate::window::DisplayPointSnapshot {
                    buffer_pos: LispCharPos1::new(1),
                    role: crate::window::DisplayPointRole::Glyph,
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 20,
                    row: 0,
                    col: 0,
                },
                crate::window::DisplayPointSnapshot {
                    buffer_pos: LispCharPos1::new(2),
                    role: crate::window::DisplayPointRole::Glyph,
                    x: 10,
                    y: 0,
                    width: 10,
                    height: 20,
                    row: 0,
                    col: 1,
                },
            ],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }
    let window = ev
        .frame_manager()
        .get(fid)
        .expect("frame")
        .find_window(wid)
        .cloned();
    let body = window_lines_pixel_dimensions_body_width(&ev, fid, window.as_ref().expect("window"));
    (ev, wid, body)
}

fn window_lines_pixel_dimensions_body_width(
    ev: &Context,
    fid: crate::window::FrameId,
    window: &crate::window::Window,
) -> i64 {
    super::window_body_width_pixels(&ev.frame_manager(), fid, window)
}

fn wlpd(eval: &mut Context, args: Vec<Value>) -> Value {
    super::builtin_window_lines_pixel_dimensions(eval, args).expect("window-lines-pixel-dimensions")
}

#[test]
fn window_lines_pixel_dimensions_reports_each_rows_width_and_bottom_edge() {
    crate::test_utils::init_test_tracing();
    let (mut ev, wid, _body) = window_lines_pixel_dimensions_fixture();
    let value = wlpd(&mut ev, vec![Value::make_window(wid.0)]);
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&value),
        // GNU: ((27 . 20) (45 . 40) (63 . 60) (9 . 80)) at a 9-pixel cell.
        "((30 . 20) (50 . 40) (70 . 60) (10 . 80))",
        "row width is the row's right edge; y is its bottom"
    );

    // BODY moves the two vertical boundaries, not the rows here (this window
    // has no tab or header line).
    let body = wlpd(
        &mut ev,
        vec![Value::make_window(wid.0), Value::NIL, Value::NIL, Value::T],
    );
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&body),
        "((30 . 20) (50 . 40) (70 . 60) (10 . 80))",
        "GNU's body answer for this window"
    );

    // FIRST and LAST are matrix indices, not text-row indices.
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&wlpd(
            &mut ev,
            vec![Value::make_window(wid.0), Value::fixnum(1)]
        )),
        "((50 . 40) (70 . 60) (10 . 80))"
    );
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&wlpd(
            &mut ev,
            vec![
                Value::make_window(wid.0),
                Value::fixnum(0),
                Value::fixnum(1)
            ]
        )),
        "((30 . 20) (50 . 40))"
    );
}

#[test]
fn window_lines_pixel_dimensions_inverts_against_the_window_edge() {
    crate::test_utils::init_test_tracing();
    let (mut ev, wid, body) = window_lines_pixel_dimensions_fixture();
    // The window is 800 px wide; GNU's INVERSE measures from the row's right
    // edge back to it (`window_width - row->pixel_width`).
    let value = wlpd(
        &mut ev,
        vec![
            Value::make_window(wid.0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
        ],
    );
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&value),
        "((770 . 20) (750 . 40) (730 . 60) (790 . 80))"
    );
    // ... and back from the BODY edge when BODY is non-nil.
    let value = wlpd(
        &mut ev,
        vec![
            Value::make_window(wid.0),
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::T,
        ],
    );
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&value),
        format!(
            "(({} . 20) ({} . 40) ({} . 60) ({} . 80))",
            body - 30,
            body - 50,
            body - 70,
            body - 10
        ),
        "INVERSE under BODY measures from `window_body_width (w, PIXELS)`"
    );
}

#[test]
fn window_lines_pixel_dimensions_left_reports_the_leftmost_glyph() {
    crate::test_utils::init_test_tracing();
    let (mut ev, wid, _body) = window_lines_pixel_dimensions_fixture();
    // LEFT reads the row's FIRST glyph, not the row: `window_width -
    // glyph->pixel_width`.  The fixture's first two rows publish a 10-pixel
    // span each; the rows with no span (the empty line) fall back to one cell.
    let value = wlpd(
        &mut ev,
        vec![
            Value::make_window(wid.0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
        ],
    );
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&value),
        "((790 . 20) (790 . 40) (790 . 60) (790 . 80))"
    );
    let value = wlpd(
        &mut ev,
        vec![
            Value::make_window(wid.0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::T,
            Value::T,
        ],
    );
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&value),
        "((10 . 20) (10 . 40) (10 . 60) (10 . 80))"
    );
}

#[test]
fn window_lines_pixel_dimensions_stops_at_the_window_edge_and_the_mode_line() {
    crate::test_utils::init_test_tracing();
    let (mut ev, wid, body) = window_lines_pixel_dimensions_fixture();
    // A row that reaches the body's right edge has no room for the cursor
    // cell GNU appends at end of line, so it reports exactly what it drew.
    {
        let fid = ev.frame_manager().find_window_frame_id(wid).expect("frame");
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        let mut snapshot = frame.redisplay_snapshot(wid).expect("snapshot").clone();
        snapshot.rows[0].end_x = body;
        frame.replace_redisplay_cache_for_test(vec![snapshot]);
    }
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&wlpd(&mut ev, vec![Value::make_window(wid.0)])),
        format!("(({} . 20) (50 . 40) (70 . 60) (10 . 80))", body),
        "a full row is measured as it was drawn"
    );
}

#[test]
fn window_lines_pixel_dimensions_refuses_what_gnu_refuses() {
    crate::test_utils::init_test_tracing();
    let (mut ev, wid, _body) = window_lines_pixel_dimensions_fixture();
    // GNU `check_integer_range (first, 0, matrix->nrows)`.
    let err = super::builtin_window_lines_pixel_dimensions(
        &mut ev,
        vec![Value::make_window(wid.0), Value::fixnum(100)],
    )
    .expect_err("an out-of-range FIRST signals");
    match err.into_kind() {
        // GNU `check_integer_range (first, 0, matrix->nrows)`: the upper bound
        // is the matrix's own row count.
        neovm_value::error::FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "args-out-of-range");
            assert_eq!(
                sig.data,
                vec![Value::fixnum(100), Value::fixnum(0), Value::fixnum(5)]
            );
        }
        other => panic!("expected args-out-of-range, got {other:?}"),
    }

    // A dead window is `window-live-p`, whatever the other arguments say.
    let fid = ev.frame_manager().find_window_frame_id(wid).expect("frame");
    let buffer = ev.edit.buffers.current_buffer().expect("buffer").id();
    let dead = ev
        .frame_manager_mut()
        .split_window(
            fid,
            wid,
            crate::window::SplitDirection::Vertical,
            buffer,
            None,
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("split the fixture window");
    assert!(ev.frame_manager_mut().delete_window(fid, dead));
    let err = super::builtin_window_lines_pixel_dimensions(
        &mut ev,
        vec![Value::make_window(dead.0), Value::fixnum(0)],
    )
    .expect_err("a dead window signals");
    match err.into_kind() {
        neovm_value::error::FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data.first(), Some(&Value::symbol("window-live-p")));
        }
        other => panic!("expected wrong-type-argument, got {other:?}"),
    }
}

/// GNU returns nil when there is no current matrix to read: a pseudo window,
/// batch, or a redisplay that has not run since the last change.
#[test]
fn window_lines_pixel_dimensions_returns_nil_without_a_current_matrix() {
    crate::test_utils::init_test_tracing();
    let (mut ev, wid, _body) = window_lines_pixel_dimensions_fixture();
    // Batch -- GNU's `if (noninteractive || w->pseudo_window_p) return Qnil;`
    // comes before the matrix is even looked at.
    ev.set_variable("noninteractive", Value::T);
    assert!(wlpd(&mut ev, vec![Value::make_window(wid.0)]).is_nil());
    ev.set_variable("noninteractive", Value::NIL);

    // A window whose rows were never committed.
    let mut fresh = Context::new(crate::test_utils::unit_runtime());
    let buf = fresh.edit.buffers.create_buffer("*empty*");
    fresh.edit.buffers.set_current(buf);
    let fid = fresh
        .frame_manager_mut()
        .create_frame("wlpd-empty", 800, 442, buf);
    let wid = fresh
        .frame_manager()
        .get(fid)
        .expect("frame")
        .selected_window;
    assert!(wlpd(&mut fresh, vec![Value::make_window(wid.0)]).is_nil());
}

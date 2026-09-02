use crate::buffer::{EmacsBytePos, LispCharPos1};
use crate::emacs_core::eval::{FontPxProbeResult, GuiFrameHostSize, ResolvedFrameFont};
use crate::emacs_core::window_cmds::SplitWindowSide;
use crate::emacs_core::{Context, DisplayHost, GuiFrameHostRequest, Value, format_eval_result};
use crate::face::{FontSlant, FontWeight, FontWidth};
use crate::heap_types::LispString;
use crate::test_utils::{runtime_startup_context, runtime_startup_eval_all};
use crate::window::{FrameFullscreen, FrameParam};
use std::cell::Cell;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn frame_scale_factor_reads_the_selected_frames_presented_device_scale() {
    let mut eval = Context::new();
    let frame_id = super::ensure_selected_frame_id(&mut eval);
    eval.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .device_scale_factor = 1.75;

    let value = crate::emacs_core::frame::builtin_frame_scale_factor(&mut eval, vec![])
        .expect("scale factor");

    assert_eq!(value, Value::make_float(1.75));
}

/// Evaluate all forms with a fresh evaluator that has a frame+window set up.
fn eval_with_frame(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    // Create a buffer for the initial window.
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    // Create a frame so window/frame builtins have something to work with.
    ev.frames.create_frame("F1", 800, 600, buf);
    // Tests that exercise `make-frame` need a usable terminal (production
    // --batch deliberately has none, so frame creation errors like GNU).
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    ev.eval_str_each(src)
        .iter()
        .map(format_eval_result)
        .collect()
}

fn eval_one_with_frame(src: &str) -> String {
    eval_with_frame(src).into_iter().next().unwrap()
}

#[test]
fn window_tree_primitives_validate_arguments_like_gnu() {
    crate::test_utils::init_test_tracing();
    let out = eval_one_with_frame(
        r#"(list
             (condition-case err
                 (combine-windows "not-a-window" nil)
               (error err))
             (condition-case err
                 (uncombine-window "not-a-window")
               (error err))
             (condition-case err
                 (window-discard-buffer-from-window (current-buffer) "not-a-window")
               (error err)))"#,
    );
    assert_eq!(
        out,
        r#"OK ((wrong-type-argument window-valid-p "not-a-window") (wrong-type-argument window-valid-p "not-a-window") (error "Not a live window"))"#
    );
}

fn eval_with_gui_frame(src: &str) -> Vec<String> {
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    ev.eval_str_each(src)
        .iter()
        .map(format_eval_result)
        .collect()
}

fn publish_selected_gui_window_regions(
    ev: &mut Context,
    fid: crate::window::FrameId,
    presentation: u64,
    regions: crate::window::PresentedWindowRegions,
) {
    let window_id = ev.frames.get(fid).expect("frame").selected_window;
    ev.frames
        .get_mut(fid)
        .expect("frame")
        .prepare_and_activate_display_presentation_for_test(
            crate::window::geometry::PresentationId::new(presentation),
            vec![crate::window::WindowDisplaySnapshot {
                window_id,
                regions,
                regions_materialized: true,
                ..Default::default()
            }],
        )
        .expect("presented GUI geometry");
}

fn bootstrap_eval_with_frame(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
}

/// Evaluate all forms in a FULLY BOOTED runtime whose terminal is marked
/// usable.
///
/// This is where a test goes when it needs one of the seventeen names
/// DIVERGENCES.md 154 deleted -- `delete-window', `delete-other-windows',
/// `make-frame', `switch-to-buffer', `select-frame-set-input-focus' and the
/// rest.  GNU has no C version of any of them: they are `window.el' and
/// `frame.el' `defun's, so they exist only after `loadup.el', and the bare
/// evaluator above is GNU BEFORE `loadup.el', where they are void in GNU too.
///
/// The terminal is marked usable because GNU's `make-frame' needs one --
/// production `--batch' deliberately has none and GNU answers
/// `(error "Unknown terminal type")' -- and the bare helper these tests came
/// from marked it for exactly that reason.
fn runtime_eval_with_usable_terminal(src: &str) -> Vec<String> {
    let mut ev = runtime_startup_context();
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    ev.eval_str_each(src)
        .iter()
        .map(format_eval_result)
        .collect()
}

fn runtime_eval_one_with_usable_terminal(src: &str) -> String {
    runtime_eval_with_usable_terminal(src)
        .into_iter()
        .next()
        .expect("result")
}

fn bootstrap_eval_one_with_frame(src: &str) -> String {
    bootstrap_eval_with_frame(src)
        .into_iter()
        .next()
        .expect("result")
}

#[test]
fn active_minibuffer_window_tracks_live_minibuffer_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.frames.create_frame("F1", 800, 600, buf);

    let fid = super::ensure_selected_frame_id_in_state(&mut ev.frames, &mut ev.buffers);
    let minibuffer_buffer_id = {
        let frame = ev.frames.get(fid).expect("selected frame");
        let minibuffer_wid = frame.minibuffer_window.expect("minibuffer window");
        frame
            .find_window(minibuffer_wid)
            .and_then(|window| window.buffer_id())
            .expect("minibuffer buffer")
    };
    ev.minibuffers
        .read_from_minibuffer(minibuffer_buffer_id, "M-x ", None, None)
        .expect("active minibuffer state");

    let minibuffer_window = super::builtin_minibuffer_window(&mut ev, vec![]).unwrap();
    let active_minibuffer_window =
        super::builtin_active_minibuffer_window(&mut ev, vec![]).unwrap();
    assert_eq!(active_minibuffer_window, minibuffer_window);
    assert!(!active_minibuffer_window.is_nil());
}

#[derive(Clone, Default)]
struct RecordingDisplayHost {
    realized: Rc<RefCell<Vec<GuiFrameHostRequest>>>,
    resized: Rc<RefCell<Vec<GuiFrameHostRequest>>>,
    destroyed_gui_frames: Rc<RefCell<Vec<crate::window::FrameId>>>,
    shown_child_frames: Rc<RefCell<Vec<crate::window::FrameId>>>,
    removed_child_frames: Rc<RefCell<Vec<crate::window::FrameId>>>,
    fullscreen_changes: Rc<RefCell<Vec<(crate::window::FrameId, FrameFullscreen)>>>,
    visibility_changes: Rc<RefCell<Vec<(crate::window::FrameId, crate::window::FrameVisibility)>>>,
    geometry_hints:
        Rc<RefCell<Vec<(crate::window::FrameId, crate::window::GuiFrameGeometryHints)>>>,
    primary_size: Option<GuiFrameHostSize>,
    resolved_frame_font: Option<ResolvedFrameFont>,
}

impl RecordingDisplayHost {
    fn new() -> Self {
        Self::default()
    }

    fn with_primary_size(width: u32, height: u32) -> Self {
        Self {
            primary_size: Some(GuiFrameHostSize { width, height }),
            ..Self::default()
        }
    }

    fn with_resolved_frame_font(resolved_frame_font: ResolvedFrameFont) -> Self {
        Self {
            resolved_frame_font: Some(resolved_frame_font),
            ..Self::default()
        }
    }
}

fn resolved_frame_font(
    family: &str,
    postscript_name: &str,
    height_tenths: i32,
    metrics: FontPxProbeResult,
) -> ResolvedFrameFont {
    ResolvedFrameFont {
        font: crate::emacs_core::eval::test_resolved_opened_font(
            family,
            None,
            None,
            FontWeight::NORMAL,
            FontSlant::Normal,
            FontWidth::Normal,
            Some(postscript_name),
            metrics,
            None,
        ),
        height_tenths,
    }
}

fn remapped_mono_font_metrics() -> ResolvedFrameFont {
    resolved_frame_font(
        "Remapped Mono",
        "RemappedMono-Regular",
        240,
        FontPxProbeResult {
            pixel_size: 28,
            height: 32,
            ascent: 24,
            descent: 8,
            max_width: 16,
            space_width: 16,
            average_width: 16,
        },
    )
}

impl DisplayHost for RecordingDisplayHost {
    fn realize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        self.realized.borrow_mut().push(request);
        Ok(())
    }

    fn resize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        self.resized.borrow_mut().push(request);
        Ok(())
    }

    fn set_gui_frame_fullscreen(
        &mut self,
        frame_id: crate::window::FrameId,
        fullscreen: FrameFullscreen,
    ) -> Result<(), String> {
        self.fullscreen_changes
            .borrow_mut()
            .push((frame_id, fullscreen));
        Ok(())
    }

    fn set_gui_frame_visibility(
        &mut self,
        frame_id: crate::window::FrameId,
        visibility: crate::window::FrameVisibility,
    ) -> Result<(), String> {
        self.visibility_changes
            .borrow_mut()
            .push((frame_id, visibility));
        Ok(())
    }

    fn set_gui_frame_geometry_hints(
        &mut self,
        frame_id: crate::window::FrameId,
        geometry_hints: crate::window::GuiFrameGeometryHints,
    ) -> Result<(), String> {
        self.geometry_hints
            .borrow_mut()
            .push((frame_id, geometry_hints));
        Ok(())
    }

    fn current_primary_window_size(&self) -> Option<GuiFrameHostSize> {
        self.primary_size
    }

    fn opening_gui_frame_pending(&self) -> bool {
        self.realized.borrow().is_empty()
    }

    fn destroy_gui_frame(&mut self, frame_id: crate::window::FrameId) -> Result<(), String> {
        self.destroyed_gui_frames.borrow_mut().push(frame_id);
        Ok(())
    }

    fn show_gui_child_frame(&mut self, frame_id: crate::window::FrameId) -> Result<(), String> {
        self.shown_child_frames.borrow_mut().push(frame_id);
        Ok(())
    }

    fn remove_gui_child_frame(&mut self, frame_id: crate::window::FrameId) -> Result<(), String> {
        self.removed_child_frames.borrow_mut().push(frame_id);
        Ok(())
    }

    fn resolve_frame_font(
        &mut self,
        _frame_id: crate::window::FrameId,
        _request: crate::emacs_core::display_host::FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        Ok(self.resolved_frame_font.clone())
    }
}

fn due_gnu_timer(callback: Value, args: Value) -> Value {
    let when = SystemTime::now()
        .checked_sub(Duration::from_millis(10))
        .expect("timer deadline should fit in system time")
        .duration_since(UNIX_EPOCH)
        .expect("timer deadline should be after unix epoch");
    let secs = when.as_secs() as i64;

    Value::vector(vec![
        Value::NIL,
        Value::fixnum(secs >> 16),
        Value::fixnum(secs & 0xFFFF),
        Value::fixnum(when.subsec_micros() as i64),
        Value::NIL,
        callback,
        args,
        Value::NIL,
        Value::fixnum(0),
        Value::NIL,
    ])
}

// -- Window queries --

#[test]
fn bootstrap_window_command_boundary_matches_gnu_emacs() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(list (subrp (symbol-function 'select-window))
                 (subrp (symbol-function 'split-window-internal))
                 (subrp (symbol-function 'delete-window-internal))
                 (subrp (symbol-function 'delete-other-windows-internal))
                 (subrp (symbol-function 'other-window-for-scrolling))
                 (subrp (symbol-function 'display-buffer))
                 (subrp (symbol-function 'switch-to-buffer))
                 (subrp (symbol-function 'pop-to-buffer))
                 (subrp (symbol-function 'other-window))
                 (subrp (symbol-function 'delete-window))
                 (subrp (symbol-function 'delete-other-windows))
                 (subrp (symbol-function 'split-window))
                 (subrp (symbol-function 'split-window-below))
                 (subrp (symbol-function 'split-window-right)))"#,
    );
    assert_eq!(result, "OK (t t t t t nil nil nil nil nil nil nil nil nil)");
}

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
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
fn frame_root_window_window_valid_and_minibuffer_activity_semantics() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(window-valid-p (selected-window))
         (window-valid-p (minibuffer-window))
         (window-valid-p nil)
         (window-valid-p 999999)
         (window-valid-p 'foo)
         (eq (frame-root-window) (selected-window))
         (eq (frame-root-window (selected-frame)) (selected-window))
         (eq (frame-root-window (selected-window)) (selected-window))
         (eq (frame-root-window (minibuffer-window)) (selected-window))
         (eq (window-next-sibling (frame-root-window)) (minibuffer-window))
         (eq (window-prev-sibling (minibuffer-window)) (frame-root-window))
         (minibuffer-selected-window)
         (active-minibuffer-window)
         (minibuffer-window-active-p (minibuffer-window))
         (minibuffer-window-active-p (selected-window))
         (minibuffer-window-active-p nil)
         (minibuffer-window-active-p 999999)
         (minibuffer-window-active-p 'foo)
         (condition-case err (window-valid-p) (error err))
         (condition-case err (window-valid-p nil nil) (error err))
         (condition-case err (frame-root-window 999999) (error err))
         (condition-case err (frame-root-window 'foo) (error err))
         (condition-case err (frame-root-window nil nil) (error err))
         (condition-case err (minibuffer-selected-window nil) (error err))
         (condition-case err (active-minibuffer-window nil) (error err))
         (condition-case err (minibuffer-window-active-p) (error err))
         (condition-case err (minibuffer-window-active-p nil nil) (error err))",
    );
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK t");
    assert_eq!(out[2], "OK nil");
    assert_eq!(out[3], "OK nil");
    assert_eq!(out[4], "OK nil");
    assert_eq!(out[5], "OK t");
    assert_eq!(out[6], "OK t");
    assert_eq!(out[7], "OK t");
    assert_eq!(out[8], "OK t");
    assert_eq!(out[9], "OK t");
    assert_eq!(out[10], "OK t");
    assert_eq!(out[11], "OK nil");
    assert_eq!(out[12], "OK nil");
    assert_eq!(out[13], "OK nil");
    assert_eq!(out[14], "OK nil");
    assert_eq!(out[15], "OK nil");
    assert_eq!(out[16], "OK nil");
    assert_eq!(out[17], "OK nil");
    assert_eq!(out[18], "OK (wrong-number-of-arguments window-valid-p 0)");
    assert_eq!(out[19], "OK (wrong-number-of-arguments window-valid-p 2)");
    assert_eq!(out[20], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[21], "OK (wrong-type-argument frame-live-p foo)");
    assert_eq!(
        out[22],
        "OK (wrong-number-of-arguments frame-root-window 2)"
    );
    assert_eq!(
        out[23],
        "OK (wrong-number-of-arguments minibuffer-selected-window 1)"
    );
    assert_eq!(
        out[24],
        "OK (wrong-number-of-arguments active-minibuffer-window 1)"
    );
    // GNU `minibuffer-window-active-p` is a Lisp defun (window.el),
    // so its arity errors carry the (MIN . MAX) tuple, not the symbol.
    assert_eq!(out[25], "OK (wrong-number-of-arguments (1 . 1) 0)");
    assert_eq!(out[26], "OK (wrong-number-of-arguments (1 . 1) 2)");
}

#[test]
fn frame_root_window_p_semantics_and_errors() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(frame-root-window-p (selected-window))
         (frame-root-window-p (minibuffer-window))
         (condition-case err (frame-root-window-p 999999) (error err))
         (condition-case err (frame-root-window-p 'foo) (error err))
         (condition-case err (frame-root-window-p) (error (car err)))
         (condition-case err (frame-root-window-p nil nil) (error (car err)))",
    );
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK nil");
    assert_eq!(out[2], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[3], "OK (wrong-type-argument frame-live-p foo)");
    assert_eq!(out[4], "OK wrong-number-of-arguments");
    assert_eq!(out[5], "OK wrong-number-of-arguments");
}

#[test]
fn window_at_matches_batch_coordinate_and_error_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
fn windowp_true_for_stale_deleted_window() {
    crate::test_utils::init_test_tracing();
    let r = runtime_eval_one_with_usable_terminal(
        "(let ((w (split-window-internal (selected-window) nil nil nil)))
           (delete-window w)
           (windowp w))",
    );
    assert_eq!(r, "OK t");
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
fn window_buffer_returns_buffer() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(bufferp (window-buffer))");
    assert_eq!(r, "OK t");
}

#[test]
fn window_buffer_returns_nil_for_stale_deleted_window() {
    crate::test_utils::init_test_tracing();
    let r = runtime_eval_one_with_usable_terminal(
        "(let ((w (split-window-internal (selected-window) nil nil nil)))
           (delete-window w)
           (window-buffer w))",
    );
    assert_eq!(r, "OK nil");
}

#[test]
fn window_start_default() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-start)");
    assert_eq!(r, "OK 1");
}

#[test]
fn set_window_start_and_read() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((w (selected-window)))
            (save-current-buffer (set-buffer (window-buffer w))
              (erase-buffer)
              (insert (make-string 200 ?x)))
            (set-window-start w 42))
         (window-start)",
    );
    assert_eq!(results[0], "OK 42");
    assert_eq!(results[1], "OK 42");
}

#[test]
fn window_point_default() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame("(window-point)");
    assert_eq!(r, "OK 1");
}

#[test]
fn set_window_point_and_read() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((w (selected-window)))
            (save-current-buffer (set-buffer (window-buffer w))
              (erase-buffer)
              (insert (make-string 200 ?x)))
            (set-window-point w 10))
         (window-point)",
    );
    assert_eq!(results[0], "OK 10");
    assert_eq!(results[1], "OK 10");
}

#[test]
fn window_point_selected_window_uses_live_buffer_point_when_current_buffer_differs() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame(
        r#"(let* ((w (selected-window))
                  (orig (window-buffer w))
                  (other (get-buffer-create "*other*")))
             (save-current-buffer (set-buffer orig)
               (erase-buffer)
               (insert "abc
def
")
               (goto-char 5))
             (save-current-buffer (set-buffer other)
               (list (eq (current-buffer) orig)
                     (window-point w)
                     (save-current-buffer (set-buffer orig) (point)))))"#,
    );
    assert_eq!(r, "OK (nil 5 5)");
}

#[test]
fn window_point_nonselected_window_reads_marker_adjusted_by_buffer_edits() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        r#"(let ((other (split-window-internal (selected-window) nil nil nil)))
             (erase-buffer)
             (insert "    alpha
beta
")
             (set-window-point other 12)
             (replace-region-contents 1 5 "")
             (list (buffer-string) (window-point other)))"#,
    );
    assert_eq!(
        result,
        r#"OK ("alpha
beta
" 8)"#
    );
}

#[test]
fn set_window_point_selected_window_updates_live_buffer_point_when_current_buffer_differs() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame(
        r#"(let* ((w (selected-window))
                  (orig (window-buffer w))
                  (other (get-buffer-create "*other*")))
             (save-current-buffer (set-buffer orig)
               (erase-buffer)
               (insert "abc
def
")
               (goto-char 5))
             (save-current-buffer (set-buffer other)
               (set-window-point w 2)
               (list (buffer-name (current-buffer))
                     (window-point w)
                     (save-current-buffer (set-buffer orig) (point)))))"#,
    );
    assert_eq!(r, "OK (\"*other*\" 2 2)");
}

#[test]
fn set_window_point_preserves_window_old_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame(
        r#"(let* ((w (selected-window))
                  (b (get-buffer-create " *window-old-point*")))
             (unwind-protect
                 (progn
                   (save-current-buffer
                     (set-buffer b)
                     (erase-buffer)
                     (insert (make-string 40 ?x))
                     (goto-char 7))
                   (set-window-buffer w b)
                   (set-window-point w 13)
                   (list (window-point w) (window-old-point w)))
               (if (buffer-live-p b) (kill-buffer b))))"#,
    );
    assert_eq!(r, "OK (13 7)");
}

#[test]
fn selected_window_sync_prefers_live_current_buffer_point_before_resync() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
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
        .goto_buffer_emacs_byte_pos(buffer_id, crate::buffer::EmacsBytePos::new(4))
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

    crate::emacs_core::window_cmds::remember_selected_window_point_in_state(
        &mut ev.frames,
        &mut ev.buffers,
        frame_id,
    );
    crate::emacs_core::window_cmds::sync_selected_window_buffer_in_state(
        &ev.frames,
        &mut ev.buffers,
        frame_id,
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
fn set_window_start_point_and_group_start_accept_marker_positions() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let out = ev
        .eval_str_each(
            "(let* ((w (selected-window))
                (m (with-current-buffer (window-buffer w)
                     (erase-buffer)
                     (insert \"abcdef\")
                     (goto-char 3)
                     (point-marker))))
           (list (markerp (set-window-start w m))
                 (window-start w)
                 (set-window-point w m)
                 (window-point w)
                 (markerp (set-window-group-start w m))
                 (window-start w)
                 (window-point w)))
         (let* ((w (selected-window))
                (_ (progn
                     (set-window-start w 7)
                     (set-window-point w 7)))
                (m (with-current-buffer (get-buffer-create \" *neovm-marker-other*\")
                     (erase-buffer)
                     (insert \"xyz\")
                     (goto-char 2)
                     (point-marker))))
           (list (markerp (set-window-start w m))
                 (window-start w)
                 (set-window-point w m)
                 (window-point w)
                 (markerp (set-window-group-start w m))
                 (window-start w)
                 (window-point w)))
         (let* ((w (selected-window))
                (_ (set-window-start w 1))
                (_ (set-window-point w 1)))
           (list (= (set-window-start w 0) 0)
                 (= (window-start w) 1)
                 (= (set-window-point w 0) 0)
                 (= (window-point w) 1)
                 (= (set-window-group-start w 0) 0)
                 (= (window-group-start w) 1)
                 (= (window-point w) 1)
                 (= (set-window-start w -10) -10)
                 (= (window-start w) 1)
                 (= (set-window-point w -10) -10)
                 (= (window-point w) 1)
                 (= (set-window-group-start w -10) -10)
                 (= (window-group-start w) 1)
                 (= (window-point w) 1)))
         (let* ((w (selected-window))
                (_ (set-window-start w 1))
                (_ (set-window-point w 1))
                (m0 (make-marker))
                (_ (set-marker m0 0 (window-buffer w)))
                (mneg (make-marker))
                (_ (set-marker mneg -5 (window-buffer w))))
           (list (markerp (set-window-start w m0))
                 (= (window-start w) 1)
                 (= (set-window-point w m0) 1)
                 (= (window-point w) 1)
                 (markerp (set-window-group-start w m0))
                 (= (window-group-start w) 1)
                 (= (window-point w) 1)
                 (markerp (set-window-start w mneg))
                 (= (window-start w) 1)
                 (= (set-window-point w mneg) 1)
                 (= (window-point w) 1)
                 (markerp (set-window-group-start w mneg))
                 (= (window-group-start w) 1)
                 (= (window-point w) 1)))
         (let* ((w (selected-window))
                (_ (with-current-buffer (window-buffer w)
                     (erase-buffer)
                     (insert \"abcdef\")
                     (goto-char 1)))
                (_ (set-window-start w 1))
                (_ (set-window-point w 1)))
           (list (= (set-window-start w 9999) 9999)
                 (= (window-start w) 7)
                 (= (set-window-point w 9999) 9999)
                 (= (window-point w) 7)
                 (= (set-window-group-start w 9999) 9999)
                 (= (window-group-start w) 7)
                 (= (window-point w) 7)))
         (let* ((w (selected-window))
                (_ (with-current-buffer (window-buffer w)
                     (erase-buffer)
                     (insert \"abcdef\")
                     (goto-char 1)))
                (m (make-marker))
                (_ (set-marker m 9999 (window-buffer w))))
           (list (markerp (set-window-start w m))
                 (= (window-start w) 7)
                 (= (set-window-point w m) 7)
                 (= (window-point w) 7)
                 (markerp (set-window-group-start w m))
                 (= (window-group-start w) 7)
                 (= (window-point w) 7)))
         (let ((m (make-marker)))
           (list (condition-case err (set-window-start (selected-window) m) (error err))
                 (condition-case err (set-window-point (selected-window) m) (error err))
                 (condition-case err (set-window-group-start (selected-window) m) (error err))))
         (list (condition-case err (set-window-start nil 1.5) (error err))
               (condition-case err (set-window-point nil 1.5) (error err))
               (condition-case err (set-window-group-start nil 1.5) (error err))
               (condition-case err (set-window-start nil 'foo) (error err))
               (condition-case err (set-window-point nil 'foo) (error err))
               (condition-case err (set-window-group-start nil 'foo) (error err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (t 3 3 3 t 3 3)");
    assert_eq!(out[1], "OK (t 2 2 2 t 2 2)");
    assert_eq!(out[2], "OK (t t t t t t t t t t t t t t)");
    assert_eq!(out[3], "OK (t t t t t t t t t t t t t t)");
    assert_eq!(out[4], "OK (t t t t t t t)");
    assert_eq!(out[5], "OK (t t t t t t t)");
    assert_eq!(
        out[6],
        "OK (#<marker in no buffer> (error \"Marker does not point anywhere\") #<marker in no buffer>)"
    );
    assert_eq!(
        out[7],
        "OK ((wrong-type-argument integer-or-marker-p 1.5) (wrong-type-argument integer-or-marker-p 1.5) (wrong-type-argument integer-or-marker-p 1.5) (wrong-type-argument integer-or-marker-p foo) (wrong-type-argument integer-or-marker-p foo) (wrong-type-argument integer-or-marker-p foo))"
    );
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
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*remapped-window-body*");
    eval.buffers.set_current(buffer);
    let frame_id = eval.frames.create_frame("F1", 800, 600, buffer);
    eval.frames
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

    assert_eq!(
        crate::emacs_core::print::print_value(&value),
        "(97 48 35 17)"
    );
}

#[test]
fn window_body_remap_uses_the_current_buffers_face_remapping_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let displayed = eval.buffers.create_buffer("*displayed-window-body*");
    eval.buffers.set_current(displayed);
    let frame_id = eval.frames.create_frame("F1", 800, 600, displayed);
    eval.frames
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));

    // GNU's `window_body_width` reads the buffer-local
    // `Vface_remapping_alist` of the current buffer.  The WINDOW argument
    // supplies geometry; it does not switch buffers as a side effect.
    let query_buffer = eval.buffers.create_buffer("*query-window-body*");
    eval.buffers.set_current(query_buffer);
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
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
    let window_id = ev.frames.get(fid).expect("frame").selected_window;
    ev.frames
        .get_mut(fid)
        .expect("frame")
        .prepare_and_activate_display_presentation_for_test(
            crate::window::geometry::PresentationId::new(1),
            vec![crate::window::WindowDisplaySnapshot {
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
        super::builtin_window_body_width(&mut ev, vec![Value::NIL, Value::T])
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
        super::builtin_window_body_height(&mut ev, vec![Value::NIL, Value::T])
            .expect("window-body-height"),
        Value::fixnum(568)
    );
    assert_eq!(
        super::builtin_window_fringes(&mut ev, vec![Value::NIL]).expect("window-fringes"),
        Value::list(vec![
            Value::fixnum(8),
            Value::fixnum(12),
            Value::NIL,
            Value::NIL,
        ])
    );
    assert_eq!(
        super::builtin_window_margins(&mut ev, vec![Value::NIL]).expect("window-margins"),
        Value::cons(Value::fixnum(1), Value::fixnum(2))
    );
}

#[test]
fn gui_window_fringes_default_to_frame_defaults_when_reset() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    ev.frames
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
        super::builtin_window_fringes(&mut ev, vec![Value::NIL]).expect("default fringes"),
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
        super::builtin_window_fringes(&mut ev, vec![Value::NIL]).expect("explicit fringes"),
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
        super::builtin_window_fringes(&mut ev, vec![Value::NIL]).expect("reset fringes"),
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    ev.frames
        .get_mut(fid)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));

    assert_eq!(
        ev.eval_str("(window-scroll-bars)")
            .map(|value| crate::emacs_core::print::print_value(&value))
            .expect("default scroll bars"),
        "(nil 1 t nil 0 t nil)"
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
        .map(|value| crate::emacs_core::print::print_value(&value))
        .expect("realized scroll bars"),
        "((13 2 left 9 1 bottom t) 13 9)"
    );
}

#[test]
fn gui_window_body_geometry_excludes_scroll_bar_area() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    ev.frames
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
            .map(|value| crate::emacs_core::print::print_value(&value))
            .expect("body geometry"),
        "(771 771)"
    );
}

#[test]
fn gui_set_window_buffer_applies_buffer_local_display_defaults() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let fid = ev.frames.create_frame("F1", 800, 600, scratch);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    let buffer_name = " *gui-swb-display*";
    let buffer_id = ev.buffers.create_buffer(buffer_name);
    ev.buffers
        .set_buffer_local_property(buffer_id, "left-fringe-width", Value::fixnum(3))
        .expect("left fringe");
    ev.buffers
        .set_buffer_local_property(buffer_id, "right-fringe-width", Value::fixnum(5))
        .expect("right fringe");
    ev.buffers
        .set_buffer_local_property(buffer_id, "fringes-outside-margins", Value::T)
        .expect("outside margins");
    ev.buffers
        .set_buffer_local_property(buffer_id, "scroll-bar-width", Value::fixnum(11))
        .expect("scroll bar width");
    ev.buffers
        .set_buffer_local_property(buffer_id, "vertical-scroll-bar", Value::symbol("left"))
        .expect("vertical scroll bar");
    ev.buffers
        .set_buffer_local_property(buffer_id, "scroll-bar-height", Value::fixnum(7))
        .expect("scroll bar height");
    ev.buffers
        .set_buffer_local_property(buffer_id, "horizontal-scroll-bar", Value::symbol("bottom"))
        .expect("horizontal scroll bar");

    ev.eval_str("(set-window-buffer (selected-window) \" *gui-swb-display*\")")
        .expect("set-window-buffer");
    let window_id = ev.frames.get(fid).expect("frame").selected_window;
    ev.frames
        .get_mut(fid)
        .expect("frame")
        .prepare_and_activate_display_presentation_for_test(
            crate::window::geometry::PresentationId::new(1),
            vec![crate::window::WindowDisplaySnapshot {
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
        .map(|value| crate::emacs_core::print::print_value(&value))
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
fn get_buffer_window_prefers_selected_window_when_buffer_is_displayed_twice() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        r#"(let ((shared (get-buffer-create "*get-buffer-window-shared*")))
             (unwind-protect
                 (progn
                   (set-window-buffer (selected-window) shared)
                   (let ((second (split-window-internal (selected-window) nil nil nil)))
                     (set-window-buffer second shared)
                     (select-window second)
                     (eq (get-buffer-window shared) (selected-window))))
               (kill-buffer shared)))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn get_buffer_window_list_returns_matching_windows() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_with_frame("(length (get-buffer-window-list (window-buffer)))");
    assert_eq!(result[0], "OK 1");
}

#[test]
fn get_buffer_window_list_includes_active_minibuffer_by_default() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let fid = super::seed_batch_startup_frame_in_state(&mut ev.frames, &mut ev.buffers);
    let minibuffer_buffer = {
        let frame = ev.frames.get(fid).expect("frame should exist");
        let minibuffer_wid = frame.minibuffer_window.expect("minibuffer window");
        frame
            .find_window(minibuffer_wid)
            .and_then(|window| window.buffer_id())
            .expect("minibuffer buffer")
    };
    ev.minibuffers
        .read_from_minibuffer(minibuffer_buffer, "M-x ", None, None)
        .expect("active minibuffer state");
    ev.buffers.set_current(minibuffer_buffer);

    let active_minibuffer =
        super::builtin_active_minibuffer_window(&mut ev, vec![]).expect("active minibuffer");
    let default_list =
        super::builtin_window_list_1(&mut ev, vec![Value::NIL, Value::NIL, Value::NIL])
            .expect("window-list-1 with nil minibuf");
    let default_windows =
        crate::emacs_core::value::list_to_vec(&default_list).expect("default window list");
    assert!(
        default_windows.contains(&active_minibuffer),
        "nil MINIBUF should include the active minibuffer"
    );

    let excluded_list =
        super::builtin_window_list_1(&mut ev, vec![Value::NIL, Value::symbol("not"), Value::NIL])
            .expect("window-list-1 with non-nil non-t minibuf");
    let excluded_windows =
        crate::emacs_core::value::list_to_vec(&excluded_list).expect("excluded window list");
    assert!(
        !excluded_windows.contains(&active_minibuffer),
        "non-t MINIBUF should exclude the active minibuffer"
    );
}

#[test]
fn get_buffer_window_and_list_match_optional_and_missing_buffer_semantics() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((vm-gbwl-live (generate-new-buffer \"gbwl-live\"))
               (vm-gbwl-dead (generate-new-buffer \"gbwl-dead\")))
           (list
            (windowp (condition-case err (get-buffer-window) (error err)))
            (windowp (condition-case err (get-buffer-window nil) (error err)))
            (condition-case err (get-buffer-window \"missing\") (error err))
            (windowp (get-buffer-window \"*scratch*\"))
            (length (get-buffer-window-list))
            (length (get-buffer-window-list nil))
            (length (get-buffer-window-list \"*scratch*\"))
            (condition-case err (get-buffer-window-list \"missing\") (error err))
            (condition-case err (get-buffer-window-list 1) (error err))
            (prog1 (condition-case err (get-buffer-window-list vm-gbwl-live) (error err))
              (kill-buffer vm-gbwl-live))
            (progn
              (kill-buffer vm-gbwl-dead)
              (condition-case err (get-buffer-window-list vm-gbwl-dead) (error err)))))",
    );
    assert_eq!(
        results[0],
        "OK (t t nil t 1 1 1 (error \"No such live buffer missing\") (error \"No such buffer 1\") nil (error \"No such live buffer #<killed buffer>\"))"
    );
}

#[test]
fn fit_window_to_buffer_returns_nil_in_batch_mode() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_with_frame("(fit-window-to-buffer)");
    assert_eq!(result[0], "OK nil");
}

#[test]
fn fit_window_to_buffer_invalid_window_designators_signal_error() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (fit-window-to-buffer 999999) (error (car err)))
         (condition-case err (fit-window-to-buffer 'foo) (error (car err)))",
    );
    // GNU's Lisp `fit-window-to-buffer` normalizes WINDOW via
    // `window-normalize-window`, which signals plain `error` for invalid
    // designators.
    assert_eq!(results[0], "OK error");
    assert_eq!(results[1], "OK error");
}

#[test]
fn window_resize_apply_preserves_lisp_computed_vertical_sizes() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((w1 (selected-window))
                  (w2 (split-window-internal w1 nil nil nil))
                  (root (frame-root-window))
                  (root-pixels (window-pixel-height root))
                  (char-height (frame-char-height)))
             (let* ((frame (window-frame root))
                    (small-pixels (min (* 5 char-height)
                                       (max char-height (- root-pixels char-height))))
                    (large-pixels (- root-pixels small-pixels)))
               (set-window-new-pixel root root-pixels)
               (set-window-new-pixel w1 large-pixels)
               (set-window-new-pixel w2 small-pixels)
             (list (window-resize-apply frame nil)
                     (= (window-pixel-height w1) large-pixels)
                     (= (window-pixel-height w2) small-pixels)
                     (= (+ (window-pixel-height w1)
                           (window-pixel-height w2))
                        root-pixels))))"#,
    );
    assert_eq!(result, "OK (t t t t)");
}

#[test]
fn resize_mini_window_internal_applies_staged_pixel_heights() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (root-height (window-pixel-height root))
                  (mini-height (window-pixel-height mini))
                  (delta (frame-char-height frame)))
             (set-window-new-pixel root (- root-height delta))
             (set-window-new-pixel mini (+ mini-height delta))
             (list (resize-mini-window-internal mini)
                   (= (window-pixel-height root) (- root-height delta))
                   (= (window-pixel-height mini) (+ mini-height delta))
                   (= (cadr (window-pixel-edges mini))
                      (nth 3 (window-pixel-edges root)))
                   (= (window-new-pixel mini) 0)))"#,
    );
    assert_eq!(result, "OK (t t t t t)");
}

#[test]
fn resize_mini_window_internal_rejects_nonconserving_geometry() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (root-height (window-pixel-height root))
                  (mini-height (window-pixel-height mini))
                  (delta (frame-char-height frame)))
             (set-window-new-pixel root (- root-height delta))
             (set-window-new-pixel mini (+ mini-height delta 2))
             (list (condition-case err
                       (resize-mini-window-internal mini)
                     (error (car err)))
                   (= (window-pixel-height root) root-height)
                   (= (window-pixel-height mini) mini-height)))"#,
    );
    assert_eq!(result, "OK (error t t)");
}

#[test]
fn resize_mini_window_internal_rejects_missing_root_staging() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (root-height (window-pixel-height root))
                  (mini-height (window-pixel-height mini)))
             (set-window-new-pixel mini mini-height)
             (list (condition-case err
                       (resize-mini-window-internal mini)
                     (error (car err)))
                   (= (window-pixel-height root) root-height)
                   (= (window-pixel-height mini) mini-height)))"#,
    );
    assert_eq!(result, "OK (error t t)");
}

#[test]
fn resize_mini_window_internal_rejects_non_positive_minibuffer_staging() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (root-height (window-pixel-height root))
                  (mini-height (window-pixel-height mini)))
             (set-window-new-pixel root (+ root-height mini-height))
             (set-window-new-pixel mini 0)
             (list (condition-case err
                       (resize-mini-window-internal mini)
                     (error (car err)))
                   (= (window-pixel-height root) root-height)
                   (= (window-pixel-height mini) mini-height)))"#,
    );
    assert_eq!(result, "OK (error t t)");
}

#[test]
fn window_resize_minibuffer_grows_and_conserves_pixel_height() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let* ((frame (selected-frame))
                  (root (frame-root-window frame))
                  (mini (minibuffer-window frame))
                  (old-root-height (window-pixel-height root))
                  (old-mini-height (window-pixel-height mini))
                  (old-total (+ old-root-height old-mini-height)))
             (window-resize mini 1)
             (let ((new-root-height (window-pixel-height root))
                   (new-mini-height (window-pixel-height mini)))
               (list (> new-mini-height old-mini-height)
                     (= (+ new-root-height new-mini-height) old-total))))"#,
    );
    assert_eq!(result, "OK (t t)");
}

#[test]
fn display_buffer_fit_window_to_buffer_shrinks_new_window() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((buf (get-buffer-create "*fit-window-probe*")))
             (with-current-buffer buf
               (erase-buffer)
               (insert "a\nb\nc\n"))
             (let ((window
                    (display-buffer
                     buf
                     '((display-buffer-below-selected)
                       . ((window-height . fit-window-to-buffer))))))
               (list (eq window-combination-limit 'window-size)
                     (window-live-p window)
                     (not (null (window-combined-p window)))
                     (= (window-total-height window) window-min-height))))"#,
    );
    assert_eq!(result, "OK (t t t t)");
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
fn window_list_1_stale_window_signals_wrong_type_argument() {
    crate::test_utils::init_test_tracing();
    let r = runtime_eval_one_with_usable_terminal(
        "(let ((w (split-window-internal (selected-window) nil nil nil)))
           (delete-window w)
           (list (condition-case err (window-list-1 w nil) (error (car err)))
                 (condition-case err (funcall #'window-list-1 w nil) (error (car err)))
                 (condition-case err (apply #'window-list-1 (list w nil)) (error (car err)))))",
    );
    assert_eq!(
        r,
        "OK (wrong-type-argument wrong-type-argument wrong-type-argument)"
    );
}

#[test]
fn window_list_1_all_frames_includes_other_frame_windows() {
    crate::test_utils::init_test_tracing();
    let r = runtime_eval_one_with_usable_terminal(
        "(let ((f1 (selected-frame))
              (f2 (make-frame)))
           (let ((w1 (progn (select-frame f1) (selected-window)))
                 (w2 (progn (select-frame f2) (selected-window))))
             (prog1
                 (list (null (memq w2 (window-list-1 w1 nil nil)))
                       (if (memq w2 (window-list-1 w1 nil t)) t)
                       (if (memq w2 (window-list-1 w1 nil 'visible)) t)
                       (if (memq w2 (window-list-1 w1 nil 0)) t)
                       (if (memq w2 (window-list-1 w1 nil f2)) t)
                       (null (memq w2 (window-list-1 w1 nil :bad))))
               (select-frame f1)
               (delete-frame f2))))",
    );
    assert_eq!(r, "OK (t t t t t t)");
}

#[test]
fn get_buffer_window_all_frames_selects_gnu_frame_scope() {
    crate::test_utils::init_test_tracing();
    let r = runtime_eval_one_with_usable_terminal(
        "(let ((f1 (selected-frame))
              (f2 (make-frame)))
           (let ((w1 (progn (select-frame f1) (selected-window)))
                 (w2 (progn (select-frame f2) (selected-window)))
                 (buf (current-buffer)))
             (select-frame f1)
             (prog1
                 (list (eq (get-buffer-window buf nil) w1)
                       (eq (get-buffer-window buf f2) w2)
                       (eq (get-buffer-window buf :bad) w1))
               (select-frame f1)
               (delete-frame f2))))",
    );
    assert_eq!(r, "OK (t t t)");
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
    let mut ev = Context::new();
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
fn minibuffer_window_from_window_list_supports_basic_accessors() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
            "(let ((m (car (nthcdr (1- (length (window-list nil t))) (window-list nil t)))))
           (list (window-live-p m)
                 (windowp m)
                 (buffer-name (window-buffer m))
                 (window-start m)
                 (window-point m)
                 (window-body-height m)
                 (window-body-height m t)))
         (let ((m (car (nthcdr (1- (length (window-list nil t))) (window-list nil t)))))
           (set-window-start m 7)
           (window-start m))
         (let ((m (car (nthcdr (1- (length (window-list nil t))) (window-list nil t)))))
           (set-window-point m 8)
           (window-point m))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (t t \" *Minibuf-0*\" 1 1 1 1)");
    assert_eq!(out[1], "OK 1");
    assert_eq!(out[2], "OK 1");
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
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
fn split_window_internal_creates_new() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(split-window-internal (selected-window) nil nil nil)
         (length (window-list))",
    );
    assert!(results[0].starts_with("OK "));
    assert_eq!(results[1], "OK 2");
}

#[test]
fn split_window_side_domain_matches_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        SplitWindowSide::from_lisp_value(&Value::NIL),
        Some(SplitWindowSide::Below)
    );
    assert_eq!(
        SplitWindowSide::from_lisp_value(&Value::T),
        Some(SplitWindowSide::Right)
    );
    assert_eq!(
        SplitWindowSide::from_lisp_value(&Value::symbol("above")),
        Some(SplitWindowSide::Above)
    );
    assert_eq!(
        SplitWindowSide::from_lisp_value(&Value::symbol("below")),
        Some(SplitWindowSide::Below)
    );
    assert_eq!(
        SplitWindowSide::from_lisp_value(&Value::symbol("left")),
        Some(SplitWindowSide::Left)
    );
    assert_eq!(
        SplitWindowSide::from_lisp_value(&Value::symbol("right")),
        Some(SplitWindowSide::Right)
    );
    assert_eq!(SplitWindowSide::Right.name(), "right");
    assert!(SplitWindowSide::Right.is_horizontal());
    assert!(SplitWindowSide::Left.is_horizontal());
    assert!(!SplitWindowSide::Above.is_horizontal());
    assert!(!SplitWindowSide::Below.is_horizontal());
    assert_eq!(
        SplitWindowSide::from_lisp_value(&Value::symbol("other")),
        None
    );
}

#[test]
fn split_window_side_t_splits_horizontally_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let* ((old (selected-window))
                (new (split-window old nil t)))
           (list (> (window-left-column new) (window-left-column old))
                 (= (window-top-line new) (window-top-line old))))",
    );
    assert_eq!(results[0], "OK (t t)");
}

#[test]
fn split_window_preserves_requested_normal_size_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let* ((old (selected-window))
                (new (split-window old 20 'right)))
           (list (window-total-width old)
                 (window-total-width new)
                 (window-normal-size old t)
                 (window-normal-size new t)
                 (window-normal-size old)
                 (window-normal-size new)))",
    );
    assert_eq!(results[0], "OK (20 60 0.25 0.75 1.0 1.0)");
}

#[test]
fn display_buffer_in_left_side_window_places_window_on_left_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((side-window
                (display-buffer-in-side-window
                 (get-buffer-create \"*side*\")
                 '((side . left)))))
           (list (window-edges side-window)
                 (window-total-width side-window)
                 (window-parameter side-window 'window-side)
                 (mapcar (lambda (w)
                           (list (buffer-name (window-buffer w))
                                 (window-edges w)))
                         (window-list nil 'no-minibuf nil))))",
    );
    assert_eq!(
        results[0],
        "OK ((0 0 20 24) 20 left ((\"*scratch*\" (20 0 80 24)) (\"*side*\" (0 0 20 24))))"
    );
}

#[test]
fn display_buffer_respects_requested_width_when_buffer_width_is_fixed() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((buffer (get-buffer-create \"*fixed-side*\")))
           (with-current-buffer buffer
             (setq-local window-size-fixed 'width))
           (let ((side-window
                  (display-buffer-in-side-window
                   buffer
                   '((side . left) (window-width . 20)))))
             (list (window-edges side-window)
                   (window-total-width side-window)
                   (window-size-fixed-p side-window t)
                   (mapcar (lambda (w)
                             (list (buffer-name (window-buffer w))
                                   (window-edges w)))
                           (window-list nil 'no-minibuf nil)))))",
    );
    assert_eq!(
        results[0],
        "OK ((0 0 20 24) 20 (width t) ((\"*scratch*\" (20 0 80 24)) (\"*fixed-side*\" (0 0 20 24))))"
    );
}

#[test]
fn display_buffer_side_window_splits_internal_root_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(progn
           (display-buffer (get-buffer-create \"*Warnings*\"))
           (let ((buffer (get-buffer-create \"*fixed-side*\")))
             (with-current-buffer buffer
               (setq-local window-size-fixed 'width))
             (let ((side-window
                    (display-buffer-in-side-window
                     buffer
                     '((side . left) (window-width . 20)))))
               (list (window-live-p side-window)
                     (window-edges side-window)
                     (window-total-width side-window)
                     (window-size-fixed-p side-window t)
                     (mapcar (lambda (w)
                               (list (buffer-name (window-buffer w))
                                     (window-edges w)
                                     (window-width w)
                                     (window-parameter w 'window-side)))
                             (window-list nil 'no-minibuf nil))))))",
    );
    assert_eq!(
        results[0],
        "OK (t (0 0 20 24) 20 (width t) ((\"*scratch*\" (20 0 80 12) 60 nil) (\"*Warnings*\" (20 12 80 24) 60 nil) (\"*fixed-side*\" (0 0 20 24) 19 left)))"
    );
}

#[test]
fn split_window_accepts_internal_root_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(progn
           (split-window (selected-window) nil 'below)
           (let ((new (split-window (frame-root-window) nil 'left)))
             (list (window-live-p new)
                   (window-valid-p (frame-root-window))
                   (window-edges new)
                   (mapcar (lambda (w)
                             (list (buffer-name (window-buffer w))
                                   (window-edges w)))
                           (window-list nil 'no-minibuf nil)))))",
    );
    assert_eq!(
        results[0],
        "OK (t t (0 0 40 24) ((\"*scratch*\" (40 0 80 12)) (\"*scratch*\" (40 12 80 24)) (\"*scratch*\" (0 0 40 24))))"
    );
}

#[test]
fn display_buffer_in_top_side_window_places_window_on_top_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((side-window
                (display-buffer-in-side-window
                 (get-buffer-create \"*side*\")
                 '((side . top)))))
           (list (window-edges side-window)
                 (window-total-height side-window)
                 (window-parameter side-window 'window-side)
                 (mapcar (lambda (w)
                           (list (buffer-name (window-buffer w))
                                 (window-edges w)))
                         (window-list nil 'no-minibuf nil))))",
    );
    assert_eq!(
        results[0],
        "OK ((0 0 80 6) 6 top ((\"*scratch*\" (0 6 80 24)) (\"*side*\" (0 0 80 6))))"
    );
}

#[test]
fn split_window_internal_enforces_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
            "(condition-case err
             (split-window-internal (selected-window) nil nil nil nil nil)
           (error (car err)))
         (let ((w (split-window-internal (selected-window) nil nil nil)))
           (window-live-p w))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK t");
}

#[test]
fn split_delete_window_invalid_designators_signal_error() {
    crate::test_utils::init_test_tracing();
    let results = runtime_eval_with_usable_terminal(
        "(condition-case err
             (split-window-internal 999999 nil nil nil)
           (error (car err)))
         (condition-case err
             (split-window-internal 'foo nil nil nil)
           (error (car err)))
         (condition-case err (delete-window 999999) (error (car err)))
         (condition-case err (delete-window 'foo) (error (car err)))
         (condition-case err (delete-other-windows 999999) (error (car err)))
         (condition-case err (delete-other-windows 'foo) (error (car err)))",
    );
    // `split-window-internal' is the C DEFUN (src/window.c) and signals
    // `wrong-type-argument' for an invalid window designator.
    assert_eq!(results[0], "OK wrong-type-argument");
    assert_eq!(results[1], "OK wrong-type-argument");
    // `delete-window' and `delete-other-windows' are NOT C: they are
    // lisp/window.el:4318 and :4453 and start with
    // `(window-normalize-window window)', which signals a PLAIN `error' whose
    // message is "%s is not a valid window".  The Rust subrs answered
    // `wrong-type-argument', and this assertion used to encode that.  Measured
    // on GNU 31.0.90 -Q --batch (tmp/pw61/gnu-more.txt):
    //   (delete-window 999999)        => (error "999999 is not a valid window")
    //   (delete-other-windows 'foo)   => (error "foo is not a valid window")
    // DIVERGENCES.md 154.
    assert_eq!(results[2], "OK error");
    assert_eq!(results[3], "OK error");
    assert_eq!(results[4], "OK error");
    assert_eq!(results[5], "OK error");
}

#[test]
fn delete_window_after_split() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((new-win (split-window-internal (selected-window) nil nil nil)))
           (delete-window new-win)
           (length (window-list)))",
    );
    assert_eq!(results[0], "OK 1");
}

#[test]
fn delete_window_updates_current_buffer_to_selected_window_buffer() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(save-current-buffer
           (let* ((b1 (get-buffer-create \"dw-curbuf-a\"))
                  (b2 (get-buffer-create \"dw-curbuf-b\")))
             (set-window-buffer nil b1)
             (let ((w2 (split-window-internal (selected-window) nil nil nil)))
               (set-window-buffer w2 b2)
               (select-window w2)
               (delete-window w2)
               (buffer-name (current-buffer)))))",
    );
    assert_eq!(result, "OK \"dw-curbuf-a\"");
}

#[test]
fn delete_sole_window_errors() {
    crate::test_utils::init_test_tracing();
    let r = bootstrap_eval_one_with_frame("(delete-window)");
    assert!(r.contains("ERR"), "deleting sole window should error: {r}");
}

#[test]
fn delete_window_and_delete_other_windows_enforce_max_arity() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(condition-case err (delete-window nil nil) (error (car err)))
         (condition-case err (delete-other-windows nil nil nil) (error (car err)))
         (condition-case err
             (let ((w2 (split-window-internal (selected-window) nil nil nil)))
               (delete-other-windows w2 nil))
           (error err))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK wrong-number-of-arguments");
    assert_eq!(out[2], "OK nil");
}

#[test]
fn delete_other_windows_keeps_one() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(split-window-internal (selected-window) nil nil nil)
         (split-window-internal (selected-window) nil nil nil)
         (delete-other-windows)
         (length (window-list))",
    );
    assert_eq!(results[3], "OK 1");
}

#[test]
fn delete_other_windows_relocates_kept_window_across_frame_width_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(let ((right (split-window nil nil 'right)))
           (select-window right)
           (delete-other-windows)
           (let ((edges (window-edges)))
             (list (car edges) (nth 2 edges) (frame-width))))",
    );
    assert_eq!(result, "OK (0 80 80)");
}

#[test]
fn delete_other_windows_internal_only_replaces_its_root_subtree_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(let* ((left (selected-window))
                (right (split-window nil nil 'right))
                (bottom-right (split-window right nil 'below))
                (right-root (window-parent right)))
           (select-window left)
           (delete-other-windows-internal bottom-right right-root)
           (let ((left-edges (window-edges left))
                 (kept-edges (window-edges bottom-right)))
             (list (length (window-list))
                   (car left-edges) (nth 2 left-edges)
                   (car kept-edges) (nth 2 kept-edges)
                   (eq (selected-window) bottom-right))))",
    );
    assert_eq!(result, "OK (2 0 40 40 80 t)");
}

#[test]
fn delete_other_windows_updates_current_buffer_when_kept_window_differs() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(save-current-buffer
           (let* ((b1 (get-buffer-create \"dow-curbuf-a\"))
                  (b2 (get-buffer-create \"dow-curbuf-b\")))
             (set-window-buffer nil b1)
             (let ((w2 (split-window-internal (selected-window) nil nil nil))
                   (w1 (selected-window)))
               (set-window-buffer w2 b2)
               (select-window w2)
               (delete-other-windows w1)
               (buffer-name (current-buffer)))))",
    );
    assert_eq!(result, "OK \"dow-curbuf-a\"");
}

#[test]
fn select_window_works() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((new-win (split-window-internal (selected-window) nil nil nil)))
           (select-window new-win)
           (eq (selected-window) new-win))",
    );
    assert_eq!(results[0], "OK t");
}

#[test]
fn select_window_selects_a_live_child_windows_owning_frame() {
    crate::test_utils::init_test_tracing();
    let result = runtime_eval_one_with_usable_terminal(
        "(let* ((parent (selected-frame))
                (child (make-frame
                        (list (cons 'parent-frame parent)
                              '(minibuffer . nil))))
                (window (frame-root-window child)))
           (select-frame parent)
           (prog1
               (list (window-live-p window)
                     (eq (select-window window t) window)
                     (eq (selected-window) window)
                     (eq (selected-frame) child)
                     (eq (frame-selected-window child) window))
             (select-frame parent)
             (delete-frame child t)))",
    );
    assert_eq!(result, "OK (t t t t t)");
}

#[test]
fn select_window_accepts_minibuffer_window_and_switches_current_buffer() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((mw (minibuffer-window)))
           (select-window mw)
           (list (eq (selected-window) mw)
                 (window-minibuffer-p (selected-window))
                 (eq (current-buffer) (window-buffer mw))))",
    );
    assert_eq!(results[0], "OK (t t t)");
}

#[test]
fn select_window_validates_designators_and_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
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
fn select_window_updates_current_buffer_to_selected_window_buffer() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        "(save-current-buffer
           (let* ((b1 (get-buffer-create \"sw-curbuf-a\"))
                  (b2 (get-buffer-create \"sw-curbuf-b\")))
             (set-window-buffer nil b1)
             (let ((w2 (split-window-internal (selected-window) nil nil nil)))
               (set-window-buffer w2 b2)
               (select-window w2)
               (buffer-name (current-buffer)))))",
    );
    assert_eq!(result, "OK \"sw-curbuf-b\"");
}

#[test]
fn select_window_runs_buffer_list_update_hook_unless_norecord() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        "(let* ((w1 (selected-window))
                (b2 (get-buffer-create \"sw-hook-buf\"))
                (w2 (split-window-internal w1 nil nil nil))
                (sw-log nil))
           (set-window-buffer w2 b2)
           (setq buffer-list-update-hook
                 (list (lambda ()
                         (setq sw-log (cons (buffer-name) sw-log)))))
           (let ((norecord (progn (select-window w2 t) sw-log)))
             (select-window w1 t)
             (setq sw-log nil)
             (let ((recorded (progn (select-window w2) sw-log)))
               (list norecord recorded (buffer-name)))))",
    );
    assert_eq!(result, "OK (nil (\"sw-hook-buf\") \"sw-hook-buf\")");
}

#[test]
fn select_window_swaps_buffer_point_between_windows() {
    crate::test_utils::init_test_tracing();
    let result = runtime_eval_one_with_usable_terminal(
        "(let ((w1 (selected-window)))
           (set-buffer (window-buffer w1))
           (insert \"0123456789abcdefghijklmnopqrstuvwxyz\")
           (let ((w2 (split-window-internal w1 nil nil nil)))
             (set-window-point w1 3)
             (set-window-point w2 10)
             (select-window w2)
             (prog1
                 (list (window-point w1)
                       (window-point w2)
                       (point))
               (select-window w1)
               (delete-window w2))))",
    );
    assert_eq!(result, "OK (3 10 10)");
}

#[test]
fn other_window_cycles() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let ((w1 (selected-window)))
           (split-window-internal (selected-window) nil nil nil)
           (other-window 1)
           (not (eq (selected-window) w1)))",
    );
    assert_eq!(results[0], "OK t");
}

#[test]
fn other_window_updates_current_buffer_to_selected_window_buffer() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        "(save-current-buffer
           (let* ((b1 (get-buffer-create \"ow-curbuf-a\"))
                  (b2 (get-buffer-create \"ow-curbuf-b\")))
             (set-window-buffer nil b1)
             (let ((w2 (split-window-internal (selected-window) nil nil nil)))
               (set-window-buffer w2 b2)
               (other-window 1)
               (buffer-name (current-buffer)))))",
    );
    assert_eq!(result, "OK \"ow-curbuf-b\"");
}

#[test]
fn other_window_requires_count_and_enforces_number_or_marker_p() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(condition-case err (other-window) (error (car err)))
         (condition-case err (other-window nil) (error err))
         (condition-case err (other-window \"x\") (error err))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK (wrong-type-argument number-or-marker-p nil)");
    assert_eq!(out[2], "OK (wrong-type-argument number-or-marker-p \"x\")");
}

#[test]
fn other_window_accepts_float_counts_with_floor_semantics() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(let* ((w1 (progn (delete-other-windows) (selected-window)))
                (w2 (split-window-internal (selected-window) nil nil nil)))
           (list
             (progn (other-window 1.5) (eq (selected-window) w2))
             (progn (select-window w1) (other-window 0.4) (eq (selected-window) w1))
             (progn (select-window w1) (other-window -0.4) (eq (selected-window) w2))
             (progn (select-window w1) (other-window -1.2) (eq (selected-window) w1))))",
    );
    assert_eq!(results[0], "OK (t t t t)");
}

#[test]
fn other_window_enforces_max_arity() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(condition-case err (other-window 1 nil nil nil) (error (car err)))
         (condition-case err
             (let ((w1 (selected-window)))
               (split-window-internal (selected-window) nil nil nil)
               (other-window 1 nil nil)
               (not (eq (selected-window) w1)))
           (error err))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK t");
}

#[test]
fn other_window_without_selected_frame_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let results = ev.eval_str_each("(other-window 1)");
    assert_eq!(format_eval_result(&results[0]), "OK nil");
}

#[test]
fn selected_frame_bootstraps_initial_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let results = ev.eval_str_each("(list (framep (selected-frame)) (length (frame-list)))");
    assert_eq!(format_eval_result(&results[0]), "OK (t 1)");
}

#[test]
fn window_size_queries_bootstrap_initial_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
fn window_use_time_and_old_state_queries_match_batch_defaults_and_error_predicates() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
            "(let* ((w (selected-window))
                (m (minibuffer-window)))
           (list (window-use-time w)
                 (window-use-time m)
                 (window-old-point w)
                 (window-old-point m)
                 (window-old-buffer w)
                 (window-old-buffer m)
                 (window-prev-buffers w)
                 (window-prev-buffers m)
                 (window-next-buffers w)
                 (window-next-buffers m)))
         (let* ((w1 (selected-window))
                (w2 (split-window-internal (selected-window) nil nil nil))
                (m (minibuffer-window)))
           (list (window-use-time w1)
                 (window-use-time w2)
                 (window-use-time m)
                 (window-old-point w1)
                 (window-old-point w2)
                 (window-old-point m)
                 (window-old-buffer w1)
                 (window-old-buffer w2)
                 (window-prev-buffers w1)
                 (window-prev-buffers w2)
                 (window-next-buffers w1)
                 (window-next-buffers w2)))
         (list (condition-case err (window-use-time 999999) (error err))
               (condition-case err (window-old-point 999999) (error err))
               (condition-case err (window-old-buffer 999999) (error err))
               (condition-case err (window-prev-buffers 999999) (error err))
               (condition-case err (window-next-buffers 999999) (error err))
               (condition-case err (window-use-time nil nil) (error err))
               (condition-case err (window-old-point nil nil) (error err))
               (condition-case err (window-old-buffer nil nil) (error err))
               (condition-case err (window-prev-buffers nil nil) (error err))
               (condition-case err (window-next-buffers nil nil) (error err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (1 0 1 1 nil nil nil nil nil nil)");
    assert_eq!(out[1], "OK (1 0 0 1 1 1 nil nil nil nil nil nil)");
    assert_eq!(
        out[2],
        "OK ((wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-number-of-arguments window-use-time 2) (wrong-number-of-arguments window-old-point 2) (wrong-number-of-arguments window-old-buffer 2) (wrong-number-of-arguments window-prev-buffers 2) (wrong-number-of-arguments window-next-buffers 2))"
    );
}

#[test]
fn window_bump_use_time_tracks_second_most_recent_window() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
        "(let* ((w1 (selected-window))
                (w2 (split-window-internal (selected-window) nil nil nil)))
           (list (window-use-time w1)
                 (window-use-time w2)
                 (window-bump-use-time w2)
                 (window-use-time w1)
                 (window-use-time w2)
                 (window-bump-use-time w1)))
         (list (condition-case err (window-bump-use-time 1) (error err))
               (condition-case err (window-bump-use-time nil nil) (error err))
               (let ((w (split-window-internal (selected-window) nil nil nil)))
                 (delete-window w)
                 (condition-case err (window-bump-use-time w) (error (car err)))))",
    );
    assert_eq!(out[0], "OK (1 0 1 2 1 nil)");
    assert_eq!(
        out[1],
        "OK ((wrong-type-argument window-live-p 1) (wrong-number-of-arguments window-bump-use-time 2) wrong-type-argument)"
    );
}

#[test]
fn window_bump_use_time_shared_state_smoke() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
            "(let* ((w1 (selected-window))
                (w2 (split-window-internal (selected-window) nil nil nil)))
           (list (window-use-time w1)
                 (window-use-time w2)
                 (window-bump-use-time w2)
                 (window-use-time w1)
                 (window-use-time w2)
                 (window-bump-use-time w1)))
         (list (condition-case err (window-bump-use-time 1) (error err))
               (condition-case err (window-bump-use-time nil nil) (error err)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (1 0 1 2 1 nil)");
    assert_eq!(
        out[1],
        "OK ((wrong-type-argument window-live-p 1) (wrong-number-of-arguments window-bump-use-time 2))"
    );
}

#[test]
fn window_vscroll_helpers_match_batch_defaults_and_error_predicates() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
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
                 (window-vscroll w t)))
         (list (condition-case err (window-vscroll 999999) (error err))
               (condition-case err (window-vscroll 'foo) (error err))
               (condition-case err (set-window-vscroll 999999 1) (error err))
               (condition-case err (set-window-vscroll 'foo 1) (error err))
               (condition-case err (set-window-vscroll nil 'foo) (error err))
               (condition-case err (window-vscroll nil nil nil) (error err))
               (condition-case err (set-window-vscroll nil 1 nil nil nil) (error err)))
         (let ((w (split-window-internal (selected-window) nil nil nil)))
           (delete-window w)
           (list (condition-case err (window-vscroll w) (error (car err)))
                 (condition-case err (set-window-vscroll w 1) (error (car err)))))",
    );
    assert_eq!(out[0], "OK (0 0 0 0 0 0 0 0 0 0)");
    assert_eq!(
        out[1],
        "OK ((wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p foo) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p foo) (wrong-type-argument numberp foo) (wrong-number-of-arguments window-vscroll 3) (wrong-number-of-arguments set-window-vscroll 5))"
    );
    assert_eq!(out[2], "OK (wrong-type-argument wrong-type-argument)");
}

#[test]
fn window_scroll_state_shared_state_smoke() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
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
fn window_hscroll_and_margin_setters_match_batch_defaults_and_error_predicates() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
        "(let* ((w (selected-window))
                (m (minibuffer-window)))
           (list (window-hscroll w)
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
                 (window-hscroll m)
                 (set-window-hscroll m 4)
                 (window-hscroll m)
                 (window-margins m)
                 (set-window-margins m 4 5)
                 (window-margins m)))
         (list (condition-case err (set-window-hscroll nil 1.5) (error err))
               (condition-case err (set-window-hscroll nil 'foo) (error err))
               (condition-case err (set-window-hscroll 999999 1) (error err))
               (condition-case err (set-window-hscroll 'foo 1) (error err))
               (condition-case err (set-window-hscroll nil) (error err))
               (condition-case err (set-window-hscroll nil 1 nil) (error err))
               (condition-case err (set-window-margins nil -1 0) (error err))
               (condition-case err (set-window-margins nil 1 -2) (error err))
               (condition-case err (set-window-margins nil 1.5 0) (error err))
               (condition-case err (set-window-margins nil 'foo 0) (error err))
               (condition-case err (set-window-margins nil 1 'foo) (error err))
               (condition-case err (set-window-margins 999999 1 2) (error err))
               (condition-case err (set-window-margins 'foo 1 2) (error err))
               (condition-case err (set-window-margins nil) (error err))
               (condition-case err (set-window-margins nil 1 2 3) (error err)))
         (let ((w (split-window-internal (selected-window) nil nil nil)))
           (delete-window w)
           (list (condition-case err (set-window-hscroll w 1) (error (car err)))
                 (condition-case err (set-window-margins w 1 2) (error (car err)))))",
    );
    assert_eq!(
        out[0],
        "OK (0 3 3 0 0 97 97 (nil) t (1 . 2) nil t (nil) t (3) nil 0 4 4 (nil) t (4 . 5))"
    );
    assert_eq!(
        out[1],
        "OK ((wrong-type-argument fixnump 1.5) (wrong-type-argument fixnump foo) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p foo) (wrong-number-of-arguments set-window-hscroll 1) (wrong-number-of-arguments set-window-hscroll 3) (args-out-of-range -1 0 2147483647) (args-out-of-range -2 0 2147483647) (wrong-type-argument integerp 1.5) (wrong-type-argument integerp foo) (wrong-type-argument integerp foo) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p foo) (wrong-number-of-arguments set-window-margins 1) (wrong-number-of-arguments set-window-margins 4))"
    );
    assert_eq!(out[2], "OK (wrong-type-argument wrong-type-argument)");
}

#[test]
fn window_fringes_and_scroll_bar_setters_match_batch_defaults_and_error_predicates() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
        "(let* ((w (selected-window))
                (m (minibuffer-window)))
           (list (window-fringes w)
                 (window-fringes m)
                 (set-window-fringes w 0 0)
                 (set-window-fringes w 1 2)
                 (set-window-fringes w nil nil)
                 (window-fringes w)
                 (window-fringes m)
                 (window-scroll-bars w)
                 (window-scroll-bars m)
                 (set-window-scroll-bars w nil nil nil nil)
                 (set-window-scroll-bars w 'left)
                 (window-scroll-bars w)
                 (window-scroll-bars m)
                 (set-window-fringes m 0 0)
                 (set-window-scroll-bars m nil)
                 (window-fringes m)
                 (window-scroll-bars m)))
         (list (condition-case err (set-window-fringes nil 1 2 nil nil nil) (error err))
               (condition-case err (set-window-scroll-bars nil nil nil nil nil nil nil) (error err))
               (condition-case err (set-window-fringes 999999 0 0) (error err))
               (condition-case err (set-window-fringes 'foo 0 0) (error err))
               (condition-case err (set-window-scroll-bars 999999 nil) (error err))
               (condition-case err (set-window-scroll-bars 'foo nil) (error err))
               (condition-case err (set-window-fringes nil) (error err))
               (condition-case err (set-window-scroll-bars) (error err)))
         (let ((w (split-window-internal (selected-window) nil nil nil)))
           (delete-window w)
           (list (condition-case err (set-window-fringes w 0 0) (error (car err)))
                 (condition-case err (set-window-scroll-bars w nil) (error (car err)))))",
    );
    assert_eq!(
        out[0],
        "OK ((0 0 nil nil) (0 0 nil nil) nil nil nil (0 0 nil nil) (0 0 nil nil) (nil 0 t nil 0 t nil) (nil 0 t nil 0 t nil) nil nil (nil 0 t nil 0 t nil) (nil 0 t nil 0 t nil) nil nil (0 0 nil nil) (nil 0 t nil 0 t nil))"
    );
    assert_eq!(
        out[1],
        "OK ((wrong-number-of-arguments set-window-fringes 6) (wrong-number-of-arguments set-window-scroll-bars 7) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p foo) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p foo) (wrong-number-of-arguments set-window-fringes 1) (wrong-number-of-arguments set-window-scroll-bars 0))"
    );
    assert_eq!(out[2], "OK (wrong-type-argument wrong-type-argument)");
}

#[test]
fn window_parameter_helpers_match_batch_defaults_and_key_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
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

#[test]
fn window_display_table_helpers_match_batch_defaults_and_set_get_semantics() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
        "(let* ((w (selected-window))
                (m (minibuffer-window))
                (dt '(1 2 3)))
           (list (null (window-display-table w))
                 (null (window-display-table m))
                 (let ((rv (set-window-display-table w dt))) (equal rv dt))
                 (equal (window-display-table w) dt)
                 (null (set-window-display-table w nil))
                 (null (window-display-table w))
                 (let ((rv (set-window-display-table m dt))) (equal rv dt))
                 (equal (window-display-table m) dt)
                 (eq (set-window-display-table m 'foo) 'foo)
                 (eq (window-display-table m) 'foo)
                 (null (set-window-display-table m nil))
                 (null (window-display-table m))))
         (list (condition-case err (window-display-table nil nil) (error err))
               (condition-case err (set-window-display-table nil nil nil) (error err))
               (condition-case err (window-display-table 999999) (error err))
               (condition-case err (set-window-display-table 999999 nil) (error err))
               (condition-case err (window-display-table 'foo) (error err))
               (condition-case err (set-window-display-table 'foo nil) (error err)))
         (let ((w (split-window-internal (selected-window) nil nil nil)))
           (delete-window w)
           (list (condition-case err (window-display-table w) (error (car err)))
                 (condition-case err (set-window-display-table w nil) (error (car err)))))",
    );
    assert_eq!(out[0], "OK (t t t t t t t t t t t t)");
    assert_eq!(
        out[1],
        "OK ((wrong-number-of-arguments window-display-table 2) (wrong-number-of-arguments set-window-display-table 3) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p foo) (wrong-type-argument window-live-p foo))"
    );
    assert_eq!(out[2], "OK (wrong-type-argument wrong-type-argument)");
}

#[test]
fn window_cursor_type_helpers_match_batch_defaults_and_set_get_semantics() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
        "(let* ((w (selected-window))
                (m (minibuffer-window)))
           (list (window-cursor-type w)
                 (window-cursor-type m)
                 (set-window-cursor-type w nil)
                 (window-cursor-type w)
                 (set-window-cursor-type w 'bar)
                 (window-cursor-type w)
                 (set-window-cursor-type w t)
                 (window-cursor-type w)
                 (set-window-cursor-type m 'hbar)
                 (window-cursor-type m)
                 (set-window-cursor-type m nil)
                 (window-cursor-type m)))
         (list (condition-case err (window-cursor-type nil nil) (error err))
               (condition-case err (set-window-cursor-type nil) (error err))
               (condition-case err (set-window-cursor-type nil nil nil) (error err))
               (condition-case err (window-cursor-type 999999) (error err))
               (condition-case err (set-window-cursor-type 999999 nil) (error err))
               (condition-case err (window-cursor-type 'foo) (error err))
               (condition-case err (set-window-cursor-type 'foo nil) (error err)))
         (let ((w (split-window-internal (selected-window) nil nil nil)))
           (delete-window w)
           (list (condition-case err (window-cursor-type w) (error (car err)))
                 (condition-case err (set-window-cursor-type w nil) (error (car err)))))",
    );
    assert_eq!(out[0], "OK (t t nil nil bar bar t t hbar hbar nil nil)");
    assert_eq!(
        out[1],
        "OK ((wrong-number-of-arguments window-cursor-type 2) (wrong-number-of-arguments set-window-cursor-type 1) (wrong-number-of-arguments set-window-cursor-type 3) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p foo) (wrong-type-argument window-live-p foo))"
    );
    assert_eq!(out[2], "OK (wrong-type-argument wrong-type-argument)");
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
    let mut ev = Context::new();
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let wid = ev.frames.get(fid).expect("frame").selected_window;

    ev.frames.set_window_cursor_type(wid, Value::symbol("bar"));
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let wid = ev.frames.get(fid).expect("frame").selected_window;

    ev.frames.set_window_cursor_type(wid, Value::symbol("bar"));
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
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
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
fn window_preserve_size_fixed_and_resizable_helpers_match_batch_semantics() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(let ((w (selected-window)))
           (list (window-size-fixed-p w)
                 (window-size-fixed-p w t)
                 (let ((r (window-preserve-size w nil t)))
                   (list (bufferp (car r))
                         (nth 1 r)
                         (integerp (nth 2 r))))
                 (window-size-fixed-p w)
                 (window-size-fixed-p w t)
                 (let ((r (window-preserve-size w t t)))
                   (list (bufferp (car r))
                         (integerp (nth 1 r))
                         (integerp (nth 2 r))))
                 (window-size-fixed-p w)
                 (window-size-fixed-p w t)
                 (window-size-fixed-p w nil t)
                 (window-size-fixed-p w t t)
                 (progn
                   (window-preserve-size w nil nil)
                   (window-preserve-size w t nil)
                   (list (window-size-fixed-p w)
                         (window-size-fixed-p w t)))))
         (let ((w (split-window-internal (selected-window) nil 'right nil)))
           (split-window-internal w nil 'below nil)
           (window-preserve-size w t t)
           (let ((before (list (window-resizable w 100 t)
                               (window-resizable w -100 t)
                               (window-resizable w 100 nil)
                               (window-resizable w -100 nil)
                               (window-size-fixed-p w)
                               (window-size-fixed-p w t)
                               (window-resizable w 1 t)
                               (window-resizable w 1 t 'preserved)
                               (window-resizable w 1.5 t)
                               (window-resizable w -1.5 t))))
             (window-preserve-size w t nil)
             (list before
                   (window-size-fixed-p w t)
                   (window-resizable w 1 t)
                   (window-resizable w 1.5 t)
                   (window-resizable w -1.5 t))))
         (list (condition-case err (window-size-fixed-p 999999) (error (car err)))
               (condition-case err (window-preserve-size 999999 nil t) (error (car err)))
               (condition-case err (window-resizable 999999 1) (error (car err)))
               (condition-case err (window-resizable nil 'foo) (error (car err)))
               (condition-case err (window-size-fixed-p nil nil nil nil) (error err))
               (condition-case err (window-preserve-size nil nil nil nil) (error err))
               (condition-case err (window-resizable nil 1 nil nil nil nil) (error err)))",
    );
    assert_eq!(
        out[0],
        "OK (nil nil (t nil t) t nil (t t t) t t nil nil (nil nil))"
    );
    assert_eq!(out[1], "OK ((0 0 8 -8 nil t 0 1 0 0) nil 1 1.5 -1.5)");
    // window-size-fixed-p, window-preserve-size, window-resizable
    // are Lisp defuns (window.el), so arity errors carry (MIN . MAX)
    // tuples instead of the function symbol.
    assert_eq!(
        out[2],
        "OK (error error error wrong-type-argument (wrong-number-of-arguments (0 . 3) 4) (wrong-number-of-arguments (0 . 3) 4) (wrong-number-of-arguments (2 . 5) 6))"
    );
}

#[test]
fn window_tree_navigation_and_normal_size_match_gnu_runtime() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(let* ((left (selected-window))
                (right (split-window nil nil 'right))
                (bottom (split-window right nil 'below))
                (root (frame-root-window))
                (vparent (window-parent right)))
           (list (window-valid-p root)
                 (window-live-p root)
                 (eq (window-parent left) root)
                 (eq (window-next-sibling left) vparent)
                 (eq (window-left-child root) left)
                 (window-top-child root)
                 (eq (window-parent right) vparent)
                 (eq (window-parent bottom) vparent)
                 (eq (window-top-child vparent) right)
                 (window-left-child vparent)
                 (eq (window-next-sibling right) bottom)
                 (eq (window-prev-sibling bottom) right)
                 (window-normal-size left)
                 (window-normal-size left t)
                 (window-normal-size right)
                 (window-normal-size right t)
                 (window-normal-size vparent)
                 (window-normal-size vparent t)))",
    );
    assert_eq!(
        out[0],
        "OK (t nil t t t nil t t t nil t t 1.0 0.5 0.5 1.0 1.0 0.5)"
    );
}

#[test]
fn vertical_motion_truncates_partial_width_split_windows_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-truncate-narrow*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert (make-string 200 ?x))
                   (let ((w2 (split-window nil 40 'right)))
                     (select-window w2)
                     (goto-char (point-min))
                     (list (window-body-width)
                           (truncated-partial-width-window-p)
                           (vertical-motion 1)
                           (point)
                           (count-screen-lines (point-min) (point-max)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(result, "OK (40 t 0 201 0)");
}

/// A buffer-local `truncate-partial-width-windows' turns partial-width
/// truncation OFF for that buffer, and screen-line motion must see it.
///
/// GNU's `Vtruncate_partial_width_windows' is a `DEFVAR_LISP', so `setq-local'
/// localizes the symbol and the C global always holds the value swapped in for
/// `current_buffer'; `init_iterator' (src/xdisp.c:3416-3426) therefore picks
/// WINDOW_WRAP/WORD_WRAP over TRUNCATE from the BUFFER's value, not from the
/// global default.  GNU's own Lisp predicate spells the same rule out:
/// `truncated-partial-width-window-p' reads
/// `(buffer-local-value 'truncate-partial-width-windows (window-buffer window))'
/// (lisp/window.el:11285-11298).
///
/// `visual-line-mode' depends on exactly this: it does
/// `(setq-local truncate-partial-width-windows nil)' (lisp/simple.el:8716) so
/// that a window narrower than the 50-column default still wraps.  Reading the
/// global instead collapses `beginning-of-visual-line' -- which is just
/// `(vertical-motion 0)' (lisp/simple.el:8573) -- onto the LOGICAL line start.
///
/// Ground truth measured under GNU Emacs 31.0.90 in a TTY frame, on a
/// 40-column partial-width window over 200 `x' characters:
///   buffer-local nil -> tpwwp nil, 6 screen lines, (vertical-motion 0) from
///                       ZV lands on 196, (vertical-motion 1) from BOB is 1.
#[test]
fn buffer_local_truncate_partial_width_windows_keeps_motion_wrapping_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-tpww-buffer-local*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert (make-string 200 ?x))
                   (let ((w2 (split-window nil 40 'right)))
                     (select-window w2)
                     (switch-to-buffer b)
                     (setq-local truncate-partial-width-windows nil)
                     (list (window-body-width)
                           (truncated-partial-width-window-p)
                           (count-screen-lines (point-min) (point-max))
                           (progn (goto-char (point-max))
                                  (vertical-motion 0)
                                  (point))
                           (progn (goto-char (point-min))
                                  (vertical-motion 1)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(result, "OK (40 nil 6 196 1)");
}

/// `word-wrap` breaks a screen line at the last WORD boundary that fits, not
/// at the window edge -- the difference between GNU's `WORD_WRAP` and
/// `WINDOW_WRAP` (`enum line_wrap_method`, src/dispextern.h), chosen by
/// `init_iterator` from the buffer's `word-wrap` (src/xdisp.c:3425-3426).
///
/// GNU records the break with `wrap_it` inside `move_it_in_display_line_to`
/// (src/xdisp.c:10280-10300): a candidate is saved at each glyph that both
/// follows a wrappable glyph and can be wrapped before -- with the default
/// `word-wrap-by-category` nil that is "the first non-whitespace after
/// whitespace" (`char_can_wrap_after` / `char_can_wrap_before`,
/// src/xdisp.c:577-617).  When the row overflows, GNU restores the saved
/// candidate; with none saved it breaks at the edge like `WINDOW_WRAP`
/// (src/xdisp.c:10612).
///
/// Ground truth measured under GNU Emacs 31.0.90 in a TTY frame, on a
/// 24-column `visual-line-mode` window over
/// "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi
/// omicron": the screen lines start at 1, 20, 43 and 60, so from position 48
/// `(vertical-motion 0)` answers 43 and `(vertical-motion 1)` answers 60 --
/// where a character wrap would answer 47 and 70.
///
/// **In a TTY frame** is load-bearing, and this test says so with
/// `(noninteractive nil)`.  `word-wrap` is an input to `init_iterator`
/// (src/xdisp.c:3425-3426), and `Fvertical_motion` reaches `init_iterator`
/// only from its non-`noninteractive` arm (src/indent.c:2287); the batch arm
/// is `vmotion` -> `compute_motion`, which has no word-wrap concept at all.
/// The batch answers to this very probe are pinned by
/// [`word_wrap_is_inert_under_the_batch_motion_engine_like_gnu`] below, and
/// they are different numbers.
#[test]
fn word_wrap_screen_line_motion_breaks_at_word_boundaries_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-word-wrap-motion*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n")
                   (let ((w2 (split-window nil -24 'right))
                         (noninteractive nil))
                     (select-window w2)
                     (switch-to-buffer b)
                     (setq-local truncate-lines nil)
                     (setq-local truncate-partial-width-windows nil)
                     (setq-local word-wrap t)
                     (list (window-body-width)
                           (progn (goto-char 48) (vertical-motion 0) (point))
                           (progn (goto-char 48) (vertical-motion 1) (point))
                           (progn (goto-char 48) (vertical-motion -1) (point))
                           (progn (goto-char 1) (vertical-motion 1) (point)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(result, "OK (24 43 60 20 20)");
}

/// The same window, the same buffer, the same `word-wrap t` -- under the BATCH
/// engine, where GNU ignores it.
///
/// `Fvertical_motion` under `noninteractive` is `vmotion` -> `compute_motion`
/// (src/indent.c:2280-2286, :1963, :1253).  `compute_motion` decides a line
/// end with exactly one test, `if (hpos > width)`, and then either truncates
/// or continues (src/indent.c:1474-1527); the identifier `word_wrap` does not
/// occur anywhere in src/indent.c.  So a 24-column window character-wraps at
/// column 23 whatever the buffer asks for, and the rows start at 1, 24, 47 and
/// 70 instead of 1, 20, 43 and 60.
///
/// Ground truth, GNU Emacs 31.0.90, the probe above run under `emacs --batch`
/// on the same 80-column frame this harness has:
///
/// ```text
///   (24 47 70 24 24)      -- batch,    this test
///   (24 43 60 20 20)      -- terminal, the test above
/// ```
///
/// Ledger 191 pinned the terminal numbers here, in a harness that is batch,
/// and made the port word-wrap in both engines to satisfy them.  That is the
/// regression this pair exists to keep out.
#[test]
fn word_wrap_is_inert_under_the_batch_motion_engine_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-word-wrap-batch*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n")
                   (let ((w2 (split-window nil -24 'right)))
                     (select-window w2)
                     (switch-to-buffer b)
                     (setq-local truncate-lines nil)
                     (setq-local truncate-partial-width-windows nil)
                     (setq-local word-wrap t)
                     (list (window-body-width)
                           (progn (goto-char 48) (vertical-motion 0) (point))
                           (progn (goto-char 48) (vertical-motion 1) (point))
                           (progn (goto-char 48) (vertical-motion -1) (point))
                           (progn (goto-char 1) (vertical-motion 1) (point)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(result, "OK (24 47 70 24 24)");
}

/// The oracle's own probe (`div_l0_word_wrap_at_spaces`), as a unit pin on
/// both engines: one 201-character line whose only space sits at column 100,
/// in an 80-column window.
///
/// The line matters because no wrap boundary is reachable on the first row --
/// 100 unbroken `x` characters exceed the width -- so the two engines part
/// company on the SECOND row: the display iterator restores the wrap point it
/// saved after the space (src/xdisp.c:10289-10300, :10601) and starts a fourth
/// row, while `compute_motion` continues at the edge and there are three.
///
/// Ground truth, GNU Emacs 31.0.90, 80-column terminal:
///
/// ```text
///   emacs --batch        rows 1 80 159       count-screen-lines 3
///   emacs -nw in a pty   rows 1 80 102 181   count-screen-lines 4
/// ```
#[test]
fn count_screen_lines_ignores_word_wrap_under_the_batch_engine_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(with-temp-buffer
             (insert (make-string 100 ?x) " " (make-string 100 ?x))
             (let ((word-wrap t))
               (count-screen-lines (point-min) (point-max))))"#,
    );
    assert_eq!(result, "OK 3");
}

/// The display-engine half of the pair above.
#[test]
fn count_screen_lines_honors_word_wrap_under_the_display_engine_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(with-temp-buffer
             (insert (make-string 100 ?x) " " (make-string 100 ?x))
             (let ((word-wrap t)
                   (noninteractive nil))
               (count-screen-lines (point-min) (point-max))))"#,
    );
    assert_eq!(result, "OK 4");
}

/// `move-to-window-line' counts SCREEN lines and answers how many it actually
/// moved over -- it is `vertical-motion' with the window's line count folded
/// into ARG, not a logical-line walk from `window-start'.
///
/// GNU `Fmove_to_window_line' (src/window.c:7498-7573):
///
/// ```c
///   Fgoto_char (w->start);
///   lines = displayed_window_lines (w);
///   if (NILP (arg)) XSETFASTINT (arg, lines / 2);
///   else { EMACS_INT iarg = XFIXNUM (Fprefix_numeric_value (arg));
///          if (iarg < 0) iarg = iarg + lines; arg = make_fixnum (iarg); }
///   if (w->vscroll) XSETINT (arg, XFIXNUM (arg) + 1);
///   return Fvertical_motion (arg, window, Qnil);
/// ```
///
/// Three consequences the port used to get wrong, all measured under GNU
/// Emacs 31.0.90 in a TTY window whose body is 47 lines:
///
///   * The value is `vertical-motion''s -- the lines actually moved over.  On a
///     three-line buffer `(move-to-window-line 5)` answers 3 and stops at ZV,
///     and `(move-to-window-line -1)` answers 3 as well.
///   * A positive ARG is NOT clamped to the window: over 200 logical lines
///     `(move-to-window-line 100)` answers 100 and lands on line 101.
///   * The lines counted are SCREEN lines.  On one 500-character logical line
///     wrapped in a 160-column window the rows start at 1, 160 and 319, so
///     `(move-to-window-line 2)` lands on 319; truncated, the same buffer has
///     one screen line and `(move-to-window-line 2)` answers 1 at ZV.
#[test]
fn move_to_window_line_counts_screen_lines_and_returns_lines_moved_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-move-to-window-line*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (erase-buffer)
                   (insert "aaa\nbbb\nccc\n")
                   (let ((walk
                          (lambda ()
                            (mapcar (lambda (n)
                                      (goto-char (point-min))
                                      (set-window-start (selected-window)
                                                        (point-min) t)
                                      (list n (move-to-window-line n) (point)))
                                    '(0 1 2 5 -1 100)))))
                     (let ((short (funcall walk)))
                       (erase-buffer)
                       (insert (make-string 500 ?x) "\n")
                       (setq-local truncate-lines nil)
                       (setq-local truncate-partial-width-windows nil)
                       (let ((wrapped (funcall walk)))
                         (setq-local truncate-lines t)
                         (let ((truncated (funcall walk)))
                           (erase-buffer)
                           (setq-local truncate-lines nil)
                           (insert (mapconcat (lambda (n) (format "line%03d" n))
                                              (number-sequence 1 200) "\n")
                                   "\n")
                           (goto-char (point-min))
                           (set-window-start (selected-window) (point-min) t)
                           (list (window-body-width)
                                 short
                                 wrapped
                                 truncated
                                 ;; A buffer taller than the window: GNU's
                                 ;; negative ARG counts from the window's own
                                 ;; line count, so -1 is the LAST window line.
                                 (list (= (move-to-window-line -1)
                                          (1- (window-body-height)))
                                       (progn
                                         (goto-char (point-min))
                                         (set-window-start (selected-window)
                                                           (point-min) t)
                                         (move-to-window-line 100)))))))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    // GNU Emacs 31.0.90 on an 80-column TTY frame:
    //   short-3-logical    ((0 0 1) (1 1 5) (2 2 9) (5 3 13) (-1 3 13) (100 3 13))
    //   one-long-wrapped   ((0 0 1) (1 1 80) (2 2 159) (5 5 396) (-1 7 502) (100 7 502))
    //   one-long-truncated ((0 0 1) (1 1 502) (2 1 502) (5 1 502) (-1 1 502) (100 1 502))
    //   tall-200-logical   (move-to-window-line -1)  = body height - 1
    //                      (move-to-window-line 100) = 100  (ARG is not clamped)
    // The window body height differs between that frame and this bootstrap
    // one, so the last pair is pinned as GNU's RELATION to the body height
    // rather than as GNU's number.
    assert_eq!(
        result,
        "OK (80 \
         ((0 0 1) (1 1 5) (2 2 9) (5 3 13) (-1 3 13) (100 3 13)) \
         ((0 0 1) (1 1 80) (2 2 159) (5 5 396) (-1 7 502) (100 7 502)) \
         ((0 0 1) (1 1 502) (2 1 502) (5 1 502) (-1 1 502) (100 1 502)) \
         (t 100))"
    );
}

/// `(vertical-motion (COLS . 0))` stops at the LAST column that does not pass
/// COLS, and never leaves the screen row.
///
/// GNU reaches the goal with
/// `move_it_in_display_line (&it, ZV, first_x + to_x, MOVE_TO_X)`
/// (src/indent.c:2540).  `move_it_in_display_line_to` places a glyph only
/// while it still fits before the goal and backs up to `x_before_this_char`
/// as soon as one would pass it (src/xdisp.c:10385-10400), and it also stops
/// where the display line itself ends (`it->last_visible_x`).
///
/// This is what `end-of-visual-line` -- `(vertical-motion (cons (window-width)
/// 0))`, lisp/simple.el:8558 -- rides on, and it is why `move-to-column`
/// cannot stand in for the walk: `move-to-column` moves PAST a TAB to the
/// column where it ends, while GNU stops before it.
///
/// Measured under GNU Emacs 31.0.90 on a 24-column TTY window:
///
///   row "\tabcdef..."  goals 0..7 -> point 1 (the TAB, column 0)
///                      goal  8    -> point 2 (column 8)
///                      goal  23   -> point 17 (column 23)
///                      goal  30   -> point 17  (saturates at the row's edge)
///   row "xxxx..."      goal  23   -> point 24 (column 23)
///                      goal  30   -> point 24
///
/// -- identical answers whether the window truncates or wraps, because both
/// display lines end at the same right edge.
#[test]
fn vertical_motion_goal_column_stops_before_overshooting_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-goal-column*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let ((w2 (split-window nil -24 'right))
                         ;; The goal-column walk is display motion: GNU leaves
                         ;; point at the line start under `noninteractive'
                         ;; (src/indent.c:2280-2286 takes the batch `vmotion'
                         ;; path), so the probe has to be an interactive one.
                         (noninteractive nil)
                         (walk
                          (lambda ()
                            (mapcar (lambda (n)
                                      (goto-char (point-min))
                                      (list n (vertical-motion (cons n 0))
                                            (point)))
                                    '(0 3 7 8 22 23 24 30)))))
                     (select-window w2)
                     (switch-to-buffer b)
                     (erase-buffer)
                     (insert "\tabcdefghijklmnopqrstuvwxyz0123456789\n")
                     (setq-local truncate-lines t)
                     (let ((tab-truncated (funcall walk)))
                       (setq-local truncate-lines nil)
                       (setq-local truncate-partial-width-windows nil)
                       (let ((tab-wrapped (funcall walk)))
                         (erase-buffer)
                         (insert (make-string 200 ?x) "\n")
                         (list (window-body-width)
                               tab-truncated
                               tab-wrapped
                               (funcall walk))))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (24 \
         ((0 0 1) (3 0 1) (7 0 1) (8 0 2) (22 0 16) (23 0 17) (24 0 17) (30 0 17)) \
         ((0 0 1) (3 0 1) (7 0 1) (8 0 2) (22 0 16) (23 0 17) (24 0 17) (30 0 17)) \
         ((0 0 1) (3 0 4) (7 0 8) (8 0 9) (22 0 23) (23 0 24) (24 0 24) (30 0 24)))"
    );
}

/// A goal column past the end of the row rests ON the row -- and where the row
/// ends depends on WHY it ended.
///
/// This is `end-of-visual-line`, which is `(vertical-motion (cons
/// (window-width) 0))` (lisp/simple.el:8558): the goal is always past the row,
/// so the answer is always the row's end boundary and nothing else.
///
/// GNU's `move_it_in_display_line_to` stops where the DISPLAY LINE ends
/// (`it->last_visible_x`), and a `WORD_WRAP` row does not reach that edge: it
/// broke at a saved wrap point (src/xdisp.c:10280-10300) whose position is
/// drawn on the NEXT row.  So a word-wrapped row's last stop is its last
/// GLYPH, while a row that filled the width has one more stop past its last
/// glyph -- the next row's first position, at the edge column.
///
/// Measured under GNU Emacs 31.0.90 on a 24-column TTY window over
/// "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi
/// omicron\n", walking the goal from 20 to 40 from each row start:
///
/// ```text
///   word wrap  row 1..19   goals 20+ -> 19   the row's LAST GLYPH
///   word wrap  row 20..42  goals 23+ -> 42   likewise (22 -> 42, 21 -> 41, 20 -> 40)
///   word wrap  row 43..59  goals 20+ -> 59   likewise
///   word wrap  row 60..83  goals 23+ -> 83   the NEWLINE, which draws nothing
///   char wrap  row 1..23   goals 23+ -> 24   the NEXT ROW's first position
///   char wrap  row 24..46  goals 23+ -> 47   likewise
/// ```
#[test]
fn goal_column_past_a_word_wrapped_row_rests_on_its_last_glyph_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-row-end*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let ((w2 (split-window nil -24 'right))
                         (noninteractive nil)
                         (walk
                          (lambda ()
                            (mapcar
                             (lambda (start)
                               (cons start
                                     (mapcar (lambda (n)
                                               (goto-char start)
                                               (vertical-motion (cons n 0))
                                               (point))
                                             '(20 21 22 23 24 40))))
                             '(1 20 43 60)))))
                     (select-window w2)
                     (switch-to-buffer b)
                     (erase-buffer)
                     (insert "  alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron\n")
                     (setq-local truncate-lines nil)
                     (setq-local truncate-partial-width-windows nil)
                     (setq-local word-wrap t)
                     (let ((wrapped (funcall walk)))
                       (setq-local word-wrap nil)
                       (list (window-body-width) wrapped (funcall walk)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (24 \
         ((1 19 19 19 19 19 19) \
          (20 40 41 42 42 42 42) \
          (43 59 59 59 59 59 59) \
          (60 80 81 82 83 83 83)) \
         ((1 21 22 23 24 24 24) \
          (20 21 22 23 24 24 24) \
          (43 44 45 46 47 47 47) \
          (60 67 68 69 70 70 70)))"
    );
}

#[test]
fn tty_window_body_width_reserves_the_non_rightmost_separator_column() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(progn
             (delete-other-windows)
             (let* ((left (selected-window))
                    (right (split-window-horizontally 50)))
               (list (list (window-body-width left)
                           (window-total-width left)
                           (window-right-divider-width left))
                     (list (window-body-width right)
                           (window-total-width right)
                           (window-right-divider-width right)))))"#,
    );
    assert_eq!(result, "OK ((49 50 0) (30 30 0))");
}

#[test]
fn raw_context_does_not_prebind_window_inside_aliases() {
    crate::test_utils::init_test_tracing();
    let eval = super::super::eval::Context::new();
    for name in ["window-inside-pixel-edges", "window-inside-edges"] {
        assert!(
            eval.obarray.symbol_function(name).is_none(),
            "{name} should come from GNU window.el, not Context::new"
        );
    }
}

#[test]
fn gnu_window_el_defines_window_inside_aliases() {
    crate::test_utils::init_test_tracing();
    let source =
        fs::read_to_string(PathBuf::from(env!("CARGO_WORKSPACE_DIR")).join("lisp/window.el"))
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

#[test]
fn window_geometry_queries_match_batch_alias_and_edge_shapes() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(list (symbol-function 'window-inside-pixel-edges)
               (symbol-function 'window-inside-edges))
         (let* ((w (selected-window))
                (m (minibuffer-window)))
           (list (window-mode-line-height w)
                 (window-mode-line-height m)
                 (window-header-line-height w)
                 (window-header-line-height m)
                 (window-pixel-height w)
                 (window-pixel-height m)
                 (window-pixel-width w)
                 (window-pixel-width m)
                 (window-text-height w)
                 (window-text-height m)
                 (window-text-height w t)
                 (window-text-height m t)
                 (window-text-width w)
                 (window-text-width m)
                 (window-text-width w t)
                 (window-text-width m t)
                 (window-body-pixel-edges w)
                 (window-body-pixel-edges m)
                 (window-pixel-edges w)
                 (window-pixel-edges m)
                 (window-body-edges w)
                 (window-body-edges m)
                 (window-edges w)
                 (window-edges m)
                 (window-edges w t)
                 (window-edges m t)))
         (list (condition-case err (window-mode-line-height 999999) (error err))
               (condition-case err (window-header-line-height 999999) (error err))
               (condition-case err (window-pixel-height 999999) (error err))
               (condition-case err (window-pixel-width 999999) (error err))
               (condition-case err (window-text-height 999999) (error err))
               (condition-case err (window-text-width 999999) (error err))
               (condition-case err (window-body-pixel-edges 999999) (error err))
               (condition-case err (window-pixel-edges 999999) (error err))
               (condition-case err (window-body-edges 999999) (error err))
               (condition-case err (window-edges 999999) (error err))
               (condition-case err (window-text-height nil nil nil) (error err))
               (condition-case err (window-mode-line-height nil nil) (error err))
               (condition-case err (window-inside-pixel-edges nil nil) (error (car err)))
               (condition-case err (window-edges nil nil nil nil) (error err))
               (condition-case err (window-edges nil nil nil nil nil) (error err)))",
    );
    assert_eq!(out[0], "OK (window-body-pixel-edges window-body-edges)");
    assert_eq!(
        out[1],
        "OK (1 0 0 0 24 1 80 80 23 1 23 1 80 80 80 80 (0 0 80 23) (0 24 80 25) (0 0 80 24) (0 24 80 25) (0 0 80 23) (0 24 80 25) (0 0 80 24) (0 24 80 25) (0 0 80 23) (0 24 80 25))"
    );
    assert_eq!(
        out[2],
        "OK ((wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-valid-p 999999) (wrong-type-argument window-valid-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (error \"999999 is not a live window\") (error \"999999 is not a valid window\") (error \"999999 is not a live window\") (error \"999999 is not a valid window\") (wrong-number-of-arguments window-text-height 3) (wrong-number-of-arguments window-mode-line-height 2) wrong-number-of-arguments (0 0 80 24) (wrong-number-of-arguments (0 . 4) 5))"
    );
}

#[test]
fn next_window_cycles() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((w1 (selected-window)))
           (split-window-internal (selected-window) nil nil nil)
           (let ((w2 (next-window)))
             (null (eq w1 w2))))",
    );
    assert_eq!(results[0], "OK t");
}

#[test]
fn one_window_p_tracks_current_window_count() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(list (one-window-p)
               (progn
                 (split-window-internal (selected-window) nil nil nil)
                 (one-window-p)))",
    );
    assert_eq!(results[0], "OK (t nil)");
}

#[test]
fn one_window_p_enforces_max_arity() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (one-window-p nil nil nil) (error (car err)))",
    );
    assert_eq!(results[0], "OK wrong-number-of-arguments");
}

#[test]
fn next_previous_window_enforce_max_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
            "(condition-case err (next-window nil nil nil nil) (error (car err)))
         (condition-case err (previous-window nil nil nil nil) (error (car err)))
         (let ((w1 (selected-window)))
           (split-window-internal (selected-window) nil nil nil)
           (windowp (next-window w1 nil nil)))
         (let ((w1 (selected-window)))
           (split-window-internal (selected-window) nil nil nil)
           (windowp (previous-window w1 nil nil)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK wrong-number-of-arguments");
    assert_eq!(out[2], "OK t");
    assert_eq!(out[3], "OK t");
}

#[test]
fn previous_window_wraps() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(split-window-internal (selected-window) nil nil nil)
         (let ((w (previous-window)))
           (windowp w))",
    );
    assert_eq!(results[1], "OK t");
}

// -- Frame operations --

#[test]
fn frame_ops_enforce_max_arity() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
        "(condition-case err (make-frame nil nil) (error (car err)))
         (condition-case err (delete-frame nil nil nil) (error (car err)))
         (condition-case err (frame-parameter nil 'name nil) (error (car err)))
         (condition-case err (frame-parameters nil nil) (error (car err)))
         (condition-case err (modify-frame-parameters nil nil nil) (error (car err)))
         (condition-case err (frame-visible-p nil nil) (error (car err)))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK wrong-number-of-arguments");
    assert_eq!(out[2], "OK wrong-number-of-arguments");
    assert_eq!(out[3], "OK wrong-number-of-arguments");
    assert_eq!(out[4], "OK wrong-number-of-arguments");
    assert_eq!(out[5], "OK wrong-number-of-arguments");
}

#[test]
fn frame_visible_p_enforces_arity_and_designators() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
    let mut ev = Context::new();
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
fn frame_identity_builtins_match_gnu_batch_startup_defaults() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
            r#"(let ((mouse (mouse-position))
                 (pixel (mouse-pixel-position)))
             (list (frame-id)
                   (eq (frame-root-frame) (selected-frame))
                   (eq (next-frame) (selected-frame))
                   (eq (previous-frame) (selected-frame))
                   (eq (old-selected-frame) (selected-frame))
                   (eq (car mouse) (selected-frame))
                   (cdr mouse)
                   (eq (car pixel) (selected-frame))
                   (cdr pixel)))"#,
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (1 t t t t t (nil) t (nil))");
}

#[test]
fn frame_query_builtins_report_pixel_sizes_for_gui_frames() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("gui", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("gui frame");
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
        Value::fixnum(776)
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("gui", 800, 600, buf);
    let selected_window = {
        let frame = ev.frames.get_mut(fid).expect("gui frame");
        frame.set_window_system(Some(Value::symbol("x")));
        frame.selected_window
    };

    assert!(
        ev.frames
            .set_window_fringes(selected_window, Some(5), Some(5), false, false),
        "window-local fringes should change"
    );

    assert_eq!(
        crate::emacs_core::frame::builtin_frame_text_width(&mut ev, vec![Value::make_frame(fid.0)])
            .unwrap(),
        Value::fixnum(776)
    );
}

#[test]
fn frame_query_builtins_use_internal_window_system_state() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("gui", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("gui frame");
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
fn select_frame_arity_designators_and_selection() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
        "(condition-case err (select-frame) (error (car err)))
         (condition-case err (select-frame nil) (error err))
         (condition-case err (select-frame \"x\") (error err))
         (condition-case err (select-frame 999999) (error err))
         (let ((f1 (selected-frame))
               (f2 (make-frame)))
           (prog1
               (list (framep (select-frame f2))
                     (eq (selected-frame) f2))
             (select-frame f1)
             (delete-frame f2)))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK (wrong-type-argument frame-live-p nil)");
    assert_eq!(out[2], "OK (wrong-type-argument frame-live-p \"x\")");
    assert_eq!(out[3], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[4], "OK (t t)");
}

#[test]
fn deleting_selected_tty_frame_restores_surviving_window_point() {
    crate::test_utils::init_test_tracing();
    let result = runtime_eval_one_with_usable_terminal(
        r##"(progn
               (erase-buffer)
               (insert "one\ntwo\nthree")
               (let* ((original-frame (selected-frame))
                      (original-window (selected-window))
                      (original-point (point))
                      (new-frame (make-frame)))
                 (select-frame new-frame)
                 (delete-frame new-frame)
                 (list (eq (selected-frame) original-frame)
                       (eq (selected-window) original-window)
                       (= (point) original-point)
                       (= (window-point original-window) original-point))))"##,
    );
    assert_eq!(result, "OK (t t t t)");
}

#[test]
fn select_frame_set_input_focus_arity_designators_and_result() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
        "(condition-case err (select-frame-set-input-focus) (error (car err)))
         (condition-case err (select-frame-set-input-focus nil) (error err))
         (condition-case err (select-frame-set-input-focus \"x\") (error err))
         (condition-case err (select-frame-set-input-focus 999999) (error err))
         (let ((f (selected-frame)))
           (list (select-frame-set-input-focus f)
                 (eq (selected-frame) f)))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK (wrong-type-argument frame-live-p nil)");
    assert_eq!(out[2], "OK (wrong-type-argument frame-live-p \"x\")");
    assert_eq!(out[3], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[4], "OK (nil t)");
}

#[test]
fn set_frame_selected_window_matches_selection_and_error_semantics() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
        "(condition-case err (set-frame-selected-window) (error (car err)))
         (condition-case err (set-frame-selected-window nil nil) (error err))
         (condition-case err (set-frame-selected-window nil 999999) (error err))
         (let* ((w1 (selected-window))
                (w2 (split-window-internal (selected-window) nil nil nil)))
           (prog1
               (list (eq (set-frame-selected-window nil w2) w2)
                     (eq (selected-window) w2))
             (select-window w1)
             (delete-window w2)))
         (let* ((w1 (selected-window))
                (w2 (split-window-internal (selected-window) nil nil nil))
                (t1 (window-use-time w1))
                (t2 (window-use-time w2)))
           (prog1
               (list (eq (set-frame-selected-window nil w2 t) w2)
                     (= (window-use-time w1) t1)
                     (= (window-use-time w2) t2)
                     (eq (selected-window) w2))
             (select-window w1)
             (delete-window w2)))
         (let* ((f1 (selected-frame))
                (f2 (make-frame))
                (w2 (progn
                      (select-frame f2)
                      (split-window-internal (selected-window) nil nil nil))))
           (select-frame f1)
           (prog1
               (list (eq (set-frame-selected-window f2 w2) w2)
                     (eq (selected-frame) f1)
                     (eq (frame-selected-window f2) w2))
             (select-frame f2)
             (delete-window w2)
             (select-frame f1)
             (delete-frame f2)))
         (let* ((f2 (make-frame))
                (w1 (selected-window)))
           (prog1
               (condition-case err (set-frame-selected-window f2 w1) (error err))
             (delete-frame f2)))
         (let* ((w1 (selected-window))
                (w2 (split-window-internal (selected-window) nil nil nil)))
           (prog1
               (list (eq (funcall #'set-frame-selected-window nil w2) w2)
                     (eq (apply #'set-frame-selected-window (list nil w1)) w1))
             (select-window w1)
             (delete-window w2)))
         (list (condition-case err (funcall #'set-frame-selected-window nil (selected-window) nil nil) (error err))
               (condition-case err (apply #'set-frame-selected-window '(nil)) (error err)))",
    );
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK (wrong-type-argument window-live-p nil)");
    assert_eq!(out[2], "OK (wrong-type-argument window-live-p 999999)");
    assert_eq!(out[3], "OK (t t)");
    assert_eq!(out[4], "OK (t t t t)");
    assert_eq!(out[5], "OK (t t t)");
    assert_eq!(
        out[6],
        "OK (error \"In `set-frame-selected-window', WINDOW is not on FRAME\")"
    );
    assert_eq!(out[7], "OK (t t)");
    assert_eq!(
        out[8],
        "OK ((wrong-number-of-arguments #<subr set-frame-selected-window> 4) (wrong-number-of-arguments #<subr set-frame-selected-window> 1))"
    );
}

#[test]
fn old_selected_window_matches_stable_and_stale_window_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let out = ev
        .eval_str_each(
            "(windowp (old-selected-window))
         (let* ((w1 (selected-window))
                (w2 (split-window-internal (selected-window) nil nil nil)))
           (prog1
               (list (eq (old-selected-window) w1)
                     (progn (select-window w2) (eq (old-selected-window) w1))
                     (progn (select-window w1) (eq (old-selected-window) w1))
                     (progn (other-window 1) (eq (old-selected-window) w1))
                     (progn (other-window 1) (eq (old-selected-window) w1))
                     (progn (select-window w2 t) (eq (old-selected-window) w1))
                     (progn (select-window w1 t) (eq (old-selected-window) w1)))
             (select-window w1)
             (delete-window w2)))
         (let* ((w1 (selected-window))
                (w2 (split-window-internal (selected-window) nil nil nil)))
           (prog1
               (list (progn (select-window w2) (eq (old-selected-window) w1))
                     (progn (delete-window w1) (windowp (old-selected-window)))
                     (window-live-p (old-selected-window))
                     (eq (old-selected-window) w2))
             (delete-other-windows w2)))
         (list (condition-case err (old-selected-window nil) (error (car err)))
               (eq (funcall #'old-selected-window) (old-selected-window))
               (eq (apply #'old-selected-window nil) (old-selected-window))
               (condition-case err (funcall #'old-selected-window nil) (error (car err)))
               (condition-case err (apply #'old-selected-window '(nil)) (error (car err))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK (t t t t t t t)");
    assert_eq!(out[2], "OK (t t nil nil)");
    assert_eq!(
        out[3],
        "OK (wrong-number-of-arguments t t wrong-number-of-arguments wrong-number-of-arguments)"
    );
}

#[test]
fn frame_old_selected_window_matches_batch_and_arity_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let out = ev
        .eval_str_each(
        "(condition-case err (frame-old-selected-window 999999) (error err))
         (condition-case err (frame-old-selected-window 'foo) (error err))
         (condition-case err (frame-old-selected-window nil nil) (error (car err)))
         (let ((f (selected-frame)))
           (list (frame-old-selected-window)
                 (frame-old-selected-window nil)
                 (frame-old-selected-window f)
                 (frame-old-selected-window (window-frame (selected-window)))))
         (let* ((w1 (selected-window))
                (w2 (split-window-internal (selected-window) nil nil nil)))
           ;; GNU `Fframe_old_selected_window` returns
           ;; `frame->old_selected_window`, which is updated only
           ;; by `window_change_record` (run from
           ;; `run_window_change_functions` at redisplay time, see
           ;; `src/window.c:3954-3990`). In batch mode the change
           ;; hooks never run, so the field stays at its initial
           ;; nil. Verified against GNU Emacs 31.0.50 with
           ;; `(emacs -Q --batch ...)`. Window audit Critical 8 in
           ;; `drafts/window-system-audit.md`.
           (prog1
               (list (eq (frame-old-selected-window) nil)
                     (progn (select-window w2) (eq (frame-old-selected-window) nil))
                     (progn (other-window 1) (eq (frame-old-selected-window) nil))
                     (progn (set-frame-selected-window nil w2) (eq (frame-old-selected-window) nil))
                     (progn (set-frame-selected-window nil w1) (eq (frame-old-selected-window) nil))
                     (progn (set-frame-selected-window nil w2 t) (eq (frame-old-selected-window) nil))
                     (progn (set-frame-selected-window nil w1 t) (eq (frame-old-selected-window) nil)))
             (select-window w1)
             (delete-window w2)))
         (list (condition-case err (funcall #'frame-old-selected-window nil nil) (error err))
               (condition-case err (apply #'frame-old-selected-window '(nil nil)) (error err))
               (eq (funcall #'frame-old-selected-window) (frame-old-selected-window))
               (eq (apply #'frame-old-selected-window nil) (frame-old-selected-window)))",
    )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[1], "OK (wrong-type-argument frame-live-p foo)");
    assert_eq!(out[2], "OK wrong-number-of-arguments");
    assert_eq!(out[3], "OK (nil nil nil nil)");
    assert_eq!(out[4], "OK (t t t t t t t)");
    assert_eq!(
        out[5],
        "OK ((wrong-number-of-arguments #<subr frame-old-selected-window> 2) (wrong-number-of-arguments #<subr frame-old-selected-window> 2) t t)"
    );
}

#[test]
fn frame_old_selected_window_direct_wrapper_matches_batch_nil_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
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
    match err {
        crate::emacs_core::error::Flow::Signal(sig) => {
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
fn make_frame_creates_new() {
    crate::test_utils::init_test_tracing();
    let results = runtime_eval_with_usable_terminal(
        "(make-frame)
         (length (frame-list))",
    );
    assert!(results[0].starts_with("OK "));
    assert_eq!(results[1], "OK 2");
}

#[test]
fn make_terminal_frame_creates_tty_child_frame_with_gnu_geometry_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let root_id = ev.frames.create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frames.get_mut(root_id).expect("root frame");
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
        crate::emacs_core::value::list_to_vec(&edges).expect("edges list"),
        vec![
            Value::fixnum(4),
            Value::fixnum(2),
            Value::fixnum(10),
            Value::fixnum(5),
        ]
    );

    let root_minibuffer = ev
        .frames
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let child_frame = ev.frames.get(child_id).expect("child frame");
    assert_eq!(child_frame.minibuffer_window, Some(root_minibuffer));
    assert!(child_frame.minibuffer_leaf.is_none());
    assert_eq!(
        child_frame.parameter("minibuffer"),
        Some(Value::make_window(root_minibuffer.0))
    );

    let hidden_z_order =
        super::builtin_tty_frame_list_z_order(&mut ev, vec![root]).expect("hidden z-order");
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&hidden_z_order).expect("hidden z-order list"),
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
        crate::emacs_core::value::list_to_vec(&visible_z_order).expect("visible z-order list"),
        vec![child, root]
    );

    let hit = super::builtin_tty_frame_at(&mut ev, vec![Value::fixnum(5), Value::fixnum(3)])
        .expect("tty-frame-at");
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&hit).expect("hit list"),
        vec![child, Value::fixnum(1), Value::fixnum(1)]
    );

    let border_hit = super::builtin_tty_frame_at(&mut ev, vec![Value::fixnum(4), Value::fixnum(1)])
        .expect("tty-frame-at border");
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&border_hit).expect("border hit list"),
        vec![child, Value::fixnum(0), Value::fixnum(-1)]
    );
}

#[test]
fn make_terminal_frame_accepts_tty_minibuffer_window_parameter() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let root_id = ev.frames.create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frames.get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frames
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
    let child_frame = ev.frames.get(child_id).expect("child frame");
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
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let root_id = ev.frames.create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frames.get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frames
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
    let child_frame = ev.frames.get(child_id).expect("child frame");
    assert_eq!(child_frame.width, 17);
    assert_eq!(child_frame.height, 4);
    assert_eq!(child_frame.parameter("width"), Some(Value::fixnum(17)));
    assert_eq!(child_frame.parameter("height"), Some(Value::fixnum(4)));
    assert_eq!(
        *child_frame.root_window.bounds(),
        crate::window::Rect::new(0.0, 0.0, 17.0, 4.0)
    );
}

#[test]
fn modify_frame_parameters_accepts_text_pixel_size_on_tty_child_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let root_id = ev.frames.create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frames.get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frames
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
    let child_frame = ev.frames.get(child_id).expect("child frame");
    assert_eq!(child_frame.width, 91);
    assert_eq!(child_frame.height, 18);
    assert_eq!(child_frame.parameter("width"), Some(Value::fixnum(91)));
    assert_eq!(child_frame.parameter("height"), Some(Value::fixnum(18)));
    assert_eq!(
        *child_frame.root_window.bounds(),
        crate::window::Rect::new(0.0, 0.0, 91.0, 18.0)
    );
}

#[test]
fn window_text_pixel_size_uses_supplied_child_window_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let root_id = ev.frames.create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frames.get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frames
        .get(root_id)
        .expect("root frame")
        .minibuffer_window
        .expect("root minibuffer");
    let child_buffer = ev.buffers.create_buffer("*child-measure*");
    ev.buffers
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
        let frame = ev.frames.get_mut(child_id).expect("child frame");
        let child_window = frame.root_window.id();
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
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let root_id = ev.frames.create_frame("F1", 80, 25, scratch);
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    {
        let root = ev.frames.get_mut(root_id).expect("root frame");
        root.char_width = 1.0;
        root.char_height = 1.0;
        root.font_pixel_size = 1.0;
    }

    let root = Value::make_frame(root_id.0);
    let root_minibuffer = ev
        .frames
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
    let child_frame = ev.frames.get(child_id).expect("child frame");
    assert_eq!(child_frame.minibuffer_window, Some(root_minibuffer));
    assert!(child_frame.minibuffer_leaf.is_none());

    assert_eq!(
        ev.frames.find_window_frame_id(root_minibuffer),
        Some(root_id)
    );
    assert_eq!(
        ev.frames.find_valid_window_frame_id(root_minibuffer),
        Some(root_id)
    );
    assert!(ev.frames.lookup_window(root_minibuffer).is_some());
    assert_eq!(
        super::builtin_window_frame(&mut ev, vec![root_minibuffer_value]).expect("window-frame"),
        root
    );
    assert!(
        super::builtin_window_buffer(&mut ev, vec![root_minibuffer_value])
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
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("bootstrap", 800, 600, scratch);
    ev.frames
        .get_mut(fid)
        .expect("bootstrap frame")
        .set_parameter(Value::symbol("window-system"), Value::symbol("x"));

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
    let frame = ev.frames.get(created_id).expect("created frame");
    assert_eq!(ev.frames.frame_list().len(), 2);
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
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev.frames.get_mut(fid).expect("bootstrap frame");
        frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));
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
    assert_eq!(ev.frames.frame_list().len(), 2);
    let frame = ev.frames.get(created_id).expect("created opening frame");
    assert_eq!(frame.parameter("width"), Some(Value::fixnum(80)));
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(25)));
    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, created_id);
    assert_eq!(requests[0].title, LispString::from_utf8("Neomacs"));
    assert_eq!(requests[0].width, frame.width);
    assert_eq!(requests[0].height, frame.height);
    assert_eq!(
        ev.frames.selected_frame().expect("selected frame").id,
        created_id
    );
}

#[test]
fn x_create_frame_fullscreen_maximized_param_reaches_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev.frames.get_mut(fid).expect("bootstrap frame");
        frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));
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
    let frame = ev.frames.get(created_id).expect("created opening frame");
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
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.set_display_identity(crate::window::FrameDisplayIdentity::wayland("wayland-7"));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
        if let Some(mini_leaf) = parent.minibuffer_leaf.as_mut() {
            mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
        }
    }
    ev.frames.select_frame(parent_id);
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
        .frames
        .get(parent_id)
        .and_then(|frame| frame.minibuffer_window)
        .expect("parent minibuffer");
    let child = ev.frames.get(child_id).expect("child frame");

    assert_eq!(requests.borrow().len(), 0);
    assert_eq!(child.parent_frame, Value::make_frame(parent_id.0));
    assert_eq!(
        child.parameter("parent-frame"),
        Some(Value::make_frame(parent_id.0))
    );
    assert_eq!(child.left_pos, 30);
    assert_eq!(child.top_pos, 40);
    assert_eq!(child.width, 200);
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
    let mut ev = Context::new();
    let input = "M-x ifconfig";
    let scratch = ev.buffers.create_buffer("*x-child-marker*");
    ev.buffers.set_current(scratch);
    ev.buffers.insert_into_buffer(scratch, input);
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
    }
    ev.frames.select_frame(parent_id);

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
        let child = ev.frames.get_mut(child_id).expect("child frame");
        crate::window::window_markers::set_window_point_with_marker(
            &mut ev.buffers,
            &mut child.root_window,
            old_end,
        );
    }

    // Backspace at EOB moves GNU's w->pointm marker with the deletion.  A
    // child frame must therefore observe the new EOB before its next
    // redisplay, without waiting for a later `set-window-point` call.
    ev.buffers
        .get_mut(scratch)
        .expect("child buffer")
        .delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            input.len() - 1,
            input.len(),
        ));
    ev.sync_window_positions(scratch);

    let child = ev.frames.get(child_id).expect("child frame");
    let crate::window::Window::Leaf {
        point,
        position_markers,
        ..
    } = &child.root_window
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
    let mut ev = Context::new();
    let input = "M-x ifconfig";
    let minibuffer = ev.buffers.create_buffer(" *Minibuf-vertico-gc*");
    ev.buffers.set_current(minibuffer);
    ev.buffers.insert_into_buffer(minibuffer, input);

    let parent_id = ev
        .frames
        .create_frame("vertico-parent", 960, 640, minibuffer);
    let parent_minibuffer = ev
        .frames
        .get(parent_id)
        .expect("parent frame")
        .selected_window;
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.minibuffer_window = Some(parent_minibuffer);
    }
    ev.frames.select_frame(parent_id);

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
        .frames
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

    ev.buffers
        .get_mut(minibuffer)
        .expect("minibuffer")
        .delete_emacs_byte_range(crate::buffer::EmacsByteRange::from_usize(
            input.len() - 1,
            input.len(),
        ));
    ev.sync_window_positions(minibuffer);

    let child = ev.frames.get(child_id).expect("child frame");
    let crate::window::Window::Leaf { point, .. } = &child.root_window else {
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
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    let parent_font_name = Value::string("-*-Hack-regular-normal-*-*-102-*-*-*-m-0-iso10646-1");
    let mut parent_font_face = crate::face::Face::new("default");
    parent_font_face.family = Some(Value::string("Hack"));
    parent_font_face.weight = Some(FontWeight::NORMAL);
    parent_font_face.slant = Some(FontSlant::Normal);
    parent_font_face.width = Some(FontWidth::Normal);
    parent_font_face.height = Some(crate::face::FaceHeight::Absolute(102));
    let parent_font_object = crate::emacs_core::font::build_font_object(&parent_font_face);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.set_known_parameter(FrameParam::Font, parent_font_name);
        parent.set_parameter(Value::symbol("font-parameter"), parent_font_object);
        parent.char_width = 7.2;
        parent.char_height = 17.0;
        parent.font_pixel_size = 13.0;
    }
    ev.frames.select_frame(parent_id);
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
    let child = ev.frames.get(child_id).expect("child frame");

    assert_eq!(
        child.known_parameter(FrameParam::Font),
        Some(parent_font_name)
    );
    assert_eq!(child.parameter("font-parameter"), Some(parent_font_object));
}

#[test]
fn x_create_frame_with_explicit_font_resolves_internal_font_parameter() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.set_known_parameter(FrameParam::Font, Value::string("JetBrains Mono-9"));
        parent.char_width = 7.2;
        parent.char_height = 17.0;
        parent.font_pixel_size = 13.0;
    }
    ev.frames.select_frame(parent_id);
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
    let child = ev.frames.get(child_id).expect("child frame");

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

#[test]
fn live_default_font_change_on_child_frame_skips_top_level_geometry_hints() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
    }
    ev.frames.select_frame(parent_id);
    let host = RecordingDisplayHost::with_resolved_frame_font(resolved_frame_font(
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
    ));
    let geometry_hints = host.geometry_hints.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let child = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(
        child
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", child)),
    );

    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Noto Sans Mono-16"),
            Value::make_frame(child_id.0),
        ],
    )
    .expect("set child default face font");

    let child_frame = ev.frames.get(child_id).expect("child frame");
    assert_eq!(child_frame.char_width, 13.0);
    assert_eq!(child_frame.char_height, 31.0);
    assert!(
        geometry_hints.borrow().is_empty(),
        "child frames are composited by the parent and must not emit top-level geometry hints"
    );
}

#[test]
fn x_create_frame_accepts_text_pixel_size_on_gui_child_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frames.select_frame(parent_id);
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

    let child = ev.frames.get(child_id).expect("child frame");
    assert_eq!(requests.borrow().len(), 0);
    assert_eq!(child.width, 194);
    assert_eq!(child.height, 82);
    assert_eq!(child.internal_border_width(), 2);
    assert_eq!(
        *child.root_window.bounds(),
        crate::window::Rect::new(2.0, 2.0, 190.0, 78.0)
    );
}

#[test]
fn modify_frame_parameters_resizes_gui_child_frame_text_pixels() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frames.select_frame(parent_id);
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

    let child = ev.frames.get(child_id).expect("child frame");
    assert_eq!(child.width, 194);
    assert_eq!(child.height, 82);
    assert_eq!(child.internal_border_width(), 2);
    assert_eq!(
        *child.root_window.bounds(),
        crate::window::Rect::new(2.0, 2.0, 190.0, 78.0)
    );
}

#[test]
fn modify_frame_parameters_resolves_child_fractional_size_before_position() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(scratch);
    let parent_id = ev.frames.create_frame("parent", 624, 648, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 8.0;
        parent.char_height = 16.0;
        parent.font_pixel_size = 16.0;
    }
    ev.frames.select_frame(parent_id);
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
    let child = super::x_create_frame_impl(
        &mut ev.frames,
        &mut ev.buffers,
        &mut ev.display_host,
        vec![params],
    )
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
    let child_frame = ev.frames.get(child_id).expect("child frame");
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
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("x")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frames.select_frame(parent_id);
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

    assert!(ev.frames.get(child_id).is_none());
    assert_eq!(*removed_child_frames.borrow(), vec![child_id]);
}

#[test]
fn make_frame_invisible_removes_gui_child_overlay_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frames.select_frame(parent_id);
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

    let child = ev.frames.get(child_id).expect("child frame remains live");
    assert_eq!(child.visibility, crate::window::FrameVisibility::Invisible);
    assert_eq!(*removed_child_frames.borrow(), vec![child_id]);
}

#[test]
fn make_frame_visible_shows_gui_child_overlay_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frames.select_frame(parent_id);
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

    let child = ev.frames.get(child_id).expect("child frame remains live");
    assert_eq!(child.visibility, crate::window::FrameVisibility::Visible);
    assert_eq!(*shown_child_frames.borrow(), vec![child_id]);
}

#[test]
fn timer_deferred_make_frame_invisible_removes_gui_child_overlay_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frames.select_frame(parent_id);
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

    ev.eval_str(
        r#"(progn
             (fset 'neomacs-test-hide-frame
                   (lambda (frame) (make-frame-invisible frame)))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install timer handler");
    ev.set_variable(
        "timer-list",
        Value::list(vec![due_gnu_timer(
            Value::symbol("neomacs-test-hide-frame"),
            Value::list(vec![created]),
        )]),
    );

    ev.fire_pending_timers();

    let child = ev.frames.get(child_id).expect("child frame remains live");
    assert_eq!(child.visibility, crate::window::FrameVisibility::Invisible);
    assert_eq!(*removed_child_frames.borrow(), vec![child_id]);
}

#[test]
fn delete_frame_destroys_top_level_gui_window_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let first_id = ev.frames.create_frame("first", 960, 640, scratch);
    let second_id = ev.frames.create_frame("second", 800, 500, scratch);
    for frame_id in [first_id, second_id] {
        ev.frames
            .get_mut(frame_id)
            .expect("gui frame")
            .set_window_system(Some(Value::symbol("x")));
    }
    ev.frames.select_frame(first_id);
    let host = RecordingDisplayHost::new();
    let destroyed_gui_frames = host.destroyed_gui_frames.clone();
    let removed_child_frames = host.removed_child_frames.clone();
    ev.set_display_host(Box::new(host));

    crate::emacs_core::frame::builtin_delete_frame(&mut ev, vec![Value::make_frame(second_id.0)])
        .expect("delete top-level gui frame");

    assert!(ev.frames.get(first_id).is_some());
    assert!(ev.frames.get(second_id).is_none());
    assert_eq!(*destroyed_gui_frames.borrow(), vec![second_id]);
    assert!(removed_child_frames.borrow().is_empty());
}

#[test]
fn delete_parent_frame_cascades_to_gui_child_overlays() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let parent_id = ev.frames.create_frame("parent", 960, 640, scratch);
    let other_id = ev.frames.create_frame("other", 960, 640, scratch);
    {
        let parent = ev.frames.get_mut(parent_id).expect("parent frame");
        parent.set_window_system(Some(Value::symbol("x")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frames.select_frame(parent_id);
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

    assert!(ev.frames.get(parent_id).is_none());
    assert!(ev.frames.get(child_id).is_none());
    assert!(ev.frames.get(other_id).is_some());
    assert_eq!(*removed_child_frames.borrow(), vec![child_id]);
}

#[test]
fn x_create_frame_reserves_tab_bar_space_above_root_window() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("bootstrap", 960, 640, scratch);
    ev.frames
        .get_mut(fid)
        .expect("bootstrap frame")
        .set_parameter(Value::symbol("window-system"), Value::symbol("x"));

    let params = Value::list(vec![
        Value::cons(Value::symbol("name"), Value::string("GUI")),
        Value::cons(Value::symbol("width"), Value::fixnum(80)),
        Value::cons(Value::symbol("height"), Value::fixnum(25)),
        Value::cons(Value::symbol("tab-bar-lines"), Value::fixnum(1)),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");

    let created_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let frame = ev.frames.get(created_id).expect("created frame");

    assert_eq!(frame.tab_bar_height, 16);
    assert_eq!(
        *frame.root_window.bounds(),
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
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev.frames.get_mut(fid).expect("bootstrap frame");
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
    let frame = ev.frames.get(created_id).expect("created opening frame");
    assert_eq!(frame.effective_window_system(), Some(Value::symbol("neo")));
    assert_eq!(frame.width, 800);
    assert_eq!(frame.height, 500);
    assert_eq!(
        ev.frames.selected_frame().expect("selected frame").id,
        created_id
    );

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, created_id);
    assert_eq!(requests[0].width, 800);
    assert_eq!(requests[0].height, 500);
}

#[test]
fn x_create_frame_syncs_pending_resize_before_adopting_opening_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev.frames.get_mut(fid).expect("bootstrap frame");
        frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));
        frame.char_width = 10.0;
        frame.char_height = 20.0;
        if let Some(mini_leaf) = frame.minibuffer_leaf.as_mut() {
            mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
        }
    }
    ev.set_variable("terminal-frame", Value::make_frame(fid.0));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Focus {
        focused: true,
        emacs_frame_id: 0,
    })
    .expect("queue focus");
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 1500,
        height: 1900,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    })
    .expect("queue resize");

    let host = RecordingDisplayHost::new();
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
    let frame = ev.frames.get(created_id).expect("created opening frame");
    assert_eq!(frame.width, 1500);
    assert_eq!(frame.height, 1900);

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, created_id);
    assert_eq!(requests[0].width, 1500);
    assert_eq!(requests[0].height, 1900);
}

#[test]
fn x_create_frame_prefers_display_host_primary_window_size_without_explicit_geometry() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let scratch = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev.frames.get_mut(fid).expect("bootstrap frame");
        frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));
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
    let frame = ev.frames.get(created_id).expect("created opening frame");
    assert_eq!(frame.width, 1500);
    assert_eq!(frame.height, 1900);

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].width, 1500);
    assert_eq!(requests[0].height, 1900);
    assert_eq!(
        ev.frames.selected_frame().expect("selected frame").id,
        created_id
    );
}

#[test]
fn delete_frame_works() {
    crate::test_utils::init_test_tracing();
    let results = runtime_eval_with_usable_terminal(
        "(let ((f2 (make-frame)))
           (delete-frame f2)
           (length (frame-list)))",
    );
    assert_eq!(results[0], "OK 1");
}

#[test]
fn delete_frame_on_dead_frame_object_returns_nil() {
    crate::test_utils::init_test_tracing();
    let results = runtime_eval_with_usable_terminal(
        "(let ((f2 (make-frame)))
           (delete-frame f2)
           (delete-frame f2))",
    );
    assert_eq!(results[0], "OK nil");
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let _primary = ev.frames.create_frame("F1", 800, 600, buf);
    crate::emacs_core::terminal::pure::ensure_terminal_runtime_owner(
        7,
        "tty-7",
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::interactive(
            Some("xterm-256color".to_string()),
            neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256),
        ),
    );
    let secondary = ev.frames.create_frame_on_terminal("F2", 7, 800, 600, buf);
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
            vec![secondary_terminal]
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
    let mut ev = Context::new();
    let frame_id = super::ensure_selected_frame_id(&mut ev);
    ev.frames
        .get_mut(frame_id)
        .expect("selected frame")
        .set_parameter(Value::symbol("window-system"), Value::symbol("x"));

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
    let mut ev = Context::new();
    let fid = super::ensure_selected_frame_id(&mut ev);
    let frame = Value::make_frame(fid.0);

    assert_eq!(
        crate::emacs_core::frame::builtin_framep(&mut ev, vec![frame]).unwrap(),
        Value::T
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_live_p(&mut ev, vec![frame]).unwrap(),
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
        crate::emacs_core::builtins::symbols::builtin_raise_frame(vec![frame]).unwrap(),
        Value::NIL
    );
}

#[test]
fn iconify_top_level_tty_frame_is_a_noop() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
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
    let mut ev = Context::new();
    let fid = super::ensure_selected_frame_id(&mut ev);
    ev.frames
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let primary = ev.frames.create_frame("F1", 800, 600, buf);
    ev.command_loop
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
    let secondary = ev.frames.create_frame_on_terminal("F2", 7, 800, 600, buf);

    assert_eq!(
        crate::emacs_core::frame::builtin_select_frame(
            &mut ev,
            vec![Value::make_frame(secondary.0)]
        )
        .expect("select secondary frame"),
        Value::make_frame(secondary.0)
    );
    assert_eq!(ev.command_loop.keyboard.active_terminal_id(), 7);
    assert_eq!(ev.command_loop.keyboard.input_decode_map(), Value::NIL);

    ev.command_loop
        .keyboard
        .set_input_decode_map(Value::symbol("secondary-map"));

    assert_eq!(
        crate::emacs_core::frame::builtin_select_frame(&mut ev, vec![Value::make_frame(primary.0)])
            .expect("reselect primary frame"),
        Value::make_frame(primary.0)
    );
    assert_eq!(
        ev.command_loop.keyboard.input_decode_map(),
        Value::symbol("primary-map")
    );

    crate::emacs_core::frame::builtin_select_frame(&mut ev, vec![Value::make_frame(secondary.0)])
        .expect("reselect secondary frame");
    assert_eq!(
        ev.command_loop.keyboard.input_decode_map(),
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let primary = ev.frames.create_frame("F1", 800, 600, buf);
    let secondary = ev.frames.create_frame("F2", 800, 600, buf);

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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
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
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_known_parameter(FrameParam::Font, font_object);
    }

    let frame_value = Value::make_frame(fid.0);
    let direct = crate::emacs_core::frame::builtin_frame_parameter(
        &mut ev,
        vec![frame_value, FrameParam::Font.symbol()],
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
fn modify_frame_parameters_buffer_lists_use_gnu_special_storage() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        r#"(let* ((b1 (get-buffer-create "frame-bl-one"))
                  (b2 (get-buffer-create "frame-bl-two"))
                  (dead (get-buffer-create "frame-bl-dead")))
             (kill-buffer dead)
             (modify-frame-parameters
              nil
              (list
               (cons 'buffer-list
                     (cons b2 (cons 'not-a-buffer (cons dead (cons b1 'dotted-tail)))))
               (cons 'buried-buffer-list (list b1 dead b2))))
             (list
              (mapcar #'buffer-name (frame-parameter nil 'buffer-list))
              (mapcar #'buffer-name (frame-parameter nil 'buried-buffer-list))
              (let ((count 0)
                    (tail (frame-parameters)))
                (while tail
                  (if (eq (car (car tail)) 'buffer-list)
                      (setq count (1+ count)))
                  (setq tail (cdr tail)))
                count)
              (let ((count 0)
                    (tail (frame-parameters)))
                (while tail
                  (if (eq (car (car tail)) 'buried-buffer-list)
                      (setq count (1+ count)))
                  (setq tail (cdr tail)))
                count)))"#,
    );
    assert_eq!(
        results[0],
        r#"OK (("frame-bl-two" "frame-bl-one") ("frame-bl-one" "frame-bl-two") 1 1)"#
    );
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
fn window_right_divider_width_only_applies_to_non_rightmost_windows() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_gui_frame(
        "(modify-frame-parameters (selected-frame) '((right-divider-width . 6)))
         (let ((left (selected-window))
               (right (split-window-internal (selected-window) nil 'right nil)))
           (list (window-right-divider-width left)
                 (window-right-divider-width right)))",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK (6 0)");
}

#[test]
fn modify_frame_parameters_top_level_tty_reports_live_width_height() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let (initial_width, initial_height) = {
        let frame = ev.frames.get(fid).expect("frame should exist");
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
    let items = crate::emacs_core::value::list_to_vec(value).expect("result list");
    assert_eq!(items[0], initial_width);
    assert_eq!(items[1], initial_height);
    assert_eq!(items[2], Value::fixnum(7));
    assert_eq!(items[3], Value::fixnum(8));
}

#[test]
fn modify_frame_parameters_width_height_resizes_live_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let host = RecordingDisplayHost::new();
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
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
    assert_eq!(resize_requests[0].width, 664);
    assert_eq!(resize_requests[0].height, 400);
    assert_eq!(
        resize_requests[0].geometry_hints,
        ev.frames
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );

    let frame = ev.frames.get(fid).expect("frame should exist");
    assert_eq!(frame.width, 664);
    assert_eq!(frame.height, 400);
    assert_eq!(frame.parameter("width"), Some(Value::fixnum(80)));
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(25)));
}

#[test]
fn modify_frame_parameters_fullscreen_maximized_updates_live_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let host = RecordingDisplayHost::new();
    let fullscreen_changes = host.fullscreen_changes.clone();
    ev.set_display_host(Box::new(host));
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.char_height = 20.0;
        frame.minibuffer_leaf = None;
    }
    ev.frames.select_frame(fid);

    let metrics = super::current_gui_frame_metrics_in_state(&ev.frames);
    assert_eq!(
        metrics.minibuffer_height, 20.0,
        "a frame's default minibuffer must be one text line (char_height), not two"
    );
}

#[test]
fn modify_frame_parameters_after_live_font_change_defers_gui_resize_until_geometry_query() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let host = RecordingDisplayHost::with_resolved_frame_font(resolved_frame_font(
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
    ));
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
    }

    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Noto Sans Mono-16"),
            Value::make_frame(fid.0),
        ],
    )
    .expect("set live default face font");

    let out = ev
        .eval_str_each("(modify-frame-parameters (selected-frame) '((width . 80) (height . 25)))");
    assert!(
        out[0].is_ok(),
        "modify-frame-parameters failed: {:?}",
        out[0]
    );

    assert!(
        resized.borrow().is_empty(),
        "font-followup resize should stay deferred until geometry query"
    );

    let expected_width = {
        let frame = ev.frames.get(fid).expect("frame should exist");
        assert_eq!(frame.width, 800);
        assert_eq!(frame.height, 600);
        assert_eq!(frame.parameter("width"), Some(Value::fixnum(80)));
        assert_eq!(frame.parameter("height"), Some(Value::fixnum(25)));
        assert_eq!(frame.char_width, 13.0);
        assert_eq!(frame.char_height, 31.0);
        80 * 13 + frame.horizontal_non_text_width()
    };

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Resize {
        width: expected_width as u32,
        height: 775,
        scale_factor: 1.0,
        emacs_frame_id: fid.0,
    })
    .expect("queue resize ack");

    let result = ev
        .eval_str("(list (frame-native-width) (frame-native-height))")
        .expect("geometry query should flush deferred resize");
    let values = crate::emacs_core::value::list_to_vec(&result).expect("result list");
    assert_eq!(
        values,
        vec![Value::fixnum(expected_width), Value::fixnum(775)]
    );

    let resize_requests = resized.borrow();
    assert_eq!(resize_requests.len(), 1);
    assert_eq!(resize_requests[0].frame_id, fid);
    assert_eq!(resize_requests[0].width, expected_width as u32);
    assert_eq!(resize_requests[0].height, 775);
    assert_eq!(
        resize_requests[0].geometry_hints,
        ev.frames
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );
}

#[test]
fn live_default_font_change_updates_gui_geometry_hints() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let host = RecordingDisplayHost::with_resolved_frame_font(resolved_frame_font(
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
    ));
    let geometry_hints = host.geometry_hints.clone();
    ev.set_display_host(Box::new(host));
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
    }

    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Noto Sans Mono-16"),
            Value::make_frame(fid.0),
        ],
    )
    .expect("set live default face font");

    let hints = geometry_hints.borrow();
    assert_eq!(hints.len(), 1);
    assert_eq!(hints[0].0, fid);
    assert_eq!(
        hints[0].1,
        ev.frames
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );
}

#[test]
fn modify_frame_parameters_resize_ignores_window_local_fringes_for_gui_frames() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let host = RecordingDisplayHost::new();
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let selected_window = {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
        frame.selected_window
    };
    assert!(
        ev.frames
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
    assert_eq!(resize_requests[0].width, 664);
    assert_eq!(resize_requests[0].height, 400);
    assert_eq!(
        resize_requests[0].geometry_hints,
        ev.frames
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );
}

#[test]
fn modify_frame_parameters_tab_bar_lines_reflows_root_window_tree() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
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

    let frame = ev.frames.get(fid).expect("frame should exist");
    assert_eq!(frame.tab_bar_height, 20);
    assert_eq!(
        *frame.root_window.bounds(),
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
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

    let frame = ev.frames.get(fid).expect("frame should exist");
    assert_eq!(frame.tool_bar_height, 40);
    assert_eq!(
        *frame.root_window.bounds(),
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
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

    let frame = ev.frames.get(fid).expect("frame should exist");
    assert_eq!(frame.width, 800);
    assert_eq!(frame.height, 600);
    assert_eq!(frame.parameter("width"), None);
    assert_eq!(frame.parameter("height"), None);
    assert_eq!(frame.parameter("neovm--frame-text-lines"), None);
}

#[test]
fn set_frame_size_builtins_resize_live_gui_frames_and_notify_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame should exist");
        frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));
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

    let frame = ev.frames.get(fid).expect("frame should exist");
    assert_eq!(frame.width, 800);
    assert_eq!(frame.height, 600);
    assert_eq!(frame.parameter("width"), None);
    assert_eq!(frame.parameter("height"), None);
    assert_eq!(frame.parameter("neovm--frame-text-lines"), None);

    let requests = resized.borrow();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.frame_id, fid);
    assert_eq!(request.width, 824);
    assert_eq!(request.height, 560);
    assert_eq!(
        request.geometry_hints,
        ev.frames
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );

    drop(requests);

    ev.apply_resize_input_event(824, 560, 1.0, fid.0, false);

    let frame = ev
        .frames
        .get(fid)
        .expect("frame should exist after host ack");
    assert_eq!(frame.width, 824);
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let root_id = ev.frames.create_frame("F1", 624, 648, buf);
    {
        let root = ev.frames.get_mut(root_id).expect("root frame");
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
    let child = super::x_create_frame_impl(
        &mut ev.frames,
        &mut ev.buffers,
        &mut ev.display_host,
        vec![params],
    )
    .expect("x-create-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));
    let child_frame = ev.frames.get(child_id).expect("child frame");
    let root_window_id = child_frame.root_window.id();

    assert!(child_frame.minibuffer_leaf.is_none());
    assert_eq!(child_frame.minibuffer_window, Some(root_window_id));
    let crate::window::Window::Leaf { buffer_id, .. } = &child_frame.root_window else {
        panic!("child frame root must be a leaf window");
    };
    assert!(
        ev.buffers
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let root_id = ev.frames.create_frame("F1", 624, 648, buf);
    {
        let root = ev.frames.get_mut(root_id).expect("root frame");
        root.set_window_system(Some(Value::symbol("neo")));
        root.char_width = 7.2;
        root.char_height = 17.0;
        root.font_pixel_size = 12.0;
    }
    ev.set_display_host(Box::new(RecordingDisplayHost::new()));
    let root_minibuffer = ev
        .frames
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
    let child = super::x_create_frame_impl(
        &mut ev.frames,
        &mut ev.buffers,
        &mut ev.display_host,
        vec![params],
    )
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

    let child_frame = ev.frames.get(child_id).expect("child frame");
    assert_eq!(
        super::frame_text_width_pixels_in_state(&ev.frames, child_id),
        525
    );
    assert_eq!(super::frame_text_height_pixels(child_frame), 374);
    assert_eq!(child_frame.width, 527);
    assert_eq!(child_frame.height, 376);
}

#[test]
fn set_frame_size_and_position_pixelwise_resizes_gui_child_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let root_id = ev.frames.create_frame("F1", 624, 648, buf);
    {
        let root = ev.frames.get_mut(root_id).expect("root frame");
        root.set_window_system(Some(Value::symbol("neo")));
        root.char_width = 7.2;
        root.char_height = 17.0;
        root.font_pixel_size = 12.0;
    }
    ev.set_display_host(Box::new(RecordingDisplayHost::new()));
    let root_minibuffer = ev
        .frames
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
    let child = super::x_create_frame_impl(
        &mut ev.frames,
        &mut ev.buffers,
        &mut ev.display_host,
        vec![params],
    )
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

    let child_frame = ev.frames.get(child_id).expect("child frame");
    assert_eq!(
        super::frame_text_width_pixels_in_state(&ev.frames, child_id),
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
fn set_frame_size_and_position_pixelwise_updates_top_level_tty_native_totals() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let frame = Value::make_frame(ev.frames.selected_frame().expect("selected frame").id.0);

    crate::emacs_core::frame::builtin_set_frame_size_and_position_pixelwise(
        &mut ev,
        vec![
            frame,
            Value::fixnum(123),
            Value::fixnum(45),
            Value::fixnum(17),
            Value::fixnum(19),
        ],
    )
    .expect("set-frame-size-and-position-pixelwise");

    assert_eq!(
        crate::emacs_core::frame::builtin_frame_native_width(&mut ev, vec![frame]).unwrap(),
        Value::fixnum(123)
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_native_height(&mut ev, vec![frame]).unwrap(),
        Value::fixnum(46)
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_total_cols(&mut ev, vec![frame]).unwrap(),
        Value::fixnum(123)
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_total_lines(&mut ev, vec![frame]).unwrap(),
        Value::fixnum(46)
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_text_width(&mut ev, vec![frame]).unwrap(),
        Value::fixnum(123)
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_text_height(&mut ev, vec![frame]).unwrap(),
        Value::fixnum(45)
    );

    let frame_state = ev.frames.selected_frame().expect("selected frame");
    assert_eq!(frame_state.parameter("width"), Some(Value::fixnum(80)));
    assert_eq!(frame_state.parameter("height"), Some(Value::fixnum(25)));
    assert_eq!(frame_state.parameter("left"), Some(Value::fixnum(17)));
    assert_eq!(frame_state.parameter("top"), Some(Value::fixnum(19)));
    assert_eq!(frame_state.left_pos, 0);
    assert_eq!(frame_state.top_pos, 0);
}

#[test]
fn resize_input_preserves_buffer_local_fixed_width_side_window() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let fid = ev
        .frames
        .selected_frame()
        .expect("selected frame should exist")
        .id;
    {
        let frame = ev.frames.get_mut(fid).expect("frame should exist");
        frame.char_width = 10.0;
        frame.char_height = 20.0;
        frame.resize_pixelwise(400, 260);
    }

    ev.eval_str(
        r#"(let ((buf (get-buffer-create "*fixed-side*")))
             (display-buffer-in-side-window
              buf '((side . left) (window-width . 20)))
             (with-current-buffer buf
               (setq-local window-size-fixed 'width)))"#,
    )
    .expect("display fixed side window");

    ev.apply_resize_input_event(800, 260, 1.0, fid.0, false);

    let result = ev
        .eval_str(
            r#"(let ((side (get-buffer-window "*fixed-side*")))
                 (list (window-total-width side)
                       (window-edges side)
                       (mapcar (lambda (w)
                                 (list (buffer-name (window-buffer w))
                                       (window-edges w)))
                               (window-list nil 'no-minibuf nil))))"#,
        )
        .expect("inspect window state");
    assert_eq!(
        format_eval_result(&Ok(result)),
        "OK (20 (0 0 20 12) ((\"*scratch*\" (20 0 80 12)) (\"*fixed-side*\" (0 0 20 12))))"
    );
}

#[test]
fn set_frame_size_syncs_resize_event_before_followup_frame_width_queries() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 1300, 1188, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame should exist");
        frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));
        frame.char_width = 16.0;
        frame.char_height = 33.0;
        frame.set_parameter(Value::symbol("width"), Value::fixnum(79));
        frame.set_parameter(Value::symbol("height"), Value::fixnum(36));
        frame.set_parameter(Value::symbol("neovm--frame-text-lines"), Value::fixnum(35));
    }

    let host = RecordingDisplayHost::new();
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 2144,
        height: 1386,
        scale_factor: 1.0,
        emacs_frame_id: fid.0,
    })
    .expect("queue resize ack");

    let result = ev
        .eval_str(
            "(progn
               (set-frame-size (selected-frame) 132 42)
               (list (frame-total-cols) (frame-native-width)))",
        )
        .expect("set-frame-size and followup queries should succeed");
    let values = crate::emacs_core::value::list_to_vec(&result).expect("result list");
    assert_eq!(values, vec![Value::fixnum(132), Value::fixnum(2144)]);

    let requests = resized.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, fid);
    assert_eq!(requests[0].width, 2144);
    assert_eq!(requests[0].height, 1386);
    assert_eq!(
        requests[0].geometry_hints,
        ev.frames
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );
}

#[test]
fn set_frame_size_keeps_resize_pending_until_geometry_queries_force_sync() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 1300, 1188, buf);
    {
        let frame = ev.frames.get_mut(fid).expect("frame should exist");
        frame.set_parameter(Value::symbol("window-system"), Value::symbol("x"));
        frame.char_width = 16.0;
        frame.char_height = 33.0;
        frame.set_parameter(Value::symbol("width"), Value::fixnum(79));
        frame.set_parameter(Value::symbol("height"), Value::fixnum(36));
        frame.set_parameter(Value::symbol("neovm--frame-text-lines"), Value::fixnum(35));
    }

    let host = RecordingDisplayHost::new();
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));

    let (tx, rx) = crossbeam_channel::unbounded();
    ev.input_rx = Some(rx);
    tx.send(crate::keyboard::InputEvent::Resize {
        width: 2144,
        height: 1386,
        scale_factor: 1.0,
        emacs_frame_id: fid.0,
    })
    .expect("queue resize ack");

    let result = ev
        .eval_str(
            "(progn
               (set-frame-size (selected-frame) 132 42)
               (list (frame-parameter nil 'width) (frame-parameter nil 'height)))",
        )
        .expect("set-frame-size without geometry query should succeed");
    let values = crate::emacs_core::value::list_to_vec(&result).expect("result list");
    assert_eq!(values, vec![Value::fixnum(79), Value::fixnum(36)]);

    let requests = resized.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, fid);
    assert_eq!(requests[0].width, 2144);
    assert_eq!(requests[0].height, 1386);
}

#[test]
fn switch_to_buffer_changes_window() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(get-buffer-create \"other-buf\")
         (switch-to-buffer \"other-buf\")
         (bufferp (window-buffer))",
    );
    assert_eq!(results[2], "OK t");
}

#[test]
fn switch_to_buffer_runs_buffer_list_update_hook_unless_norecord() {
    crate::test_utils::init_test_tracing();
    let result = runtime_eval_one_with_usable_terminal(
        "(let ((stb-log nil))
           (setq buffer-list-update-hook
                 (list (lambda ()
                         (setq stb-log (cons (buffer-name) stb-log)))))
           (let ((norecord (progn (switch-to-buffer \"stb-hook\" t) stb-log)))
             (switch-to-buffer \"*scratch*\" t)
             (setq stb-log nil)
             (let ((recorded (progn (switch-to-buffer \"stb-hook\") stb-log)))
               (list norecord
                     recorded
                     (buffer-name)
                     (buffer-name (window-buffer))))))",
    );
    // Measured on GNU 31.0.90 -Q --batch (tmp/pw61/gnu-more.txt).  The Rust
    // subr ran `buffer-list-update-hook' only for the recording call; GNU runs
    // it from `get-buffer-create' as well (src/buffer.c), so the NORECORD call
    // still logs twice -- under the name that is current at the time, which is
    // still "*scratch*".  DIVERGENCES.md 154: the old expectation was the Rust
    // subr's answer, not GNU's.
    assert_eq!(
        result,
        "OK ((\"*scratch*\" \"*scratch*\") (\"stb-hook\" \"*scratch*\") \"stb-hook\" \"stb-hook\")"
    );
}

#[test]
fn set_window_buffer_works() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(get-buffer-create \"buf2\")
         (set-window-buffer (selected-window) \"buf2\")
         (bufferp (window-buffer))",
    );
    assert_eq!(results[1], "OK nil"); // set-window-buffer returns nil
    assert_eq!(results[2], "OK t");
}

#[test]
fn set_window_buffer_runs_buffer_list_update_hook_for_normal_windows() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        "(let ((swb-log nil)
               (w (selected-window))
               (b (get-buffer-create \"swb-hook-target\")))
           (setq buffer-list-update-hook
                 (list (lambda ()
                         (setq swb-log
                               (cons (list (buffer-name)
                                           (buffer-name (window-buffer w)))
                                     swb-log)))))
           (set-window-buffer w b)
           (list swb-log
                 (buffer-name)
                 (buffer-name (window-buffer w))))",
    );
    assert_eq!(
        result,
        "OK (((\"*scratch*\" \"*scratch*\")) \"*scratch*\" \"swb-hook-target\")"
    );
}

#[test]
fn set_window_buffer_does_not_record_the_displayed_buffer_like_gnu() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        r#"(let ((a (get-buffer-create "swb-order-a"))
                 (b (get-buffer-create "swb-order-b")))
             (unwind-protect
                 (progn
                   (set-window-buffer (selected-window) b)
                   (list (eq (window-buffer (selected-window)) b)
                         (delq nil
                               (mapcar
                                (lambda (buffer)
                                  (and (memq buffer (list a b))
                                       (buffer-name buffer)))
                                (buffer-list)))))
               (kill-buffer a)
               (kill-buffer b)))"#,
    );
    assert_eq!(result, r#"OK (t ("swb-order-a" "swb-order-b"))"#);
}

#[test]
fn set_window_buffer_restores_saved_window_point_and_keep_margins() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(setq swb-test-w (selected-window))
         (setq swb-test-b1 (get-buffer-create \"swb-state-a\"))
         (setq swb-test-b2 (get-buffer-create \"swb-state-b\"))
         (save-current-buffer (set-buffer swb-test-b1)
           (erase-buffer)
           (insert (make-string 300 ?a))
           (goto-char 120))
         (save-current-buffer (set-buffer swb-test-b2)
           (erase-buffer)
           (insert (make-string 300 ?b))
           (goto-char 150))
         (set-window-buffer swb-test-w swb-test-b1)
         (set-window-start swb-test-w 110)
         (set-window-point swb-test-w 120)
         (set-window-margins swb-test-w 3 4)
         (list (window-start swb-test-w)
               (window-point swb-test-w)
               (window-margins swb-test-w))
         (progn
           (set-window-buffer swb-test-w swb-test-b2)
           (list (window-start swb-test-w)
                 (window-point swb-test-w)
                 (window-margins swb-test-w)))
         (progn
           (set-window-margins swb-test-w 7 8)
           (set-window-buffer swb-test-w swb-test-b1 t)
           (list (window-start swb-test-w)
                 (window-point swb-test-w)
                 (window-margins swb-test-w)))
         (progn
           (set-window-margins swb-test-w 9 10)
           (set-window-buffer swb-test-w swb-test-b2 t)
           (list (window-start swb-test-w)
                 (window-point swb-test-w)
                 (window-margins swb-test-w)))
         (progn
           (set-window-margins swb-test-w 11 12)
           (set-window-buffer swb-test-w swb-test-b1 nil)
           (list (window-start swb-test-w)
                 (window-point swb-test-w)
                 (window-margins swb-test-w)))",
    );
    assert_eq!(results[9], "OK (110 120 (3 . 4))");
    assert_eq!(results[10], "OK (1 150 (nil))");
    assert_eq!(results[11], "OK (110 120 (7 . 8))");
    assert_eq!(results[12], "OK (1 150 (9 . 10))");
    assert_eq!(results[13], "OK (110 120 (nil))");
}

#[test]
fn set_window_buffer_updates_history_lists_on_real_buffer_switches() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let* ((w (selected-window))
                (b1 (get-buffer-create \"swb-hist-a\"))
                (b2 (get-buffer-create \"swb-hist-b\"))
                (n '((foo 1 2))))
           (save-current-buffer (set-buffer b1)
             (erase-buffer)
             (insert (make-string 300 ?a)))
           (save-current-buffer (set-buffer b2)
             (erase-buffer)
             (insert (make-string 300 ?b)))
           (set-window-prev-buffers w nil)
           (set-window-next-buffers w nil)
           (set-window-buffer w b1)
           (set-window-start w 7)
           (set-window-point w 11)
           (set-window-next-buffers w n)
           (set-window-buffer w b2)
           (list (null (window-next-buffers w))
                 (mapcar (lambda (e) (buffer-name (car e))) (window-prev-buffers w))
                 (mapcar (lambda (e)
                           (list (markerp (nth 1 e))
                                 (marker-position (nth 1 e))
                                 (markerp (nth 2 e))
                                 (marker-position (nth 2 e))))
                         (window-prev-buffers w))))
         (let* ((w (selected-window))
                (same (window-buffer w))
                (n '((foo 1 2)))
                (before (window-prev-buffers w)))
           (set-window-next-buffers w n)
           (set-window-buffer w same)
           (list (equal (window-prev-buffers w) before)
                 (equal (window-next-buffers w) n)))
         (let* ((w (selected-window))
                (b1 (get-buffer-create \"swb-hist-d1\"))
                (b2 (get-buffer-create \"swb-hist-d2\")))
           (set-window-prev-buffers w nil)
           (set-window-buffer w b1)
           (set-window-buffer w b2)
           (set-window-buffer w b1)
           (set-window-buffer w b2)
           (mapcar (lambda (e) (buffer-name (car e))) (window-prev-buffers w)))",
    );
    assert_eq!(
        results[0],
        "OK (t (\"swb-hist-a\" \"*scratch*\") ((t 7 t 11) (t 1 t 1)))"
    );
    assert_eq!(results[1], "OK (t t)");
    // The buffer just switched to is kept out of the window's previous buffers
    // (GNU `record-window-buffer` semantics), so after d1,d2,d1,d2 the current
    // buffer d2 is absent and only d1 (plus the earlier b) remain.
    assert_eq!(results[2], "OK (\"swb-hist-d1\" \"swb-hist-b\")");
}

#[test]
fn set_window_configuration_preserves_reused_window_history_across_gc() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        r#"(let* ((w (selected-window))
                  (b1 (get-buffer-create "wcfg-next-a"))
                  (b2 (get-buffer-create "wcfg-next-b"))
                  (cfg (current-window-configuration)))
             (set-window-next-buffers w (list b2 b1))
             (garbage-collect)
             (set-window-configuration cfg)
             (mapcar #'buffer-name (window-next-buffers w)))"#,
    );
    assert_eq!(results[0], r#"OK ("wcfg-next-b" "wcfg-next-a")"#);
}

/// GNU `current-window-configuration` deliberately does NOT record point in the
/// buffer that was current when the configuration was saved, so restoring must
/// leave that window's point where the live session put it.
/// `Fset_window_configuration` implements this by recomputing `old_point` from
/// the LIVE state (`src/window.c:7692-7733`) and writing it back over the
/// restored marker after the tree is installed (`src/window.c:7978-7984`).
#[test]
fn set_window_configuration_keeps_live_point_for_the_saved_current_buffer() {
    crate::test_utils::init_test_tracing();
    let results = runtime_eval_with_usable_terminal(
        r#"(save-current-buffer
             (let ((b (get-buffer-create "wcfg-pt-a"))
                   (other (selected-window))
                   conf w)
               (setq w (split-window-internal other nil nil nil))
               (set-window-buffer w b)
               (set-buffer b)
               (erase-buffer) (insert "aaa\nbbb\nccc\nddd\n") (goto-char 1)
               (select-window w)
               (setq conf (current-window-configuration))
               (select-window other)
               (set-window-point w 13)
               (set-buffer b) (goto-char 13)
               (set-window-configuration conf)
               (prog1 (list (window-point w) (progn (set-buffer b) (point)))
                 (select-window other)
                 (delete-window w))))
           (save-current-buffer
             (let ((b (get-buffer-create "wcfg-pt-b"))
                   (c (get-buffer-create "wcfg-pt-c"))
                   (other (selected-window))
                   conf w)
               (setq w (split-window-internal other nil nil nil))
               (set-window-buffer w b)
               (set-buffer b)
               (erase-buffer) (insert "aaa\nbbb\nccc\nddd\n") (goto-char 1)
               (select-window w)
               (set-buffer c)
               (setq conf (current-window-configuration))
               (select-window other)
               (set-window-point w 13)
               (set-buffer b) (goto-char 13)
               (set-window-configuration conf)
               (prog1 (list (window-point w) (progn (set-buffer b) (point)))
                 (select-window other)
                 (delete-window w))))
           (save-current-buffer
             (let ((b (get-buffer-create "wcfg-pt-d"))
                   (other (selected-window))
                   conf w)
               (setq w (split-window-internal other nil nil nil))
               (set-window-buffer w b)
               (set-buffer b)
               (erase-buffer) (insert "aaa\nbbb\nccc\nddd\n") (goto-char 1)
               (select-window w)
               (setq conf (current-window-configuration))
               (goto-char 13)
               (set-window-configuration conf)
               (prog1 (list (window-point w) (progn (set-buffer b) (point)))
                 (select-window other)
                 (delete-window w))))"#,
    );
    // The saved-selected window still shows the buffer that was current when
    // the configuration was saved: its live point survives the restore.
    assert_eq!(results[0], "OK (13 13)");
    // A different buffer was current at save time, so the window's saved point
    // is restored normally.
    assert_eq!(results[1], "OK (1 1)");
    // The saved-selected window is still the selected window at restore time:
    // `old_point` comes from PT, again leaving the live point alone.
    assert_eq!(results[2], "OK (13 13)");
}

#[test]
fn window_end_greater_than_start() {
    crate::test_utils::init_test_tracing();
    // Check that window-end and window-start return valid positions.
    // Use >= since they can be equal for small/empty visible regions.
    let r = eval_one_with_frame(
        "(progn (insert \"hello\\nworld\\n\") (goto-char (point-min)) (list (window-start) (window-end) (>= (window-end) (window-start))))",
    );
    assert!(r.starts_with("OK (1 "), "expected (1 N t), got: {r}");
}

#[test]
fn window_end_reads_the_atomic_record_when_a_snapshot_disagrees() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    ev.buffers
        .get_mut(buf)
        .expect("scratch buffer")
        .insert("hello\nworld\nmore\n");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let wid = ev.frames.get(fid).expect("frame").selected_window;
    let point_max = ev
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_char_pos()
        .get();
    let buffer_z_byte = ev
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_emacs_byte_pos();

    {
        let frame = ev.frames.get_mut(fid).expect("frame");
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
            window_id: wid,
            rows: vec![crate::window::DisplayRowSnapshot {
                row: 0,
                y: 0,
                height: 16,
                start_x: 0,
                start_col: 0,
                end_x: 0,
                end_col: 0,
                start_buffer_pos: Some(crate::buffer::LispCharPos1::new(1)),
                end_buffer_pos: Some(crate::buffer::LispCharPos1::new(12)),
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    ev.buffers
        .get_mut(buf)
        .expect("scratch buffer")
        .insert("hello");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let wid = ev.frames.get(fid).expect("frame").selected_window;
    let point_max = ev
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_char_pos()
        .get()
        .saturating_add(1);

    let buffer_z_byte = ev
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_emacs_byte_pos();
    ev.frames
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
    let mut ev = Context::new();
    ev.set_variable("noninteractive", Value::T);
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    ev.buffers
        .get_mut(buf)
        .expect("scratch buffer")
        .insert(&vec!["line of content"; 50].join("\n"));
    let fid = ev.frames.create_frame("F1", 80, 25, buf);
    let wid = ev.frames.get(fid).expect("frame").selected_window;
    let point_max = ev
        .buffers
        .get(buf)
        .expect("scratch buffer")
        .point_max_char_pos()
        .get();

    {
        let frame = ev.frames.get_mut(fid).expect("frame");
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
    let mut ev = Context::new();
    ev.set_variable("noninteractive", Value::NIL);
    let buf = ev.buffers.create_buffer("*window-end-layout-query*");
    ev.buffers.set_current(buf);
    ev.buffers
        .get_mut(buf)
        .expect("window-end buffer")
        .insert("//! Unicode — repro\nsecond line\nthird line\n");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let wid = ev.frames.get(fid).expect("frame").selected_window;
    ev.frames.get_mut(fid).expect("frame").initial = false;
    let buffer_z_char = LispCharPos1::from_one_based_usize(
        ev.buffers
            .get(buf)
            .expect("window-end buffer")
            .point_max_char_pos()
            .get()
            .saturating_add(1),
    );
    let buffer_z_byte = ev
        .buffers
        .get(buf)
        .expect("window-end buffer")
        .point_max_emacs_byte_pos();
    ev.frames
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
    ev.install_window_layout_query(move |_eval, frame_id, window_id| {
        assert_eq!(frame_id, fid);
        assert_eq!(window_id, wid);
        observed_calls.set(observed_calls.get() + 1);
        crate::window::WindowLayoutQueryOutcome::Ready(crate::window::WindowLayoutQuery::new(
            LispCharPos1::new(17),
            None,
        ))
    });

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
    let mut ev = Context::new();
    ev.set_variable("noninteractive", Value::NIL);
    let buf = ev.buffers.create_buffer("*window-end-layout-failure*");
    ev.buffers.set_current(buf);
    ev.buffers
        .get_mut(buf)
        .expect("buffer")
        .insert("one\ntwo\n");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    ev.frames.get_mut(fid).expect("frame").initial = false;
    ev.install_window_layout_query(|_eval, _frame_id, _window_id| {
        crate::window::WindowLayoutQueryOutcome::Failed(
            crate::window::WindowLayoutQueryFailure::DidNotConverge,
        )
    });

    let error = super::builtin_window_end(&mut ev, vec![Value::NIL, Value::T])
        .expect_err("an installed adapter failure must not return stale retained state");
    let crate::emacs_core::error::Flow::Signal(signal) = error else {
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
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let wid = ev.frames.get(fid).expect("frame").selected_window;

    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.commit_redisplay_cache_for_test(vec![crate::window::WindowDisplaySnapshot {
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
        super::builtin_window_header_line_height(&mut ev, vec![]).expect("header-line height"),
        Value::fixnum(35)
    );
    assert_eq!(
        super::builtin_window_tab_line_height(&mut ev, vec![]).expect("tab-line height"),
        Value::fixnum(34)
    );
}

#[test]
fn window_body_pixel_edges_begin_below_rendered_header_and_tab_lines() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let wid = ev.frames.get(fid).expect("frame").selected_window;

    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(1),
                vec![crate::window::WindowDisplaySnapshot {
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
                        buffer_pos: crate::buffer::LispCharPos1::new(1),
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
    let posn = crate::emacs_core::value::list_to_vec(&posn).expect("position list");
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
fn gnu_lisp_window_edges_use_logical_outer_and_presented_body_regions() {
    crate::test_utils::init_test_tracing();
    for (presentation, scrollbar_left, body_left, body_right) in
        [(1, true, 173.0, 756.0), (2, false, 160.0, 743.0)]
    {
        let mut ev = runtime_startup_context();
        let fid = ev.frames.selected_frame().expect("selected frame").id;
        let wid = ev.frames.get(fid).expect("frame").selected_window;
        let outer = neomacs_display_protocol::types::Rect::new(144.0, 32.0, 640.0, 480.0);
        let body = neomacs_display_protocol::types::Rect::new(
            body_left,
            54.0,
            body_right - body_left,
            438.0,
        );
        let scrollbar = neomacs_display_protocol::types::Rect::new(
            if scrollbar_left { 144.0 } else { 771.0 },
            54.0,
            13.0,
            438.0,
        );
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(presentation),
                vec![crate::window::WindowDisplaySnapshot {
                    window_id: wid,
                    cell_origin: crate::window::geometry::CellOrigin::new(18, 2),
                    regions: crate::window::PresentedWindowRegions {
                        outer,
                        text_body: body,
                        left_margin_columns: 1,
                        right_margin_columns: 2,
                        left_margin: Some(neomacs_display_protocol::types::Rect::new(
                            165.0, 54.0, 8.0, 438.0,
                        )),
                        right_margin: Some(neomacs_display_protocol::types::Rect::new(
                            body_right, 54.0, 16.0, 438.0,
                        )),
                        left_fringe: Some(neomacs_display_protocol::types::Rect::new(
                            if scrollbar_left { 157.0 } else { 144.0 },
                            54.0,
                            8.0,
                            438.0,
                        )),
                        right_fringe: Some(neomacs_display_protocol::types::Rect::new(
                            body_right + 16.0,
                            54.0,
                            12.0,
                            438.0,
                        )),
                        left_scroll_bar: scrollbar_left.then_some(scrollbar),
                        right_scroll_bar: (!scrollbar_left).then_some(scrollbar),
                        tab_line: Some(neomacs_display_protocol::types::Rect::new(
                            144.0, 32.0, 640.0, 17.0,
                        )),
                        header_line: Some(neomacs_display_protocol::types::Rect::new(
                            144.0, 49.0, 640.0, 5.0,
                        )),
                        mode_line: Some(neomacs_display_protocol::types::Rect::new(
                            144.0, 492.0, 640.0, 20.0,
                        )),
                        right_divider: Some(neomacs_display_protocol::types::Rect::new(
                            782.0, 32.0, 2.0, 480.0,
                        )),
                        bottom_divider: Some(neomacs_display_protocol::types::Rect::new(
                            144.0, 510.0, 640.0, 2.0,
                        )),
                        ..Default::default()
                    },
                    regions_materialized: true,
                    header_line_height: 99,
                    tab_line_height: 99,
                    mode_line_height: 99,
                    text_area_left_offset: 999,
                    points: vec![crate::window::DisplayPointSnapshot {
                        role: crate::window::DisplayPointRole::Glyph,
                        buffer_pos: crate::buffer::LispCharPos1::ONE,
                        x: 72,
                        y: 999,
                        width: 7,
                        height: 17,
                        row: 99,
                        col: 9,
                    }],
                    body_rows: vec![crate::window::PresentedBodyRowSnapshot {
                        output_row: 99,
                        body_row: 2,
                        body_y: 34,
                    }],
                    ..Default::default()
                }],
            )
            .expect("presented GUI geometry");
        let logical_window = frame.find_window_mut(wid).expect("live window");
        logical_window.set_bounds(crate::window::Rect::new(0.0, 0.0, 80.0, 24.0));
        logical_window.set_left_col(18);
        logical_window.set_top_line(2);
        let display = frame
            .find_window_mut(wid)
            .expect("live window")
            .display_mut()
            .expect("leaf display state");
        display.scroll_bar_width = 13;
        display.vertical_scroll_bar_type =
            Value::symbol(if scrollbar_left { "left" } else { "right" });
        display.horizontal_scroll_bar_type = Value::NIL;

        let result = ev
            .eval_str(
                "(list (window-pixel-left)
                       (window-pixel-top)
                       (window-left-column)
                       (window-top-line)
                       (window-pixel-edges)
                       (window-inside-pixel-edges)
                       (window-inside-edges)
                       (window-body-width)
                       (window-text-width)
                       (window-body-height)
                       (window-text-height)
                       (window-text-width nil t)
                       (window-text-height nil t)
                       (posn-at-point 1))",
            )
            .expect("GNU window.el geometry query");
        assert_eq!(
            crate::emacs_core::print::print_value(&result),
            format!(
                "(0 0 18 2 (0 0 80 24) ({} 22 {} 460) ({} 1 {} 29) 72 72 27 27 {} 438 (#<window {}> 1 (72 . 34) 0 nil 1 (9 . 2) nil (0 . 0) (7 . 17)))",
                (body_left - outer.x) as i64,
                (body_right - outer.x) as i64,
                ((body_left - outer.x) / 8.0).floor() as i64,
                ((body_right - outer.x) / 8.0).ceil() as i64,
                (body_right - body_left) as i64,
                wid.0,
            )
        );
    }
}

#[test]
fn gui_geometry_queries_distinguish_logical_layout_from_presented_output() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer = ev.buffers.create_buffer("*geometry-invariant*");
    ev.buffers.set_current(buffer);
    let frame_id = ev.frames.create_frame("gui", 800, 600, buffer);
    let window_id = ev.frames.get(frame_id).expect("frame").selected_window;
    let frame = ev.frames.get_mut(frame_id).expect("frame");
    frame.set_window_system(Some(Value::symbol("neo")));
    frame
        .find_window_mut(window_id)
        .expect("window")
        .set_bounds(crate::window::Rect::new(901.0, 902.0, 903.0, 904.0));

    assert_eq!(
        super::builtin_window_pixel_width(&mut ev, vec![])
            .expect("GNU exposes the synchronous logical pixel width")
            .as_int(),
        Some(903),
        "window-pixel-width reads the window's current layout before first presentation"
    );
    assert_eq!(
        super::builtin_window_pixel_height(&mut ev, vec![])
            .expect("GNU exposes the synchronous logical pixel height")
            .as_int(),
        Some(904),
        "window-pixel-height reads the window's current layout before first presentation"
    );
    assert_eq!(
        super::builtin_window_pixel_left(&mut ev, vec![])
            .expect("GNU exposes the synchronous logical pixel origin")
            .as_int(),
        Some(901)
    );
    assert_eq!(
        super::builtin_window_pixel_top(&mut ev, vec![])
            .expect("GNU exposes the synchronous logical pixel origin")
            .as_int(),
        Some(902)
    );
    assert_eq!(
        super::builtin_window_body_height(&mut ev, vec![Value::NIL, Value::T])
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
fn winner_can_capture_a_graphical_startup_layout_before_first_presentation() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    ev.eval_str("(require 'winner)")
        .expect("load Winner before the synthetic frame becomes graphical");
    let frame_id = ev.frames.selected_frame().expect("selected frame").id;
    ev.frames
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));
    assert!(
        ev.frames
            .get(frame_id)
            .expect("frame")
            .active_presentation_geometry()
            .is_none(),
        "the regression requires Winner to run before the first redisplay publication"
    );

    let result = ev.eval_str(
        "(progn
           (winner-mode 1)
           (winner-save-old-configurations)
           (window-edges))",
    );
    assert!(
        result.is_ok(),
        "Winner startup must not require rendered geometry: {}",
        format_eval_result(&result)
    );
}

#[test]
fn presented_fringe_geometry_and_scrollbar_tuple_keep_live_gnu_configuration() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer = ev.buffers.create_buffer("*geometry-tuples*");
    ev.buffers.set_current(buffer);
    let frame_id = ev.frames.create_frame("gui", 800, 600, buffer);
    let window_id = ev.frames.get(frame_id).expect("frame").selected_window;
    {
        let frame = ev.frames.get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    assert!(
        ev.frames
            .set_window_fringes(window_id, Some(8), Some(0), true, true)
    );
    assert!(ev.frames.set_window_scroll_bars(
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
            .frames
            .get_mut(frame_id)
            .expect("frame")
            .find_window_mut(window_id)
            .expect("window")
            .display_mut()
            .expect("leaf display state");
        display.vertical_scroll_bar_type = Value::symbol("left");
        display.horizontal_scroll_bar_type = Value::symbol("bottom");
    }
    ev.frames
        .get_mut(frame_id)
        .expect("frame")
        .prepare_and_activate_display_presentation_for_test(
            crate::window::geometry::PresentationId::new(1),
            vec![crate::window::WindowDisplaySnapshot {
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

    let fringes = super::builtin_window_fringes(&mut ev, vec![]).expect("fringes");
    let scrollbars = super::builtin_window_scroll_bars(&mut ev, vec![]).expect("scrollbars");
    assert_eq!(crate::emacs_core::print::print_value(&fringes), "(8 0 t t)");
    assert_eq!(
        crate::emacs_core::print::print_value(&scrollbars),
        "(0 0 left 0 0 bottom t)"
    );

    {
        let display = ev
            .frames
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
    let inherited = super::builtin_window_scroll_bars(&mut ev, vec![]).expect("scrollbars");
    assert_eq!(
        crate::emacs_core::print::print_value(&inherited),
        "(nil 1 t nil 0 t nil)",
        "realized rectangles must not replace GNU's raw inherited tuple fields"
    );
}

#[test]
fn display_buffer_returns_window() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(get-buffer-create \"disp-buf\")
         (windowp (display-buffer \"disp-buf\"))",
    );
    assert_eq!(results[1], "OK t");
}

#[test]
fn pop_to_buffer_returns_buffer() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(get-buffer-create \"pop-buf\")
         (bufferp (pop-to-buffer \"pop-buf\"))",
    );
    assert_eq!(results[1], "OK t");
}

#[test]
fn switch_display_pop_bootstrap_initial_frame() {
    crate::test_utils::init_test_tracing();
    let out = bootstrap_eval_with_frame(
        "(save-current-buffer (bufferp (switch-to-buffer \"*scratch*\")))
         (save-current-buffer (windowp (display-buffer \"*scratch*\")))
         (save-current-buffer (bufferp (pop-to-buffer \"*scratch*\")))",
    );
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK t");
    assert_eq!(out[2], "OK t");
}

#[test]
fn switch_display_pop_enforce_max_arity() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (switch-to-buffer \"*scratch*\" nil nil nil) (error (car err)))
         (condition-case err (display-buffer \"*scratch*\" nil nil nil) (error (car err)))
         (condition-case err (pop-to-buffer \"*scratch*\" nil nil nil) (error (car err)))
         (condition-case err (set-window-buffer (selected-window) \"*scratch*\" nil nil) (error (car err)))",
    );
    assert_eq!(results[0], "OK wrong-number-of-arguments");
    assert_eq!(results[1], "OK wrong-number-of-arguments");
    assert_eq!(results[2], "OK wrong-number-of-arguments");
    assert_eq!(results[3], "OK wrong-number-of-arguments");
}

#[test]
fn switch_display_pop_reject_non_buffer_designators() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (switch-to-buffer 1) (error (list (car err) (nth 1 err) (nth 2 err))))
         (condition-case err (display-buffer 1) (error (list (car err) (nth 1 err) (nth 2 err))))
         (condition-case err (pop-to-buffer 1) (error (list (car err) (nth 1 err) (nth 2 err))))
         (condition-case err (set-window-buffer (selected-window) 1) (error (list (car err) (nth 1 err) (nth 2 err))))",
    );
    assert_eq!(results[0], "OK (wrong-type-argument stringp 1)");
    assert_eq!(results[1], "OK (wrong-type-argument stringp 1)");
    assert_eq!(results[2], "OK (wrong-type-argument stringp 1)");
    assert_eq!(results[3], "OK (wrong-type-argument stringp 1)");
}

#[test]
fn switch_and_pop_create_missing_named_buffers() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(save-current-buffer (bufferp (switch-to-buffer \"sw-auto-create\")))
         (buffer-live-p (get-buffer \"sw-auto-create\"))
         (kill-buffer \"sw-auto-create\")
         (save-current-buffer (bufferp (pop-to-buffer \"pop-auto-create\")))
         (buffer-live-p (get-buffer \"pop-auto-create\"))
         (kill-buffer \"pop-auto-create\")",
    );
    assert_eq!(results[0], "OK t");
    assert_eq!(results[1], "OK t");
    assert_eq!(results[2], "OK t");
    assert_eq!(results[3], "OK t");
    assert_eq!(results[4], "OK t");
    assert_eq!(results[5], "OK t");
}

#[test]
fn display_buffer_missing_or_dead_signals_invalid_buffer() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval_with_frame(
        "(condition-case err (display-buffer \"db-missing\") (error err))
         (let ((b (get-buffer-create \"db-dead\")))
           (kill-buffer b)
           (condition-case err (display-buffer b) (error err)))",
    );
    assert_eq!(results[0], "OK (error \"Invalid buffer\")");
    assert_eq!(results[1], "OK (error \"Invalid buffer\")");
}

#[test]
fn set_window_buffer_matches_window_and_buffer_designator_errors() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.frames.create_frame("F1", 800, 600, buf);
    let dead = Value::make_buffer(ev.buffers.create_buffer("swb-dead"));
    ev.set_variable("vm-swb-dead", dead);
    let results = ev
        .eval_str_each(
            "(condition-case err (set-window-buffer nil \"*scratch*\") (error err))
         (condition-case err (set-window-buffer nil \"swb-missing\") (error err))
         (progn
           (kill-buffer vm-swb-dead)
           (condition-case err (set-window-buffer nil vm-swb-dead) (error err)))
         (condition-case err (set-window-buffer 999999 \"*scratch*\") (error err))
         (condition-case err (set-window-buffer 'foo \"*scratch*\") (error err))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK (wrong-type-argument bufferp nil)");
    assert_eq!(
        results[2],
        "OK (error \"Attempt to display deleted buffer\")"
    );
    assert_eq!(results[3], "OK (wrong-type-argument window-live-p 999999)");
    assert_eq!(results[4], "OK (wrong-type-argument window-live-p foo)");
}

#[test]
fn set_window_buffer_bootstraps_initial_frame_for_nil_window_designator() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
            "(condition-case err
             (let ((b (get-buffer-create \"swb-bootstrap\")))
               (set-buffer b)
               (erase-buffer)
               (insert \"abcdef\")
               (goto-char 1)
               (set-window-buffer nil b)
               (list (buffer-name (window-buffer nil))
                     (window-start nil)
                     (window-end nil)))
           (error err))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK (\"swb-bootstrap\" 1 7)");
}

#[test]
fn scroll_and_recenter_use_selected_window_state() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((w (selected-window)))
           (save-current-buffer (set-buffer (window-buffer w))
             (erase-buffer)
             (insert \"a\nb\nc\nd\ne\nf\ng\nh\n\"))
           (set-window-point w 1)
           (list (condition-case err
                     (progn (scroll-up 2) (scroll-down 1) (window-point w))
                   (error (car err)))
                 (progn (scroll-left 3) (window-hscroll w))
                 (progn (scroll-right 1) (window-hscroll w))
                 (progn (set-window-point w 9) (recenter 1) (window-start w))))",
    );
    // GNU `window_scroll_line_based` performs the second scroll from the
    // start forced by the first one. Point remains visible, so scrolling back
    // one line succeeds instead of spuriously recentering at point-min.
    // (Real-frame behavior: `eval_with_frame` makes a NON-initial frame, where
    // `pos_visible_p` is geometric. On the --batch INITIAL frame GNU answers
    // `beginning-of-buffer` — see `vm_scroll_and_recenter_builtins_use_shared_window_state`.)
    assert_eq!(results[0], "OK (5 3 2 7)");
}

#[test]
fn recenter_uses_current_buffer_point_for_selected_window() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    ev.eval_str_each("(erase-buffer) (insert \"a\\nb\\nc\\nd\\ne\\nf\\ng\\nh\\n\") (goto-char 7)");
    let wid = ev.frames.get(fid).expect("frame").selected_window;
    if let Some(crate::window::Window::Leaf { point, .. }) = ev
        .frames
        .get_mut(fid)
        .and_then(|frame| frame.find_window_mut(wid))
    {
        *point = LispCharPos1::ONE;
    }
    let results = ev
        .eval_str_each(
            "(progn
               (recenter 0)
               (list (window-start)
                     (line-number-at-pos (window-start))
                     (line-number-at-pos (point))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(results[0], "OK (7 4 4)");
}

#[test]
fn only_full_frame_recenter_crosses_the_menu_bar_rebuild_boundary() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer = ev.buffers.create_buffer("recenter-menu-boundary");
    ev.buffers.set_current(buffer);
    ev.frames.create_frame("F1", 800, 600, buffer);
    ev.eval_str("(insert \"a\\nb\\nc\\nd\\n\") (goto-char 3)")
        .expect("prepare recenter buffer");

    let initial = ev.menu_bar_rebuild_generation();
    ev.eval_str("(recenter 0 t)")
        .expect("numeric recenter ignores redraw request");
    assert_eq!(ev.menu_bar_rebuild_generation(), initial);

    ev.eval_str("(let ((recenter-redisplay nil)) (recenter nil t))")
        .expect("disabled recenter redraw policy");
    assert_eq!(ev.menu_bar_rebuild_generation(), initial);

    ev.eval_str("(let ((recenter-redisplay t)) (recenter nil t))")
        .expect("full-frame recenter");
    assert_ne!(ev.menu_bar_rebuild_generation(), initial);
}

#[test]
fn deleting_a_window_crosses_the_menu_bar_rebuild_boundary() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer = ev.buffers.create_buffer("delete-window-menu-boundary");
    ev.buffers.set_current(buffer);
    let frame_id = ev.frames.create_frame("F1", 800, 600, buffer);
    ev.eval_str("(split-window-internal (selected-window) nil nil nil)")
        .expect("split the selected window");
    let deleted_window = ev
        .frames
        .get(frame_id)
        .expect("frame")
        .window_list()
        .into_iter()
        .nth(1)
        .expect("second live window");

    let initial = ev.menu_bar_rebuild_generation();
    super::builtin_delete_window_internal(&mut ev, vec![super::window_value(deleted_window)])
        .expect("delete second window");

    assert_ne!(ev.menu_bar_rebuild_generation(), initial);
}

#[test]
fn only_nonselected_window_redisplay_crosses_the_menu_bar_rebuild_boundary() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer = ev.buffers.create_buffer("window-redisplay-menu-boundary");
    ev.buffers.set_current(buffer);
    let frame_id = ev.frames.create_frame("F1", 800, 600, buffer);
    ev.eval_str("(split-window-internal (selected-window) nil nil nil)")
        .expect("split the selected window");
    let windows = ev.frames.get(frame_id).expect("frame").window_list();
    let selected = ev.frames.get(frame_id).expect("frame").selected_window;
    let nonselected = windows
        .into_iter()
        .find(|window| *window != selected)
        .expect("nonselected live window");

    let initial = ev.menu_bar_rebuild_generation();
    ev.mark_chrome_dirty_window(selected);
    assert_eq!(ev.menu_bar_rebuild_generation(), initial);

    ev.mark_chrome_dirty_window(nonselected);
    assert_ne!(ev.menu_bar_rebuild_generation(), initial);
}

/// `recenter` walks backward `target_line` screen lines from point. Exercises
/// the multi-line backward walk and the begv clamp -- the paths the 0-line /
/// 1-line cases above miss.
#[test]
fn recenter_backward_multiple_lines_scans_bytes_to_window_start() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((w (selected-window)))
           (save-current-buffer (set-buffer (window-buffer w))
             (erase-buffer)
             (insert \"l0\nl1\nl2\nl3\nl4\nl5\nl6\nl7\nl8\nl9\n\"))
           (list
             ;; point on line 8 (l7); recenter 5 -> window-start at line 3 (l2)
             (progn (goto-char (point-min)) (forward-line 7) (recenter 5)
                    (line-number-at-pos (window-start)))
             ;; point on line 2 (l1); only one line precedes it, so recenter 5
             ;; clamps the backward walk at the buffer start -> line 1
             (progn (goto-char (point-min)) (forward-line 1) (recenter 5)
                    (line-number-at-pos (window-start)))))",
    );
    assert_eq!(results[0], "OK (3 1)");
}

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
#[test]
fn recenter_counts_screen_lines_not_buffer_lines_over_invisible_text() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((w (selected-window)))
           (save-current-buffer (set-buffer (window-buffer w))
             (erase-buffer)
             (insert \"l1\\nl2\\nl3\\nl4\\nl5\\nl6\\nl7\\nl8\\nl9\\nl10\\nl11\\nl12\\n\"))
           (list
             ;; Nothing hidden: 4 screen lines back from line 9 is line 5.
             (progn (goto-char (point-min)) (forward-line 8) (recenter 4)
                    (line-number-at-pos (window-start)))
             ;; Hide line 6 entirely. It is no longer a screen line, so the
             ;; same 4 screen lines back reach one line further, to line 4.
             (progn
               (let ((overlay (make-overlay
                               (save-excursion (goto-char (point-min))
                                               (forward-line 5) (point))
                               (save-excursion (goto-char (point-min))
                                               (forward-line 6) (point)))))
                 (overlay-put overlay 'invisible t))
               (goto-char (point-min)) (forward-line 8) (recenter 4)
               (line-number-at-pos (window-start)))))",
    );
    assert_eq!(results[0], "OK (5 4)");
}

#[test]
fn scroll_up_down_updates_window_start_for_multibyte_content() {
    crate::test_utils::init_test_tracing();
    // dotimes is no longer a special form; use let+while equivalent
    let results = eval_with_frame(
        "(let ((w (selected-window)))
           (save-current-buffer (set-buffer (window-buffer w))
             (erase-buffer)
             (let ((i 0))
               (while (< i 120)
                 (insert (format \"L%03d — multibyte scrolling line\\n\" i))
                 (setq i (1+ i)))))
           (set-window-point w 1)
           (set-window-start w 1)
           (let ((before (window-start w)))
               (scroll-up 10)
               (let ((after-up (window-start w)))
                 (list (= before 1)
                       (> after-up before)
                       (condition-case err
                           (progn
                             (scroll-down 5)
                             (list :ok
                                   (< (window-start w) after-up)
                                   (= (window-start w) (window-point w))))
                         (error (list :err
                                      (car err)
                                      (window-start w)
                                      (window-point w))))))))",
    );
    assert_eq!(results[0], "OK (t t (:ok t nil))");
}

/// Multiple wheel events can dispatch their `scroll-up` commands before the
/// asynchronous GUI redisplay publishes a fresh window-end.  GNU keeps using
/// the explicit window-start in that interval, so same-direction commands
/// must advance monotonically rather than recentering around point.
#[test]
fn consecutive_scroll_up_before_redisplay_advances_from_window_start() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*queued-scroll*");
    ev.buffers.set_current(buffer_id);
    ev.buffers
        .insert_into_buffer(buffer_id, &"line\n".repeat(200));
    let frame_id = ev.frames.create_frame("F1", 800, 600, buffer_id);
    let window_id = ev
        .frames
        .get(frame_id)
        .expect("scroll frame")
        .selected_window;

    // The last redisplay showed line 50 at the top with point on line 51.
    // Each line occupies five chars/bytes, including its newline.
    let start = LispCharPos1::new(246);
    let point = LispCharPos1::new(251);
    let displayed_end = LispCharPos1::new(421);
    ev.buffers
        .goto_buffer_emacs_byte_pos(buffer_id, EmacsBytePos::new(250));
    let (buffer_z_char, buffer_z_byte) = {
        let buffer = ev.buffers.get(buffer_id).expect("scroll buffer");
        (
            buffer.point_max_char_pos().to_lisp(),
            buffer.point_max_emacs_byte_pos(),
        )
    };
    let window = ev
        .frames
        .get_mut(frame_id)
        .expect("scroll frame")
        .find_window_mut(window_id)
        .expect("scroll window");
    crate::window::window_markers::set_window_start_with_marker(&mut ev.buffers, window, start);
    crate::window::window_markers::set_window_point_with_marker(&mut ev.buffers, window, point);
    window.set_window_end_from_positions(
        buffer_z_char,
        buffer_z_byte,
        displayed_end,
        EmacsBytePos::new(420),
        34,
    );

    let result = ev
        .eval_str_each(
            "(let ((w (selected-window)))
               (scroll-up 1)
               (let ((after-first (line-number-at-pos (window-start w))))
                 (scroll-up 1)
                 (list after-first
                       (line-number-at-pos (window-start w)))))",
        )
        .into_iter()
        .map(|result| format_eval_result(&result))
        .collect::<Vec<_>>();

    assert_eq!(result, ["OK (51 52)"]);
}

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
#[test]
fn scroll_down_from_eob_pulls_point_to_last_visible_line_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((w (selected-window)))
           (save-current-buffer (set-buffer (window-buffer w))
             (erase-buffer)
             (let ((i 0))
               (while (< i 200)
                 (insert (format \"line %03d\\n\" i))
                 (setq i (1+ i)))))
           (goto-char (point-max))
           (set-window-point w (point-max))
           (list (window-body-height w)
                 next-screen-context-lines
                 (condition-case err
                     (progn
                       (scroll-down nil)
                       (list :ok
                           (line-number-at-pos (window-start w))
                           (line-number-at-pos (window-point w))
                           (window-start w)
                           (window-point w)))
                   (error (list :err (car err))))))",
    );
    assert_eq!(results[0], "OK (35 2 (:ok 151 185 1351 1657))");
}

/// A near-full-screen scroll is measured in displayed screen rows, not raw
/// buffer newlines.  This matters for folded Org subtrees: invisible logical
/// lines occupy no part of the window and therefore cannot consume a page.
#[test]
fn scroll_down_skips_invisible_logical_lines_when_measuring_a_page() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let ((w (selected-window)))
           (save-current-buffer (set-buffer (window-buffer w))
             (erase-buffer)
             (let ((i 1))
               (while (<= i 80)
                 (insert (format \"v%02d\\n\" i))
                 (setq i (1+ i))))
             (goto-char (point-min))
             (forward-line 20)
             (setq buffer-invisibility-spec '((fold . t)))
             (let ((hidden-beg (point)))
               (forward-line 40)
               (put-text-property hidden-beg (point) 'invisible 'fold))
             (goto-char (point-min))
             (forward-line 69))
           (set-window-point w (point))
           (set-window-start w (point))
           (scroll-down nil)
           (list (window-body-height w)
                 next-screen-context-lines
                 (line-number-at-pos (window-start w))
                 (line-number-at-pos (point))
                 (invisible-p (point))))",
    );
    assert_eq!(results[0], "OK (35 2 1 70 nil)");
}

/// Page-up near the end of a large buffer must not restart display scanning at
/// `point-min` for every requested screen row.  That made one `C-b` in a
/// 149-KiB Org journal execute roughly `page_height * buffer_size` display
/// iterator steps and block the command loop for about 25 seconds.
#[test]
fn scroll_down_page_scans_buffer_once_not_once_per_screen_row() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buffer_id = ev.buffers.create_buffer("*scroll-complexity*");
    ev.buffers.set_current(buffer_id);
    let frame_id = ev
        .frames
        .create_frame("scroll-complexity", 800, 600, buffer_id);
    ev.buffers
        .insert_into_buffer(buffer_id, &"some journal text on this line\n".repeat(400));
    let end = ev
        .buffers
        .get(buffer_id)
        .expect("scroll buffer")
        .emacs_byte_pos_to_lisp_char_pos(
            ev.buffers
                .get(buffer_id)
                .expect("scroll buffer")
                .accessible_emacs_byte_region()
                .end(),
        );
    let window_id = ev
        .frames
        .get(frame_id)
        .expect("scroll frame")
        .selected_window;
    let window = ev
        .frames
        .get_mut(frame_id)
        .expect("scroll frame")
        .find_window_mut(window_id)
        .expect("scroll window");
    crate::window::window_markers::set_window_point_with_marker(&mut ev.buffers, window, end);
    crate::window::window_markers::set_window_start_with_marker(&mut ev.buffers, window, end);

    crate::emacs_core::indent::reset_screen_line_scan_steps_for_test();
    crate::emacs_core::indent::reset_display_stop_recomputes_for_test();
    super::builtin_scroll_down(&mut ev, vec![Value::NIL]).expect("page up from end");
    let after_start = super::builtin_window_start(&mut ev, vec![Value::make_window(window_id.0)])
        .expect("window-start after page up")
        .as_fixnum()
        .expect("window-start is an integer");
    let scan_steps = crate::emacs_core::indent::screen_line_scan_steps_for_test();
    let buffer_bytes = ev
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
    let recomputes = crate::emacs_core::indent::display_stop_recomputes_for_test();
    assert!(
        recomputes.saturating_mul(4) < scan_steps,
        "display-stop cache did not coalesce property probes: {recomputes} stop recomputes \
         for {scan_steps} per-char scan steps (expected far fewer recomputes)"
    );
}

/// Reproduces the observable bug reported after `C-x 2` in an
/// interactive `neomacs -nw -Q` session: the cursor ends up on the
/// *bottom* (newly-created) window, and both mode lines render in
/// their active face.
///
/// GNU Emacs behavior (verified against `emacs -Q --batch` with
/// 31.0.50 on 2026-04-09):
///
///   BEFORE: selected = #<window 1 on *scratch*>
///   split-window-below returns #<window 4 on *scratch*>
///   AFTER : selected = #<window 1 on *scratch*>          ;; UNCHANGED
///   (eq new-window (selected-window)) = nil
///
/// The selected window must remain the ORIGINAL (top) window.
/// Only one window at a time owns the active `mode-line` face;
/// every other window uses `mode-line-inactive`. Matching GNU
/// semantics is critical for visual focus cues.
#[test]
fn split_window_below_keeps_selected_window_on_top_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let out = ev
        .eval_str_each(
            "(let ((before (selected-window)))
               (let ((new-window (split-window-below)))
                 (list
                  ;; Selected window after split is still the ORIGINAL.
                  (eq (selected-window) before)
                  ;; `split-window-below` returns the new window.
                  (windowp new-window)
                  ;; The new window is NOT the selected window.
                  (not (eq new-window (selected-window)))
                  ;; Both windows show up in window-list.
                  (= (length (window-list)) 2))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        out[0], "OK (t t t t)",
        "split-window-below must keep the original window selected, matching GNU"
    );
}

/// Second-layer verification that complements
/// `split_window_below_keeps_selected_window_on_top_like_gnu`:
/// checks the raw `Frame::selected_window` / leaf tree invariant
/// at the `FrameManager` layer, matching what
/// `collect_layout_params` in neomacs-layout-engine reads when
/// deciding which window gets the active `mode-line` face vs
/// `mode-line-inactive`.
///
/// The visible bug is: after `C-x 2` in an interactive `neomacs
/// -nw -Q` session, BOTH mode lines render with
/// `mode-line-inactive` colors. GNU Emacs's mode-line face is
/// chosen by `frame->selected_window == window`
/// (`src/xdisp.c::display_mode_line`), so the `Rust` analog is
/// `frame.selected_window == win_id` at layout time. This test
/// pins the contract that:
///
///   1. Exactly one leaf has `id == frame.selected_window`.
///   2. That leaf is the ORIGINAL window, not the newly split
///      sibling.
///   3. `frame.selected_window` is a live leaf id, not a stale
///      handle or an internal-node id.
#[test]
fn split_window_below_keeps_frame_selected_window_on_top_leaf() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();

    // Create a frame with a real buffer, mirroring what the
    // other runtime-startup tests in this file do.
    let scratch = ev.buffers.create_buffer("*m-x-target*");
    ev.buffers.set_current(scratch);
    let frame_id = ev.frames.create_frame("F1", 960, 640, scratch);
    assert!(
        ev.frames.select_frame(frame_id),
        "should be able to select the newly created frame"
    );
    let selected_before = ev.frames.get(frame_id).unwrap().selected_window;

    let out = ev
        .eval_str_each("(split-window-below)")
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert!(
        out[0].starts_with("OK "),
        "split-window-below should succeed, got {}",
        out[0]
    );

    let frame = ev.frames.get(frame_id).expect("frame still exists");
    let selected_after = frame.selected_window;
    let leaves: Vec<_> = frame.root_window.leaf_ids();

    assert_eq!(
        leaves.len(),
        2,
        "expected exactly two leaves after split, got {leaves:?}"
    );
    assert_eq!(
        selected_after, selected_before,
        "frame.selected_window must remain the original window after \
         split-window-below (GNU src/window.c::Fsplit_window_internal \
         does not reassign frame->selected_window)"
    );
    assert!(
        leaves.contains(&selected_after),
        "frame.selected_window {:?} must be a live leaf id among {:?}",
        selected_after,
        leaves
    );

    // The exact count `is_selected` would produce in
    // collect_layout_params: comparison against each leaf.
    let selected_count = leaves.iter().filter(|id| **id == selected_after).count();
    assert_eq!(
        selected_count, 1,
        "exactly ONE leaf must match frame.selected_window after split \
         (the other gets mode-line-inactive face); got {selected_count}"
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

/// Renders the frame's window tree as a structure of combination direction,
/// buffer names and column spans.  Column spans (not full edges) keep the
/// assertion comparable with GNU `--batch`, whose root window sits at a
/// different y origin than this harness's frame.
const WINDOW_TREE_RENDERER: &str = r#"
(defun neo--wt (node)
  (cond
   ((windowp node)
    (list (buffer-name (window-buffer node))
          (nth 0 (window-edges node))
          (nth 2 (window-edges node))))
   ((consp node)
    (cons (if (car node) 'v 'h) (mapcar #'neo--wt (cddr node))))))
(defun neo--tree () (neo--wt (car (window-tree))))
"#;

fn eval_window_tree(body: &str) -> String {
    crate::test_utils::init_test_tracing();
    let src = format!("{WINDOW_TREE_RENDERER}\n{body}");
    bootstrap_eval_with_frame(&src)
        .pop()
        .expect("at least one result")
}

/// The reported bug: with a `left` side window present, a new `right` side
/// window must land at the frame's far right as the LAST child of the root
/// combination -- not between the main windows.
///
/// GNU Emacs 31 `--batch`, 80-column frame:
///   (h ("*left-side*" 0 20) (h ("*scratch*" 20 40) ("*scratch*" 40 60))
///      ("*right-side*" 60 80))
#[test]
fn right_side_window_attaches_at_frame_far_right_when_left_side_window_exists() {
    let tree = eval_window_tree(
        r#"(progn
             (setq display-buffer-alist
                   '(("\\*left-side\\*"  (display-buffer-in-side-window)
                      (side . left)  (window-width . 20))
                     ("\\*right-side\\*" (display-buffer-in-side-window)
                      (side . right) (window-width . 20))))
             (display-buffer (get-buffer-create "*left-side*"))
             (split-window (selected-window) nil 'right)
             (display-buffer (get-buffer-create "*right-side*"))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*left-side*\" 0 20) \
         (h (\"*scratch*\" 20 40) (\"*scratch*\" 40 60)) \
         (\"*right-side*\" 60 80))"
    );
}

/// The intermediate step of the same scenario: `split-window` binds
/// `window-combination-limit` to t when the split target has a side-window
/// sibling (lisp/window.el, "If `window-combination-resize' is 'side and
/// window has a side window sibling"), so the main area must be wrapped in
/// its own internal node rather than flattened into the root.
#[test]
fn splitting_next_to_a_side_window_wraps_the_main_area_in_a_new_parent() {
    let tree = eval_window_tree(
        r#"(progn
             (setq display-buffer-alist
                   '(("\\*left-side\\*" (display-buffer-in-side-window)
                      (side . left) (window-width . 20))))
             (display-buffer (get-buffer-create "*left-side*"))
             (split-window (selected-window) nil 'right)
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*left-side*\" 0 20) \
         (h (\"*scratch*\" 20 50) (\"*scratch*\" 50 80)))"
    );
}

/// `window-combination-limit` is a DYNAMIC variable read by
/// `split-window-internal`; binding it to t must force a fresh parent even
/// though the target's parent is iso-combined.
#[test]
fn split_window_honors_dynamic_window_combination_limit() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit t))
               (split-window (selected-window) nil 'right)
               (split-window (selected-window) nil 'right))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 40)) (\"*scratch*\" 40 80))"
    );
}

/// The control case: with the variable nil, an iso-combined parent is reused
/// and the tree stays flat.  (This already passed before the fix; it guards
/// against over-correcting into always making a new parent.)
#[test]
fn split_window_reuses_iso_combined_parent_when_limit_is_nil() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit nil))
               (split-window (selected-window) nil 'right)
               (split-window (selected-window) nil 'right))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 40) (\"*scratch*\" 40 80))"
    );
}

/// Reusing the parent must not be gated on the split target being a LEAF.
/// GNU splices a new sibling next to an internal node just the same, which is
/// exactly how a side window is attached beside the main-window group.
#[test]
fn split_window_reuses_iso_combined_parent_when_target_is_internal() {
    let tree = eval_window_tree(
        r#"(progn
             (split-window (selected-window) nil 'right)
             (let ((window-combination-limit t))
               (split-window (selected-window) nil 'right))
             (let ((window-combination-limit nil)
                   (ignore-window-parameters t))
               (split-window (window-parent (selected-window)) nil 'right))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (h (\"*scratch*\" 0 10) (\"*scratch*\" 10 20)) \
         (\"*scratch*\" 20 40) (\"*scratch*\" 40 80))"
    );
}

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

/// Deleting the MIDDLE of three siblings gives its columns to the window on
/// its left.  GNU Emacs 31 `--batch`, 80-column frame: `(h 0-40 40-80)`.
#[test]
fn deleting_a_middle_window_gives_its_space_to_the_previous_sibling() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit nil))
               (split-window nil nil 'right)
               (split-window nil nil 'right))
             (delete-window (nth 1 (window-list nil 'no-minibuf nil)))
             (neo--tree))"#,
    );
    assert_eq!(tree, "OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 80))");
}

/// Deleting the LAST of three siblings gives its columns to the one before it;
/// the first window must not move.  GNU: `(h 0-20 20-80)`.
#[test]
fn deleting_the_last_window_gives_its_space_to_the_previous_sibling() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit nil))
               (split-window nil nil 'right)
               (split-window nil nil 'right))
             (delete-window (nth 2 (window-list nil 'no-minibuf nil)))
             (neo--tree))"#,
    );
    assert_eq!(tree, "OK (h (\"*scratch*\" 0 20) (\"*scratch*\" 20 80))");
}

/// Deleting the FIRST sibling has no previous sibling, so the NEXT one absorbs
/// the space and slides left; the third window must not move.  GNU:
/// `(h 0-40 40-80)`.
#[test]
fn deleting_the_first_window_gives_its_space_to_the_next_sibling() {
    let tree = eval_window_tree(
        r#"(progn
             (let ((window-combination-limit nil))
               (split-window nil nil 'right)
               (split-window nil nil 'right))
             (delete-window (nth 0 (window-list nil 'no-minibuf nil)))
             (neo--tree))"#,
    );
    assert_eq!(tree, "OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 80))");
}

/// A side window keeps its width when an unrelated window is deleted: the
/// freed columns belong to the main group's sibling, not to every child of the
/// root.  GNU: `(h ("*L*" 0 20) (h 20-50 50-80))`.
#[test]
fn deleting_a_right_side_window_leaves_the_left_side_window_width_intact() {
    let tree = eval_window_tree(
        r#"(progn
             (setq display-buffer-alist
                   '(("\\*L\\*" (display-buffer-in-side-window)
                      (side . left) (window-width . 20))
                     ("\\*R\\*" (display-buffer-in-side-window)
                      (side . right) (window-width . 20))))
             (display-buffer (get-buffer-create "*L*"))
             (split-window (window-main-window) nil 'right)
             (display-buffer (get-buffer-create "*R*"))
             (delete-window (get-buffer-window "*R*"))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*L*\" 0 20) (h (\"*scratch*\" 20 50) (\"*scratch*\" 50 80)))"
    );
}

/// GNU `recombine_windows` (src/window.c:2606-2650), called on the window
/// promoted into its parent's slot (src/window.c:5801): if that window is
/// itself a combination along the SAME axis as its new parent, and its stored
/// `combination_limit` slot is nil, its children are spliced into the parent
/// and it disappears.
///
/// The combination must be UNSEALED for this to fire, so the nesting here is
/// built with *orthogonal* splits — a `window-combination-limit t` split seals
/// the parent it creates, and GNU skips sealed nodes.
///
/// GNU Emacs 31 `--batch`, 80-column frame: `(h 0-40 40-60 60-80)`.
#[test]
fn deleting_a_window_recombines_the_promoted_child_into_an_iso_parent() {
    let tree = eval_window_tree(
        r#"(progn
             (let* ((w (selected-window))
                    (n1 (split-window w nil 'right)))
               (select-window n1)
               (let ((n2 (split-window n1 nil 'below)))
                 (select-window n2)
                 (split-window n2 nil 'right)
                 (delete-window n1)))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*scratch*\" 0 40) (\"*scratch*\" 40 60) (\"*scratch*\" 60 80))"
    );
}

/// A SEALED combination must survive promotion intact — `set-window-combination-limit`
/// exists precisely to stop the merge (and `window--make-major-side-window`
/// relies on it, Bug#80665).
#[test]
fn deleting_a_window_does_not_recombine_a_sealed_promoted_child() {
    let tree = eval_window_tree(
        r#"(progn
             (let* ((w (selected-window))
                    (n1 (split-window w nil 'right)))
               (select-window n1)
               (let ((n2 (split-window n1 nil 'below)))
                 (select-window n2)
                 (split-window n2 nil 'right)
                 (set-window-combination-limit (window-parent (selected-window)) t)
                 (delete-window n1)))
             (neo--tree))"#,
    );
    assert_eq!(
        tree,
        "OK (h (\"*scratch*\" 0 40) (h (\"*scratch*\" 40 60) (\"*scratch*\" 60 80)))"
    );
}

/// A wrap that lands exactly at end of buffer starts no screen line, so
/// `vertical-motion' must report that it moved over none.
///
/// GNU returns the number of screen lines actually moved over, "closer to zero
/// if beginning or end of buffer was reached" (src/indent.c, Fvertical_motion).
/// In batch GNU answers from `vmotion' -> `compute_motion', which stops at ZV
/// and counts only the lines it genuinely crossed. Filling the body width
/// exactly puts point at ZV with nothing on a following line, so the count is
/// zero even though point moves.
///
/// The one-past case is pinned alongside it deliberately: with a single extra
/// character there IS an occupied continuation line, and the answer is 1. That
/// keeps the end-of-buffer rule from degenerating into "always report zero at
/// ZV", which would break every ordinary wrap.
#[test]
fn vertical_motion_counts_only_screen_lines_that_are_actually_occupied() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-vertical-motion-eob*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let ((width (window-body-width)))
                     (mapcar
                      (lambda (n)
                        (erase-buffer)
                        (insert (make-string n ?x))
                        (goto-char (point-min))
                        (list n (vertical-motion 1) (point)))
                      (list (- width 1) width (+ width 1)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    // Ground truth taken from GNU 31.0.90 on the same probe, body width 80:
    //   ((79 0 80) (80 1 80) (81 1 80))
    assert_eq!(
        result, "OK ((79 0 80) (80 1 80) (81 1 80))",
        "a wrap landing exactly at end of buffer must report no screen line \
         moved, while one character more must still report the continuation line"
    );
}

/// A TRUNCATE row CLIPPED at the window's right edge is one screen line to
/// GNU's display iterator and none at all to `compute_motion`, and this port
/// has to answer both.
///
/// GNU's two `vertical-motion` engines part company exactly here, and their
/// own sources say so in four lines:
///
/// * `compute_motion`'s truncating branch skips to the next newline and
///   leaves `vpos` alone (`src/indent.c:1494-1502`), where its CONTINUING
///   branch increments it (`src/indent.c:1523`).  A clipped row that runs to
///   the end of the buffer without a newline therefore crosses no screen line
///   at all -- and `count-screen-lines`, which is `1+` that count unless the
///   end is off-screen (`lisp/window.el:9886-9889`), then answers 0 for a
///   buffer with text in it.  **That is GNU's own answer under `--batch`**,
///   measured: `((78 0 79 1) (79 0 80 1) (80 0 81 1) (81 0 82 0) (160 0 161 0))`.
/// * The display iterator's `MOVE_LINE_TRUNCATED` arm reseats to the next
///   visible line start and falls through to `++it->vpos`
///   (`src/xdisp.c:11118-11143` and `:11200`); only the "Stop when ZV reached"
///   exit above it (`src/xdisp.c:10250-10257`, which runs BEFORE the row is
///   found to overflow) returns without counting.  So a row that the buffer
///   merely ran out on counts none, and a row that was CLIPPED counts one.
///
/// The two answers cannot both come out of one row label, which is why
/// `ScreenLineEnd::ClippedAtBufferEnd` is read through `MotionEngine`.
///
/// `(vertical-motion 0)` from ZV is pinned beside it, and it is the reason the
/// engine-dependence lives on the FORWARD count and not on "is `next` a row
/// start": GNU answers the LOGICAL line start under BOTH engines, at every one
/// of these lengths, because its backward walk
/// (`move_it_vertically_backward`, `src/xdisp.c:11473-11492`) puts the clipped
/// remainder back on the row it was clipped from.  A fix that made the clipped
/// boundary a row START would answer 81 here where GNU answers 1.
///
/// Ground truth, GNU Emacs 31.0.90, body width 80, `truncate-lines' t, one
/// line of `x' with no trailing newline, fields
/// `(LEN MOVED POINT COUNT-SCREEN-LINES VERTICAL-MOTION-0-FROM-ZV)':
///
/// ```text
///   --batch          (79 0 80 1 1) (80 0 81 1 1) (81 0 82 0 1)
///   -nw in a pty     (79 0 80 1 1) (80 1 81 2 1) (81 1 82 1 1)
/// ```
#[test]
fn a_clipped_truncate_row_counts_as_a_screen_line_only_under_the_display_iterator() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-clipped-truncate-row*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let* ((width (window-body-width))
                          (walk
                           (lambda ()
                             (mapcar
                              (lambda (n)
                                (erase-buffer)
                                (insert (make-string n ?x))
                                (setq-local truncate-lines t)
                                (setq-local word-wrap nil)
                                (goto-char (point-min))
                                (let* ((moved (vertical-motion (buffer-size)))
                                       (landed (point))
                                       (csl (count-screen-lines (point-min)
                                                                (point-max)))
                                       (bovl (progn (goto-char (point-max))
                                                    (vertical-motion 0)
                                                    (point))))
                                  (list n moved landed csl bovl)))
                              (list (- width 1) width (+ width 1))))))
                     (list width
                           (funcall walk)
                           (let ((noninteractive nil)) (funcall walk)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (80 \
         ((79 0 80 1 1) (80 0 81 1 1) (81 0 82 0 1)) \
         ((79 0 80 1 1) (80 1 81 2 1) (81 1 82 1 1)))",
        "a clipped TRUNCATE row must count as one screen line under the display \
         iterator and as none under compute_motion, while `vertical-motion 0' \
         from ZV answers the logical line start under both"
    );
}

/// The same clipped row, but NOT the last one: a TRUNCATE row that ends at a
/// NEWLINE counts under BOTH engines, and must keep doing so.
///
/// This is the control that keeps the engine split from being applied to every
/// truncated row.  `compute_motion` reaches the newline it skipped to and
/// increments `vpos` there (`src/indent.c:1494-1502` leaves `pos` just before
/// the newline, and the main loop then consumes it), while the display
/// iterator counted the row itself; the two arrive at the same place with the
/// same count.  Only the row whose clipped remainder ends at ZV -- with no
/// newline for `compute_motion` to reach -- divides them.
///
/// Ground truth, GNU Emacs 31.0.90, body width 80, `truncate-lines' t, over
/// LEN `x', a newline, LEN `y' and a newline, fields
/// `(LEN MOVED POINT COUNT-SCREEN-LINES)':
///
/// ```text
///   --batch        (79 2 161 2) (80 2 163 2) (160 2 323 2)
///   -nw in a pty   (79 2 161 2) (80 2 163 3) (160 2 323 3)
/// ```
///
/// The MOVED and POINT columns are identical in the two engines -- it is only
/// `count-screen-lines`, which narrows away the final newline and so ends the
/// buffer in the middle of a clipped row, that can tell them apart.
#[test]
fn a_clipped_truncate_row_that_ends_at_a_newline_counts_under_both_engines() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-clipped-truncate-newline*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let* ((width (window-body-width))
                          (walk
                           (lambda ()
                             (mapcar
                              (lambda (n)
                                (erase-buffer)
                                (insert (make-string n ?x) "\n"
                                        (make-string n ?y) "\n")
                                (setq-local truncate-lines t)
                                (setq-local word-wrap nil)
                                (goto-char (point-min))
                                (let* ((moved (vertical-motion (buffer-size)))
                                       (landed (point))
                                       (csl (count-screen-lines (point-min)
                                                                (point-max))))
                                  (list n moved landed csl)))
                              (list (- width 1) width (+ width 80))))))
                     (list width
                           (funcall walk)
                           (let ((noninteractive nil)) (funcall walk)))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (80 \
         ((79 2 161 2) (80 2 163 2) (160 2 323 2)) \
         ((79 2 161 2) (80 2 163 3) (160 2 323 3)))",
        "a truncated row terminated by a newline must count under both engines; \
         only the one whose clipped remainder ends at ZV divides them"
    );
}

/// The end of the accessible buffer is a goal-column STOP on the row that
/// reached it -- one more stop than that row has glyphs.
///
/// GNU's goal walk is `move_it_in_display_line (&it, ZV, first_x + to_x,
/// MOVE_TO_X)` (`src/indent.c:2540`), and `move_it_in_display_line_to` tests
/// `get_next_display_element` -- "Stop when ZV reached" -- BEFORE it tests the
/// row's right edge (`src/xdisp.c:10250-10257`).  So a row the buffer ran out
/// on returns `MOVE_POS_MATCH_OR_ZV` with the iterator standing ON ZV, which
/// draws nothing and therefore sits one column past the last glyph, exactly
/// like the newline on a terminated row.
///
/// A CLIPPED row is the case that keeps this from being "always stop at ZV":
/// its ZV is past the window edge, and the walk stops at the edge first.
///
/// Measured under GNU Emacs 31.0.90, body width 80, one line of `x' with no
/// trailing newline, `(vertical-motion (cons GOAL 0))' from `point-min',
/// identical whether the window truncates or wraps:
///
/// ```text
///   len 78   goal 77 -> 78    goal 78, 79, 80, 200 -> 79   (79 is ZV)
///   len 79   goal 77 -> 78    goal 78 -> 79   goal 79, 80, 200 -> 80  (80 is ZV)
///   len 80   goal 77 -> 78    goal 78 -> 79   goal 79, 80, 200 -> 80  (the row
///                                     was CLIPPED, so its ZV -- 81 -- is not
///                                     on it and 80 is the last stop)
/// ```
#[test]
fn the_end_of_the_buffer_is_a_goal_column_stop_on_the_row_that_reached_it() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-goal-at-buffer-end*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let* ((width (window-body-width))
                          (noninteractive nil)
                          (walk
                           (lambda (trunc)
                             (mapcar
                              (lambda (n)
                                (erase-buffer)
                                (insert (make-string n ?x))
                                (setq-local truncate-lines trunc)
                                (setq-local word-wrap nil)
                                (cons n
                                      (mapcar
                                       (lambda (goal)
                                         (goto-char (point-min))
                                         (vertical-motion (cons goal 0))
                                         (point))
                                       (list (- width 3) (- width 2)
                                             (- width 1) width 200))))
                              (list (- width 2) (- width 1) width)))))
                     (list width
                           (funcall walk t)
                           (funcall walk nil))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (80 \
         ((78 78 79 79 79 79) (79 78 79 80 80 80) (80 78 79 80 80 80)) \
         ((78 78 79 79 79 79) (79 78 79 80 80 80) (80 78 79 80 80 80)))",
        "the goal-column walk must be able to come to rest at the end of the \
         accessible buffer, which is a stop on the row that reached it"
    );
}

/// A hscrolled window's right edge is `hscroll` columns further into the LINE,
/// so a hscrolled `TRUNCATE` row is clipped later -- or not at all.
///
/// GNU puts the hscroll into the iterator's coordinates, not into the row's
/// content: `it->first_visible_x = window_hscroll_limited (w, f) *
/// FRAME_COLUMN_WIDTH (it->f)` (`src/xdisp.c:3500-3501`) and
/// `it->last_visible_x = it->first_visible_x + body_width`
/// (`src/xdisp.c:3507`), less the truncation glyph on a `TRUNCATE` row
/// (`src/xdisp.c:3512-3518`).  `it->current_x` still starts at 0 at the line
/// start, so the column at which the row is cut off is
/// `hscroll + body-width - 1` and not `body-width - 1`.
///
/// Measured under GNU Emacs 31.0.90 at body width 80, `truncate-lines' t, one
/// line of `x' with no trailing newline, `set-window-hscroll' with NO
/// redisplay, `(vertical-motion (buffer-size))' from `point-min':
///
/// ```text
///   len  80   hscroll 0                 -> (1 81)   clipped at column 79
///   len  80   hscroll 1, 5, 40, 100     -> (0 81)   80 columns now fit
///   len 160   hscroll 0, 1, 5, 40       -> (1 161)  still clipped
///   len 160   hscroll 100, 200          -> (0 161)  no longer clipped
/// ```
///
/// The boundary is exactly `hscroll + 79`: 160 columns are clipped while that
/// is 119 and not while it is 179.
#[test]
fn a_hscrolled_truncate_row_is_clipped_hscroll_columns_further_along() {
    crate::test_utils::init_test_tracing();
    let result = bootstrap_eval_one_with_frame(
        r#"(let ((b (get-buffer-create " *probe-hscrolled-clip*")))
             (unwind-protect
                 (progn
                   (delete-other-windows)
                   (switch-to-buffer b)
                   (let* ((width (window-body-width))
                          (noninteractive nil))
                     (cons width
                           (mapcar
                            (lambda (n)
                              (erase-buffer)
                              (insert (make-string n ?x))
                              (setq-local truncate-lines t)
                              (setq-local word-wrap nil)
                              (cons n
                                    (mapcar
                                     (lambda (hs)
                                       (set-window-hscroll (selected-window) hs)
                                       (goto-char (point-min))
                                       (list hs (vertical-motion (buffer-size))
                                             (point)))
                                     '(0 1 5 40 100))))
                            (list width (* 2 width))))))
               (if (buffer-live-p b)
                   (kill-buffer b))
               (set-window-hscroll (selected-window) 0)
               (delete-other-windows)))"#,
    );
    assert_eq!(
        result,
        "OK (80 \
         (80 (0 1 81) (1 0 81) (5 0 81) (40 0 81) (100 0 81)) \
         (160 (0 1 161) (1 1 161) (5 1 161) (40 1 161) (100 0 161)))",
        "the column at which a TRUNCATE row is cut off is hscroll + body-width \
         - 1, so hscrolling a window makes a row that was clipped stop being \
         clipped"
    );
}

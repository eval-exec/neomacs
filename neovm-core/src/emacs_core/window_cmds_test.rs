use crate::buffer::LispCharPos1;
use crate::emacs_core::eval::{GuiFrameHostSize, ResolvedFrameFont};
use crate::emacs_core::value::{ValueKind, VecLikeType};
use crate::emacs_core::window_cmds::SplitWindowSide;
use crate::emacs_core::{Context, DisplayHost, GuiFrameHostRequest, Value, format_eval_result};
use crate::face::{FontSlant, FontWeight, FontWidth};
use crate::heap_types::LispString;
use crate::test_utils::{runtime_startup_context, runtime_startup_eval_all};
use crate::window::FrameParam;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

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

fn bootstrap_eval_with_frame(src: &str) -> Vec<String> {
    runtime_startup_eval_all(src)
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
    removed_child_frames: Rc<RefCell<Vec<crate::window::FrameId>>>,
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

impl DisplayHost for RecordingDisplayHost {
    fn realize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        self.realized.borrow_mut().push(request);
        Ok(())
    }

    fn resize_gui_frame(&mut self, request: GuiFrameHostRequest) -> Result<(), String> {
        self.resized.borrow_mut().push(request);
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

    fn remove_gui_child_frame(&mut self, frame_id: crate::window::FrameId) -> Result<(), String> {
        self.removed_child_frames.borrow_mut().push(frame_id);
        Ok(())
    }

    fn resolve_frame_font(
        &mut self,
        _frame_id: crate::window::FrameId,
        _face: crate::face::Face,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        Ok(self.resolved_frame_font.clone())
    }
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
    let r = eval_one_with_frame(
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
    let r = eval_one_with_frame(
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
        &ev.buffers,
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
    assert_eq!(
        super::builtin_window_edges(&mut ev, vec![Value::NIL, Value::T, Value::NIL, Value::T])
            .expect("window-edges"),
        Value::list(vec![
            Value::fixnum(16),
            Value::fixnum(0),
            Value::fixnum(764),
            Value::fixnum(568),
        ])
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
    let out = eval_with_gui_frame(
        "(let ((w (selected-window)))
           (list (window-fringes w)
                 (set-window-fringes w 0 4)
                 (window-fringes w)
                 (set-window-fringes w nil nil)
                 (window-fringes w)))",
    );
    assert_eq!(out[0], "OK ((8 8 nil nil) t (0 4 nil nil) t (8 8 nil nil))");
}

#[test]
fn gui_window_scroll_bars_round_trip_explicit_state() {
    crate::test_utils::init_test_tracing();
    let out = eval_with_gui_frame(
        "(let ((w (selected-window)))
           (list (window-scroll-bars w)
                 (set-window-scroll-bars w 13 'left 9 'bottom t)
                 (window-scroll-bars w)
                 (window-scroll-bar-width w)
                 (window-scroll-bar-height w)))",
    );
    assert_eq!(
        out[0],
        "OK ((nil 1 t nil 0 t nil) t (13 2 left 9 1 bottom t) 13 9)"
    );
}

#[test]
fn gui_window_body_geometry_excludes_scroll_bar_area() {
    crate::test_utils::init_test_tracing();
    let out = eval_with_gui_frame(
        "(let ((w (selected-window)))
           (set-window-scroll-bars w 13 'left)
           (list (window-body-width w t)
                 (window-text-width w t)))",
    );
    assert_eq!(out[0], "OK (771 771)");
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

    let out = ev
        .eval_str_each(
            "(let ((w (selected-window)))
           (set-window-buffer w \" *gui-swb-display*\")
           (list (window-fringes w)
                 (window-scroll-bars w)
                 (window-scroll-bar-width w)
                 (window-scroll-bar-height w)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK ((3 5 t nil) (11 2 left 7 1 bottom nil) 11 7)");
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
    let r = eval_one_with_frame(
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
    let r = eval_one_with_frame(
        "(let ((f1 (selected-frame))
              (f2 (make-frame)))
           (let ((w1 (progn (select-frame f1) (selected-window)))
                 (w2 (progn (select-frame f2) (selected-window))))
             (prog1
                 (list (null (memq w2 (window-list-1 w1 nil nil)))
                       (not (null (memq w2 (window-list-1 w1 nil t))))
                       (not (null (memq w2 (window-list-1 w1 nil 'visible))))
                       (not (null (memq w2 (window-list-1 w1 nil 0))))
                       (not (null (memq w2 (window-list-1 w1 nil f2))))
                       (null (memq w2 (window-list-1 w1 nil :bad))))
               (select-frame f1)
               (delete-frame f2))))",
    );
    assert_eq!(r, "OK (t t t t t t)");
}

#[test]
fn get_buffer_window_all_frames_selects_gnu_frame_scope() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame(
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
fn split_window_internal_side_t_splits_horizontally_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(let* ((old (selected-window))
                (new (split-window-internal old nil t nil)))
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
        "OK (t (0 0 20 24) 20 (width t) ((\"*scratch*\" (20 0 80 12) 60 nil) (\"*Warnings*\" (20 12 80 24) 60 nil) (\"*fixed-side*\" (0 0 20 24) 20 left)))"
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
    let results = eval_with_frame(
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
    // GNU Emacs signals wrong-type-argument for invalid window
    // designators (not generic error).
    assert_eq!(results[0], "OK wrong-type-argument");
    assert_eq!(results[1], "OK wrong-type-argument");
    assert_eq!(results[2], "OK wrong-type-argument");
    assert_eq!(results[3], "OK wrong-type-argument");
    assert_eq!(results[4], "OK wrong-type-argument");
    assert_eq!(results[5], "OK wrong-type-argument");
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
    let result = eval_one_with_frame(
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
               (condition-case err (window-bump-use-time nil nil) (error err))
               (let ((w (split-window-internal (selected-window) nil nil nil)))
                 (delete-window w)
                 (condition-case err (window-bump-use-time w) (error (car err)))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
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
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
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
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
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
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
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
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
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
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
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
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
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
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
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
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
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
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
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
        frame.replace_display_snapshots(vec![crate::window::WindowDisplaySnapshot {
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
        frame.replace_display_snapshots(vec![crate::window::WindowDisplaySnapshot {
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
    let source = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("project root")
            .join("lisp/window.el"),
    )
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
             (not (eq w1 w2))))",
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
    let mut ev = Context::new();
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    let out = ev
        .eval_str_each(
            "(condition-case err (make-frame nil nil) (error (car err)))
         (condition-case err (delete-frame nil nil nil) (error (car err)))
         (condition-case err (frame-parameter nil 'name nil) (error (car err)))
         (condition-case err (frame-parameters nil nil) (error (car err)))
         (condition-case err (modify-frame-parameters nil nil nil) (error (car err)))
         (condition-case err (frame-visible-p nil nil) (error (car err)))",
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
        super::builtin_frame_native_width(&mut ev, vec![Value::make_frame(fid.0)]).unwrap(),
        Value::fixnum(800)
    );
    assert_eq!(
        super::builtin_frame_native_height(&mut ev, vec![Value::make_frame(fid.0)]).unwrap(),
        Value::fixnum(600)
    );
    assert_eq!(
        super::builtin_frame_text_width(&mut ev, vec![Value::make_frame(fid.0)]).unwrap(),
        Value::fixnum(776)
    );
    assert_eq!(
        super::builtin_frame_text_height(&mut ev, vec![Value::make_frame(fid.0)]).unwrap(),
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
        super::builtin_frame_text_width(&mut ev, vec![Value::make_frame(fid.0)]).unwrap(),
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
        super::builtin_frame_native_width(&mut ev, vec![Value::make_frame(fid.0)]).unwrap(),
        Value::fixnum(800)
    );
    assert_eq!(
        super::builtin_frame_native_height(&mut ev, vec![Value::make_frame(fid.0)]).unwrap(),
        Value::fixnum(600)
    );
}

#[test]
fn select_frame_arity_designators_and_selection() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    let out = ev
        .eval_str_each(
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
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK (wrong-type-argument frame-live-p nil)");
    assert_eq!(out[2], "OK (wrong-type-argument frame-live-p \"x\")");
    assert_eq!(out[3], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[4], "OK (t t)");
}

#[test]
fn select_frame_set_input_focus_arity_designators_and_result() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let out = ev
        .eval_str_each(
            "(condition-case err (select-frame-set-input-focus) (error (car err)))
         (condition-case err (select-frame-set-input-focus nil) (error err))
         (condition-case err (select-frame-set-input-focus \"x\") (error err))
         (condition-case err (select-frame-set-input-focus 999999) (error err))
         (let ((f (selected-frame)))
           (list (select-frame-set-input-focus f)
                 (eq (selected-frame) f)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK (wrong-type-argument frame-live-p nil)");
    assert_eq!(out[2], "OK (wrong-type-argument frame-live-p \"x\")");
    assert_eq!(out[3], "OK (wrong-type-argument frame-live-p 999999)");
    assert_eq!(out[4], "OK (nil t)");
}

#[test]
fn set_frame_selected_window_matches_selection_and_error_semantics() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    let out = ev
        .eval_str_each(
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
    )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
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
    let results = eval_with_frame(
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
    let child =
        super::builtin_make_terminal_frame(&mut ev, vec![params]).expect("make-terminal-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));

    assert_eq!(
        super::builtin_frame_parent(&mut ev, vec![child]).expect("frame-parent"),
        root
    );
    assert_eq!(
        super::builtin_frame_ancestor_p(&mut ev, vec![root, child]).expect("frame-ancestor-p"),
        Value::T
    );

    let position = super::builtin_frame_position(&mut ev, vec![child]).expect("frame-position");
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
        super::builtin_make_frame_visible(&mut ev, vec![child]).expect("make-frame-visible"),
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

    let child =
        super::builtin_make_terminal_frame(&mut ev, vec![params]).expect("make-terminal-frame");
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

    let child =
        super::builtin_make_terminal_frame(&mut ev, vec![params]).expect("make-terminal-frame");
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
    let child =
        super::builtin_make_terminal_frame(&mut ev, vec![params]).expect("make-terminal-frame");
    let child_id = crate::window::FrameId(child.as_frame_id().expect("child frame"));
    let text_pixels = |n| Value::cons(Value::symbol("text-pixels"), Value::fixnum(n));
    let alist = Value::list(vec![
        Value::cons(Value::symbol("width"), text_pixels(91)),
        Value::cons(Value::symbol("height"), text_pixels(18)),
    ]);

    super::builtin_modify_frame_parameters(&mut ev, vec![child, alist])
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
    let child =
        super::builtin_make_terminal_frame(&mut ev, vec![params]).expect("make-terminal-frame");
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

    let child =
        super::builtin_make_terminal_frame(&mut ev, vec![params]).expect("make-terminal-frame");
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
    assert!(!frame.visible);
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
fn x_create_frame_with_parent_frame_creates_gui_child_overlay_without_host_window() {
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
    assert!(child.undecorated);
    assert!(child.no_accept_focus);
    assert_eq!(child.minibuffer_window, Some(parent_minibuffer));
    assert!(child.minibuffer_leaf.is_none());
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

    super::builtin_modify_frame_parameters(&mut ev, vec![created, alist])
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

    super::builtin_delete_frame(&mut ev, vec![created]).expect("delete child frame");

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

    super::builtin_make_frame_invisible(&mut ev, vec![created])
        .expect("make child frame invisible");

    let child = ev.frames.get(child_id).expect("child frame remains live");
    assert!(!child.visible);
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

    super::builtin_delete_frame(&mut ev, vec![Value::make_frame(second_id.0)])
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

    super::builtin_delete_frame(&mut ev, vec![Value::make_frame(parent_id.0)])
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
fn make_frame_uses_gui_creation_path_when_display_host_is_active() {
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
    let created = super::builtin_make_frame(&mut ev, vec![params]).expect("make-frame");

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
    let results = eval_with_frame(
        "(let ((f2 (make-frame)))
           (delete-frame f2)
           (length (frame-list)))",
    );
    assert_eq!(results[0], "OK 1");
}

#[test]
fn delete_frame_on_dead_frame_object_returns_nil() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
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
            256,
        ),
    );
    let secondary = ev.frames.create_frame_on_terminal("F2", 7, 800, 600, buf);
    let secondary_terminal =
        crate::emacs_core::terminal::pure::terminal_handle_value_for_id(7).expect("terminal 7");

    assert_eq!(
        super::builtin_delete_frame(&mut ev, vec![Value::make_frame(secondary.0)]).unwrap(),
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

    let result = super::builtin_framep(&mut ev, vec![Value::make_frame(frame_id.0)]).unwrap();
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
        super::builtin_framep(&mut ev, vec![frame]).unwrap(),
        Value::T
    );
    assert_eq!(
        super::builtin_frame_live_p(&mut ev, vec![frame]).unwrap(),
        Value::T
    );
    assert_eq!(
        super::builtin_frame_visible_p(&mut ev, vec![frame]).unwrap(),
        Value::T
    );
    assert_eq!(
        super::builtin_select_frame(&mut ev, vec![frame]).unwrap(),
        Value::make_frame(fid.0)
    );
    assert_eq!(
        super::builtin_select_frame_set_input_focus(&mut ev, vec![frame]).unwrap(),
        Value::NIL
    );
}

#[test]
fn frame_initial_p_accepts_terminal_object_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    ev.buffers.set_current(buf);

    // The bootstrap initial terminal (id 0) is GNU's `output_initial`.
    let initial_terminal = crate::emacs_core::terminal::pure::terminal_handle_value_for_id(
        crate::emacs_core::terminal::pure::TERMINAL_ID,
    )
    .expect("initial terminal");
    assert_eq!(
        super::builtin_frame_initial_p(&mut ev, vec![initial_terminal]).unwrap(),
        Value::T,
        "initial terminal should satisfy frame-initial-p (xt-mouse.el passes terminals)"
    );

    // A live secondary tty is a terminal but not the initial one.
    crate::emacs_core::terminal::pure::ensure_terminal_runtime_owner(
        7,
        "tty-7",
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::interactive(
            Some("xterm-256color".to_string()),
            256,
        ),
    );
    let secondary =
        crate::emacs_core::terminal::pure::terminal_handle_value_for_id(7).expect("terminal 7");
    assert_eq!(
        super::builtin_frame_initial_p(&mut ev, vec![secondary]).unwrap(),
        Value::NIL
    );

    // Non-frame, non-terminal values still signal wrong-type-argument.
    assert!(super::builtin_frame_initial_p(&mut ev, vec![Value::symbol("nonsense")]).is_err());
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
            256,
        ),
    );
    let secondary = ev.frames.create_frame_on_terminal("F2", 7, 800, 600, buf);

    assert_eq!(
        super::builtin_select_frame(&mut ev, vec![Value::make_frame(secondary.0)])
            .expect("select secondary frame"),
        Value::make_frame(secondary.0)
    );
    assert_eq!(ev.command_loop.keyboard.active_terminal_id(), 7);
    assert_eq!(ev.command_loop.keyboard.input_decode_map(), Value::NIL);

    ev.command_loop
        .keyboard
        .set_input_decode_map(Value::symbol("secondary-map"));

    assert_eq!(
        super::builtin_select_frame(&mut ev, vec![Value::make_frame(primary.0)])
            .expect("reselect primary frame"),
        Value::make_frame(primary.0)
    );
    assert_eq!(
        ev.command_loop.keyboard.input_decode_map(),
        Value::symbol("primary-map")
    );

    super::builtin_select_frame(&mut ev, vec![Value::make_frame(secondary.0)])
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
        super::builtin_frame_focus(&mut ev, vec![Value::make_frame(primary.0)]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        super::builtin_redirect_frame_focus(
            &mut ev,
            vec![Value::make_frame(primary.0), Value::make_frame(primary.0)],
        )
        .unwrap(),
        Value::NIL
    );
    assert_eq!(
        super::builtin_frame_focus(&mut ev, vec![Value::make_frame(primary.0)]).unwrap(),
        Value::make_frame(primary.0)
    );

    super::builtin_select_frame(&mut ev, vec![Value::make_frame(secondary.0)])
        .expect("select secondary frame");
    assert_eq!(
        super::builtin_frame_focus(&mut ev, vec![Value::make_frame(primary.0)]).unwrap(),
        Value::make_frame(secondary.0)
    );

    assert_eq!(
        super::builtin_redirect_frame_focus(&mut ev, vec![Value::make_frame(primary.0)]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        super::builtin_frame_focus(&mut ev, vec![Value::make_frame(primary.0)]).unwrap(),
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
    let font_object = Value::vector(vec![
        Value::keyword("font-object"),
        Value::keyword("name"),
        Value::string("-*-Hack-regular-normal-*-*-27-*-*-*-*-*-*-*"),
        Value::keyword("family"),
        Value::string("Hack"),
        Value::keyword("weight"),
        Value::symbol("regular"),
        Value::keyword("slant"),
        Value::symbol("normal"),
        Value::keyword("size"),
        Value::fixnum(27),
    ]);
    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.set_known_parameter(FrameParam::Font, font_object);
    }

    let frame_value = Value::make_frame(fid.0);
    let direct =
        super::builtin_frame_parameter(&mut ev, vec![frame_value, FrameParam::Font.symbol()])
            .expect("frame-parameter");
    assert_eq!(
        direct.as_utf8_str(),
        Some("-*-Hack-regular-normal-*-*-27-*-*-*-*-*-*-*")
    );

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
        r#"OK ("-*-Hack-regular-normal-*-*-27-*-*-*-*-*-*-*" "-*-Hack-regular-normal-*-*-27-*-*-*-*-*-*-*")"#
    );
}

#[test]
fn modify_frame_parameters_name() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        "(modify-frame-parameters (selected-frame) '((name . \"NewName\")))
         (frame-parameter (selected-frame) 'name)
         (frame-parameter (selected-frame) 'explicit-name)",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], r#"OK "NewName""#);
    assert_eq!(results[2], "OK t");
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
fn divider_width_builtins_read_frame_parameters_like_gnu() {
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
    assert_eq!(results[1], "OK (6 4 6 4)");
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
    let results = eval_with_frame(
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
fn modify_frame_parameters_width_height_preserve_pixel_dimensions() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let buf = ev.buffers.create_buffer("*scratch*");
    let fid = ev.frames.create_frame("F1", 800, 600, buf);
    let out = ev
        .eval_str_each("(modify-frame-parameters (selected-frame) '((width . 80) (height . 25)))");
    assert!(
        out[0].is_ok(),
        "modify-frame-parameters failed: {:?}",
        out[0]
    );

    let frame = ev.frames.get(fid).expect("frame should exist");
    assert_eq!(frame.width, 800);
    assert_eq!(frame.height, 600);
    assert_eq!(frame.parameter("width"), Some(Value::fixnum(80)));
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(25)));
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
    let host = RecordingDisplayHost::with_resolved_frame_font(ResolvedFrameFont {
        family: LispString::from_utf8("Noto Sans Mono"),
        foundry: None,
        weight: FontWeight::NORMAL,
        slant: FontSlant::Normal,
        width: FontWidth::Normal,
        postscript_name: Some(LispString::from_utf8("NotoSansMono-Regular")),
        height_tenths: 160,
        font_size_px: 22.0,
        char_width: 13.0,
        line_height: 31.0,
    });
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

    crate::emacs_core::font::builtin_internal_set_lisp_face_attribute(
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
    let host = RecordingDisplayHost::with_resolved_frame_font(ResolvedFrameFont {
        family: LispString::from_utf8("Noto Sans Mono"),
        foundry: None,
        weight: FontWeight::NORMAL,
        slant: FontSlant::Normal,
        width: FontWidth::Normal,
        postscript_name: Some(LispString::from_utf8("NotoSansMono-Regular")),
        height_tenths: 160,
        font_size_px: 22.0,
        char_width: 13.0,
        line_height: 31.0,
    });
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

    crate::emacs_core::font::builtin_internal_set_lisp_face_attribute(
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
fn set_frame_size_builtins_preserve_pixel_dimensions() {
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
    assert_eq!(frame.parameter("width"), Some(Value::fixnum(100)));
    assert_eq!(frame.parameter("height"), Some(Value::fixnum(36)));
    assert_eq!(
        frame.parameter("neovm--frame-text-lines"),
        Some(Value::fixnum(35))
    );
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

    ev.apply_resize_input_event(824, 560, fid.0, false);

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

    super::builtin_set_frame_size(
        &mut ev,
        vec![child, Value::fixnum(525), Value::fixnum(374), Value::T],
    )
    .expect("first pixelwise set-frame-size");
    super::builtin_set_frame_size(
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

    ev.apply_resize_input_event(800, 260, fid.0, false);

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
    let result = eval_one_with_frame(
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
    assert_eq!(result, "OK (nil (\"stb-hook\") \"stb-hook\" \"stb-hook\")");
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
    assert_eq!(
        results[2],
        "OK (\"swb-hist-d1\" \"swb-hist-d2\" \"swb-hist-b\")"
    );
}

#[test]
fn current_window_configuration_roots_window_history_across_gc() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        r#"(let* ((w (selected-window))
                  (b1 (get-buffer-create "wcfg-next-a"))
                  (b2 (get-buffer-create "wcfg-next-b"))
                  (cfg nil))
             (set-window-next-buffers w (list b2 b1))
             (setq cfg (current-window-configuration))
             (set-window-next-buffers w nil)
             (garbage-collect)
             (set-window-configuration cfg)
             (mapcar #'buffer-name (window-next-buffers w)))"#,
    );
    assert_eq!(results[0], r#"OK ("wcfg-next-b" "wcfg-next-a")"#);
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
fn window_end_prefers_last_redisplay_snapshot_when_available() {
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

    {
        let frame = ev.frames.get_mut(fid).expect("frame");
        if let Some(crate::window::Window::Leaf {
            window_start,
            window_end_pos,
            window_end_valid,
            ..
        }) = frame.find_window_mut(wid)
        {
            *window_start = LispCharPos1::ONE;
            *window_end_pos = point_max;
            *window_end_valid = false;
        } else {
            panic!("selected window should be a leaf");
        }

        frame.replace_display_snapshots(vec![crate::window::WindowDisplaySnapshot {
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
            }],
            ..crate::window::WindowDisplaySnapshot::default()
        }]);
    }

    let result = super::builtin_window_end(&mut ev, vec![]).expect("window-end");
    assert_eq!(result, Value::fixnum(12));
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
        frame.replace_display_snapshots(vec![crate::window::WindowDisplaySnapshot {
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
           (list (progn (scroll-up 2) (window-point w))
                 (progn (scroll-down 1) (window-point w))
                 (progn (scroll-left 3) (window-hscroll w))
                 (progn (scroll-right 1) (window-hscroll w))
                 (progn (set-window-point w 9) (recenter 1) (window-start w))))",
    );
    assert_eq!(results[0], "OK (5 3 3 2 7)");
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
               (scroll-down 5)
               (list (= before 1)
                     (> after-up before)
                     (< (window-start w) after-up)
                     (= (window-start w) (window-point w))))))",
    );
    assert_eq!(results[0], "OK (t t t t)");
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

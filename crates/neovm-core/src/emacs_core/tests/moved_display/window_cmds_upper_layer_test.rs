//! Window-command tests that need the core upper layers.
//!
//! Re-homed from neovm-display (display upper-layer re-home): these tests
//! need the core upper layers that the display unit runtime refuses.
use crate::emacs_core::window_cmds::{FrameDomain, SplitWindowSide};
use crate::face::{FontSlant, FontWeight, FontWidth};
use crate::window::display::WindowDisplayContextExt;
use crate::window::{DisplayFrameExt, DisplayFrameManagerExt};
use crate::window::{FrameFullscreen, FrameParam};
use neovm_display::test_utils::window_cmds_host::{
    RecordingDisplayHost, due_gnu_timer, remapped_mono_font_metrics, resolved_frame_font,
};
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

use neovm_display::test_support::authority::{
    window_cmds_eval_one_with_frame_for_test as eval_one_with_frame,
    window_cmds_eval_with_frame_for_test as eval_with_frame,
    window_cmds_eval_with_gui_frame_for_test as eval_with_gui_frame,
};

fn graphical_recenter_context(
    row_height: i64,
    extra_line_spacing: i64,
    body_height: i64,
    mut first_query_form: Option<String>,
) -> Context {
    use crate::window::{
        DisplayRowSnapshot, WindowDisplaySnapshot, WindowLayoutQuery, WindowLayoutQueryOutcome,
        WindowLayoutQueryScope,
    };

    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.set_variable("noninteractive", Value::NIL);
    let buffer = eval.edit.buffers.create_buffer("*bottom-recenter-pixels*");
    eval.edit.buffers.set_current(buffer);
    eval.edit
        .buffers
        .get_mut(buffer)
        .expect("buffer")
        .insert(&"line\n".repeat(100));
    let frame = eval
        .frame_manager_mut()
        .create_frame("F1", 800, 600, buffer);
    let window = eval
        .frame_manager()
        .get(frame)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.initial = false;
        frame.char_height = 10.0;
        frame
            .find_window_mut(window)
            .expect("window")
            .bounds_mut()
            .height = body_height as f32;
    }
    eval.eval_str(
        "(setq mode-line-format nil header-line-format nil tab-line-format nil scroll-margin 0)
         (goto-char 246)",
    )
    .expect("point on line 50, without window chrome");

    // Display-host metrics can differ from the canonical frame font, as with
    // fallback fonts and line spacing. The public command must use those rows.
    eval.install_window_layout_query(move |eval, queried_frame, queried_window, scope| {
        assert_eq!(queried_frame, frame);
        assert_eq!(queried_window, window);
        if let Some(form) = first_query_form.take() {
            // Fontification can run Lisp without editing the source. Point
            // changes must invalidate the whole placement measurement too.
            eval.eval_str(&form).expect("display callback");
        }
        let viewport_start = match eval
            .frame_manager()
            .get(frame)
            .and_then(|frame| frame.find_window(window))
        {
            Some(crate::window::Window::Leaf { window_start, .. }) => *window_start,
            _ => return WindowLayoutQueryOutcome::Unavailable,
        };
        let viewport_count = (body_height as usize).div_ceil(row_height as usize);
        let (start, count) = match scope {
            WindowLayoutQueryScope::Rows { start, count } => (start, count.get()),
            WindowLayoutQueryScope::Pixels { start, height } => {
                (start, height.get().div_ceil(row_height as usize))
            }
            WindowLayoutQueryScope::Viewport => (viewport_start, viewport_count),
            WindowLayoutQueryScope::Position { target } => {
                let through_target =
                    ((target.as_i64() - viewport_start.as_i64()).max(0) / 5 + 1) as usize;
                (viewport_start, through_target.min(viewport_count))
            }
            WindowLayoutQueryScope::TextExtent { .. } => {
                return WindowLayoutQueryOutcome::Unavailable;
            }
        };
        let first_line = (start.as_i64() - 1) / 5;
        let rows: Vec<_> = (first_line..101)
            .take(count)
            .enumerate()
            .map(|(row, line)| DisplayRowSnapshot {
                row: row as i64,
                y: row as i64 * row_height,
                height: row_height,
                extra_line_spacing,
                start_buffer_pos: Some(LispCharPos1::new(line * 5 + 1)),
                end_buffer_pos: Some(LispCharPos1::new((line * 5 + 5).min(501))),
                end_x: 32,
                end_col: 4,
                ..Default::default()
            })
            .collect();
        let end = rows
            .last()
            .and_then(|row| row.end_buffer_pos)
            .unwrap_or(start);
        let layout_freshness = eval.window_display_snapshot_freshness(frame, window, buffer);
        WindowLayoutQueryOutcome::Ready(WindowLayoutQuery::new(
            end,
            Some(WindowDisplaySnapshot {
                window_id: window,
                rows,
                layout_freshness,
                ..Default::default()
            }),
        ))
    })
    .expect("install the layout-query handler");
    eval
}

#[test]
fn focus_redirect_sync_failure_keeps_frame_creation_and_restoration_effects() {
    let mut ev = crate::test_utils::new_context();
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let host = RecordingDisplayHost::new();
    let fail_focus_sync = host.fail_focus_sync.clone();
    fail_focus_sync.set(true);
    ev.set_display_host(Box::new(host));
    let created = ev.eval_str_each(
        "(setq neomacs--test-frame (x-create-frame nil)) (and (frame-live-p neomacs--test-frame) t)",
    );
    assert_eq!(format_eval_result(&created[1]), "OK t");

    let saved = ev.eval_str_each("(setq neomacs--test-config (current-window-configuration))");
    assert!(saved[0].is_ok());
    let before = ev.menu_bar_rebuild_generation();
    let restored = ev.eval_str_each(
        "(condition-case err (set-window-configuration neomacs--test-config) (error (car (cdr err))))",
    );
    assert_eq!(
        format_eval_result(&restored[0]),
        "OK \"focus channel closed\""
    );
    // The accepted restoration still requests its redisplay before signaling.
    assert_ne!(ev.menu_bar_rebuild_generation(), before);
}

#[test]
fn window_tree_primitives_validate_arguments_like_gnu() {
    crate::test_utils::init_test_tracing();
    let out = eval_one_with_frame(
        crate::test_utils::new_context,
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

#[test]
fn frame_selected_window_accepts_any_valid_window_as_its_own_frame() {
    // GNU `Fframe_selected_window` (src/window.c:416-438) takes FRAME-OR-WINDOW
    // and branches three ways: nil, `WINDOW_VALID_P` -> that window's frame,
    // else `CHECK_LIVE_FRAME`.  `WINDOW_VALID_P` covers INTERNAL windows too,
    // which is the branch `window--transpose` (lisp/window-x.el) relies on when
    // it asks for the selected window of the parent window it is rotating.
    // Ground truth taken from GNU Emacs 31.1: a live, an internal and the
    // minibuffer window all answer the frame's selected window, and only a
    // deleted window falls through to the frame type check.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let out = ev
        .eval_str_each(
            "(progn ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
                (windowp (selected-window)))
         (eq (frame-selected-window (selected-window)) (frame-selected-window))
         (window-valid-p (window-parent (selected-window)))
         (window-live-p (window-parent (selected-window)))
         (eq (frame-selected-window (window-parent (selected-window)))
             (frame-selected-window))
         (eq (frame-selected-window (minibuffer-window)) (frame-selected-window))
         (let ((doomed ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (delete-window-internal doomed)
           (list (window-valid-p doomed)
                 (condition-case err (frame-selected-window doomed) (error (car err)))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
    assert_eq!(
        out[1], "OK t",
        "a live window names its own frame, GNU src/window.c:429-430"
    );
    assert_eq!(out[2], "OK t", "an internal window is still window-valid-p");
    assert_eq!(out[3], "OK nil", "and is not window-live-p");
    assert_eq!(
        out[4], "OK t",
        "an internal window names its own frame too -- the window--transpose case"
    );
    assert_eq!(out[5], "OK t", "so does the minibuffer window");
    assert_eq!(
        out[6], "OK (nil wrong-type-argument)",
        "a deleted window is no longer window-valid-p, so it falls through to \
         GNU's CHECK_LIVE_FRAME and is rejected"
    );
}

#[test]
fn window_new_size_slots_decode_a_valid_window_and_default_to_the_selected_one() {
    // GNU decodes WINDOW for `window-new-pixel` / `-total` / `-normal` and all
    // three setters with `decode_valid_window` (src/window.c): nil means the
    // SELECTED window, an internal window is accepted, and anything else --
    // a deleted window, a symbol -- signals `window-valid-p`.
    //
    // Ground truth from GNU Emacs 31.1:
    //   nil-default-roundtrip pixel=4242 total=77 normal=0.5
    //   internal-window       pixel=t normal=nil
    //   dead-window           err=window-valid-p
    //   symbol                err=window-valid-p
    //
    // Neomacs decoded through a private designator helper with no nil arm and
    // no validity check, so a nil WINDOW read back 0/nil and -- worse -- a nil
    // WINDOW *write* was dropped on the floor.  window.el's resize engine sets
    // these slots with an omitted WINDOW constantly.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let out = ev
        .eval_str_each(
            "(progn ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil) t)
         (progn (set-window-new-pixel nil 4242) (window-new-pixel (selected-window)))
         (progn (set-window-new-total nil 77) (window-new-total (selected-window)))
         (progn (set-window-new-normal nil 0.5) (window-new-normal (selected-window)))
         (integerp (window-new-pixel (window-parent (selected-window))))
         (let ((doomed ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (delete-window-internal doomed)
           (condition-case err (window-new-pixel doomed) (error (car (cdr err)))))
         (condition-case err (window-new-pixel 'foo) (error (car (cdr err))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
    assert_eq!(
        out[1], "OK 4242",
        "a nil WINDOW must write to, and read back from, the selected window"
    );
    assert_eq!(out[2], "OK 77", "same for the total-size slot");
    assert_eq!(out[3], "OK 0.5", "and for the normal-size slot");
    assert_eq!(
        out[4], "OK t",
        "decode_valid_window accepts an internal window"
    );
    assert_eq!(
        out[5], "OK window-valid-p",
        "a deleted window is rejected against window-valid-p, not silently read as 0"
    );
    assert_eq!(out[6], "OK window-valid-p", "and so is a non-window");
}

#[test]
fn window_old_buffer_decodes_any_window_including_internal_and_deleted_ones() {
    // GNU decodes WINDOW with `decode_any_window` -- `CHECK_WINDOW`, the
    // loosest of its three decoders -- so "WINDOW can be any window and
    // defaults to the selected one" (src/window.c).  Internal and DELETED
    // windows are answers, not errors; only a non-window signals, and it
    // signals `windowp` rather than `window-live-p` or `window-valid-p`.
    //
    // Measured on GNU Emacs 31.1:
    //   decoder-arms: nil="ok" internal="ok" dead="ok"
    //   rejects:      symbol=windowp string=windowp buffer=windowp
    //
    // Neomacs decoded with `window-live-p`, so an internal or deleted window
    // signalled where GNU answers.
    //
    // NOT asserted here: GNU returns t for a window that was live and has
    // since been deleted, and the old buffer itself otherwise.  That needs
    // per-window `old_buffer`/`change_stamp` state which neomacs does not keep
    // yet -- see the audit note at crates/neovm-core/src/window/mod.rs and
    // `drafts/window-system-audit.md` Phase 4.  Returning nil is what this
    // build actually knows; inventing a `t` here would make the test pass
    // while the state behind it stayed missing.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let out = ev
        .eval_str_each(
            "(progn ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil) t)
         (condition-case err (progn (window-old-buffer nil) 'ok) (error (car (cdr err))))
         (condition-case err (progn (window-old-buffer (selected-window)) 'ok)
           (error (car (cdr err))))
         (condition-case err (progn (window-old-buffer (window-parent (selected-window))) 'ok)
           (error (car (cdr err))))
         (let ((doomed ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (delete-window-internal doomed)
           (condition-case err (progn (window-old-buffer doomed) 'ok) (error (car (cdr err)))))
         (condition-case err (window-old-buffer 'foo) (error (car (cdr err))))
         (condition-case err (window-old-buffer (current-buffer)) (error (car (cdr err))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK ok", "nil defaults to the selected window");
    assert_eq!(out[2], "OK ok", "a live window is accepted");
    assert_eq!(
        out[3], "OK ok",
        "decode_any_window accepts an internal window"
    );
    assert_eq!(
        out[4], "OK ok",
        "and a deleted window -- GNU answers for it rather than signalling"
    );
    assert_eq!(
        out[5], "OK windowp",
        "a non-window signals windowp, not window-live-p"
    );
    assert_eq!(out[6], "OK windowp", "a buffer is not a window either");
}

#[test]
fn coordinates_in_window_p_decodes_a_live_window_before_its_coordinates() {
    // GNU: `w = decode_live_window (window); CHECK_CONS (coordinates);`
    // (src/window.c) -- the WINDOW is decoded FIRST, and `decode_live_window`
    // rejects an internal window, a deleted window, a frame and a symbol alike,
    // all against `window-live-p`.
    //
    // Measured on GNU Emacs 31.1:
    //   nil (0 . 0)   live (0 . 0)
    //   internal ERR:window-live-p   dead ERR:window-live-p
    //   frame    ERR:window-live-p   symbol ERR:window-live-p
    //
    // Neomacs never decoded the window itself: it delegated to
    // `window-total-width`, which decodes with `window-valid-p`, so an internal
    // window was ACCEPTED and everything else reported the wrong predicate.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let out = ev
        .eval_str_each(
            "(progn ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil) t)
         (coordinates-in-window-p '(0 . 0) nil)
         (coordinates-in-window-p '(0 . 0) (selected-window))
         (condition-case err (coordinates-in-window-p '(0 . 0) (window-parent (selected-window)))
           (error (car (cdr err))))
         (let ((doomed ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (delete-window-internal doomed)
           (condition-case err (coordinates-in-window-p '(0 . 0) doomed) (error (car (cdr err)))))
         (condition-case err (coordinates-in-window-p '(0 . 0) (selected-frame))
           (error (car (cdr err))))
         (condition-case err (coordinates-in-window-p '(0 . 0) 'foo) (error (car (cdr err))))
         (condition-case err (coordinates-in-window-p 'not-a-cons 'foo) (error (car (cdr err))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK t");
    assert_eq!(out[1], "OK (0 . 0)", "nil means the selected window");
    assert_eq!(out[2], "OK (0 . 0)", "a live window answers");
    assert_eq!(
        out[3], "OK window-live-p",
        "an internal window is NOT live, so decode_live_window rejects it"
    );
    assert_eq!(out[4], "OK window-live-p", "nor is a deleted window");
    assert_eq!(out[5], "OK window-live-p", "a frame is not a window at all");
    assert_eq!(out[6], "OK window-live-p", "nor is a symbol");
    assert_eq!(
        out[7], "OK window-live-p",
        "and the WINDOW is decoded before the coordinates are type-checked"
    );
}

#[test]
fn lower_frame_decodes_a_live_frame() {
    // GNU `Flower_frame` opens with `decode_live_frame (frame)`
    // (src/frame.c), so a window or a symbol is rejected against
    // `frame-live-p` before any lowering is attempted.  Measured on GNU
    // Emacs 31.1: nil => nil, frame => nil, live-window => ERR:frame-live-p,
    // symbol => ERR:frame-live-p.
    //
    // Neomacs accepted anything: the subr was a stub that checked its arity
    // and returned nil, so a caller passing the wrong object got silence.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let out = ev
        .eval_str_each(
            "(lower-frame)
         (lower-frame nil)
         (lower-frame (selected-frame))
         (condition-case err (lower-frame (selected-window)) (error (car (cdr err))))
         (condition-case err (lower-frame 'foo) (error (car (cdr err))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK nil", "omitted means the selected frame");
    assert_eq!(out[1], "OK nil", "and so does nil");
    assert_eq!(out[2], "OK nil", "a live frame is accepted");
    assert_eq!(
        out[3], "OK frame-live-p",
        "a window is not a frame -- GNU signals rather than doing nothing"
    );
    assert_eq!(out[4], "OK frame-live-p", "nor is a symbol");
}

#[test]
fn window_old_buffer_reports_a_deleted_window_as_a_stale_epoch() {
    // GNU answers `window-old-buffer` from two slots (src/window.c):
    //
    //     return (NILP (w->old_buffer)                                   ? Qnil
    //             : (w->change_stamp != WINDOW_XFRAME (w)->change_stamp) ? Qt
    //             : w->old_buffer);
    //
    // `w->old_buffer` is written in exactly two places: when window change
    // functions run (`window_change_record_windows`, which also stamps the
    // window), and when a live window is DELETED -- `Fdelete_window_internal`
    // does `wset_old_buffer (w, w->contents)` before clearing contents.
    //
    // Measured on GNU Emacs 31.1, batch:
    //     live window, created since the last record => nil
    //     the same window after delete-window        => t
    //
    // The `t` is not arbitrary: the window was created after the last record
    // so its own stamp is 0, while the frame's stamp is already non-zero, and
    // deletion gives it an old_buffer.  Recording the buffer WITHOUT the
    // stamps would answer with the buffer here and diverge.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let out = ev
        .eval_str_each(
            "(let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (list (window-old-buffer w)
                 (progn (delete-window-internal w) (window-old-buffer w))))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        out[0], "OK (nil t)",
        "a window created since the last record has no old buffer; once deleted \
         it has one, but from an older epoch, so GNU answers t"
    );
}

#[test]
fn window_old_buffer_reports_the_recorded_buffer_after_a_change_epoch() {
    // The third arm of GNU's `window-old-buffer`.  Once the window change
    // functions have run, `window_change_record_windows` has stamped each live
    // leaf and set its `old_buffer` to the buffer it is showing
    // (`src/window.c`), so the window's epoch matches its frame's and the
    // recorded buffer is the answer -- neither nil nor t.
    //
    // Unobservable from `--batch` Lisp, because neomacs runs the change
    // functions from redisplay and batch has none; drive the recorder directly,
    // which is what the command loop does at
    // `runtime/eval/command_loop.rs`.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buffer = ev
        .edit
        .buffers
        .current_buffer()
        .expect("current buffer")
        .id();

    // Before any epoch: the window was created since the last record, so it has
    // no old buffer at all.
    let before = ev
        .eval_str_each("(window-old-buffer (selected-window))")
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(before[0], "OK nil", "no epoch has been recorded yet");

    neovm_eval::emacs_core::eval::lifecycle::run_redisplay_window_change_hooks(&mut ev)
        .expect("window change hooks");

    let after = ev
        .eval_str_each("(bufferp (window-old-buffer (selected-window)))")
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        after[0], "OK t",
        "after an epoch the window reports the buffer it was showing, not t"
    );
    assert_eq!(
        ev.frame_manager().window_old_buffer(
            ev.frame_manager()
                .selected_frame()
                .expect("frame")
                .selected_window
        ),
        crate::window::WindowOldBuffer::Recorded(buffer),
        "and it is the buffer the window actually shows"
    );
}

#[test]
fn window_old_buffer_survives_a_same_epoch_configuration_round_trip() {
    // A configuration saved and restored WITHIN one epoch keeps the window's
    // old buffer.  GNU gates that on the frame's stamp (`src/window.c`):
    //
    //     if (data->change_stamp == f->change_stamp)
    //       /* ... only if the configuration was saved and restored in between
    //          two redisplay cycles ... */
    //       w->old_buffer = p->old_buffer;
    //
    // so the window's own stamp still matches its frame's and the answer is the
    // BUFFER.  The docstring's "t if WINDOW has been restored from a window
    // configuration" describes the other side of that gate -- a restore ACROSS
    // epochs -- which this does not exercise.
    //
    // Deliberately not asserted: that cross-epoch case.  It is unobservable
    // from `--batch` on the GNU side, because the change functions never run
    // there and a saved old_buffer is nil to begin with.  Asserting an
    // inference from the docstring instead of a measured answer is exactly how
    // the wrong expectation got written here the first time.
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    // Materialise the frame first: the hook runner walks `frame_list`, so
    // running it against a context whose selected frame has not been demanded
    // yet records nothing at all.
    let _ = ev.eval_str_each("(selected-window)");

    // Open an epoch so the window has a real record to restore.
    neovm_eval::emacs_core::eval::lifecycle::run_redisplay_window_change_hooks(&mut ev)
        .expect("window change hooks");
    let recorded = ev
        .eval_str_each("(bufferp (window-old-buffer (selected-window)))")
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        recorded[0], "OK t",
        "precondition: the epoch recorded a buffer"
    );

    let out = ev
        .eval_str_each(
            "(let ((cfg (current-window-configuration)))
               (set-window-configuration cfg)
               (window-old-buffer (selected-window)))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(
        out[0], "OK #<buffer 1>",
        "saved and restored inside one epoch, the window still reports the \
         buffer its record named"
    );
}

#[test]
fn window_buffer_returns_buffer() {
    crate::test_utils::init_test_tracing();
    let r = eval_one_with_frame(crate::test_utils::new_context, "(bufferp (window-buffer))");
    assert_eq!(r, "OK t");
}

#[test]
fn set_window_start_and_read() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
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
fn set_window_point_and_read() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
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
        crate::test_utils::new_context,
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
        crate::test_utils::new_context,
        r#"(let ((other ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
        crate::test_utils::new_context,
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
        crate::test_utils::new_context,
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
fn get_buffer_window_prefers_selected_window_when_buffer_is_displayed_twice() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(
        crate::test_utils::new_context,
        r#"(let ((shared (get-buffer-create "*get-buffer-window-shared*")))
             (unwind-protect
                 (progn
                   (set-window-buffer (selected-window) shared)
                   (let ((second ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
                     (set-window-buffer second shared)
                     (select-window second)
                     (eq (get-buffer-window shared) (selected-window))))
               (kill-buffer shared)))"#,
    );
    assert_eq!(result, "OK t");
}

#[test]
fn minibuffer_window_from_window_list_supports_basic_accessors() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
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
fn split_window_internal_creates_new() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(crate::test_utils::new_context,
        "((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
         (length (window-list))",
    );
    assert!(results[0].starts_with("OK "));
    assert_eq!(results[1], "OK 2");
}

#[test]
fn split_window_internal_enforces_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let out = ev
        .eval_str_each(
            "(condition-case err
             (split-window-internal (selected-window) nil nil nil nil nil)
           (error (car err)))
         (let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (window-live-p w))",
        )
        .iter()
        .map(format_eval_result)
        .collect::<Vec<_>>();
    assert_eq!(out[0], "OK wrong-number-of-arguments");
    assert_eq!(out[1], "OK t");
}

#[test]
fn select_window_works() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(crate::test_utils::new_context,
        "(let ((new-win ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (select-window new-win)
           (eq (selected-window) new-win))",
    );
    assert_eq!(results[0], "OK t");
}

#[test]
fn select_window_accepts_minibuffer_window_and_switches_current_buffer() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
        "(let ((mw (minibuffer-window)))
           (select-window mw)
           (list (eq (selected-window) mw)
                 (window-minibuffer-p (selected-window))
                 (eq (current-buffer) (window-buffer mw))))",
    );
    assert_eq!(results[0], "OK (t t t)");
}

#[test]
fn select_window_updates_current_buffer_to_selected_window_buffer() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(crate::test_utils::new_context,
        "(save-current-buffer
           (let* ((b1 (get-buffer-create \"sw-curbuf-a\"))
                  (b2 (get-buffer-create \"sw-curbuf-b\")))
             (set-window-buffer nil b1)
             (let ((w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
               (set-window-buffer w2 b2)
               (select-window w2)
               (buffer-name (current-buffer)))))",
    );
    assert_eq!(result, "OK \"sw-curbuf-b\"");
}

#[test]
fn select_window_runs_buffer_list_update_hook_unless_norecord() {
    crate::test_utils::init_test_tracing();
    let result = eval_one_with_frame(crate::test_utils::new_context,
        "(let* ((w1 (selected-window))
                (b2 (get-buffer-create \"sw-hook-buf\"))
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) w1 (/ (window-pixel-height w1) 2) nil nil))
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
fn window_use_time_and_old_state_queries_match_batch_defaults_and_error_predicates() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
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
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil))
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
    // The third entry is `window-old-buffer`, and its predicate is `windowp`
    // rather than `window-live-p`: GNU decodes it with `decode_any_window`
    // (`CHECK_WINDOW`) where its four neighbours here use `decode_live_window`.
    // Confirmed against GNU Emacs 31.1 for this exact argument:
    //   (window-old-buffer 999999) => (wrong-type-argument windowp 999999)
    //   (window-use-time   999999) => (wrong-type-argument window-live-p 999999)
    //   (window-old-point  999999) => (wrong-type-argument window-live-p 999999)
    // This expectation previously pinned `window-live-p` for all five, which
    // is what let the wrong decoder survive here.
    assert_eq!(
        out[2],
        "OK ((wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument windowp 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-number-of-arguments window-use-time 2) (wrong-number-of-arguments window-old-point 2) (wrong-number-of-arguments window-old-buffer 2) (wrong-number-of-arguments window-prev-buffers 2) (wrong-number-of-arguments window-next-buffers 2))"
    );
}

#[test]
fn window_bump_use_time_shared_state_smoke() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let out = ev
        .eval_str_each(
            "(let* ((w1 (selected-window))
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
fn next_window_cycles() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(crate::test_utils::new_context,
        "(let ((w1 (selected-window)))
           ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
           (let ((w2 (next-window)))
             (null (eq w1 w2))))",
    );
    assert_eq!(results[0], "OK t");
}

#[test]
fn next_previous_window_enforce_max_arity() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let out = ev
        .eval_str_each(
            "(condition-case err (next-window nil nil nil nil) (error (car err)))
         (condition-case err (previous-window nil nil nil nil) (error (car err)))
         (let ((w1 (selected-window)))
           ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
           (windowp (next-window w1 nil nil)))
         (let ((w1 (selected-window)))
           ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
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
    let results = eval_with_frame(crate::test_utils::new_context,
        "((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)
         (let ((w (previous-window)))
           (windowp w))",
    );
    assert_eq!(results[1], "OK t");
}

#[test]
fn frame_identity_builtins_match_gnu_batch_startup_defaults() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
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
fn live_default_font_change_on_child_frame_skips_top_level_geometry_hints() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
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
    }
    ev.frame_manager_mut().select_frame(parent_id);
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
    let child = crate::emacs_core::window_cmds::builtin_x_create_frame(&mut ev, vec![params])
        .expect("x-create-frame");
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

    let child_frame = ev.frame_manager().get(child_id).expect("child frame");
    assert_eq!(child_frame.char_width, 13.0);
    assert_eq!(child_frame.char_height, 31.0);
    assert!(
        geometry_hints.borrow().is_empty(),
        "child frames are composited by the parent and must not emit top-level geometry hints"
    );
}

#[test]
fn modify_frame_parameters_buffer_lists_use_gnu_special_storage() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
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
fn window_right_divider_width_only_applies_to_non_rightmost_windows() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_gui_frame(crate::test_utils::new_context,
        "(modify-frame-parameters (selected-frame) '((right-divider-width . 6)))
         (let ((left (selected-window))
               (right ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-width (selected-window)) 2) 'right nil)))
           (list (window-right-divider-width left)
                 (window-right-divider-width right)))",
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK (6 0)");
}

#[test]
fn live_default_font_change_updates_gui_geometry_hints() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
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
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
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
        ev.frame_manager()
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );
}

#[test]
fn switch_to_buffer_changes_window() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
        "(get-buffer-create \"other-buf\")
         (switch-to-buffer \"other-buf\")
         (bufferp (window-buffer))",
    );
    assert_eq!(results[2], "OK t");
}

#[test]
fn set_window_buffer_works() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
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
        crate::test_utils::new_context,
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
        crate::test_utils::new_context,
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
        crate::test_utils::new_context,
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
        crate::test_utils::new_context,
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
        crate::test_utils::new_context,
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

#[test]
fn window_end_greater_than_start() {
    crate::test_utils::init_test_tracing();
    // Check that window-end and window-start return valid positions.
    // Use >= since they can be equal for small/empty visible regions.
    let r = eval_one_with_frame(
        crate::test_utils::new_context,
        "(progn (insert \"hello\\nworld\\n\") (goto-char (point-min)) (list (window-start) (window-end) (>= (window-end) (window-start))))",
    );
    assert!(r.starts_with("OK (1 "), "expected (1 N t), got: {r}");
}

#[test]
fn set_window_buffer_matches_window_and_buffer_designator_errors() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    let dead = Value::make_buffer(ev.edit.buffers.create_buffer("swb-dead"));
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
    let mut ev = crate::test_utils::new_context();
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
        crate::test_utils::new_context,
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
    let mut ev = crate::test_utils::new_context();
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    ev.eval_str_each("(erase-buffer) (insert \"a\\nb\\nc\\nd\\ne\\nf\\ng\\nh\\n\") (goto-char 7)");
    let wid = ev.frame_manager().get(fid).expect("frame").selected_window;
    if let Some(crate::window::Window::Leaf { point, .. }) = ev
        .frame_manager_mut()
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
fn graphical_bottom_recenter_fits_measured_rows_without_moving_point() {
    // The 100-pixel text body fits five measured 20-pixel rows, including
    // point's row, instead of nine canonical 10-pixel rows above point.
    let mut eval = graphical_recenter_context(20, 0, 100, None);
    let result = eval
        .eval_str(
            "(progn (recenter -1)
                    (list (line-number-at-pos (window-start))
                          (line-number-at-pos) (point)
                          (window-body-height nil t) (window-body-height)))",
        )
        .expect("graphical bottom recenter");
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&result),
        "(46 50 246 100 10)"
    );
}

#[test]
fn graphical_bottom_recenter_keeps_start_when_row_advance_fills_body() {
    let mut eval = graphical_recenter_context(100, 20, 100, None);
    // GNU checks the remaining pixel budget before excluding the final row's
    // extra spacing. A full-body row advance leaves no backward budget, so
    // recenter returns without changing the existing viewport or point.
    let result = eval
        .eval_str(
            "(progn (set-window-start nil 236)
                    (recenter -1)
                    (list (line-number-at-pos (window-start))
                          (line-number-at-pos) (point)
                          (window-body-height nil t) (window-body-height)))",
        )
        .expect("full-body row leaves recenter start unchanged");
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&result),
        "(48 50 246 100 10)"
    );
}

#[test]
fn graphical_bottom_recenter_remeasures_point_changed_by_display_callback() {
    let mut eval = graphical_recenter_context(20, 0, 30, Some("(goto-char 296)".to_owned()));
    // The callback moves point from line 50 to line 60 without changing the
    // buffer's modification tick. Only its own 20-pixel row fits the 30-pixel
    // body, so this measurement settles with one query. Placement must follow
    // the live point without restoring it or publishing its old placement.
    let result = eval
        .eval_str(
            "(progn (recenter -1)
                    (list (line-number-at-pos (window-start))
                          (line-number-at-pos) (point)
                          (window-body-height nil t) (window-body-height)))",
        )
        .expect("point mutation restarts recenter measurement");
    assert_eq!(
        neovm_eval::emacs_core::print::print_value(&result),
        "(60 60 296 30 3)"
    );
}

#[test]
fn graphical_bottom_recenter_keeps_its_target_when_display_callback_selects_another_window() {
    for same_frame in [true, false] {
        let mut eval = graphical_recenter_context(
            20,
            0,
            100,
            Some(
                "(progn (goto-char 296)
                        (select-window recenter-other-window)
                        (make-local-variable 'scroll-margin)
                        (setq scroll-margin 0)
                        (save-current-buffer
                          (set-buffer (window-buffer recenter-original-window))
                          (goto-char 246)))"
                    .to_owned(),
            ),
        );
        let frame = eval.frame_manager().selected_frame().expect("frame").id;
        let original = eval
            .frame_manager()
            .get(frame)
            .expect("frame")
            .selected_window;
        let other_buffer = eval.edit.buffers.create_buffer("*other-recenter-window*");
        eval.edit
            .buffers
            .get_mut(other_buffer)
            .expect("other buffer")
            .insert("Other window\n");
        let other = if same_frame {
            eval.frame_manager_mut()
                .split_window(
                    frame,
                    original,
                    crate::window::SplitDirection::Horizontal,
                    other_buffer,
                    None,
                    crate::window::SplitPlacement::AfterTarget,
                )
                .expect("split the graphical window horizontally")
        } else {
            let other_frame = eval
                .frame_manager_mut()
                .create_frame("F2", 800, 600, other_buffer);
            let other_frame = eval
                .frame_manager_mut()
                .get_mut(other_frame)
                .expect("other frame");
            other_frame.set_window_system(Some(Value::symbol("neo")));
            other_frame.initial = false;
            other_frame.selected_window
        };
        eval.set_variable("recenter-original-window", Value::make_window(original.0));
        eval.set_variable("recenter-other-window", Value::make_window(other.0));

        // Recenter retains the original window and its buffer-local margin.
        // Its callback saves point at line 60 by selecting a different window,
        // then leaves buffer point on line 50 without changing window point.
        // A retry must keep that target and policy without stealing selection
        // or replacing the original window's independent point with buffer point.
        let result = eval
            .eval_str(
                "(progn (select-window recenter-original-window)
                    (make-local-variable 'scroll-margin)
                    (setq scroll-margin 1)
                    (recenter -1)
                    (list (eq (selected-window) recenter-other-window)
                          (eq (current-buffer) (window-buffer recenter-other-window))
                          (window-point recenter-original-window)
                          (save-current-buffer
                            (set-buffer (window-buffer recenter-original-window))
                            (list (line-number-at-pos (window-start recenter-original-window))
                                  (line-number-at-pos (window-point recenter-original-window))
                                  (line-number-at-pos)))))",
            )
            .expect("recenter follows its original window across callback selection");
        assert_eq!(
            neovm_eval::emacs_core::print::print_value(&result),
            "(t t 296 (57 60 50))",
            "same_frame={same_frame}"
        );
    }
}

#[test]
fn only_full_frame_recenter_crosses_the_menu_bar_rebuild_boundary() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buffer = ev.edit.buffers.create_buffer("recenter-menu-boundary");
    ev.edit.buffers.set_current(buffer);
    ev.frame_manager_mut().create_frame("F1", 800, 600, buffer);
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
    let mut ev = crate::test_utils::new_context();
    let buffer = ev.edit.buffers.create_buffer("delete-window-menu-boundary");
    ev.edit.buffers.set_current(buffer);
    let frame_id = ev.frame_manager_mut().create_frame("F1", 800, 600, buffer);
    ev.eval_str("((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)")
        .expect("split the selected window");
    let deleted_window = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .window_list()
        .into_iter()
        .nth(1)
        .expect("second live window");

    let initial = ev.menu_bar_rebuild_generation();
    crate::emacs_core::window_cmds::builtin_delete_window_internal(
        &mut ev,
        vec![crate::emacs_core::window_cmds::window_value(deleted_window)],
    )
    .expect("delete second window");

    assert_ne!(ev.menu_bar_rebuild_generation(), initial);
}

#[test]
fn only_nonselected_window_redisplay_crosses_the_menu_bar_rebuild_boundary() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buffer = ev
        .edit
        .buffers
        .create_buffer("window-redisplay-menu-boundary");
    ev.edit.buffers.set_current(buffer);
    let frame_id = ev.frame_manager_mut().create_frame("F1", 800, 600, buffer);
    ev.eval_str("((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)")
        .expect("split the selected window");
    let windows = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .window_list();
    let selected = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
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

#[test]
fn recenter_backward_multiple_lines_scans_bytes_to_window_start() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
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

#[test]
fn recenter_counts_screen_lines_not_buffer_lines_over_invisible_text() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
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
        crate::test_utils::new_context,
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

#[test]
fn consecutive_scroll_up_before_redisplay_advances_from_window_start() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buffer_id = ev.edit.buffers.create_buffer("*queued-scroll*");
    ev.edit.buffers.set_current(buffer_id);
    ev.edit
        .buffers
        .insert_into_buffer(buffer_id, &"line\n".repeat(200));
    let frame_id = ev
        .frame_manager_mut()
        .create_frame("F1", 800, 600, buffer_id);
    let window_id = ev
        .frame_manager()
        .get(frame_id)
        .expect("scroll frame")
        .selected_window;

    // The last redisplay showed line 50 at the top with point on line 51.
    // Each line occupies five chars/bytes, including its newline.
    let start = LispCharPos1::new(246);
    let point = LispCharPos1::new(251);
    let displayed_end = LispCharPos1::new(421);
    ev.edit
        .buffers
        .goto_buffer_emacs_byte_pos(buffer_id, EmacsBytePos::new(250));
    let (buffer_z_char, buffer_z_byte) = {
        let buffer = ev.edit.buffers.get(buffer_id).expect("scroll buffer");
        (
            buffer.point_max_char_pos().to_lisp(),
            buffer.point_max_emacs_byte_pos(),
        )
    };
    let (wm_frames, wm_buffers) = ev.frame_manager_and_buffers_mut();
    let window = wm_frames
        .get_mut(frame_id)
        .expect("scroll frame")
        .find_window_mut(window_id)
        .expect("scroll window");
    crate::window::window_markers::set_window_start_with_marker(wm_buffers, window, start);
    crate::window::window_markers::set_window_point_with_marker(wm_buffers, window, point);
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

#[test]
fn scroll_down_from_eob_pulls_point_to_last_visible_line_like_gnu() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
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

#[test]
fn scroll_down_skips_invisible_logical_lines_when_measuring_a_page() {
    crate::test_utils::init_test_tracing();
    let results = eval_with_frame(
        crate::test_utils::new_context,
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

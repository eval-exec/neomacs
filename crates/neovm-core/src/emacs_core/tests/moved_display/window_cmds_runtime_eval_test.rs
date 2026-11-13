//! Re-homed FULL display window-command tests (runtime eval terminal
//! harnesses). Test bodies are verbatim from the display source; the two
//! harness helpers move once with their only users.

use crate::emacs_core::DisplayHost;
use crate::emacs_core::window_cmds::FrameDomain;
use crate::face::{FontSlant, FontWeight, FontWidth};
use crate::test_utils::{runtime_startup_context, runtime_startup_eval_all};
use crate::window::DisplayFrameExt;
use crate::window::DisplayFrameManagerExt;
use crate::window::display::WindowDisplayContextExt;
use crate::window::{FrameFullscreen, FrameParam};
use neovm_edit::buffer::{EmacsBytePos, LispCharPos1};
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::GuiFrameHostRequest;
use neovm_eval::emacs_core::eval::{FontPxProbeResult, GuiFrameHostSize, ResolvedFrameFont};
use neovm_eval::emacs_core::print::format_eval_result;
use neovm_value::error::FlowResultExt as _;
use neovm_value::heap_types::LispString;
use neovm_value::value::Value;
use std::cell::Cell;
use std::cell::RefCell;
use std::fs;
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

#[test]
fn windowp_true_for_stale_deleted_window() {
    crate::test_utils::init_test_tracing();
    let r = runtime_eval_one_with_usable_terminal(
        "(let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (delete-window w)
           (windowp w))",
    );
    assert_eq!(r, "OK t");
}

#[test]
fn window_buffer_returns_nil_for_stale_deleted_window() {
    crate::test_utils::init_test_tracing();
    let r = runtime_eval_one_with_usable_terminal(
        "(let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (delete-window w)
           (window-buffer w))",
    );
    assert_eq!(r, "OK nil");
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
fn window_list_1_stale_window_signals_wrong_type_argument() {
    crate::test_utils::init_test_tracing();
    let r = runtime_eval_one_with_usable_terminal(
        "(let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
fn an_integer_is_not_a_frame_even_when_it_matches_a_frame_id() {
    crate::test_utils::init_test_tracing();
    // `3df6e81a4` removed the `Fixnum(n) => Id(n)` designator arm from the
    // window decoders and from `frame-visible-p`, but `framep` and
    // `frame-live-p` kept theirs:
    //
    //     ValueKind::Fixnum(n) => n as u64,
    //     ...
    //     Ok(Value::bool_val(frames.get(FrameId(id)).is_some()))
    //
    // Probing `(framep 1)` in batch answers nil and looks fine -- but only
    // because the sole frame's internal `FrameId` does not happen to be 1.
    // That is dormant by luck, not by design: hand it an integer that DOES
    // match a live frame's id and the predicate says t, which is the same type
    // confusion `3df6e81a4` removed elsewhere.  GNU's `framep` is a
    // pseudovector tag test and no integer can pass it.
    //
    // So this test asks the question with the id the frame actually has,
    // rather than with a guess that happens to miss.
    let mut ev = runtime_startup_context();
    let buf = ev
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let fid = ev.frame_manager_mut().create_frame("probe", 800, 600, buf);
    let as_integer = Value::fixnum(fid.0 as i64);

    assert_eq!(
        crate::emacs_core::frame::builtin_framep(&mut ev, vec![as_integer])
            .expect("framep must not signal"),
        Value::NIL,
        "an integer equal to a live frame's id is still not a frame"
    );
    assert_eq!(
        crate::emacs_core::frame::builtin_frame_live_p(&mut ev, &vec![as_integer])
            .expect("frame-live-p must not signal"),
        Value::NIL,
        "an integer equal to a live frame's id is still not a live frame"
    );
    // the real frame object still answers t, so the tag path is intact
    assert_eq!(
        crate::emacs_core::frame::builtin_framep(&mut ev, vec![Value::make_frame(fid.0)])
            .expect("framep must not signal"),
        Value::T
    );
}

#[test]
fn live_frame_subrs_reject_a_deleted_frame_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU's two frame decoders differ in more than the predicate they name
    // (`src/frame.c:98,107`):
    //
    //     decode_live_frame: if (NILP (frame)) frame = selected_frame;
    //                        CHECK_LIVE_FRAME (frame);   /* frame-live-p */
    //     decode_any_frame:  if (NILP (frame)) frame = selected_frame;
    //                        CHECK_FRAME (frame);        /* framep       */
    //
    // `CHECK_FRAME` passes a DELETED frame -- it is still a frame object -- so
    // the `Any` callers are right to test only the type tag.  `CHECK_LIVE_FRAME`
    // is `FRAME_LIVE_P`, which tests `f->terminal`, so the `Live` callers must
    // consult the frame table and reject a deleted frame.
    //
    // `expect_frame_in_domain` only tag-tests, so the `Live` callers still
    // built on it accepted a deleted frame.  This cannot be shown from Lisp in
    // batch -- `make-frame` fails there, in GNU and neomacs alike, so no
    // deleted frame can be constructed -- which is why it is tested from Rust,
    // where `create_frame` + `delete_frame` can build one.
    let mut ev = runtime_startup_context();
    let buf = ev
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let doomed = ev.frame_manager_mut().create_frame("doomed", 800, 600, buf);
    let dead = Value::make_frame(doomed.0);
    assert!(ev.frame_manager_mut().delete_frame(doomed).was_deleted());
    assert!(
        ev.frame_manager().get(doomed).is_none(),
        "precondition: the frame must really be gone from the table"
    );

    // `framep` still answers t for it, exactly as GNU's CHECK_FRAME would.
    let still_a_frame = crate::emacs_core::frame::builtin_framep(&mut ev, vec![dead])
        .expect("framep must not signal");
    assert_eq!(
        still_a_frame,
        Value::NIL,
        "a deleted frame is gone from the table"
    );

    // `frame--set-was-invisible`, `frame-after-make-frame` and
    // `frame-font-cache` share the same defect but live in a private module, so
    // `lower-frame` stands in for the group here.
    for (name, result) in [(
        "lower-frame",
        crate::emacs_core::display_entries::builtin_lower_frame(&mut ev, vec![dead]),
    )] {
        match result.kinded() {
            Err(neovm_value::error::FlowKind::Signal(sig)) => {
                assert_eq!(sig.symbol_name(), "wrong-type-argument", "{name}");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("frame-live-p"), dead],
                    "{name} must name frame-live-p for a deleted frame"
                );
            }
            other => panic!("{name}: expected frame-live-p signal, got {other:?}"),
        }
    }
}

#[test]
fn split_delete_window_invalid_designators_signal_error() {
    crate::test_utils::init_test_tracing();
    let results = runtime_eval_with_usable_terminal(
        "(condition-case err
             (split-window-internal 999999 (/ (window-pixel-height 999999) 2) nil nil)
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
fn select_window_swaps_buffer_point_between_windows() {
    crate::test_utils::init_test_tracing();
    let result = runtime_eval_one_with_usable_terminal(
        "(let ((w1 (selected-window)))
           (set-buffer (window-buffer w1))
           (insert \"0123456789abcdefghijklmnopqrstuvwxyz\")
           (let ((w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) w1 (/ (window-pixel-height w1) 2) nil nil)))
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
fn other_window_without_selected_frame_returns_nil() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let results = ev.eval_str_each("(other-window 1)");
    assert_eq!(format_eval_result(&results[0]), "OK nil");
}

/// `coordinates-in-window-p` measures from the window's PIXEL edge.
///
/// GNU converts the canonical-character COORDINATES to pixels
/// (`FRAME_PIXEL_Y_FROM_CANON_Y`) and compares them against
/// `WINDOW_TOP_EDGE_Y`, which is the internal border plus
/// `WINDOW_TOP_PIXEL_EDGE` (`src/window.h:797`) -- the window's `pixel_top`,
/// not its `top_line`.
///
/// The two are different origins whenever the frame has a top margin whose
/// rows cost no pixels, which is a terminal menu bar: the shipped binary and
/// GNU both report `top_line` 1 beside `pixel_top` 0 for the upper window
/// after `(split-window-below)` on the startup frame (verified against both).
/// Measuring from `top_line`, as this did, shifted every answer down a row.
/// Measured on GNU Emacs 31.0.50, `emacs -Q --batch`, against neomacs before
/// this fix:
///
///     (0 . 0)   GNU (0 . 0)     neomacs nil
///     (2 . 2)   GNU (2 . 2)     neomacs (2 . 1)
///     (0 . 11)  GNU mode-line   neomacs (0 . 10)
///     (0 . 12)  GNU nil         neomacs mode-line
///
/// The chrome comparisons had the same unit confusion from the other side:
/// `chrome_height_pixels` returns PIXELS and was compared against a
/// character-unit `y`, which agrees only while a character cell is one pixel
/// tall -- true on the terminal this was measured on, false on a GUI frame.
/// Working in pixels throughout settles both.
///
/// The state has to be built here rather than reached through `split-window`:
/// this harness's frame carries `menu-bar-lines` nil, so its margin is 0 and
/// the two origins coincide, which is why the bug is invisible to it.  That
/// gap between the harness frame and the shipped binary's is recorded
/// separately.
#[test]
fn coordinates_in_window_p_measures_from_the_pixel_edge_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::runtime_startup_context();

    // Realize the margin the way a terminal frame does: the character row
    // moves down, the pixel origin does not.
    let fid = ev
        .frame_manager()
        .selected_frame()
        .expect("a selected frame")
        .id;
    {
        let frame = ev
            .frame_manager_mut()
            .get_mut(fid)
            .expect("the selected frame");
        frame.set_parameter(Value::symbol("menu-bar-lines"), Value::fixnum(1));
        frame.root_window_mut().set_top_line(1);
    }

    let out = ev.eval_str_each(
        "(progn
           (split-window-below)
           (let ((w (selected-window)))
             (prin1-to-string
              (list (window-top-line w) (window-pixel-top w)
                    (coordinates-in-window-p '(0 . 0) w)
                    (coordinates-in-window-p '(2 . 2) w)))))",
    );
    let text = out
        .into_iter()
        .next()
        .expect("one form")
        .expect("the form evaluates")
        .as_str_owned()
        .expect("prin1-to-string returns a string");
    assert_eq!(
        text, "(1 0 (0 . 0) (2 . 2))",
        "the split must inherit top_line 1 with pixel_top 0, and both answers \
         must be measured from the pixel edge"
    );
}

#[test]
fn window_bump_use_time_tracks_second_most_recent_window() {
    crate::test_utils::init_test_tracing();
    let out = runtime_eval_with_usable_terminal(
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
               (condition-case err (window-bump-use-time nil nil) (error err))
               (let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
         (let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
         (let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
         (let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
         (let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
         (let ((w ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
           (prog1
               (list (eq (set-frame-selected-window nil w2) w2)
                     (eq (selected-window) w2))
             (select-window w1)
             (delete-window w2)))
         (let* ((w1 (selected-window))
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil))
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
                      ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil))))
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
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
                (w2 ((lambda (old size side normal)
                   (set-window-new-pixel old
                     (- (if (memq side '(t left right))
                            (window-pixel-width old)
                          (window-pixel-height old)) size))
                   (split-window-internal old size side normal)) (selected-window) (/ (window-pixel-height (selected-window)) 2) nil nil)))
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
fn set_frame_size_and_position_pixelwise_updates_top_level_tty_native_totals() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    let frame = Value::make_frame(
        ev.frame_manager()
            .selected_frame()
            .expect("selected frame")
            .id
            .0,
    );

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

    let frame_state = ev.frame_manager().selected_frame().expect("selected frame");
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
        .frame_manager()
        .selected_frame()
        .expect("selected frame should exist")
        .id;
    {
        let frame = ev
            .frame_manager_mut()
            .get_mut(fid)
            .expect("frame should exist");
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

    ev.apply_resize_input_event(800, 260, 1.0, fid.0, false)
        .expect("resize redisplay");

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
               (set-window-new-pixel other
                 (- (window-pixel-height other) (/ (window-pixel-height other) 2)))
               (setq w (split-window-internal other (/ (window-pixel-height other) 2) nil nil))
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
               (set-window-new-pixel other
                 (- (window-pixel-height other) (/ (window-pixel-height other) 2)))
               (setq w (split-window-internal other (/ (window-pixel-height other) 2) nil nil))
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
               (set-window-new-pixel other
                 (- (window-pixel-height other) (/ (window-pixel-height other) 2)))
               (setq w (split-window-internal other (/ (window-pixel-height other) 2) nil nil))
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
fn winner_can_capture_a_graphical_startup_layout_before_first_presentation() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    ev.eval_str("(require 'winner)")
        .expect("load Winner before the synthetic frame becomes graphical");
    let frame_id = ev
        .frame_manager()
        .selected_frame()
        .expect("selected frame")
        .id;
    ev.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .set_window_system(Some(Value::symbol("neo")));
    assert!(
        ev.frame_manager()
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
    let scratch = ev.buffer_manager_mut().create_buffer("*m-x-target*");
    ev.buffer_manager_mut().set_current(scratch);
    let frame_id = ev.frame_manager_mut().create_frame("F1", 960, 640, scratch);
    assert!(
        ev.frame_manager_mut().select_frame(frame_id),
        "should be able to select the newly created frame"
    );
    let selected_before = ev.frame_manager().get(frame_id).unwrap().selected_window;

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

    let frame = ev
        .frame_manager()
        .get(frame_id)
        .expect("frame still exists");
    let selected_after = frame.selected_window;
    let leaves: Vec<_> = frame.root_leaf_ids();

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

#[cfg(test)]
#[test]
fn minibuffer_selected_window_returns_nil_after_the_invoking_window_is_deleted() {
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    ev.eval_str("(setq issue560-origin (split-window)) (select-window issue560-origin)")
        .expect("create and select the invoking window");
    let mini = ev.buffer_manager_mut().create_buffer(" *Minibuf-1*");
    ev.activate_minibuffer_window_for_buffer(mini, LispString::from_utf8("Prompt: "), None)
        .expect("activate minibuffer");
    assert_eq!(
        format_eval_result(&ev.eval_str("(eq (minibuffer-selected-window) issue560-origin)")),
        "OK t"
    );
    let result = ev.eval_str(
        "(delete-window issue560-origin)
         (list (minibuffer-selected-window) (window-live-p issue560-origin)
               (with-selected-window
                   (or (minibuffer-selected-window) (selected-window)) 'ok))",
    );
    assert_eq!(format_eval_result(&result), "OK (nil nil ok)");
}

#[test]
fn minibuffer_selected_window_requires_an_active_selected_minibuffer() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_one(
        r#"(let ((executing-kbd-macro t)
      (origin (split-window))
      (observations nil))
  (select-window origin)
  (push (minibuffer-selected-window) observations)
  (catch 'issue560-done
    (minibuffer-with-setup-hook
        (lambda ()
          (let ((mini (selected-window)))
            (push (list (> (minibuffer-depth) 0)
                        (window-minibuffer-p)
                        (eq (minibuffer-selected-window) origin)) observations)
            (select-window origin)
            (push (list (> (minibuffer-depth) 0)
                        (window-minibuffer-p)
                        (minibuffer-selected-window)) observations)
            (select-window mini)
            (push (eq (minibuffer-selected-window) origin) observations))
          (throw 'issue560-done nil))
      (read-from-minibuffer "Prompt: ")))
  (push (list (minibuffer-depth) (minibuffer-selected-window)) observations)
  (delete-window origin)
  (nreverse observations))
"#,
    );
    assert_eq!(result, "OK (nil (t t t) (t nil nil) t (0 nil))");
}

#[test]
fn category_word_wrap_projection_tracks_inherited_sets_and_nested_observations() {
    use crate::window::WordWrapDirection;
    use neovm_value::tagged::collection_reads::capture;

    let mut ev = runtime_startup_context();
    ev.eval_str(
        r#"(progn
        (setq-local word-wrap t word-wrap-by-category t)
        (setq wrap623-parent (make-category-table)
              wrap623-child (make-category-table))
        (set-char-table-range wrap623-parent t nil)
        (set-char-table-range wrap623-parent nil (make-category-set "|"))
        (set-char-table-range wrap623-child t nil)
        (set-char-table-range wrap623-child nil nil)
        (set-char-table-parent wrap623-child wrap623-parent)
        (set-category-table wrap623-child))"#,
    )
    .unwrap();
    let buffer = ev.buffer_manager().current_buffer_id().unwrap();
    let permits_x = |ev: &Context| {
        ev.layout_word_wrap_policy(buffer)
            .unwrap()
            .opportunity(u32::from('x'), WordWrapDirection::Forward)
            .after()
    };
    assert!(permits_x(&ev));

    // A warm native result must still contribute its mutable Lisp
    // dependencies to an enclosing observation used by a layout query.
    let (permitted, dependencies) = capture(|| permits_x(&ev));
    assert!(permitted);
    let dependencies = dependencies.expect("coherent native projection");
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    unrelated.set_car(Value::T);
    assert!(dependencies.unchanged());

    ev.eval_str("(aset (char-table-range wrap623-parent nil) ?| nil)")
        .unwrap();
    assert!(
        !dependencies.unchanged(),
        "inherited bool-vector dependency"
    );
    assert!(!permits_x(&ev));

    ev.eval_str("(set-char-table-range wrap623-parent nil (make-category-set \"|\"))")
        .unwrap();
    assert!(permits_x(&ev), "parent default replacement");
    ev.eval_str("(set-char-table-range wrap623-child nil (make-category-set \"\"))")
        .unwrap();
    assert!(!permits_x(&ev), "local default shadows the parent");
    ev.eval_str("(set-char-table-range wrap623-child nil nil)")
        .unwrap();
    assert!(permits_x(&ev));
    ev.eval_str("(garbage-collect)").unwrap();
    assert!(permits_x(&ev), "live source survives collection");
    ev.eval_str(
        "(progn (set-category-table (make-category-table)) (setq wrap623-parent nil wrap623-child nil) (garbage-collect))",
    )
    .unwrap();
    assert!(!permits_x(&ev), "replace and reclaim the old source");
}

#[test]
fn make_frame_applies_early_init_default_frame_alist_bars() {
    // #600: an early-init file setting these in `default-frame-alist' must
    // reach the opening frame through the real `make-frame' merge path.
    // `make-frame' is lisp/frame.el and exists only after loadup.
    crate::test_utils::init_test_tracing();
    let mut ev = runtime_startup_context();
    crate::emacs_core::terminal::pure::mark_selected_terminal_usable_for_test(&ev);
    let base = ev
        .frame_manager()
        .selected_frame()
        .expect("startup frame")
        .id;
    {
        let frame = ev.frame_manager_mut().get_mut(base).expect("base frame");
        frame.set_window_system(Some(Value::symbol("neo")));
    }
    ev.frame_manager_mut().select_frame(base);
    ev.set_variable("window-system", Value::symbol("neo"));
    ev.set_display_host(Box::new(
        neovm_display::test_utils::window_cmds_host::RecordingDisplayHost::new(),
    ));

    let out = ev.eval_str_each(
        r#"(let ((default-frame-alist '((menu-bar-lines . 0)
                                        (tool-bar-lines . 0)
                                        (vertical-scroll-bars))))
             (make-frame nil))"#,
    );
    let frame_value = out[0].as_ref().expect("make-frame should succeed");
    let fid = crate::window::FrameId(
        frame_value
            .as_frame_id()
            .expect("make-frame returns a frame"),
    );
    let frame = ev.frame_manager().get(fid).expect("created frame");

    assert_eq!(
        frame.parameter("menu-bar-lines"),
        Some(Value::fixnum(0)),
        "menu-bar-lines from default-frame-alist must reach the frame"
    );
    assert_eq!(
        frame.parameter("tool-bar-lines"),
        Some(Value::fixnum(0)),
        "tool-bar-lines from default-frame-alist must reach the frame"
    );
    assert_eq!(
        frame.parameter("vertical-scroll-bars"),
        Some(Value::NIL),
        "the bare (vertical-scroll-bars) entry must be stored as an explicit nil"
    );
    assert_eq!(frame.menu_bar_height, 0);
    assert_eq!(frame.tool_bar_height, 0);
    assert_eq!(
        frame.root_window().bounds().y,
        0.0,
        "with both bars disabled the root window starts at the top"
    );
}

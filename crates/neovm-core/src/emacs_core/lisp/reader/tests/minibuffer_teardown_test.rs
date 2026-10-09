//! Tests for the GNU-style minibuffer teardown boundary
//! (`teardown_minibuffer_level_in_state`).
//!
//! These assert the two teardown invariants neomacs previously dropped: after
//! the boundary runs at the outermost level, the expired ` *Minibuf-N*` has
//! ZERO overlays (the vertico candidate `after-string` overlay is gone) and the
//! minibuffer window is exactly one line tall (the analogue of GNU's
//! unconditional `resize_mini_window (minibuf_window, 0)` at unwind).

use super::*;
use crate::emacs_core::eval::Context;

/// Build a context with a selected frame, an active-looking ` *Minibuf-1*`
/// buffer that carries an overlay, a grown (multi-line) minibuffer window, and a
/// captured `ActiveMinibufferWindowState`, then run the teardown boundary.
///
/// Returns `(minibuf_id, frame_id, mini_window_height_after, mini_char_height)`.
fn run_teardown_and_collect(
    ev: &mut Context,
) -> (crate::buffer::BufferId, crate::window::FrameId, f32, f32) {
    // A normal buffer for the frame's root window.
    let root_buf = ev.buffers.create_buffer("teardown-root");
    let frame_id = ev
        .frames
        .create_frame("minibuffer-teardown", 200, 320, root_buf);

    // The active minibuffer pool buffer, with prompt text and a candidate
    // overlay (vertico-style after-string overlay) that must NOT survive
    // teardown.
    let minibuf_id = find_or_create_minibuffer_buffer_in_state(&mut ev.buffers, 1);
    let _ = ev.buffers.replace_buffer_contents(minibuf_id, "M-x ");
    // `make-overlay` anchors on the *current* buffer when the BUFFER arg is nil;
    // switch to *Minibuf-1* so the candidate overlay lands there.
    let _ = ev.buffers.switch_current(minibuf_id);
    let overlay = crate::emacs_core::buffer::builtin_make_overlay_in_buffers(
        &mut ev.buffers,
        vec![
            Value::fixnum(1),
            Value::fixnum(1),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    )
    .expect("make overlay on *Minibuf-1*");
    let _ = overlay; // keep the binding alive

    assert!(
        ev.buffers
            .get(minibuf_id)
            .map(|b| !b.overlays().is_empty())
            .unwrap_or(false),
        "precondition: *Minibuf-1* should carry the candidate overlay before teardown"
    );

    // Grow the minibuffer window so it is multi-line, simulating a redisplay
    // that expanded it for the candidate list.
    if let Some(frame) = ev.frames.get_mut(frame_id) {
        frame.grow_mini_window(8);
    }
    let char_h = ev.frames.get(frame_id).expect("frame").char_height.max(1.0);
    let grown_h = ev
        .frames
        .get(frame_id)
        .and_then(|f| f.minibuffer_leaf.as_ref())
        .map(|m| m.bounds().height)
        .expect("mini-window bounds");
    assert!(
        grown_h > char_h + 0.5,
        "precondition: mini-window should be grown multi-line, got {grown_h} (char_h={char_h})"
    );

    // Activate the minibuffer window so we have a saved state to restore.
    let saved = activate_minibuffer_window_in_state(
        &mut ev.frames,
        &mut ev.buffers,
        &mut ev.minibuffer_selected_window,
        &mut ev.active_minibuffer_window,
        minibuf_id,
        crate::emacs_core::minibuffer::MinibufferEntryLevel::Outermost,
    )
    .expect("activate minibuffer window");

    // Outermost teardown: depth_after_pop == 0 (no nested minibuffer remains).
    let outcome = teardown_minibuffer_level_in_state(
        &mut ev.frames,
        &mut ev.buffers,
        &mut ev.minibuffer_selected_window,
        &mut ev.active_minibuffer_window,
        minibuf_id,
        0,
        saved,
        || Ok(Value::NIL),
    );
    assert!(
        outcome.inactive_mode_result.is_ok(),
        "teardown inactive-mode hook should succeed"
    );
    outcome.window_restore.apply(ev);

    let height_after = ev
        .frames
        .get(frame_id)
        .and_then(|f| f.minibuffer_leaf.as_ref())
        .map(|m| m.bounds().height)
        .expect("mini-window bounds after teardown");
    (minibuf_id, frame_id, height_after, char_h)
}

#[test]
fn teardown_deletes_all_overlays_on_expired_minibuffer() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (minibuf_id, _frame_id, _height_after, _char_h) = run_teardown_and_collect(&mut ev);

    let overlay_count = ev
        .buffers
        .get(minibuf_id)
        .map(|b| b.overlays().len())
        .expect("minibuffer buffer present");
    assert_eq!(
        overlay_count, 0,
        "teardown must delete ALL overlays on the expired *Minibuf-1* \
         (the candidate after-string overlay is the carrier of the multi-line \
         content); text-erase alone is not enough"
    );
}

#[test]
fn teardown_force_resizes_mini_window_to_one_line() {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    let (_minibuf_id, _frame_id, height_after, char_h) = run_teardown_and_collect(&mut ev);

    assert!(
        (height_after - char_h).abs() < 0.5,
        "teardown must force the mini-window back to exactly one line \
         (height {height_after} should equal char height {char_h})"
    );
}

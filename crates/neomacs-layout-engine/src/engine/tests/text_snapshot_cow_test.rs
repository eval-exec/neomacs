//! P3.5 stage A: layout snapshots share the buffer text copy-on-write, and
//! no snapshot outlives the layout that built it.

use super::*;
use neovm_core::buffer::text_snapshot::{TextSnapshotMode, set_text_snapshot_mode_override};

struct ModeGuard;

impl ModeGuard {
    fn force(mode: TextSnapshotMode) -> Self {
        set_text_snapshot_mode_override(Some(mode));
        ModeGuard
    }
}

impl Drop for ModeGuard {
    fn drop(&mut self) {
        set_text_snapshot_mode_override(None);
    }
}

/// Type at the end of a line and lay the frame out after every keystroke.
/// Returns the per-frame `buffer_text_cow_copies` and the snapshot counts.
fn type_and_layout(mode: TextSnapshotMode, keys: usize) -> (Vec<usize>, Vec<usize>) {
    let _mode = ModeGuard::force(mode);
    let text = "(defun f (a b) (+ a b))\n".repeat(60);
    let (mut eval, frame_id, buf_id, _window) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let edit_at = 10 * 24 + 5;
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(edit_at));
    let mut copies = Vec::new();
    let mut snapshots = Vec::new();
    for i in 0..keys {
        eval.buffer_manager_mut()
            .get_mut(buf_id)
            .expect("buffer")
            .insert(if i % 2 == 0 { "x" } else { "y" });
        engine.layout_frame_rust(&mut eval, frame_id);
        copies.push(engine.last_layout_stats().buffer_text_cow_copies);
        snapshots.push(engine.last_layout_stats().buffer_snapshots_built);
    }
    (copies, snapshots)
}

#[test]
fn steady_typing_with_shared_snapshots_copies_no_buffer_text() {
    let (copies, snapshots) = type_and_layout(TextSnapshotMode::Share, 12);
    assert!(
        snapshots.iter().all(|&n| n > 0),
        "every frame snapshots the displayed buffer: {snapshots:?}"
    );
    assert!(
        copies.iter().all(|&n| n == 0),
        "a snapshot outlived its layout, so a keystroke copied the text: {copies:?}"
    );
}

/// The counter is live: a snapshot held across a keystroke (what a leaked
/// layout snapshot would do) makes that keystroke copy the text once, and
/// the next frame's stats report it.
#[test]
fn a_snapshot_held_across_a_keystroke_is_counted_in_the_next_frame() {
    let _mode = ModeGuard::force(TextSnapshotMode::Share);
    let text = "(defun f (a b) (+ a b))\n".repeat(20);
    let (mut eval, frame_id, buf_id, _window) = incr_editing_frame(&text, 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let held = eval
        .buffer_manager()
        .get(buf_id)
        .expect("buffer")
        .text_snapshot();
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .insert("z");
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(engine.last_layout_stats().buffer_text_cow_copies, 1);
    drop(held);
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .insert("z");
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(engine.last_layout_stats().buffer_text_cow_copies, 0);
}

#[test]
fn shared_and_copied_snapshots_lay_out_identical_frames() {
    fn frame_after_typing(mode: TextSnapshotMode) -> BackendLayoutTrace {
        let _mode = ModeGuard::force(mode);
        let text = "(defun f (a b) (+ a b))\n\t\u{4e2d}\u{6587} tab\n".repeat(30);
        let (mut eval, frame_id, buf_id, _window) = incr_editing_frame(&text, 800, 600);
        let mut engine = LayoutEngine::new();
        engine.layout_frame_rust(&mut eval, frame_id);
        // Descending byte positions, each on a character boundary of the
        // original text, so no insert moves a later one.
        for (at, insert) in [(91usize, "x"), (90, "\n"), (40, "\u{e9}"), (7, "\n")] {
            let buffer = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
            buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(at));
            buffer.insert(insert);
            engine.layout_frame_rust(&mut eval, frame_id);
        }
        selected_window_layout_trace(&eval, &engine, frame_id)
    }
    assert_eq!(
        frame_after_typing(TextSnapshotMode::Share),
        frame_after_typing(TextSnapshotMode::Copy),
        "the snapshot's representation must not change what is displayed"
    );
}

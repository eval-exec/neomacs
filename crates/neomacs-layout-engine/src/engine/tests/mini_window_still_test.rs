//! P3.5 D (`NEOMACS_LAYOUT_MINI_STILL`): the mini-window stands still when
//! nothing it displays changed, and lays out exactly as a full walk would.

use super::*;
use crate::incremental_layout::set_mini_window_still_for_test;

struct StillGuard;

impl StillGuard {
    fn on() -> Self {
        set_mini_window_still_for_test(Some(true));
        StillGuard
    }
}

impl Drop for StillGuard {
    fn drop(&mut self) {
        set_mini_window_still_for_test(None);
    }
}

/// Every window's rows as text, from the last accepted frame.
fn frame_rows_text(engine: &LayoutEngine) -> Vec<(i64, Vec<String>)> {
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("an accepted frame")
        .state();
    state
        .window_matrices
        .iter()
        .map(|entry| {
            let rows = entry
                .matrix
                .rows
                .iter()
                .filter(|row| row.enabled)
                .map(|row| {
                    GlyphArea::ALL
                        .iter()
                        .map(|area| glyphs_logical_text(&row.glyphs[area.index()]))
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .collect();
            (entry.window_id.get(), rows)
        })
        .collect()
}

/// The rows a fresh engine (no retained state) lays out for the same state.
fn fresh_rows(
    eval: &mut Context,
    frame_id: neovm_core::window::FrameId,
) -> Vec<(i64, Vec<String>)> {
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(eval, frame_id);
    frame_rows_text(&fresh)
}

fn message(eval: &mut Context, text: Option<&str>) {
    eval.set_current_message(text.map(neovm_core::heap_types::LispString::from_utf8));
}

#[test]
fn the_mini_window_stands_still_until_what_it_shows_changes() {
    let _still = StillGuard::on();
    let (mut eval, frame_id, _buf, _window) =
        incr_editing_frame(&"(some text)\n".repeat(20), 800, 600);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().mini_window_still,
        0,
        "first frame"
    );

    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().mini_window_still,
        1,
        "idle frame"
    );
    assert_eq!(frame_rows_text(&engine), fresh_rows(&mut eval, frame_id));

    message(&mut eval, Some("hello from the echo area"));
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().mini_window_still,
        0,
        "a new message"
    );
    let shown = frame_rows_text(&engine);
    assert!(
        shown.iter().any(|(_, rows)| rows
            .iter()
            .any(|row| row.contains("hello from the echo area"))),
        "{shown:?}"
    );
    assert_eq!(shown, fresh_rows(&mut eval, frame_id));

    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().mini_window_still,
        1,
        "message stays"
    );
    assert_eq!(frame_rows_text(&engine), fresh_rows(&mut eval, frame_id));

    message(&mut eval, Some("a second message"));
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().mini_window_still,
        0,
        "message replaced"
    );
    assert_eq!(frame_rows_text(&engine), fresh_rows(&mut eval, frame_id));

    message(&mut eval, None);
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().mini_window_still,
        0,
        "message cleared"
    );
    let cleared = frame_rows_text(&engine);
    assert!(
        !cleared
            .iter()
            .any(|(_, rows)| rows.iter().any(|row| row.contains("second message"))),
        "{cleared:?}"
    );
    assert_eq!(cleared, fresh_rows(&mut eval, frame_id));

    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        engine.last_layout_stats().mini_window_still,
        1,
        "idle again"
    );
}

#[test]
fn text_written_into_the_idle_minibuffer_is_shown() {
    // `minibuffer-line' writes status text into " *Minibuf-0*"; the idle
    // mini-window must show each change.
    let _still = StillGuard::on();
    let (mut eval, frame_id, _buf, _window) = incr_editing_frame("body\n", 800, 600);
    let minibuffer = eval
        .buffers
        .find_buffer_by_name(" *Minibuf-0*")
        .expect("minibuffer buffer");
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(engine.last_layout_stats().mini_window_still, 1);

    for status in ["status one", " two"] {
        eval.buffer_manager_mut()
            .get_mut(minibuffer)
            .expect("minibuffer")
            .insert(status);
        engine.layout_frame_rust(&mut eval, frame_id);
        assert_eq!(engine.last_layout_stats().mini_window_still, 0, "{status}");
        let shown = frame_rows_text(&engine);
        assert!(
            shown
                .iter()
                .any(|(_, rows)| rows.iter().any(|row| row.contains(status.trim()))),
            "{shown:?}"
        );
        assert_eq!(shown, fresh_rows(&mut eval, frame_id));
        engine.layout_frame_rust(&mut eval, frame_id);
        assert_eq!(
            engine.last_layout_stats().mini_window_still,
            1,
            "{status} stays"
        );
    }
}

#[test]
fn the_mini_window_is_relaid_every_frame_with_the_knob_off() {
    set_mini_window_still_for_test(Some(false));
    let (mut eval, frame_id, _buf, _window) = incr_editing_frame("body\n", 800, 600);
    let mut engine = LayoutEngine::new();
    for _ in 0..3 {
        engine.layout_frame_rust(&mut eval, frame_id);
        assert_eq!(engine.last_layout_stats().mini_window_still, 0);
    }
    set_mini_window_still_for_test(None);
}

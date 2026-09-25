//! `NEOMACS_PRESENT_CHROME_POS=rows` finds exactly the chrome string
//! positions the whole-frame walk finds.

use super::{ChromePositionSource, set_chrome_position_source_for_test};
use crate::LayoutEngine;
use neovm_core::emacs_core::{Context, Value};

fn positions_both_ways(
    state: &neomacs_display_protocol::glyph_matrix::FrameDisplayState,
) -> (
    Vec<neomacs_display_protocol::PresentedStringPosition>,
    Vec<neomacs_display_protocol::PresentedStringPosition>,
) {
    set_chrome_position_source_for_test(Some(ChromePositionSource::Frame));
    let frame = super::window_chrome_string_positions(state).expect("frame walk");
    set_chrome_position_source_for_test(Some(ChromePositionSource::Rows));
    let rows = super::window_chrome_string_positions(state).expect("row walk");
    set_chrome_position_source_for_test(None);
    (frame, rows)
}

/// Lay out a frame whose windows carry string-sourced tab, header and mode
/// lines, as a terminal frame or a graphical one, split or not.
fn laid_out_frame(
    window_system: bool,
    split: bool,
) -> neomacs_display_protocol::glyph_matrix::FrameDisplayState {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        buffer.insert("line one\nline two\nline three\n");
        buffer.set_buffer_local(
            "tab-line-format",
            Value::list(vec![Value::string(" tab-one "), Value::string(" tab-two ")]),
        );
        buffer.set_buffer_local(
            "header-line-format",
            Value::string(" header \u{4e2d}\u{6587} text "),
        );
        buffer.set_buffer_local(
            "mode-line-format",
            Value::list(vec![Value::string("-- mode "), Value::string("line --")]),
        );
    }
    let (width, height) = (800, 600);
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("chrome-positions", width, height, buffer_id);
    if window_system {
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .set_window_system(Some(Value::symbol("neo")));
    }
    if split {
        let selected = eval
            .frame_manager()
            .get(frame_id)
            .expect("frame")
            .selected_window;
        let left = eval
            .frame_manager_mut()
            .split_window(
                frame_id,
                selected,
                neovm_core::window::SplitDirection::Horizontal,
                buffer_id,
                None,
                neovm_core::window::SplitPlacement::BeforeTarget,
            )
            .expect("side-by-side windows");
        eval.frame_manager_mut()
            .split_window(
                frame_id,
                left,
                neovm_core::window::SplitDirection::Vertical,
                buffer_id,
                None,
                neovm_core::window::SplitPlacement::AfterTarget,
            )
            .expect("stacked windows");
    }
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    engine
        .last_frame_display_state
        .clone()
        .expect("a laid-out frame")
        .into_state()
}

#[test]
fn row_walk_finds_the_whole_frame_walks_chrome_string_positions() {
    for window_system in [false, true] {
        for split in [false, true] {
            let state = laid_out_frame(window_system, split);
            let (frame, rows) = positions_both_ways(&state);
            assert!(
                !frame.is_empty(),
                "fixture has chrome strings (window_system={window_system}, split={split})"
            );
            assert_eq!(rows, frame, "window_system={window_system}, split={split}");
        }
    }
}

#[test]
fn row_walk_of_a_frame_without_chrome_strings_is_empty() {
    let mut eval = Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    eval.buffer_manager_mut()
        .get_mut(buffer_id)
        .expect("buffer")
        .set_buffer_local("mode-line-format", Value::NIL);
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("no-chrome", 80, 24, buffer_id);
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine
        .last_frame_display_state
        .clone()
        .expect("frame")
        .into_state();
    let (frame, rows) = positions_both_ways(&state);
    assert_eq!(rows, frame);
}

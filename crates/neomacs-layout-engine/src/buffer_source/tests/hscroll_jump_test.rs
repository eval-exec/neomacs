//! GNU's long-line hscroll jump (`src/xdisp.c:25896-25920`).
//!
//! `fast_move_it_horizontally` seats the iterator at the first visible
//! boundary in one step. The work the shortcut removes is the per-character
//! skip, so that is what this test counts.

use super::super::row_lifecycle::{
    consume_hscroll_skip_from_position, hscroll_skip_char_steps, long_line_hscroll_jump,
    reset_hscroll_skip_char_steps,
};
use crate::display_row::walk_state::{HorizontalScrollSkipState, HorizontalScrollTruncationTarget};
use crate::display_source::DisplaySourceTextPosition;
use crate::neovm_bridge::LayoutBufferView;
use crate::types::LineWrapMode;
use neovm_core::buffer::{Buffer, BufferId, EmacsBytePos, EmacsByteRange};
use neovm_core::emacs_core::Value;

/// A buffer holding one ASCII line of `len` characters and no newline, with
/// redisplay's long-line flag set as `redisplay_window` would set it.
fn long_ascii_buffer(len: usize) -> Buffer {
    let _runtime = neovm_core::emacs_core::eval_hook_installers::new_context();
    let mut buffer = Buffer::new_standalone(BufferId(42), Value::string("*hscroll-jump*"));
    buffer.insert(&"a".repeat(len));
    assert!(
        buffer.refresh_long_line_optimizations(50_000),
        "the test line must set long_line_optimizations_p"
    );
    buffer
}

/// Per-character skip steps one hscrolled truncated row spends with `len`
/// characters skipped.
fn skip_work(len: usize) -> usize {
    let buffer = long_ascii_buffer(len);
    let text_len = buffer.layout_total_emacs_byte_len().get();
    let mut text = Vec::with_capacity(text_len);
    buffer.layout_copy_emacs_byte_range_to(
        EmacsByteRange::new(EmacsBytePos::ZERO, EmacsBytePos::new(text_len)),
        &mut text,
    );
    let mut skip = HorizontalScrollSkipState::new(
        LineWrapMode::Truncate,
        len as i32,
        HorizontalScrollTruncationTarget::FirstVisibleSourceGlyph,
    );
    let mut position = DisplaySourceTextPosition::new(0, 0);
    reset_hscroll_skip_char_steps();
    // The same choice the row renderer makes: jump when GNU would, else
    // consume one character and retry.
    loop {
        if let Some(jumped) = long_line_hscroll_jump(&text, position, &mut skip, &buffer, 0, None) {
            position = jumped;
            break;
        }
        let Some(action) =
            consume_hscroll_skip_from_position(&text, &mut position, &mut skip, 8, &buffer, 0)
        else {
            break;
        };
        if action.is_line_break() {
            break;
        }
    }
    assert_eq!(
        position.charpos() as usize,
        len,
        "the jump must land exactly at the hscroll boundary"
    );
    hscroll_skip_char_steps()
}

/// GNU's shortcut replaces the character walk, so the per-character work must
/// not grow with how far the window is hscrolled.
#[test]
fn hscroll_skip_work_is_flat_as_the_line_grows() {
    let short = skip_work(200_000);
    let long = skip_work(400_000);
    assert_eq!(
        long, short,
        "hscroll skip work must not grow with the line: {short} -> {long}"
    );
    assert!(
        short <= 1,
        "the long-line jump must skip the character walk: {short} characters consumed"
    );
}

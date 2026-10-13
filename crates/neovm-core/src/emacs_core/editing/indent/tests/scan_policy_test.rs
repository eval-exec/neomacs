use super::*;
use crate::emacs_core::eval::Context;

fn text_context(text: &str) -> (Context, BufferId) {
    let mut context = Context::new();
    let buffer = context.buffers.current_buffer_id().expect("current buffer");
    context
        .buffers
        .insert_into_buffer(buffer, text)
        .expect("insert scan fixture");
    (context, buffer)
}

fn assert_row(
    context: &mut Context,
    buffer: BufferId,
    start: EmacsBytePos,
    width: usize,
    wrap: LineWrap,
    next: EmacsBytePos,
    end: ScreenLineEnd,
) {
    let step = next_screen_line_start_from(context, buffer, start, width, wrap)
        .expect("scan fixture")
        .expect("nonempty accessible row");
    assert_eq!(step.next, next, "next byte for {wrap:?}");
    assert_eq!(step.end, end, "row end for {wrap:?}");
}

#[test]
fn exactly_full_text_followed_by_newline_stays_one_row_in_every_policy() {
    let (mut context, buffer) = text_context("abcd\nz");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WindowWrap,
        EmacsBytePos::new(5),
        ScreenLineEnd::Newline,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WordWrap,
        EmacsBytePos::new(5),
        ScreenLineEnd::Newline,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::Truncate,
        EmacsBytePos::new(5),
        ScreenLineEnd::Newline,
    );
}

#[test]
fn exactly_full_accessible_text_has_no_following_row() {
    let (mut context, buffer) = text_context("abcd");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WindowWrap,
        EmacsBytePos::new(4),
        ScreenLineEnd::BufferEnd,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WordWrap,
        EmacsBytePos::new(4),
        ScreenLineEnd::BufferEnd,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::Truncate,
        EmacsBytePos::new(4),
        ScreenLineEnd::BufferEnd,
    );
}

#[test]
fn truncation_skips_hidden_remainder_to_newline_while_wrap_continues() {
    let (mut context, buffer) = text_context("abcdef\n");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WindowWrap,
        EmacsBytePos::new(4),
        ScreenLineEnd::Edge,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WordWrap,
        EmacsBytePos::new(4),
        ScreenLineEnd::Edge,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::Truncate,
        EmacsBytePos::new(7),
        ScreenLineEnd::Newline,
    );
}

#[test]
fn overflowing_wide_glyph_moves_whole_to_the_following_row() {
    let (mut context, buffer) = text_context("abc界z");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WindowWrap,
        EmacsBytePos::new(3),
        ScreenLineEnd::Edge,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WordWrap,
        EmacsBytePos::new(3),
        ScreenLineEnd::Edge,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::Truncate,
        EmacsBytePos::new(7),
        ScreenLineEnd::ClippedAtBufferEnd,
    );
}

#[test]
fn first_wide_glyph_makes_progress_even_in_a_one_column_window() {
    let (mut context, buffer) = text_context("界x");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        1,
        LineWrap::WindowWrap,
        EmacsBytePos::new(3),
        ScreenLineEnd::Edge,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        1,
        LineWrap::WordWrap,
        EmacsBytePos::new(3),
        ScreenLineEnd::Edge,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        1,
        LineWrap::Truncate,
        EmacsBytePos::new(4),
        ScreenLineEnd::ClippedAtBufferEnd,
    );
}

#[test]
fn unicode_byte_positions_and_tab_columns_remain_distinct() {
    let (mut context, buffer) = text_context("é\tX");
    // One glyph column for é, then the default tab stop at column eight.
    // The next row starts at byte three, rather than character index two.
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        8,
        LineWrap::WindowWrap,
        EmacsBytePos::new(3),
        ScreenLineEnd::Edge,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        8,
        LineWrap::WordWrap,
        EmacsBytePos::new(3),
        ScreenLineEnd::WordWrapPoint,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        8,
        LineWrap::Truncate,
        EmacsBytePos::new(4),
        ScreenLineEnd::ClippedAtBufferEnd,
    );
}

#[test]
fn word_wrap_returns_to_saved_word_start_before_the_window_edge() {
    let (mut context, buffer) = text_context("one two z");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        6,
        LineWrap::WindowWrap,
        EmacsBytePos::new(6),
        ScreenLineEnd::Edge,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        6,
        LineWrap::WordWrap,
        EmacsBytePos::new(4),
        ScreenLineEnd::WordWrapPoint,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        6,
        LineWrap::Truncate,
        EmacsBytePos::new(9),
        ScreenLineEnd::ClippedAtBufferEnd,
    );
}

#[test]
fn word_wrap_does_not_start_a_row_inside_a_run_of_spaces() {
    let (mut context, buffer) = text_context("aa   b");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WordWrap,
        EmacsBytePos::new(4),
        ScreenLineEnd::Edge,
    );
}

#[test]
fn narrowed_end_hides_the_following_newline_from_every_policy() {
    let (mut context, buffer) = text_context("HEADabcd\nTAIL");
    context
        .buffers
        .narrow_buffer_to_emacs_byte_range(
            buffer,
            EmacsByteRange::new(EmacsBytePos::new(4), EmacsBytePos::new(8)),
        )
        .expect("narrow fixture");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::new(4),
        4,
        LineWrap::WindowWrap,
        EmacsBytePos::new(8),
        ScreenLineEnd::BufferEnd,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::new(4),
        4,
        LineWrap::WordWrap,
        EmacsBytePos::new(8),
        ScreenLineEnd::BufferEnd,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::new(4),
        4,
        LineWrap::Truncate,
        EmacsBytePos::new(8),
        ScreenLineEnd::BufferEnd,
    );
    assert!(
        next_screen_line_start_from(
            &mut context,
            buffer,
            EmacsBytePos::new(8),
            4,
            LineWrap::WindowWrap
        )
        .expect("scan accessible end")
        .is_none()
    );
}

#[test]
fn a_new_scan_reacquires_the_edited_buffer_and_accessible_end() {
    let (mut context, buffer) = text_context("abcd");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WindowWrap,
        EmacsBytePos::new(4),
        ScreenLineEnd::BufferEnd,
    );
    context
        .buffers
        .goto_buffer_emacs_byte_pos(buffer, EmacsBytePos::new(4));
    context
        .buffers
        .insert_into_buffer(buffer, "ef\n")
        .expect("extend fixture");
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::WindowWrap,
        EmacsBytePos::new(4),
        ScreenLineEnd::Edge,
    );
    assert_row(
        &mut context,
        buffer,
        EmacsBytePos::ZERO,
        4,
        LineWrap::Truncate,
        EmacsBytePos::new(7),
        ScreenLineEnd::Newline,
    );
}

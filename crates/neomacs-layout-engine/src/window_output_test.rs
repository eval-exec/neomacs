use super::ChromeRowOutput;
use super::ChromeRowProgress;
use super::DisplayProgressSink;
use super::DisplayTextRowBegin;
use super::DisplayTextRowGeometryTransition;
use super::DisplayTextRowMetrics;
use super::DisplayTextRowStoredMetrics;
use super::DisplayTextRowTransition;
use super::TextWindowBodyOutputInstall;
use super::TextWindowCursor;
use super::TextWindowCursorEffects;
use super::TextWindowCursorRole;
use super::TextWindowCursorSlots;
use super::TextWindowDecorativeCursor;
use super::TextWindowDisplayRange;
use super::TextWindowOutputTarget;
use super::TextWindowPendingRowFinish;
use super::TextWindowRedisplayPositions;
use super::TextWindowRowDecorationRequest;
use super::WindowOutputEmitter;
use super::begin_text_window_row;
use super::close_text_window_output;
use super::finish_and_end_text_window_row;
use super::finish_pending_text_window_row;
use super::finish_text_window_row;
use super::install_text_window_body_output;
use super::install_text_window_cursor_effects;
use super::install_text_window_finished_rows;
use super::install_text_window_row_decoration;
use super::publish_text_window_cursor;
use super::publish_text_window_decorative_cursor;
use super::record_text_window_display_range;
use super::transition_text_window_row;
use super::transition_text_window_row_with_limit;
use crate::display_cursor::CursorSlotResolutionState;
use crate::display_item::DisplaySourcePosition;
use crate::display_row::builder::{
    DisplayRowAppendProgress, DisplayRowAppendStatus, DisplayRowGlyphSlot, DisplayRowPosition,
};
use crate::display_row::geometry::{
    DisplayRowGeometryState, DisplayRowLimit, DisplayRowYPositions,
};
use crate::display_row::text_output::TextRowOutput;
use crate::display_row::walk_state::HitRowRangeTracker;
use crate::display_status_line::DisplayRowOutputProgress;
use crate::output::builder::DisplayOutputBuilder;
use crate::types::LayoutCharPos0;
use neomacs_display_protocol::effect_config::EffectsConfig;
use neomacs_display_protocol::frame_glyphs::{
    CursorStyle, DisplaySlotId, GlyphRowRole, WindowInfo,
};
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{Color, Rect};
use neomacs_display_protocol::{Glyph, GlyphArea, GlyphProvenance, GlyphType};
use neovm_core::buffer::{BufferId, CharPos0, EmacsBytePos, LispCharPos1};
use neovm_core::emacs_core::Context;

fn assert_char_glyph(glyph: &Glyph, ch: char, face_id: FaceId) {
    assert_eq!(glyph.glyph_type, GlyphType::Char { ch });
    assert_eq!(glyph.face_id, face_id);
}

fn write_char_to_current_row(
    builder: &mut DisplayOutputBuilder,
    ch: char,
    face_id: FaceId,
    charpos: usize,
) {
    builder
        .edit_current_row_for_test(|row| {
            crate::glyph_row_writer::push_char_to_row(row, ch, face_id, charpos, 0.0);
        })
        .expect("current row");
}

fn write_left_margin_char_to_current_row(
    builder: &mut DisplayOutputBuilder,
    ch: char,
    face_id: FaceId,
) {
    builder
        .edit_current_row_for_test(|row| {
            row.glyphs[GlyphArea::LeftMargin.index()].push(Glyph::char(ch, face_id, 0));
        })
        .expect("current row");
}

fn write_left_margin_stretch_to_current_row(
    builder: &mut DisplayOutputBuilder,
    width_cols: u16,
    face_id: FaceId,
) {
    builder
        .edit_current_row_for_test(|row| {
            row.glyphs[GlyphArea::LeftMargin.index()].push(Glyph::stretch(width_cols, face_id));
        })
        .expect("current row");
}

fn write_stretch_to_current_row(
    builder: &mut DisplayOutputBuilder,
    width_cols: u16,
    pixel_width: f32,
    face_id: FaceId,
    provenance: GlyphProvenance,
) {
    builder
        .edit_current_row_for_test(|row| {
            row.glyphs[GlyphArea::Text.index()].push(
                Glyph::stretch(width_cols, face_id)
                    .with_pixel_width(pixel_width)
                    .with_provenance(provenance),
            );
        })
        .expect("current row");
}

fn window_info(window_id: i64) -> WindowInfo {
    WindowInfo {
        window_id: neomacs_display_protocol::types::DisplayWindowId::new(window_id),
        buffer_id: 9,
        buffer_name: String::new(),
        window_start: 1,
        window_end: 1,
        buffer_size: 100,
        bounds: Rect::new(0.0, 0.0, 80.0, 16.0),
        geometry: Default::default(),
        mode_line_height: 0.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_file_name: String::new(),
        modified: false,
    }
}

#[test]
fn emit_text_span_advances_live_output_before_row_finish() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("output-emitter-span", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    emitter.emit_text_span(
        &mut eval,
        LispCharPos1::new(1),
        0,
        0.0,
        0.0,
        0.0,
        24.0,
        16.0,
        0,
        3,
    );

    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");

    assert_eq!(
        display.output_cursor,
        Some(neovm_core::window::WindowCursorPos {
            x: 24,
            y: 0,
            row: 0,
            col: 3,
        })
    );
}

#[test]
fn speculative_output_emitter_keeps_live_window_state_unchanged() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("speculative-output-emitter", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut emitter = WindowOutputEmitter::new_speculative(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    emitter.emit_text_span(
        &mut eval,
        LispCharPos1::ONE,
        0,
        0.0,
        0.0,
        0.0,
        24.0,
        16.0,
        0,
        3,
    );

    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(display.output_cursor, None);
}

#[test]
fn display_progress_sink_emits_buffer_slots_from_row_builder_progress() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-row-position-span", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    emitter.emit_text_progress(
        &mut eval,
        TextRowOutput::new(0, 0.0, 0.0, 16.0),
        &DisplayRowAppendProgress::from_positions(
            DisplayRowPosition::new(8.0, 1),
            DisplayRowPosition::new(24.0, 3),
            DisplayRowAppendStatus::Complete,
            vec![DisplayRowGlyphSlot::new(
                DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(0), EmacsBytePos::new(0)),
                8.0,
                1,
                16.0,
                2,
            )],
        ),
    );

    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");

    assert_eq!(emitter.display_point_len(), 1);
    assert_eq!(
        emitter
            .point_for_lisp_buffer_pos(LispCharPos1::ONE)
            .expect("buffer display point")
            .width,
        16
    );
    assert_eq!(
        display.output_cursor,
        Some(neovm_core::window::WindowCursorPos {
            x: 24,
            y: 0,
            row: 0,
            col: 3,
        })
    );
}

#[test]
fn text_source_slot_emission_accepts_rendered_row_slots() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval.frame_manager_mut().create_frame(
        "output-emitter-rendered-row-slots",
        320,
        120,
        buf_id,
    );
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    emitter.begin_text_row(&mut eval, 0, 1, 0.0, 4.0);
    let output = TextRowOutput::new(0, 0.0, 0.0, 16.0);
    let slots = [DisplayRowGlyphSlot::new(
        DisplaySourcePosition::buffer(BufferId(7), CharPos0::ZERO, EmacsBytePos::ZERO),
        4.0,
        1,
        16.0,
        2,
    )];
    emitter.emit_text_output_spans(
        &mut eval,
        output,
        output.spans_for_source_slots(&slots),
        DisplayRowPosition::new(20.0, 3),
    );

    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");
    let point = emitter
        .point_for_lisp_buffer_pos(LispCharPos1::ONE)
        .expect("buffer display point");

    assert_eq!(point.x, 4);
    assert_eq!(point.width, 16);
    assert_eq!(
        display.output_cursor,
        Some(neovm_core::window::WindowCursorPos {
            x: 20,
            y: 0,
            row: 0,
            col: 3,
        })
    );
}

#[test]
fn display_progress_sink_merges_contiguous_slots_for_same_buffer_position() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-merged-slots", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    let source = DisplaySourcePosition::buffer(BufferId(7), CharPos0::new(0), EmacsBytePos::new(0));
    emitter.emit_text_progress(
        &mut eval,
        TextRowOutput::new(0, 0.0, 0.0, 16.0),
        &DisplayRowAppendProgress::from_positions(
            DisplayRowPosition::new(8.0, 1),
            DisplayRowPosition::new(24.0, 3),
            DisplayRowAppendStatus::Complete,
            vec![
                DisplayRowGlyphSlot::new(source.clone(), 8.0, 1, 8.0, 1),
                DisplayRowGlyphSlot::new(source, 16.0, 2, 8.0, 1),
            ],
        ),
    );

    let point = emitter
        .point_for_buffer_pos(LispCharPos1::ONE)
        .expect("merged display point");
    assert_eq!(emitter.display_point_len(), 1);
    assert_eq!(point.x, 8);
    assert_eq!(point.width, 16);
}

#[test]
fn display_progress_sink_advances_without_points_for_non_buffer_slots() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-lisp-string-slot", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    emitter.begin_text_row(&mut eval, 0, 0, 0.0, 0.0);
    emitter.emit_text_progress(
        &mut eval,
        TextRowOutput::new(0, 0.0, 0.0, 16.0),
        &DisplayRowAppendProgress::from_positions(
            DisplayRowPosition::new(0.0, 0),
            DisplayRowPosition::new(24.0, 3),
            DisplayRowAppendStatus::Complete,
            vec![DisplayRowGlyphSlot::new(
                DisplaySourcePosition::lisp_string(3, 0, 0),
                0.0,
                0,
                24.0,
                3,
            )],
        ),
    );

    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");

    assert_eq!(emitter.display_point_len(), 0);
    assert_eq!(
        display.output_cursor,
        Some(neovm_core::window::WindowCursorPos {
            x: 24,
            y: 0,
            row: 0,
            col: 3,
        })
    );
}

#[test]
fn display_progress_sink_records_chrome_row_progress() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-chrome-progress", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    let output = ChromeRowOutput::new(2, 18.0);
    let progress = DisplayRowOutputProgress::new(40.0, 5, 18.0, 14.0);

    emitter.emit_chrome_progress(&mut eval, ChromeRowProgress::new(output, progress));

    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");

    assert_eq!(
        display.output_cursor,
        Some(neovm_core::window::WindowCursorPos {
            x: 40,
            y: 18,
            row: 2,
            col: 5,
        })
    );
    assert_eq!(emitter.rows().len(), 1);
    assert_eq!(emitter.rows()[0].row, 2);
    assert_eq!(emitter.rows()[0].height, 14);
}

#[test]
fn text_matrix_row_output_begins_row() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-row-output", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 10, Rect::new(0.0, 0.0, 80.0, 16.0), true);

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    let outcome = begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 0.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );

    assert_eq!(outcome, 0);

    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(
        display.output_cursor,
        Some(neovm_core::window::WindowCursorPos {
            x: 0,
            y: 0,
            row: 0,
            col: 0,
        })
    );

    builder.end_row();
    builder.end_window();
}

#[test]
fn text_matrix_row_output_finishes_with_matrix_metrics() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-row-apply", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 10, Rect::new(0.0, 4.0, 80.0, 16.0), true);

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 4.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );
    let outcome = finish_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        DisplayTextRowMetrics {
            y: 20.0,
            height: 16.0,
            ascent: 11.0,
        },
    );

    assert_eq!(
        outcome.metrics,
        DisplayTextRowStoredMetrics {
            pixel_y: 16.0,
            height_px: 16.0,
            ascent_px: 11.0,
        }
    );
    assert_eq!(outcome.display_row_index, 0);

    builder.end_row();
    builder.end_window();
}

#[test]
fn record_text_window_display_range_updates_matching_last_window_info() {
    let mut builder = DisplayOutputBuilder::new();
    builder.add_output_window_info(window_info(41));
    builder.install_window_metadata(
        crate::output::install_request::OutputPresentedWindowGeometryInstallRequest {
            window_id: neomacs_display_protocol::DisplayWindowId::new(41),
            geometry: neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Skipped {
                cell_origin: neomacs_display_protocol::frame_glyphs::PresentedCellOrigin::default(),
                outer: Rect::new(0.0, 0.0, 80.0, 16.0),
            },
        },
    );

    record_text_window_display_range(
        TextWindowOutputTarget::from_builder(&mut builder),
        TextWindowDisplayRange {
            window_id: 41,
            window_start: LispCharPos1::new(7),
            window_end: LispCharPos1::new(19),
        },
    );

    let info = builder.window_infos().last().expect("window info");
    assert_eq!(info.window_start, 7);
    assert_eq!(info.window_end, 19);

    record_text_window_display_range(
        TextWindowOutputTarget::from_builder(&mut builder),
        TextWindowDisplayRange {
            window_id: 42,
            window_start: LispCharPos1::new(11),
            window_end: LispCharPos1::new(23),
        },
    );

    let info = builder.window_infos().last().expect("window info");
    assert_eq!(info.window_start, 7);
    assert_eq!(info.window_end, 19);
}

#[test]
fn text_window_redisplay_positions_use_last_row_with_buffer_position() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval.frame_manager_mut().create_frame(
        "output-emitter-redisplay-positions",
        320,
        120,
        buf_id,
    );
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 2, 10, Rect::new(0.0, 0.0, 80.0, 32.0), true);
    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);

    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 0.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );
    emitter.note_display_buffer_pos(LispCharPos1::new(7));
    finish_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        DisplayTextRowMetrics {
            y: 0.0,
            height: 16.0,
            ascent: 12.0,
        },
    );
    builder.end_row();

    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 1,
            row: 1,
            col: 0,
            y: 16.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );
    finish_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        DisplayTextRowMetrics {
            y: 16.0,
            height: 16.0,
            ascent: 12.0,
        },
    );

    let positions = TextWindowRedisplayPositions::from_output_rows(&emitter, 3, 100, 4);

    assert_eq!(positions.window_start(), LispCharPos1::new(4));
    assert_eq!(positions.window_end_lisp(), LispCharPos1::new(8));
    assert_eq!(
        positions.window_end_position().anchor().emacs_byte_pos(),
        EmacsBytePos::new(104)
    );
    assert_eq!(positions.window_end_position().matrix_row().get(), 1);
    assert_eq!(
        positions.display_range(41),
        TextWindowDisplayRange {
            window_id: 41,
            window_start: LispCharPos1::new(4),
            window_end: LispCharPos1::new(8),
        }
    );
}

#[test]
fn close_text_window_output_closes_active_matrix_window() {
    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(9, 1, 5, Rect::new(0.0, 0.0, 40.0, 16.0), true);

    close_text_window_output(TextWindowOutputTarget::from_builder(&mut builder));

    assert_eq!(builder.completed_window_count(), 1);
    assert_eq!(builder.completed_window_id(0), Some(9));
}

#[test]
fn publish_text_window_cursor_installs_selected_phys_cursor_without_window_cursor_item() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-selected-cursor", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(window_id.0, 1, 10, Rect::new(0.0, 0.0, 80.0, 16.0), true);
    builder.begin_row(0, GlyphRowRole::Text);
    write_left_margin_char_to_current_row(&mut builder, '1', FaceId::new(7));
    write_left_margin_stretch_to_current_row(&mut builder, 1, FaceId::new(7));
    write_char_to_current_row(&mut builder, 'H', FaceId::new(3), 100);

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 16.0, 8.0);
    let outcome = publish_text_window_cursor(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        TextWindowCursor {
            role: TextWindowCursorRole::Active,
            window_id: window_id.0 as i64,
            charpos: 100,
            slots: TextWindowCursorSlots::from_capture(
                DisplaySlotId {
                    window_id: neomacs_display_protocol::types::DisplayWindowId::new(
                        window_id.0 as i64,
                    ),
                    row: 0,
                    col: 0,
                },
                CursorSlotResolutionState::Unresolved,
            ),
            x: 40.0,
            y: 24.0,
            width: 8.0,
            height: 16.0,
            ascent: 12.0,
            style: CursorStyle::FilledBox,
            color: Color::WHITE,
            cursor_fg: Color::BLACK,
            text_area_left: 16.0,
            window_top: 8.0,
            grid_x_override: None,
        },
    );

    builder.end_row();
    builder.end_window();
    let snapshot = emitter.finish_snapshot_with_geometry(
        &mut eval,
        neovm_core::window::geometry::CellOrigin::default(),
        neovm_core::window::PresentedWindowRegions::default(),
        0,
        0,
        0,
    );
    let state = builder.finish(10, 1, 8.0, 16.0);

    assert!(state.cursors.is_empty());
    let phys = state.phys_cursor.expect("selected phys cursor");
    assert_eq!(phys.slot_id.col, 2);
    assert_eq!(state.window_matrices[0].matrix.rows[0].cursor_col, Some(2));
    assert_eq!(outcome.installed_cursor_artifact, false);
    assert_eq!(outcome.stored_phys_cursor, true);
    assert_eq!(outcome.row, 0);
    assert_eq!(outcome.row_col, 2);

    let live = snapshot.phys_cursor.expect("live phys cursor");
    assert_eq!(live.x, 24);
    assert_eq!(live.y, 16);
    assert_eq!(live.row, 0);
    assert_eq!(live.col, 0);
}

#[test]
fn publish_non_selected_cursor_resolves_one_gutter_aware_slot_for_every_artifact() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval.frame_manager_mut().create_frame(
        "output-emitter-non-selected-cursor",
        320,
        120,
        buf_id,
    );
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(window_id.0, 1, 10, Rect::new(0.0, 0.0, 80.0, 16.0), false);
    builder.begin_row(0, GlyphRowRole::Text);
    write_left_margin_char_to_current_row(&mut builder, '1', FaceId::new(7));
    write_left_margin_stretch_to_current_row(&mut builder, 1, FaceId::new(7));
    write_char_to_current_row(&mut builder, 'H', FaceId::new(3), 100);

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 16.0, 8.0);
    let outcome = publish_text_window_cursor(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        TextWindowCursor {
            role: TextWindowCursorRole::Inactive,
            window_id: window_id.0 as i64,
            charpos: 100,
            slots: TextWindowCursorSlots::from_capture(
                DisplaySlotId {
                    window_id: neomacs_display_protocol::types::DisplayWindowId::new(
                        window_id.0 as i64,
                    ),
                    row: 0,
                    col: 0,
                },
                CursorSlotResolutionState::Unresolved,
            ),
            x: 40.0,
            y: 24.0,
            width: 8.0,
            height: 16.0,
            ascent: 12.0,
            style: CursorStyle::Hollow,
            color: Color::WHITE,
            cursor_fg: Color::BLACK,
            text_area_left: 16.0,
            window_top: 8.0,
            grid_x_override: None,
        },
    );

    builder.end_row();
    builder.end_window();
    let snapshot = emitter.finish_snapshot_with_geometry(
        &mut eval,
        neovm_core::window::geometry::CellOrigin::default(),
        neovm_core::window::PresentedWindowRegions::default(),
        0,
        0,
        0,
    );
    let state = builder.finish(10, 1, 8.0, 16.0);

    assert!(state.phys_cursor.is_none());
    assert_eq!(state.cursors.len(), 1);
    assert_eq!(state.cursors[0].slot_id.col, 2);
    assert_eq!(state.window_matrices[0].matrix.rows[0].cursor_col, Some(2));
    assert!(outcome.installed_cursor_artifact);
    assert!(!outcome.stored_phys_cursor);
    assert_eq!(outcome.row_col, 2);
    assert_eq!(outcome.live_cursor.col, 0);
    assert_eq!(snapshot.phys_cursor.expect("live cursor").col, 0);
}

#[test]
fn publish_selected_empty_gutter_cursor_preserves_layout_x_through_materialization() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval.frame_manager_mut().create_frame(
        "output-emitter-empty-gutter-cursor",
        504,
        48,
        buf_id,
    );
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window_with_text_bounds(
        window_id.0,
        3,
        58,
        Rect::new(160.0, 384.0, 504.0, 48.0),
        Rect::new(188.0, 384.0, 467.0, 48.0),
        true,
    );
    builder.begin_row(1, GlyphRowRole::Text);
    write_left_margin_char_to_current_row(&mut builder, '2', FaceId::new(7));
    write_left_margin_char_to_current_row(&mut builder, ' ', FaceId::new(7));
    write_left_margin_stretch_to_current_row(&mut builder, 2, FaceId::new(7));
    write_stretch_to_current_row(
        &mut builder,
        54,
        435.0,
        FaceId::new(9),
        GlyphProvenance::line_end(),
    );

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 16.0, 8.0);
    let outcome = publish_text_window_cursor(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        TextWindowCursor {
            role: TextWindowCursorRole::Active,
            window_id: window_id.0 as i64,
            charpos: 6,
            slots: TextWindowCursorSlots::from_capture(
                DisplaySlotId {
                    window_id: neomacs_display_protocol::types::DisplayWindowId::new(
                        window_id.0 as i64,
                    ),
                    row: 1,
                    col: 0,
                },
                CursorSlotResolutionState::Unresolved,
            ),
            x: 220.0,
            y: 400.0,
            width: 8.0,
            height: 16.0,
            ascent: 12.0,
            style: CursorStyle::FilledBox,
            color: Color::WHITE,
            cursor_fg: Color::BLACK,
            text_area_left: 188.0,
            window_top: 384.0,
            grid_x_override: None,
        },
    );

    builder.end_row();
    builder.end_window();
    let state = builder.finish(58, 3, 8.0, 16.0);
    let phys = state.phys_cursor.as_ref().expect("selected phys cursor");
    assert_eq!(outcome.row_col, 4);
    assert_eq!(phys.slot_id.col, 4);
    assert_eq!(phys.x, 220.0);
    let slot_id = phys.slot_id;

    let buffer = state.materialize();
    assert!(matches!(
        buffer.slot_glyph(slot_id),
        Some(neomacs_display_protocol::frame_glyphs::FrameGlyph::Stretch { .. })
    ));
    assert_eq!(buffer.active_cursor().expect("active cursor").x, 220.0);
}

#[test]
fn publish_text_window_decorative_cursor_installs_cursor_item_and_effects_only() {
    let mut builder = DisplayOutputBuilder::new();
    let effects = EffectsConfig::default();

    publish_text_window_decorative_cursor(
        TextWindowOutputTarget::from_builder(&mut builder),
        TextWindowDecorativeCursor {
            window_id: 77,
            slot_id: DisplaySlotId {
                window_id: neomacs_display_protocol::types::DisplayWindowId::new(77),
                row: 3,
                col: 5,
            },
            x: 40.0,
            y: 24.0,
            width: 8.0,
            height: 16.0,
            style: CursorStyle::Bar(2.0),
            color: Color::WHITE,
            cursor_fg: Color::BLACK,
            effects: Some(effects.clone()),
        },
    );

    let state = builder.finish(10, 1, 8.0, 16.0);
    assert!(state.phys_cursor.is_none());
    assert_eq!(state.cursors.len(), 1);
    assert_eq!(state.cursors[0].window_id.get(), 77);
    assert_eq!(state.cursors[0].slot_id.row, 3);
    assert_eq!(state.cursors[0].slot_id.col, 5);
    assert_eq!(state.cursors[0].cursor_fg, Color::BLACK);
    assert_eq!(
        state
            .cursor_effects_by_window
            .get(&neomacs_display_protocol::types::DisplayWindowId::new(77)),
        Some(&effects)
    );
}

#[test]
fn finished_snapshot_publishes_selected_window_outer_body_and_cell_origin() {
    use neomacs_display_protocol::types::Rect as TransportRect;
    use neovm_core::window::PresentedWindowRegions;
    use neovm_core::window::geometry::CellOrigin;

    let mut eval = neovm_core::emacs_core::Context::new();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("regions", 1975, 1214, buffer_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 168.0, 24.0);

    let regions = PresentedWindowRegions {
        outer: TransportRect::new(144.0, 24.0, 1831.0, 1172.0),
        text_body: TransportRect::new(168.0, 41.0, 1807.0, 1138.0),
        ..PresentedWindowRegions::default()
    };
    let snapshot = emitter.finish_snapshot_with_geometry(
        &mut eval,
        CellOrigin::new(20, 1),
        regions,
        17,
        5,
        12,
    );

    assert_eq!(
        snapshot.regions.outer,
        TransportRect::new(144.0, 24.0, 1831.0, 1172.0)
    );
    assert_eq!(
        snapshot.regions.text_body,
        TransportRect::new(168.0, 41.0, 1807.0, 1138.0)
    );
    assert_eq!(snapshot.cell_origin, CellOrigin::new(20, 1));
    assert_eq!(snapshot.header_line_height, 5);
    assert_eq!(snapshot.tab_line_height, 12);
}

#[test]
fn install_text_window_cursor_effects_records_window_effect_profile() {
    let mut builder = DisplayOutputBuilder::new();
    let effects = EffectsConfig::default();

    install_text_window_cursor_effects(
        TextWindowOutputTarget::from_builder(&mut builder),
        TextWindowCursorEffects {
            window_id: 42,
            effects: effects.clone(),
        },
    );

    let state = builder.finish(10, 1, 8.0, 16.0);
    assert_eq!(
        state
            .cursor_effects_by_window
            .get(&neomacs_display_protocol::types::DisplayWindowId::new(42)),
        Some(&effects)
    );
    assert!(state.cursors.is_empty());
    assert!(state.phys_cursor.is_none());
}

#[test]
fn text_matrix_row_commands_begin_and_finish_output() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-row-commands", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 10, Rect::new(0.0, 0.0, 80.0, 16.0), true);

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 0.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );

    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(
        display.output_cursor,
        Some(neovm_core::window::WindowCursorPos {
            x: 0,
            y: 0,
            row: 0,
            col: 0,
        })
    );

    finish_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        DisplayTextRowMetrics {
            y: 0.0,
            height: 16.0,
            ascent: 12.0,
        },
    );

    assert_eq!(emitter.rows().len(), 1);
    assert_eq!(emitter.rows()[0].row, 0);

    builder.end_row();
    builder.end_window();
    let state = builder.finish(10, 1, 8.0, 16.0);
    assert_eq!(state.window_matrices[0].matrix.rows.len(), 1);
}

#[test]
fn display_text_row_metrics_finish_and_end_closes_matrix_row() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-row-finish-end", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 10, Rect::new(0.0, 0.0, 80.0, 16.0), true);

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 0.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );

    finish_and_end_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        DisplayTextRowMetrics {
            y: 0.0,
            height: 16.0,
            ascent: 12.0,
        },
    );

    assert_eq!(emitter.rows().len(), 1);

    builder.end_window();
    let state = builder.finish(10, 1, 8.0, 16.0);
    assert_eq!(state.window_matrices[0].matrix.rows.len(), 1);
}

#[test]
fn finish_pending_text_window_row_records_hit_and_row_metrics() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-pending-text-row", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 10, Rect::new(0.0, 0.0, 80.0, 20.0), true);
    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 4.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );

    let row_geometry = DisplayRowGeometryState::new(0, 4.0, 0.0, 18.0, 13.0);
    let row_y_positions = DisplayRowYPositions::with_first_row(4.0, 18.0);
    let mut hit_row_range = HitRowRangeTracker::new(2);
    let mut hit_rows = Vec::new();

    let finished = finish_pending_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        TextWindowPendingRowFinish {
            source_exhausted: false,
            row_geometry: &row_geometry,
            row_limit: DisplayRowLimit { max_rows: 1 },
            row_y_positions: &row_y_positions,
            text_y: 4.0,
            char_height: 18.0,
            charpos: 5,
            hit_row_range: &mut hit_row_range,
            hit_rows: &mut hit_rows,
        },
    );

    assert!(finished);
    assert_eq!(hit_rows.len(), 1);
    assert_eq!(hit_rows[0].charpos_start, 2);
    assert_eq!(hit_rows[0].charpos_end, 5);
    assert_eq!(emitter.rows().len(), 1);
    assert_eq!(emitter.row_metrics()[0].pixel_y(), 4.0);
    assert_eq!(emitter.row_metrics()[0].height(), 18.0);
    assert_eq!(emitter.row_metrics()[0].ascent(), 13.0);
}

#[test]
fn install_text_window_output_installs_row_metrics() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval.frame_manager_mut().create_frame(
        "output-emitter-install-text-window",
        320,
        120,
        buf_id,
    );
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 5, Rect::new(0.0, 0.0, 40.0, 20.0), true);
    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 2.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );
    write_char_to_current_row(&mut builder, 'x', FaceId::new(7), 0);
    finish_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        DisplayTextRowMetrics {
            y: 2.0,
            height: 20.0,
            ascent: 15.0,
        },
    );

    install_text_window_finished_rows(TextWindowOutputTarget::from_builder(&mut builder), &emitter);

    builder.end_window();
    let state = builder.finish(5, 1, 8.0, 16.0);
    let row = &state.window_matrices[0].matrix.rows[0];

    assert_eq!(row.height_px, 20.0);
    assert_eq!(row.ascent_px, 15.0);
    assert_char_glyph(&row.glyphs[GlyphArea::Text.index()][0], 'x', FaceId::new(7));
}

#[test]
fn install_text_window_body_output_records_redisplay_and_installs_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-install-body", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.add_output_window_info(window_info(41));
    builder.install_window_metadata(
        crate::output::install_request::OutputPresentedWindowGeometryInstallRequest {
            window_id: neomacs_display_protocol::DisplayWindowId::new(41),
            geometry: neomacs_display_protocol::frame_glyphs::PresentedWindowGeometry::Skipped {
                cell_origin: neomacs_display_protocol::frame_glyphs::PresentedCellOrigin::default(),
                outer: Rect::new(0.0, 0.0, 80.0, 16.0),
            },
        },
    );
    builder.begin_window(41, 1, 5, Rect::new(0.0, 0.0, 40.0, 20.0), true);
    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 2.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );
    emitter.note_display_buffer_pos(LispCharPos1::new(7));
    write_char_to_current_row(&mut builder, 'x', FaceId::new(7), 0);
    finish_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        DisplayTextRowMetrics {
            y: 2.0,
            height: 20.0,
            ascent: 15.0,
        },
    );

    let positions = install_text_window_body_output(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        TextWindowBodyOutputInstall {
            window_id: 41,
            window_start: 3,
            text_start_byte: 100,
            byte_idx: 4,
            right_edge_markers: None,
        },
        None,
    );

    assert_eq!(positions.window_start(), LispCharPos1::new(4));
    assert_eq!(positions.window_end_lisp(), LispCharPos1::new(8));
    assert_eq!(
        positions.window_end_position().anchor().emacs_byte_pos(),
        EmacsBytePos::new(104)
    );
    assert_eq!(positions.window_end_position().matrix_row().get(), 0);
    let info = builder.window_infos().last().expect("window info");
    assert_eq!(info.window_start, 4);
    assert_eq!(info.window_end, 8);

    builder.end_window();
    let state = builder.finish(5, 1, 8.0, 16.0);
    let row = &state.window_matrices[0].matrix.rows[0];
    assert_eq!(row.height_px, 20.0);
    assert_eq!(row.ascent_px, 15.0);
    assert_char_glyph(&row.glyphs[GlyphArea::Text.index()][0], 'x', FaceId::new(7));
}

#[test]
fn mark_current_text_row_truncated_left_sets_current_row_flag() {
    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 2, 5, Rect::new(0.0, 0.0, 40.0, 32.0), true);
    builder.begin_row(1, GlyphRowRole::Text);

    install_text_window_row_decoration(
        &mut builder,
        TextWindowRowDecorationRequest::MarkCurrentTruncatedLeft,
    );

    builder.end_row();
    builder.end_window();
    let state = builder.finish(10, 2, 8.0, 16.0);
    let matrix = &state.window_matrices[0].matrix;
    assert!(!matrix.rows[0].truncated_left);
    assert!(matrix.rows[1].truncated_left);
}

#[test]
fn text_matrix_row_transition_finishes_without_starting_past_max_rows() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-row-exhaustion", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 1, 10, Rect::new(0.0, 0.0, 80.0, 16.0), true);

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 0.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );

    let transition = transition_text_window_row_with_limit(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowGeometryTransition {
            finished_row: DisplayTextRowMetrics {
                y: 0.0,
                height: 16.0,
                ascent: 12.0,
            },
            begin_row: DisplayTextRowBegin {
                display_row_index: 1,
                row: 1,
                col: 0,
                y: 16.0,
                x: 0.0,
                start_charpos: LayoutCharPos0::new(0),
            },
        },
        1,
    );

    assert_eq!(transition, DisplayTextRowTransition::ExhaustedRows);
    assert_eq!(emitter.rows().len(), 1);

    builder.end_window();
    let state = builder.finish(10, 1, 8.0, 16.0);
    assert_eq!(state.window_matrices[0].matrix.rows.len(), 1);
}

#[test]
fn text_matrix_row_transition_emits_finish_and_begin() {
    let mut eval = Context::new();
    let buf_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("output-emitter-row-transition", 320, 120, buf_id);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;

    let mut builder = DisplayOutputBuilder::new();
    builder.begin_window(1, 2, 10, Rect::new(0.0, 0.0, 80.0, 32.0), true);

    let mut emitter = WindowOutputEmitter::new(frame_id, window_id, 0, 0.0, 0.0);
    emitter.begin_update(&mut eval);
    begin_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowBegin {
            display_row_index: 0,
            row: 0,
            col: 0,
            y: 0.0,
            x: 0.0,
            start_charpos: LayoutCharPos0::new(0),
        },
    );

    transition_text_window_row(
        TextWindowOutputTarget::from_builder(&mut builder),
        &mut emitter,
        &mut eval,
        DisplayTextRowGeometryTransition {
            finished_row: DisplayTextRowMetrics {
                y: 0.0,
                height: 16.0,
                ascent: 12.0,
            },
            begin_row: DisplayTextRowBegin {
                display_row_index: 1,
                row: 1,
                col: 0,
                y: 16.0,
                x: 0.0,
                start_charpos: LayoutCharPos0::new(0),
            },
        },
    );

    assert_eq!(emitter.rows().len(), 1);
    assert_eq!(emitter.rows()[0].row, 0);
    let display = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.find_window(window_id))
        .and_then(|window| window.display())
        .expect("window display state");
    assert_eq!(
        display.output_cursor,
        Some(neovm_core::window::WindowCursorPos {
            x: 0,
            y: 16,
            row: 1,
            col: 0,
        })
    );

    builder.end_row();
    builder.end_window();
    let state = builder.finish(10, 1, 8.0, 16.0);
    assert_eq!(state.window_matrices[0].matrix.rows.len(), 2);
}

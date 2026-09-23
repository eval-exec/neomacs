use super::*;
use crate::buffer_source::producer::vocabulary::{
    ProducedGlyphProvenance, covered_text_glyph, natural_text_glyph,
    provenance_from_source_position,
};
use crate::display_item::{
    DisplayGlyphless, DisplayItem, DisplayItemKind, DisplayLength, DisplaySourceMappedText,
    DisplaySourcePosition, DisplayStretch, DisplayStretchWidth, DisplayTextRun, GlyphlessMethod,
    RenderFaceRef, SourceSpan,
};
use crate::display_source::{
    DisplayItemSource, DisplaySourceContext, LispStringSourceCursor, LispStringSourceOrigin,
};
use crate::display_text_run_measurement::{
    DisplayTextRunAdvance, DisplayTextRunByteAdvance, DisplayTextRunMeasurement,
    DisplayTextRunMeasurementPlan,
};
use crate::output::builder::DisplayOutputBuilder;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphArea, GlyphPointerAppearance, GlyphPointerOccurrenceIdentity,
    GlyphPointerSourceIdentity, GlyphPointerSourceKind, GlyphProvenance, GlyphRow, GlyphStringId,
    GlyphStringSource, GlyphType, GlyphlessPresentation,
};
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::Rect;
use neovm_core::emacs_core::emacs_char::EmacsChar;
use neovm_core::emacs_core::{Context, Value};

fn layout() -> DisplayRowLayout {
    DisplayRowLayout {
        line_number_width_px: 0.0,
        role: GlyphRowRole::Text,
        y_px: 0.0,
        height_px: 16.0,
        ascent_px: 12.0,
        char_width_px: 8.0,
        tab_policy: DisplayTabPolicy::every(4),
        base_face: RenderFaceRef::FaceId(FaceId::new(1)),
        pixel_calc: crate::display_pixel_calc::PixelCalcContext::for_chrome_row(
            240.0,
            8.0,
            16.0,
            std::collections::HashMap::new(),
        ),
        space_image_params: None,
    }
}

fn text_item(text: &str) -> DisplayItem {
    DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::lisp_string(1, 0, 0),
            DisplaySourcePosition::lisp_string(1, text.chars().count(), text.len()),
        ),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::TextRun(DisplayTextRun::new(text)),
    )
}

fn independent_text_item(text: &str) -> DisplayItem {
    DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::lisp_string(1, 0, 0),
            DisplaySourcePosition::lisp_string(1, text.chars().count(), text.len()),
        ),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::TextRun(DisplayTextRun::independent(text)),
    )
}

fn pointer_appearance(source_id: u64) -> GlyphPointerAppearance {
    GlyphPointerAppearance {
        source: GlyphPointerSourceIdentity {
            kind: GlyphPointerSourceKind::LispString,
            source_id,
            range_start: 0,
            range_end: 1,
            property_owner: 0,
            occurrence: GlyphPointerOccurrenceIdentity::Source,
        },
        face_id: FaceId::new(9),
    }
}

#[test]
fn glyph_checkpoint_restores_pointer_table_for_wrap_boundaries() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let first_margins = neomacs_display_protocol::ImageMargins::new(1.0, 2.0);
    row.intern_image_margins(first_margins)
        .expect("first image-margin token");
    let first_source = row
        .push_string_source(GlyphStringSource::new(GlyphStringId::new(11)))
        .expect("first string source");
    let first = row
        .intern_pointer_appearance(pointer_appearance(1))
        .expect("first pointer token");
    row.glyphs[GlyphArea::Text.index()].push(Glyph {
        pointer_appearance: Some(first),
        ..Glyph::char_with_provenance(
            'a',
            FaceId::new(0),
            GlyphProvenance::string(first_source, 0),
        )
    });
    let before_run = DisplayRowGlyphCheckpoint::capture(&row);

    let second_margins = neomacs_display_protocol::ImageMargins::asymmetric(3.0, 4.0, 5.0, 6.0);
    row.intern_image_margins(second_margins)
        .expect("second image-margin token");
    let second_source = row
        .push_string_source(GlyphStringSource::new(GlyphStringId::new(12)))
        .expect("second string source");
    let second = row
        .intern_pointer_appearance(pointer_appearance(2))
        .expect("second pointer token");
    row.glyphs[GlyphArea::Text.index()].push(Glyph {
        pointer_appearance: Some(second),
        ..Glyph::char_with_provenance(
            'b',
            FaceId::new(0),
            GlyphProvenance::string(second_source, 0),
        )
    });
    let after_run = DisplayRowGlyphCheckpoint::capture(&row);

    let mut keep_run_prefix = row.clone();
    before_run
        .with_added_text_glyphs(1, after_run)
        .restore(&mut keep_run_prefix);
    assert_eq!(keep_run_prefix.pointer_appearances().len(), 2);
    assert_eq!(keep_run_prefix.image_margins_table().len(), 2);
    assert_eq!(keep_run_prefix.string_sources().len(), 2);
    let token = keep_run_prefix.glyphs[GlyphArea::Text.index()][1]
        .pointer_appearance
        .expect("retained prefix pointer token");
    assert_eq!(
        keep_run_prefix.pointer_appearance(token),
        Some(&pointer_appearance(2))
    );

    before_run.restore(&mut row);
    assert_eq!(row.glyphs[GlyphArea::Text.index()].len(), 1);
    assert_eq!(row.pointer_appearances().len(), 1);
    assert_eq!(row.image_margins_table(), &[first_margins]);
    assert_eq!(row.string_sources().len(), 1);
    assert_eq!(row.pointer_appearance(first), Some(&pointer_appearance(1)));
    assert_eq!(
        row.string_source(first_source),
        Some(&GlyphStringSource::new(GlyphStringId::new(11)))
    );
}

fn glyphless_item(ch: char, method: GlyphlessMethod) -> DisplayItem {
    DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::lisp_string(1, 0, 0),
            DisplaySourcePosition::lisp_string(1, 1, ch.len_utf8()),
        ),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::Glyphless(DisplayGlyphless { ch, method }),
    )
}

fn control_item(ch: char) -> DisplayItem {
    DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::lisp_string(1, 0, 0),
            DisplaySourcePosition::lisp_string(1, 1, ch.len_utf8()),
        ),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::ControlChar { ch },
    )
}

fn mapped_text_item(text: &str) -> DisplayItem {
    DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::lisp_string(1, 0, 0),
            DisplaySourcePosition::lisp_string(1, 1, text.len()),
        ),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new(text)),
    )
}

fn stretch_item(width: DisplayLength) -> DisplayItem {
    DisplayItem::new(
        SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(width),
            height: None,
            ascent: None,
        }),
    )
}

fn row_text(row: &neomacs_display_protocol::glyph_matrix::GlyphRow) -> String {
    let mut text = String::new();
    for glyph in row.glyphs[GlyphArea::Text.index()]
        .iter()
        .filter(|glyph| !glyph.padding)
    {
        match &glyph.glyph_type {
            GlyphType::Char { ch } | GlyphType::Glyphless { ch, .. } => text.push(*ch),
            GlyphType::Composite { text: cluster }
            | GlyphType::AutomaticComposite { text: cluster, .. } => text.push_str(cluster),
            GlyphType::Stretch { width_cols } => {
                text.push_str(&" ".repeat(usize::from(*width_cols)))
            }
            GlyphType::Image { .. }
            | GlyphType::Video { .. }
            | GlyphType::Xwidget { .. }
            | GlyphType::Surface { .. } => {}
        }
    }
    text
}

#[test]
fn display_row_text_char_state_names_row_tail_policy() {
    assert_eq!(
        DisplayRowTextCharState::for_tail('\t', Some(('x', false))).kind(),
        DisplayRowTextNaturalAdvanceKind::Tab
    );
    assert_eq!(
        DisplayRowTextCharState::for_tail('\u{301}', Some(('e', false))).kind(),
        DisplayRowTextNaturalAdvanceKind::ClusterContinuation
    );
    assert_eq!(
        DisplayRowTextCharState::for_tail('\u{0633}', Some(('\u{0627}', false))).kind(),
        DisplayRowTextNaturalAdvanceKind::ComplexRunMember
    );
    assert_eq!(
        DisplayRowTextCharState::for_tail('中', None).kind(),
        DisplayRowTextNaturalAdvanceKind::FaceColumns { columns: 2 }
    );
    assert_eq!(
        DisplayRowTextCharState::independent('ꦧ').kind(),
        DisplayRowTextNaturalAdvanceKind::StandaloneZeroWidth
    );
}

#[test]
fn independent_text_keeps_zero_width_characters_as_independent_glyphs() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let progress = {
        let mut writer = DisplayRowProgressWriter::new(
            &row_layout,
            &mut row,
            DisplayRowPosition::new(0.0, 0),
            80.0,
        );
        writer.push_item(independent_text_item("ꦧꦱꦗ"))
    };

    assert_eq!(progress.end().col(), 0);
    assert_eq!(
        row.glyphs[GlyphArea::Text.index()]
            .iter()
            .map(|glyph| &glyph.glyph_type)
            .collect::<Vec<_>>(),
        vec![
            &GlyphType::Char { ch: 'ꦧ' },
            &GlyphType::Char { ch: 'ꦱ' },
            &GlyphType::Char { ch: 'ꦗ' },
        ]
    );
}

#[test]
fn display_row_progress_writer_skips_zero_width_glyphless_item() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer = DisplayRowProgressWriter::new(
        &row_layout,
        &mut row,
        DisplayRowPosition::new(16.0, 2),
        80.0,
    );

    let progress = writer.push_item(glyphless_item('\u{200b}', GlyphlessMethod::ZeroWidth));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(progress.end(), DisplayRowPosition::new(16.0, 2));
    assert!(progress.slots().is_empty());
    assert!(row.glyphs[GlyphArea::Text.index()].is_empty());
}

#[test]
fn display_row_progress_writer_uses_empty_box_glyphless_width() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);

    let progress = writer.push_item(glyphless_item('\u{fffc}', GlyphlessMethod::EmptyBox));

    assert_eq!(progress.end(), DisplayRowPosition::new(8.0, 1));
    let glyph = &row.glyphs[GlyphArea::Text.index()][0];
    assert_eq!(
        glyph.glyph_type,
        GlyphType::Glyphless {
            ch: '\u{fffc}',
            presentation: GlyphlessPresentation::EmptyBox,
        }
    );
    assert_eq!(glyph.pixel_width, 8.0);
}

#[test]
fn display_row_progress_writer_uses_hex_code_glyphless_width() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);

    let progress = writer.push_item(glyphless_item('\u{fff0}', GlyphlessMethod::HexCode));

    assert_eq!(progress.end(), DisplayRowPosition::new(48.0, 6));
    assert_eq!(progress.slots()[0].width_px(), 48.0);
    assert_eq!(progress.slots()[0].width_cols(), 6);
    let glyph = &row.glyphs[GlyphArea::Text.index()][0];
    assert_eq!(
        glyph.glyph_type,
        GlyphType::Glyphless {
            ch: '\u{fff0}',
            presentation: GlyphlessPresentation::HexCode,
        }
    );
    assert_eq!(glyph.pixel_width, 48.0);
}

#[test]
fn display_row_progress_writer_uses_thin_space_glyphless_width() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);

    let progress = writer.push_item(glyphless_item('\u{2009}', GlyphlessMethod::ThinSpace));

    assert_eq!(progress.end(), DisplayRowPosition::new(2.0, 1));
    let glyph = &row.glyphs[GlyphArea::Text.index()][0];
    assert_eq!(
        glyph.glyph_type,
        GlyphType::Glyphless {
            ch: '\u{2009}',
            presentation: GlyphlessPresentation::ThinSpace,
        }
    );
    assert_eq!(glyph.pixel_width, 2.0);
}

#[test]
fn display_row_progress_writer_renders_tty_glyphless_acronyms_as_text() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);
    let acronym = crate::display_item::GlyphlessAcronym::from_ascii("v")
        .expect("one-character ASCII acronym");

    let progress = writer.push_item(glyphless_item('▼', GlyphlessMethod::Acronym(acronym)));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(progress.end(), DisplayRowPosition::new(8.0, 1));
    assert_eq!(row_text(&row), "v");
}

#[test]
fn display_row_progress_writer_clips_glyphless_before_row_mutation() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer = DisplayRowProgressWriter::new(
        &row_layout,
        &mut row,
        DisplayRowPosition::new(40.0, 5),
        80.0,
    );

    let progress = writer.push_item(glyphless_item('\u{fff0}', GlyphlessMethod::HexCode));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Clipped);
    assert_eq!(progress.end(), DisplayRowPosition::new(40.0, 5));
    assert!(progress.slots().is_empty());
    assert!(row.glyphs[GlyphArea::Text.index()].is_empty());
    assert!(!row.displays_text);
}

#[test]
fn display_row_progress_writer_clips_stretch_before_row_mutation() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer = DisplayRowProgressWriter::new(
        &row_layout,
        &mut row,
        DisplayRowPosition::new(64.0, 8),
        80.0,
    );

    let progress = writer.push_item(stretch_item(DisplayLength::Pixels(24.0)));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Clipped);
    assert_eq!(progress.end(), DisplayRowPosition::new(64.0, 8));
    assert!(progress.slots().is_empty());
    assert!(row.glyphs[GlyphArea::Text.index()].is_empty());
    assert!(!row.displays_text);
}

#[test]
fn display_row_builder_renders_control_char_as_caret_notation() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(control_item('\u{0001}'));

    let row = builder.finish();
    let glyphs = &row.glyphs[GlyphArea::Text.index()];

    assert_eq!(row_text(&row), "^A");
    assert_eq!(glyphs.len(), 2);
    assert!(glyphs.iter().all(|glyph| glyph.legacy_charpos() == 0));
}

#[test]
fn display_row_builder_renders_delete_control_char_as_caret_question() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(control_item('\u{007f}'));

    let row = builder.finish();

    assert_eq!(row_text(&row), "^?");
}

#[test]
fn display_row_progress_writer_reports_control_char_as_single_source_slot() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(8.0, 1), 80.0);

    let progress = writer.push_item(control_item('\u{0001}'));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(progress.end(), DisplayRowPosition::new(24.0, 3));
    assert_eq!(progress.slots().len(), 1);
    assert_eq!(progress.slots()[0].width_px(), 16.0);
    assert_eq!(progress.slots()[0].width_cols(), 2);
    assert_eq!(row_text(&row), "^A");
}

#[test]
fn append_display_item_to_current_text_row_returns_progress_and_updates_row() {
    let row_layout = layout();
    let mut matrix = DisplayOutputBuilder::new();
    matrix.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    matrix.begin_row(0, GlyphRowRole::Text);

    let progress = append_display_item_to_current_text_row(
        &mut matrix,
        &row_layout,
        text_item("ab"),
        DisplayRowPosition::new(8.0, 1),
        80.0,
    )
    .expect("append progress");

    assert_eq!(progress.start(), DisplayRowPosition::new(8.0, 1));
    assert_eq!(progress.end(), DisplayRowPosition::new(24.0, 3));
    assert_eq!(progress.slots().len(), 2);
    matrix
        .edit_current_row_for_test(|row| {
            assert_eq!(row_text(row), "ab");
        })
        .expect("current row");
}

#[test]
fn append_measured_display_item_to_current_text_row_uses_glyph_measurer() {
    struct TestMeasurer;

    impl DisplayGlyphMeasurer for TestMeasurer {
        fn glyph_advance_px(
            &mut self,
            ch: char,
            _face_id: FaceId,
            _columns: u8,
            _fallback_advance_px: f32,
        ) -> Option<f32> {
            match ch {
                'm' => Some(12.0),
                'i' => Some(4.0),
                _ => None,
            }
        }
    }

    let row_layout = layout();
    let mut matrix = DisplayOutputBuilder::new();
    let mut measurer = TestMeasurer;
    matrix.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    matrix.begin_row(0, GlyphRowRole::Text);

    let progress = append_measured_display_item_to_current_text_row(
        &mut matrix,
        &row_layout,
        text_item("mi"),
        &mut measurer,
        DisplayRowPosition::new(0.0, 0),
        80.0,
    )
    .expect("append progress");

    assert_eq!(progress.end(), DisplayRowPosition::new(16.0, 2));
    matrix
        .edit_current_row_for_test(|row| {
            let glyphs = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(glyphs[0].pixel_width, 12.0);
            assert_eq!(glyphs[1].pixel_width, 4.0);
        })
        .expect("current row");
}

#[test]
fn measured_text_glyph_carries_the_selected_font_vertical_metrics() {
    struct FallbackFontMeasurer;

    impl DisplayGlyphMeasurer for FallbackFontMeasurer {
        fn glyph_advance_px(
            &mut self,
            _ch: char,
            _face_id: FaceId,
            _columns: u8,
            _fallback_advance_px: f32,
        ) -> Option<f32> {
            Some(10.0)
        }

        fn glyph_vertical_metrics_px(
            &mut self,
            _ch: char,
            _face_id: FaceId,
        ) -> Option<DisplayRowVerticalMetrics> {
            Some(DisplayRowVerticalMetrics::new(12.0, 9.0))
        }
    }

    let row_layout = layout();
    let mut row = GlyphRow::new(GlyphRowRole::TabLine);
    let mut measurer = FallbackFontMeasurer;
    let mut writer = DisplayRowWriter::with_glyph_measurer(&row_layout, &mut row, &mut measurer);
    writer.push_item(text_item("\u{e632}"));

    let glyph = &row.glyphs[GlyphArea::Text.index()][0];
    assert_eq!(glyph.pixel_height, 12.0);
    assert_eq!(glyph.pixel_ascent, 9.0);
}

#[test]
fn display_row_append_cursor_updates_position_after_append() {
    let row_layout = layout();
    let mut matrix = DisplayOutputBuilder::new();
    matrix.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    matrix.begin_row(0, GlyphRowRole::Text);

    let mut cursor = DisplayRowAppendCursor::new(DisplayRowPosition::new(8.0, 1), 80.0);
    let progress = cursor
        .append_item_to_current_text_row(&mut matrix, &row_layout, text_item("ab"))
        .expect("append progress");

    assert_eq!(progress.start(), DisplayRowPosition::new(8.0, 1));
    assert_eq!(progress.end(), DisplayRowPosition::new(24.0, 3));
    assert_eq!(cursor.position(), DisplayRowPosition::new(24.0, 3));
    matrix
        .edit_current_row_for_test(|row| {
            assert_eq!(row_text(row), "ab");
        })
        .expect("current row");
}

#[test]
fn display_row_append_cursor_updates_position_to_clipped_end() {
    let row_layout = layout();
    let mut matrix = DisplayOutputBuilder::new();
    matrix.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    matrix.begin_row(0, GlyphRowRole::Text);

    let mut cursor = DisplayRowAppendCursor::new(DisplayRowPosition::new(8.0, 1), 16.0);
    let progress = cursor
        .append_item_to_current_text_row(&mut matrix, &row_layout, text_item("ab"))
        .expect("append progress");

    assert_eq!(progress.status(), DisplayRowAppendStatus::Clipped);
    assert_eq!(progress.end(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(cursor.position(), DisplayRowPosition::new(16.0, 2));
    matrix
        .edit_current_row_for_test(|row| {
            assert_eq!(row_text(row), "a");
        })
        .expect("current row");
}

#[test]
fn display_row_append_cursor_uses_glyph_measurer() {
    let row_layout = layout();
    let mut matrix = DisplayOutputBuilder::new();
    let mut measurer = FixedGlyphAdvances::new();
    measurer.insert('m', FaceId::new(2), 12.0);
    measurer.insert('i', FaceId::new(2), 4.0);
    matrix.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    matrix.begin_row(0, GlyphRowRole::Text);

    let mut cursor = DisplayRowAppendCursor::new(DisplayRowPosition::new(0.0, 0), 80.0);
    let progress = cursor
        .append_measured_item_to_current_text_row(
            &mut matrix,
            &row_layout,
            text_item("mi"),
            &mut measurer,
        )
        .expect("append progress");

    assert_eq!(progress.end(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(cursor.position(), DisplayRowPosition::new(16.0, 2));
    matrix
        .edit_current_row_for_test(|row| {
            let glyphs = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(glyphs[0].pixel_width, 12.0);
            assert_eq!(glyphs[1].pixel_width, 4.0);
        })
        .expect("current row");
}

#[test]
fn display_row_append_cursor_appends_explicit_source_item() {
    let _eval = Context::new();
    let row_layout = layout();
    let mut matrix = DisplayOutputBuilder::new();
    matrix.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    matrix.begin_row(0, GlyphRowRole::Text);
    let mut source = LispStringSourceCursor::new(
        1,
        Value::string("abc"),
        RenderFaceRef::FaceId(FaceId::new(2)),
        LispStringSourceOrigin::Normal,
    )
    .expect("source");
    let mut context = DisplaySourceContext::empty();

    let mut cursor = DisplayRowAppendCursor::new(DisplayRowPosition::new(8.0, 1), 80.0);
    let item = source.next_item(&mut context).expect("source item");
    let progress = cursor
        .append_item_to_current_text_row(&mut matrix, &row_layout, item)
        .expect("append progress");

    assert_eq!(progress.start(), DisplayRowPosition::new(8.0, 1));
    assert_eq!(progress.end(), DisplayRowPosition::new(32.0, 4));
    assert_eq!(cursor.position(), DisplayRowPosition::new(32.0, 4));
    matrix
        .edit_current_row_for_test(|row| {
            assert_eq!(row_text(row), "abc");
        })
        .expect("current row");
}

#[test]
fn display_row_append_cursor_appends_explicit_source_item_with_glyph_measurer() {
    let _eval = Context::new();
    let row_layout = layout();
    let mut matrix = DisplayOutputBuilder::new();
    let mut measurer = FixedGlyphAdvances::new();
    measurer.insert('m', FaceId::new(2), 12.0);
    measurer.insert('i', FaceId::new(2), 4.0);
    matrix.begin_window(1, 1, 20, Rect::new(0.0, 0.0, 160.0, 16.0), true);
    matrix.begin_row(0, GlyphRowRole::Text);
    let mut source = LispStringSourceCursor::new(
        1,
        Value::string("mi"),
        RenderFaceRef::FaceId(FaceId::new(2)),
        LispStringSourceOrigin::Normal,
    )
    .expect("source");
    let mut context = DisplaySourceContext::empty();

    let mut cursor = DisplayRowAppendCursor::new(DisplayRowPosition::new(0.0, 0), 80.0);
    let item = source.next_item(&mut context).expect("source item");
    let progress = cursor
        .append_measured_item_to_current_text_row(&mut matrix, &row_layout, item, &mut measurer)
        .expect("append progress");

    assert_eq!(progress.end(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(cursor.position(), DisplayRowPosition::new(16.0, 2));
    matrix
        .edit_current_row_for_test(|row| {
            let glyphs = &row.glyphs[GlyphArea::Text.index()];
            assert_eq!(glyphs[0].pixel_width, 12.0);
            assert_eq!(glyphs[1].pixel_width, 4.0);
        })
        .expect("current row");
}

#[test]
fn fixed_glyph_advance_matches_only_configured_glyph() {
    let mut measurer = FixedGlyphAdvance::new('m', FaceId::new(7), 13.0);

    assert_eq!(
        measurer.glyph_advance_px('m', FaceId::new(7), 1, 8.0),
        Some(13.0)
    );
    assert_eq!(measurer.glyph_advance_px('m', FaceId::new(8), 1, 8.0), None);
    assert_eq!(measurer.glyph_advance_px('i', FaceId::new(7), 1, 8.0), None);
}

#[test]
fn fixed_glyph_advances_return_inserted_widths() {
    let mut measurer = FixedGlyphAdvances::new();
    measurer.insert('m', FaceId::new(7), 13.0);
    measurer.insert('i', FaceId::new(7), 4.0);

    assert_eq!(
        measurer.glyph_advance_px('m', FaceId::new(7), 1, 8.0),
        Some(13.0)
    );
    assert_eq!(
        measurer.glyph_advance_px('i', FaceId::new(7), 1, 8.0),
        Some(4.0)
    );
    assert_eq!(measurer.glyph_advance_px('m', FaceId::new(8), 1, 8.0), None);
}

#[test]
fn display_text_run_measurement_exposes_measured_advances() {
    let advances = vec![
        DisplayTextRunAdvance::new(0, 0, 8.0),
        DisplayTextRunAdvance::new(1, 1, 9.0),
    ];
    let measured = DisplayTextRunMeasurement::Measured(advances.clone());

    assert_eq!(measured.measured_advances(), Some(advances.as_slice()));
    assert_eq!(DisplayTextRunMeasurement::PerChar.measured_advances(), None);
}

#[test]
fn display_text_run_measurement_builds_uniform_advances_for_text() {
    let measurement = DisplayTextRunMeasurementPlan::uniform_for_text("aé中", 5.0);

    let advances = measurement
        .measured_advances()
        .expect("uniform measurement should produce advances");
    assert_eq!(
        advances
            .iter()
            .map(|advance| (advance.char_offset, advance.byte_offset, advance.advance_px))
            .collect::<Vec<_>>(),
        vec![(0, 0, 5.0), (1, 1, 5.0), (2, 3, 5.0)]
    );
}

#[test]
fn display_text_run_measurement_maps_base_char_byte_advances() {
    let measurement = DisplayTextRunMeasurement::Measured(vec![
        DisplayTextRunAdvance::new(0, 0, 7.0),
        DisplayTextRunAdvance::new(1, 1, 0.0),
        DisplayTextRunAdvance::new(2, 3, 9.0),
    ]);

    assert_eq!(
        measurement.base_char_byte_advances("a\u{301}中", 100),
        vec![
            DisplayTextRunByteAdvance::new(100, 7.0),
            DisplayTextRunByteAdvance::new(103, 9.0),
        ]
    );
    assert_eq!(
        DisplayTextRunMeasurement::PerChar.base_char_byte_advances("a\u{301}", 100),
        Vec::<DisplayTextRunByteAdvance>::new()
    );
}

#[test]
fn display_row_builder_renders_source_mapped_text_with_one_source_charpos() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(mapped_text_item("\\ "));

    let row = builder.finish();
    let glyphs = &row.glyphs[GlyphArea::Text.index()];

    assert_eq!(row_text(&row), "\\ ");
    assert_eq!(glyphs.len(), 2);
    assert!(glyphs.iter().all(|glyph| glyph.legacy_charpos() == 0));
}

#[test]
fn display_row_progress_writer_reports_source_mapped_text_slots_with_same_source() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(8.0, 1), 80.0);

    let progress = writer.push_item(mapped_text_item("\\-"));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(progress.end(), DisplayRowPosition::new(24.0, 3));
    assert_eq!(progress.slots().len(), 2);
    assert!(
        progress
            .slots
            .iter()
            .all(|slot| slot.source() == DisplaySourcePosition::lisp_string(1, 0, 0))
    );
    assert_eq!(progress.slots()[0].width_px(), 8.0);
    assert_eq!(progress.slots()[0].width_cols(), 1);
    assert_eq!(progress.slots()[1].width_px(), 8.0);
    assert_eq!(progress.slots()[1].width_cols(), 1);
    assert_eq!(row_text(&row), "\\-");
}

#[test]
fn display_row_builder_emits_ascii_text_items() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(text_item("abc"));

    let row = builder.finish();

    assert_eq!(row.role, GlyphRowRole::Text);
    assert_eq!(row_text(&row), "abc");
    assert_eq!(row.glyphs[GlyphArea::Text.index()].len(), 3);
    assert!(
        row.glyphs[GlyphArea::Text.index()]
            .iter()
            .all(|glyph| glyph.face_id == FaceId::new(2))
    );
}

#[test]
fn display_row_builder_emits_space_width_as_primary_face_stretch() {
    struct SymbolFaceMeasurer;

    impl DisplayGlyphMeasurer for SymbolFaceMeasurer {
        fn glyph_advance_px(
            &mut self,
            ch: char,
            _face_id: FaceId,
            _columns: u8,
            _fallback_advance_px: f32,
        ) -> Option<f32> {
            match ch {
                '\u{e6ad}' => Some(10.0),
                // A concrete ASCII space falls back to a taller text font.
                ' ' => Some(9.0),
                _ => None,
            }
        }

        fn glyph_vertical_metrics_px(
            &mut self,
            ch: char,
            _face_id: FaceId,
        ) -> Option<DisplayRowVerticalMetrics> {
            match ch {
                '\u{e6ad}' => Some(DisplayRowVerticalMetrics::new(10.0, 8.0)),
                ' ' => Some(DisplayRowVerticalMetrics::new(14.0, 11.0)),
                _ => None,
            }
        }

        fn face_vertical_metrics_px(
            &mut self,
            _face_id: FaceId,
        ) -> Option<DisplayRowVerticalMetrics> {
            Some(DisplayRowVerticalMetrics::new(10.0, 8.0))
        }

        fn face_space_width_px(&mut self, _face_id: FaceId) -> Option<f32> {
            Some(10.0)
        }
    }

    let mut row_layout = layout();
    row_layout.height_px = 12.0;
    row_layout.ascent_px = 9.0;
    let mut measurer = SymbolFaceMeasurer;
    let mut builder = DisplayRowBuilder::with_glyph_measurer(row_layout, &mut measurer);
    builder.push_item(text_item("\u{e6ad} ").with_layout(DisplayItemLayout {
        raise: Some(0.15),
        height: None,
        space_width: Some(0.4),
        break_after_row: false,
    }));

    let row = builder.finish();
    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(glyphs[0].pixel_width, 10.0);
    assert_eq!(glyphs[1].glyph_type, GlyphType::Stretch { width_cols: 1 });
    assert_eq!(glyphs[1].pixel_width, 4.0);
    assert_eq!(glyphs[1].pixel_height, 10.0);
    assert_eq!(glyphs[1].pixel_ascent, 8.0);
    assert_eq!(glyphs[0].vertical_offset_px, -1.0);
    assert_eq!(glyphs[1].vertical_offset_px, -1.0);
    assert_eq!(row.height_px, 12.0);
    assert_eq!(row.ascent_px, 9.0);
}

#[test]
fn graphical_relative_stretch_measures_the_covered_character() {
    let face_id = FaceId::new(2);
    let mut measurer = FixedGlyphAdvance::new('W', face_id, 15.0);
    let mut builder = DisplayRowBuilder::with_glyph_measurer(layout(), &mut measurer);
    builder.push_item(DisplayItem::new(
        SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::FaceId(face_id),
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::RelativeToSource {
                factor: 0.5,
                source: EmacsChar::from_char('W'),
            },
            height: None,
            ascent: None,
        }),
    ));

    let row = builder.finish();
    let glyph = &row.glyphs[GlyphArea::Text.index()][0];
    assert_eq!(glyph.pixel_width, 7.0);
    assert_eq!(glyph.glyph_type, GlyphType::Stretch { width_cols: 1 });
}

#[test]
fn display_row_builder_consumes_display_item_source() {
    let _eval = Context::new();
    let mut source = LispStringSourceCursor::new(
        1,
        Value::string("abc"),
        RenderFaceRef::FaceId(FaceId::new(2)),
        LispStringSourceOrigin::Normal,
    )
    .expect("source");
    let mut context = DisplaySourceContext::empty();
    let mut builder = DisplayRowBuilder::new(layout());

    builder.push_source(&mut source, &mut context);

    let row = builder.finish();
    assert_eq!(row_text(&row), "abc");
}

#[test]
fn display_row_builder_emits_tab_as_stretch_to_next_tab_stop() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(text_item("a\tb"));

    let row = builder.finish();
    let glyphs = &row.glyphs[GlyphArea::Text.index()];

    assert_eq!(row_text(&row), "a   b");
    assert_eq!(glyphs[1].glyph_type, GlyphType::Stretch { width_cols: 3 });
}

#[test]
fn display_row_builder_tabs_use_realized_face_space_width_after_nerd_icon() {
    struct NerdIconMeasurer;

    impl DisplayGlyphMeasurer for NerdIconMeasurer {
        fn glyph_advance_px(
            &mut self,
            ch: char,
            _face_id: FaceId,
            _columns: u8,
            _fallback_advance_px: f32,
        ) -> Option<f32> {
            match ch {
                '\u{e6ad}' => Some(9.0),
                // Character fallback may report a different U+0020 width.
                // TAB geometry must not use it.
                ' ' => Some(19.0),
                'n' => Some(5.0),
                _ => None,
            }
        }

        fn face_space_width_px(&mut self, _face_id: FaceId) -> Option<f32> {
            Some(5.0)
        }
    }

    let mut row_layout = layout();
    row_layout.tab_policy = DisplayTabPolicy::every(1);
    let mut measurer = NerdIconMeasurer;
    let mut builder = DisplayRowBuilder::with_glyph_measurer(row_layout, &mut measurer);
    builder.push_item(text_item("\u{e6ad}\tn"));

    let row = builder.finish();
    let glyphs = &row.glyphs[GlyphArea::Text.index()];

    assert_eq!(glyphs[0].pixel_width, 9.0);
    assert_eq!(glyphs[1].glyph_type, GlyphType::Stretch { width_cols: 2 });
    assert_eq!(
        glyphs[1].pixel_width, 6.0,
        "GNU xdisp computes TAB stops and the minimum-space threshold from the TAB face font's space_width; after a 9px icon with 5px spaces, tab-width 1 advances to x=15"
    );
    assert_eq!(glyphs[2].pixel_width, 5.0);
}

#[test]
fn tab_advance_targets_gnu_pixel_grid_without_rounding_the_renderer_pen() {
    let position = DisplayRowPosition::new(12.150_001, 1);
    let advance = DisplayTabPolicy::every(1).advance_from(position, 12.15);

    assert!(
        (position.x_px + advance.pixel_width - 24.0).abs() < 0.001,
        "GNU rounds the 12.15px pen and space advance to integer pixel 12, then targets pixel 24"
    );
    assert_eq!(advance.width_cols, 1);
}

#[test]
fn tab_advance_has_typed_screen_and_continued_physical_line_coordinates() {
    let policy = DisplayTabPolicy::every(8);
    let mut physical_line = DisplayPhysicalLineTabState::default();
    physical_line.continue_after_visual_row(624.0);

    let screen_position = DisplayRowPosition::new(176.0, 22);
    let continued_position = screen_position.with_tab_coordinates(physical_line.coordinates());

    assert_eq!(policy.advance_from(screen_position, 8.0).pixel_width, 16.0);
    assert_eq!(
        policy.advance_from(continued_position, 8.0).pixel_width,
        32.0,
        "physical column 100 advances to GNU's column-104 tab stop"
    );

    // A wrap prefix is measured on the screen-line grid, then excluded from
    // the ordinary buffer text's physical-line coordinate.
    physical_line.record_wrap_prefix(16.0);
    let after_prefix =
        DisplayRowPosition::new(192.0, 24).with_tab_coordinates(physical_line.coordinates());
    assert_eq!(policy.advance_from(after_prefix, 8.0).pixel_width, 32.0);
    assert_eq!(
        policy
            .advance_from(after_prefix.on_screen_line_tab_grid(), 8.0)
            .pixel_width,
        64.0
    );

    // GNU keeps the signed `continuation_lines_width - wrap_prefix_width`
    // term even when a wide prefix exceeds the preceding visual row. Losing
    // that negative offset would silently fall back to the screen-line grid.
    let mut narrow_physical_line = DisplayPhysicalLineTabState::default();
    narrow_physical_line.continue_after_visual_row(24.0);
    narrow_physical_line.record_wrap_prefix(32.0);
    let after_wide_prefix =
        DisplayRowPosition::new(32.0, 4).with_tab_coordinates(narrow_physical_line.coordinates());
    assert_eq!(
        policy.advance_from(after_wide_prefix, 8.0).pixel_width,
        40.0,
        "physical pixel 24 advances to 64 despite the wider wrap prefix"
    );

    physical_line.reset_for_physical_line();
    assert_eq!(
        policy
            .advance_from(
                screen_position.with_tab_coordinates(physical_line.coordinates()),
                8.0,
            )
            .pixel_width,
        16.0
    );
}

#[test]
fn row_tail_reconciliation_preserves_the_walks_tab_coordinate_space() {
    let mut physical_line = DisplayPhysicalLineTabState::default();
    physical_line.continue_after_visual_row(624.0);
    let requested =
        DisplayRowPosition::new(8.0, 1).with_tab_coordinates(physical_line.coordinates());
    let row_tail = DisplayRowPosition::new(24.0, 3);

    assert_eq!(
        append_start_position(requested, row_tail),
        requested.at_screen_position(24.0, 3),
        "a structural row prefix may move the pen but must not erase continuation_lines_width"
    );
}

#[test]
fn tab_advance_keeps_dired_filenames_on_one_stop_across_subpixel_pen_drift() {
    // Captured from two adjacent nerd-icons-dired rows.  GNU xdisp stores
    // `current_x' and `font->space_width' as integer pixels, so both source
    // positions are pixel 336 relative to the text origin and advance to the
    // same tab stop at absolute x=510.
    let policy = DisplayTabPolicy::from_tab_width_and_stops(167.0, 1, &[]);
    let main_position = DisplayRowPosition::new(503.400_3, 47);
    let main_test_position = DisplayRowPosition::new(503.000_27, 47);

    let main = policy.advance_from(main_position, 7.0);
    let main_test = policy.advance_from(main_test_position, 7.0);
    let main_filename_x = main_position.x_px + main.pixel_width;
    let main_test_filename_x = main_test_position.x_px + main_test.pixel_width;

    assert!(
        (main_filename_x - 510.0).abs() < 0.01,
        "main.rs must land on GNU's integer-pixel tab stop; got {main_filename_x}"
    );
    assert!(
        (main_test_filename_x - 510.0).abs() < 0.01,
        "main_test.rs must land on GNU's integer-pixel tab stop; got {main_test_filename_x}"
    );
    assert_eq!(main.width_cols, main_test.width_cols);
}

#[test]
fn display_row_writer_appends_items_to_existing_row_tab_context() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    crate::glyph_row_writer::push_char_to_row(&mut row, 'x', FaceId::new(2), 0, 8.0);

    let row_layout = layout();
    let mut writer = DisplayRowWriter::new(&row_layout, &mut row);
    writer.push_item(text_item("a\tb"));

    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(row_text(&row), "xa  b");
    assert_eq!(glyphs[2].glyph_type, GlyphType::Stretch { width_cols: 2 });
}

#[test]
fn display_row_writer_can_append_items_to_left_margin_area() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer = DisplayRowWriter::for_area(&row_layout, &mut row, GlyphArea::LeftMargin);

    let stretch_metrics = writer.push_item(stretch_item(DisplayLength::Pixels(8.0)));
    let text_metrics = writer.push_item(text_item("12"));

    let margin = &row.glyphs[GlyphArea::LeftMargin.index()];
    assert!(row.glyphs[GlyphArea::Text.index()].is_empty());
    assert!(!row.displays_text);
    assert_eq!(stretch_metrics.width_cols(), 1);
    assert_eq!(text_metrics.width_cols(), 2);
    assert_eq!(margin.len(), 3);
    assert_eq!(margin[0].glyph_type, GlyphType::Stretch { width_cols: 1 });
    assert_eq!(margin[1].glyph_type, GlyphType::Char { ch: '1' });
    assert_eq!(margin[2].glyph_type, GlyphType::Char { ch: '2' });
}

#[test]
fn display_row_writer_consumes_display_item_source() {
    let _eval = Context::new();
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    crate::glyph_row_writer::push_char_to_row(&mut row, 'x', FaceId::new(2), 0, 8.0);
    let mut source = LispStringSourceCursor::new(
        1,
        Value::string("a\tb"),
        RenderFaceRef::FaceId(FaceId::new(2)),
        LispStringSourceOrigin::Normal,
    )
    .expect("source");
    let mut context = DisplaySourceContext::empty();

    let row_layout = layout();
    let mut writer = DisplayRowWriter::new(&row_layout, &mut row);
    writer.push_source(&mut source, &mut context);

    assert_eq!(row_text(&row), "xa  b");
}

#[test]
fn display_row_writer_reports_appended_metrics() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer = DisplayRowWriter::new(&row_layout, &mut row);

    let metrics = writer.push_item(text_item("a\tb"));

    assert_eq!(metrics.width_cols(), 5);
    assert_eq!(metrics.width_px(), 40.0);
}

#[test]
fn display_row_progress_writer_stops_text_before_right_limit() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 20.0);

    let progress = writer.push_item(text_item("abcd"));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Clipped);
    assert_eq!(progress.start(), DisplayRowPosition::new(0.0, 0));
    assert_eq!(progress.end(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(writer.position(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(row_text(&row), "ab");
}

#[test]
fn display_row_progress_writer_reports_source_slots_for_text_run() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(4.0, 2), 80.0);

    let progress = writer.push_item(text_item("aé"));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(progress.slots().len(), 2);
    assert_eq!(
        progress.slots()[0].source(),
        DisplaySourcePosition::lisp_string(1, 0, 0)
    );
    assert_eq!(progress.slots()[0].x_px(), 4.0);
    assert_eq!(progress.slots()[0].col(), 2);
    assert_eq!(progress.slots()[0].width_px(), 8.0);
    assert_eq!(
        progress.slots()[1].source(),
        DisplaySourcePosition::lisp_string(1, 1, 1)
    );
    assert_eq!(progress.slots()[1].x_px(), 12.0);
    assert_eq!(progress.slots()[1].col(), 3);
    assert_eq!(progress.slots()[1].width_px(), 8.0);
}

#[test]
fn display_row_progress_writer_uses_text_run_measurement_plan() {
    struct RunOnlyMeasurer;

    impl DisplayGlyphMeasurer for RunOnlyMeasurer {
        fn glyph_advance_px(
            &mut self,
            _ch: char,
            _face_id: FaceId,
            _columns: u8,
            _fallback_advance_px: f32,
        ) -> Option<f32> {
            panic!("text run should use the run measurement plan");
        }

        fn text_run_advances_px(
            &mut self,
            text: &str,
            face_id: FaceId,
            _fallback_char_width_px: f32,
        ) -> DisplayTextRunMeasurement {
            assert_eq!(text, "abc");
            assert_eq!(face_id, FaceId::new(2));
            DisplayTextRunMeasurement::Measured(vec![
                DisplayTextRunAdvance::new(0, 0, 4.0),
                DisplayTextRunAdvance::new(1, 1, 20.0),
                DisplayTextRunAdvance::new(2, 2, 6.0),
            ])
        }
    }

    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut measurer = RunOnlyMeasurer;
    let mut writer = DisplayRowProgressWriter::with_glyph_measurer(
        &row_layout,
        &mut row,
        &mut measurer,
        DisplayRowPosition::new(0.0, 0),
        80.0,
    );

    let progress = writer.push_item(text_item("abc"));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(progress.end(), DisplayRowPosition::new(30.0, 3));
    assert_eq!(
        progress
            .slots
            .iter()
            .map(|slot| slot.width_px())
            .collect::<Vec<_>>(),
        vec![4.0, 20.0, 6.0]
    );
    assert_eq!(
        row.glyphs[GlyphArea::Text.index()]
            .iter()
            .map(|glyph| glyph.pixel_width)
            .collect::<Vec<_>>(),
        vec![4.0, 20.0, 6.0]
    );
}

#[test]
fn display_row_progress_writer_accepts_direct_text_run_measurement_plan() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let measurement = DisplayTextRunMeasurement::Measured(vec![
        DisplayTextRunAdvance::new(0, 0, 4.0),
        DisplayTextRunAdvance::new(1, 1, 20.0),
        DisplayTextRunAdvance::new(2, 2, 6.0),
    ]);
    let mut writer = DisplayRowProgressWriter::with_text_run_measurement(
        &row_layout,
        &mut row,
        measurement,
        DisplayRowPosition::new(0.0, 0),
        80.0,
    );

    let progress = writer.push_item(text_item("abc"));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(progress.end(), DisplayRowPosition::new(30.0, 3));
    assert_eq!(
        row.glyphs[GlyphArea::Text.index()]
            .iter()
            .map(|glyph| glyph.pixel_width)
            .collect::<Vec<_>>(),
        vec![4.0, 20.0, 6.0]
    );
}

#[test]
fn display_row_progress_writer_clips_margin_glyph_to_structural_lane() {
    // A GUI font can report a fractional advance (7.2 px) while Emacs exposes
    // the corresponding one-column margin as an integer 7 px.  Structural
    // content must remain visible inside that authoritative lane; dropping the
    // whole glyph makes git-gutter markers disappear.
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let measurement =
        DisplayTextRunMeasurement::Measured(vec![DisplayTextRunAdvance::new(0, 0, 7.2)]);
    let mut writer = DisplayRowProgressWriter::with_text_run_measurement_for_area(
        &row_layout,
        &mut row,
        measurement,
        DisplayRowPosition::new(0.0, 0),
        7.0,
        GlyphArea::LeftMargin,
    );

    let progress = writer.push_item(text_item("+"));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Clipped);
    assert_eq!(progress.end(), DisplayRowPosition::new(7.0, 1));
    let margin = &row.glyphs[GlyphArea::LeftMargin.index()];
    assert_eq!(
        margin.len(),
        1,
        "the partially fitting marker stays visible"
    );
    assert_eq!(margin[0].glyph_type, GlyphType::Char { ch: '+' });
    assert_eq!(margin[0].pixel_width, 7.0);
    assert!(row.glyphs[GlyphArea::Text.index()].is_empty());
}

/// A composed Arabic cluster already on the row must report its full
/// `string-width` worth of columns when a *new* writer (e.g. the TAB item in
/// the multi-item buffer walk) resumes the row. GNU advances the column by the
/// composition's `cmp->width` (= string-width); counting the whole cluster as
/// a single cell made the resumed TAB over-fill (the etc/HELLO Arabic/Indic
/// regression, where the name renders identically but the greeting is pushed
/// ~6 cells right).
///
/// This reconstructs the exact glyph shape the buffer-text walk produces for a
/// contextual-shaping run: one `Composite` holding the whole cluster plus
/// zero-width per-letter padding cells (the GUI shapes the run; the TTY lays
/// it one grapheme per cell). `current_text_cols` must therefore count the
/// Composite as `string-width("العربيّة")` = 7, not 1.
#[test]
fn resumed_writer_counts_composed_cluster_by_string_width() {
    use neomacs_display_protocol::glyph_matrix::Glyph;

    let mut row_layout = layout();
    row_layout.tab_policy = DisplayTabPolicy::every(42);
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;

    // `Arabic (` — eight plain Char glyphs (cols 0..8).
    for (i, ch) in "Arabic (".chars().enumerate() {
        crate::glyph_row_writer::push_char_to_row(&mut row, ch, FaceId::new(2), i, 8.0);
    }
    // The Arabic word as one Composite cluster (string-width 7: six letters
    // plus the teh-marbuta; the shadda U+0651 is zero-width) followed by
    // zero-width grapheme padding cells, matching the buffer walk's output.
    {
        let text = &mut row.glyphs[GlyphArea::Text.index()];
        text.push(Glyph {
            glyph_type: GlyphType::Composite {
                text: "العربيّة".into(),
            },
            ..Glyph::char('ا', FaceId::new(2), 8)
        });
        for charpos in 9..15 {
            text.push(Glyph::padding_for(FaceId::new(2), charpos));
        }
    }
    // The closing paren — one more Char glyph.
    crate::glyph_row_writer::push_char_to_row(&mut row, ')', FaceId::new(2), 15, 8.0);

    // A fresh writer (the TAB item) must recover col 16 from the row glyphs:
    // 8 ("Arabic (") + 7 (composed cluster) + 1 (")").
    let tab = {
        let mut writer = DisplayRowProgressWriter::new(
            &row_layout,
            &mut row,
            DisplayRowPosition::new(0.0, 0),
            1280.0,
        );
        writer.push_item(text_item("\t"))
    };

    assert_eq!(
        tab.start.col, 16,
        "resume must recover the buffer column (16) from the composed cluster; got {}",
        tab.start.col
    );
    assert_eq!(
        tab.end.col, 42,
        "resumed TAB must fill to the buffer tab stop (col 42); got {}",
        tab.end.col
    );
}

/// Same etc/HELLO contextual-shaping scenario as
/// `resumed_writer_counts_composed_cluster_by_string_width`, but built with the
/// *live* glyph shape the buffer walk actually emits today. The refactored
/// `push_run_member_to_area` no longer pushes zero-width grapheme padding: each
/// run member is a `padding` `GlyphType::Char { ch }` cell carrying a POSITIVE
/// `pixel_width` (so the GUI x-advance and per-cell cursor work). The base
/// `Composite` already carries the whole run's width via
/// `composed_cluster_cols(text)` (= GNU's `cmp->width`, set once in
/// `produce_composite_glyph`, src/term.c:1859), so those padding cells must
/// count 0 cols / 0 px or the run is double-counted: a 7-cell Arabic word would
/// report 7 + 6 = 13 cols, putting the resumed column at 22 and over-filling the
/// following TAB (the etc/HELLO Arabic/Bengali/etc. left-shift regression).
#[test]
fn resumed_writer_counts_live_run_member_padding_once() {
    let mut row_layout = layout();
    row_layout.tab_policy = DisplayTabPolicy::every(42);
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;

    // `Arabic (` — eight plain Char glyphs (cols 0..8).
    for (i, ch) in "Arabic (".chars().enumerate() {
        crate::glyph_row_writer::push_char_to_row(&mut row, ch, FaceId::new(2), i, 8.0);
    }
    // The Arabic word built EXACTLY as the buffer walk does: the first letter is
    // a plain Char, then each subsequent letter is pushed via
    // `push_run_member_to_row`, which grows the base into one `Composite`
    // ("العربيّة") and appends a positive-`pixel_width` `Char` padding cell per
    // member. string-width is 7 (six letters + teh-marbuta; the shadda U+0651 is
    // a zero-width combining mark folded into the cluster, not its own member).
    let word = "العربيّة";
    let mut chars = word.chars();
    let first = chars.next().expect("non-empty word");
    crate::glyph_row_writer::push_char_to_row(&mut row, first, FaceId::new(2), 8, 8.0);
    for (offset, ch) in chars.enumerate() {
        crate::glyph_row_writer::push_run_member_to_row(
            &mut row,
            ch,
            FaceId::new(2),
            9 + offset,
            8.0,
        );
    }
    // Sanity: exactly what the live writer produces — one non-padding Composite
    // base plus per-member padding cells with positive pixel widths.
    {
        let text = &row.glyphs[GlyphArea::Text.index()];
        let base = &text[8];
        assert!(!base.padding, "run base must not be padding");
        assert_eq!(
            base.glyph_type,
            GlyphType::Composite { text: word.into() },
            "run base must hold the whole composed cluster"
        );
        assert!(
            text[9..].iter().all(|g| g.padding && g.pixel_width > 0.0),
            "live run-member padding cells carry POSITIVE pixel_width: {:?}",
            text[9..].iter().map(|g| g.pixel_width).collect::<Vec<_>>()
        );
    }
    // The closing paren — one more Char glyph.
    let close_pos = 9 + word.chars().count() - 1;
    crate::glyph_row_writer::push_char_to_row(&mut row, ')', FaceId::new(2), close_pos, 8.0);

    // A fresh writer (the TAB item) must recover col 16 from the row glyphs:
    // 8 ("Arabic (") + 7 (composed cluster, counted ONCE) + 1 (")"). If the
    // run-member padding cells are double-counted it lands at 22 and the TAB
    // over-fills.
    let tab = {
        let mut writer = DisplayRowProgressWriter::new(
            &row_layout,
            &mut row,
            DisplayRowPosition::new(0.0, 0),
            1280.0,
        );
        writer.push_item(text_item("\t"))
    };

    assert_eq!(
        tab.start.col, 16,
        "resume must count the composed cluster once (16), not per run member; got {}",
        tab.start.col
    );
    assert_eq!(
        tab.end.col, 42,
        "resumed TAB must fill to the buffer tab stop (col 42); got {}",
        tab.end.col
    );
}

/// The buffer walk owns its requested position because structural text-area
/// prefixes (line numbers) are already included in that coordinate.  Extending
/// a contextual run mutates the earlier Composite glyph instead of adding a
/// normal-width glyph at the tail, so its per-item progress must still advance
/// by the Composite's growth.  Otherwise the following HELLO-file TAB sees
/// column 10 instead of 16 and emits 32 spaces instead of GNU's 26.
#[test]
fn source_position_progress_counts_complex_run_growth_before_tab() {
    let mut row_layout = layout();
    row_layout.tab_policy = DisplayTabPolicy::every(42);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    let mut glyph_advances = FixedGlyphAdvances::new();
    let mut writer =
        DisplayRowProgressWriter::with_text_run_measurement_and_glyph_measurer_for_area_and_start_policy(
            &row_layout,
            &mut row,
            DisplayTextRunMeasurement::PerChar,
            &mut glyph_advances,
            DisplayRowPosition::new(0.0, 0),
            1280.0,
            crate::display_row::geometry::DisplayRowTextAreaOrigin::row_local(),
            GlyphArea::Text,
            DisplayRowAppendStartPolicy::SourcePosition,
        );

    let progress = writer.push_item(text_item("Arabic (العربيّة)\tx"));

    assert_eq!(
        progress.end().col,
        43,
        "GNU lands the TAB at column 42, then advances once for `x`"
    );
    let stretch_width = row.glyphs[GlyphArea::Text.index()]
        .iter()
        .find_map(|glyph| match glyph.glyph_type {
            GlyphType::Stretch { width_cols } => Some(width_cols),
            _ => None,
        });
    assert_eq!(stretch_width, Some(26));
}

#[test]
fn display_row_progress_writer_uses_position_for_tabs() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer = DisplayRowProgressWriter::new(
        &row_layout,
        &mut row,
        DisplayRowPosition::new(16.0, 2),
        80.0,
    );

    let progress = writer.push_item(text_item("\tb"));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(progress.end(), DisplayRowPosition::new(40.0, 5));
    assert_eq!(
        row.glyphs[GlyphArea::Text.index()][0].glyph_type,
        GlyphType::Stretch { width_cols: 2 }
    );
}

#[test]
fn display_row_progress_writer_uses_tab_policy_origin_for_pixel_tabs() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let mut row_layout = layout();
    row_layout.tab_policy = DisplayTabPolicy::from_tab_width_and_stops(96.0, 8, &[]);
    let mut writer = DisplayRowProgressWriter::new(
        &row_layout,
        &mut row,
        DisplayRowPosition::new(96.0 + 24.0, 3),
        240.0,
    );

    let progress = writer.push_item(text_item("\tb"));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(
        row.glyphs[GlyphArea::Text.index()][0].glyph_type,
        GlyphType::Stretch { width_cols: 5 }
    );
    assert_eq!(progress.slots()[0].x_px(), 120.0);
    assert_eq!(progress.slots()[0].width_px(), 40.0);
    assert_eq!(progress.end(), DisplayRowPosition::new(168.0, 9));
}

#[test]
fn display_row_progress_writer_uses_tab_policy_explicit_stops() {
    let mut row = neomacs_display_protocol::glyph_matrix::GlyphRow::new(GlyphRowRole::Text);
    let mut row_layout = layout();
    row_layout.tab_policy = DisplayTabPolicy::from_tab_width_and_stops(100.0, 8, &[4, 10]);
    let mut writer = DisplayRowProgressWriter::new(
        &row_layout,
        &mut row,
        DisplayRowPosition::new(100.0 + 24.0, 3),
        240.0,
    );

    let progress = writer.push_item(text_item("\tb"));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    assert_eq!(
        row.glyphs[GlyphArea::Text.index()][0].glyph_type,
        GlyphType::Stretch { width_cols: 1 }
    );
    assert_eq!(progress.slots()[0].x_px(), 124.0);
    assert_eq!(progress.slots()[0].width_px(), 8.0);
    assert_eq!(progress.end(), DisplayRowPosition::new(140.0, 5));
}

#[test]
fn display_row_builder_uses_glyph_measurer_for_text_pixel_widths() {
    struct TestMeasurer;

    impl DisplayGlyphMeasurer for TestMeasurer {
        fn glyph_advance_px(
            &mut self,
            ch: char,
            _face_id: FaceId,
            _columns: u8,
            _fallback_advance_px: f32,
        ) -> Option<f32> {
            match ch {
                'm' => Some(12.0),
                'i' => Some(4.0),
                _ => None,
            }
        }
    }

    let mut measurer = TestMeasurer;
    let mut builder = DisplayRowBuilder::with_glyph_measurer(layout(), &mut measurer);
    builder.push_item(text_item("mi"));

    let row = builder.finish();
    let glyphs = &row.glyphs[GlyphArea::Text.index()];

    assert_eq!(glyphs[0].pixel_width, 12.0);
    assert_eq!(glyphs[1].pixel_width, 4.0);
}

#[test]
fn display_row_builder_push_measured_item_accepts_per_call_measurer() {
    struct TestMeasurer;

    impl DisplayGlyphMeasurer for TestMeasurer {
        fn glyph_advance_px(
            &mut self,
            ch: char,
            _face_id: FaceId,
            _columns: u8,
            _fallback_advance_px: f32,
        ) -> Option<f32> {
            (ch == '中').then_some(24.0)
        }
    }

    let mut builder = DisplayRowBuilder::new(layout());
    let mut measurer = TestMeasurer;

    builder.push_measured_item(text_item("A中"), &mut measurer);
    let row = builder.finish();
    let cjk = row.glyphs[GlyphArea::Text.index()]
        .iter()
        .find(|glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }))
        .expect("CJK glyph");

    assert_eq!(row_text(&row), "A中");
    assert_eq!(cjk.pixel_width, 24.0);
}

#[test]
fn display_row_builder_emits_cjk_wide_char_with_padding() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(text_item("A中B"));

    let row = builder.finish();
    let glyphs = &row.glyphs[GlyphArea::Text.index()];

    assert_eq!(row_text(&row), "A中B");
    assert!(
        glyphs.iter().any(|glyph| {
            matches!(glyph.glyph_type, GlyphType::Char { ch: '中' }) && glyph.wide
        })
    );
    assert!(glyphs.iter().any(|glyph| glyph.padding));
}

#[test]
fn display_row_builder_composes_emoji_zwj_cluster() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(text_item("👨‍👩"));

    let row = builder.finish();
    let glyphs = &row.glyphs[GlyphArea::Text.index()];

    assert!(glyphs
        .iter()
        .any(|glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.contains('\u{200d}'))));
}

#[test]
fn display_row_builder_composes_combining_mark_cluster() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(text_item("e\u{301}"));

    let row = builder.finish();

    assert!(row.glyphs[GlyphArea::Text.index()]
        .iter()
        .any(|glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.as_ref() == "e\u{301}")));
}

#[test]
fn display_row_builder_groups_arabic_complex_run() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(text_item("سلام"));

    let row = builder.finish();
    let glyphs = &row.glyphs[GlyphArea::Text.index()];

    assert!(glyphs.iter().any(
        |glyph| matches!(&glyph.glyph_type, GlyphType::Composite { text } if text.as_ref() == "سلام")
    ));
    assert!(glyphs.iter().filter(|glyph| glyph.padding).count() >= 3);
}

#[test]
fn display_row_builder_produces_logical_order_rows() {
    // Slice 5: DisplayRowBuilder::finish normalizes displays_text but no longer
    // DisplayRowBuilder only normalizes `displays_text`; complete typed row
    // rendering owns standalone row bidi finalization.
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(text_item("אב"));

    let row = builder.finish();

    assert!(!row.reversed_p);
    assert_eq!(row_text(&row), "אב");
}

#[test]
fn display_row_builder_emits_stretch_with_pixel_width() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(DisplayItem::new(
        SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::FaceId(FaceId::new(4)),
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(24.0)),
            height: Some(DisplayLength::Pixels(16.0)),
            ascent: Some(DisplayLength::Pixels(12.0)),
        }),
    ));

    let row = builder.finish();
    let glyph = &row.glyphs[GlyphArea::Text.index()][0];

    assert_eq!(glyph.glyph_type, GlyphType::Stretch { width_cols: 3 });
    assert_eq!(glyph.face_id, FaceId::new(4));
    assert_eq!(glyph.pixel_width, 24.0);
    assert_eq!(glyph.pixel_height, 16.0);
    assert_eq!(glyph.pixel_ascent, 12.0);
}

#[test]
fn display_row_builder_promotes_explicit_stretch_height_to_row_metrics() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(DisplayItem::new(
        SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::FaceId(FaceId::new(4)),
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(24.0)),
            height: Some(DisplayLength::Pixels(24.0)),
            ascent: Some(DisplayLength::Pixels(24.0)),
        }),
    ));

    let row = builder.finish();

    assert_eq!(row.height_px, 24.0);
    assert_eq!(row.ascent_px, 24.0);
}

#[test]
fn display_row_vertical_metrics_include_in_row_only_grows_extents() {
    let mut row = new_display_row(&layout());

    DisplayRowVerticalMetrics::new(10.0, 9.0).include_in_row(&mut row);
    assert_eq!(row.height_px, 16.0);
    assert_eq!(row.ascent_px, 12.0);

    DisplayRowVerticalMetrics::new(24.0, 20.0).include_in_row(&mut row);
    assert_eq!(row.height_px, 24.0);
    assert_eq!(row.ascent_px, 20.0);

    DisplayRowVerticalMetrics::new(0.0, 40.0).include_in_row(&mut row);
    assert_eq!(row.height_px, 24.0);
    assert_eq!(row.ascent_px, 20.0);
}

/// `(space :height EXPR)` and `:ascent EXPR` are vertical measurements: GNU
/// evaluates both with `width_p = false` (xdisp.c:32893 and :32914), so a bare
/// number scales by `FRAME_LINE_HEIGHT`, not `FRAME_COLUMN_WIDTH`.
///
/// The row builder evaluated every `DisplayLength::Expr` with `width_p = true`,
/// so a height expression came out scaled by the character WIDTH. The
/// buffer-text path (`DisplaySpaceHeightPolicy`/`DisplaySpaceAscentPolicy`)
/// already passed `false` — the two paths had drifted.
#[test]
fn display_row_builder_scales_height_expressions_by_line_height() {
    let mut eval = Context::new();
    // `(+ 1 1)` is an arithmetic form, so it reaches the evaluator rather than
    // the plain-number `Em` arm.
    let expr = eval
        .eval_str("(quote (+ 1 1))")
        .expect("read height expression");

    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(DisplayItem::new(
        SourceSpan::synthetic(1, 0, 1),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::Stretch(DisplayStretch {
            width: DisplayStretchWidth::Length(DisplayLength::Pixels(8.0)),
            height: Some(DisplayLength::Expr(expr)),
            ascent: None,
        }),
    ));

    let row = builder.finish();
    let glyph = &row.glyphs[GlyphArea::Text.index()][0];

    // The fixture row has char width 8 and line height 16, so the two scalings
    // are distinguishable: 2 x 16 = 32, not 2 x 8 = 16.
    assert_eq!(
        glyph.pixel_height, 32.0,
        "a `:height` expression must scale by line height, not char width"
    );
}

#[test]
fn display_row_builder_ceil_pixel_stretch_columns() {
    let mut builder = DisplayRowBuilder::new(layout());
    builder.push_item(stretch_item(DisplayLength::Pixels(9.0)));

    let row = builder.finish();
    let glyph = &row.glyphs[GlyphArea::Text.index()][0];

    assert_eq!(glyph.glyph_type, GlyphType::Stretch { width_cols: 2 });
    assert_eq!(glyph.pixel_width, 9.0);
}

// Generic source-mapped text has buffer-anchor provenance. A real Lisp-string
// replacement carries a separate string-source start and therefore takes the
// GNU string-index arm tested below.

fn covered_buffer_span(start_char: usize, end_char: usize) -> SourceSpan {
    SourceSpan::new(
        DisplaySourcePosition::buffer(
            neovm_core::buffer::BufferId(1),
            neovm_core::buffer::CharPos0::new(start_char),
            neovm_core::buffer::EmacsBytePos::new(start_char + 1),
        ),
        DisplaySourcePosition::buffer(
            neovm_core::buffer::BufferId(1),
            neovm_core::buffer::CharPos0::new(end_char),
            neovm_core::buffer::EmacsBytePos::new(end_char + 1),
        ),
    )
}

#[test]
fn covered_provenance_run_stamps_every_glyph_with_covered_start() {
    // Generic mapped text standing for covered buffer chars [5, 7): 3 glyphs
    // for 2 covered chars, every glyph maps to 5 and every slot to span start.
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);

    let progress = writer.push_item(DisplayItem::new(
        covered_buffer_span(5, 7),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("STR")),
    ));

    assert_eq!(progress.status(), DisplayRowAppendStatus::Complete);
    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(glyphs.len(), 3, "N string glyphs for M covered chars");
    for glyph in glyphs {
        assert_eq!(
            glyph.provenance,
            GlyphProvenance::buffer(5),
            "every generic mapped glyph carries the covered START charpos"
        );
    }
    assert_eq!(row_text(&row), "STR");
}

/// The typed producer vocabulary agrees with generic source-mapped append.
#[test]
fn covered_provenance_glyph_stamps_match_the_typed_vocabulary() {
    let span = covered_buffer_span(5, 7);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);

    writer.push_item(DisplayItem::new(
        span.clone(),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("STR")),
    ));

    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(glyphs.len(), 3);
    for glyph in glyphs {
        assert_eq!(
            glyph.provenance,
            GlyphProvenance::buffer(5),
            "the covered rule is what the append path stamps"
        );
    }
    assert_eq!(
        covered_text_glyph(&span.start),
        ProducedGlyphProvenance::buffer(5)
    );
}

/// The natural-text contrast, against the same append path: an ordinary
/// `TextRun` advances the stamp per character.
#[test]
fn natural_text_glyph_stamps_match_the_typed_vocabulary() {
    let span = covered_buffer_span(5, 8);
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);

    writer.push_item(DisplayItem::new(
        span.clone(),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::TextRun(DisplayTextRun::new("abc")),
    ));

    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(glyphs.len(), 3);
    for (offset, glyph) in glyphs.iter().enumerate() {
        assert_eq!(glyph.provenance, GlyphProvenance::buffer(5 + offset));
        assert_eq!(
            natural_text_glyph(&span.start, offset),
            ProducedGlyphProvenance::buffer(5 + offset)
        );
    }
}

#[test]
fn covered_provenance_run_slots_all_carry_the_span_start() {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);

    let progress = writer.push_item(DisplayItem::new(
        covered_buffer_span(5, 7),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::SourceMappedText(DisplaySourceMappedText::new("STR")),
    ));

    let expected = DisplaySourcePosition::buffer(
        neovm_core::buffer::BufferId(1),
        neovm_core::buffer::CharPos0::new(5),
        neovm_core::buffer::EmacsBytePos::new(6),
    );
    assert_eq!(progress.slots().len(), 3);
    for slot in progress.slots() {
        assert_eq!(
            slot.source(),
            expected,
            "covered-provenance slots do not advance per char"
        );
    }
}

#[test]
fn natural_text_run_stamps_glyphs_per_char_by_contrast() {
    // The contrast pin: the SAME span rendered as a TextRun advances charpos
    // per char -- the exact limitation that forced the display-string refusals
    // before this increment.
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);

    writer.push_item(DisplayItem::new(
        covered_buffer_span(5, 8),
        RenderFaceRef::FaceId(FaceId::new(2)),
        DisplayItemKind::TextRun(DisplayTextRun::new("abc")),
    ));

    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(
        glyphs.iter().map(Glyph::legacy_charpos).collect::<Vec<_>>(),
        vec![5, 6, 7]
    );
}

#[test]
fn replacement_string_session_stamps_gnu_string_indices() {
    // The producer seam the display-replacement session drives
    // (`BufferDisplayReplacementStringRequest` -> `LispStringSourceCursor`
    // wrapped in the covered rewrite): for string "STR" covering buffer
    // [2, 4), GNU stamps glyphs with indices in the STRING, not with the
    // covered buffer start.  Buffer coverage remains on the row/item track.
    let _eval = Context::new();
    let covered = crate::display_item::BufferDisplayReplacementSource::spanning(
        neovm_core::buffer::BufferId(1),
        neovm_core::buffer::CharPos0::new(2),
        neovm_core::buffer::EmacsBytePos::new(3),
        neovm_core::buffer::CharPos0::new(4),
        neovm_core::buffer::EmacsBytePos::new(5),
    );
    let mut source = crate::display_source::BufferDisplayReplacementStringRequest::new(
        7,
        Value::string("STR"),
        covered,
    )
    .into_source(FaceId::new(2))
    .expect("replacement string source");
    let mut context = DisplaySourceContext::empty();

    let item = source.next_item(&mut context).expect("covered run");
    assert!(
        source.next_item(&mut context).is_none(),
        "a plain (property-less) string yields exactly one covered run"
    );
    let DisplayItemKind::SourceMappedText(mapped) = &item.kind else {
        panic!("replacement source must yield source-mapped text");
    };
    assert_eq!(&*mapped.text, "STR");
    let glyph_string_start = mapped
        .glyph_string_start
        .as_ref()
        .expect("replacement preserves its string coordinate space")
        .clone();
    assert_eq!(
        item.span.buffer_end_charpos(),
        Some(neovm_core::buffer::CharPos0::new(4))
    );

    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let row_layout = layout();
    let mut writer =
        DisplayRowProgressWriter::new(&row_layout, &mut row, DisplayRowPosition::new(0.0, 0), 80.0);
    writer.push_item(item);
    let glyphs = &row.glyphs[GlyphArea::Text.index()];
    assert_eq!(glyphs.len(), 3);
    assert_eq!(
        glyphs.iter().map(Glyph::legacy_charpos).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    let covered_range = neomacs_display_protocol::glyph_matrix::GlyphStringBufferRange::new(2, 4);
    let ProducedGlyphProvenance::Str {
        string: expected_string,
        ..
    } = provenance_from_source_position(&glyph_string_start)
    else {
        panic!("expected producer-side Lisp-string provenance")
    };
    let mut expected_source = None;
    for (index, glyph) in glyphs.iter().enumerate() {
        let GlyphProvenance::Str {
            source,
            index: actual,
        } = glyph.provenance
        else {
            panic!("expected row-local Lisp-string provenance")
        };
        assert_eq!(actual, index);
        assert_eq!(*expected_source.get_or_insert(source), source);
    }
    let source = row
        .string_source(expected_source.expect("replacement source token"))
        .expect("replacement source metadata");
    assert_eq!(source.string(), expected_string);
    assert_eq!(source.covered_buffer_range(), Some(covered_range));
}

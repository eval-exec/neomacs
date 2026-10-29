//! Fixed-cell admission is an explicit concrete proof, not a TTY-width guess.
use super::*;
use crate::display_row::face_state::{
    DisplayRowFace, DisplayRowGlyphMeasurer, DisplayRowMeasurementMode,
};
use crate::glyph_advance::GlyphAdvanceQuantization;
use crate::neovm_bridge::ResolvedFace;
use std::cell::Cell;

fn face_with_width(width: f32) -> DisplayRowFace {
    let mut face = DisplayRowFace::from_resolved(FaceId::new(2), &ResolvedFace::default());
    face.metrics.set_char_width_px(width);
    face
}

#[test]
fn fixed_terminal_capability_requires_no_font_logical_mode_and_valid_face() {
    let faces = [face_with_width(8.4)];
    let logical = DisplayRowGlyphMeasurer::with_mode(
        &faces,
        None,
        8.0,
        GlyphAdvanceQuantization::SnapToIntegerPixels,
        DisplayRowMeasurementMode::LogicalCells,
    );
    let fixed = logical
        .fixed_terminal_cell_advances(FaceId::new(2), 8.0)
        .unwrap();
    assert_eq!(fixed.advance_for_columns(1), Some(8.0));
    assert_eq!(fixed.advance_for_columns(2), Some(17.0));
    assert_ne!(
        fixed.advance_for_columns(2).unwrap(),
        fixed.advance_for_columns(1).unwrap() * 2.0
    );
    assert_eq!(fixed.advance_for_columns(0), None);
    assert_eq!(fixed.advance_for_columns(3), None);
    assert_eq!(fixed.advance_for_columns(usize::MAX), None);
    assert!(
        logical
            .fixed_terminal_cell_advances(FaceId::new(999), 8.0)
            .is_none()
    );
    let concrete = DisplayRowGlyphMeasurer::with_mode(
        &faces,
        None,
        8.0,
        GlyphAdvanceQuantization::PreserveLogicalPixels,
        DisplayRowMeasurementMode::ConcreteFont,
    );
    assert!(
        concrete
            .fixed_terminal_cell_advances(FaceId::new(2), 8.0)
            .is_none()
    );
    let mut fonts = crate::font::metrics::test_catalog::metrics_service();
    let logical_with_fonts = DisplayRowGlyphMeasurer::with_mode(
        &faces,
        Some(&mut fonts),
        8.0,
        GlyphAdvanceQuantization::SnapToIntegerPixels,
        DisplayRowMeasurementMode::LogicalCells,
    );
    assert!(
        logical_with_fonts
            .fixed_terminal_cell_advances(FaceId::new(2), 8.0)
            .is_none()
    );
}

#[test]
fn fixed_terminal_fractional_face_width_preserves_wide_glyphs_and_clipping() {
    for (area, expected_end, expected_widths) in [
        (
            GlyphArea::Text,
            DisplayRowPosition::new(25.0, 3),
            &[8.0, 17.0][..],
        ),
        (
            GlyphArea::LeftMargin,
            DisplayRowPosition::new(26.0, 4),
            &[8.0, 17.0, 1.0][..],
        ),
    ] {
        let faces = [face_with_width(8.4)];
        let mut measurer = DisplayRowGlyphMeasurer::with_mode(
            &faces,
            None,
            8.0,
            GlyphAdvanceQuantization::SnapToIntegerPixels,
            DisplayRowMeasurementMode::LogicalCells,
        );
        let row_layout = layout();
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        let progress = DisplayRowProgressWriter::with_glyph_measurer_for_area(
            &row_layout,
            &mut row,
            &mut measurer,
            DisplayRowPosition::new(0.0, 0),
            26.0,
            area,
        )
        .push_item(independent_text_item("A中W"));
        assert_eq!(progress.status(), DisplayRowAppendStatus::Clipped);
        assert_eq!(progress.end(), expected_end);
        assert_eq!(
            progress
                .slots()
                .iter()
                .map(|slot| slot.width_px())
                .collect::<Vec<_>>(),
            expected_widths
        );
        let glyphs = &row.glyphs[area.index()];
        assert_eq!(
            glyphs
                .iter()
                .filter(|glyph| !glyph.padding)
                .map(|glyph| glyph.pixel_width)
                .collect::<Vec<_>>(),
            expected_widths
        );
        assert!(glyphs[1].wide);
        assert!(glyphs[2].padding);
    }
}

struct RecordingLogical<'a> {
    concrete: DisplayRowGlyphMeasurer<'a>,
    capability_queries: Cell<usize>,
    glyph_calls: Vec<(char, u8)>,
    run_queries: usize,
}
impl DisplayGlyphMeasurer for RecordingLogical<'_> {
    fn glyph_advance_px(
        &mut self,
        ch: char,
        face: FaceId,
        columns: u8,
        fallback: f32,
    ) -> Option<f32> {
        self.glyph_calls.push((ch, columns));
        self.concrete.glyph_advance_px(ch, face, columns, fallback)
    }
    fn fixed_terminal_cell_advances(
        &self,
        face: FaceId,
        fallback: f32,
    ) -> Option<FixedTerminalCellAdvances> {
        self.capability_queries
            .set(self.capability_queries.get() + 1);
        self.concrete.fixed_terminal_cell_advances(face, fallback)
    }
    fn text_run_advances_px(&mut self, _: &str, _: FaceId, _: f32) -> DisplayTextRunMeasurement {
        self.run_queries += 1;
        DisplayTextRunMeasurement::PerChar
    }
    fn display_geometry_backend(&self) -> DisplayGeometryBackend {
        self.concrete.display_geometry_backend()
    }
}

#[test]
fn fixed_terminal_capability_is_once_per_item_and_keeps_tab_callback_path() {
    let faces = [face_with_width(8.0)];
    let mut measurer = RecordingLogical {
        concrete: DisplayRowGlyphMeasurer::with_mode(
            &faces,
            None,
            8.0,
            GlyphAdvanceQuantization::SnapToIntegerPixels,
            DisplayRowMeasurementMode::LogicalCells,
        ),
        capability_queries: Cell::new(0),
        glyph_calls: Vec::new(),
        run_queries: 0,
    };
    let row_layout = layout();
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut writer = DisplayRowProgressWriter::with_glyph_measurer(
        &row_layout,
        &mut row,
        &mut measurer,
        DisplayRowPosition::new(0.0, 0),
        200.0,
    );
    assert_eq!(
        writer.push_item(independent_text_item("AB")).end(),
        DisplayRowPosition::new(16.0, 2)
    );
    let end = writer.push_item(independent_text_item("\tC")).end();
    assert_eq!(end, DisplayRowPosition::new(40.0, 5));
    drop(writer);
    assert_eq!(measurer.capability_queries.get(), 2);
    assert_eq!(measurer.run_queries, 2);
    assert_eq!(measurer.glyph_calls, [(' ', 1), (' ', 1)]);
}

#[test]
fn fixed_terminal_custom_backend_still_uses_character_specific_callbacks() {
    struct CustomTerminal {
        calls: Vec<char>,
    }
    impl DisplayGlyphMeasurer for CustomTerminal {
        fn display_geometry_backend(&self) -> DisplayGeometryBackend {
            DisplayGeometryBackend::TerminalCells
        }
        fn glyph_advance_px(
            &mut self,
            ch: char,
            _face: FaceId,
            _columns: u8,
            _fallback: f32,
        ) -> Option<f32> {
            self.calls.push(ch);
            Some(match ch {
                'm' => 12.0,
                'i' => 4.0,
                '中' => 19.0,
                _ => 14.0,
            })
        }
    }
    let mut measurer = CustomTerminal { calls: Vec::new() };
    let row_layout = layout();
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let progress = DisplayRowProgressWriter::with_glyph_measurer(
        &row_layout,
        &mut row,
        &mut measurer,
        DisplayRowPosition::new(0.0, 0),
        36.0,
    )
    .push_item(independent_text_item("mi中W"));
    assert_eq!(progress.end(), DisplayRowPosition::new(35.0, 4));
    assert_eq!(progress.status(), DisplayRowAppendStatus::Clipped);
    assert_eq!(measurer.calls, ['m', 'i', '中', 'W']);
}

#[test]
fn fixed_terminal_measured_run_plan_wins_without_capability_or_glyph_queries() {
    struct MeasuredPlan;
    impl DisplayGlyphMeasurer for MeasuredPlan {
        fn glyph_advance_px(
            &mut self,
            _ch: char,
            _face: FaceId,
            _columns: u8,
            _fallback: f32,
        ) -> Option<f32> {
            panic!("measured ordinary text must keep its supplied widths");
        }
        fn fixed_terminal_cell_advances(
            &self,
            _face: FaceId,
            _fallback: f32,
        ) -> Option<FixedTerminalCellAdvances> {
            panic!("a measured plan must not request fixed geometry");
        }
        fn text_run_advances_px(
            &mut self,
            _: &str,
            _: FaceId,
            _: f32,
        ) -> DisplayTextRunMeasurement {
            DisplayTextRunMeasurement::Measured(vec![
                DisplayTextRunAdvance::new(0, 0, 12.0),
                DisplayTextRunAdvance::new(1, 1, 4.0),
            ])
        }
    }
    for supplied_directly in [false, true] {
        let mut measurer = MeasuredPlan;
        let row_layout = layout();
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        let mut writer = if supplied_directly {
            DisplayRowProgressWriter::with_text_run_measurement_and_glyph_measurer_for_area_and_start_policy(
                &row_layout, &mut row,
                DisplayTextRunMeasurement::Measured(vec![
                    DisplayTextRunAdvance::new(0, 0, 12.0), DisplayTextRunAdvance::new(1, 1, 4.0),
                ]), &mut measurer, DisplayRowPosition::new(0.0, 0), 80.0,
                crate::display_row::geometry::DisplayRowTextAreaOrigin::row_local(),
                GlyphArea::Text, DisplayRowAppendStartPolicy::SourcePosition,
            )
        } else {
            DisplayRowProgressWriter::with_glyph_measurer(
                &row_layout,
                &mut row,
                &mut measurer,
                DisplayRowPosition::new(0.0, 0),
                80.0,
            )
        };
        let progress = writer.push_item(independent_text_item("mi"));
        assert_eq!(progress.end(), DisplayRowPosition::new(16.0, 2));
        assert_eq!(
            progress
                .slots()
                .iter()
                .map(|slot| slot.width_px())
                .collect::<Vec<_>>(),
            [12.0, 4.0]
        );
    }
}

#[test]
fn fixed_terminal_special_kinds_keep_existing_emission_and_lookup_rules() {
    for (item, expected_end, expected_calls) in [
        (
            text_item("a\u{301}"),
            DisplayRowPosition::new(8.0, 1),
            vec![],
        ),
        (
            independent_text_item("\u{301}"),
            DisplayRowPosition::new(0.0, 0),
            vec![],
        ),
        (
            text_item("اب"),
            DisplayRowPosition::new(16.0, 2),
            vec![('ب', 1)],
        ),
    ] {
        let faces = [face_with_width(8.0)];
        let mut measurer = RecordingLogical {
            concrete: DisplayRowGlyphMeasurer::with_mode(
                &faces,
                None,
                8.0,
                GlyphAdvanceQuantization::SnapToIntegerPixels,
                DisplayRowMeasurementMode::LogicalCells,
            ),
            capability_queries: Cell::new(0),
            glyph_calls: Vec::new(),
            run_queries: 0,
        };
        let row_layout = layout();
        let mut row = GlyphRow::new(GlyphRowRole::Text);
        let progress = DisplayRowProgressWriter::with_glyph_measurer(
            &row_layout,
            &mut row,
            &mut measurer,
            DisplayRowPosition::new(0.0, 0),
            200.0,
        )
        .push_item(item);
        assert_eq!(progress.end(), expected_end);
        assert_eq!(measurer.glyph_calls, expected_calls);
        assert_eq!(measurer.capability_queries.get(), 1);
    }
    let faces = [face_with_width(8.0)];
    let mut measurer = RecordingLogical {
        concrete: DisplayRowGlyphMeasurer::with_mode(
            &faces,
            None,
            8.0,
            GlyphAdvanceQuantization::SnapToIntegerPixels,
            DisplayRowMeasurementMode::LogicalCells,
        ),
        capability_queries: Cell::new(0),
        glyph_calls: Vec::new(),
        run_queries: 0,
    };
    let row_layout = layout();
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut control = text_item("x");
    control.kind = DisplayItemKind::ControlChar { ch: '\u{1}' };
    let progress = DisplayRowProgressWriter::with_glyph_measurer(
        &row_layout,
        &mut row,
        &mut measurer,
        DisplayRowPosition::new(0.0, 0),
        200.0,
    )
    .push_item(control);
    assert_eq!(progress.end(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(measurer.capability_queries.get(), 0);
    assert_eq!(measurer.glyph_calls, [('^', 1), ('A', 1)]);
}

#[test]
fn fixed_terminal_missing_face_keeps_row_fallback_geometry() {
    let faces = [face_with_width(20.0)];
    let mut measurer = DisplayRowGlyphMeasurer::with_mode(
        &faces,
        None,
        8.0,
        GlyphAdvanceQuantization::SnapToIntegerPixels,
        DisplayRowMeasurementMode::LogicalCells,
    );
    let row_layout = layout();
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let mut item = independent_text_item("A中");
    item.face = RenderFaceRef::FaceId(FaceId::new(999));
    let progress = DisplayRowProgressWriter::with_glyph_measurer(
        &row_layout,
        &mut row,
        &mut measurer,
        DisplayRowPosition::new(0.0, 0),
        200.0,
    )
    .push_item(item);
    assert_eq!(progress.end(), DisplayRowPosition::new(24.0, 3));
    assert_eq!(
        progress
            .slots()
            .iter()
            .map(|slot| slot.width_px())
            .collect::<Vec<_>>(),
        [8.0, 16.0]
    );
}

/// An independent default-None wrapper preserves the pre-capability callback
/// path, allowing final glyph/slot/pen geometry to be compared directly.
struct LookupOnlyLogical<'a> {
    concrete: DisplayRowGlyphMeasurer<'a>,
    glyph_calls: Vec<(char, u8)>,
}
impl DisplayGlyphMeasurer for LookupOnlyLogical<'_> {
    fn glyph_advance_px(
        &mut self,
        ch: char,
        face: FaceId,
        columns: u8,
        fallback: f32,
    ) -> Option<f32> {
        self.glyph_calls.push((ch, columns));
        self.concrete.glyph_advance_px(ch, face, columns, fallback)
    }
    fn display_geometry_backend(&self) -> DisplayGeometryBackend {
        self.concrete.display_geometry_backend()
    }
}

fn fixed_terminal_snapshot(
    row_layout: &DisplayRowLayout,
    measurer: &mut dyn DisplayGlyphMeasurer,
    text: &str,
    edge: f32,
) -> (GlyphRow, DisplayRowAppendProgress) {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let progress = DisplayRowProgressWriter::with_glyph_measurer(
        row_layout,
        &mut row,
        measurer,
        DisplayRowPosition::new(0.0, 0),
        edge,
    )
    .push_item(independent_text_item(text));
    (row, progress)
}

#[test]
fn fixed_terminal_final_geometry_matches_character_path_for_distinct_fallbacks_and_quantization() {
    for (face_width, row_width, measurer_fallback, quantization, expected_widths) in [
        (
            8.4,
            10.4,
            6.0,
            GlyphAdvanceQuantization::SnapToIntegerPixels,
            [10.0, 21.0],
        ),
        (
            8.4,
            10.4,
            6.0,
            GlyphAdvanceQuantization::PreserveLogicalPixels,
            [10.4, 20.8],
        ),
        (
            8.4,
            10.4,
            13.4,
            GlyphAdvanceQuantization::SnapToIntegerPixels,
            [13.0, 27.0],
        ),
        (
            8.4,
            10.4,
            13.4,
            GlyphAdvanceQuantization::PreserveLogicalPixels,
            [13.4, 26.8],
        ),
        (
            0.0,
            8.0,
            11.4,
            GlyphAdvanceQuantization::SnapToIntegerPixels,
            [11.0, 23.0],
        ),
        (
            0.0,
            8.0,
            11.4,
            GlyphAdvanceQuantization::PreserveLogicalPixels,
            [11.4, 22.8],
        ),
        (
            0.0,
            8.0,
            4.2,
            GlyphAdvanceQuantization::SnapToIntegerPixels,
            [8.0, 16.0],
        ),
        (
            0.0,
            8.0,
            4.2,
            GlyphAdvanceQuantization::PreserveLogicalPixels,
            [8.0, 16.0],
        ),
    ] {
        let faces = [face_with_width(face_width)];
        let mut fixed = RecordingLogical {
            concrete: DisplayRowGlyphMeasurer::with_mode(
                &faces,
                None,
                measurer_fallback,
                quantization,
                DisplayRowMeasurementMode::LogicalCells,
            ),
            capability_queries: Cell::new(0),
            glyph_calls: Vec::new(),
            run_queries: 0,
        };
        let mut lookup = LookupOnlyLogical {
            concrete: DisplayRowGlyphMeasurer::with_mode(
                &faces,
                None,
                measurer_fallback,
                quantization,
                DisplayRowMeasurementMode::LogicalCells,
            ),
            glyph_calls: Vec::new(),
        };
        let mut row_layout = layout();
        row_layout.char_width_px = row_width;
        let (fixed_row, fixed_progress) =
            fixed_terminal_snapshot(&row_layout, &mut fixed, "A中", 200.0);
        let (lookup_row, lookup_progress) =
            fixed_terminal_snapshot(&row_layout, &mut lookup, "A中", 200.0);
        assert_eq!(fixed_row, lookup_row);
        assert_eq!(fixed_progress, lookup_progress);
        assert_eq!(fixed_progress.status(), DisplayRowAppendStatus::Complete);
        assert_eq!(
            fixed_progress.end(),
            DisplayRowPosition::new(expected_widths.iter().sum(), 3)
        );
        assert_eq!(
            fixed_progress
                .slots()
                .iter()
                .map(|slot| slot.width_px())
                .collect::<Vec<_>>(),
            expected_widths
        );
        assert_eq!(
            fixed_row.glyphs[GlyphArea::Text.index()]
                .iter()
                .filter(|glyph| !glyph.padding)
                .map(|glyph| glyph.pixel_width)
                .collect::<Vec<_>>(),
            expected_widths
        );
        assert_eq!(fixed.capability_queries.get(), 1);
        assert_eq!(fixed.run_queries, 1);
        assert!(fixed.glyph_calls.is_empty());
        assert_eq!(lookup.glyph_calls, [('A', 1), ('中', 2)]);
    }
}

#[test]
fn fixed_terminal_stored_explicit_per_char_plan_keeps_capability_and_default_none_paths() {
    let faces = [face_with_width(8.4)];
    let row_layout = layout();
    let mut fixed = RecordingLogical {
        concrete: DisplayRowGlyphMeasurer::with_mode(
            &faces,
            None,
            8.0,
            GlyphAdvanceQuantization::SnapToIntegerPixels,
            DisplayRowMeasurementMode::LogicalCells,
        ),
        capability_queries: Cell::new(0),
        glyph_calls: Vec::new(),
        run_queries: 0,
    };
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let progress = DisplayRowProgressWriter::with_text_run_measurement_and_glyph_measurer_for_area_and_start_policy(
        &row_layout, &mut row, DisplayTextRunMeasurement::PerChar, &mut fixed,
        DisplayRowPosition::new(0.0, 0), 200.0,
        crate::display_row::geometry::DisplayRowTextAreaOrigin::row_local(),
        GlyphArea::Text, DisplayRowAppendStartPolicy::SourcePosition,
    ).push_item(independent_text_item("A中"));
    assert_eq!(progress.end(), DisplayRowPosition::new(25.0, 3));
    assert_eq!(
        progress
            .slots()
            .iter()
            .map(|slot| slot.width_px())
            .collect::<Vec<_>>(),
        [8.0, 17.0]
    );
    assert_eq!(
        fixed.run_queries, 0,
        "the stored plan bypasses run measurement"
    );
    assert_eq!(fixed.capability_queries.get(), 1);
    assert!(fixed.glyph_calls.is_empty());

    struct StoredPlanOnly {
        calls: Vec<char>,
    }
    impl DisplayGlyphMeasurer for StoredPlanOnly {
        fn display_geometry_backend(&self) -> DisplayGeometryBackend {
            DisplayGeometryBackend::TerminalCells
        }
        fn text_run_advances_px(
            &mut self,
            _: &str,
            _: FaceId,
            _: f32,
        ) -> DisplayTextRunMeasurement {
            panic!("an explicit PerChar plan must not request a replacement run plan");
        }
        fn glyph_advance_px(&mut self, ch: char, _: FaceId, _: u8, _: f32) -> Option<f32> {
            self.calls.push(ch);
            Some(match ch {
                'm' => 12.0,
                'i' => 4.0,
                _ => panic!("unexpected character"),
            })
        }
        // Deliberately retain the trait's default-None capability.
    }
    let mut custom = StoredPlanOnly { calls: Vec::new() };
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let progress = DisplayRowProgressWriter::with_text_run_measurement_and_glyph_measurer_for_area_and_start_policy(
        &row_layout, &mut row, DisplayTextRunMeasurement::PerChar, &mut custom,
        DisplayRowPosition::new(0.0, 0), 200.0,
        crate::display_row::geometry::DisplayRowTextAreaOrigin::row_local(),
        GlyphArea::Text, DisplayRowAppendStartPolicy::SourcePosition,
    ).push_item(independent_text_item("mi"));
    assert_eq!(progress.end(), DisplayRowPosition::new(16.0, 2));
    assert_eq!(
        progress
            .slots()
            .iter()
            .map(|slot| slot.width_px())
            .collect::<Vec<_>>(),
        [12.0, 4.0]
    );
    assert_eq!(custom.calls, ['m', 'i']);
}

#[test]
fn fixed_terminal_short_empty_special_and_rejected_items_preserve_callback_geometry() {
    for (text, edge, expected_end, expected_status, expected_calls) in [
        (
            "",
            200.0,
            DisplayRowPosition::new(0.0, 0),
            DisplayRowAppendStatus::Complete,
            vec![],
        ),
        (
            "A",
            200.0,
            DisplayRowPosition::new(8.0, 1),
            DisplayRowAppendStatus::Complete,
            vec![],
        ),
        (
            "AB",
            200.0,
            DisplayRowPosition::new(16.0, 2),
            DisplayRowAppendStatus::Complete,
            vec![],
        ),
        (
            "中",
            200.0,
            DisplayRowPosition::new(16.0, 2),
            DisplayRowAppendStatus::Complete,
            vec![],
        ),
        (
            "\t",
            200.0,
            DisplayRowPosition::new(32.0, 4),
            DisplayRowAppendStatus::Complete,
            vec![(' ', 1), (' ', 1)],
        ),
        (
            "\u{301}",
            200.0,
            DisplayRowPosition::new(0.0, 0),
            DisplayRowAppendStatus::Complete,
            vec![],
        ),
        (
            "A",
            0.0,
            DisplayRowPosition::new(0.0, 0),
            DisplayRowAppendStatus::Clipped,
            vec![],
        ),
    ] {
        let faces = [face_with_width(8.0)];
        let mut fixed = RecordingLogical {
            concrete: DisplayRowGlyphMeasurer::with_mode(
                &faces,
                None,
                8.0,
                GlyphAdvanceQuantization::SnapToIntegerPixels,
                DisplayRowMeasurementMode::LogicalCells,
            ),
            capability_queries: Cell::new(0),
            glyph_calls: Vec::new(),
            run_queries: 0,
        };
        let mut lookup = LookupOnlyLogical {
            concrete: DisplayRowGlyphMeasurer::with_mode(
                &faces,
                None,
                8.0,
                GlyphAdvanceQuantization::SnapToIntegerPixels,
                DisplayRowMeasurementMode::LogicalCells,
            ),
            glyph_calls: Vec::new(),
        };
        let (fixed_row, fixed_progress) =
            fixed_terminal_snapshot(&layout(), &mut fixed, text, edge);
        let (lookup_row, lookup_progress) =
            fixed_terminal_snapshot(&layout(), &mut lookup, text, edge);
        assert_eq!(fixed_row, lookup_row);
        assert_eq!(fixed_progress, lookup_progress);
        assert_eq!(fixed_progress.end(), expected_end);
        assert_eq!(fixed_progress.status(), expected_status);
        assert_eq!(
            fixed.capability_queries.get(),
            1,
            "eager setup remains once even when unused"
        );
        assert_eq!(fixed.run_queries, 1);
        assert_eq!(fixed.glyph_calls, expected_calls);
        // These assertions expose callback behavior, not an instruction budget.
        // The short/unused capability arithmetic still needs direct measurement.
    }
}

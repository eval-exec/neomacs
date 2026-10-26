use super::super::{
    ChromeCompositionRules, ChromeMemo, ChromeRootAlias, ChromeRowContext, ChromeRowInputs,
    ChromeSourceIndex, ChromeSpanShape, ChromeTextContent, ChromeTextStorage, RetainedChrome,
};
use super::*;
use crate::display_row::builder::DisplayTabPolicy;
use crate::display_row::measured_state::WindowChromeKind;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::neovm_bridge::ResolvedFace;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::GlyphRow;
use neomacs_display_protocol::types::Rect;
use neovm_core::buffer::CharPos0;
use neovm_core::emacs_core::image_catalog::ImageScaleEnvironment;
use std::sync::Arc;

const FACE: FaceId = FaceId::new(7);

fn fingerprint(text: &str) -> ChromeRowFingerprint {
    ChromeRowFingerprint {
        context: ChromeRowContext {
            inputs: ChromeRowInputs {
                kind: WindowChromeKind::ModeLine,
                display_row_index: 1,
                bounds: Rect::new(0.0, 16.0, 640.0, 16.0),
                text_area_left_px: 0.0,
                selected: true,
                metrics: DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
                tab_policy: DisplayTabPolicy::from_tab_width_and_stops(0.0, 8, &[]),
                base_face: ResolvedFace::default(),
                image_scale_environment: ImageScaleEnvironment::default(),
                glyphless_table_bits: None,
                automatic_composition: ChromeCompositionRules::Absent,
                face_change_count: 0,
                char_table_revision: neovm_core::window::CharTableLayoutRevision::default(),
                media_generation: 0,
                symbol_values: Box::new([]),
            },
            storage: ChromeTextStorage::Unibyte,
            props: Box::new([]),
            spans: Box::new([]),
            root_alias: ChromeRootAlias::Separate,
        },
        text: ChromeTextContent(text.as_bytes().into()),
    }
}

fn row(text: &str) -> GlyphRow {
    let mut row = GlyphRow::new(GlyphRowRole::ModeLine);
    for (index, ch) in text.chars().enumerate() {
        // The reservoir deliberately ignores this old source identity: replay
        // must construct fresh root provenance instead of retaining these stamps.
        let mut glyph = Glyph::char(ch, FACE, 1_000 + index);
        glyph.pixel_width = 8.0;
        glyph.pixel_height = 16.0;
        glyph.pixel_ascent = 12.0;
        row.glyphs[GlyphArea::Text.index()].push(glyph);
    }
    row
}

fn memo(previous: ChromeRowFingerprint, row: GlyphRow) -> ChromeMemo {
    ChromeMemo::new(
        RetainedChrome {
            rows: vec![(1, MatrixRow::new(row))],
            row_snapshots: Vec::new(),
            chrome_strings: Default::default(),
            metrics: Default::default(),
        },
        Arc::from(vec![previous]),
    )
}

#[test]
fn unchanged_runs_survive_a_changed_middle_digit() {
    let previous = fingerprint("buf L510 tail");
    let current = fingerprint("buf L511 tail");
    let memo = memo(previous, row("buf L510 tail"));
    let reservoir = memo
        .glyph_reservoir(&current)
        .expect("same paint environment");
    let prefix = CharRange::new(CharPos0::new(0), CharPos0::new(4));
    let position = CharRange::new(CharPos0::new(4), CharPos0::new(8));
    let suffix = CharRange::new(CharPos0::new(8), CharPos0::new(13));
    assert_eq!(reservoir.glyphs_for(prefix, "buf ", FACE).unwrap().len(), 4);
    assert!(reservoir.glyphs_for(position, "L511", FACE).is_none());
    assert_eq!(
        reservoir.glyphs_for(suffix, " tail", FACE).unwrap().len(),
        5
    );
}

#[test]
fn native_sequence_validation_rejects_missing_extra_or_reordered_glyphs() {
    for old_row in [row("ab"), row("abc "), row("acb")] {
        assert!(
            memo(fingerprint("abc"), old_row)
                .glyph_reservoir(&fingerprint("abd"))
                .is_none()
        );
    }
}

#[test]
fn unsupported_whole_row_geometry_cannot_produce_a_reservoir() {
    let mutations: &[fn(&mut GlyphRow)] = &[
        |row| row.reversed_p = true,
        |row| row.glyphs[GlyphArea::Text.index()][0].padding = true,
        |row| row.glyphs[GlyphArea::Text.index()][0].wide = true,
        |row| row.glyphs[GlyphArea::Text.index()][0].bidi_level = 2,
        |row| {
            row.glyphs[GlyphArea::Text.index()][0].glyph_type = GlyphType::Stretch { width_cols: 1 }
        },
        |row| row.glyphs[GlyphArea::Text.index()][0].pixel_width = 0.0,
        |row| row.glyphs[GlyphArea::Text.index()][0].pixel_width = f32::NAN,
        |row| row.glyphs[GlyphArea::Text.index()][0].pixel_height = f32::INFINITY,
        |row| row.glyphs[GlyphArea::Text.index()][0].pixel_ascent = -1.0,
        |row| row.glyphs[GlyphArea::Text.index()][0].vertical_offset_px = f32::NAN,
    ];
    for mutate in mutations {
        let mut old_row = row("abc");
        mutate(&mut old_row);
        assert!(
            memo(fingerprint("abc"), old_row)
                .glyph_reservoir(&fingerprint("abd"))
                .is_none()
        );
    }
}

#[test]
fn complete_canonical_context_is_required_even_for_unchanged_text() {
    let mutations: &[fn(&mut ChromeRowFingerprint)] = &[
        |key| key.context.inputs.selected = false,
        |key| key.context.inputs.face_change_count += 1,
        |key| key.context.inputs.media_generation += 1,
        |key| key.context.inputs.automatic_composition = ChromeCompositionRules::Present,
        |key| key.context.storage = ChromeTextStorage::Multibyte,
        |key| key.context.props = Box::new([1]),
        |key| key.context.root_alias = ChromeRootAlias::Source(ChromeSourceIndex::from(0)),
        |key| {
            key.context.spans = Box::new([ChromeSpanShape {
                output_start: 0,
                output_end: 3,
                source_start: 0,
                source_index: 0,
            }]);
        },
    ];
    let memo = memo(fingerprint("abc"), row("abc"));
    for mutate in mutations {
        let mut current = fingerprint("abc");
        mutate(&mut current);
        assert!(memo.glyph_reservoir(&current).is_none());
    }
}

#[test]
fn composition_rules_do_not_exclude_independent_ascii_paint() {
    let mut previous = fingerprint("abc");
    previous.context.inputs.automatic_composition = ChromeCompositionRules::Present;
    let current = previous.clone();
    assert!(
        memo(previous, row("abc"))
            .glyph_reservoir(&current)
            .is_some()
    );
}

#[test]
fn printable_ascii_and_unchanged_character_length_are_required() {
    for (previous, current) in [
        ("ab\t", "abd"),
        ("abc", "ab\n"),
        ("abc", "abcd"),
        ("aé", "aé"),
    ] {
        assert!(
            memo(fingerprint(previous), row(previous))
                .glyph_reservoir(&fingerprint(current))
                .is_none()
        );
    }
}

#[test]
fn run_key_rejects_text_face_range_and_mixed_face_changes() {
    let mut old_row = row("abc");
    old_row.glyphs[GlyphArea::Text.index()][2].face_id = FaceId::new(8);
    let reservoir = memo(fingerprint("abc"), old_row)
        .glyph_reservoir(&fingerprint("abd"))
        .unwrap();
    let whole = CharRange::new(CharPos0::new(0), CharPos0::new(3));
    let prefix = CharRange::new(CharPos0::new(0), CharPos0::new(2));
    assert!(reservoir.glyphs_for(whole, "abc", FACE).is_none());
    assert!(reservoir.glyphs_for(prefix, "ab", FACE).is_some());
    assert!(reservoir.glyphs_for(prefix, "ac", FACE).is_none());
    assert!(reservoir.glyphs_for(prefix, "ab", FaceId::new(8)).is_none());
    assert!(reservoir.glyphs_for(prefix, "a", FACE).is_none());
    assert!(
        reservoir
            .glyphs_for(CharRange::new(CharPos0::new(1), CharPos0::new(0)), "", FACE)
            .is_none()
    );
    assert!(
        reservoir
            .glyphs_for(
                CharRange::new(CharPos0::new(3), CharPos0::new(5)),
                "de",
                FACE
            )
            .is_none()
    );
}

#[test]
fn boxed_primitives_are_admitted_without_inferring_neighbor_topology() {
    let mut old_row = row("abc");
    old_row.glyphs[GlyphArea::Text.index()][0].box_vertical_edges =
        neomacs_display_protocol::face::BoxVerticalEdges::Both;
    let reservoir = memo(fingerprint("abc"), old_row)
        .glyph_reservoir(&fingerprint("abd"))
        .unwrap();
    assert!(
        reservoir
            .glyphs_for(
                CharRange::new(CharPos0::new(0), CharPos0::new(1)),
                "a",
                FACE
            )
            .is_some()
    );
}

#[test]
fn typed_root_alias_remains_part_of_full_fingerprint_equality() {
    let separate = fingerprint("abc");
    let mut aliased = separate.clone();
    aliased.context.root_alias = ChromeRootAlias::Source(ChromeSourceIndex::from(0));
    assert_ne!(separate, aliased);
    assert_eq!(separate.first_difference(&aliased), Some("text"));
    assert_eq!(separate, separate.clone());
}

#[test]
fn typed_content_and_alias_equality_matches_the_previous_footer_encoding() {
    // Test-only reference for the exact representation this change replaces.
    fn old_text_key(key: &ChromeRowFingerprint) -> Vec<u8> {
        let mut encoded = key.text.0.to_vec();
        let alias = match key.context.root_alias {
            ChromeRootAlias::Separate => u64::MAX,
            ChromeRootAlias::Source(ChromeSourceIndex(index)) => index as u64,
        };
        encoded.extend_from_slice(&alias.to_le_bytes());
        encoded
    }
    let mut variants = Vec::new();
    for text in ["abc", "abd", "abc\0", ""] {
        for alias in [
            ChromeRootAlias::Separate,
            ChromeRootAlias::Source(ChromeSourceIndex::from(0)),
            ChromeRootAlias::Source(ChromeSourceIndex::from(1)),
        ] {
            let mut key = fingerprint(text);
            key.context.root_alias = alias;
            variants.push(key);
        }
    }
    for left in &variants {
        for right in &variants {
            assert_eq!(left == right, old_text_key(left) == old_text_key(right));
        }
    }
}

use crate::display_item::{
    DisplayItem, DisplayItemKind, DisplayItemLayout, DisplayPointerAppearance,
    DisplayPointerSourceRange, DisplaySourcePosition, DisplayTextComposition, DisplayTextRun,
    RenderFaceRef, SourceSpan,
};
use crate::display_row::builder::{
    ChromePrimitiveReuse, DisplayGlyphMeasurer, DisplayRowAppendProgress, DisplayRowLayout,
    DisplayRowPosition, DisplayRowProgressWriter, DisplayRowVerticalMetrics,
    FixedTerminalCellAdvances, new_display_row,
};
use crate::display_row::root_lisp_string_id;
use neomacs_display_protocol::glyph_matrix::{
    GlyphPointerAppearance, GlyphPointerOccurrenceIdentity, GlyphPointerSourceIdentity,
    GlyphPointerSourceKind, GlyphProvenance, TerminalComposition, TerminalCompositionCell,
};

fn layout() -> DisplayRowLayout {
    DisplayRowLayout {
        line_number_width_px: 0.0,
        role: GlyphRowRole::ModeLine,
        y_px: 0.0,
        height_px: 16.0,
        ascent_px: 12.0,
        char_width_px: 8.0,
        tab_policy: DisplayTabPolicy::every(4),
        base_face: RenderFaceRef::FaceId(FACE),
        pixel_calc: crate::display_pixel_calc::PixelCalcContext::for_chrome_row(
            240.0,
            8.0,
            16.0,
            std::collections::HashMap::new(),
        ),
        space_image_params: None,
        line_wrap: crate::display_row::append_context::DisplayRowLineWrap::chrome_row(),
    }
}

fn item(text: &str) -> DisplayItem {
    let root = root_lisp_string_id().get();
    DisplayItem::new(
        SourceSpan::new(
            DisplaySourcePosition::lisp_string(root, 0, 0),
            DisplaySourcePosition::lisp_string(root, text.len(), text.len()),
        ),
        RenderFaceRef::FaceId(FACE),
        DisplayItemKind::TextRun(DisplayTextRun::independent(text)),
    )
}

struct FixedMetrics;

impl DisplayGlyphMeasurer for FixedMetrics {
    fn fixed_terminal_cell_advances(
        &self,
        _face: FaceId,
        _fallback: f32,
    ) -> Option<FixedTerminalCellAdvances> {
        FixedTerminalCellAdvances::new(8.0, 16.0)
    }

    fn glyph_advance_px(
        &mut self,
        _ch: char,
        _face: FaceId,
        _columns: u8,
        _fallback: f32,
    ) -> Option<f32> {
        Some(8.0)
    }

    fn glyph_vertical_metrics_px(
        &mut self,
        _ch: char,
        _face: FaceId,
    ) -> Option<DisplayRowVerticalMetrics> {
        Some(DisplayRowVerticalMetrics::new(23.0, 17.0))
    }
}

fn write(
    item: DisplayItem,
    position: DisplayRowPosition,
    max_x: f32,
    reuse: &ChromePrimitiveReuse,
) -> (GlyphRow, DisplayRowAppendProgress) {
    write_with_measurer(item, position, max_x, reuse, &mut FixedMetrics)
}

fn write_with_measurer(
    item: DisplayItem,
    position: DisplayRowPosition,
    max_x: f32,
    reuse: &ChromePrimitiveReuse,
    measurer: &mut impl DisplayGlyphMeasurer,
) -> (GlyphRow, DisplayRowAppendProgress) {
    let layout = layout();
    let mut row = new_display_row(&layout);
    let progress =
        DisplayRowProgressWriter::with_glyph_measurer(&layout, &mut row, measurer, position, max_x)
            .push_item_with_chrome_reuse(&item, reuse);
    (row, progress)
}

fn assert_same_output(
    canonical: (GlyphRow, DisplayRowAppendProgress),
    mut reused: (GlyphRow, DisplayRowAppendProgress),
) -> (GlyphRow, DisplayRowAppendProgress) {
    // Fresh rows deliberately get distinct opaque paint-cache identities.
    // Compare every semantic/native field after normalizing that identity only.
    reused.0.keep_appearance_of(&canonical.0);
    assert_eq!(canonical, reused);
    reused
}

fn writer_reservoir() -> ChromeGlyphReservoir {
    let (mut old_row, _) = write(
        item("abc"),
        DisplayRowPosition::new(0.0, 0),
        240.0,
        &ChromePrimitiveReuse::Canonical,
    );
    let pointer = old_row.intern_pointer_appearance(GlyphPointerAppearance {
        source: GlyphPointerSourceIdentity {
            kind: GlyphPointerSourceKind::Synthetic,
            source_id: 99,
            range_start: 0,
            range_end: 3,
            property_owner: 0,
            occurrence: GlyphPointerOccurrenceIdentity::Source,
        },
        face_id: FaceId::new(99),
    });
    for glyph in &mut old_row.glyphs[GlyphArea::Text.index()] {
        glyph.provenance = GlyphProvenance::buffer(9_000);
        glyph.pointer_appearance = pointer;
        glyph.box_vertical_edges = neomacs_display_protocol::face::BoxVerticalEdges::Both;
    }
    memo(fingerprint("abc"), old_row)
        .glyph_reservoir(&fingerprint("abd"))
        .expect("native printable ASCII primitive reservoir")
}

#[test]
fn writer_reuse_rebinds_current_source_pointer_boxes_and_fractional_pen() {
    let reservoir = writer_reservoir();
    let range = CharRange::new(CharPos0::new(0), CharPos0::new(2));
    assert!(reservoir.glyphs_for(range, "ab", FACE).is_some());
    let mut current = item("ab");
    current.pointer_appearance = Some(DisplayPointerAppearance::new(
        DisplayPointerSourceRange::effective(current.span.start.clone(), 0, 2, None),
        RenderFaceRef::FaceId(FaceId::new(13)),
    ));
    current.box_run_membership = neomacs_display_protocol::face::BoxRunMembership::Boxed;
    current.box_vertical_edges = neomacs_display_protocol::face::BoxVerticalEdges::Neither;
    let position = DisplayRowPosition::new(4.25, 2);
    let canonical = write(
        current.clone(),
        position,
        240.0,
        &ChromePrimitiveReuse::Canonical,
    );
    let reused = write(
        current,
        position,
        240.0,
        &ChromePrimitiveReuse::Retained(reservoir),
    );
    let reused = assert_same_output(canonical, reused);
    assert_eq!(reused.0.height_px, 23.0);
    assert_eq!(reused.0.ascent_px, 17.0);
    assert_eq!(reused.1.end(), DisplayRowPosition::new(20.25, 4));
    assert_eq!(reused.0.pointer_appearances().len(), 1);
    assert_eq!(reused.0.pointer_appearances()[0].face_id, FaceId::new(13));
}

/// Required glyph queries make a degenerate all-fallback implementation fail.
struct ReuseOnlyMetrics;

impl DisplayGlyphMeasurer for ReuseOnlyMetrics {
    fn fixed_terminal_cell_advances(
        &self,
        _face: FaceId,
        _fallback: f32,
    ) -> Option<FixedTerminalCellAdvances> {
        FixedTerminalCellAdvances::new(8.0, 16.0)
    }

    fn glyph_advance_px(
        &mut self,
        _ch: char,
        _face: FaceId,
        _columns: u8,
        _fallback: f32,
    ) -> Option<f32> {
        panic!("admitted reuse unexpectedly queried character metrics")
    }

    fn glyph_vertical_metrics_px(
        &mut self,
        _ch: char,
        _face: FaceId,
    ) -> Option<DisplayRowVerticalMetrics> {
        panic!("admitted reuse unexpectedly queried character metrics")
    }
}

#[test]
fn writer_admitted_reuse_skips_character_measurement_queries() {
    let canonical = write(
        item("ab"),
        DisplayRowPosition::new(0.0, 0),
        240.0,
        &ChromePrimitiveReuse::Canonical,
    );
    let reuse = ChromePrimitiveReuse::Retained(writer_reservoir());
    let actual = write_with_measurer(
        item("ab"),
        DisplayRowPosition::new(0.0, 0),
        240.0,
        &reuse,
        &mut ReuseOnlyMetrics,
    );
    assert_same_output(canonical, actual);
}

struct UncertifiedMetrics;

impl DisplayGlyphMeasurer for UncertifiedMetrics {
    fn glyph_advance_px(
        &mut self,
        _ch: char,
        _face: FaceId,
        _columns: u8,
        _fallback: f32,
    ) -> Option<f32> {
        Some(7.0)
    }
}

#[test]
fn writer_without_fixed_cell_certificate_uses_current_measurement() {
    let position = DisplayRowPosition::new(0.0, 0);
    let canonical = write_with_measurer(
        item("ab"),
        position,
        240.0,
        &ChromePrimitiveReuse::Canonical,
        &mut UncertifiedMetrics,
    );
    let reuse = ChromePrimitiveReuse::Retained(writer_reservoir());
    let actual = write_with_measurer(item("ab"), position, 240.0, &reuse, &mut UncertifiedMetrics);
    assert_eq!(actual.1.end().x_px(), 14.0);
    assert_same_output(canonical, actual);
}

#[test]
fn writer_unicode_fallback_respects_a_changed_prefix_cluster_tail() {
    let reservoir = memo(fingerprint("abc"), row("abc"))
        .glyph_reservoir(&fingerprint("dbc"))
        .expect("same owner context");
    let range = CharRange::new(CharPos0::new(1), CharPos0::new(3));
    assert!(reservoir.glyphs_for(range, "bc", FACE).is_some());
    let mut current = item("bc");
    current.span = SourceSpan::new(
        DisplaySourcePosition::lisp_string(root_lisp_string_id().get(), 1, 1),
        DisplaySourcePosition::lisp_string(root_lisp_string_id().get(), 3, 3),
    );
    current.kind = DisplayItemKind::TextRun(DisplayTextRun::new("bc"));
    let append = |reuse: &ChromePrimitiveReuse| {
        let layout = layout();
        let mut row = new_display_row(&layout);
        let mut prefix = Glyph::char('x', FACE, 0);
        prefix.glyph_type = GlyphType::Composite {
            text: "x\u{200d}".into(),
        };
        prefix.pixel_width = 8.0;
        row.glyphs[GlyphArea::Text.index()].push(prefix);
        row.displays_text = true;
        let progress = DisplayRowProgressWriter::with_glyph_measurer(
            &layout,
            &mut row,
            &mut FixedMetrics,
            DisplayRowPosition::new(8.0, 1),
            240.0,
        )
        .push_item_with_chrome_reuse(&current, reuse);
        (row, progress)
    };
    assert_same_output(
        append(&ChromePrimitiveReuse::Canonical),
        append(&ChromePrimitiveReuse::Retained(reservoir)),
    );
}

#[test]
fn writer_falls_back_for_clipping_changed_text_source_layout_and_composition() {
    let reservoir = writer_reservoir();
    let reuse = ChromePrimitiveReuse::Retained(reservoir);
    let position = DisplayRowPosition::new(0.0, 0);
    let clipped = write(item("ab"), position, 12.0, &ChromePrimitiveReuse::Canonical);
    assert_same_output(clipped, write(item("ab"), position, 12.0, &reuse));
    let mut foreign = item("ab");
    foreign.span = SourceSpan::new(
        DisplaySourcePosition::lisp_string(987, 0, 0),
        DisplaySourcePosition::lisp_string(987, 2, 2),
    );
    let mut raised = item("ab");
    raised.layout = DisplayItemLayout {
        raise: Some(0.5),
        ..DisplayItemLayout::default()
    };
    let automatic = DisplayItem::new(
        item("ab").span,
        RenderFaceRef::FaceId(FACE),
        DisplayItemKind::TextRun(DisplayTextRun::with_composition(
            "ab",
            DisplayTextComposition::Automatic(TerminalComposition {
                cells: ['a', 'b']
                    .map(|base| TerminalCompositionCell {
                        base,
                        extenders: "".into(),
                        width_cols: 1,
                        source_char_len: 1,
                    })
                    .into(),
                width_cols: 2,
            }),
        )),
    );
    for current in [item("ac"), foreign, raised, automatic] {
        assert_same_output(
            write(
                current.clone(),
                position,
                240.0,
                &ChromePrimitiveReuse::Canonical,
            ),
            write(current, position, 240.0, &reuse),
        );
    }
}

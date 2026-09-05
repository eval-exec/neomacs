use super::*;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::presentation_origin::BufferModiff;
use neomacs_display_protocol::types::DisplayWindowId;

/// A text row rendering `content` at `pixel_y`.
///
/// Every row carries the same character range on purpose: an edit renumbers
/// positions, so nothing this module does may depend on them.
fn text_row(content: u64, pixel_y: f32) -> GlyphRow {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.displays_text = true;
    row.hash = content;
    row.start_charpos = 0;
    row.end_charpos = 1;
    row.pixel_y = pixel_y;
    row.height_px = 16.0;
    row
}

/// A window showing `buffer_id` whose text is at modification tick `modiff`.
fn window(buffer_id: u64, modiff: u64) -> WindowInfo {
    WindowInfo {
        window_id: DisplayWindowId::new(1),
        buffer_id,
        window_start: 100,
        window_end: 200,
        buffer_size: 10_000,
        buffer_modiff: BufferModiff::new(modiff),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
        geometry: neomacs_display_protocol::PresentedWindowGeometry::default(),
        mode_line_height: 20.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_name: String::from("scratch"),
        buffer_file_name: String::new(),
        modified: true,
    }
}

/// The imprints a presentation's rows offer.
fn imprints(rows: &[GlyphRow]) -> Vec<RowImprint> {
    rows.iter().filter_map(RowImprint::of).collect()
}

// =======================================================================
// A measured reflow
// =======================================================================

#[test]
fn an_insertion_above_shifts_the_rows_below_it_and_leaves_the_rest_where_they_were() {
    // A line was inserted after the second row. The first two rows do not
    // move; everything after them moves down by exactly one row height.
    let before = [
        text_row(0xA1, 0.0),
        text_row(0xA2, 16.0),
        text_row(0xA3, 32.0),
        text_row(0xA4, 48.0),
    ];
    let after = [
        text_row(0xA1, 0.0),
        text_row(0xA2, 16.0),
        text_row(0xFF, 32.0), // the inserted line: new content, matches nothing
        text_row(0xA3, 48.0),
        text_row(0xA4, 64.0),
    ];
    match shift(
        &window(7, 1),
        &window(7, 2),
        &imprints(&before),
        &imprints(&after),
    ) {
        RowShift::Shifted {
            pixels,
            first_moved_y,
            rows,
        } => {
            assert!((pixels - 16.0).abs() < 1e-4, "moved down one row: {pixels}");
            assert!(
                (first_moved_y - 48.0).abs() < 1e-4,
                "the reflow begins at the topmost row that MOVED, not at the \
                 top of the window and not at the topmost matched row: \
                 {first_moved_y}"
            );
            assert_eq!(rows.get(), 2, "two rows were displaced");
        }
        other => panic!("expected a measured shift, got {other:?}"),
    }
}

#[test]
fn a_deletion_above_shifts_the_rows_below_it_upward() {
    // The second row was deleted. The row above it holds still; the two below
    // it come up by one row height, so the delta is negative.
    let before = [
        text_row(0xB1, 0.0),
        text_row(0xB2, 16.0),
        text_row(0xB3, 32.0),
        text_row(0xB4, 48.0),
    ];
    let after = [
        text_row(0xB1, 0.0),
        text_row(0xB3, 16.0),
        text_row(0xB4, 32.0),
    ];
    match shift(
        &window(7, 4),
        &window(7, 5),
        &imprints(&before),
        &imprints(&after),
    ) {
        RowShift::Shifted {
            pixels,
            first_moved_y,
            rows,
        } => {
            assert!(
                (pixels + 16.0).abs() < 1e-4,
                "negative: the rows moved up the screen: {pixels}"
            );
            assert!(
                (first_moved_y - 16.0).abs() < 1e-4,
                "the moved run starts where the deleted row used to be: \
                 {first_moved_y}"
            );
            assert_eq!(rows.get(), 2);
        }
        other => panic!("expected a measured shift, got {other:?}"),
    }
}

#[test]
fn a_long_stationary_run_above_the_edit_does_not_drag_the_measurement_toward_zero() {
    // Six rows sit above the edit and one below it. Averaging every matched
    // row would report about two pixels; the answer is the moved group's
    // sixteen, and the reflow begins near the bottom of the window rather
    // than at its top.
    let before: Vec<GlyphRow> = (0..7)
        .map(|i| text_row(0xC0 + i, i as f32 * 16.0))
        .collect();
    let mut after: Vec<GlyphRow> = (0..6)
        .map(|i| text_row(0xC0 + i, i as f32 * 16.0))
        .collect();
    after.push(text_row(0xFE, 96.0)); // the inserted line
    after.push(text_row(0xC6, 112.0));

    match shift(
        &window(7, 1),
        &window(7, 2),
        &imprints(&before),
        &imprints(&after),
    ) {
        RowShift::Shifted {
            pixels,
            first_moved_y,
            rows,
        } => {
            assert!((pixels - 16.0).abs() < 1e-4, "got {pixels}");
            assert!((first_moved_y - 112.0).abs() < 1e-4, "got {first_moved_y}");
            assert_eq!(rows.get(), 1, "only the last row was displaced");
        }
        other => panic!("expected a measured shift, got {other:?}"),
    }
}

#[test]
fn rows_are_recognized_by_content_even_though_the_edit_renumbered_their_positions() {
    // This is why matching cannot key on charpos the way scroll does: after an
    // insertion above them the same rows carry entirely different character
    // ranges, and a range-keyed matcher would find no correspondence at all.
    let mut before = [text_row(0xD1, 0.0), text_row(0xD2, 16.0)];
    before[0].start_charpos = 100;
    before[0].end_charpos = 120;
    before[1].start_charpos = 120;
    before[1].end_charpos = 140;

    let mut after = [text_row(0xD1, 16.0), text_row(0xD2, 32.0)];
    after[0].start_charpos = 137;
    after[0].end_charpos = 157;
    after[1].start_charpos = 157;
    after[1].end_charpos = 177;

    assert_eq!(
        shift(
            &window(7, 1),
            &window(7, 2),
            &imprints(&before),
            &imprints(&after)
        )
        .shifted_pixels(),
        Some(16.0)
    );
}

#[test]
fn the_shift_is_the_height_of_what_changed_not_one_character_height() {
    // An inserted image row is three text rows tall. The producer hint this
    // replaces reports a constant `char_height` whatever was inserted; a
    // measurement reports what the rows below it actually did.
    let before = [text_row(0xE1, 0.0), text_row(0xE2, 16.0)];
    let mut inserted = text_row(0xEE, 16.0);
    inserted.height_px = 48.0;
    let after = [text_row(0xE1, 0.0), inserted, text_row(0xE2, 64.0)];
    assert_eq!(
        shift(
            &window(7, 1),
            &window(7, 2),
            &imprints(&before),
            &imprints(&after)
        )
        .shifted_pixels(),
        Some(48.0)
    );
}

#[test]
fn rows_that_all_held_their_positions_report_unchanged() {
    // Typing inside a line that did not rewrap: the tick moved, the rows
    // around it did not, and nothing was displaced.
    let before = [text_row(0xF1, 0.0), text_row(0xF2, 16.0)];
    let after = [text_row(0xF1, 0.0), text_row(0xF2, 16.0)];
    assert_eq!(
        shift(
            &window(7, 1),
            &window(7, 2),
            &imprints(&before),
            &imprints(&after)
        ),
        RowShift::Unchanged
    );
}

// =======================================================================
// The cases where a number would be a lie
// =======================================================================

#[test]
fn an_unchanged_modification_tick_is_a_scroll_and_must_not_be_claimed_as_a_reflow() {
    // Every row moved up by one row height with the text untouched. That is a
    // viewport moving, which is the sibling module's fact, not this one's.
    // Note the inversion: scroll refuses when the tick CHANGED, this refuses
    // when it did not.
    let before = [text_row(0x11, 16.0), text_row(0x12, 32.0)];
    let after = [text_row(0x11, 0.0), text_row(0x12, 16.0)];
    let got = shift(
        &window(7, 3),
        &window(7, 3),
        &imprints(&before),
        &imprints(&after),
    );
    assert_eq!(
        got,
        RowShift::Ambiguous {
            reason: ReflowAmbiguity::SameBufferTick
        }
    );
    assert_eq!(got.shifted_pixels(), None);
}

#[test]
fn a_different_buffer_is_ambiguous_however_well_the_rows_match() {
    // Two buffers can share a row fingerprint — an empty line, a lone brace —
    // and the distance between two such rows means nothing.
    let before = [text_row(0x21, 16.0)];
    let after = [text_row(0x21, 0.0)];
    assert_eq!(
        shift(
            &window(7, 1),
            &window(9, 2),
            &imprints(&before),
            &imprints(&after)
        ),
        RowShift::Ambiguous {
            reason: ReflowAmbiguity::BufferChanged
        }
    );
}

#[test]
fn rows_that_disagree_about_the_distance_are_ambiguous() {
    // Two rows moved by different amounts, so more happened than one edit
    // pushing a run of rows: there is no single shift to report.
    let before = [text_row(0x31, 0.0), text_row(0x32, 16.0)];
    let after = [text_row(0x31, 16.0), text_row(0x32, 48.0)];
    assert_eq!(
        shift(
            &window(7, 1),
            &window(7, 2),
            &imprints(&before),
            &imprints(&after)
        ),
        RowShift::Ambiguous {
            reason: ReflowAmbiguity::InconsistentShift
        }
    );
}

#[test]
fn a_stationary_row_below_the_moved_run_is_not_the_shape_of_an_edit() {
    // An edit's shift runs to the bottom of the window: everything after the
    // edit moves. A row that stayed put *below* rows that moved therefore
    // means the match is wrong, or something other than an edit changed the
    // layout. Reporting a shift here would claim a region that did not move —
    // the failure that tolerating stationary rows above the edit risks.
    let before = [text_row(0x41, 0.0), text_row(0x42, 48.0)];
    let after = [text_row(0x41, 16.0), text_row(0x42, 48.0)];
    assert_eq!(
        shift(
            &window(7, 1),
            &window(7, 2),
            &imprints(&before),
            &imprints(&after)
        ),
        RowShift::Ambiguous {
            reason: ReflowAmbiguity::StationaryRowBelowShift
        }
    );
}

#[test]
fn a_wholesale_rewrite_leaves_no_common_rows() {
    let before = [text_row(0x51, 0.0), text_row(0x52, 16.0)];
    let after = [text_row(0x91, 0.0), text_row(0x92, 16.0)];
    assert_eq!(
        shift(
            &window(7, 1),
            &window(7, 2),
            &imprints(&before),
            &imprints(&after)
        ),
        RowShift::Ambiguous {
            reason: ReflowAmbiguity::NoCommonRows
        }
    );
}

#[test]
fn repeated_identical_rows_within_one_presentation_cannot_be_matched() {
    // Identical lines are ordinary in code. Two rows rendering the same thing
    // say nothing about which of them ended up where, so neither may be used —
    // though a naive matcher would happily pair one of them and report 32px.
    let before = [text_row(0x61, 0.0), text_row(0x61, 16.0)];
    let after = [text_row(0x61, 32.0)];
    assert_eq!(
        shift(
            &window(7, 1),
            &window(7, 2),
            &imprints(&before),
            &imprints(&after)
        ),
        RowShift::Ambiguous {
            reason: ReflowAmbiguity::NoCommonRows
        },
        "an ambiguous fingerprint is evidence of nothing"
    );
}

// =======================================================================
// Which rows may participate
// =======================================================================

#[test]
fn a_mode_line_never_participates_because_an_edit_does_not_displace_it() {
    // The mode line sits at the bottom and stays there while the text above it
    // is pushed down. If it participated it would be a stationary row below
    // the moved run, and the whole measurement would be rejected.
    let mut mode_before = text_row(0x71, 580.0);
    mode_before.mode_line = true;
    let mut mode_after = text_row(0x71, 580.0);
    mode_after.mode_line = true;

    let before = [text_row(0x72, 0.0), text_row(0x73, 16.0), mode_before];
    let after = [text_row(0x72, 0.0), text_row(0x73, 32.0), mode_after];
    match shift(
        &window(7, 1),
        &window(7, 2),
        &imprints(&before),
        &imprints(&after),
    ) {
        RowShift::Shifted {
            pixels,
            first_moved_y,
            rows,
        } => {
            assert!((pixels - 16.0).abs() < 1e-4, "got {pixels}");
            assert!((first_moved_y - 32.0).abs() < 1e-4, "got {first_moved_y}");
            assert_eq!(rows.get(), 1);
        }
        other => panic!("the mode line must not disturb the measurement: {other:?}"),
    }
}

#[test]
fn disabled_blank_synthetic_and_unhashed_rows_never_participate() {
    let mut disabled = text_row(0x81, 0.0);
    disabled.enabled = false;
    let mut blank = text_row(0x82, 16.0);
    blank.displays_text = false;
    let mut synthetic = text_row(0x83, 32.0);
    synthetic.start_charpos = NO_BUFFER_POSITION_CHARPOS;
    synthetic.end_charpos = NO_BUFFER_POSITION_CHARPOS;
    // Hash zero is the protocol's "no content computed" sentinel rather than a
    // fingerprint; matching on it would pair every such row with every other.
    let unhashed = text_row(0, 48.0);

    let before = [
        disabled.clone(),
        blank.clone(),
        synthetic.clone(),
        unhashed.clone(),
    ];
    let after = [disabled, blank, synthetic, unhashed];
    assert_eq!(
        shift(
            &window(7, 1),
            &window(7, 2),
            &imprints(&before),
            &imprints(&after)
        ),
        RowShift::Ambiguous {
            reason: ReflowAmbiguity::NoCommonRows
        },
        "none of these rows may be matched, so nothing is measurable"
    );
}

// =======================================================================
// Diagnostics
// =======================================================================

#[test]
fn outcomes_have_stable_names() {
    let name: &'static str = RowShift::Unchanged.into();
    assert_eq!(name, "unchanged");
    let reason: &'static str = ReflowAmbiguity::SameBufferTick.into();
    assert_eq!(reason, "same_buffer_tick");
    let reason: &'static str = ReflowAmbiguity::StationaryRowBelowShift.into();
    assert_eq!(reason, "stationary_row_below_shift");
}

// =======================================================================
// Extracting imprints from a presentation
// =======================================================================

#[test]
fn imprints_are_taken_per_window_and_exclude_rows_that_cannot_be_matched() {
    use neomacs_display_protocol::glyph_matrix::{FrameDisplayState, GlyphMatrix, MatrixRow};

    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);

    let mut matrix = GlyphMatrix::new(0, 80);
    let mut mode = text_row(0x99, 580.0);
    mode.mode_line = true;
    for row in [text_row(0x9A, 0.0), text_row(0x9B, 16.0), mode] {
        matrix.rows.push(MatrixRow::new(row));
    }
    state
        .window_matrices
        .push(neomacs_display_protocol::glyph_matrix::WindowMatrixEntry {
            window_id: DisplayWindowId::new(3),
            matrix,
            pixel_bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
            text_pixel_bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 580.0),
            text_clip_bounds: None,
            selected: false,
        });

    let by_window = imprints_by_window(&state);
    let imprints = by_window
        .get(&DisplayWindowId::new(3))
        .expect("the window contributed imprints");
    assert_eq!(
        imprints.len(),
        2,
        "an edit does not displace the mode line, so it leaves no imprint"
    );
}

#[test]
fn a_presentation_with_no_matrices_yields_no_imprints() {
    use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
    let state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    assert!(imprints_by_window(&state).is_empty());
}

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

// =======================================================================
// The compositor path: two installs, one observation
// =======================================================================

/// A frame presenting `window`, with `rows` already reduced to imprints.
///
/// `measure_reflow` reads the retained frame's `window_infos` and takes the
/// incoming presentation's imprints separately, mirroring ingest: the imprints
/// are extracted while the matrices still exist, the buffer that survives has
/// no rows.
fn install(
    render: &mut crate::render_thread::frame_windows::GuiFrameRenderState,
    info: &WindowInfo,
    rows: &[GlyphRow],
) {
    let mut frame = crate::core::frame_glyphs::FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.presentation_id = next_presentation(render);
    frame.window_infos.push(info.clone());
    let mut by_window = std::collections::HashMap::default();
    by_window.insert(info.window_id, imprints(rows));
    render.measure_reflow(Some(&frame), &by_window);
    render.compositor.incoming_reflow_imprints = by_window;
    render.compositor.current_frame = Some(frame);
    // Say the frame reached the screen. The measurement baseline advances only
    // on composition, so a test that installed without composing would be
    // measuring the next presentation against one the user never saw — which
    // is the very thing that advance rule exists to prevent.
    render.begin_presentable_render();
}

/// A presentation id one past whatever is retained, so each install is a
/// distinct presentation — promotion is idempotent per presentation, so reusing
/// an id would silently skip advancing the baseline.
fn next_presentation(
    render: &crate::render_thread::frame_windows::GuiFrameRenderState,
) -> neomacs_display_protocol::PresentationId {
    let last = render
        .compositor
        .current_frame
        .as_ref()
        .map_or(0, |frame| frame.presentation_id.get());
    neomacs_display_protocol::PresentationId::new(last + 1)
}

fn empty_render() -> crate::render_thread::frame_windows::GuiFrameRenderState {
    crate::render_thread::frame_windows::GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    )
}

#[test]
fn installing_an_edited_presentation_leaves_one_measured_reflow_pending() {
    let mut render = empty_render();
    let before = [
        text_row(0xB1, 0.0),
        text_row(0xB2, 16.0),
        text_row(0xB3, 32.0),
    ];
    install(&mut render, &window(7, 1), &before);
    assert!(
        render.compositor.pending.reflows.is_empty(),
        "the first install has no previous presentation to have moved from"
    );

    // A line was inserted after the first row: the rest slid down 16px.
    let after = [
        text_row(0xB1, 0.0),
        text_row(0xB2, 32.0),
        text_row(0xB3, 48.0),
    ];
    install(&mut render, &window(7, 2), &after);

    let pending = render.take_pending_continuity(true);
    assert_eq!(pending.reflows.len(), 1);
    let reflow = pending.reflows[0];
    assert_eq!(reflow.window, DisplayWindowId::new(1));
    assert!(
        (reflow.pixels - 16.0).abs() < f32::EPSILON,
        "measured, not assumed"
    );
    assert!(
        (reflow.first_moved_y - 32.0).abs() < f32::EPSILON,
        "the displaced run begins where it now sits, not at the cursor"
    );
}

#[test]
fn the_minibuffer_is_never_measured_for_a_reflow() {
    // Every `M-x` rewrites the echo area wholesale. Sliding it would fire the
    // effect on nearly every command, and there is no edit behind it.
    let mut render = empty_render();
    let mut mini = window(7, 1);
    mini.is_minibuffer = true;
    let before = [text_row(0xC1, 0.0), text_row(0xC2, 16.0)];
    install(&mut render, &mini, &before);

    let mut mini_after = window(7, 2);
    mini_after.is_minibuffer = true;
    let after = [text_row(0xC1, 16.0), text_row(0xC2, 32.0)];
    install(&mut render, &mini_after, &after);

    assert!(render.take_pending_continuity(true).reflows.is_empty());
}

#[test]
fn an_ambiguous_shift_leaves_nothing_pending_rather_than_a_guess() {
    let mut render = empty_render();
    install(
        &mut render,
        &window(7, 1),
        &[text_row(0xD1, 0.0), text_row(0xD2, 16.0)],
    );
    // Rows that disagree about the distance mean the layout changed as well as
    // shifting; there is no one displacement to animate.
    install(
        &mut render,
        &window(7, 2),
        &[text_row(0xD1, 16.0), text_row(0xD2, 64.0)],
    );
    assert!(render.take_pending_continuity(true).reflows.is_empty());
}

#[test]
fn a_second_pass_over_one_install_observes_no_reflow() {
    let mut render = empty_render();
    install(
        &mut render,
        &window(7, 1),
        &[text_row(0xE1, 0.0), text_row(0xE2, 16.0)],
    );
    install(
        &mut render,
        &window(7, 2),
        &[text_row(0xE1, 16.0), text_row(0xE2, 32.0)],
    );
    assert_eq!(render.take_pending_continuity(true).reflows.len(), 1);
    assert!(
        render.take_pending_continuity(true).reflows.is_empty(),
        "re-arming the slide on every render pass would sustain a redraw loop"
    );
}

/// Install `windows`/`rows` WITHOUT composing, the way `poll_frame` does when
/// two commits arrive between ticks.
fn install_without_composing(
    render: &mut crate::render_thread::frame_windows::GuiFrameRenderState,
    info: &WindowInfo,
    rows: &[GlyphRow],
) {
    let mut frame = crate::core::frame_glyphs::FrameGlyphBuffer::with_size(800.0, 600.0);
    frame.presentation_id = next_presentation(render);
    frame.window_infos.push(info.clone());
    let mut by_window = std::collections::HashMap::default();
    by_window.insert(info.window_id, imprints(rows));
    render.measure_reflow(Some(&frame), &by_window);
    render.compositor.incoming_reflow_imprints = by_window;
    render.compositor.current_frame = Some(frame);
}

#[test]
fn a_commit_superseded_before_it_was_drawn_does_not_become_the_thing_the_next_one_is_measured_against()
 {
    // `poll_frame` drains the whole channel, so two commits arriving between
    // ticks are both installed and only the second is ever drawn. Measuring
    // against the presentation that was never composed reports the motion from
    // a picture nobody saw; the user's eye travelled the whole distance, from
    // the last frame on screen to this one.
    let mut render = empty_render();
    let first = [text_row(0xF1, 0.0), text_row(0xF2, 16.0)];
    install(&mut render, &window(7, 1), &first);

    // Commit two: rows slide 16px. Installed, never composed.
    let second = [text_row(0xF1, 16.0), text_row(0xF2, 32.0)];
    install_without_composing(&mut render, &window(7, 2), &second);

    // Commit three: another 16px. This is the frame that will actually draw.
    let third = [text_row(0xF1, 32.0), text_row(0xF2, 48.0)];
    install_without_composing(&mut render, &window(7, 3), &third);

    let pending = render.take_pending_continuity(true);
    assert_eq!(pending.reflows.len(), 1);
    assert!(
        (pending.reflows[0].pixels - 32.0).abs() < f32::EPSILON,
        "measured from the last composed frame, not from the commit in between: \
         got {}px, expected the full 32px the viewer's eye travelled",
        pending.reflows[0].pixels
    );
}

#[test]
fn an_observation_survives_a_commit_that_supersedes_it_before_any_frame_drew() {
    // Each measurement clears its own pending list first, so before the
    // baseline advance rule a second commit wiped the first one's findings.
    // A reflow that happened would simply never animate whenever the editor
    // batched two commits - which is under load, exactly when it shows.
    let mut render = empty_render();
    install(
        &mut render,
        &window(7, 1),
        &[text_row(0xE1, 0.0), text_row(0xE2, 16.0)],
    );
    install_without_composing(
        &mut render,
        &window(7, 2),
        &[text_row(0xE1, 16.0), text_row(0xE2, 32.0)],
    );
    // A commit that moves nothing further - a keystroke redrawing the buffer.
    install_without_composing(
        &mut render,
        &window(7, 3),
        &[text_row(0xE1, 16.0), text_row(0xE2, 32.0)],
    );

    assert_eq!(
        render.take_pending_continuity(true).reflows.len(),
        1,
        "the displacement is still pending, not cleared by the commit after it"
    );
}

use super::*;
use crate::render_thread::frame_compositor::continuity::ScrollObservation;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::presentation_origin::BufferModiff;
use neomacs_display_protocol::types::DisplayWindowId;

/// A text row showing `[start, end)` at `pixel_y`.
fn text_row(start: usize, end: usize, pixel_y: f32) -> GlyphRow {
    let mut row = GlyphRow::new(GlyphRowRole::Text);
    row.enabled = true;
    row.displays_text = true;
    row.start_charpos = start;
    row.end_charpos = end;
    row.pixel_y = pixel_y;
    row.height_px = 16.0;
    row
}

fn window(buffer_id: u64, window_start: i64, modiff: u64) -> WindowInfo {
    let mut info = window_at(window_start);
    info.buffer_id = buffer_id;
    info.buffer_modiff = BufferModiff::new(modiff);
    info
}

fn window_at(window_start: i64) -> WindowInfo {
    WindowInfo {
        window_id: DisplayWindowId::new(1),
        buffer_id: 7,
        window_start,
        window_end: window_start + 100,
        buffer_size: 10_000,
        buffer_modiff: BufferModiff::new(1),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 800.0, 600.0),
        geometry: neomacs_display_protocol::PresentedWindowGeometry::default(),
        line_number_field: None,
        mode_line_height: 20.0,
        header_line_height: 0.0,
        tab_line_height: 0.0,
        selected: true,
        is_minibuffer: false,
        char_height: 16.0,
        buffer_name: String::from("scratch"),
        buffer_file_name: String::new(),
        modified: false,
    }
}

/// The anchors a presentation's rows offer.
fn refs(rows: &[GlyphRow]) -> Vec<RowAnchor> {
    rows.iter().filter_map(RowAnchor::of).collect()
}

// =======================================================================
// Exact displacement
// =======================================================================

#[test]
fn a_three_line_scroll_measures_exactly_three_line_heights() {
    // The same three rows, each 48px higher after the scroll.
    let before = [
        text_row(100, 120, 48.0),
        text_row(120, 140, 64.0),
        text_row(140, 160, 80.0),
    ];
    let after = [
        text_row(100, 120, 0.0),
        text_row(120, 140, 16.0),
        text_row(140, 160, 32.0),
    ];
    let got = displacement(
        &window(7, 100, 1),
        &window(7, 148, 1),
        &refs(&before),
        &refs(&after),
    );
    match got {
        ScrollDisplacement::Exact {
            pixels,
            direction,
            anchors,
        } => {
            assert!((pixels - 48.0).abs() < 1e-4, "got {pixels}");
            assert_eq!(direction, ScrollDirection::TowardBufferEnd);
            assert_eq!(anchors.get(), 3, "all three rows anchor the measurement");
        }
        other => panic!("expected an exact displacement, got {other:?}"),
    }
}

#[test]
fn one_surviving_row_is_enough_to_be_exact() {
    let before = [text_row(100, 120, 200.0), text_row(120, 140, 216.0)];
    let after = [text_row(120, 140, 8.0), text_row(140, 160, 24.0)];
    let got = displacement(
        &window(7, 100, 1),
        &window(7, 120, 1),
        &refs(&before),
        &refs(&after),
    );
    assert_eq!(got.exact_pixels(), Some(208.0));
}

#[test]
fn variable_row_heights_do_not_disturb_the_measurement() {
    // An image row twice the text height sits between two text rows. A
    // character-count estimate would get this wrong; an anchor cannot.
    let mut tall_before = text_row(120, 121, 64.0);
    tall_before.height_px = 32.0;
    let mut tall_after = text_row(120, 121, 24.0);
    tall_after.height_px = 32.0;
    let before = [
        text_row(100, 120, 48.0),
        tall_before,
        text_row(121, 140, 96.0),
    ];
    let after = [
        text_row(100, 120, 8.0),
        tall_after,
        text_row(121, 140, 56.0),
    ];
    assert_eq!(
        displacement(
            &window(7, 100, 1),
            &window(7, 110, 1),
            &refs(&before),
            &refs(&after)
        )
        .exact_pixels(),
        Some(40.0)
    );
}

#[test]
fn scrolling_backward_reports_the_distance_as_a_magnitude() {
    let before = [text_row(100, 120, 0.0)];
    let after = [text_row(100, 120, 32.0)];
    let got = displacement(
        &window(7, 140, 1),
        &window(7, 100, 1),
        &refs(&before),
        &refs(&after),
    );
    assert_eq!(
        got.exact_pixels(),
        Some(32.0),
        "distance, not signed offset"
    );
    assert_eq!(got.direction(), ScrollDirection::TowardBufferStart);
}

// =======================================================================
// The cases where an exact answer would be a lie
// =======================================================================

#[test]
fn an_equal_length_edit_is_ambiguous_even_though_every_range_still_matches() {
    // This is the case the modification tick exists for. Both presentations
    // have identical charpos ranges at different pixel positions, so a matcher
    // without the tick would confidently return 16.0 — for text that changed.
    let before = [text_row(100, 120, 16.0)];
    let after = [text_row(100, 120, 0.0)];
    let got = displacement(
        &window(7, 100, 1),
        &window(7, 100, 2),
        &refs(&before),
        &refs(&after),
    );
    assert_eq!(
        got,
        ScrollDisplacement::Ambiguous {
            reason: AmbiguityReason::ModiffChanged,
            direction: ScrollDirection::TowardBufferStart,
        }
    );
    assert_eq!(got.exact_pixels(), None);
}

#[test]
fn a_different_buffer_is_ambiguous_regardless_of_matching_ranges() {
    let before = [text_row(100, 120, 16.0)];
    let after = [text_row(100, 120, 0.0)];
    let got = displacement(
        &window(7, 100, 1),
        &window(9, 100, 1),
        &refs(&before),
        &refs(&after),
    );
    assert_eq!(
        got,
        ScrollDisplacement::Ambiguous {
            reason: AmbiguityReason::BufferChanged,
            direction: ScrollDirection::TowardBufferStart,
        }
    );
}

#[test]
fn rows_that_disagree_about_the_distance_are_ambiguous() {
    // The window was scrolled AND reflowed, so there is no one displacement.
    let before = [text_row(100, 120, 48.0), text_row(120, 140, 64.0)];
    let after = [text_row(100, 120, 0.0), text_row(120, 140, 32.0)];
    let got = displacement(
        &window(7, 100, 1),
        &window(7, 130, 1),
        &refs(&before),
        &refs(&after),
    );
    assert_eq!(
        got,
        ScrollDisplacement::Ambiguous {
            reason: AmbiguityReason::InconsistentDisplacement,
            direction: ScrollDirection::TowardBufferEnd,
        }
    );
}

#[test]
fn a_repeated_range_within_one_presentation_cannot_anchor() {
    // Two rows claiming the same range say nothing about which one moved where.
    let before = [text_row(100, 120, 0.0), text_row(100, 120, 16.0)];
    let after = [text_row(100, 120, 32.0)];
    let got = displacement(
        &window(7, 100, 1),
        &window(7, 110, 1),
        &refs(&before),
        &refs(&after),
    );
    assert_eq!(
        got.exact_pixels(),
        None,
        "an ambiguous range is not an anchor"
    );
}

#[test]
fn a_page_jump_with_no_surviving_row_reports_no_overlap() {
    let before = [text_row(100, 120, 0.0), text_row(120, 140, 16.0)];
    let after = [text_row(9000, 9020, 0.0), text_row(9020, 9040, 16.0)];
    let got = displacement(
        &window(7, 100, 1),
        &window(7, 9000, 1),
        &refs(&before),
        &refs(&after),
    );
    assert_eq!(
        got,
        ScrollDisplacement::NoOverlap {
            direction: ScrollDirection::TowardBufferEnd
        }
    );
}

// =======================================================================
// Which rows may anchor
// =======================================================================

#[test]
fn a_mode_line_never_anchors_because_it_does_not_scroll() {
    let mut mode_before = text_row(100, 120, 580.0);
    mode_before.mode_line = true;
    let mut mode_after = text_row(100, 120, 580.0);
    mode_after.mode_line = true;
    // The mode line sits still while the text scrolls; anchoring on it would
    // report a displacement of zero.
    let before = [text_row(200, 220, 48.0), mode_before];
    let after = [text_row(200, 220, 0.0), mode_after];
    assert_eq!(
        displacement(
            &window(7, 200, 1),
            &window(7, 248, 1),
            &refs(&before),
            &refs(&after)
        )
        .exact_pixels(),
        Some(48.0)
    );
}

#[test]
fn synthetic_and_disabled_rows_never_anchor() {
    let mut synthetic = text_row(NO_BUFFER_POSITION_CHARPOS, NO_BUFFER_POSITION_CHARPOS, 0.0);
    synthetic.displays_text = true;
    let mut disabled = text_row(300, 320, 16.0);
    disabled.enabled = false;
    let mut blank = text_row(400, 420, 32.0);
    blank.displays_text = false;

    let before = [synthetic.clone(), disabled.clone(), blank.clone()];
    let after = [synthetic, disabled, blank];
    assert_eq!(
        displacement(
            &window(7, 100, 1),
            &window(7, 110, 1),
            &refs(&before),
            &refs(&after)
        ),
        ScrollDisplacement::Ambiguous {
            reason: AmbiguityReason::NoUniqueRow,
            direction: ScrollDirection::TowardBufferEnd,
        },
        "none of these rows may anchor, so nothing is measurable"
    );
}

#[test]
fn a_continued_row_does_not_match_its_continuation() {
    // Both halves of a wrapped line can share a charpos range; the flags are
    // what keep them apart.
    let mut continued = text_row(100, 140, 0.0);
    continued.continued = true;
    let tail = text_row(100, 140, 16.0);

    let before = [continued, tail];
    let after = [text_row(100, 140, 48.0)];
    // The two `before` rows differ by their flags, so both are unique keys, and
    // only the non-continued one matches.
    assert_eq!(
        displacement(
            &window(7, 100, 1),
            &window(7, 90, 1),
            &refs(&before),
            &refs(&after)
        )
        .exact_pixels(),
        Some(32.0)
    );
}

// =======================================================================
// Diagnostics
// =======================================================================

#[test]
fn outcomes_have_stable_names() {
    let name: &'static str = ScrollDisplacement::NoOverlap {
        direction: ScrollDirection::TowardBufferEnd,
    }
    .into();
    assert_eq!(name, "no_overlap");
    let reason: &'static str = AmbiguityReason::ModiffChanged.into();
    assert_eq!(reason, "modiff_changed");
}

// =======================================================================
// Extracting anchors from a presentation
// =======================================================================

#[test]
fn anchors_are_taken_per_window_and_exclude_rows_that_cannot_anchor() {
    use neomacs_display_protocol::glyph_matrix::{FrameDisplayState, GlyphMatrix, MatrixRow};

    let mut state = FrameDisplayState::new(80, 24, 8.0, 16.0);

    let mut matrix = GlyphMatrix::new(0, 80);
    let mut mode = text_row(500, 520, 580.0);
    mode.mode_line = true;
    for row in [text_row(100, 120, 0.0), text_row(120, 140, 16.0), mode] {
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

    let by_window = anchors_by_window(&state);
    let anchors = by_window
        .get(&DisplayWindowId::new(3))
        .expect("the window contributed anchors");
    assert_eq!(
        anchors.len(),
        2,
        "the mode line does not scroll, so it is not an anchor"
    );
}

#[test]
fn a_presentation_with_no_matrices_yields_no_anchors() {
    use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
    let state = FrameDisplayState::new(80, 24, 8.0, 16.0);
    assert!(anchors_by_window(&state).is_empty());
}

// =======================================================================
// Observations are consumed exactly once
// =======================================================================

#[test]
fn taking_pending_continuity_leaves_nothing_for_a_second_pass() {
    use crate::render_thread::frame_windows::GuiFrameRenderState;

    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    render.compositor.pending.scrolls.push(ScrollObservation {
        window: DisplayWindowId::new(3),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 100.0, 120.0),
        same_buffer: true,
        transition: None,
        displacement: ScrollDisplacement::NoOverlap {
            direction: ScrollDirection::TowardBufferEnd,
        },
    });

    let first = render.take_pending_continuity(
        &crate::render_thread::render_pass::surface::SurfaceAcquired::for_test(),
        true,
    );
    assert_eq!(
        first.scrolls.len(),
        1,
        "the frame that consumes them sees them"
    );
    assert!(
        first.accept_derived_effects,
        "stamped with the frame's quality plan"
    );

    // A render pass can run again over the same retained presentation. If it
    // saw the observation again, every derived effect would re-arm, report
    // needs_redraw, and schedule yet another pass — a loop with no editor
    // activity behind it.
    let second = render.take_pending_continuity(
        &crate::render_thread::render_pass::surface::SurfaceAcquired::for_test(),
        true,
    );
    assert!(
        second.scrolls.is_empty(),
        "a second pass over one install must observe nothing"
    );
}

#[test]
fn the_quality_plan_decides_whether_derived_effects_run() {
    use crate::render_thread::frame_windows::GuiFrameRenderState;

    let mut render = GuiFrameRenderState::new_without_device(
        0x42,
        false,
        neomacs_display_protocol::frame_time::observe_platform_now(),
    );
    assert!(
        !render
            .take_pending_continuity(
                &crate::render_thread::render_pass::surface::SurfaceAcquired::for_test(),
                false
            )
            .accept_derived_effects,
        "a degraded frame declines compositor-derived effects, as it did producer hints"
    );
}

#[test]
fn an_observation_without_compatible_pixels_plans_no_transition() {
    // A window whose buffer-owned region changed — a tab line appeared, or it
    // was split — cannot have retained pixels blitted into it. That
    // disqualifies the slide, but the window still scrolled, so effects that
    // draw over it are still entitled to know.
    let observation = ScrollObservation {
        window: DisplayWindowId::new(3),
        bounds: neomacs_display_protocol::types::Rect::new(0.0, 0.0, 100.0, 120.0),
        same_buffer: true,
        displacement: ScrollDisplacement::Exact {
            pixels: 48.0,
            direction: ScrollDirection::TowardBufferEnd,
            anchors: std::num::NonZeroUsize::new(1).expect("one"),
        },
        transition: None,
    };
    assert_eq!(
        observation.displacement.exact_pixels(),
        Some(48.0),
        "the measurement stands on its own"
    );
    assert!(
        observation.transition.is_none(),
        "but there is nothing safe to blit"
    );
}

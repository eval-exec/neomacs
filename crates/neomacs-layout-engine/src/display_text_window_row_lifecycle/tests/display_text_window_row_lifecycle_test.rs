use super::*;

#[test]
fn text_window_append_surface_request_reserves_right_columns() {
    let tab_stops = vec![4, 12];
    let surface = TextWindowAppendSurfaceRequest::new(
        20.0,
        200.0,
        16.0,
        true,
        true,
        8.0,
        6,
        &tab_stops,
        DisplayRowLineWrap::Truncate,
    )
    .into_surface();

    assert_eq!(surface.content_x(), 20.0);
    // GNU xdisp reserves both the continuation glyph and a non-rightmost
    // terminal window's border before it consumes body text.  The marker
    // must not replace the final source character at its display column.
    assert_eq!(surface.right_edge(), 188.0);
    assert_eq!(surface.full_text_right_edge(), 204.0);
}

#[test]
fn tall_partially_visible_point_row_matches_gnu_policy_matrix() {
    use crate::display_row::walk_state::{
        TallPointRowScrollContext, next_window_start_for_tall_partially_visible_point_row,
    };
    use neovm_core::buffer::LispCharPos1;
    use neovm_core::window::{DisplayRowEndSource, DisplayRowSnapshot, ScrollMargin};

    fn make_rows(normal_count: usize, normal_h: i64, tall_h: i64) -> Vec<DisplayRowSnapshot> {
        let mut rows = Vec::with_capacity(normal_count + 1);
        let mut y = 0i64;
        for idx in 0..=normal_count {
            let start = (idx as i64) * 10 + 1;
            let end = start + 8;
            let height = if idx == normal_count {
                tall_h
            } else {
                normal_h
            };
            rows.push(DisplayRowSnapshot {
                row: idx as i64,
                y,
                height,
                start_buffer_pos: Some(LispCharPos1::new(start)),
                end_buffer_pos: Some(LispCharPos1::new(end)),
                end_source: DisplayRowEndSource::Buffer,
                ..DisplayRowSnapshot::default()
            });
            y += height;
        }
        rows
    }

    let base_ctx = TallPointRowScrollContext {
        default_line_height: 17,
        is_minibuffer: false,
        has_vscroll: false,
        force_start: false,
        temp_scroll_step: true,
        already_scrolled_this_redisplay: false,
    };
    let no_margin = ScrollMargin::default();

    // 8-line window (body = 119 px = 7*17), tall row = 164 px > 119.
    // Line indices are 0-based layout charpos: line k starts at (k - 1) * 10.
    let run = |normal_count: usize,
               body_h: i64,
               tall_h: i64,
               policy: ScrollPolicy,
               ctx: TallPointRowScrollContext| {
        let rows = make_rows(normal_count, 17, tall_h);
        let point = (normal_count as i64) * 10;
        next_window_start_for_tall_partially_visible_point_row(
            &rows, point, 0, body_h, 0, policy, no_margin, ctx,
        )
    };

    // Default policy (sc=0, ss=0), cursor-only (temp_scroll_step=true):
    // line 2 -> start=line 2 (10); line 3 -> start=line 3 (20); line 6 -> recenter start=line 3 (20).
    assert_eq!(run(1, 119, 164, ScrollPolicy::Recenter, base_ctx), Some(10));
    assert_eq!(run(2, 119, 164, ScrollPolicy::Recenter, base_ctx), Some(20));
    assert_eq!(run(5, 119, 164, ScrollPolicy::Recenter, base_ctx), Some(20));

    // Non-cursor-only edit (temp_scroll_step=false) skips try_scrolling -> recenter keeps line 1.
    let edit_ctx = TallPointRowScrollContext {
        temp_scroll_step: false,
        ..base_ctx
    };
    assert_eq!(run(1, 119, 164, ScrollPolicy::Recenter, edit_ctx), None);
    // With sc=101, edit on line 6 still enters try_scrolling (extra=1,2 -> 2+3=5 -> line 6).
    assert_eq!(
        run(5, 119, 164, ScrollPolicy::Unlimited, edit_ctx),
        Some(50)
    );

    // Policy boundaries on 8-line window:
    assert_eq!(
        run(
            5,
            119,
            164,
            ScrollPolicy::Conservative { max_lines: 1 },
            base_ctx
        ),
        Some(20)
    );
    assert_eq!(
        run(
            5,
            119,
            164,
            ScrollPolicy::Conservative { max_lines: 2 },
            base_ctx
        ),
        Some(50)
    );
    assert_eq!(
        run(
            6,
            119,
            164,
            ScrollPolicy::Conservative { max_lines: 2 },
            base_ctx
        ),
        Some(30)
    );
    assert_eq!(
        run(6, 119, 164, ScrollPolicy::Step { lines: 2 }, base_ctx),
        Some(60)
    );
    assert_eq!(
        run(5, 119, 164, ScrollPolicy::Unlimited, base_ctx),
        Some(50)
    );

    // 16-line window (body = 255 px = 15*17), tall row at line 10 (295 px > 255):
    // sc=101 stops at extra > 1 after 1+2+3=6 lines (start=line 7 -> 60).
    // default recenter steps back body/2 (7 lines above -> start=line 3 -> 20).
    assert_eq!(
        run(9, 255, 295, ScrollPolicy::Unlimited, base_ctx),
        Some(60)
    );
    assert_eq!(run(9, 255, 295, ScrollPolicy::Recenter, base_ctx), Some(20));

    // GNU cursor_row_fully_visible_p suppression boundaries:
    assert_eq!(run(0, 119, 164, ScrollPolicy::Unlimited, base_ctx), None);
    assert_eq!(
        run(
            5,
            119,
            164,
            ScrollPolicy::Unlimited,
            TallPointRowScrollContext {
                has_vscroll: true,
                ..base_ctx
            }
        ),
        None
    );
    assert_eq!(
        run(
            5,
            119,
            164,
            ScrollPolicy::Unlimited,
            TallPointRowScrollContext {
                is_minibuffer: true,
                ..base_ctx
            }
        ),
        None
    );
    assert_eq!(
        run(
            5,
            119,
            164,
            ScrollPolicy::Unlimited,
            TallPointRowScrollContext {
                force_start: true,
                ..base_ctx
            }
        ),
        None
    );
    assert_eq!(
        run(
            5,
            119,
            164,
            ScrollPolicy::Unlimited,
            TallPointRowScrollContext {
                already_scrolled_this_redisplay: true,
                ..base_ctx
            }
        ),
        None
    );
}

fn tall_row_snapshots_with_heights(heights: &[i64]) -> Vec<neovm_core::window::DisplayRowSnapshot> {
    use neovm_core::buffer::LispCharPos1;
    use neovm_core::window::{DisplayRowEndSource, DisplayRowSnapshot};

    let mut rows = Vec::with_capacity(heights.len());
    let mut y = 0i64;
    for (idx, &height) in heights.iter().enumerate() {
        let start = (idx as i64) * 10 + 1;
        let end = start + 8;
        rows.push(DisplayRowSnapshot {
            row: idx as i64,
            y,
            height,
            start_buffer_pos: Some(LispCharPos1::new(start)),
            end_buffer_pos: Some(LispCharPos1::new(end)),
            end_source: DisplayRowEndSource::Buffer,
            ..DisplayRowSnapshot::default()
        });
        y += height;
    }
    rows
}

#[test]
fn tall_point_row_recenter_does_not_step_back_at_exact_two_thirds_threshold() {
    use crate::display_row::walk_state::{
        TallPointRowScrollContext, next_window_start_for_tall_partially_visible_point_row,
    };
    use neovm_core::window::ScrollMargin;

    // 3 normal rows of height 17 (2/3 * 17 = 11) followed by a tall point row
    // of height 100 at anchor = 3 (charpos 30). Non-cursor-only edit skips
    // try_scrolling and enters GNU's recenter: fallback (src/xdisp.c:11391-11392).
    let rows = tall_row_snapshots_with_heights(&[17, 17, 17, 100]);
    let ctx = TallPointRowScrollContext {
        default_line_height: 17,
        is_minibuffer: false,
        has_vscroll: false,
        force_start: false,
        temp_scroll_step: false,
        already_scrolled_this_redisplay: false,
    };

    // body = 56 -> dy = 28 = 17 + 11. After jumping back 1 row to idx = 2,
    // remaining shortfall is exactly 11 == 17 * 2 / 3. GNU uses strict '>'
    // so it must NOT step back further to idx = 1 (charpos 10).
    assert_eq!(
        next_window_start_for_tall_partially_visible_point_row(
            &rows,
            30,
            0,
            56,
            0,
            ScrollPolicy::Recenter,
            ScrollMargin::default(),
            ctx,
        ),
        Some(20),
        "exact 2/3 shortfall (11 == 17*2/3) must stop at row 2 (charpos 20)"
    );

    // body = 58 -> dy = 29 = 17 + 12. Remaining shortfall 12 > 11 strictly
    // exceeds 17 * 2 / 3, so GNU steps back once more to idx = 1 (charpos 10).
    assert_eq!(
        next_window_start_for_tall_partially_visible_point_row(
            &rows,
            30,
            0,
            58,
            0,
            ScrollPolicy::Recenter,
            ScrollMargin::default(),
            ctx,
        ),
        Some(10),
        "shortfall exceeding 2/3 (12 > 17*2/3) must step back to row 1 (charpos 10)"
    );
}

#[test]
fn tall_point_row_recenter_uses_nlines_jump_and_forward_overshoot_loop() {
    use crate::display_row::walk_state::{
        TallPointRowScrollContext, next_window_start_for_tall_partially_visible_point_row,
    };
    use neovm_core::window::ScrollMargin;

    // Row 2 is a 60 px heading (2/3 * 60 = 40); normal rows are 17 px; tall
    // point row is 200 px at anchor = 4 (charpos 40, y = 111).
    // body = 110 -> dy = 55.
    // - GNU move_it_vertically_backward (src/xdisp.c:11265-11432) jumps back
    //   nlines = max(1, 55 / 17) = 3 rows to idx = 1 (y = 17, moved = 94,
    //   target_y = 56 >= 17 + 17), then steps forward through row 2 (y = 34 <= 56)
    //   and stops before row 3 (y = 94 > 56), landing on idx = 2 (charpos 20).
    // - A naive 1-row-at-a-time loop stops after row 3 (remaining 38 < 40) at
    //   idx = 3 (charpos 30).
    let rows = tall_row_snapshots_with_heights(&[17, 17, 60, 17, 200]);
    let ctx = TallPointRowScrollContext {
        default_line_height: 17,
        is_minibuffer: false,
        has_vscroll: false,
        force_start: false,
        temp_scroll_step: false,
        already_scrolled_this_redisplay: false,
    };

    assert_eq!(
        next_window_start_for_tall_partially_visible_point_row(
            &rows,
            40,
            0,
            110,
            0,
            ScrollPolicy::Recenter,
            ScrollMargin::default(),
            ctx,
        ),
        Some(20),
        "nlines jump + forward overshoot loop must land on row 2 (charpos 20)"
    );
}

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
        TallPointRowScrollContext, TallPointRowScrollTarget,
        next_window_start_for_tall_partially_visible_point_row,
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
        scroll_step: 0,
        make_cursor_line_fully_visible: true,
        is_minibuffer: false,
        has_vscroll: false,
        force_start: false,
        temp_scroll_step: true,
        already_scrolled_this_redisplay: false,
        initial_extra_scroll_margin_lines: 0,
        point_row_prefix_height: None,
        point_row_ends_at_eob_without_newline: false,
    };
    let no_margin = ScrollMargin::default();

    // 8-line window (body = 119 px = 7*17), tall row = 164 px > 119.
    // Line indices are 0-based layout charpos: line k starts at (k - 1) * 10.
    let run = |normal_count: usize,
               body_h: i64,
               tall_h: i64,
               policy: ScrollPolicy,
               margin: ScrollMargin,
               ctx: TallPointRowScrollContext| {
        let rows = make_rows(normal_count, 17, tall_h);
        let point = (normal_count as i64) * 10;
        next_window_start_for_tall_partially_visible_point_row(
            &rows, point, 0, body_h, 0, policy, margin, ctx,
        )
    };
    let ws = |pos: i64| Some(TallPointRowScrollTarget::WindowStart(pos));

    // Default policy (sc=0, ss=0), cursor-only (temp_scroll_step=true):
    // line 2 -> start=line 2 (10); line 3 -> start=line 3 (20); line 6 -> recenter start=line 3 (20).
    assert_eq!(
        run(1, 119, 164, ScrollPolicy::Recenter, no_margin, base_ctx),
        ws(10)
    );
    assert_eq!(
        run(2, 119, 164, ScrollPolicy::Recenter, no_margin, base_ctx),
        ws(20)
    );
    assert_eq!(
        run(5, 119, 164, ScrollPolicy::Recenter, no_margin, base_ctx),
        ws(20)
    );

    // Non-cursor-only edit (temp_scroll_step=false) skips try_scrolling -> recenter keeps line 1.
    let edit_ctx = TallPointRowScrollContext {
        temp_scroll_step: false,
        ..base_ctx
    };
    assert_eq!(
        run(1, 119, 164, ScrollPolicy::Recenter, no_margin, edit_ctx),
        None
    );
    // F1.2: initial extra=0 even on non-cursor-only edit (try_window CHECK_MARGINS returns -1).
    assert_eq!(
        run(
            2,
            119,
            164,
            ScrollPolicy::Conservative { max_lines: 1 },
            no_margin,
            TallPointRowScrollContext {
                point_row_prefix_height: Some(17),
                ..edit_ctx
            },
        ),
        ws(20),
        "edit-l3-sc1: extra starts at 0 and reaches line 3"
    );
    assert_eq!(
        run(5, 119, 164, ScrollPolicy::Unlimited, no_margin, edit_ctx),
        ws(50)
    );
    assert_eq!(
        run(
            9,
            255,
            293,
            ScrollPolicy::Unlimited,
            no_margin,
            TallPointRowScrollContext {
                point_row_prefix_height: Some(17),
                ..edit_ctx
            },
        ),
        ws(60),
        "w16-edit-l10-sc101: extra=0,1,2 -> 1+2+3=6 -> line 7"
    );

    // F1.1: non-BOL PT after tall glyph uses prefix_height (164/293), failing try_scrolling -> recenter to 1.
    assert_eq!(
        run(
            1,
            119,
            164,
            ScrollPolicy::Recenter,
            no_margin,
            TallPointRowScrollContext {
                point_row_prefix_height: Some(164),
                ..base_ctx
            },
        ),
        None,
        "col3-def-l2: dy=164 > 17 -> recenter keeps line 1"
    );
    assert_eq!(
        run(
            9,
            255,
            293,
            ScrollPolicy::Unlimited,
            no_margin,
            TallPointRowScrollContext {
                point_row_prefix_height: Some(293),
                ..base_ctx
            },
        ),
        None,
        "col3-w16-sc101-l10: dy=293 overshoots past line 10 -> recenter keeps line 1"
    );

    // F1.3: overshoot past PT must fail into recenter (no clamping at point_row).
    assert_eq!(
        run(
            2,
            119,
            164,
            ScrollPolicy::Step { lines: 20 },
            no_margin,
            base_ctx
        ),
        None,
        "ss20-l3: scroll-step 20 overshoots past line 3 -> recenter keeps line 1"
    );
    let margin_2 = ScrollMargin::from_lisp_values(2, None);
    assert_eq!(
        run(9, 255, 293, ScrollPolicy::Unlimited, margin_2, base_ctx),
        None,
        "m2-w16-sc101-l10: persistent dy=68 overshoots past line 10 -> recenter keeps line 1"
    );
    let margin_1 = ScrollMargin::from_lisp_values(1, None);
    assert_eq!(
        run(9, 255, 293, ScrollPolicy::Unlimited, margin_1, base_ctx),
        ws(90),
        "m1-w16-sc101-l10: lands exactly on line 10"
    );

    // F1.4: tall row at ZV without trailing newline (IT_EOB partial_line_height = 0).
    assert_eq!(
        run(
            1,
            119,
            164,
            ScrollPolicy::Recenter,
            no_margin,
            TallPointRowScrollContext {
                point_row_ends_at_eob_without_newline: true,
                ..base_ctx
            },
        ),
        None,
        "tall-at-zv-l2: untruncated EOB keeps line 1"
    );

    // F1.5: exact-height boundary (row.height == body_h) on cursor-only stays at line 1.
    assert_eq!(
        run(1, 119, 119, ScrollPolicy::Recenter, no_margin, base_ctx),
        None,
        "eqh-119-cursor-only: exact body height on cursor-only does not scroll"
    );

    // F4.1: scroll-conservatively + larger scroll-step (scroll_max uses max(ss, sc, temp)).
    assert_eq!(
        run(
            6,
            119,
            164,
            ScrollPolicy::Conservative { max_lines: 2 },
            no_margin,
            TallPointRowScrollContext {
                scroll_step: 5,
                ..base_ctx
            },
        ),
        ws(50),
        "sc2-ss5-l7: dy=51 <= 85 (5*17), conservative cap=34 (2*17) -> line 6"
    );

    // F4.2: make-cursor-line-fully-visible = nil suppresses scrolling.
    assert_eq!(
        run(
            1,
            119,
            164,
            ScrollPolicy::Recenter,
            no_margin,
            TallPointRowScrollContext {
                make_cursor_line_fully_visible: false,
                ..base_ctx
            },
        ),
        None,
        "mclfv-nil-l2: make-cursor-line-fully-visible nil suppresses scroll"
    );

    // Policy boundaries on 8-line window:
    assert_eq!(
        run(
            5,
            119,
            164,
            ScrollPolicy::Conservative { max_lines: 1 },
            no_margin,
            base_ctx
        ),
        ws(20)
    );
    assert_eq!(
        run(
            5,
            119,
            164,
            ScrollPolicy::Conservative { max_lines: 2 },
            no_margin,
            base_ctx
        ),
        ws(50)
    );
    assert_eq!(
        run(
            6,
            119,
            164,
            ScrollPolicy::Conservative { max_lines: 2 },
            no_margin,
            base_ctx
        ),
        ws(30)
    );
    assert_eq!(
        run(
            6,
            119,
            164,
            ScrollPolicy::Step { lines: 2 },
            no_margin,
            base_ctx
        ),
        ws(60)
    );
    assert_eq!(
        run(5, 119, 164, ScrollPolicy::Unlimited, no_margin, base_ctx),
        ws(50)
    );

    // 16-line window (body = 255 px = 15*17), tall row at line 10 (295 px > 255):
    // sc=101 stops at extra > 1 after 1+2+3=6 lines (start=line 7 -> 60).
    // default recenter steps back body/2 (7 lines above -> start=line 3 -> 20).
    assert_eq!(
        run(9, 255, 295, ScrollPolicy::Unlimited, no_margin, base_ctx),
        ws(60)
    );
    assert_eq!(
        run(9, 255, 295, ScrollPolicy::Recenter, no_margin, base_ctx),
        ws(20)
    );

    // GNU cursor_row_fully_visible_p suppression boundaries:
    assert_eq!(
        run(0, 119, 164, ScrollPolicy::Unlimited, no_margin, base_ctx),
        None
    );
    assert_eq!(
        run(
            5,
            119,
            164,
            ScrollPolicy::Unlimited,
            no_margin,
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
            no_margin,
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
            no_margin,
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
            no_margin,
            TallPointRowScrollContext {
                already_scrolled_this_redisplay: true,
                ..base_ctx
            }
        ),
        None
    );
}

#[test]
fn tall_point_row_under_header_line_recenters_backward() {
    use crate::display_row::walk_state::{
        TallPointRowScrollContext, TallPointRowScrollTarget,
        next_window_start_for_tall_partially_visible_point_row,
    };
    use neovm_core::buffer::LispCharPos1;
    use neovm_core::window::{DisplayRowEndSource, DisplayRowSnapshot, ScrollMargin};

    // F4.3: under a header-line, the first text body row has window-absolute
    // `row == 1` (`w->cursor.vpos == 1 != 0`). With body_h = 102 (6 * 17),
    // recenter steps back 51 px = 3 lines above `current_start`.
    let rows = vec![DisplayRowSnapshot {
        row: 1,
        y: 17,
        height: 164,
        start_buffer_pos: Some(LispCharPos1::new(41)),
        end_buffer_pos: Some(LispCharPos1::new(49)),
        end_source: DisplayRowEndSource::Buffer,
        ..DisplayRowSnapshot::default()
    }];
    let ctx = TallPointRowScrollContext {
        default_line_height: 17,
        scroll_step: 0,
        make_cursor_line_fully_visible: true,
        is_minibuffer: false,
        has_vscroll: false,
        force_start: false,
        temp_scroll_step: false,
        already_scrolled_this_redisplay: false,
        initial_extra_scroll_margin_lines: 0,
        point_row_prefix_height: Some(17),
        point_row_ends_at_eob_without_newline: false,
    };
    assert_eq!(
        next_window_start_for_tall_partially_visible_point_row(
            &rows,
            40,
            17,
            119,
            40,
            ScrollPolicy::Recenter,
            ScrollMargin::default(),
            ctx,
        ),
        Some(TallPointRowScrollTarget::LinesBeforeCurrentStart(3)),
        "hdr-start5-edit: first body row under header-line (vpos=1) recenters 3 lines back"
    );

    // R2-4(a): With sc > 0 or ss > 0 on body row 0 under a header-line, GNU
    // enters `try_scrolling`, finds `to_y < 0` with `scroll_down_p = false` for
    // extra = 0..=2, and accepts `startp` unchanged at extra > 1 (no recenter).
    for (label, policy, step) in [
        (
            "hdr-start5-edit-sc1",
            ScrollPolicy::Conservative { max_lines: 1 },
            0,
        ),
        ("hdr-start5-edit-sc101", ScrollPolicy::Unlimited, 0),
        ("hdr-start5-edit-ss2", ScrollPolicy::Step { lines: 2 }, 2),
    ] {
        let mut sc_ctx = ctx;
        sc_ctx.scroll_step = step;
        assert_eq!(
            next_window_start_for_tall_partially_visible_point_row(
                &rows,
                40,
                17,
                119,
                40,
                policy,
                ScrollMargin::default(),
                sc_ctx,
            ),
            None,
            "{label}: try_scrolling succeeds at extra > 1 with start unchanged"
        );
    }

    // R2-4(a): Under a header-line (`rows[0].row == 1`), reaching
    // `start_idx == point_row_index` at `extra <= 1` does NOT stop early
    // because window-absolute vpos is 1 != 0: on line 4 with sc=101,
    // iter 0: 1->2, iter 1: 2->4 (still vpos=1!), iter 2 overshoots past PT
    // into recenter -> start=1 (None). On line 6, iter 2 lands on 6 at extra=2.
    let hdr_rows_l4: Vec<DisplayRowSnapshot> = (0..4)
        .map(|i| DisplayRowSnapshot {
            row: i + 1,
            y: 17 + i * 17,
            height: if i == 3 { 164 } else { 17 },
            start_buffer_pos: Some(LispCharPos1::new(i * 10 + 1)),
            end_buffer_pos: Some(LispCharPos1::new(i * 10 + 9)),
            end_source: DisplayRowEndSource::Buffer,
            ..DisplayRowSnapshot::default()
        })
        .collect();
    assert_eq!(
        next_window_start_for_tall_partially_visible_point_row(
            &hdr_rows_l4,
            30,
            17,
            119,
            0,
            ScrollPolicy::Unlimited,
            ScrollMargin::default(),
            TallPointRowScrollContext {
                temp_scroll_step: true,
                point_row_prefix_height: None,
                ..ctx
            },
        ),
        None,
        "hdr-l4-sc101: reaching body row 0 under header-line at extra=1 overshoots at extra=2 -> recenter start=1"
    );
    let hdr_rows_l6: Vec<DisplayRowSnapshot> = (0..6)
        .map(|i| DisplayRowSnapshot {
            row: i + 1,
            y: 17 + i * 17,
            height: if i == 5 { 164 } else { 17 },
            start_buffer_pos: Some(LispCharPos1::new(i * 10 + 1)),
            end_buffer_pos: Some(LispCharPos1::new(i * 10 + 9)),
            end_source: DisplayRowEndSource::Buffer,
            ..DisplayRowSnapshot::default()
        })
        .collect();
    assert_eq!(
        next_window_start_for_tall_partially_visible_point_row(
            &hdr_rows_l6,
            50,
            17,
            119,
            0,
            ScrollPolicy::Unlimited,
            ScrollMargin::default(),
            TallPointRowScrollContext {
                temp_scroll_step: true,
                point_row_prefix_height: None,
                ..ctx
            },
        ),
        Some(TallPointRowScrollTarget::WindowStart(50)),
        "hdr-l6-sc101: lands on line 6 at extra=2 -> succeeds at line 6"
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
        TallPointRowScrollContext, TallPointRowScrollTarget,
        next_window_start_for_tall_partially_visible_point_row,
    };
    use neovm_core::window::ScrollMargin;

    // 3 normal rows of height 17 (2/3 * 17 = 11) followed by a tall point row
    // of height 100 at anchor = 3 (charpos 30). Non-cursor-only edit skips
    // try_scrolling and enters GNU's recenter: fallback (src/xdisp.c:11391-11392).
    let rows = tall_row_snapshots_with_heights(&[17, 17, 17, 100]);
    let ctx = TallPointRowScrollContext {
        default_line_height: 17,
        scroll_step: 0,
        make_cursor_line_fully_visible: true,
        is_minibuffer: false,
        has_vscroll: false,
        force_start: false,
        temp_scroll_step: false,
        already_scrolled_this_redisplay: false,
        initial_extra_scroll_margin_lines: 0,
        point_row_prefix_height: None,
        point_row_ends_at_eob_without_newline: false,
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
        Some(TallPointRowScrollTarget::WindowStart(20)),
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
        Some(TallPointRowScrollTarget::WindowStart(10)),
        "shortfall exceeding 2/3 (12 > 17*2/3) must step back to row 1 (charpos 10)"
    );
}

#[test]
fn tall_point_row_recenter_uses_nlines_jump_and_forward_overshoot_loop() {
    use crate::display_row::walk_state::{
        TallPointRowScrollContext, TallPointRowScrollTarget,
        next_window_start_for_tall_partially_visible_point_row,
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
        scroll_step: 0,
        make_cursor_line_fully_visible: true,
        is_minibuffer: false,
        has_vscroll: false,
        force_start: false,
        temp_scroll_step: false,
        already_scrolled_this_redisplay: false,
        initial_extra_scroll_margin_lines: 0,
        point_row_prefix_height: None,
        point_row_ends_at_eob_without_newline: false,
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
        Some(TallPointRowScrollTarget::WindowStart(20)),
        "nlines jump + forward overshoot loop must land on row 2 (charpos 20)"
    );
}

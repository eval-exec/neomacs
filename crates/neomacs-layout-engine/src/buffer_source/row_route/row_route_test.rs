use super::*;
use crate::buffer_source::text_source::BufferTextSourceCursor;
use crate::display_source::{DisplayItemSource, DisplaySourceContext};
use neovm_core::buffer::CharLen;
use neovm_core::emacs_core::Context;

fn buffer_with_text(eval: &mut Context, text: &str) -> BufferId {
    let buf_id = eval.buffer_manager_mut().create_buffer("*row-route*");
    eval.buffer_manager_mut()
        .get_mut(buf_id)
        .expect("buffer")
        .insert(text);
    buf_id
}

fn plain_policy() -> RowRouteWindowPolicy {
    RowRouteWindowPolicy {
        // Far outside any test row.
        point_charpos: 1_000,
        hscroll_active: false,
        selective_display: 0,
        word_wrap: false,
        show_trailing_whitespace: false,
        wrap_mode: LineWrapMode::Truncate,
        overlay_string_window: Some(0),
    }
}

fn wrap_policy() -> RowRouteWindowPolicy {
    RowRouteWindowPolicy {
        wrap_mode: LineWrapMode::Wrap,
        ..plain_policy()
    }
}

fn row_start(text: &[u8], byte_idx: usize, charpos: i64) -> RowRouteRowStart<'_> {
    RowRouteRowStart {
        text,
        byte_idx,
        charpos,
        text_start_byte: 0,
    }
}

static TAB_EVERY_8: std::sync::LazyLock<DisplayTabPolicy> =
    std::sync::LazyLock::new(|| DisplayTabPolicy::every(8));

fn wide_fit() -> RowRouteFit<'static> {
    fit_to(640.0)
}

fn fit_to(right_edge_px: f32) -> RowRouteFit<'static> {
    RowRouteFit {
        start_position: DisplayRowPosition::new(0.0, 0),
        char_width_px: 8.0,
        right_edge_px,
        tab_policy: &TAB_EVERY_8,
    }
}

fn classify_in_buffer(
    eval: &Context,
    buf_id: BufferId,
    row: RowRouteRowStart<'_>,
    fit: RowRouteFit<'_>,
    policy: RowRouteWindowPolicy,
) -> RowAcquisitionRoute {
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    RowAcquisitionRoute::of(&plan_plain_row_classified(buffer, row, fit, policy))
}

#[test]
fn classifier_routes_plain_ascii_row_to_item_renderer() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello world\n");
    let text = b"hello world\n";
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text, 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
}

#[test]
fn classifier_routes_trailing_whitespace_row_when_highlight_off() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "ab  \n");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(b"ab  \n", 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
}

#[test]
fn classifier_routes_tab_and_wide_char_rows() {
    let mut eval = Context::new();
    // Tabs, narrow non-ASCII (e-acute), and wide CJK chars all route since
    // the phase 2b extension.
    for text in [
        "a\tb\n",
        "\t\tindent\n",
        "h\u{00e9}llo\n",
        "ab\u{4E2D}\u{6587}cd\n",
        "a\t\u{4E2D}b\n",
    ] {
        let buf_id = buffer_with_text(&mut eval, text);
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::ItemRenderer,
            "content {text:?} must route to the item renderer"
        );
    }
}

#[test]
fn plan_reports_tab_wide_flags_and_char_byte_lengths() {
    let mut eval = Context::new();
    let text = "a\t\u{4E2D}b\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("tab+wide row routes");
    assert_eq!(plan.line_char_len(), 4);
    assert_eq!(plan.line_byte_len(), 6, "one 3-byte CJK char");
    assert!(plan.has_tab());
    assert!(plan.has_wide());

    // A plain ASCII row classifies without either flag.
    let buf_id = buffer_with_text(&mut eval, "ab\n");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plain =
        plan_plain_row_classified(buffer, row_start(b"ab\n", 0, 0), wide_fit(), plain_policy())
            .expect("plain row routes");
    assert!(!plain.has_tab());
    assert!(!plain.has_wide());
}

#[test]
fn plan_face_boundaries_are_char_offsets_on_multibyte_rows() {
    let mut eval = Context::new();
    // "a e-acute CJK b": 4 chars, 7 bytes. A face span over chars 2..4
    // (1-based [2, 4) = e-acute + CJK) must split at CHAR offsets 1 and 3.
    let text = "a\u{00E9}\u{4E2D}b\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 2 4 'face 'bold)")
        .expect("put-text-property");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("multibyte faced row routes");
    assert_eq!(plan.line_char_len(), 4);
    assert_eq!(plan.line_byte_len(), 7);
    assert_eq!(plan.face_boundaries(), &[1, 3]);
    assert_eq!(
        plan.segment_ranges(CharPos0::ZERO),
        vec![
            (CharPos0::ZERO, CharPos0::new(1)),
            (CharPos0::new(1), CharPos0::new(3)),
            (CharPos0::new(3), CharPos0::new(4)),
        ]
    );
}

#[test]
fn classifier_rejects_tab_line_exactly_filling_the_row() {
    let mut eval = Context::new();
    // "ab\t": the tab expands from col 2 to the col-8 stop, 64px at 8px
    // cells. A 64px row is exact fill — refused; one cell of slack routes.
    let buf_id = buffer_with_text(&mut eval, "ab\t\n");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(b"ab\t\n", 0, 0),
            fit_to(64.0),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "tab expansion landing exactly on the right edge must refuse"
    );
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(b"ab\t\n", 0, 0),
            fit_to(72.0),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
}

#[test]
fn classifier_fit_advances_a_full_stop_for_tab_exactly_on_a_stop() {
    let mut eval = Context::new();
    // GNU next_tab_x (xdisp.c gui_produce_glyphs): the +1 in
    // ((1 + x + tab_width - 1) / tab_width) * tab_width forces a tab landing
    // EXACTLY on a stop to advance a FULL stop. "abcdefgh\t": the tab starts
    // exactly on the col-8 stop, so the line ends at col 16 (128px at 8px
    // cells) — a 16-cell row is exact fill (refused), 17 cells route.
    let buf_id = buffer_with_text(&mut eval, "abcdefgh\t\n");
    let text = b"abcdefgh\t\n";
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text, 0, 0),
            fit_to(128.0),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "a tab exactly on a stop must expand a full stop (to col 16), exact fill refuses"
    );
    // If the +1 rule were broken (tab advancing zero or one cell), the line
    // would end well inside 12 cells and wrongly plan as a whole-line route.
    // With the correct rule the full-stop expansion crosses the 12-cell row,
    // so phase 2f plans the 8-char prefix and hands the edge-crossing tab
    // back to the pipeline (which clips it, GNU xdisp.c:26390).
    {
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        let plan =
            plan_plain_row_classified(buffer, row_start(text, 0, 0), fit_to(96.0), plain_policy())
                .expect("overflow prefix plan");
        assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
        assert_eq!(plan.line_char_len(), 8, "prefix ends BEFORE the tab");
        assert!(!plan.has_tab(), "the edge-crossing tab is not routed");
    }
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text, 0, 0),
            fit_to(136.0),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
}

#[test]
fn classifier_rejects_wide_char_exact_fill_and_straddle() {
    let mut eval = Context::new();
    // "abc" + CJK = 5 cols = 40px at 8px cells.
    let text = "abc\u{4E2D}\n";
    let buf_id = buffer_with_text(&mut eval, text);
    for (edge, expected) in [
        (40.0, RowAcquisitionRoute::BufferPipeline), // exact fill
        (48.0, RowAcquisitionRoute::ItemRenderer),   // one cell of slack
    ] {
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                fit_to(edge),
                plain_policy()
            ),
            expected,
            "edge {edge}px"
        );
    }
    // Phase 2f: the wide char STRADDLING the edge (x=24, +16 crosses a 36px
    // row) no longer refuses the whole row — the fitting "abc" prefix routes
    // and the straddling char hands off to the pipeline's overflow machinery
    // (GNU consumes it into the truncation skip / pushes it to the next row).
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        fit_to(36.0),
        plain_policy(),
    )
    .expect("straddle prefix plan");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert_eq!(plan.line_char_len(), 3);
    assert_eq!(plan.line_byte_len(), 3);
    assert!(!plan.has_wide(), "the straddling wide char is not routed");
}

#[test]
fn classifier_rejects_content_the_item_route_does_not_cover() {
    let mut eval = Context::new();
    // Control chars, zero-width chars, complex scripts, regional-indicator
    // pairs, and nobreak space/hyphen (nobreak-char-display consults a
    // setting): all stay on the buffer pipeline. (Combining marks on a
    // simple base left this list in phase 2e rung 2 — see the
    // composed-cluster tests; the missing-final-newline EOB tail and the
    // empty line left it in phase 2h — see the end-of-source and
    // empty-line tests.)
    for text in [
        b"a\x01b\n".as_slice(),
        b"a\rb\n".as_slice(),
        "a\u{200B}b\n".as_bytes(),                       // zero-width space
        "a\u{200D}b\n".as_bytes(),                       // ZWJ
        "\u{0633}\u{0644}\u{0627}\u{0645}\n".as_bytes(), // Arabic (shaped run)
        "\u{1F1E6}\u{1F1E9}\n".as_bytes(),               // regional-indicator flag pair
        "a\u{00A0}b\n".as_bytes(),                       // no-break space
        "a\u{00AD}b\n".as_bytes(),                       // soft hyphen
        "a\u{0080}b\n".as_bytes(),                       // C1 control (octal escape)
    ] {
        let buf_id = buffer_with_text(&mut eval, std::str::from_utf8(text).unwrap());
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text, 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::BufferPipeline,
            "content {:?} must stay on the buffer pipeline",
            String::from_utf8_lossy(text)
        );
    }
}

/// P4.8(a): a mid-line start is classified like any other position. The
/// entry taxonomy is gone, so a position the walk reaches in the middle of a
/// line is planned from its own charpos and the live pen — there is nothing
/// about being mid-line that the classifier needs to refuse.
#[test]
fn classifier_routes_mid_line_start() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "abc\n");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(b"abc\n", 1, 1),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
}

/// P4.8(a) hazard pin, and it is a PREDICTION that had to be verified rather
/// than reasoned about: the entry gate used to refuse the overflow handoff
/// char on the FIRST row of a continued line, whose row is not flagged
/// Continuation yet, so that the pipeline's own overflow machinery would keep
/// consuming it. With the gate gone that position attempts the route, and
/// what must keep it out is the FIT walk, not the entry taxonomy — the pen
/// already sits at the right edge, so no fitting prefix exists.
///
/// The refusal reason changes (MidLineStart to ScanNoFitFirstChar); the
/// behaviour must not.
#[test]
fn classifier_refuses_a_mid_line_start_whose_first_char_does_not_fit() {
    let mut eval = Context::new();
    let text = "abcdef\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    // The pen stands ON the right edge: the handoff char cannot fit.
    let fit = RowRouteFit {
        start_position: DisplayRowPosition::new(24.0, 3),
        char_width_px: 8.0,
        right_edge_px: 24.0,
        tab_policy: &TAB_EVERY_8,
    };
    assert_eq!(
        plan_plain_row_classified(buffer, row_start(text.as_bytes(), 3, 3), fit, wrap_policy())
            .unwrap_err(),
        RouteRefusal::ScanNoFitFirstChar
    );
}

/// P4.8(a): a mid-line tail — the remainder of a visually wrapped line —
/// plans from its own charpos with no attestation from the walk. Before the
/// entry unification this position needed the continuation-resume entry to
/// be classified at all.
#[test]
fn mid_line_tail_plans_from_its_own_charpos() {
    let mut eval = Context::new();
    let text = "abcdef\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let row = row_start(text.as_bytes(), 3, 3);
    let plan = plan_plain_row_classified(buffer, row, wide_fit(), wrap_policy())
        .expect("mid-line tail plans");
    assert_eq!(plan.line_char_len(), 3);
    assert_eq!(plan.line_byte_len(), 3);
    assert_eq!(plan.line_end(), RoutedRowLineEnd::Newline);
}

/// The resume fit walk starts from the CARRIED pen (x, col), so a tab in the
/// tail expands from the live pen exactly as the pipeline's own
/// `DisplayTabPolicy::advance_from` would — the tab-after-wrap acid case:
/// the tab's width depends on where the resumed walk actually stands.
#[test]
fn continuation_resume_tab_expands_from_carried_column() {
    let mut eval = Context::new();
    let text = "abc\txy\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let row = row_start(text.as_bytes(), 3, 3);
    let carried = RowRouteFit {
        start_position: DisplayRowPosition::new(24.0, 3),
        char_width_px: 8.0,
        right_edge_px: 72.0,
        tab_policy: &TAB_EVERY_8,
    };
    let plan = plan_plain_row_classified(buffer, row, carried, wrap_policy())
        .expect("carried tab tail plans");
    assert!(plan.has_tab());
    // tab (24px -> the 64px stop) + 'x' (64 -> 72, landing AT the edge) fit;
    // 'y' (72 -> 80) crosses, so the plan covers only the fitting prefix and
    // hands the walk back to the pipeline at 'y'.
    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert_eq!(plan.line_char_len(), 2);
    // With enough slack the whole tail fits strictly inside the edge.
    let wider = RowRouteFit {
        right_edge_px: 96.0,
        ..carried
    };
    let plan = plan_plain_row_classified(buffer, row, wider, wrap_policy())
        .expect("wider carried tab tail plans");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::Newline);
    assert_eq!(plan.line_char_len(), 3);
}

#[test]
fn continuation_route_preserves_the_physical_line_tab_grid() {
    let mut eval = Context::new();
    let text = "abc\txy\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let row = row_start(text.as_bytes(), 3, 3);
    let mut physical_line = crate::display_row::builder::DisplayPhysicalLineTabState::default();
    physical_line.continue_after_visual_row(24.0);
    let fit = RowRouteFit {
        start_position: DisplayRowPosition::new(24.0, 3)
            .with_tab_coordinates(physical_line.coordinates()),
        char_width_px: 8.0,
        right_edge_px: 48.0,
        tab_policy: &TAB_EVERY_8,
    };

    let plan = plan_plain_row_classified(buffer, row, fit, wrap_policy())
        .expect("the physical-grid tab and one following char fit");

    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert_eq!(plan.line_char_len(), 2);
}

/// Cursor capture stays a buffer-pipeline responsibility on the resume entry
/// too: point inside the resumed tail refuses.
#[test]
fn continuation_resume_still_refuses_point_in_tail() {
    let mut eval = Context::new();
    let text = "abcdef\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let policy = RowRouteWindowPolicy {
        point_charpos: 4,
        ..wrap_policy()
    };
    assert_eq!(
        plan_plain_row_classified(buffer, row_start(text.as_bytes(), 3, 3), wide_fit(), policy,)
            .unwrap_err(),
        RouteRefusal::PointInRow
    );
}

/// A wrap that lands exactly on the line end resumes AT the newline: the
/// resume plan is the RowBreak-only empty coverage (the shared line-end plan
/// consumes the newline), mirroring the pipeline rendering the line end on
/// the continuation row.
#[test]
fn continuation_resume_at_newline_plans_row_break_only() {
    let mut eval = Context::new();
    let text = "abc\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 3, 3),
        wide_fit(),
        wrap_policy(),
    )
    .expect("newline resume plans");
    assert!(plan.is_empty_line());
    assert_eq!(plan.line_end(), RoutedRowLineEnd::Newline);
}

#[test]
fn classifier_rejects_line_exactly_filling_the_row() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "abcd\n");
    // 4 chars * 8px == the full 32px row: exact fill is NOT eligible.
    let exact = fit_to(32.0);
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(b"abcd\n", 0, 0),
            exact,
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );
    // One cell of slack routes.
    let slack = fit_to(40.0);
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(b"abcd\n", 0, 0),
            slack,
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
}

#[test]
fn classifier_rejects_rows_containing_point() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "abc\nxyz\n");
    let text = b"abc\nxyz\n";
    for point in 0..=3 {
        let policy = RowRouteWindowPolicy {
            point_charpos: point,
            ..plain_policy()
        };
        assert_eq!(
            classify_in_buffer(&eval, buf_id, row_start(text, 0, 0), wide_fit(), policy),
            RowAcquisitionRoute::BufferPipeline,
            "point {point} lies on the row (incl. its newline)"
        );
    }
    // Point on the NEXT line does not disqualify this row.
    let policy = RowRouteWindowPolicy {
        point_charpos: 4,
        ..plain_policy()
    };
    assert_eq!(
        classify_in_buffer(&eval, buf_id, row_start(text, 0, 0), wide_fit(), policy),
        RowAcquisitionRoute::ItemRenderer
    );
}

#[test]
fn classifier_rejects_window_policy_features() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "abc\n");
    let text = b"abc\n";
    let policies = [
        RowRouteWindowPolicy {
            hscroll_active: true,
            ..plain_policy()
        },
        RowRouteWindowPolicy {
            selective_display: 2,
            ..plain_policy()
        },
        RowRouteWindowPolicy {
            word_wrap: true,
            ..plain_policy()
        },
        RowRouteWindowPolicy {
            show_trailing_whitespace: true,
            ..plain_policy()
        },
    ];
    for policy in policies {
        assert_eq!(
            classify_in_buffer(&eval, buf_id, row_start(text, 0, 0), wide_fit(), policy),
            RowAcquisitionRoute::BufferPipeline,
            "policy {policy:?} must stay on the buffer pipeline"
        );
    }
}

#[test]
fn classifier_accepts_face_property_span_and_plans_boundaries() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\nworld\n");
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 5 'face 'bold)")
        .expect("put-text-property");
    let text = b"hello\nworld\n";
    // A face span mid-line routes and segments the row at each property
    // change ("he" / "ll" bold / "o").
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(buffer, row_start(text, 0, 0), wide_fit(), plain_policy())
        .expect("face-propped row routes");
    assert_eq!(plan.line_char_len(), 5);
    assert_eq!(plan.line_byte_len(), 5);
    assert_eq!(plan.face_boundaries(), &[2, 4]);
    assert!(plan.is_segmented());
    assert_eq!(
        plan.segment_ranges(CharPos0::ZERO),
        vec![
            (CharPos0::ZERO, CharPos0::new(2)),
            (CharPos0::new(2), CharPos0::new(4)),
            (CharPos0::new(4), CharPos0::new(5)),
        ]
    );
    // The second, unfaced row routes unsegmented.
    let plan = plan_plain_row_classified(buffer, row_start(text, 6, 6), wide_fit(), plain_policy())
        .expect("unfaced row routes");
    assert!(!plan.is_segmented());
}

#[test]
fn classifier_accepts_font_lock_face_and_whole_line_span() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "keyword\n");
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 1 8 'font-lock-face 'bold)")
        .expect("put-text-property");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(b"keyword\n", 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("font-lock-faced row routes");
    // The property covers the whole line but ends before the newline: the
    // change on the newline byte is not a text-segment boundary.
    assert_eq!(plan.face_boundaries(), &[] as &[usize]);
}

#[test]
fn classifier_accepts_fontified_boundary_as_segment_split() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "abcd\n");
    eval.buffer_manager_mut().set_current(buf_id);
    // A non-face property change (fontified) still splits the run, exactly
    // like GNU compute_stop_pos stops at EVERY property change.
    eval.eval_str("(put-text-property 1 3 'fontified t)")
        .expect("put-text-property");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(b"abcd\n", 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("fontified-bounded row routes");
    assert_eq!(plan.face_boundaries(), &[2]);
}

#[test]
fn classifier_rejects_hazard_properties_anywhere_on_the_line() {
    // `invisible` left this list in phase 2d: the plain-elision sub-case is
    // routed (see the elision tests below); ellipsis / newline-spanning /
    // row-start invisibility still refuse through the elision scan.
    // `composition` left this list in phase 2e: the refusal is now grounded
    // in the pipeline's own replacement predicate (see the composition tests
    // below). `display` left this list in increment 2i: routable string
    // replacements route (see the replacement tests below) and every other
    // display shape refuses through the dedicated replacement scan.
    for (prop, value) in [("mouse-face", "'highlight"), ("line-height", "2.0")] {
        let mut eval = Context::new();
        let buf_id = buffer_with_text(&mut eval, "hello\n");
        eval.buffer_manager_mut().set_current(buf_id);
        eval.eval_str(&format!("(put-text-property 3 5 '{prop} {value})"))
            .expect("put-text-property");
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(b"hello\n", 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::BufferPipeline,
            "mid-line {prop} must stay on the buffer pipeline"
        );
    }
}

#[test]
fn classifier_rejects_hazard_property_on_the_newline() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\nx\n");
    eval.buffer_manager_mut().set_current(buf_id);
    // A display property covering ONLY the newline would replace the line
    // end; the hazard probe must reach it.
    eval.eval_str("(put-text-property 6 7 'display \"|\")")
        .expect("put-text-property");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(b"hello\nx\n", 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );
}

#[test]
fn classifier_accepts_face_only_overlay_and_plans_boundaries() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\n");
    eval.buffer_manager_mut().set_current(buf_id);
    // Overlay over "el" (elisp 2..4) carrying only face-affecting props.
    eval.eval_str(
        "(let ((ov (make-overlay 2 4))) \
           (overlay-put ov 'face 'bold) \
           (overlay-put ov 'priority 5) \
           (overlay-put ov 'evaporate t))",
    )
    .expect("face-only overlay");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(b"hello\n", 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("face-only overlay row routes");
    // The overlay's start and end are face-segment boundaries, the neomacs
    // mirror of GNU compute_stop_pos folding next_overlay_change into
    // stop_charpos.
    assert_eq!(plan.face_boundaries(), &[1, 3]);
    assert!(plan.is_segmented());
}

#[test]
fn classifier_merges_overlay_and_text_prop_boundaries() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\n");
    eval.buffer_manager_mut().set_current(buf_id);
    // Text face over "he" plus overlapping overlays: boundaries merge,
    // sort, and dedupe into one ascending char-offset list.
    eval.eval_str(
        "(progn (put-text-property 1 3 'face 'bold) \
                (overlay-put (make-overlay 3 5) 'face 'highlight) \
                (overlay-put (make-overlay 2 5) 'face 'underline))",
    )
    .expect("overlapping overlays over a text span");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(b"hello\n", 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("face-only overlays route");
    assert_eq!(plan.face_boundaries(), &[1, 2, 4]);
}

#[test]
fn classifier_accepts_zero_length_face_only_overlay() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\n");
    eval.buffer_manager_mut().set_current(buf_id);
    // GNU next_overlay_change: an empty overlay contributes exactly one
    // stop, at its position; face merging paints nothing for it in either
    // path (the shadow suite proves glyph identity).
    eval.eval_str("(overlay-put (make-overlay 3 3) 'face 'bold)")
        .expect("zero-length overlay");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(b"hello\n", 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("zero-length face-only overlay routes");
    assert_eq!(plan.face_boundaries(), &[2]);
}

#[test]
fn classifier_rejects_overlay_hazard_properties() {
    // Any intersecting overlay carrying a property beyond the allow-list keeps
    // the buffer pipeline: display/invisible rewrite content, window restricts
    // applicability, category indirects to arbitrary props, and unknown props
    // are conservatively refused.
    //
    // before-string/after-string are NOT here since P4.6 sub-step 3b: the
    // producer owns their collection and GNU ordering, and the routed commit
    // delegates the append to the pipeline's own session, so they route (see
    // classifier_routes_a_mid_line_overlay_string_anchor). What still refuses
    // is the anchor POSITION and the string SHAPE, not the property.
    for (prop, value) in [
        ("display", "\"X\""),
        ("invisible", "t"),
        ("mouse-face", "'highlight"),
        ("window", "t"),
        ("category", "'some-category"),
        ("line-prefix", "\"> \""),
        ("help-echo", "\"tip\""),
    ] {
        let mut eval = Context::new();
        let buf_id = buffer_with_text(&mut eval, "hello\n");
        eval.buffer_manager_mut().set_current(buf_id);
        eval.eval_str(&format!(
            "(let ((ov (make-overlay 2 4))) \
               (overlay-put ov 'face 'bold) \
               (overlay-put ov '{prop} {value}))"
        ))
        .expect("hazard overlay");
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(b"hello\n", 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::BufferPipeline,
            "an intersecting overlay with {prop} must stay on the buffer pipeline"
        );
    }
}

#[test]
fn classifier_rejects_string_overlay_touching_row_endpoints() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "ab\ncd\n");
    eval.buffer_manager_mut().set_current(buf_id);
    // Overlay ends exactly at the second row's start: its after-string fires
    // there (GNU load_overlay_strings collects at end == charpos), so the row
    // must refuse. This bound is CORRECTNESS, not conservatism: the visible
    // loop attempts the route BEFORE the pipeline step that emits the strings,
    // so a routed row start would drop them entirely.
    eval.eval_str("(overlay-put (make-overlay 1 4) 'after-string \"A\")")
        .expect("after-string overlay");
    let text = b"ab\ncd\n";
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text, 3, 3),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "an overlay ending at the row start with an after-string must refuse"
    );
    // Overlay starting exactly at the row's newline: its before-string fires
    // at the newline position, which the pipeline's line-break lifecycle owns.
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "ab\ncd\n");
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(overlay-put (make-overlay 3 5) 'before-string \"B\")")
        .expect("before-string overlay");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text, 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "an overlay starting at the row's newline with a before-string must refuse"
    );
}

#[test]
fn classifier_ignores_overlays_on_other_rows() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\nworld\n");
    eval.buffer_manager_mut().set_current(buf_id);
    // A string-carrying overlay entirely on the SECOND row. Neither row is
    // disqualified by it: the first does not intersect it at all, and the
    // second carries the anchor strictly inside itself, which routes.
    eval.eval_str("(overlay-put (make-overlay 8 10) 'before-string \"B\")")
        .expect("second-row overlay");
    let text = b"hello\nworld\n";
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text, 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer,
        "an overlay on another row must not disqualify this row"
    );
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text, 6, 6),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer,
        "the anchor's own row routes: it sits strictly inside the line"
    );
}

/// The rung-4 un-refusal, at the classifier: a mid-line anchor routes, and
/// the plan carries the producer's collection so the commit can delegate it.
#[test]
fn classifier_routes_a_mid_line_overlay_string_anchor() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\n");
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(
        "(let ((ov (make-overlay 3 5))) \
           (overlay-put ov 'face 'bold) \
           (overlay-put ov 'before-string \"[\") \
           (overlay-put ov 'after-string \"]\"))",
    )
    .expect("string overlay");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(b"hello\n", 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("a mid-line overlay-string anchor routes");
    // Anchors at the overlay's start (before-string) and end (after-string),
    // 0-based char offsets 2 and 4; each contributes one column.
    let anchors: Vec<(usize, usize)> = plan
        .overlay_strings()
        .iter()
        .map(|anchor| (anchor.at(), anchor.strings().len()))
        .collect();
    assert_eq!(anchors, vec![(2, 1), (4, 1)]);
    assert_eq!(
        plan.overlay_strings()[0].advance_cols(),
        1,
        "a one-character string advances one column"
    );
    // Both endpoints are also face-segment boundaries, which is what gives
    // each insertion a text segment to sort ahead of.
    assert_eq!(plan.face_boundaries(), &[2, 4]);
}

/// Anchor POSITION and string SHAPE are what refuse now, not the property.
#[test]
fn classifier_rejects_unroutable_overlay_string_shapes() {
    for (value, why) in [
        ("\"a\\nb\"", "a newline in the string would end the row"),
        ("\"a\\tb\"", "a tab expands pen-dependently in the session"),
        (
            "(propertize \"x\" 'face 'bold)",
            "text properties re-face the string mid-flight",
        ),
        ("'not-a-string", "a non-string value is not displayable"),
    ] {
        let mut eval = Context::new();
        let buf_id = buffer_with_text(&mut eval, "hello\n");
        eval.buffer_manager_mut().set_current(buf_id);
        eval.eval_str(&format!(
            "(overlay-put (make-overlay 3 5) 'before-string {value})"
        ))
        .expect("string overlay");
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        let plan = plan_plain_row_classified(
            buffer,
            row_start(b"hello\n", 0, 0),
            wide_fit(),
            plain_policy(),
        );
        match plan {
            // A non-string value is dropped at collection (GNU STRINGP), so
            // the position is not an anchor at all and the row routes with no
            // insertion — the same outcome as an empty string.
            Ok(plan) if value == "'not-a-string" => {
                assert!(plan.overlay_strings().is_empty(), "{why}");
            }
            Ok(_) => panic!("expected a refusal: {why}"),
            Err(_) => {}
        }
    }
}

/// An UNROUTABLE string is likewise only this row's business when the row's
/// coverage reaches it. Refusing it at scan time cost the TUI child-frame
/// minibuffer session all three of its routed continuation resumes: its
/// wrapped line carries a propertized string far down the line, which no
/// prefix row ever reaches.
#[test]
fn classifier_routes_a_prefix_whose_unroutable_string_it_never_reaches() {
    let line = "x".repeat(40);
    let text = format!("{line}\n");
    // A propertized string is outside the routable class, anchored at char
    // offset 30 - past a 10-column prefix, inside a whole-line plan.
    let overlay = "(overlay-put (make-overlay 31 33) 'before-string \
                   (propertize \"S\" 'face 'bold))";

    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, &text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(overlay).expect("overlay");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "a whole-line plan reaches the unroutable string"
    );

    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, &text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(overlay).expect("overlay");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(80.0),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer,
        "a prefix that never reaches the unroutable string must still route"
    );
}

/// An anchor at the LINE end is only this row's business when the row's
/// coverage reaches it. Deciding that during the anchor scan - before the fit
/// walk knows where the coverage ends - refused every row of a long wrapped
/// line carrying an anchor at its end, which cost the TUI child-frame
/// minibuffer session three routed continuation resumes and ~480 refusals.
#[test]
fn classifier_routes_a_prefix_whose_line_end_anchor_it_never_reaches() {
    let line = "x".repeat(40);
    let text = format!("{line}\n");
    let overlay_at_line_end = |eval: &mut Context| {
        // Overlay ENDING at the line's newline position: its after-string
        // anchors there, at char offset 40.
        eval.eval_str("(overlay-put (make-overlay 39 41) 'after-string \"A\")")
            .expect("line-end overlay");
    };

    // Whole-line plan (the coverage IS the line): the anchor sits on the line
    // end the pipeline's line-break lifecycle owns, so the row refuses.
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, &text);
    eval.buffer_manager_mut().set_current(buf_id);
    overlay_at_line_end(&mut eval);
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "a whole-line plan reaches its line-end anchor"
    );

    // Overflow-prefix plan holding 10 columns: the coverage stops 30 chars
    // short of the anchor, so the prefix still routes.
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, &text);
    eval.buffer_manager_mut().set_current(buf_id);
    overlay_at_line_end(&mut eval);
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(80.0),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer,
        "a prefix that never reaches the line-end anchor must still route"
    );
}

/// An overlay-string anchor never routes on an OVERFLOW-PREFIX plan: the
/// append session clips at the right edge and can break the row itself, so a
/// handoff cut taken mid-anchor would not be the pipeline's overflow point.
/// An anchor BEYOND the cut is simply not this row's business.
#[test]
fn classifier_rejects_overlay_string_anchors_in_an_overflow_prefix() {
    let line = "x".repeat(40);
    let text = format!("{line}\n");
    for (anchor_charpos, expected_route) in [
        // Inside the fitting prefix (the fit below holds 10 columns).
        (4usize, RowAcquisitionRoute::BufferPipeline),
        // Well past the handoff cut: unrouted remainder the pipeline emits at
        // resume, so the prefix still routes.
        (30, RowAcquisitionRoute::ItemRenderer),
    ] {
        let mut eval = Context::new();
        let buf_id = buffer_with_text(&mut eval, &text);
        eval.buffer_manager_mut().set_current(buf_id);
        eval.eval_str(&format!(
            "(overlay-put (make-overlay {} {}) 'before-string \"S\")",
            anchor_charpos + 1,
            anchor_charpos + 2
        ))
        .expect("string overlay");
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                fit_to(80.0),
                plain_policy()
            ),
            expected_route,
            "anchor at {anchor_charpos}"
        );
    }
}

#[test]
fn classifier_rejects_active_display_table() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "abc\n");
    {
        let table = neovm_core::emacs_core::Value::make_char_table(
            Value::symbol("display-table"),
            Value::NIL,
            6,
        );
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.set_buffer_local("buffer-display-table", table);
    }
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(b"abc\n", 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );
}

/// P4.8(b): an active display table is a property of the BUFFER, not of the
/// row, so it must refuse BEFORE any per-position row scan runs. The pin is
/// the REASON on a row that also carries a content refusal: the display table
/// has to win, which it can only do by being decided first. Production
/// route-stats after (a) put 14718 of 75490 attempts here, each having paid a
/// full classifier walk (display-property scan, overlay scans, fit walk,
/// elision scan) before reaching a verdict that never depended on any of it.
#[test]
fn classifier_refuses_a_display_table_buffer_before_scanning_the_row() {
    let mut eval = Context::new();
    // A C0 control char: the char scan refuses this row on content alone.
    let text = "a\x01b\n";
    let buf_id = buffer_with_text(&mut eval, text);
    {
        let table = neovm_core::emacs_core::Value::make_char_table(
            Value::symbol("display-table"),
            Value::NIL,
            6,
        );
        let buf = eval.buffer_manager_mut().get_mut(buf_id).expect("buffer");
        buf.set_buffer_local("buffer-display-table", table);
    }
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    assert_eq!(
        plan_plain_row_classified(
            buffer,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        )
        .unwrap_err(),
        RouteRefusal::DisplayTable,
        "the buffer-global refusal must be taken before the row is scanned"
    );
}

/// P4.8(b): the refusal window is BOUNDED on both sides. Its whole reason to
/// carry an end is that a refusal proven for the positions up to point says
/// nothing about the positions after it — 1703 corpus rows route there — and
/// a window opened at one line must not swallow a position before it either.
#[test]
fn a_refusal_window_covers_only_the_range_it_was_given() {
    let mut window = RouteRefusalWindow::default();
    assert!(!window.covers(7), "an empty window covers nothing");
    window.refuse_through(4, 9);
    assert!(!window.covers(3), "before the window start");
    assert!(window.covers(4), "the start itself");
    assert!(window.covers(9), "the end itself — point's own position");
    assert!(
        !window.covers(10),
        "past the end, where the walk must classify"
    );
    // A later refusal describes later ground; it replaces the old window
    // rather than merging with it.
    window.refuse_through(20, 24);
    assert!(!window.covers(9));
    assert!(window.covers(24));
    // An end before the start is not a range and records nothing.
    window.refuse_through(40, 39);
    assert!(
        window.covers(24),
        "the degenerate record left the window alone"
    );
}

/// A planned row covers zero characters exactly when the source position is
/// standing on the line's newline.  Keep this classifier invariant explicit:
/// the routed renderer uses the zero-coverage case as its RowBreak-only path.
/// (A first character that does not fit refuses instead of planning, and the
/// visible loop never stands at text end, so neither is a counterexample.)
#[test]
fn a_zero_coverage_plan_is_exactly_a_position_standing_on_a_newline() {
    let mut eval = Context::new();
    let text = "ab\n\ncd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let mut planned_on_newline = 0;
    let mut planned_on_text = 0;
    for byte_idx in 0..text.len() {
        let plan = plan_plain_row_classified(
            buffer,
            row_start(text.as_bytes(), byte_idx, byte_idx as i64),
            wide_fit(),
            wrap_policy(),
        )
        .unwrap_or_else(|reason| panic!("byte {byte_idx} must plan, refused {reason:?}"));
        let on_newline = text.as_bytes()[byte_idx] == b'\n';
        assert_eq!(
            plan.is_empty_line(),
            on_newline,
            "byte {byte_idx} ({:?})",
            text.as_bytes()[byte_idx] as char
        );
        if on_newline {
            planned_on_newline += 1;
        } else {
            planned_on_text += 1;
        }
    }
    assert_eq!((planned_on_newline, planned_on_text), (3, 4));
}

#[test]
fn plain_source_matches_buffer_text_source_cursor_items() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello world\n");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let start = CharPos0::ZERO;
    let line_end = CharPos0::new("hello world".chars().count());

    let mut cursor = BufferTextSourceCursor::new(
        buf_id,
        buffer,
        start,
        line_end.add_len(CharLen::new(1)),
        RenderFaceRef::Inherit,
    );
    let mut cursor_items = Vec::new();
    let mut context = DisplaySourceContext::empty();
    while let Some(item) = cursor.next_item(&mut context) {
        cursor_items.push(item);
    }

    let mut plain = BufferPlainItemSource::with_row_break(
        buf_id,
        buffer,
        start,
        line_end,
        RenderFaceRef::Inherit,
    );
    let mut plain_items = Vec::new();
    while let Some(item) = plain.next_item(&mut context) {
        plain_items.push(item);
    }

    assert_eq!(plain_items, cursor_items);
    assert_eq!(plain_items.len(), 2, "one text run, then the row break");
}

#[test]
fn routed_source_matches_buffer_text_source_cursor_items_for_tab_and_wide() {
    let mut eval = Context::new();
    // Tab and a wide CJK char inside the run: the cursor keeps both in ONE
    // plain TextRun (tab and CJK classify as Text), and the routed source
    // must produce the identical item — same UTF-8 text, same char/byte
    // spans — followed by the identical row break.
    let text = "a\t\u{4E2D} b\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let start = CharPos0::ZERO;
    let line_end = CharPos0::new(text.chars().count() - 1);

    let mut cursor = BufferTextSourceCursor::new(
        buf_id,
        buffer,
        start,
        line_end.add_len(CharLen::new(1)),
        RenderFaceRef::Inherit,
    );
    let mut cursor_items = Vec::new();
    let mut context = DisplaySourceContext::empty();
    while let Some(item) = cursor.next_item(&mut context) {
        cursor_items.push(item);
    }

    let mut routed = BufferPlainItemSource::with_row_break(
        buf_id,
        buffer,
        start,
        line_end,
        RenderFaceRef::Inherit,
    );
    let mut routed_items = Vec::new();
    while let Some(item) = routed.next_item(&mut context) {
        routed_items.push(item);
    }

    assert_eq!(routed_items, cursor_items);
    assert_eq!(routed_items.len(), 2, "one text run, then the row break");
    let DisplayItemKind::TextRun(run) = &routed_items[0].kind else {
        panic!("expected text run, got {:?}", routed_items[0].kind);
    };
    assert_eq!(run.text.as_ref(), "a\t\u{4E2D} b");
}

fn face_resolver_for(eval: &Context) -> FaceResolver {
    FaceResolver::new(eval.face_table(), 0x00FF_FFFF, 0x0000_0000, 14.0, None)
}

#[test]
fn plan_row_face_segments_resolves_per_segment_stable_ids() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\n");
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 5 'face 'bold)")
        .expect("put-text-property");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(b"hello\n", 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("plan");
    let resolver = face_resolver_for(&eval);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(0);
    let segments = plan_row_face_segments(buffer, &resolver, &mut face_ids, CharPos0::ZERO, &plan);
    assert_eq!(segments.len(), 3);
    assert_eq!(
        segments
            .iter()
            .map(|segment| (segment.start.get(), segment.end.get()))
            .collect::<Vec<_>>(),
        vec![(0, 2), (2, 4), (4, 5)]
    );
    // The outer (unfaced) segments content-address to the SAME stable id;
    // the bold span gets its own.
    assert_eq!(segments[0].face_id, segments[2].face_id);
    assert_ne!(segments[0].face_id, segments[1].face_id);
    assert_ne!(
        segments[0].resolved.font_weight,
        segments[1].resolved.font_weight
    );
}

#[test]
fn resolve_routed_position_face_covers_the_newline_span() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "abcd\nnext\n");
    eval.buffer_manager_mut().set_current(buf_id);
    // Face span covering the newline (1-based [3, 6) covers chars "cd\n").
    eval.eval_str("(put-text-property 3 6 'face 'bold)")
        .expect("put-text-property");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let resolver = face_resolver_for(&eval);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(0);
    let (span_id, _) =
        resolve_routed_position_face(buffer, &resolver, &mut face_ids, CharPos0::new(2));
    let (newline_id, _) =
        resolve_routed_position_face(buffer, &resolver, &mut face_ids, CharPos0::new(4));
    let (base_id, _) =
        resolve_routed_position_face(buffer, &resolver, &mut face_ids, CharPos0::new(5));
    assert_eq!(
        span_id, newline_id,
        "a span covering the newline keeps its face at the newline position"
    );
    assert_ne!(newline_id, base_id);

    // A span ending exactly at the newline leaves the newline on the base
    // face.
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "abcd\nnext\n");
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 5 'face 'bold)")
        .expect("put-text-property");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let resolver = face_resolver_for(&eval);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(0);
    let (span_id, _) =
        resolve_routed_position_face(buffer, &resolver, &mut face_ids, CharPos0::new(2));
    let (newline_id, _) =
        resolve_routed_position_face(buffer, &resolver, &mut face_ids, CharPos0::new(4));
    let (base_id, _) =
        resolve_routed_position_face(buffer, &resolver, &mut face_ids, CharPos0::new(5));
    assert_ne!(span_id, newline_id);
    assert_eq!(newline_id, base_id);
}

#[test]
fn routed_segment_item_face_agrees_for_plain_face_spans() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\n");
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 5 'face 'bold)")
        .expect("put-text-property");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let resolver = face_resolver_for(&eval);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(0);
    let default_resolved = resolver.default_face().clone();
    let default_face_id = crate::display_row::face_state::stable_face_id_for_resolved(
        &mut face_ids,
        &default_resolved,
    );
    for pos in [0usize, 2, 4] {
        let (expected_id, _) =
            resolve_routed_position_face(buffer, &resolver, &mut face_ids, CharPos0::new(pos));
        assert!(
            !routed_segment_item_face_diverges(
                buffer,
                &resolver,
                &mut face_ids,
                &default_resolved,
                default_face_id,
                CharPos0::new(pos),
                expected_id,
            ),
            "checkpoint and per-run face chains must agree at {pos}"
        );
    }
}

#[test]
fn routed_segment_item_face_agrees_under_default_remapping() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\n");
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 1 6 'face 'italic)")
        .expect("put-text-property");
    eval.eval_str(
        "(progn (make-local-variable 'face-remapping-alist) \
                (setq face-remapping-alist '((default . bold))))",
    )
    .expect("face remapping");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let resolver = face_resolver_for(&eval);
    let mut face_ids = FrameFaceAttempt::for_test_with_next_id(0);
    let default_resolved = resolver.resolve_buffer_default_face(buffer);
    let default_face_id = crate::display_row::face_state::stable_face_id_for_resolved(
        &mut face_ids,
        &default_resolved,
    );
    let (checkpoint_id, checkpoint_resolved) =
        resolve_routed_position_face(buffer, &resolver, &mut face_ids, CharPos0::ZERO);
    assert_eq!(
        checkpoint_resolved.font_weight,
        neovm_core::face::FontWeight::BOLD.css_weight()
    );
    assert!(checkpoint_resolved.italic);
    assert!(
        !routed_segment_item_face_diverges(
            buffer,
            &resolver,
            &mut face_ids,
            &default_resolved,
            default_face_id,
            CharPos0::ZERO,
            checkpoint_id,
        ),
        "checkpoint and per-run face chains must apply inherited default remapping identically"
    );
}

#[test]
fn plain_source_segments_produce_per_face_text_runs_and_break_face() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "hello\n");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let bold = RenderFaceRef::FaceId(neomacs_display_protocol::types::FaceId::new(40));
    let base = RenderFaceRef::FaceId(neomacs_display_protocol::types::FaceId::new(33));
    let mut source = BufferPlainItemSource::with_row_break_segments(
        buf_id,
        buffer,
        &[
            PlainRowItemSegment {
                start: CharPos0::ZERO,
                end: CharPos0::new(2),
                face: base,
            },
            PlainRowItemSegment {
                start: CharPos0::new(2),
                end: CharPos0::new(4),
                face: bold,
            },
            PlainRowItemSegment {
                start: CharPos0::new(4),
                end: CharPos0::new(5),
                face: base,
            },
        ],
        CharPos0::new(5),
        bold,
    );
    let mut context = DisplaySourceContext::empty();
    let mut items = Vec::new();
    while let Some(item) = source.next_item(&mut context) {
        items.push(item);
    }
    assert_eq!(items.len(), 4, "three text runs then the row break");
    let texts: Vec<_> = items[..3]
        .iter()
        .map(|item| match &item.kind {
            DisplayItemKind::TextRun(run) => run.text.to_string(),
            other => panic!("expected text run, got {other:?}"),
        })
        .collect();
    assert_eq!(texts, vec!["he", "ll", "o"]);
    assert_eq!(
        items.iter().map(|item| item.face).collect::<Vec<_>>(),
        vec![base, bold, base, bold]
    );
    assert!(matches!(items[3].kind, DisplayItemKind::RowBreak(_)));
    // Spans stay contiguous over the row.
    assert_eq!(items[0].span.end, items[1].span.start);
    assert_eq!(items[1].span.end, items[2].span.start);
    assert_eq!(items[2].span.end, items[3].span.start);
}

#[test]
fn plain_source_text_only_omits_the_row_break() {
    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, "ab\n");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let mut source = BufferPlainItemSource::text_only(
        buf_id,
        buffer,
        CharPos0::ZERO,
        CharPos0::new(2),
        RenderFaceRef::Inherit,
    );
    let mut context = DisplaySourceContext::empty();
    let first = source.next_item(&mut context).expect("text run item");
    assert!(matches!(first.kind, DisplayItemKind::TextRun(_)));
    assert_eq!(source.next_item(&mut context), None);
}

// ---- Phase 2d rung 1: invisible text (plain elision, no ellipsis) ----

#[test]
fn classifier_routes_mid_line_plain_elision_and_plans_segments() {
    let mut eval = Context::new();
    // "abXXcd": chars 2..4 invisible (default buffer-invisibility-spec is t,
    // so any non-nil `invisible` value hides without an ellipsis). The row
    // routes with the hidden span elided; the visible segments skip it and
    // the charpos bookkeeping jumps across the gap.
    let text = "abXXcd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 5 'invisible t)")
        .expect("invisible span");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("plain-elision row routes");
    assert_eq!(plan.line_char_len(), 6);
    assert!(plan.has_elision());
    assert_eq!(plan.elided(), &[(2, 4)]);
    assert!(plan.is_segmented());
    assert_eq!(
        plan.segment_ranges(CharPos0::ZERO),
        vec![
            (CharPos0::ZERO, CharPos0::new(2)),
            (CharPos0::new(4), CharPos0::new(6)),
        ],
        "visible segments must skip the elided span"
    );
}

#[test]
fn classifier_routes_trailing_elision_ending_at_newline() {
    let mut eval = Context::new();
    // "abcXX" with "XX" invisible: the hidden run ends exactly AT the
    // newline, which stays visible — the line structure is unchanged.
    let text = "abcXX\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 4 6 'invisible t)")
        .expect("trailing invisible span");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("trailing-elision row routes");
    assert_eq!(plan.elided(), &[(3, 5)]);
    assert_eq!(
        plan.segment_ranges(CharPos0::ZERO),
        vec![(CharPos0::ZERO, CharPos0::new(3))],
        "only the visible prefix renders; the newline stays with the line-break lifecycle"
    );
}

#[test]
fn classifier_rejects_ellipsis_invisible() {
    let mut eval = Context::new();
    // buffer-invisibility-spec entry (org . t) means "hidden WITH ellipsis":
    // the pipeline appends the `...` glyphs with their own rules (GNU
    // setup_for_ellipsis: saved face, newpos-1 provenance) — refused.
    let text = "abXXcd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(
        "(progn (setq buffer-invisibility-spec '((org . t))) \
                (put-text-property 3 5 'invisible 'org))",
    )
    .expect("ellipsis invisible span");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "ellipsis-invisible rows must stay on the buffer pipeline"
    );
}

#[test]
fn classifier_elision_uses_entry_run_ellipsis_flag_for_adjacent_runs() {
    // Adjacent invisible runs collapse into ONE region whose ellipsis flag is
    // the ENTRY run's (the pipeline's check_invisible contract): a
    // no-ellipsis run followed by an ellipsis run still elides plainly and
    // routes; entering ON the ellipsis run refuses.
    let mut eval = Context::new();
    let text = "aXYb\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(
        "(progn (setq buffer-invisibility-spec '(i1 (i2 . t))) \
                (put-text-property 2 3 'invisible 'i1) \
                (put-text-property 3 4 'invisible 'i2))",
    )
    .expect("adjacent invisible runs");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("entry run without ellipsis routes");
    assert_eq!(plan.elided(), &[(1, 3)], "adjacent runs collapse into one");

    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(
        "(progn (setq buffer-invisibility-spec '(i1 (i2 . t))) \
                (put-text-property 2 3 'invisible 'i2) \
                (put-text-property 3 4 'invisible 'i1))",
    )
    .expect("adjacent invisible runs, ellipsis entry");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "an ellipsis ENTRY run must refuse"
    );
}

#[test]
fn classifier_rejects_invisible_spanning_the_newline() {
    let mut eval = Context::new();
    // The hidden region swallows the newline (org-fold shape): the fold joins
    // buffer lines into one display row — a line-structure change the
    // single-row item route cannot express.
    let text = "abXX\ncd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 7 'invisible t)")
        .expect("newline-spanning invisible");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );

    // The invisible run covering ONLY the newline refuses too.
    let mut eval = Context::new();
    let text = "abc\nx\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 4 5 'invisible t)")
        .expect("invisible newline");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "hiding the newline changes the line structure"
    );
}

#[test]
fn classifier_rejects_row_start_invisible() {
    let mut eval = Context::new();
    // A hidden run AT the row start is consumed by the visible loop's
    // invisible checkpoint BEFORE the route attempt (the walk then resumes
    // mid-line, which refuses); the classifier mirrors that ordering so a
    // direct classification agrees with production.
    let text = "XXab\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 1 3 'invisible t)")
        .expect("row-start invisible");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );
}

#[test]
fn classifier_rejects_invisible_from_overlay() {
    let mut eval = Context::new();
    // Overlay-sourced invisibility stays refused (2c allow-list): overlay
    // `invisible` shadows the text property outright in the pipeline's
    // precedence (GNU get_char_property_and_overlay) and interacts with
    // overlay-string emission at both endpoints.
    let text = "abXXcd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(overlay-put (make-overlay 3 5) 'invisible t)")
        .expect("invisible overlay");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );
}

#[test]
fn classifier_routes_non_hiding_invisible_value() {
    let mut eval = Context::new();
    // An `invisible` value NOT in buffer-invisibility-spec hides nothing:
    // the row routes with no elision (the property change still segments,
    // exactly like any other property boundary).
    let text = "abXXcd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(
        "(progn (setq buffer-invisibility-spec '(org)) \
                (put-text-property 3 5 'invisible 'other))",
    )
    .expect("non-hiding invisible value");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("non-hiding invisible value routes");
    assert!(!plan.has_elision());
    assert_eq!(plan.face_boundaries(), &[2, 4]);
}

// ---- Phase 2d rungs 2+3: display-prop refusals (string / space specs) ----

#[test]
fn classifier_routes_display_string_replacement() {
    let mut eval = Context::new();
    // Increment 2i rung 2 (flipping the former 2d refusal pin): a plain
    // property-less single-line string-valued `display` property is now
    // ROUTED. The plan records the covered span; production renders it
    // through the pipeline's OWN replacement session (typed string-index
    // provenance with a covered buffer range, string base-face policy), so
    // the vocabulary limitation that forced the 2d refusal — TextRun's plain
    // buffer-charpos advance — no longer applies.
    let text = "abXXcd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 5 'display \"STR\")")
        .expect("display string replacement");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("display string replacement routes");
    assert!(plan.has_replacement());
    assert_eq!(plan.replacement_ranges(), &[(2, 4)]);
    assert_eq!(plan.line_char_len(), 6);
}

#[test]
fn routed_replacement_extent_follows_the_same_display_object_across_a_property_change() {
    // The covered range of a text-property `display` is GNU's
    // `display_prop_end`: the run over which the RESOLVED VALUE stays the same
    // OBJECT, not the run over which no property changes. Here `face` changes
    // at 5 while the very same string object is the `display` value from 3 to
    // 7, so one replacement covers chars 2..6 — the walk must step over the
    // face boundary rather than stop at it.
    //
    // Explicit goldens, not values read back off the plan: one span, exactly
    // (2, 6), and a line of 8 chars.
    let mut eval = Context::new();
    let text = "abXXYYcd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(
        "(let ((s \"STR\")) \
           (put-text-property 3 7 'display s) \
           (put-text-property 5 7 'face 'bold))",
    )
    .expect("same display object across a face change");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("same-object replacement routes");
    assert_eq!(plan.replacement_ranges(), &[(2, 6)]);
    assert_eq!(plan.line_char_len(), 8);
}

#[test]
fn routed_replacement_extent_stops_at_a_different_display_object_with_equal_text() {
    // The mirror of the pin above, and the reason the rule is stated over
    // object identity rather than string contents: two DISTINCT string
    // objects that happen to spell the same thing are two replacements, each
    // covering its own run. `eq`-ness is the whole rule, so a fold that
    // compared text would pass the pin above and red here.
    let mut eval = Context::new();
    let text = "abXXYYcd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(
        "(progn (put-text-property 3 5 'display (copy-sequence \"STR\")) \
                (put-text-property 5 7 'display (copy-sequence \"STR\")))",
    )
    .expect("two distinct display objects");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("two replacements route");
    assert_eq!(plan.replacement_ranges(), &[(2, 4), (4, 6)]);
    assert_eq!(plan.line_char_len(), 8);
}

#[test]
fn classifier_fits_replacement_by_string_width_not_covered_width() {
    let mut eval = Context::new();
    // The fit walk advances by the REPLACEMENT's width (5 cols for "WIDER"),
    // not the covered chars' width (2 cols): "abXXcd" displays as
    // "abWIDERcd" = 9 cols. A 9-col row exactly fills (refuse, the line-end
    // edge interacts with continuation policy); a 10-col row routes.
    let text = "abXXcd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 5 'display \"WIDER\")")
        .expect("display string replacement");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(9.0 * 8.0),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "a replacement row exactly filling the row keeps the pipeline"
    );
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(10.0 * 8.0),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
}

#[test]
fn classifier_rejects_unroutable_display_string_shapes() {
    // The routed replacement class is deliberately narrow; each shape below
    // has session machinery the routed plan does not predict, so it keeps
    // the buffer pipeline (provenance EXISTS now — these are fit/production
    // scope refusals, not vocabulary gaps):
    // - a string containing a newline (multi-row: the session emits a row
    //   break, GNU display_line ends the row on a display-string '\n');
    // - a string carrying its own text properties (per-run string faces:
    //   the plan's single-run width prediction does not model them);
    // - an empty string (zero-glyph elision-like shape);
    // - a replacement starting AT the row start (the routed commit replays
    //   the loop's segment-0 checkpoint for buffer text);
    // - a covered range extending over the line's newline (line-structure
    //   change, hidden line join);
    // - a string containing a TAB (pen-dependent expansion inside the
    //   session's full-text-width frame).
    for (setup, label) in [
        (
            "(put-text-property 3 5 'display \"S\\nT\")",
            "newline in string",
        ),
        (
            "(put-text-property 3 5 'display (propertize \"STR\" 'face 'bold))",
            "string with own props",
        ),
        ("(put-text-property 3 5 'display \"\")", "empty string"),
        ("(put-text-property 1 3 'display \"STR\")", "row-start"),
        (
            "(put-text-property 5 8 'display \"STR\")",
            "covers the newline",
        ),
        (
            "(put-text-property 3 5 'display \"S\\tT\")",
            "tab in string",
        ),
    ] {
        let mut eval = Context::new();
        let text = "abXXcd\n";
        let buf_id = buffer_with_text(&mut eval, text);
        eval.buffer_manager_mut().set_current(buf_id);
        eval.eval_str(setup).expect(label);
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::BufferPipeline,
            "{label} must keep the buffer pipeline"
        );
    }
}

#[test]
fn classifier_rejects_replacement_combined_with_elision_or_overflow() {
    // Conservative composition refusals: a routed row carries EITHER plain
    // elision OR a replacement, never both (their skip bookkeeping would
    // interleave), and a replacement row never routes as an overflow prefix
    // (the scan's handoff cut would not be the pipeline's overflow point).
    let mut eval = Context::new();
    let text = "abXXcdEFGH\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str(
        "(progn (put-text-property 3 5 'display \"STR\") \
                (put-text-property 7 9 'invisible t))",
    )
    .expect("replacement + invisible");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "replacement + elision must keep the buffer pipeline"
    );

    let mut eval = Context::new();
    let text = "abXXcdefghij\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 5 'display \"STR\")")
        .expect("replacement");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(6.0 * 8.0),
            wrap_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "an over-wide replacement row must not route an overflow prefix"
    );
}

// ---- Phase 2e rung 1: composition refusals grounded in the pipeline seam ----

#[test]
fn classifier_rejects_valid_static_composition_prop() {
    // The pipeline replaces the covered chars when (and only when) the
    // `composition` text property parses to display text
    // (BufferTextSourceCursor::next_text_item_with_layout ->
    // composition_display_text_for_property). The classifier refuses on the
    // SAME predicate. Both the fixnum-component and string-component Form-A
    // shapes (prettify-symbols / compose-region) refuse.
    for value in ["'((2 . ?x))", "'((2 . \"ab\"))"] {
        let mut eval = Context::new();
        let buf_id = buffer_with_text(&mut eval, "hello\n");
        eval.buffer_manager_mut().set_current(buf_id);
        eval.eval_str(&format!("(put-text-property 3 5 'composition {value})"))
            .expect("put-text-property");
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(b"hello\n", 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::BufferPipeline,
            "a parseable composition prop ({value}) must stay on the buffer pipeline"
        );
    }
}

#[test]
fn classifier_routes_inert_composition_prop() {
    // A `composition` prop the pipeline's replacement predicate does NOT
    // parse (zero length, or garbage) renders its chars literally through
    // the ordinary text run — the row routes, and the property change
    // positions segment it exactly like any other property boundary.
    for value in ["'((0 . 1))", "'garbage"] {
        let mut eval = Context::new();
        let buf_id = buffer_with_text(&mut eval, "hello\n");
        eval.buffer_manager_mut().set_current(buf_id);
        eval.eval_str(&format!("(put-text-property 3 5 'composition {value})"))
            .expect("put-text-property");
        let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
        let plan = plan_plain_row_classified(
            buffer,
            row_start(b"hello\n", 0, 0),
            wide_fit(),
            plain_policy(),
        )
        .unwrap_or_else(|refusal| {
            panic!("an inert composition prop ({value}) must route, got {refusal:?}")
        });
        assert_eq!(plan.face_boundaries(), &[2, 4]);
    }
}

#[test]
fn classifier_rejects_rows_the_pipeline_would_compose() {
    // Each row contains a char the shared writer would COMPOSE into the
    // previous glyph (composition.rs continues_cluster /
    // continues_complex_run — the same predicate the classifier consults)
    // in a shape OUTSIDE the rung-2 routed class (zero-width extender on a
    // simple 1-col base): ZWJ/ZWNJ joiners (open-ended sequences), a
    // 2-col skin-tone extender, a same-script contextual-shaping run, an
    // extender on a WIDE base (merges into the padding-cell shape), and
    // extenders with no in-row base (row start / after a tab's stretch
    // glyph, where the writer pushes a standalone orphan glyph instead).
    for text in [
        "a\u{1F468}\u{200D}\u{1F469}b\n", // ZWJ emoji sequence
        "a\u{200D}b\n",                   // bare ZWJ joiner
        "a\u{200C}b\n",                   // bare ZWNJ joiner
        "a\u{1F44D}\u{1F3FB}b\n",         // thumbs-up + skin-tone modifier
        "\u{0E01}\u{0E49}\n",             // Thai consonant + tone mark run
        "a\u{4E2D}\u{0301}b\n",           // combining mark on a wide base
        "\u{0301}x\n",                    // combining mark at row start
        "a\t\u{0301}b\n",                 // combining mark after a tab
    ] {
        let mut eval = Context::new();
        let buf_id = buffer_with_text(&mut eval, text);
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::BufferPipeline,
            "content {text:?} composes in the pipeline and must refuse the route"
        );
    }
}

#[test]
fn classifier_routes_plain_space_width_spec() {
    let mut eval = Context::new();
    // Increment 2i rung 3 (flipping the former 2d rung-3 refusal pin): a
    // plain `(space :width N)` spec with a positive fixnum N now ROUTES.
    // Production renders it through the pipeline's replacement session
    // (one stretch glyph with covered-buffer provenance — GNU stamps the
    // covered buffer position on stretch glyphs, xdisp.c 6604+32684); the
    // plan credits the covered span with N columns for the fit walk.
    let text = "ab cd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 4 'display '(space :width 3))")
        .expect("space width spec");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("plain space :width spec routes");
    assert!(plan.has_replacement());
    assert_eq!(plan.replacement_ranges(), &[(2, 3)]);
}

#[test]
fn classifier_rejects_space_specs_outside_the_plain_width_form() {
    // Rung 3 keeps every other `(space …)` shape refused: their widths are
    // pen/metric/expression-dependent in ways the plan's logical-cell fit
    // pre-filter cannot predict (`:align-to` targets a column, relative
    // widths consult the covered char's font, extra keys add vertical
    // geometry, floats and expressions ride calc_pixel_width_or_height).
    for (spec, label) in [
        ("'(space :align-to 5)", "align-to"),
        ("'(space :relative-width 2)", "relative-width"),
        ("'(space :width 3 :height 2)", "extra height key"),
        ("'(space :width 2.5)", "float width"),
        ("'(space :width (+ 1 2))", "expression width"),
        ("'(space :width 0)", "zero width"),
        ("'(space)", "bare space"),
    ] {
        let mut eval = Context::new();
        let text = "ab cd\n";
        let buf_id = buffer_with_text(&mut eval, text);
        eval.buffer_manager_mut().set_current(buf_id);
        eval.eval_str(&format!("(put-text-property 3 4 'display {spec})"))
            .expect(label);
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::BufferPipeline,
            "{label} must keep the buffer pipeline"
        );
    }
}

#[test]
fn classifier_fits_space_spec_by_stretch_width() {
    let mut eval = Context::new();
    // "ab cd" with (space :width 3) over the blank displays as 7 cols
    // (ab + 3-col stretch + cd): a 7-col row exactly fills (refuse), an
    // 8-col row routes.
    let text = "ab cd\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 4 'display '(space :width 3))")
        .expect("space width spec");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(7.0 * 8.0),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(8.0 * 8.0),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
}

// ---- Phase 2e rung 2: routed composed clusters (zero-width extenders) ----

#[test]
fn classifier_routes_combining_mark_cluster_and_plans_composed() {
    let mut eval = Context::new();
    // "ae\u{301}b": the combining acute is a zero-width extender the shared
    // writer merges into the 'e' Char glyph (Char -> Composite). The row
    // routes with the extender recorded as a composed offset.
    let text = "ae\u{0301}b\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("combining-mark cluster row routes");
    assert_eq!(plan.line_char_len(), 4);
    assert_eq!(plan.line_byte_len(), 5, "one 2-byte combining mark");
    assert!(plan.has_composed());
    assert_eq!(plan.composed(), &[2]);
    assert!(!plan.has_wide());
}

#[test]
fn classifier_routes_keycap_cluster() {
    let mut eval = Context::new();
    // "1\u{FE0F}\u{20E3}x": VS16 and the combining enclosing keycap are both
    // zero-width extenders on the '1' base — the same Composite merge.
    let text = "1\u{FE0F}\u{20E3}x\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("keycap cluster row routes");
    assert_eq!(plan.line_char_len(), 4);
    assert_eq!(plan.composed(), &[1, 2]);
}

#[test]
fn classifier_fit_counts_composed_extenders_as_zero_cols() {
    let mut eval = Context::new();
    // "ae\u{301}b" occupies 3 columns (the mark contributes 0, exactly like
    // GNU cmp->width / string-width): a 3-cell row is exact fill (refused),
    // 4 cells route.
    let text = "ae\u{0301}b\n";
    let buf_id = buffer_with_text(&mut eval, text);
    for (edge, expected) in [
        (24.0, RowAcquisitionRoute::BufferPipeline),
        (32.0, RowAcquisitionRoute::ItemRenderer),
    ] {
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                fit_to(edge),
                plain_policy()
            ),
            expected,
            "edge {edge}px"
        );
    }
}

#[test]
fn classifier_rejects_composed_cluster_straddling_face_boundary() {
    let mut eval = Context::new();
    // A face change lands exactly ON the extender: the segment split would
    // put the mark at a segment start, but the writer still merges it into
    // the PREVIOUS segment's base glyph (keeping that base's face) — a
    // cross-segment shape the per-segment routed render does not replicate.
    let text = "ae\u{0301}b\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    // 1-based char [3, 4) is the combining mark.
    eval.eval_str("(put-text-property 3 4 'face 'bold)")
        .expect("face on the mark");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "a face boundary on the extender must refuse"
    );
}

#[test]
fn classifier_rejects_composed_cluster_with_hidden_base() {
    let mut eval = Context::new();
    // The base 'e' is elided but the mark stays visible: the pipeline's
    // walk then merges the mark into the PRECEDING visible glyph ('a'),
    // while the routed segments would render the mark at a segment start —
    // refuse. Hiding the MARK itself is fine (nothing composes: the elided
    // span simply drops it), so that row routes.
    let text = "ae\u{0301}b\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 2 3 'invisible t)")
        .expect("hide the base");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline,
        "a visible extender whose base is hidden must refuse"
    );

    let mut eval = Context::new();
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 3 4 'invisible t)")
        .expect("hide the mark");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        wide_fit(),
        plain_policy(),
    )
    .expect("a hidden extender elides and routes");
    assert_eq!(plan.elided(), &[(2, 3)]);
}

#[test]
fn routed_source_matches_cursor_items_for_combining_mark() {
    let mut eval = Context::new();
    // The cursor keeps the mark inside ONE plain TextRun (it classifies as
    // Text); the routed source must produce the identical item.
    let text = "ae\u{0301}b\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let start = CharPos0::ZERO;
    let line_end = CharPos0::new(text.chars().count() - 1);

    let mut cursor = BufferTextSourceCursor::new(
        buf_id,
        buffer,
        start,
        line_end.add_len(CharLen::new(1)),
        RenderFaceRef::Inherit,
    );
    let mut cursor_items = Vec::new();
    let mut context = DisplaySourceContext::empty();
    while let Some(item) = cursor.next_item(&mut context) {
        cursor_items.push(item);
    }

    let mut routed = BufferPlainItemSource::with_row_break(
        buf_id,
        buffer,
        start,
        line_end,
        RenderFaceRef::Inherit,
    );
    let mut routed_items = Vec::new();
    while let Some(item) = routed.next_item(&mut context) {
        routed_items.push(item);
    }

    assert_eq!(routed_items, cursor_items);
    assert_eq!(routed_items.len(), 2, "one text run, then the row break");
    let DisplayItemKind::TextRun(run) = &routed_items[0].kind else {
        panic!("expected text run, got {:?}", routed_items[0].kind);
    };
    assert_eq!(run.text.as_ref(), "ae\u{0301}b");
}

// ---- Phase 2f: overflow-prefix routing (truncation / character wrap) ----

#[test]
fn classifier_plans_truncation_prefix_for_overwide_line() {
    let mut eval = Context::new();
    // 8 chars in a 5-cell row (40px at 8px cells): "abcde" fits — the 5th
    // char ends exactly AT the edge, which the pipeline's fit rule accepts
    // (x + advance <= right_edge) — and "f" would cross, so the plan covers
    // the 5-char prefix and hands the walk back at "f". The pipeline's own
    // truncation machinery (consume_truncation_skip, Truncated row flag,
    // row transition) then runs unchanged.
    let text = "abcdefgh\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        fit_to(40.0),
        plain_policy(),
    )
    .expect("truncation prefix plan");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert!(plan.is_overflow_handoff());
    assert_eq!(plan.line_char_len(), 5);
    assert_eq!(plan.line_byte_len(), 5);
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(40.0),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
}

#[test]
fn classifier_plans_wrap_prefix_for_overwide_line() {
    let mut eval = Context::new();
    // Same prefix cut under character wrap (WINDOW_WRAP): the plan is
    // identical — the wrap-vs-truncate decision is the PIPELINE's, made at
    // the handoff char by its own overflow action (CharacterWrap emits the
    // continuation transition, carry-over bookkeeping and all).
    let text = "abcdefgh\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        fit_to(40.0),
        wrap_policy(),
    )
    .expect("wrap prefix plan");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert_eq!(plan.line_char_len(), 5);
}

#[test]
fn classifier_routes_hebrew_rtl_rows() {
    // Phase 2g audit pin: Hebrew is NON-shaping (not a `complex_script`) and
    // unambiguous width 1, so RTL Hebrew lines route. This is sound because
    // bidi reordering in neomacs is a row-level INSTALL step
    // (`GlyphRowFinalizer::finalize` -> `reorder_row_bidi`), downstream of
    // the acquisition seam: both producers emit identical logical-order rows
    // and the same pure function permutes them to visual order (proven
    // glyph-for-glyph by the hebrew/mixed shadow tests in engine_test.rs).
    let mut eval = Context::new();
    for text in [
        "\u{05D0}\u{05D1}\u{05D2}\n",
        "abc \u{05D0}\u{05D1}\u{05D2} def\n",
        "\u{05E9}\u{05DC}\u{05D5}\u{05DD} world\n",
    ] {
        let buf_id = buffer_with_text(&mut eval, text);
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::ItemRenderer,
            "RTL/mixed content {text:?} must route to the item renderer"
        );
    }
}

#[test]
fn classifier_refuses_directional_formatting_chars() {
    // Phase 2g pin: every directional formatting char — the implicit marks
    // LRM/RLM, the explicit embeddings/overrides LRE/RLE/PDF/LRO/RLO, and
    // the isolates LRI/RLI/FSI/PDI — refuses the route. They classify as
    // Glyphless ZeroWidth (`glyphless_method_for_char`: the 0x200B..=0x200F
    // fast path and the Cf + Default_Ignorable arm), i.e. non-Text, so the
    // ladder's non-Text arm refuses; the 2e composed-extender arm cannot
    // swallow them (it only admits Text-class cluster extenders). NOTE the
    // honest gap this pin sits on: the pipeline drops these marks before the
    // row (ZeroWidth glyphless emits no glyph), so the install-time UBA
    // never sees them and they cannot steer resolution as they do in GNU —
    // a pipeline-wide parity gap on BOTH paths, recorded for the divergence
    // backlog, not a routing divergence.
    let mut eval = Context::new();
    for mark in [
        '\u{200E}', // LRM
        '\u{200F}', // RLM
        '\u{202A}', // LRE
        '\u{202B}', // RLE
        '\u{202C}', // PDF
        '\u{202D}', // LRO
        '\u{202E}', // RLO
        '\u{2066}', // LRI
        '\u{2067}', // RLI
        '\u{2068}', // FSI
        '\u{2069}', // PDI
    ] {
        let text = format!("ab{mark}cd\n");
        let buf_id = buffer_with_text(&mut eval, &text);
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                wide_fit(),
                plain_policy()
            ),
            RowAcquisitionRoute::BufferPipeline,
            "directional formatting char U+{:04X} must refuse the route",
            mark as u32
        );
    }
}

#[test]
fn classifier_refuses_overwide_line_under_word_wrap() {
    // Phase 2f rung 3 pin: WORD wrap stays refused whole. The pipeline's
    // word-wrap machinery records a break candidate PER RENDERED CHAR while
    // the row is still filling (WordWrapRenderState::record_candidate: byte
    // and char position, the row's display-point count, the row's first/last
    // display positions, and a glyph checkpoint — GNU SAVE_IT of wrap_it
    // plus the wrap_row_* metric snapshot, xdisp.c:26071-26105), so the
    // overflow action can roll the row BACK to the candidate
    // (restore_glyph_checkpoint + truncate_display_points, GNU
    // back_to_wrap's unproduce_glyphs + RESTORE_IT). A routed run appends
    // whole TextRuns without recording those per-char snapshots, so the
    // rollback state would be missing at the overflow point. Gap analysis
    // for phase 4: the item flow would need the writer to expose a
    // per-appended-char candidate hook (char, glyph checkpoint,
    // display-point count) so WordWrapRenderState can be fed during run
    // appends — or the word-wrap candidate search must move behind the
    // append seam entirely.
    let mut eval = Context::new();
    let text = "aaa bbb ccc\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let policy = RowRouteWindowPolicy {
        word_wrap: true,
        wrap_mode: LineWrapMode::Wrap,
        ..plain_policy()
    };
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(40.0),
            policy
        ),
        RowAcquisitionRoute::BufferPipeline,
        "word wrap keeps the whole row on the buffer pipeline"
    );
}

#[test]
fn classifier_refuses_prefix_when_first_char_overflows() {
    let mut eval = Context::new();
    // A sub-cell row: even the first char crosses the edge, so there is no
    // fitting prefix to route; the pipeline owns the whole line.
    let text = "abc\n";
    let buf_id = buffer_with_text(&mut eval, text);
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(4.0),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );
}

#[test]
fn classifier_prefix_point_refusal_covers_the_whole_cursor_line() {
    let mut eval = Context::new();
    let text = "abcdefgh\n";
    let buf_id = buffer_with_text(&mut eval, text);
    // Point ANYWHERE on the line (through the newline) refuses, including
    // the unrouted remainder of an over-wide line: the cursor row is the row
    // the steady-state edit path re-lays every keystroke, so its refusal is
    // decided by the cheap pre-gate (memchr + arithmetic) BEFORE any pen
    // walk — the phase-3 probe-cost fix. Refusing the conservative superset
    // is always safe; the buffer pipeline owns cursor rows.
    for point in 0..=8 {
        let policy = RowRouteWindowPolicy {
            point_charpos: point,
            ..plain_policy()
        };
        assert_eq!(
            classify_in_buffer(
                &eval,
                buf_id,
                row_start(text.as_bytes(), 0, 0),
                fit_to(40.0),
                policy
            ),
            RowAcquisitionRoute::BufferPipeline,
            "point {point} on the cursor line"
        );
    }
    // Point past the line's newline routes: the cursor is on a later row.
    let policy = RowRouteWindowPolicy {
        point_charpos: 9,
        ..plain_policy()
    };
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(40.0),
            policy
        ),
        RowAcquisitionRoute::ItemRenderer,
        "point on the next line"
    );
}

#[test]
fn classifier_prefix_plans_face_boundaries_inside_prefix_only() {
    let mut eval = Context::new();
    let text = "abcdefgh\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    // 1-based [2, 4) = chars 1..3: a face span crossing nothing — boundaries
    // at char offsets 1 and 3, both strictly inside the 5-char prefix.
    eval.eval_str("(put-text-property 2 4 'face 'bold)")
        .expect("face span");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        fit_to(40.0),
        plain_policy(),
    )
    .expect("prefix plan with face span");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert_eq!(plan.face_boundaries(), &[1, 3]);
    assert_eq!(
        plan.segment_ranges(CharPos0::ZERO),
        vec![
            (CharPos0::ZERO, CharPos0::new(1)),
            (CharPos0::new(1), CharPos0::new(3)),
            (CharPos0::new(3), CharPos0::new(5)),
        ]
    );
}

#[test]
fn classifier_prefix_face_span_crossing_the_clip_splits_at_its_start_only() {
    let mut eval = Context::new();
    let text = "abcdefgh\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    // 1-based [4, 8) = chars 3..7: the span CROSSES the handoff (prefix ends
    // at char 5). Only its start boundary lands inside the prefix; the
    // remainder of the span is unrouted and the pipeline re-resolves it at
    // resume.
    eval.eval_str("(put-text-property 4 8 'face 'bold)")
        .expect("face span crossing the clip");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        fit_to(40.0),
        plain_policy(),
    )
    .expect("prefix plan with clip-crossing face span");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert_eq!(plan.face_boundaries(), &[3]);
    assert_eq!(
        plan.segment_ranges(CharPos0::ZERO),
        vec![
            (CharPos0::ZERO, CharPos0::new(3)),
            (CharPos0::new(3), CharPos0::new(5)),
        ]
    );
}

#[test]
fn classifier_prefix_ignores_hazards_beyond_the_handoff() {
    let mut eval = Context::new();
    let text = "abcdefgh\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    // A display replacement entirely BEYOND the handoff (1-based [7, 9) =
    // chars 6..8, prefix ends at 5): unrouted remainder, pipeline handles it
    // at resume identically flag-on and flag-off.
    eval.eval_str("(put-text-property 7 9 'display \"XX\")")
        .expect("display prop beyond the prefix");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        fit_to(40.0),
        plain_policy(),
    )
    .expect("hazard beyond the handoff must not refuse the prefix");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert_eq!(plan.line_char_len(), 5);
}

#[test]
fn classifier_prefix_refuses_hazard_inside_the_prefix() {
    let mut eval = Context::new();
    let text = "abcdefgh\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 2 4 'display \"XX\")")
        .expect("display prop inside the prefix");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(40.0),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );
}

#[test]
fn classifier_prefix_refuses_elision_inside_the_prefix() {
    let mut eval = Context::new();
    // An invisible span inside the prefix would make the scan's fit walk
    // overcount (it advances the pen for hidden chars too), so the handoff
    // cut would not be the pipeline's overflow point: refuse.
    let text = "abcdefgh\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    eval.eval_str("(put-text-property 2 4 'invisible t)")
        .expect("invisible span inside the prefix");
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 0, 0),
            fit_to(40.0),
            plain_policy()
        ),
        RowAcquisitionRoute::BufferPipeline
    );
}

#[test]
fn classifier_prefix_byte_len_tracks_multibyte_chars() {
    let mut eval = Context::new();
    // Five 1-column 2-byte chars in a 3-cell row: prefix is 3 chars, 6 bytes.
    let text = "ééééé\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        fit_to(24.0),
        plain_policy(),
    )
    .expect("multibyte prefix plan");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert_eq!(plan.line_char_len(), 3);
    assert_eq!(plan.line_byte_len(), 6);
}

// ---- Phase 2h rung 1: empty lines (bare-newline rows) ----

#[test]
fn classifier_routes_empty_line_and_plans_row_break_only() {
    let mut eval = Context::new();
    // "x", an empty line, then "y": the empty line (a bare newline at
    // charpos 2) routes with a RowBreak-only plan — zero covered chars, the
    // shared line-end plan consumes the newline (GNU display_line's
    // at_end_of_line branch, xdisp.c:26517).
    let text = "x\n\ny\n";
    let buf_id = buffer_with_text(&mut eval, text);
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 2, 2),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 2, 2),
        wide_fit(),
        plain_policy(),
    )
    .expect("empty-line plan");
    assert_eq!(plan.line_char_len(), 0);
    assert_eq!(plan.line_byte_len(), 0);
    assert_eq!(plan.line_end(), RoutedRowLineEnd::Newline);
    assert!(plan.is_empty_line());
    assert!(plan.segment_ranges(CharPos0::new(2)).is_empty());
}

#[test]
fn classifier_refuses_empty_line_when_point_on_newline() {
    let mut eval = Context::new();
    // Point ON the empty line's newline: the cursor rides the appended
    // newline space (GNU set_cursor_from_row's empty_line_p glyph), a
    // documented buffer-pipeline responsibility.
    let text = "x\n\ny\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let refused = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 2, 2),
        wide_fit(),
        RowRouteWindowPolicy {
            point_charpos: 2,
            ..plain_policy()
        },
    );
    assert_eq!(refused.unwrap_err(), RouteRefusal::PointInRow);
}

#[test]
fn classifier_refuses_empty_line_with_hazard_prop_on_newline() {
    let mut eval = Context::new();
    let text = "x\n\ny\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    // A display prop covering the newline replaces the line end. Since
    // increment 2i the dedicated replacement scan reports it: a string
    // display value outside the routed class refuses as Replacement.
    eval.eval_str("(put-text-property 3 4 'display \"D\")")
        .expect("display prop on the empty line's newline");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let refused = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 2, 2),
        wide_fit(),
        plain_policy(),
    );
    assert_eq!(refused.unwrap_err(), RouteRefusal::Replacement);
}

#[test]
fn classifier_refuses_empty_line_with_string_overlay_at_newline() {
    let mut eval = Context::new();
    let text = "x\n\ny\n";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    // An overlay with a before-string anchored on the empty line injects a
    // Lisp-string run; the overlay allow-list refuses it.
    eval.eval_str("(overlay-put (make-overlay 3 4) 'before-string \"S\")")
        .expect("string overlay at the empty line");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let refused = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 2, 2),
        wide_fit(),
        plain_policy(),
    );
    assert_eq!(refused.unwrap_err(), RouteRefusal::Overlay);
}

#[test]
fn plain_source_row_break_only_produces_single_row_break_item() {
    let mut eval = Context::new();
    let text = "x\n\ny\n";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let mut source = BufferPlainItemSource::with_row_break_segments(
        buf_id,
        buffer,
        &[],
        CharPos0::new(2),
        RenderFaceRef::FaceId(FaceId::new(7)),
    );
    let mut context = DisplaySourceContext::empty();
    let mut items = Vec::new();
    while let Some(item) = source.next_item(&mut context) {
        items.push(item);
    }
    assert_eq!(items.len(), 1, "RowBreak-only production: {items:?}");
    let DisplayItemKind::RowBreak(row_break) = items[0].kind else {
        panic!("expected a row break, got {:?}", items[0].kind);
    };
    assert_eq!(
        row_break,
        DisplayRowBreak::explicit_newline()
            .with_line_height(DisplayLineHeightPolicy::from_property(None))
    );
    let byte_at = |pos: CharPos0| buffer.layout_char_pos_to_emacs_byte_pos(pos);
    assert_eq!(
        items[0].span.start,
        DisplaySourcePosition::buffer(buf_id, CharPos0::new(2), byte_at(CharPos0::new(2)))
    );
    assert_eq!(
        items[0].span.end,
        DisplaySourcePosition::buffer(buf_id, CharPos0::new(3), byte_at(CharPos0::new(3)))
    );
    assert_eq!(items[0].face, RenderFaceRef::FaceId(FaceId::new(7)));
}

// ---- Phase 2h rung 2: the EOB tail row (line ending at the source end) ----

#[test]
fn classifier_routes_eob_tail_row_without_newline() {
    let mut eval = Context::new();
    // The buffer ends without a trailing newline: the last line ends at the
    // window read's end, which the bounded read guarantees is the accessible
    // end (the read bound always cuts AFTER a newline, never mid-line). The
    // row routes as TextRun-only coverage; the pipeline's post-loop EOB
    // machinery (appended default-face space, ends_at_zv, ZV placeholder)
    // runs unchanged after the walk exits (GNU xdisp.c:26007 EOB path).
    let text = "abc\nxyz";
    let buf_id = buffer_with_text(&mut eval, text);
    assert_eq!(
        classify_in_buffer(
            &eval,
            buf_id,
            row_start(text.as_bytes(), 4, 4),
            wide_fit(),
            plain_policy()
        ),
        RowAcquisitionRoute::ItemRenderer
    );
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 4, 4),
        wide_fit(),
        plain_policy(),
    )
    .expect("EOB tail plan");
    assert_eq!(plan.line_char_len(), 3);
    assert_eq!(plan.line_byte_len(), 3);
    assert_eq!(plan.line_end(), RoutedRowLineEnd::EndOfSource);
    assert!(plan.is_end_of_source());
    assert_eq!(
        plan.segment_ranges(CharPos0::new(4)),
        vec![(CharPos0::new(4), CharPos0::new(7))]
    );
}

#[test]
fn classifier_refuses_eob_tail_containing_point() {
    let mut eval = Context::new();
    let text = "abc\nxyz";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    // Point inside the tail AND point at EOB (one past the last char — the
    // cursor-at-EOB row) both refuse: cursor capture is pipeline-owned.
    for point in [4, 5, 7] {
        let refused = plan_plain_row_classified(
            buffer,
            row_start(text.as_bytes(), 4, 4),
            wide_fit(),
            RowRouteWindowPolicy {
                point_charpos: point,
                ..plain_policy()
            },
        );
        assert_eq!(
            refused.unwrap_err(),
            RouteRefusal::PointInRow,
            "point {point} sits on the EOB tail row"
        );
    }
}

#[test]
fn classifier_plans_overflow_prefix_for_overwide_eob_tail() {
    let mut eval = Context::new();
    // An over-wide tail line: the routed coverage is the fitting prefix and
    // the pipeline's own overflow machinery consumes the rest, exactly like
    // a phase 2f over-wide newline line.
    let text = "abcdefgh";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        fit_to(40.0),
        plain_policy(),
    )
    .expect("over-wide EOB tail prefix plan");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::OverflowHandoff);
    assert_eq!(plan.line_char_len(), 5);
}

#[test]
fn classifier_routes_exactly_filling_eob_tail() {
    let mut eval = Context::new();
    // A tail line exactly filling the row routes: with no following char
    // there is no continuation/truncation edge to interact with — the
    // pipeline appends the same chars (each satisfying x + advance <=
    // right_edge) and the walk simply ends at the source end.
    let text = "abcde";
    let buf_id = buffer_with_text(&mut eval, text);
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 0, 0),
        fit_to(40.0),
        plain_policy(),
    )
    .expect("exact-fill EOB tail plan");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::EndOfSource);
    assert_eq!(plan.line_char_len(), 5);
}

#[test]
fn classifier_eob_tail_plans_face_boundaries() {
    let mut eval = Context::new();
    let text = "abc\nxyz";
    let buf_id = buffer_with_text(&mut eval, text);
    eval.buffer_manager_mut().set_current(buf_id);
    // Face span over "y" (1-based buffer positions 6..7): segments the tail.
    eval.eval_str("(put-text-property 6 7 'face 'bold)")
        .expect("face span in the EOB tail");
    let buffer = eval.buffer_manager().get(buf_id).expect("buffer");
    let plan = plan_plain_row_classified(
        buffer,
        row_start(text.as_bytes(), 4, 4),
        wide_fit(),
        plain_policy(),
    )
    .expect("segmented EOB tail plan");
    assert_eq!(plan.line_end(), RoutedRowLineEnd::EndOfSource);
    assert_eq!(plan.face_boundaries(), &[1, 2]);
}

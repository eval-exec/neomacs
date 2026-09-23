use super::*;

const NEWLINE_FACE: FaceId = FaceId::new(7);
const EXTEND_FACE: FaceId = FaceId::new(11);
/// The Leuven default background, `#FFFFFF`.
fn frame_bg() -> Color {
    Color::from_pixel(0x00FFFFFF)
}

fn terminal_ctx() -> LineEndContext {
    LineEndContext {
        newline_face_id: NEWLINE_FACE,
        measurement_mode: DisplayRowMeasurementMode::LogicalCells,
        pen_x: 24.0,
        pen_col: 3,
        right_edge_x: 80.0,
        char_width: 8.0,
        indicator: None,
        extend: None,
        frame_background: frame_bg(),
        trailing_whitespace_enabled: false,
        box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges::Neither,
        box_run_membership: neomacs_display_protocol::face::BoxRunMembership::Unboxed,
    }
}

fn with_extend(mut ctx: LineEndContext) -> LineEndContext {
    ctx.extend = Some(LineEndExtend {
        bg: Color::from_pixel(0x00112233),
        face_id: EXTEND_FACE,
    });
    ctx
}

/// An `:extend` face whose background is EXACTLY the frame background --
/// the Leuven `diff-context` shape (GNU defines it `'((t :extend t))`,
/// lisp/vc/diff-mode.el:476-479; Leuven maps it to `diff-none`, whose
/// realized background is the default `#FFFFFF`).
fn with_invisible_extend(mut ctx: LineEndContext) -> LineEndContext {
    ctx.extend = Some(LineEndExtend {
        bg: frame_bg(),
        face_id: EXTEND_FACE,
    });
    ctx
}

fn with_indicator(mut ctx: LineEndContext, col: i32) -> LineEndContext {
    ctx.indicator = Some(LineEndIndicator { col, ch: '|' });
    ctx
}

#[test]
fn terminal_newline_appends_newline_face_space_then_extends_from_advanced_pen() {
    let ctx = with_extend(terminal_ctx());
    let plan = plan(&ctx);
    assert_eq!(
        plan.steps(),
        &[
            LineEndStep::AppendGlyph {
                ch: ' ',
                face: AppendedGlyphFace::NewlineFace,
            },
            // The fill starts one char cell past the pre-advance pen.
            LineEndStep::ExtendFill { from_x: 32.0 },
        ]
    );
}

/// P4.1 vocabulary pin: the glyph `append_space_for_newline` appends is
/// redisplay's own, and its stamp is exactly what
/// `GlyphProvenance::line_end()` says it is -- not a buffer position, and
/// (per the vocabulary) a different THING from a truncation mark even
/// though the glyph field cannot tell them apart.
#[test]
fn appended_newline_glyph_carries_line_end_provenance() {
    use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
    use neomacs_display_protocol::glyph_matrix::GlyphProvenance;

    let mut row = GlyphRow::new(GlyphRowRole::Text);
    let appended = AppendNewlineGlyphMutation {
        ch: ' ',
        face_id: NEWLINE_FACE,
        char_width: 8.0,
    }
    .apply(&mut row);

    assert!(appended);
    let glyph = &row.glyphs[GlyphArea::Text.index()][0];
    assert_eq!(glyph.provenance, GlyphProvenance::line_end());
    assert_eq!(GlyphProvenance::line_end().buffer_charpos(), None);
}

/// With no `:extend` face, GNU's terminal branch still materializes the row
/// tail (`src/xdisp.c:24679-24809`) -- but with the DEFAULT face, and the
/// TTY backend already emits exactly those cells while rasterizing the row
/// (`crates/neomacs-display-runtime/src/backend/tty/rif.rs`, ledger 81). So this
/// seam emitting no fill here is a LAYERING choice, not a divergence: the
/// glyph row differs from GNU's, the resulting terminal cells do not.
///
/// Do not "fix" this by synthesizing a default-face stretch. The backend
/// derives a row tail's blank-erase class from the last non-padding glyph's
/// face, so an extra default-face glyph here would reclassify tails that
/// ledger 81 deliberately keeps `Explicit`.
#[test]
fn terminal_newline_without_extend_appends_only() {
    let plan = plan(&terminal_ctx());
    assert_eq!(
        plan.steps(),
        &[LineEndStep::AppendGlyph {
            ch: ' ',
            face: AppendedGlyphFace::NewlineFace,
        }]
    );
}

/// THE REGRESSION TEST for ledger 94, in both shapes GNU distinguishes.
///
/// GNU's "skip the fill, nothing would show" early return is guarded by
/// `FRAME_WINDOW_P` (`src/xdisp.c:24388`). A terminal frame can never take
/// it, so an `:extend` face whose background happens to equal the frame's
/// STILL fills to the text-area edge -- which is how GNU carries such a
/// face's foreground (Leuven `diff-context`, `#A0A1A7` on `#FFFFFF`) across
/// the row. A window-system frame takes the skip and paints nothing.
///
/// Both arms live in one test on purpose: a fix that filled on neither, or
/// on both, would leave half of this green.
#[test]
fn invisible_extend_fills_on_a_terminal_row_but_is_skipped_on_a_window_system_row() {
    let terminal = with_invisible_extend(terminal_ctx());
    assert_eq!(
        plan(&terminal).steps(),
        &[
            LineEndStep::AppendGlyph {
                ch: ' ',
                face: AppendedGlyphFace::NewlineFace,
            },
            LineEndStep::ExtendFill { from_x: 32.0 },
        ],
        "terminal row: GNU cannot reach the FRAME_WINDOW_P skip, so the \
         fill runs even though the extend background is invisible"
    );

    let mut gui = with_invisible_extend(terminal_ctx());
    gui.measurement_mode = DisplayRowMeasurementMode::ConcreteFont;
    assert_eq!(
        plan(&gui).steps(),
        &[],
        "window-system row: the fill would paint frame background over \
         frame background, so GNU returns early (xdisp.c:24388)"
    );
}

/// The visible-background case must keep filling on BOTH frame types --
/// guards against "fix" that simply inverted the mode test.
#[test]
fn visible_extend_fills_on_both_frame_types() {
    let terminal = with_extend(terminal_ctx());
    assert!(
        plan(&terminal)
            .steps()
            .contains(&LineEndStep::ExtendFill { from_x: 32.0 })
    );

    let mut gui = with_extend(terminal_ctx());
    gui.measurement_mode = DisplayRowMeasurementMode::ConcreteFont;
    assert_eq!(
        plan(&gui).steps(),
        &[LineEndStep::ExtendFill { from_x: 24.0 }],
        "no appended glyph on a window-system row, so the fill starts at \
         the un-advanced pen"
    );
}

#[test]
fn pen_at_indicator_column_merges_indicator_into_appended_glyph() {
    let ctx = with_indicator(with_extend(terminal_ctx()), 3);
    let plan = plan(&ctx);
    assert_eq!(
        plan.steps(),
        &[LineEndStep::AppendGlyph {
            ch: '|',
            face: AppendedGlyphFace::MergedIndicator,
        }],
        "merged indicator: no IndicatorFill and no ExtendFill after it"
    );
}

#[test]
fn pen_before_indicator_column_appends_space_then_indicator_fill_from_advanced_pen() {
    let ctx = with_indicator(terminal_ctx(), 6);
    let plan = plan(&ctx);
    assert_eq!(
        plan.steps(),
        &[
            LineEndStep::AppendGlyph {
                ch: ' ',
                face: AppendedGlyphFace::NewlineFace,
            },
            LineEndStep::IndicatorFill { from_x: 32.0 },
        ]
    );
}

#[test]
fn pen_past_indicator_column_covers_indicator_and_extends_instead() {
    let ctx = with_indicator(with_extend(terminal_ctx()), 2);
    let plan = plan(&ctx);
    assert_eq!(
        plan.steps(),
        &[
            LineEndStep::AppendGlyph {
                ch: ' ',
                face: AppendedGlyphFace::NewlineFace,
            },
            LineEndStep::ExtendFill { from_x: 32.0 },
        ]
    );
}

#[test]
fn gui_rows_get_no_appended_glyph() {
    let mut ctx = with_extend(terminal_ctx());
    ctx.measurement_mode = DisplayRowMeasurementMode::ConcreteFont;
    let plan = plan(&ctx);
    assert_eq!(
        plan.steps(),
        &[LineEndStep::ExtendFill { from_x: 24.0 }],
        "GUI fill starts at the unadvanced pen"
    );
}

#[test]
fn gui_pen_at_indicator_column_produces_indicator_fill_not_a_merge() {
    let mut ctx = with_indicator(terminal_ctx(), 3);
    ctx.measurement_mode = DisplayRowMeasurementMode::ConcreteFont;
    let plan = plan(&ctx);
    assert_eq!(
        plan.steps(),
        &[LineEndStep::IndicatorFill { from_x: 24.0 }],
        "the merge exists only when a glyph is appended (terminal rows)"
    );
}

#[test]
fn full_row_appends_nothing() {
    let mut ctx = terminal_ctx();
    ctx.pen_x = ctx.right_edge_x;
    assert_eq!(plan(&ctx).steps(), &[]);
}

#[test]
fn trailing_whitespace_enabled_puts_highlight_first() {
    let mut ctx = with_extend(terminal_ctx());
    ctx.trailing_whitespace_enabled = true;
    let plan = plan(&ctx);
    assert_eq!(
        plan.steps(),
        &[
            LineEndStep::HighlightTrailingWhitespace,
            LineEndStep::AppendGlyph {
                ch: ' ',
                face: AppendedGlyphFace::NewlineFace,
            },
            LineEndStep::ExtendFill { from_x: 32.0 },
        ]
    );
}

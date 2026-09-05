//! The single typed seam for GNU line-end row semantics.
//!
//! GNU ends a display line (xdisp.c `display_line`, `at_end_of_line`,
//! xdisp.c:26525-26533) with a fixed sequence: highlight trailing whitespace,
//! `append_space_for_newline` (xdisp.c:24122), then
//! `extend_face_to_end_of_line`. Two neomacs paths used to encode that order
//! independently — the buffer-source row lifecycle and the item-renderer
//! [`super::finalizer::DisplayRowLineEndFinalizer`]. [`plan`] is now the one
//! place that decides WHICH effects happen and in WHAT order; callers build a
//! [`LineEndContext`], resolve the returned [`LineEndPlan`] against their face
//! services, and apply the [`ResolvedLineEndPlan`] to the glyph row.
//!
//! A wrapped or truncated row never gets here. GNU calls
//! `append_space_for_newline` only from `display_line`'s
//! `ITERATOR_AT_END_OF_LINE_P` branch, and every overflow exit
//! (xdisp.c:26324-26435) calls `extend_face_to_end_of_line` on its own;
//! neomacs matches that shape, with `buffer_source/overflow.rs` and
//! `buffer_source/render.rs` calling
//! `TextRowSourceRenderState::extend_face_to_end_of_line` directly. So "this
//! is a real line end" is a property of reaching [`plan`] at all, not a field
//! on its context.
//!
//! Rules encoded here (and nowhere else):
//! - the appended newline glyph exists only on terminal rows
//!   ([`DisplayRowMeasurementMode::LogicalCells`]) with room left on the row;
//! - the `display-fill-column-indicator` merge is checked at the PRE-advance
//!   pen column: when the pen sits exactly at the indicator column, the
//!   appended glyph IS the indicator character with the indicator face (GNU
//!   `append_space_for_newline`'s corner case), and no separate indicator
//!   fill runs;
//! - every fill that follows the appended glyph starts from the POST-advance
//!   pen (GNU's PRODUCE_GLYPHS moves `it->current_x`), so fills never overlap
//!   the appended glyph;
//! - the indicator fill replaces the plain `:extend` fill whenever it runs
//!   (it carries the extend background through its gap and tail itself).

use crate::display_current_row_output::DisplayCurrentRowMutation;
use crate::display_row::face_state::DisplayRowMeasurementMode;
use crate::display_row::finalizer::RowExtendFill;
use crate::display_row::trailing_whitespace::HighlightTrailingWhitespaceMutation;
use crate::glyph_row_writer::push_stretch_to_area;
use neomacs_display_protocol::glyph_matrix::{Glyph, GlyphArea, GlyphProvenance, GlyphRow};
use neomacs_display_protocol::types::{Color, FaceId};

/// `display-fill-column-indicator` configuration relevant at a line end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineEndIndicator {
    /// Indicator buffer column (never negative; a disabled indicator is a
    /// `None` on the context, not a sentinel).
    pub(crate) col: i32,
    /// `display-fill-column-indicator-character`.
    pub(crate) ch: char,
}

/// The active `:extend` face at the line end, exactly as face resolution
/// produced it.
///
/// This is deliberately NOT pre-filtered against the frame background. Whether
/// an invisible fill may be skipped is a property of the FRAME TYPE, not of
/// the caller: GNU guards that skip with `FRAME_WINDOW_P`
/// (`src/xdisp.c:24388`), so a terminal frame can never take it. Callers used
/// to apply the background filter themselves, which made a terminal row
/// silently drop a fill GNU always performs. [`extend_fill_runs`] is now the
/// single place that decides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineEndExtend {
    pub(crate) bg: Color,
    pub(crate) face_id: FaceId,
}

/// Does the plain `:extend` fill run for this line end?
///
/// GNU's "nothing would be painted, so skip the fill" early return is guarded
/// by `FRAME_WINDOW_P` (`src/xdisp.c:24388`). A terminal frame cannot reach
/// it, so control always falls through to the terminal fill branch
/// (`src/xdisp.c:24679-24809`), which materializes the row out to the
/// text-area edge regardless of how the face looks — carrying, for instance, a
/// `:extend` face's FOREGROUND even when its background equals the frame's.
///
/// Encoding that as a total match on [`DisplayRowMeasurementMode`] is what
/// makes the omission unrepresentable: the skip is reachable only from the
/// `ConcreteFont` arm, so no caller and no future branch can pre-filter a
/// terminal row's fill away by forgetting the rule.
pub(crate) fn extend_fill_runs(
    mode: DisplayRowMeasurementMode,
    extend: Option<LineEndExtend>,
    frame_background: Color,
) -> bool {
    let Some(extend) = extend else {
        // No `:extend` face is active. GNU's terminal branch still materializes
        // the row tail, but with the DEFAULT face — and the TTY backend already
        // produces exactly those cells when it rasterizes the row
        // (`crates/neomacs-display-runtime/src/backend/tty/rif.rs`, GNU
        // `src/xdisp.c:24681-24758`). Synthesizing a default-face stretch here
        // instead would change that backend's blank-erase classification, which
        // is derived from the last non-padding glyph's face, so the fill stays
        // the backend's job in this case.
        return false;
    };
    match mode {
        DisplayRowMeasurementMode::LogicalCells => true,
        DisplayRowMeasurementMode::ConcreteFont => extend.bg != frame_background,
    }
}

/// Everything the line-end decision needs, as pure data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineEndContext {
    /// Face carried by the newline itself (`it->face_id` in GNU): the
    /// appended terminal space keeps this face.
    pub(crate) newline_face_id: FaceId,
    pub(crate) measurement_mode: DisplayRowMeasurementMode,
    /// Pen x BEFORE any line-end glyph is appended.
    pub(crate) pen_x: f32,
    /// Pen column BEFORE any line-end glyph is appended (the indicator merge
    /// is decided at this column).
    pub(crate) pen_col: i64,
    /// Text-area right edge; fills stop here.
    pub(crate) right_edge_x: f32,
    /// Nominal character-cell advance for the appended glyph and fills.
    pub(crate) char_width: f32,
    pub(crate) indicator: Option<LineEndIndicator>,
    pub(crate) extend: Option<LineEndExtend>,
    /// Frame background, used ONLY by the window-system visibility skip
    /// (GNU `src/xdisp.c:24388`). Required rather than optional so that every
    /// call site is forced to supply it instead of quietly pre-filtering
    /// `extend` against it and losing the terminal fill.
    pub(crate) frame_background: Color,
    pub(crate) trailing_whitespace_enabled: bool,
    /// Source-derived GNU `start_of_box_run_p` / `end_of_box_run_p` for the
    /// line-end element. Visible rows are not sufficient evidence.
    pub(crate) box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    pub(crate) box_run_membership: neomacs_display_protocol::face::BoxRunMembership,
}

impl LineEndContext {
    fn remaining_px(&self) -> f32 {
        self.right_edge_x - self.pen_x
    }

    /// The `:extend` face that actually paints, after GNU's frame-type-gated
    /// visibility skip (`src/xdisp.c:24388`).
    ///
    /// Every consumer of the extend face must go through this rather than the
    /// raw [`Self::extend`] field: the plain fill, the indicator gap/tail, and
    /// the merged-indicator background all have to agree on whether the
    /// highlight is painting, or a row would get an indicator tail in a face
    /// whose fill was skipped.
    fn effective_extend(&self) -> Option<LineEndExtend> {
        extend_fill_runs(self.measurement_mode, self.extend, self.frame_background)
            .then_some(self.extend)
            .flatten()
    }
}

/// Face of the glyph appended by GNU `append_space_for_newline`. The
/// indicator-merge rule is a type: `MergedIndicator` means the appended glyph
/// IS the fill-column indicator (indicator face, indicator character), not a
/// newline-face space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppendedGlyphFace {
    NewlineFace,
    MergedIndicator,
}

/// One ordered line-end effect. `from_x` on the fill steps is already the
/// POST-advance pen (past the appended glyph when one was planned).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LineEndStep {
    HighlightTrailingWhitespace,
    AppendGlyph { ch: char, face: AppendedGlyphFace },
    IndicatorFill { from_x: f32 },
    ExtendFill { from_x: f32 },
}

/// Ordered line-end effects for one row.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct LineEndPlan {
    steps: Vec<LineEndStep>,
}

impl LineEndPlan {
    pub(crate) fn steps(&self) -> &[LineEndStep] {
        &self.steps
    }
}

/// Decide the ordered line-end effects for `ctx`. This is THE single encoding
/// of the GNU line-end order; both the buffer-source lifecycle and the
/// item-renderer finalizer consume it.
pub(crate) fn plan(ctx: &LineEndContext) -> LineEndPlan {
    let mut steps = Vec::new();

    // GNU highlights trailing whitespace before the appended glyph (the
    // appended glyph has no buffer position, so GNU's walk would skip it —
    // appending first would still be wrong for order).
    if ctx.trailing_whitespace_enabled {
        steps.push(LineEndStep::HighlightTrailingWhitespace);
    }

    let indicator = ctx
        .indicator
        .filter(|indicator| indicator.col >= 0 && ctx.char_width > 0.0);

    // GNU append_space_for_newline: terminal rows only, and only when the row
    // has room left.
    let append =
        ctx.measurement_mode == DisplayRowMeasurementMode::LogicalCells && ctx.remaining_px() > 0.0;
    // Indicator merge is decided at the PRE-advance pen column.
    let merged =
        append && indicator.is_some_and(|indicator| ctx.pen_col == i64::from(indicator.col));
    if append {
        let (ch, face) = match (merged, indicator) {
            (true, Some(indicator)) => (indicator.ch, AppendedGlyphFace::MergedIndicator),
            _ => (' ', AppendedGlyphFace::NewlineFace),
        };
        steps.push(LineEndStep::AppendGlyph { ch, face });
    }

    // Every later fill starts from the POST-advance pen.
    let from_x = ctx.pen_x + if append { ctx.char_width } else { 0.0 };

    if merged {
        // The appended glyph already was the indicator; neither the indicator
        // fill nor the plain extend fill runs after it.
        return LineEndPlan { steps };
    }
    if let Some(indicator) = indicator {
        // Visibility is decided by COLUMN: the indicator shows whenever the
        // row text ends at or before the indicator column. When it runs it
        // owns the whole trailing region (gap + indicator + tail), including
        // any `:extend` background, so the plain extend fill is skipped.
        if ctx.pen_col <= i64::from(indicator.col) {
            steps.push(LineEndStep::IndicatorFill { from_x });
            return LineEndPlan { steps };
        }
    }
    if ctx.effective_extend().is_some() && ctx.right_edge_x - from_x > 0.0 {
        steps.push(LineEndStep::ExtendFill { from_x });
    }
    LineEndPlan { steps }
}

/// Vertical/horizontal geometry the fills need but the plan does not decide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineEndFillGeometry {
    /// Buffer-text origin x (the indicator pixel x is
    /// `content_x + col * char_width`).
    pub(crate) content_x: f32,
    pub(crate) height_px: f32,
    pub(crate) ascent_px: f32,
    /// Character advance used for the extend fill's column accounting (may
    /// come from the extend face's own metrics, unlike the appended glyph's
    /// `LineEndContext::char_width`).
    pub(crate) fill_char_width: f32,
}

/// Face-resolution services the executor needs. The named-face lookups
/// (`trailing-whitespace`, `fill-column-indicator`) stay behind this trait so
/// the seam itself has no resolver dependency; contexts whose plan can never
/// contain those steps (no indicator, trailing disabled) may implement them
/// as unreachable.
pub(crate) trait LineEndFaceResolver {
    fn trailing_whitespace_face_id(&mut self) -> FaceId;
    /// Resolve the `fill-column-indicator` face, with its background
    /// overridden by `extend_bg` when an `:extend` highlight is active so the
    /// highlight stays continuous through the indicator (GNU merges
    /// `fill-column-indicator` over the extend face).
    fn fill_column_indicator_face_id(&mut self, extend_bg: Option<Color>) -> FaceId;
}

/// Resolver for degenerate contexts (no indicator, no trailing whitespace):
/// [`plan`] provably never emits a step that needs a named face from such a
/// context.
pub(crate) struct NoNamedLineEndFaces;

impl LineEndFaceResolver for NoNamedLineEndFaces {
    fn trailing_whitespace_face_id(&mut self) -> FaceId {
        unreachable!("plan() emits no trailing-whitespace step without the enabled flag")
    }

    fn fill_column_indicator_face_id(&mut self, _extend_bg: Option<Color>) -> FaceId {
        unreachable!("plan() emits no indicator step without an indicator config")
    }
}

/// A [`LineEndStep`] with its faces and geometry resolved: pure row effects.
#[derive(Clone, Copy, Debug)]
enum ResolvedLineEndStep {
    HighlightTrailingWhitespace {
        face_id: FaceId,
    },
    AppendGlyph {
        ch: char,
        face_id: FaceId,
        char_width: f32,
    },
    IndicatorFill(FillColumnIndicatorFill),
    ExtendFill(RowExtendFill),
}

/// The one line-end executor: ordered resolved effects applied to a
/// [`GlyphRow`]. Also usable as a current-row mutation by the buffer-source
/// output path.
#[derive(Clone, Debug, Default)]
pub(crate) struct ResolvedLineEndPlan {
    steps: Vec<ResolvedLineEndStep>,
    box_vertical_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    box_run_membership: neomacs_display_protocol::face::BoxRunMembership,
}

impl LineEndPlan {
    /// Resolve faces and fill geometry for every planned step.
    pub(crate) fn resolve<R: LineEndFaceResolver>(
        &self,
        ctx: &LineEndContext,
        geometry: LineEndFillGeometry,
        resolver: &mut R,
    ) -> ResolvedLineEndPlan {
        let steps = self
            .steps
            .iter()
            .map(|step| resolve_step(*step, ctx, geometry, resolver))
            .collect();
        ResolvedLineEndPlan {
            steps,
            box_vertical_edges: ctx.box_vertical_edges,
            box_run_membership: ctx.box_run_membership,
        }
    }
}

fn resolve_step<R: LineEndFaceResolver>(
    step: LineEndStep,
    ctx: &LineEndContext,
    geometry: LineEndFillGeometry,
    resolver: &mut R,
) -> ResolvedLineEndStep {
    match step {
        LineEndStep::HighlightTrailingWhitespace => {
            ResolvedLineEndStep::HighlightTrailingWhitespace {
                face_id: resolver.trailing_whitespace_face_id(),
            }
        }
        LineEndStep::AppendGlyph { ch, face } => {
            let face_id = match face {
                AppendedGlyphFace::NewlineFace => ctx.newline_face_id,
                AppendedGlyphFace::MergedIndicator => resolver
                    .fill_column_indicator_face_id(ctx.effective_extend().map(|extend| extend.bg)),
            };
            ResolvedLineEndStep::AppendGlyph {
                ch,
                face_id,
                char_width: ctx.char_width,
            }
        }
        LineEndStep::IndicatorFill { from_x } => ResolvedLineEndStep::IndicatorFill(
            resolve_indicator_fill(ctx, geometry, from_x, resolver),
        ),
        LineEndStep::ExtendFill { from_x } => {
            let extend = ctx
                .effective_extend()
                .expect("plan() emits ExtendFill only with an extend config");
            ResolvedLineEndStep::ExtendFill(RowExtendFill::new(
                extend.bg,
                extend.face_id,
                ctx.right_edge_x - from_x,
                geometry.height_px,
                geometry.ascent_px,
                geometry.fill_char_width,
            ))
        }
    }
}

/// Build the indicator fill exactly as GNU's `extend_face_to_end_of_line`
/// trailing region does (xdisp.c:24752): a gap stretch up to the indicator
/// column, the indicator character, and — under an `:extend` highlight — a
/// tail stretch to the right edge carrying the extend face.
fn resolve_indicator_fill<R: LineEndFaceResolver>(
    ctx: &LineEndContext,
    geometry: LineEndFillGeometry,
    from_x: f32,
    resolver: &mut R,
) -> FillColumnIndicatorFill {
    let indicator = ctx
        .indicator
        .expect("plan() emits IndicatorFill only with an indicator config");
    let char_width = ctx.char_width;
    let indicator_px = geometry.content_x + indicator.col as f32 * char_width;
    // Positioning stays pixel-based: when the text ends exactly at the
    // indicator column, `from_x` may be a hair past `indicator_px`, so clamp
    // the gap to 0 and place the indicator right after the text (as GNU does).
    let gap_px = (indicator_px - from_x).max(0.0);
    let gap_cols = (gap_px / char_width).round().clamp(0.0, u16::MAX as f32) as u16;
    match ctx.effective_extend() {
        Some(extend) => {
            let indicator_face_id = resolver.fill_column_indicator_face_id(Some(extend.bg));
            let tail_px = (ctx.right_edge_x - (indicator_px + char_width)).max(0.0);
            let tail_cols = (tail_px / char_width).round().clamp(0.0, u16::MAX as f32) as u16;
            FillColumnIndicatorFill {
                gap_px,
                gap_cols,
                gap_face_id: extend.face_id,
                indicator_char: indicator.ch,
                indicator_face_id,
                tail_px,
                tail_cols,
                tail_face_id: extend.face_id,
                char_width,
                height_px: geometry.height_px,
                ascent_px: geometry.ascent_px,
            }
        }
        None => {
            // Plain row: the gap is transparent (fill-column-indicator has no
            // background) and there is no tail past the indicator.
            let face_id = resolver.fill_column_indicator_face_id(None);
            FillColumnIndicatorFill {
                gap_px,
                gap_cols,
                gap_face_id: face_id,
                indicator_char: indicator.ch,
                indicator_face_id: face_id,
                tail_px: 0.0,
                tail_cols: 0,
                tail_face_id: face_id,
                char_width,
                height_px: geometry.height_px,
                ascent_px: geometry.ascent_px,
            }
        }
    }
}

impl ResolvedLineEndPlan {
    /// Apply the resolved plan to `row` — the single line-end executor shared
    /// by the buffer-source lifecycle and the item-renderer finalizer.
    pub(crate) fn apply_to(self, row: &mut GlyphRow) {
        let text_index = GlyphArea::Text.index();
        let before_len = row.glyphs[text_index].len();
        for step in self.steps {
            match step {
                ResolvedLineEndStep::HighlightTrailingWhitespace { face_id } => {
                    HighlightTrailingWhitespaceMutation { face_id }.apply(row);
                }
                ResolvedLineEndStep::AppendGlyph {
                    ch,
                    face_id,
                    char_width,
                } => {
                    AppendNewlineGlyphMutation {
                        ch,
                        face_id,
                        char_width,
                    }
                    .apply(row);
                }
                ResolvedLineEndStep::IndicatorFill(fill) => {
                    FillColumnIndicatorMutation { fill }.apply(row);
                }
                ResolvedLineEndStep::ExtendFill(fill) => {
                    fill.apply_to(row);
                }
            }
        }
        super::builder::apply_box_run_topology_to_glyphs(
            &mut row.glyphs[text_index][before_len..],
            self.box_run_membership,
            self.box_vertical_edges,
            true,
        );
    }
}

impl DisplayCurrentRowMutation for ResolvedLineEndPlan {
    type Output = ();

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        self.apply_to(row);
    }
}

/// GNU `append_space_for_newline` (xdisp.c:24122) for terminal rows: append
/// one glyph at a real line end. Normally a space carrying the NEWLINE's
/// face, so a face spanning the newline (a font-lock comment, a string)
/// paints its foreground on the end-of-line cell exactly as GNU does. When
/// the pen sits exactly at the `display-fill-column-indicator` column, GNU
/// makes this same appended glyph BE the indicator character with the
/// indicator face merged over the newline's face -- the space and the
/// indicator are one glyph, not two.
struct AppendNewlineGlyphMutation {
    ch: char,
    face_id: FaceId,
    char_width: f32,
}

impl DisplayCurrentRowMutation for AppendNewlineGlyphMutation {
    type Output = bool;

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        debug_assert!(!row.reversed_p, "line-end glyphs precede bidi finalization");
        let text_index = GlyphArea::Text.index();
        row.glyphs[text_index].push(
            Glyph::char_with_provenance(self.ch, self.face_id, GlyphProvenance::line_end())
                .with_pixel_width(self.char_width.max(1.0)),
        );
        true
    }
}

/// Geometry + faces for the `display-fill-column-indicator` glyph produced in a
/// row's trailing region (GNU `extend_face_to_end_of_line`, xdisp.c:24752): a
/// `gap` stretch pads from end-of-text to the indicator column, the indicator
/// character carries the `fill-column-indicator` face, and an optional `tail`
/// stretch continues to the right edge. On a plain row the gap is transparent
/// and there is no tail; on an `:extend`-highlighted row (region/hl-line) the
/// gap and tail carry the extend face so the whole trailing region stays
/// highlighted, and the indicator char face keeps the highlight background.
#[derive(Clone, Copy, Debug)]
pub(super) struct FillColumnIndicatorFill {
    pub(super) gap_px: f32,
    pub(super) gap_cols: u16,
    pub(super) gap_face_id: FaceId,
    pub(super) indicator_char: char,
    pub(super) indicator_face_id: FaceId,
    pub(super) tail_px: f32,
    pub(super) tail_cols: u16,
    pub(super) tail_face_id: FaceId,
    pub(super) char_width: f32,
    pub(super) height_px: f32,
    pub(super) ascent_px: f32,
}

pub(super) struct FillColumnIndicatorMutation {
    pub(super) fill: FillColumnIndicatorFill,
}

impl DisplayCurrentRowMutation for FillColumnIndicatorMutation {
    type Output = ();

    fn apply(self, row: &mut GlyphRow) -> Self::Output {
        debug_assert!(
            !row.reversed_p,
            "line-end fill must precede bidi finalization"
        );
        let text_index = GlyphArea::Text.index();
        let f = self.fill;
        // Pad the trailing region up to the indicator column.
        if f.gap_cols >= 1 && f.gap_px > 0.5 {
            push_stretch_to_area(
                row,
                text_index,
                f.gap_cols,
                f.gap_face_id,
                f.gap_px,
                f.height_px,
                f.ascent_px,
                GlyphProvenance::line_end(),
            );
        }
        // The indicator character itself. It maps to no buffer position, so the
        // blank-line cursor never latches onto it.
        row.glyphs[text_index].push(
            Glyph::char_with_provenance(
                f.indicator_char,
                f.indicator_face_id,
                GlyphProvenance::line_end(),
            )
            .with_pixel_width(f.char_width.max(1.0)),
        );
        // Continue the `:extend` highlight past the indicator to the right edge.
        if f.tail_cols >= 1 && f.tail_px > 0.5 {
            push_stretch_to_area(
                row,
                text_index,
                f.tail_cols,
                f.tail_face_id,
                f.tail_px,
                f.height_px,
                f.ascent_px,
                GlyphProvenance::line_end(),
            );
        }
        row.displays_text = true;
    }
}

#[cfg(test)]
#[path = "fill_column_indicator_test.rs"]
mod fill_column_indicator_tests;

#[cfg(test)]
mod tests {
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
}

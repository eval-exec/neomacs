use crate::buffer_source::producer::vocabulary::{
    ProducedGlyphProvenance, natural_text_glyph, source_mapped_text_glyph,
};
use crate::composition::base_width_cols;
use crate::display_face_ref::render_face_ref_id;
use crate::display_item::{
    DisplayGlyphless, DisplayItem, DisplayItemKind, DisplayItemLayout, DisplayLength,
    DisplayMediaReplacement, DisplayMediaReplacementKind, DisplaySourcePosition, DisplayStretch,
    DisplayStretchWidth, DisplayTextComposition, GlyphlessMethod, RenderFaceRef, SourceSpan,
    control_char_caret_char,
};
use crate::display_pixel_calc::{PixelCalcContext, calc_pixel_width_or_height};
use crate::display_row::append_context::{
    DisplayRowTextCharState, DisplayRowTextNaturalAdvanceKind, DisplayRowTextNaturalAdvancePolicy,
    DisplayRowTextNaturalAdvanceRequest,
};
use crate::display_row::geometry::DisplayRowTextAreaOrigin;
#[cfg(test)]
use crate::display_source::{DisplayItemSource, DisplaySourceContext};
use crate::display_source_overflow::{DisplayXwidgetOverflowAction, WindowLocalRowExtent};
use crate::glyph_row_writer;
#[cfg(test)]
use crate::output::builder::DisplayOutputBuilder;
use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{
    Glyph, GlyphArea, GlyphProvenance, GlyphRow, GlyphStringSource, GlyphType,
    NO_BUFFER_POSITION_CHARPOS, TerminalComposition,
};
use neomacs_display_protocol::types::FaceId;

use crate::display_text_run_measurement::DisplayTextRunMeasurement;

/// Stamp one source item's affine GNU box-run terminals onto the concrete
/// glyph slice it produced. Padding never owns an edge; clipping can suppress
/// the source end until the item is completed on a later row.
pub(super) fn apply_box_run_topology_to_glyphs(
    glyphs: &mut [Glyph],
    membership: neomacs_display_protocol::face::BoxRunMembership,
    source_edges: neomacs_display_protocol::face::BoxVerticalEdges,
    source_item_complete: bool,
) {
    for glyph in glyphs.iter_mut() {
        glyph.box_vertical_edges =
            neomacs_display_protocol::face::BoxVerticalEdges::Neither.with_membership(membership);
    }
    if !membership.is_boxed() {
        return;
    }
    let Some(first) = glyphs.iter().position(|glyph| !glyph.padding) else {
        return;
    };
    let last = glyphs
        .iter()
        .rposition(|glyph| !glyph.padding)
        .expect("a first non-padding glyph has a last glyph");
    glyphs[first].box_vertical_edges =
        neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(
            source_edges.owns_left(),
            source_item_complete && first == last && source_edges.owns_right(),
        );
    if last != first {
        glyphs[last].box_vertical_edges =
            neomacs_display_protocol::face::BoxVerticalEdges::from_ownership(
                false,
                source_item_complete && source_edges.owns_right(),
            );
    }
}

/// Which axis a `(space …)` length measures — GNU's `width_p` argument to
/// `calc_pixel_width_or_height`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PixelCalcAxis {
    Horizontal,
    Vertical,
}

impl PixelCalcAxis {
    const fn is_horizontal(self) -> bool {
        matches!(self, Self::Horizontal)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisplayRowLayout {
    pub(crate) role: GlyphRowRole,
    pub(crate) y_px: f32,
    pub(crate) height_px: f32,
    pub(crate) ascent_px: f32,
    pub(crate) char_width_px: f32,
    pub(crate) tab_policy: DisplayTabPolicy,
    pub(crate) base_face: RenderFaceRef,
    /// Window/face/frame pixel state for the single GNU-faithful
    /// `(space :width/:align-to …)` evaluator. Mode-line, header-line and
    /// tab-line rows resolve region symbols (`text`, `right`, fringes, …)
    /// through this context, the same authority the buffer text path uses.
    pub(crate) pixel_calc: PixelCalcContext,
    /// Window inputs needed to resolve `(image …)` operands appearing inside a
    /// `(space :width/:align-to …)` expression on this row. `None` for rows
    /// built without window context (tests, terminal frames), which matches
    /// GNU's `FRAME_WINDOW_P` guard: the operand fails and `:align-to` is not
    /// applied.
    pub(crate) space_image_params: Option<crate::display_pixel_calc::PixelCalcImageInputs>,
}

impl DisplayRowLayout {
    /// The pixel-calc context for evaluating `spec`, with any `(image …)`
    /// operands it contains resolved to real pixel sizes.
    fn pixel_calc_for_space_spec(&self, spec: &neovm_core::emacs_core::Value) -> PixelCalcContext {
        let Some(inputs) = self.space_image_params.as_ref() else {
            return self.pixel_calc.clone();
        };
        let sizes =
            crate::display_pixel_calc::PixelCalcImageSizes::resolve_for_space_spec(spec, inputs);
        if sizes.is_empty() {
            return self.pixel_calc.clone();
        }
        let mut pixel_calc = self.pixel_calc.clone();
        pixel_calc.image_sizes = sizes;
        pixel_calc
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayRowVerticalMetrics {
    height_px: f32,
    ascent_px: f32,
}

impl DisplayRowVerticalMetrics {
    pub(crate) fn new(height_px: f32, ascent_px: f32) -> Self {
        Self {
            height_px,
            ascent_px,
        }
    }

    #[cfg(test)]
    pub(crate) fn from_row(row: &GlyphRow) -> Self {
        Self::new(row.height_px, row.ascent_px)
    }

    pub(crate) fn height_px(self) -> f32 {
        self.height_px
    }

    pub(crate) fn ascent_px(self) -> f32 {
        self.ascent_px
    }

    fn from_glyph(glyph: &Glyph) -> Option<Self> {
        (glyph.pixel_height > 0.0).then(|| Self::new(glyph.pixel_height, glyph.pixel_ascent))
    }

    fn with_vertical_offset(self, offset_px: f32) -> Self {
        let ascent = self.ascent_px.max(0.0).min(self.height_px.max(0.0));
        let descent = (self.height_px - ascent).max(0.0);
        let shifted_ascent = (ascent - offset_px).max(0.0);
        let shifted_descent = (descent + offset_px).max(0.0);
        Self::new(shifted_ascent + shifted_descent, shifted_ascent)
    }

    pub(crate) fn include_in_row(self, row: &mut GlyphRow) {
        if self.height_px <= 0.0 {
            return;
        }
        let row_descent = (row.height_px - row.ascent_px).max(0.0);
        let glyph_ascent = self.ascent_px.max(0.0);
        let glyph_descent = (self.height_px - glyph_ascent).max(0.0);
        row.ascent_px = row.ascent_px.max(glyph_ascent);
        row.height_px = (row.ascent_px + row_descent.max(glyph_descent)).max(1.0);
    }
}

fn display_row_glyph_count(row: &GlyphRow) -> usize {
    row.glyphs.iter().map(Vec::len).sum()
}

impl DisplayRowLayout {
    fn natural_text_advance_policy(&self) -> DisplayRowTextNaturalAdvancePolicy {
        DisplayRowTextNaturalAdvancePolicy::new(self.tab_policy.clone())
    }

    fn row_height_px(&self) -> f32 {
        self.height_px.max(1.0)
    }

    fn row_ascent_px(&self) -> f32 {
        self.ascent_px.max(0.0).min(self.row_height_px())
    }

    fn apply_to_row(&self, row: &mut GlyphRow) {
        row.enabled = true;
        row.role = self.role;
        row.mode_line = matches!(self.role, GlyphRowRole::ModeLine);
        row.pixel_y = self.y_px;
        let layout_metrics =
            DisplayRowVerticalMetrics::new(self.row_height_px(), self.row_ascent_px());
        if display_row_glyph_count(row) == 0 {
            row.height_px = self.row_height_px();
            row.ascent_px = self.row_ascent_px();
        } else {
            layout_metrics.include_in_row(row);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayTabPolicy {
    pub(crate) origin_x_px: f32,
    pub(crate) width_cols: u16,
    pub(crate) stop_cols: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DisplayTabAdvance {
    pub(crate) pixel_width: f32,
    pub(crate) width_cols: usize,
}

#[derive(Clone, Copy, Debug)]
struct DisplayRowTextCharAdvanceRequest<'a> {
    char_state: DisplayRowTextCharState,
    face_id: FaceId,
    position: DisplayRowPosition,
    char_offset: usize,
    byte_offset: usize,
    measurement: &'a DisplayTextRunMeasurement,
}

impl<'a> DisplayRowTextCharAdvanceRequest<'a> {
    fn new(
        char_state: DisplayRowTextCharState,
        face_id: FaceId,
        position: DisplayRowPosition,
        char_offset: usize,
        byte_offset: usize,
        measurement: &'a DisplayTextRunMeasurement,
    ) -> Self {
        Self {
            char_state,
            face_id,
            position,
            char_offset,
            byte_offset,
            measurement,
        }
    }

    fn per_char(
        char_state: DisplayRowTextCharState,
        face_id: FaceId,
        position: DisplayRowPosition,
        measurement: &'a DisplayTextRunMeasurement,
    ) -> Self {
        Self::new(char_state, face_id, position, 0, 0, measurement)
    }

    fn ch(self) -> char {
        self.char_state.ch()
    }

    fn kind(self) -> DisplayRowTextNaturalAdvanceKind {
        self.char_state.kind()
    }

    fn natural_advance_request(self) -> DisplayRowTextNaturalAdvanceRequest {
        self.char_state
            .natural_advance_request(self.position, self.face_id)
    }

    fn measured_advance(self) -> Option<f32> {
        self.measurement
            .advance_for(self.char_offset, self.byte_offset)
    }

    fn resolve_advance_px(
        self,
        policy: &DisplayRowTextNaturalAdvancePolicy,
        glyph_advance_px: impl FnMut(char, FaceId, usize) -> f32,
    ) -> f32 {
        let measured = match self.kind() {
            DisplayRowTextNaturalAdvanceKind::Tab
            | DisplayRowTextNaturalAdvanceKind::ClusterContinuation
            | DisplayRowTextNaturalAdvanceKind::StandaloneZeroWidth => None,
            DisplayRowTextNaturalAdvanceKind::ComplexRunMember
            | DisplayRowTextNaturalAdvanceKind::FaceColumns { .. } => self.measured_advance(),
        };
        measured.unwrap_or_else(|| {
            policy.resolve_with(self.natural_advance_request(), glyph_advance_px)
        })
    }

    fn resolve_advance_px_with_writer(self, writer: &mut DisplayRowWriter<'_, '_, '_>) -> f32 {
        let policy = writer.layout.natural_text_advance_policy();
        self.resolve_advance_px(&policy, |ch, face_id, columns| {
            writer.glyph_advance_px(ch, face_id, columns)
        })
    }
}

impl DisplayTabPolicy {
    pub(crate) fn every(width_cols: u16) -> Self {
        Self {
            origin_x_px: 0.0,
            width_cols: width_cols.max(1),
            stop_cols: Vec::new(),
        }
    }

    pub(crate) fn from_tab_width_and_stops(
        origin_x_px: f32,
        tab_width: i32,
        tab_stop_list: &[i32],
    ) -> Self {
        Self {
            origin_x_px,
            width_cols: tab_width.max(1).min(i32::from(u16::MAX)) as u16,
            stop_cols: tab_stop_list
                .iter()
                .copied()
                .filter(|stop| *stop >= 0)
                .map(|stop| stop as usize)
                .collect(),
        }
    }

    pub(crate) fn advance_from(
        &self,
        position: DisplayRowPosition,
        char_width_px: f32,
    ) -> DisplayTabAdvance {
        let char_width = TabGridPixel::from_renderer_px(char_width_px.max(1.0));
        let tab_width = char_width * i64::from(self.width_cols.max(1));
        let row_x = TabGridPixel::from_renderer_px((position.x_px - self.origin_x_px).max(0.0));
        let tab_x = row_x + position.tab_coordinates.offset();
        let next_tab_x = if !self.stop_cols.is_empty() {
            self.stop_cols
                .iter()
                .copied()
                .map(|stop| char_width * i64::try_from(stop).unwrap_or(i64::MAX))
                .find(|stop| *stop > tab_x)
                .unwrap_or_else(|| {
                    let last = char_width
                        * i64::try_from(self.stop_cols.last().copied().unwrap_or_default())
                            .unwrap_or(i64::MAX);
                    if tab_x >= last && tab_width > TabGridPixel::ZERO {
                        let repeated_stops = (tab_x - last).raw() / tab_width.raw() + 1;
                        last + tab_width * repeated_stops
                    } else {
                        last
                    }
                })
        } else if tab_width > TabGridPixel::ZERO {
            tab_width * (tab_x.raw() / tab_width.raw() + 1)
        } else {
            tab_x + char_width
        };
        // GNU's minimum-one-space guard (gui_produce_glyphs: a tab landing
        // exactly on a stop advances a full stop) works in integer pixels.
        // `TabGridPixel' makes that domain distinct from both renderer f32
        // coordinates and the protocol's subpixel `LayoutUnit'.
        let next_tab_x = if next_tab_x - tab_x < char_width {
            next_tab_x + tab_width
        } else {
            next_tab_x
        };
        // `next_tab_x` is in the physical-line coordinate. Translate its
        // target back to this screen row before subtracting the renderer's
        // unrounded f32 pen. This preserves GNU's integer tab grid without
        // introducing subpixel drift (12.15px still lands at pixel 24), and
        // it keeps the logical column target local to the current row.
        let screen_target_x = next_tab_x - position.tab_coordinates.offset();
        let target_x_px = self.origin_x_px + screen_target_x.to_renderer_px();
        let pixel_width = (target_x_px - position.x_px).max(0.0);
        let next_col =
            usize::try_from((screen_target_x.raw() + char_width.raw() / 2) / char_width.raw())
                .unwrap_or(usize::MAX)
                .max(position.col.saturating_add(1));
        DisplayTabAdvance {
            pixel_width,
            width_cols: next_col.saturating_sub(position.col).max(1),
        }
    }
}

/// An integer pixel in GNU Emacs's tab-stop coordinate system.
///
/// Glyph shaping and GPU placement remain subpixel, but GNU xdisp computes
/// TAB stops with integer `current_x' and integer `font->space_width'.  Keeping
/// this as a separate type prevents a deterministic fixed-point coordinate
/// from being mistaken for GNU's integer-pixel domain.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct TabGridPixel(i64);

impl TabGridPixel {
    const ZERO: Self = Self(0);

    fn from_renderer_px(px: f32) -> Self {
        Self(px.round() as i64)
    }

    const fn raw(self) -> i64 {
        self.0
    }

    fn to_renderer_px(self) -> f32 {
        self.0 as f32
    }
}

/// Which horizontal coordinate system a TAB uses.
///
/// GNU measures ordinary continuation-row tabs in the physical line, but
/// measures tabs inside a wrap-prefix string from the current screen row.
/// Making the choice explicit prevents a row-local x coordinate from silently
/// standing in for `continuation_lines_width`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum DisplayTabCoordinates {
    #[default]
    ScreenLine,
    ContinuedPhysicalLine {
        offset: TabGridPixel,
    },
}

impl DisplayTabCoordinates {
    const fn offset(self) -> TabGridPixel {
        match self {
            Self::ScreenLine => TabGridPixel::ZERO,
            Self::ContinuedPhysicalLine { offset } => offset,
        }
    }
}

/// Walk-scoped GNU `continuation_lines_width` / `wrap_prefix_width` state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DisplayPhysicalLineTabState {
    continuation_width: TabGridPixel,
    wrap_prefix_width: TabGridPixel,
}

impl DisplayPhysicalLineTabState {
    pub(crate) fn coordinates(self) -> DisplayTabCoordinates {
        let offset = self.continuation_width - self.wrap_prefix_width;
        if offset != TabGridPixel::ZERO {
            DisplayTabCoordinates::ContinuedPhysicalLine { offset }
        } else {
            DisplayTabCoordinates::ScreenLine
        }
    }

    pub(crate) fn continue_after_visual_row(&mut self, width_px: f32) {
        self.continuation_width =
            self.continuation_width + TabGridPixel::from_renderer_px(width_px.max(0.0));
        self.wrap_prefix_width = TabGridPixel::ZERO;
    }

    pub(crate) fn record_wrap_prefix(&mut self, width_px: f32) {
        self.wrap_prefix_width = TabGridPixel::from_renderer_px(width_px.max(0.0));
    }

    pub(crate) fn reset_for_physical_line(&mut self) {
        *self = Self::default();
    }
}

impl std::ops::Add for TabGridPixel {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

impl std::ops::Sub for TabGridPixel {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }
}

impl std::ops::Mul<i64> for TabGridPixel {
    type Output = Self;

    fn mul(self, factor: i64) -> Self {
        Self(self.0.saturating_mul(factor))
    }
}

pub(crate) trait DisplayGlyphMeasurer {
    fn glyph_advance_px(
        &mut self,
        ch: char,
        face_id: FaceId,
        columns: u8,
        fallback_advance_px: f32,
    ) -> Option<f32>;

    fn glyph_vertical_metrics_px(
        &mut self,
        _ch: char,
        _face_id: FaceId,
    ) -> Option<DisplayRowVerticalMetrics> {
        None
    }

    /// Global metrics of the face's primary font, independent of which font
    /// would cover a concrete character.  GNU uses these for stretch glyphs
    /// produced by `(space-width ...)`.
    fn face_vertical_metrics_px(&mut self, _face_id: FaceId) -> Option<DisplayRowVerticalMetrics> {
        None
    }

    /// The face primary font's space advance.  This deliberately differs
    /// from `glyph_advance_px(' ', ...)`, which may select an ASCII fallback
    /// when the requested face is a symbol-only font.
    fn face_space_width_px(&mut self, _face_id: FaceId) -> Option<f32> {
        None
    }

    /// The coordinate domain GNU uses for display geometry.  This controls
    /// both which modifiers are meaningful and where integer truncation
    /// happens; callers must exhaustively choose pixels or terminal cells.
    fn display_geometry_backend(&self) -> DisplayGeometryBackend {
        DisplayGeometryBackend::WindowSystemPixels
    }

    fn text_run_advances_px(
        &mut self,
        _text: &str,
        _face_id: FaceId,
        _fallback_char_width_px: f32,
    ) -> DisplayTextRunMeasurement {
        DisplayTextRunMeasurement::PerChar
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayGeometryBackend {
    WindowSystemPixels,
    TerminalCells,
}

pub(crate) enum DisplayRowItemMeasurement {
    Default,
    TextRun(DisplayTextRunMeasurement),
}

#[cfg(test)]
pub(crate) struct FixedGlyphAdvance {
    ch: char,
    face_id: FaceId,
    advance_px: f32,
}

#[cfg(test)]
impl FixedGlyphAdvance {
    pub(crate) fn new(ch: char, face_id: FaceId, advance_px: f32) -> Self {
        Self {
            ch,
            face_id,
            advance_px,
        }
    }
}

#[cfg(test)]
impl DisplayGlyphMeasurer for FixedGlyphAdvance {
    fn glyph_advance_px(
        &mut self,
        ch: char,
        face_id: FaceId,
        _columns: u8,
        _fallback_advance_px: f32,
    ) -> Option<f32> {
        (self.ch == ch && self.face_id == face_id).then_some(self.advance_px)
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct FixedGlyphAdvances {
    advances: std::collections::HashMap<(char, FaceId), f32>,
}

#[cfg(test)]
impl FixedGlyphAdvances {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn insert(&mut self, ch: char, face_id: FaceId, advance_px: f32) {
        self.advances.insert((ch, face_id), advance_px);
    }
}

#[cfg(test)]
impl DisplayGlyphMeasurer for FixedGlyphAdvances {
    fn glyph_advance_px(
        &mut self,
        ch: char,
        face_id: FaceId,
        _columns: u8,
        _fallback_advance_px: f32,
    ) -> Option<f32> {
        self.advances.get(&(ch, face_id)).copied()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DisplayRowWriteMetrics {
    width_px: f32,
    width_cols: usize,
}

impl DisplayRowWriteMetrics {
    pub(crate) const fn new(width_px: f32, width_cols: usize) -> Self {
        Self {
            width_px,
            width_cols,
        }
    }

    pub(crate) fn width_px(self) -> f32 {
        self.width_px
    }

    pub(crate) fn width_cols(self) -> usize {
        self.width_cols
    }

    pub(crate) fn has_positive_width(self) -> bool {
        self.width_px > 0.0
    }

    pub(crate) fn is_empty(self) -> bool {
        self.width_px <= 0.0 && self.width_cols == 0
    }

    fn from_glyphs(glyphs: &[Glyph], char_width_px: f32) -> Self {
        glyphs.iter().fold(Self::default(), |mut metrics, glyph| {
            // A complex-run member's padding cell carries its own grapheme (a
            // non-blank Char or Composite) plus a POSITIVE `pixel_width` so the
            // GUI advances x and the TTY can decompose the run one cell each.
            // But the run's base `Composite` already accounts for the whole
            // run's width via `composed_cluster_cols` (= GNU's `cmp->width`,
            // set once in `produce_composite_glyph`, src/term.c:1859; the
            // caller advances `it->current_x` by it a single time, :1762).
            // Counting these padding cells again would double-count the run
            // (the etc/HELLO Arabic/Indic left-shift): so they contribute 0
            // cols and 0 px, just like the old zero-width padding shape did.
            if glyph_row_writer::is_run_member_padding(glyph) {
                return metrics;
            }
            let width_cols = match &glyph.glyph_type {
                GlyphType::Stretch { width_cols } => usize::from((*width_cols).max(1)),
                GlyphType::AutomaticComposite { terminal, .. } => usize::from(terminal.width_cols),
                GlyphType::Image { width_cols, .. }
                | GlyphType::Video { width_cols, .. }
                | GlyphType::Xwidget { width_cols, .. } => usize::from((*width_cols).max(1)),
                GlyphType::Glyphless { .. } if glyph.pixel_width > 0.0 => {
                    (glyph.pixel_width / char_width_px.max(1.0)).ceil().max(1.0) as usize
                }
                // A composed grapheme cluster advances the column by GNU's
                // `cmp->width` (= `string-width` of the cluster), not a single
                // cell — combining marks within it contribute 0.
                GlyphType::Composite { text } => crate::composition::composed_cluster_cols(text),
                GlyphType::Char { ch } if base_width_cols(*ch) == 0 => 0,
                _ if glyph.padding && glyph.pixel_width <= 0.0 => 0,
                _ if glyph.wide => 2,
                _ => 1,
            };
            let width_px = if glyph.pixel_width > 0.0 {
                glyph.pixel_width
            } else {
                width_cols as f32 * char_width_px.max(1.0)
            };
            metrics.width_cols += width_cols;
            metrics.width_px += width_px;
            metrics
        })
    }

    fn add(&mut self, other: Self) {
        self.width_px += other.width_px;
        self.width_cols += other.width_cols;
    }

    /// Growth of one materialized glyph area since `before`.
    ///
    /// Most characters append a new tail glyph, but a contextual-script run
    /// grows the earlier Composite base and appends a zero-accounting padding
    /// cell.  Taking the captured existing-glyph delta makes both
    /// representations report the same incremental progress to the
    /// source-position authority without rescanning the whole row.
    fn growth_since(self, before: Self) -> Self {
        Self::new(
            (self.width_px - before.width_px).max(0.0),
            self.width_cols.saturating_sub(before.width_cols),
        )
    }
}

/// Metric state captured immediately before one text character is emitted.
///
/// A text write can append tail glyphs and, for contextual shaping or a
/// grapheme continuation, grow exactly one existing non-padding tail glyph.
/// Encoding those two mutation regions here avoids taking whole-row metric
/// snapshots before and after every character.  The `must_use` contract makes
/// the caller finish metric accounting after layout/clipping mutations.
#[must_use = "finish the pending row write after all glyph mutations"]
#[derive(Clone, Copy, Debug)]
struct PendingDisplayRowWriteMetrics {
    appended_from: usize,
    existing_tail: Option<ExistingGlyphMetricCheckpoint>,
}

#[derive(Clone, Copy, Debug)]
struct ExistingGlyphMetricCheckpoint {
    index: usize,
    metrics: DisplayRowWriteMetrics,
}

impl PendingDisplayRowWriteMetrics {
    fn capture(
        glyphs: &[Glyph],
        char_width_px: f32,
        kind: DisplayRowTextNaturalAdvanceKind,
    ) -> Self {
        let existing_tail = matches!(
            kind,
            DisplayRowTextNaturalAdvanceKind::ClusterContinuation
                | DisplayRowTextNaturalAdvanceKind::ComplexRunMember
        )
        .then(|| {
            glyphs
                .iter()
                .rposition(|glyph| !glyph.padding)
                .map(|index| ExistingGlyphMetricCheckpoint {
                    index,
                    metrics: DisplayRowWriteMetrics::from_glyphs(
                        std::slice::from_ref(&glyphs[index]),
                        char_width_px,
                    ),
                })
        })
        .flatten();

        Self {
            appended_from: glyphs.len(),
            existing_tail,
        }
    }

    fn finish(self, glyphs: &[Glyph], char_width_px: f32) -> DisplayRowWriteMetrics {
        let mut written =
            DisplayRowWriteMetrics::from_glyphs(&glyphs[self.appended_from..], char_width_px);
        if let Some(checkpoint) = self.existing_tail
            && let Some(glyph) = glyphs.get(checkpoint.index)
        {
            let current =
                DisplayRowWriteMetrics::from_glyphs(std::slice::from_ref(glyph), char_width_px);
            written.add(current.growth_since(checkpoint.metrics));
        }
        written
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DisplayRowGlyphCheckpoint {
    area_lengths: [usize; 3],
    displays_text: bool,
    pointer_appearances_len: usize,
    image_margins_len: usize,
    string_sources_len: usize,
}

impl DisplayRowGlyphCheckpoint {
    pub(crate) fn capture(row: &GlyphRow) -> Self {
        Self {
            area_lengths: [
                row.glyphs[GlyphArea::LeftMargin.index()].len(),
                row.glyphs[GlyphArea::Text.index()].len(),
                row.glyphs[GlyphArea::RightMargin.index()].len(),
            ],
            displays_text: row.displays_text,
            pointer_appearances_len: row.pointer_appearances().len(),
            image_margins_len: row.image_margins_table().len(),
            string_sources_len: row.string_sources().len(),
        }
    }

    pub(crate) fn restore(self, row: &mut GlyphRow) {
        row.glyphs[GlyphArea::LeftMargin.index()].truncate(self.area_lengths[0]);
        row.glyphs[GlyphArea::Text.index()].truncate(self.area_lengths[1]);
        row.glyphs[GlyphArea::RightMargin.index()].truncate(self.area_lengths[2]);
        row.displays_text = self.displays_text;
        row.truncate_pointer_appearances(self.pointer_appearances_len);
        row.truncate_image_margins(self.image_margins_len);
        row.truncate_string_sources(self.string_sources_len);
    }

    /// Keep only glyphs appended after this checkpoint while retaining the
    /// row-local dependency tables that those glyphs reference.
    ///
    /// A rendered structural fragment can contain string-source, pointer, or
    /// image tokens whose indices are meaningful only with the source row's
    /// side tables.  Draining the earlier glyphs instead of rebuilding the
    /// fragment preserves those typed references without exposing raw table
    /// indices to callers.  Unreferenced earlier entries are harmless and are
    /// preferable to an incomplete fragment.
    pub(crate) fn retain_added_glyphs(self, row: &mut GlyphRow) {
        for area in GlyphArea::ALL {
            let area_index = area.index();
            let added_from = self.area_lengths[area_index].min(row.glyphs[area_index].len());
            row.glyphs[area_index].drain(..added_from);
        }
        row.displays_text = false;
    }

    pub(crate) fn first_new_text_glyph<'a>(
        self,
        row: &'a GlyphRow,
    ) -> Option<&'a neomacs_display_protocol::glyph_matrix::Glyph> {
        row.glyphs[GlyphArea::Text.index()].get(self.area_lengths[GlyphArea::Text.index()])
    }

    /// Derive a checkpoint `added` text glyphs further along than `self`. Used by
    /// the whole-text-run word-wrap path, which records candidates *after* the
    /// run is appended: the base checkpoint snapshots the row before the run, and
    /// each candidate's boundary is `base + char_offset` text glyphs (natural
    /// text runs map one source char to one text glyph). Any added text glyph
    /// means the row now displays text.
    pub(crate) fn with_added_text_glyphs(
        self,
        added: usize,
        after_append: DisplayRowGlyphCheckpoint,
    ) -> Self {
        let mut area_lengths = self.area_lengths;
        area_lengths[GlyphArea::Text.index()] += added;
        Self {
            area_lengths,
            displays_text: self.displays_text || added > 0,
            pointer_appearances_len: if added > 0 {
                after_append.pointer_appearances_len
            } else {
                self.pointer_appearances_len
            },
            image_margins_len: if added > 0 {
                after_append.image_margins_len
            } else {
                self.image_margins_len
            },
            string_sources_len: if added > 0 {
                after_append.string_sources_len
            } else {
                self.string_sources_len
            },
        }
    }
}

pub(crate) fn new_display_row(layout: &DisplayRowLayout) -> GlyphRow {
    let mut row = new_display_row_for_role(layout.role);
    layout.apply_to_row(&mut row);
    row
}

pub(crate) fn new_display_row_for_role(role: GlyphRowRole) -> GlyphRow {
    let mut row = GlyphRow::new(role);
    row.enabled = true;
    row
}

pub(crate) fn display_row_text_glyph_count(row: &GlyphRow) -> usize {
    row.glyphs[GlyphArea::Text.index()].len()
}

pub(crate) fn display_row_text_is_empty(row: &GlyphRow) -> bool {
    display_row_text_glyph_count(row) == 0
}

/// A row extent measured in terminal/display columns, not backing-vector
/// glyphs.  A single stretch glyph can occupy many columns, so keeping this as
/// a distinct type prevents structural decorators from accidentally treating
/// `Vec<Glyph>::len()` as a screen coordinate.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct DisplayRowColumnCount(usize);

impl DisplayRowColumnCount {
    pub(crate) fn from_row(row: &GlyphRow, char_width_px: f32) -> Self {
        Self(
            row.glyphs
                .iter()
                .map(|area| DisplayRowWriteMetrics::from_glyphs(area, char_width_px).width_cols())
                .sum(),
        )
    }

    pub(crate) const fn get(self) -> usize {
        self.0
    }
}

pub(crate) fn trim_display_row_text_to_total_columns(
    row: &mut GlyphRow,
    target: usize,
    char_width_px: f32,
) {
    while DisplayRowColumnCount::from_row(row, char_width_px).get() > target {
        let text_area = &mut row.glyphs[GlyphArea::Text.index()];
        if text_area.is_empty() {
            break;
        }
        text_area.pop();
    }
}

pub(crate) fn pop_display_row_trailing_text_char(row: &mut GlyphRow, ch: char) -> Option<Glyph> {
    if row.glyphs[GlyphArea::Text.index()].last().is_some_and(
        |glyph| matches!(glyph.glyph_type, GlyphType::Char { ch: glyph_ch } if glyph_ch == ch),
    ) {
        row.glyphs[GlyphArea::Text.index()].pop()
    } else {
        None
    }
}

pub(crate) fn apply_display_row_source_slot_bounds(
    row: &mut GlyphRow,
    slots: &[DisplayRowGlyphSlot],
) {
    let Some((start, end)) = display_row_buffer_source_slot_bounds(slots) else {
        return;
    };
    set_display_row_buffer_source_bounds(row, start, end);
}

pub(crate) fn merge_display_row_source_slot_bounds(
    row: &mut GlyphRow,
    slots: &[DisplayRowGlyphSlot],
) {
    let Some((start, end)) = display_row_buffer_source_slot_bounds(slots) else {
        return;
    };
    merge_display_row_buffer_source_bounds(row, start, end);
}

fn display_row_buffer_source_slot_bounds(slots: &[DisplayRowGlyphSlot]) -> Option<(usize, usize)> {
    slots.iter().fold(None::<(usize, usize)>, |bounds, slot| {
        let DisplaySourcePosition::Buffer { char_pos, .. } = slot.source else {
            return bounds;
        };
        let start = char_pos.get();
        let end = start.saturating_add(1);
        Some(match bounds {
            Some((old_start, old_end)) => (old_start.min(start), old_end.max(end)),
            None => (start, end),
        })
    })
}

fn merge_display_row_buffer_source_bounds(row: &mut GlyphRow, start: usize, end: usize) {
    // Row bounds are real from the row's BEGIN (stamped with the walk-start
    // position), so glyph spans always MERGE — never replace. No numeric
    // "unset" state exists to test for.
    set_display_row_buffer_source_bounds(
        row,
        row.start_charpos.min(start),
        row.end_charpos.max(end),
    );
}

fn set_display_row_buffer_source_bounds(row: &mut GlyphRow, start: usize, end: usize) {
    row.start_charpos = start;
    row.end_charpos = end;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DisplayRowPosition {
    x_px: f32,
    col: usize,
    tab_coordinates: DisplayTabCoordinates,
}

impl DisplayRowPosition {
    pub(crate) const fn new(x_px: f32, col: usize) -> Self {
        Self {
            x_px,
            col,
            tab_coordinates: DisplayTabCoordinates::ScreenLine,
        }
    }

    pub(crate) fn x_px(self) -> f32 {
        self.x_px
    }

    pub(crate) fn col(self) -> usize {
        self.col
    }

    pub(crate) const fn with_tab_coordinates(
        mut self,
        tab_coordinates: DisplayTabCoordinates,
    ) -> Self {
        self.tab_coordinates = tab_coordinates;
        self
    }

    pub(crate) const fn on_screen_line_tab_grid(mut self) -> Self {
        self.tab_coordinates = DisplayTabCoordinates::ScreenLine;
        self
    }

    /// Move the renderer pen while retaining the TAB coordinate space.
    ///
    /// A continued physical line's TAB origin is walk state, not a property
    /// of its current screen-row x/column.  Fit probes and alternate row
    /// routes use this instead of reconstructing a screen-local position.
    pub(crate) const fn at_screen_position(mut self, x_px: f32, col: usize) -> Self {
        self.x_px = x_px;
        self.col = col;
        self
    }

    pub(crate) fn advance_by(self, metrics: DisplayRowWriteMetrics) -> Self {
        Self {
            x_px: self.x_px + metrics.width_px,
            col: self.col + metrics.width_cols,
            tab_coordinates: self.tab_coordinates,
        }
    }

    pub(crate) fn saturating_width_to(self, end: Self) -> DisplayRowWriteMetrics {
        DisplayRowWriteMetrics::new(
            (end.x_px - self.x_px).max(0.0),
            end.col.saturating_sub(self.col),
        )
    }
}

fn append_start_position(
    requested: DisplayRowPosition,
    current_tail: DisplayRowPosition,
) -> DisplayRowPosition {
    if current_tail.col > requested.col || current_tail.x_px > requested.x_px {
        // Row-local structural glyphs may advance the screen pen, but the
        // source walk remains the authority for GNU's physical-line TAB
        // coordinate. Reconcile geometry without replacing that typed state.
        requested.at_screen_position(current_tail.x_px, current_tail.col)
    } else {
        requested
    }
}

/// Chooses which coordinate authority starts an append into an existing row.
///
/// Structural glyphs can share `TEXT_AREA` with source glyphs without belonging
/// to the source walk.  GNU line numbers are the canonical example: they advance
/// the glyph-row tail, while the buffer iterator has already moved its requested
/// pixel origin past the reserved gutter.  Reconciling those two positions
/// would count the gutter twice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayRowAppendStartPolicy {
    /// Continue after every glyph already materialized in the target area.
    ReconcileWithRowTail,
    /// Trust the source walk's position; structural glyphs are already reflected
    /// in that position's coordinate origin.
    SourcePosition,
}

impl DisplayRowAppendStartPolicy {
    fn resolve(
        self,
        requested: DisplayRowPosition,
        current_tail: DisplayRowPosition,
    ) -> DisplayRowPosition {
        match self {
            Self::ReconcileWithRowTail => append_start_position(requested, current_tail),
            Self::SourcePosition => requested,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DisplayRowAppendStatus {
    Complete,
    Clipped,
    RowBreak,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayRowGlyphSlot {
    source: DisplaySourcePosition,
    x_px: f32,
    col: usize,
    width_px: f32,
    width_cols: usize,
}

impl DisplayRowGlyphSlot {
    #[cfg(test)]
    pub(crate) fn new(
        source: DisplaySourcePosition,
        x_px: f32,
        col: usize,
        width_px: f32,
        width_cols: usize,
    ) -> Self {
        Self::with_pointer_appearance(source, x_px, col, width_px, width_cols, None)
    }

    pub(crate) fn with_pointer_appearance(
        source: DisplaySourcePosition,
        x_px: f32,
        col: usize,
        width_px: f32,
        width_cols: usize,
        _pointer_appearance: Option<crate::display_item::DisplayPointerAppearance>,
    ) -> Self {
        Self {
            source,
            x_px,
            col,
            width_px,
            width_cols,
        }
    }

    pub(crate) fn source(&self) -> DisplaySourcePosition {
        self.source.clone()
    }

    pub(crate) fn x_px(&self) -> f32 {
        self.x_px
    }

    pub(crate) fn col(&self) -> usize {
        self.col
    }

    pub(crate) fn width_px(&self) -> f32 {
        self.width_px
    }

    pub(crate) fn width_cols(&self) -> usize {
        self.width_cols
    }

    pub(crate) fn start_position(&self) -> DisplayRowPosition {
        DisplayRowPosition::new(self.x_px(), self.col())
    }

    pub(crate) fn end_position(&self) -> DisplayRowPosition {
        DisplayRowPosition::new(
            self.x_px() + self.width_px(),
            self.col() + self.width_cols(),
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DisplayRowAppendProgress {
    start: DisplayRowPosition,
    end: DisplayRowPosition,
    metrics: DisplayRowWriteMetrics,
    status: DisplayRowAppendStatus,
    slots: Vec<DisplayRowGlyphSlot>,
}

impl DisplayRowAppendProgress {
    pub(crate) fn new(
        start: DisplayRowPosition,
        end: DisplayRowPosition,
        metrics: DisplayRowWriteMetrics,
        status: DisplayRowAppendStatus,
        slots: Vec<DisplayRowGlyphSlot>,
    ) -> Self {
        Self {
            start,
            end,
            metrics,
            status,
            slots,
        }
    }

    pub(crate) fn from_positions(
        start: DisplayRowPosition,
        end: DisplayRowPosition,
        status: DisplayRowAppendStatus,
        slots: Vec<DisplayRowGlyphSlot>,
    ) -> Self {
        Self::new(start, end, start.saturating_width_to(end), status, slots)
    }

    pub(crate) fn start(&self) -> DisplayRowPosition {
        self.start
    }

    pub(crate) fn end(&self) -> DisplayRowPosition {
        self.end
    }

    pub(crate) fn metrics(&self) -> DisplayRowWriteMetrics {
        self.metrics
    }

    pub(crate) fn status(&self) -> DisplayRowAppendStatus {
        self.status
    }

    pub(crate) fn slots(&self) -> &[DisplayRowGlyphSlot] {
        &self.slots
    }

    pub(crate) fn is_complete_with_positive_width(&self) -> bool {
        self.status == DisplayRowAppendStatus::Complete && self.metrics.has_positive_width()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DisplayTextClustering {
    UnicodeFallback,
    Independent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DisplayTextSourceMapping<'a> {
    NaturalText,
    SourceMapped {
        glyph_string_start: Option<&'a DisplaySourcePosition>,
    },
}

impl DisplayTextSourceMapping<'_> {
    fn resolve(self, span: &SourceSpan, row: &mut GlyphRow) -> ResolvedDisplayTextSourceMapping {
        let provenance = match self {
            Self::NaturalText => natural_text_glyph(&span.start, 0),
            Self::SourceMapped { glyph_string_start } => {
                source_mapped_text_glyph(span, glyph_string_start, 0)
            }
        };
        let advances = match self {
            Self::NaturalText => true,
            Self::SourceMapped { glyph_string_start } => glyph_string_start.is_some(),
        };
        let start = match provenance {
            ProducedGlyphProvenance::Buffer { charpos } => GlyphProvenance::buffer(charpos),
            ProducedGlyphProvenance::Str {
                string,
                index,
                covered_buffer,
            } => {
                let source = match covered_buffer {
                    Some(range) => GlyphStringSource::replacement(string, range),
                    None => GlyphStringSource::new(string),
                };
                let source = row
                    .push_string_source(source)
                    .expect("display row string-source table exhausted");
                GlyphProvenance::string(source, index)
            }
            ProducedGlyphProvenance::Redisplay(provenance) => {
                GlyphProvenance::Redisplay(provenance)
            }
        };
        ResolvedDisplayTextSourceMapping { start, advances }
    }

    fn slot_source(
        self,
        span_start: &DisplaySourcePosition,
        char_offset: usize,
        byte_offset: usize,
    ) -> DisplaySourcePosition {
        match self {
            Self::NaturalText => span_start.advanced_by(char_offset, byte_offset),
            Self::SourceMapped { .. } => span_start.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedDisplayTextSourceMapping {
    start: GlyphProvenance,
    advances: bool,
}

impl ResolvedDisplayTextSourceMapping {
    const fn provenance(self, char_offset: usize) -> GlyphProvenance {
        if self.advances {
            self.start.advanced_by(char_offset)
        } else {
            self.start
        }
    }
}

#[cfg(test)]
struct DisplayRowBuilder<'a> {
    layout: DisplayRowLayout,
    row: GlyphRow,
    glyph_measurer: Option<&'a mut dyn DisplayGlyphMeasurer>,
}

struct DisplayRowWriter<'layout, 'row, 'measurer> {
    layout: &'layout DisplayRowLayout,
    row: &'row mut GlyphRow,
    glyph_measurer: Option<&'measurer mut dyn DisplayGlyphMeasurer>,
    // Keep the semantic target instead of erasing it to a usize.  Structural
    // margin lanes have different boundary behavior from flowing text, and an
    // exhaustive enum match makes that distinction compile-time checked.
    area: GlyphArea,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DisplayRowOverflowPolicy {
    RejectOverflowingGlyph,
    ClipToStructuralLane,
}

pub(crate) struct DisplayRowProgressWriter<'layout, 'row, 'measurer> {
    writer: DisplayRowWriter<'layout, 'row, 'measurer>,
    position: DisplayRowPosition,
    max_x_px: f32,
    /// Where `position` and `max_x_px` are measured from in GNU's
    /// window-local terms; see `DisplayRowTextAreaOrigin`.
    text_area_origin: DisplayRowTextAreaOrigin,
    text_run_measurement: Option<DisplayTextRunMeasurement>,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq)]
struct DisplayRowAppendCursor {
    position: DisplayRowPosition,
    max_x_px: f32,
}

#[cfg(test)]
impl DisplayRowAppendCursor {
    fn new(position: DisplayRowPosition, max_x_px: f32) -> Self {
        Self { position, max_x_px }
    }

    fn position(&self) -> DisplayRowPosition {
        self.position
    }

    #[cfg(test)]
    fn append_item_to_current_text_row(
        &mut self,
        builder: &mut DisplayOutputBuilder,
        layout: &DisplayRowLayout,
        item: DisplayItem,
    ) -> Option<DisplayRowAppendProgress> {
        let progress = append_display_item_to_current_text_row(
            builder,
            layout,
            item,
            self.position,
            self.max_x_px,
        )?;
        self.position = progress.end();
        Some(progress)
    }

    fn append_measured_item_to_current_text_row(
        &mut self,
        builder: &mut DisplayOutputBuilder,
        layout: &DisplayRowLayout,
        item: DisplayItem,
        glyph_measurer: &mut dyn DisplayGlyphMeasurer,
    ) -> Option<DisplayRowAppendProgress> {
        let progress = append_measured_display_item_to_current_text_row(
            builder,
            layout,
            item,
            glyph_measurer,
            self.position,
            self.max_x_px,
        )?;
        self.position = progress.end();
        Some(progress)
    }
}

#[cfg(test)]
fn append_display_item_to_current_text_row(
    builder: &mut DisplayOutputBuilder,
    layout: &DisplayRowLayout,
    item: DisplayItem,
    position: DisplayRowPosition,
    max_x_px: f32,
) -> Option<DisplayRowAppendProgress> {
    builder.edit_current_row_for_test(|row| {
        let mut writer = DisplayRowProgressWriter::new(layout, row, position, max_x_px);
        writer.push_item(item)
    })
}

#[cfg(test)]
fn append_measured_display_item_to_current_text_row(
    builder: &mut DisplayOutputBuilder,
    layout: &DisplayRowLayout,
    item: DisplayItem,
    glyph_measurer: &mut dyn DisplayGlyphMeasurer,
    position: DisplayRowPosition,
    max_x_px: f32,
) -> Option<DisplayRowAppendProgress> {
    builder.edit_current_row_for_test(|row| {
        let mut writer = DisplayRowProgressWriter::with_glyph_measurer(
            layout,
            row,
            glyph_measurer,
            position,
            max_x_px,
        );
        writer.push_item(item)
    })
}

#[cfg(test)]
impl DisplayRowBuilder<'_> {
    fn new(layout: DisplayRowLayout) -> Self {
        let row = new_display_row(&layout);
        Self {
            layout,
            row,
            glyph_measurer: None,
        }
    }
}

#[cfg(test)]
impl<'a> DisplayRowBuilder<'a> {
    #[cfg(test)]
    fn with_glyph_measurer(
        layout: DisplayRowLayout,
        glyph_measurer: &'a mut dyn DisplayGlyphMeasurer,
    ) -> Self {
        let mut builder = Self::new(layout);
        builder.glyph_measurer = Some(glyph_measurer);
        builder
    }

    fn push_item(&mut self, item: DisplayItem) -> DisplayRowWriteMetrics {
        if let Some(glyph_measurer) = self.glyph_measurer.as_deref_mut() {
            let mut writer =
                DisplayRowWriter::with_glyph_measurer(&self.layout, &mut self.row, glyph_measurer);
            writer.push_item(item)
        } else {
            let mut writer = DisplayRowWriter::new(&self.layout, &mut self.row);
            writer.push_item(item)
        }
    }

    fn push_measured_item(
        &mut self,
        item: DisplayItem,
        glyph_measurer: &mut dyn DisplayGlyphMeasurer,
    ) -> DisplayRowWriteMetrics {
        let mut writer =
            DisplayRowWriter::with_glyph_measurer(&self.layout, &mut self.row, glyph_measurer);
        writer.push_item(item)
    }

    fn push_source(
        &mut self,
        source: &mut impl DisplayItemSource,
        context: &mut DisplaySourceContext<'_>,
    ) -> DisplayRowWriteMetrics {
        if let Some(glyph_measurer) = self.glyph_measurer.as_deref_mut() {
            let mut writer =
                DisplayRowWriter::with_glyph_measurer(&self.layout, &mut self.row, glyph_measurer);
            writer.push_source(source, context)
        } else {
            let mut writer = DisplayRowWriter::new(&self.layout, &mut self.row);
            writer.push_source(source, context)
        }
    }

    fn finish(mut self) -> GlyphRow {
        glyph_row_writer::normalize_external_row(&mut self.row);
        self.row
    }
}

impl<'layout, 'row> DisplayRowProgressWriter<'layout, 'row, '_> {
    #[cfg(test)]
    pub(crate) fn new(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        position: DisplayRowPosition,
        max_x_px: f32,
    ) -> Self {
        let mut writer = DisplayRowWriter::new(layout, row);
        writer.set_empty_row_start(position);
        let position = append_start_position(position, writer.current_text_position());
        Self {
            writer,
            position,
            max_x_px,
            text_area_origin: DisplayRowTextAreaOrigin::row_local(),
            text_run_measurement: None,
        }
    }
}

impl<'layout, 'row, 'measurer> DisplayRowProgressWriter<'layout, 'row, 'measurer> {
    #[cfg(test)]
    pub(crate) fn with_glyph_measurer(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        glyph_measurer: &'measurer mut dyn DisplayGlyphMeasurer,
        position: DisplayRowPosition,
        max_x_px: f32,
    ) -> Self {
        Self::with_glyph_measurer_for_area(
            layout,
            row,
            glyph_measurer,
            position,
            max_x_px,
            GlyphArea::Text,
        )
    }

    #[cfg(test)]
    pub(crate) fn with_glyph_measurer_for_area(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        glyph_measurer: &'measurer mut dyn DisplayGlyphMeasurer,
        position: DisplayRowPosition,
        max_x_px: f32,
        area: GlyphArea,
    ) -> Self {
        Self::with_glyph_measurer_for_area_and_start_policy(
            layout,
            row,
            glyph_measurer,
            position,
            max_x_px,
            DisplayRowTextAreaOrigin::row_local(),
            area,
            DisplayRowAppendStartPolicy::ReconcileWithRowTail,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_glyph_measurer_for_area_and_start_policy(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        glyph_measurer: &'measurer mut dyn DisplayGlyphMeasurer,
        position: DisplayRowPosition,
        max_x_px: f32,
        text_area_origin: DisplayRowTextAreaOrigin,
        area: GlyphArea,
        start_policy: DisplayRowAppendStartPolicy,
    ) -> Self {
        let mut writer =
            DisplayRowWriter::with_glyph_measurer_for_area(layout, row, glyph_measurer, area);
        writer.set_empty_row_start(position);
        let position = start_policy.resolve(position, writer.current_text_position());
        Self {
            writer,
            position,
            max_x_px,
            text_area_origin,
            text_run_measurement: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_text_run_measurement(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        text_run_measurement: DisplayTextRunMeasurement,
        position: DisplayRowPosition,
        max_x_px: f32,
    ) -> Self {
        Self::with_text_run_measurement_for_area(
            layout,
            row,
            text_run_measurement,
            position,
            max_x_px,
            GlyphArea::Text,
        )
    }

    #[cfg(test)]
    pub(crate) fn with_text_run_measurement_for_area(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        text_run_measurement: DisplayTextRunMeasurement,
        position: DisplayRowPosition,
        max_x_px: f32,
        area: GlyphArea,
    ) -> Self {
        let mut writer = DisplayRowWriter::for_area(layout, row, area);
        writer.set_empty_row_start(position);
        let position = append_start_position(position, writer.current_text_position());
        Self {
            writer,
            position,
            max_x_px,
            text_area_origin: DisplayRowTextAreaOrigin::row_local(),
            text_run_measurement: Some(text_run_measurement),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_text_run_measurement_and_glyph_measurer_for_area_and_start_policy(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        text_run_measurement: DisplayTextRunMeasurement,
        glyph_measurer: &'measurer mut dyn DisplayGlyphMeasurer,
        position: DisplayRowPosition,
        max_x_px: f32,
        text_area_origin: DisplayRowTextAreaOrigin,
        area: GlyphArea,
        start_policy: DisplayRowAppendStartPolicy,
    ) -> Self {
        let mut writer =
            DisplayRowWriter::with_glyph_measurer_for_area(layout, row, glyph_measurer, area);
        writer.set_empty_row_start(position);
        let position = start_policy.resolve(position, writer.current_text_position());
        Self {
            writer,
            position,
            max_x_px,
            text_area_origin,
            text_run_measurement: Some(text_run_measurement),
        }
    }

    #[cfg(test)]
    pub(crate) fn position(&self) -> DisplayRowPosition {
        self.position
    }

    pub(crate) fn push_item(&mut self, item: DisplayItem) -> DisplayRowAppendProgress {
        let start = self.position;
        let mut metrics = DisplayRowWriteMetrics::default();
        let mut slots = Vec::new();
        let DisplayItem {
            span,
            face,
            kind,
            layout: item_layout,
            pointer_appearance,
            box_vertical_edges,
            box_run_membership,
        } = item;
        let item_glyph_start = self.area_len();
        let status = match kind {
            DisplayItemKind::RowBreak(_) => DisplayRowAppendStatus::RowBreak,
            DisplayItemKind::TextRun(run) => match run.composition {
                DisplayTextComposition::UnicodeFallback => self.push_text_item(
                    &span,
                    face,
                    item_layout,
                    run.text.as_ref(),
                    DisplayTextClustering::UnicodeFallback,
                    DisplayTextSourceMapping::NaturalText,
                    pointer_appearance.as_ref(),
                    &mut metrics,
                    &mut slots,
                ),
                DisplayTextComposition::Independent => self.push_text_item(
                    &span,
                    face,
                    item_layout,
                    run.text.as_ref(),
                    DisplayTextClustering::Independent,
                    DisplayTextSourceMapping::NaturalText,
                    pointer_appearance.as_ref(),
                    &mut metrics,
                    &mut slots,
                ),
                DisplayTextComposition::Automatic(terminal) => self.push_automatic_text_item(
                    &span,
                    face,
                    item_layout,
                    run.text.as_ref(),
                    terminal,
                    pointer_appearance.as_ref(),
                    &mut metrics,
                    &mut slots,
                ),
            },
            DisplayItemKind::SourceMappedText(text) => self.push_text_item(
                &span,
                face,
                item_layout,
                text.text.as_ref(),
                DisplayTextClustering::UnicodeFallback,
                DisplayTextSourceMapping::SourceMapped {
                    glyph_string_start: text.glyph_string_start.as_ref(),
                },
                pointer_appearance.as_ref(),
                &mut metrics,
                &mut slots,
            ),
            kind => {
                let slot_start = self.position;
                let slot_source = span.start.clone();
                let before_len = self.area_len();
                // GNU crops a wide xwidget's advance when it produces the
                // glyph, before `display_line` measures the row; see
                // `DisplayXwidgetOverflowAction`.  Xwidgets in body text only:
                // images have their own GNU rule (not ported), and margin
                // lanes keep their own structural clip below.
                let kind = match kind {
                    DisplayItemKind::MediaReplacement(media)
                        if matches!(media.kind, DisplayMediaReplacementKind::Xwidget { .. })
                            && self.writer.overflow_policy()
                                == DisplayRowOverflowPolicy::RejectOverflowingGlyph =>
                    {
                        let extent = WindowLocalRowExtent::from_frame_coordinates(
                            self.text_area_origin,
                            self.position.x_px(),
                            self.max_x_px,
                        );
                        let action = DisplayXwidgetOverflowAction::for_xwidget(
                            media.width,
                            extent,
                            before_len == 0,
                        );
                        DisplayItemKind::MediaReplacement(match action {
                            DisplayXwidgetOverflowAction::CropAdvanceToVisibleWidth {
                                visible_width_px,
                            } => media.xwidget_advance_cropped_to(visible_width_px),
                            DisplayXwidgetOverflowAction::Fits
                            | DisplayXwidgetOverflowAction::LeaveWhole => media,
                        })
                    }
                    kind => kind,
                };
                let checkpoint = DisplayRowGlyphCheckpoint::capture(self.writer.row);
                let mut written = self.writer.push_item(
                    DisplayItem::new(span, face, kind)
                        .with_layout(item_layout)
                        .with_box_vertical_edges(box_vertical_edges)
                        .with_pointer_appearance(pointer_appearance.clone()),
                );
                let mut status = DisplayRowAppendStatus::Complete;
                if written.has_positive_width()
                    && self.position.x_px() + written.width_px() > self.max_x_px
                {
                    let available_px = (self.max_x_px - self.position.x_px()).max(0.0);
                    match self.writer.overflow_policy() {
                        DisplayRowOverflowPolicy::RejectOverflowingGlyph
                        | DisplayRowOverflowPolicy::ClipToStructuralLane
                            if available_px <= 0.0 =>
                        {
                            checkpoint.restore(self.writer.row);
                            return DisplayRowAppendProgress::new(
                                start,
                                self.position,
                                metrics,
                                DisplayRowAppendStatus::Clipped,
                                slots,
                            );
                        }
                        DisplayRowOverflowPolicy::RejectOverflowingGlyph => {
                            checkpoint.restore(self.writer.row);
                            return DisplayRowAppendProgress::new(
                                start,
                                self.position,
                                metrics,
                                DisplayRowAppendStatus::Clipped,
                                slots,
                            );
                        }
                        DisplayRowOverflowPolicy::ClipToStructuralLane => {
                            self.clip_new_glyphs_to_available_width(before_len, available_px);
                            written = self.metrics_since(before_len);
                            status = DisplayRowAppendStatus::Clipped;
                        }
                    }
                }
                if !written.is_empty() {
                    slots.push(DisplayRowGlyphSlot::with_pointer_appearance(
                        slot_source,
                        slot_start.x_px(),
                        slot_start.col(),
                        written.width_px(),
                        written.width_cols(),
                        pointer_appearance.clone(),
                    ));
                }
                self.advance(written);
                metrics.add(written);
                status
            }
        };

        self.apply_item_box_run_topology(
            item_glyph_start,
            box_run_membership,
            box_vertical_edges,
            status == DisplayRowAppendStatus::Complete,
        );

        DisplayRowAppendProgress::new(start, self.position, metrics, status, slots)
    }

    fn apply_item_box_run_topology(
        &mut self,
        before_len: usize,
        membership: neomacs_display_protocol::face::BoxRunMembership,
        source_edges: neomacs_display_protocol::face::BoxVerticalEdges,
        source_item_complete: bool,
    ) {
        let area_index = self.writer.area_index();
        let glyphs = &mut self.writer.row.glyphs[area_index][before_len..];
        apply_box_run_topology_to_glyphs(glyphs, membership, source_edges, source_item_complete);
    }

    fn push_text_item(
        &mut self,
        span: &SourceSpan,
        face: RenderFaceRef,
        item_layout: DisplayItemLayout,
        text: &str,
        clustering: DisplayTextClustering,
        source_mapping: DisplayTextSourceMapping<'_>,
        pointer_appearance: Option<&crate::display_item::DisplayPointerAppearance>,
        metrics: &mut DisplayRowWriteMetrics,
        slots: &mut Vec<DisplayRowGlyphSlot>,
    ) -> DisplayRowAppendStatus {
        let face_id = self.writer.face_id(face);
        let measurement = self.text_run_measurement(text, face_id);
        let glyph_pointer_appearance =
            pointer_appearance.and_then(|appearance| appearance.glyph_metadata());
        let mut pointer_metadata = None;
        let mut resolved_source_mapping = None;
        let mut status = DisplayRowAppendStatus::Complete;
        let mut byte_offset = 0usize;
        for (char_offset, ch) in text.chars().enumerate() {
            let char_state = match clustering {
                DisplayTextClustering::UnicodeFallback => self.writer.text_char_state(ch),
                DisplayTextClustering::Independent => DisplayRowTextCharState::independent(ch),
            };
            let advance_request = DisplayRowTextCharAdvanceRequest::new(
                char_state,
                face_id,
                self.position,
                char_offset,
                byte_offset,
                &measurement,
            );
            let natural_advance = advance_request.resolve_advance_px_with_writer(&mut self.writer);
            let advance =
                self.writer
                    .item_horizontal_advance_px(ch, face_id, natural_advance, item_layout);
            let overflowing = advance > 0.0 && self.position.x_px + advance > self.max_x_px;
            if overflowing
                && (self.writer.overflow_policy()
                    == DisplayRowOverflowPolicy::RejectOverflowingGlyph
                    || self.position.x_px() >= self.max_x_px)
            {
                status = DisplayRowAppendStatus::Clipped;
                break;
            }

            let before_len = self.area_len();
            let pending_metrics = PendingDisplayRowWriteMetrics::capture(
                &self.writer.row.glyphs[self.writer.area_index()],
                self.writer.layout.char_width_px,
                advance_request.kind(),
            );
            let slot_start = self.position;
            if resolved_source_mapping.is_none() {
                resolved_source_mapping = Some(source_mapping.resolve(span, self.writer.row));
            }
            self.writer.push_text_char_state_with_advance_request(
                resolved_source_mapping
                    .expect("text source mapping resolved before glyph emission")
                    .provenance(char_offset),
                advance_request,
            );
            self.writer
                .apply_item_layout_since(before_len, face_id, item_layout);
            if overflowing {
                let available_px = (self.max_x_px - slot_start.x_px()).max(0.0);
                self.clip_new_glyphs_to_available_width(before_len, available_px);
                status = DisplayRowAppendStatus::Clipped;
            }
            if self.area_len() > before_len
                && pointer_metadata.is_none()
                && let Some(appearance) = glyph_pointer_appearance
            {
                pointer_metadata = self.writer.row.intern_pointer_appearance(appearance);
            }
            let area_index = self.writer.area_index();
            for glyph in &mut self.writer.row.glyphs[area_index][before_len..] {
                glyph.pointer_appearance = pointer_metadata;
            }
            let written = pending_metrics.finish(
                &self.writer.row.glyphs[self.writer.area_index()],
                self.writer.layout.char_width_px,
            );
            slots.push(DisplayRowGlyphSlot::with_pointer_appearance(
                source_mapping.slot_source(&span.start, char_offset, byte_offset),
                slot_start.x_px(),
                slot_start.col(),
                written.width_px(),
                written.width_cols(),
                pointer_appearance.cloned(),
            ));
            self.advance(written);
            metrics.add(written);
            byte_offset += ch.len_utf8();
            if overflowing {
                break;
            }
        }
        status
    }

    #[allow(clippy::too_many_arguments)]
    fn push_automatic_text_item(
        &mut self,
        span: &SourceSpan,
        face: RenderFaceRef,
        item_layout: DisplayItemLayout,
        text: &str,
        terminal: TerminalComposition,
        pointer_appearance: Option<&crate::display_item::DisplayPointerAppearance>,
        metrics: &mut DisplayRowWriteMetrics,
        slots: &mut Vec<DisplayRowGlyphSlot>,
    ) -> DisplayRowAppendStatus {
        let face_id = self.writer.face_id(face);
        let advance = self
            .writer
            .automatic_composition_advance_px(text, face_id, &terminal);
        if advance > 0.0 && self.position.x_px() + advance > self.max_x_px {
            return DisplayRowAppendStatus::Clipped;
        }

        let slot_start = self.position;
        let source_mapping = DisplayTextSourceMapping::NaturalText;
        let resolved = source_mapping.resolve(span, self.writer.row);
        let before_len = self.area_len();
        self.writer
            .push_automatic_composition(text, terminal, face_id, resolved, advance);
        self.writer
            .apply_item_layout_since(before_len, face_id, item_layout);

        let glyph_pointer_appearance =
            pointer_appearance.and_then(|appearance| appearance.glyph_metadata());
        let pointer_metadata = glyph_pointer_appearance
            .and_then(|appearance| self.writer.row.intern_pointer_appearance(appearance));
        let area_index = self.writer.area_index();
        for glyph in &mut self.writer.row.glyphs[area_index][before_len..] {
            glyph.pointer_appearance = pointer_metadata;
        }

        let written = DisplayRowWriteMetrics::from_glyphs(
            &self.writer.row.glyphs[area_index][before_len..],
            self.writer.layout.char_width_px,
        );
        let mut byte_offset = 0usize;
        for (char_offset, ch) in text.chars().enumerate() {
            slots.push(DisplayRowGlyphSlot::with_pointer_appearance(
                source_mapping.slot_source(&span.start, char_offset, byte_offset),
                slot_start.x_px(),
                slot_start.col(),
                if char_offset == 0 {
                    written.width_px()
                } else {
                    0.0
                },
                if char_offset == 0 {
                    written.width_cols()
                } else {
                    0
                },
                pointer_appearance.cloned(),
            ));
            byte_offset += ch.len_utf8();
        }
        self.advance(written);
        metrics.add(written);
        DisplayRowAppendStatus::Complete
    }

    /// Constrain newly appended structural glyphs to their lane's remaining
    /// pixel extent.  The final fitting glyph is retained with a clipped cell,
    /// while later glyphs are discarded.  This preserves both visibility and
    /// the authoritative start position of the following text lane.
    fn clip_new_glyphs_to_available_width(&mut self, before_len: usize, available_px: f32) {
        let area_index = self.writer.area_index();
        let char_width_px = self.writer.layout.char_width_px;
        let glyphs = &mut self.writer.row.glyphs[area_index];
        let mut remaining_px = available_px.max(0.0);
        let mut keep_len = before_len;

        for index in before_len..glyphs.len() {
            if remaining_px <= 0.0 {
                break;
            }
            let width = DisplayRowWriteMetrics::from_glyphs(
                std::slice::from_ref(&glyphs[index]),
                char_width_px,
            )
            .width_px();
            keep_len = index + 1;
            if width > remaining_px {
                glyphs[index].pixel_width = remaining_px;
                break;
            }
            remaining_px -= width;
        }
        glyphs.truncate(keep_len);
    }

    fn area_len(&self) -> usize {
        self.writer.row.glyphs[self.writer.area_index()].len()
    }

    fn metrics_since(&self, before_len: usize) -> DisplayRowWriteMetrics {
        DisplayRowWriteMetrics::from_glyphs(
            &self.writer.row.glyphs[self.writer.area_index()][before_len..],
            self.writer.layout.char_width_px,
        )
    }

    fn advance(&mut self, metrics: DisplayRowWriteMetrics) {
        self.position = self.position.advance_by(metrics);
    }

    fn text_run_measurement(&mut self, text: &str, face_id: FaceId) -> DisplayTextRunMeasurement {
        self.text_run_measurement
            .clone()
            .unwrap_or_else(|| self.writer.text_run_measurement(text, face_id))
    }
}

impl<'layout, 'row, 'measurer> DisplayRowWriter<'layout, 'row, 'measurer> {
    fn area_index(&self) -> usize {
        self.area.index()
    }

    fn overflow_policy(&self) -> DisplayRowOverflowPolicy {
        match self.area {
            GlyphArea::Text => DisplayRowOverflowPolicy::RejectOverflowingGlyph,
            GlyphArea::LeftMargin | GlyphArea::RightMargin => {
                DisplayRowOverflowPolicy::ClipToStructuralLane
            }
        }
    }

    #[cfg(test)]
    fn new(layout: &'layout DisplayRowLayout, row: &'row mut GlyphRow) -> Self {
        Self::for_area(layout, row, GlyphArea::Text)
    }

    #[cfg(test)]
    fn for_area(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        area: GlyphArea,
    ) -> Self {
        layout.apply_to_row(row);
        Self {
            layout,
            row,
            glyph_measurer: None,
            area,
        }
    }

    #[cfg(test)]
    fn with_glyph_measurer(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        glyph_measurer: &'measurer mut dyn DisplayGlyphMeasurer,
    ) -> Self {
        Self::with_glyph_measurer_for_area(layout, row, glyph_measurer, GlyphArea::Text)
    }

    fn with_glyph_measurer_for_area(
        layout: &'layout DisplayRowLayout,
        row: &'row mut GlyphRow,
        glyph_measurer: &'measurer mut dyn DisplayGlyphMeasurer,
        area: GlyphArea,
    ) -> Self {
        layout.apply_to_row(row);
        Self {
            layout,
            row,
            glyph_measurer: Some(glyph_measurer),
            area,
        }
    }

    fn push_item(&mut self, item: DisplayItem) -> DisplayRowWriteMetrics {
        let item_layout = item.layout;
        let pointer_appearance = item
            .pointer_appearance
            .as_ref()
            .and_then(|appearance| appearance.glyph_metadata());
        let face_id = self.face_id(item.face);
        let area_index = self.area_index();
        let before_len = self.row.glyphs[area_index].len();
        let box_vertical_edges = item.box_vertical_edges;
        let box_run_membership = item.box_run_membership;
        match item.kind {
            DisplayItemKind::TextRun(run) => match run.composition {
                DisplayTextComposition::UnicodeFallback => self.push_text_item(
                    run.text.as_ref(),
                    face_id,
                    &item.span,
                    DisplayTextClustering::UnicodeFallback,
                    DisplayTextSourceMapping::NaturalText,
                ),
                DisplayTextComposition::Independent => self.push_text_item(
                    run.text.as_ref(),
                    face_id,
                    &item.span,
                    DisplayTextClustering::Independent,
                    DisplayTextSourceMapping::NaturalText,
                ),
                DisplayTextComposition::Automatic(terminal) => {
                    let mapping =
                        DisplayTextSourceMapping::NaturalText.resolve(&item.span, self.row);
                    let advance =
                        self.automatic_composition_advance_px(&run.text, face_id, &terminal);
                    self.push_automatic_composition(&run.text, terminal, face_id, mapping, advance);
                }
            },
            DisplayItemKind::SourceMappedText(text) => {
                self.push_text_item(
                    text.text.as_ref(),
                    face_id,
                    &item.span,
                    DisplayTextClustering::UnicodeFallback,
                    DisplayTextSourceMapping::SourceMapped {
                        glyph_string_start: text.glyph_string_start.as_ref(),
                    },
                );
            }
            DisplayItemKind::Stretch(stretch) => {
                self.push_stretch(stretch, face_id, source_span_start_char(&item.span))
            }
            DisplayItemKind::MediaReplacement(media) => self.push_media(
                media.apply_positive_box_expansion(box_vertical_edges),
                face_id,
                source_span_start_char(&item.span),
            ),
            DisplayItemKind::ControlChar { ch } => {
                self.push_control_char(ch, face_id, source_span_start_char(&item.span));
            }
            DisplayItemKind::Glyphless(glyphless) => {
                self.push_glyphless(glyphless, face_id, source_span_start_char(&item.span));
            }
            DisplayItemKind::RowBreak(_) => {}
        }
        self.apply_item_layout_since(before_len, face_id, item_layout);
        let pointer_appearance = if self.row.glyphs[area_index].len() > before_len {
            pointer_appearance.and_then(|appearance| self.row.intern_pointer_appearance(appearance))
        } else {
            None
        };
        for glyph in &mut self.row.glyphs[area_index][before_len..] {
            glyph.pointer_appearance = pointer_appearance;
        }
        let appended = &mut self.row.glyphs[area_index][before_len..];
        apply_box_run_topology_to_glyphs(appended, box_run_membership, box_vertical_edges, true);
        DisplayRowWriteMetrics::from_glyphs(
            &self.row.glyphs[area_index][before_len..],
            self.layout.char_width_px,
        )
    }

    #[cfg(test)]
    fn push_source(
        &mut self,
        source: &mut impl DisplayItemSource,
        context: &mut DisplaySourceContext<'_>,
    ) -> DisplayRowWriteMetrics {
        let mut metrics = DisplayRowWriteMetrics::default();
        while let Some(item) = source.next_item(context) {
            metrics.add(self.push_item(item));
        }
        metrics
    }

    fn push_text_item(
        &mut self,
        text: &str,
        face_id: FaceId,
        span: &SourceSpan,
        clustering: DisplayTextClustering,
        source_mapping: DisplayTextSourceMapping<'_>,
    ) {
        if text.is_empty() {
            return;
        }
        let source_mapping = source_mapping.resolve(span, self.row);
        let measurement = self.text_run_measurement(text, face_id);
        let mut byte_offset = 0usize;
        for (char_offset, ch) in text.chars().enumerate() {
            let char_state = match clustering {
                DisplayTextClustering::UnicodeFallback => self.text_char_state(ch),
                DisplayTextClustering::Independent => DisplayRowTextCharState::independent(ch),
            };
            self.push_text_char_with_measurement(
                char_state,
                face_id,
                source_mapping.provenance(char_offset),
                char_offset,
                byte_offset,
                &measurement,
            );
            byte_offset += ch.len_utf8();
        }
    }

    fn automatic_composition_advance_px(
        &mut self,
        text: &str,
        face_id: FaceId,
        terminal: &TerminalComposition,
    ) -> f32 {
        let backend = self
            .glyph_measurer
            .as_deref()
            .map(DisplayGlyphMeasurer::display_geometry_backend)
            .unwrap_or(DisplayGeometryBackend::WindowSystemPixels);
        if backend == DisplayGeometryBackend::TerminalCells {
            return f32::from(terminal.width_cols) * self.layout.char_width_px.max(1.0);
        }

        self.text_run_measurement(text, face_id)
            .measured_advances()
            .map(|advances| advances.iter().map(|advance| advance.advance_px).sum())
            .filter(|width: &f32| width.is_finite() && *width > 0.0)
            .unwrap_or_else(|| f32::from(terminal.width_cols) * self.layout.char_width_px.max(1.0))
    }

    fn push_automatic_composition(
        &mut self,
        text: &str,
        terminal: TerminalComposition,
        face_id: FaceId,
        source_mapping: ResolvedDisplayTextSourceMapping,
        pixel_width: f32,
    ) {
        let Some(first) = text.chars().next() else {
            return;
        };
        let area_index = self.area_index();
        let mut base = Glyph::char_with_provenance(first, face_id, source_mapping.provenance(0));
        base.glyph_type = GlyphType::AutomaticComposite {
            text: text.into(),
            terminal,
        };
        base.pixel_width = pixel_width.max(0.0);
        if let Some(vertical) = self
            .glyph_measurer
            .as_deref_mut()
            .and_then(|measurer| measurer.glyph_vertical_metrics_px(first, face_id))
        {
            base.pixel_height = vertical.height_px;
            base.pixel_ascent = vertical.ascent_px;
        }
        self.row.glyphs[area_index].push(base);
        for (char_offset, ch) in text.chars().enumerate().skip(1) {
            let mut padding =
                Glyph::padding_with_provenance(face_id, source_mapping.provenance(char_offset));
            padding.glyph_type = GlyphType::Char { ch };
            self.row.glyphs[area_index].push(padding);
        }
        if area_index == GlyphArea::Text.index() {
            self.row.displays_text = true;
        }
    }

    fn push_text_char(&mut self, ch: char, face_id: FaceId, charpos: usize) {
        let char_state = self.text_char_state(ch);
        self.push_text_char_state_at_position(
            char_state,
            face_id,
            GlyphProvenance::buffer(charpos),
            self.current_text_position(),
        );
    }

    fn push_text_char_with_measurement(
        &mut self,
        char_state: DisplayRowTextCharState,
        face_id: FaceId,
        provenance: GlyphProvenance,
        char_offset: usize,
        byte_offset: usize,
        measurement: &DisplayTextRunMeasurement,
    ) {
        self.push_text_char_state_with_advance_request(
            provenance,
            DisplayRowTextCharAdvanceRequest::new(
                char_state,
                face_id,
                self.current_text_position(),
                char_offset,
                byte_offset,
                measurement,
            ),
        );
    }

    fn push_text_char_state_at_position(
        &mut self,
        char_state: DisplayRowTextCharState,
        face_id: FaceId,
        provenance: GlyphProvenance,
        position: DisplayRowPosition,
    ) {
        let measurement = DisplayTextRunMeasurement::PerChar;
        self.push_text_char_state_with_advance_request(
            provenance,
            DisplayRowTextCharAdvanceRequest::per_char(char_state, face_id, position, &measurement),
        );
    }

    fn push_text_char_state_with_advance_request(
        &mut self,
        provenance: GlyphProvenance,
        advance_request: DisplayRowTextCharAdvanceRequest<'_>,
    ) {
        let ch = advance_request.ch();
        let face_id = advance_request.face_id;
        let area_index = self.area_index();
        let before_len = self.row.glyphs[area_index].len();
        match advance_request.kind() {
            DisplayRowTextNaturalAdvanceKind::Tab => {
                self.push_tab_at_position(
                    advance_request.face_id,
                    advance_request.position,
                    provenance,
                );
            }
            DisplayRowTextNaturalAdvanceKind::ClusterContinuation => {
                glyph_row_writer::push_cluster_continuation_to_area(
                    self.row,
                    area_index,
                    ch,
                    advance_request.face_id,
                    provenance,
                );
            }
            DisplayRowTextNaturalAdvanceKind::ComplexRunMember => {
                let advance = advance_request.resolve_advance_px_with_writer(self);
                glyph_row_writer::push_run_member_to_area(
                    self.row,
                    area_index,
                    ch,
                    advance_request.face_id,
                    provenance,
                    advance,
                );
            }
            DisplayRowTextNaturalAdvanceKind::FaceColumns { columns } if columns > 1 => {
                let advance = advance_request.resolve_advance_px_with_writer(self);
                glyph_row_writer::push_wide_char_to_area(
                    self.row,
                    area_index,
                    ch,
                    advance_request.face_id,
                    provenance,
                    advance,
                );
            }
            DisplayRowTextNaturalAdvanceKind::StandaloneZeroWidth => {
                glyph_row_writer::push_standalone_zero_width_char_to_area(
                    self.row,
                    area_index,
                    ch,
                    advance_request.face_id,
                    provenance,
                );
            }
            DisplayRowTextNaturalAdvanceKind::FaceColumns { .. } => {
                let advance = advance_request.resolve_advance_px_with_writer(self);
                glyph_row_writer::push_char_to_area(
                    self.row,
                    area_index,
                    ch,
                    advance_request.face_id,
                    provenance,
                    advance,
                );
            }
        }
        let Some(metrics) = self
            .glyph_measurer
            .as_deref_mut()
            .and_then(|measurer| measurer.glyph_vertical_metrics_px(ch, face_id))
        else {
            return;
        };
        for glyph in &mut self.row.glyphs[area_index][before_len..] {
            if !glyph.padding {
                glyph.pixel_height = metrics.height_px;
                glyph.pixel_ascent = metrics.ascent_px;
            }
        }
    }

    fn apply_item_layout_since(
        &mut self,
        before_len: usize,
        face_id: FaceId,
        item_layout: DisplayItemLayout,
    ) {
        let geometry_backend = self
            .glyph_measurer
            .as_deref()
            .map(DisplayGlyphMeasurer::display_geometry_backend)
            .unwrap_or(DisplayGeometryBackend::WindowSystemPixels);
        let applied_space_width = match geometry_backend {
            DisplayGeometryBackend::TerminalCells => None,
            DisplayGeometryBackend::WindowSystemPixels => item_layout.space_width,
        };
        let face_space_width = applied_space_width.and_then(|_| {
            self.glyph_measurer
                .as_deref_mut()
                .and_then(|measurer| measurer.face_space_width_px(face_id))
                .filter(|width| width.is_finite() && *width > 0.0)
        });
        let face_vertical_metrics = applied_space_width.and_then(|_| {
            self.glyph_measurer
                .as_deref_mut()
                .and_then(|measurer| measurer.face_vertical_metrics_px(face_id))
        });
        let area_index = self.area_index();
        for glyph in &mut self.row.glyphs[area_index][before_len..] {
            if matches!(glyph.glyph_type, GlyphType::Char { ch: ' ' }) {
                // GNU xdisp turns a space carrying `(space-width FACTOR)`
                // into a stretch glyph.  Its width and vertical box come
                // from the face's PRIMARY font, not from the fallback font
                // that happens to cover U+0020.
                if applied_space_width.is_some() {
                    glyph.glyph_type = GlyphType::Stretch { width_cols: 1 };
                    let natural_width = face_space_width.unwrap_or(glyph.pixel_width);
                    glyph.pixel_width = item_layout.horizontal_advance_px(' ', natural_width);
                    if let Some(metrics) = face_vertical_metrics {
                        glyph.pixel_height = metrics.height_px;
                        glyph.pixel_ascent = metrics.ascent_px;
                    }
                }
            }
            let reference_height = if glyph.pixel_height > 0.0 {
                glyph.pixel_height
            } else {
                self.layout.height_px
            };
            glyph.vertical_offset_px = item_layout.vertical_offset_px(reference_height);
        }
        let vertical_metrics = self.row.glyphs[area_index][before_len..]
            .iter()
            .filter_map(|glyph| {
                DisplayRowVerticalMetrics::from_glyph(glyph)
                    .map(|metrics| metrics.with_vertical_offset(glyph.vertical_offset_px))
            })
            .collect::<Vec<_>>();
        for metrics in vertical_metrics {
            metrics.include_in_row(self.row);
        }
    }

    fn item_horizontal_advance_px(
        &mut self,
        ch: char,
        face_id: FaceId,
        natural_advance_px: f32,
        item_layout: DisplayItemLayout,
    ) -> f32 {
        let geometry_backend = self
            .glyph_measurer
            .as_deref()
            .map(DisplayGlyphMeasurer::display_geometry_backend)
            .unwrap_or(DisplayGeometryBackend::WindowSystemPixels);
        let applied_space_width = match geometry_backend {
            DisplayGeometryBackend::TerminalCells => None,
            DisplayGeometryBackend::WindowSystemPixels => item_layout.space_width,
        };
        let natural_advance_px = if ch == ' ' && applied_space_width.is_some() {
            self.glyph_measurer
                .as_deref_mut()
                .and_then(|measurer| measurer.face_space_width_px(face_id))
                .filter(|width| width.is_finite() && *width > 0.0)
                .unwrap_or(natural_advance_px)
        } else {
            natural_advance_px
        };
        DisplayItemLayout {
            space_width: applied_space_width,
            ..item_layout
        }
        .horizontal_advance_px(ch, natural_advance_px)
    }

    fn text_char_state(&self, ch: char) -> DisplayRowTextCharState {
        DisplayRowTextCharState::for_glyphs(ch, &self.row.glyphs[self.area_index()])
    }

    fn text_run_measurement(&mut self, text: &str, face_id: FaceId) -> DisplayTextRunMeasurement {
        self.glyph_measurer
            .as_mut()
            .map(|measurer| {
                measurer.text_run_advances_px(text, face_id, self.layout.char_width_px.max(1.0))
            })
            .unwrap_or(DisplayTextRunMeasurement::PerChar)
    }

    fn push_tab_at_position(
        &mut self,
        face_id: FaceId,
        position: DisplayRowPosition,
        provenance: GlyphProvenance,
    ) {
        // GNU's gui_produce_glyphs uses the TAB face's primary font
        // `space_width`, not character fallback selection for U+0020.
        let space_width_px = self
            .glyph_measurer
            .as_deref_mut()
            .and_then(|measurer| measurer.face_space_width_px(face_id))
            .filter(|width| width.is_finite() && *width > 0.0)
            .unwrap_or_else(|| self.glyph_advance_px(' ', face_id, 1));
        let advance = self
            .layout
            .tab_policy
            .advance_from(position, space_width_px);
        let width_cols = advance.width_cols.min(usize::from(u16::MAX)) as u16;
        glyph_row_writer::push_stretch_to_area(
            self.row,
            self.area_index(),
            width_cols,
            face_id,
            advance.pixel_width,
            0.0,
            0.0,
            // A TAB is a buffer character that happens to render as a
            // stretch; its glyph addresses the tab itself.
            provenance,
        );
    }

    fn current_text_position(&self) -> DisplayRowPosition {
        let metrics = self.current_text_metrics();
        DisplayRowPosition::new(
            self.layout.tab_policy.origin_x_px + self.row.pixel_x + metrics.width_px(),
            usize::from(self.row.start_col) + metrics.width_cols(),
        )
    }

    /// Capture the initial pen and display-slot position on the row itself.
    ///
    /// The old media side channel remembered this position separately while
    /// ordinary glyph materialization always began at column zero.  Stamp an
    /// empty text row once so every primitive follows the same geometry.  A
    /// non-empty row already owns its origin and must never be repositioned by
    /// a later append fragment.
    fn set_empty_row_start(&mut self, requested: DisplayRowPosition) {
        if self.area != GlyphArea::Text || display_row_glyph_count(self.row) != 0 {
            return;
        }

        let origin_x = self.layout.tab_policy.origin_x_px;
        if requested.x_px() >= origin_x {
            self.row.pixel_x = requested.x_px() - origin_x;
        }
        self.row.start_col = requested.col().min(usize::from(u16::MAX)) as u16;
    }

    fn current_text_metrics(&self) -> DisplayRowWriteMetrics {
        DisplayRowWriteMetrics::from_glyphs(
            &self.row.glyphs[self.area_index()],
            self.layout.char_width_px,
        )
    }

    fn glyph_advance_px(&mut self, ch: char, face_id: FaceId, columns: usize) -> f32 {
        let fallback = self.layout.char_width_px.max(1.0) * columns.max(1) as f32;
        let measured_columns = columns.min(usize::from(u8::MAX)) as u8;
        self.glyph_measurer
            .as_mut()
            .and_then(|measurer| measurer.glyph_advance_px(ch, face_id, measured_columns, fallback))
            .filter(|advance| advance.is_finite() && *advance >= 0.0)
            .unwrap_or(fallback)
    }

    fn push_stretch(&mut self, stretch: DisplayStretch, face_id: FaceId, charpos: usize) {
        let Some((width_cols, pixel_width)) = self.stretch_width(&stretch.width, face_id) else {
            return;
        };
        // GNU `produce_stretch_glyph` accepts zero as the result of valid
        // `:width` and `:align-to` forms, but only appends a glyph when the
        // computed width is positive (xdisp.c).  Keeping a zero-width Stretch
        // in the matrix is not inert: terminal column accounting deliberately
        // gives every materialized glyph at least one cell.
        if pixel_width <= 0.0 {
            return;
        }
        let pixel_height = stretch
            .height
            .as_ref()
            .and_then(|length| {
                self.length_pixels(length, self.layout.height_px, PixelCalcAxis::Vertical)
            })
            .unwrap_or(0.0);
        let pixel_ascent = stretch
            .ascent
            .as_ref()
            .and_then(|length| {
                self.length_pixels(length, self.layout.ascent_px, PixelCalcAxis::Vertical)
            })
            .unwrap_or(0.0);

        glyph_row_writer::push_stretch_to_area(
            self.row,
            self.area_index(),
            width_cols,
            face_id,
            pixel_width,
            pixel_height,
            pixel_ascent,
            GlyphProvenance::buffer(charpos),
        );
        self.promote_row_metrics_for_explicit_stretch();
    }

    fn push_media(&mut self, media: DisplayMediaReplacement, face_id: FaceId, charpos: usize) {
        if matches!(media.kind, DisplayMediaReplacementKind::EmptyImageSlice) {
            return;
        }
        let Some((width_cols, pixel_width)) =
            self.stretch_width(&media.replacement_stretch().width, face_id)
        else {
            return;
        };
        let glyph_type = match media.kind {
            DisplayMediaReplacementKind::Image {
                image_id,
                source_rect,
                margin_left,
                margin_right,
                margin_top,
                margin_bottom,
                opaque_background,
            } => {
                let margins = neomacs_display_protocol::ImageMargins::asymmetric(
                    margin_left,
                    margin_right,
                    margin_top,
                    margin_bottom,
                );
                let Some(margins) = self.row.intern_image_margins(margins) else {
                    return;
                };
                GlyphType::Image {
                    image_id: image_id as i32,
                    width_cols,
                    source_rect,
                    margins,
                    opaque_background: neomacs_display_protocol::ImageOpaqueBackground::new(
                        opaque_background,
                    ),
                }
            }
            DisplayMediaReplacementKind::EmptyImageSlice => {
                unreachable!("empty image slices return before glyph construction")
            }
            DisplayMediaReplacementKind::Video { video_id, opacity } => GlyphType::Video {
                video_id,
                width_cols,
                opacity,
            },
            DisplayMediaReplacementKind::Xwidget {
                xwidget_id,
                webview_id,
                content,
            } => GlyphType::Xwidget {
                xwidget_id,
                webview_id,
                width_cols,
                content,
            },
            DisplayMediaReplacementKind::Surface { surface_id } => GlyphType::Surface {
                surface_id: surface_id as i32,
                width_cols,
            },
        };
        let mut glyph = Glyph::stretch(width_cols, face_id).with_pixel_geometry(
            pixel_width,
            media.height,
            media.ascent,
        );
        glyph.glyph_type = glyph_type;
        glyph.provenance = GlyphProvenance::buffer(charpos);
        let area_index = self.area_index();
        self.row.glyphs[area_index].push(glyph);
        if self.area == GlyphArea::Text {
            self.row.displays_text = true;
        }
        self.promote_row_metrics_for_explicit_stretch();
    }

    fn promote_row_metrics_for_explicit_stretch(&mut self) {
        let Some(glyph) = self.row.glyphs[self.area_index()].last() else {
            return;
        };
        if let Some(metrics) = DisplayRowVerticalMetrics::from_glyph(glyph) {
            if display_row_glyph_count(self.row) == 1 {
                self.row.height_px = metrics.height_px.max(1.0);
                self.row.ascent_px = metrics.ascent_px.max(0.0);
            } else {
                metrics.include_in_row(self.row);
            }
        }
    }

    fn push_control_char(&mut self, ch: char, face_id: FaceId, charpos: usize) {
        let Some(caret_char) = control_char_caret_char(ch) else {
            return;
        };
        self.push_text_char('^', face_id, charpos);
        self.push_text_char(caret_char, face_id, charpos);
    }

    fn push_glyphless(&mut self, glyphless: DisplayGlyphless, face_id: FaceId, charpos: usize) {
        if let GlyphlessMethod::Acronym(acronym) = glyphless.method {
            for ch in acronym.tty_text().chars() {
                self.push_text_char(ch, face_id, charpos);
            }
            return;
        }
        let Some((presentation, pixel_width)) = self.glyphless_presentation(&glyphless) else {
            return;
        };
        let glyph = Glyph {
            glyph_type: GlyphType::Glyphless {
                ch: glyphless.ch,
                presentation,
            },
            face_id,
            provenance: GlyphProvenance::buffer(charpos),
            bidi_level: 0,
            wide: false,
            pixel_width,
            pixel_height: 0.0,
            pixel_ascent: 0.0,
            vertical_offset_px: 0.0,
            padding: false,
            box_vertical_edges: Default::default(),
            pointer_appearance: None,
        };
        let area_index = self.area_index();
        self.row.glyphs[area_index].push(glyph);
        if self.area == GlyphArea::Text {
            self.row.displays_text = true;
        }
    }

    fn glyphless_presentation(
        &self,
        glyphless: &DisplayGlyphless,
    ) -> Option<(
        neomacs_display_protocol::glyph_matrix::GlyphlessPresentation,
        f32,
    )> {
        use neomacs_display_protocol::glyph_matrix::GlyphlessPresentation;
        let char_width_px = self.layout.char_width_px.max(1.0);
        match glyphless.method {
            GlyphlessMethod::ZeroWidth => None,
            GlyphlessMethod::ThinSpace => {
                Some((GlyphlessPresentation::ThinSpace, char_width_px * 0.25))
            }
            GlyphlessMethod::EmptyBox => Some((GlyphlessPresentation::EmptyBox, {
                let width_cols = base_width_cols(glyphless.ch).clamp(1, 4);
                char_width_px * f32::from(width_cols)
            })),
            GlyphlessMethod::HexCode => Some((GlyphlessPresentation::HexCode, {
                let label_cols = if (glyphless.ch as u32) < 0x10000 {
                    6
                } else {
                    8
                };
                char_width_px * label_cols as f32
            })),
            GlyphlessMethod::Acronym(_) => {
                unreachable!("glyphless acronyms are lowered to text before presentation")
            }
        }
    }

    fn stretch_width(
        &mut self,
        width: &DisplayStretchWidth,
        face_id: FaceId,
    ) -> Option<(u16, f32)> {
        match width {
            DisplayStretchWidth::Length(length) => {
                let pixels = self.length_pixels(
                    length,
                    self.layout.char_width_px,
                    PixelCalcAxis::Horizontal,
                )?;
                let cols = (pixels / self.layout.char_width_px.max(1.0))
                    .ceil()
                    .max(1.0) as u16;
                Some((cols, pixels))
            }
            DisplayStretchWidth::RelativeToSource { factor, source } => {
                let source_char = source.as_rust_char().unwrap_or(' ');
                let source_columns = usize::from(base_width_cols(source_char).max(1));
                let geometry_backend = self
                    .glyph_measurer
                    .as_deref()
                    .map(DisplayGlyphMeasurer::display_geometry_backend)
                    .unwrap_or(DisplayGeometryBackend::WindowSystemPixels);
                match geometry_backend {
                    // On a TTY GNU's `it2.pixel_width` is a terminal-cell
                    // count. Assignment to integer `width` truncates the
                    // factor product before the nonzero fallback, and only
                    // then do we map those cells into this engine's logical
                    // pixel coordinates.
                    DisplayGeometryBackend::TerminalCells => {
                        let columns = (*factor * source_columns as f32).trunc().max(1.0) as u16;
                        Some((
                            columns,
                            f32::from(columns) * self.layout.char_width_px.max(1.0),
                        ))
                    }
                    // A window-system frame performs the same integer
                    // assignment in concrete pixels after measuring the
                    // covered character in its selected font.
                    DisplayGeometryBackend::WindowSystemPixels => {
                        let source_advance =
                            self.glyph_advance_px(source_char, face_id, source_columns);
                        let pixels = (*factor * source_advance).trunc().max(1.0);
                        let columns = (pixels / self.layout.char_width_px.max(1.0))
                            .ceil()
                            .max(1.0) as u16;
                        Some((columns, pixels))
                    }
                }
            }
            DisplayStretchWidth::AlignTo(prop) => {
                // GNU `display_line`/`produce_stretch_glyph` (xdisp.c) feeds
                // the `:align-to` operand to `calc_pixel_width_or_height` with
                // `*align_to == -1`, then takes the difference from the current
                // pen X. We mirror the buffer text path exactly (see
                // `DisplaySpaceWidthPolicy::resolve`), substituting
                // the chrome row's pen X as `current_x`. For window chrome
                // rows, GNU then adds `window_box_left_offset (TEXT_AREA)` to
                // raw numeric targets; region-symbol targets have already set
                // `align_to >= 0` and keep the resolved region coordinate.
                let mut align_to: i32 = -1;
                // An `(image …)` operand resolves to the image's own pixel
                // width (GNU `lookup_image`, xdisp.c:30506). Resolve the
                // operand's images for this evaluation; the sizes are owned, so
                // the evaluator itself stays free of the display host.
                let pixel_calc = self.layout.pixel_calc_for_space_spec(prop);
                let evaluated =
                    calc_pixel_width_or_height(&pixel_calc, prop, true, Some(&mut align_to));
                // GNU xdisp.c:32878 — when no width form computes, the stretch
                // falls back to the canonical char width rather than vanishing.
                let Some(pixels) = evaluated else {
                    return Some((1, self.layout.char_width_px.max(1.0)));
                };
                let content_x = self.layout.tab_policy.origin_x_px;
                let raw_align_base_x = if align_to < 0
                    && matches!(
                        self.layout.role,
                        GlyphRowRole::ModeLine | GlyphRowRole::HeaderLine | GlyphRowRole::TabLine
                    ) {
                    self.layout.pixel_calc.text_area_left as f32
                } else {
                    content_x
                };
                let target_x = if align_to >= 0 {
                    align_to as f32 + pixels as f32
                } else {
                    raw_align_base_x + pixels as f32
                };
                let current =
                    self.layout.tab_policy.origin_x_px + self.current_text_metrics().width_px();
                let width_px = (target_x - current).max(0.0);
                if !width_px.is_finite() {
                    return None;
                }
                let cols = (width_px / self.layout.char_width_px.max(1.0)).round() as u16;
                Some((cols, width_px))
            }
        }
    }

    /// Evaluate a `(space :width/:height/:ascent …)` length.
    ///
    /// `axis` selects GNU's `width_p`: `:width` is horizontal (xdisp.c:32794),
    /// while `:height` (:32893) and `:ascent` (:32914) are vertical, so bare
    /// numbers scale by `FRAME_LINE_HEIGHT` rather than `FRAME_COLUMN_WIDTH`
    /// and `in`/`mm`/`cm` convert through the vertical resolution.
    fn length_pixels(
        &self,
        length: &DisplayLength,
        em_px: f32,
        axis: PixelCalcAxis,
    ) -> Option<f32> {
        match length {
            DisplayLength::Columns(cols) => Some(f32::from(*cols) * self.layout.char_width_px),
            DisplayLength::Pixels(px) => Some(*px),
            DisplayLength::Em(em) => Some(*em * em_px.max(1.0)),
            // `:width`/`:height` arithmetic forms route through the single
            // GNU-faithful evaluator. `em_px` (the caller's base unit) is the
            // frame column/line size in the pixel-calc context already, so the
            // authority scales bare numbers consistently with the explicit
            // `Em` arm above.
            DisplayLength::Expr(prop) => {
                let pixel_calc = self.layout.pixel_calc_for_space_spec(prop);
                calc_pixel_width_or_height(&pixel_calc, prop, axis.is_horizontal(), None)
                    .map(|pixels| pixels as f32)
            }
        }
        .filter(|pixels| pixels.is_finite() && *pixels >= 0.0)
    }

    fn face_id(&self, face: RenderFaceRef) -> FaceId {
        render_face_ref_id(
            face,
            render_face_ref_id(self.layout.base_face, FaceId::new(0)),
        )
    }
}

fn source_span_start_char(span: &SourceSpan) -> usize {
    match &span.start {
        DisplaySourcePosition::Buffer { char_pos, .. } => char_pos.get(),
        DisplaySourcePosition::LispString { char_index, .. } => *char_index,
        // Redisplay-owned glyphs have no buffer object/position.  GNU stamps
        // line-number glyphs with position -1 before copying them into
        // TEXT_AREA; the protocol sentinel is the typed Rust equivalent.
        DisplaySourcePosition::Synthetic { .. } => NO_BUFFER_POSITION_CHARPOS,
    }
}

#[cfg(test)]
#[path = "builder_test.rs"]
mod tests;

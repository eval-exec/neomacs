//! Glyphs methods for WgpuRenderer.

use super::super::glyph_atlas::{ComposedGlyphKey, GlyphKey, WgpuGlyphAtlas};
use super::super::vertex::{RectVertex, SubpixelGlyphVertex, Uniforms};
use super::GlyphRenderStats;
use super::ModeLineFadeEntry;
use super::WgpuRenderer;
use super::cursor_presentation::{
    CursorColorPolicy, CursorShape, FilledBoxPresentation, InverseVideoCell, PresentedCursorPaint,
    ResolvedCursorPaint,
};
use super::frame_pass::{BoxSpan, FrameParams, FramePassCtx};
use super::scissor::SurfaceScissor;
use neomacs_display_protocol::PointerAppearanceSelection;
use neomacs_display_protocol::effect_config::EffectsConfig;
use neomacs_display_protocol::face::Face;
use neomacs_display_protocol::frame_glyphs::{
    CursorStyle, DisplaySlotId, FrameGlyph, FrameGlyphBuffer, GlyphRowRole, WindowCursor,
};
use neomacs_display_protocol::gradient::{ColorStop, Gradient};
use neomacs_display_protocol::types::FaceId;
use neomacs_display_protocol::types::{AnimatedCursor, Color, DisplayWindowId, Rect};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{
    OnceLock,
    atomic::{AtomicU64, Ordering},
};

pub(super) const CHAR_OVERLAP_MIN_AXIS: f32 = 0.5;
const CHAR_OVERLAP_MIN_AREA: f32 = 1.0;
const CHAR_OVERLAP_LOG_LIMIT: usize = 32;

#[derive(Debug, Clone)]
pub(super) struct RenderedCharBounds {
    pub(super) glyph_index: usize,
    pub(super) row_role: GlyphRowRole,
    pub(super) slot_id: DisplaySlotId,
    pub(super) label: String,
    pub(super) face_id: FaceId,
    pub(super) font_size: f32,
    pub(super) geometry: RenderedGlyphGeometry,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct RenderedGlyphGeometry {
    /// The layout-owned advance cell.
    cell: Rect,
    /// The clipped bitmap rectangle actually submitted for rendering.
    bitmap: Rect,
}

impl RenderedGlyphGeometry {
    pub(super) const fn new(cell: Rect, bitmap: Rect) -> Self {
        Self { cell, bitmap }
    }

    #[cfg(test)]
    pub(super) const fn cell(self) -> Rect {
        self.cell
    }

    #[cfg(test)]
    pub(super) const fn bitmap(self) -> Rect {
        self.bitmap
    }

    pub(super) fn translated_y(mut self, dy: f32) -> Self {
        self.cell.y += dy;
        self.bitmap.y += dy;
        self
    }

    fn overhang(self) -> GlyphOverhang {
        GlyphOverhang {
            left: (self.cell.x - self.bitmap.x).max(0.0),
            right: (self.bitmap.right() - self.cell.right()).max(0.0),
            top: (self.cell.y - self.bitmap.y).max(0.0),
            bottom: (self.bitmap.bottom() - self.cell.bottom()).max(0.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct GlyphOverhang {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlapAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AxisSpan {
    start: f32,
    end: f32,
}

impl OverlapAxis {
    fn project(self, rect: Rect) -> AxisSpan {
        match self {
            Self::Horizontal => AxisSpan {
                start: rect.x,
                end: rect.right(),
            },
            Self::Vertical => AxisSpan {
                start: rect.y,
                end: rect.bottom(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedCharOverlap {
    HorizontalOverhang,
    VerticalOverhang,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharOverlapClassification {
    Expected(ExpectedCharOverlap),
    Unexpected,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CharOverlap {
    bounds: Rect,
    classification: CharOverlapClassification,
}

fn char_overlap(a: &RenderedCharBounds, b: &RenderedCharBounds) -> Option<CharOverlap> {
    let x0 = a.geometry.bitmap.x.max(b.geometry.bitmap.x);
    let y0 = a.geometry.bitmap.y.max(b.geometry.bitmap.y);
    let x1 = a.geometry.bitmap.right().min(b.geometry.bitmap.right());
    let y1 = a.geometry.bitmap.bottom().min(b.geometry.bitmap.bottom());
    let width = x1 - x0;
    let height = y1 - y0;
    if width <= CHAR_OVERLAP_MIN_AXIS
        || height <= CHAR_OVERLAP_MIN_AXIS
        || width * height <= CHAR_OVERLAP_MIN_AREA
    {
        return None;
    }
    let bounds = Rect::new(x0, y0, width, height);
    let classification = classify_char_overlap(a, b, bounds);
    Some(CharOverlap {
        bounds,
        classification,
    })
}

fn classify_char_overlap(
    a: &RenderedCharBounds,
    b: &RenderedCharBounds,
    overlap: Rect,
) -> CharOverlapClassification {
    if overlap_is_expected_on_axis(a, b, overlap, OverlapAxis::Horizontal) {
        CharOverlapClassification::Expected(ExpectedCharOverlap::HorizontalOverhang)
    } else if overlap_is_expected_on_axis(a, b, overlap, OverlapAxis::Vertical) {
        CharOverlapClassification::Expected(ExpectedCharOverlap::VerticalOverhang)
    } else {
        CharOverlapClassification::Unexpected
    }
}

fn overlap_is_expected_on_axis(
    a: &RenderedCharBounds,
    b: &RenderedCharBounds,
    overlap: Rect,
    axis: OverlapAxis,
) -> bool {
    // Match GNU redisplay's ownership discipline: horizontal bearings belong
    // to one glyph row, while vertical overlap belongs to adjacent rows. A
    // geometric intersection alone must never erase that logical boundary.
    let same_render_run = a.slot_id.window_id == b.slot_id.window_id && a.row_role == b.row_role;
    let grid_neighbors = same_render_run
        && match axis {
            OverlapAxis::Horizontal => a.slot_id.row == b.slot_id.row,
            OverlapAxis::Vertical => {
                a.slot_id.col == b.slot_id.col && a.slot_id.row.abs_diff(b.slot_id.row) == 1
            }
        };
    if !grid_neighbors {
        return false;
    }

    let a_cell = axis.project(a.geometry.cell);
    let b_cell = axis.project(b.geometry.cell);
    let a_bitmap = axis.project(a.geometry.bitmap);
    let b_bitmap = axis.project(b.geometry.bitmap);
    let overlap = axis.project(overlap);
    // Order by cell start, then by cell end, so a zero-width cell sorts
    // before the cell that begins where it sits regardless of argument order.
    let (before_cell, before_bitmap, after_cell, after_bitmap) = if a_cell
        .start
        .total_cmp(&b_cell.start)
        .then(a_cell.end.total_cmp(&b_cell.end))
        .is_le()
    {
        (a_cell, a_bitmap, b_cell, b_bitmap)
    } else {
        (b_cell, b_bitmap, a_cell, a_bitmap)
    };

    // Advance cells that intersect are a layout defect no bearing explains.
    if before_cell.end > after_cell.start + CHAR_OVERLAP_MIN_AXIS {
        return false;
    }
    match axis {
        // GNU xdisp.c `right_overwritten` / `left_overwriting` walk the
        // following (preceding) glyphs while the overhang still exceeds their
        // summed `pixel_width`, so a bearing legitimately reaches past a
        // narrower intervening cell; the cells need not touch. GNU bounds
        // that reach by the font's own bearing (`gui_get_glyph_overhangs`),
        // which this diagnostic cannot see; the substitute below is that the
        // reaching ink must start inside its own cell.
        OverlapAxis::Horizontal => {}
        // Vertical overlap belongs to adjacent rows and must straddle the
        // boundary they share.
        OverlapAxis::Vertical => {
            if !approx_eq(before_cell.end, after_cell.start, CHAR_OVERLAP_MIN_AXIS) {
                return false;
            }
            let shared_cell_boundary = before_cell.end;
            if overlap.start > shared_cell_boundary + CHAR_OVERLAP_MIN_AXIS
                || overlap.end < shared_cell_boundary - CHAR_OVERLAP_MIN_AXIS
            {
                return false;
            }
        }
    }

    // Overhang is derived from the two rectangles. Keeping it out of stored
    // state makes a bitmap/cell pair with contradictory overhang impossible.
    // Horizontally, a bitmap that lies wholly outside its own cell is
    // displaced, not overhanging: GNU's `rbearing > width` / `lbearing < 0`
    // describe ink that starts in the cell and extends past it. Vertical
    // overlap is already bound to the shared row boundary above.
    let starts_in_own_cell = |bitmap: AxisSpan, cell: AxisSpan| {
        axis == OverlapAxis::Vertical
            || (bitmap.start < cell.end + CHAR_OVERLAP_MIN_AXIS
                && bitmap.end > cell.start - CHAR_OVERLAP_MIN_AXIS)
    };
    let before_extends_after_cell =
        before_bitmap.end > before_cell.end && starts_in_own_cell(before_bitmap, before_cell);
    let after_extends_before_cell =
        after_bitmap.start < after_cell.start && starts_in_own_cell(after_bitmap, after_cell);
    if !before_extends_after_cell && !after_extends_before_cell {
        return false;
    }

    let expected_start = if after_extends_before_cell {
        after_bitmap.start
    } else {
        before_cell.end
    };
    let expected_end = if before_extends_after_cell {
        before_bitmap.end
    } else {
        after_cell.start
    };

    overlap.start >= expected_start - CHAR_OVERLAP_MIN_AXIS
        && overlap.end <= expected_end + CHAR_OVERLAP_MIN_AXIS
}

pub(super) fn log_rendered_char_overlaps(
    frame_id: u64,
    pass_name: &str,
    chars: &[RenderedCharBounds],
) -> usize {
    let mut sorted: Vec<&RenderedCharBounds> = chars.iter().collect();
    sorted.sort_by(|a, b| {
        a.geometry
            .bitmap
            .x
            .total_cmp(&b.geometry.bitmap.x)
            .then(a.geometry.bitmap.y.total_cmp(&b.geometry.bitmap.y))
            .then(a.glyph_index.cmp(&b.glyph_index))
    });

    let mut unexpected_total = 0usize;
    let mut overhang_total = 0usize;
    for (i, a) in sorted.iter().enumerate() {
        for b in sorted.iter().skip(i + 1) {
            if b.geometry.bitmap.x >= a.geometry.bitmap.right() {
                break;
            }
            let Some(overlap) = char_overlap(a, b) else {
                continue;
            };
            match overlap.classification {
                CharOverlapClassification::Expected(reason) => {
                    overhang_total += 1;
                    let a_overhang = a.geometry.overhang();
                    let b_overhang = b.geometry.overhang();
                    tracing::debug!(
                        "char_overhang frame_id={} pass={} reason={:?} overlap=({:.1},{:.1},{:.1}x{:.1}) \
                     a[glyph={} label={:?} face={} cell=({:.1},{:.1},{:.1}x{:.1}) \
                     bitmap=({:.1},{:.1},{:.1}x{:.1}) overhang=({:.1},{:.1},{:.1},{:.1})] \
                     b[glyph={} label={:?} face={} cell=({:.1},{:.1},{:.1}x{:.1}) \
                     bitmap=({:.1},{:.1},{:.1}x{:.1}) overhang=({:.1},{:.1},{:.1},{:.1})]",
                        frame_id,
                        pass_name,
                        reason,
                        overlap.bounds.x,
                        overlap.bounds.y,
                        overlap.bounds.width,
                        overlap.bounds.height,
                        a.glyph_index,
                        a.label,
                        a.face_id,
                        a.geometry.cell.x,
                        a.geometry.cell.y,
                        a.geometry.cell.width,
                        a.geometry.cell.height,
                        a.geometry.bitmap.x,
                        a.geometry.bitmap.y,
                        a.geometry.bitmap.width,
                        a.geometry.bitmap.height,
                        a_overhang.left,
                        a_overhang.right,
                        a_overhang.top,
                        a_overhang.bottom,
                        b.glyph_index,
                        b.label,
                        b.face_id,
                        b.geometry.cell.x,
                        b.geometry.cell.y,
                        b.geometry.cell.width,
                        b.geometry.cell.height,
                        b.geometry.bitmap.x,
                        b.geometry.bitmap.y,
                        b.geometry.bitmap.width,
                        b.geometry.bitmap.height,
                        b_overhang.left,
                        b_overhang.right,
                        b_overhang.top,
                        b_overhang.bottom,
                    );
                }
                CharOverlapClassification::Unexpected => {
                    unexpected_total += 1;
                    if unexpected_total <= CHAR_OVERLAP_LOG_LIMIT {
                        tracing::error!(
                            "char_overlap frame_id={} pass={} overlap=({:.1},{:.1},{:.1}x{:.1}) \
                     a[glyph={} window={} role={:?} slot=({}, {}) label={:?} face={} font={:.1} \
                     cell=({:.1},{:.1},{:.1}x{:.1}) bitmap=({:.1},{:.1},{:.1}x{:.1})] \
                     b[glyph={} window={} role={:?} slot=({}, {}) label={:?} face={} font={:.1} \
                     cell=({:.1},{:.1},{:.1}x{:.1}) bitmap=({:.1},{:.1},{:.1}x{:.1})]",
                            frame_id,
                            pass_name,
                            overlap.bounds.x,
                            overlap.bounds.y,
                            overlap.bounds.width,
                            overlap.bounds.height,
                            a.glyph_index,
                            a.slot_id.window_id.get(),
                            a.row_role,
                            a.slot_id.row,
                            a.slot_id.col,
                            a.label,
                            a.face_id,
                            a.font_size,
                            a.geometry.cell.x,
                            a.geometry.cell.y,
                            a.geometry.cell.width,
                            a.geometry.cell.height,
                            a.geometry.bitmap.x,
                            a.geometry.bitmap.y,
                            a.geometry.bitmap.width,
                            a.geometry.bitmap.height,
                            b.glyph_index,
                            b.slot_id.window_id.get(),
                            b.row_role,
                            b.slot_id.row,
                            b.slot_id.col,
                            b.label,
                            b.face_id,
                            b.font_size,
                            b.geometry.cell.x,
                            b.geometry.cell.y,
                            b.geometry.cell.width,
                            b.geometry.cell.height,
                            b.geometry.bitmap.x,
                            b.geometry.bitmap.y,
                            b.geometry.bitmap.width,
                            b.geometry.bitmap.height,
                        );
                    }
                }
            }
        }
    }

    if unexpected_total > CHAR_OVERLAP_LOG_LIMIT {
        tracing::error!(
            "char_overlap frame_id={} pass={} total_overlaps={} suppressed={}",
            frame_id,
            pass_name,
            unexpected_total,
            unexpected_total - CHAR_OVERLAP_LOG_LIMIT
        );
    }
    if overhang_total > 0 {
        tracing::debug!(
            "char_overhang frame_id={} pass={} total_overhang_overlaps={}",
            frame_id,
            pass_name,
            overhang_total
        );
    }
    unexpected_total
}

fn cursor_glyph_slot_rect(
    frame_glyphs: &FrameGlyphBuffer,
    cursor: &WindowCursor,
) -> (f32, f32, f32, f32) {
    // Single source of truth for cursor placement (display-protocol). The
    // animation target and per-window cursors resolve through the same call,
    // so they cannot drift from where the cursor is statically drawn.
    frame_glyphs.cursor_draw_rect(
        cursor.slot_id,
        cursor.style,
        cursor.ascent,
        (cursor.x, cursor.y, cursor.width, cursor.height),
    )
}

/// The relationship a resolved cursor rectangle must have to its owning cell.
///
/// This deliberately models cursor geometry, not glyph ink.  Font ink may be
/// smaller than its cell or overhang it, while GNU Emacs draws bar cursors as
/// independent rectangles on a cell edge.  Keeping the style-to-contract
/// mapping exhaustive makes a new cursor style a compile-time decision here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorCellContract {
    FullCell,
    VerticalLeadingEdge(CursorInlineDirection),
}

/// Physical leading edge selected by the glyph's resolved bidi level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorInlineDirection {
    LeftToRight,
    RightToLeft,
}

impl CursorInlineDirection {
    fn from_bidi_level(level: Option<u8>) -> Self {
        if level.is_some_and(|level| level & 1 != 0) {
            Self::RightToLeft
        } else {
            Self::LeftToRight
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorCellAlignment {
    Aligned,
    Misaligned { expected: CursorCellContract },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ResolvedCursorRect(Rect);

#[derive(Debug, Clone, Copy, PartialEq)]
struct GlyphCellRect(Rect);

impl CursorCellContract {
    fn for_style(style: CursorStyle, direction: CursorInlineDirection) -> Self {
        match style {
            CursorStyle::FilledBox | CursorStyle::Hbar(_) | CursorStyle::Hollow => Self::FullCell,
            CursorStyle::Bar(_) => Self::VerticalLeadingEdge(direction),
        }
    }

    fn accepts(
        self,
        ResolvedCursorRect(cursor): ResolvedCursorRect,
        GlyphCellRect(cell): GlyphCellRect,
        tolerance: f32,
    ) -> bool {
        match self {
            Self::FullCell => rect_edges_match(cursor, cell, tolerance),
            Self::VerticalLeadingEdge(direction) => {
                let has_cell_height = approx_eq(cursor.y, cell.y, tolerance)
                    && approx_eq(cursor.bottom(), cell.bottom(), tolerance);
                let lies_within_cell = cursor.x >= cell.x - tolerance
                    && cursor.right() <= cell.right() + tolerance
                    && cursor.width > 0.0
                    && cursor.width <= cell.width + tolerance;
                let touches_leading_edge = match direction {
                    CursorInlineDirection::LeftToRight => approx_eq(cursor.x, cell.x, tolerance),
                    CursorInlineDirection::RightToLeft => {
                        approx_eq(cursor.right(), cell.right(), tolerance)
                    }
                };

                has_cell_height && lies_within_cell && touches_leading_edge
            }
        }
    }
}

fn cursor_cell_alignment(
    style: CursorStyle,
    direction: CursorInlineDirection,
    cursor: ResolvedCursorRect,
    cell: GlyphCellRect,
    tolerance: f32,
) -> CursorCellAlignment {
    let expected = CursorCellContract::for_style(style, direction);
    if expected.accepts(cursor, cell, tolerance) {
        CursorCellAlignment::Aligned
    } else {
        CursorCellAlignment::Misaligned { expected }
    }
}

fn approx_eq(left: f32, right: f32, tolerance: f32) -> bool {
    (left - right).abs() <= tolerance
}

fn rect_edges_match(left: Rect, right: Rect, tolerance: f32) -> bool {
    approx_eq(left.x, right.x, tolerance)
        && approx_eq(left.y, right.y, tolerance)
        && approx_eq(left.right(), right.right(), tolerance)
        && approx_eq(left.bottom(), right.bottom(), tolerance)
}

pub(super) fn log_cursor_glyph_alignment(
    frame_id: u64,
    pass_name: &str,
    frame_glyphs: &FrameGlyphBuffer,
    chars: &[RenderedCharBounds],
) {
    let Some(cursor) = frame_glyphs.active_cursor() else {
        return;
    };
    let Some(glyph) = chars.iter().find(|bounds| bounds.slot_id == cursor.slot_id) else {
        return;
    };
    let (cx, cy, cw, ch) = cursor_glyph_slot_rect(frame_glyphs, cursor);
    let tol = 1.0_f32;
    let cursor_rect = ResolvedCursorRect(Rect::new(cx, cy, cw, ch));
    let cell_rect = GlyphCellRect(glyph.geometry.cell);
    let direction = CursorInlineDirection::from_bidi_level(
        frame_glyphs
            .slot_glyph(cursor.slot_id)
            .and_then(FrameGlyph::bidi_level),
    );
    let CursorCellAlignment::Misaligned { expected } =
        cursor_cell_alignment(cursor.style, direction, cursor_rect, cell_rect, tol)
    else {
        return;
    };

    tracing::error!(
        "cursor_glyph_mismatch frame_id={} pass={} cursor_slot=({}, {}) style={:?} \
             contract={:?} \
             cursor=({:.1},{:.1},{:.1}x{:.1}) \
             cell=({:.1},{:.1},{:.1}x{:.1}) \
             bitmap=({:.1},{:.1},{:.1}x{:.1}) label={:?} face={} font={:.1}",
        frame_id,
        pass_name,
        cursor.slot_id.row,
        cursor.slot_id.col,
        cursor.style,
        expected,
        cx,
        cy,
        cw,
        ch,
        glyph.geometry.cell.x,
        glyph.geometry.cell.y,
        glyph.geometry.cell.width,
        glyph.geometry.cell.height,
        glyph.geometry.bitmap.x,
        glyph.geometry.bitmap.y,
        glyph.geometry.bitmap.width,
        glyph.geometry.bitmap.height,
        glyph.label,
        glyph.face_id,
        glyph.font_size,
    );
}

fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

fn sample_color_stops(stops: &[ColorStop], t: f32) -> Color {
    match stops {
        [] => Color::BLACK,
        [stop] => stop.color,
        _ => {
            let t = t.clamp(0.0, 1.0);
            if t <= stops[0].position {
                return stops[0].color;
            }
            for window in stops.windows(2) {
                let start = &window[0];
                let end = &window[1];
                if t <= end.position {
                    let span = (end.position - start.position).max(f32::EPSILON);
                    return lerp_color(start.color, end.color, (t - start.position) / span);
                }
            }
            stops[stops.len() - 1].color
        }
    }
}

fn fract(v: f32) -> f32 {
    v - v.floor()
}

fn pseudo_noise_2d(x: f32, y: f32) -> f32 {
    fract((x * 12.9898 + y * 78.233).sin() * 43_758.547)
}

fn sample_gradient_color(gradient: &Gradient, bounds: &Rect, x: f32, y: f32) -> Color {
    let width = bounds.width.max(f32::EPSILON);
    let height = bounds.height.max(f32::EPSILON);
    let u = ((x - bounds.x) / width).clamp(0.0, 1.0);
    let v = ((y - bounds.y) / height).clamp(0.0, 1.0);

    match gradient {
        Gradient::Linear { angle, stops } => {
            let radians = angle.to_radians();
            let dir_x = radians.cos();
            let dir_y = radians.sin();
            let min_proj =
                if dir_x < 0.0 { dir_x } else { 0.0 } + if dir_y < 0.0 { dir_y } else { 0.0 };
            let max_proj =
                if dir_x > 0.0 { dir_x } else { 0.0 } + if dir_y > 0.0 { dir_y } else { 0.0 };
            let proj = u * dir_x + v * dir_y;
            let t = if (max_proj - min_proj).abs() <= f32::EPSILON {
                0.0
            } else {
                (proj - min_proj) / (max_proj - min_proj)
            };
            sample_color_stops(stops, t)
        }
        Gradient::Radial {
            center_x,
            center_y,
            radius,
            stops,
        } => {
            let dx = u - *center_x;
            let dy = v - *center_y;
            let dist = (dx * dx + dy * dy).sqrt() / (*radius).max(f32::EPSILON);
            sample_color_stops(stops, dist)
        }
        Gradient::Conic {
            center_x,
            center_y,
            angle_offset,
            stops,
        } => {
            let angle = (v - *center_y).atan2(u - *center_x);
            let turns = fract((angle + angle_offset.to_radians()) / std::f32::consts::TAU);
            sample_color_stops(stops, turns)
        }
        Gradient::Noise {
            scale,
            octaves,
            color1,
            color2,
        } => {
            let mut noise = 0.0;
            let mut amplitude = 1.0;
            let mut frequency = 1.0;
            let mut max_value = 0.0;
            for _ in 0..*octaves {
                noise +=
                    amplitude * pseudo_noise_2d(u * *scale * frequency, v * *scale * frequency);
                max_value += amplitude;
                amplitude *= 0.5;
                frequency *= 2.0;
            }
            let t = if max_value <= f32::EPSILON {
                0.0
            } else {
                noise / max_value
            };
            lerp_color(*color1, *color2, t)
        }
    }
}

/// Default glyph metrics carried by a frame, or None when the frame has no
/// resolved font metrics. Callers must NOT substitute invented defaults: a
/// default-metric change clears the whole glyph atlas, so letting a
/// metric-less frame (e.g. a synthetic overlay frame) overwrite a real
/// default with a guess evicts every cached glyph twice — once for the
/// guess, once when the next real frame restores the truth.
fn frame_default_glyph_metrics(frame_glyphs: &FrameGlyphBuffer) -> Option<(f32, f32)> {
    if !(frame_glyphs.font_pixel_size.is_finite() && frame_glyphs.font_pixel_size > 0.0) {
        return None;
    }
    let font_size = frame_glyphs.font_pixel_size;
    let line_height = if frame_glyphs.char_height.is_finite() && frame_glyphs.char_height > 0.0 {
        frame_glyphs.char_height
    } else {
        font_size * 1.2
    };

    Some((font_size, line_height.max(font_size)))
}

pub(super) fn subpixel_foreground_color(bg: Color, fg: Color, blend: f32) -> [f32; 4] {
    let t = blend.clamp(0.0, 1.0);
    [
        bg.r + (fg.r - bg.r) * t,
        bg.g + (fg.g - bg.g) * t,
        bg.b + (fg.b - bg.b) * t,
        1.0,
    ]
}

pub(super) fn subpixel_background_color(bg: Color) -> [f32; 4] {
    [bg.r, bg.g, bg.b, bg.a]
}

pub(super) fn build_subpixel_vertices(
    glyph_x: f32,
    glyph_y: f32,
    glyph_w: f32,
    glyph_h: f32,
    tex_u_min: f32,
    tex_u_max: f32,
    tex_v_min: f32,
    tex_v_max: f32,
    fg_color: [f32; 4],
    bg_color: [f32; 4],
) -> [SubpixelGlyphVertex; 6] {
    [
        SubpixelGlyphVertex {
            position: [glyph_x, glyph_y],
            tex_coords: [tex_u_min, tex_v_min],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x + glyph_w, glyph_y],
            tex_coords: [tex_u_max, tex_v_min],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x + glyph_w, glyph_y + glyph_h],
            tex_coords: [tex_u_max, tex_v_max],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x, glyph_y],
            tex_coords: [tex_u_min, tex_v_min],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x + glyph_w, glyph_y + glyph_h],
            tex_coords: [tex_u_max, tex_v_max],
            fg_color,
            bg_color,
        },
        SubpixelGlyphVertex {
            position: [glyph_x, glyph_y + glyph_h],
            tex_coords: [tex_u_min, tex_v_max],
            fg_color,
            bg_color,
        },
    ]
}

pub(super) fn trace_face_debug_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("NEOMACS_TRACE_FACE_COLORS").is_some())
}

fn next_face_debug_call_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

pub(super) fn color_is_grayscale(color: Color) -> bool {
    (color.r - color.g).abs() < 0.0001 && (color.g - color.b).abs() < 0.0001
}

/// Trace-log FrameGlyph entries near y=27 (the gray line area) for debugging.
fn log_frame_glyph_debug_scan(frame_glyphs: &FrameGlyphBuffer) {
    let mut logged_count = 0;
    for (i, glyph) in frame_glyphs.glyphs.iter().enumerate() {
        if logged_count > 20 {
            break;
        }
        match glyph {
            FrameGlyph::Char {
                x,
                y,
                width,
                height,
                ascent,
                face_id,
                char: ch,
                row_role,
                ..
            }
                // Log first row chars AND any char touching y=24-32
                if (*y < 1.0 || (*y < 32.0 && *y + *height > 24.0)) => {
                    let rf = frame_glyphs.resolved_face(*face_id);
                    let fg = rf.fg;
                    let font_size = rf.font_size;
                    let bg_str = format!("({:.3},{:.3},{:.3})", rf.bg.r, rf.bg.g, rf.bg.b);
                    tracing::trace!(
                        "frame_glyph[{}]: Char '{}' face={} pos=({:.1},{:.1}) size=({:.1},{:.1}) ascent={:.1} fg=({:.3},{:.3},{:.3}) bg={} font_sz={:.1} role={:?}",
                        i,
                        *ch as u8 as char,
                        face_id,
                        x,
                        y,
                        width,
                        height,
                        ascent,
                        fg.r,
                        fg.g,
                        fg.b,
                        bg_str,
                        font_size,
                        row_role
                    );
                    logged_count += 1;
                }
            FrameGlyph::Stretch {
                x,
                y,
                width,
                height,
                bg,
                row_role,
                ..
            }
                if *y < 32.0 && *y + *height > 24.0 => {
                    tracing::trace!(
                        "frame_glyph[{}]: Stretch pos=({:.1},{:.1}) size=({:.1},{:.1}) bg=({:.3},{:.3},{:.3}) role={:?}",
                        i,
                        x,
                        y,
                        width,
                        height,
                        bg.r,
                        bg.g,
                        bg.b,
                        row_role
                    );
                    logged_count += 1;
                }
            FrameGlyph::Background { bounds, color }
                if bounds.y < 32.0 && bounds.y + bounds.height > 24.0 => {
                    tracing::trace!(
                        "frame_glyph[{}]: Background pos=({:.1},{:.1}) size=({:.1},{:.1}) color=({:.3},{:.3},{:.3})",
                        i,
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        bounds.height,
                        color.r,
                        color.g,
                        color.b
                    );
                    logged_count += 1;
                }
            FrameGlyph::Border {
                x,
                y,
                width,
                height,
                color,
                ..
            }
                if *y < 32.0 && *y + *height > 24.0 => {
                    tracing::trace!(
                        "frame_glyph[{}]: Border pos=({:.1},{:.1}) size=({:.1},{:.1}) color=({:.3},{:.3},{:.3})",
                        i,
                        x,
                        y,
                        width,
                        height,
                        color.r,
                        color.g,
                        color.b
                    );
                    logged_count += 1;
                }
            _ => {}
        }
    }
}

fn log_face_debug_summary(
    call_id: u64,
    frame_glyphs: &FrameGlyphBuffer,
    faces: &HashMap<FaceId, Face>,
) {
    if !trace_face_debug_enabled() {
        return;
    }

    let mut used_face_ids = BTreeSet::new();
    for glyph in &frame_glyphs.glyphs {
        match glyph {
            FrameGlyph::Char { face_id, .. } | FrameGlyph::Stretch { face_id, .. } => {
                used_face_ids.insert(*face_id);
            }
            _ => {}
        }
    }

    tracing::info!(
        "face-debug call={} frame={}x{} used_faces={} faces_map={}",
        call_id,
        frame_glyphs.width,
        frame_glyphs.height,
        used_face_ids.len(),
        faces.len()
    );

    for face_id in used_face_ids.iter().take(48) {
        if let Some(face) = faces.get(face_id) {
            tracing::info!(
                "face-debug call={} face id={} fg=({:.3},{:.3},{:.3},{:.3}) bg=({:.3},{:.3},{:.3},{:.3}) family={:?} size={:.1} weight={} attrs={:?}",
                call_id,
                face_id,
                face.foreground.r,
                face.foreground.g,
                face.foreground.b,
                face.foreground.a,
                face.background.r,
                face.background.g,
                face.background.b,
                face.background.a,
                face.font_family,
                face.font_size,
                face.font_weight,
                face.attributes
            );
        } else {
            tracing::info!("face-debug call={} face id={} missing", call_id, face_id);
        }
    }

    let mut logged_chars = 0usize;
    for glyph in &frame_glyphs.glyphs {
        let FrameGlyph::Char {
            char,
            x,
            y,
            face_id,
            row_role,
            ..
        } = glyph
        else {
            continue;
        };

        let rf = frame_glyphs.resolved_face(*face_id);
        let fg = &rf.fg;
        let bg: Option<Color> = Some(rf.bg);
        let colorful_fg = !color_is_grayscale(*fg);
        let colorful_bg = bg.is_some_and(|color| !color_is_grayscale(color));
        if colorful_fg || colorful_bg {
            tracing::info!(
                "face-debug call={} glyph char={:?} face={} pos=({:.1},{:.1}) role={:?} fg=({:.3},{:.3},{:.3},{:.3}) bg={:?}",
                call_id,
                char,
                face_id,
                x,
                y,
                row_role,
                fg.r,
                fg.g,
                fg.b,
                fg.a,
                bg.map(|color| (color.r, color.g, color.b, color.a))
            );
            logged_chars += 1;
            if logged_chars >= 48 {
                break;
            }
        }
    }

    if logged_chars == 0 {
        tracing::info!(
            "face-debug call={} no colorful char glyphs found in frame",
            call_id
        );
    }
}

impl WgpuRenderer {
    fn presented_cursor_paint(
        &self,
        style: CursorStyle,
        resolved_background: Color,
        resolved_glyph_foreground: Color,
        effects: &EffectsConfig,
    ) -> PresentedCursorPaint {
        let resolved = ResolvedCursorPaint::new(resolved_background, resolved_glyph_foreground);
        let policy = if !style.is_hollow()
            && let Some(pulse) = self.cursor_error_pulse_override()
        {
            CursorColorPolicy::Override(pulse)
        } else if effects.cursor_color_cycle.enabled && !style.is_hollow() {
            CursorColorPolicy::Cycle {
                config: &effects.cursor_color_cycle,
                origin: self.clocks.cursor_color_cycle_start,
            }
        } else {
            CursorColorPolicy::Inherit
        };
        PresentedCursorPaint::resolve(resolved, policy, self.frame_sample_time)
    }

    fn active_cursor_inverse_video(
        &self,
        frame_glyphs: &FrameGlyphBuffer,
        cursor_visible: bool,
        animated_cursor: Option<&AnimatedCursor>,
    ) -> Option<InverseVideoCell> {
        if !cursor_visible {
            return None;
        }
        let cursor = frame_glyphs.active_cursor()?;
        if !matches!(cursor.style, CursorStyle::FilledBox) {
            return None;
        }
        let effects = frame_glyphs.effective_window_cursor_effects(cursor.window_id, &self.effects);
        let paint =
            self.presented_cursor_paint(cursor.style, cursor.color, cursor.cursor_fg, effects);
        let (x, y, width, height) = frame_glyphs.cursor_draw_rect(
            cursor.slot_id,
            cursor.style,
            cursor.ascent,
            (cursor.x, cursor.y, cursor.width, cursor.height),
        );
        FilledBoxPresentation::resolve(
            cursor.window_id,
            cursor.slot_id,
            Rect::new(x, y, width, height),
            animated_cursor,
            paint,
        )
        .inverse_video()
    }

    fn cursor_wake_factor_for(&self, effects: &EffectsConfig) -> f32 {
        if !effects.cursor_wake.enabled {
            return 1.0;
        }
        if let Some(started) = self.fx.cursor_wake.started {
            let elapsed = started.elapsed().as_millis() as f32;
            let duration = effects.cursor_wake.duration_ms as f32;
            if elapsed >= duration {
                return 1.0;
            }
            let t = elapsed / duration;
            let ease = t * (2.0 - t);
            1.0 + (effects.cursor_wake.scale - 1.0) * (1.0 - ease)
        } else {
            1.0
        }
    }

    pub(super) fn emit_cursor_visual(
        &mut self,
        window_id: DisplayWindowId,
        static_rect: (f32, f32, f32, f32),
        slot_id: DisplaySlotId,
        style: CursorStyle,
        color: &Color,
        cursor_fg: Color,
        effects: &EffectsConfig,
        cursor_visible: bool,
        animated_cursor: &Option<AnimatedCursor>,
        cursor_bg_vertices: &mut Vec<RectVertex>,
        behind_text_cursor_vertices: &mut Vec<RectVertex>,
        cursor_vertices: &mut Vec<RectVertex>,
    ) {
        let paint = self.presented_cursor_paint(style, *color, cursor_fg, effects);
        let effective_color = &paint.body_background;

        let wake = self.cursor_wake_factor_for(effects);
        let wake_active = wake != 1.0 && !style.is_hollow();

        // Compose the slide animation at draw time, never by mutating the
        // frame's stored cursor geometry. A non-filled cursor follows the
        // render-local interpolated rect; filled boxes use the typed
        // `FilledBoxPresentation` below so body and inverse-video text cannot
        // disagree about whether the cursor has reached its destination.
        let (cx, cy, cw, ch) = match animated_cursor.as_ref() {
            Some(anim) if anim.window_id == window_id && !style.is_hollow() => {
                (anim.x, anim.y, anim.width, anim.height)
            }
            _ => static_rect,
        };

        if matches!(style, CursorStyle::FilledBox) {
            if cursor_visible {
                let presentation = FilledBoxPresentation::resolve(
                    window_id,
                    slot_id,
                    Rect::new(static_rect.0, static_rect.1, static_rect.2, static_rect.3),
                    animated_cursor.as_ref(),
                    paint,
                );
                match presentation {
                    FilledBoxPresentation::Settled { rect, .. } => {
                        let (x, y, width, height) = if wake_active {
                            Self::scale_rect(rect.x, rect.y, rect.width, rect.height, wake)
                        } else {
                            (rect.x, rect.y, rect.width, rect.height)
                        };
                        self.add_rect(cursor_bg_vertices, x, y, width, height, effective_color);
                    }
                    FilledBoxPresentation::InFlight { shape, .. } => match shape {
                        CursorShape::Rect(rect) => {
                            let (x, y, width, height) = if wake_active {
                                Self::scale_rect(rect.x, rect.y, rect.width, rect.height, wake)
                            } else {
                                (rect.x, rect.y, rect.width, rect.height)
                            };
                            self.add_rect(
                                behind_text_cursor_vertices,
                                x,
                                y,
                                width,
                                height,
                                effective_color,
                            );
                        }
                        CursorShape::Quad(corners) => {
                            self.add_quad(behind_text_cursor_vertices, &corners, effective_color)
                        }
                    },
                }
            }
            return;
        }

        let use_corners = animated_cursor.as_ref().is_some_and(|anim| {
            anim.window_id == window_id && !style.is_hollow() && anim.corners.is_some()
        });

        if use_corners {
            if let Some(anim) = animated_cursor.as_ref()
                && let Some(corners) = anim.corners.as_ref()
                && cursor_visible
            {
                self.add_quad(cursor_vertices, corners, effective_color);
            }
            return;
        }

        let should_draw = style.is_hollow() || cursor_visible;
        if !should_draw {
            return;
        }

        match style {
            CursorStyle::Bar(bar_w) => {
                if wake_active {
                    let (sx, sy, sw, sh) = Self::scale_rect(cx, cy, bar_w, ch, wake);
                    self.add_rect(cursor_vertices, sx, sy, sw, sh, effective_color);
                } else {
                    self.add_rect(cursor_vertices, cx, cy, bar_w, ch, effective_color);
                }
            }
            CursorStyle::Hbar(hbar_h) => {
                if wake_active {
                    let (sx, sy, sw, sh) = Self::scale_rect(cx, cy + ch - hbar_h, cw, hbar_h, wake);
                    self.add_rect(cursor_vertices, sx, sy, sw, sh, effective_color);
                } else {
                    self.add_rect(
                        cursor_vertices,
                        cx,
                        cy + ch - hbar_h,
                        cw,
                        hbar_h,
                        effective_color,
                    );
                }
            }
            CursorStyle::Hollow => {
                self.add_rect(cursor_vertices, cx, cy, cw, 1.0, effective_color);
                self.add_rect(cursor_vertices, cx, cy + ch - 1.0, cw, 1.0, effective_color);
                self.add_rect(cursor_vertices, cx, cy, 1.0, ch, effective_color);
                self.add_rect(cursor_vertices, cx + cw - 1.0, cy, 1.0, ch, effective_color);
            }
            CursorStyle::FilledBox => {
                self.add_rect(cursor_vertices, cx, cy, cw, ch, effective_color);
            }
        }
    }

    fn gradient_bounds_for_rect(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        clip_rect: Option<&Rect>,
    ) -> Rect {
        clip_rect
            .copied()
            .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
            .unwrap_or_else(|| Rect::new(x, y, width, height))
    }

    pub(super) fn sample_face_background(
        face: Option<&Face>,
        fallback: Option<Color>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        clip_rect: Option<&Rect>,
    ) -> Option<Color> {
        if let Some(face) = face
            && let Some(gradient) = face.background_gradient.as_deref()
        {
            let bounds = Self::gradient_bounds_for_rect(x, y, width, height, clip_rect);
            return Some(sample_gradient_color(
                gradient,
                &bounds,
                x + width * 0.5,
                y + height * 0.5,
            ));
        }
        fallback.or_else(|| face.map(|resolved| resolved.background))
    }

    pub(super) fn sample_face_paint_background(
        face: Option<&Face>,
        fallback: Option<Color>,
        paint: super::pointer_override::FacePaint,
    ) -> Option<Color> {
        let domain = paint.domain();
        Self::sample_face_background(
            face,
            fallback,
            domain.x,
            domain.y,
            domain.width,
            domain.height,
            None,
        )
    }

    fn add_gradient_quad(
        vertices: &mut Vec<RectVertex>,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        c00: Color,
        c10: Color,
        c11: Color,
        c01: Color,
    ) {
        vertices.push(RectVertex {
            position: [x0, y0],
            color: [c00.r, c00.g, c00.b, c00.a],
        });
        vertices.push(RectVertex {
            position: [x1, y0],
            color: [c10.r, c10.g, c10.b, c10.a],
        });
        vertices.push(RectVertex {
            position: [x0, y1],
            color: [c01.r, c01.g, c01.b, c01.a],
        });
        vertices.push(RectVertex {
            position: [x1, y0],
            color: [c10.r, c10.g, c10.b, c10.a],
        });
        vertices.push(RectVertex {
            position: [x1, y1],
            color: [c11.r, c11.g, c11.b, c11.a],
        });
        vertices.push(RectVertex {
            position: [x0, y1],
            color: [c01.r, c01.g, c01.b, c01.a],
        });
    }

    pub(super) fn add_face_background_rect(
        &self,
        vertices: &mut Vec<RectVertex>,
        face: Option<&Face>,
        fallback: &Color,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        clip_rect: Option<&Rect>,
    ) {
        let Some(face) = face else {
            self.add_rect(vertices, x, y, width, height, fallback);
            return;
        };
        let Some(gradient) = face.background_gradient.as_deref() else {
            self.add_rect(vertices, x, y, width, height, fallback);
            return;
        };

        let bounds = Self::gradient_bounds_for_rect(x, y, width, height, clip_rect);
        let (segments_x, segments_y) = match gradient {
            Gradient::Linear { .. } => (1, 1),
            Gradient::Radial { .. } => (8, 8),
            Gradient::Conic { .. } => (12, 12),
            Gradient::Noise { .. } => (12, 12),
        };
        let step_x = width / segments_x as f32;
        let step_y = height / segments_y as f32;

        for iy in 0..segments_y {
            for ix in 0..segments_x {
                let x0 = x + step_x * ix as f32;
                let y0 = y + step_y * iy as f32;
                let x1 = if ix + 1 == segments_x {
                    x + width
                } else {
                    x0 + step_x
                };
                let y1 = if iy + 1 == segments_y {
                    y + height
                } else {
                    y0 + step_y
                };
                let c00 = sample_gradient_color(gradient, &bounds, x0, y0);
                let c10 = sample_gradient_color(gradient, &bounds, x1, y0);
                let c11 = sample_gradient_color(gradient, &bounds, x1, y1);
                let c01 = sample_gradient_color(gradient, &bounds, x0, y1);
                Self::add_gradient_quad(vertices, x0, y0, x1, y1, c00, c10, c11, c01);
            }
        }
    }

    pub(super) fn add_face_paint_background(
        &self,
        vertices: &mut Vec<RectVertex>,
        face: Option<&Face>,
        fallback: &Color,
        paint: super::pointer_override::FacePaint,
        offset_x: f32,
        offset_y: f32,
    ) {
        let domain = paint.domain();
        let start = vertices.len();
        self.add_face_background_rect(
            vertices,
            face,
            fallback,
            domain.x + offset_x,
            domain.y + offset_y,
            domain.width,
            domain.height,
            None,
        );
        let clip = paint.clip().map(|clip| Rect {
            x: clip.x + offset_x,
            y: clip.y + offset_y,
            ..clip
        });
        super::pointer_override::clip_new_rect_vertices(vertices, start, clip.as_ref());
    }

    pub(super) fn add_stipple_paint(
        &self,
        vertices: &mut Vec<RectVertex>,
        fg: &Color,
        pattern: &neomacs_display_protocol::StipplePattern,
        paint: super::pointer_override::FacePaint,
        offset_x: f32,
        offset_y: f32,
    ) {
        let domain = paint.domain();
        let start = vertices.len();
        self.render_stipple_pattern(
            vertices,
            domain.x + offset_x,
            domain.y + offset_y,
            domain.width,
            domain.height,
            fg,
            pattern,
        );
        let clip = paint.clip().map(|clip| Rect {
            x: clip.x + offset_x,
            y: clip.y + offset_y,
            ..clip
        });
        super::pointer_override::clip_new_rect_vertices(vertices, start, clip.as_ref());
    }

    /// Render frame glyphs to a texture view
    ///
    /// `surface_width` and `surface_height` should be the actual surface dimensions
    /// for correct coordinate transformation.
    #[allow(clippy::too_many_arguments)]
    // The `background_gradient` parameter is an RGB-pair tuple; a type alias
    // would not materially improve this signature.
    #[allow(clippy::type_complexity)]
    #[allow(clippy::too_many_arguments)]
    pub fn render_frame_glyphs(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        mapping: neomacs_display_protocol::PresentMapping,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        mouse_pos: (f32, f32),
        background_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
        pointer_selection: Option<PointerAppearanceSelection>,
        row_damage: Option<&super::row_reuse::FrameRowDamage>,
    ) {
        self.render_frame_glyphs_impl(
            view,
            frame_glyphs,
            glyph_atlas,
            mapping,
            cursor_visible,
            animated_cursor,
            mouse_pos,
            background_gradient,
            pointer_selection,
            row_damage,
            false,
            None,
        );
    }

    /// Render a frame into `view` preserving existing content (`LoadOp::Load`)
    /// and clipped to `scissor` (physical x, y, w, h). Used by the
    /// retained-static fast path to redraw only the filled-box cursor cell
    /// (box plus the inverse-video character) over the composited scene, from a
    /// single-glyph mini-frame so no full-frame glyph work is done.
    #[allow(clippy::too_many_arguments)]
    pub fn render_frame_cell_loaded(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        mapping: neomacs_display_protocol::PresentMapping,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        mouse_pos: (f32, f32),
        scissor: (u32, u32, u32, u32),
    ) {
        self.render_frame_glyphs_impl(
            view,
            frame_glyphs,
            glyph_atlas,
            mapping,
            cursor_visible,
            animated_cursor,
            mouse_pos,
            None,
            None,
            None,
            true,
            Some(scissor),
        );
    }

    // The parameter mirrors `FrameParams` and the display-runtime effects
    // boundary; a local alias would only hide this one site.
    #[allow(clippy::type_complexity)]
    #[allow(clippy::too_many_arguments)]
    fn render_frame_glyphs_impl(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        mapping: neomacs_display_protocol::PresentMapping,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        mouse_pos: (f32, f32),
        background_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
        pointer_selection: Option<PointerAppearanceSelection>,
        row_damage: Option<&super::row_reuse::FrameRowDamage>,
        load_existing: bool,
        scissor: Option<(u32, u32, u32, u32)>,
    ) {
        debug_assert_eq!(mapping.presentation(), frame_glyphs.presentation_id);
        debug_assert_eq!(mapping.content_logical_size().width(), frame_glyphs.width);
        debug_assert_eq!(mapping.content_logical_size().height(), frame_glyphs.height);
        let surface = mapping.surface();
        let surface_width = surface.device_width().get();
        let surface_height = surface.device_height().get();
        let scissor = match scissor {
            Some(scissor) => {
                let Some(scissor) =
                    SurfaceScissor::intersect(scissor, surface_width, surface_height)
                else {
                    // A retained frame may briefly describe cells outside a
                    // newly resized surface. An empty clip has no pixels to
                    // update and must not become an invalid WGPU scissor.
                    return;
                };
                Some(scissor)
            }
            None => None,
        };

        self.arenas.begin_frame();

        let face_debug_call_id = if trace_face_debug_enabled() {
            next_face_debug_call_id()
        } else {
            0
        };
        let faces = &frame_glyphs.faces;

        tracing::trace!(
            "render_frame_glyphs: frame={}x{} surface={}x{}, {} glyphs, {} faces",
            frame_glyphs.width,
            frame_glyphs.height,
            surface_width,
            surface_height,
            frame_glyphs.glyphs.len(),
            faces.len(),
        );

        log_face_debug_summary(face_debug_call_id, frame_glyphs, faces);

        if let Some((default_font_size, default_line_height)) =
            frame_default_glyph_metrics(frame_glyphs)
        {
            glyph_atlas.set_metrics(default_font_size, default_line_height);
        }

        self.refresh_frame_animation_state(frame_glyphs);
        if trace_face_debug_enabled() {
            tracing::info!(
                "face-debug call={} milestone=after_refresh",
                face_debug_call_id
            );
        }

        // Advance glyph atlas generation for LRU tracking
        glyph_atlas.advance_generation();
        if trace_face_debug_enabled() {
            tracing::info!(
                "face-debug call={} milestone=after_advance_generation",
                face_debug_call_id
            );
        }

        let (logical_w, logical_h) = self.prepare_frame_uniforms(mapping);
        if trace_face_debug_enabled() {
            tracing::info!(
                "face-debug call={} milestone=after_prepare_uniforms logical=({:.1},{:.1})",
                face_debug_call_id,
                logical_w,
                logical_h
            );
        }

        // Rendering order for correct z-layering (inverse video cursor):
        //   1. Non-overlay backgrounds (window bg, stretches, char bg)
        //   2. Cursor bg rect (inverse video background for filled box cursor)
        //   3. Animated cursor trail (behind text, for filled box cursor motion)
        //   4. Non-overlay text (with cursor_fg swap for char at cursor position)
        //   5. Overlay backgrounds (mode-line/echo bg)
        //   6. Overlay text (mode-line/echo text)
        //   7. Inline media (images, videos, xwidgets)
        //   8. Front cursors (bar, hbar, hollow) and borders
        //
        // Filled box cursor (style 0) is split across steps 2-4 for inverse video.
        // Bar/hbar/hollow cursors are drawn on top of text in step 8.

        log_frame_glyph_debug_scan(frame_glyphs);

        let params = FrameParams {
            frame_glyphs,
            pointer_override: super::pointer_override::PointerOverrideResolver::new(
                frame_glyphs,
                pointer_selection,
            ),
            faces,
            cursor_visible,
            animated_cursor: &animated_cursor,
            cursor_inverse_video: self.active_cursor_inverse_video(
                frame_glyphs,
                cursor_visible,
                animated_cursor.as_ref(),
            ),
            mouse_pos,
            background_gradient,
            logical_w,
            logical_h,
            device_scale: surface.device_scale(),
            face_debug_call_id,
            has_line_anims: !self.fx.line_anim.active.is_empty()
                || !self.fx.scroll_spacing.active.is_empty(),
            row_damage,
        };

        let box_spans = self.collect_box_spans(&params);
        let non_overlay_rect_vertices = self.collect_non_overlay_backgrounds(&params, &box_spans);
        let overlay_rect_vertices = self.collect_overlay_backgrounds(&params, &box_spans);
        let chrome = self.collect_chrome_layers(&params);

        let mut stats = GlyphRenderStats::new();
        stats.total_frame_glyphs = frame_glyphs.glyphs.len();
        for glyph in &frame_glyphs.glyphs {
            if let FrameGlyph::Char { composed, .. } = glyph {
                stats.text_glyphs += 1;
                if composed.is_some() {
                    stats.composed_glyphs += 1;
                }
            }
        }
        let mut seen_single_keys: HashSet<GlyphKey> = HashSet::new();
        let mut seen_composed_keys: HashSet<ComposedGlyphKey> = HashSet::new();
        #[cfg(feature = "video")]
        let submitted_video_ids;
        #[cfg(feature = "video")]
        let gpu_timing = self.begin_video_frame_content_pass(
            frame_glyphs
                .glyphs
                .iter()
                .any(|glyph| matches!(glyph, FrameGlyph::Video { .. })),
        );
        #[cfg(feature = "video")]
        let timestamp_writes = gpu_timing.as_ref().map(|timing| timing.timestamp_writes());
        #[cfg(not(feature = "video"))]
        let timestamp_writes = None;

        // Create command encoder
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Frame Glyphs Encoder"),
            });

        // Render pass - Clear with frame background color since we rebuild
        // the entire frame from current_matrix each time (no incremental updates).
        let bg = &frame_glyphs.background;
        let load = if load_existing {
            // Preserve already-composited content (the retained static scene)
            // and only overwrite within the scissor rect below.
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color {
                // Pre-multiply RGB by alpha for correct compositing
                r: (bg.r * bg.a) as f64,
                g: (bg.g * bg.a) as f64,
                b: (bg.b * bg.a) as f64,
                a: bg.a as f64,
            })
        };
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frame Glyphs Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(scissor) = scissor {
                // Clip every draw in this pass to the cell; combined with
                // LoadOp::Load, only the cursor cell is overwritten.
                scissor.apply(&mut render_pass);
            }
            let mut ctx = FramePassCtx {
                pass: render_pass,
                params: &params,
            };

            // === Step 1: non-overlay backgrounds ===
            self.draw_non_overlay_backgrounds(&mut ctx.pass, &non_overlay_rect_vertices);

            // Build shared effect context for all effect functions.
            // Clone effect config into a local so we can mutably borrow `self`
            // while effect functions still read configuration.
            let effects_for_ctx = frame_glyphs
                .effective_phys_cursor_effects(&self.effects)
                .clone();
            let ectx = super::effect_common::EffectCtx {
                effects: &effects_for_ctx,
                frame_glyphs,
                animated_cursor: &animated_cursor,
                cursor_visible,
                mouse_pos,
                surface_width,
                surface_height,
                aurora_start: self.ambient.aurora_start,
                scale_factor: self.scale_factor,
                logical_w,
                logical_h,
                renderer_width: self.width as f32,
                renderer_height: self.height as f32,
            };

            self.draw_pre_content_background_effects(&mut ctx.pass, &ectx, faces, &box_spans.spans);

            self.draw_pre_content_effects(&mut ctx.pass, &ectx);

            // === Steps 2-3: cursor bg rect and behind-text cursor trail ===
            self.draw_pre_text_cursor_layers(
                &mut ctx.pass,
                &chrome.cursor_bg,
                &chrome.behind_text_cursor,
            );

            // === Steps 4-6: buffer text, overlay backgrounds, overlay text ===
            self.draw_text_and_overlay_passes(
                &mut ctx,
                &box_spans,
                &overlay_rect_vertices,
                glyph_atlas,
                &mut stats,
                &mut seen_single_keys,
                &mut seen_composed_keys,
            );

            // === Step 7: inline media ===
            self.draw_inline_images(&mut ctx);
            #[cfg(feature = "video")]
            {
                submitted_video_ids = self.draw_inline_videos(&mut ctx);
            }
            #[cfg(all(feature = "webview", target_os = "linux"))]
            self.draw_inline_webkit_views(&mut ctx);
            self.draw_inline_surfaces(&mut ctx);

            // === Step 8: front cursors, borders, scroll bar tracks + thumbs ===
            self.draw_cursor_layer(&mut ctx, &chrome);

            self.draw_post_content_effects(&mut ctx.pass, &ectx, faces);
        }

        stats.unique_single_glyph_keys = seen_single_keys.len();
        stats.unique_composed_glyph_keys = seen_composed_keys.len();
        stats.cache_hits = glyph_atlas.cache_hits_this_frame;
        stats.cache_misses = glyph_atlas.cache_misses_this_frame;
        stats.glyph_texture_uploads = glyph_atlas.cache_misses_this_frame;
        stats.buffers_created = self.arenas.buffers_created_since_snapshot() as usize;
        self.glyph_stats = stats.clone();
        stats.log_if_enabled();

        #[cfg(feature = "video")]
        self.submit_video_frame_content_pass(encoder, gpu_timing, submitted_video_ids);
        #[cfg(not(feature = "video"))]
        self.queue.submit([encoder.finish()]);
    }

    /// Draw only the cursor layer onto `view`, preserving existing content
    /// (`LoadOp::Load`). Used by the retained-scene composite path: the static
    /// scene is blitted first, then the cursor is drawn on top. Correct only
    /// for clean top-layer cursor styles (bar/hbar/hollow); the filled-box
    /// inverse-video cursor is not separable this way and must go through the
    /// full render (see `CursorStyle::is_clean_top_layer`).
    #[allow(clippy::too_many_arguments)]
    pub fn render_cursor_only(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        mapping: neomacs_display_protocol::PresentMapping,
        cursor_visible: bool,
        animated_cursor: Option<AnimatedCursor>,
        mouse_pos: (f32, f32),
    ) {
        debug_assert_eq!(mapping.presentation(), frame_glyphs.presentation_id);
        debug_assert_eq!(mapping.content_logical_size().width(), frame_glyphs.width);
        debug_assert_eq!(mapping.content_logical_size().height(), frame_glyphs.height);
        let (logical_w, logical_h) = self.prepare_frame_uniforms(mapping);
        let params = FrameParams {
            frame_glyphs,
            pointer_override: super::pointer_override::PointerOverrideResolver::new(
                frame_glyphs,
                None,
            ),
            faces: &frame_glyphs.faces,
            cursor_visible,
            animated_cursor: &animated_cursor,
            cursor_inverse_video: None,
            mouse_pos,
            background_gradient: None,
            logical_w,
            logical_h,
            device_scale: mapping.surface().device_scale(),
            face_debug_call_id: 0,
            has_line_anims: false,
            row_damage: None,
        };
        let chrome = self.collect_chrome_layers(&params);
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Cursor-Only Encoder"),
            });
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Cursor-Only Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            // Filled-box inverse-video parts (cursor_bg, behind-text trail) are
            // empty for clean cursors; drawing them is a harmless no-op there.
            self.draw_pre_text_cursor_layers(
                &mut render_pass,
                &chrome.cursor_bg,
                &chrome.behind_text_cursor,
            );
            if let Some(upload) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &chrome.cursors)
            {
                render_pass.set_pipeline(&self.pipelines.rect);
                render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                render_pass.set_vertex_buffer(0, upload.buffer_slice());
                render_pass.draw(0..chrome.cursors.len() as u32, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
    }

    fn refresh_frame_animation_state(&mut self, frame_glyphs: &FrameGlyphBuffer) {
        // Reset animated borders flag (set during box rendering if any fancy style is used).
        self.fx.has_animated_borders = false;

        self.refresh_line_animation_state();
        self.refresh_mode_line_transition_state(frame_glyphs);
        self.refresh_text_fade_state();
        self.refresh_scroll_spacing_state();
        self.refresh_cursor_wake_state();
        self.refresh_cursor_error_pulse_state();
        self.refresh_scroll_momentum_state();
    }

    fn refresh_line_animation_state(&mut self) {
        self.fx
            .line_anim
            .active
            .retain(|a| a.started.elapsed() < a.duration);
    }

    fn refresh_mode_line_transition_state(&mut self, frame_glyphs: &FrameGlyphBuffer) {
        self.fx
            .mode_line_fade
            .active
            .retain(|e| e.started.elapsed() < e.duration);

        if !self.effects.mode_line_transition.enabled {
            return;
        }

        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let now_ml = std::time::Instant::now();
        for info in &frame_glyphs.window_infos {
            if info.mode_line_height < 1.0 || info.is_minibuffer {
                continue;
            }
            let ml_y = info.bounds.y + info.bounds.height - info.mode_line_height;
            // Hash overlay chars within mode-line area.
            let mut hasher = DefaultHasher::new();
            for g in &frame_glyphs.glyphs {
                if let FrameGlyph::Char {
                    x,
                    y,
                    char: ch,
                    row_role,
                    ..
                } = g
                {
                    if !row_role.is_chrome() {
                        continue;
                    }
                    if *x >= info.bounds.x
                        && *x < info.bounds.x + info.bounds.width
                        && *y >= ml_y
                        && *y < ml_y + info.mode_line_height
                    {
                        ch.hash(&mut hasher);
                    }
                }
            }
            let hash = hasher.finish();
            let prev = self
                .fx
                .mode_line_fade
                .prev_hashes
                .insert(info.window_id.get(), hash);
            if let Some(prev_hash) = prev
                && prev_hash != hash
            {
                self.fx
                    .mode_line_fade
                    .active
                    .retain(|e| e.window_id != info.window_id.get());
                self.fx.mode_line_fade.active.push(ModeLineFadeEntry {
                    window_id: info.window_id.get(),
                    mode_line_y: ml_y,
                    mode_line_h: info.mode_line_height,
                    bounds_x: info.bounds.x,
                    bounds_w: info.bounds.width,
                    started: now_ml,
                    duration: std::time::Duration::from_millis(
                        self.effects.mode_line_transition.duration_ms as u64,
                    ),
                });
            }
        }
    }

    fn refresh_text_fade_state(&mut self) {
        self.fx
            .text_fade
            .active
            .retain(|e| e.started.elapsed() < e.duration);
    }

    fn refresh_scroll_spacing_state(&mut self) {
        let now_spacing = std::time::Instant::now();
        self.fx
            .scroll_spacing
            .active
            .retain(|e| now_spacing.duration_since(e.started) < e.duration);
    }

    fn refresh_cursor_wake_state(&mut self) {
        if let Some(started) = self.fx.cursor_wake.started {
            let dur = std::time::Duration::from_millis(self.effects.cursor_wake.duration_ms as u64);
            if started.elapsed() >= dur {
                self.fx.cursor_wake.started = None;
            }
        }
    }

    fn refresh_cursor_error_pulse_state(&mut self) {
        if let Some(started) = self.fx.error_pulse.started {
            let dur = std::time::Duration::from_millis(
                self.effects.cursor_error_pulse.duration_ms as u64,
            );
            if started.elapsed() >= dur {
                self.fx.error_pulse.started = None;
            }
        }
    }

    fn refresh_scroll_momentum_state(&mut self) {
        self.fx
            .scroll_momentum
            .active
            .retain(|e| e.started.elapsed() < e.duration);
    }

    fn prepare_frame_uniforms(
        &mut self,
        mapping: neomacs_display_protocol::PresentMapping,
    ) -> (f32, f32) {
        // The swapchain is the destination and the immutable glyph presentation
        // is the source. Their sizes deliberately advance on different clocks.
        // Projection therefore follows the resolved mapping's live-surface
        // extent; using source bounds here non-uniformly stretches stale text.
        let logical_size = mapping.surface_logical_size();
        let logical_w = logical_size.width();
        let logical_h = logical_size.height();
        let elapsed = self.ambient.render_start_time.elapsed().as_secs_f32();
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: elapsed,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));
        (logical_w, logical_h)
    }

    fn face_has_rounded_box(faces: &HashMap<FaceId, Face>, face_id: FaceId) -> bool {
        faces
            .get(&face_id)
            .map(|f| f.box_corner_radius > 0)
            .unwrap_or(false)
    }

    /// Whether this exact face paint is replaced by a rendered rounded fill.
    /// Matching face, clip, row and primitive coverage keeps transient paint
    /// complements independent even though their geometry is adjacent.
    pub(super) fn paint_has_rounded_box_span(
        gx: f32,
        gy: f32,
        width: f32,
        height: f32,
        face_id: FaceId,
        clip: Option<&Rect>,
        row_role: GlyphRowRole,
        box_spans: &[BoxSpan],
        faces: &HashMap<FaceId, Face>,
    ) -> bool {
        let right = gx + width;
        box_spans.iter().any(|s| {
            s.face_id == face_id
                && s.clip.as_ref() == clip
                && s.row_role == row_role
                && Self::face_has_rounded_box(faces, s.face_id)
                && (s.y - gy).abs() < 0.5
                && (s.height - height).abs() < 0.5
                && gx >= s.x - 0.5
                && right <= s.x + s.width + 0.5
        })
    }

    pub(super) fn draw_rect_vertex_layer(
        &mut self,
        render_pass: &mut wgpu::RenderPass<'_>,
        rect_vertices: &[RectVertex],
    ) {
        let Some(upload) = self
            .arenas
            .rect
            .upload(&self.device, &self.queue, rect_vertices)
        else {
            return;
        };
        render_pass.set_pipeline(&self.pipelines.rect);
        render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        render_pass.set_vertex_buffer(0, upload.buffer_slice());
        render_pass.draw(0..rect_vertices.len() as u32, 0..1);
    }
}

#[cfg(test)]
#[path = "glyphs_test.rs"]
mod tests;

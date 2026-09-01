//! Immutable lookup of transient paint overrides for one presented frame.

use super::super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, SubpixelGlyphVertex};
use neomacs_display_protocol::Color;
use neomacs_display_protocol::types::Rect;
use neomacs_display_protocol::{
    FaceId, FrameGlyphBuffer, FrameRect, PointerAppearancePhase, PointerAppearanceSelection,
    PointerDrawMode, PointerImageRelief, PresentedPrimitiveKind,
};
#[cfg(test)]
use neomacs_display_protocol::{FrameGlyph, MaterializedFaceData};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PrimitivePointerOverride {
    mode: PointerDrawMode,
    clip: FrameRect,
}

impl PrimitivePointerOverride {
    pub(super) const fn mode(self) -> PointerDrawMode {
        self.mode
    }

    pub(super) const fn clip(self) -> FrameRect {
        self.clip
    }
}

#[cfg(test)]
pub(super) struct ResolvedGlyphPaint<'a> {
    primitive: &'a FrameGlyph,
    face_id: Option<FaceId>,
    materialized_face: MaterializedFaceData,
    clip: FrameRect,
}

#[cfg(test)]
impl<'a> ResolvedGlyphPaint<'a> {
    pub(super) const fn primitive(&self) -> &'a FrameGlyph {
        self.primitive
    }

    pub(super) const fn face_id(&self) -> Option<FaceId> {
        self.face_id
    }

    pub(super) const fn materialized_face(&self) -> MaterializedFaceData {
        self.materialized_face
    }

    pub(super) const fn clip(&self) -> FrameRect {
        self.clip
    }
}

/// Deep, immutable resolver shared by every renderer layer.
///
/// It addresses the already-presented primitive table; it cannot modify a
/// glyph, its source slot, or any geometry used by layout.
pub(super) struct PointerOverrideResolver {
    overrides: Vec<Option<(PresentedPrimitiveKind, PrimitivePointerOverride)>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FacePaint {
    face_id: FaceId,
    domain: Rect,
    output_clip: Option<Rect>,
}

impl FacePaint {
    #[cfg(test)]
    pub(super) const fn new(face_id: FaceId, domain: Rect, output_clip: Option<Rect>) -> Self {
        Self {
            face_id,
            domain,
            output_clip,
        }
    }

    pub(super) const fn face_id(self) -> FaceId {
        self.face_id
    }

    pub(super) const fn clip(self) -> Option<Rect> {
        self.output_clip
    }

    pub(super) const fn domain(self) -> Rect {
        self.domain
    }
}

/// Allocation-free replacement plan for one primitive. A rectangle minus one
/// rectangle has at most four complement pieces plus the alternate paint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PrimitivePaintPlan {
    paints: [Option<FacePaint>; 5],
}

impl PrimitivePaintPlan {
    fn one(paint: FacePaint) -> Self {
        let mut paints = [None; 5];
        paints[0] = Some(paint);
        Self { paints }
    }

    fn push(&mut self, paint: FacePaint) {
        let slot = self.paints.iter_mut().find(|slot| slot.is_none());
        *slot.expect("rectangle complement plan capacity") = Some(paint);
    }
}

impl IntoIterator for PrimitivePaintPlan {
    type Item = FacePaint;
    type IntoIter = std::iter::Flatten<std::array::IntoIter<Option<FacePaint>, 5>>;

    fn into_iter(self) -> Self::IntoIter {
        self.paints.into_iter().flatten()
    }
}

impl PointerOverrideResolver {
    pub(super) fn new(
        frame: &FrameGlyphBuffer,
        selection: Option<PointerAppearanceSelection>,
    ) -> Self {
        let mut overrides = Vec::new();
        if let Some(selection) = selection
            && let Some(appearance) = frame.presented_pointer().appearance(selection.appearance())
        {
            overrides.resize(frame.glyphs.len(), None);
            for span in appearance.paint_spans() {
                let mode = match selection.phase() {
                    PointerAppearancePhase::Hover => {
                        span.hover().unwrap_or_else(|| appearance.hover())
                    }
                    PointerAppearancePhase::Pressed => {
                        span.pressed().unwrap_or_else(|| appearance.pressed())
                    }
                };
                let Ok(first) = usize::try_from(span.first()) else {
                    continue;
                };
                let Ok(len) = usize::try_from(span.len()) else {
                    continue;
                };
                let Some(end) = first.checked_add(len) else {
                    continue;
                };
                for slot in overrides.get_mut(first..end).into_iter().flatten() {
                    *slot = Some((
                        span.kind(),
                        PrimitivePointerOverride {
                            mode,
                            clip: span.clip(),
                        },
                    ));
                }
            }
        }
        Self { overrides }
    }

    #[cfg(test)]
    pub(super) fn glyph_override(&self, index: usize) -> Option<PrimitivePointerOverride> {
        self.primitive_override(PresentedPrimitiveKind::Glyph, index)
    }

    pub(super) fn image_override(&self, index: usize) -> Option<PrimitivePointerOverride> {
        self.primitive_override(PresentedPrimitiveKind::Image, index)
    }

    /// Face installed by the current transient primitive paint, if any. This
    /// is intentionally narrower than `face_paints_for`: box topology needs
    /// run identity, not the clip decomposition of its paint rectangles.
    pub(super) fn transient_face(
        &self,
        kind: PresentedPrimitiveKind,
        index: usize,
    ) -> Option<FaceId> {
        match self.primitive_override(kind, index)?.mode() {
            PointerDrawMode::Face(face) => Some(face),
            PointerDrawMode::ImageRelief(_) => None,
        }
    }

    #[cfg(test)]
    pub(super) fn face_id(&self, index: usize, base: FaceId) -> FaceId {
        match self
            .glyph_override(index)
            .map(PrimitivePointerOverride::mode)
        {
            Some(PointerDrawMode::Face(face)) => face,
            _ => base,
        }
    }

    /// Replacement paint plan for a face-backed primitive. Base paint covers
    /// only the complement of the transient override; the alternate face is
    /// last and covers exactly the effective override clip.
    pub(super) fn face_paints(
        &self,
        index: usize,
        base_face: FaceId,
        primitive_bounds: Rect,
        original_clip: Option<&Rect>,
    ) -> PrimitivePaintPlan {
        self.face_paints_for(
            PresentedPrimitiveKind::Glyph,
            index,
            base_face,
            primitive_bounds,
            original_clip,
        )
    }

    pub(super) fn face_paints_for(
        &self,
        kind: PresentedPrimitiveKind,
        index: usize,
        base_face: FaceId,
        primitive_bounds: Rect,
        original_clip: Option<&Rect>,
    ) -> PrimitivePaintPlan {
        let base = FacePaint {
            face_id: base_face,
            domain: primitive_bounds,
            output_clip: original_clip.copied(),
        };
        let Some(override_paint) = self.primitive_override(kind, index) else {
            return PrimitivePaintPlan::one(base);
        };
        let PointerDrawMode::Face(override_face) = override_paint.mode() else {
            return PrimitivePaintPlan::one(base);
        };
        let domain = match original_clip {
            Some(clip) => {
                let Some(domain) = intersect_rect(&primitive_bounds, clip) else {
                    return PrimitivePaintPlan::one(base);
                };
                domain
            }
            None => primitive_bounds,
        };
        let raw = override_paint.clip();
        let raw = Rect::new(raw.x(), raw.y(), raw.width(), raw.height());
        let semantic_clip = match original_clip {
            Some(clip) => {
                let Some(semantic_clip) = intersect_rect(clip, &raw) else {
                    return PrimitivePaintPlan::one(base);
                };
                semantic_clip
            }
            None => raw,
        };
        let Some(cut) = intersect_rect(&domain, &semantic_clip) else {
            return PrimitivePaintPlan::one(base);
        };
        let mut paints = PrimitivePaintPlan { paints: [None; 5] };
        for clip in rect_complement(&domain, &cut).into_iter().flatten() {
            paints.push(FacePaint {
                face_id: base_face,
                domain: primitive_bounds,
                output_clip: Some(clip),
            });
        }
        paints.push(FacePaint {
            face_id: override_face,
            domain: primitive_bounds,
            output_clip: Some(semantic_clip),
        });
        paints
    }

    #[cfg(test)]
    pub(super) fn resolve_glyph<'a>(
        &self,
        frame: &'a FrameGlyphBuffer,
        index: usize,
    ) -> Option<ResolvedGlyphPaint<'a>> {
        let primitive = frame.glyphs.get(index)?;
        let base_face = primitive.face_id()?;
        let face_id = self.face_id(index, base_face);
        let clip = self
            .glyph_clip(index, primitive.clip_rect().as_ref())
            .and_then(|clip| FrameRect::new(clip.x, clip.y, clip.width, clip.height).ok())
            .or_else(|| {
                primitive
                    .cell_rect()
                    .and_then(|(x, y, width, height)| FrameRect::new(x, y, width, height).ok())
            })?;
        Some(ResolvedGlyphPaint {
            primitive,
            face_id: Some(face_id),
            materialized_face: frame.resolved_face(face_id),
            clip,
        })
    }

    pub(super) fn affects_glyph_range(
        &self,
        glyphs: &[neomacs_display_protocol::FrameGlyph],
        range: std::ops::Range<usize>,
    ) -> bool {
        let Some(entries) = self.overrides.get(range.clone()) else {
            return false;
        };
        let Some(glyphs) = glyphs.get(range) else {
            return false;
        };
        entries.iter().zip(glyphs).any(|(entry, glyph)| {
            let Some((PresentedPrimitiveKind::Glyph, paint)) = entry else {
                return false;
            };
            let Some((x, y, width, height)) = glyph.cell_rect() else {
                return false;
            };
            let bounds = Rect::new(x, y, width, height);
            let clip = paint.clip();
            let clip = Rect::new(clip.x(), clip.y(), clip.width(), clip.height());
            intersect_rect(&bounds, &clip).is_some()
        })
    }

    #[cfg(test)]
    pub(super) fn glyph_clip(&self, index: usize, base: Option<&Rect>) -> Option<Rect> {
        self.effective_clip(PresentedPrimitiveKind::Glyph, index, base)
    }

    pub(super) fn image_clip(&self, index: usize, base: Option<&Rect>) -> Option<Rect> {
        self.effective_clip(PresentedPrimitiveKind::Image, index, base)
    }

    fn effective_clip(
        &self,
        kind: PresentedPrimitiveKind,
        index: usize,
        base: Option<&Rect>,
    ) -> Option<Rect> {
        let Some(override_paint) = self.primitive_override(kind, index) else {
            return base.cloned();
        };
        let clip = override_paint.clip();
        let override_rect = Rect {
            x: clip.x(),
            y: clip.y(),
            width: clip.width(),
            height: clip.height(),
        };
        base.map_or(Some(override_rect), |base| {
            intersect_rect(base, &override_rect)
        })
    }

    fn primitive_override(
        &self,
        kind: PresentedPrimitiveKind,
        index: usize,
    ) -> Option<PrimitivePointerOverride> {
        self.overrides
            .get(index)
            .and_then(|entry| *entry)
            .and_then(|(actual_kind, value)| (actual_kind == kind).then_some(value))
    }
}

fn intersect_rect(left: &Rect, right: &Rect) -> Option<Rect> {
    let x = left.x.max(right.x);
    let y = left.y.max(right.y);
    let right_edge = (left.x + left.width).min(right.x + right.width);
    let bottom = (left.y + left.height).min(right.y + right.height);
    (right_edge > x && bottom > y).then_some(Rect {
        x,
        y,
        width: right_edge - x,
        height: bottom - y,
    })
}

fn rect_complement(domain: &Rect, cut: &Rect) -> [Option<Rect>; 4] {
    let mut out = [None; 4];
    let mut len = 0;
    let domain_right = domain.x + domain.width;
    let domain_bottom = domain.y + domain.height;
    let cut_right = cut.x + cut.width;
    let cut_bottom = cut.y + cut.height;
    if cut.y > domain.y {
        out[len] = Some(Rect::new(
            domain.x,
            domain.y,
            domain.width,
            cut.y - domain.y,
        ));
        len += 1;
    }
    if cut_bottom < domain_bottom {
        out[len] = Some(Rect::new(
            domain.x,
            cut_bottom,
            domain.width,
            domain_bottom - cut_bottom,
        ));
        len += 1;
    }
    if cut.x > domain.x {
        out[len] = Some(Rect::new(domain.x, cut.y, cut.x - domain.x, cut.height));
        len += 1;
    }
    if cut_right < domain_right {
        out[len] = Some(Rect::new(
            cut_right,
            cut.y,
            domain_right - cut_right,
            cut.height,
        ));
    }
    out
}

pub(super) fn clip_geometry(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    clip: Option<&Rect>,
) -> Option<(f32, f32, f32, f32)> {
    let Some(clip) = clip else {
        return Some((x, y, width, height));
    };
    let draw_x = x.max(clip.x);
    let draw_y = y.max(clip.y);
    let right = (x + width).min(clip.x + clip.width);
    let bottom = (y + height).min(clip.y + clip.height);
    (right > draw_x && bottom > draw_y).then_some((draw_x, draw_y, right - draw_x, bottom - draw_y))
}

/// Clip rect primitives appended since `start` without changing their layer
/// order. Every renderer rect is encoded as six vertices.
pub(super) fn clip_new_rect_vertices(
    vertices: &mut Vec<RectVertex>,
    start: usize,
    clip: Option<&Rect>,
) {
    let Some(clip) = clip else { return };
    let original_len = vertices.len();
    let mut write = start;
    for read in (start..original_len).step_by(6) {
        let Ok(rect) = <[RectVertex; 6]>::try_from(&vertices[read..read + 6]) else {
            break;
        };
        let min_x = rect
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
        let min_y = rect
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_x = rect
            .iter()
            .map(|v| v.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = rect
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let Some((x, y, width, height)) =
            clip_geometry(min_x, min_y, max_x - min_x, max_y - min_y, Some(clip))
        else {
            continue;
        };
        let color = rect[0].color;
        let clipped = [
            RectVertex {
                position: [x, y],
                color,
            },
            RectVertex {
                position: [x + width, y],
                color,
            },
            RectVertex {
                position: [x + width, y + height],
                color,
            },
            RectVertex {
                position: [x, y],
                color,
            },
            RectVertex {
                position: [x + width, y + height],
                color,
            },
            RectVertex {
                position: [x, y + height],
                color,
            },
        ];
        vertices[write..write + 6].copy_from_slice(&clipped);
        write += 6;
    }
    vertices.truncate(write);
}

pub(super) fn clip_new_rounded_vertices(
    vertices: &mut Vec<RoundedRectVertex>,
    start: usize,
    clip: Option<&Rect>,
) {
    let Some(clip) = clip else { return };
    let original_len = vertices.len();
    let mut write = start;
    for read in (start..original_len).step_by(6) {
        let Ok(quad) = <[RoundedRectVertex; 6]>::try_from(&vertices[read..read + 6]) else {
            break;
        };
        let min_x = quad
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
        let min_y = quad
            .iter()
            .map(|v| v.position[1])
            .fold(f32::INFINITY, f32::min);
        let max_x = quad
            .iter()
            .map(|v| v.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = quad
            .iter()
            .map(|v| v.position[1])
            .fold(f32::NEG_INFINITY, f32::max);
        let Some((x, y, width, height)) =
            clip_geometry(min_x, min_y, max_x - min_x, max_y - min_y, Some(clip))
        else {
            continue;
        };
        let template = quad[0];
        let clipped = [
            [x, y],
            [x + width, y],
            [x + width, y + height],
            [x, y],
            [x + width, y + height],
            [x, y + height],
        ]
        .map(|position| RoundedRectVertex {
            position,
            ..template
        });
        vertices[write..write + 6].copy_from_slice(&clipped);
        write += 6;
    }
    vertices.truncate(write);
}

pub(super) fn clip_glyph_quad(
    quad: [GlyphVertex; 6],
    clip: Option<&Rect>,
) -> Option<[GlyphVertex; 6]> {
    let Some(clip) = clip else { return Some(quad) };
    let min_x = quad
        .iter()
        .map(|v| v.position[0])
        .fold(f32::INFINITY, f32::min);
    let min_y = quad
        .iter()
        .map(|v| v.position[1])
        .fold(f32::INFINITY, f32::min);
    let max_x = quad
        .iter()
        .map(|v| v.position[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let max_y = quad
        .iter()
        .map(|v| v.position[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let width = max_x - min_x;
    let height = max_y - min_y;
    let (x, y, draw_width, draw_height) = clip_geometry(min_x, min_y, width, height, Some(clip))?;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let min_u = quad
        .iter()
        .map(|v| v.tex_coords[0])
        .fold(f32::INFINITY, f32::min);
    let max_u = quad
        .iter()
        .map(|v| v.tex_coords[0])
        .fold(f32::NEG_INFINITY, f32::max);
    let min_v = quad
        .iter()
        .map(|v| v.tex_coords[1])
        .fold(f32::INFINITY, f32::min);
    let max_v = quad
        .iter()
        .map(|v| v.tex_coords[1])
        .fold(f32::NEG_INFINITY, f32::max);
    let u0 = min_u + (max_u - min_u) * ((x - min_x) / width);
    let u1 = min_u + (max_u - min_u) * ((x + draw_width - min_x) / width);
    let v0 = min_v + (max_v - min_v) * ((y - min_y) / height);
    let v1 = min_v + (max_v - min_v) * ((y + draw_height - min_y) / height);
    let color = quad[0].color;
    Some([
        GlyphVertex {
            position: [x, y],
            tex_coords: [u0, v0],
            color,
        },
        GlyphVertex {
            position: [x + draw_width, y],
            tex_coords: [u1, v0],
            color,
        },
        GlyphVertex {
            position: [x + draw_width, y + draw_height],
            tex_coords: [u1, v1],
            color,
        },
        GlyphVertex {
            position: [x, y],
            tex_coords: [u0, v0],
            color,
        },
        GlyphVertex {
            position: [x + draw_width, y + draw_height],
            tex_coords: [u1, v1],
            color,
        },
        GlyphVertex {
            position: [x, y + draw_height],
            tex_coords: [u0, v1],
            color,
        },
    ])
}

pub(super) fn clip_subpixel_quad(
    quad: [SubpixelGlyphVertex; 6],
    clip: Option<&Rect>,
) -> Option<[SubpixelGlyphVertex; 6]> {
    let glyph_quad = quad.map(|vertex| GlyphVertex {
        position: vertex.position,
        tex_coords: vertex.tex_coords,
        color: vertex.fg_color,
    });
    let clipped = clip_glyph_quad(glyph_quad, clip)?;
    let fg_color = quad[0].fg_color;
    let bg_color = quad[0].bg_color;
    Some(clipped.map(|vertex| SubpixelGlyphVertex {
        position: vertex.position,
        tex_coords: vertex.tex_coords,
        fg_color,
        bg_color,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ReliefEdge {
    corners: [[f32; 2]; 4],
    color: Color,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ReliefEdgePlan([Option<ReliefEdge>; 6]);

impl IntoIterator for ReliefEdgePlan {
    type Item = ReliefEdge;
    type IntoIter = std::iter::Flatten<std::array::IntoIter<Option<ReliefEdge>, 6>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter().flatten()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ReliefCorner {
    BottomRight,
    BottomLeft,
    TopLeft,
    TopRight,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ReliefCornerErase {
    corner: ReliefCorner,
    bounds: Rect,
    color: Color,
    radius: f32,
    margin: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ReliefCornerErasePlan([Option<ReliefCornerErase>; 4]);

impl IntoIterator for ReliefCornerErasePlan {
    type Item = ReliefCornerErase;
    type IntoIter = std::iter::Flatten<std::array::IntoIter<Option<ReliefCornerErase>, 4>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter().flatten()
    }
}

impl ReliefCornerErase {
    #[cfg(test)]
    pub(super) const fn corner(self) -> ReliefCorner {
        self.corner
    }

    #[cfg(test)]
    pub(super) const fn bounds(self) -> (f32, f32, f32, f32) {
        (
            self.bounds.x,
            self.bounds.y,
            self.bounds.width,
            self.bounds.height,
        )
    }

    #[cfg(test)]
    pub(super) const fn color(self) -> Color {
        self.color
    }
}

impl ReliefEdge {
    #[cfg(test)]
    pub(super) fn bounds(self) -> (f32, f32, f32, f32) {
        let min_x = self
            .corners
            .iter()
            .map(|point| point[0])
            .fold(f32::INFINITY, f32::min);
        let min_y = self
            .corners
            .iter()
            .map(|point| point[1])
            .fold(f32::INFINITY, f32::min);
        let max_x = self
            .corners
            .iter()
            .map(|point| point[0])
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = self
            .corners
            .iter()
            .map(|point| point[1])
            .fold(f32::NEG_INFINITY, f32::max);
        (min_x, min_y, max_x - min_x, max_y - min_y)
    }

    pub(super) const fn color(self) -> Color {
        self.color
    }

    pub(super) const fn corners(self) -> [[f32; 2]; 4] {
        self.corners
    }
}

/// GNU pgtk relief outside the original image content rectangle. Side rectangles draw
/// first; horizontal edges then select the top-right and bottom-left corner
/// pixels. Top tapers only into a right edge and bottom only out of a left
/// edge. Thick relief finishes with the dark one-pixel top/left outer lines.
pub(super) fn relief_edges(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    relief: PointerImageRelief,
) -> Option<ReliefEdgePlan> {
    if relief.thickness() <= 0.0 {
        return None;
    }
    let bounds = resolved_relief_rect(x, y, width, height, relief)?;
    let (x, y, width, height) = (bounds.x, bounds.y, bounds.width, bounds.height);
    let top_left = relief.top_left_color();
    let bottom_right = relief.bottom_right_color();
    let edge = relief.thickness().min(width * 0.5).min(height * 0.5);
    let enabled = relief.edges();
    let right = x + width;
    let bottom = y + height;
    let rectangle = |left: f32, top: f32, right: f32, bottom: f32, color| ReliefEdge {
        corners: [[left, top], [right, top], [right, bottom], [left, bottom]],
        color,
    };
    let top = if enabled.right() {
        ReliefEdge {
            corners: [[x, y], [right, y], [right - edge, y + edge], [x, y + edge]],
            color: top_left,
        }
    } else {
        rectangle(x, y, right, y + edge, top_left)
    };
    let bottom_edge = if enabled.left() {
        ReliefEdge {
            corners: [
                [x + edge, bottom - edge],
                [right, bottom - edge],
                [right, bottom],
                [x, bottom],
            ],
            color: bottom_right,
        }
    } else {
        rectangle(x, bottom - edge, right, bottom, bottom_right)
    };
    Some(ReliefEdgePlan([
        enabled.left().then_some(ReliefEdge {
            corners: [[x, y], [x + edge, y], [x + edge, bottom], [x, bottom]],
            color: top_left,
        }),
        enabled.right().then_some(ReliefEdge {
            corners: [
                [right - edge, y],
                [right, y],
                [right, bottom],
                [right - edge, bottom],
            ],
            color: bottom_right,
        }),
        enabled.top().then_some(top),
        enabled.bottom().then_some(bottom_edge),
        (enabled.left() && edge > 1.0).then_some(rectangle(x, y, x + 1.0, bottom, bottom_right)),
        (enabled.top() && edge > 1.0).then_some(rectangle(x, y, right, y + 1.0, bottom_right)),
    ]))
}

fn resolved_relief_rect(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    relief: PointerImageRelief,
) -> Option<Rect> {
    let margins = relief.margins();
    let bounds = Rect {
        x: x - margins.left(),
        y: y - margins.top(),
        width: width + margins.left() + margins.right(),
        height: height + margins.top() + margins.bottom(),
    };
    (bounds.width > 0.0 && bounds.height > 0.0).then_some(bounds)
}

/// GNU pgtk erases rounded corners only where a horizontal and vertical relief
/// edge meet. The order matches its `enum corners` iteration order.
pub(super) fn relief_corner_erases(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    relief: PointerImageRelief,
) -> ReliefCornerErasePlan {
    if relief.thickness() <= 0.0 {
        return ReliefCornerErasePlan([None; 4]);
    }
    let Some(bounds) = resolved_relief_rect(x, y, width, height, relief) else {
        return ReliefCornerErasePlan([None; 4]);
    };
    let enabled = relief.edges();
    let erase = relief.corner_erase();
    let make = |corner| ReliefCornerErase {
        corner,
        bounds,
        color: erase.color(),
        radius: erase.radius(),
        margin: erase.margin(),
    };
    ReliefCornerErasePlan([
        (enabled.bottom() && enabled.right()).then(|| make(ReliefCorner::BottomRight)),
        (enabled.bottom() && enabled.left()).then(|| make(ReliefCorner::BottomLeft)),
        (enabled.top() && enabled.left()).then(|| make(ReliefCorner::TopLeft)),
        (enabled.top() && enabled.right()).then(|| make(ReliefCorner::TopRight)),
    ])
}

/// Append the complete resolved relief in GNU paint order: edges, thick-edge
/// corrections, then background-colored corner erasure.
pub(super) fn append_clipped_relief(
    vertices: &mut Vec<RectVertex>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    relief: PointerImageRelief,
    clip: Option<&Rect>,
) {
    if let Some(edges) = relief_edges(x, y, width, height, relief) {
        for edge in edges {
            append_clipped_relief_edge(vertices, edge, clip);
        }
    }
    for erase in relief_corner_erases(x, y, width, height, relief) {
        append_clipped_relief_corner_erase(vertices, erase, clip);
    }
}

fn append_clipped_relief_corner_erase(
    vertices: &mut Vec<RectVertex>,
    erase: ReliefCornerErase,
    clip: Option<&Rect>,
) {
    let bounds = erase.bounds;
    let left = bounds.x;
    let top = bounds.y;
    let right = left + bounds.width;
    let bottom = top + bounds.height;
    let (outer, center, start_angle) = match erase.corner {
        ReliefCorner::BottomRight => (
            [right + erase.margin, bottom + erase.margin],
            [
                right + erase.margin - erase.radius,
                bottom + erase.margin - erase.radius,
            ],
            0.0,
        ),
        ReliefCorner::BottomLeft => (
            [left - erase.margin, bottom + erase.margin],
            [
                left - erase.margin + erase.radius,
                bottom + erase.margin - erase.radius,
            ],
            std::f32::consts::FRAC_PI_2,
        ),
        ReliefCorner::TopLeft => (
            [left - erase.margin, top - erase.margin],
            [
                left - erase.margin + erase.radius,
                top - erase.margin + erase.radius,
            ],
            std::f32::consts::PI,
        ),
        ReliefCorner::TopRight => (
            [right + erase.margin, top - erase.margin],
            [
                right + erase.margin - erase.radius,
                top - erase.margin + erase.radius,
            ],
            3.0 * std::f32::consts::FRAC_PI_2,
        ),
    };
    const SEGMENTS: usize = 16;
    let point = |index: usize| {
        let angle = start_angle + std::f32::consts::FRAC_PI_2 * (index as f32 / SEGMENTS as f32);
        [
            center[0] + erase.radius * angle.cos(),
            center[1] + erase.radius * angle.sin(),
        ]
    };
    for index in 0..SEGMENTS {
        append_clipped_relief_triangle(
            vertices,
            [outer, point(index), point(index + 1)],
            erase.color,
            &bounds,
            clip,
        );
    }
}

fn append_clipped_relief_triangle(
    vertices: &mut Vec<RectVertex>,
    triangle: [[f32; 2]; 3],
    color: Color,
    bounds: &Rect,
    clip: Option<&Rect>,
) {
    let mut polygon = [[0.0; 2]; 8];
    polygon[..3].copy_from_slice(&triangle);
    let mut len = 3;
    for clip in std::iter::once(bounds).chain(clip) {
        for (axis, boundary, keep_greater) in [
            (0, clip.x, true),
            (0, clip.x + clip.width, false),
            (1, clip.y, true),
            (1, clip.y + clip.height, false),
        ] {
            (polygon, len) = clip_convex_polygon(polygon, len, axis, boundary, keep_greater);
            if len < 3 {
                return;
            }
        }
    }
    let color = [color.r, color.g, color.b, color.a];
    for index in 1..len - 1 {
        for position in [polygon[0], polygon[index], polygon[index + 1]] {
            vertices.push(RectVertex { position, color });
        }
    }
}

pub(super) fn append_clipped_relief_edge(
    vertices: &mut Vec<RectVertex>,
    edge: ReliefEdge,
    clip: Option<&Rect>,
) {
    let mut polygon = [[0.0; 2]; 8];
    polygon[..4].copy_from_slice(&edge.corners());
    let mut len = 4;
    if let Some(clip) = clip {
        for (axis, boundary, keep_greater) in [
            (0, clip.x, true),
            (0, clip.x + clip.width, false),
            (1, clip.y, true),
            (1, clip.y + clip.height, false),
        ] {
            (polygon, len) = clip_convex_polygon(polygon, len, axis, boundary, keep_greater);
            if len < 3 {
                return;
            }
        }
    }
    let color = edge.color();
    let color = [color.r, color.g, color.b, color.a];
    for index in 1..len - 1 {
        for position in [polygon[0], polygon[index], polygon[index + 1]] {
            vertices.push(RectVertex { position, color });
        }
    }
}

fn clip_convex_polygon(
    input: [[f32; 2]; 8],
    len: usize,
    axis: usize,
    boundary: f32,
    keep_greater: bool,
) -> ([[f32; 2]; 8], usize) {
    let mut output = [[0.0; 2]; 8];
    let mut out_len = 0;
    let inside = |point: [f32; 2]| {
        if keep_greater {
            point[axis] >= boundary
        } else {
            point[axis] <= boundary
        }
    };
    let intersection = |from: [f32; 2], to: [f32; 2]| {
        let delta = to[axis] - from[axis];
        let t = if delta.abs() <= f32::EPSILON {
            0.0
        } else {
            (boundary - from[axis]) / delta
        };
        [
            from[0] + (to[0] - from[0]) * t,
            from[1] + (to[1] - from[1]) * t,
        ]
    };
    let mut previous = input[len - 1];
    let mut previous_inside = inside(previous);
    for &current in &input[..len] {
        let current_inside = inside(current);
        if current_inside != previous_inside {
            output[out_len] = intersection(previous, current);
            out_len += 1;
        }
        if current_inside {
            output[out_len] = current;
            out_len += 1;
        }
        previous = current;
        previous_inside = current_inside;
    }
    (output, out_len)
}

#[cfg(test)]
#[path = "pointer_override_test.rs"]
mod tests;

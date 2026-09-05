//! Shared context types for the `render_frame_glyphs` phase pipeline.
//!
//! A frame renders in two stages:
//! 1. CPU collection phases build per-layer vertex sets (`layer_backgrounds`,
//!    `layer_chrome`) from the frame glyph buffer.
//! 2. A single render pass draws the layers in the documented z-order
//!    (backgrounds -> text -> decorations -> borders -> media -> cursors);
//!    the pass-side phases live in `layer_text`, `layer_media`, and
//!    `layer_chrome`.
//!
//! [`FrameParams`] carries the immutable per-frame inputs through both
//! stages; [`FramePassCtx`] bundles the active render pass with those
//! params for the draw phases.

use neomacs_display_protocol::types::FaceId;
use std::collections::HashMap;

use neomacs_display_protocol::DeviceScale;
use neomacs_display_protocol::PresentedPrimitiveKind;
use neomacs_display_protocol::face::{BoxType, BoxVerticalEdges, Face};
use neomacs_display_protocol::frame_glyphs::{
    FrameGlyph, FrameGlyphBuffer, GlyphRowRole, ScrollBarIdentity,
};
use neomacs_display_protocol::types::{AnimatedCursor, Color, Rect};

use super::super::vertex::RectVertex;
use super::cursor_presentation::InverseVideoCell;
use super::pointer_override::PointerOverrideResolver;

/// Immutable per-frame inputs shared by every render phase.
pub(super) struct FrameParams<'a> {
    pub(super) frame_glyphs: &'a FrameGlyphBuffer,
    pub(super) pointer_override: PointerOverrideResolver,
    pub(super) faces: &'a HashMap<FaceId, Face>,
    pub(super) cursor_visible: bool,
    pub(super) animated_cursor: &'a Option<AnimatedCursor>,
    /// Present-time inverse-video contract for the active filled-box cursor.
    /// `None` while its visual box is in flight, so text cannot be recolored
    /// at a destination the box has not reached.
    pub(super) cursor_inverse_video: Option<InverseVideoCell>,
    /// Which scroll bar the pointer is over, resolved by the compositor.
    ///
    /// An identity rather than a position: the pointer's position is on the
    /// root surface and every rect in this buffer is in the presentation being
    /// drawn, and while a pane is in motion those name different pixels. A
    /// draw phase has no way to tell them apart, so it is given the answer.
    pub(super) hovered_scroll_bar: Option<ScrollBarIdentity>,
    // RGB-pair gradient endpoints; a dedicated type alias would add little here.
    #[allow(clippy::type_complexity)]
    pub(super) background_gradient: Option<((f32, f32, f32), (f32, f32, f32))>,
    /// Logical frame size from `prepare_frame_uniforms`.
    pub(super) logical_w: f32,
    pub(super) logical_h: f32,
    /// Native pixels per logical pixel for device-defined decoration widths.
    pub(super) device_scale: DeviceScale,
    pub(super) face_debug_call_id: u64,
    /// Whether line/scroll-spacing animations are active this frame
    /// (glyph Y positions then go through `line_y_offset`).
    pub(super) has_line_anims: bool,
    /// Layout row damage for this frame (built by display-runtime from the
    /// same FrameDisplayState the glyph buffer was materialized from).
    pub(super) row_damage: Option<&'a super::row_reuse::FrameRowDamage>,
}

/// The active render pass plus the per-frame params, handed to each draw
/// phase in z-order.
pub(super) struct FramePassCtx<'e, 'a> {
    pub(super) pass: wgpu::RenderPass<'e>,
    pub(super) params: &'e FrameParams<'a>,
}

/// A merged span of adjacent boxed glyphs on the same row.
pub(super) struct BoxSpan {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) face_id: FaceId,
    pub(super) row_role: GlyphRowRole,
    pub(super) bg: Option<Color>,
    pub(super) clip: Option<Rect>,
    pub(super) policy: BoxPaintPolicy,
    /// Image textures do not cover their margin/row-sized box slot, so the
    /// shared box pass must paint that face background first.
    pub(super) requires_background_fill: bool,
    /// Vertical terminal sides owned by the first and last glyph in the span.
    pub(super) box_vertical_edges: BoxVerticalEdges,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum BoxPaintPolicy {
    Rounded,
    Sharp,
}

/// A transient face paint starts a box run of its own.  Reusing the base
/// glyph's edge ownership would make a hover/pressed box inherit an unrelated
/// open side; GNU adds terminal sides at mouse-face transitions instead.
pub(super) const fn box_edges_for_face_paint(
    base_face: FaceId,
    painted_face: FaceId,
    base_edges: BoxVerticalEdges,
    continues_left: bool,
    continues_right: bool,
) -> BoxVerticalEdges {
    if base_face.get() == painted_face.get() {
        base_edges
    } else {
        BoxVerticalEdges::from_ownership(!continues_left, !continues_right)
    }
}

fn transient_face_continues(
    frame_glyphs: &FrameGlyphBuffer,
    pointer_override: &PointerOverrideResolver,
    neighbor_index: Option<usize>,
    current_rect: Rect,
    current_role: GlyphRowRole,
    painted_face: FaceId,
    neighbor_on_left: bool,
) -> bool {
    let Some(neighbor_index) = neighbor_index else {
        return false;
    };
    let Some(neighbor) = frame_glyphs.glyphs.get(neighbor_index) else {
        return false;
    };
    if pointer_override.transient_face(box_pointer_kind(neighbor), neighbor_index)
        != Some(painted_face)
    {
        return false;
    }
    let Some(neighbor_rect) = neighbor.box_rect() else {
        return false;
    };
    if neighbor.row_role() != Some(current_role)
        || (neighbor_rect.y - current_rect.y).abs() >= 1.0
        || (neighbor_rect.height - current_rect.height).abs() >= 1.0
    {
        return false;
    }
    if neighbor_on_left {
        ((neighbor_rect.x + neighbor_rect.width) - current_rect.x).abs() < 1.0
    } else {
        ((current_rect.x + current_rect.width) - neighbor_rect.x).abs() < 1.0
    }
}

const fn box_pointer_kind(glyph: &FrameGlyph) -> PresentedPrimitiveKind {
    if matches!(glyph, FrameGlyph::Image { .. }) {
        PresentedPrimitiveKind::Image
    } else {
        PresentedPrimitiveKind::Glyph
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct BoxGroupKey {
    row_role: GlyphRowRole,
    y: u32,
    height: u32,
    clip: Option<(u32, u32, u32, u32)>,
    policy: BoxPaintPolicy,
    face: FaceId,
    requires_background_fill: bool,
}

impl BoxGroupKey {
    fn new(span: &BoxSpan) -> Self {
        let clip = span.clip.map(|clip| {
            (
                clip.x.to_bits(),
                clip.y.to_bits(),
                clip.width.to_bits(),
                clip.height.to_bits(),
            )
        });
        Self {
            row_role: span.row_role,
            y: span.y.to_bits(),
            height: span.height.to_bits(),
            clip,
            policy: span.policy,
            face: span.face_id,
            requires_background_fill: span.requires_background_fill,
        }
    }
}

/// O(1)-average semantic grouping while retaining first-contribution output
/// order. Face identity remains part of the paint key: edge ownership can
/// suppress an internal cap, but it must never merge distinct GNU face
/// materials and paint the second span with the first span's color/style.
#[derive(Default)]
pub(super) struct BoxSpanAccumulator {
    spans: Vec<BoxSpan>,
    open_group: HashMap<BoxGroupKey, usize>,
}

impl BoxSpanAccumulator {
    pub(super) fn push(&mut self, candidate: BoxSpan) {
        let key = BoxGroupKey::new(&candidate);
        if let Some(&index) = self.open_group.get(&key) {
            let existing = &mut self.spans[index];
            // Explicit terminals are affine run boundaries, even when the
            // adjoining runs happen to use the same realized face.  GNU
            // includes left/right box flags in glyph-string equality; merging
            // across a Right|Left seam would erase both valid caps.
            let boundary_is_open = !existing.box_vertical_edges.owns_right()
                && !candidate.box_vertical_edges.owns_left();
            if boundary_is_open && ((existing.x + existing.width) - candidate.x).abs() < 1.0 {
                existing.box_vertical_edges = existing
                    .box_vertical_edges
                    .merged_with(candidate.box_vertical_edges);
                existing.width = candidate.x + candidate.width - existing.x;
                return;
            }
        }
        let index = self.spans.len();
        self.spans.push(candidate);
        self.open_group.insert(key, index);
    }

    pub(super) fn finish(self) -> Vec<BoxSpan> {
        self.spans
    }

    #[cfg(test)]
    pub(super) fn group_count(&self) -> usize {
        self.open_group.len()
    }
}

/// Collect GNU box runs from every inline face-bearing primitive.
///
/// Main and child-frame rendering deliberately share this semantic pass.  The
/// only difference between those paths is the later coordinate offset; box
/// membership, pointer-face substitution, material identity, and terminal
/// ownership must not drift.
pub(super) fn collect_frame_box_spans(
    frame_glyphs: &FrameGlyphBuffer,
    faces: &HashMap<FaceId, Face>,
    pointer_override: &PointerOverrideResolver,
) -> Vec<BoxSpan> {
    let mut box_spans = BoxSpanAccumulator::default();
    for (glyph_index, glyph) in frame_glyphs.glyphs.iter().enumerate() {
        let Some(rect) = glyph.box_rect() else {
            continue;
        };
        let (gx, gy, gw, gh) = (rect.x, rect.y, rect.width, rect.height);
        let Some(base_face_id) = glyph.face_id() else {
            continue;
        };
        let Some(row_role) = glyph.row_role() else {
            continue;
        };
        let glyph_rect = Rect::new(gx, gy, gw, gh);
        for paint in pointer_override.face_paints_for(
            box_pointer_kind(glyph),
            glyph_index,
            base_face_id,
            glyph_rect,
            glyph.clip_rect().as_ref(),
        ) {
            let face_id = paint.face_id();
            let Some(face) = faces.get(&face_id) else {
                continue;
            };
            if matches!(face.box_type, BoxType::None) || !face.box_line_width.is_visible() {
                continue;
            }
            let policy = if face.box_corner_radius > 0 {
                BoxPaintPolicy::Rounded
            } else {
                BoxPaintPolicy::Sharp
            };
            box_spans.push(BoxSpan {
                x: gx,
                y: gy,
                width: gw,
                height: gh,
                face_id,
                row_role,
                bg: Some(face.background),
                clip: paint.clip(),
                policy,
                requires_background_fill: glyph.box_requires_background_fill(),
                box_vertical_edges: box_edges_for_face_paint(
                    base_face_id,
                    face_id,
                    glyph.box_vertical_edges().unwrap_or_default(),
                    transient_face_continues(
                        frame_glyphs,
                        pointer_override,
                        glyph_index.checked_sub(1),
                        glyph_rect,
                        row_role,
                        face_id,
                        true,
                    ),
                    transient_face_continues(
                        frame_glyphs,
                        pointer_override,
                        glyph_index.checked_add(1),
                        glyph_rect,
                        row_role,
                        face_id,
                        false,
                    ),
                ),
            });
        }
    }
    box_spans.finish()
}

/// All merged box spans of a frame.
pub(super) struct BoxSpanSet {
    pub(super) spans: Vec<BoxSpan>,
}

/// Cursor and window-border vertex sets collected before the render pass.
/// `cursor_bg` and `behind_text_cursor` draw before text (inverse-video filled
/// box cursor); `cursors` draws after text and also carries the scroll bar
/// track + thumb rects, which all go through the rect pipeline.
pub(super) struct ChromeLayerVertices {
    pub(super) cursor_bg: Vec<RectVertex>,
    pub(super) behind_text_cursor: Vec<RectVertex>,
    pub(super) cursors: Vec<RectVertex>,
}

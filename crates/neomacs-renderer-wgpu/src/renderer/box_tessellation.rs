//! Shared box-span tessellation for top-level and child-frame render paths.
//!
//! Layout owns box-run topology.  This module is the single renderer seam that
//! turns that topology into sharp/3D/rounded geometry, so child frames cannot
//! drift from the primary frame when a new edge policy is added.

use neomacs_display_protocol::DeviceScale;
use neomacs_display_protocol::face::{BoxType, Face};
use neomacs_display_protocol::types::{Color, Rect};

use super::super::vertex::{RectVertex, RoundedRectVertex};
use super::WgpuRenderer;
use super::frame_pass::{BoxPaintPolicy, BoxSpan};

fn offset_clip(clip: Option<Rect>, offset_x: f32, offset_y: f32) -> Option<Rect> {
    clip.map(|clip| Rect {
        x: clip.x + offset_x,
        y: clip.y + offset_y,
        width: clip.width,
        height: clip.height,
    })
}

fn sharp_edge_colors(face: &Face, box_color: Color) -> (Color, Color) {
    let light = Color {
        r: (box_color.r * 1.4).min(1.0),
        g: (box_color.g * 1.4).min(1.0),
        b: (box_color.b * 1.4).min(1.0),
        a: box_color.a,
    };
    let dark = Color {
        r: box_color.r * 0.6,
        g: box_color.g * 0.6,
        b: box_color.b * 0.6,
        a: box_color.a,
    };
    match face.box_type {
        BoxType::Raised3D => (light, dark),
        BoxType::Sunken3D => (dark, light),
        _ => (box_color, box_color),
    }
}

impl WgpuRenderer {
    pub(super) fn append_required_box_background_fill_geometry(
        &self,
        vertices: &mut Vec<RectVertex>,
        span: &BoxSpan,
        offset_x: f32,
        offset_y: f32,
    ) {
        if !span.requires_background_fill || span.policy != BoxPaintPolicy::Sharp {
            return;
        }
        let Some(background) = span.bg.as_ref() else {
            return;
        };
        let start = vertices.len();
        self.add_rect(
            vertices,
            span.x + offset_x,
            span.y + offset_y,
            span.width,
            span.height,
            background,
        );
        super::pointer_override::clip_new_rect_vertices(
            vertices,
            start,
            offset_clip(span.clip, offset_x, offset_y).as_ref(),
        );
    }

    pub(super) fn append_rounded_box_fill_geometry(
        &self,
        vertices: &mut Vec<RoundedRectVertex>,
        span: &BoxSpan,
        face: &Face,
        offset_x: f32,
        offset_y: f32,
    ) {
        let Some(background) = span.bg.as_ref() else {
            return;
        };
        if face.box_corner_radius <= 0 {
            return;
        }
        let start = vertices.len();
        let radius = (face.box_corner_radius as f32)
            .min(span.height * 0.45)
            .min(span.width * 0.45);
        self.add_rounded_rect_with_box_vertical_edges(
            vertices,
            span.x + offset_x,
            span.y + offset_y,
            span.width,
            span.height,
            span.height.max(span.width),
            radius,
            background,
            span.box_vertical_edges,
        );
        super::pointer_override::clip_new_rounded_vertices(
            vertices,
            start,
            offset_clip(span.clip, offset_x, offset_y).as_ref(),
        );
    }

    /// Append one span's border and report whether it is animated.
    pub(super) fn append_box_border_geometry(
        &self,
        sharp: &mut Vec<RectVertex>,
        rounded: &mut Vec<RoundedRectVertex>,
        span: &BoxSpan,
        face: &Face,
        device_scale: DeviceScale,
        offset_x: f32,
        offset_y: f32,
    ) -> bool {
        let box_color = *face.box_color.as_ref().unwrap_or(&face.foreground);
        let border_width = face
            .box_line_width
            .logical_geometry(device_scale)
            .paint_thickness()
            .get();
        let clip = offset_clip(span.clip, offset_x, offset_y);
        let x = span.x + offset_x;
        let y = span.y + offset_y;

        if face.box_corner_radius > 0 {
            let start = rounded.len();
            let radius = (face.box_corner_radius as f32)
                .min(span.height * 0.45)
                .min(span.width * 0.45);
            let color2 = face.box_color2.as_ref().unwrap_or(&box_color);
            self.add_rounded_rect_styled_with_box_vertical_edges(
                rounded,
                x,
                y,
                span.width,
                span.height,
                border_width,
                radius,
                &box_color,
                face.box_border_style.gnu_code(),
                face.box_border_speed,
                color2,
                span.box_vertical_edges,
            );
            super::pointer_override::clip_new_rounded_vertices(rounded, start, clip.as_ref());
            return face.box_border_style.is_fancy();
        }

        let start = sharp.len();
        let (top_left, bottom_right) = sharp_edge_colors(face, box_color);
        self.add_rect(sharp, x, y, span.width, border_width, &top_left);
        self.add_rect(
            sharp,
            x,
            y + span.height - border_width,
            span.width,
            border_width,
            &bottom_right,
        );
        if span.box_vertical_edges.owns_left() {
            self.add_rect(sharp, x, y, border_width, span.height, &top_left);
        }
        if span.box_vertical_edges.owns_right() {
            self.add_rect(
                sharp,
                x + span.width - border_width,
                y,
                border_width,
                span.height,
                &bottom_right,
            );
        }
        super::pointer_override::clip_new_rect_vertices(sharp, start, clip.as_ref());
        false
    }
}

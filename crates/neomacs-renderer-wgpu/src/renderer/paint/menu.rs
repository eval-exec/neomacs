//! UI overlay rendering methods for WgpuRenderer.

use super::super::{RenderTarget, WgpuRenderer, draw};
use crate::glyph_atlas::{
    GlyphAtlasHandle, GlyphKey, SubpixelRequest, WgpuGlyphAtlas, glyph_font_identity,
};
use crate::vertex::RectVertex;
use cosmic_text::SubpixelBin;
use neomacs_display_protocol::types::{Color, FaceId};

impl WgpuRenderer {
    pub(in crate::renderer) fn paint_menu(
        &mut self,
        target: RenderTarget<'_>,
        draw: &draw::DrawParameters,
        menu: &neomacs_display_protocol::menu::MenuPanelPaint<'_>,
        glyph_atlas: &mut WgpuGlyphAtlas,
    ) {
        let view = target.view;
        let scale_factor = target.surface.device_scale().get();
        // Native menus are standalone frames. All preceding window work has
        // been submitted before this call; reset their uploads so repeated
        // hover redraws do not grow arenas while the editor is otherwise idle.
        self.arenas.rect.begin_frame();
        self.arenas.glyph.begin_frame();

        // Derive colors from face colors if provided, otherwise use defaults.
        let (fg_r, fg_g, fg_b) = menu.face_fg.unwrap_or((0.9, 0.9, 0.9));
        let (bg_r, bg_g, bg_b) = menu.face_bg.unwrap_or((0.15, 0.15, 0.18));

        let bg_color = Color::new(bg_r, bg_g, bg_b, 0.95).srgb_to_linear();
        let border_color = Color::new(
            (bg_r * 0.6 + 0.15).min(1.0),
            (bg_g * 0.6 + 0.15).min(1.0),
            (bg_b * 0.6 + 0.15).min(1.0),
            1.0,
        )
        .srgb_to_linear();
        let hover_color = Color::new(
            bg_r * 0.5 + fg_r * 0.3,
            bg_g * 0.5 + fg_g * 0.3,
            bg_b * 0.5 + fg_b * 0.3,
            0.9,
        )
        .srgb_to_linear();
        let text_color = {
            let c = Color::new(fg_r, fg_g, fg_b, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let disabled_color = {
            let c = Color::new(
                fg_r * 0.5 + bg_r * 0.5,
                fg_g * 0.5 + bg_g * 0.5,
                fg_b * 0.5 + bg_b * 0.5,
                1.0,
            )
            .srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let separator_color = Color::new(
            bg_r * 0.7 + fg_r * 0.3,
            bg_g * 0.7 + fg_g * 0.3,
            bg_b * 0.7 + fg_b * 0.3,
            0.8,
        )
        .srgb_to_linear();
        let title_color = {
            let c = Color::new(
                fg_r * 0.8 + bg_r * 0.2,
                fg_g * 0.8 + bg_g * 0.2,
                fg_b * 0.85 + bg_b * 0.15,
                1.0,
            )
            .srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let shortcut_color = {
            let c = Color::new(
                fg_r * 0.65 + bg_r * 0.35,
                fg_g * 0.65 + bg_g * 0.35,
                fg_b * 0.65 + bg_b * 0.35,
                1.0,
            )
            .srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        let padding = 4.0_f32;
        let _font_size = glyph_atlas.default_font_size();
        let char_width = glyph_atlas.default_char_width();
        let font_size_bits = 0.0_f32.to_bits();

        // Render each panel (root + open submenus)
        {
            let panel = menu.panel;
            let (mx, my, mw, mh) = panel.bounds;
            let indicator_width = panel.indicator_width(menu.all_items);
            let shortcut_right = panel.shortcut_right(menu.all_items, char_width);

            // === Pass 1: Background rectangles ===
            let mut rect_vertices: Vec<RectVertex> = Vec::new();

            // Drop shadow
            let shadow_layers = 4;
            for i in 1..=shadow_layers {
                let offset = i as f32 * 1.5;
                let alpha = 0.12 * (1.0 - (i - 1) as f32 / shadow_layers as f32);
                let shadow = Color::new(0.0, 0.0, 0.0, alpha);
                self.add_rect(
                    &mut rect_vertices,
                    mx + offset,
                    my + offset,
                    mw,
                    mh,
                    &shadow,
                );
            }

            // Background
            self.add_rect(&mut rect_vertices, mx, my, mw, mh, &bg_color);

            // Border
            let bw = 1.0_f32;
            self.add_rect(&mut rect_vertices, mx, my, mw, bw, &border_color);
            self.add_rect(&mut rect_vertices, mx, my + mh - bw, mw, bw, &border_color);
            self.add_rect(&mut rect_vertices, mx, my, bw, mh, &border_color);
            self.add_rect(&mut rect_vertices, mx + mw - bw, my, bw, mh, &border_color);

            // Hover highlight
            if panel.hover_index >= 0 && (panel.hover_index as usize) < panel.item_indices.len() {
                let idx = panel.hover_index as usize;
                let iy = my + panel.item_offsets[idx];
                self.add_rect(
                    &mut rect_vertices,
                    mx + bw,
                    iy,
                    mw - 2.0 * bw,
                    panel.item_height,
                    &hover_color,
                );
            }

            // Separators
            for (i, &item_idx) in panel.item_indices.iter().enumerate() {
                if menu.all_items[item_idx].separator() {
                    let iy = my + panel.item_offsets[i] + 3.0;
                    self.add_rect(
                        &mut rect_vertices,
                        mx + 8.0,
                        iy,
                        mw - 16.0,
                        1.0,
                        &separator_color,
                    );
                }
            }

            for (i, &item_idx) in panel.item_indices.iter().enumerate() {
                let item = &menu.all_items[item_idx];
                let color = if item.enabled() {
                    text_color
                } else {
                    disabled_color
                };
                indicators::paint(
                    &mut rect_vertices,
                    item.indicator(),
                    mx + padding * 2.0,
                    my + panel.item_offsets[i],
                    panel.item_height,
                    color,
                );
                if item.submenu() {
                    indicators::submenu_arrow(
                        &mut rect_vertices,
                        mx + mw - padding * 2.0,
                        my + panel.item_offsets[i],
                        panel.item_height,
                        color,
                    );
                }
            }

            // Title separator (root panel only)
            if menu.title.is_some() {
                let sep_y = my + panel.item_height + 2.0;
                self.add_rect(
                    &mut rect_vertices,
                    mx + 4.0,
                    sep_y,
                    mw - 8.0,
                    1.0,
                    &separator_color,
                );
            }

            // Submit rect pass
            if let Some(rect_buffer) =
                self.arenas
                    .rect
                    .upload(&self.device, &self.queue, &rect_vertices)
            {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Popup Menu Rect Encoder"),
                        });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Popup Menu Rect Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    pass.set_pipeline(&self.pipelines.rect);
                    pass.set_bind_group(0, draw.binding(), &[]);
                    pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
                    pass.draw(0..rect_vertices.len() as u32, 0..1);
                }
                self.queue.submit(Some(encoder.finish()));
            }

            // === Pass 2: Text glyphs ===
            let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();

            // Title (root panel only)
            if let Some(title) = menu.title {
                let tx = mx + padding * 2.0;
                for (ci, ch) in title.chars().enumerate() {
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: FaceId::new(0),
                        font_size_bits,
                        font_identity: glyph_font_identity(menu.font_face),
                        x_bin: SubpixelBin::Zero,
                        y_bin: SubpixelBin::Zero,
                    };
                    if let Some(handle) = glyph_atlas.get_or_create_atlas(
                        &self.device,
                        &self.queue,
                        &key,
                        menu.font_face,
                        SubpixelRequest::Disabled,
                    ) {
                        overlay_glyphs.push((
                            handle,
                            tx + (ci as f32) * char_width,
                            my + padding,
                            title_color,
                        ));
                    }
                }
            }

            // Menu items
            for (i, &item_idx) in panel.item_indices.iter().enumerate() {
                let item = &menu.all_items[item_idx];
                if item.separator() {
                    continue;
                }
                let iy = my + panel.item_offsets[i];
                let color = if !item.enabled() {
                    disabled_color
                } else {
                    text_color
                };

                let label_x = mx + padding * 2.0 + indicator_width;
                for (ci, ch) in item.label.chars().enumerate() {
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: FaceId::new(0),
                        font_size_bits,
                        font_identity: glyph_font_identity(menu.font_face),
                        x_bin: SubpixelBin::Zero,
                        y_bin: SubpixelBin::Zero,
                    };
                    if let Some(handle) = glyph_atlas.get_or_create_atlas(
                        &self.device,
                        &self.queue,
                        &key,
                        menu.font_face,
                        SubpixelRequest::Disabled,
                    ) {
                        overlay_glyphs.push((
                            handle,
                            label_x + (ci as f32) * char_width,
                            iy + 2.0,
                            color,
                        ));
                    }
                }

                if !item.shortcut.is_empty() {
                    let shortcut_x =
                        shortcut_right - item.shortcut.chars().count() as f32 * char_width;
                    for (ci, ch) in item.shortcut.chars().enumerate() {
                        let key = GlyphKey {
                            charcode: ch as u32,
                            face_id: FaceId::new(0),
                            font_size_bits,
                            font_identity: glyph_font_identity(menu.font_face),
                            x_bin: SubpixelBin::Zero,
                            y_bin: SubpixelBin::Zero,
                        };
                        if let Some(handle) = glyph_atlas.get_or_create_atlas(
                            &self.device,
                            &self.queue,
                            &key,
                            menu.font_face,
                            SubpixelRequest::Disabled,
                        ) {
                            overlay_glyphs.push((
                                handle,
                                shortcut_x + (ci as f32) * char_width,
                                iy + 2.0,
                                shortcut_color,
                            ));
                        }
                    }
                }
            }

            self.render_overlay_glyphs_scaled(
                view,
                &mut overlay_glyphs,
                glyph_atlas,
                scale_factor,
                draw,
            );
        }
    }
}
mod indicators;

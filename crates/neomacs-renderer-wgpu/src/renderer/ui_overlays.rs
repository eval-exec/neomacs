//! UI overlay rendering methods for WgpuRenderer.

use super::super::glyph_atlas::{
    AnyAtlasEntry, GlyphAtlasHandle, GlyphKey, GlyphMaterialKind, SubpixelRequest, WgpuGlyphAtlas,
    glyph_font_identity,
};
use super::super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, Uniforms};
use super::TitleFadeEntry;
use super::WgpuRenderer;
use cosmic_text::SubpixelBin;
use neomacs_display_protocol::font::GlyphSampling;
use neomacs_display_protocol::frame_chrome::{BandRect, FrameRect, PositionedChromeItem};
use neomacs_display_protocol::frame_glyphs::FrameGlyphBuffer;
use neomacs_display_protocol::types::{Color, FaceId, ImageId};
use neomacs_display_protocol::{MenuBarItem, ToolBarImageSource, ToolBarItem};
use std::collections::HashMap;

pub(super) fn placed_chrome_item_bounds(
    band: FrameRect,
    item: BandRect,
) -> neomacs_display_protocol::types::Rect {
    band.place(item)
        .expect("published frame chrome item must fit its band")
        .raw()
}

pub(super) fn toolbar_texture_id(
    icon_textures: &HashMap<(ToolBarImageSource, u32), ImageId>,
    image: &ToolBarImageSource,
    icon_size: u32,
) -> Option<ImageId> {
    icon_textures.get(&(image.clone(), icon_size)).copied()
}

impl WgpuRenderer {
    /// Render a popup menu overlay on top of all content.
    pub fn render_popup_menu(
        &mut self,
        view: &wgpu::TextureView,
        menu: &crate::overlay_state::PopupMenuState,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

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
        let panels = menu.panels();
        for (panel_idx, panel) in panels.iter().enumerate() {
            let (mx, my, mw, mh) = panel.bounds;

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
                if menu.all_items[item_idx].separator {
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

            // Title separator (root panel only)
            if panel_idx == 0 && menu.title.is_some() {
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
                    pass.set_pipeline(&self.pipelines.rect);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
                    pass.draw(0..rect_vertices.len() as u32, 0..1);
                }
                self.queue.submit(Some(encoder.finish()));
            }

            // === Pass 2: Text glyphs ===
            let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();

            // Title (root panel only)
            if panel_idx == 0
                && let Some(ref title) = menu.title
            {
                let tx = mx + padding * 2.0;
                for (ci, ch) in title.chars().enumerate() {
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: FaceId::new(0),
                        font_size_bits,
                        font_identity: glyph_font_identity(None),
                        x_bin: SubpixelBin::Zero,
                        y_bin: SubpixelBin::Zero,
                    };
                    if let Some(handle) = glyph_atlas.get_or_create_atlas(
                        &self.device,
                        &self.queue,
                        &key,
                        None,
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
                if item.separator {
                    continue;
                }
                let iy = my + panel.item_offsets[i];
                let color = if !item.enabled {
                    disabled_color
                } else {
                    text_color
                };

                let label_x = mx + padding * 2.0;
                for (ci, ch) in item.label.chars().enumerate() {
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: FaceId::new(0),
                        font_size_bits,
                        font_identity: glyph_font_identity(None),
                        x_bin: SubpixelBin::Zero,
                        y_bin: SubpixelBin::Zero,
                    };
                    if let Some(handle) = glyph_atlas.get_or_create_atlas(
                        &self.device,
                        &self.queue,
                        &key,
                        None,
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
                        mx + mw - padding * 2.0 - (item.shortcut.len() as f32 * char_width);
                    for (ci, ch) in item.shortcut.chars().enumerate() {
                        let key = GlyphKey {
                            charcode: ch as u32,
                            face_id: FaceId::new(0),
                            font_size_bits,
                            font_identity: glyph_font_identity(None),
                            x_bin: SubpixelBin::Zero,
                            y_bin: SubpixelBin::Zero,
                        };
                        if let Some(handle) = glyph_atlas.get_or_create_atlas(
                            &self.device,
                            &self.queue,
                            &key,
                            None,
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

                if item.submenu {
                    let arrow_x = mx + mw - padding * 2.0 - char_width;
                    let key = GlyphKey {
                        charcode: '\u{25B8}' as u32,
                        face_id: FaceId::new(0),
                        font_size_bits,
                        font_identity: glyph_font_identity(None),
                        x_bin: SubpixelBin::Zero,
                        y_bin: SubpixelBin::Zero,
                    };
                    if let Some(handle) = glyph_atlas.get_or_create_atlas(
                        &self.device,
                        &self.queue,
                        &key,
                        None,
                        SubpixelRequest::Disabled,
                    ) {
                        overlay_glyphs.push((handle, arrow_x, iy + 2.0, text_color));
                    }
                }
            }

            self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
        }
    }

    /// Render a batch of overlay glyphs in a single render pass.
    ///
    /// Each entry is (GlyphKey, x, y, color). Glyphs are sorted by key
    /// so identical characters share a single bind_group switch, and all
    /// rendering happens in one encoder submit instead of one per glyph.
    fn render_overlay_glyphs(
        &mut self,
        view: &wgpu::TextureView,
        glyphs: &mut [(GlyphAtlasHandle, f32, f32, [f32; 4])],
        glyph_atlas: &WgpuGlyphAtlas,
    ) {
        if glyphs.is_empty() {
            return;
        }

        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(glyphs.len() * 6);
        let mut page_ids: Vec<(GlyphMaterialKind, u32, GlyphSampling)> =
            Vec::with_capacity(glyphs.len());

        let sf = self.scale_factor;
        let font_ascent = glyph_atlas.default_font_ascent();

        for (handle, x, y, color) in glyphs.iter() {
            let entry = handle.entry;
            let metrics = entry.metrics();
            let uv = entry.uv();
            let content_rect = entry.rect();
            let tex_u_min = uv.min()[0];
            let tex_u_max = uv.max()[0];
            let tex_v_min = uv.min()[1];
            let tex_v_max = uv.max()[1];
            let glyph_x = *x + metrics.bearing_x / sf;
            let glyph_y = *y + font_ascent - metrics.bearing_y / sf;
            let glyph_w = content_rect.width() as f32 / sf;
            let glyph_h = content_rect.height() as f32 / sf;

            vertices.extend_from_slice(&[
                GlyphVertex {
                    position: [glyph_x, glyph_y],
                    tex_coords: [tex_u_min, tex_v_min],
                    color: *color,
                },
                GlyphVertex {
                    position: [glyph_x + glyph_w, glyph_y],
                    tex_coords: [tex_u_max, tex_v_min],
                    color: *color,
                },
                GlyphVertex {
                    position: [glyph_x + glyph_w, glyph_y + glyph_h],
                    tex_coords: [tex_u_max, tex_v_max],
                    color: *color,
                },
                GlyphVertex {
                    position: [glyph_x, glyph_y],
                    tex_coords: [tex_u_min, tex_v_min],
                    color: *color,
                },
                GlyphVertex {
                    position: [glyph_x + glyph_w, glyph_y + glyph_h],
                    tex_coords: [tex_u_max, tex_v_max],
                    color: *color,
                },
                GlyphVertex {
                    position: [glyph_x, glyph_y + glyph_h],
                    tex_coords: [tex_u_min, tex_v_max],
                    color: *color,
                },
            ]);
            page_ids.push(entry.binding_id_value());
        }

        let Some(buffer) = self
            .arenas
            .glyph
            .upload(&self.device, &self.queue, &vertices)
        else {
            return;
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Overlay Glyph Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Overlay Glyph Pass"),
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
            pass.set_pipeline(&self.pipelines.glyph);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.buffer_slice());

            let mut vert_idx = 0u32;
            let mut i = 0;
            while i < glyphs.len() {
                let (handle, _, _, _) = &glyphs[i];
                let entry = handle.entry;
                let page_id = entry.binding_id_value();
                let batch_start = vert_idx;
                vert_idx += 6;
                i += 1;
                while i < glyphs.len() && page_ids[i] == page_id {
                    vert_idx += 6;
                    i += 1;
                }
                let bg = match glyph_atlas.atlas_bind_group(entry) {
                    Ok(bg) => bg,
                    Err(err) => {
                        tracing::warn!(?err, "skipping stale overlay glyph batch");
                        continue;
                    }
                };
                if matches!(entry, AnyAtlasEntry::Color(_)) {
                    pass.set_pipeline(&self.pipelines.opaque_image);
                } else {
                    pass.set_pipeline(&self.pipelines.glyph);
                }
                pass.set_bind_group(1, bg, &[]);
                pass.draw(batch_start..vert_idx, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render watermark text in windows with small/empty buffers.
    pub fn render_window_watermarks(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
    ) {
        if !self.effects.window_watermark.enabled {
            return;
        }

        let font_size = glyph_atlas.default_font_size();
        let scale = 3.0_f32;
        let char_width = glyph_atlas.default_char_width() * scale;
        let char_height = font_size * scale;
        let font_size_bits = 0.0_f32.to_bits();
        let alpha = self.effects.window_watermark.opacity.clamp(0.0, 1.0);

        let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4], f32)> = Vec::new();

        for info in &frame_glyphs.window_infos {
            if info.is_minibuffer {
                continue;
            }
            if info.buffer_size > self.effects.window_watermark.threshold as i64 {
                continue;
            }

            let b = &info.bounds;
            // FIXME(chrome-insets): ignores top chrome (tab/header line); region
            // starts at b.y and bleeds over it. Use info.content_rect(). See the
            // chrome-insets module note in window_effects.rs.
            let content_h = b.height - info.mode_line_height;
            if content_h < char_height * 1.5 {
                continue;
            }

            // Determine watermark text: use buffer file name basename, or fallback
            let text = if !info.buffer_file_name.is_empty() {
                let name = info
                    .buffer_file_name
                    .rsplit('/')
                    .next()
                    .unwrap_or(&info.buffer_file_name);
                name.to_string()
            } else {
                "empty".to_string()
            };

            // Truncate long names to fit window width
            let max_chars = ((b.width * 0.8) / char_width) as usize;
            let display_text: String = if text.len() > max_chars && max_chars > 3 {
                text.chars().take(max_chars - 2).collect::<String>() + ".."
            } else {
                text.clone()
            };

            let text_width = display_text.chars().count() as f32 * char_width;
            let start_x = b.x + (b.width - text_width) / 2.0;
            let start_y = b.y + (content_h - char_height) / 2.0;

            let color = [1.0, 1.0, 1.0, alpha];

            for (ci, ch) in display_text.chars().enumerate() {
                if ch == ' ' {
                    continue;
                }
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: FaceId::new(0),
                    font_size_bits,
                    font_identity: glyph_font_identity(None),
                    x_bin: SubpixelBin::Zero,
                    y_bin: SubpixelBin::Zero,
                };
                if let Some(handle) = glyph_atlas.get_or_create_atlas(
                    &self.device,
                    &self.queue,
                    &key,
                    None,
                    SubpixelRequest::Disabled,
                ) {
                    overlay_glyphs.push((
                        handle,
                        start_x + ci as f32 * char_width,
                        start_y,
                        color,
                        scale,
                    ));
                }
            }
        }

        if overlay_glyphs.is_empty() {
            return;
        }

        // Sort by page_id for batching
        overlay_glyphs.sort_by(|a, b| {
            let pa = a.0.entry.binding_id_value();
            let pb = b.0.entry.binding_id_value();
            pa.cmp(&pb)
        });

        let sf = self.scale_factor;
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(overlay_glyphs.len() * 6);
        let mut page_ids: Vec<(GlyphMaterialKind, u32, GlyphSampling)> =
            Vec::with_capacity(overlay_glyphs.len());

        for (handle, x, y, color, s) in overlay_glyphs.iter() {
            let entry = handle.entry;
            let metrics = entry.metrics();
            let uv = entry.uv();
            let content_rect = entry.rect();
            let tex_u_min = uv.min()[0];
            let tex_u_max = uv.max()[0];
            let tex_v_min = uv.min()[1];
            let tex_v_max = uv.max()[1];
            let gw = content_rect.width() as f32 / sf * s;
            let gh = content_rect.height() as f32 / sf * s;
            let gx = *x + metrics.bearing_x / sf * s;
            let gy = *y + (char_height * 0.7) - metrics.bearing_y / sf * s;

            vertices.extend_from_slice(&[
                GlyphVertex {
                    position: [gx, gy],
                    tex_coords: [tex_u_min, tex_v_min],
                    color: *color,
                },
                GlyphVertex {
                    position: [gx + gw, gy],
                    tex_coords: [tex_u_max, tex_v_min],
                    color: *color,
                },
                GlyphVertex {
                    position: [gx + gw, gy + gh],
                    tex_coords: [tex_u_max, tex_v_max],
                    color: *color,
                },
                GlyphVertex {
                    position: [gx, gy],
                    tex_coords: [tex_u_min, tex_v_min],
                    color: *color,
                },
                GlyphVertex {
                    position: [gx + gw, gy + gh],
                    tex_coords: [tex_u_max, tex_v_max],
                    color: *color,
                },
                GlyphVertex {
                    position: [gx, gy + gh],
                    tex_coords: [tex_u_min, tex_v_max],
                    color: *color,
                },
            ]);
            page_ids.push(entry.binding_id_value());
        }

        let Some(buffer) = self
            .arenas
            .glyph
            .upload(&self.device, &self.queue, &vertices)
        else {
            return;
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Watermark Glyph Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Watermark Glyph Pass"),
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
            pass.set_pipeline(&self.pipelines.glyph);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.buffer_slice());

            let mut vert_idx = 0u32;
            let mut i = 0;
            while i < overlay_glyphs.len() {
                let (handle, _, _, _, _) = &overlay_glyphs[i];
                let entry = handle.entry;
                let page_id = entry.binding_id_value();
                let batch_start = vert_idx;
                vert_idx += 6;
                i += 1;
                while i < overlay_glyphs.len() && page_ids[i] == page_id {
                    vert_idx += 6;
                    i += 1;
                }
                let bg = match glyph_atlas.atlas_bind_group(entry) {
                    Ok(bg) => bg,
                    Err(err) => {
                        tracing::warn!(?err, "skipping stale watermark glyph batch");
                        continue;
                    }
                };
                if matches!(entry, AnyAtlasEntry::Color(_)) {
                    pass.set_pipeline(&self.pipelines.opaque_image);
                } else {
                    pass.set_pipeline(&self.pipelines.glyph);
                }
                pass.set_bind_group(1, bg, &[]);
                pass.draw(batch_start..vert_idx, 0..1);
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render a tooltip overlay on top of the scene.
    pub fn render_tooltip(
        &mut self,
        view: &wgpu::TextureView,
        tooltip: &crate::overlay_state::TooltipState,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let (tx, ty, tw, th) = tooltip.bounds;

        // Convert user-specified colors to linear space (surface is sRGB)
        let bg_color = Color::new(tooltip.bg.0, tooltip.bg.1, tooltip.bg.2, 0.95).srgb_to_linear();
        let border_color = Color::new(
            (tooltip.bg.0 * 0.6 + 0.15).min(1.0),
            (tooltip.bg.1 * 0.6 + 0.15).min(1.0),
            (tooltip.bg.2 * 0.6 + 0.15).min(1.0),
            1.0,
        )
        .srgb_to_linear();
        let text_color = {
            let c = Color::new(tooltip.fg.0, tooltip.fg.1, tooltip.fg.2, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        // === Pass 1: Background and border rectangles ===
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // Drop shadow (layered for soft edge)
        let shadow_layers = 3;
        for i in 1..=shadow_layers {
            let offset = i as f32 * 1.0;
            let alpha = 0.10 * (1.0 - (i - 1) as f32 / shadow_layers as f32);
            let shadow = Color::new(0.0, 0.0, 0.0, alpha);
            self.add_rect(
                &mut rect_vertices,
                tx + offset,
                ty + offset,
                tw,
                th,
                &shadow,
            );
        }

        // Background
        self.add_rect(&mut rect_vertices, tx, ty, tw, th, &bg_color);

        // Border (1px)
        let bw = 1.0_f32;
        self.add_rect(&mut rect_vertices, tx, ty, tw, bw, &border_color); // top
        self.add_rect(&mut rect_vertices, tx, ty + th - bw, tw, bw, &border_color); // bottom
        self.add_rect(&mut rect_vertices, tx, ty, bw, th, &border_color); // left
        self.add_rect(&mut rect_vertices, tx + tw - bw, ty, bw, th, &border_color); // right

        if let Some(rect_buffer) =
            self.arenas
                .rect
                .upload(&self.device, &self.queue, &rect_vertices)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Tooltip Rect Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Tooltip Rect Pass"),
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
                pass.set_pipeline(&self.pipelines.rect);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // === Pass 2: Collect all text glyphs and render batched ===
        let padding = 6.0_f32;
        let line_height = glyph_atlas.default_line_height();
        let char_width = glyph_atlas.default_char_width();
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();

        for (line_idx, line) in tooltip.lines.iter().enumerate() {
            let ly = ty + padding + line_idx as f32 * line_height;
            for (ci, ch) in line.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: FaceId::new(0),
                    font_size_bits,
                    font_identity: glyph_font_identity(None),
                    x_bin: SubpixelBin::Zero,
                    y_bin: SubpixelBin::Zero,
                };
                if let Some(handle) = glyph_atlas.get_or_create_atlas(
                    &self.device,
                    &self.queue,
                    &key,
                    None,
                    SubpixelRequest::Disabled,
                ) {
                    overlay_glyphs.push((
                        handle,
                        tx + padding + (ci as f32) * char_width,
                        ly,
                        text_color,
                    ));
                }
            }
        }

        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render a custom title bar overlay for borderless/undecorated windows.
    /// Draws a dark bar at the top with the window title and close/maximize/minimize buttons.
    pub fn render_custom_titlebar(
        &mut self,
        view: &wgpu::TextureView,
        title: &str,
        titlebar_height: f32,
        hover: u32,
        frame_bg: Option<(f32, f32, f32)>,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let tb_h = titlebar_height;
        let btn_w = 46.0_f32;

        // Derive colors from frame background (already in linear space) or fallback
        let bg_color = if let Some((r, g, b)) = frame_bg {
            // Slightly darken the frame bg for the title bar
            Color::new(r * 0.85, g * 0.85, b * 0.85, 0.95)
        } else {
            Color::new(0.12, 0.12, 0.14, 0.95).srgb_to_linear()
        };
        // Determine if theme is light or dark based on luminance
        let luminance = bg_color.r * 0.299 + bg_color.g * 0.587 + bg_color.b * 0.114;
        let is_light = luminance > 0.3;

        let border_color = if is_light {
            Color::new(bg_color.r * 0.8, bg_color.g * 0.8, bg_color.b * 0.8, 1.0)
        } else {
            Color::new(
                (bg_color.r + 0.05).min(1.0),
                (bg_color.g + 0.05).min(1.0),
                (bg_color.b + 0.05).min(1.0),
                1.0,
            )
        };
        let close_hover_color = Color::new(0.9, 0.2, 0.2, 0.9).srgb_to_linear();
        let btn_hover_color = if is_light {
            Color::new(0.0, 0.0, 0.0, 0.1)
        } else {
            Color::new(1.0, 1.0, 1.0, 0.1)
        };
        let text_color = if is_light {
            let c = Color::new(0.15, 0.15, 0.15, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        } else {
            let c = Color::new(0.8, 0.8, 0.82, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let btn_icon_color = if is_light {
            let c = Color::new(0.3, 0.3, 0.3, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        } else {
            let c = Color::new(0.7, 0.7, 0.72, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let close_icon_hover = {
            let c = Color::new(1.0, 1.0, 1.0, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        // === Pass 1: Background and button rectangles ===
        let mut rect_vertices: Vec<RectVertex> = Vec::new();

        // Title bar background
        self.add_rect(&mut rect_vertices, 0.0, 0.0, logical_w, tb_h, &bg_color);

        // Bottom border (1px)
        self.add_rect(
            &mut rect_vertices,
            0.0,
            tb_h - 1.0,
            logical_w,
            1.0,
            &border_color,
        );

        // Button positions
        let close_x = logical_w - btn_w;
        let max_x = logical_w - btn_w * 2.0;
        let min_x = logical_w - btn_w * 3.0;

        // Button hover highlights
        // hover: 0=none, 2=close, 3=maximize, 4=minimize
        if hover == 2 {
            self.add_rect(
                &mut rect_vertices,
                close_x,
                0.0,
                btn_w,
                tb_h,
                &close_hover_color,
            );
        } else if hover == 3 {
            self.add_rect(
                &mut rect_vertices,
                max_x,
                0.0,
                btn_w,
                tb_h,
                &btn_hover_color,
            );
        } else if hover == 4 {
            self.add_rect(
                &mut rect_vertices,
                min_x,
                0.0,
                btn_w,
                tb_h,
                &btn_hover_color,
            );
        }

        // Subtle button separator lines
        let sep_color = Color::new(0.2, 0.2, 0.22, 0.5).srgb_to_linear();
        self.add_rect(
            &mut rect_vertices,
            close_x,
            4.0,
            1.0,
            tb_h - 8.0,
            &sep_color,
        );
        self.add_rect(&mut rect_vertices, max_x, 4.0, 1.0, tb_h - 8.0, &sep_color);
        self.add_rect(&mut rect_vertices, min_x, 4.0, 1.0, tb_h - 8.0, &sep_color);

        // Render rect pass
        if let Some(rect_buffer) =
            self.arenas
                .rect
                .upload(&self.device, &self.queue, &rect_vertices)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Titlebar Rect Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Titlebar Rect Pass"),
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
                pass.set_pipeline(&self.pipelines.rect);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // === Pass 2: Title text and button icons ===
        let font_size = glyph_atlas.default_font_size();
        let char_width = glyph_atlas.default_char_width();
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();

        // Center title text
        let title_pixel_width = title.chars().count() as f32 * char_width;
        let title_x = (logical_w - title_pixel_width) / 2.0;
        let title_y = (tb_h - font_size) / 2.0;

        for (ci, ch) in title.chars().enumerate() {
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: FaceId::new(0),
                font_size_bits,
                font_identity: glyph_font_identity(None),
                x_bin: SubpixelBin::Zero,
                y_bin: SubpixelBin::Zero,
            };
            if let Some(handle) = glyph_atlas.get_or_create_atlas(
                &self.device,
                &self.queue,
                &key,
                None,
                SubpixelRequest::Disabled,
            ) {
                overlay_glyphs.push((
                    handle,
                    title_x + ci as f32 * char_width,
                    title_y,
                    text_color,
                ));
            }
        }

        // Button icons: minimize (─), maximize (□), close (×)
        let btn_center_y = (tb_h - font_size) / 2.0;
        let min_color = if hover == 4 {
            text_color
        } else {
            btn_icon_color
        };
        let max_color = if hover == 3 {
            text_color
        } else {
            btn_icon_color
        };
        let close_color = if hover == 2 {
            close_icon_hover
        } else {
            btn_icon_color
        };

        // Minimize: ─ (U+2500)
        let min_icon_x = min_x + (btn_w - char_width) / 2.0;
        let min_key = GlyphKey {
            charcode: 0x2500,
            face_id: FaceId::new(0),
            font_size_bits,
            font_identity: glyph_font_identity(None),
            x_bin: SubpixelBin::Zero,
            y_bin: SubpixelBin::Zero,
        };
        if let Some(handle) = glyph_atlas.get_or_create_atlas(
            &self.device,
            &self.queue,
            &min_key,
            None,
            SubpixelRequest::Disabled,
        ) {
            overlay_glyphs.push((handle, min_icon_x, btn_center_y, min_color));
        }

        // Maximize: □ (U+25A1)
        let max_icon_x = max_x + (btn_w - char_width) / 2.0;
        let max_key = GlyphKey {
            charcode: 0x25A1,
            face_id: FaceId::new(0),
            font_size_bits,
            font_identity: glyph_font_identity(None),
            x_bin: SubpixelBin::Zero,
            y_bin: SubpixelBin::Zero,
        };
        if let Some(handle) = glyph_atlas.get_or_create_atlas(
            &self.device,
            &self.queue,
            &max_key,
            None,
            SubpixelRequest::Disabled,
        ) {
            overlay_glyphs.push((handle, max_icon_x, btn_center_y, max_color));
        }

        // Close: × (U+00D7)
        let close_icon_x = close_x + (btn_w - char_width) / 2.0;
        let close_key = GlyphKey {
            charcode: 0x00D7,
            face_id: FaceId::new(0),
            font_size_bits,
            font_identity: glyph_font_identity(None),
            x_bin: SubpixelBin::Zero,
            y_bin: SubpixelBin::Zero,
        };
        if let Some(handle) = glyph_atlas.get_or_create_atlas(
            &self.device,
            &self.queue,
            &close_key,
            None,
            SubpixelRequest::Disabled,
        ) {
            overlay_glyphs.push((handle, close_icon_x, btn_center_y, close_color));
        }

        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render thin scroll position indicators on the right edge of each window.
    pub fn render_scroll_indicators(
        &mut self,
        view: &wgpu::TextureView,
        window_infos: &[neomacs_display_protocol::frame_glyphs::WindowInfo],
        surface_width: u32,
        surface_height: u32,
    ) {
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        let indicator_width = 3.0_f32;
        let multi_window = window_infos.len() > 1;

        for info in window_infos {
            // Focus ring for selected window (only when multiple windows visible)
            if multi_window && info.selected {
                let b = &info.bounds;
                let bw = 2.0_f32;
                let accent = Color::new(0.3, 0.5, 0.9, 0.4).srgb_to_linear();
                // Top
                self.add_rect(&mut rect_vertices, b.x, b.y, b.width, bw, &accent);
                // Bottom (above mode-line)
                let bottom_y = b.y + b.height - info.mode_line_height - bw;
                self.add_rect(&mut rect_vertices, b.x, bottom_y, b.width, bw, &accent);
                // Left
                self.add_rect(
                    &mut rect_vertices,
                    b.x,
                    b.y,
                    bw,
                    b.height - info.mode_line_height,
                    &accent,
                );
                // Right
                self.add_rect(
                    &mut rect_vertices,
                    b.x + b.width - bw,
                    b.y,
                    bw,
                    b.height - info.mode_line_height,
                    &accent,
                );
            }

            // Skip windows with no meaningful buffer content for scroll indicator
            if info.buffer_size <= 1 {
                continue;
            }

            let b = &info.bounds;
            // Content area height (exclude mode-line)
            // FIXME(chrome-insets): ignores top chrome (tab/header line); region
            // starts at b.y and bleeds over it. Use info.content_rect(). See the
            // chrome-insets module note in window_effects.rs.
            let content_h = b.height - info.mode_line_height;
            if content_h < 20.0 {
                continue;
            }

            // Scroll ratio: what fraction of the buffer is before window_start
            let start_ratio = (info.window_start as f32 - 1.0).max(0.0)
                / (info.buffer_size as f32 - 1.0).max(1.0);

            // Viewport ratio: what fraction of the buffer is visible
            let visible_chars = if info.window_end > 0 {
                (info.window_end - info.window_start).max(1) as f32
            } else {
                // Estimate: content_h worth of text
                content_h * 2.0 // rough chars estimate
            };
            let viewport_ratio = (visible_chars / info.buffer_size as f32).clamp(0.02, 1.0);

            // Indicator bar position and size
            let bar_h = (content_h * viewport_ratio).max(8.0).min(content_h);
            let bar_y = b.y + start_ratio * (content_h - bar_h);

            // Semi-transparent indicator color
            let color = Color::new(0.5, 0.5, 0.5, 0.25).srgb_to_linear();
            let x = b.x + b.width - indicator_width;

            self.add_rect(&mut rect_vertices, x, bar_y, indicator_width, bar_h, &color);
        }

        let Some(rect_buffer) = self
            .arenas
            .rect
            .upload(&self.device, &self.queue, &rect_vertices)
        else {
            return;
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Scroll Indicator Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Scroll Indicator Pass"),
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
            pass.set_pipeline(&self.pipelines.rect);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render IME preedit text at the cursor position with underline.
    pub fn render_ime_preedit(
        &mut self,
        view: &wgpu::TextureView,
        preedit_text: &str,
        cursor_x: f32,
        cursor_y: f32,
        cursor_height: f32,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        if preedit_text.is_empty() {
            return;
        }

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let font_size_bits = 0.0_f32.to_bits();
        // A preedit update is one replaceable Unicode text run.  Shape it as
        // a whole, just like ordinary frame text: advancing once per Rust
        // `char` is incorrect for fallback CJK fonts, combining marks,
        // ligatures, emoji sequences, and bidirectional scripts.
        let shaped_preedit = glyph_atlas.get_or_create_composed_atlas(
            &self.device,
            &self.queue,
            preedit_text,
            FaceId::new(0),
            font_size_bits,
            None,
            SubpixelBin::Zero,
            SubpixelBin::Zero,
            SubpixelRequest::Disabled,
        );
        let preedit_width = shaped_preedit.as_ref().map_or_else(
            || preedit_text.chars().count() as f32 * glyph_atlas.default_char_width(),
            |handles| {
                handles
                    .first()
                    .map_or(0.0, |handle| handle.advance_width / self.scale_factor)
            },
        );

        // Background and underline rects
        let bg_color = Color::new(0.15, 0.15, 0.2, 0.95).srgb_to_linear();
        let underline_color = Color::new(0.4, 0.6, 1.0, 1.0).srgb_to_linear();

        let px = cursor_x;
        let py = cursor_y;
        let pw = preedit_width + 4.0;
        let ph = cursor_height;

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        // Background
        self.add_rect(&mut rect_vertices, px, py, pw, ph, &bg_color);
        // Underline (2px at bottom)
        self.add_rect(
            &mut rect_vertices,
            px,
            py + ph - 2.0,
            pw,
            2.0,
            &underline_color,
        );

        if let Some(rect_buffer) =
            self.arenas
                .rect
                .upload(&self.device, &self.queue, &rect_vertices)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("IME Preedit Rect Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("IME Preedit Rect Pass"),
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
                pass.set_pipeline(&self.pipelines.rect);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Text glyphs
        let text_color = {
            let c = Color::new(1.0, 1.0, 1.0, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = shaped_preedit
            .map(|handles| {
                handles
                    .into_iter()
                    .map(|handle| (handle, px + 2.0, py, text_color))
                    .collect()
            })
            .unwrap_or_default();
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render a visual bell flash overlay (semi-transparent white rectangle fading out).
    /// Render an FPS counter overlay in the top-right corner.
    /// Render a corner mask to clip the window to a rounded rectangle.
    /// Uses dst = dst * src_alpha blend mode to zero out pixels outside
    /// the rounded rect shape.
    pub fn render_corner_mask(
        &mut self,
        view: &wgpu::TextureView,
        corner_radius: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Filled rounded rect covering the whole frame with alpha=1 inside, 0 outside.
        // border_width=0 triggers filled mode in the shader.
        let mut vertices: Vec<RoundedRectVertex> = Vec::new();
        self.add_rounded_rect(
            &mut vertices,
            0.0,
            0.0,
            logical_w,
            logical_h,
            0.0, // border_width=0 → filled mode
            corner_radius,
            &Color::new(1.0, 1.0, 1.0, 1.0), // white, alpha=1
        );

        let Some(buffer) = self
            .arenas
            .rounded
            .upload(&self.device, &self.queue, &vertices)
        else {
            return;
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Corner Mask Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Corner Mask Pass"),
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
            pass.set_pipeline(&self.pipelines.corner_mask);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.buffer_slice());
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    pub(super) fn breadcrumb_display_chars(path: &str) -> Vec<(char, bool)> {
        let separator = " \u{203A} "; // " › "
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if components.is_empty() {
            return Vec::new();
        }
        let show_start = if components.len() > 3 {
            components.len() - 3
        } else {
            0
        };
        let shown = &components[show_start..];
        let mut display_chars: Vec<(char, bool)> = Vec::new();
        if show_start > 0 {
            display_chars.push(('\u{2026}', true));
            for c in separator.chars() {
                display_chars.push((c, true));
            }
        }
        for (i, comp) in shown.iter().enumerate() {
            if i > 0 {
                for c in separator.chars() {
                    display_chars.push((c, true));
                }
            }
            let is_last = i == shown.len() - 1;
            for c in comp.chars() {
                display_chars.push((c, !is_last));
            }
        }
        display_chars
    }

    /// Render breadcrumb/path bars for windows with file-backed buffers
    pub fn render_breadcrumbs(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
    ) {
        if !self.effects.breadcrumb.enabled {
            return;
        }

        let char_width = glyph_atlas.default_char_width();
        let line_height = glyph_atlas.default_line_height();
        let bar_height = line_height + 4.0;
        let padding_x = 6.0_f32;
        let opacity = self.effects.breadcrumb.opacity.clamp(0.0, 1.0);

        // Detect title changes and start fade animations
        if self.effects.title_fade.enabled {
            for info in &frame_glyphs.window_infos {
                if info.is_minibuffer || info.buffer_file_name.is_empty() {
                    continue;
                }
                let wid = info.window_id.get();
                let new_text = &info.buffer_file_name;
                let changed = match self.fx.title_fade.prev_breadcrumb_text.get(&wid) {
                    Some(old) => old != new_text,
                    None => false, // first time seeing this window, no fade
                };
                if changed {
                    let old_text = self
                        .fx
                        .title_fade
                        .prev_breadcrumb_text
                        .get(&wid)
                        .cloned()
                        .unwrap_or_default();
                    // Remove any existing fade for this window
                    self.fx.title_fade.active.retain(|f| f.window_id != wid);
                    self.fx.title_fade.active.push(TitleFadeEntry {
                        window_id: wid,
                        bounds: info.bounds,
                        old_text,
                        new_text: new_text.clone(),
                        started: crate::clock::Instant::now(),
                        duration: std::time::Duration::from_millis(
                            self.effects.title_fade.duration_ms as u64,
                        ),
                    });
                }
                self.fx
                    .title_fade
                    .prev_breadcrumb_text
                    .insert(wid, new_text.clone());
            }
            // Clean up expired fades
            self.fx
                .title_fade
                .active
                .retain(|f| f.started.elapsed() < f.duration);
        }

        let mut all_rect_vertices: Vec<RectVertex> = Vec::new();
        let mut all_text_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();
        let font_size_bits = 0.0_f32.to_bits();
        let text_color_base = [0.85_f32, 0.85, 0.85, 1.0];
        let sep_color_base = [0.5_f32, 0.5, 0.5, 1.0];

        for info in &frame_glyphs.window_infos {
            if info.is_minibuffer || info.buffer_file_name.is_empty() {
                continue;
            }

            let b = &info.bounds;

            // Check if this window has an active title fade
            let active_fade = self
                .fx
                .title_fade
                .active
                .iter()
                .find(|f| f.window_id == info.window_id.get());

            if let Some(fade) = active_fade {
                // Crossfade: render old text fading out, new text fading in
                let t =
                    (fade.started.elapsed().as_secs_f32() / fade.duration.as_secs_f32()).min(1.0);
                // Ease-out quadratic
                let eased = t * (2.0 - t);
                let new_alpha = eased;
                let old_alpha = 1.0 - eased;

                // Background rect (full opacity)
                let display_chars_new = Self::breadcrumb_display_chars(&info.buffer_file_name);
                let display_chars_old = Self::breadcrumb_display_chars(&fade.old_text);
                let max_len = display_chars_new.len().max(display_chars_old.len());
                let bar_w = (max_len as f32 * char_width + padding_x * 2.0).min(b.width);
                let bar_x = b.x;
                let bar_y = b.y;

                let bg_color = Color::new(0.0, 0.0, 0.0, opacity);
                self.add_rect(
                    &mut all_rect_vertices,
                    bar_x,
                    bar_y,
                    bar_w,
                    bar_height,
                    &bg_color,
                );
                let edge_color = Color::new(0.3, 0.3, 0.3, opacity * 0.5);
                self.add_rect(
                    &mut all_rect_vertices,
                    bar_x,
                    bar_y + bar_height,
                    bar_w,
                    1.0,
                    &edge_color,
                );

                let text_y = bar_y + 2.0;

                // Old text fading out
                for (ci, &(ch, is_dim)) in display_chars_old.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w {
                        break;
                    }
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: FaceId::new(0),
                        font_size_bits,
                        font_identity: glyph_font_identity(None),
                        x_bin: SubpixelBin::Zero,
                        y_bin: SubpixelBin::Zero,
                    };
                    if let Some(handle) = glyph_atlas.get_or_create_atlas(
                        &self.device,
                        &self.queue,
                        &key,
                        None,
                        SubpixelRequest::Disabled,
                    ) {
                        let base = if is_dim {
                            sep_color_base
                        } else {
                            text_color_base
                        };
                        all_text_glyphs.push((
                            handle,
                            cx,
                            text_y,
                            [base[0], base[1], base[2], base[3] * old_alpha],
                        ));
                    }
                }

                // New text fading in
                for (ci, &(ch, is_dim)) in display_chars_new.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w {
                        break;
                    }
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: FaceId::new(0),
                        font_size_bits,
                        font_identity: glyph_font_identity(None),
                        x_bin: SubpixelBin::Zero,
                        y_bin: SubpixelBin::Zero,
                    };
                    if let Some(handle) = glyph_atlas.get_or_create_atlas(
                        &self.device,
                        &self.queue,
                        &key,
                        None,
                        SubpixelRequest::Disabled,
                    ) {
                        let base = if is_dim {
                            sep_color_base
                        } else {
                            text_color_base
                        };
                        all_text_glyphs.push((
                            handle,
                            cx,
                            text_y,
                            [base[0], base[1], base[2], base[3] * new_alpha],
                        ));
                    }
                }
            } else {
                // Normal rendering (no active fade)
                let display_chars = Self::breadcrumb_display_chars(&info.buffer_file_name);
                if display_chars.is_empty() {
                    continue;
                }

                let text_width = display_chars.len() as f32 * char_width;
                let bar_w = (text_width + padding_x * 2.0).min(b.width);
                let bar_x = b.x;
                let bar_y = b.y;

                let bg_color = Color::new(0.0, 0.0, 0.0, opacity);
                self.add_rect(
                    &mut all_rect_vertices,
                    bar_x,
                    bar_y,
                    bar_w,
                    bar_height,
                    &bg_color,
                );
                let edge_color = Color::new(0.3, 0.3, 0.3, opacity * 0.5);
                self.add_rect(
                    &mut all_rect_vertices,
                    bar_x,
                    bar_y + bar_height,
                    bar_w,
                    1.0,
                    &edge_color,
                );

                let text_y = bar_y + 2.0;
                for (ci, &(ch, is_dim)) in display_chars.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w {
                        break;
                    }
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: FaceId::new(0),
                        font_size_bits,
                        font_identity: glyph_font_identity(None),
                        x_bin: SubpixelBin::Zero,
                        y_bin: SubpixelBin::Zero,
                    };
                    if let Some(handle) = glyph_atlas.get_or_create_atlas(
                        &self.device,
                        &self.queue,
                        &key,
                        None,
                        SubpixelRequest::Disabled,
                    ) {
                        all_text_glyphs.push((
                            handle,
                            cx,
                            text_y,
                            if is_dim {
                                sep_color_base
                            } else {
                                text_color_base
                            },
                        ));
                    }
                }
            }
        }

        // Draw background rects
        if let Some(rect_buffer) =
            self.arenas
                .rect
                .upload(&self.device, &self.queue, &all_rect_vertices)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Breadcrumb Rect Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Breadcrumb Rect Pass"),
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
                pass.set_pipeline(&self.pipelines.rect);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
                pass.draw(0..all_rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Draw text glyphs
        if !all_text_glyphs.is_empty() {
            self.render_overlay_glyphs(view, &mut all_text_glyphs, glyph_atlas);
        }
    }

    /// Render typing speed (WPM) indicator in the bottom-right of the selected window
    pub fn render_typing_speed(
        &mut self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        wpm: f32,
    ) {
        // Find the selected window (non-minibuffer)
        let selected = frame_glyphs
            .window_infos
            .iter()
            .find(|w| w.selected && !w.is_minibuffer);
        let info = match selected {
            Some(i) => i,
            None => return,
        };

        let wpm_int = wpm.round() as u32;
        let label = format!("{} WPM", wpm_int);

        let char_width = glyph_atlas.default_char_width();
        let line_height = glyph_atlas.default_line_height();
        let padding_x = 8.0_f32;
        let padding_y = 2.0_f32;
        let bar_w = label.len() as f32 * char_width + padding_x * 2.0;
        let bar_h = line_height + padding_y * 2.0;
        let b = &info.bounds;
        let bar_x = b.x + b.width - bar_w - 4.0;
        // Place just above the mode-line
        let bar_y = b.y + b.height - info.mode_line_height - bar_h - 2.0;

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        let bg_color = Color::new(0.0, 0.0, 0.0, 0.6);
        self.add_rect(&mut rect_vertices, bar_x, bar_y, bar_w, bar_h, &bg_color);

        // Color the label based on WPM: gray→green→yellow→red
        let text_color = if wpm_int == 0 {
            [0.5, 0.5, 0.5, 0.8]
        } else if wpm_int < 40 {
            [0.4, 0.8, 0.4, 1.0] // green
        } else if wpm_int < 80 {
            [0.8, 0.8, 0.2, 1.0] // yellow
        } else {
            [1.0, 0.4, 0.2, 1.0] // orange-red
        };

        let font_size_bits = 0.0_f32.to_bits();
        let mut text_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();
        let text_y = bar_y + padding_y;
        for (ci, ch) in label.chars().enumerate() {
            let cx = bar_x + padding_x + ci as f32 * char_width;
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: FaceId::new(0),
                font_size_bits,
                font_identity: glyph_font_identity(None),
                x_bin: SubpixelBin::Zero,
                y_bin: SubpixelBin::Zero,
            };
            if let Some(handle) = glyph_atlas.get_or_create_atlas(
                &self.device,
                &self.queue,
                &key,
                None,
                SubpixelRequest::Disabled,
            ) {
                text_glyphs.push((handle, cx, text_y, text_color));
            }
        }

        // Draw background
        if let Some(rect_buffer) =
            self.arenas
                .rect
                .upload(&self.device, &self.queue, &rect_vertices)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Typing Speed Rect Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Typing Speed Rect Pass"),
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
                pass.set_pipeline(&self.pipelines.rect);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Draw text
        if !text_glyphs.is_empty() {
            self.render_overlay_glyphs(view, &mut text_glyphs, glyph_atlas);
        }
    }

    pub fn render_fps_overlay(
        &mut self,
        view: &wgpu::TextureView,
        lines: &[String],
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        if lines.is_empty() {
            return;
        }

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let char_width = glyph_atlas.default_char_width();
        let line_height = glyph_atlas.default_line_height();
        let padding = 4.0_f32;
        let line_spacing = 2.0_f32;

        // Badge size: width = longest line, height = all lines
        let max_text_w = lines
            .iter()
            .map(|l| l.len() as f32 * char_width)
            .fold(0.0_f32, f32::max);
        let num_lines = lines.len() as f32;
        let badge_w = max_text_w + padding * 2.0;
        let badge_h = num_lines * line_height + (num_lines - 1.0) * line_spacing + padding * 2.0;
        let badge_x = logical_w - badge_w - 4.0;
        let badge_y = 4.0;

        // Background badge (semi-transparent dark)
        let bg = Color::new(0.0, 0.0, 0.0, 0.6);
        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        self.add_rect(&mut rect_vertices, badge_x, badge_y, badge_w, badge_h, &bg);

        let Some(rect_buffer) = self
            .arenas
            .rect
            .upload(&self.device, &self.queue, &rect_vertices)
        else {
            return;
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("FPS Rect Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("FPS Rect Pass"),
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
            pass.set_pipeline(&self.pipelines.rect);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));

        // Text glyphs (green for good visibility)
        let text_color = [0.0_f32, 1.0, 0.0, 1.0]; // green in linear
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();
        for (li, line) in lines.iter().enumerate() {
            let y = badge_y + padding + li as f32 * (line_height + line_spacing);
            for (ci, ch) in line.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: FaceId::new(0),
                    font_size_bits,
                    font_identity: glyph_font_identity(None),
                    x_bin: SubpixelBin::Zero,
                    y_bin: SubpixelBin::Zero,
                };
                if let Some(handle) = glyph_atlas.get_or_create_atlas(
                    &self.device,
                    &self.queue,
                    &key,
                    None,
                    SubpixelRequest::Disabled,
                ) {
                    overlay_glyphs.push((
                        handle,
                        badge_x + padding + ci as f32 * char_width,
                        y,
                        text_color,
                    ));
                }
            }
        }
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    pub fn render_visual_bell(
        &mut self,
        view: &wgpu::TextureView,
        surface_width: u32,
        surface_height: u32,
        alpha: f32,
    ) {
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Semi-transparent white overlay in linear space
        let flash_color = Color::new(1.0, 1.0, 1.0, alpha).srgb_to_linear();

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        self.add_rect(
            &mut rect_vertices,
            0.0,
            0.0,
            logical_w,
            logical_h,
            &flash_color,
        );

        let Some(rect_buffer) = self
            .arenas
            .rect
            .upload(&self.device, &self.queue, &rect_vertices)
        else {
            return;
        };

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Visual Bell Encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Visual Bell Pass"),
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
            pass.set_pipeline(&self.pipelines.rect);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.buffer_slice());
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render the GPU menu bar overlay at the top of the frame.
    pub fn render_menu_bar(
        &mut self,
        view: &wgpu::TextureView,
        items: &[PositionedChromeItem<MenuBarItem>],
        band: FrameRect,
        fg: (f32, f32, f32),
        bg: (f32, f32, f32),
        hovered: Option<u32>,
        active: Option<u32>,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let bg_color = Color::new(bg.0, bg.1, bg.2, 1.0).srgb_to_linear();
        let padding_x = 8.0_f32;
        let font_size = glyph_atlas.default_font_size();
        let char_width = glyph_atlas.default_char_width();
        let font_size_bits = 0.0_f32.to_bits();

        // --- Pass 1: Background bar + item highlights ---
        let mut rect_verts: Vec<RectVertex> = Vec::new();

        // Full menu bar background
        self.add_rect(
            &mut rect_verts,
            band.x(),
            band.y(),
            band.width(),
            band.height(),
            &bg_color,
        );

        // Item hover/active highlights
        for positioned in items {
            let item = positioned.item();
            let bounds = placed_chrome_item_bounds(band, positioned.local_bounds());

            let is_hovered = hovered == Some(item.index);
            let is_active = active == Some(item.index);

            if is_active {
                let c = Color::new(fg.0, fg.1, fg.2, 0.15).srgb_to_linear();
                self.add_rect(
                    &mut rect_verts,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    &c,
                );
            } else if is_hovered {
                let c = Color::new(fg.0, fg.1, fg.2, 0.1).srgb_to_linear();
                self.add_rect(
                    &mut rect_verts,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    &c,
                );
            }
        }

        // Bottom border line
        let border_color = Color::new(fg.0, fg.1, fg.2, 0.15).srgb_to_linear();
        self.add_rect(
            &mut rect_verts,
            band.x(),
            band.bottom() - 1.0,
            band.width(),
            1.0,
            &border_color,
        );

        if let Some(buffer) = self
            .arenas
            .rect
            .upload(&self.device, &self.queue, &rect_verts)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Menu Bar Rect Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Menu Bar Rect Pass"),
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
                pass.set_pipeline(&self.pipelines.rect);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, buffer.buffer_slice());
                pass.draw(0..rect_verts.len() as u32, 0..1);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // --- Pass 2: Text labels via glyph atlas ---
        let text_color = {
            let c = Color::new(fg.0, fg.1, fg.2, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();
        let text_y = band.y() + (band.height() - font_size) / 2.0;

        for positioned in items {
            let item = positioned.item();
            let bounds = placed_chrome_item_bounds(band, positioned.local_bounds());
            let label_x = bounds.x + padding_x;
            for (ci, ch) in item.label.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: FaceId::new(0),
                    font_size_bits,
                    font_identity: glyph_font_identity(None),
                    x_bin: SubpixelBin::Zero,
                    y_bin: SubpixelBin::Zero,
                };
                if let Some(handle) = glyph_atlas.get_or_create_atlas(
                    &self.device,
                    &self.queue,
                    &key,
                    None,
                    SubpixelRequest::Disabled,
                ) {
                    overlay_glyphs.push((
                        handle,
                        label_x + (ci as f32) * char_width,
                        text_y,
                        text_color,
                    ));
                }
            }
        }

        tracing::trace!(
            "render_menu_bar: {} overlay_glyphs, text_y={}",
            overlay_glyphs.len(),
            text_y
        );
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    // Tab bar rendering has been moved to the layout engine's status-line
    // pipeline (GlyphRowRole::TabBar).  The render_tab_bar() method that
    // was here (~300 lines) has been removed.

    /// Render the compact GUI chrome bar: menu labels followed by tool-bar icons.
    pub fn render_compact_bar(
        &mut self,
        view: &wgpu::TextureView,
        menu_items: &[PositionedChromeItem<MenuBarItem>],
        tool_items: &[PositionedChromeItem<ToolBarItem>],
        band: FrameRect,
        menu_fg: (f32, f32, f32),
        menu_bg: (f32, f32, f32),
        tool_fg: (f32, f32, f32),
        _tool_bg: (f32, f32, f32),
        icon_textures: &HashMap<(ToolBarImageSource, u32), ImageId>,
        menu_hovered: Option<u32>,
        menu_active: Option<u32>,
        tool_hovered: Option<u32>,
        tool_pressed: Option<u32>,
        icon_size: u32,
        padding: u32,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        self.arenas.image.begin_frame();
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let bg_color = Color::new(menu_bg.0, menu_bg.1, menu_bg.2, 1.0).srgb_to_linear();
        let padding_x = 8.0_f32;
        let font_size = glyph_atlas.default_font_size();
        let char_width = glyph_atlas.default_char_width();
        let font_size_bits = 0.0_f32.to_bits();
        let icon_sz = icon_size as f32;
        let pad = padding as f32;

        let mut rect_verts: Vec<RectVertex> = Vec::new();
        self.add_rect(
            &mut rect_verts,
            band.x(),
            band.y(),
            band.width(),
            band.height(),
            &bg_color,
        );

        for positioned in menu_items {
            let item = positioned.item();
            let bounds = placed_chrome_item_bounds(band, positioned.local_bounds());
            let is_hovered = menu_hovered == Some(item.index);
            let is_active = menu_active == Some(item.index);
            if is_active {
                let c = Color::new(menu_fg.0, menu_fg.1, menu_fg.2, 0.15).srgb_to_linear();
                self.add_rect(
                    &mut rect_verts,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    &c,
                );
            } else if is_hovered {
                let c = Color::new(menu_fg.0, menu_fg.1, menu_fg.2, 0.1).srgb_to_linear();
                self.add_rect(
                    &mut rect_verts,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    &c,
                );
            }
        }

        for positioned in tool_items {
            let item = positioned.item();
            let bounds = placed_chrome_item_bounds(band, positioned.local_bounds());
            if item.is_separator() {
                let sep_x = bounds.x + bounds.width / 2.0 - 0.5;
                let sep_y = bounds.y + pad;
                let sep_h = (bounds.height - pad * 2.0).max(0.0);
                let sep_color = Color::new(tool_fg.0, tool_fg.1, tool_fg.2, 0.2).srgb_to_linear();
                self.add_rect(&mut rect_verts, sep_x, sep_y, 1.0, sep_h, &sep_color);
                continue;
            }

            let is_hovered = tool_hovered == Some(item.index);
            let is_pressed = tool_pressed == Some(item.index);
            if is_pressed {
                let c = Color::new(tool_fg.0, tool_fg.1, tool_fg.2, 0.2).srgb_to_linear();
                self.add_rect(
                    &mut rect_verts,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    &c,
                );
            } else if is_hovered && item.enabled {
                let c = Color::new(tool_fg.0, tool_fg.1, tool_fg.2, 0.1).srgb_to_linear();
                self.add_rect(
                    &mut rect_verts,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    &c,
                );
            }
        }

        let border_color = Color::new(menu_fg.0, menu_fg.1, menu_fg.2, 0.15).srgb_to_linear();
        self.add_rect(
            &mut rect_verts,
            band.x(),
            band.bottom() - 1.0,
            band.width(),
            1.0,
            &border_color,
        );

        if let Some(buffer) = self
            .arenas
            .rect
            .upload(&self.device, &self.queue, &rect_verts)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Compact Bar Rect Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Compact Bar Rect Pass"),
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
                pass.set_pipeline(&self.pipelines.rect);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, buffer.buffer_slice());
                pass.draw(0..rect_verts.len() as u32, 0..1);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        let text_color = {
            let c = Color::new(menu_fg.0, menu_fg.1, menu_fg.2, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let text_y = band.y() + (band.height() - font_size) / 2.0;
        let mut overlay_glyphs: Vec<(GlyphAtlasHandle, f32, f32, [f32; 4])> = Vec::new();
        for positioned in menu_items {
            let item = positioned.item();
            let bounds = placed_chrome_item_bounds(band, positioned.local_bounds());
            let label_x = bounds.x + padding_x;
            for (ci, ch) in item.label.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: FaceId::new(0),
                    font_size_bits,
                    font_identity: glyph_font_identity(None),
                    x_bin: SubpixelBin::Zero,
                    y_bin: SubpixelBin::Zero,
                };
                if let Some(handle) = glyph_atlas.get_or_create_atlas(
                    &self.device,
                    &self.queue,
                    &key,
                    None,
                    SubpixelRequest::Disabled,
                ) {
                    overlay_glyphs.push((
                        handle,
                        label_x + (ci as f32) * char_width,
                        text_y,
                        text_color,
                    ));
                }
            }
        }
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);

        // --- Pass 3: Tool icons (batched) ---
        let mut icon_batches: Vec<(ImageId, wgpu::BindGroup, Vec<GlyphVertex>)> = Vec::new();
        {
            for positioned in tool_items {
                let item = positioned.item();
                let bounds = placed_chrome_item_bounds(band, positioned.local_bounds());
                if item.is_separator() {
                    continue;
                }
                let icon_x = bounds.x + (bounds.width - icon_sz) / 2.0;
                let icon_y = bounds.y + (bounds.height - icon_sz) / 2.0;
                let alpha = if item.enabled { 1.0 } else { 0.4 };
                let tint = [1.0, 1.0, 1.0, alpha];
                if let Some(image) = item.image.as_ref()
                    && let Some(image_id) = toolbar_texture_id(icon_textures, image, icon_size)
                    && let Some(cached) = self.caches.image.get(image_id)
                {
                    let bg = cached.bind_group.clone();
                    let verts = [
                        GlyphVertex {
                            position: [icon_x, icon_y],
                            tex_coords: [0.0, 0.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x + icon_sz, icon_y],
                            tex_coords: [1.0, 0.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x + icon_sz, icon_y + icon_sz],
                            tex_coords: [1.0, 1.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x, icon_y],
                            tex_coords: [0.0, 0.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x + icon_sz, icon_y + icon_sz],
                            tex_coords: [1.0, 1.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x, icon_y + icon_sz],
                            tex_coords: [0.0, 1.0],
                            color: tint,
                        },
                    ];
                    match icon_batches.last() {
                        Some((prev_id, _, _)) if *prev_id == image_id => {
                            icon_batches.last_mut().unwrap().2.extend_from_slice(&verts);
                        }
                        _ => {
                            icon_batches.push((image_id, bg, verts.to_vec()));
                        }
                    }
                }
            }
        }
        if !icon_batches.is_empty() {
            let mut all_verts: Vec<GlyphVertex> = Vec::new();
            let mut batch_ranges: Vec<(wgpu::BindGroup, std::ops::Range<u32>)> = Vec::new();
            for (_, bg, verts) in icon_batches {
                let start = all_verts.len() as u32;
                all_verts.extend_from_slice(&verts);
                let end = all_verts.len() as u32;
                batch_ranges.push((bg, start..end));
            }
            let icon_upload = self
                .arenas
                .image
                .upload(&self.device, &self.queue, &all_verts);
            if let Some(ref upload) = icon_upload {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Compact Bar Icon Encoder"),
                        });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Compact Bar Icon Pass"),
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
                    pass.set_pipeline(&self.pipelines.image);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.arenas.image.slice(upload));
                    for (bg, range) in &batch_ranges {
                        pass.set_bind_group(1, bg, &[]);
                        pass.draw(range.clone(), 0..1);
                    }
                }
                self.queue.submit(std::iter::once(encoder.finish()));
            }
        }
    }

    /// Render the GPU toolbar overlay at the top of the frame.
    pub fn render_toolbar(
        &mut self,
        view: &wgpu::TextureView,
        items: &[PositionedChromeItem<ToolBarItem>],
        band: FrameRect,
        fg: (f32, f32, f32),
        bg: (f32, f32, f32),
        icon_textures: &HashMap<(ToolBarImageSource, u32), ImageId>,
        hovered: Option<u32>,
        pressed: Option<u32>,
        icon_size: u32,
        padding: u32,
        surface_width: u32,
        surface_height: u32,
    ) {
        self.arenas.image.begin_frame();
        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            time: 0.0,
            _padding: 0.0,
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let bg_color = Color::new(bg.0, bg.1, bg.2, 1.0).srgb_to_linear();
        let icon_sz = icon_size as f32;
        let pad = padding as f32;

        // --- Pass 1: Background bar + item highlights ---
        let mut rect_verts: Vec<RectVertex> = Vec::new();

        // Full toolbar background
        self.add_rect(
            &mut rect_verts,
            band.x(),
            band.y(),
            band.width(),
            band.height(),
            &bg_color,
        );

        // Item backgrounds (hover/pressed states)
        for positioned in items {
            let item = positioned.item();
            let bounds = placed_chrome_item_bounds(band, positioned.local_bounds());
            if item.is_separator() {
                // Draw separator line
                let sep_x = bounds.x + bounds.width / 2.0 - 0.5;
                let sep_y = bounds.y + pad;
                let sep_h = (bounds.height - pad * 2.0).max(0.0);
                let sep_color = Color::new(fg.0, fg.1, fg.2, 0.2).srgb_to_linear();
                self.add_rect(&mut rect_verts, sep_x, sep_y, 1.0, sep_h, &sep_color);
                continue;
            }

            let is_hovered = hovered == Some(item.index);
            let is_pressed = pressed == Some(item.index);

            if is_pressed {
                let c = Color::new(fg.0, fg.1, fg.2, 0.2).srgb_to_linear();
                self.add_rect(
                    &mut rect_verts,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    &c,
                );
            } else if is_hovered && item.enabled {
                let c = Color::new(fg.0, fg.1, fg.2, 0.1).srgb_to_linear();
                self.add_rect(
                    &mut rect_verts,
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    bounds.height,
                    &c,
                );
            }

            if item.selected {
                // Draw selection indicator (bottom accent line)
                let accent = Color::new(0.3, 0.6, 1.0, 0.8).srgb_to_linear();
                self.add_rect(
                    &mut rect_verts,
                    bounds.x,
                    bounds.y + bounds.height - 2.0,
                    bounds.width,
                    2.0,
                    &accent,
                );
            }
        }

        // Bottom border line
        let border_color = Color::new(fg.0, fg.1, fg.2, 0.15).srgb_to_linear();
        self.add_rect(
            &mut rect_verts,
            band.x(),
            band.bottom() - 1.0,
            band.width(),
            1.0,
            &border_color,
        );

        if let Some(buffer) = self
            .arenas
            .rect
            .upload(&self.device, &self.queue, &rect_verts)
        {
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Toolbar Rect Encoder"),
                });
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Toolbar Rect Pass"),
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
                pass.set_pipeline(&self.pipelines.rect);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, buffer.buffer_slice());
                pass.draw(0..rect_verts.len() as u32, 0..1);
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // --- Pass 2: Icon textures (batched) ---
        let mut icon_batches: Vec<(ImageId, wgpu::BindGroup, Vec<GlyphVertex>)> = Vec::new();
        {
            for positioned in items {
                let item = positioned.item();
                let bounds = placed_chrome_item_bounds(band, positioned.local_bounds());
                if item.is_separator() {
                    continue;
                }
                let icon_x = bounds.x + (bounds.width - icon_sz) / 2.0;
                let icon_y = bounds.y + (bounds.height - icon_sz) / 2.0;
                let alpha = if item.enabled { 1.0 } else { 0.4 };
                let tint = [1.0, 1.0, 1.0, alpha];
                if let Some(image) = item.image.as_ref()
                    && let Some(image_id) = toolbar_texture_id(icon_textures, image, icon_size)
                    && let Some(cached) = self.caches.image.get(image_id)
                {
                    let bg = cached.bind_group.clone();
                    let verts = [
                        GlyphVertex {
                            position: [icon_x, icon_y],
                            tex_coords: [0.0, 0.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x + icon_sz, icon_y],
                            tex_coords: [1.0, 0.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x + icon_sz, icon_y + icon_sz],
                            tex_coords: [1.0, 1.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x, icon_y],
                            tex_coords: [0.0, 0.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x + icon_sz, icon_y + icon_sz],
                            tex_coords: [1.0, 1.0],
                            color: tint,
                        },
                        GlyphVertex {
                            position: [icon_x, icon_y + icon_sz],
                            tex_coords: [0.0, 1.0],
                            color: tint,
                        },
                    ];
                    match icon_batches.last() {
                        Some((prev_id, _, _)) if *prev_id == image_id => {
                            icon_batches.last_mut().unwrap().2.extend_from_slice(&verts);
                        }
                        _ => {
                            icon_batches.push((image_id, bg, verts.to_vec()));
                        }
                    }
                }
            }
        }
        if !icon_batches.is_empty() {
            let mut all_verts: Vec<GlyphVertex> = Vec::new();
            let mut batch_ranges: Vec<(wgpu::BindGroup, std::ops::Range<u32>)> = Vec::new();
            for (_, bg, verts) in icon_batches {
                let start = all_verts.len() as u32;
                all_verts.extend_from_slice(&verts);
                let end = all_verts.len() as u32;
                batch_ranges.push((bg, start..end));
            }
            let icon_upload = self
                .arenas
                .image
                .upload(&self.device, &self.queue, &all_verts);
            if let Some(ref upload) = icon_upload {
                let mut encoder =
                    self.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Toolbar Icon Encoder"),
                        });
                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Toolbar Icon Pass"),
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
                    pass.set_pipeline(&self.pipelines.image);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_vertex_buffer(0, self.arenas.image.slice(upload));
                    for (bg, range) in &batch_ranges {
                        pass.set_bind_group(1, bg, &[]);
                        pass.draw(range.clone(), 0..1);
                    }
                }
                self.queue.submit(std::iter::once(encoder.finish()));
            }
        }
    }
}

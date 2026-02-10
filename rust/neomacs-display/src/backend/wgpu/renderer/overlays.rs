//! Overlays methods for WgpuRenderer.

use super::WgpuRenderer;
use super::TitleFadeEntry;
use wgpu::util::DeviceExt;
use super::super::vertex::{GlyphVertex, RectVertex, RoundedRectVertex, Uniforms};
use crate::core::types::{Color, Rect};
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use super::super::glyph_atlas::{GlyphKey, WgpuGlyphAtlas};
use crate::render_thread::PopupMenuState;
use crate::render_thread::TooltipState;

impl WgpuRenderer {
    /// Render floating videos from the scene.
    ///
    /// This renders video frames at fixed screen positions (not inline with text).
    #[cfg(feature = "video")]
    pub fn render_floating_videos(
        &self,
        view: &wgpu::TextureView,
        floating_videos: &[crate::core::scene::FloatingVideo],
    ) {
        use wgpu::util::DeviceExt;

        if floating_videos.is_empty() {
            return;
        }

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Floating Video Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Floating Video Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Don't clear - render on top
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.image_pipeline);
            render_pass.set_bind_group(0, &self.uniform_bind_group, &[]);

            for fv in floating_videos {
                log::debug!("Rendering floating video {} at ({}, {}) size {}x{}",
                           fv.video_id, fv.x, fv.y, fv.width, fv.height);

                if let Some(cached) = self.video_cache.get(fv.video_id) {
                    if let Some(ref bind_group) = cached.bind_group {
                        let vertices = [
                            GlyphVertex { position: [fv.x, fv.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x + fv.width, fv.y], tex_coords: [1.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x + fv.width, fv.y + fv.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x, fv.y], tex_coords: [0.0, 0.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x + fv.width, fv.y + fv.height], tex_coords: [1.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                            GlyphVertex { position: [fv.x, fv.y + fv.height], tex_coords: [0.0, 1.0], color: [1.0, 1.0, 1.0, 1.0] },
                        ];

                        let video_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Floating Video Vertex Buffer"),
                            contents: bytemuck::cast_slice(&vertices),
                            usage: wgpu::BufferUsages::VERTEX,
                        });

                        render_pass.set_bind_group(1, bind_group, &[]);
                        render_pass.set_vertex_buffer(0, video_buffer.slice(..));
                        render_pass.draw(0..6, 0..1);
                    } else {
                        log::debug!("Video {} has no bind_group yet", fv.video_id);
                    }
                } else {
                    log::debug!("Video {} not found in cache", fv.video_id);
                }
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
    }

    /// Render a WebKit view texture at the given bounds.
    ///
    /// This method renders the WebKit view content (from a wgpu texture)
    /// to the screen at the specified rectangle.
    ///
    /// # Arguments
    /// * `_encoder` - The command encoder to use for rendering
    /// * `_view` - The output texture view to render to
    /// * `_webkit_bind_group` - The bind group containing the WebKit texture
    /// * `_bounds` - The rectangle where the WebKit view should be rendered
    #[cfg(feature = "wpe-webkit")]
    pub fn render_webkit_view(
        &mut self,
        _encoder: &mut wgpu::CommandEncoder,
        _view: &wgpu::TextureView,
        _webkit_bind_group: &wgpu::BindGroup,
        _bounds: crate::core::types::Rect,
    ) {
        // TODO: Implement texture rendering
    }

    /// Render a popup menu overlay on top of all content.
    pub(crate) fn render_popup_menu(
        &self,
        view: &wgpu::TextureView,
        menu: &crate::render_thread::PopupMenuState,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
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
        ).srgb_to_linear();
        let hover_color = Color::new(
            bg_r * 0.5 + fg_r * 0.3,
            bg_g * 0.5 + fg_g * 0.3,
            bg_b * 0.5 + fg_b * 0.3,
            0.9,
        ).srgb_to_linear();
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
            ).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let separator_color = Color::new(
            bg_r * 0.7 + fg_r * 0.3,
            bg_g * 0.7 + fg_g * 0.3,
            bg_b * 0.7 + fg_b * 0.3,
            0.8,
        ).srgb_to_linear();
        let title_color = {
            let c = Color::new(
                fg_r * 0.8 + bg_r * 0.2,
                fg_g * 0.8 + bg_g * 0.2,
                fg_b * 0.85 + bg_b * 0.15,
                1.0,
            ).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let shortcut_color = {
            let c = Color::new(
                fg_r * 0.65 + bg_r * 0.35,
                fg_g * 0.65 + bg_g * 0.35,
                fg_b * 0.65 + bg_b * 0.35,
                1.0,
            ).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };

        let padding = 4.0_f32;
        let font_size = glyph_atlas.default_font_size();
        let char_width = font_size * 0.6;
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
                self.add_rect(&mut rect_vertices, mx + offset, my + offset, mw, mh, &shadow);
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
                self.add_rect(&mut rect_vertices, mx + bw, iy, mw - 2.0 * bw, panel.item_height, &hover_color);
            }

            // Separators
            for (i, &item_idx) in panel.item_indices.iter().enumerate() {
                if menu.all_items[item_idx].separator {
                    let iy = my + panel.item_offsets[i] + 3.0;
                    self.add_rect(&mut rect_vertices, mx + 8.0, iy, mw - 16.0, 1.0, &separator_color);
                }
            }

            // Title separator (root panel only)
            if panel_idx == 0 {
                if menu.title.is_some() {
                    let sep_y = my + panel.item_height + 2.0;
                    self.add_rect(&mut rect_vertices, mx + 4.0, sep_y, mw - 8.0, 1.0, &separator_color);
                }
            }

            // Submit rect pass
            if !rect_vertices.is_empty() {
                let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Popup Menu Rect Buffer"),
                    contents: bytemuck::cast_slice(&rect_vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    });
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                    pass.set_vertex_buffer(0, rect_buffer.slice(..));
                    pass.draw(0..rect_vertices.len() as u32, 0..1);
                }
                self.queue.submit(Some(encoder.finish()));
            }

            // === Pass 2: Text glyphs ===
            let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();

            // Title (root panel only)
            if panel_idx == 0 {
                if let Some(ref title) = menu.title {
                    let tx = mx + padding * 2.0;
                    for (ci, ch) in title.chars().enumerate() {
                        let key = GlyphKey {
                            charcode: ch as u32,
                            face_id: 0,
                            font_size_bits,
                        };
                        glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                        overlay_glyphs.push((key, tx + (ci as f32) * char_width, my + padding, title_color));
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
                let color = if !item.enabled { disabled_color } else { text_color };

                let label_x = mx + padding * 2.0;
                for (ci, ch) in item.label.chars().enumerate() {
                    let key = GlyphKey {
                        charcode: ch as u32,
                        face_id: 0,
                        font_size_bits,
                    };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    overlay_glyphs.push((key, label_x + (ci as f32) * char_width, iy + 2.0, color));
                }

                if !item.shortcut.is_empty() {
                    let shortcut_x = mx + mw - padding * 2.0 - (item.shortcut.len() as f32 * char_width);
                    for (ci, ch) in item.shortcut.chars().enumerate() {
                        let key = GlyphKey {
                            charcode: ch as u32,
                            face_id: 0,
                            font_size_bits,
                        };
                        glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                        overlay_glyphs.push((key, shortcut_x + (ci as f32) * char_width, iy + 2.0, shortcut_color));
                    }
                }

                if item.submenu {
                    let arrow_x = mx + mw - padding * 2.0 - char_width;
                    let key = GlyphKey {
                        charcode: '\u{25B8}' as u32,
                        face_id: 0,
                        font_size_bits,
                    };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    overlay_glyphs.push((key, arrow_x, iy + 2.0, text_color));
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
        &self,
        view: &wgpu::TextureView,
        glyphs: &mut Vec<(GlyphKey, f32, f32, [f32; 4])>,
        glyph_atlas: &WgpuGlyphAtlas,
    ) {
        use wgpu::util::DeviceExt;

        if glyphs.is_empty() {
            return;
        }

        // Sort by key for batching consecutive same-texture draws
        glyphs.sort_by(|a, b| {
            a.0.face_id.cmp(&b.0.face_id)
                .then(a.0.font_size_bits.cmp(&b.0.font_size_bits))
                .then(a.0.charcode.cmp(&b.0.charcode))
        });

        // Build vertex buffer for all glyphs at once
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(glyphs.len() * 6);
        let mut valid: Vec<bool> = Vec::with_capacity(glyphs.len());

        for (key, x, y, color) in glyphs.iter() {
            if let Some(cached) = glyph_atlas.get(key) {
                let gw = cached.width as f32;
                let gh = cached.height as f32;
                let gx = *x + cached.bearing_x;
                let gy = *y - cached.bearing_y + 14.0;

                vertices.extend_from_slice(&[
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy], tex_coords: [1.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy + gh], tex_coords: [0.0, 1.0], color: *color },
                ]);
                valid.push(true);
            } else {
                valid.push(false);
            }
        }

        if vertices.is_empty() {
            return;
        }

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Overlay Glyph Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.image_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));

            // Batch draw calls: consecutive same-key glyphs share one bind_group
            let mut vert_idx = 0u32;
            let mut i = 0;
            while i < glyphs.len() {
                if !valid[i] {
                    i += 1;
                    continue;
                }
                let (ref key, _, _, _) = glyphs[i];
                if let Some(cached) = glyph_atlas.get(key) {
                    if cached.is_color {
                        pass.set_pipeline(&self.opaque_image_pipeline);
                    } else {
                        pass.set_pipeline(&self.image_pipeline);
                    }
                    pass.set_bind_group(1, &cached.bind_group, &[]);
                    let batch_start = vert_idx;
                    vert_idx += 6;
                    i += 1;
                    while i < glyphs.len() && valid[i] && glyphs[i].0 == *key {
                        vert_idx += 6;
                        i += 1;
                    }
                    pass.draw(batch_start..vert_idx, 0..1);
                } else {
                    i += 1;
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render watermark text in windows with small/empty buffers.
    pub fn render_window_watermarks(
        &self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
    ) {
        use wgpu::util::DeviceExt;

        if !self.effects.window_watermark.enabled { return; }

        let font_size = glyph_atlas.default_font_size();
        let scale = 3.0_f32;
        let char_width = font_size * 0.6 * scale;
        let char_height = font_size * scale;
        let font_size_bits = 0.0_f32.to_bits();
        let alpha = self.effects.window_watermark.opacity.clamp(0.0, 1.0);

        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4], f32)> = Vec::new();

        for info in &frame_glyphs.window_infos {
            if info.is_minibuffer { continue; }
            if info.buffer_size > self.effects.window_watermark.threshold as i64 { continue; }

            let b = &info.bounds;
            let content_h = b.height - info.mode_line_height;
            if content_h < char_height * 1.5 { continue; }

            // Determine watermark text: use buffer file name basename, or fallback
            let text = if !info.buffer_file_name.is_empty() {
                let name = info.buffer_file_name.rsplit('/').next()
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
                if ch == ' ' { continue; }
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                overlay_glyphs.push((key, start_x + ci as f32 * char_width, start_y, color, scale));
            }
        }

        if overlay_glyphs.is_empty() { return; }

        // Sort by key for batching
        overlay_glyphs.sort_by(|a, b| {
            a.0.face_id.cmp(&b.0.face_id)
                .then(a.0.font_size_bits.cmp(&b.0.font_size_bits))
                .then(a.0.charcode.cmp(&b.0.charcode))
        });

        let sf = self.scale_factor;
        let mut vertices: Vec<GlyphVertex> = Vec::with_capacity(overlay_glyphs.len() * 6);
        let mut valid: Vec<bool> = Vec::with_capacity(overlay_glyphs.len());

        for (key, x, y, color, s) in overlay_glyphs.iter() {
            if let Some(cached) = glyph_atlas.get(key) {
                let gw = cached.width as f32 / sf * s;
                let gh = cached.height as f32 / sf * s;
                let gx = *x + cached.bearing_x / sf * s;
                let gy = *y + (char_height * 0.7) - cached.bearing_y / sf * s;

                vertices.extend_from_slice(&[
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy], tex_coords: [1.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy], tex_coords: [0.0, 0.0], color: *color },
                    GlyphVertex { position: [gx + gw, gy + gh], tex_coords: [1.0, 1.0], color: *color },
                    GlyphVertex { position: [gx, gy + gh], tex_coords: [0.0, 1.0], color: *color },
                ]);
                valid.push(true);
            } else {
                valid.push(false);
            }
        }

        if vertices.is_empty() { return; }

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Watermark Glyph Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.image_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));

            let mut vert_idx = 0u32;
            let mut i = 0;
            while i < overlay_glyphs.len() {
                if !valid[i] {
                    i += 1;
                    continue;
                }
                let (ref key, _, _, _, _) = overlay_glyphs[i];
                if let Some(cached) = glyph_atlas.get(key) {
                    if cached.is_color {
                        pass.set_pipeline(&self.opaque_image_pipeline);
                    } else {
                        pass.set_pipeline(&self.image_pipeline);
                    }
                    pass.set_bind_group(1, &cached.bind_group, &[]);
                    let batch_start = vert_idx;
                    vert_idx += 6;
                    i += 1;
                    while i < overlay_glyphs.len() && valid[i] && overlay_glyphs[i].0 == *key {
                        vert_idx += 6;
                        i += 1;
                    }
                    pass.draw(batch_start..vert_idx, 0..1);
                } else {
                    i += 1;
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render a tooltip overlay on top of the scene.
    pub(crate) fn render_tooltip(
        &self,
        view: &wgpu::TextureView,
        tooltip: &crate::render_thread::TooltipState,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
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
        ).srgb_to_linear();
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
            self.add_rect(&mut rect_vertices,
                          tx + offset, ty + offset, tw, th, &shadow);
        }

        // Background
        self.add_rect(&mut rect_vertices, tx, ty, tw, th, &bg_color);

        // Border (1px)
        let bw = 1.0_f32;
        self.add_rect(&mut rect_vertices, tx, ty, tw, bw, &border_color); // top
        self.add_rect(&mut rect_vertices, tx, ty + th - bw, tw, bw, &border_color); // bottom
        self.add_rect(&mut rect_vertices, tx, ty, bw, th, &border_color); // left
        self.add_rect(&mut rect_vertices, tx + tw - bw, ty, bw, th, &border_color); // right

        if !rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Tooltip Rect Buffer"),
                contents: bytemuck::cast_slice(&rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // === Pass 2: Collect all text glyphs and render batched ===
        let padding = 6.0_f32;
        let line_height = glyph_atlas.default_line_height();
        let char_width = glyph_atlas.default_font_size() * 0.6;
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();

        for (line_idx, line) in tooltip.lines.iter().enumerate() {
            let ly = ty + padding + line_idx as f32 * line_height;
            for (ci, ch) in line.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                overlay_glyphs.push((key, tx + padding + (ci as f32) * char_width, ly, text_color));
            }
        }

        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render a custom title bar overlay for borderless/undecorated windows.
    /// Draws a dark bar at the top with the window title and close/maximize/minimize buttons.
    pub fn render_custom_titlebar(
        &self,
        view: &wgpu::TextureView,
        title: &str,
        titlebar_height: f32,
        hover: u32,
        frame_bg: Option<(f32, f32, f32)>,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
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
        self.add_rect(&mut rect_vertices, 0.0, tb_h - 1.0, logical_w, 1.0, &border_color);

        // Button positions
        let close_x = logical_w - btn_w;
        let max_x = logical_w - btn_w * 2.0;
        let min_x = logical_w - btn_w * 3.0;

        // Button hover highlights
        // hover: 0=none, 2=close, 3=maximize, 4=minimize
        if hover == 2 {
            self.add_rect(&mut rect_vertices, close_x, 0.0, btn_w, tb_h, &close_hover_color);
        } else if hover == 3 {
            self.add_rect(&mut rect_vertices, max_x, 0.0, btn_w, tb_h, &btn_hover_color);
        } else if hover == 4 {
            self.add_rect(&mut rect_vertices, min_x, 0.0, btn_w, tb_h, &btn_hover_color);
        }

        // Subtle button separator lines
        let sep_color = Color::new(0.2, 0.2, 0.22, 0.5).srgb_to_linear();
        self.add_rect(&mut rect_vertices, close_x, 4.0, 1.0, tb_h - 8.0, &sep_color);
        self.add_rect(&mut rect_vertices, max_x, 4.0, 1.0, tb_h - 8.0, &sep_color);
        self.add_rect(&mut rect_vertices, min_x, 4.0, 1.0, tb_h - 8.0, &sep_color);

        // Render rect pass
        if !rect_vertices.is_empty() {
            let rect_buffer =
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Titlebar Rect Buffer"),
                        contents: bytemuck::cast_slice(&rect_vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    });

            let mut encoder =
                self.device
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
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // === Pass 2: Title text and button icons ===
        let font_size = glyph_atlas.default_font_size();
        let char_width = font_size * 0.6;
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();

        // Center title text
        let title_pixel_width = title.chars().count() as f32 * char_width;
        let title_x = (logical_w - title_pixel_width) / 2.0;
        let title_y = (tb_h - font_size) / 2.0;

        for (ci, ch) in title.chars().enumerate() {
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: 0,
                font_size_bits,
            };
            glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
            overlay_glyphs.push((key, title_x + ci as f32 * char_width, title_y, text_color));
        }

        // Button icons: minimize (─), maximize (□), close (×)
        let btn_center_y = (tb_h - font_size) / 2.0;
        let min_color = if hover == 4 { text_color } else { btn_icon_color };
        let max_color = if hover == 3 { text_color } else { btn_icon_color };
        let close_color = if hover == 2 { close_icon_hover } else { btn_icon_color };

        // Minimize: ─ (U+2500)
        let min_icon_x = min_x + (btn_w - char_width) / 2.0;
        let min_key = GlyphKey { charcode: 0x2500, face_id: 0, font_size_bits };
        glyph_atlas.get_or_create(&self.device, &self.queue, &min_key, None);
        overlay_glyphs.push((min_key, min_icon_x, btn_center_y, min_color));

        // Maximize: □ (U+25A1)
        let max_icon_x = max_x + (btn_w - char_width) / 2.0;
        let max_key = GlyphKey { charcode: 0x25A1, face_id: 0, font_size_bits };
        glyph_atlas.get_or_create(&self.device, &self.queue, &max_key, None);
        overlay_glyphs.push((max_key, max_icon_x, btn_center_y, max_color));

        // Close: × (U+00D7)
        let close_icon_x = close_x + (btn_w - char_width) / 2.0;
        let close_key = GlyphKey { charcode: 0x00D7, face_id: 0, font_size_bits };
        glyph_atlas.get_or_create(&self.device, &self.queue, &close_key, None);
        overlay_glyphs.push((close_key, close_icon_x, btn_center_y, close_color));

        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render thin scroll position indicators on the right edge of each window.
    pub fn render_scroll_indicators(
        &self,
        view: &wgpu::TextureView,
        window_infos: &[crate::core::frame_glyphs::WindowInfo],
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
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
                self.add_rect(&mut rect_vertices, b.x, b.y, bw, b.height - info.mode_line_height, &accent);
                // Right
                self.add_rect(&mut rect_vertices, b.x + b.width - bw, b.y, bw, b.height - info.mode_line_height, &accent);
            }

            // Skip windows with no meaningful buffer content for scroll indicator
            if info.buffer_size <= 1 {
                continue;
            }

            let b = &info.bounds;
            // Content area height (exclude mode-line)
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

        if rect_vertices.is_empty() {
            return;
        }

        let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Scroll Indicator Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Render IME preedit text at the cursor position with underline.
    pub fn render_ime_preedit(
        &self,
        view: &wgpu::TextureView,
        preedit_text: &str,
        cursor_x: f32,
        cursor_y: f32,
        cursor_height: f32,
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        if preedit_text.is_empty() {
            return;
        }

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let font_size_bits = 0.0_f32.to_bits();
        let text_len = preedit_text.chars().count();
        let preedit_width = text_len as f32 * char_width;

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
        self.add_rect(&mut rect_vertices, px, py + ph - 2.0, pw, 2.0, &underline_color);

        if !rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("IME Preedit Rect Buffer"),
                contents: bytemuck::cast_slice(&rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
                pass.draw(0..rect_vertices.len() as u32, 0..1);
            }
            self.queue.submit(Some(encoder.finish()));
        }

        // Text glyphs
        let text_color = {
            let c = Color::new(1.0, 1.0, 1.0, 1.0).srgb_to_linear();
            [c.r, c.g, c.b, c.a]
        };
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        for (ci, ch) in preedit_text.chars().enumerate() {
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: 0,
                font_size_bits,
            };
            glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
            overlay_glyphs.push((key, px + 2.0 + (ci as f32) * char_width, py, text_color));
        }
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    /// Render a visual bell flash overlay (semi-transparent white rectangle fading out).
    /// Render an FPS counter overlay in the top-right corner.
    /// Render a corner mask to clip the window to a rounded rectangle.
    /// Uses dst = dst * src_alpha blend mode to zero out pixels outside
    /// the rounded rect shape.
    pub fn render_corner_mask(
        &self,
        view: &wgpu::TextureView,
        corner_radius: f32,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Filled rounded rect covering the whole frame with alpha=1 inside, 0 outside.
        // border_width=0 triggers filled mode in the shader.
        let mut vertices: Vec<RoundedRectVertex> = Vec::new();
        self.add_rounded_rect(
            &mut vertices,
            0.0, 0.0, logical_w, logical_h,
            0.0,            // border_width=0 → filled mode
            corner_radius,
            &Color::new(1.0, 1.0, 1.0, 1.0), // white, alpha=1
        );

        let buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Corner Mask Buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.corner_mask_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Build breadcrumb display chars from a file path
    /// Map a file extension to a distinct HSL-based accent color (linear RGB)
    pub(super) fn extension_to_color(ext: &str) -> (f32, f32, f32) {
        // Well-known extensions get specific colors
        match ext {
            "rs" => (0.8, 0.3, 0.1),  // Rust - orange
            "el" | "lisp" | "scm" => (0.6, 0.2, 0.8),  // Lisp - purple
            "c" | "h" => (0.2, 0.5, 0.8),  // C - blue
            "cpp" | "cc" | "hpp" => (0.2, 0.4, 0.7),  // C++ - darker blue
            "py" => (0.2, 0.6, 0.2),  // Python - green
            "js" | "jsx" => (0.9, 0.8, 0.2),  // JavaScript - yellow
            "ts" | "tsx" => (0.2, 0.5, 0.9),  // TypeScript - blue
            "rb" => (0.8, 0.2, 0.2),  // Ruby - red
            "go" => (0.0, 0.6, 0.7),  // Go - teal
            "java" => (0.7, 0.3, 0.1),  // Java - brown-orange
            "html" | "htm" => (0.9, 0.3, 0.2),  // HTML - red-orange
            "css" | "scss" => (0.2, 0.4, 0.9),  // CSS - blue
            "json" | "yaml" | "yml" | "toml" => (0.5, 0.5, 0.5),  // Config - gray
            "md" | "org" | "txt" => (0.4, 0.7, 0.4),  // Text - green
            "sh" | "bash" | "zsh" => (0.3, 0.7, 0.3),  // Shell - green
            _ => {
                // Hash-based color for unknown extensions
                let mut hash: u32 = 5381;
                for byte in ext.bytes() {
                    hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
                }
                let hue = (hash % 360) as f32 / 360.0;
                // Simple HSL to RGB (saturation=0.6, lightness=0.5)
                let s = 0.6_f32;
                let l = 0.5_f32;
                let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
                let x = c * (1.0 - ((hue * 6.0) % 2.0 - 1.0).abs());
                let m = l - c / 2.0;
                let (r, g, b) = match (hue * 6.0) as i32 {
                    0 => (c, x, 0.0),
                    1 => (x, c, 0.0),
                    2 => (0.0, c, x),
                    3 => (0.0, x, c),
                    4 => (x, 0.0, c),
                    _ => (c, 0.0, x),
                };
                (r + m, g + m, b + m)
            }
        }
    }

    pub(super) fn breadcrumb_display_chars(path: &str) -> Vec<(char, bool)> {
        let separator = " \u{203A} "; // " › "
        let components: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if components.is_empty() {
            return Vec::new();
        }
        let show_start = if components.len() > 3 { components.len() - 3 } else { 0 };
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
        use wgpu::util::DeviceExt;

        if !self.effects.breadcrumb.enabled {
            return;
        }

        let char_width = glyph_atlas.default_font_size() * 0.6;
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
                let wid = info.window_id;
                let new_text = &info.buffer_file_name;
                let changed = match self.prev_breadcrumb_text.get(&wid) {
                    Some(old) => old != new_text,
                    None => false, // first time seeing this window, no fade
                };
                if changed {
                    let old_text = self.prev_breadcrumb_text.get(&wid).cloned().unwrap_or_default();
                    // Remove any existing fade for this window
                    self.active_title_fades.retain(|f| f.window_id != wid);
                    self.active_title_fades.push(TitleFadeEntry {
                        window_id: wid,
                        bounds: info.bounds,
                        old_text,
                        new_text: new_text.clone(),
                        started: std::time::Instant::now(),
                        duration: std::time::Duration::from_millis(self.effects.title_fade.duration_ms as u64),
                    });
                }
                self.prev_breadcrumb_text.insert(wid, new_text.clone());
            }
            // Clean up expired fades
            self.active_title_fades.retain(|f| f.started.elapsed() < f.duration);
            if !self.active_title_fades.is_empty() {
                self.needs_continuous_redraw = true;
            }
        }

        let mut all_rect_vertices: Vec<RectVertex> = Vec::new();
        let mut all_text_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        let font_size_bits = 0.0_f32.to_bits();
        let text_color_base = [0.85_f32, 0.85, 0.85, 1.0];
        let sep_color_base = [0.5_f32, 0.5, 0.5, 1.0];

        for info in &frame_glyphs.window_infos {
            if info.is_minibuffer || info.buffer_file_name.is_empty() {
                continue;
            }

            let b = &info.bounds;

            // Check if this window has an active title fade
            let active_fade = self.active_title_fades.iter().find(|f| f.window_id == info.window_id);

            if let Some(fade) = active_fade {
                // Crossfade: render old text fading out, new text fading in
                let t = (fade.started.elapsed().as_secs_f32() / fade.duration.as_secs_f32()).min(1.0);
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
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y, bar_w, bar_height, &bg_color);
                let edge_color = Color::new(0.3, 0.3, 0.3, opacity * 0.5);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y + bar_height, bar_w, 1.0, &edge_color);

                let text_y = bar_y + 2.0;

                // Old text fading out
                for (ci, &(ch, is_dim)) in display_chars_old.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w { break; }
                    let key = GlyphKey { charcode: ch as u32, face_id: 0, font_size_bits };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    let base = if is_dim { sep_color_base } else { text_color_base };
                    all_text_glyphs.push((key, cx, text_y,
                        [base[0], base[1], base[2], base[3] * old_alpha]));
                }

                // New text fading in
                for (ci, &(ch, is_dim)) in display_chars_new.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w { break; }
                    let key = GlyphKey { charcode: ch as u32, face_id: 0, font_size_bits };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    let base = if is_dim { sep_color_base } else { text_color_base };
                    all_text_glyphs.push((key, cx, text_y,
                        [base[0], base[1], base[2], base[3] * new_alpha]));
                }
            } else {
                // Normal rendering (no active fade)
                let display_chars = Self::breadcrumb_display_chars(&info.buffer_file_name);
                if display_chars.is_empty() { continue; }

                let text_width = display_chars.len() as f32 * char_width;
                let bar_w = (text_width + padding_x * 2.0).min(b.width);
                let bar_x = b.x;
                let bar_y = b.y;

                let bg_color = Color::new(0.0, 0.0, 0.0, opacity);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y, bar_w, bar_height, &bg_color);
                let edge_color = Color::new(0.3, 0.3, 0.3, opacity * 0.5);
                self.add_rect(&mut all_rect_vertices, bar_x, bar_y + bar_height, bar_w, 1.0, &edge_color);

                let text_y = bar_y + 2.0;
                for (ci, &(ch, is_dim)) in display_chars.iter().enumerate() {
                    let cx = bar_x + padding_x + ci as f32 * char_width;
                    if cx + char_width > bar_x + bar_w { break; }
                    let key = GlyphKey { charcode: ch as u32, face_id: 0, font_size_bits };
                    glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                    all_text_glyphs.push((key, cx, text_y,
                        if is_dim { sep_color_base } else { text_color_base }));
                }
            }
        }

        // Draw background rects
        if !all_rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Breadcrumb Rect Buffer"),
                contents: bytemuck::cast_slice(&all_rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
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
        &self,
        view: &wgpu::TextureView,
        frame_glyphs: &FrameGlyphBuffer,
        glyph_atlas: &mut WgpuGlyphAtlas,
        wpm: f32,
    ) {
        use wgpu::util::DeviceExt;

        // Find the selected window (non-minibuffer)
        let selected = frame_glyphs.window_infos.iter().find(|w| w.selected && !w.is_minibuffer);
        let info = match selected {
            Some(i) => i,
            None => return,
        };

        let wpm_int = wpm.round() as u32;
        let label = format!("{} WPM", wpm_int);

        let char_width = glyph_atlas.default_font_size() * 0.6;
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
        let mut text_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        let text_y = bar_y + padding_y;
        for (ci, ch) in label.chars().enumerate() {
            let cx = bar_x + padding_x + ci as f32 * char_width;
            let key = GlyphKey {
                charcode: ch as u32,
                face_id: 0,
                font_size_bits,
            };
            glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
            text_glyphs.push((key, cx, text_y, text_color));
        }

        // Draw background
        if !rect_vertices.is_empty() {
            let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Typing Speed Rect Buffer"),
                contents: bytemuck::cast_slice(&rect_vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.rect_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, rect_buffer.slice(..));
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
        &self,
        view: &wgpu::TextureView,
        lines: &[String],
        glyph_atlas: &mut WgpuGlyphAtlas,
        surface_width: u32,
        surface_height: u32,
    ) {
        use wgpu::util::DeviceExt;

        if lines.is_empty() {
            return;
        }

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        let char_width = glyph_atlas.default_font_size() * 0.6;
        let line_height = glyph_atlas.default_line_height();
        let padding = 4.0_f32;
        let line_spacing = 2.0_f32;

        // Badge size: width = longest line, height = all lines
        let max_text_w = lines.iter()
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

        let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("FPS Rect Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));

        // Text glyphs (green for good visibility)
        let text_color = [0.0_f32, 1.0, 0.0, 1.0]; // green in linear
        let font_size_bits = 0.0_f32.to_bits();
        let mut overlay_glyphs: Vec<(GlyphKey, f32, f32, [f32; 4])> = Vec::new();
        for (li, line) in lines.iter().enumerate() {
            let y = badge_y + padding + li as f32 * (line_height + line_spacing);
            for (ci, ch) in line.chars().enumerate() {
                let key = GlyphKey {
                    charcode: ch as u32,
                    face_id: 0,
                    font_size_bits,
                };
                glyph_atlas.get_or_create(&self.device, &self.queue, &key, None);
                overlay_glyphs.push((
                    key,
                    badge_x + padding + ci as f32 * char_width,
                    y,
                    text_color,
                ));
            }
        }
        self.render_overlay_glyphs(view, &mut overlay_glyphs, glyph_atlas);
    }

    pub fn render_visual_bell(
        &self,
        view: &wgpu::TextureView,
        surface_width: u32,
        surface_height: u32,
        alpha: f32,
    ) {
        use wgpu::util::DeviceExt;

        let logical_w = surface_width as f32 / self.scale_factor;
        let logical_h = surface_height as f32 / self.scale_factor;
        let uniforms = Uniforms {
            screen_size: [logical_w, logical_h],
            _padding: [0.0, 0.0],
        };
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Semi-transparent white overlay in linear space
        let flash_color = Color::new(1.0, 1.0, 1.0, alpha).srgb_to_linear();

        let mut rect_vertices: Vec<RectVertex> = Vec::new();
        self.add_rect(&mut rect_vertices, 0.0, 0.0, logical_w, logical_h, &flash_color);

        let rect_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Visual Bell Buffer"),
            contents: bytemuck::cast_slice(&rect_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.rect_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, rect_buffer.slice(..));
            pass.draw(0..rect_vertices.len() as u32, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
    }
}

//! GSK GPU-accelerated scene renderer.
//!
//! This renderer builds GskRenderNode trees for GPU rendering.
//! Uses cosmic-text for text shaping and rasterization.

use std::collections::HashMap;

use gtk4::prelude::*;
use gtk4::{gdk, gsk, pango, graphene};

use crate::core::scene::{Scene, WindowScene, CursorStyle};
use crate::core::glyph::{GlyphRow, GlyphType, GlyphData};
use crate::core::face::FaceCache;
use crate::core::types::Color;
use crate::text::{TextEngine, GlyphAtlas, GlyphKey};
use super::video::VideoCache;
use super::image::ImageCache;
use crate::backend::webkit::WebKitCache;

/// GPU-accelerated renderer using GSK render nodes.
pub struct GskRenderer {
    /// Pango context for fallback text layout
    pango_context: Option<pango::Context>,

    /// Face cache for text styling
    face_cache: FaceCache,

    /// Cached font descriptions
    font_cache: HashMap<u32, pango::FontDescription>,

    /// Text engine using cosmic-text
    text_engine: TextEngine,

    /// Glyph atlas for caching rasterized glyphs as textures
    glyph_atlas: GlyphAtlas,
}

impl Default for GskRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl GskRenderer {
    pub fn new() -> Self {
        Self {
            pango_context: None,
            face_cache: FaceCache::new(),
            font_cache: HashMap::new(),
            text_engine: TextEngine::new(),
            glyph_atlas: GlyphAtlas::new(),
        }
    }

    /// Initialize with a Pango context
    pub fn init_with_context(&mut self, context: pango::Context) {
        self.pango_context = Some(context);
    }

    /// Get mutable access to face cache
    pub fn face_cache_mut(&mut self) -> &mut FaceCache {
        &mut self.face_cache
    }

    /// Render scene to a GtkSnapshot (for GPU-accelerated rendering)
    pub fn render_to_snapshot(&mut self, snapshot: &gtk4::Snapshot, scene: &Scene) {
        if let Some(node) = self.build_render_node(scene, None, None, None) {
            snapshot.append_node(&node);
        }
    }

    /// Render scene to Cairo context via GSK nodes
    /// This still uses the GSK scene graph but renders through Cairo
    pub fn render_to_cairo(&mut self, cr: &gtk4::cairo::Context, scene: &Scene) {
        if let Some(node) = self.build_render_node(scene, None, None, None) {
            // GSK render nodes can be drawn to a Cairo context
            node.draw(cr);
        }
    }

    /// Render scene to Cairo context with video support
    pub fn render_to_cairo_with_video(
        &mut self,
        cr: &gtk4::cairo::Context,
        scene: &Scene,
        video_cache: &VideoCache,
    ) {
        if let Some(node) = self.build_render_node(scene, Some(video_cache), None, None) {
            node.draw(cr);
        }
    }

    /// Render scene to Cairo context with video and image support
    pub fn render_to_cairo_with_caches(
        &mut self,
        cr: &gtk4::cairo::Context,
        scene: &Scene,
        video_cache: &VideoCache,
        image_cache: &mut ImageCache,
    ) {
        if let Some(node) = self.build_render_node(scene, Some(video_cache), Some(image_cache), None) {
            node.draw(cr);
        }
    }

    /// Render scene to Cairo context with all caches (video, image, webkit)
    pub fn render_to_cairo_with_all_caches(
        &mut self,
        cr: &gtk4::cairo::Context,
        scene: &Scene,
        video_cache: &VideoCache,
        image_cache: &mut ImageCache,
        webkit_cache: &WebKitCache,
    ) {
        if let Some(node) = self.build_render_node(scene, Some(video_cache), Some(image_cache), Some(webkit_cache)) {
            node.draw(cr);
        }
    }

    /// Build a complete GSK render node tree for the scene
    pub fn build_render_node(
        &mut self,
        scene: &Scene,
        video_cache: Option<&VideoCache>,
        mut image_cache: Option<&mut ImageCache>,
        webkit_cache: Option<&WebKitCache>,
    ) -> Option<gsk::RenderNode> {
        let mut nodes: Vec<gsk::RenderNode> = Vec::new();

        // Background color node
        let bg_rect = graphene::Rect::new(0.0, 0.0, scene.width, scene.height);
        let bg_color = color_to_gdk(&scene.background);
        let bg_node = gsk::ColorNode::new(&bg_color, &bg_rect);
        nodes.push(bg_node.upcast());

        // Render each window - use index-based iteration to allow reborrowing image_cache
        let window_count = scene.windows.len();
        for i in 0..window_count {
            let window = &scene.windows[i];
            // Reborrow image_cache for each window
            let img_cache = image_cache.as_deref_mut();
            if let Some(window_node) = self.build_window_node(window, scene, video_cache, img_cache, webkit_cache) {
                nodes.push(window_node);
            }
        }

        // Render floating videos on top
        #[cfg(feature = "video")]
        if let Some(cache) = video_cache {
            for floating in &scene.floating_videos {
                if let Some(player) = cache.get(floating.video_id) {
                    // Prefer using paintable directly (more efficient for gtk4paintablesink)
                    if let Some(paintable) = player.get_paintable() {
                        let pw = paintable.intrinsic_width();
                        let ph = paintable.intrinsic_height();

                        // Skip if paintable has no content yet
                        if pw <= 0 || ph <= 0 {
                            // Draw placeholder
                            let video_rect = graphene::Rect::new(
                                floating.x,
                                floating.y,
                                floating.width,
                                floating.height,
                            );
                            let placeholder_color = gdk::RGBA::new(0.2, 0.2, 0.3, 1.0);
                            let placeholder_node = gsk::ColorNode::new(&placeholder_color, &video_rect);
                            nodes.push(placeholder_node.upcast());
                            continue;
                        }

                        // Try to get current image as a texture for direct rendering
                        let current_image = paintable.current_image();
                        
                        if let Ok(texture) = current_image.clone().downcast::<gdk::Texture>() {
                            // We have a texture, render it directly
                            let video_rect = graphene::Rect::new(
                                floating.x,
                                floating.y,
                                floating.width,
                                floating.height,
                            );
                            let texture_node = gsk::TextureNode::new(&texture, &video_rect);
                            nodes.push(texture_node.upcast());
                        } else {
                            // Snapshot the paintable
                            let snapshot = gtk4::Snapshot::new();
                            snapshot.save();
                            snapshot.translate(&graphene::Point::new(floating.x, floating.y));
                            paintable.snapshot(
                                snapshot.upcast_ref::<gdk::Snapshot>(),
                                floating.width as f64,
                                floating.height as f64,
                            );
                            snapshot.restore();
                            if let Some(node) = snapshot.to_node() {
                                nodes.push(node);
                            }
                        }
                    } else if let Some(texture) = player.get_frame_texture() {
                        // Fallback to texture if paintable not available
                        let video_rect = graphene::Rect::new(
                            floating.x,
                            floating.y,
                            floating.width,
                            floating.height,
                        );
                        let texture_node = gsk::TextureNode::new(&texture, &video_rect);
                        nodes.push(texture_node.upcast());
                    }
                }
            }
        }

        // Render floating images on top
        if let Some(ref mut cache) = image_cache {
            for floating in &scene.floating_images {
                if let Some(img) = cache.get_mut(floating.image_id) {
                    if let Some(texture) = img.get_texture() {
                        let img_rect = graphene::Rect::new(
                            floating.x,
                            floating.y,
                            floating.width,
                            floating.height,
                        );
                        let texture_node = gsk::TextureNode::new(&texture, &img_rect);
                        nodes.push(texture_node.upcast());
                    }
                }
            }
        }

        // Render floating WebKit views on top (highest z-order)
        #[cfg(feature = "wpe-webkit")]
        if let Some(cache) = webkit_cache {
            for floating in &scene.floating_webkits {
                if let Some(view) = cache.get(floating.webkit_id) {
                    if let Some(texture) = view.texture() {
                        let webkit_rect = graphene::Rect::new(
                            floating.x,
                            floating.y,
                            floating.width,
                            floating.height,
                        );
                        let texture_node = gsk::TextureNode::new(texture, &webkit_rect);
                        nodes.push(texture_node.upcast());
                    } else {
                        // Loading placeholder - dark rectangle with loading indicator
                        let webkit_rect = graphene::Rect::new(
                            floating.x,
                            floating.y,
                            floating.width,
                            floating.height,
                        );
                        let placeholder_color = gdk::RGBA::new(0.1, 0.1, 0.15, 1.0);
                        let placeholder_node = gsk::ColorNode::new(&placeholder_color, &webkit_rect);
                        nodes.push(placeholder_node.upcast());
                    }
                }
            }
        }

        // Fallback for floating_webkits when wpe-webkit is not enabled
        #[cfg(not(feature = "wpe-webkit"))]
        {
            for floating in &scene.floating_webkits {
                let webkit_rect = graphene::Rect::new(
                    floating.x,
                    floating.y,
                    floating.width,
                    floating.height,
                );
                let placeholder_color = gdk::RGBA::new(0.2, 0.1, 0.1, 1.0);
                let placeholder_node = gsk::ColorNode::new(&placeholder_color, &webkit_rect);
                nodes.push(placeholder_node.upcast());
            }
        }

        // Combine all nodes into a container
        if nodes.is_empty() {
            None
        } else {
            Some(gsk::ContainerNode::new(&nodes).upcast())
        }
    }

    /// Build render nodes for a window
    fn build_window_node(
        &mut self,
        window: &WindowScene,
        scene: &Scene,
        video_cache: Option<&VideoCache>,
        mut image_cache: Option<&mut ImageCache>,
        webkit_cache: Option<&WebKitCache>,
    ) -> Option<gsk::RenderNode> {
        let mut nodes: Vec<gsk::RenderNode> = Vec::new();

        let x = window.bounds.x;
        let y = window.bounds.y;
        let w = window.bounds.width;
        let h = window.bounds.height;

        // Window background
        let bg_rect = graphene::Rect::new(x, y, w, h);
        let bg_color = color_to_gdk(&window.background);
        let bg_node = gsk::ColorNode::new(&bg_color, &bg_rect);
        nodes.push(bg_node.upcast());

        // Render glyph rows - use index-based iteration to allow reborrowing image_cache
        let row_count = window.rows.len();
        for i in 0..row_count {
            let row = &window.rows[i];
            if row.enabled {
                let img_cache = image_cache.as_deref_mut();
                if let Some(row_nodes) = self.build_row_nodes(row, x, y + row.y as f32, scene, video_cache, img_cache, webkit_cache) {
                    nodes.extend(row_nodes);
                }
            }
        }

        // Render cursor
        if let Some(cursor) = &window.cursor {
            if cursor.visible {
                if let Some(cursor_node) = self.build_cursor_node(
                    x + cursor.x,
                    y + cursor.y,
                    cursor.width,
                    cursor.height,
                    cursor.style,
                    &cursor.color,
                ) {
                    nodes.push(cursor_node);
                }
            }
        }

        // Clip to window bounds
        if nodes.is_empty() {
            None
        } else {
            let container = gsk::ContainerNode::new(&nodes);
            let clip_rect = graphene::Rect::new(x, y, w, h);
            Some(gsk::ClipNode::new(&container, &clip_rect).upcast())
        }
    }

    /// Build render nodes for a row of glyphs using cosmic-text
    fn build_row_nodes(
        &mut self,
        row: &GlyphRow,
        base_x: f32,
        base_y: f32,
        scene: &Scene,
        video_cache: Option<&VideoCache>,
        mut image_cache: Option<&mut ImageCache>,
        webkit_cache: Option<&WebKitCache>,
    ) -> Option<Vec<gsk::RenderNode>> {
        let mut nodes: Vec<gsk::RenderNode> = Vec::new();
        let mut x = base_x;
        let baseline_y = base_y + row.ascent as f32;
        let row_height = row.height as f32;

        // Render each glyph at Emacs's exact position
        for glyph in &row.glyphs {
            // Get face for styling - look in scene first, then fall back to renderer cache
            let face = scene.get_face(glyph.face_id)
                .or_else(|| self.face_cache.get(glyph.face_id))
                .cloned();

            // Draw background if face has one
            if let Some(ref f) = face {
                if f.background.a > 0.01 {
                    let bg_rect = graphene::Rect::new(x, base_y, glyph.pixel_width as f32, row_height);
                    let bg_color = gdk::RGBA::new(f.background.r, f.background.g, f.background.b, f.background.a);
                    let bg_node = gsk::ColorNode::new(&bg_color, &bg_rect);
                    nodes.push(bg_node.upcast());
                }
            }

            match glyph.glyph_type {
                GlyphType::Char => {
                    // Get the character
                    let c = if let GlyphData::Char { code } = glyph.data {
                        code
                    } else if glyph.charcode > 0 {
                        char::from_u32(glyph.charcode).unwrap_or('\u{FFFD}')
                    } else {
                        x += glyph.pixel_width as f32;
                        continue;
                    };

                    // Skip null/control characters and spaces
                    if c == '\0' || c.is_control() || c == ' ' {
                        x += glyph.pixel_width as f32;
                        continue;
                    }

                    // Check glyph atlas cache first
                    let key = GlyphKey {
                        charcode: glyph.charcode,
                        face_id: glyph.face_id,
                    };

                    let texture = if let Some(cached) = self.glyph_atlas.get(&key) {
                        Some((cached.texture.clone(), cached.width, cached.height, cached.bearing_x, cached.bearing_y))
                    } else {
                        // Rasterize the glyph using cosmic-text
                        if let Some((w, h, pixels)) = self.text_engine.rasterize_char(c, face.as_ref()) {
                            if let Some(tex) = TextEngine::create_texture(w, h, &pixels) {
                                // Cache it
                                self.glyph_atlas.insert_texture(key, tex.clone(), w, h, 0, 0);
                                Some((tex, w, h, 0.0, 0.0))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    };

                    // Create texture node at exact position
                    if let Some((tex, w, h, _bearing_x, bearing_y)) = texture {
                        // Position: Emacs gives us x for left edge, baseline_y is the baseline
                        // Glyph should be drawn with top at (baseline_y - ascent + bearing_y)
                        let glyph_y = baseline_y - (h as f32) + (bearing_y as f32);
                        let rect = graphene::Rect::new(x, glyph_y, w as f32, h as f32);
                        let texture_node = gsk::TextureNode::new(&tex, &rect);
                        nodes.push(texture_node.upcast());
                    }

                    // Draw underline if face has one
                    if let Some(ref f) = face {
                        if f.has_underline() {
                            let ul_color = f.get_underline_color();
                            let ul_y = baseline_y + 2.0;
                            let ul_height = match f.underline_style {
                                crate::core::face::UnderlineStyle::Double => 3.0,
                                crate::core::face::UnderlineStyle::Wave => 2.0,
                                _ => 1.0,
                            };
                            let ul_rect = graphene::Rect::new(x, ul_y, glyph.pixel_width as f32, ul_height);
                            let ul_gdk = gdk::RGBA::new(ul_color.r, ul_color.g, ul_color.b, ul_color.a);
                            let ul_node = gsk::ColorNode::new(&ul_gdk, &ul_rect);
                            nodes.push(ul_node.upcast());
                        }

                        // Draw box if face has one
                        if f.box_type != crate::core::face::BoxType::None && f.box_line_width > 0 {
                            let bx_color = f.box_color.unwrap_or(f.foreground);
                            let bx_gdk = gdk::RGBA::new(bx_color.r, bx_color.g, bx_color.b, bx_color.a);
                            let lw = f.box_line_width as f32;

                            // Top border
                            let top_rect = graphene::Rect::new(x, base_y, glyph.pixel_width as f32, lw);
                            nodes.push(gsk::ColorNode::new(&bx_gdk, &top_rect).upcast());

                            // Bottom border
                            let bottom_rect = graphene::Rect::new(x, base_y + row_height - lw, glyph.pixel_width as f32, lw);
                            nodes.push(gsk::ColorNode::new(&bx_gdk, &bottom_rect).upcast());

                            // Left border (only if first char or left_box_line flag)
                            if glyph.left_box_line {
                                let left_rect = graphene::Rect::new(x, base_y, lw, row_height);
                                nodes.push(gsk::ColorNode::new(&bx_gdk, &left_rect).upcast());
                            }

                            // Right border (only if right_box_line flag)
                            if glyph.right_box_line {
                                let right_rect = graphene::Rect::new(x + glyph.pixel_width as f32 - lw, base_y, lw, row_height);
                                nodes.push(gsk::ColorNode::new(&bx_gdk, &right_rect).upcast());
                            }
                        }
                    }

                    // Advance by Emacs's pixel_width (exact positioning)
                    x += glyph.pixel_width as f32;
                }
                GlyphType::Stretch => {
                    // Stretch glyphs are whitespace - just advance x
                    x += glyph.pixel_width as f32;
                }
                GlyphType::Image => {
                    // Render image from cache
                    if let GlyphData::Image { image_id } = glyph.data {
                        let image_rect = graphene::Rect::new(
                            x,
                            base_y,
                            glyph.pixel_width as f32,
                            glyph.ascent as f32,
                        );

                        let mut rendered = false;
                        if let Some(ref mut cache) = image_cache {
                            if let Some(img) = cache.get_mut(image_id) {
                                if let Some(texture) = img.get_texture() {
                                    let texture_node = gsk::TextureNode::new(&texture, &image_rect);
                                    nodes.push(texture_node.upcast());
                                    rendered = true;
                                }
                            }
                        }

                        // Fall back to placeholder if no texture
                        if !rendered {
                            let placeholder_color = gdk::RGBA::new(0.3, 0.3, 0.3, 1.0);
                            let placeholder_node = gsk::ColorNode::new(&placeholder_color, &image_rect);
                            nodes.push(placeholder_node.upcast());
                        }
                    }
                    x += glyph.pixel_width as f32;
                }
                GlyphType::Video => {
                    // Render video frame
                    if let GlyphData::Video { video_id } = glyph.data {
                        let video_rect = graphene::Rect::new(
                            x,
                            base_y,
                            glyph.pixel_width as f32,
                            glyph.ascent as f32,
                        );

                        // Try to get video frame from cache using paintable (more efficient)
                        #[cfg(feature = "video")]
                        let has_frame = if let Some(cache) = video_cache {
                            if let Some(player) = cache.get(video_id) {
                                // Prefer paintable for better performance with gtk4paintablesink
                                if let Some(paintable) = player.get_paintable() {
                                    let pw = paintable.intrinsic_width();
                                    let ph = paintable.intrinsic_height();

                                    // Check if paintable has content (dimensions > 0)
                                    if pw > 0 && ph > 0 {
                                        // Use snapshot to render paintable into a node
                                        let snapshot = gtk4::Snapshot::new();
                                        // Translate to target position first
                                        snapshot.translate(&graphene::Point::new(x, base_y));
                                        // Render the paintable at this position
                                        paintable.snapshot(
                                            snapshot.upcast_ref::<gdk::Snapshot>(),
                                            glyph.pixel_width as f64,
                                            glyph.ascent as f64,
                                        );
                                        if let Some(node) = snapshot.to_node() {
                                            // Clip to video bounds
                                            let clipped = gsk::ClipNode::new(&node, &video_rect);
                                            nodes.push(clipped.upcast());
                                            true
                                        } else {
                                            false
                                        }
                                    } else {
                                        // Paintable has no content yet, fall back to placeholder
                                        false
                                    }
                                } else if let Some(texture) = player.get_frame_texture() {
                                    let texture_node = gsk::TextureNode::new(&texture, &video_rect);
                                    nodes.push(texture_node.upcast());
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        #[cfg(not(feature = "video"))]
                        let has_frame = false;

                        // Fall back to placeholder if no frame
                        if !has_frame {
                            let video_color = gdk::RGBA::new(0.1, 0.2, 0.4, 1.0);
                            let video_node = gsk::ColorNode::new(&video_color, &video_rect);
                            nodes.push(video_node.upcast());

                            let icon_size = (glyph.ascent as f32 * 0.3).min(30.0);
                            let icon_x = x + (glyph.pixel_width as f32 - icon_size) / 2.0;
                            let icon_y = base_y + (glyph.ascent as f32 - icon_size) / 2.0;
                            let icon_rect = graphene::Rect::new(icon_x, icon_y, icon_size, icon_size);
                            let icon_color = gdk::RGBA::new(1.0, 1.0, 1.0, 0.8);
                            let icon_node = gsk::ColorNode::new(&icon_color, &icon_rect);
                            nodes.push(icon_node.upcast());
                        }
                    }
                    x += glyph.pixel_width as f32;
                }
                GlyphType::Wpe => {
                    // Render WebKit view
                    if let GlyphData::Wpe { view_id } = glyph.data {
                        let webkit_rect = graphene::Rect::new(
                            x,
                            base_y,
                            glyph.pixel_width as f32,
                            glyph.ascent as f32,
                        );

                        // Try to get texture from WebKitCache
                        #[cfg(feature = "wpe-webkit")]
                        let has_texture = if let Some(cache) = webkit_cache {
                            if let Some(view) = cache.get(view_id) {
                                if let Some(texture) = view.texture() {
                                    let texture_node = gsk::TextureNode::new(texture, &webkit_rect);
                                    nodes.push(texture_node.upcast());
                                    true
                                } else {
                                    false
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        #[cfg(not(feature = "wpe-webkit"))]
                        let has_texture = false;

                        // Fall back to placeholder if no texture
                        if !has_texture {
                            let webkit_color = gdk::RGBA::new(0.1, 0.3, 0.5, 1.0); // Blue-ish
                            let webkit_node = gsk::ColorNode::new(&webkit_color, &webkit_rect);
                            nodes.push(webkit_node.upcast());

                            // Draw a globe icon placeholder
                            let icon_size = (glyph.ascent as f32 * 0.3).min(30.0);
                            let icon_x = x + (glyph.pixel_width as f32 - icon_size) / 2.0;
                            let icon_y = base_y + (glyph.ascent as f32 - icon_size) / 2.0;
                            let icon_rect = graphene::Rect::new(icon_x, icon_y, icon_size, icon_size);
                            let icon_color = gdk::RGBA::new(1.0, 1.0, 1.0, 0.8);
                            let icon_node = gsk::ColorNode::new(&icon_color, &icon_rect);
                            nodes.push(icon_node.upcast());
                        }
                    }
                    x += glyph.pixel_width as f32;
                }
                _ => {
                    x += glyph.pixel_width as f32;
                }
            }
        }

        // Debug node count for y=0 rows
        if row.y == 0 && !row.glyphs.is_empty() {
            let first_chars: String = row.glyphs.iter()
                .take(10)
                .filter_map(|g| match &g.data {
                    crate::core::glyph::GlyphData::Char { code } => Some(*code),
                    _ => None,
                })
                .collect();
        }

        if nodes.is_empty() {
            None
        } else {
            Some(nodes)
        }
    }

    /// Build a render node for a single character at exact position
    fn build_single_char_node(
        &self,
        context: &pango::Context,
        c: char,
        x: f32,
        baseline_y: f32,
        char_width: f32,
        char_height: f32,
        face_id: u32,
    ) -> Option<Vec<gsk::RenderNode>> {
        let mut nodes: Vec<gsk::RenderNode> = Vec::new();

        let layout = pango::Layout::new(context);
        let text = c.to_string();
        layout.set_text(&text);

        // Get face for styling
        let (fg_color, bg_color, font_desc) = if let Some(face) = self.face_cache.get(face_id) {
            let font_desc_str = face.to_pango_font_description();
            let font_desc = pango::FontDescription::from_string(&font_desc_str);
            layout.set_font_description(Some(&font_desc));
            (
                color_to_gdk(&face.foreground),
                if face.background.a > 0.01 { Some(color_to_gdk(&face.background)) } else { None },
                font_desc,
            )
        } else {
            let font_desc = pango::FontDescription::from_string("Monospace 12");
            layout.set_font_description(Some(&font_desc));
            (
                gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
                None,
                font_desc,
            )
        };

        // Get metrics for background positioning
        let metrics = context.metrics(Some(&font_desc), None);
        let ascent = metrics.ascent() / pango::SCALE;
        let text_y = baseline_y - ascent as f32;

        // Background color node (for selection highlighting)
        if let Some(bg) = bg_color {
            let bg_rect = graphene::Rect::new(x, text_y, char_width, char_height);
            let bg_node = gsk::ColorNode::new(&bg, &bg_rect);
            nodes.push(bg_node.upcast());
        }

        // Text node using GskTextNode
        let mut iter = layout.iter();
        loop {
            if let Some(run) = iter.run_readonly() {
                let item = run.item();
                let analysis = item.analysis();
                let font = analysis.font();
                let glyphs = run.glyph_string();

                // Position at exact x, baseline_y
                let offset = graphene::Point::new(x, baseline_y);

                if let Some(text_node) = gsk::TextNode::new(&font, &glyphs, &fg_color, &offset) {
                    nodes.push(text_node.upcast());
                }
            }

            if !iter.next_run() {
                break;
            }
        }

        if nodes.is_empty() {
            None
        } else {
            Some(nodes)
        }
    }

    /// Build text render nodes (background + foreground text)
    /// Returns (nodes, rendered_width) so caller can position next elements correctly
    fn build_text_nodes(
        &self,
        context: &pango::Context,
        text: &str,
        x: f32,
        baseline_y: f32,
        face_id: u32,
    ) -> Option<(Vec<gsk::RenderNode>, f32)> {
        let mut nodes: Vec<gsk::RenderNode> = Vec::new();

        let layout = pango::Layout::new(context);
        layout.set_text(text);

        // Get face for styling
        let (fg_color, bg_color, font_desc) = if let Some(face) = self.face_cache.get(face_id) {
            let font_desc_str = face.to_pango_font_description();
            let font_desc = pango::FontDescription::from_string(&font_desc_str);
            layout.set_font_description(Some(&font_desc));
            (
                color_to_gdk(&face.foreground),
                if face.background.a > 0.01 { Some(color_to_gdk(&face.background)) } else { None },
                font_desc,
            )
        } else {
            // Default: white text, no background
            let font_desc = pango::FontDescription::from_string("Monospace 12");
            layout.set_font_description(Some(&font_desc));
            (
                gdk::RGBA::new(1.0, 1.0, 1.0, 1.0),
                None,
                font_desc,
            )
        };

        // Get metrics for positioning
        let metrics = context.metrics(Some(&font_desc), None);
        let ascent = metrics.ascent() / pango::SCALE;
        let (width, height) = layout.pixel_size();
        let rendered_width = width as f32;

        // Calculate top-left position (baseline_y - ascent)
        let text_y = baseline_y - ascent as f32;

        // Background color node (if face has background)
        if let Some(bg) = bg_color {
            let bg_rect = graphene::Rect::new(x, text_y, width as f32, height as f32);
            let bg_node = gsk::ColorNode::new(&bg, &bg_rect);
            nodes.push(bg_node.upcast());
        }

        // Text node using GskTextNode
        // Iterate over layout lines and runs to extract glyph information
        let mut iter = layout.iter();
        loop {
            if let Some(run) = iter.run_readonly() {
                // GlyphItem has item() which returns Item
                // Item has analysis() -> Analysis which has font()
                let item = run.item();
                let analysis = item.analysis();
                let font = analysis.font();

                // Get the glyph string from the run
                let glyphs = run.glyph_string();

                // Get run extents to position it
                let (_, logical_rect) = iter.run_extents();
                let run_x = x + (logical_rect.x() / pango::SCALE) as f32;

                // Create text node with offset (at baseline)
                let offset = graphene::Point::new(run_x, baseline_y);

                // GskTextNode::new takes font, glyphs, color, offset
                if let Some(text_node) = gsk::TextNode::new(&font, &glyphs, &fg_color, &offset) {
                    nodes.push(text_node.upcast());
                }
            }

            if !iter.next_run() {
                break;
            }
        }

        if nodes.is_empty() {
            None
        } else {
            Some((nodes, rendered_width))
        }
    }

    /// Build a cursor render node
    fn build_cursor_node(
        &self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        style: CursorStyle,
        color: &Color,
    ) -> Option<gsk::RenderNode> {
        let gdk_color = color_to_gdk(color);

        match style {
            CursorStyle::Box => {
                let rect = graphene::Rect::new(x, y, width, height);
                Some(gsk::ColorNode::new(&gdk_color, &rect).upcast())
            }
            CursorStyle::Bar => {
                let rect = graphene::Rect::new(x, y, 2.0, height);
                Some(gsk::ColorNode::new(&gdk_color, &rect).upcast())
            }
            CursorStyle::Underline => {
                let rect = graphene::Rect::new(x, y + height - 2.0, width, 2.0);
                Some(gsk::ColorNode::new(&gdk_color, &rect).upcast())
            }
            CursorStyle::Hollow => {
                // Hollow cursor needs 4 border rectangles
                let thickness = 1.0;
                let mut nodes = vec![
                    // Top
                    gsk::ColorNode::new(&gdk_color, &graphene::Rect::new(x, y, width, thickness)).upcast(),
                    // Bottom
                    gsk::ColorNode::new(&gdk_color, &graphene::Rect::new(x, y + height - thickness, width, thickness)).upcast(),
                    // Left
                    gsk::ColorNode::new(&gdk_color, &graphene::Rect::new(x, y, thickness, height)).upcast(),
                    // Right
                    gsk::ColorNode::new(&gdk_color, &graphene::Rect::new(x + width - thickness, y, thickness, height)).upcast(),
                ];
                Some(gsk::ContainerNode::new(&nodes).upcast())
            }
        }
    }
}

/// Convert our Color to GDK RGBA
fn color_to_gdk(color: &Color) -> gdk::RGBA {
    gdk::RGBA::new(color.r, color.g, color.b, color.a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gsk_renderer_creation() {
        let renderer = GskRenderer::new();
        assert!(renderer.pango_context.is_none());
    }
}

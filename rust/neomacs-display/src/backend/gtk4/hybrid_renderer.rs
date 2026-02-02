//! Hybrid GSK renderer - renders directly from FrameGlyphBuffer.
//!
//! This bypasses the scene graph and builds GSK nodes directly from
//! the glyph buffer, matching Emacs's immediate-mode redisplay model.
//!
//! Uses cosmic-text for text rendering (pure Rust, no Pango).
//!
//! Enable logging with: RUST_LOG=neomacs_display::backend::gtk4::hybrid_renderer=debug

use gtk4::prelude::*;
use gtk4::{gdk, gsk, graphene};
use log::{debug, trace, warn};

use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::core::types::Color;
use crate::core::face::{Face, FaceCache};
use crate::core::scene::FloatingWebKit;
use crate::text::{TextEngine, GlyphAtlas, GlyphKey, CachedGlyph};
use super::video::VideoCache;
use super::image::ImageCache;
#[cfg(feature = "wpe-webkit")]
use crate::backend::webkit::WebKitCache;

/// Hybrid renderer that builds GSK nodes directly from FrameGlyphBuffer.
/// Uses cosmic-text for text rendering instead of Pango.
pub struct HybridRenderer {
    /// cosmic-text engine for text shaping and rasterization
    text_engine: TextEngine,
    /// Glyph texture atlas for caching
    glyph_atlas: GlyphAtlas,
    /// Face cache for styling
    face_cache: FaceCache,
    /// Display scale factor for HiDPI (1.0 = normal, 2.0 = 2x HiDPI)
    scale_factor: f32,
}

impl Default for HybridRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl HybridRenderer {
    pub fn new() -> Self {
        Self {
            text_engine: TextEngine::new(),
            glyph_atlas: GlyphAtlas::new(),
            face_cache: FaceCache::new(),
            scale_factor: 1.0,
        }
    }

    /// Set the display scale factor for HiDPI rendering
    pub fn set_scale_factor(&mut self, scale: f32) {
        if (self.scale_factor - scale).abs() > 0.01 {
            // Scale changed - clear glyph cache since textures are resolution-dependent
            self.glyph_atlas.clear();
            self.scale_factor = scale;
            debug!("HybridRenderer: scale factor changed to {}", scale);
        }
    }

    /// Get mutable face cache
    pub fn face_cache_mut(&mut self) -> &mut FaceCache {
        &mut self.face_cache
    }

    /// Get or rasterize a glyph, returning a cached texture
    fn get_or_rasterize_glyph(
        &mut self,
        c: char,
        face_id: u32,
        fg: &Color,
        font_family: &str,
        bold: bool,
        italic: bool,
    ) -> Option<&CachedGlyph> {
        let key = GlyphKey {
            charcode: c as u32,
            face_id,
        };

        // Check cache first
        if self.glyph_atlas.contains(&key) {
            return self.glyph_atlas.get(&key);
        }

        warn!("Rasterizing '{}' (face_id={}, fg={:?}, font='{}', bold={}, italic={}, scale={})", c, face_id, fg, font_family, bold, italic, self.scale_factor);

        // Need to rasterize - create a temporary face with the foreground color
        let mut attrs = crate::core::face::FaceAttributes::empty();
        if bold {
            attrs |= crate::core::face::FaceAttributes::BOLD;
        }
        if italic {
            attrs |= crate::core::face::FaceAttributes::ITALIC;
        }

        let face = Face {
            id: face_id,
            foreground: *fg,
            background: Color::TRANSPARENT,
            underline_color: None,
            overline_color: None,
            strike_through_color: None,
            box_color: None,
            font_family: font_family.to_string(),
            font_size: 13.0,
            font_weight: if bold { 700 } else { 400 },
            attributes: attrs,
            underline_style: crate::core::face::UnderlineStyle::None,
            box_type: crate::core::face::BoxType::None,
            box_line_width: 0,
        };

        // Rasterize the character at the current scale factor for HiDPI
        if let Some((width, height, pixels, bearing_x, bearing_y)) =
            self.text_engine.rasterize_char_scaled(c, Some(&face), self.scale_factor)
        {
            warn!("Rasterized '{}': {}x{} bearing=({},{}) pixels_len={} scale={}", c, width, height, bearing_x, bearing_y, pixels.len(), self.scale_factor);
            // Sample some pixel data to verify - find max alpha
            let max_alpha = pixels.chunks(4).map(|c| c[3]).max().unwrap_or(0);
            let non_zero_count = pixels.chunks(4).filter(|c| c[3] > 0).count();
            warn!("  max_alpha={} non_zero_alpha_pixels={}", max_alpha, non_zero_count);
            // Create GPU texture
            if let Some(texture) = TextEngine::create_texture(width, height, &pixels) {
                warn!("Created texture for '{}' size={}x{}", c, texture.width(), texture.height());
                self.glyph_atlas.insert_texture(
                    key.clone(),
                    texture,
                    width,
                    height,
                    bearing_x,
                    bearing_y,
                );
                return self.glyph_atlas.get(&key);
            } else {
                warn!("Failed to create texture for '{}'", c);
            }
        } else {
            warn!("Failed to rasterize '{}'", c);
        }

        None
    }

    /// Build GSK render nodes from FrameGlyphBuffer
    #[cfg(feature = "wpe-webkit")]
    pub fn build_render_node(
        &mut self,
        buffer: &FrameGlyphBuffer,
        mut video_cache: Option<&mut VideoCache>,
        mut image_cache: Option<&mut ImageCache>,
        floating_images: &[crate::core::scene::FloatingImage],
        floating_webkits: &[FloatingWebKit],
        webkit_cache: Option<&WebKitCache>,
    ) -> Option<gsk::RenderNode> {
        self.build_render_node_impl(buffer, video_cache, image_cache, floating_images, floating_webkits, webkit_cache)
    }

    #[cfg(not(feature = "wpe-webkit"))]
    pub fn build_render_node(
        &mut self,
        buffer: &FrameGlyphBuffer,
        mut video_cache: Option<&mut VideoCache>,
        mut image_cache: Option<&mut ImageCache>,
        floating_images: &[crate::core::scene::FloatingImage],
        floating_webkits: &[FloatingWebKit],
        _webkit_cache: Option<()>,
    ) -> Option<gsk::RenderNode> {
        self.build_render_node_impl(buffer, video_cache, image_cache, floating_images, floating_webkits)
    }

    #[cfg(feature = "wpe-webkit")]
    fn build_render_node_impl(
        &mut self,
        buffer: &FrameGlyphBuffer,
        mut video_cache: Option<&mut VideoCache>,
        mut image_cache: Option<&mut ImageCache>,
        floating_images: &[crate::core::scene::FloatingImage],
        floating_webkits: &[FloatingWebKit],
        webkit_cache: Option<&WebKitCache>,
    ) -> Option<gsk::RenderNode> {
        // Update ALL video players FIRST before any rendering
        // This ensures bus polling doesn't happen during the render loop
        #[cfg(feature = "video")]
        if let Some(ref mut cache) = video_cache {
            cache.update_all();
        }
        
        let mut nodes: Vec<gsk::RenderNode> = Vec::with_capacity(buffer.len() + 10);

        // Frame background
        let bg_rect = graphene::Rect::new(0.0, 0.0, buffer.width, buffer.height);
        let bg_color = color_to_gdk(&buffer.background);
        nodes.push(gsk::ColorNode::new(&bg_color, &bg_rect).upcast());
        debug!("Added frame background node");

        // Collect glyph data and partition into regular vs overlay
        let glyphs: Vec<_> = buffer.glyphs.iter().cloned().collect();
        let (regular_glyphs, overlay_glyphs): (Vec<_>, Vec<_>) = glyphs.into_iter().partition(|g| !g.is_overlay());

        // Process backgrounds FIRST (from regular glyphs only)
        let mut bg_count = 0;
        for glyph in &regular_glyphs {
            if let FrameGlyph::Background { bounds, color } = glyph {
                bg_count += 1;
                let rect = graphene::Rect::new(bounds.x, bounds.y, bounds.width, bounds.height);
                let gdk_color = color_to_gdk(color);
                nodes.push(gsk::ColorNode::new(&gdk_color, &rect).upcast());
            }
        }
        debug!("Added {} background(s) FIRST", bg_count);

        // Process regular glyphs (excluding backgrounds, which were handled above)
        let mut char_count = 0;
        for glyph in regular_glyphs {
            self.render_glyph(&glyph, buffer, &mut nodes, &mut video_cache, &mut image_cache, webkit_cache, &mut char_count, false);
        }

        // Process overlay glyphs LAST so they render on top
        for glyph in &overlay_glyphs {
            if let FrameGlyph::Background { bounds, color } = glyph {
                let rect = graphene::Rect::new(bounds.x, bounds.y, bounds.width, bounds.height);
                let gdk_color = color_to_gdk(color);
                nodes.push(gsk::ColorNode::new(&gdk_color, &rect).upcast());
            }
        }
        for glyph in overlay_glyphs {
            self.render_glyph(&glyph, buffer, &mut nodes, &mut video_cache, &mut image_cache, webkit_cache, &mut char_count, true);
        }

        // Render floating images on top
        if let Some(ref mut cache) = image_cache {
            debug!("Rendering {} floating images", floating_images.len());
            for floating in floating_images {
                if let Some(img) = cache.get_mut(floating.image_id) {
                    if let Some(texture) = img.get_texture() {
                        debug!("Got texture for floating image {}", floating.image_id);
                        let img_rect = graphene::Rect::new(
                            floating.x,
                            floating.y,
                            floating.width,
                            floating.height,
                        );
                        let texture_node = gsk::TextureNode::new(&texture, &img_rect);
                        nodes.push(texture_node.upcast());
                    } else {
                        warn!("No texture for floating image {}", floating.image_id);
                    }
                } else {
                    warn!("Floating image {} not in cache", floating.image_id);
                }
            }
        }

        // Render floating webkit views on top (highest z-order)
        if let Some(cache) = webkit_cache {
            debug!("Rendering {} floating webkit views", floating_webkits.len());
            for floating in floating_webkits {
                if let Some(view) = cache.get(floating.webkit_id) {
                    if let Some(texture) = view.texture() {
                        debug!("Got texture for webkit view {}: {}x{}", floating.webkit_id, texture.width(), texture.height());
                        let webkit_rect = graphene::Rect::new(
                            floating.x,
                            floating.y,
                            floating.width,
                            floating.height,
                        );
                        let texture_node = gsk::TextureNode::new(&texture, &webkit_rect);
                        nodes.push(texture_node.upcast());
                    } else {
                        // Loading placeholder - dark rectangle
                        debug!("No texture for webkit view {}, showing placeholder", floating.webkit_id);
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
                } else {
                    warn!("Webkit view {} not in cache", floating.webkit_id);
                }
            }
        }

        debug!("Processed {} chars, {} backgrounds, total {} nodes", char_count, bg_count, nodes.len());

        if nodes.is_empty() {
            debug!("build_render_node: returning None (empty nodes)");
            None
        } else {
            debug!("build_render_node: returning ContainerNode with {} nodes", nodes.len());
            Some(gsk::ContainerNode::new(&nodes).upcast())
        }
    }

    #[cfg(not(feature = "wpe-webkit"))]
    fn build_render_node_impl(
        &mut self,
        buffer: &FrameGlyphBuffer,
        mut video_cache: Option<&mut VideoCache>,
        mut image_cache: Option<&mut ImageCache>,
        floating_images: &[crate::core::scene::FloatingImage],
        _floating_webkits: &[FloatingWebKit],
    ) -> Option<gsk::RenderNode> {
        // Update ALL video players FIRST before any rendering
        #[cfg(feature = "video")]
        if let Some(ref mut cache) = video_cache {
            cache.update_all();
        }
        
        let mut nodes: Vec<gsk::RenderNode> = Vec::with_capacity(buffer.len() + 10);

        // Frame background
        let bg_rect = graphene::Rect::new(0.0, 0.0, buffer.width, buffer.height);
        let bg_color = color_to_gdk(&buffer.background);
        nodes.push(gsk::ColorNode::new(&bg_color, &bg_rect).upcast());

        // Collect glyph data and partition into regular vs overlay
        let glyphs: Vec<_> = buffer.glyphs.iter().cloned().collect();
        let (regular_glyphs, overlay_glyphs): (Vec<_>, Vec<_>) = glyphs.into_iter().partition(|g| !g.is_overlay());

        // First pass: process only backgrounds
        let mut bg_count = 0;
        for glyph in &regular_glyphs {
            if let FrameGlyph::Background { x, y, width, height, color } = glyph {
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                let gdk_color = color_to_gdk(color);
                nodes.push(gsk::ColorNode::new(&gdk_color, &rect).upcast());
                bg_count += 1;
            }
        }

        // Second pass: render non-background glyphs
        let mut char_count = 0;
        for glyph in &regular_glyphs {
            if !matches!(glyph, FrameGlyph::Background { .. }) {
                self.render_glyph(glyph, buffer, &mut nodes, &mut video_cache, &mut image_cache, &mut char_count, false);
            }
        }

        // Process overlay glyphs last
        for glyph in &overlay_glyphs {
            if let FrameGlyph::Background { x, y, width, height, color } = glyph {
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                let gdk_color = color_to_gdk(color);
                nodes.push(gsk::ColorNode::new(&gdk_color, &rect).upcast());
            }
        }
        for glyph in overlay_glyphs {
            self.render_glyph(&glyph, buffer, &mut nodes, &mut video_cache, &mut image_cache, &mut char_count, true);
        }

        // Render floating images
        if let Some(ref mut cache) = image_cache {
            for floating in floating_images {
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

        if nodes.is_empty() {
            None
        } else {
            Some(gsk::ContainerNode::new(&nodes).upcast())
        }
    }

    /// Render a single glyph to the nodes list
    #[cfg(feature = "wpe-webkit")]
    fn render_glyph(
        &mut self,
        glyph: &FrameGlyph,
        buffer: &FrameGlyphBuffer,
        nodes: &mut Vec<gsk::RenderNode>,
        video_cache: &mut Option<&mut VideoCache>,
        image_cache: &mut Option<&mut ImageCache>,
        webkit_cache: Option<&WebKitCache>,
        char_count: &mut usize,
        _is_overlay_pass: bool,
    ) {
        self.render_glyph_inner(glyph, buffer, nodes, video_cache, image_cache, webkit_cache, char_count, _is_overlay_pass)
    }

    #[cfg(not(feature = "wpe-webkit"))]
    fn render_glyph(
        &mut self,
        glyph: &FrameGlyph,
        buffer: &FrameGlyphBuffer,
        nodes: &mut Vec<gsk::RenderNode>,
        video_cache: &mut Option<&mut VideoCache>,
        image_cache: &mut Option<&mut ImageCache>,
        char_count: &mut usize,
        _is_overlay_pass: bool,
    ) {
        self.render_glyph_inner(glyph, buffer, nodes, video_cache, image_cache, char_count, _is_overlay_pass)
    }

    #[cfg(feature = "wpe-webkit")]
    fn render_glyph_inner(
        &mut self,
        glyph: &FrameGlyph,
        buffer: &FrameGlyphBuffer,
        nodes: &mut Vec<gsk::RenderNode>,
        video_cache: &mut Option<&mut VideoCache>,
        image_cache: &mut Option<&mut ImageCache>,
        webkit_cache: Option<&WebKitCache>,
        char_count: &mut usize,
        _is_overlay_pass: bool,
    ) {
        match glyph {
            FrameGlyph::Background { .. } => {
                // Already processed in background pass
            }

            FrameGlyph::Char {
                char,
                x,
                y,
                width,
                height,
                ascent,
                fg,
                bg,
                face_id,
                bold,
                italic,
                ..
            } => {
                *char_count += 1;
                // Draw char background if specified
                if let Some(bg_color) = bg {
                    let rect = graphene::Rect::new(*x, *y, *width, *height);
                    nodes.push(gsk::ColorNode::new(&color_to_gdk(bg_color), &rect).upcast());
                }

                // Skip whitespace - no need to render
                if *char == ' ' || *char == '\t' || *char == '\n' {
                    return;
                }

                // Get font family for this face
                let font_family = buffer.get_face_font(*face_id);

                // Get or rasterize glyph
                let scale = self.scale_factor;
                if let Some(cached) = self.get_or_rasterize_glyph(*char, *face_id, fg, font_family, *bold, *italic) {
                    // Position glyph using bearing (bearing is already in device pixels, divide by scale)
                    let glyph_x = x + cached.bearing_x / scale;
                    let glyph_y = y + ascent - cached.bearing_y / scale;

                    // Texture is in device pixels, but we render at logical size
                    let rect = graphene::Rect::new(
                        glyph_x,
                        glyph_y,
                        cached.width as f32 / scale,
                        cached.height as f32 / scale,
                    );

                    // Create texture node
                    let texture_node = gsk::TextureNode::new(&cached.texture, &rect);
                    nodes.push(texture_node.upcast());
                }
            }

            FrameGlyph::Stretch {
                x,
                y,
                width,
                height,
                bg,
                ..
            } => {
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                nodes.push(gsk::ColorNode::new(&color_to_gdk(bg), &rect).upcast());
            }

            FrameGlyph::Cursor {
                window_id: _,
                x,
                y,
                width,
                height,
                style,
                color,
            } => {
                let cursor_color = color_to_gdk(color);
                match style {
                    0 => {
                        // Box (filled)
                        let rect = graphene::Rect::new(*x, *y, *width, *height);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &rect).upcast());
                    }
                    1 => {
                        // Bar (vertical line)
                        let rect = graphene::Rect::new(*x, *y, 2.0, *height);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &rect).upcast());
                    }
                    2 => {
                        // Underline
                        let rect = graphene::Rect::new(*x, *y + *height - 2.0, *width, 2.0);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &rect).upcast());
                    }
                    3 => {
                        // Hollow box (outline)
                        let thickness = 1.0;
                        let top = graphene::Rect::new(*x, *y, *width, thickness);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &top).upcast());
                        let bottom = graphene::Rect::new(*x, *y + *height - thickness, *width, thickness);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &bottom).upcast());
                        let left = graphene::Rect::new(*x, *y, thickness, *height);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &left).upcast());
                        let right = graphene::Rect::new(*x + *width - thickness, *y, thickness, *height);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &right).upcast());
                    }
                    _ => {}
                }
            }

            FrameGlyph::Border {
                x,
                y,
                width,
                height,
                color,
            } => {
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                nodes.push(gsk::ColorNode::new(&color_to_gdk(color), &rect).upcast());
            }

            FrameGlyph::Image {
                image_id,
                x,
                y,
                width,
                height,
            } => {
                // Render image from cache
                if let Some(ref mut cache) = image_cache {
                    if let Some(img) = cache.get_mut(*image_id) {
                        if let Some(texture) = img.get_texture() {
                            let rect = graphene::Rect::new(*x, *y, *width, *height);
                            let texture_node = gsk::TextureNode::new(&texture, &rect);
                            nodes.push(texture_node.upcast());
                        } else {
                            // No texture yet - render placeholder
                            let rect = graphene::Rect::new(*x, *y, *width, *height);
                            let placeholder = gdk::RGBA::new(0.5, 0.3, 0.3, 1.0);
                            nodes.push(gsk::ColorNode::new(&placeholder, &rect).upcast());
                        }
                    } else {
                        // Image not in cache - render placeholder
                        let rect = graphene::Rect::new(*x, *y, *width, *height);
                        let placeholder = gdk::RGBA::new(0.3, 0.3, 0.4, 1.0);
                        nodes.push(gsk::ColorNode::new(&placeholder, &rect).upcast());
                    }
                } else {
                    // No image cache - render placeholder
                    let rect = graphene::Rect::new(*x, *y, *width, *height);
                    let placeholder = gdk::RGBA::new(0.3, 0.3, 0.4, 1.0);
                    nodes.push(gsk::ColorNode::new(&placeholder, &rect).upcast());
                }
            }

            FrameGlyph::Video {
                video_id,
                x,
                y,
                width,
                height,
            } => {
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                let mut rendered = false;
                
                // Try to render video from cache (update() already called at start of frame)
                if let Some(ref mut cache) = video_cache {
                    if let Some(player) = cache.get_mut(*video_id) {
                        if let Some(paintable) = player.get_paintable() {
                            let pw = paintable.intrinsic_width();
                            let ph = paintable.intrinsic_height();
                            
                            if pw > 0 && ph > 0 {
                                // Calculate dimensions that preserve aspect ratio
                                let video_aspect = pw as f32 / ph as f32;
                                let target_aspect = width / height;
                                
                                let (render_w, render_h, offset_x, offset_y) = if video_aspect > target_aspect {
                                    // Video is wider - fit to width, center vertically
                                    let h = width / video_aspect;
                                    (*width, h, 0.0, (*height - h) / 2.0)
                                } else {
                                    // Video is taller - fit to height, center horizontally
                                    let w = height * video_aspect;
                                    (w, *height, (*width - w) / 2.0, 0.0)
                                };
                                
                                // Use snapshot to render paintable into a node
                                let snapshot = gtk4::Snapshot::new();
                                snapshot.translate(&graphene::Point::new(*x + offset_x, *y + offset_y));
                                paintable.snapshot(
                                    snapshot.upcast_ref::<gdk::Snapshot>(),
                                    render_w as f64,
                                    render_h as f64,
                                );
                                if let Some(node) = snapshot.to_node() {
                                    let clipped = gsk::ClipNode::new(&node, &rect);
                                    nodes.push(clipped.upcast());
                                    rendered = true;
                                    // Count this frame for FPS tracking
                                    player.count_frame();
                                }
                            }
                        }
                    }
                }
                
                // Placeholder if video not available
                if !rendered {
                    let placeholder = gdk::RGBA::new(0.2, 0.2, 0.3, 1.0);
                    nodes.push(gsk::ColorNode::new(&placeholder, &rect).upcast());
                }
            }

            FrameGlyph::WebKit {
                webkit_id,
                x,
                y,
                width,
                height,
            } => {
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                
                // Try to get texture from webkit cache
                if let Some(cache) = webkit_cache {
                    if let Some(view) = cache.get(*webkit_id) {
                        if let Some(texture) = view.texture() {
                            let texture_node = gsk::TextureNode::new(&texture, &rect);
                            nodes.push(texture_node.upcast());
                            return;
                        }
                    }
                }
                
                // Fallback: render placeholder
                let placeholder = gdk::RGBA::new(0.1, 0.1, 0.2, 1.0);
                nodes.push(gsk::ColorNode::new(&placeholder, &rect).upcast());
            }
        }
    }

    #[cfg(not(feature = "wpe-webkit"))]
    fn render_glyph_inner(
        &mut self,
        glyph: &FrameGlyph,
        buffer: &FrameGlyphBuffer,
        nodes: &mut Vec<gsk::RenderNode>,
        video_cache: &mut Option<&mut VideoCache>,
        image_cache: &mut Option<&mut ImageCache>,
        char_count: &mut usize,
        _is_overlay_pass: bool,
    ) {
        match glyph {
            FrameGlyph::Background { .. } => {
                // Already processed in background pass
            }

            FrameGlyph::Char {
                char,
                x,
                y,
                width,
                height,
                ascent,
                fg,
                bg,
                face_id,
                bold,
                italic,
                ..
            } => {
                *char_count += 1;
                if let Some(bg_color) = bg {
                    let rect = graphene::Rect::new(*x, *y, *width, *height);
                    nodes.push(gsk::ColorNode::new(&color_to_gdk(bg_color), &rect).upcast());
                }
                if *char == ' ' || *char == '\t' || *char == '\n' {
                    return;
                }
                // Get font family for this face
                let font_family = buffer.get_face_font(*face_id);
                let scale = self.scale_factor;
                if let Some(cached) = self.get_or_rasterize_glyph(*char, *face_id, fg, font_family, *bold, *italic) {
                    // Scale down from device pixels to logical pixels for rendering
                    let tex_rect = graphene::Rect::new(
                        *x + cached.bearing_x / scale,
                        *y + (*ascent - cached.bearing_y / scale),
                        cached.width as f32 / scale,
                        cached.height as f32 / scale,
                    );
                    let texture_node = gsk::TextureNode::new(&cached.texture, &tex_rect);
                    nodes.push(texture_node.upcast());
                }
            }

            FrameGlyph::Stretch { x, y, width, height, bg, .. } => {
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                nodes.push(gsk::ColorNode::new(&color_to_gdk(bg), &rect).upcast());
            }

            FrameGlyph::Cursor { x, y, width, height, style, color, .. } => {
                let cursor_color = color_to_gdk(color);
                match style {
                    0 => {
                        let rect = graphene::Rect::new(*x, *y, *width, *height);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &rect).upcast());
                    }
                    1 => {
                        let rect = graphene::Rect::new(*x, *y, 2.0, *height);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &rect).upcast());
                    }
                    2 => {
                        let rect = graphene::Rect::new(*x, *y + *height - 2.0, *width, 2.0);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &rect).upcast());
                    }
                    3 => {
                        let thickness = 1.0;
                        let top = graphene::Rect::new(*x, *y, *width, thickness);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &top).upcast());
                        let bottom = graphene::Rect::new(*x, *y + *height - thickness, *width, thickness);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &bottom).upcast());
                        let left = graphene::Rect::new(*x, *y, thickness, *height);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &left).upcast());
                        let right = graphene::Rect::new(*x + *width - thickness, *y, thickness, *height);
                        nodes.push(gsk::ColorNode::new(&cursor_color, &right).upcast());
                    }
                    _ => {}
                }
            }

            FrameGlyph::Border { x, y, width, height, color } => {
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                nodes.push(gsk::ColorNode::new(&color_to_gdk(color), &rect).upcast());
            }

            FrameGlyph::Image { image_id, x, y, width, height } => {
                if let Some(ref mut cache) = image_cache {
                    if let Some(img) = cache.get_mut(*image_id) {
                        if let Some(texture) = img.get_texture() {
                            let rect = graphene::Rect::new(*x, *y, *width, *height);
                            let texture_node = gsk::TextureNode::new(&texture, &rect);
                            nodes.push(texture_node.upcast());
                            return;
                        }
                    }
                }
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                let placeholder = gdk::RGBA::new(0.3, 0.3, 0.4, 1.0);
                nodes.push(gsk::ColorNode::new(&placeholder, &rect).upcast());
            }

            FrameGlyph::Video { video_id, x, y, width, height } => {
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                let mut rendered = false;
                #[cfg(feature = "video")]
                if let Some(ref mut cache) = video_cache {
                    if let Some(player) = cache.get_mut(*video_id) {
                        if let Some(texture) = player.get_texture() {
                            let texture_node = gsk::TextureNode::new(&texture, &rect);
                            nodes.push(texture_node.upcast());
                            rendered = true;
                            player.count_frame();
                        }
                    }
                }
                if !rendered {
                    let placeholder = gdk::RGBA::new(0.2, 0.2, 0.3, 1.0);
                    nodes.push(gsk::ColorNode::new(&placeholder, &rect).upcast());
                }
            }

            FrameGlyph::WebKit { x, y, width, height, .. } => {
                // No webkit support in non-wpe build
                let rect = graphene::Rect::new(*x, *y, *width, *height);
                let placeholder = gdk::RGBA::new(0.1, 0.1, 0.2, 1.0);
                nodes.push(gsk::ColorNode::new(&placeholder, &rect).upcast());
            }
        }
    }
}

/// Convert our Color to GDK RGBA
fn color_to_gdk(color: &Color) -> gdk::RGBA {
    // Color fields are already in 0.0-1.0 range
    gdk::RGBA::new(color.r, color.g, color.b, color.a)
}

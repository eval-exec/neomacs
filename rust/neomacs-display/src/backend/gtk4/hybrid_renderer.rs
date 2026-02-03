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
use log::{debug, info, trace, warn};

use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::core::types::Color;
use crate::core::face::{Face, FaceCache};
use crate::core::scene::FloatingWebKit;
use crate::core::cursor_animation::{CursorAnimator, CursorAnimationMode, Particle, Ring};
use crate::core::buffer_transition::{BufferTransitionAnimator, BufferTransitionEffect, BufferTransition};
use crate::core::animation_config::AnimationConfig;
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
    /// Animation configuration (user settings)
    pub animation_config: AnimationConfig,
    /// Cursor animator for smooth cursor movement and effects
    pub cursor_animator: CursorAnimator,
    /// Buffer transition animator
    pub buffer_transition: BufferTransitionAnimator,
    /// Snapshot texture for buffer transitions
    snapshot_texture: Option<gdk::Texture>,
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
            animation_config: AnimationConfig::default(), // Disabled by default
            cursor_animator: CursorAnimator::new(),
            buffer_transition: BufferTransitionAnimator::new(),
            snapshot_texture: None,
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

    /// Set animation option by name (for Lisp integration)
    pub fn set_animation_option(&mut self, name: &str, value: &str) -> bool {
        let result = self.animation_config.set_option(name, value);
        // Sync config to animators
        self.sync_animation_config();
        result
    }

    /// Sync animation config to individual animators
    fn sync_animation_config(&mut self) {
        // Update cursor animator from config
        if self.animation_config.cursor_animation_active() {
            self.cursor_animator.set_mode(self.animation_config.cursor.mode);
            self.cursor_animator.set_animation_speed(self.animation_config.cursor.speed);
            self.cursor_animator.glow_intensity = if self.animation_config.cursor.glow {
                self.animation_config.cursor.glow_intensity
            } else {
                0.0
            };
            self.cursor_animator.set_particle_count(self.animation_config.cursor.particle_count);
        } else {
            self.cursor_animator.set_mode(CursorAnimationMode::None);
        }
        
        // Update buffer transition from config
        if self.animation_config.buffer_transition_active() {
            self.buffer_transition.set_default_effect(self.animation_config.buffer_transition.effect);
            self.buffer_transition.set_default_duration(self.animation_config.buffer_transition.duration());
            self.buffer_transition.auto_detect = self.animation_config.buffer_transition.auto_detect;
        } else {
            self.buffer_transition.set_default_effect(BufferTransitionEffect::None);
        }
    }

    /// Update animations - call each frame
    /// Returns true if any animation is active (needs redraw)
    pub fn update_animations(&mut self) -> bool {
        if !self.animation_config.enabled {
            return false;
        }
        let cursor_active = if self.animation_config.cursor_animation_active() {
            self.cursor_animator.update()
        } else {
            false
        };
        let transition_active = if self.animation_config.buffer_transition_active() {
            self.buffer_transition.update()
        } else {
            false
        };
        cursor_active || transition_active
    }

    /// Set cursor animation mode
    pub fn set_cursor_animation_mode(&mut self, mode: CursorAnimationMode) {
        self.animation_config.cursor.mode = mode;
        self.cursor_animator.set_mode(mode);
    }

    /// Set buffer transition effect
    pub fn set_buffer_transition_effect(&mut self, effect: BufferTransitionEffect) {
        self.animation_config.buffer_transition.effect = effect;
        self.buffer_transition.set_default_effect(effect);
    }

    /// Capture current frame as snapshot for transitions
    pub fn capture_snapshot(&mut self, snapshot: gdk::Texture) {
        self.snapshot_texture = Some(snapshot);
        self.buffer_transition.has_snapshot = true;
    }

    /// Start a buffer transition (no-arg version)
    pub fn start_buffer_transition_default(&mut self) {
        if self.animation_config.buffer_transition_active() && self.snapshot_texture.is_some() {
            self.buffer_transition.start_transition();
        }
    }

    /// Start a buffer transition with specific effect and duration
    pub fn start_buffer_transition(&mut self, effect_name: &str, duration_ms: u32) {
        let effect = match effect_name {
            "crossfade" => BufferTransitionEffect::Crossfade,
            "slide-left" => BufferTransitionEffect::SlideLeft,
            "slide-right" => BufferTransitionEffect::SlideRight,
            "slide-up" => BufferTransitionEffect::SlideUp,
            "slide-down" => BufferTransitionEffect::SlideDown,
            "scale-fade" => BufferTransitionEffect::ScaleFade,
            "push" => BufferTransitionEffect::Push,
            "blur" => BufferTransitionEffect::Blur,
            "page-curl" => BufferTransitionEffect::PageCurl,
            "none" => BufferTransitionEffect::None,
            _ => self.animation_config.buffer_transition.effect,
        };
        self.buffer_transition.set_default_effect(effect);
        self.buffer_transition.set_default_duration(std::time::Duration::from_millis(duration_ms as u64));
        if self.snapshot_texture.is_some() {
            self.buffer_transition.start_transition();
        }
    }

    /// Check if animations need continuous rendering
    pub fn needs_animation_frame(&self) -> bool {
        if !self.animation_config.enabled {
            return false;
        }
        self.cursor_animator.is_animating() || self.buffer_transition.is_active()
    }

    /// Get animation option value (for Lisp)
    pub fn get_animation_option(&self, name: &str) -> Option<String> {
        self.animation_config.get_option(name)
    }

    /// Update animation state with delta time
    /// Returns true if animation is active
    pub fn update_animation(&mut self, dt: f32) -> bool {
        if !self.animation_config.enabled {
            return false;
        }
        
        let cursor_active = if self.animation_config.cursor_animation_active() {
            self.cursor_animator.update_with_dt(dt)
        } else {
            false
        };
        
        let transition_active = if self.animation_config.buffer_transition_active() {
            self.buffer_transition.update_with_dt(dt)
        } else {
            false
        };
        
        cursor_active || transition_active
    }

    /// Check if any animation is currently active
    pub fn animation_active(&self) -> bool {
        self.needs_animation_frame()
    }

    /// Capture the last rendered frame as a snapshot texture for transitions.
    /// This should be called before switching buffers.
    /// 
    /// renderer: A GSK renderer to render the snapshot
    /// node: The render node to capture (from last frame)
    /// width/height: Dimensions of the capture
    pub fn capture_frame_snapshot(&mut self, renderer: &gsk::Renderer, node: &gsk::RenderNode, width: f32, height: f32) {
        // Create a texture from the render node
        let rect = graphene::Rect::new(0.0, 0.0, width, height);
        
        // Use renderer.render_texture to get a GdkTexture from the node
        // In gsk4 0.9.x, this returns Texture directly, not Option
        let texture = renderer.render_texture(node, Some(&rect));
        debug!("Captured frame snapshot: {}x{}", width, height);
        self.snapshot_texture = Some(texture);
        self.buffer_transition.has_snapshot = true;
    }

    /// Store a pre-rendered texture as the snapshot (for widget-driven capture)
    pub fn set_snapshot_texture(&mut self, texture: gdk::Texture) {
        debug!("Setting snapshot texture: {}x{}", texture.width(), texture.height());
        self.snapshot_texture = Some(texture);
        self.buffer_transition.has_snapshot = true;
    }

    /// Check if we have a snapshot ready for transition
    pub fn has_snapshot(&self) -> bool {
        self.snapshot_texture.is_some()
    }

    /// Clear the snapshot texture (after transition completes)
    pub fn clear_snapshot(&mut self) {
        self.snapshot_texture = None;
        self.buffer_transition.has_snapshot = false;
    }

    /// Prepare for buffer switch by capturing snapshot
    /// Call this BEFORE the buffer content changes
    pub fn prepare_buffer_transition(&mut self) {
        // Mark that we want to capture on next frame
        // The actual capture happens in the widget's snapshot
        if self.animation_config.buffer_transition_active() {
            debug!("Preparing for buffer transition");
            // Snapshot capture will be triggered by widget
        }
    }

    /// Start buffer transition after buffer has changed
    /// Call this AFTER the buffer content changes
    pub fn trigger_buffer_transition(&mut self) {
        if self.animation_config.buffer_transition_active() && self.snapshot_texture.is_some() {
            debug!("Triggering buffer transition");
            self.buffer_transition.start_transition();
        }
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

        debug!("Rasterizing '{}' (face_id={}, fg={:?}, font='{}', bold={}, italic={}, scale={})", c, face_id, fg, font_family, bold, italic, self.scale_factor);

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
            debug!("Rasterized '{}': {}x{} bearing=({},{}) pixels_len={} scale={}", c, width, height, bearing_x, bearing_y, pixels.len(), self.scale_factor);
            // Sample some pixel data to verify - find max alpha
            let max_alpha = pixels.chunks(4).map(|c| c[3]).max().unwrap_or(0);
            let non_zero_count = pixels.chunks(4).filter(|c| c[3] > 0).count();
            debug!("  max_alpha={} non_zero_alpha_pixels={}", max_alpha, non_zero_count);
            // Create GPU texture
            if let Some(texture) = TextEngine::create_texture(width, height, &pixels) {
                debug!("Created texture for '{}' size={}x{}", c, texture.width(), texture.height());
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

        // Render animated cursor (if animation enabled)
        if self.animation_config.cursor_animation_active() {
            self.render_animated_cursor(&mut nodes);
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
                        // Loading placeholder - cyan rectangle
                        debug!("No texture for webkit view {}, showing placeholder", floating.webkit_id);
                        let webkit_rect = graphene::Rect::new(
                            floating.x,
                            floating.y,
                            floating.width,
                            floating.height,
                        );
                        let placeholder_color = gdk::RGBA::new(0.0, 0.8, 0.8, 0.7);  // Semi-transparent cyan
                        let placeholder_node = gsk::ColorNode::new(&placeholder_color, &webkit_rect);
                        nodes.push(placeholder_node.upcast());
                    }
                } else {
                    warn!("Webkit view {} not in cache", floating.webkit_id);
                }
            }
        }

        debug!("Processed {} chars, {} backgrounds, total {} nodes", char_count, bg_count, nodes.len());

        // Build final node, applying buffer transition if active
        let content_node = if nodes.is_empty() {
            debug!("build_render_node: returning None (empty nodes)");
            return None;
        } else {
            gsk::ContainerNode::new(&nodes).upcast()
        };

        // Apply buffer transition effect if active
        if let Some(ref transition) = self.buffer_transition.active_transition {
            if let Some(ref old_texture) = self.snapshot_texture {
                let final_node = self.apply_buffer_transition(&content_node, old_texture, transition, buffer.width, buffer.height);
                return Some(final_node);
            }
        }

        debug!("build_render_node: returning ContainerNode with {} nodes", nodes.len());
        Some(content_node)
    }

    /// Apply buffer transition effect between old snapshot and new content
    fn apply_buffer_transition(
        &self,
        new_content: &gsk::RenderNode,
        old_texture: &gdk::Texture,
        transition: &BufferTransition,
        width: f32,
        height: f32,
    ) -> gsk::RenderNode {
        let progress = transition.eased_progress();
        let rect = graphene::Rect::new(0.0, 0.0, width, height);
        
        // Create node from old snapshot
        let old_node: gsk::RenderNode = gsk::TextureNode::new(old_texture, &rect).upcast();
        
        match transition.effect {
            BufferTransitionEffect::None => {
                new_content.clone()
            }
            
            BufferTransitionEffect::Crossfade => {
                // Simple crossfade using GSK's CrossFadeNode
                gsk::CrossFadeNode::new(&old_node, new_content, progress).upcast()
            }
            
            BufferTransitionEffect::SlideLeft | BufferTransitionEffect::SlideRight => {
                // Slide old out, new in (horizontally)
                let direction = if transition.effect == BufferTransitionEffect::SlideLeft { -1.0 } else { 1.0 };
                let old_dx = progress * width * direction;
                let new_dx = (1.0 - progress) * width * (-direction);
                
                let old_transform = gsk::Transform::new().translate(&graphene::Point::new(old_dx, 0.0));
                let new_transform = gsk::Transform::new().translate(&graphene::Point::new(new_dx, 0.0));
                
                let old_transformed = gsk::TransformNode::new(&old_node, &old_transform);
                let new_transformed = gsk::TransformNode::new(new_content, &new_transform);
                
                gsk::ContainerNode::new(&[
                    old_transformed.upcast(),
                    new_transformed.upcast(),
                ]).upcast()
            }
            
            BufferTransitionEffect::SlideUp | BufferTransitionEffect::SlideDown => {
                let direction = if transition.effect == BufferTransitionEffect::SlideUp { -1.0 } else { 1.0 };
                let old_dy = progress * height * direction;
                let new_dy = (1.0 - progress) * height * (-direction);
                
                let old_transform = gsk::Transform::new().translate(&graphene::Point::new(0.0, old_dy));
                let new_transform = gsk::Transform::new().translate(&graphene::Point::new(0.0, new_dy));
                
                let old_transformed = gsk::TransformNode::new(&old_node, &old_transform);
                let new_transformed = gsk::TransformNode::new(new_content, &new_transform);
                
                gsk::ContainerNode::new(&[
                    old_transformed.upcast(),
                    new_transformed.upcast(),
                ]).upcast()
            }
            
            BufferTransitionEffect::ScaleFade => {
                // Scale and fade
                let old_scale = transition.scale_old();
                let new_scale = transition.scale_new();
                let old_opacity = transition.crossfade_old_opacity();
                let new_opacity = transition.crossfade_new_opacity();
                
                // Center point for scaling
                let cx = width / 2.0;
                let cy = height / 2.0;
                
                let old_transform = gsk::Transform::new()
                    .translate(&graphene::Point::new(cx, cy))
                    .scale(old_scale, old_scale)
                    .translate(&graphene::Point::new(-cx, -cy));
                let new_transform = gsk::Transform::new()
                    .translate(&graphene::Point::new(cx, cy))
                    .scale(new_scale, new_scale)
                    .translate(&graphene::Point::new(-cx, -cy));
                
                let old_transformed = gsk::TransformNode::new(&old_node, &old_transform);
                let new_transformed = gsk::TransformNode::new(new_content, &new_transform);
                
                let old_faded = gsk::OpacityNode::new(&old_transformed.upcast(), old_opacity);
                let new_faded = gsk::OpacityNode::new(&new_transformed.upcast(), new_opacity);
                
                gsk::ContainerNode::new(&[
                    old_faded.upcast(),
                    new_faded.upcast(),
                ]).upcast()
            }
            
            BufferTransitionEffect::Push => {
                // New pushes over old (old stays, new slides in)
                let (new_dx, _) = transition.slide_new_offset();
                let new_transform = gsk::Transform::new().translate(&graphene::Point::new(new_dx, 0.0));
                let new_transformed = gsk::TransformNode::new(new_content, &new_transform);
                
                // Add shadow on the new content edge
                let shadow_opacity = (1.0 - progress) * 0.3;
                let shadow_rect = graphene::Rect::new(new_dx - 20.0, 0.0, 20.0, height);
                let shadow_color = gdk::RGBA::new(0.0, 0.0, 0.0, shadow_opacity);
                let shadow_node = gsk::ColorNode::new(&shadow_color, &shadow_rect);
                
                gsk::ContainerNode::new(&[
                    old_node,
                    shadow_node.upcast(),
                    new_transformed.upcast(),
                ]).upcast()
            }
            
            BufferTransitionEffect::Blur => {
                // Blur transition - old blurs out, new blurs in
                let old_blur = transition.blur_old_radius();
                let new_blur = transition.blur_new_radius();
                let old_opacity = transition.crossfade_old_opacity();
                let new_opacity = transition.crossfade_new_opacity();
                
                let old_blurred: gsk::RenderNode = if old_blur > 0.5 {
                    gsk::BlurNode::new(&old_node, old_blur).upcast()
                } else {
                    old_node
                };
                
                let new_blurred: gsk::RenderNode = if new_blur > 0.5 {
                    gsk::BlurNode::new(new_content, new_blur).upcast()
                } else {
                    new_content.clone()
                };
                
                let old_faded = gsk::OpacityNode::new(&old_blurred, old_opacity);
                let new_faded = gsk::OpacityNode::new(&new_blurred, new_opacity);
                
                gsk::ContainerNode::new(&[
                    old_faded.upcast(),
                    new_faded.upcast(),
                ]).upcast()
            }
            
            BufferTransitionEffect::PageCurl => {
                // Page curl effect - approximate with transform
                // Full page curl requires custom shader, this is a simplified 3D-ish version
                let (curl_progress, curl_angle, shadow_opacity) = transition.page_curl_params();
                
                // Create a pseudo-3D effect using perspective transform
                // The old page rotates around the left edge
                let cx = 0.0; // Rotate around left edge (spine)
                let cy = height / 2.0;
                
                // Approximate curl with Y-axis rotation via scale + skew
                let _scale_x = (1.0 - curl_progress).max(0.01);
                let _skew_amount = curl_progress * 0.3;
                
                // Old page curling away - translate_3d takes Point3D
                let old_transform = gsk::Transform::new()
                    .perspective(1000.0)
                    .translate(&graphene::Point::new(cx, cy))  // Simplified to 2D translate
                    .rotate_3d(curl_angle.to_degrees() * 0.5, &graphene::Vec3::new(0.0, 1.0, 0.0))
                    .translate(&graphene::Point::new(-cx, -cy));
                
                let old_transformed: gsk::RenderNode = gsk::TransformNode::new(&old_node, &old_transform).upcast();
                
                // Darken old page as it turns (simulating back side)
                let old_darkened: gsk::RenderNode = if curl_progress > 0.5 {
                    let darken = (curl_progress - 0.5) * 0.4;
                    gsk::OpacityNode::new(&old_transformed, 1.0 - darken).upcast()
                } else {
                    old_transformed
                };
                
                // Shadow under the curling page
                let shadow_width = width * curl_progress * 0.3;
                let shadow_rect = graphene::Rect::new(width * (1.0 - curl_progress) - shadow_width, 0.0, shadow_width, height);
                let shadow_color = gdk::RGBA::new(0.0, 0.0, 0.0, shadow_opacity);
                let shadow_node = gsk::ColorNode::new(&shadow_color, &shadow_rect);
                
                gsk::ContainerNode::new(&[
                    new_content.clone(),
                    shadow_node.upcast(),
                    old_darkened,
                ]).upcast()
            }
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

        // Render animated cursor (if animation enabled)
        if self.animation_config.cursor_animation_active() {
            self.render_animated_cursor(&mut nodes);
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
                // If animation is enabled, update animator and skip direct render
                if self.animation_config.cursor_animation_active() {
                    self.cursor_animator.set_target(
                        *x, *y, *width, *height, *style,
                        [color.r, color.g, color.b, color.a],
                    );
                    // Cursor will be rendered via render_animated_cursor() at the end
                } else {
                    // No animation - render cursor directly
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
                // If animation is enabled, update animator and skip direct render
                if self.animation_config.cursor_animation_active() {
                    self.cursor_animator.set_target(
                        *x, *y, *width, *height, *style,
                        [color.r, color.g, color.b, color.a],
                    );
                    // Cursor will be rendered via render_animated_cursor() at the end
                } else {
                    // No animation - render cursor directly
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

    /// Render the animated cursor with effects
    /// Call this after rendering all glyphs, passing the cursor target from glyphs
    pub fn render_animated_cursor(&mut self, nodes: &mut Vec<gsk::RenderNode>) {
        use std::time::Instant;
        let now = Instant::now();
        
        // Don't render if cursor is not visible (blink off)
        if !self.cursor_animator.is_visible() {
            // Still render particles even when cursor blinks off
            self.render_cursor_particles(nodes, now);
            return;
        }
        
        // Get animated cursor position and properties
        let x = self.cursor_animator.current_x;
        let y = self.cursor_animator.current_y;
        let width = self.cursor_animator.current_width;
        let height = self.cursor_animator.current_height;
        let style = self.cursor_animator.style;
        let color = self.cursor_animator.color;
        
        let cursor_color = gdk::RGBA::new(color[0], color[1], color[2], color[3]);
        
        // Render cursor glow effect (if enabled)
        if self.cursor_animator.glow_intensity > 0.0 {
            let glow_expand = 4.0;
            let glow_rect = graphene::Rect::new(
                x - glow_expand,
                y - glow_expand,
                width + glow_expand * 2.0,
                height + glow_expand * 2.0,
            );
            let glow_color = gdk::RGBA::new(
                color[0], color[1], color[2],
                color[3] * self.cursor_animator.glow_intensity * 0.5,
            );
            nodes.push(gsk::ColorNode::new(&glow_color, &glow_rect).upcast());
        }
        
        // Render the cursor itself
        match style {
            0 => {
                // Box (filled)
                let rect = graphene::Rect::new(x, y, width, height);
                nodes.push(gsk::ColorNode::new(&cursor_color, &rect).upcast());
            }
            1 => {
                // Bar (vertical line)
                let rect = graphene::Rect::new(x, y, 2.0, height);
                nodes.push(gsk::ColorNode::new(&cursor_color, &rect).upcast());
            }
            2 => {
                // Underline
                let rect = graphene::Rect::new(x, y + height - 2.0, width, 2.0);
                nodes.push(gsk::ColorNode::new(&cursor_color, &rect).upcast());
            }
            3 => {
                // Hollow box (outline)
                let thickness = 1.0;
                let top = graphene::Rect::new(x, y, width, thickness);
                nodes.push(gsk::ColorNode::new(&cursor_color, &top).upcast());
                let bottom = graphene::Rect::new(x, y + height - thickness, width, thickness);
                nodes.push(gsk::ColorNode::new(&cursor_color, &bottom).upcast());
                let left = graphene::Rect::new(x, y, thickness, height);
                nodes.push(gsk::ColorNode::new(&cursor_color, &left).upcast());
                let right = graphene::Rect::new(x + width - thickness, y, thickness, height);
                nodes.push(gsk::ColorNode::new(&cursor_color, &right).upcast());
            }
            _ => {}
        }
        
        // Render particle effects
        self.render_cursor_particles(nodes, now);
    }
    
    /// Render cursor particles (trails, sparkles, etc.)
    fn render_cursor_particles(&self, nodes: &mut Vec<gsk::RenderNode>, now: std::time::Instant) {
        // Render particles
        for particle in &self.cursor_animator.particles {
            let opacity = particle.opacity(now);
            let size = particle.current_size(now);
            
            if opacity > 0.01 && size > 0.5 {
                let color = gdk::RGBA::new(
                    particle.color[0],
                    particle.color[1],
                    particle.color[2],
                    particle.color[3] * opacity,
                );
                let rect = graphene::Rect::new(
                    particle.x - size / 2.0,
                    particle.y - size / 2.0,
                    size,
                    size,
                );
                nodes.push(gsk::ColorNode::new(&color, &rect).upcast());
            }
        }
        
        // Render rings (sonicboom, ripple)
        for ring in &self.cursor_animator.rings {
            let opacity = ring.opacity(now);
            
            if opacity > 0.01 {
                let color = gdk::RGBA::new(
                    ring.color[0],
                    ring.color[1],
                    ring.color[2],
                    ring.color[3] * opacity,
                );
                
                // Render ring as 4 arcs (approximated with rectangles for now)
                // Top
                let top = graphene::Rect::new(
                    ring.x - ring.radius,
                    ring.y - ring.radius,
                    ring.radius * 2.0,
                    ring.thickness,
                );
                nodes.push(gsk::ColorNode::new(&color, &top).upcast());
                // Bottom
                let bottom = graphene::Rect::new(
                    ring.x - ring.radius,
                    ring.y + ring.radius - ring.thickness,
                    ring.radius * 2.0,
                    ring.thickness,
                );
                nodes.push(gsk::ColorNode::new(&color, &bottom).upcast());
                // Left
                let left = graphene::Rect::new(
                    ring.x - ring.radius,
                    ring.y - ring.radius,
                    ring.thickness,
                    ring.radius * 2.0,
                );
                nodes.push(gsk::ColorNode::new(&color, &left).upcast());
                // Right
                let right = graphene::Rect::new(
                    ring.x + ring.radius - ring.thickness,
                    ring.y - ring.radius,
                    ring.thickness,
                    ring.radius * 2.0,
                );
                nodes.push(gsk::ColorNode::new(&color, &right).upcast());
            }
        }
        
        // Render torpedo trail
        if !self.cursor_animator.trail.is_empty() {
            let trail_lifetime = std::time::Duration::from_millis(200);
            let color = &self.cursor_animator.color;
            
            for (i, point) in self.cursor_animator.trail.iter().enumerate() {
                let age = now.duration_since(point.time).as_secs_f32();
                let max_age = trail_lifetime.as_secs_f32();
                let opacity = (1.0 - age / max_age).max(0.0).powi(2);
                let size = 3.0 * (1.0 - age / max_age).max(0.1);
                
                if opacity > 0.01 {
                    let trail_color = gdk::RGBA::new(color[0], color[1], color[2], color[3] * opacity * 0.7);
                    let rect = graphene::Rect::new(
                        point.x - size / 2.0,
                        point.y - size / 2.0,
                        size,
                        size,
                    );
                    nodes.push(gsk::ColorNode::new(&trail_color, &rect).upcast());
                }
            }
        }
    }
    
    /// Update cursor target from a cursor glyph
    pub fn set_cursor_target(&mut self, x: f32, y: f32, width: f32, height: f32, style: u8, color: &Color) {
        self.cursor_animator.set_target(
            x, y, width, height, style,
            [color.r, color.g, color.b, color.a],
        );
    }
}

/// Convert our Color to GDK RGBA
fn color_to_gdk(color: &Color) -> gdk::RGBA {
    // Color fields are already in 0.0-1.0 range
    gdk::RGBA::new(color.r, color.g, color.b, color.a)
}

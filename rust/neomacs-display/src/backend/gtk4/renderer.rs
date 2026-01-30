//! GTK4 scene renderer using Cairo (compatible with DrawingArea).
//!
//! Note: GTK4's DrawingArea still uses Cairo contexts. For full GPU acceleration,
//! we would need to use GtkSnapshot with custom widgets, but Cairo provides
//! good performance for text-heavy workloads like Emacs.

use std::collections::HashMap;

use gtk4::prelude::*;
use gtk4::{gdk, pango, cairo};

use crate::core::scene::{Scene, WindowScene, Node, NodeKind, CursorStyle};
use crate::core::glyph::{Glyph, GlyphRow, GlyphType, GlyphData};
use crate::core::face::{Face, FaceCache};
use crate::core::types::{Color, Rect};
use super::image::ImageCache;
use super::video::VideoCache;

/// Renderer that converts our scene graph to Cairo drawing commands.
pub struct Gtk4Renderer {
    /// Pango context for text layout
    pango_context: Option<pango::Context>,
    
    /// Face cache for text styling
    face_cache: FaceCache,
    
    /// Cached Pango layouts for performance
    layout_cache: HashMap<u32, pango::Layout>,
    
    /// Image cache
    image_cache: ImageCache,
    
    /// Video cache
    video_cache: VideoCache,
}

impl Default for Gtk4Renderer {
    fn default() -> Self {
        Self::new()
    }
}

impl Gtk4Renderer {
    pub fn new() -> Self {
        Self {
            pango_context: None,
            face_cache: FaceCache::new(),
            layout_cache: HashMap::new(),
            image_cache: ImageCache::new(),
            video_cache: VideoCache::new(),
        }
    }
    
    /// Get or create a face by ID
    pub fn get_or_create_face(&mut self, face_id: u32) -> &Face {
        self.face_cache.get_or_create(face_id)
    }
    
    /// Register a face
    pub fn register_face(&mut self, face: Face) -> u32 {
        self.face_cache.insert(face)
    }

    /// Initialize renderer with a Pango context from a widget
    pub fn init_with_context(&mut self, context: pango::Context) {
        self.pango_context = Some(context);
    }

    /// Render a scene to a Cairo context
    pub fn render(&self, cr: &cairo::Context, scene: &Scene) {
        // Background
        self.render_color_rect(cr, 0.0, 0.0, scene.width, scene.height, &scene.background);

        // Render each window
        for window in &scene.windows {
            self.render_window(cr, window);
        }
    }

    /// Render a window scene
    fn render_window(&self, cr: &cairo::Context, window: &WindowScene) {
        // Save state
        cr.save().ok();

        // Translate to window position
        cr.translate(window.bounds.x as f64, window.bounds.y as f64);

        // Clip to window bounds
        cr.rectangle(0.0, 0.0, window.bounds.width as f64, window.bounds.height as f64);
        cr.clip();

        // Apply scroll offset
        if window.scroll_offset != 0.0 {
            cr.translate(0.0, -window.scroll_offset as f64);
        }

        // Window background
        self.render_color_rect(cr, 0.0, 0.0, window.bounds.width, window.bounds.height, &window.background);

        // Render glyph rows
        for row in &window.rows {
            if row.enabled {
                self.render_glyph_row(cr, row);
            }
        }

        // Render cursor if visible
        if let Some(cursor) = &window.cursor {
            if cursor.visible {
                self.render_cursor(
                    cr,
                    cursor.x,
                    cursor.y,
                    cursor.width,
                    cursor.height,
                    cursor.style,
                    &cursor.color,
                );
            }
        }

        // Draw a sample text to show it works
        if let Some(context) = &self.pango_context {
            self.render_sample_text(cr, context, window);
        }

        cr.restore().ok();
    }

    /// Render sample text to demonstrate text rendering
    fn render_sample_text(&self, cr: &cairo::Context, context: &pango::Context, window: &WindowScene) {
        let layout = pango::Layout::new(context);
        
        // Set font
        let font_desc = pango::FontDescription::from_string("Monospace 14");
        layout.set_font_description(Some(&font_desc));
        
        // Set text
        let text = if window.selected {
            "Welcome to Neomacs Display Engine\n\nThis is a GPU-accelerated display engine\nwritten in Rust using GTK4.\n\n;; Press C-x C-c to quit"
        } else {
            "[Minibuffer]"
        };
        layout.set_text(text);

        // Set color (light text)
        cr.set_source_rgb(0.9, 0.9, 0.85);
        
        // Position and render
        cr.move_to(10.0, 10.0);
        pangocairo::functions::show_layout(cr, &layout);
    }

    /// Render a solid color rectangle
    fn render_color_rect(&self, cr: &cairo::Context, x: f32, y: f32, width: f32, height: f32, color: &Color) {
        cr.set_source_rgba(color.r as f64, color.g as f64, color.b as f64, color.a as f64);
        cr.rectangle(x as f64, y as f64, width as f64, height as f64);
        cr.fill().ok();
    }

    /// Render text using Pango
    fn render_text(&self, cr: &cairo::Context, text: &str, x: f32, y: f32) {
        let Some(context) = &self.pango_context else {
            return;
        };

        let layout = pango::Layout::new(context);
        layout.set_text(text);

        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.move_to(x as f64, y as f64);
        pangocairo::functions::show_layout(cr, &layout);
    }

    /// Render cursor
    fn render_cursor(
        &self,
        cr: &cairo::Context,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        style: CursorStyle,
        color: &Color,
    ) {
        cr.set_source_rgba(color.r as f64, color.g as f64, color.b as f64, color.a as f64);

        match style {
            CursorStyle::Box => {
                cr.rectangle(x as f64, y as f64, width as f64, height as f64);
                cr.fill().ok();
            }
            CursorStyle::Bar => {
                cr.rectangle(x as f64, y as f64, 2.0, height as f64);
                cr.fill().ok();
            }
            CursorStyle::Underline => {
                cr.rectangle(x as f64, (y + height - 2.0) as f64, width as f64, 2.0);
                cr.fill().ok();
            }
            CursorStyle::Hollow => {
                cr.set_line_width(1.0);
                cr.rectangle(x as f64 + 0.5, y as f64 + 0.5, (width - 1.0) as f64, (height - 1.0) as f64);
                cr.stroke().ok();
            }
        }
    }

    /// Render a row of glyphs
    fn render_glyph_row(&self, cr: &cairo::Context, row: &GlyphRow) {
        let Some(context) = &self.pango_context else {
            return;
        };

        let mut x = 0.0f64;
        let y = row.y as f64;
        let baseline_y = y + row.ascent as f64;

        // Group consecutive glyphs with same face for efficient rendering
        let mut current_face_id: Option<u32> = None;
        let mut text_buffer = String::new();
        let mut text_start_x = 0.0f64;

        for glyph in &row.glyphs {
            match glyph.glyph_type {
                GlyphType::Char => {
                    // Check if we need to flush the current text run
                    if current_face_id != Some(glyph.face_id) {
                        // Flush previous text
                        if !text_buffer.is_empty() {
                            self.render_text_run(
                                cr,
                                context,
                                &text_buffer,
                                text_start_x,
                                baseline_y,
                                current_face_id.unwrap_or(0),
                            );
                            text_buffer.clear();
                        }
                        current_face_id = Some(glyph.face_id);
                        text_start_x = x;
                    }

                    // Add character to buffer
                    if let GlyphData::Char { code } = glyph.data {
                        text_buffer.push(code);
                    } else if glyph.charcode > 0 {
                        if let Some(c) = char::from_u32(glyph.charcode) {
                            text_buffer.push(c);
                        }
                    }
                }
                GlyphType::Stretch => {
                    // Flush text before stretch
                    if !text_buffer.is_empty() {
                        self.render_text_run(
                            cr,
                            context,
                            &text_buffer,
                            text_start_x,
                            baseline_y,
                            current_face_id.unwrap_or(0),
                        );
                        text_buffer.clear();
                        current_face_id = None;
                    }
                    // Stretch glyphs are just whitespace
                }
                GlyphType::Image => {
                    // Flush text
                    if !text_buffer.is_empty() {
                        self.render_text_run(
                            cr,
                            context,
                            &text_buffer,
                            text_start_x,
                            baseline_y,
                            current_face_id.unwrap_or(0),
                        );
                        text_buffer.clear();
                        current_face_id = None;
                    }
                    // Render image
                    if let GlyphData::Image { image_id } = glyph.data {
                        self.render_image(cr, image_id, x as f32, y as f32, glyph.pixel_width, glyph.height());
                    }
                }
                GlyphType::Video => {
                    // Flush any pending text
                    if !text_buffer.is_empty() {
                        self.render_text_run(
                            cr,
                            context,
                            &text_buffer,
                            text_start_x,
                            baseline_y,
                            current_face_id.unwrap_or(0),
                        );
                        text_buffer.clear();
                        current_face_id = None;
                    }
                    // Render video
                    if let GlyphData::Video { video_id } = glyph.data {
                        #[cfg(feature = "video")]
                        self.render_video(cr, video_id, x as f32, y as f32, glyph.pixel_width, glyph.height());
                        #[cfg(not(feature = "video"))]
                        self.render_video_placeholder(cr, x as f32, y as f32, glyph.pixel_width, glyph.height());
                    }
                }
                _ => {
                    // Flush text for other glyph types
                    if !text_buffer.is_empty() {
                        self.render_text_run(
                            cr,
                            context,
                            &text_buffer,
                            text_start_x,
                            baseline_y,
                            current_face_id.unwrap_or(0),
                        );
                        text_buffer.clear();
                        current_face_id = None;
                    }
                }
            }

            x += glyph.pixel_width as f64;
        }

        // Flush remaining text
        if !text_buffer.is_empty() {
            self.render_text_run(
                cr,
                context,
                &text_buffer,
                text_start_x,
                baseline_y,
                current_face_id.unwrap_or(0),
            );
        }
    }

    /// Render a text run with a specific face
    fn render_text_run(
        &self,
        cr: &cairo::Context,
        context: &pango::Context,
        text: &str,
        x: f64,
        baseline_y: f64,
        face_id: u32,
    ) {
        let layout = pango::Layout::new(context);
        layout.set_text(text);

        // Get face and apply font
        let face = self.face_cache.get(face_id);
        if let Some(face) = face {
            // Parse font description string into Pango FontDescription
            let font_desc_str = face.to_pango_font_description();
            let font_desc = pango::FontDescription::from_string(&font_desc_str);
            layout.set_font_description(Some(&font_desc));
            
            // Set foreground color
            cr.set_source_rgba(
                face.foreground.r as f64,
                face.foreground.g as f64,
                face.foreground.b as f64,
                face.foreground.a as f64,
            );

            // Draw background if not default
            if face.background.a > 0.01 {
                let (width, height) = layout.pixel_size();
                cr.save().ok();
                cr.set_source_rgba(
                    face.background.r as f64,
                    face.background.g as f64,
                    face.background.b as f64,
                    face.background.a as f64,
                );
                // Adjust y for baseline
                let metrics = context.metrics(Some(&font_desc), None);
                let ascent = metrics.ascent() / pango::SCALE;
                cr.rectangle(x, baseline_y - ascent as f64, width as f64, height as f64);
                cr.fill().ok();
                cr.restore().ok();
                
                // Reset foreground color
                cr.set_source_rgba(
                    face.foreground.r as f64,
                    face.foreground.g as f64,
                    face.foreground.b as f64,
                    face.foreground.a as f64,
                );
            }
        } else {
            // Default white text
            cr.set_source_rgb(1.0, 1.0, 1.0);
        }

        // Position at baseline and render
        cr.save().ok();
        
        // Get font metrics to position correctly
        let metrics = layout.context().metrics(layout.font_description().as_ref(), None);
        let ascent = metrics.ascent() / pango::SCALE;
        
        cr.move_to(x, baseline_y - ascent as f64);
        pangocairo::functions::show_layout(cr, &layout);
        
        cr.restore().ok();
    }

    /// Render an image glyph
    fn render_image(&self, cr: &cairo::Context, image_id: u32, x: f32, y: f32, width: i32, height: i32) {
        if let Some(cached_image) = self.image_cache.get(image_id) {
            cr.save().ok();
            
            // Scale image to fit glyph dimensions
            let scale_x = width as f64 / cached_image.width as f64;
            let scale_y = height as f64 / cached_image.height as f64;
            
            cr.translate(x as f64, y as f64);
            cr.scale(scale_x, scale_y);
            
            cr.set_source_surface(&cached_image.surface, 0.0, 0.0).ok();
            cr.paint().ok();
            
            cr.restore().ok();
        } else {
            // Image not loaded, show placeholder
            self.render_image_placeholder(cr, x, y, width, height);
        }
    }

    /// Render a placeholder for images (for images not yet loaded)
    fn render_image_placeholder(&self, cr: &cairo::Context, x: f32, y: f32, width: i32, height: i32) {
        // Draw a gray box with an X
        cr.set_source_rgba(0.3, 0.3, 0.3, 1.0);
        cr.rectangle(x as f64, y as f64, width as f64, height as f64);
        cr.fill().ok();

        // Draw border
        cr.set_source_rgba(0.5, 0.5, 0.5, 1.0);
        cr.set_line_width(1.0);
        cr.rectangle(x as f64 + 0.5, y as f64 + 0.5, (width - 1) as f64, (height - 1) as f64);
        cr.stroke().ok();

        // Draw X
        cr.move_to(x as f64, y as f64);
        cr.line_to((x as i32 + width) as f64, (y as i32 + height) as f64);
        cr.move_to((x as i32 + width) as f64, y as f64);
        cr.line_to(x as f64, (y as i32 + height) as f64);
        cr.stroke().ok();
    }
    
    /// Render a video glyph
    #[cfg(feature = "video")]
    fn render_video(&self, cr: &cairo::Context, video_id: u32, x: f32, y: f32, width: i32, height: i32) {
        if let Some(player) = self.video_cache.get(video_id) {
            if let Some(surface) = player.get_frame() {
                cr.save().ok();
                
                // Scale video to fit glyph dimensions
                let scale_x = width as f64 / surface.width() as f64;
                let scale_y = height as f64 / surface.height() as f64;
                
                cr.translate(x as f64, y as f64);
                cr.scale(scale_x, scale_y);
                
                cr.set_source_surface(&surface, 0.0, 0.0).ok();
                cr.paint().ok();
                
                cr.restore().ok();
            } else {
                // No frame available yet, show placeholder
                self.render_video_placeholder(cr, x, y, width, height);
            }
        } else {
            // Video not loaded, show placeholder
            self.render_video_placeholder(cr, x, y, width, height);
        }
    }
    
    /// Render placeholder for video (when no frame available)
    fn render_video_placeholder(&self, cr: &cairo::Context, x: f32, y: f32, width: i32, height: i32) {
        // Dark box with play symbol
        cr.set_source_rgba(0.1, 0.1, 0.1, 1.0);
        cr.rectangle(x as f64, y as f64, width as f64, height as f64);
        cr.fill().ok();

        // Draw border
        cr.set_source_rgba(0.3, 0.3, 0.3, 1.0);
        cr.set_line_width(1.0);
        cr.rectangle(x as f64 + 0.5, y as f64 + 0.5, (width - 1) as f64, (height - 1) as f64);
        cr.stroke().ok();

        // Draw play triangle in center
        let cx = x as f64 + width as f64 / 2.0;
        let cy = y as f64 + height as f64 / 2.0;
        let size = (width.min(height) as f64 * 0.3).min(30.0);
        
        cr.set_source_rgba(0.5, 0.5, 0.5, 1.0);
        cr.move_to(cx - size / 2.0, cy - size / 2.0);
        cr.line_to(cx + size / 2.0, cy);
        cr.line_to(cx - size / 2.0, cy + size / 2.0);
        cr.close_path();
        cr.fill().ok();
    }
    
    /// Get mutable access to image cache
    pub fn image_cache_mut(&mut self) -> &mut ImageCache {
        &mut self.image_cache
    }
    
    /// Get image cache
    pub fn image_cache(&self) -> &ImageCache {
        &self.image_cache
    }
    
    /// Get mutable access to video cache
    pub fn video_cache_mut(&mut self) -> &mut VideoCache {
        &mut self.video_cache
    }
    
    /// Get video cache
    pub fn video_cache(&self) -> &VideoCache {
        &self.video_cache
    }
    
    /// Update all video players (call periodically)
    pub fn update_videos(&mut self) {
        self.video_cache.update_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_creation() {
        let renderer = Gtk4Renderer::new();
        assert!(renderer.pango_context.is_none());
    }
}

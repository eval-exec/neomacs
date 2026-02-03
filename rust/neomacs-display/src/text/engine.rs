//! Text rendering engine using cosmic-text

use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics,
    ShapeBuffer, SwashCache, Weight, Style,
};
#[cfg(feature = "gtk4-backend")]
use gtk4::gdk;
#[cfg(feature = "gtk4-backend")]
use gtk4::prelude::Cast;

use crate::core::types::Color;
use crate::core::face::{Face, FaceAttributes};

/// Text rendering engine that uses cosmic-text for shaping and rasterization
pub struct TextEngine {
    /// Font system - manages font database
    font_system: FontSystem,
    /// Swash cache for glyph rasterization
    swash_cache: SwashCache,
    /// Shape buffer for text shaping
    shape_buffer: ShapeBuffer,
    /// Default font size in pixels
    default_font_size: f32,
    /// Default line height in pixels
    default_line_height: f32,
}

impl TextEngine {
    /// Create a new text engine
    pub fn new() -> Self {
        let font_system = FontSystem::new();

        Self {
            font_system,
            swash_cache: SwashCache::new(),
            shape_buffer: ShapeBuffer::default(),
            // Use base font size matching Emacs metrics (height=17, ascent=13)
            // GTK handles HiDPI scaling automatically via scale_factor
            default_font_size: 13.0,
            default_line_height: 17.0,
        }
    }

    /// Create a new text engine with custom font size
    pub fn with_font_size(font_size: f32, line_height: f32) -> Self {
        let mut engine = Self::new();
        engine.default_font_size = font_size;
        engine.default_line_height = line_height;
        engine
    }

    /// Get metrics for the default font
    pub fn metrics(&self) -> Metrics {
        Metrics::new(self.default_font_size, self.default_line_height)
    }

    /// Rasterize a single character and return RGBA pixel data
    ///
    /// Returns (width, height, pixels, bearing_x, bearing_y) where pixels is RGBA data
    /// The scale_factor parameter is used for HiDPI rendering (e.g., 2.0 for 2x displays)
    pub fn rasterize_char(
        &mut self,
        c: char,
        face: Option<&Face>,
    ) -> Option<(u32, u32, Vec<u8>, f32, f32)> {
        self.rasterize_char_scaled(c, face, 1.0)
    }

    /// Rasterize a single character at the given scale factor
    ///
    /// Returns (width, height, pixels, bearing_x, bearing_y) where pixels is RGBA data
    /// The scale_factor parameter is used for HiDPI rendering (e.g., 2.0 for 2x displays)
    pub fn rasterize_char_scaled(
        &mut self,
        c: char,
        face: Option<&Face>,
        scale_factor: f32,
    ) -> Option<(u32, u32, Vec<u8>, f32, f32)> {
        // Create attributes from face
        let attrs = self.face_to_attrs(face);

        // Create a small buffer for single character
        let metrics = self.metrics();
        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, Some(100.0), Some(50.0));
        buffer.set_text(&mut self.font_system, &c.to_string(), attrs, cosmic_text::Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        // Get the glyph info
        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                // Rasterize the glyph at the specified scale factor for HiDPI
                let physical_glyph = glyph.physical((0.0, 0.0), scale_factor);

                if let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical_glyph.cache_key) {
                    let width = image.placement.width as u32;
                    let height = image.placement.height as u32;

                    if width == 0 || height == 0 {
                        continue;
                    }

                    // Get bearing for positioning (scale-adjusted)
                    let bearing_x = image.placement.left as f32;
                    let bearing_y = image.placement.top as f32;

                    // Convert to RGBA (clone image data to avoid borrow conflict)
                    let pixels = image_to_rgba(&image, face);
                    return Some((width, height, pixels, bearing_x, bearing_y));
                }
            }
        }

        None
    }

    /// Rasterize a string of text and return positioned glyphs with RGBA data
    pub fn rasterize_text(
        &mut self,
        text: &str,
        face: Option<&Face>,
    ) -> Vec<RasterizedGlyph> {
        let mut glyphs = Vec::new();

        let attrs = self.face_to_attrs(face);
        let metrics = self.metrics();

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_size(&mut self.font_system, Some(10000.0), Some(100.0));
        buffer.set_text(&mut self.font_system, text, attrs, cosmic_text::Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.font_system, false);

        for run in buffer.layout_runs() {
            for glyph in run.glyphs.iter() {
                let physical_glyph = glyph.physical((0.0, 0.0), 1.0);

                if let Some(image) = self.swash_cache.get_image(&mut self.font_system, physical_glyph.cache_key) {
                    let width = image.placement.width as u32;
                    let height = image.placement.height as u32;

                    if width == 0 || height == 0 {
                        continue;
                    }

                    let pixels = image_to_rgba(&image, face);

                    glyphs.push(RasterizedGlyph {
                        x: physical_glyph.x as f32 + image.placement.left as f32,
                        y: run.line_y + image.placement.top as f32,
                        width,
                        height,
                        pixels,
                    });
                }
            }
        }

        glyphs
    }

    /// Convert Emacs Face to cosmic-text Attrs
    fn face_to_attrs(&self, face: Option<&Face>) -> Attrs<'static> {
        let mut attrs = Attrs::new();

        if let Some(f) = face {
            // Font family
            if !f.font_family.is_empty() {
                // cosmic-text needs static lifetime, use common families
                attrs = match f.font_family.to_lowercase().as_str() {
                    "monospace" | "mono" => attrs.family(Family::Monospace),
                    "serif" => attrs.family(Family::Serif),
                    "sans-serif" | "sans" => attrs.family(Family::SansSerif),
                    _ => attrs.family(Family::Monospace), // Default to monospace for Emacs
                };
            } else {
                attrs = attrs.family(Family::Monospace);
            }

            // Font weight
            attrs = attrs.weight(Weight(f.font_weight));

            // Font style (italic)
            if f.attributes.contains(FaceAttributes::ITALIC) {
                attrs = attrs.style(Style::Italic);
            }

            // Color
            attrs = attrs.color(CosmicColor::rgba(
                (f.foreground.r * 255.0) as u8,
                (f.foreground.g * 255.0) as u8,
                (f.foreground.b * 255.0) as u8,
                (f.foreground.a * 255.0) as u8,
            ));
        } else {
            // Default: white monospace
            attrs = attrs
                .family(Family::Monospace)
                .color(CosmicColor::rgba(255, 255, 255, 255));
        }

        attrs
    }

    /// Create a GdkTexture from RGBA pixel data (premultiplied alpha)
    #[cfg(feature = "gtk4-backend")]
    pub fn create_texture(width: u32, height: u32, pixels: &[u8]) -> Option<gdk::Texture> {
        if width == 0 || height == 0 || pixels.is_empty() {
            return None;
        }

        // Premultiply alpha and convert RGBA to BGRA for GSK compatibility
        let mut premultiplied = pixels.to_vec();
        for chunk in premultiplied.chunks_mut(4) {
            let alpha = chunk[3] as f32 / 255.0;
            let r = (chunk[0] as f32 * alpha) as u8;
            let g = (chunk[1] as f32 * alpha) as u8;
            let b = (chunk[2] as f32 * alpha) as u8;
            // Swap R and B for BGRA format
            chunk[0] = b;
            chunk[1] = g;
            chunk[2] = r;
            // chunk[3] stays as alpha
        }

        let bytes = glib::Bytes::from(&premultiplied);
        Some(gdk::MemoryTexture::new(
            width as i32,
            height as i32,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &bytes,
            (width * 4) as usize,
        ).upcast())
    }
}

/// Convert cosmic-text SwashImage to RGBA pixels with face color
fn image_to_rgba(image: &cosmic_text::SwashImage, face: Option<&Face>) -> Vec<u8> {
    let width = image.placement.width as usize;
    let height = image.placement.height as usize;
    let mut pixels = vec![0u8; width * height * 4];

    // Get foreground color from face or default to white
    let (r, g, b) = if let Some(f) = face {
        (
            (f.foreground.r * 255.0) as u8,
            (f.foreground.g * 255.0) as u8,
            (f.foreground.b * 255.0) as u8,
        )
    } else {
        (255, 255, 255)
    };

    // Debug: log info
    let max_alpha = image.data.iter().max().copied().unwrap_or(0);
    log::debug!("image_to_rgba: {}x{} content={:?} fg=({},{},{}) max_alpha={}", 
                width, height, image.content, r, g, b, max_alpha);

    match image.content {
        cosmic_text::SwashContent::Mask => {
            // Alpha mask - apply foreground color
            for (i, alpha) in image.data.iter().enumerate() {
                let offset = i * 4;
                pixels[offset] = r;
                pixels[offset + 1] = g;
                pixels[offset + 2] = b;
                pixels[offset + 3] = *alpha;
            }
        }
        cosmic_text::SwashContent::Color => {
            // Full color (emoji, etc)
            pixels.copy_from_slice(&image.data);
        }
        cosmic_text::SwashContent::SubpixelMask => {
            // Subpixel rendering - treat as grayscale for now
            for (i, chunk) in image.data.chunks(3).enumerate() {
                if i * 4 + 3 >= pixels.len() {
                    break;
                }
                let alpha = ((chunk[0] as u16 + chunk[1] as u16 + chunk[2] as u16) / 3) as u8;
                let offset = i * 4;
                pixels[offset] = r;
                pixels[offset + 1] = g;
                pixels[offset + 2] = b;
                pixels[offset + 3] = alpha;
            }
        }
    }

    pixels
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A rasterized glyph with position and pixel data
#[derive(Debug)]
pub struct RasterizedGlyph {
    /// X offset from text origin
    pub x: f32,
    /// Y offset from text origin (baseline relative)
    pub y: f32,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// RGBA pixel data
    pub pixels: Vec<u8>,
}

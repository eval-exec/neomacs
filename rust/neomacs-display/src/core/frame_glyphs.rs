//! Frame glyph buffer for hybrid rendering.
//!
//! This module implements a simple buffer that collects glyphs during
//! Emacs redisplay. Unlike the scene graph approach, we don't track
//! window state - Emacs provides positions, we just render.

use crate::core::types::{Color, Rect};
use std::collections::HashMap;

/// A single glyph to render
#[derive(Debug, Clone)]
pub enum FrameGlyph {
    /// Character glyph with text
    Char {
        /// Character to render
        char: char,
        /// Frame-absolute X position
        x: f32,
        /// Frame-absolute Y position
        y: f32,
        /// Glyph width
        width: f32,
        /// Row height
        height: f32,
        /// Font ascent
        ascent: f32,
        /// Foreground color
        fg: Color,
        /// Background color (if not transparent)
        bg: Option<Color>,
        /// Face ID for font lookup
        face_id: u32,
        /// Bold flag
        bold: bool,
        /// Italic flag
        italic: bool,
        /// Font size in pixels
        font_size: f32,
        /// Underline style (0=none, 1=single, 2=double, 3=wave)
        underline: u8,
        /// Underline color
        underline_color: Option<Color>,
        /// True if this is mode-line/echo area (renders on top)
        is_overlay: bool,
    },

    /// Stretch (whitespace) glyph
    Stretch {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        bg: Color,
        /// True if this is mode-line/echo area (renders on top)
        is_overlay: bool,
    },

    /// Image glyph
    Image {
        image_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },

    /// Video glyph (inline in buffer)
    Video {
        video_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },

    /// WebKit glyph (inline in buffer)
    WebKit {
        webkit_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },

    /// Cursor
    Cursor {
        window_id: i32,  // Window ID to track which window this cursor belongs to
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        /// 0=box, 1=hollow, 2=bar, 3=hbar
        style: u8,
        color: Color,
    },

    /// Window background
    Background {
        bounds: Rect,
        color: Color,
    },

    /// Window border (vertical/horizontal divider)
    Border {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: Color,
    },
}

impl FrameGlyph {
    /// Returns true if this glyph is an overlay (mode-line/echo area)
    /// that should be rendered on top of other content.
    pub fn is_overlay(&self) -> bool {
        match self {
            FrameGlyph::Char { is_overlay, .. } => *is_overlay,
            FrameGlyph::Stretch { is_overlay, .. } => *is_overlay,
            // Other glyph types are never overlays
            _ => false,
        }
    }
}

/// Buffer collecting glyphs for current frame
#[derive(Debug, Default, Clone)]
pub struct FrameGlyphBuffer {
    /// Frame dimensions
    pub width: f32,
    pub height: f32,

    /// Frame background color
    pub background: Color,

    /// All glyphs to render this frame
    pub glyphs: Vec<FrameGlyph>,

    /// Window regions for this frame (rebuilt each frame by add_window calls)
    pub window_regions: Vec<Rect>,

    /// Window regions from previous frame (for detecting layout changes)
    pub prev_window_regions: Vec<Rect>,

    /// Flag: layout changed last frame, clear glyphs at start of next frame
    pub layout_changed: bool,

    /// Current face attributes (set before adding char glyphs)
    current_face_id: u32,
    current_fg: Color,
    current_bg: Option<Color>,
    current_font_family: String,
    current_bold: bool,
    current_italic: bool,
    current_font_size: f32,
    current_underline: u8,
    current_underline_color: Option<Color>,

    /// Font family cache: face_id -> font_family
    pub face_fonts: HashMap<u32, String>,
}

impl FrameGlyphBuffer {
    pub fn new() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            background: Color::BLACK, // Default to dark (Emacs default theme)
            glyphs: Vec::with_capacity(10000), // Pre-allocate for typical frame
            window_regions: Vec::with_capacity(16),
            prev_window_regions: Vec::with_capacity(16),
            layout_changed: false,
            current_face_id: 0,
            current_fg: Color::WHITE, // Default foreground white for dark theme
            current_bg: None,
            current_font_family: "monospace".to_string(),
            current_bold: false,
            current_italic: false,
            current_font_size: 14.0,
            current_underline: 0,
            current_underline_color: None,
            face_fonts: HashMap::new(),
        }
    }

    /// Create a new buffer with specified dimensions
    pub fn with_size(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            background: Color::BLACK, // Default to dark (Emacs default theme)
            glyphs: Vec::with_capacity(10000), // Pre-allocate for typical frame
            window_regions: Vec::with_capacity(16),
            prev_window_regions: Vec::with_capacity(16),
            layout_changed: false,
            current_face_id: 0,
            current_fg: Color::WHITE, // Default foreground white for dark theme
            current_bg: None,
            current_font_family: "monospace".to_string(),
            current_bold: false,
            current_italic: false,
            current_font_size: 14.0,
            current_underline: 0,
            current_underline_color: None,
            face_fonts: HashMap::new(),
        }
    }

    /// Start new frame - prepare for new content
    pub fn start_frame(&mut self) {
        // Save current window regions to previous (for comparison at end of frame)
        std::mem::swap(&mut self.prev_window_regions, &mut self.window_regions);
        // Clear current regions - they'll be populated during this frame
        self.window_regions.clear();
    }

    /// End frame - remove glyphs when layout changes
    /// Returns true if glyphs were cleared (caller should force Emacs to resend)
    pub fn end_frame(&mut self) -> bool {
        // NOTE: We can't rely on window_regions count because Emacs does incremental
        // updates - only changed windows call update_window_begin. So during a cursor
        // blink, only 1 window might be updated even though 3 exist.
        //
        // For now, don't auto-clear based on region count. The overlapping removal
        // in add_char handles content updates, and explicit resize/clear operations
        // handle layout changes.
        
        false
    }
    
    /// Check and reset layout_changed flag
    pub fn take_layout_changed(&mut self) -> bool {
        let was_changed = self.layout_changed;
        self.layout_changed = false;
        was_changed
    }

    /// Clear buffer for new frame
    pub fn begin_frame(&mut self, width: f32, height: f32, background: Color) {
        self.width = width;
        self.height = height;
        self.background = background;
        self.glyphs.clear();
    }

    /// Set current face attributes for subsequent char glyphs (with font family)
    pub fn set_face_with_font(&mut self, face_id: u32, fg: Color, bg: Option<Color>,
                    font_family: &str, bold: bool, italic: bool, font_size: f32, underline: u8, underline_color: Option<Color>) {
        self.current_face_id = face_id;
        self.current_fg = fg;
        self.current_bg = bg;
        self.current_font_family = font_family.to_string();
        self.current_bold = bold;
        self.current_italic = italic;
        self.current_font_size = font_size;
        self.current_underline = underline;
        self.current_underline_color = underline_color;
        // Store font family for this face_id
        self.face_fonts.insert(face_id, font_family.to_string());
    }

    /// Set current face attributes for subsequent char glyphs
    pub fn set_face(&mut self, face_id: u32, fg: Color, bg: Option<Color>,
                    bold: bool, italic: bool, underline: u8, underline_color: Option<Color>) {
        self.current_face_id = face_id;
        self.current_fg = fg;
        self.current_bg = bg;
        self.current_bold = bold;
        self.current_italic = italic;
        self.current_underline = underline;
        self.current_underline_color = underline_color;
    }

    /// Get font family for a face_id
    pub fn get_face_font(&self, face_id: u32) -> &str {
        self.face_fonts.get(&face_id).map(|s| s.as_str()).unwrap_or("monospace")
    }

    /// Get current font family
    pub fn get_current_font_family(&self) -> &str {
        &self.current_font_family
    }

    /// Get current face background color (for stretch glyphs)
    pub fn get_current_bg(&self) -> Option<Color> {
        self.current_bg
    }

    /// Add a window background rectangle and record the window region
    pub fn add_background(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        // Record this window region
        self.window_regions.push(Rect::new(x, y, width, height));

        // Don't remove overlapping glyphs here - that would break incremental redisplay
        // since add_background is called on every frame even when content hasn't changed.
        // Instead, overlapping is handled when new glyphs are added (add_char calls remove_overlapping)

        self.glyphs.push(FrameGlyph::Background {
            bounds: Rect::new(x, y, width, height),
            color,
        });
    }

    /// Remove stale glyphs only if window layout indicates windows were deleted
    /// Called at end of frame.
    /// Only removes stale glyphs when a single full-width window is drawn,
    /// which happens after C-x 0 or C-x 1 (delete-other-windows).
    pub fn remove_stale_glyphs_if_layout_changed(&mut self) {
        // No current regions means no windows were updated this frame (incremental)
        // Keep all existing glyphs
        if self.window_regions.is_empty() {
            return;
        }

        // Check if any window covers the full frame width
        // This indicates a full redisplay after window deletion (C-x 0, C-x 1)
        let has_full_width_window = self.window_regions.iter().any(|r| {
            r.x < 1.0 && (r.width - self.width).abs() < 10.0
        });

        if !has_full_width_window {
            // Windows were split/resized but not deleted - keep all glyphs
            // The right window wasn't updated because its content didn't change
            return;
        }

        // A full-width window was drawn - remove glyphs outside current regions
        self.glyphs.retain(|g| {
            let (gx, gy) = match g {
                FrameGlyph::Char { x, y, .. } => (*x, *y),
                FrameGlyph::Stretch { x, y, .. } => (*x, *y),
                FrameGlyph::Image { x, y, .. } => (*x, *y),
                FrameGlyph::Video { x, y, .. } => (*x, *y),
                FrameGlyph::WebKit { x, y, .. } => (*x, *y),
                // Keep backgrounds, cursors, borders - they're added fresh each frame
                _ => return true,
            };

            // Keep if glyph is inside ANY window region
            self.window_regions.iter().any(|r| {
                gx >= r.x && gx < r.x + r.width &&
                gy >= r.y && gy < r.y + r.height
            })
        });
    }

    /// Remove glyphs that are outside all current window regions
    /// Call this at end of frame to clean up stale glyphs from deleted windows
    #[allow(dead_code)]
    pub fn remove_stale_glyphs(&mut self) {
        if self.window_regions.is_empty() {
            return;
        }

        self.glyphs.retain(|g| {
            let (gx, gy) = match g {
                FrameGlyph::Char { x, y, .. } => (*x, *y),
                FrameGlyph::Stretch { x, y, .. } => (*x, *y),
                FrameGlyph::Image { x, y, .. } => (*x, *y),
                FrameGlyph::Video { x, y, .. } => (*x, *y),
                FrameGlyph::WebKit { x, y, .. } => (*x, *y),
                // Keep backgrounds, cursors, borders - they're added fresh each frame
                _ => return true,
            };

            // Keep if glyph is inside ANY window region
            self.window_regions.iter().any(|r| {
                gx >= r.x && gx < r.x + r.width &&
                gy >= r.y && gy < r.y + r.height
            })
        });
    }

    /// Remove glyphs that overlap with the given rectangle
    /// Only removes Char/Stretch glyphs with exact Y match (within 1px) to prevent mode-line issues.
    /// Image/Video glyphs are NOT removed here - they are managed by add_image/add_video.
    fn remove_overlapping(&mut self, x: f32, y: f32, width: f32, _height: f32) {
        let x_end = x + width;
        self.glyphs.retain(|g| {
            let (gx, gy, gw) = match g {
                FrameGlyph::Char { x, y, width, .. } => (*x, *y, *width),
                FrameGlyph::Stretch { x, y, width, .. } => (*x, *y, *width),
                // Keep Image/Video glyphs - they are managed by add_image/add_video
                FrameGlyph::Image { .. } => return true,
                FrameGlyph::Video { .. } => return true,
                // Keep WebKit glyphs - persistent embedded views
                FrameGlyph::WebKit { .. } => return true,
                // Keep backgrounds, cursors, borders - managed separately
                _ => return true,
            };
            let gx_end = gx + gw;
            // Keep if no X overlap OR Y doesn't match (within 1px tolerance)
            // This prevents mode-line (y=561) from being cleared when content rows
            // on a shifted grid (y=554) add their chars.
            gx_end <= x || gx >= x_end || (gy - y).abs() > 1.0
        });
    }

    /// Clear all glyphs in a rectangular area
    /// Called by gui_clear_end_of_line and related functions
    /// Note: Image and Video glyphs are NOT removed here - they are managed by add_image/add_video
    pub fn clear_area(&mut self, x: f32, y: f32, width: f32, height: f32) {
        let x_end = x + width;
        let y_end = y + height;

        self.glyphs.retain(|g| {
            let (gx, gy, gw, gh) = match g {
                FrameGlyph::Char { x, y, width, height, .. } => (*x, *y, *width, *height),
                FrameGlyph::Stretch { x, y, width, height, .. } => (*x, *y, *width, *height),
                // Keep Image and Video glyphs - they are managed by add_image/add_video
                // which handles removing same-ID images and overlapping images.
                // Line-by-line clear_area calls during text redraw should NOT remove images.
                FrameGlyph::Image { .. } => return true,
                FrameGlyph::Video { .. } => return true,
                // WebKit glyphs: remove if the clear area covers the webkit's top edge
                // This handles scrolling (where webkit moves up and out of view)
                // while protecting against mode-line clears (which are at the bottom)
                FrameGlyph::WebKit { x: wx, y: wy, width: ww, height: wh, .. } => {
                    let gx_end = *wx + *ww;
                    // Check horizontal overlap
                    let h_overlap = *wx < x_end && gx_end > x;
                    // Remove if clear area starts at or above webkit's top AND has horizontal overlap
                    // This catches scroll operations that move webkit out of view
                    let covers_top = y <= *wy && y_end > *wy && h_overlap;
                    // Also remove if fully contained (for completeness)
                    let gy_end = *wy + *wh;
                    let fully_contained = *wx >= x && gx_end <= x_end && *wy >= y && gy_end <= y_end;
                    return !(covers_top || fully_contained);
                }
                // Keep backgrounds, cursors, borders - managed separately
                _ => return true,
            };
            let gx_end = gx + gw;
            let gy_end = gy + gh;
            // Keep if glyph does NOT overlap with the clear area
            gx_end <= x || gx >= x_end || gy_end <= y || gy >= y_end
        });
    }

    /// Add a character glyph (removes overlapping glyphs first)
    pub fn add_char(&mut self, char: char, x: f32, y: f32, width: f32, height: f32, ascent: f32, is_overlay: bool) {
        // Remove any existing glyphs at this position
        self.remove_overlapping(x, y, width, height);
        self.glyphs.push(FrameGlyph::Char {
            char,
            x,
            y,
            width,
            height,
            ascent,
            fg: self.current_fg,
            bg: self.current_bg,
            face_id: self.current_face_id,
            bold: self.current_bold,
            italic: self.current_italic,
            font_size: self.current_font_size,
            underline: self.current_underline,
            underline_color: self.current_underline_color,
            is_overlay,
        });
    }

    /// Add a stretch (whitespace) glyph (removes overlapping glyphs first)
    pub fn add_stretch(&mut self, x: f32, y: f32, width: f32, height: f32, bg: Color, is_overlay: bool) {
        self.remove_overlapping(x, y, width, height);
        self.glyphs.push(FrameGlyph::Stretch { x, y, width, height, bg, is_overlay });
    }

    /// Add an image glyph
    /// Removes any existing image glyph with the same ID first, then removes
    /// any other images that overlap with this image's position.
    pub fn add_image(&mut self, image_id: u32, x: f32, y: f32, width: f32, height: f32) {
        // Remove any existing image glyph with the same ID (handles scrolling)
        // This prevents duplicate images when the same image is redrawn at a different position
        self.glyphs.retain(|g| {
            if let FrameGlyph::Image { image_id: id, .. } = g {
                *id != image_id
            } else {
                true
            }
        });

        // Remove any OTHER image that overlaps with this image's position
        // This handles the case where different images end up at the same screen position
        // (e.g., after scrolling, Image 4 is at y=51 and Image 1 should replace it)
        let x_end = x + width;
        let y_end = y + height;
        self.glyphs.retain(|g| {
            if let FrameGlyph::Image { x: gx, y: gy, width: gw, height: gh, .. } = g {
                let gx_end = *gx + *gw;
                let gy_end = *gy + *gh;
                // Keep if no overlap
                gx_end <= x || *gx >= x_end || gy_end <= y || *gy >= y_end
            } else {
                true
            }
        });

        // Remove overlapping char/stretch glyphs (but NOT other images - already handled above)
        self.remove_overlapping(x, y, width, height);
        self.glyphs.push(FrameGlyph::Image { image_id, x, y, width, height });
    }

    /// Add a video glyph
    /// Removes any existing video glyph with the same ID first to handle scrolling
    pub fn add_video(&mut self, video_id: u32, x: f32, y: f32, width: f32, height: f32) {
        // Remove any existing video glyph with the same ID (handles scrolling)
        self.glyphs.retain(|g| {
            if let FrameGlyph::Video { video_id: id, .. } = g {
                *id != video_id
            } else {
                true
            }
        });

        // Remove any OTHER video that overlaps with this video's position
        // This handles the case where different videos end up at the same screen position
        let x_end = x + width;
        let y_end = y + height;
        self.glyphs.retain(|g| {
            if let FrameGlyph::Video { x: gx, y: gy, width: gw, height: gh, .. } = g {
                let gx_end = *gx + *gw;
                let gy_end = *gy + *gh;
                // Keep if no overlap
                gx_end <= x || *gx >= x_end || gy_end <= y || *gy >= y_end
            } else {
                true
            }
        });

        self.remove_overlapping(x, y, width, height);
        self.glyphs.push(FrameGlyph::Video { video_id, x, y, width, height });
    }

    /// Add a webkit glyph
    /// Removes any existing webkit glyph with the same ID first, then removes
    /// any other webkits that overlap with this webkit's position.
    pub fn add_webkit(&mut self, webkit_id: u32, x: f32, y: f32, width: f32, height: f32) {
        // Remove any existing webkit glyph with the same ID (handles scrolling)
        self.glyphs.retain(|g| {
            if let FrameGlyph::WebKit { webkit_id: id, .. } = g {
                *id != webkit_id
            } else {
                true
            }
        });

        // Remove any OTHER webkit that overlaps with this webkit's position
        // This handles the case where different webkits end up at the same screen position
        let x_end = x + width;
        let y_end = y + height;
        self.glyphs.retain(|g| {
            if let FrameGlyph::WebKit { x: gx, y: gy, width: gw, height: gh, .. } = g {
                let gx_end = *gx + *gw;
                let gy_end = *gy + *gh;
                // Keep if no overlap
                gx_end <= x || *gx >= x_end || gy_end <= y || *gy >= y_end
            } else {
                true
            }
        });

        self.remove_overlapping(x, y, width, height);
        self.glyphs.push(FrameGlyph::WebKit { webkit_id, x, y, width, height });
    }

    /// Add cursor - removes any existing cursor for the same window first
    pub fn add_cursor(&mut self, window_id: i32, x: f32, y: f32, width: f32, height: f32, style: u8, color: Color) {
        // Remove any existing cursor for THIS window only (other windows keep their cursors)
        self.glyphs.retain(|g| {
            if let FrameGlyph::Cursor { window_id: wid, .. } = g {
                *wid != window_id
            } else {
                true
            }
        });
        self.glyphs.push(FrameGlyph::Cursor { window_id, x, y, width, height, style, color });
    }

    /// Add border
    pub fn add_border(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.glyphs.push(FrameGlyph::Border { x, y, width, height, color });
    }

    /// Get glyph count
    pub fn len(&self) -> usize {
        self.glyphs.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }
}

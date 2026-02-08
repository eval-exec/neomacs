//! Frame glyph buffer for matrix-based full-frame rendering.
//!
//! Each frame, the C-side matrix walker extracts ALL visible glyphs from
//! Emacs's current_matrix and rebuilds this buffer from scratch. No
//! incremental overlap tracking is needed.

use crate::core::face::Face;
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
        /// Underline style (0=none, 1=single, 2=wave, 3=double, 4=dotted, 5=dashed)
        underline: u8,
        /// Underline color
        underline_color: Option<Color>,
        /// Strike-through (0=none, 1=enabled)
        strike_through: u8,
        /// Strike-through color
        strike_through_color: Option<Color>,
        /// Overline (0=none, 1=enabled)
        overline: u8,
        /// Overline color
        overline_color: Option<Color>,
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
        face_id: u32,
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
        /// 0=box, 1=bar, 2=hbar, 3=hollow
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

    /// Terminal glyph (inline in buffer or window-mode)
    #[cfg(feature = "neo-term")]
    Terminal {
        terminal_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
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

/// Inverse video info for the character under a filled box cursor
#[derive(Debug, Clone)]
pub struct CursorInverseInfo {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// Cursor rect color (drawn as background)
    pub cursor_bg: Color,
    /// Text color for the character at cursor position
    pub cursor_fg: Color,
}

/// Per-window metadata for animation transition detection
#[derive(Debug, Clone, PartialEq)]
pub struct WindowInfo {
    /// Window pointer as i64 (unique window identifier)
    pub window_id: i64,
    /// Buffer pointer as u64 (unique buffer identifier)
    pub buffer_id: u64,
    /// First visible character position (marker_position(w->start))
    pub window_start: i64,
    /// Frame-absolute window bounds (includes mode-line)
    pub bounds: Rect,
    /// Height of the mode-line in pixels (0 if no mode-line)
    pub mode_line_height: f32,
    /// Whether this is the selected (active) window
    pub selected: bool,
}

/// Buffer collecting glyphs for current frame.
///
/// With matrix-based rendering, this buffer is cleared and rebuilt from scratch
/// each frame by the C-side matrix walker. No incremental state management needed.
#[derive(Debug, Default, Clone)]
pub struct FrameGlyphBuffer {
    /// Frame dimensions
    pub width: f32,
    pub height: f32,

    /// Default character cell dimensions (from FRAME_COLUMN_WIDTH / FRAME_LINE_HEIGHT)
    pub char_width: f32,
    pub char_height: f32,
    /// Default font pixel size (from FRAME_FONT(f)->pixel_size)
    pub font_pixel_size: f32,

    /// Frame background color
    pub background: Color,

    /// All glyphs to render this frame
    pub glyphs: Vec<FrameGlyph>,

    /// Window regions for this frame (rebuilt each frame by add_window calls)
    pub window_regions: Vec<Rect>,

    /// Window regions from previous frame (kept for compatibility)
    pub prev_window_regions: Vec<Rect>,

    /// Per-window metadata for animation detection
    pub window_infos: Vec<WindowInfo>,

    /// Inverse video info for filled box cursor (set by C for style 0)
    pub cursor_inverse: Option<CursorInverseInfo>,

    /// Flag: layout changed last frame (kept for compatibility)
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
    current_strike_through: u8,
    current_strike_through_color: Option<Color>,
    current_overline: u8,
    current_overline_color: Option<Color>,

    /// Font family cache: face_id -> font_family
    pub face_fonts: HashMap<u32, String>,

    /// Full face data: face_id -> Face (includes box, underline, etc.)
    pub faces: HashMap<u32, Face>,
}

impl FrameGlyphBuffer {
    pub fn new() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            char_width: 8.0,
            char_height: 16.0,
            font_pixel_size: 14.0,
            background: Color::BLACK,
            glyphs: Vec::with_capacity(10000),
            window_regions: Vec::with_capacity(16),
            prev_window_regions: Vec::with_capacity(16),
            window_infos: Vec::with_capacity(16),
            cursor_inverse: None,
            layout_changed: false,
            current_face_id: 0,
            current_fg: Color::WHITE,
            current_bg: None,
            current_font_family: "monospace".to_string(),
            current_bold: false,
            current_italic: false,
            current_font_size: 14.0,
            current_underline: 0,
            current_underline_color: None,
            current_strike_through: 0,
            current_strike_through_color: None,
            current_overline: 0,
            current_overline_color: None,
            face_fonts: HashMap::new(),
            faces: HashMap::new(),
        }
    }

    /// Create a new buffer with specified dimensions
    pub fn with_size(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            ..Self::new()
        }
    }

    /// Clear all glyphs for a fresh full-frame rebuild.
    /// Called at the start of each frame by the matrix walker.
    pub fn clear_all(&mut self) {
        self.glyphs.clear();
        self.window_regions.clear();
        self.window_infos.clear();
        self.cursor_inverse = None;
    }

    /// Start new frame - prepare for new content (compatibility shim)
    pub fn start_frame(&mut self) {
        std::mem::swap(&mut self.prev_window_regions, &mut self.window_regions);
        self.window_regions.clear();
    }

    /// End frame (compatibility shim, always returns false now)
    pub fn end_frame(&mut self) -> bool {
        false
    }

    /// Check and reset layout_changed flag (compatibility)
    pub fn take_layout_changed(&mut self) -> bool {
        let was_changed = self.layout_changed;
        self.layout_changed = false;
        was_changed
    }

    /// Clear buffer for new frame (legacy API)
    pub fn begin_frame(&mut self, width: f32, height: f32, background: Color) {
        self.width = width;
        self.height = height;
        self.background = background;
        self.glyphs.clear();
        self.cursor_inverse = None;
    }

    /// Set current face attributes for subsequent char glyphs (with font family)
    pub fn set_face_with_font(&mut self, face_id: u32, fg: Color, bg: Option<Color>,
                    font_family: &str, bold: bool, italic: bool, font_size: f32,
                    underline: u8, underline_color: Option<Color>,
                    strike_through: u8, strike_through_color: Option<Color>,
                    overline: u8, overline_color: Option<Color>) {
        self.current_face_id = face_id;
        self.current_fg = fg;
        self.current_bg = bg;
        self.current_font_family = font_family.to_string();
        self.current_bold = bold;
        self.current_italic = italic;
        self.current_font_size = font_size;
        self.current_underline = underline;
        self.current_underline_color = underline_color;
        self.current_strike_through = strike_through;
        self.current_strike_through_color = strike_through_color;
        self.current_overline = overline;
        self.current_overline_color = overline_color;
        self.face_fonts.insert(face_id, font_family.to_string());
    }

    /// Set current face attributes for subsequent char glyphs
    pub fn set_face(&mut self, face_id: u32, fg: Color, bg: Option<Color>,
                    bold: bool, italic: bool, underline: u8, underline_color: Option<Color>,
                    strike_through: u8, strike_through_color: Option<Color>,
                    overline: u8, overline_color: Option<Color>) {
        self.current_face_id = face_id;
        self.current_fg = fg;
        self.current_bg = bg;
        self.current_bold = bold;
        self.current_italic = italic;
        self.current_underline = underline;
        self.current_underline_color = underline_color;
        self.current_strike_through = strike_through;
        self.current_strike_through_color = strike_through_color;
        self.current_overline = overline;
        self.current_overline_color = overline_color;
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

    /// Add a window background rectangle and record the window region.
    /// With full-frame rebuild, no stale-background removal is needed.
    pub fn add_background(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.window_regions.push(Rect::new(x, y, width, height));
        self.glyphs.push(FrameGlyph::Background {
            bounds: Rect::new(x, y, width, height),
            color,
        });
    }

    /// No-op kept for API compatibility. With full-frame rebuild, stale glyphs
    /// are impossible since the buffer is cleared each frame.
    #[allow(dead_code)]
    pub fn remove_stale_glyphs_if_layout_changed(&mut self) {}

    /// No-op kept for API compatibility.
    #[allow(dead_code)]
    pub fn remove_stale_glyphs(&mut self) {}

    /// No-op kept for API compatibility. With full-frame rebuild, clear_area
    /// is not needed since we rebuild from scratch.
    pub fn clear_area(&mut self, _x: f32, _y: f32, _width: f32, _height: f32) {}

    /// No-op kept for API compatibility.
    pub fn clear_media_in_area(&mut self, _x: f32, _y: f32, _width: f32, _height: f32) {}

    /// Add a character glyph. No overlap removal needed with full-frame rebuild.
    pub fn add_char(&mut self, char: char, x: f32, y: f32, width: f32, height: f32, ascent: f32, is_overlay: bool) {
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
            strike_through: self.current_strike_through,
            strike_through_color: self.current_strike_through_color,
            overline: self.current_overline,
            overline_color: self.current_overline_color,
            is_overlay,
        });
    }

    /// Get current font size
    pub fn font_size(&self) -> f32 {
        self.current_font_size
    }

    /// Set current font size (for display property height scaling)
    pub fn set_font_size(&mut self, size: f32) {
        self.current_font_size = size;
    }

    /// Add a stretch (whitespace) glyph. No overlap removal needed.
    pub fn add_stretch(&mut self, x: f32, y: f32, width: f32, height: f32, bg: Color, face_id: u32, is_overlay: bool) {
        self.glyphs.push(FrameGlyph::Stretch { x, y, width, height, bg, face_id, is_overlay });
    }

    /// Add an image glyph
    pub fn add_image(&mut self, image_id: u32, x: f32, y: f32, width: f32, height: f32) {
        self.glyphs.push(FrameGlyph::Image { image_id, x, y, width, height });
    }

    /// Add a video glyph
    pub fn add_video(&mut self, video_id: u32, x: f32, y: f32, width: f32, height: f32) {
        self.glyphs.push(FrameGlyph::Video { video_id, x, y, width, height });
    }

    /// Add a webkit glyph
    pub fn add_webkit(&mut self, webkit_id: u32, x: f32, y: f32, width: f32, height: f32) {
        self.glyphs.push(FrameGlyph::WebKit { webkit_id, x, y, width, height });
    }

    /// Add cursor
    pub fn add_cursor(&mut self, window_id: i32, x: f32, y: f32, width: f32, height: f32, style: u8, color: Color) {
        self.glyphs.push(FrameGlyph::Cursor { window_id, x, y, width, height, style, color });
    }

    /// Add per-window metadata for animation detection
    pub fn add_window_info(&mut self, window_id: i64, buffer_id: u64, window_start: i64,
                           x: f32, y: f32, width: f32, height: f32,
                           mode_line_height: f32, selected: bool) {
        self.window_infos.push(WindowInfo {
            window_id,
            buffer_id,
            window_start,
            bounds: Rect::new(x, y, width, height),
            mode_line_height,
            selected,
        });
    }

    /// Set cursor inverse video info (for filled box cursor)
    pub fn set_cursor_inverse(&mut self, x: f32, y: f32, width: f32, height: f32,
                              cursor_bg: Color, cursor_fg: Color) {
        self.cursor_inverse = Some(CursorInverseInfo {
            x, y, width, height, cursor_bg, cursor_fg,
        });
    }

    /// Add border
    pub fn add_border(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        self.glyphs.push(FrameGlyph::Border { x, y, width, height, color });
    }

    /// Add terminal glyph (inline or window mode)
    #[cfg(feature = "neo-term")]
    pub fn add_terminal(&mut self, terminal_id: u32, x: f32, y: f32, width: f32, height: f32) {
        self.glyphs.push(FrameGlyph::Terminal { terminal_id, x, y, width, height });
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

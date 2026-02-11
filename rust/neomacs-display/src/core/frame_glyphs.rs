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
        /// Character to render (base character for single-codepoint glyphs)
        char: char,
        /// Composed text for multi-codepoint grapheme clusters (emoji ZWJ, combining marks).
        /// When Some, the renderer uses this instead of `char` for glyph lookup.
        composed: Option<Box<str>>,
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
        /// Font weight (CSS scale: 100=thin, 400=normal, 700=bold, 900=black)
        font_weight: u16,
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
        /// Stipple pattern ID (0 = none, references stipple_patterns in FrameGlyphBuffer)
        stipple_id: i32,
        /// Foreground color for stipple pattern (stipple bits use fg, gaps use bg)
        stipple_fg: Option<Color>,
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

    /// Scroll bar (GPU-rendered)
    ScrollBar {
        /// True for horizontal, false for vertical
        horizontal: bool,
        /// Frame-absolute position and dimensions of the scroll bar track
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        /// Thumb start position (pixels from track start)
        thumb_start: f32,
        /// Thumb size (pixels)
        thumb_size: f32,
        /// Track background color
        track_color: Color,
        /// Thumb color
        thumb_color: Color,
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

/// Stipple pattern: XBM bitmap data for tiled background patterns
#[derive(Debug, Clone)]
pub struct StipplePattern {
    /// Pattern width in pixels
    pub width: u32,
    /// Pattern height in pixels
    pub height: u32,
    /// Raw XBM bits: row-by-row, each row is (width+7)/8 bytes, LSB-first
    pub bits: Vec<u8>,
}

/// Lightweight box attributes for a face (avoids storing full Face on Emacs thread)
#[derive(Debug, Clone)]
pub struct FaceBoxAttrs {
    /// Box type (0=none, 1=line)
    pub box_type: i32,
    /// Box color
    pub box_color: Color,
    /// Box line width
    pub box_line_width: i32,
    /// Box corner radius (0 = sharp)
    pub box_corner_radius: i32,
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
    /// Last visible character position
    pub window_end: i64,
    /// Total buffer size in characters (BUF_Z)
    pub buffer_size: i64,
    /// Frame-absolute window bounds (includes mode-line)
    pub bounds: Rect,
    /// Height of the mode-line in pixels (0 if no mode-line)
    pub mode_line_height: f32,
    /// Whether this is the selected (active) window
    pub selected: bool,
    /// Whether this is the minibuffer window
    pub is_minibuffer: bool,
    /// Character cell height for this window (tracks text-scale-adjust)
    pub char_height: f32,
    /// Buffer file name (empty string if no file)
    pub buffer_file_name: String,
    /// Whether the buffer has unsaved modifications
    pub modified: bool,
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

    // --- Child frame identity (Phase 1) ---
    /// Frame pointer cast to u64 (0 = root/unset)
    pub frame_id: u64,
    /// Parent frame pointer (0 = root frame, no parent)
    pub parent_id: u64,
    /// Position relative to parent frame (pixels)
    pub parent_x: f32,
    pub parent_y: f32,
    /// Stacking order among sibling child frames
    pub z_order: i32,
    /// Child frame border width (pixels)
    pub border_width: f32,
    /// Child frame border color
    pub border_color: Color,
    /// Background opacity (1.0 = opaque, 0.0 = transparent)
    pub background_alpha: f32,
    /// Whether this frame should not accept keyboard focus
    pub no_accept_focus: bool,

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
    current_font_weight: u16,
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

    /// Stipple patterns: bitmap_id -> StipplePattern
    pub stipple_patterns: HashMap<i32, StipplePattern>,

    /// Box attributes per face: face_id -> FaceBoxAttrs
    /// Populated by the Rust layout engine's apply_face() as a lightweight
    /// side-channel (avoids inserting full Face objects which causes heap corruption).
    pub face_box_attrs: HashMap<u32, FaceBoxAttrs>,
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
            frame_id: 0,
            parent_id: 0,
            parent_x: 0.0,
            parent_y: 0.0,
            z_order: 0,
            border_width: 0.0,
            border_color: Color::BLACK,
            background_alpha: 1.0,
            no_accept_focus: false,
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
            current_font_weight: 400,
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
            stipple_patterns: HashMap::new(),
            face_box_attrs: HashMap::new(),
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
        self.stipple_patterns.clear();
        self.face_box_attrs.clear();
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
        self.stipple_patterns.clear();
        self.face_box_attrs.clear();
    }

    /// Set frame identity for child frame support.
    /// Called after begin_frame, before glyphs are added.
    pub fn set_frame_identity(
        &mut self,
        frame_id: u64,
        parent_id: u64,
        parent_x: f32,
        parent_y: f32,
        z_order: i32,
        border_width: f32,
        border_color: Color,
        no_accept_focus: bool,
        background_alpha: f32,
    ) {
        self.frame_id = frame_id;
        self.parent_id = parent_id;
        self.parent_x = parent_x;
        self.parent_y = parent_y;
        self.z_order = z_order;
        self.border_width = border_width;
        self.border_color = border_color;
        self.no_accept_focus = no_accept_focus;
        self.background_alpha = background_alpha;
    }

    /// Set current face attributes for subsequent char glyphs (with font family)
    pub fn set_face_with_font(&mut self, face_id: u32, fg: Color, bg: Option<Color>,
                    font_family: &str, font_weight: u16, italic: bool, font_size: f32,
                    underline: u8, underline_color: Option<Color>,
                    strike_through: u8, strike_through_color: Option<Color>,
                    overline: u8, overline_color: Option<Color>) {
        self.current_face_id = face_id;
        self.current_fg = fg;
        self.current_bg = bg;
        self.current_font_family = font_family.to_string();
        self.current_font_weight = font_weight;
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
                    font_weight: u16, italic: bool, underline: u8, underline_color: Option<Color>,
                    strike_through: u8, strike_through_color: Option<Color>,
                    overline: u8, overline_color: Option<Color>) {
        self.current_face_id = face_id;
        self.current_fg = fg;
        self.current_bg = bg;
        self.current_font_weight = font_weight;
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

    /// Add a character glyph. No overlap removal needed with full-frame rebuild.
    pub fn add_char(&mut self, char: char, x: f32, y: f32, width: f32, height: f32, ascent: f32, is_overlay: bool) {
        self.glyphs.push(FrameGlyph::Char {
            char,
            composed: None,
            x,
            y,
            width,
            height,
            ascent,
            fg: self.current_fg,
            bg: self.current_bg,
            face_id: self.current_face_id,
            font_weight: self.current_font_weight,
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

    /// Add a composed (multi-codepoint) character glyph.
    /// Used for grapheme clusters like emoji ZWJ sequences, combining diacritics.
    pub fn add_composed_char(&mut self, text: &str, base_char: char, x: f32, y: f32, width: f32, height: f32, ascent: f32, is_overlay: bool) {
        self.glyphs.push(FrameGlyph::Char {
            char: base_char,
            composed: Some(text.into()),
            x,
            y,
            width,
            height,
            ascent,
            fg: self.current_fg,
            bg: self.current_bg,
            face_id: self.current_face_id,
            font_weight: self.current_font_weight,
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
        self.glyphs.push(FrameGlyph::Stretch { x, y, width, height, bg, face_id, is_overlay, stipple_id: 0, stipple_fg: None });
    }

    /// Add a stretch glyph with a stipple pattern
    pub fn add_stretch_stipple(&mut self, x: f32, y: f32, width: f32, height: f32,
                               bg: Color, fg: Color, face_id: u32, is_overlay: bool,
                               stipple_id: i32) {
        self.glyphs.push(FrameGlyph::Stretch {
            x, y, width, height, bg, face_id, is_overlay,
            stipple_id, stipple_fg: Some(fg),
        });
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
    pub fn add_window_info(&mut self, window_id: i64, buffer_id: u64,
                           window_start: i64, window_end: i64, buffer_size: i64,
                           x: f32, y: f32, width: f32, height: f32,
                           mode_line_height: f32, selected: bool,
                           is_minibuffer: bool, char_height: f32,
                           buffer_file_name: String, modified: bool) {
        self.window_infos.push(WindowInfo {
            window_id,
            buffer_id,
            window_start,
            window_end,
            buffer_size,
            bounds: Rect::new(x, y, width, height),
            mode_line_height,
            selected,
            is_minibuffer,
            char_height,
            buffer_file_name,
            modified,
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

    /// Add a scroll bar glyph (GPU-rendered)
    pub fn add_scroll_bar(&mut self, horizontal: bool, x: f32, y: f32, width: f32, height: f32,
                          thumb_start: f32, thumb_size: f32, track_color: Color, thumb_color: Color) {
        self.glyphs.push(FrameGlyph::ScrollBar {
            horizontal, x, y, width, height,
            thumb_start, thumb_size, track_color, thumb_color,
        });
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

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Helper: assert a Color matches expected RGBA (with tolerance for floats)
    // -----------------------------------------------------------------------
    fn assert_color_eq(actual: &Color, expected: &Color) {
        assert!(
            (actual.r - expected.r).abs() < 1e-5
                && (actual.g - expected.g).abs() < 1e-5
                && (actual.b - expected.b).abs() < 1e-5
                && (actual.a - expected.a).abs() < 1e-5,
            "Colors differ: actual {:?} vs expected {:?}",
            actual,
            expected,
        );
    }

    // =======================================================================
    // new() - initial state
    // =======================================================================

    #[test]
    fn new_creates_empty_buffer() {
        let buf = FrameGlyphBuffer::new();
        assert!(buf.glyphs.is_empty());
        assert!(buf.window_regions.is_empty());
        assert!(buf.window_infos.is_empty());
        assert!(buf.faces.is_empty());
        assert!(buf.face_fonts.is_empty());
        assert!(buf.stipple_patterns.is_empty());
        assert!(buf.face_box_attrs.is_empty());
        assert!(buf.cursor_inverse.is_none());
        assert!(!buf.layout_changed);
    }

    #[test]
    fn new_has_correct_defaults() {
        let buf = FrameGlyphBuffer::new();
        assert_eq!(buf.width, 0.0);
        assert_eq!(buf.height, 0.0);
        assert_eq!(buf.char_width, 8.0);
        assert_eq!(buf.char_height, 16.0);
        assert_eq!(buf.font_pixel_size, 14.0);
        assert_color_eq(&buf.background, &Color::BLACK);
        assert_eq!(buf.frame_id, 0);
        assert_eq!(buf.parent_id, 0);
        assert_eq!(buf.background_alpha, 1.0);
        assert!(!buf.no_accept_focus);
    }

    #[test]
    fn new_is_empty_and_len_zero() {
        let buf = FrameGlyphBuffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
    }

    // =======================================================================
    // with_size()
    // =======================================================================

    #[test]
    fn with_size_sets_dimensions() {
        let buf = FrameGlyphBuffer::with_size(1920.0, 1080.0);
        assert_eq!(buf.width, 1920.0);
        assert_eq!(buf.height, 1080.0);
        // Everything else should match new()
        assert!(buf.glyphs.is_empty());
        assert_eq!(buf.char_width, 8.0);
    }

    // =======================================================================
    // clear_all()
    // =======================================================================

    #[test]
    fn clear_all_resets_glyphs_and_metadata() {
        let mut buf = FrameGlyphBuffer::new();

        // Populate some data
        buf.add_char('A', 0.0, 0.0, 8.0, 16.0, 12.0, false);
        buf.add_stretch(0.0, 0.0, 100.0, 16.0, Color::RED, 0, false);
        buf.add_cursor(1, 10.0, 20.0, 2.0, 16.0, 1, Color::WHITE);
        buf.add_window_info(
            1, 100, 0, 500, 1000,
            0.0, 0.0, 800.0, 600.0,
            20.0, true, false, 16.0,
            "test.rs".to_string(), false,
        );
        buf.set_cursor_inverse(10.0, 20.0, 8.0, 16.0, Color::WHITE, Color::BLACK);
        buf.stipple_patterns.insert(1, StipplePattern {
            width: 8,
            height: 8,
            bits: vec![0xAA; 8],
        });
        buf.face_box_attrs.insert(1, FaceBoxAttrs {
            box_type: 1,
            box_color: Color::RED,
            box_line_width: 1,
            box_corner_radius: 0,
        });

        assert!(!buf.glyphs.is_empty());
        assert!(!buf.window_infos.is_empty());

        buf.clear_all();

        assert!(buf.glyphs.is_empty());
        assert!(buf.window_regions.is_empty());
        assert!(buf.window_infos.is_empty());
        assert!(buf.cursor_inverse.is_none());
        assert!(buf.stipple_patterns.is_empty());
        assert!(buf.face_box_attrs.is_empty());
    }

    #[test]
    fn clear_all_preserves_frame_dimensions() {
        let mut buf = FrameGlyphBuffer::new();
        buf.begin_frame(1920.0, 1080.0, Color::BLUE);
        buf.add_char('X', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        buf.clear_all();

        // Dimensions and background should survive clear_all
        assert_eq!(buf.width, 1920.0);
        assert_eq!(buf.height, 1080.0);
        assert_color_eq(&buf.background, &Color::BLUE);
    }

    // =======================================================================
    // begin_frame()
    // =======================================================================

    #[test]
    fn begin_frame_sets_dimensions_and_background() {
        let mut buf = FrameGlyphBuffer::new();
        let bg = Color::rgb(0.1, 0.2, 0.3);
        buf.begin_frame(800.0, 600.0, bg);

        assert_eq!(buf.width, 800.0);
        assert_eq!(buf.height, 600.0);
        assert_color_eq(&buf.background, &bg);
    }

    #[test]
    fn begin_frame_clears_glyphs() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_char('Z', 5.0, 5.0, 8.0, 16.0, 12.0, false);
        assert_eq!(buf.len(), 1);

        buf.begin_frame(800.0, 600.0, Color::BLACK);
        assert!(buf.is_empty());
    }

    #[test]
    fn begin_frame_clears_cursor_inverse() {
        let mut buf = FrameGlyphBuffer::new();
        buf.set_cursor_inverse(0.0, 0.0, 8.0, 16.0, Color::WHITE, Color::BLACK);
        assert!(buf.cursor_inverse.is_some());

        buf.begin_frame(800.0, 600.0, Color::BLACK);
        assert!(buf.cursor_inverse.is_none());
    }

    #[test]
    fn begin_frame_clears_stipple_patterns_and_box_attrs() {
        let mut buf = FrameGlyphBuffer::new();
        buf.stipple_patterns.insert(1, StipplePattern {
            width: 4, height: 4, bits: vec![0xFF; 2],
        });
        buf.face_box_attrs.insert(1, FaceBoxAttrs {
            box_type: 1,
            box_color: Color::RED,
            box_line_width: 2,
            box_corner_radius: 0,
        });

        buf.begin_frame(800.0, 600.0, Color::BLACK);
        assert!(buf.stipple_patterns.is_empty());
        assert!(buf.face_box_attrs.is_empty());
    }

    #[test]
    fn begin_frame_then_add_then_begin_frame_clears_previous() {
        let mut buf = FrameGlyphBuffer::new();

        // First frame
        buf.begin_frame(800.0, 600.0, Color::BLACK);
        buf.add_char('A', 0.0, 0.0, 8.0, 16.0, 12.0, false);
        buf.add_char('B', 8.0, 0.0, 8.0, 16.0, 12.0, false);
        buf.add_cursor(1, 16.0, 0.0, 2.0, 16.0, 1, Color::WHITE);
        buf.add_stretch(0.0, 16.0, 800.0, 16.0, Color::BLACK, 0, false);
        buf.add_window_info(
            1, 100, 0, 100, 200,
            0.0, 0.0, 800.0, 600.0,
            20.0, true, false, 16.0,
            String::new(), false,
        );
        assert_eq!(buf.len(), 4);
        assert_eq!(buf.window_infos.len(), 1);

        // Second frame - should clear all glyphs
        buf.begin_frame(1024.0, 768.0, Color::WHITE);
        assert!(buf.is_empty());
        assert_eq!(buf.width, 1024.0);
        assert_eq!(buf.height, 768.0);
        assert_color_eq(&buf.background, &Color::WHITE);
        // Note: begin_frame does NOT clear window_infos (that's clear_all's job)
    }

    // =======================================================================
    // add_char()
    // =======================================================================

    #[test]
    fn add_char_appends_char_glyph() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_char('H', 10.0, 20.0, 8.0, 16.0, 12.0, false);

        assert_eq!(buf.len(), 1);
        match &buf.glyphs[0] {
            FrameGlyph::Char { char: ch, x, y, width, height, ascent, is_overlay, composed, .. } => {
                assert_eq!(*ch, 'H');
                assert_eq!(*x, 10.0);
                assert_eq!(*y, 20.0);
                assert_eq!(*width, 8.0);
                assert_eq!(*height, 16.0);
                assert_eq!(*ascent, 12.0);
                assert!(!*is_overlay);
                assert!(composed.is_none());
            }
            other => panic!("Expected Char glyph, got {:?}", other),
        }
    }

    #[test]
    fn add_char_uses_current_face_attributes() {
        let mut buf = FrameGlyphBuffer::new();
        let fg = Color::rgb(1.0, 0.0, 0.0);
        let bg = Color::rgb(0.0, 0.0, 1.0);
        buf.set_face(
            42, fg, Some(bg),
            700, true,
            1, Some(Color::GREEN),   // underline
            1, Some(Color::RED),     // strike-through
            1, Some(Color::BLUE),    // overline
        );
        buf.add_char('X', 0.0, 0.0, 8.0, 16.0, 12.0, true);

        match &buf.glyphs[0] {
            FrameGlyph::Char {
                fg: glyph_fg, bg: glyph_bg, face_id,
                font_weight, italic, underline, strike_through, overline,
                is_overlay, underline_color, strike_through_color, overline_color,
                ..
            } => {
                assert_color_eq(glyph_fg, &fg);
                assert_eq!(*glyph_bg, Some(bg));
                assert_eq!(*face_id, 42);
                assert_eq!(*font_weight, 700);
                assert!(*italic);
                assert_eq!(*underline, 1);
                assert_eq!(*underline_color, Some(Color::GREEN));
                assert_eq!(*strike_through, 1);
                assert_eq!(*strike_through_color, Some(Color::RED));
                assert_eq!(*overline, 1);
                assert_eq!(*overline_color, Some(Color::BLUE));
                assert!(*is_overlay);
            }
            other => panic!("Expected Char glyph, got {:?}", other),
        }
    }

    #[test]
    fn add_char_multiple_appends_in_order() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_char('A', 0.0, 0.0, 8.0, 16.0, 12.0, false);
        buf.add_char('B', 8.0, 0.0, 8.0, 16.0, 12.0, false);
        buf.add_char('C', 16.0, 0.0, 8.0, 16.0, 12.0, false);

        assert_eq!(buf.len(), 3);
        let chars: Vec<char> = buf.glyphs.iter().map(|g| {
            match g {
                FrameGlyph::Char { char: ch, .. } => *ch,
                _ => panic!("Expected Char"),
            }
        }).collect();
        assert_eq!(chars, vec!['A', 'B', 'C']);
    }

    #[test]
    fn add_char_overlay_flag() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_char('M', 0.0, 0.0, 8.0, 16.0, 12.0, true);
        assert!(buf.glyphs[0].is_overlay());

        buf.add_char('N', 0.0, 0.0, 8.0, 16.0, 12.0, false);
        assert!(!buf.glyphs[1].is_overlay());
    }

    // =======================================================================
    // add_composed_char()
    // =======================================================================

    #[test]
    fn add_composed_char_stores_text_and_base() {
        let mut buf = FrameGlyphBuffer::new();
        // Emoji ZWJ sequence: family emoji
        let composed_text = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
        buf.add_composed_char(composed_text, '\u{1F468}', 0.0, 0.0, 24.0, 16.0, 12.0, false);

        assert_eq!(buf.len(), 1);
        match &buf.glyphs[0] {
            FrameGlyph::Char { char: ch, composed, width, .. } => {
                assert_eq!(*ch, '\u{1F468}');
                assert!(composed.is_some());
                assert_eq!(&**composed.as_ref().unwrap(), composed_text);
                assert_eq!(*width, 24.0);
            }
            other => panic!("Expected Char glyph, got {:?}", other),
        }
    }

    #[test]
    fn add_composed_char_uses_current_face() {
        let mut buf = FrameGlyphBuffer::new();
        let fg = Color::rgb(0.5, 0.5, 0.5);
        buf.set_face(10, fg, None, 400, false, 0, None, 0, None, 0, None);
        buf.add_composed_char("e\u{0301}", 'e', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        match &buf.glyphs[0] {
            FrameGlyph::Char { face_id, fg: glyph_fg, bg: glyph_bg, .. } => {
                assert_eq!(*face_id, 10);
                assert_color_eq(glyph_fg, &fg);
                assert_eq!(*glyph_bg, None);
            }
            other => panic!("Expected Char glyph, got {:?}", other),
        }
    }

    // =======================================================================
    // add_cursor()
    // =======================================================================

    #[test]
    fn add_cursor_appends_cursor_glyph() {
        let mut buf = FrameGlyphBuffer::new();
        let cursor_color = Color::rgb(0.0, 1.0, 0.0);
        buf.add_cursor(42, 100.0, 200.0, 2.0, 16.0, 1, cursor_color);

        assert_eq!(buf.len(), 1);
        match &buf.glyphs[0] {
            FrameGlyph::Cursor { window_id, x, y, width, height, style, color } => {
                assert_eq!(*window_id, 42);
                assert_eq!(*x, 100.0);
                assert_eq!(*y, 200.0);
                assert_eq!(*width, 2.0);
                assert_eq!(*height, 16.0);
                assert_eq!(*style, 1); // bar
                assert_color_eq(color, &cursor_color);
            }
            other => panic!("Expected Cursor glyph, got {:?}", other),
        }
    }

    #[test]
    fn add_cursor_all_styles() {
        let mut buf = FrameGlyphBuffer::new();
        let c = Color::WHITE;
        buf.add_cursor(1, 0.0, 0.0, 8.0, 16.0, 0, c); // box
        buf.add_cursor(1, 0.0, 0.0, 2.0, 16.0, 1, c); // bar
        buf.add_cursor(1, 0.0, 0.0, 8.0, 2.0, 2, c);  // hbar
        buf.add_cursor(1, 0.0, 0.0, 8.0, 16.0, 3, c);  // hollow

        assert_eq!(buf.len(), 4);
        for (i, expected_style) in [0u8, 1, 2, 3].iter().enumerate() {
            match &buf.glyphs[i] {
                FrameGlyph::Cursor { style, .. } => {
                    assert_eq!(style, expected_style, "Cursor {} has wrong style", i);
                }
                other => panic!("Expected Cursor at index {}, got {:?}", i, other),
            }
        }
    }

    #[test]
    fn cursor_glyph_is_not_overlay() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_cursor(1, 0.0, 0.0, 8.0, 16.0, 0, Color::WHITE);
        assert!(!buf.glyphs[0].is_overlay());
    }

    // =======================================================================
    // add_stretch()
    // =======================================================================

    #[test]
    fn add_stretch_appends_stretch_glyph() {
        let mut buf = FrameGlyphBuffer::new();
        let bg = Color::rgb(0.2, 0.2, 0.2);
        buf.add_stretch(0.0, 100.0, 800.0, 16.0, bg, 5, false);

        assert_eq!(buf.len(), 1);
        match &buf.glyphs[0] {
            FrameGlyph::Stretch { x, y, width, height, bg: stretch_bg, face_id, is_overlay, stipple_id, stipple_fg } => {
                assert_eq!(*x, 0.0);
                assert_eq!(*y, 100.0);
                assert_eq!(*width, 800.0);
                assert_eq!(*height, 16.0);
                assert_color_eq(stretch_bg, &bg);
                assert_eq!(*face_id, 5);
                assert!(!*is_overlay);
                assert_eq!(*stipple_id, 0);
                assert!(stipple_fg.is_none());
            }
            other => panic!("Expected Stretch glyph, got {:?}", other),
        }
    }

    #[test]
    fn add_stretch_overlay() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_stretch(0.0, 0.0, 800.0, 20.0, Color::BLUE, 0, true);
        assert!(buf.glyphs[0].is_overlay());
    }

    #[test]
    fn add_stretch_stipple_stores_pattern_info() {
        let mut buf = FrameGlyphBuffer::new();
        let bg = Color::BLACK;
        let fg = Color::WHITE;
        buf.add_stretch_stipple(0.0, 0.0, 100.0, 100.0, bg, fg, 3, false, 7);

        assert_eq!(buf.len(), 1);
        match &buf.glyphs[0] {
            FrameGlyph::Stretch { stipple_id, stipple_fg, .. } => {
                assert_eq!(*stipple_id, 7);
                assert_eq!(*stipple_fg, Some(fg));
            }
            other => panic!("Expected Stretch glyph, got {:?}", other),
        }
    }

    // =======================================================================
    // add_window_info()
    // =======================================================================

    #[test]
    fn add_window_info_appends_metadata() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_window_info(
            0x1234, 0xABCD, 1, 500, 1000,
            10.0, 20.0, 780.0, 560.0,
            22.0, true, false, 16.0,
            "main.rs".to_string(), true,
        );

        assert_eq!(buf.window_infos.len(), 1);
        let info = &buf.window_infos[0];
        assert_eq!(info.window_id, 0x1234);
        assert_eq!(info.buffer_id, 0xABCD);
        assert_eq!(info.window_start, 1);
        assert_eq!(info.window_end, 500);
        assert_eq!(info.buffer_size, 1000);
        assert_eq!(info.bounds, Rect::new(10.0, 20.0, 780.0, 560.0));
        assert_eq!(info.mode_line_height, 22.0);
        assert!(info.selected);
        assert!(!info.is_minibuffer);
        assert_eq!(info.char_height, 16.0);
        assert_eq!(info.buffer_file_name, "main.rs");
        assert!(info.modified);
    }

    #[test]
    fn add_window_info_multiple_windows() {
        let mut buf = FrameGlyphBuffer::new();

        // Two side-by-side windows
        buf.add_window_info(
            1, 100, 0, 200, 500,
            0.0, 0.0, 400.0, 600.0,
            20.0, true, false, 16.0,
            "left.rs".to_string(), false,
        );
        buf.add_window_info(
            2, 200, 0, 300, 800,
            400.0, 0.0, 400.0, 600.0,
            20.0, false, false, 16.0,
            "right.rs".to_string(), true,
        );

        assert_eq!(buf.window_infos.len(), 2);
        assert_eq!(buf.window_infos[0].window_id, 1);
        assert!(buf.window_infos[0].selected);
        assert_eq!(buf.window_infos[1].window_id, 2);
        assert!(!buf.window_infos[1].selected);
    }

    #[test]
    fn add_window_info_minibuffer() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_window_info(
            99, 50, 0, 0, 0,
            0.0, 580.0, 800.0, 20.0,
            0.0, false, true, 16.0,
            String::new(), false,
        );

        let info = &buf.window_infos[0];
        assert!(info.is_minibuffer);
        assert!(!info.selected);
        assert_eq!(info.buffer_file_name, "");
    }

    // =======================================================================
    // set_face() / set_face_with_font()
    // =======================================================================

    #[test]
    fn set_face_affects_subsequent_chars() {
        let mut buf = FrameGlyphBuffer::new();

        // Default face
        buf.add_char('A', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        // Change face
        let red = Color::rgb(1.0, 0.0, 0.0);
        buf.set_face(5, red, None, 700, true, 0, None, 0, None, 0, None);
        buf.add_char('B', 8.0, 0.0, 8.0, 16.0, 12.0, false);

        // First char uses default face
        match &buf.glyphs[0] {
            FrameGlyph::Char { face_id, font_weight, italic, .. } => {
                assert_eq!(*face_id, 0);
                assert_eq!(*font_weight, 400);
                assert!(!*italic);
            }
            _ => panic!("Expected Char"),
        }

        // Second char uses newly set face
        match &buf.glyphs[1] {
            FrameGlyph::Char { face_id, font_weight, italic, fg, .. } => {
                assert_eq!(*face_id, 5);
                assert_eq!(*font_weight, 700);
                assert!(*italic);
                assert_color_eq(fg, &red);
            }
            _ => panic!("Expected Char"),
        }
    }

    #[test]
    fn set_face_with_font_stores_font_family() {
        let mut buf = FrameGlyphBuffer::new();
        let fg = Color::WHITE;
        buf.set_face_with_font(
            7, fg, None,
            "Fira Code", 400, false, 14.0,
            0, None, 0, None, 0, None,
        );

        // face_fonts map should have the entry
        assert_eq!(buf.get_face_font(7), "Fira Code");
        assert_eq!(buf.get_current_font_family(), "Fira Code");
    }

    #[test]
    fn set_face_with_font_updates_font_size() {
        let mut buf = FrameGlyphBuffer::new();
        buf.set_face_with_font(
            1, Color::WHITE, None,
            "monospace", 400, false, 24.0,
            0, None, 0, None, 0, None,
        );
        buf.add_char('A', 0.0, 0.0, 12.0, 24.0, 18.0, false);

        match &buf.glyphs[0] {
            FrameGlyph::Char { font_size, .. } => {
                assert_eq!(*font_size, 24.0);
            }
            _ => panic!("Expected Char"),
        }
    }

    #[test]
    fn set_face_preserves_font_family_from_previous_set_face_with_font() {
        let mut buf = FrameGlyphBuffer::new();

        // First set with font
        buf.set_face_with_font(
            1, Color::WHITE, None,
            "JetBrains Mono", 400, false, 14.0,
            0, None, 0, None, 0, None,
        );
        assert_eq!(buf.get_face_font(1), "JetBrains Mono");

        // Then set_face (without font) for a different face_id
        buf.set_face(2, Color::WHITE, None, 400, false, 0, None, 0, None, 0, None);

        // face_id 1 font should still be accessible
        assert_eq!(buf.get_face_font(1), "JetBrains Mono");
        // face_id 2 font should be default
        assert_eq!(buf.get_face_font(2), "monospace");
    }

    #[test]
    fn set_face_with_font_decoration_attributes() {
        let mut buf = FrameGlyphBuffer::new();
        let ul_color = Color::rgb(1.0, 1.0, 0.0);
        let st_color = Color::rgb(1.0, 0.0, 1.0);
        let ol_color = Color::rgb(0.0, 1.0, 1.0);
        buf.set_face_with_font(
            3, Color::WHITE, None,
            "monospace", 400, false, 14.0,
            2, Some(ul_color),  // wave underline
            1, Some(st_color),  // strike-through
            1, Some(ol_color),  // overline
        );
        buf.add_char('D', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        match &buf.glyphs[0] {
            FrameGlyph::Char {
                underline, underline_color,
                strike_through, strike_through_color,
                overline, overline_color,
                ..
            } => {
                assert_eq!(*underline, 2);
                assert_eq!(*underline_color, Some(ul_color));
                assert_eq!(*strike_through, 1);
                assert_eq!(*strike_through_color, Some(st_color));
                assert_eq!(*overline, 1);
                assert_eq!(*overline_color, Some(ol_color));
            }
            _ => panic!("Expected Char"),
        }
    }

    #[test]
    fn get_current_bg_returns_current_face_bg() {
        let mut buf = FrameGlyphBuffer::new();
        assert_eq!(buf.get_current_bg(), None);

        let bg = Color::rgb(0.1, 0.2, 0.3);
        buf.set_face(1, Color::WHITE, Some(bg), 400, false, 0, None, 0, None, 0, None);
        assert_eq!(buf.get_current_bg(), Some(bg));
    }

    // =======================================================================
    // set_frame_identity()
    // =======================================================================

    #[test]
    fn set_frame_identity_stores_all_fields() {
        let mut buf = FrameGlyphBuffer::new();
        let border_color = Color::rgb(0.5, 0.5, 0.5);
        buf.set_frame_identity(
            0x100, 0x200,
            50.0, 75.0,
            5,
            2.0, border_color,
            true,
            0.85,
        );

        assert_eq!(buf.frame_id, 0x100);
        assert_eq!(buf.parent_id, 0x200);
        assert_eq!(buf.parent_x, 50.0);
        assert_eq!(buf.parent_y, 75.0);
        assert_eq!(buf.z_order, 5);
        assert_eq!(buf.border_width, 2.0);
        assert_color_eq(&buf.border_color, &border_color);
        assert!(buf.no_accept_focus);
        assert_eq!(buf.background_alpha, 0.85);
    }

    #[test]
    fn set_frame_identity_root_frame() {
        let mut buf = FrameGlyphBuffer::new();
        buf.set_frame_identity(
            0x100, 0, // parent_id 0 = root frame
            0.0, 0.0,
            0,
            0.0, Color::BLACK,
            false,
            1.0,
        );

        assert_eq!(buf.frame_id, 0x100);
        assert_eq!(buf.parent_id, 0);
        assert!(!buf.no_accept_focus);
        assert_eq!(buf.background_alpha, 1.0);
    }

    // =======================================================================
    // set_cursor_inverse()
    // =======================================================================

    #[test]
    fn set_cursor_inverse_stores_info() {
        let mut buf = FrameGlyphBuffer::new();
        let cursor_bg = Color::rgb(0.9, 0.9, 0.0);
        let cursor_fg = Color::rgb(0.0, 0.0, 0.0);
        buf.set_cursor_inverse(50.0, 100.0, 8.0, 16.0, cursor_bg, cursor_fg);

        assert!(buf.cursor_inverse.is_some());
        let inv = buf.cursor_inverse.as_ref().unwrap();
        assert_eq!(inv.x, 50.0);
        assert_eq!(inv.y, 100.0);
        assert_eq!(inv.width, 8.0);
        assert_eq!(inv.height, 16.0);
        assert_color_eq(&inv.cursor_bg, &cursor_bg);
        assert_color_eq(&inv.cursor_fg, &cursor_fg);
    }

    // =======================================================================
    // font_size() / set_font_size()
    // =======================================================================

    #[test]
    fn font_size_accessors() {
        let mut buf = FrameGlyphBuffer::new();
        assert_eq!(buf.font_size(), 14.0); // default

        buf.set_font_size(20.0);
        assert_eq!(buf.font_size(), 20.0);

        // Affects subsequently added chars
        buf.add_char('X', 0.0, 0.0, 10.0, 20.0, 15.0, false);
        match &buf.glyphs[0] {
            FrameGlyph::Char { font_size, .. } => assert_eq!(*font_size, 20.0),
            _ => panic!("Expected Char"),
        }
    }

    // =======================================================================
    // start_frame() / end_frame() / take_layout_changed()
    // =======================================================================

    #[test]
    fn start_frame_swaps_window_regions() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_background(0.0, 0.0, 400.0, 300.0, Color::BLACK);
        buf.add_background(400.0, 0.0, 400.0, 300.0, Color::BLACK);
        assert_eq!(buf.window_regions.len(), 2);
        assert!(buf.prev_window_regions.is_empty());

        buf.start_frame();
        // Previous regions moved to prev, current cleared
        assert_eq!(buf.prev_window_regions.len(), 2);
        assert!(buf.window_regions.is_empty());
    }

    #[test]
    fn end_frame_returns_false() {
        let mut buf = FrameGlyphBuffer::new();
        assert!(!buf.end_frame());
    }

    #[test]
    fn take_layout_changed_returns_and_resets() {
        let mut buf = FrameGlyphBuffer::new();
        assert!(!buf.take_layout_changed());

        buf.layout_changed = true;
        assert!(buf.take_layout_changed());
        assert!(!buf.take_layout_changed()); // second call returns false
    }

    // =======================================================================
    // add_background()
    // =======================================================================

    #[test]
    fn add_background_adds_glyph_and_window_region() {
        let mut buf = FrameGlyphBuffer::new();
        let bg = Color::rgb(0.15, 0.15, 0.15);
        buf.add_background(10.0, 20.0, 780.0, 560.0, bg);

        assert_eq!(buf.len(), 1);
        assert_eq!(buf.window_regions.len(), 1);
        assert_eq!(buf.window_regions[0], Rect::new(10.0, 20.0, 780.0, 560.0));

        match &buf.glyphs[0] {
            FrameGlyph::Background { bounds, color } => {
                assert_eq!(*bounds, Rect::new(10.0, 20.0, 780.0, 560.0));
                assert_color_eq(color, &bg);
            }
            other => panic!("Expected Background glyph, got {:?}", other),
        }
    }

    // =======================================================================
    // add_border()
    // =======================================================================

    #[test]
    fn add_border_appends_border_glyph() {
        let mut buf = FrameGlyphBuffer::new();
        let border_color = Color::rgb(0.3, 0.3, 0.3);
        buf.add_border(400.0, 0.0, 1.0, 600.0, border_color);

        assert_eq!(buf.len(), 1);
        match &buf.glyphs[0] {
            FrameGlyph::Border { x, y, width, height, color } => {
                assert_eq!(*x, 400.0);
                assert_eq!(*y, 0.0);
                assert_eq!(*width, 1.0);
                assert_eq!(*height, 600.0);
                assert_color_eq(color, &border_color);
            }
            other => panic!("Expected Border glyph, got {:?}", other),
        }
    }

    #[test]
    fn border_glyph_is_not_overlay() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_border(0.0, 0.0, 1.0, 100.0, Color::WHITE);
        assert!(!buf.glyphs[0].is_overlay());
    }

    // =======================================================================
    // add_image() / add_video() / add_webkit()
    // =======================================================================

    #[test]
    fn add_image_appends_image_glyph() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_image(42, 100.0, 200.0, 320.0, 240.0);

        assert_eq!(buf.len(), 1);
        match &buf.glyphs[0] {
            FrameGlyph::Image { image_id, x, y, width, height } => {
                assert_eq!(*image_id, 42);
                assert_eq!(*x, 100.0);
                assert_eq!(*y, 200.0);
                assert_eq!(*width, 320.0);
                assert_eq!(*height, 240.0);
            }
            other => panic!("Expected Image glyph, got {:?}", other),
        }
    }

    #[test]
    fn add_video_appends_video_glyph() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_video(7, 0.0, 0.0, 640.0, 480.0);

        match &buf.glyphs[0] {
            FrameGlyph::Video { video_id, .. } => assert_eq!(*video_id, 7),
            other => panic!("Expected Video glyph, got {:?}", other),
        }
    }

    #[test]
    fn add_webkit_appends_webkit_glyph() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_webkit(99, 0.0, 0.0, 800.0, 600.0);

        match &buf.glyphs[0] {
            FrameGlyph::WebKit { webkit_id, .. } => assert_eq!(*webkit_id, 99),
            other => panic!("Expected WebKit glyph, got {:?}", other),
        }
    }

    // =======================================================================
    // add_scroll_bar()
    // =======================================================================

    #[test]
    fn add_scroll_bar_appends_scrollbar_glyph() {
        let mut buf = FrameGlyphBuffer::new();
        let track = Color::rgb(0.1, 0.1, 0.1);
        let thumb = Color::rgb(0.5, 0.5, 0.5);
        buf.add_scroll_bar(false, 790.0, 0.0, 10.0, 600.0, 50.0, 100.0, track, thumb);

        assert_eq!(buf.len(), 1);
        match &buf.glyphs[0] {
            FrameGlyph::ScrollBar {
                horizontal, x, y, width, height,
                thumb_start, thumb_size, track_color, thumb_color,
            } => {
                assert!(!*horizontal);
                assert_eq!(*x, 790.0);
                assert_eq!(*y, 0.0);
                assert_eq!(*width, 10.0);
                assert_eq!(*height, 600.0);
                assert_eq!(*thumb_start, 50.0);
                assert_eq!(*thumb_size, 100.0);
                assert_color_eq(track_color, &track);
                assert_color_eq(thumb_color, &thumb);
            }
            other => panic!("Expected ScrollBar glyph, got {:?}", other),
        }
    }

    // =======================================================================
    // is_overlay() dispatch
    // =======================================================================

    #[test]
    fn is_overlay_returns_false_for_non_char_stretch_types() {
        let mut buf = FrameGlyphBuffer::new();
        buf.add_border(0.0, 0.0, 1.0, 100.0, Color::WHITE);
        buf.add_cursor(1, 0.0, 0.0, 8.0, 16.0, 0, Color::WHITE);
        buf.add_image(1, 0.0, 0.0, 100.0, 100.0);

        for glyph in &buf.glyphs {
            assert!(!glyph.is_overlay());
        }
    }

    // =======================================================================
    // Full frame simulation: realistic multi-window frame
    // =======================================================================

    #[test]
    fn full_frame_simulation() {
        let mut buf = FrameGlyphBuffer::new();
        let frame_bg = Color::rgb(0.12, 0.12, 0.12);

        // Begin frame
        buf.begin_frame(1920.0, 1080.0, frame_bg);
        buf.set_frame_identity(0x1, 0, 0.0, 0.0, 0, 0.0, Color::BLACK, false, 1.0);

        // Window 1: left pane background
        let win_bg = Color::rgb(0.13, 0.13, 0.13);
        buf.add_background(0.0, 0.0, 960.0, 1060.0, win_bg);

        // Window 1: some text
        let text_fg = Color::rgb(0.87, 0.87, 0.87);
        buf.set_face_with_font(
            0, text_fg, None,
            "Iosevka", 400, false, 14.0,
            0, None, 0, None, 0, None,
        );
        for (i, ch) in "Hello, Neomacs!".chars().enumerate() {
            buf.add_char(ch, i as f32 * 8.0, 0.0, 8.0, 16.0, 12.0, false);
        }

        // Window 1: cursor
        buf.add_cursor(1, 15.0 * 8.0, 0.0, 2.0, 16.0, 1, Color::WHITE);
        buf.set_cursor_inverse(15.0 * 8.0, 0.0, 8.0, 16.0, Color::WHITE, Color::BLACK);

        // Vertical border
        buf.add_border(960.0, 0.0, 1.0, 1060.0, Color::rgb(0.3, 0.3, 0.3));

        // Window 2: right pane background
        buf.add_background(961.0, 0.0, 959.0, 1060.0, win_bg);

        // Mode-line (overlay)
        let ml_bg = Color::rgb(0.2, 0.2, 0.3);
        buf.set_face(10, Color::WHITE, Some(ml_bg), 700, false, 0, None, 0, None, 0, None);
        buf.add_stretch(0.0, 1060.0, 1920.0, 20.0, ml_bg, 10, true);

        // Window infos
        buf.add_window_info(
            1, 100, 0, 500, 1000,
            0.0, 0.0, 960.0, 1060.0,
            20.0, true, false, 16.0,
            "left.rs".to_string(), false,
        );
        buf.add_window_info(
            2, 200, 0, 300, 800,
            961.0, 0.0, 959.0, 1060.0,
            20.0, false, false, 16.0,
            "right.rs".to_string(), true,
        );

        // Verify totals
        // 15 chars + 1 cursor + 2 backgrounds + 1 border + 1 mode-line stretch = 20
        assert_eq!(buf.len(), 20);
        assert_eq!(buf.window_infos.len(), 2);
        assert_eq!(buf.window_regions.len(), 2);
        assert!(buf.cursor_inverse.is_some());
        assert_eq!(buf.frame_id, 0x1);
        assert_eq!(buf.width, 1920.0);
        assert_eq!(buf.height, 1080.0);

        // Verify overlay count
        let overlay_count = buf.glyphs.iter().filter(|g| g.is_overlay()).count();
        assert_eq!(overlay_count, 1); // just the mode-line stretch
    }
}

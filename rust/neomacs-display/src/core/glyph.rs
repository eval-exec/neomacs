//! Glyph types matching Emacs display model.

use crate::core::types::{Color, Rect};

/// Glyph types - matches Emacs `enum glyph_type`
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphType {
    /// Regular character glyph
    Char = 0,
    /// Composed character (ligatures, complex scripts)
    Composite = 1,
    /// Character that can't be displayed (replacement shown)
    Glyphless = 2,
    /// Image (PNG, JPEG, SVG, etc.)
    Image = 3,
    /// Stretch/whitespace glyph
    Stretch = 4,
    /// Embedded widget (xwidget)
    XWidget = 5,
    /// Video frame (new in Neomacs)
    Video = 6,
    /// WPE WebKit view (new in Neomacs)
    Wpe = 7,
}

impl Default for GlyphType {
    fn default() -> Self {
        Self::Char
    }
}

/// A single glyph in the display
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Glyph {
    /// Type of this glyph
    pub glyph_type: GlyphType,

    /// Character code (for Char glyphs)
    pub charcode: u32,

    /// Face ID for styling
    pub face_id: u32,

    /// X position in pixels (for position-based replacement)
    pub x: i32,

    /// Width in pixels
    pub pixel_width: i32,

    /// Ascent (pixels above baseline)
    pub ascent: i32,

    /// Descent (pixels below baseline)
    pub descent: i32,

    /// Position in buffer (charpos)
    pub charpos: i64,

    /// Left box line present
    pub left_box_line: bool,

    /// Right box line present
    pub right_box_line: bool,

    /// Padding glyph (not real character)
    pub padding: bool,

    /// Type-specific data
    pub data: GlyphData,
}

impl Default for Glyph {
    fn default() -> Self {
        Self {
            glyph_type: GlyphType::Char,
            charcode: 0,
            face_id: 0,
            x: 0,
            pixel_width: 0,
            ascent: 0,
            descent: 0,
            charpos: 0,
            left_box_line: false,
            right_box_line: false,
            padding: false,
            data: GlyphData::None,
        }
    }
}

impl Glyph {
    /// Create a character glyph with explicit dimensions
    pub fn char(code: char, face_id: u32, width: i32, ascent: i32, descent: i32) -> Self {
        Self {
            glyph_type: GlyphType::Char,
            charcode: code as u32,
            face_id,
            pixel_width: width,
            ascent,
            descent,
            data: GlyphData::Char { code },
            ..Default::default()
        }
    }

    /// Create a character glyph with default sizing (for later measurement)
    pub fn char_simple(code: char, face_id: u32) -> Self {
        Self {
            glyph_type: GlyphType::Char,
            charcode: code as u32,
            face_id,
            pixel_width: 0, // Will be measured by renderer
            ascent: 0,
            descent: 0,
            data: GlyphData::Char { code },
            ..Default::default()
        }
    }

    /// Create an image glyph
    pub fn image(image_id: u32, width: i32, height: i32) -> Self {
        Self {
            glyph_type: GlyphType::Image,
            pixel_width: width,
            ascent: height,
            descent: 0,
            data: GlyphData::Image { image_id },
            ..Default::default()
        }
    }

    /// Create a video glyph
    pub fn video(video_id: u32, width: i32, height: i32) -> Self {
        Self {
            glyph_type: GlyphType::Video,
            pixel_width: width,
            ascent: height,
            descent: 0,
            data: GlyphData::Video { video_id },
            ..Default::default()
        }
    }

    /// Create a WPE WebKit glyph
    pub fn wpe(view_id: u32, width: i32, height: i32) -> Self {
        Self {
            glyph_type: GlyphType::Wpe,
            pixel_width: width,
            ascent: height,
            descent: 0,
            data: GlyphData::Wpe { view_id },
            ..Default::default()
        }
    }

    /// Create a stretch (whitespace) glyph
    pub fn stretch(width: i32, height: i32) -> Self {
        Self {
            glyph_type: GlyphType::Stretch,
            pixel_width: width,
            ascent: height,
            descent: 0,
            data: GlyphData::Stretch { width },
            ..Default::default()
        }
    }

    /// Total height (ascent + descent)
    pub fn height(&self) -> i32 {
        self.ascent + self.descent
    }
}

/// Type-specific glyph data
#[derive(Debug, Clone)]
pub enum GlyphData {
    None,
    Char { code: char },
    Image { image_id: u32 },
    Video { video_id: u32 },
    Wpe { view_id: u32 },
    Stretch { width: i32 },
    Composite { components: Vec<u32> },
}

impl Default for GlyphData {
    fn default() -> Self {
        Self::None
    }
}

/// A row of glyphs
#[derive(Debug, Clone, Default)]
pub struct GlyphRow {
    /// Glyphs in the text area
    pub glyphs: Vec<Glyph>,

    /// Y position of this row
    pub y: i32,

    /// Height of row in pixels
    pub height: i32,

    /// Visible height (may be clipped)
    pub visible_height: i32,

    /// Ascent (baseline position from top)
    pub ascent: i32,

    /// Is this row enabled (should be displayed)?
    pub enabled: bool,

    /// Row displays cursor?
    pub cursor_in_row: bool,

    /// Mode line row?
    pub mode_line: bool,

    /// Header line row?
    pub header_line: bool,

    /// Frame counter when this row was last cleared (for incremental updates)
    pub last_frame_cleared: u64,
}

impl GlyphRow {
    pub fn new(y: i32, height: i32, ascent: i32) -> Self {
        Self {
            y,
            height,
            visible_height: height,
            ascent,
            enabled: true,
            ..Default::default()
        }
    }

    /// Total width of all glyphs
    pub fn width(&self) -> i32 {
        self.glyphs.iter().map(|g| g.pixel_width).sum()
    }

    /// Add a glyph to the row
    pub fn push(&mut self, glyph: Glyph) {
        self.glyphs.push(glyph);
    }
}

/// A glyph string - contiguous glyphs with same face
#[derive(Debug, Clone)]
pub struct GlyphString {
    /// The glyphs
    pub glyphs: Vec<Glyph>,

    /// Face ID (same for all glyphs)
    pub face_id: u32,

    /// X position
    pub x: i32,

    /// Y position (baseline)
    pub y: i32,

    /// Total width
    pub width: i32,

    /// Background color
    pub background: Color,

    /// Foreground color
    pub foreground: Color,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glyph_creation() {
        let g = Glyph::char('A', 0, 10, 12, 4);
        assert_eq!(g.glyph_type, GlyphType::Char);
        assert_eq!(g.pixel_width, 10);
        assert_eq!(g.height(), 16);
    }

    #[test]
    fn test_glyph_row() {
        let mut row = GlyphRow::new(0, 20, 16);
        row.push(Glyph::char('H', 0, 10, 12, 4));
        row.push(Glyph::char('i', 0, 5, 12, 4));
        assert_eq!(row.width(), 15);
    }
}

//! Types for the Rust layout engine.
//!
//! These are the intermediate representation between buffer data and rendering.
//! The layout engine produces LayoutOutput which is then converted to
//! FrameGlyphBuffer for the existing renderer.

use crate::core::types::{Color, Rect};

/// Complete layout output for one frame.
/// Produced by the layout engine, consumed by the renderer.
#[derive(Debug, Clone)]
pub struct LayoutOutput {
    /// Frame dimensions in pixels
    pub width: f32,
    pub height: f32,

    /// Frame background color (sRGB, will be converted to linear)
    pub background: Color,

    /// Default character cell dimensions
    pub char_width: f32,
    pub char_height: f32,
    pub font_pixel_size: f32,

    /// Laid-out windows
    pub windows: Vec<WindowLayout>,
}

/// Layout output for a single window.
#[derive(Debug, Clone)]
pub struct WindowLayout {
    /// Window identifier (pointer cast to i64)
    pub window_id: i64,
    /// Buffer identifier (pointer cast to u64)
    pub buffer_id: u64,
    /// Frame-absolute bounds of this window
    pub bounds: Rect,
    /// Whether this is the selected (active) window
    pub selected: bool,
    /// First visible buffer position
    pub window_start: i64,
    /// Mode-line height in pixels
    pub mode_line_height: f32,

    /// Laid-out rows (visual lines)
    pub rows: Vec<LayoutRow>,

    /// Cursor position (if visible in this window)
    pub cursor: Option<CursorLayout>,

    /// Last visible buffer position (for window-end feedback)
    pub window_end_pos: i64,
}

/// A single laid-out visual line.
#[derive(Debug, Clone)]
pub struct LayoutRow {
    /// Glyphs on this row
    pub glyphs: Vec<LayoutGlyph>,
    /// Frame-absolute Y position
    pub y: f32,
    /// Row height in pixels
    pub height: f32,
    /// Font ascent for this row
    pub ascent: f32,
    /// Whether this is a mode-line/header-line/tab-line row
    pub is_mode_line: bool,
}

/// A single laid-out glyph.
#[derive(Debug, Clone)]
pub enum LayoutGlyph {
    /// Character glyph
    Char {
        /// The character
        ch: char,
        /// Frame-absolute X position
        x: f32,
        /// Pixel width
        width: f32,
        /// Face ID
        face_id: u32,
        /// Buffer position this glyph represents
        charpos: i64,
    },

    /// Stretch (whitespace) glyph
    Stretch {
        /// Frame-absolute X position
        x: f32,
        /// Pixel width
        width: f32,
        /// Face ID
        face_id: u32,
    },

    /// Image glyph
    Image {
        /// GPU image ID
        image_id: u32,
        /// Frame-absolute X position
        x: f32,
        /// Pixel width
        width: f32,
        /// Pixel height
        height: f32,
    },
}

/// Cursor layout information.
#[derive(Debug, Clone)]
pub struct CursorLayout {
    /// Frame-absolute X position
    pub x: f32,
    /// Frame-absolute Y position
    pub y: f32,
    /// Width in pixels
    pub width: f32,
    /// Height in pixels
    pub height: f32,
    /// Cursor style: 0=box, 1=bar, 2=hbar, 3=hollow
    pub style: u8,
    /// Cursor color
    pub color: Color,
    /// Character under cursor (for inverse video with filled box)
    pub char_under: Option<char>,
    /// Face ID of character under cursor
    pub char_face_id: Option<u32>,
}

/// Parameters for a window that the layout engine needs.
/// Populated from Emacs data via FFI before layout runs.
#[derive(Debug, Clone)]
pub struct WindowParams {
    /// Window identifier (pointer value)
    pub window_id: i64,
    /// Buffer identifier (pointer value)
    pub buffer_id: u64,
    /// Frame-absolute bounds
    pub bounds: Rect,
    /// Text area bounds (excludes fringes, margins, scroll bars)
    pub text_bounds: Rect,
    /// Whether this is the selected window
    pub selected: bool,

    /// First visible buffer position (marker_position(w->start))
    pub window_start: i64,
    /// Point position in this window's buffer
    pub point: i64,
    /// Buffer size (ZV - narrowing end)
    pub buffer_size: i64,
    /// Buffer beginning (BEGV - narrowing start)
    pub buffer_begv: i64,

    /// Horizontal scroll offset in columns
    pub hscroll: i32,

    /// Whether to truncate long lines
    pub truncate_lines: bool,
    /// Whether to wrap at word boundaries
    pub word_wrap: bool,
    /// Tab width in columns
    pub tab_width: i32,

    /// Default face foreground/background for this window
    pub default_fg: u32,
    pub default_bg: u32,

    /// Character cell dimensions
    pub char_width: f32,
    pub char_height: f32,
    /// Font pixel size
    pub font_pixel_size: f32,
    /// Font ascent
    pub font_ascent: f32,

    /// Mode-line height (0 if no mode-line)
    pub mode_line_height: f32,
    /// Header-line height (0 if no header-line)
    pub header_line_height: f32,
    /// Tab-line height (0 if no tab-line)
    pub tab_line_height: f32,

    /// Cursor type for this window: 0=box, 1=bar, 2=hbar, 3=hollow
    pub cursor_type: u8,
    /// Cursor bar width (for bar cursor)
    pub cursor_bar_width: i32,

    /// Fringe widths in pixels
    pub left_fringe_width: f32,
    pub right_fringe_width: f32,
    /// indicate-empty-lines: 0=off, 1=left, 2=right
    pub indicate_empty_lines: i32,
    /// Whether to show trailing whitespace
    pub show_trailing_whitespace: bool,
    /// Trailing-whitespace face background color
    pub trailing_ws_bg: u32,
    /// Fill-column-indicator column (0 = off)
    pub fill_column_indicator: i32,
    /// Fill-column-indicator character
    pub fill_column_indicator_char: char,
    /// Fill-column-indicator face foreground color
    pub fill_column_indicator_fg: u32,
    /// Extra line spacing in pixels
    pub extra_line_spacing: f32,
    /// Whether to show cursor in non-selected windows
    pub cursor_in_non_selected: bool,
}

/// Frame-level parameters for layout.
#[derive(Debug, Clone)]
pub struct FrameParams {
    /// Frame pixel dimensions
    pub width: f32,
    pub height: f32,
    /// Default character cell dimensions
    pub char_width: f32,
    pub char_height: f32,
    /// Font pixel size
    pub font_pixel_size: f32,
    /// Frame background color (sRGB pixel)
    pub background: u32,
    /// Vertical border face foreground color (sRGB pixel)
    pub vertical_border_fg: u32,
}

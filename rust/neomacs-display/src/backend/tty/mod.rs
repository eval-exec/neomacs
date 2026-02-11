//! Terminal/TTY display backend.
//!
//! Renders Neomacs frames to a terminal using ANSI escape sequences.
//! Supports:
//! - Alternate screen buffer (smcup/rmcup)
//! - Raw mode terminal setup/teardown
//! - 256-color and 24-bit true color via ANSI SGR
//! - Cursor positioning
//! - Frame diffing: maintains a previous-frame grid and only outputs changed cells
//! - Basic text rendering from Scene and FrameGlyphBuffer data

use std::io::{self, Write};

use crate::backend::DisplayBackend;
use crate::core::error::{DisplayError, DisplayResult};
use crate::core::frame_glyphs::{FrameGlyph, FrameGlyphBuffer};
use crate::core::scene::Scene;
use crate::core::types::Color;

// ---------------------------------------------------------------------------
// ANSI escape sequence helpers
// ---------------------------------------------------------------------------

/// ANSI escape sequence constants and builders.
///
/// All sequences target xterm-compatible terminals (virtually all modern
/// terminals). No terminfo dependency is required.
pub mod ansi {
    /// CSI (Control Sequence Introducer)
    pub const CSI: &str = "\x1b[";

    /// Enter alternate screen buffer (smcup equivalent)
    pub const ENTER_ALT_SCREEN: &str = "\x1b[?1049h";
    /// Leave alternate screen buffer (rmcup equivalent)
    pub const LEAVE_ALT_SCREEN: &str = "\x1b[?1049l";

    /// Hide cursor
    pub const HIDE_CURSOR: &str = "\x1b[?25l";
    /// Show cursor
    pub const SHOW_CURSOR: &str = "\x1b[?25h";

    /// Reset all SGR attributes
    pub const SGR_RESET: &str = "\x1b[0m";

    /// Clear entire screen
    pub const CLEAR_SCREEN: &str = "\x1b[2J";

    /// Move cursor to home position (1,1)
    pub const CURSOR_HOME: &str = "\x1b[H";

    /// Enable mouse tracking (SGR extended mode)
    pub const ENABLE_MOUSE: &str = "\x1b[?1006h\x1b[?1003h";
    /// Disable mouse tracking
    pub const DISABLE_MOUSE: &str = "\x1b[?1003l\x1b[?1006l";

    /// Enable bracketed paste mode
    pub const ENABLE_BRACKETED_PASTE: &str = "\x1b[?2004h";
    /// Disable bracketed paste mode
    pub const DISABLE_BRACKETED_PASTE: &str = "\x1b[?2004l";

    /// Move cursor to (row, col), both 1-based.
    pub fn cursor_goto(buf: &mut Vec<u8>, row: u16, col: u16) {
        use std::io::Write;
        let _ = write!(buf, "\x1b[{};{}H", row, col);
    }

    /// Set foreground color using 24-bit true color (SGR 38;2;r;g;b).
    /// r, g, b are 0-255.
    pub fn fg_truecolor(buf: &mut Vec<u8>, r: u8, g: u8, b: u8) {
        use std::io::Write;
        let _ = write!(buf, "\x1b[38;2;{};{};{}m", r, g, b);
    }

    /// Set background color using 24-bit true color (SGR 48;2;r;g;b).
    /// r, g, b are 0-255.
    pub fn bg_truecolor(buf: &mut Vec<u8>, r: u8, g: u8, b: u8) {
        use std::io::Write;
        let _ = write!(buf, "\x1b[48;2;{};{};{}m", r, g, b);
    }

    /// Set foreground color using 256-color palette (SGR 38;5;n).
    pub fn fg_256(buf: &mut Vec<u8>, index: u8) {
        use std::io::Write;
        let _ = write!(buf, "\x1b[38;5;{}m", index);
    }

    /// Set background color using 256-color palette (SGR 48;5;n).
    pub fn bg_256(buf: &mut Vec<u8>, index: u8) {
        use std::io::Write;
        let _ = write!(buf, "\x1b[48;5;{}m", index);
    }

    /// Set bold attribute
    pub const SGR_BOLD: &str = "\x1b[1m";
    /// Set italic attribute
    pub const SGR_ITALIC: &str = "\x1b[3m";
    /// Set underline attribute
    pub const SGR_UNDERLINE: &str = "\x1b[4m";
    /// Set double underline
    pub const SGR_DOUBLE_UNDERLINE: &str = "\x1b[21m";
    /// Set curly/wave underline (not universally supported)
    pub const SGR_CURLY_UNDERLINE: &str = "\x1b[4:3m";
    /// Set dotted underline
    pub const SGR_DOTTED_UNDERLINE: &str = "\x1b[4:4m";
    /// Set dashed underline
    pub const SGR_DASHED_UNDERLINE: &str = "\x1b[4:5m";
    /// Set strikethrough attribute
    pub const SGR_STRIKETHROUGH: &str = "\x1b[9m";
    /// Set inverse/reverse video
    pub const SGR_INVERSE: &str = "\x1b[7m";

    /// Set underline color (SGR 58;2;r;g;b) -- requires terminal support.
    pub fn underline_color(buf: &mut Vec<u8>, r: u8, g: u8, b: u8) {
        use std::io::Write;
        let _ = write!(buf, "\x1b[58;2;{};{};{}m", r, g, b);
    }

    /// Build a complete SGR sequence from cell attributes, writing into `buf`.
    /// Returns the number of bytes appended.
    pub fn write_sgr(buf: &mut Vec<u8>, attrs: &CellAttrs) {
        // Always start with reset to avoid attribute leaking
        buf.extend_from_slice(SGR_RESET.as_bytes());

        if attrs.bold {
            buf.extend_from_slice(SGR_BOLD.as_bytes());
        }
        if attrs.italic {
            buf.extend_from_slice(SGR_ITALIC.as_bytes());
        }
        match attrs.underline {
            1 => buf.extend_from_slice(SGR_UNDERLINE.as_bytes()),
            2 => buf.extend_from_slice(SGR_CURLY_UNDERLINE.as_bytes()),
            3 => buf.extend_from_slice(SGR_DOUBLE_UNDERLINE.as_bytes()),
            4 => buf.extend_from_slice(SGR_DOTTED_UNDERLINE.as_bytes()),
            5 => buf.extend_from_slice(SGR_DASHED_UNDERLINE.as_bytes()),
            _ => {}
        }
        if attrs.strikethrough {
            buf.extend_from_slice(SGR_STRIKETHROUGH.as_bytes());
        }
        if attrs.inverse {
            buf.extend_from_slice(SGR_INVERSE.as_bytes());
        }

        // Foreground color (24-bit)
        fg_truecolor(buf, attrs.fg.0, attrs.fg.1, attrs.fg.2);

        // Background color (24-bit)
        bg_truecolor(buf, attrs.bg.0, attrs.bg.1, attrs.bg.2);

        // Underline color if different from fg
        if let Some((r, g, b)) = attrs.underline_color {
            underline_color(buf, r, g, b);
        }
    }

    /// Cell attributes that map to SGR sequences.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CellAttrs {
        /// Foreground color as (R, G, B) in 0-255
        pub fg: (u8, u8, u8),
        /// Background color as (R, G, B) in 0-255
        pub bg: (u8, u8, u8),
        /// Bold
        pub bold: bool,
        /// Italic
        pub italic: bool,
        /// Underline style (0=none, 1=single, 2=wave, 3=double, 4=dotted, 5=dashed)
        pub underline: u8,
        /// Underline color override (None = use fg)
        pub underline_color: Option<(u8, u8, u8)>,
        /// Strikethrough
        pub strikethrough: bool,
        /// Inverse video
        pub inverse: bool,
    }

    impl Default for CellAttrs {
        fn default() -> Self {
            Self {
                fg: (255, 255, 255),
                bg: (0, 0, 0),
                bold: false,
                italic: false,
                underline: 0,
                underline_color: None,
                strikethrough: false,
                inverse: false,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TTY Cell and Grid for frame diffing
// ---------------------------------------------------------------------------

/// A single cell in the TTY grid used for frame diffing.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TtyCell {
    /// Character content (may be multi-byte for wide/composed chars)
    text: String,
    /// Display width of this cell (1 for normal, 2 for wide chars)
    width: u8,
    /// Cell attributes
    attrs: ansi::CellAttrs,
}

impl Default for TtyCell {
    fn default() -> Self {
        Self {
            text: " ".to_string(),
            width: 1,
            attrs: ansi::CellAttrs::default(),
        }
    }
}

/// TTY cell grid: two-dimensional array of cells for frame diffing.
#[derive(Debug, Clone)]
struct TtyGrid {
    width: usize,
    height: usize,
    cells: Vec<TtyCell>,
}

impl TtyGrid {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![TtyCell::default(); width * height],
        }
    }

    fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.cells.resize(width * height, TtyCell::default());
    }

    fn clear(&mut self) {
        for cell in &mut self.cells {
            *cell = TtyCell::default();
        }
    }

    fn get(&self, col: usize, row: usize) -> Option<&TtyCell> {
        if col < self.width && row < self.height {
            Some(&self.cells[row * self.width + col])
        } else {
            None
        }
    }

    fn get_mut(&mut self, col: usize, row: usize) -> Option<&mut TtyCell> {
        if col < self.width && row < self.height {
            Some(&mut self.cells[row * self.width + col])
        } else {
            None
        }
    }

    fn set(&mut self, col: usize, row: usize, cell: TtyCell) {
        if col < self.width && row < self.height {
            self.cells[row * self.width + col] = cell;
        }
    }
}

// ---------------------------------------------------------------------------
// Color conversion
// ---------------------------------------------------------------------------

/// Convert a `Color` (f32 0.0-1.0 components) to (R, G, B) in 0-255.
/// The Color may be in linear or sRGB space; for TTY output we want sRGB
/// bytes, so we clamp to [0,1] and multiply by 255.
fn color_to_rgb8(c: &Color) -> (u8, u8, u8) {
    let r = (c.r.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let g = (c.g.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let b = (c.b.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    (r, g, b)
}

// ---------------------------------------------------------------------------
// Raw mode helpers (POSIX)
// ---------------------------------------------------------------------------

/// Saved terminal state for restoring on shutdown.
#[cfg(unix)]
struct SavedTermios {
    original: libc::termios,
}

#[cfg(unix)]
impl SavedTermios {
    /// Put the terminal into raw mode and return the saved state.
    fn enable_raw_mode() -> io::Result<Self> {
        use std::mem::MaybeUninit;
        unsafe {
            let mut original = MaybeUninit::<libc::termios>::uninit();
            if libc::tcgetattr(libc::STDIN_FILENO, original.as_mut_ptr()) != 0 {
                return Err(io::Error::last_os_error());
            }
            let original = original.assume_init();

            let mut raw = original;
            // Input: no break, no CR→NL, no parity, no strip, no start/stop
            raw.c_iflag &= !(libc::BRKINT
                | libc::ICRNL
                | libc::INPCK
                | libc::ISTRIP
                | libc::IXON);
            // Output: disable post-processing
            raw.c_oflag &= !libc::OPOST;
            // Control: 8-bit chars
            raw.c_cflag |= libc::CS8;
            // Local: no echo, no canonical, no signals, no extended
            raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::ISIG | libc::IEXTEN);
            // Read returns immediately with at least 1 byte or timeout
            raw.c_cc[libc::VMIN] = 0;
            raw.c_cc[libc::VTIME] = 0;

            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &raw) != 0 {
                return Err(io::Error::last_os_error());
            }

            Ok(Self { original })
        }
    }

    /// Restore the saved terminal state.
    fn restore(&self) -> io::Result<()> {
        unsafe {
            if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSAFLUSH, &self.original) != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }
}

/// Query terminal size via ioctl(TIOCGWINSZ).
#[cfg(unix)]
fn get_terminal_size() -> Option<(u16, u16)> {
    use std::mem::MaybeUninit;
    unsafe {
        let mut ws = MaybeUninit::<libc::winsize>::uninit();
        if libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, ws.as_mut_ptr()) == 0 {
            let ws = ws.assume_init();
            if ws.ws_col > 0 && ws.ws_row > 0 {
                return Some((ws.ws_col, ws.ws_row));
            }
        }
    }
    None
}

#[cfg(not(unix))]
fn get_terminal_size() -> Option<(u16, u16)> {
    None
}

// ---------------------------------------------------------------------------
// Frame diffing
// ---------------------------------------------------------------------------

/// Compute the minimal set of terminal writes needed to transition from
/// `prev` to `next`. Returns the escape sequence bytes to write.
///
/// The algorithm walks every cell. When a cell differs, it emits:
/// 1. A cursor-goto if the cursor isn't already at the right position
/// 2. SGR attribute changes if the attributes differ from the last emitted
/// 3. The cell text
///
/// Consecutive changed cells on the same row avoid redundant cursor-goto
/// sequences (the cursor naturally advances after printing).
fn diff_grids(prev: &TtyGrid, next: &TtyGrid) -> Vec<u8> {
    assert_eq!(prev.width, next.width);
    assert_eq!(prev.height, next.height);

    let mut out = Vec::with_capacity(4096);
    let mut last_attrs: Option<ansi::CellAttrs> = None;
    let mut cursor_row: Option<usize> = None;
    let mut cursor_col: Option<usize> = None;

    for row in 0..next.height {
        let mut col = 0;
        while col < next.width {
            let next_cell = &next.cells[row * next.width + col];
            let prev_cell = &prev.cells[row * prev.width + col];

            if next_cell != prev_cell {
                // Need to position cursor if not already here
                let need_goto = cursor_row != Some(row) || cursor_col != Some(col);
                if need_goto {
                    // ANSI uses 1-based coordinates
                    ansi::cursor_goto(&mut out, (row + 1) as u16, (col + 1) as u16);
                }

                // Emit SGR if attrs changed from last emitted
                if last_attrs.as_ref() != Some(&next_cell.attrs) {
                    ansi::write_sgr(&mut out, &next_cell.attrs);
                    last_attrs = Some(next_cell.attrs);
                }

                // Emit the character text
                out.extend_from_slice(next_cell.text.as_bytes());

                // Advance cursor tracking
                let advance = next_cell.width.max(1) as usize;
                cursor_row = Some(row);
                cursor_col = Some(col + advance);

                col += advance;
            } else {
                col += next_cell.width.max(1) as usize;
            }
        }
    }

    // Reset attributes at the end to avoid leaking into the shell
    if last_attrs.is_some() {
        out.extend_from_slice(ansi::SGR_RESET.as_bytes());
    }

    out
}

/// Full-screen render: emit every cell (used for first frame or after resize).
fn render_full(grid: &TtyGrid) -> Vec<u8> {
    let mut out = Vec::with_capacity(grid.width * grid.height * 20);

    // Home cursor
    out.extend_from_slice(ansi::CURSOR_HOME.as_bytes());

    let mut last_attrs: Option<ansi::CellAttrs> = None;

    for row in 0..grid.height {
        if row > 0 {
            // Move to start of next row
            ansi::cursor_goto(&mut out, (row + 1) as u16, 1);
        }
        let mut col = 0;
        while col < grid.width {
            let cell = &grid.cells[row * grid.width + col];

            if last_attrs.as_ref() != Some(&cell.attrs) {
                ansi::write_sgr(&mut out, &cell.attrs);
                last_attrs = Some(cell.attrs);
            }

            out.extend_from_slice(cell.text.as_bytes());
            col += cell.width.max(1) as usize;
        }
    }

    if last_attrs.is_some() {
        out.extend_from_slice(ansi::SGR_RESET.as_bytes());
    }

    out
}

// ---------------------------------------------------------------------------
// FrameGlyphBuffer -> TtyGrid rasterizer
// ---------------------------------------------------------------------------

/// Rasterize a `FrameGlyphBuffer` into a `TtyGrid`.
///
/// This maps pixel-coordinate glyphs into character-cell positions using
/// the frame's `char_width` and `char_height` as the cell dimensions.
fn rasterize_frame_glyphs(
    frame: &FrameGlyphBuffer,
    grid: &mut TtyGrid,
    bg_color: (u8, u8, u8),
) {
    // Clear grid with background
    for cell in &mut grid.cells {
        cell.text = " ".to_string();
        cell.width = 1;
        cell.attrs = ansi::CellAttrs {
            fg: (255, 255, 255),
            bg: bg_color,
            ..Default::default()
        };
    }

    let cw = frame.char_width.max(1.0);
    let ch = frame.char_height.max(1.0);

    for glyph in &frame.glyphs {
        match glyph {
            FrameGlyph::Char {
                char: character,
                composed,
                x,
                y,
                fg,
                bg,
                font_weight,
                italic,
                underline,
                underline_color,
                strike_through,
                ..
            } => {
                let col = (*x / cw) as usize;
                let row = (*y / ch) as usize;

                if col >= grid.width || row >= grid.height {
                    continue;
                }

                let text = if let Some(comp) = composed {
                    comp.to_string()
                } else {
                    character.to_string()
                };

                let fg_rgb = color_to_rgb8(fg);
                let bg_rgb = bg.map(|c| color_to_rgb8(&c)).unwrap_or(bg_color);
                let ul_color = underline_color.map(|c| {
                    let (r, g, b) = color_to_rgb8(&c);
                    (r, g, b)
                });

                let cell = TtyCell {
                    text,
                    width: 1, // will be updated below for wide chars
                    attrs: ansi::CellAttrs {
                        fg: fg_rgb,
                        bg: bg_rgb,
                        bold: *font_weight >= 700,
                        italic: *italic,
                        underline: *underline,
                        underline_color: ul_color,
                        strikethrough: *strike_through > 0,
                        inverse: false,
                    },
                };

                // Determine display width (approximate: use glyph pixel
                // width relative to char_width)
                let display_width = ((*x + frame.char_width - 0.5).max(0.0) / cw) as usize;
                let glyph_cols = ((glyph_pixel_width(glyph) / cw) + 0.5) as usize;
                let w = glyph_cols.max(1).min(2) as u8;

                grid.set(col, row, TtyCell { width: w, ..cell });

                // For wide chars, mark the continuation cell
                if w == 2 && col + 1 < grid.width {
                    grid.set(
                        col + 1,
                        row,
                        TtyCell {
                            text: String::new(),
                            width: 0, // continuation cell
                            attrs: ansi::CellAttrs {
                                fg: fg_rgb,
                                bg: bg_rgb,
                                ..Default::default()
                            },
                        },
                    );
                }
                let _ = display_width; // suppress unused warning
            }

            FrameGlyph::Stretch { x, y, width, height, bg, .. } => {
                let col_start = (*x / cw) as usize;
                let row_start = (*y / ch) as usize;
                let col_end = ((*x + *width) / cw).ceil() as usize;
                let row_end = ((*y + *height) / ch).ceil() as usize;

                let bg_rgb = color_to_rgb8(bg);

                for row in row_start..row_end.min(grid.height) {
                    for col in col_start..col_end.min(grid.width) {
                        if let Some(cell) = grid.get_mut(col, row) {
                            cell.attrs.bg = bg_rgb;
                        }
                    }
                }
            }

            FrameGlyph::Cursor {
                x, y, width, height, style, color, ..
            } => {
                let col = (*x / cw) as usize;
                let row = (*y / ch) as usize;

                if col >= grid.width || row >= grid.height {
                    continue;
                }

                let cursor_rgb = color_to_rgb8(color);

                match style {
                    // Box cursor: inverse the cell
                    0 => {
                        if let Some(cell) = grid.get_mut(col, row) {
                            // Swap fg/bg for inverse video effect
                            let old_fg = cell.attrs.fg;
                            cell.attrs.fg = cell.attrs.bg;
                            cell.attrs.bg = cursor_rgb;
                        }
                    }
                    // Bar cursor: we can approximate with inverse on the cell
                    1 => {
                        if let Some(cell) = grid.get_mut(col, row) {
                            // Bar cursor: use a thin bar indicator; in a real
                            // terminal we can't draw a sub-cell bar, so use
                            // the cursor's native position (set via escape).
                            // For the grid, mark it inverse as a visual cue.
                            cell.attrs.inverse = true;
                        }
                    }
                    // Underline cursor
                    2 => {
                        if let Some(cell) = grid.get_mut(col, row) {
                            cell.attrs.underline = 1;
                        }
                    }
                    // Hollow cursor: just draw a box outline (not really
                    // possible in a cell grid; use inverse as approximation)
                    3 => {
                        if let Some(cell) = grid.get_mut(col, row) {
                            cell.attrs.inverse = true;
                        }
                    }
                    _ => {}
                }
            }

            FrameGlyph::Background { bounds, color } => {
                let col_start = (bounds.x / cw) as usize;
                let row_start = (bounds.y / ch) as usize;
                let col_end = ((bounds.x + bounds.width) / cw).ceil() as usize;
                let row_end = ((bounds.y + bounds.height) / ch).ceil() as usize;

                let bg_rgb = color_to_rgb8(color);

                for row in row_start..row_end.min(grid.height) {
                    for col in col_start..col_end.min(grid.width) {
                        if let Some(cell) = grid.get_mut(col, row) {
                            cell.attrs.bg = bg_rgb;
                        }
                    }
                }
            }

            FrameGlyph::Border { x, y, width, height, color } => {
                let col_start = (*x / cw) as usize;
                let row_start = (*y / ch) as usize;
                let col_end = ((*x + *width) / cw).ceil() as usize;
                let row_end = ((*y + *height) / ch).ceil() as usize;

                let border_rgb = color_to_rgb8(color);

                // For vertical borders (width <= 1 cell), use box-drawing char
                let is_vertical = (col_end - col_start) <= 1;
                let is_horizontal = (row_end - row_start) <= 1;

                for row in row_start..row_end.min(grid.height) {
                    for col in col_start..col_end.min(grid.width) {
                        if let Some(cell) = grid.get_mut(col, row) {
                            cell.attrs.fg = border_rgb;
                            if is_vertical {
                                cell.text = "\u{2502}".to_string(); // │
                            } else if is_horizontal {
                                cell.text = "\u{2500}".to_string(); // ─
                            } else {
                                cell.text = "\u{2588}".to_string(); // █
                            }
                        }
                    }
                }
            }

            // Non-text glyphs are not rendered in TTY mode
            FrameGlyph::Image { .. }
            | FrameGlyph::Video { .. }
            | FrameGlyph::WebKit { .. }
            | FrameGlyph::ScrollBar { .. } => {}

            #[cfg(feature = "neo-term")]
            FrameGlyph::Terminal { .. } => {}
        }
    }
}

/// Get the pixel width of a glyph.
fn glyph_pixel_width(glyph: &FrameGlyph) -> f32 {
    match glyph {
        FrameGlyph::Char { width, .. } => *width,
        FrameGlyph::Stretch { width, .. } => *width,
        FrameGlyph::Image { width, .. } => *width,
        FrameGlyph::Video { width, .. } => *width,
        FrameGlyph::WebKit { width, .. } => *width,
        FrameGlyph::Cursor { width, .. } => *width,
        FrameGlyph::Background { bounds, .. } => bounds.width,
        FrameGlyph::Border { width, .. } => *width,
        FrameGlyph::ScrollBar { width, .. } => *width,
        #[cfg(feature = "neo-term")]
        FrameGlyph::Terminal { width, .. } => *width,
    }
}

// ---------------------------------------------------------------------------
// TtyBackend
// ---------------------------------------------------------------------------

/// Output target abstraction for testing. In production this writes to
/// stdout; in tests it writes to a Vec<u8>.
trait TtyOutput: Write + Send {
    fn flush_output(&mut self) -> io::Result<()>;
}

impl TtyOutput for io::Stdout {
    fn flush_output(&mut self) -> io::Result<()> {
        self.flush()
    }
}

impl TtyOutput for Vec<u8> {
    fn flush_output(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// TTY backend state
pub struct TtyBackend {
    initialized: bool,
    width: u32,
    height: u32,

    /// Current frame grid (what should be on screen)
    current: TtyGrid,
    /// Previous frame grid (what is on screen)
    previous: TtyGrid,

    /// Whether next render should do a full repaint
    force_full_render: bool,

    /// Buffered output bytes to write on present()
    output_buf: Vec<u8>,

    /// Saved terminal state for raw mode
    #[cfg(unix)]
    saved_termios: Option<SavedTermios>,

    /// Cursor position to set after rendering (col, row) -- 0-based
    cursor_position: Option<(u16, u16)>,
    /// Whether to show the terminal cursor
    cursor_visible: bool,

    /// Last received FrameGlyphBuffer for rendering
    frame_glyphs: Option<FrameGlyphBuffer>,
}

impl Default for TtyBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TtyBackend {
    pub fn new() -> Self {
        Self {
            initialized: false,
            width: 80,
            height: 24,
            current: TtyGrid::new(80, 24),
            previous: TtyGrid::new(80, 24),
            force_full_render: true,
            output_buf: Vec::with_capacity(65536),
            #[cfg(unix)]
            saved_termios: None,
            cursor_position: None,
            cursor_visible: false,
            frame_glyphs: None,
        }
    }

    /// Set a FrameGlyphBuffer to be rendered on the next render() call.
    pub fn set_frame_glyphs(&mut self, frame: FrameGlyphBuffer) {
        self.frame_glyphs = Some(frame);
    }

    /// Get the current grid dimensions in characters.
    pub fn grid_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Force a full repaint on the next render.
    pub fn force_redraw(&mut self) {
        self.force_full_render = true;
    }

    /// Internal: build output bytes from current vs previous grid.
    fn build_output(&mut self) {
        self.output_buf.clear();

        if self.force_full_render {
            self.output_buf = render_full(&self.current);
            self.force_full_render = false;
        } else {
            self.output_buf = diff_grids(&self.previous, &self.current);
        }

        // Position cursor and show/hide
        if let Some((col, row)) = self.cursor_position {
            ansi::cursor_goto(&mut self.output_buf, row + 1, col + 1);
            if self.cursor_visible {
                self.output_buf.extend_from_slice(ansi::SHOW_CURSOR.as_bytes());
            } else {
                self.output_buf.extend_from_slice(ansi::HIDE_CURSOR.as_bytes());
            }
        } else {
            self.output_buf.extend_from_slice(ansi::HIDE_CURSOR.as_bytes());
        }
    }
}

impl DisplayBackend for TtyBackend {
    fn init(&mut self) -> DisplayResult<()> {
        // Get terminal size
        if let Some((cols, rows)) = get_terminal_size() {
            self.width = cols as u32;
            self.height = rows as u32;
        }

        // Resize grids
        self.current.resize(self.width as usize, self.height as usize);
        self.previous.resize(self.width as usize, self.height as usize);

        // Enter raw mode
        #[cfg(unix)]
        {
            match SavedTermios::enable_raw_mode() {
                Ok(saved) => self.saved_termios = Some(saved),
                Err(e) => {
                    return Err(DisplayError::Backend(format!(
                        "Failed to enable raw mode: {}",
                        e
                    )));
                }
            }
        }

        // Enter alternate screen, hide cursor, clear screen
        let mut stdout = io::stdout();
        let init_seq = format!(
            "{}{}{}",
            ansi::ENTER_ALT_SCREEN,
            ansi::HIDE_CURSOR,
            ansi::CLEAR_SCREEN,
        );
        stdout
            .write_all(init_seq.as_bytes())
            .map_err(|e| DisplayError::Backend(format!("Failed to write init sequence: {}", e)))?;
        stdout
            .flush()
            .map_err(|e| DisplayError::Backend(format!("Failed to flush stdout: {}", e)))?;

        self.force_full_render = true;
        self.initialized = true;
        Ok(())
    }

    fn shutdown(&mut self) {
        if !self.initialized {
            return;
        }

        // Show cursor, leave alternate screen, reset attributes
        let shutdown_seq = format!(
            "{}{}{}",
            ansi::SGR_RESET,
            ansi::SHOW_CURSOR,
            ansi::LEAVE_ALT_SCREEN,
        );

        let mut stdout = io::stdout();
        let _ = stdout.write_all(shutdown_seq.as_bytes());
        let _ = stdout.flush();

        // Restore terminal state
        #[cfg(unix)]
        if let Some(ref saved) = self.saved_termios {
            let _ = saved.restore();
        }

        #[cfg(unix)]
        {
            self.saved_termios = None;
        }

        self.initialized = false;
    }

    fn render(&mut self, scene: &Scene) -> DisplayResult<()> {
        if !self.initialized {
            return Err(DisplayError::Backend("TTY backend not initialized".into()));
        }

        // Save previous grid for diffing
        self.previous = self.current.clone();

        // If we have a FrameGlyphBuffer, rasterize it
        if let Some(ref frame) = self.frame_glyphs {
            let bg_rgb = color_to_rgb8(&frame.background);
            rasterize_frame_glyphs(frame, &mut self.current, bg_rgb);

            // Extract cursor position from frame glyphs
            self.cursor_position = None;
            self.cursor_visible = false;
            for glyph in &frame.glyphs {
                if let FrameGlyph::Cursor { x, y, style, .. } = glyph {
                    let cw = frame.char_width.max(1.0);
                    let ch = frame.char_height.max(1.0);
                    let col = (*x / cw) as u16;
                    let row = (*y / ch) as u16;
                    self.cursor_position = Some((col, row));
                    // Show cursor for bar and underline styles (box uses inverse)
                    self.cursor_visible = *style == 1 || *style == 2;
                    break;
                }
            }
        } else {
            // Fallback: render from Scene (limited -- Scene doesn't carry
            // per-character data in the same way)
            let bg_rgb = color_to_rgb8(&scene.background);
            self.current.clear();
            for cell in &mut self.current.cells {
                cell.attrs.bg = bg_rgb;
            }
        }

        // Build diff output
        self.build_output();

        Ok(())
    }

    fn present(&mut self) -> DisplayResult<()> {
        if !self.initialized {
            return Err(DisplayError::Backend("TTY backend not initialized".into()));
        }

        if self.output_buf.is_empty() {
            return Ok(());
        }

        let mut stdout = io::stdout();
        stdout
            .write_all(&self.output_buf)
            .map_err(|e| DisplayError::Backend(format!("Failed to write to stdout: {}", e)))?;
        stdout
            .flush()
            .map_err(|e| DisplayError::Backend(format!("Failed to flush stdout: {}", e)))?;

        self.output_buf.clear();

        Ok(())
    }

    fn name(&self) -> &'static str {
        "tty"
    }

    fn is_initialized(&self) -> bool {
        self.initialized
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.current.resize(width as usize, height as usize);
        self.previous.resize(width as usize, height as usize);
        self.force_full_render = true;
    }

    fn set_vsync(&mut self, _enabled: bool) {
        // No vsync on TTY
    }
}

impl Drop for TtyBackend {
    fn drop(&mut self) {
        self.shutdown();
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::Rect;

    // -------------------------------------------------------------------
    // ANSI escape sequence generation
    // -------------------------------------------------------------------

    #[test]
    fn test_cursor_goto_1_1() {
        let mut buf = Vec::new();
        ansi::cursor_goto(&mut buf, 1, 1);
        assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[1;1H");
    }

    #[test]
    fn test_cursor_goto_various() {
        let mut buf = Vec::new();
        ansi::cursor_goto(&mut buf, 10, 20);
        assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[10;20H");
    }

    #[test]
    fn test_fg_truecolor() {
        let mut buf = Vec::new();
        ansi::fg_truecolor(&mut buf, 255, 128, 0);
        assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[38;2;255;128;0m");
    }

    #[test]
    fn test_bg_truecolor() {
        let mut buf = Vec::new();
        ansi::bg_truecolor(&mut buf, 0, 64, 128);
        assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[48;2;0;64;128m");
    }

    #[test]
    fn test_fg_256() {
        let mut buf = Vec::new();
        ansi::fg_256(&mut buf, 196);
        assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[38;5;196m");
    }

    #[test]
    fn test_bg_256() {
        let mut buf = Vec::new();
        ansi::bg_256(&mut buf, 27);
        assert_eq!(String::from_utf8(buf).unwrap(), "\x1b[48;5;27m");
    }

    #[test]
    fn test_write_sgr_default_attrs() {
        let mut buf = Vec::new();
        let attrs = ansi::CellAttrs::default();
        ansi::write_sgr(&mut buf, &attrs);
        let s = String::from_utf8(buf).unwrap();
        // Should contain reset
        assert!(s.starts_with("\x1b[0m"));
        // Should contain fg and bg color sequences
        assert!(s.contains("\x1b[38;2;255;255;255m")); // white fg
        assert!(s.contains("\x1b[48;2;0;0;0m")); // black bg
        // Should NOT contain bold/italic/underline
        assert!(!s.contains("\x1b[1m"));
        assert!(!s.contains("\x1b[3m"));
        assert!(!s.contains("\x1b[4m"));
    }

    #[test]
    fn test_write_sgr_bold_italic() {
        let mut buf = Vec::new();
        let attrs = ansi::CellAttrs {
            bold: true,
            italic: true,
            ..Default::default()
        };
        ansi::write_sgr(&mut buf, &attrs);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\x1b[1m")); // bold
        assert!(s.contains("\x1b[3m")); // italic
    }

    #[test]
    fn test_write_sgr_underline_styles() {
        // Single underline
        {
            let mut buf = Vec::new();
            let attrs = ansi::CellAttrs {
                underline: 1,
                ..Default::default()
            };
            ansi::write_sgr(&mut buf, &attrs);
            let s = String::from_utf8(buf).unwrap();
            assert!(s.contains("\x1b[4m"));
        }

        // Wave underline
        {
            let mut buf = Vec::new();
            let attrs = ansi::CellAttrs {
                underline: 2,
                ..Default::default()
            };
            ansi::write_sgr(&mut buf, &attrs);
            let s = String::from_utf8(buf).unwrap();
            assert!(s.contains("\x1b[4:3m"));
        }

        // Double underline
        {
            let mut buf = Vec::new();
            let attrs = ansi::CellAttrs {
                underline: 3,
                ..Default::default()
            };
            ansi::write_sgr(&mut buf, &attrs);
            let s = String::from_utf8(buf).unwrap();
            assert!(s.contains("\x1b[21m"));
        }
    }

    #[test]
    fn test_write_sgr_strikethrough() {
        let mut buf = Vec::new();
        let attrs = ansi::CellAttrs {
            strikethrough: true,
            ..Default::default()
        };
        ansi::write_sgr(&mut buf, &attrs);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\x1b[9m"));
    }

    #[test]
    fn test_write_sgr_inverse() {
        let mut buf = Vec::new();
        let attrs = ansi::CellAttrs {
            inverse: true,
            ..Default::default()
        };
        ansi::write_sgr(&mut buf, &attrs);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\x1b[7m"));
    }

    #[test]
    fn test_write_sgr_custom_colors() {
        let mut buf = Vec::new();
        let attrs = ansi::CellAttrs {
            fg: (128, 64, 32),
            bg: (10, 20, 30),
            ..Default::default()
        };
        ansi::write_sgr(&mut buf, &attrs);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\x1b[38;2;128;64;32m"));
        assert!(s.contains("\x1b[48;2;10;20;30m"));
    }

    #[test]
    fn test_write_sgr_underline_color() {
        let mut buf = Vec::new();
        let attrs = ansi::CellAttrs {
            underline: 1,
            underline_color: Some((255, 0, 0)),
            ..Default::default()
        };
        ansi::write_sgr(&mut buf, &attrs);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.contains("\x1b[58;2;255;0;0m"));
    }

    #[test]
    fn test_write_sgr_all_attributes() {
        let mut buf = Vec::new();
        let attrs = ansi::CellAttrs {
            fg: (200, 100, 50),
            bg: (10, 20, 30),
            bold: true,
            italic: true,
            underline: 1,
            underline_color: Some((0, 255, 0)),
            strikethrough: true,
            inverse: true,
        };
        ansi::write_sgr(&mut buf, &attrs);
        let s = String::from_utf8(buf).unwrap();
        assert!(s.starts_with("\x1b[0m")); // reset first
        assert!(s.contains("\x1b[1m")); // bold
        assert!(s.contains("\x1b[3m")); // italic
        assert!(s.contains("\x1b[4m")); // underline
        assert!(s.contains("\x1b[9m")); // strikethrough
        assert!(s.contains("\x1b[7m")); // inverse
        assert!(s.contains("\x1b[38;2;200;100;50m")); // fg
        assert!(s.contains("\x1b[48;2;10;20;30m")); // bg
        assert!(s.contains("\x1b[58;2;0;255;0m")); // underline color
    }

    // -------------------------------------------------------------------
    // Color conversion
    // -------------------------------------------------------------------

    #[test]
    fn test_color_to_rgb8_black() {
        assert_eq!(color_to_rgb8(&Color::BLACK), (0, 0, 0));
    }

    #[test]
    fn test_color_to_rgb8_white() {
        assert_eq!(color_to_rgb8(&Color::WHITE), (255, 255, 255));
    }

    #[test]
    fn test_color_to_rgb8_red() {
        assert_eq!(color_to_rgb8(&Color::RED), (255, 0, 0));
    }

    #[test]
    fn test_color_to_rgb8_mid_gray() {
        let c = Color::rgb(0.5, 0.5, 0.5);
        let (r, g, b) = color_to_rgb8(&c);
        assert_eq!(r, 128);
        assert_eq!(g, 128);
        assert_eq!(b, 128);
    }

    #[test]
    fn test_color_to_rgb8_clamping() {
        let c = Color::new(1.5, -0.5, 2.0, 1.0);
        assert_eq!(color_to_rgb8(&c), (255, 0, 255));
    }

    // -------------------------------------------------------------------
    // TtyGrid basic operations
    // -------------------------------------------------------------------

    #[test]
    fn test_grid_new() {
        let grid = TtyGrid::new(10, 5);
        assert_eq!(grid.width, 10);
        assert_eq!(grid.height, 5);
        assert_eq!(grid.cells.len(), 50);
    }

    #[test]
    fn test_grid_get_set() {
        let mut grid = TtyGrid::new(10, 5);
        let cell = TtyCell {
            text: "A".to_string(),
            width: 1,
            attrs: ansi::CellAttrs {
                fg: (255, 0, 0),
                ..Default::default()
            },
        };
        grid.set(3, 2, cell.clone());
        assert_eq!(grid.get(3, 2).unwrap(), &cell);
    }

    #[test]
    fn test_grid_get_out_of_bounds() {
        let grid = TtyGrid::new(10, 5);
        assert!(grid.get(10, 0).is_none());
        assert!(grid.get(0, 5).is_none());
        assert!(grid.get(100, 100).is_none());
    }

    #[test]
    fn test_grid_clear() {
        let mut grid = TtyGrid::new(5, 3);
        grid.set(
            2,
            1,
            TtyCell {
                text: "X".to_string(),
                width: 1,
                attrs: ansi::CellAttrs {
                    fg: (255, 0, 0),
                    ..Default::default()
                },
            },
        );
        assert_eq!(grid.get(2, 1).unwrap().text, "X");

        grid.clear();
        assert_eq!(grid.get(2, 1).unwrap().text, " ");
        assert_eq!(grid.get(2, 1).unwrap().attrs, ansi::CellAttrs::default());
    }

    #[test]
    fn test_grid_resize() {
        let mut grid = TtyGrid::new(5, 3);
        grid.set(
            2,
            1,
            TtyCell {
                text: "A".to_string(),
                ..Default::default()
            },
        );
        grid.resize(10, 8);
        assert_eq!(grid.width, 10);
        assert_eq!(grid.height, 8);
        assert_eq!(grid.cells.len(), 80);
    }

    // -------------------------------------------------------------------
    // Frame diffing
    // -------------------------------------------------------------------

    #[test]
    fn test_diff_identical_grids_produces_no_output() {
        let grid = TtyGrid::new(10, 5);
        let output = diff_grids(&grid, &grid);
        // No cells changed, so no output (no SGR reset needed either)
        assert!(output.is_empty());
    }

    #[test]
    fn test_diff_single_cell_change() {
        let prev = TtyGrid::new(10, 5);
        let mut next = TtyGrid::new(10, 5);
        next.set(
            3,
            2,
            TtyCell {
                text: "X".to_string(),
                width: 1,
                attrs: ansi::CellAttrs {
                    fg: (255, 0, 0),
                    bg: (0, 0, 0),
                    ..Default::default()
                },
            },
        );

        let output = diff_grids(&prev, &next);
        let s = String::from_utf8(output).unwrap();

        // Should contain cursor goto for row 3 (1-based), col 4 (1-based)
        assert!(s.contains("\x1b[3;4H"));
        // Should contain the character
        assert!(s.contains("X"));
        // Should contain fg color
        assert!(s.contains("\x1b[38;2;255;0;0m"));
        // Should end with SGR reset
        assert!(s.ends_with("\x1b[0m"));
    }

    #[test]
    fn test_diff_consecutive_changes_no_redundant_goto() {
        let prev = TtyGrid::new(10, 5);
        let mut next = TtyGrid::new(10, 5);

        let attrs = ansi::CellAttrs {
            fg: (0, 255, 0),
            bg: (0, 0, 0),
            ..Default::default()
        };

        // Set three consecutive cells on row 0
        for col in 0..3 {
            next.set(
                col,
                0,
                TtyCell {
                    text: ((b'A' + col as u8) as char).to_string(),
                    width: 1,
                    attrs,
                },
            );
        }

        let output = diff_grids(&prev, &next);
        let s = String::from_utf8(output).unwrap();

        // Should contain only ONE cursor_goto (for the first cell)
        let goto_count = s.matches("\x1b[1;").count();
        assert_eq!(goto_count, 1, "Expected 1 goto, got {}: {}", goto_count, s);

        // Should contain all three characters
        assert!(s.contains('A'));
        assert!(s.contains('B'));
        assert!(s.contains('C'));
    }

    #[test]
    fn test_diff_non_consecutive_changes_emit_goto() {
        let prev = TtyGrid::new(20, 5);
        let mut next = TtyGrid::new(20, 5);

        let attrs = ansi::CellAttrs::default();

        // Set cell at col 0, row 0
        next.set(
            0,
            0,
            TtyCell {
                text: "A".to_string(),
                width: 1,
                attrs,
            },
        );
        // Set cell at col 10, row 0 (gap)
        next.set(
            10,
            0,
            TtyCell {
                text: "B".to_string(),
                width: 1,
                attrs,
            },
        );

        let output = diff_grids(&prev, &next);
        let s = String::from_utf8(output).unwrap();

        // Should contain two cursor_goto sequences (one for each non-consecutive cell)
        assert!(s.contains("\x1b[1;1H"));
        assert!(s.contains("\x1b[1;11H"));
    }

    #[test]
    fn test_diff_attrs_change_emits_new_sgr() {
        let prev = TtyGrid::new(10, 5);
        let mut next = TtyGrid::new(10, 5);

        // Two cells with different attributes
        next.set(
            0,
            0,
            TtyCell {
                text: "A".to_string(),
                width: 1,
                attrs: ansi::CellAttrs {
                    fg: (255, 0, 0),
                    bg: (0, 0, 0),
                    bold: true,
                    ..Default::default()
                },
            },
        );
        next.set(
            1,
            0,
            TtyCell {
                text: "B".to_string(),
                width: 1,
                attrs: ansi::CellAttrs {
                    fg: (0, 255, 0),
                    bg: (0, 0, 0),
                    italic: true,
                    ..Default::default()
                },
            },
        );

        let output = diff_grids(&prev, &next);
        let s = String::from_utf8(output).unwrap();

        // Should contain both SGR sequences
        assert!(s.contains("\x1b[1m")); // bold
        assert!(s.contains("\x1b[3m")); // italic
        assert!(s.contains("\x1b[38;2;255;0;0m")); // red fg
        assert!(s.contains("\x1b[38;2;0;255;0m")); // green fg
    }

    #[test]
    fn test_diff_same_attrs_no_redundant_sgr() {
        let prev = TtyGrid::new(10, 5);
        let mut next = TtyGrid::new(10, 5);

        let attrs = ansi::CellAttrs {
            fg: (255, 255, 0),
            bg: (0, 0, 0),
            bold: true,
            ..Default::default()
        };

        // Two consecutive cells with SAME attributes
        next.set(
            0,
            0,
            TtyCell {
                text: "A".to_string(),
                width: 1,
                attrs,
            },
        );
        next.set(
            1,
            0,
            TtyCell {
                text: "B".to_string(),
                width: 1,
                attrs,
            },
        );

        let output = diff_grids(&prev, &next);
        let s = String::from_utf8(output).unwrap();

        // Should contain exactly ONE SGR reset (at start of attribute set) + ONE at end
        // The two cells share attributes so only one write_sgr call should happen
        // Count occurrences of the bold sequence
        let bold_count = s.matches("\x1b[1m").count();
        assert_eq!(bold_count, 1, "Expected 1 bold SGR, got {}", bold_count);
    }

    // -------------------------------------------------------------------
    // Full render
    // -------------------------------------------------------------------

    #[test]
    fn test_render_full_starts_with_home() {
        let grid = TtyGrid::new(3, 2);
        let output = render_full(&grid);
        let s = String::from_utf8(output).unwrap();
        assert!(s.starts_with(ansi::CURSOR_HOME));
    }

    #[test]
    fn test_render_full_contains_all_cells() {
        let mut grid = TtyGrid::new(3, 2);
        // Set specific cells
        grid.set(
            0,
            0,
            TtyCell {
                text: "A".to_string(),
                ..Default::default()
            },
        );
        grid.set(
            1,
            0,
            TtyCell {
                text: "B".to_string(),
                ..Default::default()
            },
        );
        grid.set(
            2,
            0,
            TtyCell {
                text: "C".to_string(),
                ..Default::default()
            },
        );
        grid.set(
            0,
            1,
            TtyCell {
                text: "D".to_string(),
                ..Default::default()
            },
        );

        let output = render_full(&grid);
        let s = String::from_utf8(output).unwrap();
        assert!(s.contains("A"));
        assert!(s.contains("B"));
        assert!(s.contains("C"));
        assert!(s.contains("D"));
    }

    #[test]
    fn test_render_full_ends_with_sgr_reset() {
        let grid = TtyGrid::new(3, 2);
        let output = render_full(&grid);
        let s = String::from_utf8(output).unwrap();
        assert!(s.ends_with(ansi::SGR_RESET));
    }

    // -------------------------------------------------------------------
    // Rasterizer: FrameGlyphBuffer -> TtyGrid
    // -------------------------------------------------------------------

    #[test]
    fn test_rasterize_empty_frame() {
        let frame = FrameGlyphBuffer::new();
        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        // All cells should be spaces with black bg
        for cell in &grid.cells {
            assert_eq!(cell.text, " ");
            assert_eq!(cell.attrs.bg, (0, 0, 0));
        }
    }

    #[test]
    fn test_rasterize_char_glyph() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let fg = Color::rgb(1.0, 0.0, 0.0);
        frame.set_face(0, fg, None, 700, false, 0, None, 0, None, 0, None);
        // Place 'H' at pixel (0, 0) -> col 0, row 0
        frame.add_char('H', 0.0, 0.0, 8.0, 16.0, 12.0, false);
        // Place 'i' at pixel (8, 0) -> col 1, row 0
        frame.add_char('i', 8.0, 0.0, 8.0, 16.0, 12.0, false);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        assert_eq!(grid.get(0, 0).unwrap().text, "H");
        assert_eq!(grid.get(0, 0).unwrap().attrs.fg, (255, 0, 0));
        assert!(grid.get(0, 0).unwrap().attrs.bold);

        assert_eq!(grid.get(1, 0).unwrap().text, "i");
    }

    #[test]
    fn test_rasterize_stretch_glyph() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let bg = Color::rgb(0.0, 0.0, 1.0);
        // Stretch from pixel (0, 16) to (80, 32) -> row 1, cols 0-9
        frame.add_stretch(0.0, 16.0, 80.0, 16.0, bg, 0, false);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        // Row 1 should have blue background
        for col in 0..10 {
            assert_eq!(grid.get(col, 1).unwrap().attrs.bg, (0, 0, 255));
        }
        // Row 0 should still be black
        assert_eq!(grid.get(0, 0).unwrap().attrs.bg, (0, 0, 0));
    }

    #[test]
    fn test_rasterize_border_vertical() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let border_color = Color::rgb(0.5, 0.5, 0.5);
        // Vertical border at pixel x=40 (col 5), spanning full height
        frame.add_border(40.0, 0.0, 1.0, 80.0, border_color);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        // Column 5 should have the vertical border char
        for row in 0..5 {
            let cell = grid.get(5, row).unwrap();
            assert_eq!(cell.text, "\u{2502}"); // │
        }
    }

    #[test]
    fn test_rasterize_cursor_box() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        // First put a character
        let fg = Color::rgb(1.0, 1.0, 1.0);
        frame.set_face(0, fg, Some(Color::BLACK), 400, false, 0, None, 0, None, 0, None);
        frame.add_char('A', 16.0, 0.0, 8.0, 16.0, 12.0, false);

        // Then add a box cursor at same position
        frame.add_cursor(1, 16.0, 0.0, 8.0, 16.0, 0, Color::rgb(1.0, 1.0, 1.0));

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        // Cell at col 2, row 0 should have inverse bg (cursor color)
        let cell = grid.get(2, 0).unwrap();
        assert_eq!(cell.attrs.bg, (255, 255, 255)); // cursor color as bg
    }

    #[test]
    fn test_rasterize_cursor_underline() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        frame.add_cursor(1, 0.0, 0.0, 8.0, 16.0, 2, Color::WHITE);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        let cell = grid.get(0, 0).unwrap();
        assert_eq!(cell.attrs.underline, 1);
    }

    #[test]
    fn test_rasterize_background_glyph() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let bg = Color::rgb(0.2, 0.2, 0.2);
        frame.add_background(0.0, 0.0, 80.0, 48.0, bg);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        let expected_bg = color_to_rgb8(&bg);
        // First 3 rows (48/16=3), all 10 cols should have bg
        for row in 0..3 {
            for col in 0..10 {
                assert_eq!(
                    grid.get(col, row).unwrap().attrs.bg,
                    expected_bg,
                    "Wrong bg at col={}, row={}",
                    col,
                    row
                );
            }
        }
    }

    #[test]
    fn test_rasterize_glyph_out_of_bounds() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let fg = Color::WHITE;
        frame.set_face(0, fg, None, 400, false, 0, None, 0, None, 0, None);
        // Place char way outside the grid
        frame.add_char('Z', 1000.0, 1000.0, 8.0, 16.0, 12.0, false);

        let mut grid = TtyGrid::new(10, 5);
        // Should not panic
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));
    }

    #[test]
    fn test_rasterize_composed_char() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let fg = Color::WHITE;
        frame.set_face(0, fg, None, 400, false, 0, None, 0, None, 0, None);
        frame.add_composed_char("e\u{0301}", 'e', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        assert_eq!(grid.get(0, 0).unwrap().text, "e\u{0301}");
    }

    #[test]
    fn test_rasterize_bold_face() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let fg = Color::WHITE;
        frame.set_face(0, fg, None, 700, false, 0, None, 0, None, 0, None);
        frame.add_char('B', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        assert!(grid.get(0, 0).unwrap().attrs.bold);
    }

    #[test]
    fn test_rasterize_italic_face() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let fg = Color::WHITE;
        frame.set_face(0, fg, None, 400, true, 0, None, 0, None, 0, None);
        frame.add_char('I', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        assert!(grid.get(0, 0).unwrap().attrs.italic);
    }

    #[test]
    fn test_rasterize_underline_with_color() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let fg = Color::WHITE;
        let ul_color = Color::rgb(1.0, 0.0, 0.0);
        frame.set_face(0, fg, None, 400, false, 1, Some(ul_color), 0, None, 0, None);
        frame.add_char('U', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        let cell = grid.get(0, 0).unwrap();
        assert_eq!(cell.attrs.underline, 1);
        assert_eq!(cell.attrs.underline_color, Some((255, 0, 0)));
    }

    #[test]
    fn test_rasterize_strikethrough() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let fg = Color::WHITE;
        frame.set_face(0, fg, None, 400, false, 0, None, 1, None, 0, None);
        frame.add_char('S', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        assert!(grid.get(0, 0).unwrap().attrs.strikethrough);
    }

    // -------------------------------------------------------------------
    // TtyBackend methods
    // -------------------------------------------------------------------

    #[test]
    fn test_backend_name() {
        let backend = TtyBackend::new();
        assert_eq!(backend.name(), "tty");
    }

    #[test]
    fn test_backend_not_initialized_by_default() {
        let backend = TtyBackend::new();
        assert!(!backend.is_initialized());
    }

    #[test]
    fn test_backend_default_size() {
        let backend = TtyBackend::new();
        assert_eq!(backend.grid_size(), (80, 24));
    }

    #[test]
    fn test_backend_resize() {
        let mut backend = TtyBackend::new();
        backend.resize(120, 40);
        assert_eq!(backend.grid_size(), (120, 40));
        assert!(backend.force_full_render);
    }

    #[test]
    fn test_backend_render_not_initialized_returns_error() {
        let mut backend = TtyBackend::new();
        let scene = Scene::new(800.0, 600.0);
        let result = backend.render(&scene);
        assert!(result.is_err());
    }

    #[test]
    fn test_backend_present_not_initialized_returns_error() {
        let mut backend = TtyBackend::new();
        let result = backend.present();
        assert!(result.is_err());
    }

    #[test]
    fn test_backend_force_redraw() {
        let mut backend = TtyBackend::new();
        backend.force_full_render = false;
        backend.force_redraw();
        assert!(backend.force_full_render);
    }

    #[test]
    fn test_backend_set_vsync_is_noop() {
        let mut backend = TtyBackend::new();
        // Should not panic
        backend.set_vsync(true);
        backend.set_vsync(false);
    }

    // -------------------------------------------------------------------
    // ANSI constant checks
    // -------------------------------------------------------------------

    #[test]
    fn test_ansi_constants() {
        assert_eq!(ansi::ENTER_ALT_SCREEN, "\x1b[?1049h");
        assert_eq!(ansi::LEAVE_ALT_SCREEN, "\x1b[?1049l");
        assert_eq!(ansi::HIDE_CURSOR, "\x1b[?25l");
        assert_eq!(ansi::SHOW_CURSOR, "\x1b[?25h");
        assert_eq!(ansi::SGR_RESET, "\x1b[0m");
        assert_eq!(ansi::CLEAR_SCREEN, "\x1b[2J");
        assert_eq!(ansi::CURSOR_HOME, "\x1b[H");
    }

    #[test]
    fn test_ansi_sgr_attribute_constants() {
        assert_eq!(ansi::SGR_BOLD, "\x1b[1m");
        assert_eq!(ansi::SGR_ITALIC, "\x1b[3m");
        assert_eq!(ansi::SGR_UNDERLINE, "\x1b[4m");
        assert_eq!(ansi::SGR_DOUBLE_UNDERLINE, "\x1b[21m");
        assert_eq!(ansi::SGR_CURLY_UNDERLINE, "\x1b[4:3m");
        assert_eq!(ansi::SGR_DOTTED_UNDERLINE, "\x1b[4:4m");
        assert_eq!(ansi::SGR_DASHED_UNDERLINE, "\x1b[4:5m");
        assert_eq!(ansi::SGR_STRIKETHROUGH, "\x1b[9m");
        assert_eq!(ansi::SGR_INVERSE, "\x1b[7m");
    }

    // -------------------------------------------------------------------
    // CellAttrs equality
    // -------------------------------------------------------------------

    #[test]
    fn test_cell_attrs_equality() {
        let a = ansi::CellAttrs::default();
        let b = ansi::CellAttrs::default();
        assert_eq!(a, b);

        let c = ansi::CellAttrs {
            bold: true,
            ..Default::default()
        };
        assert_ne!(a, c);
    }

    #[test]
    fn test_cell_attrs_default_values() {
        let attrs = ansi::CellAttrs::default();
        assert_eq!(attrs.fg, (255, 255, 255));
        assert_eq!(attrs.bg, (0, 0, 0));
        assert!(!attrs.bold);
        assert!(!attrs.italic);
        assert_eq!(attrs.underline, 0);
        assert!(attrs.underline_color.is_none());
        assert!(!attrs.strikethrough);
        assert!(!attrs.inverse);
    }

    // -------------------------------------------------------------------
    // TtyCell equality
    // -------------------------------------------------------------------

    #[test]
    fn test_tty_cell_equality() {
        let a = TtyCell::default();
        let b = TtyCell::default();
        assert_eq!(a, b);

        let c = TtyCell {
            text: "X".to_string(),
            ..Default::default()
        };
        assert_ne!(a, c);
    }

    #[test]
    fn test_tty_cell_default() {
        let cell = TtyCell::default();
        assert_eq!(cell.text, " ");
        assert_eq!(cell.width, 1);
        assert_eq!(cell.attrs, ansi::CellAttrs::default());
    }

    // -------------------------------------------------------------------
    // Integration: build_output with frame diff
    // -------------------------------------------------------------------

    #[test]
    fn test_build_output_force_full() {
        let mut backend = TtyBackend::new();
        backend.initialized = true;
        backend.width = 5;
        backend.height = 3;
        backend.current = TtyGrid::new(5, 3);
        backend.previous = TtyGrid::new(5, 3);
        backend.force_full_render = true;

        backend.build_output();

        let s = String::from_utf8(backend.output_buf.clone()).unwrap();
        // Full render starts with cursor home
        assert!(s.contains(ansi::CURSOR_HOME));
        assert!(!backend.force_full_render); // should be cleared
    }

    #[test]
    fn test_build_output_diff_mode() {
        let mut backend = TtyBackend::new();
        backend.initialized = true;
        backend.width = 5;
        backend.height = 3;
        backend.current = TtyGrid::new(5, 3);
        backend.previous = TtyGrid::new(5, 3);
        backend.force_full_render = false;

        // Change one cell
        backend.current.set(
            2,
            1,
            TtyCell {
                text: "Q".to_string(),
                width: 1,
                attrs: ansi::CellAttrs {
                    fg: (0, 128, 255),
                    ..Default::default()
                },
            },
        );

        backend.build_output();

        let s = String::from_utf8(backend.output_buf.clone()).unwrap();
        assert!(s.contains("Q"));
        assert!(s.contains("\x1b[2;3H")); // row 2, col 3 (1-based)
    }

    #[test]
    fn test_build_output_cursor_position() {
        let mut backend = TtyBackend::new();
        backend.initialized = true;
        backend.width = 10;
        backend.height = 5;
        backend.current = TtyGrid::new(10, 5);
        backend.previous = TtyGrid::new(10, 5);
        backend.force_full_render = false;
        backend.cursor_position = Some((5, 3));
        backend.cursor_visible = true;

        // Make a change so build_output produces something
        backend.current.set(
            0,
            0,
            TtyCell {
                text: "X".to_string(),
                ..Default::default()
            },
        );

        backend.build_output();

        let s = String::from_utf8(backend.output_buf.clone()).unwrap();
        // Cursor should be positioned at row 4, col 6 (1-based)
        assert!(s.contains("\x1b[4;6H"));
        assert!(s.contains(ansi::SHOW_CURSOR));
    }

    #[test]
    fn test_build_output_cursor_hidden() {
        let mut backend = TtyBackend::new();
        backend.initialized = true;
        backend.width = 10;
        backend.height = 5;
        backend.current = TtyGrid::new(10, 5);
        backend.previous = TtyGrid::new(10, 5);
        backend.force_full_render = false;
        backend.cursor_position = None;

        // Force at least a diff computation
        backend.current.set(
            0,
            0,
            TtyCell {
                text: "X".to_string(),
                ..Default::default()
            },
        );

        backend.build_output();

        let s = String::from_utf8(backend.output_buf.clone()).unwrap();
        assert!(s.contains(ansi::HIDE_CURSOR));
    }

    // -------------------------------------------------------------------
    // Multi-row rasterization
    // -------------------------------------------------------------------

    #[test]
    fn test_rasterize_multiple_rows() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 8.0;
        frame.char_height = 16.0;

        let fg = Color::WHITE;
        frame.set_face(0, fg, None, 400, false, 0, None, 0, None, 0, None);

        // Row 0: "ABC"
        frame.add_char('A', 0.0, 0.0, 8.0, 16.0, 12.0, false);
        frame.add_char('B', 8.0, 0.0, 8.0, 16.0, 12.0, false);
        frame.add_char('C', 16.0, 0.0, 8.0, 16.0, 12.0, false);

        // Row 1: "XY"
        frame.add_char('X', 0.0, 16.0, 8.0, 16.0, 12.0, false);
        frame.add_char('Y', 8.0, 16.0, 8.0, 16.0, 12.0, false);

        let mut grid = TtyGrid::new(10, 5);
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));

        assert_eq!(grid.get(0, 0).unwrap().text, "A");
        assert_eq!(grid.get(1, 0).unwrap().text, "B");
        assert_eq!(grid.get(2, 0).unwrap().text, "C");
        assert_eq!(grid.get(0, 1).unwrap().text, "X");
        assert_eq!(grid.get(1, 1).unwrap().text, "Y");
        // Rest should be spaces
        assert_eq!(grid.get(3, 0).unwrap().text, " ");
        assert_eq!(grid.get(2, 1).unwrap().text, " ");
    }

    // -------------------------------------------------------------------
    // Edge cases
    // -------------------------------------------------------------------

    #[test]
    fn test_diff_empty_grids() {
        let a = TtyGrid::new(0, 0);
        let b = TtyGrid::new(0, 0);
        let output = diff_grids(&a, &b);
        assert!(output.is_empty());
    }

    #[test]
    fn test_render_full_empty_grid() {
        let grid = TtyGrid::new(0, 0);
        let output = render_full(&grid);
        let s = String::from_utf8(output).unwrap();
        // Just cursor home, no content
        assert!(s.starts_with(ansi::CURSOR_HOME));
    }

    #[test]
    fn test_rasterize_zero_char_dimensions() {
        let mut frame = FrameGlyphBuffer::new();
        frame.char_width = 0.0; // Would cause division by zero without max(1.0)
        frame.char_height = 0.0;

        let fg = Color::WHITE;
        frame.set_face(0, fg, None, 400, false, 0, None, 0, None, 0, None);
        frame.add_char('A', 0.0, 0.0, 8.0, 16.0, 12.0, false);

        let mut grid = TtyGrid::new(10, 5);
        // Should not panic (char_width/height clamped to 1.0)
        rasterize_frame_glyphs(&frame, &mut grid, (0, 0, 0));
    }

    #[test]
    fn test_diff_wide_char_skips_continuation() {
        let prev = TtyGrid::new(10, 3);
        let mut next = TtyGrid::new(10, 3);

        // Place a wide character (width=2) at col 0
        next.set(
            0,
            0,
            TtyCell {
                text: "\u{4E2D}".to_string(), // 中
                width: 2,
                attrs: ansi::CellAttrs::default(),
            },
        );
        // Continuation cell at col 1
        next.set(
            1,
            0,
            TtyCell {
                text: String::new(),
                width: 0,
                attrs: ansi::CellAttrs::default(),
            },
        );
        // Normal char at col 2
        next.set(
            2,
            0,
            TtyCell {
                text: "A".to_string(),
                width: 1,
                attrs: ansi::CellAttrs::default(),
            },
        );

        let output = diff_grids(&prev, &next);
        let s = String::from_utf8(output).unwrap();

        // Should contain the wide char and 'A'
        assert!(s.contains("\u{4E2D}"));
        assert!(s.contains("A"));
    }
}

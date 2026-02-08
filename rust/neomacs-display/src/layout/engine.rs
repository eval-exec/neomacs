//! The Rust layout engine — Phase 1: Monospace ASCII.
//!
//! Reads buffer text via FFI, computes line breaks, positions glyphs on a
//! fixed-width grid, and produces FrameGlyphBuffer compatible with the
//! existing wgpu renderer.
//!
//! This is the simplest possible layout: fixed-width font, single face (default),
//! no overlays, no display properties. It covers basic text editing in
//! fundamental-mode.

use crate::core::frame_glyphs::{FrameGlyphBuffer, FrameGlyph};
use crate::core::types::{Color, Rect};
use super::types::*;
use super::emacs_ffi::*;

/// The main Rust layout engine.
///
/// Called on the Emacs thread during redisplay. Reads buffer data via FFI,
/// computes layout, and produces a FrameGlyphBuffer.
pub struct LayoutEngine {
    /// Reusable text buffer to avoid allocation per frame
    text_buf: Vec<u8>,
}

impl LayoutEngine {
    /// Create a new layout engine.
    pub fn new() -> Self {
        Self {
            text_buf: Vec::with_capacity(64 * 1024), // 64KB initial
        }
    }

    /// Perform layout for an entire frame.
    ///
    /// This is the main entry point, called from FFI when
    /// `neomacs-use-rust-display` is enabled.
    ///
    /// # Safety
    /// Must be called on the Emacs thread. The frame pointer must be valid.
    pub unsafe fn layout_frame(
        &mut self,
        frame: EmacsFrame,
        frame_params: &FrameParams,
        frame_glyphs: &mut FrameGlyphBuffer,
    ) {
        // Set up frame dimensions
        frame_glyphs.width = frame_params.width;
        frame_glyphs.height = frame_params.height;
        frame_glyphs.char_width = frame_params.char_width;
        frame_glyphs.char_height = frame_params.char_height;
        frame_glyphs.font_pixel_size = frame_params.font_pixel_size;
        frame_glyphs.background = Color::from_pixel(frame_params.background);

        // Get number of windows
        let window_count = neomacs_layout_frame_window_count(frame);

        for i in 0..window_count {
            let mut wp = WindowParamsFFI::default();
            if neomacs_layout_get_window_params(frame, i, &mut wp) != 0 {
                continue;
            }

            // Convert FFI params to our types
            let params = WindowParams {
                window_id: wp.window_id,
                buffer_id: wp.buffer_id,
                bounds: Rect::new(wp.x, wp.y, wp.width, wp.height),
                text_bounds: Rect::new(wp.text_x, wp.text_y, wp.text_width, wp.text_height),
                selected: wp.selected != 0,
                window_start: wp.window_start,
                point: wp.point,
                buffer_size: wp.buffer_zv,
                buffer_begv: wp.buffer_begv,
                hscroll: wp.hscroll,
                truncate_lines: wp.truncate_lines != 0,
                tab_width: wp.tab_width,
                default_fg: wp.default_fg,
                default_bg: wp.default_bg,
                char_width: wp.char_width,
                char_height: wp.char_height,
                font_pixel_size: wp.font_pixel_size,
                font_ascent: wp.font_ascent,
                mode_line_height: wp.mode_line_height,
                header_line_height: wp.header_line_height,
                tab_line_height: wp.tab_line_height,
                cursor_type: wp.cursor_type,
                cursor_bar_width: wp.cursor_bar_width,
            };

            // Add window background
            frame_glyphs.add_background(
                params.bounds.x,
                params.bounds.y,
                params.bounds.width,
                params.bounds.height,
                Color::from_pixel(params.default_bg),
            );

            // Add window info for animation detection
            frame_glyphs.add_window_info(
                params.window_id,
                params.buffer_id,
                params.window_start,
                params.bounds.x,
                params.bounds.y,
                params.bounds.width,
                params.bounds.height,
                params.mode_line_height,
                params.selected,
            );

            // Layout this window's content
            self.layout_window(&params, &wp, frame_glyphs);
        }
    }

    /// Layout a single window's buffer content.
    ///
    /// Phase 1: Monospace ASCII layout.
    /// - Fixed-width characters on a grid
    /// - Tab expansion
    /// - Line wrapping or truncation
    /// - Cursor positioning
    unsafe fn layout_window(
        &mut self,
        params: &WindowParams,
        wp: &WindowParamsFFI,
        frame_glyphs: &mut FrameGlyphBuffer,
    ) {
        let buffer = wp.buffer_ptr;
        if buffer.is_null() {
            return;
        }

        // Calculate available text area
        let text_x = params.text_bounds.x;
        let text_y = params.text_bounds.y + params.header_line_height + params.tab_line_height;
        let text_width = params.text_bounds.width;
        let text_height = params.text_bounds.height
            - params.header_line_height
            - params.tab_line_height
            - params.mode_line_height;

        let char_w = params.char_width;
        let char_h = params.char_height;
        let ascent = params.font_ascent;

        // How many columns and rows fit
        let cols = (text_width / char_w).floor() as i32;
        let max_rows = (text_height / char_h).floor() as i32;

        if cols <= 0 || max_rows <= 0 {
            return;
        }

        // Read buffer text from window_start
        let read_chars = (params.buffer_size - params.window_start + 1).min(cols as i64 * max_rows as i64 * 2);
        if read_chars <= 0 {
            return;
        }

        // Ensure text buffer is large enough (4 bytes per char max for UTF-8)
        let buf_size = (read_chars * 4) as usize;
        self.text_buf.resize(buf_size, 0);

        let bytes_read = neomacs_layout_buffer_text(
            buffer,
            params.window_start,
            (params.window_start + read_chars).min(params.buffer_size),
            self.text_buf.as_mut_ptr(),
            buf_size as i64,
        );

        if bytes_read <= 0 {
            return;
        }

        let text = &self.text_buf[..bytes_read as usize];

        // Set face for all glyphs (Phase 1: use default face)
        let fg = Color::from_pixel(params.default_fg);
        let bg_color = Color::from_pixel(params.default_bg);
        frame_glyphs.set_face(
            0, // DEFAULT_FACE_ID
            fg,
            Some(bg_color),
            false, // bold
            false, // italic
            0,     // underline
            None,  // underline_color
            0,     // strike_through
            None,  // strike_through_color
            0,     // overline
            None,  // overline_color
        );

        // Walk through text, placing characters on the grid
        let mut col = 0i32;
        let mut row = 0i32;
        let mut charpos = params.window_start;
        let mut cursor_placed = false;
        let mut window_end_charpos = params.window_start;
        let mut byte_idx = 0usize;

        while byte_idx < bytes_read as usize && row < max_rows {
            // Check if cursor is at this position
            if !cursor_placed && charpos >= params.point {
                let cursor_x = text_x + col as f32 * char_w;
                let cursor_y = text_y + row as f32 * char_h;

                let (cursor_w, cursor_h) = match params.cursor_type {
                    1 => (params.cursor_bar_width.max(1) as f32, char_h), // bar
                    2 => (char_w, 2.0),                                    // hbar
                    _ => (char_w, char_h),                                 // box/hollow
                };

                let cursor_style = if params.selected {
                    params.cursor_type
                } else {
                    3 // hollow for inactive windows
                };

                frame_glyphs.add_cursor(
                    params.window_id as i32,
                    cursor_x,
                    cursor_y,
                    cursor_w,
                    cursor_h,
                    cursor_style,
                    fg,
                );

                // Set inverse for filled box cursor
                if cursor_style == 0 {
                    frame_glyphs.set_cursor_inverse(
                        cursor_x,
                        cursor_y,
                        cursor_w,
                        cursor_h,
                        fg,         // cursor_bg = text fg
                        bg_color,   // cursor_fg = text bg (inverse)
                    );
                }

                cursor_placed = true;
            }

            // Decode one UTF-8 character
            let (ch, ch_len) = decode_utf8(&text[byte_idx..]);
            byte_idx += ch_len;
            charpos += 1;

            match ch {
                '\n' => {
                    // Fill rest of line with stretch
                    let remaining = (cols - col) as f32 * char_w;
                    if remaining > 0.0 {
                        let gx = text_x + col as f32 * char_w;
                        let gy = text_y + row as f32 * char_h;
                        frame_glyphs.add_stretch(gx, gy, remaining, char_h, bg_color, 0, false);
                    }
                    col = 0;
                    row += 1;
                }
                '\t' => {
                    // Tab: advance to next tab stop
                    let tab_w = params.tab_width.max(1);
                    let next_tab = ((col / tab_w) + 1) * tab_w;
                    let spaces = (next_tab - col).min(cols - col);

                    // Render tab as stretch glyph
                    let gx = text_x + col as f32 * char_w;
                    let gy = text_y + row as f32 * char_h;
                    let tab_pixel_w = spaces as f32 * char_w;
                    frame_glyphs.add_stretch(gx, gy, tab_pixel_w, char_h, bg_color, 0, false);

                    col += spaces;
                    if col >= cols {
                        if params.truncate_lines {
                            // Skip to end of line
                            while byte_idx < bytes_read as usize {
                                let (c, l) = decode_utf8(&text[byte_idx..]);
                                byte_idx += l;
                                charpos += 1;
                                if c == '\n' {
                                    col = 0;
                                    row += 1;
                                    break;
                                }
                            }
                        } else {
                            col = 0;
                            row += 1;
                        }
                    }
                }
                '\r' => {
                    // Carriage return: skip (we handle \n for line breaks)
                }
                _ if ch < ' ' => {
                    // Control character: display as ^X (2 columns)
                    let gx = text_x + col as f32 * char_w;
                    let gy = text_y + row as f32 * char_h;

                    if col + 2 <= cols {
                        frame_glyphs.add_char('^', gx, gy, char_w, char_h, ascent, false);
                        frame_glyphs.add_char(
                            char::from((ch as u8) + b'@'),
                            gx + char_w,
                            gy,
                            char_w,
                            char_h,
                            ascent,
                            false,
                        );
                        col += 2;
                    } else {
                        // Wrap or truncate
                        if params.truncate_lines {
                            // Skip to next line
                            while byte_idx < bytes_read as usize {
                                let (c, l) = decode_utf8(&text[byte_idx..]);
                                byte_idx += l;
                                charpos += 1;
                                if c == '\n' {
                                    col = 0;
                                    row += 1;
                                    break;
                                }
                            }
                        } else {
                            col = 0;
                            row += 1;
                        }
                    }
                }
                _ => {
                    // Normal character
                    // Determine display width (CJK = 2 columns)
                    let char_cols = if is_wide_char(ch) { 2 } else { 1 };

                    if col + char_cols > cols {
                        // Line full
                        if params.truncate_lines {
                            // Skip rest of logical line
                            while byte_idx < bytes_read as usize {
                                let (c, l) = decode_utf8(&text[byte_idx..]);
                                byte_idx += l;
                                charpos += 1;
                                if c == '\n' {
                                    col = 0;
                                    row += 1;
                                    break;
                                }
                            }
                            continue;
                        } else {
                            // Wrap to next visual line
                            // Fill remaining space
                            let remaining = (cols - col) as f32 * char_w;
                            if remaining > 0.0 {
                                let gx = text_x + col as f32 * char_w;
                                let gy = text_y + row as f32 * char_h;
                                frame_glyphs.add_stretch(gx, gy, remaining, char_h, bg_color, 0, false);
                            }
                            col = 0;
                            row += 1;
                            if row >= max_rows {
                                break;
                            }
                        }
                    }

                    let gx = text_x + col as f32 * char_w;
                    let gy = text_y + row as f32 * char_h;
                    let glyph_w = char_cols as f32 * char_w;

                    frame_glyphs.add_char(ch, gx, gy, glyph_w, char_h, ascent, false);
                    col += char_cols;
                }
            }

            window_end_charpos = charpos;
        }

        // If cursor wasn't placed (point is past visible content), place at end
        if !cursor_placed && params.point >= params.window_start {
            let cursor_x = text_x + col as f32 * char_w;
            let cursor_y = text_y + row.min(max_rows - 1) as f32 * char_h;

            let cursor_style = if params.selected {
                params.cursor_type
            } else {
                3
            };

            frame_glyphs.add_cursor(
                params.window_id as i32,
                cursor_x,
                cursor_y,
                char_w,
                char_h,
                cursor_style,
                fg,
            );

            if cursor_style == 0 {
                frame_glyphs.set_cursor_inverse(
                    cursor_x,
                    cursor_y,
                    char_w,
                    char_h,
                    fg,
                    bg_color,
                );
            }
        }

        // Fill remaining rows with background
        let filled_rows = row + 1;
        if filled_rows < max_rows {
            let gy = text_y + filled_rows as f32 * char_h;
            let remaining_h = text_height - filled_rows as f32 * char_h;
            if remaining_h > 0.0 {
                frame_glyphs.add_stretch(text_x, gy, text_width, remaining_h, bg_color, 0, false);
            }
        }

        // Write layout results back to Emacs
        neomacs_layout_set_window_end(
            wp.window_ptr,
            window_end_charpos,
            row.min(max_rows - 1),
        );
    }
}

/// Decode one UTF-8 character from a byte slice.
/// Returns (char, bytes_consumed).
fn decode_utf8(bytes: &[u8]) -> (char, usize) {
    if bytes.is_empty() {
        return ('\0', 0);
    }

    let b0 = bytes[0];
    if b0 < 0x80 {
        (b0 as char, 1)
    } else if b0 < 0xC0 {
        // Invalid continuation byte — treat as replacement
        ('\u{FFFD}', 1)
    } else if b0 < 0xE0 {
        if bytes.len() < 2 {
            return ('\u{FFFD}', 1);
        }
        let cp = ((b0 as u32 & 0x1F) << 6) | (bytes[1] as u32 & 0x3F);
        (char::from_u32(cp).unwrap_or('\u{FFFD}'), 2)
    } else if b0 < 0xF0 {
        if bytes.len() < 3 {
            return ('\u{FFFD}', 1);
        }
        let cp = ((b0 as u32 & 0x0F) << 12)
            | ((bytes[1] as u32 & 0x3F) << 6)
            | (bytes[2] as u32 & 0x3F);
        (char::from_u32(cp).unwrap_or('\u{FFFD}'), 3)
    } else {
        if bytes.len() < 4 {
            return ('\u{FFFD}', 1);
        }
        let cp = ((b0 as u32 & 0x07) << 18)
            | ((bytes[1] as u32 & 0x3F) << 12)
            | ((bytes[2] as u32 & 0x3F) << 6)
            | (bytes[3] as u32 & 0x3F);
        (char::from_u32(cp).unwrap_or('\u{FFFD}'), 4)
    }
}

/// Check if a character is a wide (CJK) character that occupies 2 columns.
fn is_wide_char(ch: char) -> bool {
    let cp = ch as u32;
    // CJK Unified Ideographs
    (0x4E00..=0x9FFF).contains(&cp)
    // CJK Extension A
    || (0x3400..=0x4DBF).contains(&cp)
    // CJK Extension B
    || (0x20000..=0x2A6DF).contains(&cp)
    // CJK Compatibility Ideographs
    || (0xF900..=0xFAFF).contains(&cp)
    // Fullwidth Forms
    || (0xFF01..=0xFF60).contains(&cp)
    || (0xFFE0..=0xFFE6).contains(&cp)
    // Hangul Syllables
    || (0xAC00..=0xD7AF).contains(&cp)
    // CJK Radicals
    || (0x2E80..=0x2FDF).contains(&cp)
    // Katakana/Hiragana
    || (0x3000..=0x303F).contains(&cp)
    || (0x3040..=0x309F).contains(&cp)
    || (0x30A0..=0x30FF).contains(&cp)
    || (0x31F0..=0x31FF).contains(&cp)
}

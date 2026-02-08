//! The Rust layout engine — Phase 1+2: Monospace layout with face resolution.
//!
//! Reads buffer text via FFI, resolves faces per character position,
//! computes line breaks, positions glyphs on a fixed-width grid, and
//! produces FrameGlyphBuffer compatible with the existing wgpu renderer.

use std::ffi::CStr;

use crate::core::frame_glyphs::FrameGlyphBuffer;
use crate::core::types::{Color, Rect};
use super::types::*;
use super::emacs_ffi::*;

/// Which kind of status line to render.
enum StatusLineKind {
    ModeLine,
    HeaderLine,
    TabLine,
}

/// The main Rust layout engine.
///
/// Called on the Emacs thread during redisplay. Reads buffer data via FFI,
/// resolves faces, computes layout, and produces a FrameGlyphBuffer.
pub struct LayoutEngine {
    /// Reusable text buffer to avoid allocation per frame
    text_buf: Vec<u8>,
    /// Cached face data to avoid redundant FFI calls
    face_data: FaceDataFFI,
}

impl LayoutEngine {
    /// Create a new layout engine.
    pub fn new() -> Self {
        Self {
            text_buf: Vec::with_capacity(64 * 1024), // 64KB initial
            face_data: FaceDataFFI::default(),
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
                word_wrap: wp.word_wrap != 0,
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
                left_fringe_width: wp.left_fringe_width,
                right_fringe_width: wp.right_fringe_width,
                indicate_empty_lines: wp.indicate_empty_lines,
                show_trailing_whitespace: wp.show_trailing_whitespace != 0,
                trailing_ws_bg: wp.trailing_ws_bg,
                fill_column_indicator: wp.fill_column_indicator,
                fill_column_indicator_char: char::from_u32(wp.fill_column_indicator_char as u32).unwrap_or('|'),
                fill_column_indicator_fg: wp.fill_column_indicator_fg,
                extra_line_spacing: wp.extra_line_spacing,
                cursor_in_non_selected: wp.cursor_in_non_selected != 0,
                selective_display: wp.selective_display,
                escape_glyph_fg: wp.escape_glyph_fg,
                nobreak_char_display: wp.nobreak_char_display,
                nobreak_char_fg: wp.nobreak_char_fg,
                wrap_prefix: if wp.wrap_prefix_len > 0 {
                    wp.wrap_prefix[..wp.wrap_prefix_len as usize].to_vec()
                } else {
                    Vec::new()
                },
                line_prefix: if wp.line_prefix_len > 0 {
                    wp.line_prefix[..wp.line_prefix_len as usize].to_vec()
                } else {
                    Vec::new()
                },
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

            // Draw vertical border on right side if window doesn't reach frame edge
            let right_edge = params.bounds.x + params.bounds.width;
            if right_edge < frame_params.width - 1.0 {
                let border_color = Color::from_pixel(frame_params.vertical_border_fg);
                frame_glyphs.add_stretch(
                    right_edge,
                    params.bounds.y,
                    1.0,
                    params.bounds.height,
                    border_color,
                    0,
                    false,
                );
            }
        }
    }

    /// Apply face data from FFI to the FrameGlyphBuffer's current face state.
    unsafe fn apply_face(&self, face: &FaceDataFFI, frame_glyphs: &mut FrameGlyphBuffer) {
        let fg = Color::from_pixel(face.fg);
        let bg = Color::from_pixel(face.bg);
        let bold = face.font_weight >= 700;
        let italic = face.italic != 0;

        // Get font family string from C pointer
        let font_family = if !face.font_family.is_null() {
            CStr::from_ptr(face.font_family).to_str().unwrap_or("monospace")
        } else {
            "monospace"
        };

        let underline_color = if face.underline_style > 0 {
            Some(Color::from_pixel(face.underline_color))
        } else {
            None
        };

        let strike_color = if face.strike_through > 0 {
            Some(Color::from_pixel(face.strike_through_color))
        } else {
            None
        };

        let overline_color = if face.overline > 0 {
            Some(Color::from_pixel(face.overline_color))
        } else {
            None
        };

        frame_glyphs.set_face_with_font(
            face.face_id,
            fg,
            Some(bg),
            font_family,
            bold,
            italic,
            face.font_size as f32,
            face.underline_style as u8,
            underline_color,
            face.strike_through as u8,
            strike_color,
            face.overline as u8,
            overline_color,
        );
    }

    /// Layout a single window's buffer content.
    ///
    /// Phase 1+2: Monospace layout with per-character face resolution.
    /// - Fixed-width characters on a grid
    /// - Per-character face colors (syntax highlighting)
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
        let window = wp.window_ptr;
        if buffer.is_null() || window.is_null() {
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
        let char_h = params.char_height + params.extra_line_spacing;
        let ascent = params.font_ascent;

        // Fringe dimensions (use actual widths from window params)
        let left_fringe_width = params.left_fringe_width;
        let left_fringe_x = params.text_bounds.x - left_fringe_width;
        let right_fringe_x = params.text_bounds.x + params.text_bounds.width;
        let right_fringe_width = params.right_fringe_width;

        // Check line number configuration
        let mut lnum_config = LineNumberConfigFFI::default();
        let lnum_enabled = neomacs_layout_line_number_config(
            window,
            buffer,
            params.buffer_size,
            (text_height / char_h).floor() as i32,
            &mut lnum_config,
        ) == 0 && lnum_config.mode > 0;

        let lnum_cols = if lnum_enabled { lnum_config.width } else { 0 };
        let lnum_pixel_width = lnum_cols as f32 * char_w;

        // How many columns and rows fit (accounting for line numbers)
        let cols = ((text_width - lnum_pixel_width) / char_w).floor() as i32;
        let max_rows = (text_height / char_h).floor() as i32;

        if cols <= 0 || max_rows <= 0 {
            return;
        }

        // Effective text start X (shifted right for line numbers)
        let content_x = text_x + lnum_pixel_width;

        // Trigger fontification (jit-lock) for the visible region so that
        // face text properties are set before we read them.
        let read_chars = (params.buffer_size - params.window_start + 1).min(cols as i64 * max_rows as i64 * 2);
        let fontify_end = (params.window_start + read_chars).min(params.buffer_size);
        neomacs_layout_ensure_fontified(buffer, params.window_start, fontify_end);

        // Read buffer text from window_start
        let bytes_read = if read_chars <= 0 {
            0
        } else {
            let buf_size = (read_chars * 4) as usize;
            self.text_buf.resize(buf_size, 0);
            let n = neomacs_layout_buffer_text(
                buffer,
                params.window_start,
                (params.window_start + read_chars).min(params.buffer_size),
                self.text_buf.as_mut_ptr(),
                buf_size as i64,
            );
            n.max(0)
        };

        let text = if bytes_read > 0 {
            &self.text_buf[..bytes_read as usize]
        } else {
            &[]
        };

        // Default face colors (fallback)
        let default_fg = Color::from_pixel(params.default_fg);
        let default_bg = Color::from_pixel(params.default_bg);

        // Set initial default face
        frame_glyphs.set_face(
            0, // DEFAULT_FACE_ID
            default_fg,
            Some(default_bg),
            false, false,
            0, None, 0, None, 0, None,
        );

        // Face resolution state: we only call face_at_pos when charpos >= next_face_check
        let mut current_face_id: i32 = -1; // force first lookup
        let mut next_face_check: i64 = 0;
        let mut face_fg = default_fg;
        let mut face_bg = default_bg;

        // Invisible text state: next charpos where we need to re-check
        let mut next_invis_check: i64 = params.window_start;

        // Display text property state
        let mut next_display_check: i64 = params.window_start;
        let mut display_prop = DisplayPropFFI::default();
        let mut display_str_buf = [0u8; 1024];

        // Overlay string buffers
        let mut overlay_before_buf = [0u8; 512];
        let mut overlay_after_buf = [0u8; 512];
        let mut overlay_before_len: i32 = 0;
        let mut overlay_after_len: i32 = 0;

        // Line number state
        let mut current_line: i64 = if lnum_enabled {
            neomacs_layout_count_line_number(
                buffer, params.window_start, lnum_config.widen,
            )
        } else {
            1
        };
        let point_line: i64 = if lnum_enabled && lnum_config.mode >= 2 {
            neomacs_layout_count_line_number(
                buffer, params.point, lnum_config.widen,
            )
        } else {
            0
        };
        let mut lnum_face = FaceDataFFI::default();
        let mut need_line_number = lnum_enabled; // render on first row

        // Horizontal scroll: skip first hscroll columns
        let hscroll = if params.truncate_lines { params.hscroll.max(0) } else { 0 };
        // Reserve 1 column for truncation indicator when needed
        let show_left_trunc = hscroll > 0;

        // Walk through text, placing characters on the grid
        let mut col = 0i32;
        let mut row = 0i32;
        let mut charpos = params.window_start;
        let mut cursor_placed = false;
        let mut cursor_col = 0i32;
        let mut cursor_row = 0i32;
        let mut window_end_charpos = params.window_start;
        let mut byte_idx = 0usize;
        // hscroll state: how many columns to skip on each line
        let mut hscroll_remaining = hscroll;

        // Fringe indicator tracking:
        // row_continued[row] = true if row wraps to next line (show \ in right fringe)
        // row_continuation[row] = true if row is a continuation from prev (show \ in left fringe)
        let mut row_continued = vec![false; max_rows as usize];
        let mut row_continuation = vec![false; max_rows as usize];
        let mut row_truncated = vec![false; max_rows as usize];

        // Trailing whitespace tracking
        let trailing_ws_bg = if params.show_trailing_whitespace {
            Some(Color::from_pixel(params.trailing_ws_bg))
        } else {
            None
        };
        let mut trailing_ws_start_col: i32 = -1; // -1 = no trailing ws
        let mut trailing_ws_row: i32 = 0;

        // Word-wrap tracking: position after last breakable whitespace
        let mut wrap_break_col = 0i32;
        let mut wrap_break_byte_idx = 0usize;
        let mut wrap_break_charpos = params.window_start;
        let mut wrap_break_glyph_count = 0usize;
        let mut wrap_has_break = false;

        // Line/wrap prefix tracking: 0=none, 1=line_prefix, 2=wrap_prefix
        let mut need_prefix: u8 = if !params.line_prefix.is_empty() { 1 } else { 0 };

        // Raise display property: Y offset applied to glyphs
        let mut raise_y_offset: f32 = 0.0;
        let mut raise_end: i64 = 0;

        while byte_idx < bytes_read as usize && row < max_rows {
            // Render line number at the start of each new row
            if need_line_number && lnum_enabled {
                // Determine displayed number based on mode
                let display_num = match lnum_config.mode {
                    2 => {
                        // Relative mode
                        if lnum_config.current_absolute != 0
                            && current_line == point_line
                        {
                            current_line + lnum_config.offset as i64
                        } else {
                            (current_line - point_line).abs()
                        }
                    }
                    3 => {
                        // Visual mode: relative to point line
                        if lnum_config.current_absolute != 0
                            && current_line == point_line
                        {
                            current_line + lnum_config.offset as i64
                        } else {
                            (current_line - point_line).abs()
                        }
                    }
                    _ => {
                        // Absolute mode
                        current_line + lnum_config.offset as i64
                    }
                };

                let is_current = if current_line == point_line { 1 } else { 0 };
                neomacs_layout_line_number_face(
                    window,
                    is_current,
                    current_line,
                    lnum_config.major_tick,
                    lnum_config.minor_tick,
                    &mut lnum_face,
                );

                // Apply line number face and render digits
                self.apply_face(&lnum_face, frame_glyphs);
                let lnum_bg = Color::from_pixel(lnum_face.bg);

                // Format the number right-aligned
                let num_str = format!("{}", display_num);
                let num_chars = num_str.len() as i32;
                let padding = (lnum_cols - 1) - num_chars; // -1 for trailing space

                let gy = text_y + row as f32 * char_h;

                // Leading padding
                if padding > 0 {
                    frame_glyphs.add_stretch(
                        text_x, gy,
                        padding as f32 * char_w, char_h,
                        lnum_bg, lnum_face.face_id, false,
                    );
                }

                // Number digits
                for (i, ch) in num_str.chars().enumerate() {
                    let dx = text_x + (padding.max(0) + i as i32) as f32 * char_w;
                    frame_glyphs.add_char(ch, dx, gy, char_w, char_h, ascent, false);
                }

                // Trailing space
                let space_x = text_x + (lnum_cols - 1) as f32 * char_w;
                frame_glyphs.add_stretch(
                    space_x, gy,
                    char_w, char_h,
                    lnum_bg, lnum_face.face_id, false,
                );

                // Restore text face
                if current_face_id >= 0 {
                    self.apply_face(&self.face_data, frame_glyphs);
                }

                need_line_number = false;
            }

            // Render line-prefix or wrap-prefix at start of visual lines
            if need_prefix > 0 && row < max_rows {
                let prefix_bytes = if need_prefix == 2 {
                    &params.wrap_prefix[..]
                } else {
                    &params.line_prefix[..]
                };

                if !prefix_bytes.is_empty() {
                    let mut pi = 0usize;
                    while pi < prefix_bytes.len() {
                        let (pch, plen) = decode_utf8(&prefix_bytes[pi..]);
                        pi += plen;
                        if pch == '\n' || pch == '\r' { continue; }

                        let pchar_cols = if is_wide_char(pch) { 2 } else { 1 };
                        if col + pchar_cols > cols { break; }

                        let gx = content_x + col as f32 * char_w;
                        let gy = text_y + row as f32 * char_h;
                        frame_glyphs.add_char(
                            pch, gx, gy, pchar_cols as f32 * char_w,
                            char_h, ascent, false,
                        );
                        col += pchar_cols;
                    }
                }
                need_prefix = 0;
            }

            // Handle hscroll: show $ indicator and skip columns
            if hscroll_remaining > 0 {
                // Skip characters consumed by hscroll
                let (ch, ch_len) = decode_utf8(&text[byte_idx..]);
                byte_idx += ch_len;
                charpos += 1;

                if ch == '\n' {
                    // Newline within hscroll region: new line
                    col = 0;
                    row += 1;
                    current_line += 1;
                    need_line_number = lnum_enabled;
                    hscroll_remaining = hscroll; // reset for next line
                    wrap_has_break = false;
                } else {
                    let ch_cols = if ch == '\t' {
                        let tab_w = params.tab_width.max(1);
                        ((hscroll - hscroll_remaining) / tab_w + 1) * tab_w - (hscroll - hscroll_remaining)
                    } else if is_wide_char(ch) {
                        2
                    } else {
                        1
                    };
                    hscroll_remaining -= ch_cols.min(hscroll_remaining);

                    // When hscroll is done, show $ at left edge
                    if hscroll_remaining <= 0 && show_left_trunc {
                        let gy = text_y + row as f32 * char_h;
                        frame_glyphs.add_char('$', content_x, gy, char_w, char_h, ascent, false);
                        col = 1; // $ takes 1 column
                    }
                }
                window_end_charpos = charpos;
                continue;
            }

            // Check for invisible text at property change boundaries
            if charpos >= next_invis_check {
                let mut next_visible: i64 = 0;
                let invis = neomacs_layout_check_invisible(
                    buffer,
                    window,
                    charpos,
                    &mut next_visible,
                );

                if invis > 0 {
                    // Skip invisible characters: advance byte_idx
                    // and charpos to next_visible
                    let chars_to_skip = next_visible - charpos;
                    for _ in 0..chars_to_skip {
                        if byte_idx >= bytes_read as usize {
                            break;
                        }
                        let (_, ch_len) = decode_utf8(&text[byte_idx..]);
                        byte_idx += ch_len;
                    }
                    // Show ellipsis for invis==2
                    if invis == 2 && col + 3 <= cols && row < max_rows {
                        let gy = text_y + row as f32 * char_h;
                        for _ in 0..3 {
                            let dx = content_x + col as f32 * char_w;
                            frame_glyphs.add_char(
                                '.', dx, gy, char_w, char_h, ascent, false,
                            );
                            col += 1;
                        }
                    }
                    charpos = next_visible;
                    next_invis_check = next_visible;
                    // Force face re-check at new position
                    current_face_id = -1;
                    continue;
                } else {
                    // Visible: next_visible tells us when to re-check
                    next_invis_check = if next_visible > charpos {
                        next_visible
                    } else {
                        charpos + 1
                    };
                }
            }

            // Check for overlay before-string/after-string at this position.
            // Before-strings render at overlay start, after-strings at end.
            {
                overlay_before_len = 0;
                overlay_after_len = 0;
                neomacs_layout_overlay_strings_at(
                    buffer, window, charpos,
                    overlay_before_buf.as_mut_ptr(),
                    overlay_before_buf.len() as i32,
                    &mut overlay_before_len,
                    overlay_after_buf.as_mut_ptr(),
                    overlay_after_buf.len() as i32,
                    &mut overlay_after_len,
                );

                // Render before-string (if any) — insert before buffer text
                if overlay_before_len > 0 {
                    // Resolve face for this position first
                    if charpos >= next_face_check || current_face_id < 0 {
                        let mut next_check: i64 = 0;
                        let fid = neomacs_layout_face_at_pos(
                            window, charpos,
                            &mut self.face_data as *mut FaceDataFFI,
                            &mut next_check,
                        );
                        if fid >= 0 && fid != current_face_id {
                            current_face_id = fid;
                            face_fg = Color::from_pixel(self.face_data.fg);
                            face_bg = Color::from_pixel(self.face_data.bg);
                            self.apply_face(&self.face_data, frame_glyphs);
                        }
                        next_face_check = if next_check > charpos { next_check } else { charpos + 1 };
                    }

                    let bstr = &overlay_before_buf[..overlay_before_len as usize];
                    let mut bi = 0usize;
                    while bi < bstr.len() && row < max_rows {
                        let (bch, blen) = decode_utf8(&bstr[bi..]);
                        bi += blen;
                        if bch == '\n' || bch == '\r' { continue; }

                        let bchar_cols = if is_wide_char(bch) { 2 } else { 1 };
                        if col + bchar_cols > cols {
                            if params.truncate_lines { break; }
                            col = 0;
                            row += 1;
                            if row >= max_rows { break; }
                        }
                        let gx = content_x + col as f32 * char_w;
                        let gy = text_y + row as f32 * char_h;
                        frame_glyphs.add_char(bch, gx, gy, bchar_cols as f32 * char_w, char_h, ascent, false);
                        col += bchar_cols;
                    }
                }

                // After-strings are rendered after the buffer text at the
                // position, so we defer rendering. We'll render them after
                // the character at this position has been processed.
                // (Stored in overlay_after_len for use after char rendering)
            }

            // Check for display text property at property boundaries
            if charpos >= next_display_check {
                neomacs_layout_check_display_prop(
                    buffer,
                    window,
                    charpos,
                    display_str_buf.as_mut_ptr(),
                    display_str_buf.len() as i32,
                    &mut display_prop,
                );

                if display_prop.prop_type == 1 {
                    // String replacement: render the display string instead
                    // of buffer text, skip original chars up to covers_to.

                    // First resolve face at this position
                    if charpos >= next_face_check || current_face_id < 0 {
                        let mut next_check: i64 = 0;
                        let fid = neomacs_layout_face_at_pos(
                            window, charpos,
                            &mut self.face_data as *mut FaceDataFFI,
                            &mut next_check,
                        );
                        if fid >= 0 && fid != current_face_id {
                            current_face_id = fid;
                            face_fg = Color::from_pixel(self.face_data.fg);
                            face_bg = Color::from_pixel(self.face_data.bg);
                            self.apply_face(&self.face_data, frame_glyphs);
                        }
                        next_face_check = if next_check > charpos { next_check } else { charpos + 1 };
                    }

                    // Render display string characters
                    let dstr = &display_str_buf[..display_prop.str_len as usize];
                    let mut di = 0usize;
                    while di < dstr.len() && row < max_rows {
                        let (dch, dlen) = decode_utf8(&dstr[di..]);
                        di += dlen;

                        if dch == '\n' || dch == '\r' {
                            continue;
                        }

                        let dchar_cols = if is_wide_char(dch) { 2 } else { 1 };
                        if col + dchar_cols > cols {
                            if params.truncate_lines {
                                break;
                            }
                            col = 0;
                            row += 1;
                            if row >= max_rows { break; }
                        }

                        let gx = content_x + col as f32 * char_w;
                        let gy = text_y + row as f32 * char_h;
                        let glyph_w = dchar_cols as f32 * char_w;
                        frame_glyphs.add_char(dch, gx, gy, glyph_w, char_h, ascent, false);
                        col += dchar_cols;
                    }

                    // Skip original buffer text covered by this display prop
                    let chars_to_skip = display_prop.covers_to - charpos;
                    for _ in 0..chars_to_skip {
                        if byte_idx >= bytes_read as usize { break; }
                        let (_, ch_len) = decode_utf8(&text[byte_idx..]);
                        byte_idx += ch_len;
                    }
                    charpos = display_prop.covers_to;
                    window_end_charpos = charpos;
                    next_display_check = display_prop.covers_to;
                    current_face_id = -1; // force re-check at new position
                    continue;
                } else if display_prop.prop_type == 2 {
                    // Space spec: render a stretch glyph

                    // Resolve face first
                    if charpos >= next_face_check || current_face_id < 0 {
                        let mut next_check: i64 = 0;
                        let fid = neomacs_layout_face_at_pos(
                            window, charpos,
                            &mut self.face_data as *mut FaceDataFFI,
                            &mut next_check,
                        );
                        if fid >= 0 && fid != current_face_id {
                            current_face_id = fid;
                            face_fg = Color::from_pixel(self.face_data.fg);
                            face_bg = Color::from_pixel(self.face_data.bg);
                            self.apply_face(&self.face_data, frame_glyphs);
                        }
                        next_face_check = if next_check > charpos { next_check } else { charpos + 1 };
                    }

                    let space_cols = display_prop.space_width.ceil() as i32;
                    let space_pixel_w = display_prop.space_width * char_w;

                    if col + space_cols <= cols && row < max_rows {
                        let gx = content_x + col as f32 * char_w;
                        let gy = text_y + row as f32 * char_h;
                        frame_glyphs.add_stretch(
                            gx, gy, space_pixel_w, char_h,
                            face_bg, self.face_data.face_id, false,
                        );
                        col += space_cols;
                    }

                    // Skip original buffer text
                    let chars_to_skip = display_prop.covers_to - charpos;
                    for _ in 0..chars_to_skip {
                        if byte_idx >= bytes_read as usize { break; }
                        let (_, ch_len) = decode_utf8(&text[byte_idx..]);
                        byte_idx += ch_len;
                    }
                    charpos = display_prop.covers_to;
                    window_end_charpos = charpos;
                    next_display_check = display_prop.covers_to;
                    current_face_id = -1;
                    continue;
                } else if display_prop.prop_type == 3 {
                    // Align-to spec: render stretch from current col to target column

                    // Resolve face first
                    if charpos >= next_face_check || current_face_id < 0 {
                        let mut next_check: i64 = 0;
                        let fid = neomacs_layout_face_at_pos(
                            window, charpos,
                            &mut self.face_data as *mut FaceDataFFI,
                            &mut next_check,
                        );
                        if fid >= 0 && fid != current_face_id {
                            current_face_id = fid;
                            face_fg = Color::from_pixel(self.face_data.fg);
                            face_bg = Color::from_pixel(self.face_data.bg);
                            self.apply_face(&self.face_data, frame_glyphs);
                        }
                        next_face_check = if next_check > charpos { next_check } else { charpos + 1 };
                    }

                    let target_col = display_prop.align_to.ceil() as i32;
                    if target_col > col && row < max_rows {
                        let stretch_cols = target_col - col;
                        let gx = content_x + col as f32 * char_w;
                        let gy = text_y + row as f32 * char_h;
                        let stretch_w = stretch_cols as f32 * char_w;
                        frame_glyphs.add_stretch(
                            gx, gy, stretch_w, char_h,
                            face_bg, self.face_data.face_id, false,
                        );
                        col = target_col;
                    }

                    // Skip original buffer text
                    let chars_to_skip = display_prop.covers_to - charpos;
                    for _ in 0..chars_to_skip {
                        if byte_idx >= bytes_read as usize { break; }
                        let (_, ch_len) = decode_utf8(&text[byte_idx..]);
                        byte_idx += ch_len;
                    }
                    charpos = display_prop.covers_to;
                    window_end_charpos = charpos;
                    next_display_check = display_prop.covers_to;
                    current_face_id = -1;
                    continue;
                } else if display_prop.prop_type == 4 {
                    // Image display property: render image glyph
                    let img_w = display_prop.image_width as f32;
                    let img_h = display_prop.image_height as f32;

                    if row < max_rows && display_prop.image_gpu_id != 0 {
                        let gx = content_x + col as f32 * char_w;
                        let gy = text_y + row as f32 * char_h;
                        frame_glyphs.add_image(
                            display_prop.image_gpu_id,
                            gx, gy, img_w, img_h,
                        );
                        // Advance by image width in columns
                        let img_cols = (img_w / char_w).ceil() as i32;
                        col += img_cols;
                        // Advance rows if image is taller than one line
                        let img_rows = ((img_h / char_h).ceil() as i32).max(1);
                        if img_rows > 1 {
                            row += img_rows - 1;
                        }
                    }

                    // Skip original buffer text
                    let chars_to_skip = display_prop.covers_to - charpos;
                    for _ in 0..chars_to_skip {
                        if byte_idx >= bytes_read as usize { break; }
                        let (_, ch_len) = decode_utf8(&text[byte_idx..]);
                        byte_idx += ch_len;
                    }
                    charpos = display_prop.covers_to;
                    window_end_charpos = charpos;
                    next_display_check = display_prop.covers_to;
                    current_face_id = -1;
                    continue;
                } else if display_prop.prop_type == 5 {
                    // Raise: set Y offset for subsequent glyphs until covers_to
                    raise_y_offset = -(display_prop.raise_factor * char_h);
                    raise_end = display_prop.covers_to;
                    next_display_check = display_prop.covers_to;
                    // Don't skip text - raise modifies rendering, not content
                } else {
                    // No display prop: covers_to tells us when to re-check
                    next_display_check = display_prop.covers_to;
                }

                // Reset raise offset when past the raise region
                if raise_end > 0 && charpos >= raise_end {
                    raise_y_offset = 0.0;
                    raise_end = 0;
                }
            }

            // Resolve face if needed (when entering a new face region)
            if charpos >= next_face_check || current_face_id < 0 {
                let mut next_check: i64 = 0;
                let fid = neomacs_layout_face_at_pos(
                    window,
                    charpos,
                    &mut self.face_data as *mut FaceDataFFI,
                    &mut next_check,
                );

                if fid >= 0 {
                    if fid != current_face_id {
                        current_face_id = fid;
                        face_fg = Color::from_pixel(self.face_data.fg);
                        face_bg = Color::from_pixel(self.face_data.bg);
                        self.apply_face(&self.face_data, frame_glyphs);
                    }
                    // next_check is 0 when face_at_buffer_position returns no limit
                    next_face_check = if next_check > charpos { next_check } else { charpos + 1 };
                } else {
                    // Fallback to default face
                    next_face_check = charpos + 1;
                }
            }

            // Check if cursor is at this position
            if !cursor_placed && charpos >= params.point {
                cursor_col = col;
                cursor_row = row;
                let cursor_x = content_x + col as f32 * char_w;
                let cursor_y = text_y + row as f32 * char_h;

                let (cursor_w, cursor_h) = match params.cursor_type {
                    1 => (params.cursor_bar_width.max(1) as f32, char_h), // bar
                    2 => (char_w, 2.0),                                    // hbar
                    _ => (char_w, char_h),                                 // box/hollow
                };

                let cursor_style = if params.selected {
                    params.cursor_type
                } else if params.cursor_in_non_selected {
                    3 // hollow for inactive windows
                } else {
                    255 // skip: no cursor in non-selected windows
                };

                if cursor_style < 255 {
                    frame_glyphs.add_cursor(
                        params.window_id as i32,
                        cursor_x,
                        cursor_y,
                        cursor_w,
                        cursor_h,
                        cursor_style,
                        face_fg,
                    );

                    // Set inverse for filled box cursor
                    if cursor_style == 0 {
                        frame_glyphs.set_cursor_inverse(
                            cursor_x,
                            cursor_y,
                            cursor_w,
                            cursor_h,
                            face_fg,     // cursor_bg = text fg
                            face_bg,     // cursor_fg = text bg (inverse)
                        );
                    }
                }

                cursor_placed = true;
            }

            // Decode one UTF-8 character
            let (ch, ch_len) = decode_utf8(&text[byte_idx..]);
            byte_idx += ch_len;
            charpos += 1;

            match ch {
                '\n' => {
                    // Highlight trailing whitespace (overlay stretch on top)
                    if let Some(tw_bg) = trailing_ws_bg {
                        if trailing_ws_start_col >= 0 && trailing_ws_row == row {
                            let tw_x = content_x + trailing_ws_start_col as f32 * char_w;
                            let tw_w = (col - trailing_ws_start_col) as f32 * char_w;
                            let gy = text_y + row as f32 * char_h;
                            if tw_w > 0.0 {
                                frame_glyphs.add_stretch(tw_x, gy, tw_w, char_h, tw_bg, 0, false);
                            }
                        }
                    }
                    trailing_ws_start_col = -1;

                    // Fill rest of line with stretch (use face bg)
                    let remaining = (cols - col) as f32 * char_w;
                    if remaining > 0.0 {
                        let gx = content_x + col as f32 * char_w;
                        let gy = text_y + row as f32 * char_h;
                        frame_glyphs.add_stretch(gx, gy, remaining, char_h, face_bg, self.face_data.face_id, false);
                    }
                    col = 0;
                    row += 1;
                    current_line += 1;
                    need_line_number = lnum_enabled;
                    wrap_has_break = false;
                    hscroll_remaining = hscroll;
                    if !params.line_prefix.is_empty() { need_prefix = 1; }

                    // Selective display: skip lines indented beyond threshold
                    if params.selective_display > 0 {
                        let mut shown_ellipsis = false;
                        while byte_idx < bytes_read as usize && row < max_rows {
                            // Peek at indentation of next line
                            let mut indent = 0i32;
                            let mut peek_idx = byte_idx;
                            while peek_idx < bytes_read as usize {
                                let (pch, plen) = decode_utf8(&text[peek_idx..]);
                                if pch == ' ' {
                                    indent += 1;
                                } else if pch == '\t' {
                                    indent = ((indent / params.tab_width) + 1) * params.tab_width;
                                } else {
                                    break;
                                }
                                peek_idx += plen;
                            }

                            if indent > params.selective_display {
                                // Show ... ellipsis once for the hidden block
                                if !shown_ellipsis && row > 0 {
                                    let gy = text_y + (row - 1) as f32 * char_h;
                                    for dot_i in 0..3i32.min(cols) {
                                        frame_glyphs.add_char(
                                            '.', content_x + dot_i as f32 * char_w,
                                            gy, char_w, char_h, ascent, false,
                                        );
                                    }
                                    shown_ellipsis = true;
                                }
                                // Skip this hidden line
                                while byte_idx < bytes_read as usize {
                                    let (sch, slen) = decode_utf8(&text[byte_idx..]);
                                    byte_idx += slen;
                                    charpos += 1;
                                    if sch == '\n' {
                                        current_line += 1;
                                        break;
                                    }
                                }
                            } else {
                                break; // Next line is visible
                            }
                        }
                    }
                }
                '\t' => {
                    // Tab: advance to next tab stop
                    let tab_w = params.tab_width.max(1);
                    let next_tab = ((col / tab_w) + 1) * tab_w;
                    let spaces = (next_tab - col).min(cols - col);

                    // Render tab as stretch glyph (use face bg)
                    let gx = content_x + col as f32 * char_w;
                    let gy = text_y + row as f32 * char_h;
                    let tab_pixel_w = spaces as f32 * char_w;
                    frame_glyphs.add_stretch(gx, gy, tab_pixel_w, char_h, face_bg, self.face_data.face_id, false);

                    col += spaces;
                    // Tab is a breakpoint for word-wrap
                    if params.word_wrap {
                        wrap_break_col = col;
                        wrap_break_byte_idx = byte_idx;
                        wrap_break_charpos = charpos;
                        wrap_break_glyph_count = frame_glyphs.glyphs.len();
                        wrap_has_break = true;
                    }
                    if col >= cols {
                        if params.truncate_lines {
                            if (row as usize) < row_truncated.len() {
                                row_truncated[row as usize] = true;
                            }
                            while byte_idx < bytes_read as usize {
                                let (c, l) = decode_utf8(&text[byte_idx..]);
                                byte_idx += l;
                                charpos += 1;
                                if c == '\n' {
                                    col = 0;
                                    row += 1;
                                    current_line += 1;
                                    need_line_number = lnum_enabled;
                                    wrap_has_break = false;
                                    break;
                                }
                            }
                        } else {
                            if (row as usize) < row_continued.len() {
                                row_continued[row as usize] = true;
                            }
                            col = 0;
                            row += 1;
                            if (row as usize) < row_continuation.len() {
                                row_continuation[row as usize] = true;
                            }
                            wrap_has_break = false;
                            if !params.wrap_prefix.is_empty() { need_prefix = 2; }
                        }
                    }
                }
                '\r' => {
                    if params.selective_display > 0 {
                        // In selective-display mode, \r hides until next \n
                        // Show ... ellipsis
                        let gy = text_y + row as f32 * char_h;
                        if col + 3 <= cols {
                            for dot_i in 0..3 {
                                frame_glyphs.add_char(
                                    '.', content_x + (col + dot_i) as f32 * char_w,
                                    gy, char_w, char_h, ascent, false,
                                );
                            }
                        }
                        // Skip to next \n
                        while byte_idx < bytes_read as usize {
                            let (sch, slen) = decode_utf8(&text[byte_idx..]);
                            byte_idx += slen;
                            charpos += 1;
                            if sch == '\n' {
                                col = 0;
                                row += 1;
                                current_line += 1;
                                need_line_number = lnum_enabled;
                                wrap_has_break = false;
                                hscroll_remaining = hscroll;
                                break;
                            }
                        }
                    }
                    // Otherwise: carriage return is just skipped
                }
                _ if ch < ' ' => {
                    // Control character: display as ^X (2 columns)
                    // Use escape-glyph face for control char display
                    let escape_fg = Color::from_pixel(params.escape_glyph_fg);
                    frame_glyphs.set_face(
                        0, escape_fg, Some(face_bg),
                        false, false, 0, None, 0, None, 0, None,
                    );

                    let gx = content_x + col as f32 * char_w;
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
                        if params.truncate_lines {
                            while byte_idx < bytes_read as usize {
                                let (c, l) = decode_utf8(&text[byte_idx..]);
                                byte_idx += l;
                                charpos += 1;
                                if c == '\n' {
                                    col = 0;
                                    row += 1;
                                    current_line += 1;
                                    need_line_number = lnum_enabled;
                                    wrap_has_break = false;
                                    break;
                                }
                            }
                        } else {
                            col = 0;
                            row += 1;
                            wrap_has_break = false;
                        }
                    }
                    // Restore text face after escape-glyph
                    if current_face_id >= 0 {
                        self.apply_face(&self.face_data, frame_glyphs);
                    }
                }
                _ => {
                    // Non-breaking space and soft hyphen highlighting
                    if params.nobreak_char_display > 0 && (ch == '\u{00A0}' || ch == '\u{00AD}') {
                        let nb_fg = Color::from_pixel(params.nobreak_char_fg);
                        frame_glyphs.set_face(
                            0, nb_fg, Some(face_bg),
                            false, false, 0, None, 0, None, 0, None,
                        );
                        let gx = content_x + col as f32 * char_w;
                        let gy = text_y + row as f32 * char_h;
                        let display_ch = if ch == '\u{00A0}' { ' ' } else { '-' };
                        if col < cols {
                            frame_glyphs.add_char(display_ch, gx, gy, char_w, char_h, ascent, false);
                            col += 1;
                        }
                        // Restore text face
                        if current_face_id >= 0 {
                            self.apply_face(&self.face_data, frame_glyphs);
                        }
                        window_end_charpos = charpos;
                        continue;
                    }

                    // Combining/non-spacing character: render at previous position, no col advance
                    if is_combining_char(ch) {
                        if col > 0 {
                            let gx = content_x + (col - 1) as f32 * char_w;
                            let gy = text_y + row as f32 * char_h;
                            frame_glyphs.add_char(ch, gx, gy, 0.0, char_h, ascent, false);
                        }
                        // Don't advance column
                        window_end_charpos = charpos;
                        continue;
                    }

                    // Track word-wrap breakpoints: after space/tab
                    if params.word_wrap && (ch == ' ' || ch == '\t') {
                        // Record break AFTER this whitespace
                        // (will be updated after rendering below)
                    }

                    // Normal character
                    let char_cols = if is_wide_char(ch) { 2 } else { 1 };

                    if col + char_cols > cols {
                        // Line full
                        if params.truncate_lines {
                            // Show $ truncation indicator at right edge
                            let trunc_x = content_x + (cols - 1) as f32 * char_w;
                            let gy = text_y + row as f32 * char_h;
                            frame_glyphs.add_char('$', trunc_x, gy, char_w, char_h, ascent, false);
                            if (row as usize) < row_truncated.len() {
                                row_truncated[row as usize] = true;
                            }

                            while byte_idx < bytes_read as usize {
                                let (c, l) = decode_utf8(&text[byte_idx..]);
                                byte_idx += l;
                                charpos += 1;
                                if c == '\n' {
                                    col = 0;
                                    row += 1;
                                    current_line += 1;
                                    need_line_number = lnum_enabled;
                                    wrap_has_break = false;
                                    hscroll_remaining = hscroll;
                                    break;
                                }
                            }
                            continue;
                        } else if params.word_wrap && wrap_has_break && wrap_break_col > 0 {
                            // Word-wrap: rewind to last breakpoint
                            frame_glyphs.glyphs.truncate(wrap_break_glyph_count);
                            // Fill from break to end of line with bg
                            let fill_cols = cols - wrap_break_col;
                            if fill_cols > 0 {
                                let gx = content_x + wrap_break_col as f32 * char_w;
                                let gy = text_y + row as f32 * char_h;
                                frame_glyphs.add_stretch(
                                    gx, gy,
                                    fill_cols as f32 * char_w, char_h,
                                    face_bg, self.face_data.face_id, false,
                                );
                            }
                            if (row as usize) < row_continued.len() {
                                row_continued[row as usize] = true;
                            }
                            // Rewind position to the break
                            byte_idx = wrap_break_byte_idx;
                            charpos = wrap_break_charpos;
                            // Force face re-check since we rewound
                            current_face_id = -1;
                            col = 0;
                            row += 1;
                            if (row as usize) < row_continuation.len() {
                                row_continuation[row as usize] = true;
                            }
                            wrap_has_break = false;
                            if !params.wrap_prefix.is_empty() { need_prefix = 2; }
                            if row >= max_rows {
                                break;
                            }
                            continue;
                        } else {
                            // Character wrap: fill remaining space
                            let remaining = (cols - col) as f32 * char_w;
                            if remaining > 0.0 {
                                let gx = content_x + col as f32 * char_w;
                                let gy = text_y + row as f32 * char_h;
                                frame_glyphs.add_stretch(gx, gy, remaining, char_h, face_bg, self.face_data.face_id, false);
                            }
                            if (row as usize) < row_continued.len() {
                                row_continued[row as usize] = true;
                            }
                            col = 0;
                            row += 1;
                            if (row as usize) < row_continuation.len() {
                                row_continuation[row as usize] = true;
                            }
                            wrap_has_break = false;
                            if !params.wrap_prefix.is_empty() { need_prefix = 2; }
                            if row >= max_rows {
                                break;
                            }
                        }
                    }

                    let gx = content_x + col as f32 * char_w;
                    let gy = text_y + row as f32 * char_h + raise_y_offset;
                    let glyph_w = char_cols as f32 * char_w;

                    frame_glyphs.add_char(ch, gx, gy, glyph_w, char_h, ascent, false);
                    col += char_cols;

                    // Track trailing whitespace
                    if trailing_ws_bg.is_some() {
                        if ch == ' ' || ch == '\t' {
                            if trailing_ws_start_col < 0 {
                                trailing_ws_start_col = col - char_cols;
                                trailing_ws_row = row;
                            }
                        } else {
                            trailing_ws_start_col = -1;
                        }
                    }

                    // Record break AFTER whitespace characters
                    if params.word_wrap && (ch == ' ' || ch == '\t') {
                        wrap_break_col = col;
                        wrap_break_byte_idx = byte_idx;
                        wrap_break_charpos = charpos;
                        wrap_break_glyph_count = frame_glyphs.glyphs.len();
                        wrap_has_break = true;
                    }
                }
            }

            // Render overlay after-string (if any) — collected earlier
            if overlay_after_len > 0 && row < max_rows {
                let astr = &overlay_after_buf[..overlay_after_len as usize];
                let mut ai = 0usize;
                while ai < astr.len() && row < max_rows {
                    let (ach, alen) = decode_utf8(&astr[ai..]);
                    ai += alen;
                    if ach == '\n' || ach == '\r' { continue; }

                    let achar_cols = if is_wide_char(ach) { 2 } else { 1 };
                    if col + achar_cols > cols {
                        if params.truncate_lines { break; }
                        col = 0;
                        row += 1;
                        if row >= max_rows { break; }
                    }
                    let gx = content_x + col as f32 * char_w;
                    let gy = text_y + row as f32 * char_h;
                    frame_glyphs.add_char(ach, gx, gy, achar_cols as f32 * char_w, char_h, ascent, false);
                    col += achar_cols;
                }
            }

            window_end_charpos = charpos;
        }

        // If cursor wasn't placed (point is past visible content), place at end
        if !cursor_placed && params.point >= params.window_start {
            cursor_col = col;
            cursor_row = row.min(max_rows - 1);
            let cursor_x = content_x + col as f32 * char_w;
            let cursor_y = text_y + row.min(max_rows - 1) as f32 * char_h;

            let cursor_style = if params.selected {
                params.cursor_type
            } else if params.cursor_in_non_selected {
                3
            } else {
                255 // skip
            };

            if cursor_style < 255 {
                frame_glyphs.add_cursor(
                    params.window_id as i32,
                    cursor_x,
                    cursor_y,
                    char_w,
                    char_h,
                    cursor_style,
                    face_fg,
                );
            }

            if cursor_style == 0 {
                frame_glyphs.set_cursor_inverse(
                    cursor_x,
                    cursor_y,
                    char_w,
                    char_h,
                    face_fg,
                    face_bg,
                );
            }
        }

        // Fill remaining rows with default background
        let filled_rows = row + 1;
        if filled_rows < max_rows {
            let gy = text_y + filled_rows as f32 * char_h;
            let remaining_h = text_height - filled_rows as f32 * char_h;
            if remaining_h > 0.0 {
                frame_glyphs.add_stretch(text_x, gy, text_width, remaining_h, default_bg, 0, false);
            }
        }

        // Render fringe indicators
        let actual_rows = (row + 1).min(max_rows);
        if right_fringe_width > 0.0 || left_fringe_width > 0.0 {
            // Use default face for fringe rendering
            frame_glyphs.set_face(
                0, default_fg, Some(default_bg),
                false, false, 0, None, 0, None, 0, None,
            );

            for r in 0..actual_rows as usize {
                let gy = text_y + r as f32 * char_h;

                // Right fringe: continuation indicator for wrapped lines
                if right_fringe_width >= char_w && row_continued.get(r).copied().unwrap_or(false) {
                    let fx = right_fringe_x + (right_fringe_width - char_w) / 2.0;
                    frame_glyphs.add_char('\\', fx, gy, char_w, char_h, ascent, false);
                }

                // Right fringe: truncation indicator
                if right_fringe_width >= char_w && row_truncated.get(r).copied().unwrap_or(false) {
                    let fx = right_fringe_x + (right_fringe_width - char_w) / 2.0;
                    frame_glyphs.add_char('$', fx, gy, char_w, char_h, ascent, false);
                }

                // Left fringe: continuation indicator for continued lines
                if left_fringe_width >= char_w && row_continuation.get(r).copied().unwrap_or(false) {
                    let fx = left_fringe_x + (left_fringe_width - char_w) / 2.0;
                    frame_glyphs.add_char('\\', fx, gy, char_w, char_h, ascent, false);
                }
            }

            // EOB empty line indicators (tilde in fringe for lines past buffer end)
            if params.indicate_empty_lines > 0 {
                let eob_start = actual_rows;
                for r in eob_start as usize..max_rows as usize {
                    let gy = text_y + r as f32 * char_h;
                    if params.indicate_empty_lines == 2 {
                        // Right fringe
                        if right_fringe_width >= char_w {
                            let fx = right_fringe_x + (right_fringe_width - char_w) / 2.0;
                            frame_glyphs.add_char('~', fx, gy, char_w, char_h, ascent, false);
                        }
                    } else {
                        // Left fringe (default)
                        if left_fringe_width >= char_w {
                            let fx = left_fringe_x + (left_fringe_width - char_w) / 2.0;
                            frame_glyphs.add_char('~', fx, gy, char_w, char_h, ascent, false);
                        }
                    }
                }
            }
        }

        // Render fill-column indicator
        if params.fill_column_indicator > 0 {
            let fci_col = params.fill_column_indicator;
            let fci_char = params.fill_column_indicator_char;
            let fci_fg = Color::from_pixel(params.fill_column_indicator_fg);

            frame_glyphs.set_face(
                0, fci_fg, Some(default_bg),
                false, false, 0, None, 0, None, 0, None,
            );

            // Draw indicator character at the fill column on each row
            if fci_col < cols {
                for r in 0..max_rows {
                    let gx = content_x + fci_col as f32 * char_w;
                    let gy = text_y + r as f32 * char_h;
                    frame_glyphs.add_char(fci_char, gx, gy, char_w, char_h, ascent, false);
                }
            }
        }

        // Render tab-line if this window has one
        if params.tab_line_height > 0.0 {
            self.render_status_line(
                params.bounds.x,
                params.bounds.y,
                params.bounds.width,
                params.tab_line_height,
                params.char_width,
                params.font_ascent,
                wp,
                frame_glyphs,
                StatusLineKind::TabLine,
            );
        }

        // Render header-line if this window has one
        if params.header_line_height > 0.0 {
            self.render_status_line(
                params.bounds.x,
                params.bounds.y + params.tab_line_height,
                params.bounds.width,
                params.header_line_height,
                params.char_width,
                params.font_ascent,
                wp,
                frame_glyphs,
                StatusLineKind::HeaderLine,
            );
        }

        // Render mode-line if this window has one
        if params.mode_line_height > 0.0 {
            self.render_status_line(
                params.bounds.x,
                params.bounds.y + params.bounds.height - params.mode_line_height,
                params.bounds.width,
                params.mode_line_height,
                params.char_width,
                params.font_ascent,
                wp,
                frame_glyphs,
                StatusLineKind::ModeLine,
            );
        }

        // Write layout results back to Emacs
        neomacs_layout_set_window_end(
            wp.window_ptr,
            window_end_charpos,
            row.min(max_rows - 1),
        );

        // Set cursor position for Emacs (needed for recenter, scroll, etc.)
        neomacs_layout_set_cursor(
            wp.window_ptr,
            (content_x + cursor_col as f32 * char_w) as i32,
            (text_y + cursor_row as f32 * char_h) as i32,
            cursor_col,
            cursor_row,
        );
    }

    /// Render a status line (mode-line, header-line, or tab-line).
    unsafe fn render_status_line(
        &mut self,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        char_w: f32,
        ascent: f32,
        wp: &WindowParamsFFI,
        frame_glyphs: &mut FrameGlyphBuffer,
        kind: StatusLineKind,
    ) {
        let mut line_face = FaceDataFFI::default();
        let buf_size = 1024usize;
        let mut line_buf = vec![0u8; buf_size];

        let bytes = match kind {
            StatusLineKind::TabLine => neomacs_layout_tab_line_text(
                wp.window_ptr,
                std::ptr::null_mut(),
                line_buf.as_mut_ptr(),
                buf_size as i64,
                &mut line_face,
            ),
            StatusLineKind::HeaderLine => neomacs_layout_header_line_text(
                wp.window_ptr,
                std::ptr::null_mut(),
                line_buf.as_mut_ptr(),
                buf_size as i64,
                &mut line_face,
            ),
            StatusLineKind::ModeLine => neomacs_layout_mode_line_text(
                wp.window_ptr,
                std::ptr::null_mut(),
                line_buf.as_mut_ptr(),
                buf_size as i64,
                &mut line_face,
            ),
        };

        // Apply face
        self.apply_face(&line_face, frame_glyphs);
        let bg = Color::from_pixel(line_face.bg);

        // Draw background
        frame_glyphs.add_stretch(x, y, width, height, bg, line_face.face_id, true);

        if bytes <= 0 {
            return;
        }

        // Render text
        let text = &line_buf[..bytes as usize];
        let cols = (width / char_w).floor() as i32;
        let mut col = 0i32;
        let mut byte_idx = 0usize;

        while byte_idx < text.len() && col < cols {
            let (ch, ch_len) = decode_utf8(&text[byte_idx..]);
            byte_idx += ch_len;

            if ch == '\n' || ch == '\r' {
                continue;
            }

            let gx = x + col as f32 * char_w;
            frame_glyphs.add_char(ch, gx, y, char_w, height, ascent, true);
            col += 1;
        }

        // Fill remaining with background
        if col < cols {
            let gx = x + col as f32 * char_w;
            let remaining = (cols - col) as f32 * char_w;
            frame_glyphs.add_stretch(gx, y, remaining, height, bg, line_face.face_id, true);
        }
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
/// Check if a character is a Unicode combining/non-spacing mark.
/// These characters overlay the previous base character and don't advance the column.
fn is_combining_char(ch: char) -> bool {
    let cp = ch as u32;
    // Combining Diacritical Marks
    (0x0300..=0x036F).contains(&cp)
    // Combining Diacritical Marks Extended
    || (0x1AB0..=0x1AFF).contains(&cp)
    // Combining Diacritical Marks Supplement
    || (0x1DC0..=0x1DFF).contains(&cp)
    // Combining Diacritical Marks for Symbols
    || (0x20D0..=0x20FF).contains(&cp)
    // Combining Half Marks
    || (0xFE20..=0xFE2F).contains(&cp)
    // General Category Mn (non-spacing marks) in common scripts
    // Hebrew points and accents
    || (0x0591..=0x05BD).contains(&cp)
    || cp == 0x05BF
    || (0x05C1..=0x05C2).contains(&cp)
    || (0x05C4..=0x05C5).contains(&cp)
    || cp == 0x05C7
    // Arabic combining marks
    || (0x0610..=0x061A).contains(&cp)
    || (0x064B..=0x065F).contains(&cp)
    || cp == 0x0670
    || (0x06D6..=0x06DC).contains(&cp)
    || (0x06DF..=0x06E4).contains(&cp)
    || (0x06E7..=0x06E8).contains(&cp)
    || (0x06EA..=0x06ED).contains(&cp)
    // Devanagari combining marks
    || (0x0901..=0x0903).contains(&cp)
    || (0x093A..=0x094F).contains(&cp)
    || (0x0951..=0x0957).contains(&cp)
    || (0x0962..=0x0963).contains(&cp)
    // Thai combining marks
    || (0x0E31..=0x0E31).contains(&cp)
    || (0x0E34..=0x0E3A).contains(&cp)
    || (0x0E47..=0x0E4E).contains(&cp)
    // Hangul Jamo combining vowels/final consonants
    || (0x1160..=0x11FF).contains(&cp)
    // Zero-width characters
    || cp == 0x200B // zero-width space
    || cp == 0x200C // zero-width non-joiner
    || cp == 0x200D // zero-width joiner
    || cp == 0x200E // left-to-right mark
    || cp == 0x200F // right-to-left mark
    || cp == 0xFEFF // zero-width no-break space (BOM)
    // Variation selectors
    || (0xFE00..=0xFE0F).contains(&cp)
    || (0xE0100..=0xE01EF).contains(&cp)
}

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

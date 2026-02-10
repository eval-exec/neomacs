//! The Rust layout engine — Phase 1+2: Monospace layout with face resolution.
//!
//! Reads buffer text via FFI, resolves faces per character position,
//! computes line breaks, positions glyphs on a fixed-width grid, and
//! produces FrameGlyphBuffer compatible with the existing wgpu renderer.

use std::ffi::CStr;
use std::ffi::c_int;

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

/// A face run within an overlay/display string: byte offset + fg/bg colors.
struct OverlayFaceRun {
    byte_offset: u16,
    fg: u32,
    bg: u32,
}

/// Parse face runs appended after text in a buffer.
/// Runs are stored as 10-byte records: u16 byte_offset + u32 fg + u32 bg.
fn parse_overlay_face_runs(buf: &[u8], text_len: usize, nruns: i32) -> Vec<OverlayFaceRun> {
    let mut runs = Vec::with_capacity(nruns as usize);
    let runs_start = text_len;
    for ri in 0..nruns as usize {
        let off = runs_start + ri * 10;
        if off + 10 <= buf.len() {
            let byte_offset = u16::from_ne_bytes([buf[off], buf[off + 1]]);
            let fg = u32::from_ne_bytes([buf[off + 2], buf[off + 3], buf[off + 4], buf[off + 5]]);
            let bg = u32::from_ne_bytes([buf[off + 6], buf[off + 7], buf[off + 8], buf[off + 9]]);
            runs.push(OverlayFaceRun { byte_offset, fg, bg });
        }
    }
    runs
}

/// Apply the face run covering the current byte index.
/// Returns the updated current_run index.
fn apply_overlay_face_run(
    runs: &[OverlayFaceRun],
    byte_idx: usize,
    current_run: usize,
    frame_glyphs: &mut FrameGlyphBuffer,
) -> usize {
    let mut cr = current_run;
    // Advance to the correct run
    while cr + 1 < runs.len() && byte_idx >= runs[cr + 1].byte_offset as usize {
        cr += 1;
    }
    if byte_idx >= runs[cr].byte_offset as usize {
        let run = &runs[cr];
        if run.fg != 0 || run.bg != 0 {
            let rfg = Color::from_pixel(run.fg);
            let rbg = Color::from_pixel(run.bg);
            frame_glyphs.set_face(0, rfg, Some(rbg), false, false, 0, None, 0, None, 0, None);
        }
        // Pre-advance if next run starts at next byte
        if cr + 1 < runs.len() && byte_idx + 1 >= runs[cr + 1].byte_offset as usize {
            cr += 1;
        }
    }
    cr
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
    /// Per-face ASCII width cache for proportional fonts.
    /// Key: (face_id, font_size), Value: advance widths for chars 0-127.
    ascii_width_cache: std::collections::HashMap<(u32, i32), [f32; 128]>,
}

impl LayoutEngine {
    /// Create a new layout engine.
    pub fn new() -> Self {
        Self {
            text_buf: Vec::with_capacity(64 * 1024), // 64KB initial
            face_data: FaceDataFFI::default(),
            ascii_width_cache: std::collections::HashMap::new(),
        }
    }

    // char_advance is a standalone function (below) to avoid borrow conflicts
    // with self.text_buf

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
        log::debug!("layout_frame: {}x{} char={}x{} windows={}",
            frame_params.width, frame_params.height,
            frame_params.char_width, frame_params.char_height,
            window_count);

        for i in 0..window_count {
            let mut wp = WindowParamsFFI::default();
            let ret = neomacs_layout_get_window_params(frame, i, &mut wp);
            log::debug!("  window[{}]: id={} mini={} bounds=({},{},{},{}) bufsz={} start={} point={}",
                i, wp.window_id, wp.is_minibuffer,
                wp.x, wp.y, wp.width, wp.height,
                wp.buffer_zv, wp.window_start, wp.point);
            if ret != 0 {
                continue;
            }

            // Convert FFI params to our types
            let params = WindowParams {
                window_id: wp.window_id,
                buffer_id: wp.buffer_id,
                bounds: Rect::new(wp.x, wp.y, wp.width, wp.height),
                text_bounds: Rect::new(wp.text_x, wp.text_y, wp.text_width, wp.text_height),
                selected: wp.selected != 0,
                is_minibuffer: wp.is_minibuffer != 0,
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
                glyphless_char_fg: wp.glyphless_char_fg,
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
                left_margin_width: wp.left_margin_width,
                right_margin_width: wp.right_margin_width,
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
            // Extract buffer file name from FFI
            let buffer_file_name = if wp.buffer_file_name.is_null() {
                String::new()
            } else {
                CStr::from_ptr(wp.buffer_file_name).to_string_lossy().into_owned()
            };
            frame_glyphs.add_window_info(
                params.window_id,
                params.buffer_id,
                params.window_start,
                0, // window_end filled after layout
                params.buffer_size,
                params.bounds.x,
                params.bounds.y,
                params.bounds.width,
                params.bounds.height,
                params.mode_line_height,
                params.selected,
                params.is_minibuffer,
                params.char_height,
                buffer_file_name,
                wp.modified != 0,
            );

            // Layout this window's content
            self.layout_window(&params, &wp, frame, frame_glyphs);

            // Draw window dividers or simple vertical border
            let right_edge = params.bounds.x + params.bounds.width;
            let bottom_edge = params.bounds.y + params.bounds.height;
            let is_rightmost = right_edge >= frame_params.width - 1.0;
            let is_bottommost = bottom_edge >= frame_params.height - 1.0;

            if frame_params.right_divider_width > 0 && !is_rightmost {
                // Draw right divider with first/last pixel faces
                let dw = frame_params.right_divider_width as f32;
                let x0 = right_edge - dw;
                let y0 = params.bounds.y;
                let h = params.bounds.height
                    - if frame_params.bottom_divider_width > 0 && !is_bottommost {
                        frame_params.bottom_divider_width as f32
                    } else {
                        0.0
                    };
                let first_fg = Color::from_pixel(frame_params.divider_first_fg);
                let mid_fg = Color::from_pixel(frame_params.divider_fg);
                let last_fg = Color::from_pixel(frame_params.divider_last_fg);
                if dw >= 3.0 {
                    frame_glyphs.add_stretch(x0, y0, 1.0, h, first_fg, 0, false);
                    frame_glyphs.add_stretch(x0 + 1.0, y0, dw - 2.0, h, mid_fg, 0, false);
                    frame_glyphs.add_stretch(x0 + dw - 1.0, y0, 1.0, h, last_fg, 0, false);
                } else if dw >= 2.0 {
                    frame_glyphs.add_stretch(x0, y0, 1.0, h, first_fg, 0, false);
                    frame_glyphs.add_stretch(x0 + 1.0, y0, 1.0, h, last_fg, 0, false);
                } else {
                    frame_glyphs.add_stretch(x0, y0, 1.0, h, mid_fg, 0, false);
                }
            } else if !is_rightmost {
                // Fallback: simple 1px vertical border
                let border_color = Color::from_pixel(frame_params.vertical_border_fg);
                frame_glyphs.add_stretch(
                    right_edge, params.bounds.y, 1.0, params.bounds.height,
                    border_color, 0, false,
                );
            }

            if frame_params.bottom_divider_width > 0 && !is_bottommost {
                // Draw bottom divider with first/last pixel faces
                let dw = frame_params.bottom_divider_width as f32;
                let x0 = params.bounds.x;
                let y0 = bottom_edge - dw;
                let w = params.bounds.width
                    - if frame_params.right_divider_width > 0 && !is_rightmost {
                        frame_params.right_divider_width as f32
                    } else {
                        0.0
                    };
                let first_fg = Color::from_pixel(frame_params.divider_first_fg);
                let mid_fg = Color::from_pixel(frame_params.divider_fg);
                let last_fg = Color::from_pixel(frame_params.divider_last_fg);
                if dw >= 3.0 {
                    frame_glyphs.add_stretch(x0, y0, w, 1.0, first_fg, 0, false);
                    frame_glyphs.add_stretch(x0, y0 + 1.0, w, dw - 2.0, mid_fg, 0, false);
                    frame_glyphs.add_stretch(x0, y0 + dw - 1.0, w, 1.0, last_fg, 0, false);
                } else if dw >= 2.0 {
                    frame_glyphs.add_stretch(x0, y0, w, 1.0, first_fg, 0, false);
                    frame_glyphs.add_stretch(x0, y0 + 1.0, w, 1.0, last_fg, 0, false);
                } else {
                    frame_glyphs.add_stretch(x0, y0, w, 1.0, mid_fg, 0, false);
                }
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
        frame: EmacsFrame,
        frame_glyphs: &mut FrameGlyphBuffer,
    ) {
        let buffer = wp.buffer_ptr;
        let window = wp.window_ptr;
        if buffer.is_null() || window.is_null() {
            log::debug!("  layout_window: EARLY RETURN — null buffer={:?} or window={:?}", buffer, window);
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
            log::debug!("  layout_window id={}: skip — cols={} max_rows={}", params.window_id, cols, max_rows);
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

        log::debug!("  layout_window id={}: text_y={:.1} text_h={:.1} char_h={:.1} max_rows={} bytes_read={} bufsz={} is_mini={}",
            params.window_id, text_y, text_height, char_h, max_rows,
            bytes_read, params.buffer_size, params.is_minibuffer);

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

        // Overlay string buffers (4096 to handle fido-vertical-mode completions)
        let mut overlay_before_buf = [0u8; 4096];
        let mut overlay_after_buf = [0u8; 4096];
        let mut overlay_before_len: i32 = 0;
        let mut overlay_after_len: i32 = 0;
        let mut overlay_after_face = FaceDataFFI::default();
        let mut overlay_before_nruns: i32 = 0;
        let mut overlay_after_nruns: i32 = 0;

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

        // Available pixel width for text content (excluding line numbers)
        let avail_width = text_width - lnum_pixel_width;

        // Walk through text, placing characters on the grid
        let mut col = 0i32;        // column counter (for tab stops, cursor feedback)
        let mut x_offset: f32 = 0.0;  // pixel offset from content_x
        let mut row = 0i32;
        let mut charpos = params.window_start;
        let mut cursor_placed = false;
        let mut cursor_col = 0i32;
        let mut cursor_x: f32 = 0.0;  // pixel X of cursor
        let mut cursor_row = 0i32;
        let mut window_end_charpos = params.window_start;
        let mut byte_idx = 0usize;
        // hscroll state: how many columns to skip on each line
        let mut hscroll_remaining = hscroll;
        // Track current face's monospace status, space width, and line metrics
        let mut face_is_mono = true;
        let mut face_space_w = char_w;
        let mut face_h: f32 = char_h;    // current face's line height
        let mut face_ascent: f32 = ascent; // current face's font ascent

        // Fringe indicator tracking:
        // row_continued[row] = true if row wraps to next line (show \ in right fringe)
        // row_continuation[row] = true if row is a continuation from prev (show \ in left fringe)
        let mut row_continued = vec![false; max_rows as usize];
        let mut row_continuation = vec![false; max_rows as usize];
        let mut row_truncated = vec![false; max_rows as usize];
        // Per-row user fringe bitmaps from display properties
        // (bitmap_id, fg_color, bg_color): 0=none
        let mut row_left_fringe: Vec<(i32, u32, u32)> = vec![(0, 0, 0); max_rows as usize];
        let mut row_right_fringe: Vec<(i32, u32, u32)> = vec![(0, 0, 0); max_rows as usize];

        // Per-row Y positions — supports variable row heights from
        // line-height / line-spacing text properties.
        let row_capacity = (max_rows + 2) as usize;
        let mut row_y: Vec<f32> = (0..row_capacity)
            .map(|r| text_y + r as f32 * char_h)
            .collect();
        let mut row_extra_y: f32 = 0.0; // cumulative extra height from previous rows
        let mut row_max_height: f32 = char_h; // max glyph height on current row
        let mut row_max_ascent: f32 = ascent; // max ascent on current row

        // Trailing whitespace tracking
        let trailing_ws_bg = if params.show_trailing_whitespace {
            Some(Color::from_pixel(params.trailing_ws_bg))
        } else {
            None
        };
        let mut trailing_ws_start_col: i32 = -1; // -1 = no trailing ws
        let mut trailing_ws_start_x: f32 = 0.0; // pixel position of trailing ws start
        let mut trailing_ws_row: i32 = 0;

        // Word-wrap tracking: position after last breakable whitespace
        let mut wrap_break_col = 0i32;
        let mut wrap_break_x: f32 = 0.0;  // pixel position of wrap break
        let mut wrap_break_byte_idx = 0usize;
        let mut wrap_break_charpos = params.window_start;
        let mut wrap_break_glyph_count = 0usize;
        let mut wrap_has_break = false;

        // Line/wrap prefix tracking: 0=none, 1=line_prefix, 2=wrap_prefix
        let mut need_prefix: u8 = if !params.line_prefix.is_empty() { 1 } else { 0 };

        // Raise display property: Y offset applied to glyphs
        let mut raise_y_offset: f32 = 0.0;
        let mut raise_end: i64 = 0;
        // Height display property: font scale factor
        let mut height_scale: f32 = 0.0; // 0.0 = no scaling
        let mut height_end: i64 = 0;

        // Margin rendering: check at start of each visual line
        let has_margins = params.left_margin_width > 0.0 || params.right_margin_width > 0.0;
        let mut need_margin_check = has_margins;

        // Box face tracking: borders around text regions with box_type > 0
        let mut box_active = false;
        let mut box_start_x: f32 = 0.0;
        let mut box_row: i32 = 0;
        let mut box_color = Color::from_pixel(0);
        let mut box_line_width: i32 = 1;

        // Pixel Y limit: stop rendering when rows exceed the text area,
        // which can happen with variable-height faces pushing rows down.
        let text_y_limit = text_y + text_height;

        while byte_idx < bytes_read as usize && row < max_rows
            && row_y[row as usize] < text_y_limit
        {
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

                let gy = row_y[row as usize];

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
                let prefix_type = if need_prefix == 2 { 1 } else { 0 };
                let mut tp_width: f32 = -1.0;

                // Check text property prefix first (overrides window default)
                neomacs_layout_check_line_prefix(
                    buffer, window, charpos, prefix_type, &mut tp_width,
                );

                if tp_width >= 0.0 {
                    // Text property prefix: render as space
                    let px_w = tp_width * char_w;
                    if px_w > 0.0 && x_offset + px_w <= avail_width {
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];
                        frame_glyphs.add_stretch(
                            gx, gy, px_w, char_h, default_bg, 0, false,
                        );
                        let prefix_cols = tp_width.ceil() as i32;
                        col += prefix_cols;
                        x_offset += px_w;
                    }
                } else {
                    // Fall back to window-level prefix string
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
                            let adv = pchar_cols as f32 * char_w;
                            if x_offset + adv > avail_width { break; }

                            let gx = content_x + x_offset;
                            let gy = row_y[row as usize];
                            frame_glyphs.add_char(
                                pch, gx, gy, adv,
                                char_h, ascent, false,
                            );
                            col += pchar_cols;
                            x_offset += adv;
                        }
                    }
                }
                need_prefix = 0;
            }

            // Render margin content at the start of each visual line
            if need_margin_check && (params.left_margin_width > 0.0 || params.right_margin_width > 0.0) {
                need_margin_check = false;
                let mut left_margin_buf = [0u8; 256];
                let mut right_margin_buf = [0u8; 256];
                let mut left_len: c_int = 0;
                let mut right_len: c_int = 0;
                neomacs_layout_margin_strings_at(
                    buffer, window, charpos,
                    left_margin_buf.as_mut_ptr(), 256, &mut left_len,
                    right_margin_buf.as_mut_ptr(), 256, &mut right_len,
                );

                // Render left margin content
                if left_len > 0 && params.left_margin_width > 0.0 {
                    let margin_x = text_x - params.left_margin_width;
                    let gy = row_y[row as usize];
                    let margin_cols = (params.left_margin_width / char_w).floor() as i32;
                    let s = std::str::from_utf8_unchecked(
                        &left_margin_buf[..left_len as usize],
                    );
                    let mut mcol = 0i32;
                    for mch in s.chars() {
                        if mcol >= margin_cols { break; }
                        let gx = margin_x + mcol as f32 * char_w;
                        frame_glyphs.add_char(
                            mch, gx, gy, char_w, char_h, ascent, false,
                        );
                        mcol += 1;
                    }
                }

                // Render right margin content
                if right_len > 0 && params.right_margin_width > 0.0 {
                    let margin_x = text_x + text_width;
                    let gy = row_y[row as usize];
                    let margin_cols = (params.right_margin_width / char_w).floor() as i32;
                    let s = std::str::from_utf8_unchecked(
                        &right_margin_buf[..right_len as usize],
                    );
                    let mut mcol = 0i32;
                    for mch in s.chars() {
                        if mcol >= margin_cols { break; }
                        let gx = margin_x + mcol as f32 * char_w;
                        frame_glyphs.add_char(
                            mch, gx, gy, char_w, char_h, ascent, false,
                        );
                        mcol += 1;
                    }
                }
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
                    x_offset = 0.0;
                    row += 1;
                    current_line += 1;
                    need_line_number = lnum_enabled;
                    need_margin_check = has_margins;
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
                        let gy = row_y[row as usize];
                        frame_glyphs.add_char('$', content_x, gy, char_w, char_h, ascent, false);
                        col = 1; // $ takes 1 column
                        x_offset = char_w;
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
                    if invis == 2 && x_offset + 3.0 * char_w <= avail_width && row < max_rows {
                        let gy = row_y[row as usize];
                        for _ in 0..3 {
                            let dx = content_x + x_offset;
                            frame_glyphs.add_char(
                                '.', dx, gy, char_w, char_h, ascent, false,
                            );
                            col += 1;
                            x_offset += char_w;
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
                let mut overlay_before_face = FaceDataFFI::default();
                overlay_after_face = FaceDataFFI::default();
                overlay_before_nruns = 0;
                overlay_after_nruns = 0;
                neomacs_layout_overlay_strings_at(
                    buffer, window, charpos,
                    overlay_before_buf.as_mut_ptr(),
                    overlay_before_buf.len() as i32,
                    &mut overlay_before_len,
                    overlay_after_buf.as_mut_ptr(),
                    overlay_after_buf.len() as i32,
                    &mut overlay_after_len,
                    &mut overlay_before_face,
                    &mut overlay_after_face,
                    &mut overlay_before_nruns,
                    &mut overlay_after_nruns,
                );

                // Render before-string (if any) — insert before buffer text
                if overlay_before_len > 0 {
                    let before_has_runs = overlay_before_nruns > 0;
                    let before_face_runs = if before_has_runs {
                        parse_overlay_face_runs(&overlay_before_buf, overlay_before_len as usize, overlay_before_nruns)
                    } else {
                        Vec::new()
                    };

                    // Use per-char face runs, overlay face, or resolve face for position
                    if !before_has_runs {
                        if overlay_before_face.face_id != 0 {
                            self.apply_face(&overlay_before_face, frame_glyphs);
                        } else if charpos >= next_face_check || current_face_id < 0 {
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
                    }

                    let bstr = &overlay_before_buf[..overlay_before_len as usize];
                    let mut bi = 0usize;
                    let mut bcurrent_run = 0usize;
                    while bi < bstr.len() && row < max_rows {
                        // Apply face run if needed
                        if before_has_runs && bcurrent_run < before_face_runs.len() {
                            bcurrent_run = apply_overlay_face_run(
                                &before_face_runs, bi, bcurrent_run, frame_glyphs,
                            );
                        }

                        let (bch, blen) = decode_utf8(&bstr[bi..]);
                        bi += blen;
                        if bch == '\n' {
                            col = 0;
                            x_offset = 0.0;
                            row += 1;
                            if row >= max_rows { break; }
                            continue;
                        }
                        if bch == '\r' { continue; }

                        let bchar_cols = if is_wide_char(bch) { 2 } else { 1 };
                        let badv = bchar_cols as f32 * char_w;
                        if x_offset + badv > avail_width {
                            if params.truncate_lines { break; }
                            col = 0;
                            x_offset = 0.0;
                            row += 1;
                            if row >= max_rows { break; }
                        }
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];
                        frame_glyphs.add_char(bch, gx, gy, badv, char_h, ascent, false);
                        col += bchar_cols;
                        x_offset += badv;
                    }

                    // Restore text face after overlay face was used
                    if (before_has_runs || overlay_before_face.face_id != 0) && current_face_id >= 0 {
                        self.apply_face(&self.face_data, frame_glyphs);
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

                    // Parse face runs for display string (if present)
                    struct DFaceRun { byte_offset: u16, fg: u32, bg: u32 }
                    let mut dface_runs: Vec<DFaceRun> = Vec::new();
                    let has_face_runs = display_prop.display_nruns > 0;
                    if has_face_runs {
                        let runs_start = display_prop.str_len as usize;
                        for ri in 0..display_prop.display_nruns as usize {
                            let off = runs_start + ri * 10;
                            if off + 10 <= display_str_buf.len() {
                                let byte_offset = u16::from_ne_bytes([
                                    display_str_buf[off], display_str_buf[off + 1],
                                ]);
                                let fg = u32::from_ne_bytes([
                                    display_str_buf[off + 2], display_str_buf[off + 3],
                                    display_str_buf[off + 4], display_str_buf[off + 5],
                                ]);
                                let bg = u32::from_ne_bytes([
                                    display_str_buf[off + 6], display_str_buf[off + 7],
                                    display_str_buf[off + 8], display_str_buf[off + 9],
                                ]);
                                dface_runs.push(DFaceRun { byte_offset, fg, bg });
                            }
                        }
                    } else {
                        // Single-face fallback (backward compat)
                        let has_display_face = display_prop.display_fg != 0
                            || display_prop.display_bg != 0;
                        if has_display_face {
                            let dfg = Color::from_pixel(display_prop.display_fg);
                            let dbg = Color::from_pixel(display_prop.display_bg);
                            frame_glyphs.set_face(
                                0, dfg, Some(dbg),
                                false, false, 0, None, 0, None, 0, None,
                            );
                        }
                    }

                    // Render display string characters with face runs
                    let dstr = &display_str_buf[..display_prop.str_len as usize];
                    let mut di = 0usize;
                    let mut dcurrent_run = 0usize;
                    while di < dstr.len() && row < max_rows {
                        // Apply face run if needed
                        if has_face_runs && dcurrent_run < dface_runs.len() {
                            while dcurrent_run + 1 < dface_runs.len()
                                && di >= dface_runs[dcurrent_run + 1].byte_offset as usize
                            {
                                dcurrent_run += 1;
                            }
                            if di >= dface_runs[dcurrent_run].byte_offset as usize {
                                let run = &dface_runs[dcurrent_run];
                                if run.fg != 0 || run.bg != 0 {
                                    let rfg = Color::from_pixel(run.fg);
                                    let rbg = Color::from_pixel(run.bg);
                                    frame_glyphs.set_face(
                                        0, rfg, Some(rbg),
                                        false, false, 0, None, 0, None, 0, None,
                                    );
                                }
                                if dcurrent_run + 1 < dface_runs.len()
                                    && di + 1 >= dface_runs[dcurrent_run + 1].byte_offset as usize
                                {
                                    dcurrent_run += 1;
                                }
                            }
                        }

                        let (dch, dlen) = decode_utf8(&dstr[di..]);
                        di += dlen;

                        if dch == '\n' || dch == '\r' {
                            continue;
                        }

                        let dchar_cols = if is_wide_char(dch) { 2 } else { 1 };
                        let d_advance = dchar_cols as f32 * char_w;
                        if x_offset + d_advance > avail_width {
                            if params.truncate_lines {
                                break;
                            }
                            col = 0;
                            x_offset = 0.0;
                            row += 1;
                            if row >= max_rows { break; }
                        }

                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];
                        frame_glyphs.add_char(dch, gx, gy, d_advance, char_h, ascent, false);
                        col += dchar_cols;
                        x_offset += d_advance;
                    }

                    // Restore text face after display string
                    if has_face_runs || display_prop.display_fg != 0 || display_prop.display_bg != 0 {
                        if current_face_id >= 0 {
                            self.apply_face(&self.face_data, frame_glyphs);
                        }
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
                    let space_h = if display_prop.space_height > 0.0 {
                        display_prop.space_height
                    } else {
                        char_h
                    };

                    if x_offset + space_pixel_w <= avail_width && row < max_rows {
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];
                        frame_glyphs.add_stretch(
                            gx, gy, space_pixel_w, space_h,
                            face_bg, self.face_data.face_id, false,
                        );
                        col += space_cols;
                        x_offset += space_pixel_w;
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
                    let target_x = target_col as f32 * char_w;
                    if target_x > x_offset && row < max_rows {
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];
                        let stretch_w = target_x - x_offset;
                        frame_glyphs.add_stretch(
                            gx, gy, stretch_w, char_h,
                            face_bg, self.face_data.face_id, false,
                        );
                        col = target_col;
                        x_offset = target_x;
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
                    let hmargin = display_prop.image_hmargin as f32;
                    let vmargin = display_prop.image_vmargin as f32;
                    // Total dimensions including margins
                    let total_w = img_w + hmargin * 2.0;
                    let total_h = img_h + vmargin * 2.0;

                    if row < max_rows && display_prop.image_gpu_id != 0 {
                        let gx = content_x + x_offset + hmargin;
                        let gy_base = row_y[row as usize];

                        // For images that fit within one text line, use ascent-based
                        // alignment with the text baseline. For taller images, place
                        // at the row top and extend downward.
                        let gy = if total_h <= char_h {
                            let img_ascent = display_prop.image_ascent;
                            let ascent_px = if img_ascent == -1 {
                                // Centered: align middle of image with font baseline center
                                (total_h + ascent - (char_h - ascent) + 1.0) / 2.0
                            } else {
                                // Percentage: ascent% of total height
                                total_h * (img_ascent as f32 / 100.0)
                            };
                            gy_base + ascent - ascent_px + vmargin
                        } else {
                            // Large image: start at current row top
                            gy_base + vmargin
                        };

                        frame_glyphs.add_image(
                            display_prop.image_gpu_id,
                            gx, gy, img_w, img_h,
                        );
                        // Advance by total width (including margins)
                        let img_cols = (total_w / char_w).ceil() as i32;
                        col += img_cols;
                        x_offset += total_w;
                        // Advance rows if image is taller than one line
                        let img_rows = ((total_h / char_h).ceil() as i32).max(1);
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
                } else if display_prop.prop_type == 9 {
                    // Video display property: render video glyph
                    let vid_w = display_prop.image_width as f32;
                    let vid_h = display_prop.image_height as f32;

                    if row < max_rows && display_prop.video_id != 0 {
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];

                        frame_glyphs.add_video(
                            display_prop.video_id,
                            gx, gy, vid_w, vid_h,
                        );
                        let vid_cols = (vid_w / char_w).ceil() as i32;
                        col += vid_cols;
                        x_offset += vid_w;
                        let vid_rows = ((vid_h / char_h).ceil() as i32).max(1);
                        if vid_rows > 1 {
                            row += vid_rows - 1;
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
                } else if display_prop.prop_type == 10 {
                    // WebKit display property: render webkit glyph
                    let wk_w = display_prop.image_width as f32;
                    let wk_h = display_prop.image_height as f32;

                    if row < max_rows && display_prop.webkit_id != 0 {
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];

                        frame_glyphs.add_webkit(
                            display_prop.webkit_id,
                            gx, gy, wk_w, wk_h,
                        );
                        let wk_cols = (wk_w / char_w).ceil() as i32;
                        col += wk_cols;
                        x_offset += wk_w;
                        let wk_rows = ((wk_h / char_h).ceil() as i32).max(1);
                        if wk_rows > 1 {
                            row += wk_rows - 1;
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
                } else if display_prop.prop_type == 5 || display_prop.prop_type == 8 {
                    // Raise and/or height: modify rendering of subsequent glyphs
                    if display_prop.raise_factor != 0.0 {
                        raise_y_offset = -(display_prop.raise_factor * char_h);
                        raise_end = display_prop.covers_to;
                    }
                    if display_prop.height_factor > 0.0 {
                        height_scale = display_prop.height_factor;
                        height_end = display_prop.covers_to;
                    }
                    next_display_check = display_prop.covers_to;
                    // Don't skip text - these modify rendering, not content
                } else if display_prop.prop_type == 6 || display_prop.prop_type == 7 {
                    // Left-fringe (6) or right-fringe (7) display property
                    let r = row as usize;
                    if display_prop.prop_type == 6 {
                        if r < row_left_fringe.len() {
                            row_left_fringe[r] = (
                                display_prop.fringe_bitmap_id,
                                display_prop.fringe_fg,
                                display_prop.fringe_bg,
                            );
                        }
                    } else {
                        if r < row_right_fringe.len() {
                            row_right_fringe[r] = (
                                display_prop.fringe_bitmap_id,
                                display_prop.fringe_fg,
                                display_prop.fringe_bg,
                            );
                        }
                    }
                    // Skip the covered text
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
                } else {
                    // No display prop: covers_to tells us when to re-check
                    next_display_check = display_prop.covers_to;
                }

                // Reset raise offset when past the raise region
                if raise_end > 0 && charpos >= raise_end {
                    raise_y_offset = 0.0;
                    raise_end = 0;
                }
                // Reset height scale when past the height region
                if height_end > 0 && charpos >= height_end {
                    height_scale = 0.0;
                    height_end = 0;
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
                        // Close previous box face region if active
                        if box_active {
                            let box_end_x = content_x + x_offset;
                            let gy = row_y[box_row as usize];
                            let bw = box_line_width.max(1) as f32;
                            // Top border
                            frame_glyphs.add_border(box_start_x, gy, box_end_x - box_start_x, bw, box_color);
                            // Bottom border
                            frame_glyphs.add_border(box_start_x, gy + char_h - bw, box_end_x - box_start_x, bw, box_color);
                            // Left border
                            frame_glyphs.add_border(box_start_x, gy, bw, char_h, box_color);
                            // Right border
                            frame_glyphs.add_border(box_end_x - bw, gy, bw, char_h, box_color);
                            box_active = false;
                        }

                        current_face_id = fid;
                        face_fg = Color::from_pixel(self.face_data.fg);
                        face_bg = Color::from_pixel(self.face_data.bg);
                        face_is_mono = self.face_data.font_is_monospace != 0;
                        face_space_w = if self.face_data.font_space_width > 0.0 {
                            self.face_data.font_space_width
                        } else {
                            char_w
                        };
                        // Compute per-face line height and ascent.
                        // Scale proportionally from the window's default font metrics.
                        if self.face_data.font_ascent > 0.0 && self.face_data.font_size > 0 {
                            face_ascent = self.face_data.font_ascent;
                            // Line height = ascent + descent, scaled similarly
                            // Use ratio of face font_size to window font_pixel_size
                            let scale = self.face_data.font_size as f32 / params.font_pixel_size;
                            face_h = char_h * scale;
                        } else {
                            face_h = char_h;
                            face_ascent = ascent;
                        }
                        self.apply_face(&self.face_data, frame_glyphs);

                        // Debug: check all face properties
                        if charpos < params.window_start + 5 {
                            log::debug!("face: id={} fg=0x{:06X} bg=0x{:06X} underline_style={} underline_color=0x{:06X} strike_through={} strike_color=0x{:06X} overline={} overline_color=0x{:06X} box_type={} box_color=0x{:06X} box_lw={}",
                                self.face_data.face_id, self.face_data.fg, self.face_data.bg,
                                self.face_data.underline_style, self.face_data.underline_color,
                                self.face_data.strike_through, self.face_data.strike_through_color,
                                self.face_data.overline, self.face_data.overline_color,
                                self.face_data.box_type, self.face_data.box_color, self.face_data.box_line_width);
                        }

                        // Start new box face region if this face has a box
                        if self.face_data.box_type > 0 {
                            box_active = true;
                            box_start_x = content_x + x_offset;
                            box_row = row;
                            box_color = Color::from_pixel(self.face_data.box_color);
                            box_line_width = self.face_data.box_line_width;
                        }
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
                cursor_x = x_offset;
                cursor_row = row;
                let cursor_px = content_x + x_offset;
                let cursor_y = row_y[row as usize];

                // Use face-specific dimensions so cursor matches variable-height faces
                let cursor_face_w = if self.face_data.font_char_width > 0.0 {
                    self.face_data.font_char_width
                } else {
                    char_w
                };
                let (cursor_w, cursor_h) = match params.cursor_type {
                    1 => (params.cursor_bar_width.max(1) as f32, face_h), // bar
                    2 => (cursor_face_w, 2.0),                             // hbar
                    _ => (cursor_face_w, face_h),                          // box/hollow
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
                        cursor_px,
                        cursor_y,
                        cursor_w,
                        cursor_h,
                        cursor_style,
                        face_fg,
                    );

                    // Set inverse for filled box cursor
                    if cursor_style == 0 {
                        frame_glyphs.set_cursor_inverse(
                            cursor_px,
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
                            let tw_x = content_x + trailing_ws_start_x;
                            let tw_w = x_offset - trailing_ws_start_x;
                            let gy = row_y[row as usize];
                            if tw_w > 0.0 {
                                frame_glyphs.add_stretch(tw_x, gy, tw_w, char_h, tw_bg, 0, false);
                            }
                        }
                    }
                    trailing_ws_start_col = -1;

                    // Fill rest of line with stretch.
                    // Use face bg if :extend is set, default bg otherwise.
                    let remaining = avail_width - x_offset;
                    if remaining > 0.0 {
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];
                        let fill_bg = if self.face_data.extend != 0 { face_bg } else { default_bg };
                        let fill_face = if self.face_data.extend != 0 { self.face_data.face_id } else { 0 };
                        frame_glyphs.add_stretch(gx, gy, remaining, char_h, fill_bg, fill_face, false);
                    }

                    // Close box face at end of line (box continues on next line)
                    if box_active {
                        let box_end_x = content_x + x_offset;
                        let gy = row_y[box_row as usize];
                        let bw = box_line_width.max(1) as f32;
                        frame_glyphs.add_border(box_start_x, gy, box_end_x - box_start_x, bw, box_color);
                        frame_glyphs.add_border(box_start_x, gy + char_h - bw, box_end_x - box_start_x, bw, box_color);
                        frame_glyphs.add_border(box_start_x, gy, bw, char_h, box_color);
                        frame_glyphs.add_border(box_end_x - bw, gy, bw, char_h, box_color);
                        // Box stays active but restarts on new row
                        box_start_x = content_x;
                        // box_row will update after row += 1
                    }

                    col = 0;
                    x_offset = 0.0;
                    row += 1;

                    // Apply extra height from variable-height faces on this row
                    if row_max_height > char_h {
                        let extra = row_max_height - char_h;
                        row_extra_y += extra;
                        for ri in (row as usize)..row_y.len() {
                            row_y[ri] = text_y + ri as f32 * char_h + row_extra_y;
                        }
                    }
                    // Reset per-row tracking for the new row
                    row_max_height = char_h;
                    row_max_ascent = ascent;

                    // Check line-height / line-spacing text properties on the newline
                    {
                        let mut extra_h: f32 = 0.0;
                        let nl_pos = charpos - 1; // the newline we just consumed
                        neomacs_layout_check_line_spacing(
                            buffer, window, nl_pos, char_h, &mut extra_h,
                        );
                        if extra_h > 0.0 {
                            row_extra_y += extra_h;
                            // Update all remaining row_y entries
                            for ri in (row as usize)..row_y.len() {
                                row_y[ri] = text_y + ri as f32 * char_h + row_extra_y;
                            }
                        }
                    }

                    if box_active { box_row = row; }
                    current_line += 1;
                    need_line_number = lnum_enabled;
                    need_margin_check = has_margins;
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
                                    let gy = row_y[(row - 1) as usize];
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
                    // Tab: advance to next tab stop (column-based, pixel width uses space_w)
                    let tab_w = params.tab_width.max(1);
                    let next_tab = ((col / tab_w) + 1) * tab_w;
                    let spaces = (next_tab - col).min(cols - col);
                    let tab_pixel_w = spaces as f32 * face_space_w;

                    // Render tab as stretch glyph (use face bg)
                    let gx = content_x + x_offset;
                    let gy = row_y[row as usize];
                    frame_glyphs.add_stretch(gx, gy, tab_pixel_w, char_h, face_bg, self.face_data.face_id, false);

                    col += spaces;
                    x_offset += tab_pixel_w;
                    // Tab is a breakpoint for word-wrap
                    if params.word_wrap {
                        wrap_break_col = col;
                        wrap_break_x = x_offset;
                        wrap_break_byte_idx = byte_idx;
                        wrap_break_charpos = charpos;
                        wrap_break_glyph_count = frame_glyphs.glyphs.len();
                        wrap_has_break = true;
                    }
                    if x_offset >= avail_width {
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
                                    x_offset = 0.0;
                                    row += 1;
                                    current_line += 1;
                                    need_line_number = lnum_enabled;
                                    need_margin_check = has_margins;
                                    wrap_has_break = false;
                                    break;
                                }
                            }
                        } else {
                            if (row as usize) < row_continued.len() {
                                row_continued[row as usize] = true;
                            }
                            col = 0;
                            x_offset = 0.0;
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
                        let gy = row_y[row as usize];
                        if x_offset + 3.0 * char_w <= avail_width {
                            for dot_i in 0..3 {
                                frame_glyphs.add_char(
                                    '.', content_x + x_offset + dot_i as f32 * char_w,
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
                                x_offset = 0.0;
                                row += 1;
                                current_line += 1;
                                need_line_number = lnum_enabled;
                                need_margin_check = has_margins;
                                wrap_has_break = false;
                                hscroll_remaining = hscroll;
                                break;
                            }
                        }
                    }
                    // Otherwise: carriage return is just skipped
                }
                _ if ch < ' ' || ch == '\x7F' => {
                    // Control character: display as ^X (2 columns)
                    // DEL (0x7F) displays as ^?
                    // Use escape-glyph face for control char display
                    let escape_fg = Color::from_pixel(params.escape_glyph_fg);
                    frame_glyphs.set_face(
                        0, escape_fg, Some(face_bg),
                        false, false, 0, None, 0, None, 0, None,
                    );

                    let gx = content_x + x_offset;
                    let gy = row_y[row as usize];

                    let ctrl_ch = if ch == '\x7F' { '?' } else { char::from((ch as u8) + b'@') };
                    if x_offset + 2.0 * char_w <= avail_width {
                        frame_glyphs.add_char('^', gx, gy, char_w, char_h, ascent, false);
                        frame_glyphs.add_char(
                            ctrl_ch,
                            gx + char_w,
                            gy,
                            char_w,
                            char_h,
                            ascent,
                            false,
                        );
                        col += 2;
                        x_offset += 2.0 * char_w;
                    } else {
                        if params.truncate_lines {
                            while byte_idx < bytes_read as usize {
                                let (c, l) = decode_utf8(&text[byte_idx..]);
                                byte_idx += l;
                                charpos += 1;
                                if c == '\n' {
                                    col = 0;
                                    x_offset = 0.0;
                                    row += 1;
                                    current_line += 1;
                                    need_line_number = lnum_enabled;
                                    need_margin_check = has_margins;
                                    wrap_has_break = false;
                                    break;
                                }
                            }
                        } else {
                            col = 0;
                            x_offset = 0.0;
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
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];
                        let display_ch = if ch == '\u{00A0}' { ' ' } else { '-' };
                        if x_offset + char_w <= avail_width {
                            frame_glyphs.add_char(display_ch, gx, gy, char_w, char_h, ascent, false);
                            col += 1;
                            x_offset += char_w;
                        }
                        // Restore text face
                        if current_face_id >= 0 {
                            self.apply_face(&self.face_data, frame_glyphs);
                        }
                        window_end_charpos = charpos;
                        continue;
                    }

                    // Grapheme cluster detection: collect combining marks,
                    // ZWJ sequences, variation selectors, skin tone modifiers,
                    // and regional indicator pairs with the base character.
                    let (cluster_text, cluster_extra_bytes, cluster_extra_chars) =
                        collect_grapheme_cluster(ch, &text[byte_idx..bytes_read as usize]);

                    if let Some(ref cluster) = cluster_text {
                        // Multi-codepoint grapheme cluster (emoji ZWJ, combining marks, etc.)
                        // Advance past the extra characters consumed
                        byte_idx += cluster_extra_bytes;
                        charpos += cluster_extra_chars as i64;

                        // Determine width: composed emoji are 2 columns wide
                        let char_cols = if is_wide_char(ch) { 2 } else { 1 };
                        let glyph_w = char_cols as f32 * char_w;

                        if x_offset + glyph_w > avail_width {
                            if params.truncate_lines {
                                // Skip to end of line
                                while byte_idx < bytes_read as usize {
                                    let (c, l) = decode_utf8(&text[byte_idx..]);
                                    byte_idx += l;
                                    charpos += 1;
                                    if c == '\n' {
                                        col = 0;
                                        x_offset = 0.0;
                                        row += 1;
                                        current_line += 1;
                                        need_line_number = lnum_enabled;
                                        need_margin_check = has_margins;
                                        wrap_has_break = false;
                                        hscroll_remaining = hscroll;
                                        break;
                                    }
                                }
                                window_end_charpos = charpos;
                                continue;
                            } else {
                                // Wrap to next line
                                col = 0;
                                x_offset = 0.0;
                                row += 1;
                                wrap_has_break = false;
                                if row >= max_rows { break; }
                            }
                        }

                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize] + raise_y_offset;

                        if height_scale > 0.0 && height_scale != 1.0 {
                            let orig_size = frame_glyphs.font_size();
                            frame_glyphs.set_font_size(orig_size * height_scale);
                            frame_glyphs.add_composed_char(cluster, ch, gx, gy, glyph_w, char_h, ascent, false);
                            frame_glyphs.set_font_size(orig_size);
                        } else {
                            frame_glyphs.add_composed_char(cluster, ch, gx, gy, glyph_w, char_h, ascent, false);
                        }
                        col += char_cols;
                        x_offset += glyph_w;
                        window_end_charpos = charpos;
                        continue;
                    }

                    // Standalone combining mark without a base: render as zero-width
                    // at the previous position (fallback for bare combining marks)
                    if is_cluster_extender(ch) && ch != '\u{200D}' && ch != '\u{200C}'
                        && ch != '\u{200B}' && ch != '\u{200E}' && ch != '\u{200F}'
                        && ch != '\u{FEFF}' {
                        if x_offset > 0.0 {
                            // Place combining mark at the position of the previous character
                            let gx = content_x + x_offset - char_w;
                            let gy = row_y[row as usize];
                            frame_glyphs.add_char(ch, gx, gy, 0.0, char_h, ascent, false);
                        }
                        window_end_charpos = charpos;
                        continue;
                    }

                    // Glyphless character check for C1 control and other
                    // non-printable chars
                    if is_potentially_glyphless(ch) {
                        let mut method: c_int = 0;
                        let mut str_buf = [0u8; 64];
                        let mut str_len: c_int = 0;
                        neomacs_layout_check_glyphless(
                            frame,
                            ch as c_int,
                            &mut method,
                            str_buf.as_mut_ptr(),
                            64,
                            &mut str_len,
                        );
                        if method != 0 {
                            let glyph_fg = Color::from_pixel(params.glyphless_char_fg);
                            frame_glyphs.set_face(
                                0, glyph_fg, Some(face_bg),
                                false, false, 0, None, 0, None, 0, None,
                            );
                            let gx = content_x + x_offset;
                            let gy = row_y[row as usize];
                            match method {
                                1 => {
                                    // thin-space: 1-pixel-wide stretch
                                    frame_glyphs.add_stretch(
                                        gx, gy, 1.0, char_h,
                                        face_bg, 0, false,
                                    );
                                    x_offset += 1.0;
                                }
                                2 => {
                                    // empty-box: render as hollow box char
                                    if x_offset + char_w <= avail_width {
                                        frame_glyphs.add_char(
                                            '\u{25A1}', gx, gy,
                                            char_w, char_h, ascent, false,
                                        );
                                        col += 1;
                                        x_offset += char_w;
                                    }
                                }
                                3 => {
                                    // hex-code: render as [U+XXXX]
                                    let hex = if (ch as u32) < 0x10000 {
                                        format!("U+{:04X}", ch as u32)
                                    } else {
                                        format!("U+{:06X}", ch as u32)
                                    };
                                    let needed = hex.len() as i32;
                                    let needed_px = needed as f32 * char_w;
                                    if x_offset + needed_px <= avail_width {
                                        for (i, hch) in hex.chars().enumerate() {
                                            frame_glyphs.add_char(
                                                hch,
                                                gx + i as f32 * char_w,
                                                gy, char_w, char_h, ascent, false,
                                            );
                                        }
                                        col += needed;
                                        x_offset += needed_px;
                                    } else {
                                        x_offset = avail_width; // truncate
                                    }
                                }
                                4 => {
                                    // acronym: render the string
                                    if str_len > 0 {
                                        let s = std::str::from_utf8_unchecked(
                                            &str_buf[..str_len as usize],
                                        );
                                        let needed = s.len() as i32;
                                        let needed_px = needed as f32 * char_w;
                                        if x_offset + needed_px <= avail_width {
                                            for (i, ach) in s.chars().enumerate() {
                                                frame_glyphs.add_char(
                                                    ach,
                                                    gx + i as f32 * char_w,
                                                    gy, char_w, char_h, ascent, false,
                                                );
                                            }
                                            col += needed;
                                            x_offset += needed_px;
                                        } else {
                                            x_offset = avail_width;
                                        }
                                    }
                                }
                                5 => {
                                    // zero-width: skip entirely
                                }
                                _ => {}
                            }
                            // Restore face
                            if current_face_id >= 0 {
                                self.apply_face(&self.face_data, frame_glyphs);
                            }
                            window_end_charpos = charpos;
                            continue;
                        }
                    }

                    // Track word-wrap breakpoints: after space/tab
                    if params.word_wrap && (ch == ' ' || ch == '\t') {
                        // Record break AFTER this whitespace
                        // (will be updated after rendering below)
                    }

                    // Normal character — compute advance width
                    let char_cols = if is_wide_char(ch) { 2 } else { 1 };
                    let face_id = self.face_data.face_id;
                    let font_size = self.face_data.font_size;
                    let face_char_w = self.face_data.font_char_width;
                    let advance = char_advance(
                        &mut self.ascii_width_cache,
                        ch, char_cols, char_w, face_is_mono,
                        face_id, font_size, face_char_w, window,
                    );

                    if x_offset + advance > avail_width {
                        // Line full
                        if params.truncate_lines {
                            // Show $ truncation indicator at right edge
                            let trunc_x = content_x + avail_width - char_w;
                            let gy = row_y[row as usize];
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
                                    x_offset = 0.0;
                                    row += 1;
                                    // Apply variable-height row adjustment
                                    if row_max_height > char_h {
                                        row_extra_y += row_max_height - char_h;
                                        for ri in (row as usize)..row_y.len() {
                                            row_y[ri] = text_y + ri as f32 * char_h + row_extra_y;
                                        }
                                    }
                                    row_max_height = char_h;
                                    row_max_ascent = ascent;
                                    current_line += 1;
                                    need_line_number = lnum_enabled;
                                    need_margin_check = has_margins;
                                    wrap_has_break = false;
                                    hscroll_remaining = hscroll;
                                    break;
                                }
                            }
                            continue;
                        } else if params.word_wrap && wrap_has_break && wrap_break_x > 0.0 {
                            // Word-wrap: rewind to last breakpoint
                            frame_glyphs.glyphs.truncate(wrap_break_glyph_count);
                            // Fill from break to end of line with bg
                            let fill_w = avail_width - wrap_break_x;
                            if fill_w > 0.0 {
                                let gx = content_x + wrap_break_x;
                                let gy = row_y[row as usize];
                                frame_glyphs.add_stretch(
                                    gx, gy,
                                    fill_w, char_h,
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
                            x_offset = 0.0;
                            row += 1;
                            // Apply variable-height row adjustment
                            if row_max_height > char_h {
                                row_extra_y += row_max_height - char_h;
                                for ri in (row as usize)..row_y.len() {
                                    row_y[ri] = text_y + ri as f32 * char_h + row_extra_y;
                                }
                            }
                            row_max_height = char_h;
                            row_max_ascent = ascent;
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
                            let remaining = avail_width - x_offset;
                            if remaining > 0.0 {
                                let gx = content_x + x_offset;
                                let gy = row_y[row as usize];
                                frame_glyphs.add_stretch(gx, gy, remaining, char_h, face_bg, self.face_data.face_id, false);
                            }
                            if (row as usize) < row_continued.len() {
                                row_continued[row as usize] = true;
                            }
                            col = 0;
                            x_offset = 0.0;
                            row += 1;
                            // Apply variable-height row adjustment
                            if row_max_height > char_h {
                                row_extra_y += row_max_height - char_h;
                                for ri in (row as usize)..row_y.len() {
                                    row_y[ri] = text_y + ri as f32 * char_h + row_extra_y;
                                }
                            }
                            row_max_height = char_h;
                            row_max_ascent = ascent;
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

                    let gx = content_x + x_offset;
                    let gy = row_y[row as usize] + raise_y_offset;

                    // Track per-row max height for variable-height faces
                    if face_h > row_max_height {
                        row_max_height = face_h;
                    }
                    if face_ascent > row_max_ascent {
                        row_max_ascent = face_ascent;
                    }

                    // Apply height scaling if active
                    if height_scale > 0.0 && height_scale != 1.0 {
                        let orig_size = frame_glyphs.font_size();
                        frame_glyphs.set_font_size(orig_size * height_scale);
                        frame_glyphs.add_char(ch, gx, gy, advance, face_h, face_ascent, false);
                        frame_glyphs.set_font_size(orig_size);
                    } else {
                        frame_glyphs.add_char(ch, gx, gy, advance, face_h, face_ascent, false);
                    }
                    col += char_cols;
                    x_offset += advance;

                    // Track trailing whitespace
                    if trailing_ws_bg.is_some() {
                        if ch == ' ' || ch == '\t' {
                            if trailing_ws_start_col < 0 {
                                trailing_ws_start_col = col - char_cols;
                                trailing_ws_start_x = x_offset - advance;
                                trailing_ws_row = row;
                            }
                        } else {
                            trailing_ws_start_col = -1;
                        }
                    }

                    // Record break AFTER whitespace characters
                    if params.word_wrap && (ch == ' ' || ch == '\t') {
                        wrap_break_col = col;
                        wrap_break_x = x_offset;
                        wrap_break_byte_idx = byte_idx;
                        wrap_break_charpos = charpos;
                        wrap_break_glyph_count = frame_glyphs.glyphs.len();
                        wrap_has_break = true;
                    }
                }
            }

            // Place cursor before rendering overlay after-string.
            // When an overlay ends at point (e.g., fido-vertical-mode completions),
            // the after-string visually follows the cursor. Without this check,
            // the cursor would be placed after the entire after-string content.
            if !cursor_placed && charpos >= params.point && overlay_after_len > 0 {
                cursor_col = col;
                cursor_x = x_offset;
                cursor_row = row;
                let cursor_px = content_x + x_offset;
                let cursor_y = row_y[row as usize];

                // Use face-specific dimensions so cursor matches variable-height faces
                let cursor_face_w = if self.face_data.font_char_width > 0.0 {
                    self.face_data.font_char_width
                } else {
                    char_w
                };
                let (cursor_w, cursor_h) = match params.cursor_type {
                    1 => (params.cursor_bar_width.max(1) as f32, face_h), // bar
                    2 => (cursor_face_w, 2.0),                             // hbar
                    _ => (cursor_face_w, face_h),                          // box/hollow
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
                        cursor_px,
                        cursor_y,
                        cursor_w,
                        cursor_h,
                        cursor_style,
                        face_fg,
                    );

                    if cursor_style == 0 {
                        frame_glyphs.set_cursor_inverse(
                            cursor_px,
                            cursor_y,
                            cursor_w,
                            cursor_h,
                            face_fg,
                            face_bg,
                        );
                    }
                }

                cursor_placed = true;
            }

            // Render overlay after-string (if any) — collected earlier
            if overlay_after_len > 0 && row < max_rows {
                let after_has_runs = overlay_after_nruns > 0;
                let after_face_runs = if after_has_runs {
                    parse_overlay_face_runs(&overlay_after_buf, overlay_after_len as usize, overlay_after_nruns)
                } else {
                    Vec::new()
                };

                // Apply overlay face for after-string if no per-char runs
                if !after_has_runs && overlay_after_face.face_id != 0 {
                    self.apply_face(&overlay_after_face, frame_glyphs);
                }

                let astr = &overlay_after_buf[..overlay_after_len as usize];
                let mut ai = 0usize;
                let mut acurrent_run = 0usize;
                while ai < astr.len() && row < max_rows {
                    // Apply face run if needed
                    if after_has_runs && acurrent_run < after_face_runs.len() {
                        acurrent_run = apply_overlay_face_run(
                            &after_face_runs, ai, acurrent_run, frame_glyphs,
                        );
                    }

                    let (ach, alen) = decode_utf8(&astr[ai..]);
                    ai += alen;
                    if ach == '\n' {
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        if row >= max_rows { break; }
                        continue;
                    }
                    if ach == '\r' { continue; }

                    let achar_cols = if is_wide_char(ach) { 2 } else { 1 };
                    let a_advance = achar_cols as f32 * char_w;
                    if x_offset + a_advance > avail_width {
                        if params.truncate_lines { break; }
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        if row >= max_rows { break; }
                    }
                    let gx = content_x + x_offset;
                    let gy = row_y[row as usize];
                    frame_glyphs.add_char(ach, gx, gy, a_advance, char_h, ascent, false);
                    col += achar_cols;
                    x_offset += a_advance;
                }

                // Restore text face after overlay after-string
                if (after_has_runs || overlay_after_face.face_id != 0) && current_face_id >= 0 {
                    self.apply_face(&self.face_data, frame_glyphs);
                }
            }

            window_end_charpos = charpos;
        }

        // Place cursor before end-of-buffer overlay strings.
        // When point is at end-of-buffer and overlays have after-strings there
        // (e.g., fido-vertical-mode completions), the cursor must be placed
        // BEFORE the overlay content is rendered.
        if !cursor_placed && params.point >= params.window_start
            && charpos >= params.point
        {
            cursor_col = col;
            cursor_x = x_offset;
            cursor_row = row.min(max_rows - 1);
            let cursor_px = content_x + x_offset;
            let cursor_y = row_y[row.min(max_rows - 1) as usize];

            let cursor_style = if params.selected {
                params.cursor_type
            } else if params.cursor_in_non_selected {
                3
            } else {
                255 // skip
            };

            if cursor_style < 255 {
                // Use face-specific dimensions so cursor matches variable-height faces
                let cursor_face_w = if self.face_data.font_char_width > 0.0 {
                    self.face_data.font_char_width
                } else {
                    char_w
                };
                let (cursor_w, cursor_h) = match params.cursor_type {
                    1 => (params.cursor_bar_width.max(1) as f32, face_h), // bar
                    2 => (cursor_face_w, 2.0),                             // hbar
                    _ => (cursor_face_w, face_h),                          // box/hollow
                };

                frame_glyphs.add_cursor(
                    params.window_id as i32,
                    cursor_px,
                    cursor_y,
                    cursor_w,
                    cursor_h,
                    cursor_style,
                    face_fg,
                );

                if cursor_style == 0 {
                    frame_glyphs.set_cursor_inverse(
                        cursor_px,
                        cursor_y,
                        cursor_w,
                        cursor_h,
                        face_fg,
                        face_bg,
                    );
                }
            }

            cursor_placed = true;
        }

        // Check for overlay strings at end-of-buffer (e.g., fido-vertical-mode
        // completions placed as after-strings at point-max).
        if row < max_rows {
            overlay_after_len = 0;
            overlay_after_face = FaceDataFFI::default();
            overlay_before_nruns = 0;
            overlay_after_nruns = 0;
            let mut eob_before_len: i32 = 0;
            let mut eob_before_face = FaceDataFFI::default();
            neomacs_layout_overlay_strings_at(
                buffer, window, charpos,
                overlay_before_buf.as_mut_ptr(),
                overlay_before_buf.len() as i32,
                &mut eob_before_len,
                overlay_after_buf.as_mut_ptr(),
                overlay_after_buf.len() as i32,
                &mut overlay_after_len,
                &mut eob_before_face,
                &mut overlay_after_face,
                &mut overlay_before_nruns,
                &mut overlay_after_nruns,
            );

            // Render before-string at end-of-buffer
            if eob_before_len > 0 {
                let eob_before_has_runs = overlay_before_nruns > 0;
                let eob_before_face_runs = if eob_before_has_runs {
                    parse_overlay_face_runs(&overlay_before_buf, eob_before_len as usize, overlay_before_nruns)
                } else {
                    Vec::new()
                };

                if !eob_before_has_runs && eob_before_face.face_id != 0 {
                    self.apply_face(&eob_before_face, frame_glyphs);
                }
                let bstr = &overlay_before_buf[..eob_before_len as usize];
                let mut bi = 0usize;
                let mut bcurrent_run = 0usize;
                while bi < bstr.len() && row < max_rows {
                    if eob_before_has_runs && bcurrent_run < eob_before_face_runs.len() {
                        bcurrent_run = apply_overlay_face_run(
                            &eob_before_face_runs, bi, bcurrent_run, frame_glyphs,
                        );
                    }

                    let (bch, blen) = decode_utf8(&bstr[bi..]);
                    bi += blen;
                    if bch == '\n' {
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        if row >= max_rows { break; }
                        continue;
                    }
                    if bch == '\r' { continue; }
                    let bchar_cols = if is_wide_char(bch) { 2 } else { 1 };
                    let b_advance = bchar_cols as f32 * char_w;
                    if x_offset + b_advance > avail_width {
                        if params.truncate_lines { break; }
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        if row >= max_rows { break; }
                    }
                    let gx = content_x + x_offset;
                    let gy = row_y[row as usize];
                    frame_glyphs.add_char(bch, gx, gy, b_advance, char_h, ascent, false);
                    col += bchar_cols;
                    x_offset += b_advance;
                }
                if (eob_before_has_runs || eob_before_face.face_id != 0) && current_face_id >= 0 {
                    self.apply_face(&self.face_data, frame_glyphs);
                }
            }

            // Render after-string at end-of-buffer
            if overlay_after_len > 0 {
                let eob_after_has_runs = overlay_after_nruns > 0;
                let eob_after_face_runs = if eob_after_has_runs {
                    parse_overlay_face_runs(&overlay_after_buf, overlay_after_len as usize, overlay_after_nruns)
                } else {
                    Vec::new()
                };

                if !eob_after_has_runs && overlay_after_face.face_id != 0 {
                    self.apply_face(&overlay_after_face, frame_glyphs);
                }
                let astr = &overlay_after_buf[..overlay_after_len as usize];
                let mut ai = 0usize;
                let mut acurrent_run = 0usize;
                while ai < astr.len() && row < max_rows {
                    if eob_after_has_runs && acurrent_run < eob_after_face_runs.len() {
                        acurrent_run = apply_overlay_face_run(
                            &eob_after_face_runs, ai, acurrent_run, frame_glyphs,
                        );
                    }

                    let (ach, alen) = decode_utf8(&astr[ai..]);
                    ai += alen;
                    if ach == '\n' {
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        if row >= max_rows { break; }
                        continue;
                    }
                    if ach == '\r' { continue; }
                    let achar_cols = if is_wide_char(ach) { 2 } else { 1 };
                    let a_advance = achar_cols as f32 * char_w;
                    if x_offset + a_advance > avail_width {
                        if params.truncate_lines { break; }
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        if row >= max_rows { break; }
                    }
                    let gx = content_x + x_offset;
                    let gy = row_y[row as usize];
                    frame_glyphs.add_char(ach, gx, gy, a_advance, char_h, ascent, false);
                    col += achar_cols;
                    x_offset += a_advance;
                }
                if (eob_after_has_runs || overlay_after_face.face_id != 0) && current_face_id >= 0 {
                    self.apply_face(&self.face_data, frame_glyphs);
                }
            }
        }

        // Close any remaining box face region at end of text
        if box_active {
            let box_end_x = content_x + x_offset;
            let gy = row_y[box_row.min(max_rows - 1) as usize];
            let bw = box_line_width.max(1) as f32;
            frame_glyphs.add_border(box_start_x, gy, box_end_x - box_start_x, bw, box_color);
            frame_glyphs.add_border(box_start_x, gy + char_h - bw, box_end_x - box_start_x, bw, box_color);
            frame_glyphs.add_border(box_start_x, gy, bw, char_h, box_color);
            frame_glyphs.add_border(box_end_x - bw, gy, bw, char_h, box_color);
            box_active = false;
        }

        // If cursor wasn't placed (point is past visible content), place at end
        if !cursor_placed && params.point >= params.window_start {
            cursor_col = col;
            cursor_row = row.min(max_rows - 1);
            cursor_x = x_offset;
            let cursor_px = content_x + x_offset;
            let cursor_y = row_y[row.min(max_rows - 1) as usize];

            let cursor_style = if params.selected {
                params.cursor_type
            } else if params.cursor_in_non_selected {
                3
            } else {
                255 // skip
            };

            if cursor_style < 255 {
                // Use face-specific dimensions so cursor matches variable-height faces
                let cursor_face_w = if self.face_data.font_char_width > 0.0 {
                    self.face_data.font_char_width
                } else {
                    char_w
                };
                let (cursor_w, cursor_h) = match params.cursor_type {
                    1 => (params.cursor_bar_width.max(1) as f32, face_h), // bar
                    2 => (cursor_face_w, 2.0),                             // hbar
                    _ => (cursor_face_w, face_h),                          // box/hollow
                };
                frame_glyphs.add_cursor(
                    params.window_id as i32,
                    cursor_px,
                    cursor_y,
                    cursor_w,
                    cursor_h,
                    cursor_style,
                    face_fg,
                );
            }

            if cursor_style == 0 {
                let cursor_face_w = if self.face_data.font_char_width > 0.0 {
                    self.face_data.font_char_width
                } else {
                    char_w
                };
                let cursor_h = face_h;
                frame_glyphs.set_cursor_inverse(
                    cursor_px,
                    cursor_y,
                    cursor_face_w,
                    cursor_h,
                    face_fg,
                    face_bg,
                );
            }
        }

        // Fill remaining rows with default background
        let filled_rows = row + 1;
        if filled_rows < max_rows {
            let gy = row_y[filled_rows as usize];
            let remaining_h = (text_y + text_height) - row_y[filled_rows as usize];
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
                let gy = row_y[r];

                // Right fringe: continuation indicator for wrapped lines
                if right_fringe_width > 0.0 && row_continued.get(r).copied().unwrap_or(false) {
                    // Bitmap 7: left-curly-arrow (continuation)
                    render_fringe_bitmap(
                        7, right_fringe_x, gy,
                        right_fringe_width, char_h, default_fg,
                        frame_glyphs,
                    );
                }

                // Right fringe: truncation indicator
                if right_fringe_width > 0.0 && row_truncated.get(r).copied().unwrap_or(false) {
                    // Bitmap 4: right-arrow (truncation)
                    render_fringe_bitmap(
                        4, right_fringe_x, gy,
                        right_fringe_width, char_h, default_fg,
                        frame_glyphs,
                    );
                }

                // Left fringe: continuation indicator for continued lines
                if left_fringe_width > 0.0 && row_continuation.get(r).copied().unwrap_or(false) {
                    // Bitmap 8: right-curly-arrow (continuation from prev)
                    render_fringe_bitmap(
                        8, left_fringe_x, gy,
                        left_fringe_width, char_h, default_fg,
                        frame_glyphs,
                    );
                }

                // User-specified left-fringe display property bitmap
                if left_fringe_width > 0.0 {
                    if let Some(&(bid, fg, bg)) = row_left_fringe.get(r) {
                        if bid > 0 {
                            let ffg = if fg != 0 { Color::from_pixel(fg) } else { default_fg };
                            render_fringe_bitmap(
                                bid, left_fringe_x, gy,
                                left_fringe_width, char_h, ffg,
                                frame_glyphs,
                            );
                        }
                    }
                }

                // User-specified right-fringe display property bitmap
                if right_fringe_width > 0.0 {
                    if let Some(&(bid, fg, bg)) = row_right_fringe.get(r) {
                        if bid > 0 {
                            let ffg = if fg != 0 { Color::from_pixel(fg) } else { default_fg };
                            render_fringe_bitmap(
                                bid, right_fringe_x, gy,
                                right_fringe_width, char_h, ffg,
                                frame_glyphs,
                            );
                        }
                    }
                }
            }

            // EOB empty line indicators (bitmap 24 = empty_line)
            if params.indicate_empty_lines > 0 {
                let eob_start = actual_rows;
                for r in eob_start as usize..max_rows as usize {
                    let gy = row_y[r];
                    if params.indicate_empty_lines == 2 {
                        // Right fringe
                        if right_fringe_width > 0.0 {
                            render_fringe_bitmap(
                                24, right_fringe_x, gy,
                                right_fringe_width, char_h, default_fg,
                                frame_glyphs,
                            );
                        }
                    } else {
                        // Left fringe (default)
                        if left_fringe_width > 0.0 {
                            render_fringe_bitmap(
                                24, left_fringe_x, gy,
                                left_fringe_width, char_h, default_fg,
                                frame_glyphs,
                            );
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
                for r in 0..max_rows as usize {
                    let gx = content_x + fci_col as f32 * char_w;
                    let gy = row_y[r];
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
            (content_x + cursor_x) as i32,
            (row_y[cursor_row as usize]) as i32,
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
        let default_fg = Color::from_pixel(line_face.fg);

        // Draw background
        frame_glyphs.add_stretch(x, y, width, height, bg, line_face.face_id, true);

        if bytes <= 0 {
            return;
        }

        // Extract text length and face run count from packed return value
        let text_len = (bytes & 0xFFFFFFFF) as usize;
        let nruns = (bytes >> 32) as usize;

        let text = &line_buf[..text_len];

        // Parse face runs: each run is 10 bytes (u16 byte_offset, u32 fg, u32 bg)
        // stored after text data
        struct FaceRun {
            byte_offset: u16,
            fg: u32,
            bg: u32,
        }
        let mut face_runs: Vec<FaceRun> = Vec::with_capacity(nruns);
        if nruns > 0 {
            let runs_start = text_len;
            for i in 0..nruns {
                let off = runs_start + i * 10;
                if off + 10 <= line_buf.len() {
                    let byte_offset = u16::from_ne_bytes([
                        line_buf[off], line_buf[off + 1],
                    ]);
                    let fg = u32::from_ne_bytes([
                        line_buf[off + 2], line_buf[off + 3],
                        line_buf[off + 4], line_buf[off + 5],
                    ]);
                    let bg_val = u32::from_ne_bytes([
                        line_buf[off + 6], line_buf[off + 7],
                        line_buf[off + 8], line_buf[off + 9],
                    ]);
                    face_runs.push(FaceRun {
                        byte_offset,
                        fg,
                        bg: bg_val,
                    });
                }
            }
        }

        // Render text with face runs
        let mut sl_x_offset: f32 = 0.0;
        let mut byte_idx = 0usize;
        let mut current_run = 0usize;

        while byte_idx < text.len() && sl_x_offset < width {
            // Check if we need to switch face for this byte position
            if current_run < face_runs.len() {
                let next_run = if current_run + 1 < face_runs.len() {
                    current_run + 1
                } else {
                    face_runs.len()
                };
                // Move to the correct run for this byte position
                while next_run < face_runs.len()
                    && byte_idx >= face_runs[next_run].byte_offset as usize
                {
                    // Already past this run, skip
                    break;
                }
                if byte_idx >= face_runs[current_run].byte_offset as usize {
                    // Check if next run starts here or we're in current run
                    if current_run + 1 < face_runs.len()
                        && byte_idx >= face_runs[current_run + 1].byte_offset as usize
                    {
                        current_run += 1;
                    }
                    let run = &face_runs[current_run];
                    if run.fg != 0 || run.bg != 0 {
                        let run_fg = Color::from_pixel(run.fg);
                        let run_bg = Color::from_pixel(run.bg);
                        frame_glyphs.set_face(
                            line_face.face_id, run_fg, Some(run_bg),
                            false, false, 0, None, 0, None, 0, None,
                        );
                    }
                }
            }

            let (ch, ch_len) = decode_utf8(&text[byte_idx..]);
            byte_idx += ch_len;

            if ch == '\n' || ch == '\r' {
                continue;
            }

            let gx = x + sl_x_offset;
            frame_glyphs.add_char(ch, gx, y, char_w, height, ascent, true);
            sl_x_offset += char_w;
        }

        // Restore default mode-line face
        frame_glyphs.set_face(
            line_face.face_id, default_fg, Some(bg),
            false, false, 0, None, 0, None, 0, None,
        );

        // Fill remaining with background
        if sl_x_offset < width {
            let gx = x + sl_x_offset;
            let remaining = width - sl_x_offset;
            frame_glyphs.add_stretch(gx, y, remaining, height, bg, line_face.face_id, true);
        }

        // Draw box borders if the face has box type
        if line_face.box_type > 0 {
            let bw = line_face.box_line_width.max(1) as f32;
            let box_color = Color::from_pixel(line_face.box_color);
            // Top border
            frame_glyphs.add_border(x, y, width, bw, box_color);
            // Bottom border
            frame_glyphs.add_border(x, y + height - bw, width, bw, box_color);
            // Left border
            frame_glyphs.add_border(x, y, bw, height, box_color);
            // Right border
            frame_glyphs.add_border(x + width - bw, y, bw, height, box_color);
        }
    }
}

/// Get the advance width for a character in a specific face.
/// For monospace fonts, returns char_cols * char_w.
/// For proportional fonts, queries the font via FFI (with ASCII cache).
///
/// Standalone function to avoid borrow conflicts with `LayoutEngine::text_buf`.
unsafe fn char_advance(
    ascii_width_cache: &mut std::collections::HashMap<(u32, i32), [f32; 128]>,
    ch: char,
    char_cols: i32,
    char_w: f32,
    is_mono: bool,
    face_id: u32,
    font_size: i32,
    face_char_w: f32,
    window: EmacsWindow,
) -> f32 {
    // Use the face-specific character width when available (handles
    // faces with :height attribute that use a differently-sized font).
    let face_w = if face_char_w > 0.0 { face_char_w } else { char_w };

    if is_mono {
        return char_cols as f32 * face_w;
    }

    let cp = ch as u32;
    if cp < 128 {
        // Check ASCII cache
        let cache_key = (face_id, font_size);
        if !ascii_width_cache.contains_key(&cache_key) {
            let mut widths = [0.0f32; 128];
            neomacs_layout_fill_ascii_widths(
                window,
                face_id as c_int,
                widths.as_mut_ptr(),
            );
            for w in widths.iter_mut() {
                if *w < 0.0 {
                    *w = face_w;
                }
            }
            ascii_width_cache.insert(cache_key, widths);
        }
        return ascii_width_cache[&cache_key][cp as usize];
    }

    // Non-ASCII: query individually
    let w = neomacs_layout_char_width(window, cp as c_int, face_id as c_int);
    if w > 0.0 { w } else { char_cols as f32 * face_w }
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
// is_combining_char has been replaced by is_cluster_extender and
// collect_grapheme_cluster which properly handle multi-codepoint
// grapheme clusters (emoji ZWJ, combining marks, etc.)

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
    // Emoji (East Asian Width = W in emoji presentation)
    || is_emoji_presentation(cp)
}

/// Check if a codepoint is an emoji that should have wide (2-column) presentation.
fn is_emoji_presentation(cp: u32) -> bool {
    // Emoticons
    (0x1F600..=0x1F64F).contains(&cp)
    // Miscellaneous Symbols and Pictographs
    || (0x1F300..=0x1F5FF).contains(&cp)
    // Transport and Map Symbols
    || (0x1F680..=0x1F6FF).contains(&cp)
    // Supplemental Symbols and Pictographs
    || (0x1F900..=0x1F9FF).contains(&cp)
    // Symbols and Pictographs Extended-A
    || (0x1FA00..=0x1FA6F).contains(&cp)
    || (0x1FA70..=0x1FAFF).contains(&cp)
    // Dingbats (selected emoji)
    || (0x2702..=0x27B0).contains(&cp)
    // Regional Indicator Symbols
    || (0x1F1E0..=0x1F1FF).contains(&cp)
    // Playing cards, mahjong, dominos
    || cp == 0x1F004  // mahjong red dragon
    || cp == 0x1F0CF  // playing card black joker
    // Skin tone modifiers (display-width 0 when following emoji, but 2 when standalone)
    // We'll treat them as part of clusters, so this is just for standalone
}

/// Check if a character is a grapheme cluster extender: it should be
/// bundled with the preceding base character for proper rendering.
fn is_cluster_extender(ch: char) -> bool {
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
    // Hebrew combining marks
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
    // Variation selectors
    || (0xFE00..=0xFE0F).contains(&cp)
    || (0xE0100..=0xE01EF).contains(&cp)
    // Emoji skin tone modifiers
    || (0x1F3FB..=0x1F3FF).contains(&cp)
    // Combining Enclosing Keycap
    || cp == 0x20E3
    // Emoji tag characters (U+E0020..U+E007F, used in flag tag sequences)
    || (0xE0020..=0xE007F).contains(&cp)
    // Zero-width joiner (handled specially in collect_grapheme_cluster)
    || cp == 0x200D
    // Zero-width non-joiner, zero-width space, directional marks
    || cp == 0x200C
    || cp == 0x200B
    || cp == 0x200E
    || cp == 0x200F
    || cp == 0xFEFF
}

/// Check if a codepoint is a Regional Indicator Symbol.
fn is_regional_indicator(cp: u32) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&cp)
}

/// Collect a grapheme cluster starting with the base character `base_ch`.
/// Peeks at subsequent bytes in `remaining` to find cluster extenders.
///
/// Returns (cluster_string, extra_bytes_consumed, extra_chars_consumed).
/// If there are no extenders, returns (None, 0, 0) — use single-char path.
fn collect_grapheme_cluster(
    base_ch: char,
    remaining: &[u8],
) -> (Option<String>, usize, usize) {
    let mut extra_bytes = 0usize;
    let mut extra_chars = 0usize;
    let mut cluster = String::new();
    cluster.push(base_ch);

    let mut peek = 0usize;
    let base_is_ri = is_regional_indicator(base_ch as u32);

    loop {
        if peek >= remaining.len() { break; }
        let (next_ch, next_len) = decode_utf8(&remaining[peek..]);

        if next_ch == '\u{200D}' {
            // ZWJ: consume it AND the next character (emoji ZWJ sequence)
            cluster.push(next_ch);
            peek += next_len;
            extra_bytes += next_len;
            extra_chars += 1;

            // Consume the character after ZWJ
            if peek < remaining.len() {
                let (zjoin_ch, zjoin_len) = decode_utf8(&remaining[peek..]);
                cluster.push(zjoin_ch);
                peek += zjoin_len;
                extra_bytes += zjoin_len;
                extra_chars += 1;
            }
        } else if is_cluster_extender(next_ch) && next_ch != '\u{200D}' {
            // Combining mark, variation selector, skin tone modifier, etc.
            cluster.push(next_ch);
            peek += next_len;
            extra_bytes += next_len;
            extra_chars += 1;
        } else if base_is_ri && is_regional_indicator(next_ch as u32)
                  && cluster.chars().count() == 1 {
            // Second regional indicator forms a flag pair
            cluster.push(next_ch);
            peek += next_len;
            extra_bytes += next_len;
            extra_chars += 1;
            break; // Flags are exactly 2 regional indicators
        } else {
            break;
        }
    }

    if extra_chars > 0 {
        (Some(cluster), extra_bytes, extra_chars)
    } else {
        (None, 0, 0)
    }
}

/// Check if a character is potentially glyphless and should be looked up
/// in the glyphless-char-display char-table.
/// This is a fast pre-filter — only chars in these ranges trigger the FFI call.
fn is_potentially_glyphless(ch: char) -> bool {
    let cp = ch as u32;
    // C1 control characters (0x80-0x9F)
    (0x80..=0x9F).contains(&cp)
    // Soft hyphen (sometimes glyphless)
    || cp == 0xAD
    // Unicode format/control characters
    || (0x200B..=0x200F).contains(&cp)  // ZWSP, ZWNJ, ZWJ, LRM, RLM
    || (0x202A..=0x202E).contains(&cp)  // bidi embedding
    || (0x2060..=0x2069).contains(&cp)  // word joiner, invisible separators
    || (0x2028..=0x2029).contains(&cp)  // line/paragraph separator
    || cp == 0xFEFF                      // BOM / ZWNBSP
    || (0xFFF0..=0xFFFD).contains(&cp)  // specials (interlinear annotation, replacement)
    // Emacs raw bytes (BYTE8 encoding: 0x3FFF80..0x3FFFFF)
    || (0x3FFF80..=0x3FFFFF).contains(&cp)
    // Unassigned/private use — only very high ranges
    || (0xE0000..=0xE007F).contains(&cp)  // tags block
}

/// Map Emacs standard fringe bitmap IDs to Unicode characters.
/// Render a fringe bitmap at the given position using Border rects.
/// Queries the actual bitmap data from Emacs via FFI and draws
/// each set bit as a filled pixel rectangle.
unsafe fn render_fringe_bitmap(
    bitmap_id: i32,
    fringe_x: f32,
    row_y: f32,
    fringe_width: f32,
    row_height: f32,
    fg: Color,
    frame_glyphs: &mut FrameGlyphBuffer,
) {
    let mut bits = [0u16; 64]; // max 64 rows
    let mut bm_width: c_int = 0;
    let mut bm_height: c_int = 0;
    let mut bm_align: c_int = 0;

    let rows = neomacs_layout_get_fringe_bitmap(
        bitmap_id,
        bits.as_mut_ptr(),
        64,
        &mut bm_width,
        &mut bm_height,
        &mut bm_align,
    );

    if rows <= 0 || bm_width <= 0 {
        return;
    }

    let bm_w = bm_width as f32;
    let bm_h = rows as f32;

    // Calculate pixel scale: map bitmap pixels to screen pixels
    // Scale to fit within fringe width while maintaining aspect ratio
    let scale = (fringe_width / bm_w).min(row_height / bm_h).max(1.0);
    let pixel_w = scale;
    let pixel_h = scale;

    let scaled_w = bm_w * scale;
    let scaled_h = bm_h * scale;

    // Center horizontally in fringe
    let x_start = fringe_x + (fringe_width - scaled_w) / 2.0;

    // Vertical alignment within the row
    let y_start = match bm_align {
        1 => row_y,                                  // top
        2 => row_y + row_height - scaled_h,          // bottom
        _ => row_y + (row_height - scaled_h) / 2.0,  // center (default)
    };

    // Render each row of the bitmap
    for r in 0..rows as usize {
        let row_bits = bits[r];
        if row_bits == 0 { continue; }

        let py = y_start + r as f32 * pixel_h;
        if py + pixel_h < row_y || py > row_y + row_height {
            continue; // skip rows outside visible area
        }

        // Scan for horizontal runs of consecutive set bits
        let mut bit = bm_width - 1; // MSB = leftmost pixel
        while bit >= 0 {
            if row_bits & (1 << bit) != 0 {
                // Start of a run
                let run_start = bit;
                while bit > 0 && row_bits & (1 << (bit - 1)) != 0 {
                    bit -= 1;
                }
                let run_end = bit;
                let run_len = (run_start - run_end + 1) as f32;
                let px = x_start + (bm_width - 1 - run_start) as f32 * pixel_w;
                frame_glyphs.add_border(px, py, run_len * pixel_w, pixel_h, fg);
            }
            bit -= 1;
        }
    }
}

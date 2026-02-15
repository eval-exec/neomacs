//! The Rust layout engine — Phase 1+2: Monospace layout with face resolution.
//!
//! Reads buffer text via FFI, resolves faces per character position,
//! computes line breaks, positions glyphs on a fixed-width grid, and
//! produces FrameGlyphBuffer compatible with the existing wgpu renderer.

use std::ffi::CStr;
use std::ffi::c_int;
use std::ffi::c_void;

use crate::core::face::{Face, FaceAttributes, UnderlineStyle, BoxType};
use crate::core::frame_glyphs::{CursorStyle, FrameGlyphBuffer, StipplePattern};
use crate::core::types::{Color, Rect};
use super::types::*;
use super::emacs_ffi::*;
use super::unicode::*;
use super::hit_test::*;
use super::status_line::*;
use super::bidi_layout::reorder_row_bidi;
use super::font_metrics::FontMetricsService;

/// Maximum number of characters in a ligature run before forced flush.
const MAX_LIGATURE_RUN_LEN: usize = 64;

/// Buffer for accumulating same-face text runs for ligature shaping.
struct LigatureRunBuffer {
    chars: Vec<char>,
    advances: Vec<f32>,
    start_x: f32,
    start_y: f32,
    face_h: f32,
    face_ascent: f32,
    face_id: u32,
    total_advance: f32,
    is_overlay: bool,
    height_scale: f32,
}

impl LigatureRunBuffer {
    fn new() -> Self {
        Self {
            chars: Vec::with_capacity(MAX_LIGATURE_RUN_LEN),
            advances: Vec::with_capacity(MAX_LIGATURE_RUN_LEN),
            start_x: 0.0,
            start_y: 0.0,
            face_h: 0.0,
            face_ascent: 0.0,
            face_id: 0,
            total_advance: 0.0,
            is_overlay: false,
            height_scale: 0.0,
        }
    }

    fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    fn len(&self) -> usize {
        self.chars.len()
    }

    fn clear(&mut self) {
        self.chars.clear();
        self.advances.clear();
        self.total_advance = 0.0;
    }

    /// Push a character and its advance width into the run.
    fn push(&mut self, ch: char, advance: f32) {
        self.chars.push(ch);
        self.advances.push(advance);
        self.total_advance += advance;
    }

    /// Start a new run at the given position with the given face parameters.
    fn start(&mut self, x: f32, y: f32, face_h: f32, face_ascent: f32,
             face_id: u32, is_overlay: bool, height_scale: f32) {
        self.clear();
        self.start_x = x;
        self.start_y = y;
        self.face_h = face_h;
        self.face_ascent = face_ascent;
        self.face_id = face_id;
        self.is_overlay = is_overlay;
        self.height_scale = height_scale;
    }
}

/// Check if a character is a ligature-eligible symbol/punctuation.
/// Programming font ligatures only form between these characters.
#[inline]
fn is_ligature_char(ch: char) -> bool {
    matches!(ch,
        '!' | '#' | '$' | '%' | '&' | '*' | '+' | '-' | '.' | '/' |
        ':' | ';' | '<' | '=' | '>' | '?' | '@' | '\\' | '^' | '|' | '~'
    )
}

/// Check if a run consists entirely of ligature-eligible characters.
/// Mixed runs (e.g., "arrow:" or "Font:") should NOT be composed,
/// only pure symbol runs (e.g., "->", "!=", "===").
#[inline]
fn run_is_pure_ligature(run: &LigatureRunBuffer) -> bool {
    run.chars.iter().all(|&ch| is_ligature_char(ch))
}

/// Flush the accumulated ligature run as either individual chars or a composed glyph.
fn flush_run(run: &LigatureRunBuffer, frame_glyphs: &mut FrameGlyphBuffer, ligatures: bool) {
    if run.is_empty() {
        return;
    }
    // Only compose runs of pure ligature-forming characters (punctuation/symbols).
    // Alphabetic/numeric runs are emitted as individual chars.
    let compose = ligatures && run.len() > 1 && run_is_pure_ligature(run);
    if !compose {
        // Emit individual chars (fallback / ligatures disabled / single char)
        let mut x = run.start_x;
        for (i, &ch) in run.chars.iter().enumerate() {
            let adv = run.advances[i];
            if run.height_scale > 0.0 && run.height_scale != 1.0 {
                let orig_size = frame_glyphs.font_size();
                frame_glyphs.set_font_size(orig_size * run.height_scale);
                frame_glyphs.add_char(ch, x, run.start_y, adv, run.face_h, run.face_ascent, run.is_overlay);
                frame_glyphs.set_font_size(orig_size);
            } else {
                frame_glyphs.add_char(ch, x, run.start_y, adv, run.face_h, run.face_ascent, run.is_overlay);
            }
            x += adv;
        }
    } else {
        // Emit as composed glyph — render thread will shape via HarfBuzz
        let text: String = run.chars.iter().collect();
        let base_char = run.chars[0];
        if run.height_scale > 0.0 && run.height_scale != 1.0 {
            let orig_size = frame_glyphs.font_size();
            frame_glyphs.set_font_size(orig_size * run.height_scale);
            frame_glyphs.add_composed_char(&text, base_char, run.start_x, run.start_y,
                run.total_advance, run.face_h, run.face_ascent, run.is_overlay);
            frame_glyphs.set_font_size(orig_size);
        } else {
            frame_glyphs.add_composed_char(&text, base_char, run.start_x, run.start_y,
                run.total_advance, run.face_h, run.face_ascent, run.is_overlay);
        }
    }
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
    /// Per-face ASCII width cache: actual glyph widths via text_extents().
    /// Key: (face_id, font_size), Value: advance widths for chars 0-127.
    pub(crate) ascii_width_cache: std::collections::HashMap<(u32, i32), [f32; 128]>,
    /// Hit-test data being built for current frame
    hit_data: Vec<WindowHitData>,
    /// Reusable ligature run buffer
    run_buf: LigatureRunBuffer,
    /// Whether ligatures are enabled
    pub ligatures_enabled: bool,
    /// Default face font family (set during first face resolution of each window).
    /// Used for overstrike: when bold variant unavailable, renderer uses this
    /// family instead of the proportional fallback.
    default_font_family: String,
    /// Cosmic-text font metrics service (lazily initialized on first use)
    font_metrics: Option<FontMetricsService>,
    /// Whether to use cosmic-text for font metrics instead of C FFI
    pub use_cosmic_metrics: bool,
}

impl LayoutEngine {
    /// Create a new layout engine.
    pub fn new() -> Self {
        Self {
            text_buf: Vec::with_capacity(64 * 1024), // 64KB initial
            face_data: FaceDataFFI::default(),
            ascii_width_cache: std::collections::HashMap::new(),
            hit_data: Vec::new(),
            run_buf: LigatureRunBuffer::new(),
            ligatures_enabled: false,
            default_font_family: String::new(),
            font_metrics: None,
            use_cosmic_metrics: true,
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

        // Clear hit-test data for new frame
        self.hit_data.clear();

        // Lazy-initialize FontMetricsService when cosmic metrics are enabled
        if self.use_cosmic_metrics && self.font_metrics.is_none() {
            self.font_metrics = Some(FontMetricsService::new());
        } else if !self.use_cosmic_metrics && self.font_metrics.is_some() {
            // Drop the service when switching back to C metrics
            self.font_metrics = None;
        }

        // Always populate face_id=0 (DEFAULT_FACE_ID) in the faces map.
        // Many code paths use face_id=0 as a fallback: initial set_face(),
        // divider stretches, overlay strings without explicit face, and
        // the legacy menu/tool bar extraction.  Without this, glyphs with
        // face_id=0 have no Face entry and fall back to generic monospace.
        {
            let mut default_face = FaceDataFFI::default();
            let rc = neomacs_layout_default_face(frame, &mut default_face);
            if rc >= 0 {
                self.apply_face(&default_face, frame, frame_glyphs);
            }
        }

        // Get number of windows (direct Rust struct access, no FFI call)
        let window_count = super::emacs_types::frame_window_count(
            frame as *const std::ffi::c_void,
        );
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

            // Read buffer metadata directly from Emacs struct (Phase 2: bypass C wrappers)
            let (rust_begv, rust_zv) = if !wp.buffer_ptr.is_null() {
                super::emacs_types::buffer_bounds(wp.buffer_ptr)
            } else {
                (1, 1)
            };
            let rust_point = if !wp.buffer_ptr.is_null() {
                super::emacs_types::buffer_point(wp.buffer_ptr)
            } else {
                1
            };
            let rust_tab_width = if !wp.buffer_ptr.is_null() {
                super::emacs_types::buffer_tab_width(wp.buffer_ptr)
            } else {
                8
            };
            let rust_truncate = if !wp.buffer_ptr.is_null() {
                super::emacs_types::buffer_truncate_lines(wp.buffer_ptr)
            } else {
                false
            };
            let rust_word_wrap = if !wp.buffer_ptr.is_null() {
                super::emacs_types::buffer_word_wrap(wp.buffer_ptr)
            } else {
                false
            };

            // Convert FFI params to our types
            // Buffer metadata fields use direct Rust struct reads instead of C values
            let params = WindowParams {
                window_id: wp.window_id,
                buffer_id: wp.buffer_id,
                bounds: Rect::new(wp.x, wp.y, wp.width, wp.height),
                text_bounds: Rect::new(wp.text_x, wp.text_y, wp.text_width, wp.text_height),
                selected: wp.selected != 0,
                is_minibuffer: wp.is_minibuffer != 0,
                window_start: wp.window_start,
                window_end: wp.window_end,
                point: rust_point,
                buffer_size: rust_zv,
                buffer_begv: rust_begv,
                hscroll: wp.hscroll,
                truncate_lines: rust_truncate,
                word_wrap: rust_word_wrap,
                tab_width: rust_tab_width,
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

        // Publish hit-test data for mouse interaction queries
        unsafe {
            *std::ptr::addr_of_mut!(FRAME_HIT_DATA) = Some(std::mem::take(&mut self.hit_data));
        }
    }

    /// Apply face data from FFI to the FrameGlyphBuffer's current face state.
    pub(crate) unsafe fn apply_face(&self, face: &FaceDataFFI, frame: EmacsFrame,
                          frame_glyphs: &mut FrameGlyphBuffer) {
        let fg = Color::from_pixel(face.fg);
        let bg = Color::from_pixel(face.bg);
        let font_weight = face.font_weight as u16;
        let italic = face.italic != 0;
        let overstrike = face.overstrike != 0;

        // Get font family string from C pointer
        let font_family = if !face.font_family.is_null() {
            CStr::from_ptr(face.font_family).to_str().unwrap_or("monospace")
        } else {
            "monospace"
        };

        // When overstrike is set, Emacs couldn't find a bold variant of the
        // font, so it kept the regular (non-bold) font. Use the default
        // face's font family for rendering so the renderer draws with the
        // monospace font (matching official Emacs behavior).
        let effective_family = if overstrike && !self.default_font_family.is_empty() {
            &self.default_font_family
        } else {
            font_family
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
            effective_family,
            font_weight,
            italic,
            face.font_size as f32,
            face.underline_style as u8,
            underline_color,
            face.strike_through as u8,
            strike_color,
            face.overline as u8,
            overline_color,
            overstrike,
        );

        // Build complete Face for this face_id so the render thread gets
        // all attributes (box, underline, etc.) in one shot per frame,
        // eliminating stale-cache bugs when Emacs reuses face IDs.
        let mut attrs = FaceAttributes::empty();
        if font_weight >= 700 { attrs |= FaceAttributes::BOLD; }
        if italic { attrs |= FaceAttributes::ITALIC; }
        if face.underline_style > 0 { attrs |= FaceAttributes::UNDERLINE; }
        if face.strike_through > 0 { attrs |= FaceAttributes::STRIKE_THROUGH; }
        if face.overline > 0 { attrs |= FaceAttributes::OVERLINE; }
        if face.box_type > 0 { attrs |= FaceAttributes::BOX; }

        frame_glyphs.faces.insert(face.face_id, Face {
            id: face.face_id,
            foreground: fg,
            background: bg,
            underline_color,
            overline_color,
            strike_through_color: strike_color,
            box_color: if face.box_type > 0 { Some(Color::from_pixel(face.box_color)) } else { None },
            font_family: effective_family.to_string(),
            font_size: face.font_size as f32,
            font_weight,
            attributes: attrs,
            underline_style: match face.underline_style {
                1 => UnderlineStyle::Line,
                2 => UnderlineStyle::Wave,
                3 => UnderlineStyle::Double,
                4 => UnderlineStyle::Dotted,
                5 => UnderlineStyle::Dashed,
                _ => UnderlineStyle::None,
            },
            box_type: if face.box_type == 1 { BoxType::Line } else { BoxType::None },
            box_line_width: face.box_line_width,
            box_corner_radius: face.box_corner_radius,
            font_ascent: face.font_ascent as i32,
            font_descent: face.font_descent,
            underline_position: face.underline_position.max(1),
            underline_thickness: face.underline_thickness.max(1),
        });

        // Fetch stipple pattern data if present and not yet cached
        if face.stipple > 0 && !frame_glyphs.stipple_patterns.contains_key(&face.stipple) {
            let mut bits_buf = [0u8; 1024]; // max 1024 bytes for stipple bitmap
            let mut w: c_int = 0;
            let mut h: c_int = 0;
            let rc = neomacs_layout_get_stipple_bitmap(
                frame as *mut c_void,
                face.stipple,
                bits_buf.as_mut_ptr(),
                bits_buf.len() as c_int,
                &mut w,
                &mut h,
            );
            if rc == 0 && w > 0 && h > 0 {
                let bytes_per_row = ((w + 7) / 8) as usize;
                let nbytes = bytes_per_row * h as usize;
                frame_glyphs.stipple_patterns.insert(face.stipple, StipplePattern {
                    width: w as u32,
                    height: h as u32,
                    bits: bits_buf[..nbytes].to_vec(),
                });
            }
        }
    }

    /// Add a stretch glyph, automatically using stipple if the given face has one.
    pub(crate) fn add_stretch_for_face(
        face: &FaceDataFFI,
        frame_glyphs: &mut FrameGlyphBuffer,
        x: f32, y: f32, width: f32, height: f32,
        bg: Color, face_id: u32, is_overlay: bool,
    ) {
        if face.stipple > 0 {
            let fg = Color::from_pixel(face.fg);
            frame_glyphs.add_stretch_stipple(x, y, width, height, bg, fg, face_id, is_overlay, face.stipple);
        } else {
            frame_glyphs.add_stretch(x, y, width, height, bg, face_id, is_overlay);
        }
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

        // Guard against zero/negative dimensions from FFI
        let char_w = if params.char_width > 0.0 { params.char_width } else { 8.0 };
        let char_h = if params.char_height > 0.0 {
            params.char_height + params.extra_line_spacing
        } else {
            16.0
        };
        let ascent = if params.font_ascent > 0.0 { params.font_ascent } else { 12.0 };

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

        // The minibuffer must always render at least 1 row.  Its pixel
        // height may be fractionally smaller than char_h (e.g. 24px vs
        // 24.15 with line-spacing) causing floor() to yield 0.
        let max_rows = if params.is_minibuffer && max_rows <= 0 && text_height > 0.0 {
            1
        } else {
            max_rows
        };

        if cols <= 0 || max_rows <= 0 {
            log::debug!("  layout_window id={}: skip — cols={} max_rows={}", params.window_id, cols, max_rows);
            return;
        }

        // Effective text start X (shifted right for line numbers)
        let content_x = text_x + lnum_pixel_width;

        // --- Scroll adjustment ---
        let window_start = if params.point > 0
            && params.point < params.window_start
            && !params.is_minibuffer
        {
            // Backward scroll: put point near top (1/4 down)
            let lines_above = (max_rows / 4).clamp(2, 10);
            let new_start = neomacs_layout_adjust_window_start(
                wp.window_ptr,
                wp.buffer_ptr,
                params.point,
                lines_above,
            );
            log::debug!("  scroll backward: point={} was before start={}, new start={}",
                params.point, params.window_start, new_start);
            new_start
        } else if params.point > 0
            && params.window_end > 0
            && params.point > params.window_end
            && !params.is_minibuffer
        {
            // Forward scroll: put point near bottom (3/4 down)
            let lines_above = if max_rows <= 2 { 1 } else { (max_rows * 3 / 4).clamp(2, max_rows - 1) };
            let new_start = neomacs_layout_adjust_window_start(
                wp.window_ptr,
                wp.buffer_ptr,
                params.point,
                lines_above,
            );
            log::debug!("  scroll forward: point={} was past end={}, new start={}",
                params.point, params.window_end, new_start);
            new_start
        } else {
            params.window_start
        };

        // Trigger fontification (jit-lock) for the visible region so that
        // face text properties are set before we read them.
        let read_chars = (params.buffer_size - window_start + 1).min(cols as i64 * max_rows as i64 * 2);
        let fontify_end = (window_start + read_chars).min(params.buffer_size);
        neomacs_layout_ensure_fontified(buffer, window_start, fontify_end);

        // Read buffer text directly from gap buffer (Phase 3: eliminates
        // per-character FFI overhead from the old neomacs_layout_buffer_text).
        let bytes_read = if read_chars <= 0 {
            0i64
        } else {
            let text_end = (window_start + read_chars).min(params.buffer_size);
            let byte_from = neomacs_buf_charpos_to_bytepos(buffer, window_start);
            let byte_to = neomacs_buf_charpos_to_bytepos(buffer, text_end);
            super::emacs_types::gap_buffer_copy_text(
                buffer as *const std::ffi::c_void,
                byte_from as isize,
                byte_to as isize,
                &mut self.text_buf,
            );
            self.text_buf.len() as i64
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
            400, false,
            0, None, 0, None, 0, None,
        );

        // Face resolution state: we only call face_at_pos when charpos >= next_face_check
        let mut current_face_id: i32 = -1; // force first lookup
        let mut next_face_check: i64 = 0;
        // Overstrike: when Emacs can't find bold variant, it sets face->overstrike
        // and keeps the regular font. We use default font metrics for layout.
        let mut overstrike = false;
        let mut face_fg = default_fg;
        let mut face_bg = default_bg;

        // Invisible text state: next charpos where we need to re-check
        let mut next_invis_check: i64 = window_start;

        // Display text property state
        let mut next_display_check: i64 = window_start;
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
        let mut overlay_before_naligns: i32 = 0;
        let mut overlay_after_naligns: i32 = 0;

        // Line number state
        let mut current_line: i64 = if lnum_enabled {
            neomacs_layout_count_line_number(
                buffer, window_start, lnum_config.widen,
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
        let mut charpos = window_start;
        let mut cursor_placed = false;
        let mut cursor_col = 0i32;
        let mut cursor_x: f32 = 0.0;  // pixel X of cursor
        let mut cursor_row = 0i32;
        let mut window_end_charpos = window_start;
        let mut byte_idx = 0usize;
        // hscroll state: how many columns to skip on each line
        let mut hscroll_remaining = hscroll;
        // Track current face's space width and line metrics
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
        let mut wrap_break_charpos = window_start;
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

        // Box face tracking: track active box regions for renderer's span detection
        let mut box_active = false;
        let mut box_start_x: f32 = 0.0;
        let mut box_row: i32 = 0;

        // Pixel Y limit: stop rendering when rows exceed the text area,
        // which can happen with variable-height faces pushing rows down.
        let text_y_limit = text_y + text_height;

        // Hit-test data for this window
        let mut hit_rows: Vec<HitRow> = Vec::new();
        let mut hit_row_charpos_start: i64 = window_start;

        // Ligature run accumulation
        let ligatures = self.ligatures_enabled;
        self.run_buf.clear();

        // Bidi reordering: track where each row's glyphs start in frame_glyphs.glyphs
        let mut row_glyph_start: usize = frame_glyphs.glyphs.len();

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
                self.apply_face(&lnum_face, frame, frame_glyphs);
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
                    self.apply_face(&self.face_data, frame, frame_glyphs);
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
                    reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                    col = 0;
                    x_offset = 0.0;
                    row += 1;
                    row_glyph_start = frame_glyphs.glyphs.len();
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
                    // Flush ligature run before invisible text skip
                    flush_run(&self.run_buf, frame_glyphs, ligatures);
                    self.run_buf.clear();
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
                let mut ovl_left_fringe_bitmap: i32 = 0;
                let mut ovl_left_fringe_fg: u32 = 0;
                let mut ovl_left_fringe_bg: u32 = 0;
                let mut ovl_right_fringe_bitmap: i32 = 0;
                let mut ovl_right_fringe_fg: u32 = 0;
                let mut ovl_right_fringe_bg: u32 = 0;
                overlay_before_naligns = 0;
                overlay_after_naligns = 0;
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
                    &mut ovl_left_fringe_bitmap,
                    &mut ovl_left_fringe_fg,
                    &mut ovl_left_fringe_bg,
                    &mut ovl_right_fringe_bitmap,
                    &mut ovl_right_fringe_fg,
                    &mut ovl_right_fringe_bg,
                    &mut overlay_before_naligns,
                    &mut overlay_after_naligns,
                );

                // Store fringe bitmaps from overlay display properties
                let r = row as usize;
                if ovl_left_fringe_bitmap > 0 && r < row_left_fringe.len() {
                    row_left_fringe[r] = (ovl_left_fringe_bitmap, ovl_left_fringe_fg, ovl_left_fringe_bg);
                }
                if ovl_right_fringe_bitmap > 0 && r < row_right_fringe.len() {
                    row_right_fringe[r] = (ovl_right_fringe_bitmap, ovl_right_fringe_fg, ovl_right_fringe_bg);
                }

                // Flush ligature run before overlay strings (only if overlays exist)
                if overlay_before_len > 0 || overlay_after_len > 0 {
                    flush_run(&self.run_buf, frame_glyphs, ligatures);
                    self.run_buf.clear();
                }

                // Render before-string (if any) — insert before buffer text
                if overlay_before_len > 0 {
                    let before_has_runs = overlay_before_nruns > 0;
                    let before_face_runs = if before_has_runs {
                        parse_overlay_face_runs(&overlay_before_buf, overlay_before_len as usize, overlay_before_nruns)
                    } else {
                        Vec::new()
                    };
                    let before_align_entries = if overlay_before_naligns > 0 {
                        parse_overlay_align_entries(&overlay_before_buf, overlay_before_len as usize, overlay_before_nruns, overlay_before_naligns)
                    } else {
                        Vec::new()
                    };
                    let mut bcurrent_align = 0usize;

                    // Use per-char face runs, overlay face, or resolve face for position
                    if !before_has_runs {
                        if overlay_before_face.face_id != 0 {
                            self.apply_face(&overlay_before_face, frame, frame_glyphs);
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
                                self.apply_face(&self.face_data, frame, frame_glyphs);
                            }
                            next_face_check = if next_check > charpos { next_check } else { charpos + 1 };
                        }
                    }

                    let bstr = &overlay_before_buf[..overlay_before_len as usize];
                    let mut bi = 0usize;
                    let mut bcurrent_run = 0usize;
                    while bi < bstr.len() && row < max_rows {
                        // Check for align-to entry at this byte offset
                        if bcurrent_align < before_align_entries.len()
                            && bi == before_align_entries[bcurrent_align].byte_offset as usize
                        {
                            let target_x = before_align_entries[bcurrent_align].align_to_cols * char_w;
                            if target_x > x_offset {
                                let gx = content_x + x_offset;
                                let gy = row_y[row as usize];
                                let stretch_w = target_x - x_offset;
                                Self::add_stretch_for_face(
                                    &self.face_data, frame_glyphs,
                                    gx, gy, stretch_w, char_h,
                                    face_bg, self.face_data.face_id, false,
                                );
                                col = before_align_entries[bcurrent_align].align_to_cols.ceil() as i32;
                                x_offset = target_x;
                            }
                            bcurrent_align += 1;
                            // Skip the character with the display property
                            let (_bch, blen) = decode_utf8(&bstr[bi..]);
                            bi += blen;
                            continue;
                        }

                        // Apply face run if needed
                        if before_has_runs && bcurrent_run < before_face_runs.len() {
                            bcurrent_run = apply_overlay_face_run(
                                &before_face_runs, bi, bcurrent_run, frame_glyphs,
                            );
                        }

                        let (bch, blen) = decode_utf8(&bstr[bi..]);
                        bi += blen;
                        if bch == '\n' {
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                            col = 0;
                            x_offset = 0.0;
                            row += 1;
                            row_glyph_start = frame_glyphs.glyphs.len();
                            if row >= max_rows { break; }
                            continue;
                        }
                        if bch == '\r' { continue; }

                        let bchar_cols = if is_wide_char(bch) { 2 } else { 1 };
                        let badv = bchar_cols as f32 * char_w;
                        if x_offset + badv > avail_width {
                            if params.truncate_lines {
                                // Skip to next newline, then advance to next row
                                reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                                while bi < bstr.len() {
                                    let (sc, sl) = decode_utf8(&bstr[bi..]);
                                    bi += sl;
                                    if sc == '\n' {
                                        col = 0;
                                        x_offset = 0.0;
                                        row += 1;
                                        row_glyph_start = frame_glyphs.glyphs.len();
                                        break;
                                    }
                                }
                                if row >= max_rows { break; }
                                continue;
                            }
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                            col = 0;
                            x_offset = 0.0;
                            row += 1;
                            row_glyph_start = frame_glyphs.glyphs.len();
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
                        self.apply_face(&self.face_data, frame, frame_glyphs);
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

                if display_prop.prop_type != 0 {
                    // Flush ligature run before display property handling
                    flush_run(&self.run_buf, frame_glyphs, ligatures);
                    self.run_buf.clear();
                }

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
                            self.apply_face(&self.face_data, frame, frame_glyphs);
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
                                400, false, 0, None, 0, None, 0, None,
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
                                        400, false, 0, None, 0, None, 0, None,
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
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                            col = 0;
                            x_offset = 0.0;
                            row += 1;
                            row_glyph_start = frame_glyphs.glyphs.len();
                            if row >= max_rows { break; }
                        }

                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];
                        frame_glyphs.add_char(dch, gx, gy, d_advance, char_h, ascent, false);
                        col += dchar_cols;
                        x_offset += d_advance;
                    }

                    // Restore text face after display string
                    if (has_face_runs || display_prop.display_fg != 0 || display_prop.display_bg != 0)
                        && current_face_id >= 0
                    {
                        self.apply_face(&self.face_data, frame, frame_glyphs);
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
                            self.apply_face(&self.face_data, frame, frame_glyphs);
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
                        Self::add_stretch_for_face(
                            &self.face_data, frame_glyphs,
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
                            self.apply_face(&self.face_data, frame, frame_glyphs);
                        }
                        next_face_check = if next_check > charpos { next_check } else { charpos + 1 };
                    }

                    let target_x = display_prop.align_to * char_w;
                    if target_x > x_offset && row < max_rows {
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize];
                        let stretch_w = target_x - x_offset;
                        Self::add_stretch_for_face(
                            &self.face_data, frame_glyphs,
                            gx, gy, stretch_w, char_h,
                            face_bg, self.face_data.face_id, false,
                        );
                        col = display_prop.align_to.ceil() as i32;
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
                        // Flush ligature run before face change
                        flush_run(&self.run_buf, frame_glyphs, ligatures);
                        self.run_buf.clear();
                        // Close previous box face region if active.
                        // Box borders are now rendered by the renderer's box span
                        // detection (supports both sharp and SDF rounded corners).
                        if box_active {
                            box_active = false;
                        }

                        current_face_id = fid;
                        face_fg = Color::from_pixel(self.face_data.fg);
                        face_bg = Color::from_pixel(self.face_data.bg);
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
                        // On first face resolution (at/before window_start),
                        // capture the default font family for overstrike.
                        if charpos <= window_start {
                            let family = if !self.face_data.font_family.is_null() {
                                CStr::from_ptr(self.face_data.font_family)
                                    .to_str().unwrap_or("monospace")
                            } else {
                                "monospace"
                            };
                            self.default_font_family = family.to_string();
                        }

                        // Overstrike: Emacs sets this when bold variant is
                        // unavailable. Use default font metrics for layout.
                        overstrike = self.face_data.overstrike != 0;

                        self.apply_face(&self.face_data, frame, frame_glyphs);

                        // Debug: check all face properties
                        if charpos < window_start + 5 {
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
                // Flush ligature run before cursor to split run at cursor position
                flush_run(&self.run_buf, frame_glyphs, ligatures);
                self.run_buf.clear();
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

                let cursor_style = if params.selected {
                    CursorStyle::from_type(params.cursor_type, params.cursor_bar_width)
                } else if params.cursor_in_non_selected {
                    Some(CursorStyle::Hollow)
                } else {
                    None
                };

                if let Some(style) = cursor_style {
                    frame_glyphs.add_cursor(
                        params.window_id as i32,
                        cursor_px,
                        cursor_y,
                        cursor_face_w,
                        face_h,
                        style,
                        face_fg,
                    );

                    if matches!(style, CursorStyle::FilledBox) {
                        frame_glyphs.set_cursor_inverse(
                            cursor_px,
                            cursor_y,
                            cursor_face_w,
                            face_h,
                            face_fg,
                            face_bg,
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
                    // Flush ligature run before newline
                    flush_run(&self.run_buf, frame_glyphs, ligatures);
                    self.run_buf.clear();

                    // Bidi reorder: reorder glyph X positions for this completed row
                    reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);

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
                        if self.face_data.extend != 0 {
                            Self::add_stretch_for_face(&self.face_data, frame_glyphs, gx, gy, remaining, char_h, fill_bg, fill_face, false);
                        } else {
                            frame_glyphs.add_stretch(gx, gy, remaining, char_h, fill_bg, fill_face, false);
                        }
                    }

                    // Box face tracking: box stays active across line breaks.
                    // Borders are rendered by the renderer's box span detection.
                    if box_active {
                        box_start_x = content_x;
                    }

                    // Record hit-test row (newline ends the row)
                    if (row as usize) < row_y.len() {
                        hit_rows.push(HitRow {
                            y_start: row_y[row as usize],
                            y_end: row_y[row as usize] + row_max_height,
                            charpos_start: hit_row_charpos_start,
                            charpos_end: charpos,
                        });
                        hit_row_charpos_start = charpos;
                    }

                    col = 0;
                    x_offset = 0.0;
                    row += 1;
                    row_glyph_start = frame_glyphs.glyphs.len();

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
                    // Flush ligature run before tab
                    flush_run(&self.run_buf, frame_glyphs, ligatures);
                    self.run_buf.clear();

                    // Tab: advance to next tab stop (column-based, pixel width uses space_w)
                    let tab_w = params.tab_width.max(1);
                    let next_tab = ((col / tab_w) + 1) * tab_w;
                    let spaces = (next_tab - col).min(cols - col);
                    let tab_pixel_w = spaces as f32 * face_space_w;

                    // Render tab as stretch glyph (use face bg)
                    let gx = content_x + x_offset;
                    let gy = row_y[row as usize];
                    Self::add_stretch_for_face(&self.face_data, frame_glyphs, gx, gy, tab_pixel_w, char_h, face_bg, self.face_data.face_id, false);

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
                        // Bidi reorder before advancing to next row
                        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
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
                                    row_glyph_start = frame_glyphs.glyphs.len();
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
                            row_glyph_start = frame_glyphs.glyphs.len();
                            if (row as usize) < row_continuation.len() {
                                row_continuation[row as usize] = true;
                            }
                            wrap_has_break = false;
                            if !params.wrap_prefix.is_empty() { need_prefix = 2; }
                        }
                    }
                }
                '\r' => {
                    // Flush ligature run before carriage return
                    flush_run(&self.run_buf, frame_glyphs, ligatures);
                    self.run_buf.clear();

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
                        // Bidi reorder before advancing to next row
                        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                        // Skip to next \n
                        while byte_idx < bytes_read as usize {
                            let (sch, slen) = decode_utf8(&text[byte_idx..]);
                            byte_idx += slen;
                            charpos += 1;
                            if sch == '\n' {
                                col = 0;
                                x_offset = 0.0;
                                row += 1;
                                row_glyph_start = frame_glyphs.glyphs.len();
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
                    // Flush ligature run before control char
                    flush_run(&self.run_buf, frame_glyphs, ligatures);
                    self.run_buf.clear();

                    // Control character: display as ^X (2 columns)
                    // DEL (0x7F) displays as ^?
                    // Use escape-glyph face for control char display
                    let escape_fg = Color::from_pixel(params.escape_glyph_fg);
                    frame_glyphs.set_face(
                        0, escape_fg, Some(face_bg),
                        400, false, 0, None, 0, None, 0, None,
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
                        // Bidi reorder before advancing to next row (control char overflow)
                        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                        if params.truncate_lines {
                            while byte_idx < bytes_read as usize {
                                let (c, l) = decode_utf8(&text[byte_idx..]);
                                byte_idx += l;
                                charpos += 1;
                                if c == '\n' {
                                    col = 0;
                                    x_offset = 0.0;
                                    row += 1;
                                    row_glyph_start = frame_glyphs.glyphs.len();
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
                            row_glyph_start = frame_glyphs.glyphs.len();
                            wrap_has_break = false;
                        }
                    }
                    // Restore text face after escape-glyph
                    if current_face_id >= 0 {
                        self.apply_face(&self.face_data, frame, frame_glyphs);
                    }
                }
                _ => {
                    // Non-breaking space and soft hyphen highlighting
                    if params.nobreak_char_display > 0 && (ch == '\u{00A0}' || ch == '\u{00AD}') {
                        // Flush ligature run before nobreak char special handling
                        flush_run(&self.run_buf, frame_glyphs, ligatures);
                        self.run_buf.clear();
                        let nb_fg = Color::from_pixel(params.nobreak_char_fg);
                        frame_glyphs.set_face(
                            0, nb_fg, Some(face_bg),
                            400, false, 0, None, 0, None, 0, None,
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
                            self.apply_face(&self.face_data, frame, frame_glyphs);
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
                        // Flush ligature run before grapheme cluster (emoji/ZWJ)
                        flush_run(&self.run_buf, frame_glyphs, ligatures);
                        self.run_buf.clear();
                        // Multi-codepoint grapheme cluster (emoji ZWJ, combining marks, etc.)
                        // Advance past the extra characters consumed
                        byte_idx += cluster_extra_bytes;
                        charpos += cluster_extra_chars as i64;

                        // Determine width: composed emoji are 2 columns wide
                        let char_cols = if is_wide_char(ch) { 2 } else { 1 };
                        let glyph_w = char_cols as f32 * char_w;

                        if x_offset + glyph_w > avail_width {
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
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
                                        row_glyph_start = frame_glyphs.glyphs.len();
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
                                row_glyph_start = frame_glyphs.glyphs.len();
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
                        // Flush ligature run before combining mark
                        flush_run(&self.run_buf, frame_glyphs, ligatures);
                        self.run_buf.clear();
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
                        // Flush ligature run before glyphless handling
                        flush_run(&self.run_buf, frame_glyphs, ligatures);
                        self.run_buf.clear();
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
                                400, false, 0, None, 0, None, 0, None,
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
                                self.apply_face(&self.face_data, frame, frame_glyphs);
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
                    let advance = if overstrike {
                        // Overstrike: Emacs couldn't find bold variant, kept
                        // regular font. Use default monospace width for grid
                        // alignment (matching official Emacs behavior).
                        char_cols as f32 * char_w
                    } else {
                        let face_id = self.face_data.face_id;
                        let font_size = self.face_data.font_size;
                        let face_char_w = self.face_data.font_char_width;
                        let font_family = if !self.face_data.font_family.is_null() {
                            CStr::from_ptr(self.face_data.font_family).to_str().unwrap_or("")
                        } else {
                            ""
                        };
                        let font_weight = self.face_data.font_weight as u16;
                        let font_italic = self.face_data.italic != 0;
                        char_advance(
                            &mut self.ascii_width_cache,
                            &mut self.font_metrics,
                            ch, char_cols, char_w,
                            face_id, font_size, face_char_w, window,
                            font_family, font_weight, font_italic,
                        )
                    };

                    if x_offset + advance > avail_width {
                        // Flush ligature run before line wrap/truncation
                        flush_run(&self.run_buf, frame_glyphs, ligatures);
                        self.run_buf.clear();

                        // Line full
                        if params.truncate_lines {
                            // Bidi reorder this completed row before truncation
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
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
                                    row_glyph_start = frame_glyphs.glyphs.len();
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
                                Self::add_stretch_for_face(
                                    &self.face_data, frame_glyphs,
                                    gx, gy, fill_w, char_h,
                                    face_bg, self.face_data.face_id, false,
                                );
                            }
                            // Bidi reorder after word-wrap truncation (re-reorder the truncated glyphs)
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                            if (row as usize) < row_continued.len() {
                                row_continued[row as usize] = true;
                            }
                            // Rewind position to the break
                            byte_idx = wrap_break_byte_idx;
                            charpos = wrap_break_charpos;
                            // Record hit-test row (word-wrap break)
                            if (row as usize) < row_y.len() {
                                hit_rows.push(HitRow {
                                    y_start: row_y[row as usize],
                                    y_end: row_y[row as usize] + row_max_height,
                                    charpos_start: hit_row_charpos_start,
                                    charpos_end: charpos,
                                });
                                hit_row_charpos_start = charpos;
                            }
                            // Force face re-check since we rewound
                            current_face_id = -1;
                            col = 0;
                            x_offset = 0.0;
                            row += 1;
                            row_glyph_start = frame_glyphs.glyphs.len();
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
                            // Bidi reorder this completed row before char-wrap
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                            // Character wrap: fill remaining space
                            let remaining = avail_width - x_offset;
                            if remaining > 0.0 {
                                let gx = content_x + x_offset;
                                let gy = row_y[row as usize];
                                Self::add_stretch_for_face(&self.face_data, frame_glyphs, gx, gy, remaining, char_h, face_bg, self.face_data.face_id, false);
                            }
                            if (row as usize) < row_continued.len() {
                                row_continued[row as usize] = true;
                            }
                            // Record hit-test row (char-wrap break)
                            if (row as usize) < row_y.len() {
                                hit_rows.push(HitRow {
                                    y_start: row_y[row as usize],
                                    y_end: row_y[row as usize] + row_max_height,
                                    charpos_start: hit_row_charpos_start,
                                    charpos_end: charpos,
                                });
                                hit_row_charpos_start = charpos;
                            }
                            col = 0;
                            x_offset = 0.0;
                            row += 1;
                            row_glyph_start = frame_glyphs.glyphs.len();
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

                    // Track per-row max height for variable-height faces
                    if face_h > row_max_height {
                        row_max_height = face_h;
                    }
                    if face_ascent > row_max_ascent {
                        row_max_ascent = face_ascent;
                    }

                    // Spaces break ligature runs (they never ligate) and serve
                    // as word-wrap breakpoints. Flush and emit individually.
                    if ch == ' ' {
                        flush_run(&self.run_buf, frame_glyphs, ligatures);
                        self.run_buf.clear();

                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize] + raise_y_offset;
                        if height_scale > 0.0 && height_scale != 1.0 {
                            let orig_size = frame_glyphs.font_size();
                            frame_glyphs.set_font_size(orig_size * height_scale);
                            frame_glyphs.add_char(ch, gx, gy, advance, face_h, face_ascent, false);
                            frame_glyphs.set_font_size(orig_size);
                        } else {
                            frame_glyphs.add_char(ch, gx, gy, advance, face_h, face_ascent, false);
                        }
                    } else if ligatures {
                        // Accumulate into ligature run
                        let gy = row_y[row as usize] + raise_y_offset;
                        if self.run_buf.is_empty() {
                            let gx = content_x + x_offset;
                            self.run_buf.start(gx, gy, face_h, face_ascent,
                                self.face_data.face_id, false, height_scale);
                        }
                        self.run_buf.push(ch, advance);

                        // Flush at max run length to limit texture sizes
                        if self.run_buf.len() >= MAX_LIGATURE_RUN_LEN {
                            flush_run(&self.run_buf, frame_glyphs, ligatures);
                            self.run_buf.clear();
                        }
                    } else {
                        // Ligatures disabled: emit directly
                        let gx = content_x + x_offset;
                        let gy = row_y[row as usize] + raise_y_offset;
                        if height_scale > 0.0 && height_scale != 1.0 {
                            let orig_size = frame_glyphs.font_size();
                            frame_glyphs.set_font_size(orig_size * height_scale);
                            frame_glyphs.add_char(ch, gx, gy, advance, face_h, face_ascent, false);
                            frame_glyphs.set_font_size(orig_size);
                        } else {
                            frame_glyphs.add_char(ch, gx, gy, advance, face_h, face_ascent, false);
                        }
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
                        // Flush ligature run at word-wrap boundary so truncate()
                        // never cuts inside a composed glyph
                        flush_run(&self.run_buf, frame_glyphs, ligatures);
                        self.run_buf.clear();

                        wrap_break_col = col;
                        wrap_break_x = x_offset;
                        wrap_break_byte_idx = byte_idx;
                        wrap_break_charpos = charpos;
                        wrap_break_glyph_count = frame_glyphs.glyphs.len();
                        wrap_has_break = true;
                    }
                }
            }

            // Flush ligature run only if we have overlay after-strings to render
            if overlay_after_len > 0 {
                flush_run(&self.run_buf, frame_glyphs, ligatures);
                self.run_buf.clear();
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

                let cursor_style = if params.selected {
                    CursorStyle::from_type(params.cursor_type, params.cursor_bar_width)
                } else if params.cursor_in_non_selected {
                    Some(CursorStyle::Hollow)
                } else {
                    None
                };

                if let Some(style) = cursor_style {
                    frame_glyphs.add_cursor(
                        params.window_id as i32,
                        cursor_px,
                        cursor_y,
                        cursor_face_w,
                        face_h,
                        style,
                        face_fg,
                    );

                    if matches!(style, CursorStyle::FilledBox) {
                        frame_glyphs.set_cursor_inverse(
                            cursor_px,
                            cursor_y,
                            cursor_face_w,
                            face_h,
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
                let after_align_entries = if overlay_after_naligns > 0 {
                    parse_overlay_align_entries(&overlay_after_buf, overlay_after_len as usize, overlay_after_nruns, overlay_after_naligns)
                } else {
                    Vec::new()
                };
                let mut acurrent_align = 0usize;

                // Apply overlay face for after-string if no per-char runs
                if !after_has_runs && overlay_after_face.face_id != 0 {
                    self.apply_face(&overlay_after_face, frame, frame_glyphs);
                }

                let astr = &overlay_after_buf[..overlay_after_len as usize];
                let mut ai = 0usize;
                let mut acurrent_run = 0usize;
                while ai < astr.len() && row < max_rows {
                    // Check for align-to entry at this byte offset
                    if acurrent_align < after_align_entries.len()
                        && ai == after_align_entries[acurrent_align].byte_offset as usize
                    {
                        let target_x = after_align_entries[acurrent_align].align_to_cols * char_w;
                        if target_x > x_offset {
                            let gx = content_x + x_offset;
                            let gy = row_y[row as usize];
                            let stretch_w = target_x - x_offset;
                            Self::add_stretch_for_face(
                                &self.face_data, frame_glyphs,
                                gx, gy, stretch_w, char_h,
                                face_bg, self.face_data.face_id, false,
                            );
                            col = after_align_entries[acurrent_align].align_to_cols.ceil() as i32;
                            x_offset = target_x;
                        }
                        acurrent_align += 1;
                        let (_ach, alen) = decode_utf8(&astr[ai..]);
                        ai += alen;
                        continue;
                    }

                    // Apply face run if needed
                    if after_has_runs && acurrent_run < after_face_runs.len() {
                        acurrent_run = apply_overlay_face_run(
                            &after_face_runs, ai, acurrent_run, frame_glyphs,
                        );
                    }

                    let (ach, alen) = decode_utf8(&astr[ai..]);
                    ai += alen;
                    if ach == '\n' {
                        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        row_glyph_start = frame_glyphs.glyphs.len();
                        if row >= max_rows { break; }
                        continue;
                    }
                    if ach == '\r' { continue; }

                    let achar_cols = if is_wide_char(ach) { 2 } else { 1 };
                    let a_advance = achar_cols as f32 * char_w;
                    if x_offset + a_advance > avail_width {
                        if params.truncate_lines {
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                            while ai < astr.len() {
                                let (sc, sl) = decode_utf8(&astr[ai..]);
                                ai += sl;
                                if sc == '\n' {
                                    col = 0;
                                    x_offset = 0.0;
                                    row += 1;
                                    row_glyph_start = frame_glyphs.glyphs.len();
                                    break;
                                }
                            }
                            if row >= max_rows { break; }
                            continue;
                        }
                        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        row_glyph_start = frame_glyphs.glyphs.len();
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
                    self.apply_face(&self.face_data, frame, frame_glyphs);
                }
            }

            window_end_charpos = charpos;
        }

        // Flush any remaining ligature run at end of buffer
        flush_run(&self.run_buf, frame_glyphs, ligatures);
        self.run_buf.clear();

        // Place cursor before end-of-buffer overlay strings.
        // When point is at end-of-buffer and overlays have after-strings there
        // (e.g., fido-vertical-mode completions), the cursor must be placed
        // BEFORE the overlay content is rendered.
        if !cursor_placed && params.point >= window_start
            && charpos >= params.point
        {
            let clamped_row = row.min(max_rows - 1);
            let cursor_y = row_y[clamped_row as usize];

            // Safety: don't place cursor past text area
            if cursor_y < text_y_limit {
                cursor_col = col;
                cursor_x = x_offset;
                cursor_row = clamped_row;
                let cursor_px = content_x + x_offset;

                let cursor_face_w = if self.face_data.font_char_width > 0.0 {
                    self.face_data.font_char_width
                } else {
                    char_w
                };

                let cursor_style = if params.selected {
                    CursorStyle::from_type(params.cursor_type, params.cursor_bar_width)
                } else if params.cursor_in_non_selected {
                    Some(CursorStyle::Hollow)
                } else {
                    None
                };

                if let Some(style) = cursor_style {
                    frame_glyphs.add_cursor(
                        params.window_id as i32,
                        cursor_px,
                        cursor_y,
                        cursor_face_w,
                        face_h,
                        style,
                        face_fg,
                    );

                    if matches!(style, CursorStyle::FilledBox) {
                        frame_glyphs.set_cursor_inverse(
                            cursor_px,
                            cursor_y,
                            cursor_face_w,
                            face_h,
                            face_fg,
                            face_bg,
                        );
                    }
                }

                cursor_placed = true;
            }
            // If cursor_y >= text_y_limit, skip placement — forward scroll will fix next frame
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
            let mut eob_left_fringe_bitmap: i32 = 0;
            let mut eob_left_fringe_fg: u32 = 0;
            let mut eob_left_fringe_bg: u32 = 0;
            let mut eob_right_fringe_bitmap: i32 = 0;
            let mut eob_right_fringe_fg: u32 = 0;
            let mut eob_right_fringe_bg: u32 = 0;
            let mut eob_before_naligns: i32 = 0;
            let mut eob_after_naligns: i32 = 0;
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
                &mut eob_left_fringe_bitmap,
                &mut eob_left_fringe_fg,
                &mut eob_left_fringe_bg,
                &mut eob_right_fringe_bitmap,
                &mut eob_right_fringe_fg,
                &mut eob_right_fringe_bg,
                &mut eob_before_naligns,
                &mut eob_after_naligns,
            );

            // Store fringe bitmaps from overlay display properties at EOB
            let r = row as usize;
            if eob_left_fringe_bitmap > 0 && r < row_left_fringe.len() {
                row_left_fringe[r] = (eob_left_fringe_bitmap, eob_left_fringe_fg, eob_left_fringe_bg);
            }
            if eob_right_fringe_bitmap > 0 && r < row_right_fringe.len() {
                row_right_fringe[r] = (eob_right_fringe_bitmap, eob_right_fringe_fg, eob_right_fringe_bg);
            }

            // Render before-string at end-of-buffer
            if eob_before_len > 0 {
                let eob_before_has_runs = overlay_before_nruns > 0;
                let eob_before_face_runs = if eob_before_has_runs {
                    parse_overlay_face_runs(&overlay_before_buf, eob_before_len as usize, overlay_before_nruns)
                } else {
                    Vec::new()
                };
                let eob_before_align_entries = if eob_before_naligns > 0 {
                    parse_overlay_align_entries(&overlay_before_buf, eob_before_len as usize, overlay_before_nruns, eob_before_naligns)
                } else {
                    Vec::new()
                };
                let mut eob_bcurrent_align = 0usize;

                if !eob_before_has_runs && eob_before_face.face_id != 0 {
                    self.apply_face(&eob_before_face, frame, frame_glyphs);
                }
                let bstr = &overlay_before_buf[..eob_before_len as usize];
                let mut bi = 0usize;
                let mut bcurrent_run = 0usize;
                while bi < bstr.len() && row < max_rows {
                    // Check for align-to entry at this byte offset
                    if eob_bcurrent_align < eob_before_align_entries.len()
                        && bi == eob_before_align_entries[eob_bcurrent_align].byte_offset as usize
                    {
                        let target_x = eob_before_align_entries[eob_bcurrent_align].align_to_cols * char_w;
                        if target_x > x_offset {
                            let gx = content_x + x_offset;
                            let gy = row_y[row as usize];
                            let stretch_w = target_x - x_offset;
                            Self::add_stretch_for_face(
                                &self.face_data, frame_glyphs,
                                gx, gy, stretch_w, char_h,
                                face_bg, self.face_data.face_id, false,
                            );
                            col = eob_before_align_entries[eob_bcurrent_align].align_to_cols.ceil() as i32;
                            x_offset = target_x;
                        }
                        eob_bcurrent_align += 1;
                        let (_bch, blen) = decode_utf8(&bstr[bi..]);
                        bi += blen;
                        continue;
                    }

                    if eob_before_has_runs && bcurrent_run < eob_before_face_runs.len() {
                        bcurrent_run = apply_overlay_face_run(
                            &eob_before_face_runs, bi, bcurrent_run, frame_glyphs,
                        );
                    }

                    let (bch, blen) = decode_utf8(&bstr[bi..]);
                    bi += blen;
                    if bch == '\n' {
                        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        row_glyph_start = frame_glyphs.glyphs.len();
                        if row >= max_rows { break; }
                        continue;
                    }
                    if bch == '\r' { continue; }
                    let bchar_cols = if is_wide_char(bch) { 2 } else { 1 };
                    let b_advance = bchar_cols as f32 * char_w;
                    if x_offset + b_advance > avail_width {
                        if params.truncate_lines {
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                            while bi < bstr.len() {
                                let (sc, sl) = decode_utf8(&bstr[bi..]);
                                bi += sl;
                                if sc == '\n' {
                                    col = 0;
                                    x_offset = 0.0;
                                    row += 1;
                                    row_glyph_start = frame_glyphs.glyphs.len();
                                    break;
                                }
                            }
                            if row >= max_rows { break; }
                            continue;
                        }
                        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        row_glyph_start = frame_glyphs.glyphs.len();
                        if row >= max_rows { break; }
                    }
                    let gx = content_x + x_offset;
                    let gy = row_y[row as usize];
                    frame_glyphs.add_char(bch, gx, gy, b_advance, char_h, ascent, false);
                    col += bchar_cols;
                    x_offset += b_advance;
                }
                if (eob_before_has_runs || eob_before_face.face_id != 0) && current_face_id >= 0 {
                    self.apply_face(&self.face_data, frame, frame_glyphs);
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
                let eob_after_align_entries = if eob_after_naligns > 0 {
                    parse_overlay_align_entries(&overlay_after_buf, overlay_after_len as usize, overlay_after_nruns, eob_after_naligns)
                } else {
                    Vec::new()
                };
                let mut eob_acurrent_align = 0usize;

                if !eob_after_has_runs && overlay_after_face.face_id != 0 {
                    self.apply_face(&overlay_after_face, frame, frame_glyphs);
                }
                let astr = &overlay_after_buf[..overlay_after_len as usize];
                let mut ai = 0usize;
                let mut acurrent_run = 0usize;
                while ai < astr.len() && row < max_rows {
                    // Check for align-to entry at this byte offset
                    if eob_acurrent_align < eob_after_align_entries.len()
                        && ai == eob_after_align_entries[eob_acurrent_align].byte_offset as usize
                    {
                        let target_x = eob_after_align_entries[eob_acurrent_align].align_to_cols * char_w;
                        if target_x > x_offset {
                            let gx = content_x + x_offset;
                            let gy = row_y[row as usize];
                            let stretch_w = target_x - x_offset;
                            Self::add_stretch_for_face(
                                &self.face_data, frame_glyphs,
                                gx, gy, stretch_w, char_h,
                                face_bg, self.face_data.face_id, false,
                            );
                            col = eob_after_align_entries[eob_acurrent_align].align_to_cols.ceil() as i32;
                            x_offset = target_x;
                        }
                        eob_acurrent_align += 1;
                        let (_ach, alen) = decode_utf8(&astr[ai..]);
                        ai += alen;
                        continue;
                    }

                    if eob_after_has_runs && acurrent_run < eob_after_face_runs.len() {
                        acurrent_run = apply_overlay_face_run(
                            &eob_after_face_runs, ai, acurrent_run, frame_glyphs,
                        );
                    }

                    let (ach, alen) = decode_utf8(&astr[ai..]);
                    ai += alen;
                    if ach == '\n' {
                        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        row_glyph_start = frame_glyphs.glyphs.len();
                        if row >= max_rows { break; }
                        continue;
                    }
                    if ach == '\r' { continue; }
                    let achar_cols = if is_wide_char(ach) { 2 } else { 1 };
                    let a_advance = achar_cols as f32 * char_w;
                    if x_offset + a_advance > avail_width {
                        if params.truncate_lines {
                            reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                            while ai < astr.len() {
                                let (sc, sl) = decode_utf8(&astr[ai..]);
                                ai += sl;
                                if sc == '\n' {
                                    col = 0;
                                    x_offset = 0.0;
                                    row += 1;
                                    row_glyph_start = frame_glyphs.glyphs.len();
                                    break;
                                }
                            }
                            if row >= max_rows { break; }
                            continue;
                        }
                        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);
                        col = 0;
                        x_offset = 0.0;
                        row += 1;
                        row_glyph_start = frame_glyphs.glyphs.len();
                        if row >= max_rows { break; }
                    }
                    let gx = content_x + x_offset;
                    let gy = row_y[row as usize];
                    frame_glyphs.add_char(ach, gx, gy, a_advance, char_h, ascent, false);
                    col += achar_cols;
                    x_offset += a_advance;
                }
                if (eob_after_has_runs || overlay_after_face.face_id != 0) && current_face_id >= 0 {
                    self.apply_face(&self.face_data, frame, frame_glyphs);
                }

            }
        }

        // Flush any remaining ligature run and bidi reorder the last row
        flush_run(&self.run_buf, frame_glyphs, ligatures);
        self.run_buf.clear();
        reorder_row_bidi(frame_glyphs, row_glyph_start, frame_glyphs.glyphs.len(), content_x);

        // Fill rest of last line with :extend background if applicable
        // (handles end-of-buffer without trailing newline)
        if row < max_rows && x_offset > 0.0 {
            let remaining = avail_width - x_offset;
            if remaining > 0.0 {
                let gx = content_x + x_offset;
                let gy = row_y[row as usize];
                let fill_bg = if self.face_data.extend != 0 { face_bg } else { default_bg };
                let fill_face = if self.face_data.extend != 0 { self.face_data.face_id } else { 0 };
                if self.face_data.extend != 0 {
                    Self::add_stretch_for_face(&self.face_data, frame_glyphs, gx, gy, remaining, char_h, fill_bg, fill_face, false);
                } else {
                    frame_glyphs.add_stretch(gx, gy, remaining, char_h, fill_bg, fill_face, false);
                }
            }
        }

        // Close any remaining box face region at end of text.
        // Borders are rendered by the renderer's box span detection.
        if box_active {
            box_active = false;
        }

        // If cursor wasn't placed (point is past visible content), place at end
        if !cursor_placed && params.point >= window_start {
            let clamped_row = row.min(max_rows - 1);
            let cursor_y = row_y[clamped_row as usize];

            // Safety: don't place cursor past text area
            if cursor_y < text_y_limit {
                cursor_col = col;
                cursor_row = clamped_row;
                cursor_x = x_offset;
                let cursor_px = content_x + x_offset;

                let cursor_face_w = if self.face_data.font_char_width > 0.0 {
                    self.face_data.font_char_width
                } else {
                    char_w
                };

                let cursor_style = if params.selected {
                    CursorStyle::from_type(params.cursor_type, params.cursor_bar_width)
                } else if params.cursor_in_non_selected {
                    Some(CursorStyle::Hollow)
                } else {
                    None
                };

                if let Some(style) = cursor_style {
                    frame_glyphs.add_cursor(
                        params.window_id as i32,
                        cursor_px,
                        cursor_y,
                        cursor_face_w,
                        face_h,
                        style,
                        face_fg,
                    );

                    if matches!(style, CursorStyle::FilledBox) {
                        frame_glyphs.set_cursor_inverse(
                            cursor_px,
                            cursor_y,
                            cursor_face_w,
                            face_h,
                            face_fg,
                            face_bg,
                        );
                    }
                }
            }
            // If cursor_y >= text_y_limit, skip — forward scroll will fix next frame
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
                400, false, 0, None, 0, None, 0, None,
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
                400, false, 0, None, 0, None, 0, None,
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
                frame,
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
                frame,
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
                frame,
                frame_glyphs,
                StatusLineKind::ModeLine,
            );
        }

        // Record last hit-test row (end of visible text)
        if row < max_rows && (row as usize) < row_y.len() && charpos > hit_row_charpos_start {
            hit_rows.push(HitRow {
                y_start: row_y[row as usize],
                y_end: row_y[row as usize] + row_max_height,
                charpos_start: hit_row_charpos_start,
                charpos_end: charpos,
            });
        }

        // Store hit-test data for this window
        self.hit_data.push(WindowHitData {
            window_id: params.window_id,
            content_x,
            char_w,
            rows: hit_rows,
        });

        // Write layout results back to Emacs
        neomacs_layout_set_window_end(
            wp.window_ptr,
            window_end_charpos,
            row.min(max_rows - 1),
        );

        // Set cursor position for Emacs (needed for recenter, scroll, etc.)
        // Ensure cursor_row is valid and within text area
        if cursor_row < max_rows && row_y[cursor_row as usize] < text_y_limit {
            neomacs_layout_set_cursor(
                wp.window_ptr,
                (content_x + cursor_x) as i32,
                (row_y[cursor_row as usize]) as i32,
                cursor_col,
                cursor_row,
            );
        } else {
            // Set cursor at row 0 as fallback — scroll will fix next frame
            neomacs_layout_set_cursor(
                wp.window_ptr,
                content_x as i32,
                text_y as i32,
                0,
                0,
            );
        }
    }
}

/// Get the advance width for a character in a specific face.
///
/// Standalone function to avoid borrow conflicts with `LayoutEngine::text_buf`.
///
/// Supports two measurement backends:
/// - **C FFI** (default): Uses `neomacs_layout_fill_ascii_widths()` / `neomacs_layout_char_width()`
///   which read from Emacs C font metrics (fontconfig/freetype).
/// - **Cosmic-text**: Uses `FontMetricsService` for measurement, matching the render thread's
///   font resolution exactly. Eliminates width mismatches between layout and rendering.
///
/// The backend is selected by `font_metrics_svc` being Some (cosmic) or None (C FFI).
unsafe fn char_advance(
    ascii_width_cache: &mut std::collections::HashMap<(u32, i32), [f32; 128]>,
    font_metrics_svc: &mut Option<FontMetricsService>,
    ch: char,
    char_cols: i32,
    char_w: f32,
    face_id: u32,
    font_size: i32,
    face_char_w: f32,
    window: EmacsWindow,
    font_family: &str,
    font_weight: u16,
    font_italic: bool,
) -> f32 {
    // Use the face-specific character width when available (handles
    // faces with :height attribute that use a differently-sized font).
    let face_w = if face_char_w > 0.0 { face_char_w } else { char_w };

    // Cosmic-text path: use FontMetricsService for measurement
    if let Some(ref mut svc) = font_metrics_svc {
        let font_size_f = font_size as f32;
        return svc.char_width(ch, font_family, font_weight, font_italic, font_size_f);
    }

    // C FFI path (default): use pre-warmed Emacs font metrics
    let cp = ch as u32;
    if cp < 128 {
        // ASCII: use cached widths from pre-warmed font metrics
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

    // Non-ASCII: query individually via text_extents()
    let w = neomacs_layout_char_width(window, cp as c_int, face_id as c_int);
    if w > 0.0 { w } else { char_cols as f32 * face_w }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::frame_glyphs::FrameGlyph;

    #[test]
    fn test_ligature_run_buffer_new() {
        let buf = LigatureRunBuffer::new();

        // All fields should be zeroed/empty
        assert_eq!(buf.chars.len(), 0);
        assert_eq!(buf.advances.len(), 0);
        assert_eq!(buf.start_x, 0.0);
        assert_eq!(buf.start_y, 0.0);
        assert_eq!(buf.face_h, 0.0);
        assert_eq!(buf.face_ascent, 0.0);
        assert_eq!(buf.face_id, 0);
        assert_eq!(buf.total_advance, 0.0);
        assert_eq!(buf.is_overlay, false);
        assert_eq!(buf.height_scale, 0.0);

        // Vectors should be pre-allocated
        assert!(buf.chars.capacity() >= MAX_LIGATURE_RUN_LEN);
        assert!(buf.advances.capacity() >= MAX_LIGATURE_RUN_LEN);
    }

    #[test]
    fn test_ligature_run_buffer_is_empty_len() {
        let mut buf = LigatureRunBuffer::new();

        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);

        buf.push('a', 8.0);

        assert!(!buf.is_empty());
        assert_eq!(buf.len(), 1);

        buf.push('b', 8.0);

        assert!(!buf.is_empty());
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn test_ligature_run_buffer_push() {
        let mut buf = LigatureRunBuffer::new();

        buf.push('h', 8.0);
        assert_eq!(buf.chars, vec!['h']);
        assert_eq!(buf.advances, vec![8.0]);
        assert_eq!(buf.total_advance, 8.0);

        buf.push('e', 8.0);
        assert_eq!(buf.chars, vec!['h', 'e']);
        assert_eq!(buf.advances, vec![8.0, 8.0]);
        assert_eq!(buf.total_advance, 16.0);

        buf.push('l', 7.5);
        assert_eq!(buf.chars, vec!['h', 'e', 'l']);
        assert_eq!(buf.advances, vec![8.0, 8.0, 7.5]);
        assert_eq!(buf.total_advance, 23.5);
    }

    #[test]
    fn test_ligature_run_buffer_clear() {
        let mut buf = LigatureRunBuffer::new();

        buf.push('a', 8.0);
        buf.push('b', 8.0);
        buf.start_x = 100.0;
        buf.start_y = 200.0;
        buf.face_h = 16.0;
        buf.face_ascent = 12.0;
        buf.face_id = 42;
        buf.is_overlay = true;
        buf.height_scale = 1.5;

        buf.clear();

        // Vectors and total_advance cleared
        assert_eq!(buf.chars.len(), 0);
        assert_eq!(buf.advances.len(), 0);
        assert_eq!(buf.total_advance, 0.0);

        // Position/face fields NOT cleared
        assert_eq!(buf.start_x, 100.0);
        assert_eq!(buf.start_y, 200.0);
        assert_eq!(buf.face_h, 16.0);
        assert_eq!(buf.face_ascent, 12.0);
        assert_eq!(buf.face_id, 42);
        assert_eq!(buf.is_overlay, true);
        assert_eq!(buf.height_scale, 1.5);
    }

    #[test]
    fn test_ligature_run_buffer_start() {
        let mut buf = LigatureRunBuffer::new();

        buf.push('x', 10.0);
        buf.start_x = 999.0;

        buf.start(50.0, 60.0, 20.0, 15.0, 5, true, 1.2);

        // Clears chars/advances/total_advance
        assert_eq!(buf.chars.len(), 0);
        assert_eq!(buf.advances.len(), 0);
        assert_eq!(buf.total_advance, 0.0);

        // Sets all position/face params
        assert_eq!(buf.start_x, 50.0);
        assert_eq!(buf.start_y, 60.0);
        assert_eq!(buf.face_h, 20.0);
        assert_eq!(buf.face_ascent, 15.0);
        assert_eq!(buf.face_id, 5);
        assert_eq!(buf.is_overlay, true);
        assert_eq!(buf.height_scale, 1.2);
    }

    #[test]
    fn test_max_ligature_run_len_constant() {
        assert_eq!(MAX_LIGATURE_RUN_LEN, 64);
    }

    #[test]
    fn test_flush_run_empty() {
        let run = LigatureRunBuffer::new();
        let mut frame_glyphs = FrameGlyphBuffer::new();

        flush_run(&run, &mut frame_glyphs, true);

        // No glyphs added
        assert_eq!(frame_glyphs.glyphs.len(), 0);
    }

    #[test]
    fn test_flush_run_single_char_ligatures_true() {
        let mut run = LigatureRunBuffer::new();
        run.start(10.0, 20.0, 16.0, 12.0, 1, false, 0.0);
        run.push('a', 8.0);

        let mut frame_glyphs = FrameGlyphBuffer::new();
        flush_run(&run, &mut frame_glyphs, true);

        // Single char emits as individual char, not composed
        assert_eq!(frame_glyphs.glyphs.len(), 1);
        match &frame_glyphs.glyphs[0] {
            FrameGlyph::Char { char: ch, composed, x, y, width, height, ascent, is_overlay, .. } => {
                assert_eq!(*ch, 'a');
                assert_eq!(*composed, None);
                assert_eq!(*x, 10.0);
                assert_eq!(*y, 20.0);
                assert_eq!(*width, 8.0);
                assert_eq!(*height, 16.0);
                assert_eq!(*ascent, 12.0);
                assert_eq!(*is_overlay, false);
            }
            _ => panic!("Expected Char glyph"),
        }
    }

    #[test]
    fn test_flush_run_single_char_ligatures_false() {
        let mut run = LigatureRunBuffer::new();
        run.start(100.0, 200.0, 18.0, 14.0, 2, true, 0.0);
        run.push('x', 9.0);

        let mut frame_glyphs = FrameGlyphBuffer::new();
        flush_run(&run, &mut frame_glyphs, false);

        // Single char emits as individual char
        assert_eq!(frame_glyphs.glyphs.len(), 1);
        match &frame_glyphs.glyphs[0] {
            FrameGlyph::Char { char: ch, composed, x, y, width, is_overlay, .. } => {
                assert_eq!(*ch, 'x');
                assert_eq!(*composed, None);
                assert_eq!(*x, 100.0);
                assert_eq!(*y, 200.0);
                assert_eq!(*width, 9.0);
                assert_eq!(*is_overlay, true);
            }
            _ => panic!("Expected Char glyph"),
        }
    }

    #[test]
    fn test_flush_run_multiple_chars_ligatures_false() {
        let mut run = LigatureRunBuffer::new();
        run.start(50.0, 60.0, 16.0, 12.0, 1, false, 0.0);
        run.push('f', 6.0);
        run.push('i', 4.0);
        run.push('j', 4.0);

        let mut frame_glyphs = FrameGlyphBuffer::new();
        flush_run(&run, &mut frame_glyphs, false);

        // Emits individual chars with correct x positions
        assert_eq!(frame_glyphs.glyphs.len(), 3);

        match &frame_glyphs.glyphs[0] {
            FrameGlyph::Char { char: ch, x, width, .. } => {
                assert_eq!(*ch, 'f');
                assert_eq!(*x, 50.0);
                assert_eq!(*width, 6.0);
            }
            _ => panic!("Expected Char glyph"),
        }

        match &frame_glyphs.glyphs[1] {
            FrameGlyph::Char { char: ch, x, width, .. } => {
                assert_eq!(*ch, 'i');
                assert_eq!(*x, 56.0); // 50.0 + 6.0
                assert_eq!(*width, 4.0);
            }
            _ => panic!("Expected Char glyph"),
        }

        match &frame_glyphs.glyphs[2] {
            FrameGlyph::Char { char: ch, x, width, .. } => {
                assert_eq!(*ch, 'j');
                assert_eq!(*x, 60.0); // 56.0 + 4.0
                assert_eq!(*width, 4.0);
            }
            _ => panic!("Expected Char glyph"),
        }
    }

    #[test]
    fn test_flush_run_multiple_chars_ligatures_true() {
        // Use ligature-eligible chars (pure symbol run)
        let mut run = LigatureRunBuffer::new();
        run.start(10.0, 20.0, 16.0, 12.0, 1, false, 0.0);
        run.push('-', 6.0);
        run.push('>', 4.0);

        let mut frame_glyphs = FrameGlyphBuffer::new();
        flush_run(&run, &mut frame_glyphs, true);

        // Emits as composed glyph
        assert_eq!(frame_glyphs.glyphs.len(), 1);

        match &frame_glyphs.glyphs[0] {
            FrameGlyph::Char { char: ch, composed, x, y, width, height, ascent, is_overlay, .. } => {
                assert_eq!(*ch, '-'); // base char
                assert_eq!(composed.as_ref().map(|s: &Box<str>| s.as_ref()), Some("->"));
                assert_eq!(*x, 10.0);
                assert_eq!(*y, 20.0);
                assert_eq!(*width, 10.0); // total_advance = 6.0 + 4.0
                assert_eq!(*height, 16.0);
                assert_eq!(*ascent, 12.0);
                assert_eq!(*is_overlay, false);
            }
            _ => panic!("Expected Char glyph"),
        }
    }

    #[test]
    fn test_flush_run_mixed_alpha_symbol_not_composed() {
        // Mixed alphanumeric+symbol runs should NOT compose (e.g., "arrow:")
        let mut run = LigatureRunBuffer::new();
        run.start(0.0, 0.0, 16.0, 12.0, 1, false, 0.0);
        run.push('f', 6.0);
        run.push('i', 4.0);

        let mut frame_glyphs = FrameGlyphBuffer::new();
        flush_run(&run, &mut frame_glyphs, true);

        // Should emit as individual chars, not composed
        assert_eq!(frame_glyphs.glyphs.len(), 2);
    }

    #[test]
    fn test_flush_run_height_scale_individual() {
        let mut run = LigatureRunBuffer::new();
        run.start(0.0, 0.0, 16.0, 12.0, 1, false, 1.5);
        run.push('a', 8.0);

        let mut frame_glyphs = FrameGlyphBuffer::new();
        frame_glyphs.set_font_size(14.0);

        flush_run(&run, &mut frame_glyphs, false);

        // Font size should be restored after flush
        assert_eq!(frame_glyphs.font_size(), 14.0);

        // Glyph should exist
        assert_eq!(frame_glyphs.glyphs.len(), 1);
    }

    #[test]
    fn test_flush_run_height_scale_composed() {
        // Use ligature-eligible chars for composed path
        let mut run = LigatureRunBuffer::new();
        run.start(0.0, 0.0, 16.0, 12.0, 1, false, 2.0);
        run.push('=', 6.0);
        run.push('>', 4.0);

        let mut frame_glyphs = FrameGlyphBuffer::new();
        frame_glyphs.set_font_size(14.0);

        flush_run(&run, &mut frame_glyphs, true);

        // Font size should be restored after flush
        assert_eq!(frame_glyphs.font_size(), 14.0);

        // Composed glyph should exist
        assert_eq!(frame_glyphs.glyphs.len(), 1);
    }

    #[test]
    fn test_is_ligature_char() {
        // Ligature-eligible characters
        for ch in ['-', '>', '<', '=', '!', '|', '&', '*', '+', '.', '/', ':', ';', '?', '@', '\\', '^', '~', '#', '$', '%'] {
            assert!(is_ligature_char(ch), "'{}' should be a ligature char", ch);
        }
        // Non-ligature characters
        for ch in ['a', 'Z', '0', '9', ' ', '\n', '\t', '(', ')', '[', ']', '{', '}', ',', '\'', '"'] {
            assert!(!is_ligature_char(ch), "'{}' should NOT be a ligature char", ch);
        }
    }

    #[test]
    fn test_run_is_pure_ligature() {
        // Pure symbol run
        let mut run = LigatureRunBuffer::new();
        run.start(0.0, 0.0, 16.0, 12.0, 1, false, 0.0);
        run.push('-', 8.0);
        run.push('>', 8.0);
        assert!(run_is_pure_ligature(&run));

        // Mixed run (alpha + symbol)
        let mut run2 = LigatureRunBuffer::new();
        run2.start(0.0, 0.0, 16.0, 12.0, 1, false, 0.0);
        run2.push('a', 8.0);
        run2.push(':', 8.0);
        assert!(!run_is_pure_ligature(&run2));

        // Pure alpha run
        let mut run3 = LigatureRunBuffer::new();
        run3.start(0.0, 0.0, 16.0, 12.0, 1, false, 0.0);
        run3.push('h', 8.0);
        run3.push('i', 8.0);
        assert!(!run_is_pure_ligature(&run3));
    }
}


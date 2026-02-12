//! FFI declarations for reading Emacs data structures.
//!
//! These functions are implemented in C (neomacsterm.c) and called by
//! the Rust layout engine to read buffer text, face data, and window geometry.
//!
//! Safety: All functions here are called on the Emacs thread during
//! redisplay_internal(), so they are thread-safe (Emacs is single-threaded).
//! The pointers are valid for the duration of the layout computation.

use std::ffi::{c_int, c_void};
use std::os::raw::c_char;

/// Opaque pointer to an Emacs frame (struct frame *)
pub type EmacsFrame = *mut std::ffi::c_void;
/// Opaque pointer to an Emacs window (struct window *)
pub type EmacsWindow = *mut std::ffi::c_void;
/// Opaque pointer to an Emacs buffer (struct buffer *)
pub type EmacsBuffer = *mut std::ffi::c_void;

extern "C" {
    // ========================================================================
    // Buffer text access
    // ========================================================================

    /// Convert character position to byte position in a buffer.
    /// Uses Emacs byte-charpos cache (O(log n)).
    /// Works directly on the buffer struct — no set_buffer_internal_1 needed.
    pub fn neomacs_buf_charpos_to_bytepos(
        buffer: EmacsBuffer,
        charpos: i64,
    ) -> i64;

    // Buffer bulk text copy is now handled directly in Rust via
    // emacs_types::gap_buffer_copy_text() — no FFI call needed.
    //
    // Buffer metadata (bounds, point, tab_width, etc.) is also read
    // directly from Emacs structs in Rust — see emacs_types.rs.

    // ========================================================================
    // Window geometry
    // ========================================================================

    // frame_window_count is now implemented directly in Rust
    // (see emacs_types::frame_window_count — uses direct struct access).

    /// Get window parameters for the Nth leaf window.
    /// Fills the WindowParamsFFI struct. Returns 0 on success, -1 on error.
    pub fn neomacs_layout_get_window_params(
        frame: EmacsFrame,
        window_index: c_int,
        params: *mut WindowParamsFFI,
    ) -> c_int;

    // ========================================================================
    // Face resolution
    // ========================================================================

    /// Get the resolved face at a buffer position for a given window.
    /// Returns face data in the FaceDataFFI struct.
    /// This calls face_at_buffer_position() internally.
    /// If next_check_out is non-null, writes the position where the face may change.
    pub fn neomacs_layout_face_at_pos(
        window: EmacsWindow,
        charpos: i64,
        face_out: *mut FaceDataFFI,
        next_check_out: *mut i64,
    ) -> c_int;

    /// Get the default face for a frame.
    pub fn neomacs_layout_default_face(
        frame: EmacsFrame,
        face_out: *mut FaceDataFFI,
    ) -> c_int;

    /// Get stipple bitmap data for a given bitmap ID.
    /// Returns 0 on success, -1 on failure.
    pub fn neomacs_layout_get_stipple_bitmap(
        frame: *mut c_void,
        bitmap_id: c_int,
        bits_out: *mut u8,
        bits_buf_len: c_int,
        width_out: *mut c_int,
        height_out: *mut c_int,
    ) -> c_int;

    /// Get the advance width of a single character in a specific face's font.
    /// Returns the pixel width, or -1.0 if unmeasurable.
    pub fn neomacs_layout_char_width(
        window: EmacsWindow,
        charcode: c_int,
        face_id: c_int,
    ) -> f32;

    /// Fill advance widths for ASCII characters (0-127) in a face's font.
    /// widths must point to an array of at least 128 floats.
    pub fn neomacs_layout_fill_ascii_widths(
        window: EmacsWindow,
        face_id: c_int,
        widths: *mut f32,
    );

    // ========================================================================
    // Writing layout results back to Emacs
    // ========================================================================

    /// Adjust window_start so that point is visible (backward scrolling).
    /// Returns the new window_start charpos.
    pub fn neomacs_layout_adjust_window_start(
        window: EmacsWindow,
        buffer: EmacsBuffer,
        point: i64,
        lines_above: c_int,
    ) -> i64;

    /// Set window_end_pos on an Emacs window (for window-end Lisp function).
    pub fn neomacs_layout_set_window_end(
        window: EmacsWindow,
        end_pos: i64,
        end_vpos: c_int,
    );

    /// Set cursor position on an Emacs window.
    pub fn neomacs_layout_set_cursor(
        window: EmacsWindow,
        x: c_int,
        y: c_int,
        hpos: c_int,
        vpos: c_int,
    );

    // ========================================================================
    // Fontification callback
    // ========================================================================

    /// Trigger fontification at a position (calls fontification-functions).
    /// Returns 1 if fontification happened, 0 if text was already fontified.
    pub fn neomacs_layout_ensure_fontified(
        buffer: EmacsBuffer,
        from: i64,
        to: i64,
    ) -> c_int;

    // ========================================================================
    // Invisible text
    // ========================================================================

    /// Check if text at charpos is invisible.
    /// Returns 0 = visible, 1 = invisible (hidden), 2 = invisible (ellipsis).
    /// If invisible, *next_visible_out is set to the next visible position.
    pub fn neomacs_layout_check_invisible(
        buffer: EmacsBuffer,
        window: EmacsWindow,
        charpos: i64,
        next_visible_out: *mut i64,
    ) -> c_int;

    // ========================================================================
    // Mode-line
    // ========================================================================

    /// Get mode-line text for a window as plain UTF-8.
    /// Returns the number of bytes written, or -1 on error.
    /// Also fills face_out with the mode-line face (active or inactive).
    pub fn neomacs_layout_mode_line_text(
        window: EmacsWindow,
        frame: EmacsFrame,
        out_buf: *mut u8,
        out_buf_len: i64,
        face_out: *mut FaceDataFFI,
    ) -> i64;

    /// Get header-line text for a window as plain UTF-8.
    /// Returns the number of bytes written, 0 if no header-line, or -1 on error.
    /// Also fills face_out with the header-line face (active or inactive).
    pub fn neomacs_layout_header_line_text(
        window: EmacsWindow,
        frame: EmacsFrame,
        out_buf: *mut u8,
        out_buf_len: i64,
        face_out: *mut FaceDataFFI,
    ) -> i64;

    /// Get tab-line text for a window as plain UTF-8.
    /// Returns the number of bytes written, 0 if no tab-line, or -1 on error.
    /// Also fills face_out with the tab-line face.
    pub fn neomacs_layout_tab_line_text(
        window: EmacsWindow,
        frame: EmacsFrame,
        out_buf: *mut u8,
        out_buf_len: i64,
        face_out: *mut FaceDataFFI,
    ) -> i64;

    // ========================================================================
    // Line numbers
    // ========================================================================

    /// Get line number display configuration for a window.
    /// Returns 0 on success, -1 on error.
    pub fn neomacs_layout_line_number_config(
        window: EmacsWindow,
        buffer: EmacsBuffer,
        buffer_zv: i64,
        max_rows: c_int,
        config_out: *mut LineNumberConfigFFI,
    ) -> c_int;

    /// Count the line number at a character position.
    /// Returns the 1-based line number.
    pub fn neomacs_layout_count_line_number(
        buffer: EmacsBuffer,
        charpos: i64,
        widen: c_int,
    ) -> i64;

    /// Resolve the face for a line number and fill FaceDataFFI.
    pub fn neomacs_layout_line_number_face(
        window: EmacsWindow,
        is_current: c_int,
        lnum: i64,
        major_tick: c_int,
        minor_tick: c_int,
        face_out: *mut FaceDataFFI,
    ) -> c_int;

    // ========================================================================
    // Display text property
    // ========================================================================

    /// Check for a 'display text property at charpos.
    /// Handles string replacement and space specs.
    /// Writes replacement string into str_buf (type=1).
    /// Fills DisplayPropFFI with type, length, and region end.
    /// Returns 0 on success, -1 on error.
    pub fn neomacs_layout_check_display_prop(
        buffer: EmacsBuffer,
        window: EmacsWindow,
        charpos: i64,
        str_buf: *mut u8,
        str_buf_len: c_int,
        out: *mut DisplayPropFFI,
    ) -> c_int;

    // ========================================================================
    // Overlay strings
    // ========================================================================

    /// Collect overlay before-string and after-string at a position.
    /// Before-strings come from overlays starting at charpos.
    /// After-strings come from overlays ending at charpos.
    /// Returns 0 on success, -1 on error.
    pub fn neomacs_layout_overlay_strings_at(
        buffer: EmacsBuffer,
        window: EmacsWindow,
        charpos: i64,
        before_buf: *mut u8,
        before_buf_len: c_int,
        before_len_out: *mut c_int,
        after_buf: *mut u8,
        after_buf_len: c_int,
        after_len_out: *mut c_int,
        before_face_out: *mut FaceDataFFI,
        after_face_out: *mut FaceDataFFI,
        before_nruns_out: *mut c_int,
        after_nruns_out: *mut c_int,
        left_fringe_bitmap_out: *mut c_int,
        left_fringe_fg_out: *mut u32,
        left_fringe_bg_out: *mut u32,
        right_fringe_bitmap_out: *mut c_int,
        right_fringe_fg_out: *mut u32,
        right_fringe_bg_out: *mut u32,
    ) -> c_int;

    // ========================================================================
    // Glyphless characters
    // ========================================================================

    /// Check if a character should display as a glyphless glyph.
    /// Looks up Vglyphless_char_display char-table.
    /// method_out: 0=normal, 1=thin_space, 2=empty_box, 3=hex_code,
    ///             4=acronym, 5=zero_width.
    /// For method=4 (acronym), writes the string into str_buf.
    pub fn neomacs_layout_check_glyphless(
        frame: EmacsFrame,
        codepoint: c_int,
        method_out: *mut c_int,
        str_buf: *mut u8,
        str_buf_len: c_int,
        str_len_out: *mut c_int,
    ) -> c_int;

    // ========================================================================
    // Margin strings
    // ========================================================================

    /// Collect margin overlay strings at a buffer position.
    /// Checks overlay before/after-strings for (margin left-margin) and
    /// (margin right-margin) display properties.
    pub fn neomacs_layout_margin_strings_at(
        buffer: EmacsBuffer,
        window: EmacsWindow,
        charpos: i64,
        left_buf: *mut u8,
        left_buf_len: c_int,
        left_len_out: *mut c_int,
        right_buf: *mut u8,
        right_buf_len: c_int,
        right_len_out: *mut c_int,
    ) -> c_int;

    // ========================================================================
    // Line height / spacing
    // ========================================================================

    /// Check line-height and line-spacing text properties at a position.
    /// Returns extra height (pixels) beyond base_height for the row.
    pub fn neomacs_layout_check_line_spacing(
        buffer: EmacsBuffer,
        window: EmacsWindow,
        charpos: i64,
        base_height: f32,
        extra_height_out: *mut f32,
    ) -> c_int;

    /// Check line-prefix or wrap-prefix text property at a position.
    /// prefix_type: 0=line-prefix, 1=wrap-prefix
    /// Returns prefix width in columns via width_out.
    /// width_out = -1.0 means no override (use window default).
    pub fn neomacs_layout_check_line_prefix(
        buffer: EmacsBuffer,
        window: EmacsWindow,
        charpos: i64,
        prefix_type: c_int,
        width_out: *mut f32,
    ) -> c_int;

    /// Get fringe bitmap data for a given bitmap ID.
    /// Writes row data (one u16 per row) into bits_out.
    /// Returns number of rows written, 0 if bitmap not found.
    pub fn neomacs_layout_get_fringe_bitmap(
        bitmap_id: c_int,
        bits_out: *mut u16,
        bits_buf_len: c_int,
        width_out: *mut c_int,
        height_out: *mut c_int,
        align_out: *mut c_int,
    ) -> c_int;
}

/// FFI-safe line number configuration struct.
/// Matches the C struct LineNumberConfigFFI in neomacsterm.c.
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct LineNumberConfigFFI {
    /// 0=off, 1=absolute, 2=relative, 3=visual
    pub mode: c_int,
    /// Column width for line numbers (including padding)
    pub width: c_int,
    /// display-line-numbers-offset
    pub offset: c_int,
    /// display-line-numbers-major-tick
    pub major_tick: c_int,
    /// display-line-numbers-minor-tick
    pub minor_tick: c_int,
    /// display-line-numbers-current-absolute
    pub current_absolute: c_int,
    /// display-line-numbers-widen
    pub widen: c_int,
}

/// FFI-safe display text property result.
/// Matches the C struct DisplayPropFFI in neomacsterm.c.
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct DisplayPropFFI {
    /// 0=none, 1=string replacement, 2=space, 3=align-to, 4=image, 5=raise
    pub prop_type: c_int,
    /// Bytes of replacement string (type=1)
    pub str_len: c_int,
    /// Space width in columns (type=2)
    pub space_width: f32,
    /// Space height in pixels (type=2), 0 = use default char_h
    pub space_height: f32,
    /// Charpos where this display property region ends
    pub covers_to: i64,
    /// Align-to column (type=3)
    pub align_to: f32,
    /// GPU image ID (type=4)
    pub image_gpu_id: u32,
    /// Image width in pixels (type=4)
    pub image_width: c_int,
    /// Image height in pixels (type=4)
    pub image_height: c_int,
    /// Image horizontal margin in pixels (type=4)
    pub image_hmargin: c_int,
    /// Image vertical margin in pixels (type=4)
    pub image_vmargin: c_int,
    /// Image ascent: 0-100 = percent, -1 = centered (type=4)
    pub image_ascent: c_int,
    /// Raise factor (type=5), fraction of line height
    pub raise_factor: f32,
    /// Display string face fg (type=1), 0=use position face
    pub display_fg: u32,
    /// Display string face bg (type=1), 0=use position face
    pub display_bg: u32,
    /// Fringe bitmap ID (type=6 left-fringe, type=7 right-fringe)
    pub fringe_bitmap_id: c_int,
    /// Fringe face foreground color (type=6,7)
    pub fringe_fg: u32,
    /// Fringe face background color (type=6,7)
    pub fringe_bg: u32,
    /// Font height multiplier (type=5,8): 0.0=default, >0=scale factor
    pub height_factor: f32,
    /// Video ID (type=9)
    pub video_id: u32,
    /// WebKit view ID (type=10)
    pub webkit_id: u32,
    /// Number of face runs in display string (type=1)
    pub display_nruns: c_int,
}

/// FFI-safe window parameters struct.
/// Matches the C struct in neomacsterm.c.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct WindowParamsFFI {
    /// Window pointer as i64
    pub window_id: i64,
    /// Buffer pointer as u64
    pub buffer_id: u64,
    /// Window pointer (for FFI callbacks)
    pub window_ptr: EmacsWindow,
    /// Buffer pointer (for FFI callbacks)
    pub buffer_ptr: EmacsBuffer,

    /// Frame-absolute position
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,

    /// Text area bounds (excluding fringes, margins)
    pub text_x: f32,
    pub text_y: f32,
    pub text_width: f32,
    pub text_height: f32,

    /// Whether this is the selected window
    pub selected: c_int,
    /// Whether this is the minibuffer window
    pub is_minibuffer: c_int,

    /// First visible buffer position
    pub window_start: i64,
    /// Last visible buffer position from previous frame (0 = unknown)
    pub window_end: i64,
    /// Point position
    pub point: i64,
    /// Buffer size (ZV)
    pub buffer_zv: i64,
    /// Buffer beginning (BEGV)
    pub buffer_begv: i64,

    /// Horizontal scroll
    pub hscroll: c_int,
    /// Truncate long lines
    pub truncate_lines: c_int,
    /// Word wrap at word boundaries
    pub word_wrap: c_int,
    /// Tab width
    pub tab_width: c_int,

    /// Default face colors (sRGB pixels)
    pub default_fg: u32,
    pub default_bg: u32,

    /// Character cell dimensions
    pub char_width: f32,
    pub char_height: f32,
    /// Font pixel size
    pub font_pixel_size: f32,
    /// Font ascent
    pub font_ascent: f32,

    /// Special line heights
    pub mode_line_height: f32,
    pub header_line_height: f32,
    pub tab_line_height: f32,

    /// Cursor type and width
    pub cursor_type: u8,
    pub cursor_bar_width: c_int,

    /// Fringe widths in pixels
    pub left_fringe_width: f32,
    pub right_fringe_width: f32,
    /// indicate-empty-lines: 0=off, 1=left, 2=right
    pub indicate_empty_lines: c_int,
    /// show-trailing-whitespace
    pub show_trailing_whitespace: c_int,
    /// trailing-whitespace face background color (sRGB pixel)
    pub trailing_ws_bg: u32,
    /// fill-column-indicator column (0 = off)
    pub fill_column_indicator: c_int,
    /// fill-column-indicator character (0 = use default '|')
    pub fill_column_indicator_char: c_int,
    /// fill-column-indicator face foreground (sRGB pixel)
    pub fill_column_indicator_fg: u32,
    /// Extra line spacing in pixels
    pub extra_line_spacing: f32,
    /// Whether to show cursor in non-selected windows
    pub cursor_in_non_selected: c_int,
    /// selective-display: 0=off, >0=hide lines indented more than N columns
    pub selective_display: c_int,
    /// escape-glyph face foreground color for control chars
    pub escape_glyph_fg: u32,
    /// nobreak-char-display: 0=off, 1=highlight, 2=escape notation
    pub nobreak_char_display: c_int,
    /// nobreak-char face foreground color
    pub nobreak_char_fg: u32,
    /// glyphless-char face foreground color
    pub glyphless_char_fg: u32,
    /// wrap-prefix: string rendered at start of continuation lines
    pub wrap_prefix: [u8; 128],
    pub wrap_prefix_len: c_int,
    /// line-prefix: string rendered at start of all visual lines
    pub line_prefix: [u8; 128],
    pub line_prefix_len: c_int,
    /// Margin widths in pixels
    pub left_margin_width: f32,
    pub right_margin_width: f32,
    /// Buffer file name (NULL if no file)
    pub buffer_file_name: *const std::ffi::c_char,
    /// Whether the buffer has unsaved modifications
    pub modified: c_int,
}

impl Default for WindowParamsFFI {
    fn default() -> Self {
        // Safety: All fields are either numeric types or arrays of u8,
        // which are all valid when zeroed.
        unsafe { std::mem::zeroed() }
    }
}

/// FFI-safe face data struct.
#[repr(C)]
#[derive(Debug, Clone, Default)]
pub struct FaceDataFFI {
    /// Face ID
    pub face_id: u32,
    /// Foreground color (sRGB pixel: 0x00RRGGBB)
    pub fg: u32,
    /// Background color (sRGB pixel: 0x00RRGGBB)
    pub bg: u32,
    /// Font family name (null-terminated C string, valid for duration of layout)
    pub font_family: *const c_char,
    /// Font weight (CSS scale: 400=normal, 700=bold)
    pub font_weight: c_int,
    /// Italic flag
    pub italic: c_int,
    /// Font pixel size
    pub font_size: c_int,
    /// Underline style (0=none, 1=single, 2=wave, 3=double, 4=dotted, 5=dashed)
    pub underline_style: c_int,
    /// Underline color (sRGB pixel)
    pub underline_color: u32,
    /// Strike-through (0=none, 1=enabled)
    pub strike_through: c_int,
    /// Strike-through color
    pub strike_through_color: u32,
    /// Overline (0=none, 1=enabled)
    pub overline: c_int,
    /// Overline color
    pub overline_color: u32,
    /// Box type (0=none, 1=line)
    pub box_type: c_int,
    /// Box color
    pub box_color: u32,
    /// Box line width
    pub box_line_width: c_int,
    /// Box corner radius (0 = sharp corners)
    pub box_corner_radius: c_int,
    /// Extend: face bg extends to end of visual line (0=no, 1=yes)
    pub extend: c_int,
    /// Per-face font character width (0.0 = use window default)
    pub font_char_width: f32,
    /// Per-face font ascent (0.0 = use window default)
    pub font_ascent: f32,
    /// Per-face space width (for tab stop calculations with proportional fonts)
    pub font_space_width: f32,
    /// Whether the face's font is monospace (1=monospace, 0=proportional)
    pub font_is_monospace: c_int,
    /// Stipple bitmap ID (0 = none, positive = 1-based bitmap index)
    pub stipple: c_int,
    /// Overstrike flag (1 = simulate bold by drawing twice at x and x+1)
    /// Set by Emacs when bold variant is unavailable for the font.
    pub overstrike: c_int,
}

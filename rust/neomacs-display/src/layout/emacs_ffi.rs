//! FFI declarations for reading Emacs data structures.
//!
//! These functions are implemented in C (neomacsterm.c) and called by
//! the Rust layout engine to read buffer text, face data, and window geometry.
//!
//! Safety: All functions here are called on the Emacs thread during
//! redisplay_internal(), so they are thread-safe (Emacs is single-threaded).
//! The pointers are valid for the duration of the layout computation.

use std::ffi::c_int;
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

    /// Get a byte from buffer text at the given byte position.
    /// Handles the gap buffer transparently.
    /// Returns -1 if pos is out of range.
    pub fn neomacs_layout_buffer_byte_at(
        buffer: EmacsBuffer,
        byte_pos: i64,
    ) -> c_int;

    /// Copy buffer text (UTF-8) into the provided buffer.
    /// Handles gap buffer and multibyte encoding.
    /// Returns the number of bytes written, or -1 on error.
    /// `from` and `to` are character positions (not byte positions).
    pub fn neomacs_layout_buffer_text(
        buffer: EmacsBuffer,
        from: i64,
        to: i64,
        out_buf: *mut u8,
        out_buf_len: i64,
    ) -> i64;

    /// Get the character at a character position.
    /// Returns the Unicode codepoint, or -1 if out of range.
    pub fn neomacs_layout_char_at(
        buffer: EmacsBuffer,
        charpos: i64,
    ) -> i32;

    // ========================================================================
    // Buffer metadata
    // ========================================================================

    /// Get buffer narrowing bounds: BEGV and ZV (character positions).
    pub fn neomacs_layout_buffer_bounds(
        buffer: EmacsBuffer,
        begv: *mut i64,
        zv: *mut i64,
    );

    /// Get buffer point position (character position).
    pub fn neomacs_layout_buffer_point(buffer: EmacsBuffer) -> i64;

    /// Check if buffer uses multibyte encoding.
    pub fn neomacs_layout_buffer_multibyte_p(buffer: EmacsBuffer) -> c_int;

    /// Get buffer-local tab-width.
    pub fn neomacs_layout_buffer_tab_width(buffer: EmacsBuffer) -> c_int;

    /// Get buffer-local truncate-lines setting.
    pub fn neomacs_layout_buffer_truncate_lines(buffer: EmacsBuffer) -> c_int;

    // ========================================================================
    // Window geometry
    // ========================================================================

    /// Get the number of leaf windows in the frame.
    pub fn neomacs_layout_frame_window_count(frame: EmacsFrame) -> c_int;

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

    // ========================================================================
    // Writing layout results back to Emacs
    // ========================================================================

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
    /// 0=none, 1=string replacement, 2=space
    pub prop_type: c_int,
    /// Bytes of replacement string (type=1)
    pub str_len: c_int,
    /// Space width in columns (type=2)
    pub space_width: f32,
    /// Charpos where this display property region ends
    pub covers_to: i64,
}

/// FFI-safe window parameters struct.
/// Matches the C struct in neomacsterm.c.
#[repr(C)]
#[derive(Debug, Clone, Default)]
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

    /// First visible buffer position
    pub window_start: i64,
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
}

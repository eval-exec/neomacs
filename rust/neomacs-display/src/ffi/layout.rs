//! Rust Layout Engine FFI Entry Point
//!
//! neomacs_rust_layout_frame, neomacs_layout_charpos_at_pixel,
//! neomacs_layout_window_charpos.

use super::*;

// ============================================================================
// Rust Layout Engine FFI Entry Point
// ============================================================================

/// Global layout engine instance (lazily initialized)
pub(crate) static mut LAYOUT_ENGINE: Option<crate::layout::LayoutEngine> = None;

/// Pending ligatures-enabled flag, set before layout engine is initialized.
/// Applied on first engine creation so init.el settings are not lost.
pub(crate) static mut PENDING_LIGATURES_ENABLED: Option<bool> = None;

/// Called from C when `neomacs-use-rust-display` is enabled.
/// The Rust layout engine reads buffer data via FFI helpers and produces
/// a FrameGlyphBuffer, bypassing the C matrix extraction.
///
/// # Safety
/// Must be called on the Emacs thread. All pointers must be valid.
#[no_mangle]
pub unsafe extern "C" fn neomacs_rust_layout_frame(
    handle: *mut NeomacsDisplay,
    frame_ptr: *mut c_void,
    width: f32,
    height: f32,
    char_width: f32,
    char_height: f32,
    font_pixel_size: f32,
    background: u32,
    vertical_border_fg: u32,
    right_divider_width: i32,
    bottom_divider_width: i32,
    divider_fg: u32,
    divider_first_fg: u32,
    divider_last_fg: u32,
) {
    if handle.is_null() || frame_ptr.is_null() {
        log::error!("neomacs_rust_layout_frame: null handle or frame_ptr");
        return;
    }

    // Wrap in catch_unwind to prevent Rust panics from crossing FFI boundary
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let display = &mut *handle;

        // Validate Emacs struct offsets on first call
        crate::layout::emacs_types::ensure_offsets_valid();

        // Initialize layout engine on first call
        if (*std::ptr::addr_of!(LAYOUT_ENGINE)).is_none() {
            let mut engine = crate::layout::LayoutEngine::new();
            // Apply pending ligatures setting from init.el (set before engine existed)
            if let Some(enabled) = *std::ptr::addr_of!(PENDING_LIGATURES_ENABLED) {
                engine.ligatures_enabled = enabled;
                log::info!("Applied pending ligatures_enabled={}", enabled);
            }
            *std::ptr::addr_of_mut!(LAYOUT_ENGINE) = Some(engine);
            log::info!("Rust layout engine initialized");
        }

        let engine = match (*std::ptr::addr_of_mut!(LAYOUT_ENGINE)).as_mut() {
            Some(e) => e,
            None => {
                log::error!("Rust layout engine initialization failed");
                return;
            }
        };
        let frame_params = crate::layout::FrameParams {
            width,
            height,
            char_width: if char_width > 0.0 { char_width } else { 8.0 },
            char_height: if char_height > 0.0 { char_height } else { 16.0 },
            font_pixel_size: if font_pixel_size > 0.0 { font_pixel_size } else { 14.0 },
            background,
            vertical_border_fg,
            right_divider_width,
            bottom_divider_width,
            divider_fg,
            divider_first_fg,
            divider_last_fg,
        };

        engine.layout_frame(
            frame_ptr,
            &frame_params,
            &mut display.frame_glyphs,
        );
    }));

    if let Err(e) = result {
        let msg = if let Some(s) = e.downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = e.downcast_ref::<String>() {
            s.clone()
        } else {
            "unknown panic".to_string()
        };
        log::error!("PANIC in neomacs_rust_layout_frame: {}", msg);
    }
}

/// Query buffer character position at given frame-relative pixel coordinates.
/// Used by mouse interaction (note_mouse_highlight, mouse clicks).
/// Returns charpos, or -1 if not found.
///
/// # Safety
/// Must be called on the Emacs thread.
#[no_mangle]
pub unsafe extern "C" fn neomacs_layout_charpos_at_pixel(
    px: f32,
    py: f32,
) -> i64 {
    crate::layout::hit_test_charpos_at_pixel(px, py)
}

/// Query buffer character position for a specific window at
/// window-relative pixel coordinates.
/// Returns charpos, or -1 if not found.
///
/// # Safety
/// Must be called on the Emacs thread.
#[no_mangle]
pub unsafe extern "C" fn neomacs_layout_window_charpos(
    window_id: i64,
    wx: f32,
    wy: f32,
) -> i64 {
    crate::layout::hit_test_window_charpos(window_id, wx, wy)
}

// Note: Event Polling FFI Functions have been removed
// Events are now delivered via the threaded mode wakeup mechanism
// Use neomacs_display_drain_input() instead

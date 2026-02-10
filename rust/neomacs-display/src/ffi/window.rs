//! Window Management and Window-Targeted Rendering FFI functions
//!
//! Create/destroy/show/title/size windows, begin_frame_window, end_frame_window.

use super::*;

// ============================================================================
// Window Management FFI Functions
// ============================================================================

/// Create a new window with the specified dimensions and title.
///
/// Returns the window ID. The window will be created during the next poll_events call.
/// Returns 0 if the backend is not available.
#[no_mangle]
pub extern "C" fn neomacs_display_create_window(
    _handle: *mut NeomacsDisplay,
    _width: i32,
    _height: i32,
    _title: *const c_char,
) -> u32 {
    // In threaded mode, the main window is created automatically by the render thread
    // Return window ID 1 (the main window)
    #[cfg(feature = "winit-backend")]
    unsafe {
        if (*std::ptr::addr_of!(super::THREADED_STATE)).is_some() {
            return 1; // Main window ID
        }
    }

    0
}

/// Destroy a window by its ID.
#[no_mangle]
pub extern "C" fn neomacs_display_destroy_window(handle: *mut NeomacsDisplay, window_id: u32) {
    let display = unsafe { &mut *handle };

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        backend.destroy_window(window_id);
    }
}

/// Show or hide a window.
#[no_mangle]
pub extern "C" fn neomacs_display_show_window(
    handle: *mut NeomacsDisplay,
    window_id: u32,
    visible: bool,
) {
    let display = unsafe { &mut *handle };

    #[cfg(feature = "winit-backend")]
    if let Some(ref backend) = display.winit_backend {
        if let Some(state) = backend.get_window(window_id) {
            state.window.set_visible(visible);
        }
    }
}

/// Set the title of a window.
#[no_mangle]
pub extern "C" fn neomacs_display_set_window_title(
    handle: *mut NeomacsDisplay,
    window_id: u32,
    title: *const c_char,
) {
    let display = unsafe { &mut *handle };

    #[cfg(feature = "winit-backend")]
    if let Some(ref backend) = display.winit_backend {
        if let Some(state) = backend.get_window(window_id) {
            let title_str = if title.is_null() {
                "Emacs"
            } else {
                unsafe { std::ffi::CStr::from_ptr(title).to_str().unwrap_or("Emacs") }
            };
            state.window.set_title(title_str);
        }
    }
}

/// Set the size of a window.
#[no_mangle]
pub extern "C" fn neomacs_display_set_window_size(
    handle: *mut NeomacsDisplay,
    window_id: u32,
    width: i32,
    height: i32,
) {
    let display = unsafe { &mut *handle };

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(state) = backend.get_window_mut(window_id) {
            let _ = state.window.request_inner_size(
                winit::dpi::PhysicalSize::new(width as u32, height as u32)
            );
        }
    }
}

// ============================================================================
// Window-Targeted Rendering FFI Functions
// ============================================================================

/// Begin a frame for a specific window.
///
/// Clears the window's scene to prepare for new content.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_begin_frame_window(
    handle: *mut NeomacsDisplay,
    window_id: u32,
    char_width: f32,
    char_height: f32,
    font_pixel_size: f32,
) {
    let display = &mut *handle;

    // Track which window we're currently rendering to
    display.current_render_window_id = window_id;

    // Matrix-based full-frame rendering: sync frame dimensions and background
    // from the scene, then clear all glyphs for the new frame.
    display.frame_glyphs.width = display.scene.width;
    display.frame_glyphs.height = display.scene.height;
    display.frame_glyphs.char_width = if char_width > 0.0 { char_width } else { 8.0 };
    display.frame_glyphs.char_height = if char_height > 0.0 { char_height } else { 16.0 };
    display.frame_glyphs.font_pixel_size = if font_pixel_size > 0.0 { font_pixel_size } else { 14.0 };
    display.frame_glyphs.background = display.scene.background;
    display.frame_glyphs.clear_all();

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        backend.begin_frame_for_window(window_id);
    }
}

/// End a frame for a specific window and present it.
///
/// Renders the window's scene to its surface and presents it.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_end_frame_window(
    handle: *mut NeomacsDisplay,
    window_id: u32,
) {
    let display = &mut *handle;

    #[cfg(feature = "winit-backend")]
    {
        if let Some(state) = (*std::ptr::addr_of!(super::THREADED_STATE)).as_ref() {
            // Matrix-based full-frame rendering: always send the complete frame.
            // The buffer was cleared at begin_frame and rebuilt by the matrix walker,
            // so it always contains the complete visible state.
            let frame = display.frame_glyphs.clone();
            let _ = state.emacs_comms.frame_tx.try_send(frame);
        } else if let Some(ref mut backend) = display.winit_backend {
            backend.end_frame_for_window(
                window_id,
                &display.frame_glyphs,
                &display.faces,
            );
        }
    }

    #[cfg(not(feature = "winit-backend"))]
    {
        let _ = window_id;
    }

    display.current_render_window_id = 0;
}

// ============================================================================
// Multi-Window FFI (Threaded Mode)
// ============================================================================

/// Create a new OS window for a top-level Emacs frame.
///
/// The window is created asynchronously on the render thread.
/// `emacs_frame_id` should be the Emacs frame pointer cast to u64.
///
/// # Safety
/// Must be called from the Emacs thread with valid parameters.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_create_os_window(
    _handle: *mut NeomacsDisplay,
    emacs_frame_id: u64,
    width: c_int,
    height: c_int,
    title: *const c_char,
) {
    #[cfg(feature = "winit-backend")]
    {
        if let Some(state) = (*std::ptr::addr_of!(super::THREADED_STATE)).as_ref() {
            let title_str = if title.is_null() {
                "neomacs".to_string()
            } else {
                CStr::from_ptr(title).to_str().unwrap_or("neomacs").to_string()
            };
            let _ = state.emacs_comms.cmd_tx.try_send(
                RenderCommand::CreateWindow {
                    emacs_frame_id,
                    width: width as u32,
                    height: height as u32,
                    title: title_str,
                }
            );
        }
    }
}

/// Destroy an OS window for a top-level Emacs frame.
///
/// # Safety
/// Must be called from the Emacs thread.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_destroy_os_window(
    _handle: *mut NeomacsDisplay,
    emacs_frame_id: u64,
) {
    #[cfg(feature = "winit-backend")]
    {
        if let Some(state) = (*std::ptr::addr_of!(super::THREADED_STATE)).as_ref() {
            let _ = state.emacs_comms.cmd_tx.try_send(
                RenderCommand::DestroyWindow { emacs_frame_id }
            );
        }
    }
}

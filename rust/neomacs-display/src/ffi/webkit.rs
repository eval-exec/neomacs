//! WebKit Integration and Terminal (neo-term) FFI functions

use super::*;

// ============================================================================
// WebKit Integration
// ============================================================================

#[cfg(feature = "wpe-webkit")]
use std::cell::RefCell;
#[cfg(feature = "wpe-webkit")]
use crate::backend::wpe::WpeBackend;

#[cfg(feature = "wpe-webkit")]
thread_local! {
    static WPE_BACKEND: RefCell<Option<WpeBackend>> = const { RefCell::new(None) };
}

// ============================================================================
// Terminal (neo-term) FFI
// ============================================================================

/// Create a new terminal.
///
/// Returns terminal ID (>0 on success, 0 on failure).
/// `mode`: 0=Window, 1=Inline, 2=Floating
/// `shell`: optional shell path (NULL for default)
#[cfg(feature = "neo-term")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_terminal_create(
    cols: u16,
    rows: u16,
    mode: u8,
    shell: *const c_char,
) -> u32 {
    if let Some(ref state) = THREADED_STATE {
        let id = TERMINAL_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let shell_str = if shell.is_null() {
            None
        } else {
            std::ffi::CStr::from_ptr(shell).to_str().ok().map(|s| s.to_string())
        };
        let cmd = RenderCommand::TerminalCreate {
            id,
            cols,
            rows,
            mode,
            shell: shell_str,
        };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        log::info!("terminal_create: id={}, {}x{}, mode={}", id, cols, rows, mode);
        return id;
    }
    0
}

/// Write input data to a terminal (keyboard input from user).
#[cfg(feature = "neo-term")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_terminal_write(
    terminal_id: u32,
    data: *const u8,
    len: usize,
) {
    if data.is_null() || len == 0 {
        return;
    }
    if let Some(ref state) = THREADED_STATE {
        let bytes = std::slice::from_raw_parts(data, len).to_vec();
        let cmd = RenderCommand::TerminalWrite {
            id: terminal_id,
            data: bytes,
        };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Resize a terminal.
#[cfg(feature = "neo-term")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_terminal_resize(
    terminal_id: u32,
    cols: u16,
    rows: u16,
) {
    if let Some(ref state) = THREADED_STATE {
        let cmd = RenderCommand::TerminalResize {
            id: terminal_id,
            cols,
            rows,
        };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Destroy a terminal.
#[cfg(feature = "neo-term")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_terminal_destroy(
    terminal_id: u32,
) {
    if let Some(ref state) = THREADED_STATE {
        let cmd = RenderCommand::TerminalDestroy { id: terminal_id };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Set floating terminal position and opacity.
#[cfg(feature = "neo-term")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_terminal_set_float(
    terminal_id: u32,
    x: f32,
    y: f32,
    opacity: f32,
) {
    if let Some(ref state) = THREADED_STATE {
        let cmd = RenderCommand::TerminalSetFloat {
            id: terminal_id,
            x,
            y,
            opacity,
        };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Get visible text from a terminal.
///
/// Returns a malloc'd C string (caller must free with `free()`).
/// Returns NULL on failure.
#[cfg(feature = "neo-term")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_terminal_get_text(
    terminal_id: u32,
) -> *mut c_char {
    #[cfg(feature = "winit-backend")]
    {
        if let Some(ref state) = THREADED_STATE {
            if let Ok(shared) = state.shared_terminals.lock() {
                if let Some(term_arc) = shared.get(&terminal_id) {
                    use alacritty_terminal::grid::Dimensions;
                    let term = term_arc.lock();
                    let grid = term.grid();
                    let cols = grid.columns();
                    let rows = grid.screen_lines();
                    let text = crate::terminal::content::extract_text(
                        &*term, 0, 0,
                        rows.saturating_sub(1),
                        cols.saturating_sub(1),
                    );
                    drop(term);
                    match CString::new(text) {
                        Ok(c_string) => return c_string.into_raw(),
                        Err(_) => return std::ptr::null_mut(),
                    }
                }
            }
        }
    }
    std::ptr::null_mut()
}

/// Callback type for webkit new window requests
pub type WebKitNewWindowCallback = extern "C" fn(u32, *const c_char, *const c_char) -> bool;

/// Set callback for WebKit new window/tab requests
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_set_new_window_callback(
    callback: Option<extern "C" fn(u32, *const c_char, *const c_char) -> bool>,
) {
    #[cfg(feature = "wpe-webkit")]
    {
        crate::backend::wpe::set_new_window_callback(callback);
        if callback.is_some() {
            log::info!("WebKit new window callback set");
        } else {
            log::info!("WebKit new window callback cleared");
        }
    }
    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = callback;
    }
}

/// Callback type for WebKit page load events
pub type WebKitLoadCallback = extern "C" fn(u32, c_int, *const c_char);

/// Set callback for WebKit page load events
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_set_load_callback(
    callback: Option<extern "C" fn(u32, c_int, *const c_char)>,
) {
    #[cfg(feature = "wpe-webkit")]
    {
        crate::backend::wpe::set_load_callback(callback);
        if callback.is_some() {
            log::info!("WebKit load callback set");
        } else {
            log::info!("WebKit load callback cleared");
        }
    }
    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = callback;
    }
}

/// Initialize WebKit subsystem with EGL display
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_init(
    handle: *mut NeomacsDisplay,
    egl_display: *mut libc::c_void,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        log::info!("neomacs_display_webkit_init: ENTER egl_display={:?}", egl_display);

        // In threaded mode, skip WPE init here -- the render thread will do it
        // with the correct DRM render node from wgpu adapter info.
        if (*std::ptr::addr_of!(super::THREADED_STATE)).is_some() {
            log::info!("neomacs_display_webkit_init: threaded mode, deferring WPE init to render thread");
            return 0;
        }

        // Non-threaded path (legacy): initialize WPE here
        let egl_display = if egl_display.is_null() {
            log::info!("neomacs_display_webkit_init: egl_display is NULL, trying eglGetCurrentDisplay");
            let current = egl_get_current_display();
            log::info!("neomacs_display_webkit_init: eglGetCurrentDisplay returned {:?}", current);
            current
        } else {
            egl_display
        };

        // Try to get the DRM render node from wgpu adapter info for GPU device selection
        log::debug!("neomacs_display_webkit_init: getting DRM device path, handle={:?}", handle);

        let device_path: Option<String> = if !handle.is_null() {
            #[cfg(all(feature = "winit-backend", target_os = "linux"))]
            {
                use crate::backend::wgpu::get_render_node_from_adapter_info;

                log::debug!("neomacs_display_webkit_init: checking winit backend");

                if let Some(ref backend) = (*handle).winit_backend {
                    log::debug!("neomacs_display_webkit_init: have winit backend, checking adapter_info");

                    if let Some(adapter_info) = backend.adapter_info() {
                        log::info!("neomacs_display_webkit_init: Found wgpu adapter info");

                        if let Some(path) = get_render_node_from_adapter_info(adapter_info) {
                            log::info!("neomacs_display_webkit_init: Using DRM render node: {:?}", path);
                            Some(path.to_string_lossy().into_owned())
                        } else {
                            log::warn!("neomacs_display_webkit_init: Could not find matching DRM render node");
                            None
                        }
                    } else {
                        log::warn!("neomacs_display_webkit_init: No adapter info available");
                        None
                    }
                } else {
                    log::warn!("neomacs_display_webkit_init: No winit backend available");
                    None
                }
            }
            #[cfg(not(all(feature = "winit-backend", target_os = "linux")))]
            {
                None
            }
        } else {
            log::warn!("neomacs_display_webkit_init: handle is NULL, using default GPU");
            None
        };

        // Initialize WPE backend with optional device path
        let result = if let Some(ref path) = device_path {
            WpeBackend::new_with_device(egl_display, Some(path.as_str()))
        } else {
            WpeBackend::new(egl_display)
        };

        match result {
            Ok(backend) => {
                WPE_BACKEND.with(|wpe| {
                    *wpe.borrow_mut() = Some(backend);
                });

                log::info!("neomacs_display_webkit_init: WPE backend initialized successfully");
                return 0;
            }
            Err(e) => {
                log::error!("neomacs_display_webkit_init: Failed to initialize WPE backend: {}", e);
                return -1;
            }
        }
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = handle;
        let _ = egl_display;
        log::warn!("WebKit support not compiled");
        -1
    }
}

/// Try to get current EGL display
#[cfg(feature = "wpe-webkit")]
unsafe fn egl_get_current_display() -> *mut libc::c_void {
    extern "C" {
        fn eglGetCurrentDisplay() -> *mut libc::c_void;
    }
    eglGetCurrentDisplay()
}

/// Create a new WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_create(
    _handle: *mut NeomacsDisplay,
    width: c_int,
    height: c_int,
) -> u32 {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let id = WEBKIT_VIEW_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let cmd = RenderCommand::WebKitCreate {
                id,
                width: width as u32,
                height: height as u32,
            };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return id;
        }
        log::error!("webkit_create: threaded mode not initialized");
        return 0;
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (width, height);
        log::warn!("WebKit support not compiled");
        0
    }
}

/// Destroy a WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_destroy(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitDestroy { id: view_id };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }
        log::error!("webkit_destroy: threaded mode not initialized");
        return -1;
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Load a URI in a WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_load_uri(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
    uri: *const c_char,
) -> c_int {
    if uri.is_null() {
        return -1;
    }

    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let url = CStr::from_ptr(uri).to_string_lossy().into_owned();
            let cmd = RenderCommand::WebKitLoadUri { id: view_id, url };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }
        log::error!("webkit_load_uri: threaded mode not initialized");
        return -1;
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Go back in a WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_go_back(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitGoBack { id: view_id };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }
        log::error!("webkit_go_back: threaded mode not initialized");
        return -1;
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Go forward in a WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_go_forward(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitGoForward { id: view_id };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }
        log::error!("webkit_go_forward: threaded mode not initialized");
        return -1;
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Reload a WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_reload(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitReload { id: view_id };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }
        log::error!("webkit_reload: threaded mode not initialized");
        return -1;
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Resize a WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_resize(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
    width: c_int,
    height: c_int,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitResize {
                id: view_id,
                width: width as u32,
                height: height as u32,
            };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }
        log::error!("webkit_resize: threaded mode not initialized");
        return -1;
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (view_id, width, height);
        -1
    }
}

/// Execute JavaScript in a WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_execute_js(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
    script: *const c_char,
) -> c_int {
    if script.is_null() {
        return -1;
    }

    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let script_str = match CStr::from_ptr(script).to_str() {
                Ok(s) => s,
                Err(_) => return -1,
            };
            let cmd = RenderCommand::WebKitExecuteJavaScript {
                id: view_id,
                script: script_str.to_string(),
            };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }
        log::error!("webkit_execute_js: threaded mode not initialized");
        return -1;
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Set a floating WebKit view position and size
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_floating_webkit(
    handle: *mut NeomacsDisplay,
    webkit_id: u32,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
) {
    info!("neomacs_display_set_floating_webkit: webkit_id={} x={} y={} {}x{}", webkit_id, x, y, width, height);
    if handle.is_null() {
        warn!("neomacs_display_set_floating_webkit: handle is null!");
        return;
    }

    let display = &mut *handle;

    // Update local scene (used for hit-testing in webkit_at_position)
    let target_scene = display.get_target_scene();
    target_scene.floating_webkits.retain(|w| w.webkit_id != webkit_id);
    target_scene.add_floating_webkit(
        webkit_id,
        x as f32,
        y as f32,
        width as f32,
        height as f32,
    );
    info!("neomacs_display_set_floating_webkit: now have {} floating webkits", target_scene.floating_webkits.len());

    // Send to render thread so it can actually render the floating overlay
    if let Some(ref state) = THREADED_STATE {
        let cmd = RenderCommand::WebKitSetFloating {
            id: webkit_id,
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
        };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Hide a floating WebKit view
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_hide_floating_webkit(
    handle: *mut NeomacsDisplay,
    webkit_id: u32,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.get_target_scene().remove_floating_webkit(webkit_id);

    // Send to render thread
    if let Some(ref state) = THREADED_STATE {
        let cmd = RenderCommand::WebKitRemoveFloating { id: webkit_id };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
    }
}

/// Find which webkit view (floating or inline) is at the given coordinates
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_at_position(
    handle: *mut NeomacsDisplay,
    x: c_int,
    y: c_int,
    out_webkit_id: *mut u32,
    out_rel_x: *mut c_int,
    out_rel_y: *mut c_int,
) -> c_int {
    if handle.is_null() {
        return 0;
    }

    let display = &*handle;

    // Check floating webkits in reverse order (top-most first)
    for webkit in display.scene.floating_webkits.iter().rev() {
        let wx = webkit.x as i32;
        let wy = webkit.y as i32;
        let ww = webkit.width as i32;
        let wh = webkit.height as i32;

        if x >= wx && x < wx + ww && y >= wy && y < wy + wh {
            if !out_webkit_id.is_null() {
                *out_webkit_id = webkit.webkit_id;
            }
            if !out_rel_x.is_null() {
                *out_rel_x = x - wx;
            }
            if !out_rel_y.is_null() {
                *out_rel_y = y - wy;
            }
            return 1;
        }
    }

    // Check inline webkit views from the glyph buffer
    for glyph in display.frame_glyphs.glyphs.iter().rev() {
        if let FrameGlyph::WebKit { webkit_id, x: wx, y: wy, width, height } = glyph {
            let gwx = *wx as i32;
            let gwy = *wy as i32;
            let gww = *width as i32;
            let gwh = *height as i32;

            if x >= gwx && x < gwx + gww && y >= gwy && y < gwy + gwh {
                if !out_webkit_id.is_null() {
                    *out_webkit_id = *webkit_id;
                }
                if !out_rel_x.is_null() {
                    *out_rel_x = x - gwx;
                }
                if !out_rel_y.is_null() {
                    *out_rel_y = y - gwy;
                }
                return 1;
            }
        }
    }

    0 // No webkit at position
}

/// Send keyboard event to WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_send_key(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
    key_code: u32,
    hardware_key_code: u32,
    pressed: c_int,
    modifiers: u32,
) {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitKeyEvent {
                id: webkit_id,
                keyval: key_code,
                keycode: hardware_key_code,
                pressed: pressed != 0,
                modifiers,
            };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return;
        }
        log::error!("webkit_send_key: threaded mode not initialized");
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (webkit_id, key_code, hardware_key_code, pressed, modifiers);
    }
}

/// Send pointer/mouse event to WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_send_pointer(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
    event_type: u32,
    x: c_int,
    y: c_int,
    button: u32,
    state: u32,
    modifiers: u32,
) {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state_ref) = THREADED_STATE {
            let cmd = RenderCommand::WebKitPointerEvent {
                id: webkit_id,
                event_type,
                x: x as i32,
                y: y as i32,
                button,
                state,
                modifiers,
            };
            let _ = state_ref.emacs_comms.cmd_tx.try_send(cmd);
            return;
        }
        log::error!("webkit_send_pointer: threaded mode not initialized");
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (webkit_id, event_type, x, y, button, state, modifiers);
    }
}

/// Send scroll event to WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_send_scroll(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
    x: c_int,
    y: c_int,
    delta_x: c_int,
    delta_y: c_int,
) {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitScroll {
                id: webkit_id,
                x: x as i32,
                y: y as i32,
                delta_x: delta_x as i32,
                delta_y: delta_y as i32,
            };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return;
        }
        log::error!("webkit_send_scroll: threaded mode not initialized");
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (webkit_id, x, y, delta_x, delta_y);
    }
}

/// Click in WebKit view (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_click(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
    x: c_int,
    y: c_int,
    button: u32,
) {
    #[cfg(feature = "wpe-webkit")]
    {
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitClick {
                id: webkit_id,
                x: x as i32,
                y: y as i32,
                button,
            };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return;
        }
        log::error!("webkit_click: threaded mode not initialized");
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (webkit_id, x, y, button);
    }
}

/// Scroll blit pixels in the pixel buffer (threaded mode only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_scroll_blit(
    _handle: *mut NeomacsDisplay,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    from_y: c_int,
    to_y: c_int,
    bg_r: f32,
    bg_g: f32,
    bg_b: f32,
) {
    #[cfg(feature = "winit-backend")]
    {
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::ScrollBlit {
                x: x as i32,
                y: y as i32,
                width: width as i32,
                height: height as i32,
                from_y: from_y as i32,
                to_y: to_y as i32,
                bg_r,
                bg_g,
                bg_b,
            };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            log::debug!("scroll_blit: sent command x={} y={} w={} h={} from_y={} to_y={}",
                       x, y, width, height, from_y, to_y);
            return;
        }
        log::error!("scroll_blit: threaded mode not initialized");
    }

    #[cfg(not(feature = "winit-backend"))]
    {
        let _ = (x, y, width, height, from_y, to_y, bg_r, bg_g, bg_b);
    }
}

/// Get WebKit view title
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_get_title(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
) -> *mut c_char {
    #[cfg(feature = "wpe-webkit")]
    {
        log::debug!("webkit_get_title: use InputEvent::WebKitTitleChanged callback instead");
        let _ = webkit_id;
        std::ptr::null_mut()
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = webkit_id;
        std::ptr::null_mut()
    }
}

/// Get WebKit view URL
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_get_url(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
) -> *mut c_char {
    #[cfg(feature = "wpe-webkit")]
    {
        log::debug!("webkit_get_url: use InputEvent::WebKitUrlChanged callback instead");
        let _ = webkit_id;
        std::ptr::null_mut()
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = webkit_id;
        std::ptr::null_mut()
    }
}

/// Get WebKit view loading progress
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_get_progress(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
) -> f64 {
    #[cfg(feature = "wpe-webkit")]
    {
        log::debug!("webkit_get_progress: use InputEvent::WebKitProgressChanged callback instead");
        let _ = webkit_id;
        -1.0
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = webkit_id;
        -1.0
    }
}

/// Check if WebKit view is loading
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_is_loading(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        log::debug!("webkit_is_loading: use InputEvent::WebKitProgressChanged callback instead");
        let _ = webkit_id;
        -1
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = webkit_id;
        -1
    }
}

/// Free a string returned by webkit_get_title or webkit_get_url
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_free_string(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

/// Update WebKit view - no-op in threaded mode
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_update(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
) -> c_int {
    let _ = webkit_id;
    0
}

/// Update all WebKit views - no-op in threaded mode
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_update_all(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    0
}

/// Add a WPE glyph to the current row
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_add_wpe_glyph(
    handle: *mut NeomacsDisplay,
    view_id: u32,
    pixel_width: c_int,
    pixel_height: c_int,
) {
    log::debug!("add_wpe_glyph: view_id={} size={}x{}", view_id, pixel_width, pixel_height);

    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    let current_y = display.current_row_y;
    let current_x = display.current_row_x;

    log::debug!("add_wpe_glyph: at ({}, {})", current_x, current_y);

    display.frame_glyphs.add_webkit(
        view_id,
        current_x as f32,
        current_y as f32,
        pixel_width as f32,
        pixel_height as f32,
    );
    display.current_row_x += pixel_width;
}

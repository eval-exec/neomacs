//! Threaded Mode FFI
//!
//! ThreadedState initialization, monitor info, event draining,
//! send_frame, send_command, shutdown, wakeup fd, display handle.

use super::*;

/// Access THREADED_STATE without creating a reference to the static.
/// Returns Option<&ThreadedState> using raw pointer indirection (Rust 2024 safe).
unsafe fn threaded_state() -> Option<&'static ThreadedState> {
    (*std::ptr::addr_of!(THREADED_STATE)).as_ref()
}

// ============================================================================
// Threaded Mode Initialization
// ============================================================================

/// Initialize display in threaded mode
///
/// Returns the wakeup pipe fd that Emacs should select() on,
/// or -1 on error.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_init_threaded(
    width: u32,
    height: u32,
    title: *const c_char,
) -> c_int {
    let _ = env_logger::try_init();
    log::info!("neomacs_display_init_threaded: {}x{}", width, height);

    let title = if title.is_null() {
        "Emacs".to_string()
    } else {
        CStr::from_ptr(title).to_string_lossy().into_owned()
    };

    // Create communication channels
    let comms = match ThreadComms::new() {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to create thread comms: {:?}", e);
            return -1;
        }
    };

    let wakeup_fd = comms.wakeup.read_fd();
    let (emacs_comms, render_comms) = comms.split();

    // Create shared image dimensions map
    let image_dimensions = Arc::new(Mutex::new(HashMap::new()));

    // Create shared monitor info storage (with condvar for sync)
    let shared_monitors: SharedMonitorInfo = Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new()));

    // Create shared terminal handles for cross-thread text extraction
    #[cfg(feature = "neo-term")]
    let shared_terminals: crate::terminal::SharedTerminals =
        Arc::new(Mutex::new(HashMap::new()));

    // Spawn render thread with shared maps
    let render_thread = RenderThread::spawn(
        render_comms,
        width,
        height,
        title,
        Arc::clone(&image_dimensions),
        Arc::clone(&shared_monitors),
        #[cfg(feature = "neo-term")]
        Arc::clone(&shared_terminals),
    );

    // Create a NeomacsDisplay handle for C code to use with frame operations
    // This is a lightweight handle that doesn't own the backend (render thread does)
    let display = Box::new(NeomacsDisplay {
        backend_type: BackendType::Wgpu,
        tty_backend: None,
        winit_backend: None,
        event_loop: None,
        scene: Scene::new(width as f32, height as f32),
        frame_glyphs: FrameGlyphBuffer::with_size(width as f32, height as f32),
        use_hybrid: true,
        animations: AnimationManager::new(),
        current_row_y: -1,
        current_row_x: 0,
        current_row_height: 0,
        current_row_ascent: 0,
        current_row_is_overlay: false,
        current_window_id: -1,
        current_window_x: 0.0,
        current_window_width: 0.0,
        in_frame: false,
        frame_counter: 0,
        current_render_window_id: 0,
        faces: HashMap::new(),
    });
    let display_ptr = Box::into_raw(display);

    *std::ptr::addr_of_mut!(THREADED_STATE) = Some(ThreadedState {
        emacs_comms,
        render_thread: Some(render_thread),
        display_handle: display_ptr,
        image_dimensions,
        shared_monitors,
        #[cfg(feature = "neo-term")]
        shared_terminals,
    });

    wakeup_fd
}

// ============================================================================
// Monitor Info FFI
// ============================================================================

/// Monitor info struct for C FFI
#[repr(C)]
pub struct NeomacsMonitorInfo {
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
    pub scale: c_double,
    pub width_mm: c_int,
    pub height_mm: c_int,
}

/// Wait for monitor info to be available (with timeout).
/// Call after neomacs_display_init_threaded().
/// Returns number of monitors, or 0 on timeout.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_wait_for_monitors() -> c_int {
    let state = match threaded_state() {
        Some(s) => s,
        None => return 0,
    };
    let (ref lock, ref cvar) = *state.shared_monitors;
    let timeout = std::time::Duration::from_secs(5);
    match lock.lock() {
        Ok(guard) => {
            // Wait until monitors are populated or timeout
            let result = cvar.wait_timeout_while(guard, timeout, |m| m.is_empty());
            match result {
                Ok((monitors, _)) => monitors.len() as c_int,
                Err(_) => 0,
            }
        }
        Err(_) => 0,
    }
}

/// Get the number of monitors available.
/// Must be called after neomacs_display_init_threaded().
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_monitor_count() -> c_int {
    let state = match threaded_state() {
        Some(s) => s,
        None => return 0,
    };
    let (ref lock, _) = *state.shared_monitors;
    match lock.lock() {
        Ok(monitors) => monitors.len() as c_int,
        Err(_) => 0,
    }
}

/// Get info about a specific monitor by index.
/// Returns 1 on success, 0 on failure.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_monitor_info(
    index: c_int,
    info: *mut NeomacsMonitorInfo,
) -> c_int {
    if info.is_null() {
        return 0;
    }
    let state = match threaded_state() {
        Some(s) => s,
        None => return 0,
    };
    let (ref lock, _) = *state.shared_monitors;
    match lock.lock() {
        Ok(monitors) => {
            if index < 0 || index as usize >= monitors.len() {
                return 0;
            }
            let m = &monitors[index as usize];
            (*info).x = m.x as c_int;
            (*info).y = m.y as c_int;
            (*info).width = m.width as c_int;
            (*info).height = m.height as c_int;
            (*info).scale = m.scale;
            (*info).width_mm = m.width_mm as c_int;
            (*info).height_mm = m.height_mm as c_int;
            1
        }
        Err(_) => 0,
    }
}

/// Get the name of a monitor by index.
/// Returns a pointer to a static string (valid until next call), or NULL.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_monitor_name(
    index: c_int,
) -> *const c_char {
    static mut NAME_BUF: Option<CString> = None;

    let state = match threaded_state() {
        Some(s) => s,
        None => return ptr::null(),
    };
    let (ref lock, _) = *state.shared_monitors;
    match lock.lock() {
        Ok(monitors) => {
            if index < 0 || index as usize >= monitors.len() {
                return ptr::null();
            }
            match &monitors[index as usize].name {
                Some(name) => {
                    *std::ptr::addr_of_mut!(NAME_BUF) = CString::new(name.as_str()).ok();
                    match (*std::ptr::addr_of!(NAME_BUF)).as_ref() {
                        Some(cs) => cs.as_ptr(),
                        None => ptr::null(),
                    }
                }
                None => ptr::null(),
            }
        }
        Err(_) => ptr::null(),
    }
}

// ============================================================================
// Event Draining
// ============================================================================

/// Drain input events from render thread
///
/// Returns number of events written to buffer.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_drain_input(
    events: *mut NeomacsInputEvent,
    max_events: c_int,
) -> c_int {
    let state = match threaded_state() {
        Some(s) => s,
        None => return 0,
    };

    // Clear wakeup pipe
    state.emacs_comms.wakeup_clear.clear();

    let mut count = 0;
    while count < max_events {
        match state.emacs_comms.input_rx.try_recv() {
            Ok(event) => {
                let out = &mut *events.add(count as usize);
                *out = NeomacsInputEvent::default();

                match event {
                    InputEvent::Key {
                        keysym,
                        modifiers,
                        pressed,
                    } => {
                        out.kind = if pressed {
                            NEOMACS_EVENT_KEY_PRESS
                        } else {
                            NEOMACS_EVENT_KEY_RELEASE
                        };
                        out.keysym = keysym;
                        out.modifiers = modifiers;
                    }
                    InputEvent::MouseButton {
                        button,
                        x,
                        y,
                        pressed,
                        modifiers,
                        target_frame_id,
                    } => {
                        out.kind = if pressed {
                            NEOMACS_EVENT_BUTTON_PRESS
                        } else {
                            NEOMACS_EVENT_BUTTON_RELEASE
                        };
                        out.x = x as i32;
                        out.y = y as i32;
                        out.button = button;
                        out.modifiers = modifiers;
                        out.target_frame_id = target_frame_id;
                    }
                    InputEvent::MouseMove { x, y, modifiers, target_frame_id } => {
                        out.kind = NEOMACS_EVENT_MOUSE_MOVE;
                        out.x = x as i32;
                        out.y = y as i32;
                        out.modifiers = modifiers;
                        out.target_frame_id = target_frame_id;
                    }
                    InputEvent::MouseScroll {
                        delta_x,
                        delta_y,
                        x,
                        y,
                        modifiers,
                        pixel_precise,
                        target_frame_id,
                    } => {
                        out.kind = NEOMACS_EVENT_SCROLL;
                        out.x = x as i32;
                        out.y = y as i32;
                        out.scroll_delta_x = delta_x;
                        out.scroll_delta_y = delta_y;
                        out.modifiers = modifiers;
                        out.pixel_precise = if pixel_precise { 1 } else { 0 };
                        out.target_frame_id = target_frame_id;
                    }
                    InputEvent::WindowResize { width, height, emacs_frame_id } => {
                        out.kind = NEOMACS_EVENT_RESIZE;
                        out.width = width;
                        out.height = height;
                        out.target_frame_id = emacs_frame_id;
                    }
                    InputEvent::WindowClose { emacs_frame_id } => {
                        out.kind = NEOMACS_EVENT_CLOSE;
                        out.target_frame_id = emacs_frame_id;
                    }
                    InputEvent::WindowFocus { focused, emacs_frame_id } => {
                        out.kind = if focused {
                            NEOMACS_EVENT_FOCUS_IN
                        } else {
                            NEOMACS_EVENT_FOCUS_OUT
                        };
                        out.target_frame_id = emacs_frame_id;
                    }
                    InputEvent::ImageDimensionsReady { id, width, height } => {
                        out.kind = NEOMACS_EVENT_IMAGE_DIMENSIONS_READY;
                        out.window_id = id;  // Reuse window_id field for image_id
                        out.width = width;
                        out.height = height;
                    }
                    // WebKit events are handled separately via callbacks
                    #[cfg(feature = "wpe-webkit")]
                    InputEvent::WebKitTitleChanged { .. }
                    | InputEvent::WebKitUrlChanged { .. }
                    | InputEvent::WebKitProgressChanged { .. }
                    | InputEvent::WebKitLoadFinished { .. } => {
                        // Skip these in the event queue - they're handled via webkit-specific API
                        continue;
                    }
                    // Terminal events
                    #[cfg(feature = "neo-term")]
                    InputEvent::TerminalExited { id } => {
                        out.kind = NEOMACS_EVENT_TERMINAL_EXITED;
                        out.keysym = id;  // reuse keysym field for terminal ID
                    }
                    #[cfg(feature = "neo-term")]
                    InputEvent::TerminalTitleChanged { id, title } => {
                        out.kind = NEOMACS_EVENT_TERMINAL_TITLE_CHANGED;
                        out.keysym = id;
                        if let Ok(mut queue) = TERMINAL_TITLES.lock() {
                            queue.push((id, title));
                        }
                    }
                    InputEvent::MenuSelection { index } => {
                        out.kind = NEOMACS_EVENT_MENU_SELECTION;
                        out.x = index;
                        // y field unused, set to 0
                    }
                    InputEvent::FileDrop { paths, x, y } => {
                        out.kind = NEOMACS_EVENT_FILE_DROP;
                        out.x = x as i32;
                        out.y = y as i32;
                        // Store paths in global queue for C to retrieve
                        if let Ok(mut queue) = DROPPED_FILES.lock() {
                            queue.push(paths);
                        }
                    }
                }
                count += 1;
            }
            Err(_) => break,
        }
    }

    count
}

// ============================================================================
// Dropped Files and Terminal Titles
// ============================================================================

/// Get the next batch of dropped file paths.
/// Returns the number of paths written.  Each path is a null-terminated
/// C string that must be freed with `neomacs_clipboard_free_text`.
/// Call repeatedly until it returns 0 to drain all pending drops.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_dropped_files(
    out_paths: *mut *mut c_char,
    max_paths: c_int,
) -> c_int {
    let batch = {
        let mut queue = match DROPPED_FILES.lock() {
            Ok(q) => q,
            Err(_) => return 0,
        };
        if queue.is_empty() {
            return 0;
        }
        queue.remove(0)
    };

    let mut count = 0;
    for path in batch {
        if count >= max_paths {
            break;
        }
        if let Ok(cstr) = std::ffi::CString::new(path) {
            *out_paths.add(count as usize) = cstr.into_raw();
            count += 1;
        }
    }
    count
}

/// Free a string returned by `neomacs_display_get_dropped_files`.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_free_dropped_path(path: *mut c_char) {
    if !path.is_null() {
        drop(std::ffi::CString::from_raw(path));
    }
}

/// Get the terminal title from the most recent title change event.
/// Returns a C string that must be freed with
/// `neomacs_display_free_dropped_path` (same allocator), or NULL.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_terminal_title(
    terminal_id: u32,
) -> *mut c_char {
    let mut queue = match TERMINAL_TITLES.lock() {
        Ok(q) => q,
        Err(_) => return std::ptr::null_mut(),
    };
    // Find and remove the first entry matching terminal_id
    if let Some(pos) = queue.iter().position(|(id, _)| *id == terminal_id) {
        let (_id, title) = queue.remove(pos);
        match std::ffi::CString::new(title) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => std::ptr::null_mut(),
        }
    } else {
        std::ptr::null_mut()
    }
}

// ============================================================================
// Frame / Command Sending
// ============================================================================

/// Send frame glyphs to render thread
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_send_frame(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &*handle;

    let state = match threaded_state() {
        Some(s) => s,
        None => return,
    };

    // Clone frame glyphs and send to render thread
    let frame = display.frame_glyphs.clone();
    let _ = state.emacs_comms.frame_tx.try_send(frame);
}

/// Send command to render thread
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_send_command(
    cmd_type: c_int,
    id: u32,
    param1: u32,
    param2: u32,
    str_param: *const c_char,
) {
    let state = match threaded_state() {
        Some(s) => s,
        None => return,
    };

    let cmd = match cmd_type {
        0 => RenderCommand::Shutdown,
        1 => RenderCommand::WebKitCreate {
            id,
            width: param1,
            height: param2,
        },
        2 => {
            let url = if str_param.is_null() {
                String::new()
            } else {
                CStr::from_ptr(str_param).to_string_lossy().into_owned()
            };
            RenderCommand::WebKitLoadUri { id, url }
        }
        3 => RenderCommand::WebKitResize {
            id,
            width: param1,
            height: param2,
        },
        4 => RenderCommand::WebKitDestroy { id },
        _ => return,
    };

    let _ = state.emacs_comms.cmd_tx.try_send(cmd);
}

// ============================================================================
// Shutdown / Handle Access
// ============================================================================

/// Shutdown threaded display
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_shutdown_threaded() {
    if let Some(mut state) = (*std::ptr::addr_of_mut!(THREADED_STATE)).take() {
        // Send shutdown command
        let _ = state.emacs_comms.cmd_tx.try_send(RenderCommand::Shutdown);

        // Wait for render thread
        if let Some(rt) = state.render_thread.take() {
            rt.join();
        }

        // Free the display handle
        if !state.display_handle.is_null() {
            let _ = Box::from_raw(state.display_handle);
        }
    }
}

/// Get wakeup fd for threaded mode (for Emacs to select() on)
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_threaded_wakeup_fd() -> c_int {
    match threaded_state() {
        Some(state) => state.emacs_comms.wakeup_read_fd,
        None => -1,
    }
}

/// Get display handle for threaded mode
///
/// Returns the NeomacsDisplay handle for use with frame operations.
/// Returns NULL if threaded mode is not initialized.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_threaded_handle() -> *mut NeomacsDisplay {
    match threaded_state() {
        Some(state) => state.display_handle,
        None => std::ptr::null_mut(),
    }
}

//! C FFI layer for integration with Emacs.
//!
//! Enable logging with: RUST_LOG=neomacs_display=debug

use std::ffi::{c_char, c_int, c_uint, c_double, c_void, CStr, CString};
use std::panic;
use std::ptr;

use log::{debug, trace, warn, info, error};

use crate::backend::{BackendType, DisplayBackend};

// ============================================================================
// Event Callback for FFI
// ============================================================================

#[cfg(feature = "winit-backend")]
use crate::backend::wgpu::{
    NeomacsInputEvent, WinitBackend,
    NEOMACS_EVENT_KEY_PRESS, NEOMACS_EVENT_KEY_RELEASE,
    NEOMACS_EVENT_BUTTON_PRESS, NEOMACS_EVENT_BUTTON_RELEASE,
    NEOMACS_EVENT_SCROLL, NEOMACS_EVENT_RESIZE, NEOMACS_EVENT_CLOSE,
};

/// Event callback function type for C FFI
#[cfg(feature = "winit-backend")]
type EventCallback = extern "C" fn(*const NeomacsInputEvent);

/// Global event callback - set by C code to receive input events
#[cfg(feature = "winit-backend")]
static mut EVENT_CALLBACK: Option<EventCallback> = None;
use crate::backend::tty::TtyBackend;
use crate::core::types::{Color, Rect};
use crate::core::scene::{Scene, WindowScene, CursorState, CursorStyle};
use crate::core::glyph::{Glyph, GlyphRow, GlyphType, GlyphData};
use crate::core::animation::AnimationManager;
use crate::core::frame_glyphs::{FrameGlyphBuffer, FrameGlyph};

/// Opaque handle to the display engine
pub struct NeomacsDisplay {
    backend_type: BackendType,
    tty_backend: Option<TtyBackend>,
    #[cfg(feature = "winit-backend")]
    winit_backend: Option<WinitBackend>,
    #[cfg(feature = "winit-backend")]
    event_loop: Option<winit::event_loop::EventLoop<crate::backend::wgpu::UserEvent>>,
    scene: Scene,           // The scene for rendering (legacy)
    frame_glyphs: FrameGlyphBuffer,  // Hybrid approach: direct glyph buffer
    use_hybrid: bool,       // Whether to use hybrid rendering (default: true)
    animations: AnimationManager,
    current_row_y: i32,     // Y position of current row being built
    current_row_x: i32,     // X position for next glyph in current row
    current_row_height: i32, // Height of current row
    current_row_ascent: i32, // Ascent of current row
    current_row_is_overlay: bool, // True if current row is mode-line/echo area
    current_window_id: i32, // ID of current window being updated
    in_frame: bool,         // Whether we're currently in a frame update
    frame_counter: u64,     // Frame counter for tracking row updates
    current_render_window_id: u32, // Winit window ID being rendered to (0 = legacy rendering)
}

impl NeomacsDisplay {
    fn get_backend(&mut self) -> Option<&mut dyn DisplayBackend> {
        match self.backend_type {
            BackendType::Tty => self.tty_backend.as_mut().map(|b| b as &mut dyn DisplayBackend),
            #[cfg(feature = "winit-backend")]
            BackendType::Wgpu => self.winit_backend.as_mut().map(|b| b as &mut dyn DisplayBackend),
        }
    }

    /// Get the scene to render to based on current_render_window_id.
    /// Returns the winit window's scene if rendering to a window,
    /// otherwise returns the legacy scene.
    fn get_target_scene(&mut self) -> &mut Scene {
        if self.current_render_window_id > 0 {
            #[cfg(feature = "winit-backend")]
            if let Some(ref mut backend) = self.winit_backend {
                if let Some(scene) = backend.get_scene_mut(self.current_render_window_id) {
                    return scene;
                }
            }
        }
        &mut self.scene
    }
}

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the display engine
///
/// # Safety
/// Returns a pointer to NeomacsDisplay that must be freed with neomacs_display_shutdown.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_init(backend: BackendType) -> *mut NeomacsDisplay {
    // Initialize logger (only once, errors if called multiple times)
    let _ = env_logger::try_init();

    // Check environment variable for hybrid mode (default: enabled)
    let use_hybrid = std::env::var("NEOMACS_HYBRID")
        .map(|v| v != "0")
        .unwrap_or(true);

    if use_hybrid {
        info!("Using HYBRID rendering mode");
    } else {
        info!("Using LEGACY scene graph mode");
    }

    let mut display = Box::new(NeomacsDisplay {
        backend_type: backend,
        tty_backend: None,
        #[cfg(feature = "winit-backend")]
        winit_backend: None,
        #[cfg(feature = "winit-backend")]
        event_loop: None,
        scene: Scene::new(800.0, 600.0),
        frame_glyphs: FrameGlyphBuffer::with_size(800.0, 600.0),  // Match initial scene size
        use_hybrid,
        animations: AnimationManager::new(),
        current_row_y: -1,
        current_row_x: 0,
        current_row_height: 0,
        current_row_ascent: 0,
        current_row_is_overlay: false,
        current_window_id: -1,
        in_frame: false,
        frame_counter: 0,
        current_render_window_id: 0, // 0 = legacy rendering
    });

    // Create the backend
    match backend {
        BackendType::Tty => {
            let mut tty = TtyBackend::new();
            if let Err(e) = tty.init() {
                eprintln!("Failed to initialize TTY backend: {}", e);
                return ptr::null_mut();
            }
            display.tty_backend = Some(tty);
        }
        #[cfg(feature = "winit-backend")]
        BackendType::Wgpu => {
            use winit::event_loop::EventLoop;

            // Create the event loop first
            let event_loop = match EventLoop::<crate::backend::wgpu::UserEvent>::with_user_event()
                .build()
            {
                Ok(el) => el,
                Err(e) => {
                    eprintln!("Failed to create event loop: {}", e);
                    return ptr::null_mut();
                }
            };

            let mut winit = WinitBackend::new();
            if let Err(e) = winit.init() {
                eprintln!("Failed to initialize Winit/wgpu backend: {}", e);
                return ptr::null_mut();
            }
            // Initialize wgpu in headless mode so we can create windows later
            if let Err(e) = winit.init_wgpu_headless() {
                eprintln!("Failed to initialize wgpu: {}", e);
                return ptr::null_mut();
            }

            // Store the event loop proxy for async notifications
            winit.set_event_loop_proxy(event_loop.create_proxy());

            display.winit_backend = Some(winit);
            display.event_loop = Some(event_loop);
        }
    }

    Box::into_raw(display)
}

/// Shutdown the display engine
///
/// # Safety
/// The handle must have been returned by neomacs_display_init.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_shutdown(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let mut display = Box::from_raw(handle);

    if let Some(backend) = display.get_backend() {
        backend.shutdown();
    }

    // display is dropped here
}

// ============================================================================
// Scene Management
// ============================================================================

/// Resize the display
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_resize(
    handle: *mut NeomacsDisplay,
    width: c_int,
    height: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.scene = Scene::new(width as f32, height as f32);

    // Update frame_glyphs buffer size and CLEAR it for fresh redraw
    display.frame_glyphs.width = width as f32;
    display.frame_glyphs.height = height as f32;
    display.frame_glyphs.glyphs.clear();  // Clear all glyphs - Emacs will resend
    display.frame_glyphs.window_regions.clear();
    display.frame_glyphs.prev_window_regions.clear();

    if let Some(backend) = display.get_backend() {
        backend.resize(width as u32, height as u32);
    }
}

/// Begin building a new frame
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_begin_frame(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    // Increment frame counter - used to track which rows need clearing
    display.frame_counter += 1;
    // Mark that we're in a frame update cycle
    display.in_frame = true;

    debug!("begin_frame: frame={}, hybrid={}, glyphs={}",
           display.frame_counter, display.use_hybrid, display.frame_glyphs.len());

    // DON'T clear glyphs - accumulate them for incremental redisplay.
    // Emacs sends only changed content; old content is retained.
    // When add_char is called, it removes overlapping old glyphs.
    if display.use_hybrid {
        display.frame_glyphs.width = display.scene.width;
        display.frame_glyphs.height = display.scene.height;
        display.frame_glyphs.background = display.scene.background;
        // Start frame - saves previous window regions for layout change detection
        display.frame_glyphs.start_frame();
    }

    // NOTE: Don't clear rows here - Emacs does incremental updates
    // Rows will be cleared individually when begin_row is called
}

/// Add a window to the current frame
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_add_window(
    handle: *mut NeomacsDisplay,
    window_id: c_int,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    bg_color: u32,
    selected: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    let current_frame = display.frame_counter;

    // Track current window for subsequent glyph operations
    display.current_window_id = window_id;

    // Hybrid path: just add window background rectangle
    if display.use_hybrid {
        display.frame_glyphs.add_background(
            x, y, width, height,
            Color::from_pixel(bg_color),
        );
        return;
    }

    // Legacy scene graph path...
    // Find existing window by ID or create new one
    let window_idx = display.get_target_scene().windows.iter().position(|w| w.window_id == window_id);

    if let Some(idx) = window_idx {
        // Update existing window
        let window = &mut display.get_target_scene().windows[idx];

        // Check if bounds changed
        let old_height = window.bounds.height;
        let new_height = height;

        window.bounds = Rect::new(x, y, width, height);
        window.background = Color::from_pixel(bg_color);
        window.selected = selected != 0;
        window.last_frame_touched = current_frame;

        // If window got smaller, remove rows that are now outside bounds
        // Row Y is window-relative (0 to height), so rows outside [0, height) should be removed
        if new_height < old_height {
            let max_row_y = height as i32;
            window.rows.retain(|row| {
                let row_bottom = row.y + row.height;
                row.y >= 0 && row_bottom <= max_row_y
            });
        }
    } else {
        // Create new window
        let window = WindowScene {
            window_id,
            bounds: Rect::new(x, y, width, height),
            background: Color::from_pixel(bg_color),
            rows: Vec::new(),
            cursor: None,
            scroll_offset: 0.0,
            selected: selected != 0,
            mode_line_height: 0,
            header_line_height: 0,
            last_frame_touched: current_frame,
        };
        display.get_target_scene().windows.push(window);
    }
}

/// Set cursor for a specific window
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_cursor(
    handle: *mut NeomacsDisplay,
    window_id: c_int,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    style: c_int,
    color: u32,
    visible: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;

    // Hybrid path: add cursor directly to glyph buffer
    if display.use_hybrid {
        if visible != 0 && display.animations.cursor_visible() {
            // style: 0=box, 1=bar, 2=underline, 3=hollow
            display.frame_glyphs.add_cursor(
                window_id,
                x, y, width, height,
                style as u8,
                Color::from_pixel(color),
            );
        }
        return;
    }

    // Legacy scene graph path...
    // Compute cursor visibility before borrowing target scene
    let cursor_visible = visible != 0 && display.animations.cursor_visible();

    // Find the window by ID
    if let Some(window) = display.get_target_scene().windows.iter_mut().find(|w| w.window_id == window_id) {
        window.cursor = Some(CursorState {
            x,
            y,
            width,
            height,
            style: match style {
                0 => CursorStyle::Box,
                1 => CursorStyle::Bar,
                2 => CursorStyle::Underline,
                3 => CursorStyle::Hollow,
                _ => CursorStyle::Box,
            },
            color: Color::from_pixel(color),
            visible: cursor_visible,
        });
    }
}

/// Draw a border rectangle (for window dividers)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_draw_border(
    handle: *mut NeomacsDisplay,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    color: u32,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;

    // Hybrid path: add border directly to glyph buffer
    if display.use_hybrid {
        display.frame_glyphs.add_border(
            x as f32, y as f32,
            width as f32, height as f32,
            Color::from_pixel(color),
        );
        return;
    }

    // Legacy path
    display.get_target_scene().add_border(
        x as f32,
        y as f32,
        width as f32,
        height as f32,
        Color::from_pixel(color),
    );
}

// ============================================================================
// Glyph Row Management
// ============================================================================

/// Begin a new glyph row for the current window
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_begin_row(
    handle: *mut NeomacsDisplay,
    y: c_int,  // Frame-absolute Y coordinate
    x: c_int,  // Starting X position for this glyph string
    height: c_int,
    ascent: c_int,
    mode_line: c_int,
    header_line: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    let current_frame = display.frame_counter;
    let current_window_id = display.current_window_id;

    // Track current row Y (frame-absolute) and X for glyph additions
    display.current_row_y = y;
    display.current_row_x = x;  // Set starting X for this glyph string
    display.current_row_height = height;  // Store for hybrid path
    display.current_row_ascent = ascent;  // Store for hybrid path
    // Mode-line and header-line are overlays that render on top
    display.current_row_is_overlay = mode_line != 0 || header_line != 0;

    // Hybrid path: we don't need window tracking - just use frame-absolute coords
    if display.use_hybrid {
        // Nothing else needed - glyphs will use current_row_y/x/height/ascent directly
        return;
    }

    // Legacy scene graph path below...
    // Find the current window by ID
    let target_scene = display.get_target_scene();
    let window = target_scene.windows
        .iter_mut()
        .find(|w| w.window_id == current_window_id);

    let window = if let Some(w) = window {
        w
    } else {
        // Create default window if none exists for this ID
        // Use scene background (dark by default) instead of white
        target_scene.windows.push(crate::core::scene::WindowScene {
            window_id: current_window_id,
            bounds: crate::core::Rect::new(0.0, 0.0,
                target_scene.width as f32, target_scene.height as f32),
            background: target_scene.background, // Match scene background
            rows: Vec::new(),
            cursor: None,
            scroll_offset: 0.0,
            selected: true,
            mode_line_height: 0,
            header_line_height: 0,
            last_frame_touched: current_frame,
        });
        target_scene.windows.last_mut().unwrap()
    };

    // Convert frame-absolute Y to window-relative Y
    let window_y = window.bounds.y as i32;
    let relative_y = y - window_y;

    // Look for existing row at this Y position within this window (using window-relative Y)
    let is_mode_line = mode_line != 0;
    let is_header_line = header_line != 0;

    if let Some(existing_row) = window.rows.iter_mut().find(|r| r.y == relative_y) {
        // If row type changed (mode_line <-> content), clear all glyphs
        // to avoid stale content from previous row type
        if existing_row.mode_line != is_mode_line || existing_row.header_line != is_header_line {
            existing_row.glyphs.clear();
        }

        // Update the properties
        existing_row.height = height;
        existing_row.visible_height = height;
        existing_row.ascent = ascent;
        existing_row.mode_line = is_mode_line;
        existing_row.header_line = is_header_line;
        existing_row.last_frame_touched = current_frame;
    } else {
        // Add new row to this window (with window-relative Y)
        window.rows.push(GlyphRow {
            glyphs: Vec::new(),
            y: relative_y,
            height,
            visible_height: height,
            ascent,
            enabled: true,
            cursor_in_row: false,
            mode_line: is_mode_line,
            header_line: is_header_line,
            last_frame_cleared: current_frame,
            last_frame_touched: current_frame,
        });
    }
}

/// Add a character glyph to the current row
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_add_char_glyph(
    handle: *mut NeomacsDisplay,
    charcode: u32,
    face_id: u32,
    pixel_width: c_int,
    ascent: c_int,
    descent: c_int,
) {
    if handle.is_null() {
        return;
    }

    // Catch panics to prevent aborting across FFI boundary
    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let display = &mut *handle;
        let current_y = display.current_row_y;  // Frame-absolute Y
        let current_x = display.current_row_x;
        let c = char::from_u32(charcode).unwrap_or('\u{FFFD}');

        // Hybrid path: append directly to frame glyph buffer
        if display.use_hybrid {
            display.frame_glyphs.add_char(
                c,
                current_x as f32,
                current_y as f32,
                pixel_width as f32,
                display.current_row_height as f32,
                display.current_row_ascent as f32,
                display.current_row_is_overlay,
            );
            display.current_row_x += pixel_width;
            return;
        }

        // Legacy scene graph path...
        let current_window_id = display.current_window_id;

        // Find the correct window by ID
        if let Some(window) = display.get_target_scene().windows.iter_mut().find(|w| w.window_id == current_window_id) {
            // Convert frame-absolute Y to window-relative Y
            let relative_y = current_y - window.bounds.y as i32;
            // Convert frame-absolute X to window-relative X
            let relative_x = current_x - window.bounds.x as i32;

            if let Some(row) = window.rows.iter_mut().find(|r| r.y == relative_y) {
                // Remove any existing glyphs that overlap this X range (using window-relative X)
                let x_start = relative_x;
                let x_end = relative_x + pixel_width;
                row.glyphs.retain(|g| {
                    // Keep glyphs that don't overlap with our new glyph's X range
                    let g_end = g.x + g.pixel_width;
                    g_end <= x_start || g.x >= x_end
                });

                let glyph = Glyph {
                    glyph_type: GlyphType::Char,
                    charcode,
                    face_id,
                    x: relative_x,  // Use window-relative X
                    pixel_width,
                    ascent,
                    descent,
                    charpos: 0,
                    left_box_line: false,
                    right_box_line: false,
                    padding: false,
                    data: GlyphData::Char {
                        code: c,
                    },
                };
                row.glyphs.push(glyph);

                // Advance X position for next glyph (keep as frame-absolute for C code)
                display.current_row_x += pixel_width;
            }
        }
    }));

    if let Err(e) = result {
        eprintln!("PANIC in neomacs_display_add_char_glyph: {:?}", e);
    }
}

/// Add a stretch (whitespace) glyph to the current row
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_add_stretch_glyph(
    handle: *mut NeomacsDisplay,
    pixel_width: c_int,
    height: c_int,
    face_id: u32,
) {
    if handle.is_null() {
        return;
    }

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let display = &mut *handle;
        let current_y = display.current_row_y;  // Frame-absolute Y
        let current_x = display.current_row_x;

        // Hybrid path: append directly to frame glyph buffer
        if display.use_hybrid {
            // Get the background color from the current face
            let bg_color = display.frame_glyphs.get_current_bg()
                .unwrap_or(display.frame_glyphs.background);

            display.frame_glyphs.add_stretch(
                current_x as f32,
                current_y as f32,
                pixel_width as f32,
                height as f32,
                bg_color,
                display.current_row_is_overlay,
            );
            display.current_row_x += pixel_width;
            return;
        }

        // Legacy scene graph path...
        let current_window_id = display.current_window_id;

        // Find the correct window by ID
        if let Some(window) = display.get_target_scene().windows.iter_mut().find(|w| w.window_id == current_window_id) {
            // Convert frame-absolute Y to window-relative Y
            let relative_y = current_y - window.bounds.y as i32;
            // Convert frame-absolute X to window-relative X
            let relative_x = current_x - window.bounds.x as i32;

            if let Some(row) = window.rows.iter_mut().find(|r| r.y == relative_y) {
                // Remove any existing glyphs that overlap this X range (using window-relative X)
                let x_start = relative_x;
                let x_end = relative_x + pixel_width;
                row.glyphs.retain(|g| {
                    let g_end = g.x + g.pixel_width;
                    g_end <= x_start || g.x >= x_end
                });

                let glyph = Glyph {
                    glyph_type: GlyphType::Stretch,
                    charcode: 0,
                    face_id,
                    x: relative_x,  // Use window-relative X
                    pixel_width,
                    ascent: height,
                    descent: 0,
                    charpos: 0,
                    left_box_line: false,
                    right_box_line: false,
                    padding: false,
                    data: GlyphData::Stretch { width: pixel_width },
                };
                row.glyphs.push(glyph);

                // Advance X position (keep as frame-absolute for C code)
                display.current_row_x += pixel_width;
            }
        }
    }));

    if let Err(e) = result {
        eprintln!("PANIC in neomacs_display_add_stretch_glyph: {:?}", e);
    }
}

/// Add an image glyph to the current row
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_add_image_glyph(
    handle: *mut NeomacsDisplay,
    image_id: u32,
    pixel_width: c_int,
    pixel_height: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    let current_y = display.current_row_y;  // Frame-absolute Y
    let current_x = display.current_row_x;

    // Hybrid path: append directly to frame glyph buffer
    if display.use_hybrid {
        display.frame_glyphs.add_image(
            image_id,
            current_x as f32,
            current_y as f32,
            pixel_width as f32,
            pixel_height as f32,
        );
        display.current_row_x += pixel_width;
        return;
    }

    // Legacy scene graph path
    let current_window_id = display.current_window_id;

    // Find the correct window by ID
    if let Some(window) = display.get_target_scene().windows.iter_mut().find(|w| w.window_id == current_window_id) {
        // Convert frame-absolute Y to window-relative Y
        let relative_y = current_y - window.bounds.y as i32;
        // Convert frame-absolute X to window-relative X
        let relative_x = current_x - window.bounds.x as i32;

        if let Some(row) = window.rows.iter_mut().find(|r| r.y == relative_y) {
            // Remove overlapping glyphs (using window-relative X)
            let x_start = relative_x;
            let x_end = relative_x + pixel_width;
            row.glyphs.retain(|g| {
                let g_end = g.x + g.pixel_width;
                g_end <= x_start || g.x >= x_end
            });

            let glyph = Glyph {
                glyph_type: GlyphType::Image,
                charcode: 0,
                face_id: 0,
                x: relative_x,  // Use window-relative X
                pixel_width,
                ascent: pixel_height,
                descent: 0,
                charpos: 0,
                left_box_line: false,
                right_box_line: false,
                padding: false,
                data: GlyphData::Image { image_id },
            };
            row.glyphs.push(glyph);

            // Advance X position (keep as frame-absolute for C code)
            display.current_row_x += pixel_width;
        }
    }
}

/// End the current row
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_end_row(handle: *mut NeomacsDisplay) {
    // Currently a no-op, but could be used for row finalization
    let _ = handle;
}

// ============================================================================
// Face Management
// ============================================================================

use crate::core::face::{Face, FaceAttributes, UnderlineStyle, BoxType};

/// Register or update a face
/// Colors are in 0xRRGGBB format
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_face(
    handle: *mut NeomacsDisplay,
    face_id: u32,
    foreground: u32,  // 0xRRGGBB
    background: u32,  // 0xRRGGBB
    font_family: *const c_char, // Font family name (e.g., "monospace", "Sans")
    font_weight: u16, // 400=normal, 700=bold
    is_italic: c_int,
    underline_style: c_int, // 0=none, 1=line, 2=wave, 3=double, 4=dotted, 5=dashed
    underline_color: u32,
    box_type: c_int,  // 0=none, 1=line, 2=raised3d, 3=sunken3d
    box_color: u32,
    box_line_width: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;

    // Convert font family from C string (defensively)
    let font_family_str = if font_family.is_null() {
        "monospace".to_string()
    } else {
        // Safety: need to verify the C string is valid
        match std::ffi::CStr::from_ptr(font_family).to_str() {
            Ok(s) if !s.is_empty() => s.to_string(),
            _ => "monospace".to_string(),
        }
    };

    trace!("set_face: id={}, fg=0x{:06x}, bg=0x{:06x}, family={}, weight={}", face_id, foreground, background, font_family_str, font_weight);

    // Convert colors from 0xRRGGBB to Color
    let fg = Color {
        r: ((foreground >> 16) & 0xFF) as f32 / 255.0,
        g: ((foreground >> 8) & 0xFF) as f32 / 255.0,
        b: (foreground & 0xFF) as f32 / 255.0,
        a: 1.0,
    };

    let bg = Color {
        r: ((background >> 16) & 0xFF) as f32 / 255.0,
        g: ((background >> 8) & 0xFF) as f32 / 255.0,
        b: (background & 0xFF) as f32 / 255.0,
        a: if background == 0 { 0.0 } else { 1.0 },
    };

    // Build attributes
    let mut attrs = FaceAttributes::empty();
    if font_weight >= 700 {
        attrs |= FaceAttributes::BOLD;
    }
    if is_italic != 0 {
        attrs |= FaceAttributes::ITALIC;
    }
    if underline_style != 0 {
        attrs |= FaceAttributes::UNDERLINE;
    }
    if box_type != 0 {
        attrs |= FaceAttributes::BOX;
    }

    // Underline style
    let ul_style = match underline_style {
        1 => UnderlineStyle::Line,
        2 => UnderlineStyle::Wave,
        3 => UnderlineStyle::Double,
        4 => UnderlineStyle::Dotted,
        5 => UnderlineStyle::Dashed,
        _ => UnderlineStyle::None,
    };

    // Box type
    let bx_type = match box_type {
        1 => BoxType::Line,
        2 => BoxType::Raised3D,
        3 => BoxType::Sunken3D,
        _ => BoxType::None,
    };

    // Underline color
    let ul_color = if underline_color != 0 {
        Some(Color {
            r: ((underline_color >> 16) & 0xFF) as f32 / 255.0,
            g: ((underline_color >> 8) & 0xFF) as f32 / 255.0,
            b: (underline_color & 0xFF) as f32 / 255.0,
            a: 1.0,
        })
    } else {
        None
    };

    // Box color
    let bx_color = if box_color != 0 {
        Some(Color {
            r: ((box_color >> 16) & 0xFF) as f32 / 255.0,
            g: ((box_color >> 8) & 0xFF) as f32 / 255.0,
            b: (box_color & 0xFF) as f32 / 255.0,
            a: 1.0,
        })
    } else {
        None
    };

    let face = Face {
        id: face_id,
        foreground: fg,
        background: bg,
        underline_color: ul_color,
        overline_color: None,
        strike_through_color: None,
        box_color: bx_color,
        font_family: font_family_str.clone(),
        font_size: 14.0,
        font_weight,
        attributes: attrs,
        underline_style: ul_style,
        box_type: bx_type,
        box_line_width,
    };

    // Hybrid path: set current face attributes for frame glyph buffer
    if display.use_hybrid {
        let bg_opt = if background == 0 { None } else { Some(bg) };
        let ul_color_opt = if underline_color != 0 { ul_color } else { None };
        display.frame_glyphs.set_face_with_font(
            face_id,
            fg,
            bg_opt,
            &font_family_str,
            font_weight >= 700,
            is_italic != 0,
            underline_style as u8,
            ul_color_opt,
        );
    }

    // Register face in the scene
    display.get_target_scene().set_face(face.clone());
}

/// Set the frame/scene background color
/// Color is in 0xRRGGBB format
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_background(
    handle: *mut NeomacsDisplay,
    color: u32,  // 0xRRGGBB
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    let bg = Color {
        r: ((color >> 16) & 0xFF) as f32 / 255.0,
        g: ((color >> 8) & 0xFF) as f32 / 255.0,
        b: (color & 0xFF) as f32 / 255.0,
        a: 1.0,
    };

    let target_scene = display.get_target_scene();
    target_scene.background = bg;

    // Also set background for existing windows
    for window in &mut target_scene.windows {
        window.background = bg;
    }
}

// ============================================================================
// Image Management (stubs - no GTK4 backend)
// ============================================================================

/// Add a video glyph to the current row
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_add_video_glyph(
    handle: *mut NeomacsDisplay,
    video_id: u32,
    pixel_width: c_int,
    pixel_height: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    let current_y = display.current_row_y;  // Frame-absolute Y
    let current_x = display.current_row_x;

    // Hybrid path: append directly to frame glyph buffer
    if display.use_hybrid {
        display.frame_glyphs.add_video(
            video_id,
            current_x as f32,
            current_y as f32,
            pixel_width as f32,
            pixel_height as f32,
        );
        display.current_row_x += pixel_width;
        return;
    }

    // Legacy scene graph path
    let current_window_id = display.current_window_id;

    // Find the correct window by ID
    if let Some(window) = display.get_target_scene().windows.iter_mut().find(|w| w.window_id == current_window_id) {
        // Convert frame-absolute Y to window-relative Y
        let relative_y = current_y - window.bounds.y as i32;
        // Convert frame-absolute X to window-relative X
        let relative_x = current_x - window.bounds.x as i32;

        if let Some(row) = window.rows.iter_mut().find(|r| r.y == relative_y) {
            // Remove overlapping glyphs (using window-relative X)
            let x_start = relative_x;
            let x_end = relative_x + pixel_width;
            row.glyphs.retain(|g| {
                let g_end = g.x + g.pixel_width;
                g_end <= x_start || g.x >= x_end
            });

            let glyph = Glyph {
                glyph_type: GlyphType::Video,
                charcode: 0,
                face_id: 0,
                x: relative_x,  // Use window-relative X
                pixel_width,
                ascent: pixel_height,
                descent: 0,
                charpos: 0,
                left_box_line: false,
                right_box_line: false,
                padding: false,
                data: GlyphData::Video { video_id },
            };
            row.glyphs.push(glyph);

            // Advance X position (keep as frame-absolute for C code)
            display.current_row_x += pixel_width;
        }
    }
}

/// Load a video from URI (stub - video not supported without GTK4)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_video(
    _handle: *mut NeomacsDisplay,
    _uri: *const c_char,
) -> u32 {
    0
}

/// Play a loaded video (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_play(
    _handle: *mut NeomacsDisplay,
    _video_id: u32,
) -> c_int {
    -1
}

/// Pause a video (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_pause(
    _handle: *mut NeomacsDisplay,
    _video_id: u32,
) -> c_int {
    -1
}

/// Stop a video (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_stop(
    _handle: *mut NeomacsDisplay,
    _video_id: u32,
) -> c_int {
    -1
}

/// Set video loop mode (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_set_loop(
    _handle: *mut NeomacsDisplay,
    _video_id: u32,
    _loop_count: c_int,
) -> c_int {
    -1
}

/// Update video frame (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_update(
    _handle: *mut NeomacsDisplay,
    _video_id: u32,
) -> c_int {
    -1
}

// ============================================================================
// Image Functions (stubs - no GTK4 backend)
// ============================================================================

/// Load an image from a file path (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image(
    _handle: *mut NeomacsDisplay,
    _path: *const c_char,
) -> u32 { 0 }

/// Load an image from raw bytes (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_data(
    _handle: *mut NeomacsDisplay,
    _data: *const u8,
    _len: usize,
) -> u32 { 0 }

/// Load an image from raw bytes with optional scaling (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_data_scaled(
    _handle: *mut NeomacsDisplay,
    _data: *const u8,
    _len: usize,
    _max_width: c_int,
    _max_height: c_int,
) -> u32 { 0 }

/// Load an image from raw ARGB32 pixel data (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_argb32(
    _handle: *mut NeomacsDisplay,
    _data: *const u8,
    _width: c_int,
    _height: c_int,
    _stride: c_int,
) -> u32 { 0 }

/// Load an image from raw RGB24 pixel data (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_rgb24(
    _handle: *mut NeomacsDisplay,
    _data: *const u8,
    _width: c_int,
    _height: c_int,
    _stride: c_int,
) -> u32 { 0 }

/// Load an image from a file path (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_file(
    _handle: *mut NeomacsDisplay,
    _path: *const c_char,
) -> u32 { 0 }

/// Load an image from a file path with scaling (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_file_scaled(
    _handle: *mut NeomacsDisplay,
    _path: *const c_char,
    _max_width: c_int,
    _max_height: c_int,
) -> u32 { 0 }

/// Load an image directly as texture (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_file_direct(
    _handle: *mut NeomacsDisplay,
    _path: *const c_char,
) -> u32 { 0 }

/// Load an image directly as texture with scaling (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_file_direct_scaled(
    _handle: *mut NeomacsDisplay,
    _path: *const c_char,
    _max_width: c_int,
    _max_height: c_int,
) -> u32 { 0 }

/// Get image dimensions (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_image_size(
    _handle: *mut NeomacsDisplay,
    _image_id: u32,
    _width: *mut c_int,
    _height: *mut c_int,
) -> c_int { -1 }

/// Query image file dimensions without loading (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_query_image_file_size(
    _handle: *mut NeomacsDisplay,
    _path: *const c_char,
    _width: *mut c_int,
    _height: *mut c_int,
) -> c_int {
    -1
}

/// Free an image from cache (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_free_image(
    _handle: *mut NeomacsDisplay,
    _image_id: u32,
) -> c_int { -1 }

/// Set a floating video at a specific screen position
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_floating_video(
    handle: *mut NeomacsDisplay,
    video_id: u32,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;

    // Remove existing floating video for this ID
    let target_scene = display.get_target_scene();
    target_scene.remove_floating_video(video_id);

    // Add new floating video
    target_scene.add_floating_video(
        video_id,
        x as f32,
        y as f32,
        width as f32,
        height as f32,
    );
}

/// Remove a floating video
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_floating_video(
    handle: *mut NeomacsDisplay,
    video_id: u32,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.get_target_scene().remove_floating_video(video_id);
}

/// Set a floating image at a specific screen position
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_floating_image(
    handle: *mut NeomacsDisplay,
    image_id: u32,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;

    // Remove existing floating image for this ID
    let target_scene = display.get_target_scene();
    target_scene.remove_floating_image(image_id);

    // Add new floating image
    target_scene.add_floating_image(
        image_id,
        x as f32,
        y as f32,
        width as f32,
        height as f32,
    );
}

/// Remove a floating image
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_floating_image(
    handle: *mut NeomacsDisplay,
    image_id: u32,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.get_target_scene().remove_floating_image(image_id);
}

/// Clear a rectangular area of the display
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_area(
    handle: *mut NeomacsDisplay,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;

    // For hybrid path, clear the area in frame_glyphs
    if display.use_hybrid {
        display.frame_glyphs.clear_area(
            x as f32,
            y as f32,
            width as f32,
            height as f32,
        );
    }
}

/// Clear all glyphs - used when frame layout changes
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_all_glyphs(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.frame_glyphs.glyphs.clear();
    display.frame_glyphs.window_regions.clear();
    display.frame_glyphs.prev_window_regions.clear();
}

/// Clear all cursors
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_all_cursors(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.frame_glyphs.glyphs.retain(|g| !matches!(g, FrameGlyph::Cursor { .. }));
}

/// Clear all borders (window dividers)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_all_borders(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.frame_glyphs.glyphs.retain(|g| !matches!(g, FrameGlyph::Border { .. }));
}

/// End frame and render
/// Returns 0 on success, 1 if layout changed, -1 on error
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_end_frame(handle: *mut NeomacsDisplay) -> c_int {
    if handle.is_null() {
        return -1;
    }

    let display = &mut *handle;
    let current_frame = display.frame_counter;

    // Reset frame flag
    display.in_frame = false;

    debug!("end_frame: frame={}, glyphs={}, regions={}",
           current_frame, display.frame_glyphs.len(), display.frame_glyphs.window_regions.len());

    // End frame - this handles layout change detection and stale glyph removal
    let mut layout_cleared = false;
    if display.use_hybrid {
        layout_cleared = display.frame_glyphs.end_frame();
        debug!("After end_frame: {} glyphs, cleared={}", display.frame_glyphs.len(), layout_cleared);
    }

    // Build scene if it has content
    let scene_rows: usize = display.scene.windows.iter().map(|w| w.rows.len()).sum();

    if scene_rows > 0 {
        // Build the scene graph
        display.scene.build();
    }

    // Update animations
    display.animations.tick();

    // Render - we need to match backend type explicitly to avoid borrow conflict
    let result = match display.backend_type {
        BackendType::Tty => {
            if let Some(backend) = display.tty_backend.as_mut() {
                backend.render(&display.scene)
                    .and_then(|_| backend.present())
            } else {
                Ok(())
            }
        }
        #[cfg(feature = "winit-backend")]
        BackendType::Wgpu => {
            if let Some(backend) = display.winit_backend.as_mut() {
                backend.render(&display.scene)
                    .and_then(|_| backend.present())
            } else {
                Ok(())
            }
        }
    };

    if let Err(e) = result {
        eprintln!("Render error: {}", e);
        return -1;
    }

    display.scene.clear_dirty();

    if layout_cleared { 1 } else { 0 }
}

/// Render the scene to an external Cairo context (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_render_to_cairo(
    _handle: *mut NeomacsDisplay,
    _cairo_context: *mut c_void,
) -> c_int {
    // Not implemented without GTK4
    -1
}

/// Initialize the renderer with a Pango context (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_init_pango(
    _handle: *mut NeomacsDisplay,
    _pango_context: *mut c_void,
) {
    // Not implemented without GTK4
}

/// Enable or disable GSK rendering (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_gsk_enabled(
    _handle: *mut NeomacsDisplay,
    _enabled: c_int,
) {
    // Not implemented without GTK4
}

// ============================================================================
// Animation
// ============================================================================

/// Start smooth scroll animation
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_smooth_scroll(
    handle: *mut NeomacsDisplay,
    window_id: c_int,
    from_offset: f32,
    to_offset: f32,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.animations.animate_scroll(window_id, from_offset, to_offset);
}

/// Reset cursor blink (call when cursor moves)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_reset_cursor_blink(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.animations.reset_cursor_blink();
}

/// Check if animations are active
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_has_animations(handle: *mut NeomacsDisplay) -> c_int {
    if handle.is_null() {
        return 0;
    }

    let display = &mut *handle;
    display.animations.has_active_animations() as c_int
}

// ============================================================================
// Backend Info
// ============================================================================

/// Get backend name
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_backend_name(handle: *mut NeomacsDisplay) -> *const c_char {
    if handle.is_null() {
        return b"null\0".as_ptr() as *const c_char;
    }

    let display = &mut *handle;

    match display.get_backend() {
        Some(backend) => backend.name().as_ptr() as *const c_char,
        None => b"none\0".as_ptr() as *const c_char,
    }
}

/// Check if backend is initialized
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_is_initialized(handle: *mut NeomacsDisplay) -> c_int {
    if handle.is_null() {
        return 0;
    }

    let display = &mut *handle;

    match display.get_backend() {
        Some(backend) => backend.is_initialized() as c_int,
        None => 0,
    }
}

// ============================================================================
// Widget Functions (stubs - no GTK4 backend)
// ============================================================================

/// Create a GPU-accelerated NeomacsWidget (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_create_widget() -> *mut c_void {
    ptr::null_mut()
}

/// Set the scene on a NeomacsWidget (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_widget_set_scene(
    _handle: *mut NeomacsDisplay,
    _widget: *mut c_void,
) -> c_int {
    -1
}

/// Initialize the GSK renderer's Pango context from a NeomacsWidget (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_widget_init_pango(
    _handle: *mut NeomacsDisplay,
    _widget: *mut c_void,
) {
    // Not implemented without GTK4
}

/// Render scene to a NeomacsWidget (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_render_to_widget(
    _handle: *mut NeomacsDisplay,
    _widget: *mut c_void,
) -> c_int {
    -1
}

/// Type for the resize callback function pointer from C
pub type ResizeCallbackFn = extern "C" fn(user_data: *mut c_void, width: c_int, height: c_int);

/// Set the resize callback (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_resize_callback(
    _callback: ResizeCallbackFn,
    _user_data: *mut c_void,
) {
    // Not implemented without GTK4
}

// ============================================================================
// Mouse Event Callbacks (stubs - no GTK4 backend)
// ============================================================================

/// Type for mouse button callback
pub type MouseButtonCallbackFn = extern "C" fn(
    user_data: *mut c_void,
    x: c_double,
    y: c_double,
    button: c_uint,
    pressed: c_int,
    modifiers: c_uint,
    time: c_uint,
);

/// Type for mouse motion callback
pub type MouseMotionCallbackFn = extern "C" fn(
    user_data: *mut c_void,
    x: c_double,
    y: c_double,
    modifiers: c_uint,
    time: c_uint,
);

/// Type for mouse scroll callback
pub type MouseScrollCallbackFn = extern "C" fn(
    user_data: *mut c_void,
    x: c_double,
    y: c_double,
    delta_x: c_double,
    delta_y: c_double,
    modifiers: c_uint,
    time: c_uint,
);

/// Set the mouse button callback (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_mouse_button_callback(
    _callback: MouseButtonCallbackFn,
    _user_data: *mut c_void,
) {
    // Not implemented without GTK4
}

/// Set the mouse motion callback (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_mouse_motion_callback(
    _callback: MouseMotionCallbackFn,
    _user_data: *mut c_void,
) {
    // Not implemented without GTK4
}

/// Set the mouse scroll callback (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_mouse_scroll_callback(
    _callback: MouseScrollCallbackFn,
    _user_data: *mut c_void,
) {
    // Not implemented without GTK4
}

// ============================================================================
// WebKit Integration
// ============================================================================

#[cfg(feature = "wpe-webkit")]
use std::cell::RefCell;
#[cfg(feature = "wpe-webkit")]
use crate::backend::wpe::{WpeBackend, WebKitViewCache};

#[cfg(feature = "wpe-webkit")]
thread_local! {
    static WEBKIT_CACHE: RefCell<Option<WebKitViewCache>> = const { RefCell::new(None) };
    static WPE_BACKEND: RefCell<Option<WpeBackend>> = const { RefCell::new(None) };
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
    _handle: *mut NeomacsDisplay,
    egl_display: *mut libc::c_void,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        eprintln!("neomacs_display_webkit_init: egl_display={:?}", egl_display);

        // If no EGL display provided, try to get one from the current context
        let egl_display = if egl_display.is_null() {
            eprintln!("neomacs_display_webkit_init: egl_display is NULL, trying eglGetCurrentDisplay");
            let current = egl_get_current_display();
            eprintln!("neomacs_display_webkit_init: eglGetCurrentDisplay returned {:?}", current);
            current
        } else {
            egl_display
        };

        // Initialize WPE backend
        match WpeBackend::new(egl_display) {
            Ok(backend) => {
                WPE_BACKEND.with(|wpe| {
                    *wpe.borrow_mut() = Some(backend);
                });

                // Initialize cache
                WEBKIT_CACHE.with(|cache| {
                    *cache.borrow_mut() = Some(WebKitViewCache::new());
                });

                eprintln!("neomacs_display_webkit_init: WebKit subsystem initialized successfully");
                return 0;
            }
            Err(e) => {
                eprintln!("neomacs_display_webkit_init: Failed to initialize WPE backend: {}", e);
                return -1;
            }
        }
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = egl_display;
        eprintln!("WebKit support not compiled");
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

/// Create a new WebKit view
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_create(
    _handle: *mut NeomacsDisplay,
    width: c_int,
    height: c_int,
) -> u32 {
    #[cfg(feature = "wpe-webkit")]
    {
        return WPE_BACKEND.with(|wpe_cell| {
            let wpe_borrow = wpe_cell.borrow();
            let backend = match wpe_borrow.as_ref() {
                Some(b) => b,
                None => {
                    eprintln!("WebKit not initialized - call neomacs_display_webkit_init first");
                    return 0;
                }
            };

            WEBKIT_CACHE.with(|cache_cell| {
                let mut cache_borrow = cache_cell.borrow_mut();
                if let Some(cache) = cache_borrow.as_mut() {
                    match cache.create_with_backend(backend, width, height) {
                        Ok(id) => id,
                        Err(e) => {
                            eprintln!("Failed to create WebKit view: {}", e);
                            0
                        }
                    }
                } else {
                    0
                }
            })
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (width, height);
        eprintln!("WebKit support not compiled");
        0
    }
}

/// Destroy a WebKit view
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_destroy(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        return WEBKIT_CACHE.with(|cache_cell| {
            let mut cache_borrow = cache_cell.borrow_mut();
            if let Some(cache) = cache_borrow.as_mut() {
                if cache.remove(view_id) {
                    return 0;
                }
            }
            -1
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Load a URI in a WebKit view
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
        return WEBKIT_CACHE.with(|cache_cell| {
            let mut cache_borrow = cache_cell.borrow_mut();
            if let Some(cache) = cache_borrow.as_mut() {
                let uri_str = match CStr::from_ptr(uri).to_str() {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                if let Err(e) = cache.load_uri(view_id, uri_str) {
                    eprintln!("Failed to load URI: {}", e);
                    return -1;
                }
                return 0;
            }
            -1
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Go back in a WebKit view
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_go_back(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        return WEBKIT_CACHE.with(|cache_cell| {
            let mut cache_borrow = cache_cell.borrow_mut();
            if let Some(cache) = cache_borrow.as_mut() {
                if let Some(view) = cache.get_mut(view_id) {
                    let _ = view.go_back();
                    return 0;
                }
            }
            -1
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Go forward in a WebKit view
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_go_forward(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        return WEBKIT_CACHE.with(|cache_cell| {
            let mut cache_borrow = cache_cell.borrow_mut();
            if let Some(cache) = cache_borrow.as_mut() {
                if let Some(view) = cache.get_mut(view_id) {
                    let _ = view.go_forward();
                    return 0;
                }
            }
            -1
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Reload a WebKit view
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_reload(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        return WEBKIT_CACHE.with(|cache_cell| {
            let mut cache_borrow = cache_cell.borrow_mut();
            if let Some(cache) = cache_borrow.as_mut() {
                if let Some(view) = cache.get_mut(view_id) {
                    let _ = view.reload();
                    return 0;
                }
            }
            -1
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = view_id;
        -1
    }
}

/// Resize a WebKit view
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_resize(
    _handle: *mut NeomacsDisplay,
    view_id: u32,
    width: c_int,
    height: c_int,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        return WEBKIT_CACHE.with(|cache_cell| {
            let mut cache_borrow = cache_cell.borrow_mut();
            if let Some(cache) = cache_borrow.as_mut() {
                if let Some(view) = cache.get_mut(view_id) {
                    view.resize(width as u32, height as u32);
                    return 0;
                }
            }
            -1
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (view_id, width, height);
        -1
    }
}

/// Execute JavaScript in a WebKit view
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
        return WEBKIT_CACHE.with(|cache_cell| {
            let mut cache_borrow = cache_cell.borrow_mut();
            if let Some(cache) = cache_borrow.as_mut() {
                let script_str = match CStr::from_ptr(script).to_str() {
                    Ok(s) => s,
                    Err(_) => return -1,
                };

                if let Err(e) = cache.execute_javascript(view_id, script_str) {
                    eprintln!("Failed to execute JavaScript: {}", e);
                    return -1;
                }
                return 0;
            }
            -1
        });
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

    // Remove existing webkit with same ID
    let target_scene = display.get_target_scene();
    target_scene.floating_webkits.retain(|w| w.webkit_id != webkit_id);

    // Add webkit at position
    target_scene.add_floating_webkit(
        webkit_id,
        x as f32,
        y as f32,
        width as f32,
        height as f32,
    );
    info!("neomacs_display_set_floating_webkit: now have {} floating webkits", target_scene.floating_webkits.len());
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
}

/// Find which floating webkit view is at the given coordinates
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

    0 // No webkit at position
}

/// Send keyboard event to WebKit view
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
        WEBKIT_CACHE.with(|cache| {
            if let Some(ref c) = *cache.borrow() {
                if let Err(e) = c.send_keyboard_event(
                    webkit_id,
                    key_code,
                    hardware_key_code,
                    pressed != 0,
                    modifiers,
                ) {
                    eprintln!("WebKit key event error: {}", e);
                }
            }
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (webkit_id, key_code, hardware_key_code, pressed, modifiers);
    }
}

/// Send pointer/mouse event to WebKit view
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
        WEBKIT_CACHE.with(|cache| {
            if let Some(ref c) = *cache.borrow() {
                if let Err(e) = c.send_pointer_event(
                    webkit_id,
                    event_type,
                    x,
                    y,
                    button,
                    state,
                    modifiers,
                ) {
                    eprintln!("WebKit pointer event error: {}", e);
                }
            }
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (webkit_id, event_type, x, y, button, state, modifiers);
    }
}

/// Send scroll event to WebKit view
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
        WEBKIT_CACHE.with(|cache| {
            if let Some(ref c) = *cache.borrow() {
                if let Err(e) = c.send_scroll_event(
                    webkit_id,
                    x,
                    y,
                    delta_x,
                    delta_y,
                ) {
                    eprintln!("WebKit scroll event error: {}", e);
                }
            }
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (webkit_id, x, y, delta_x, delta_y);
    }
}

/// Click in WebKit view (convenience function)
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
        WEBKIT_CACHE.with(|cache| {
            if let Some(ref c) = *cache.borrow() {
                if let Err(e) = c.click(webkit_id, x, y, button) {
                    eprintln!("WebKit click error: {}", e);
                }
            }
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = (webkit_id, x, y, button);
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
        let result = WEBKIT_CACHE.with(|cache| {
            if let Some(ref c) = *cache.borrow() {
                c.get_title(webkit_id)
            } else {
                None
            }
        });

        match result {
            Some(title) => {
                match CString::new(title) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => std::ptr::null_mut(),
                }
            }
            None => std::ptr::null_mut(),
        }
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
        let result = WEBKIT_CACHE.with(|cache| {
            if let Some(ref c) = *cache.borrow() {
                c.get_url(webkit_id)
            } else {
                None
            }
        });

        match result {
            Some(url) => {
                match CString::new(url) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => std::ptr::null_mut(),
                }
            }
            None => std::ptr::null_mut(),
        }
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
        WEBKIT_CACHE.with(|cache| {
            if let Some(ref c) = *cache.borrow() {
                c.get_progress(webkit_id).unwrap_or(-1.0)
            } else {
                -1.0
            }
        })
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
        WEBKIT_CACHE.with(|cache| {
            if let Some(ref c) = *cache.borrow() {
                match c.is_loading(webkit_id) {
                    Some(true) => 1,
                    Some(false) => 0,
                    None => -1,
                }
            } else {
                -1
            }
        })
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

    log::debug!("add_wpe_glyph: at ({}, {}), use_hybrid={}", current_x, current_y, display.use_hybrid);

    // Hybrid path: add to frame glyph buffer
    if display.use_hybrid {
        display.frame_glyphs.add_webkit(
            view_id,
            current_x as f32,
            current_y as f32,
            pixel_width as f32,
            pixel_height as f32,
        );
        display.current_row_x += pixel_width;
        return;
    }

    // Legacy scene graph path
    if let Some(window) = display.get_target_scene().windows.first_mut() {
        if let Some(row) = window.rows.iter_mut().find(|r| r.y == current_y) {
            // Remove overlapping glyphs
            let x_start = current_x;
            let x_end = current_x + pixel_width;
            row.glyphs.retain(|g| {
                let g_end = g.x + g.pixel_width;
                g_end <= x_start || g.x >= x_end
            });

            let glyph = Glyph {
                glyph_type: GlyphType::Wpe,
                charcode: 0,
                face_id: 0,
                x: current_x,
                pixel_width,
                ascent: pixel_height,
                descent: 0,
                charpos: 0,
                left_box_line: false,
                right_box_line: false,
                padding: false,
                data: GlyphData::Wpe { view_id },
            };
            row.glyphs.push(glyph);

            // Advance X position
            display.current_row_x += pixel_width;
        }
    }
}

// ============================================================================
// Window Management FFI Functions
// ============================================================================

/// Create a new window with the specified dimensions and title.
///
/// Returns the window ID. The window will be created during the next poll_events call.
/// Returns 0 if the backend is not available.
#[no_mangle]
pub extern "C" fn neomacs_display_create_window(
    handle: *mut NeomacsDisplay,
    width: i32,
    height: i32,
    title: *const c_char,
) -> u32 {
    let display = unsafe { &mut *handle };

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        let title_str = if title.is_null() {
            "Emacs"
        } else {
            unsafe { std::ffi::CStr::from_ptr(title).to_str().unwrap_or("Emacs") }
        };
        // Queue the window creation request - it will be processed during poll_events
        return backend.queue_window_request(width as u32, height as u32, title_str);
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
pub extern "C" fn neomacs_display_begin_frame_window(
    handle: *mut NeomacsDisplay,
    window_id: u32,
) {
    let display = unsafe { &mut *handle };

    // Track which window we're currently rendering to
    display.current_render_window_id = window_id;

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        // This clears the window's scene to prepare for new content
        backend.begin_frame_for_window(window_id);
    }
}

/// End a frame for a specific window and present it.
///
/// Renders the window's scene to its surface and presents it.
#[no_mangle]
pub extern "C" fn neomacs_display_end_frame_window(
    handle: *mut NeomacsDisplay,
    window_id: u32,
) {
    let display = unsafe { &mut *handle };

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        // Render and present the window's scene
        backend.end_frame_for_window(window_id);
    }

    // Reset current window tracking after rendering is complete
    display.current_render_window_id = 0;
}

// ============================================================================
// Event Polling FFI Functions
// ============================================================================

/// Set the event callback function.
///
/// The callback will be invoked for each input event when polling.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub extern "C" fn neomacs_display_set_event_callback(callback: EventCallback) {
    unsafe {
        EVENT_CALLBACK = Some(callback);
    }
}

/// Poll for input events and invoke the callback for each event.
///
/// This uses winit's pump_events to process the event loop non-blocking,
/// creates any pending windows, and delivers input events via callback.
///
/// Returns the number of events processed.
#[no_mangle]
pub extern "C" fn neomacs_display_poll_events(handle: *mut NeomacsDisplay) -> i32 {
    if handle.is_null() {
        return 0;
    }

    let display = unsafe { &mut *handle };

    #[cfg(feature = "winit-backend")]
    {
        use std::time::Duration;
        use winit::event::{Event, WindowEvent};
        use winit::event_loop::ControlFlow;
        use winit::platform::pump_events::EventLoopExtPumpEvents;

        // Take the event loop temporarily
        let event_loop = match display.event_loop.take() {
            Some(el) => el,
            None => return 0,
        };

        // We need to collect events and process them after pump_events returns
        // because we can't borrow display.winit_backend while event_loop is borrowed
        let mut collected_events = Vec::new();

        // Create a raw pointer to the backend for use in the closure
        let backend_ptr = display.winit_backend.as_mut()
            .map(|b| b as *mut WinitBackend);

        // Pump events with zero timeout (non-blocking)
        let mut event_loop = event_loop;
        let _ = event_loop.pump_events(Some(Duration::ZERO), |event, elwt| {
            elwt.set_control_flow(ControlFlow::Poll);

            match &event {
                Event::Resumed | Event::AboutToWait => {
                    // Process pending windows whenever we have ActiveEventLoop
                    // AboutToWait fires after each batch of events
                    if let Some(ptr) = backend_ptr {
                        let backend = unsafe { &mut *ptr };
                        backend.process_pending_windows(elwt);
                    }
                }
                Event::WindowEvent { window_id: _, event: window_event } => {
                    if let Some(ptr) = backend_ptr {
                        let backend = unsafe { &mut *ptr };

                        // Translate window events to NeomacsInputEvent
                        match window_event {
                            WindowEvent::KeyboardInput { event: key_event, .. } => {
                                use winit::event::ElementState;
                                use winit::platform::scancode::PhysicalKeyExtScancode;

                                let kind = match key_event.state {
                                    ElementState::Pressed => NEOMACS_EVENT_KEY_PRESS,
                                    ElementState::Released => NEOMACS_EVENT_KEY_RELEASE,
                                };

                                let keysym = backend.translate_key_public(&key_event.logical_key);

                                let ev = NeomacsInputEvent {
                                    kind,
                                    window_id: 1, // TODO: map winit window_id
                                    timestamp: 0,
                                    x: 0,
                                    y: 0,
                                    keycode: key_event.physical_key.to_scancode().unwrap_or(0),
                                    keysym,
                                    modifiers: backend.get_current_modifiers(),
                                    button: 0,
                                    scroll_delta_x: 0.0,
                                    scroll_delta_y: 0.0,
                                    width: 0,
                                    height: 0,
                                };
                                collected_events.push(ev);
                            }
                            WindowEvent::ModifiersChanged(modifiers) => {
                                backend.update_modifiers(modifiers);
                            }
                            WindowEvent::CursorMoved { position, .. } => {
                                backend.update_mouse_position(position.x as i32, position.y as i32);
                            }
                            WindowEvent::MouseInput { state, button, .. } => {
                                use winit::event::{ElementState, MouseButton};

                                let kind = match state {
                                    ElementState::Pressed => NEOMACS_EVENT_BUTTON_PRESS,
                                    ElementState::Released => NEOMACS_EVENT_BUTTON_RELEASE,
                                };

                                let btn = match button {
                                    MouseButton::Left => 1,
                                    MouseButton::Middle => 2,
                                    MouseButton::Right => 3,
                                    _ => 0,
                                };

                                let (mx, my) = backend.get_mouse_position();
                                let ev = NeomacsInputEvent {
                                    kind,
                                    window_id: 1,
                                    timestamp: 0,
                                    x: mx,
                                    y: my,
                                    keycode: 0,
                                    keysym: 0,
                                    modifiers: backend.get_current_modifiers(),
                                    button: btn,
                                    scroll_delta_x: 0.0,
                                    scroll_delta_y: 0.0,
                                    width: 0,
                                    height: 0,
                                };
                                collected_events.push(ev);
                            }
                            WindowEvent::MouseWheel { delta, .. } => {
                                use winit::event::MouseScrollDelta;

                                let (dx, dy) = match delta {
                                    MouseScrollDelta::LineDelta(x, y) => (*x, *y),
                                    MouseScrollDelta::PixelDelta(pos) => (pos.x as f32, pos.y as f32),
                                };

                                let (mx, my) = backend.get_mouse_position();
                                let ev = NeomacsInputEvent {
                                    kind: NEOMACS_EVENT_SCROLL,
                                    window_id: 1,
                                    timestamp: 0,
                                    x: mx,
                                    y: my,
                                    keycode: 0,
                                    keysym: 0,
                                    modifiers: backend.get_current_modifiers(),
                                    button: 0,
                                    scroll_delta_x: dx,
                                    scroll_delta_y: dy,
                                    width: 0,
                                    height: 0,
                                };
                                collected_events.push(ev);
                            }
                            WindowEvent::Resized(size) => {
                                let ev = NeomacsInputEvent {
                                    kind: NEOMACS_EVENT_RESIZE,
                                    window_id: 1,
                                    timestamp: 0,
                                    x: 0,
                                    y: 0,
                                    keycode: 0,
                                    keysym: 0,
                                    modifiers: 0,
                                    button: 0,
                                    scroll_delta_x: 0.0,
                                    scroll_delta_y: 0.0,
                                    width: size.width,
                                    height: size.height,
                                };
                                collected_events.push(ev);
                            }
                            WindowEvent::CloseRequested => {
                                let ev = NeomacsInputEvent {
                                    kind: NEOMACS_EVENT_CLOSE,
                                    window_id: 1,
                                    timestamp: 0,
                                    x: 0,
                                    y: 0,
                                    keycode: 0,
                                    keysym: 0,
                                    modifiers: 0,
                                    button: 0,
                                    scroll_delta_x: 0.0,
                                    scroll_delta_y: 0.0,
                                    width: 0,
                                    height: 0,
                                };
                                collected_events.push(ev);
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        });

        // Put the event loop back
        display.event_loop = Some(event_loop);

        // Deliver events via callback
        let count = collected_events.len() as i32;
        unsafe {
            if let Some(callback) = EVENT_CALLBACK {
                for event in &collected_events {
                    callback(event as *const _);
                }
            }
        }

        return count;
    }

    #[cfg(not(feature = "winit-backend"))]
    0
}

// ============================================================================
// Animation FFI functions (stubs - no GTK4 backend)
// ============================================================================

/// Set an animation configuration option (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_animation_option(
    _handle: *mut NeomacsDisplay,
    _key: *const c_char,
    _value: *const c_char,
) -> c_int {
    0
}

/// Get an animation configuration option (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_animation_option(
    _handle: *mut NeomacsDisplay,
    _key: *const c_char,
) -> *mut c_char {
    ptr::null_mut()
}

/// Free a string returned by neomacs_display_get_animation_option
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_free_string(s: *mut c_char) {
    if !s.is_null() {
        let _ = CString::from_raw(s);
    }
}

/// Update cursor animation state (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_update_animation(
    _handle: *mut NeomacsDisplay,
    _dt: c_double,
) -> c_int {
    0
}

/// Check if animation needs continuous redraw (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_animation_active(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    0
}

/// Trigger a buffer transition animation (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_start_buffer_transition(
    _handle: *mut NeomacsDisplay,
    _effect: *const c_char,
    _duration_ms: c_int,
) -> c_int {
    0
}

/// Prepare for buffer transition (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_prepare_buffer_transition(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    0
}

/// Trigger buffer transition animation (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_trigger_buffer_transition(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    0
}

/// Check if buffer transition is ready (stub)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_has_transition_snapshot(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    0
}

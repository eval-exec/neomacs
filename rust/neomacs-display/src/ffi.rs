//! C FFI layer for integration with Emacs.
//!
//! Enable logging with: RUST_LOG=neomacs_display=debug

use std::collections::HashMap;
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
    NEOMACS_EVENT_MOUSE_MOVE, NEOMACS_EVENT_SCROLL,
    NEOMACS_EVENT_RESIZE, NEOMACS_EVENT_CLOSE,
    NEOMACS_EVENT_FOCUS_IN, NEOMACS_EVENT_FOCUS_OUT,
};

/// Resize callback function type for C FFI
#[cfg(feature = "winit-backend")]
type ResizeCallback = extern "C" fn(user_data: *mut std::ffi::c_void, width: std::ffi::c_int, height: std::ffi::c_int);

/// Global resize callback - set by C code to receive resize events
#[cfg(feature = "winit-backend")]
static mut RESIZE_CALLBACK: Option<ResizeCallback> = None;

/// User data pointer for resize callback
#[cfg(feature = "winit-backend")]
static mut RESIZE_CALLBACK_USER_DATA: *mut std::ffi::c_void = std::ptr::null_mut();
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
    faces: HashMap<u32, Face>,
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

// Note: neomacs_display_init() has been removed - use neomacs_display_init_threaded() instead

/// Shutdown the display engine
///
/// # Safety
/// The handle must have been returned by neomacs_display_init_threaded.
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
    let new_width = width as f32;
    let new_height = height as f32;

    // Only clear glyphs if size actually changed
    let size_changed = (display.frame_glyphs.width - new_width).abs() > 1.0
                    || (display.frame_glyphs.height - new_height).abs() > 1.0;

    if size_changed {
        log::info!("neomacs_display_resize: {}x{} -> {}x{}, clearing {} glyphs",
            display.frame_glyphs.width, display.frame_glyphs.height,
            width, height, display.frame_glyphs.glyphs.len());
        display.scene = Scene::new(new_width, new_height);
        display.frame_glyphs.width = new_width;
        display.frame_glyphs.height = new_height;
        display.frame_glyphs.glyphs.clear();  // Clear all glyphs - Emacs will resend
        display.frame_glyphs.window_regions.clear();
        display.frame_glyphs.prev_window_regions.clear();
    } else {
        log::debug!("neomacs_display_resize: size unchanged ({}x{}), keeping {} glyphs",
            width, height, display.frame_glyphs.glyphs.len());
    }

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

    debug!("begin_frame: frame={}, hybrid={}, glyphs={} chars={}",
           display.frame_counter, display.use_hybrid, display.frame_glyphs.len(),
           display.frame_glyphs.glyphs.iter().filter(|g| matches!(g, FrameGlyph::Char { .. })).count());

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
    // Skip hybrid path if rendering to a winit window (current_render_window_id > 0)
    if display.use_hybrid {
        let color = Color::from_pixel(bg_color);
        debug!("neomacs_display_add_window: id={} at ({},{}) size {}x{} bg=0x{:06x}->({:.3},{:.3},{:.3})",
               window_id, x, y, width, height, bg_color, color.r, color.g, color.b);
        display.frame_glyphs.add_background(
            x, y, width, height,
            color,
        );
        return;
    }

    // Scene graph path (used for winit windows and legacy rendering)...
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
    font_size: c_int, // Font size in pixels (from face->font->pixel_size)
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

    let new_font_size = if font_size > 0 { font_size as f32 } else { 14.0 };

    // Detect text-scale changes: When text-scale-increase is used, Emacs creates NEW faces
    // with larger font_size values (typically IDs > 22). We need to clear glyphs when this
    // happens because the text layout will be different.
    //
    // Get baseline font size from face 0 (the default face). If face 0 doesn't exist yet,
    // we're in initial setup and shouldn't clear anything.
    let baseline_font_size = display.faces.get(&0).map(|f| f.font_size);

    let should_clear = if let Some(baseline) = baseline_font_size {
        // We have a baseline - check for font size changes
        if let Some(existing_face) = display.faces.get(&face_id) {
            // Existing face - check if size changed
            (existing_face.font_size - new_font_size).abs() > 0.1
        } else {
            // New face - clear if this is a scaled face (font_size differs from baseline)
            // This detects text-scale-increase creating new scaled faces
            (new_font_size - baseline).abs() > 0.1
        }
    } else {
        // No baseline yet (face 0 not created) - don't clear during initial setup
        false
    };

    if should_clear {
        log::debug!("Text scale change detected: face {}: size={}, clearing non-overlay glyphs",
                   face_id, new_font_size);
        // Only clear non-overlay glyphs (buffer content).
        // Preserve overlay glyphs (modeline, echo area) since they don't change with text-scale.
        display.frame_glyphs.glyphs.retain(|g| {
            match g {
                crate::core::frame_glyphs::FrameGlyph::Char { is_overlay, .. } => *is_overlay,
                crate::core::frame_glyphs::FrameGlyph::Stretch { is_overlay, .. } => *is_overlay,
                // Keep cursors, borders, backgrounds, images, etc.
                crate::core::frame_glyphs::FrameGlyph::Cursor { .. } => true,
                crate::core::frame_glyphs::FrameGlyph::Border { .. } => true,
                crate::core::frame_glyphs::FrameGlyph::Background { .. } => true,
                _ => false, // Clear images, videos, webkit (unlikely to be affected but safer)
            }
        });
        display.frame_glyphs.window_regions.clear();
    }

    let face = Face {
        id: face_id,
        foreground: fg,
        background: bg,
        underline_color: ul_color,
        overline_color: None,
        strike_through_color: None,
        box_color: bx_color,
        font_family: font_family_str.clone(),
        font_size: new_font_size,
        font_weight,
        attributes: attrs,
        underline_style: ul_style,
        box_type: bx_type,
        box_line_width,
    };

    // Store face for later lookup during rendering
    display.faces.insert(face_id, face.clone());

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
            if font_size > 0 { font_size as f32 } else { 14.0 },
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

    debug!("neomacs_display_set_background: color=0x{:06x} -> ({:.3},{:.3},{:.3})",
           color, bg.r, bg.g, bg.b);

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

/// Load a video from file path (async - uses GStreamer)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_video(
    handle: *mut NeomacsDisplay,
    path: *const c_char,
) -> u32 {
    let display = match handle.as_mut() {
        Some(d) => d,
        None => return 0,
    };

    let path_str = match std::ffi::CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    log::info!("load_video: path={}", path_str);

    // Threaded path: send command to render thread
    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref state) = THREADED_STATE {
        let id = VIDEO_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let cmd = RenderCommand::VideoCreate {
            id,
            path: path_str.to_string(),
        };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        log::info!("load_video: threaded path, id={}", id);
        return id;
    }

    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            let id = renderer.load_video_file(path_str);
            log::info!("load_video: returned id={}", id);
            return id;
        }
    }

    0
}

/// Play a loaded video
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_play(
    handle: *mut NeomacsDisplay,
    video_id: u32,
) -> c_int {
    // Threaded path
    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref state) = THREADED_STATE {
        let cmd = RenderCommand::VideoPlay { id: video_id };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        return 0;
    }

    let display = match handle.as_mut() {
        Some(d) => d,
        None => return -1,
    };

    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            renderer.video_play(video_id);
            return 0;
        }
    }

    -1
}

/// Pause a video
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_pause(
    handle: *mut NeomacsDisplay,
    video_id: u32,
) -> c_int {
    // Threaded path
    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref state) = THREADED_STATE {
        let cmd = RenderCommand::VideoPause { id: video_id };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        return 0;
    }

    let display = match handle.as_mut() {
        Some(d) => d,
        None => return -1,
    };

    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            renderer.video_pause(video_id);
            return 0;
        }
    }

    -1
}

/// Stop a video
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_stop(
    handle: *mut NeomacsDisplay,
    video_id: u32,
) -> c_int {
    // Threaded path: stop maps to destroy
    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref state) = THREADED_STATE {
        let cmd = RenderCommand::VideoDestroy { id: video_id };
        let _ = state.emacs_comms.cmd_tx.try_send(cmd);
        return 0;
    }

    let display = match handle.as_mut() {
        Some(d) => d,
        None => return -1,
    };

    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            renderer.video_stop(video_id);
            return 0;
        }
    }

    -1
}

/// Set video loop mode (-1 for infinite)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_set_loop(
    handle: *mut NeomacsDisplay,
    video_id: u32,
    loop_count: c_int,
) -> c_int {
    let display = match handle.as_mut() {
        Some(d) => d,
        None => return -1,
    };

    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            renderer.video_set_loop(video_id, loop_count);
            return 0;
        }
    }

    -1
}

/// Process pending video frames (call each frame)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_update(
    handle: *mut NeomacsDisplay,
    _video_id: u32,
) -> c_int {
    let display = match handle.as_mut() {
        Some(d) => d,
        None => return -1,
    };

    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            renderer.process_pending_videos();
            return 0;
        }
    }

    -1
}

/// Get video dimensions (works for pending and loaded videos)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_video_size(
    handle: *mut NeomacsDisplay,
    video_id: u32,
    width: *mut c_int,
    height: *mut c_int,
) -> c_int {
    if handle.is_null() || width.is_null() || height.is_null() {
        return -1;
    }
    let display = &mut *handle;

    #[cfg(all(feature = "winit-backend", feature = "video"))]
    if let Some(ref backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer() {
            if let Some((w, h)) = renderer.get_video_size(video_id) {
                *width = w as c_int;
                *height = h as c_int;
                return 0;
            }
        }
    }

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

/// Load an image from a file path (async - returns ID immediately)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_file(
    handle: *mut NeomacsDisplay,
    path: *const c_char,
) -> u32 {
    if handle.is_null() || path.is_null() {
        return 0;
    }
    let display = &mut *handle;
    let path_str = match std::ffi::CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            return renderer.load_image_file(path_str, 0, 0);
        }
    }
    0
}

/// Load an image from a file path with scaling (async)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_file_scaled(
    handle: *mut NeomacsDisplay,
    path: *const c_char,
    max_width: c_int,
    max_height: c_int,
) -> u32 {
    if handle.is_null() || path.is_null() {
        return 0;
    }
    let display = &mut *handle;
    let path_str = match std::ffi::CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    log::info!("load_image_file_scaled: path={}, max={}x{}", path_str, max_width, max_height);

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            let id = renderer.load_image_file(
                path_str,
                max_width.max(0) as u32,
                max_height.max(0) as u32,
            );
            log::info!("load_image_file_scaled: returned id={}", id);
            return id;
        }
    }
    0
}

/// Load an image directly as texture (same as load_image_file)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_file_direct(
    handle: *mut NeomacsDisplay,
    path: *const c_char,
) -> u32 {
    neomacs_display_load_image_file(handle, path)
}

/// Load an image directly as texture with scaling
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_file_direct_scaled(
    handle: *mut NeomacsDisplay,
    path: *const c_char,
    max_width: c_int,
    max_height: c_int,
) -> u32 {
    neomacs_display_load_image_file_scaled(handle, path, max_width, max_height)
}

/// Get image dimensions (works for pending and loaded images)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_get_image_size(
    handle: *mut NeomacsDisplay,
    image_id: u32,
    width: *mut c_int,
    height: *mut c_int,
) -> c_int {
    if handle.is_null() || width.is_null() || height.is_null() {
        return -1;
    }
    let display = &mut *handle;

    #[cfg(feature = "winit-backend")]
    if let Some(ref backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer() {
            if let Some((w, h)) = renderer.get_image_size(image_id) {
                *width = w as c_int;
                *height = h as c_int;
                return 0;
            }
        }
    }
    -1
}

/// Query image file dimensions without loading (fast - reads header only)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_query_image_file_size(
    _handle: *mut NeomacsDisplay,
    path: *const c_char,
    width: *mut c_int,
    height: *mut c_int,
) -> c_int {
    if path.is_null() || width.is_null() || height.is_null() {
        return -1;
    }
    let path_str = match std::ffi::CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return -1,
    };

    #[cfg(feature = "winit-backend")]
    {
        use crate::backend::wgpu::WgpuRenderer;
        if let Some((w, h)) = WgpuRenderer::query_image_file_size(path_str) {
            *width = w as c_int;
            *height = h as c_int;
            return 0;
        }
    }
    -1
}

/// Free an image from cache
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_free_image(
    handle: *mut NeomacsDisplay,
    image_id: u32,
) -> c_int {
    if handle.is_null() {
        return -1;
    }
    let display = &mut *handle;

    #[cfg(feature = "winit-backend")]
    if let Some(ref mut backend) = display.winit_backend {
        if let Some(renderer) = backend.renderer_mut() {
            renderer.free_image(image_id);
            return 0;
        }
    }
    -1
}

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
        let before = display.frame_glyphs.glyphs.len();
        display.frame_glyphs.clear_area(
            x as f32,
            y as f32,
            width as f32,
            height as f32,
        );
        let after = display.frame_glyphs.glyphs.len();
        // Log ALL clear_area calls to trace what's happening
        log::debug!("neomacs_display_clear_area: ({},{}) {}x{} glyphs: {} -> {} (removed {})",
            x, y, width, height, before, after, before.saturating_sub(after));
    }
}

/// Clear all glyphs - used when frame layout changes
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_all_glyphs(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    log::info!("neomacs_display_clear_all_glyphs: clearing {} glyphs", display.frame_glyphs.glyphs.len());
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

/// Set the resize callback for winit windows.
///
/// The callback will be invoked when the window is resized.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_resize_callback(
    callback: ResizeCallbackFn,
    user_data: *mut c_void,
) {
    #[cfg(feature = "winit-backend")]
    {
        RESIZE_CALLBACK = Some(callback);
        RESIZE_CALLBACK_USER_DATA = user_data;
        log::debug!("Resize callback set");
    }
    #[cfg(not(feature = "winit-backend"))]
    {
        let _ = callback;
        let _ = user_data;
    }
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
    /// WebKit view cache for WPE views (public for renderer access).
    pub static WEBKIT_CACHE: RefCell<Option<WebKitViewCache>> = const { RefCell::new(None) };
    static WPE_BACKEND: RefCell<Option<WpeBackend>> = const { RefCell::new(None) };
}

/// Atomic counter for generating WebKit view IDs in threaded mode
#[cfg(feature = "wpe-webkit")]
static WEBKIT_VIEW_ID_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// Atomic counter for generating video IDs in threaded mode
#[cfg(feature = "video")]
static VIDEO_ID_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

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

        // If no EGL display provided, try to get one from the current context
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

                // Initialize cache
                WEBKIT_CACHE.with(|cache| {
                    *cache.borrow_mut() = Some(WebKitViewCache::new());
                });

                log::info!("neomacs_display_webkit_init: WebKit subsystem initialized successfully");
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

/// Create a new WebKit view
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_create(
    _handle: *mut NeomacsDisplay,
    width: c_int,
    height: c_int,
) -> u32 {
    #[cfg(feature = "wpe-webkit")]
    {
        // Use threaded path if available
        #[cfg(feature = "winit-backend")]
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

        // Fallback to old path (non-threaded)
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
        // Use threaded path if available
        #[cfg(feature = "winit-backend")]
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitDestroy { id: view_id };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }

        // Fallback to old path (non-threaded)
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
        // Use threaded path if available
        #[cfg(feature = "winit-backend")]
        if let Some(ref state) = THREADED_STATE {
            let url = CStr::from_ptr(uri).to_string_lossy().into_owned();
            let cmd = RenderCommand::WebKitLoadUri { id: view_id, url };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }

        // Fallback to old path (non-threaded)
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
        // Use threaded path if available
        #[cfg(feature = "winit-backend")]
        if let Some(ref state) = THREADED_STATE {
            let cmd = RenderCommand::WebKitResize {
                id: view_id,
                width: width as u32,
                height: height as u32,
            };
            let _ = state.emacs_comms.cmd_tx.try_send(cmd);
            return 0;
        }

        // Fallback to old path (non-threaded)
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

/// Update WebKit view - pumps GLib main context to process events
/// This MUST be called regularly (e.g., every frame or via timer) for WebKit to render
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_update(
    _handle: *mut NeomacsDisplay,
    webkit_id: u32,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        return WEBKIT_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(ref mut cache) = *cache {
                if let Some(view) = cache.get_mut(webkit_id) {
                    view.update();
                    return 0;
                }
            }
            -1
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        let _ = webkit_id;
        -1
    }
}

/// Update all WebKit views - pumps GLib main context
/// Call this once per frame to process all webkit events
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_webkit_update_all(
    _handle: *mut NeomacsDisplay,
) -> c_int {
    #[cfg(feature = "wpe-webkit")]
    {
        return WEBKIT_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            if let Some(ref mut cache) = *cache {
                cache.update_all();
                return 0;
            }
            -1
        });
    }

    #[cfg(not(feature = "wpe-webkit"))]
    {
        -1
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
    _handle: *mut NeomacsDisplay,
    _width: i32,
    _height: i32,
    _title: *const c_char,
) -> u32 {
    // In threaded mode, the main window is created automatically by the render thread
    // Return window ID 1 (the main window)
    #[cfg(feature = "winit-backend")]
    unsafe {
        if THREADED_STATE.is_some() {
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
pub extern "C" fn neomacs_display_begin_frame_window(
    handle: *mut NeomacsDisplay,
    window_id: u32,
) {
    let display = unsafe { &mut *handle };

    log::debug!("neomacs_display_begin_frame_window: window_id={}, current glyphs={}",
        window_id, display.frame_glyphs.glyphs.len());

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

    // Count char glyphs to detect partial updates (cursor-only frames)
    let char_count = display.frame_glyphs.glyphs.iter()
        .filter(|g| matches!(g, FrameGlyph::Char { .. }))
        .count();

    log::debug!("neomacs_display_end_frame_window: window_id={}, glyphs={}, chars={}, faces={}",
        window_id, display.frame_glyphs.glyphs.len(), char_count, display.faces.len());

    #[cfg(feature = "winit-backend")]
    {
        // In threaded mode, send frame to render thread
        // Safety: THREADED_STATE is only modified during init/shutdown
        if let Some(ref state) = unsafe { THREADED_STATE.as_ref() } {
            // Only send frames that have character content.
            // Skip cursor-only or background-only frames to preserve previous full frame.
            // This handles Emacs's incremental update model where cursor blinks
            // don't resend all text content.
            if char_count > 0 {
                // Clone frame glyphs and send to render thread
                let frame = display.frame_glyphs.clone();
                let _ = state.emacs_comms.frame_tx.try_send(frame);
            } else {
                log::debug!("neomacs_display_end_frame_window: skipping frame with no char glyphs");
            }
        } else if let Some(ref mut backend) = display.winit_backend {
            // Non-threaded mode: render directly
            backend.end_frame_for_window(
                window_id,
                &display.frame_glyphs,
                &display.faces,
            );
        } else {
            log::debug!("neomacs_display_end_frame_window: no winit_backend and not in threaded mode");
        }
    }

    #[cfg(not(feature = "winit-backend"))]
    {
        let _ = window_id;
        log::debug!("neomacs_display_end_frame_window: winit-backend feature not enabled");
    }

    // Reset current window tracking after rendering is complete
    display.current_render_window_id = 0;
}

// Note: Event Polling FFI Functions have been removed
// Events are now delivered via the threaded mode wakeup mechanism
// Use neomacs_display_drain_input() instead

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

// ============================================================================
// Threaded Mode FFI
// ============================================================================

#[cfg(feature = "winit-backend")]
use crate::thread_comm::{EmacsComms, InputEvent, RenderCommand, ThreadComms};
#[cfg(feature = "winit-backend")]
use crate::render_thread::RenderThread;

/// Global state for threaded mode
#[cfg(feature = "winit-backend")]
static mut THREADED_STATE: Option<ThreadedState> = None;

#[cfg(feature = "winit-backend")]
struct ThreadedState {
    emacs_comms: EmacsComms,
    render_thread: Option<RenderThread>,
    display_handle: *mut NeomacsDisplay,
}

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

    // Spawn render thread
    let render_thread = RenderThread::spawn(render_comms, width, height, title);

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
        in_frame: false,
        frame_counter: 0,
        current_render_window_id: 0,
        faces: HashMap::new(),
    });
    let display_ptr = Box::into_raw(display);

    THREADED_STATE = Some(ThreadedState {
        emacs_comms,
        render_thread: Some(render_thread),
        display_handle: display_ptr,
    });

    wakeup_fd
}

/// Drain input events from render thread
///
/// Returns number of events written to buffer.
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_drain_input(
    events: *mut NeomacsInputEvent,
    max_events: c_int,
) -> c_int {
    let state = match THREADED_STATE.as_ref() {
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
                    }
                    InputEvent::MouseMove { x, y, modifiers } => {
                        out.kind = NEOMACS_EVENT_MOUSE_MOVE;
                        out.x = x as i32;
                        out.y = y as i32;
                        out.modifiers = modifiers;
                    }
                    InputEvent::MouseScroll {
                        delta_x,
                        delta_y,
                        x,
                        y,
                        modifiers,
                    } => {
                        out.kind = NEOMACS_EVENT_SCROLL;
                        out.x = x as i32;
                        out.y = y as i32;
                        out.scroll_delta_x = delta_x;
                        out.scroll_delta_y = delta_y;
                        out.modifiers = modifiers;
                    }
                    InputEvent::WindowResize { width, height } => {
                        out.kind = NEOMACS_EVENT_RESIZE;
                        out.width = width;
                        out.height = height;
                    }
                    InputEvent::WindowClose => {
                        out.kind = NEOMACS_EVENT_CLOSE;
                    }
                    InputEvent::WindowFocus { focused } => {
                        out.kind = if focused {
                            NEOMACS_EVENT_FOCUS_IN
                        } else {
                            NEOMACS_EVENT_FOCUS_OUT
                        };
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
                }
                count += 1;
            }
            Err(_) => break,
        }
    }

    count
}

/// Send frame glyphs to render thread
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_send_frame(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &*handle;

    let state = match THREADED_STATE.as_ref() {
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
    let state = match THREADED_STATE.as_ref() {
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

/// Shutdown threaded display
#[cfg(feature = "winit-backend")]
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_shutdown_threaded() {
    if let Some(mut state) = THREADED_STATE.take() {
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
    match THREADED_STATE.as_ref() {
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
    match THREADED_STATE.as_ref() {
        Some(state) => state.display_handle,
        None => std::ptr::null_mut(),
    }
}

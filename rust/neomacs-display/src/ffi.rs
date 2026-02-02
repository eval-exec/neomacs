//! C FFI layer for integration with Emacs.
//!
//! Enable logging with: RUST_LOG=neomacs_display=debug

use std::ffi::{c_char, c_int, c_uint, c_double, c_void, CStr, CString};
use std::panic;
use std::ptr;

use log::{debug, trace, warn, info, error};

use crate::backend::{BackendType, DisplayBackend};
use crate::backend::gtk4::{Gtk4Backend, Gtk4Renderer, GskRenderer, HybridRenderer, VideoCache, ImageCache, set_video_widget};
use crate::backend::tty::TtyBackend;
use crate::core::types::{Color, Rect};
use crate::core::scene::{Scene, WindowScene, CursorState, CursorStyle};
use crate::core::glyph::{Glyph, GlyphRow, GlyphType, GlyphData};
use crate::core::animation::AnimationManager;
use crate::core::frame_glyphs::{FrameGlyphBuffer, FrameGlyph};

/// Opaque handle to the display engine
pub struct NeomacsDisplay {
    backend_type: BackendType,
    gtk4_backend: Option<Gtk4Backend>,
    tty_backend: Option<TtyBackend>,
    scene: Scene,           // The scene for rendering (legacy)
    frame_glyphs: FrameGlyphBuffer,  // Hybrid approach: direct glyph buffer
    use_hybrid: bool,       // Whether to use hybrid rendering (default: true)
    animations: AnimationManager,
    renderer: Gtk4Renderer,  // Cairo renderer for external Cairo context
    gsk_renderer: GskRenderer, // GSK renderer for GPU-accelerated rendering
    hybrid_renderer: HybridRenderer, // Hybrid renderer for direct GSK rendering
    use_gsk: bool,          // Whether to use GSK rendering
    video_cache: VideoCache, // Video player cache
    image_cache: ImageCache, // Image cache
    current_row_y: i32,     // Y position of current row being built
    current_row_x: i32,     // X position for next glyph in current row
    current_row_height: i32, // Height of current row
    current_row_ascent: i32, // Ascent of current row
    current_row_is_overlay: bool, // True if current row is mode-line/echo area
    current_window_id: i32, // ID of current window being updated
    in_frame: bool,         // Whether we're currently in a frame update
    frame_counter: u64,     // Frame counter for tracking row updates
}

impl NeomacsDisplay {
    fn get_backend(&mut self) -> Option<&mut dyn DisplayBackend> {
        match self.backend_type {
            BackendType::Gtk4 => self.gtk4_backend.as_mut().map(|b| b as &mut dyn DisplayBackend),
            BackendType::Tty => self.tty_backend.as_mut().map(|b| b as &mut dyn DisplayBackend),
        }
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
        gtk4_backend: None,
        tty_backend: None,
        scene: Scene::new(800.0, 600.0),
        frame_glyphs: FrameGlyphBuffer::with_size(800.0, 600.0),  // Match initial scene size
        use_hybrid,
        animations: AnimationManager::new(),
        renderer: Gtk4Renderer::new(),
        gsk_renderer: GskRenderer::new(),
        hybrid_renderer: HybridRenderer::new(),
        use_gsk: true,  // Enable GSK rendering by default
        video_cache: VideoCache::new(),
        image_cache: ImageCache::new(),
        current_row_y: -1,
        current_row_x: 0,
        current_row_height: 0,
        current_row_ascent: 0,
        current_row_is_overlay: false,
        current_window_id: -1,
        in_frame: false,
        frame_counter: 0,
    });

    // Create the backend
    match backend {
        BackendType::Gtk4 => {
            // Initialize GTK4 library first
            if let Err(e) = gtk4::init() {
                eprintln!("Failed to initialize GTK4: {}", e);
                return ptr::null_mut();
            }

            let mut gtk4 = Gtk4Backend::new();
            if let Err(e) = gtk4.init() {
                eprintln!("Failed to initialize GTK4 backend: {}", e);
                return ptr::null_mut();
            }
            display.gtk4_backend = Some(gtk4);
        }
        BackendType::Tty => {
            let mut tty = TtyBackend::new();
            if let Err(e) = tty.init() {
                eprintln!("Failed to initialize TTY backend: {}", e);
                return ptr::null_mut();
            }
            display.tty_backend = Some(tty);
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
    let window_idx = display.scene.windows.iter().position(|w| w.window_id == window_id);

    if let Some(idx) = window_idx {
        // Update existing window
        let window = &mut display.scene.windows[idx];

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
        display.scene.windows.push(window);
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
    // Find the window by ID
    if let Some(window) = display.scene.windows.iter_mut().find(|w| w.window_id == window_id) {
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
            visible: visible != 0 && display.animations.cursor_visible(),
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
    display.scene.add_border(
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
    let window = display.scene.windows
        .iter_mut()
        .find(|w| w.window_id == current_window_id);

    let window = if let Some(w) = window {
        w
    } else {
        // Create default window if none exists for this ID
        // Use scene background (dark by default) instead of white
        display.scene.windows.push(crate::core::scene::WindowScene {
            window_id: current_window_id,
            bounds: crate::core::Rect::new(0.0, 0.0,
                display.scene.width as f32, display.scene.height as f32),
            background: display.scene.background, // Match scene background
            rows: Vec::new(),
            cursor: None,
            scroll_offset: 0.0,
            selected: true,
            mode_line_height: 0,
            header_line_height: 0,
            last_frame_touched: current_frame,
        });
        display.scene.windows.last_mut().unwrap()
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
        if let Some(window) = display.scene.windows.iter_mut().find(|w| w.window_id == current_window_id) {
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
        if let Some(window) = display.scene.windows.iter_mut().find(|w| w.window_id == current_window_id) {
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
    if let Some(window) = display.scene.windows.iter_mut().find(|w| w.window_id == current_window_id) {
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

    // Register face both in the renderer's cache AND in the scene
    // The scene will be cloned to the widget which has its own renderer
    display.scene.set_face(face.clone());
    display.gsk_renderer.face_cache_mut().insert(face);
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
    
    display.scene.background = bg;
    
    // Also set background for existing windows
    for window in &mut display.scene.windows {
        window.background = bg;
    }
}

// ============================================================================
// Image Management
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
    if let Some(window) = display.scene.windows.iter_mut().find(|w| w.window_id == current_window_id) {
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

/// Load a video from URI
/// Returns video_id on success, 0 on failure
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_video(
    handle: *mut NeomacsDisplay,
    uri: *const c_char,
) -> u32 {
    if handle.is_null() || uri.is_null() {
        return 0;
    }

    let display = &mut *handle;
    let uri_str = match CStr::from_ptr(uri).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    match display.video_cache.load(uri_str) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to load video: {}", e);
            0
        }
    }
}

/// Play a loaded video
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_play(
    handle: *mut NeomacsDisplay,
    video_id: u32,
) -> c_int {
    if handle.is_null() {
        return -1;
    }

    let display = &mut *handle;

    #[cfg(feature = "video")]
    if let Some(player) = display.video_cache.get_mut(video_id) {
        return match player.play() {
            Ok(()) => 0,
            Err(_) => -1,
        };
    }

    -1
}

/// Pause a video
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_pause(
    handle: *mut NeomacsDisplay,
    video_id: u32,
) -> c_int {
    if handle.is_null() {
        return -1;
    }

    let display = &mut *handle;

    #[cfg(feature = "video")]
    if let Some(player) = display.video_cache.get_mut(video_id) {
        return match player.pause() {
            Ok(()) => 0,
            Err(_) => -1,
        };
    }

    -1
}

/// Stop a video
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_stop(
    handle: *mut NeomacsDisplay,
    video_id: u32,
) -> c_int {
    if handle.is_null() {
        return -1;
    }

    let display = &mut *handle;

    #[cfg(feature = "video")]
    if let Some(player) = display.video_cache.get_mut(video_id) {
        return match player.stop() {
            Ok(()) => 0,
            Err(_) => -1,
        };
    }

    -1
}

/// Set video loop mode
/// count: -1 = infinite loop, 0 = no loop, n > 0 = loop n times
/// Returns 0 on success, -1 on failure
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_set_loop(
    handle: *mut NeomacsDisplay,
    video_id: u32,
    loop_count: c_int,
) -> c_int {
    if handle.is_null() {
        return -1;
    }

    let display = &mut *handle;

    #[cfg(feature = "video")]
    if let Some(player) = display.video_cache.get_mut(video_id) {
        player.set_looping(loop_count);
        return 0;
    }

    -1
}

/// Update video frame (called from Emacs redisplay)
/// Returns 0 on success, -1 on failure
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_video_update(
    handle: *mut NeomacsDisplay,
    video_id: u32,
) -> c_int {
    if handle.is_null() {
        return -1;
    }

    let display = &mut *handle;

    #[cfg(feature = "video")]
    if let Some(player) = display.video_cache.get_mut(video_id) {
        player.update();
        return 0;
    }

    -1
}

// ============================================================================
// Image Functions
// ============================================================================

/// Load an image from a file path
/// Returns image_id on success, 0 on failure
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image(
    handle: *mut NeomacsDisplay,
    path: *const c_char,
) -> u32 {
    if handle.is_null() || path.is_null() {
        return 0;
    }

    let display = &mut *handle;
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    match display.image_cache.load_from_file(std::path::Path::new(path_str)) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to load image: {}", e);
            0
        }
    }
}

/// Load an image from raw bytes
/// Returns image_id on success, 0 on failure
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_data(
    handle: *mut NeomacsDisplay,
    data: *const u8,
    len: usize,
) -> u32 {
    if handle.is_null() || data.is_null() || len == 0 {
        return 0;
    }

    let display = &mut *handle;
    let bytes = std::slice::from_raw_parts(data, len);

    match display.image_cache.load_from_bytes(bytes) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to load image data: {}", e);
            0
        }
    }
}

/// Load an image from raw bytes with optional scaling
/// Returns image_id on success, 0 on failure
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_data_scaled(
    handle: *mut NeomacsDisplay,
    data: *const u8,
    len: usize,
    max_width: c_int,
    max_height: c_int,
) -> u32 {
    if handle.is_null() || data.is_null() || len == 0 {
        return 0;
    }

    let display = &mut *handle;
    let bytes = std::slice::from_raw_parts(data, len);

    let mw = if max_width > 0 { Some(max_width) } else { None };
    let mh = if max_height > 0 { Some(max_height) } else { None };

    match display.image_cache.load_from_bytes_scaled(bytes, mw, mh) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to load image data: {}", e);
            0
        }
    }
}

/// Load an image from raw ARGB32 pixel data (Cairo/Emacs format)
/// Returns image_id on success, 0 on failure
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_argb32(
    handle: *mut NeomacsDisplay,
    data: *const u8,
    width: c_int,
    height: c_int,
    stride: c_int,
) -> u32 {
    if handle.is_null() || data.is_null() || width <= 0 || height <= 0 || stride <= 0 {
        return 0;
    }

    let display = &mut *handle;
    let data_len = (stride * height) as usize;
    let bytes = std::slice::from_raw_parts(data, data_len);

    match display.image_cache.load_from_argb32(bytes, width, height, stride) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to load ARGB32 image: {}", e);
            0
        }
    }
}

/// Load an image from raw RGB24 pixel data (no alpha)
/// Returns image_id on success, 0 on failure
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_load_image_rgb24(
    handle: *mut NeomacsDisplay,
    data: *const u8,
    width: c_int,
    height: c_int,
    stride: c_int,
) -> u32 {
    if handle.is_null() || data.is_null() || width <= 0 || height <= 0 || stride <= 0 {
        return 0;
    }

    let display = &mut *handle;
    let data_len = (stride * height) as usize;
    let bytes = std::slice::from_raw_parts(data, data_len);

    match display.image_cache.load_from_rgb24(bytes, width, height, stride) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to load RGB24 image: {}", e);
            0
        }
    }
}

/// Load an image from a file path
/// Returns image_id on success, 0 on failure
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

    match display.image_cache.load_from_file(path_str) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to load image file '{}': {}", path_str, e);
            0
        }
    }
}

/// Load an image from a file path with scaling
/// If max_width or max_height is 0, that dimension is not constrained
/// Returns image_id on success, 0 on failure
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

    let max_w = if max_width > 0 { Some(max_width) } else { None };
    let max_h = if max_height > 0 { Some(max_height) } else { None };

    match display.image_cache.load_from_file_scaled(path_str, max_w, max_h) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Failed to load scaled image file '{}': {}", path_str, e);
            0
        }
    }
}

/// Get image dimensions
/// Returns 0 on success, -1 on failure
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

    if let Some(img) = display.image_cache.get(image_id) {
        *width = img.width;
        *height = img.height;
        return 0;
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
    if display.image_cache.remove(image_id) {
        0
    } else {
        -1
    }
}

/// Set a floating video at a specific screen position
/// The video will be rendered on top of the frame
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
    display.scene.remove_floating_video(video_id);

    // Add new floating video
    display.scene.add_floating_video(
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
    display.scene.remove_floating_video(video_id);
}

/// Set a floating image at a specific screen position
/// The image will be rendered on top of the frame
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
    display.scene.remove_floating_image(image_id);

    // Add new floating image
    display.scene.add_floating_image(
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
    display.scene.remove_floating_image(image_id);
}

/// Clear a rectangular area of the display
/// Used by gui_clear_end_of_line and related functions
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

/// Clear all glyphs - used when frame layout changes (e.g., tab-bar-mode toggle)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_all_glyphs(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    eprintln!("DEBUG: clear_all_glyphs called, clearing {} glyphs", display.frame_glyphs.len());
    display.frame_glyphs.glyphs.clear();
    display.frame_glyphs.window_regions.clear();
    display.frame_glyphs.prev_window_regions.clear();
}

/// Clear all cursors - called at start of each frame to prevent ghost cursors
/// when focus changes between windows (e.g., buffer <-> minibuffer)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_all_cursors(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.frame_glyphs.glyphs.retain(|g| !matches!(g, FrameGlyph::Cursor { .. }));
}

/// Clear all borders (window dividers) - called at start of each frame
/// to prevent stale dividers when windows are deleted
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_clear_all_borders(handle: *mut NeomacsDisplay) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.frame_glyphs.glyphs.retain(|g| !matches!(g, FrameGlyph::Border { .. }));
}

/// End frame and render
/// Returns 0 on success, 1 if layout changed (Emacs should force refresh), -1 on error
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
        BackendType::Gtk4 => {
            if let Some(backend) = display.gtk4_backend.as_mut() {
                backend.render(&display.scene)
                    .and_then(|_| backend.present())
            } else {
                Ok(())
            }
        }
        BackendType::Tty => {
            if let Some(backend) = display.tty_backend.as_mut() {
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

/// Render the scene to an external Cairo context
///
/// # Safety
/// The cairo_context must be a valid cairo_t pointer from C.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_render_to_cairo(
    handle: *mut NeomacsDisplay,
    cairo_context: *mut c_void,
) -> c_int {
    if handle.is_null() || cairo_context.is_null() {
        return -1;
    }

    let display = &mut *handle;

    // Convert the C cairo_t pointer to a Rust Cairo context
    // This is safe because we're wrapping an existing context, not creating one
    let cr = match gtk4::cairo::Context::from_raw_borrow(cairo_context as *mut _) {
        cr => cr,
    };

    // Build the scene graph from accumulated data
    display.scene.build();

    // Render using GSK renderer (GPU-accelerated scene graph) or Cairo renderer
    if display.use_gsk {
        // Try to get webkit cache from thread-local storage
        #[cfg(feature = "wpe-webkit")]
        {
            WEBKIT_CACHE.with(|cache| {
                let cache_ref = cache.borrow();
                if let Some(webkit_cache) = cache_ref.as_ref() {
                    display.gsk_renderer.render_to_cairo_with_all_caches(
                        &cr,
                        &display.scene,
                        &display.video_cache,
                        &mut display.image_cache,
                        webkit_cache,
                    );
                } else {
                    display.gsk_renderer.render_to_cairo_with_caches(
                        &cr,
                        &display.scene,
                        &display.video_cache,
                        &mut display.image_cache,
                    );
                }
            });
        }

        #[cfg(not(feature = "wpe-webkit"))]
        {
            display.gsk_renderer.render_to_cairo_with_caches(
                &cr,
                &display.scene,
                &display.video_cache,
                &mut display.image_cache,
            );
        }
    } else {
        display.renderer.render(&cr, &display.scene);
    }

    0
}

/// Initialize the renderer with a Pango context (call after widget is realized)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_init_pango(
    handle: *mut NeomacsDisplay,
    pango_context: *mut c_void,
) {
    if handle.is_null() || pango_context.is_null() {
        return;
    }

    let display = &mut *handle;

    // Convert C PangoContext to Rust
    let context: gtk4::pango::Context = gtk4::glib::translate::from_glib_none(pango_context as *mut _);

    // Initialize both renderers with the Pango context
    display.renderer.init_with_context(context.clone());
    display.gsk_renderer.init_with_context(context);
}

/// Enable or disable GSK rendering
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_gsk_enabled(
    handle: *mut NeomacsDisplay,
    enabled: c_int,
) {
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    display.use_gsk = enabled != 0;
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
// GPU-Accelerated Widget (GSK)
// ============================================================================

use crate::backend::gtk4::{NeomacsWidget, set_widget_video_cache, set_widget_image_cache, set_widget_frame_glyphs, set_widget_use_hybrid, set_widget_floating_images, set_widget_floating_webkits};
#[cfg(feature = "wpe-webkit")]
use crate::backend::gtk4::set_widget_webkit_cache;

/// Create a GPU-accelerated NeomacsWidget
///
/// # Safety
/// Returns a pointer to a NeomacsWidget that can be added to a GTK container.
/// The widget is owned by GTK's reference counting system.
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_create_widget() -> *mut c_void {
    use gtk4::glib::translate::ToGlibPtr;
    use gtk4::prelude::{WidgetExt, Cast};

    let widget = NeomacsWidget::new();

    // Cast to gtk4::Widget first, then get the pointer
    let gtk_widget: gtk4::Widget = widget.upcast();
    let ptr: *mut gtk4::ffi::GtkWidget = gtk_widget.to_glib_full();
    ptr as *mut c_void
}

/// Set the scene on a NeomacsWidget (triggers GPU-accelerated redraw)
///
/// # Safety
/// handle must be a valid NeomacsDisplay pointer
/// widget must be a valid NeomacsWidget pointer
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_widget_set_scene(
    handle: *mut NeomacsDisplay,
    widget: *mut c_void,
) -> c_int {
    use gtk4::glib::translate::from_glib_none;

    if handle.is_null() || widget.is_null() {
        return -1;
    }

    let display = &mut *handle;
    let widget: NeomacsWidget = from_glib_none(widget as *mut _);

    // Clone the committed scene and set it on the widget
    widget.set_scene(display.scene.clone());

    0
}

/// Initialize the GSK renderer's Pango context from a NeomacsWidget
///
/// # Safety
/// handle must be valid, widget must be a realized NeomacsWidget
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_widget_init_pango(
    handle: *mut NeomacsDisplay,
    widget: *mut c_void,
) {
    use gtk4::glib::translate::from_glib_none;
    use gtk4::prelude::{WidgetExt, Cast};

    if handle.is_null() || widget.is_null() {
        return;
    }

    let display = &mut *handle;
    let widget: NeomacsWidget = from_glib_none(widget as *mut _);

    // Get Pango context from widget and initialize renderer
    let context = widget.pango_context();
    display.renderer.init_with_context(context);

    // Set widget for video frame invalidation callbacks early
    // This ensures the widget is available when videos start producing frames
    set_video_widget(Some(widget.clone().upcast::<gtk4::Widget>()));
}

/// Render scene to a NeomacsWidget using GSK (GPU-accelerated)
///
/// This renders directly using GSK render nodes for GPU acceleration.
///
/// # Safety
/// handle must be valid, widget must be a valid NeomacsWidget
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_render_to_widget(
    handle: *mut NeomacsDisplay,
    widget: *mut c_void,
) -> c_int {
    use gtk4::glib::translate::from_glib_none;
    use gtk4::prelude::{WidgetExt, Cast};

    if handle.is_null() || widget.is_null() {
        warn!("render_to_widget: null handle or widget");
        return -1;
    }

    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        let display = &mut *handle;
        let widget: NeomacsWidget = from_glib_none(widget as *mut _);

        // Set widget for video frame invalidation callbacks
        // This allows video paintables to trigger redraws when new frames arrive
        set_video_widget(Some(widget.clone().upcast::<gtk4::Widget>()));

        // Set caches for widget rendering (thread-local storage)
        // The widget's snapshot() method will use these during GTK's draw cycle
        set_widget_video_cache(&display.video_cache as *const VideoCache);
        set_widget_image_cache(&mut display.image_cache as *mut ImageCache);

        // Set hybrid mode flag
        set_widget_use_hybrid(display.use_hybrid);

        // Update all webkit views to process pending frames
        #[cfg(feature = "wpe-webkit")]
        WEBKIT_CACHE.with(|cache| {
            if let Some(ref mut c) = *cache.borrow_mut() {
                c.update_all();
            }
        });

        if display.use_hybrid {
            // Hybrid path: pass FrameGlyphBuffer to widget via thread-local
            debug!("render_to_widget: hybrid mode, {} glyphs", display.frame_glyphs.len());
            set_widget_frame_glyphs(&display.frame_glyphs as *const FrameGlyphBuffer);

            // Pass floating images to widget for overlay rendering
            set_widget_floating_images(display.scene.floating_images.clone());

            // Pass floating webkits to widget for overlay rendering
            set_widget_floating_webkits(display.scene.floating_webkits.clone());

            // Set webkit cache for widget rendering
            #[cfg(feature = "wpe-webkit")]
            WEBKIT_CACHE.with(|cache| {
                if let Some(ref c) = *cache.borrow() {
                    set_widget_webkit_cache(c as *const WebKitCache);
                }
            });

            // Trigger redraw - widget will read from thread-local frame_glyphs
            widget.queue_draw();
        } else {
            // Legacy path: build scene and set on widget
            debug!("render_to_widget: legacy mode, building scene");
            display.scene.build();
            let cloned_scene = display.scene.clone();
            widget.set_scene(cloned_scene);
        }
    }));

    if let Err(e) = result {
        error!("PANIC in render_to_widget: {:?}", e);
        return -1;
    }

    0
}

/// Type for the resize callback function pointer from C
pub type ResizeCallbackFn = extern "C" fn(user_data: *mut c_void, width: c_int, height: c_int);

/// Set the resize callback for the NeomacsWidget
/// The callback will be called whenever the widget is resized
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_resize_callback(
    callback: ResizeCallbackFn,
    user_data: *mut c_void,
) {
    use crate::backend::gtk4::set_widget_resize_callback;

    // Store the raw pointer - it must remain valid for the lifetime of the callback
    let user_data_ptr = user_data as usize;

    set_widget_resize_callback(move |width, height| {
        // Call the C callback
        callback(user_data_ptr as *mut c_void, width as c_int, height as c_int);
    });
}

// ============================================================================
// Mouse Event Callbacks
// ============================================================================

/// Type for mouse button callback: (user_data, x, y, button, pressed, modifiers, time)
pub type MouseButtonCallbackFn = extern "C" fn(
    user_data: *mut c_void,
    x: c_double,
    y: c_double,
    button: c_uint,
    pressed: c_int,
    modifiers: c_uint,
    time: c_uint,
);

/// Type for mouse motion callback: (user_data, x, y, modifiers, time)
pub type MouseMotionCallbackFn = extern "C" fn(
    user_data: *mut c_void,
    x: c_double,
    y: c_double,
    modifiers: c_uint,
    time: c_uint,
);

/// Type for mouse scroll callback: (user_data, x, y, delta_x, delta_y, modifiers, time)
pub type MouseScrollCallbackFn = extern "C" fn(
    user_data: *mut c_void,
    x: c_double,
    y: c_double,
    delta_x: c_double,
    delta_y: c_double,
    modifiers: c_uint,
    time: c_uint,
);

/// Set the mouse button callback for the NeomacsWidget
/// Called on button press (pressed=1) and release (pressed=0)
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_mouse_button_callback(
    callback: MouseButtonCallbackFn,
    user_data: *mut c_void,
) {
    use crate::backend::gtk4::set_widget_mouse_button_callback;

    let user_data_ptr = user_data as usize;

    set_widget_mouse_button_callback(move |x, y, button, pressed, modifiers, time| {
        log::debug!("FFI: Mouse button callback -> C: x={:.1}, y={:.1}, btn={}, pressed={}",
                   x, y, button, pressed);
        callback(
            user_data_ptr as *mut c_void,
            x as c_double,
            y as c_double,
            button as c_uint,
            if pressed { 1 } else { 0 },
            modifiers as c_uint,
            time as c_uint,
        );
    });
}

/// Set the mouse motion callback for the NeomacsWidget
/// Called on mouse movement
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_mouse_motion_callback(
    callback: MouseMotionCallbackFn,
    user_data: *mut c_void,
) {
    use crate::backend::gtk4::set_widget_mouse_motion_callback;

    let user_data_ptr = user_data as usize;

    set_widget_mouse_motion_callback(move |x, y, modifiers, time| {
        // Only log occasionally to avoid spam (every ~100th call based on time)
        if time % 1000 < 10 {
            log::trace!("FFI: Mouse motion callback -> C: x={:.1}, y={:.1}", x, y);
        }
        callback(
            user_data_ptr as *mut c_void,
            x as c_double,
            y as c_double,
            modifiers as c_uint,
            time as c_uint,
        );
    });
}

/// Set the mouse scroll callback for the NeomacsWidget
/// Called on scroll wheel events
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_mouse_scroll_callback(
    callback: MouseScrollCallbackFn,
    user_data: *mut c_void,
) {
    use crate::backend::gtk4::set_widget_mouse_scroll_callback;

    let user_data_ptr = user_data as usize;

    set_widget_mouse_scroll_callback(move |x, y, dx, dy, modifiers, time| {
        log::debug!("FFI: Mouse scroll callback -> C: dx={:.2}, dy={:.2}", dx, dy);
        callback(
            user_data_ptr as *mut c_void,
            x as c_double,
            y as c_double,
            dx as c_double,
            dy as c_double,
            modifiers as c_uint,
            time as c_uint,
        );
    });
}

// ============================================================================
// WebKit Integration
// ============================================================================

// ============================================================================
// WebKit Functions
// ============================================================================

#[cfg(feature = "wpe-webkit")]
use std::cell::RefCell;
#[cfg(feature = "wpe-webkit")]
use crate::backend::webkit::WebKitCache;
#[cfg(feature = "wpe-webkit")]
use crate::backend::wpe::WpeBackend;

#[cfg(feature = "wpe-webkit")]
thread_local! {
    static WEBKIT_CACHE: RefCell<Option<WebKitCache>> = const { RefCell::new(None) };
    static WPE_BACKEND: RefCell<Option<WpeBackend>> = const { RefCell::new(None) };
}

/// Initialize WebKit subsystem with EGL display
/// Must be called before creating WebKit views
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
                    *cache.borrow_mut() = Some(WebKitCache::new());
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
    // Link to EGL
    extern "C" {
        fn eglGetCurrentDisplay() -> *mut libc::c_void;
    }
    eglGetCurrentDisplay()
}

/// Create a new WebKit view
/// Returns view_id on success, 0 on failure
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
    display.scene.floating_webkits.retain(|w| w.webkit_id != webkit_id);

    // Add webkit at position
    display.scene.add_floating_webkit(
        webkit_id,
        x as f32,
        y as f32,
        width as f32,
        height as f32,
    );
    info!("neomacs_display_set_floating_webkit: now have {} floating webkits", display.scene.floating_webkits.len());
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
    display.scene.remove_floating_webkit(webkit_id);
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

/// Get WebKit view title (returns null-terminated string, caller must free)
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

/// Get WebKit view URL (returns null-terminated string, caller must free)
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

/// Get WebKit view loading progress (0.0 - 1.0), returns -1 if view not found
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

/// Check if WebKit view is loading (1=loading, 0=not loading, -1=not found)
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
    if handle.is_null() {
        return;
    }

    let display = &mut *handle;
    let current_y = display.current_row_y;
    let current_x = display.current_row_x;

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
    if let Some(window) = display.scene.windows.first_mut() {
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

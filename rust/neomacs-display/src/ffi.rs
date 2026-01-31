//! C FFI layer for integration with Emacs.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::ptr;

use crate::backend::{BackendType, DisplayBackend};
use crate::backend::gtk4::{Gtk4Backend, Gtk4Renderer, GskRenderer, VideoCache, ImageCache, set_video_widget};
use crate::backend::tty::TtyBackend;
use crate::core::types::{Color, Rect};
use crate::core::scene::{Scene, WindowScene, CursorState, CursorStyle};
use crate::core::glyph::{Glyph, GlyphRow, GlyphType, GlyphData};
use crate::core::animation::AnimationManager;

/// Opaque handle to the display engine
pub struct NeomacsDisplay {
    backend_type: BackendType,
    gtk4_backend: Option<Gtk4Backend>,
    tty_backend: Option<TtyBackend>,
    scene: Scene,           // The scene for rendering
    animations: AnimationManager,
    renderer: Gtk4Renderer,  // Cairo renderer for external Cairo context
    gsk_renderer: GskRenderer, // GSK renderer for GPU-accelerated rendering
    use_gsk: bool,          // Whether to use GSK rendering
    video_cache: VideoCache, // Video player cache
    image_cache: ImageCache, // Image cache
    current_row_y: i32,     // Y position of current row being built
    current_row_x: i32,     // X position for next glyph in current row
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
    let mut display = Box::new(NeomacsDisplay {
        backend_type: backend,
        gtk4_backend: None,
        tty_backend: None,
        scene: Scene::new(800.0, 600.0),
        animations: AnimationManager::new(),
        renderer: Gtk4Renderer::new(),
        gsk_renderer: GskRenderer::new(),
        use_gsk: true,  // Enable GSK rendering by default
        video_cache: VideoCache::new(),
        image_cache: ImageCache::new(),
        current_row_y: -1,
        current_row_x: 0,
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
    // Don't clear rows here - Emacs does incremental updates
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

    // Track current window (for backward compatibility, though we use single window)
    display.current_window_id = window_id;

    // Use single window approach - update first window or create if none
    if display.scene.windows.is_empty() {
        // Create the one and only window
        let window = WindowScene {
            window_id: 0,  // Use fixed ID 0
            bounds: Rect::new(x, y, width, height),
            background: Color::from_pixel(bg_color),
            rows: Vec::new(),
            cursor: None,
            scroll_offset: 0.0,
            selected: selected != 0,
            mode_line_height: 0,
            header_line_height: 0,
        };
        display.scene.windows.push(window);
    } else {
        // Update the single window's properties (use largest bounds)
        let window = &mut display.scene.windows[0];
        // Expand bounds to encompass all Emacs windows
        let new_right = (x + width).max(window.bounds.x + window.bounds.width);
        let new_bottom = (y + height).max(window.bounds.y + window.bounds.height);
        window.bounds.width = new_right - window.bounds.x.min(x);
        window.bounds.height = new_bottom - window.bounds.y.min(y);
        window.bounds.x = window.bounds.x.min(x);
        window.bounds.y = window.bounds.y.min(y);
        window.selected = selected != 0 || window.selected;
    }
}

/// Set cursor for the most recently added window
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_set_cursor(
    handle: *mut NeomacsDisplay,
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

    // Use staging scene
    if let Some(window) = display.scene.windows.last_mut() {
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

// ============================================================================
// Glyph Row Management
// ============================================================================

/// Begin a new glyph row for the current window
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_begin_row(
    handle: *mut NeomacsDisplay,
    y: c_int,
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

    // Track current row Y and X for glyph additions
    display.current_row_y = y;
    display.current_row_x = x;  // Set starting X for this glyph string

    // Use the FIRST window for all content (ignore window_id)
    // This simplifies everything since we render to a single GTK widget
    let window = if display.scene.windows.is_empty() {
        // Create default window if none exists
        display.scene.windows.push(crate::core::scene::WindowScene {
            window_id: 0,
            bounds: crate::core::Rect::new(0.0, 0.0, 
                display.scene.width as f32, display.scene.height as f32),
            background: crate::core::Color::from_u8(0, 0, 0, 255),
            rows: Vec::new(),
            cursor: None,
            scroll_offset: 0.0,
            selected: true,
            mode_line_height: 0,
            header_line_height: 0,
        });
        &mut display.scene.windows[0]
    } else {
        &mut display.scene.windows[0]
    };
    
    // Look for existing row at this Y position
    if let Some(existing_row) = window.rows.iter_mut().find(|r| r.y == y) {
        // DON'T clear the entire row - instead we use X-position based replacement
        // when adding glyphs. This preserves content that Emacs doesn't redraw.
        
        // Update the properties
        existing_row.height = height;
        existing_row.visible_height = height;
        existing_row.ascent = ascent;
        existing_row.mode_line = mode_line != 0;
        existing_row.header_line = header_line != 0;
    } else {
        // Add new row
        window.rows.push(GlyphRow {
            glyphs: Vec::new(),
            y,
            height,
            visible_height: height,
            ascent,
            enabled: true,
            cursor_in_row: false,
            mode_line: mode_line != 0,
            header_line: header_line != 0,
            last_frame_cleared: current_frame,
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

    let display = &mut *handle;
    let current_y = display.current_row_y;
    let current_x = display.current_row_x;
    let c = char::from_u32(charcode).unwrap_or('\u{FFFD}');

    // Use first window for all content
    if let Some(window) = display.scene.windows.first_mut() {
        if let Some(row) = window.rows.iter_mut().find(|r| r.y == current_y) {
            // Remove any existing glyphs that overlap this X range
            let x_start = current_x;
            let x_end = current_x + pixel_width;
            row.glyphs.retain(|g| {
                // Keep glyphs that don't overlap with our new glyph's X range
                let g_end = g.x + g.pixel_width;
                g_end <= x_start || g.x >= x_end
            });
            
            let glyph = Glyph {
                glyph_type: GlyphType::Char,
                charcode,
                face_id,
                x: current_x,
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
            
            // Advance X position for next glyph
            display.current_row_x += pixel_width;
        }
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

    let display = &mut *handle;
    let current_y = display.current_row_y;
    let current_x = display.current_row_x;

    // Use first window for all content
    if let Some(window) = display.scene.windows.first_mut() {
        if let Some(row) = window.rows.iter_mut().find(|r| r.y == current_y) {
            // Remove any existing glyphs that overlap this X range
            let x_start = current_x;
            let x_end = current_x + pixel_width;
            row.glyphs.retain(|g| {
                let g_end = g.x + g.pixel_width;
                g_end <= x_start || g.x >= x_end
            });
            
            let glyph = Glyph {
                glyph_type: GlyphType::Stretch,
                charcode: 0,
                face_id,
                x: current_x,
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
            
            // Advance X position
            display.current_row_x += pixel_width;
        }
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
    let current_y = display.current_row_y;
    let current_x = display.current_row_x;

    // Use first window for all content
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
                glyph_type: GlyphType::Image,
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
                data: GlyphData::Image { image_id },
            };
            row.glyphs.push(glyph);
            
            // Advance X position
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
        font_family: "monospace".to_string(),
        font_size: 14.0,
        font_weight,
        attributes: attrs,
        underline_style: ul_style,
        box_type: bx_type,
        box_line_width,
    };

    // Register face both in the renderer's cache AND in the scene
    // The scene will be cloned to the widget which has its own renderer
    display.scene.set_face(face.clone());
    display.gsk_renderer.face_cache_mut().insert(face);
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
    let current_y = display.current_row_y;
    let current_x = display.current_row_x;

    // Use first window for all content
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
                glyph_type: GlyphType::Video,
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
                data: GlyphData::Video { video_id },
            };
            row.glyphs.push(glyph);
            
            // Advance X position
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

/// End frame and render
#[no_mangle]
pub unsafe extern "C" fn neomacs_display_end_frame(handle: *mut NeomacsDisplay) -> c_int {
    if handle.is_null() {
        return -1;
    }

    let display = &mut *handle;

    // Reset frame flag
    display.in_frame = false;

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

    0
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

use crate::backend::gtk4::{NeomacsWidget, set_widget_video_cache, set_widget_image_cache};

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
        return -1;
    }

    let display = &mut *handle;
    let widget: NeomacsWidget = from_glib_none(widget as *mut _);

    // Set widget for video frame invalidation callbacks
    // This allows video paintables to trigger redraws when new frames arrive
    set_video_widget(Some(widget.clone().upcast::<gtk4::Widget>()));

    // Set caches for widget rendering (thread-local storage)
    // The widget's snapshot() method will use these during GTK's draw cycle
    set_widget_video_cache(&display.video_cache as *const VideoCache);
    set_widget_image_cache(&mut display.image_cache as *mut ImageCache);

    // Always build and set the scene (needed for floating videos even without glyphs)
    display.scene.build();
    
    let cloned_scene = display.scene.clone();
    widget.set_scene(cloned_scene);

    0
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

                log::info!("WebKit subsystem initialized");
                return 0;
            }
            Err(e) => {
                eprintln!("Failed to initialize WPE backend: {}", e);
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
    if handle.is_null() {
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

    // Use first window for all content
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

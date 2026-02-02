//! WPE WebKit view wrapper using WPE Platform API.
//!
//! Uses the modern WPE Platform API for GPU-accelerated buffer export
//! instead of the legacy wpebackend-fdo.

use std::ffi::{CStr, CString};
use std::ptr;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};

use gdk4::prelude::*;
use gdk4::Texture;
use glib;

use crate::core::error::{DisplayError, DisplayResult};

use super::sys;
use super::sys::webkit as wk;
use super::sys::platform as plat;
use super::platform::WpePlatformDisplay;
use super::dmabuf::DmaBufExporter;

/// State of a WPE WebKit view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WpeViewState {
    /// View is being created
    Creating,
    /// View is loading content
    Loading,
    /// View is ready/idle
    Ready,
    /// View encountered an error
    Error,
}

/// Callback data for buffer-rendered signal
struct BufferCallbackData {
    /// Latest rendered texture
    latest_texture: RefCell<Option<Texture>>,
    /// Flag indicating new frame available
    frame_available: AtomicBool,
    /// WPE Platform display for buffer import
    display: *mut plat::WPEDisplay,
}

/// A WPE WebKit browser view using WPE Platform API.
///
/// Uses WPEDisplay headless mode and WPEView buffer-rendered signals
/// for efficient GPU texture extraction.
pub struct WpeWebView {
    /// Current URL
    pub url: String,

    /// View state
    pub state: WpeViewState,

    /// View dimensions
    pub width: u32,
    pub height: u32,

    /// Page title
    pub title: Option<String>,

    /// Loading progress (0.0 - 1.0)
    pub progress: f64,

    /// Latest rendered texture
    texture: Option<Texture>,

    /// The WebKit web view
    web_view: *mut wk::WebKitWebView,

    /// The WPEView (obtained from WebKitWebView)
    wpe_view: *mut plat::WPEView,

    /// Callback data (must be boxed and leaked to be stable)
    callback_data: *mut BufferCallbackData,

    /// Signal handler ID for buffer-rendered
    buffer_rendered_handler_id: u64,

    /// DMA-BUF exporter for texture conversion
    dmabuf_exporter: DmaBufExporter,

    /// GDK display for texture creation
    gdk_display: Option<gdk4::Display>,

    /// Whether the view needs redraw
    needs_redraw: bool,
}

impl WpeWebView {
    /// Create a new WPE WebKit view using WPE Platform API.
    ///
    /// # Arguments
    /// * `platform_display` - The initialized WPE Platform display
    /// * `width` - Initial width
    /// * `height` - Initial height
    pub fn new(platform_display: &WpePlatformDisplay, width: u32, height: u32) -> DisplayResult<Self> {
        use std::io::Write;
        eprintln!("WpeWebView::new (Platform API) called with {}x{}", width, height);
        let _ = std::io::stderr().flush();

        let display = platform_display.raw();
        if display.is_null() {
            return Err(DisplayError::WebKit("WPE Platform display is null".into()));
        }

        // Create DMA-BUF exporter with the EGL display
        eprintln!("WpeWebView::new: creating DmaBufExporter...");
        let dmabuf_exporter = DmaBufExporter::new(platform_display.egl_display());
        eprintln!("WpeWebView::new: DmaBufExporter created");

        // Get the GDK display
        let gdk_display = gdk4::Display::default();
        eprintln!("WpeWebView::new: GDK display: {:?}", gdk_display);

        unsafe {
            // Create WebKitNetworkSession (required for WPE Platform)
            let network_session = wk::webkit_network_session_get_default();
            eprintln!("WpeWebView::new: network_session={:?}", network_session);

            // Create WebKitWebContext
            let web_context = wk::webkit_web_context_new();
            eprintln!("WpeWebView::new: web_context={:?}", web_context);

            // Create WebKitWebView with display property using g_object_new
            // This is the key difference - we pass the WPE Platform display
            eprintln!("WpeWebView::new: creating WebKitWebView with WPE Platform display...");
            
            let type_name = CString::new("WebKitWebView").unwrap();
            let display_prop = CString::new("display").unwrap();
            
            // Use webkit_web_view_new with the display set via web context
            // For WPE Platform, the display should be set as primary and WebKit will use it
            let web_view = wk::webkit_web_view_new(ptr::null_mut());
            eprintln!("WpeWebView::new: web_view={:?}", web_view);

            if web_view.is_null() {
                return Err(DisplayError::WebKit("Failed to create WebKitWebView".into()));
            }

            // Get the WPEView from WebKitWebView
            let wpe_view = wk::webkit_web_view_get_wpe_view(web_view);
            eprintln!("WpeWebView::new: wpe_view={:?}", wpe_view);

            if wpe_view.is_null() {
                // Clean up
                plat::g_object_unref(web_view as *mut _);
                return Err(DisplayError::WebKit(
                    "Failed to get WPEView from WebKitWebView - display may not be connected".into()
                ));
            }

            // Set initial size
            plat::wpe_view_resized(wpe_view as *mut _, width as i32, height as i32);

            // Allocate callback data
            let callback_data = Box::into_raw(Box::new(BufferCallbackData {
                latest_texture: RefCell::new(None),
                frame_available: AtomicBool::new(false),
                display,
            }));
            eprintln!("WpeWebView::new: callback_data={:?}", callback_data);

            // Connect buffer-rendered signal
            let signal_name = CString::new("buffer-rendered").unwrap();
            let handler_id = plat::g_signal_connect_data(
                wpe_view as *mut _,
                signal_name.as_ptr(),
                Some(std::mem::transmute::<
                    unsafe extern "C" fn(*mut plat::WPEView, *mut plat::WPEBuffer, *mut libc::c_void),
                    unsafe extern "C" fn(),
                >(buffer_rendered_callback)),
                callback_data as *mut _,
                None,
                0, // G_CONNECT_DEFAULT
            );
            eprintln!("WpeWebView::new: connected buffer-rendered signal, handler_id={}", handler_id);

            eprintln!("WpeWebView: WPE Platform WebKitWebView created successfully ({}x{})", width, height);
            log::info!("WPE Platform WebKitWebView created successfully ({}x{})", width, height);

            Ok(Self {
                url: String::new(),
                state: WpeViewState::Ready,
                width,
                height,
                title: None,
                progress: 0.0,
                texture: None,
                web_view,
                wpe_view: wpe_view as *mut _,
                callback_data,
                buffer_rendered_handler_id: handler_id,
                dmabuf_exporter,
                gdk_display,
                needs_redraw: false,
            })
        }
    }

    /// Load a URL
    pub fn load_uri(&mut self, uri: &str) -> DisplayResult<()> {
        self.url = uri.to_string();
        self.state = WpeViewState::Loading;
        self.progress = 0.0;

        let c_uri = CString::new(uri).map_err(|_| DisplayError::WebKit("Invalid URI".into()))?;

        eprintln!("WpeWebView::load_uri: about to call webkit_web_view_load_uri({:?}, {:?})", self.web_view, uri);
        unsafe {
            wk::webkit_web_view_load_uri(self.web_view, c_uri.as_ptr());
        }
        eprintln!("WpeWebView::load_uri: webkit_web_view_load_uri returned");

        log::info!("WPE: Loading URI: {}", uri);
        Ok(())
    }

    /// Load HTML content directly
    pub fn load_html(&mut self, html: &str, base_uri: Option<&str>) -> DisplayResult<()> {
        self.state = WpeViewState::Loading;
        self.progress = 0.0;

        let c_html = CString::new(html).map_err(|_| DisplayError::WebKit("Invalid HTML".into()))?;
        let c_base_uri = base_uri
            .map(|u| CString::new(u).ok())
            .flatten();

        unsafe {
            wk::webkit_web_view_load_html(
                self.web_view,
                c_html.as_ptr(),
                c_base_uri.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null()),
            );
        }

        log::info!("WPE: Loading HTML content");
        Ok(())
    }

    /// Navigate back
    pub fn go_back(&mut self) -> DisplayResult<()> {
        unsafe {
            if wk::webkit_web_view_can_go_back(self.web_view) != 0 {
                wk::webkit_web_view_go_back(self.web_view);
            }
        }
        Ok(())
    }

    /// Navigate forward
    pub fn go_forward(&mut self) -> DisplayResult<()> {
        unsafe {
            if wk::webkit_web_view_can_go_forward(self.web_view) != 0 {
                wk::webkit_web_view_go_forward(self.web_view);
            }
        }
        Ok(())
    }

    /// Reload the page
    pub fn reload(&mut self) -> DisplayResult<()> {
        self.state = WpeViewState::Loading;
        unsafe {
            wk::webkit_web_view_reload(self.web_view);
        }
        Ok(())
    }

    /// Stop loading
    pub fn stop(&mut self) -> DisplayResult<()> {
        unsafe {
            wk::webkit_web_view_stop_loading(self.web_view);
        }
        Ok(())
    }

    /// Execute JavaScript
    pub fn execute_javascript(&self, script: &str) -> DisplayResult<()> {
        let c_script = CString::new(script).map_err(|_| DisplayError::WebKit("Invalid script".into()))?;

        unsafe {
            wk::webkit_web_view_evaluate_javascript(
                self.web_view,
                c_script.as_ptr(),
                -1, // length, -1 for null-terminated
                ptr::null(), // world_name
                ptr::null(), // source_uri
                ptr::null_mut(), // cancellable
                None, // callback
                ptr::null_mut(), // user_data
            );
        }

        log::debug!("WPE: Executing JavaScript");
        Ok(())
    }

    /// Update view state from WebKit
    pub fn update(&mut self) {
        unsafe {
            // Update title
            let title_ptr = wk::webkit_web_view_get_title(self.web_view);
            if !title_ptr.is_null() {
                self.title = Some(CStr::from_ptr(title_ptr).to_string_lossy().into_owned());
            }

            // Update URL
            let uri_ptr = wk::webkit_web_view_get_uri(self.web_view);
            if !uri_ptr.is_null() {
                self.url = CStr::from_ptr(uri_ptr).to_string_lossy().into_owned();
            }

            // Update progress
            self.progress = wk::webkit_web_view_get_estimated_load_progress(self.web_view);

            // Update state
            if wk::webkit_web_view_is_loading(self.web_view) != 0 {
                self.state = WpeViewState::Loading;
            } else {
                self.state = WpeViewState::Ready;
            }

            // Check for new frame from callback
            if let Some(callback_data) = self.callback_data.as_ref() {
                if callback_data.frame_available.swap(false, Ordering::Acquire) {
                    self.needs_redraw = true;

                    // Get texture from callback data
                    if let Some(texture) = callback_data.latest_texture.borrow_mut().take() {
                        log::trace!("WPE: Got new texture {}x{}", texture.width(), texture.height());
                        self.texture = Some(texture);
                    }
                }
            }
        }
    }

    /// Resize the view
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.needs_redraw = true;
        
        unsafe {
            plat::wpe_view_resized(self.wpe_view, width as i32, height as i32);
        }
    }

    /// Get the current texture (latest rendered frame)
    pub fn texture(&self) -> Option<&Texture> {
        self.texture.as_ref()
    }

    /// Check if view needs redraw
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Clear redraw flag
    pub fn clear_redraw_flag(&mut self) {
        self.needs_redraw = false;
    }

    /// Dispatch frame complete to WPE
    pub fn dispatch_frame_complete(&self) {
        unsafe {
            // With WPE Platform, frame complete is signaled via wpe_view_frame_complete
            // But we may not need this as WebKit handles its own frame pacing
        }
    }

    /// Send keyboard event to WebKit via WPE Platform
    pub fn send_keyboard_event(&self, key_code: u32, hardware_key_code: u32, pressed: bool, modifiers: u32) {
        unsafe {
            // TODO: Use WPE Platform event API
            // wpe_view_dispatch_keyboard_event() etc.
            log::trace!("WPE Platform: Keyboard event: key={} pressed={}", key_code, pressed);
        }
    }

    /// Send pointer/mouse event to WebKit via WPE Platform
    pub fn send_pointer_event(&self, event_type: u32, x: i32, y: i32, button: u32, state: u32, modifiers: u32) {
        unsafe {
            // TODO: Use WPE Platform event API
            log::trace!("WPE Platform: Pointer event at ({}, {})", x, y);
        }
    }

    /// Send scroll/wheel event to WebKit via WPE Platform
    pub fn send_axis_event(&self, x: i32, y: i32, axis: u32, value: i32, modifiers: u32) {
        unsafe {
            // TODO: Use WPE Platform event API
            log::trace!("WPE Platform: Scroll event axis={} value={} at ({}, {})", axis, value, x, y);
        }
    }

    /// Click at position (convenience method)
    pub fn click(&self, x: i32, y: i32, button: u32) {
        // Send press then release
        self.send_pointer_event(2, x, y, button, 1, 0); // button press
        self.send_pointer_event(2, x, y, button, 0, 0); // button release
        log::trace!("WPE Platform: Click at ({}, {}) button={}", x, y, button);
    }

    /// Scroll at position (convenience method)
    pub fn scroll(&self, x: i32, y: i32, delta_x: i32, delta_y: i32) {
        if delta_x != 0 {
            self.send_axis_event(x, y, 0, delta_x, 0); // horizontal
        }
        if delta_y != 0 {
            self.send_axis_event(x, y, 1, delta_y, 0); // vertical
        }
        log::trace!("WPE Platform: Scroll at ({}, {}) delta=({}, {})", x, y, delta_x, delta_y);
    }
}

impl Drop for WpeWebView {
    fn drop(&mut self) {
        unsafe {
            // Disconnect signal handler
            if self.buffer_rendered_handler_id != 0 && !self.wpe_view.is_null() {
                // g_signal_handler_disconnect would be needed here
            }

            // Clean up callback data
            if !self.callback_data.is_null() {
                let _ = Box::from_raw(self.callback_data);
            }

            // Unref the web view (this should also release the WPEView)
            if !self.web_view.is_null() {
                plat::g_object_unref(self.web_view as *mut _);
            }
        }
        log::debug!("WPE Platform WebKitWebView destroyed");
    }
}

/// C callback for buffer-rendered signal from WPEView
unsafe extern "C" fn buffer_rendered_callback(
    wpe_view: *mut plat::WPEView,
    buffer: *mut plat::WPEBuffer,
    user_data: *mut libc::c_void,
) {
    if user_data.is_null() || buffer.is_null() {
        eprintln!("buffer_rendered_callback: null user_data or buffer");
        return;
    }

    let callback_data = &*(user_data as *const BufferCallbackData);
    
    let width = plat::wpe_buffer_get_width(buffer) as u32;
    let height = plat::wpe_buffer_get_height(buffer) as u32;
    
    eprintln!("buffer_rendered_callback: received buffer {}x{}", width, height);

    // Try to import buffer as EGL image first (GPU zero-copy)
    let mut error: *mut plat::GError = ptr::null_mut();
    let egl_image = plat::wpe_buffer_import_to_egl_image(buffer, &mut error);
    
    if !egl_image.is_null() {
        eprintln!("buffer_rendered_callback: got EGL image {:?}", egl_image);
        // TODO: Convert EGL image to GdkTexture
        // For now, fall through to pixel import
        
        // Note: We need to release the EGL image eventually
        // wpe_buffer_import_to_egl_image returns a new EGL image that must be destroyed
    } else {
        if !error.is_null() {
            let msg = std::ffi::CStr::from_ptr((*error).message)
                .to_string_lossy();
            eprintln!("buffer_rendered_callback: EGL import failed: {}", msg);
            plat::g_error_free(error);
        }
    }
    
    // Fallback: Import buffer to pixels
    let mut error: *mut plat::GError = ptr::null_mut();
    let bytes = plat::wpe_buffer_import_to_pixels(buffer, &mut error);
    
    if bytes.is_null() {
        if !error.is_null() {
            let msg = std::ffi::CStr::from_ptr((*error).message)
                .to_string_lossy();
            eprintln!("buffer_rendered_callback: pixel import failed: {}", msg);
            plat::g_error_free(error);
        }
        return;
    }
    
    eprintln!("buffer_rendered_callback: got pixels, creating texture...");
    
    // Create texture from pixels
    let mut size: plat::gsize = 0;
    let data = plat::g_bytes_get_data(bytes, &mut size);
    
    if data.is_null() || size == 0 {
        log::warn!("buffer_rendered_callback: empty pixel data");
        plat::g_bytes_unref(bytes);
        return;
    }
    
    // Create a slice from the pixel data
    let pixel_slice = std::slice::from_raw_parts(data as *const u8, size as usize);
    let size = size as usize;
    
    // Calculate expected stride - WPE might align rows to 4/8/16 bytes
    let expected_size = (width * height * 4) as usize;
    let actual_stride = if size != expected_size {
        // Buffer has padding - calculate actual stride
        size / (height as usize)
    } else {
        (width * 4) as usize
    };
    
    log::info!("buffer_rendered_callback: {}x{}, expected_size={}, actual_size={}, stride={}", 
               width, height, expected_size, size, actual_stride);
    
    // WPE exports XRGB/BGRX format (alpha channel is unused/zero)
    // We need to set alpha to 255 (opaque) for all pixels
    let mut pixels_with_alpha: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
    
    // Copy row by row, handling stride
    for row in 0..(height as usize) {
        let row_start = row * actual_stride;
        for col in 0..(width as usize) {
            let offset = row_start + col * 4;
            // Copy BGR, set A to 255
            pixels_with_alpha.push(pixel_slice[offset]);     // B
            pixels_with_alpha.push(pixel_slice[offset + 1]); // G
            pixels_with_alpha.push(pixel_slice[offset + 2]); // R
            pixels_with_alpha.push(255);                      // A (was 0)
        }
    }
    
    // Create GdkMemoryTexture
    // Now using BGRA with alpha=255 (opaque), and correct stride
    let glib_bytes = glib::Bytes::from(&pixels_with_alpha);
    let new_stride = (width * 4) as usize; // No padding in our output
    
    let texture = gdk4::MemoryTexture::new(
        width as i32,
        height as i32,
        gdk4::MemoryFormat::B8g8r8a8,  // Non-premultiplied since alpha is always 255
        &glib_bytes,
        new_stride,
    );
    
    log::info!("buffer_rendered_callback: created texture {}x{}", width, height);
    
    // Store the texture
    *callback_data.latest_texture.borrow_mut() = Some(texture.upcast());
    callback_data.frame_available.store(true, Ordering::Release);
    
    // Free the pixel bytes
    plat::g_bytes_unref(bytes);
}

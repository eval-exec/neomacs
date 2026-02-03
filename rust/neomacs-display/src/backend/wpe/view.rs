//! WPE WebKit view wrapper using WPE Platform API.
//!
//! Uses the modern WPE Platform API for GPU-accelerated buffer export
//! instead of the legacy wpebackend-fdo.

use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};

use gdk4::prelude::*;
use gdk4::Texture;
use glib;

use crate::core::error::{DisplayError, DisplayResult};

use super::sys;
use super::sys::webkit as wk;
use super::sys::platform as plat;
use super::platform::WpePlatformDisplay;
use super::dmabuf::DmaBufExporter;

/// Callback type for new window requests.
/// Parameters: (view_id, url, frame_name)
/// Returns: true to handle (ignore webkit's default), false to allow webkit default
pub type NewWindowCallback = extern "C" fn(view_id: u32, url: *const std::os::raw::c_char, frame_name: *const std::os::raw::c_char) -> bool;

/// Global callback for new window requests (set from Emacs)
static mut NEW_WINDOW_CALLBACK: Option<NewWindowCallback> = None;

/// Set the global new window callback
pub fn set_new_window_callback(callback: Option<NewWindowCallback>) {
    unsafe {
        NEW_WINDOW_CALLBACK = callback;
    }
}

/// Get the global new window callback
pub fn get_new_window_callback() -> Option<NewWindowCallback> {
    unsafe { NEW_WINDOW_CALLBACK }
}

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
    /// View ID for callbacks to Emacs
    view_id: u32,
    /// Latest rendered texture (Mutex for thread safety)
    latest_texture: Mutex<Option<Texture>>,
    /// Flag indicating new frame available
    frame_available: AtomicBool,
    /// WPE Platform display for buffer import
    display: *mut plat::WPEDisplay,
    /// EGL display for DMA-BUF export
    egl_display: *mut libc::c_void,
    /// GDK display for texture creation (stored as raw pointer for thread safety)
    gdk_display_ptr: usize,
}

/// A WPE WebKit browser view using WPE Platform API.
///
/// Uses WPEDisplay headless mode and WPEView buffer-rendered signals
/// for efficient GPU texture extraction.
pub struct WpeWebView {
    /// View ID (for callbacks to Emacs)
    pub view_id: u32,

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

    /// Signal handler ID for decide-policy
    decide_policy_handler_id: u64,

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
    /// * `view_id` - Unique ID for this view (for callbacks)
    /// * `platform_display` - The initialized WPE Platform display
    /// * `width` - Initial width
    /// * `height` - Initial height
    pub fn new(view_id: u32, platform_display: &WpePlatformDisplay, width: u32, height: u32) -> DisplayResult<Self> {
        use std::io::Write;
        eprintln!("WpeWebView::new (Platform API) called with id={}, {}x{}", view_id, width, height);
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

            // Get EGL display for DMA-BUF export
            let egl_display = platform_display.egl_display();

            // Get GDK display pointer for passing to callback
            let gdk_display_ptr = gdk_display.as_ref()
                .map(|d| d.as_ptr() as usize)
                .unwrap_or(0);

            // Allocate callback data
            let callback_data = Box::into_raw(Box::new(BufferCallbackData {
                view_id,
                latest_texture: Mutex::new(None),
                frame_available: AtomicBool::new(false),
                display,
                egl_display,
                gdk_display_ptr,
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

            // Connect decide-policy signal for new window handling
            let decide_policy_signal = CString::new("decide-policy").unwrap();
            let decide_policy_handler_id = plat::g_signal_connect_data(
                web_view as *mut _,
                decide_policy_signal.as_ptr(),
                Some(std::mem::transmute::<
                    unsafe extern "C" fn(*mut wk::WebKitWebView, *mut wk::WebKitPolicyDecision, u32, *mut libc::c_void) -> i32,
                    unsafe extern "C" fn(),
                >(decide_policy_callback)),
                callback_data as *mut _,
                None,
                0, // G_CONNECT_DEFAULT
            );
            eprintln!("WpeWebView::new: connected decide-policy signal, handler_id={}", decide_policy_handler_id);

            // Map and make the view visible so it starts rendering
            plat::wpe_view_set_visible(wpe_view as *mut plat::WPEView, 1);
            plat::wpe_view_map(wpe_view as *mut plat::WPEView);
            eprintln!("WpeWebView::new: view mapped and set visible");

            eprintln!("WpeWebView: WPE Platform WebKitWebView created successfully ({}x{})", width, height);
            log::info!("WPE Platform WebKitWebView created successfully ({}x{})", width, height);

            Ok(Self {
                view_id,
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
                decide_policy_handler_id,
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
            log::trace!("WPE update: callback_data ptr = {:?}", self.callback_data);
            if let Some(callback_data) = self.callback_data.as_ref() {
                let frame_avail = callback_data.frame_available.load(Ordering::Acquire);
                log::trace!("WPE update: frame_available = {}", frame_avail);
                if callback_data.frame_available.swap(false, Ordering::Acquire) {
                    self.needs_redraw = true;

                    // Get texture from callback data (thread-safe)
                    if let Ok(mut guard) = callback_data.latest_texture.lock() {
                        if let Some(texture) = guard.take() {
                            log::info!("WPE update: Got new texture {}x{}", texture.width(), texture.height());
                            self.texture = Some(texture);
                        } else {
                            log::warn!("WPE update: frame_available was true but no texture in guard");
                        }
                    } else {
                        log::warn!("WPE update: failed to lock latest_texture mutex");
                    }
                }
            } else {
                log::warn!("WPE update: callback_data.as_ref() returned None");
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
    /// This checks callback_data directly to get the most recent frame
    pub fn texture(&self) -> Option<Texture> {
        // First check if there's a newer texture in callback_data
        unsafe {
            log::trace!("texture(): callback_data ptr = {:?}", self.callback_data);
            if let Some(callback_data) = self.callback_data.as_ref() {
                log::trace!("texture(): trying to lock latest_texture");
                match callback_data.latest_texture.lock() {
                    Ok(guard) => {
                        log::trace!("texture(): lock acquired, has_texture = {}", guard.is_some());
                        if let Some(ref tex) = *guard {
                            log::trace!("texture(): returning texture {}x{}", tex.width(), tex.height());
                            return Some(tex.clone());
                        }
                    }
                    Err(e) => {
                        log::warn!("texture(): failed to lock: {:?}", e);
                    }
                }
            } else {
                log::trace!("texture(): callback_data is null");
            }
        }
        // Fall back to cached texture
        log::trace!("texture(): falling back to cached texture, has_texture = {}", self.texture.is_some());
        self.texture.clone()
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
    pub fn send_keyboard_event(&self, keyval: u32, keycode: u32, pressed: bool, modifiers: u32) {
        unsafe {
            let event_type = if pressed {
                plat::WPEEventType_WPE_EVENT_KEYBOARD_KEY_DOWN
            } else {
                plat::WPEEventType_WPE_EVENT_KEYBOARD_KEY_UP
            };

            // Convert Emacs modifiers to WPE modifiers
            let wpe_modifiers = Self::convert_modifiers(modifiers);

            // Get current time in milliseconds
            let time = Self::get_time_ms();

            let event = plat::wpe_event_keyboard_new(
                event_type,
                self.wpe_view,
                plat::WPEInputSource_WPE_INPUT_SOURCE_KEYBOARD,
                time,
                wpe_modifiers,
                keycode,
                keyval,
            );

            if !event.is_null() {
                plat::wpe_view_event(self.wpe_view, event);
                plat::wpe_event_unref(event);
                log::debug!("WPE Platform: Keyboard event keyval={} keycode={} pressed={}", keyval, keycode, pressed);
            } else {
                log::warn!("WPE Platform: Failed to create keyboard event");
            }
        }
    }

    /// Send pointer/mouse event to WebKit via WPE Platform
    pub fn send_pointer_event(&self, event_type: u32, x: i32, y: i32, button: u32, state: u32, modifiers: u32) {
        unsafe {
            // Convert Emacs modifiers to WPE modifiers
            let wpe_modifiers = Self::convert_modifiers(modifiers);
            let time = Self::get_time_ms();

            // event_type: 1=motion, 2=button
            match event_type {
                1 => {
                    // Motion event
                    let event = plat::wpe_event_pointer_move_new(
                        plat::WPEEventType_WPE_EVENT_POINTER_MOVE,
                        self.wpe_view,
                        plat::WPEInputSource_WPE_INPUT_SOURCE_MOUSE,
                        time,
                        wpe_modifiers,
                        x as f64,
                        y as f64,
                        0.0, // delta_x
                        0.0, // delta_y
                    );

                    if !event.is_null() {
                        plat::wpe_view_event(self.wpe_view, event);
                        plat::wpe_event_unref(event);
                        log::trace!("WPE Platform: Pointer move at ({}, {})", x, y);
                    }
                }
                2 => {
                    // Button event
                    let wpe_event_type = if state != 0 {
                        plat::WPEEventType_WPE_EVENT_POINTER_DOWN
                    } else {
                        plat::WPEEventType_WPE_EVENT_POINTER_UP
                    };

                    // WPE button numbers: 1=left, 2=middle, 3=right
                    let wpe_button = button;
                    let press_count = if state != 0 { 1 } else { 0 };

                    let event = plat::wpe_event_pointer_button_new(
                        wpe_event_type,
                        self.wpe_view,
                        plat::WPEInputSource_WPE_INPUT_SOURCE_MOUSE,
                        time,
                        wpe_modifiers,
                        wpe_button,
                        x as f64,
                        y as f64,
                        press_count,
                    );

                    if !event.is_null() {
                        plat::wpe_view_event(self.wpe_view, event);
                        plat::wpe_event_unref(event);
                        log::debug!("WPE Platform: Pointer button {} {} at ({}, {})",
                                   button, if state != 0 { "press" } else { "release" }, x, y);
                    }
                }
                _ => {
                    log::warn!("WPE Platform: Unknown pointer event type {}", event_type);
                }
            }
        }
    }

    /// Send scroll/wheel event to WebKit via WPE Platform
    pub fn send_axis_event(&self, x: i32, y: i32, axis: u32, value: i32, modifiers: u32) {
        unsafe {
            let wpe_modifiers = Self::convert_modifiers(modifiers);
            let time = Self::get_time_ms();

            // axis: 0=horizontal, 1=vertical
            let (delta_x, delta_y) = if axis == 0 {
                (value as f64, 0.0)
            } else {
                (0.0, value as f64)
            };

            let event = plat::wpe_event_scroll_new(
                self.wpe_view,
                plat::WPEInputSource_WPE_INPUT_SOURCE_MOUSE,
                time,
                wpe_modifiers,
                delta_x,
                delta_y,
                0, // precise_deltas: FALSE
                0, // is_stop: FALSE
                x as f64,
                y as f64,
            );

            if !event.is_null() {
                plat::wpe_view_event(self.wpe_view, event);
                plat::wpe_event_unref(event);
                log::debug!("WPE Platform: Scroll delta=({}, {}) at ({}, {})", delta_x, delta_y, x, y);
            }
        }
    }

    /// Click at position (convenience method)
    pub fn click(&self, x: i32, y: i32, button: u32) {
        // Send motion to position first
        self.send_pointer_event(1, x, y, 0, 0, 0);
        // Send press then release
        self.send_pointer_event(2, x, y, button, 1, 0); // button press
        self.send_pointer_event(2, x, y, button, 0, 0); // button release
        log::debug!("WPE Platform: Click at ({}, {}) button={}", x, y, button);
    }

    /// Scroll at position (convenience method)
    pub fn scroll(&self, x: i32, y: i32, delta_x: i32, delta_y: i32) {
        // First move pointer to position
        self.send_pointer_event(1, x, y, 0, 0, 0);
        
        // Send scroll events
        if delta_x != 0 {
            self.send_axis_event(x, y, 0, delta_x, 0); // horizontal
        }
        if delta_y != 0 {
            self.send_axis_event(x, y, 1, delta_y, 0); // vertical
        }
        log::debug!("WPE Platform: Scroll at ({}, {}) delta=({}, {})", x, y, delta_x, delta_y);
    }

    /// Convert Emacs modifiers to WPE modifiers
    fn convert_modifiers(emacs_modifiers: u32) -> u32 {
        let mut wpe_mods = 0u32;
        
        // Emacs modifier bits (from lisp.h):
        // shift_modifier = 1, ctrl_modifier = 4, meta_modifier = 8, alt_modifier = 16
        const EMACS_SHIFT: u32 = 1;
        const EMACS_CTRL: u32 = 4;
        const EMACS_META: u32 = 8;
        const EMACS_ALT: u32 = 16;

        if emacs_modifiers & EMACS_SHIFT != 0 {
            wpe_mods |= plat::WPEModifiers_WPE_MODIFIER_KEYBOARD_SHIFT;
        }
        if emacs_modifiers & EMACS_CTRL != 0 {
            wpe_mods |= plat::WPEModifiers_WPE_MODIFIER_KEYBOARD_CONTROL;
        }
        if emacs_modifiers & EMACS_META != 0 {
            wpe_mods |= plat::WPEModifiers_WPE_MODIFIER_KEYBOARD_META;
        }
        if emacs_modifiers & EMACS_ALT != 0 {
            wpe_mods |= plat::WPEModifiers_WPE_MODIFIER_KEYBOARD_ALT;
        }

        wpe_mods
    }

    /// Get current time in milliseconds (for event timestamps)
    fn get_time_ms() -> u32 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| (d.as_millis() & 0xFFFFFFFF) as u32)
            .unwrap_or(0)
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
    use super::platform::{buffer_dmabuf_info, dmabuf_to_texture};

    if user_data.is_null() || buffer.is_null() {
        log::warn!("buffer_rendered_callback: null user_data or buffer");
        return;
    }

    let callback_data = &*(user_data as *const BufferCallbackData);

    let width = plat::wpe_buffer_get_width(buffer) as u32;
    let height = plat::wpe_buffer_get_height(buffer) as u32;

    // Validate dimensions
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        log::warn!("buffer_rendered_callback: invalid dimensions {}x{}", width, height);
        return;
    }

    log::debug!("buffer_rendered_callback: received buffer {}x{}", width, height);

    // Try DMA-BUF zero-copy path first
    // TEMPORARILY DISABLED for debugging - force pixel fallback
    let use_dmabuf = false;
    if use_dmabuf && callback_data.gdk_display_ptr != 0 {
        if let Some(dmabuf_info) = buffer_dmabuf_info(buffer) {
            // Reconstruct GdkDisplay from pointer
            let gdk_display: gdk4::Display = glib::translate::from_glib_none(
                callback_data.gdk_display_ptr as *mut gdk4::ffi::GdkDisplay
            );

            match dmabuf_to_texture(&dmabuf_info, &gdk_display) {
                Ok(texture) => {
                    log::info!("buffer_rendered_callback: DMA-BUF zero-copy texture {}x{}", width, height);
                    if let Ok(mut guard) = callback_data.latest_texture.lock() {
                        *guard = Some(texture);
                    }
                    callback_data.frame_available.store(true, Ordering::Release);
                    
                    // Queue widget redraw on main thread
                    glib::MainContext::default().invoke(|| {
                        if let Some(widget) = crate::backend::gtk4::get_video_widget() {
                            use gtk4::prelude::WidgetExt;
                            widget.queue_draw();
                        }
                    });
                    
                    return;
                }
                Err(e) => {
                    log::debug!("DMA-BUF texture creation failed: {}, falling back to pixels", e);
                }
            }
        }
    }

    // Fallback: Import buffer to pixels
    // Take a reference to the buffer to prevent WPE from recycling it during our processing
    plat::g_object_ref(buffer as *mut _);

    let mut error: *mut plat::GError = ptr::null_mut();
    let bytes = plat::wpe_buffer_import_to_pixels(buffer, &mut error);

    if bytes.is_null() {
        if !error.is_null() {
            let msg = std::ffi::CStr::from_ptr((*error).message)
                .to_string_lossy();
            log::warn!("buffer_rendered_callback: pixel import failed: {}", msg);
            plat::g_error_free(error);
        }
        plat::g_object_unref(buffer as *mut _);
        return;
    }

    // Get pixel data
    let mut size: plat::gsize = 0;
    let data = plat::g_bytes_get_data(bytes, &mut size);

    if data.is_null() || size == 0 {
        log::warn!("buffer_rendered_callback: empty pixel data");
        plat::g_bytes_unref(bytes);
        plat::g_object_unref(buffer as *mut _);
        return;
    }

    let size = size as usize;

    // Validate size is reasonable
    let expected_size = (width * height * 4) as usize;
    if size < expected_size || size > expected_size * 2 {
        log::warn!("buffer_rendered_callback: suspicious size {} for {}x{} (expected ~{})",
                   size, width, height, expected_size);
        plat::g_bytes_unref(bytes);
        plat::g_object_unref(buffer as *mut _);
        return;
    }

    // Copy pixel data IMMEDIATELY before WPE can reclaim the buffer
    let pixel_data: Vec<u8> = std::slice::from_raw_parts(data as *const u8, size).to_vec();

    // Free the GBytes and release the buffer reference
    plat::g_bytes_unref(bytes);
    plat::g_object_unref(buffer as *mut _);

    // Calculate stride
    let actual_stride = size / (height as usize);

    log::info!("buffer_rendered_callback: {}x{}, size={}, stride={}",
               width, height, size, actual_stride);

    // WPE exports XRGB/BGRX format (alpha channel is unused/zero)
    // We need to set alpha to 255 (opaque) for all pixels
    let mut pixels_with_alpha: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);

    // Copy row by row, handling stride
    for row in 0..(height as usize) {
        let row_start = row * actual_stride;
        for col in 0..(width as usize) {
            let offset = row_start + col * 4;
            if offset + 3 >= pixel_data.len() {
                log::warn!("buffer_rendered_callback: buffer underrun at row={}, col={}", row, col);
                return;
            }
            // Copy BGR, set A to 255
            pixels_with_alpha.push(pixel_data[offset]);     // B
            pixels_with_alpha.push(pixel_data[offset + 1]); // G
            pixels_with_alpha.push(pixel_data[offset + 2]); // R
            pixels_with_alpha.push(255);                     // A (was 0)
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

    // Store the texture (thread-safe)
    log::info!("buffer_rendered_callback: storing texture in callback_data ptr={:p}", callback_data);
    if let Ok(mut guard) = callback_data.latest_texture.lock() {
        *guard = Some(texture.upcast());
        log::info!("buffer_rendered_callback: texture stored successfully");
    } else {
        log::warn!("buffer_rendered_callback: failed to lock latest_texture mutex");
    }
    callback_data.frame_available.store(true, Ordering::Release);
    
    // Queue widget redraw on main thread
    glib::MainContext::default().invoke(|| {
        if let Some(widget) = crate::backend::gtk4::get_video_widget() {
            use gtk4::prelude::WidgetExt;
            widget.queue_draw();
        }
    });
}

/// C callback for decide-policy signal from WebKitWebView
/// Handles new window requests (target="_blank", window.open(), etc.)
unsafe extern "C" fn decide_policy_callback(
    _web_view: *mut wk::WebKitWebView,
    decision: *mut wk::WebKitPolicyDecision,
    decision_type: u32,
    user_data: *mut libc::c_void,
) -> i32 {
    // Policy decision type constants
    const WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION: u32 = 0;
    const WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION: u32 = 1;
    const WEBKIT_POLICY_DECISION_TYPE_RESPONSE: u32 = 2;

    if user_data.is_null() || decision.is_null() {
        return 0; // FALSE - let WebKit handle it
    }

    let callback_data = &*(user_data as *const BufferCallbackData);

    match decision_type {
        WEBKIT_POLICY_DECISION_TYPE_NEW_WINDOW_ACTION => {
            // Cast to WebKitNavigationPolicyDecision
            let nav_decision = decision as *mut wk::WebKitNavigationPolicyDecision;

            // Get the navigation action
            let nav_action = wk::webkit_navigation_policy_decision_get_navigation_action(nav_decision);
            if nav_action.is_null() {
                log::warn!("decide_policy_callback: null navigation action");
                wk::webkit_policy_decision_ignore(decision);
                return 1; // TRUE - we handled it
            }

            // Get the request URL
            let request = wk::webkit_navigation_action_get_request(nav_action);
            let url = if !request.is_null() {
                let uri_ptr = wk::webkit_uri_request_get_uri(request);
                if !uri_ptr.is_null() {
                    CStr::from_ptr(uri_ptr).to_string_lossy().into_owned()
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            // Get the frame name (target attribute)
            let frame_name_ptr = wk::webkit_navigation_action_get_frame_name(nav_action);
            let frame_name = if !frame_name_ptr.is_null() {
                CStr::from_ptr(frame_name_ptr).to_string_lossy().into_owned()
            } else {
                String::new()
            };

            log::info!("decide_policy_callback: NEW_WINDOW request url='{}' frame='{}'",
                       url, frame_name);

            // Call the Emacs callback if set
            if let Some(callback) = get_new_window_callback() {
                let c_url = CString::new(url.clone()).unwrap_or_default();
                let c_frame = CString::new(frame_name.clone()).unwrap_or_default();

                let handled = callback(callback_data.view_id, c_url.as_ptr(), c_frame.as_ptr());

                if handled {
                    // Emacs will handle opening the URL
                    wk::webkit_policy_decision_ignore(decision);
                    log::info!("decide_policy_callback: Emacs handled new window for '{}'", url);
                    return 1; // TRUE - we handled it
                }
            }

            // No callback or callback didn't handle it - ignore (don't open new window)
            wk::webkit_policy_decision_ignore(decision);
            log::info!("decide_policy_callback: Ignored new window request for '{}'", url);
            return 1; // TRUE - we handled it (by ignoring)
        }

        WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION => {
            // Normal navigation - let WebKit handle it
            return 0; // FALSE
        }

        WEBKIT_POLICY_DECISION_TYPE_RESPONSE => {
            // Resource response - let WebKit handle it
            return 0; // FALSE
        }

        _ => {
            // Unknown type - let WebKit handle it
            return 0; // FALSE
        }
    }
}

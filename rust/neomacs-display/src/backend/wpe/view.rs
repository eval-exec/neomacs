//! WPE WebKit view wrapper using WPE Platform API.
//!
//! Uses the modern WPE Platform API for GPU-accelerated buffer export
//! instead of the legacy wpebackend-fdo.

use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};

use crate::core::error::{DisplayError, DisplayResult};

use super::sys::webkit as wk;
use super::sys::platform as plat;
use super::platform::{WpePlatformDisplay, buffer_dmabuf_info};
use super::dmabuf::{DmaBufExporter, ExportedDmaBuf};

/// Callback type for new window requests.
/// Parameters: (view_id, url, frame_name)
/// Returns: true to handle (ignore webkit's default), false to allow webkit default
pub type NewWindowCallback = extern "C" fn(view_id: u32, url: *const std::os::raw::c_char, frame_name: *const std::os::raw::c_char) -> bool;

/// Callback type for page load events.
/// Parameters: (view_id, load_event, uri)
/// load_event: 0=started, 1=redirected, 2=committed, 3=finished, 4=failed
pub type LoadCallback = extern "C" fn(view_id: u32, load_event: std::os::raw::c_int, uri: *const std::os::raw::c_char);

/// Global callback for new window requests (set from Emacs)
static mut NEW_WINDOW_CALLBACK: Option<NewWindowCallback> = None;

/// Global callback for page load events (set from Emacs)
static mut LOAD_CALLBACK: Option<LoadCallback> = None;

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

/// Set the global load callback
pub fn set_load_callback(callback: Option<LoadCallback>) {
    unsafe {
        LOAD_CALLBACK = callback;
    }
}

/// Get the global load callback
pub fn get_load_callback() -> Option<LoadCallback> {
    unsafe { LOAD_CALLBACK }
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

/// Raw pixel data from WebKit buffer (fallback path)
/// Used when DMA-BUF is not available.
struct RawFrameData {
    /// Raw BGRA pixel data
    pixels: Vec<u8>,
    /// Frame width in pixels
    width: u32,
    /// Frame height in pixels
    height: u32,
}

/// Public raw pixel frame data for fallback rendering.
/// Used when DMA-BUF import fails (e.g., incompatible modifier).
pub struct RawPixelData {
    /// Raw BGRA pixel data
    pub pixels: Vec<u8>,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
}

/// Public DMA-BUF frame data for export to wgpu renderer.
/// The file descriptors are duped and owned by this struct.
pub struct DmaBufData {
    /// DMA-BUF file descriptors (one per plane, duped for ownership)
    pub fds: Vec<i32>,
    /// Stride for each plane
    pub strides: Vec<u32>,
    /// Offset for each plane
    pub offsets: Vec<u32>,
    /// DRM fourcc format code
    pub fourcc: u32,
    /// DRM modifier
    pub modifier: u64,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
}

/// Internal DMA-BUF frame data for zero-copy rendering
/// Stores DMA-BUF file descriptors and metadata for GPU-to-GPU texture import.
struct DmaBufFrameData {
    /// DMA-BUF file descriptors (one per plane, duped for ownership)
    fds: Vec<i32>,
    /// Stride for each plane
    strides: Vec<u32>,
    /// Offset for each plane
    offsets: Vec<u32>,
    /// DRM fourcc format code
    fourcc: u32,
    /// DRM modifier
    modifier: u64,
    /// Frame width in pixels
    width: u32,
    /// Frame height in pixels
    height: u32,
}

impl DmaBufFrameData {
    /// Take ownership of fds for passing to GdkDmabufTexture.
    /// After this call, the fds are set to -1 so Drop won't close them.
    /// The caller (GdkDmabufTexture) takes ownership of the fds.
    fn take_fds(&mut self) -> Vec<i32> {
        let fds = std::mem::take(&mut self.fds);
        self.fds = vec![-1; fds.len()]; // Set to -1 so Drop doesn't close them
        fds
    }
}

impl Drop for DmaBufFrameData {
    fn drop(&mut self) {
        // Close the duped file descriptors (if not taken by texture)
        for fd in &self.fds {
            if *fd >= 0 {
                unsafe { libc::close(*fd); }
            }
        }
    }
}

/// Callback data for buffer-rendered signal
struct BufferCallbackData {
    /// View ID for callbacks to Emacs
    view_id: u32,
    /// Latest DMA-BUF frame data for zero-copy rendering (preferred path)
    latest_dmabuf: Mutex<Option<DmaBufFrameData>>,
    /// Latest raw frame data (fallback when DMA-BUF not available)
    latest_frame: Mutex<Option<RawFrameData>>,
    /// Flag indicating new frame available
    frame_available: AtomicBool,
    /// Flag indicating DMA-BUF frame available (prefer over raw frame)
    dmabuf_available: AtomicBool,
    /// WPE Platform display for buffer import
    display: *mut plat::WPEDisplay,
    /// EGL display for DMA-BUF export
    egl_display: *mut libc::c_void,
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

    /// Signal handler ID for load-changed
    load_changed_handler_id: u64,

    /// DMA-BUF exporter for texture conversion
    dmabuf_exporter: DmaBufExporter,

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
        log::info!("WpeWebView::new (Platform API) called with id={}, {}x{}", view_id, width, height);

        let display = platform_display.raw();
        if display.is_null() {
            return Err(DisplayError::WebKit("WPE Platform display is null".into()));
        }

        // Create DMA-BUF exporter with the EGL display
        log::debug!("WpeWebView::new: creating DmaBufExporter...");
        let dmabuf_exporter = DmaBufExporter::new(platform_display.egl_display());
        log::debug!("WpeWebView::new: DmaBufExporter created");

        unsafe {
            // Create WebKitNetworkSession (required for WPE Platform)
            let network_session = wk::webkit_network_session_get_default();
            log::debug!("WpeWebView::new: network_session={:?}", network_session);

            // Create WebKitWebContext
            let web_context = wk::webkit_web_context_new();
            log::debug!("WpeWebView::new: web_context={:?}", web_context);

            // Create WebKitWebView with "display" construct-only property via g_object_new.
            // This ensures the view uses our headless WPE Platform display rather than
            // falling back to wpe_display_get_default() which may differ on multi-GPU systems.
            log::debug!("WpeWebView::new: creating WebKitWebView with WPE Platform display {:?}...", display);

            let display_prop = CString::new("display").unwrap();
            let web_view = plat::g_object_new(
                wk::webkit_web_view_get_type(),
                display_prop.as_ptr(),
                display as *mut libc::c_void,
                ptr::null::<libc::c_char>(),
            ) as *mut wk::WebKitWebView;
            log::debug!("WpeWebView::new: web_view={:?}", web_view);

            if web_view.is_null() {
                return Err(DisplayError::WebKit("Failed to create WebKitWebView".into()));
            }

            // Get the WPEView from WebKitWebView
            let wpe_view = wk::webkit_web_view_get_wpe_view(web_view);
            log::debug!("WpeWebView::new: wpe_view={:?}", wpe_view);

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

            // Allocate callback data
            // Store raw pixel data in callback, create textures on main thread
            let callback_data = Box::into_raw(Box::new(BufferCallbackData {
                view_id,
                latest_dmabuf: Mutex::new(None),
                latest_frame: Mutex::new(None),
                frame_available: AtomicBool::new(false),
                dmabuf_available: AtomicBool::new(false),
                display,
                egl_display,
            }));
            log::debug!("WpeWebView::new: callback_data={:?}", callback_data);

            // Connect buffer-rendered signal on WPEView
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
            log::debug!("WpeWebView::new: connected buffer-rendered signal, handler_id={}", handler_id);

            // Also add frame-displayed callback on WebKitWebView as a backup notification mechanism
            let frame_callback_id = wk::webkit_web_view_add_frame_displayed_callback(
                web_view,
                Some(frame_displayed_callback),
                callback_data as *mut _,
                None,
            );
            log::debug!("WpeWebView::new: added frame-displayed callback, id={}", frame_callback_id);

            // Connect buffer-released signal to debug buffer lifecycle
            let buffer_released_signal = CString::new("buffer-released").unwrap();
            let buffer_released_handler_id = plat::g_signal_connect_data(
                wpe_view as *mut _,
                buffer_released_signal.as_ptr(),
                Some(std::mem::transmute::<
                    unsafe extern "C" fn(*mut plat::WPEView, *mut plat::WPEBuffer, *mut libc::c_void),
                    unsafe extern "C" fn(),
                >(buffer_released_callback)),
                callback_data as *mut _,
                None,
                0,
            );
            log::debug!("WpeWebView::new: connected buffer-released signal, handler_id={}", buffer_released_handler_id);

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
            log::debug!("WpeWebView::new: connected decide-policy signal, handler_id={}", decide_policy_handler_id);

            // Connect load-changed signal for page load events
            let load_changed_signal = CString::new("load-changed").unwrap();
            let load_changed_handler_id = plat::g_signal_connect_data(
                web_view as *mut _,
                load_changed_signal.as_ptr(),
                Some(std::mem::transmute::<
                    unsafe extern "C" fn(*mut wk::WebKitWebView, u32, *mut libc::c_void),
                    unsafe extern "C" fn(),
                >(load_changed_callback)),
                callback_data as *mut _,
                None,
                0, // G_CONNECT_DEFAULT
            );
            log::debug!("WpeWebView::new: connected load-changed signal, handler_id={}", load_changed_handler_id);

            // Create a headless toplevel and attach it to the view
            // This is required for WPEViewHeadless to start rendering and emit buffer-rendered signals
            // IMPORTANT: We must get the display from the view itself to match what WebKit is using
            let view_display = plat::wpe_view_get_display(wpe_view as *mut plat::WPEView);
            if view_display.is_null() {
                log::warn!("WpeWebView::new: view has no display");
            } else {
                log::debug!("WpeWebView::new: view display = {:?}", view_display);
                let toplevel = plat::wpe_toplevel_headless_new(view_display as *mut _);
                if !toplevel.is_null() {
                    log::debug!("WpeWebView::new: created headless toplevel {:?}", toplevel);
                    plat::wpe_view_set_toplevel(wpe_view as *mut plat::WPEView, toplevel);
                    log::debug!("WpeWebView::new: set toplevel on view");
                    // Resize the toplevel to the view size
                    plat::wpe_toplevel_resize(toplevel, width as i32, height as i32);
                } else {
                    log::warn!("WpeWebView::new: failed to create headless toplevel");
                }
            }

            // Map and make the view visible so it starts rendering
            plat::wpe_view_set_visible(wpe_view as *mut plat::WPEView, 1);
            plat::wpe_view_map(wpe_view as *mut plat::WPEView);
            log::debug!("WpeWebView::new: view mapped and set visible");

            log::info!("WPE Platform WebKitWebView created successfully ({}x{})", width, height);

            Ok(Self {
                view_id,
                url: String::new(),
                state: WpeViewState::Ready,
                width,
                height,
                title: None,
                progress: 0.0,
                web_view,
                wpe_view: wpe_view as *mut _,
                callback_data,
                buffer_rendered_handler_id: handler_id,
                decide_policy_handler_id,
                load_changed_handler_id,
                dmabuf_exporter,
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

        log::debug!("WpeWebView::load_uri: calling webkit_web_view_load_uri({:?}, {:?})", self.web_view, uri);
        unsafe {
            wk::webkit_web_view_load_uri(self.web_view, c_uri.as_ptr());
        }
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
        log::trace!("WpeWebView::update() called for view {}", self.view_id);
        unsafe {
            // Pump GLib main context to process WebKit events (including buffer-rendered signal)
            // This is critical - without this, WebKit's internal events won't be dispatched
            //
            // IMPORTANT: WPEViewHeadless attaches its frame source to g_main_context_get_thread_default(),
            // so we need to iterate that context. If no thread-default is set, it falls back to default.
            let thread_ctx = plat::g_main_context_get_thread_default();
            let ctx = if thread_ctx.is_null() {
                plat::g_main_context_default()
            } else {
                thread_ctx
            };
            while plat::g_main_context_iteration(ctx, 0) != 0 {
                // Process all pending events
            }
            // Also iterate the default context in case signals are attached there
            let default_ctx = plat::g_main_context_default();
            if default_ctx != ctx {
                while plat::g_main_context_iteration(default_ctx, 0) != 0 {
                    // Process default context events
                }
            }

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
                if frame_avail {
                    // New frame available - texture will be created lazily in texture() method
                    // on the main thread to avoid GdkTexture threading issues
                    self.needs_redraw = true;
                    log::info!("WPE update: new frame available, triggering redraw");
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

    /// Get current frame as ExportedDmaBuf for wgpu rendering.
    ///
    /// Returns the latest rendered frame as a DMA-BUF that can be imported
    /// into wgpu for zero-copy GPU-to-GPU texture sharing.
    pub fn get_frame_dmabuf(&self) -> Option<ExportedDmaBuf> {
        // Get EGLImage from WPE
        // Export to DmaBuf using DmaBufExporter
        // This will be fully implemented when the integration is complete
        None // Stub for now
    }

    /// Check if view needs redraw
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Clear redraw flag
    pub fn clear_redraw_flag(&mut self) {
        self.needs_redraw = false;
    }

    /// Take the latest DMA-BUF frame data for rendering.
    /// Returns the frame data and clears the stored frame.
    /// The caller takes ownership of the file descriptors.
    pub fn take_latest_dmabuf(&self) -> Option<DmaBufData> {
        unsafe {
            if let Some(callback_data) = self.callback_data.as_ref() {
                // Check if DMA-BUF frame is available
                if callback_data.dmabuf_available.swap(false, Ordering::AcqRel) {
                    if let Ok(mut guard) = callback_data.latest_dmabuf.lock() {
                        if let Some(mut frame) = guard.take() {
                            // Take ownership of the fds
                            let fds = frame.take_fds();
                            // Note: Don't clear frame_available here - pixel fallback may still need it

                            return Some(DmaBufData {
                                fds,
                                strides: frame.strides.clone(),
                                offsets: frame.offsets.clone(),
                                fourcc: frame.fourcc,
                                modifier: frame.modifier,
                                width: frame.width,
                                height: frame.height,
                            });
                        }
                    }
                }
            }
        }
        None
    }

    /// Take the latest raw pixel frame data as fallback.
    /// This is used when DMA-BUF import fails (e.g., incompatible modifier).
    pub fn take_latest_pixels(&self) -> Option<RawPixelData> {
        unsafe {
            if let Some(callback_data) = self.callback_data.as_ref() {
                // Check if raw frame is available
                if callback_data.frame_available.swap(false, Ordering::AcqRel) {
                    if let Ok(mut guard) = callback_data.latest_frame.lock() {
                        if let Some(frame) = guard.take() {
                            return Some(RawPixelData {
                                pixels: frame.pixels,
                                width: frame.width,
                                height: frame.height,
                            });
                        }
                    }
                }
            }
        }
        None
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
    _wpe_view: *mut plat::WPEView,
    buffer: *mut plat::WPEBuffer,
    user_data: *mut libc::c_void,
) {
    // Debug: verify callback is being triggered
    log::info!("buffer_rendered_callback CALLED! buffer={:?}", buffer);

    // Step 1: Basic validation
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

    // Try zero-copy DMA-BUF path first
    if let Some(dmabuf_info) = buffer_dmabuf_info(buffer) {
        log::info!("buffer_rendered_callback: using zero-copy DMA-BUF path {}x{}", width, height);

        // Dup the file descriptors so we own them after callback returns
        let mut fds = Vec::with_capacity(dmabuf_info.planes.len());
        let mut strides = Vec::with_capacity(dmabuf_info.planes.len());
        let mut offsets = Vec::with_capacity(dmabuf_info.planes.len());

        for plane in &dmabuf_info.planes {
            // dup() the fd so it survives after callback returns
            let duped_fd = libc::dup(plane.fd);
            if duped_fd < 0 {
                log::warn!("buffer_rendered_callback: failed to dup fd {}", plane.fd);
                // Close any already duped fds
                for fd in &fds {
                    libc::close(*fd);
                }
                // Fall through to pixel path
                break;
            }
            fds.push(duped_fd);
            strides.push(plane.stride);
            offsets.push(plane.offset);
        }

        if fds.len() == dmabuf_info.planes.len() {
            // Successfully duped all fds, store DMA-BUF frame data
            if let Ok(mut guard) = callback_data.latest_dmabuf.lock() {
                *guard = Some(DmaBufFrameData {
                    fds,
                    strides,
                    offsets,
                    fourcc: dmabuf_info.fourcc,
                    modifier: dmabuf_info.modifier,
                    width: dmabuf_info.width,
                    height: dmabuf_info.height,
                });
                log::info!("buffer_rendered_callback: DMA-BUF frame stored (zero-copy)");
            }
            callback_data.dmabuf_available.store(true, Ordering::Release);
            // Don't return early - also capture pixels as fallback for incompatible modifiers
        }
    }

    // Always capture pixels as fallback (for when DMA-BUF import fails on renderer)
    log::info!("buffer_rendered_callback: attempting pixel capture as fallback");

    let mut error: *mut plat::GError = ptr::null_mut();
    let bytes = plat::wpe_buffer_import_to_pixels(buffer, &mut error);

    if bytes.is_null() {
        if !error.is_null() {
            let msg = std::ffi::CStr::from_ptr((*error).message)
                .to_string_lossy();
            log::warn!("buffer_rendered_callback: pixel import failed: {}", msg);
            plat::g_error_free(error);
        } else {
            log::warn!("buffer_rendered_callback: pixel import failed (no error message)");
        }
        return;
    }
    log::info!("buffer_rendered_callback: pixel import succeeded, extracting data");

    // Get pixel data from GBytes
    let mut size: plat::gsize = 0;
    let data = plat::g_bytes_get_data(bytes, &mut size);

    if data.is_null() || size == 0 {
        log::warn!("buffer_rendered_callback: empty pixel data");
        return;
    }

    let size = size as usize;
    let expected_size = (width * height * 4) as usize;
    if size < expected_size || size > expected_size * 2 {
        log::warn!("buffer_rendered_callback: suspicious size {} for {}x{}", size, width, height);
        return;
    }

    // Copy pixel data before callback returns
    let pixel_data: Vec<u8> = std::slice::from_raw_parts(data as *const u8, size).to_vec();
    let actual_stride = size / (height as usize);

    // Convert XRGB to BGRA with alpha=255
    let mut pixels_with_alpha: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..(height as usize) {
        let row_start = row * actual_stride;
        for col in 0..(width as usize) {
            let offset = row_start + col * 4;
            if offset + 3 >= pixel_data.len() {
                return;
            }
            pixels_with_alpha.push(pixel_data[offset]);     // B
            pixels_with_alpha.push(pixel_data[offset + 1]); // G
            pixels_with_alpha.push(pixel_data[offset + 2]); // R
            pixels_with_alpha.push(255);                     // A
        }
    }

    // Store raw pixel data
    if let Ok(mut guard) = callback_data.latest_frame.lock() {
        *guard = Some(RawFrameData {
            pixels: pixels_with_alpha,
            width,
            height,
        });
    }
    callback_data.frame_available.store(true, Ordering::Release);

    // Note: Widget redraw notification will be handled by wgpu integration
}

/// C callback for buffer-released signal from WPEView
/// In headless mode, buffer-rendered doesn't fire, but buffer-released does.
/// We capture the buffer data here before it's released.
/// Prefers zero-copy DMA-BUF path, falls back to pixel import.
unsafe extern "C" fn buffer_released_callback(
    _wpe_view: *mut plat::WPEView,
    buffer: *mut plat::WPEBuffer,
    user_data: *mut libc::c_void,
) {
    if user_data.is_null() || buffer.is_null() {
        return;
    }

    let callback_data = &*(user_data as *const BufferCallbackData);

    let width = plat::wpe_buffer_get_width(buffer) as u32;
    let height = plat::wpe_buffer_get_height(buffer) as u32;

    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return;
    }

    log::info!("buffer_released_callback: capturing {}x{} buffer", width, height);

    // Try zero-copy DMA-BUF path first
    if let Some(dmabuf_info) = buffer_dmabuf_info(buffer) {
        log::info!("buffer_released_callback: using zero-copy DMA-BUF path {}x{}", width, height);

        // Dup the file descriptors so we own them after callback returns
        let mut fds = Vec::with_capacity(dmabuf_info.planes.len());
        let mut strides = Vec::with_capacity(dmabuf_info.planes.len());
        let mut offsets = Vec::with_capacity(dmabuf_info.planes.len());

        for plane in &dmabuf_info.planes {
            let duped_fd = libc::dup(plane.fd);
            if duped_fd < 0 {
                log::warn!("buffer_released_callback: failed to dup fd {}", plane.fd);
                for fd in &fds {
                    libc::close(*fd);
                }
                break;
            }
            fds.push(duped_fd);
            strides.push(plane.stride);
            offsets.push(plane.offset);
        }

        if fds.len() == dmabuf_info.planes.len() {
            if let Ok(mut guard) = callback_data.latest_dmabuf.lock() {
                *guard = Some(DmaBufFrameData {
                    fds,
                    strides,
                    offsets,
                    fourcc: dmabuf_info.fourcc,
                    modifier: dmabuf_info.modifier,
                    width: dmabuf_info.width,
                    height: dmabuf_info.height,
                });
                log::info!("buffer_released_callback: DMA-BUF frame stored (zero-copy)");
            }
            callback_data.dmabuf_available.store(true, Ordering::Release);
            callback_data.frame_available.store(true, Ordering::Release);

            // Note: Widget redraw notification will be handled by wgpu integration
            return;
        }
    }

    // Fallback: Import pixels from the buffer
    log::debug!("buffer_released_callback: falling back to pixel import path");

    let mut error: *mut plat::GError = ptr::null_mut();
    let bytes = plat::wpe_buffer_import_to_pixels(buffer, &mut error);

    if bytes.is_null() {
        if !error.is_null() {
            let msg = std::ffi::CStr::from_ptr((*error).message).to_string_lossy();
            log::warn!("buffer_released_callback: import failed: {}", msg);
            plat::g_error_free(error);
        }
        return;
    }

    let mut size: plat::gsize = 0;
    let data = plat::g_bytes_get_data(bytes, &mut size);

    if data.is_null() || size == 0 {
        return;
    }

    let size = size as usize;
    let expected_size = (width * height * 4) as usize;

    if size < expected_size {
        log::warn!("buffer_released_callback: size {} < expected {}", size, expected_size);
        return;
    }

    log::debug!("buffer_released_callback: got {} bytes for {}x{}", size, width, height);

    let pixel_data: Vec<u8> = std::slice::from_raw_parts(data as *const u8, size).to_vec();
    let actual_stride = size / (height as usize);

    // Convert XRGB to BGRA with alpha=255
    let mut pixels_with_alpha: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..(height as usize) {
        let row_start = row * actual_stride;
        for col in 0..(width as usize) {
            let offset = row_start + col * 4;
            if offset + 3 >= pixel_data.len() {
                return;
            }
            pixels_with_alpha.push(pixel_data[offset]);     // B
            pixels_with_alpha.push(pixel_data[offset + 1]); // G
            pixels_with_alpha.push(pixel_data[offset + 2]); // R
            pixels_with_alpha.push(255);                     // A
        }
    }

    log::info!("buffer_released_callback: storing frame {}x{}", width, height);

    if let Ok(mut guard) = callback_data.latest_frame.lock() {
        *guard = Some(RawFrameData {
            pixels: pixels_with_alpha,
            width,
            height,
        });
    }
    callback_data.frame_available.store(true, Ordering::Release);

    // Note: Widget redraw notification will be handled by wgpu integration
}

/// C callback for frame-displayed notification from WebKitWebView
/// This is called when a frame has been displayed by the backend.
/// We use this to capture the frame since buffer-rendered doesn't fire in headless mode.
unsafe extern "C" fn frame_displayed_callback(
    web_view: *mut wk::WebKitWebView,
    user_data: *mut libc::c_void,
) {
    if user_data.is_null() {
        return;
    }

    // Get the WPEView from WebKitWebView
    let wpe_view = wk::webkit_web_view_get_wpe_view(web_view);
    if wpe_view.is_null() {
        log::warn!("frame_displayed_callback: wpe_view is null");
        return;
    }

    let callback_data = &*(user_data as *const BufferCallbackData);

    // Get view dimensions (cast WPEView type from webkit module to platform module)
    let wpe_view_plat = wpe_view as *mut plat::WPEView;
    let width = plat::wpe_view_get_width(wpe_view_plat) as u32;
    let height = plat::wpe_view_get_height(wpe_view_plat) as u32;

    log::debug!("frame_displayed_callback: view={}x{}", width, height);

    // Mark that a frame is available
    callback_data.frame_available.store(true, Ordering::Release);

    // Note: Widget redraw notification will be handled by wgpu integration
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

/// Callback for WebKit load-changed signal
/// load_event: WEBKIT_LOAD_STARTED=0, WEBKIT_LOAD_REDIRECTED=1, WEBKIT_LOAD_COMMITTED=2, WEBKIT_LOAD_FINISHED=3
unsafe extern "C" fn load_changed_callback(
    web_view: *mut wk::WebKitWebView,
    load_event: u32,
    user_data: *mut libc::c_void,
) {
    // WebKit load event constants
    const WEBKIT_LOAD_STARTED: u32 = 0;
    const WEBKIT_LOAD_REDIRECTED: u32 = 1;
    const WEBKIT_LOAD_COMMITTED: u32 = 2;
    const WEBKIT_LOAD_FINISHED: u32 = 3;

    // Debug: verify callback is being triggered
    log::info!("load_changed_callback CALLED! event={}", load_event);

    if user_data.is_null() {
        return;
    }

    let callback_data = &*(user_data as *const BufferCallbackData);

    // Map WebKit load events to our callback events:
    // 0=started, 1=redirected, 2=committed, 3=finished, 4=failed
    let event_id = match load_event {
        WEBKIT_LOAD_STARTED => 0,
        WEBKIT_LOAD_REDIRECTED => 1,
        WEBKIT_LOAD_COMMITTED => 2,
        WEBKIT_LOAD_FINISHED => 3,
        _ => return, // Unknown event
    };

    // Get the current URI
    let uri = if !web_view.is_null() {
        let uri_ptr = wk::webkit_web_view_get_uri(web_view);
        if !uri_ptr.is_null() {
            CStr::from_ptr(uri_ptr).to_string_lossy().into_owned()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    log::debug!("load_changed_callback: view={} event={} uri='{}'",
               callback_data.view_id, event_id, uri);

    // Call the Emacs callback if set
    if let Some(callback) = get_load_callback() {
        let c_uri = CString::new(uri).unwrap_or_default();
        callback(callback_data.view_id, event_id, c_uri.as_ptr());
    }
}

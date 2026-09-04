//! WPE WebKit view wrapper using WPEPlatform.
//!
//! Uses the modern WPE Platform API for GPU-accelerated buffer export
//! instead of the legacy wpebackend-fdo.

use std::cell::RefCell;
use std::ffi::{CStr, CString};
use std::os::fd::{FromRawFd, OwnedFd};
use std::ptr;
use std::ptr::NonNull;
use std::rc::Rc;

use super::error::{DisplayError, DisplayResult};

use super::display::{WpePlatformDisplay, buffer_dmabuf_info};
use super::native;
use super::sys::platform as plat;
use super::sys::webkit as wk;
use crate::{
    ButtonState, FocusIntent, PointerButton, ScriptError, ScriptRequest, ScriptRequestId,
    ScriptWorld, WebContentPoint, WebContentSize, WebProcessFailure, WebValue, WebViewEvent,
    WebViewFrameTransport, WebViewGeneration, WebViewId, WebViewInput, WebViewModifiers,
    WebViewPolicy, WebViewScrollDelta, WebViewWake,
};

/// Concrete Linux frame transport. Unlike the public preference, this enum
/// has no `Auto` state, so capture code must select exactly one representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WpeFrameTransport {
    SoftwarePixels,
    DmaBuf,
}

impl WpeFrameTransport {
    pub(super) fn resolve(preference: WebViewFrameTransport) -> Self {
        match preference {
            // Software pixels remain the reliable automatic choice until wgpu
            // can import a HAL texture with its existing Vulkan image layout.
            WebViewFrameTransport::Auto | WebViewFrameTransport::SoftwarePixels => {
                Self::SoftwarePixels
            }
            WebViewFrameTransport::DmaBuf => Self::DmaBuf,
        }
    }
}

pub(crate) struct WpeViewCreation<'a> {
    pub(crate) id: WebViewId,
    pub(crate) generation: WebViewGeneration,
    pub(crate) platform_display: &'a WpePlatformDisplay,
    pub(crate) network_session: NonNull<wk::WebKitNetworkSession>,
    pub(crate) related_view: Option<NonNull<wk::WebKitWebView>>,
    pub(crate) size: WebContentSize,
    pub(crate) policy: &'a WebViewPolicy,
    pub(crate) frame_transport: WpeFrameTransport,
    pub(crate) wake: WebViewWake,
}

/// State of a WPE WebKit view
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WpeViewState {
    /// View is loading content
    Loading,
    /// View is ready/idle
    Ready,
}

/// Raw pixel data from WebKit buffer (fallback path)
/// Used when DMA-BUF is not available.
pub(super) struct RawFrameData {
    /// Raw BGRA pixel data
    pub(super) pixels: Vec<u8>,
    /// Frame width in pixels
    pub(super) width: u32,
    /// Frame height in pixels
    pub(super) height: u32,
    /// Defers native acknowledgement until after this owned pixel frame has
    /// left the WPE callback.
    _lease: NativeWpeBufferLease,
}

pub(super) struct DmaBufPlaneData {
    pub(super) fd: OwnedFd,
    pub(super) stride: u32,
    pub(super) offset: u32,
}

/// One browser-owned DMA-BUF frame. Plane descriptors, producer fence, and
/// native buffer lifetime move together and therefore cannot get out of sync.
pub(super) struct DmaBufData {
    pub(super) planes: Vec<DmaBufPlaneData>,
    pub(super) rendering_fence: Option<OwnedFd>,
    /// DRM fourcc format code
    pub(super) fourcc: u32,
    /// DRM modifier
    pub(super) modifier: u64,
    /// Frame width in pixels
    pub(super) width: u32,
    /// Frame height in pixels
    pub(super) height: u32,
    pub(super) lease: NativeWpeBufferLease,
}

pub(super) enum CapturedFrame {
    Pixels(RawFrameData),
    DmaBuf(DmaBufData),
}

const MAX_WPE_BUFFER_DIMENSION: u32 = 8_192;
const SOFTWARE_PIXEL_BYTES_PER_PIXEL: usize = 4;
const MAX_SOFTWARE_FRAME_BYTES: usize = MAX_WPE_BUFFER_DIMENSION as usize
    * MAX_WPE_BUFFER_DIMENSION as usize
    * SOFTWARE_PIXEL_BYTES_PER_PIXEL;

/// Validated memory layout for a software frame borrowed from WPE.
///
/// WPE's DMA-BUF fallback may pad each row, so imported bytes are not assumed
/// to be tightly packed. The constructor validates every value derived from
/// foreign metadata before Rust creates a slice or allocates a destination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SoftwarePixelLayout {
    width: usize,
    height: usize,
    stride: usize,
    packed_len: usize,
}

impl SoftwarePixelLayout {
    fn new(width: u32, height: u32, byte_len: usize) -> Result<Self, SoftwarePixelLayoutError> {
        let width = width as usize;
        let height = height as usize;

        if byte_len > MAX_SOFTWARE_FRAME_BYTES {
            return Err(SoftwarePixelLayoutError::ExceedsFrameLimit {
                actual: byte_len,
                maximum: MAX_SOFTWARE_FRAME_BYTES,
            });
        }
        if height == 0 {
            return Err(SoftwarePixelLayoutError::PartialRow { byte_len, height });
        }
        if !byte_len.is_multiple_of(height) {
            return Err(SoftwarePixelLayoutError::PartialRow { byte_len, height });
        }

        let stride = byte_len / height;
        let minimum_stride = width
            .checked_mul(SOFTWARE_PIXEL_BYTES_PER_PIXEL)
            .ok_or(SoftwarePixelLayoutError::DimensionOverflow)?;
        if stride < minimum_stride {
            return Err(SoftwarePixelLayoutError::StrideTooShort {
                actual: stride,
                minimum: minimum_stride,
            });
        }

        let packed_len = minimum_stride
            .checked_mul(height)
            .ok_or(SoftwarePixelLayoutError::DimensionOverflow)?;
        Ok(Self {
            width,
            height,
            stride,
            packed_len,
        })
    }

    const fn stride(self) -> usize {
        self.stride
    }

    const fn packed_len(self) -> usize {
        self.packed_len
    }

    fn copy_bgra_opaque(self, source: &[u8]) -> Vec<u8> {
        debug_assert_eq!(source.len(), self.stride * self.height);

        let mut packed = Vec::with_capacity(self.packed_len());
        for row in source.chunks_exact(self.stride).take(self.height) {
            for pixel in row[..self.width * SOFTWARE_PIXEL_BYTES_PER_PIXEL].chunks_exact(4) {
                packed.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        packed
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
enum SoftwarePixelLayoutError {
    #[error("software pixel dimensions overflow addressable memory")]
    DimensionOverflow,
    #[error("software pixel import has {actual} bytes; supported frames use at most {maximum}")]
    ExceedsFrameLimit { actual: usize, maximum: usize },
    #[error("software pixel import length {byte_len} does not contain {height} complete rows")]
    PartialRow { byte_len: usize, height: usize },
    #[error("software pixel row stride {actual} is shorter than {minimum}")]
    StrideTooShort { actual: usize, minimum: usize },
}

/// A WPE buffer borrowed from the native render callback.
///
/// This is a pointer token rather than `&WPEBuffer`: importing pixels may
/// mutate WPE's internal cache, so representing the foreign object as a Rust
/// shared reference would promise an aliasing guarantee that the C API does
/// not make.
struct BorrowedWpeBuffer(NonNull<plat::WPEBuffer>);

impl BorrowedWpeBuffer {
    unsafe fn from_render_callback(buffer: *mut plat::WPEBuffer) -> Self {
        Self(NonNull::new_unchecked(buffer))
    }

    unsafe fn import_pixels(
        &self,
        width: u32,
        height: u32,
    ) -> Result<BorrowedWpePixels<'_>, SoftwarePixelImportError> {
        let mut error: *mut plat::GError = ptr::null_mut();
        let bytes = plat::wpe_buffer_import_to_pixels(self.0.as_ptr(), &mut error);

        if bytes.is_null() {
            let message = if error.is_null() {
                "pixel import failed without an error".to_owned()
            } else {
                let message = CStr::from_ptr((*error).message)
                    .to_string_lossy()
                    .into_owned();
                plat::g_error_free(error);
                message
            };
            return Err(SoftwarePixelImportError::Native(message));
        }

        let mut byte_len: plat::gsize = 0;
        let data = plat::g_bytes_get_data(bytes, &mut byte_len);
        if data.is_null() || byte_len == 0 {
            return Err(SoftwarePixelImportError::Empty);
        }

        let byte_len = byte_len as usize;
        let layout = SoftwarePixelLayout::new(width, height, byte_len)?;
        let bytes = std::slice::from_raw_parts(data.cast(), byte_len);
        Ok(BorrowedWpePixels { bytes, layout })
    }
}

/// Pixels borrowed from a WPE buffer for the duration of the render callback.
///
/// `wpe_buffer_import_to_pixels` returns `(transfer none)`: the `GBytes` and
/// its storage remain owned by `WPEBuffer`. Binding the resulting Rust slice
/// to [`BorrowedWpeBuffer`] prevents it from escaping that borrow.
struct BorrowedWpePixels<'buffer> {
    bytes: &'buffer [u8],
    layout: SoftwarePixelLayout,
}

impl<'buffer> BorrowedWpePixels<'buffer> {
    fn into_bgra_opaque(self) -> Vec<u8> {
        self.layout.copy_bgra_opaque(self.bytes)
    }
}

#[derive(Debug, thiserror::Error)]
enum SoftwarePixelImportError {
    #[error("{0}")]
    Native(String),
    #[error("software pixel import returned no data")]
    Empty,
    #[error(transparent)]
    Layout(#[from] SoftwarePixelLayoutError),
}

/// Reactor-local acknowledgement for one buffer accepted by our custom
/// `WPEView::render_buffer` implementation.
///
/// WPE requires both notifications after `render_buffer` returns true. Keeping
/// the view and buffer references in one non-cloneable value makes omission,
/// duplication, and cross-thread native-pointer transfer unrepresentable in
/// the safe Rust layer.
pub(super) struct NativeWpeBufferLease {
    view: NonNull<plat::WPEView>,
    buffer: NonNull<plat::WPEBuffer>,
}

impl NativeWpeBufferLease {
    unsafe fn retain(view: *mut plat::WPEView, buffer: *mut plat::WPEBuffer) -> Self {
        plat::g_object_ref(view.cast());
        plat::g_object_ref(buffer.cast());
        Self {
            view: NonNull::new_unchecked(view),
            buffer: NonNull::new_unchecked(buffer),
        }
    }
}

impl Drop for NativeWpeBufferLease {
    fn drop(&mut self) {
        unsafe {
            // These are the two acknowledgements required after our custom
            // render_buffer vfunc accepted this exact buffer.
            plat::wpe_view_buffer_rendered(self.view.as_ptr(), self.buffer.as_ptr());
            plat::wpe_view_buffer_released(self.view.as_ptr(), self.buffer.as_ptr());
            plat::g_object_unref(self.buffer.as_ptr().cast());
            plat::g_object_unref(self.view.as_ptr().cast());
        }
    }
}

/// Callback data for the custom WPE render vfunc.
struct BufferCallbackData {
    /// View ID for callbacks to Emacs
    view_id: u32,
    /// Bounded latest-frame slot. The resolved transport ensures one native
    /// buffer creates one representation, never parallel DMA-BUF and pixels.
    latest_frame: RefCell<Option<CapturedFrame>>,
    frame_transport: WpeFrameTransport,
    generation: WebViewGeneration,
    events: Rc<RefCell<Vec<WebViewEvent>>>,
    wake: WebViewWake,
}

/// A WPE WebKit browser view using WPE Platform API.
///
/// Uses WPEDisplay headless mode and WPEView buffer-rendered signals
/// for efficient GPU texture extraction.
pub struct WpeWebView {
    /// View ID (for callbacks to Emacs)
    pub view_id: WebViewId,

    generation: WebViewGeneration,

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

    events: Rc<RefCell<Vec<WebViewEvent>>>,
    wake: WebViewWake,
}

impl WpeWebView {
    pub(super) const fn generation(&self) -> WebViewGeneration {
        self.generation
    }

    /// Create a new WPE WebKit view using WPE Platform API.
    ///
    pub fn new(creation: WpeViewCreation<'_>) -> DisplayResult<Self> {
        let view_id = creation.id;
        let width = creation.size.width();
        let height = creation.size.height();
        tracing::info!(
            "WpeWebView::new (Platform API) called with id={}, {}x{}",
            view_id,
            width,
            height
        );

        let display = creation.platform_display.raw();
        if display.is_null() {
            return Err(DisplayError::WebKit("WPE Platform display is null".into()));
        }

        unsafe {
            // Create WebKitWebView with "display" construct-only property via g_object_new.
            // This ensures the view uses our headless WPE Platform display rather than
            // falling back to wpe_display_get_default() which may differ on multi-GPU systems.
            let display_ptr = display;
            tracing::debug!(
                "WpeWebView::new: creating WebKitWebView with WPE Platform display {:?}...",
                display_ptr
            );

            let display_prop = CString::new("display").unwrap();
            let web_view = if let Some(related) = creation.related_view {
                let related_prop = CString::new("related-view").unwrap();
                plat::g_object_new(
                    wk::webkit_web_view_get_type(),
                    display_prop.as_ptr(),
                    display as *mut libc::c_void,
                    related_prop.as_ptr(),
                    related.as_ptr(),
                    ptr::null::<libc::c_char>(),
                )
            } else {
                let session_prop = CString::new("network-session").unwrap();
                plat::g_object_new(
                    wk::webkit_web_view_get_type(),
                    display_prop.as_ptr(),
                    display as *mut libc::c_void,
                    session_prop.as_ptr(),
                    creation.network_session.as_ptr(),
                    ptr::null::<libc::c_char>(),
                )
            } as *mut wk::WebKitWebView;
            tracing::debug!("WpeWebView::new: web_view={:?}", web_view);

            if web_view.is_null() {
                return Err(DisplayError::WebKit(
                    "Failed to create WebKitWebView".into(),
                ));
            }

            let settings = wk::webkit_web_view_get_settings(web_view);
            if settings.is_null() {
                plat::g_object_unref(web_view.cast());
                return Err(DisplayError::WebKit(
                    "Failed to acquire WebKitSettings".into(),
                ));
            }
            wk::webkit_settings_set_enable_javascript(
                settings,
                i32::from(creation.policy.javascript()),
            );
            wk::webkit_settings_set_enable_developer_extras(
                settings,
                i32::from(creation.policy.developer_tools()),
            );

            // Get the WPEView from WebKitWebView
            let wpe_view = wk::webkit_web_view_get_wpe_view(web_view);
            tracing::debug!("WpeWebView::new: wpe_view={:?}", wpe_view);

            if wpe_view.is_null() {
                // Clean up
                plat::g_object_unref(web_view as *mut _);
                return Err(DisplayError::WebKit(
                    "Failed to get WPEView from WebKitWebView - display may not be connected"
                        .into(),
                ));
            }

            // Set initial size
            plat::wpe_view_resized(wpe_view as *mut _, width as i32, height as i32);

            // Allocate callback data
            // Store raw pixel data in callback, create textures on main thread
            let events = Rc::new(RefCell::new(Vec::new()));
            let callback_data = Box::into_raw(Box::new(BufferCallbackData {
                view_id: view_id.get(),
                latest_frame: RefCell::new(None),
                frame_transport: creation.frame_transport,
                generation: creation.generation,
                events: events.clone(),
                wake: creation.wake.clone(),
            }));
            tracing::debug!("WpeWebView::new: callback_data={:?}", callback_data);

            // The Neomacs display creates our WPEView subclass. Its
            // render_buffer vfunc transfers each accepted frame into Rust and
            // lets the frame lease decide the exact acknowledgement time.
            if !native::set_render_buffer_callback(
                wpe_view.cast(),
                render_buffer_callback,
                callback_data.cast(),
            ) {
                let _ = Box::from_raw(callback_data);
                plat::g_object_unref(web_view.cast());
                return Err(DisplayError::WebKit(
                    "WPE display did not create the Rust frame-owning view".into(),
                ));
            }

            // Connect decide-policy signal for new window handling
            let decide_policy_signal = CString::new("decide-policy").unwrap();
            let _decide_policy_handler_id = plat::g_signal_connect_data(
                web_view as *mut _,
                decide_policy_signal.as_ptr(),
                Some(std::mem::transmute::<
                    unsafe extern "C" fn(
                        *mut wk::WebKitWebView,
                        *mut wk::WebKitPolicyDecision,
                        u32,
                        *mut libc::c_void,
                    ) -> i32,
                    unsafe extern "C" fn(),
                >(decide_policy_callback)),
                callback_data as *mut _,
                None,
                0, // G_CONNECT_DEFAULT
            );
            tracing::debug!(
                "WpeWebView::new: connected decide-policy signal, handler_id={}",
                _decide_policy_handler_id
            );

            let process_failed_signal = CString::new("web-process-terminated").unwrap();
            let _process_failed_handler_id = plat::g_signal_connect_data(
                web_view.cast(),
                process_failed_signal.as_ptr(),
                Some(std::mem::transmute::<
                    unsafe extern "C" fn(
                        *mut wk::WebKitWebView,
                        wk::WebKitWebProcessTerminationReason,
                        *mut libc::c_void,
                    ),
                    unsafe extern "C" fn(),
                >(web_process_terminated_callback)),
                callback_data.cast(),
                None,
                0,
            );

            // The custom display attaches a matching custom toplevel while it
            // creates the view, keeping WPE's same-display invariant intact.
            let toplevel = plat::wpe_view_get_toplevel(wpe_view.cast());
            if toplevel.is_null() {
                tracing::warn!("WpeWebView::new: custom view has no toplevel");
            } else {
                plat::wpe_toplevel_resize(toplevel, width as i32, height as i32);
            }

            // Map and make the view visible so it starts rendering
            plat::wpe_view_set_visible(wpe_view as *mut plat::WPEView, 1);
            plat::wpe_view_map(wpe_view as *mut plat::WPEView);
            tracing::debug!("WpeWebView::new: view mapped and set visible");

            tracing::info!(
                "WPE Platform WebKitWebView created successfully ({}x{})",
                width,
                height
            );

            Ok(Self {
                view_id,
                generation: creation.generation,
                url: String::new(),
                state: WpeViewState::Ready,
                width,
                height,
                title: None,
                progress: 0.0,
                web_view,
                wpe_view: wpe_view as *mut _,
                callback_data,
                events,
                wake: creation.wake,
            })
        }
    }

    pub(crate) fn native(&self) -> NonNull<wk::WebKitWebView> {
        // Construction rejects a null WebKitWebView and the pointer remains
        // valid until this wrapper's Drop implementation releases it.
        unsafe { NonNull::new_unchecked(self.web_view) }
    }

    /// Load a URL
    pub fn load_uri(&mut self, uri: &str) -> DisplayResult<()> {
        self.url = uri.to_string();
        self.state = WpeViewState::Loading;
        self.progress = 0.0;

        let c_uri = CString::new(uri).map_err(|_| DisplayError::WebKit("Invalid URI".into()))?;

        tracing::debug!(
            "WpeWebView::load_uri: calling webkit_web_view_load_uri({:?}, {:?})",
            self.web_view,
            uri
        );
        unsafe {
            wk::webkit_web_view_load_uri(self.web_view, c_uri.as_ptr());
        }
        tracing::info!("WPE: Loading URI: {}", uri);
        Ok(())
    }

    /// Load HTML content directly
    pub fn load_html(&mut self, html: &str, base_uri: Option<&str>) -> DisplayResult<()> {
        self.state = WpeViewState::Loading;
        self.progress = 0.0;

        let c_html = CString::new(html).map_err(|_| DisplayError::WebKit("Invalid HTML".into()))?;
        let c_base_uri = base_uri.and_then(|u| CString::new(u).ok());

        unsafe {
            wk::webkit_web_view_load_html(
                self.web_view,
                c_html.as_ptr(),
                c_base_uri
                    .as_ref()
                    .map(|s| s.as_ptr())
                    .unwrap_or(ptr::null()),
            );
        }

        tracing::info!("WPE: Loading HTML content");
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

    /// Execute JavaScript
    pub fn execute_javascript(&self, request: &ScriptRequest) -> DisplayResult<()> {
        let c_script = CString::new(request.source.as_str())
            .map_err(|_| DisplayError::WebKit("script contains a NUL byte".into()))?;
        let isolated_world = (request.world == ScriptWorld::Isolated)
            .then(|| CString::new("neomacs-isolated").expect("static world name is valid"));
        let callback = Box::new(ScriptCallbackData {
            view: self.view_id,
            generation: self.generation,
            request: request.request,
            events: self.events.clone(),
            wake: self.wake.clone(),
        });

        unsafe {
            wk::webkit_web_view_evaluate_javascript(
                self.web_view,
                c_script.as_ptr(),
                -1,
                isolated_world
                    .as_ref()
                    .map_or(ptr::null(), |world| world.as_ptr()),
                ptr::null(),
                ptr::null_mut(),
                Some(script_finished_callback),
                Box::into_raw(callback).cast(),
            );
        }

        tracing::debug!("WPE: Executing JavaScript");
        Ok(())
    }

    pub(crate) fn take_events(&self) -> Vec<WebViewEvent> {
        std::mem::take(&mut *self.events.borrow_mut())
    }

    /// Update view state from WebKit
    pub fn update(&mut self) {
        tracing::trace!("WpeWebView::update() called for view {}", self.view_id);
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
            tracing::trace!("WPE update: callback_data ptr = {:?}", self.callback_data);
            if let Some(callback_data) = self.callback_data.as_ref() {
                let frame_avail = callback_data
                    .latest_frame
                    .try_borrow()
                    .is_ok_and(|frame| frame.is_some());
                tracing::trace!("WPE update: frame_available = {}", frame_avail);
                if frame_avail {
                    tracing::info!("WPE update: new frame available, triggering redraw");
                }
            } else {
                tracing::warn!("WPE update: callback_data.as_ref() returned None");
            }
        }
    }

    /// Resize the view
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        unsafe {
            plat::wpe_view_resized(self.wpe_view, width as i32, height as i32);
        }
    }

    /// Apply native keyboard-focus state to the WPE view.
    pub fn set_focus(&self, intent: FocusIntent) -> bool {
        unsafe {
            let before = plat::wpe_view_get_has_focus(self.wpe_view) != 0;
            match intent {
                FocusIntent::Focus => plat::wpe_view_focus_in(self.wpe_view),
                FocusIntent::Blur => plat::wpe_view_focus_out(self.wpe_view),
            }
            before != (plat::wpe_view_get_has_focus(self.wpe_view) != 0)
        }
    }

    /// Take the one negotiated representation of the latest native frame.
    pub(super) fn take_latest_frame(&self) -> Option<CapturedFrame> {
        unsafe {
            if let Some(callback_data) = self.callback_data.as_ref()
                && let Ok(mut latest) = callback_data.latest_frame.try_borrow_mut()
            {
                return latest.take();
            }
        }
        None
    }

    pub fn send_input(&self, input: WebViewInput) {
        match input {
            WebViewInput::Keyboard {
                key_value,
                hardware_key_code,
                state,
                modifiers,
            } => self.send_keyboard_event(key_value, hardware_key_code, state, modifiers),
            WebViewInput::PointerMove {
                position,
                modifiers,
            } => self.send_pointer_move(position, modifiers),
            WebViewInput::PointerButton {
                position,
                button,
                state,
                modifiers,
            } => self.send_pointer_button(position, button, state, modifiers),
            WebViewInput::Scroll {
                position,
                delta,
                modifiers,
            } => self.send_scroll(position, delta, modifiers),
        }
    }

    fn send_keyboard_event(
        &self,
        key_value: u32,
        hardware_key_code: u32,
        state: ButtonState,
        modifiers: WebViewModifiers,
    ) {
        unsafe {
            let event_type = match state {
                ButtonState::Pressed => plat::WPEEventType_WPE_EVENT_KEYBOARD_KEY_DOWN,
                ButtonState::Released => plat::WPEEventType_WPE_EVENT_KEYBOARD_KEY_UP,
            };
            let wpe_modifiers = Self::convert_modifiers(modifiers);
            let time = Self::get_time_ms();

            let event = plat::wpe_event_keyboard_new(
                event_type,
                self.wpe_view,
                plat::WPEInputSource_WPE_INPUT_SOURCE_KEYBOARD,
                time,
                wpe_modifiers,
                hardware_key_code,
                key_value,
            );

            if !event.is_null() {
                plat::wpe_view_event(self.wpe_view, event);
                plat::wpe_event_unref(event);
                tracing::debug!(
                    ?state,
                    key_value,
                    hardware_key_code,
                    "WPE Platform keyboard event"
                );
            } else {
                tracing::warn!("WPE Platform: Failed to create keyboard event");
            }
        }
    }

    fn send_pointer_move(&self, position: WebContentPoint, modifiers: WebViewModifiers) {
        unsafe {
            let wpe_modifiers = Self::convert_modifiers(modifiers);
            let time = Self::get_time_ms();
            let event = plat::wpe_event_pointer_move_new(
                plat::WPEEventType_WPE_EVENT_POINTER_MOVE,
                self.wpe_view,
                plat::WPEInputSource_WPE_INPUT_SOURCE_MOUSE,
                time,
                wpe_modifiers,
                position.x().into(),
                position.y().into(),
                0.0,
                0.0,
            );
            if !event.is_null() {
                plat::wpe_view_event(self.wpe_view, event);
                plat::wpe_event_unref(event);
            }
        }
    }

    fn send_pointer_button(
        &self,
        position: WebContentPoint,
        button: PointerButton,
        state: ButtonState,
        modifiers: WebViewModifiers,
    ) {
        unsafe {
            let wpe_modifiers = Self::convert_modifiers(modifiers);
            let time = Self::get_time_ms();
            let event_type = match state {
                ButtonState::Pressed => plat::WPEEventType_WPE_EVENT_POINTER_DOWN,
                ButtonState::Released => plat::WPEEventType_WPE_EVENT_POINTER_UP,
            };
            let button = match button {
                PointerButton::Primary => 1,
                PointerButton::Middle => 2,
                PointerButton::Secondary => 3,
                PointerButton::Back => 4,
                PointerButton::Forward => 5,
                PointerButton::Other(button) => u32::from(button),
            };
            let press_count = match state {
                ButtonState::Pressed => 1,
                ButtonState::Released => 0,
            };
            let event = plat::wpe_event_pointer_button_new(
                event_type,
                self.wpe_view,
                plat::WPEInputSource_WPE_INPUT_SOURCE_MOUSE,
                time,
                wpe_modifiers,
                button,
                position.x().into(),
                position.y().into(),
                press_count,
            );
            if !event.is_null() {
                plat::wpe_view_event(self.wpe_view, event);
                plat::wpe_event_unref(event);
            }
        }
    }

    fn send_scroll(
        &self,
        position: WebContentPoint,
        delta: WebViewScrollDelta,
        modifiers: WebViewModifiers,
    ) {
        let (x, y, precise) = match delta {
            WebViewScrollDelta::Lines { x, y } => (x, y, 0),
            WebViewScrollDelta::Pixels { x, y } => (x, y, 1),
        };
        unsafe {
            let event = plat::wpe_event_scroll_new(
                self.wpe_view,
                plat::WPEInputSource_WPE_INPUT_SOURCE_MOUSE,
                Self::get_time_ms(),
                Self::convert_modifiers(modifiers),
                x.into(),
                y.into(),
                precise,
                0,
                position.x().into(),
                position.y().into(),
            );
            if !event.is_null() {
                plat::wpe_view_event(self.wpe_view, event);
                plat::wpe_event_unref(event);
            }
        }
    }

    fn convert_modifiers(modifiers: WebViewModifiers) -> u32 {
        let mut wpe_mods = 0u32;
        if modifiers.contains(WebViewModifiers::SHIFT) {
            wpe_mods |= plat::WPEModifiers_WPE_MODIFIER_KEYBOARD_SHIFT;
        }
        if modifiers.contains(WebViewModifiers::CONTROL) {
            wpe_mods |= plat::WPEModifiers_WPE_MODIFIER_KEYBOARD_CONTROL;
        }
        if modifiers.intersects(WebViewModifiers::META | WebViewModifiers::SUPER) {
            wpe_mods |= plat::WPEModifiers_WPE_MODIFIER_KEYBOARD_META;
        }
        if modifiers.contains(WebViewModifiers::ALT) {
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

unsafe extern "C" fn web_process_terminated_callback(
    _web_view: *mut wk::WebKitWebView,
    reason: wk::WebKitWebProcessTerminationReason,
    user_data: *mut libc::c_void,
) {
    guard_wpe_callback("web-process-terminated", (), || unsafe {
        let Some(callback) = (user_data as *mut BufferCallbackData).as_ref() else {
            return;
        };
        let failure = match reason {
            wk::WebKitWebProcessTerminationReason_WEBKIT_WEB_PROCESS_CRASHED => {
                WebProcessFailure::Crashed
            }
            wk::WebKitWebProcessTerminationReason_WEBKIT_WEB_PROCESS_EXCEEDED_MEMORY_LIMIT => {
                WebProcessFailure::ExceededMemoryLimit
            }
            wk::WebKitWebProcessTerminationReason_WEBKIT_WEB_PROCESS_TERMINATED_BY_API => {
                WebProcessFailure::Terminated
            }
            other => WebProcessFailure::Other(other as i32),
        };
        callback
            .events
            .borrow_mut()
            .push(WebViewEvent::ProcessFailed {
                id: WebViewId::new(callback.view_id),
                generation: callback.generation,
                failure,
            });
        callback.wake.notify();
    });
}

impl Drop for WpeWebView {
    fn drop(&mut self) {
        unsafe {
            // Destroy native signal owners while callback userdata is still
            // alive: WPE may emit final release notifications during teardown.
            if !self.web_view.is_null() {
                plat::g_object_unref(self.web_view as *mut _);
            }
            if !self.callback_data.is_null() {
                let _ = Box::from_raw(self.callback_data);
            }
        }
        tracing::debug!("WPE Platform WebKitWebView destroyed");
    }
}

struct ScriptCallbackData {
    view: WebViewId,
    generation: WebViewGeneration,
    request: ScriptRequestId,
    events: Rc<RefCell<Vec<WebViewEvent>>>,
    wake: WebViewWake,
}

unsafe extern "C" fn script_finished_callback(
    source: *mut wk::GObject,
    result: *mut wk::GAsyncResult,
    user_data: wk::gpointer,
) {
    guard_wpe_callback("script-finished", (), || unsafe {
        if user_data.is_null() {
            return;
        }
        let callback = Box::from_raw(user_data.cast::<ScriptCallbackData>());
        let script_result = finish_script(source.cast(), result);
        callback
            .events
            .borrow_mut()
            .push(WebViewEvent::ScriptFinished {
                view: callback.view,
                generation: callback.generation,
                request: callback.request,
                result: script_result,
            });
        callback.wake.notify();
    });
}

unsafe fn finish_script(
    web_view: *mut wk::WebKitWebView,
    result: *mut wk::GAsyncResult,
) -> Result<WebValue, ScriptError> {
    let mut error = ptr::null_mut();
    let value = wk::webkit_web_view_evaluate_javascript_finish(web_view, result, &mut error);
    if !error.is_null() {
        let glib_error = error.cast::<plat::GError>();
        let message = if (*glib_error).message.is_null() {
            "WebKit rejected script evaluation".to_owned()
        } else {
            CStr::from_ptr((*glib_error).message)
                .to_string_lossy()
                .into_owned()
        };
        plat::g_error_free(glib_error);
        return Err(ScriptError::Rejected(message));
    }
    if value.is_null() {
        return Err(ScriptError::ProcessFailed);
    }

    let converted = if wk::jsc_value_is_undefined(value) != 0 || wk::jsc_value_is_null(value) != 0 {
        Ok(WebValue::Null)
    } else {
        let json = wk::jsc_value_to_json(value, 0);
        if json.is_null() {
            Err(ScriptError::Rejected(
                "WebKit returned a non-serializable script value".to_owned(),
            ))
        } else {
            let parsed = CStr::from_ptr(json).to_string_lossy();
            let converted = serde_json::from_str(parsed.as_ref())
                .map(WebValue::from_json)
                .map_err(|error| ScriptError::Rejected(error.to_string()));
            wk::g_free(json.cast());
            converted
        }
    };
    wk::g_object_unref(value.cast());
    converted
}

/// Run a WPE/WebKit `extern "C"` callback body under a panic guard.
///
/// libwpe and WebKit invoke these callbacks synchronously across the FFI
/// boundary in response to web-content-driven events (buffer rendered/released,
/// frame displayed, navigation policy, load state). A Rust panic that unwound
/// across that boundary is undefined behavior and, in practice, aborts the whole
/// editor — so untrusted page activity must never be able to trigger one. We
/// contain any panic here: log it once with the callback name and a best-effort
/// payload string, then hand the C caller `neutral` so it observes an ordinary
/// return. `neutral` is the caller-chosen safe result for the callback's return
/// type (`()` for the void callbacks; see each call site's comment otherwise).
fn guard_wpe_callback<T>(name: &str, neutral: T, body: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(payload) => {
            let detail = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_string());
            tracing::error!(
                "WPE callback {} panicked; contained and returning neutral value: {}",
                name,
                detail
            );
            neutral
        }
    }
}

/// FFI callback for the custom `WPEView::render_buffer` implementation.
///
/// Returning true transfers acknowledgement responsibility to Neomacs. Both
/// transports carry a native lease out of this callback: pixels release it on
/// the next reactor service pass, while DMA-BUF releases it after GPU
/// retirement.
unsafe extern "C" fn render_buffer_callback(
    wpe_view: *mut plat::WPEView,
    buffer: *mut plat::WPEBuffer,
    _damage_rects: *const plat::WPERectangle,
    _damage_rect_count: plat::guint,
    _error: *mut *mut plat::GError,
    user_data: *mut libc::c_void,
) -> plat::gboolean {
    guard_wpe_callback("render_buffer_callback", 0, || unsafe {
        i32::from(capture_render_buffer(wpe_view, buffer, user_data))
    })
}

unsafe fn capture_render_buffer(
    wpe_view: *mut plat::WPEView,
    buffer: *mut plat::WPEBuffer,
    user_data: *mut libc::c_void,
) -> bool {
    tracing::trace!("render_buffer_callback called: buffer={:?}", buffer);

    if wpe_view.is_null() || user_data.is_null() || buffer.is_null() {
        tracing::warn!("render_buffer_callback: null view, user data, or buffer");
        return false;
    }

    let callback_data = &*(user_data as *const BufferCallbackData);
    let width = plat::wpe_buffer_get_width(buffer) as u32;
    let height = plat::wpe_buffer_get_height(buffer) as u32;

    if width == 0
        || height == 0
        || width > MAX_WPE_BUFFER_DIMENSION
        || height > MAX_WPE_BUFFER_DIMENSION
    {
        tracing::warn!(
            "render_buffer_callback: invalid dimensions {}x{}",
            width,
            height
        );
        return false;
    }

    // Acquire the reactor-local bounded slot before creating a native lease.
    // RefCell makes the thread confinement explicit and rejects reentrant
    // capture before the adapter accepts a buffer.
    let Ok(mut latest) = callback_data.latest_frame.try_borrow_mut() else {
        tracing::error!("render_buffer_callback: reentrant latest-frame capture");
        return false;
    };

    let captured = match callback_data.frame_transport {
        WpeFrameTransport::SoftwarePixels => capture_pixels(wpe_view, buffer, width, height),
        WpeFrameTransport::DmaBuf => {
            capture_dmabuf(wpe_view, buffer, width, height).or_else(|| {
                // This remains one representation for this frame. The fallback is
                // local to an unexportable native buffer, not a second parallel
                // capture stream.
                tracing::warn!(
                    "render_buffer_callback: DMA-BUF unavailable; using one software frame"
                );
                capture_pixels(wpe_view, buffer, width, height)
            })
        }
    };
    let Some(captured) = captured else {
        return false;
    };

    *latest = Some(captured);
    callback_data.wake.notify();
    true
}

unsafe fn capture_dmabuf(
    wpe_view: *mut plat::WPEView,
    buffer: *mut plat::WPEBuffer,
    width: u32,
    height: u32,
) -> Option<CapturedFrame> {
    let dmabuf_info = buffer_dmabuf_info(buffer)?;
    tracing::debug!("render_buffer_callback: capturing DMA-BUF {width}x{height}");

    let mut planes = Vec::with_capacity(dmabuf_info.planes.len());
    for plane in &dmabuf_info.planes {
        let duped_fd = libc::dup(plane.fd);
        if duped_fd < 0 {
            tracing::warn!("render_buffer_callback: failed to duplicate DMA-BUF plane");
            return None;
        }
        planes.push(DmaBufPlaneData {
            // SAFETY: `dup` returned a fresh descriptor whose ownership is
            // transferred exactly once into `OwnedFd`.
            fd: OwnedFd::from_raw_fd(duped_fd),
            stride: plane.stride,
            offset: plane.offset,
        });
    }

    let dma_buf = buffer.cast::<plat::WPEBufferDMABuf>();
    let rendering_fence = plat::wpe_buffer_dma_buf_get_rendering_fence(dma_buf);
    let rendering_fence = if rendering_fence < 0 {
        None
    } else {
        let duplicate = libc::dup(rendering_fence);
        if duplicate < 0 {
            tracing::warn!("render_buffer_callback: failed to duplicate rendering fence");
            return None;
        }
        // SAFETY: `dup` returned a fresh descriptor owned by this frame.
        Some(OwnedFd::from_raw_fd(duplicate))
    };

    Some(CapturedFrame::DmaBuf(DmaBufData {
        planes,
        rendering_fence,
        fourcc: dmabuf_info.fourcc,
        modifier: dmabuf_info.modifier,
        width: dmabuf_info.width,
        height: dmabuf_info.height,
        // Retain only after every fallible operation. From this point the
        // custom view has accepted the buffer and must acknowledge it once.
        lease: NativeWpeBufferLease::retain(wpe_view, buffer),
    }))
}

unsafe fn capture_pixels(
    wpe_view: *mut plat::WPEView,
    buffer: *mut plat::WPEBuffer,
    width: u32,
    height: u32,
) -> Option<CapturedFrame> {
    tracing::trace!("render_buffer_callback: capturing software pixels");

    let borrowed_buffer = BorrowedWpeBuffer::from_render_callback(buffer);
    let imported = match borrowed_buffer.import_pixels(width, height) {
        Ok(imported) => imported,
        Err(error) => {
            tracing::warn!("render_buffer_callback: {error}");
            return None;
        }
    };
    tracing::debug!(
        "render_buffer_callback: pixel data size={}, {}x{}, stride={} (min={})",
        imported.bytes.len(),
        width,
        height,
        imported.layout.stride(),
        width as usize * SOFTWARE_PIXEL_BYTES_PER_PIXEL
    );

    // Cairo ARGB32 / XRGB8888 format: bytes in memory [B, G, R, A/X] on little-endian.
    // Target: BGRA for wgpu Bgra8UnormSrgb — same byte order, just force alpha=255.
    let pixels_with_alpha = imported.into_bgra_opaque();

    Some(CapturedFrame::Pixels(RawFrameData {
        pixels: pixels_with_alpha,
        width,
        height,
        // Retain only after the owned copy and all validation are complete.
        _lease: NativeWpeBufferLease::retain(wpe_view, buffer),
    }))
}

/// FFI callback for decide-policy signal from WebKitWebView
/// Handles new window requests (target="_blank", window.open(), etc.)
///
/// Thin panic-containment shim over [`decide_policy_callback_impl`]; see
/// [`guard_wpe_callback`].
unsafe extern "C" fn decide_policy_callback(
    web_view: *mut wk::WebKitWebView,
    decision: *mut wk::WebKitPolicyDecision,
    decision_type: u32,
    user_data: *mut libc::c_void,
) -> i32 {
    // Conservative neutral value on panic is `0` (GLib FALSE). `decide-policy`
    // returns a gboolean: TRUE stops signal emission and asserts that this
    // handler fully resolved the WebKitPolicyDecision (via use/ignore/download),
    // while FALSE lets WebKit's built-in default handler resolve it. A panic can
    // interrupt us before we call any decision method, so returning TRUE would
    // leave the decision unresolved and stall the load; returning FALSE hands it
    // to WebKit's default handler, which resolves deterministically. That is the
    // conservative outcome, it matches what this callback already returns for the
    // cases it declines to special-case (null decision, plain navigation,
    // resource responses, unknown decision types), and — because neomacs wires no
    // `create` signal — it cannot spawn an uncontrolled new WebKitWebView.
    guard_wpe_callback("decide_policy_callback", 0, || unsafe {
        decide_policy_callback_impl(web_view, decision, decision_type, user_data)
    })
}

unsafe fn decide_policy_callback_impl(
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
            let nav_action =
                wk::webkit_navigation_policy_decision_get_navigation_action(nav_decision);
            if nav_action.is_null() {
                tracing::warn!("decide_policy_callback: null navigation action");
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

            tracing::info!(
                view_id = callback_data.view_id,
                "ignoring page-requested new window for {url:?}"
            );
            wk::webkit_policy_decision_ignore(decision);
            1 // TRUE - we handled it (by ignoring)
        }

        WEBKIT_POLICY_DECISION_TYPE_NAVIGATION_ACTION => {
            // Normal navigation - let WebKit handle it
            0 // FALSE
        }

        WEBKIT_POLICY_DECISION_TYPE_RESPONSE => {
            // Resource response - let WebKit handle it
            0 // FALSE
        }

        _ => {
            // Unknown type - let WebKit handle it
            0 // FALSE
        }
    }
}

#[cfg(test)]
mod guard_tests {
    use super::guard_wpe_callback;

    // The five WPE callbacks themselves require a live libwpe/WebKit runtime and
    // raw C pointers to WPE buffers/views, so they are not unit-testable here.
    // These tests cover the one piece of new logic — the panic-containment helper
    // — which is the whole point of the change: a panicking body must never unwind
    // past the guard, and the normal path must be a transparent passthrough.

    #[test]
    fn ok_path_passes_value_through() {
        assert_eq!(guard_wpe_callback("test", 0, || 42), 42);
    }

    #[test]
    fn ok_path_runs_unit_body() {
        let mut ran = false;
        guard_wpe_callback("test", (), || ran = true);
        assert!(ran, "body must run on the normal path");
    }

    #[test]
    fn static_str_panic_returns_neutral() {
        // A panic hook line on stderr is expected; the test still passes because
        // the panic is contained rather than propagated.
        assert_eq!(guard_wpe_callback("test", 7, || panic!("boom")), 7);
    }

    #[test]
    fn string_panic_returns_neutral() {
        // Exercises the String (owned) payload downcast branch.
        let neutral = guard_wpe_callback("test", -1, || panic!("{}", String::from("dynamic")));
        assert_eq!(neutral, -1);
    }
}

#[cfg(test)]
mod software_pixel_tests {
    use super::{SoftwarePixelLayout, SoftwarePixelLayoutError};

    #[test]
    fn pixel_layout_accepts_padded_rows() {
        let layout = SoftwarePixelLayout::new(784, 2, 6_656).unwrap();

        assert_eq!(layout.stride(), 3_328);
        assert_eq!(layout.packed_len(), 6_272);
    }

    #[test]
    fn pixel_copy_discards_row_padding_and_forces_opaque_alpha() {
        let layout = SoftwarePixelLayout::new(1, 2, 16).unwrap();
        let imported = [1, 2, 3, 4, 90, 91, 92, 93, 5, 6, 7, 8, 94, 95, 96, 97];

        assert_eq!(
            layout.copy_bgra_opaque(&imported),
            [1, 2, 3, 255, 5, 6, 7, 255]
        );
    }

    #[test]
    fn pixel_layout_rejects_an_import_larger_than_any_supported_frame() {
        assert_eq!(
            SoftwarePixelLayout::new(1_280, 1_122, 1_342_003_188_534_479_329),
            Err(SoftwarePixelLayoutError::ExceedsFrameLimit {
                actual: 1_342_003_188_534_479_329,
                maximum: 268_435_456,
            })
        );
    }

    #[test]
    fn pixel_layout_rejects_partial_rows() {
        assert_eq!(
            SoftwarePixelLayout::new(2, 2, 17),
            Err(SoftwarePixelLayoutError::PartialRow {
                byte_len: 17,
                height: 2,
            })
        );
    }

    #[test]
    fn pixel_layout_rejects_short_rows() {
        assert_eq!(
            SoftwarePixelLayout::new(2, 2, 8),
            Err(SoftwarePixelLayoutError::StrideTooShort {
                actual: 4,
                minimum: 8,
            })
        );
    }
}

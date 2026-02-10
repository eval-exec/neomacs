//! WPE Backend initialization using WPE Platform API.
//!
//! Uses the modern WPE Platform API (wpe-platform-2.0) for GPU-accelerated
//! web rendering instead of legacy wpebackend-fdo.

use std::ptr;
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::error::{DisplayError, DisplayResult};

use super::sys::platform as plat;
use super::platform::WpePlatformDisplay;

static WPE_INIT: Once = Once::new();
static mut WPE_PLATFORM_DISPLAY: Option<WpePlatformDisplay> = None;
static mut WPE_INIT_ERROR: Option<String> = None;

/// Flag to track if WebKit encountered a fatal error
static WEBKIT_FATAL_ERROR: AtomicBool = AtomicBool::new(false);
/// Store the last WebKit error message
static mut WEBKIT_ERROR_MESSAGE: Option<String> = None;

/// Check if required sandbox tools are available
fn check_sandbox_prerequisites() -> Result<(), String> {
    // Check for bubblewrap
    let bwrap_available = std::process::Command::new("bwrap")
        .arg("--version")
        .output()
        .is_ok();

    // Check for xdg-dbus-proxy
    let dbus_proxy_available = std::process::Command::new("xdg-dbus-proxy")
        .arg("--version")
        .output()
        .is_ok();

    if !bwrap_available || !dbus_proxy_available {
        let mut missing = Vec::new();
        if !bwrap_available {
            missing.push("bubblewrap (bwrap)");
        }
        if !dbus_proxy_available {
            missing.push("xdg-dbus-proxy");
        }

        // Check if sandbox is disabled
        if std::env::var("WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS").is_ok() {
            log::warn!("WebKit sandbox disabled - missing: {}", missing.join(", "));
            return Ok(());
        }

        return Err(format!(
            "WebKit requires sandbox tools: {}. \
             Install them or set WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 to disable sandbox (not recommended).",
            missing.join(", ")
        ));
    }

    Ok(())
}

/// Check if WebKit has encountered a fatal error
pub fn has_webkit_error() -> bool {
    WEBKIT_FATAL_ERROR.load(Ordering::SeqCst)
}

/// Get and clear the last WebKit error message
pub fn take_webkit_error() -> Option<String> {
    if WEBKIT_FATAL_ERROR.swap(false, Ordering::SeqCst) {
        unsafe { (*std::ptr::addr_of_mut!(WEBKIT_ERROR_MESSAGE)).take() }
    } else {
        None
    }
}

/// WPE Backend manager using WPE Platform API.
///
/// Uses headless WPE Platform display for embedding web content
/// without requiring a Wayland compositor.
pub struct WpeBackend {
    /// Reference to the shared platform display
    display: *mut plat::WPEDisplay,
    /// EGL display for texture operations
    egl_display: *mut libc::c_void,
}

impl WpeBackend {
    /// Initialize WPE backend with WPE Platform API.
    ///
    /// Creates a headless WPE Platform display for embedding.
    /// If a device path is not provided, uses the default GPU.
    pub unsafe fn new(_egl_display_hint: *mut libc::c_void) -> DisplayResult<Self> {
        Self::new_with_device(_egl_display_hint, None)
    }

    /// Initialize WPE backend with a specific DRM device.
    ///
    /// This allows WPE to use the same GPU as wgpu for zero-copy DMA-BUF sharing.
    ///
    /// # Arguments
    /// * `egl_display_hint` - EGL display hint (unused with WPE Platform API)
    /// * `device_path` - Optional DRM render node path (e.g., "/dev/dri/renderD128")
    pub unsafe fn new_with_device(
        _egl_display_hint: *mut libc::c_void,
        device_path: Option<&str>,
    ) -> DisplayResult<Self> {
        // Store the device path in a static for the once closure
        static mut DEVICE_PATH: Option<String> = None;
        if let Some(path) = device_path {
            *std::ptr::addr_of_mut!(DEVICE_PATH) = Some(path.to_string());
        }

        WPE_INIT.call_once(|| {
            let dev_path = (*std::ptr::addr_of!(DEVICE_PATH)).as_deref();
            if let Some(path) = dev_path {
                log::info!("WpeBackend: Initializing WPE Platform API with device: {}", path);
            } else {
                log::info!("WpeBackend: Initializing WPE Platform API (default device)...");
            }

            // Check sandbox prerequisites first
            if let Err(msg) = check_sandbox_prerequisites() {
                log::error!("WpeBackend: ERROR - {}", msg);
                *std::ptr::addr_of_mut!(WPE_INIT_ERROR) = Some(msg);
                return;
            }

            let result = if let Some(path) = dev_path {
                WpePlatformDisplay::new_headless_for_device(path)
            } else {
                WpePlatformDisplay::new_headless()
            };

            match result {
                Ok(display) => {
                    log::info!("WpeBackend: WPE Platform display created successfully");
                    log::info!("WpeBackend: EGL available: {}", display.has_egl());
                    *std::ptr::addr_of_mut!(WPE_PLATFORM_DISPLAY) = Some(display);
                }
                Err(e) => {
                    let msg = format!("Failed to create WPE Platform display: {}", e);
                    log::error!("WpeBackend: ERROR - {}", msg);
                    *std::ptr::addr_of_mut!(WPE_INIT_ERROR) = Some(msg);
                }
            }
        });

        // Check for init error
        if let Some(ref error) = *std::ptr::addr_of!(WPE_INIT_ERROR) {
            return Err(DisplayError::WebKit(error.clone()));
        }

        // Get the display
        let platform_display = (*std::ptr::addr_of!(WPE_PLATFORM_DISPLAY)).as_ref()
            .ok_or_else(|| DisplayError::WebKit("WPE Platform not initialized".into()))?;

        Ok(Self {
            display: platform_display.raw(),
            egl_display: platform_display.egl_display(),
        })
    }

    /// Check if WPE is initialized
    pub fn is_initialized(&self) -> bool {
        !self.display.is_null()
    }

    /// Get the EGL display
    pub fn egl_display(&self) -> *mut libc::c_void {
        self.egl_display
    }

    /// Get the WPE Platform display
    pub fn platform_display(&self) -> Option<&WpePlatformDisplay> {
        unsafe { (*std::ptr::addr_of!(WPE_PLATFORM_DISPLAY)).as_ref() }
    }
}

impl Drop for WpeBackend {
    fn drop(&mut self) {
        log::debug!("WpeBackend dropped");
    }
}

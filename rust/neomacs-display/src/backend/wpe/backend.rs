//! WPE Backend initialization using WPE Platform API.
//!
//! Uses the modern WPE Platform API (wpe-platform-2.0) for GPU-accelerated
//! web rendering instead of legacy wpebackend-fdo.

use std::ptr;
use std::sync::Once;

use crate::core::error::{DisplayError, DisplayResult};

use super::sys::platform as plat;
use super::platform::WpePlatformDisplay;

static WPE_INIT: Once = Once::new();
static mut WPE_PLATFORM_DISPLAY: Option<WpePlatformDisplay> = None;
static mut WPE_INIT_ERROR: Option<String> = None;

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
    pub unsafe fn new(_egl_display_hint: *mut libc::c_void) -> DisplayResult<Self> {
        WPE_INIT.call_once(|| {
            eprintln!("WpeBackend: Initializing WPE Platform API...");
            
            match WpePlatformDisplay::new_headless() {
                Ok(display) => {
                    eprintln!("WpeBackend: WPE Platform display created successfully");
                    eprintln!("WpeBackend: EGL available: {}", display.has_egl());
                    WPE_PLATFORM_DISPLAY = Some(display);
                }
                Err(e) => {
                    let msg = format!("Failed to create WPE Platform display: {}", e);
                    eprintln!("WpeBackend: ERROR - {}", msg);
                    WPE_INIT_ERROR = Some(msg);
                }
            }
        });

        // Check for init error
        if let Some(ref error) = WPE_INIT_ERROR {
            return Err(DisplayError::WebKit(error.clone()));
        }

        // Get the display
        let platform_display = WPE_PLATFORM_DISPLAY.as_ref()
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
        unsafe { WPE_PLATFORM_DISPLAY.as_ref() }
    }
}

impl Drop for WpeBackend {
    fn drop(&mut self) {
        log::debug!("WpeBackend dropped");
    }
}

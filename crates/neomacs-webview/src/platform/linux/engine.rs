//! WPEPlatform process and display initialization.
//!
//! Uses the modern WPE Platform API (wpe-platform-2.0) for GPU-accelerated
//! web rendering instead of legacy wpebackend-fdo.

use super::error::{DisplayError, DisplayResult};

use super::display::WpePlatformDisplay;
use super::sys::platform as plat;

struct ThreadMainContext(*mut plat::GMainContext);

impl ThreadMainContext {
    unsafe fn new() -> DisplayResult<Self> {
        let context = plat::g_main_context_new();
        if context.is_null() {
            return Err(DisplayError::WebKit(
                "failed to create the WebKit GLib main context".into(),
            ));
        }
        if plat::g_main_context_acquire(context) == 0 {
            plat::g_main_context_unref(context);
            return Err(DisplayError::WebKit(
                "failed to acquire the WebKit GLib main context".into(),
            ));
        }
        plat::g_main_context_push_thread_default(context);
        Ok(Self(context))
    }
}

impl Drop for ThreadMainContext {
    fn drop(&mut self) {
        unsafe {
            plat::g_main_context_pop_thread_default(self.0);
            plat::g_main_context_release(self.0);
            plat::g_main_context_unref(self.0);
        }
    }
}

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
            tracing::warn!("WebKit sandbox disabled - missing: {}", missing.join(", "));
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

/// WPE Backend manager using WPE Platform API.
///
/// Uses headless WPE Platform display for embedding web content
/// without requiring a Wayland compositor.
pub struct WpeBackend {
    /// Reactor-local display. Declared before the context so Rust drops every
    /// WPE/GObject reference while its owning thread-default context is alive.
    display: WpePlatformDisplay,
    /// Dedicated thread-default GLib context inherited by WebKit async work;
    /// this must remain the final field.
    _main_context: ThreadMainContext,
}

impl WpeBackend {
    /// Initialize WPE backend with WPE Platform API.
    ///
    /// Creates a headless WPE Platform display for embedding.
    /// If a device path is not provided, uses the default GPU.
    ///
    /// # Safety
    /// `egl_display_hint` must be null or a valid EGL display pointer. WPE
    /// Platform initialization is confined to the calling reactor thread.
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
    ///
    /// # Safety
    /// `egl_display_hint` must be null or a valid EGL display pointer. This
    /// creates reactor-local WPE Platform state.
    pub unsafe fn new_with_device(
        _egl_display_hint: *mut libc::c_void,
        device_path: Option<&str>,
    ) -> DisplayResult<Self> {
        let main_context = ThreadMainContext::new()?;

        check_sandbox_prerequisites().map_err(DisplayError::WebKit)?;
        let platform_display = if let Some(path) = device_path {
            tracing::info!("WpeBackend: initializing WPE Platform for {path}");
            WpePlatformDisplay::new_headless_for_device(path)?
        } else {
            tracing::info!("WpeBackend: initializing WPE Platform on the default device");
            WpePlatformDisplay::new_headless()?
        };
        tracing::info!(
            egl_available = platform_display.has_egl(),
            "WpeBackend: reactor-local WPE display initialized"
        );

        Ok(Self {
            display: platform_display,
            _main_context: main_context,
        })
    }

    /// Get the WPE Platform display
    pub fn platform_display(&self) -> &WpePlatformDisplay {
        &self.display
    }

    /// The context owned by the current WPE reactor thread.
    pub(super) const fn main_context(&self) -> *mut plat::GMainContext {
        self._main_context.0
    }
}

impl Drop for WpeBackend {
    fn drop(&mut self) {
        tracing::debug!("WpeBackend dropped");
    }
}

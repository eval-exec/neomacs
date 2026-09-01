//! WPEPlatform display integration for GPU-accelerated web rendering.
//!
//! This module uses the new WPE Platform API (wpe-platform-2.0) instead of
//! the legacy wpebackend-fdo. The Platform API provides:
//! - Direct dma-buf buffer access (zero-copy GPU rendering)
//! - Proper device handling for WebKit subprocesses
//! - Cleaner architecture with GObject signals
//!
//! Architecture:
//! ```text
//! WPEDisplay (headless) → WebKitWebView → WPEView
//!                                            ↓ "buffer-rendered" signal
//!                                         WPEBuffer
//!                                            ↓ wpe_buffer_import_to_egl_image()
//!                                         EGLImage → GdkTexture
//! ```

use std::ffi::CString;
use std::ptr;
use tracing::{debug, info, warn};

use super::error::{DisplayError, DisplayResult};
use super::native;
use super::sys::platform as plat;

/// WPE Platform Display wrapper
///
/// Uses headless mode for embedding - doesn't require a Wayland compositor
pub struct WpePlatformDisplay {
    display: *mut plat::WPEDisplay,
    egl_display: *mut libc::c_void,
}

impl WpePlatformDisplay {
    /// Create a new headless WPE Platform display
    pub fn new_headless() -> DisplayResult<Self> {
        Self::new_headless_internal(None)
    }

    /// Create a new headless WPE Platform display for a specific DRM device.
    ///
    /// This allows WPE to use the same GPU as wgpu for zero-copy DMA-BUF sharing.
    ///
    /// # Arguments
    /// * `device_path` - The DRM render node path (e.g., "/dev/dri/renderD128")
    pub fn new_headless_for_device(device_path: &str) -> DisplayResult<Self> {
        Self::new_headless_internal(Some(device_path))
    }

    /// Internal implementation for headless display creation.
    fn new_headless_internal(device_path: Option<&str>) -> DisplayResult<Self> {
        unsafe {
            if let Some(path) = device_path {
                info!(
                    "WpePlatformDisplay: Creating headless display for device: {}",
                    path
                );
            } else {
                info!("WpePlatformDisplay: Creating headless display (default device)...");
            }

            // WPE's stock headless display supplies EGL/DRM capabilities. A
            // Neomacs display adapter delegates those capabilities while its
            // `create_view` vfunc returns the frame-acknowledging view owned by
            // our reactor.
            let delegate = if let Some(path) = device_path {
                let c_path = CString::new(path)
                    .map_err(|_| DisplayError::WebKit("Invalid device path".into()))?;
                let mut error: *mut plat::GError = ptr::null_mut();
                let d = plat::wpe_display_headless_new_for_device(c_path.as_ptr(), &mut error);
                if d.is_null() {
                    let error_msg = if !error.is_null() {
                        let msg = std::ffi::CStr::from_ptr((*error).message)
                            .to_string_lossy()
                            .into_owned();
                        plat::g_error_free(error);
                        msg
                    } else {
                        "Unknown error".into()
                    };
                    return Err(DisplayError::WebKit(format!(
                        "Failed to create WPE headless display for device {}: {}",
                        path, error_msg
                    )));
                }
                d
            } else {
                plat::wpe_display_headless_new()
            };

            if delegate.is_null() {
                return Err(DisplayError::WebKit(
                    "Failed to create WPE headless display".into(),
                ));
            }
            let display = native::new_display(delegate);
            plat::g_object_unref(delegate.cast());
            if display.is_null() {
                return Err(DisplayError::WebKit(
                    "Failed to create Neomacs WPE display adapter".into(),
                ));
            }
            let display_ptr = display;
            info!(
                "WpePlatformDisplay: Headless display created: {:?}",
                display_ptr
            );

            // Connect the display
            let mut error: *mut plat::GError = ptr::null_mut();
            let connected = plat::wpe_display_connect(display, &mut error);
            if connected == 0 {
                let error_msg = if !error.is_null() {
                    let msg = std::ffi::CStr::from_ptr((*error).message)
                        .to_string_lossy()
                        .into_owned();
                    plat::g_error_free(error);
                    msg
                } else {
                    "Unknown error".into()
                };
                plat::g_object_unref(display as *mut _);
                return Err(DisplayError::WebKit(format!(
                    "Failed to connect WPE display: {}",
                    error_msg
                )));
            }
            info!("WpePlatformDisplay: Display connected");

            // Get EGL display from WPE Platform
            let mut error: *mut plat::GError = ptr::null_mut();
            let egl_display = plat::wpe_display_get_egl_display(display, &mut error);
            if egl_display.is_null() {
                let error_msg = if !error.is_null() {
                    let msg = std::ffi::CStr::from_ptr((*error).message)
                        .to_string_lossy()
                        .into_owned();
                    plat::g_error_free(error);
                    msg
                } else {
                    "Unknown error".into()
                };
                warn!(
                    "WpePlatformDisplay: Failed to get EGL display: {}",
                    error_msg
                );
                // Continue without EGL - will use pixel import fallback
            }
            info!("WpePlatformDisplay: EGL display: {:?}", egl_display);

            Ok(Self {
                display,
                egl_display,
            })
        }
    }

    /// Get the raw WPEDisplay pointer
    pub fn raw(&self) -> *mut plat::WPEDisplay {
        self.display
    }

    /// Check if EGL is available
    pub fn has_egl(&self) -> bool {
        !self.egl_display.is_null()
    }
}

impl Drop for WpePlatformDisplay {
    fn drop(&mut self) {
        unsafe {
            if !self.display.is_null() {
                plat::g_object_unref(self.display as *mut _);
            }
        }
    }
}

/// Check if a WPEBuffer is a DMA-BUF buffer and return its info
///
/// Returns (fourcc, n_planes, modifier, fd, stride, offset) if it's a DMA-BUF buffer
pub fn buffer_dmabuf_info(buffer: *mut plat::WPEBuffer) -> Option<DmaBufInfo> {
    unsafe {
        if buffer.is_null() {
            return None;
        }

        // Check if buffer is WPEBufferDMABuf type
        let dmabuf_type = plat::wpe_buffer_dma_buf_get_type();
        let buffer_type = plat::g_type_check_instance_is_a(buffer as *mut _, dmabuf_type);

        if buffer_type == 0 {
            debug!("buffer_dmabuf_info: buffer is not WPEBufferDMABuf");
            return None;
        }

        let dmabuf = buffer as *mut plat::WPEBufferDMABuf;

        let fourcc = plat::wpe_buffer_dma_buf_get_format(dmabuf);
        let n_planes = plat::wpe_buffer_dma_buf_get_n_planes(dmabuf);
        let modifier = plat::wpe_buffer_dma_buf_get_modifier(dmabuf);
        let width = plat::wpe_buffer_get_width(buffer) as u32;
        let height = plat::wpe_buffer_get_height(buffer) as u32;

        if n_planes == 0 || n_planes > 4 {
            warn!("buffer_dmabuf_info: invalid plane count: {}", n_planes);
            return None;
        }

        let mut planes = Vec::with_capacity(n_planes as usize);
        for i in 0..n_planes {
            let fd = plat::wpe_buffer_dma_buf_get_fd(dmabuf, i);
            let stride = plat::wpe_buffer_dma_buf_get_stride(dmabuf, i);
            let offset = plat::wpe_buffer_dma_buf_get_offset(dmabuf, i);
            planes.push(DmaBufPlane { fd, stride, offset });
        }

        info!(
            "buffer_dmabuf_info: DMA-BUF buffer {}x{}, fourcc={:08x}, planes={}, modifier={:016x}",
            width, height, fourcc, n_planes, modifier
        );

        Some(DmaBufInfo {
            fourcc,
            n_planes,
            modifier,
            width,
            height,
            planes,
        })
    }
}

/// DMA-BUF buffer information
#[derive(Debug)]
pub struct DmaBufInfo {
    pub fourcc: u32,
    /// Plane count reported by WPE; the per-plane data itself lives in `planes`.
    #[allow(dead_code)]
    pub n_planes: u32,
    pub modifier: u64,
    pub width: u32,
    pub height: u32,
    pub planes: Vec<DmaBufPlane>,
}

/// DMA-BUF plane information
#[derive(Debug)]
pub struct DmaBufPlane {
    pub fd: i32,
    pub stride: u32,
    pub offset: u32,
}

#[cfg(test)]
#[path = "display_test.rs"]
mod tests;

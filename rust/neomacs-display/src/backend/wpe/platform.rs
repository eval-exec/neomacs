//! WPE Platform API integration for GPU-accelerated web rendering.
//!
//! This module uses the new WPE Platform API (wpe-platform-2.0) instead of
//! the legacy wpebackend-fdo. The Platform API provides:
//! - Direct dma-buf buffer access (zero-copy GPU rendering)
//! - Proper device handling for WebKit subprocesses
//! - Cleaner architecture with GObject signals
//!
//! Architecture:
//! ```
//! WPEDisplay (headless) → WebKitWebView → WPEView
//!                                            ↓ "buffer-rendered" signal
//!                                         WPEBuffer
//!                                            ↓ wpe_buffer_import_to_egl_image()
//!                                         EGLImage → GdkTexture
//! ```

use std::ptr;
use std::ffi::CString;
use gtk4::gdk;
use gtk4::prelude::*;
use log::{debug, info, warn, error};

use super::sys::platform as plat;
use super::sys::egl;
use super::sys::webkit as wk;
use crate::error::{DisplayError, DisplayResult};

/// WPE Platform Display wrapper
/// 
/// Uses headless mode for embedding - doesn't require a Wayland compositor
pub struct WpePlatformDisplay {
    display: *mut plat::WPEDisplay,
    egl_display: egl::EGLDisplay,
}

impl WpePlatformDisplay {
    /// Create a new headless WPE Platform display
    pub fn new_headless() -> DisplayResult<Self> {
        unsafe {
            info!("WpePlatformDisplay: Creating headless display...");
            
            // Create headless display
            let display = plat::wpe_display_headless_new();
            if display.is_null() {
                return Err(DisplayError::WebKit("Failed to create WPE headless display".into()));
            }
            info!("WpePlatformDisplay: Headless display created: {:?}", display);
            
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
                    "Failed to connect WPE display: {}", error_msg
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
                warn!("WpePlatformDisplay: Failed to get EGL display: {}", error_msg);
                // Continue without EGL - will use pixel import fallback
            }
            info!("WpePlatformDisplay: EGL display: {:?}", egl_display);
            
            // Set as primary display for WebKit
            plat::wpe_display_set_primary(display);
            
            Ok(Self {
                display,
                egl_display: egl_display as egl::EGLDisplay,
            })
        }
    }
    
    /// Get the raw WPEDisplay pointer
    pub fn raw(&self) -> *mut plat::WPEDisplay {
        self.display
    }
    
    /// Get the EGL display
    pub fn egl_display(&self) -> egl::EGLDisplay {
        self.egl_display
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

/// WPE Platform View wrapper
///
/// Wraps the WPEView obtained from webkit_web_view_get_wpe_view()
/// Handles buffer-rendered signals for GPU texture extraction
pub struct WpePlatformView {
    wpe_view: *mut plat::WPEView,
    web_view: *mut wk::WebKitWebView,
    width: u32,
    height: u32,
}

impl WpePlatformView {
    /// Create a new WPE Platform view from a WebKitWebView
    ///
    /// The WebKitWebView must have been created with a WPE Platform display
    pub fn from_web_view(web_view: *mut wk::WebKitWebView) -> DisplayResult<Self> {
        unsafe {
            if web_view.is_null() {
                return Err(DisplayError::WebKit("WebKitWebView is null".into()));
            }
            
            // Get the WPEView from WebKitWebView
            let wpe_view = wk::webkit_web_view_get_wpe_view(web_view);
            if wpe_view.is_null() {
                return Err(DisplayError::WebKit(
                    "WebKitWebView has no WPEView - was it created with WPE Platform display?".into()
                ));
            }
            
            let width = plat::wpe_view_get_width(wpe_view as *mut _) as u32;
            let height = plat::wpe_view_get_height(wpe_view as *mut _) as u32;
            
            info!("WpePlatformView: Got WPEView {:?} from WebKitWebView {:?} ({}x{})",
                  wpe_view, web_view, width, height);
            
            Ok(Self {
                wpe_view: wpe_view as *mut _,
                web_view,
                width,
                height,
            })
        }
    }
    
    /// Get the raw WPEView pointer
    pub fn raw(&self) -> *mut plat::WPEView {
        self.wpe_view
    }
    
    /// Resize the view
    pub fn resize(&mut self, width: u32, height: u32) {
        unsafe {
            plat::wpe_view_resized(self.wpe_view, width as i32, height as i32);
            self.width = width;
            self.height = height;
        }
    }
    
    /// Get current dimensions
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    
    /// Focus the view
    pub fn focus(&self) {
        unsafe {
            plat::wpe_view_focus_in(self.wpe_view);
        }
    }
    
    /// Unfocus the view
    pub fn unfocus(&self) {
        unsafe {
            plat::wpe_view_focus_out(self.wpe_view);
        }
    }
}

/// Import a WPEBuffer to an EGL image
///
/// Returns the EGLImage that can be converted to a GdkTexture
pub fn buffer_to_egl_image(buffer: *mut plat::WPEBuffer) -> DisplayResult<egl::EGLImageKHR> {
    unsafe {
        if buffer.is_null() {
            return Err(DisplayError::WebKit("WPEBuffer is null".into()));
        }
        
        let mut error: *mut plat::GError = ptr::null_mut();
        let egl_image = plat::wpe_buffer_import_to_egl_image(buffer, &mut error);
        
        if egl_image.is_null() {
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
                "Failed to import buffer to EGL image: {}", error_msg
            )));
        }
        
        Ok(egl_image as egl::EGLImageKHR)
    }
}

/// Import a WPEBuffer to pixels (fallback for non-EGL)
///
/// Returns pixel data as GBytes
pub fn buffer_to_pixels(buffer: *mut plat::WPEBuffer) -> DisplayResult<(*mut plat::GBytes, u32, u32)> {
    unsafe {
        if buffer.is_null() {
            return Err(DisplayError::WebKit("WPEBuffer is null".into()));
        }
        
        let width = plat::wpe_buffer_get_width(buffer) as u32;
        let height = plat::wpe_buffer_get_height(buffer) as u32;
        
        let mut error: *mut plat::GError = ptr::null_mut();
        let bytes = plat::wpe_buffer_import_to_pixels(buffer, &mut error);
        
        if bytes.is_null() {
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
                "Failed to import buffer to pixels: {}", error_msg
            )));
        }
        
        Ok((bytes, width, height))
    }
}

/// Convert GBytes pixel data to GdkTexture
pub fn pixels_to_texture(
    bytes: *mut plat::GBytes,
    width: u32,
    height: u32,
) -> DisplayResult<gdk::Texture> {
    unsafe {
        if bytes.is_null() {
            return Err(DisplayError::WebKit("GBytes is null".into()));
        }
        
        let mut size: plat::gsize = 0;
        let data = plat::g_bytes_get_data(bytes, &mut size);
        
        if data.is_null() || size == 0 {
            plat::g_bytes_unref(bytes);
            return Err(DisplayError::WebKit("Empty pixel data".into()));
        }
        
        // Create a slice from the pixel data
        let pixel_slice = std::slice::from_raw_parts(data as *const u8, size as usize);
        
        // Create GdkMemoryTexture
        // WPE uses BGRA8888 format (premultiplied)
        let glib_bytes = gtk4::glib::Bytes::from(pixel_slice);
        let stride = width * 4; // 4 bytes per pixel (BGRA)
        
        let texture = gdk::MemoryTexture::new(
            width as i32,
            height as i32,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &glib_bytes,
            stride as usize,
        );
        
        // Free the original GBytes
        plat::g_bytes_unref(bytes);
        
        Ok(texture.upcast())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_platform_display_types() {
        // Just verify types compile
        let _: *mut plat::WPEDisplay = std::ptr::null_mut();
        let _: *mut plat::WPEView = std::ptr::null_mut();
        let _: *mut plat::WPEBuffer = std::ptr::null_mut();
    }
}

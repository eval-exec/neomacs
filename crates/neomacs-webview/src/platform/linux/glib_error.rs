//! Owning adapter for GLib's nullable `GError **` convention.
//!
//! Native APIs write a full-transfer `GError` into the slot on failure. Keeping
//! that pointer private makes message access and destruction the responsibility
//! of `glib::Error` instead of duplicating unchecked field dereferences at each
//! call site.

use std::{mem, ptr};

use glib::translate::from_glib_full;

pub(super) struct GlibErrorSlot {
    raw: *mut glib::ffi::GError,
}

impl GlibErrorSlot {
    pub(super) const fn new() -> Self {
        Self {
            raw: ptr::null_mut(),
        }
    }

    /// Returns the storage address expected by a GLib `GError **` parameter.
    pub(super) fn out_ptr(&mut self) -> *mut *mut glib::ffi::GError {
        ptr::from_mut(&mut self.raw)
    }

    pub(super) fn is_set(&self) -> bool {
        !self.raw.is_null()
    }

    pub(super) fn into_message(mut self, fallback: &str) -> String {
        self.take()
            .map(|error| error.to_string())
            .unwrap_or_else(|| fallback.to_owned())
    }

    fn take(&mut self) -> Option<glib::Error> {
        let raw = mem::replace(&mut self.raw, ptr::null_mut());
        if raw.is_null() {
            None
        } else {
            // SAFETY: GLib error out-parameters return a newly allocated,
            // full-transfer GError. `take` clears the slot before constructing
            // the sole owning wrapper, so it cannot be converted or freed twice.
            Some(unsafe { from_glib_full(raw) })
        }
    }
}

impl Drop for GlibErrorSlot {
    fn drop(&mut self) {
        let _ = self.take();
    }
}

#[cfg(test)]
#[path = "glib_error/tests/glib_error_test.rs"]
mod tests;

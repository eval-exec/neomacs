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
mod tests {
    use glib::translate::IntoGlibPtr;

    use super::GlibErrorSlot;

    #[test]
    fn empty_error_slot_uses_the_callers_fallback() {
        let slot = GlibErrorSlot::new();

        assert_eq!(
            slot.into_message("native call failed"),
            "native call failed"
        );
    }

    #[test]
    fn error_slot_owns_and_formats_the_error_returned_by_glib() {
        let mut slot = GlibErrorSlot::new();
        let error = glib::Error::new(glib::FileError::Failed, "native failure");

        unsafe {
            *slot.out_ptr() = error.into_glib_ptr();
        }

        assert_eq!(slot.into_message("fallback"), "native failure");
    }
}

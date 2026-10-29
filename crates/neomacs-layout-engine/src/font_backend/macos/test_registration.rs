use objc2_core_foundation::{CFRetained, CFURL};
use objc2_core_text::{
    CTFontManagerRegisterFontsForURL, CTFontManagerScope, CTFontManagerUnregisterFontsForURL,
};
use std::path::Path;
use std::ptr;

/// Only unregister a registration successfully owned by this test. Retaining
/// its URL and leaving the pinned file in place satisfies CoreText's lifetime
/// contract; process scope cannot change another application's font catalog.
#[must_use = "retain the registration until every native font user has closed"]
pub(crate) struct ProcessFontRegistration(CFRetained<CFURL>);

// CFRetained implements Pointer but does not implement Debug. Expose only
// the owned URL handle, without adding a framework call to error formatting.
impl std::fmt::Debug for ProcessFontRegistration {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProcessFontRegistration")
            .field("url", &format_args!("{:p}", self.0))
            .finish()
    }
}

impl ProcessFontRegistration {
    pub(crate) fn new(path: &Path) -> Self {
        let url = CFURL::from_file_path(path).expect("pinned fixture file URL");
        // SAFETY: the URL is retained throughout registration, the fixture is
        // not moved, and the optional CFError output pointer is documented null.
        assert!(
            unsafe {
                CTFontManagerRegisterFontsForURL(&url, CTFontManagerScope::Process, ptr::null_mut())
            },
            "CoreText must register the pinned fixture for this process"
        );
        Self(url)
    }
}

impl Drop for ProcessFontRegistration {
    fn drop(&mut self) {
        // SAFETY: this URL registered successfully with the same process scope;
        // the retained URL is live and a null error-output pointer is permitted.
        // Avoid a second panic during unwinding; process scope also bounds the
        // registration lifetime if the OS reports an unregistration failure.
        let _ = unsafe {
            CTFontManagerUnregisterFontsForURL(
                &self.0,
                CTFontManagerScope::Process,
                ptr::null_mut(),
            )
        };
    }
}

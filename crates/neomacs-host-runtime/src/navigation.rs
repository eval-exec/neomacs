//! External web navigation, supplied by the browser rather than a subprocess.

/// Ask the embedding browser to open a web URL in another tab.
///
/// Success means the request was accepted, not that the destination loaded.
/// The frontend owns popup-blocker feedback and any required user interaction.
/// Native applications continue to use the ordinary Lisp `browse-url` backends.
pub fn open_external_url(url: &str) -> Result<(), &'static str> {
    std::cfg_select! {
        target_family = "wasm" => {
            #[link(wasm_import_module = "neomacs_host")]
            unsafe extern "C" {
                #[link_name = "open_external_url"]
                fn imported_open_external_url(source: *const u8, length: u32) -> u32;
            }
            let length = u32::try_from(url.len()).map_err(|_| "URL is too long")?;
            // SAFETY: the host copies and validates this UTF-8 slice before
            // returning; no pointer or Lisp value crosses the Worker boundary.
            match unsafe { imported_open_external_url(url.as_ptr(), length) } {
                1 => Ok(()),
                _ => Err("The browser accepts only absolute HTTP or HTTPS URLs"),
            }
        }
        _ => {
            let _ = url;
            Err("Browser navigation is unavailable on this host; use browse-url")
        }
    }
}

//! Runtime projection of native resources embedded in a macOS app bundle.
//!
//! Packaging owns the concrete directory layout.  The executable consumes one
//! small interface here so GStreamer never falls back to the build machine's
//! paths when Neomacs is launched from a relocatable `.app`.

#[cfg(any(target_os = "macos", test))]
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct MacOsBundleRuntime {
    plugin_system_path: PathBuf,
    plugin_scanner: PathBuf,
    gio_modules: PathBuf,
}

#[cfg(any(target_os = "macos", test))]
impl MacOsBundleRuntime {
    fn from_executable(executable: &Path) -> Option<Self> {
        let macos = executable.parent()?;
        if macos.file_name()? != "MacOS" {
            return None;
        }
        let contents = macos.parent()?;
        if contents.file_name()? != "Contents" {
            return None;
        }

        Some(Self {
            // Loadable modules live under Resources, not PlugIns.  Contents's
            // V2 resource rules mark Frameworks|PlugIns|MacOS|Helpers as
            // NESTED, so codesign treats any SUBDIRECTORY of them as a nested
            // bundle and refuses one that is not: "bundle format unrecognized,
            // invalid, or unsuitable".  Measured on macOS 26.5.2 arm64 -- a
            // subdirectory fails under both PlugIns and Frameworks, while a
            // FLAT file under Frameworks and anything under Resources passes.
            // gst-plugin-scanner stays in Helpers because it is flat there.
            plugin_system_path: contents.join("Resources/gstreamer-1.0"),
            plugin_scanner: contents.join("Helpers/gst-plugin-scanner"),
            gio_modules: contents.join("Resources/gio"),
        })
    }

    #[cfg(any(feature = "video", test))]
    fn media_is_complete(&self) -> bool {
        self.plugin_system_path.is_dir() && self.plugin_scanner.is_file()
    }
}

/// Configure the private native runtime before any process thread is created.
///
/// `std::env::set_var` is unsafe in a multithreaded Unix process.  The sole
/// caller is deliberately the first statement in `main`, before Neomacs starts
/// the evaluator, renderer, or decoder threads.
#[cfg(target_os = "macos")]
pub fn configure_before_threads() {
    let Some(runtime) = std::env::current_exe()
        .ok()
        .and_then(|path| MacOsBundleRuntime::from_executable(&path))
    else {
        return;
    };

    unsafe {
        #[cfg(feature = "video")]
        if runtime.media_is_complete() {
            // Restrict packaged builds to the signed plug-ins inside the app.
            // The versioned names take precedence in GStreamer 1.x; the
            // unversioned scanner name retains compatibility with older 1.x
            // runtimes.
            std::env::set_var("GST_PLUGIN_PATH", "");
            std::env::set_var("GST_PLUGIN_PATH_1_0", "");
            std::env::set_var("GST_PLUGIN_SYSTEM_PATH", &runtime.plugin_system_path);
            std::env::set_var("GST_PLUGIN_SYSTEM_PATH_1_0", &runtime.plugin_system_path);
            std::env::set_var("GST_PLUGIN_SCANNER_1_0", &runtime.plugin_scanner);
            std::env::set_var("GST_PLUGIN_SCANNER", &runtime.plugin_scanner);
            if runtime.gio_modules.is_dir() {
                std::env::set_var("GIO_EXTRA_MODULES", &runtime.gio_modules);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn configure_before_threads() {}

#[cfg(test)]
#[path = "macos_bundle_runtime_test.rs"]
mod tests;

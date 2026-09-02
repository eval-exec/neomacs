//! Packaged runtime-resource installation for native sandboxed hosts.

std::cfg_select! {
    target_family = "wasm" => {}
    _ => {
        mod extracted;
        pub use extracted::{
            RuntimeResourceError, RuntimeResourceInstall, RuntimeResourceRoot,
        };
    }
}

/// Content ID stored beside the packaged runtime resource archive.
pub const RUNTIME_RESOURCE_ID_ASSET: &str = "neomacs-runtime.sha256";

/// Deterministic archive containing the installed `share/neomacs` tree.
pub const RUNTIME_RESOURCE_ARCHIVE_ASSET: &str = "neomacs-runtime.tar.gz";

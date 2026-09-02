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

/// Deterministic gzip-compressed tar archive containing the installed
/// `share/neomacs` tree.
///
/// The opaque suffix is part of the cross-host transport contract. Android's
/// asset packager reserves `.gz`, decompresses such inputs, and removes the
/// suffix before runtime asset lookup.
pub const RUNTIME_RESOURCE_ARCHIVE_ASSET: &str = "neomacs-runtime.bundle";
